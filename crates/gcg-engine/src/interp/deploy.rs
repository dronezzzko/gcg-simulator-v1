//! Deploying and resources (`interp.py:879-1100`): battle-area and base-section excess
//! (rules 11-4, 11-5), `DeployCard` with `deploy_cards`, tokens, (EX) Resources, and setting
//! Resources active or rested (Inv 06a §13.4).

use gcg_data::CardType;
use gcg_effects::{
    DeployCard, DeployToken, Ev, NodeId, PlaceExResource, PlaceResource, Registry, RestResources,
    RuleKind, SetResourcesActive,
};

use super::{
    Status, by_of, ctx_of, decide, execute, player_of, refs, set_did, set_var_uids, to_i32, to_uid,
    val,
};
use crate::core::{self as core_};
use crate::view;
use crate::{
    Action, ActionKind, BASE_LIMIT, BATTLE_LIMIT, CtxKey, DecisionKind, EX_RESOURCE_LIMIT,
    EngineError, EnginePrompt, EvKey, GameState, HistKind, IntKey, MoveOpts, NO_ARG, Prompt,
    RESOURCE_LIMIT, Uid, Zone,
};

/// A zone-size limit as an `i64` for the excess arithmetic.
fn limit(n: usize) -> i64 {
    i64::try_from(n).unwrap_or(i64::MAX)
}

/// `_excess_needed(st, player, incoming_units, incoming_bases)` (`interp.py:879-886`): how
/// many cards each area exceeds its limit by after the incoming cards arrive.
fn excess_needed(
    st: &GameState,
    player: u8,
    incoming_units: i64,
    incoming_bases: i64,
) -> (i64, i64) {
    let units = limit(st.zone(player, Zone::Battle).len());
    let bases = limit(st.zone(player, Zone::Base).len());
    (
        (units + incoming_units - limit(BATTLE_LIMIT)).max(0),
        (bases + incoming_bases - limit(BASE_LIMIT)).max(0),
    )
}

/// `_ask_excess(st, f, player, zone, protect)` (`interp.py:889-896`): an EXCESS decision over
/// the zone's cards that are not being deployed; the prompt is "battle area excess" for bases
/// too.
fn ask_excess(
    st: &mut GameState,
    fi: usize,
    player: u8,
    zone: Zone,
    protect: &[Uid],
) -> Result<Status, EngineError> {
    let opts: Vec<Action> = st
        .zone(player, zone)
        .iter()
        .filter(|u| !protect.contains(u))
        .map(|&u| Action::new(ActionKind::Select, i32::from(u), NO_ARG, NO_ARG))
        .collect();
    decide(
        st,
        fi,
        player,
        DecisionKind::Excess,
        &opts,
        Prompt::Engine(EnginePrompt::BattleAreaExcess),
        &[(CtxKey::Zone, i32::from(zone.value()))],
    )
}

/// `_r_excess(st, f, ins, action)` (`interp.py:899-901`): trashes the chosen card, then runs
/// the whole `DeployCard`/`DeployToken` handler again.
pub(super) fn r_excess(
    st: &mut GameState,
    reg: &Registry,
    fi: usize,
    ins: NodeId,
    action: Action,
) -> Result<Status, EngineError> {
    let uid = to_uid(st, action.a)?;
    trash_excess(st, uid)?;
    execute(st, reg, fi, ins)
}

/// `trash_excess(st, uid)` (`interp.py:904-910`): rules 11-4-2 / 11-5-2, placed into the trash
/// (not destroyed, 5-10-4) and recorded; a token that is in the trash is removed from the game.
pub(super) fn trash_excess(st: &mut GameState, uid: Uid) -> Result<(), EngineError> {
    let owner = st.cards[usize::from(uid)].owner;
    core_::move_card(st, uid, Zone::Trash, MoveOpts::default())?;
    core_::record(
        st,
        HistKind::Excess,
        owner,
        by_of(owner),
        i32::from(uid),
        NO_ARG,
    );
    if st.def_of(uid).is_token() && st.cards[usize::from(uid)].zone == Zone::Trash {
        core_::remove_from_game(st, uid)?;
    }
    Ok(())
}

