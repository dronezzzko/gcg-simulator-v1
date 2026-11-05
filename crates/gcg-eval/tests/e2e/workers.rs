//! Part of the `e2e` target (feature `slow`, A12). Identical outputs at `--workers 1`, `4` and
//! `10` for every player (T-003, T-080; PLAN §7.2, §8.8, §9.6): the reference search and the
//! test agents through `gcg-eval reference-benchmark` (every report file except `timing.json`,
//! replays included), and the reference search, the strengthened search at both shipped
//! presets, greedy and random through `gcg-eval head-to-head` (the `--out` report with every
//! game), each run in a fresh `gcg-eval` process. `gcg-sim benchmark` with the shipped `fast`
//! preset is checked at 1, 4, 10 and `auto` workers in `crates/gcg-cli/tests/benchmark.rs`.

use std::path::{Path, PathBuf};
use std::process::Command;

const WORKERS: [&str; 3] = ["1", "4", "10"];
const ZEON: &str = "examples/decks/red-green-zeon.txt";
const FED: &str = "examples/decks/blue-white-federation.txt";
const TEK: &str = "examples/decks/purple-white-tekkadan.txt";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("workers-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn gcg_eval(args: &[&str], out: &Path) {
    let done = Command::new(env!("CARGO_BIN_EXE_gcg-eval"))
        .args(args)
        .arg("--out")
        .arg(out)
        .current_dir(root())
        .env_remove("COLUMNS")
        .output()
        .expect("gcg-eval runs");
    assert_eq!(
        done.status.code(),
        Some(0),
        "{args:?}: {}",
        String::from_utf8_lossy(&done.stderr)
    );
}

/// Every file of a report directory except `timing.json`, as (relative path, bytes).
fn report_tree(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    for name in ["results.json", "summary.md", "games.ndjson"] {
        out.push((name.to_owned(), std::fs::read(dir.join(name)).expect(name)));
    }
    let mut replays: Vec<PathBuf> = std::fs::read_dir(dir.join("replays"))
        .expect("replays")
        .map(|e| e.expect("entry").path())
        .collect();
    replays.sort();
    assert!(!replays.is_empty(), "{}: no replays", dir.display());
    for p in replays {
        let name = p.file_name().expect("name").to_string_lossy().into_owned();
        out.push((
            format!("replays/{name}"),
            std::fs::read(&p).expect("replay"),
        ));
    }
    out
}

fn timing_workers(dir: &Path) -> serde_json::Value {
    let text = std::fs::read_to_string(dir.join("timing.json")).expect("timing.json");
    let doc: serde_json::Value = serde_json::from_str(&text).expect("JSON");
    doc["workers"].clone()
}

#[test]
fn reference_benchmark_is_identical_at_1_4_and_10_workers() {
    let runs: [(&str, &[&str]); 3] = [
        (
            "ref-fast",
            &[
                ZEON,
                FED,
                "--matches",
                "4",
                "--seed",
                "21",
                "--format",
                "bo1",
                "--reference-preset",
                "fast",
            ],
        ),
        (
            "ref-standard-bo3",
            &[
                TEK,
                ZEON,
                "--matches",
                "2",
                "--seed",
                "22",
                "--reference-preset",
                "standard",
                "--decision-log",
            ],
        ),
        (
            "test-random",
            &[
                FED,
                TEK,
                "--matches",
                "24",
                "--seed",
                "23",
                "--agent",
                "test-random",
            ],
        ),
    ];
    for (id, args) in runs {
        let dir = scratch(id);
        let mut first: Option<Vec<(String, Vec<u8>)>> = None;
        for w in WORKERS {
            let out = dir.join(format!("w{w}"));
            let mut argv = vec!["reference-benchmark"];
            argv.extend_from_slice(args);
            argv.extend_from_slice(&["--workers", w]);
            gcg_eval(&argv, &out);
            let matches: u64 = args[3].parse().expect("--matches");
            let want = w.parse::<u64>().expect("count").min(matches);
            assert_eq!(timing_workers(&out), want, "{id} at --workers {w}");
            let tree = report_tree(&out);
            match &first {
                None => first = Some(tree),
                Some(one) => assert!(one == &tree, "{id}: --workers {w} differs from --workers 1"),
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn head_to_head_is_identical_at_1_4_and_10_workers_for_every_player() {
    let runs: [(&str, &[&str]); 5] = [
        (
            "ref-greedy",
            &["ref:fast", "greedy", "--games", "8", "--seed", "31"],
        ),
        (
            "ref-standard",
            &["ref:standard", "random", "--games", "4", "--seed", "32"],
        ),
        (
            "search-fast",
            &["search:fast", "greedy", "--games", "8", "--seed", "33"],
        ),
        (
            "search-standard",
            &["search:standard", "random", "--games", "4", "--seed", "34"],
        ),
        (
            "random-greedy",
            &[
                "random",
                "greedy",
                "--games",
                "24",
                "--seed",
                "35",
                "--decks",
                "tuning",
                "--choose-first",
                "agents",
            ],
        ),
    ];
    for (id, args) in runs {
        let dir = scratch(id);
        let mut first: Option<Vec<u8>> = None;
        for w in WORKERS {
            let out = dir.join(format!("w{w}.json"));
            let mut argv = vec!["head-to-head"];
            argv.extend_from_slice(args);
            argv.extend_from_slice(&["--workers", w]);
            gcg_eval(&argv, &out);
            let report = std::fs::read(&out).expect("report");
            match &first {
                None => {
                    let doc: serde_json::Value = serde_json::from_slice(&report).expect("JSON");
                    assert!(
                        doc["outcomes"].as_array().is_some_and(|o| !o.is_empty()),
                        "{id}: the report lists its games"
                    );
                    first = Some(report);
                }
                Some(one) => assert!(
                    one == &report,
                    "{id}: --workers {w} differs from --workers 1"
                ),
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// T-079 at the M9 gate's three configurations: `gcg-eval compare` in a fresh process writes
/// the stdout and `--out` bytes of `python -m gcg_sim.ai.compare` with the same arguments
/// (`oracle/export_parity.py --section compare`).
#[test]
fn compare_processes_equal_python_at_the_gate_configurations() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/vectors/parity/compare.json");
    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).expect("compare.json")).expect("JSON");
    let cases = v["cases"].as_array().expect("cases");
    assert_eq!(cases.len(), 3);
    let dir = scratch("compare-gate");
    for (k, case) in cases.iter().enumerate() {
        let args: Vec<&str> = case["argv"]
            .as_array()
            .expect("argv")
            .iter()
            .map(|a| a.as_str().expect("argument"))
            .collect();
        let out = dir.join(format!("report-{k}.json"));
        let done = Command::new(env!("CARGO_BIN_EXE_gcg-eval"))
            .arg("compare")
            .args(&args)
            .arg("--out")
            .arg(&out)
            .current_dir(root())
            .env_remove("COLUMNS")
            .output()
            .expect("gcg-eval runs");
        assert_eq!(done.status.code(), Some(0), "{args:?}");
        assert_eq!(
            String::from_utf8(done.stdout).expect("UTF-8"),
            case["stdout"].as_str().expect("stdout"),
            "{args:?}: stdout"
        );
        assert_eq!(
            std::fs::read_to_string(&out).expect("--out"),
            case["out"].as_str().expect("out"),
            "{args:?}: --out"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}
