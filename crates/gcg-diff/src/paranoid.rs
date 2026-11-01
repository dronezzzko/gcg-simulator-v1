//! `gcg-diff paranoid-games` (PLAN §8.7, §9.1; W-49, W-124): the games of
//! `tests/rules/test_engine_caches.py:26-34` `play_games`, played on a `paranoid` build of the
//! engine, whose own checks compare every cached view with a fresh computation, every skipped
//! event with its trigger candidates and every skipped rules check with a full one:
//!
//! - `fuzz.run(16, seed=11, workers=1)` with no failure;
//! - for seeds 1 and 2, `run_game((greedy(seed), greedy(seed + 10)), (TUNING_DECKS[0],
//!   TUNING_DECKS[1]), seed)` (`ai/selfplay.py:60-75`) ending with a winner.

use std::num::NonZeroUsize;

use gcg_ai::{Agent, AgentCx, DEFAULT_WEIGHTS, Scratch};
use gcg_core::pool::{Cancel, Pool, TaskCx};
use gcg_effects::Registry;
use gcg_engine::NewGameOptions;

use crate::engine::{apply_error, new_game_error};
use crate::fuzz::{self, AgentKind, AgentSpec, FuzzArgs};

/// `FUZZ_GAMES` (`tests/rules/test_engine_caches.py:18`).
pub const FUZZ_GAMES: u64 = 16;
/// `fuzz.run(FUZZ_GAMES, seed=11, ...)` (`:28`).
pub const FUZZ_SEED: u64 = 11;
/// `DECISION_LIMIT` (`ai/selfplay.py:22`).
pub const DECISION_LIMIT: u32 = 20_000;

/// `RESOURCES` (`ai/tuning.py:62`).
const RESOURCES: [&str; 10] = ["R-001"; 10];
/// `ZEON_COUNTS` (`ai/tuning.py:63-67`).
const ZEON_COUNTS: [(&str, usize); 20] = [
    ("ST03-006", 3),
    ("ST03-007", 3),
    ("ST03-008", 4),
    ("GD01-031", 3),
    ("GD01-030", 3),
    ("GD01-036", 3),
    ("GD01-037", 2),
    ("GD01-032", 2),
    ("GD01-027", 2),
    ("GD01-092", 3),
    ("GD02-089", 2),
    ("ST03-003", 3),
    ("ST03-004", 2),
    ("GD01-059", 2),
    ("GD01-105", 2),
    ("ST03-013", 3),
    ("GD01-115", 2),
    ("ST03-015", 2),
    ("GD01-126", 2),
    ("ST03-012", 2),
];
/// `WING_COUNTS` (`ai/tuning.py:68-72`).
const WING_COUNTS: [(&str, usize); 19] = [
    ("ST02-001", 2),
    ("ST02-002", 3),
    ("ST02-003", 2),
    ("ST02-004", 3),
    ("ST02-005", 4),
    ("GD01-033", 3),
    ("GD01-040", 3),
    ("GD01-041", 2),
    ("GD01-043", 2),
    ("GD02-028", 3),
    ("ST02-010", 4),
    ("ST02-006", 2),
    ("ST02-008", 3),
    ("ST02-009", 3),
    ("ST02-011", 2),
    ("ST02-012", 2),
    ("ST02-014", 3),
    ("GD01-126", 2),
    ("GD01-099", 2),
];

