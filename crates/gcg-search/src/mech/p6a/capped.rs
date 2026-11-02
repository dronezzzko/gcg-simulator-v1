//! `capped_rollout_continuation` (W-15, PLAN §12.3 S2): a rollout that stopped at its decision
//! cap (`max_rollout_decisions`, `ai/config.py:33`) stopped wherever it was, possibly in a battle
//! or an effect's resolution, where the reference scores it by the static evaluation
//! (`ai/mcts.py:211-214`). Here the capped line is continued on a copy with the deterministic
//! policy (the reference policy's best option, no random draws) to the next main-phase decision
//! of either player, for at most [`CONTINUATION_DECISIONS`] more decisions, and scored there;
//! a line still unsettled after them scores 0.5. Every apply of the continuation counts as
//! auxiliary work, and every capped line is counted (`DecisionState`).

use gcg_ai::{apply_unchecked, choose};
use gcg_engine::{DecisionKind, EngineError, GameState};

/// The most decisions a capped rollout is continued by.
pub(crate) const CONTINUATION_DECISIONS: u32 = 64;

/// Where a continued line ended.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Continued {
    /// At a main-phase decision or with the game over: the leaf is taken here.
    Settled { applies: u32 },
    /// Still resolving after [`CONTINUATION_DECISIONS`] decisions.
    Unsettled { applies: u32 },
}

/// A position the leaf may be taken at: the game is over, or a main-phase decision is pending
/// with nothing resolving.
pub(crate) fn settled(s: &GameState) -> bool {
    if s.winner().is_some() {
        return true;
    }
    s.pending()
        .is_some_and(|d| d.hdr.kind == DecisionKind::Main)
        && s.battles().is_empty()
        && s.frames().is_empty()
        && s.pending_triggers().is_empty()
}

/// Continues the capped line `s` in place with the deterministic policy of `knobs` until it is
/// [`settled`], the line's horizon turn `horizon` is passed, or [`CONTINUATION_DECISIONS`]
/// decisions were made.
///
/// # Errors
/// The policy's and the engine's errors (an error leaf of the line).
pub(crate) fn continue_line(
    s: &mut GameState,
    horizon: i32,
    knobs: &gcg_ai::PolicyKnobs,
) -> Result<Continued, EngineError> {
    let mut applies = 0;
    while !settled(s) && applies < CONTINUATION_DECISIONS && s.turn() <= horizon {
        if s.pending().is_none() {
            break;
        }
        let a = choose(s, None, 0.0, knobs)?;
        apply_unchecked(s, a)?;
        applies += 1;
    }
    Ok(if settled(s) && s.turn() <= horizon {
        Continued::Settled { applies }
    } else {
        Continued::Unsettled { applies }
    })
}
