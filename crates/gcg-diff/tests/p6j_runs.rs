//! P6-J run-level evidence (docs/port/PLAN.md §12.2 P6-J; docs/port/eval/weakness-status/P6-J.md):
//!
//! - W-143 e2e: a `test-random` benchmark whose players build the two-GD04-095 redirect cycle
//!   fails with exit status 1 (`CliError`) and writes no report, as Python's does.
//! - W-46, W-47, W-48, W-54, W-162 over Python's fuzz games: the games of `python -m
//!   gcg_sim.tools.fuzz --seed 2026` (decks `deck_for`, seeds `fuzz-game`/`fuzz-agent`, uniform
//!   random choices, `tools/fuzz.py:47-120`), each checked against Python's per-game row
//!   (`port/corpus/v1/tier1/fuzz/summary-2026-10000.games.jsonl`), with no PAYMENT decision,
//!   no ATTACK_TRIGGERS step, no `NoProgress`, no safety cap, no derived view that hit the
//!   fixpoint cap, and (feature `paranoid`) the list of triggers whose host moved again before
//!   they resolved. The first 200 games run in the default suite, all 10,000 with `slow`.

use std::path::PathBuf;

use gcg_core::derive_seed;
use gcg_core::pool::Cancel;
use gcg_core::rng::SplitMix64;
use gcg_diff::fuzz::Pools;
use gcg_effects::Registry;
use gcg_engine::{DecisionKind, EndReason, NewGameOptions, Step};
use gcg_runner::argparse::PyStr;
use gcg_runner::cli_support::{BenchmarkRequest, CliError, benchmark_command};
use gcg_runner::{Format, SchemaVersion, TestAgentKind, Workers};
use serde_json::{Value, json};

fn root() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

// ---------------------------------------------------------------------------------------------
// W-143 e2e

/// A mono-purple Minerva Squad deck with four GD04-095 Lunamaria Hawke (legal: 50 cards, 10
/// Resources; Purple).
const MINERVA: &str = "\
# W-143 e2e: mono-purple Minerva Squad, four GD04-095 Lunamaria Hawke
4 GD04-095 Lunamaria Hawke
4 GD04-062 Lunamaria's Gunner Zaku Warrior
4 GD04-053
4 ST09-006
4 GD04-056
4 GD05-064
4 ST09-005
4 ST09-001
4 ST09-002
4 ST09-007
4 GD04-055
4 GD04-093
2 GD04-116
10 R-001
";

const REPORTS: [&str; 5] = [
    "results.json",
    "summary.md",
    "games.ndjson",
    "timing.json",
    "replays",
];

fn run_minerva(matches: u32, out: &std::path::Path) -> (Result<(), CliError>, String) {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("p6j_w143");
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    let deck = dir.join("minerva.txt");
    std::fs::write(&deck, MINERVA).expect("writes the deck");
    let _ = std::fs::remove_dir_all(out);
    let deck = PyStr::from(deck.to_str().expect("a UTF-8 path"));
    let out_text = PyStr::from(out.to_str().expect("a UTF-8 path"));
    let req = BenchmarkRequest {
        deck_under_test: &deck,
        benchmark_deck: &deck,
        matches,
        seed: 4,
        workers: Workers::Count(1),
        format: Format::Bo1,
        ai_preset: "standard",
        out: &out_text,
        decision_log: false,
        replays: 1,
        schema: SchemaVersion::V1_1,
        package: "p6j",
        code_digest: "0000000000000000000000000000000000000000000000000000000000000000",
    };
    let factory = gcg_runner::test_agents::factory(TestAgentKind::Random);
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    let result = benchmark_command(
        &req,
        &*factory,
        false,
        &Cancel::new(),
        &mut stdout,
        &mut stderr,
    );
    (result, String::from_utf8_lossy(&stdout).into_owned())
}

