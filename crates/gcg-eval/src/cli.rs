//! The `gcg-eval` command line: `reference-benchmark`, `preset-benchmark`, `head-to-head`,
//! `compare` and `env` (docs/port/PLAN.md §2.1, §10.4 F, §12.8), on the argparse-compatible
//! parser that `gcg-sim` shares (`gcg_runner::argparse`), with `gcg-sim`'s exit codes
//! (0, 1 with `error: {message}`, 2 for usage errors, 130 on SIGINT).
//!
//! - `reference-benchmark DUT BENCH` takes every `gcg-sim benchmark` option, with
//!   `--reference-preset fast|standard|strong` (default `standard`) in place of `--ai-preset`,
//!   and `--agent test-random|test-illegal` for the runner's test agents (the
//!   `GCG_SIM_AGENT_FACTORY` hook of `cli.py:162-164`). It writes Python's preset and agent
//!   names and results schema 1.1.0 (PLAN A6, A9, A10).
//! - `preset-benchmark DUT BENCH --search-config FILE` is the `gcg-sim benchmark` pipeline with
//!   the strengthened search configured by FILE ([`crate::search_config`]); `--ai-preset`
//!   (`standard`, `fast`) only labels the run (PLAN §12.8).
//! - `head-to-head CHALLENGER OPPONENT` ([`crate::head_to_head`]).
//! - `compare CHALLENGER OPPONENT` is `python -m gcg_sim.ai.compare` ([`crate::compare`]): its
//!   `type=int` options convert after parsing, so with several errors the first reported can
//!   differ from Python's, and `prog` is `gcg-eval compare`.
//! - `env` prints [`crate::env`]'s document.

use std::ffi::{OsStr, OsString};
use std::io::Write;
use std::path::PathBuf;

use gcg_core::pool::Cancel;
use gcg_core::pyjson::{Json, dumps};
use gcg_runner::argparse::{
    Action, Converter, Exit, Namespace, Parser, PyInt, PyStr, SubCommand, Value, pyint,
};
use gcg_runner::{
    DEFAULT_OUT_DIR, DEFAULT_REPLAYS, EXIT_INTERRUPTED, EXIT_INVALID, EXIT_OK, Format,
    INTERRUPTED_MESSAGE, SchemaVersion, TestAgentKind, Workers,
};
use gcg_search::Preset;

use crate::bench_cmd::{BenchArgs, CommandError, ProgressPrinter, benchmark};
use crate::compare::{CompareArgs, indented, int_arg, outputs};
use crate::factories::{ReferenceFactory, SearchConfigFactory};
use crate::head_to_head::{self, HeadToHeadArgs, PairSource};
use crate::ledger::{self, DEFAULT_LEDGER};
use crate::selfplay::play_all;
use crate::{ChooseFirst, DeckSet, PairSet, SeedRole};

/// `prog` of the top-level parser.
pub const PROG: &str = "gcg-eval";
/// The top-level description.
pub const DESCRIPTION: &str = "Evaluation harness of the gcg-sim port: the reference search, \
     head-to-head comparisons of agents and the tuning tools.";
/// `--workers` help, as `gcg-sim`'s (PLAN A5, §7.4).
pub const WORKERS_HELP: &str =
    "worker processes; auto uses one per logical CPU (at most one per match)";
/// The reference presets, in `PRESETS` order (`ai/config.py:80-111`).
pub const REFERENCE_PRESETS: [&str; 3] = ["fast", "standard", "strong"];
/// `compare`'s description: the first line of `ai/compare.py`'s docstring.
pub const COMPARE_DESCRIPTION: &str =
    "Head-to-head comparison of two AI versions in mirrored blocks of four games.";

fn help(s: &str) -> Option<String> {
    Some(s.to_owned())
}

