//! The paired-simulation machinery of the pre-game choices (docs/dev/PLAN.md §12.3 S11): one
//! simulated line per `(choice, seed)`, played from a world determinised from the observer's
//! information-set state by the strengthened rollout policy (`Mechanisms::rollout_choice` of the
//! configuration, for both players) and scored by its leaf value (`Mechanisms::leaf_value`).
//!
//! It is the reference's play/draw simulation (`_result`, `ai/setup.py:114-120`) made general:
//! every choice is played on the same determinisations (the same seed drives the world and the
//! rollout generator: common random numbers), the lines run as one fork-join on the work pool with
//! the results in input order (all lines of the first choice, then the second's), and an optional
//! stop turn ends a line before the end of the game. With the reference's rollout settings
//! (temperature 0.5, 3,000 decisions, no stop turn) and the reference's mechanisms, a line makes
//! the moves of `ai/setup.py:117` exactly.

use std::sync::Arc;

use gcg_ai::{AgentError, Density, Scratch, apply_unchecked, burst_density};
use gcg_core::pool::Cancel;
use gcg_core::pymath::py_sum;
use gcg_core::rng::SplitMix64;
use gcg_engine::observe::determinize;
use gcg_engine::{Action, GameState};

use super::super::p6o::math::Math;
use super::super::{Composed, LeafCx, Mechanisms, PregameCx, RolloutCx, math_of};
use crate::StrengthenedConfig;

/// `SIGNIFICANCE` (`ai/setup.py:27`): a choice is preferred when it is better by more than this
/// many standard errors of the paired difference.
pub(crate) const SIGNIFICANCE: f64 = 2.0;

/// How a simulated line ended.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Ending {
    /// The game ended: the winner's seat, or `-1` when both players lost (`engine/game.py`).
    Finished(i32),
    /// The line reached its stop turn with the game going on.
    Stopped,
    /// The line made its most decisions, or found no decision, with the game unfinished.
    Capped,
    /// The line ended in an engine error that `engine_error_leaf` scores.
    Failed,
}

/// One simulated line.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) struct Line {
    pub(crate) ending: Ending,
    /// The value of the final state for the observer: `Mechanisms::leaf_value`, or
    /// `Mechanisms::error_leaf` for a failed line.
    pub(crate) value: f64,
    /// Engine applies of the line: the choice, every rollout move and the rollout policy's
    /// look-aheads (P6-F's `applies_aux`; PLAN §12.3: simulations count as applies).
    pub(crate) applies: u32,
}

/// The rollout settings of a set of lines.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) struct Rollout {
    /// The rollout policy's temperature (`StrengthenedConfig::rollout_temperature` of the lines).
    pub(crate) temperature: f64,
    /// The turn at which a line stops (`st.turn >= stop`), or `None` to play to the end.
    pub(crate) stop_turn: Option<i32>,
    /// The most rollout decisions of a line (`max_decisions` of `ai/playout.py:29-41`).
    pub(crate) max_decisions: u32,
}

/// What every line of one decision shares.
struct Shared {
    /// The observer's configuration with the rollout temperature of the lines.
    cfg: StrengthenedConfig,
    player: u8,
    rollout: Rollout,
    /// `burst_density` of the information-set state, as the search's leaves use it.
    density: Density,
    root_turn: i32,
    cancel: Cancel,
}

/// One line of the fork-join: `(state clone, choice, seed)` to the line.
type LineJob = Arc<
    dyn Fn(usize, (GameState, Action, u64), &mut Scratch) -> Result<Line, AgentError> + Send + Sync,
>;

/// The lines of every choice in `choices` on every seed in `seeds`, in that order (choice-major),
/// on the work pool of `cx.agent`.
///
/// # Errors
/// [`AgentError::Interrupted`] when the SIGINT flag is set before a line starts; otherwise the
/// first error in input order of a world, of a choice's apply, or of a rollout the configuration
/// does not score (`engine_error_leaf` off).
pub(crate) fn simulate(
    cx: &mut PregameCx<'_, '_>,
    choices: &[Action],
    seeds: &[u64],
    rollout: Rollout,
) -> Result<Vec<Line>, AgentError> {
    if cx.agent.cancel.is_cancelled() {
        return Err(AgentError::Interrupted);
    }
    let mut cfg = cx.cfg.clone();
    cfg.rollout_temperature = rollout.temperature;
    let shared = Arc::new(Shared {
        cfg,
        player: cx.player,
        rollout,
        density: burst_density(cx.st, cx.player),
        root_turn: cx.st.turn(),
        cancel: cx.agent.cancel.clone(),
    });
    let inputs: Vec<(GameState, Action, u64)> = choices
        .iter()
        .flat_map(|&choice| seeds.iter().map(move |&seed| (choice, seed)))
        .map(|(choice, seed)| (cx.st.clone(), choice, seed))
        .collect();
    let job: LineJob = Arc::new(move |_, (info, choice, seed), _| {
        if shared.cancel.is_cancelled() {
            return Err(AgentError::Interrupted);
        }
        line(&shared, &info, choice, seed)
    });
    let lines: Vec<Line> = cx
        .agent
        .par
        .fork_join(inputs, job)
        .into_iter()
        .collect::<Result<_, _>>()?;
    cx.applies += lines.iter().map(|l| u64::from(l.applies)).sum::<u64>();
    Ok(lines)
}

