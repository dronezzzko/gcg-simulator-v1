//! GAP-B, W-08 (docs/dev/WEAKNESSES.md): the exact-pools API of `observe` and the `_fit`
//! pad/trim counter.
//!
//! - The testkit scenario of W-08's verification: a card of player 0 moved into player 1's
//!   deck. `determinize` (the reference, `engine/observe.py:56-110`) pads one pool and trims
//!   the other, which the counter records (pinned); `determinize_exact` gives each owner the
//!   true hidden multiset and keeps the observer's information set.
//! - Where no card lies in the other player's zones and `_fit` changes nothing, the exact world
//!   is `determinize`'s, byte for byte (states of seeded random games).
//! - `assign_hidden`, kept identities, `with_decklists` and the conservation error.
//! - The fuzz invariant over 10,000 seeded random games: at every sampled state, for both
//!   observers, the exact world's hidden card counts of each owner equal the true state's.

mod l5_common;

use std::collections::BTreeMap;

use gcg_core::rng::SplitMix64;
use gcg_data::DefId;
use gcg_engine::canon::{from_canonical, state_digest};
use gcg_engine::observe::{
    HIDDEN_ZONES, assign_hidden, determinize, determinize_exact, determinize_exact_into,
    exact_pools, fit_adjustments, information_set_key, is_hidden_from, unknown_pools,
    with_decklists,
};
use gcg_engine::testkit::snapshot;
use gcg_engine::{
    AddOptions, GameState, HistKind, NewGameOptions, Scenario, ScenarioOptions, Step, Uid, Zone,
    apply, legal_actions,
};
use l5_common::{engine_records, reg, start_game};
use serde_json::{Value, json};

/// Each owner's cards hidden from `observer` (by owner, in any player's hidden zones), as a
/// sorted multiset of printed identities.
fn hidden_multiset(st: &GameState, observer: u8, owner: u8) -> Vec<DefId> {
    let db = st.db();
    let mut out: Vec<DefId> = (0..2u8)
        .flat_map(|side| HIDDEN_ZONES.iter().map(move |&z| (side, z)))
        .flat_map(|(side, z)| st.zone(side, z).iter().copied())
        .filter(|&u| st.card(u).owner == owner && is_hidden_from(st, u, observer))
        .map(|u| db.base_def_id(st.card(u).def_id))
        .collect();
    out.sort();
    out
}

/// The cards lying in the zones of the player who does not own them.
fn crossed(st: &GameState) -> usize {
    (0..2u8)
        .flat_map(|side| HIDDEN_ZONES.iter().map(move |&z| (side, z)))
        .flat_map(|(side, z)| st.zone(side, z).iter().map(move |&u| (side, u)))
        .filter(|&(side, u)| st.card(u).owner != side)
        .count()
}

/// A main-phase scenario with distinct hidden cards on both sides.
fn base_scenario() -> GameState {
    let reg = reg();
    let mut sc = Scenario::new(&reg, ScenarioOptions::default());
    let hidden = AddOptions {
        known: false,
        ..AddOptions::default()
    };
    for (p, hand) in [
        (0u8, ["ST01-005", "GD01-013"]),
        (1, ["GD03-114", "ST01-014"]),
    ] {
        for number in hand {
            sc.add(p, number, Zone::Hand, hidden)
                .unwrap_or_else(|e| panic!("add {number}: {e:?}"));
        }
    }
    sc.shields(0, &["GD01-060"; 5]).expect("shields");
    sc.shields(1, &["GD01-013"; 5]).expect("shields");
    sc.deck(
        0,
        &[
            "GD01-060", "GD01-060", "GD01-060", "ST01-005", "ST01-014", "GD01-060",
        ],
    );
    sc.deck(
        1,
        &[
            "GD01-013", "GD03-114", "GD01-013", "GD01-013", "GD01-013", "GD01-013",
        ],
    );
    sc.start(Step::Main).expect("start");
    sc.into_state()
}

