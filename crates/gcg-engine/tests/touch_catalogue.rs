//! T-046, the touch catalogue (PLAN §4.4; Inv 05 §3.2, Inv 06a §4, Inv 06b §6, §15, §18).
//! `version` counts Python's `touch()` calls and is serialised and digested, so every touch
//! site must match Python one for one. Against `tests/fixtures/engine_vectors.ndjson`
//! (`port/oracle/export_engine_vectors.py`):
//!
//! - `new_game` leaves version 120: one `new_card` touch per card of 2 x (50 + 10);
//! - the setup accounting: SETUP_DRAW +10, +11 per redraw (5 + 5 moves and the shuffle),
//!   SETUP_SHIELDS +16 (12 moves, 3 new cards, 1 touch) and turn 1 +3 (active step, draw,
//!   resource), so the first MAIN decision has version 149 + 11 x (players who redrew), for
//!   every redraw plan, both first players and three deck pairs, each with Python's digest;
//! - the "always option 0" games: Python's version and action count before every decision and
//!   the final facts (Inv 06b §18: seed 12345 ends at version 357, digest `5ce28ed9…31e9a`);
//! - each touching primitive's version delta on one Scenario, equal to Python's and to the
//!   delta the inventory documents, with Python's digest after it.

mod l5_common;

use std::collections::BTreeSet;

use gcg_engine::canon::state_digest;
use gcg_engine::testkit::engine as tk;
use gcg_engine::{
    ActionKind, ContRef, Duration, GameState, HistKind, LastingArgs, NewGameOptions, Step, Uid,
    Zone, apply, concede, observe, procedures,
};
use l5_common::{
    action, action_json, engine_records, int, reg, replay_scenario, start_game, u64_of,
};
use serde_json::{Value, json};

fn version_of(v: &Value) -> u64 {
    v.as_u64().unwrap_or_else(|| panic!("not a version: {v}"))
}

#[test]
fn new_game_touches_once_per_card() {
    for rec in engine_records("setup_touch") {
        let id = rec["id"].as_str().unwrap();
        let st = start_game(rec, NewGameOptions::default()).expect("new_game");
        assert_eq!(st.cards().len(), 120, "{id}");
        assert_eq!(st.version(), 120, "{id}");
        let py = &rec["after_new_game"];
        assert_eq!(st.version(), version_of(&py["version"]), "{id}");
        assert_eq!(state_digest(&st), py["digest"].as_str().unwrap(), "{id}");
    }
}

#[test]
fn the_first_main_decision_has_version_149_plus_11_per_redraw() {
    let mut redraw_counts = BTreeSet::new();
    let mut firsts = BTreeSet::new();
    for rec in engine_records("setup_touch") {
        let id = rec["id"].as_str().unwrap();
        let mut st = start_game(rec, NewGameOptions::default()).expect("new_game");
        let mut redraws = 0u64;
        for (k, s) in rec["steps"].as_array().unwrap().iter().enumerate() {
            let a = action(&s["apply"]);
            apply(&mut st, a, true).unwrap_or_else(|e| panic!("{id}: {e}"));
            if a.kind == ActionKind::Redraw {
                redraws += 1;
            }
            let want = match k {
                0 => 120 + 10,           // SETUP_DRAW: 2 x 5 moves
                1 => 130 + 11 * redraws, // Player One's redraw: 10 moves + shuffle
                _ => 149 + 11 * redraws, // + SETUP_SHIELDS 16 + turn 1's 3
            };
            let at = format!("{id} step {k}");
            assert_eq!(st.version(), want, "{at}");
            assert_eq!(st.version(), version_of(&s["version"]), "{at}");
            assert_eq!(i64::from(st.step().value()), int(&s["step"]), "{at}");
            assert_eq!(json!(st.redraws()), s["redraws"], "{at}");
            assert_eq!(state_digest(&st), s["digest"].as_str().unwrap(), "{at}");
        }
        assert_eq!((st.step(), st.turn()), (Step::Main, 1), "{id}");
        redraw_counts.insert(redraws);
        firsts.insert(st.first_player());
    }
    assert_eq!(
        redraw_counts,
        BTreeSet::from([0, 1, 2]),
        "every redraw case"
    );
    assert_eq!(firsts, BTreeSet::from([0, 1]), "both first players");
}

