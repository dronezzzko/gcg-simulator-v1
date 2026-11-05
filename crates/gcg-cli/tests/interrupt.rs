//! Ctrl-C (T-075; PLAN §7.5, D13): `tests/cli/test_console_script.py:90-112` with every
//! behavioural assertion kept (exit 130, the message, empty stdout, no `results.json`) and both
//! timing assertions dropped (the crates hold no timing code). The wait for the process is a plain
//! blocking wait, with no timeout.
//!
//! It plays real games, so it needs the engine's game layer (`needs_engine_`).

use std::io::{BufRead as _, BufReader, Read as _};
use std::path::Path;
use std::process::{Command, Stdio};

use gcg_runner::{EXIT_INTERRUPTED, INTERRUPTED_MESSAGE};

#[test]
fn needs_engine_ctrl_c_stops_workers_and_writes_nothing() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out =
        Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("interrupted-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out);
    let mut child = Command::new(env!("CARGO_BIN_EXE_gcg-sim"))
        .args([
            "benchmark",
            "examples/decks/purple-white-tekkadan.txt",
            "examples/decks/blue-white-federation.txt",
            "--matches",
            "100000",
            "--workers",
            "2",
            "--ai-preset",
            "fast",
            "--out",
        ])
        .arg(&out)
        .current_dir(&root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("gcg-sim starts");
    let mut stderr = BufReader::new(child.stderr.take().expect("stderr pipe"));
    let mut first = String::new();
    stderr.read_line(&mut first).expect("a progress line");
    assert!(first.starts_with("matches 1/100000"), "{first}");
    let status = Command::new("kill")
        .args(["-INT", &child.id().to_string()])
        .status()
        .expect("kill runs");
    assert!(status.success());
    let code = child.wait().expect("gcg-sim exits");
    let mut rest = String::new();
    stderr.read_to_string(&mut rest).expect("stderr");
    let mut stdout = String::new();
    child
        .stdout
        .take()
        .expect("stdout pipe")
        .read_to_string(&mut stdout)
        .expect("stdout");
    assert_eq!(code.code(), Some(i32::from(EXIT_INTERRUPTED)));
    assert!(rest.contains(INTERRUPTED_MESSAGE), "{rest}");
    assert_eq!(stdout, "");
    assert!(!out.join("results.json").exists());
    let _ = std::fs::remove_dir_all(&out);
}
