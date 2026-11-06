//! Pairing, playing and paying (`interp.py:1340-1575`): `can_pair`, `Pair` and `pair`,
//! `PlayCard`, `pay_generic`, `ActivateMain`, `StartBattle`, `ChangeAttackTarget`, `BindVar`,
//! `PayCost` and the custom-step dispatcher.

use gcg_data::CardType;
use gcg_effects::{
    ActivateMain, BindVar, ChangeAttackTarget, Ev, HookParams, NodeId, P, Pair, PlayCard, Registry,
    RuleKind, StartBattle, SystemProgram,
};

use super::{
    Status, add_pc, by_of, ctx_of, hooks, in_play, player_of, push_frame, refs, set_did,
    set_var_uids, to_i32, val,
};
use crate::core::{self as core_};
use crate::view;
use crate::{
    Derived, EngineError, EvKey, FrameKind, GameState, HistKind, MoveOpts, NO_ARG, PushFrame, Uid,
    Zone,
};

/// `can_pair(st, dv, unit)` (`interp.py:1340-1344`): a Unit in the battle area without a Pilot
/// and without a CANT_BE_PAIRED rule.
#[allow(
    dead_code,
    reason = "called by layer L5 (game, testkit) and C's tests (docs/dev/contracts.md); they land after L4"
)]
pub(crate) fn can_pair(st: &GameState, dv: &Derived, unit: Uid) -> bool {
    let c = &st.cards[usize::from(unit)];
    c.zone == Zone::Battle
        && c.pair < 0
        && view::rules_of(dv, unit, RuleKind::CantBePaired)
            .next()
            .is_none()
}

/// `_h_pair` (`interp.py:1347-1365`): the first pilot and the first unit reference; the pilot
/// must be pilot-capable and not OUTSIDE, the unit pairable (on the view computed before the
/// checks).
pub(super) fn h_pair(st: &mut GameState, fi: usize, ins: &Pair) -> Result<Status, EngineError> {
    let pilots = refs(st, fi, ins.pilot)?;
    let units = refs(st, fi, ins.unit)?;
    let dv = view::derived(st)?;
    let (Some(&pilot), Some(&unit)) = (pilots.first(), units.first()) else {
        set_did(st, fi, false);
        return Ok(Status::Next);
    };
    if !st.def_of(pilot).is_pilot_capable()
        || !can_pair(st, &dv, unit)
        || st.cards[usize::from(pilot)].zone == Zone::Outside
    {
        set_did(st, fi, false);
        return Ok(Status::Next);
    }
    drop(dv);
    let by = i32::from(st.frames[fi].controller);
    pair(st, pilot, unit, by)?;
    set_did(st, fi, true);
    Ok(Status::Next)
}

/// `pair(st, pilot, unit, *, by)` (`interp.py:1368-1393`): rule 5-9, the Pilot is placed
/// beneath the Unit (one touch by `move`, one here), recorded, then PAIRED and, when the pair
/// is linked on the fresh view, LINKED, both in one new group.
fn pair(st: &mut GameState, pilot: Uid, unit: Uid, by: i32) -> Result<(), EngineError> {
    let from_zone = st.cards[usize::from(pilot)].zone;
    core_::move_card(st, pilot, Zone::Paired, MoveOpts::default())?;
    st.cards[usize::from(pilot)].pair = i32::from(unit);
    st.cards[usize::from(unit)].pair = i32::from(pilot);
    st.touch();
    let owner = st.cards[usize::from(unit)].owner;
    let by_i8 = i8::try_from(by).map_err(|_| EngineError::Overflow { what: "history by" })?;
    core_::record(st, HistKind::Paired, owner, by_i8, i32::from(pilot), NO_ARG);
    let dv = view::derived(st)?;
    let group = to_i32(core_::next_group(st), "event group")?;
    let linked = view::is_linked(&dv, unit);
    drop(dv);
    core_::emit(
        st,
        Ev::Paired,
        i32::from(unit),
        i32::from(owner),
        None,
        &[
            (EvKey::Pilot, i32::from(pilot)),
            (EvKey::By, by),
            (EvKey::Group, group),
            (EvKey::FromLoc, view::zone_loc_code(from_zone)),
            (EvKey::Linked, i32::from(linked)),
        ],
    )?;
    if linked {
        crate::diag::note_interpretation(crate::diag::Interpretation::I17);
        let owner = st.cards[usize::from(unit)].owner;
        core_::emit(
            st,
            Ev::Linked,
            i32::from(unit),
            i32::from(owner),
            None,
            &[
                (EvKey::Pilot, i32::from(pilot)),
                (EvKey::By, by),
                (EvKey::Group, group),
            ],
        )?;
    }
    Ok(())
}

