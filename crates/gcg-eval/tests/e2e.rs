//! End-to-end runs of `gcg-eval` through the engine, the agents and the runner (feature
//! `slow`, PLAN A12): `compare` on real games equals `python -m gcg_sim.ai.compare` byte for byte
//! (T-079, vectors `tests/vectors/games.json` from `export_stats.py --games`); `head-to-head`
//! writes identical reports at 1, 4 and 10 workers (T-080) and realises the A24 balance with
//! `--choose-first seat0` (T-149); `reference-benchmark` writes Python's preset and agent names
//! and schema 1.1.0, and its replays verify (T-082, the M8 smoke run); `preset-benchmark` runs a
//! configuration file.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Once;

use gcg_core::pool::Cancel;
use gcg_eval::cli::{Command, parse, run};
use gcg_eval::selfplay::play_all;
use gcg_eval::{ChooseFirst, MirroredSpec};
use serde_json::Value;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

static CHDIR: Once = Once::new();

/// Every relative path below (examples/decks, the default panel) is the repository's.
fn at_root() {
    CHDIR.call_once(|| std::env::set_current_dir(root()).expect("cd to the repository root"));
}

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("e2e-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn games_vectors() -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/vectors/games.json");
    let text = std::fs::read_to_string(&path).expect("read games.json");
    serde_json::from_str(&text).expect("parse games.json")
}

fn os(args: &[&str]) -> Vec<OsString> {
    args.iter().map(OsString::from).collect()
}

fn main_of(args: &[OsString]) -> (u8, String, String) {
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let status = run(args, None, &Cancel::new(), false, &mut out, &mut err);
    (
        status,
        String::from_utf8_lossy(&out).into_owned(),
        String::from_utf8_lossy(&err).into_owned(),
    )
}

