//! The report set written as one unit (W-102) and the opt-in files of `gcg-eval benchmark`
//! (A30: `--compress`, `--analysis`, `--replays-extremes`, `--diagnostics`; docs/port/PLAN.md
//! §12.2 P6-M).
//!
//! [`write_report_set`] builds every file in memory first: a schema violation, a failing
//! replay document or an interrupt before the swap writes nothing. It then writes the files
//! into a sibling staging directory `.{name}.{pid}.new` and swaps it in: one `rename` when the
//! report directory does not exist yet, otherwise `rename(out, .{name}.{pid}.old)`,
//! `rename(staging, out)` and the removal of the old set. A reader sees the complete old set,
//! the complete new one, or (between the two renames) no directory, never a mix. The swap
//! needs a directory that holds report files only ([`is_report_entry`]); a directory with
//! anything else in it (or `.`, or a symlink) keeps its other entries: the files are then
//! staged under temporary names inside it and renamed only once all are written,
//! `results.json` last, then the stale replays are removed, as Python's writer does
//! (`reports/writer.py:79-110`).
//!
//! File contents are those of [`super::write_reports_with`], byte for byte; the opt-in files
//! are additions. Without opt-ins the files equal `gcg-sim benchmark`'s (T-153).

use std::fs;
use std::path::{Path, PathBuf};

use gcg_core::pool::Cancel;
use gcg_core::pyjson::Json;

use super::pyregex::Regex;
use super::results::{ResultsData, build_results_with, replay_name, select_replays};
use super::schema::validate_results;
use super::summary::render_summary;
use super::writer::{
    DECK_DIRS, TimingHost, games_ndjson, is_replay_file_name, reproduce_command, timing_json,
    to_json_text, written_decks,
};
use crate::{BenchmarkRun, GameRecord, ReplayError, ReportError, ReportPaths};

/// The extra replays of `--replays-extremes` (W-148): `{kind}-{result}-m{match:04}-g{game}.json`
/// with kind `shortest` or `longest` and result `win` or `loss`.
pub const EXTREME_REPLAY_FILE: &str = r"^(shortest|longest)-(win|loss)-m\d{4,}-g[1-3]\.json$";

/// The fixed names of report files at the top of the directory: Python's four, the
/// compressed games file of `--compress`, and the opt-in side files.
pub const REPORT_FILES: [&str; 9] = [
    "results.json",
    "summary.md",
    "games.ndjson",
    "timing.json",
    "games.ndjson.zst",
    "games.ndjson.gz",
    "analysis.json",
    "diagnostics.json",
    crate::checkpoint::CHECKPOINT_FILE,
];

/// `games.ndjson` replaced by its compressed bytes (W-99).
pub struct Compression<'a> {
    /// `games.ndjson.zst` or `games.ndjson.gz`.
    pub file: &'a str,
    pub encode: &'a dyn Fn(&[u8]) -> Vec<u8>,
}

/// What `gcg-eval benchmark`'s opt-in options add to the set.
#[derive(Default)]
pub struct SetOptions<'a> {
    pub compress: Option<Compression<'a>>,
    /// `--replays-extremes` (W-148).
    pub extremes: bool,
    /// Further files `(name relative to the report directory, bytes)`: `analysis.json`,
    /// `diagnostics.json`.
    pub extra_files: Vec<(String, Vec<u8>)>,
    /// Keys added to `timing.json` after Python's and the topology (W-104).
    pub timing_extra: Vec<(String, Json)>,
    /// Called after each file is written into the staging area; `false` stops the write as an
    /// interrupt would, before the swap (W-102's abort test).
    pub after_file: Option<&'a mut dyn FnMut(&Path) -> bool>,
}

/// The shortest and longest deck-under-test win and loss (W-148): the minimum and maximum of
/// `turns`, ties broken by the lowest `(match_index, game_index)`. Labels `shortest-win`,
/// `longest-win`, `shortest-loss`, `longest-loss`, in that order; a result without games
/// gives none.
#[must_use]
pub fn select_extremes(run: &BenchmarkRun) -> Vec<(&'static str, &'static str, &GameRecord)> {
    let mut out = Vec::new();
    for (result, label) in [
        (super::aggregate::DutResult::Win, "win"),
        (super::aggregate::DutResult::Loss, "loss"),
    ] {
        let games: Vec<&GameRecord> = run
            .games()
            .filter(|g| super::aggregate::DutResult::of(g) == result)
            .collect();
        let key = |g: &&GameRecord| (g.match_index, g.game_index);
        let mut shortest: Option<&GameRecord> = None;
        let mut longest: Option<&GameRecord> = None;
        for g in &games {
            if shortest
                .is_none_or(|s| g.turns < s.turns || (g.turns == s.turns && key(g) < key(&s)))
            {
                shortest = Some(g);
            }
            if longest.is_none_or(|l| g.turns > l.turns || (g.turns == l.turns && key(g) < key(&l)))
            {
                longest = Some(g);
            }
        }
        if let Some(g) = shortest {
            out.push(("shortest", label, g));
        }
        if let Some(g) = longest {
            out.push(("longest", label, g));
        }
    }
    out
}

