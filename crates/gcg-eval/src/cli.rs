//! The `gcg-eval` command line (docs/port/PLAN.md §2.1, §10.4 F, §12.2 P6-I, §12.4-§12.8, A30),
//! on the argparse-compatible parser that `gcg-sim` shares (`gcg_runner::argparse`), with
//! `gcg-sim`'s exit codes (0, 1 with `error: {message}`, 2 for usage errors, 130 on SIGINT).
//!
//! - `reference-benchmark DUT BENCH` takes every `gcg-sim benchmark` option, with
//!   `--reference-preset fast|standard|strong` (default `standard`) in place of `--ai-preset`,
//!   and `--agent test-random|test-illegal` for the runner's test agents (the
//!   `GCG_SIM_AGENT_FACTORY` hook of `cli.py:162-164`). It writes Python's preset and agent
//!   names and results schema 1.1.0 (PLAN A6, A9, A10).
//! - `preset-benchmark DUT BENCH --search-config FILE` is the `gcg-sim benchmark` pipeline with
//!   the strengthened search configured by FILE ([`crate::search_config`]); `--ai-preset`
//!   (`standard`, `fast`) only labels the run (PLAN §12.8). Both benchmark commands take the
//!   opt-in `--decision-trace FILE` (W-21), which leaves their reports byte-identical.
//! - `head-to-head CHALLENGER OPPONENT` ([`crate::head_to_head`]).
//! - `compare CHALLENGER OPPONENT` is `python -m gcg_sim.ai.compare` ([`crate::compare`]): its
//!   `type=int` options convert after parsing, so with several errors the first reported can
//!   differ from Python's, and `prog` is `gcg-eval compare`.
//! - The Phase 6 tools: `ablation` and `budget-curve` ([`crate::ablation`]), `fidelity`
//!   ([`crate::fidelity`]), `regret` ([`crate::regret`]), `tune` ([`crate::tune`]),
//!   `calibrate` ([`crate::calibrate`]), `compare-runs` ([`crate::compare_runs`]),
//!   `decks --overlap` ([`crate::overlap`]), `gates` ([`crate::gates`]) and
//!   `evidence add|check` ([`crate::evidence`]). Lists (scales, fields, lambdas, variants,
//!   deck sets) are comma-separated.
//! - `env` prints [`crate::env`]'s document.

use std::ffi::{OsStr, OsString};
use std::io::Write;
use std::path::PathBuf;

use gcg_ai::FeatureSet;
use gcg_core::pool::Cancel;
use gcg_core::pyjson::{Json, dumps};
use gcg_runner::argparse::{
    Action, ActionKind, Converter, Exit, Namespace, Parser, PyInt, PyStr, SubCommand, Value, pyint,
};
use gcg_runner::{
    DEFAULT_OUT_DIR, DEFAULT_REPLAYS, EXIT_INTERRUPTED, EXIT_INVALID, EXIT_OK, Format,
    INTERRUPTED_MESSAGE, SchemaVersion, TestAgentKind, Workers,
};
use gcg_search::Preset;

use crate::ablation::{
    AblationArgs, BUDGET_FIELDS, BudgetCurveArgs, Bundle, RowKind, run_ablation, run_budget_curve,
};
use crate::bench_cmd::{BenchArgs, CommandError, ProgressPrinter, benchmark};
use crate::calibrate::{CalibrateArgs, DEFAULT_BINS};
use crate::compare::{CompareArgs, indented, int_arg, outputs};
use crate::compare_runs::CompareRunsArgs;
use crate::evidence::EvidenceArgs;
use crate::factories::{ReferenceFactory, SearchConfigFactory};
use crate::fidelity::FidelityArgs;
use crate::gates::GatesArgs;
use crate::head_to_head::{self, HeadToHeadArgs, PairSource};
use crate::ledger::{self, DEFAULT_LEDGER};
use crate::overlap::{DEFAULT_BOUND, OverlapArgs};
use crate::regret::{Decisions, Judge, RegretArgs};
use crate::rows::RunOptions;
use crate::selfplay::{GameResult, play_all, play_all_traced, play_observed, run_indexed};
use crate::tune::{DEFAULT_FOLDS, NEWTON_STEPS, TuneArgs};
use crate::{ChooseFirst, DeckSet, PairSet, SeedRole};

/// `prog` of the top-level parser.
pub const PROG: &str = "gcg-eval";
/// The top-level description.
pub const DESCRIPTION: &str = "Evaluation harness of the gcg-sim port: the reference search, \
     head-to-head comparisons of agents and the tuning tools.";
/// `--workers` help, as `gcg-sim`'s (PLAN A5, §7.4).
pub const WORKERS_HELP: &str =
    "worker processes; auto uses one per performance-core CPU (at most one per match)";
/// The reference presets, in `PRESETS` order (`ai/config.py:80-111`).
pub const REFERENCE_PRESETS: [&str; 3] = ["fast", "standard", "strong"];
/// The reference specs an ablation row can start from.
pub const ROW_REFERENCES: [&str; 5] = [
    "fast",
    "standard",
    "strong",
    "baseline-standard",
    "baseline-strong",
];
/// `compare`'s description: the first line of `ai/compare.py`'s docstring.
pub const COMPARE_DESCRIPTION: &str =
    "Head-to-head comparison of two AI versions in mirrored blocks of four games.";
