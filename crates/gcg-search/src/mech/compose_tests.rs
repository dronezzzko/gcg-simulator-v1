//! The composition of the packages' mechanisms in [`Composed`] (Phase 6 wave 2 integration):
//! the engine applies of root judgements (P6-E) and pre-game simulations (P6-H) reach the
//! reported effort, and the root trees of `root_trees` (P6-D) see the root findings of P6-E and
//! report the look-ahead applies of P6-F. Wave 3: the leaf mechanisms of P6-A, P6-B and P6-G
//! compose in the documented order, and each leaf flag alone is its package's own leaf.

use std::path::PathBuf;

use gcg_ai::{
    Agent, AgentCx, AgentError, Alternative, MoveKey, Scratch, SearchResult, burst_density,
    play_until_turn,
};
use gcg_core::pool::{Cancel, Pool, TaskCx};
use gcg_core::rng::SplitMix64;
use gcg_effects::Registry;
use gcg_engine::canon::from_canonical;
use gcg_engine::observe::determinize_into;
use gcg_engine::{Action, EngineError, GameState, StateSnapshot};
use serde_json::Value;

use super::tests::{config, run_with, searched_states};
use super::{
    BackupCx, Budget, ChildIdx, Composed, HorizonDecision, LeafCx, Mechanisms, NodeCx, PregameCx,
    RolloutCx, RootCx, WorldCx, math_of, p6a, p6b, p6g, reference, validate,
};
use crate::mcts::{pregame_with_applies, search_with_carry};
use crate::{
    MechanismEffort, MechanismFlags, Observation, Preset, StrengthenedConfig, make_search_agent,
    preset_config,
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

/// The leaf flags of P6-A, P6-B and P6-G, each with the package whose own leaf it is.
const LEAF_FLAGS: [(&str, Pkg); 15] = [
    ("generalized_leaf", Pkg::A),
    ("capped_rollout_continuation", Pkg::A),
    ("adaptive_horizon", Pkg::A),
    ("threat_leaf", Pkg::B),
    ("effective_blockers", Pkg::B),
    ("solver_can_kill", Pkg::B),
    ("leaf_density", Pkg::G),
    ("refit_weights", Pkg::G),
    ("determinized_fit", Pkg::G),
    ("no_tempo_feature", Pkg::G),
    ("keyword_features", Pkg::G),
    ("ability_count_features", Pkg::G),
    ("hand_slots", Pkg::G),
    ("burst_shield_probability", Pkg::G),
    ("phase_weights", Pkg::G),
];

/// A package with a leaf mechanism.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Pkg {
    A,
    B,
    G,
}

/// The composed mechanisms with one package's own leaf in place of the composed one: the
/// search of that package's branch.
struct OwnLeaf<'c> {
    mech: Composed<'c>,
    pkg: Pkg,
}

impl Mechanisms for OwnLeaf<'_> {
    fn root_moves(&self, cx: &mut RootCx<'_>) -> Result<Vec<(MoveKey, Action)>, EngineError> {
        self.mech.root_moves(cx)
    }

    fn root_applies(&self) -> u64 {
        self.mech.root_applies()
    }

    fn budget(&self, cx: &RootCx<'_>, n_moves: usize) -> Budget {
        self.mech.budget(cx, n_moves)
    }

    fn world(
        &self,
        cx: &mut WorldCx<'_>,
        k: usize,
        out: &mut GameState,
    ) -> Result<(), EngineError> {
        self.mech.world(cx, k, out)
    }

    fn priors(&self, cx: &mut NodeCx<'_>) -> Result<(), EngineError> {
        self.mech.priors(cx)
    }

    fn select(&self, cx: &mut NodeCx<'_>) -> ChildIdx {
        self.mech.select(cx)
    }

    fn rollout_choice(&self, cx: &mut RolloutCx<'_>) -> Result<Action, EngineError> {
        self.mech.rollout_choice(cx)
    }

    fn horizon(&self, cx: &LeafCx<'_>) -> HorizonDecision {
        self.mech.horizon(cx)
    }

    fn leaf_value(&self, cx: &mut LeafCx<'_>) -> Result<f64, EngineError> {
        own_leaf(self.pkg, &self.mech, cx)
    }

    fn error_leaf(&self, cx: &LeafCx<'_>, err: &EngineError) -> Option<f64> {
        self.mech.error_leaf(cx, err)
    }

    fn backup(&self, cx: &mut BackupCx<'_>) {
        self.mech.backup(cx);
    }

    fn pregame(
        &self,
        cx: &mut PregameCx<'_, '_>,
    ) -> Result<(Action, Vec<Alternative>), AgentError> {
        self.mech.pregame(cx)
    }
}

