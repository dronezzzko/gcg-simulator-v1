//! The extension points of the strengthened search (docs/port/PLAN.md §12.3): the core loop
//! (`mcts.rs`) calls a [`Mechanisms`] implementation at every point a Phase 6 mechanism can
//! change, and every method's default is the reference behaviour (`reference.rs`, a line-by-line
//! split of the reference search, `ai/mcts.py:82-339`, `ai/agents.py:85-98`).
//!
//! Each work package implements its pieces in `mech/<pkg>/` (one writer per directory) and
//! [`Composed`] dispatches to them by the flags of [`crate::MechanismFlags`]; with every flag off
//! it is the reference, so the null configuration replays the reference bit for bit
//! (`tests/null_config.rs`). A flag whose mechanism no package has implemented yet is refused by
//! [`validate`], never silently ignored.
//!
//! Every context gives the mechanism the observer's *information-set state* (or a world
//! determinised from it), never the true state (W-135, `observation.rs`).
//!
//! P6-D's mechanisms of the tree itself run in the core loop, gated by their flags: the root
//! ensemble (`root_trees`), the moves of a tree node (`in_tree_filters`, `macro_multi_pick`), the
//! node of a walk (`transpositions`) and the agent's kept trees (`subtree_reuse`); its
//! `prior_worlds` and `visited_set_backup` dispatch through [`Composed`] (`mech/p6d/`).
//!
//! P6-C's hidden-information mechanisms (`mech/p6c/`) dispatch through [`Composed`] too: the
//! worlds (`belief_worlds`, `opponent_view_share`), the selection at opponent nodes and the
//! backup (`opponent_buckets`, `opponent_view_share`).
//!
//! # Composition order
//!
//! Where several packages hook one method, [`Composed`] runs them in this order; each step runs
//! only while its flags are on, so every flag alone behaves as in its own package and the null
//! configuration is the reference (`compose_tests.rs` checks both).
//!
//! - `root_moves`: P6-A's leaf-constant check (W-02), P6-C's note of the root, P6-E's root moves
//!   (or the reference's), then P6-B's `lethal_root` on those moves.
//! - `root_applies`: P6-E's plus P6-B's. `aux_applies`: P6-F's, P6-A's, P6-B's and P6-C's.
//! - `budget`: P6-E's `target_rollouts` budget (or the reference's), then P6-B's solver cap and
//!   `critical_extension`.
//! - `world`: P6-C's worlds (or the reference's). P6-B's `lethal_root` builds its worlds through
//!   this method, so it sees P6-C's.
//! - `priors`: P6-E, P6-D and P6-F as before, then P6-C's note. `select`: P6-C at opponent
//!   nodes, else P6-E's (or the reference's), then P6-C's note. `backup`: P6-C's opponent-view
//!   backup, else P6-D's (or the reference's), then P6-E's record.
//! - `rollout_choice`: P6-B's proved lethal (`tactical_solver`), else P6-F's policy (or the
//!   reference's). `horizon`: P6-A's `adaptive_horizon` (or the reference's).
//! - `leaf_value`, outermost first: (1) P6-A's `capped_rollout_continuation` continues a capped
//!   line on a copy (0.5 if still unsettled); (2) a finished line takes the leaf mapping's win
//!   or loss (P6-A's `generalized_leaf` mapping, else the reference constants); (3) P6-B's
//!   `threat_leaf`: a proved lethal of the leaf's turn player takes the same mapping's win or
//!   loss; (4) otherwise the mapping's estimate of the evaluation's log-odds, which is a P6-G
//!   model when a model flag is on, else the configuration's weights. P6-B's
//!   `effective_blockers` and `solver_can_kill` replace the `can_kill` (and race) columns of
//!   the model's input, or of the weights' features when those are the role features (as in
//!   P6-B alone); P6-G's `leaf_density` picks the Burst density either way. The packages'
//!   own leaves, `p6a::leaf_value`, `p6b::leaf::leaf_value` and `p6g::leaf_value`, are the
//!   test oracles of this order.
//! - `error_leaf`: P6-O's `engine_error_leaf`, valued by P6-A's mapping under
//!   `generalized_leaf`. `pregame`: P6-H, whose simulations score through `leaf_value`.
//! - [`validate`]: availability, then `p6c::check`, then `p6g::validate`.

