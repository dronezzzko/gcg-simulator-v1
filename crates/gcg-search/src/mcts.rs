//! The strengthened search, v0: a copy of the reference ISMCTS (`gcg-ai-reference`'s `mcts.rs`,
//! which ports `ai/mcts.py:82-339`) over [`StrengthenedConfig`], so that each Phase 6 mechanism
//! is a configuration field and every ablation row a flag flip (PLAN X12, §12.3). With every
//! mechanism off (the null configuration of `tests/null_config.rs`) it makes the reference's
//! decisions, alternatives and RNG draws bit for bit.
//!
//! One tree is built from the deciding player's point of view. Every iteration picks a root
//! move by UCB1 plus a progressive prior, plays it in the k-th determinization (k = that move's
//! visit count, so the k-th visit of every root move shares world k and rollout seed k: common
//! random numbers), walks the tree among the moves legal in that world, adds one node, rolls
//! out with the playout policy to the horizon and backs up the leaf value.
//!
//! Exactness rules kept here: every seed is drawn up front in the order `W[0:B]`, `R[0:B]`,
//! `W[B:cap]`, `R[B:cap]`; every argmax keeps the first maximum (CPython's `max` replaces only on
//! a strict `>`); float expressions keep Python's evaluation order; `sum()` is the Neumaier
//! `py_sum`; `x ** 2` is libm `pow` (`gcg_core::libm::square`); the per-visit differences are
//! `total_after - total_before`; the backup path keeps duplicates (W-06).

use gcg_core::libm::{exp, ln, sqrt, square};
use gcg_core::pymath::py_sum;
use gcg_core::rng::SplitMix64;
use gcg_engine::observe::determinize;
use gcg_engine::{Action, ActionKind, CtxKey, DecisionKind, DecisionView, EngineError, GameState};

use gcg_ai::{
    AgentCx, AgentError, Alternative, Density, IterTrace, MoveKey, NoopFlags, PolicyKnobs,
    PruneFlags, SPENDING, SearchEffort, SearchResult, SearchStats, SearchTracer, Weights,
    apply_unchecked, burst_density, canonical, logistic, merge_equivalent, option_scores,
    prune_dominated, score, uid_argument, without_effect,
};

use crate::{Observation, StrengthenedConfig};

/// `WIN_DECAY` (`ai/config.py:9`).
const WIN_DECAY: f64 = 0.01;
/// `LEAF_FLOOR` (`ai/config.py:10`).
const LEAF_FLOOR: f64 = 0.05;
/// `TIE_TOLERANCE` (`ai/mcts.py:41`).
const TIE_TOLERANCE: f64 = 1e-9;

/// `_TARGET_KINDS` (`ai/mcts.py:42-44`).
const TARGET_KINDS: [DecisionKind; 4] = [
    DecisionKind::Select,
    DecisionKind::Excess,
    DecisionKind::Discard,
    DecisionKind::Arrange,
];

/// `ChildKey = (dec.player, dec.kind.value if cfg.observation_keys else "", move_key)`
/// (`ai/mcts.py:40`, `:267-268`); `None` stands for `""`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct ChildKey {
    player: u8,
    kind: Option<DecisionKind>,
    key: MoveKey,
}

/// `_Child` (`ai/mcts.py:47-57`); `node` indexes [`Tree::nodes`].
#[derive(Clone, Copy, Debug)]
struct Child {
    mover: u8,
    prior: f64,
    visits: u32,
    total: f64,
    avail: u32,
    node: Option<u32>,
}

impl Child {
    /// `mean()` (`ai/mcts.py:56-57`).
    fn mean(&self) -> f64 {
        if self.visits == 0 {
            0.0
        } else {
            self.total / f64::from(self.visits)
        }
    }
}

/// The tree as two arenas: `nodes[i]` is `_Node.children` (`ai/mcts.py:60-62`) as a list of
/// `(key, child index)` in insertion order, used for lookups only (never iterated for a
/// decision), and `children` holds every `_Child`.
#[derive(Default)]
struct Tree {
    nodes: Vec<Vec<(ChildKey, u32)>>,
    children: Vec<Child>,
}

impl Tree {
    fn new_node(&mut self) -> u32 {
        self.nodes.push(Vec::new());
        u32::try_from(self.nodes.len() - 1).expect("tree nodes fit in u32")
    }

