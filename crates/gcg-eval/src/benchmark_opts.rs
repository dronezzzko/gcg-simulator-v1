//! The opt-in user tools of A30 for P6-M (docs/port/PLAN.md §12.2): `gcg-eval benchmark`,
//! `gcg-eval replay-game` (W-148) and `gcg-eval replay-check` (W-156).
//!
//! `benchmark` is `gcg-sim benchmark`'s pipeline with the shipped presets: the same deck and
//! directory checks and messages (`gcg_runner::cli_support`), the same agent factory
//! (`gcg_search::make_agent(preset_config(p))`, `ai.factory` = `Preset::factory_description`),
//! the same results schema, reports and stdout. Without an opt-in option it writes the bytes
//! `gcg-sim benchmark` writes (T-153), as one set (W-102,
//! `gcg_runner::reports::write_report_set`). The opt-in options:
//!
//! - `--analysis` writes `analysis.json` (W-90, W-92-W-96, W-98, W-101, W-109, W-147) and, for
//!   its swap tests, `variants/*.txt`; `--swiss-draws` adds the Swiss scoring of W-109 to it;
//! - `--compress` replaces `games.ndjson` by `games.ndjson.zst` (builds with the `zstd`
//!   feature) or `games.ndjson.gz` (the in-house gzip writer), which decompress to the same
//!   bytes (W-99);
//! - `--resume` checkpoints every finished match and resumes a run (W-105);
//! - `--replays-extremes` also saves the shortest and longest win and loss (W-148);
//! - `--diagnostics` writes `diagnostics.json`, the cap kind of every `turn_limit` game (W-46);
//! - `--opponent-list known|archetype` sets the strengthened search's `opponent_list` flag
//!   (W-134; the mechanism itself is P6-C's).
//!
//! `timing.json` also gets the deterministic decision counts (W-104); it is not part of T-153.

use std::io::Write;
use std::path::{Path, PathBuf};

use gcg_ai::{Agent, AgentError, SearchSettings};
use gcg_core::pool::Cancel;
use gcg_core::pyjson::Json;
use gcg_data::py_path_str;
use gcg_effects::Registry;
use gcg_engine::{EndReason, MAX_ACTIONS_DEFAULT, canon};
use gcg_runner::argparse::pypath::path_str;
use gcg_runner::argparse::{Action, Converter, Exit, Namespace, Parser, PyInt, PyStr, Value};
use gcg_runner::checkpoint::{ResumeError, checkpoint_path, run_resumable};
use gcg_runner::cli_support::{
    CliError, ProgressPrinter, legal_deck, rate_line, replay_command, writable_dir,
};
use gcg_runner::reports::aggregate::Outcomes;
use gcg_runner::reports::identity::version_differences;
use gcg_runner::reports::{
    Compression, ResultsData, SetOptions, TimingHost, build_results_with, to_json_text,
    write_report_set,
};
use gcg_runner::{
    AgentFactory, AgentSpec, BenchmarkConfig, DEFAULT_OUT_DIR, DEFAULT_REPLAYS, Format, GameRecord,
    Outcome, ReportError, RunError, SchemaVersion, Workers, run_benchmark,
};
use gcg_search::{
    OpponentList, Preset, StrengthenedConfig, make_agent, preset_config, search_settings,
};
use serde_json::Value as JsonValue;

use crate::bench_cmd::CommandError;
use crate::build_info::{CODE_DIGEST, PACKAGE_VERSION};
use crate::cli::{PROG, WORKERS_HELP};

/// The results schema `gcg-sim benchmark` writes (`gcg-cli`'s `commands::SCHEMA`): 1.2.0, with
/// the strengthened search's `ai.search` and `ai.effort` (PLAN §12.7, A10;
/// docs/port/contracts.md O9). `gcg-eval benchmark` writes the same, so T-153 holds.
pub const SIM_SCHEMA: SchemaVersion = SchemaVersion::V1_2;

/// The options of `gcg-eval benchmark` besides `gcg-sim benchmark`'s.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct OptIns {
    pub analysis: bool,
    pub swiss_draws: bool,
    pub compress: bool,
    pub resume: bool,
    pub replays_extremes: bool,
    pub diagnostics: bool,
    pub opponent_list: Option<OpponentList>,
}

/// The parsed `gcg-eval benchmark` arguments; paths as argparse's `type=Path` text.
#[derive(Clone, PartialEq, Debug)]
pub struct EvalBenchArgs {
    pub deck_under_test: PyStr,
    pub benchmark_deck: PyStr,
    pub matches: u32,
    pub seed: u64,
    pub workers: Workers,
    pub format: Format,
    pub preset: Preset,
    pub out: PyStr,
    pub decision_log: bool,
    pub replays: u32,
    pub opt: OptIns,
}

