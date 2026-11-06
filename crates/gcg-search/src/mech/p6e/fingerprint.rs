//! Uid-independent typed merge fingerprints (`typed_fingerprint`, W-10; docs/dev/PLAN.md §12.3
//! S6).
//!
//! The reference's `merge_equivalent` (`ai/actions.py:280-393`, reproduced in
//! `gcg_ai::fingerprint`) compares raw numbers in two places: a card is "in flight" when *any*
//! value of a resolving frame's or a waiting trigger's event equals its uid (`_in_flight`,
//! `:284-295`), and its once-per-turn references are every once key holding an int equal to
//! its uid, with each such int rewritten to "self" (`_references`, `:304-310`). An event amount,
//! a zone sequence number or a turn that merely equals a card's uid therefore keeps two
//! interchangeable copies apart, or rewrites an unrelated number.
//!
//! Here only *typed card references* count: the event keys that name a card (`attacker`,
//! `blocker`, `pilot`, `source`, `subject`, `target`) and the card positions of each once-key
//! layout (`gcg_engine::OnceKey`: `(aid | tag, host, zone_seq)`, `(-8 | -7, aid, host, uid,
//! zone_seq)`, `(-8 | -7, -1, effect, serial, uid, zone_seq)`, `(-9, uid, turn)`); a key of
//! another layout falls back to the reference's raw comparison. History entries, lasting
//! effects, battles, frame buffers and chosen variables already hold typed uids and are compared
//! as the reference does. The rest of the fingerprint is the reference's.

use gcg_ai::{MoveKey, uid_argument};
use gcg_effects::{AbilityId, ContRef, Duration, FilterSetId, KwAmounts};
use gcg_engine::view::{
    abilities_of, ap_of, derived, hp_of, is_linked, keywords, rules_all, traits_present,
};
use gcg_engine::{
    Action, ActionKind, DecisionView, Derived, EngineError, EvKey, EventPayload, GameState,
    HistKind, OnceElem, OnceKey, Uid, Zone,
};

/// `_FIELD` (`ai/actions.py:280`).
const FIELD: [Zone; 2] = [Zone::Battle, Zone::Base];

/// The event payload keys whose value is a card uid.
const CARD_KEYS: [EvKey; 6] = [
    EvKey::Attacker,
    EvKey::Blocker,
    EvKey::Pilot,
    EvKey::Source,
    EvKey::Subject,
    EvKey::Target,
];

/// `uid` as an index of `st.cards`.
fn card_index(st: &GameState, uid: i32) -> Option<Uid> {
    let u = usize::try_from(uid).ok()?;
    if u < st.cards().len() {
        Uid::try_from(u).ok()
    } else {
        None
    }
}

/// Whether an event payload names `uid` under a card key.
fn event_names(ev: &EventPayload, uid: i32) -> bool {
    ev.0.as_slice()
        .iter()
        .any(|&(k, v)| v == uid && CARD_KEYS.contains(&k))
}

/// `_in_flight(st, uid)` (`ai/actions.py:284-295`) with typed event references.
fn in_flight(st: &GameState, uid: i32) -> bool {
    if st
        .battles()
        .iter()
        .any(|b| uid == b.attacker || uid == b.target)
    {
        return true;
    }
    for f in st.frames() {
        if uid == f.host
            || uid == f.card_uid
            || st.frame_buf(f).contains(&uid)
            || event_names(&f.event, uid)
        {
            return true;
        }
        if st
            .frame_vars(f)
            .iter()
            .any(|(_, chosen)| chosen.contains(&uid))
        {
            return true;
        }
    }
    let named = |t: &gcg_engine::TriggerInst| {
        uid == t.host || uid == t.card_uid || event_names(&t.event, uid)
    };
    st.pending_triggers().iter().any(named) || st.batches().any(|b| b.iter().any(named))
}

/// An element of a typed once reference: a card position equal to the card is "self".
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum OnceRef {
    Int(i32),
    Cont(ContRef),
    SelfRef,
}

