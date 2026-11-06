//! P6-F's mechanisms (docs/dev/PLAN.md §12.2, §12.3 S9): the strengthened rollout policy. The
//! search asks it for every rollout move ([`rollout_choice`]) and for the priors of every new
//! tree node ([`priors`]), where the reference asks `gcg_ai`'s hand-written policy
//! (`ai/policy.py`). Each flag changes one piece; with every P6-F flag off the reference policy
//! runs unchanged ([`active`] is false and `Composed` calls the reference functions).
//!
//! | Flag | Item | Piece |
//! | --- | --- | --- |
//! | `calibrated_policy` | W-23 | fitted score offsets (`calibrated.rs`, fit in `fit.rs`) |
//! | `modal_lookahead` | W-24 | modes scored by resolving each on a clone (`lookahead.rs`) |
//! | `opponent_knobs` | W-25 | the opponent's rollout moves by its own fitted model |
//! | `kill_aware_targeting` | W-26 | damage targets that the damage destroys preferred |
//! | `yes_no_lookahead` | W-27 | YES_NO by look-ahead; a free beneficial "you may" accepted |
//! | `end_phase_actions` | W-28 | end-phase action-step plays scored by look-ahead |
//! | `order_trigger_scorer` | W-29 | ORDER_TRIGGER by look-ahead (≤ 6 orders) or a heuristic |
//! | `exhaustive_polarity` | W-30 | an exhaustive target-polarity table (`polarity.rs`) |
//! | `exact_fight` | W-31 | fights resolved by the engine on clones (`fight.rs`) |
//! | `burst_decline_list` | W-163 | Bursts on the reviewed list by look-ahead (`bursts.rs`) |
//!
//! Cost (PLAN §12.3): every engine apply of a look-ahead or an exact fight is counted in
//! [`DecisionState`] and reported as the decision's `applies_aux`; once a decision has spent
//! [`AUX_APPLY_CAP`] of them, the policy falls back to the reference scores for the rest of
//! that decision. Everything is deterministic: look-aheads make no random draws (nested
//! choices take the reference policy's best option) and the cap is a count.

pub(crate) mod bursts;
pub(crate) mod calibrated;
pub(crate) mod fight;
#[cfg(test)]
pub(crate) mod fit;
#[cfg(all(test, feature = "slow"))]
mod fit_selfplay;
mod fitted;
pub(crate) mod lookahead;
pub(crate) mod order;
pub(crate) mod polarity;
pub(crate) mod scorer;
pub(crate) mod targets;
#[cfg(test)]
mod tests;

use std::cell::Cell;

use gcg_ai::{Density, burst_density};
use gcg_core::rng::SplitMix64;
use gcg_engine::{Action, EngineError, GameState};

use super::p6o::math::Math;
use super::{NodeCx, RolloutCx};
use crate::MechanismFlags;
use crate::mech::reference::py_max;
use scorer::Scoring;

/// The mechanism flags this package implements.
pub(crate) const PROVIDES: [&str; 10] = [
    "burst_decline_list",
    "calibrated_policy",
    "end_phase_actions",
    "exact_fight",
    "exhaustive_polarity",
    "kill_aware_targeting",
    "modal_lookahead",
    "opponent_knobs",
    "order_trigger_scorer",
    "yes_no_lookahead",
];

/// The most engine applies the policy's look-aheads and exact fights may spend in one search
/// decision; past it the reference scores are used (PLAN §12.3: aux work capped per decision).
pub(crate) const AUX_APPLY_CAP: u64 = 8192;

/// Whether any P6-F flag is on.
pub(crate) const fn active(f: &MechanismFlags) -> bool {
    f.calibrated_policy
        || f.modal_lookahead
        || f.yes_no_lookahead
        || f.end_phase_actions
        || f.burst_decline_list
        || f.order_trigger_scorer
        || f.exhaustive_polarity
        || f.exact_fight
        || f.opponent_knobs
        || f.kill_aware_targeting
}

/// The policy's state for one search decision: the auxiliary applies spent so far and the
/// evaluation's Burst density per viewer, fixed at its first use so that every look-ahead of
/// the decision compares values on the same scale.
pub(crate) struct DecisionState {
    aux: Cell<u64>,
    density: [Cell<Option<Density>>; 2],
}

impl DecisionState {
    pub(crate) const fn new() -> Self {
        Self {
            aux: Cell::new(0),
            density: [Cell::new(None), Cell::new(None)],
        }
    }

    /// The auxiliary applies spent in this decision.
    pub(crate) const fn aux_applies(&self) -> u64 {
        self.aux.get()
    }

    /// Whether the decision may still spend auxiliary applies.
    pub(crate) fn has_budget(&self) -> bool {
        self.aux.get() < AUX_APPLY_CAP
    }

