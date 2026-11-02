//! Shared helpers of P6-F's scenario tests (docs/port/PLAN.md §9.8): positions on the Rust
//! testkit, the reference configuration as the strengthened search's null configuration (which
//! replays the reference bit for bit, `tests/null_config.rs`) with P6-F flags switched on, and
//! agent seeds 1..=16 played through the shared `Agent` interface.
//!
//! Both sides of a comparison are `search-<preset>` agents with the same seeds, so a scenario's
//! two counts differ only by the mechanism.

use gcg_ai::{Agent, AgentCx, ROLE_WEIGHTS, Scratch};
use gcg_ai_reference::ReferenceConfig;
use gcg_core::pool::{Cancel, Pool, TaskCx};
use gcg_effects::Registry;
use gcg_engine::{
    Action, AddOptions, DecisionKind, GameState, Scenario, ScenarioOptions, Step, Uid, Zone, apply,
};
use gcg_search::{StrengthenedConfig, make_search_agent};

use crate::common;

/// The agent seeds of a scenario (PLAN §9.8: N = 16).
pub const SEEDS: std::ops::RangeInclusive<u64> = 1..=16;

pub const FILLER: &str = "GD01-060";

/// The reference configuration `r` (null mechanisms) with the P6-F flags `on`.
pub fn config(r: &ReferenceConfig, on: &[&str]) -> StrengthenedConfig {
    let mut cfg = common::null_config(r, &ROLE_WEIGHTS);
    for f in on {
        assert_eq!(cfg.mechanisms.set_text(f, "true"), Some(Ok(())), "{f}");
    }
    cfg
}

/// Runs `f` with an agent context.
pub fn with_cx<T>(f: impl FnOnce(&mut AgentCx<'_>) -> T) -> T {
    let cancel = Cancel::new();
    Pool::run_inline(|scratch: &mut Scratch, par: &TaskCx<'_, Scratch>| {
        let mut cx = AgentCx {
            scratch,
            par,
            cancel: &cancel,
        };
        f(&mut cx)
    })
}

pub fn add(sc: &mut Scenario, player: u8, number: &str, zone: Zone) -> Uid {
    add_with(sc, player, number, zone, AddOptions::default())
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

/// The decision of `agent` at `st`'s pending decision.
pub fn choose(agent: &mut dyn Agent, st: &GameState) -> Action {
    let p = st.pending().expect("a decision").hdr.player;
    with_cx(|cx| agent.choose(st, p, cx)).expect("a move")
}

/// Plays `turns` turns from `st` with one agent of `cfg` per seat (seeds `seed`, `seed + 100`).
pub fn play_turns(cfg: &StrengthenedConfig, st: &mut GameState, seed: u64, turns: i32) {
    let mut agents = [
        make_search_agent(cfg, seed, false),
        make_search_agent(cfg, seed + 100, false),
    ];
    let stop = st.turn() + turns;
    with_cx(|cx| {
        while let Some(dec) = st.pending() {
            if st.turn() >= stop || st.winner().is_some() {
                break;
            }
            let p = dec.hdr.player;
            let a = agents[usize::from(p)].choose(st, p, cx).expect("a move");
            apply(st, a, true).unwrap_or_else(|e| panic!("apply {a:?}: {e}"));
        }
    });
}

/// How many of the agent seeds `solved` accepts, playing `cfg`.
pub fn solves(cfg: &StrengthenedConfig, solved: impl Fn(&StrengthenedConfig, u64) -> bool) -> u32 {
    SEEDS.map(|seed| u32::from(solved(cfg, seed))).sum()
}

/// The registry with the synthetic cards of `docs/port/eval/P6-F/<name>` (written by
/// `synthetic_cards.py`), which must override exactly `cards`.
pub fn fragment_registry(name: &str, cards: &[&str]) -> std::sync::Arc<Registry> {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/port/eval/P6-F")
        .join(name);
    let doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).expect("the fragment")).expect("JSON");
    assert_eq!(doc["card_numbers"], serde_json::json!(cards));
    std::sync::Arc::new(
        Registry::embedded()
            .with_fragment(&doc["fragment"])
            .expect("the fragment applies"),
    )
}

/// A vanilla Lv.5 Unit: neither player can deploy it with three Resources.
pub const DRAWN: &str = "EB01-007";

/// Player 0's main phase holding the Units `s` and `t` (one deployable this turn with three
/// Resources), against an opponent that can do nothing: both decks hold only [`DRAWN`], player
/// 1 has an empty board, and player 0 is behind on Shields so that the evaluation is not
/// saturated. Returns the state and `s`'s uid.
pub fn two_unit_position(reg: &std::sync::Arc<Registry>, s: &str, t: &str) -> (GameState, Uid) {
    let mut sc = Scenario::new(reg, ScenarioOptions::default());
    let s_uid = add(&mut sc, 0, s, Zone::Hand);
    add(&mut sc, 0, t, Zone::Hand);
    sc.resources(0, 3, 0, 0).expect("resources");
    sc.resources(1, 3, 0, 0).expect("resources");
    sc.deck(0, &[DRAWN; 12]);
    sc.deck(1, &[DRAWN; 12]);
    sc.shields(0, &[FILLER, FILLER]).expect("shields");
    sc.shields(1, &[FILLER; 6]).expect("shields");
    (start(sc), s_uid)
}

/// The kind and player of `st`'s pending decision.
pub fn pending(st: &GameState) -> (DecisionKind, u8) {
    let d = st.pending().expect("a decision");
    (d.hdr.kind, d.hdr.player)
}
