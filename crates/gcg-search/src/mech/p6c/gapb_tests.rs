//! GAP-B's unit tests of P6-C's world mechanisms (docs/port/eval/weakness-status/GAP-B.md):
//!
//! - W-08 `exact_pools`: the strengthened world of the moved-card scenario holds each owner's
//!   true hidden multiset where the reference pads, and on the searched decisions of the D2
//!   vectors every exact search is the reference's (no pool there is padded or trimmed).
//! - W-134 `opponent_list = archetype`: the posterior, a one-list prior equal to the true list
//!   reproducing `known` mode bit for bit (every searched vector decision, and every decision of
//!   two full games), and the information-set test: replacing the opponent's true decklist by
//!   another list consistent with the seen cards changes no decision or statistic, while the
//!   `known` search does change (the negative control).
//! - W-139 `opponent_view_share` at every decision: opponent-view worlds keep the identities of
//!   the cards the root options name and redraw the rest from the opponent's view.

use std::collections::BTreeMap;

use gcg_ai::{AgentCx, NoTrace, Scratch, SearchResult};
use gcg_core::pool::{Cancel, Pool, TaskCx};
use gcg_core::rng::{SeedPart, SplitMix64, derive_seed};
use gcg_data::{DefId, parse_deck};
use gcg_engine::canon::{canonical_state, from_canonical};
use gcg_engine::observe::{
    HIDDEN_ZONES, determinize, determinize_exact_into, information_set_key, is_hidden_from,
    with_decklists,
};
use gcg_engine::testkit::snapshot;
use gcg_engine::{
    DecisionKind, GameState, NewGameOptions, Uid, Zone, apply, legal_actions, new_game_by_number,
};

use super::archetype::{Posterior, Prior, hidden_of};
use super::tests::{add, bits, config, filled, registry, search, search_info, searched_states};
use super::{Search, Shared, view};
use crate::mcts::run;
use crate::mech::{Composed, WorldCx};
use crate::{MechanismEffort, Observation, StrengthenedConfig};

/// Each owner's cards hidden from `observer`, wherever they lie, as a sorted multiset.
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

/// One search of `cfg` at the information-set state `info` whose P6-C state starts as `shared`.
fn search_with(
    info: &GameState,
    player: u8,
    cfg: &StrengthenedConfig,
    seed: u64,
    shared: Shared,
) -> (SearchResult, Search) {
    let cancel = Cancel::new();
    let mut composed = Composed::new(cfg);
    composed.p6c = shared;
    let (result, _): (SearchResult, MechanismEffort) =
        Pool::run_inline(|scratch: &mut Scratch, par: &TaskCx<'_, Scratch>| {
            let mut cx = AgentCx {
                scratch,
                par,
                cancel: &cancel,
            };
            let mut rng = SplitMix64::new(seed);
            run(
                &composed,
                info,
                player,
                cfg,
                &mut rng,
                &mut cx,
                &mut NoTrace,
                None,
            )
        })
        .unwrap_or_else(|e| panic!("search failed: {e}"));
    (result, composed.p6c.into_search())
}

/// The prior of one list: the opponent's true decklists in `st`.
fn true_list_prior(st: &GameState, player: u8) -> Prior {
    let opponent = usize::from(1 - player);
    Prior::one(
        "true",
        st.decklists()[opponent],
        st.resource_decklists()[opponent],
    )
}

// ---------------------------------------------------------------------------------------------
// W-08