/// `st` with the top card of player 0's deck moved into player 1's deck (its owner stays 0):
/// the canonical state is edited and loaded back, as no rule moves a card there.
fn moved_into_the_other_deck(st: &GameState) -> (GameState, Uid) {
    let deck = usize::from(Zone::Deck.value());
    let mut snap = snapshot(st);
    let zones = &mut snap.state["zones"];
    let uid = zones[0][deck][0].as_u64().expect("a deck card");
    let rest: Vec<Value> = zones[0][deck].as_array().expect("a list")[1..].to_vec();
    zones[0][deck] = Value::Array(rest);
    zones[1][deck]
        .as_array_mut()
        .expect("a list")
        .push(json!(uid));
    let moved = from_canonical(&reg(), &snap).expect("the edited state loads");
    (moved, Uid::try_from(uid).expect("a uid"))
}

#[test]
fn w08_a_card_in_the_other_players_deck_is_padded_by_the_reference_and_exact_here() {
    let (st, moved) = moved_into_the_other_deck(&base_scenario());
    assert_eq!(st.card(moved).owner, 0);
    assert!(st.zone(1, Zone::Deck).contains(&moved));
    assert_eq!(crossed(&st), 1);
    for observer in 0..2u8 {
        let truth: Vec<Vec<DefId>> = (0..2).map(|o| hidden_multiset(&st, observer, o)).collect();
        let before = fit_adjustments();
        let reference = determinize(&st, observer, 2026).expect("a reference world");
        let after = fit_adjustments();
        // The reference pads player 1's pool (one slot more than its owner's unseen cards)
        // and trims player 0's (one card more than player 0's slots), pinned.
        assert_eq!(
            (after.pads - before.pads, after.trims - before.trims),
            (1, 1),
            "observer {observer}"
        );
        let reference_counts: Vec<Vec<DefId>> = (0..2)
            .map(|o| hidden_multiset(&reference, observer, o))
            .collect();
        assert_ne!(
            reference_counts, truth,
            "observer {observer}: the reference pads"
        );
        let exact = determinize_exact(&st, observer, 2026).expect("an exact world");
        assert_eq!(fit_adjustments(), after, "exact pools never call _fit");
        for o in 0..2u8 {
            assert_eq!(
                hidden_multiset(&exact, observer, o),
                truth[usize::from(o)],
                "observer {observer}, owner {o}"
            );
        }
        assert!(
            information_set_key(&exact, observer) == information_set_key(&st, observer),
            "observer {observer}: the exact world keeps the information set"
        );
        let pools = exact_pools(&st, observer, 0).expect("conservation");
        assert!(
            pools.slots().contains(&moved),
            "the moved card is player 0's slot"
        );
        assert_eq!(
            pools.main.len() + pools.res.len(),
            pools.main_slots.len() + pools.res_slots.len()
        );
    }
}

/// The three example decks, as the `setup_touch` vectors carry them.
fn example_decks() -> Vec<&'static Value> {
    let mut out: Vec<&Value> = Vec::new();
    for rec in engine_records("setup_touch") {
        for d in rec["decks"].as_array().expect("decks") {
            if !out.contains(&d) {
                out.push(d);
            }
        }
    }
    assert_eq!(out.len(), 3, "the three example decks");
    out
}

/// One seeded random game between two example decks; `visit` sees every `every`-th decision
/// state and the final state. Returns the decisions played.
fn random_game(seed: u64, every: u64, mut visit: impl FnMut(&GameState)) -> u64 {
    let decks = example_decks();
    let mut rng = SplitMix64::new(seed);
    let rec = json!({
        "decks": [
            decks[rng.randrange_usize(decks.len())],
            decks[rng.randrange_usize(decks.len())],
        ],
        "seed": rng.next_u64().to_string(),
    });
    let mut st = start_game(&rec, NewGameOptions::default()).expect("a game");
    let mut n = 0u64;
    loop {
        let options = legal_actions(&st);
        if options.is_empty() {
            break;
        }
        if n.is_multiple_of(every) {
            visit(&st);
        }
        let a = options[rng.randrange_usize(options.len())];
        apply(&mut st, a, true).unwrap_or_else(|e| panic!("game {seed}: {a}: {e}"));
        n += 1;
    }
    visit(&st);
    n
}