/// `_deployable(st, uids)` (`interp.py:913-920`): Unit and Base cards that are not already in
/// play or OUTSIDE.
fn deployable(st: &GameState, uids: &[Uid]) -> Vec<Uid> {
    uids.iter()
        .copied()
        .filter(|&u| {
            let t = st.def_of(u).card_type;
            (t.is_unit() || t.is_base())
                && !matches!(
                    st.cards[usize::from(u)].zone,
                    Zone::Battle | Zone::Base | Zone::Outside
                )
        })
        .collect()
}

/// `_h_deploy_card` (`interp.py:923-953`; `pay_cost` and `ignore_level` are unused). With
/// `as_unit` every reference that has a Unit variant becomes it (one touch each). Owners in
/// ascending order: an exceeded battle area is an EXCESS decision; an exceeded base section
/// trashes its only Base automatically, or asks. Then `deploy_cards` with the frame's
/// `ex_used`.
pub(super) fn h_deploy_card(
    st: &mut GameState,
    fi: usize,
    ins: &DeployCard,
) -> Result<Status, EngineError> {
    if ins.as_unit {
        let db = st.db();
        for u in refs(st, fi, ins.r#ref)? {
            let c = st.cards[usize::from(u)];
            if let Some(variant) = db.unit_variant(c.def_id)
                && !matches!(c.zone, Zone::Battle | Zone::Base | Zone::Outside)
            {
                st.cards[usize::from(u)].def_id = variant.def_id;
                st.touch();
            }
        }
    }
    let cards = deployable(st, &refs(st, fi, ins.r#ref)?);
    if cards.is_empty() {
        set_did(st, fi, false);
        return Ok(Status::Next);
    }
    let mut by_player: [Vec<Uid>; 2] = [Vec::new(), Vec::new()];
    for &u in &cards {
        by_player[usize::from(st.cards[usize::from(u)].owner)].push(u);
    }
    for (p, us) in (0u8..).zip(&by_player) {
        if us.is_empty() {
            continue;
        }
        let n_units = limit(
            us.iter()
                .filter(|&&u| core_::card_type(st, u).is_unit())
                .count(),
        );
        let n_bases = limit(us.len()) - n_units;
        let (ex_u, ex_b) = excess_needed(st, p, n_units, n_bases);
        if ex_u > 0 {
            return ask_excess(st, fi, p, Zone::Battle, us);
        }
        if ex_b > 0 {
            if let [only] = st.zone(p, Zone::Base) {
                let only = *only;
                trash_excess(st, only)?;
            } else {
                return ask_excess(st, fi, p, Zone::Base, us);
            }
        }
    }
    let by = i32::from(st.frames[fi].controller);
    let ex_used = st.int_get(fi, IntKey::ExUsed).unwrap_or(0);
    deploy_cards(st, &cards, ins.rested, by, ex_used)?;
    set_did(st, fi, true);
    Ok(Status::Next)
}

/// `deploy_cards(st, cards, *, rested, by, ex_used=0)` (`interp.py:956-976`): one group; each
/// card to BATTLE (Units) or BASE and recorded; cards with a DEPLOYED_RESTED rule enter rested;
/// then DEPLOYED for each with the zone it came from and `ex_used`.
///
/// # Errors
/// Any error of `move`, the view or `emit`.
#[allow(
    dead_code,
    reason = "called by layer L5 (game, testkit) and C's tests (docs/port/contracts.md); they land after L4"
)]
pub(crate) fn deploy_cards(
    st: &mut GameState,
    cards: &[Uid],
    rested: bool,
    by: i32,
    ex_used: i32,
) -> Result<(), EngineError> {
    let group = to_i32(core_::next_group(st), "event group")?;
    let froms: Vec<Zone> = cards
        .iter()
        .map(|&u| st.cards[usize::from(u)].zone)
        .collect();
    let by_i8 = i8::try_from(by).map_err(|_| EngineError::Overflow { what: "history by" })?;
    for &u in cards {
        let dst = if core_::card_type(st, u).is_unit() {
            Zone::Battle
        } else {
            Zone::Base
        };
        core_::move_card(
            st,
            u,
            dst,
            MoveOpts {
                rested,
                ..MoveOpts::default()
            },
        )?;
        let owner = st.cards[usize::from(u)].owner;
        core_::record(st, HistKind::Deployed, owner, by_i8, i32::from(u), NO_ARG);
    }
    enter_rested(st, cards)?;
    for (&u, &from) in cards.iter().zip(&froms) {
        let owner = st.cards[usize::from(u)].owner;
        // `froms` is a dict keyed by uid; a repeated uid has the same value everywhere.
        core_::emit(
            st,
            Ev::Deployed,
            i32::from(u),
            i32::from(owner),
            None,
            &[
                (EvKey::By, by),
                (EvKey::Group, group),
                (EvKey::FromLoc, view::zone_loc_code(from)),
                (EvKey::ExUsed, ex_used),
            ],
        )?;
    }
    Ok(())
}

