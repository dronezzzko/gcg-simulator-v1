//! W-138 (unit): with `macro_multi_pick`, games of strengthened agents that search multi-card
//! choices as subsets record only the engine's single picks: every recorded action sequence
//! replays with `apply(check=True)` from the game's start to the same end, and the searches did
//! use subset moves at multi-card choices.

use std::sync::Arc;

use gcg_ai::{Agent, AgentCx, NoTrace, Scratch};
use gcg_core::derive_seed;
use gcg_core::pool::{Cancel, Pool, TaskCx};
use gcg_diff::fuzz::Pools;
use gcg_effects::Registry;
use gcg_engine::{
    Action, ActionKind, DecisionKind, GameState, NewGameOptions, apply, new_game_by_number,
};
use gcg_search::{StrengthenedConfig, make_search_agent};

use super::config;

const MASTER: u64 = 13_808;
const GAMES: u32 = 10;

fn refs(v: &[String]) -> Vec<&str> {
    v.iter().map(String::as_str).collect()
}

/// The start of fuzz game `index` of `MASTER` (the decks of `gcg_diff::fuzz`).
fn start(reg: &Arc<Registry>, pools: &Pools<'_>, index: u32) -> GameState {
    let d0 = pools.deck_for(2 * u64::from(index), MASTER);
    let d1 = pools.deck_for(2 * u64::from(index) + 1, MASTER);
    let (m0, r0, m1, r1) = (
        refs(&d0.main),
        refs(&d0.resources),
        refs(&d1.main),
        refs(&d1.resources),
    );
    new_game_by_number(
        reg,
        [(&m0, &r0), (&m1, &r1)],
        derive_seed!(MASTER, "p6d-macro-game", index),
        NewGameOptions::default(),
    )
    .expect("a legal fuzz game")
}

/// One game of two agents of `cfg`: the actions, the winner, and the searched decisions whose
/// root moves were subsets.
fn play(cfg: &StrengthenedConfig, st: &mut GameState, index: u32) -> (Vec<Action>, i8, usize) {
    let mut agents = [0u32, 1].map(|p| {
        make_search_agent(
            cfg,
            derive_seed!(MASTER, "p6d-macro-agent", index, p),
            false,
        )
    });
    let cancel = Cancel::new();
    let mut actions = Vec::new();
    let mut subsets = 0;
    while st.winner().is_none() {
        let player = st.pending().expect("a decision").hdr.player;
        let agent = &mut agents[usize::from(player)];
        let (action, trace) =
            Pool::run_inline(|scratch: &mut Scratch, par: &TaskCx<'_, Scratch>| {
                let mut cx = AgentCx {
                    scratch,
                    par,
                    cancel: &cancel,
                };
                agent.choose_traced(st, player, &mut cx, &mut NoTrace)
            })
            .expect("a decision");
        if let Some(search) = trace.and_then(|t| t.search) {
            subsets += usize::from(
                search
                    .roots
                    .iter()
                    .any(|k| k.kind == ActionKind::Select && k.variant < -1),
            );
        }
        apply(st, action, true).expect("the agent's action is legal");
        actions.push(action);
        assert!(actions.len() < 5000, "game {index} does not end");
    }
    (actions, st.winner().expect("a winner"), subsets)
}

#[test]
fn subset_moves_replay_as_single_picks() {
    let reg = Registry::embedded();
    let pools = Pools::new(&reg);
    let cfg = {
        let mut c = config(&[("macro_multi_pick", "true")]);
        c.iterations_per_option = 8;
        c.min_iterations = 16;
        c.max_iterations = 64;
        c
    };
    let mut searched_subsets = 0;
    let mut multi_picks = 0;
    for index in 0..GAMES {
        let mut st = start(&reg, &pools, index);
        let (actions, winner, subsets) = play(&cfg, &mut st, index);
        searched_subsets += subsets;
        // The recorded single picks replay from the start with legality checks.
        let mut replay = start(&reg, &pools, index);
        for &a in &actions {
            let dec = replay.pending().expect("a decision");
            if dec.hdr.kind == DecisionKind::Select
                && dec.hdr.ctx(gcg_engine::CtxKey::Max) >= 2
                && a.kind == ActionKind::Select
            {
                multi_picks += 1;
            }
            apply(&mut replay, a, true).expect("a recorded single pick is legal");
        }
        assert_eq!(replay.winner(), Some(winner), "game {index}");
        assert_eq!(
            gcg_engine::canon::state_digest(&replay),
            gcg_engine::canon::state_digest(&st),
            "game {index}"
        );
    }
    assert!(
        multi_picks > 0 && searched_subsets > 0,
        "{multi_picks} {searched_subsets}"
    );
    println!("W-138: {searched_subsets} searches over subsets, {multi_picks} multi-card picks");
}