#[test]
fn exact_worlds_are_the_reference_worlds_where_nothing_crosses() {
    let (mut compared, mut states) = (0u32, 0u32);
    for game in 0..24u64 {
        random_game(5_000 + game, 7, |st| {
            states += 1;
            assert_eq!(
                crossed(st),
                0,
                "no rule moves a card into the other player's zones"
            );
            for observer in 0..2u8 {
                for seed in [11u64, 0x5eed_0000_0000_0001] {
                    let before = fit_adjustments();
                    let Ok(reference) = determinize(st, observer, seed) else {
                        continue;
                    };
                    assert_eq!(fit_adjustments(), before, "a real game never pads or trims");
                    let mut exact = st.clone();
                    determinize_exact_into(st, observer, seed, &[], &mut exact)
                        .expect("conservation");
                    assert_eq!(state_digest(&exact), state_digest(&reference));
                    assert_eq!(exact.rng_state(), reference.rng_state());
                    assert_eq!(exact.version(), reference.version());
                    assert_eq!(exact.history_epoch(), reference.history_epoch());
                    compared += 1;
                }
            }
        });
    }
    assert!(
        states > 300 && compared == 4 * states,
        "{states} states, {compared} worlds"
    );
}

#[test]
fn assign_hidden_sets_only_hidden_identities_and_rewrites_the_history() {
    let st = base_scenario();
    let hand0 = st.zone(0, Zone::Hand).to_vec();
    let visible = st.zone(0, Zone::Battle).first().copied().or_else(|| {
        (0..u16::try_from(st.cards().len()).expect("uids")).find(|&u| !is_hidden_from(&st, u, 1))
    });
    let other = st.card(hand0[1]).def_id;
    let mut s = st.clone();
    if let Some(v) = visible {
        let err = assign_hidden(&mut s, 1, &[(hand0[0], other), (v, other)]);
        assert!(err.is_err(), "a card the observer sees is refused");
        assert_eq!(
            state_digest(&s),
            state_digest(&st),
            "an error changes nothing"
        );
    }
    // Player 0's own hand is not hidden from player 0.
    assert!(assign_hidden(&mut s, 0, &[(hand0[0], other)]).is_err());
    assert!(assign_hidden(&mut s, 1, &[(hand0[0], DefId(u16::MAX))]).is_err());
    let version = s.version();
    tk_record(&mut s, hand0[0]);
    assign_hidden(&mut s, 1, &[(hand0[0], other)]).expect("a hidden card");
    assert_eq!(s.card(hand0[0]).def_id, other);
    assert_eq!(
        s.version(),
        version + 2,
        "one touch by the record, one by assign_hidden"
    );
    let named = s
        .history()
        .iter()
        .filter(|h| h.uid == i32::from(hand0[0]))
        .collect::<Vec<_>>();
    assert!(!named.is_empty(), "the history names the card");
    assert!(
        named.iter().all(|h| h.def_id == i32::from(other.0)),
        "every entry names the new identity"
    );
}

/// A history entry naming `uid` (a reveal to its owner only would not do: any entry kind
/// works for the rewrite).
fn tk_record(s: &mut GameState, uid: Uid) {
    gcg_engine::testkit::engine::record(s, HistKind::Play, 0, -1, i32::from(uid), -1);
    gcg_engine::testkit::engine::touch(s);
}

#[test]
fn kept_cards_keep_their_identities_and_the_rest_stays_exact() {
    let st = base_scenario();
    let keep: Vec<Uid> = st.zone(0, Zone::Hand).to_vec();
    let truth: Vec<Vec<DefId>> = (0..2).map(|o| hidden_multiset(&st, 1, o)).collect();
    let mut changed = 0;
    for seed in 0..32u64 {
        let mut w = st.clone();
        determinize_exact_into(&st, 1, seed, &keep, &mut w).expect("an exact world");
        for &u in &keep {
            assert_eq!(w.card(u).def_id, st.card(u).def_id, "seed {seed}: kept {u}");
        }
        for o in 0..2u8 {
            assert_eq!(
                hidden_multiset(&w, 1, o),
                truth[usize::from(o)],
                "seed {seed}"
            );
        }
        let deck = |s: &GameState| -> Vec<DefId> {
            s.zone(0, Zone::Deck)
                .iter()
                .map(|&u| s.card(u).def_id)
                .collect()
        };
        changed += usize::from(deck(&w) != deck(&st));
    }
    assert!(
        changed > 16,
        "the rest is redrawn ({changed} of 32 decks changed)"
    );
    // A kept card the observer sees is refused.
    let seen = st.zone(1, Zone::Hand)[0];
    let mut w = st.clone();
    assert!(determinize_exact_into(&st, 1, 1, &[seen], &mut w).is_err());
}

