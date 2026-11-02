//! The extension points of the core loop exercised with test mechanisms: the defaults are the
//! composed null configuration, an engine error inside a simulated line is scored or fails the
//! search as `error_leaf` says, errors outside simulated lines always propagate, and horizon
//! extensions run the rollout further and terminate.

use std::cell::Cell;
use std::path::PathBuf;

use gcg_ai::{AgentCx, AgentError, MoveKey, NoTrace, Scratch, SearchResult};
use gcg_core::pool::{Cancel, Pool, TaskCx};
use gcg_core::rng::SplitMix64;
use gcg_effects::Registry;
use gcg_engine::canon::from_canonical;
use gcg_engine::{Action, EngineError, GameState, StateSnapshot, UnimplementedCardError};
use serde_json::Value;

use super::{Composed, HorizonDecision, LeafCx, Mechanisms, RolloutCx, RootCx, WorldCx, reference};
use crate::mcts::run;
use crate::{MechanismEffort, Observation, Preset, StrengthenedConfig, preset_config};

/// The searched decisions of the D2 vectors (`crates/gcg-ai/tests/vectors/search/`), as
/// `(state, player, per-decision seed)`.
pub(crate) fn searched_states(limit: usize) -> Vec<(GameState, u8, u64)> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../gcg-ai/tests/vectors/search/decisions.ndjson");
    let reg = Registry::embedded();
    std::fs::read_to_string(path)
        .expect("the vectors")
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("a record"))
        .filter(|rec| rec["search"].as_array().is_some_and(|s| !s.is_empty()))
        .take(limit)
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

/// `fast` with every mechanism off but those `flags` names.
pub(crate) fn config(flags: &[&str]) -> StrengthenedConfig {
    let mut cfg = preset_config(Preset::Fast).clone();
    cfg.mechanisms.engine_error_leaf = false;
    for f in flags {
        assert_eq!(cfg.mechanisms.set_text(f, "true"), Some(Ok(())), "{f}");
    }
    cfg
}

pub(crate) type Outcome = Result<(SearchResult, MechanismEffort), AgentError>;

pub(crate) fn run_with<M: Mechanisms>(
    mech: &M,
    st: &GameState,
    player: u8,
    cfg: &StrengthenedConfig,
    seed: u64,
) -> Outcome {
    let cancel = Cancel::new();
    let obs = Observation::new(st, player).expect("an observation");
    Pool::run_inline(|scratch: &mut Scratch, par: &TaskCx<'_, Scratch>| {
        let mut cx = AgentCx {
            scratch,
            par,
            cancel: &cancel,
        };
        let mut rng = SplitMix64::new(seed);
        run(
            mech,
            obs.info(),
            player,
            cfg,
            &mut rng,
            &mut cx,
            &mut NoTrace,
            None,
        )
    })
}

fn bits(r: &SearchResult) -> Vec<(Action, u64, u32)> {
    r.alternatives
        .iter()
        .map(|a| (a.action, a.value.to_bits(), a.visits))
        .collect()
}

/// Every default, with nothing overridden.
struct Defaults;

impl Mechanisms for Defaults {}

/// A rollout that fails with `err` at its first decision, counting its calls; with `absorb`,
/// the error leaf is `Composed`'s (the `engine_error_leaf` flag of the configuration).
struct FailingRollouts<'c> {
    err: EngineError,
    calls: Cell<u64>,
    composed: Composed<'c>,
}

impl Mechanisms for FailingRollouts<'_> {
    fn rollout_choice(&self, cx: &mut RolloutCx<'_>) -> Result<Action, EngineError> {
        assert!(cx.player < 2);
        self.calls.set(self.calls.get() + 1);
        Err(self.err.clone())
    }

    fn error_leaf(&self, cx: &LeafCx<'_>, err: &EngineError) -> Option<f64> {
        self.composed.error_leaf(cx, err)
    }
}

fn redirect_cycle() -> EngineError {
    EngineError::RedirectCycle {
        uids: vec![36, 3, 36].into_boxed_slice(),
    }
}

