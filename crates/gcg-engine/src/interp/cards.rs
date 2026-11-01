//! Card and zone actions (`interp.py:477-873`, `:1103-1246`): drawing, discarding, damage,
//! destruction, resting (with rest substitution), setting active, returning to hand or deck,
//! exiling, trashing, adding to hand or shields, milling, looking, arranging, revealing,
//! shuffling and recovering (Inv 06a §13.4).

use gcg_effects::{
    AddToHand, AddToShields, Arrange, Damage, Destroy, Discard, Draw, Ev, Exile, LookTop, Mill, P,
    Recover, Registry, Rest, ReturnToHand, Reveal, RuleKind, SetActive, ShieldToHand, Shuffle,
    Side, ToDeck, ToTrash, ValueRef,
};

use super::{
    Status, as_ints, by_of, ctx_of, decide, in_play, player_of, refs, rule_mod, set_did,
    set_var_uids, simple_ctx, to_i32, to_uid, to_uids, val,
};
use crate::core::{self as core_};
use crate::view;
use crate::{
    Action, ActionKind, BOTH_KNOW, CtxKey, DamageOpts, DecisionKind, Derived, EngineError,
    EnginePrompt, EvKey, GameState, HistKind, IntKey, MoveOpts, NO_ARG, OnceElem, OnceKey, Prompt,
    REST_SUB_TAG, Uid, Zone,
};

/// The current `event_group` as an event payload value.
fn group_value(group: i64) -> Result<i32, EngineError> {
    to_i32(group, "event group")
}

/// `_h_draw` (`interp.py:489-497`): `each_player` draws for the active player, then the
/// standby player; `did = drawn > 0`.
pub(super) fn h_draw(st: &mut GameState, fi: usize, ins: &Draw) -> Result<Status, EngineError> {
    let n = val(st, fi, ins.count)?;
    if ins.each_player {
        let first = draw(st, st.active, n, true)?;
        let second = draw(st, 1 - st.active, n, true)?;
        set_did(st, fi, first + second > 0);
        return Ok(Status::Next);
    }
    let p = player_of(st, &ctx_of(st, fi)?, ins.player);
    let drawn = draw(st, p, n, true)?;
    set_did(st, fi, drawn > 0);
    Ok(Status::Next)
}

/// `draw(st, p, n, *, by_effect=False)` (`interp.py:500-512`): one new event group (always),
/// then up to `max(0, n)` cards from the top of the deck, each moved, recorded and announced
/// (DRAWN) at once; stops at an empty deck. Returns the number drawn.
///
/// # Errors
/// Any error of `move`, `emit` or the payload conversions.
#[allow(
    dead_code,
    reason = "called by layer L5 (game, testkit) and C's tests (docs/port/contracts.md); they land after L4"
)]
pub(crate) fn draw(st: &mut GameState, p: u8, n: i64, by_effect: bool) -> Result<i64, EngineError> {
    let mut drawn = 0;
    let group = group_value(core_::next_group(st))?;
    for _ in 0..n.max(0) {
        let Some(&uid) = st.zone(p, Zone::Deck).first() else {
            break;
        };
        core_::move_card(st, uid, Zone::Hand, MoveOpts::default())?;
        drawn += 1;
        core_::record(st, HistKind::Draw, p, by_of(p), i32::from(uid), NO_ARG);
        core_::emit(
            st,
            Ev::Drawn,
            i32::from(uid),
            i32::from(p),
            None,
            &[
                (EvKey::Group, group),
                (EvKey::ByEffect, i32::from(by_effect)),
            ],
        )?;
    }
    Ok(drawn)
}

/// `_discard_candidates(st, f, ins, p)` (`interp.py:515-520`): the hand in order, filtered by
/// `V.matches` with the frame's context when the instruction has filters.
fn discard_candidates(
    st: &GameState,
    fi: usize,
    ins: &Discard,
    p: u8,
) -> Result<Vec<Uid>, EngineError> {
    let hand = st.zone(p, Zone::Hand).to_vec();
    if ins.filters.is_empty() {
        return Ok(hand);
    }
    let dv = view::derived(st)?;
    let ctx = ctx_of(st, fi)?;
    let mut out = Vec::with_capacity(hand.len());
    for u in hand {
        if view::matches(st, &dv, &ctx, u, &ins.filters)? {
            out.push(u);
        }
    }
    Ok(out)
}

/// The chooser of a discard (`interp.py:539`, `:554`): the discarding player unless the
/// instruction names one.
fn discard_chooser(st: &GameState, fi: usize, ins: &Discard, p: u8) -> Result<u8, EngineError> {
    Ok(match ins.chooser {
        None => p,
        Some(c) => player_of(st, &ctx_of(st, fi)?, c),
    })
}

