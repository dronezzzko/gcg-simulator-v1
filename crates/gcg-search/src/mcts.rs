//! The core loop of the strengthened search (docs/port/PLAN.md §12.3): the reference ISMCTS
//! (`gcg-ai-reference`'s `mcts.rs`, which ports `ai/mcts.py:82-339`) with every point a Phase 6
//! mechanism changes behind the [`Mechanisms`] trait (`mech/`). With every flag off (the null
//! configuration of `tests/null_config.rs`) it makes the reference's decisions, alternatives and
//! RNG draws bit for bit.
//!
//! One tree is built from the deciding player's point of view, on worlds determinised from the
//! observer's information-set state (W-135). Every iteration picks a root move, plays it in the
//! k-th world (k = that move's visit count, so the k-th visit of every root move shares world k
//! and rollout seed k: common random numbers), walks the tree among the moves legal in that
//! world, adds one node, rolls out to the horizon and backs up the leaf value.
//!
//! Exactness rules kept here: every seed is drawn up front in the order `W[0:B]`, `R[0:B]`,
//! `W[B:cap]`, `R[B:cap]`; every argmax keeps the first maximum (CPython's `max` replaces only on
//! a strict `>`); float expressions keep Python's evaluation order; `sum()` is the Neumaier
//! `py_sum`; the per-visit differences are `total_after - total_before`; the backup path keeps
//! duplicates (W-06).

use gcg_core::pymath::py_sum;
use gcg_core::rng::SplitMix64;
use gcg_engine::{Action, ActionKind, CtxKey, DecisionKind, DecisionView, EngineError, GameState};

use gcg_ai::{
    AgentCx, AgentError, Alternative, Density, IterTrace, MoveKey, PolicyKnobs, SearchEffort,
    SearchResult, SearchStats, SearchTracer, apply_unchecked, burst_density, canonical,
    uid_argument,
};

use crate::mech::p6o::math::Math;
use crate::mech::{
    BackupCx, ChildIdx, Composed, HorizonDecision, LeafCx, Mechanisms, NodeCx, PregameCx,
    RolloutCx, RootCx, WorldCx, math_of, validate,
};
use crate::tree::{Child, ChildKey, Tree};
use crate::{MechanismEffort, Observation, StrengthenedConfig};

/// `TIE_TOLERANCE` (`ai/mcts.py:41`).
const TIE_TOLERANCE: f64 = 1e-9;

/// `_TARGET_KINDS` (`ai/mcts.py:42-44`).
const TARGET_KINDS: [DecisionKind; 4] = [
    DecisionKind::Select,
    DecisionKind::Excess,
    DecisionKind::Discard,
    DecisionKind::Arrange,
];

/// `_child_key(dec, key, cfg)` (`ai/mcts.py:267-268`).
fn child_key(dec: &DecisionView<'_>, key: MoveKey, cfg: &StrengthenedConfig) -> ChildKey {
    ChildKey {
        player: dec.hdr.player,
        kind: cfg.observation_keys.then_some(dec.hdr.kind),
        key,
    }
}

/// `is_target_decision(dec)` (`ai/mcts.py:165-169`): the decision chooses the cards an effect
/// acts on.
#[must_use]
pub fn is_target_decision(dec: &DecisionView<'_>) -> bool {
    TARGET_KINDS.contains(&dec.hdr.kind) && uid_argument(dec) && dec.hdr.ctx(CtxKey::RestSub) < 0
}

/// CPython `max(range(n), key=lambda i: (visits[i], mean[i]))`: the first index whose
/// `(visits, mean)` tuple is greatest (`ai/mcts.py:187`, `:338`).
fn first_max_visits_mean(items: impl IntoIterator<Item = (u32, f64)>) -> usize {
    let mut best = 0;
    let mut best_key: Option<(u32, f64)> = None;
    for (i, (v, m)) in items.into_iter().enumerate() {
        let greater = match best_key {
            None => true,
            Some((bv, bm)) => v > bv || (v == bv && m > bm),
        };
        if greater {
            best = i;
            best_key = Some((v, m));
        }
    }
    best
}

