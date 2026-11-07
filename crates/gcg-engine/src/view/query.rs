//! Characteristic queries (`engine/view.py:125-138`, `:471-474`, `:490-570`, `:936-954`):
//! every per-uid read falls back to the printed value where Python's `.get` does.

use gcg_data::{CardDef, CardType};
use gcg_effects::{AbilityId, ContView, Kw, KwAmounts, Loc, Registry, RuleKind};

use super::derived;
use super::eval::{matches, value};
use crate::{
    Ctx, CtxVars, Derived, EngineError, EventPayload, GameState, NO_ARG, Names, RuleInst, Traits,
    Uid, Zone,
};

/// `ap_of(st, dv, uid)` (`engine/view.py:490-494`).
#[must_use]
pub fn ap_of(st: &GameState, dv: &Derived, uid: Uid) -> i32 {
    dv.ap_get(uid).unwrap_or_else(|| st.def_of(uid).ap.max(0))
}

/// `hp_of(st, dv, uid)` (`engine/view.py:497-504`): a card in SHIELD without a view entry
/// has 1 HP.
#[must_use]
pub fn hp_of(st: &GameState, dv: &Derived, uid: Uid) -> i32 {
    if let Some(v) = dv.hp_get(uid) {
        return v;
    }
    if st.card(uid).zone == Zone::Shield {
        return 1;
    }
    st.def_of(uid).hp.max(0)
}

/// `level_of(st, uid)` (`engine/view.py:507-509`): 0 for tokens.
#[must_use]
pub fn level_of(st: &GameState, uid: Uid) -> i32 {
    let cd = st.def_of(uid);
    if cd.is_token() { 0 } else { cd.level }
}

/// `cost_of(st, uid)` (`engine/view.py:512-514`): 0 for tokens.
#[must_use]
pub fn cost_of(st: &GameState, uid: Uid) -> i32 {
    let cd = st.def_of(uid);
    if cd.is_token() { 0 } else { cd.cost }
}

/// `dv.cost_mod.get(uid, 0)` (`engine/view.py:517`, `effects/bindings/wp_gd02_b.py:62`,
/// `wp_gd03_a.py:106`): the view's cost modification of a card, 0 when it has none.
#[must_use]
pub(crate) fn cost_mod_of(dv: &Derived, uid: Uid) -> i32 {
    dv.cost_mod_get(uid).unwrap_or(0)
}

/// `play_cost(st, dv, uid)` (`engine/view.py:517-518`):
/// `max(0, cost_of + dv.cost_mod.get(uid, 0) + player_cost_mod(st, uid))`, where
/// `player_cost_mod` calls `derived(st)` itself.
///
/// # Errors
/// What `player_cost_mod` raises, or [`EngineError::Overflow`].
pub fn play_cost(st: &GameState, dv: &Derived, uid: Uid) -> Result<i32, EngineError> {
    let base = i64::from(cost_of(st, uid)) + i64::from(cost_mod_of(dv, uid));
    let total = base
        .checked_add(player_cost_mod(st, uid)?)
        .ok_or(EngineError::Overflow { what: "play cost" })?;
    i32::try_from(total.max(0)).map_err(|_| EngineError::Overflow { what: "play cost" })
}

/// `play_level(st, dv, uid)` (`engine/view.py:521-522`): player-level level modifiers are
/// not included. Both terms are `i32`, so their sum cannot leave `i64`; a sum beyond `i32`
/// saturates (no card reaches it).
#[must_use]
pub fn play_level(st: &GameState, dv: &Derived, uid: Uid) -> i32 {
    let v = i64::from(level_of(st, uid)) + i64::from(dv.level_mod_get(uid).unwrap_or(0));
    i32::try_from(v.max(0)).unwrap_or(i32::MAX)
}