#[test]
fn first_option_games_touch_as_python_before_every_decision() {
    let recs = engine_records("game_touch");
    for rec in &recs {
        let id = rec["id"].as_str().unwrap();
        let mut st = start_game(rec, NewGameOptions::default()).expect("new_game");
        let decisions = rec["decisions"].as_array().unwrap();
        for (i, d) in decisions.iter().enumerate() {
            let at = format!("{id} decision {i}");
            let dec = st.pending().unwrap_or_else(|| panic!("{at}: no decision"));
            assert_eq!(st.version(), version_of(&d["version"]), "{at}: version");
            assert_eq!(st.action_count(), int(&d["action_count"]), "{at}");
            assert_eq!(dec.hdr.kind.as_str(), d["kind"].as_str().unwrap(), "{at}");
            let a = dec.options[0];
            assert_eq!(action_json(&a), d["chosen"], "{at}");
            apply(&mut st, a, true).unwrap_or_else(|e| panic!("{at}: {e}"));
        }
        assert!(st.pending().is_none(), "{id}: Python's game ended here");
        let f = &rec["final"];
        assert_eq!(st.version(), version_of(&f["version"]), "{id}");
        assert_eq!(state_digest(&st), f["digest"].as_str().unwrap(), "{id}");
        assert_eq!(st.winner().map(i64::from), f["winner"].as_i64(), "{id}");
        assert_eq!(
            st.end_reason().map(|e| e.as_str()),
            f["end_reason"].as_str(),
            "{id}"
        );
        assert_eq!(i64::from(st.turn()), int(&f["turn"]), "{id}");
        assert_eq!(st.action_count(), int(&f["action_count"]), "{id}");
        assert_eq!(st.event_group(), int(&f["event_group"]), "{id}");
        assert_eq!(st.rng_state(), u64_of(&f["rng"]), "{id}");
    }
    // Inv 06b §18.
    let inv = recs
        .iter()
        .find(|r| r["id"] == "rgz-bwf/12345")
        .expect("the inventory game");
    assert_eq!(inv["decisions"].as_array().unwrap().len(), 69);
    assert_eq!(version_of(&inv["final"]["version"]), 357);
    assert_eq!(
        inv["final"]["digest"],
        "5ce28ed973cf9e06670a38f6a320c26a9a2c9f15fa5066e8fd8a6744c2f31e9a"
    );
}

// ---------------------------------------------------------------------------------------------
// Per-primitive deltas

fn uid(v: &Value) -> Uid {
    Uid::try_from(int(v)).expect("a uid")
}

fn player(v: &Value) -> u8 {
    u8::try_from(int(v)).expect("a player")
}

fn zone(v: &Value) -> Zone {
    Zone::from_value(int(v)).expect("a zone")
}

fn small<T: TryFrom<i64>>(v: i64) -> T {
    T::try_from(v).unwrap_or_else(|_| panic!("out of range: {v}"))
}