/// A main-phase position with distinct hidden cards on both sides, then the top card of the
/// searcher's deck moved into the opponent's deck (its owner stays the searcher): the
/// canonical state edited and loaded back, as no rule moves a card there.
fn moved_card_state() -> (GameState, Uid) {
    let reg = registry();
    let mut sc = gcg_engine::Scenario::new(&reg, gcg_engine::ScenarioOptions::default());
    for (p, hand) in [
        (0u8, ["ST01-005", "GD01-013"]),
        (1, ["GD03-114", "ST01-014"]),
    ] {
        for number in hand {
            add(&mut sc, p, number, Zone::Hand, false);
        }
    }
    sc.shields(0, &["GD01-060"; 5]).expect("shields");
    sc.shields(1, &["GD01-013"; 5]).expect("shields");
    sc.deck(
        0,
        &["GD01-060", "GD01-060", "ST01-005", "ST01-014", "GD01-060"],
    );
    sc.deck(
        1,
        &["GD01-013", "GD03-114", "GD01-013", "GD01-013", "GD01-013"],
    );
    sc.start(gcg_engine::Step::Main).expect("start");
    let st = sc.into_state();
    let deck = usize::from(Zone::Deck.value());
    let mut snap = snapshot(&st);
    let zones = &mut snap.state["zones"];
    let uid = zones[0][deck][0].as_u64().expect("a deck card");
    let rest: Vec<serde_json::Value> = zones[0][deck].as_array().expect("a list")[1..].to_vec();
    zones[0][deck] = serde_json::Value::Array(rest);
    zones[1][deck]
        .as_array_mut()
        .expect("a list")
        .push(serde_json::json!(uid));
    let moved = from_canonical(&registry(), &snap).expect("the edited state loads");
    (moved, Uid::try_from(uid).expect("a uid"))
}

/// W-08's testkit scenario in the search's worlds: every `exact_pools` world holds each
/// owner's true hidden multiset; every reference world pads one pool and trims the other.
#[test]
fn w08_exact_pool_worlds_hold_the_true_multisets_where_the_reference_pads() {
    let (st, moved) = moved_card_state();
    assert!(st.zone(1, Zone::Deck).contains(&moved) && st.card(moved).owner == 0);
    let truth: Vec<Vec<DefId>> = (0..2).map(|o| hidden_multiset(&st, 0, o)).collect();
    let obs = Observation::new(&st, 0).expect("an observation");
    let info = obs.info();
    let exact = config(&[("exact_pools", "true")]);
    let null = config(&[]);
    let mut rng = SplitMix64::new(808);
    let seeds: Vec<u64> = (0..64).map(|_| rng.next_u64()).collect();
    let (mut exact_ok, mut reference_ok) = (0, 0);
    let before = gcg_engine::observe::fit_adjustments();
    for k in 0..seeds.len() {
        let mut w = info.clone();
        let shared = Shared::new();
        let mut cx = WorldCx {
            st: info,
            player: 0,
            cfg: &exact,
            seeds: &seeds,
        };
        super::world(&exact.mechanisms, &mut cx, k, &mut w, &shared).expect("an exact world");
        let counts: Vec<Vec<DefId>> = (0..2).map(|o| hidden_multiset(&w, 0, o)).collect();
        exact_ok += usize::from(counts == truth);
        assert!(information_set_key(&w, 0) == information_set_key(info, 0));
        let mut r = info.clone();
        let mut cx = WorldCx {
            st: info,
            player: 0,
            cfg: &null,
            seeds: &seeds,
        };
        crate::mech::reference::world(&mut cx, k, &mut r).expect("a reference world");
        let counts: Vec<Vec<DefId>> = (0..2).map(|o| hidden_multiset(&r, 0, o)).collect();
        reference_ok += usize::from(counts == truth);
    }
    let after = gcg_engine::observe::fit_adjustments();
    eprintln!(
        "W-08 moved card: exact {exact_ok}/64 worlds hold the true multisets, reference \
         {reference_ok}/64; reference pads {} trims {}",
        after.pads - before.pads,
        after.trims - before.trims
    );
    assert_eq!(exact_ok, 64);
    assert_eq!(reference_ok, 0, "the reference never holds them");
    // One pad and one trim per reference world (the observation's own determinisation is
    // before `before`).
    assert_eq!(
        (after.pads - before.pads, after.trims - before.trims),
        (64, 64)
    );
}

#[test]
fn w08_exact_searches_are_the_reference_searches_on_the_vectors() {
    let states = searched_states();
    let null = config(&[]);
    let exact = config(&[("exact_pools", "true")]);
    let before = gcg_engine::observe::fit_adjustments();
    for (st, player, seed) in &states {
        let (base, _) = search(st, *player, &null, *seed);
        let (r, _) = search(st, *player, &exact, *seed);
        assert_eq!(bits(&r), bits(&base));
        assert_eq!(
            (r.action, r.stats.iterations),
            (base.action, base.stats.iterations)
        );
    }
    let after = gcg_engine::observe::fit_adjustments();
    assert_eq!(
        (after.pads - before.pads, after.trims - before.trims),
        (0, 0),
        "no pool of the vectors is padded or trimmed"
    );
    eprintln!("W-08: {} vector searches identical", states.len());
}