/// `_enter_rested(st, uids)` (`interp.py:979-986`): GD04-022, GD05-026; one view decides for
/// every card (Inv 06a §14), one touch per card rested.
fn enter_rested(st: &mut GameState, uids: &[Uid]) -> Result<(), EngineError> {
    let dv = view::derived(st)?;
    for &u in uids {
        if !st.cards[usize::from(u)].rested
            && view::rules_of(&dv, u, RuleKind::DeployedRested)
                .next()
                .is_some()
        {
            st.cards[usize::from(u)].rested = true;
            st.touch();
        }
    }
    Ok(())
}

/// `_h_deploy_token` (`interp.py:989-1012`): the batches `[(token_key, count), *also]`; an
/// exceeded non-empty battle area asks first (nothing protected); then one group, tokens
/// created batch by batch up to 6 in all (each recorded), entering rested where a rule says
/// so, DEPLOYED (`token=1`) for each, bound to `var`.
pub(super) fn h_deploy_token(
    st: &mut GameState,
    reg: &Registry,
    fi: usize,
    ins: &DeployToken,
) -> Result<Status, EngineError> {
    let p = player_of(st, &ctx_of(st, fi)?, ins.player);
    let mut batches = vec![(ins.token_key, val(st, fi, ins.count)?)];
    batches.extend(ins.also.iter().copied());
    let mut n: i64 = 0;
    for &(_, k) in &batches {
        n = n.checked_add(k.max(0)).ok_or(EngineError::Overflow {
            what: "token count",
        })?;
    }
    if n <= 0 {
        set_did(st, fi, false);
        return Ok(Status::Next);
    }
    let (ex_u, _) = excess_needed(st, p, n, 0);
    if ex_u > 0 && !st.zone(p, Zone::Battle).is_empty() {
        return ask_excess(st, fi, p, Zone::Battle, &[]);
    }
    let group = to_i32(core_::next_group(st), "event group")?;
    let controller = st.frames[fi].controller;
    let mut made: Vec<Uid> = Vec::new();
    for (key, k) in batches {
        let spec_key = token_key(reg.str(key))?;
        let tdef = st
            .db()
            .token_for_key(&spec_key)
            .ok_or_else(|| EngineError::Key(gcg_data::py_repr(&spec_key)))?
            .def_id;
        let room = limit(BATTLE_LIMIT) - limit(made.len());
        for _ in 0..k.min(room).max(0) {
            let uid = core_::new_card(st, tdef, p, Zone::Battle, ins.rested)?;
            made.push(uid);
            core_::record(
                st,
                HistKind::Deployed,
                p,
                by_of(controller),
                i32::from(uid),
                NO_ARG,
            );
        }
    }
    enter_rested(st, &made)?;
    for &uid in &made {
        core_::emit(
            st,
            Ev::Deployed,
            i32::from(uid),
            i32::from(p),
            None,
            &[
                (EvKey::By, i32::from(controller)),
                (EvKey::Group, group),
                (EvKey::Token, 1),
            ],
        )?;
    }
    set_var_uids(st, fi, ins.var, &made)?;
    set_did(st, fi, !made.is_empty());
    Ok(Status::Next)
}

/// Python's whitespace for `str.strip()` and `int()` on ASCII text.
const fn is_py_ascii_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c | 0x1c..=0x1f)
}

