//! The `gcg-sim benchmark` pipeline as `gcg-eval reference-benchmark` and `preset-benchmark`
//! run it: `cmd_benchmark` (`cli.py:188-229`) with `_read_deck`/`_legal_deck`
//! (`:102-126`), `_writable_dir` (`:176-185`), `ProgressPrinter` (`:143-159`) and `_rate_line`
//! (`:167-173`), over gcg-runner's `run_benchmark` and report writer.
//!
//! The factory, the preset label and the results schema are the caller's:
//! `reference-benchmark` writes schema 1.1.0 byte-identical to Python with Python's preset
//! names (PLAN A6, A10). These helpers are the parts of `cli_support` (docs/dev/contracts.md
//! O11) that `gcg-eval` needs, written against the frozen gcg-runner and gcg-data API.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use gcg_core::pool::Cancel;
use gcg_core::pyfloat::format_fixed;
use gcg_data::{Deck, ValidateOptions, py_path_str, validate};
use gcg_effects::Registry;
use gcg_engine::{DEFAULT_TURN_LIMIT, MAX_ACTIONS_DEFAULT};
use gcg_runner::reports::aggregate::Outcomes;
use gcg_runner::reports::{ResultsData, TimingHost, write_reports_with};
use gcg_runner::{
    AgentFactory, BenchmarkConfig, Format, ReportError, RunError, SchemaVersion, Workers,
    run_benchmark,
};

use crate::build_info::{CODE_DIGEST, PACKAGE_VERSION};
use crate::decks::read_deck;
use crate::selfplay::PlayError;

/// How a command failed (`main`, `cli.py:394-399`): `error: {message}` and exit 1, or the
/// interrupt (exit 130).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum CommandError {
    Invalid(String),
    Interrupted,
}

/// A failed self-play run: `simulation failed: {game}` (as `cli.py:208-209` words a failed
/// benchmark), or the interrupt.
impl From<PlayError> for CommandError {
    fn from(e: PlayError) -> Self {
        match e {
            PlayError::Interrupted => Self::Interrupted,
            PlayError::Game(f) => Self::Invalid(format!("simulation failed: {f}")),
        }
    }
}

/// The parsed `benchmark` arguments (`cli.py:73-85`), paths as `str(Path(text))`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct BenchArgs {
    pub deck_under_test: String,
    pub benchmark_deck: String,
    pub matches: u32,
    pub seed: u64,
    pub workers: Workers,
    pub format: Format,
    /// `ai_preset` (`config.py:26`): the `results.json` `ai.preset` and the seats' preset.
    pub preset: &'static str,
    pub out: String,
    pub decision_log: bool,
    pub replays: u32,
    /// `--decision-trace FILE` (W-21): the per-decision trace of `crate::trace`, written
    /// besides the reports, which stay byte-identical.
    pub decision_trace: Option<PathBuf>,
}

/// `_legal_deck(path)` (`cli.py:119-126`): the deck, or every violation of
/// `validate_deck(deck)` (B&R and the implementation check included).
///
/// # Errors
/// The read error or `{path}: {n} violation(s)` with one `  [{code}] {message}` line each.
pub fn legal_deck(path: &str) -> Result<Deck, String> {
    let deck = read_deck(Path::new(path))?;
    let registry = Registry::embedded();
    let violations = validate(
        &deck,
        ValidateOptions {
            banlist: true,
            implemented: Some(&*registry),
        },
    );
    if violations.is_empty() {
        return Ok(deck);
    }
    let mut lines = vec![format!("{path}: {} violation(s)", violations.len())];
    lines.extend(
        violations
            .iter()
            .map(|v| format!("  [{}] {}", v.code.as_str(), v.message)),
    );
    Err(lines.join("\n"))
}