// ---------------------------------------------------------------------------------------------
// W-134

#[test]
fn w134_the_posterior_of_one_true_list_is_the_reference() {
    for (st, player, _) in searched_states().into_iter().step_by(3) {
        let obs = Observation::new(&st, player).expect("an observation");
        let info = obs.info();
        let p = Posterior::of(info, player, &true_list_prior(&st, player)).expect("a posterior");
        assert_eq!(p.entries.len(), 1);
        assert_eq!(p.entries[0].weight.to_bits(), 1.0f64.to_bits());
        assert_eq!(
            p.density.map(f64::to_bits),
            gcg_ai::burst_density(info, player).map(f64::to_bits)
        );
        assert_eq!(p.entries[0].state.decklists(), info.decklists());
        assert_eq!(
            p.entries[0].state.resource_decklists(),
            info.resource_decklists()
        );
    }
}

#[test]
fn w134_the_packaged_posterior_follows_the_seen_cards() {
    let (mut narrowed, mut states, mut true_kept) = (0, 0, 0);
    let prior = Prior::packaged(gcg_data::CardDb::embedded()).expect("the prior");
    for (st, player, _) in searched_states() {
        let obs = Observation::new(&st, player).expect("an observation");
        let p = Posterior::of(obs.info(), player, &prior).expect("a posterior");
        let total: f64 = p.entries.iter().map(|e| e.weight).sum();
        assert!((total - 1.0).abs() < 1e-12, "{total}");
        states += 1;
        narrowed += usize::from(p.entries.len() < prior.candidates.len());
        let opponent = usize::from(1 - player);
        let true_list = st.decklists()[opponent];
        true_kept += usize::from(
            p.entries
                .iter()
                .any(|e| e.state.decklists()[opponent] == true_list),
        );
        for e in &p.entries {
            // Every candidate world can hold the opponent's seen cards.
            let w = determinize(&e.state, player, 5).expect("a candidate world");
            assert!(information_set_key(&w, player) == information_set_key(obs.info(), player));
        }
    }
    eprintln!(
        "W-134 packaged prior: narrowed at {narrowed}/{states} decisions, the true list among \
         the candidates at {true_kept}"
    );
    assert!(narrowed > 0);
    assert_eq!(true_kept, states, "the vectors' decks are packaged lists");
}

#[test]
fn w134_a_one_list_prior_of_the_true_list_reproduces_known_mode_on_the_vectors() {
    let null = config(&[]);
    let archetype = config(&[("opponent_list", "archetype")]);
    let states = searched_states();
    for (st, player, seed) in &states {
        let obs = Observation::new(st, *player).expect("an observation");
        let (base, _) = search_info(obs.info(), *player, &null, *seed);
        let shared = Shared::with_prior(true_list_prior(st, *player));
        let (r, _) = search_with(obs.info(), *player, &archetype, *seed, shared);
        assert_eq!(bits(&r), bits(&base));
        assert_eq!(
            (r.action, r.stats.iterations),
            (base.action, base.stats.iterations)
        );
    }
    eprintln!("W-134: {} one-list searches identical", states.len());
}

/// A packaged deck as the card numbers `new_game_by_number` takes.
fn deck_numbers(name: &str) -> (Vec<String>, Vec<String>) {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(root.join(format!("decks/prior-{name}.txt")))
        .expect("a packaged deck");
    let deck = parse_deck(&text, name, None).expect("a deck");
    let expand = |cards: &[gcg_data::CardCount]| -> Vec<String> {
        cards
            .iter()
            .flat_map(|c| (0..c.count).map(|_| c.card_number.to_string()))
            .collect()
    };
    (expand(&deck.main), expand(&deck.resources))
}

fn refs(v: &[String]) -> Vec<&str> {
    v.iter().map(String::as_str).collect()
}