#[test]
fn the_defaults_are_the_composed_null_configuration() {
    let cfg = config(&[]);
    for (st, player, seed) in searched_states(12) {
        let (a, ea) = run_with(&Defaults, &st, player, &cfg, seed).expect("a search");
        let (b, eb) = run_with(&Composed::new(&cfg), &st, player, &cfg, seed).expect("a search");
        assert_eq!(
            (a.action, bits(&a), &a.stats),
            (b.action, bits(&b), &b.stats)
        );
        assert_eq!(
            (ea, eb),
            (MechanismEffort::default(), MechanismEffort::default())
        );
    }
}

#[test]
fn an_engine_error_in_a_simulated_line_fails_the_search_without_the_flag() {
    let cfg = config(&[]);
    let (st, player, seed) = searched_states(1).remove(0);
    let mech = FailingRollouts {
        err: redirect_cycle(),
        calls: Cell::new(0),
        composed: Composed::new(&cfg),
    };
    match run_with(&mech, &st, player, &cfg, seed) {
        Err(AgentError::Engine(e)) => assert_eq!(e, redirect_cycle()),
        other => panic!("expected the redirect cycle, got {other:?}"),
    }
    assert_eq!(mech.calls.get(), 1);
}

#[test]
fn an_engine_error_in_a_simulated_line_scores_a_loss_with_the_flag() {
    let cfg = config(&["engine_error_leaf"]);
    let mut leaves = 0;
    for (st, player, seed) in searched_states(6) {
        let mech = FailingRollouts {
            err: redirect_cycle(),
            calls: Cell::new(0),
            composed: Composed::new(&cfg),
        };
        let (result, effort) = run_with(&mech, &st, player, &cfg, seed).expect("a search");
        assert_eq!(effort.engine_error_leaves, mech.calls.get());
        leaves += effort.engine_error_leaves;
        if result.stats.iterations == 0 {
            continue;
        }
        // A line that failed scored a loss of the searcher before the horizon:
        // `WIN_DECAY * (turn - root_turn)` with at most `horizon_turns` turns elapsed.
        let loss = reference::WIN_DECAY * f64::from(cfg.horizon_turns);
        assert!(
            result
                .alternatives
                .iter()
                .any(|alt| alt.visits > 0 && alt.value <= loss),
            "{:?}",
            result.alternatives
        );
    }
    assert!(leaves > 0);
}

#[test]
fn an_unimplemented_card_is_never_absorbed() {
    let cfg = config(&["engine_error_leaf"]);
    let (st, player, seed) = searched_states(1).remove(0);
    let err = EngineError::Unimplemented(UnimplementedCardError {
        card_number: "GD01-001".into(),
        reason: "test".into(),
    });
    let mech = FailingRollouts {
        err: err.clone(),
        calls: Cell::new(0),
        composed: Composed::new(&cfg),
    };
    match run_with(&mech, &st, player, &cfg, seed) {
        Err(AgentError::Engine(e)) => assert_eq!(e, err),
        other => panic!("expected the unimplemented card, got {other:?}"),
    }
}

/// Root moves that fail: an error outside the simulated lines.
struct FailingRoot<'c>(Composed<'c>);

impl Mechanisms for FailingRoot<'_> {
    fn root_moves(&self, _cx: &mut RootCx<'_>) -> Result<Vec<(MoveKey, Action)>, EngineError> {
        Err(redirect_cycle())
    }

    fn error_leaf(&self, cx: &LeafCx<'_>, err: &EngineError) -> Option<f64> {
        self.0.error_leaf(cx, err)
    }
}

/// World building that fails: also outside the simulated lines.
struct FailingWorlds<'c>(Composed<'c>);

impl Mechanisms for FailingWorlds<'_> {
    fn world(
        &self,
        cx: &mut WorldCx<'_>,
        k: usize,
        out: &mut GameState,
    ) -> Result<(), EngineError> {
        assert!(cx.cfg.mechanisms.engine_error_leaf);
        reference::world(cx, k, out)?;
        Err(EngineError::Value(
            "cannot determinize: empty pool for hidden cards".into(),
        ))
    }

    fn error_leaf(&self, cx: &LeafCx<'_>, err: &EngineError) -> Option<f64> {
        self.0.error_leaf(cx, err)
    }
}

