//! Lasting effects and delayed triggers (`interp.py:1249-1337`, `:1526-1559`): `Apply` with
//! frozen amounts and AP_REDUCED, `add_lasting`, `ApplyPlayer`, and `DelayedTrigger`
//! (Inv 06a §13.4; PLAN §5.6 for the identity of runtime effects).

use gcg_effects::{
    Apply, ApplyPlayer, ContRef, ContinuousId, Duration, Ev, Node, NodeId, RefId, Registry,
    ValueRef,
};

use super::{Status, refs, set_did, to_i32, to_uid, val};
use crate::core::{self as core_};
use crate::view;
use crate::{EngineError, EvKey, GameState, Lasting, LastingOpts, NO_ARG, Span, Uid, Zone};

/// The effect of an `Apply`: an IR node (frozen when it resolves), or a value a custom hook
/// builds at run time (`d.StatMod(ap=-n)`, `wp_gd03_a.py:126`).
#[derive(Clone, Copy, Debug)]
pub(super) enum Effect {
    Ir(ContinuousId),
    StatModValue { ap: i32, hp: i32 },
}

/// `isinstance(eff, d.StatMod) and eff.ap != 0` on the effect as written
/// (`interp.py:1265-1266`): a non-literal amount is never equal to 0 (Inv 06a hazard 14).
fn lowers_ap(reg: &Registry, effect: Effect) -> bool {
    match effect {
        Effect::Ir(id) => match reg.node(id.node()) {
            Node::StatMod(m) => m.ap != ValueRef::Lit(0),
            _ => false,
        },
        Effect::StatModValue { ap, .. } => ap != 0,
    }
}

/// `_frozen(st, f, eff)` (`interp.py:1276-1283`): FAQ Q106, the amounts of a lasting effect are
/// fixed when it resolves. A `StatMod` with a non-literal AP or HP becomes
/// `StatMod(val(ap), val(hp))` (both evaluated, AP first); a `KeywordGrant` with a non-literal
/// amount is evaluated; anything else is unchanged. The result is the canonical `ContRef` that
/// `R.cont_key` interns (PLAN §5.6).
fn frozen(
    st: &GameState,
    reg: &Registry,
    fi: usize,
    effect: Effect,
) -> Result<ContRef, EngineError> {
    let id = match effect {
        Effect::Ir(id) => id,
        Effect::StatModValue { ap, hp } => return Ok(ContRef::StatMod { ap, hp }),
    };
    match reg.node(id.node()) {
        Node::StatMod(m)
            if !(matches!(m.ap, ValueRef::Lit(_)) && matches!(m.hp, ValueRef::Lit(_))) =>
        {
            let ap = to_i32(val(st, fi, m.ap)?, "StatMod ap")?;
            let hp = to_i32(val(st, fi, m.hp)?, "StatMod hp")?;
            Ok(ContRef::StatMod { ap, hp })
        }
        Node::KeywordGrant(g) if !matches!(g.amount, ValueRef::Lit(_)) => {
            Ok(ContRef::KeywordGrant {
                kw: g.keyword,
                amount: to_i32(val(st, fi, g.amount)?, "KeywordGrant amount")?,
            })
        }
        _ => Ok(reg.cont_of(id)),
    }
}