const AGENT_GRAMMAR: &str = "random, greedy, ref:<preset>, ref:baseline-<preset>, \
     search:<preset>, search:null-<preset> or search:FILE.json, each with optional \
     :field=value,...";

fn help(s: &str) -> Option<String> {
    Some(s.to_owned())
}

fn decision_trace_option() -> Action {
    Action::option(
        "--decision-trace",
        "decision_trace",
        Converter::Path,
        None,
        Value::None,
        "also write every decision's search trace here (NDJSON; W-21)".to_owned(),
    )
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
    p.push(decision_trace_option());
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

/// What the run options of a subcommand default to.
struct RunDefaults {
    /// Whether the command selects pairs (`--decks`, `--pairs`, `--pair`).
    pairs: bool,
    decks: DeckSet,
    pair_set: PairSet,
    games: u64,
    games_help: &'static str,
    seed: u64,
}

/// The options of [`RunOptions`]: pairs, games, seed, role, ledger, workers, play/draw, out.
fn push_run_options(p: &mut Parser, d: &RunDefaults) {
    if d.pairs {
        p.push(Action::option(
            "--decks",
            "decks",
            Converter::Str,
            Some(DeckSet::ALL.iter().map(|s| s.as_str()).collect()),
            Value::Str(PyStr::from(d.decks.as_str())),
            format!(
                "deck set; every pair of its decks is played (default {})",
                d.decks.as_str()
            ),
        ));
        p.push(Action::option(
            "--pairs",
            "pairs",
            Converter::Str,
            Some(PairSet::ALL.iter().map(|s| s.as_str()).collect()),
            Value::Str(PyStr::from(d.pair_set.as_str())),
            format!(
                "which pairs of the deck set (held-out split: docs/port/eval/panel.json; \
                 default {})",
                d.pair_set.as_str()
            ),
        ));
        p.push(Action::append(
            "--pair",
            "pair",
            Converter::Str,
            None,
            "explicit pairs DECK_A:DECK_B[,DECK_C:DECK_D...] (deck names or files); repeatable; replaces --decks"
                .to_owned(),
        ));
    }
    p.push(Action::option(
        "--games",
        "games",
        Converter::PositiveInt,
        None,
        Value::Int(PyInt::from_u64(d.games)),
        format!("{} (default {})", d.games_help, d.games),
    ));
    p.push(Action::option(
        "--seed",
        "seed",
        Converter::Seed,
        None,
        Value::Int(PyInt::from_u64(d.seed)),
        format!("master seed (default {})", d.seed),
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
        "worker threads; auto uses one per performance-core CPU".to_owned(),
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
        Some(format!("agent under test: {AGENT_GRAMMAR}")),
    ));
    p.push(Action::positional(
        "opponent",
        Converter::Str,
        help("the other agent, same grammar"),
    ));
    push_run_options(
        &mut p,
        &RunDefaults {
            pairs: true,
            decks: DeckSet::Tests,
            pair_set: PairSet::All,
            games: 400,
            games_help: "games in all, a multiple of 4 per pair",
            seed: 1,
        },
    );
    p.push(decision_trace_option());
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

fn str_option(
    option: &'static str,
    dest: &'static str,
    default: Option<&str>,
    help: &str,
) -> Action {
    Action::option(
        option,
        dest,
        Converter::Str,
        None,
        default.map_or(Value::None, |d| Value::Str(PyStr::from(d))),
        help.to_owned(),
    )
}

fn path_option(option: &'static str, dest: &'static str, help: &str) -> Action {
    Action::option(
        option,
        dest,
        Converter::Path,
        None,
        Value::None,
        help.to_owned(),
    )
}

fn int_option(
    option: &'static str,
    dest: &'static str,
    convert: Converter,
    default: u64,
    help: &str,
) -> Action {
    Action::option(
        option,
        dest,
        convert,
        None,
        Value::Int(PyInt::from_u64(default)),
        format!("{help} (default {default})"),
    )
}

fn required(mut a: Action) -> Action {
    a.required = true;
    a
}

/// The protocol defaults of the row tools (PLAN §12.4: 400 games on the panel's 15 tuning
/// pairs).
const ROW_DEFAULTS: RunDefaults = RunDefaults {
    pairs: true,
    decks: DeckSet::Panel,
    pair_set: PairSet::Tuning,
    games: 400,
    games_help: "games per row, a multiple of 4, blocks dealt round-robin over the pairs",
    seed: 1,
};

fn ablation_parser() -> Parser {
    let mut p = Parser::new(
        &format!("{PROG} ablation"),
        Some(
            "An ablation row (PLAN §12.4): individual (the reference configuration plus one \
             mechanism against the reference) or leave-one-out (the bundle against the bundle \
             without one mechanism).",
        ),
    );
    p.push(Action {
        option_strings: Vec::new(),
        dest: Some("row"),
        kind: ActionKind::Store {
            convert: Converter::Str,
            choices: Some(RowKind::ALL.iter().map(|k| k.as_str()).collect()),
        },
        required: true,
        help: help("individual or leave-one-out"),
        default: Some(Value::None),
    });
    p.push(required(str_option(
        "--mechanism",
        "mechanism",
        None,
        "field=value[,...] that switches the mechanism on (individual) or off (leave-one-out)",
    )));
    p.push(Action::option(
        "--reference",
        "reference",
        Converter::Str,
        Some(ROW_REFERENCES.to_vec()),
        Value::Str(PyStr::from("strong")),
        "individual: the reference preset (default strong)".to_owned(),
    ));
    p.push(str_option(
        "--bundle",
        "bundle",
        Some("search:standard"),
        "leave-one-out: the bundle, a search: spec (default search:standard)",
    ));
    p.push(path_option(
        "--search-config",
        "search_config",
        "leave-one-out: the bundle from a configuration file (replaces --bundle)",
    ));
    push_run_options(&mut p, &ROW_DEFAULTS);
    p
}

fn budget_curve_parser() -> Parser {
    let mut p = Parser::new(
        &format!("{PROG} budget-curve"),
        Some("A search: challenger at scaled budgets against a fixed opponent (PLAN §12.8)."),
    );
    p.push(Action::positional(
        "challenger",
        Converter::Str,
        help("a search: spec whose budget fields are scaled"),
    ));
    p.push(Action::positional(
        "opponent",
        Converter::Str,
        Some(format!("the fixed opponent: {AGENT_GRAMMAR}")),
    ));
    p.push(str_option(
        "--scales",
        "scales",
        Some("0.25,0.5,1,2,4,8"),
        "comma-separated budget scales (default 0.25,0.5,1,2,4,8)",
    ));
    p.push(str_option(
        "--fields",
        "fields",
        Some(&BUDGET_FIELDS.join(",")),
        "comma-separated integer fields to scale (default iterations_per_option,min_iterations,max_iterations)",
    ));
    push_run_options(&mut p, &ROW_DEFAULTS);
    p
}

fn fidelity_parser() -> Parser {
    let mut p = Parser::new(
        &format!("{PROG} fidelity"),
        Some("Do two agents rank deck variants alike on the same deals (W-18)?"),
    );
    p.push(Action::positional(
        "agent_a",
        Converter::Str,
        help("first agent (plays both seats)"),
    ));
    p.push(Action::positional(
        "agent_b",
        Converter::Str,
        help("second agent (plays both seats)"),
    ));
    p.push(required(str_option(
        "--variants",
        "variants",
        None,
        "comma-separated deck variants (deck names or files), at least two",
    )));
    p.push(required(str_option(
        "--against",
        "against",
        None,
        "the deck every variant plays against",
    )));
    p.push(str_option(
        "--min-spearman",
        "min_spearman",
        None,
        "exit 1 when the rank correlation is below this value",
    ));
    push_run_options(
        &mut p,
        &RunDefaults {
            pairs: false,
            decks: DeckSet::Tests,
            pair_set: PairSet::All,
            games: 400,
            games_help: "games per variant and agent, a multiple of 4",
            seed: 1,
        },
    );
    p
}

fn regret_parser() -> Parser {
    let mut p = Parser::new(
        &format!("{PROG} regret"),
        Some("Decision-level regret of a candidate against an oracle search (W-43)."),
    );
    p.push(Action::positional(
        "candidate",
        Converter::Str,
        Some(format!("the agent judged: {AGENT_GRAMMAR}")),
    ));
    p.push(Action::positional(
        "oracle",
        Converter::Str,
        help("the much larger search"),
    ));
    p.push(str_option(
        "--recorder",
        "recorder",
        None,
        "the agent whose self-play games give the positions (default the candidate)",
    ));
    p.push(int_option(
        "--per-game",
        "per_game",
        Converter::PositiveInt,
        2,
        "positions sampled per game",
    ));
    p.push(str_option(
        "--decisions",
        "decisions",
        Some("all"),
        "which decisions are sampled: all, target, or a decision kind such as main or select (default all)",
    ));
    p.push(int_option(
        "--worlds",
        "worlds",
        Converter::PositiveInt,
        8,
        "shared worlds per position",
    ));
    p.push(str_option(
        "--judge",
        "judge",
        Some("policy"),
        "policy (the playout policy) or an agent spec that finishes the playouts (default policy)",
    ));
    p.push(str_option(
        "--temperature",
        "temperature",
        Some("0"),
        "playout policy temperature (default 0)",
    ));
    p.push(int_option(
        "--max-decisions",
        "max_decisions",
        Converter::PositiveInt,
        2000,
        "decisions per playout before the evaluation scores it",
    ));
    push_run_options(
        &mut p,
        &RunDefaults {
            games: 40,
            games_help: "recording games, a multiple of 4",
            ..ROW_DEFAULTS
        },
    );
    p
}

fn tune_parser() -> Parser {
    let mut p = Parser::new(
        &format!("{PROG} tune"),
        Some(
            "Fit the evaluation weights by L2 logistic regression with cross-validation by game (W-40).",
        ),
    );
    p.push(str_option(
        "--agent",
        "agent",
        Some("search:standard"),
        "the self-play agent; its weights are the starting weights (default search:standard)",
    ));
    p.push(Action::option(
        "--feature-set",
        "feature_set",
        Converter::Str,
        Some(FeatureSet::ALL.iter().map(|f| f.as_str()).collect()),
        Value::None,
        "features recorded and fitted (default the agent's weights' set)".to_owned(),
    ));
    p.push(int_option(
        "--folds",
        "folds",
        Converter::PositiveInt,
        u64::from(DEFAULT_FOLDS),
        "cross-validation folds",
    ));
    p.push(str_option(
        "--lambdas",
        "lambdas",
        Some("0.01,0.1,1,10,100"),
        "comma-separated L2 strengths scored by cross-validation (default 0.01,0.1,1,10,100)",
    ));
    p.push(int_option(
        "--steps",
        "steps",
        Converter::PositiveInt,
        u64::from(NEWTON_STEPS),
        "Newton steps",
    ));
    p.push(Action::option(
        "--holdout-pairs",
        "holdout_pairs",
        Converter::NonNegativeInt,
        None,
        Value::None,
        "the last N selected pairs are only the held-out check (default 1 with two or more pairs)"
            .to_owned(),
    ));
    p.push(int_option(
        "--validate",
        "validate",
        Converter::NonNegativeInt,
        0,
        "validation games, fitted against starting weights",
    ));
    p.push(path_option(
        "--positions",
        "positions",
        "fit these recorded positions instead of playing",
    ));
    p.push(path_option(
        "--record",
        "record",
        "write the positions here",
    ));
    push_run_options(
        &mut p,
        &RunDefaults {
            games: 240,
            games_help: "self-play games, a multiple of 4",
            ..ROW_DEFAULTS
        },
    );
    p
}

fn calibrate_parser() -> Parser {
    let mut p = Parser::new(
        &format!("{PROG} calibrate"),
        Some("Simulated matchup rates against observed results (W-41)."),
    );
    p.push(Action::positional(
        "observed",
        Converter::Path,
        help("observed matches (crates/gcg-eval/schema/observed-matches.v1.schema.json)"),
    ));
    p.push(str_option(
        "--agent",
        "agent",
        Some("search:standard"),
        "the agent of both seats (default search:standard)",
    ));
    p.push(int_option(
        "--bins",
        "bins",
        Converter::PositiveInt,
        u64::from(DEFAULT_BINS),
        "reliability bins",
    ));
    push_run_options(
        &mut p,
        &RunDefaults {
            pairs: false,
            decks: DeckSet::Tests,
            pair_set: PairSet::All,
            games: 400,
            games_help: "games per matchup, a multiple of 4",
            seed: 1,
        },
    );
    p
}

fn compare_runs_parser() -> Parser {
    let mut p = Parser::new(
        &format!("{PROG} compare-runs"),
        Some("Paired analysis of two same-seed benchmark report directories (W-42)."),
    );
    p.push(Action::positional(
        "run_a",
        Converter::Path,
        help("first report directory"),
    ));
    p.push(Action::positional(
        "run_b",
        Converter::Path,
        help("second report directory"),
    ));
    p.push(path_option("--out", "out", "also write the report here"));
    p.push(Action::flag(
        "--allow-mixed",
        "allow_mixed",
        "compare runs whose AI identity or data and code versions differ (W-91)".to_owned(),
    ));
    p
}

fn decks_parser() -> Parser {
    let mut p = Parser::new(&format!("{PROG} decks"), Some("Deck-set tools (W-20)."));
    p.push(Action::flag(
        "--overlap",
        "overlap",
        "main-deck copy overlaps between deck sets".to_owned(),
    ));
    p.push(str_option(
        "--rows",
        "rows",
        Some("panel"),
        "comma-separated deck sets of the rows (default panel)",
    ));
    p.push(str_option(
        "--cols",
        "cols",
        Some("tuning,tests"),
        "comma-separated deck sets of the columns (default tuning,tests)",
    ));
    p.push(int_option(
        "--bound",
        "bound",
        Converter::NonNegativeInt,
        u64::from(DEFAULT_BOUND),
        "largest overlap a held-out deck may have with a tuning or test deck",
    ));
    p.push(Action::flag(
        "--check",
        "check",
        "exit 1 when a held-out deck of docs/port/eval/panel.json breaks the bound".to_owned(),
    ));
    p.push(path_option("--out", "out", "also write the report here"));
    p
}

fn gates_parser() -> Parser {
    let mut p = Parser::new(
        &format!("{PROG} gates"),
        Some("The final-evaluation gates E1-E4 from head-to-head --out reports (PLAN §12.6)."),
    );
    for (opt, dest, h) in [
        (
            "--e1",
            "e1",
            "E1: search:standard vs ref:strong on the panel",
        ),
        ("--e2", "e2", "E2: search:fast vs ref:standard on the panel"),
        (
            "--e3-tests",
            "e3_tests",
            "E3: search:standard vs random on the AI test decks",
        ),
        (
            "--e3-panel",
            "e3_panel",
            "E3: search:standard vs random on the panel",
        ),
        (
            "--e4-tests",
            "e4_tests",
            "E4: search:standard vs greedy on the AI test decks",
        ),
        (
            "--e4-panel",
            "e4_panel",
            "E4: search:standard vs greedy on the panel",
        ),
        (
            "--losses-documented",
            "losses",
            "E3: the documented losses with their replays",
        ),
        ("--out", "out", "also write the report here"),
    ] {
        p.push(path_option(opt, dest, h));
    }
    p.push(Action::flag(
        "--check",
        "check",
        "exit 1 unless every gate has its reports and passes".to_owned(),
    ));
    p
}

fn evidence_parser() -> Parser {
    let mut add = Parser::new(&format!("{PROG} evidence add"), None);
    add.push(Action::positional(
        "file",
        Converter::Path,
        help("a report under docs/port/eval/evidence/"),
    ));
    add.push(required(str_option(
        "--command",
        "evidence_command",
        None,
        "the exact command that wrote the file",
    )));
    let mut check = Parser::new(&format!("{PROG} evidence check"), None);
    check.push(Action::append(
        "--path",
        "path",
        Converter::Path,
        None,
        "a file or directory to check (repeatable; default docs/port/eval)".to_owned(),
    ));
    let mut p = Parser::new(
        &format!("{PROG} evidence"),
        Some("The evidence directory and its commit and path check (W-127)."),
    );
    p.push(Action::subcommands(
        "evidence",
        vec![
            SubCommand {
                name: "add",
                help: Some("record a report in the evidence manifest"),
                parser: add,
            },
            SubCommand {
                name: "check",
                help: Some("check cited commits, paths and the manifest"),
                parser: check,
            },
        ],
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
                name: "ablation",
                help: Some("an individual or leave-one-out ablation row"),
                parser: ablation_parser(),
            },
            SubCommand {
                name: "budget-curve",
                help: Some("a search at scaled budgets against a fixed opponent"),
                parser: budget_curve_parser(),
            },
            SubCommand {
                name: "fidelity",
                help: Some("do two agents rank deck variants alike"),
                parser: fidelity_parser(),
            },
            SubCommand {
                name: "regret",
                help: Some("decision-level regret against an oracle search"),
                parser: regret_parser(),
            },
            SubCommand {
                name: "tune",
                help: Some("fit the evaluation weights from recorded positions"),
                parser: tune_parser(),
            },
            SubCommand {
                name: "tune-leaf",
                help: Some(crate::tune_ext::HELP),
                parser: {
                    let mut p = crate::tune_ext::parser();
                    push_run_options(&mut p, &ROW_DEFAULTS);
                    p
                },
            },
            SubCommand {
                name: "calibrate",
                help: Some("simulated against observed matchup rates"),
                parser: calibrate_parser(),
            },
            SubCommand {
                name: "compare-runs",
                help: Some("paired analysis of two same-seed benchmark runs"),
                parser: compare_runs_parser(),
            },
            SubCommand {
                name: "decks",
                help: Some("deck-set tools: --overlap"),
                parser: decks_parser(),
            },
            SubCommand {
                name: "gates",
                help: Some("the final-evaluation gates E1-E4"),
                parser: gates_parser(),
            },
            SubCommand {
                name: "evidence",
                help: Some("the evidence directory: add, check"),
                parser: evidence_parser(),
            },
            SubCommand {
                name: "validate",
                help: Some("the deck report of W-71 to W-78 (A30)"),
                parser: crate::validate_cmd::parser(PROG),
            },
            SubCommand {
                name: "env",
                help: Some("build, CPU tiers and CPU features as JSON"),
                parser: env,
            },
            SubCommand {
                name: "benchmark",
                help: Some("gcg-sim benchmark with opt-in analysis, compression, resume"),
                parser: crate::benchmark_opts::benchmark_parser(),
            },
            SubCommand {
                name: "replay-game",
                help: Some("re-simulate and verify one game of a games.ndjson"),
                parser: crate::benchmark_opts::replay_game_parser(),
            },
            SubCommand {
                name: "replay-check",
                help: Some("gcg-sim replay with the data, rules and code version check"),
                parser: crate::benchmark_opts::replay_check_parser(),
            },
        ],
    ));
    top
}