/// W-134's end-to-end check: two full games (seat 0 searching, seat 1 random) in which, at
/// every decision of seat 0 that searches, the archetype search with the one-list prior of
/// the true list equals the `known` search bit for bit; the game continues with that action.
#[test]
fn w134_a_one_list_prior_reproduces_known_mode_over_full_games() {
    let reg = registry();
    let mut known = config(&[]);
    known.max_iterations = 48;
    known.min_iterations = 16;
    let mut archetype = known.clone();
    archetype.mechanisms.opponent_list = crate::OpponentList::Archetype;
    let mut compared = 0;
    for (game, (a, b)) in [
        ("red-green-zeon", "blue-white-federation"),
        ("blue-green-ramp", "purple-blue-swarm"),
    ]
    .into_iter()
    .enumerate()
    {
        let (m0, r0) = deck_numbers(a);
        let (m1, r1) = deck_numbers(b);
        let (m0, r0, m1, r1) = (refs(&m0), refs(&r0), refs(&m1), refs(&r1));
        let seed = 20_261_134 + u64::try_from(game).expect("a game index");
        let mut st = new_game_by_number(
            &reg,
            [(&m0[..], &r0[..]), (&m1[..], &r1[..])],
            seed,
            NewGameOptions::default(),
        )
        .expect("a game");
        let mut rng = SplitMix64::new(seed);
        let mut k = 0i64;
        while let Some(dec) = st.pending() {
            let options = legal_actions(&st);
            let pregame = matches!(
                dec.hdr.kind,
                DecisionKind::Redraw | DecisionKind::ChooseFirst
            );
            let a = if dec.hdr.player == 1 || options.len() == 1 || pregame {
                options[rng.randrange_usize(options.len())]
            } else {
                k += 1;
                let s = derive_seed(seed, &[SeedPart::Int(k)]);
                let obs = Observation::new(&st, 0).expect("an observation");
                let (base, _) = search_info(obs.info(), 0, &known, s);
                let shared = Shared::with_prior(true_list_prior(&st, 0));
                let (r, _) = search_with(obs.info(), 0, &archetype, s, shared);
                assert_eq!(bits(&r), bits(&base), "game {game}, decision {k}");
                assert_eq!(r.action, base.action);
                compared += 1;
                base.action
            };
            apply(&mut st, a, true).expect("a legal action");
        }
    }
    eprintln!("W-134 e2e: {compared} searched decisions identical over two games");
    assert!(compared > 40, "{compared}");
}

/// The other true state of `st` for the information-set test: the opponent's decklist replaced
/// by `list` (a list that contains every card of the opponent the searcher sees) and the
/// opponent's hidden cards drawn from it.
fn with_other_list(st: &GameState, player: u8, list: &[DefId], seed: u64) -> GameState {
    let opponent = usize::from(1 - player);
    let mut main = st.decklists().map(|l| l.to_vec().into_boxed_slice());
    main[opponent] = list.to_vec().into_boxed_slice();
    let res = st
        .resource_decklists()
        .map(|l| l.to_vec().into_boxed_slice());
    let other = with_decklists(st, main, res).expect("lists");
    let mut out = other.clone();
    determinize_exact_into(&other, player, seed, &[], &mut out).expect("an exact state");
    out
}

/// The opponent's seen main-deck cards, by printed identity.
fn seen_main(st: &GameState, player: u8) -> BTreeMap<DefId, u32> {
    let db = st.db();
    let mut out = BTreeMap::new();
    for (i, c) in st.cards().iter().enumerate() {
        let uid = Uid::try_from(i).expect("a uid");
        if c.owner == player || is_hidden_from(st, uid, player) {
            continue;
        }
        let def = db.by_id(db.base_def_id(c.def_id));
        if def.card_type.is_token() || def.card_type == gcg_data::CardType::Resource {
            continue;
        }
        *out.entry(def.def_id).or_insert(0) += 1;
    }
    out
}