/// `_h_apply` (`interp.py:1249-1273`).
pub(super) fn h_apply(
    st: &mut GameState,
    reg: &Registry,
    fi: usize,
    ins: &Apply,
) -> Result<Status, EngineError> {
    let targets = refs(st, fi, ins.r#ref)?;
    if matches!(reg.node(ins.r#ref.node()), Node::All(_)) {
        crate::diag::note_interpretation(crate::diag::Interpretation::I04);
    }
    apply_to(
        st,
        reg,
        fi,
        &targets,
        ins.aux,
        Effect::Ir(ins.effect),
        ins.duration,
    )
}

/// The body of `_h_apply` after `targets = _refs(st, f, ins.ref)` (`interp.py:1251-1273`), shared
/// with the hook that executes a run-time `Apply` (`wp_gd03_a.py:126`). The live targets keep
/// their `zone_seq`; an `aux` reference that resolves to nothing clears `did`; the AP of every
/// target is read on the view before the effect; `did` is set even when `add_lasting` declines
/// a THIS_BATTLE effect outside a battle; an AP-lowering `StatMod` announces AP_REDUCED (one
/// new group) for every live BATTLE target whose AP dropped on a fresh view.
pub(super) fn apply_to(
    st: &mut GameState,
    reg: &Registry,
    fi: usize,
    targets: &[Uid],
    aux: Option<RefId>,
    effect: Effect,
    duration: Duration,
) -> Result<Status, EngineError> {
    let live: Vec<(Uid, i32)> = targets
        .iter()
        .filter(|&&u| st.cards[usize::from(u)].zone != Zone::Outside)
        .map(|&u| (u, st.cards[usize::from(u)].zone_seq))
        .collect();
    if live.is_empty() {
        set_did(st, fi, false);
        return Ok(Status::Next);
    }
    let mut aux_uid = NO_ARG;
    if let Some(r) = aux {
        let aux_cards = refs(st, fi, r)?;
        let Some(&first) = aux_cards.first() else {
            set_did(st, fi, false);
            return Ok(Status::Next);
        };
        aux_uid = i32::from(first);
    }
    // `before = {u: V.ap_of(st, V.derived(st), u) for u, _ in live}`: nothing changes the state
    // inside the comprehension, so every `derived` call there returns the same view.
    let dv = view::derived(st)?;
    let before: Vec<i32> = live.iter().map(|&(u, _)| view::ap_of(st, &dv, u)).collect();
    drop(dv);
    let eff = frozen(st, reg, fi, effect)?;
    let (controller, host) = (st.frames[fi].controller, st.frames[fi].host);
    add_lasting(
        st,
        eff,
        controller,
        host,
        &live,
        duration,
        LastingOpts {
            player: -1,
            uses: 0,
            filters: None,
            aux: aux_uid,
        },
    )?;
    set_did(st, fi, true);
    if lowers_ap(reg, effect) {
        let dv = view::derived(st)?;
        let group = to_i32(core_::next_group(st), "event group")?;
        for (&(u, _), &ap_before) in live.iter().zip(&before) {
            let c = st.cards[usize::from(u)];
            // `before[u]` of a dict keyed by uid: a repeated uid has the same value everywhere.
            if c.zone == Zone::Battle && view::ap_of(st, &dv, u) < ap_before {
                core_::emit(
                    st,
                    Ev::ApReduced,
                    i32::from(u),
                    i32::from(c.owner),
                    None,
                    &[(EvKey::By, i32::from(controller)), (EvKey::Group, group)],
                )?;
            }
        }
    }
    Ok(Status::Next)
}

/// `add_lasting(st, effect, controller, source, targets, duration, *, player=-1, uses=0,
/// filters=(), aux=-1)` (`interp.py:1286-1320`): rules 8-2-3 and 8-6-1, a THIS_BATTLE effect
/// outside a battle (or after it ended) has no effect; else the effect is appended with the
/// next serial, and the state touched. `effect` is `R.cont_key(effect)`, the canonical
/// `ContRef`; `o.filters` is `R.filters_key(filters)` when there are filters.
///
/// # Errors
/// Arena or serial overflow.
#[allow(
    dead_code,
    reason = "called by layer L5 (game, testkit) and C's tests (docs/port/contracts.md); they land after L4"
)]
pub(crate) fn add_lasting(
    st: &mut GameState,
    effect: ContRef,
    controller: u8,
    source: i32,
    targets: &[(Uid, i32)],
    duration: Duration,
    o: LastingOpts,
) -> Result<(), EngineError> {
    let b = st.battle().copied();
    if duration == Duration::ThisBattle && b.is_none_or(|b| b.ended) {
        return Ok(());
    }
    let serial = st.next_lasting_serial;
    st.lasting_push(
        Lasting {
            effect,
            controller,
            source_uid: source,
            targets: Span::EMPTY,
            duration,
            created_turn: st.turn,
            battle_id: b.map_or(NO_ARG, |b| b.battle_id),
            player: o.player,
            uses: o.uses,
            filters: o.filters,
            aux: o.aux,
            serial,
        },
        targets,
    )?;
    st.next_lasting_serial = serial.checked_add(1).ok_or(EngineError::Overflow {
        what: "lasting serial",
    })?;
    st.touch();
    Ok(())
}

