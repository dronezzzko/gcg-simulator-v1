//! W-05 (T-105, strategy fusion): the decision-level record of the fusion position.
//!
//! The searcher's Gundam (GD01-013, AP 3, HP 4) can attack the player. The defender has a
//! GD02-079 Blocker (AP 3, HP 3), three active Resources and one hidden card; 13 of its 46 unseen
//! cards are GD02-118 (【Action】 an enemy Unit with 4 or less HP battling a friendly <Blocker>
//! Unit returns to its owner's hand). With the trick the defender blocks and bounces Gundam;
//! without it a block only loses the Blocker.
//!
//! The node-level evidence of the mechanism is the unit test
//! `mech::p6c::tests::w05_bucketed_opponent_nodes_reply_by_hand`: with `opponent_buckets` the
//! defender blocks in 93% of the trick worlds' visits and in 7% of the others, against one
//! reply for both hands with pooled statistics. At the root, the attack stays the better move in
//! the search's own evaluation (at 4,000 iterations the bucketed search values the attack 0.313
//! against 0.299 for ending the main phase; with the trick known, 0.292 against 0.310), so the
//! decision does not change: this file records the counts the entry's verification asks for.

use gcg_engine::{ActionKind, GameState, Scenario, ScenarioOptions, Zone};

use crate::aiport::registry;
use crate::p6c_common::{
    FILLER, add, count, count_cfg, hidden, reference_agent, start, strengthened,
};

/// The fusion position: player 0's main phase.
pub fn fusion() -> GameState {
    let mut sc = Scenario::new(&registry(), ScenarioOptions::default());
    add(&mut sc, 0, "GD01-013", Zone::Battle);
    sc.resources(0, 3, 0, 0).expect("resources");
    sc.shields(0, &[FILLER; 5]).expect("shields");
    sc.deck(0, &[FILLER; 20]);
    add(&mut sc, 1, "GD02-079", Zone::Battle);
    sc.resources(1, 3, 0, 0).expect("resources");
    let pool: Vec<&str> = (0..26)
        .map(|i| if i % 2 == 0 { "GD02-118" } else { FILLER })
        .collect();
    hidden(&mut sc, 1, pool[0]);
    sc.shields(1, &pool[1..6]).expect("shields");
    sc.deck(1, &pool[6..]);
    start(sc)
}

fn attacks(a: gcg_engine::Action) -> bool {
    a.kind == ActionKind::Attack
}

#[test]
fn w05_fusion_decisions_are_recorded() {
    let st = fusion();
    let dec = st.pending().expect("the main phase");
    assert!(dec.options.iter().any(|o| o.kind == ActionKind::Attack));
    let reference = count(&st, attacks, reference_agent);
    let b4 = count_cfg(&st, attacks, &strengthened(&[("opponent_buckets", "4")]));
    let b8 = count_cfg(&st, attacks, &strengthened(&[("opponent_buckets", "8")]));
    eprintln!("W-05: attacks of 16 seeds: reference {reference}, opponent_buckets=4 {b4}, =8 {b8}");
    // Recorded 2026-11-02: reference 16, opponent_buckets=4 16, =8 16 (the plan's bar, the
    // strengthened search holding back in at least 13, is not met; see the module docs).
    assert_eq!((reference, b4, b8), (16, 16, 16));
}
