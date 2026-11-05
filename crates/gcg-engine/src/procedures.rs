//! Engine entry points the AI calls on its own clones (docs/port/contracts.md, gcg-engine
//! `procedures`; PLAN §10.3.3). The read-only ones may fill the state's view cache, exactly as
//! the Python functions they port do (`view::derived`, PLAN §4.6).

use gcg_effects::Timing;

use crate::{Action, Battle, EngineError, GameState, Uid};

/// `battle.declare_attack(st, attacker, target)` (`engine/battle.py:12-32`, rule 8-2-1).
///
/// # Errors
/// What the attack declaration raises (its `ATTACKS` event queues triggers).
pub fn declare_attack(st: &mut GameState, attacker: Uid, target: i32) -> Result<(), EngineError> {
    crate::battle::declare_attack(st, attacker, target)
}

/// `battle.eligible_blockers(st, b)` (`engine/battle.py:47-73`; rules 8-3-1..8-3-3, 13-1-4,
/// 13-1-6).
///
/// # Errors
/// An error of the derived view.
pub fn eligible_blockers(st: &GameState, b: &Battle) -> Result<Vec<Uid>, EngineError> {
    crate::battle::eligible_blockers(st, b)
}

/// `game.main_options(st)` (`engine/game.py:543-583`, rule 7-5-1).
///
/// # Errors
/// An error of the derived view or of a condition, value or target check.
pub fn main_options(st: &GameState) -> Result<Vec<Action>, EngineError> {
    crate::game::main_options(st)
}

/// `game.action_step_options(st, p)` (`engine/game.py:647-660`, rule 9-3).
///
/// # Errors
/// As [`main_options`].
pub fn action_step_options(st: &GameState, p: u8) -> Result<Vec<Action>, EngineError> {
    crate::game::action_step_options(st, p)
}

/// `game._activated_list(st, p, timing)` (`engine/game.py:410-433`): `(host, aid or
/// SUPPORT_AID, resource cost)` per activatable ability.
///
/// # Errors
/// As [`main_options`].
pub fn activated_list(
    st: &GameState,
    p: u8,
    timing: Timing,
) -> Result<Vec<(Uid, i32, i32)>, EngineError> {
    crate::game::activated_list(st, p, timing)
}

/// `game._command_playable(st, p, uid, timing)` (`engine/game.py:395-407`).
///
/// # Errors
/// As [`main_options`].
pub fn command_playable(
    st: &GameState,
    p: u8,
    uid: Uid,
    timing: Timing,
) -> Result<bool, EngineError> {
    crate::game::command_playable(st, p, uid, timing)
}

/// `rested.cards[uid].rested = rested; rested.touch()` (`ai/actions.py:228-229`): the only
/// write the AI makes into a state of its own, a trial world.
pub fn set_rested_for_trial(st: &mut GameState, uid: Uid, rested: bool) {
    st.cards[usize::from(uid)].rested = rested;
    st.touch();
}