/// What `run` dispatches.
#[derive(Clone, PartialEq, Debug)]
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
    Ablation(AblationArgs),
    BudgetCurve(BudgetCurveArgs),
    Fidelity(FidelityArgs),
    Regret(RegretArgs),
    Tune(TuneArgs),
    TuneLeaf(crate::tune_ext::TuneLeafArgs),
    Calibrate(CalibrateArgs),
    /// The arguments and `--allow-mixed` (W-91).
    CompareRuns(CompareRunsArgs, bool),
    Overlap(OverlapArgs),
    Gates(GatesArgs),
    Evidence(EvidenceArgs),
    Validate(crate::validate_cmd::ValidateArgs),
    Env,
    Benchmark(crate::benchmark_opts::EvalBenchArgs),
    ReplayGame(crate::benchmark_opts::ReplayGameArgs),
    ReplayCheck(crate::benchmark_opts::ReplayCheckArgs),
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

fn need_path(ns: &Namespace, dest: &str) -> PathBuf {
    get_path(ns, dest).unwrap_or_else(|| unreachable!("{dest} is a Path argument"))
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

fn u32_opt(
    ns: &Namespace,
    dest: &str,
    option: &str,
    sub: &Parser,
    columns: i64,
) -> Result<u32, Exit> {
    to_u32(&int(ns, dest), option, sub, columns)
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

fn seed(ns: &Namespace) -> u64 {
    int(ns, "seed")
        .to_u64()
        .unwrap_or_else(|| unreachable!("_seed keeps the seed in [0, 2**64)"))
}

fn usage(sub: &Parser, columns: i64, msg: &str) -> Exit {
    sub.error(&PyStr::from(msg), columns)
}

/// Comma-separated finite numbers.
fn floats(text: &str, option: &str, sub: &Parser, columns: i64) -> Result<Vec<f64>, Exit> {
    text.split(',')
        .map(|t| {
            t.trim()
                .parse::<f64>()
                .ok()
                .filter(|x| x.is_finite())
                .ok_or_else(|| {
                    usage(
                        sub,
                        columns,
                        &format!("argument {option}: not a number: {t:?}"),
                    )
                })
        })
        .collect()
}

/// Comma-separated non-empty items.
fn items(text: &str) -> Vec<String> {
    text.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

fn deck_sets(text: &str, option: &str, sub: &Parser, columns: i64) -> Result<Vec<DeckSet>, Exit> {
    items(text)
        .iter()
        .map(|s| {
            DeckSet::from_value(s).ok_or_else(|| {
                usage(
                    sub,
                    columns,
                    &format!(
                        "argument {option}: unknown deck set {s:?} (choose from {})",
                        DeckSet::ALL.map(DeckSet::as_str).join(", ")
                    ),
                )
            })
        })
        .collect()
}

fn pair_source(ns: &Namespace) -> PairSource {
    match ns.get("pair") {
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
    }
}

/// The [`RunOptions`] of a parsed namespace; `pairs: false` leaves the source empty for the
/// command to fill.
fn run_options(
    ns: &Namespace,
    sub: &Parser,
    columns: i64,
    pairs: bool,
) -> Result<RunOptions, Exit> {
    Ok(RunOptions {
        source: if pairs {
            pair_source(ns)
        } else {
            PairSource::Named(String::new())
        },
        games: u32_opt(ns, "games", "--games", sub, columns)?,
        seed: seed(ns),
        role: get_text(ns, "role").map(|r| choice(&SeedRole::ALL, SeedRole::as_str, &r)),
        ledger: get_path(ns, "ledger").unwrap_or_else(|| PathBuf::from(DEFAULT_LEDGER)),
        workers: workers(ns),
        choose_first: choice(
            &ChooseFirst::ALL,
            ChooseFirst::as_str,
            &need_text(ns, "choose_first"),
        ),
        out: get_path(ns, "out"),
    })
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
        seed: seed(ns),
        workers: workers(ns),
        format: choice(&Format::ALL, Format::as_str, &need_text(ns, "format")),
        preset,
        out: need_text(ns, "out"),
        decision_log: flag(ns, "decision_log"),
        replays: to_u32(&int(ns, "replays"), "--replays", sub, columns)?,
        decision_trace: get_path(ns, "decision_trace"),
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

/// The Phase 6 commands of a parsed namespace.
#[expect(clippy::too_many_lines, reason = "one arm per subcommand")]
fn phase6_command(name: &str, ns: &Namespace, sub: &Parser, columns: i64) -> Result<Command, Exit> {
    Ok(match name {
        "ablation" => {
            let bundle = get_path(ns, "search_config")
                .map_or_else(|| Bundle::Spec(need_text(ns, "bundle")), Bundle::File);
            Command::Ablation(AblationArgs {
                row: choice(&RowKind::ALL, RowKind::as_str, &need_text(ns, "row")),
                mechanism: need_text(ns, "mechanism"),
                reference: need_text(ns, "reference"),
                bundle,
                opts: run_options(ns, sub, columns, true)?,
            })
        }
        "budget-curve" => Command::BudgetCurve(BudgetCurveArgs {
            challenger: need_text(ns, "challenger"),
            opponent: need_text(ns, "opponent"),
            scales: floats(&need_text(ns, "scales"), "--scales", sub, columns)?,
            fields: items(&need_text(ns, "fields")),
            opts: run_options(ns, sub, columns, true)?,
        }),
        "fidelity" => {
            let variants = items(&need_text(ns, "variants"));
            let against = need_text(ns, "against");
            let mut opts = run_options(ns, sub, columns, false)?;
            opts.source = PairSource::Named(
                variants
                    .iter()
                    .map(|v| format!("{v}:{against}"))
                    .collect::<Vec<_>>()
                    .join(","),
            );
            let min_spearman = match get_text(ns, "min_spearman") {
                Some(t) => Some(
                    floats(&t, "--min-spearman", sub, columns)?
                        .first()
                        .copied()
                        .unwrap_or(0.0),
                ),
                None => None,
            };
            Command::Fidelity(FidelityArgs {
                agents: [need_text(ns, "agent_a"), need_text(ns, "agent_b")],
                variants,
                against,
                min_spearman,
                opts,
            })
        }
        "regret" => {
            let judge = need_text(ns, "judge");
            let which = need_text(ns, "decisions");
            let decisions = Decisions::from_value(&which).ok_or_else(|| {
                usage(
                    sub,
                    columns,
                    &format!(
                        "argument --decisions: {which:?} is not all, target or a decision kind"
                    ),
                )
            })?;
            Command::Regret(RegretArgs {
                candidate: need_text(ns, "candidate"),
                oracle: need_text(ns, "oracle"),
                recorder: get_text(ns, "recorder"),
                per_game: u32_opt(ns, "per_game", "--per-game", sub, columns)?,
                decisions,
                worlds: u32_opt(ns, "worlds", "--worlds", sub, columns)?,
                judge: if judge == "policy" {
                    Judge::Policy
                } else {
                    Judge::Agent(judge)
                },
                temperature: floats(&need_text(ns, "temperature"), "--temperature", sub, columns)?
                    .first()
                    .copied()
                    .unwrap_or(0.0),
                max_decisions: u32_opt(ns, "max_decisions", "--max-decisions", sub, columns)?,
                opts: run_options(ns, sub, columns, true)?,
            })
        }
        "tune" => Command::Tune(TuneArgs {
            agent: need_text(ns, "agent"),
            feature_set: get_text(ns, "feature_set").and_then(|f| FeatureSet::from_value(&f)),
            folds: u32_opt(ns, "folds", "--folds", sub, columns)?,
            lambdas: floats(&need_text(ns, "lambdas"), "--lambdas", sub, columns)?,
            steps: u32_opt(ns, "steps", "--steps", sub, columns)?,
            holdout_pairs: match ns.get("holdout_pairs") {
                Some(Value::Int(v)) => Some(to_u32(v, "--holdout-pairs", sub, columns)?),
                _ => None,
            },
            validate: u32_opt(ns, "validate", "--validate", sub, columns)?,
            positions: get_path(ns, "positions"),
            record: get_path(ns, "record"),
            opts: run_options(ns, sub, columns, true)?,
        }),
        "tune-leaf" => Command::TuneLeaf(crate::tune_ext::args(
            ns,
            run_options(ns, sub, columns, true)?,
        )),
        "calibrate" => Command::Calibrate(CalibrateArgs {
            file: need_path(ns, "observed"),
            agent: need_text(ns, "agent"),
            bins: u32_opt(ns, "bins", "--bins", sub, columns)?,
            opts: run_options(ns, sub, columns, false)?,
        }),
        "compare-runs" => Command::CompareRuns(
            CompareRunsArgs {
                a: need_path(ns, "run_a"),
                b: need_path(ns, "run_b"),
                out: get_path(ns, "out"),
            },
            flag(ns, "allow_mixed"),
        ),
        "decks" => {
            if !flag(ns, "overlap") {
                return Err(usage(sub, columns, "the decks command needs --overlap"));
            }
            Command::Overlap(OverlapArgs {
                rows: deck_sets(&need_text(ns, "rows"), "--rows", sub, columns)?,
                cols: deck_sets(&need_text(ns, "cols"), "--cols", sub, columns)?,
                bound: u32_opt(ns, "bound", "--bound", sub, columns)?,
                check: flag(ns, "check"),
                out: get_path(ns, "out"),
            })
        }
        "gates" => Command::Gates(GatesArgs {
            e1: get_path(ns, "e1"),
            e2: get_path(ns, "e2"),
            e3_tests: get_path(ns, "e3_tests"),
            e3_panel: get_path(ns, "e3_panel"),
            e4_tests: get_path(ns, "e4_tests"),
            e4_panel: get_path(ns, "e4_panel"),
            losses: get_path(ns, "losses"),
            check: flag(ns, "check"),
            out: get_path(ns, "out"),
        }),
        "evidence" => {
            if need_text(ns, "evidence") == "add" {
                Command::Evidence(EvidenceArgs::Add {
                    file: need_path(ns, "file"),
                    command: need_text(ns, "evidence_command"),
                })
            } else {
                let paths = match ns.get("path") {
                    Some(Value::List(items)) => items
                        .iter()
                        .filter_map(|v| match v {
                            Value::Path(s) => Some(PathBuf::from(s.to_os_string())),
                            _ => None,
                        })
                        .collect(),
                    _ => Vec::new(),
                };
                Command::Evidence(EvidenceArgs::Check { paths })
            }
        }
        "validate" => Command::Validate(crate::validate_cmd::args(ns, sub, columns)?),
        _ => Command::Env,
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
            let o = run_options(ns, sub, columns, true)?;
            Ok(Command::HeadToHead(HeadToHeadArgs {
                challenger: need_text(ns, "challenger"),
                opponent: need_text(ns, "opponent"),
                source: o.source,
                games: o.games,
                seed: o.seed,
                role: o.role,
                ledger: o.ledger,
                workers: o.workers,
                choose_first: o.choose_first,
                out: o.out,
                decision_trace: get_path(ns, "decision_trace"),
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
        "env" => Ok(Command::Env),
        "benchmark" => crate::benchmark_opts::bench_args(ns, sub, columns).map(Command::Benchmark),
        "replay-game" => {
            crate::benchmark_opts::replay_game_args(ns, sub, columns).map(Command::ReplayGame)
        }
        "replay-check" => Ok(Command::ReplayCheck(
            crate::benchmark_opts::replay_check_args(ns),
        )),
        other => phase6_command(other, ns, sub, columns),
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

/// Plays `specs` traced, with the `games i/n` progress lines on `err`.
///
/// # Errors
/// The lowest failing game, or the interrupt.
pub fn play_with_progress(
    specs: &[crate::MirroredSpec],
    workers: Workers,
    choose_first: ChooseFirst,
    cancel: &Cancel,
    tty: bool,
    err: &mut dyn Write,
) -> Result<Vec<GameResult>, CommandError> {
    let total = u64::try_from(specs.len()).unwrap_or(u64::MAX);
    let mut printer = ProgressPrinter::new(err, tty, "games");
    let mut progress = |done: usize| printer.update(u64::try_from(done).unwrap_or(0), total);
    play_all_traced(specs, workers, choose_first, cancel, &mut progress).map_err(CommandError::from)
}

/// Plays `specs` with a decision trace per game (W-21): the results and the NDJSON text.
///
/// # Errors
/// The lowest failing game (or trace), or the interrupt.
pub fn play_with_trace(
    specs: &[crate::MirroredSpec],
    workers: Workers,
    choose_first: ChooseFirst,
    cancel: &Cancel,
    tty: bool,
    err: &mut dyn Write,
) -> Result<(Vec<GameResult>, String), CommandError> {
    let total = u64::try_from(specs.len()).unwrap_or(u64::MAX);
    let mut printer = ProgressPrinter::new(err, tty, "games");
    let mut progress = |done: usize| printer.update(u64::try_from(done).unwrap_or(0), total);
    let per = run_indexed(
        specs.len(),
        workers,
        cancel,
        &mut progress,
        |i, cx| {
            let s = &specs[i];
            let names = [
                crate::agent_spec::agent_name(&s.agents[0]),
                crate::agent_spec::agent_name(&s.agents[1]),
            ];
            let mut obs = crate::trace::TraceObserver::new(u64::from(s.index), names);
            let r = play_observed(s, choose_first, cx, &mut obs)?;
            if let Some(e) = obs.error {
                return Err(crate::selfplay::GameError::engine(&e));
            }
            Ok((r, obs.lines))
        },
        |i| (specs[i].index, specs[i].game_seed),
    )
    .map_err(CommandError::from)?;
    let mut results = Vec::with_capacity(per.len());
    let mut lines = Vec::new();
    for (r, l) in per {
        results.push(r);
        lines.extend(l);
    }
    Ok((results, crate::trace::ndjson(lines)))
}

fn run_head_to_head(
    args: &HeadToHeadArgs,
    cancel: &Cancel,
    tty: bool,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<(), CommandError> {
    let plan = head_to_head::plan(args).map_err(CommandError::Invalid)?;
    let results = match &args.decision_trace {
        None => play_with_progress(
            &plan.specs,
            args.workers,
            args.choose_first,
            cancel,
            tty,
            err,
        )?,
        Some(path) => {
            let (results, text) = play_with_trace(
                &plan.specs,
                args.workers,
                args.choose_first,
                cancel,
                tty,
                err,
            )?;
            std::fs::write(path, text).map_err(|e| {
                CommandError::Invalid(format!("cannot write {}: {e}", gcg_data::py_path_str(path)))
            })?;
            results
        }
    };
    let names = head_to_head::names(&plan);
    let report = head_to_head::report_results(
        args,
        [&names[0], &names[1]],
        &plan.selection,
        &plan.specs,
        &plan.counts,
        &results,
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
        Command::Ablation(args) => run_ablation(args, cancel, tty, out, err),
        Command::BudgetCurve(args) => run_budget_curve(args, cancel, tty, out, err),
        Command::Fidelity(args) => crate::fidelity::run(args, cancel, tty, out, err),
        Command::Regret(args) => crate::regret::run(args, cancel, tty, out, err),
        Command::Tune(args) => crate::tune::run(args, cancel, tty, out, err),
        Command::TuneLeaf(args) => crate::tune_ext::run(args, cancel, tty, out, err),
        Command::Calibrate(args) => crate::calibrate::run(args, cancel, tty, out, err),
        Command::CompareRuns(args, allow_mixed) => {
            crate::benchmark_opts::guard_runs(&args.a, &args.b, *allow_mixed, err)?;
            crate::compare_runs::run(args, out)
        }
        Command::Overlap(args) => crate::overlap::run(args, out),
        Command::Gates(args) => crate::gates::run(args, out),
        Command::Evidence(args) => crate::evidence::run(args, out),
        Command::Validate(args) => crate::validate_cmd::run(args, out),
        Command::Env => {
            let doc: Json = crate::env::current();
            let _ = writeln!(out, "{}", dumps(&doc, indented(2)));
            Ok(())
        }
        Command::Benchmark(args) => crate::benchmark_opts::benchmark(args, cancel, tty, out, err),
        Command::ReplayGame(args) => crate::benchmark_opts::replay_game(args, out),
        Command::ReplayCheck(args) => crate::benchmark_opts::replay_check(args, out),
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