/// `pkg`'s own leaf with `mech`'s package state.
fn own_leaf(pkg: Pkg, mech: &Composed<'_>, cx: &mut LeafCx<'_>) -> Result<f64, EngineError> {
    match pkg {
        Pkg::A => p6a::leaf_value(&mech.p6a, cx),
        Pkg::B => p6b::leaf::leaf_value(&mech.p6b, mech.flags, cx),
        Pkg::G => p6g::leaf_value(mech.flags, cx),
    }
}

/// The configuration with the leaf flags `flags` (and a short rollout cap when `capped`).
fn leaf_config(flags: &[&str], capped: bool) -> StrengthenedConfig {
    let mut cfg = config(&["portable_math"]);
    for f in flags {
        assert_eq!(cfg.mechanisms.set_text(f, "true"), Some(Ok(())), "{f}");
    }
    if capped {
        cfg.max_rollout_decisions = 6;
    }
    assert_eq!(validate(&cfg.mechanisms), Ok(()), "{flags:?}");
    cfg
}

fn result_bits(r: &SearchResult) -> (Action, Vec<(Action, u64, u32)>, String) {
    let alts = r
        .alternatives
        .iter()
        .map(|a| (a.action, a.value.to_bits(), a.visits))
        .collect();
    (r.action, alts, format!("{:?}", r.stats))
}

/// The leaves a search of `st` scores, as the core builds them: the first world of the
/// observer's information-set state, played by the reference rollout policy to the horizon
/// (or to the decision cap: a capped line), with the root's Burst density.
fn leaves(n: usize, cfg: &StrengthenedConfig) -> Vec<(GameState, GameState, u8, bool)> {
    let mut out = Vec::new();
    for (st, player, seed) in searched_states(n) {
        let obs = Observation::new(&st, player).expect("an observation");
        let mut world = st.clone();
        determinize_into(obs.info(), player, seed, &mut world).expect("a world");
        let horizon = st.turn() + cfg.horizon_turns;
        let mut rng = SplitMix64::new(seed);
        let mut leaf = world;
        let n = play_until_turn(
            &mut leaf,
            horizon,
            Some(&mut rng),
            cfg.rollout_temperature,
            cfg.max_rollout_decisions,
            &cfg.knobs(),
        )
        .expect("a rollout");
        let capped =
            n >= cfg.max_rollout_decisions && leaf.winner().is_none() && leaf.turn() < horizon;
        out.push((obs.info().clone(), leaf, player, capped));
    }
    out
}

