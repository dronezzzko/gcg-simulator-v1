//! Shared helpers of P6-E's scenario tests: the reference agent and the strengthened agent at
//! the reference `standard` budget, and solve counts over agent seeds.

use gcg_ai::{Agent, MoveKey, NoTrace, ROLE_WEIGHTS};
use gcg_engine::{Action, GameState};
use gcg_search::StrengthenedConfig;

use crate::aiport::with_cx;
use crate::common::null_config;

/// The reference `standard` preset with the shipped weights (`make_agent("mcts", seed=seed)`):
/// the shipped reference agent the WEAKNESSES entries measure.
pub fn reference_agent(seed: u64) -> Box<dyn Agent> {
    gcg_ai_reference::make_agent("mcts", "standard", seed, false, None, None).expect("mcts")
}

/// The strengthened search at the reference `standard` budget (the null configuration of
/// reference `standard` with the shipped weights) with the mechanism fields `on` set.
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

/// How many agent seeds `1..=seeds` choose `want` at `st`.
pub fn solves(
    st: &GameState,
    want: Action,
    seeds: u64,
    mut agent: impl FnMut(u64) -> Box<dyn Agent>,
) -> u32 {
    (1..=seeds)
        .map(|seed| u32::from(decide(&mut *agent(seed), st) == want))
        .sum()
}

/// The decision of `agent` at `st` for its pending player and the root moves of its search
/// (empty without a search).
pub fn decide_traced(agent: &mut dyn Agent, st: &GameState) -> (Action, Vec<MoveKey>) {
    let p = st.pending().expect("a pending decision").hdr.player;
    let (a, trace) = with_cx(|cx| agent.choose_traced(&st.clone(), p, cx, &mut NoTrace))
        .unwrap_or_else(|e| panic!("choose: {e}"));
    let roots = trace
        .and_then(|t| t.search)
        .map_or_else(Vec::new, |s| s.roots);
    (a, roots)
}

/// How many agent seeds `1..=seeds` keep a root move that `keep` accepts.
pub fn keeps(
    st: &GameState,
    seeds: u64,
    keep: impl Fn(&MoveKey) -> bool,
    mut agent: impl FnMut(u64) -> Box<dyn Agent>,
) -> u32 {
    (1..=seeds)
        .map(|seed| u32::from(decide_traced(&mut *agent(seed), st).1.iter().any(&keep)))
        .sum()
}

/// `solves` with the strengthened agent of `cfg`.
pub fn strengthened_solves(
    st: &GameState,
    want: Action,
    seeds: u64,
    cfg: &StrengthenedConfig,
) -> u32 {
    solves(st, want, seeds, |seed| {
        gcg_search::make_agent(cfg, seed, false)
    })
}
