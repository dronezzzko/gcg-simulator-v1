//! Writing the report files (`reports/writer.py`). Each file is written to a temporary name and
//! atomically renamed, and `results.json` is written last, so an interrupted write never leaves
//! a corrupt file.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use gcg_core::pool::Cancel;
use gcg_core::pyfloat::round_f64;
use gcg_core::pyjson::{DumpOptions, Json, dumps};
use gcg_core::pymath::py_sum;
use gcg_core::sys::topology::{Topology, TopologySource};

use super::pyregex::Regex;
use super::replay_doc::{Resimulation, replay_document_with};
use super::results::{ResultsData, build_results_with, replay_name, select_replays};
use super::schema::validate_results;
use super::summary::render_summary;
use crate::{BenchmarkRun, GameRecord, ReplayError, ReportError, ReportPaths};

/// `REPLAY_FILE` (`reports/writer.py:20`): Python's `\d` is any Unicode decimal digit and `$`
/// also matches before a final newline.
pub const REPLAY_FILE: &str = r"^(win|loss|draw)-m\d{4,}-g[1-3]\.json$";

/// The `note` of `timing.json` (`reports/writer.py:64`).
pub const TIMING_NOTE: &str = "wall-clock measurements; the only non-reproducible report file";

/// `to_json_text(obj)` (`reports/writer.py:33-34`): `json.dumps(obj, indent=2, sort_keys=True,
/// ensure_ascii=False) + "\n"` (results.json, replays, timing.json).
#[must_use]
pub fn to_json_text(doc: &Json) -> String {
    let mut text = dumps(doc, DumpOptions::PRETTY_SORTED_UTF8);
    text.push('\n');
    text
}

/// `_games_text(run)` (`reports/writer.py:54-57`): one `json.dumps(g.to_json(),
/// sort_keys=True, separators=(",", ":"))` line per game (ASCII-escaped), in run order, each
/// ending with `"\n"`.
#[must_use]
pub fn games_ndjson(run: &BenchmarkRun) -> String {
    let mut out = String::new();
    for g in run.games() {
        out.push_str(&dumps(&g.to_json(), DumpOptions::COMPACT_SORTED_ASCII));
        out.push('\n');
    }
    out
}

/// `shlex.quote(s)` (Python 3.15): `''` for the empty string, `s` itself when it is ASCII made
/// only of `%+,-./0-9:=@A-Z_a-z`, otherwise `s` in single quotes with each `'` written as
/// `'"'"'`.
#[must_use]
pub fn shlex_quote(s: &str) -> String {
    if s.is_empty() {
        return "''".to_owned();
    }
    let safe = |c: char| c.is_ascii_alphanumeric() || "%+,-./:=@_".contains(c);
    if s.chars().all(safe) {
        return s.to_owned();
    }
    format!("'{}'", s.replace('\'', "'\"'\"'"))
}

/// The `ai.factory` of a run whose agents a release `gcg-sim --ai-preset {preset}` builds:
/// `gcg_search::Preset::factory_description` (`gcg-search::make_agent('<preset>')`).
#[must_use]
pub fn release_factory(preset: &str) -> String {
    format!("gcg-search::make_agent({})", gcg_data::py_repr(preset))
}

/// The directories under `decks/` of the deck copies of W-100: the deck under test, then the
/// benchmark deck.
pub const DECK_DIRS: [&str; 2] = ["deck_under_test", "benchmark"];

/// The report-directory file that holds a deck without a source file in a 1.2.0 run
/// (W-100): `decks/deck_under_test/{name}.txt` or `decks/benchmark/{name}.txt`, so that
/// `load_deck` gives the deck its name back (the path's stem). `None` when the name cannot be
/// a file stem (empty, `.`, `..`, or holding `/` or NUL).
#[must_use]
pub fn deck_file_name(role: crate::Role, name: &str) -> Option<String> {
    let usable = !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\0']);
    let dir = DECK_DIRS[usize::from(role == crate::Role::Bench)];
    usable.then(|| format!("decks/{dir}/{name}.txt"))
}

/// Whether a deck has a source file the reproduce line can name.
#[must_use]
pub fn has_source(d: &gcg_data::Deck) -> bool {
    d.source.as_deref().is_some_and(|s| !s.is_empty())
}

