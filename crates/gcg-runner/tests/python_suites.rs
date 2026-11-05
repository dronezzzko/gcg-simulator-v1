//! The T-078 register (`tests/python_suites.json`): every test function of `tests/runner`,
//! `tests/reports`, `tests/cli` and `tests/deck`, as pytest collects them
//! (`tests/vectors/parity/python_suites.json`, `port/oracle/export_parity.py --section
//! collect`), is listed once, in collection order, with its Rust coverage:
//!
//! - `native`: Rust tests with the Python test's inputs and assertions;
//! - `transitive`: Rust reproduces Python's outputs byte for byte on the test's inputs (named
//!   vector files), so every assertion the test makes over them holds;
//! - `not-ported`: the test exercises something the Rust design cannot express, with the reason.
//!
//! Every Rust test named exists as a `#[test]` function in the named file, and every vector
//! path exists. Whether those tests pass is the job of the test run itself.

use std::path::{Path, PathBuf};

use serde_json::Value;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read_json(path: &Path) -> Value {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn register() -> Value {
    read_json(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/python_suites.json"))
}

/// The collected test functions, in collection order, once each.
fn collected_functions() -> Vec<String> {
    let v = read_json(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/vectors/parity/python_suites.json"),
    );
    let mut out: Vec<String> = Vec::new();
    for t in v["tests"].as_array().expect("tests") {
        let f = t["function"].as_str().expect("function").to_owned();
        if !out.contains(&f) {
            out.push(f);
        }
    }
    out
}

/// Whether `source` defines `fn name(` as a `#[test]`: the lines above it, past attributes and
/// comments, include `#[test]`.
fn defines_test(source: &str, name: &str) -> bool {
    let lines: Vec<&str> = source.lines().collect();
    let def = format!("fn {name}(");
    lines.iter().enumerate().any(|(i, line)| {
        if !line.trim_start().starts_with(&def) {
            return false;
        }
        lines[..i]
            .iter()
            .rev()
            .map(|l| l.trim())
            .take_while(|l| l.starts_with("#[") || l.starts_with("//") || l.ends_with(")]"))
            .any(|l| l == "#[test]")
    })
}

/// The problems of `register` against `functions` (empty: the register is complete and every
/// reference resolves).
fn problems(register: &Value, functions: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let entries = register["entries"]
        .as_array()
        .map_or(&[][..], Vec::as_slice);
    let listed: Vec<&str> = entries
        .iter()
        .map(|e| e["python"].as_str().unwrap_or(""))
        .collect();
    let want: Vec<&str> = functions.iter().map(String::as_str).collect();
    if listed != want {
        for f in &want {
            if !listed.contains(f) {
                out.push(format!("{f}: not in the register"));
            }
        }
        for f in &listed {
            if !want.contains(f) {
                out.push(format!("{f}: not a collected test"));
            }
        }
        if out.is_empty() {
            out.push("the register is not in collection order".to_owned());
        }
    }
    for e in entries {
        let python = e["python"].as_str().unwrap_or("?");
        let coverage = e["coverage"].as_str().unwrap_or("");
        let strs = |key: &str| -> Vec<&str> {
            e[key]
                .as_array()
                .map(|a| a.iter().filter_map(Value::as_str).collect())
                .unwrap_or_default()
        };
        let rust = strs("rust");
        let vectors = strs("vectors");
        let note = e["note"].as_str().unwrap_or("");
        match coverage {
            "native" if rust.is_empty() => {
                out.push(format!("{python}: native without a Rust test"))
            }
            "transitive" if rust.is_empty() || vectors.is_empty() || note.is_empty() => {
                out.push(format!(
                    "{python}: transitive needs Rust tests, vectors and a note"
                ));
            }
            "not-ported" if note.is_empty() => {
                out.push(format!("{python}: not-ported without a reason"))
            }
            "native" | "transitive" | "not-ported" => {}
            other => out.push(format!("{python}: unknown coverage {other:?}")),
        }
        for r in rust {
            let Some((file, name)) = r.split_once("::") else {
                out.push(format!("{python}: {r} is not FILE::FUNCTION"));
                continue;
            };
            match std::fs::read_to_string(root().join(file)) {
                Ok(source) if defines_test(&source, name) => {}
                Ok(_) => out.push(format!("{python}: no #[test] fn {name} in {file}")),
                Err(e) => out.push(format!("{python}: {file}: {e}")),
            }
        }
        for v in vectors {
            if !root().join(v).exists() {
                out.push(format!("{python}: vector path {v} does not exist"));
            }
        }
    }
    out
}

#[test]
fn every_python_test_of_the_four_suites_has_rust_coverage() {
    let functions = collected_functions();
    assert!(functions.len() >= 130, "{} functions", functions.len());
    for suite in [
        "tests/runner/",
        "tests/reports/",
        "tests/cli/",
        "tests/deck/",
    ] {
        assert!(functions.iter().any(|f| f.starts_with(suite)), "{suite}");
    }
    let reg = register();
    let found = problems(&reg, &functions);
    assert!(found.is_empty(), "{}", found.join("\n"));
    let count = |c: &str| {
        reg["entries"]
            .as_array()
            .expect("entries")
            .iter()
            .filter(|e| e["coverage"] == c)
            .count()
    };
    println!(
        "{} Python test functions: {} native, {} transitive, {} not ported",
        functions.len(),
        count("native"),
        count("transitive"),
        count("not-ported")
    );
    assert!(
        count("not-ported") <= 3,
        "a new not-ported entry needs review"
    );
}

#[test]
fn the_check_catches_a_missing_test_a_dangling_reference_and_a_bad_vector() {
    let functions = collected_functions();
    let reg = register();
    let mut missing = reg.clone();
    missing["entries"]
        .as_array_mut()
        .expect("entries")
        .remove(0);
    assert!(
        problems(&missing, &functions)
            .iter()
            .any(|p| p.ends_with("not in the register"))
    );
    let mut dangling = reg.clone();
    dangling["entries"][0]["rust"] =
        serde_json::json!(["crates/gcg-runner/src/config.rs::no_such_test"]);
    assert!(
        problems(&dangling, &functions)
            .iter()
            .any(|p| p.contains("no #[test] fn no_such_test"))
    );
    let mut not_a_test = reg.clone();
    not_a_test["entries"][0]["rust"] =
        serde_json::json!(["crates/gcg-runner/src/config.rs::game_seed"]);
    assert!(
        problems(&not_a_test, &functions)
            .iter()
            .any(|p| p.contains("no #[test] fn game_seed"))
    );
    let mut vector = reg.clone();
    let k = vector["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .position(|e| e["coverage"] == "transitive")
        .expect("a transitive entry");
    vector["entries"][k]["vectors"] = serde_json::json!(["crates/no/such/vector.json"]);
    assert!(
        problems(&vector, &functions)
            .iter()
            .any(|p| p.contains("does not exist"))
    );
    let mut reason = reg;
    let k = reason["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .position(|e| e["coverage"] == "not-ported")
        .expect("a not-ported entry");
    reason["entries"][k]["note"] = Value::from("");
    assert!(
        problems(&reason, &functions)
            .iter()
            .any(|p| p.ends_with("not-ported without a reason"))
    );
}

#[test]
fn slow_python_tests_run_natively() {
    let v = read_json(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/vectors/parity/python_suites.json"),
    );
    let reg = register();
    for t in v["tests"].as_array().expect("tests") {
        if t["slow"] == true {
            let f = t["function"].as_str().expect("function");
            let entry = reg["entries"]
                .as_array()
                .expect("entries")
                .iter()
                .find(|e| e["python"] == f)
                .expect("registered");
            assert_eq!(entry["coverage"], "native", "{f}");
        }
    }
}
