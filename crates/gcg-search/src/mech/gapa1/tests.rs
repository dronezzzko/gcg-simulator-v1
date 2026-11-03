//! GAP-A1's unit tests: `target_multi_rollouts` (W-14 plan item 2) plays `R` rollouts per
//! iteration at effect-target decisions only, divides the budget by `R`, and with `R = 1` is the
//! null configuration bit for bit.

use std::cell::Cell;

use gcg_ai::SearchResult;
use gcg_engine::{Action, EngineError, GameState};

use crate::mcts::is_target_decision;
use crate::mech::tests::{config, run_with, searched_states};
use crate::mech::{Budget, Composed, LeafCx, Mechanisms, RootCx};
use crate::{Observation, StrengthenedConfig};

/// `Composed` with its leaves counted.
struct Counted<'c> {
    composed: Composed<'c>,
    leaves: Cell<u64>,
}

impl Mechanisms for Counted<'_> {
    fn budget(&self, cx: &RootCx<'_>, n_moves: usize) -> Budget {
        self.composed.budget(cx, n_moves)
    }

    fn rollouts(&self, cx: &RootCx<'_>) -> u32 {
        self.composed.rollouts(cx)
    }

    fn leaf_value(&self, cx: &mut LeafCx<'_>) -> Result<f64, EngineError> {
        self.leaves.set(self.leaves.get() + 1);
        self.composed.leaf_value(cx)
    }
}

fn with_rollouts(r: &str) -> StrengthenedConfig {
    let mut cfg = config(&[]);
    assert_eq!(
        cfg.mechanisms.set_text("target_multi_rollouts", r),
        Some(Ok(()))
    );
    assert_eq!(cfg.check_mechanisms(), Ok(()));
    cfg
}

/// A search's action, alternatives (value bits) and its iteration, budget and cap counts.
type Bits = (Action, Vec<(Action, u64, u32)>, u32, u32, u32);

fn bits(r: &SearchResult) -> Bits {
    (
        r.action,
        r.alternatives
            .iter()
            .map(|a| (a.action, a.value.to_bits(), a.visits))
            .collect(),
        r.stats.iterations,
        r.stats.budget,
        r.stats.cap,
    )
}

fn is_target(st: &GameState, player: u8) -> bool {
    let obs = Observation::new(st, player).expect("an observation");
    obs.info().pending().is_some_and(|d| is_target_decision(&d))
}

#[test]
fn the_budget_is_divided_by_the_rollouts_rounding_up() {
    let b = Budget {
        iterations: 160,
        cap: 480,
    };
    assert_eq!(super::budget(1, b), b);
    assert_eq!(
        super::budget(4, b),
        Budget {
            iterations: 40,
            cap: 120
        }
    );
    assert_eq!(
        super::budget(
            3,
            Budget {
                iterations: 161,
                cap: 161
            }
        ),
        Budget {
            iterations: 54,
            cap: 54
        }
    );
    assert_eq!(
        super::budget(
            8,
            Budget {
                iterations: 3,
                cap: 3
            }
        ),
        Budget {
            iterations: 1,
            cap: 1
        }
    );
}

#[test]
fn one_rollout_per_iteration_is_the_null_configuration() {
    let null = config(&[]);
    let one = with_rollouts("1");
    let mut checked = 0;
    for (st, player, seed) in searched_states(24) {
        let a = run_with(&Composed::new(&null), &st, player, &null, seed).expect("null");
        let b = run_with(&Composed::new(&one), &st, player, &one, seed).expect("one");
        assert_eq!(bits(&a.0), bits(&b.0));
        assert_eq!(a.0.stats.effort, b.0.stats.effort);
        checked += 1;
    }
    assert_eq!(checked, 24);
}

#[test]
fn multi_rollouts_change_only_target_decisions_and_play_r_rollouts_per_iteration() {
    const R: u64 = 4;
    let null = config(&[]);
    let multi = with_rollouts("4");
    let (mut targets, mut others) = (0, 0);
    for (st, player, seed) in searched_states(usize::MAX) {
        let reference = Counted {
            composed: Composed::new(&null),
            leaves: Cell::new(0),
        };
        let a = run_with(&reference, &st, player, &null, seed).expect("null");
        let counted = Counted {
            composed: Composed::new(&multi),
            leaves: Cell::new(0),
        };
        let b = run_with(&counted, &st, player, &multi, seed).expect("multi");
        let again = run_with(&Composed::new(&multi), &st, player, &multi, seed).expect("again");
        assert_eq!(bits(&b.0), bits(&again.0), "deterministic");
        if a.0.stats.iterations == 0 {
            assert_eq!(bits(&a.0), bits(&b.0));
            continue;
        }
        assert_eq!(reference.leaves.get(), u64::from(a.0.stats.iterations));
        if is_target(&st, player) {
            targets += 1;
            assert_eq!(b.0.stats.budget, a.0.stats.budget.div_ceil(4));
            assert!(b.0.stats.cap >= b.0.stats.budget);
            assert_eq!(counted.leaves.get(), R * u64::from(b.0.stats.iterations));
            assert!(b.0.stats.iterations <= b.0.stats.cap);
        } else {
            others += 1;
            assert_eq!(bits(&a.0), bits(&b.0));
            assert_eq!(a.0.stats.effort, b.0.stats.effort);
            assert_eq!(counted.leaves.get(), u64::from(b.0.stats.iterations));
        }
    }
    eprintln!(
        "target_multi_rollouts on the D2 vectors: {targets} target decisions, {others} others"
    );
    assert!(
        targets > 0 && others > 0,
        "{targets} target, {others} other decisions"
    );
}

#[test]
fn tree_lethal_select_is_deterministic_and_spends_counted_solver_applies() {
    let null = config(&[]);
    let on = config(&["tree_lethal_select"]);
    let (mut changed, mut spent) = (0, 0);
    for (st, player, seed) in searched_states(usize::MAX) {
        let a = run_with(&Composed::new(&null), &st, player, &null, seed).expect("null");
        let m = Composed::new(&on);
        let b = run_with(&m, &st, player, &on, seed).expect("on");
        let again = run_with(&Composed::new(&on), &st, player, &on, seed).expect("again");
        assert_eq!(bits(&b.0), bits(&again.0), "deterministic");
        let solver = m.aux_applies();
        if solver > 0 {
            spent += 1;
        }
        if bits(&a.0) != bits(&b.0) {
            changed += 1;
            assert!(solver > 0, "a changed search ran the solver");
        }
    }
    eprintln!(
        "tree_lethal_select on the D2 vectors: {spent} searches ran the solver, {changed} changed"
    );
    assert!(spent > 0, "the solver runs at some in-tree MAIN decision");
}
