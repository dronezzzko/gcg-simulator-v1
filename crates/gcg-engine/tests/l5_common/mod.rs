//! Shared helpers of the engine's integration tests: the Python vectors of
//! `tests/fixtures/l5_vectors.ndjson` (`tests/fixtures/export_l5_vectors.py`) and
//! `tests/fixtures/engine_vectors.ndjson` (`port/oracle/export_engine_vectors.py`), decks by
//! card number, snapshots, decision JSON and the replay of Scenario operations.

#![allow(dead_code, reason = "each test target uses a different part")]

use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use gcg_effects::Registry;
use gcg_engine::canon::from_canonical;
use gcg_engine::{
    Action, ActionKind, AddOptions, GameState, NewGameError, NewGameOptions, Scenario,
    ScenarioOptions, StateSnapshot, Step, TestkitError, Zone, new_game_by_number,
};
use serde_json::{Value, json};

pub fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn ndjson(name: &str) -> Vec<Value> {
    fs::read_to_string(crate_dir().join("tests/fixtures").join(name))
        .unwrap_or_else(|e| panic!("{name}: {e}"))
        .lines()
        .map(|l| serde_json::from_str(l).expect("a JSON record"))
        .collect()
}

pub fn vectors() -> &'static [Value] {
    static V: OnceLock<Vec<Value>> = OnceLock::new();
    V.get_or_init(|| ndjson("l5_vectors.ndjson"))
}

/// The records of `tests/fixtures/engine_vectors.ndjson` of type `t`.
pub fn engine_records(t: &str) -> Vec<&'static Value> {
    static V: OnceLock<Vec<Value>> = OnceLock::new();
    let all = V.get_or_init(|| ndjson("engine_vectors.ndjson"));
    let out: Vec<&Value> = all.iter().filter(|r| r["t"] == t).collect();
    assert!(!out.is_empty(), "no {t} records in engine_vectors.ndjson");
    out
}

fn player(v: &Value) -> u8 {
    u8::try_from(v.as_u64().expect("a player")).expect("a player")
}

fn zone(v: &Value) -> Zone {
    Zone::from_value(int(v)).expect("a zone")
}

fn count(v: &Value) -> usize {
    usize::try_from(v.as_u64().expect("a count")).expect("a count")
}

/// `Scenario(**init)` (`testkit.py:38-49`) with Python's defaults for absent keys.
pub fn scenario_options(init: &Value) -> ScenarioOptions {
    let d = ScenarioOptions::default();
    ScenarioOptions {
        active: init.get("active").map_or(d.active, player),
        turn: init
            .get("turn")
            .map_or(d.turn, |v| i32::try_from(int(v)).expect("a turn")),
        seed: init.get("seed").map_or(d.seed, u64_of),
        deck_size: init.get("deck_size").map_or(d.deck_size, count),
    }
}

/// Runs one Scenario operation of a vector record (the op lists of both exporters); the
/// result as Python's JSON.
///
/// # Errors
/// What the testkit raised.
pub fn run_op(sc: &mut Scenario, op: &[Value]) -> Result<Value, TestkitError> {
    let name = op[0].as_str().expect("an op name");
    let a = &op[1..];
    let step = |v: &Value| Step::from_value(int(v)).expect("a step");
    Ok(match name {
        "prepare_start" => {
            sc.prepare_start(step(&a[0]))?;
            Value::Null
        }
        "start" => {
            sc.start(step(&a[0]))?;
            Value::Null
        }
        "add" => {
            let kw = &a[3];
            let flag = |k: &str, d: bool| kw.get(k).map_or(d, |v| v.as_bool().expect("a bool"));
            let o = AddOptions {
                rested: flag("rested", false),
                damage: kw
                    .get("damage")
                    .map_or(0, |v| i32::try_from(int(v)).expect("a damage")),
                pilot: kw.get("pilot").map(|v| v.as_str().expect("a number")),
                deployed_this_turn: flag("deployed_this_turn", false),
                known: flag("known", true),
            };
            json!(sc.add(
                player(&a[0]),
                a[1].as_str().expect("a number"),
                zone(&a[2]),
                o
            )?)
        }
        "hand" => json!(sc.hand(player(&a[0]), &strs(&a[1]))?),
        "trash" => json!(sc.trash(player(&a[0]), &strs(&a[1]))?),
        "shields" => json!(sc.shields(player(&a[0]), &strs(&a[1]))?),
        "deck" => {
            sc.deck(player(&a[0]), &strs(&a[1]));
            Value::Null
        }
        "resources" => {
            json!(sc.resources(player(&a[0]), count(&a[1]), count(&a[2]), count(&a[3]))?)
        }
        "base" => {
            let damage = i32::try_from(int(&a[2])).expect("a damage");
            json!(sc.base(player(&a[0]), a[1].as_str(), damage)?)
        }
        "resource_deck" => {
            sc.resource_deck(player(&a[0]), count(&a[1]))?;
            Value::Null
        }
        "add_raw" => {
            json!(sc.add_raw(player(&a[0]), a[1].as_str().expect("a number"), zone(&a[2]))?)
        }
        other => panic!("unknown scenario op {other}"),
    })
}

/// The Scenario of a record's `init` after its `ops`, each op returning what Python's did.
pub fn replay_scenario(rec: &Value) -> Scenario {
    let id = &rec["id"];
    let mut sc = Scenario::new(&reg(), scenario_options(&rec["init"]));
    let ops = rec["ops"].as_array().expect("ops");
    let results = rec["results"].as_array().expect("results");
    assert_eq!(ops.len(), results.len(), "{id}");
    for (op, want) in ops.iter().zip(results) {
        let op = op.as_array().expect("an op");
        let got = run_op(&mut sc, op).unwrap_or_else(|e| panic!("{id}: {op:?}: {e:?}"));
        assert_eq!(&got, want, "{id}: {op:?}");
    }
    sc
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
