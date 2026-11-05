//! The calibrated policy (`calibrated_policy`, W-23) and the opponent's own model
//! (`opponent_knobs`, W-25).
//!
//! The reference's scores are hand-set (`ai/policy.py:42-47`, `:175-183`, `:288-298`); measured,
//! its rollouts cast Commands less and activate abilities more than the search does
//! (`docs/python/AI.md:453-461`). The calibrated policy keeps every reference score and adds a handful
//! of fitted offsets ([`OFFSETS`]): the PLAY_COMMAND and ACTIVATE bases, <Support>, the
//! action-step plays in a battle and in the end phase, and the EX Resource penalty.
//!
//! The offsets are a conditional logit fitted by `fit.rs` (fixed-order IRLS in f64) to the
//! search's own choices in Rust self-play on tuning seeds (`fitted.rs`,
//! `docs/dev/eval/P6-F/policy-fit.json`): `logit = score / T + offsets · x`, the reference
//! score entering at the rollout temperature `T = 0.5` of the fit, so the fitted offsets are
//! moment conditions: at the fit's decisions the model's expected share of each offset's moves
//! equals the search's (W-23's frequency matching).
//!
//! **The opponent's model** (`opponent_knobs`, W-25's "competent opponent"): the opponent's
//! rollout moves take the fitted offsets (both seats of the fit are the search, so its choices
//! are the opponent's too) at [`OPPONENT_TEMPERATURE_SCALE`] times the rollout temperature. A
//! purely predictive fit of the opponent's temperature (the coefficient of the reference score
//! fitted with the offsets, `opponent_predictive` in the fit report) gives about 3.2: a
//! near-uniform policy that predicts the search's choices best but would model the opponent as
//! playing almost at random, so it is recorded and not used.

use gcg_ai::actions::ex_used;
use gcg_engine::{Action, ActionKind, DecisionKind, DecisionView, GameState, SUPPORT_AID};

use super::fitted;

/// The offsets, in feature order.
pub(crate) const OFFSETS: [&str; 6] = [
    "main_command",
    "main_activate",
    "main_support",
    "battle_play",
    "end_phase_play",
    "ex_resource",
];
/// The number of offsets.
pub(crate) const N: usize = OFFSETS.len();

/// The opponent model's temperature as a share of the rollout temperature.
pub(crate) const OPPONENT_TEMPERATURE_SCALE: f64 = 0.25;

/// Which fitted offsets a scoring adds.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Model {
    /// The reference scores.
    None,
    /// The calibrated policy (also the opponent's model).
    Own,
}

/// The offset features of option `o` of a decision of `kind` (in or out of a battle).
#[must_use]
pub(crate) fn features(kind: DecisionKind, in_battle: bool, o: Action) -> [f64; N] {
    let mut x = [0.0; N];
    let playing = matches!(o.kind, ActionKind::PlayCommand | ActionKind::Activate);
    match kind {
        DecisionKind::Main => match o.kind {
            ActionKind::PlayCommand => x[0] = 1.0,
            ActionKind::Activate if o.b == SUPPORT_AID => x[2] = 1.0,
            ActionKind::Activate => x[1] = 1.0,
            _ => {}
        },
        DecisionKind::ActionStep if playing => x[if in_battle { 3 } else { 4 }] = 1.0,
        _ => {}
    }
    if matches!(kind, DecisionKind::Main | DecisionKind::ActionStep) {
        x[5] = f64::from(ex_used(o));
    }
    x
}

/// The offsets of `model` in score units.
#[must_use]
pub(crate) const fn offsets(model: Model) -> Option<&'static [f64; N]> {
    match model {
        Model::None => None,
        Model::Own => Some(&fitted::OWN_OFFSETS),
    }
}

/// The opponent model's rollout temperature.
#[must_use]
pub(crate) fn opponent_temperature(rollout_temperature: f64) -> f64 {
    OPPONENT_TEMPERATURE_SCALE * rollout_temperature
}

/// Adds `model`'s offsets to `scores` (one per option of `dec`).
pub(crate) fn adjust(
    model: Model,
    scores: &mut [f64],
    s: &GameState,
    dec: &DecisionView<'_>,
    options: &[Action],
) {
    let Some(theta) = offsets(model) else {
        return;
    };
    let kind = dec.hdr.kind;
    if !matches!(kind, DecisionKind::Main | DecisionKind::ActionStep) {
        return;
    }
    let in_battle = !s.battles().is_empty();
    for (score, &o) in scores.iter_mut().zip(options) {
        let x = features(kind, in_battle, o);
        for (t, xi) in theta.iter().zip(x) {
            if xi != 0.0 {
                *score += t * xi;
            }
        }
    }
}
