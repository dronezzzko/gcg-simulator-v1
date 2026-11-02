//! `adaptive_horizon` (W-01, PLAN §12.3 S2): a line whose rollout stops at its horizon in an
//! unsettled position plays one more turn boundary. Unsettled means, judged from the line's own
//! determinised world (never the true state, so the extension stays information-set honest):
//!
//! - something is still resolving: a battle, an effect frame or a pending trigger, or a pending
//!   decision other than a main-phase one (a start-of-turn trigger, for example); or
//! - either side could win within two turns on the current boards: the evaluation's race
//!   estimate (`_race`, `ai/evaluation.py:244-277`, read through `role_features`, whose last two
//!   terms are `1 / race` of the turn player and of the other player) is at most
//!   [`RACE_TURNS`], so the leaf would price a fight the next turn decides.
//!
//! The extension adds one turn boundary and a full rollout decision cap for it; it applies once
//! per line ([`MAX_EXTENSION`]), so a line advances at most `horizon_turns + 1` turns, which the
//! leaf mapping's check covers (`leaf.rs`). A capped rollout is not extended (it stopped before
//! its horizon; `capped_rollout_continuation` handles it).

use gcg_ai::role_features;
use gcg_engine::{DecisionKind, EngineError, GameState};

use crate::mech::{HorizonDecision, LeafCx};

/// The turn boundaries one line may be extended by.
pub(crate) const MAX_EXTENSION: i32 = 1;

/// A race estimate of at most this many turns makes a leaf unsettled.
pub(crate) const RACE_TURNS: u8 = 2;

/// `1 / RACE_TURNS`: `role_features` reports `1 / race` with `race` an integer in `1..=6`, and
/// `1 / 1 > 1 / 2 > 1 / 3` in floating point, so `race <= 2` is exactly `1 / race >= 0.5`.
const INV_RACE_BOUND: f64 = 1.0 / RACE_TURNS as f64;

/// The index of `inv_race_turn_player` in `ROLE_FEATURES`; `inv_race_other` follows it.
pub(crate) const INV_RACE: usize = gcg_ai::ROLE_FEATURES.len() - 2;

/// Why a leaf is unsettled (or that it is settled).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Unsettled {
    Settled,
    /// A battle, an effect frame, a pending trigger or a non-main-phase decision.
    Resolving,
    /// A race estimate of at most [`RACE_TURNS`].
    Race,
}

/// Whether the world `s` is unsettled for an extension (module docs); `density` is the
/// search's Burst density, as the evaluation reads it.
///
/// # Errors
/// The derived view's errors.
pub(crate) fn unsettled(s: &GameState, density: gcg_ai::Density) -> Result<Unsettled, EngineError> {
    let resolving = !s.battles().is_empty()
        || !s.frames().is_empty()
        || !s.pending_triggers().is_empty()
        || s.pending()
            .is_some_and(|d| d.hdr.kind != DecisionKind::Main);
    if resolving {
        return Ok(Unsettled::Resolving);
    }
    let r = role_features(s, density)?;
    let close = r[INV_RACE..].iter().any(|&inv| inv >= INV_RACE_BOUND);
    Ok(if close {
        Unsettled::Race
    } else {
        Unsettled::Settled
    })
}

/// How many extensions the line of `cx` has used.
pub(crate) const fn extensions_used(cx: &LeafCx<'_>) -> i32 {
    cx.horizon - (cx.root_turn + cx.cfg.horizon_turns)
}

/// The `adaptive_horizon` decision at a stopped rollout: extend a line that reached its horizon
/// unfinished, uncapped and unsettled, once. A derived-view error ends the line here: the leaf
/// value reads the same view and reports the error (`Mechanisms::error_leaf` decides).
pub(crate) fn decide(cx: &LeafCx<'_>) -> HorizonDecision {
    let s = cx.s;
    let at_horizon =
        s.winner().is_none() && !cx.capped && s.pending().is_some() && s.turn() >= cx.horizon;
    if !at_horizon || extensions_used(cx) >= MAX_EXTENSION {
        return HorizonDecision::Leaf;
    }
    match unsettled(s, cx.density) {
        Ok(Unsettled::Resolving | Unsettled::Race) => HorizonDecision::Extend {
            turns: 1,
            decisions: cx.cfg.max_rollout_decisions,
        },
        Ok(Unsettled::Settled) | Err(_) => HorizonDecision::Leaf,
    }
}
