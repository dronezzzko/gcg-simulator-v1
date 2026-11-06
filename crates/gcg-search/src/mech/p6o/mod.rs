//! P6-O's mechanisms (docs/dev/PLAN.md §12.2): `portable_math` (W-137, `math.rs`) and
//! `engine_error_leaf` (W-143, W-110).

pub(crate) mod math;

use gcg_engine::EngineError;

use super::LeafCx;
use super::reference::loss_value;

/// The mechanism flags this package implements.
pub(crate) const PROVIDES: [&str; 2] = ["engine_error_leaf", "portable_math"];

/// `engine_error_leaf`: a simulated line that ended in `err` scores as a loss of the searching
/// player at the turn it reached (the reference's loss value, `ai/mcts.py:259-264`), so the
/// search avoids lines the engine cannot finish (for example the W-143 redirect cycle) instead
/// of failing the real game. An unimplemented card is never absorbed: it stays a hard failure
/// (`effects/registry.py:143-147`).
pub(crate) fn error_leaf(cx: &LeafCx<'_>, err: &EngineError) -> Option<f64> {
    match err {
        EngineError::Unimplemented(_) => None,
        _ => Some(loss_value(cx)),
    }
}
