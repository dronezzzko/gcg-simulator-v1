//! W-13 (T-113): the "while rested" Rest-target exception. Amuro Ray pairs with the Gundam:
//! 【When Paired】rest 1 enemy Unit with 5 or less HP. The candidates are the active Freedom
//! Gundam (GD03-070: "While this Unit is rested, friendly Shields can't receive battle damage
//! from enemy Units.") and an already rested Zaku II. The defender has one Shield and no Base,
//! and the attacker's GM and Gundam can attack: resting the rested Zaku (a no-op) keeps the
//! lethal, resting Freedom protects the Shield and loses it. The no-op Rest-target prune would
//! drop the Zaku and force the Freedom; the exception (resting Freedom changes the derived view,
//! `_resting_changes_more_than_the_unit`, `ai/actions.py:214-232`) keeps it. The reference and
//! the strengthened search (the exception judged over the K search worlds) both find the
//! lethal; the negative control with the exception disabled is the unit test
//! `mech::p6e::tests::without_the_exception_the_freedom_lethal_is_missed`.

use gcg_engine::{Action, ActionKind, GameState};

use crate::aiport::positions::play_card;
use crate::aiport::{FILLER, Sc};
use crate::p6e_common::{reference_agent, solves, strengthened, strengthened_solves};

/// The position: the SELECT of Amuro's 【When Paired】, and the lethal choice (the Zaku).
pub fn freedom_lethal() -> (GameState, Action) {
    let mut sc = Sc::new();
    sc.resources(0, 4, 0, 0);
    let host = sc.add(0, "GD01-013");
    sc.add(0, "ST01-005");
    let amuro = sc.in_hand(0, "ST01-010");
    sc.filler_shields(0, 5);
    sc.deck(0, &[FILLER; 20]);
    sc.add(1, "GD03-070");
    let zaku = sc.rested(1, "GD01-035");
    sc.shields(1, &[FILLER]);
    sc.deck(1, &[FILLER; 20]);
    let mut st = sc.start();
    play_card(&mut st, amuro, Some(host));
    (st, Action::new(ActionKind::Select, i32::from(zaku), -1, -1))
}

#[test]
fn both_searches_rest_the_rested_unit_and_keep_the_lethal() {
    let (st, want) = freedom_lethal();
    let dec = st.pending().expect("the When Paired target");
    assert_eq!(dec.options.len(), 2, "Freedom and the rested Zaku");
    let reference = solves(&st, want, 16, reference_agent);
    let null = strengthened_solves(&st, want, 16, &strengthened(&[]));
    let k_worlds = strengthened_solves(
        &st,
        want,
        16,
        &strengthened(&[("rest_prune_worlds", "true")]),
    );
    eprintln!(
        "W-13 Freedom lethal: reference {reference}/16, null {null}/16, rest_prune_worlds {k_worlds}/16"
    );
    // Recorded 2026-11-02: 16/16 for all three (the entry's verification).
    assert_eq!(reference, 16, "the reference finds the lethal (pinned)");
    assert_eq!(null, 16);
    assert_eq!(k_worlds, 16, "the K-world exception finds it too");
}
