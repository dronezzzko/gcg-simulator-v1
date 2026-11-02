//! `gcg-eval reference-benchmark` against Python's `gcg-sim benchmark`, through both command
//! lines (`port/oracle/export_parity.py`, sections `test-random` and `reference`):
//!
//! - T-073: `--agent test-random` against Python with
//!   `GCG_SIM_AGENT_FACTORY=tests/runner/random_agent.py:make_random_agent`, in the
//!   `tests/reports/conftest.py` configuration, a 12-match run (Python at 2 workers, Rust at 4)
//!   and a BO1 run with decision logs, the largest seed and Python's preset name `strong`;
//! - T-082: the reference search at each of Python's presets (`--reference-preset
//!   fast|standard|strong` against `--ai-preset fast|standard|strong`), one BO1 game each.
//!
//! The exit status, stdout and stderr equal Python's (the report directory is `{OUT}` in
//! both), the binary writes the same report files, and `results.json`, `summary.md`,
//! `games.ndjson` and every replay are Python's bytes once the three implementation fields of
//! `scripts/normalize_outputs.py` (`versions.package`, `versions.code_digest`, `ai.factory`) are
//! given Python's values. Files are compared by SHA-256 (the first 64 bits, through
//! `gcg_core::stable_hash`). The preset and agent names, the schema id and the schema
//! versions are compared as values too, and every `results.json` validates against 1.1.0.
//!
//! T-004 (Rust to Python) for `gcg-eval`: the replays of the `gcg-eval` run in
//! `crates/gcg-cli/tests/vectors/parity/rust-replays.json` are the files that Python's `gcg-sim
//! replay` verified, and the replay command (`gcg_runner::cli_support::replay_command`, the
//! body of `gcg-sim replay`) prints Python's transcripts for them.

use std::path::{Path, PathBuf};
use std::process::Command;

use gcg_core::pyjson::{DumpOptions, Json, dumps};
use gcg_core::stable_hash::stable_key;
use gcg_runner::SchemaVersion;
use gcg_runner::reports::{json_from_value, validate_results};
use serde_json::Value;

const OUT_TOKEN: &str = "{OUT}";
const REPORT_FILES: [&str; 3] = ["results.json", "summary.md", "games.ndjson"];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn vectors(name: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/vectors/parity")
        .join(name);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("parity-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

/// The first 16 hex digits of the SHA-256 of `bytes`.
fn sha_prefix(bytes: &[u8]) -> String {
    format!("{:016x}", stable_key(bytes).0)
}

fn str_of<'a>(v: &'a Value, what: &str) -> &'a str {
    v.as_str().unwrap_or_else(|| panic!("{what} is a string"))
}

/// `text` with the single occurrence of `old` replaced by `new`.
fn replace_once(text: &str, old: &str, new: &str, what: &str) -> String {
    assert_eq!(text.matches(old).count(), 1, "{what}: {old:?}");
    text.replacen(old, new, 1)
}