/// `player_cost_mod(st, uid)` (`engine/view.py:525-545`): the `CostMod` effects of
/// player-level lasting effects of the card's owner, in `st.lasting` order. A filtered one
/// is skipped unless the card matches its filter set against `derived(st)`; each surviving
/// one adds its `cost` evaluated against `derived(st)` (computed before the value, as
/// Python evaluates the arguments). `level` and `uses` are ignored.
///
/// # Errors
/// What `derived`, a filter or the value raises, or [`EngineError::Overflow`].
pub(crate) fn player_cost_mod(st: &GameState, uid: Uid) -> Result<i64, EngineError> {
    if st.lasting.is_empty() {
        return Ok(0);
    }
    let reg = &**st.registry();
    let owner = i32::from(st.card(uid).owner);
    let mut total: i64 = 0;
    for le in &st.lasting {
        if i32::from(le.player) != owner {
            continue;
        }
        let ContView::CostMod { cost, .. } = reg.cont_node(le.effect) else {
            continue;
        };
        let ctx = Ctx::new(
            le.controller,
            le.source_uid,
            NO_ARG,
            CtxVars::Empty,
            EventPayload::default(),
        );
        if let Some(fs) = le.filters {
            let dv = derived(st)?;
            if !matches(st, &dv, &ctx, uid, &reg.filter_set(fs).filters)? {
                continue;
            }
        }
        let dv = derived(st)?;
        total = total
            .checked_add(value(st, &dv, &ctx, cost)?)
            .ok_or(EngineError::Overflow { what: "play cost" })?;
    }
    Ok(total)
}

/// `keywords(dv, uid)` (`engine/view.py:548-549`): `dv.kw.get(uid, {})`.
#[must_use]
pub fn keywords(dv: &Derived, uid: Uid) -> KwAmounts {
    dv.kw_get(uid).copied().unwrap_or_default()
}

/// `has_kw(dv, uid, kw)` (`engine/view.py:552-553`): the key is present (a stacking keyword
/// granted 0 counts).
#[must_use]
pub fn has_kw(dv: &Derived, uid: Uid, kw: Kw) -> bool {
    dv.kw_get(uid).is_some_and(|k| k.get(kw).is_some())
}

/// `kw_amount(dv, uid, kw)` (`engine/view.py:556-557`): `.get(kw, 0)`.
#[must_use]
pub fn kw_amount(dv: &Derived, uid: Uid, kw: Kw) -> i32 {
    dv.kw_get(uid).and_then(|k| k.get(kw)).map_or(0, i32::from)
}

/// `traits_of(st, dv, uid)` (`engine/view.py:560-562`): the view's traits, else the printed
/// ones.
#[must_use]
pub fn traits_of<'a>(st: &'a GameState, dv: &'a Derived, uid: Uid) -> Traits<'a> {
    match dv.traits_get(uid) {
        Some((t, granted)) => Traits {
            printed: dv.printed_traits(t),
            granted,
        },
        None => Traits {
            printed: &st.def_of(uid).traits,
            granted: &[],
        },
    }
}

/// `dv.traits.get(uid)` (`ai/actions.py:357` reads `dv.traits.get(uid, ())`): the view's
/// traits of `uid`, or `None` when the view has no entry for it (no printed fallback).
#[must_use]
pub fn traits_present(dv: &Derived, uid: Uid) -> Option<Traits<'_>> {
    dv.traits_get(uid).map(|(t, granted)| Traits {
        printed: dv.printed_traits(t),
        granted,
    })
}

/// `rules_of(dv, uid, kind)` (`engine/view.py:565-566`): the uid's rule instances of `kind`
/// in application order (the first valid REDIRECT_BATTLE_DAMAGE wins, `core.py:588-591`).
pub fn rules_of(dv: &Derived, uid: Uid, kind: RuleKind) -> impl Iterator<Item = &RuleInst> + '_ {
    dv.rules_slice(uid)
        .iter()
        .zip(dv.rule_kinds_slice(uid))
        .filter_map(move |(r, &k)| (k == kind).then_some(r))
}

/// `side_allows(rule, owner, by)` (`engine/view.py`): whether a rule limited to effects of
/// `source_side` (relative to the affected card's `owner`) covers an action of player `by`;
/// `by < 0` is no effect at all (the start-phase untap).
#[must_use]
pub fn side_allows(rule: &gcg_effects::RuleMod, owner: u8, by: i32) -> bool {
    match rule.source_side {
        gcg_effects::Side::Any => true,
        _ if by < 0 => false,
        side => (by != i32::from(owner)) == (side == gcg_effects::Side::Enemy),
    }
}

