//! P6-H's mechanisms (docs/dev/PLAN.md §12.2, §12.3 S11): the strengthened agent's pre-game
//! choices, on the paired-simulation machinery of `sim.rs`.
//!
//! - `exclude_capped_play_draw` (W-44, `play_draw.rs`): CHOOSE_FIRST from paired simulations in
//!   which unfinished games are excluded from the paired estimate and scored by the evaluation.
//! - `paired_mulligan` (W-45, `mulligan.rs`): REDRAW by paired simulation of the opening turns
//!   on shared worlds, with two paired standard errors and the reference rule as the fallback.
//!
//! With a flag off its decision is the reference's (`reference::pregame`).

pub(crate) mod mulligan;
pub(crate) mod play_draw;
pub(crate) mod sim;
#[cfg(test)]
mod tests;

use gcg_ai::{AgentError, Alternative};
use gcg_engine::{Action, DecisionKind};

use super::{PregameCx, reference};
use crate::MechanismFlags;

/// The mechanism flags this package implements.
pub(crate) const PROVIDES: [&str; 2] = ["exclude_capped_play_draw", "paired_mulligan"];

/// REDRAW and CHOOSE_FIRST with the pre-game mechanisms `flags` enables.
///
/// # Errors
/// As `reference::pregame`, and the errors of the simulations (`sim::simulate`).
pub(crate) fn pregame(
    flags: &MechanismFlags,
    cx: &mut PregameCx<'_, '_>,
) -> Result<(Action, Vec<Alternative>), AgentError> {
    match cx.dec.hdr.kind {
        DecisionKind::Redraw if flags.paired_mulligan => mulligan::choose(cx),
        DecisionKind::ChooseFirst if flags.exclude_capped_play_draw => play_draw::choose(cx),
        _ => reference::pregame(cx),
    }
}
