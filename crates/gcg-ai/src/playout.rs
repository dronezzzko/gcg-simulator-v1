//! Playing a (determinised) state forward with the heuristic policy (`ai/playout.py`;
//! docs/port/inventory/08-ai-policy-eval.md §5).
//!
//! Both functions mutate `st` in place and make both players' decisions with
//! [`crate::policy::choose`]; the engine's own randomness comes from the state's generator.
//! Each sampled multi-option decision draws one `random()` per option from `rng`; a single
//! option draws nothing.

use gcg_core::rng::SplitMix64;
use gcg_engine::{ApplyError, EngineError, GameState, apply};

use crate::PolicyKnobs;
use crate::math::FloatMath;
use crate::policy::{choose, choose_with};

/// `play_until_turn(st, turn, rng, temperature, max_decisions, knobs=BASELINE_KNOBS)`
/// (`ai/playout.py:12-26`): apply policy moves until the game ends, `st.turn` reaches `turn`,
/// `max_decisions` moves were made or no decision is pending (checked in that order before
/// every move). Returns the number of moves applied.
///
/// # Errors
/// Any error of the policy or of `apply(check=False)`.
pub fn play_until_turn(
    st: &mut GameState,
    turn: i32,
    mut rng: Option<&mut SplitMix64>,
    temperature: f64,
    max_decisions: u32,
    knobs: &PolicyKnobs,
) -> Result<u32, EngineError> {
    let mut n = 0;
    while st.winner().is_none() && st.turn() < turn && n < max_decisions && st.pending().is_some() {
        let action = choose(st, rng.as_deref_mut(), temperature, knobs)?;
        apply(st, action, false).map_err(engine_error)?;
        n += 1;
    }
    Ok(n)
}

/// `play_to_end(st, rng, temperature, max_decisions, knobs=BASELINE_KNOBS)`
/// (`ai/playout.py:29-41`): [`play_until_turn`] without the turn bound.
///
/// # Errors
/// Any error of the policy or of `apply(check=False)`.
pub fn play_to_end(
    st: &mut GameState,
    rng: Option<&mut SplitMix64>,
    temperature: f64,
    max_decisions: u32,
    knobs: &PolicyKnobs,
) -> Result<u32, EngineError> {
    play_to_end_with(
        st,
        rng,
        temperature,
        max_decisions,
        knobs,
        FloatMath::Platform,
    )
}

/// [`play_to_end`] with the policy's Gumbel draws at `math` ([`choose_with`], W-137): at
/// [`FloatMath::Platform`] it is [`play_to_end`], bit for bit.
///
/// # Errors
/// As [`play_to_end`].
pub fn play_to_end_with(
    st: &mut GameState,
    mut rng: Option<&mut SplitMix64>,
    temperature: f64,
    max_decisions: u32,
    knobs: &PolicyKnobs,
    math: FloatMath,
) -> Result<u32, EngineError> {
    let mut n = 0;
    while st.winner().is_none() && n < max_decisions && st.pending().is_some() {
        let action = choose_with(st, rng.as_deref_mut(), temperature, knobs, math)?;
        apply(st, action, false).map_err(engine_error)?;
        n += 1;
    }
    Ok(n)
}

/// An `apply(check=False)` error as the playouts report it. With a pending decision and no
/// legality check `apply` raises only engine errors (`engine/game.py:780-818`); the
/// `IllegalActionError` arm keeps Python's message for completeness.
fn engine_error(e: ApplyError) -> EngineError {
    match e {
        ApplyError::Engine(e) => e,
        ApplyError::Illegal(i) => EngineError::Value(i.to_string()),
    }
}
