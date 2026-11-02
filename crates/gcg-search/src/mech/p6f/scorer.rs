//! The strengthened option scores (PLAN §12.3 S9): the reference policy's scores
//! (`gcg_ai::option_scores`, `ai/policy.py:104-108`) with each P6-F flag replacing the scorer
//! of the decisions it covers, then the fitted offsets of the calibrated model.

use gcg_ai::{PolicyKnobs, option_scores};
use gcg_engine::{Action, ActionKind, CtxKey, DecisionKind, DecisionView, EngineError, GameState};

use super::lookahead::{Until, relative_scores, value_after, viewers_next_turn};
use super::{DecisionState, bursts, calibrated, fight, order, targets};
use crate::{MechanismFlags, StrengthenedConfig};

/// The look-ahead's score per unit of evaluation log-odds: one card in hand (about 0.55 in
/// `ROLE_WEIGHTS`) is worth about 2.2 points, so a rollout at temperature 0.5 prefers it about
/// 80 to 1.
pub(crate) const LOOKAHEAD_SCALE: f64 = 4.0;
/// The largest score distance the look-ahead gives (twice the presets' Burst score).
pub(crate) const LOOKAHEAD_CAP: f64 = 8.0;

/// One scoring of a decision's options.
pub(crate) struct Scoring<'a> {
    pub(crate) flags: &'a MechanismFlags,
    pub(crate) state: &'a DecisionState,
    pub(crate) cfg: &'a StrengthenedConfig,
    pub(crate) knobs: PolicyKnobs,
    pub(crate) model: calibrated::Model,
}

