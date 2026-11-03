//! GAP-A1's mechanisms and analysis tools (docs/port/eval/weakness-gaps.md: W-05, W-14, W-17,
//! W-25, W-26).
//!
//! | Flag | Item | Piece |
//! | --- | --- | --- |
//! | `target_multi_rollouts{R}` | W-14 plan item (2) | [`rollouts`], [`budget`]: `R` rollouts per iteration at an effect-target root decision, the budget divided by `R` |
//! | `tree_lethal_select` | W-25 | [`lethal_select`]: at an in-tree MAIN decision of the turn player, the first move of a lethal line P6-B's solver proves on the iteration's world |
//!
//! The analysis tools that `gcg-eval weakness` runs ([`api`], re-exported as
//! `gcg_search::gapa1`) read the information-set state only, never change a search, and are
//! not mechanisms: the W-05 split census, the W-17 proved-lethal check and the W-26 policy-level
//! Damage regret.

pub mod api;
mod census;
mod damage;
#[cfg(test)]
mod tests;

use gcg_ai::action_key;
use gcg_core::rng::SplitMix64;

use crate::MechanismFlags;
use crate::mcts::is_target_decision;
use crate::mech::{Budget, ChildIdx, NodeCx, RolloutCx, RootCx, p6b};

/// The mechanism flags this package implements.
pub(crate) const PROVIDES: [&str; 2] = ["target_multi_rollouts", "tree_lethal_select"];

/// The root node: the core creates it first (`mcts.rs`, `run`).
const ROOT: u32 = 0;

/// Whether the package changes the budget (`target_multi_rollouts`).
pub(crate) const fn divides_budget(f: &MechanismFlags) -> bool {
    f.target_multi_rollouts.is_some()
}

/// `tree_lethal_select`: at an in-tree MAIN decision of the turn player whose static attack step
/// may be lethal, the node's move that plays the first move of the lethal line P6-B's
/// `rollout_lethal` proves on the node's world (`p6b::rollout_lethal`, with its per-decision
/// solver cap in `state`); `None` elsewhere, when no line is proved, or when the solver fails
/// (the line's own rollout then meets the same error). The root is `lethal_root`'s.
pub(crate) fn lethal_select(state: &p6b::DecisionState, cx: &NodeCx<'_>) -> Option<ChildIdx> {
    if cx.node == ROOT {
        return None;
    }
    let mut unused = SplitMix64::new(0);
    let rollout = RolloutCx {
        s: cx.s,
        rng: &mut unused,
        player: cx.dec.hdr.player,
        cfg: cx.cfg,
        knobs: cx.knobs,
        math: cx.math,
    };
    let a = p6b::rollout_lethal(state, &rollout).ok().flatten()?;
    let key = action_key(cx.s, cx.dec, a);
    cx.moves
        .iter()
        .position(|&(k, m)| m == a || k == key)
        .map(ChildIdx)
}

/// The rollouts every iteration of this decision plays (`target_multi_rollouts`): `R` at an
/// effect-target root decision (`is_target_decision`, `ai/mcts.py:165-169`), one elsewhere.
pub(crate) fn rollouts(f: &MechanismFlags, cx: &RootCx<'_>) -> u32 {
    match f.target_multi_rollouts {
        Some(r) if r > 1 && is_target_decision(cx.dec) => r,
        _ => 1,
    }
}

/// The budget `b` in iterations of `rollouts` rollouts each: both counts divided by the rollouts
/// per iteration (rounded up), so the decision plays about as many rollouts as `b` plays
/// iterations. A count of one leaves `b` as it is.
pub(crate) fn budget(rollouts: u32, b: Budget) -> Budget {
    if rollouts <= 1 {
        return b;
    }
    let iterations = b.iterations.div_ceil(rollouts).max(1);
    Budget {
        iterations,
        cap: b.cap.div_ceil(rollouts).max(iterations),
    }
}
