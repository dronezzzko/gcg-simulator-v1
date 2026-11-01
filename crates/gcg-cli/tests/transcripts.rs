//! CLI transcripts (T-072, T-152; PLAN A30): for every exported command line, the Rust
//! parser's exit status, stdout, stderr and namespace equal those of Python 3.15.1's argparse
//! running the Rust-shaped parser (Python's `build_parser()` with only `--ai-preset
//! standard|fast` and `--workers N|auto` changed), byte for byte, at every exported
//! `COLUMNS`, with a non-TTY stdout. Each case also carries its relation to the real Python
//! parser, which `port/oracle/export_cli.py` asserted and this test checks again:
//!
//! - `same`: the Rust transcript is the real Python transcript;
//! - `usage`: only the benchmark usage block differs (`--ai-preset {standard,fast}`); the
//!   error line is identical;
//! - `rust`: an intended difference of `--ai-preset` or `--workers auto`.
//!
//! `subprocess` cases were run through the installed console script and a child process
//! (unset and malformed `COLUMNS` included) and equal the in-process transcripts.

mod common;

use std::ffi::OsString;

use common::{os_arg, py_text, str_of, text_json, vectors};
use gcg_cli::cli::{parse_namespace, terminal_columns};
use gcg_runner::argparse::{Namespace, Value};
use serde_json::{Value as Json, json};

fn namespace_json(ns: &Namespace) -> Json {
    let mut map = serde_json::Map::new();
    for (dest, value) in ns.entries() {
        let entry = match value {
            Value::Bool(b) => json!(["bool", if *b { "True" } else { "False" }]),
            Value::Int(v) => json!(["int", v.to_decimal()]),
            Value::Path(p) => json!(["path", text_json(p)]),
            Value::Str(s) => json!(["str", text_json(s)]),
            Value::Auto => json!(["str", "auto"]),
            Value::None => json!(["none", ""]),
            Value::List(_) => panic!("gcg-sim has no append option ({dest})"),
        };
        map.insert((*dest).to_owned(), entry);
    }
    Json::Object(map)
}

fn transcript(args: &[OsString], columns: Option<&Json>) -> Json {
    let env = columns.and_then(Json::as_str).map(OsString::from);
    let width = terminal_columns(env.as_deref());
    match parse_namespace(args, width) {
        Ok(ns) => json!({"exit": 0, "stdout": "", "stderr": "", "namespace": namespace_json(&ns)}),
        Err(exit) => json!({
            "exit": exit.status,
            "stdout": String::from_utf8(exit.stdout).expect("stdout is UTF-8"),
            "stderr": String::from_utf8(exit.stderr).expect("stderr is UTF-8"),
            "namespace": null,
        }),
    }
}

fn last_line(v: &Json) -> &str {
    str_of(&v["stderr"]).lines().last().unwrap_or("")
}

#[test]
fn every_transcript_matches_python() {
    let v = vectors();
    let cases = v["transcripts"].as_array().expect("transcripts");
    assert!(cases.len() > 300, "{} transcripts", cases.len());
    let mut relations = [0_usize; 4];
    for case in cases {
        let args: Vec<OsString> = case["argv"]
            .as_array()
            .expect("argv")
            .iter()
            .map(os_arg)
            .collect();
        let relation = str_of(&case["relation"]);
        let mut want = case["rust"].clone();
        if relation == "subprocess" {
            want["namespace"] = Json::Null;
        }
        let got = transcript(&args, Some(&case["columns"]));
        assert_eq!(
            got, want,
            "argv {} at COLUMNS={}: Rust output differs from the Rust-shaped Python parser",
            case["argv"], case["columns"]
        );
        let python = &case["python"];
        match relation {
            "same" => {
                assert_eq!(python, "same", "{}", case["argv"]);
                relations[0] += 1;
            }
            "usage" => {
                assert_ne!(python, &want);
                assert_eq!(python["exit"], 2);
                assert_eq!(python["stdout"], "");
                assert!(str_of(&python["stderr"]).starts_with("usage: gcg-sim benchmark"));
                assert_eq!(last_line(python), last_line(&got), "{}", case["argv"]);
                relations[1] += 1;
            }
            "rust" => {
                assert!(python.is_object(), "{}", case["argv"]);
                assert_ne!(python, &want);
                relations[2] += 1;
            }
            "subprocess" => {
                assert!(python.is_object());
                relations[3] += 1;
            }
            other => panic!("unknown relation {other}"),
        }
    }
    assert!(relations.iter().all(|&n| n > 0), "{relations:?}");
}

#[test]
fn the_documented_differences_are_the_only_ones_in_the_benchmark_help() {
    // At 80 columns the benchmark help differs from Python's in exactly the --workers and
    // --ai-preset entries and the usage lines that name --ai-preset.
    let v = vectors();
    let case = v["transcripts"]
        .as_array()
        .expect("transcripts")
        .iter()
        .find(|c| c["argv"] == json!(["benchmark", "--help"]) && c["columns"] == "80")
        .expect("benchmark --help at 80 columns");
    let rust = str_of(&case["rust"]["stdout"]);
    let python = str_of(&case["python"]["stdout"]);
    let keep = |text: &str| -> Vec<String> {
        text.lines()
            .filter(|l| !l.contains("--ai-preset") && !l.starts_with("                        "))
            .filter(|l| !l.contains("--workers"))
            .map(str::to_owned)
            .collect()
    };
    assert_eq!(keep(rust), keep(python));
    assert!(rust.contains("--ai-preset {standard,fast}"));
    assert!(!rust.contains("strong}"));
    assert!(python.contains("--ai-preset {fast,standard,strong}"));
}

#[test]
fn the_comparison_sees_a_one_column_difference() {
    // Negative control: a narrower terminal rewraps the help, and the comparison notices (79
    // columns happen to wrap this text as 80 do, 81 do not).
    let v = vectors();
    let at = |columns: &str| {
        v["transcripts"]
            .as_array()
            .expect("transcripts")
            .iter()
            .find(|c| c["argv"] == json!(["benchmark", "--help"]) && c["columns"] == columns)
            .map(|c| c["rust"].clone())
            .expect("exported width")
    };
    let args = [OsString::from("benchmark"), OsString::from("--help")];
    assert_eq!(transcript(&args, Some(&json!("80"))), at("80"));
    assert_ne!(transcript(&args, Some(&json!("60"))), at("80"));
    assert_eq!(transcript(&args, Some(&json!("60"))), at("60"));
    assert_eq!(at("79"), at("80"));
    assert_ne!(at("81"), at("80"));
    assert_ne!(transcript(&args, Some(&json!("81"))), at("80"));
}

#[test]
fn texts_with_lone_surrogates_round_trip_through_the_vectors() {
    let s = py_text(&json!({"cps": [0x61, 0xDCFF]}));
    assert_eq!(s.to_surrogateescape_bytes(), b"a\xff");
    assert_eq!(text_json(&s), json!({"cps": [0x61, 0xDCFF]}));
}
