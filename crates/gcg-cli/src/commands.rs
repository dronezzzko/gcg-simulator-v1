//! The four `gcg-sim` command bodies behind [`crate::cli::Commands`] (`cli.py:129-370`), over
//! the shared `gcg_runner::cli_support` and the [`SearchFactory`].
//!
//! SIGINT sets the run's [`Cancel`] (`gcg_core::sys::sigint::install`, done by `main`). The
//! benchmark checks it at match start, between decisions and around every report file (PLAN
//! §7.5); a command does not start once it is set, and any command that ends while it is set
//! reports the interrupt, as Python's `KeyboardInterrupt` would have stopped it
//! (`cli.py:397-399`).

use std::io::Write;

use gcg_core::pool::Cancel;
use gcg_runner::SchemaVersion;
use gcg_runner::cli_support::{
    BenchmarkRequest, CliError, benchmark_command, data_status_command, replay_command,
    validate_command,
};

use crate::cli::{BenchmarkArgs, CommandError, Commands, DataStatusArgs, ReplayArgs, ValidateArgs};
use crate::factory::SearchFactory;
use crate::{CODE_DIGEST, PACKAGE_VERSION};

/// The results schema `gcg-sim` writes: 1.1.0 until the Phase 6 schema 1.2.0 lands
/// (docs/port/contracts.md open item O9); its `ai.preset` values are in the 1.1.0 enum.
pub const SCHEMA: SchemaVersion = SchemaVersion::V1_1;

/// The real command bodies of `gcg-sim`.
#[derive(Debug)]
pub struct SimCommands {
    cancel: Cancel,
    /// `sys.stderr.isatty()` for the progress printer (`cli.py:147`).
    stderr_tty: bool,
}

impl SimCommands {
    /// `cancel` is the flag SIGINT sets.
    #[must_use]
    pub const fn new(cancel: Cancel, stderr_tty: bool) -> Self {
        Self { cancel, stderr_tty }
    }

    fn start(&self) -> Result<(), CommandError> {
        if self.cancel.is_cancelled() {
            Err(CommandError::Interrupted)
        } else {
            Ok(())
        }
    }

    fn finish(&self, result: Result<(), CliError>) -> Result<(), CommandError> {
        if self.cancel.is_cancelled() {
            return Err(CommandError::Interrupted);
        }
        result.map_err(|e| match e {
            CliError::Invalid(message) => CommandError::Invalid(message),
            CliError::Interrupted => CommandError::Interrupted,
        })
    }
}

impl Commands for SimCommands {
    fn benchmark(
        &mut self,
        args: &BenchmarkArgs,
        out: &mut dyn Write,
        err: &mut dyn Write,
    ) -> Result<(), CommandError> {
        self.start()?;
        let request = BenchmarkRequest {
            deck_under_test: args.deck_under_test.text(),
            benchmark_deck: args.benchmark_deck.text(),
            matches: args.matches,
            seed: args.seed,
            workers: args.workers,
            format: args.format,
            ai_preset: args.ai_preset.as_str(),
            out: args.out.text(),
            decision_log: args.decision_log,
            replays: args.replays,
            schema: SCHEMA,
            package: PACKAGE_VERSION,
            code_digest: CODE_DIGEST,
        };
        let factory = SearchFactory::new(args.ai_preset);
        let result = benchmark_command(&request, &factory, self.stderr_tty, &self.cancel, out, err);
        self.finish(result)
    }

    fn validate(&mut self, args: &ValidateArgs, out: &mut dyn Write) -> Result<(), CommandError> {
        self.start()?;
        let result = validate_command(args.deck.text(), out);
        self.finish(result)
    }

    fn data_status(
        &mut self,
        args: &DataStatusArgs,
        out: &mut dyn Write,
    ) -> Result<(), CommandError> {
        self.start()?;
        data_status_command(args.json, PACKAGE_VERSION, CODE_DIGEST, out);
        self.finish(Ok(()))
    }

    fn replay(&mut self, args: &ReplayArgs, out: &mut dyn Write) -> Result<(), CommandError> {
        self.start()?;
        let result = replay_command(args.replay_json.text(), args.log, out);
        self.finish(result)
    }
}
