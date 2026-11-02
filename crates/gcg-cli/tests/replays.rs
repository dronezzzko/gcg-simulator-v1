//! Replay verification in both directions (T-004; PLAN §8.8 item 6), against the transcripts of
//! Python's `gcg-sim replay` and `gcg-sim replay --log` (`port/oracle/export_parity.py`):
//!
//! - Python to Rust: every committed Python replay
//!   (`crates/gcg-runner/tests/vectors/records/*/replays/*.json`) verifies under the Rust
//!   `gcg-sim replay`, with Python's exit status, stdout and stderr, and `--log` prints
//!   Python's log byte for byte (SHA-256 and line count);
//! - Rust to Python: the replays that `gcg-sim benchmark` writes with the shipped presets are
//!   byte-identical to the ones the exporter had Python verify, so Python's verdict holds for
//!   them, and the Rust `replay` transcripts of those files are Python's.
//!   (`crates/gcg-eval/tests/parity.rs` checks the `gcg-eval reference-benchmark` run of the
//!   same vector file.)

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;
use sha2::{Digest, Sha256};

const OUT_TOKEN: &str = "{OUT}";
const VERIFIED: &str = "replay matches the recorded outcome and final state";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn vectors(name: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/vectors/parity")
        .join(name);
    let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn str_of<'a>(v: &'a Value, what: &str) -> &'a str {
    v.as_str().unwrap_or_else(|| panic!("{what} is a string"))
}

struct Transcript {
    exit: Option<i32>,
    stdout: Vec<u8>,
    stderr: String,
}

fn gcg_sim<S: AsRef<std::ffi::OsStr>>(args: &[S]) -> Transcript {
    let done = Command::new(env!("CARGO_BIN_EXE_gcg-sim"))
        .args(args)
        .current_dir(root())
        .env_remove("COLUMNS")
        .output()
        .expect("gcg-sim runs");
    Transcript {
        exit: done.status.code(),
        stdout: done.stdout,
        stderr: String::from_utf8(done.stderr).expect("UTF-8 stderr"),
    }
}

fn exit_of(v: &Value) -> Option<i32> {
    v.as_i64().and_then(|c| i32::try_from(c).ok())
}

/// `gcg-sim replay FILE` and `gcg-sim replay --log FILE` against Python's transcripts.
fn check_transcripts(file: &Path, want: &Value, what: &str) {
    let plain = gcg_sim(&[Path::new("replay"), file]);
    assert_eq!(plain.exit, exit_of(&want["exit"]), "{what}: exit");
    assert_eq!(
        String::from_utf8(plain.stdout).expect("UTF-8"),
        str_of(&want["stdout"], "stdout"),
        "{what}: stdout"
    );
    assert_eq!(plain.stderr, str_of(&want["stderr"], "stderr"), "{what}");
    let logged = gcg_sim(&[Path::new("replay"), Path::new("--log"), file]);
    assert_eq!(
        logged.exit,
        exit_of(&want["log_exit"]),
        "{what}: --log exit"
    );
    assert_eq!(
        logged.stderr,
        str_of(&want["log_stderr"], "log_stderr"),
        "{what}"
    );
    let log = String::from_utf8(logged.stdout).expect("UTF-8");
    assert_eq!(
        Some(log.lines().count()),
        want["log_lines"]
            .as_u64()
            .and_then(|n| usize::try_from(n).ok()),
        "{what}: --log lines"
    );
    assert_eq!(
        sha256(log.as_bytes()),
        str_of(&want["log_sha256"], "log_sha256"),
        "{what}: --log output"
    );
    assert_eq!(log.lines().last(), Some(VERIFIED), "{what}");
}

#[test]
fn needs_engine_every_python_replay_verifies_with_pythons_transcripts() {
    let v = vectors("python-replays.json");
    let cases = v["cases"].as_array().expect("cases");
    let mut on_disk: Vec<String> = Vec::new();
    let records = root().join("crates/gcg-runner/tests/vectors/records");
    for run in fs::read_dir(&records).expect("records") {
        let replays = run.expect("entry").path().join("replays");
        if replays.is_dir() {
            for f in fs::read_dir(&replays).expect("replays") {
                let path = f.expect("entry").path();
                let rel = path.strip_prefix(root()).expect("inside the repository");
                on_disk.push(rel.to_str().expect("UTF-8").to_owned());
            }
        }
    }
    on_disk.sort();
    let listed: Vec<String> = cases
        .iter()
        .map(|c| str_of(&c["file"], "file").to_owned())
        .collect();
    assert_eq!(
        listed, on_disk,
        "every committed Python replay has transcripts"
    );
    assert!(cases.len() >= 20, "{} Python replays", cases.len());
    for case in cases {
        let file = str_of(&case["file"], "file");
        let bytes = fs::read(root().join(file)).expect("a replay file");
        assert_eq!(sha256(&bytes), str_of(&case["sha256"], "sha256"), "{file}");
        check_transcripts(Path::new(file), case, file);
    }
}

#[test]
fn needs_engine_rust_replays_are_the_files_python_verified() {
    let v = vectors("rust-replays.json");
    let runs: Vec<&Value> = v["runs"]
        .as_array()
        .expect("runs")
        .iter()
        .filter(|r| r["argv"][0] == "gcg-sim")
        .collect();
    assert!(runs.len() >= 3, "{} gcg-sim runs", runs.len());
    let mut presets = Vec::new();
    for run in runs {
        let id = str_of(&run["id"], "id");
        let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("rust-replays-{id}"));
        let _ = fs::remove_dir_all(&out);
        let shown = gcg_data::py_path_str(&out);
        let args: Vec<String> = run["argv"]
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
        let done = gcg_sim(&args);
        assert_eq!(done.exit, Some(0), "{id}: {}", done.stderr);
        let results: Value = serde_json::from_str(
            &fs::read_to_string(out.join("results.json")).expect("results.json"),
        )
        .expect("JSON");
        presets.push(str_of(&results["ai"]["preset"], "preset").to_owned());
        let mut names: Vec<String> = fs::read_dir(out.join("replays"))
            .expect("replays")
            .map(|e| e.expect("entry").file_name().into_string().expect("UTF-8"))
            .collect();
        names.sort();
        let recorded = run["replays"].as_object().expect("replays");
        assert_eq!(
            names,
            recorded.keys().cloned().collect::<Vec<_>>(),
            "{id}: replay files"
        );
        for (name, want) in recorded {
            let path = out.join("replays").join(name);
            let bytes = fs::read(&path).expect("replay");
            assert_eq!(
                sha256(&bytes),
                str_of(&want["sha256"], "sha256"),
                "{id}/{name} is not the file Python verified; rerun \
                 port/oracle/export_parity.py --section rust-replays"
            );
            assert_eq!(want["exit"], 0, "{id}/{name}: Python verified it");
            check_transcripts(&path, want, &format!("{id}/{name}"));
        }
        let _ = fs::remove_dir_all(&out);
    }
    presets.sort();
    presets.dedup();
    assert_eq!(presets, ["fast", "standard"], "both shipped presets");
}