/// `_close_call(children, per_visit, z)` (`ai/mcts.py:181-202`): the leader is not `z` paired
/// standard errors better than some other visited move.
fn close_call(children: &[Child], per_visit: &[Vec<f64>], z: f64, math: Math) -> bool {
    let leader = first_max_visits_mean(children.iter().map(|c| (c.visits, c.mean())));
    let lead = &per_visit[leader];
    for (i, other) in per_visit.iter().enumerate() {
        if i == leader || other.is_empty() {
            continue;
        }
        let n = lead.len().min(other.len());
        if n < 2 {
            return true;
        }
        let diffs: Vec<f64> = (0..n).map(|j| lead[j] - other[j]).collect();
        let abs: Vec<f64> = diffs.iter().map(|x| x.abs()).collect();
        if crate::mech::reference::py_max(&abs) <= TIE_TOLERANCE {
            continue;
        }
        let n_f = count_f64(n);
        let mean = py_sum(diffs.iter().copied()) / n_f;
        let var = py_sum(diffs.iter().map(|&x| math.square(x - mean))) / count_f64(n - 1);
        if mean <= z * math.sqrt(var / n_f) {
            return true;
        }
    }
    false
}

/// A Python `int` count converted to a float (exact for every count in scope).
fn count_f64(n: usize) -> f64 {
    f64::from(u32::try_from(n).expect("counts in scope fit in u32"))
}

/// `_unobserved_pass(dec, action, player)` (`ai/mcts.py:238-245`).
fn unobserved_pass(dec_player: u8, dec_kind: DecisionKind, action: Action, player: u8) -> bool {
    dec_player != player && dec_kind == DecisionKind::ActionStep && action.kind == ActionKind::Pass
}

/// The search context `_Search` (`ai/mcts.py:65-72`) together with the tree, the effort
/// counters and the drill-down state of the running iteration.
struct Search<'c, M: Mechanisms, T: SearchTracer + ?Sized> {
    mech: &'c M,
    player: u8,
    cfg: &'c StrengthenedConfig,
    knobs: PolicyKnobs,
    math: Math,
    density: Density,
    root_turn: i32,
    /// `root_turn + horizon_turns` (`ai/mcts.py:119`).
    horizon: i32,
    /// The running line's horizon: `horizon` plus the extensions of `Mechanisms::horizon`.
    line_horizon: i32,
    tree: Tree,
    effort: SearchEffort,
    mech_effort: MechanismEffort,
    tracer: &'c mut T,
    /// The running iteration's index and its apply counters (level 2-3 drill-down).
    n: u32,
    applies_walk: u32,
    applies_rollout: u32,
    path: Vec<u32>,
}

