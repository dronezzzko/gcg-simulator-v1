//! P6-C's unit tests: the settings `check` refuses, the resampling, the opponent-view share,
//! the backup of an opponent-view iteration, the bucket abstraction, honesty (the worlds and the
//! search depend on the information set only, W-05/W-07) and a run of every flag over the
//! searched decisions of the D2 vectors.

use std::path::PathBuf;

use gcg_ai::{AgentCx, MoveKey, NoTrace, Scratch, SearchResult};
use gcg_core::pool::{Cancel, Pool, TaskCx};
use gcg_core::rng::SplitMix64;
use gcg_effects::Registry;
use gcg_engine::canon::{canonical_state, from_canonical};
use gcg_engine::observe::{determinize, information_set_key, permute_hidden};
use gcg_engine::{
    ActionKind, AddOptions, DecisionKind, GameState, Scenario, ScenarioOptions, StateSnapshot,
    Step, Zone,
};
use serde_json::Value;

use super::{Search, Shared, belief, buckets, check, view};
use crate::mcts::run;
use crate::mech::{BackupCx, Composed, WorldCx};
use crate::tree::{ChildKey, Tree};
use crate::{
    MechanismEffort, MechanismFlags, Observation, Preset, StrengthenedConfig, reference_mapping,
};

/// The searched decisions of the D2 vectors (`crates/gcg-ai/tests/vectors/search/`), as
/// `(state, player, per-decision seed)`.
fn searched_states() -> Vec<(GameState, u8, u64)> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../gcg-ai/tests/vectors/search/decisions.ndjson");
    let reg = Registry::embedded();
    std::fs::read_to_string(path)
        .expect("the vectors")
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("a record"))
        .filter(|rec| rec["search"].as_array().is_some_and(|s| !s.is_empty()))
        .map(|rec| {
            let snap = &rec["snapshot"];
            let st = from_canonical(
                &reg,
                &StateSnapshot {
                    state: snap["state"].clone(),
                    continuous: snap["continuous"].clone(),
                    filter_sets: snap["filter_sets"].clone(),
                    rules_checked: snap["rules_checked"].as_i64().expect("rules_checked"),
                },
            )
            .expect("a state");
            let player = u8::try_from(rec["player"].as_i64().expect("a seat")).expect("a seat");
            let seed = rec["seed"]
                .as_u64()
                .or_else(|| rec["seed"].as_str().and_then(|s| s.parse().ok()))
                .expect("a seed");
            (st, player, seed)
        })
        .collect()
}

/// `fast` (the reference `standard` budget) with every mechanism off but `on`.
fn config(on: &[(&str, &str)]) -> StrengthenedConfig {
    let mut cfg = reference_mapping(Preset::Fast).clone();
    cfg.mechanisms = MechanismFlags::NULL;
    for &(f, v) in on {
        assert_eq!(cfg.mechanisms.set_text(f, v), Some(Ok(())), "{f}");
    }
    assert_eq!(cfg.check_mechanisms(), Ok(()));
    cfg
}

