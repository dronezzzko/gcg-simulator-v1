//! P6-G's mechanisms (docs/dev/PLAN.md §12.2, §12.3 S10): the strengthened leaf evaluation
//! (W-34, W-35, W-36, W-37, W-39, W-142).
//!
//! With a model flag on (`refit_weights`, `determinized_fit`, `no_tempo_feature`,
//! `keyword_features`, `ability_count_features`, `hand_slots`, `burst_shield_probability`,
//! `phase_weights`), the leaf scores the superset features of the leaf world (`features.rs`)
//! with the embedded model those flags name (`model.rs`, `weights.txt`), fitted by
//! `gcg-eval tune-leaf` on Rust self-play of the strengthened search at [`LEAF_POINT`]. With
//! `leaf_density` the Burst density is the searcher's view of the leaf world instead of the
//! root's (W-142); alone it changes only the density of the configuration's weights. Terminal
//! leaves, the leaf floor and the logistic are the reference's; the logistic is the search's
//! math (`portable_math`, W-137), and the model's sums run in column order in f64.
//!
//! [`api`] is the shared code path of the tuning tool and the search: the recorder calls the
//! same feature and density functions as the leaf (W-142), at the same point (W-39).

pub(crate) mod features;
pub(crate) mod model;
#[cfg(test)]
mod tests;

use gcg_ai::{Density, burst_density, score};
use gcg_engine::{EngineError, GameState};

use super::LeafCx;
#[cfg(test)]
use super::reference::{self, LEAF_FLOOR};
use crate::MechanismFlags;
use model::ModelSpec;

/// The mechanism flags this package implements.
pub(crate) const PROVIDES: [&str; 9] = [
    "ability_count_features",
    "burst_shield_probability",
    "determinized_fit",
    "hand_slots",
    "keyword_features",
    "leaf_density",
    "no_tempo_feature",
    "phase_weights",
    "refit_weights",
];

/// Whether any P6-G flag is on (otherwise the leaf is the reference's).
pub(crate) const fn active(flags: &MechanismFlags) -> bool {
    flags.leaf_density || has_model(flags)
}

/// Whether a model flag is on (the leaf scores a fitted model, not the configuration's weights).
pub(crate) const fn has_model(flags: &MechanismFlags) -> bool {
    ModelSpec::of(flags).is_some()
}

/// The Burst density the leaf's evaluation uses: the searcher's view of the leaf world with
/// `leaf_density`, else the root's (`cx.density`).
pub(crate) fn density(flags: &MechanismFlags, cx: &LeafCx<'_>) -> Density {
    if flags.leaf_density {
        burst_density(cx.s, cx.player)
    } else {
        cx.density
    }
}

/// Refuses a flag combination whose model the embedded table does not have.
///
/// # Errors
/// `P6-G has no fitted model <key> in this build`, or a malformed embedded table.
pub(crate) fn validate(flags: &MechanismFlags) -> Result<(), String> {
    match ModelSpec::of(flags) {
        Some(spec) => model::embedded(&spec).map(|_| ()),
        None => Ok(()),
    }
}

/// The leaf's log-odds for the searching player: the model of the flags on the leaf world's
/// superset features (negated when the searcher is not the turn player), or the reference's
/// `score` with the configuration's weights; the density is the leaf world's with
/// `leaf_density`, else the root's.
///
/// # Errors
/// The derived view's error, a card in hand without a hand slot, or a missing model (refused
/// beforehand by [`validate`]).
pub(crate) fn leaf_score(flags: &MechanismFlags, cx: &LeafCx<'_>) -> Result<f64, EngineError> {
    leaf_score_with(flags, cx, |_, _| Ok(()))
}

/// [`leaf_score`] with `adjust` changing a model's superset features of the leaf world before
/// the model scores them (`Composed::leaf_value` passes P6-B's feature swaps); without a model
/// flag `adjust` is not called.
///
/// # Errors
/// As [`leaf_score`], and `adjust`'s.
pub(crate) fn leaf_score_with(
    flags: &MechanismFlags,
    cx: &LeafCx<'_>,
    adjust: impl FnOnce(&GameState, &mut [f64]) -> Result<(), EngineError>,
) -> Result<f64, EngineError> {
    let s = cx.s;
    let Some(spec) = ModelSpec::of(flags) else {
        return score(s, cx.player, cx.weights, density(flags, cx));
    };
    let m = model::embedded(&spec).map_err(EngineError::Assertion)?;
    let mut x = if flags.leaf_density {
        api::leaf_features(s, cx.player)?
    } else {
        features::superset(s, cx.density)?
    };
    adjust(s, &mut x)?;
    let z = m.score(&x, s.turn());
    Ok(if cx.player == s.active() { z } else { -z })
}

