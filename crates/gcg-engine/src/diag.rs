//! Engine diagnostics that change no game and no output (docs/port/PLAN.md §12.2 P6-J): which
//! safety cap ended a game (W-46) and how many derived views reached the fixpoint cap without
//! converging (W-48). Nothing here is serialised, hashed or read by the engine itself.

use std::cell::Cell;

use crate::{EndReason, GameState};

/// The engine safety cap that ended a game (W-46). Python reports both as
/// `EndReason.TURN_LIMIT` (`engine/game.py:325-332`, `:737-742`), and so does every output of
/// the port that must equal Python's; this only tells the two apart, for diagnostics.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum CapKind {
    /// `_next_turn` at `turn >= turn_limit` (`game.py:325-332`).
    Turns,
    /// `advance` at `action_count > max_actions` (`game.py:737-742`).
    Decisions,
}

impl CapKind {
    /// The diagnostic name: `turns` or `decisions`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Turns => "turns",
            Self::Decisions => "decisions",
        }
    }
}

impl GameState {
    /// W-46: the safety cap that ended this game, or `None` for a game that is running or
    /// ended any other way. It needs no state of its own: `advance` checks the decision cap
    /// first in every iteration and `action_count` changes only in `apply`, so the turn cap
    /// (`turn_step` inside the same iteration) can fire only while `action_count <=
    /// max_actions`, and the decision cap only when it is greater. Both values are part of
    /// the canonical state, so a reloaded state answers the same.
    #[must_use]
    pub fn cap_kind(&self) -> Option<CapKind> {
        if self.end_reason != Some(EndReason::TurnLimit) {
            return None;
        }
        Some(if self.action_count > self.max_actions() {
            CapKind::Decisions
        } else {
            CapKind::Turns
        })
    }
}

/// W-54: `DecisionKind::Payment` and `Step::AttackTriggers` exist only so that every Python
/// value parses and serialises (`engine/types.py:79`, `:114`); the engine never produces them.
/// EX-versus-normal payment is folded into the play action's `c`: `c` is the number of EX
/// Resources used (`payment_choices`, `game.py:372-382`), plus `ALT_PLAY_BASE * (i + 1)` for
/// the `i`-th alternative play cost (`game.py:32`, `:631`; `game.rs` `alt_play_options`), so
/// no PAYMENT decision is needed. Attack triggers resolve as a trigger batch before the
/// attack-end check (rule 8-2-2), so no code enters the step. Debug builds and the `paranoid`
/// feature assert both where decisions are created and in every iteration of `advance`;
/// other builds compile the check out.
#[inline]
pub(crate) fn assert_live_decision(kind: crate::DecisionKind) {
    if cfg!(any(debug_assertions, feature = "paranoid")) {
        assert!(
            kind != crate::DecisionKind::Payment,
            "W-54: the engine created a PAYMENT decision"
        );
    }
}

/// W-54: see [`assert_live_decision`].
#[inline]
pub(crate) fn assert_live_step(step: crate::Step) {
    if cfg!(any(debug_assertions, feature = "paranoid")) {
        assert!(
            step != crate::Step::AttackTriggers,
            "W-54: the engine entered Step.ATTACK_TRIGGERS"
        );
    }
}

thread_local! {
    static NON_CONVERGED: Cell<u64> = const { Cell::new(0) };
}

/// W-48: the fixpoint of a view computation stopped at `FIXPOINT_PASSES` without converging.
pub(crate) fn note_non_converged() {
    NON_CONVERGED.with(|n| n.set(n.get().saturating_add(1)));
}

/// W-48: how many derived views computed on this thread so far reached the fixpoint cap
/// without converging (each keeps the last pass, as Python does). Fuzz and paranoid runs
/// read it before and after their games; it never changes a game.
#[must_use]
pub fn non_converged_views() -> u64 {
    NON_CONVERGED.with(Cell::get)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_counter_belongs_to_its_thread() {
        let before = non_converged_views();
        note_non_converged();
        assert_eq!(non_converged_views(), before + 1);
        let other = std::thread::spawn(non_converged_views)
            .join()
            .expect("joins");
        assert_eq!(other, 0);
    }

    #[test]
    fn cap_names() {
        assert_eq!(CapKind::Turns.as_str(), "turns");
        assert_eq!(CapKind::Decisions.as_str(), "decisions");
    }
}