/// `gcg-eval replay-game`'s arguments (W-148).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ReplayGameArgs {
    pub games: PathBuf,
    pub results: PathBuf,
    pub match_index: u32,
    pub game_number: u32,
}

/// `gcg-eval replay-check`'s arguments (W-156).
#[derive(Clone, PartialEq, Debug)]
pub struct ReplayCheckArgs {
    pub replay: PyStr,
    pub results: Option<PathBuf>,
    pub log: bool,
}

/// The `gcg-sim` agent factory with the `opponent_list` override of `--opponent-list`.
#[derive(Clone, PartialEq, Debug)]
pub struct ShippedFactory {
    preset: Preset,
    config: StrengthenedConfig,
    opponent_list: Option<OpponentList>,
}

impl ShippedFactory {
    /// `gcg-sim`'s factory for `preset`; `opponent_list` overrides the flag.
    #[must_use]
    pub fn new(preset: Preset, opponent_list: Option<OpponentList>) -> Self {
        let mut config = preset_config(preset).clone();
        if let Some(list) = opponent_list {
            config.mechanisms.opponent_list = list;
        }
        Self {
            preset,
            config,
            opponent_list,
        }
    }
}

impl AgentFactory for ShippedFactory {
    fn make(&self, spec: &AgentSpec) -> Result<Box<dyn Agent>, AgentError> {
        Ok(make_agent(&self.config, spec.seed, spec.log))
    }

    /// `gcg-sim`'s `Preset::factory_description`, naming a changed `opponent_list`.
    fn describe(&self) -> String {
        match self.opponent_list {
            Some(list) if list != preset_config(self.preset).mechanisms.opponent_list => {
                format!(
                    "gcg-search::make_agent({}, opponent_list={})",
                    gcg_data::py_repr(self.preset.as_str()),
                    gcg_data::py_repr(list.as_str())
                )
            }
            _ => self.preset.factory_description().to_owned(),
        }
    }

    /// Results 1.2.0 `ai.search` (PLAN §12.7, O9): `gcg-sim`'s settings of the preset, with
    /// the `--opponent-list` override in the mechanisms.
    fn search_block(&self) -> Option<SearchSettings> {
        Some(search_settings(self.preset, &self.config))
    }
}

fn help(s: &str) -> Option<String> {
    Some(s.to_owned())
}

/// `gcg-eval benchmark`'s parser: `gcg-sim benchmark`'s arguments plus the opt-in options.
#[must_use]
pub fn benchmark_parser() -> Parser {
    let mut p = Parser::new(
        &format!("{PROG} benchmark"),
        Some(
            "gcg-sim benchmark with opt-in reports: the same options and, without the options \
             below --replays, the same files.",
        ),
    );
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
    p.push(Action::option(
        "--ai-preset",
        "ai_preset",
        Converter::Str,
        Some(Preset::ALL.iter().map(|p| p.as_str()).collect()),
        Value::Str(PyStr::from(Preset::DEFAULT.as_str())),
        "the shipped preset, as gcg-sim's --ai-preset (default standard)".to_owned(),
    ));
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
    for (option, dest, text) in [
        (
            "--analysis",
            "analysis",
            "also write analysis.json: stratified, clustered and run-level statistics (W-90 ...)",
        ),
        (
            "--swiss-draws",
            "swiss_draws",
            "with --analysis: also score matches decided by a double defeat as Swiss draws (W-109)",
        ),
        (
            "--compress",
            "compress",
            "write games.ndjson compressed (games.ndjson.zst, or .gz without the zstd encoder; W-99)",
        ),
        (
            "--resume",
            "resume",
            "checkpoint finished matches in the report directory and resume from them (W-105)",
        ),
        (
            "--replays-extremes",
            "replays_extremes",
            "also save the shortest and longest win and loss (W-148)",
        ),
        (
            "--diagnostics",
            "diagnostics",
            "also write diagnostics.json: which safety cap ended each capped game (W-46)",
        ),
    ] {
        p.push(Action::flag(option, dest, text.to_owned()));
    }
    p.push(Action::option(
        "--opponent-list",
        "opponent_list",
        Converter::Str,
        Some(vec![
            OpponentList::Known.as_str(),
            OpponentList::Archetype.as_str(),
        ]),
        Value::None,
        "where the search samples the opponent's unseen cards from (W-134; default: the \
         preset's, known)"
            .to_owned(),
    ));
    p
}

