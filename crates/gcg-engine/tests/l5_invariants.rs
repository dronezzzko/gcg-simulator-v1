//! Layer 5, invariants (`engine/invariants.py`): `check` against the initial
//! multiset of each state's game gives Python's result on every Python snapshot whose check
//! needs no derived view (setup states, states with frames, finished games), including states
//! broken on purpose in each invariant family (damage, zone lists, pairing, the card multiset,
//! the pending decision), with Python's exact messages. The mid-game states whose lethal-damage
//! check reads the view are in `l5_needs_engine.rs`.

mod l5_common;

use std::collections::BTreeMap;

use gcg_engine::invariants::{check, initial_multiset};
use gcg_engine::{CheckError, GameState};
use l5_common::{load, records};
use serde_json::Value;

/// The id of the state whose initial multiset the record's check used: the game's first
/// decision, or the setup state the broken and pool states were derived from.
fn base_id(id: &str) -> String {
    match id.rsplit_once('/') {
        Some((game, _)) if !id.starts_with("broken/") && !id.starts_with("pools/") => {
            format!("{game}/d0")
        }
        _ => "broken/base".to_owned(),
    }
}

fn states() -> BTreeMap<String, (&'static Value, GameState)> {
    records("state")
        .into_iter()
        .map(|r| {
            (
                r["id"].as_str().unwrap().to_owned(),
                (r, load(&r["snapshot"])),
            )
        })
        .collect()
}

#[test]
fn check_matches_python_where_no_view_is_needed() {
    let states = states();
    let mut checked = 0;
    let mut violations = 0;
    for (id, (rec, st)) in &states {
        let inv = &rec["invariants"];
        if inv["needs_view"].as_bool().unwrap() {
            continue;
        }
        let base = &states[&base_id(id)].1;
        let initial = initial_multiset(base);
        let got = check(st, Some(&initial));
        match inv["error"].as_str() {
            None => assert_eq!(got, Ok(()), "{id}"),
            Some(msg) => {
                let Err(CheckError::Violation(v)) = got else {
                    panic!("{id}: expected a violation, got {got:?}");
                };
                assert_eq!(v.to_string(), msg, "{id}");
                violations += 1;
            }
        }
        checked += 1;
    }
    assert!(
        checked >= 20 && violations == 5,
        "{checked} checked, {violations} violations"
    );
}

#[test]
fn every_game_keeps_its_initial_multiset() {
    let states = states();
    for (id, (_, st)) in &states {
        if id.starts_with("broken/") || id.starts_with("pools/") {
            continue;
        }
        let base = &states[&base_id(id)].1;
        assert_eq!(initial_multiset(st), initial_multiset(base), "{id}");
    }
    let (_, broken) = &states["broken/multiset"];
    assert_ne!(
        initial_multiset(broken),
        initial_multiset(&states["broken/base"].1)
    );
}

#[test]
fn violations_display_at_most_ten_messages() {
    let states = states();
    let (_, st) = &states["broken/zones"];
    let Err(CheckError::Violation(v)) = check(st, None) else {
        panic!("broken/zones has violations");
    };
    let mut many = v.clone();
    for i in 0..12 {
        many.errors.push(format!("extra {i}"));
    }
    let shown = many.to_string();
    assert_eq!(shown.split("; ").count(), 10);
    assert!(shown.starts_with(&v.to_string()));
}