/// The positions of a once key that hold card uids, by its layout; `None` for a layout this
/// table does not know (compared raw, as the reference does).
fn card_positions(key: &[OnceElem]) -> Option<&'static [usize]> {
    match key {
        [
            OnceElem::Int(-8 | -7),
            OnceElem::Int(-1),
            OnceElem::Cont(_),
            _,
            _,
            _,
        ] => Some(&[4]),
        [OnceElem::Int(-8 | -7), _, _, _, _] => Some(&[2, 3]),
        [OnceElem::Int(-9), _, _] | [OnceElem::Int(_), _, _] => Some(&[1]),
        _ => None,
    }
}

/// The typed once reference of `key` to the card `uid`, or `None` when the key does not name
/// the card.
fn once_ref(key: &OnceKey, uid: i32) -> Option<Vec<OnceRef>> {
    let elems = key.0.as_slice();
    let positions = card_positions(elems);
    let is_card = |i: usize, x: i32| x == uid && positions.is_none_or(|p| p.contains(&i));
    if !elems
        .iter()
        .enumerate()
        .any(|(i, e)| matches!(*e, OnceElem::Int(x) if is_card(i, x)))
    {
        return None;
    }
    Some(
        elems
            .iter()
            .enumerate()
            .map(|(i, e)| match *e {
                OnceElem::Int(x) if is_card(i, x) => OnceRef::SelfRef,
                OnceElem::Int(x) => OnceRef::Int(x),
                OnceElem::Cont(c) => OnceRef::Cont(c),
            })
            .collect(),
    )
}

/// One element of the `lasting` part of `_references` (`ai/actions.py:312-330`).
type LastingRef = (
    ContRef,
    u8,
    Duration,
    i32,
    i32,
    i8,
    i32,
    Option<FilterSetId>,
    i32,
    bool,
    bool,
);

/// `_references(st, uid)` (`ai/actions.py:298-331`) with typed once references.
#[derive(Clone, PartialEq, Eq, Debug)]
struct References {
    history: Vec<(HistKind, u8, i8, bool, bool)>,
    once: Vec<Vec<OnceRef>>,
    lasting: Vec<LastingRef>,
}

fn references(st: &GameState, uid: Uid) -> References {
    let u = i32::from(uid);
    let history = st
        .history()
        .iter()
        .filter(|e| u == e.uid || u == e.source)
        .map(|e| (e.kind, e.player, e.by, e.uid == u, e.source == u))
        .collect();
    let mut once: Vec<Vec<OnceRef>> = st
        .once_used()
        .iter()
        .filter_map(|k| once_ref(k, u))
        .collect();
    once.sort();
    let seq = st.card(uid).zone_seq;
    let mut lasting: Vec<LastingRef> = st
        .lasting()
        .iter()
        .chain(st.delayed())
        .filter_map(|le| {
            let targeted = st.lasting_targets(le).contains(&(uid, seq));
            let sourced = le.source_uid == u;
            (targeted || sourced).then_some((
                le.effect,
                le.controller,
                le.duration,
                le.created_turn,
                le.battle_id,
                le.player,
                le.uses,
                le.filters,
                le.aux,
                targeted,
                sourced,
            ))
        })
        .collect();
    lasting.sort();
    References {
        history,
        once,
        lasting,
    }
}

/// The `"field"` tuple of `_fingerprint` (`ai/actions.py:346-363`).
#[derive(Clone, PartialEq, Eq, Debug)]
struct FieldFp<'r> {
    owner: u8,
    def_id: u16,
    zone: Zone,
    rested: bool,
    damage: i32,
    entered_this_turn: bool,
    ap: i32,
    hp: i32,
    kw: KwAmounts,
    traits: Vec<&'r str>,
    linked: bool,
    abilities: Vec<AbilityId>,
    rules: Vec<(&'r str, u8, bool)>,
    references: References,
    paired: Option<(u16, References)>,
}

/// `_fingerprint(st, dv, uid)` (`ai/actions.py:334-363`).
#[derive(Clone, PartialEq, Eq, Debug)]
enum Fingerprint<'r> {
    Arg(i32),
    Card(i32),
    Field(Box<FieldFp<'r>>),
}

