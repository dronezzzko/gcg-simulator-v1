//! `paired_mulligan` (W-45; docs/port/PLAN.md §12.3 S11): the mulligan by paired simulation of
//! the opening turns on shared worlds.
//!
//! The reference (`mulligan_choice`, `ai/setup.py:63-83`) redraws when the hand's
//! `opening_score` (a greedy curve of Units and Pilots over four turns, blind to Commands, Bases,
//! effects and the matchup) is below 90% of the average of fresh five-card samples. Here, on the
//! play/draw machinery (`sim.rs`):
//!
//! 1. the reference rule runs first with the decision's generator, exactly as the reference
//!    (its draws and its choice are the fallback);
//! 2. `mulligan_samples` pair seeds are drawn next; on each seed's world, determinised from the
//!    observer's information set, KEEP and REDRAW are both played (the redrawn hand comes from
//!    the world's own deck order, rule 6-2-1-6-1) by the strengthened rollout policy for both
//!    players until each player has had [`OPENING_TURNS`] turns, and the final state is scored
//!    by the search's leaf value (`Mechanisms::leaf_value`; a game that ended scores as a won or
//!    lost leaf);
//! 3. REDRAW is chosen when the paired difference `redraw - keep` is above two paired standard
//!    errors, KEEP when it is below minus two, and the reference rule's choice otherwise.
//!
//! The alternatives carry the simulated mean values of KEEP and REDRAW with the number of pairs.

use gcg_ai::{AgentError, Alternative, mulligan_choice};
use gcg_core::pymath::py_sum;
use gcg_engine::{Action, ActionKind};

use super::super::{PregameCx, math_of};
use super::play_draw::MAX_GAME_DECISIONS;
use super::sim::{Line, Rollout, SIGNIFICANCE, count, paired, simulate};

/// `OPENING_TURNS` (`ai/setup.py:22`): the turns of each player a mulligan line plays.
pub(crate) const OPENING_TURNS: i32 = 4;

/// The rollout settings of the mulligan lines from a REDRAW decision at turn `turn` (the setup,
/// turn 0): stop at the start of turn `turn + 1 + 2 * OPENING_TURNS`, when each player has had
/// [`OPENING_TURNS`] turns; the search's rollout temperature.
pub(crate) const fn mulligan_rollout(turn: i32, temperature: f64) -> Rollout {
    Rollout {
        temperature,
        stop_turn: Some(turn + 1 + 2 * OPENING_TURNS),
        max_decisions: MAX_GAME_DECISIONS,
    }
}

/// The REDRAW choice of `cx` with `paired_mulligan`.
///
/// # Errors
/// As [`simulate`].
pub(crate) fn choose(cx: &mut PregameCx<'_, '_>) -> Result<(Action, Vec<Alternative>), AgentError> {
    let (fallback, _) = mulligan_choice(cx.st, cx.player, cx.cfg.mulligan_samples, cx.rng);
    let keep = Action::of(ActionKind::Keep);
    let redraw = Action::of(ActionKind::Redraw);
    let pairs = cx.cfg.mulligan_samples;
    let seeds: Vec<u64> = (0..pairs).map(|_| cx.rng.next_u64()).collect();
    let rollout = mulligan_rollout(cx.st.turn(), cx.cfg.rollout_temperature);
    let lines = simulate(cx, &[keep, redraw], &seeds, rollout)?;
    let (kept, redrawn) = lines.split_at(seeds.len());
    let diffs: Vec<f64> = kept
        .iter()
        .zip(redrawn)
        .map(|(k, r)| r.value - k.value)
        .collect();
    let n = count(seeds.len().max(1));
    let mean_of = |ls: &[Line]| py_sum(ls.iter().map(|l| l.value)) / n;
    let stats = vec![
        Alternative {
            action: keep,
            value: mean_of(kept),
            visits: pairs,
        },
        Alternative {
            action: redraw,
            value: mean_of(redrawn),
            visits: pairs,
        },
    ];
    let chosen = match paired(&diffs, math_of(cx.cfg)) {
        Some((mean, se)) if mean > 0.0 && mean > SIGNIFICANCE * se => redraw,
        Some((mean, se)) if mean < 0.0 && -mean > SIGNIFICANCE * se => keep,
        _ => fallback,
    };
    Ok((chosen, stats))
}
