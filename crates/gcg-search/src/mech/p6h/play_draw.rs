//! `exclude_capped_play_draw` (W-44; docs/dev/PLAN.md §12.3 S11): the play/draw choice from
//! paired simulations in which a game that did not finish no longer counts as an even game.
//!
//! The reference (`first_player_choice`, `ai/setup.py:86-120`) plays `first_player_games` pairs
//! of policy-vs-policy games, one going first and one going second on each determinisation, and
//! scores a game that hit the 3,000-decision cap or ended drawn as 0.5: an unfinished game then
//! pulls the paired difference towards zero, so towards going first. Here:
//!
//! - the games are the reference's, played by the strengthened rollout policy
//!   (`Mechanisms::rollout_choice` at the reference's temperature; the reference policy until a
//!   package replaces it), on the same seeds drawn in the same order;
//! - a pair in which either game is capped, drawn or failed (`engine_error_leaf`) is excluded
//!   from the paired estimate (W-44's resolution plan), so the decision rests on finished games
//!   only: go second only when that is better by more than two paired standard errors over the
//!   counted pairs, otherwise go first;
//! - each choice's reported score scores a capped (or failed) game by the evaluation, the leaf
//!   value of its final state (`Mechanisms::leaf_value`), instead of 0.5, and a drawn game 0.5;
//! - each alternative's `visits` is the number of counted pairs, so the decision trace records
//!   how many simulations were excluded (`first_player_games - visits` pairs).
//!
//! With every game finished the choice, the alternatives and the generator draws are the
//! reference's, bit for bit.

use gcg_ai::{AgentError, Alternative};
use gcg_core::pymath::py_sum;
use gcg_engine::{Action, ActionKind};

use super::super::{PregameCx, math_of};
use super::sim::{Ending, Line, Rollout, SIGNIFICANCE, paired, simulate};

/// `FIRST_PLAYER_TEMPERATURE` (`ai/setup.py:25`).
pub(crate) const FIRST_PLAYER_TEMPERATURE: f64 = 0.5;
/// `MAX_GAME_DECISIONS` (`ai/setup.py:26`).
pub(crate) const MAX_GAME_DECISIONS: u32 = 3000;

/// The reference's rollout settings of a play/draw game: to the end, at most 3,000 decisions.
pub(crate) const PLAY_DRAW_ROLLOUT: Rollout = Rollout {
    temperature: FIRST_PLAYER_TEMPERATURE,
    stop_turn: None,
    max_decisions: MAX_GAME_DECISIONS,
};

/// A game's score in the choice's reported mean: a finished game 1, 0 or (drawn) 0.5; a capped
/// or failed game its evaluation.
fn reported(line: &Line, player: u8) -> f64 {
    match line.ending {
        Ending::Finished(w) if w == i32::from(player) => 1.0,
        Ending::Finished(-1) => 0.5,
        Ending::Finished(_) => 0.0,
        Ending::Stopped | Ending::Capped | Ending::Failed => line.value,
    }
}

/// A finished game with a winner: 1 or 0 for the observer; `None` for a capped, drawn or failed
/// game, which excludes its pair from the paired estimate.
fn decided(line: &Line, player: u8) -> Option<f64> {
    match line.ending {
        Ending::Finished(-1) => None,
        Ending::Finished(w) => Some(if w == i32::from(player) { 1.0 } else { 0.0 }),
        Ending::Stopped | Ending::Capped | Ending::Failed => None,
    }
}

/// The play/draw choice of `cx` (CHOOSE_FIRST) with `exclude_capped_play_draw`: the
/// reference's games ([`PLAY_DRAW_ROLLOUT`]).
///
/// # Errors
/// As [`simulate`].
pub(crate) fn choose(cx: &mut PregameCx<'_, '_>) -> Result<(Action, Vec<Alternative>), AgentError> {
    choose_with(cx, PLAY_DRAW_ROLLOUT)
}

/// [`choose`] with the games of `rollout`.
///
/// # Errors
/// As [`simulate`].
pub(crate) fn choose_with(
    cx: &mut PregameCx<'_, '_>,
    rollout: Rollout,
) -> Result<(Action, Vec<Alternative>), AgentError> {
    let player = cx.player;
    let games = cx.cfg.first_player_games;
    let first = Action::new(ActionKind::GoFirst, i32::from(player), -1, -1);
    let second = Action::new(ActionKind::GoFirst, 1 - i32::from(player), -1, -1);
    let seeds: Vec<u64> = (0..games).map(|_| cx.rng.next_u64()).collect();
    let lines = simulate(cx, &[first, second], &seeds, rollout)?;
    let (as_first, as_second) = lines.split_at(seeds.len());
    let diffs: Vec<f64> = as_first
        .iter()
        .zip(as_second)
        .filter_map(|(a, b)| Some(decided(a, player)? - decided(b, player)?))
        .collect();
    let counted = u32::try_from(diffs.len()).expect("pairs fit in u32");
    let games_f = f64::from(games.max(1));
    let mean_of = |ls: &[Line]| py_sum(ls.iter().map(|l| reported(l, player))) / games_f;
    let stats = vec![
        Alternative {
            action: first,
            value: mean_of(as_first),
            visits: counted,
        },
        Alternative {
            action: second,
            value: mean_of(as_second),
            visits: counted,
        },
    ];
    let chosen = match paired(&diffs, math_of(cx.cfg)) {
        Some((mean, se)) if mean < 0.0 && -mean > SIGNIFICANCE * se => second,
        _ => first,
    };
    Ok((chosen, stats))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(ending: Ending, value: f64) -> Line {
        Line {
            ending,
            value,
            applies: 1,
        }
    }

    /// W-44's unit test: a capped simulation is excluded from the paired estimate and scored by
    /// its evaluation in the reported mean (the reference scores it 0.5,
    /// `crates/gcg-search/tests/scenarios/p6h_play_draw.rs` pins that side).
    #[test]
    fn unfinished_games_are_excluded_from_the_pairs_and_scored_by_the_evaluation() {
        let capped = line(Ending::Capped, 0.8);
        assert_eq!(decided(&capped, 0), None);
        assert!((reported(&capped, 0) - 0.8).abs() < f64::EPSILON);
        let drawn = line(Ending::Finished(-1), 0.3);
        assert_eq!(decided(&drawn, 1), None);
        assert!((reported(&drawn, 1) - 0.5).abs() < f64::EPSILON);
        let failed = line(Ending::Failed, 0.02);
        assert_eq!(decided(&failed, 0), None);
        assert!((reported(&failed, 0) - 0.02).abs() < f64::EPSILON);
        for p in 0..2u8 {
            let won = line(Ending::Finished(i32::from(p)), 0.9);
            let lost = line(Ending::Finished(1 - i32::from(p)), 0.1);
            assert_eq!(decided(&won, p), Some(1.0));
            assert_eq!(decided(&lost, p), Some(0.0));
            assert!((reported(&won, p) - 1.0).abs() < f64::EPSILON);
            assert!(reported(&lost, p).abs() < f64::EPSILON);
        }
    }
}
