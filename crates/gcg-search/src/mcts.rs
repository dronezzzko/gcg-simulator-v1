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
//!
//! The P6-D mechanisms of the core (`mech/p6d/`): `root_trees` runs K such trees as one
//! fork-join per phase and merges their root statistics (S1); `in_tree_filters` and
//! `macro_multi_pick` change the moves of a tree node (W-11, W-138); `transpositions` keys the
//! walk's nodes by the searcher's observation (S7); `prior_worlds` refreshes a node's priors in
//! its first K worlds (W-140, through `Mechanisms::priors`).
//!
//! `Mechanisms::rollouts` (GAP-A1's `target_multi_rollouts`, W-14) sets how many rollouts an
//! iteration plays from the end of its walk: the reference plays one; with more, rollout `j > 0`
//! starts from a copy of the walk's end with the generator
//! `derive_seed(rollout_seed, "multi-rollout", j)` (so the k-th visit of every root move shares
//! them too) and the line's value is the `py_sum` mean of the rollouts' leaf values.

use std::sync::Arc;

use gcg_core::pool::Cancel;
use gcg_core::pymath::py_sum;
use gcg_core::rng::{SeedPart, SplitMix64, derive_seed};
use gcg_engine::{Action, ActionKind, CtxKey, DecisionKind, DecisionView, EngineError, GameState};

use gcg_ai::{
    AgentCx, AgentError, Alternative, Density, IterTrace, MoveKey, NoTrace, PolicyKnobs, Scratch,
    SearchEffort, SearchResult, SearchStats, SearchTracer, apply_unchecked, canonical,
    uid_argument,
};