/// The run `gcg-eval reference-benchmark minerva.txt minerva.txt --agent test-random --format
/// bo1 --matches 47 --seed 4 --workers 1` makes (its `--agent test-random` is Python's
/// `GCG_SIM_AGENT_FACTORY` hook, T-073): match 46 reaches the cycle, the run fails with the
/// CLI's `simulation failed` error (printed as `error: ...`, exit 1), and no report is
/// written. Python, the same decks and options with
/// `GCG_SIM_AGENT_FACTORY=tests/runner/random_agent.py:make_random_agent gcg-sim benchmark`:
///
/// ```text
/// error: simulation failed: match 46 game 1 (seed 13255759802130423377) failed: RecursionError: maximum recursion depth exceeded
/// (exit 1, no report directory)
/// ```
///
/// Only the exception text differs (W-143's resolution plan). With 46 matches the same run
/// completes and writes every report (the control).
#[test]
fn w143_a_benchmark_that_builds_the_cycle_fails_without_reports() {
    let out = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("p6j_w143/out");
    let (result, stdout) = run_minerva(47, &out);
    let Err(CliError::Invalid(message)) = result else {
        panic!("the run must fail: {result:?}");
    };
    assert_eq!(
        message.to_string_opt().expect("UTF-8"),
        "simulation failed: match 46 game 1 (seed 13255759802130423377) failed: \
         RecursionError: battle damage redirect cycle through units [55, 96, 55]"
    );
    assert!(stdout.is_empty(), "{stdout}");
    for name in REPORTS {
        assert!(!out.join(name).exists(), "{name} was written");
    }
    let (result, _) = run_minerva(46, &out);
    assert_eq!(result, Ok(()));
    for name in REPORTS {
        assert!(out.join(name).exists(), "{name} is missing");
    }
}

// ---------------------------------------------------------------------------------------------
// Fuzz diagnostics

const FUZZ_SEED: u64 = 2026;

/// What one fuzz game showed.
#[derive(Debug)]
struct GameDiag {
    index: u64,
    row: Value,
    payment: u64,
    attack_triggers: u64,
    non_converged: u64,
    #[cfg(feature = "paranoid")]
    moved: Vec<gcg_engine::paranoid::MovedHost>,
}

/// `play_one(index, 2026)` of `tools/fuzz.py:80-120` with the fuzz player, on this thread.
fn fuzz_game(reg: &std::sync::Arc<Registry>, pools: &Pools<'_>, index: u64) -> GameDiag {
    let part = i64::try_from(index).expect("an index");
    let d0 = pools.deck_for(2 * index, FUZZ_SEED);
    let d1 = pools.deck_for(2 * index + 1, FUZZ_SEED);
    let game_seed = derive_seed!(FUZZ_SEED, "fuzz-game", part);
    let mut rng = SplitMix64::new(derive_seed!(FUZZ_SEED, "fuzz-agent", part));
    let (m0, r0, m1, r1) = (
        strs(&d0.main),
        strs(&d0.resources),
        strs(&d1.main),
        strs(&d1.resources),
    );
    fn strs(v: &[String]) -> Vec<&str> {
        v.iter().map(String::as_str).collect()
    }
    let non_converged = gcg_engine::diag::non_converged_views();
    #[cfg(feature = "paranoid")]
    let moved = gcg_engine::paranoid::moved_hosts().len();
    let mut diag = GameDiag {
        index,
        row: Value::Null,
        payment: 0,
        attack_triggers: 0,
        non_converged: 0,
        #[cfg(feature = "paranoid")]
        moved: Vec::new(),
    };
    let mut st = gcg_engine::new_game_by_number(
        reg,
        [(&m0, &r0), (&m1, &r1)],
        game_seed,
        NewGameOptions::default(),
    )
    .expect("a fuzz game starts");
    let mut actions = 0u64;
    while st.winner().is_none() {
        diag.attack_triggers += u64::from(st.step() == Step::AttackTriggers);
        let pending = st.pending().expect("a decision");
        diag.payment += u64::from(pending.hdr.kind == DecisionKind::Payment);
        let options = pending.options;
        let a = options[rng.randrange_usize(options.len())];
        gcg_engine::apply(&mut st, a, true).unwrap_or_else(|e| panic!("game {index}: {e}"));
        actions += 1;
    }
    assert_eq!(st.cap_kind(), None, "game {index} hit a safety cap");
    let end = st.end_reason().map_or("none", EndReason::as_str);
    diag.row = json!({
        "actions": actions,
        "end": end,
        "index": index,
        "ok": st.end_reason() != Some(EndReason::TurnLimit),
        "seed": game_seed,
        "turns": st.turn(),
    });
    diag.non_converged = gcg_engine::diag::non_converged_views() - non_converged;
    #[cfg(feature = "paranoid")]
    {
        diag.moved = gcg_engine::paranoid::moved_hosts()[moved..].to_vec();
    }
    diag
}

