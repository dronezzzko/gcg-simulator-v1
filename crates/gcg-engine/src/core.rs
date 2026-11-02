//! Layer L3 of the engine, part 1 (docs/port/PLAN.md §5.5, §10.3.4): a function-for-function
//! port of `engine/core.py` — zone movement with new-card semantics, history, events and
//! trigger collection, last-known information, damage, destruction and rules management.
//!
//! Every function keeps Python's statement order, so every `touch()` (Inv 06a §4), every
//! `next_group()` (Inv 06a hazard 13), every `derived()` call and every iteration order is
//! Python's. Integers that Python keeps unbounded are checked: leaving the Rust type is
//! [`EngineError::Overflow`] (PLAN §4.1 item 6). Python list indexing with a negative index
//! (`st.cards[-1]`, Inv 06a hazard 3) is reproduced by [`card_at`].

#![allow(
    dead_code,
    reason = "called by layers L4 (interp) and L5 (game, procedures, testkit), which land after L3 (docs/port/contracts.md)"
)]

use gcg_data::{CardType, DefId};
use gcg_effects::dsl::{self, Node};
use gcg_effects::{
    AbilityEntry, AbilityId, CondId, ContView, DamageKind, Ev, KeywordProgram, Kw, P, Registry,
    RuleKind, RuleModId, SelId, Side, TriggerId, ValueRef,
};

use gcg_core::small::InlineVec;

use crate::view;
use crate::{
    AbilityKey, BOTH_KNOW, CardInstance, Ctx, CtxVars, DamageOpts, Derived, EVENT_CAPACITY,
    EndReason, EngineError, EvKey, EventPayload, GameState, HistKind, HistoryEvent, Lki, LkiEntry,
    MoveOpts, NO_ARG, OnceElem, OnceKey, PREVENT_ONCE_TAG, Phase, REDUCE_ONCE_TAG, RuleInst,
    RuleKey, Span, Step, TOKEN_ZONES, TriggerInst, Uid, UidBuf, Zone,
};

// ---------------------------------------------------------------------------------------------
// Python integer and list semantics

/// `x` as an `i32` slot (event payloads, card fields): [`EngineError::Overflow`] otherwise.
pub(crate) fn to_i32(x: i64, what: &'static str) -> Result<i32, EngineError> {
    i32::try_from(x).map_err(|_| EngineError::Overflow { what })
}

/// `x + 1` for the `i32` counters Python keeps unbounded (`zone_seq`).
fn incr(x: i32, what: &'static str) -> Result<i32, EngineError> {
    x.checked_add(1).ok_or(EngineError::Overflow { what })
}

/// The index of `st.cards[i]` with Python's list semantics: a negative `i` counts from the
/// end (Inv 06a hazard 3: `_delayed_triggers` reads `st.cards[-1]` for a host-less delayed
/// trigger), and an index outside the list is Python's `IndexError`.
pub(crate) fn py_index(len: usize, i: i32) -> Result<usize, EngineError> {
    let at = if i < 0 {
        i64::try_from(len).unwrap_or(i64::MAX) + i64::from(i)
    } else {
        i64::from(i)
    };
    usize::try_from(at)
        .ok()
        .filter(|&k| k < len)
        .ok_or_else(|| EngineError::Index("list index out of range".to_owned()))
}

/// `st.cards[i]` with Python's list semantics ([`py_index`]).
pub(crate) fn card_at(st: &GameState, i: i32) -> Result<&CardInstance, EngineError> {
    Ok(&st.cards[py_index(st.cards.len(), i)?])
}

/// The uid that `st.cards[i]` addresses ([`py_index`]), for Python call sites that pass an
/// `int` uid on to a function taking a card.
pub(crate) fn uid_at(st: &GameState, i: i32) -> Result<Uid, EngineError> {
    let k = py_index(st.cards.len(), i)?;
    Uid::try_from(k).map_err(|_| EngineError::Overflow { what: "uid" })
}

/// `by` of `core.record` (`core.py:142-151`): a player index or -1 at every call site.
pub(crate) fn by_i8(by: i32) -> Result<i8, EngineError> {
    i8::try_from(by).map_err(|_| EngineError::Overflow {
        what: "history `by`",
    })
}

// ---------------------------------------------------------------------------------------------
// Typed reads of the effect IR (the node kinds are validated when the IR loads, PLAN §5.1)

fn kind_error(what: &str, id: u32) -> EngineError {
    EngineError::Internal(format!("effect IR node {id} is not a {what}"))
}

/// `a.ability` of a triggered ability entry; Python asserts `isinstance(ab, d.Triggered)`
/// (`core.py:258`, `:345`).
pub(crate) fn triggered<'r>(
    reg: &'r Registry,
    a: &AbilityEntry,
) -> Result<&'r dsl::Triggered, EngineError> {
    match reg.node(a.ability.node()) {
        Node::Triggered(t) => Ok(t),
        _ => Err(EngineError::Assertion(format!(
            "ability {} is not Triggered",
            a.aid.0
        ))),
    }
}

/// The `Trigger` node of a handle.
pub(crate) fn trigger_node(reg: &Registry, id: TriggerId) -> Result<&dsl::Trigger, EngineError> {
    match reg.node(id.node()) {
        Node::Trigger(t) => Ok(t),
        _ => Err(kind_error("Trigger", id.node().0)),
    }
}

/// The `RuleMod` node of a handle.
pub(crate) fn rule_mod(reg: &Registry, id: RuleModId) -> Result<&dsl::RuleMod, EngineError> {
    match reg.node(id.node()) {
        Node::RuleMod(r) => Ok(r),
        _ => Err(kind_error("RuleMod", id.node().0)),
    }
}

fn sel_node(reg: &Registry, id: SelId) -> Result<&dsl::Sel, EngineError> {
    match reg.node(id.node()) {
        Node::Sel(s) => Ok(s),
        _ => Err(kind_error("Sel", id.node().0)),
    }
}

/// `cond is d.TRUE` (`core.py:349`): the identity check on the singleton `And(())`. The IR
/// hash-conses nodes by equality, so every `And(())` is this node; Python evaluates an equal
/// but distinct `And(())` to `True` with no side effect, so the two agree (Inv 06a hazard 16).
pub(crate) fn is_true(reg: &Registry, c: CondId) -> bool {
    matches!(reg.node(c.node()), Node::And(a) if a.conds.is_empty())
}

/// `isinstance(v, int)` of a `Value` field read as its literal, else `default`
/// (`core.py:518`, `:678`).
const fn int_or(v: ValueRef, default: i64) -> i64 {
    match v {
        ValueRef::Lit(x) => x,
        ValueRef::Node(_) => default,
    }
}

// ---------------------------------------------------------------------------------------------
// zone movement (core.py:38-139)

/// `_remove_from_zone(st, c)` (`core.py:38-44`): nothing for a card OUTSIDE, or RESOLVING but
/// not in its owner's RESOLVING list; otherwise `list.remove(uid)` (first occurrence;
/// Python's `ValueError` when absent).
fn remove_from_zone(st: &mut GameState, uid: Uid) -> Result<(), EngineError> {
    let c = st.cards[usize::from(uid)];
    if c.zone == Zone::Outside
        || (c.zone == Zone::Resolving && !st.zones.get(c.owner, Zone::Resolving).contains(&uid))
    {
        return Ok(());
    }
    st.zones.remove(c.owner, c.zone, uid)
}

/// `_knowledge_on_enter(c, src, dst, reveal)` (`core.py:47-52`).
fn knowledge_on_enter(c: &mut CardInstance, src: Zone, dst: Zone, reveal: bool) {
    if dst.is_public() || reveal {
        c.known = BOTH_KNOW;
    } else if dst == Zone::Hand {
        c.known = if src.is_public() {
            BOTH_KNOW
        } else {
            c.known | (1 << c.owner)
        };
    }
    // deck/shield/resource deck keep whatever knowledge players already had of this card
}

/// `move(st, uid, dst, *, bottom=False, reveal=False, rested=False)` (`core.py:55-81`): a card
/// moves to `dst` of its owner with new-card semantics (rule 4-1-5). A paired Pilot follows
/// its Unit and moves first, always active (rule 3-3-6); a Pilot leaving PAIRED unpairs both.
/// One `touch` per call, after both moves (Inv 06a §3.1).
///
/// # Errors
/// Python's `ValueError` when a listed card is missing from its zone list; overflows.
pub(crate) fn move_card(
    st: &mut GameState,
    uid: Uid,
    dst: Zone,
    o: MoveOpts,
) -> Result<(), EngineError> {
    let c = st.cards[usize::from(uid)];
    let src = c.zone;
    if c.pair >= 0 {
        let other = uid_at(st, c.pair)?;
        if src == Zone::Battle {
            st.cards[usize::from(other)].pair = NO_ARG;
            st.cards[usize::from(uid)].pair = NO_ARG;
            move_single(st, other, dst, o.bottom, o.reveal, false)?;
        } else if src == Zone::Paired {
            st.cards[usize::from(other)].pair = NO_ARG;
            st.cards[usize::from(uid)].pair = NO_ARG;
        }
    }
    move_single(st, uid, dst, o.bottom, o.reveal, o.rested)?;
    st.touch(); // core.py:81
    Ok(())
}

/// `_move_single(st, c, dst, *, bottom, reveal, rested)` (`core.py:84-106`): a Unit variant
/// reverts to its printed card outside BATTLE; a token arriving outside the token zones
/// ceases to exist (rule 5-17-2-5) after its fields were updated; DECK, SHIELD and
/// RESOURCE_DECK receive on top unless `bottom`, every other zone appends.
fn move_single(
    st: &mut GameState,
    uid: Uid,
    dst: Zone,
    bottom: bool,
    reveal: bool,
    rested: bool,
) -> Result<(), EngineError> {
    let i = usize::from(uid);
    let src = st.cards[i].zone;
    remove_from_zone(st, uid)?;
    let db = st.db();
    let turn = st.turn;
    let c = &mut st.cards[i];
    if dst != Zone::Battle {
        c.def_id = db.base_def_id(c.def_id); // a Unit variant is the printed card again
    }
    c.zone_seq = incr(c.zone_seq, "zone_seq")?;
    c.damage = 0;
    c.rested = rested;
    c.entered_turn = turn;
    knowledge_on_enter(c, src, dst, reveal);
    if db.by_id(c.def_id).is_token() && !TOKEN_ZONES.contains(&dst) {
        c.zone = Zone::Outside;
        c.known = BOTH_KNOW;
        return Ok(());
    }
    c.zone = dst;
    let owner = c.owner;
    if matches!(dst, Zone::Deck | Zone::Shield | Zone::ResourceDeck) && !bottom {
        st.zones.insert(owner, dst, 0, uid)
    } else {
        st.zones.push(owner, dst, uid)
    }
}

/// `remove_from_game(st, uid)` (`core.py:109-116`): a token leaves the game entirely (rules
/// 5-17-2-5, 5-17-3-2-3); damage, rest and entered turn stay as they were.
///
/// # Errors
/// Python's `ValueError` when the card is missing from its zone list; overflows.
pub(crate) fn remove_from_game(st: &mut GameState, uid: Uid) -> Result<(), EngineError> {
    remove_from_zone(st, uid)?;
    let c = &mut st.cards[usize::from(uid)];
    c.zone = Zone::Outside;
    c.zone_seq = incr(c.zone_seq, "zone_seq")?;
    c.known = BOTH_KNOW;
    st.touch(); // core.py:116
    Ok(())
}

