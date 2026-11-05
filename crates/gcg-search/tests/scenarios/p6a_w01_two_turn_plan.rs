//! W-01 (two-turn rollout horizon): a two-turn plan whose payoff lies beyond the reference
//! horizon (docs/port/WEAKNESSES.md W-01, PLAN §12.3 S2).
//!
//! Player 0 is to move in turn T at Lv.3 (three Resources and a Resource deck). It has DINN
//! (GD01-064, 3/2) in play, one Shield, and holds DINN (Commander Type) (GD03-047, Lv.4, cost 2,
//! linked by Rau Le Creuset) and Rau Le Creuset (GD03-091, a Lv.4 Pilot, cost 1), neither
//! playable at Lv.3. Player 1 has no Shields and no Base, the Blocker G-Falcon (GD04-061, 3/1,
//! which cannot attack with its trash this small) active, and Gundam Ez8 High Mobility Custom
//! (EB01-032, 4/3) rested; it holds three Lv.5 Units it cannot play, so the evaluation sees a
//! contested game. Every deck holds only Lv.5 Units neither side can play.
//!
//! Player 0's three moves:
//!
//! - **Attack the player with DINN.** G-Falcon must block (otherwise DINN wins at once) and both
//!   are destroyed. In turn T+2, at Lv.4, player 0 deploys DINN (Commander Type), links Rau Le
//!   Creuset (a Link Unit attacks the turn it is deployed) and attacks a player with no Blocker:
//!   a forced win.
//! - **End the turn.** Ez8 can only attack player 0's Shield. In turn T+2 DINN and the new Link
//!   Unit attack into one Blocker: one gets through, a forced win.
//! - **The trap: DINN attacks the rested Ez8**, and both are destroyed unless G-Falcon blocks.
//!   A defender that keeps G-Falcon back faces a single attacker in turn T+2 and survives it.
//!
//! The reference's leaf is the start of turn T+2. There no line shows lethal, because the
//! evaluation cannot see a Link Unit in hand attack this turn, and the trade looks no worse than
//! the plans; a three-turn horizon plays turn T+2 in the rollouts and finds the forced wins. The
//! adaptive horizon reaches them as well: after either plan, player 1's race estimate against
//! player 0's last Shield is at most two turns, so those leaves are unsettled and play one more
//! turn.
//!
//! An agent seed solves the position when player 0's move is one of the two plans.

use gcg_ai::Agent;
use gcg_ai_reference::ReferenceConfig;
use gcg_engine::{
    Action, ActionKind, AddOptions, GameState, Scenario, ScenarioOptions, Step, Zone, apply,
};
use gcg_search::StrengthenedConfig;

use crate::p6f_support::{DRAWN, SEEDS, add, add_with, config, play_turns, with_cx};

/// The position of the module docs.
pub fn position() -> GameState {
    let reg = gcg_effects::Registry::embedded();
    let mut sc = Scenario::new(&reg, ScenarioOptions::default());
    add(&mut sc, 0, "GD01-064", Zone::Battle);
    add(&mut sc, 0, "GD03-047", Zone::Hand);
    add(&mut sc, 0, "GD03-091", Zone::Hand);
    sc.resources(0, 3, 0, 0).expect("resources");
    sc.resource_deck(0, 4).expect("a resource deck");
    sc.shields(0, &[DRAWN]).expect("a Shield");
    sc.deck(0, &[DRAWN; 12]);
    add(&mut sc, 1, "GD04-061", Zone::Battle);
    add_with(
        &mut sc,
        1,
        "EB01-032",
        Zone::Battle,
        AddOptions {
            rested: true,
            ..AddOptions::default()
        },
    );
    sc.resources(1, 2, 0, 0).expect("resources");
    sc.resource_deck(1, 4).expect("a resource deck");
    sc.hand(1, &[DRAWN; 3]).expect("a hand");
    sc.deck(1, &[DRAWN; 12]);
    sc.start(Step::Main).expect("the scenario starts");
    sc.into_state()
}

/// Player 0's move at the position.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Move {
    Face,
    Trade,
    End,
}

impl Move {
    fn of(a: Action) -> Self {
        match a.kind {
            ActionKind::Attack if a.b < 0 => Self::Face,
            ActionKind::Attack => Self::Trade,
            _ => Self::End,
        }
    }

    const fn is_plan(self) -> bool {
        matches!(self, Self::Face | Self::End)
    }
}

