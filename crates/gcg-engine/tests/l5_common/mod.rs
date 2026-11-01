//! Shared helpers of the layer-5 tests: the Python vectors of
//! `tests/fixtures/l5_vectors.ndjson` (`tests/fixtures/export_l5_vectors.py`), decks by card
//! number, snapshots and decision JSON.

#![allow(dead_code, reason = "each test target uses a different part")]

use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use gcg_effects::Registry;
use gcg_engine::canon::from_canonical;
use gcg_engine::{
    Action, ActionKind, GameState, NewGameError, NewGameOptions, StateSnapshot, new_game_by_number,
};
use serde_json::{Value, json};

pub fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

pub fn vectors() -> &'static [Value] {
    static V: OnceLock<Vec<Value>> = OnceLock::new();
    V.get_or_init(|| {
        fs::read_to_string(crate_dir().join("tests/fixtures/l5_vectors.ndjson"))
            .expect("the L5 vector file")
            .lines()
            .map(|l| serde_json::from_str(l).expect("a JSON record"))
            .collect()
    })
}

pub fn records(t: &str) -> Vec<&'static Value> {
    let out: Vec<&Value> = vectors().iter().filter(|r| r["t"] == t).collect();
    assert!(!out.is_empty(), "no {t} records in the vector file");
    out
}

pub fn record(t: &str, id: &str) -> &'static Value {
    vectors()
        .iter()
        .find(|r| r["t"] == t && r["id"] == id)
        .unwrap_or_else(|| panic!("no {t} record {id}"))
}

pub fn reg() -> Arc<Registry> {
    Registry::embedded()
}

pub fn strs(v: &Value) -> Vec<&str> {
    v.as_array()
        .expect("a list")
        .iter()
        .map(|x| x.as_str().expect("a string"))
        .collect()
}

pub fn u64_of(v: &Value) -> u64 {
    match v {
        Value::String(s) => s.parse().expect("a u64 string"),
        other => other.as_u64().expect("a u64"),
    }
}

pub fn int(v: &Value) -> i64 {
    v.as_i64().expect("an int")
}

/// The two decks of a record as `(main, resources)` card-number lists.
pub fn deck_numbers(rec: &Value) -> [(Vec<&str>, Vec<&str>); 2] {
    let d = |p: usize| {
        (
            strs(&rec["decks"][p]["main"]),
            strs(&rec["decks"][p]["res"]),
        )
    };
    [d(0), d(1)]
}

/// `new_game(decks, seed, chooser=...)` on the record's card numbers.
pub fn start_game(rec: &Value, opts: NewGameOptions) -> Result<GameState, NewGameError> {
    let [(m0, r0), (m1, r1)] = deck_numbers(rec);
    new_game_by_number(&reg(), [(&m0, &r0), (&m1, &r1)], u64_of(&rec["seed"]), opts)
}

pub fn snapshot(v: &Value) -> StateSnapshot {
    StateSnapshot {
        state: v["state"].clone(),
        continuous: v["continuous"].clone(),
        filter_sets: v["filter_sets"].clone(),
        rules_checked: int(&v["rules_checked"]),
    }
}

/// The state of a `state` or `scenario` record's snapshot.
pub fn load(v: &Value) -> GameState {
    from_canonical(&reg(), &snapshot(v)).expect("the snapshot loads")
}

/// `Action.from_json([kind, a, b, c])` (`engine/state.py:26-28`).
pub fn action(v: &Value) -> Action {
    let a = v.as_array().expect("an action list");
    let kind = ActionKind::from_value(a[0].as_str().expect("a kind")).expect("a known kind");
    let n = |i: usize| i32::try_from(int(&a[i])).expect("an i32");
    Action::new(kind, n(1), n(2), n(3))
}

pub fn action_json(a: &Action) -> Value {
    json!([a.kind.as_str(), a.a, a.b, a.c])
}

/// `st.pending.to_json()` (`engine/state.py:51-58`), or `null`.
pub fn pending_json(st: &GameState) -> Value {
    match st.pending() {
        None => Value::Null,
        Some(d) => json!({
            "player": d.hdr.player,
            "kind": d.hdr.kind.as_str(),
            "options": d.options.iter().map(action_json).collect::<Vec<_>>(),
            "prompt": d.hdr.prompt.text(st.registry()),
            "context": d.hdr.context.as_slice().iter().map(|(k, v)| json!([k.as_str(), v])).collect::<Vec<_>>(),
        }),
    }
}