/// `new_card(st, def_id, owner, zone, *, rested=False)` (`core.py:119-131`): appends a card
/// (zone_seq 0, damage 0, unpaired, known to both) to `zones[owner][zone]`, always at the end.
///
/// # Errors
/// [`EngineError::Overflow`] past `u16::MAX` cards or zone-arena entries.
pub(crate) fn new_card(
    st: &mut GameState,
    def: DefId,
    owner: u8,
    zone: Zone,
    rested: bool,
) -> Result<Uid, EngineError> {
    let uid = Uid::try_from(st.cards.len()).map_err(|_| EngineError::Overflow { what: "uid" })?;
    st.cards.push(CardInstance {
        def_id: def,
        owner,
        zone,
        rested,
        damage: 0,
        pair: NO_ARG,
        zone_seq: 0,
        entered_turn: st.turn,
        known: BOTH_KNOW,
    });
    st.zones.push(owner, zone, uid)?;
    st.touch(); // core.py:130
    Ok(uid)
}

/// `shuffle_deck(st, player)` (`core.py:134-139`): one `st.rng.shuffle` of the whole DECK
/// list in place (Inv 06a §15 item 1), every deck card unknown to both players, one touch.
pub(crate) fn shuffle_deck(st: &mut GameState, p: u8) {
    st.rng.shuffle(st.zones.get_mut(p, Zone::Deck));
    for &uid in st.zones.get(p, Zone::Deck) {
        st.cards[usize::from(uid)].known = 0;
    }
    st.touch(); // core.py:139
}

/// `record(st, kind, player, by=-1, uid=-1, source=-1)` (`core.py:142-151`): appends a
/// history event with the card's `def_id` at record time, or -1. Does not touch (the history
/// length is part of the view key instead, `view.py:229`).
pub(crate) fn record(
    st: &mut GameState,
    kind: HistKind,
    player: u8,
    by: i8,
    uid: i32,
    source: i32,
) {
    let def_id = if uid >= 0 {
        // Every call site passes the uid of an existing card.
        st.cards
            .get(usize::try_from(uid).unwrap_or(usize::MAX))
            .map_or(NO_ARG, |c| i32::from(c.def_id.0))
    } else {
        NO_ARG
    };
    st.history.push(HistoryEvent {
        kind,
        player,
        by,
        uid,
        def_id,
        source,
    });
}

// ---------------------------------------------------------------------------------------------
// events and triggers (core.py:158-482)

/// `next_group(st)` (`core.py:158-160`): `st.event_group += 1` and the new value; called
/// unconditionally at Python's sites (Inv 06a hazard 13).
pub(crate) fn next_group(st: &mut GameState) -> i64 {
    st.event_group += 1;
    st.event_group
}

/// `_payload(event, key)` (`core.py:187-191`): the first pair with `key`, else -1.
pub(crate) fn payload(event: &EventPayload, key: EvKey) -> i32 {
    event.get(key).unwrap_or(NO_ARG)
}

/// `_subject_matches(st, dv, host_owner, host, sel, subject, include_self, event)`
/// (`core.py:163-184`); `sel.loc` and `sel.top_n` are not read.
#[allow(
    clippy::too_many_arguments,
    reason = "Python's parameter list (core.py:163-172)"
)]
fn subject_matches(
    st: &GameState,
    dv: &Derived,
    host_owner: u8,
    host: i32,
    sel: SelId,
    subject: i32,
    include_self: bool,
    event: &EventPayload,
) -> Result<bool, EngineError> {
    if subject < 0 {
        return Ok(false);
    }
    if !include_self && subject == host {
        return Ok(false);
    }
    let owner = card_at(st, subject)?.owner;
    let sel = sel_node(st.registry(), sel)?;
    if sel.side == Side::Friendly && owner != host_owner {
        return Ok(false);
    }
    if sel.side == Side::Enemy && owner == host_owner {
        return Ok(false);
    }
    if !sel.filters.is_empty() {
        let ctx = Ctx::new(host_owner, host, NO_ARG, CtxVars::Empty, *event);
        return view::matches(st, dv, &ctx, uid_at(st, subject)?, &sel.filters);
    }
    Ok(true)
}

/// `_trigger_ok(st, dv, trig, host, host_owner, event)` (`core.py:194-233`), checks in
/// Python's order (Inv 06a §6.1): `whose_turn` YOU means the host owner's turn and every other
/// value the opponent's.
pub(crate) fn trigger_ok(
    st: &GameState,
    dv: &Derived,
    trig: TriggerId,
    host: i32,
    host_owner: u8,
    event: &EventPayload,
) -> Result<bool, EngineError> {
    let reg = st.registry();
    let trig = trigger_node(reg, trig)?;
    let subject = payload(event, EvKey::Subject);
    if let Some(whose) = trig.whose_turn {
        let want = if whose == P::You {
            host_owner
        } else {
            1 - host_owner
        };
        if st.active != want {
            return Ok(false);
        }
    }
    if trig.self_only {
        if subject != host {
            return Ok(false);
        }
    } else if let Some(sel) = trig.subject {
        if !subject_matches(
            st,
            dv,
            host_owner,
            host,
            sel,
            subject,
            trig.include_self,
            event,
        )? {
            return Ok(false);
        }
    } else if !trig.include_self && subject == host {
        return Ok(false);
    }
    if !trig.pilot_filters.is_empty() {
        let pilot = payload(event, EvKey::Pilot);
        if pilot < 0 {
            return Ok(false);
        }
        let ctx = Ctx::new(
            host_owner,
            host,
            NO_ARG,
            CtxVars::Empty,
            EventPayload::default(),
        );
        if !view::matches(st, dv, &ctx, uid_at(st, pilot)?, &trig.pilot_filters)? {
            return Ok(false);
        }
    }
    if !trig.target_filters.is_empty() {
        let target = payload(event, EvKey::Target);
        if target < 0 {
            return Ok(false);
        }
        let ctx = Ctx::new(
            host_owner,
            host,
            NO_ARG,
            CtxVars::Empty,
            EventPayload::default(),
        );
        if !view::matches(st, dv, &ctx, uid_at(st, target)?, &trig.target_filters)? {
            return Ok(false);
        }
    }
    if let Some(battle_only) = trig.battle_only
        && (payload(event, EvKey::Battle) == 1) != battle_only
    {
        return Ok(false);
    }
    if let Some(by_enemy) = trig.by_enemy {
        let by = payload(event, EvKey::By);
        if (by >= 0 && by != i32::from(host_owner)) != by_enemy {
            return Ok(false);
        }
    }
    Ok(true)
}

/// `_once_key(a, host, host_seq)` (`core.py:236-237`): `(aid, host, host_seq)`.
fn once_key(a: &AbilityEntry, host: Uid, host_seq: i32) -> Result<OnceKey, EngineError> {
    OnceKey::from_elems(&[
        OnceElem::Int(i32::from(a.aid.0)),
        OnceElem::Int(i32::from(host)),
        OnceElem::Int(host_seq),
    ])
}

/// `_card_uid_for(st, a, host)` (`core.py:240-245`): the paired Pilot for a Pilot-granted
/// (`unit_text`) ability whose provider is that Pilot, else the host.
pub(crate) fn card_uid_for(st: &GameState, a: &AbilityEntry, host: Uid) -> i32 {
    if a.unit_text {
        let c = st.cards[usize::from(host)];
        if c.pair >= 0
            && st
                .cards
                .get(usize::try_from(c.pair).unwrap_or(usize::MAX))
                .is_some_and(|p| p.def_id == a.def_id)
        {
            return c.pair;
        }
    }
    i32::from(host)
}

/// `queue_trigger(st, a, host, controller, event, card_uid=-1, host_seq=-1)`
/// (`core.py:248-284`): the simultaneous-event dedupe on `(aid, host, group)` over
/// `st.pending_triggers` only (rule 10-1-6-3), the once-per-turn check, then a new pending
/// `TriggerInst` (Inv 06a §6.2).
///
/// # Errors
/// Python's assertion when `a` is not a triggered ability; capacities.
#[allow(
    clippy::too_many_arguments,
    reason = "Python's parameter list (core.py:248-256)"
)]
pub(crate) fn queue_trigger(
    st: &mut GameState,
    a: AbilityId,
    host: Uid,
    controller: u8,
    event: &EventPayload,
    card_uid: i32,
    host_seq: i32,
) -> Result<(), EngineError> {
    let reg = st.registry().clone();
    let a = reg.ability(a);
    let ab = triggered(&reg, a)?;
    let host_seq = if host_seq == NO_ARG {
        st.cards[usize::from(host)].zone_seq
    } else {
        host_seq
    };
    let group = payload(event, EvKey::Group);
    let aid = i32::from(a.aid.0);
    for t in st.pending_triggers() {
        if t.ability_key.aid == aid
            && t.host == i32::from(host)
            && payload(&t.event, EvKey::Group) == group
            && group >= 0
        {
            return Ok(()); // rule 10-1-6-3: simultaneous events trigger an effect only once
        }
    }
    let once = if ab.once_per_turn {
        Some(once_key(a, host, host_seq)?)
    } else {
        None
    };
    if let Some(k) = &once
        && st.once_contains(k)
    {
        return Ok(());
    }
    let program = a
        .program
        .ok_or_else(|| EngineError::Internal(format!("triggered ability {aid} has no program")))?;
    let card_uid = if card_uid != NO_ARG {
        card_uid
    } else {
        card_uid_for(st, a, host)
    };
    st.trigger_arena().push_pending(TriggerInst {
        program,
        controller,
        host: i32::from(host),
        host_seq,
        card_uid,
        ability_key: AbilityKey {
            def_id: i32::from(a.def_id.0),
            aid,
            unit_text: a.unit_text,
        },
        event: *event,
        burst: false,
        once_key: once,
        optional: false,
    })
}

/// The event tuple of `emit` (`core.py:328-333`): `("subject", s), ("player", p), ("group",
/// g)`, then the other keyword arguments sorted by key (their derived order is string order).
/// A `Group` in `extra` is popped first; -1 or no group means the current `st.event_group`.
fn event_payload(
    st: &GameState,
    subject: i32,
    player: i32,
    extra: &[(EvKey, i32)],
) -> Result<EventPayload, EngineError> {
    let mut group = NO_ARG;
    // The extras sorted on the stack: more than fit would overflow the payload below anyway,
    // so a full buffer only remembers that the payload must fail at the same point.
    let mut rest: InlineVec<(EvKey, i32), EVENT_CAPACITY> = InlineVec::new();
    let mut overflow = false;
    for &(k, v) in extra {
        if k == EvKey::Group {
            group = v;
        } else if rest.try_push((k, v)).is_err() {
            overflow = true;
        }
    }
    if group == NO_ARG {
        group = to_i32(st.event_group, "event group")?;
    }
    rest.as_mut_slice().sort();
    let mut ev = EventPayload::default();
    ev.push(EvKey::Subject, subject)?;
    ev.push(EvKey::Player, player)?;
    ev.push(EvKey::Group, group)?;
    for &(k, v) in rest.as_slice() {
        ev.push(k, v)?;
    }
    if overflow {
        return Err(EngineError::Capacity {
            what: "event payload",
            capacity: EVENT_CAPACITY,
        });
    }
    Ok(ev)
}

