//! P6-B's mechanisms (docs/port/PLAN.md §12.2, §12.3 S3): the tactical solver and what the
//! search does with it.
//!
//! | Flag | Items | What it does |
//! | --- | --- | --- |
//! | `tactical_solver` | W-03, W-17 | a rollout's turn player plays a lethal line the solver proves on the rollout world (`rollout_lethal`) |
//! | `lethal_root` | W-17, W-03 | at the root, a lethal of the searcher proved in each of the first K worlds is played (`root.rs`); during the opponent's turn, root moves after which the opponent has a forced lethal in every one of the K worlds are dropped while another move remains |
//! | `threat_leaf` | W-03 | a leaf whose turn player has a proved lethal at its MAIN decision takes the time-shaped terminal value of that player's win (the loss value when it is the opponent) |
//! | `critical_extension` | W-16 | MAIN, BLOCK and ACTION_STEP decisions where the static attack step is lethal for either side get two extra budgets for the close-call extension |
//! | `effective_blockers` | W-32, W-38 | the leaf evaluation's Blocker counts (`can_kill`, `_race`) by the engine's eligibility and the strengthened race of `board::race` |
//! | `solver_can_kill` | W-38 | the leaf evaluation's `can_kill` features by the exact static attack step (`board::static_lethal`) |
//!
//! The solver (`solver.rs`) is an engine-backed AND-OR search of the rest of one turn on a
//! determinised world; the static attack step (`board.rs`) solves the effect-free game exactly
//! and gates the solver. Every engine apply of the solver is counted: the root's as the
//! decision's root applies, the rollouts' and leaves' as auxiliary applies, the latter capped
//! at [`SOLVER_APPLIES_PER_ITERATION`] per iteration of the decision's cap, after which the
//! decision's rollouts and leaves are the reference's. With every P6-B flag off nothing here
//! runs and the composed search is unchanged.

#[cfg(all(test, feature = "slow"))]
mod audit;
pub(crate) mod board;
#[cfg(all(test, feature = "slow"))]
mod can_kill_census;
pub(crate) mod leaf;
pub(crate) mod root;
pub(crate) mod solver;
#[cfg(test)]
mod tests;

use std::cell::Cell;

use gcg_engine::{Action, DecisionKind, EngineError};

use super::{Budget, RolloutCx, RootCx};
use crate::MechanismFlags;
use board::{Board, Turn, player_attacks, static_lethal};
use solver::{Limits, Verdict, lethal_line};

/// The mechanism flags this package implements.
pub(crate) const PROVIDES: [&str; 6] = [
    "critical_extension",
    "effective_blockers",
    "lethal_root",
    "solver_can_kill",
    "tactical_solver",
    "threat_leaf",
];

/// The solver applies the rollouts and leaves of one decision may spend per iteration of its
/// cap (PLAN §12.3: solver applies, capped).
pub(crate) const SOLVER_APPLIES_PER_ITERATION: u64 = 16;

/// The extra budgets of a critical decision (W-16: up to two).
pub(crate) const CRITICAL_BUDGETS: u32 = 2;

/// The bound of one solve in a rollout or at a leaf: attacks only, the turn player passes its
/// own action steps, the defender's replies are all explored.
pub(crate) const LINE_LIMITS: Limits = Limits {
    max_applies: 400,
    setups: 0,
    own_tricks: false,
};

/// Whether any flag of the package is on.
pub(crate) const fn any_on(f: &MechanismFlags) -> bool {
    f.tactical_solver
        || f.lethal_root
        || f.threat_leaf
        || f.critical_extension
        || f.effective_blockers
        || f.solver_can_kill
}

/// Whether the package changes the leaf value.
pub(crate) const fn changes_leaf(f: &MechanismFlags) -> bool {
    f.threat_leaf || f.effective_blockers || f.solver_can_kill
}

/// The package's state for one search decision: the solver applies spent by rollouts and
/// leaves and their cap, and those of the root judgements.
#[derive(Debug, Default)]
pub(crate) struct DecisionState {
    aux: Cell<u64>,
    cap: Cell<u64>,
    root: Cell<u64>,
}

impl DecisionState {
    pub(crate) const fn new() -> Self {
        Self {
            aux: Cell::new(0),
            cap: Cell::new(0),
            root: Cell::new(0),
        }
    }