/// `_h_apply_player` (`interp.py:1323-1337`): a player-level effect, not frozen, with the
/// instruction's filters and uses.
pub(super) fn h_apply_player(
    st: &mut GameState,
    reg: &Registry,
    fi: usize,
    ins: &ApplyPlayer,
) -> Result<Status, EngineError> {
    let p = super::player_of(st, &super::ctx_of(st, fi)?, ins.player);
    let filters = if ins.filters.is_empty() {
        None
    } else {
        Some(reg.filters_key(&ins.filters).ok_or_else(|| {
            EngineError::Internal(
                "an ApplyPlayer filter tuple is not one of the registry's filter sets".to_owned(),
            )
        })?)
    };
    let (controller, host) = (st.frames[fi].controller, st.frames[fi].host);
    let player = i8::try_from(p).map_err(|_| EngineError::Overflow {
        what: "lasting player",
    })?;
    let uses = i32::try_from(ins.uses).map_err(|_| EngineError::Overflow {
        what: "lasting uses",
    })?;
    add_lasting(
        st,
        reg.cont_of(ins.effect),
        controller,
        host,
        &[],
        ins.duration,
        LastingOpts {
            player,
            uses,
            filters,
            aux: NO_ARG,
        },
    )?;
    set_did(st, fi, true);
    Ok(Status::Next)
}

/// `_h_delayed` (`interp.py:1526-1559`): the trigger's index among the card's own then unit
/// delayed triggers in `core._walk_delayed` order (the first structurally equal one, which is
/// the first equal `NodeId`: the IR is hash-consed; Inv 06a hazard 16); not found is
/// `EngineError("delayed trigger not registered on its card")`. A THIS_BATTLE trigger outside
/// a battle clears `did`. Else the bound vars' cards (with their `zone_seq`) become the
/// targets of a delayed `Lasting` keyed by `AbilityGrant(card_number, index)`, with Python's
/// defaults for the rest (serial -1); no serial is consumed and nothing is touched.
pub(super) fn h_delayed(
    st: &mut GameState,
    reg: &Registry,
    fi: usize,
    ins: NodeId,
) -> Result<Status, EngineError> {
    let Node::DelayedTrigger(dt) = reg.node(ins) else {
        return Err(EngineError::Internal("not a DelayedTrigger".to_owned()));
    };
    let card_uid = st.frames[fi].card_uid;
    let mut found: Option<(gcg_effects::StrId, i32)> = None;
    if card_uid >= 0 {
        let def = st.cards[usize::from(to_uid(st, card_uid)?)].def_id;
        let entry = reg.card(def);
        if let Some(&(n, _, _)) = entry.delayed.iter().find(|&&(_, node, _)| node == ins) {
            let number = &*st.db().by_id(def).card_number;
            let card = reg.str_id(number).ok_or_else(|| {
                EngineError::Internal(format!("card number {number} is not an IR string"))
            })?;
            found = Some((card, i32::from(n)));
        }
    }
    let Some((card, index)) = found else {
        return Err(EngineError::Internal(
            "delayed trigger not registered on its card".to_owned(),
        ));
    };
    let b = st.battle().copied();
    if dt.duration == Duration::ThisBattle && b.is_none_or(|b| b.ended) {
        set_did(st, fi, false);
        return Ok(Status::Next);
    }
    let mut bound: Vec<(Uid, i32)> = Vec::new();
    for &v in &dt.bind_vars {
        for &x in st.var_get(fi, v).unwrap_or(&[]) {
            let u = to_uid(st, x)?;
            bound.push((u, st.cards[usize::from(u)].zone_seq));
        }
    }
    let (controller, host) = (st.frames[fi].controller, st.frames[fi].host);
    st.delayed_push(
        Lasting {
            effect: ContRef::AbilityGrant { card, index },
            controller,
            source_uid: host,
            targets: Span::EMPTY,
            duration: dt.duration,
            created_turn: st.turn,
            battle_id: b.map_or(NO_ARG, |b| b.battle_id),
            player: -1,
            uses: 0,
            filters: None,
            aux: NO_ARG,
            serial: -1,
        },
        &bound,
    )?;
    set_did(st, fi, true);
    Ok(Status::Next)
}