/// An alternative opponent list: another packaged list holding the seen cards (each missing
/// copy replaces a copy of an unseen card).
fn alternative_list(st: &GameState, player: u8) -> Option<Box<[DefId]>> {
    let opponent = usize::from(1 - player);
    let true_list = st.decklists()[opponent];
    let prior = Prior::packaged(st.db()).expect("the prior");
    let seen = seen_main(st, player);
    prior
        .candidates
        .iter()
        .find(|c| &*c.main != true_list)
        .map(|c| {
            let mut counts: BTreeMap<DefId, u32> = BTreeMap::new();
            for &d in &*c.main {
                *counts.entry(d).or_insert(0) += 1;
            }
            for (&d, &n) in &seen {
                while counts.get(&d).copied().unwrap_or(0) < n {
                    let spare = counts
                        .iter()
                        .filter(|&(e, &m)| m > seen.get(e).copied().unwrap_or(0))
                        .map(|(&e, _)| e)
                        .next()
                        .expect("an unseen card to replace");
                    *counts.get_mut(&spare).expect("a count") -= 1;
                    *counts.entry(d).or_insert(0) += 1;
                }
            }
            counts
                .iter()
                .flat_map(|(&d, &n)| std::iter::repeat_n(d, n as usize))
                .collect()
        })
}

#[test]
fn w134_archetype_searches_read_no_true_decklist() {
    let archetype = config(&[("opponent_list", "archetype")]);
    let null = config(&[]);
    let (mut states, mut known_changed) = (0, 0);
    for (st, player, seed) in searched_states().into_iter().step_by(2) {
        let Some(list) = alternative_list(&st, player) else {
            continue;
        };
        let other = with_other_list(&st, player, &list, 0x0711);
        assert!(information_set_key(&other, player) == information_set_key(&st, player));
        assert_ne!(other.decklists(), st.decklists());
        assert_ne!(hidden_of(&other, player), hidden_of(&st, player));
        let (r, s) = search(&st, player, &archetype, seed);
        let (o, os) = search(&other, player, &archetype, seed);
        assert_eq!(bits(&o), bits(&r));
        assert_eq!(
            (o.action, o.stats.iterations),
            (r.action, r.stats.iterations)
        );
        assert_eq!(os.posterior, s.posterior);
        let (kr, _) = search(&st, player, &null, seed);
        let (ko, _) = search(&other, player, &null, seed);
        known_changed += usize::from(bits(&kr) != bits(&ko));
        states += 1;
    }
    eprintln!(
        "W-134 information set: {states} decisions identical in archetype mode; known mode \
         changed at {known_changed} (the negative control)"
    );
    assert!(states >= 10, "{states}");
    assert!(known_changed > 0, "known mode reads the true decklist");
}

// ---------------------------------------------------------------------------------------------
// W-139

#[test]
fn w139_opponent_view_worlds_keep_the_named_cards_at_every_decision() {
    let reg = registry();
    // The searcher (player 0) can play ST01-005 (its option names it) and holds GD04-113, an
    // 【Action】 Command no main-phase option names.
    let mut sc = gcg_engine::Scenario::new(&reg, gcg_engine::ScenarioOptions::default());
    sc.resources(0, 3, 0, 0).expect("resources");
    add(&mut sc, 0, "ST01-005", Zone::Hand, false);
    add(&mut sc, 0, "GD04-113", Zone::Hand, false);
    add(&mut sc, 0, "GD01-013", Zone::Battle, true);
    let st = filled(sc);
    let dec = st.pending().expect("the main phase");
    let named = view::named_hidden(&st, &dec, 0);
    let hand = st.zone(0, Zone::Hand).to_vec();
    let playable = hand
        .iter()
        .copied()
        .find(|&u| st.def_of(u).card_number.as_ref() == "ST01-005")
        .expect("in hand");
    let trick = hand
        .iter()
        .copied()
        .find(|&u| st.def_of(u).card_number.as_ref() == "GD04-113")
        .expect("in hand");
    assert!(
        named.contains(&playable) && !named.contains(&trick),
        "{named:?}"
    );
    let obs = Observation::new(&st, 0).expect("an observation");
    let info = obs.info();
    let cfg = config(&[("opponent_view_share", "0.25")]);
    let shared = Shared::from_search(Search {
        player: Some(0),
        view_ok: true,
        view_keep: named.clone(),
        ..Search::default()
    });
    let mut rng = SplitMix64::new(139);
    let seeds: Vec<u64> = (0..200).map(|_| rng.next_u64()).collect();
    let (mut views, mut trick_kept) = (0, 0);
    for k in 0..seeds.len() {
        let mut cx = WorldCx {
            st: info,
            player: 0,
            cfg: &cfg,
            seeds: &seeds,
        };
        let mut w = info.clone();
        super::world(&cfg.mechanisms, &mut cx, k, &mut w, &shared).expect("a world");
        let reference = determinize(info, 0, seeds[k]).expect("a world");
        if !view::is_view_world(k, 0.25) {
            assert_eq!(canonical_state(&w), canonical_state(&reference));
            continue;
        }
        views += 1;
        assert_eq!(
            w.card(playable).def_id,
            st.card(playable).def_id,
            "world {k}"
        );
        trick_kept += usize::from(w.card(trick).def_id == st.card(trick).def_id);
        assert!(information_set_key(&w, 1) == information_set_key(&reference, 1));
        for o in 0..2u8 {
            assert_eq!(hidden_multiset(&w, 1, o), hidden_multiset(&reference, 1, o));
        }
    }
    let s = shared.into_search();
    eprintln!(
        "W-139 every decision: {views} opponent-view worlds kept the playable card; the \
         unnamed trick kept its identity in {trick_kept}"
    );
    assert_eq!((s.view_worlds, s.view_kept_worlds), (50, 50));
    assert!(
        trick_kept < 25,
        "the unnamed trick is redrawn ({trick_kept} of {views})"
    );
}

