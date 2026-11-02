//! W-11 (unit, subtree reuse): an agent with `subtree_reuse` starts the next decision its move
//! led to from the kept subtree (its root moves carry visits from the last search, beyond the
//! decision's own iterations), only when nothing else happened in between; without the flag
//! every decision starts afresh. The games are deterministic: the same seeds give the same
//! actions in two runs, and the concrete agent of `make_search_agent` carries its trees as the
//! agent of `make_agent` does.

use std::sync::Arc;

use gcg_ai::{Agent, AgentCx, NoTrace, Scratch};
use gcg_core::derive_seed;
use gcg_core::pool::{Cancel, Pool, TaskCx};
use gcg_diff::fuzz::Pools;
use gcg_effects::Registry;
use gcg_engine::{Action, GameState, NewGameOptions, apply, new_game_by_number};
use gcg_search::{StrengthenedConfig, make_agent, make_search_agent};

use super::config;

const MASTER: u64 = 15_011;
const GAMES: u32 = 4;

fn refs(v: &[String]) -> Vec<&str> {
    v.iter().map(String::as_str).collect()
}

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
        derive_seed!(MASTER, "p6d-reuse-game", index),
        NewGameOptions::default(),
    )
    .expect("a legal fuzz game")
}

/// A reduced budget, so that a game is quick.
fn small(flags: &[(&str, &str)]) -> StrengthenedConfig {
    let mut c = config(flags);
    c.iterations_per_option = 8;
    c.min_iterations = 16;
    c.max_iterations = 64;
    c
}

/// One game of two agents of `cfg` (from `make_agent`): the actions, and the searches whose
/// root moves carried more visits than the search's own iterations.
fn play(cfg: &StrengthenedConfig, st: &mut GameState, index: u32) -> (Vec<Action>, usize, usize) {
    play_with(cfg, st, index, make_agent)
}

/// [`play`] with the agents of `make`.
fn play_with(
    cfg: &StrengthenedConfig,
    st: &mut GameState,
    index: u32,
    make: impl Fn(&StrengthenedConfig, u64, bool) -> Box<dyn Agent>,
) -> (Vec<Action>, usize, usize) {
    let mut agents: [Box<dyn Agent>; 2] = [0u32, 1].map(|p| {
        make(
            cfg,
            derive_seed!(MASTER, "p6d-reuse-agent", index, p),
            false,
        )
    });
    let cancel = Cancel::new();
    let mut actions = Vec::new();
    let (mut searches, mut reused) = (0, 0);
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
        if let Some(t) = trace
            && let Some(search) = &t.search
            && search.iterations > 0
        {
            searches += 1;
            let visits: u32 = t.alternatives.iter().map(|a| a.visits).sum();
            assert!(visits >= search.iterations);
            reused += usize::from(visits > search.iterations);
        }
        apply(st, action, true).expect("the agent's action is legal");
        actions.push(action);
        assert!(actions.len() < 5000, "game {index} does not end");
    }
    (actions, searches, reused)
}

#[test]
fn consecutive_own_decisions_start_from_the_kept_subtree() {
    let reg = Registry::embedded();
    let pools = Pools::new(&reg);
    for flags in [
        &[("subtree_reuse", "true")][..],
        &[("subtree_reuse", "true"), ("transpositions", "true")],
        &[("subtree_reuse", "true"), ("root_trees", "2")],
    ] {
        let cfg = small(flags);
        let (mut searches, mut reused) = (0, 0);
        for index in 0..GAMES {
            let (a, s, r) = play(&cfg, &mut start(&reg, &pools, index), index);
            let (b, _, _) = play(&cfg, &mut start(&reg, &pools, index), index);
            assert_eq!(a, b, "{flags:?} game {index}");
            searches += s;
            reused += r;
        }
        assert!(
            reused > 0 && reused < searches,
            "{flags:?}: {reused} of {searches}"
        );
        println!("W-11 subtree reuse {flags:?}: {reused} of {searches} searches reused a subtree");
    }
    // Without the flag no search carries visits.
    let cfg = small(&[]);
    for index in 0..GAMES {
        let (_, _, reused) = play(&cfg, &mut start(&reg, &pools, index), index);
        assert_eq!(reused, 0);
    }
}

#[test]
fn the_concrete_search_agent_carries_its_trees() {
    let reg = Registry::embedded();
    let pools = Pools::new(&reg);
    let cfg = small(&[("subtree_reuse", "true")]);
    let concrete = |cfg: &StrengthenedConfig, seed: u64, log: bool| -> Box<dyn Agent> {
        Box::new(make_search_agent(cfg, seed, log))
    };
    let (mut searches, mut reused) = (0, 0);
    for index in 0..GAMES {
        let (a, s, r) = play(&cfg, &mut start(&reg, &pools, index), index);
        let (b, t, q) = play_with(&cfg, &mut start(&reg, &pools, index), index, concrete);
        assert_eq!((a, s, r), (b, t, q), "game {index}");
        searches += s;
        reused += r;
    }
    assert!(reused > 0 && reused < searches, "{reused} of {searches}");
}