/// `emit(st, ev, subject=-1, *, player=-1, lki=None, **extra)` (`core.py:316-356`): emits an
/// event and queues every triggered effect it fulfils (rule 10-1-6). Nothing once the game is
/// over; nothing when no card of the game listens, no delayed trigger is armed and no LKI is
/// given. Candidates come in trigger-index order (live hosts not in `lki`), then the LKI
/// hosts sorted by `(owner != active, uid)` with their snapshot abilities; then delayed and
/// keyword triggers (Inv 06a §6.7). One view serves every check.
///
/// # Errors
/// Errors of the view and of filter, condition and value evaluation; capacities.
pub(crate) fn emit(
    st: &mut GameState,
    ev: Ev,
    subject: i32,
    player: i32,
    lki: Option<&Lki>,
    extra: &[(EvKey, i32)],
) -> Result<(), EngineError> {
    if st.winner.is_some() {
        return Ok(());
    }
    let event = event_payload(st, subject, player, extra)?;
    let lki = lki.filter(|l| !l.is_empty());
    if !st.konst.listened.contains(ev) && st.delayed.is_empty() && lki.is_none() {
        #[cfg(feature = "paranoid")]
        paranoid::unlistened_event(st, ev)?;
        return Ok(());
    }
    let dv = view::derived(st)?;
    let reg = st.registry().clone();
    let empty = Lki::default();
    let lki = lki.unwrap_or(&empty);
    let mut candidates: Vec<(Uid, AbilityId)> = view::trigger_index(st, &dv)
        .get(ev)
        .iter()
        .filter(|(host, _)| !lki.contains(*host))
        .copied()
        .collect();
    let mut hosts: Vec<Uid> = lki.entries.iter().map(|e| e.uid).collect();
    let active = st.active;
    hosts.sort_by_key(|&u| (st.cards[usize::from(u)].owner != active, u));
    for u in hosts {
        candidates.extend(
            lki.abilities_of(u)
                .iter()
                .filter(|&&a| reg.ability(a).trigger_event == Some(ev))
                .map(|&a| (u, a)),
        );
    }
    for (host, aid) in candidates {
        let a = reg.ability(aid);
        let ab = triggered(&reg, a)?;
        let owner = st.cards[usize::from(host)].owner;
        if !trigger_ok(st, &dv, ab.trigger, i32::from(host), owner, &event)? {
            continue;
        }
        if !is_true(&reg, ab.cond) {
            let ctx = Ctx::new(
                owner,
                i32::from(host),
                card_uid_for(st, a, host),
                CtxVars::Empty,
                event,
            );
            if !view::cond(st, &dv, &ctx, ab.cond, true)? {
                continue;
            }
        }
        let host_seq = lki.get(host).map_or(NO_ARG, |s| s.seq);
        queue_trigger(st, aid, host, owner, &event, NO_ARG, host_seq)?;
    }
    delayed_triggers(st, &dv, ev, &event, lki)?;
    keyword_triggers(st, &dv, ev, &event, subject)
}

/// `_delayed_triggers(st, dv, ev, event, lki)` (`core.py:359-403`): every armed delayed
/// trigger whose event this is and whose trigger is fulfilled queues its program, every time
/// (rule 10-1-6-1-1), with no dedupe, once key or condition (Inv 06a §6.4). `_find_delayed`
/// (`core.py:411-421`) is the IR's per-card `delayed` table in `_walk_delayed` order over the
/// card's own then unit abilities (PLAN §3.3), and `R.program(dt.steps, ...)` its program.
fn delayed_triggers(
    st: &mut GameState,
    dv: &Derived,
    ev: Ev,
    event: &EventPayload,
    lki: &Lki,
) -> Result<(), EngineError> {
    if st.delayed.is_empty() {
        return Ok(());
    }
    let reg = st.registry().clone();
    for i in 0..st.delayed.len() {
        let le = st.delayed[i];
        let ContView::AbilityGrant {
            card_number,
            ability_index,
        } = reg.cont_node(le.effect)
        else {
            return Err(EngineError::Assertion(
                "a delayed trigger's effect is not an AbilityGrant".to_owned(),
            ));
        };
        let Some(entry) = st.db().get(reg.str(card_number)) else {
            continue;
        };
        let Some(&(_, dt, program)) = reg
            .card(entry.def_id)
            .delayed
            .iter()
            .find(|&&(n, _, _)| i64::from(n) == ability_index)
        else {
            continue;
        };
        let Node::DelayedTrigger(dt) = reg.node(dt) else {
            return Err(kind_error("DelayedTrigger", dt.0));
        };
        let trig = trigger_node(&reg, dt.trigger)?;
        if trig.event != ev {
            continue;
        }
        let host = le.source_uid;
        let targets = &st.lasting_targets[le.targets.range()];
        let mut bound: Vec<i32> = Vec::with_capacity(targets.len());
        for &(u, seq) in targets {
            let cur = lki
                .get(u)
                .map_or(st.cards[usize::from(u)].zone_seq, |s| s.seq);
            if cur == seq {
                bound.push(i32::from(u));
            }
        }
        let subject = payload(event, EvKey::Subject);
        let ok = if trig.self_only && !targets.is_empty() {
            bound.contains(&subject)
                && trigger_ok(
                    st,
                    dv,
                    dt.trigger,
                    subject,
                    card_at(st, subject)?.owner,
                    event,
                )?
        } else {
            trigger_ok(st, dv, dt.trigger, host, le.controller, event)?
        };
        if ok {
            let host_seq = card_at(st, host)?.zone_seq;
            st.trigger_arena().push_pending(TriggerInst {
                program,
                controller: le.controller,
                host,
                host_seq,
                card_uid: host,
                ability_key: AbilityKey {
                    def_id: -1,
                    aid: -1,
                    unit_text: false,
                },
                event: *event,
                burst: false,
                once_key: None,
                optional: false,
            })?;
        }
    }
    Ok(())
}

/// `_keyword_triggers(st, dv, ev, event, subject)` (`core.py:441-482`): <Repair> at TURN_END
/// for each damaged active Unit with Repair (rule 13-1-1-1, Q51), and <Breach> on
/// DESTROYS_BY_BATTLE during its owner's turn when the victim is a Unit and the victim's owner
/// still has a Base or Shield (rules 13-1-2, 13-1-2-4).
fn keyword_triggers(
    st: &mut GameState,
    dv: &Derived,
    ev: Ev,
    event: &EventPayload,
    subject: i32,
) -> Result<(), EngineError> {
    if ev == Ev::TurnEnd {
        let program = st.registry().keyword_program(KeywordProgram::Repair);
        let active = st.active;
        let field: UidBuf = st.zones.get(active, Zone::Battle).iter().copied().collect();
        for uid in field {
            let c = st.cards[usize::from(uid)];
            if view::kw_amount(dv, uid, Kw::Repair) > 0 && c.damage > 0 {
                st.trigger_arena().push_pending(TriggerInst {
                    program,
                    controller: active,
                    host: i32::from(uid),
                    host_seq: c.zone_seq,
                    card_uid: i32::from(uid),
                    ability_key: AbilityKey {
                        def_id: -2,
                        aid: 0,
                        unit_text: false,
                    },
                    event: *event,
                    burst: false,
                    once_key: None,
                    optional: false,
                })?;
            }
        }
    } else if ev == Ev::DestroysByBattle && subject >= 0 {
        let owner = card_at(st, subject)?.owner;
        let amount = payload(event, EvKey::Breach);
        let victim = payload(event, EvKey::Target);
        if amount <= 0 || owner != st.active || victim < 0 {
            return Ok(());
        }
        if !st
            .db()
            .by_id(card_at(st, victim)?.def_id)
            .card_type
            .is_unit()
        {
            return Ok(());
        }
        let victim_owner = 1 - owner;
        if st.zones.get(victim_owner, Zone::Base).is_empty()
            && st.zones.get(victim_owner, Zone::Shield).is_empty()
        {
            return Ok(()); // rule 13-1-2-4
        }
        let program = st.registry().keyword_program(KeywordProgram::Breach);
        let host_seq = card_at(st, subject)?.zone_seq;
        st.trigger_arena().push_pending(TriggerInst {
            program,
            controller: owner,
            host: subject,
            host_seq,
            card_uid: subject,
            ability_key: AbilityKey {
                def_id: -3,
                aid: 0,
                unit_text: false,
            },
            event: *event,
            burst: false,
            once_key: None,
            optional: false,
        })?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// last-known information (core.py:485-500)

impl Lki {
    /// `uid in lki`.
    pub(crate) fn contains(&self, uid: Uid) -> bool {
        self.entries.iter().any(|e| e.uid == uid)
    }

    /// `lki.get(uid)`.
    pub(crate) fn get(&self, uid: Uid) -> Option<&LkiEntry> {
        self.entries.iter().find(|e| e.uid == uid)
    }

    /// `not lki`.
    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// `lki[uid].abilities`, or nothing for a uid without a snapshot.
    pub(crate) fn abilities_of(&self, uid: Uid) -> &[AbilityId] {
        self.get(uid)
            .map_or(&[][..], |e| &self.abilities[e.abilities.range()])
    }

    /// `lki[uid] = Snapshot(abilities, seq)`: a new uid is appended, an existing one keeps
    /// its position and takes the new value (dict semantics).
    pub(crate) fn insert(
        &mut self,
        uid: Uid,
        abilities: &[AbilityId],
        seq: i32,
    ) -> Result<(), EngineError> {
        let start = u32::try_from(self.abilities.len())
            .map_err(|_| EngineError::Overflow { what: "LKI" })?;
        let len =
            u32::try_from(abilities.len()).map_err(|_| EngineError::Overflow { what: "LKI" })?;
        self.abilities.extend_from_slice(abilities);
        let span = if len == 0 {
            Span::EMPTY
        } else {
            Span { start, len }
        };
        if let Some(e) = self.entries.iter_mut().find(|e| e.uid == uid) {
            e.seq = seq;
            e.abilities = span;
        } else {
            self.entries.push(LkiEntry {
                uid,
                seq,
                abilities: span,
            });
        }
        Ok(())
    }

    /// `{k: v for k, v in lki.items() if k == uid}` (`interp.py:776`) and `{src: lki[src]}`
    /// (`core.py:910`, `battle.py:117`): the snapshot of `uid` alone, or an empty map.
    pub(crate) fn only(&self, uid: Uid) -> Self {
        let mut out = Self::default();
        if let Some(e) = self.get(uid) {
            out.abilities
                .extend_from_slice(&self.abilities[e.abilities.range()]);
            out.entries.push(LkiEntry {
                uid,
                seq: e.seq,
                abilities: if e.abilities.len == 0 {
                    Span::EMPTY
                } else {
                    Span {
                        start: 0,
                        len: e.abilities.len,
                    }
                },
            });
        }
        out
    }
}

/// `ability_snapshot(st, uids)` (`core.py:494-496`): `{u: Snapshot(dv.abilities.get(u, ()),
/// zone_seq)}` in `uids` order on a fresh view (rules 13-2-8-2, 13-2-8-2-1).
///
/// # Errors
/// Errors of the view.
pub(crate) fn ability_snapshot(st: &GameState, uids: &[Uid]) -> Result<Lki, EngineError> {
    let dv = view::derived(st)?;
    let mut out = Lki::default();
    for &u in uids {
        out.insert(
            u,
            view::abilities_of(&dv, u),
            st.cards[usize::from(u)].zone_seq,
        )?;
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------
// damage (core.py:503-771)

/// `(tag, *r.key, uid, zone_seq)` (`core.py:524`, `:561`): `r.key` is `(aid, host)` for a
/// constant and `(-1, effect_key, serial)` for a lasting effect (`view.py:436`, `:451`).
fn rule_once_key(tag: i32, key: RuleKey, uid: Uid, seq: i32) -> Result<OnceKey, EngineError> {
    let (u, s) = (OnceElem::Int(i32::from(uid)), OnceElem::Int(seq));
    match key {
        RuleKey::Constant { aid, host } => OnceKey::from_elems(&[
            OnceElem::Int(tag),
            OnceElem::Int(i32::from(aid.0)),
            OnceElem::Int(i32::from(host)),
            u,
            s,
        ]),
        RuleKey::Lasting { effect, serial } => OnceKey::from_elems(&[
            OnceElem::Int(tag),
            OnceElem::Int(NO_ARG),
            OnceElem::Cont(effect),
            OnceElem::Int(serial),
            u,
            s,
        ]),
    }
}

/// The rules of `kind` on `uid` in the view, copied out so that `st` can change while they
/// are walked (Python's `rules_of` builds a new list, `view.py:565-566`).
fn rules(dv: &Derived, uid: Uid, kind: RuleKind) -> Vec<RuleInst> {
    view::rules_of(dv, uid, kind).copied().collect()
}

/// `_prevented(st, dv, uid, amount, source, battle, by, *, thresholds=False)`
/// (`core.py:503-529`): a "can't receive damage" rule applies. With `thresholds`, only rules
/// limited to damage of their `amount` or less (GD05-123), against the already reduced amount
/// (Q416); otherwise only unlimited rules. A once-per-turn rule's key is consumed here, even
/// when the caller only asks (Inv 06a hazard 12).
#[allow(
    clippy::too_many_arguments,
    reason = "Python's parameter list (core.py:503-513)"
)]
fn prevented(
    st: &mut GameState,
    dv: &Derived,
    uid: Uid,
    amount: i64,
    source: i32,
    battle: bool,
    by: i32,
    thresholds: bool,
) -> Result<bool, EngineError> {
    let reg = st.registry().clone();
    for r in rules(dv, uid, RuleKind::CantReceiveDamage) {
        let rule = rule_mod(&reg, r.rule)?;
        let limit = int_or(rule.amount, 0);
        let limited = limit > 0;
        if limited != thresholds || (limited && amount > limit) {
            continue;
        }
        if damage_rule_applies(st, dv, &r, uid, source, battle, by)? {
            if rule.once_per_turn {
                let key = rule_once_key(
                    PREVENT_ONCE_TAG,
                    r.key,
                    uid,
                    st.cards[usize::from(uid)].zone_seq,
                )?;
                if st.once_contains(&key) {
                    continue;
                }
                st.once_add(key);
            }
            return Ok(true);
        }
    }
    Ok(false)
}

/// `_damage_rule_applies(st, dv, r, uid, source, battle, by)` (`core.py:532-550`).
#[allow(
    clippy::too_many_arguments,
    reason = "Python's parameter list (core.py:532-534)"
)]
fn damage_rule_applies(
    st: &GameState,
    dv: &Derived,
    r: &RuleInst,
    uid: Uid,
    source: i32,
    battle: bool,
    by: i32,
) -> Result<bool, EngineError> {
    let rule = rule_mod(st.registry(), r.rule)?;
    if rule.damage_kind == DamageKind::Battle && !battle {
        return Ok(false);
    }
    if rule.damage_kind == DamageKind::Effect && battle {
        return Ok(false);
    }
    let owner = i32::from(st.cards[usize::from(uid)].owner);
    if rule.source_side == Side::Enemy && (by < 0 || by == owner) {
        return Ok(false);
    }
    if rule.source_side == Side::Friendly && by != owner {
        return Ok(false);
    }
    if !rule.source_filters.is_empty() {
        if source < 0 {
            return Ok(false);
        }
        let ctx = Ctx::new(
            st.cards[usize::from(uid)].owner,
            i32::from(uid),
            NO_ARG,
            CtxVars::Empty,
            EventPayload::default(),
        );
        if !view::matches(st, dv, &ctx, uid_at(st, source)?, &rule.source_filters)? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// `_reduce(st, dv, uid, amount, source, battle, by)` (`core.py:553-567`): every applicable
/// "reduce damage" rule in view order, stopping once nothing is left (before further once keys
/// are consumed); at least 0.
#[allow(
    clippy::too_many_arguments,
    reason = "Python's parameter list (core.py:553-555)"
)]
fn reduce(
    st: &mut GameState,
    dv: &Derived,
    uid: Uid,
    mut amount: i64,
    source: i32,
    battle: bool,
    by: i32,
) -> Result<i64, EngineError> {
    let reg = st.registry().clone();
    for r in rules(dv, uid, RuleKind::ReduceDamage) {
        if amount <= 0 {
            break;
        }
        if !damage_rule_applies(st, dv, &r, uid, source, battle, by)? {
            continue;
        }
        let rule = rule_mod(&reg, r.rule)?;
        let ikey = rule_once_key(
            REDUCE_ONCE_TAG,
            r.key,
            uid,
            st.cards[usize::from(uid)].zone_seq,
        )?;
        if rule.once_per_turn {
            if st.once_contains(&ikey) {
                continue;
            }
            st.once_add(ikey);
        }
        let ctx = Ctx::new(
            r.controller,
            r.source,
            NO_ARG,
            CtxVars::Empty,
            EventPayload::default(),
        );
        let v = view::value(st, dv, &ctx, rule.amount)?;
        amount = amount
            .checked_sub(v)
            .ok_or(EngineError::Overflow { what: "damage" })?;
    }
    Ok(amount.max(0))
}

