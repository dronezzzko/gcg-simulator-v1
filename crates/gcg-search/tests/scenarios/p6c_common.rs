//! Shared helpers of P6-C's scenario tests (docs/dev/PLAN.md §9.8): positions on the Rust
//! testkit, the reference `standard` configuration as the strengthened search's null
//! configuration (which replays the reference bit for bit, `tests/null_config.rs`) with P6-C
//! flags set, and counts over the agent seeds 1..=16 through the shared `Agent` interface.

use gcg_ai::{Agent, ROLE_WEIGHTS};
use gcg_engine::{Action, AddOptions, GameState, Scenario, Step, Uid, Zone};
use gcg_search::{StrengthenedConfig, make_agent};

use crate::aiport::with_cx;
use crate::common::null_config;

/// The agent seeds of a scenario (PLAN §9.8: N = 16).
pub const SEEDS: u64 = 16;

pub const FILLER: &str = "GD01-060";

/// The reference `standard` agent (`make_agent("mcts", seed=seed)`), the shipped reference the
/// WEAKNESSES entries measure.
pub fn reference_agent(seed: u64) -> Box<dyn Agent> {
    gcg_ai_reference::make_agent("mcts", "standard", seed, false, None, None).expect("mcts")
}

/// The strengthened search at the reference `standard` budget (its null configuration) with
/// the mechanism fields `on` set.
pub fn strengthened(on: &[(&str, &str)]) -> StrengthenedConfig {
    let mut cfg = null_config(&gcg_ai_reference::STANDARD, &ROLE_WEIGHTS);
    for &(field, value) in on {
        assert_eq!(
            cfg.mechanisms.set_text(field, value),
            Some(Ok(())),
            "{field}"
        );
    }
    assert_eq!(cfg.check_mechanisms(), Ok(()));
    cfg
}

/// The decision of `agent` at `st` for its pending player.
pub fn decide(agent: &mut dyn Agent, st: &GameState) -> Action {
    let p = st.pending().expect("a pending decision").hdr.player;
    with_cx(|cx| agent.choose(&st.clone(), p, cx)).unwrap_or_else(|e| panic!("choose: {e}"))
}

/// How many agent seeds `1..=SEEDS` make a decision at `st` that `pick` accepts.
pub fn count(
    st: &GameState,
    pick: impl Fn(Action) -> bool,
    mut agent: impl FnMut(u64) -> Box<dyn Agent>,
) -> u32 {
    (1..=SEEDS)
        .map(|seed| u32::from(pick(decide(&mut *agent(seed), st))))
        .sum()
}

/// [`count`] for the strengthened agent of `cfg`.
pub fn count_cfg(st: &GameState, pick: impl Fn(Action) -> bool, cfg: &StrengthenedConfig) -> u32 {
    count(st, pick, |seed| make_agent(cfg, seed, false))
}

pub fn add(sc: &mut Scenario, player: u8, number: &str, zone: Zone) -> Uid {
    add_with(sc, player, number, zone, AddOptions::default())
}

pub fn hidden(sc: &mut Scenario, player: u8, number: &str) -> Uid {
    add_with(
        sc,
        player,
        number,
        Zone::Hand,
        AddOptions {
            known: false,
            ..AddOptions::default()
        },
    )
}

pub fn add_with(
    sc: &mut Scenario,
    player: u8,
    number: &str,
    zone: Zone,
    opts: AddOptions<'_>,
) -> Uid {
    sc.add(player, number, zone, opts)
        .unwrap_or_else(|e| panic!("add {number}: {e:?}"))
}

pub fn start(mut sc: Scenario) -> GameState {
    sc.start(Step::Main).expect("the scenario starts");
    sc.into_state()
}