/// `os.access(path, os.W_OK | os.X_OK)` through `test -w` and `test -x` (safe Rust has no
/// `access(2)`); they make the same check.
fn writable_and_searchable(path: &Path) -> bool {
    let check = |flag: &str| {
        Command::new("/bin/test")
            .arg(flag)
            .arg(path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    };
    check("-w") && check("-x")
}

/// `_writable_dir(path)` (`cli.py:176-185`): the nearest existing ancestor must be a writable
/// directory; nothing is created.
///
/// # Errors
/// `cannot write reports to {path}: {existing} is not a directory` or `...: permission denied
/// on {existing}`.
pub fn writable_dir(path: &str) -> Result<(), String> {
    let mut existing = PathBuf::from(path);
    while !existing.exists() {
        match parent(&existing) {
            Some(p) => existing = p,
            None => break,
        }
    }
    let shown = py_path_str(&existing);
    if !existing.is_dir() {
        return Err(format!(
            "cannot write reports to {path}: {shown} is not a directory"
        ));
    }
    if !writable_and_searchable(&existing) {
        return Err(format!(
            "cannot write reports to {path}: permission denied on {shown}"
        ));
    }
    Ok(())
}

/// `Path.parent`, which is `.` for a relative name and `None` (the loop's
/// `existing != existing.parent`) at `.` and `/`.
fn parent(p: &Path) -> Option<PathBuf> {
    let text = py_path_str(p);
    if text == "." || text == "/" {
        return None;
    }
    match p.parent() {
        Some(q) if q.as_os_str().is_empty() => Some(PathBuf::from(".")),
        Some(q) => Some(q.to_path_buf()),
        None => None,
    }
}

/// `ProgressPrinter` (`cli.py:143-159`): in place on a terminal, else about every 10%.
pub struct ProgressPrinter<'a> {
    stream: &'a mut dyn Write,
    tty: bool,
    label: &'static str,
    last_decile: Option<u64>,
}

impl<'a> ProgressPrinter<'a> {
    #[must_use]
    pub fn new(stream: &'a mut dyn Write, tty: bool, label: &'static str) -> Self {
        Self {
            stream,
            tty,
            label,
            last_decile: None,
        }
    }

    /// `__call__(done, total)`.
    pub fn update(&mut self, done: u64, total: u64) {
        if self.tty {
            let end = if done == total { "\n" } else { "" };
            let _ = write!(self.stream, "\r{} {done}/{total}{end}", self.label);
            let _ = self.stream.flush();
            return;
        }
        let decile = (10 * done).checked_div(total).unwrap_or(0);
        if self.last_decile != Some(decile) {
            self.last_decile = Some(decile);
            let _ = writeln!(self.stream, "{} {done}/{total}", self.label);
            let _ = self.stream.flush();
        }
    }
}

/// `_rate_line(label, block)` (`cli.py:167-173`).
#[must_use]
pub fn rate_line(label: &str, block: &Outcomes) -> String {
    let r = &block.dut_win_rate;
    let rate = r.rate.unwrap_or(0.0);
    let (lo, hi) = r.ci95.unwrap_or((0.0, 1.0));
    format!(
        "{label}: DUT {}-{} (draws {}), win rate {}% [95% CI {}%-{}%] over {}",
        block.dut_wins,
        block.bench_wins,
        block.draws,
        format_fixed(100.0 * rate, 1),
        format_fixed(100.0 * lo, 1),
        format_fixed(100.0 * hi, 1),
        block.n
    )
}

/// `exc.strerror or exc`: the OS message without Rust's ` (os error N)` suffix.
fn strerror(e: &std::io::Error) -> String {
    let text = e.to_string();
    match text.rfind(" (os error ") {
        Some(i) if text.ends_with(')') => text[..i].to_owned(),
        _ => text,
    }
}

/// The `BenchmarkConfig` of a command (`cli.py:192-203`) with the binary's schema and version
/// strings and Python's limits (`config.py:31-32`).
#[must_use]
pub fn config(args: &BenchArgs, dut: Deck, bench: Deck, schema: SchemaVersion) -> BenchmarkConfig {
    BenchmarkConfig {
        deck_under_test: dut,
        benchmark_deck: bench,
        matches: args.matches,
        seed: args.seed,
        workers: args.workers,
        format: args.format,
        ai_preset: args.preset,
        out_dir: PathBuf::from(&args.out),
        decision_log: args.decision_log,
        replays: args.replays,
        turn_limit: DEFAULT_TURN_LIMIT,
        max_actions: MAX_ACTIONS_DEFAULT,
        schema,
        package: PACKAGE_VERSION,
        code_digest: CODE_DIGEST,
    }
}

