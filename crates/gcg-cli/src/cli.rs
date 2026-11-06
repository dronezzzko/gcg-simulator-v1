//! The `gcg-sim` command line (`cli.py:68-95`, `:376-399`): the parser, the typed commands it
//! yields, and `main`'s flow from arguments to an exit status.
//!
//! The parser is Python's exactly, except two deliberate differences
//! (PLAN A5, A30, §6.5): `--ai-preset` accepts exactly `standard` (the
//! default) and `fast`, with the preset descriptions in its help, and `--workers` also accepts
//! `auto`. `oracle/export_cli.py` builds the same parser from Python's with these two
//! changes, and the transcripts of both are compared in `tests/transcripts.rs` (T-072,
//! T-152).

use std::ffi::{OsStr, OsString};
use std::io::Write;

use gcg_runner::argparse::{
    Action, Converter, Exit, Namespace, Parser, PyInt, PyStr, SubCommand, Value, pyint,
};
use gcg_runner::{
    DEFAULT_OUT_DIR, DEFAULT_REPLAYS, EXIT_INTERRUPTED, EXIT_INVALID, EXIT_OK, Format,
    INTERRUPTED_MESSAGE, Workers,
};
use gcg_search::Preset;

/// `prog` of the top-level parser (`cli.py:69-71`).
pub const PROG: &str = "gcg-sim";

/// `--workers` help: Python's "worker processes" (PLAN A5) and the additive `auto` (§7.4).
pub const WORKERS_HELP: &str =
    "worker processes; auto uses one per performance-core CPU (at most one per match)";

/// `--ai-preset` help: the description of each value (PLAN
/// A11) and the mapping to the Python presets the budgets start from (PLAN D12).
#[must_use]
pub fn preset_help() -> String {
    format!(
        "AI strength. standard (default): {}. fast: {}. Both start from the budgets of the \
         Python presets strong (standard) and standard (fast); docs/AI.md has the table",
        Preset::Standard.help(),
        Preset::Fast.help()
    )
}

/// `build_parser()` (`cli.py:68-95`) with the Rust `--ai-preset` and `--workers`.
#[must_use]
pub fn build_parser() -> Parser {
    let help = |s: &str| Some(s.to_owned());
    let mut bench = Parser::new(&format!("{PROG} benchmark"), None);
    bench.push(Action::positional(
        "deck_under_test",
        Converter::Path,
        help("deck file of the deck under test"),
    ));
    bench.push(Action::positional(
        "benchmark_deck",
        Converter::Path,
        help("deck file of the benchmark deck"),
    ));
    let mut matches = Action::option(
        "--matches",
        "matches",
        Converter::PositiveInt,
        None,
        Value::None,
        "number of matches".to_owned(),
    );
    matches.required = true;
    bench.push(matches);
    bench.push(Action::option(
        "--seed",
        "seed",
        Converter::Seed,
        None,
        Value::Int(PyInt::from_u64(0)),
        "master seed (default 0)".to_owned(),
    ));
    bench.push(Action::option(
        "--workers",
        "workers",
        Converter::Workers,
        None,
        Value::Int(PyInt::from_u64(1)),
        WORKERS_HELP.to_owned(),
    ));
    bench.push(Action::option(
        "--format",
        "format",
        Converter::Str,
        Some(Format::ALL.iter().map(|f| f.as_str()).collect()),
        Value::Str(PyStr::from(Format::Bo3.as_str())),
        "match format".to_owned(),
    ));
    bench.push(Action::option(
        "--ai-preset",
        "ai_preset",
        Converter::Str,
        Some(Preset::ALL.iter().map(|p| p.as_str()).collect()),
        Value::Str(PyStr::from(Preset::DEFAULT.as_str())),
        preset_help(),
    ));
    bench.push(Action::option(
        "--out",
        "out",
        Converter::Path,
        None,
        Value::Path(PyStr::from(DEFAULT_OUT_DIR)),
        "report directory".to_owned(),
    ));
    bench.push(Action::flag(
        "--decision-log",
        "decision_log",
        "record AI decision logs".to_owned(),
    ));
    bench.push(Action::option(
        "--replays",
        "replays",
        Converter::NonNegativeInt,
        None,
        Value::Int(PyInt::from_u64(u64::from(DEFAULT_REPLAYS))),
        "replays saved per result".to_owned(),
    ));

    let mut validate = Parser::new(&format!("{PROG} validate"), None);
    validate.push(Action::positional("deck", Converter::Path, None));

    let mut status = Parser::new(&format!("{PROG} data status"), None);
    status.push(Action::flag(
        "--json",
        "json",
        "print JSON instead of text".to_owned(),
    ));
    let mut data = Parser::new(&format!("{PROG} data"), None);
    data.push(Action::subcommands(
        "data_command",
        vec![SubCommand {
            name: "status",
            help: Some("data snapshot, rules, B&R, coverage, conflicts"),
            parser: status,
        }],
    ));

    let mut replay = Parser::new(&format!("{PROG} replay"), None);
    replay.push(Action::positional("replay_json", Converter::Path, None));
    replay.push(Action::flag(
        "--log",
        "log",
        "print the game log".to_owned(),
    ));

    let mut top = Parser::new(PROG, Some("Gundam Card Game deck benchmarking simulator."));
    top.push(Action::subcommands(
        "command",
        vec![
            SubCommand {
                name: "benchmark",
                help: Some("play a deck under test against a benchmark deck"),
                parser: bench,
            },
            SubCommand {
                name: "validate",
                help: Some("check a deck file against the construction rules"),
                parser: validate,
            },
            SubCommand {
                name: "data",
                help: Some("packaged data"),
                parser: data,
            },
            SubCommand {
                name: "replay",
                help: Some("re-simulate a saved replay and verify its outcome"),
                parser: replay,
            },
        ],
    ));
    top
}