/// `damage_card(st, uid, amount, *, source, battle, by)` (`core.py:570-573`): deals damage to
/// a Unit or Base and returns the damage actually dealt (rules 5-5, 5-21).
///
/// # Errors
/// [`EngineError::RedirectCycle`] (W-143) and the errors of the view and of emitting.
pub(crate) fn damage_card(
    st: &mut GameState,
    uid: Uid,
    amount: i64,
    d: DamageOpts,
) -> Result<i64, EngineError> {
    let (dest, fin) = resolve_damage(st, uid, amount, d)?;
    apply_damage(st, dest, fin, d)
}

/// `resolve_damage(st, uid, amount, *, source, battle, by)` (`core.py:576-599`): where damage
/// lands and how much after redirection, prevention and reduction, on the current state (so
/// simultaneous battle damage uses the pre-damage state, rule 8-5-3-2).
///
/// Battle damage follows the REDIRECT_BATTLE_DAMAGE chain first ([`follow_redirects`]): Python
/// recurses once per hop and, on a cycle, until `RecursionError`; Rust walks the chain and
/// returns [`EngineError::RedirectCycle`] (W-143). Every chain without a repeat lands where
/// Python's does.
///
/// # Errors
/// [`EngineError::RedirectCycle`]; errors of the view, filters and values.
pub(crate) fn resolve_damage(
    st: &mut GameState,
    uid: Uid,
    amount: i64,
    d: DamageOpts,
) -> Result<(Uid, i64), EngineError> {
    if amount <= 0 {
        return Ok((uid, 0));
    }
    if !matches!(st.cards[usize::from(uid)].zone, Zone::Battle | Zone::Base) {
        return Ok((uid, 0));
    }
    let dv = view::derived(st)?;
    // Every hop lands on a Unit in BATTLE with the same `amount`, and nothing touches the
    // state between hops, so Python's recursive call re-checks nothing new and reads the same
    // cached view (core.py:582-591).
    let uid = if d.battle {
        follow_redirects(uid, |u| redirect_of(st, &dv, u))?
    } else {
        uid
    };
    let (source, battle, by) = (d.source, d.battle, d.by);
    if prevented(st, &dv, uid, amount, source, battle, by, false)? {
        return Ok((uid, 0));
    }
    let mut amount = reduce(st, &dv, uid, amount, source, battle, by)?;
    let c = st.cards[usize::from(uid)];
    if c.zone == Zone::Base {
        let r = shield_area_reduction(st, c.owner, source, battle, by)?;
        amount = amount
            .checked_sub(r)
            .ok_or(EngineError::Overflow { what: "damage" })?;
    }
    if amount <= 0 || prevented(st, &dv, uid, amount, source, battle, by, true)? {
        return Ok((uid, 0));
    }
    Ok((uid, amount))
}

/// The destination of the first usable REDIRECT_BATTLE_DAMAGE rule on `uid`
/// (`core.py:588-591`): its `aux` is another Unit in BATTLE.
fn redirect_of(st: &GameState, dv: &Derived, uid: Uid) -> Result<Option<Uid>, EngineError> {
    for r in view::rules_of(dv, uid, RuleKind::RedirectBattleDamage) {
        let dest = r.aux;
        if dest >= 0 && dest != i32::from(uid) && card_at(st, dest)?.zone == Zone::Battle {
            return Ok(Some(uid_at(st, dest)?));
        }
    }
    Ok(None)
}

/// The end of the redirect chain from `start`, where `next(u)` is `u`'s redirect destination.
/// Python recurses once per hop (`core.py:591`); a destination already on the chain repeats
/// the same calls forever, until `RecursionError`. Here it is [`EngineError::RedirectCycle`]
/// naming the chain and the repeated Unit (W-143).
fn follow_redirects(
    start: Uid,
    mut next: impl FnMut(Uid) -> Result<Option<Uid>, EngineError>,
) -> Result<Uid, EngineError> {
    let mut chain: UidBuf = UidBuf::new();
    let mut uid = start;
    while let Some(dest) = next(uid)? {
        chain.push(uid);
        if chain.contains(&dest) {
            chain.push(dest);
            return Err(EngineError::RedirectCycle {
                uids: chain.to_vec().into_boxed_slice(),
            });
        }
        uid = dest;
    }
    Ok(uid)
}

/// `apply_damage(st, uid, amount, *, source, battle, by)` (`core.py:602-633`): nothing for no
/// damage or a card outside BATTLE/BASE (rules 5-5-5, 5-21-2-1); else the damage is added,
/// effect damage records its source in `effect_hits`, one touch, then DAMAGED and, with a
/// source, DEALS_DAMAGE.
///
/// # Errors
/// Overflows and the errors of emitting.
pub(crate) fn apply_damage(
    st: &mut GameState,
    uid: Uid,
    amount: i64,
    d: DamageOpts,
) -> Result<i64, EngineError> {
    let c = st.cards[usize::from(uid)];
    if amount <= 0 || !matches!(c.zone, Zone::Battle | Zone::Base) {
        return Ok(0); // rule 5-5-5 / 5-21-2-1
    }
    let amt = to_i32(amount, "damage")?;
    st.cards[usize::from(uid)].damage = c
        .damage
        .checked_add(amt)
        .ok_or(EngineError::Overflow { what: "damage" })?;
    if !d.battle && d.source >= 0 {
        let src = uid_at(st, d.source)?;
        st.effect_hit_set(uid, src);
    }
    st.touch(); // core.py:611
    let battle = i32::from(d.battle);
    emit(
        st,
        Ev::Damaged,
        i32::from(uid),
        i32::from(c.owner),
        None,
        &[
            (EvKey::Amount, amt),
            (EvKey::Battle, battle),
            (EvKey::Source, d.source),
            (EvKey::By, d.by),
        ],
    )?;
    if d.source >= 0 {
        let owner = card_at(st, d.source)?.owner;
        emit(
            st,
            Ev::DealsDamage,
            d.source,
            i32::from(owner),
            None,
            &[
                (EvKey::Target, i32::from(uid)),
                (EvKey::Amount, amt),
                (EvKey::Battle, battle),
                (EvKey::By, d.by),
            ],
        )?;
    }
    Ok(amount)
}