fn fingerprint<'r>(st: &'r GameState, dv: &'r Derived, uid: i32) -> Fingerprint<'r> {
    let Some(u) = card_index(st, uid) else {
        return Fingerprint::Arg(uid);
    };
    let c = st.card(u);
    let pilot = c.pair;
    if !FIELD.contains(&c.zone) || in_flight(st, uid) || (pilot >= 0 && in_flight(st, pilot)) {
        return Fingerprint::Card(uid);
    }
    let reg = st.registry();
    let paired = card_index(st, pilot).map(|p| (st.card(p).def_id.0, references(st, p)));
    let mut traits: Vec<&str> =
        traits_present(dv, u).map_or_else(Vec::new, |t| t.iter(reg).collect());
    traits.sort();
    let mut abilities: Vec<AbilityId> = abilities_of(dv, u).to_vec();
    abilities.sort();
    let mut rules: Vec<(&str, u8, bool)> = rules_all(dv, u)
        .iter()
        .map(|r| (reg.rule_mod_repr(r.rule), r.controller, r.source == uid))
        .collect();
    rules.sort();
    Fingerprint::Field(Box::new(FieldFp {
        owner: c.owner,
        def_id: c.def_id.0,
        zone: c.zone,
        rested: c.rested,
        damage: c.damage,
        entered_this_turn: c.entered_turn == st.turn(),
        ap: ap_of(st, dv, u),
        hp: hp_of(st, dv, u),
        kw: keywords(dv, u),
        traits,
        linked: is_linked(dv, u),
        abilities,
        rules,
        references: references(st, u),
        paired,
    }))
}

/// `_public_move(st, dv, dec, key, action)` (`ai/actions.py:366-376`).
#[derive(Clone, PartialEq, Eq, Debug)]
enum PublicMove<'r> {
    Attack(Fingerprint<'r>, Fingerprint<'r>),
    Block(Fingerprint<'r>, i32),
    Select(Fingerprint<'r>, i32),
    Pair(i64, Fingerprint<'r>, i32),
    Key(MoveKey),
}

fn public_move<'r>(
    st: &'r GameState,
    dv: &'r Derived,
    dec: &DecisionView<'_>,
    key: MoveKey,
    a: Action,
) -> PublicMove<'r> {
    match a.kind {
        ActionKind::Attack => {
            PublicMove::Attack(fingerprint(st, dv, a.a), fingerprint(st, dv, a.b))
        }
        ActionKind::Block => PublicMove::Block(fingerprint(st, dv, a.a), a.b),
        ActionKind::Select if uid_argument(dec) => {
            PublicMove::Select(fingerprint(st, dv, a.a), a.b)
        }
        ActionKind::Pair => PublicMove::Pair(key.a, fingerprint(st, dv, a.b), key.variant),
        _ => PublicMove::Key(key),
    }
}

/// `merge_equivalent(st, dec, moves)` (`ai/actions.py:379-393`) over the typed fingerprints:
/// the first move of every group whose public forms are equal, in order.
///
/// # Errors
/// The engine's error from `view::derived(st)`.
pub(crate) fn merge_equivalent(
    st: &GameState,
    dec: &DecisionView<'_>,
    moves: Vec<(MoveKey, Action)>,
) -> Result<Vec<(MoveKey, Action)>, EngineError> {
    let dv = derived(st)?;
    let mut seen: Vec<PublicMove<'_>> = Vec::with_capacity(moves.len());
    let mut out = Vec::with_capacity(moves.len());
    for (key, action) in moves {
        let public = public_move(st, &dv, dec, key, action);
        if !seen.contains(&public) {
            seen.push(public);
            out.push((key, action));
        }
    }
    Ok(out)
}

/// The merged groups of `moves`: for every move, the index of the first move of its group
/// (the property test of W-10 checks each merged pair's successors).
///
/// # Errors
/// The engine's error from `view::derived(st)`.
#[cfg(test)]
pub(crate) fn groups(
    st: &GameState,
    dec: &DecisionView<'_>,
    moves: &[(MoveKey, Action)],
) -> Result<Vec<usize>, EngineError> {
    let dv = derived(st)?;
    let publics: Vec<PublicMove<'_>> = moves
        .iter()
        .map(|&(key, action)| public_move(st, &dv, dec, key, action))
        .collect();
    Ok(publics
        .iter()
        .map(|p| publics.iter().position(|q| q == p).expect("itself"))
        .collect())
}
