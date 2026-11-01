//! Direct calls of Python tests into engine internals (`call` records, PLAN §8.2;
//! docs/port/contracts.md, gcg-engine `testkit::engine`): each forwards to the internal
//! function with Python's parameters.

use gcg_data::CardType;
use gcg_effects::{ContinuousId, ProgramId};

use crate::{
    Duration, EngineError, FrameArgs, GameState, LastingArgs, LastingOpts, PushFrame, Uid,
};

/// `interp.push_frame(st, program_id, *, controller, host, card_uid=-1, kind, event=(),
/// vars=None)` (`engine/interp.py:61-88`): the new frame's index.
///
/// # Errors
/// An arena overflow of the frame stack.
pub fn push_frame(
    st: &mut GameState,
    program: ProgramId,
    args: FrameArgs<'_>,
) -> Result<usize, EngineError> {
    let f = PushFrame {
        controller: args.controller,
        host: args.host,
        card_uid: args.card_uid,
        kind: args.kind,
        event: args.event,
        vars: args.vars,
    };
    crate::interp::push_frame(st, program, f)
}

/// `game.advance(st)` (`engine/game.py:734-760`).
///
/// # Errors
/// Anything the loop raises.
pub fn advance(st: &mut GameState) -> Result<(), EngineError> {
    crate::game::advance(st)
}

/// `interp.add_lasting(st, effect, controller, source, targets, duration, *, player=-1,
/// uses=0, filters=(), aux=-1)` (`engine/interp.py:1286-1320`): `effect` is the registry's
/// canonical continuous effect (`Registry::cont_of`); a non-empty filter tuple is the
/// registry's filter set (`registry.py:132-138`).
///
/// # Errors
/// [`EngineError::Internal`] for a filter tuple outside the registry's filter sets, or an
/// error of the interpreter.
pub fn add_lasting(
    st: &mut GameState,
    effect: ContinuousId,
    controller: u8,
    source: i32,
    targets: &[(Uid, i32)],
    duration: Duration,
    o: LastingArgs<'_>,
) -> Result<(), EngineError> {
    let reg = st.registry();
    let cont = reg.cont_of(effect);
    let filters = if o.filters.is_empty() {
        None
    } else {
        Some(reg.filters_key(o.filters).ok_or_else(|| {
            EngineError::Internal(
                "the filter tuple is not a filter set of the effect registry".to_owned(),
            )
        })?)
    };
    let opts = LastingOpts {
        player: o.player,
        uses: o.uses,
        filters,
        aux: o.aux,
    };
    crate::interp::add_lasting(st, cont, controller, source, targets, duration, opts)
}

/// `interp.draw(st, p, n, *, by_effect=False)` (`engine/interp.py:500-512`): the cards drawn.
///
/// # Errors
/// An error of the events the draws emit.
pub fn draw(st: &mut GameState, p: u8, n: i64, by_effect: bool) -> Result<i64, EngineError> {
    crate::interp::draw(st, p, n, by_effect)
}

/// `core.card_type(st, uid)` (`engine/core.py:945-946`).
#[must_use]
pub fn card_type(st: &GameState, uid: Uid) -> CardType {
    crate::core::card_type(st, uid)
}

/// `interp.deploy_cards(st, cards, *, rested, by, ex_used=0)` (`engine/interp.py:956-976`).
///
/// # Errors
/// An error of the deployment.
pub fn deploy_cards(
    st: &mut GameState,
    cards: &[Uid],
    rested: bool,
    by: i32,
    ex_used: i32,
) -> Result<(), EngineError> {
    crate::interp::deploy_cards(st, cards, rested, by, ex_used)
}
