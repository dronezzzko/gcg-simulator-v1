//! The Rust fuzz runner (PLAN §9.4; T-055, T-056, W-110) on runs small enough for every `cargo test`:
//! `test_small_fuzz_smoke` (`tests/slow/test_robustness.py:27-29`: 40 games at seed 5) checked
//! against the outcomes Python recorded for those games, the first games of the seed-2026 run
//! against Python's per-game rows, the commit-derived seed against Python's `derive_seed`,
//! strategic agents with invariants after every action, and the same results for any
//! `--workers`. The 10,000-game and 2,000-game runs are `fuzz_long.rs` (feature `slow`).

use std::num::NonZeroUsize;
use std::path::PathBuf;

use gcg_diff::fuzz::{self, AgentKind, AgentSpec, FuzzArgs, GameOut};
use gcg_diff::json::py_dumps;
use gcg_diff::{Record, read_trace};

fn tier1_fuzz() -> PathBuf {
    PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../oracle/corpus/v1/tier1/fuzz"
    ))
}

fn args(games: u64, seed: u64, workers: usize, agent: AgentSpec) -> FuzzArgs {
    FuzzArgs {
        games,
        seed,
        workers: NonZeroUsize::new(workers).expect("workers > 0"),
        agent,
    }
}

fn assert_all_ok(games: &[GameOut]) {
    let failed: Vec<String> = games
        .iter()
        .filter(|g| !g.ok)
        .map(|g| format!("game {} (seed {}): {:?}", g.index, g.seed, g.error))
        .collect();
    assert!(failed.is_empty(), "{}", failed.join("\n"));
    for g in games {
        assert!(g.finished, "game {}", g.index);
        let end = g.end.as_deref().unwrap_or_default();
        assert!(
            ["battle_damage", "deck_out", "both_defeated"].contains(&end),
            "game {} ended by {end}",
            g.index
        );
    }
}

/// `(actions, turn, end)` of every recorded outcome in `files`, in trace order.
fn recorded_outcomes(files: &[&str]) -> Vec<(u64, i32, Option<&'static str>)> {
    let mut out = Vec::new();
    for f in files {
        for t in read_trace(&tier1_fuzz().join(f)).expect("fuzz traces decode") {
            let o = t
                .records
                .iter()
                .find_map(|r| match r {
                    Record::Outcome(o) => Some(o.clone()),
                    _ => None,
                })
                .expect("an outcome");
            out.push((
                o.actions,
                o.turn,
                o.end_reason.map(gcg_engine::EndReason::as_str),
            ));
        }
    }
    out
}

#[test]
fn forty_games_at_seed_5_hold_invariants_and_end_as_in_python() {
    let games = fuzz::run(&args(40, 5, 1, AgentSpec::Both(AgentKind::Fuzz))).expect("runs");
    assert_eq!(games.len(), 40);
    assert_all_ok(&games);
    let want = recorded_outcomes(&["s5-g0000-0019.ndjson", "s5-g0020-0039.ndjson"]);
    assert_eq!(want.len(), 40);
    for (g, w) in games.iter().zip(&want) {
        assert_eq!(
            (g.actions, g.turns, g.end.as_deref()),
            (w.0, Some(w.1), w.2)
        );
    }
}

#[test]
fn the_first_games_at_seed_2026_equal_pythons_rows() {
    const N: usize = 200;
    let games =
        fuzz::run(&args(N as u64, 2026, 4, AgentSpec::Both(AgentKind::Fuzz))).expect("runs");
    let rows = std::fs::read_to_string(tier1_fuzz().join("summary-2026-10000.games.jsonl"))
        .expect("Python's per-game rows");
    let want: Vec<&str> = rows.lines().take(N).collect();
    assert_eq!(want.len(), N);
    for (g, w) in games.iter().zip(want) {
        assert_eq!(py_dumps(&g.row()), w, "game {}", g.index);
    }
    assert_all_ok(&games);
}

#[test]
fn the_commit_seed_is_pythons_derive_seed() {
    // uv run python -c "from gcg_sim.rng import derive_seed;
    //   print(derive_seed(0, 'fuzz-commit', SHA))"
    for (sha, want) in [
        (
            "6daf17f200f3efa3cf4a1828646d753916252a8f",
            18_153_865_437_585_773_634_u64,
        ),
        (
            "0000000000000000000000000000000000000000",
            5_303_823_986_362_735_348,
        ),
    ] {
        assert_eq!(fuzz::commit_seed(sha), want, "{sha}");
    }
}

#[test]
fn results_do_not_depend_on_the_worker_count() {
    for agent in [
        AgentSpec::Both(AgentKind::Fuzz),
        AgentSpec::Both(AgentKind::Random),
        AgentSpec::Mixed(AgentKind::Greedy, AgentKind::Random),
    ] {
        let one = fuzz::run(&args(12, 9, 1, agent.clone())).expect("runs");
        for workers in [3, 8] {
            let many = fuzz::run(&args(12, 9, workers, agent.clone())).expect("runs");
            assert_eq!(one, many, "{agent:?} with {workers} workers");
        }
    }
}

#[test]
fn strategic_agents_play_legal_games_with_invariants() {
    for agent in [
        AgentSpec::Both(AgentKind::Greedy),
        AgentSpec::Both(AgentKind::Random),
        AgentSpec::Mixed(AgentKind::Greedy, AgentKind::Random),
    ] {
        let games = fuzz::run(&args(8, 2026, 4, agent.clone())).expect("runs");
        assert_eq!(games.len(), 8, "{agent:?}");
        assert_all_ok(&games);
    }
}

#[test]
fn mixed_agents_are_seeded_per_seat_and_swap_seats() {
    // `mixed:A,A` is `A` at both seats: the agent seeds depend on the seat only.
    let same = fuzz::run(&args(
        4,
        31,
        1,
        AgentSpec::Mixed(AgentKind::Greedy, AgentKind::Greedy),
    ))
    .expect("runs");
    let both = fuzz::run(&args(4, 31, 1, AgentSpec::Both(AgentKind::Greedy))).expect("runs");
    assert_eq!(same, both);
    // `mixed:A,B` and `mixed:B,A` differ in every game: the seat assignment reaches the agents.
    let mixed = fuzz::run(&args(
        2,
        31,
        1,
        AgentSpec::Mixed(AgentKind::Greedy, AgentKind::Random),
    ))
    .expect("runs");
    let swapped = fuzz::run(&args(
        2,
        31,
        1,
        AgentSpec::Mixed(AgentKind::Random, AgentKind::Greedy),
    ))
    .expect("runs");
    assert_ne!(mixed[0], swapped[0]);
    assert_ne!(mixed[1], swapped[1]);
    assert_all_ok(&mixed);
    assert_all_ok(&swapped);
}