/// The reference configuration `r` with the horizon `turns` and the P6-A flags `on`.
fn with_horizon(r: &ReferenceConfig, turns: i32, on: &[&str]) -> StrengthenedConfig {
    let mut cfg = config(r, on);
    cfg.horizon_turns = turns;
    assert_eq!(cfg.check_mechanisms(), Ok(()));
    cfg
}

/// The P6-A bundle at the budget of `r`: horizon 3 and every P6-A flag.
fn bundle(r: &ReferenceConfig) -> StrengthenedConfig {
    with_horizon(
        r,
        3,
        &[
            "adaptive_horizon",
            "generalized_leaf",
            "capped_rollout_continuation",
        ],
    )
}

/// How many agent seeds play one of the plans with `cfg`.
fn plans(cfg: &StrengthenedConfig) -> u32 {
    let st = position();
    SEEDS
        .map(|seed| {
            let mut agent = gcg_search::make_search_agent(cfg, seed, false);
            let a = with_cx(|cx| agent.choose(&st, 0, cx)).expect("a move");
            u32::from(Move::of(a).is_plan())
        })
        .sum()
}

/// Whether player 0 has won by the end of turn T+2 after playing `first`, both seats then
/// playing `cfg` (seeds `seed`, `seed + 100`).
fn wins_after(first: Move, cfg: &StrengthenedConfig, seed: u64) -> bool {
    let mut st = position();
    let dec = st.pending().expect("player 0's decision");
    let a = *dec
        .options
        .iter()
        .find(|&&a| Move::of(a) == first)
        .expect("the move is legal");
    apply(&mut st, a, true).expect("the move applies");
    play_turns(cfg, &mut st, seed, 3);
    st.winner() == Some(0)
}

/// The scenario's premise, with a defender that sees turn T+2 (the P6-A bundle at the
/// reference `strong` budget playing both seats after the first move): both plans win by the
/// end of turn T+2 in every seed, and after the trade the defender keeps its Blocker back.
#[test]
fn w01_both_plans_win_by_force_and_the_trade_does_not() {
    let defender = bundle(&gcg_ai_reference::STRONG);
    let count = |m: Move| -> u32 {
        SEEDS
            .map(|seed| u32::from(wins_after(m, &defender, seed)))
            .sum()
    };
    let (face, end, trade) = (count(Move::Face), count(Move::End), count(Move::Trade));
    eprintln!("W-01 wins by turn T+2 of 16: face {face}, end {end}, trade {trade}");
    assert_eq!((face, end), (16, 16));
    assert!(trade <= 4, "{trade}");
}

/// The reference's count at the reference `standard` budget (pinned) and the strengthened bar
/// (WEAKNESSES W-01: the reference fails at least 8 of 16, the strengthened search solves at
/// least 14).
const REFERENCE_PLANS: u32 = 0;
const STRENGTHENED_BAR: u32 = 14;

#[test]
fn w01_a_longer_horizon_finds_the_two_turn_plan() {
    let standard = &gcg_ai_reference::STANDARD;
    let reference = plans(&with_horizon(standard, 2, &[]));
    let horizon3 = plans(&with_horizon(standard, 3, &[]));
    let adaptive = plans(&with_horizon(standard, 2, &["adaptive_horizon"]));
    let all = plans(&bundle(standard));
    eprintln!(
        "W-01 plans of 16 at the reference standard budget: reference {reference}, \
         horizon 3 {horizon3}, adaptive_horizon {adaptive}, P6-A bundle {all}"
    );
    assert_eq!(reference, REFERENCE_PLANS, "the reference's recorded count");
    assert!(16 - reference >= 8, "the scenario reproduces the weakness");
    for (name, n) in [
        ("horizon 3", horizon3),
        ("adaptive_horizon", adaptive),
        ("P6-A bundle", all),
    ] {
        assert!(n >= STRENGTHENED_BAR, "{name}: {n} plans");
    }
}

/// At the reference `strong` budget (where the shipped `standard` preset starts) the reference
/// already plays a plan here; the three-turn horizon keeps it.
#[test]
fn w01_the_strong_budget_keeps_the_plan_with_a_longer_horizon() {
    let strong = &gcg_ai_reference::STRONG;
    let reference = plans(&with_horizon(strong, 2, &[]));
    let all = plans(&bundle(strong));
    eprintln!(
        "W-01 plans of 16 at the reference strong budget: reference {reference}, P6-A bundle {all}"
    );
    assert_eq!(reference, 16, "the reference's recorded count");
    assert!(all >= STRENGTHENED_BAR, "{all} plans");
}
