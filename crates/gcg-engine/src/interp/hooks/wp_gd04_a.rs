//! Step hooks of `effects/bindings/wp_gd04_a.py` (#20, #21).

use gcg_effects::{ContRef, Duration, Ev, Kw, Registry, StrId, VarId};

use crate::core::{self as core_};
use crate::interp::{add_lasting, ctx_of, to_i32, to_uid, var_items};
use crate::view;
use crate::{EngineError, EvKey, GameState, LastingOpts, NO_ARG, Uid, Zone};

/// `add_lasting`'s keyword defaults: `player=-1, uses=0, filters=(), aux=-1`.
const DEFAULTS: LastingOpts = LastingOpts {
    player: -1,
    uses: 0,
    filters: None,
    aux: NO_ARG,
};

/// `d.STACKING_KEYWORDS` (`effects/dsl.py:72`).
const fn is_stacking(kw: Kw) -> bool {
    matches!(kw, Kw::Repair | Kw::Breach | Kw::Support)
}

/// `wp_gd04_a_copy_keywords` (`wp_gd04_a.py:69-81`): GD04-067, this Unit gains during this
/// turn every printed keyword of the first chosen card (`R.printed_keywords`, iterated by
/// keyword value as `sorted(kws.items())` does), a stacking keyword with its amount and any
/// other with 0; one lasting effect per keyword on the host. False without a choice or with
/// the host outside the battle area; else whether the card had a keyword.
///
/// Python reads `st.cards[f.host]` before testing the choice; a hostless frame (`f.host` -1,
/// Python's last card) never runs this hook, whose ability is the host's own, so it is an
/// `IndexError` here.
pub(super) fn copy_keywords(
    st: &mut GameState,
    reg: &Registry,
    fi: usize,
    var: VarId,
) -> Result<bool, EngineError> {
    let chosen = var_items(st, fi, var);
    let (controller, host) = (st.frames[fi].controller, st.frames[fi].host);
    let Some(&first) = chosen.first() else {
        return Ok(false);
    };
    let host_uid = to_uid(st, host)?;
    let hc = st.cards[usize::from(host_uid)];
    if hc.zone != Zone::Battle {
        return Ok(false);
    }
    let target = [(host_uid, hc.zone_seq)];
    let def = st.cards[usize::from(to_uid(st, first)?)].def_id;
    let kws = reg.card(def).printed_keywords;
    let mut any = false;
    for kw in Kw::SORTED_BY_VALUE {
        let Some(amount) = kws.get(kw) else {
            continue;
        };
        any = true;
        let grant = ContRef::KeywordGrant {
            kw,
            amount: if is_stacking(kw) {
                i32::from(amount)
            } else {
                0
            },
        };
        add_lasting(
            st,
            grant,
            controller,
            host,
            &target,
            Duration::ThisTurn,
            DEFAULTS,
        )?;
    }
    Ok(any)
}

/// `wp_gd04_a_ap_down_by_named_trash_units` (`wp_gd04_a.py:84-109`): GD04-057, AP-X during
/// this turn for the chosen cards in the battle area, X = the Unit cards in the controller's
/// trash with the name part in one of their names, fixed when the effect resolves. The
/// selection `Sel(FRIENDLY, TRASH, (IsKind((UNIT,)), NameContains((name,))))` is built at run
/// time, so it is evaluated here as `V.select` would (`view.py:577-632`, `:702-720`: the
/// controller's trash in order, `_kind_ok` UNIT is `card_type.is_unit`, `NameContains` is a
/// substring test over `names_of`), after the `derived` call of the selection. Every target
/// whose AP dropped on the fresh view is announced (AP_REDUCED, one new group).
pub(super) fn ap_down_by_named_trash_units(
    st: &mut GameState,
    reg: &Registry,
    fi: usize,
    name: StrId,
    var: VarId,
) -> Result<bool, EngineError> {
    let part = reg.str(name);
    let dv = view::derived(st)?;
    let ctx = ctx_of(st, fi)?;
    let mut n: i64 = 0;
    for &u in st.zone(ctx.controller, Zone::Trash) {
        let cd = st.def_of(u);
        if cd.card_type.is_unit()
            && cd
                .names()
                .iter()
                .map(|s| &**s)
                .chain(reg.aliases(cd.def_id).iter().map(|&a| reg.str(a)))
                .any(|nm| nm.contains(part))
        {
            n += 1;
        }
    }
    drop(dv);
    let mut live: Vec<(Uid, i32)> = Vec::new();
    for x in var_items(st, fi, var) {
        let u = to_uid(st, x)?;
        let c = &st.cards[usize::from(u)];
        if c.zone == Zone::Battle {
            live.push((u, c.zone_seq));
        }
    }
    if n <= 0 || live.is_empty() {
        return Ok(false);
    }
    let dv = view::derived(st)?;
    let before: Vec<i32> = live.iter().map(|&(u, _)| view::ap_of(st, &dv, u)).collect();
    drop(dv);
    let (controller, host) = (st.frames[fi].controller, st.frames[fi].host);
    let ap = to_i32(-n, "StatMod ap")?;
    add_lasting(
        st,
        ContRef::StatMod { ap, hp: 0 },
        controller,
        host,
        &live,
        Duration::ThisTurn,
        DEFAULTS,
    )?;
    let dv = view::derived(st)?;
    let group = to_i32(core_::next_group(st), "event group")?;
    for (&(u, _), &ap_before) in live.iter().zip(&before) {
        if view::ap_of(st, &dv, u) < ap_before {
            let owner = st.cards[usize::from(u)].owner;
            core_::emit(
                st,
                Ev::ApReduced,
                i32::from(u),
                i32::from(owner),
                None,
                &[(EvKey::By, i32::from(controller)), (EvKey::Group, group)],
            )?;
        }
    }
    Ok(true)
}
