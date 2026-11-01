//! Layer 5, testkit (`testkit.py`): `Scenario` builds replayed from Python's operations give
//! Python's uids and the digest before `Scenario.start`'s `advance` (`Scenario::prepare_start`,
//! open item O14) and after later additions; the helpers' assertion texts, `has_action`,
//! `legal_kinds`, `expect_illegal`, `vanilla_deck` and the trace hooks behave as in Python.

mod l5_common;

use gcg_engine::canon::state_digest;
use gcg_engine::testkit::{
    act, expect_illegal, has_action, legal_kinds, set_force_link, vanilla_deck, zone_of,
};
use gcg_engine::{
    Action, ActionKind, AddOptions, NewGameOptions, Scenario, ScenarioOptions, Step, TestkitError,
    Zone,
};
use l5_common::{int, load, record, records, reg, start_game, strs};
use serde_json::{Value, json};

fn player(v: &Value) -> u8 {
    u8::try_from(v.as_u64().unwrap()).unwrap()
}

fn zone(v: &Value) -> Zone {
    Zone::from_value(int(v)).unwrap()
}

fn options(init: &Value) -> ScenarioOptions {
    let d = ScenarioOptions::default();
    ScenarioOptions {
        active: init.get("active").map_or(d.active, player),
        turn: init
            .get("turn")
            .map_or(d.turn, |v| i32::try_from(int(v)).unwrap()),
        seed: init.get("seed").map_or(d.seed, |v| v.as_u64().unwrap()),
        deck_size: init.get("deck_size").map_or(d.deck_size, |v| {
            usize::try_from(v.as_u64().unwrap()).unwrap()
        }),
    }
}

/// Runs one Python operation of a `scenario` record; the result as Python's JSON.
fn run_op(sc: &mut Scenario, op: &[Value]) -> Result<Value, TestkitError> {
    let name = op[0].as_str().unwrap();
    let a = &op[1..];
    Ok(match name {
        "prepare_start" => {
            let step = Step::from_value(int(&a[0])).unwrap();
            sc.prepare_start(step)?;
            Value::Null
        }
        "add" => {
            let kw = &a[3];
            let flag = |k: &str, d: bool| kw.get(k).map_or(d, |v| v.as_bool().unwrap());
            let o = AddOptions {
                rested: flag("rested", false),
                damage: kw
                    .get("damage")
                    .map_or(0, |v| i32::try_from(int(v)).unwrap()),
                pilot: kw.get("pilot").map(|v| v.as_str().unwrap()),
                deployed_this_turn: flag("deployed_this_turn", false),
                known: flag("known", true),
            };
            json!(sc.add(player(&a[0]), a[1].as_str().unwrap(), zone(&a[2]), o)?)
        }
        "hand" => json!(sc.hand(player(&a[0]), &strs(&a[1]))?),
        "trash" => json!(sc.trash(player(&a[0]), &strs(&a[1]))?),
        "shields" => json!(sc.shields(player(&a[0]), &strs(&a[1]))?),
        "deck" => {
            sc.deck(player(&a[0]), &strs(&a[1]));
            Value::Null
        }
        "resources" => {
            let n = |v: &Value| usize::try_from(v.as_u64().unwrap()).unwrap();
            json!(sc.resources(player(&a[0]), n(&a[1]), n(&a[2]), n(&a[3]))?)
        }
        "base" => {
            let damage = i32::try_from(int(&a[2])).unwrap();
            json!(sc.base(player(&a[0]), a[1].as_str(), damage)?)
        }
        "resource_deck" => {
            sc.resource_deck(
                player(&a[0]),
                usize::try_from(a[1].as_u64().unwrap()).unwrap(),
            )?;
            Value::Null
        }
        "add_raw" => json!(sc.add_raw(player(&a[0]), a[1].as_str().unwrap(), zone(&a[2]))?),
        other => panic!("unknown scenario op {other}"),
    })
}

#[test]
fn scenarios_build_pythons_state_before_advance() {
    for rec in records("scenario") {
        let id = rec["id"].as_str().unwrap();
        let mut sc = Scenario::new(&reg(), options(&rec["init"]));
        let mut before = None;
        for (op, want) in rec["ops"]
            .as_array()
            .unwrap()
            .iter()
            .zip(rec["results"].as_array().unwrap())
        {
            let op = op.as_array().unwrap();
            let got = run_op(&mut sc, op).unwrap_or_else(|e| panic!("{id}: {op:?}: {e:?}"));
            assert_eq!(&got, want, "{id}: {op:?}");
            if op[0] == "prepare_start" {
                before = Some(state_digest(sc.state()));
                assert_eq!(
                    sc.state().rules_checked(),
                    int(&rec["rules_checked"]),
                    "{id}"
                );
            }
        }
        assert_eq!(
            before.as_deref(),
            rec["before"].as_str(),
            "{id}: before advance"
        );
        assert_eq!(
            state_digest(sc.state()),
            rec["final"].as_str().unwrap(),
            "{id}: final"
        );
        let snap = load(&rec["final_snapshot"]);
        assert_eq!(
            state_digest(&snap),
            state_digest(sc.state()),
            "{id}: snapshot"
        );
        assert_eq!(sc.state().decklists(), snap.decklists(), "{id}: decklists");
    }
}