/// `_h_play_card` (`interp.py:1396-1439`; `ignore_level` is unused): unless free, the card's
/// owner pays `max(0, play_cost + cost_delta)`. A Unit or Base is deployed by a `system:deploy`
/// sub-frame (`play_sub`, which advances this frame when it finishes); a Command with a
/// command ability goes to RESOLVING, is recorded and announced (COMMAND_PLAYED) and its
/// program pushed (`command`; this frame advances now). Anything else clears `did`.
pub(super) fn h_play_card(
    st: &mut GameState,
    reg: &Registry,
    fi: usize,
    ins: &PlayCard,
) -> Result<Status, EngineError> {
    let cards = refs(st, fi, ins.r#ref)?;
    let Some(&uid) = cards.first() else {
        set_did(st, fi, false);
        return Ok(Status::Next);
    };
    let t = core_::card_type(st, uid);
    if !ins.free {
        let dv = view::derived(st)?;
        let base = i64::from(view::play_cost(st, &dv, uid)?);
        drop(dv);
        let cost = (base + val(st, fi, ins.cost_delta)?).max(0);
        let owner = st.cards[usize::from(uid)].owner;
        if !pay_generic(st, owner, cost, NO_ARG)? {
            set_did(st, fi, false);
            return Ok(Status::Next);
        }
    }
    let controller = st.frames[fi].controller;
    if t.is_unit() || t.is_base() {
        let vars = st.engine_vars();
        let (play, card) = (vars.play, vars.card);
        st.var_set(fi, play, &[i32::from(uid)])?;
        let deploy = super::system_program(st, SystemProgram::Deploy);
        push_frame(
            st,
            deploy,
            PushFrame {
                controller,
                host: i32::from(uid),
                card_uid: NO_ARG,
                kind: FrameKind::PlaySub,
                event: crate::EventPayload::default(),
                vars: &[(card, &[i32::from(uid)])],
            },
        )?;
        set_did(st, fi, true);
        return Ok(Status::Pushed);
    }
    if t == CardType::Command {
        let entry = reg.card(st.cards[usize::from(uid)].def_id);
        let Some(aid) = entry.command_aid else {
            set_did(st, fi, false);
            return Ok(Status::Next);
        };
        core_::move_card(
            st,
            uid,
            Zone::Resolving,
            MoveOpts {
                reveal: true,
                ..MoveOpts::default()
            },
        )?;
        core_::record(
            st,
            HistKind::CommandActivated,
            controller,
            by_of(controller),
            i32::from(uid),
            NO_ARG,
        );
        core_::emit(
            st,
            Ev::CommandPlayed,
            i32::from(uid),
            i32::from(controller),
            None,
            &[(EvKey::ExUsed, 0)],
        )?;
        let program = ability_program(reg, aid)?;
        push_frame(
            st,
            program,
            PushFrame {
                controller,
                host: i32::from(uid),
                card_uid: NO_ARG,
                kind: FrameKind::Command,
                event: crate::EventPayload::default(),
                vars: &[],
            },
        )?;
        add_pc(st, fi, 1)?;
        set_did(st, fi, true);
        return Ok(Status::Pushed);
    }
    set_did(st, fi, false);
    Ok(Status::Next)
}

/// `R.abilities[aid].program_id` of a command ability, which always has steps.
fn ability_program(
    reg: &Registry,
    aid: gcg_effects::AbilityId,
) -> Result<gcg_effects::ProgramId, EngineError> {
    reg.ability(aid)
        .program
        .ok_or_else(|| EngineError::Internal(format!("ability {} has no program", aid.0)))
}

/// `pay_generic(st, p, cost, ex_used=-1)` (`interp.py:1442-1470`): rule 2-10-1, rest `cost`
/// active Resources; EX Resources used are removed from the game (5-17-3-2-3), each recorded
/// and announced (EXILED with `ex_resource=1`). `ex_used < 0` prefers normal Resources. A
/// cost of 0 or less is paid without a touch; a payment touches once at the end (after the
/// touches of each EX Resource's removal).
///
/// # Errors
/// Any error of `remove_from_game` or `emit`.
#[allow(
    dead_code,
    reason = "called by layer L5 (game, testkit) and C's tests (docs/dev/contracts.md); they land after L4"
)]
pub(crate) fn pay_generic(
    st: &mut GameState,
    p: u8,
    cost: i64,
    ex_used: i32,
) -> Result<bool, EngineError> {
    if cost <= 0 {
        return Ok(true);
    }
    let area = st.zone(p, Zone::ResourceArea);
    let (mut normal, mut ex) = (Vec::new(), Vec::new());
    for &u in area {
        if st.cards[usize::from(u)].rested {
            continue;
        }
        if core_::card_type(st, u) == CardType::ExResource {
            ex.push(u);
        } else {
            normal.push(u);
        }
    }
    let n_normal = i64::try_from(normal.len()).unwrap_or(i64::MAX);
    let n_ex = i64::try_from(ex.len()).unwrap_or(i64::MAX);
    let mut ex_used = i64::from(ex_used);
    if ex_used < 0 {
        ex_used = (cost - n_normal).max(0);
    }
    if ex_used > n_ex || cost - ex_used > n_normal || ex_used > cost {
        return Ok(false);
    }
    let k_normal = usize::try_from(cost - ex_used).unwrap_or(0);
    let k_ex = usize::try_from(ex_used).unwrap_or(0);
    for &u in &normal[..k_normal] {
        st.cards[usize::from(u)].rested = true;
    }
    for &u in &ex[..k_ex] {
        st.cards[usize::from(u)].rested = true;
        core_::remove_from_game(st, u)?;
        core_::record(st, HistKind::ExExiled, p, by_of(p), i32::from(u), NO_ARG);
        core_::emit(
            st,
            Ev::Exiled,
            i32::from(u),
            i32::from(p),
            None,
            &[(EvKey::By, i32::from(p)), (EvKey::ExResource, 1)],
        )?;
    }
    st.touch();
    Ok(true)
}

