//! `in_tree_filters` (W-11 part 1): the cheap, honest root filters applied for the mover at
//! every tree node, so that the tree below the root does not split its visits between moves the
//! root would have merged or dropped.
//!
//! The filters are those of `prune_dominated` without the trial-world checks
//! (`ai/actions.py:242-277` with `strict_attacks=False, rest_targets=False`): a free 【Burst】
//! (one that only adds its own card to the hand) is accepted, and a MAIN attack by a Unit whose
//! attack cannot matter (AP 0 or less, no attack trigger for its controller, no action-step
//! play, `_attack_can_matter`) is dropped; then `merge_equivalent` (`ai/actions.py:280-346`)
//! keeps one move per public outcome. `without_effect` stays a root-only filter: it plays every
//! spending move to the next turn and is too costly per node (W-11 plan). Each filter reads only
//! the world state at the node from the mover's side (its own cards and the public zones), so it
//! uses no information the mover lacks. None applies the engine, so the filters cost no
//! applies.

use gcg_ai::{MoveKey, PruneFlags, merge_equivalent, prune_dominated};
use gcg_engine::{Action, DecisionView, EngineError, GameState};

/// The cheap prune flags: no trial determinisation, no trial rests.
const CHEAP: PruneFlags = PruneFlags {
    strict_attacks: false,
    rest_targets: false,
};

/// The moves of the decision `dec` of the world state `s` after the cheap filters, in order;
/// at least one move always remains (`prune_dominated` keeps every move when it would drop all,
/// and `merge_equivalent` keeps the first of each class).
///
/// # Errors
/// The engine's error from the derived view.
pub(crate) fn filter(
    s: &GameState,
    dec: &DecisionView<'_>,
    moves: Vec<(MoveKey, Action)>,
) -> Result<Vec<(MoveKey, Action)>, EngineError> {
    let moves = prune_dominated(s, dec, moves, CHEAP)?;
    merge_equivalent(s, dec, moves)
}
