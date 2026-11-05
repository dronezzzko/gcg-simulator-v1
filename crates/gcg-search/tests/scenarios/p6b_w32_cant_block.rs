//! W-32 (T-132): a lethal that is on because the only enemy Blocker cannot block. Player 0's
//! Gaia Gundam links with Stellar Loussier under Girty Lue, whose trigger makes the enemy Demi
//! Trainer unable to activate <Blocker> this turn (CANT_BLOCK). The reference policy's Blocker
//! count (`values.ready_blockers`) still counts Demi Trainer; the reference search sees the
//! engine's real block options in its tree. Two positions:
//!
//! - one enemy Shield, Gaia and a GM must both attack the player: the reference search finds it
//!   (16/16), so this one is the entry's no-regression check;
//! - three enemy Shields and two rested enemy Pisces (1/2) a GM destroys for free, Gaia and
//!   three GMs must all attack the player: the reproducer (the reference solves at most 8/16).
//!
//! The strengthened `standard` with `lethal_root` and `effective_blockers` must solve both
//! 16/16: its static attack step takes the Blockers from the engine's eligibility.

use gcg_engine::testkit;
use gcg_engine::{AddOptions, DecisionKind, GameState, Zone};

use crate::aiport::{FILLER, Sc};
use crate::p6b_support::{agent_of, count, play_turn, reference, strengthened};

const GM: &str = "ST01-005";
const DEMI: &str = "ST01-008";
const GIRTY_LUE: &str = "GD05-127";
const GAIA: &str = "GD05-034";
const STELLAR: &str = "GD05-090";

/// Pisces, a vanilla 1/2: a GM destroys a rested one and survives.
const PISCES: &str = "GD01-021";

/// Player 0's main phase after the link, with `gms` GMs: Demi Trainer can't block this turn;
/// player 1 has `shields` Shields and `decoys` rested Pisces.
pub fn cant_block(gms: usize, shields: usize, decoys: usize) -> GameState {
    let mut sc = Sc::new();
    sc.resources(0, 4, 0, 0);
    sc.base(0, Some(GIRTY_LUE));
    let gaia = sc.add(0, GAIA);
    for _ in 0..gms {
        sc.add(0, GM);
    }
    let stellar = sc.add_with(0, STELLAR, Zone::Hand, AddOptions::default());
    let demi = sc.add(1, DEMI);
    for _ in 0..decoys {
        sc.rested(1, PISCES);
    }
    sc.filler_shields(0, 3);
    sc.filler_shields(1, shields);
    sc.deck(0, &[FILLER; 20]);
    sc.deck(1, &[FILLER; 20]);
    let mut st = sc.start();
    testkit::play(&mut st, stellar, Some(gaia), 0).expect("pair Stellar with Gaia");
    testkit::select_if_asked(&mut st, &[demi], None).expect("Girty Lue's target");
    assert_eq!(
        st.pending().map(|d| (d.hdr.player, d.hdr.kind)),
        Some((0, DecisionKind::Main))
    );
    st
}

/// The solve counts over 16 agent seeds of the reference `standard`, the reference `fast` and
/// the strengthened `standard`.
fn solves(st: &GameState) -> (u32, u32, u32) {
    let wins = |s: &mut GameState, players: &mut [Box<dyn gcg_ai::Agent>; 2]| {
        play_turn(s, players) == Some(0)
    };
    let reference_standard = count(
        st,
        16,
        |seed| {
            [
                reference("standard", seed),
                reference("standard", seed + 100),
            ]
        },
        wins,
    );
    let reference_fast = count(
        st,
        16,
        |seed| [reference("fast", seed), reference("fast", seed + 100)],
        wins,
    );
    let cfg = strengthened("standard", &["lethal_root", "effective_blockers"]);
    let strengthened_standard = count(
        st,
        16,
        |seed| [agent_of(&cfg, seed), reference("standard", seed + 100)],
        wins,
    );
    (reference_standard, reference_fast, strengthened_standard)
}

#[test]
fn a_lethal_past_a_blocker_that_cannot_block() {
    let (reference_standard, reference_fast, strengthened_standard) = solves(&cant_block(1, 1, 0));
    eprintln!(
        "W-32 CANT_BLOCK lethal (no-regression): reference standard {reference_standard}/16, \
         reference fast {reference_fast}/16, lethal_root + effective_blockers \
         {strengthened_standard}/16"
    );
    assert_eq!(
        strengthened_standard, 16,
        "the strengthened search plays the lethal on every seed"
    );
    let (reference_standard, reference_fast, strengthened_standard) = solves(&cant_block(3, 3, 2));
    eprintln!(
        "W-32 CANT_BLOCK lethal (reproducer): reference standard {reference_standard}/16, \
         reference fast {reference_fast}/16, lethal_root + effective_blockers \
         {strengthened_standard}/16"
    );
    // Recorded 2026-11-02: no-regression position reference standard 16/16, reference fast
    // 16/16, strengthened 16/16; reproducer reference standard 0/16, reference fast 0/16,
    // strengthened 16/16.
    assert!(
        reference_standard <= 8,
        "the reference solves at most half the seeds: {reference_standard}/16"
    );
    assert_eq!(reference_standard, 0, "the reference solve count (pinned)");
    assert_eq!(
        strengthened_standard, 16,
        "the strengthened search plays the lethal on every seed"
    );
}