impl<M: Mechanisms, T: SearchTracer + ?Sized> Search<'_, M, T> {
    /// One engine `apply` of the walk (the root move included), reported to the tracer.
    fn walk_apply(&mut self, s: &mut GameState, a: Action) -> Result<(), EngineError> {
        apply_unchecked(s, a)?;
        let j = self.applies_walk + self.applies_rollout;
        self.applies_walk += 1;
        self.effort.applies_tree += 1;
        self.tracer.apply(self.n, j, a, s);
        Ok(())
    }

    /// `_select(node, s, dec, moves, cfg)` (`ai/mcts.py:271-302`): the priors when a move has
    /// no child yet, one `avail` count for every move, then the mechanism's choice. Returns
    /// `(child, action, fresh)`; a child without visits is fresh.
    fn select(
        &mut self,
        node: u32,
        s: &GameState,
        dec: &DecisionView<'_>,
        moves: &[(MoveKey, Action)],
    ) -> Result<(u32, Action, bool), EngineError> {
        let keys: Vec<ChildKey> = moves
            .iter()
            .map(|&(key, _)| child_key(dec, key, self.cfg))
            .collect();
        if keys.iter().any(|k| self.tree.get(node, k).is_none()) {
            self.mech.priors(&mut NodeCx {
                tree: &mut self.tree,
                node,
                s,
                dec,
                moves,
                keys: &keys,
                children: &[],
                cfg: self.cfg,
                knobs: self.knobs,
                math: self.math,
            })?;
        }
        let children: Vec<u32> = keys
            .iter()
            .map(|k| self.tree.get(node, k).expect("the priors add every child"))
            .collect();
        for &c in &children {
            self.tree.children[c as usize].avail += 1;
        }
        let ChildIdx(i) = self.mech.select(&mut NodeCx {
            tree: &mut self.tree,
            node,
            s,
            dec,
            moves,
            keys: &keys,
            children: &children,
            cfg: self.cfg,
            knobs: self.knobs,
            math: self.math,
        });
        let c = children[i];
        Ok((c, moves[i].1, self.tree.children[c as usize].visits == 0))
    }

    /// `_descend(ctx, node, s, path)` (`ai/mcts.py:220-235`): walks the tree in world `s` until
    /// a new child is added or the horizon is reached. The child joins the path before its move
    /// is applied (Python appends after; nothing reads the path in between), so a move whose
    /// apply fails is charged with the error leaf.
    fn descend(&mut self, mut node: u32, s: &mut GameState) -> Result<(), EngineError> {
        while s.winner().is_none() && s.turn() < self.line_horizon {
            let Some(dec) = s.pending() else { break };
            let moves = canonical(s, &dec);
            if moves.len() == 1 {
                self.walk_apply(s, moves[0].1)?;
                continue;
            }
            let (child, action, fresh) = self.select(node, s, &dec, &moves)?;
            let (dec_player, dec_kind) = (dec.hdr.player, dec.hdr.kind);
            self.path.push(child);
            self.walk_apply(s, action)?;
            if fresh {
                return Ok(());
            }
            if self.cfg.observation_keys
                && unobserved_pass(dec_player, dec_kind, action, self.player)
            {
                continue;
            }
            node = self.tree.node_of(child);
        }
        Ok(())
    }

    fn leaf_cx<'s>(&'s self, s: &'s GameState, decisions: u32, capped: bool) -> LeafCx<'s> {
        LeafCx {
            s,
            player: self.player,
            root_turn: self.root_turn,
            horizon: self.line_horizon,
            decisions,
            capped,
            cfg: self.cfg,
            weights: &self.cfg.weights,
            density: self.density,
            math: self.math,
        }
    }

    /// `play_until_turn(s, ctx.horizon, rng, cfg.rollout_temperature,
    /// cfg.max_rollout_decisions, knobs_for(cfg))` (`ai/mcts.py:211-213`), the loop of
    /// `ai/playout.py:22-26` with every apply reported to the tracer and one
    /// `Mechanisms::rollout_choice` per move, then `Mechanisms::horizon` whenever it stops.
    /// Returns the rollout decisions taken and whether the last stop was at the cap. After the
    /// first stop, an extension is followed only while the continued rollout applies something.
    fn rollout(
        &mut self,
        s: &mut GameState,
        rng: &mut SplitMix64,
    ) -> Result<(u32, bool), EngineError> {
        let mut max = self.cfg.max_rollout_decisions;
        let mut n: u32 = 0;
        let mut first_stop = true;
        loop {
            let start = n;
            while s.winner().is_none()
                && s.turn() < self.line_horizon
                && n < max
                && s.pending().is_some()
            {
                let a = self.mech.rollout_choice(&mut RolloutCx {
                    s: &*s,
                    rng: &mut *rng,
                    player: self.player,
                    cfg: self.cfg,
                    knobs: self.knobs,
                    math: self.math,
                })?;
                apply_unchecked(s, a)?;
                n += 1;
                let j = self.applies_walk + self.applies_rollout;
                self.applies_rollout += 1;
                self.effort.applies_rollout += 1;
                self.tracer.apply(self.n, j, a, s);
            }
            let capped = n == max
                && s.winner().is_none()
                && s.turn() < self.line_horizon
                && s.pending().is_some();
            let progressed = first_stop || n > start;
            first_stop = false;
            match self.mech.horizon(&self.leaf_cx(s, n, capped)) {
                HorizonDecision::Extend { turns, decisions } if progressed => {
                    self.line_horizon = self.line_horizon.saturating_add(turns);
                    max = max.saturating_add(decisions);
                }
                _ => return Ok((n, capped)),
            }
        }
    }

    /// Root move `action` in world `s`, the walk unless the root child is `fresh`, the
    /// rollout and the leaf value (`ai/mcts.py:205-213`, `:254-264`).
    fn line(
        &mut self,
        action: Action,
        fresh: bool,
        first: u32,
        s: &mut GameState,
        rng: &mut SplitMix64,
    ) -> Result<f64, EngineError> {
        self.walk_apply(s, action)?;
        if !fresh {
            let node = self.tree.node_of(first);
            self.descend(node, s)?;
        }
        let (decisions, capped) = self.rollout(s, rng)?;
        if capped {
            self.effort.rollouts_capped += 1;
        }
        let mut leaf = self.leaf_cx(s, decisions, capped);
        self.mech.leaf_value(&mut leaf)
    }

    /// `_iterate(ctx, first, fresh, s, rng)` (`ai/mcts.py:205-217`) with the root apply of
    /// `ai/mcts.py:137`; returns the leaf value and whether the game ended. A line that fails
    /// with an engine error is scored by `Mechanisms::error_leaf`, or fails the search.
    fn iterate(
        &mut self,
        first: u32,
        action: Action,
        fresh: bool,
        s: &mut GameState,
        mut rng: SplitMix64,
    ) -> Result<(f64, bool), EngineError> {
        self.path.clear();
        self.path.push(first);
        self.line_horizon = self.horizon;
        let value = match self.line(action, fresh, first, s, &mut rng) {
            Ok(v) => v,
            Err(e) => {
                let v = self
                    .mech
                    .error_leaf(&self.leaf_cx(s, 0, false), &e)
                    .ok_or(e)?;
                self.mech_effort.engine_error_leaves += 1;
                v
            }
        };
        let path = std::mem::take(&mut self.path);
        self.mech.backup(&mut BackupCx {
            tree: &mut self.tree,
            path: &path,
            value,
            player: self.player,
        });
        self.path = path;
        Ok((value, s.winner().is_some()))
    }

    /// `_settled(root, keys, remaining)` (`ai/mcts.py:320-328`).
    fn settled(&self, root: u32, keys: &[ChildKey], remaining: u32) -> bool {
        let mut visits: Vec<u32> = keys
            .iter()
            .filter_map(|k| self.tree.get(root, k))
            .map(|c| self.tree.children[c as usize].visits)
            .collect();
        if visits.len() < keys.len() {
            return false;
        }
        visits.sort_by(|a, b| b.cmp(a));
        i64::from(visits[0]) - i64::from(visits[1]) > i64::from(remaining)
    }

    /// `_result(root, keys, moves, iterations)` (`ai/mcts.py:331-339`): the first move with
    /// the most visits, then the higher mean.
    fn result(
        &self,
        root: u32,
        keys: &[ChildKey],
        moves: &[(MoveKey, Action)],
    ) -> (Action, Vec<Alternative>) {
        let stats: Vec<Alternative> = keys
            .iter()
            .zip(moves)
            .map(|(k, &(_, action))| match self.tree.get(root, k) {
                Some(c) => {
                    let ch = &self.tree.children[c as usize];
                    Alternative {
                        action,
                        value: ch.mean(),
                        visits: ch.visits,
                    }
                }
                None => Alternative {
                    action,
                    value: 0.0,
                    visits: 0,
                },
            })
            .collect();
        let best = first_max_visits_mean(stats.iter().map(|a| (a.visits, a.value)));
        (stats[best].action, stats)
    }
}

