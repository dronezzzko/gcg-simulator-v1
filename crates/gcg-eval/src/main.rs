//! The `gcg-eval` binary (docs/dev/PLAN.md §2.3): the command line of [`gcg_eval::cli`], with
//! SIGINT turned into the cancel flag (exit 130, PLAN §7.5).

use std::ffi::OsString;
use std::io::IsTerminal;
use std::process::ExitCode;

use gcg_core::pool::Cancel;

/// The allocator of PLAN §11.4 step 7 (feature `mimalloc`, on by default; without it the
/// system allocator).
#[cfg(feature = "mimalloc")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() -> ExitCode {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let cancel = Cancel::new();
    if let Err(e) = gcg_core::sys::sigint::install(&cancel) {
        eprintln!("error: cannot install the SIGINT handler: {e}");
        return ExitCode::from(gcg_runner::EXIT_INVALID);
    }
    let columns = std::env::var_os("COLUMNS");
    let tty = std::io::stderr().is_terminal();
    let stdout = std::io::stdout();
    let stderr = std::io::stderr();
    let code = gcg_eval::cli::run(
        &args,
        columns.as_deref(),
        &cancel,
        tty,
        &mut stdout.lock(),
        &mut stderr.lock(),
    );
    ExitCode::from(code)
}
