//! P6-A's mechanisms (docs/port/PLAN.md §12.2, §12.3 S2): the horizon and the leaf.
//!
//! | Flag | Item | Piece |
//! | --- | --- | --- |
//! | `horizon_turns` (a configuration field the core reads) | W-01 | the per-preset horizon |
//! | `adaptive_horizon` | W-01 | one more turn boundary for an unsettled leaf (`horizon.rs`) |
//! | `generalized_leaf` | W-02 | leaf constants that cover every horizon (`leaf.rs`) |
//! | `capped_rollout_continuation` | W-15 | a capped rollout finishes its step (`capped.rs`) |
//!
//! With every P6-A flag off `Composed` calls the reference `horizon` and `leaf_value`, so the
//! null configuration replays the reference. Cost (PLAN §12.3): extended rollouts are rollout
//! applies of the line; the continuation of a capped rollout runs on a copy and its applies are
//! auxiliary work ([`DecisionState`]). Nothing here draws a random number.

pub(crate) mod capped;
pub(crate) mod horizon;
pub(crate) mod leaf;
#[cfg(test)]
mod tests;

use std::cell::Cell;

use gcg_engine::EngineError;

use super::{LeafCx, reference};
use crate::{MechanismFlags, StrengthenedConfig};
use capped::Continued;
use leaf::LeafMapping;

/// The mechanism flags this package implements.
pub(crate) const PROVIDES: [&str; 3] = [
    "adaptive_horizon",
    "capped_rollout_continuation",
    "generalized_leaf",
];

/// Whether any P6-A flag is on.
pub(crate) const fn active(f: &MechanismFlags) -> bool {
    f.adaptive_horizon || f.generalized_leaf || f.capped_rollout_continuation
}

/// Whether the leaf value is P6-A's (otherwise the reference's).
pub(crate) const fn changes_leaf(f: &MechanismFlags) -> bool {
    f.generalized_leaf || f.capped_rollout_continuation
}

/// The constructor check of the leaf constants (W-02) for `cfg`: a win at the longest line
/// `cfg` plays outranks every evaluated position, and a loss there ranks below every one.
///
/// # Errors
/// The horizon (with its extension) is too long for the constants.
pub(crate) fn check(cfg: &StrengthenedConfig) -> Result<(), String> {
    LeafMapping::of(cfg).check()
}

/// P6-A's counters for one search: the auxiliary applies of capped-rollout continuations, and
/// how many lines were extended, capped, and still unsettled after their continuation.
#[derive(Debug, Default)]
pub(crate) struct DecisionState {
    aux: Cell<u64>,
    extended: Cell<u64>,
    capped: Cell<u64>,
    unsettled: Cell<u64>,
}

/// A snapshot of [`DecisionState`]'s counters.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) struct Counts {
    pub(crate) aux_applies: u64,
    pub(crate) extended: u64,
    pub(crate) capped: u64,
    pub(crate) unsettled: u64,
}

impl DecisionState {
    pub(crate) const fn new() -> Self {
        Self {
            aux: Cell::new(0),
            extended: Cell::new(0),
            capped: Cell::new(0),
            unsettled: Cell::new(0),
        }
    }

    /// The auxiliary applies spent so far.
    pub(crate) const fn aux_applies(&self) -> u64 {
        self.aux.get()
    }

    /// The counters so far (reported in `MechanismEffort`).
    pub(crate) const fn counts(&self) -> Counts {
        Counts {
            aux_applies: self.aux.get(),
            extended: self.extended.get(),
            capped: self.capped.get(),
            unsettled: self.unsettled.get(),
        }
    }

    fn bump(cell: &Cell<u64>, by: u64) {
        cell.set(cell.get().saturating_add(by));
    }
}

/// `Mechanisms::horizon` with `adaptive_horizon` on (`horizon.rs`), counting the extensions.
pub(crate) fn horizon(state: &DecisionState, cx: &LeafCx<'_>) -> super::HorizonDecision {
    let d = horizon::decide(cx);
    if matches!(d, super::HorizonDecision::Extend { .. }) {
        DecisionState::bump(&state.extended, 1);
    }
    d
}

