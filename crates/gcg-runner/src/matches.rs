//! Match sequencing: best-of-three (official BO3 Match Rules) or single games
//! (`runner/match.py`; docs/port/inventory/01-rng-runner-cli.md §2.5). The module is not
//! called `match`, which is a Rust keyword.

use gcg_ai::AgentCx;

use crate::config::dut_seat;
use crate::game::{match_loser_seat, play_game};
use crate::{AgentFactory, BenchmarkConfig, GameRecord, MatchRecord, Outcome, Role, RunError};

/// `_winner(dut_points, bench_points)` (`runner/match.py:11-16`).
const fn winner(dut_points: u32, bench_points: u32) -> Outcome {
    if dut_points > bench_points {
        Outcome::Dut
    } else if bench_points > dut_points {
        Outcome::Bench
    } else {
        Outcome::Draw
    }
}

/// `play_match(config, match_index, agent_factory=...)` (`runner/match.py:19-46`): games until
/// one side has `wins_needed` match points or the game cap is reached.
///
/// Game 1's Player One is chosen after a seeded die roll; each later game is chosen by the
/// previous game's (match-scoring) loser. After a game without a match point (turn limit, or
/// a BO1-style draw) the next chooser is again decided by the seeded die roll.
///
/// # Errors
/// The first game's [`RunError`].
pub fn play_match(
    cfg: &BenchmarkConfig,
    m: u32,
    f: &dyn AgentFactory,
    cx: &mut AgentCx<'_>,
) -> Result<MatchRecord, RunError> {
    play_match_with(cfg, m, &mut |g, chooser| {
        play_game(cfg, m, g, chooser, f, cx)
    })
}

/// [`play_match`] over any game player (`play(game_index, chooser)`), so the sequencing is
/// testable on scripted games (`tests/runner/test_match.py:49-63` monkeypatches `play_game`).
pub(crate) fn play_match_with(
    cfg: &BenchmarkConfig,
    m: u32,
    play: &mut dyn FnMut(u32, Option<u8>) -> Result<GameRecord, RunError>,
) -> Result<MatchRecord, RunError> {
    let mut games = Vec::new();
    let (mut dut_points, mut bench_points) = (0_u32, 0_u32);
    let mut chooser = None;
    for g in 0..cfg.games_per_match() {
        let record = play(g, chooser)?;
        match record.match_point {
            Some(Role::Dut) => dut_points += 1,
            Some(Role::Bench) => bench_points += 1,
            None => {}
        }
        let next = match_loser_seat(&record);
        games.push(record);
        if dut_points.max(bench_points) >= cfg.wins_needed() {
            break;
        }
        chooser = next;
    }
    Ok(MatchRecord {
        match_index: m,
        dut_seat: dut_seat(m),
        games,
        winner: winner(dut_points, bench_points),
        dut_points,
        bench_points,
    })
}

#[cfg(test)]
mod tests {
    use gcg_engine::EndReason;

    use super::*;
    use crate::Format;
    use crate::config::tests::config;
    use crate::game::tests::fake_record;

    type Script = [(Outcome, Option<Role>, EndReason)];

    /// `scripted(monkeypatch, outcomes)` (`tests/runner/test_match.py:49-63`): plays the
    /// scripted outcomes and returns the choosers it was asked with.
    fn scripted(
        cfg: &BenchmarkConfig,
        m: u32,
        outcomes: &Script,
    ) -> (MatchRecord, Vec<Option<u8>>) {
        let mut choosers = Vec::new();
        let record = play_match_with(cfg, m, &mut |g, chooser| {
            choosers.push(chooser);
            let (winner, point, reason) = outcomes[usize::try_from(g).expect("index")];
            Ok(fake_record(m, g, chooser, winner, point, reason))
        })
        .expect("scripted games never fail");
        (record, choosers)
    }

    const DUT: (Outcome, Option<Role>, EndReason) =
        (Outcome::Dut, Some(Role::Dut), EndReason::BattleDamage);

    #[test]
    fn match_ends_when_a_player_has_two_wins() {
        // tests/runner/test_match.py:66-73
        let (record, choosers) = scripted(&config(), 0, &[DUT; 3]);
        assert_eq!(record.games.len(), 2);
        assert_eq!(
            (record.winner, record.dut_points, record.bench_points),
            (Outcome::Dut, 2, 0)
        );
        // The deck under test is seat 0 in match 0; the bench seat lost game 1.
        assert_eq!(choosers, [None, Some(1)]);
    }

    #[test]
    fn simultaneous_defeat_scores_for_the_non_turn_player_in_bo3() {
        // tests/runner/test_match.py:76-87
        let both = EndReason::BothDefeated;
        let (record, choosers) = scripted(
            &config(),
            1,
            &[
                (Outcome::Draw, Some(Role::Bench), both),
                DUT,
                (Outcome::Dut, Some(Role::Dut), EndReason::DeckOut),
            ],
        );
        let winners: Vec<Outcome> = record.games.iter().map(|g| g.winner).collect();
        assert_eq!(winners, [Outcome::Draw, Outcome::Dut, Outcome::Dut]);
        assert_eq!(
            (record.winner, record.dut_points, record.bench_points),
            (Outcome::Dut, 2, 1)
        );
        assert_eq!(choosers, [None, Some(1), Some(0)]);
        assert_eq!(record.dut_seat, 1);
    }

    #[test]
    fn turn_limit_draw_scores_nothing_and_next_chooser_is_random() {
        // tests/runner/test_match.py:90-101
        let limit = EndReason::TurnLimit;
        let (record, choosers) = scripted(
            &config(),
            0,
            &[
                (Outcome::Draw, None, limit),
                DUT,
                (Outcome::Bench, Some(Role::Bench), EndReason::DeckOut),
            ],
        );
        assert_eq!(record.games.len(), 3);
        assert_eq!(
            (record.winner, record.dut_points, record.bench_points),
            (Outcome::Draw, 1, 1)
        );
        assert_eq!(choosers, [None, None, Some(1)]);
    }

    #[test]
    fn bo1_plays_a_single_game() {
        // tests/runner/test_match.py:104-109
        let mut cfg = config();
        cfg.format = Format::Bo1;
        let (record, choosers) =
            scripted(&cfg, 0, &[(Outcome::Draw, None, EndReason::BothDefeated)]);
        assert_eq!(record.games.len(), 1);
        assert_eq!(record.winner, Outcome::Draw);
        assert_eq!(choosers, [None]);
    }

    #[test]
    fn a_bo3_win_and_two_draws_wins_the_match_one_nil() {
        // Inv 01 §2.5: BO3 can end with fewer than 2 points after 3 games.
        let limit = (Outcome::Draw, None, EndReason::TurnLimit);
        let (record, _) = scripted(&config(), 2, &[DUT, limit, limit]);
        assert_eq!(record.games.len(), 3);
        assert_eq!(
            (record.winner, record.dut_points, record.bench_points),
            (Outcome::Dut, 1, 0)
        );
    }

    #[test]
    fn a_failing_game_fails_the_match() {
        let cfg = config();
        let mut calls = 0;
        let result = play_match_with(&cfg, 0, &mut |g, chooser| {
            calls += 1;
            if g == 1 {
                Err(RunError::GameFailure("boom".to_owned()))
            } else {
                Ok(fake_record(0, g, chooser, DUT.0, DUT.1, DUT.2))
            }
        });
        assert_eq!(result, Err(RunError::GameFailure("boom".to_owned())));
        assert_eq!(calls, 2);
    }
}