impl Scoring<'_> {
    /// One score per option of `dec` (in `options`' order).
    ///
    /// # Errors
    /// The reference scorer's errors and the engine's errors inside a look-ahead.
    pub(crate) fn scores(
        &self,
        s: &GameState,
        dec: &DecisionView<'_>,
        options: &[Action],
    ) -> Result<Vec<f64>, EngineError> {
        let f = self.flags;
        let mut out = match dec.hdr.kind {
            DecisionKind::Main if f.exact_fight => fight::main_scores(self, s, dec, options)?,
            DecisionKind::Block if f.exact_fight => fight::block_scores(self, s, dec, options)?,
            DecisionKind::Select if dec.hdr.ctx(CtxKey::Mode) == 1 => {
                if f.modal_lookahead {
                    self.by_lookahead(s, dec, options, Until::Effect)?
                } else {
                    option_scores(s, dec, options, &self.knobs)?
                }
            }
            DecisionKind::Select if f.exhaustive_polarity || f.kill_aware_targeting => {
                targets::select_scores(self, s, dec, options)?
            }
            DecisionKind::YesNo if f.yes_no_lookahead => self.yes_no(s, dec, options)?,
            DecisionKind::Burst if f.burst_decline_list => self.burst(s, dec, options)?,
            DecisionKind::OrderTrigger if f.order_trigger_scorer => {
                order::scores(self, s, dec, options)?
            }
            DecisionKind::ActionStep if f.end_phase_actions && s.battles().is_empty() => {
                self.end_phase(s, dec, options)?
            }
            _ => option_scores(s, dec, options, &self.knobs)?,
        };
        calibrated::adjust(self.model, &mut out, s, dec, options);
        Ok(out)
    }

    /// Every option resolved on a clone and scored by its value relative to the best; the
    /// reference scores when the decision's look-ahead budget is spent.
    pub(crate) fn by_lookahead(
        &self,
        s: &GameState,
        dec: &DecisionView<'_>,
        options: &[Action],
        until: Until,
    ) -> Result<Vec<f64>, EngineError> {
        let viewer = dec.hdr.player;
        let mut values = Vec::with_capacity(options.len());
        for &o in options {
            match value_after(self, s, o, viewer, until)? {
                Some(v) => values.push(v),
                None => return option_scores(s, dec, options, &self.knobs),
            }
        }
        Ok(relative_scores(&values))
    }

    /// The value difference of answering YES rather than NO, or `None` without budget or
    /// without both answers among `options`.
    fn yes_minus_no(
        &self,
        s: &GameState,
        dec: &DecisionView<'_>,
        options: &[Action],
    ) -> Result<Option<f64>, EngineError> {
        let find = |k: ActionKind| options.iter().copied().find(|o| o.kind == k);
        let (Some(yes), Some(no)) = (find(ActionKind::Yes), find(ActionKind::No)) else {
            return Ok(None);
        };
        let viewer = dec.hdr.player;
        let Some(y) = value_after(self, s, yes, viewer, Until::Effect)? else {
            return Ok(None);
        };
        let Some(n) = value_after(self, s, no, viewer, Until::Effect)? else {
            return Ok(None);
        };
        Ok(Some(y - n))
    }

    /// W-27: a "you may" whose body costs nothing and harms none of its controller's cards is
    /// accepted with the Burst score unless the look-ahead shows a loss; any other is accepted
    /// only as far as the look-ahead says it pays.
    fn yes_no(
        &self,
        s: &GameState,
        dec: &DecisionView<'_>,
        options: &[Action],
    ) -> Result<Vec<f64>, EngineError> {
        let Some(delta) = self.yes_minus_no(s, dec, options)? else {
            return option_scores(s, dec, options, &self.knobs);
        };
        let free = bursts::waiting_may_review(s).is_none_or(|r| r.is_clean());
        let yes = if free && delta >= 0.0 {
            at_least(self.knobs.burst_yes_score, scaled(delta))
        } else {
            scaled(delta)
        };
        Ok(per_option(options, yes))
    }

    /// W-163: a Burst on the review list (`bursts::review_program`) is answered by the
    /// look-ahead: accepted with the Burst score unless the look-ahead shows a loss, declined
    /// as far as it loses; every other Burst keeps the high acceptance score.
    fn burst(
        &self,
        s: &GameState,
        dec: &DecisionView<'_>,
        options: &[Action],
    ) -> Result<Vec<f64>, EngineError> {
        let listed = s
            .frames()
            .last()
            .is_some_and(|f| !bursts::review_program(s.registry(), f.program).is_clean());
        if !listed {
            return option_scores(s, dec, options, &self.knobs);
        }
        let Some(delta) = self.yes_minus_no(s, dec, options)? else {
            return option_scores(s, dec, options, &self.knobs);
        };
        let yes = if delta >= 0.0 {
            at_least(self.knobs.burst_yes_score, scaled(delta))
        } else {
            scaled(delta)
        };
        Ok(per_option(options, yes))
    }

    /// W-28: in the end phase's action step, a play whose effect outlasts the turn is scored
    /// by its value against passing, both lines played on with the reference policy's best
    /// options through the deciding player's next turn (so that what the play enables there,
    /// such as an attack past a Blocker it rested, is played out: the evaluation alone credits a
    /// removal Command still in hand as an answer to a Blocker, whatever its timing); other
    /// plays keep the reference's pass preference.
    fn end_phase(
        &self,
        s: &GameState,
        dec: &DecisionView<'_>,
        options: &[Action],
    ) -> Result<Vec<f64>, EngineError> {
        let mut out = option_scores(s, dec, options, &self.knobs)?;
        let lasting: Vec<usize> = (0..options.len())
            .filter(|&i| bursts::play_outlasts_turn(s, options[i]))
            .collect();
        let Some(pass) = options.iter().position(|o| o.kind == ActionKind::Pass) else {
            return Ok(out);
        };
        if lasting.is_empty() {
            return Ok(out);
        }
        let viewer = dec.hdr.player;
        let until = Until::ThroughTurn(viewers_next_turn(s.turn(), s.active(), viewer));
        let Some(base) = value_after(self, s, options[pass], viewer, until)? else {
            return Ok(out);
        };
        for i in lasting {
            let Some(v) = value_after(self, s, options[i], viewer, until)? else {
                return Ok(out);
            };
            out[i] = out[pass] + scaled(v - base);
        }
        Ok(out)
    }
}

/// `LOOKAHEAD_SCALE * delta`, clamped to `±LOOKAHEAD_CAP`.
pub(crate) fn scaled(delta: f64) -> f64 {
    (LOOKAHEAD_SCALE * delta).clamp(-LOOKAHEAD_CAP, LOOKAHEAD_CAP)
}

/// `max(floor, x)` with Python's first-extreme rule.
fn at_least(floor: f64, x: f64) -> f64 {
    if x > floor { x } else { floor }
}

/// YES gets `yes`, every other option 0.
fn per_option(options: &[Action], yes: f64) -> Vec<f64> {
    options
        .iter()
        .map(|o| if o.kind == ActionKind::Yes { yes } else { 0.0 })
        .collect()
}