/// The value of `s` from the searching player's point of view under `map` (the reference's
/// `_leaf_value`, `ai/mcts.py:254-264`, with the mapping's constants in its expressions).
fn value_of(cx: &LeafCx<'_>, map: &LeafMapping) -> Result<f64, EngineError> {
    let s = cx.s;
    if let Some(w) = s.winner() {
        let turns = s.turn() - cx.root_turn;
        if i32::from(w) == i32::from(cx.player) {
            return Ok(map.win(turns));
        }
        return Ok(if i32::from(w) == 1 - i32::from(cx.player) {
            map.loss(turns)
        } else {
            0.5
        });
    }
    let z = gcg_ai::score(s, cx.player, cx.weights, cx.density)? / cx.cfg.value_scale;
    Ok(map.estimate(cx.math.logistic(z)))
}

/// `Mechanisms::leaf_value` with `generalized_leaf` or `capped_rollout_continuation` on: a
/// capped line is first continued on a copy (`capped.rs`); the value uses the configuration's
/// leaf mapping (the reference constants without `generalized_leaf`, bit for bit).
///
/// P6-A's leaf alone (its unit tests and the composition tests); the search composes the
/// packages' leaves through [`leaf_value_with`] (`Composed::leaf_value`).
///
/// # Errors
/// The evaluation's and the continuation's engine errors.
#[cfg(test)]
pub(crate) fn leaf_value(state: &DecisionState, cx: &mut LeafCx<'_>) -> Result<f64, EngineError> {
    leaf_value_with(state, cx, settled_value)
}

/// `leaf_value` with `settled` scoring the line once it needs no continuation (P6-A's own
/// [`settled_value`] alone): the point where `Composed::leaf_value` puts the other packages'
/// leaf mechanisms behind the capped-rollout continuation.
///
/// # Errors
/// The continuation's engine errors and `settled`'s.
pub(crate) fn leaf_value_with(
    state: &DecisionState,
    cx: &mut LeafCx<'_>,
    settled: impl FnOnce(&mut LeafCx<'_>) -> Result<f64, EngineError>,
) -> Result<f64, EngineError> {
    let flags = &cx.cfg.mechanisms;
    if flags.capped_rollout_continuation && cx.capped {
        DecisionState::bump(&state.capped, 1);
        let mut s = cx.s.clone();
        let continued = capped::continue_line(&mut s, cx.horizon, &cx.cfg.knobs())?;
        let (Continued::Settled { applies } | Continued::Unsettled { applies }) = continued;
        DecisionState::bump(&state.aux, u64::from(applies));
        if matches!(continued, Continued::Unsettled { .. }) {
            DecisionState::bump(&state.unsettled, 1);
            return Ok(0.5);
        }
        let mut at = LeafCx {
            s: &s,
            capped: false,
            ..*cx
        };
        return settled(&mut at);
    }
    settled(cx)
}

/// The value of a line that needs no continuation.
///
/// # Errors
/// The evaluation's engine errors.
pub(crate) fn settled_value(cx: &mut LeafCx<'_>) -> Result<f64, EngineError> {
    if cx.cfg.mechanisms.generalized_leaf {
        value_of(cx, &LeafMapping::of(cx.cfg))
    } else {
        reference::leaf_value(cx)
    }
}

/// The value of a line whose turn player `winner` has a proved win at the leaf's turn (P6-B's
/// `threat_leaf`) under the configuration's leaf mapping: the reference's time-shaped win or
/// loss without `generalized_leaf`, bit for bit.
pub(crate) fn proved_value(cx: &LeafCx<'_>, winner: u8) -> f64 {
    let map = LeafMapping::of(cx.cfg);
    let turns = cx.s.turn() - cx.root_turn;
    if winner == cx.player {
        map.win(turns)
    } else {
        map.loss(turns)
    }
}

/// The value of an unfinished line whose evaluation is `z = score / value_scale` under the
/// configuration's leaf mapping (`LEAF_FLOOR + (1 - 2 LEAF_FLOOR) σ(z)` either way: the
/// generalised mapping keeps the reference floor).
pub(crate) fn estimate_value(cx: &LeafCx<'_>, z: f64) -> f64 {
    LeafMapping::of(cx.cfg).estimate(cx.math.logistic(z))
}

/// The loss value of a line that ended in an engine error (`engine_error_leaf`, W-143) under
/// the generalised mapping, at the turn the line reached.
pub(crate) fn loss_value(cx: &LeafCx<'_>) -> f64 {
    LeafMapping::of(cx.cfg).loss(cx.s.turn() - cx.root_turn)
}