#[cfg(test)]
mod compose_tests;
pub(crate) mod p6a;
pub(crate) mod p6b;
pub(crate) mod p6c;
pub(crate) mod p6d;
pub(crate) mod p6e;
pub(crate) mod p6f;
pub(crate) mod p6g;
pub(crate) mod p6h;
pub(crate) mod p6o;
pub(crate) mod reference;
#[cfg(test)]
mod tests;

use gcg_ai::{
    AgentCx, AgentError, Alternative, Density, FeatureSet, MoveKey, PolicyKnobs, ROLE_FEATURES,
    Weights,
};
use gcg_core::rng::SplitMix64;
use gcg_engine::{Action, DecisionView, EngineError, GameState};

use crate::tree::{ChildKey, Tree};
use crate::{MechanismEffort, MechanismFlags, StrengthenedConfig};
use p6o::math::Math;

/// The iteration budget of one decision (`ai/mcts.py:121-124`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Budget {
    /// Iterations before the first settled or close-call check (`config.budget(n)`).
    pub(crate) iterations: u32,
    /// The most iterations a close call may extend to (`_iteration_cap`).
    pub(crate) cap: u32,
}

/// The move a selection chose: an index into [`NodeCx::moves`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct ChildIdx(pub(crate) usize);

/// What a simulated line does when its rollout stops (PLAN §12.3 S2).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum HorizonDecision {
    /// Score the leaf here (the reference).
    Leaf,
    /// Continue the rollout: the horizon moves `turns` turn boundaries later and the decision
    /// cap grows by `decisions`. The core stops anyway once a continued rollout applies nothing.
    Extend { turns: i32, decisions: u32 },
}

/// The root of one decision: the information-set state and its pending decision.
pub(crate) struct RootCx<'x> {
    /// The observer's information-set state (never the true state).
    pub(crate) st: &'x GameState,
    pub(crate) dec: &'x DecisionView<'x>,
    pub(crate) player: u8,
    pub(crate) cfg: &'x StrengthenedConfig,
    pub(crate) knobs: PolicyKnobs,
    /// The per-decision generator; root mechanisms draw from it in a fixed order.
    pub(crate) rng: &'x mut SplitMix64,
}

/// The building of world `k` (`ai/mcts.py:126-130`, `:142-145`).
pub(crate) struct WorldCx<'x> {
    /// The observer's information-set state the worlds are determinised from.
    pub(crate) st: &'x GameState,
    pub(crate) player: u8,
    pub(crate) cfg: &'x StrengthenedConfig,
    /// The world seeds drawn up front (`W[0:cap]`).
    pub(crate) seeds: &'x [u64],
}

/// One tree node during a walk: its priors (`_add_children`) and its selection (`_select`).
pub(crate) struct NodeCx<'x> {
    pub(crate) tree: &'x mut Tree,
    pub(crate) node: u32,
    /// The world state at the node.
    pub(crate) s: &'x GameState,
    pub(crate) dec: &'x DecisionView<'x>,
    /// The moves available at the node, in order (the root moves at the root).
    pub(crate) moves: &'x [(MoveKey, Action)],
    /// The child key of each move.
    pub(crate) keys: &'x [ChildKey],
    /// The child of each move; empty while the priors are computed.
    pub(crate) children: &'x [u32],
    pub(crate) cfg: &'x StrengthenedConfig,
    pub(crate) knobs: PolicyKnobs,
    pub(crate) math: Math,
    /// The index of the running iteration's world (the k-th visit of a root move plays in
    /// world k, `ai/mcts.py:136-145`); `prior_worlds` (W-140) counts the worlds by it.
    pub(crate) world: usize,
}