/// `cmd_benchmark(args, out, err)` (`cli.py:188-229`) with `factory` building the agents.
///
/// # Errors
/// As `main` reports them.
pub fn benchmark(
    args: &BenchArgs,
    schema: SchemaVersion,
    factory: &dyn AgentFactory,
    cancel: &Cancel,
    tty: bool,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<(), CommandError> {
    let dut = legal_deck(&args.deck_under_test).map_err(CommandError::Invalid)?;
    let bench = legal_deck(&args.benchmark_deck).map_err(CommandError::Invalid)?;
    writable_dir(&args.out).map_err(CommandError::Invalid)?;
    let (dut_name, bench_name) = (dut.name.clone(), bench.name.clone());
    let cfg = config(args, dut, bench, schema);
    let sink = std::sync::Arc::new(crate::trace::Sink::default());
    let traced = crate::trace::TracingFactory {
        inner: factory,
        sink: std::sync::Arc::clone(&sink),
    };
    let factory: &dyn AgentFactory = if args.decision_trace.is_some() {
        &traced
    } else {
        factory
    };
    let run = {
        let mut printer = ProgressPrinter::new(err, tty, "matches");
        let mut progress =
            |done: u32, total: u32| printer.update(u64::from(done), u64::from(total));
        run_benchmark(&cfg, factory, &mut progress, cancel).map_err(|e| match e {
            RunError::GameFailure(message) => {
                CommandError::Invalid(format!("simulation failed: {message}"))
            }
            RunError::Interrupted => CommandError::Interrupted,
        })?
    };
    if let Some(path) = &args.decision_trace {
        let games: Vec<[u64; 2]> = run.games().map(|g| g.agent_seeds).collect();
        let lines = sink.take(|seed| {
            games.iter().enumerate().find_map(|(i, s)| {
                s.iter()
                    .position(|x| *x == seed)
                    .map(|seat| (u64::try_from(i).unwrap_or(u64::MAX), u8::from(seat == 1)))
            })
        });
        std::fs::write(path, crate::trace::ndjson(lines)).map_err(|e| {
            CommandError::Invalid(format!(
                "cannot write {}: {}",
                py_path_str(path),
                strerror(&e)
            ))
        })?;
    }
    let data = ResultsData::embedded(&cfg).map_err(|e| CommandError::Invalid(e.to_string()))?;
    let mut replay_doc = |label: &'static str, record: &gcg_runner::GameRecord| {
        gcg_runner::replay::replay_document(record, &cfg, label)
    };
    let paths = write_reports_with(
        &run,
        &cfg.out_dir,
        cancel,
        &data,
        &mut replay_doc,
        &TimingHost::rust(),
    )
    .map_err(|e| match e {
        ReportError::Io { path, source } => CommandError::Invalid(format!(
            "cannot write reports to {}: {}: {}",
            args.out,
            py_path_str(&path),
            strerror(&source)
        )),
        ReportError::Interrupted => CommandError::Interrupted,
        other => CommandError::Invalid(other.to_string()),
    })?;
    let mut lines = vec![
        format!("{dut_name} (deck under test) vs {bench_name} (benchmark)"),
        rate_line("matches", &Outcomes::of_matches(&run.matches)),
        rate_line("games", &Outcomes::of_games(run.games())),
        "reports:".to_owned(),
    ];
    for (label, path) in [
        ("results", &paths.results),
        ("summary", &paths.summary),
        ("games", &paths.games),
        ("timing", &paths.timing),
    ] {
        lines.push(format!("  {label:8} {}", py_path_str(path)));
    }
    lines.push(format!(
        "  replays  {} file(s) in {}",
        paths.replays.len(),
        py_path_str(&paths.out_dir.join("replays"))
    ));
    let mut text = lines.join("\n");
    text.push('\n');
    let _ = out.write_all(text.as_bytes());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_prints_deciles_off_a_terminal() {
        let mut buf = Vec::new();
        {
            let mut p = ProgressPrinter::new(&mut buf, false, "matches");
            for done in 1..=20 {
                p.update(done, 20);
            }
        }
        let text = String::from_utf8(buf).unwrap_or_default();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.first(), Some(&"matches 1/20"));
        assert_eq!(lines.len(), 11);
        assert_eq!(lines.last(), Some(&"matches 20/20"));
        let mut tty = Vec::new();
        {
            let mut p = ProgressPrinter::new(&mut tty, true, "games");
            p.update(1, 2);
            p.update(2, 2);
        }
        assert_eq!(tty, b"\rgames 1/2\rgames 2/2\n");
    }

    #[test]
    fn rate_lines_and_parents() {
        let block = Outcomes::of([
            gcg_runner::Outcome::Dut,
            gcg_runner::Outcome::Bench,
            gcg_runner::Outcome::Draw,
            gcg_runner::Outcome::Dut,
        ]);
        assert_eq!(
            rate_line("games", &block),
            "games: DUT 2-1 (draws 1), win rate 50.0% [95% CI 15.0%-85.0%] over 4"
        );
        assert_eq!(parent(Path::new("out")), Some(PathBuf::from(".")));
        assert_eq!(parent(Path::new("a/b")), Some(PathBuf::from("a")));
        assert_eq!(parent(Path::new(".")), None);
        assert_eq!(parent(Path::new("/")), None);
        assert_eq!(
            strerror(&std::io::Error::from_raw_os_error(13)),
            "Permission denied"
        );
    }
}
