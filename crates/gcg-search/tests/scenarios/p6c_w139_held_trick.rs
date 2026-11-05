//! W-139 (T-139, the held trick): the decision-level record.
//!
//! The searcher's GD02-078 (AP 5, HP 4) has attacked and is rested; the searcher holds GD04-113
//! (【Action】 a friendly Unit receives 3 less battle damage during the battle) with one of its
//! four Resources open, and GD01-053 can attack or spend that Resource on its
//! 【Activate･Main】 (1 damage to an enemy Unit with 2 or less AP). The opponent's GD03-016 (AP 5,
//! HP 4) can attack the rested Unit next turn: without the trick the two trade, with it only the
//! attacker is destroyed. Keeping the Resource open (attacking with GD01-053 or ending the main
//! phase) keeps the trick; activating spends it. No root option names a hidden card, so
//! opponent-view worlds are used.
//!
//! The world-level evidence is the unit test
//! `mech::p6c::tests::w139_opponent_view_worlds_hide_the_held_trick` (in the opponent-view
//! worlds the searcher holds the trick in 2 of 100, in the usual worlds in all 300, and the
//! opponent's information set is the usual world's). At the root, the search's evaluation
//! values the activation highest with or without opponent-view worlds (it counts the trick card
//! in hand and values the saved Unit little), so the decision does not change.

use gcg_engine::{ActionKind, AddOptions, GameState, Scenario, ScenarioOptions, Zone};

use crate::aiport::registry;
use crate::p6c_common::{
    FILLER, add, add_with, count, count_cfg, hidden, reference_agent, start, strengthened,
};

/// The position: player 0's main phase.
pub fn held_trick() -> GameState {
    let mut sc = Scenario::new(&registry(), ScenarioOptions::default());
    add_with(
        &mut sc,
        0,
        "GD02-078",
        Zone::Battle,
        AddOptions {
            rested: true,
            ..AddOptions::default()
        },
    );
    add(&mut sc, 0, "GD01-053", Zone::Battle);
    sc.resources(0, 4, 3, 0).expect("resources");
    hidden(&mut sc, 0, "GD04-113");
    sc.shields(0, &[FILLER; 5]).expect("shields");
    sc.deck(0, &[FILLER; 20]);
    add(&mut sc, 1, "GD03-016", Zone::Battle);
    add(&mut sc, 1, FILLER, Zone::Battle);
    sc.resources(1, 5, 0, 0).expect("resources");
    hidden(&mut sc, 1, FILLER);
    hidden(&mut sc, 1, FILLER);
    sc.shields(1, &[FILLER; 5]).expect("shields");
    sc.deck(1, &[FILLER; 20]);
    start(sc)
}

fn keeps(a: gcg_engine::Action) -> bool {
    a.kind != ActionKind::Activate
}

#[test]
fn w139_held_trick_decisions_are_recorded() {
    let st = held_trick();
    let dec = st.pending().expect("the main phase");
    assert!(dec.options.iter().any(|o| o.kind == ActionKind::Activate));
    let reference = count(&st, keeps, reference_agent);
    let v25 = count_cfg(
        &st,
        keeps,
        &strengthened(&[("opponent_view_share", "0.25")]),
    );
    let v50 = count_cfg(&st, keeps, &strengthened(&[("opponent_view_share", "0.5")]));
    eprintln!(
        "W-139: keeps the trick of 16 seeds: reference {reference}, share 0.25 {v25}, 0.5 {v50}"
    );
    eprintln!("pinned {:?}", (reference, v25, v50));
    assert!(
        reference <= 8,
        "the reference keeps it in at most 8: {reference}"
    );
}
