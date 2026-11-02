//! Checkpoints and `--resume` (W-105; `gcg-eval benchmark --resume`, A30).
//!
//! With `--resume` the runner appends each finished match to `checkpoint.ndjson` in the report
//! directory: a header line with the configuration and its digest, then one line per match
//! (its index, its wall seconds for `timing.json`, and its full game records), in completion
//! order. A later run with `--resume` and the same configuration reads the file, verifies the
//! digest and plays only the missing matches. Every match is a pure function of the
//! configuration, the master seed and its index (`runner/run.py:1-7`), and a record survives
//! the JSON round trip unchanged (`GameRecord::to_json`/`from_json`), so the reports equal an
//! uninterrupted run's byte for byte (`timing.json` aside). A different configuration is
//! refused; a final line cut short by a kill is ignored and its match played again.
//!
//! The checkpoint is written only with `--resume`; the report writer removes it with the old
//! report set once the reports are written ([`crate::reports::set`]). Failures keep the
//! reference semantics (no reports, exit 1 or 130), and the checkpoint stays for the next
//! resume.
//!
//! Agent factories cross threads by construction, so no runtime check is needed (W-107): a
//! factory that is not `Sync` does not compile.
//!
//! ```compile_fail
//! use std::cell::Cell;
//! use gcg_runner::{AgentFactory, AgentSpec};
//! struct NotSync(Cell<u32>);
//! impl AgentFactory for NotSync {
//!     fn make(&self, _: &AgentSpec) -> Result<Box<dyn gcg_ai::Agent>, gcg_ai::AgentError> {
//!         unreachable!()
//!     }
//!     fn describe(&self) -> String { String::new() }
//! }
//! ```

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use gcg_core::pool::Cancel;
use gcg_core::pyjson::{DumpOptions, Json, dumps};
use gcg_core::stable_hash::stable_key;
use gcg_core::sys::topology;
use serde_json::Value;

use crate::clock::Stopwatch;
use crate::matches::play_match;
use crate::run::run_matches;
use crate::{
    AgentFactory, BenchmarkConfig, BenchmarkRun, GameRecord, MatchRecord, Outcome, RunError,
};

/// The checkpoint's file name in the report directory.
pub const CHECKPOINT_FILE: &str = "checkpoint.ndjson";
/// The header's `format`.
pub const CHECKPOINT_FORMAT: &str = "gcg-eval-checkpoint";
/// The header's `version`.
pub const CHECKPOINT_VERSION: u32 = 1;

/// Why a resumable run stopped without a result.
#[derive(Debug)]
pub enum ResumeError {
    /// The run failed (lowest failing match, W-149) or was interrupted; the checkpoint keeps
    /// the matches that finished.
    Run(RunError),
    /// The checkpoint belongs to another configuration, is not a checkpoint, or cannot be
    /// read or written.
    Checkpoint(String),
}

impl fmt::Display for ResumeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Run(RunError::GameFailure(m)) => write!(f, "simulation failed: {m}"),
            Self::Run(RunError::Interrupted) => f.write_str(crate::INTERRUPTED_MESSAGE),
            Self::Checkpoint(m) => f.write_str(m),
        }
    }
}

/// What a match record depends on: both decks (name and digest), every `BenchmarkConfig`
/// field except `workers` and `out_dir`, and the agent factory's description (which names
/// the AI, `results.json` `ai.factory`).
#[must_use]
pub fn config_identity(cfg: &BenchmarkConfig, factory: &str) -> Json {
    let deck = |d: &gcg_data::Deck| {
        let mut o = Json::object();
        o.set("name", &*d.name);
        o.set("digest", d.digest.as_str());
        o
    };
    let mut o = Json::object();
    o.set("deck_under_test", deck(&cfg.deck_under_test));
    o.set("benchmark_deck", deck(&cfg.benchmark_deck));
    o.set("matches", cfg.matches);
    o.set("seed", cfg.seed);
    o.set("format", cfg.format.as_str());
    o.set("ai_preset", cfg.ai_preset);
    o.set("decision_log", cfg.decision_log);
    o.set("replays", cfg.replays);
    o.set("turn_limit", cfg.turn_limit);
    o.set("max_actions", cfg.max_actions);
    o.set("schema", cfg.schema.as_str());
    o.set("package", cfg.package);
    o.set("code_digest", cfg.code_digest);
    o.set("factory", factory);
    o
}

/// The 16-hex-digit digest of a configuration identity: the first 8 bytes of the SHA-256 of
/// its compact sorted JSON (`gcg_core::stable_hash`).
#[must_use]
pub fn config_digest(identity: &Json) -> String {
    let text = dumps(identity, DumpOptions::COMPACT_SORTED_ASCII);
    format!("{:016x}", stable_key(text.as_bytes()).0)
}

