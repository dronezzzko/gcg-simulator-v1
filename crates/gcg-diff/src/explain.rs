//! `gcg-diff explain <trace-id> <decision>` (PLAN §8.7, A21): the state the Rust engine
//! reaches at a decision of a game or search trace, diffed field by field against Python's
//! canonical state there, and the two option lists side by side.
//!
//! Python's state is the trace's own snapshot at that decision when it stores one (offline);
//! otherwise `uv run python oracle/traces.py state --trace ID --decision K` regenerates it.
//! Rust's state comes from replaying the recorded choices of every earlier decision through
//! the real engine. The diff names card and battle fields after `CardInstance.to_json` and
//! `Battle.to_json` (`engine/state.py:118-130`, `:173-185`), zones by their `Zone` name and
//! definitions by their card number.

use std::path::Path;
use std::process::Command;

use gcg_data::{CardDb, DefId};
use gcg_engine::Zone;
use serde_json::Value;

use crate::engine::RealEngine;
use crate::json::short;
use crate::manifest::{self, Repo};
use crate::reader::{parse_record, read_trace};
use crate::replay::{action_display, header_spec};
use crate::{DecisionRec, EngineUnderTest, Record, SnapshotRec, Trace, TraceKind, TraceOptions};
use gcg_engine::StateSnapshot;

/// `CardInstance.to_json` positions (`engine/state.py:118-130`).
pub const CARD_FIELDS: [&str; 10] = [
    "uid",
    "def_id",
    "owner",
    "zone",
    "rested",
    "damage",
    "pair",
    "zone_seq",
    "entered_turn",
    "known",
];

/// `Battle.to_json` positions (`engine/state.py:173-185`).
pub const BATTLE_FIELDS: [&str; 10] = [
    "attacker",
    "attacker_seq",
    "target",
    "target_seq",
    "defender",
    "battle_id",
    "blocked",
    "damage_only",
    "attack_triggers",
    "ended",
];

/// A trace found in a corpus tier: the file it is in (relative to the tier root) and the
/// trace.
pub struct Found {
    pub file: String,
    pub trace: Trace,
}

/// The trace with this id in tier `tier`, or in `file` when given. The set is the id's first
/// path component (`fuzz/s1/g0017`), or `tests` for a pytest node id.
///
/// # Errors
/// A manifest or decoding error, or no trace with that id.
pub fn find_trace(repo: &Repo, tier: u8, file: Option<&Path>, id: &str) -> Result<Found, String> {
    if let Some(f) = file {
        let traces = read_trace(f).map_err(|e| e.to_string())?;
        return traces
            .into_iter()
            .find(|t| t.header.id == id)
            .map(|trace| Found {
                file: f.display().to_string(),
                trace,
            })
            .ok_or_else(|| format!("{}: no trace {id}", f.display()));
    }
    let m = manifest::load(repo, tier)?;
    let set = if id.starts_with("tests/") {
        "tests"
    } else {
        id.split('/').next().unwrap_or_default()
    };
    for e in m
        .entries
        .iter()
        .filter(|e| e.records.is_some() && e.set == set)
    {
        let traces = read_trace(&m.root.join(&e.path)).map_err(|x| x.to_string())?;
        if let Some(trace) = traces.into_iter().find(|t| t.header.id == id) {
            return Ok(Found {
                file: e.path.clone(),
                trace,
            });
        }
    }
    Err(format!("no trace {id} in set {set:?} of tier {tier}"))
}

/// The decision record with index `k`.
///
/// # Errors
/// The trace has none (per-test traces have no decisions).
pub fn decision(t: &Trace, k: u32) -> Result<&DecisionRec, String> {
    t.records
        .iter()
        .find_map(|r| match r {
            Record::Decision(d) if d.index == k => Some(d),
            _ => None,
        })
        .ok_or_else(|| format!("{} has no decision {k}", t.header.id))
}

