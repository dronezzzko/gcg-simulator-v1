//! ORDER_TRIGGER scoring (`order_trigger_scorer`, W-29). The reference has no scorer for it
//! (`ai/policy.py:104-108`): every order scores 0, so deterministic rollouts take the first and
//! softmax rollouts are uniform. Here each order is resolved on a clone and scored by the
//! evaluation when there are at most [`LOOKAHEAD_ORDERS`]; with more, a heuristic puts its
//! controller's beneficial triggers first, draws before discards, and damage before Repair.

use gcg_effects::{Node, P, ProgramId, Registry};
use gcg_engine::{Action, ActionKind, DecisionView, EngineError, GameState, TriggerInst};

use super::bursts;
use super::lookahead::Until;
use super::scorer::Scoring;

/// The most orders scored by look-ahead.
pub(crate) const LOOKAHEAD_ORDERS: usize = 6;

/// The scores of an ORDER_TRIGGER decision.
///
/// # Errors
/// The engine's errors inside a look-ahead.
pub(crate) fn scores(
    sc: &Scoring<'_>,
    st: &GameState,
    dec: &DecisionView<'_>,
    options: &[Action],
) -> Result<Vec<f64>, EngineError> {
    if options.len() <= LOOKAHEAD_ORDERS {
        return sc.by_lookahead(st, dec, options, Until::Effect);
    }
    Ok(heuristic(st, dec.hdr.player, options))
}

/// The heuristic order: per trigger, +1 when it is its controller's and harms none of its
/// controller's cards, +2 when it draws, -2 when its controller discards, +1 when it deals
/// damage and -1 when it recovers HP (so damage resolves before a Repair heals it).
#[must_use]
pub(crate) fn heuristic(st: &GameState, player: u8, options: &[Action]) -> Vec<f64> {
    let batch: &[TriggerInst] = st.batches().last().unwrap_or_default();
    options
        .iter()
        .map(|o| {
            let t = (o.kind == ActionKind::Order)
                .then(|| usize::try_from(o.a).ok())
                .flatten()
                .and_then(|i| batch.get(i));
            t.map_or(0.0, |t| trigger_score(st, player, t))
        })
        .collect()
}

fn trigger_score(st: &GameState, player: u8, t: &TriggerInst) -> f64 {
    program_score(st.registry(), t.program, t.controller == player)
}

/// The heuristic score of a trigger resolving `program`, `own` when the decider controls it.
pub(crate) fn program_score(reg: &Registry, program: ProgramId, own: bool) -> f64 {
    let instrs = &reg.program(program).instrs;
    let mut s = 0.0;
    if own && !bursts::review(reg, instrs, 0..instrs.len()).self_harm {
        s += 1.0;
    }
    for &ins in instrs.iter() {
        match reg.node(ins) {
            Node::Draw(d) if d.player == P::You => s += 2.0,
            Node::Discard(d) if d.player == P::You => s -= 2.0,
            Node::Damage(_) => s += 1.0,
            Node::Recover(_) => s -= 1.0,
            _ => {}
        }
    }
    s
}