/// `_shield_area_rules(st, player, kind, source, battle, by)` (`core.py:636-664`): the rules of
/// `kind` granted to the player by lasting effects that apply to this damage (no FRIENDLY
/// check here). A view is computed whenever any effect is lasting.
fn shield_area_rules(
    st: &GameState,
    player: u8,
    kind: RuleKind,
    source: i32,
    battle: bool,
    by: i32,
) -> Result<Vec<RuleModId>, EngineError> {
    if st.lasting.is_empty() {
        return Ok(Vec::new());
    }
    let reg = st.registry();
    let dv = view::derived(st)?;
    let mut out = Vec::new();
    for le in &st.lasting {
        if i32::from(le.player) != i32::from(player) {
            continue;
        }
        let ContView::RuleGrant { rule: rid } = reg.cont_node(le.effect) else {
            continue;
        };
        let rule = rule_mod(reg, rid)?;
        if rule.kind != kind {
            continue;
        }
        if rule.damage_kind == DamageKind::Battle && !battle {
            continue;
        }
        if rule.damage_kind == DamageKind::Effect && battle {
            continue;
        }
        if rule.source_side == Side::Enemy && (by < 0 || by == i32::from(player)) {
            continue;
        }
        if !rule.source_filters.is_empty() {
            if source < 0 {
                continue;
            }
            let ctx = Ctx::new(
                player,
                source,
                NO_ARG,
                CtxVars::Empty,
                EventPayload::default(),
            );
            if !view::matches(st, &dv, &ctx, uid_at(st, source)?, &rule.source_filters)? {
                continue;
            }
        }
        out.push(rid);
    }
    Ok(out)
}

/// `shield_area_protected(st, player, source, battle, by)` (`core.py:667-672`).
///
/// # Errors
/// Errors of the view and of filter evaluation.
pub(crate) fn shield_area_protected(
    st: &GameState,
    player: u8,
    source: i32,
    battle: bool,
    by: i32,
) -> Result<bool, EngineError> {
    Ok(!shield_area_rules(
        st,
        player,
        RuleKind::ShieldAreaProtection,
        source,
        battle,
        by,
    )?
    .is_empty())
}

/// `shield_area_reduction(st, player, source, battle, by)` (`core.py:675-678`): the sum of the
/// int amounts (ST11-006, ruling Q430); other amounts are ignored.
///
/// # Errors
/// Errors of the view and of filter evaluation.
pub(crate) fn shield_area_reduction(
    st: &GameState,
    player: u8,
    source: i32,
    battle: bool,
    by: i32,
) -> Result<i64, EngineError> {
    let reg = st.registry();
    let mut total: i64 = 0;
    for rid in shield_area_rules(
        st,
        player,
        RuleKind::ShieldAreaReduction,
        source,
        battle,
        by,
    )? {
        if let ValueRef::Lit(x) = rule_mod(reg, rid)?.amount {
            total = total
                .checked_add(x)
                .ok_or(EngineError::Overflow { what: "damage" })?;
        }
    }
    Ok(total)
}

/// `shields_receiving_damage(st, uids, amount, *, source, battle, by)` (`core.py:681-697`):
/// the Shields among `uids` that `amount` damage would destroy: not protected by an unlimited
/// "can't receive damage" rule, and (only then) not by a limited one against the amount left
/// after the shield-area reduction. Both checks consume once keys (Inv 06a hazard 12).
///
/// # Errors
/// Errors of the view and of filter evaluation.
pub(crate) fn shields_receiving_damage(
    st: &mut GameState,
    uids: &[Uid],
    amount: i64,
    d: DamageOpts,
) -> Result<UidBuf, EngineError> {
    let Some(&first) = uids.first() else {
        return Ok(UidBuf::new());
    };
    let dv = view::derived(st)?;
    let owner = st.cards[usize::from(first)].owner;
    let left = amount
        .checked_sub(shield_area_reduction(st, owner, d.source, d.battle, d.by)?)
        .ok_or(EngineError::Overflow { what: "damage" })?;
    if left <= 0 {
        return Ok(UidBuf::new());
    }
    let mut out = UidBuf::new();
    for &u in uids {
        if !prevented(st, &dv, u, amount, d.source, d.battle, d.by, false)?
            && !prevented(st, &dv, u, left, d.source, d.battle, d.by, true)?
        {
            out.push(u);
        }
    }
    Ok(out)
}

/// `destroy_shields(st, player, uids, *, battle, source, by)` (`core.py:700-749`): Shields are
/// destroyed simultaneously (rules 5-10-3, 13-1-7-4): one group; each is revealed, a Burst card
/// moves to RESOLVING and queues its Burst trigger (so every Burst precedes the triggers of the
/// destruction events), any other card goes to the trash; then SHIELD_DESTROYED and, with a
/// source, DESTROYS_SHIELD_CARD per Shield (Inv 06a §6.6).
///
/// # Errors
/// Errors of moving and emitting; capacities.
pub(crate) fn destroy_shields(
    st: &mut GameState,
    p: u8,
    uids: &[Uid],
    d: DamageOpts,
) -> Result<(), EngineError> {
    if uids.is_empty() {
        return Ok(());
    }
    let reg = st.registry().clone();
    let group = to_i32(next_group(st), "event group")?;
    let by = by_i8(d.by)?;
    let reveal = MoveOpts {
        bottom: false,
        reveal: true,
        rested: false,
    };
    for &uid in uids {
        st.cards[usize::from(uid)].known = BOTH_KNOW;
        let entry = reg.card(st.cards[usize::from(uid)].def_id);
        if entry.has_burst {
            let burst_aid = entry.burst_aid.ok_or_else(|| {
                EngineError::Internal("a Burst card without a Burst ability".to_owned())
            })?;
            let program = reg.ability(burst_aid).program.ok_or_else(|| {
                EngineError::Internal("a Burst ability without a program".to_owned())
            })?;
            move_card(st, uid, Zone::Resolving, reveal)?;
            let c = st.cards[usize::from(uid)];
            let mut event = EventPayload::default();
            event.push(EvKey::Subject, i32::from(uid))?;
            event.push(EvKey::Player, i32::from(p))?;
            event.push(EvKey::Group, group)?;
            st.trigger_arena().push_pending(TriggerInst {
                program,
                controller: p,
                host: i32::from(uid),
                host_seq: c.zone_seq,
                card_uid: i32::from(uid),
                ability_key: AbilityKey {
                    def_id: i32::from(c.def_id.0),
                    aid: i32::from(burst_aid.0),
                    unit_text: false,
                },
                event,
                burst: true,
                once_key: None,
                optional: false,
            })?;
        } else {
            move_card(st, uid, Zone::Trash, reveal)?;
        }
        record(st, HistKind::ShieldDestroyed, p, by, i32::from(uid), NO_ARG);
    }
    let battle = i32::from(d.battle);
    for &uid in uids {
        emit(
            st,
            Ev::ShieldDestroyed,
            i32::from(uid),
            i32::from(p),
            None,
            &[
                (EvKey::Battle, battle),
                (EvKey::Source, d.source),
                (EvKey::By, d.by),
                (EvKey::Group, group),
            ],
        )?;
        if d.source >= 0 {
            let owner = card_at(st, d.source)?.owner;
            emit(
                st,
                Ev::DestroysShieldCard,
                d.source,
                i32::from(owner),
                None,
                &[
                    (EvKey::Target, i32::from(uid)),
                    (EvKey::Battle, battle),
                    (EvKey::Group, group),
                ],
            )?;
        }
    }
    Ok(())
}

/// `damage_shield_area(st, player, amount, *, cards, source, battle, by)` (`core.py:752-767`):
/// the Base if present, else the top `cards` Shields (sliced before anything is destroyed).
///
/// # Errors
/// Errors of damaging, destroying and emitting.
pub(crate) fn damage_shield_area(
    st: &mut GameState,
    p: u8,
    amount: i64,
    cards: usize,
    d: DamageOpts,
) -> Result<(), EngineError> {
    if amount <= 0 {
        return Ok(());
    }
    if shield_area_protected(st, p, d.source, d.battle, d.by)? {
        return Ok(());
    }
    if let Some(&base) = st.zones.get(p, Zone::Base).first() {
        damage_card(st, base, amount, d)?;
        return Ok(());
    }
    let top: UidBuf = st
        .zones
        .get(p, Zone::Shield)
        .iter()
        .take(cards)
        .copied()
        .collect();
    let shields = shields_receiving_damage(st, &top, amount, d)?;
    destroy_shields(st, p, &shields, d)
}

// ---------------------------------------------------------------------------------------------
// destruction and rules management (core.py:774-946)

/// `destroy(st, uids, *, battle, by, source=-1)` (`core.py:774-827`): Units, Bases and Pilots
/// paired in the battle area (GD03-110) are destroyed simultaneously (rules 5-10, 13-2-8),
/// except where a "can't be destroyed" rule applies. Duplicates in `uids` are kept, as in
/// Python. The snapshot and the per-card facts are taken before any move; one group; all
/// moves and records first, then one DESTROYED per target. Returns the destroyed uids.
///
/// # Errors
/// Errors of the view, moving and emitting.
pub(crate) fn destroy(
    st: &mut GameState,
    uids: &[Uid],
    battle: bool,
    by: i32,
    source: i32,
) -> Result<UidBuf, EngineError> {
    let dv = view::derived(st)?;
    let mut targets: UidBuf = UidBuf::new();
    for &u in uids {
        let c = st.cards[usize::from(u)];
        if !matches!(c.zone, Zone::Battle | Zone::Base | Zone::Paired) {
            continue;
        }
        let mut protected = false;
        for r in view::rules_of(&dv, u, RuleKind::CantBeDestroyed) {
            if destroy_rule_applies(st, r, u, battle, by)? {
                protected = true;
                break;
            }
        }
        if protected {
            continue;
        }
        targets.push(u);
    }
    if targets.is_empty() {
        return Ok(targets);
    }
    let lki = ability_snapshot(st, &targets)?;
    // info[u]: owner, pilot (-1 for a card in PAIRED), linked; "ap" and "was_zone" are never
    // read (core.py:796-805). A repeated uid overwrites its entry with equal values.
    let info: Vec<(u8, i32, i32)> = targets
        .iter()
        .map(|&u| {
            let c = st.cards[usize::from(u)];
            let on_unit = c.zone != Zone::Paired;
            (
                c.owner,
                if on_unit { c.pair } else { NO_ARG },
                i32::from(view::is_linked(&dv, u)),
            )
        })
        .collect();
    let group = to_i32(next_group(st), "event group")?;
    let by8 = by_i8(by)?;
    for (k, &u) in targets.iter().enumerate() {
        move_card(st, u, Zone::Trash, MoveOpts::default())?;
        record(
            st,
            HistKind::Destroyed,
            info[k].0,
            by8,
            i32::from(u),
            source,
        );
    }
    for (k, &u) in targets.iter().enumerate() {
        let (owner, pilot, linked) = info[k];
        emit(
            st,
            Ev::Destroyed,
            i32::from(u),
            i32::from(owner),
            Some(&lki),
            &[
                (EvKey::Battle, i32::from(battle)),
                (EvKey::By, by),
                (EvKey::Source, source),
                (EvKey::Pilot, pilot),
                (EvKey::Linked, linked),
                (EvKey::Group, group),
            ],
        )?;
    }
    for &u in &targets {
        let c = st.cards[usize::from(u)];
        if st.db().by_id(c.def_id).is_token() && c.zone == Zone::Trash {
            remove_from_game(st, u)?;
        }
    }
    Ok(targets)
}

/// `_destroy_rule_applies(st, dv, r, uid, battle, by)` (`core.py:830-839`).
fn destroy_rule_applies(
    st: &GameState,
    r: &RuleInst,
    uid: Uid,
    battle: bool,
    by: i32,
) -> Result<bool, EngineError> {
    let rule = rule_mod(st.registry(), r.rule)?;
    if rule.damage_kind == DamageKind::Battle && !battle {
        return Ok(false);
    }
    if rule.damage_kind == DamageKind::Effect && battle {
        return Ok(false);
    }
    let owner = i32::from(st.cards[usize::from(uid)].owner);
    Ok(!(rule.source_side == Side::Enemy && (by < 0 || by == owner)))
}