/// Replay `t`'s recorded choices before decision `k` through `engine`, which then holds the
/// state Python's decision `k` saw.
///
/// # Errors
/// A per-test trace, a failed `new_game`, snapshot load or `apply`.
pub fn replay_to(t: &Trace, k: u32, engine: &mut RealEngine) -> Result<(), String> {
    if t.header.kind == TraceKind::Test {
        return Err(format!(
            "{} is a per-test trace: explain works on game and search traces",
            t.header.id
        ));
    }
    let mut loaded = false;
    if let Some(spec) = header_spec(&t.header) {
        engine
            .new_game(&spec)
            .map_err(|e| format!("new_game: {}: {}", e.class, e.message))?;
        loaded = true;
    }
    for r in &t.records {
        match r {
            Record::Override(o) => engine
                .set_registry(&o.fragment)
                .map_err(|e| format!("override: {}", e.message))?,
            Record::Snapshot(s) if !loaded => {
                engine
                    .load(&s.snapshot)
                    .map_err(|e| format!("snapshot {}: {}", s.index, e.message))?;
                loaded = true;
            }
            Record::Decision(d) if d.index >= k => return Ok(()),
            Record::Decision(d) => engine.apply(d.chosen, true).map_err(|e| {
                format!(
                    "decision {}: apply {}: {}: {}",
                    d.index,
                    action_display(&d.chosen),
                    e.class,
                    e.message
                )
            })?,
            _ => {}
        }
    }
    Err(format!("{} ends before decision {k}", t.header.id))
}

