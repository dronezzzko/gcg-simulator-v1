//! Layer 5, setup (rule 6): `new_game`'s RNG order, uid assignment, decklists and pending
//! decision, the setup decisions up to the second player's redraw choice, the deck
//! construction checks and their messages, `apply`'s errors, `step`, `legal_actions` and
//! `concede`, all against the Python vectors of `tests/fixtures/l5_vectors.ndjson`
//! (`engine/game.py:58-201`, `:763-818`, `:961-965`).

mod l5_common;

use gcg_data::DeckList;
use gcg_engine::canon::state_digest;
use gcg_engine::{
    Action, ActionKind, ApplyError, IllegalActionError, NewGameError, NewGameOptions, Step, Zone,
    apply, concede, deck_construction_problems, deck_construction_problems_by_number,
    legal_actions, new_game, step,
};
use l5_common::{action, int, pending_json, record, records, reg, start_game, strs, u64_of};
use serde_json::{Value, json};

fn opts(rec: &Value) -> NewGameOptions {
    NewGameOptions {
        chooser: rec["chooser"]
            .as_u64()
            .map(|c| u8::try_from(c).expect("a player")),
        ..NewGameOptions::default()
    }
}

fn zone_json(st: &gcg_engine::GameState, z: Zone) -> Value {
    json!([st.zone(0, z), st.zone(1, z)])
}

#[test]
fn new_game_shuffles_cuts_and_numbers_cards_as_python() {
    for rec in records("new_game") {
        let id = rec["id"].as_str().unwrap();
        let st = start_game(rec, opts(rec)).expect("new_game");
        let after = &rec["after"];
        assert_eq!(
            state_digest(&st),
            after["digest"].as_str().unwrap(),
            "{id}: digest"
        );
        assert_eq!(st.rng_state(), u64_of(&after["rng"]), "{id}: rng");
        assert_eq!(
            st.version(),
            after["version"].as_u64().unwrap(),
            "{id}: version"
        );
        assert_eq!(
            i64::from(st.setup_chooser()),
            int(&after["setup_chooser"]),
            "{id}"
        );
        assert_eq!(pending_json(&st), after["pending"], "{id}: pending");
        let cards: Vec<Value> = st
            .cards()
            .iter()
            .map(|c| json!([c.def_id.0, c.owner, c.zone.value(), c.known]))
            .collect();
        assert_eq!(Value::from(cards), after["cards"], "{id}: cards");
        let dl = st.decklists();
        let rdl = st.resource_decklists();
        let ids = |s: &[gcg_data::DefId]| s.iter().map(|d| d.0).collect::<Vec<_>>();
        assert_eq!(json!([ids(dl[0]), ids(dl[1])]), after["decklists"], "{id}");
        assert_eq!(
            json!([ids(rdl[0]), ids(rdl[1])]),
            after["resource_decklists"],
            "{id}"
        );
        assert_eq!(legal_actions(&st), st.pending().unwrap().options, "{id}");
        assert_eq!(st.phase(), gcg_engine::Phase::Setup);
        assert_eq!(st.step(), Step::SetupChooseFirst);
    }
}

#[test]
fn new_game_on_def_ids_equals_new_game_on_card_numbers() {
    let reg = reg();
    let db = reg.db();
    for rec in records("new_game") {
        let list = |p: usize| DeckList {
            main: strs(&rec["decks"][p]["main"])
                .iter()
                .map(|n| db.get(n).unwrap().def_id)
                .collect(),
            resources: strs(&rec["decks"][p]["res"])
                .iter()
                .map(|n| db.get(n).unwrap().def_id)
                .collect(),
        };
        let (d0, d1) = (list(0), list(1));
        assert!(deck_construction_problems(db, &d0).is_empty());
        let st = new_game(&reg, [&d0, &d1], u64_of(&rec["seed"]), opts(rec)).expect("new_game");
        assert_eq!(state_digest(&st), rec["after"]["digest"].as_str().unwrap());
    }
}