/// `_h_discard` (`interp.py:523-541`): nothing to discard binds `()` and clears `did`; a random
/// discard draws `randrange(len(pool))` `min(n, len(pool))` times; at most `n` candidates are
/// all discarded; else a DISCARD decision with the candidates in hand order.
pub(super) fn h_discard(
    st: &mut GameState,
    fi: usize,
    ins: &Discard,
) -> Result<Status, EngineError> {
    let p = player_of(st, &ctx_of(st, fi)?, ins.player);
    let n = val(st, fi, ins.count)?;
    let cands = discard_candidates(st, fi, ins, p)?;
    st.buf_set(fi, &[])?;
    if n <= 0 || cands.is_empty() {
        st.var_set(fi, ins.var, &[])?;
        set_did(st, fi, false);
        return Ok(Status::Next);
    }
    if ins.random {
        let mut pool = cands;
        let draws = usize::try_from(n).map_or(pool.len(), |n| n.min(pool.len()));
        let mut chosen = Vec::with_capacity(draws);
        for _ in 0..draws {
            let j = st.rng.randrange_usize(pool.len());
            chosen.push(pool.remove(j));
        }
        return do_discard(st, fi, ins, p, &chosen);
    }
    if i64::try_from(cands.len()).unwrap_or(i64::MAX) <= n {
        return do_discard(st, fi, ins, p, &cands);
    }
    let chooser = discard_chooser(st, fi, ins, p)?;
    let opts: Vec<Action> = cands
        .iter()
        .map(|&u| Action::new(ActionKind::Select, i32::from(u), NO_ARG, NO_ARG))
        .collect();
    decide(
        st,
        fi,
        chooser,
        DecisionKind::Discard,
        &opts,
        Prompt::Engine(EnginePrompt::Discard),
        &[(CtxKey::Count, to_i32(n, "discard count")?)],
    )
}

/// `_r_discard` (`interp.py:544-556`): appends the pick; discards the picks in pick order once
/// `n` are picked or none is left; else asks again with the candidates above the highest
/// picked uid, or every candidate when there is none (Inv 06a hazard 22).
pub(super) fn r_discard(
    st: &mut GameState,
    fi: usize,
    ins: &Discard,
    action: Action,
) -> Result<Status, EngineError> {
    let p = player_of(st, &ctx_of(st, fi)?, ins.player);
    let n = val(st, fi, ins.count)?;
    st.buf_push(fi, action.a)?;
    let buf = st.buf(fi).to_vec();
    let cands: Vec<Uid> = discard_candidates(st, fi, ins, p)?
        .into_iter()
        .filter(|&u| !buf.contains(&i32::from(u)))
        .collect();
    let picked = i64::try_from(buf.len()).unwrap_or(i64::MAX);
    if picked >= n || cands.is_empty() {
        let uids = to_uids(st, &buf)?;
        return do_discard(st, fi, ins, p, &uids);
    }
    let highest = buf.iter().copied().max().unwrap_or(NO_ARG);
    let mut opts: Vec<Action> = cands
        .iter()
        .filter(|&&u| i32::from(u) > highest)
        .map(|&u| Action::new(ActionKind::Select, i32::from(u), NO_ARG, NO_ARG))
        .collect();
    if opts.is_empty() {
        opts = cands
            .iter()
            .map(|&u| Action::new(ActionKind::Select, i32::from(u), NO_ARG, NO_ARG))
            .collect();
    }
    let chooser = discard_chooser(st, fi, ins, p)?;
    decide(
        st,
        fi,
        chooser,
        DecisionKind::Discard,
        &opts,
        Prompt::Engine(EnginePrompt::Discard),
        &[(CtxKey::Count, to_i32(n - picked, "discard count")?)],
    )?;
    Ok(Status::Wait)
}

/// `_do_discard(st, f, ins, p, uids)` (`interp.py:559-564`).
fn do_discard(
    st: &mut GameState,
    fi: usize,
    ins: &Discard,
    p: u8,
    uids: &[Uid],
) -> Result<Status, EngineError> {
    let by = st.frames[fi].controller;
    discard(st, p, uids, by)?;
    set_var_uids(st, fi, ins.var, uids)?;
    st.buf_set(fi, &[])?;
    set_did(st, fi, !uids.is_empty());
    Ok(Status::Next)
}

/// `discard(st, p, uids, by)` (`interp.py:567-573`): one group; every card to the trash with a
/// record, then DISCARDED for each.
fn discard(st: &mut GameState, p: u8, uids: &[Uid], by: u8) -> Result<(), EngineError> {
    let group = group_value(core_::next_group(st))?;
    for &u in uids {
        core_::move_card(st, u, Zone::Trash, MoveOpts::default())?;
        core_::record(st, HistKind::Discard, p, by_of(by), i32::from(u), NO_ARG);
    }
    for &u in uids {
        core_::emit(
            st,
            Ev::Discarded,
            i32::from(u),
            i32::from(p),
            None,
            &[(EvKey::By, i32::from(by)), (EvKey::Group, group)],
        )?;
    }
    Ok(())
}

/// The effect-damage options of a frame (`source=f.host, battle=False, by=f.controller`).
fn effect_damage(st: &GameState, fi: usize) -> DamageOpts {
    let f = &st.frames[fi];
    DamageOpts {
        source: f.host,
        battle: false,
        by: i32::from(f.controller),
    }
}