/// The terminal width argparse formats for: `shutil.get_terminal_size().columns`
/// (`argparse.py:186-191`) is `int(COLUMNS)` when that parses as a positive base-10 integer,
/// else the size of the terminal on stdout, else 80. The Rust implementation has no safe way to ask the
/// terminal for its size, so it uses 80 there too: the only difference from Python, and only
/// on a terminal without `COLUMNS` (transcripts are taken with a pipe).
#[must_use]
pub fn terminal_columns(columns: Option<&OsStr>) -> i64 {
    let parsed = columns.and_then(|text| pyint::parse_int(&PyStr::from_os(text), 10));
    match parsed {
        Some(value) if value.cmp_i64(0).is_gt() => value
            .to_u64()
            .map_or(i64::MAX, |v| i64::try_from(v).unwrap_or(i64::MAX)),
        _ => 80,
    }
}

/// `parse_args(argv)` of the `gcg-sim` parser: the namespace, or the help/usage exit.
///
/// # Errors
/// The [`Exit`] argparse's `SystemExit` stands for.
pub fn parse_namespace(args: &[OsString], columns: i64) -> Result<Namespace, Exit> {
    let args: Vec<PyStr> = args.iter().map(|a| PyStr::from_os(a)).collect();
    build_parser().parse_args(args, columns)
}

/// A `type=Path` argument: Python's `str(Path(text))`, which is both what
/// Python prints and what it opens.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct PathArg(pub PyStr);

impl PathArg {
    /// The path to open (`os.fspath`, `surrogateescape`).
    #[must_use]
    pub fn to_os_string(&self) -> OsString {
        self.0.to_os_string()
    }

    /// The path as `print(path)` writes it to stdout.
    #[must_use]
    pub fn stdout_bytes(&self) -> Vec<u8> {
        self.0.to_surrogateescape_bytes()
    }

    /// The text, for messages on stderr.
    #[must_use]
    pub const fn text(&self) -> &PyStr {
        &self.0
    }
}

/// `benchmark` (`cli.py:73-85`, `:188-229`).
#[derive(Clone, PartialEq, Debug)]
pub struct BenchmarkArgs {
    pub deck_under_test: PathArg,
    pub benchmark_deck: PathArg,
    pub matches: u32,
    pub seed: u64,
    pub workers: Workers,
    pub format: Format,
    pub ai_preset: Preset,
    pub out: PathArg,
    pub decision_log: bool,
    pub replays: u32,
}

