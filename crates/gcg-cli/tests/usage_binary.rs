//! Help and usage errors of the `gcg-sim` process (T-152, T-005; PLAN A30): every exported
//! transcript that ends in the parser (help, `--version`-less usage errors, invalid choices),
//! run through the binary with its `COLUMNS`, prints exactly the in-process parser's output on
//! the same stream with the same exit status. `tests/transcripts.rs` compares that parser
//! with Python 3.15.1's argparse case by case; this test covers `main`: the `COLUMNS` lookup,
//! the stream each text goes to and the exit codes. The help of every subcommand (`gcg-sim`,
//! `benchmark`, `validate`, `data`, `data status`, `replay`) and a usage error of every
//! subcommand are among the cases.

mod common;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use common::{os_arg, str_of, vectors};
use serde_json::Value as Json;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Cases the parser decides on its own: help (exit 0 with stdout) or a usage error (exit 2).
fn parser_cases(v: &Json) -> Vec<&Json> {
    v["transcripts"]
        .as_array()
        .expect("transcripts")
        .iter()
        .filter(|c| c["rust"]["namespace"].is_null())
        .collect()
}

#[test]
fn the_binary_prints_the_parsers_help_and_usage_errors() {
    let v = vectors();
    let cases = parser_cases(&v);
    assert!(cases.len() > 200, "{} parser cases", cases.len());
    for case in cases {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_gcg-sim"));
        cmd.current_dir(root()).env_remove("COLUMNS");
        for a in case["argv"].as_array().expect("argv") {
            cmd.arg(os_arg(a));
        }
        if let Some(columns) = case["columns"].as_str() {
            cmd.env("COLUMNS", columns);
        }
        let done = cmd.output().expect("gcg-sim runs");
        let want = &case["rust"];
        let what = format!("argv {} at COLUMNS={}", case["argv"], case["columns"]);
        assert_eq!(
            done.status.code().map(i64::from),
            want["exit"].as_i64(),
            "{what}"
        );
        assert_eq!(
            String::from_utf8(done.stdout).expect("UTF-8"),
            str_of(&want["stdout"]),
            "{what}: stdout"
        );
        assert_eq!(
            String::from_utf8(done.stderr).expect("UTF-8"),
            str_of(&want["stderr"]),
            "{what}: stderr"
        );
    }
}

fn argv_strings(case: &Json) -> Vec<String> {
    case["argv"]
        .as_array()
        .expect("argv")
        .iter()
        .map(|a| a.as_str().unwrap_or("<bytes>").to_owned())
        .collect()
}

#[test]
fn every_subcommand_has_its_help_and_a_usage_error_among_the_cases() {
    let v = vectors();
    let cases = parser_cases(&v);
    let helps: BTreeSet<Vec<String>> = cases
        .iter()
        .filter(|c| c["rust"]["exit"] == 0 && c["columns"] == "80")
        .map(|c| argv_strings(c))
        .collect();
    for path in [
        &["--help"][..],
        &["benchmark", "--help"],
        &["validate", "--help"],
        &["data", "--help"],
        &["data", "status", "--help"],
        &["replay", "--help"],
    ] {
        let path: Vec<String> = path.iter().map(|s| (*s).to_owned()).collect();
        assert!(helps.contains(&path), "no help transcript for {path:?}");
    }
    let errors: Vec<Vec<String>> = cases
        .iter()
        .filter(|c| c["rust"]["exit"] == 2)
        .map(|c| argv_strings(c))
        .collect();
    for (command, usage) in [
        (None, "usage: gcg-sim [-h]"),
        (Some("benchmark"), "usage: gcg-sim benchmark"),
        (Some("validate"), "usage: gcg-sim validate"),
        (Some("data"), "usage: gcg-sim data"),
        (Some("replay"), "usage: gcg-sim replay"),
    ] {
        let found = cases.iter().any(|c| {
            c["rust"]["exit"] == 2
                && str_of(&c["rust"]["stderr"]).starts_with(usage)
                && command
                    .is_none_or(|name| argv_strings(c).first().map(String::as_str) == Some(name))
        });
        assert!(
            found,
            "no usage error of {command:?} ({} errors)",
            errors.len()
        );
    }
    assert!(
        errors
            .iter()
            .any(|a| a.len() >= 2 && a[0] == "data" && a[1] == "status"),
        "a usage error of data status"
    );
}

/// T-005 at the process level: `--ai-preset` accepts exactly `standard` and `fast`; `strong`
/// (Python's third preset) and any other value is argparse's invalid-choice usage error, exit 2,
/// nothing on stdout. (`tests/presets.rs` checks the parsed default; `tests/replays.rs` runs
/// the default preset end to end and finds `standard` in `results.json`.)
#[test]
fn only_standard_and_fast_reach_the_benchmark() {
    let d1 = "examples/decks/blue-white-federation.txt";
    let d2 = "examples/decks/red-green-zeon.txt";
    for bad in [
        "strong",
        "Strong",
        "STANDARD",
        "",
        "mcts-standard",
        "reference",
        "fast ",
    ] {
        let done = Command::new(env!("CARGO_BIN_EXE_gcg-sim"))
            .args(["benchmark", d1, d2, "--matches", "1", "--ai-preset", bad])
            .current_dir(root())
            .env_remove("COLUMNS")
            .output()
            .expect("gcg-sim runs");
        assert_eq!(done.status.code(), Some(2), "--ai-preset {bad:?}");
        assert!(done.stdout.is_empty());
        let err = String::from_utf8(done.stderr).expect("UTF-8");
        assert!(err.starts_with("usage: gcg-sim benchmark"), "{err}");
        let last = err.lines().last().unwrap_or_default();
        let repr = gcg_data::py_repr(bad);
        assert!(
            last.starts_with(&format!(
                "gcg-sim benchmark: error: argument --ai-preset: invalid choice: {repr}"
            )) && last.ends_with("(choose from 'standard', 'fast')"),
            "{last}"
        );
    }
    let strong = Command::new(env!("CARGO_BIN_EXE_gcg-sim"))
        .args([
            "benchmark",
            d1,
            d2,
            "--matches",
            "1",
            "--ai-preset",
            "strong",
        ])
        .current_dir(root())
        .env_remove("COLUMNS")
        .output()
        .expect("gcg-sim runs");
    assert_eq!(
        String::from_utf8(strong.stderr)
            .expect("UTF-8")
            .lines()
            .last(),
        Some(
            "gcg-sim benchmark: error: argument --ai-preset: invalid choice: 'strong' (choose \
             from 'standard', 'fast')"
        )
    );
}