/// `replays/{kind}-{result}-m{match:04}-g{game}.json`.
#[must_use]
pub fn extreme_replay_name(kind: &str, result: &str, g: &GameRecord) -> String {
    format!(
        "replays/{kind}-{result}-m{:04}-g{}.json",
        g.match_index,
        u64::from(g.game_index) + 1
    )
}

fn extreme_pattern() -> Regex {
    Regex::new(EXTREME_REPLAY_FILE).expect("EXTREME_REPLAY_FILE is in the supported subset")
}

/// A replay file name this writer owns: Python's `REPLAY_FILE` or an extreme replay.
fn is_owned_replay(name: &str) -> bool {
    is_replay_file_name(name) || extreme_pattern().is_match_at_start(name)
}

/// Whether `name` inside a report directory belongs to the report set (so the swap may
/// replace it): the report files, the `replays/`, `decks/` (W-100) and `variants/` (the
/// swap-test decks of `analysis.json`) directories, and this writer's temporary names.
#[must_use]
pub fn is_report_entry(name: &str) -> bool {
    REPORT_FILES.contains(&name)
        || name == "replays"
        || name == "decks"
        || name == "variants"
        || (name.starts_with('.') && name.ends_with(".tmp"))
}

fn io_error(path: &Path) -> impl FnOnce(std::io::Error) -> ReportError + '_ {
    move |source| ReportError::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// Whether the directory `dir` holds only report-set entries (and `replays/` only replay
/// files of this writer, `decks/` only the [`DECK_DIRS`] with deck files), so it can be
/// replaced as a whole.
fn only_report_entries(dir: &Path) -> Result<bool, ReportError> {
    for entry in fs::read_dir(dir).map_err(io_error(dir))? {
        let entry = entry.map_err(io_error(dir))?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            return Ok(false);
        };
        if !is_report_entry(name) {
            return Ok(false);
        }
        let path = entry.path();
        let kind = entry.file_type().map_err(io_error(&path))?;
        let replay_entry = |n: &str, k: &fs::FileType| {
            k.is_file() && (is_owned_replay(n) || (n.starts_with('.') && n.ends_with(".tmp")))
        };
        let ok = match name {
            "replays" => kind.is_dir() && dir_matches(&path, &replay_entry)?,
            "decks" => {
                kind.is_dir()
                    && dir_matches(&path, &|n, k| k.is_dir() && DECK_DIRS.contains(&n))?
                    && DECK_DIRS.iter().all(|sub| {
                        let p = path.join(sub);
                        !p.exists()
                            || dir_matches(&p, &|n, k| k.is_file() && n.ends_with(".txt"))
                                .unwrap_or(false)
                    })
            }
            "variants" => {
                kind.is_dir() && dir_matches(&path, &|n, k| k.is_file() && n.ends_with(".txt"))?
            }
            _ => kind.is_file(),
        };
        if !ok {
            return Ok(false);
        }
    }
    Ok(true)
}

fn dir_matches(dir: &Path, ok: &dyn Fn(&str, &fs::FileType) -> bool) -> Result<bool, ReportError> {
    for entry in fs::read_dir(dir).map_err(io_error(dir))? {
        let entry = entry.map_err(io_error(dir))?;
        let kind = entry.file_type().map_err(io_error(&entry.path()))?;
        match entry.file_name().to_str() {
            Some(n) if ok(n, &kind) => {}
            _ => return Ok(false),
        }
    }
    Ok(true)
}

/// One file of the set: its path relative to the report directory and its bytes.
struct Planned {
    rel: String,
    bytes: Vec<u8>,
}

