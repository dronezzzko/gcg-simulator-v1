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

pub(crate) mod p6o;
pub(crate) mod reference;
#[cfg(test)]
mod tests;

use gcg_ai::{AgentCx, AgentError, Alternative, Density, MoveKey, PolicyKnobs, Weights};
use gcg_core::rng::SplitMix64;
use gcg_engine::{Action, DecisionView, EngineError, GameState};

use crate::tree::{ChildKey, Tree};
use crate::{MechanismFlags, StrengthenedConfig};
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
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "returned by the P6-A horizon mechanisms (W-01, W-15)"
        )
    )]
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
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "read by the P6-C world mechanisms (W-05, W-07)")
    )]
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
}

/// One rollout decision (`play_until_turn`, `ai/playout.py:22-26`).
pub(crate) struct RolloutCx<'x> {
    pub(crate) s: &'x GameState,
    pub(crate) rng: &'x mut SplitMix64,
    /// The searching player (the rollout's mover is `s`'s pending player).
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "read by the P6-F opponent policy (W-23)")
    )]
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
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "read by the P6-A horizon mechanisms (W-01)")
    )]
    pub(crate) horizon: i32,
    /// Rollout decisions taken in this line.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "read by the P6-A horizon mechanisms (W-15)")
    )]
    pub(crate) decisions: u32,
    /// The rollout stopped at its decision cap with the game unfinished before the horizon.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "read by the P6-A horizon mechanisms (W-15)")
    )]
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
}

/// The extension points of the core loop (PLAN §12.3). Every default is the reference.
pub(crate) trait Mechanisms {
    /// The root moves after pruning, the no-effect filter and merging (`ai/mcts.py:89-110`).
    fn root_moves(&self, cx: &mut RootCx<'_>) -> Result<Vec<(MoveKey, Action)>, EngineError> {
        reference::root_moves(cx)
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
}

impl<'c> Composed<'c> {
    pub(crate) const fn new(cfg: &'c StrengthenedConfig) -> Self {
        Self {
            flags: &cfg.mechanisms,
        }
    }
}

impl Mechanisms for Composed<'_> {
    fn error_leaf(&self, cx: &LeafCx<'_>, err: &EngineError) -> Option<f64> {
        if self.flags.engine_error_leaf {
            p6o::error_leaf(cx, err)
        } else {
            None
        }
    }
}

/// The math every context of `cfg` uses (`portable_math`, W-137).
pub(crate) const fn math_of(cfg: &StrengthenedConfig) -> Math {
    Math::of(cfg.mechanisms.portable_math)
}

/// Whether a package has implemented the mechanism `name`.
fn available(name: &str) -> bool {
    p6o::PROVIDES.contains(&name)
}

/// Refuses a configuration that enables a mechanism no package has implemented yet.
///
/// # Errors
/// `mechanism <name> (<package>) is not available in this build`, for the first such flag in
/// declaration order.
pub(crate) fn validate(flags: &MechanismFlags) -> Result<(), String> {
    for &(name, pkg) in MechanismFlags::FIELDS {
        if flags.get(name).is_some_and(crate::FlagValue::is_on) && !available(name) {
            return Err(format!(
                "mechanism {name} ({pkg}) is not available in this build"
            ));
        }
    }
    Ok(())
}
