//! The target critic (`target_rollouts`, W-14; docs/dev/PLAN.md §12.3 S6).
//!
//! At an effect-target decision (`is_target_decision`, `ai/mcts.py:165-169`) the reference's
//! iterations give each target one noisy line per visit, and the calibrated leaf value squeezes
//! the log-odds of a clearly lost or won position into a few hundredths near `LEAF_FLOOR` or its
//! mirror, so two targets whose difference is real but small are a coin flip (W-14: the
//! <Repair> ping puzzle, solved by the reference about a third of the time).
//!
//! The critic judges each target by what it *leaves*: in each of the first K search worlds the
//! root move is played, the effect is resolved to its end with the deterministic policy (as the
//! no-effect check resolves a move, `ai/noop.py:67-74`), and then both players idle to the
//! search horizon (ending the main phase, passing, not blocking, declining and choosing nothing
//! wherever the decision allows it; the deterministic policy elsewhere), so the clock's own
//! consequences apply: end-of-turn healing (<Repair>), the end of "during this turn" effects,
//! the opponent's untapping. The line is scored by the evaluation's log-odds (`score`,
//! `ai/evaluation.py:310-317`), where the targets' difference is not squeezed. The K worlds
//! pair the targets (common random numbers). A move the effect allows to choose nothing (DONE)
//! is a root move, so "no target" is the baseline of every target.
//!
//! The critic speaks only for what the clock adds. Each line is also scored right after the
//! effect resolved (the *immediate* log-odds). The critic's *best* move is the move with the
//! highest mean at the horizon whose paired horizon difference with every other move is more
//! than [`CRITIC_Z`] paired standard errors (a difference that is the same in every world
//! counts) while the immediate evaluation does not already favour it (mean immediate difference
//! at most zero): a preference that shows at once is the search's own business (its priors and
//! lines see it), and a critic that also decided those choices by its idle lines played worse
//! than the search (measured, docs/dev/eval/weakness-status/P6-E.md, W-14). The best seeds the
//! root prior and breaks the search's ties: from the middle of the budget on, a root selection
//! of another move goes to the critic's best unless the search's own root values show that move
//! better by more than [`SEARCH_Z`] standard errors (`Shared` records the root values for it).
//! A decision with a critic's best does not extend its budget for a close call
//! (`target_extension`), whose job the critic does. Every apply of the critic is counted. When
//! the targets leave the same position, or differ at once, the critic is silent and the search
//! is the reference's.

use gcg_ai::{Density, MoveKey, PolicyKnobs, apply_unchecked, score};
use gcg_core::pymath::py_sum;
use gcg_engine::{Action, ActionKind, EngineError, GameState};

use super::noop::{MAX_DECISIONS, resolving};
use super::worlds::Worlds;
use crate::mech::p6o::math::Math;
use crate::mech::{Mechanisms, RootCx};

/// The paired standard errors by which the critic's best must beat every other move.
pub(crate) const CRITIC_Z: f64 = 2.0;
/// The standard errors by which the search's own values must favour another move over the
/// critic's best to keep the reference's selection.
pub(crate) const SEARCH_Z: f64 = 2.0;
/// The log-odds of a finished line: a win, or minus it for a loss.
const WIN_LOGIT: f64 = 20.0;
/// The idle choices, in order of preference: end the main phase, pass, do not block, decline,
/// choose nothing.
const IDLE: [ActionKind; 5] = [
    ActionKind::EndMain,
    ActionKind::Pass,
    ActionKind::NoBlock,
    ActionKind::No,
    ActionKind::Done,
];

/// The critic's estimate of the root moves of one target decision.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Critic {
    /// The root move keys, in root order.
    pub(crate) keys: Vec<MoveKey>,
    /// The mean log-odds of each root move.
    pub(crate) means: Vec<f64>,
    /// The index of the critic's best move, when it is significantly best.
    pub(crate) best: Option<usize>,
}

/// The idle move at `s`: the first idle kind its decision offers, otherwise the deterministic
/// policy.
fn idle_choice(s: &GameState, knobs: &PolicyKnobs) -> Result<Action, EngineError> {
    let Some(dec) = s.pending() else {
        return Err(EngineError::Assertion(String::new()));
    };
    for kind in IDLE {
        if let Some(&a) = dec.options.iter().find(|a| a.kind == kind) {
            return Ok(a);
        }
    }
    gcg_ai::choose(s, None, 0.0, knobs)
}

/// The log-odds of `s` for the searching player: the evaluation's, or [`WIN_LOGIT`] (a win),
/// its negation (a loss) or 0 (a draw) for a finished game.
fn log_odds(cx: &RootCx<'_>, s: &GameState, density: Density) -> Result<f64, EngineError> {
    Ok(match s.winner() {
        Some(w) if i32::from(w) == i32::from(cx.player) => WIN_LOGIT,
        Some(w) if i32::from(w) == 1 - i32::from(cx.player) => -WIN_LOGIT,
        Some(_) => 0.0,
        None => score(s, cx.player, &cx.cfg.weights, density)?,
    })
}

