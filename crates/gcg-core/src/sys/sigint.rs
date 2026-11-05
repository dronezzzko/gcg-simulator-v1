//! SIGINT handling (docs/port/PLAN.md A3, §7.5): `install(&Cancel)` registers the cancel flag
//! with `signal_hook::flag::register(SIGINT, ..)`, which stores `true` on SIGINT. The module is
//! safe code; the coordinator then stops dispatch, joins the workers, writes no report, prints
//! `interrupted: workers stopped, no reports written` and exits 130 (`cli.py:397-399`).
//!
//! Python ignores SIGINT in its worker processes and turns it into `KeyboardInterrupt` in the
//! parent (`runner/run.py:41-42`, `:102-104`); with threads, the flag is the whole mechanism:
//! the default disposition would end the process with status -2, not 130
//! (`tests/cli/test_console_script.py:109`).

use std::io;

use crate::pool::Cancel;

/// Registers `cancel` so that SIGINT sets it (`signal_hook::flag::register(SIGINT, flag)`).
/// Once installed, SIGINT no longer ends the process; the run checks the flag at match start,
/// between decisions and between search iterations (PLAN §7.5).
///
/// # Errors
/// The `io::Error` of `signal_hook::flag::register` (the OS refused the handler).
pub fn install(cancel: &Cancel) -> io::Result<()> {
    signal_hook::flag::register(
        signal_hook::consts::SIGINT,
        std::sync::Arc::clone(cancel.flag()),
    )
    .map(|_| ())
}