/// The Rust run's three implementation fields, as `(results.json key, Rust value, Python
/// value)`.
fn masks(results: &Value, python: &Value) -> [(&'static str, String, String); 3] {
    let get = |v: &Value, a: &str, b: &str| str_of(&v[a][b], b).to_owned();
    [
        (
            "package",
            get(results, "versions", "package"),
            str_of(&python["package"], "package").to_owned(),
        ),
        (
            "code_digest",
            get(results, "versions", "code_digest"),
            str_of(&python["code_digest"], "code_digest").to_owned(),
        ),
        (
            "factory",
            get(results, "ai", "factory"),
            str_of(&python["factory"], "factory").to_owned(),
        ),
    ]
}

/// A string as `to_json_text` writes it (`reports/writer.py:33-34`).
fn json_str(s: &str) -> String {
    dumps(&Json::from(s), DumpOptions::PRETTY_SORTED_UTF8)
}

/// `scripts/normalize_outputs.py` with Python's values in place of the placeholder:
/// `results.json` members and their two renderings in `summary.md`.
fn normalized(name: &str, text: &str, masks: &[(&'static str, String, String); 3]) -> String {
    let mut text = text.to_owned();
    for (key, rust, python) in masks {
        if rust == python {
            continue;
        }
        let (py_json, rust_json) = (json_str(python), json_str(rust));
        text = match (name, *key) {
            ("results.json", _) => replace_once(
                &text,
                &format!("\"{key}\": {rust_json}"),
                &format!("\"{key}\": {py_json}"),
                name,
            ),
            ("summary.md", "factory") => replace_once(
                &text,
                &format!("factory `{rust}`)."),
                &format!("factory `{python}`)."),
                name,
            ),
            ("summary.md", "package") => replace_once(
                &text,
                &format!("\ngcg-sim {rust}; results schema "),
                &format!("\ngcg-sim {python}; results schema "),
                name,
            ),
            _ => text,
        };
    }
    text
}

fn names(dir: &Path) -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|e| e.expect("entry").file_name().into_string().expect("UTF-8"))
        .collect();
    out.sort();
    out
}

/// Runs one exported case and compares everything with Python's.
fn check_case(case: &Value) {
    let id = str_of(&case["id"], "id");
    let dir = scratch(id);
    let out = dir.join("out");
    let shown = gcg_data::py_path_str(&out);
    let argv: Vec<String> = case["rust_argv"]
        .as_array()
        .expect("rust_argv")
        .iter()
        .skip(1)
        .map(|a| {
            let a = str_of(a, "argument");
            if a == OUT_TOKEN {
                shown.clone()
            } else {
                a.to_owned()
            }
        })
        .collect();
    let done = Command::new(env!("CARGO_BIN_EXE_gcg-eval"))
        .args(&argv)
        .current_dir(root())
        .env_remove("COLUMNS")
        .output()
        .expect("gcg-eval runs");
    let stdout = String::from_utf8(done.stdout).expect("UTF-8");
    let stderr = String::from_utf8(done.stderr).expect("UTF-8");
    assert_eq!(
        done.status.code(),
        case["exit"].as_i64().and_then(|c| i32::try_from(c).ok())
    );
    assert_eq!(
        stdout.replace(&shown, OUT_TOKEN),
        str_of(&case["stdout"], "stdout"),
        "{id}"
    );
    assert_eq!(
        stderr.replace(&shown, OUT_TOKEN),
        str_of(&case["stderr"], "stderr"),
        "{id}"
    );
    assert_eq!(
        names(&out),
        [
            "games.ndjson",
            "replays",
            "results.json",
            "summary.md",
            "timing.json"
        ],
        "{id}: the report files"
    );

    let results_text = std::fs::read_to_string(out.join("results.json")).expect("results.json");
    let results: Value = serde_json::from_str(&results_text).expect("JSON");
    assert_eq!(
        validate_results(&json_from_value(&results), SchemaVersion::V1_1),
        Ok(()),
        "{id}"
    );
    for (key, rust) in [
        ("ai", &results["ai"]),
        ("schema", &results["schema"]),
        ("schema_version", &results["schema_version"]),
        ("results_schema", &results["versions"]["results_schema"]),
    ] {
        let mut want = case[key].clone();
        let mut got = rust.clone();
        if key == "ai" {
            // ai.factory is an implementation field; every other ai value is Python's.
            want["factory"] = Value::Null;
            got["factory"] = Value::Null;
        }
        assert_eq!(got, want, "{id}: {key}");
    }
    let masks = masks(&results, &case["masked"]);
    for name in REPORT_FILES {
        let text = std::fs::read_to_string(out.join(name)).expect(name);
        let text = normalized(name, &text, &masks);
        assert_eq!(
            sha_prefix(text.as_bytes()),
            str_of(&case["files"][name], name)[..16],
            "{id}: {name} differs from Python's (Rust's is in {})",
            out.display()
        );
    }
    let want: Vec<String> = case["replays"]
        .as_object()
        .expect("replays")
        .keys()
        .cloned()
        .collect();
    assert_eq!(names(&out.join("replays")), want, "{id}: replay files");
    for (name, sha) in case["replays"].as_object().expect("replays") {
        let path = out.join("replays").join(name);
        let bytes = std::fs::read(&path).expect("replay");
        assert_eq!(
            sha_prefix(&bytes),
            str_of(sha, "sha256")[..16],
            "{id}: {name} differs from Python's"
        );
        let doc: Value = serde_json::from_slice(&bytes).expect("replay JSON");
        let check = gcg_runner::replay::check_replay(&doc).expect("a replay document");
        assert!(check.ok, "{id}/{name}: {:?}", check.mismatches);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

fn cases(name: &str) -> Vec<Value> {
    vectors(name)["cases"].as_array().expect("cases").clone()
}

#[test]
fn test_random_runs_equal_python_with_the_factory_hook() {
    let cases = cases("test-random.json");
    assert_eq!(cases.len(), 3);
    for case in &cases {
        assert_eq!(
            case["factory_env"],
            "tests/runner/random_agent.py:make_random_agent"
        );
        assert_eq!(case["ai"]["dut_agent"], "test-random");
        check_case(case);
    }
}

/// `tests/runner/test_config.py::test_runner_presets_are_the_ai_presets`: the presets a
/// benchmark configuration accepts are the reference search's, in `PRESETS` order.
#[test]
fn runner_presets_are_the_reference_presets() {
    assert_eq!(
        gcg_runner::config::AI_PRESETS,
        gcg_ai_reference::PRESETS.map(|(name, _)| name)
    );
}

#[test]
fn reference_presets_write_pythons_names_and_reports() {
    let cases = cases("reference.json");
    let presets: Vec<&str> = cases
        .iter()
        .map(|c| str_of(&c["ai"]["preset"], "preset"))
        .collect();
    assert_eq!(presets, ["fast", "standard", "strong"]);
    for case in &cases {
        let preset = str_of(&case["ai"]["preset"], "preset");
        let agent = format!("mcts-{preset}");
        assert_eq!(case["ai"]["dut_agent"], agent.as_str());
        assert_eq!(case["ai"]["bench_agent"], agent.as_str());
        assert_eq!(case["schema"], "urn:gcg-sim:schema:results:1.1.0");
        assert_eq!(case["schema_version"], "1.1.0");
        check_case(case);
    }
}

/// The comparison sees a one-byte change: the normaliser masks only the implementation fields,
/// so a changed seed in a recorded file is a different digest.
#[test]
fn the_digest_comparison_is_sensitive() {
    let a = sha_prefix(b"{\"seed\": 3}\n");
    assert_ne!(a, sha_prefix(b"{\"seed\": 4}\n"));
    assert_ne!(a, sha_prefix(b"{\"seed\": 3}"));
    let masks = [
        ("package", "0.1.0".to_owned(), "0.1.0".to_owned()),
        ("code_digest", "a".repeat(64), "b".repeat(64)),
        ("factory", "rust".to_owned(), "python".to_owned()),
    ];
    let doc = |digest: &str, factory: &str| {
        format!(
            "{{\n \"versions\": {{\n  \"code_digest\": \"{digest}\"\n }},\n \"ai\": {{\n  \
             \"factory\": \"{factory}\"\n }}\n}}\n"
        )
    };
    let text = doc(&"a".repeat(64), "rust");
    let want = doc(&"b".repeat(64), "python");
    assert_eq!(normalized("results.json", &text, &masks), want);
    assert_eq!(
        normalized("summary.md", "x (factory `rust`).\n", &masks),
        "x (factory `python`).\n"
    );
}

/// The replay command's output for `path`, with or without `--log`.
fn replay_text(path: &str, log: bool) -> String {
    let mut text = Vec::new();
    gcg_runner::cli_support::replay_command(
        &gcg_runner::argparse::PyStr::from(path),
        log,
        &mut text,
    )
    .unwrap_or_else(|e| panic!("{path}: {e:?}"));
    String::from_utf8(text).expect("UTF-8")
}

#[test]
fn reference_replays_are_the_files_python_verified() {
    let path = root().join("crates/gcg-cli/tests/vectors/parity/rust-replays.json");
    let text = std::fs::read_to_string(&path).expect("rust-replays.json");
    let v: Value = serde_json::from_str(&text).expect("JSON");
    let runs: Vec<&Value> = v["runs"]
        .as_array()
        .expect("runs")
        .iter()
        .filter(|r| r["argv"][0] == "gcg-eval")
        .collect();
    assert_eq!(runs.len(), 1);
    for run in runs {
        let id = str_of(&run["id"], "id");
        let dir = scratch(id);
        let out = dir.join("out");
        let shown = gcg_data::py_path_str(&out);
        let argv: Vec<String> = run["argv"]
            .as_array()
            .expect("argv")
            .iter()
            .skip(1)
            .map(|a| {
                let a = str_of(a, "argument");
                if a == OUT_TOKEN {
                    shown.clone()
                } else {
                    a.to_owned()
                }
            })
            .collect();
        let done = Command::new(env!("CARGO_BIN_EXE_gcg-eval"))
            .args(&argv)
            .current_dir(root())
            .env_remove("COLUMNS")
            .output()
            .expect("gcg-eval runs");
        assert_eq!(done.status.code(), Some(0), "{id}");
        let recorded = run["replays"].as_object().expect("replays");
        assert_eq!(
            names(&out.join("replays")),
            recorded.keys().cloned().collect::<Vec<_>>(),
            "{id}"
        );
        for (name, want) in recorded {
            let file = format!("{shown}/replays/{name}");
            assert_eq!(
                sha_prefix(&std::fs::read(&file).expect("replay")),
                str_of(&want["sha256"], "sha256")[..16],
                "{id}/{name} is not the file Python verified"
            );
            assert_eq!(want["exit"], 0, "{id}/{name}: Python verified it");
            assert_eq!(
                replay_text(&file, false),
                str_of(&want["stdout"], "stdout"),
                "{id}/{name}: replay"
            );
            let log = replay_text(&file, true);
            assert_eq!(
                Some(log.lines().count()),
                want["log_lines"]
                    .as_u64()
                    .and_then(|n| usize::try_from(n).ok()),
                "{id}/{name}: --log lines"
            );
            assert_eq!(
                sha_prefix(log.as_bytes()),
                str_of(&want["log_sha256"], "log_sha256")[..16],
                "{id}/{name}: replay --log"
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