/// The strengthened search (`search(st, player, config, config.weights, rng)`,
/// `ai/mcts.py:82-151`, with the mechanisms of `cfg`) over the observation `obs` of its player:
/// chooses a move for the observer's pending decision from the observer's information set
/// alone. `rng` is the per-decision generator; the search reads `cx.cancel` before every
/// iteration.
///
/// # Errors
/// [`AgentError::Value`] for a configuration that enables a mechanism this build lacks,
/// [`AgentError::Assertion`] without a decision for the observer (`ai/mcts.py:87-88`),
/// [`AgentError::Interrupted`] on SIGINT, and the engine's errors (inside simulated lines only
/// when `engine_error_leaf` is off).
pub fn search<T: SearchTracer + ?Sized>(
    obs: &Observation<'_>,
    cfg: &StrengthenedConfig,
    rng: &mut SplitMix64,
    cx: &mut AgentCx<'_>,
    tracer: &mut T,
) -> Result<SearchResult, AgentError> {
    search_with_effort(obs, cfg, rng, cx, tracer).map(|(result, _)| result)
}

/// [`search`] with the counters of the strengthened mechanisms.
///
/// # Errors
/// As [`search`].
pub fn search_with_effort<T: SearchTracer + ?Sized>(
    obs: &Observation<'_>,
    cfg: &StrengthenedConfig,
    rng: &mut SplitMix64,
    cx: &mut AgentCx<'_>,
    tracer: &mut T,
) -> Result<(SearchResult, MechanismEffort), AgentError> {
    validate(&cfg.mechanisms).map_err(AgentError::Value)?;
    run(
        &Composed::new(cfg),
        obs.info(),
        obs.player(),
        cfg,
        rng,
        cx,
        tracer,
    )
}

