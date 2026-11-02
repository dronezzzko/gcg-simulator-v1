//! The one-ply look-ahead of the strengthened policy (W-24, W-27, W-28, W-29, W-163): an
//! option is applied on a clone of the rollout world, the game continues with the reference
//! policy's best option (no random draws) until the option's effect has resolved, and the
//! result is scored by the search's evaluation from the deciding player's point of view.

use gcg_ai::{apply_unchecked, choose, score};
use gcg_engine::{Action, DecisionKind, EngineError, GameState};

use super::scorer::{LOOKAHEAD_CAP, LOOKAHEAD_SCALE, Scoring};
use crate::mech::reference::py_max;

/// The most applies one look-ahead follows after its option ([`Until::Effect`]).
pub(crate) const MAX_RESOLVE_APPLIES: u32 = 64;
/// The most applies a look-ahead through a whole turn follows ([`Until::ThroughTurn`]).
pub(crate) const MAX_TURN_APPLIES: u32 = 160;
/// The value of a won game in evaluation log-odds (a lost one is its negative): far above any
/// evaluation of an unfinished game.
pub(crate) const WIN_VALUE: f64 = 50.0;

/// Where a look-ahead stops.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Until {
    /// The effect has resolved: the next decision belongs to the turn structure (MAIN,
    /// ACTION_STEP, BLOCK or a pre-game choice).
    Effect,
    /// The start of the turn after turn `n`: the game is played on through turn `n` (the end-
    /// phase plays of W-28 follow the viewer's next turn, so that what a play enables there,
    /// such as an attack past a rested Blocker, is played out rather than estimated).
    ThroughTurn(i32),
}

impl Until {
    fn stops_at(self, t: &GameState, kind: DecisionKind) -> bool {
        match self {
            Self::Effect => matches!(
                kind,
                DecisionKind::Main
                    | DecisionKind::ActionStep
                    | DecisionKind::Block
                    | DecisionKind::ChooseFirst
                    | DecisionKind::Redraw
            ),
            Self::ThroughTurn(n) => t.turn() > n,
        }
    }

    const fn max_applies(self) -> u32 {
        match self {
            Self::Effect => MAX_RESOLVE_APPLIES,
            Self::ThroughTurn(_) => MAX_TURN_APPLIES,
        }
    }
}

/// The viewer's next own turn, counted from a state at turn `turn` whose turn player is
/// `active` (the current turn when it is not the viewer's).
pub(crate) const fn viewers_next_turn(turn: i32, active: u8, viewer: u8) -> i32 {
    if active == viewer { turn + 2 } else { turn + 1 }
}

/// The value for `viewer` of playing `option` in `s` and following the game until `until`;
/// `None` once the decision's auxiliary budget is spent. Every apply is counted.
///
/// # Errors
/// The engine's errors on the clone and the reference policy's.
pub(crate) fn value_after(
    sc: &Scoring<'_>,
    s: &GameState,
    option: Action,
    viewer: u8,
    until: Until,
) -> Result<Option<f64>, EngineError> {
    if !sc.state.has_budget() {
        return Ok(None);
    }
    let mut t = s.clone();
    let applies = follow(sc, &mut t, option, until);
    sc.state.spend(u64::from(applies?));
    value_of(sc, &t, viewer).map(Some)
}

/// Applies `option` and follows the game in place with the reference policy's best option;
/// returns the applies made.
fn follow(
    sc: &Scoring<'_>,
    t: &mut GameState,
    option: Action,
    until: Until,
) -> Result<u32, EngineError> {
    apply_unchecked(t, option)?;
    let mut applies = 1;
    while applies <= until.max_applies() && t.winner().is_none() {
        let Some(dec) = t.pending() else { break };
        if until.stops_at(t, dec.hdr.kind) {
            break;
        }
        let a = choose(t, None, 0.0, &sc.knobs)?;
        apply_unchecked(t, a)?;
        applies += 1;
    }
    Ok(applies)
}

/// The evaluation of `t` for `viewer` in log-odds (`score / value_scale`), or `±WIN_VALUE`
/// for a finished game (0 for a draw).
pub(crate) fn value_of(sc: &Scoring<'_>, t: &GameState, viewer: u8) -> Result<f64, EngineError> {
    if let Some(w) = t.winner() {
        return Ok(if i32::from(w) == i32::from(viewer) {
            WIN_VALUE
        } else if i32::from(w) == 1 - i32::from(viewer) {
            -WIN_VALUE
        } else {
            0.0
        });
    }
    let density = sc.state.density(t, viewer);
    Ok(score(t, viewer, &sc.cfg.weights, density)? / sc.cfg.value_scale)
}

/// Scores from values: `LOOKAHEAD_SCALE * (v - max)`, at least `-LOOKAHEAD_CAP`; the best
/// option scores 0.
pub(crate) fn relative_scores(values: &[f64]) -> Vec<f64> {
    if values.is_empty() {
        return Vec::new();
    }
    let top = py_max(values);
    values
        .iter()
        .map(|&v| {
            let x = LOOKAHEAD_SCALE * (v - top);
            if x < -LOOKAHEAD_CAP {
                -LOOKAHEAD_CAP
            } else {
                x
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_scores_put_the_best_at_zero_and_floor_the_rest() {
        assert_eq!(
            relative_scores(&[0.5, 0.25, -10.0]),
            [0.0, -1.0, -LOOKAHEAD_CAP]
        );
        assert_eq!(viewers_next_turn(3, 0, 0), 5);
        assert_eq!(viewers_next_turn(4, 1, 0), 5);
    }
}