/// The main and resource decklists of both players.
type Lists = ([Box<[DefId]>; 2], [Box<[DefId]>; 2]);

#[test]
fn with_decklists_moves_the_pools_and_the_same_lists_change_nothing() {
    let st = base_scenario();
    let lists = |s: &GameState| -> Lists {
        let [m0, m1] = s.decklists();
        let [r0, r1] = s.resource_decklists();
        ([m0.into(), m1.into()], [r0.into(), r1.into()])
    };
    let (main, res) = lists(&st);
    let same = with_decklists(&st, main, res).expect("the same lists");
    for observer in 0..2u8 {
        assert_eq!(
            state_digest(&determinize(&same, observer, 9).expect("a world")),
            state_digest(&determinize(&st, observer, 9).expect("a world"))
        );
    }
    // Player 1 with another list: one GD01-013 replaced by a ST01-005.
    let (main, res) = lists(&st);
    let [m0, m1] = main;
    let db = st.db();
    let swap_from = db.get("GD01-013").expect("a card").def_id;
    let swap_to = db.get("ST01-005").expect("a card").def_id;
    let mut other: Vec<DefId> = m1.to_vec();
    let i = other
        .iter()
        .position(|&d| d == swap_from)
        .expect("in the list");
    other[i] = swap_to;
    let moved = with_decklists(&st, [m0, other.into_boxed_slice()], res).expect("a list");
    let count = |pool: &[DefId], d: DefId| pool.iter().filter(|&&x| x == d).count();
    let (before, _) = unknown_pools(&st, 0, 1);
    let (after, _) = unknown_pools(&moved, 0, 1);
    assert_eq!(count(&after, swap_to), count(&before, swap_to) + 1);
    assert_eq!(count(&after, swap_from) + 1, count(&before, swap_from));
    let mut sorted = moved.decklists()[1].to_vec();
    sorted.sort();
    assert_eq!(moved.decklists()[1], &sorted[..], "stored sorted");
    // A list one card short is a conservation failure for the exact pools.
    let (main, res) = lists(&st);
    let [m0, m1] = main;
    let short = with_decklists(&st, [m0, m1[1..].into()], res).expect("a list");
    assert!(exact_pools(&short, 0, 1).is_err());
    assert!(determinize_exact(&short, 0, 3).is_err());
}

/// W-08's fuzz invariant: over 10,000 seeded random games, at every 16th decision and the
/// final state, for both observers, the exact world's hidden card counts of each owner equal
/// the true state's; the reference's `_fit` never pads or trims in them (the one-off probe of
/// `docs/dev/eval/weakness-gaps.md`, 0 of 3,528 pools, now over every sampled state).
#[test]
fn w08_fuzz_exact_worlds_keep_every_owners_hidden_card_counts() {
    let (mut states, mut worlds, mut decisions) = (0u64, 0u64, 0u64);
    let before = fit_adjustments();
    for game in 0..10_000u64 {
        decisions += random_game(0x6d08_0000 + game, 16, |st| {
            states += 1;
            for observer in 0..2u8 {
                let seed = st.version() ^ u64::from(observer);
                let w = determinize_exact(st, observer, seed)
                    .unwrap_or_else(|e| panic!("game {game}: {e}"));
                for owner in 0..2u8 {
                    let count = |s: &GameState| {
                        let mut m: BTreeMap<DefId, u32> = BTreeMap::new();
                        for d in hidden_multiset(s, observer, owner) {
                            *m.entry(d).or_default() += 1;
                        }
                        m
                    };
                    assert_eq!(count(&w), count(st), "game {game}, observer {observer}");
                }
                worlds += 1;
                let _ = determinize(st, observer, seed);
            }
        });
    }
    let after = fit_adjustments();
    eprintln!(
        "W-08 fuzz: 10000 games, {decisions} decisions, {states} sampled states, {worlds} exact \
         worlds; _fit pads {} trims {}",
        after.pads - before.pads,
        after.trims - before.trims
    );
    assert_eq!(
        (after.pads - before.pads, after.trims - before.trims),
        (0, 0),
        "{states} states"
    );
    assert!(
        states > 10_000 && worlds == 2 * states,
        "{states} states over {decisions} decisions"
    );
}
