//! `main`'s interrupt flow (`cli.py:397-399`; PLAN §7.5) without playing a game: once SIGINT
//! has set the cancel flag, a command reports `interrupted: workers stopped, no reports
//! written` on stderr, prints nothing on stdout and exits 130. Its own test binary, so the
//! SIGINT handler never reaches another test's process.

use std::ffi::OsString;

use gcg_cli::cli;
use gcg_cli::commands::SimCommands;
use gcg_core::pool::Cancel;
use gcg_runner::{EXIT_INTERRUPTED, INTERRUPTED_MESSAGE};

#[test]
fn an_interrupted_command_exits_130_with_the_message() {
    let cancel = Cancel::new();
    gcg_core::sys::sigint::install(&cancel).expect("SIGINT handler installed");
    let pid = std::process::id().to_string();
    let status = std::process::Command::new("kill")
        .args(["-INT", &pid])
        .status()
        .expect("kill runs");
    assert!(status.success());
    // The signal arrives asynchronously; the handler sets the flag.
    while !cancel.is_cancelled() {
        std::hint::spin_loop();
    }
    let deck = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/decks/blue-white-federation.txt"
    );
    for args in [
        vec!["validate", deck],
        vec!["data", "status"],
        vec!["replay", "no-such-replay.json"],
        vec![
            "benchmark",
            deck,
            deck,
            "--matches",
            "1",
            "--out",
            "never-written",
        ],
    ] {
        let args: Vec<OsString> = args.into_iter().map(OsString::from).collect();
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let mut commands = SimCommands::new(cancel.clone(), false);
        let code = cli::run(&args, None, &mut commands, &mut out, &mut err);
        assert_eq!(code, EXIT_INTERRUPTED, "{args:?}");
        assert!(out.is_empty(), "{args:?}");
        assert_eq!(
            String::from_utf8(err).expect("UTF-8"),
            format!("{INTERRUPTED_MESSAGE}\n")
        );
    }
    assert!(!std::path::Path::new("never-written").exists());
    // Usage errors are still argparse's (exit 2), as Python's parse_args runs first.
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let mut commands = SimCommands::new(cancel, false);
    let code = cli::run(&[], None, &mut commands, &mut out, &mut err);
    assert_eq!(code, 2);
}