/// `_h_activate_main` (`interp.py:1473-1492`): a card's command effect activated by another
/// effect (rule 3-4-6-3-1): recorded, announced (COMMAND_PLAYED with `as_command=1`) and its
/// program pushed (`activate_main_sub`, which passes its `did` up); the card is not moved.
pub(super) fn h_activate_main(
    st: &mut GameState,
    reg: &Registry,
    fi: usize,
    ins: &ActivateMain,
) -> Result<Status, EngineError> {
    let cards = refs(st, fi, ins.r#ref)?;
    let Some(&uid) = cards.first() else {
        set_did(st, fi, false);
        return Ok(Status::Next);
    };
    let entry = reg.card(st.cards[usize::from(uid)].def_id);
    let Some(aid) = entry.command_aid else {
        set_did(st, fi, false);
        return Ok(Status::Next);
    };
    let controller = st.frames[fi].controller;
    core_::record(
        st,
        HistKind::CommandActivated,
        controller,
        by_of(controller),
        i32::from(uid),
        NO_ARG,
    );
    core_::emit(
        st,
        Ev::CommandPlayed,
        i32::from(uid),
        i32::from(controller),
        None,
        &[(EvKey::ExUsed, 0), (EvKey::AsCommand, 1)],
    )?;
    let program = ability_program(reg, aid)?;
    push_frame(
        st,
        program,
        PushFrame {
            controller,
            host: i32::from(uid),
            card_uid: NO_ARG,
            kind: FrameKind::ActivateMainSub,
            event: crate::EventPayload::default(),
            vars: &[],
        },
    )?;
    Ok(Status::Pushed)
}

/// `_h_start_battle` (`interp.py:1495-1505`; `damage_only` is unused): the first attacker and
/// the first target in play fight a damage-only battle.
pub(super) fn h_start_battle(
    st: &mut GameState,
    fi: usize,
    ins: &StartBattle,
) -> Result<Status, EngineError> {
    let attackers = in_play(st, &refs(st, fi, ins.attacker)?);
    let targets = in_play(st, &refs(st, fi, ins.target)?);
    let (Some(&attacker), Some(&target)) = (attackers.first(), targets.first()) else {
        set_did(st, fi, false);
        return Ok(Status::Next);
    };
    crate::battle::damage_only_battle(st, attacker, target)?;
    set_did(st, fi, true);
    Ok(Status::Next)
}

/// `_h_change_target` (`interp.py:1508-1518`): the live battle's target becomes the first
/// in-play reference (with its `zone_seq`), touched.
pub(super) fn h_change_target(
    st: &mut GameState,
    fi: usize,
    ins: &ChangeAttackTarget,
) -> Result<Status, EngineError> {
    let live = st.battle().is_some_and(|b| !b.ended);
    let targets = in_play(st, &refs(st, fi, ins.target)?);
    let (true, Some(&target)) = (live, targets.first()) else {
        set_did(st, fi, false);
        return Ok(Status::Next);
    };
    let seq = st.cards[usize::from(target)].zone_seq;
    let Some(b) = st.battles.last_mut() else {
        return Err(EngineError::Internal("battle vanished".to_owned()));
    };
    b.target = i32::from(target);
    b.target_seq = seq;
    st.touch();
    set_did(st, fi, true);
    Ok(Status::Next)
}

/// `_h_bind` (`interp.py:1521-1523`).
pub(super) fn h_bind(st: &mut GameState, fi: usize, ins: &BindVar) -> Result<Status, EngineError> {
    let items = refs(st, fi, ins.r#ref)?;
    set_var_uids(st, fi, ins.var, &items)?;
    Ok(Status::Next)
}

/// `_h_pay_cost` (`interp.py:1562-1568`): `did` is whether the player paid; a positive paid
/// amount with a host is recorded and announced (COST_PAID).
pub(super) fn pay_cost(
    st: &mut GameState,
    fi: usize,
    amount: i64,
    player: P,
) -> Result<Status, EngineError> {
    let p = player_of(st, &ctx_of(st, fi)?, player);
    let did = pay_generic(st, p, amount, NO_ARG)?;
    set_did(st, fi, did);
    let host = st.frames[fi].host;
    if did && amount > 0 && host >= 0 {
        core_::record(st, HistKind::CostPaid, p, by_of(p), host, NO_ARG);
        core_::emit(
            st,
            Ev::CostPaid,
            host,
            i32::from(p),
            None,
            &[(EvKey::Amount, to_i32(amount, "cost paid")?)],
        )?;
    }
    Ok(Status::Next)
}

/// `_h_custom` (`interp.py:1571-1575`): `did` is the hook's result, and the state is touched
/// after every custom step, whatever it did (PLAN §5.3). The parameters are the node's typed
/// `dict(params)` (`Registry::hook_params`).
pub(super) fn h_custom(
    st: &mut GameState,
    reg: &Registry,
    fi: usize,
    ins: NodeId,
) -> Result<Status, EngineError> {
    let Some(HookParams::Step(params)) = reg.hook_params(ins) else {
        return Err(EngineError::Internal(
            "a CustomStep without typed step parameters".to_owned(),
        ));
    };
    let did = hooks::run_step(st, reg, fi, *params)?;
    set_did(st, fi, did);
    st.touch();
    Ok(Status::Next)
}
