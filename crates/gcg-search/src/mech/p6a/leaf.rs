//! `generalized_leaf` (W-02, PLAN §12.3 S2): the leaf mapping as parameters. A finished line
//! scores `1 - decay * turns` for a win and `decay * turns` for a loss (0.5 for a draw), an
//! unfinished one `floor + (1 - 2 floor) * σ(score / value_scale)`, with
//! `decay = floor / (max_turns + 1)` and `max_turns = max(horizon_turns, 1) + max_extension`
//! the longest line a configuration plays, so a certain result outranks every estimate at any
//! horizon. The reference keeps `LEAF_FLOOR` and `WIN_DECAY` (`ai/config.py:9-10`) and its own
//! check (`ai/config.py:48-53`, `gcg_ai_reference::ReferenceConfig::check`).
//!
//! Why `max(horizon_turns, 1)`: the root move is applied before the horizon test, so even a
//! horizon of 0 turns can end its line one turn boundary later; after that every apply happens
//! before the line's horizon turn, and one apply crosses at most one turn boundary.

use super::horizon::MAX_EXTENSION;
use crate::StrengthenedConfig;
use crate::mech::reference::{LEAF_FLOOR, WIN_DECAY};

/// The `floor` of the generalised mapping: the reference's `LEAF_FLOOR`, so that only the decay
/// changes with the horizon.
pub(crate) const FLOOR: f64 = LEAF_FLOOR;

/// The leaf constants of a configuration and the longest line they must cover.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) struct LeafMapping {
    pub(crate) floor: f64,
    pub(crate) decay: f64,
    /// The most turns a simulated line can advance past its root turn.
    pub(crate) max_turns: i32,
    /// Whether these are the reference constants (the check's message names them).
    pub(crate) reference: bool,
}

/// The most turns a line of a configuration with `horizon_turns` and `max_extension` can
/// advance past its root turn.
pub(crate) const fn max_turns(horizon_turns: i32, max_extension: i32) -> i32 {
    let h = if horizon_turns < 1 { 1 } else { horizon_turns };
    let e = if max_extension < 0 { 0 } else { max_extension };
    h.saturating_add(e)
}

impl LeafMapping {
    /// `LEAF_FLOOR` and `WIN_DECAY` over lines of up to `max_turns` turns.
    pub(crate) const fn reference(max_turns: i32) -> Self {
        Self {
            floor: LEAF_FLOOR,
            decay: WIN_DECAY,
            max_turns,
            reference: true,
        }
    }

    /// `floor` and `decay = floor / (max_turns + 1)` for `horizon_turns` and `max_extension`.
    pub(crate) fn generalized(horizon_turns: i32, max_extension: i32) -> Self {
        let max_turns = max_turns(horizon_turns, max_extension);
        Self {
            floor: FLOOR,
            decay: FLOOR / f64::from(max_turns.saturating_add(1)),
            max_turns,
            reference: false,
        }
    }

    /// The mapping `cfg` scores its leaves with: generalised with `generalized_leaf`, the
    /// reference constants otherwise, over the longest line of its horizon and extensions.
    pub(crate) fn of(cfg: &StrengthenedConfig) -> Self {
        let extension = if cfg.mechanisms.adaptive_horizon {
            MAX_EXTENSION
        } else {
            0
        };
        if cfg.mechanisms.generalized_leaf {
            Self::generalized(cfg.horizon_turns, extension)
        } else {
            Self::reference(max_turns(cfg.horizon_turns, extension))
        }
    }

    /// The value of a win after `turns` turns.
    pub(crate) fn win(&self, turns: i32) -> f64 {
        1.0 - self.decay * f64::from(self.counted(turns))
    }

    /// The value of a loss after `turns` turns.
    pub(crate) fn loss(&self, turns: i32) -> f64 {
        self.decay * f64::from(self.counted(turns))
    }

    /// The turns a terminal value is shaded by: the generalised mapping counts at most
    /// `max_turns`, so that a finished line longer than any search line (a pre-game simulation
    /// played to the end, P6-H) still outranks every estimate; the reference counts them all.
    const fn counted(&self, turns: i32) -> i32 {
        if !self.reference && turns > self.max_turns {
            self.max_turns
        } else {
            turns
        }
    }

    /// The value of an unfinished line whose evaluation is `p = σ(score / value_scale)`.
    pub(crate) fn estimate(&self, p: f64) -> f64 {
        self.floor + (1.0 - 2.0 * self.floor) * p
    }

    /// The constructor check: over every line length up to `max_turns`, the worst win (the
    /// longest line) is worth more than the best estimate and the best loss less than the worst
    /// one. `σ` saturates at exactly 0.0 and 1.0 in floating point, so those are the extremes.
    ///
    /// # Errors
    /// The configuration's horizon is too long for its constants.
    pub(crate) fn check(&self) -> Result<(), String> {
        let worst_win = self.win(self.max_turns);
        let best_loss = self.loss(self.max_turns);
        if self.floor > 0.0
            && self.floor < 0.5
            && worst_win > self.estimate(1.0)
            && best_loss < self.estimate(0.0)
        {
            return Ok(());
        }
        Err(if self.reference {
            format!(
                "a line of {} turns is too long for the reference leaf constants: a win at the \
                 extended horizon would not outrank every evaluated position \
                 (WIN_DECAY * turns < LEAF_FLOOR); enable generalized_leaf",
                self.max_turns
            )
        } else {
            format!(
                "the generalized leaf constants (floor {}, decay {}) do not order a win after {} \
                 turns above every evaluated position",
                self.floor, self.decay, self.max_turns
            )
        })
    }
}