#[test]
fn errors_outside_simulated_lines_propagate_with_the_flag() {
    let cfg = config(&["engine_error_leaf"]);
    let (st, player, seed) = searched_states(1).remove(0);
    let root = run_with(&FailingRoot(Composed::new(&cfg)), &st, player, &cfg, seed);
    assert!(matches!(
        root,
        Err(AgentError::Engine(EngineError::RedirectCycle { .. }))
    ));
    let worlds = run_with(&FailingWorlds(Composed::new(&cfg)), &st, player, &cfg, seed);
    assert!(matches!(
        worlds,
        Err(AgentError::Engine(EngineError::Value(_)))
    ));
}

/// Extends every line once by `turns` turn boundaries and `decisions` rollout decisions.
struct ExtendOnce {
    turns: i32,
    decisions: u32,
    stops: Cell<u64>,
}

impl Mechanisms for ExtendOnce {
    fn horizon(&self, cx: &LeafCx<'_>) -> HorizonDecision {
        self.stops.set(self.stops.get() + 1);
        assert!(!cx.capped || cx.decisions >= cx.cfg.max_rollout_decisions);
        if cx.horizon == cx.root_turn + cx.cfg.horizon_turns {
            HorizonDecision::Extend {
                turns: self.turns,
                decisions: self.decisions,
            }
        } else {
            HorizonDecision::Leaf
        }
    }
}

#[test]
fn an_extension_that_changes_nothing_terminates_with_the_reference_result() {
    let cfg = config(&[]);
    for (st, player, seed) in searched_states(6) {
        let mech = ExtendOnce {
            turns: 0,
            decisions: 0,
            stops: Cell::new(0),
        };
        let (a, _) = run_with(&mech, &st, player, &cfg, seed).expect("a search");
        let (b, _) = run_with(&Defaults, &st, player, &cfg, seed).expect("a search");
        assert_eq!(
            (a.action, bits(&a), &a.stats),
            (b.action, bits(&b), &b.stats)
        );
    }
}

#[test]
fn a_horizon_extension_rolls_out_further() {
    let cfg = config(&[]);
    let mut longer = 0;
    for (st, player, seed) in searched_states(6) {
        let mech = ExtendOnce {
            turns: 1,
            decisions: cfg.max_rollout_decisions,
            stops: Cell::new(0),
        };
        let (a, _) = run_with(&mech, &st, player, &cfg, seed).expect("a search");
        let (b, _) = run_with(&Defaults, &st, player, &cfg, seed).expect("a search");
        if b.stats.iterations > 0 {
            assert!(mech.stops.get() >= u64::from(a.stats.iterations));
            assert!(a.stats.effort.applies_rollout >= b.stats.effort.applies_rollout);
            longer += u64::from(a.stats.effort.applies_rollout > b.stats.effort.applies_rollout);
        }
    }
    assert!(longer > 0);
}

/// `portable_math` runs the same search with the portable functions: the same root moves,
/// budgets and seeds, and on most decisions the same move as the platform functions.
#[test]
fn portable_math_runs_the_same_search_with_portable_functions() {
    let platform = config(&[]);
    let portable = config(&["portable_math"]);
    assert_eq!(
        super::math_of(&portable),
        crate::mech::p6o::math::Math::Portable
    );
    let states = searched_states(16);
    let mut same = 0;
    for (st, player, seed) in &states {
        let (a, _) =
            run_with(&Composed::new(&portable), st, *player, &portable, *seed).expect("a search");
        let (b, _) =
            run_with(&Composed::new(&platform), st, *player, &platform, *seed).expect("a search");
        assert_eq!(a.stats.roots, b.stats.roots);
        assert_eq!((a.stats.budget, a.stats.cap), (b.stats.budget, b.stats.cap));
        same += usize::from(a.action == b.action);
    }
    assert!(same * 4 >= states.len() * 3, "{same} of {}", states.len());
}