/// `_h_damage` (`interp.py:576-593`): one group (always); a Shield that would receive the
/// damage is destroyed (its own group), any other card is dealt damage.
pub(super) fn h_damage(st: &mut GameState, fi: usize, ins: &Damage) -> Result<Status, EngineError> {
    let targets = refs(st, fi, ins.r#ref)?;
    let amount = val(st, fi, ins.amount)?;
    let mut dealt = false;
    core_::next_group(st);
    let d = effect_damage(st, fi);
    for u in targets {
        if st.cards[usize::from(u)].zone == Zone::Shield {
            if !core_::shields_receiving_damage(st, &[u], amount, d)?.is_empty() {
                let owner = st.cards[usize::from(u)].owner;
                core_::destroy_shields(st, owner, &[u], d)?;
                dealt = true;
            }
            continue;
        }
        if core_::damage_card(st, u, amount, d)? > 0 {
            dealt = true;
        }
    }
    set_did(st, fi, dealt);
    Ok(Status::Next)
}

/// `_h_damage_shield_area` (`interp.py:596-604`) and `_h_damage_player` (`:607-613`, one
/// card): `did` is whether the shield area had a card and the amount was positive, even when
/// the damage was prevented.
pub(super) fn h_damage_shield_area(
    st: &mut GameState,
    fi: usize,
    player: P,
    amount: ValueRef,
    cards: i64,
) -> Result<Status, EngineError> {
    let p = player_of(st, &ctx_of(st, fi)?, player);
    let amount = val(st, fi, amount)?;
    let had = !st.zone(p, Zone::Base).is_empty() || !st.zone(p, Zone::Shield).is_empty();
    let cards = usize::try_from(cards)
        .map_err(|_| EngineError::Internal(format!("negative shield-area card count {cards}")))?;
    let d = effect_damage(st, fi);
    core_::damage_shield_area(st, p, amount, cards, d)?;
    set_did(st, fi, had && amount > 0);
    Ok(Status::Next)
}

/// `_h_destroy` (`interp.py:616-623`): the references in BATTLE, BASE or PAIRED; `destroy` is
/// called even with none (it computes the view first).
pub(super) fn h_destroy(
    st: &mut GameState,
    fi: usize,
    ins: &Destroy,
) -> Result<Status, EngineError> {
    let targets: Vec<Uid> = refs(st, fi, ins.r#ref)?
        .into_iter()
        .filter(|&u| {
            matches!(
                st.cards[usize::from(u)].zone,
                Zone::Battle | Zone::Base | Zone::Paired
            )
        })
        .collect();
    let f = &st.frames[fi];
    let (by, source) = (i32::from(f.controller), f.host);
    let destroyed = core_::destroy(st, &targets, false, by, source)?;
    set_did(st, fi, !destroyed.is_empty());
    Ok(Status::Next)
}

/// `_rest_substitutes(st, f, target, taken)` (`interp.py:626-652`): the controller's
/// unrested BATTLE then BASE cards (not the target, not `taken`) whose first matching
/// REST_SUBSTITUTE rule allows resting them instead of `target` (rule 10-1-9).
fn rest_substitutes(
    st: &GameState,
    reg: &Registry,
    fi: usize,
    target: Uid,
    taken: &[i32],
) -> Result<Vec<Uid>, EngineError> {
    let tc = st.cards[usize::from(target)];
    let (controller, host) = (st.frames[fi].controller, st.frames[fi].host);
    if tc.owner != controller || host < 0 {
        return Ok(Vec::new());
    }
    let dv = view::derived(st)?;
    let kind = if tc.zone == Zone::Base {
        "base"
    } else {
        "unit"
    };
    let mut out = Vec::new();
    for z in [Zone::Battle, Zone::Base] {
        for &u in st.zone(tc.owner, z) {
            if u == target || taken.contains(&i32::from(u)) || st.cards[usize::from(u)].rested {
                continue;
            }
            for r in view::rules_of(&dv, u, RuleKind::RestSubstitute) {
                let m = rule_mod(reg, r.rule)?;
                let name = reg.str(m.name);
                if !name.is_empty() && name != kind {
                    continue;
                }
                if !m.source_filters.is_empty()
                    && !view::matches(
                        st,
                        &dv,
                        &simple_ctx(tc.owner, i32::from(u)),
                        to_uid(st, host)?,
                        &m.source_filters,
                    )?
                {
                    continue;
                }
                if m.once_per_turn && st.once_contains(&rest_sub_key(i32::from(u), st.turn)?) {
                    continue;
                }
                out.push(u);
                break;
            }
        }
    }
    Ok(out)
}

/// `(core.REST_SUB_TAG, u, st.turn)` (`interp.py:648`, `:674`).
fn rest_sub_key(uid: i32, turn: i32) -> Result<OnceKey, EngineError> {
    OnceKey::from_elems(&[
        OnceElem::Int(REST_SUB_TAG),
        OnceElem::Int(uid),
        OnceElem::Int(turn),
    ])
}

/// `_rest_targets(st, f, ins)` (`interp.py:655-661`): the references in BATTLE, BASE or the
/// resource area that are not rested (recomputed on every call).
fn rest_targets(st: &GameState, fi: usize, ins: &Rest) -> Result<Vec<Uid>, EngineError> {
    Ok(refs(st, fi, ins.r#ref)?
        .into_iter()
        .filter(|&u| {
            let c = &st.cards[usize::from(u)];
            matches!(c.zone, Zone::Battle | Zone::Base | Zone::ResourceArea) && !c.rested
        })
        .collect())
}

/// `_r_rest` (`interp.py:668-676`): a SELECT stores the substitute for target `__rest_i` and
/// uses its once-per-turn substitution; DONE stores nothing; then the next target.
pub(super) fn r_rest(
    st: &mut GameState,
    reg: &Registry,
    fi: usize,
    ins: &Rest,
    action: Action,
) -> Result<Status, EngineError> {
    let i = st.int_get(fi, IntKey::RestI).unwrap_or(0);
    if action.kind == ActionKind::Select {
        st.int_set(fi, rest_as(i)?, action.a)?;
        let dv = view::derived(st)?;
        let sub = to_uid(st, action.a)?;
        let mut once = false;
        for r in view::rules_of(&dv, sub, RuleKind::RestSubstitute) {
            if rule_mod(reg, r.rule)?.once_per_turn {
                once = true;
                break;
            }
        }
        if once {
            st.once_add(rest_sub_key(action.a, st.turn)?);
        }
    }
    st.int_set(fi, IntKey::RestI, to_i32(i64::from(i) + 1, "__rest_i")?)?;
    h_rest_continue(st, reg, fi, ins)
}

/// `f"__rest_as{i}"`.
fn rest_as(i: i32) -> Result<IntKey, EngineError> {
    u16::try_from(i)
        .map(IntKey::RestAs)
        .map_err(|_| EngineError::Overflow {
            what: "__rest_as index",
        })
}

/// `_h_rest` / `_h_rest_continue` (`interp.py:664-713`): offers a substitution for every
/// target without one yet (SELECT substitutes then DONE, asked of the target's owner); then
/// one group (always) and every final card still unrested in BATTLE, BASE or the resource area
/// is rested, touched and announced (RESTED).
pub(super) fn h_rest_continue(
    st: &mut GameState,
    reg: &Registry,
    fi: usize,
    ins: &Rest,
) -> Result<Status, EngineError> {
    let targets = rest_targets(st, fi, ins)?;
    let mut done = st.int_get(fi, IntKey::RestI).unwrap_or(0);
    while usize::try_from(done).is_ok_and(|d| d < targets.len()) {
        let d = usize::try_from(done).unwrap_or(usize::MAX);
        if st.int_get(fi, rest_as(done)?).is_some() {
            done += 1;
            continue;
        }
        let mut taken = as_ints(&targets);
        for e in st.frame_ints(&st.frames[fi]) {
            if matches!(e.key, IntKey::RestAs(_)) && !taken.contains(&e.value) {
                taken.push(e.value);
            }
        }
        let subs = rest_substitutes(st, reg, fi, targets[d], &taken)?;
        if !subs.is_empty() {
            st.int_set(fi, IntKey::RestI, done)?;
            let mut opts: Vec<Action> = subs
                .iter()
                .map(|&u| Action::new(ActionKind::Select, i32::from(u), NO_ARG, NO_ARG))
                .collect();
            opts.push(Action::of(ActionKind::Done));
            let owner = st.cards[usize::from(targets[d])].owner;
            return decide(
                st,
                fi,
                owner,
                DecisionKind::Select,
                &opts,
                Prompt::Engine(EnginePrompt::RestInstead),
                &[(CtxKey::RestSub, i32::from(targets[d]))],
            );
        }
        done += 1;
    }
    st.int_del(fi, IntKey::RestI);
    let mut finals = Vec::with_capacity(targets.len());
    for (i, &t) in targets.iter().enumerate() {
        let key = rest_as(super::idx_i32(i)?)?;
        finals.push(match st.int_del(fi, key) {
            Some(sub) => to_uid(st, sub)?,
            None => t,
        });
    }
    let mut did = false;
    let group = group_value(core_::next_group(st))?;
    let by = i32::from(st.frames[fi].controller);
    for u in finals {
        let c = st.cards[usize::from(u)];
        if matches!(c.zone, Zone::Battle | Zone::Base | Zone::ResourceArea) && !c.rested {
            st.cards[usize::from(u)].rested = true;
            st.touch();
            did = true;
            core_::emit(
                st,
                Ev::Rested,
                i32::from(u),
                i32::from(c.owner),
                None,
                &[(EvKey::By, by), (EvKey::Group, group)],
            )?;
        }
    }
    set_did(st, fi, did);
    Ok(Status::Next)
}

/// `_h_set_active` (`interp.py:716-732`): one view before the loop decides CANT_BE_SET_ACTIVE
/// for every card (Inv 06a §14); one group (always); each rested card in BATTLE, BASE or the
/// resource area is set active, touched, recorded when it is a Resource, and announced.
pub(super) fn h_set_active(
    st: &mut GameState,
    fi: usize,
    ins: &SetActive,
) -> Result<Status, EngineError> {
    let mut did = false;
    let dv = view::derived(st)?;
    let group = group_value(core_::next_group(st))?;
    let (controller, host) = (st.frames[fi].controller, st.frames[fi].host);
    for u in refs(st, fi, ins.r#ref)? {
        let c = st.cards[usize::from(u)];
        if matches!(c.zone, Zone::Battle | Zone::Base | Zone::ResourceArea) && c.rested {
            if view::rules_of(&dv, u, RuleKind::CantBeSetActive)
                .next()
                .is_some()
            {
                continue;
            }
            st.cards[usize::from(u)].rested = false;
            st.touch();
            did = true;
            if c.zone == Zone::ResourceArea {
                core_::record(
                    st,
                    HistKind::ResourceSetActive,
                    c.owner,
                    by_of(controller),
                    i32::from(u),
                    host,
                );
            }
            core_::emit(
                st,
                Ev::SetActive,
                i32::from(u),
                i32::from(c.owner),
                None,
                &[(EvKey::By, i32::from(controller)), (EvKey::Group, group)],
            )?;
        }
    }
    set_did(st, fi, did);
    Ok(Status::Next)
}

/// `_returnable(st, dv, u, by)` (`interp.py:735-740`): no CANT_BE_RETURNED rule, except one
/// limited to enemy sources when the returning player owns the card.
fn returnable(
    st: &GameState,
    reg: &Registry,
    dv: &Derived,
    u: Uid,
    by: u8,
) -> Result<bool, EngineError> {
    for r in view::rules_of(dv, u, RuleKind::CantBeReturned) {
        if rule_mod(reg, r.rule)?.source_side == Side::Enemy && by == st.cards[usize::from(u)].owner
        {
            continue;
        }
        return Ok(false);
    }
    Ok(true)
}

/// `_h_return_to_hand` (`interp.py:743-752`): the references in BATTLE, BASE, PAIRED, TRASH
/// or RESOLVING that can be returned, judged on one view.
pub(super) fn h_return_to_hand(
    st: &mut GameState,
    reg: &Registry,
    fi: usize,
    ins: &ReturnToHand,
) -> Result<Status, EngineError> {
    let dv = view::derived(st)?;
    let controller = st.frames[fi].controller;
    let mut targets = Vec::new();
    for u in refs(st, fi, ins.r#ref)? {
        if matches!(
            st.cards[usize::from(u)].zone,
            Zone::Battle | Zone::Base | Zone::Paired | Zone::Trash | Zone::Resolving
        ) && returnable(st, reg, &dv, u, controller)?
        {
            targets.push(u);
        }
    }
    let did = return_to_hand(st, &targets, controller)?;
    set_did(st, fi, did);
    Ok(Status::Next)
}

/// `return_to_hand(st, targets, by)` (`interp.py:755-778`): last-known information of the
/// targets in play, one group, every target not already OUTSIDE to its owner's hand, then
/// RETURNED_TO_HAND for each moved card with its own last-known information.
fn return_to_hand(st: &mut GameState, targets: &[Uid], by: u8) -> Result<bool, EngineError> {
    if targets.is_empty() {
        return Ok(false);
    }
    let on_field = in_play(st, targets);
    let lki = core_::ability_snapshot(st, &on_field)?;
    let group = group_value(core_::next_group(st))?;
    let mut moved = Vec::with_capacity(targets.len());
    for &u in targets {
        if st.cards[usize::from(u)].zone == Zone::Outside {
            continue;
        }
        core_::move_card(st, u, Zone::Hand, MoveOpts::default())?;
        moved.push(u);
    }
    for &u in &moved {
        let own = lki.only(u);
        let owner = st.cards[usize::from(u)].owner;
        core_::emit(
            st,
            Ev::ReturnedToHand,
            i32::from(u),
            i32::from(owner),
            Some(&own),
            &[(EvKey::By, i32::from(by)), (EvKey::Group, group)],
        )?;
    }
    Ok(!moved.is_empty())
}

/// `_h_to_deck` (`interp.py:781-812`): the targets (and the Pilots of targeted Units, captured
/// before moving) go to their owners' decks; with `shuffle` each owner's deck is shuffled in
/// ascending owner order; otherwise the owner orders every group of more than one card placed
/// in one deck (rules 4-1-6, 4-1-7), hidden from the other player.
pub(super) fn h_to_deck(
    st: &mut GameState,
    fi: usize,
    ins: &ToDeck,
) -> Result<Status, EngineError> {
    let todeck = st.engine_vars().todeck;
    if st.var_get(fi, todeck).is_none() {
        let targets: Vec<Uid> = refs(st, fi, ins.r#ref)?
            .into_iter()
            .filter(|&u| st.cards[usize::from(u)].zone != Zone::Outside)
            .collect();
        let mut pilots = Vec::new();
        for &u in &targets {
            let c = &st.cards[usize::from(u)];
            if c.zone == Zone::Battle && c.pair >= 0 {
                pilots.push(to_uid(st, c.pair)?);
            }
        }
        let mut owners: Vec<u8> = Vec::new();
        for &u in &targets {
            let owner = st.cards[usize::from(u)].owner;
            if !owners.contains(&owner) {
                owners.push(owner);
            }
            core_::move_card(
                st,
                u,
                Zone::Deck,
                MoveOpts {
                    bottom: ins.bottom,
                    ..MoveOpts::default()
                },
            )?;
        }
        set_did(st, fi, !targets.is_empty());
        if ins.shuffle {
            owners.sort();
            for p in owners {
                core_::shuffle_deck(st, p);
            }
            return Ok(Status::Next);
        }
        let moved: Vec<Uid> = targets
            .iter()
            .chain(&pilots)
            .copied()
            .filter(|&u| st.cards[usize::from(u)].zone == Zone::Deck)
            .collect();
        let owner_of = |u: Uid| st.cards[usize::from(u)].owner;
        let mut together: Vec<Uid> = moved
            .iter()
            .copied()
            .filter(|&u| {
                moved
                    .iter()
                    .filter(|&&v| owner_of(v) == owner_of(u))
                    .count()
                    > 1
            })
            .collect();
        if together.is_empty() {
            return Ok(Status::Next);
        }
        for &u in &together {
            let c = &mut st.cards[usize::from(u)];
            c.known &= 1 << c.owner;
        }
        together.sort_by_key(|&u| (st.cards[usize::from(u)].owner, u));
        set_var_uids(st, fi, todeck, &together)?;
        st.buf_set(fi, &[])?;
    }
    todeck_next(st, fi, ins)
}

/// `_todeck_next(st, f, ins)` (`interp.py:815-830`): owner by owner (ascending), an ARRANGE
/// decision while more than one of the owner's cards is unplaced, else the owner's cards in
/// pick order are placed; done deletes `__todeck` and clears the buffer.
fn todeck_next(st: &mut GameState, fi: usize, ins: &ToDeck) -> Result<Status, EngineError> {
    let todeck = st.engine_vars().todeck;
    let Some(group) = st.var_get(fi, todeck).map(<[i32]>::to_vec) else {
        return Err(EngineError::Key("'__todeck'".to_owned()));
    };
    let group = to_uids(st, &group)?;
    let unplaced = |st: &GameState, u: Uid| !st.buf(fi).contains(&i32::from(u));
    let mut pending_owners: Vec<u8> = Vec::new();
    for &u in &group {
        let owner = st.cards[usize::from(u)].owner;
        if unplaced(st, u) && !pending_owners.contains(&owner) {
            pending_owners.push(owner);
        }
    }
    pending_owners.sort();
    for owner in pending_owners {
        let remaining: Vec<Uid> = group
            .iter()
            .copied()
            .filter(|&u| st.cards[usize::from(u)].owner == owner && unplaced(st, u))
            .collect();
        if remaining.len() > 1 {
            let opts: Vec<Action> = remaining
                .iter()
                .map(|&u| Action::new(ActionKind::Select, i32::from(u), NO_ARG, NO_ARG))
                .collect();
            return decide(
                st,
                fi,
                owner,
                DecisionKind::Arrange,
                &opts,
                Prompt::Engine(EnginePrompt::OrderTopFirst),
                &[(CtxKey::Owner, i32::from(owner))],
            );
        }
        let mut chosen = Vec::new();
        for &x in st.buf(fi) {
            let u = to_uid(st, x)?;
            if st.cards[usize::from(u)].owner == owner {
                chosen.push(u);
            }
        }
        chosen.extend_from_slice(&remaining);
        place_ordered(st, owner, &chosen, ins.bottom)?;
        for &u in &remaining {
            st.buf_push(fi, i32::from(u))?;
        }
    }
    st.var_del(fi, todeck);
    st.buf_set(fi, &[])?;
    Ok(Status::Next)
}

/// `_r_to_deck` (`interp.py:833-835`).
pub(super) fn r_to_deck(
    st: &mut GameState,
    fi: usize,
    ins: &ToDeck,
    action: Action,
) -> Result<Status, EngineError> {
    st.buf_push(fi, action.a)?;
    todeck_next(st, fi, ins)
}

/// The references that are not OUTSIDE.
fn live_refs(st: &GameState, fi: usize, r: gcg_effects::RefId) -> Result<Vec<Uid>, EngineError> {
    Ok(refs(st, fi, r)?
        .into_iter()
        .filter(|&u| st.cards[usize::from(u)].zone != Zone::Outside)
        .collect())
}

/// `_h_exile` (`interp.py:838-846`): one group (always), every live target to REMOVAL, then
/// EXILED for each.
pub(super) fn h_exile(st: &mut GameState, fi: usize, ins: &Exile) -> Result<Status, EngineError> {
    let targets = live_refs(st, fi, ins.r#ref)?;
    let group = group_value(core_::next_group(st))?;
    for &u in &targets {
        core_::move_card(st, u, Zone::Removal, MoveOpts::default())?;
    }
    let by = i32::from(st.frames[fi].controller);
    for &u in &targets {
        let owner = st.cards[usize::from(u)].owner;
        core_::emit(
            st,
            Ev::Exiled,
            i32::from(u),
            i32::from(owner),
            None,
            &[(EvKey::By, by), (EvKey::Group, group)],
        )?;
    }
    set_did(st, fi, !targets.is_empty());
    Ok(Status::Next)
}

/// `_h_to_trash` (`interp.py:849-860`): rule 5-10-1, an effect placing a Unit or Base from the
/// field into the trash destroys it; any other live target is moved without an event.
pub(super) fn h_to_trash(
    st: &mut GameState,
    fi: usize,
    ins: &ToTrash,
) -> Result<Status, EngineError> {
    let targets = live_refs(st, fi, ins.r#ref)?;
    let field: Vec<Uid> = targets
        .iter()
        .copied()
        .filter(|&u| matches!(st.cards[usize::from(u)].zone, Zone::Battle | Zone::Base))
        .collect();
    let rest: Vec<Uid> = targets
        .iter()
        .copied()
        .filter(|u| !field.contains(u))
        .collect();
    let destroyed = if field.is_empty() {
        Vec::new()
    } else {
        let f = &st.frames[fi];
        let (by, source) = (i32::from(f.controller), f.host);
        core_::destroy(st, &field, false, by, source)?
    };
    for &u in &rest {
        core_::move_card(st, u, Zone::Trash, MoveOpts::default())?;
    }
    set_did(st, fi, !destroyed.is_empty() || !rest.is_empty());
    Ok(Status::Next)
}

/// `_h_add_to_hand` (`interp.py:863-873`): the targets not in HAND or OUTSIDE; one group
/// (always); all moved (revealed when asked), then ADDED_TO_HAND for each.
pub(super) fn h_add_to_hand(
    st: &mut GameState,
    fi: usize,
    ins: &AddToHand,
) -> Result<Status, EngineError> {
    let targets: Vec<Uid> = refs(st, fi, ins.r#ref)?
        .into_iter()
        .filter(|&u| !matches!(st.cards[usize::from(u)].zone, Zone::Hand | Zone::Outside))
        .collect();
    let group = group_value(core_::next_group(st))?;
    for &u in &targets {
        core_::move_card(
            st,
            u,
            Zone::Hand,
            MoveOpts {
                reveal: ins.reveal,
                ..MoveOpts::default()
            },
        )?;
    }
    let by = i32::from(st.frames[fi].controller);
    for &u in &targets {
        let owner = st.cards[usize::from(u)].owner;
        core_::emit(
            st,
            Ev::AddedToHand,
            i32::from(u),
            i32::from(owner),
            None,
            &[(EvKey::By, by), (EvKey::Group, group)],
        )?;
    }
    set_did(st, fi, !targets.is_empty());
    Ok(Status::Next)
}

/// `zones[p][z][:max(0, n)]` copied before any card moves (Inv 06a hazard 26).
fn top_cards(st: &GameState, p: u8, z: Zone, n: i64) -> Vec<Uid> {
    let list = st.zone(p, z);
    let k = usize::try_from(n.max(0)).map_or(list.len(), |k| k.min(list.len()));
    list[..k].to_vec()
}

/// `_h_shield_to_hand` (`interp.py:1103-1114`): the top `n` Shields; one group (always); each
/// moved and recorded, then SHIELD_TO_HAND for each.
pub(super) fn h_shield_to_hand(
    st: &mut GameState,
    fi: usize,
    ins: &ShieldToHand,
) -> Result<Status, EngineError> {
    let p = player_of(st, &ctx_of(st, fi)?, ins.player);
    let n = val(st, fi, ins.count)?;
    let shields = top_cards(st, p, Zone::Shield, n);
    let group = group_value(core_::next_group(st))?;
    let controller = st.frames[fi].controller;
    for &u in &shields {
        core_::move_card(st, u, Zone::Hand, MoveOpts::default())?;
        core_::record(
            st,
            HistKind::ShieldToHand,
            p,
            by_of(controller),
            i32::from(u),
            NO_ARG,
        );
    }
    for &u in &shields {
        core_::emit(
            st,
            Ev::ShieldToHand,
            i32::from(u),
            i32::from(p),
            None,
            &[(EvKey::By, i32::from(controller)), (EvKey::Group, group)],
        )?;
    }
    set_did(st, fi, !shields.is_empty());
    Ok(Status::Next)
}

/// `_h_add_to_shields` (`interp.py:1117-1127`): the top `n` deck cards, or the live
/// references, moved in order to the top (default) or bottom of the Shields; no event.
pub(super) fn h_add_to_shields(
    st: &mut GameState,
    fi: usize,
    ins: &AddToShields,
) -> Result<Status, EngineError> {
    let p = player_of(st, &ctx_of(st, fi)?, ins.player);
    let cards = match ins.r#ref {
        None => {
            let n = val(st, fi, ins.count)?;
            top_cards(st, p, Zone::Deck, n)
        }
        Some(r) => live_refs(st, fi, r)?,
    };
    for &u in &cards {
        core_::move_card(
            st,
            u,
            Zone::Shield,
            MoveOpts {
                bottom: !ins.top,
                ..MoveOpts::default()
            },
        )?;
    }
    set_did(st, fi, !cards.is_empty());
    Ok(Status::Next)
}

/// `_h_mill` (`interp.py:1130-1138`): the top `n` deck cards to the trash in order, bound to
/// `var`; no event.
pub(super) fn h_mill(st: &mut GameState, fi: usize, ins: &Mill) -> Result<Status, EngineError> {
    let p = player_of(st, &ctx_of(st, fi)?, ins.player);
    let n = val(st, fi, ins.count)?;
    let cards = top_cards(st, p, Zone::Deck, n);
    for &u in &cards {
        core_::move_card(st, u, Zone::Trash, MoveOpts::default())?;
    }
    set_var_uids(st, fi, ins.var, &cards)?;
    set_did(st, fi, !cards.is_empty());
    Ok(Status::Next)
}

/// `_h_look_top` (`interp.py:1141-1150`): the top `n` deck cards become known to the
/// controller (to both players when revealed), bound to `var`; no touch.
pub(super) fn h_look_top(
    st: &mut GameState,
    fi: usize,
    ins: &LookTop,
) -> Result<Status, EngineError> {
    let p = player_of(st, &ctx_of(st, fi)?, ins.player);
    let n = val(st, fi, ins.count)?;
    let cards = top_cards(st, p, Zone::Deck, n);
    let bits = if ins.reveal {
        BOTH_KNOW
    } else {
        1 << st.frames[fi].controller
    };
    for &u in &cards {
        st.cards[usize::from(u)].known |= bits;
    }
    set_var_uids(st, fi, ins.var, &cards)?;
    set_did(st, fi, !cards.is_empty());
    Ok(Status::Next)
}

/// The references in the DECK (`interp.py:1154`, `:1205`).
fn deck_refs(st: &GameState, fi: usize, r: gcg_effects::RefId) -> Result<Vec<Uid>, EngineError> {
    Ok(refs(st, fi, r)?
        .into_iter()
        .filter(|&u| st.cards[usize::from(u)].zone == Zone::Deck)
        .collect())
}

/// The two arrange modes that place cards on the top or the bottom (`interp.py:1160`).
fn is_top_or_bottom(mode: &str) -> bool {
    mode == "top_or_bottom" || mode == "top_or_bottom_each"
}

/// `_h_arrange` (`interp.py:1153-1191`). `top_or_bottom` with several cards asks once for all
/// of them; `top_or_bottom[_each]` otherwise asks card by card; the other modes ask for the
/// next card (top first) until at most one is left, then place the order on the top, or on the
/// bottom for `bottom_any_order`. No knowledge bit changes.
pub(super) fn h_arrange(
    st: &mut GameState,
    reg: &Registry,
    fi: usize,
    ins: &Arrange,
) -> Result<Status, EngineError> {
    let cards = deck_refs(st, fi, ins.r#ref)?;
    let Some(&first) = cards.first() else {
        set_did(st, fi, false);
        return Ok(Status::Next);
    };
    let p = st.cards[usize::from(first)].owner;
    let chooser = player_of(st, &ctx_of(st, fi)?, ins.player);
    let mode = reg.str(ins.mode);
    let remaining: Vec<Uid> = cards
        .iter()
        .copied()
        .filter(|&u| !st.buf(fi).contains(&i32::from(u)))
        .collect();
    if is_top_or_bottom(mode) {
        if mode == "top_or_bottom" && cards.len() > 1 {
            return decide(
                st,
                fi,
                chooser,
                DecisionKind::Arrange,
                &[
                    Action::new(ActionKind::Select, 0, NO_ARG, NO_ARG),
                    Action::new(ActionKind::Select, 1, NO_ARG, NO_ARG),
                ],
                Prompt::Engine(EnginePrompt::Top0OrBottom1),
                &[(CtxKey::All, 1)],
            );
        }
        let Some(&next) = remaining.first() else {
            st.buf_set(fi, &[])?;
            set_did(st, fi, true);
            return Ok(Status::Next);
        };
        let next = i32::from(next);
        return decide(
            st,
            fi,
            chooser,
            DecisionKind::Arrange,
            &[
                Action::new(ActionKind::Select, 0, next, NO_ARG),
                Action::new(ActionKind::Select, 1, next, NO_ARG),
            ],
            Prompt::Engine(EnginePrompt::Top0OrBottom1),
            &[(CtxKey::Card, next)],
        );
    }
    if remaining.len() <= 1 {
        let mut order = to_uids(st, st.buf(fi))?;
        order.extend_from_slice(&remaining);
        place_ordered(st, p, &order, mode == "bottom_any_order")?;
        st.buf_set(fi, &[])?;
        set_did(st, fi, true);
        return Ok(Status::Next);
    }
    let opts: Vec<Action> = remaining
        .iter()
        .map(|&u| Action::new(ActionKind::Select, i32::from(u), NO_ARG, NO_ARG))
        .collect();
    decide(
        st,
        fi,
        chooser,
        DecisionKind::Arrange,
        &opts,
        Prompt::Engine(EnginePrompt::NextCardTopFirst),
        &[(CtxKey::Order, 1)],
    )
}

/// `_place_ordered(st, p, order, *, bottom)` (`interp.py:1194-1202`): removes each card from
/// the deck (first occurrence; `ValueError` when absent), puts the order on the bottom or the
/// top, and touches.
pub(super) fn place_ordered(
    st: &mut GameState,
    p: u8,
    order: &[Uid],
    bottom: bool,
) -> Result<(), EngineError> {
    for &u in order {
        st.zone_remove(p, Zone::Deck, u)?;
    }
    if bottom {
        st.zone_extend(p, Zone::Deck, order)?;
    } else {
        st.zone_insert_front(p, Zone::Deck, order)?;
    }
    st.touch();
    Ok(())
}

/// `_r_arrange` (`interp.py:1205-1218`).
pub(super) fn r_arrange(
    st: &mut GameState,
    reg: &Registry,
    fi: usize,
    ins: &Arrange,
    action: Action,
) -> Result<Status, EngineError> {
    let cards = deck_refs(st, fi, ins.r#ref)?;
    let Some(&first) = cards.first() else {
        return Err(EngineError::Index("list index out of range".to_owned()));
    };
    let p = st.cards[usize::from(first)].owner;
    let mode = reg.str(ins.mode);
    if mode == "top_or_bottom" && cards.len() > 1 {
        place_ordered(st, p, &cards, action.a == 1)?;
        set_did(st, fi, true);
        return Ok(Status::Next);
    }
    if is_top_or_bottom(mode) {
        let u = if action.b == NO_ARG {
            first
        } else {
            to_uid(st, action.b)?
        };
        place_ordered(st, p, &[u], action.a == 1)?;
        st.buf_push(fi, i32::from(u))?;
        return h_arrange(st, reg, fi, ins);
    }
    st.buf_push(fi, action.a)?;
    h_arrange(st, reg, fi, ins)
}

/// `_h_reveal` (`interp.py:1221-1225`): both players know the references; no touch.
pub(super) fn h_reveal(st: &mut GameState, fi: usize, ins: &Reveal) -> Result<Status, EngineError> {
    for u in refs(st, fi, ins.r#ref)? {
        st.cards[usize::from(u)].known = BOTH_KNOW;
    }
    set_did(st, fi, true);
    Ok(Status::Next)
}

/// `_h_shuffle` (`interp.py:1228-1231`).
pub(super) fn h_shuffle(
    st: &mut GameState,
    fi: usize,
    ins: &Shuffle,
) -> Result<Status, EngineError> {
    let p = player_of(st, &ctx_of(st, fi)?, ins.player);
    core_::shuffle_deck(st, p);
    set_did(st, fi, true);
    Ok(Status::Next)
}

/// `_h_recover` (`interp.py:1234-1246`): every damaged reference in play recovers
/// `min(damage, amount)` (rule 5-6-3), touched and announced (RECOVERED with the amount).
pub(super) fn h_recover(
    st: &mut GameState,
    fi: usize,
    ins: &Recover,
) -> Result<Status, EngineError> {
    let amount = val(st, fi, ins.amount)?;
    let mut did = false;
    let targets = in_play(st, &refs(st, fi, ins.r#ref)?);
    for u in targets {
        let c = st.cards[usize::from(u)];
        if c.damage > 0 && amount > 0 {
            let healed = i64::from(c.damage).min(amount);
            let healed = to_i32(healed, "recovered amount")?;
            st.cards[usize::from(u)].damage -= healed;
            st.touch();
            did = true;
            core_::emit(
                st,
                Ev::Recovered,
                i32::from(u),
                i32::from(c.owner),
                None,
                &[(EvKey::Amount, healed)],
            )?;
        }
    }
    set_did(st, fi, did);
    Ok(Status::Next)
}