/// One line: the world `determinize(info, player, seed)`, `apply(choice, check=True)`, then the
/// rollout policy with `SplitMix64(seed)` until the game ends, the stop turn is reached, the
/// decision cap is hit or no decision is pending (`ai/setup.py:114-120`, `ai/playout.py:29-41`).
fn line(sh: &Shared, info: &GameState, choice: Action, seed: u64) -> Result<Line, AgentError> {
    let mech = Composed::new(&sh.cfg);
    let math = math_of(&sh.cfg);
    let knobs = sh.cfg.knobs();
    let mut s = determinize(info, sh.player, seed)?;
    gcg_engine::apply(&mut s, choice, true)?;
    let mut rng = SplitMix64::new(seed);
    let mut decisions: u32 = 0;
    let ending = loop {
        if let Some(w) = s.winner() {
            break Ending::Finished(i32::from(w));
        }
        if sh.rollout.stop_turn.is_some_and(|t| s.turn() >= t) {
            break Ending::Stopped;
        }
        if decisions >= sh.rollout.max_decisions || s.pending().is_none() {
            break Ending::Capped;
        }
        let step = mech
            .rollout_choice(&mut RolloutCx {
                s: &s,
                rng: &mut rng,
                player: sh.player,
                cfg: &sh.cfg,
                knobs,
                math,
            })
            .and_then(|a| apply_unchecked(&mut s, a));
        decisions += 1;
        if let Err(e) = step {
            let leaf = leaf_cx(sh, &s, decisions, false, math);
            let value = mech.error_leaf(&leaf, &e).ok_or(e)?;
            return Ok(Line {
                ending: Ending::Failed,
                value,
                applies: line_applies(decisions, &mech),
            });
        }
    };
    let mut leaf = leaf_cx(sh, &s, decisions, ending == Ending::Capped, math);
    let value = mech.leaf_value(&mut leaf)?;
    Ok(Line {
        ending,
        value,
        applies: line_applies(decisions, &mech),
    })
}

/// The engine applies of a line of `decisions` rollout moves: the choice, the moves and the
/// policy's auxiliary applies.
fn line_applies(decisions: u32, mech: &Composed<'_>) -> u32 {
    let aux = u32::try_from(mech.aux_applies()).unwrap_or(u32::MAX);
    decisions.saturating_add(1).saturating_add(aux)
}

fn leaf_cx<'s>(
    sh: &'s Shared,
    s: &'s GameState,
    decisions: u32,
    capped: bool,
    math: Math,
) -> LeafCx<'s> {
    LeafCx {
        s,
        player: sh.player,
        root_turn: sh.root_turn,
        horizon: sh.rollout.stop_turn.unwrap_or(i32::MAX),
        decisions,
        capped,
        cfg: &sh.cfg,
        weights: &sh.cfg.weights,
        density: sh.density,
        math,
    }
}

/// The mean and the standard error of paired differences (`ai/setup.py:105-109`: `sum` in order,
/// the `n - 1` variance, `sqrt(var / n)`), or `None` for fewer than two pairs.
pub(crate) fn paired(diffs: &[f64], math: Math) -> Option<(f64, f64)> {
    if diffs.len() < 2 {
        return None;
    }
    let n = count(diffs.len());
    let mean = py_sum(diffs.iter().copied()) / n;
    let var = py_sum(diffs.iter().map(|&x| math.square(x - mean))) / (n - 1.0);
    Some((mean, math.sqrt(var / n)))
}

/// A count as a float (exact for every count in scope).
pub(crate) fn count(n: usize) -> f64 {
    f64::from(u32::try_from(n).expect("simulation counts fit in u32"))
}