/// `gcg-eval replay-game`'s parser (W-148).
#[must_use]
pub fn replay_game_parser() -> Parser {
    let mut p = Parser::new(
        &format!("{PROG} replay-game"),
        Some(
            "Re-simulate one game of a run's games.ndjson from its results.json, verify it and \
             print its log (W-148).",
        ),
    );
    for (option, dest, text) in [
        ("--games-ndjson", "games_ndjson", "the run's games.ndjson"),
        ("--results", "results", "the run's results.json"),
    ] {
        let mut a = Action::option(
            option,
            dest,
            Converter::Path,
            None,
            Value::None,
            text.to_owned(),
        );
        a.required = true;
        p.push(a);
    }
    let mut m = Action::option(
        "--match",
        "match",
        Converter::NonNegativeInt,
        None,
        Value::None,
        "match index (from 0)".to_owned(),
    );
    m.required = true;
    p.push(m);
    let mut g = Action::option(
        "--game",
        "game",
        Converter::PositiveInt,
        None,
        Value::None,
        "game number within the match (from 1)".to_owned(),
    );
    g.required = true;
    p.push(g);
    p
}

/// `gcg-eval replay-check`'s parser (W-156).
#[must_use]
pub fn replay_check_parser() -> Parser {
    let mut p = Parser::new(
        &format!("{PROG} replay-check"),
        Some(
            "gcg-sim replay with a version check: on a mismatch it says whether the run's data, \
             rules or code versions differ from this build's (W-156).",
        ),
    );
    p.push(Action::positional(
        "replay_json",
        Converter::Path,
        help("a replay file written by benchmark"),
    ));
    p.push(Action::option(
        "--results",
        "results",
        Converter::Path,
        None,
        Value::None,
        "the run's results.json (default: the one above replays/)".to_owned(),
    ));
    p.push(Action::flag(
        "--log",
        "log",
        "print the readable game log".to_owned(),
    ));
    p
}

fn get_py(ns: &Namespace, dest: &str) -> Option<PyStr> {
    match ns.get(dest) {
        Some(Value::Str(s) | Value::Path(s)) => Some(s.clone()),
        _ => None,
    }
}

fn need_py(ns: &Namespace, dest: &str) -> PyStr {
    get_py(ns, dest).unwrap_or_else(|| unreachable!("{dest} is a str or Path argument"))
}

fn path_of(s: &PyStr) -> PathBuf {
    PathBuf::from(s.to_os_string())
}

fn text_of(s: &PyStr) -> String {
    s.to_string_opt()
        .unwrap_or_else(|| String::from_utf8_lossy(&s.to_surrogateescape_bytes()).into_owned())
}

fn flag(ns: &Namespace, dest: &str) -> bool {
    matches!(ns.get(dest), Some(Value::Bool(true)))
}

fn u32_of(
    ns: &Namespace,
    dest: &str,
    option: &str,
    sub: &Parser,
    columns: i64,
) -> Result<u32, Exit> {
    let v = match ns.get(dest) {
        Some(Value::Int(v)) => v.clone(),
        _ => unreachable!("{dest} is an integer argument"),
    };
    v.to_u64()
        .and_then(|x| u32::try_from(x).ok())
        .ok_or_else(|| {
            let msg = format!(
                "argument {option}: must be at most {}, got {}",
                u32::MAX,
                v.to_decimal()
            );
            sub.error(&PyStr::from(msg.as_str()), columns)
        })
}

fn choice<T: Copy>(all: &[T], name: impl Fn(T) -> &'static str, value: &str) -> T {
    all.iter()
        .copied()
        .find(|x| name(*x) == value)
        .unwrap_or_else(|| unreachable!("{value} is one of its choices"))
}

/// The typed `benchmark` arguments.
///
/// # Errors
/// A count beyond `u32`, or `--swiss-draws` without `--analysis` (usage errors).
pub fn bench_args(ns: &Namespace, sub: &Parser, columns: i64) -> Result<EvalBenchArgs, Exit> {
    let workers = match ns.get("workers") {
        Some(Value::Auto) => Workers::Auto,
        Some(Value::Int(v)) => Workers::Count(
            v.to_u64()
                .map_or(u32::MAX, |x| u32::try_from(x).unwrap_or(u32::MAX)),
        ),
        _ => unreachable!("--workers is auto or an integer"),
    };
    let seed = match ns.get("seed") {
        Some(Value::Int(v)) => v.to_u64().unwrap_or(0),
        _ => 0,
    };
    let opt = OptIns {
        analysis: flag(ns, "analysis"),
        swiss_draws: flag(ns, "swiss_draws"),
        compress: flag(ns, "compress"),
        resume: flag(ns, "resume"),
        replays_extremes: flag(ns, "replays_extremes"),
        diagnostics: flag(ns, "diagnostics"),
        opponent_list: get_py(ns, "opponent_list")
            .and_then(|s| OpponentList::from_value(&text_of(&s))),
    };
    if opt.swiss_draws && !opt.analysis {
        return Err(sub.error(
            &PyStr::from("argument --swiss-draws: needs --analysis"),
            columns,
        ));
    }
    Ok(EvalBenchArgs {
        deck_under_test: need_py(ns, "deck_under_test"),
        benchmark_deck: need_py(ns, "benchmark_deck"),
        matches: u32_of(ns, "matches", "--matches", sub, columns)?,
        seed,
        workers,
        format: choice(
            &Format::ALL,
            Format::as_str,
            &text_of(&need_py(ns, "format")),
        ),
        preset: choice(
            &Preset::ALL,
            Preset::as_str,
            &text_of(&need_py(ns, "ai_preset")),
        ),
        out: need_py(ns, "out"),
        decision_log: flag(ns, "decision_log"),
        replays: u32_of(ns, "replays", "--replays", sub, columns)?,
        opt,
    })
}