#[test]
fn setup_decisions_draw_redraw_and_shuffle_as_python() {
    let mut checked = 0;
    for rec in records("new_game") {
        let id = rec["id"].as_str().unwrap();
        let mut st = start_game(rec, opts(rec)).expect("new_game");
        for (k, s) in rec["steps"].as_array().unwrap().iter().enumerate() {
            if int(&s["step"]) >= i64::from(Step::ActiveStep.value()) {
                // Applying this decision runs SETUP_SHIELDS and turn 1 (rules management, the
                // view, draws, main options): covered by needs_engine_setup_into_turn_one.
                break;
            }
            apply(&mut st, action(&s["apply"]), true).expect("a setup decision");
            let at = format!("{id} step {k}");
            assert_eq!(
                state_digest(&st),
                s["digest"].as_str().unwrap(),
                "{at}: digest"
            );
            assert_eq!(st.rng_state(), u64_of(&s["rng"]), "{at}: rng");
            assert_eq!(
                st.version(),
                s["version"].as_u64().unwrap(),
                "{at}: version"
            );
            assert_eq!(i64::from(st.step().value()), int(&s["step"]), "{at}: step");
            assert_eq!(st.action_count(), int(&s["action_count"]), "{at}");
            assert_eq!(
                i64::from(st.first_player()),
                int(&s["first_player"]),
                "{at}"
            );
            assert_eq!(pending_json(&st), s["pending"], "{at}: pending");
            assert_eq!(zone_json(&st, Zone::Hand), s["hands"], "{at}: hands");
            assert_eq!(zone_json(&st, Zone::Deck), s["decks"], "{at}: decks");
            assert_eq!(json!(st.redraws()), s["redraws"], "{at}: redraws");
            checked += 1;
        }
    }
    assert!(checked >= 8, "{checked} setup decisions checked");
}

#[test]
fn inventory_vector_after_new_game() {
    // Recorded: setup_chooser 1, rng 7774064181650859138, version 120, 120 cards, the
    // CHOOSE_FIRST decision, and the first ten def ids of player 0's decklist.
    let rec = record("new_game", "inv06b-18");
    let st = start_game(rec, NewGameOptions::default()).expect("new_game");
    assert_eq!(st.setup_chooser(), 1);
    assert_eq!(st.rng_state(), 7_774_064_181_650_859_138);
    assert_eq!(st.version(), 120);
    assert_eq!(st.cards().len(), 120);
    assert_eq!(
        pending_json(&st),
        json!({"player": 1, "kind": "choose_first",
               "options": [["go_first", 1, -1, -1], ["go_first", 0, -1, -1]],
               "prompt": "choose Player One", "context": []})
    );
    let first: Vec<u16> = st.decklists()[0][..10].iter().map(|d| d.0).collect();
    assert_eq!(first, [178, 178, 178, 178, 179, 179, 179, 179, 183, 183]);
    for uid in 0..120u16 {
        let c = st.card(uid);
        let p = u8::from((50..100).contains(&uid) || uid >= 110);
        assert_eq!(c.owner, p, "uid {uid}");
        let expected = if uid < 100 {
            Zone::Deck
        } else {
            Zone::ResourceDeck
        };
        assert_eq!(c.zone, expected, "uid {uid}");
    }
}

#[test]
fn construction_errors_and_lookups_raise_like_python() {
    for rec in records("new_game_error") {
        let id = rec["id"].as_str().unwrap();
        let o = NewGameOptions {
            validate: rec["validate"].as_bool().unwrap(),
            ..NewGameOptions::default()
        };
        let err = start_game(rec, o)
            .err()
            .unwrap_or_else(|| panic!("{id}: no error"));
        let class = match &err {
            NewGameError::Construction(_) => "DeckConstructionError",
            NewGameError::Unimplemented(_) => "UnimplementedCardError",
            NewGameError::Engine(e) => e.py_class(),
        };
        assert_eq!(class, rec["class"].as_str().unwrap(), "{id}");
        assert_eq!(err.to_string(), rec["message"].as_str().unwrap(), "{id}");
    }
}