/// `set_winner(st, losers, reason)` (`core.py:842-854`): nothing once decided; two losers
/// (a set: distinct players) are a draw by BOTH_DEFEATED, one loser loses by `reason`; then
/// GAME_OVER and no pending decision. Frames, batches and pending triggers stay. Every Python
/// call site passes a non-empty set (`core.py:866`, `game.py:771`, `battle.py:232`).
pub(crate) fn set_winner(st: &mut GameState, losers: &[u8], reason: EndReason) {
    if st.winner.is_some() {
        return;
    }
    let Some(&first) = losers.first() else {
        debug_assert!(false, "set_winner without a loser");
        return;
    };
    if losers.iter().any(|&p| p != first) {
        st.winner = Some(-1);
        st.end_reason = Some(EndReason::BothDefeated);
    } else {
        st.winner = Some(if first == 0 { 1 } else { 0 });
        st.end_reason = Some(reason);
    }
    st.phase = Phase::GameOver;
    st.step = Step::GameOver;
    st.clear_pending();
}

/// `rules_management(st)` (`core.py:857-892`): the defeat check (rule 11-2), then destruction
/// of cards whose damage reached their HP (11-3), repeated until nothing changes. A scan that
/// finds nothing damaged, or nothing lethal, records `_rules_checked = version`, and the state
/// is not checked again until a `touch`; only the "nothing lethal" exit and a full scan clear
/// `effect_hits` (Inv 06a §9.3, hazard 19).
///
/// # Errors
/// Errors of the view, destroying and emitting.
pub(crate) fn rules_management(st: &mut GameState) -> Result<(), EngineError> {
    if st.winner.is_some() || st.phase == Phase::Setup {
        return Ok(());
    }
    if st.rules_checked == current_version(st)? {
        #[cfg(feature = "paranoid")]
        paranoid::unchanged_state(st)?;
        return Ok(());
    }
    loop {
        let losers: Vec<u8> = (0..2u8)
            .filter(|&p| st.zones.get(p, Zone::Deck).is_empty())
            .collect();
        if !losers.is_empty() {
            set_winner(st, &losers, EndReason::DeckOut);
            return Ok(());
        }
        if !st.konst.hp_reduction_possible {
            let (damaged, lethal) = damage_scan(st);
            #[cfg(feature = "paranoid")]
            if damaged && !lethal {
                paranoid::no_printed_lethal_damage(st)?;
            }
            if !damaged {
                st.rules_checked = current_version(st)?;
                return Ok(());
            }
            if !lethal {
                st.effect_hits_clear();
                st.rules_checked = current_version(st)?;
                return Ok(());
            }
        }
        let dv = view::derived(st)?;
        let mut doomed: UidBuf = UidBuf::new();
        for p in [st.active, 1 - st.active] {
            for z in [Zone::Battle, Zone::Base] {
                for &uid in st.zones.get(p, z) {
                    if st.cards[usize::from(uid)].damage >= view::hp_of(st, &dv, uid) {
                        doomed.push(uid);
                    }
                }
            }
        }
        let mut hits: Vec<(Uid, Uid)> = Vec::new();
        for &u in &doomed {
            if let Some(src) = st.effect_hit_get(u) {
                if let Some(e) = hits.iter_mut().find(|e| e.0 == u) {
                    e.1 = src;
                } else {
                    hits.push((u, src));
                }
            }
        }
        st.effect_hits_clear();
        if doomed.is_empty() {
            return Ok(());
        }
        let mut sources: UidBuf = hits.iter().map(|&(_, s)| s).collect();
        sources.sort();
        sources.dedup();
        let sources_lki = ability_snapshot(st, &sources)?;
        let destroyed = destroy(st, &doomed, false, NO_ARG, NO_ARG)?;
        if destroyed.is_empty() {
            return Ok(());
        }
        emit_destroys_with_effect_damage(st, &destroyed, &hits, &sources_lki)?;
    }
}

/// `st.version` as Python's `_rules_checked` holds it.
fn current_version(st: &GameState) -> Result<i64, EngineError> {
    i64::try_from(st.version).map_err(|_| EngineError::Overflow { what: "version" })
}

/// `_emit_destroys_with_effect_damage(st, destroyed, hits, lki)` (`core.py:895-914`): a card
/// destroyed by rules management after effect damage from an enemy Unit was destroyed with
/// damage by that Unit (rulings ST12-001:Q437, GD05-035:Q361). One group, always.
fn emit_destroys_with_effect_damage(
    st: &mut GameState,
    destroyed: &[Uid],
    hits: &[(Uid, Uid)],
    lki: &Lki,
) -> Result<(), EngineError> {
    let group = to_i32(next_group(st), "event group")?;
    let db = st.db();
    for &u in destroyed {
        let Some(src) = hits.iter().find(|e| e.0 == u).map(|e| e.1) else {
            continue;
        };
        let sc = st.cards[usize::from(src)];
        if sc.owner == st.cards[usize::from(u)].owner {
            continue;
        }
        if !db.by_id(sc.def_id).card_type.is_unit() {
            continue;
        }
        let src_lki = (sc.zone != Zone::Battle).then(|| lki.only(src));
        let ev = if db
            .by_id(st.cards[usize::from(u)].def_id)
            .card_type
            .is_base()
        {
            Ev::DestroysShieldCard
        } else {
            Ev::DestroysByBattle
        };
        emit(
            st,
            ev,
            i32::from(src),
            i32::from(sc.owner),
            src_lki.as_ref(),
            &[
                (EvKey::Target, i32::from(u)),
                (EvKey::Battle, 0),
                (EvKey::Group, group),
            ],
        )?;
    }
    Ok(())
}

/// `_damage_scan(st)` (`core.py:917-937`): (some Unit or Base is damaged or has printed HP 0
/// or less, some Unit or Base has damage reaching its printed HP plus its paired Pilot's).
fn damage_scan(st: &GameState) -> (bool, bool) {
    let db = st.db();
    let (mut damaged, mut lethal) = (false, false);
    for p in 0..2u8 {
        for z in [Zone::Battle, Zone::Base] {
            for &uid in st.zones.get(p, z) {
                let c = st.cards[usize::from(uid)];
                let mut hp = db.by_id(c.def_id).hp;
                if c.damage > 0 || hp <= 0 {
                    damaged = true;
                }
                if z == Zone::Battle && c.pair >= 0 {
                    // A paired Unit's pair is the uid of an existing card.
                    let pilot = st.cards[usize::try_from(c.pair).unwrap_or(0)].def_id;
                    hp += db.by_id(pilot).hp;
                }
                if c.damage >= hp {
                    lethal = true;
                }
                if damaged && lethal {
                    return (true, true);
                }
            }
        }
    }
    (damaged, lethal)
}

/// `over(st)` (`core.py:940-942`): the game has ended.
pub(crate) fn over(st: &GameState) -> bool {
    st.winner.is_some()
}

/// `card_type(st, uid)` (`core.py:945-946`).
pub(crate) fn card_type(st: &GameState, uid: Uid) -> CardType {
    st.def_of(uid).card_type
}

#[cfg(feature = "paranoid")]
mod paranoid {
    //! The shortcut checks of `tests/paranoid.py:44-121` (`CacheChecker.emit`, `.damage_scan`,
    //! `.rules_management`) for the `paranoid` feature (W-49, W-124; docs/port/contracts.md):
    //! each skip is shown to change nothing, and a mismatch panics with the check's name.
    //! Gate diagnostics only, never in a shipped build.

    use super::{GameState, view};
    use crate::{EngineError, Zone};
    use gcg_effects::Ev;

    /// `CacheChecker.emit` (`tests/paranoid.py:86-100`): an event `emit` skips as unlistened
    /// has no trigger candidate.
    pub(super) fn unlistened_event(st: &GameState, ev: Ev) -> Result<(), EngineError> {
        crate::paranoid::count(crate::paranoid::Check::UnlistenedEvent);
        let dv = view::derived(st)?;
        assert!(
            view::trigger_index(st, &dv).get(ev).is_empty(),
            "paranoid: unlistened event {} has candidates, turn {}",
            ev.as_str(),
            st.turn
        );
        Ok(())
    }

    /// `CacheChecker.damage_scan` (`tests/paranoid.py:102-111`): when no damage reaches
    /// printed HP, none reaches the derived HP either.
    pub(super) fn no_printed_lethal_damage(st: &GameState) -> Result<(), EngineError> {
        crate::paranoid::count(crate::paranoid::Check::NoPrintedLethalDamage);
        let dv = view::derived(st)?;
        for p in 0..2u8 {
            for z in [Zone::Battle, Zone::Base] {
                for &uid in st.zones.get(p, z) {
                    assert!(
                        st.cards[usize::from(uid)].damage < view::hp_of(st, &dv, uid),
                        "paranoid: {uid} destroyed by rules, turn {}",
                        st.turn
                    );
                }
            }
        }
        Ok(())
    }

    /// `CacheChecker.rules_management` (`tests/paranoid.py:113-125`): a check skipped because
    /// the state has not changed since the last one changes nothing when run in full.
    pub(super) fn unchanged_state(st: &GameState) -> Result<(), EngineError> {
        crate::paranoid::count(crate::paranoid::Check::UnchangedState);
        let mut full = st.clone();
        full.rules_checked = -1;
        super::rules_management(&mut full)?;
        // Python compares the dicts, which ignores insertion order.
        let hits = |s: &GameState| {
            let mut h = s.effect_hits.clone();
            h.sort();
            h
        };
        assert!(
            (
                full.winner,
                full.version,
                hits(&full),
                full.pending_triggers().len()
            ) == (st.winner, st.version, hits(st), st.pending_triggers().len()),
            "paranoid: rules management skipped a change, turn {}",
            st.turn
        );
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod vectors {
    //! The layer-3 primitive vectors of `port/oracle/export_core_vectors.py`
    //! (`tests/fixtures/core_vectors.ndjson`): a Python call of a `core` or `battle` function
    //! captured in play, with the canonical state before it, its arguments, its result or
    //! exception, and the state after it (a delta of the canonical form, the digest and
    //! `_rules_checked`). [`replay`] rebuilds the state with `canon::from_canonical`, runs the
    //! Rust function and compares everything.

    use std::sync::{Arc, OnceLock};

    use gcg_core::pyjson::{DumpOptions, Json, Key, dumps};
    use gcg_data::CardDb;
    use gcg_effects::{InlineNode, Registry};
    use serde_json::{Value, json};

    use gcg_data::DefId;
    use gcg_effects::{AbilityId, Ev};

    use crate::canon::{canonical_state, from_canonical, state_digest};
    use crate::{
        Battle, DamageOpts, EndReason, EngineError, EvKey, GameState, Lki, MoveOpts, StateSnapshot,
        Uid, Zone,
    };

    const FIXTURE: &str = include_str!("../tests/fixtures/core_vectors.ndjson");
    const IR: &str = include_str!("../../gcg-effects/ir/registry.json");

    /// Every vector of the fixture, in file order.
    pub(crate) fn all() -> &'static [Value] {
        static ALL: OnceLock<Vec<Value>> = OnceLock::new();
        ALL.get_or_init(|| {
            FIXTURE
                .lines()
                .map(|l| serde_json::from_str(l).expect("a JSON vector"))
                .collect()
        })
    }

    /// The vectors of one function (`core.move`, `battle.damage_step`, ...).
    pub(crate) fn of(fun: &str) -> Vec<&'static Value> {
        all().iter().filter(|v| v["fn"] == fun).collect()
    }