/// `gcg-sim benchmark`'s arguments (`cli.py:73-85`) with `preset` in `--ai-preset`'s place.
fn benchmark_parser(name: &str, preset: Action, extra: Vec<Action>) -> Parser {
    let mut p = Parser::new(&format!("{PROG} {name}"), None);
    p.push(Action::positional(
        "deck_under_test",
        Converter::Path,
        help("deck file of the deck under test"),
    ));
    p.push(Action::positional(
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
    p.push(matches);
    p.push(Action::option(
        "--seed",
        "seed",
        Converter::Seed,
        None,
        Value::Int(PyInt::from_u64(0)),
        "master seed (default 0)".to_owned(),
    ));
    p.push(Action::option(
        "--workers",
        "workers",
        Converter::Workers,
        None,
        Value::Int(PyInt::from_u64(1)),
        WORKERS_HELP.to_owned(),
    ));
    p.push(Action::option(
        "--format",
        "format",
        Converter::Str,
        Some(Format::ALL.iter().map(|f| f.as_str()).collect()),
        Value::Str(PyStr::from(Format::Bo3.as_str())),
        "match format".to_owned(),
    ));
    p.push(preset);
    p.push(Action::option(
        "--out",
        "out",
        Converter::Path,
        None,
        Value::Path(PyStr::from(DEFAULT_OUT_DIR)),
        "report directory".to_owned(),
    ));
    p.push(Action::flag(
        "--decision-log",
        "decision_log",
        "record AI decision logs".to_owned(),
    ));
    p.push(Action::option(
        "--replays",
        "replays",
        Converter::NonNegativeInt,
        None,
        Value::Int(PyInt::from_u64(u64::from(DEFAULT_REPLAYS))),
        "replays saved per result".to_owned(),
    ));
    for a in extra {
        p.push(a);
    }
    p
}

fn reference_parser() -> Parser {
    let preset = Action::option(
        "--reference-preset",
        "reference_preset",
        Converter::Str,
        Some(REFERENCE_PRESETS.to_vec()),
        Value::Str(PyStr::from("standard")),
        "reference search budget: Python's --ai-preset (default standard)".to_owned(),
    );
    let agent = Action::option(
        "--agent",
        "agent",
        Converter::Str,
        Some(TestAgentKind::ALL.iter().map(|k| k.as_str()).collect()),
        Value::None,
        "play with a test agent of tests/runner/random_agent.py instead of the reference search"
            .to_owned(),
    );
    benchmark_parser("reference-benchmark", preset, vec![agent])
}

fn preset_parser() -> Parser {
    let preset = Action::option(
        "--ai-preset",
        "ai_preset",
        Converter::Str,
        Some(Preset::ALL.iter().map(|p| p.as_str()).collect()),
        Value::Str(PyStr::from(Preset::DEFAULT.as_str())),
        "the preset the configuration stands in for (labels results.json; default standard)"
            .to_owned(),
    );
    let mut config = Action::option(
        "--search-config",
        "search_config",
        Converter::Path,
        None,
        Value::None,
        "JSON file with every strengthened-search setting".to_owned(),
    );
    config.required = true;
    benchmark_parser("preset-benchmark", preset, vec![config])
}

fn head_to_head_parser() -> Parser {
    let mut p = Parser::new(
        &format!("{PROG} head-to-head"),
        Some(
            "Two agents in mirrored blocks of four games on deck pairs, with Wilson intervals \
             per pair and pooled.",
        ),
    );
    p.push(Action::positional(
        "challenger",
        Converter::Str,
        help("agent under test: random, greedy, ref:<preset>, ref:baseline-<preset>, search:<preset> or search:FILE.json, each with optional :field=value,..."),
    ));
    p.push(Action::positional(
        "opponent",
        Converter::Str,
        help("the other agent, same grammar"),
    ));
    p.push(Action::option(
        "--decks",
        "decks",
        Converter::Str,
        Some(DeckSet::ALL.iter().map(|d| d.as_str()).collect()),
        Value::Str(PyStr::from(DeckSet::Tests.as_str())),
        "deck set; every pair of its decks is played (default tests)".to_owned(),
    ));
    p.push(Action::option(
        "--pairs",
        "pairs",
        Converter::Str,
        Some(PairSet::ALL.iter().map(|s| s.as_str()).collect()),
        Value::Str(PyStr::from(PairSet::All.as_str())),
        "which pairs of the deck set (held-out split: docs/port/eval/panel.json)".to_owned(),
    ));
    p.push(Action::append(
        "--pair",
        "pair",
        Converter::Str,
        None,
        "explicit pairs DECK_A:DECK_B[,DECK_C:DECK_D...] (deck names or files); repeatable; replaces --decks"
            .to_owned(),
    ));
    p.push(Action::option(
        "--games",
        "games",
        Converter::PositiveInt,
        None,
        Value::Int(PyInt::from_u64(400)),
        "games in all, a multiple of 4 per pair (default 400)".to_owned(),
    ));
    p.push(Action::option(
        "--seed",
        "seed",
        Converter::Seed,
        None,
        Value::Int(PyInt::from_u64(1)),
        "master seed (default 1)".to_owned(),
    ));
    p.push(Action::option(
        "--role",
        "role",
        Converter::Str,
        Some(SeedRole::ALL.iter().map(|r| r.as_str()).collect()),
        Value::None,
        "seed role: the seed must lie in its range and is recorded in the ledger".to_owned(),
    ));
    p.push(Action::option(
        "--ledger",
        "ledger",
        Converter::Path,
        None,
        Value::Path(PyStr::from(DEFAULT_LEDGER)),
        format!("seed ledger of --role runs (default {DEFAULT_LEDGER})"),
    ));
    p.push(Action::option(
        "--workers",
        "workers",
        Converter::Workers,
        None,
        Value::Int(PyInt::from_u64(1)),
        "worker threads; auto uses one per logical CPU".to_owned(),
    ));
    p.push(Action::option(
        "--choose-first",
        "choose_first",
        Converter::Str,
        Some(ChooseFirst::ALL.iter().map(|c| c.as_str()).collect()),
        Value::Str(PyStr::from(ChooseFirst::DEFAULT.as_str())),
        "seat0: seat 0 goes first in every game (balanced play/draw); agents: the chooser's agent decides".to_owned(),
    ));
    p.push(Action::option(
        "--out",
        "out",
        Converter::Path,
        None,
        Value::None,
        "also write the report, with every game, here".to_owned(),
    ));
    p
}

fn compare_parser() -> Parser {
    let mut p = Parser::new(&format!("{PROG} compare"), Some(COMPARE_DESCRIPTION));
    p.push(Action::positional("challenger", Converter::Str, None));
    p.push(Action::positional("opponent", Converter::Str, None));
    p.push(Action::option(
        "--games",
        "games",
        Converter::Int,
        None,
        Value::Int(PyInt::from_u64(400)),
        "a multiple of 4 per deck pairing".to_owned(),
    ));
    let mut seed = Action::option(
        "--seed",
        "seed",
        Converter::Int,
        None,
        Value::Int(PyInt::from_u64(1)),
        String::new(),
    );
    seed.help = None;
    p.push(seed);
    let mut workers = Action::option(
        "--workers",
        "workers",
        Converter::Int,
        None,
        Value::Int(PyInt::from_u64(1)),
        String::new(),
    );
    workers.help = None;
    p.push(workers);
    let mut decks = Action::option(
        "--decks",
        "decks",
        Converter::Str,
        None,
        Value::Str(PyStr::from("tests")),
        String::new(),
    );
    decks.help = None;
    p.push(decks);
    p.push(Action::option(
        "--out",
        "out",
        Converter::Path,
        None,
        Value::None,
        "also write the report, with every game, here".to_owned(),
    ));
    p
}

/// The `gcg-eval` parser.
#[must_use]
pub fn build_parser() -> Parser {
    let env = Parser::new(&format!("{PROG} env"), None);
    let mut top = Parser::new(PROG, Some(DESCRIPTION));
    top.push(Action::subcommands(
        "command",
        vec![
            SubCommand {
                name: "reference-benchmark",
                help: Some("the benchmark pipeline with the reference search and its presets"),
                parser: reference_parser(),
            },
            SubCommand {
                name: "preset-benchmark",
                help: Some("the gcg-sim benchmark pipeline with a search configuration file"),
                parser: preset_parser(),
            },
            SubCommand {
                name: "head-to-head",
                help: Some("two agents in mirrored blocks of four on deck pairs"),
                parser: head_to_head_parser(),
            },
            SubCommand {
                name: "compare",
                help: Some("python -m gcg_sim.ai.compare"),
                parser: compare_parser(),
            },
            SubCommand {
                name: "env",
                help: Some("build, CPU tiers and CPU features as JSON"),
                parser: env,
            },
        ],
    ));
    top
}

/// What `run` dispatches.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Command {
    ReferenceBenchmark {
        args: BenchArgs,
        agent: Option<TestAgentKind>,
    },
    PresetBenchmark {
        args: BenchArgs,
        search_config: PathBuf,
    },
    HeadToHead(HeadToHeadArgs),
    Compare(CompareArgs),
    Env,
}

/// The text of a parsed string (lossy for paths that are not UTF-8).
fn text_of(s: &PyStr) -> String {
    s.to_string_opt()
        .unwrap_or_else(|| String::from_utf8_lossy(&s.to_surrogateescape_bytes()).into_owned())
}

fn get_text(ns: &Namespace, dest: &str) -> Option<String> {
    match ns.get(dest) {
        Some(Value::Str(s) | Value::Path(s)) => Some(text_of(s)),
        _ => None,
    }
}

fn need_text(ns: &Namespace, dest: &str) -> String {
    get_text(ns, dest).unwrap_or_else(|| unreachable!("{dest} is a str or Path argument"))
}

fn get_path(ns: &Namespace, dest: &str) -> Option<PathBuf> {
    match ns.get(dest) {
        Some(Value::Path(s)) => Some(PathBuf::from(s.to_os_string())),
        _ => None,
    }
}

fn int(ns: &Namespace, dest: &str) -> PyInt {
    match ns.get(dest) {
        Some(Value::Int(v)) => v.clone(),
        _ => unreachable!("{dest} is an integer argument"),
    }
}

fn flag(ns: &Namespace, dest: &str) -> bool {
    matches!(ns.get(dest), Some(Value::Bool(true)))
}

/// A count option held as `u32`: above `u32::MAX` a usage error (Python accepts any size).
fn to_u32(v: &PyInt, option: &str, parser: &Parser, columns: i64) -> Result<u32, Exit> {
    v.to_u64()
        .and_then(|x| u32::try_from(x).ok())
        .ok_or_else(|| {
            let msg = format!(
                "argument {option}: must be at most {}, got {}",
                u32::MAX,
                v.to_decimal()
            );
            parser.error(&PyStr::from(msg.as_str()), columns)
        })
}

fn workers(ns: &Namespace) -> Workers {
    match ns.get("workers") {
        Some(Value::Auto) => Workers::Auto,
        _ => Workers::Count(
            int(ns, "workers")
                .to_u64()
                .map_or(u32::MAX, |v| u32::try_from(v).unwrap_or(u32::MAX)),
        ),
    }
}

fn choice<T: Copy>(all: &[T], name: impl Fn(T) -> &'static str, value: &str) -> T {
    all.iter()
        .copied()
        .find(|x| name(*x) == value)
        .unwrap_or_else(|| unreachable!("{value} is one of its choices"))
}

fn bench_args(
    ns: &Namespace,
    preset: &'static str,
    sub: &Parser,
    columns: i64,
) -> Result<BenchArgs, Exit> {
    Ok(BenchArgs {
        deck_under_test: need_text(ns, "deck_under_test"),
        benchmark_deck: need_text(ns, "benchmark_deck"),
        matches: to_u32(&int(ns, "matches"), "--matches", sub, columns)?,
        seed: int(ns, "seed")
            .to_u64()
            .unwrap_or_else(|| unreachable!("_seed keeps the seed in [0, 2**64)")),
        workers: workers(ns),
        format: choice(&Format::ALL, Format::as_str, &need_text(ns, "format")),
        preset,
        out: need_text(ns, "out"),
        decision_log: flag(ns, "decision_log"),
        replays: to_u32(&int(ns, "replays"), "--replays", sub, columns)?,
    })
}

/// A `type=int` option of `compare.py` (`ai/compare.py:203-205`, converted by the parser's
/// [`Converter::Int`]) as `i128`: a usage error beyond what this port holds (Python accepts
/// any size).
fn compare_int(
    ns: &Namespace,
    dest: &str,
    option: &str,
    sub: &Parser,
    columns: i64,
) -> Result<i128, Exit> {
    int_arg(&int(ns, dest)).map_err(|e| {
        sub.error(
            &PyStr::from(format!("argument {option}: {e}").as_str()),
            columns,
        )
    })
}

/// The typed command of a parsed namespace.
///
/// # Errors
/// A usage error of a value this port cannot hold.
pub fn command(ns: &Namespace, columns: i64) -> Result<Command, Exit> {
    let parser = build_parser();
    let name = need_text(ns, "command");
    let sub = parser
        .subcommand(&name)
        .unwrap_or_else(|| unreachable!("{name} is a subcommand"));
    match name.as_str() {
        "reference-benchmark" => {
            let preset = choice(
                &REFERENCE_PRESETS,
                |p| p,
                &need_text(ns, "reference_preset"),
            );
            let agent = get_text(ns, "agent")
                .map(|a| choice(&TestAgentKind::ALL, TestAgentKind::as_str, &a));
            Ok(Command::ReferenceBenchmark {
                args: bench_args(ns, preset, sub, columns)?,
                agent,
            })
        }
        "preset-benchmark" => {
            let preset = choice(&Preset::ALL, Preset::as_str, &need_text(ns, "ai_preset"));
            Ok(Command::PresetBenchmark {
                args: bench_args(ns, preset.as_str(), sub, columns)?,
                search_config: get_path(ns, "search_config")
                    .unwrap_or_else(|| unreachable!("--search-config is required")),
            })
        }
        "head-to-head" => {
            let source = match ns.get("pair") {
                Some(Value::List(items)) => PairSource::Named(
                    items
                        .iter()
                        .filter_map(|v| match v {
                            Value::Str(s) => Some(text_of(s)),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join(","),
                ),
                _ => PairSource::Set {
                    decks: choice(&DeckSet::ALL, DeckSet::as_str, &need_text(ns, "decks")),
                    pairs: choice(&PairSet::ALL, PairSet::as_str, &need_text(ns, "pairs")),
                },
            };
            Ok(Command::HeadToHead(HeadToHeadArgs {
                challenger: need_text(ns, "challenger"),
                opponent: need_text(ns, "opponent"),
                source,
                games: to_u32(&int(ns, "games"), "--games", sub, columns)?,
                seed: int(ns, "seed")
                    .to_u64()
                    .unwrap_or_else(|| unreachable!("_seed keeps the seed in [0, 2**64)")),
                role: get_text(ns, "role").map(|r| choice(&SeedRole::ALL, SeedRole::as_str, &r)),
                ledger: get_path(ns, "ledger").unwrap_or_else(|| PathBuf::from(DEFAULT_LEDGER)),
                workers: workers(ns),
                choose_first: choice(
                    &ChooseFirst::ALL,
                    ChooseFirst::as_str,
                    &need_text(ns, "choose_first"),
                ),
                out: get_path(ns, "out"),
            }))
        }
        "compare" => Ok(Command::Compare(CompareArgs {
            challenger: need_text(ns, "challenger"),
            opponent: need_text(ns, "opponent"),
            games: compare_int(ns, "games", "--games", sub, columns)?,
            seed: compare_int(ns, "seed", "--seed", sub, columns)?,
            workers: compare_int(ns, "workers", "--workers", sub, columns)?,
            decks: need_text(ns, "decks"),
            out: match ns.get("out") {
                Some(Value::Path(s)) => Some(s.clone()),
                _ => None,
            },
        })),
        _ => Ok(Command::Env),
    }
}

/// `parse_args(argv)` and the typed command.
///
/// # Errors
/// The help or usage [`Exit`].
pub fn parse(args: &[OsString], columns: i64) -> Result<Command, Exit> {
    let args: Vec<PyStr> = args.iter().map(|a| PyStr::from_os(a)).collect();
    let ns = build_parser().parse_args(args, columns)?;
    command(&ns, columns)
}

/// The terminal width argparse formats for (as `gcg-sim`'s parser: `COLUMNS`, else 80).
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

fn run_head_to_head(
    args: &HeadToHeadArgs,
    cancel: &Cancel,
    tty: bool,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<(), CommandError> {
    let plan = head_to_head::plan(args).map_err(CommandError::Invalid)?;
    let total = u64::try_from(plan.specs.len()).unwrap_or(u64::MAX);
    let outcomes = {
        let mut printer = ProgressPrinter::new(err, tty, "games");
        let mut progress = |done: usize| printer.update(u64::try_from(done).unwrap_or(0), total);
        play_all(
            &plan.specs,
            args.workers,
            args.choose_first,
            cancel,
            &mut progress,
        )
        .map_err(CommandError::from)?
    };
    let names = head_to_head::names(&plan);
    let report = head_to_head::report(
        args,
        [&names[0], &names[1]],
        &plan.selection,
        &plan.specs,
        &outcomes,
    );
    let (file, stdout) = outputs(&report);
    if let Some(path) = &args.out {
        std::fs::write(path, file).map_err(|e| {
            CommandError::Invalid(format!("cannot write {}: {e}", gcg_data::py_path_str(path)))
        })?;
    }
    if let Some(role) = args.role {
        ledger::record(&args.ledger, &head_to_head::ledger_entry(args, role))
            .map_err(CommandError::Invalid)?;
    }
    let _ = out.write_all(stdout.as_bytes());
    Ok(())
}

fn run_command(
    command: &Command,
    cancel: &Cancel,
    tty: bool,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<(), CommandError> {
    match command {
        Command::ReferenceBenchmark { args, agent } => {
            let factory: Box<dyn gcg_runner::AgentFactory> = match agent {
                Some(kind) => gcg_runner::test_agents::factory(*kind),
                None => Box::new(ReferenceFactory),
            };
            benchmark(args, SchemaVersion::V1_1, &*factory, cancel, tty, out, err)
        }
        Command::PresetBenchmark {
            args,
            search_config,
        } => {
            let config =
                crate::search_config::load(search_config).map_err(CommandError::Invalid)?;
            let factory = SearchConfigFactory { config };
            benchmark(args, SchemaVersion::V1_1, &factory, cancel, tty, out, err)
        }
        Command::HeadToHead(args) => run_head_to_head(args, cancel, tty, out, err),
        Command::Compare(args) => {
            let workers = args.workers();
            let mut play = |specs: &[crate::MirroredSpec]| {
                play_all(specs, workers, ChooseFirst::Agents, cancel, &mut |_| {})
                    .map_err(CommandError::from)
            };
            let stdout = crate::compare::run(args, &mut play)?;
            let _ = out.write_all(stdout.as_bytes());
            Ok(())
        }
        Command::Env => {
            let doc: Json = crate::env::current();
            let _ = writeln!(out, "{}", dumps(&doc, indented(2)));
            Ok(())
        }
    }
}

/// `main`: parse (help and usage errors exit as argparse does), run, then `error: {message}`
/// with exit 1, or the interrupt message with exit 130. Write failures are ignored, as
/// argparse's `_print_message` ignores them.
pub fn run(
    args: &[OsString],
    columns: Option<&OsStr>,
    cancel: &Cancel,
    tty: bool,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> u8 {
    let columns = terminal_columns(columns);
    let command = match parse(args, columns) {
        Ok(c) => c,
        Err(exit) => {
            let _ = out.write_all(&exit.stdout);
            let _ = err.write_all(&exit.stderr);
            let _ = out.flush();
            let _ = err.flush();
            return exit.status;
        }
    };
    let result = run_command(&command, cancel, tty, out, err);
    let _ = out.flush();
    match result {
        Ok(()) => EXIT_OK,
        Err(CommandError::Invalid(message)) => {
            let _ = writeln!(err, "error: {message}");
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