/// One rollout decision (`play_until_turn`, `ai/playout.py:22-26`).
pub(crate) struct RolloutCx<'x> {
    pub(crate) s: &'x GameState,
    pub(crate) rng: &'x mut SplitMix64,
    /// The searching player (the rollout's mover is `s`'s pending player).
    pub(crate) player: u8,
    pub(crate) cfg: &'x StrengthenedConfig,
    pub(crate) knobs: PolicyKnobs,
    pub(crate) math: Math,
}

/// The end of a simulated line: the horizon check and the leaf value (`_leaf_value`).
pub(crate) struct LeafCx<'x> {
    pub(crate) s: &'x GameState,
    pub(crate) player: u8,
    pub(crate) root_turn: i32,
    /// The horizon turn of this line (the root turn plus `horizon_turns`, plus extensions).
    pub(crate) horizon: i32,
    /// Rollout decisions taken in this line.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "read by the P6-A horizon mechanisms (W-15)")
    )]
    pub(crate) decisions: u32,
    /// The rollout stopped at its decision cap with the game unfinished before the horizon.
    pub(crate) capped: bool,
    pub(crate) cfg: &'x StrengthenedConfig,
    pub(crate) weights: &'x Weights,
    /// `burst_density` of the information-set state (`ai/mcts.py:120`).
    pub(crate) density: Density,
    pub(crate) math: Math,
}

/// The backup of one line's value along its path (`ai/mcts.py:214-217`).
pub(crate) struct BackupCx<'x> {
    pub(crate) tree: &'x mut Tree,
    /// The children walked, the root child first; duplicates kept (W-06).
    pub(crate) path: &'x [u32],
    pub(crate) value: f64,
    pub(crate) player: u8,
}

/// A REDRAW or CHOOSE_FIRST decision (`ai/agents.py:90-95`).
pub(crate) struct PregameCx<'x, 'a> {
    /// The observer's information-set state.
    pub(crate) st: &'x GameState,
    pub(crate) dec: &'x DecisionView<'x>,
    pub(crate) player: u8,
    pub(crate) cfg: &'x StrengthenedConfig,
    pub(crate) rng: &'x mut SplitMix64,
    pub(crate) agent: &'x mut AgentCx<'a>,
    /// Engine applies of the mechanism's simulations (PLAN §12.3: simulations count as
    /// applies), reported as the decision's `applies_aux`.
    pub(crate) applies: u64,
}

/// The extension points of the core loop (PLAN §12.3). Every default is the reference.
pub(crate) trait Mechanisms {
    /// The root moves after pruning, the no-effect filter and merging (`ai/mcts.py:89-110`).
    fn root_moves(&self, cx: &mut RootCx<'_>) -> Result<Vec<(MoveKey, Action)>, EngineError> {
        reference::root_moves(cx)
    }

    /// The engine applies the last [`Mechanisms::root_moves`] spent beyond the reference's root
    /// filters (counted as the decision's `applies_aux`, PLAN §12.3); the reference counts none.
    fn root_applies(&self) -> u64 {
        0
    }

    /// The budget for `n_moves` root moves (`ai/mcts.py:121-124`).
    fn budget(&self, cx: &RootCx<'_>, n_moves: usize) -> Budget {
        reference::budget(cx, n_moves)
    }

    /// World `k` into `out` (`determinize(st, player, world_seeds[k])`).
    fn world(
        &self,
        cx: &mut WorldCx<'_>,
        k: usize,
        out: &mut GameState,
    ) -> Result<(), EngineError> {
        reference::world(cx, k, out)
    }

    /// The children of a node and their priors (`_add_children`, `ai/mcts.py:305-317`).
    fn priors(&self, cx: &mut NodeCx<'_>) -> Result<(), EngineError> {
        reference::priors(cx)
    }

