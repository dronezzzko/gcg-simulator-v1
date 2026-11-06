//! Shared helpers of P6-B's scenario tests (docs/dev/PLAN.md §12.2, §12.3 S3): the reference
//! agents of each reference preset, the strengthened search at a reference preset's budget with
//! P6-B's flags, and the play of one turn by a pair of agents.

#![allow(
    dead_code,
    reason = "each scenario file uses its own subset of the helpers"
)]

use gcg_ai::{Agent, ROLE_WEIGHTS};
use gcg_ai_reference::ReferenceConfig;
use gcg_engine::{GameState, apply};
use gcg_search::StrengthenedConfig;

use crate::aiport::with_cx;
use crate::common::null_config;

/// The reference agent of `preset` (`make_agent("mcts", preset, seed)`).
pub fn reference(preset: &str, seed: u64) -> Box<dyn Agent> {
    gcg_ai_reference::make_agent("mcts", preset, seed, false, None, None).expect("mcts")
}

/// The reference configuration of `preset`.
pub fn reference_config(preset: &str) -> &'static ReferenceConfig {
    gcg_ai_reference::PRESETS
        .iter()
        .find(|(n, _)| *n == preset)
        .map(|&(_, c)| c)
        .expect("a reference preset")
}

/// The strengthened search at the budget of reference `preset` (its null configuration with the
/// shipped weights) with the mechanism fields `on` set to `true`.
pub fn strengthened(preset: &str, on: &[&str]) -> StrengthenedConfig {
    let mut cfg = null_config(reference_config(preset), &ROLE_WEIGHTS);
    for &field in on {
        assert_eq!(
            cfg.mechanisms.set_text(field, "true"),
            Some(Ok(())),
            "{field}"
        );
    }
    assert_eq!(cfg.check_mechanisms(), Ok(()));
    cfg
}

/// The strengthened agent of `cfg`.
pub fn agent_of(cfg: &StrengthenedConfig, seed: u64) -> Box<dyn Agent> {
    gcg_search::make_agent(cfg, seed, false)
}

/// Plays `st` until its turn ends or the game is over, `players[p]` deciding for player `p`;
/// returns the winner (or `None`).
pub fn play_turn(st: &mut GameState, players: &mut [Box<dyn Agent>; 2]) -> Option<i8> {
    let turn = st.turn();
    while st.winner().is_none() && st.turn() == turn {
        let Some(dec) = st.pending() else { break };
        let p = dec.hdr.player;
        let a = with_cx(|cx| players[usize::from(p)].choose(&st.clone(), p, cx))
            .unwrap_or_else(|e| panic!("choose: {e}"));
        apply(st, a, true).unwrap_or_else(|e| panic!("apply {a:?}: {e}"));
    }
    st.winner()
}

/// Plays `st` until `turns` turn boundaries have passed or the game is over.
pub fn play_turns(st: &mut GameState, players: &mut [Box<dyn Agent>; 2], turns: i32) -> Option<i8> {
    let stop = st.turn() + turns;
    while st.winner().is_none() && st.turn() < stop {
        play_turn(st, players);
    }
    st.winner()
}

/// How many agent seeds `1..=seeds` make `ok` hold after `run` plays `st` with the agents
/// `make(seed)`.
pub fn count(
    st: &GameState,
    seeds: u64,
    mut make: impl FnMut(u64) -> [Box<dyn Agent>; 2],
    mut run: impl FnMut(&mut GameState, &mut [Box<dyn Agent>; 2]) -> bool,
) -> u32 {
    (1..=seeds)
        .map(|seed| {
            let mut players = make(seed);
            let mut s = st.clone();
            u32::from(run(&mut s, &mut players))
        })
        .sum()
}
