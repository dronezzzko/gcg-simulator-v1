//! The M5.2 gate (PLAN §10.4 B2, §13): every trace of the `views` set
//! (`oracle/traces.py` `generate_view`) loads its snapshot with `canon::from_canonical`,
//! computes `view::derived`, and reproduces Python's records: the view signature
//! (`tests/paranoid.py:24-39` as `view_signature_json`), the per-card queries of
//! `views.cards` (`ap_of`, `hp_of`, `keywords`, `traits_of`, `play_cost`, `play_level` for
//! every card outside DECK, RESOURCE_DECK and OUTSIDE) and the trigger index
//! (`V.trigger_index`). The NDJSON is parsed with `serde_json` directly (the engine cannot
//! depend on `gcg-diff`).
//!
//! Tier 1 (committed, 280 traces) always runs. Tier 2 (2034 traces, regenerated into
//! `build/oracle/tier2/` by `uv run python oracle/traces.py --write --tier 2 --only
//! views` and checked against `oracle/corpus/v1/MANIFEST-tier2.json`) runs with the `slow`
//! feature (PLAN A12).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gcg_core::pyjson::{DumpOptions, Json, dumps};
use gcg_effects::{Ev, Kw, Registry};
use gcg_engine::canon::from_canonical;
use gcg_engine::view::{
    ap_of, derived, hp_of, keywords, play_cost, play_level, signature, traits_of, trigger_index,
};
use gcg_engine::{GameState, StateSnapshot, Uid, Zone};
use serde_json::{Map, Value};

fn repo() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

fn to_value(j: &Json) -> Value {
    serde_json::from_str(&dumps(j, DumpOptions::COMPACT_SORTED_ASCII)).expect("our JSON parses")
}

/// The first path where two JSON values differ (objects compare as dicts).
fn first_difference(want: &Value, got: &Value, path: &str) -> Option<String> {
    match (want, got) {
        (Value::Object(a), Value::Object(b)) => {
            let mut keys: Vec<&String> = a.keys().chain(b.keys()).collect();
            keys.sort();
            keys.dedup();
            keys.into_iter().find_map(|k| match (a.get(k), b.get(k)) {
                (Some(x), Some(y)) => first_difference(x, y, &format!("{path}.{k}")),
                (x, y) => Some(format!("{path}.{k}: python {x:?}, rust {y:?}")),
            })
        }
        (Value::Array(a), Value::Array(b)) => a
            .iter()
            .zip(b)
            .enumerate()
            .find_map(|(i, (x, y))| first_difference(x, y, &format!("{path}[{i}]")))
            .or_else(|| {
                (a.len() != b.len())
                    .then(|| format!("{path}: python length {}, rust {}", a.len(), b.len()))
            }),
        _ => (want != got).then(|| format!("{path}: python {want}, rust {got}")),
    }
}

/// One `views` trace: its id and its records after the header.
struct Trace {
    id: String,
    records: Vec<Value>,
}

fn traces_in(dir: &Path) -> Vec<Trace> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|e| e.expect("a directory entry").path())
        .filter(|p| p.extension().is_some_and(|x| x == "ndjson"))
        .collect();
    files.sort();
    let mut out: Vec<Trace> = Vec::new();
    for f in files {
        let text = fs::read_to_string(&f).expect("an NDJSON file");
        for line in text.lines() {
            let rec: Value = serde_json::from_str(line).expect("a JSON record");
            if rec["t"] == "header" {
                out.push(Trace {
                    id: rec["id"].as_str().expect("a trace id").to_owned(),
                    records: Vec::new(),
                });
            } else {
                out.last_mut().expect("a header first").records.push(rec);
            }
        }
    }
    out
}

fn record<'a>(t: &'a Trace, kind: &str, func: Option<&str>) -> &'a Value {
    t.records
        .iter()
        .find(|r| r["t"] == kind && func.is_none_or(|f| r["fn"] == f))
        .unwrap_or_else(|| panic!("{}: no {kind} {func:?} record", t.id))
}

fn kw_json(k: gcg_effects::KwAmounts) -> Value {
    let mut m = Map::new();
    for kw in Kw::ALL {
        if let Some(n) = k.get(kw) {
            m.insert(kw.as_str().to_owned(), Value::from(n));
        }
    }
    Value::Object(m)
}

/// `views.cards` (`oracle/traces.py` `generate_view`).
fn cards_probe(st: &GameState, reg: &Registry) -> Result<Value, String> {
    let dv = derived(st).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for (i, c) in st.cards().iter().enumerate() {
        if matches!(c.zone, Zone::Deck | Zone::ResourceDeck | Zone::Outside) {
            continue;
        }
        let uid = Uid::try_from(i).expect("a uid");
        let traits: Vec<Value> = traits_of(st, &dv, uid).iter(reg).map(Value::from).collect();
        out.push(Value::Array(vec![
            Value::from(uid),
            Value::from(ap_of(st, &dv, uid)),
            Value::from(hp_of(st, &dv, uid)),
            kw_json(keywords(&dv, uid)),
            Value::Array(traits),
            Value::from(play_cost(st, &dv, uid).map_err(|e| e.to_string())?),
            Value::from(play_level(st, &dv, uid)),
        ]));
    }
    Ok(Value::Array(out))
}

