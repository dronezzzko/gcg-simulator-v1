//! W-16 (T-116): lethal at a fast-sized budget. Critical decisions get no more nodes than
//! routine ones, so at the reference `fast` budget (clamp(7n, 16, 64)) the search misses a
//! lethal whose every attack must go to the player: four GMs against two Shields and one Demi
//! Trainer <Blocker>, with two rested Pisces (1/2) that a GM destroys for free. The defender
//! blocks one attack, so all four attacks must target the player; one attack on a Pisces leaves
//! the defender alive, and the reference policy scores that free kill above an attack on a
//! player who still has Shields. The strengthened `fast` (the same budget) with `lethal_root`
//! plays the proved lethal at each MAIN decision; `critical_extension` alone gives the
//! decision up to three budgets.

use gcg_engine::GameState;

use crate::aiport::{FILLER, Sc};
use crate::p6b_support::{agent_of, count, play_turn, reference, strengthened};

const GM: &str = "ST01-005";
const DEMI: &str = "ST01-008";

/// Pisces, a vanilla 1/2: a GM destroys a rested one and survives.
const PISCES: &str = "GD01-021";

/// Player 0's main phase: four GMs; player 1: two Shields, an active Demi Trainer and
/// `decoys` rested `decoy` Units.
pub fn four_through_one(decoy: &str, decoys: usize) -> GameState {
    let mut sc = Sc::new();
    for _ in 0..4 {
        sc.add(0, GM);
    }
    sc.add(1, DEMI);
    for _ in 0..decoys {
        sc.rested(1, decoy);
    }
    sc.filler_shields(0, 3);
    sc.filler_shields(1, 2);
    sc.deck(0, &[FILLER; 20]);
    sc.deck(1, &[FILLER; 20]);
    sc.start()
}

#[test]
fn the_fast_budget_misses_a_four_attack_lethal_that_lethal_root_plays() {
    let st = four_through_one(PISCES, 2);
    let wins = |s: &mut GameState, players: &mut [Box<dyn gcg_ai::Agent>; 2]| {
        play_turn(s, players) == Some(0)
    };
    let reference_fast = count(
        &st,
        16,
        |seed| [reference("fast", seed), reference("fast", seed + 100)],
        wins,
    );
    let lethal = strengthened("fast", &["lethal_root"]);
    let strengthened_fast = count(
        &st,
        16,
        |seed| [agent_of(&lethal, seed), reference("fast", seed + 100)],
        wins,
    );
    let extension = strengthened("fast", &["critical_extension"]);
    let extended_fast = count(
        &st,
        16,
        |seed| [agent_of(&extension, seed), reference("fast", seed + 100)],
        wins,
    );
    let reference_standard = count(
        &st,
        16,
        |seed| {
            [
                reference("standard", seed),
                reference("standard", seed + 100),
            ]
        },
        wins,
    );
    eprintln!(
        "W-16 four through one: reference fast {reference_fast}/16, reference standard \
         {reference_standard}/16, fast + critical_extension {extended_fast}/16, fast + \
         lethal_root {strengthened_fast}/16"
    );
    // Recorded 2026-11-02: reference fast 0/16, reference standard 5/16, fast +
    // critical_extension 0/16, fast + lethal_root 16/16.
    assert!(
        reference_fast <= 8,
        "the reference fails at least half the seeds: {reference_fast}/16"
    );
    assert_eq!(reference_fast, 0, "the reference fast solve count (pinned)");
    assert!(
        strengthened_fast >= 14,
        "lethal_root solves at least 14 of 16: {strengthened_fast}/16"
    );
}