/// The deck files a 1.2.0 run writes beside its reports (W-100): `(file name, text)` of every
/// deck without a source; none for a 1.1.0 run or an injected-agent run (whose reproduce line
/// names no command).
#[must_use]
pub fn written_decks(run: &BenchmarkRun) -> Vec<(String, String)> {
    let c = &run.config;
    if c.schema != crate::SchemaVersion::V1_2 || run.agent_factory != release_factory(c.ai_preset) {
        return Vec::new();
    }
    [
        (crate::Role::Dut, &c.deck_under_test),
        (crate::Role::Bench, &c.benchmark_deck),
    ]
    .into_iter()
    .filter(|(_, d)| !has_source(d))
    .filter_map(|(role, d)| deck_file_name(role, &d.name).map(|n| (n, deck_file_text(d))))
    .collect()
}

/// The deck file text written for a deck without a source (W-100): one `count card_number`
/// line per card, main deck then resource deck, which `parse_deck` reads back to the same
/// card counts and digest.
#[must_use]
pub fn deck_file_text(d: &gcg_data::Deck) -> String {
    let mut text = format!("# {} (written by the report writer; W-100)\n", d.name);
    for cc in d.main.iter().chain(d.resources.iter()) {
        text.push_str(&format!("{} {}\n", cc.count, cc.card_number));
    }
    text
}

/// `reproduce_command(run)` (`reports/writer.py:44-51`): `shlex.join` of the `gcg-sim
/// benchmark` command line (deck sources, or names without a source; never `--workers` or
/// `--out`; the seed in decimal). Results 1.2.0 runs (`gcg-sim`, A23) write the corrected line
/// of W-100 instead: a deck without a source names its copy in the report directory
/// ([`deck_file_name`]), and a run whose agents no release preset builds says so and names the
/// injected factory instead of printing a command that would play the default AI.
#[must_use]
pub fn reproduce_command(run: &BenchmarkRun) -> String {
    let c = &run.config;
    if c.schema == crate::SchemaVersion::V1_2 {
        return corrected_reproduce_command(run);
    }
    let deck = |_: crate::Role, d: &gcg_data::Deck| match d.source.as_deref() {
        Some(s) if !s.is_empty() => s.to_owned(),
        _ => d.name.to_string(),
    };
    python_command(run, &deck)
}

/// The W-100 reproduce line (A23).
fn corrected_reproduce_command(run: &BenchmarkRun) -> String {
    let c = &run.config;
    if run.agent_factory != release_factory(c.ai_preset) {
        return format!(
            "not reproducible with the release CLI: the agents came from the injected factory \
             {} (results.json ai.factory), which no --ai-preset builds",
            gcg_data::py_repr(&run.agent_factory)
        );
    }
    for (role, d) in [
        (crate::Role::Dut, &c.deck_under_test),
        (crate::Role::Bench, &c.benchmark_deck),
    ] {
        if !has_source(d) && deck_file_name(role, &d.name).is_none() {
            return format!(
                "not reproducible with the release CLI: the deck {} has no source file and its \
                 name cannot be a file name",
                gcg_data::py_repr(&d.name)
            );
        }
    }
    let out = c.out_dir.to_string_lossy().into_owned();
    let deck = |role: crate::Role, d: &gcg_data::Deck| -> String {
        if has_source(d) {
            return d.source.as_deref().unwrap_or_default().to_owned();
        }
        let name = deck_file_name(role, &d.name).unwrap_or_default();
        if out.is_empty() {
            name
        } else {
            format!("{}/{name}", out.trim_end_matches('/'))
        }
    };
    python_command(run, &deck)
}

