//! The `gcg-sim` command line: `validate`, `benchmark`, `data status` and `replay` with
//! Python's flags, messages and exit codes 0/1/2/130, and the two-value `--ai-preset`
//! (docs/port/PLAN.md §2.1, §2.3, §6.5).
//!
//! Workstream E: the `gcg-sim` parser, its typed commands and `main`'s flow ([`cli`]), built
//! on the argparse-compatible parser that `gcg-sim` and `gcg-eval` share
//! ([`gcg_runner::argparse`]), and the build-time code digest. The command bodies (deck
//! loading, the benchmark run and reports, `data status`, replay checking) and the `gcg-sim`
//! binary land at integration, behind [`cli::Commands`].

pub mod cli;

/// The build-time `code_digest` (PLAN §2.4, X19, W-103): SHA-256 over the files of
/// `build-support/digest-manifest.txt`, computed by `build.rs`.
pub const CODE_DIGEST: &str = env!("GCG_CODE_DIGEST");

/// `versions.package` (`reports/versions.py:25-29`): the crate version from the build, so
/// it can never be "unknown" (W-103).
pub const PACKAGE_VERSION: &str = env!("CARGO_PKG_VERSION");