/// Runs a recorded primitive call (`[name, *args, {kwargs}]`, the dispatch of the exporter's
/// `run_primitive`): the new state for a call that returns one, and the call's result.
fn run_primitive(st: &mut GameState, call: &Value) -> (Option<GameState>, Value) {
    let c = call.as_array().expect("a call");
    let name = c[0].as_str().expect("a name");
    let (args, kw) = (&c[1..c.len() - 1], &c[c.len() - 1]);
    let flag = |k: &str, d: bool| kw.get(k).map_or(d, |v| v.as_bool().expect("a bool"));
    let num = |k: &str, d: i64| kw.get(k).map_or(d, int);
    let fail = |e: &dyn std::fmt::Display| -> ! { panic!("{name}: {e}") };
    let returned = match name {
        "touch" => {
            tk::touch(st);
            Value::Null
        }
        "core.move" => {
            let (b, r) = (flag("bottom", false), flag("reveal", false));
            tk::move_card(
                st,
                uid(&args[0]),
                zone(&args[1]),
                b,
                r,
                flag("rested", false),
            )
            .unwrap_or_else(|e| fail(&e));
            Value::Null
        }
        "core.remove_from_game" => {
            tk::remove_from_game(st, uid(&args[0])).unwrap_or_else(|e| fail(&e));
            Value::Null
        }
        "core.new_card" => {
            let def = reg().db().get(args[0].as_str().unwrap()).unwrap().def_id;
            let z = zone(&args[2]);
            json!(tk::new_card(st, def, player(&args[1]), z, flag("rested", false)).unwrap())
        }
        "core.shuffle_deck" => {
            tk::shuffle_deck(st, player(&args[0]));
            Value::Null
        }
        "core.record" => {
            let kind = HistKind::from_value(args[0].as_str().unwrap()).expect("a kind");
            let (by, u, src) = (num("by", -1), num("uid", -1), num("source", -1));
            tk::record(st, kind, player(&args[1]), small(by), small(u), small(src));
            Value::Null
        }
        "core.apply_damage" => {
            let (src, battle, by) = (num("source", -1), flag("battle", false), num("by", -1));
            let n = tk::apply_damage(
                st,
                uid(&args[0]),
                int(&args[1]),
                small(src),
                battle,
                small(by),
            );
            json!(n.unwrap_or_else(|e| fail(&e)))
        }
        "game.apply" => {
            apply(st, action(&args[0]), true).unwrap_or_else(|e| fail(&e));
            Value::Null
        }
        "interp.pay_generic" => {
            let ex = small(num("ex_used", -1));
            json!(tk::pay_generic(st, player(&args[0]), int(&args[1]), ex).unwrap())
        }
        "interp.add_lasting" => {
            let sm = &args[0]["StatMod"];
            let effect = ContRef::StatMod {
                ap: small(int(&sm[0])),
                hp: small(int(&sm[1])),
            };
            let targets: Vec<(Uid, i32)> = args[3]
                .as_array()
                .unwrap()
                .iter()
                .map(|t| (uid(&t[0]), small(int(&t[1]))))
                .collect();
            let duration = Duration::from_value(args[4].as_str().unwrap()).expect("a duration");
            let o = LastingArgs {
                player: -1,
                uses: 0,
                filters: &[],
                aux: -1,
            };
            let (ctl, src) = (player(&args[1]), small(int(&args[2])));
            tk::add_lasting_effect(st, effect, ctl, src, &targets, duration, o)
                .unwrap_or_else(|e| fail(&e));
            Value::Null
        }
        "interp.draw" => {
            let n = tk::draw(
                st,
                player(&args[0]),
                int(&args[1]),
                flag("by_effect", false),
            );
            json!(n.unwrap_or_else(|e| fail(&e)))
        }
        "interp.deploy_cards" => {
            let cards: Vec<Uid> = args[0].as_array().unwrap().iter().map(uid).collect();
            let (rested, by) = (flag("rested", false), small(num("by", -1)));
            tk::deploy_cards(st, &cards, rested, by, small(num("ex_used", 0)))
                .unwrap_or_else(|e| fail(&e));
            Value::Null
        }
        "battle.declare_attack" => {
            procedures::declare_attack(st, uid(&args[0]), small(int(&args[1])))
                .unwrap_or_else(|e| fail(&e));
            Value::Null
        }
        "game.concede" => {
            concede(st, player(&args[0]));
            Value::Null
        }
        "trial_rest" => {
            procedures::set_rested_for_trial(st, uid(&args[0]), true);
            Value::Null
        }
        "observe.determinize" => {
            let w = observe::determinize(st, player(&args[0]), u64_of(&args[1]));
            return (Some(w.unwrap_or_else(|e| fail(&e))), Value::Null);
        }
        "observe.permute_hidden" => {
            let w = observe::permute_hidden(st, player(&args[0]), u64_of(&args[1]));
            return (Some(w), Value::Null);
        }
        other => panic!("no primitive {other}"),
    };
    (None, returned)
}

#[test]
fn each_primitive_touches_as_python() {
    let recs = engine_records("primitive_touch");
    let mut touching = 0;
    for rec in &recs {
        let id = rec["id"].as_str().unwrap();
        let mut sc = replay_scenario(rec);
        let st = sc.state_mut();
        assert_eq!(
            st.version(),
            version_of(&rec["before"]),
            "{id}: the Scenario"
        );
        let source = state_digest(st);
        let (made, returned) = run_primitive(st, &rec["call"]);
        let out = made.as_ref().unwrap_or(st);
        let delta = out.version() - version_of(&rec["before"]);
        assert_eq!(
            delta,
            version_of(&rec["expect"]),
            "{id}: the documented delta"
        );
        assert_eq!(
            out.version(),
            version_of(&rec["after"]),
            "{id}: Python's delta"
        );
        assert_eq!(returned, rec["returned"], "{id}: the result");
        assert_eq!(state_digest(out), rec["digest"].as_str().unwrap(), "{id}");
        if made.is_some() {
            assert_eq!(
                state_digest(st),
                source,
                "{id}: the input state is unchanged"
            );
        }
        touching += usize::from(delta > 0);
    }
    assert!(
        recs.len() >= 29 && touching >= 20,
        "{} calls, {touching} touch",
        recs.len()
    );
}