    /// The state a root tree of this decision starts from (S1): no applies spent, and the
    /// share `1/trees` of the decision's cap.
    pub(crate) fn for_tree(&self, trees: u32) -> Self {
        Self {
            aux: Cell::new(0),
            cap: Cell::new(self.cap.get().div_ceil(u64::from(trees.max(1)))),
            root: Cell::new(0),
        }
    }

    /// The solver applies of rollouts and leaves in this decision.
    pub(crate) const fn aux_applies(&self) -> u64 {
        self.aux.get()
    }

    /// The solver applies of the root judgements.
    pub(crate) const fn root_applies(&self) -> u64 {
        self.root.get()
    }

    /// Whether rollouts and leaves may still run the solver.
    pub(crate) fn has_budget(&self) -> bool {
        self.aux.get() < self.cap.get()
    }

    pub(crate) fn spend(&self, applies: u64) {
        self.aux.set(self.aux.get().saturating_add(applies));
    }

    pub(crate) fn spend_root(&self, applies: u64) {
        self.root.set(self.root.get().saturating_add(applies));
    }

    fn set_cap(&self, cap: u64) {
        self.cap.set(cap);
    }
}

/// `tactical_solver`: at a rollout's MAIN decision of the turn player whose static attack step
/// is lethal, the first move of a lethal line the solver proves on the rollout world; `None`
/// otherwise (the rollout policy decides). The rollout world is fully determinised, so the
/// proof is exact within the bound.
///
/// # Errors
/// An unimplemented card's error (other engine errors leave the decision to the policy).
pub(crate) fn rollout_lethal(
    state: &DecisionState,
    cx: &RolloutCx<'_>,
) -> Result<Option<Action>, EngineError> {
    let s = cx.s;
    let Some(dec) = s.pending() else {
        return Ok(None);
    };
    let p = dec.hdr.player;
    if dec.hdr.kind != DecisionKind::Main || p != s.active() || !state.has_budget() {
        return Ok(None);
    }
    let attacks = player_attacks(dec.options);
    if attacks.is_empty() || !Board::of(s, p, Turn::Now).is_ok_and(|b| b.may_be_lethal()) {
        return Ok(None);
    }
    let out = lethal_line(s, p, LINE_LIMITS, &cx.knobs, &attacks)?;
    state.spend(out.applies);
    Ok(match out.verdict {
        Verdict::Lethal(a) => Some(a),
        Verdict::NoLethal | Verdict::Unknown => None,
    })
}

/// Whether the decision at the root is critical (W-16): a MAIN, BLOCK or ACTION_STEP decision
/// at which the static attack step is lethal for the searcher (this turn when it is the turn
/// player, else its next turn) or for the opponent (the rest of its turn when it is the turn
/// player, else its next turn against the searcher's Blockers as they are now).
fn critical(cx: &RootCx<'_>) -> bool {
    if !matches!(
        cx.dec.hdr.kind,
        DecisionKind::Main | DecisionKind::Block | DecisionKind::ActionStep
    ) {
        return false;
    }
    let (st, me) = (cx.st, cx.player);
    let opp = 1 - me;
    let mine = if st.active() == me {
        Turn::Now
    } else {
        Turn::Next
    };
    let theirs = if st.active() == opp {
        Turn::Rest
    } else {
        Turn::Next
    };
    static_lethal(st, me, mine).unwrap_or(false) || static_lethal(st, opp, theirs).unwrap_or(false)
}

/// The budget with P6-B's changes: the solver cap of the decision's rollouts and leaves is set
/// from the cap, and with `critical_extension` a critical decision may extend by the
/// close-call rule up to [`CRITICAL_BUDGETS`] extra budgets.
pub(crate) fn budget(
    state: &DecisionState,
    flags: &MechanismFlags,
    cx: &RootCx<'_>,
    b: Budget,
) -> Budget {
    let mut out = b;
    if flags.critical_extension && critical(cx) {
        let extended = b.iterations.saturating_mul(1 + CRITICAL_BUDGETS);
        if extended > out.cap {
            out.cap = extended;
        }
    }
    state.set_cap(SOLVER_APPLIES_PER_ITERATION.saturating_mul(u64::from(out.cap)));
    out
}