#[test]
fn scenario_first_player_follows_the_turn_parity() {
    let reg = reg();
    let mk = |active, turn| {
        Scenario::new(
            &reg,
            ScenarioOptions {
                active,
                turn,
                ..ScenarioOptions::default()
            },
        )
        .into_state()
    };
    assert_eq!(mk(0, 3).first_player(), 0);
    assert_eq!(mk(1, 3).first_player(), 1);
    assert_eq!(mk(1, 4).first_player(), 0);
    assert_eq!(mk(1, -1).first_player(), 1);
    let st = mk(1, 6);
    assert_eq!((st.turn(), st.active(), st.version()), (6, 1, 0));
}

#[test]
fn scenario_errors_name_the_card() {
    let mut sc = Scenario::new(&reg(), ScenarioOptions::default());
    let err = sc
        .add(0, "XX99-999", Zone::Hand, AddOptions::default())
        .unwrap_err();
    assert_eq!(err, TestkitError::UnknownCard("XX99-999".into()));
    assert!(sc.state().cards().is_empty());
    let err = sc.prepare_start(Step::Damage).unwrap_err();
    assert!(
        matches!(err, TestkitError::Engine(ref e) if e.py_class() == "KeyError"),
        "{err:?}"
    );
}

#[test]
fn helpers_fail_with_pythons_assertion_texts() {
    let t = record("testkit", "setup");
    let fresh = Scenario::new(&reg(), ScenarioOptions::default());
    let mut st = fresh.into_state();
    let err = act(&mut st, ActionKind::EndMain, None, None, None).unwrap_err();
    assert_eq!(
        err,
        TestkitError::Assertion(t["fail_no_decision"].as_str().unwrap().into())
    );

    let st = start_game(t, NewGameOptions::default()).unwrap();
    let mut probe = st.clone();
    let err = act(&mut probe, ActionKind::Keep, Some(3), None, None).unwrap_err();
    assert_eq!(
        err,
        TestkitError::Assertion(t["fail_keep"].as_str().unwrap().into())
    );
    let err = act(&mut probe, ActionKind::GoFirst, Some(5), None, Some(0)).unwrap_err();
    assert_eq!(
        err,
        TestkitError::Assertion(t["fail_go_first"].as_str().unwrap().into())
    );
    assert_eq!(state_digest(&probe), state_digest(&st));

    assert_eq!(
        has_action(&st, ActionKind::GoFirst, Some(1), None),
        t["has_go_first_1"]
    );
    assert_eq!(
        has_action(&st, ActionKind::GoFirst, Some(2), None),
        t["has_go_first_2"]
    );
    let kinds: Vec<&str> = legal_kinds(&st).iter().map(|k| k.as_str()).collect();
    assert_eq!(json!(kinds), t["legal_kinds"]);
}

#[test]
fn expect_illegal_accepts_rejections_and_reports_acceptance() {
    let t = record("testkit", "setup");
    let st = start_game(t, NewGameOptions::default()).unwrap();
    let mut rejected = st.clone();
    expect_illegal(&mut rejected, Action::of(ActionKind::Keep)).unwrap();
    assert!(t["rejected_unchanged"].as_bool().unwrap());
    assert_eq!(state_digest(&rejected), state_digest(&st));
    let mut probe = st.clone();
    let err = expect_illegal(&mut probe, Action::new(ActionKind::GoFirst, 1, -1, -1)).unwrap_err();
    assert_eq!(
        err,
        TestkitError::Assertion(t["accepted"].as_str().unwrap().into())
    );
    assert_eq!(state_digest(&probe), t["after_accepted"].as_str().unwrap());
}

#[test]
fn act_applies_the_first_matching_option() {
    let t = record("testkit", "setup");
    let mut st = start_game(t, NewGameOptions::default()).unwrap();
    let a = act(&mut st, ActionKind::GoFirst, None, None, None).unwrap();
    assert_eq!(a, Action::new(ActionKind::GoFirst, 1, -1, -1));
    assert_eq!(state_digest(&st), t["after_accepted"].as_str().unwrap());
    assert_eq!(zone_of(&st, st.zone(1, Zone::Hand)[0]), Zone::Hand);
}

#[test]
fn vanilla_deck_matches_python() {
    let t = record("testkit", "setup");
    let reg = reg();
    let db = reg.db();
    let d = vanilla_deck(db, "Red", "R-001").unwrap();
    let numbers = |ids: &[gcg_data::DefId]| -> Vec<String> {
        ids.iter()
            .map(|&i| db.by_id(i).card_number.to_string())
            .collect()
    };
    assert_eq!(
        json!([numbers(&d.main), numbers(&d.resources)]),
        t["vanilla_red"]
    );
    assert_eq!(
        vanilla_deck(db, "Nope", "R-001").unwrap_err(),
        TestkitError::Value(t["vanilla_error"].as_str().unwrap().into())
    );
    assert_eq!(
        vanilla_deck(db, "Red", "XX").unwrap_err(),
        TestkitError::UnknownCard("XX".into())
    );
}

#[test]
fn force_link_touches_nothing_and_clones_share_it() {
    let t = record("testkit", "setup");
    let mut st = start_game(t, NewGameOptions::default()).unwrap();
    let (digest, version) = (state_digest(&st), st.version());
    set_force_link(&mut st, &["GD05-001", "GD05-002"]);
    assert_eq!((state_digest(&st), st.version()), (digest, version));
    let mut sc = Scenario::new(&reg(), ScenarioOptions::default());
    sc.force_link(&["GD01-001"]);
    sc.prepare_start(Step::Main).unwrap();
    let plain = {
        let mut p = Scenario::new(&reg(), ScenarioOptions::default());
        p.prepare_start(Step::Main).unwrap();
        state_digest(p.state())
    };
    assert_eq!(state_digest(sc.state()), plain);
}