/// The `gcg-sim benchmark` line of `reports/writer.py:44-51` with each deck named by `deck`.
fn python_command(
    run: &BenchmarkRun,
    deck: &dyn Fn(crate::Role, &gcg_data::Deck) -> String,
) -> String {
    let c = &run.config;
    let mut args = vec![
        "gcg-sim".to_owned(),
        "benchmark".to_owned(),
        deck(crate::Role::Dut, &c.deck_under_test),
        deck(crate::Role::Bench, &c.benchmark_deck),
        "--matches".to_owned(),
        c.matches.to_string(),
        "--seed".to_owned(),
        c.seed.to_string(),
        "--format".to_owned(),
        c.format.as_str().to_owned(),
        "--ai-preset".to_owned(),
        c.ai_preset.to_owned(),
        "--replays".to_owned(),
        c.replays.to_string(),
    ];
    if c.decision_log {
        args.push("--decision-log".to_owned());
    }
    args.iter()
        .map(|a| shlex_quote(a))
        .collect::<Vec<_>>()
        .join(" ")
}

/// The two host strings of `timing.json`: Python writes `platform.python_version()` and
/// `platform.platform()` (`reports/writer.py:74-75`).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct TimingHost {
    pub python: String,
    pub platform: String,
}

impl TimingHost {
    /// The Rust build's values: `"none (Rust build)"` (PLAN A17) and the target's
    /// `{os}-{arch}` (`macos-aarch64` on Apple silicon).
    #[must_use]
    pub fn rust() -> Self {
        Self {
            python: "none (Rust build)".to_owned(),
            platform: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
        }
    }
}

fn topology_json(t: &Topology) -> Json {
    let mut o = Json::object();
    o.set(
        "levels",
        Json::Array(
            t.levels
                .iter()
                .map(|l| {
                    let mut e = Json::object();
                    e.set("name", l.name.as_str());
                    e.set("logical_cpus", l.logical_cpus);
                    e.set("physical_cpus", l.physical_cpus);
                    e
                })
                .collect(),
        ),
    );
    o.set("logical_cpus", t.logical_cpus);
    o.set("cache_line_bytes", t.cache_line_bytes);
    o.set(
        "source",
        match t.source {
            TopologySource::Sysctl => "sysctl",
            TopologySource::Fallback => "fallback",
        },
    );
    o
}

/// `_timing(run)` (`reports/writer.py:60-76`), Python's keys and values, plus `topology`, the
/// CPU tiers of the machine (PLAN §7.4, `RunTiming::topology`). The only report file that is
/// not reproducible; nothing reads it back (PLAN A17, §11.8).
#[must_use]
pub fn timing_json(run: &BenchmarkRun, host: &TimingHost) -> Json {
    let t = &run.timing;
    let seconds = &t.match_seconds;
    let games = run.games().count();
    #[expect(
        clippy::cast_precision_loss,
        reason = "Python's float / int and int / float convert the int the same way"
    )]
    let (mean, per_second) = (
        (!seconds.is_empty())
            .then(|| round_f64(py_sum(seconds.iter().copied()) / seconds.len() as f64, 3)),
        (t.wall_seconds != 0.0).then(|| round_f64(games as f64 / t.wall_seconds, 3)),
    );
    let mut o = Json::object();
    o.set("note", TIMING_NOTE);
    o.set("started_at", t.started_at.as_str());
    o.set("finished_at", t.finished_at.as_str());
    o.set("wall_seconds", round_f64(t.wall_seconds, 3));
    o.set("workers", t.workers);
    o.set("matches", run.matches.len());
    o.set("games", games);
    o.set(
        "match_seconds",
        seconds
            .iter()
            .map(|&s| round_f64(s, 3))
            .collect::<Vec<f64>>(),
    );
    o.set("mean_match_seconds", mean);
    o.set("games_per_second", per_second);
    o.set("python", host.python.as_str());
    o.set("platform", host.platform.as_str());
    o.set("topology", topology_json(&t.topology));
    o
}

/// `Io` names the failing path and the OS error (the CLI builds Python's `cannot write
/// reports to ...` line from its parts, `cli.py:210-216`). `Schema` and `Replay` have no
/// Python message: Python lets `jsonschema.ValidationError` and `ReplayError` escape as a
/// traceback.
impl fmt::Display for ReportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => write!(f, "{}: {source}", path.display()),
            Self::Schema(e) => write!(f, "results.json does not match its schema: {e}"),
            Self::Replay(e) => write!(f, "cannot build a replay document: {e}"),
            Self::Interrupted => f.write_str(crate::INTERRUPTED_MESSAGE),
        }
    }
}