/// W-139's held-trick position of `tests/scenarios/p6c_w139_held_trick.rs` (the searcher's
/// GD02-078 rested, GD01-053 ready, GD04-113 "Damage Control" (Lv.3, cost 1) in hand and one of
/// its four Resources open; the opponent's GD03-016 can attack next turn), optionally with a
/// castable GD01-060 (Lv.2, cost 1) in the searcher's hand, hidden from the opponent, so that a
/// root option names a hidden card.
fn held_trick(castable_unit: bool) -> GameState {
    let reg = registry();
    let mut sc = gcg_engine::Scenario::new(&reg, gcg_engine::ScenarioOptions::default());
    sc.add(
        0,
        "GD02-078",
        Zone::Battle,
        gcg_engine::AddOptions {
            rested: true,
            ..gcg_engine::AddOptions::default()
        },
    )
    .expect("the rested Unit");
    add(&mut sc, 0, "GD01-053", Zone::Battle, true);
    sc.resources(0, 4, 3, 0).expect("resources");
    add(&mut sc, 0, "GD04-113", Zone::Hand, false);
    if castable_unit {
        add(&mut sc, 0, "GD01-060", Zone::Hand, false);
    }
    sc.shields(0, &["GD01-060"; 5]).expect("shields");
    sc.deck(0, &["GD01-060"; 20]);
    add(&mut sc, 1, "GD03-016", Zone::Battle, true);
    add(&mut sc, 1, "GD01-060", Zone::Battle, true);
    sc.resources(1, 5, 0, 0).expect("resources");
    add(&mut sc, 1, "GD01-060", Zone::Hand, false);
    add(&mut sc, 1, "GD01-060", Zone::Hand, false);
    sc.shields(1, &["GD01-060"; 5]).expect("shields");
    sc.deck(1, &["GD01-060"; 20]);
    sc.start(gcg_engine::Step::Main).expect("start");
    sc.into_state()
}

/// The decision of the strengthened agent `search-<cfg.name>` with agent seed `seed` at `st`
/// (the scenario tests' `count_cfg`, `tests/scenarios/p6c_common.rs`).
fn agent_action(cfg: &StrengthenedConfig, seed: u64, st: &GameState) -> gcg_engine::Action {
    let mut agent = crate::make_agent(cfg, seed, false);
    let player = st.pending().expect("a decision").hdr.player;
    let cancel = Cancel::new();
    Pool::run_inline(|scratch: &mut Scratch, par: &TaskCx<'_, Scratch>| {
        let mut cx = AgentCx {
            scratch,
            par,
            cancel: &cancel,
        };
        agent.choose(&st.clone(), player, &mut cx)
    })
    .unwrap_or_else(|e| panic!("choose: {e}"))
}