/// The strengthened agent's REDRAW and CHOOSE_FIRST choice (`ai/agents.py:90-95`) at the
/// observer's pending decision, with `Mechanisms::pregame` of `cfg`: the action and its
/// alternatives.
///
/// # Errors
/// [`AgentError::Value`] for a configuration that enables a mechanism this build lacks,
/// [`AgentError::Assertion`] without a REDRAW or CHOOSE_FIRST decision for the observer,
/// [`AgentError::Interrupted`] on SIGINT, and the errors of the play/draw simulations.
pub fn pregame(
    obs: &Observation<'_>,
    cfg: &StrengthenedConfig,
    rng: &mut SplitMix64,
    cx: &mut AgentCx<'_>,
) -> Result<(Action, Vec<Alternative>), AgentError> {
    validate(&cfg.mechanisms).map_err(AgentError::Value)?;
    let st = obs.info();
    let dec = st
        .pending()
        .filter(|d| {
            d.hdr.player == obs.player()
                && matches!(d.hdr.kind, DecisionKind::Redraw | DecisionKind::ChooseFirst)
        })
        .ok_or_else(|| AgentError::Assertion(String::new()))?;
    Composed::new(cfg).pregame(&mut PregameCx {
        st,
        dec: &dec,
        player: obs.player(),
        cfg,
        rng,
        agent: cx,
    })
}