    pub(crate) fn spend(&self, applies: u64) {
        self.aux.set(self.aux.get().saturating_add(applies));
    }

    /// `burst_density(s, viewer)` of the first state this decision evaluates for `viewer`.
    pub(crate) fn density(&self, s: &GameState, viewer: u8) -> Density {
        let cell = &self.density[usize::from(viewer & 1)];
        if let Some(d) = cell.get() {
            return d;
        }
        let d = burst_density(s, viewer);
        cell.set(Some(d));
        d
    }
}

/// The Gumbel-max pick of `gcg_ai::pick` (`ai/policy.py:86-101`) with the search's math: one
/// `random()` per option in order, `0.0` read as `1e-300`, the first strictly greater value
/// wins; the first maximal score without a generator or at `temperature <= 0`. With
/// `Math::Platform` it is `gcg_ai::pick` bit for bit.
pub(crate) fn pick(math: Math, scores: &[f64], rng: &mut SplitMix64, temperature: f64) -> usize {
    if temperature <= 0.0 {
        let mut best = 0;
        for (i, &s) in scores.iter().enumerate().skip(1) {
            if s > scores[best] {
                best = i;
            }
        }
        return best;
    }
    let mut best_i = 0;
    let mut best_v = f64::NEG_INFINITY;
    for (i, &x) in scores.iter().enumerate() {
        let mut u = rng.random();
        if u == 0.0 {
            u = 1e-300;
        }
        let v = x / temperature - math.ln(-math.ln(u));
        if v > best_v {
            best_i = i;
            best_v = v;
        }
    }
    best_i
}

/// One rollout move: the strengthened scores of the pending decision's options, sampled at the
/// rollout temperature; the opponent's moves by its own fitted model (`opponent_knobs`).
pub(crate) fn rollout_choice(
    state: &DecisionState,
    flags: &MechanismFlags,
    cx: &mut RolloutCx<'_>,
) -> Result<Action, EngineError> {
    let s = cx.s;
    let Some(dec) = s.pending() else {
        return Err(EngineError::Assertion(String::new()));
    };
    if dec.options.len() == 1 {
        return Ok(dec.options[0]);
    }
    let opponent = flags.opponent_knobs && dec.hdr.player != cx.player;
    let model = if opponent || flags.calibrated_policy {
        calibrated::Model::Own
    } else {
        calibrated::Model::None
    };
    let scoring = Scoring {
        flags,
        state,
        cfg: cx.cfg,
        knobs: cx.knobs,
        model,
    };
    let scores = scoring.scores(s, &dec, dec.options)?;
    if dec.options.is_empty() {
        return Err(EngineError::Value(
            "max() iterable argument is empty".into(),
        ));
    }
    let temperature = if opponent {
        calibrated::opponent_temperature(cx.cfg.rollout_temperature)
    } else {
        cx.cfg.rollout_temperature
    };
    Ok(dec.options[pick(cx.math, &scores, cx.rng, temperature)])
}

/// `_add_children` (`ai/mcts.py:305-317`, `reference::priors`) over the strengthened scores
/// of the node's mover (its own model: priors describe the mover's good moves).
pub(crate) fn priors(
    state: &DecisionState,
    flags: &MechanismFlags,
    cx: &mut NodeCx<'_>,
) -> Result<(), EngineError> {
    let scores = node_scores(state, flags, cx)?;
    let top = py_max(&scores);
    let weights: Vec<f64> = scores
        .iter()
        .map(|&x| cx.math.exp((x - top) / cx.cfg.prior_temperature))
        .collect();
    let total = gcg_core::pymath::py_sum(weights.iter().copied());
    for (&ck, &wgt) in cx.keys.iter().zip(&weights) {
        cx.tree
            .setdefault(cx.node, ck, cx.dec.hdr.player, wgt / total);
    }
    Ok(())
}

/// The strengthened scores of a tree node's moves (in `cx.moves`' order) by the node's mover's
/// own model: the input of [`priors`], and of the other packages' priors when a P6-F flag is on.
///
/// # Errors
/// The scorer's errors.
pub(crate) fn node_scores(
    state: &DecisionState,
    flags: &MechanismFlags,
    cx: &NodeCx<'_>,
) -> Result<Vec<f64>, EngineError> {
    let options: Vec<Action> = cx.moves.iter().map(|&(_, a)| a).collect();
    let scoring = Scoring {
        flags,
        state,
        cfg: cx.cfg,
        knobs: cx.knobs,
        model: if flags.calibrated_policy {
            calibrated::Model::Own
        } else {
            calibrated::Model::None
        },
    };
    scoring.scores(cx.s, cx.dec, &options)
}