/// The typed `replay-game` arguments.
///
/// # Errors
/// A count beyond `u32`.
pub fn replay_game_args(
    ns: &Namespace,
    sub: &Parser,
    columns: i64,
) -> Result<ReplayGameArgs, Exit> {
    Ok(ReplayGameArgs {
        games: path_of(&need_py(ns, "games_ndjson")),
        results: path_of(&need_py(ns, "results")),
        match_index: u32_of(ns, "match", "--match", sub, columns)?,
        game_number: u32_of(ns, "game", "--game", sub, columns)?,
    })
}

/// The typed `replay-check` arguments.
#[must_use]
pub fn replay_check_args(ns: &Namespace) -> ReplayCheckArgs {
    ReplayCheckArgs {
        replay: need_py(ns, "replay_json"),
        results: get_py(ns, "results").map(|s| path_of(&s)),
        log: flag(ns, "log"),
    }
}

// ---------------------------------------------------------------------------------------------
// benchmark

fn invalid(e: CliError) -> CommandError {
    match e {
        CliError::Invalid(m) => CommandError::Invalid(text_of(&m)),
        CliError::Interrupted => CommandError::Interrupted,
    }
}

/// `exc.strerror or exc`: the OS message without Rust's ` (os error N)` suffix.
fn strerror(e: &std::io::Error) -> String {
    let text = e.to_string();
    match e.raw_os_error() {
        Some(code) => text
            .strip_suffix(&format!(" (os error {code})"))
            .map_or_else(|| text.clone(), str::to_owned),
        None => text,
    }
}

/// `str(path / name)` of a normalised path text, as `gcg-sim` prints its report paths.
fn join(dir: &PyStr, name: &str) -> PyStr {
    let sep = if dir.code_points().last() == Some(&u32::from('/')) {
        ""
    } else {
        "/"
    };
    let mut joined = dir.clone();
    joined.push_py(&PyStr::from(sep));
    joined.push_py(&PyStr::from(name));
    path_str(&joined)
}

fn println(out: &mut dyn Write, line: &PyStr) {
    let mut bytes = line.to_surrogateescape_bytes();
    bytes.push(b'\n');
    let _ = out.write_all(&bytes);
}

/// A byte encoder of `--compress`.
pub type Encoder = fn(&[u8]) -> Vec<u8>;

/// The compressor of `--compress`: zstd (level 19) when the build has the `zstd` encoder,
/// otherwise the in-house gzip writer.
#[must_use]
pub fn compression() -> (&'static str, Encoder) {
    #[cfg(feature = "zstd")]
    {
        fn encode(bytes: &[u8]) -> Vec<u8> {
            zstd::encode_all(bytes, 19).expect("in-memory zstd encoding cannot fail")
        }
        ("games.ndjson.zst", encode)
    }
    #[cfg(not(feature = "zstd"))]
    {
        ("games.ndjson.gz", gcg_runner::reports::gzip::gzip)
    }
}

