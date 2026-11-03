//! W-25 (T-118, GAP-A1): the over-extension scenario, "a line that loses to a punishing reply
//! the shared policy plays only rarely", with 16 agent seeds: the reference over-extends in at
//! least 8 (recorded), the strengthened search in at most 3.
//!
//! The position is P6-B's last-Blocker position (`p6b_w03_last_blocker::last_blocker`): the
//! searcher's <Blocker> Messala can attack and destroy the opponent's last Shield, which is the
//! over-extension, because the opponent's three active GMs then need only the searcher's two
//! Shields and one more hit, and nothing is left to block. The punishing reply is the line that
//! sends all three GMs at the player. The shared rollout policy plays it only rarely: it prefers
//! destroying one of the searcher's rested Pisces for free to attacking a player who still has
//! Shields. So rollouts with the shared policy for the opponent (the weakness of W-25) see the
//! trade as safe. A searcher over-extends when, at the end of its turn, it has lost or Messala is
//! rested.
//!
//! The opponent models measured: the fitted opponent policy of P6-F (`opponent_knobs`, W-25
//! plan item 1) and the tactical solver in rollouts (`tactical_solver`, P6-B), with which a
//! rollout's mover, the opponent included, plays a lethal line the solver proves on the rollout
//! world: a competent opponent where it matters most. The shipped presets run the solver.

use gcg_engine::GameState;
use gcg_search::{Preset, preset_config};

use crate::p6b_support::{agent_of, count, play_turn, reference, strengthened};
use crate::p6b_w03_last_blocker::last_blocker;

const SEEDS: u64 = 16;

#[test]
fn the_reference_over_extends_and_a_competent_opponent_model_does_not() {
    let (st, blocker) = last_blocker();
    let over_extends = |s: &mut GameState, players: &mut [Box<dyn gcg_ai::Agent>; 2]| {
        play_turn(s, players);
        s.winner().is_some() || s.card(blocker).rested
    };
    let defender = |seed: u64| reference("standard", seed + 100);
    let reference_standard = count(
        &st,
        SEEDS,
        |seed| [reference("standard", seed), defender(seed)],
        over_extends,
    );
    let with = |cfg: gcg_search::StrengthenedConfig| {
        count(
            &st,
            SEEDS,
            |seed| [agent_of(&cfg, seed), defender(seed)],
            over_extends,
        )
    };
    let null = with(strengthened("standard", &[]));
    let knobs = with(strengthened("standard", &["opponent_knobs"]));
    let solver = with(strengthened("standard", &["tactical_solver"]));
    let fast = with(preset_config(Preset::Fast).clone());
    let standard = with(preset_config(Preset::Standard).clone());
    let solver_horizon = with(strengthened(
        "standard",
        &["tactical_solver", "adaptive_horizon"],
    ));
    let mut fast_fixed_horizon = preset_config(Preset::Fast).clone();
    fast_fixed_horizon.mechanisms.adaptive_horizon = false;
    let fast_fixed_horizon = with(fast_fixed_horizon);
    let mut fast_tree = preset_config(Preset::Fast).clone();
    fast_tree.mechanisms.tree_lethal_select = true;
    let fast_tree = with(fast_tree);
    let solver_horizon_tree = with(strengthened(
        "standard",
        &["tactical_solver", "adaptive_horizon", "tree_lethal_select"],
    ));
    let tree_alone = with(strengthened("standard", &["tree_lethal_select"]));
    eprintln!(
        "W-25 over-extension (16 seeds): reference standard {reference_standard}, null \
         {null}, opponent_knobs {knobs}, tactical_solver {solver}, tactical_solver + \
         adaptive_horizon {solver_horizon}, shipped fast {fast}, shipped fast without \
         adaptive_horizon {fast_fixed_horizon}, shipped standard {standard}; \
         tree_lethal_select: alone {tree_alone}, with tactical_solver + adaptive_horizon \
         {solver_horizon_tree}, with the shipped fast {fast_tree}"
    );
    // Recorded 2026-11-03: over-extends: reference standard 16, null 16, opponent_knobs 16,
    // tactical_solver 0, tactical_solver + adaptive_horizon 15, shipped fast 16, shipped fast
    // without adaptive_horizon 0, shipped standard 0; tree_lethal_select alone 0, with
    // tactical_solver + adaptive_horizon 0, with the shipped fast 0.
    //
    // Why the shipped fast over-extends: with the adaptive horizon, ending the main phase is
    // valued lower (its lines are unsettled and play one more turn), so the search returns to
    // the attack after its first line lost to the solver's lethal in the rollout; the second
    // visit expands the opponent's turn into the tree, where the opponent's replies are tried
    // in prior order (the shared policy prefers the free Pisces kills), each of those lines is
    // a win for the searcher, and the attack's mean rises above ending the phase at a
    // fast-sized budget (seed 1: 81 visits at 0.788 against 53 at 0.736). The shipped
    // standard's budget lets the opponent's nodes find the lethal. `tree_lethal_select` plays
    // the solver's lethal at the opponent's in-tree MAIN decisions as well.
    assert!(
        reference_standard >= 8,
        "the reference over-extends in at least 8 of 16: {reference_standard}"
    );
    assert_eq!(
        (reference_standard, null, knobs),
        (16, 16, 16),
        "the shared-policy opponent and the fitted opponent policy (pinned)"
    );
    assert!(
        solver <= 3 && standard <= 3,
        "the solver's competent opponent over-extends in at most 3 of 16: {solver}, {standard}"
    );
    assert_eq!(
        (solver_horizon, fast, fast_fixed_horizon),
        (15, 16, 0),
        "the adaptive horizon's interaction at fast-sized budgets (pinned)"
    );
    assert!(
        solver_horizon_tree <= 3 && fast_tree <= 3,
        "tree_lethal_select keeps the defence with the adaptive horizon: {solver_horizon_tree}, \
         {fast_tree}"
    );
}
