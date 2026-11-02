//! The long fuzz runs of PLAN §9.4 (feature `slow`, A12), with invariants after every action:
//!
//! - T-055: `tests/slow/test_robustness.py:15-24`, 10,000 games at seed 2026, whose summary is
//!   byte-equal to Python's `python -m gcg_sim.tools.fuzz --games 10000 --seed 2026` stdout
//!   (`port/corpus/v1/tier1/fuzz/summary-2026-10000.json`) and whose every game equals Python's
//!   row (`summary-2026-10000.games.jsonl`);
//! - T-056: 2,000 games at the seed derived from the checked-out commit
//!   (`derive_seed(0, "fuzz-commit", HEAD)`, as `gcg-diff fuzz --seed-from-commit`).
//!
//! Both also assert what `test_robustness.py` does: no failure, rules-defined ends only, and
//! every pool card in some deck.
#![cfg(feature = "slow")]

use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::process::Command;

use gcg_diff::fuzz::{self, AgentKind, AgentSpec, FuzzArgs, GameOut, Pools};
use gcg_effects::Registry;

fn root() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

fn run(games: u64, seed: u64) -> Vec<GameOut> {
    let workers = gcg_core::sys::topology::detect()
        .auto_workers(usize::try_from(games).expect("games fit usize"));
    let args = FuzzArgs {
        games,
        seed,
        workers: NonZeroUsize::new(workers).expect("auto gives at least one worker"),
        agent: AgentSpec::Both(AgentKind::Fuzz),
    };
    fuzz::run(&args).expect("the run completes")
}

/// `test_robustness.py:20-24`: no failure, ends in `{battle_damage, deck_out, both_defeated}`,
/// and the decks jointly hold every card of the pool.
fn assert_robust(games: &[GameOut], n: usize) {
    assert_eq!(games.len(), n);
    let failed: Vec<String> = fuzz::failure_lines(games, 3);
    assert!(failed.is_empty(), "{}", failed.join("\n"));
    for g in games {
        let end = g.end.as_deref().unwrap_or_default();
        assert!(
            ["battle_damage", "deck_out", "both_defeated"].contains(&end),
            "game {} ended by {end}",
            g.index
        );
    }
    let reg = Registry::embedded();
    let pools = Pools::new(&reg);
    let mut seen: Vec<&str> = games
        .iter()
        .flat_map(|g| g.cards.iter().map(String::as_str))
        .collect();
    seen.sort();
    seen.dedup();
    let missing: Vec<&str> = pools
        .all()
        .into_iter()
        .filter(|n| seen.binary_search(n).is_err())
        .take(20)
        .collect();
    assert!(missing.is_empty(), "never in a deck: {missing:?}");
}

#[test]
fn ten_thousand_games_at_seed_2026_equal_pythons_summary() {
    let games = run(10_000, 2026);
    assert_robust(&games, 10_000);
    let reg = Registry::embedded();
    let text = fuzz::summary_text(&games, &Pools::new(&reg));
    let file = root().join("port/corpus/v1/tier1/fuzz/summary-2026-10000.json");
    let want = std::fs::read_to_string(&file).expect("Python's summary");
    assert_eq!(text, want);
    fuzz::compare(&text, &games, &file).expect("every game equals Python's row");
}

fn head_commit(repo: &Path) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["rev-parse", "HEAD"])
        .output()
        .expect("git runs (the fuzz seed derives from the checked-out commit)");
    assert!(out.status.success(), "git rev-parse HEAD failed");
    let sha = String::from_utf8(out.stdout).expect("utf-8");
    let sha = sha.trim().to_owned();
    assert_eq!(sha.len(), 40, "{sha:?}");
    sha
}

#[test]
fn two_thousand_games_at_the_commit_seed_hold_invariants() {
    let sha = head_commit(&root());
    let seed = fuzz::commit_seed(&sha);
    println!("seed {seed} from commit {sha}");
    let games = run(2_000, seed);
    assert_robust(&games, 2_000);
    assert!(games.iter().all(|g| g.actions > 0));
}