/// `validate` (`cli.py:86-87`, `:129-136`).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ValidateArgs {
    pub deck: PathArg,
}

/// `data status` (`cli.py:88-91`, `:325-331`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct DataStatusArgs {
    pub json: bool,
}

/// `replay` (`cli.py:92-94`, `:338-370`).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ReplayArgs {
    pub replay_json: PathArg,
    pub log: bool,
}

/// What `_dispatch` runs (`cli.py:376-383`).
#[derive(Clone, PartialEq, Debug)]
pub enum Command {
    Benchmark(BenchmarkArgs),
    Validate(ValidateArgs),
    DataStatus(DataStatusArgs),
    Replay(ReplayArgs),
}

fn path(ns: &Namespace, dest: &str) -> PathArg {
    match ns.get(dest) {
        Some(Value::Path(text)) => PathArg(text.clone()),
        _ => unreachable!("{dest} is a required Path argument"),
    }
}

fn flag(ns: &Namespace, dest: &str) -> bool {
    matches!(ns.get(dest), Some(Value::Bool(true)))
}

fn int(ns: &Namespace, dest: &str) -> PyInt {
    match ns.get(dest) {
        Some(Value::Int(value)) => value.clone(),
        _ => unreachable!("{dest} is an integer argument"),
    }
}

fn text(ns: &Namespace, dest: &str) -> PyStr {
    match ns.get(dest) {
        Some(Value::Str(value)) => value.clone(),
        _ => unreachable!("{dest} is a str argument"),
    }
}

/// `BenchmarkConfig.matches` and `.replays` are `u32` (docs/dev/contracts.md, gcg-runner).
/// Python accepts any size; above `u32::MAX` the Rust implementation reports a usage error in Python's
/// style instead (a documented difference: Python would start billions of matches).
fn to_u32(
    ns: &Namespace,
    dest: &str,
    option: &str,
    bench: &Parser,
    columns: i64,
) -> Result<u32, Exit> {
    let value = int(ns, dest);
    value
        .to_u64()
        .and_then(|v| u32::try_from(v).ok())
        .ok_or_else(|| {
            let shown = value
                .py_str()
                .map_or_else(String::new, |v| format!(", got {v}"));
            let msg = format!("argument {option}: must be at most {}{shown}", u32::MAX);
            bench.error(&PyStr::from(msg.as_str()), columns)
        })
}

/// The typed command of a parsed namespace (`_dispatch`, `cli.py:376-383`).
///
/// # Errors
/// The usage error of a `--matches` or `--replays` value above `u32::MAX`.
pub fn command(ns: &Namespace, columns: i64) -> Result<Command, Exit> {
    let name = text(ns, "command");
    if name.eq_str("benchmark") {
        let parser = build_parser();
        let bench = parser
            .subcommand("benchmark")
            .unwrap_or_else(|| unreachable!("gcg-sim has a benchmark subcommand"));
        let workers = match ns.get("workers") {
            Some(Value::Auto) => Workers::Auto,
            // Python caps workers at the match count (runner/run.py:83-85), so a larger
            // count behaves as u32::MAX.
            _ => Workers::Count(
                int(ns, "workers")
                    .to_u64()
                    .map_or(u32::MAX, |v| u32::try_from(v).unwrap_or(u32::MAX)),
            ),
        };
        let format_text = text(ns, "format");
        let preset_text = text(ns, "ai_preset");
        let format = Format::ALL
            .into_iter()
            .find(|f| format_text.eq_str(f.as_str()))
            .unwrap_or_else(|| unreachable!("--format is one of its choices"));
        let ai_preset = Preset::ALL
            .into_iter()
            .find(|p| preset_text.eq_str(p.as_str()))
            .unwrap_or_else(|| unreachable!("--ai-preset is one of its choices"));
        return Ok(Command::Benchmark(BenchmarkArgs {
            deck_under_test: path(ns, "deck_under_test"),
            benchmark_deck: path(ns, "benchmark_deck"),
            matches: to_u32(ns, "matches", "--matches", bench, columns)?,
            seed: int(ns, "seed")
                .to_u64()
                .unwrap_or_else(|| unreachable!("_seed keeps the seed in [0, 2**64)")),
            workers,
            format,
            ai_preset,
            out: path(ns, "out"),
            decision_log: flag(ns, "decision_log"),
            replays: to_u32(ns, "replays", "--replays", bench, columns)?,
        }));
    }
    if name.eq_str("validate") {
        return Ok(Command::Validate(ValidateArgs {
            deck: path(ns, "deck"),
        }));
    }
    if name.eq_str("replay") {
        return Ok(Command::Replay(ReplayArgs {
            replay_json: path(ns, "replay_json"),
            log: flag(ns, "log"),
        }));
    }
    Ok(Command::DataStatus(DataStatusArgs {
        json: flag(ns, "json"),
    }))
}