/// `V.trigger_index(st, V.derived(st))` as `{ev.value: [[uid, aid], ...]}`.
fn index_probe(st: &GameState) -> Result<Value, String> {
    let dv = derived(st).map_err(|e| e.to_string())?;
    let idx = trigger_index(st, &dv);
    let mut m = Map::new();
    for ev in Ev::ALL {
        let hosts = idx.get(ev);
        if !hosts.is_empty() {
            let list = hosts
                .iter()
                .map(|&(u, a)| Value::Array(vec![Value::from(u), Value::from(a.0)]))
                .collect();
            m.insert(ev.as_str().to_owned(), Value::Array(list));
        }
    }
    Ok(Value::Object(m))
}

/// Replays one trace in `generate_view`'s order (snapshot, `derived`, signature, cards,
/// trigger index) and returns the first difference.
fn replay(reg: &Arc<Registry>, t: &Trace) -> Result<(), String> {
    let snap = record(t, "snapshot", None);
    let snapshot = StateSnapshot {
        state: snap["state"].clone(),
        continuous: snap["continuous"].clone(),
        filter_sets: snap["filter_sets"].clone(),
        rules_checked: snap["rules_checked"].as_i64().expect("rules_checked"),
    };
    let st = from_canonical(reg, &snapshot).map_err(|e| format!("from_canonical: {e}"))?;
    let dv = derived(&st).map_err(|e| format!("derived: {e}"))?;
    let sig = to_value(&signature(&st, &dv));
    if let Some(d) = first_difference(&record(t, "view", None)["sig"], &sig, "sig") {
        return Err(d);
    }
    let cards = cards_probe(&st, reg)?;
    let want = &record(t, "probe", Some("views.cards"))["result"];
    if let Some(d) = first_difference(want, &cards, "views.cards") {
        return Err(d);
    }
    let index = index_probe(&st)?;
    let want = &record(t, "probe", Some("V.trigger_index"))["result"];
    if let Some(d) = first_difference(want, &index, "V.trigger_index") {
        return Err(d);
    }
    Ok(())
}

/// Replays every trace of `dir`; panics with the failures unless all `expected` pass.
fn check_set(dir: &Path, expected: usize) {
    let reg = Registry::embedded();
    let traces = traces_in(dir);
    let failures: Vec<String> = traces
        .iter()
        .filter_map(|t| replay(&reg, t).err().map(|e| format!("{}: {e}", t.id)))
        .collect();
    let passed = traces.len() - failures.len();
    eprintln!(
        "views {}: {passed}/{} traces pass",
        dir.display(),
        traces.len()
    );
    assert!(
        failures.is_empty(),
        "{} of {} views traces differ:\n{}",
        failures.len(),
        traces.len(),
        failures.join("\n")
    );
    assert_eq!(traces.len(), expected, "trace count of {}", dir.display());
}

#[test]
fn tier1_views_match_python() {
    check_set(&repo().join("oracle/corpus/v1/tier1/views"), 300);
}

/// The tier-2 trace count of the `views` set in `oracle/corpus/v1/MANIFEST-tier2.json`.
#[cfg(feature = "slow")]
fn tier2_expected() -> usize {
    let text = fs::read_to_string(repo().join("oracle/corpus/v1/MANIFEST-tier2.json"))
        .expect("the tier-2 manifest");
    let m: Value = serde_json::from_str(&text).expect("JSON");
    m["files"]
        .as_array()
        .expect("files")
        .iter()
        .filter(|f| f["set"] == "views")
        .map(|f| usize::try_from(f["traces"].as_u64().expect("traces")).expect("a count"))
        .sum()
}

/// Every tier-2 `views` file on disk is the one the manifest names (SHA-256 and size).
#[cfg(feature = "slow")]
fn check_tier2_files() {
    use sha2::{Digest, Sha256};
    let text = fs::read_to_string(repo().join("oracle/corpus/v1/MANIFEST-tier2.json"))
        .expect("the tier-2 manifest");
    let m: Value = serde_json::from_str(&text).expect("JSON");
    for f in m["files"].as_array().expect("files") {
        if f["set"] != "views" {
            continue;
        }
        let path = f["path"].as_str().expect("a path");
        let bytes = fs::read(repo().join("build/oracle/tier2").join(path))
            .unwrap_or_else(|e| panic!("{path}: {e} (regenerate the tier-2 views set)"));
        let sha: String = Sha256::digest(&bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(
            Value::from(sha),
            f["sha256"],
            "{path}: not the manifest's file"
        );
        assert_eq!(Value::from(bytes.len()), f["bytes"], "{path}: size");
    }
}

#[cfg(feature = "slow")]
#[test]
fn tier2_views_match_python() {
    let expected = tier2_expected();
    assert_eq!(expected, 2034);
    check_tier2_files();
    check_set(&repo().join("build/oracle/tier2/views"), expected);
}
