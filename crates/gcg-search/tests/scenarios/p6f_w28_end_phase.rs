//! W-28 (end-phase action-step plays under-explored by rollouts): a "hold the rest effect for
//! the opponent's end step" puzzle at the reference `fast` budget.
//!
//! Player 0's Gundam (GD01-013) was deployed this turn and cannot attack; player 1 has no
//! Shields and one Blocker, G-Falcon (GD04-061, 3/1, which cannot attack while its trash is
//! small). Player 0's Base is a synthetic GD01-126 (`docs/port/eval/P6-F/
//! synthetic_end_phase_fragment.json`, from `synthetic_cards.py`): "【Activate･Action】Rest this
//! Base：Choose 1 enemy Unit with 3 or less HP. Rest it. Then, draw 1." Used in player 0's own
//! end phase it draws a card but the Blocker stands up in player 1's start phase; kept for player
//! 1's end phase it rests the Blocker through player 0's next turn, and the Gundam wins. (In
//! player 0's next turn it comes too late: the Blocker blocks before the battle's action step.)
//! The search's leaf is the start of player 0's next turn, so the value of keeping the Base
//! shows only if rollouts use it in player 1's end phase, which the reference policy's pass
//! preference makes rare; the card it draws now is visible at once.
//!
//! The agent seed solves the puzzle when player 0 has won by the end of its next turn, both
//! seats playing the configuration.

use gcg_engine::{AddOptions, GameState, Scenario, ScenarioOptions, Step, Zone};

use crate::p6f_support::{DRAWN, add_with, config, fragment_registry, play_turns, solves};

fn position() -> GameState {
    let reg = fragment_registry("synthetic_end_phase_fragment.json", &["GD01-126"]);
    let mut sc = Scenario::new(&reg, ScenarioOptions::default());
    sc.resources(0, 2, 0, 0).expect("resources");
    sc.base(0, Some("GD01-126"), 0).expect("the Base");
    add_with(
        &mut sc,
        0,
        "GD01-013",
        Zone::Battle,
        AddOptions {
            deployed_this_turn: true,
            ..AddOptions::default()
        },
    );
    add_with(&mut sc, 1, "GD04-061", Zone::Battle, AddOptions::default());
    sc.resources(1, 2, 0, 0).expect("resources");
    sc.deck(0, &[DRAWN; 12]);
    sc.deck(1, &[DRAWN; 12]);
    // Player 1 holds six cards it cannot play: the evaluation sees an even game, not a won one.
    sc.hand(1, &[DRAWN; 6]).expect("a hand");
    sc.shields(0, &[DRAWN, DRAWN, DRAWN]).expect("shields");
    sc.start(Step::Main).expect("the scenario starts");
    sc.into_state()
}

fn solved(cfg: &gcg_search::StrengthenedConfig, seed: u64) -> bool {
    let mut st = position();
    play_turns(cfg, &mut st, seed, 3);
    st.winner() == Some(0)
}

/// The reference's solve count at the `fast` budget (pinned) and the strengthened bar.
const REFERENCE_SOLVES: u32 = 8;
const STRENGTHENED_BAR: u32 = 14;

#[test]
fn w28_end_phase_plays_keep_the_rest_effect_for_the_opponents_turn() {
    let reference = solves(&config(&gcg_ai_reference::FAST, &[]), solved);
    let strengthened = solves(
        &config(&gcg_ai_reference::FAST, &["end_phase_actions"]),
        solved,
    );
    eprintln!(
        "W-28 hold-the-rest solves of 16: reference {reference}, end_phase_actions {strengthened}"
    );
    assert_eq!(
        reference, REFERENCE_SOLVES,
        "the reference's recorded count"
    );
    assert!(reference <= 8, "the scenario reproduces the weakness");
    assert!(strengthened >= STRENGTHENED_BAR, "{strengthened} solves");
}