/// `_leaf_value` (`ai/mcts.py:254-264`) with [`leaf_score`] in place of `score`: P6-G's leaf
/// alone (its unit tests and the composition tests); the search composes [`leaf_score_with`]
/// with the other packages' leaves in `Composed::leaf_value`.
///
/// # Errors
/// As [`leaf_score`].
#[cfg(test)]
pub(crate) fn leaf_value(flags: &MechanismFlags, cx: &mut LeafCx<'_>) -> Result<f64, EngineError> {
    if cx.s.winner().is_some() {
        return reference::leaf_value(cx);
    }
    let z = leaf_score(flags, cx)? / cx.cfg.value_scale;
    Ok(LEAF_FLOOR + (1.0 - 2.0 * LEAF_FLOOR) * cx.math.logistic(z))
}

/// Where the search scores a line that reached its horizon and where the tuning tool records:
/// the first pending decision of a turn, after its start, draw and resource phases. The
/// search's rollout stops at the first state whose turn reached the horizon turn
/// (`play_until_turn`, `ai/playout.py:22-26`), which is that state (W-39).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LeafPoint {
    /// The first pending decision of a turn.
    TurnStart,
}

/// The leaf point of the search and the recorder (one constant for both, W-39).
pub const LEAF_POINT: LeafPoint = LeafPoint::TurnStart;

impl LeafPoint {
    /// The name the tuning tool reports.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TurnStart => "first pending decision of a turn",
        }
    }

    /// Whether `st`, the state after a decision taken in `previous_turn` (`None` before the
    /// game's first decision), is at the leaf point: the game is not over, a decision is
    /// pending, and the turn advanced.
    #[must_use]
    pub fn holds(self, previous_turn: Option<i32>, st: &GameState) -> bool {
        match self {
            Self::TurnStart => {
                st.winner().is_none()
                    && st.pending().is_some()
                    && previous_turn.is_none_or(|t| st.turn() > t)
            }
        }
    }
}

/// The shared code path of the tuning tool and the search (W-39, W-142).
pub mod api {
    use gcg_ai::burst_density;
    use gcg_engine::observe::determinize;
    use gcg_engine::{EngineError, GameState};

    pub use super::features::{N_FEATURES, NAMES, hand_slot, kw_feature};
    pub use super::model::{Model, ModelSpec, embedded_models, parse_table, table_text};
    pub use super::{LEAF_POINT, LeafPoint};
    use crate::Observation;

    /// The leaf's superset features of the world `world` for the searching player `searcher`,
    /// with the Burst density of `searcher`'s view of that world (the `leaf_density` path).
    ///
    /// # Errors
    /// The derived view's error, or a card in hand without a hand slot.
    pub fn leaf_features(world: &GameState, searcher: u8) -> Result<Vec<f64>, EngineError> {
        super::features::superset(world, burst_density(world, searcher))
    }

    /// The determinised recording of a position (W-142): the world
    /// `determinize(information set, observer, seed)` of the observation, scored as the leaf
    /// scores a world of that observer ([`leaf_features`]).
    ///
    /// # Errors
    /// `determinize`'s error, or as [`leaf_features`].
    pub fn determinized_features(
        obs: &Observation<'_>,
        seed: u64,
    ) -> Result<Vec<f64>, EngineError> {
        let world = determinize(obs.info(), obs.player(), seed)?;
        leaf_features(&world, obs.player())
    }

    /// The true-state recording of a position (the reference tuning's convention,
    /// `ai/tuning.py:128-136`): the superset features of `st` with the turn player's Burst
    /// density.
    ///
    /// # Errors
    /// As [`leaf_features`].
    pub fn true_state_features(st: &GameState) -> Result<Vec<f64>, EngineError> {
        super::features::superset(st, burst_density(st, st.active()))
    }
}
