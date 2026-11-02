//! The composition of the packages' mechanisms in [`Composed`] (Phase 6 wave 2 integration):
//! the engine applies of root judgements (P6-E) and pre-game simulations (P6-H) reach the
//! reported effort, and the root trees of `root_trees` (P6-D) see the root findings of P6-E and
//! report the look-ahead applies of P6-F.

use std::path::PathBuf;

use gcg_ai::{Agent, AgentCx, Scratch};
use gcg_core::pool::{Cancel, Pool, TaskCx};
use gcg_core::rng::SplitMix64;
use gcg_effects::Registry;
use gcg_engine::canon::from_canonical;
use gcg_engine::{GameState, StateSnapshot};
use serde_json::Value;

use super::tests::{config, run_with, searched_states};
use super::{Composed, Mechanisms};
use crate::mcts::{pregame_with_applies, search_with_carry};
use crate::{
    MechanismFlags, Observation, Preset, StrengthenedConfig, make_search_agent, preset_config,
};

/// The pre-game decisions of `kind` in the D2 vectors, as `(state, player, per-decision seed)`.
fn pregame_states(kind: &str) -> Vec<(GameState, u8, u64)> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../gcg-ai/tests/vectors/search/decisions.ndjson");
    let reg = Registry::embedded();
    std::fs::read_to_string(path)
        .expect("the vectors")
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("a record"))
        .filter(|rec| rec["kind"].as_str() == Some(kind))
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

fn inline<T>(f: impl FnOnce(&mut AgentCx<'_>) -> T) -> T {
    let cancel = Cancel::new();
    Pool::run_inline(|scratch: &mut Scratch, par: &TaskCx<'_, Scratch>| {
        let mut cx = AgentCx {
            scratch,
            par,
            cancel: &cancel,
        };
        f(&mut cx)
    })
}

#[test]
fn root_judgements_are_the_aux_applies_of_a_p6e_search() {
    let null = config(&[]);
    let mut cfg = config(&["branch_all_selects", "real_idle_line"]);
    assert_eq!(cfg.mechanisms.set_text("prune_worlds", "4"), Some(Ok(())));
    let mut judged = 0;
    for (st, player, seed) in searched_states(24) {
        let (r, _) = run_with(&Composed::new(&null), &st, player, &null, seed).expect("a search");
        assert_eq!(r.stats.effort.applies_aux, 0);
        let mech = Composed::new(&cfg);
        let (r, _) = run_with(&mech, &st, player, &cfg, seed).expect("a search");
        assert_eq!(r.stats.effort.applies_aux, mech.root_applies());
        judged += usize::from(mech.root_applies() > 0);
    }
    assert!(judged > 0, "no decision judged a root move");
}

#[test]
fn pre_game_simulations_report_their_applies_and_the_reference_reports_none() {
    let mut on = preset_config(Preset::Fast).clone();
    on.mechanisms = MechanismFlags::NULL;
    on.mechanisms.paired_mulligan = true;
    on.mechanisms.exclude_capped_play_draw = true;
    let mut off = on.clone();
    off.mechanisms = MechanismFlags::NULL;
    let mut seen = 0;
    for kind in ["redraw", "choose_first"] {
        for (st, player, _) in pregame_states(kind) {
            for (cfg, simulates) in [(&on, true), (&off, false)] {
                let mut agent = make_search_agent(cfg, 7, false);
                let (_, trace) =
                    inline(|cx| agent.choose_traced(&st, player, cx, &mut gcg_ai::NoTrace))
                        .expect("a pre-game choice");
                let trace = trace.expect("a decision trace");
                let obs = Observation::new(&st, player).expect("an observation");
                let (_, applies) = inline(|cx| {
                    pregame_with_applies(&obs, cfg, &mut SplitMix64::new(trace.seed), cx)
                })
                .expect("a pre-game choice");
                if !simulates {
                    assert_eq!(applies, 0);
                    assert!(
                        trace.search.is_none(),
                        "{kind}: the reference reports no search"
                    );
                    continue;
                }
                assert!(applies > 0, "{kind}: the simulations apply");
                let search = trace.search.expect("the simulations' effort");
                assert_eq!(
                    (search.iterations, search.budget, search.cap),
                    (0, 0, 0),
                    "{kind}"
                );
                assert!(search.roots.is_empty());
                assert_eq!(search.effort.applies_aux, applies, "{kind}");
                assert_eq!(
                    search.effort.applies_tree + search.effort.applies_rollout,
                    0
                );
                seen += 1;
            }
        }
    }
    assert!(seen > 0);
}

/// One search of `cfg` through the core's entry point (the root ensemble when `root_trees` is
/// set), with the composed mechanisms the entry point builds.
fn search(cfg: &StrengthenedConfig, st: &GameState, player: u8, seed: u64) -> gcg_ai::SearchResult {
    let obs = Observation::new(st, player).expect("an observation");
    inline(|cx| {
        search_with_carry(
            &obs,
            cfg,
            &mut SplitMix64::new(seed),
            cx,
            &mut gcg_ai::NoTrace,
            None,
        )
    })
    .expect("a search")
    .0
}

#[test]
fn root_trees_count_the_look_ahead_applies_of_every_tree() {
    let mut one_tree = config(&["exact_fight", "modal_lookahead", "yes_no_lookahead"]);
    let mut two_trees = one_tree.clone();
    assert_eq!(
        one_tree.mechanisms.set_text("root_trees", "1"),
        Some(Ok(()))
    );
    assert_eq!(
        two_trees.mechanisms.set_text("root_trees", "2"),
        Some(Ok(()))
    );
    let mut null = config(&[]);
    assert_eq!(null.mechanisms.set_text("root_trees", "2"), Some(Ok(())));
    let mut looked = [0, 0];
    for (st, player, seed) in searched_states(16) {
        assert_eq!(search(&null, &st, player, seed).stats.effort.applies_aux, 0);
        for (n, cfg) in [&one_tree, &two_trees].into_iter().enumerate() {
            let r = search(cfg, &st, player, seed);
            looked[n] += usize::from(r.stats.effort.applies_aux > 0);
        }
    }
    assert!(
        looked.iter().all(|&n| n > 0),
        "decisions whose root trees reported look-ahead applies: {looked:?}"
    );
}

#[test]
fn a_root_tree_starts_from_the_root_findings() {
    let mut cfg = config(&["ex_variants", "branch_all_selects"]);
    assert_eq!(cfg.mechanisms.set_text("prune_worlds", "4"), Some(Ok(())));
    for (st, player, seed) in searched_states(24) {
        let mech = Composed::new(&cfg);
        let _ = run_with(&mech, &st, player, &cfg, seed).expect("a search");
        let before = mech.p6e.info();
        let tree = Composed::with_state(&cfg, mech.tree_state());
        assert_eq!(tree.p6e.info(), before);
        assert_eq!(tree.aux_applies(), 0);
        assert_eq!(tree.into_state().p6e, before);
    }
}
