//! Engine diagnostics that change no game and no output (docs/port/PLAN.md §12.2 P6-J): which
//! safety cap ended a game (W-46), how many derived views reached the fixpoint cap without
//! converging (W-48) and which interpretation-tagged code paths ran (W-101). Nothing here is
//! serialised, hashed or read by the engine itself.

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
    static INTERPRETATION_RUNS: [Cell<u64>; Interpretation::ALL.len()] =
        const { [const { Cell::new(0) }; Interpretation::ALL.len()] };
}

/// W-101: the interpretations of `docs/port/audit/interpretations.json` (W-56) whose code path
/// records when it applies. Each variant names the code path; [`note_interpretation`] counts
/// it on the running thread, and [`interpretation_runs`] reads the counts. The other
/// interpretations of the table have no single engine code path: compile-time readings of the
/// card text (I-19, I-21, I-24, I-26), data overrides (I-25), and card-specific resolutions
/// traced by their cards (`gcg-runner` `analysis::interpretations_used`).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Interpretation {
    /// I-01: an attack on a Base (`battle::attack_base`): the Base deals no battle damage back.
    I01,
    /// I-02: a token arriving outside the token zones leaves the game (`core::move_single`).
    I02,
    /// I-03: a Resource or EX Resource placement at the cap does nothing
    /// (`interp::deploy::place_resource`, `place_ex_resource`).
    I03,
    /// I-04: a lasting effect on an `All` selection holds the cards the selection names when it
    /// resolves (`interp::lasting::h_apply`); a card that arrives later does not get it.
    I04,
    /// I-05: the uids of a new game are assigned after the seeded shuffle and cut
    /// (`game::new_game`).
    I05,
    /// I-06: several cards placed into one deck at once are ordered by their owner, or a
    /// shuffle orders them with the seeded RNG (`interp::cards::h_to_deck`).
    I06,
    /// I-10: a destroyed-cause condition reads the destruction event (`view::hooks::destroyed`).
    I10,
    /// I-11: a modal effect is playable, and a mode is offered, only if the mode's mandatory
    /// targets can all be chosen; with no such mode the effect does nothing
    /// (`interp::choose::targets_available`, `interp::h_mode`).
    I11,
    /// I-13: a card deployed as its "(APx･HPy) Unit" variant is that Unit in the battle area
    /// and the printed card anywhere else (`interp::deploy::h_deploy_card`, `core::move_single`).
    I13,
    /// I-14: a targeted choice from the removal area is no target choice
    /// (`interp::choose::is_target_choice`).
    I14,
    /// I-15: a card placed into the Shields goes on top (`core::move_single`).
    I15,
    /// I-16: an attack with 0 AP on a player with no Base and no Shields does not defeat
    /// (`battle::damage_step`).
    I16,
    /// I-17: the LINKED event (【When Linked】) is emitted only by a pairing that is linked
    /// (`interp::play::pair`).
    I17,
    /// I-18: <Suppression> on an attack that hits a Base does nothing (`battle::damage_step`).
    I18,
}

impl Interpretation {
    /// Every anchored interpretation, in id order.
    pub const ALL: [Self; 14] = [
        Self::I01,
        Self::I02,
        Self::I03,
        Self::I04,
        Self::I05,
        Self::I06,
        Self::I10,
        Self::I11,
        Self::I13,
        Self::I14,
        Self::I15,
        Self::I16,
        Self::I17,
        Self::I18,
    ];

    /// The id in `docs/port/audit/interpretations.json`.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::I01 => "I-01",
            Self::I02 => "I-02",
            Self::I03 => "I-03",
            Self::I04 => "I-04",
            Self::I05 => "I-05",
            Self::I06 => "I-06",
            Self::I10 => "I-10",
            Self::I11 => "I-11",
            Self::I13 => "I-13",
            Self::I14 => "I-14",
            Self::I15 => "I-15",
            Self::I16 => "I-16",
            Self::I17 => "I-17",
            Self::I18 => "I-18",
        }
    }

    const fn index(self) -> usize {
        self as usize
    }
}

/// W-101: one run of the interpretation-tagged code path `i` on this thread.
#[inline]
pub(crate) fn note_interpretation(i: Interpretation) {
    INTERPRETATION_RUNS.with(|runs| {
        let c = &runs[i.index()];
        c.set(c.get().saturating_add(1));
    });
}

/// W-101: how often each anchored interpretation's code path has run on this thread, in
/// [`Interpretation::ALL`] order. Callers read it before and after a game; it never changes
/// one.
#[must_use]
pub fn interpretation_runs() -> [u64; Interpretation::ALL.len()] {
    INTERPRETATION_RUNS.with(|runs| std::array::from_fn(|i| runs[i].get()))
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
    fn interpretation_counters_belong_to_their_thread_and_count_by_id() {
        let before = interpretation_runs();
        note_interpretation(Interpretation::I16);
        note_interpretation(Interpretation::I16);
        let after = interpretation_runs();
        for (k, i) in Interpretation::ALL.iter().enumerate() {
            let want = u64::from(*i == Interpretation::I16) * 2;
            assert_eq!(after[k] - before[k], want, "{}", i.id());
        }
        let other = std::thread::spawn(interpretation_runs)
            .join()
            .expect("joins");
        assert!(other.iter().all(|&n| n == 0));
        let mut ids: Vec<&str> = Interpretation::ALL.iter().map(|i| i.id()).collect();
        let sorted = ids.clone();
        ids.sort();
        ids.dedup();
        assert_eq!(ids, sorted, "ids are distinct and in order");
    }

    #[test]
    fn cap_names() {
        assert_eq!(CapKind::Turns.as_str(), "turns");
        assert_eq!(CapKind::Decisions.as_str(), "decisions");
    }
}
