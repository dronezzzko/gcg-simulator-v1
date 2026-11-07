//! Layer 5 tests that run the whole engine (rules management, the derived view, the
//! interpreter, battle): they pass once layers L2-L4 are integrated (PLAN §10.5, M5.5). Every
//! test name starts with `needs_engine_`.
//!
//! - the recorded "always option 0" game (its first 24 decisions and its final state) and
//!   two seeded random games, decision by decision against Python's digests, with
//!   `invariants::check` after every action;
//! - the setup decisions into turn 1, the README scenario, `invariants::check` on mid-game
//!   states, `procedures` against the recorded options, `testkit::install_view`.
//!
//! The native Rust versions of the 12 lead-example tests are in `lead_examples.rs` (T-057).

mod l5_common;

use std::sync::Arc;

use gcg_core::rng::SplitMix64;
use gcg_engine::canon::state_digest;
use gcg_engine::invariants::{check, initial_multiset};
use gcg_engine::testkit::install_view;
use gcg_engine::{
    Action, ActionKind, AddOptions, CheckError, DecisionKind, EndReason, GameState, NewGameOptions,
    Scenario, ScenarioOptions, Step, Zone, apply, legal_actions, procedures, view,
};
use l5_common::{
    action, action_json, int, load, pending_json, record, records, reg, start_game, u64_of,
};
use serde_json::Value;

/// `turn:STEP:kind:Pplayer:n_options:chosen`.
fn decision_line(st: &GameState, chosen: Action) -> String {
    let d = st.pending().expect("a decision");
    format!(
        "{}:{}:{}:P{}:{}:{chosen}",
        st.turn(),
        st.step().py_name(),
        d.hdr.kind.as_str(),
        d.hdr.player,
        d.options.len()
    )
}

#[test]
fn needs_engine_first_option_game_matches_inventory_vectors() {
    const FIRST_24: [&str; 24] = [
        "0:SETUP_CHOOSE_FIRST:choose_first:P1:2:go_first(1)",
        "0:SETUP_REDRAW_P1:redraw:P1:2:keep()",
        "0:SETUP_REDRAW_P2:redraw:P0:2:keep()",
        "1:MAIN:main:P1:1:end_main()",
        "2:MAIN:main:P0:2:play_unit(2,1)",
        "2:MAIN:main:P0:1:end_main()",
        "3:MAIN:main:P1:3:play_unit(53,0)",
        "3:MAIN:main:P1:2:play_unit(62,0)",
        "3:MAIN:main:P1:1:end_main()",
        "4:MAIN:main:P0:3:play_command(12,0)",
        "4:MAIN:select:P0:2:select(53)",
        "4:MAIN:main:P0:2:attack(2,-2)",
        "4:MAIN:main:P0:1:end_main()",
        "5:MAIN:main:P1:6:play_unit(54,0)",
        "5:MAIN:main:P1:4:play_unit(63,0)",
        "5:MAIN:main:P1:3:attack(62,-2)",
        "5:MAIN:main:P1:1:end_main()",
        "6:MAIN:main:P0:5:play_unit(0,0)",
        "6:MAIN:main:P0:5:pair(1,2,0)",
        "6:MAIN:main:P0:3:attack(2,-2)",
        "6:MAIN:main:P0:1:end_main()",
        "7:MAIN:main:P1:14:play_base(50,0)",
        "7:MAIN:main:P1:18:play_unit(51,0)",
        "7:MAIN:main:P1:8:activate(50,765,0)",
    ];
    let rec = record("game", "first-option/rgz-bwf/12345");
    let mut st = start_game(rec, NewGameOptions::default()).expect("new_game");
    let mut lines = Vec::new();
    while let Some(d) = st.pending() {
        let a = d.options[0];
        lines.push(decision_line(&st, a));
        apply(&mut st, a, true).expect("apply");
    }
    assert_eq!(lines[..24], FIRST_24);
    assert_eq!(lines.len(), 69);
    assert_eq!(st.winner(), Some(1));
    assert_eq!(st.end_reason(), Some(EndReason::BattleDamage));
    assert_eq!((st.turn(), st.active(), st.action_count()), (11, 1, 69));
    assert_eq!((st.version(), st.event_group()), (357, 123));
    assert_eq!(st.rng_state(), 7_774_064_181_650_859_138);
    assert_eq!(
        state_digest(&st),
        "b8f32aad892e6eb969a4fd17a9c9242c6bfc5c732f67669299af59bb7072cf65"
    );
}