/// `gcg-eval benchmark`.
///
/// # Errors
/// As `gcg-sim benchmark`'s messages; a checkpoint of another configuration with `--resume`.
pub fn benchmark(
    args: &EvalBenchArgs,
    cancel: &Cancel,
    tty: bool,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<(), CommandError> {
    let reg = Registry::embedded();
    let dut = legal_deck(&args.deck_under_test, &*reg).map_err(invalid)?;
    let bench = legal_deck(&args.benchmark_deck, &*reg).map_err(invalid)?;
    writable_dir(&args.out).map_err(invalid)?;
    let cfg = BenchmarkConfig {
        deck_under_test: dut,
        benchmark_deck: bench,
        matches: args.matches,
        seed: args.seed,
        workers: args.workers,
        format: args.format,
        ai_preset: args.preset.as_str(),
        out_dir: path_of(&args.out),
        decision_log: args.decision_log,
        replays: args.replays,
        turn_limit: gcg_runner::config::DEFAULT_TURN_LIMIT,
        max_actions: MAX_ACTIONS_DEFAULT,
        schema: SIM_SCHEMA,
        package: PACKAGE_VERSION,
        code_digest: CODE_DIGEST,
    };
    let factory = ShippedFactory::new(args.preset, args.opt.opponent_list);
    let mut printer = ProgressPrinter::new(tty);
    let mut progress = |done: u32, total: u32| printer.print(done, total, err);
    let run = if args.opt.resume {
        run_resumable(
            &cfg,
            &factory,
            &mut progress,
            cancel,
            &checkpoint_path(&cfg.out_dir),
        )
        .map_err(|e| match e {
            ResumeError::Run(RunError::Interrupted) => CommandError::Interrupted,
            other => CommandError::Invalid(other.to_string()),
        })?
    } else {
        run_benchmark(&cfg, &factory, &mut progress, cancel).map_err(|e| match e {
            RunError::Interrupted => CommandError::Interrupted,
            RunError::GameFailure(m) => CommandError::Invalid(format!("simulation failed: {m}")),
        })?
    };
    let data = ResultsData::embedded(&cfg).map_err(|e| CommandError::Invalid(e.to_string()))?;
    let mut extra_files: Vec<(String, Vec<u8>)> = Vec::new();
    let mut extra_lines: Vec<(&str, &str)> = Vec::new();
    if args.opt.analysis || args.opt.diagnostics {
        let facts = gcg_runner::analysis::observe_run(&run)
            .map_err(|e| CommandError::Invalid(format!("cannot analyse the run: {e}")))?;
        if args.opt.analysis {
            let results = build_results_with(&run, &data);
            let opts = gcg_runner::analysis::AnalysisOptions {
                swiss_draws: args.opt.swiss_draws,
                ..Default::default()
            };
            let (doc, variants) =
                gcg_runner::analysis::analysis_json(&run, &data, &results, &facts, opts);
            extra_files.push(("analysis.json".to_owned(), to_json_text(&doc).into_bytes()));
            extra_files.extend(variants.into_iter().map(|(n, t)| (n, t.into_bytes())));
            extra_lines.push(("analysis", "analysis.json"));
        }
        if args.opt.diagnostics {
            let doc = gcg_runner::analysis::diagnostics_json(&run, &facts);
            extra_files.push((
                "diagnostics.json".to_owned(),
                to_json_text(&doc).into_bytes(),
            ));
            extra_lines.push(("diagnostics", "diagnostics.json"));
        }
    }
    let decisions: Vec<u64> = run
        .games()
        .map(|g| u64::try_from(g.actions.len()).unwrap_or(u64::MAX))
        .collect();
    let total: u64 = decisions.iter().sum();
    let timing_extra = vec![
        ("decisions".to_owned(), Json::Int(i128::from(total))),
        (
            "decisions_per_game".to_owned(),
            Json::Array(
                decisions
                    .iter()
                    .map(|&d| Json::Int(i128::from(d)))
                    .collect(),
            ),
        ),
    ];
    let (file, encode) = compression();
    let options = SetOptions {
        compress: args.opt.compress.then_some(Compression {
            file,
            encode: &encode,
        }),
        extremes: args.opt.replays_extremes,
        extra_files,
        timing_extra,
        after_file: None,
    };
    let mut replay_doc = |label: &'static str, record: &GameRecord| {
        gcg_runner::replay::replay_document(record, &cfg, label)
    };
    let paths = write_report_set(
        &run,
        &cfg.out_dir,
        cancel,
        &data,
        &mut replay_doc,
        &TimingHost::rust(),
        options,
    )
    .map_err(|e| match e {
        ReportError::Interrupted => CommandError::Interrupted,
        ReportError::Io { path, source } => CommandError::Invalid(format!(
            "cannot write reports to {}: {}: {}",
            text_of(&args.out),
            py_path_str(&path),
            strerror(&source)
        )),
        other => CommandError::Invalid(other.to_string()),
    })?;
    if cancel.is_cancelled() {
        return Err(CommandError::Interrupted);
    }
    let games_name = if args.opt.compress {
        file
    } else {
        "games.ndjson"
    };
    println(
        out,
        &PyStr::from(
            format!(
                "{} (deck under test) vs {} (benchmark)",
                cfg.deck_under_test.name, cfg.benchmark_deck.name
            )
            .as_str(),
        ),
    );
    println(
        out,
        &PyStr::from(rate_line("matches", &Outcomes::of_matches(&run.matches)).as_str()),
    );
    println(
        out,
        &PyStr::from(rate_line("games", &Outcomes::of_games(run.games())).as_str()),
    );
    println(out, &PyStr::from("reports:"));
    let mut lines = vec![
        ("results", "results.json"),
        ("summary", "summary.md"),
        ("games", games_name),
        ("timing", "timing.json"),
    ];
    lines.extend(extra_lines);
    for (label, name) in lines {
        let mut line = PyStr::from(format!("  {label:<8} ").as_str());
        line.push_py(&join(&args.out, name));
        println(out, &line);
    }
    let mut line = PyStr::from(format!("  replays  {} file(s) in ", paths.replays.len()).as_str());
    line.push_py(&join(&args.out, "replays"));
    println(out, &line);
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// compare-runs guard (W-91)

/// The W-91 guard of `gcg-eval compare-runs`: both report directories' `results.json` must
/// name the same AI identity and data and code versions, unless `allow_mixed` (then the
/// differences are printed on `err`). A directory without `results.json` (a hand-made set of
/// game records) has no recorded identity; a note says so and the comparison proceeds.
///
/// # Errors
/// The refusal naming every difference, or an unreadable `results.json`.
pub fn guard_runs(
    a: &Path,
    b: &Path,
    allow_mixed: bool,
    err: &mut dyn Write,
) -> Result<(), CommandError> {
    let read = |dir: &Path| -> Result<Option<JsonValue>, CommandError> {
        let path = dir.join("results.json");
        if !path.is_file() {
            return Ok(None);
        }
        let shown = py_path_str(&path);
        let text = std::fs::read_to_string(&path)
            .map_err(|e| CommandError::Invalid(format!("cannot read {shown}: {}", strerror(&e))))?;
        serde_json::from_str(&text)
            .map(Some)
            .map_err(|_| CommandError::Invalid(format!("{shown} is not JSON")))
    };
    let (ra, rb) = (read(a)?, read(b)?);
    let (Some(ra), Some(rb)) = (ra, rb) else {
        let _ = writeln!(
            err,
            "note: the AI identity is unknown (a report directory has no results.json)"
        );
        return Ok(());
    };
    let names = [py_path_str(a), py_path_str(b)];
    let notes = gcg_runner::reports::identity::check_comparable(
        &ra,
        &rb,
        [&names[0], &names[1]],
        allow_mixed,
    )
    .map_err(CommandError::Invalid)?;
    for note in notes {
        let _ = writeln!(err, "{note}");
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// replay-check and replay-game (W-156, W-148)

/// The run's `versions` (from `results.json`) against this build's, as diagnosis lines: why a
/// failed check failed, or a note on a passing one.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum VersionCheck {
    /// No `results.json` (the path tried, and why).
    Unknown(String),
    Match,
    /// `field: recorded X, current Y` per differing field.
    Differ(Vec<String>),
}

/// Compares the `versions` of the `results.json` at `path` with this build's.
#[must_use]
pub fn version_check(path: Option<&Path>) -> VersionCheck {
    let Some(path) = path else {
        return VersionCheck::Unknown(
            "no results.json beside the replay's replays/ directory; pass --results".to_owned(),
        );
    };
    let shown = py_path_str(path);
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => return VersionCheck::Unknown(format!("cannot read {shown}: {}", strerror(&e))),
    };
    let doc: JsonValue = match serde_json::from_str(&text) {
        Ok(d) => d,
        Err(_) => return VersionCheck::Unknown(format!("{shown} is not JSON")),
    };
    let Some(recorded) = doc.get("versions") else {
        return VersionCheck::Unknown(format!("{shown} has no versions"));
    };
    let current = gcg_data::versions(PACKAGE_VERSION, CODE_DIGEST);
    let diffs = version_differences(recorded, &current);
    if diffs.is_empty() {
        VersionCheck::Match
    } else {
        VersionCheck::Differ(
            diffs
                .iter()
                .map(|d| format!("{}: recorded {}, current {}", d.field, d.a, d.b))
                .collect(),
        )
    }
}

impl VersionCheck {
    /// The text after a failed check.
    #[must_use]
    pub fn on_failure(&self) -> String {
        match self {
            Self::Unknown(why) => format!("the versions are unknown ({why})"),
            Self::Match => "the run's data, rules and code versions match this build's, so this \
                            is a regression or nondeterminism"
                .to_owned(),
            Self::Differ(lines) => format!(
                "the run's versions differ from this build's, so an expected change is possible:\n  {}",
                lines.join("\n  ")
            ),
        }
    }

    /// The note after a passing check: none when the versions match.
    #[must_use]
    pub fn on_success(&self) -> Option<String> {
        match self {
            Self::Match => None,
            Self::Unknown(why) => Some(format!("note: the versions are unknown ({why})")),
            Self::Differ(lines) => Some(format!(
                "note: the replay matches, but the run's versions differ from this build's:\n  {}",
                lines.join("\n  ")
            )),
        }
    }
}

/// The `results.json` of a replay's run: `--results`, else the one beside its `replays/`
/// directory.
fn results_of_replay(replay: &Path, given: Option<&Path>) -> Option<PathBuf> {
    if let Some(p) = given {
        return Some(p.to_path_buf());
    }
    let dir = replay.parent()?;
    (dir.file_name()? == "replays")
        .then(|| dir.parent().map(|p| p.join("results.json")))
        .flatten()
        .filter(|p| p.is_file())
}

/// `gcg-eval replay-check FILE [--results R] [--log]` (W-156).
///
/// # Errors
/// `gcg-sim replay`'s errors, followed by the version diagnosis.
pub fn replay_check(args: &ReplayCheckArgs, out: &mut dyn Write) -> Result<(), CommandError> {
    let mut buf = Vec::new();
    let result = replay_command(&args.replay, args.log, &mut buf);
    let _ = out.write_all(&buf);
    let replay = path_of(&args.replay);
    let check = version_check(results_of_replay(&replay, args.results.as_deref()).as_deref());
    match result {
        Ok(()) => {
            if let Some(note) = check.on_success() {
                let _ = writeln!(out, "{note}");
            }
            Ok(())
        }
        Err(CliError::Interrupted) => Err(CommandError::Interrupted),
        Err(CliError::Invalid(message)) => Err(CommandError::Invalid(format!(
            "{}\n{}",
            text_of(&message),
            check.on_failure()
        ))),
    }
}

/// A deck of `results.json` (`decks.<role>`) rebuilt from its card counts; its digest must
/// equal the recorded one.
fn rebuilt_deck(results: &JsonValue, role: &str, shown: &str) -> Result<gcg_data::Deck, String> {
    let d = results
        .pointer(&format!("/decks/{role}"))
        .ok_or_else(|| format!("{shown} has no decks.{role}"))?;
    let name = d.get("name").and_then(JsonValue::as_str).unwrap_or(role);
    let mut text = String::new();
    for part in ["main", "resources"] {
        for c in d
            .get(part)
            .and_then(JsonValue::as_array)
            .map_or(&[][..], Vec::as_slice)
        {
            let (Some(number), Some(n)) = (
                c.get("card_number").and_then(JsonValue::as_str),
                c.get("count").and_then(JsonValue::as_u64),
            ) else {
                return Err(format!("{shown}: decks.{role}.{part} is malformed"));
            };
            text.push_str(&format!("{n} {number}\n"));
        }
    }
    let deck = gcg_data::parse_deck(&text, name, None)
        .map_err(|e| format!("{shown}: decks.{role} does not parse: {e}"))?;
    let recorded = d
        .get("digest")
        .and_then(JsonValue::as_str)
        .unwrap_or_default();
    if deck.digest != recorded {
        return Err(format!(
            "{shown}: decks.{role} rebuilds to digest {} instead of the recorded {recorded} (the \
             card data differs from the run's)",
            deck.digest
        ));
    }
    Ok(deck)
}

/// The configuration of a run from its `results.json`, enough to re-simulate its games.
///
/// # Errors
/// A missing or malformed field.
pub fn run_config(results_path: &Path) -> Result<BenchmarkConfig, String> {
    let shown = py_path_str(results_path);
    let text = std::fs::read_to_string(results_path)
        .map_err(|e| format!("cannot read {shown}: {}", strerror(&e)))?;
    let doc: JsonValue = serde_json::from_str(&text).map_err(|_| format!("{shown} is not JSON"))?;
    let num = |p: &str| doc.pointer(p).and_then(JsonValue::as_u64);
    let format = doc
        .pointer("/config/format")
        .and_then(JsonValue::as_str)
        .and_then(Format::from_value)
        .ok_or_else(|| format!("{shown} has no config.format"))?;
    let preset = doc
        .pointer("/ai/preset")
        .and_then(JsonValue::as_str)
        .and_then(|p| {
            gcg_runner::config::AI_PRESETS
                .iter()
                .find(|x| **x == p)
                .copied()
        })
        .unwrap_or("standard");
    Ok(BenchmarkConfig {
        deck_under_test: rebuilt_deck(&doc, "deck_under_test", &shown)?,
        benchmark_deck: rebuilt_deck(&doc, "benchmark", &shown)?,
        matches: num("/config/matches")
            .and_then(|m| u32::try_from(m).ok())
            .ok_or_else(|| format!("{shown} has no config.matches"))?,
        seed: num("/seeds/master").ok_or_else(|| format!("{shown} has no seeds.master"))?,
        workers: Workers::Count(1),
        format,
        ai_preset: preset,
        out_dir: results_path
            .parent()
            .map_or_else(PathBuf::new, Path::to_path_buf),
        decision_log: false,
        replays: 0,
        turn_limit: doc
            .pointer("/config/turn_limit")
            .and_then(JsonValue::as_i64)
            .and_then(|t| i32::try_from(t).ok())
            .unwrap_or(gcg_engine::DEFAULT_TURN_LIMIT),
        max_actions: doc
            .pointer("/config/max_actions")
            .and_then(JsonValue::as_i64)
            .unwrap_or(MAX_ACTIONS_DEFAULT),
        schema: SchemaVersion::V1_1,
        package: PACKAGE_VERSION,
        code_digest: CODE_DIGEST,
    })
}

/// The record of game `game_number` (from 1) of match `match_index` in a `games.ndjson`.
///
/// # Errors
/// The file is unreadable, a line is malformed, or no line holds that game.
pub fn find_record(games: &Path, match_index: u32, game_number: u32) -> Result<GameRecord, String> {
    let shown = py_path_str(games);
    let text = std::fs::read_to_string(games)
        .map_err(|e| format!("cannot read {shown}: {}", strerror(&e)))?;
    for (i, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let v: JsonValue =
            serde_json::from_str(line).map_err(|e| format!("{shown}:{}: not JSON: {e}", i + 1))?;
        let m = v.get("match_index").and_then(JsonValue::as_u64);
        let g = v.get("game_index").and_then(JsonValue::as_u64);
        if m == Some(u64::from(match_index)) && g == Some(u64::from(game_number) - 1) {
            return GameRecord::from_json(&v)
                .map_err(|e| format!("{shown}:{}: not a game record: {e:?}", i + 1));
        }
    }
    Err(format!(
        "{shown} has no game {game_number} of match {match_index}"
    ))
}

/// `gcg-eval replay-game` (W-148): re-simulates the game from the run's configuration,
/// checks the recorded winner, end reason, turns and final state digest, and prints the log,
/// the seats, the result and the verdict as `gcg-sim replay --log` does.
///
/// # Errors
/// An unreadable or malformed input, or a mismatch (with the version diagnosis of W-156).
pub fn replay_game(args: &ReplayGameArgs, out: &mut dyn Write) -> Result<(), CommandError> {
    let cfg = run_config(&args.results).map_err(CommandError::Invalid)?;
    let record = find_record(&args.games, args.match_index, args.game_number)
        .map_err(CommandError::Invalid)?;
    let check = version_check(Some(&args.results));
    let fail =
        |message: String| CommandError::Invalid(format!("{message}\n{}", check.on_failure()));
    let label = format!("game {} of match {}", args.game_number, args.match_index);
    let sim = gcg_runner::replay::resimulate(&record, &cfg)
        .map_err(|e| fail(format!("cannot replay {label}: {e}")))?;
    let st = gcg_runner::replay(&record, &cfg)
        .map_err(|e| fail(format!("cannot replay {label}: {e}")))?;
    let _ = writeln!(out, "{}", sim.log.join("\n"));
    let decks = [&cfg.deck_under_test.name, &cfg.benchmark_deck.name];
    for seat in [0_u8, 1] {
        let dut = seat == record.dut_seat;
        let _ = writeln!(
            out,
            "seat {seat}: {} {} ({})",
            if dut { "dut" } else { "bench" },
            decks[usize::from(!dut)],
            record.agents[usize::from(seat)]
        );
    }
    let winner = st.winner();
    let reason = st.end_reason();
    let shown_winner = winner.map_or_else(|| "None".to_owned(), |w| w.to_string());
    let _ = writeln!(
        out,
        "result: winner seat {shown_winner} ({}) after {} turns",
        reason.map_or("None", EndReason::as_str),
        st.turn()
    );
    let expected_winner: i8 = match record.winner {
        Outcome::Dut => i8::try_from(record.dut_seat).unwrap_or(-1),
        Outcome::Bench => i8::try_from(record.bench_seat()).unwrap_or(-1),
        Outcome::Draw => -1,
    };
    let digest = canon::state_digest(&st);
    let mut mismatches = Vec::new();
    if winner != Some(expected_winner) {
        mismatches.push(format!(
            "winner_seat: expected {expected_winner}, replay gave {shown_winner}"
        ));
    }
    if reason != Some(record.end_reason) {
        mismatches.push(format!(
            "end_reason: expected {}, replay gave {}",
            record.end_reason.as_str(),
            reason.map_or("None", EndReason::as_str)
        ));
    }
    if st.turn() != record.turns {
        mismatches.push(format!(
            "turns: expected {}, replay gave {}",
            record.turns,
            st.turn()
        ));
    }
    if digest != record.final_state_sha256 {
        mismatches.push(format!(
            "final_state_sha256: expected {}, replay gave {digest}",
            record.final_state_sha256
        ));
    }
    if !mismatches.is_empty() {
        return Err(fail(format!(
            "replay does NOT match the recorded outcome:\n  {}",
            mismatches.join("\n  ")
        )));
    }
    let _ = writeln!(out, "replay matches the recorded outcome and final state");
    if let Some(note) = check.on_success() {
        let _ = writeln!(out, "{note}");
    }
    Ok(())
}
