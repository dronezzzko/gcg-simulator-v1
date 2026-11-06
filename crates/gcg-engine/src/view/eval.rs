//! DSL expression evaluation (`engine/view.py:577-933`): players and locations, `select`,
//! `stat`, `matches`/`_match1`, `resolve`, `selects`/`resolve_ref`, `value` and `cond`.
//! Dispatch follows the node's class; `and`/`or`/`all`/`any` short-circuit in Python's order.

use gcg_data::{CardDef, CardType};
use gcg_effects::{
    CardKind, CondId, FilterId, Loc, Node, Op, P, RefId, SelId, Side, Stat, ValueRef,
};

use super::query::{ap_of, cost_of, hp_of, kw_amount, level_of, names_of};
use super::{derived, hooks, ir_bug, uid_of};
use crate::{
    CardInstance, Ctx, Derived, EngineError, EvKey, EventPayload, GameState, HistKindRef, NO_ARG,
    PLAYER_TARGET, Uid, UidBuf, Zone,
};

fn overflow() -> EngineError {
    EngineError::Overflow { what: "DSL value" }
}

/// `_players(ctrl, side)` (`engine/view.py:577-582`): the controller first.
fn players(ctrl: u8, side: Side) -> ([u8; 2], usize) {
    match side {
        Side::Friendly => ([ctrl, ctrl], 1),
        Side::Enemy => ([1 - ctrl, 1 - ctrl], 1),
        Side::Any => ([ctrl, 1 - ctrl], 2),
    }
}

/// `player_of(st, ctx, p)` (`engine/view.py:585-592`): STANDBY is any value but the first
/// three.
pub(crate) fn player_of(st: &GameState, ctx: &Ctx, p: P) -> u8 {
    match p {
        P::You => ctx.controller,
        P::Opp => 1 - ctx.controller,
        P::Active => st.active,
        P::Standby => 1 - st.active,
    }
}

fn len_i64(n: usize) -> Result<i64, EngineError> {
    i64::try_from(n).map_err(|_| overflow())
}

/// The `Sel` node of a handle.
fn sel_node(st: &GameState, sel: SelId) -> Result<&gcg_effects::Sel, EngineError> {
    match st.registry().node(sel.node()) {
        Node::Sel(s) => Ok(s),
        other => Err(ir_bug(&format!(
            "a SelId names a {}",
            other.kind().py_name()
        ))),
    }
}

/// `_loc_cards(st, dv, ctx, player, sel)` (`engine/view.py:595-623`) as at most two zone
/// slices (SHIELD_AREA is `BASE + SHIELD`, any other location `BATTLE + BASE`); DECK_TOP
/// evaluates `top_n` on every call and slices `[:max(0, n)]`.
fn loc_cards<'s>(
    st: &'s GameState,
    dv: &Derived,
    ctx: &Ctx,
    player: u8,
    sel: &gcg_effects::Sel,
) -> Result<[&'s [Uid]; 2], EngineError> {
    let z = |zone| st.zone(player, zone);
    let none: &[Uid] = &[];
    Ok(match sel.loc {
        Loc::Battle => [z(Zone::Battle), none],
        Loc::Paired => [z(Zone::Paired), none],
        Loc::Base => [z(Zone::Base), none],
        Loc::Shields => [z(Zone::Shield), none],
        Loc::ShieldArea => [z(Zone::Base), z(Zone::Shield)],
        Loc::ResourceArea => [z(Zone::ResourceArea), none],
        Loc::Hand => [z(Zone::Hand), none],
        Loc::Trash => [z(Zone::Trash), none],
        Loc::Deck => [z(Zone::Deck), none],
        Loc::DeckTop => {
            let n = value(st, dv, ctx, sel.top_n)?;
            let deck = z(Zone::Deck);
            let k = usize::try_from(n.max(0)).map_or(deck.len(), |k| k.min(deck.len()));
            [&deck[..k], none]
        }
        Loc::ResourceDeck => [z(Zone::ResourceDeck), none],
        Loc::Removal => [z(Zone::Removal), none],
        Loc::FieldUnitsAndBases => [z(Zone::Battle), z(Zone::Base)],
    })
}

