//! `engine_error_leaf` (W-143, W-110): the strategic fuzz game that failed
//! (`gcg-diff fuzz --games 400 --seed 2026 --agent search:fast`, game index 221, game seed
//! 11012090770908643750) reached the GD04-095 redirect cycle `[36, 3, 36]` inside a rollout of
//! the strengthened search at the reference `standard` budget (`reference_mapping(Fast)`, the
//! shipped `fast` preset when the game failed). Since the 2026-11-06 refresh each redirection
//! applies once per damage (ruling GD06-018:Q494), so the cycle no longer fails a simulated
//! line: the game completes at both configurations, with the flag on and off, and no line is
//! scored as an engine error. The error-leaf mechanism keeps its unit tests on synthetic engine
//! errors (`crates/gcg-search/src/mech/tests.rs`).

use std::num::NonZeroUsize;
use std::sync::Arc;

use gcg_ai::{Agent, AgentCx, AgentError, Scratch};
use gcg_core::derive_seed;
use gcg_core::pool::{Cancel, Pool, TaskCx};
use gcg_diff::fuzz::{AgentKind, AgentSpec, FuzzArgs, Pools, play_one};
use gcg_effects::Registry;
use gcg_engine::{GameState, NewGameOptions, apply, new_game_by_number};
use gcg_search::{
    MechanismEffort, Preset, SearchAgent, make_search_agent, preset_config, reference_mapping,
};

const MASTER: u64 = 2026;
const INDEX: u64 = 221;
const GAME_SEED: u64 = 11_012_090_770_908_643_750;

fn refs(v: &[String]) -> Vec<&str> {
    v.iter().map(String::as_str).collect()
}

/// The game of `play_one(index = 221)` (`crates/gcg-diff/src/fuzz.rs`) with both seats played by
/// `search-fast` agents built from `cfg`: the number of actions, the winner and the agents'
/// mechanism counters, or the first agent error.
fn play(
    cfg: &gcg_search::StrengthenedConfig,
) -> (Result<(usize, i8), AgentError>, MechanismEffort) {
    let reg: Arc<Registry> = Registry::embedded();
    let pools = Pools::new(&reg);
    let index = i64::try_from(INDEX).expect("an index");
    let d0 = pools.deck_for(2 * INDEX, MASTER);
    let d1 = pools.deck_for(2 * INDEX + 1, MASTER);
    let game_seed = derive_seed!(MASTER, "fuzz-game", index);
    assert_eq!(game_seed, GAME_SEED);
    let (m0, r0, m1, r1) = (
        refs(&d0.main),
        refs(&d0.resources),
        refs(&d1.main),
        refs(&d1.resources),
    );
    let mut st: GameState = new_game_by_number(
        &reg,
        [(&m0, &r0), (&m1, &r1)],
        game_seed,
        NewGameOptions::default(),
    )
    .expect("a legal fuzz game");
    let mut agents: [SearchAgent; 2] = [0_i64, 1]
        .map(|p| make_search_agent(cfg, derive_seed!(MASTER, "fuzz-agent", index, p), false));
    let cancel = Cancel::new();
    let mut actions = 0;
    let outcome = loop {
        if let Some(w) = st.winner() {
            break Ok((actions, w));
        }
        let player = st.pending().expect("a decision").hdr.player;
        let agent = &mut agents[usize::from(player)];
        let chosen = Pool::run_inline(|scratch: &mut Scratch, par: &TaskCx<'_, Scratch>| {
            let mut cx = AgentCx {
                scratch,
                par,
                cancel: &cancel,
            };
            agent.choose(&st, player, &mut cx)
        });
        match chosen {
            Ok(a) => {
                apply(&mut st, a, true).expect("the agent's action is legal");
                actions += 1;
            }
            Err(e) => break Err(e),
        }
    };
    let mut effort = agents[0].mechanism_effort();
    effort.add(agents[1].mechanism_effort());
    (outcome, effort)
}

#[test]
fn the_failing_fuzz_game_completes_with_the_shipped_preset() {
    let reg = Registry::embedded();
    let pools = Pools::new(&reg);
    let args = FuzzArgs {
        games: 400,
        seed: MASTER,
        workers: NonZeroUsize::MIN,
        agent: AgentSpec::Both(AgentKind::Search(Preset::Fast)),
    };
    assert!(preset_config(Preset::Fast).mechanisms.engine_error_leaf);
    let out = play_one(&reg, &pools, INDEX, &args);
    assert_eq!(out.seed, GAME_SEED);
    assert!(out.ok && out.finished, "{:?}", out.error);
    // The same game through the concrete agents finishes with the same number of actions.
    let (outcome, _) = play(preset_config(Preset::Fast));
    let (actions, _) = outcome.expect("the game completes");
    assert_eq!(u64::try_from(actions).expect("a count"), out.actions);
    // At the configuration the game failed with, the simulated redirect cycles resolve, so no
    // line is scored as an engine error, and the real game finishes.
    let failing = reference_mapping(Preset::Fast);
    assert!(failing.mechanisms.engine_error_leaf);
    let (outcome, effort) = play(failing);
    outcome.expect("the game completes");
    assert_eq!(effort.engine_error_leaves, 0, "{effort:?}");
}

#[test]
fn without_the_flag_the_game_completes_too() {
    let mut cfg = reference_mapping(Preset::Fast).clone();
    cfg.mechanisms.engine_error_leaf = false;
    let (outcome, effort) = play(&cfg);
    outcome.expect("the game completes: the redirect cycle resolves");
    assert_eq!(effort, MechanismEffort::default());
}