/// `dv.rules.get(uid, ())` (`ai/actions.py:360`).
#[must_use]
pub fn rules_all(dv: &Derived, uid: Uid) -> &[RuleInst] {
    dv.rules_slice(uid)
}

/// `is_linked(dv, uid)` (`engine/view.py:569-570`).
#[must_use]
pub fn is_linked(dv: &Derived, uid: Uid) -> bool {
    dv.linked_get(uid)
}

/// `dv.abilities.get(uid, ())` (`engine/game.py:415`, `ai/actions.py:358`): the host's active
/// aids in order, duplicates kept.
#[must_use]
pub fn abilities_of(dv: &Derived, uid: Uid) -> &[AbilityId] {
    dv.abilities_slice(uid)
}

/// `link_satisfied(unit_def, pilot_def)` (`engine/view.py:125-138`) as the module attribute
/// `view.link_satisfied` that `_base_view` (`view.py:269`) and `ai/policy.py:200` look up:
/// with `TestOverrides::force_link` the named Units count as linked with any paired card
/// (`tests/cards/test_wp_gd05_a.py:101-113`: `unit_def.card_number in linked_units or
/// original(...)`).
#[must_use]
pub fn link_satisfied(st: &GameState, unit: &CardDef, pilot: &CardDef) -> bool {
    st.test_overrides()
        .is_some_and(|o| o.force_link.iter().any(|n| **n == *unit.card_number))
        || link_satisfied_unpatched(st.registry(), unit, pilot)
}

/// `link_satisfied(unit_def, pilot_def)` (`engine/view.py:125-138`) without test overrides
/// (`ai/setup.py:18` binds the original function): a bracketed name part that is a
/// substring of one of the pilot's names (a Command pilot is named by its `pilot_name` only),
/// or a listed trait among the pilot's printed traits (rules 3-2-6-2, 3-2-6-4).
#[must_use]
pub fn link_satisfied_unpatched(reg: &Registry, unit: &CardDef, pilot: &CardDef) -> bool {
    let Some(link) = &unit.link else {
        return false;
    };
    let pilot_name = pilot.pilot_name.as_deref().unwrap_or("");
    let own_contains = |part: &str| {
        if pilot.card_type == CardType::Pilot {
            pilot.names().iter().any(|n| n.contains(part))
        } else {
            pilot_name.contains(part)
        }
    };
    let aliases = reg.aliases(pilot.def_id);
    for part in &link.name_parts {
        let part = &**part;
        if own_contains(part) || aliases.iter().any(|&a| reg.str(a).contains(part)) {
            return true;
        }
    }
    link.traits.iter().any(|t| pilot.traits.contains(t))
}

/// `names_of(cd)` (`engine/view.py:471-474`): `cd.names`, then the card's aliases from
/// `NameAlias` abilities (rule 2-2-4, `registry.py:240-241`).
#[must_use]
pub fn names_of<'a>(reg: &'a Registry, cd: &'a CardDef) -> Names<'a> {
    Names {
        names: cd.names(),
        aliases: reg.aliases(cd.def_id),
    }
}

/// `zone_loc_code(z)` (`engine/view.py:938-954`): `LOC_CODES[ZONE_TO_LOC[z]]` (the `Loc`
/// declaration index), or -1 for RESOLVING and OUTSIDE.
#[must_use]
#[allow(
    dead_code,
    reason = "an L2 boundary name that layers L3-L5 call (docs/dev/contracts.md); they land after L2"
)]
pub(crate) fn zone_loc_code(z: Zone) -> i32 {
    let loc = match z {
        Zone::Battle => Loc::Battle,
        Zone::Paired => Loc::Paired,
        Zone::Base => Loc::Base,
        Zone::Shield => Loc::Shields,
        Zone::ResourceArea => Loc::ResourceArea,
        Zone::Hand => Loc::Hand,
        Zone::Trash => Loc::Trash,
        Zone::Deck => Loc::Deck,
        Zone::ResourceDeck => Loc::ResourceDeck,
        Zone::Removal => Loc::Removal,
        Zone::Resolving | Zone::Outside => return NO_ARG,
    };
    loc as i32
}