/// `select(st, dv, ctx, sel)` (`engine/view.py:626-632`): per player of the side (controller
/// first), the location's cards in list order that match every filter.
///
/// # Errors
/// What a filter or `top_n` raises.
pub(crate) fn select(
    st: &GameState,
    dv: &Derived,
    ctx: &Ctx,
    sel: SelId,
) -> Result<UidBuf, EngineError> {
    let s = sel_node(st, sel)?;
    let mut out = UidBuf::new();
    let (ps, n) = players(ctx.controller, s.side);
    for &p in &ps[..n] {
        for part in loc_cards(st, dv, ctx, p, s)? {
            for &uid in part {
                if s.filters.is_empty() || matches(st, dv, ctx, uid, &s.filters)? {
                    out.push(uid);
                }
            }
        }
    }
    Ok(out)
}

/// `_activated_as_command(event, uid)` (`engine/view.py:635-638`): tuple membership by value.
fn activated_as_command(event: &EventPayload, uid: Uid) -> bool {
    let pairs = event.pairs();
    pairs.contains(&(EvKey::Subject, i32::from(uid))) && pairs.contains(&(EvKey::AsCommand, 1))
}

/// `_kind_ok(st, dv, card, cd, kind, event)` (`engine/view.py:641-670`).
fn kind_ok(
    dv: &Derived,
    uid: Uid,
    card: &CardInstance,
    cd: &CardDef,
    kind: CardKind,
    event: &EventPayload,
) -> bool {
    let t = cd.card_type;
    match kind {
        CardKind::Unit => t.is_unit(),
        CardKind::Pilot => {
            t == CardType::Pilot || (t == CardType::Command && card.zone == Zone::Paired)
        }
        CardKind::Command => {
            t == CardType::Command
                && (card.zone != Zone::Paired || activated_as_command(event, uid))
        }
        CardKind::Base => t.is_base(),
        CardKind::Resource => t.is_resource(),
        CardKind::Token => t.is_token(),
        CardKind::UnitToken => t == CardType::UnitToken,
        CardKind::ExResource => t == CardType::ExResource,
        CardKind::ExBase => t == CardType::ExBase,
        CardKind::LinkUnit => t.is_unit() && dv.linked_get(uid),
    }
}

/// `stat(st, dv, uid, s)` (`engine/view.py:673-685`): HP referred to by card text is the
/// current HP (FAQ Q96); LV and COST are printed.
#[must_use]
pub fn stat(st: &GameState, dv: &Derived, uid: Uid, s: Stat) -> i32 {
    let remaining = || {
        hp_of(st, dv, uid)
            .saturating_sub(st.card(uid).damage)
            .max(0)
    };
    match s {
        Stat::Ap => ap_of(st, dv, uid),
        Stat::Hp | Stat::RemainingHp => remaining(),
        Stat::Lv => level_of(st, uid),
        Stat::Cost => cost_of(st, uid),
        Stat::Damage => st.card(uid).damage,
    }
}

/// `_cmp(a, op, b)` (`engine/view.py:688-699`).
fn cmp(a: i64, op: Op, b: i64) -> bool {
    match op {
        Op::Le => a <= b,
        Op::Ge => a >= b,
        Op::Eq => a == b,
        Op::Lt => a < b,
        Op::Gt => a > b,
        Op::Ne => a != b,
    }
}