/// `int(text)` for the decimal integers a token key holds: optional surrounding whitespace, an
/// optional sign, ASCII digits with single underscores between them. Any other text is
/// Python's `ValueError` (non-ASCII digits, which `int()` would also accept, cannot occur:
/// `TokenSpec.key` writes `str(int)`, `cards/tokens.py:27-30`).
fn py_int(text: &str) -> Result<i64, EngineError> {
    let invalid = || {
        EngineError::Value(format!(
            "invalid literal for int() with base 10: {}",
            gcg_data::py_repr(text)
        ))
    };
    let bytes = text.as_bytes();
    let start = bytes
        .iter()
        .position(|&b| !is_py_ascii_space(b))
        .unwrap_or(bytes.len());
    let end = bytes
        .iter()
        .rposition(|&b| !is_py_ascii_space(b))
        .map_or(start, |i| i + 1);
    let mut s = &text[start..end];
    let negative = s.starts_with('-');
    if s.starts_with('-') || s.starts_with('+') {
        s = &s[1..];
    }
    if s.is_empty() || s.starts_with('_') || s.ends_with('_') || s.contains("__") {
        return Err(invalid());
    }
    let mut value: i64 = 0;
    for b in s.bytes() {
        if b == b'_' {
            continue;
        }
        if !b.is_ascii_digit() {
            return Err(invalid());
        }
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add(i64::from(b - b'0')))
            .ok_or(EngineError::Overflow { what: "token stat" })?;
    }
    Ok(if negative { -value } else { value })
}

/// `token_spec(key).key` (`interp.py:1018-1024`, `cards/tokens.py:19-30`): the key split as
/// `name, traits, ap, hp, text = key.split("|", 4)`, the traits split on `/` without empty
/// parts, the stats read with `int()`, written back as `TokenSpec.key`. `CardDB.token_for`
/// looks the token up by that key (`cards/db.py:315-316`).
///
/// # Errors
/// Python's `ValueError` for a key with fewer than five parts or a stat that is not an int.
pub(super) fn token_key(key: &str) -> Result<String, EngineError> {
    let parts: Vec<&str> = key.splitn(5, '|').collect();
    let [name, traits, ap, hp, text] = parts[..] else {
        return Err(EngineError::Value(format!(
            "not enough values to unpack (expected 5, got {})",
            parts.len()
        )));
    };
    let traits: Vec<&str> = traits.split('/').filter(|t| !t.is_empty()).collect();
    let (ap, hp) = (py_int(ap)?, py_int(hp)?);
    Ok(format!("{name}|{}|{ap}|{hp}|{text}", traits.join("/")))
}

/// `_h_place_ex` (`interp.py:1027-1030`).
pub(super) fn h_place_ex(
    st: &mut GameState,
    fi: usize,
    ins: &PlaceExResource,
) -> Result<Status, EngineError> {
    let p = player_of(st, &ctx_of(st, fi)?, ins.player);
    let by = i32::from(st.frames[fi].controller);
    let did = place_ex_resource(st, p, ins.rested, by)?;
    set_did(st, fi, did);
    Ok(Status::Next)
}

/// `place_ex_resource(st, p, *, rested, by)` (`interp.py:1033-1042`): rules 4-4-2 and
/// 4-4-2-1 cap the resource area at 15 cards and 5 EX Resources; else an EX Resource token is
/// created, recorded and announced (EX_RESOURCE_PLACED in the current group).
fn place_ex_resource(
    st: &mut GameState,
    p: u8,
    rested: bool,
    by: i32,
) -> Result<bool, EngineError> {
    let area = st.zone(p, Zone::ResourceArea);
    let ex_def = st.db().ex_resource().def_id;
    let n_ex = area
        .iter()
        .filter(|&&u| core_::card_type(st, u) == CardType::ExResource)
        .count();
    if area.len() >= RESOURCE_LIMIT || n_ex >= EX_RESOURCE_LIMIT {
        crate::diag::note_interpretation(crate::diag::Interpretation::I03);
        return Ok(false);
    }
    let uid = core_::new_card(st, ex_def, p, Zone::ResourceArea, rested)?;
    let by_i8 = i8::try_from(by).map_err(|_| EngineError::Overflow { what: "history by" })?;
    core_::record(st, HistKind::ExResource, p, by_i8, i32::from(uid), NO_ARG);
    core_::emit(
        st,
        Ev::ExResourcePlaced,
        i32::from(uid),
        i32::from(p),
        None,
        &[(EvKey::By, by)],
    )?;
    Ok(true)
}