    fn get(&self, node: u32, key: &ChildKey) -> Option<u32> {
        self.nodes[node as usize]
            .iter()
            .find_map(|(k, c)| (k == key).then_some(*c))
    }

    /// `node.children.setdefault(key, _Child(mover, prior))` (`ai/mcts.py:317`): an existing
    /// child keeps its first prior.
    fn setdefault(&mut self, node: u32, key: ChildKey, mover: u8, prior: f64) {
        if self.get(node, &key).is_some() {
            return;
        }
        self.children.push(Child {
            mover,
            prior,
            visits: 0,
            total: 0.0,
            avail: 0,
            node: None,
        });
        let c = u32::try_from(self.children.len() - 1).expect("tree children fit in u32");
        self.nodes[node as usize].push((key, c));
    }

    /// `_node_of(child)` (`ai/mcts.py:248-251`): the child's node, created on first use.
    fn node_of(&mut self, child: u32) -> u32 {
        if let Some(n) = self.children[child as usize].node {
            return n;
        }
        let n = self.new_node();
        self.children[child as usize].node = Some(n);
        n
    }
}

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

/// `_iteration_cap(config, dec, budget)` (`ai/mcts.py:172-178`): the budget, or for a target
/// decision `max(budget, round(budget * target_extension))` (`round` ties to even).
fn iteration_cap(cfg: &StrengthenedConfig, dec: &DecisionView<'_>, budget: u32) -> u32 {
    if !is_target_decision(dec) {
        return budget;
    }
    // An integral float; `as` saturates out-of-range values, which no budget reaches.
    let extended = (f64::from(budget) * cfg.target_extension).round_ties_even() as u32;
    budget.max(extended)
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

/// CPython `max(values)` of floats: the first value, replaced only by a strictly greater one.
fn py_max(values: &[f64]) -> f64 {
    let mut it = values.iter().copied();
    let mut best = it.next().expect("max() of a non-empty sequence");
    for v in it {
        if v > best {
            best = v;
        }
    }
    best
}

/// `_close_call(children, per_visit, z)` (`ai/mcts.py:181-202`): the leader is not `z` paired
/// standard errors better than some other visited move.
fn close_call(children: &[Child], per_visit: &[Vec<f64>], z: f64) -> bool {
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
        if py_max(&abs) <= TIE_TOLERANCE {
            continue;
        }
        let n_f = count_f64(n);
        let mean = py_sum(diffs.iter().copied()) / n_f;
        let var = py_sum(diffs.iter().map(|&x| square(x - mean))) / count_f64(n - 1);
        if mean <= z * sqrt(var / n_f) {
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
struct Search<'c, T: SearchTracer + ?Sized> {
    player: u8,
    cfg: &'c StrengthenedConfig,
    weights: &'c Weights,
    knobs: PolicyKnobs,
    density: Density,
    root_turn: i32,
    horizon: i32,
    tree: Tree,
    effort: SearchEffort,
    tracer: &'c mut T,
    /// The running iteration's index and its apply counters (level 2-3 drill-down).
    n: u32,
    applies_walk: u32,
    applies_rollout: u32,
    path: Vec<u32>,
}

impl<T: SearchTracer + ?Sized> Search<'_, T> {
    /// One engine `apply` of the walk (the root move included), reported to the tracer.
    fn walk_apply(&mut self, s: &mut GameState, a: Action) -> Result<(), EngineError> {
        apply_unchecked(s, a)?;
        let j = self.applies_walk + self.applies_rollout;
        self.applies_walk += 1;
        self.effort.applies_tree += 1;
        self.tracer.apply(self.n, j, a, s);
        Ok(())
    }

    /// `_add_children(node, s, dec, moves, cfg)` (`ai/mcts.py:305-317`): priors over the given
    /// (reduced) options, scored in `s`.
    fn add_children(
        &mut self,
        node: u32,
        s: &GameState,
        dec: &DecisionView<'_>,
        moves: &[(MoveKey, Action)],
    ) -> Result<(), EngineError> {
        let options: Vec<Action> = moves.iter().map(|&(_, a)| a).collect();
        let scores = option_scores(s, dec, &options, &self.knobs)?;
        let top = py_max(&scores);
        let weights: Vec<f64> = scores
            .iter()
            .map(|&x| exp((x - top) / self.cfg.prior_temperature))
            .collect();
        let total = py_sum(weights.iter().copied());
        for (&(key, _), &wgt) in moves.iter().zip(&weights) {
            let ck = child_key(dec, key, self.cfg);
            self.tree.setdefault(node, ck, dec.hdr.player, wgt / total);
        }
        Ok(())
    }

    /// `_select(node, s, dec, moves, cfg)` (`ai/mcts.py:271-302`): UCB1 plus a progressive
    /// prior; unvisited moves first, best prior first. Returns `(child, action, fresh)`.
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
            self.add_children(node, s, dec, moves)?;
        }
        let avail: Vec<(u32, Action)> = keys
            .iter()
            .zip(moves)
            .map(|(k, &(_, a))| (self.tree.get(node, k).expect("added above"), a))
            .collect();
        for &(c, _) in &avail {
            self.tree.children[c as usize].avail += 1;
        }
        let mut best_unvisited: Option<(u32, Action, f64)> = None;
        for &(c, a) in &avail {
            let ch = &self.tree.children[c as usize];
            if ch.visits != 0 {
                continue;
            }
            let better = best_unvisited.is_none_or(|(_, _, p)| ch.prior > p);
            if better {
                best_unvisited = Some((c, a, ch.prior));
            }
        }
        if let Some((c, a, _)) = best_unvisited {
            return Ok((c, a, true));
        }
        let c = self.cfg.exploration;
        let w = self.cfg.prior_weight;
        let ucb = |ch: &Child| {
            let visits = f64::from(ch.visits);
            ch.mean() + c * sqrt(ln(f64::from(ch.avail)) / visits) + w * ch.prior / (visits + 1.0)
        };
        let mut best: Option<(u32, Action, f64)> = None;
        for &(ci, a) in &avail {
            let v = ucb(&self.tree.children[ci as usize]);
            if best.is_none_or(|(_, _, bv)| v > bv) {
                best = Some((ci, a, v));
            }
        }
        let (ci, a, _) = best.expect("a decision with options");
        Ok((ci, a, false))
    }

    /// `_descend(ctx, node, s, path)` (`ai/mcts.py:220-235`): walks the tree in world `s` until
    /// a new child is added or the horizon is reached.
    fn descend(&mut self, mut node: u32, s: &mut GameState) -> Result<(), EngineError> {
        while s.winner().is_none() && s.turn() < self.horizon {
            let Some(dec) = s.pending() else { break };
            let moves = canonical(s, &dec);
            if moves.len() == 1 {
                self.walk_apply(s, moves[0].1)?;
                continue;
            }
            let (child, action, fresh) = self.select(node, s, &dec, &moves)?;
            let (dec_player, dec_kind) = (dec.hdr.player, dec.hdr.kind);
            self.walk_apply(s, action)?;
            self.path.push(child);
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

    /// `play_until_turn(s, ctx.horizon, rng, cfg.rollout_temperature,
    /// cfg.max_rollout_decisions, knobs_for(cfg))` (`ai/mcts.py:211-213`), the loop of
    /// `ai/playout.py:22-26` with every apply reported to the tracer: one `policy.choose` per
    /// move, which draws once per option of a sampled decision and nothing for a single option.
    fn rollout(&mut self, s: &mut GameState, rng: &mut SplitMix64) -> Result<(), EngineError> {
        let max = self.cfg.max_rollout_decisions;
        let mut n = 0;
        while s.winner().is_none() && s.turn() < self.horizon && n < max && s.pending().is_some() {
            let a = gcg_ai::choose(
                s,
                Some(&mut *rng),
                self.cfg.rollout_temperature,
                &self.knobs,
            )?;
            apply_unchecked(s, a)?;
            n += 1;
            let j = self.applies_walk + self.applies_rollout;
            self.applies_rollout += 1;
            self.effort.applies_rollout += 1;
            self.tracer.apply(self.n, j, a, s);
        }
        if n == max && s.winner().is_none() && s.turn() < self.horizon && s.pending().is_some() {
            self.effort.rollouts_capped += 1;
        }
        Ok(())
    }

    /// `_leaf_value(s, ctx)` (`ai/mcts.py:254-264`).
    fn leaf_value(&self, s: &GameState) -> Result<f64, EngineError> {
        if let Some(w) = s.winner() {
            let elapsed = WIN_DECAY * f64::from(s.turn() - self.root_turn);
            if i32::from(w) == i32::from(self.player) {
                return Ok(1.0 - elapsed);
            }
            return Ok(if i32::from(w) == 1 - i32::from(self.player) {
                elapsed
            } else {
                0.5
            });
        }
        let z = score(s, self.player, self.weights, self.density)? / self.cfg.value_scale;
        Ok(LEAF_FLOOR + (1.0 - 2.0 * LEAF_FLOOR) * logistic(z))
    }

    /// `_iterate(ctx, first, fresh, s, rng)` (`ai/mcts.py:205-217`); returns the leaf value and
    /// whether the game ended.
    fn iterate(
        &mut self,
        first: u32,
        fresh: bool,
        s: &mut GameState,
        mut rng: SplitMix64,
    ) -> Result<(f64, bool), EngineError> {
        self.path.clear();
        self.path.push(first);
        if !fresh {
            let node = self.tree.node_of(first);
            self.descend(node, s)?;
        }
        self.rollout(s, &mut rng)?;
        let value = self.leaf_value(s)?;
        let path = std::mem::take(&mut self.path);
        for &c in &path {
            let child = &mut self.tree.children[c as usize];
            child.visits += 1;
            child.total += if child.mover == self.player {
                value
            } else {
                1.0 - value
            };
        }
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

/// v0 of the strengthened search: `search(st, player, config, config.weights, rng)`
/// (`ai/mcts.py:82-151`) over the observation `obs` of `player` at `st`: chooses a move for
/// `player` at `st`'s pending decision using only their information set. v0 reads the state
/// through [`Observation::state`]; `st` is never mutated, and the calls of the root-move
/// functions and `burst_density` on `st` fill its view cache as Python's do. `rng` is the
/// per-decision generator; the search reads `cx.cancel` before every iteration.
///
/// # Errors
/// [`AgentError::Assertion`] without a decision for `player` (`ai/mcts.py:87-88`),
/// [`AgentError::Interrupted`] on SIGINT, and the engine's errors.
pub fn search<T: SearchTracer + ?Sized>(
    obs: &Observation<'_>,
    cfg: &StrengthenedConfig,
    rng: &mut SplitMix64,
    cx: &mut AgentCx<'_>,
    tracer: &mut T,
) -> Result<SearchResult, AgentError> {
    let st = obs.state();
    let player = obs.player();
    let dec = st
        .pending()
        .ok_or_else(|| AgentError::Assertion(String::new()))?;
    if dec.hdr.player != player {
        return Err(AgentError::Assertion(String::new()));
    }
    let knobs = cfg.knobs();
    let mut root_moves = prune_dominated(
        st,
        &dec,
        canonical(st, &dec),
        PruneFlags {
            strict_attacks: cfg.strict_attack_prune,
            rest_targets: cfg.prune_noop_rest_targets,
        },
    )?;
    if root_moves.iter().any(|(_, a)| SPENDING.contains(&a.kind)) {
        let seed = rng.next_u64();
        let idle = without_effect(
            st,
            player,
            &dec,
            &root_moves,
            seed,
            &knobs,
            NoopFlags {
                branch_choices: cfg.noop_branch_choices,
                resource_aware: cfg.noop_resource_aware,
            },
            &cfg.weights,
        )?;
        let kept: Vec<(MoveKey, Action)> = root_moves
            .iter()
            .copied()
            .filter(|(k, _)| !idle.contains(k))
            .collect();
        if !kept.is_empty() {
            root_moves = kept;
        }
    }
    if cfg.merge_equivalent_moves {
        root_moves = merge_equivalent(st, &dec, root_moves)?;
    }
    let roots: Vec<MoveKey> = root_moves.iter().map(|&(k, _)| k).collect();
    let mut effort = SearchEffort::default();
    if root_moves.len() == 1 {
        return Ok(SearchResult {
            action: root_moves[0].1,
            alternatives: Vec::new(),
            stats: SearchStats {
                iterations: 0,
                budget: 0,
                cap: 0,
                roots,
                effort,
            },
        });
    }
    let density = burst_density(st, player);
    let budget = cfg.budget(root_moves.len());
    let mut world_seeds: Vec<u64> = (0..budget).map(|_| rng.next_u64()).collect();
    let mut rollout_seeds: Vec<u64> = (0..budget).map(|_| rng.next_u64()).collect();
    let cap = iteration_cap(cfg, &dec, budget);
    world_seeds.extend((budget..cap).map(|_| rng.next_u64()));
    rollout_seeds.extend((budget..cap).map(|_| rng.next_u64()));

    let mut worlds: Vec<GameState> = vec![determinize(st, player, world_seeds[0])?];
    effort.worlds += 1;
    let mut search = Search {
        player,
        cfg,
        weights: &cfg.weights,
        knobs,
        density,
        root_turn: st.turn(),
        horizon: st.turn() + cfg.horizon_turns,
        tree: Tree::default(),
        effort,
        tracer,
        n: 0,
        applies_walk: 0,
        applies_rollout: 0,
        path: Vec::new(),
    };
    let root = search.tree.new_node();
    search.add_children(root, &worlds[0], &dec, &root_moves)?;
    let keys: Vec<ChildKey> = root_moves
        .iter()
        .map(|&(key, _)| child_key(&dec, key, cfg))
        .collect();
    let children: Vec<u32> = keys
        .iter()
        .map(|k| search.tree.get(root, k).expect("root children were added"))
        .collect();
    let mut per_visit: Vec<Vec<f64>> = vec![Vec::new(); children.len()];
    let mut limit = budget;
    let mut done: u32 = 0;
    while done < limit {
        if cx.cancel.is_cancelled() {
            return Err(AgentError::Interrupted);
        }
        let (child, action, fresh) = search.select(root, &worlds[0], &dec, &root_moves)?;
        let k = search.tree.children[child as usize].visits;
        let ku = k as usize;
        if ku == worlds.len() {
            worlds.push(determinize(st, player, world_seeds[ku])?);
            search.effort.worlds += 1;
        }
        let mut world = cx.scratch.clone_state(&worlds[ku]);
        search.n = done;
        search.applies_walk = 0;
        search.applies_rollout = 0;
        search.walk_apply(&mut world, action)?;
        let before = search.tree.children[child as usize].total;
        let (leaf, terminal) =
            search.iterate(child, fresh, &mut world, SplitMix64::new(rollout_seeds[ku]))?;
        cx.scratch.recycle_state(world);
        let after = search.tree.children[child as usize].total;
        let slot = children
            .iter()
            .position(|&c| c == child)
            .expect("a root child");
        if cap > budget {
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
            if limit == cap || !close_call(&root_children, &per_visit, cfg.target_extension_z) {
                break;
            }
            limit = cap.min(limit + budget);
        }
    }
    let (action, alternatives) = search.result(root, &keys, &root_moves);
    Ok(SearchResult {
        action,
        alternatives,
        stats: SearchStats {
            iterations: done,
            budget,
            cap,
            roots,
            effort: search.effort,
        },
    })
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
        assert_eq!(py_max(&[1.0, 3.0, 3.0, 2.0]), 3.0);
        assert!(py_max(&[f64::NAN, 1.0]).is_nan());
    }

    #[test]
    fn close_calls_follow_the_paired_test() {
        // Fewer than two shared visits with a visited other move: a close call.
        let kids = [child(2, 1.0), child(1, 0.4)];
        assert!(close_call(&kids, &[vec![0.5, 0.5], vec![0.4]], 2.0));
        // Identical shared visits are a tie that does not count.
        let kids = [child(3, 1.5), child(3, 1.5)];
        assert!(!close_call(&kids, &[vec![0.5; 3], vec![0.5; 3]], 2.0));
        // A clear lead: mean 0.4 against z * se = 0.
        let kids = [child(3, 2.7), child(3, 1.5)];
        assert!(!close_call(&kids, &[vec![0.9; 3], vec![0.5; 3]], 2.0));
        // Unvisited moves are ignored.
        let kids = [child(3, 2.7), child(0, 0.0)];
        assert!(!close_call(&kids, &[vec![0.9; 3], vec![]], 2.0));
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