/// Each leaf flag alone, on the leaves of every searched position (finished, capped and
/// proved-lethal lines among them): the composed leaf value is its package's own leaf, bit for
/// bit, with the same solver and continuation counts; the null configuration's is the
/// reference's.
#[test]
fn each_leaf_flag_alone_is_its_packages_leaf() {
    let mut proved = 0;
    let mut finished = 0;
    for capped in [false, true] {
        let base = leaf_config(&[], capped);
        let positions = leaves(usize::MAX, &base);
        assert!(positions.len() > 100, "{} positions", positions.len());
        let mut capped_lines = 0;
        for (flag, pkg) in LEAF_FLAGS
            .iter()
            .map(|&(f, p)| (Some(f), Some(p)))
            .chain([(None, None)])
        {
            let cfg = leaf_config(flag.as_slice(), capped);
            for (info, leaf, player, is_capped) in &positions {
                let leaf_cx = || LeafCx {
                    s: leaf,
                    player: *player,
                    root_turn: info.turn(),
                    horizon: info.turn() + cfg.horizon_turns,
                    decisions: 0,
                    capped: *is_capped,
                    cfg: &cfg,
                    weights: &cfg.weights,
                    density: burst_density(info, *player),
                    math: math_of(&cfg),
                };
                let solver_cap = Budget {
                    iterations: 64,
                    cap: 64,
                };
                let dec = info.pending().expect("a decision");
                let mut rng = SplitMix64::new(1);
                let root = RootCx {
                    st: info,
                    dec: &dec,
                    player: *player,
                    cfg: &cfg,
                    knobs: cfg.knobs(),
                    rng: &mut rng,
                };
                let composed = Composed::new(&cfg);
                let own = Composed::new(&cfg);
                let _ = p6b::budget(&composed.p6b, composed.flags, &root, solver_cap);
                let _ = p6b::budget(&own.p6b, own.flags, &root, solver_cap);
                let got = composed.leaf_value(&mut leaf_cx()).expect("a leaf value");
                let want = match pkg {
                    Some(p) => own_leaf(p, &own, &mut leaf_cx()),
                    None => reference::leaf_value(&mut leaf_cx()),
                }
                .expect("a leaf value");
                assert_eq!(got.to_bits(), want.to_bits(), "{flag:?} capped={capped}");
                assert_eq!(composed.p6a.counts(), own.p6a.counts(), "{flag:?}");
                assert_eq!(
                    composed.p6b.aux_applies(),
                    own.p6b.aux_applies(),
                    "{flag:?}"
                );
                if flag == Some("threat_leaf") && leaf.winner().is_none() {
                    let turns = f64::from(leaf.turn() - info.turn());
                    let win = 1.0 - reference::WIN_DECAY * turns;
                    proved += usize::from(got == win || got == reference::WIN_DECAY * turns);
                }
                if flag.is_none() {
                    finished += usize::from(leaf.winner().is_some());
                    capped_lines += usize::from(*is_capped);
                }
            }
        }
        assert_eq!(capped, capped_lines > 0, "capped lines: {capped_lines}");
    }
    assert!(proved > 0, "no leaf with a proved lethal");
    assert!(finished > 0, "no finished line");
}

/// Each leaf flag alone: the composed search equals the search with that package's own leaf
/// (the search of its branch), every root value and visit, the statistics and the counts.
#[test]
fn each_leaf_flag_alone_searches_as_its_package() {
    let mut differs = Vec::new();
    for (flag, pkg) in LEAF_FLAGS {
        let capped = flag == "capped_rollout_continuation";
        let cfg = leaf_config(&[flag], capped);
        let null = leaf_config(&[], capped);
        let mut changed = 0;
        for (st, player, seed) in searched_states(32) {
            let own = OwnLeaf {
                mech: Composed::new(&cfg),
                pkg,
            };
            let (a, ea) = run_with(&own, &st, player, &cfg, seed).expect("a search");
            let composed = Composed::new(&cfg);
            let (b, eb) = run_with(&composed, &st, player, &cfg, seed).expect("a search");
            assert_eq!(result_bits(&a), result_bits(&b), "{flag}");
            assert_eq!(ea, eb, "{flag}");
            assert_eq!(own.mech.aux_applies(), composed.aux_applies(), "{flag}");
            let (r, _) =
                run_with(&Composed::new(&null), &st, player, &null, seed).expect("a search");
            changed += usize::from(result_bits(&r) != result_bits(&b));
        }
        differs.push((flag, changed));
    }
    // A finished or proved-lethal leaf decides no search of these 32 (the leaf test covers
    // them); every other flag changes some search, so the comparison is not vacuous.
    for (flag, changed) in differs {
        assert_eq!(
            changed > 0,
            !matches!(flag, "generalized_leaf" | "threat_leaf"),
            "{flag}: {changed} searches changed"
        );
    }
}

