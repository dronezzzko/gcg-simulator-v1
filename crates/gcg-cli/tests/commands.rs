//! `gcg-sim` command transcripts against Python's (T-072, T-078; PLAN §10.4 E gate): for every
//! case of `tests/vectors/commands.json` (written by `port/oracle/export_cli.py` from
//! `gcg_sim.cli.main`, run from the repository root), the `gcg-sim` binary run the same way
//! gives the same exit status, stdout and stderr, byte for byte. Python's `versions.package`
//! and `versions.code_digest` (the `data status` header and JSON) are replaced by the Rust
//! build's values first; `prefix` cases agree up to CPython's `JSONDecodeError` text (PLAN
//! A19).
//!
//! The cases that re-simulate a game (`replay` of a saved replay) need the engine's game layer
//! and run in `needs_engine_replay_transcripts_match_python`.

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value as Json;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn commands() -> Json {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/vectors/commands.json");
    let text = std::fs::read_to_string(path).expect("tests/vectors/commands.json is readable");
    serde_json::from_str(&text).expect("tests/vectors/commands.json is JSON")
}

struct Transcript {
    exit: i32,
    stdout: String,
    stderr: String,
}

fn run(argv: &[&str]) -> Transcript {
    let done = Command::new(env!("CARGO_BIN_EXE_gcg-sim"))
        .args(argv)
        .current_dir(repo_root())
        .env_remove("COLUMNS")
        .output()
        .expect("gcg-sim runs");
    Transcript {
        exit: done.status.code().expect("gcg-sim exits with a status"),
        stdout: String::from_utf8(done.stdout).expect("UTF-8 stdout"),
        stderr: String::from_utf8(done.stderr).expect("UTF-8 stderr"),
    }
}

/// Python's text with its package version and code digest replaced by the Rust build's.
fn masked(v: &Json, text: &str) -> String {
    let package = common::str_of(&v["package"]);
    text.replace(common::str_of(&v["code_digest"]), gcg_cli::CODE_DIGEST)
        .replace(
            &format!("gcg-sim {package} (results schema"),
            &format!("gcg-sim {} (results schema", gcg_cli::PACKAGE_VERSION),
        )
        .replace(
            &format!("\"package\": \"{package}\""),
            &format!("\"package\": \"{}\"", gcg_cli::PACKAGE_VERSION),
        )
}

const JSON_ERROR: &str = " is not valid JSON: ";

fn check(v: &Json, engine: bool) -> usize {
    let mut checked = 0;
    for case in v["cases"].as_array().expect("cases") {
        if case["engine"].as_bool() != Some(engine) {
            continue;
        }
        let argv: Vec<&str> = case["argv"]
            .as_array()
            .expect("argv")
            .iter()
            .map(common::str_of)
            .collect();
        let got = run(&argv);
        let want_out = masked(v, common::str_of(&case["stdout"]));
        let want_err = masked(v, common::str_of(&case["stderr"]));
        assert_eq!(
            Some(i64::from(got.exit)),
            case["exit"].as_i64(),
            "{argv:?}: exit"
        );
        assert_eq!(got.stdout, want_out, "{argv:?}: stdout");
        match common::str_of(&case["relation"]) {
            "same" => assert_eq!(got.stderr, want_err, "{argv:?}: stderr"),
            "prefix" => {
                let cut = want_err.find(JSON_ERROR).expect("a JSON error") + JSON_ERROR.len();
                assert!(
                    got.stderr.starts_with(&want_err[..cut]),
                    "{argv:?}: {}",
                    got.stderr
                );
                assert!(got.stderr.ends_with('\n') && got.stderr.lines().count() == 1);
            }
            other => panic!("unknown relation {other}"),
        }
        checked += 1;
    }
    assert!(
        !repo_root()
            .join("crates/gcg-cli/tests/fixtures/never")
            .exists(),
        "a benchmark that fails its checks creates no report directory"
    );
    checked
}

#[test]
fn command_transcripts_match_python() {
    let v = commands();
    assert!(check(&v, false) >= 25);
}

#[test]
fn needs_engine_replay_transcripts_match_python() {
    let v = commands();
    assert!(check(&v, true) >= 9);
}

#[test]
fn the_masking_touches_only_the_version_fields() {
    // Negative control: the masked Python output differs from the raw one exactly where the
    // version strings are, and a changed byte elsewhere is still a difference.
    let v = commands();
    let case = v["cases"]
        .as_array()
        .expect("cases")
        .iter()
        .find(|c| c["argv"] == serde_json::json!(["data", "status", "--json"]))
        .expect("data status --json");
    let python = common::str_of(&case["stdout"]);
    let rust = run(&["data", "status", "--json"]).stdout;
    assert_eq!(masked(&v, python), rust);
    let tampered = python.replacen(
        "\"rules_version\": \"1.9.0\"",
        "\"rules_version\": \"1.9.1\"",
        1,
    );
    assert_ne!(tampered, python);
    assert_ne!(masked(&v, &tampered), rust);
}