    /// The child to walk (`_select`, `ai/mcts.py:271-302`, after the priors and the `avail`
    /// counts).
    fn select(&self, cx: &mut NodeCx<'_>) -> ChildIdx {
        reference::select(cx)
    }

    /// One rollout move (`policy.choose`, `ai/policy.py:72-83`).
    fn rollout_choice(&self, cx: &mut RolloutCx<'_>) -> Result<Action, EngineError> {
        reference::rollout_choice(cx)
    }

    /// Whether a stopped rollout is the leaf.
    fn horizon(&self, cx: &LeafCx<'_>) -> HorizonDecision {
        reference::horizon(cx)
    }

    /// The leaf value from the searching player's point of view (`_leaf_value`,
    /// `ai/mcts.py:254-264`).
    fn leaf_value(&self, cx: &mut LeafCx<'_>) -> Result<f64, EngineError> {
        reference::leaf_value(cx)
    }

    /// The value of a simulated line that ended in `err`, or `None` to fail the search with it
    /// (the reference).
    fn error_leaf(&self, cx: &LeafCx<'_>, err: &EngineError) -> Option<f64> {
        let _ = (cx, err);
        None
    }

    /// The backup of the leaf value (`ai/mcts.py:214-217`).
    fn backup(&self, cx: &mut BackupCx<'_>) {
        reference::backup(cx);
    }

    /// REDRAW and CHOOSE_FIRST (`mulligan_choice`, `first_player_choice`).
    fn pregame(
        &self,
        cx: &mut PregameCx<'_, '_>,
    ) -> Result<(Action, Vec<Alternative>), AgentError> {
        reference::pregame(cx)
    }
}

/// The mechanisms a configuration's flags select.
pub(crate) struct Composed<'c> {
    flags: &'c MechanismFlags,
    /// P6-A's counters of the running search (capped-rollout continuations, extensions).
    p6a: p6a::DecisionState,
    /// P6-B's per-decision state: the tactical solver's applies and their cap.
    p6b: p6b::DecisionState,
    /// P6-C's per-search state: the opponent-node slices and the world bookkeeping.
    p6c: p6c::Shared,
    /// P6-E's root information of the running search.
    p6e: p6e::Shared,
    /// P6-F's per-decision state: its look-ahead applies and the evaluation's Burst density.
    p6f: p6f::DecisionState,
}

/// The per-search state of [`Composed`]: P6-B's solver state, P6-C's search state, P6-E's root
/// findings and P6-F's per-decision state (P6-A's counters start afresh in each phase, which
/// reports its own auxiliary applies). Each root tree of `root_trees` (S1) carries its own
/// between its phases, starting from its decision's root findings ([`Composed::tree_state`]),
/// so the trees see what the root moves found and their counts never depend on the worker
/// that ran them.
pub(crate) struct ComposedState {
    p6b: p6b::DecisionState,
    p6c: p6c::Search,
    p6e: p6e::RootInfo,
    p6f: p6f::DecisionState,
}

impl<'c> Composed<'c> {
    pub(crate) const fn new(cfg: &'c StrengthenedConfig) -> Self {
        Self {
            flags: &cfg.mechanisms,
            p6a: p6a::DecisionState::new(),
            p6b: p6b::DecisionState::new(),
            p6c: p6c::Shared::new(),
            p6e: p6e::Shared::new(),
            p6f: p6f::DecisionState::new(),
        }
    }

    /// The mechanisms of `cfg` resuming `state`.
    pub(crate) fn with_state(cfg: &'c StrengthenedConfig, state: ComposedState) -> Self {
        Self {
            flags: &cfg.mechanisms,
            p6a: p6a::DecisionState::new(),
            p6b: state.p6b,
            p6c: p6c::Shared::from_search(state.p6c),
            p6e: p6e::Shared::from_info(state.p6e),
            p6f: state.p6f,
        }
    }

