//! W-03 (T-103): keep the last Blocker against lethal. The searcher (player 0) has two
//! Shields, two rested Pisces (1/2) and one <Blocker>, Messala (5/4), which can destroy the
//! opponent's last Shield, after which any hit on the searcher's next turn wins. The opponent
//! has three active GMs: on its turn they need three hits, the two Shields and the finishing
//! hit, so with Messala at home one attack is blocked and the searcher survives, while a
//! Messala that attacked stays rested and the three GMs are lethal. The reference's rollouts
//! play the opponent's turn with the hand-written policy, which prefers destroying a rested
//! Pisces for free to attacking a player who still has Shields, so they rarely find the line
//! that sends all three GMs at the player, and the reference trades its last Blocker for the
//! Shield. With `tactical_solver` the rollouts' opponent plays the lethal line the solver proves
//! on the rollout world.

use gcg_engine::{GameState, Uid};

use crate::aiport::{FILLER, Sc};
use crate::p6b_support::{agent_of, count, play_turn, reference, strengthened};

const GM: &str = "ST01-005";
/// Messala, a 5/4 <Blocker>.
const MESSALA: &str = "GD03-003";
/// Pisces, a vanilla 1/2.
const PISCES: &str = "GD01-021";

/// Player 0's main phase and its Messala.
pub fn last_blocker() -> (GameState, Uid) {
    let mut sc = Sc::new();
    let blocker = sc.add(0, MESSALA);
    for _ in 0..2 {
        sc.rested(0, PISCES);
    }
    for _ in 0..3 {
        sc.add(1, GM);
    }
    sc.filler_shields(0, 2);
    sc.filler_shields(1, 1);
    sc.deck(0, &[FILLER; 20]);
    sc.deck(1, &[FILLER; 20]);
    (sc.start(), blocker)
}

#[test]
fn the_solver_in_rollouts_keeps_the_last_blocker_home() {
    let (st, blocker) = last_blocker();
    // Kept: Messala is still active when the searcher's turn ends.
    let keeps = |s: &mut GameState, players: &mut [Box<dyn gcg_ai::Agent>; 2]| {
        play_turn(s, players);
        s.winner().is_none() && !s.card(blocker).rested
    };
    let reference_standard = count(
        &st,
        8,
        |seed| {
            [
                reference("standard", seed),
                reference("standard", seed + 100),
            ]
        },
        keeps,
    );
    let null = strengthened("standard", &[]);
    let null_standard = count(
        &st,
        8,
        |seed| [agent_of(&null, seed), reference("standard", seed + 100)],
        keeps,
    );
    let solver = strengthened("standard", &["tactical_solver"]);
    let solver_standard = count(
        &st,
        8,
        |seed| [agent_of(&solver, seed), reference("standard", seed + 100)],
        keeps,
    );
    eprintln!(
        "W-03 last Blocker kept: reference standard {reference_standard}/8, null \
         {null_standard}/8, tactical_solver {solver_standard}/8"
    );
    // Recorded 2026-11-02: kept by the reference 0/8 (it trades the Blocker 8/8), the null
    // configuration 0/8, tactical_solver 8/8.
    assert_eq!(
        reference_standard, 0,
        "the reference trades the Blocker on every seed (pinned)"
    );
    assert!(
        null_standard <= 1,
        "the null configuration trades it too: {null_standard}/8"
    );
    assert!(
        solver_standard >= 7,
        "tactical_solver keeps it at least 7 of 8: {solver_standard}/8"
    );
}