fn read_json(path: &Path) -> Value {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

#[test]
fn compare_on_real_games_equals_python() {
    at_root();
    let v = games_vectors();
    let dir = scratch("compare");
    for (k, case) in v["compare"].as_array().expect("cases").iter().enumerate() {
        let mut argv: Vec<OsString> = vec!["compare".into()];
        argv.extend(
            case["argv"]
                .as_array()
                .expect("argv")
                .iter()
                .map(|a| OsString::from(a.as_str().unwrap_or_default())),
        );
        let out = dir.join(format!("report-{k}.json"));
        argv.push("--out".into());
        argv.push(out.clone().into_os_string());
        let Ok(Command::Compare(args)) = parse(&argv, 80) else {
            panic!("compare arguments {argv:?}")
        };
        let cancel = Cancel::new();
        let workers = args.workers();
        let mut play = |specs: &[MirroredSpec]| {
            play_all(specs, workers, ChooseFirst::Agents, &cancel, &mut |_| {})
                .map_err(|e| gcg_eval::bench_cmd::CommandError::Invalid(format!("{e:?}")))
        };
        let stdout = gcg_eval::compare::run(&args, &mut play).expect("compare runs");
        assert_eq!(
            stdout,
            case["stdout"].as_str().unwrap_or_default(),
            "{argv:?}"
        );
        assert_eq!(
            std::fs::read_to_string(&out).expect("--out"),
            case["out"].as_str().unwrap_or_default(),
            "{argv:?}"
        );
    }
}

fn head_to_head(extra: &[&str], workers: &str, out: &Path) -> Value {
    let mut args = os(&["head-to-head"]);
    args.extend(os(extra));
    args.extend(os(&["--workers", workers, "--out"]));
    args.push(out.as_os_str().to_owned());
    let (status, _, err) = main_of(&args);
    assert_eq!(status, 0, "{args:?}: {err}");
    read_json(out)
}

#[test]
fn head_to_head_is_identical_at_1_4_and_10_workers() {
    at_root();
    let dir = scratch("h2h-workers");
    for extra in [
        &[
            "random", "greedy", "--decks", "tuning", "--games", "24", "--seed", "20000001",
        ][..],
        &[
            "ref:fast",
            "random",
            "--games",
            "8",
            "--seed",
            "4",
            "--choose-first",
            "agents",
        ][..],
    ] {
        let mut texts = Vec::new();
        for w in ["1", "4", "10"] {
            let out = dir.join(format!("h2h-{w}.json"));
            head_to_head(extra, w, &out);
            texts.push(std::fs::read(&out).expect("report"));
        }
        assert_eq!(texts[0], texts[1], "{extra:?}: 1 vs 4 workers");
        assert_eq!(texts[0], texts[2], "{extra:?}: 1 vs 10 workers");
    }
}

/// T-149 on real games: with `--choose-first seat0`, seat 0 goes first in every game, so each
/// agent is on the play in exactly half the games.
#[test]
fn seat0_realises_the_play_draw_balance() {
    at_root();
    let dir = scratch("h2h-seat0");
    let doc = head_to_head(
        &[
            "greedy", "random", "--decks", "tuning", "--games", "36", "--seed", "3",
        ],
        "4",
        &dir.join("seat0.json"),
    );
    for o in doc["outcomes"].as_array().expect("outcomes") {
        assert_eq!(o[5], 0, "first_player of {o}");
    }
    for side in ["challenger", "opponent"] {
        let pd = &doc["result"]["play_draw"][side];
        assert_eq!(
            (pd["on_play"]["n"].as_u64(), pd["on_draw"]["n"].as_u64()),
            (Some(18), Some(18))
        );
    }
    for row in doc["by_pair"].as_array().expect("by_pair") {
        let pd = &row["result"]["play_draw"]["challenger"];
        assert_eq!(
            (pd["on_play"]["n"].as_u64(), pd["on_draw"]["n"].as_u64()),
            (Some(6), Some(6))
        );
    }
}

fn reference_benchmark(dir: &Path, extra: &[&str]) -> Value {
    let mut args = os(&[
        "reference-benchmark",
        "examples/decks/red-green-zeon.txt",
        "examples/decks/blue-white-federation.txt",
        "--matches",
        "2",
        "--seed",
        "1",
    ]);
    args.extend(os(extra));
    args.push("--out".into());
    args.push(dir.as_os_str().to_owned());
    let (status, out, err) = main_of(&args);
    assert_eq!(status, 0, "{args:?}: {err}");
    assert!(
        out.starts_with("red-green-zeon (deck under test) vs blue-white-federation (benchmark)\n")
    );
    for replay in std::fs::read_dir(dir.join("replays")).expect("replays") {
        let path = replay.expect("entry").path();
        let check = gcg_runner::replay::check_replay(&read_json(&path)).expect("a replay");
        assert!(check.ok, "{}: {:?}", path.display(), check.mismatches);
    }
    read_json(&dir.join("results.json"))
}

/// The M8 smoke run (T-082): Python's preset name, the reference agents, schema 1.1.0 and the
/// reference factory string; replays verify.
#[test]
fn reference_benchmark_writes_pythons_names_and_schema() {
    at_root();
    let dir = scratch("ref-fast");
    let doc = reference_benchmark(&dir, &["--reference-preset", "fast", "--decision-log"]);
    assert_eq!(doc["schema"], "urn:gcg-sim:schema:results:1.1.0");
    assert_eq!(doc["ai"]["preset"], "fast");
    assert_eq!(doc["ai"]["factory"], gcg_ai_reference::FACTORY_DESCRIPTION);
    let games = std::fs::read_to_string(dir.join("games.ndjson")).expect("games.ndjson");
    for line in games.lines() {
        let g: Value = serde_json::from_str(line).expect("a game");
        assert_eq!(g["agents"], serde_json::json!(["mcts-fast", "mcts-fast"]));
        assert!(g["decision_log"].is_array());
    }
    let dir = scratch("ref-test-random");
    let doc = reference_benchmark(&dir, &["--agent", "test-random", "--format", "bo1"]);
    assert_eq!(doc["ai"]["preset"], "standard");
    assert_eq!(doc["ai"]["factory"], "random_agent.py:make_random_agent");
}

#[test]
fn preset_benchmark_runs_a_configuration_file() {
    at_root();
    let dir = scratch("preset");
    let config = gcg_search::preset_config(gcg_search::Preset::Fast);
    let file = dir.join("candidate.json");
    let text = gcg_core::pyjson::dumps(
        &gcg_eval::search_config::to_json(config),
        gcg_core::pyjson::DumpOptions::PRETTY_SORTED_UTF8,
    );
    std::fs::write(&file, text).expect("write the configuration");
    let out = dir.join("out");
    let mut args = os(&[
        "preset-benchmark",
        "examples/decks/red-green-zeon.txt",
        "examples/decks/blue-white-federation.txt",
        "--matches",
        "1",
        "--ai-preset",
        "fast",
        "--format",
        "bo1",
        "--search-config",
    ]);
    args.push(file.into_os_string());
    args.push("--out".into());
    args.push(out.clone().into_os_string());
    let (status, _, err) = main_of(&args);
    assert_eq!(status, 0, "{err}");
    let doc = read_json(&out.join("results.json"));
    assert_eq!(doc["ai"]["preset"], "fast");
    let games = std::fs::read_to_string(out.join("games.ndjson")).expect("games.ndjson");
    let first: Value = serde_json::from_str(games.lines().next().unwrap_or("{}")).expect("game");
    let name = format!("search-{}", config.name);
    assert_eq!(first["agents"], serde_json::json!([name, name]));
}
