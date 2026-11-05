//! Direct calls of Python tests into engine internals (`call` records, PLAN §8.2;
//! docs/port/contracts.md, gcg-engine `testkit::engine`): each forwards to the internal
//! function with Python's parameters. The primitives after [`deploy_cards`] serve the touch
//! catalogue (PLAN §4.4: each primitive's `version` delta against Python) and the
//! `once_used` set tests (T-150).

use gcg_data::{CardType, DefId};
use gcg_effects::{ContRef, ContinuousId, ProgramId};

use crate::{
    DamageOpts, Duration, EngineError, FrameArgs, GameState, HistKind, LastingArgs, LastingOpts,
    MoveOpts, OnceElem, OnceKey, PushFrame, Uid, Zone,
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
    let cont = st.registry().cont_of(effect);
    add_lasting_effect(st, cont, controller, source, targets, duration, o)
}

/// [`add_lasting`] with the effect as a value (`interp.py:1286-1320`): Python passes any
/// `Continuous` object, such as a `StatMod(ap=1)` that a test builds, and `R.cont_key`
/// interns it (PLAN §5.6).
///
/// # Errors
/// As [`add_lasting`].
pub fn add_lasting_effect(
    st: &mut GameState,
    cont: ContRef,
    controller: u8,
    source: i32,
    targets: &[(Uid, i32)],
    duration: Duration,
    o: LastingArgs<'_>,
) -> Result<(), EngineError> {
    let reg = st.registry();
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

/// `st.touch()` (`engine/state.py:529-531`).
pub fn touch(st: &mut GameState) {
    st.touch();
}

/// `core.move(st, uid, dst, *, bottom=False, reveal=False, rested=False)` (`core.py:55-81`).
///
/// # Errors
/// A card missing from its zone list; overflows.
pub fn move_card(
    st: &mut GameState,
    uid: Uid,
    dst: Zone,
    bottom: bool,
    reveal: bool,
    rested: bool,
) -> Result<(), EngineError> {
    let o = MoveOpts {
        bottom,
        reveal,
        rested,
    };
    crate::core::move_card(st, uid, dst, o)
}

/// `core.remove_from_game(st, uid)` (`core.py:109-116`).
///
/// # Errors
/// A card missing from its zone list; overflows.
pub fn remove_from_game(st: &mut GameState, uid: Uid) -> Result<(), EngineError> {
    crate::core::remove_from_game(st, uid)
}

/// `core.new_card(st, def_id, owner, zone, *, rested=False)` (`core.py:119-131`): the new uid.
///
/// # Errors
/// Overflows past `u16::MAX` cards or zone-arena entries.
pub fn new_card(
    st: &mut GameState,
    def: DefId,
    owner: u8,
    zone: Zone,
    rested: bool,
) -> Result<Uid, EngineError> {
    crate::core::new_card(st, def, owner, zone, rested)
}

/// `core.shuffle_deck(st, player)` (`core.py:134-139`).
pub fn shuffle_deck(st: &mut GameState, p: u8) {
    crate::core::shuffle_deck(st, p);
}

/// `core.record(st, kind, player, by=-1, uid=-1, source=-1)` (`core.py:142-151`).
pub fn record(st: &mut GameState, kind: HistKind, player: u8, by: i8, uid: i32, source: i32) {
    crate::core::record(st, kind, player, by, uid, source);
}

/// `core.apply_damage(st, uid, amount, *, source, battle, by)` (`core.py:602-633`): the damage
/// applied.
///
/// # Errors
/// Overflows and the errors of the events it emits.
pub fn apply_damage(
    st: &mut GameState,
    uid: Uid,
    amount: i64,
    source: i32,
    battle: bool,
    by: i32,
) -> Result<i64, EngineError> {
    let d = DamageOpts { source, battle, by };
    crate::core::apply_damage(st, uid, amount, d)
}

/// `interp.pay_generic(st, p, cost, ex_used=-1)` (`interp.py:1441-1469`): whether the cost was
/// paid.
///
/// # Errors
/// The errors of removing EX Resources and of the events that emits.
pub fn pay_generic(
    st: &mut GameState,
    p: u8,
    cost: i64,
    ex_used: i32,
) -> Result<bool, EngineError> {
    crate::interp::pay_generic(st, p, cost, ex_used)
}

/// `st.once_used.add(key)` (`engine/state.py:497`) for a key of plain ints: nothing when an
/// equal key is present, and never a touch.
///
/// # Errors
/// [`EngineError::Capacity`] for a key longer than the engine's once keys.
pub fn once_add(st: &mut GameState, key: &[i32]) -> Result<(), EngineError> {
    let elems: Vec<OnceElem> = key.iter().map(|&x| OnceElem::Int(x)).collect();
    st.once_add(OnceKey::from_elems(&elems)?);
    Ok(())
}