/// Every file of the set, in Python's write order (replays, `games.ndjson`, `summary.md`,
/// `timing.json`), then the deck copies and opt-in files, `results.json` last.
fn plan_files(
    run: &BenchmarkRun,
    data: &ResultsData,
    replay_doc: &mut dyn FnMut(&'static str, &GameRecord) -> Result<Json, ReplayError>,
    host: &TimingHost,
    opts: &SetOptions<'_>,
) -> Result<(Vec<Planned>, Vec<String>), ReportError> {
    let results = build_results_with(run, data);
    validate_results(&results, run.config.schema).map_err(ReportError::Schema)?;
    let mut files = Vec::new();
    let mut replays = Vec::new();
    for (label, record) in select_replays(run) {
        let rel = replay_name(label, record);
        let doc = replay_doc(label, record).map_err(ReportError::Replay)?;
        files.push(Planned {
            rel: rel.clone(),
            bytes: to_json_text(&doc).into_bytes(),
        });
        replays.push(rel);
    }
    if opts.extremes {
        for (kind, label, record) in select_extremes(run) {
            let doc = replay_doc(label, record).map_err(ReportError::Replay)?;
            files.push(Planned {
                rel: extreme_replay_name(kind, label, record),
                bytes: to_json_text(&doc).into_bytes(),
            });
        }
    }
    let games = games_ndjson(run).into_bytes();
    match &opts.compress {
        None => files.push(Planned {
            rel: "games.ndjson".to_owned(),
            bytes: games,
        }),
        Some(c) => files.push(Planned {
            rel: c.file.to_owned(),
            bytes: (c.encode)(&games),
        }),
    }
    files.push(Planned {
        rel: "summary.md".to_owned(),
        bytes: render_summary(&results, &reproduce_command(run)).into_bytes(),
    });
    let mut timing = timing_json(run, host);
    for (k, v) in &opts.timing_extra {
        timing.set(k.as_str(), v.clone());
    }
    files.push(Planned {
        rel: "timing.json".to_owned(),
        bytes: to_json_text(&timing).into_bytes(),
    });
    for (rel, text) in written_decks(run) {
        files.push(Planned {
            rel,
            bytes: text.into_bytes(),
        });
    }
    for (rel, bytes) in &opts.extra_files {
        files.push(Planned {
            rel: rel.clone(),
            bytes: bytes.clone(),
        });
    }
    files.push(Planned {
        rel: "results.json".to_owned(),
        bytes: to_json_text(&results).into_bytes(),
    });
    Ok((files, replays))
}

fn write_file(path: &Path, bytes: &[u8]) -> Result<(), ReportError> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(io_error(parent))?;
    }
    fs::write(path, bytes).map_err(io_error(path))
}

/// The error with a staging path shown as the final report path.
fn shown_at(e: ReportError, staging: &Path, out: &Path) -> ReportError {
    match e {
        ReportError::Io { path, source } => ReportError::Io {
            path: path
                .strip_prefix(staging)
                .map_or(path.clone(), |rel| out.join(rel)),
            source,
        },
        other => other,
    }
}

fn stop(cancel: &Cancel, after: &mut Option<&mut dyn FnMut(&Path) -> bool>, path: &Path) -> bool {
    if cancel.is_cancelled() {
        return true;
    }
    match after {
        Some(f) => !f(path),
        None => false,
    }
}

