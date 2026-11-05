//! The analysis entry points of GAP-A1 that `gcg-eval weakness` runs (re-exported as
//! `gcg_search::gapa1`). None of them changes a search or a game: they read a state (the
//! decider's information set where a decision is judged) and report.
//!
//! - [`proved_lethal`] (W-17): the tactical solver of `lethal_root` from the decider's
//!   information, the first move of a this-turn lethal line that holds in every given world.
//! - [`lethal_on_state`] (W-17): the same solver on the state itself (an analysis may read the
//!   true state; the agents never do).
//! - [`damage_judgement`] (W-26): the policy-level judgement of one Damage decision.
//! - [`split_census`] (W-05): the observation-key splits of one search.

use gcg_ai::{AgentCx, AgentError, SearchResult};
use gcg_engine::observe::determinize;
use gcg_engine::{Action, EngineError, GameState};

use crate::mech::p6b::root::LETHAL_LIMITS;
use crate::mech::p6b::solver::{Verdict, first_moves, lethal_line};
use crate::{Observation, StrengthenedConfig};

pub use super::census::SplitCensus;
pub use super::damage::DamageJudgement;

/// The most moves of one oracle playout of [`damage_judgement`] before the evaluation scores it.
pub const ORACLE_MAX_DECISIONS: u32 = super::damage::MAX_DECISIONS;

/// The first move of a lethal line of `player` at its pending decision in `world` that starts
/// with one of `first`, if the solver proves one.
fn lethal_in(
    world: &GameState,
    player: u8,
    cfg: &StrengthenedConfig,
    first: &[Action],
) -> Result<Option<Action>, EngineError> {
    let out = lethal_line(world, player, LETHAL_LIMITS, &cfg.knobs(), first)?;
    Ok(match out.verdict {
        Verdict::Lethal(a) => Some(a),
        Verdict::NoLethal | Verdict::Unknown => None,
    })
}

/// The first move of a lethal line that the tactical solver of `lethal_root` (bounded by its
/// limits, `p6b::root::LETHAL_LIMITS`) proves for `player` at its pending decision in every world
/// `determinize(info, player, s)` for `s` in `world_seeds`, where `info` is `player`'s
/// information set at `st`: the line is found in the first world and its first move must be
/// lethal in each other one. `None` when `player` is not to decide, without a world, or when no
/// such line is proved.
///
/// # Errors
/// The engine's errors of a world or of the solver (an unimplemented card).
pub fn proved_lethal(
    st: &GameState,
    player: u8,
    cfg: &StrengthenedConfig,
    world_seeds: &[u64],
) -> Result<Option<Action>, EngineError> {
    let Some(dec) = st.pending().filter(|d| d.hdr.player == player) else {
        return Ok(None);
    };
    let Some((&first_seed, rest)) = world_seeds.split_first() else {
        return Ok(None);
    };
    let obs = Observation::new(st, player)?;
    let info = obs.info();
    let first = first_moves(&dec, dec.options, true);
    let world = determinize(info, player, first_seed)?;
    let Some(a) = lethal_in(&world, player, cfg, &first)? else {
        return Ok(None);
    };
    for &seed in rest {
        let w = determinize(info, player, seed)?;
        if lethal_in(&w, player, cfg, &[a])? != Some(a) {
            return Ok(None);
        }
    }
    Ok(Some(a))
}

/// The first move of a lethal line of `player` at its pending decision in `st` itself.
///
/// # Errors
/// As [`proved_lethal`].
pub fn lethal_on_state(
    st: &GameState,
    player: u8,
    cfg: &StrengthenedConfig,
) -> Result<Option<Action>, EngineError> {
    let Some(dec) = st.pending().filter(|d| d.hdr.player == player) else {
        return Ok(None);
    };
    let first = first_moves(&dec, dec.options, true);
    lethal_in(st, player, cfg, &first)
}

/// Whether `action`, a spending play (`gcg_ai::SPENDING`) of `player` at its pending decision in
/// `st`, has no effect by the reference's no-effect check (`gcg_ai::without_effect_with` at
/// `cfg`'s math, which is `gcg_ai::without_effect` with `portable_math` off; `ai/noop.py`) on
/// `player`'s information set, with `cfg`'s no-effect options and weights and the check's world
/// seeded `seed`. `false` for any other action.
///
/// # Errors
/// The engine's errors of the observation or the check.
pub fn without_effect(
    st: &GameState,
    player: u8,
    action: Action,
    cfg: &StrengthenedConfig,
    seed: u64,
) -> Result<bool, EngineError> {
    if !gcg_ai::SPENDING.contains(&action.kind)
        || st.pending().is_none_or(|d| d.hdr.player != player)
    {
        return Ok(false);
    }
    let obs = Observation::new(st, player)?;
    let info = obs.info();
    let Some(dec) = info.pending() else {
        return Ok(false);
    };
    let moves = gcg_ai::canonical(info, &dec);
    let Some(&played) = moves.iter().find(|(_, m)| *m == action) else {
        return Ok(false);
    };
    let idle = gcg_ai::without_effect_with(
        info,
        player,
        &dec,
        &[played],
        seed,
        &cfg.knobs(),
        gcg_ai::NoopFlags {
            branch_choices: cfg.noop_branch_choices,
            resource_aware: cfg.noop_resource_aware,
        },
        &cfg.weights,
        crate::mech::math_of(cfg).float(),
    )?;
    Ok(idle.contains(&played.0))
}

/// Whether the decision pending in `st` is a Damage decision (`damage.rs`).
#[must_use]
pub fn is_damage_decision(st: &GameState, cfg: &StrengthenedConfig) -> bool {
    super::damage::damage_amount(st, cfg).is_some()
}

/// The policy-level judgement of the Damage decision pending in `st` for `player` (`damage.rs`:
/// both rollout policies' picks and the oracle value of every option over `worlds` shared
/// worlds seeded from `seed`), or `None` for another decision.
///
/// # Errors
/// The engine's errors of a world, a policy or a playout.
pub fn damage_judgement(
    st: &GameState,
    player: u8,
    cfg: &StrengthenedConfig,
    worlds: u32,
    seed: u64,
) -> Result<Option<DamageJudgement>, EngineError> {
    super::damage::judge(st, player, cfg, worlds, seed)
}

/// The observation-key census of one search of `cfg` at `player`'s decision in `st`
/// (`census.rs`), with the search's result.
///
/// # Errors
/// The search's errors, and an engine error of a replayed walk.
pub fn split_census(
    st: &GameState,
    player: u8,
    cfg: &StrengthenedConfig,
    seed: u64,
    cx: &mut AgentCx<'_>,
) -> Result<(SplitCensus, SearchResult), AgentError> {
    super::census::census(st, player, cfg, seed, cx)
}