/// Python's snapshot at decision `k`: the trace's own, or `traces.py state`'s.
///
/// # Errors
/// No stored snapshot and the regeneration failed.
pub fn python_snapshot(
    repo: &Repo,
    t: &Trace,
    k: u32,
) -> Result<(StateSnapshot, &'static str), String> {
    let stored = t.records.iter().find_map(|r| match r {
        Record::Snapshot(SnapshotRec { index, snapshot }) if *index == k => Some(snapshot.clone()),
        _ => None,
    });
    if let Some(s) = stored {
        return Ok((s, "stored in the trace"));
    }
    let out = Command::new("uv")
        .args(["run", "python", "oracle/traces.py", "state", "--trace"])
        .arg(&t.header.id)
        .arg("--decision")
        .arg(k.to_string())
        .current_dir(&repo.root)
        .output()
        .map_err(|e| {
            format!("decision {k} has no stored snapshot and `uv` could not run (needs uv): {e}")
        })?;
    if !out.status.success() {
        return Err(format!(
            "traces.py state failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    match parse_record(text.trim()) {
        Ok(Record::Snapshot(s)) => Ok((s.snapshot, "regenerated by traces.py state")),
        Ok(_) => Err("traces.py state did not print a snapshot record".to_owned()),
        Err(e) => Err(format!("traces.py state: {e}")),
    }
}

fn zone_name(v: &Value) -> String {
    match v.as_i64().and_then(Zone::from_value) {
        Some(z) => format!("{} ({})", v, z.py_name()),
        None => short(v),
    }
}

fn def_name(db: &CardDb, v: &Value) -> String {
    match v.as_u64().and_then(|d| u16::try_from(d).ok()) {
        Some(d) if usize::from(d) < db.len() => {
            format!("{v} ({})", db.by_id(DefId(d)).card_number)
        }
        _ => short(v),
    }
}

/// Every difference between Python's and Rust's canonical states, in document order (keys
/// sorted), named for reading.
#[must_use]
pub fn state_diffs(py: &Value, rs: &Value, db: &CardDb) -> Vec<String> {
    let mut out = Vec::new();
    walk(py, rs, &mut Vec::new(), db, &mut out);
    out
}

fn named(path: &[String], db: &CardDb, py: &Value, rs: &Value) -> String {
    let field = |table: &[&str; 10], i: &str| {
        i.parse::<usize>()
            .ok()
            .and_then(|i| table.get(i).copied())
            .unwrap_or("?")
            .to_owned()
    };
    match path {
        [top, i, j] if top == "cards" => {
            let f = field(&CARD_FIELDS, j);
            let (p, r) = match f.as_str() {
                "zone" => (zone_name(py), zone_name(rs)),
                "def_id" => (def_name(db, py), def_name(db, rs)),
                _ => (short(py), short(rs)),
            };
            format!("cards[{i}].{f}: python {p}, rust {r}")
        }
        [top, i, j] if top == "battles" => {
            format!(
                "battles[{i}].{}: python {}, rust {}",
                field(&BATTLE_FIELDS, j),
                short(py),
                short(rs)
            )
        }
        [top, p, z, rest @ ..] if top == "zones" => {
            let zone = z
                .parse::<i64>()
                .ok()
                .and_then(Zone::from_value)
                .map_or_else(|| z.clone(), |x| x.py_name().to_owned());
            let tail: String = rest.iter().map(|x| format!("[{x}]")).collect();
            format!(
                "zones[{p}][{zone}]{tail}: python {}, rust {}",
                short(py),
                short(rs)
            )
        }
        _ => {
            let mut s = String::new();
            for (n, part) in path.iter().enumerate() {
                if part.parse::<usize>().is_ok() || part.starts_with('#') {
                    s.push_str(&format!("[{}]", part.trim_start_matches('#')));
                } else {
                    if n > 0 {
                        s.push('.');
                    }
                    s.push_str(part);
                }
            }
            format!("{s}: python {}, rust {}", short(py), short(rs))
        }
    }
}

fn walk(py: &Value, rs: &Value, path: &mut Vec<String>, db: &CardDb, out: &mut Vec<String>) {
    match (py, rs) {
        (Value::Array(a), Value::Array(b)) => {
            for (i, (x, y)) in a.iter().zip(b).enumerate() {
                path.push(i.to_string());
                walk(x, y, path, db, out);
                path.pop();
            }
            if a.len() != b.len() {
                path.push("#len".to_owned());
                let mut p = path.clone();
                p.pop();
                let mut base = named(&p, db, &Value::Null, &Value::Null);
                if let Some(cut) = base.find(": python") {
                    base.truncate(cut);
                }
                out.push(format!(
                    "{base} (length): python {}, rust {}",
                    a.len(),
                    b.len()
                ));
                path.pop();
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            let mut keys: Vec<&String> = a.keys().chain(b.keys()).collect();
            keys.sort();
            keys.dedup();
            for k in keys {
                path.push(k.clone());
                match (a.get(k.as_str()), b.get(k.as_str())) {
                    (Some(x), Some(y)) => walk(x, y, path, db, out),
                    (x, y) => out.push(named(
                        path,
                        db,
                        x.unwrap_or(&Value::Null),
                        y.unwrap_or(&Value::Null),
                    )),
                }
                path.pop();
            }
        }
        (x, y) if x == y => {}
        (x, y) => out.push(named(path, db, x, y)),
    }
}

/// The option lists side by side, in `Action` display form (`play_unit(2,1)`).
#[must_use]
pub fn options_side_by_side(py: &[gcg_engine::Action], rs: &[gcg_engine::Action]) -> Vec<String> {
    let n = py.len().max(rs.len());
    let width = py
        .iter()
        .map(|a| action_display(a).len())
        .max()
        .unwrap_or(0)
        .max(6);
    let mut out = vec![format!("  {:>3}  {:<width$}  rust", "#", "python")];
    for i in 0..n {
        let p = py.get(i).map(action_display).unwrap_or_default();
        let r = rs.get(i).map(action_display).unwrap_or_default();
        let mark = if p == r { ' ' } else { '*' };
        out.push(format!("{mark} {i:>3}  {p:<width$}  {r}"));
    }
    out
}

/// The `explain` report.
///
/// # Errors
/// The trace or decision does not exist, or a side could not be built.
pub fn explain(
    repo: &Repo,
    tier: u8,
    file: Option<&Path>,
    id: &str,
    k: u32,
    limit: usize,
) -> Result<Vec<String>, String> {
    let found = find_trace(repo, tier, file, id)?;
    let t = &found.trace;
    let d = decision(t, k)?;
    let (snap, origin) = python_snapshot(repo, t, k)?;
    let mut engine = RealEngine::for_trace(&t.header);
    let mut lines = vec![format!(
        "explain {id} at decision {k} (turn {}, step {}, player {}, kind {}) in {}",
        d.fingerprint.turn,
        d.fingerprint.step.py_name(),
        d.player,
        d.kind.as_str(),
        found.file
    )];
    lines.push(format!("  python state  {origin}"));
    if let Err(e) = replay_to(t, k, &mut engine) {
        lines.push(format!("  rust state    the replay stopped: {e}"));
        return Err(lines.join("\n"));
    }
    let rust = engine.canonical();
    let fp = engine.observe();
    lines.push(format!(
        "  digest16      python {} rust {}",
        d.fingerprint.digest16, fp.digest16
    ));
    let db = engine
        .state()
        .map_or_else(CardDb::embedded, gcg_engine::GameState::db);
    let diffs = state_diffs(&snap.state, &rust, db);
    if diffs.is_empty() {
        lines.push("  state         identical".to_owned());
    } else {
        lines.push(format!("  state         {} difference(s)", diffs.len()));
        for x in diffs.iter().take(limit) {
            lines.push(format!("    {x}"));
        }
        if diffs.len() > limit {
            lines.push(format!("    ... {} more", diffs.len() - limit));
        }
    }
    let rs_opts = engine.pending().map(|p| p.options).unwrap_or_default();
    match &d.options {
        TraceOptions::Full(py) if *py == rs_opts => {
            lines.push(format!("  options       identical ({})", py.len()));
        }
        TraceOptions::Full(py) => {
            lines.push(format!(
                "  options       differ ({} python, {} rust)",
                py.len(),
                rs_opts.len()
            ));
            lines.extend(
                options_side_by_side(py, &rs_opts)
                    .into_iter()
                    .map(|l| format!("  {l}")),
            );
        }
        TraceOptions::Sha16(h) => {
            let got = crate::options::options_sha16(&rs_opts);
            lines.push(format!(
                "  options       hashed in the trace: python {h} rust {got} ({} rust options)",
                rs_opts.len()
            ));
        }
    }
    Ok(lines)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn card_and_zone_fields_are_named() {
        let db = CardDb::embedded();
        let py = json!({"cards": [[0, 0, 0, 8, 0, 0, -1, 0, 0, 0]], "turn": 3,
                        "zones": [[[], [5]]]});
        let rs = json!({"cards": [[0, 0, 0, 7, 0, 0, -1, 0, 0, 0]], "turn": 4,
                        "zones": [[[], [5, 6]]]});
        let d = state_diffs(&py, &rs, db);
        assert_eq!(d.len(), 3, "{d:?}");
        assert!(d[0].starts_with("cards[0].zone: python 8 ("), "{}", d[0]);
        assert!(d[0].contains("rust 7 ("), "{}", d[0]);
        assert_eq!(d[1], "turn: python 3, rust 4");
        assert!(d[2].starts_with("zones[0]["), "{}", d[2]);
        assert!(d[2].contains("(length): python 1, rust 2"), "{}", d[2]);
    }

    #[test]
    fn options_print_side_by_side() {
        use gcg_engine::{Action, ActionKind};
        let a = Action::new(ActionKind::PlayUnit, 2, -1, 1);
        let b = Action::of(ActionKind::EndMain);
        let lines = options_side_by_side(&[a, b], &[a]);
        assert_eq!(lines.len(), 3);
        assert!(lines[1].starts_with("    0  play_unit(2,1)"), "{lines:?}");
        assert!(lines[2].starts_with("*   1  end_main()"), "{lines:?}");
    }
}