/// One search of `cfg` at the information-set state `info` of `player`, with the package
/// state it left.
fn search_info(
    info: &GameState,
    player: u8,
    cfg: &StrengthenedConfig,
    seed: u64,
) -> (SearchResult, Search) {
    let cancel = Cancel::new();
    let composed = Composed::new(cfg);
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

/// One search of `cfg` at the observation of `player` of the true state `st`.
fn search(
    st: &GameState,
    player: u8,
    cfg: &StrengthenedConfig,
    seed: u64,
) -> (SearchResult, Search) {
    let obs = Observation::new(st, player).expect("an observation");
    search_info(obs.info(), player, cfg, seed)
}

fn bits(r: &SearchResult) -> Vec<(gcg_engine::Action, u64, u32)> {
    r.alternatives
        .iter()
        .map(|a| (a.action, a.value.to_bits(), a.visits))
        .collect()
}

#[test]
fn settings_that_do_nothing_are_refused() {
    let with = |on: &[(&str, &str)]| {
        let mut f = MechanismFlags::NULL;
        for &(name, v) in on {
            assert_eq!(f.set_text(name, v), Some(Ok(())));
        }
        check(&f)
    };
    assert_eq!(with(&[]), Ok(()));
    assert_eq!(with(&[("belief_worlds", "4")]), Ok(()));
    assert_eq!(
        with(&[("belief_worlds", "4"), ("belief_last_n", "2")]),
        Ok(())
    );
    assert_eq!(
        with(&[("belief_last_n", "1")]),
        Err("belief_last_n needs belief_worlds".to_owned())
    );
    assert_eq!(
        with(&[("belief_worlds", "1")]),
        Err("belief_worlds needs at least 2 candidate worlds".to_owned())
    );
    assert_eq!(
        with(&[("belief_worlds", "4"), ("belief_last_n", "0")]),
        Err("belief_last_n must weigh at least 1 decision".to_owned())
    );
    let mut cfg = reference_mapping(Preset::Fast).clone();
    assert_eq!(cfg.mechanisms.set_text("belief_last_n", "1"), Some(Ok(())));
    assert!(cfg.check_mechanisms().is_err());
    for refused in [("exact_pools", "true"), ("opponent_list", "archetype")] {
        let mut cfg = reference_mapping(Preset::Fast).clone();
        assert_eq!(cfg.mechanisms.set_text(refused.0, refused.1), Some(Ok(())));
        assert_eq!(
            cfg.check_mechanisms(),
            Err(format!(
                "mechanism {} (P6-C) is not available in this build",
                refused.0
            ))
        );
    }
}

#[test]
fn resampling_draws_by_cumulative_weight() {
    assert_eq!(belief::resample(&[1.0, 1.0, 1.0, 1.0], 0.0), 0);
    assert_eq!(belief::resample(&[1.0, 1.0, 1.0, 1.0], 0.49), 1);
    assert_eq!(belief::resample(&[1.0, 1.0, 1.0, 1.0], 0.999), 3);
    assert_eq!(belief::resample(&[0.0, 0.0, 5.0, 0.0], 0.0), 2);
    assert_eq!(belief::resample(&[0.0, 0.0, 5.0, 0.0], 0.99), 2);
    assert_eq!(belief::resample(&[0.0, 0.0], 0.5), 0);
    assert_eq!(belief::resample(&[1e-12, 1.0], 0.5), 1);
    // The offsets lie in [0, 1), differ between worlds and repeat for the same seed.
    let offsets: Vec<f64> = (0..64).map(|k| belief::offset(12345, k)).collect();
    assert!(offsets.iter().all(|u| (0.0..1.0).contains(u)));
    assert_eq!(
        offsets,
        (0..64)
            .map(|k| belief::offset(12345, k))
            .collect::<Vec<_>>()
    );
    let mut sorted = offsets.clone();
    sorted.sort_by(f64::total_cmp);
    sorted.dedup();
    assert_eq!(sorted.len(), 64);
    // Low discrepancy: every tenth of [0, 1) holds 6 or 7 of the 64 offsets.
    for d in 0..10 {
        let lo = f64::from(d) / 10.0;
        let n = offsets.iter().filter(|&&u| lo <= u && u < lo + 0.1).count();
        assert!((5..=8).contains(&n), "{d}: {n}");
    }
    assert_ne!(belief::offset(12345, 0), belief::offset(54321, 0));
}

/// A tree with a root child of the searcher (player 0), its node with an opponent child, and
/// that node's searcher child below.
fn three_level_tree() -> (Tree, [u32; 3]) {
    let key = |player, kind, a| ChildKey {
        player,
        kind: Some(kind),
        key: MoveKey {
            kind: a,
            a: -1,
            b: -1,
            variant: -1,
        },
    };
    let mut tree = Tree::default();
    let root = tree.new_node();
    let k0 = key(0, DecisionKind::Main, ActionKind::EndMain);
    tree.setdefault(root, k0, 0, 0.5);
    let c0 = tree.get(root, &k0).expect("root child");
    let n1 = tree.node_of(c0);
    let k1 = key(1, DecisionKind::Main, ActionKind::EndMain);
    tree.setdefault(n1, k1, 1, 0.5);
    let c1 = tree.get(n1, &k1).expect("opponent child");
    let n2 = tree.node_of(c1);
    let k2 = key(0, DecisionKind::ActionStep, ActionKind::Pass);
    tree.setdefault(n2, k2, 0, 0.5);
    let c2 = tree.get(n2, &k2).expect("searcher child");
    for (c, visits, total) in [(c0, 4, 3.0), (c1, 4, 1.0), (c2, 2, 1.5)] {
        tree.children[c as usize].visits = visits;
        tree.children[c as usize].total = total;
    }
    (tree, [c0, c1, c2])
}

#[test]
fn an_opponent_view_iteration_trains_only_the_opponent() {
    let mut flags = MechanismFlags::NULL;
    flags.opponent_view_share = 0.25;
    let (mut tree, [c0, c1, c2]) = three_level_tree();
    let shared = Shared::from_search(Search {
        player: Some(0),
        view_ok: true,
        iter_view: true,
        walk: vec![(c1, 2)],
        ..Search::default()
    });
    let path = [c0, c1, c2];
    let backed_up = super::backup(
        &flags,
        &mut BackupCx {
            tree: &mut tree,
            path: &path,
            value: 0.9,
            player: 0,
        },
        &shared,
    );
    assert!(backed_up);
    let ch = |c: u32| tree.children[c as usize];
    // The root child: one more visit at its mean (0.75).
    assert_eq!((ch(c0).visits, ch(c0).total), (5, 3.75));
    // The opponent's child: the value from its point of view.
    assert_eq!(ch(c1).visits, 5);
    assert!((ch(c1).total - 1.1).abs() < 1e-12);
    // The deeper searcher child: unchanged.
    assert_eq!((ch(c2).visits, ch(c2).total), (2, 1.5));
    let s = shared.into_search();
    let slices = s.slices[&c1];
    assert_eq!(slices.view.visits, 1);
    assert!((slices.view.total - 0.1).abs() < 1e-12);
    assert_eq!(slices.buckets[2].visits, 1);
    assert!(s.walk.is_empty());

    // A usual iteration: the caller's pooled backup runs; only the slices are added here.
    let (mut tree, [c0, c1, c2]) = three_level_tree();
    let shared = Shared::from_search(Search {
        player: Some(0),
        view_ok: true,
        walk: vec![(c1, 1)],
        ..Search::default()
    });
    let before = tree.children.clone();
    let backed_up = super::backup(
        &flags,
        &mut BackupCx {
            tree: &mut tree,
            path: &[c0, c1, c2],
            value: 0.9,
            player: 0,
        },
        &shared,
    );
    assert!(!backed_up);
    assert_eq!(
        tree.children
            .iter()
            .map(|c| (c.visits, c.total.to_bits()))
            .collect::<Vec<_>>(),
        before
            .iter()
            .map(|c| (c.visits, c.total.to_bits()))
            .collect::<Vec<_>>()
    );
    let s = shared.into_search();
    assert_eq!(s.slices[&c1].view.visits, 0);
    assert_eq!(s.slices[&c1].buckets[1].visits, 1);
}

/// `sc` with five filler Shields and a filler deck for both players.
fn filled(mut sc: Scenario) -> GameState {
    for p in 0..2u8 {
        sc.shields(p, &["GD01-060"; 5]).expect("shields");
        sc.deck(p, &["GD01-060"; 20]);
    }
    sc.start(Step::Main).expect("start");
    sc.into_state()
}

fn add(sc: &mut Scenario, player: u8, number: &str, zone: Zone, known: bool) {
    sc.add(
        player,
        number,
        zone,
        AddOptions {
            known,
            ..AddOptions::default()
        },
    )
    .unwrap_or_else(|e| panic!("add {number}: {e:?}"));
}

#[test]
fn buckets_read_castable_tricks_and_blockers() {
    let reg = registry();
    // Opponent (player 1) with three Resources; hands differ by bucket.
    let hand = |cards: &[&str], resources: usize, rested: usize| {
        let mut sc = Scenario::new(&reg, ScenarioOptions::default());
        sc.resources(1, resources, rested, 0).expect("resources");
        for &c in cards {
            add(&mut sc, 1, c, Zone::Hand, false);
        }
        buckets::bucket(&filled(sc), 1)
    };
    // ST01-014: 【Main】/【Action】 AP-3, Lv.3, cost 1. GD01-120: 【Action】 Lv.2, cost 1.
    assert_eq!(hand(&["ST01-014"], 3, 0), 2);
    assert_eq!(hand(&["GD01-120"], 3, 0), 2);
    // Level too low, or no open Resource.
    assert_eq!(hand(&["ST01-014"], 2, 0), 0);
    assert_eq!(hand(&["ST01-014"], 3, 3), 0);
    // A 【Main】-only Command is no trick; a vanilla Unit no Blocker.
    assert_eq!(hand(&["GD01-060"], 3, 0), 0);
    let blocker = blocker_unit(&reg, 3);
    assert_eq!(hand(&[&blocker], 3, 0), 1);
    assert_eq!(hand(&[&blocker, "GD01-120"], 3, 0), 3);
}

/// The embedded registry.
fn registry() -> std::sync::Arc<Registry> {
    Registry::embedded()
}

/// The first Unit card (by card number) with a printed <Blocker>, level and cost at most `lv`.
fn blocker_unit(reg: &Registry, lv: i32) -> String {
    let db = gcg_data::CardDb::embedded();
    let mut numbers: Vec<String> = db
        .real_cards()
        .filter(|cd| {
            cd.card_type == gcg_data::CardType::Unit
                && cd.level <= lv
                && cd.cost <= lv
                && reg
                    .card(cd.def_id)
                    .printed_keywords
                    .get(gcg_effects::Kw::Blocker)
                    .is_some()
        })
        .map(|cd| cd.card_number.to_string())
        .collect();
    numbers.sort();
    numbers.into_iter().next().expect("a Blocker Unit")
}

#[test]
fn opponent_view_worlds_need_root_options_the_opponent_can_identify() {
    let reg = registry();
    // The searcher (player 0) with a castable Unit in hand: its options name a hidden card.
    let mut sc = Scenario::new(&reg, ScenarioOptions::default());
    sc.resources(0, 3, 0, 0).expect("resources");
    add(&mut sc, 0, "ST01-005", Zone::Hand, false);
    add(&mut sc, 0, "GD01-013", Zone::Battle, true);
    let st = filled(sc);
    let dec = st.pending().expect("the main phase");
    assert!(dec.options.iter().any(|o| o.kind == ActionKind::PlayUnit));
    assert!(!view::root_allows(&st, &dec, 0));
    // Only an attack and the end of the main phase: allowed.
    let mut sc = Scenario::new(&reg, ScenarioOptions::default());
    add(&mut sc, 0, "GD01-013", Zone::Battle, true);
    add(&mut sc, 0, "GD03-114", Zone::Hand, false);
    let st = filled(sc);
    let dec = st.pending().expect("the main phase");
    assert!(
        dec.options
            .iter()
            .all(|o| o.kind != ActionKind::PlayCommand)
    );
    assert!(view::root_allows(&st, &dec, 0));
    // The opponent-view world keeps the searcher's public cards and the opponent's hand, and
    // redraws the searcher's hand from the opponent's pools.
    let info = Observation::new(&st, 0).expect("an observation");
    let world = determinize(info.info(), 0, 7).expect("a world");
    let mut redrawn = world.clone();
    let mut changed = 0;
    for seed in 0..32u64 {
        redrawn.clone_from_state(&world);
        view::redraw(&mut redrawn, 0, seed).expect("a redraw");
        let hand = |s: &GameState| -> Vec<gcg_data::DefId> {
            s.zone(0, Zone::Hand)
                .iter()
                .map(|&u| s.card(u).def_id)
                .collect()
        };
        changed += usize::from(hand(&redrawn) != hand(&world));
        assert_eq!(
            information_set_key(&redrawn, 1),
            information_set_key(&world, 1)
        );
        for &u in world.zone(0, Zone::Battle) {
            assert_eq!(redrawn.card(u).def_id, world.card(u).def_id);
        }
    }
    assert!(changed > 16, "{changed}");
}

/// Two true states with one information set of `player`: the second has the opponent's hidden
/// cards drawn again.
fn alternatives(st: &GameState, player: u8) -> [GameState; 2] {
    [
        determinize(st, player, 0xA11CE).expect("a world"),
        permute_hidden(st, player, 0xB0B),
    ]
}

/// Every flag alone and together searches every fourth searched decision of the D2 vectors
/// without error; the counts show how often each changes the null configuration's decision,
/// how often belief evidence and opponent-view worlds apply, and what the candidates cost.
/// Each search is also run from two other true states of the same information set, with
/// identical results (honesty, W-05 and W-07).
#[test]
fn every_flag_runs_on_the_vector_decisions_and_reads_the_information_set_only() {
    let states: Vec<(GameState, u8, u64)> = searched_states().into_iter().step_by(4).collect();
    assert!(states.len() >= 20, "{}", states.len());
    let rows: [&[(&str, &str)]; 5] = [
        &[("opponent_buckets", "8")],
        &[("belief_worlds", "4")],
        &[("belief_worlds", "8"), ("belief_last_n", "1")],
        &[("opponent_view_share", "0.25")],
        &[
            ("opponent_buckets", "8"),
            ("belief_worlds", "4"),
            ("opponent_view_share", "0.25"),
        ],
    ];
    let null = config(&[]);
    for on in rows {
        let cfg = config(on);
        let (mut changed, mut evidence, mut view_ok, mut applies, mut reweighted) =
            (0, 0, 0, 0u64, 0u64);
        for (st, player, seed) in &states {
            let (base, _) = search(st, *player, &null, *seed);
            let (r, s) = search(st, *player, &cfg, *seed);
            changed += usize::from(bits(&base) != bits(&r));
            evidence += usize::from(s.evidence.flatten().is_some());
            view_ok += usize::from(s.view_ok);
            applies += s.applies;
            reweighted += s.reweighted;
            for other in alternatives(st, *player) {
                assert_eq!(
                    information_set_key(&other, *player),
                    information_set_key(st, *player)
                );
                let (o, os) = search(&other, *player, &cfg, *seed);
                assert_eq!(bits(&o), bits(&r));
                assert_eq!(
                    (o.action, o.stats.iterations),
                    (r.action, r.stats.iterations)
                );
                assert_eq!(os, s);
            }
        }
        eprintln!(
            "{on:?}: {changed}/{} decisions changed; evidence at {evidence}, opponent-view \
             worlds allowed at {view_ok}; {reweighted} worlds reweighted, {applies} aux applies",
            states.len()
        );
        assert!(changed > 0, "{on:?} changes some decision");
    }
}

/// The belief weights: a hand of castable Units makes the end of the main phase unlikely, a
/// hand of tricks or of cards the Resources cannot pay does not.
#[test]
fn the_end_of_the_main_phase_weighs_castable_hands_down() {
    let reg = registry();
    let st = |cards: &[&str]| {
        let mut sc = Scenario::new(
            &reg,
            ScenarioOptions {
                active: 0,
                ..ScenarioOptions::default()
            },
        );
        sc.resources(1, 2, 0, 0).expect("resources");
        for &c in cards {
            add(&mut sc, 1, c, Zone::Hand, false);
        }
        let mut s = filled(sc);
        // The searcher's turn from turn 2.
        assert!(s.turn() >= 1);
        if s.turn() < 2 {
            s = with_turn(&s, 2);
        }
        s
    };
    let math = crate::mech::p6o::math::Math::Platform;
    let weight = |cards: &[&str]| {
        let s = st(cards);
        let e = belief::Evidence::of(&s, 0).expect("evidence");
        e.likelihood(&s, 0, 1.5, math)
    };
    // ST01-005 (GM, Lv.1 cost 1): castable with two Resources. GD01-120: 【Action】 only.
    // GD01-013 (Lv.4): beyond the level.
    let unit = weight(&["ST01-005", "GD01-060"]);
    let trick = weight(&["GD01-120", "GD01-013"]);
    assert!(trick > 0.99, "{trick}");
    assert!(unit < 0.05, "{unit}");
    // No open Resource: no evidence.
    let mut sc = Scenario::new(
        &reg,
        ScenarioOptions {
            active: 0,
            ..ScenarioOptions::default()
        },
    );
    sc.resources(1, 2, 2, 0).expect("resources");
    add(&mut sc, 1, "ST01-005", Zone::Hand, false);
    let s = with_turn(&filled(sc), 2);
    assert_eq!(belief::Evidence::of(&s, 0), None);
}

/// `st` with its turn number set to `turn` (a canonical round trip; the turn is public).
fn with_turn(st: &GameState, turn: i64) -> GameState {
    let mut j: Value = serde_json::from_str(&gcg_core::pyjson::dumps(
        &canonical_state(st),
        gcg_core::pyjson::DumpOptions {
            indent: None,
            sort_keys: false,
            compact: true,
            ensure_ascii: true,
        },
    ))
    .expect("json");
    j["turn"] = Value::from(turn);
    from_canonical(
        st.registry(),
        &StateSnapshot {
            state: j,
            continuous: Value::Object(serde_json::Map::new()),
            filter_sets: Value::Object(serde_json::Map::new()),
            rules_checked: st.rules_checked(),
        },
    )
    .expect("a state")
}

/// World k of `belief_worlds` is one of its M candidates, candidate 0 being the reference's
/// world, and the same seeds give the same world.
#[test]
fn belief_worlds_are_candidates_of_the_world_seed() {
    let states: Vec<(GameState, u8, u64)> = searched_states().into_iter().step_by(5).collect();
    let cfg = config(&[("belief_worlds", "4")]);
    let mut differ = 0;
    let mut with_evidence = 0;
    for (st, player, seed) in &states {
        let obs = Observation::new(st, *player).expect("an observation");
        let Some(e) = belief::Evidence::of(obs.info(), *player) else {
            continue;
        };
        with_evidence += 1;
        let mut rng = SplitMix64::new(*seed);
        let seeds: Vec<u64> = (0..16).map(|_| rng.next_u64()).collect();
        for k in 0..16 {
            let cx = WorldCx {
                st: obs.info(),
                player: *player,
                cfg: &cfg,
                seeds: &seeds,
            };
            let mut out = obs.info().clone();
            let (scored, changed) = belief::world(&cx, k, 4, &e, &mut out).expect("a world");
            assert_eq!(scored, 4);
            let mut again = obs.info().clone();
            belief::world(&cx, k, 4, &e, &mut again).expect("a world");
            assert_eq!(canonical_state(&again), canonical_state(&out));
            let reference = determinize(obs.info(), *player, seeds[k]).expect("a world");
            assert_eq!(
                changed,
                canonical_state(&reference) != canonical_state(&out)
            );
            differ += usize::from(changed);
        }
    }
    eprintln!("belief worlds: {with_evidence} decisions with evidence, {differ} worlds changed");
    assert!(with_evidence > 0);
}

/// The opponent slices of one search of `cfg` at the information-set state of `st`'s player 0,
/// at a budget of `iterations`.
fn slices_after(
    st: &GameState,
    on: &[(&str, &str)],
    iterations: u32,
    seed: u64,
) -> (SearchResult, Search) {
    let mut cfg = config(on);
    cfg.min_iterations = iterations;
    cfg.max_iterations = iterations;
    search(st, 0, &cfg, seed)
}

/// Sums the slot `pick` over the opponent children whose key `want` accepts.
fn sum_slots(
    s: &Search,
    want: impl Fn(&ChildKey) -> bool,
    pick: impl Fn(&super::Slices) -> super::Slot,
) -> super::Slot {
    let mut out = super::Slot::default();
    for e in s.slices.values() {
        if e.key.as_ref().is_some_and(&want) {
            let x = pick(e);
            out.visits += x.visits;
            out.total += x.total;
        }
    }
    out
}

/// The W-05 position: the searcher's Gundam (GD01-013, AP 3, HP 4) can attack the player; the
/// defender has a GD02-079 Blocker (AP 3, HP 3), three active Resources and one hidden card;
/// 13 of its 46 unseen cards (the 26 placed and the testkit's 20 filler Units) are GD02-118
/// (【Action】 an enemy Unit with 4 or less HP battling a friendly <Blocker> Unit returns to its
/// owner's hand), so about 28% of the worlds hold the trick. With the trick the defender's best
/// reply to the attack is to block and bounce Gundam; without it a block only loses the
/// Blocker.
fn w05_fusion() -> GameState {
    let reg = registry();
    let mut sc = Scenario::new(&reg, ScenarioOptions::default());
    add(&mut sc, 0, "GD01-013", Zone::Battle, true);
    sc.resources(0, 3, 0, 0).expect("resources");
    sc.shields(0, &["GD01-060"; 5]).expect("shields");
    sc.deck(0, &["GD01-060"; 20]);
    add(&mut sc, 1, "GD02-079", Zone::Battle, true);
    sc.resources(1, 3, 0, 0).expect("resources");
    let pool: Vec<&str> = (0..26)
        .map(|i| if i % 2 == 0 { "GD02-118" } else { "GD01-060" })
        .collect();
    add(&mut sc, 1, pool[0], Zone::Hand, false);
    sc.shields(1, &pool[1..6]).expect("shields");
    sc.deck(1, &pool[6..]);
    sc.start(Step::Main).expect("start");
    sc.into_state()
}

/// The visits to Block among the visits to the defender's block decision, per bucket (0: no
/// castable trick, 2: the trick), over every block node of the tree.
fn block_shares(s: &Search) -> [(u32, u32); 2] {
    [0usize, 2].map(|b| {
        let at = |kind: ActionKind| {
            sum_slots(
                s,
                |k| k.kind == Some(DecisionKind::Block) && k.key.kind == kind,
                |e| e.buckets[b],
            )
            .visits
        };
        let block = at(ActionKind::Block);
        (block, block + at(ActionKind::NoBlock))
    })
}

/// W-05 (T-105, strategy fusion) at the opponent's node: with `opponent_buckets` the
/// defender's reply to the attack follows its hand (it blocks in the trick worlds and lets the
/// attack through otherwise); with pooled statistics (a bucket threshold no node reaches, so
/// the reference's selection runs and the slices only record) it gives one reply for both.
#[test]
fn w05_bucketed_opponent_nodes_reply_by_hand() {
    let st = w05_fusion();
    let mut shares = Vec::new();
    for m in ["4", "4294967295"] {
        for seed in [1u64, 2] {
            let (_, s) = slices_after(&st, &[("opponent_buckets", m)], 2000, seed);
            shares.push(block_shares(&s));
        }
    }
    eprintln!(
        "W-05 block visits (no trick, trick): bucketed {:?}, pooled {:?}",
        &shares[..2],
        &shares[2..]
    );
    // Recorded 2026-11-02: bucketed [(50, 724), (256, 276)], [(60, 725), (253, 275)]; pooled
    // [(267, 724), (110, 276)], [(129, 725), (36, 275)].
    let share = |(a, n): (u32, u32)| f64::from(a) / f64::from(n);
    for [none, trick] in &shares[..2] {
        assert!(
            share(*none) < 0.15 && share(*trick) > 0.85,
            "{none:?} {trick:?}"
        );
    }
    for [none, trick] in &shares[2..] {
        assert!(
            share(*trick) < 0.5,
            "pooled: no block with the trick {trick:?}"
        );
        assert!(
            (share(*none) - share(*trick)).abs() < 0.1,
            "{none:?} {trick:?}"
        );
    }
}

/// The W-07 position: the searcher's turn 3; the opponent passed its main phase with two of
/// its four Resources open and two hidden cards in hand. Its 26 unseen cards are 4 GD01-120
/// (【Action】 only, so the open Resources do not show it), 18 GD01-060 (Lv.2, cost 1: the
/// opponent would have played one) and 4 GD03-016 (Lv.5: beyond its level).
fn w07_open_resources() -> GameState {
    let reg = registry();
    let mut sc = Scenario::new(
        &reg,
        ScenarioOptions {
            deck_size: 0,
            ..ScenarioOptions::default()
        },
    );
    add(&mut sc, 0, "GD01-013", Zone::Battle, true);
    sc.resources(0, 3, 0, 0).expect("resources");
    sc.shields(0, &["GD01-060"; 5]).expect("shields");
    sc.deck(0, &["GD01-060"; 20]);
    add(&mut sc, 1, "ST01-008", Zone::Battle, true);
    sc.resources(1, 4, 2, 0).expect("resources");
    let pool: Vec<&str> = (0..26)
        .map(|i| match i % 13 {
            0 | 7 => "GD01-120",
            3 | 10 => "GD03-016",
            _ => "GD01-060",
        })
        .collect();
    add(&mut sc, 1, pool[0], Zone::Hand, false);
    add(&mut sc, 1, pool[13], Zone::Hand, false);
    sc.shields(1, &pool[1..6]).expect("shields");
    let mut deck: Vec<&str> = pool[6..13].to_vec();
    deck.extend(&pool[14..]);
    sc.deck(1, &deck);
    sc.start(Step::Main).expect("start");
    sc.into_state()
}

/// W-07 (T-107, inference from open Resources): among the first 400 worlds of the search, the
/// worlds of `belief_worlds` give the opponent a hand without a castable card, and so the
/// trick, far more often than the reference's uniform worlds.
#[test]
fn w07_belief_worlds_infer_the_trick_from_the_open_resources() {
    let st = w07_open_resources();
    let obs = Observation::new(&st, 0).expect("an observation");
    let info = obs.info();
    let e = belief::Evidence::of(info, 0).expect("the opponent's main phase is shown");
    let has = |w: &GameState, number: &str| {
        w.zone(1, Zone::Hand)
            .iter()
            .any(|&u| w.def_of(u).card_number.as_ref() == number)
    };
    let mut rng = SplitMix64::new(20_261_102);
    let seeds: Vec<u64> = (0..400).map(|_| rng.next_u64()).collect();
    let mut rows = Vec::new();
    for m in [None, Some(4), Some(8), Some(16)] {
        let cfg = config(&match m {
            None => vec![],
            Some(4) => vec![("belief_worlds", "4")],
            Some(8) => vec![("belief_worlds", "8")],
            Some(_) => vec![("belief_worlds", "16")],
        });
        let (mut trick, mut castable) = (0, 0);
        for k in 0..seeds.len() {
            let mut cx = WorldCx {
                st: info,
                player: 0,
                cfg: &cfg,
                seeds: &seeds,
            };
            let mut w = info.clone();
            match m {
                Some(m) => {
                    belief::world(&cx, k, m, &e, &mut w).expect("a world");
                }
                None => crate::mech::reference::world(&mut cx, k, &mut w).expect("a world"),
            }
            trick += usize::from(has(&w, "GD01-120"));
            castable += usize::from(has(&w, "GD01-060"));
        }
        rows.push((m, trick, castable));
    }
    eprintln!("W-07 worlds of 400 (M, with the trick, with a castable Unit): {rows:?}");
    let (_, uniform, uniform_castable) = rows[0];
    let (_, m8, m8_castable) = rows[2];
    let (_, m16, _) = rows[3];
    // Recorded 2026-11-02: (None, 121, 368), (4, 178, 281), (8, 224, 201), (16, 243, 154): the
    // uniform share is the pools' 1 - C(22,2)/C(26,2) = 0.29.
    assert!((95..=150).contains(&uniform), "uniform {uniform}");
    assert!(uniform_castable > 330, "{uniform_castable}");
    assert!(m8 >= uniform + 80, "M = 8: {m8} vs {uniform}");
    assert!(m8_castable + 120 <= uniform_castable, "{m8_castable}");
    assert!(m16 >= m8, "{m16} vs {m8}");
}

/// The W-139 position: the searcher's GD02-078 (AP 5, HP 4) has attacked and is rested; the
/// searcher holds GD04-113 (【Action】 a friendly Unit receives 3 less battle damage during the
/// battle) with two of its four Resources open, and GD01-053 can attack or spend one Resource on
/// its 【Activate･Main】; the opponent has GD03-016 (AP 5, HP 4), which can attack the rested
/// Unit next turn: without the trick the two Units trade, with it only the attacker is
/// destroyed. No option of the root decision names a hidden card, so opponent-view worlds are
/// used.
fn w139_held_trick() -> GameState {
    let reg = registry();
    let mut sc = Scenario::new(&reg, ScenarioOptions::default());
    sc.add(
        0,
        "GD02-078",
        Zone::Battle,
        AddOptions {
            rested: true,
            ..AddOptions::default()
        },
    )
    .expect("the rested Unit");
    add(&mut sc, 0, "GD01-053", Zone::Battle, true);
    sc.resources(0, 4, 2, 0).expect("resources");
    add(&mut sc, 0, "GD04-113", Zone::Hand, false);
    sc.shields(0, &["GD01-060"; 5]).expect("shields");
    sc.deck(0, &["GD01-060"; 20]);
    add(&mut sc, 1, "GD03-016", Zone::Battle, true);
    add(&mut sc, 1, "GD01-060", Zone::Battle, true);
    sc.resources(1, 5, 0, 0).expect("resources");
    add(&mut sc, 1, "GD01-060", Zone::Hand, false);
    add(&mut sc, 1, "GD01-060", Zone::Hand, false);
    sc.shields(1, &["GD01-060"; 5]).expect("shields");
    sc.deck(1, &["GD01-060"; 20]);
    sc.start(Step::Main).expect("start");
    sc.into_state()
}

/// W-139 (T-139, the held trick) in the search's worlds: with `opponent_view_share = 0.25`,
/// every fourth world (3, 7, 11, ...) is redrawn from the opponent's view. In the usual worlds
/// the searcher always holds its trick (the reference's single-observer worlds); in the
/// opponent-view worlds it holds it at the frequency the opponent's pools give, while the
/// opponent's information set is the usual world's, so the opponent's nodes, which select by
/// those iterations, play as an opponent who cannot see the searcher's hand.
#[test]
fn w139_opponent_view_worlds_hide_the_held_trick() {
    let st = w139_held_trick();
    let dec = st.pending().expect("the main phase");
    assert!(view::root_allows(&st, &dec, 0));
    let obs = Observation::new(&st, 0).expect("an observation");
    let info = obs.info();
    let cfg = config(&[("opponent_view_share", "0.25")]);
    let shared = Shared::from_search(Search {
        player: Some(0),
        view_ok: true,
        ..Search::default()
    });
    let mut rng = SplitMix64::new(20_261_139);
    let seeds: Vec<u64> = (0..400).map(|_| rng.next_u64()).collect();
    let holds = |w: &GameState| {
        w.zone(0, Zone::Hand)
            .iter()
            .any(|&u| w.def_of(u).card_number.as_ref() == "GD04-113")
    };
    let (mut usual, mut usual_trick, mut view_n, mut view_trick) = (0, 0, 0, 0);
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
        if view::is_view_world(k, 0.25) {
            view_n += 1;
            view_trick += usize::from(holds(&w));
            assert_eq!(
                information_set_key(&w, 1),
                information_set_key(&reference, 1),
                "the opponent's information set is the usual world's"
            );
            for &u in reference.zone(0, Zone::Battle) {
                assert_eq!(w.card(u).def_id, reference.card(u).def_id);
            }
        } else {
            usual += 1;
            usual_trick += usize::from(holds(&w));
            assert_eq!(canonical_state(&w), canonical_state(&reference));
        }
    }
    eprintln!(
        "W-139 worlds: usual {usual_trick}/{usual} hold the trick, opponent-view {view_trick}/{view_n}"
    );
    // Recorded 2026-11-02: usual 300/300, opponent-view 2/100 (1 trick among the 46 cards the
    // opponent cannot identify).
    assert_eq!((usual, view_n), (300, 100));
    assert_eq!(usual_trick, usual);
    assert!(view_trick <= 10, "{view_trick}");
    assert_eq!(shared.into_search().view_worlds, 100);
}