/// `matches(st, dv, ctx, uid, filters)` (`engine/view.py:702-705`): every filter, in order,
/// short-circuit; an empty tuple matches.
///
/// # Errors
/// What a filter raises (a value, a reference or a hook), or [`EngineError::Index`] for a uid
/// the state does not have.
pub fn matches(
    st: &GameState,
    dv: &Derived,
    ctx: &Ctx,
    uid: Uid,
    filters: &[FilterId],
) -> Result<bool, EngineError> {
    let uid = uid_of(st, i32::from(uid))?;
    let card = *st.card(uid);
    let cd = st.def_of(uid);
    for &f in filters {
        if !match1(st, dv, ctx, uid, &card, cd, f)? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn any_match(
    st: &GameState,
    dv: &Derived,
    ctx: &Ctx,
    uid: Uid,
    card: &CardInstance,
    cd: &CardDef,
    fs: &[FilterId],
) -> Result<bool, EngineError> {
    for &g in fs {
        if match1(st, dv, ctx, uid, card, cd, g)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// `_match1(st, dv, ctx, card, cd, f)` (`engine/view.py:708-781`).
fn match1(
    st: &GameState,
    dv: &Derived,
    ctx: &Ctx,
    uid: Uid,
    card: &CardInstance,
    cd: &CardDef,
    f: FilterId,
) -> Result<bool, EngineError> {
    let reg = &**st.registry();
    let in_field = matches!(card.zone, Zone::Battle | Zone::Base);
    Ok(match reg.node(f.node()) {
        Node::IsKind(x) => x
            .kinds
            .iter()
            .any(|&k| kind_ok(dv, uid, card, cd, k, &ctx.event)),
        Node::HasTrait(x) => {
            let traits = dv.traits_get(uid).filter(|_| in_field);
            x.traits.iter().any(|&t| {
                let s = reg.str(t);
                match traits {
                    Some((tr, granted)) => {
                        dv.printed_traits(tr).iter().any(|p| **p == *s) || granted.contains(&t)
                    }
                    None => cd.traits.iter().any(|p| **p == *s),
                }
            })
        }
        Node::NameContains(x) => {
            let names = names_of(reg, cd);
            x.parts.iter().any(|&part| {
                let part = reg.str(part);
                names.iter(reg).any(|n| n.contains(part))
            })
        }
        Node::NameIs(x) => names_of(reg, cd)
            .iter(reg)
            .any(|n| x.names.iter().any(|&m| reg.str(m) == n)),
        Node::HasColor(x) => match cd.color {
            Some(c) if !cd.is_token() => x.colors.iter().any(|&s| reg.str(s) == c.as_str()),
            _ => false,
        },
        Node::StatCmp(x) => {
            let a = i64::from(stat(st, dv, uid, x.stat));
            cmp(a, x.op, value(st, dv, ctx, x.value)?)
        }
        Node::IsRested(x) => card.rested == x.rested,
        Node::HasKeyword(x) => {
            if in_field {
                dv.kw_get(uid).is_some_and(|k| k.get(x.keyword).is_some())
            } else {
                reg.card(card.def_id)
                    .printed_keywords
                    .get(x.keyword)
                    .is_some()
            }
        }
        Node::IsLinked(x) => dv.linked_get(uid) == x.linked,
        Node::IsPaired(x) => (card.zone == Zone::Battle && card.pair >= 0) == x.paired,
        Node::PairedWith(x) => {
            card.zone == Zone::Battle
                && card.pair >= 0
                && matches(st, dv, ctx, uid_of(st, card.pair)?, &x.filters)?
        }
        Node::PairedTo(x) => {
            card.zone == Zone::Paired
                && card.pair >= 0
                && matches(st, dv, ctx, uid_of(st, card.pair)?, &x.filters)?
        }
        Node::IsToken(x) => cd.is_token() == x.token,
        Node::IsDamaged(x) => (card.damage > 0) == x.damaged,
        Node::NotRef(x) => !resolve(st, dv, ctx, x.r#ref)?.contains(&uid),
        Node::IsRef(x) => resolve(st, dv, ctx, x.r#ref)?.contains(&uid),
        Node::IsAttacking(x) => {
            let b = st.battle();
            b.is_some_and(|b| !b.ended && b.attacker == i32::from(uid)) == x.attacking
        }
        Node::IsBattling(x) => {
            let u = i32::from(uid);
            let b = st.battle();
            b.is_some_and(|b| !b.ended && (u == b.attacker || u == b.target)) == x.battling
        }
        Node::HasTiming(x) => reg.card(card.def_id).timings.contains(&x.timing),
        Node::HasBurst(x) => reg.card(card.def_id).has_burst == x.burst,
        Node::DeployedThisTurn(x) => (in_field && card.entered_turn == st.turn) == x.value,
        Node::HasZone(x) => x.zones.iter().any(|&z| {
            let z = reg.str(z);
            cd.zones.iter().any(|c| **c == *z)
        }),
        Node::LevelCmpRef(x) => {
            let others = resolve(st, dv, ctx, x.r#ref)?;
            match others.first() {
                None => false,
                Some(&o) => cmp(
                    i64::from(stat(st, dv, uid, x.stat)),
                    x.op,
                    i64::from(stat(st, dv, o, x.ref_stat)),
                ),
            }
        }
        Node::AnyOf(x) => any_match(st, dv, ctx, uid, card, cd, &x.filters)?,
        Node::AllOf(x) => {
            for &g in &x.filters {
                if !match1(st, dv, ctx, uid, card, cd, g)? {
                    return Ok(false);
                }
            }
            true
        }
        Node::Not(x) => !match1(st, dv, ctx, uid, card, cd, x.filter)?,
        Node::CustomFilter(_) => hooks::filter(st, dv, ctx, uid, f.node())?,
        other => {
            return Err(ir_bug(&format!(
                "a FilterId names a {}",
                other.kind().py_name()
            )));
        }
    })
}

/// `resolve(st, dv, ctx, ref)` (`engine/view.py:784-831`): an ordered tuple of uids.
/// `PairedPilotOf`, `PairedUnitOf` and `BattlingWith` keep duplicates; `Union` drops later
/// repeats.
///
/// # Errors
/// What a selection raises, or [`EngineError::Index`] for an int that is no uid of the state.
pub(crate) fn resolve(
    st: &GameState,
    dv: &Derived,
    ctx: &Ctx,
    r: RefId,
) -> Result<UidBuf, EngineError> {
    let reg = &**st.registry();
    let mut out = UidBuf::new();
    match reg.node(r.node()) {
        Node::This(_) => {
            if ctx.host != NO_ARG {
                out.push(uid_of(st, ctx.host)?);
            }
        }
        Node::ThisCard(_) => {
            if ctx.card_uid != NO_ARG {
                out.push(uid_of(st, ctx.card_uid)?);
            }
        }
        Node::Var(x) => {
            for &u in ctx.var(st, x.name)? {
                out.push(uid_of(st, u)?);
            }
        }
        Node::EventCard(x) => {
            let v = ctx.ev_str(reg, x.key, NO_ARG);
            if v >= 0 {
                out.push(uid_of(st, v)?);
            }
        }
        Node::PairedPilotOf(x) => {
            for u in resolve(st, dv, ctx, x.r#ref)? {
                let c = st.card(u);
                if c.zone == Zone::Battle && c.pair >= 0 {
                    out.push(uid_of(st, c.pair)?);
                } else if ctx.ev(EvKey::Subject, NO_ARG) == i32::from(u)
                    && ctx.ev(EvKey::Pilot, NO_ARG) >= 0
                {
                    // rule 13-2-8-2-1: a 【Destroyed】 effect refers to the last-known state
                    let pilot = uid_of(st, ctx.ev(EvKey::Pilot, NO_ARG))?;
                    if st.card(pilot).zone == c.zone {
                        out.push(pilot);
                    }
                }
            }
        }
        Node::PairedUnitOf(x) => {
            for u in resolve(st, dv, ctx, x.r#ref)? {
                let c = st.card(u);
                if c.zone == Zone::Paired && c.pair >= 0 {
                    out.push(uid_of(st, c.pair)?);
                }
            }
        }
        Node::BattlingWith(x) => {
            let Some(&b) = st.battle() else {
                return Ok(out);
            };
            if b.ended {
                return Ok(out);
            }
            for u in resolve(st, dv, ctx, x.r#ref)? {
                let u = i32::from(u);
                if u == b.attacker && b.target >= 0 {
                    out.push(uid_of(st, b.target)?);
                } else if u == b.target {
                    out.push(uid_of(st, b.attacker)?);
                }
            }
        }
        Node::All(x) => return select(st, dv, ctx, x.sel),
        Node::Union(x) => {
            for &sub in &x.refs {
                for u in resolve(st, dv, ctx, sub)? {
                    if !out.contains(&u) {
                        out.push(u);
                    }
                }
            }
        }
        other => {
            return Err(ir_bug(&format!(
                "a RefId names a {}",
                other.kind().py_name()
            )));
        }
    }
    Ok(out)
}

/// `selects(ref)` (`engine/view.py:834-842`): whether resolving the reference selects cards,
/// the only references that read the view.
#[allow(
    dead_code,
    reason = "an L2 boundary name that layers L3-L5 call (docs/dev/contracts.md); they land after L2"
)]
pub(crate) fn selects(st: &GameState, r: RefId) -> bool {
    match st.registry().node(r.node()) {
        Node::This(_) | Node::ThisCard(_) | Node::Var(_) | Node::EventCard(_) => false,
        Node::PairedPilotOf(x) => selects(st, x.r#ref),
        Node::PairedUnitOf(x) => selects(st, x.r#ref),
        Node::BattlingWith(x) => selects(st, x.r#ref),
        Node::Union(x) => x.refs.iter().any(|&s| selects(st, s)),
        _ => true,
    }
}

/// `_UNREAD = Derived()` (`engine/view.py:845`): one shared view with every key absent.
static UNREAD: Derived = Derived::unallocated();

/// `resolve_ref(st, ctx, ref)` (`engine/view.py:848-850`): `resolve` against `derived(st)`
/// only when the reference selects cards, else against `_UNREAD`; the result is the same,
/// and only whether the cache is filled differs. Under `paranoid` a view-free
/// reference is resolved again with the view (`tests/paranoid.py:125-132`).
///
/// # Errors
/// What `derived` or `resolve` raises.
#[allow(
    dead_code,
    reason = "an L2 boundary name that layers L3-L5 call (docs/dev/contracts.md); they land after L2"
)]
pub(crate) fn resolve_ref(st: &GameState, ctx: &Ctx, r: RefId) -> Result<UidBuf, EngineError> {
    if selects(st, r) {
        let dv = derived(st)?;
        return resolve(st, &dv, ctx, r);
    }
    let got = resolve(st, &UNREAD, ctx, r)?;
    #[cfg(feature = "paranoid")]
    super::paranoid::view_free_ref(st, ctx, r, &got)?;
    Ok(got)
}

/// `value(st, dv, ctx, v)` (`engine/view.py:853-882`): integer arithmetic is exact or an
/// [`EngineError::Overflow`] (Python ints are unbounded).
///
/// # Errors
/// `MinOf(())` raises `ValueError: min() iterable argument is empty` (`view.py:878`); what a
/// reference, selection or hook raises.
pub(crate) fn value(
    st: &GameState,
    dv: &Derived,
    ctx: &Ctx,
    v: ValueRef,
) -> Result<i64, EngineError> {
    let id = match v {
        ValueRef::Lit(n) => return Ok(n),
        ValueRef::Node(id) => id,
    };
    let reg = &**st.registry();
    let zone_len = |p: P, z: Zone| len_i64(st.zone(player_of(st, ctx, p), z).len());
    match reg.node(id) {
        Node::Count(x) => len_i64(select(st, dv, ctx, x.sel)?.len()),
        Node::StatOf(x) => {
            let cards = resolve(st, dv, ctx, x.r#ref)?;
            Ok(cards
                .first()
                .map_or(0, |&u| i64::from(stat(st, dv, u, x.stat))))
        }
        Node::PlayerLevel(x) => zone_len(x.player, Zone::ResourceArea),
        Node::HandSize(x) => zone_len(x.player, Zone::Hand),
        Node::ShieldCount(x) => zone_len(x.player, Zone::Shield),
        Node::DeckSize(x) => zone_len(x.player, Zone::Deck),
        Node::VarSize(x) => len_i64(ctx.var(st, x.var)?.len()),
        Node::EventAmount(x) => Ok(i64::from(ctx.ev_str(reg, x.key, 0).max(0))),
        Node::Sum(x) => {
            let mut total: i64 = 0;
            for &t in &x.terms {
                total = total
                    .checked_add(value(st, dv, ctx, t)?)
                    .ok_or_else(overflow)?;
            }
            Ok(total)
        }
        Node::Times(x) => value(st, dv, ctx, x.value)?
            .checked_mul(x.factor)
            .ok_or_else(overflow),
        Node::MinOf(x) => {
            let mut best: Option<i64> = None;
            for &t in &x.terms {
                let v = value(st, dv, ctx, t)?;
                best = Some(best.map_or(v, |b| b.min(v)));
            }
            best.ok_or_else(|| EngineError::Value("min() iterable argument is empty".to_owned()))
        }
        Node::KwAmount(x) => {
            let cards = resolve(st, dv, ctx, x.r#ref)?;
            Ok(cards
                .first()
                .map_or(0, |&u| i64::from(kw_amount(dv, u, x.keyword))))
        }
        Node::CustomValue(_) => hooks::value(st, dv, ctx, id),
        other => Err(ir_bug(&format!(
            "a Value names a {}",
            other.kind().py_name()
        ))),
    }
}

/// `cond(st, dv, ctx, c, did=True)` (`engine/view.py:885-933`).
///
/// # Errors
/// What a value, reference, selection, filter or hook raises.
pub(crate) fn cond(
    st: &GameState,
    dv: &Derived,
    ctx: &Ctx,
    c: CondId,
    did: bool,
) -> Result<bool, EngineError> {
    let reg = &**st.registry();
    Ok(match reg.node(c.node()) {
        Node::And(x) => {
            for &y in &x.conds {
                if !cond(st, dv, ctx, y, did)? {
                    return Ok(false);
                }
            }
            true
        }
        Node::Or(x) => {
            for &y in &x.conds {
                if cond(st, dv, ctx, y, did)? {
                    return Ok(true);
                }
            }
            false
        }
        Node::NotC(x) => !cond(st, dv, ctx, x.cond, did)?,
        Node::Cmp(x) => {
            let left = value(st, dv, ctx, x.left)?;
            cmp(left, x.op, value(st, dv, ctx, x.right)?)
        }
        Node::Exists(x) => len_i64(select(st, dv, ctx, x.sel)?.len())? >= x.at_least,
        Node::IsTurn(x) => st.active == player_of(st, ctx, x.player),
        Node::RefMatches(x) => {
            let cards = resolve(st, dv, ctx, x.r#ref)?;
            if cards.is_empty() {
                return Ok(false);
            }
            for &u in &cards {
                if !matches(st, dv, ctx, u, &x.filters)? {
                    return Ok(false);
                }
            }
            true
        }
        Node::RefEmpty(x) => resolve(st, dv, ctx, x.r#ref)?.is_empty(),
        Node::AttackTargetIs(x) => match st.battle() {
            Some(&b) if !b.ended => {
                if reg.str(x.kind) == "player" {
                    b.target == PLAYER_TARGET || st.card(uid_of(st, b.target)?).zone == Zone::Base
                } else {
                    b.target >= 0 && st.card(uid_of(st, b.target)?).zone == Zone::Battle
                }
            }
            _ => false,
        },
        Node::EventFrom(x) => ctx.ev(EvKey::FromLoc, NO_ARG) == x.loc as i32,
        Node::EventFlag(x) => (ctx.ev_str(reg, x.key, 0) != 0) == x.value,
        Node::HappenedThisTurn(x) => {
            let kind = HistKindRef::decode(reg.str(x.kind), x.kind);
            let who = player_of(st, ctx, x.player);
            let by = x.by.map_or(NO_ARG, |p| i32::from(player_of(st, ctx, p)));
            let mut n: i64 = 0;
            for h in st.history() {
                if !kind.matches(h.kind) || h.player != who {
                    continue;
                }
                if by != NO_ARG && by != i32::from(h.by) {
                    continue;
                }
                if !x.filters.is_empty()
                    && (h.uid < 0 || !matches(st, dv, ctx, uid_of(st, h.uid)?, &x.filters)?)
                {
                    continue;
                }
                if !x.source_filters.is_empty()
                    && (h.source < 0
                        || !matches(st, dv, ctx, uid_of(st, h.source)?, &x.source_filters)?)
                {
                    continue;
                }
                n += 1;
            }
            n >= x.at_least
        }
        Node::DidLast(_) => did,
        Node::CustomCond(_) => hooks::cond(st, dv, ctx, c.node())?,
        other => {
            return Err(ir_bug(&format!(
                "a CondId names a {}",
                other.kind().py_name()
            )));
        }
    })
}