#[test]
fn needs_engine_games_replay_decision_by_decision() {
    for rec in records("game") {
        let id = rec["id"].as_str().unwrap();
        let mut st = start_game(rec, NewGameOptions::default()).expect("new_game");
        let initial = initial_multiset(&st);
        let mut rng = rec["policy_seed"]
            .as_str()
            .map(|s| SplitMix64::new(s.parse().unwrap()));
        for (i, d) in rec["decisions"].as_array().unwrap().iter().enumerate() {
            let at = format!("{id} decision {i}");
            assert_eq!(
                &state_digest(&st)[..16],
                d["digest16"].as_str().unwrap(),
                "{at}"
            );
            let dec = st.pending().expect("a decision");
            assert_eq!(i64::from(st.turn()), int(&d["turn"]), "{at}");
            assert_eq!(i64::from(st.step().value()), int(&d["step"]), "{at}");
            assert_eq!(dec.hdr.kind.as_str(), d["kind"].as_str().unwrap(), "{at}");
            assert_eq!(i64::from(dec.hdr.player), int(&d["player"]), "{at}");
            let n = dec.options.len();
            assert_eq!(n as i64, int(&d["n"]), "{at}");
            let a = match &mut rng {
                None => dec.options[0],
                Some(r) => dec.options[r.randrange_usize(n)],
            };
            assert_eq!(action_json(&a), d["chosen"], "{at}");
            apply(&mut st, a, true).unwrap_or_else(|e| panic!("{at}: {e}"));
            check(&st, Some(&initial)).unwrap_or_else(|e| panic!("{at}: {e}"));
        }
        let f = &rec["final"];
        assert_eq!(state_digest(&st), f["digest"].as_str().unwrap(), "{id}");
        assert_eq!(st.winner().map(i64::from), f["winner"].as_i64(), "{id}");
        assert_eq!(
            st.end_reason().map(|e| e.as_str()),
            f["end_reason"].as_str(),
            "{id}"
        );
        assert_eq!(st.rng_state(), u64_of(&f["rng"]), "{id}");
        assert!(legal_actions(&st).is_empty());
    }
}

#[test]
fn needs_engine_setup_into_turn_one() {
    for rec in records("new_game") {
        let id = rec["id"].as_str().unwrap();
        let chooser = rec["chooser"].as_u64().map(|c| u8::try_from(c).unwrap());
        let o = NewGameOptions {
            chooser,
            ..NewGameOptions::default()
        };
        let mut st = start_game(rec, o).expect("new_game");
        for (k, s) in rec["steps"].as_array().unwrap().iter().enumerate() {
            apply(&mut st, action(&s["apply"]), true).expect("a setup decision");
            assert_eq!(
                state_digest(&st),
                s["digest"].as_str().unwrap(),
                "{id} step {k}"
            );
            assert_eq!(
                st.version(),
                s["version"].as_u64().unwrap(),
                "{id} step {k}"
            );
            assert_eq!(pending_json(&st), s["pending"], "{id} step {k}");
        }
        assert_eq!(st.step(), Step::Main, "{id}");
    }
}

#[test]
fn needs_engine_readme_scenario_starts_in_main() {
    let rec = record("scenario_start", "readme-example");
    let mut sc = Scenario::new(&reg(), ScenarioOptions::default());
    let gundam = sc
        .add(0, "GD01-001", Zone::Hand, AddOptions::default())
        .unwrap();
    assert_eq!(i64::from(gundam), int(&rec["hand"]));
    sc.resources(0, 5, 0, 0).unwrap();
    sc.start(Step::Main).unwrap();
    let st = sc.state();
    assert_eq!(state_digest(st), rec["after"].as_str().unwrap());
    let opts: Vec<Value> = legal_actions(st).iter().map(action_json).collect();
    assert_eq!(Value::from(opts), rec["options"]);
}

#[test]
fn needs_engine_invariants_on_mid_game_states() {
    let all = records("state");
    let by_id = |id: &str| all.iter().find(|r| r["id"] == id).expect("a state");
    let mut checked = 0;
    for rec in &all {
        let inv = &rec["invariants"];
        if !inv["needs_view"].as_bool().unwrap() {
            continue;
        }
        let id = rec["id"].as_str().unwrap();
        let game = id.rsplit_once('/').unwrap().0;
        let base = load(&by_id(&format!("{game}/d0"))["snapshot"]);
        let st = load(&rec["snapshot"]);
        let got = check(&st, Some(&initial_multiset(&base)));
        match inv["error"].as_str() {
            None => assert_eq!(got, Ok(()), "{id}"),
            Some(msg) => {
                let Err(CheckError::Violation(v)) = got else {
                    panic!("{id}: {got:?}");
                };
                assert_eq!(v.to_string(), msg, "{id}");
            }
        }
        checked += 1;
    }
    assert!(checked >= 10, "{checked}");
}

#[test]
fn needs_engine_procedures_reproduce_the_recorded_options() {
    let mut checked = 0;
    for rec in records("state") {
        let st = load(&rec["snapshot"]);
        let Some(d) = st.pending() else { continue };
        if !st.frames().is_empty() {
            continue;
        }
        match d.hdr.kind {
            DecisionKind::Main => {
                assert_eq!(
                    procedures::main_options(&st).unwrap(),
                    d.options,
                    "{}",
                    rec["id"]
                );
                checked += 1;
            }
            DecisionKind::ActionStep => {
                let mut opts = procedures::action_step_options(&st, d.hdr.player).unwrap();
                opts.push(Action::of(ActionKind::Pass));
                assert_eq!(opts, d.options, "{}", rec["id"]);
                checked += 1;
            }
            _ => {}
        }
    }
    assert!(checked >= 5, "{checked}");
}

#[test]
fn needs_engine_install_view_reuses_the_view_while_the_key_holds() {
    let rec = records("state")
        .into_iter()
        .find(|r| r["invariants"]["needs_view"].as_bool().unwrap())
        .unwrap();
    let st = load(&rec["snapshot"]);
    let from = st.clone();
    let key = view::derived(&from).unwrap().key();
    install_view(&st, &from, key).unwrap();
    assert!(Arc::ptr_eq(
        &view::derived(&st).unwrap(),
        &view::derived(&from).unwrap()
    ));
    let mut other = key;
    other.turn += 1;
    assert!(install_view(&st, &from, other).is_err());
}