/// Whether the searcher still holds GD04-113 with an open Resource to cast it after `a`.
fn keeps_the_trick(st: &GameState, a: gcg_engine::Action) -> bool {
    let mut s = st.clone();
    apply(&mut s, a, true).expect("a legal action");
    let open = s
        .zone(0, Zone::ResourceArea)
        .iter()
        .filter(|&&u| !s.card(u).rested)
        .count();
    let held = s
        .zone(0, Zone::Hand)
        .iter()
        .any(|&u| s.def_of(u).card_number.as_ref() == "GD04-113");
    held && open >= 1
}

/// W-139's scenario, recorded at every decision: how many of 16 search seeds keep the trick
/// (end the main phase or attack with GD01-053 rather than spend the open Resource on the
/// 【Activate･Main】 or on the castable Unit), for the null configuration, `opponent_view_share`
/// 0.25 and 0.5, and the shipped `standard` preset with and without 0.25. Without the Unit no
/// root option names a hidden card; with it the opponent-view worlds keep the Unit's identity
/// (the case P6-C left without opponent-view worlds). The entry's bar is 12 of 16 for
/// strengthened `standard`.
#[test]
fn w139_held_trick_decisions_at_every_decision_are_recorded() {
    let count = |st: &GameState, cfg: &StrengthenedConfig| -> u32 {
        (1..=16u64)
            .map(|seed| u32::from(keeps_the_trick(st, agent_action(cfg, seed, st))))
            .sum()
    };
    let mut standard = crate::preset_config(crate::Preset::Standard).clone();
    standard.mechanisms.opponent_view_share = 0.0;
    let mut standard_view = standard.clone();
    standard_view.mechanisms.opponent_view_share = 0.25;
    let mut rows = Vec::new();
    for castable in [false, true] {
        let st = held_trick(castable);
        let dec = st.pending().expect("the main phase");
        let named = view::named_hidden(&st, &dec, 0);
        assert_eq!(named.is_empty(), !castable, "the castable Unit is named");
        let counts = [
            count(&st, &config(&[])),
            count(&st, &config(&[("opponent_view_share", "0.25")])),
            count(&st, &config(&[("opponent_view_share", "0.5")])),
            count(&st, &standard),
            count(&st, &standard_view),
        ];
        // Recorded 2026-11-03: without the Unit 0, 0, 0; 16, 16; with it 0, 0, 0; 16, 15.
        assert!(
            counts[0] <= 8,
            "the reference keeps it in at most 8: {counts:?}"
        );
        assert!(
            counts[3] >= 12,
            "strengthened standard keeps it in 12: {counts:?}"
        );
        rows.push(format!(
            "castable Unit {castable} ({} named hidden): null {}, share 0.25 {}, 0.5 {}; \
             standard {}, standard + 0.25 {}",
            named.len(),
            counts[0],
            counts[1],
            counts[2],
            counts[3],
            counts[4]
        ));
    }
    eprintln!("W-139 keeps the trick of 16 seeds: {}", rows.join(" | "));
}

#[test]
fn every_gapb_flag_runs_on_the_vector_decisions_and_reads_the_information_set_only() {
    let states: Vec<(GameState, u8, u64)> = searched_states().into_iter().step_by(4).collect();
    let rows: [&[(&str, &str)]; 4] = [
        &[("exact_pools", "true")],
        &[("opponent_list", "archetype")],
        &[("opponent_view_share", "0.25")],
        &[
            ("exact_pools", "true"),
            ("opponent_list", "archetype"),
            ("opponent_view_share", "0.25"),
            ("belief_worlds", "4"),
        ],
    ];
    let null = config(&[]);
    for on in rows {
        let cfg = config(on);
        let (mut changed, mut kept) = (0, 0u64);
        for (st, player, seed) in &states {
            let (base, _) = search(st, *player, &null, *seed);
            let (r, s) = search(st, *player, &cfg, *seed);
            changed += usize::from(bits(&base) != bits(&r));
            kept += s.view_kept_worlds;
            for other in super::tests::alternatives(st, *player) {
                let (o, os) = search(&other, *player, &cfg, *seed);
                assert_eq!(bits(&o), bits(&r), "{on:?}");
                assert_eq!(os, s, "{on:?}");
            }
        }
        eprintln!(
            "{on:?}: {changed}/{} decisions changed; {kept} opponent-view worlds kept named cards",
            states.len()
        );
    }
}