/// `_deck(counts)` (`ai/tuning.py:86-87`): the numbers sorted, each repeated by its count.
#[must_use]
pub fn tuning_deck(counts: &[(&'static str, usize)]) -> Vec<&'static str> {
    let mut sorted = counts.to_vec();
    sorted.sort_by(|a, b| a.0.cmp(b.0));
    sorted
        .into_iter()
        .flat_map(|(n, c)| std::iter::repeat_n(n, c))
        .collect()
}

fn greedy(seed: u64) -> Box<dyn Agent> {
    gcg_ai::make_greedy_agent(seed, false, (*DEFAULT_WEIGHTS).clone())
}

/// `run_game` (`ai/selfplay.py:60-75`) with two greedy agents on the first two tuning decks.
///
/// # Errors
/// What the engine or an agent raised, or a game without a winner.
pub fn greedy_game(seed: u64) -> Result<(i8, i32, u32), String> {
    let reg = Registry::embedded();
    let zeon = tuning_deck(&ZEON_COUNTS);
    let wing = tuning_deck(&WING_COUNTS);
    let mut st = gcg_engine::new_game_by_number(
        &reg,
        [(&zeon, &RESOURCES), (&wing, &RESOURCES)],
        seed,
        NewGameOptions::default(),
    )
    .map_err(|e| {
        let r = new_game_error(&e);
        format!("{}: {}", r.class, r.message)
    })?;
    let mut agents = [greedy(seed), greedy(seed + 10)];
    let cancel = Cancel::new();
    let mut decisions = 0;
    while decisions < DECISION_LIMIT {
        let Some(p) = st.pending().map(|d| d.hdr.player) else {
            break;
        };
        let agent = &mut agents[usize::from(p)];
        let action = Pool::run_inline(|scratch: &mut Scratch, par: &TaskCx<'_, Scratch>| {
            let mut cx = AgentCx {
                scratch,
                par,
                cancel: &cancel,
            };
            agent.choose(&st, p, &mut cx)
        })
        .map_err(|e| format!("{}: {e:?}", e.py_class()))?;
        gcg_engine::apply(&mut st, action, true).map_err(|e| {
            let r = apply_error(&e);
            format!("{}: {}", r.class, r.message)
        })?;
        decisions += 1;
    }
    let winner = st
        .winner()
        .ok_or_else(|| format!("greedy game {seed} ended without a winner"))?;
    Ok((winner, st.turn(), decisions))
}

/// The `paranoid-games` report: one line per game group, and whether all passed.
///
/// # Errors
/// A fuzz run that could not run.
pub fn run() -> Result<(Vec<String>, bool), String> {
    let mut lines = Vec::new();
    let mut ok = true;
    let args = FuzzArgs {
        games: FUZZ_GAMES,
        seed: FUZZ_SEED,
        workers: NonZeroUsize::MIN,
        agent: AgentSpec::Both(AgentKind::Fuzz),
    };
    let games = fuzz::run(&args)?;
    let failed: Vec<&fuzz::GameOut> = games.iter().filter(|g| !g.ok).collect();
    lines.push(format!(
        "fuzz seed {FUZZ_SEED}: {} games, {} actions, {} failed",
        games.len(),
        games
            .iter()
            .filter(|g| g.ok)
            .map(|g| g.actions)
            .sum::<u64>(),
        failed.len()
    ));
    for g in failed.iter().take(3) {
        lines.push(format!(
            "  game {}: {}",
            g.index,
            g.error.as_deref().unwrap_or_default()
        ));
    }
    ok &= failed.is_empty();
    for seed in [1, 2] {
        match greedy_game(seed) {
            Ok((w, turn, n)) => lines.push(format!(
                "greedy seed {seed}: winner {w} on turn {turn} after {n} decisions"
            )),
            Err(e) => {
                lines.push(format!("greedy seed {seed}: {e}"));
                ok = false;
            }
        }
    }
    Ok((lines, ok))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tuning_decks_are_pythons() {
        let zeon = tuning_deck(&ZEON_COUNTS);
        let wing = tuning_deck(&WING_COUNTS);
        assert_eq!((zeon.len(), wing.len()), (50, 50));
        assert_eq!(&zeon[..4], ["GD01-027", "GD01-027", "GD01-030", "GD01-030"]);
        assert!(zeon.windows(2).all(|w| w[0] <= w[1]));
        let reg = Registry::embedded();
        for n in zeon.iter().chain(&wing) {
            assert!(reg.db().get(n).is_some(), "{n}");
        }
    }
}