/// `_h_place_resource` (`interp.py:1045-1048`).
pub(super) fn h_place_resource(
    st: &mut GameState,
    fi: usize,
    ins: &PlaceResource,
) -> Result<Status, EngineError> {
    let p = player_of(st, &ctx_of(st, fi)?, ins.player);
    let by = i32::from(st.frames[fi].controller);
    let did = place_resource(st, p, ins.rested, by)?;
    set_did(st, fi, did);
    Ok(Status::Next)
}

/// `place_resource(st, p, *, rested, by)` (`interp.py:1051-1059`): the top card of the
/// resource deck to the resource area unless the deck is empty or the area full;
/// RESOURCE_PLACED in the current group; no record.
///
/// # Errors
/// Any error of `move` or `emit`.
#[allow(
    dead_code,
    reason = "called by layer L5 (game, testkit) and C's tests (docs/port/contracts.md); they land after L4"
)]
pub(crate) fn place_resource(
    st: &mut GameState,
    p: u8,
    rested: bool,
    by: i32,
) -> Result<bool, EngineError> {
    let Some(&uid) = st.zone(p, Zone::ResourceDeck).first() else {
        return Ok(false);
    };
    if st.zone(p, Zone::ResourceArea).len() >= RESOURCE_LIMIT {
        crate::diag::note_interpretation(crate::diag::Interpretation::I03);
        return Ok(false);
    }
    core_::move_card(
        st,
        uid,
        Zone::ResourceArea,
        MoveOpts {
            rested,
            ..MoveOpts::default()
        },
    )?;
    core_::emit(
        st,
        Ev::ResourcePlaced,
        i32::from(uid),
        i32::from(p),
        None,
        &[(EvKey::By, by)],
    )?;
    Ok(true)
}

/// The resource area with normal Resources before EX Resources, area order kept within each
/// (`sorted(area, key=lambda u: card_type(u) is EX_RESOURCE)`, a stable sort; Inv 06a
/// hazard 9).
fn normal_first(st: &GameState, p: u8) -> Vec<Uid> {
    let mut ordered = st.zone(p, Zone::ResourceArea).to_vec();
    ordered.sort_by_key(|&u| core_::card_type(st, u) == CardType::ExResource);
    ordered
}

/// `_h_set_resources_active` (`interp.py:1062-1080`): up to `n` rested Resources (normal ones
/// first) set active, each recorded; one touch if any; no event.
pub(super) fn h_set_resources_active(
    st: &mut GameState,
    fi: usize,
    ins: &SetResourcesActive,
) -> Result<Status, EngineError> {
    let p = player_of(st, &ctx_of(st, fi)?, ins.player);
    let mut n = val(st, fi, ins.count)?;
    let mut did = false;
    let (controller, host) = (st.frames[fi].controller, st.frames[fi].host);
    for u in normal_first(st, p) {
        if n <= 0 {
            break;
        }
        if st.cards[usize::from(u)].rested {
            st.cards[usize::from(u)].rested = false;
            n -= 1;
            did = true;
            core_::record(
                st,
                HistKind::ResourceSetActive,
                p,
                by_of(controller),
                i32::from(u),
                host,
            );
        }
    }
    if did {
        st.touch();
    }
    set_did(st, fi, did);
    Ok(Status::Next)
}

/// `_h_rest_resources` (`interp.py:1083-1100`): up to `n` active Resources (normal ones first)
/// rested; one touch if any; no record, no event.
pub(super) fn h_rest_resources(
    st: &mut GameState,
    fi: usize,
    ins: &RestResources,
) -> Result<Status, EngineError> {
    let p = player_of(st, &ctx_of(st, fi)?, ins.player);
    let mut n = val(st, fi, ins.count)?;
    let mut did = false;
    for u in normal_first(st, p) {
        if n <= 0 {
            break;
        }
        if !st.cards[usize::from(u)].rested {
            st.cards[usize::from(u)].rested = true;
            n -= 1;
            did = true;
        }
    }
    if did {
        st.touch();
    }
    set_did(st, fi, did);
    Ok(Status::Next)
}