use crate::mech::p6d::reuse::{self, Carry, Kept};
use crate::mech::p6d::{ensemble, filters, macros, priors as p6d_priors, transpose};
use crate::mech::p6o::math::Math;
use crate::mech::{
    BackupCx, Budget, ChildIdx, Composed, ComposedState, HorizonDecision, LeafCx, Mechanisms,
    NodeCx, PregameCx, RolloutCx, RootCx, WorldCx, math_of, validate,
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

/// The auxiliary engine applies a decision may spend per iteration of its cap
/// (`macro_multi_pick`'s trial picks); past it, a multi-card choice keeps its single picks.
/// Results 1.2.0 report it as `ai.search.budget.aux_apply_cap` (PLAN §12.7).
pub const AUX_APPLIES_PER_ITERATION: u64 = 16;

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

/// The moves of a tree node or of the root: one representative per move, or with
/// `macro_multi_pick` one move per subset of a multi-card choice, whose action is the subset's
/// first pick and whose engine actions are in `seqs`.
#[derive(Clone, Debug)]
pub(crate) struct Moves {
    pub(crate) list: Vec<(MoveKey, Action)>,
    /// Per move, the engine actions that play it; empty unless the moves are subsets.
    pub(crate) seqs: Vec<Box<[Action]>>,
}

impl Moves {
    const fn single(list: Vec<(MoveKey, Action)>) -> Self {
        Self {
            list,
            seqs: Vec::new(),
        }
    }

    fn subsets(subsets: Vec<macros::Macro>) -> Self {
        let list = subsets.iter().map(|m| (m.key, m.seq[0])).collect();
        Self {
            list,
            seqs: subsets.into_iter().map(|m| m.seq).collect(),
        }
    }

    fn len(&self) -> usize {
        self.list.len()
    }
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
    /// The root node of the tree.
    root: u32,
    /// The running iteration's world index (`prior_worlds`).
    world: usize,
    /// The nodes the running walk entered (`transpositions`: a repeated node ends the walk).
    walked: Vec<u32>,
    /// The children whose `avail` count the running iteration raised (`visited_set_backup`:
    /// once per iteration, as their visits).
    counted: Vec<u32>,
    /// The auxiliary applies this tree may spend (`macro_multi_pick`).
    aux_cap: u64,
    /// The rollouts of every iteration (`Mechanisms::rollouts`; one in the reference).
    rollouts: u32,
    /// The running iteration's rollout seed (`R[k]`), from which the extra rollouts of
    /// `rollouts > 1` derive theirs.
    rollout_seed: u64,
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

    /// Plays move `i` of `moves`: its action, or every engine action of its subset.
    fn apply_move(
        &mut self,
        s: &mut GameState,
        moves: &Moves,
        i: usize,
    ) -> Result<(), EngineError> {
        match moves.seqs.get(i) {
            Some(seq) => {
                for &a in seq {
                    self.walk_apply(s, a)?;
                }
                Ok(())
            }
            None => self.walk_apply(s, moves.list[i].1),
        }
    }

    /// The moves of the decision `dec` at a tree node in world `s`: `canonical`, then the
    /// cheap filters (`in_tree_filters`) and the subsets of a multi-card choice
    /// (`macro_multi_pick`).
    fn node_moves(&mut self, s: &GameState, dec: &DecisionView<'_>) -> Result<Moves, EngineError> {
        let mut list = canonical(s, dec);
        let flags = &self.cfg.mechanisms;
        if flags.in_tree_filters && list.len() > 1 {
            list = filters::filter(s, dec, list)?;
        }
        if flags.macro_multi_pick
            && list.len() > 1
            && macros::covers(dec)
            && let Some(subsets) =
                macros::expand(s, dec, &list, &mut self.effort.applies_aux, self.aux_cap)?
        {
            return Ok(Moves::subsets(subsets));
        }
        Ok(Moves::single(list))
    }

    /// `_select(node, s, dec, moves, cfg)` (`ai/mcts.py:271-302`): the priors when a move has
    /// no child yet (or, with `prior_worlds`, while the node takes this world's scores), one
    /// `avail` count for every move, then the mechanism's choice. Returns `(child, move index,
    /// fresh)`; a child without visits is fresh.
    fn select(
        &mut self,
        node: u32,
        s: &GameState,
        dec: &DecisionView<'_>,
        moves: &[(MoveKey, Action)],
    ) -> Result<(u32, usize, bool), EngineError> {
        let keys: Vec<ChildKey> = moves
            .iter()
            .map(|&(key, _)| child_key(dec, key, self.cfg))
            .collect();
        let refresh = match self.cfg.mechanisms.prior_worlds {
            Some(k) if node != self.root => {
                p6d_priors::wants(&self.tree.p6d.priors, node, self.world, k)
            }
            _ => false,
        };
        if refresh || keys.iter().any(|k| self.tree.get(node, k).is_none()) {
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
                world: self.world,
            })?;
        }
        let children: Vec<u32> = keys
            .iter()
            .map(|k| self.tree.get(node, k).expect("the priors add every child"))
            .collect();
        for &c in &children {
            if self.cfg.mechanisms.visited_set_backup {
                if self.counted.contains(&c) {
                    continue;
                }
                self.counted.push(c);
            }
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
            world: self.world,
        });
        let c = children[i];
        Ok((c, i, self.tree.children[c as usize].visits == 0))
    }

    /// `_descend(ctx, node, s, path)` (`ai/mcts.py:220-235`): walks the tree in world `s` until
    /// a new child is added or the horizon is reached. The child joins the path before its move
    /// is applied (Python appends after; nothing reads the path in between), so a move whose
    /// apply fails is charged with the error leaf. With `transpositions` the node of every
    /// decision is the node of its observation key, and the walk ends when it would enter a
    /// node it already entered in this iteration, or the root.
    fn descend(&mut self, mut node: u32, s: &mut GameState) -> Result<(), EngineError> {
        let transpose = self.cfg.mechanisms.transpositions;
        self.walked.clear();
        while s.winner().is_none() && s.turn() < self.line_horizon {
            let Some(dec) = s.pending() else { break };
            let moves = self.node_moves(s, &dec)?;
            if moves.len() == 1 {
                self.apply_move(s, &moves, 0)?;
                continue;
            }
            if transpose {
                node = self.tree.transposed(transpose::key(s, self.player));
                // The root is never re-entered: its children's visits are the root's iterations,
                // which index the worlds (a reused root is a node of the table).
                if node == self.root || self.walked.contains(&node) {
                    break;
                }
                self.walked.push(node);
            }
            let (child, i, fresh) = self.select(node, s, &dec, &moves.list)?;
            let (dec_player, dec_kind) = (dec.hdr.player, dec.hdr.kind);
            let action = moves.list[i].1;
            self.path.push(child);
            self.apply_move(s, &moves, i)?;
            if fresh {
                return Ok(());
            }
            if transpose
                || (self.cfg.observation_keys
                    && unobserved_pass(dec_player, dec_kind, action, self.player))
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

    /// Root move `i` of `root` in world `s`, the walk unless the root child is `fresh`, the
    /// rollout and the leaf value (`ai/mcts.py:205-213`, `:254-264`).
    fn line(
        &mut self,
        root: &Moves,
        i: usize,
        fresh: bool,
        first: u32,
        s: &mut GameState,
        rng: &mut SplitMix64,
    ) -> Result<f64, EngineError> {
        self.apply_move(s, root, i)?;
        if !fresh {
            let node = if self.cfg.mechanisms.transpositions {
                self.root
            } else {
                self.tree.node_of(first)
            };
            self.descend(node, s)?;
        }
        if self.rollouts > 1 {
            return self.rollouts_mean(s, rng);
        }
        self.rollout_leaf(s, rng)
    }

    /// One rollout from `s` and its leaf value (`ai/mcts.py:211-213`, `:254-264`).
    fn rollout_leaf(
        &mut self,
        s: &mut GameState,
        rng: &mut SplitMix64,
    ) -> Result<f64, EngineError> {
        let (decisions, capped) = self.rollout(s, rng)?;
        if capped {
            self.effort.rollouts_capped += 1;
        }
        let mut leaf = self.leaf_cx(s, decisions, capped);
        self.mech.leaf_value(&mut leaf)
    }

    /// `self.rollouts` rollouts from the walk's end `s` (each from the same state and horizon;
    /// rollout 0 with `rng`, rollout `j` with `derive_seed(rollout_seed, "multi-rollout", j)`),
    /// and the `py_sum` mean of their leaf values; `s` ends as the last rollout left it.
    fn rollouts_mean(
        &mut self,
        s: &mut GameState,
        rng: &mut SplitMix64,
    ) -> Result<f64, EngineError> {
        let start = s.clone();
        let horizon = self.line_horizon;
        let mut values = Vec::with_capacity(self.rollouts as usize);
        values.push(self.rollout_leaf(s, rng)?);
        for j in 1..self.rollouts {
            s.clone_from_state(&start);
            self.line_horizon = horizon;
            let seed = derive_seed(
                self.rollout_seed,
                &[SeedPart::Str("multi-rollout"), SeedPart::Int(i64::from(j))],
            );
            values.push(self.rollout_leaf(s, &mut SplitMix64::new(seed))?);
        }
        Ok(py_sum(values) / f64::from(self.rollouts))
    }

    /// `_iterate(ctx, first, fresh, s, rng)` (`ai/mcts.py:205-217`) with the root apply of
    /// `ai/mcts.py:137`; returns the leaf value and whether the game ended. A line that fails
    /// with an engine error is scored by `Mechanisms::error_leaf`, or fails the search.
    fn iterate(
        &mut self,
        first: u32,
        root: &Moves,
        i: usize,
        fresh: bool,
        s: &mut GameState,
        mut rng: SplitMix64,
    ) -> Result<(f64, bool), EngineError> {
        self.path.clear();
        self.path.push(first);
        self.line_horizon = self.horizon;
        let value = match self.line(root, i, fresh, first, s, &mut rng) {
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
}

/// The root statistics of one tree: `(visits, total)` per root move, `(0, 0.0)` for a move
/// without a child.
fn root_stats(tree: &Tree, root: u32, keys: &[ChildKey]) -> Vec<(u32, f64)> {
    keys.iter()
        .map(|k| {
            tree.get(root, k).map_or((0, 0.0), |c| {
                let ch = &tree.children[c as usize];
                (ch.visits, ch.total)
            })
        })
        .collect()
}

/// `_result(root, keys, moves, iterations)` (`ai/mcts.py:331-339`) over merged statistics
/// `(visits, total)`: the first move with the most visits, then the higher mean. The
/// alternatives are the root moves'; when the moves are subsets, the subsets that start with the
/// same pick are reported as one alternative (summed visits, `py_sum` of the totals over the
/// visits), in order of first appearance, because the agent plays that pick.
fn root_result(root: &Moves, stats: &[(u32, f64)]) -> (usize, Action, Vec<Alternative>) {
    let mean = |(v, t): (u32, f64)| if v == 0 { 0.0 } else { t / f64::from(v) };
    let best = first_max_visits_mean(stats.iter().map(|&s| (s.0, mean(s))));
    let action = root.list[best].1;
    if root.seqs.is_empty() {
        let alternatives = root
            .list
            .iter()
            .zip(stats)
            .map(|(&(_, action), &s)| Alternative {
                action,
                value: mean(s),
                visits: s.0,
            })
            .collect();
        return (best, action, alternatives);
    }
    let mut firsts: Vec<(Action, u32, Vec<f64>)> = Vec::new();
    for (&(_, a), &(v, t)) in root.list.iter().zip(stats) {
        match firsts.iter_mut().find(|(b, _, _)| *b == a) {
            Some((_, visits, totals)) => {
                *visits += v;
                totals.push(t);
            }
            None => firsts.push((a, v, vec![t])),
        }
    }
    let alternatives = firsts
        .into_iter()
        .map(|(action, visits, totals)| Alternative {
            action,
            value: mean((visits, py_sum(totals))),
            visits,
        })
        .collect();
    (best, action, alternatives)
}

/// What every tree of one decision shares: the information-set state and its decision, the
/// root moves and their child keys, and the leaf's Burst density.
struct RootData<'d> {
    st: &'d GameState,
    dec: &'d DecisionView<'d>,
    player: u8,
    cfg: &'d StrengthenedConfig,
    knobs: PolicyKnobs,
    density: Density,
    root: &'d Moves,
    keys: &'d [ChildKey],
    /// The rollouts of every iteration (`Mechanisms::rollouts`).
    rollouts: u32,
}

/// One tree of a decision with its worlds and root bookkeeping, owned so that a root-tree job
/// can carry it between phases (S1).
struct TreeData {
    tree: Tree,
    effort: SearchEffort,
    mech_effort: MechanismEffort,
    root: u32,
    worlds: Vec<GameState>,
    world_seeds: Vec<u64>,
    rollout_seeds: Vec<u64>,
    children: Vec<u32>,
    per_visit: Vec<Vec<f64>>,
    /// Per root child, the visits it had before this decision (`subtree_reuse`; 0 for a new
    /// tree): the k-th new visit plays in world k.
    base: Vec<u32>,
    /// Whether the per-visit differences are recorded (`cap > budget`).
    record: bool,
    done: u32,
    aux_cap: u64,
}

/// The seeds of one tree drawn up front from `rng` in the reference order: `W[0:b]`, `R[0:b]`,
/// `W[b:cap]`, `R[b:cap]` (`ai/mcts.py:122-125`).
fn draw_seeds(rng: &mut SplitMix64, b: u32, cap: u32) -> (Vec<u64>, Vec<u64>) {
    let mut world_seeds: Vec<u64> = (0..b).map(|_| rng.next_u64()).collect();
    let mut rollout_seeds: Vec<u64> = (0..b).map(|_| rng.next_u64()).collect();
    world_seeds.extend((b..cap).map(|_| rng.next_u64()));
    rollout_seeds.extend((b..cap).map(|_| rng.next_u64()));
    (world_seeds, rollout_seeds)
}

impl<'c, M: Mechanisms, T: SearchTracer + ?Sized> Search<'c, M, T> {
    /// The search over `data`'s tree (moved in; [`Search::store`] moves it back).
    fn over(mech: &'c M, d: &RootData<'c>, data: &mut TreeData, tracer: &'c mut T) -> Self {
        let horizon = d.st.turn() + d.cfg.horizon_turns;
        Search {
            mech,
            player: d.player,
            cfg: d.cfg,
            knobs: d.knobs,
            math: math_of(d.cfg),
            density: d.density,
            root_turn: d.st.turn(),
            horizon,
            line_horizon: horizon,
            tree: std::mem::take(&mut data.tree),
            effort: data.effort,
            mech_effort: data.mech_effort,
            tracer,
            n: 0,
            applies_walk: 0,
            applies_rollout: 0,
            path: Vec::new(),
            root: data.root,
            world: 0,
            walked: Vec::new(),
            counted: Vec::new(),
            aux_cap: data.aux_cap,
            rollouts: d.rollouts,
            rollout_seed: 0,
        }
    }

    fn store(self, data: &mut TreeData) {
        data.tree = self.tree;
        data.effort = self.effort;
        data.mech_effort = self.mech_effort;
    }
}

/// The tree of the decision: world 0 (`ai/mcts.py:126-130`), the root node and the root
/// priors in world 0 (existing children keep theirs). The tree is new, or with
/// `subtree_reuse` the kept tree with its node `root` as the root.
fn new_tree<M: Mechanisms>(
    mech: &M,
    d: &RootData<'_>,
    seeds: (Vec<u64>, Vec<u64>),
    effort: SearchEffort,
    budget: Budget,
    kept: Option<(Tree, u32)>,
) -> Result<TreeData, AgentError> {
    let (world_seeds, rollout_seeds) = seeds;
    let mut first_world = d.st.clone();
    mech.world(
        &mut WorldCx {
            st: d.st,
            player: d.player,
            cfg: d.cfg,
            seeds: &world_seeds,
        },
        0,
        &mut first_world,
    )?;
    let mut effort = effort;
    effort.worlds += 1;
    let (mut tree, root) = kept.unwrap_or_else(|| {
        let mut tree = Tree::default();
        let root = tree.new_node();
        (tree, root)
    });
    mech.priors(&mut NodeCx {
        tree: &mut tree,
        node: root,
        s: &first_world,
        dec: d.dec,
        moves: &d.root.list,
        keys: d.keys,
        children: &[],
        cfg: d.cfg,
        knobs: d.knobs,
        math: math_of(d.cfg),
        world: 0,
    })?;
    let children: Vec<u32> = d
        .keys
        .iter()
        .map(|k| tree.get(root, k).expect("root children were added"))
        .collect();
    let base = children
        .iter()
        .map(|&c| tree.children[c as usize].visits)
        .collect();
    Ok(TreeData {
        tree,
        effort,
        mech_effort: MechanismEffort::default(),
        root,
        worlds: vec![first_world],
        world_seeds,
        rollout_seeds,
        per_visit: vec![Vec::new(); children.len()],
        children,
        base,
        record: budget.cap > budget.iterations,
        done: 0,
        aux_cap: AUX_APPLIES_PER_ITERATION * u64::from(budget.cap),
    })
}

/// The iterations of `data`'s tree up to `limit`, or until its leader can no longer be
/// overtaken in the iterations left (`ai/mcts.py:131-150` between two close-call checks).
fn advance<M: Mechanisms, T: SearchTracer + ?Sized>(
    mech: &M,
    d: &RootData<'_>,
    data: &mut TreeData,
    limit: u32,
    scratch: &mut Scratch,
    cancel: &Cancel,
    tracer: &mut T,
) -> Result<(), AgentError> {
    let mut search = Search::over(mech, d, data, tracer);
    let result = iterations(&mut search, d, data, limit, scratch, cancel);
    search.store(data);
    result
}

fn iterations<M: Mechanisms, T: SearchTracer + ?Sized>(
    search: &mut Search<'_, M, T>,
    d: &RootData<'_>,
    data: &mut TreeData,
    limit: u32,
    scratch: &mut Scratch,
    cancel: &Cancel,
) -> Result<(), AgentError> {
    let root = data.root;
    while data.done < limit {
        if cancel.is_cancelled() {
            return Err(AgentError::Interrupted);
        }
        search.counted.clear();
        let (child, i, fresh) = search.select(root, &data.worlds[0], d.dec, &d.root.list)?;
        let k = search.tree.children[child as usize].visits - data.base[i];
        let ku = k as usize;
        if ku == data.worlds.len() {
            let mut world = d.st.clone();
            search.mech.world(
                &mut WorldCx {
                    st: d.st,
                    player: d.player,
                    cfg: d.cfg,
                    seeds: &data.world_seeds,
                },
                ku,
                &mut world,
            )?;
            data.worlds.push(world);
            search.effort.worlds += 1;
        }
        let mut world = scratch.clone_state(&data.worlds[ku]);
        search.n = data.done;
        search.applies_walk = 0;
        search.applies_rollout = 0;
        search.world = ku;
        search.rollout_seed = data.rollout_seeds[ku];
        let before = search.tree.children[child as usize].total;
        let (leaf, terminal) = search.iterate(
            child,
            d.root,
            i,
            fresh,
            &mut world,
            SplitMix64::new(data.rollout_seeds[ku]),
        )?;
        scratch.recycle_state(world);
        let after = search.tree.children[child as usize].total;
        let slot = data
            .children
            .iter()
            .position(|&c| c == child)
            .expect("a root child");
        if data.record {
            data.per_visit[slot].push(after - before);
        }
        let it = IterTrace {
            n: data.done,
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
        search.tracer.iteration(&data.worlds[ku], &it);
        data.done += 1;
        if search.settled(root, d.keys, limit - data.done) {
            break;
        }
    }
    Ok(())
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

/// [`search`] with the counters of the strengthened mechanisms. With `root_trees` the trees
/// run as fork-join jobs and the tracer records nothing.
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
    if cfg.mechanisms.subtree_reuse {
        return Err(AgentError::Value(
            "subtree_reuse needs the agent's carry (gcg_search::make_agent)".to_owned(),
        ));
    }
    search_with_carry(obs, cfg, rng, cx, tracer, None)
}

/// [`search_with_effort`] with the `subtree_reuse` carry of the deciding agent: the trees of
/// its last search are reused when this decision is the one its move led to, and this search's
/// trees are kept for the next.
pub(crate) fn search_with_carry<T: SearchTracer + ?Sized>(
    obs: &Observation<'_>,
    cfg: &StrengthenedConfig,
    rng: &mut SplitMix64,
    cx: &mut AgentCx<'_>,
    tracer: &mut T,
    carry: Option<&mut Carry>,
) -> Result<(SearchResult, MechanismEffort), AgentError> {
    validate(&cfg.mechanisms).map_err(AgentError::Value)?;
    let mech = Composed::new(cfg);
    let (mut result, mut effort) = match cfg.mechanisms.root_trees {
        Some(k) => run_trees(
            &mech,
            obs.info(),
            obs.player(),
            cfg,
            rng,
            cx,
            k.max(1),
            carry,
        ),
        None => run(&mech, obs.info(), obs.player(), cfg, rng, cx, tracer, carry),
    }?;
    result.stats.effort.applies_aux += mech.aux_applies();
    effort.add(mech.effort());
    Ok((result, effort))
}

/// The kept trees of `carry` for the decision at `st`, with their new roots; `None` for a tree
/// whose walk never entered the node after the played move (it starts fresh).
fn reused(
    carry: Option<&mut Carry>,
    st: &GameState,
    player: u8,
    cfg: &StrengthenedConfig,
) -> Vec<Option<(Tree, u32)>> {
    let Some(kept) = carry.and_then(|c| c.take_for(st, player)) else {
        return Vec::new();
    };
    kept.into_iter()
        .map(|k| reuse::root_of(&k, st, player, cfg.mechanisms.transpositions).map(|n| (k.tree, n)))
        .collect()
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
    pregame_with_applies(obs, cfg, rng, cx).map(|(choice, _)| choice)
}

/// [`pregame`] with the engine applies of the mechanisms' simulations (P6-H; the reference's
/// own play/draw games are not counted, as in the reference agent).
///
/// # Errors
/// As [`pregame`].
pub(crate) fn pregame_with_applies(
    obs: &Observation<'_>,
    cfg: &StrengthenedConfig,
    rng: &mut SplitMix64,
    cx: &mut AgentCx<'_>,
) -> Result<((Action, Vec<Alternative>), u64), AgentError> {
    validate(&cfg.mechanisms).map_err(AgentError::Value)?;
    let st = obs.info();
    let dec = st
        .pending()
        .filter(|d| {
            d.hdr.player == obs.player()
                && matches!(d.hdr.kind, DecisionKind::Redraw | DecisionKind::ChooseFirst)
        })
        .ok_or_else(|| AgentError::Assertion(String::new()))?;
    let mut pcx = PregameCx {
        st,
        dec: &dec,
        player: obs.player(),
        cfg,
        rng,
        agent: cx,
        applies: 0,
    };
    let choice = Composed::new(cfg).pregame(&mut pcx)?;
    Ok((choice, pcx.applies))
}

/// The root of a search: its moves (after `Mechanisms::root_moves`, and with
/// `macro_multi_pick` the subsets of a multi-card choice), their keys, the budget of the moves
/// before any subset expansion, the Burst density and the effort of the root work.
struct Prepared {
    root: Moves,
    keys: Vec<ChildKey>,
    budget: Budget,
    density: Density,
    effort: SearchEffort,
    /// The rollouts of every iteration (`Mechanisms::rollouts`).
    rollouts: u32,
}

/// The decision of `st` for `player`, or the reference's assertion error.
fn decision_of(st: &GameState, player: u8) -> Result<DecisionView<'_>, AgentError> {
    let dec = st
        .pending()
        .ok_or_else(|| AgentError::Assertion(String::new()))?;
    if dec.hdr.player != player {
        return Err(AgentError::Assertion(String::new()));
    }
    Ok(dec)
}

/// The root moves, budget and density of a search (`ai/mcts.py:89-121`), or the result of a
/// decision whose root filters left one move (`ai/mcts.py:111-112`).
fn prepare<M: Mechanisms>(
    mech: &M,
    st: &GameState,
    dec: &DecisionView<'_>,
    player: u8,
    cfg: &StrengthenedConfig,
    rng: &mut SplitMix64,
) -> Result<Result<Prepared, SearchResult>, AgentError> {
    let mut root_cx = RootCx {
        st,
        dec,
        player,
        cfg,
        knobs: cfg.knobs(),
        rng,
    };
    let root_moves = mech.root_moves(&mut root_cx)?;
    let mut effort = SearchEffort {
        applies_aux: mech.root_applies(),
        ..SearchEffort::default()
    };
    if root_moves.len() == 1 {
        let roots = root_moves.iter().map(|&(k, _)| k).collect();
        return Ok(Err(SearchResult {
            action: root_moves[0].1,
            alternatives: Vec::new(),
            stats: SearchStats {
                iterations: 0,
                budget: 0,
                cap: 0,
                roots,
                effort,
            },
        }));
    }
    let density = mech.density(&root_cx)?;
    let budget = mech.budget(&root_cx, root_moves.len());
    let rollouts = mech.rollouts(&root_cx);
    let mut root = Moves::single(root_moves);
    if cfg.mechanisms.macro_multi_pick && macros::covers(dec) {
        let aux_cap = AUX_APPLIES_PER_ITERATION * u64::from(budget.cap);
        if let Some(subsets) =
            macros::expand(st, dec, &root.list, &mut effort.applies_aux, aux_cap)?
            && subsets.len() > 1
        {
            root = Moves::subsets(subsets);
        }
    }
    let keys = root
        .list
        .iter()
        .map(|&(key, _)| child_key(dec, key, cfg))
        .collect();
    Ok(Ok(Prepared {
        root,
        keys,
        budget,
        density,
        effort,
        rollouts,
    }))
}

/// The statistics of a search over `trees`.
fn search_stats(p: &Prepared, trees: &[&TreeData]) -> SearchStats {
    let mut effort = SearchEffort::default();
    let mut iterations = 0;
    for t in trees {
        iterations += t.done;
        effort.applies_tree += t.effort.applies_tree;
        effort.applies_rollout += t.effort.applies_rollout;
        effort.applies_aux += t.effort.applies_aux;
        effort.worlds += t.effort.worlds;
        effort.rollouts_capped += t.effort.rollouts_capped;
    }
    effort.applies_aux += p.effort.applies_aux;
    SearchStats {
        iterations,
        budget: p.budget.iterations,
        cap: p.budget.cap,
        roots: p.root.list.iter().map(|&(k, _)| k).collect(),
        effort,
    }
}

/// The core loop over the information-set state `st` of `player` with the mechanisms `mech`:
/// one tree (`root_trees` is [`run_trees`]), reused from `carry` when it holds the tree of the
/// move that led here (`subtree_reuse`).
#[expect(
    clippy::too_many_arguments,
    reason = "the search inputs plus the agent's subtree_reuse carry"
)]
pub(crate) fn run<M: Mechanisms, T: SearchTracer + ?Sized>(
    mech: &M,
    st: &GameState,
    player: u8,
    cfg: &StrengthenedConfig,
    rng: &mut SplitMix64,
    cx: &mut AgentCx<'_>,
    tracer: &mut T,
    mut carry: Option<&mut Carry>,
) -> Result<(SearchResult, MechanismEffort), AgentError> {
    let dec = decision_of(st, player)?;
    let kept = reused(carry.as_deref_mut(), st, player, cfg);
    let p = match prepare(mech, st, &dec, player, cfg, rng)? {
        Ok(p) => p,
        Err(single) => {
            if let Some(c) = carry {
                c.clear();
            }
            return Ok((single, MechanismEffort::default()));
        }
    };
    let (b, cap) = (p.budget.iterations, p.budget.cap);
    let seeds = draw_seeds(rng, b, cap);
    let d = RootData {
        st,
        dec: &dec,
        player,
        cfg,
        knobs: cfg.knobs(),
        density: p.density,
        root: &p.root,
        keys: &p.keys,
        rollouts: p.rollouts,
    };
    let kept = match <[_; 1]>::try_from(kept) {
        Ok([one]) => one,
        Err(_) => None,
    };
    let mut data = new_tree(mech, &d, seeds, SearchEffort::default(), p.budget, kept)?;
    let mut limit = b;
    loop {
        advance(mech, &d, &mut data, limit, cx.scratch, cx.cancel, tracer)?;
        if limit == cap {
            break;
        }
        let root_children: Vec<Child> = data
            .children
            .iter()
            .map(|&c| data.tree.children[c as usize])
            .collect();
        if !close_call(
            &root_children,
            &data.per_visit,
            cfg.target_extension_z,
            math_of(cfg),
        ) {
            break;
        }
        limit = cap.min(limit + b);
    }
    let stats = root_stats(&data.tree, data.root, &p.keys);
    let (best, action, alternatives) = root_result(&p.root, &stats);
    let mut result = SearchResult {
        action,
        alternatives,
        stats: search_stats(&p, &[&data]),
    };
    let mech_effort = data.mech_effort;
    match carry {
        Some(c) if p.root.seqs.is_empty() => {
            let child = data.children[best];
            let kept = vec![Kept {
                tree: data.tree,
                child,
            }];
            c.keep(st, player, action, kept);
            result.stats.effort.applies_aux += Carry::KEEP_APPLIES;
        }
        Some(c) => c.clear(),
        None => {}
    }
    Ok((result, mech_effort))
}

