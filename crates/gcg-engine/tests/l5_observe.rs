//! Layer 5, information sets (`engine/observe.py`; Inv 06b §16): on Python snapshots from
//! setup, mid-game and final states (and two with doctored decklists that make `_fit` trim,
//! pad or fail), `unknown_instances`, `unknown_pools`, `is_hidden_from`, `determinize` (its
//! digest, RNG and version, or its `ValueError`), `determinize_into`, `permute_hidden` and the
//! `information_set_key` equalities equal Python's for both observers and two seeds.

mod l5_common;

use gcg_engine::canon::state_digest;
use gcg_engine::observe::{
    determinize, determinize_into, information_set_key, is_hidden_from, permute_hidden,
    unknown_instances, unknown_pools,
};
use gcg_engine::{GameState, Uid};
use l5_common::{load, records, u64_of};
use serde_json::{Value, json};

fn ids(v: &[gcg_data::DefId]) -> Vec<u16> {
    v.iter().map(|d| d.0).collect()
}

fn observed() -> Vec<(&'static Value, GameState)> {
    records("state")
        .into_iter()
        .filter(|r| !r["observe"].is_null())
        .map(|r| (r, load(&r["snapshot"])))
        .collect()
}

#[test]
fn snapshots_load_to_pythons_digest() {
    for (rec, st) in observed() {
        assert_eq!(
            state_digest(&st),
            rec["digest"].as_str().unwrap(),
            "{}",
            rec["id"]
        );
    }
}

#[test]
fn hidden_cards_and_pools_match_python() {
    let mut cases = 0;
    for (rec, st) in observed() {
        for case in rec["observe"]["cases"].as_array().unwrap() {
            let o = u8::try_from(case["observer"].as_u64().unwrap()).unwrap();
            let at = format!("{} observer {o}", rec["id"]);
            let unknown: Vec<Vec<Uid>> = (0..2).map(|p| unknown_instances(&st, o, p)).collect();
            assert_eq!(json!(unknown), case["unknown"], "{at}: unknown_instances");
            let pools: Vec<Value> = (0..2)
                .map(|p| {
                    let (m, r) = unknown_pools(&st, o, p);
                    json!([ids(&m), ids(&r)])
                })
                .collect();
            assert_eq!(Value::from(pools), case["pools"], "{at}: unknown_pools");
            let hidden: Vec<Uid> = (0..u16::try_from(st.cards().len()).unwrap())
                .filter(|&u| is_hidden_from(&st, u, o))
                .collect();
            assert_eq!(json!(hidden), case["hidden"], "{at}: is_hidden_from");
            cases += 1;
        }
    }
    assert!(cases >= 40, "{cases} cases");
}

#[test]
fn determinize_resamples_with_pythons_rng_order() {
    let (mut worlds, mut errors) = (0, 0);
    for (rec, st) in observed() {
        let before = state_digest(&st);
        let key_before = |o| information_set_key(&st, o);
        for case in rec["observe"]["cases"].as_array().unwrap() {
            let o = u8::try_from(case["observer"].as_u64().unwrap()).unwrap();
            let seed = u64_of(&case["seed"]);
            let at = format!("{} observer {o} seed {seed}", rec["id"]);
            match determinize(&st, o, seed) {
                Ok(w) => {
                    let d = &case["determinize"];
                    assert!(
                        !d.is_null(),
                        "{at}: Python raised {}",
                        case["determinize_error"]
                    );
                    assert_eq!(state_digest(&w), d["digest"].as_str().unwrap(), "{at}");
                    assert_eq!(w.rng_state(), u64_of(&d["rng"]), "{at}: rng");
                    assert_eq!(w.version(), d["version"].as_u64().unwrap(), "{at}");
                    assert_eq!(st.version() + 1, w.version(), "{at}: one touch");
                    assert_eq!(
                        information_set_key(&w, o) == key_before(o),
                        d["key_eq"].as_bool().unwrap(),
                        "{at}: key"
                    );
                    assert_ne!(
                        w.history_epoch(),
                        st.history_epoch(),
                        "{at}: a new history list"
                    );
                    worlds += 1;
                }
                Err(e) => {
                    assert_eq!(e.py_class(), "ValueError", "{at}");
                    assert_eq!(
                        e.to_string(),
                        case["determinize_error"].as_str().unwrap_or_default(),
                        "{at}"
                    );
                    errors += 1;
                }
            }
        }
        assert_eq!(
            state_digest(&st),
            before,
            "{}: the input is unchanged",
            rec["id"]
        );
    }
    assert!(
        worlds >= 40 && errors >= 4,
        "{worlds} worlds, {errors} errors"
    );
}

#[test]
fn determinize_into_a_recycled_state_equals_determinize() {
    let states = observed();
    let mut out = states[0].1.clone();
    for (rec, st) in &states {
        for case in rec["observe"]["cases"].as_array().unwrap() {
            let o = u8::try_from(case["observer"].as_u64().unwrap()).unwrap();
            let seed = u64_of(&case["seed"]);
            let Ok(w) = determinize(st, o, seed) else {
                assert!(determinize_into(st, o, seed, &mut out).is_err());
                continue;
            };
            determinize_into(st, o, seed, &mut out).expect("determinize_into");
            assert_eq!(state_digest(&out), state_digest(&w), "{}", rec["id"]);
            assert_eq!(out.rng_state(), w.rng_state());
            assert_eq!(out.history_epoch(), w.history_epoch());
        }
    }
}

#[test]
fn permute_hidden_shuffles_true_identities_like_python() {
    for (rec, st) in observed() {
        for case in rec["observe"]["cases"].as_array().unwrap() {
            let o = u8::try_from(case["observer"].as_u64().unwrap()).unwrap();
            let seed = u64_of(&case["seed"]);
            let at = format!("{} observer {o} seed {seed}", rec["id"]);
            let p = permute_hidden(&st, o, seed);
            let want = &case["permute"];
            assert_eq!(state_digest(&p), want["digest"].as_str().unwrap(), "{at}");
            assert_eq!(p.rng_state(), u64_of(&want["rng"]), "{at}: rng");
            assert_eq!(
                information_set_key(&p, o) == information_set_key(&st, o),
                want["key_eq"].as_bool().unwrap(),
                "{at}: key"
            );
        }
    }
}

#[test]
fn information_set_keys_differ_between_observers_as_in_python() {
    for (rec, st) in observed() {
        assert_eq!(
            information_set_key(&st, 0) == information_set_key(&st, 1),
            rec["observe"]["keys_equal_across_observers"]
                .as_bool()
                .unwrap(),
            "{}",
            rec["id"]
        );
        assert_eq!(
            information_set_key(&st, 0),
            information_set_key(&st.clone(), 0)
        );
    }
}