/// `write_reports_with`'s files, written as one set (W-102), plus the opt-in files of `opts`.
/// `cancel` is checked before the files are built and after each one is written; an interrupt
/// (or `opts.after_file` returning `false`) leaves the previous set as it was.
///
/// # Errors
/// [`ReportError::Schema`] or [`ReportError::Replay`] (nothing written),
/// [`ReportError::Io`] (the old set kept; the path is the report path of the failing file),
/// or [`ReportError::Interrupted`] (the old set kept).
pub fn write_report_set(
    run: &BenchmarkRun,
    out: &Path,
    cancel: &Cancel,
    data: &ResultsData,
    replay_doc: &mut dyn FnMut(&'static str, &GameRecord) -> Result<Json, ReplayError>,
    host: &TimingHost,
    mut opts: SetOptions<'_>,
) -> Result<ReportPaths, ReportError> {
    if cancel.is_cancelled() {
        return Err(ReportError::Interrupted);
    }
    let (files, replays) = plan_files(run, data, replay_doc, host, &opts)?;
    if cancel.is_cancelled() {
        return Err(ReportError::Interrupted);
    }
    let games_name = opts
        .compress
        .as_ref()
        .map_or("games.ndjson", |c| c.file)
        .to_owned();
    let paths = ReportPaths {
        out_dir: out.to_path_buf(),
        results: out.join("results.json"),
        summary: out.join("summary.md"),
        games: out.join(&games_name),
        replays: replays.iter().map(|r| out.join(r)).collect(),
        timing: out.join("timing.json"),
    };
    // A symlinked report directory is updated in place, so the link and its target stay.
    let link = fs::symlink_metadata(out).is_ok_and(|m| m.file_type().is_symlink());
    let swappable = !link
        && out.file_name().is_some_and(|n| n != "." && n != "..")
        && (!out.exists() || (out.is_dir() && only_report_entries(out)?));
    if swappable {
        swap_in(out, &files, cancel, &mut opts.after_file)?;
    } else {
        write_in_place(out, &files, cancel, &mut opts.after_file)?;
    }
    Ok(paths)
}

/// The staging directory beside `out`.
fn sibling(out: &Path, tag: &str) -> PathBuf {
    let name = out
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    out.with_file_name(format!(".{name}.{}.{tag}", std::process::id()))
}

fn swap_in(
    out: &Path,
    files: &[Planned],
    cancel: &Cancel,
    after: &mut Option<&mut dyn FnMut(&Path) -> bool>,
) -> Result<(), ReportError> {
    let staging = sibling(out, "new");
    if staging.exists() {
        fs::remove_dir_all(&staging).map_err(io_error(&staging))?;
    }
    let written = (|| {
        fs::create_dir_all(&staging).map_err(io_error(&staging))?;
        for f in files {
            let path = staging.join(&f.rel);
            write_file(&path, &f.bytes)?;
            if stop(cancel, after, &out.join(&f.rel)) {
                return Err(ReportError::Interrupted);
            }
        }
        Ok(())
    })();
    if let Err(e) = written {
        let _ = fs::remove_dir_all(&staging);
        return Err(shown_at(e, &staging, out));
    }
    if !out.exists() {
        if let Some(parent) = out.parent()
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent).map_err(io_error(parent))?;
        }
        return fs::rename(&staging, out).map_err(|e| {
            let _ = fs::remove_dir_all(&staging);
            io_error(out)(e)
        });
    }
    let old = sibling(out, "old");
    if old.exists() {
        fs::remove_dir_all(&old).map_err(io_error(&old))?;
    }
    if let Err(e) = fs::rename(out, &old) {
        let _ = fs::remove_dir_all(&staging);
        return Err(io_error(out)(e));
    }
    if let Err(e) = fs::rename(&staging, out) {
        let _ = fs::rename(&old, out);
        let _ = fs::remove_dir_all(&staging);
        return Err(io_error(out)(e));
    }
    fs::remove_dir_all(&old).map_err(io_error(&old))
}

/// The temporary name of a file staged inside the report directory.
fn temp_of(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    path.with_file_name(format!(".{name}.{}.tmp", std::process::id()))
}

fn write_in_place(
    out: &Path,
    files: &[Planned],
    cancel: &Cancel,
    after: &mut Option<&mut dyn FnMut(&Path) -> bool>,
) -> Result<(), ReportError> {
    let mut staged: Vec<(PathBuf, PathBuf)> = Vec::with_capacity(files.len());
    let written = (|| {
        for f in files {
            let target = out.join(&f.rel);
            let temp = temp_of(&target);
            write_file(&temp, &f.bytes)?;
            staged.push((temp, target.clone()));
            if stop(cancel, after, &target) {
                return Err(ReportError::Interrupted);
            }
        }
        Ok(())
    })();
    if let Err(e) = written {
        for (temp, _) in &staged {
            let _ = fs::remove_file(temp);
        }
        return Err(e);
    }
    for (temp, target) in &staged {
        fs::rename(temp, target).map_err(io_error(temp))?;
    }
    // Report files of an earlier set that this set does not write (an uncompressed or
    // compressed games file, side files, a finished run's checkpoint) are stale.
    for name in REPORT_FILES {
        let path = out.join(name);
        if !files.iter().any(|f| f.rel == name) && path.is_file() {
            fs::remove_file(&path).map_err(io_error(&path))?;
        }
    }
    let keep: Vec<String> = files
        .iter()
        .filter_map(|f| f.rel.strip_prefix("replays/").map(str::to_owned))
        .collect();
    let replay_dir = out.join("replays");
    if replay_dir.is_dir() {
        for entry in fs::read_dir(&replay_dir).map_err(io_error(&replay_dir))? {
            let entry = entry.map_err(io_error(&replay_dir))?;
            let name = entry.file_name();
            let stale = name
                .to_str()
                .is_some_and(|n| is_owned_replay(n) && !keep.iter().any(|k| k == n));
            if stale {
                let path = entry.path();
                fs::remove_file(&path).map_err(io_error(&path))?;
            }
        }
    }
    Ok(())
}