/// `build_parser().parse_args(argv)` and the typed command.
///
/// # Errors
/// The help or usage [`Exit`].
pub fn parse(args: &[OsString], columns: i64) -> Result<Command, Exit> {
    let ns = parse_namespace(args, columns)?;
    command(&ns, columns)
}

/// Why a command did not succeed: a `CliError` (printed as `error: {message}`, exit 1) or
/// `KeyboardInterrupt` (exit 130), as `main` handles them (`cli.py:394-399`).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum CommandError {
    Invalid(PyStr),
    Interrupted,
}

/// The four command bodies (`cli_support` and the runner, landed at integration). Each
/// writes its stdout through `out` (bytes, so paths keep their undecodable bytes) and, for
/// `benchmark`, its progress through `err`.
pub trait Commands {
    /// `cmd_benchmark` (`cli.py:188-229`).
    ///
    /// # Errors
    /// As `main` reports them.
    fn benchmark(
        &mut self,
        args: &BenchmarkArgs,
        out: &mut dyn Write,
        err: &mut dyn Write,
    ) -> Result<(), CommandError>;

    /// `cmd_validate` (`cli.py:129-136`).
    ///
    /// # Errors
    /// As `main` reports them.
    fn validate(&mut self, args: &ValidateArgs, out: &mut dyn Write) -> Result<(), CommandError>;

    /// `cmd_data_status` (`cli.py:325-331`).
    ///
    /// # Errors
    /// As `main` reports them.
    fn data_status(
        &mut self,
        args: &DataStatusArgs,
        out: &mut dyn Write,
    ) -> Result<(), CommandError>;

    /// `cmd_replay` (`cli.py:338-370`).
    ///
    /// # Errors
    /// As `main` reports them.
    fn replay(&mut self, args: &ReplayArgs, out: &mut dyn Write) -> Result<(), CommandError>;
}

/// `main(argv)` (`cli.py:386-399`): parse (help and usage errors exit as argparse does),
/// dispatch (`_dispatch`, `:376-383`), then `error: {message}` and exit 1 for a `CliError`,
/// or the interrupt message and exit 130. Write failures on stdout and stderr are ignored,
/// as argparse's `_print_message` ignores them (`argparse.py:2918-2924`).
pub fn run(
    args: &[OsString],
    columns: Option<&OsStr>,
    commands: &mut dyn Commands,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> u8 {
    let columns = terminal_columns(columns);
    let command = match parse(args, columns) {
        Ok(command) => command,
        Err(exit) => {
            let _ = out.write_all(&exit.stdout);
            let _ = err.write_all(&exit.stderr);
            let _ = out.flush();
            let _ = err.flush();
            return exit.status;
        }
    };
    let result = match &command {
        Command::Benchmark(a) => commands.benchmark(a, out, err),
        Command::Validate(a) => commands.validate(a, out),
        Command::Replay(a) => commands.replay(a, out),
        Command::DataStatus(a) => commands.data_status(a, out),
    };
    let _ = out.flush();
    match result {
        Ok(()) => EXIT_OK,
        Err(CommandError::Invalid(message)) => {
            let mut line = b"error: ".to_vec();
            line.extend_from_slice(&message.to_stderr_bytes());
            line.push(b'\n');
            let _ = err.write_all(&line);
            let _ = err.flush();
            EXIT_INVALID
        }
        Err(CommandError::Interrupted) => {
            let _ = writeln!(err, "{INTERRUPTED_MESSAGE}");
            let _ = err.flush();
            EXIT_INTERRUPTED
        }
    }
}
