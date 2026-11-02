//! P6-D's tests of the tree and budget mechanisms (docs/port/PLAN.md §12.2, §12.3 S1, S4, S7,
//! S8, S12; W-04, W-06, W-11, W-18, W-138, W-140). Every search runs on the observation of the
//! deciding player, as the agent does (W-135).

#![allow(dead_code, reason = "each module uses its own subset of the helpers")]

mod p6d_determinism;
mod p6d_fast_preset;
mod p6d_macros;
mod p6d_priors;
mod p6d_reuse;

use std::path::PathBuf;

use gcg_ai::{AgentCx, AgentError, NoTrace, Scratch, SearchResult};
use gcg_core::pool::{Cancel, Pool, TaskCx};
use gcg_core::rng::SplitMix64;
use gcg_effects::Registry;
use gcg_engine::canon::from_canonical;
use gcg_engine::{Action, GameState, StateSnapshot};
use gcg_search::{MechanismEffort, Observation, Preset, StrengthenedConfig, preset_config};
use serde_json::Value;

/// The searched decisions of the D2 vectors (`crates/gcg-ai/tests/vectors/search/`), as
/// `(state, player, per-decision seed)`.
pub fn searched_states(limit: usize) -> Vec<(GameState, u8, u64)> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../gcg-ai/tests/vectors/search/decisions.ndjson");
    let reg = Registry::embedded();
    std::fs::read_to_string(path)
        .expect("the vectors")
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("a record"))
        .filter(|rec| rec["search"].as_array().is_some_and(|s| !s.is_empty()))
        .take(limit)
        .map(|rec| {
            let snap = &rec["snapshot"];
            let st = from_canonical(
                &reg,
                &StateSnapshot {
                    state: snap["state"].clone(),
                    continuous: snap["continuous"].clone(),
                    filter_sets: snap["filter_sets"].clone(),
                    rules_checked: snap["rules_checked"].as_i64().expect("rules_checked"),
                },
            )
            .expect("a state");
            let player = u8::try_from(rec["player"].as_i64().expect("a seat")).expect("a seat");
            let seed = rec["seed"]
                .as_u64()
                .or_else(|| rec["seed"].as_str().and_then(|s| s.parse().ok()))
                .expect("a seed");
            (st, player, seed)
        })
        .collect()
}

/// The reference `standard` budget (the strengthened `fast` preset) with every mechanism off,
/// then the `field=value` overrides of `flags`.
pub fn config(flags: &[(&str, &str)]) -> StrengthenedConfig {
    let mut cfg = preset_config(Preset::Fast).clone();
    cfg.mechanisms = gcg_search::MechanismFlags::NULL;
    for (field, value) in flags {
        assert_eq!(
            cfg.mechanisms.set_text(field, value),
            Some(Ok(())),
            "{field}={value}"
        );
    }
    cfg.check_mechanisms().expect("an available configuration");
    cfg
}

/// A search outcome as comparable bits: the action, every alternative (value bits), the
/// statistics and the mechanism counters, and the generator state afterwards.
pub type Bits = (
    Action,
    Vec<(Action, u64, u32)>,
    gcg_ai::SearchStats,
    MechanismEffort,
    u64,
);

pub fn bits(r: &SearchResult, e: MechanismEffort, rng: u64) -> Bits {
    (
        r.action,
        r.alternatives
            .iter()
            .map(|a| (a.action, a.value.to_bits(), a.visits))
            .collect(),
        r.stats.clone(),
        e,
        rng,
    )
}

/// One search with `cfg` on the observation of `player` at `st` inside a pool task.
pub fn search_in(
    scratch: &mut Scratch,
    par: &TaskCx<'_, Scratch>,
    cancel: &Cancel,
    st: &GameState,
    player: u8,
    cfg: &StrengthenedConfig,
    seed: u64,
) -> Result<Bits, AgentError> {
    let mut cx = AgentCx {
        scratch,
        par,
        cancel,
    };
    let mut rng = SplitMix64::new(seed);
    let obs = Observation::new(st, player)?;
    let (r, e) = gcg_search::search_with_effort(&obs, cfg, &mut rng, &mut cx, &mut NoTrace)?;
    Ok(bits(&r, e, rng.state))
}

/// One search outside any pool (`Pool::run_inline`).
pub fn search_inline(st: &GameState, player: u8, cfg: &StrengthenedConfig, seed: u64) -> Bits {
    let cancel = Cancel::new();
    Pool::run_inline(|scratch: &mut Scratch, par: &TaskCx<'_, Scratch>| {
        search_in(scratch, par, &cancel, st, player, cfg, seed)
    })
    .unwrap_or_else(|e| panic!("search failed: {e}"))
}
