//! `gcg-sim benchmark` and `replay` end to end through the binary (T-003, T-004, T-078;
//! `tests/cli/test_cli.py:137-156`, `:274-301`, `tests/cli/test_console_script.py:45-87`),
//! with the shipped `fast` preset: the summary on stdout, the progress on stderr, the reports,
//! identical deterministic files for any `--workers` (1, 4, 10 and `auto`), a report write
//! failure that names the file, and every saved replay verified in a fresh process.
//!
//! These play real games, so they need the engine's game layer and the search
//! (`needs_engine_`).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const FED: &str = "examples/decks/blue-white-federation.txt";
const ZEON: &str = "examples/decks/red-green-zeon.txt";
const TEK: &str = "examples/decks/purple-white-tekkadan.txt";
const DETERMINISTIC: [&str; 3] = ["results.json", "games.ndjson", "summary.md"];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn tmp(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    dir
}

fn gcg_sim(args: &[&str], out: Option<&Path>) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_gcg-sim"));
    cmd.args(args).current_dir(root()).env_remove("COLUMNS");
    if let Some(out) = out {
        cmd.arg("--out").arg(out);
    }
    cmd.output().expect("gcg-sim runs")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).expect("UTF-8")
}

#[test]
fn needs_engine_benchmark_writes_reports_and_prints_a_summary() {
    let out = tmp("summary");
    let done = gcg_sim(
        &[
            "benchmark",
            FED,
            ZEON,
            "--matches",
            "3",
            "--format",
            "bo1",
            "--ai-preset",
            "fast",
            "--seed",
            "9",
            "--replays",
            "1",
            "--decision-log",
        ],
        Some(&out),
    );
    assert_eq!(done.status.code(), Some(0), "{}", text(&done.stderr));
    let stdout = text(&done.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(
        lines[0],
        "blue-white-federation (deck under test) vs red-green-zeon (benchmark)"
    );
    assert!(lines[1].starts_with("matches: DUT "));
    assert!(lines[2].starts_with("games: DUT "));
    let shown = gcg_data::py_path_str(&out.join("results.json"));
    assert!(
        lines.contains(&format!("  results  {shown}").as_str()),
        "{stdout}"
    );
    assert_eq!(
        text(&done.stderr),
        "matches 1/3\nmatches 2/3\nmatches 3/3\n"
    );
    let results: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(out.join("results.json")).expect("results"))
            .expect("JSON");
    assert_eq!(results["config"]["matches"], 3);
    assert_eq!(results["ai"]["decision_log"], true);
    assert_eq!(results["ai"]["preset"], "fast");
    assert_eq!(results["ai"]["dut_agent"], "search-fast");
    assert_eq!(results["ai"]["bench_agent"], "search-fast");
    assert_eq!(results["ai"]["factory"], "gcg-search::make_agent('fast')");
    let replay = &results["replays"][0]["file"];
    let doc: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(out.join(replay.as_str().expect("a file name"))).expect("replay"),
    )
    .expect("JSON");
    assert!(
        doc["decision_log"]
            .as_array()
            .is_some_and(|l| !l.is_empty())
    );
    let _ = fs::remove_dir_all(&out);
}

#[test]
fn needs_engine_outputs_are_identical_for_any_worker_count() {
    let mut first: Option<(PathBuf, Vec<Vec<u8>>)> = None;
    for workers in ["1", "4", "10", "auto"] {
        let out = tmp(&format!("w{workers}"));
        let done = gcg_sim(
            &[
                "benchmark",
                TEK,
                FED,
                "--matches",
                "4",
                "--seed",
                "77",
                "--format",
                "bo1",
                "--ai-preset",
                "fast",
                "--workers",
                workers,
            ],
            Some(&out),
        );
        assert_eq!(done.status.code(), Some(0), "{}", text(&done.stderr));
        let mut files: Vec<Vec<u8>> = DETERMINISTIC
            .iter()
            .map(|n| fs::read(out.join(n)).expect("a report"))
            .collect();
        let mut replays: Vec<PathBuf> = fs::read_dir(out.join("replays"))
            .expect("replays")
            .map(|e| e.expect("entry").path())
            .collect();
        replays.sort();
        for r in &replays {
            files.push(r.file_name().expect("name").as_encoded_bytes().to_vec());
            files.push(fs::read(r).expect("replay"));
        }
        // timing.json is the one file that records the run itself: the workers that played
        // (tests/cli/test_console_script.py:62-64).
        let timing: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(out.join("timing.json")).expect("timing"))
                .expect("JSON");
        let want_workers = match workers {
            "auto" => gcg_core::sys::topology::detect().logical_cpus.min(4),
            n => n.parse::<usize>().expect("a count").min(4),
        };
        assert_eq!(timing["workers"], want_workers, "--workers {workers}");
        match &first {
            None => first = Some((out, files)),
            Some((_, want)) => {
                assert!(
                    want == &files,
                    "--workers {workers} differs from --workers 1"
                );
                let _ = fs::remove_dir_all(&out);
            }
        }
    }
    if let Some((out, _)) = first {
        let _ = fs::remove_dir_all(out);
    }
}

#[test]
fn needs_engine_saved_replays_verify_in_a_fresh_process() {
    let out = tmp("replays");
    let done = gcg_sim(
        &[
            "benchmark",
            TEK,
            FED,
            "--matches",
            "3",
            "--seed",
            "77",
            "--format",
            "bo1",
            "--ai-preset",
            "fast",
            "--workers",
            "3",
        ],
        Some(&out),
    );
    assert_eq!(done.status.code(), Some(0), "{}", text(&done.stderr));
    let replays: Vec<PathBuf> = fs::read_dir(out.join("replays"))
        .expect("replays")
        .map(|e| e.expect("entry").path())
        .collect();
    assert!(!replays.is_empty());
    for path in replays {
        let checked = gcg_sim(&["replay", path.to_str().expect("UTF-8 path")], None);
        assert_eq!(checked.status.code(), Some(0), "{}", text(&checked.stderr));
        assert_eq!(
            text(&checked.stdout).lines().last(),
            Some("replay matches the recorded outcome and final state")
        );
        let logged = gcg_sim(
            &["replay", "--log", path.to_str().expect("UTF-8 path")],
            None,
        );
        let log = text(&logged.stdout);
        assert!(log.starts_with("T0 P"));
        assert!(
            log.lines()
                .next()
                .is_some_and(|l| l.contains("choose_first"))
        );
        assert!(log.lines().any(|l| l.contains(" game over: winner P")));
    }
    let _ = fs::remove_dir_all(&out);
}

#[test]
fn needs_engine_a_report_write_failure_names_the_file() {
    // tests/cli/test_cli.py:274-285: `replays` is a file, so its directory cannot be made.
    let out = tmp("write-failure");
    fs::create_dir_all(&out).expect("out dir");
    fs::write(out.join("replays"), "").expect("a file in the way");
    let done = gcg_sim(
        &[
            "benchmark",
            ZEON,
            FED,
            "--matches",
            "2",
            "--format",
            "bo1",
            "--ai-preset",
            "fast",
        ],
        Some(&out),
    );
    assert_eq!(done.status.code(), Some(1));
    let shown = gcg_data::py_path_str(&out);
    let err = text(&done.stderr);
    assert!(
        err.contains(&format!(
            "error: cannot write reports to {shown}: {shown}/replays"
        )),
        "{err}"
    );
    let _ = fs::remove_dir_all(&out);
}