fn header(identity: &Json) -> String {
    let mut o = Json::object();
    o.set("format", CHECKPOINT_FORMAT);
    o.set("version", CHECKPOINT_VERSION);
    o.set("config_digest", config_digest(identity));
    o.set("config", identity.clone());
    let mut line = dumps(&o, DumpOptions::COMPACT_SORTED_ASCII);
    line.push('\n');
    line
}

/// One finished match as a checkpoint line.
#[must_use]
pub fn match_line(record: &MatchRecord, seconds: f64) -> String {
    let mut o = Json::object();
    o.set("match_index", record.match_index);
    o.set("seconds", seconds);
    o.set("dut_seat", record.dut_seat);
    o.set("winner", record.winner.as_str());
    o.set("dut_points", record.dut_points);
    o.set("bench_points", record.bench_points);
    o.set(
        "games",
        Json::Array(record.games.iter().map(GameRecord::to_json).collect()),
    );
    let mut line = dumps(&o, DumpOptions::COMPACT_SORTED_ASCII);
    line.push('\n');
    line
}

fn outcome(v: &Value) -> Option<Outcome> {
    match v.as_str()? {
        "dut" => Some(Outcome::Dut),
        "bench" => Some(Outcome::Bench),
        "draw" => Some(Outcome::Draw),
        _ => None,
    }
}

/// A checkpoint line back as `(record, seconds)`.
///
/// # Errors
/// What is missing or malformed.
pub fn parse_match_line(line: &str) -> Result<(MatchRecord, f64), String> {
    let v: Value = serde_json::from_str(line).map_err(|e| format!("not JSON: {e}"))?;
    let num = |k: &str| v.get(k).and_then(Value::as_u64);
    let small = |k: &str| num(k).and_then(|x| u32::try_from(x).ok());
    let (Some(match_index), Some(dut_seat), Some(dut_points), Some(bench_points)) = (
        small("match_index"),
        num("dut_seat").and_then(|x| u8::try_from(x).ok()),
        small("dut_points"),
        small("bench_points"),
    ) else {
        return Err("a match line needs match_index, dut_seat, dut_points and bench_points".into());
    };
    let winner = v
        .get("winner")
        .and_then(outcome)
        .ok_or("a match line needs a winner")?;
    let seconds = v.get("seconds").and_then(Value::as_f64).unwrap_or(0.0);
    let games = v
        .get("games")
        .and_then(Value::as_array)
        .ok_or("a match line needs games")?
        .iter()
        .map(|g| GameRecord::from_json(g).map_err(|e| format!("game record: {e:?}")))
        .collect::<Result<Vec<GameRecord>, String>>()?;
    Ok((
        MatchRecord {
            match_index,
            dut_seat,
            games,
            winner,
            dut_points,
            bench_points,
        },
        seconds,
    ))
}

/// The finished matches of the checkpoint at `path` for `identity`, by match index. A final
/// line without its newline (a write cut short) is ignored.
///
/// # Errors
/// The file is not a checkpoint of this configuration (the message names the differing
/// configuration fields), or a complete line is malformed.
pub fn load(path: &Path, identity: &Json) -> Result<BTreeMap<u32, (MatchRecord, f64)>, String> {
    let shown = gcg_data::py_path_str(path);
    let text = fs::read_to_string(path).map_err(|e| format!("cannot read {shown}: {e}"))?;
    let mut lines: Vec<&str> = text.split_inclusive('\n').collect();
    if lines.last().is_some_and(|l| !l.ends_with('\n')) {
        lines.pop();
    }
    let Some(first) = lines.first() else {
        return Err(format!(
            "{shown} is empty; delete it or run without --resume"
        ));
    };
    let head: Value = serde_json::from_str(first)
        .map_err(|_| format!("{shown} is not a {CHECKPOINT_FORMAT} file"))?;
    if head.get("format").and_then(Value::as_str) != Some(CHECKPOINT_FORMAT)
        || head.get("version").and_then(Value::as_u64) != Some(u64::from(CHECKPOINT_VERSION))
    {
        return Err(format!(
            "{shown} is not a {CHECKPOINT_FORMAT} v{CHECKPOINT_VERSION} file"
        ));
    }
    let digest = config_digest(identity);
    if head.get("config_digest").and_then(Value::as_str) != Some(digest.as_str()) {
        let want: Value = serde_json::from_str(&dumps(identity, DumpOptions::COMPACT_SORTED_ASCII))
            .unwrap_or(Value::Null);
        let had = head.get("config").cloned().unwrap_or(Value::Null);
        let mut fields: Vec<String> = want
            .as_object()
            .map(|o| {
                o.iter()
                    .filter(|(k, v)| had.get(k.as_str()) != Some(v))
                    .map(|(k, _)| k.clone())
                    .collect()
            })
            .unwrap_or_default();
        fields.sort();
        return Err(format!(
            "{shown} was written by a different configuration ({}); refusing to resume. Use the \
             original options, or delete it to start over",
            if fields.is_empty() {
                "the configuration digest differs".to_owned()
            } else {
                format!("differs in {}", fields.join(", "))
            }
        ));
    }
    let mut done = BTreeMap::new();
    for (i, line) in lines.iter().enumerate().skip(1) {
        let (record, seconds) =
            parse_match_line(line).map_err(|e| format!("{shown}:{}: {e}", i + 1))?;
        done.insert(record.match_index, (record, seconds));
    }
    Ok(done)
}