/// Leaf flags of different packages compose: the swaps of P6-B change a P6-G model's leaf,
/// the generalised mapping keeps the estimate of an unfinished line, and every value is a
/// probability.
#[test]
fn leaf_flags_of_different_packages_compose() {
    let base = leaf_config(&[], false);
    let positions = leaves(40, &base);
    let model = leaf_config(&["refit_weights"], false);
    let swapped = leaf_config(
        &["refit_weights", "effective_blockers", "solver_can_kill"],
        false,
    );
    let general = leaf_config(&["refit_weights", "generalized_leaf"], false);
    let all = leaf_config(
        &[
            "generalized_leaf",
            "capped_rollout_continuation",
            "threat_leaf",
            "effective_blockers",
            "solver_can_kill",
            "leaf_density",
            "refit_weights",
            "phase_weights",
        ],
        false,
    );
    let mut changed = 0;
    for (info, leaf, player, capped) in &positions {
        let value = |cfg: &StrengthenedConfig| {
            let mech = Composed::new(cfg);
            let mut cx = LeafCx {
                s: leaf,
                player: *player,
                root_turn: info.turn(),
                horizon: info.turn() + cfg.horizon_turns,
                decisions: 0,
                capped: *capped,
                cfg,
                weights: &cfg.weights,
                density: burst_density(info, *player),
                math: math_of(cfg),
            };
            mech.leaf_value(&mut cx).expect("a leaf value")
        };
        let m = value(&model);
        changed += usize::from(value(&swapped).to_bits() != m.to_bits());
        if leaf.winner().is_none() {
            assert_eq!(value(&general).to_bits(), m.to_bits());
        }
        let v = value(&all);
        assert!((0.0..=1.0).contains(&v), "{v}");
    }
    assert!(
        changed > 0,
        "P6-B's feature swaps never changed the model's leaf"
    );
}

/// P6-A's and P6-C's counters reach the search's `MechanismEffort` through the core's entry
/// point, with one tree and with root trees, and equal the counts of the composed mechanisms
/// of a single-tree search; the null configuration counts nothing.
#[test]
fn mechanism_counters_reach_the_search_effort() {
    type Count = fn(&MechanismEffort) -> u64;
    let cases: [(&str, &str, Count); 4] = [
        ("adaptive_horizon", "true", |e| e.horizon_extensions),
        ("capped_rollout_continuation", "true", |e| {
            e.capped_continued
        }),
        ("belief_worlds", "4", |e| e.belief_reweighted_worlds),
        ("opponent_view_share", "0.25", |e| e.opponent_view_worlds),
    ];
    let states = searched_states(16);
    for (name, value, count) in cases {
        let mut cfg = config(&[]);
        assert_eq!(cfg.mechanisms.set_text(name, value), Some(Ok(())), "{name}");
        if name == "capped_rollout_continuation" {
            cfg.max_rollout_decisions = 6;
        }
        let mut trees = cfg.clone();
        assert_eq!(trees.mechanisms.set_text("root_trees", "2"), Some(Ok(())));
        let (mut single, mut ensemble) = (0, 0);
        for (st, player, seed) in &states {
            let e = search_effort(&cfg, st, *player, *seed);
            let mech = Composed::new(&cfg);
            let _ = run_with(&mech, st, *player, &cfg, *seed).expect("a search");
            assert_eq!(e, mech.effort(), "{name}");
            assert!(e.capped_unsettled <= e.capped_continued);
            single += count(&e);
            ensemble += count(&search_effort(&trees, st, *player, *seed));
        }
        assert!(single > 0 && ensemble > 0, "{name}: {single} {ensemble}");
    }
    let null = config(&[]);
    for (st, player, seed) in &states {
        assert_eq!(
            search_effort(&null, st, *player, *seed),
            MechanismEffort::default()
        );
    }
}

/// The mechanism counters of one search of `cfg` through the core's entry point.
fn search_effort(
    cfg: &StrengthenedConfig,
    st: &GameState,
    player: u8,
    seed: u64,
) -> MechanismEffort {
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
    .1
}
