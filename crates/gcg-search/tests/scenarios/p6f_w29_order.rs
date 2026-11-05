//! W-29 (no policy scorer for ORDER_TRIGGER): two simultaneous triggers whose order matters,
//! met by the search's simulated lines.
//!
//! Synthetic cards (`docs/port/eval/P6-F/synthetic_order_fragment.json`, from
//! `synthetic_cards.py`): S = GD02-084 (1/3, cost 2) with "At the end of your turn, draw 1." and
//! "At the end of your turn, if you have 1 or fewer cards in your hand, draw 3."; T = GD01-022
//! (2/3, cost 2) with "【Deploy】Draw 2.". Player 0 holds S and T and can deploy one. Deployed, S's
//! two triggers meet at the end of the turn with one card (T) in hand: the conditional draw
//! first leaves five cards in hand, the plain draw first two. So S is worth two cards more than
//! T (minus one AP) when its triggers are ordered well and one card less when not. The order is
//! decided inside the search's simulated lines, where the reference scores every order 0
//! (`ai/policy.py:104-108`) and picks uniformly, so it values S near the average of the two
//! orders.

use gcg_effects::Node;
use gcg_engine::{Action, ActionKind, DecisionKind, GameState, Uid, Zone, testkit};
use gcg_search::{StrengthenedConfig, make_search_agent};

use crate::p6f_support::{choose, config, fragment_registry, pending, solves, two_unit_position};

const S: &str = "GD02-084";
const T: &str = "GD01-022";

fn position() -> (GameState, Uid) {
    two_unit_position(
        &fragment_registry("synthetic_order_fragment.json", &[S, T]),
        S,
        T,
    )
}

/// S deployed and the turn ended up to the end-of-turn ORDER_TRIGGER.
fn at_the_order() -> GameState {
    let (mut st, s) = position();
    testkit::play(&mut st, s, None, 0).expect("S");
    testkit::end_main(&mut st).expect("the end of the main phase");
    while pending(&st).0 == DecisionKind::ActionStep {
        testkit::pass(&mut st).expect("a pass");
    }
    assert_eq!(pending(&st), (DecisionKind::OrderTrigger, 0));
    st
}

/// Whether the order resolves the conditional draw first.
fn conditional_first(st: &GameState, order: Action) -> bool {
    let batch = st.batches().last().expect("a batch");
    let t = &batch[usize::try_from(order.a).expect("an index")];
    let reg = st.registry();
    reg.program(t.program)
        .instrs
        .iter()
        .any(|&n| matches!(reg.node(n), Node::JumpIfNot(_)))
}

fn solved(cfg: &StrengthenedConfig, seed: u64) -> bool {
    let (st, s) = position();
    let a = choose(&mut make_search_agent(cfg, seed, false), &st);
    a.kind == ActionKind::PlayUnit && a.a == i32::from(s)
}

/// The reference's count of 16 (pinned) and the strengthened bar (W-29: at least 15 of 16).
const REFERENCE_SOLVES: u32 = 7;
const STRENGTHENED_BAR: u32 = 15;

#[test]
fn w29_trigger_order_is_scored_in_simulated_lines() {
    // The order matters: the conditional draw first leaves five cards in hand, the other two.
    let st = at_the_order();
    let options = st.pending().expect("the order").options.to_vec();
    let hand_after = |order: Action| {
        let mut t = st.clone();
        gcg_engine::apply(&mut t, order, true).expect("the order");
        t.zone(0, Zone::Hand).len()
    };
    let (good, bad): (Vec<Action>, Vec<Action>) =
        options.iter().partition(|&&o| conditional_first(&st, o));
    assert_eq!((good.len(), bad.len()), (1, 1));
    assert_eq!((hand_after(good[0]), hand_after(bad[0])), (5, 2));
    let reference = solves(&config(&gcg_ai_reference::FAST, &[]), solved);
    let strengthened = solves(
        &config(&gcg_ai_reference::FAST, &["order_trigger_scorer"]),
        solved,
    );
    eprintln!(
        "W-29 deployments of S of 16: reference {reference}, order_trigger_scorer {strengthened}"
    );
    assert_eq!(
        reference, REFERENCE_SOLVES,
        "the reference's recorded count"
    );
    assert!(
        reference < STRENGTHENED_BAR,
        "the scenario reproduces the weakness"
    );
    assert!(strengthened >= STRENGTHENED_BAR, "{strengthened} solves");
}