impl std::error::Error for ReportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Schema(e) => Some(e),
            Self::Replay(e) => Some(e),
            Self::Interrupted => None,
        }
    }
}

fn io_error(path: &Path) -> impl FnOnce(std::io::Error) -> ReportError + '_ {
    move |source| ReportError::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// `write_atomic(path, text)` (`reports/writer.py:37-41`): `mkdir -p` the parent, write the
/// UTF-8 bytes to `.{name}.{pid}.tmp` beside it, then rename over `path`. The error names the
/// path of the failing call, as Python's `OSError.filename` does.
///
/// # Errors
/// [`ReportError::Io`] from creating the directory, writing or renaming.
pub fn write_atomic(path: &Path, text: &str) -> Result<(), ReportError> {
    let parent = path.parent().unwrap_or_else(|| Path::new(""));
    if !parent.as_os_str().is_empty() {
        fs::create_dir_all(parent).map_err(io_error(parent))?;
    }
    let name = path
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let tmp = path.with_file_name(format!(".{name}.{}.tmp", std::process::id()));
    fs::write(&tmp, text.as_bytes()).map_err(io_error(&tmp))?;
    fs::rename(&tmp, path).map_err(io_error(&tmp))
}

/// The stale-file matcher of `_write_replays` (`reports/writer.py:20`, `:85-90`).
fn replay_file_pattern() -> Regex {
    Regex::new(REPLAY_FILE).expect("REPLAY_FILE is in the supported subset (pyregex tests)")
}

/// `REPLAY_FILE.match(name)` (`reports/writer.py:89`).
#[must_use]
pub fn is_replay_file_name(name: &str) -> bool {
    replay_file_pattern().is_match_at_start(name)
}

fn checkpoint(cancel: &Cancel) -> Result<(), ReportError> {
    if cancel.is_cancelled() {
        Err(ReportError::Interrupted)
    } else {
        Ok(())
    }
}

fn write_checked(path: &Path, text: &str, cancel: &Cancel) -> Result<(), ReportError> {
    checkpoint(cancel)?;
    write_atomic(path, text)?;
    checkpoint(cancel)
}