/// A root tree between phases.
enum TreeState {
    /// Before the first phase: a new tree, or the kept tree and its new root
    /// (`subtree_reuse`).
    Start(Option<(Tree, u32)>),
    Running(Box<TreeData>),
}

impl TreeState {
    fn data(&self) -> &TreeData {
        match self {
            Self::Running(d) => d,
            Self::Start(_) => unreachable!("every tree ran its first phase"),
        }
    }
}

/// One root tree's job input: the tree's copy of the information-set state, its state and the
/// state of its composed mechanisms.
type TreeJob = (GameState, TreeState, ComposedState);

/// A phase of every root tree: tree `t` runs to `limit` iterations.
type TreeJobFn =
    Arc<dyn Fn(usize, TreeJob, &mut Scratch) -> Result<TreeJob, AgentError> + Send + Sync>;

/// The shared inputs of the root-tree jobs.
struct TreeShared {
    cfg: StrengthenedConfig,
    player: u8,
    root: Moves,
    density: Density,
    decision_seed: u64,
    budget: Budget,
    cancel: Cancel,
    rollouts: u32,
}

/// `root_trees{k}` (S1): `k` trees of `ceil(B/k)` iterations each, seeded
/// `derive_seed(decision_seed, "tree", t)`, run through `cx.par.fork_join` one phase at a time
/// and merged in tree order (`mech/p6d/ensemble.rs`). Each job builds the composed mechanisms
/// of the shared configuration over the tree's own mechanism state (the decision's root
/// findings, then the tree's), so the result never depends on the worker that ran it.
#[expect(
    clippy::too_many_arguments,
    reason = "the search inputs, the tree count and the agent's subtree_reuse carry"
)]
fn run_trees(
    mech: &Composed<'_>,
    st: &GameState,
    player: u8,
    cfg: &StrengthenedConfig,
    rng: &mut SplitMix64,
    cx: &mut AgentCx<'_>,
    k: u32,
    mut carry: Option<&mut Carry>,
) -> Result<(SearchResult, MechanismEffort), AgentError> {
    let dec = decision_of(st, player)?;
    let mut kept = reused(carry.as_deref_mut(), st, player, cfg);
    let p = match prepare(mech, st, &dec, player, cfg, rng)? {
        Ok(p) => p,
        Err(single) => {
            if let Some(c) = carry {
                c.clear();
            }
            return Ok((single, MechanismEffort::default()));
        }
    };
    if kept.len() != k as usize {
        kept = Vec::new();
    }
    kept.resize_with(k as usize, || None);
    let per = Budget {
        iterations: ensemble::per_tree(p.budget.iterations, k),
        cap: ensemble::per_tree(p.budget.cap, k),
    };
    let shared = Arc::new(TreeShared {
        cfg: cfg.clone(),
        player,
        root: p.root.clone(),
        density: p.density,
        decision_seed: rng.next_u64(),
        budget: per,
        cancel: cx.cancel.clone(),
        rollouts: p.rollouts,
    });
    let mut jobs: Vec<TreeJob> = kept
        .into_iter()
        .map(|t| (st.clone(), TreeState::Start(t), mech.tree_state()))
        .collect();
    let mut limit = per.iterations;
    let trees = loop {
        let phase = Arc::clone(&shared);
        let job: TreeJobFn = Arc::new(move |t, (info, data, state), scratch| {
            let dec = decision_of(&info, phase.player)?;
            let mech = Composed::with_state(&phase.cfg, state);
            let aux_before = mech.aux_applies();
            let effort_before = mech.effort();
            let keys: Vec<ChildKey> = phase
                .root
                .list
                .iter()
                .map(|&(key, _)| child_key(&dec, key, &phase.cfg))
                .collect();
            let d = RootData {
                st: &info,
                dec: &dec,
                player: phase.player,
                cfg: &phase.cfg,
                knobs: phase.cfg.knobs(),
                density: phase.density,
                root: &phase.root,
                keys: &keys,
                rollouts: phase.rollouts,
            };
            let mut data = match data {
                TreeState::Running(data) => *data,
                TreeState::Start(kept) => {
                    let mut tree_rng = SplitMix64::new(ensemble::tree_seed(phase.decision_seed, t));
                    let seeds =
                        draw_seeds(&mut tree_rng, phase.budget.iterations, phase.budget.cap);
                    new_tree(
                        &mech,
                        &d,
                        seeds,
                        SearchEffort::default(),
                        phase.budget,
                        kept,
                    )?
                }
            };
            advance(
                &mech,
                &d,
                &mut data,
                limit,
                scratch,
                &phase.cancel,
                &mut NoTrace,
            )?;
            data.effort.applies_aux += mech.aux_applies() - aux_before;
            data.mech_effort.add(mech.effort().since(effort_before));
            let state = mech.into_state();
            Ok((info, TreeState::Running(Box::new(data)), state))
        });
        let mut next = Vec::with_capacity(jobs.len());
        for r in cx.par.fork_join(jobs, job) {
            next.push(r?);
        }
        jobs = next;
        if limit == per.cap || !trees_close_call(&jobs, &p.keys, cfg) {
            break jobs;
        }
        limit = per.cap.min(limit + per.iterations);
    };
    let datas: Vec<&TreeData> = trees.iter().map(|(_, d, _)| d.data()).collect();
    let per_tree: Vec<Vec<(u32, f64)>> = datas
        .iter()
        .map(|t| root_stats(&t.tree, t.root, &p.keys))
        .collect();
    let merged: Vec<(u32, f64)> = ensemble::merge(&per_tree)
        .into_iter()
        .map(|m| (m.visits, m.total))
        .collect();
    let (best, action, alternatives) = root_result(&p.root, &merged);
    let mut mech_effort = MechanismEffort::default();
    for t in &datas {
        mech_effort.add(t.mech_effort);
    }
    let mut result = SearchResult {
        action,
        alternatives,
        stats: search_stats(&p, &datas),
    };
    match carry {
        Some(c) if !p.root.seqs.is_empty() => c.clear(),
        Some(c) => {
            let kept = trees
                .into_iter()
                .map(|(_, t, _)| match t {
                    TreeState::Running(data) => Kept {
                        child: data.children[best],
                        tree: data.tree,
                    },
                    TreeState::Start(_) => unreachable!("every tree ran its first phase"),
                })
                .collect();
            c.keep(st, player, action, kept);
            result.stats.effort.applies_aux += Carry::KEEP_APPLIES;
        }
        None => {}
    }
    Ok((result, mech_effort))
}

/// The close-call test over the root trees of `jobs` (`ensemble::close_call`).
fn trees_close_call(jobs: &[TreeJob], keys: &[ChildKey], cfg: &StrengthenedConfig) -> bool {
    let datas: Vec<&TreeData> = jobs.iter().map(|(_, d, _)| d.data()).collect();
    let per_tree: Vec<Vec<(u32, f64)>> = datas
        .iter()
        .map(|t| root_stats(&t.tree, t.root, keys))
        .collect();
    let merged = ensemble::merge(&per_tree);
    let per_visit: Vec<&[Vec<f64>]> = datas.iter().map(|t| t.per_visit.as_slice()).collect();
    ensemble::close_call(&merged, &per_visit, cfg.target_extension_z, math_of(cfg))
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