    /// The registry of the fixture's vectors ([`registry_for`]).
    pub(crate) fn registry() -> &'static Arc<Registry> {
        static REG: OnceLock<Arc<Registry>> = OnceLock::new();
        REG.get_or_init(|| registry_for(all()))
    }

    /// The embedded registry plus any filter set a state of `vs` names that the IR lacks
    /// (Python registers filter tuples at run time, `effects/registry.py:132-138`).
    pub(crate) fn registry_for(vs: &[Value]) -> Arc<Registry> {
        let base = Registry::embedded();
        let named = vs.iter().any(|v| {
            v["before"]["filter_sets"]
                .as_object()
                .is_some_and(|m| !m.is_empty())
        });
        if !named {
            return base;
        }
        let mut ir: Value = serde_json::from_str(IR).expect("the IR");
        let mut sets = ir["filter_sets"].as_array().expect("filter_sets").clone();
        for v in vs {
            let named = v["before"]["filter_sets"].as_object().expect("an object");
            for (cid, filters) in named {
                if sets.iter().any(|x| x["content_id"] == *cid) {
                    continue;
                }
                let ids: Vec<Value> = filters
                    .as_array()
                    .expect("a filter list")
                    .iter()
                    .map(|f| match base.decode_inline(f) {
                        Ok(InlineNode::Arena(id)) => Value::from(id.0),
                        other => panic!("filter {f} is not an IR node: {other:?}"),
                    })
                    .collect();
                sets.push(json!({"filters": ids, "content_id": cid}));
            }
        }
        ir["filter_sets"] = Value::Array(sets);
        Arc::new(Registry::from_ir(&ir.to_string(), CardDb::embedded()).expect("loads"))
    }

    /// The state before the call, over `reg`.
    pub(crate) fn before(reg: &Arc<Registry>, v: &Value) -> GameState {
        let b = &v["before"];
        let snap = StateSnapshot {
            state: b["state"].clone(),
            continuous: b["continuous"].clone(),
            filter_sets: b["filter_sets"].clone(),
            rules_checked: b["rules_checked"].as_i64().expect("rules_checked"),
        };
        from_canonical(reg, &snap).unwrap_or_else(|e| panic!("{}: {e}", v["id"]))
    }

    /// The canonical state after the call: the delta applied to the state before.
    fn expected_after(v: &Value) -> Value {
        let mut st = v["before"]["state"].clone();
        for (k, change) in v["after"]["delta"].as_object().expect("a delta") {
            if let Some(full) = change.get("value") {
                st[k.as_str()] = full.clone();
            } else {
                for item in change["set"].as_array().expect("set items") {
                    let i = usize::try_from(item[0].as_u64().expect("an index")).expect("index");
                    st[k.as_str()][i] = item[1].clone();
                }
            }
        }
        st
    }

    /// A trace value as a `pyjson` value (every number in the canonical form is an int; the
    /// seed and the RNG state are u64).
    pub(crate) fn to_json(v: &Value) -> Json {
        match v {
            Value::Null => Json::Null,
            Value::Bool(b) => Json::Bool(*b),
            Value::Number(n) => Json::Int(
                n.as_i64()
                    .map(i128::from)
                    .or_else(|| n.as_u64().map(i128::from))
                    .expect("an int"),
            ),
            Value::String(s) => Json::Str(s.clone()),
            Value::Array(a) => Json::Array(a.iter().map(to_json).collect()),
            Value::Object(m) => Json::Object(
                m.iter()
                    .map(|(k, x)| (Key::Str(k.clone()), to_json(x)))
                    .collect(),
            ),
        }
    }

    fn text(j: &Json) -> String {
        dumps(j, DumpOptions::COMPACT_SORTED_ASCII)
    }

    /// The differences between the Rust call and the vector: Python's result (JSON) or
    /// exception class, the canonical state after (key by key, then the digest) and
    /// `_rules_checked`.
    pub(crate) fn check(
        v: &Value,
        st: &GameState,
        outcome: Result<Value, EngineError>,
    ) -> Vec<String> {
        let mut diffs = Vec::new();
        match (&v["error"], outcome) {
            (Value::Null, Ok(got)) => {
                if text(&to_json(&got)) != text(&to_json(&v["result"])) {
                    diffs.push(format!("result: Rust {got}, Python {}", v["result"]));
                }
            }
            (Value::Null, Err(e)) => diffs.push(format!("Rust raised {}: {e}", e.py_class())),
            (err, Ok(got)) => diffs.push(format!("Python raised {err}, Rust returned {got}")),
            (err, Err(e)) => {
                if err["type"] != e.py_class() {
                    diffs.push(format!("Python raised {err}, Rust {}: {e}", e.py_class()));
                }
            }
        }
        let want = expected_after(v);
        let got = canonical_state(st);
        for (k, w) in want.as_object().expect("an object") {
            let g = got.get(k).map(text).unwrap_or_default();
            let w = text(&to_json(w));
            if g != w {
                diffs.push(format!("{k}: Rust {g}\n    Python {w}"));
            }
        }
        let digest = state_digest(st);
        if digest != v["after"]["digest"].as_str().expect("a digest") {
            diffs.push(format!(
                "digest: Rust {digest}, Python {}",
                v["after"]["digest"]
            ));
        }
        let rc = v["after"]["rules_checked"].as_i64().expect("rules_checked");
        if st.rules_checked() != rc {
            diffs.push(format!(
                "rules_checked: Rust {}, Python {rc}",
                st.rules_checked()
            ));
        }
        diffs
    }

    /// Runs every vector of `fun` through [`dispatch`] on the state before it and requires
    /// Python's outcome; panics with every difference of every failing vector. Returns the
    /// vector count.
    pub(crate) fn replay(fun: &str) -> usize {
        let vs = of(fun);
        let failures = failures(registry(), &vs);
        assert!(
            failures.is_empty(),
            "{} of {} {fun} vectors differ:\n{}",
            failures.len(),
            vs.len(),
            failures.join("\n")
        );
        vs.len()
    }

    /// The report of every vector of `vs` whose replay over `reg` differs from Python.
    pub(crate) fn failures(reg: &Arc<Registry>, vs: &[&Value]) -> Vec<String> {
        let mut out = Vec::new();
        for v in vs {
            let fun = v["fn"].as_str().expect("a function name");
            let mut st = before(reg, v);
            let outcome = dispatch(fun, &mut st, &v["args"]);
            let diffs = check(v, &st, outcome);
            if !diffs.is_empty() {
                out.push(format!("{} ({fun}):\n  {}", v["id"], diffs.join("\n  ")));
            }
        }
        out
    }

    /// The Rust port of the Python function `fun` (`core.*`, `battle.*`) called with the
    /// vector's arguments; the result in the exporter's JSON encoding.
    ///
    /// # Panics
    /// On a function the exporter does not record.
    pub(crate) fn dispatch(fun: &str, st: &mut GameState, a: &Value) -> Result<Value, EngineError> {
        use crate::{battle, core};
        let null = |r: Result<(), EngineError>| r.map(|()| Value::Null);
        match fun {
            "core.move" => {
                let o = MoveOpts {
                    bottom: flag(a, "bottom"),
                    reveal: flag(a, "reveal"),
                    rested: flag(a, "rested"),
                };
                null(core::move_card(st, uid(a, "uid"), zone(a, "dst"), o))
            }
            "core.remove_from_game" => null(core::remove_from_game(st, uid(a, "uid"))),
            "core.new_card" => {
                let def = DefId(u16::try_from(int(a, "def_id")).expect("a def_id"));
                core::new_card(
                    st,
                    def,
                    player(a, "owner"),
                    zone(a, "zone"),
                    flag(a, "rested"),
                )
                .map(Value::from)
            }
            "core.shuffle_deck" => {
                core::shuffle_deck(st, player(a, "player"));
                Ok(Value::Null)
            }
            "core.emit" => {
                let ev = Ev::from_value(a["ev"].as_str().expect("an event")).expect("an Ev");
                let lki = lki_of(&a["lki"]);
                let extra: Vec<(EvKey, i32)> = a["extra"]
                    .as_object()
                    .expect("extra")
                    .iter()
                    .map(|(k, v)| {
                        let key = EvKey::from_value(k).expect("an event key");
                        let v = i32::try_from(v.as_i64().expect("an int")).expect("an i32");
                        (key, v)
                    })
                    .collect();
                null(core::emit(
                    st,
                    ev,
                    int(a, "subject"),
                    int(a, "player"),
                    lki.as_ref(),
                    &extra,
                ))
            }
            "core.ability_snapshot" => {
                core::ability_snapshot(st, &uids(a, "uids")).map(|l| lki_json(&l))
            }
            "core.damage_card" => {
                core::damage_card(st, uid(a, "uid"), int64(a, "amount"), damage_opts(a))
                    .map(Value::from)
            }
            "core.resolve_damage" => {
                core::resolve_damage(st, uid(a, "uid"), int64(a, "amount"), damage_opts(a))
                    .map(|(u, x)| Value::from(vec![Value::from(u), Value::from(x)]))
            }
            "core.apply_damage" => {
                core::apply_damage(st, uid(a, "uid"), int64(a, "amount"), damage_opts(a))
                    .map(Value::from)
            }
            "core.shields_receiving_damage" => {
                let us = uids(a, "uids");
                core::shields_receiving_damage(st, &us, int64(a, "amount"), damage_opts(a))
                    .map(|u| uid_list(&u))
            }
            "core.destroy_shields" => null(core::destroy_shields(
                st,
                player(a, "player"),
                &uids(a, "uids"),
                damage_opts(a),
            )),
            "core.damage_shield_area" => {
                let cards = usize::try_from(int(a, "cards")).expect("a count");
                let (p, amount) = (player(a, "player"), int64(a, "amount"));
                null(core::damage_shield_area(
                    st,
                    p,
                    amount,
                    cards,
                    damage_opts(a),
                ))
            }
            "core.destroy" => core::destroy(
                st,
                &uids(a, "uids"),
                flag(a, "battle"),
                int(a, "by"),
                int(a, "source"),
            )
            .map(|u| uid_list(&u)),
            "core.set_winner" => {
                let losers: Vec<u8> = uids(a, "losers")
                    .iter()
                    .map(|&p| u8::try_from(p).expect("a player"))
                    .collect();
                let reason = EndReason::from_value(a["reason"].as_str().expect("a reason"))
                    .expect("an EndReason");
                core::set_winner(st, &losers, reason);
                Ok(Value::Null)
            }
            "core.rules_management" => null(core::rules_management(st)),
            "core.shield_area_protected" => {
                let d = damage_opts(a);
                core::shield_area_protected(st, player(a, "player"), d.source, d.battle, d.by)
                    .map(Value::from)
            }
            "core.shield_area_reduction" => {
                let d = damage_opts(a);
                core::shield_area_reduction(st, player(a, "player"), d.source, d.battle, d.by)
                    .map(Value::from)
            }
            "battle.declare_attack" => null(battle::declare_attack(
                st,
                uid(a, "attacker"),
                int(a, "target"),
            )),
            "battle.battle_broken" => {
                battle::battle_broken(st, &battle_of(&a["b"])).map(Value::from)
            }
            "battle.eligible_blockers" => {
                battle::eligible_blockers(st, &battle_of(&a["b"])).map(|u| uid_list(&u))
            }
            "battle.block" => null(battle::block(st, uid(a, "blocker"))),
            "battle.damage_step" => null(battle::damage_step(st)),
            "battle.end_battle" => null(battle::end_battle(st)),
            "battle.damage_only_battle" => null(battle::damage_only_battle(
                st,
                uid(a, "attacker"),
                uid(a, "target"),
            )),
            other => panic!("no Rust dispatch for {other}"),
        }
    }

    // Argument readers (the exporter's encoding: enums by value, LKI as [[uid, seq, [aid]]]).

    pub(crate) fn int(a: &Value, k: &str) -> i32 {
        i32::try_from(int64(a, k)).expect("an i32")
    }

    pub(crate) fn int64(a: &Value, k: &str) -> i64 {
        a[k].as_i64().unwrap_or_else(|| panic!("{k}: {a}"))
    }

    pub(crate) fn flag(a: &Value, k: &str) -> bool {
        a[k].as_bool().unwrap_or_else(|| panic!("{k}: {a}"))
    }

    pub(crate) fn uid(a: &Value, k: &str) -> Uid {
        Uid::try_from(int(a, k)).expect("a uid")
    }

    pub(crate) fn player(a: &Value, k: &str) -> u8 {
        u8::try_from(int(a, k)).expect("a player")
    }

    fn zone(a: &Value, k: &str) -> Zone {
        Zone::from_value(int64(a, k)).expect("a zone")
    }

    pub(crate) fn uids(a: &Value, k: &str) -> Vec<Uid> {
        a[k].as_array()
            .unwrap_or_else(|| panic!("{k}: {a}"))
            .iter()
            .map(|x| Uid::try_from(x.as_i64().expect("a uid")).expect("a uid"))
            .collect()
    }

    pub(crate) fn uid_list(xs: &[Uid]) -> Value {
        Value::from(xs.iter().map(|&u| Value::from(u)).collect::<Vec<_>>())
    }

    pub(crate) fn damage_opts(a: &Value) -> DamageOpts {
        DamageOpts {
            source: int(a, "source"),
            battle: flag(a, "battle"),
            by: int(a, "by"),
        }
    }

    /// `[[uid, seq, [aid, ...]], ...]` as an LKI; `None` for `null`.
    fn lki_of(v: &Value) -> Option<Lki> {
        let entries = v.as_array()?;
        let mut out = Lki::default();
        for e in entries {
            let u = Uid::try_from(e[0].as_i64().expect("a uid")).expect("a uid");
            let seq = i32::try_from(e[1].as_i64().expect("a seq")).expect("a seq");
            let abilities: Vec<AbilityId> = e[2]
                .as_array()
                .expect("aids")
                .iter()
                .map(|a| AbilityId(u16::try_from(a.as_u64().expect("an aid")).expect("an aid")))
                .collect();
            out.insert(u, &abilities, seq).expect("fits");
        }
        Some(out)
    }

    fn lki_json(l: &Lki) -> Value {
        Value::from(
            l.entries
                .iter()
                .map(|e| {
                    let aids: Vec<Value> = l
                        .abilities_of(e.uid)
                        .iter()
                        .map(|a| Value::from(a.0))
                        .collect();
                    Value::from(vec![
                        Value::from(e.uid),
                        Value::from(e.seq),
                        Value::from(aids),
                    ])
                })
                .collect::<Vec<_>>(),
        )
    }

    /// `Battle.to_json()` (`engine/state.py:173-185`) back to a battle.
    fn battle_of(v: &Value) -> Battle {
        let x = |i: usize| i32::try_from(v[i].as_i64().expect("an int")).expect("an i32");
        Battle {
            attacker: x(0),
            attacker_seq: x(1),
            target: x(2),
            target_seq: x(3),
            defender: u8::try_from(x(4)).expect("a player"),
            battle_id: x(5),
            blocked: x(6) != 0,
            damage_only: x(7) != 0,
            attack_triggers: x(8) != 0,
            ended: x(9) != 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::vectors;
    use super::*;

    #[test]
    fn move_vectors_match_python() {
        assert!(vectors::replay("core.move") > 0);
    }

    #[test]
    fn primitive_vectors_match_python() {
        for fun in [
            "core.remove_from_game",
            "core.new_card",
            "core.shuffle_deck",
            "core.ability_snapshot",
            "core.set_winner",
        ] {
            vectors::replay(fun);
        }
    }

    #[test]
    fn emit_vectors_match_python() {
        assert!(vectors::replay("core.emit") > 0);
    }

    #[test]
    fn damage_vectors_match_python() {
        let n: usize = [
            "core.damage_card",
            "core.resolve_damage",
            "core.apply_damage",
            "core.shields_receiving_damage",
            "core.destroy_shields",
            "core.damage_shield_area",
            "core.shield_area_protected",
            "core.shield_area_reduction",
        ]
        .into_iter()
        .map(vectors::replay)
        .sum();
        assert!(n > 0);
    }

    #[test]
    fn destroy_and_rules_management_vectors_match_python() {
        assert!(vectors::replay("core.destroy") > 0);
        assert!(vectors::replay("core.rules_management") > 0);
    }

    /// Every vector of the fixture, whatever its function (a new exporter target needs a
    /// dispatch arm, never a silent skip).
    #[test]
    fn all_vectors_match_python() {
        let vs: Vec<&serde_json::Value> = vectors::all().iter().collect();
        let failures = vectors::failures(vectors::registry(), &vs);
        assert!(
            failures.is_empty(),
            "{} of {} vectors differ:\n{}",
            failures.len(),
            vs.len(),
            failures.join("\n")
        );
        assert!(vs.len() >= 100);
    }

    /// W-143 data tripwire: the cards whose effect IR grants REDIRECT_BATTLE_DAMAGE (Python
    /// lists the same two: `"REDIRECT_BATTLE_DAMAGE" in repr(ability)` over `R.cards`). A new
    /// redirect source can build a new chain or cycle, so it fails here until reviewed.
    #[test]
    fn redirect_sources_are_reviewed() {
        let reg = Registry::embedded();
        let found: Vec<&str> = reg
            .db()
            .defs()
            .iter()
            .filter(|def| {
                let e = reg.card(def.def_id);
                e.own.iter().chain(e.unit.iter()).any(|&aid| {
                    reg.repr(reg.ability(aid).ability.node())
                        .contains("RuleKind.REDIRECT_BATTLE_DAMAGE")
                })
            })
            .map(|def| &*def.card_number)
            .collect();
        assert_eq!(
            found,
            ["GD04-087", "GD04-095"],
            "a new battle-damage redirect source: review it for redirect cycles (W-143)"
        );
    }

    #[test]
    fn python_list_indexing() {
        assert_eq!(py_index(3, 0), Ok(0));
        assert_eq!(py_index(3, 2), Ok(2));
        assert_eq!(py_index(3, -1), Ok(2));
        assert_eq!(py_index(3, -3), Ok(0));
        for bad in [3, 4, -4, i32::MIN] {
            assert_eq!(
                py_index(3, bad),
                Err(EngineError::Index("list index out of range".to_owned()))
            );
        }
        assert!(py_index(0, -1).is_err());
    }

    /// W-143: Python recurses once per redirect hop (`core.py:591`) and, on a cycle, until
    /// `RecursionError`; the chain walk names the cycle and lands where Python does otherwise.
    #[test]
    fn redirect_chains_land_and_cycles_fail() {
        fn graph(
            edges: &'static [(Uid, Uid)],
        ) -> impl FnMut(Uid) -> Result<Option<Uid>, EngineError> {
            move |u| Ok(edges.iter().find(|e| e.0 == u).map(|e| e.1))
        }
        assert_eq!(follow_redirects(1, graph(&[])), Ok(1));
        assert_eq!(follow_redirects(1, graph(&[(1, 2)])), Ok(2));
        assert_eq!(follow_redirects(1, graph(&[(1, 2), (2, 3)])), Ok(3));
        assert_eq!(follow_redirects(2, graph(&[(1, 2), (2, 3)])), Ok(3));
        let cycle = follow_redirects(1, graph(&[(1, 2), (2, 1)])).unwrap_err();
        assert_eq!(
            cycle,
            EngineError::RedirectCycle {
                uids: Box::new([1, 2, 1])
            }
        );
        assert_eq!(cycle.py_class(), "RecursionError");
        assert_eq!(
            cycle.to_string(),
            "battle damage redirect cycle through units [1, 2, 1]"
        );
        assert_eq!(
            follow_redirects(1, graph(&[(1, 2), (2, 3), (3, 2)])),
            Err(EngineError::RedirectCycle {
                uids: Box::new([1, 2, 3, 2])
            })
        );
        let failing = |_: Uid| Err(EngineError::Internal("view".to_owned()));
        assert_eq!(
            follow_redirects(5, failing),
            Err(EngineError::Internal("view".to_owned()))
        );
    }

    /// `lki[uid] = snapshot` keeps the first position and the last value; `only` is the
    /// one-entry sub-dict (`core.py:910`, `battle.py:117`, `interp.py:776`).
    #[test]
    fn lki_has_dict_semantics() {
        let mut l = Lki::default();
        assert!(l.is_empty());
        l.insert(7, &[AbilityId(3), AbilityId(4)], 2).unwrap();
        l.insert(5, &[], 1).unwrap();
        l.insert(7, &[AbilityId(9)], 6).unwrap();
        assert_eq!(l.entries.iter().map(|e| e.uid).collect::<Vec<_>>(), [7, 5]);
        assert_eq!(l.get(7).map(|e| e.seq), Some(6));
        assert_eq!(l.abilities_of(7), [AbilityId(9)]);
        assert_eq!(l.abilities_of(5), []);
        assert_eq!(l.abilities_of(8), []);
        assert!(l.contains(5) && !l.contains(8));
        let one = l.only(7);
        assert_eq!(one.entries.len(), 1);
        assert_eq!(one.abilities_of(7), [AbilityId(9)]);
        assert_eq!(one.get(7).map(|e| e.seq), Some(6));
        assert!(l.only(8).is_empty());
    }

    /// The event tuple is `subject, player, group, *sorted(extra)` (`core.py:328-333`), e.g.
    /// Inv 06a §5.2's DESTROYED; a popped `group` of -1 is the current group.
    #[test]
    fn event_payload_layout() {
        let (_, st) = crate::canon::fixtures::states().swap_remove(0);
        let g = i32::try_from(st.event_group).unwrap();
        let ev = event_payload(
            &st,
            121,
            0,
            &[
                (EvKey::Source, -1),
                (EvKey::Pilot, -1),
                (EvKey::Linked, 0),
                (EvKey::By, -1),
                (EvKey::Battle, 1),
                (EvKey::Group, 21),
            ],
        )
        .unwrap();
        assert_eq!(
            ev.pairs(),
            [
                (EvKey::Subject, 121),
                (EvKey::Player, 0),
                (EvKey::Group, 21),
                (EvKey::Battle, 1),
                (EvKey::By, -1),
                (EvKey::Linked, 0),
                (EvKey::Pilot, -1),
                (EvKey::Source, -1),
            ]
        );
        let cur = event_payload(&st, -1, 1, &[(EvKey::Group, NO_ARG)]).unwrap();
        assert_eq!(
            cur.pairs(),
            [(EvKey::Subject, -1), (EvKey::Player, 1), (EvKey::Group, g)]
        );
    }

    #[test]
    fn set_winner_draws_on_two_losers_and_keeps_the_first_result() {
        let (_, mut st) = crate::canon::fixtures::states().swap_remove(0);
        set_winner(&mut st, &[1, 0], EndReason::DeckOut);
        assert_eq!(
            (st.winner, st.end_reason, st.phase, st.step),
            (
                Some(-1),
                Some(EndReason::BothDefeated),
                Phase::GameOver,
                Step::GameOver
            )
        );
        assert!(st.pending.is_none() && st.pending_options.is_empty());
        set_winner(&mut st, &[0], EndReason::Concede);
        assert_eq!(st.winner, Some(-1));
        let (_, mut st) = crate::canon::fixtures::states().swap_remove(0);
        set_winner(&mut st, &[0, 0], EndReason::Concede);
        assert_eq!(
            (st.winner, st.end_reason),
            (Some(1), Some(EndReason::Concede))
        );
    }
}