/// `_write_replays(run, out_dir)` (`reports/writer.py:79-91`): writes each selected replay
/// (its document built just before its write), then deletes every file in `out/replays/` that
/// matches [`REPLAY_FILE`] and was not written now. Without a `replays/` directory nothing is
/// deleted. A document that fails to build stops the writes there, as Python's escaping
/// `ReplayError` does (no cleanup).
fn write_replays(
    run: &BenchmarkRun,
    out: &Path,
    cancel: &Cancel,
    replay_doc: &mut dyn FnMut(&'static str, &GameRecord) -> Result<Json, ReplayError>,
) -> Result<Vec<PathBuf>, ReportError> {
    let mut written = Vec::new();
    for (label, record) in select_replays(run) {
        let path = out.join(replay_name(label, record));
        let doc = replay_doc(label, record).map_err(ReportError::Replay)?;
        write_checked(&path, &to_json_text(&doc), cancel)?;
        written.push(path);
    }
    let replay_dir = out.join("replays");
    if replay_dir.is_dir() {
        let pattern = replay_file_pattern();
        let keep: Vec<&std::ffi::OsStr> = written.iter().filter_map(|p| p.file_name()).collect();
        let entries = fs::read_dir(&replay_dir).map_err(io_error(&replay_dir))?;
        for entry in entries {
            let entry = entry.map_err(io_error(&replay_dir))?;
            let name = entry.file_name();
            let stale = name.to_str().is_some_and(|n| pattern.is_match_at_start(n))
                && !keep.contains(&name.as_os_str());
            if stale {
                let path = entry.path();
                fs::remove_file(&path).map_err(io_error(&path))?;
            }
        }
    }
    Ok(written)
}

/// `write_reports(run, out_dir)` (`reports/writer.py:94-110`) with gcg-data's values in
/// `data`, the replay documents from `replay_doc` (`replay_document`, which re-simulates the
/// game and can fail) and the `timing.json` host strings:
///
/// 1. build `results.json` and validate it; a schema violation writes nothing;
/// 2. the replays, then the stale-replay cleanup;
/// 3. `games.ndjson`, `summary.md`, `timing.json`, and `results.json` last.
///
/// `cancel` is checked before and after each file (PLAN §7.5): an interrupt stops the
/// remaining writes and returns [`ReportError::Interrupted`], as Python's `KeyboardInterrupt`
/// does even after some files were written.
///
/// # Errors
/// [`ReportError::Schema`], [`ReportError::Replay`] (the replay document's error),
/// [`ReportError::Io`] or [`ReportError::Interrupted`].
pub fn write_reports_with(
    run: &BenchmarkRun,
    out: &Path,
    cancel: &Cancel,
    data: &ResultsData,
    replay_doc: &mut dyn FnMut(&'static str, &GameRecord) -> Result<Json, ReplayError>,
    host: &TimingHost,
) -> Result<ReportPaths, ReportError> {
    let results = build_results_with(run, data);
    validate_results(&results, run.config.schema).map_err(ReportError::Schema)?;
    let replays = write_replays(run, out, cancel, replay_doc)?;
    let paths = ReportPaths {
        out_dir: out.to_path_buf(),
        results: out.join("results.json"),
        summary: out.join("summary.md"),
        games: out.join("games.ndjson"),
        replays,
        timing: out.join("timing.json"),
    };
    write_checked(&paths.games, &games_ndjson(run), cancel)?;
    write_checked(
        &paths.summary,
        &render_summary(&results, &reproduce_command(run)),
        cancel,
    )?;
    write_checked(
        &paths.timing,
        &to_json_text(&timing_json(run, host)),
        cancel,
    )?;
    // Results 1.2.0 only (W-100, A23): the copies of decks without a source file, which the
    // corrected reproduce line names. A 1.1.0 run writes none.
    for (rel, text) in written_decks(run) {
        write_checked(&out.join(rel), &text, cancel)?;
    }
    write_checked(&paths.results, &to_json_text(&results), cancel)?;
    Ok(paths)
}

/// `write_reports(run, out_dir)` (`reports/writer.py:94-110`): [`write_reports_with`] with the
/// embedded gcg-data values ([`ResultsData::embedded`]), the Rust build's `timing.json` host
/// strings ([`TimingHost::rust`]) and each replay document built by
/// [`replay_document_with`] from `resimulate(record)`, the re-simulation of the recorded game
/// (`replay_with_log`, `runner/replayfile.py:75-101`), which needs the engine.
///
/// # Errors
/// As [`write_reports_with`]; a failed re-simulation is [`ReportError::Replay`].
///
/// # Panics
/// If a deck of the configuration holds a card number that is not in the embedded card data,
/// as [`super::results::build_results`] does (Python raises `KeyError`; a parsed deck never
/// does).
pub fn write_reports(
    run: &BenchmarkRun,
    out: &Path,
    cancel: &Cancel,
    resimulate: &mut dyn FnMut(&GameRecord) -> Result<Resimulation, ReplayError>,
) -> Result<ReportPaths, ReportError> {
    let data = ResultsData::embedded(&run.config)
        .unwrap_or_else(|missing| panic!("a parsed deck holds only known cards: {missing}"));
    let mut replay_doc = |label: &'static str, record: &GameRecord| {
        let sim = resimulate(record)?;
        Ok(replay_document_with(record, &run.config, label, &sim))
    };
    write_reports_with(
        run,
        out,
        cancel,
        &data,
        &mut replay_doc,
        &TimingHost::rust(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shlex_quote_matches_python_examples() {
        assert_eq!(shlex_quote(""), "''");
        assert_eq!(shlex_quote("a/b.txt"), "a/b.txt");
        assert_eq!(shlex_quote("it's"), "'it'\"'\"'s'");
        assert_eq!(shlex_quote("Zaku Ⅱ"), "'Zaku Ⅱ'");
    }

    #[test]
    fn replay_file_names() {
        assert!(is_replay_file_name("win-m0000-g1.json"));
        assert!(is_replay_file_name("draw-m12345-g3.json\n"));
        assert!(!is_replay_file_name("win-m000-g1.json"));
        assert!(!is_replay_file_name("notes.txt"));
    }
}