    /// The state of this search, moved out.
    pub(crate) fn into_state(self) -> ComposedState {
        ComposedState {
            p6b: self.p6b,
            p6c: self.p6c.into_search(),
            p6e: self.p6e.into_info(),
            p6f: self.p6f,
        }
    }

    /// The state a root tree of this search starts from: a copy of the root findings, a fresh
    /// per-decision state of the rollout policy, P6-B's share of the solver cap and P6-C's
    /// findings of the decision.
    pub(crate) fn tree_state(&self) -> ComposedState {
        ComposedState {
            p6b: self.p6b.for_tree(self.flags.root_trees.unwrap_or(1)),
            p6c: self.p6c.tree_copy(),
            p6e: self.p6e.info(),
            p6f: p6f::DecisionState::new(),
        }
    }

    /// The engine applies the mechanisms spent outside the tree walk and the rollouts
    /// (`SearchEffort::applies_aux`): P6-F's look-aheads, P6-A's capped-rollout continuations,
    /// P6-B's solver in rollouts and leaves and P6-C's candidate worlds.
    pub(crate) fn aux_applies(&self) -> u64 {
        self.p6f.aux_applies()
            + self.p6a.aux_applies()
            + self.p6b.aux_applies()
            + self.p6c.applies()
    }

    /// The mechanisms' counters of this search besides the core's `engine_error_leaves`:
    /// P6-A's extended, continued and unsettled lines and P6-C's reweighted and opponent-view
    /// worlds.
    pub(crate) fn effort(&self) -> MechanismEffort {
        let a = self.p6a.counts();
        let (reweighted, view) = self.p6c.worlds();
        MechanismEffort {
            engine_error_leaves: 0,
            horizon_extensions: a.extended,
            capped_continued: a.capped,
            capped_unsettled: a.unsettled,
            belief_reweighted_worlds: reweighted,
            opponent_view_worlds: view,
        }
    }
}

