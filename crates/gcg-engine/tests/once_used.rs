//! T-150: `st.once_used` is a Python set (`engine/state.py:497`; PLAN §4.3, §8.7 control 7).
//! Adding a key that is present changes nothing, adding never touches, and the canonical form
//! lists each key once, sorted by its `json.dumps` text (`runner/digest.py:48`):
//!
//! - Python's vectors (`tests/fixtures/engine_vectors.ndjson`, `once_used` records): keys of
//!   every int-only form added in three orders, duplicates included, give Python's canonical
//!   list, version and digest;
//! - a property test against a set model, with the canonical order computed from the
//!   `json.dumps` texts and a `from_canonical` round trip;
//! - the duplicated-key control: a snapshot that lists a key twice is rejected, since no
//!   Python state can serialise one;
//! - the set empties at the turn change (`game.py:335`).

mod l5_common;

use std::collections::BTreeSet;

use gcg_core::prop;
use gcg_core::pyjson::{DumpOptions, dumps};
use gcg_engine::canon::{canonical_state, from_canonical, state_digest};
use gcg_engine::testkit::engine::once_add;
use gcg_engine::testkit::{snapshot, to_next_turn};
use gcg_engine::{CanonError, GameState, OnceElem, Step};
use l5_common::{engine_records, replay_scenario};
use serde_json::Value;

const COMPACT: DumpOptions = DumpOptions {
    indent: None,
    sort_keys: false,
    compact: true,
    ensure_ascii: true,
};

fn canonical_json(st: &GameState) -> Value {
    serde_json::from_str(&dumps(&canonical_state(st), COMPACT)).expect("JSON")
}

fn ints(v: &Value) -> Vec<i32> {
    v.as_array()
        .expect("a key")
        .iter()
        .map(|x| i32::try_from(x.as_i64().expect("an int")).expect("an i32"))
        .collect()
}

/// `json.dumps(list(key))` with Python's default separators.
fn dumps_key(k: &[i32]) -> String {
    let parts: Vec<String> = k.iter().map(i32::to_string).collect();
    format!("[{}]", parts.join(", "))
}

/// The keys of `st.once_used` as int lists, in the state's order.
fn keys_of(st: &GameState) -> Vec<Vec<i32>> {
    st.once_used()
        .iter()
        .map(|k| {
            k.0.as_slice()
                .iter()
                .map(|e| match e {
                    OnceElem::Int(x) => *x,
                    OnceElem::Cont(c) => panic!("a continuous element {c:?}"),
                })
                .collect()
        })
        .collect()
}

/// A Scenario at its first MAIN decision with two Units (the `once_used` records' ops).
fn started() -> GameState {
    replay_scenario(engine_records("once_used")[0]).into_state()
}

#[test]
fn adding_keys_gives_pythons_canonical_set() {
    let mut digests = BTreeSet::new();
    for rec in engine_records("once_used") {
        let id = rec["id"].as_str().unwrap();
        let mut st = replay_scenario(rec).into_state();
        let version = st.version();
        for k in rec["keys"].as_array().unwrap() {
            once_add(&mut st, &ints(k)).unwrap();
        }
        assert_eq!(
            st.version(),
            version,
            "{id}: adding a once key never touches"
        );
        assert_eq!(st.version(), rec["version"].as_u64().unwrap(), "{id}");
        assert_eq!(canonical_json(&st)["once_used"], rec["canonical"], "{id}");
        let distinct: BTreeSet<Vec<i32>> =
            rec["keys"].as_array().unwrap().iter().map(ints).collect();
        assert_eq!(st.once_used().len(), distinct.len(), "{id}: each key once");
        let digest = state_digest(&st);
        assert_eq!(digest, rec["digest"].as_str().unwrap(), "{id}");
        if distinct.len() > 1 {
            digests.insert(digest);
        }
    }
    assert_eq!(digests.len(), 1, "one key set in two orders is one state");
}

#[test]
fn once_add_is_a_set_add_against_a_model() {
    let base = started();
    prop::check("once-used-set", 200, |rng| {
        let mut st = base.clone();
        let version = st.version();
        let mut model: BTreeSet<Vec<i32>> = BTreeSet::new();
        for _ in 0..rng.randrange(40) {
            // Small domains, so that keys repeat; both key lengths the engine builds.
            let len = if rng.randrange(3) == 0 { 5 } else { 3 };
            let key: Vec<i32> = (0..len)
                .map(|_| i32::try_from(rng.randrange(25)).unwrap() - 12)
                .collect();
            if key.len() >= 3 && (key[0] == -7 || key[0] == -8) && key[1] == -1 {
                continue; // the lasting rule-key form, whose third element is an effect
            }
            once_add(&mut st, &key).map_err(|e| e.to_string())?;
            model.insert(key);
            let keys = keys_of(&st);
            let as_set: BTreeSet<Vec<i32>> = keys.iter().cloned().collect();
            if keys.len() != as_set.len() || as_set != model {
                return Err(format!("{keys:?} is not the set {model:?}"));
            }
        }
        if st.version() != version {
            return Err("adding once keys touched the state".into());
        }
        let mut want: Vec<&Vec<i32>> = model.iter().collect();
        want.sort_by_key(|k| dumps_key(k));
        let got: Vec<Vec<i32>> = canonical_json(&st)["once_used"]
            .as_array()
            .unwrap()
            .iter()
            .map(ints)
            .collect();
        if got.iter().collect::<Vec<_>>() != want {
            return Err(format!(
                "canonical order {got:?}, json.dumps order {want:?}"
            ));
        }
        let back = from_canonical(st.registry(), &snapshot(&st)).map_err(|e| e.to_string())?;
        let round: BTreeSet<Vec<i32>> = keys_of(&back).into_iter().collect();
        if round != model || state_digest(&back) != state_digest(&st) {
            return Err("the once keys do not survive a round trip".into());
        }
        Ok(())
    });
}

#[test]
fn a_snapshot_listing_a_key_twice_is_rejected() {
    let rec = engine_records("once_used")
        .into_iter()
        .find(|r| r["id"] == "in-order")
        .unwrap();
    let mut st = replay_scenario(rec).into_state();
    for k in rec["keys"].as_array().unwrap() {
        once_add(&mut st, &ints(k)).unwrap();
    }
    let mut snap = snapshot(&st);
    assert_eq!(
        state_digest(&from_canonical(st.registry(), &snap).unwrap()),
        state_digest(&st)
    );
    let list = snap.state["once_used"].as_array_mut().unwrap();
    let n = list.len();
    let dup = list[3].clone();
    list.push(dup);
    match from_canonical(st.registry(), &snap) {
        Err(CanonError::Value { path, detail }) => {
            assert_eq!(path, format!("$.once_used[{n}]"));
            assert!(detail.contains("listed twice"), "{detail}");
        }
        Err(other) => panic!("the wrong error for a duplicated once key: {other:?}"),
        Ok(_) => panic!("a duplicated once key was accepted"),
    }
}

#[test]
fn the_set_empties_at_the_turn_change() {
    let mut st = started();
    for k in [[12, 0, 1], [-9, 1, 3]] {
        once_add(&mut st, &k).unwrap();
    }
    assert_eq!(st.once_used().len(), 2);
    to_next_turn(&mut st).unwrap();
    assert_eq!(st.step(), Step::Main);
    assert!(st.once_used().is_empty(), "{:?}", st.once_used());
}
