//! W-07 (T-107, inference from open Resources): the opponent kept two Resources open and
//! passed its main phase with two cards in hand.
//!
//! The searcher's turn 3: its Gundam (GD01-013, AP 3, HP 4) can attack. The opponent ended its
//! main phase with two of its four Resources open and two hidden cards in hand, and has a
//! ST01-008 Blocker (AP 1, HP 1). Its 26 unseen cards are 4 GD01-120 (【Action】 a friendly
//! <Blocker> Unit gets AP+3 during this turn: the Blocker then blocks and destroys Gundam), 18
//! GD01-060 (Lv.2, cost 1: the opponent would have played one in its main phase) and 4
//! GD03-016 (Lv.5, beyond its level). The true hand is two tricks.
//!
//! Uniform worlds give the opponent the trick in 29% of the worlds and the reference attacks.
//! Belief-weighted worlds (`belief.rs`) weigh each candidate hand by the playout policy's
//! probability of ending the main phase with it, so hands with a castable GD01-060 lose weight
//! and the trick gains it (the unit test
//! `mech::p6c::tests::w07_belief_worlds_infer_the_trick_from_the_open_resources`: 121 of the
//! first 400 reference worlds hold the trick, 224 with 8 candidates, 243 with 16), and the
//! search holds back.

use gcg_engine::{ActionKind, GameState, Scenario, ScenarioOptions, Zone};

use crate::aiport::registry;
use crate::p6c_common::{FILLER, add, count, count_cfg, hidden, reference_agent, strengthened};

/// The position: player 0's main phase, turn 3.
pub fn open_resources() -> GameState {
    let mut sc = Scenario::new(
        &registry(),
        ScenarioOptions {
            deck_size: 0,
            ..ScenarioOptions::default()
        },
    );
    add(&mut sc, 0, "GD01-013", Zone::Battle);
    sc.resources(0, 3, 0, 0).expect("resources");
    sc.shields(0, &[FILLER; 5]).expect("shields");
    sc.deck(0, &[FILLER; 20]);
    add(&mut sc, 1, "ST01-008", Zone::Battle);
    sc.resources(1, 4, 2, 0).expect("resources");
    let pool: Vec<&str> = (0..26)
        .map(|i| match i % 13 {
            0 | 7 => "GD01-120",
            3 | 10 => "GD03-016",
            _ => FILLER,
        })
        .collect();
    hidden(&mut sc, 1, pool[0]);
    hidden(&mut sc, 1, pool[13]);
    sc.shields(1, &pool[1..6]).expect("shields");
    let mut deck: Vec<&str> = pool[6..13].to_vec();
    deck.extend(&pool[14..]);
    sc.deck(1, &deck);
    sc.start(gcg_engine::Step::Main)
        .expect("the scenario starts");
    sc.into_state()
}

fn attacks(a: gcg_engine::Action) -> bool {
    a.kind == ActionKind::Attack
}

/// The reference attacks into the inferred trick in every seed; belief-weighted worlds hold
/// back more often as the candidates per world grow, and with 64 candidates in 12 of 16 seeds
/// (the entry's bar). With few candidates per world the resampling rarely finds a hand without
/// a castable card, so the inference is partial (the unit test's world counts).
#[test]
fn w07_belief_worlds_hold_back_from_the_inferred_trick() {
    let st = open_resources();
    assert_eq!((st.turn(), st.active()), (3, 0));
    let reference = count(&st, attacks, reference_agent);
    let rows: Vec<(&str, u32)> = ["8", "16", "32", "64"]
        .into_iter()
        .map(|m| {
            (
                m,
                count_cfg(&st, attacks, &strengthened(&[("belief_worlds", m)])),
            )
        })
        .collect();
    eprintln!("W-07: attacks of 16 seeds: reference {reference}, belief_worlds {rows:?}");
    // Recorded 2026-11-02: reference 16; belief_worlds 8: 16, 16: 12, 32: 8, 64: 4.
    assert!(
        reference >= 8,
        "the reference attacks in at least 8: {reference}"
    );
    let m64 = rows[3].1;
    assert!(
        16 - m64 >= 12,
        "belief_worlds=64 holds back in at least 12: {m64} attacks"
    );
    assert!(rows.windows(2).all(|w| w[1].1 <= w[0].1), "{rows:?}");
}