/// One critic line: `action` in `world`, the effect resolved with the deterministic policy,
/// then idle play to the horizon; its log-odds for the searching player right after the
/// resolution and at the horizon.
fn line(
    cx: &RootCx<'_>,
    world: &GameState,
    action: Action,
    density: Density,
    applies: &mut u64,
) -> Result<(f64, f64), EngineError> {
    let mut s = world.clone();
    apply_unchecked(&mut s, action)?;
    *applies += 1;
    let mut n = 0usize;
    while resolving(&s) && n < MAX_DECISIONS {
        let a = gcg_ai::choose(&s, None, 0.0, &cx.knobs)?;
        apply_unchecked(&mut s, a)?;
        *applies += 1;
        n += 1;
    }
    let immediate = log_odds(cx, &s, density)?;
    let horizon = cx.st.turn() + cx.cfg.horizon_turns;
    while s.winner().is_none() && s.turn() < horizon && n < MAX_DECISIONS && s.pending().is_some() {
        let a = idle_choice(&s, &cx.knobs)?;
        apply_unchecked(&mut s, a)?;
        *applies += 1;
        n += 1;
    }
    Ok((immediate, log_odds(cx, &s, density)?))
}

/// Whether the paired differences `d` have a mean more than `z` standard errors above zero.
fn significant(d: &[f64], z: f64, math: Math) -> bool {
    let n = d.len();
    if n == 0 {
        return false;
    }
    let n_f = f64::from(u32::try_from(n).expect("few worlds"));
    let mean = py_sum(d.iter().copied()) / n_f;
    if n == 1 {
        return mean > 0.0;
    }
    let var = py_sum(d.iter().map(|&x| math.square(x - mean))) / (n_f - 1.0);
    mean > z * math.sqrt(var / n_f)
}

/// The critic over `moves` in `worlds`; returns it and the applies it took.
///
/// # Errors
/// The engine's error from a world, an apply, the policy or the evaluation.
pub(crate) fn run<M: Mechanisms + ?Sized>(
    cx: &RootCx<'_>,
    moves: &[(MoveKey, Action)],
    worlds: &mut Worlds<'_, M>,
    math: Math,
) -> Result<(Critic, u64), EngineError> {
    let density = gcg_ai::burst_density(cx.st, cx.player);
    let mut applies = 0u64;
    let mut samples: Vec<Vec<f64>> = vec![Vec::new(); moves.len()];
    let mut immediate: Vec<Vec<f64>> = vec![Vec::new(); moves.len()];
    for k in 0..worlds.len() {
        let world = worlds.get(k)?;
        for (i, &(_, action)) in moves.iter().enumerate() {
            let (now, later) = line(cx, world, action, density, &mut applies)?;
            immediate[i].push(now);
            samples[i].push(later);
        }
    }
    let paired = |x: &[Vec<f64>], i: usize, j: usize| -> Vec<f64> {
        x[i].iter().zip(&x[j]).map(|(a, b)| a - b).collect()
    };
    let means: Vec<f64> = samples
        .iter()
        .map(|s| py_sum(s.iter().copied()) / f64::from(u32::try_from(s.len()).expect("few")))
        .collect();
    let mut top = 0;
    for (i, &m) in means.iter().enumerate() {
        if m > means[top] {
            top = i;
        }
    }
    let best = (0..moves.len())
        .filter(|&i| i != top)
        .all(|i| {
            let now = paired(&immediate, top, i);
            significant(&paired(&samples, top, i), CRITIC_Z, math)
                && py_sum(now.iter().copied()) <= 0.0
        })
        .then_some(top);
    Ok((
        Critic {
            keys: moves.iter().map(|&(k, _)| k).collect(),
            means,
            best,
        },
        applies,
    ))
}

/// The root values the search has backed up, per root child: count, sum, sum of squares.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Moments {
    pub(crate) n: u32,
    pub(crate) sum: f64,
    pub(crate) sum_sq: f64,
}

impl Moments {
    pub(crate) fn add(&mut self, v: f64) {
        self.n += 1;
        self.sum += v;
        self.sum_sq += v * v;
    }

    /// The mean and the variance of the mean, with at least two values.
    fn mean_var(self) -> Option<(f64, f64)> {
        if self.n < 2 {
            return None;
        }
        let n = f64::from(self.n);
        let mean = self.sum / n;
        let var = ((self.sum_sq - n * mean * mean) / (n - 1.0)).abs();
        Some((mean, var / n))
    }

    /// Whether these values beat `other`'s by more than `z` standard errors.
    pub(crate) fn beats(self, other: Self, z: f64, math: Math) -> bool {
        match (self.mean_var(), other.mean_var()) {
            (Some((a, va)), Some((b, vb))) => a - b > z * math.sqrt(va + vb),
            _ => false,
        }
    }
}