#[test]
fn deck_construction_problems_match_python_texts_and_order() {
    let reg = reg();
    for rec in records("problems") {
        let got =
            deck_construction_problems_by_number(reg.db(), &strs(&rec["main"]), &strs(&rec["res"]));
        let want: Vec<String> = strs(&rec["problems"])
            .iter()
            .map(|s| (*s).to_owned())
            .collect();
        assert_eq!(got, want, "{}", rec["id"]);
    }
}

#[test]
fn apply_rejects_illegal_actions_without_changing_the_state() {
    let rec = record("new_game", "inv06b-18");
    let mut st = start_game(rec, NewGameOptions::default()).expect("new_game");
    let before = state_digest(&st);
    let keep = Action::of(ActionKind::Keep);
    let err = apply(&mut st, keep, true).unwrap_err();
    assert_eq!(
        err,
        ApplyError::Illegal(IllegalActionError::Illegal {
            action: keep,
            kind: gcg_engine::DecisionKind::ChooseFirst
        })
    );
    assert_eq!(err.to_string(), "illegal action keep() for choose_first");
    assert_eq!(state_digest(&st), before);
    let attack = Action::new(ActionKind::Attack, 5, -2, -1);
    assert_eq!(
        apply(&mut st, attack, true).unwrap_err().to_string(),
        "illegal action attack(5,-2) for choose_first"
    );
    assert_eq!(state_digest(&st), before);
}

#[test]
fn step_returns_a_new_state_and_leaves_the_input() {
    let rec = record("new_game", "inv06b-18");
    let st = start_game(rec, NewGameOptions::default()).expect("new_game");
    let before = state_digest(&st);
    let go = action(&rec["steps"][0]["apply"]);
    let next = step(&st, go).expect("step");
    assert_eq!(state_digest(&st), before);
    assert_eq!(
        state_digest(&next),
        rec["steps"][0]["digest"].as_str().unwrap()
    );
    let mut applied = st.clone();
    apply(&mut applied, go, true).unwrap();
    assert_eq!(state_digest(&applied), state_digest(&next));
    assert!(step(&st, Action::of(ActionKind::Keep)).is_err());
}

#[test]
fn concede_ends_the_game_once_without_touching() {
    let t = record("testkit", "setup");
    let mut st = start_game(t, NewGameOptions::default()).expect("new_game");
    concede(&mut st, 0);
    let c = &t["conceded"];
    assert_eq!(state_digest(&st), c["digest"].as_str().unwrap());
    assert_eq!(st.version(), c["version"].as_u64().unwrap());
    assert_eq!(st.winner().map(i64::from), c["winner"].as_i64());
    assert_eq!(st.end_reason(), Some(gcg_engine::EndReason::Concede));
    assert_eq!(st.step(), Step::GameOver);
    assert!(st.pending().is_none() && legal_actions(&st).is_empty());
    concede(&mut st, 1);
    assert_eq!(state_digest(&st), t["conceded_twice"].as_str().unwrap());
    assert_eq!(
        apply(&mut st, Action::of(ActionKind::Keep), true).unwrap_err(),
        ApplyError::Illegal(IllegalActionError::NoDecision)
    );
    assert_eq!(
        apply(&mut st, Action::of(ActionKind::Keep), false)
            .unwrap_err()
            .to_string(),
        "no decision pending"
    );
}

#[test]
fn new_game_keeps_the_caps_and_the_explicit_chooser() {
    let rec = record("new_game", "chooser0");
    let o = NewGameOptions {
        chooser: Some(0),
        turn_limit: 7,
        max_actions: 33,
        validate: true,
    };
    let st = start_game(rec, o).expect("new_game");
    assert_eq!((st.turn_limit(), st.max_actions()), (7, 33));
    assert_eq!(st.setup_chooser(), 0);
    // An explicit chooser draws no die roll: the RNG is where the two cuts left it.
    let rolled = start_game(rec, NewGameOptions::default()).expect("new_game");
    assert_ne!(st.rng_state(), rolled.rng_state());
}