/// Games `0..n` on `threads` threads (each game wholly on one thread, so the per-thread
/// diagnostics belong to it), in index order.
fn fuzz_games(n: u64, threads: u64) -> Vec<GameDiag> {
    let reg = Registry::embedded();
    let pools = Pools::new(&reg);
    let mut all: Vec<GameDiag> = std::thread::scope(|s| {
        let handles: Vec<_> = (0..threads)
            .map(|t| {
                let (reg, pools) = (&reg, &pools);
                s.spawn(move || {
                    (t..n)
                        .step_by(usize::try_from(threads).expect("threads"))
                        .map(|i| fuzz_game(reg, pools, i))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().expect("a fuzz thread"))
            .collect()
    });
    all.sort_by_key(|g| g.index);
    all
}

fn python_rows(n: usize) -> Vec<Value> {
    let path = root().join("port/corpus/v1/tier1/fuzz/summary-2026-10000.games.jsonl");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        .lines()
        .take(n)
        .map(|l| serde_json::from_str(l).expect("a JSON row"))
        .collect()
}

/// Runs `n` games, checks them against Python's rows and the diagnostics, and prints the
/// totals (and, under `paranoid`, every moved-host trigger with its game).
fn check_fuzz(n: u64) {
    let threads = 4;
    let games = fuzz_games(n, threads);
    let rows = python_rows(usize::try_from(n).expect("n"));
    assert_eq!(games.len(), rows.len());
    for (g, row) in games.iter().zip(&rows) {
        assert_eq!(&g.row, row, "game {} differs from Python's", g.index);
    }
    let sum = |f: fn(&GameDiag) -> u64| games.iter().map(f).sum::<u64>();
    let (payment, attack_triggers, non_converged) = (
        sum(|g| g.payment),
        sum(|g| g.attack_triggers),
        sum(|g| g.non_converged),
    );
    println!(
        "{n} fuzz games (seed {FUZZ_SEED}) equal Python's rows: {} actions; PAYMENT decisions \
         {payment}, ATTACK_TRIGGERS steps {attack_triggers}, non-converged views \
         {non_converged}",
        sum(|g| g.row["actions"].as_u64().expect("actions"))
    );
    assert_eq!((payment, attack_triggers, non_converged), (0, 0, 0));
    #[cfg(feature = "paranoid")]
    {
        let moved: Vec<(u64, &gcg_engine::paranoid::MovedHost)> = games
            .iter()
            .flat_map(|g| g.moved.iter().map(move |m| (g.index, m)))
            .collect();
        let mut cards: Vec<&str> = moved.iter().map(|(_, m)| &*m.card_number).collect();
        cards.sort();
        cards.dedup();
        println!(
            "paranoid: {} triggers resolved after their host moved again, in {} games; cards: \
             {cards:?}",
            moved.len(),
            games.iter().filter(|g| !g.moved.is_empty()).count()
        );
        for (index, m) in &moved {
            println!(
                "  game {index}: {} (zone_seq {} -> {})",
                m.card_number, m.queued_seq, m.resolved_seq
            );
        }
    }
}

#[test]
fn p6j_fuzz_diagnostics_on_the_first_python_fuzz_games() {
    check_fuzz(200);
}

#[cfg(feature = "slow")]
#[test]
fn p6j_fuzz_diagnostics_on_all_10000_python_fuzz_games() {
    check_fuzz(10_000);
}