impl Mechanisms for Composed<'_> {
    /// P6-A's leaf-constant check (W-02) runs first, so a configuration whose horizon is too
    /// long for its leaf constants fails the decision before any iteration; then P6-C's note
    /// of the root, the root moves of P6-E (or the reference's), then P6-B's `lethal_root`.
    fn root_moves(&self, cx: &mut RootCx<'_>) -> Result<Vec<(MoveKey, Action)>, EngineError> {
        if p6a::active(self.flags) {
            p6a::check(cx.cfg).map_err(EngineError::Value)?;
        }
        if p6c::shapes_worlds(self.flags) || p6c::shapes_tree(self.flags) {
            p6c::note_root(self.flags, cx, &self.p6c);
        }
        let moves = if p6e::any_on(self.flags) {
            p6e::root_moves(self, self.flags, cx, &self.p6e)?
        } else {
            reference::root_moves(cx)?
        };
        if self.flags.lethal_root {
            p6b::root::root_moves(self, &self.p6b, cx, moves)
        } else {
            Ok(moves)
        }
    }

    fn root_applies(&self) -> u64 {
        self.p6e.applies() + self.p6b.root_applies()
    }

    /// The budget of P6-E (or the reference's), then P6-B's solver cap and
    /// `critical_extension`.
    fn budget(&self, cx: &RootCx<'_>, n_moves: usize) -> Budget {
        let b = if self.flags.target_rollouts {
            p6e::budget(cx, n_moves, &self.p6e)
        } else {
            reference::budget(cx, n_moves)
        };
        if p6b::any_on(self.flags) {
            p6b::budget(&self.p6b, self.flags, cx, b)
        } else {
            b
        }
    }

    fn world(
        &self,
        cx: &mut WorldCx<'_>,
        k: usize,
        out: &mut GameState,
    ) -> Result<(), EngineError> {
        if p6c::shapes_worlds(self.flags) {
            p6c::world(self.flags, cx, k, out, &self.p6c)
        } else {
            reference::world(cx, k, out)
        }
    }

    /// The scores are P6-F's strengthened scores when a P6-F flag is on (otherwise the
    /// reference's); P6-E's root findings shape the root priors, `prior_worlds` (P6-D) the
    /// in-tree priors. P6-C notes the root's children at the first call.
    fn priors(&self, cx: &mut NodeCx<'_>) -> Result<(), EngineError> {
        self.composed_priors(cx)?;
        if p6c::shapes_tree(self.flags) {
            p6c::note_priors(cx, &self.p6c);
        }
        Ok(())
    }

    fn select(&self, cx: &mut NodeCx<'_>) -> ChildIdx {
        if p6c::shapes_tree(self.flags)
            && let Some(i) = p6c::select(self.flags, cx, &self.p6c)
        {
            return i;
        }
        let i = if p6e::changes_select(cx, &self.p6e) {
            p6e::select(cx, &self.p6e)
        } else {
            reference::select(cx)
        };
        if p6c::shapes_tree(self.flags) {
            p6c::after_select(self.flags, cx, i, &self.p6c);
        }
        i
    }

    /// P6-B's proved lethal line (`tactical_solver`), else P6-F's policy (or the reference's).
    fn rollout_choice(&self, cx: &mut RolloutCx<'_>) -> Result<Action, EngineError> {
        if self.flags.tactical_solver
            && let Some(a) = p6b::rollout_lethal(&self.p6b, cx)?
        {
            return Ok(a);
        }
        if p6f::active(self.flags) {
            p6f::rollout_choice(&self.p6f, self.flags, cx)
        } else {
            reference::rollout_choice(cx)
        }
    }

    fn horizon(&self, cx: &LeafCx<'_>) -> HorizonDecision {
        if self.flags.adaptive_horizon {
            p6a::horizon(&self.p6a, cx)
        } else {
            reference::horizon(cx)
        }
    }

    /// The leaf mechanisms of P6-A, P6-B and P6-G in the order of the module documentation
    /// ("Composition order"); with none of their leaf flags on, the reference's.
    fn leaf_value(&self, cx: &mut LeafCx<'_>) -> Result<f64, EngineError> {
        let f = self.flags;
        if p6a::changes_leaf(f) || p6b::changes_leaf(f) || p6g::active(f) {
            p6a::leaf_value_with(&self.p6a, cx, |at| self.settled_leaf(at))
        } else {
            reference::leaf_value(cx)
        }
    }

    fn backup(&self, cx: &mut BackupCx<'_>) {
        let backed_up = p6c::shapes_tree(self.flags) && p6c::backup(self.flags, cx, &self.p6c);
        if !backed_up {
            if self.flags.visited_set_backup {
                p6d::backup::backup(cx);
            } else {
                reference::backup(cx);
            }
        }
        if self.flags.target_rollouts && p6e::records_backups(&self.p6e) {
            p6e::record_backup(cx, &self.p6e);
        }
    }

    /// With `generalized_leaf` the loss is P6-A's (the reference constants could rank it
    /// above an evaluated position on a long line).
    fn error_leaf(&self, cx: &LeafCx<'_>, err: &EngineError) -> Option<f64> {
        if !self.flags.engine_error_leaf {
            return None;
        }
        let loss = p6o::error_leaf(cx, err)?;
        Some(if self.flags.generalized_leaf {
            p6a::loss_value(cx)
        } else {
            loss
        })
    }

    fn pregame(
        &self,
        cx: &mut PregameCx<'_, '_>,
    ) -> Result<(Action, Vec<Alternative>), AgentError> {
        p6h::pregame(self.flags, cx)
    }
}

