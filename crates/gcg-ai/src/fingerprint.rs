//! Public fingerprints of field cards and the root merge of interchangeable moves
//! (`ai/actions.py:280-393`).
//!
//! Every fingerprint is compared for equality only (the `seen` set of `merge_equivalent`), so
//! the `sorted` calls of Python, which make the comparisons order-insensitive, may use any
//! total order whose equality is Python's `==`. Once keys keep Python's element
//! positions (`OnceKey::with_self`); the effect of a lasting rule key is a `ContRef` that never
//! equals a uid, where Python compares its process-local number (PLAN D11, §8.9).

use gcg_effects::{AbilityId, ContRef, Duration, FilterSetId, KwAmounts};
use gcg_engine::view::{
    abilities_of, ap_of, derived, hp_of, is_linked, keywords, rules_all, traits_present,
};
use gcg_engine::{
    Action, ActionKind, DecisionView, Derived, EngineError, FpOnceKey, GameState, HistKind, Uid,
    Zone,
};

use crate::MoveKey;
use crate::actions::card_index;
use crate::agent::uid_argument;

/// `_FIELD` (`ai/actions.py:280`).
const FIELD: [Zone; 2] = [Zone::Battle, Zone::Base];

/// `_in_flight(st, uid)` (`ai/actions.py:284-295`): a battle, a resolving frame or a waiting
/// trigger names the card (any event value equal to the uid counts).
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
            || f.event.0.as_slice().iter().any(|&(_, v)| v == uid)
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
        uid == t.host || uid == t.card_uid || t.event.0.as_slice().iter().any(|&(_, v)| v == uid)
    };
    st.pending_triggers().iter().any(named) || st.batches().any(|b| b.iter().any(named))
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

/// `_references(st, uid)` (`ai/actions.py:298-331`): `(history, once, lasting)`.
#[derive(Clone, PartialEq, Eq, Debug)]
struct References {
    history: Vec<(HistKind, u8, i8, bool, bool)>,
    once: Vec<FpOnceKey>,
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
    let mut once: Vec<FpOnceKey> = st
        .once_used()
        .iter()
        .filter(|k| k.contains_int(u))
        .map(|k| k.with_self(u))
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
    /// `dv.kw.get(uid, {})`: equal dicts are equal arrays (Python sorts the items).
    kw: KwAmounts,
    traits: Vec<&'r str>,
    linked: bool,
    abilities: Vec<AbilityId>,
    rules: Vec<(&'r str, u8, bool)>,
    references: References,
    /// `(cards[pilot].def_id, _references(pilot))`, or `()` without a pilot.
    paired: Option<(u16, References)>,
}

/// `_fingerprint(st, dv, uid)` (`ai/actions.py:334-363`).
#[derive(Clone, PartialEq, Eq, Debug)]
enum Fingerprint<'r> {
    /// `("arg", uid)`: out of range, e.g. the player target.
    Arg(i32),
    /// `("card", uid)`: off the field or named by a battle, frame or trigger; never merges.
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

/// `_public_move(st, dv, dec, key, action)` (`ai/actions.py:366-376`). The variants carry
/// distinct action kinds and tuple lengths, so two values are equal exactly when Python's
/// tuples are.
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

/// `merge_equivalent(st, dec, moves)` (`ai/actions.py:379-393`): keeps the first move of every
/// group whose public forms are equal, in order. No fallback: the result is never empty for a
/// non-empty input.
///
/// # Errors
/// The engine's error from `view::derived(st)`.
pub fn merge_equivalent(
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
