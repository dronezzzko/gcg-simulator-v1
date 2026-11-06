//! W-24 (modal choices scored flat): the value of a card depends on choosing its mode well
//! later, inside the search's simulated lines.
//!
//! Synthetic cards (`docs/dev/eval/P6-F/synthetic_modal_fragment.json`, from
//! `synthetic_cards.py`): S = GD02-084 (1/3, cost 2) with "At the end of your turn, choose 1 of
//! the following effects and activate it: ■Draw 1. ■Draw 3.";
//! T = GD01-022 (2/3, cost 2) with "【Deploy】Draw 2.". Player 0 holds both and can deploy one.
//! So S is worth one card more than T (minus one AP) when the mode is chosen well and one card
//! less when not. The mode is chosen at the end of the turn, inside the search's simulated
//! lines, where the reference scores every mode 0 (`ai/policy.py:349-351`) and picks uniformly,
//! so it values S at the average of the two modes, about T's value.

use gcg_engine::{ActionKind, CtxKey, DecisionKind, GameState, Uid, Zone, testkit};
use gcg_search::{StrengthenedConfig, make_search_agent};

use crate::p6f_support::{choose, config, fragment_registry, pending, solves, two_unit_position};

const S: &str = "GD02-084";
const T: &str = "GD01-022";

fn position() -> (GameState, Uid) {
    two_unit_position(
        &fragment_registry("synthetic_modal_fragment.json", &[S, T]),
        S,
        T,
    )
}

fn solved(cfg: &StrengthenedConfig, seed: u64) -> bool {
    let (st, s) = position();
    let a = choose(&mut make_search_agent(cfg, seed, false), &st);
    a.kind == ActionKind::PlayUnit && a.a == i32::from(s)
}

/// The reference's count of 16 (pinned) and the strengthened bar (W-24: at least 13 of 16).
const REFERENCE_SOLVES: u32 = 0;
const STRENGTHENED_BAR: u32 = 13;

#[test]
fn w24_modes_are_scored_in_simulated_lines() {
    // The modes differ: the second draws three cards, the first one.
    let (mut st, s) = position();
    testkit::play(&mut st, s, None, 0).expect("S");
    testkit::end_main(&mut st).expect("the end of the main phase");
    while pending(&st).0 == DecisionKind::ActionStep {
        testkit::pass(&mut st).expect("a pass");
    }
    assert_eq!(pending(&st), (DecisionKind::Select, 0));
    let dec = st.pending().expect("the mode");
    assert_eq!(dec.hdr.ctx(CtxKey::Mode), 1);
    let hand = |mode: usize| {
        let mut t = st.clone();
        let o = t.pending().expect("the mode").options[mode];
        gcg_engine::apply(&mut t, o, true).expect("the mode");
        while matches!(
            pending(&t),
            (DecisionKind::Select | DecisionKind::Discard, 0)
        ) {
            let o = t.pending().expect("a target").options[0];
            gcg_engine::apply(&mut t, o, true).expect("a target");
        }
        t.zone(0, Zone::Hand).len()
    };
    assert_eq!((hand(0), hand(1)), (2, 4));
    let reference = solves(&config(&gcg_ai_reference::FAST, &[]), solved);
    let strengthened = solves(
        &config(&gcg_ai_reference::FAST, &["modal_lookahead"]),
        solved,
    );
    eprintln!("W-24 deployments of S of 16: reference {reference}, modal_lookahead {strengthened}");
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
