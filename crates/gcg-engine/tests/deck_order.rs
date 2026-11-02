//! T-151: `new_game` keeps the caller's deck order (`engine/game.py:107-158`; Inv 06b §5). It
//! shuffles each main deck from the order it is given and leaves the resource deck in that
//! order, so one deck multiset in two orders gives two different games from the same seed,
//! each equal to Python's (`tests/fixtures/engine_vectors.ndjson`, `deck_order` records): the
//! same RNG stream and decklists, different deck orders, every resource deck in the caller's
//! order. The def-id entry point (`new_game` on `DeckList`s) agrees with the card-number one.

mod l5_common;

use gcg_data::{DeckList, DefId};
use gcg_engine::canon::state_digest;
use gcg_engine::{GameState, NewGameOptions, Zone, new_game, new_game_by_number};
use l5_common::{engine_records, int, reg, strs, u64_of};
use serde_json::{Value, json};

fn zone_defs(st: &GameState, p: u8, z: Zone) -> Vec<u16> {
    st.zone(p, z).iter().map(|&u| st.card(u).def_id.0).collect()
}

fn ids(v: &[DefId]) -> Vec<u16> {
    v.iter().map(|d| d.0).collect()
}

fn options(rec: &Value) -> NewGameOptions {
    NewGameOptions {
        chooser: rec["chooser"]
            .as_u64()
            .map(|c| u8::try_from(c).expect("a player")),
        ..NewGameOptions::default()
    }
}

/// `new_game` on the order's decks by card number, checked against Python's facts.
fn start(rec: &Value, order: &Value) -> GameState {
    let at = format!("{} {}", rec["id"], order["name"]);
    let d = |p: usize, k: &str| strs(&order["decks"][p][k]);
    let (m0, r0, m1, r1) = (d(0, "main"), d(0, "res"), d(1, "main"), d(1, "res"));
    let seed = u64_of(&rec["seed"]);
    let st = new_game_by_number(&reg(), [(&m0, &r0), (&m1, &r1)], seed, options(rec))
        .unwrap_or_else(|e| panic!("{at}: {e}"));
    let f = &order["facts"];
    assert_eq!(
        state_digest(&st),
        f["digest"].as_str().unwrap(),
        "{at}: digest"
    );
    assert_eq!(st.rng_state(), u64_of(&f["rng"]), "{at}: rng");
    assert_eq!(
        i64::from(st.setup_chooser()),
        int(&f["setup_chooser"]),
        "{at}"
    );
    let both = |z| json!([zone_defs(&st, 0, z), zone_defs(&st, 1, z)]);
    assert_eq!(both(Zone::Deck), f["deck"], "{at}: deck order");
    assert_eq!(both(Zone::ResourceDeck), f["resource_deck"], "{at}");
    let dl = st.decklists();
    assert_eq!(json!([ids(dl[0]), ids(dl[1])]), f["decklists"], "{at}");
    let rdl = st.resource_decklists();
    assert_eq!(
        json!([ids(rdl[0]), ids(rdl[1])]),
        f["resource_decklists"],
        "{at}"
    );
    // The resource deck is the caller's order, top first (`game.py:140-141`, no shuffle).
    let db = reg().db();
    for p in 0..2u8 {
        let given: Vec<u16> = strs(&order["decks"][usize::from(p)]["res"])
            .iter()
            .map(|n| db.get(n).unwrap().def_id.0)
            .collect();
        assert_eq!(
            zone_defs(&st, p, Zone::ResourceDeck),
            given,
            "{at}: player {p}"
        );
    }
    st
}

#[test]
fn two_orders_of_one_multiset_give_pythons_two_different_shuffles() {
    let mut resource_orders_differ = 0;
    for rec in engine_records("deck_order") {
        let id = &rec["id"];
        let orders = rec["orders"].as_array().unwrap();
        assert_eq!(orders.len(), 2, "{id}");
        let (a, b) = (start(rec, &orders[0]), start(rec, &orders[1]));
        assert_eq!(a.decklists(), b.decklists(), "{id}: one multiset");
        assert_eq!(a.resource_decklists(), b.resource_decklists(), "{id}");
        assert_eq!(a.rng_state(), b.rng_state(), "{id}: one RNG stream");
        for p in 0..2u8 {
            assert_ne!(
                zone_defs(&a, p, Zone::Deck),
                zone_defs(&b, p, Zone::Deck),
                "{id}: player {p}'s shuffle follows the given order"
            );
        }
        assert_ne!(state_digest(&a), state_digest(&b), "{id}");
        resource_orders_differ += usize::from(
            zone_defs(&a, 0, Zone::ResourceDeck) != zone_defs(&b, 0, Zone::ResourceDeck),
        );
    }
    assert!(
        resource_orders_differ >= 1,
        "a case with a mixed resource deck"
    );
}

#[test]
fn new_game_on_def_ids_keeps_the_order_too() {
    let reg = reg();
    let db = reg.db();
    for rec in engine_records("deck_order") {
        for order in rec["orders"].as_array().unwrap() {
            let list = |p: usize| {
                let defs = |k: &str| -> Box<[DefId]> {
                    strs(&order["decks"][p][k])
                        .iter()
                        .map(|n| db.get(n).unwrap().def_id)
                        .collect()
                };
                DeckList {
                    main: defs("main"),
                    resources: defs("res"),
                }
            };
            let (d0, d1) = (list(0), list(1));
            let st = new_game(&reg, [&d0, &d1], u64_of(&rec["seed"]), options(rec))
                .unwrap_or_else(|e| panic!("{}: {e}", rec["id"]));
            assert_eq!(
                state_digest(&st),
                order["facts"]["digest"].as_str().unwrap(),
                "{} {}",
                rec["id"],
                order["name"]
            );
        }
    }
}
