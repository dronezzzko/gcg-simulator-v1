//! `gcg-sim`: `main(argv)` of `cli.py:386-399` as a process. Exit codes 0 (success), 1
//! (invalid deck, data or replay, or a failed simulation), 2 (usage) and 130 (SIGINT).
//!
//! SIGINT is installed first (`gcg_core::sys::sigint`, PLAN A3, §7.5), so it sets the run's
//! cancel flag instead of ending the process; the commands then stop, write no report and
//! exit 130 with `interrupted: workers stopped, no reports written`.

use std::ffi::OsString;
use std::io::IsTerminal as _;
use std::process::ExitCode;

use gcg_cli::cli;
use gcg_cli::commands::SimCommands;
use gcg_core::pool::Cancel;

/// The allocator comparison of PLAN §11.4 step 7 (feature `mimalloc`, off by default).
#[cfg(feature = "mimalloc")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() -> ExitCode {
    let cancel = Cancel::new();
    // If the OS refuses the handler, SIGINT keeps its default disposition (the process ends
    // at once); everything else works unchanged.
    let _ = gcg_core::sys::sigint::install(&cancel);
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let columns = std::env::var_os("COLUMNS");
    let stderr = std::io::stderr();
    let mut commands = SimCommands::new(cancel, stderr.is_terminal());
    let mut out = std::io::stdout().lock();
    let mut err = stderr.lock();
    ExitCode::from(cli::run(
        &args,
        columns.as_deref(),
        &mut commands,
        &mut out,
        &mut err,
    ))
}