/// `run_benchmark` with a checkpoint at `path` (W-105): the matches already in it are taken
/// from it, the others played on the pool and appended as they finish. Without a checkpoint
/// file one is created with its header. The progress counts every finished match, taken or
/// played.
///
/// # Errors
/// [`ResumeError::Checkpoint`] for a checkpoint of another configuration or an I/O failure;
/// [`ResumeError::Run`] for the lowest failing match or an interrupt.
pub fn run_resumable(
    cfg: &BenchmarkConfig,
    f: &dyn AgentFactory,
    progress: &mut dyn FnMut(u32, u32),
    cancel: &Cancel,
    path: &Path,
) -> Result<BenchmarkRun, ResumeError> {
    let factory = f.describe();
    let identity = config_identity(cfg, &factory);
    let shown = gcg_data::py_path_str(path);
    let io_message = |e: &io::Error| format!("cannot write the checkpoint {shown}: {e}");
    let done = if path.exists() {
        let done = load(path, &identity).map_err(ResumeError::Checkpoint)?;
        drop_partial_line(path).map_err(|e| ResumeError::Checkpoint(io_message(&e)))?;
        done
    } else {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent).map_err(|e| ResumeError::Checkpoint(io_message(&e)))?;
        }
        fs::write(path, header(&identity)).map_err(|e| ResumeError::Checkpoint(io_message(&e)))?;
        BTreeMap::new()
    };
    let file = fs::OpenOptions::new()
        .append(true)
        .open(path)
        .map_err(|e| ResumeError::Checkpoint(io_message(&e)))?;
    let sink = Mutex::new(file);
    let write_error: Mutex<Option<String>> = Mutex::new(None);
    let result = run_matches(
        cfg,
        &topology::detect(),
        factory,
        progress,
        cancel,
        &|m, cx| {
            if let Some((record, _)) = done.get(&m) {
                return Ok(record.clone());
            }
            let watch = Stopwatch::start();
            let record = play_match(cfg, m, f, cx)?;
            let line = match_line(&record, watch.seconds());
            let written = sink
                .lock()
                .map_err(|_| io::Error::other("the checkpoint lock is poisoned"))
                .and_then(|mut file| file.write_all(line.as_bytes()));
            if let Err(e) = written {
                let message = io_message(&e);
                if let Ok(mut slot) = write_error.lock() {
                    slot.get_or_insert(message.clone());
                }
                return Err(RunError::GameFailure(message));
            }
            Ok(record)
        },
    );
    if let Some(message) = write_error.lock().ok().and_then(|mut s| s.take()) {
        return Err(ResumeError::Checkpoint(message));
    }
    let mut run = result.map_err(ResumeError::Run)?;
    for (i, record) in run.matches.iter().enumerate() {
        if let (Some((_, seconds)), Some(slot)) = (
            done.get(&record.match_index),
            run.timing.match_seconds.get_mut(i),
        ) {
            *slot = *seconds;
        }
    }
    Ok(run)
}

/// Cuts a final line without its newline (a write cut short) off the file, so that the next
/// appended line starts on a line of its own.
fn drop_partial_line(path: &Path) -> io::Result<()> {
    let bytes = fs::read(path)?;
    let keep = bytes.iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1);
    if keep < bytes.len() {
        let len = u64::try_from(keep).map_err(io::Error::other)?;
        fs::OpenOptions::new()
            .write(true)
            .open(path)?
            .set_len(len)?;
    }
    Ok(())
}

/// The checkpoint path of a report directory.
#[must_use]
pub fn checkpoint_path(out: &Path) -> PathBuf {
    out.join(CHECKPOINT_FILE)
}