impl Composed<'_> {
    /// The value of a line that needs no capped-rollout continuation: a finished line by the
    /// leaf mapping (P6-A), a proved lethal of the turn player (P6-B `threat_leaf`) by the same
    /// mapping, else the mapping's estimate of [`Composed::leaf_score`]. With P6-A's flags
    /// alone it is P6-A's settled value, bit for bit.
    fn settled_leaf(&self, cx: &mut LeafCx<'_>) -> Result<f64, EngineError> {
        let f = self.flags;
        if cx.s.winner().is_some() || !(p6b::changes_leaf(f) || p6g::active(f)) {
            return p6a::settled_value(cx);
        }
        if f.threat_leaf
            && let Some(winner) = p6b::leaf::proved_lethal(&self.p6b, cx)?
        {
            return Ok(p6a::proved_value(cx, winner));
        }
        let z = self.leaf_score(cx)? / cx.cfg.value_scale;
        Ok(p6a::estimate_value(cx, z))
    }

    /// The evaluation's log-odds of an unfinished leaf for the searching player: a P6-G model
    /// (its role-feature columns swapped by P6-B's `effective_blockers` / `solver_can_kill`),
    /// else P6-B's feature swaps under role-feature weights, else the configuration's weights;
    /// the Burst density is P6-G's `leaf_density` choice in every case.
    fn leaf_score(&self, cx: &LeafCx<'_>) -> Result<f64, EngineError> {
        let f = self.flags;
        if p6g::has_model(f) {
            return p6g::leaf_score_with(f, cx, |s, x| {
                p6b::leaf::swap_role_features(f, s, &mut x[..ROLE_FEATURES.len()])
            });
        }
        if p6b::leaf::swaps_features(f) && cx.weights.feature_set() == FeatureSet::Roles {
            return p6b::leaf::feature_score(f, cx, p6g::density(f, cx));
        }
        p6g::leaf_score(f, cx)
    }

    /// The priors of every package but P6-C.
    fn composed_priors(&self, cx: &mut NodeCx<'_>) -> Result<(), EngineError> {
        let root = p6e::changes_priors(cx, &self.p6e);
        let worlds = self.flags.prior_worlds.filter(|_| !root);
        let strengthened = p6f::active(self.flags);
        if !root && worlds.is_none() {
            return if strengthened {
                p6f::priors(&self.p6f, self.flags, cx)
            } else {
                reference::priors(cx)
            };
        }
        let scores = if strengthened {
            p6f::node_scores(&self.p6f, self.flags, cx)?
        } else {
            reference::node_scores(cx)?
        };
        match worlds {
            Some(k) => p6d::priors::priors(cx, k, &scores),
            None => p6e::priors(cx, &self.p6e, scores),
        }
        Ok(())
    }
}

/// The math every context of `cfg` uses (`portable_math`, W-137).
pub(crate) const fn math_of(cfg: &StrengthenedConfig) -> Math {
    Math::of(cfg.mechanisms.portable_math)
}

/// Whether a package has implemented the mechanism `name`.
fn available(name: &str) -> bool {
    p6a::PROVIDES.contains(&name)
        || p6b::PROVIDES.contains(&name)
        || p6c::PROVIDES.contains(&name)
        || p6d::PROVIDES.contains(&name)
        || p6e::PROVIDES.contains(&name)
        || p6f::PROVIDES.contains(&name)
        || p6g::PROVIDES.contains(&name)
        || p6h::PROVIDES.contains(&name)
        || p6o::PROVIDES.contains(&name)
}

/// Refuses a configuration that enables a mechanism no package has implemented yet.
///
/// # Errors
/// `mechanism <name> (<package>) is not available in this build`, for the first such flag in
/// declaration order; then a P6-C setting that does nothing (`p6c::check`); then P6-G's refusal
/// of a model flag combination without a fitted model (`p6g::validate`).
pub(crate) fn validate(flags: &MechanismFlags) -> Result<(), String> {
    for &(name, pkg) in MechanismFlags::FIELDS {
        if flags.get(name).is_some_and(crate::FlagValue::is_on) && !available(name) {
            return Err(format!(
                "mechanism {name} ({pkg}) is not available in this build"
            ));
        }
    }
    p6c::check(flags)?;
    p6g::validate(flags)
}