/// The core loop over the information-set state `st` of `player` with the mechanisms `mech`.
pub(crate) fn run<M: Mechanisms, T: SearchTracer + ?Sized>(
    mech: &M,
    st: &GameState,
    player: u8,
    cfg: &StrengthenedConfig,
    rng: &mut SplitMix64,
    cx: &mut AgentCx<'_>,
    tracer: &mut T,
) -> Result<(SearchResult, MechanismEffort), AgentError> {
    let dec = st
        .pending()
        .ok_or_else(|| AgentError::Assertion(String::new()))?;
    if dec.hdr.player != player {
        return Err(AgentError::Assertion(String::new()));
    }
    let knobs = cfg.knobs();
    let mut root_cx = RootCx {
        st,
        dec: &dec,
        player,
        cfg,
        knobs,
        rng,
    };
    let root_moves = mech.root_moves(&mut root_cx)?;
    let roots: Vec<MoveKey> = root_moves.iter().map(|&(k, _)| k).collect();
    let mut effort = SearchEffort::default();
    if root_moves.len() == 1 {
        let stats = SearchStats {
            iterations: 0,
            budget: 0,
            cap: 0,
            roots,
            effort,
        };
        return Ok((
            SearchResult {
                action: root_moves[0].1,
                alternatives: Vec::new(),
                stats,
            },
            MechanismEffort::default(),
        ));
    }
    let density = burst_density(st, player);
    let budget = mech.budget(&root_cx, root_moves.len());
    let rng = root_cx.rng;
    let (b, cap) = (budget.iterations, budget.cap);
    let mut world_seeds: Vec<u64> = (0..b).map(|_| rng.next_u64()).collect();
    let mut rollout_seeds: Vec<u64> = (0..b).map(|_| rng.next_u64()).collect();
    world_seeds.extend((b..cap).map(|_| rng.next_u64()));
    rollout_seeds.extend((b..cap).map(|_| rng.next_u64()));

    let mut world_cx = WorldCx {
        st,
        player,
        cfg,
        seeds: &world_seeds,
    };
    let mut first_world = st.clone();
    mech.world(&mut world_cx, 0, &mut first_world)?;
    let mut worlds: Vec<GameState> = vec![first_world];
    effort.worlds += 1;
    let horizon = st.turn() + cfg.horizon_turns;
    let mut search = Search {
        mech,
        player,
        cfg,
        knobs,
        math: math_of(cfg),
        density,
        root_turn: st.turn(),
        horizon,
        line_horizon: horizon,
        tree: Tree::default(),
        effort,
        mech_effort: MechanismEffort::default(),
        tracer,
        n: 0,
        applies_walk: 0,
        applies_rollout: 0,
        path: Vec::new(),
    };
    let root = search.tree.new_node();
    let keys: Vec<ChildKey> = root_moves
        .iter()
        .map(|&(key, _)| child_key(&dec, key, cfg))
        .collect();
    mech.priors(&mut NodeCx {
        tree: &mut search.tree,
        node: root,
        s: &worlds[0],
        dec: &dec,
        moves: &root_moves,
        keys: &keys,
        children: &[],
        cfg,
        knobs,
        math: search.math,
    })?;
    let children: Vec<u32> = keys
        .iter()
        .map(|k| search.tree.get(root, k).expect("root children were added"))
        .collect();
    let mut per_visit: Vec<Vec<f64>> = vec![Vec::new(); children.len()];
    let mut limit = b;
    let mut done: u32 = 0;
    while done < limit {
        if cx.cancel.is_cancelled() {
            return Err(AgentError::Interrupted);
        }
        let (child, action, fresh) = search.select(root, &worlds[0], &dec, &root_moves)?;
        let k = search.tree.children[child as usize].visits;
        let ku = k as usize;
        if ku == worlds.len() {
            let mut world = st.clone();
            mech.world(&mut world_cx, ku, &mut world)?;
            worlds.push(world);
            search.effort.worlds += 1;
        }
        let mut world = cx.scratch.clone_state(&worlds[ku]);
        search.n = done;
        search.applies_walk = 0;
        search.applies_rollout = 0;
        let before = search.tree.children[child as usize].total;
        let (leaf, terminal) = search.iterate(
            child,
            action,
            fresh,
            &mut world,
            SplitMix64::new(rollout_seeds[ku]),
        )?;
        cx.scratch.recycle_state(world);
        let after = search.tree.children[child as usize].total;
        let slot = children
            .iter()
            .position(|&c| c == child)
            .expect("a root child");
        if cap > b {
            per_visit[slot].push(after - before);
        }
        let it = IterTrace {
            n: done,
            child: u32::try_from(slot).expect("root moves fit in u32"),
            visit: k,
            fresh,
            path_len: u32::try_from(search.path.len()).expect("paths fit in u32"),
            applies_walk: search.applies_walk,
            applies_rollout: search.applies_rollout,
            leaf,
            terminal,
            total_before: before,
            total_after: after,
        };
        search.tracer.iteration(&worlds[ku], &it);
        done += 1;
        if done == limit || search.settled(root, &keys, limit - done) {
            let root_children: Vec<Child> = children
                .iter()
                .map(|&c| search.tree.children[c as usize])
                .collect();
            if limit == cap
                || !close_call(
                    &root_children,
                    &per_visit,
                    cfg.target_extension_z,
                    search.math,
                )
            {
                break;
            }
            limit = cap.min(limit + b);
        }
    }
    let (action, alternatives) = search.result(root, &keys, &root_moves);
    Ok((
        SearchResult {
            action,
            alternatives,
            stats: SearchStats {
                iterations: done,
                budget: b,
                cap,
                roots,
                effort: search.effort,
            },
        },
        search.mech_effort,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn child(visits: u32, total: f64) -> Child {
        Child {
            mover: 0,
            prior: 0.5,
            visits,
            total,
            avail: visits,
            node: None,
        }
    }

    #[test]
    fn argmaxes_keep_the_first_maximum() {
        assert_eq!(
            first_max_visits_mean([(3, 0.5), (4, 0.1), (4, 0.1), (4, 0.2)]),
            3
        );
        assert_eq!(first_max_visits_mean([(4, 0.2), (4, 0.2)]), 0);
        assert_eq!(first_max_visits_mean([(4, -0.0), (4, 0.0)]), 0);
        let py_max = crate::mech::reference::py_max;
        assert_eq!(py_max(&[1.0, 3.0, 3.0, 2.0]), 3.0);
        assert!(py_max(&[f64::NAN, 1.0]).is_nan());
    }

    #[test]
    fn close_calls_follow_the_paired_test() {
        for math in [Math::Platform, Math::Portable] {
            // Fewer than two shared visits with a visited other move: a close call.
            let kids = [child(2, 1.0), child(1, 0.4)];
            assert!(close_call(&kids, &[vec![0.5, 0.5], vec![0.4]], 2.0, math));
            // Identical shared visits are a tie that does not count.
            let kids = [child(3, 1.5), child(3, 1.5)];
            assert!(!close_call(&kids, &[vec![0.5; 3], vec![0.5; 3]], 2.0, math));
            // A clear lead: mean 0.4 against z * se = 0.
            let kids = [child(3, 2.7), child(3, 1.5)];
            assert!(!close_call(&kids, &[vec![0.9; 3], vec![0.5; 3]], 2.0, math));
            // Unvisited moves are ignored.
            let kids = [child(3, 2.7), child(0, 0.0)];
            assert!(!close_call(&kids, &[vec![0.9; 3], vec![]], 2.0, math));
        }
    }

    #[test]
    fn caps_extend_only_target_decisions() {
        assert_eq!(2.5f64.round_ties_even(), 2.0);
        let cfg = crate::preset_config(crate::Preset::Fast).clone();
        assert_eq!(
            (f64::from(160u32) * cfg.target_extension).round_ties_even(),
            480.0
        );
    }
}
