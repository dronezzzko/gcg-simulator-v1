//! Layer L4 of the engine (docs/port/PLAN.md §5.3-§5.4, §10.1; workstream B2): the effect
//! interpreter of `engine/interp.py`, which executes the flat programs of the IR one
//! instruction at a time, plus the 13 custom step hooks (`effects/builtins.py` and
//! `effects/bindings/*.py`) and the three system programs.
//!
//! An instruction that needs a player's choice sets `st.pending` and marks its frame as
//! waiting (`decide`); `resume` consumes the chosen action and continues. Custom steps never
//! request choices (`interp.py:1-6`).
//!
//! Every function is a port of the Python function its doc comment names, statement by
//! statement: each `derived()` call, `touch()`, `next_group()`, RNG draw, decision option
//! order and iteration order is Python's (Inv 06a §12-§16). A frame is addressed by its index
//! `fi` in `st.frames` and re-read after any call that can push frames, which is how Python's
//! `f` stays valid when a sub-frame is pushed (PLAN §5.4). The registry is held through one
//! `Arc` clone per `run_top_frame`/`resume`, so handlers borrow IR nodes while they mutate the
//! state.
//!
//! Python sources are cited relative to `src/gcg_sim/`; "Inv 06a" is
//! docs/port/inventory/06a-engine-core-interp.md.

mod cards;
mod choose;
mod deploy;
mod hooks;
mod lasting;
mod play;

use std::sync::Arc;

use gcg_effects::{
    Node, NodeId, NodeKind, P, ProgramId, Registry, RuleMod, RuleModId, SelId, SystemProgram,
};

use crate::{
    Action, Ctx, CtxKey, CtxVars, DecisionHdr, DecisionKind, EngineError, Frame, FrameKind, IntKey,
    MoveOpts, NO_ARG, Prompt, PushFrame, Span, Uid, Zone,
};

// The layer boundary of L4 (docs/port/contracts.md, gcg-engine internal boundary).
#[allow(
    unused_imports,
    reason = "called by layer L5 (game, testkit) and C's tests (docs/port/contracts.md); they land after L4"
)]
pub(crate) use self::{
    cards::draw,
    choose::targets_available,
    deploy::{deploy_cards, place_resource},
    lasting::add_lasting,
    play::{can_pair, pay_generic},
};

use crate::GameState;
use crate::view::player_of;

/// `Status` (`interp.py:28-32`): what an instruction did to the control flow. `Jumped`
/// handlers set `pc` themselves; `Wait` and `Pushed` end `run_top_frame`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Status {
    Next,
    Jumped,
    Wait,
    Pushed,
}

/// `isinstance(ins, NON_ACTIONS)` (`interp.py:41-54`): the instructions whose `did` never makes
/// a frame count as having acted (once-per-turn consumption, `finish_frame`).
const fn is_non_action(kind: NodeKind) -> bool {
    matches!(
        kind,
        NodeKind::Jump
            | NodeKind::JumpIfNot
            | NodeKind::AskMay
            | NodeKind::JumpIfNotDid
            | NodeKind::ModeSelect
            | NodeKind::LoopInit
            | NodeKind::LoopNext
            | NodeKind::SetDid
            | NodeKind::HoldRules
            | NodeKind::Choose
            | NodeKind::ChooseMode
            | NodeKind::BindVar
    )
}

// ---------------------------------------------------------------------------------------------
// Small conversions shared by the handlers

/// An `int` that Rust stores as `i32` (event payload values, decision context, frame ints):
/// a value outside the type is [`EngineError::Overflow`] (PLAN §4.1 item 6), never a wrap.
fn to_i32(x: i64, what: &'static str) -> Result<i32, EngineError> {
    i32::try_from(x).map_err(|_| EngineError::Overflow { what })
}

/// A player index as the `by` of a history record (`core.record`'s `by`, `core.py:142-151`).
fn by_of(p: u8) -> i8 {
    if p == 0 { 0 } else { 1 }
}

/// The uid in an int slot (`f.vars` items, `Action.a`, `f.host`) as an index of `st.cards`.
/// Python indexes `st.cards[x]`; every interpreter site that does so holds a uid there
/// (Inv 06a hazard 3), so an out-of-range value is Python's `IndexError`.
fn to_uid(st: &GameState, x: i32) -> Result<Uid, EngineError> {
    match Uid::try_from(x) {
        Ok(u) if usize::from(u) < st.cards.len() => Ok(u),
        _ => Err(EngineError::Index("list index out of range".to_owned())),
    }
}

/// A list of int-slot uids as card indices ([`to_uid`] for each).
fn to_uids(st: &GameState, xs: &[i32]) -> Result<Vec<Uid>, EngineError> {
    xs.iter().map(|&x| to_uid(st, x)).collect()
}

/// Uids as the ints Python stores in `f.vars` and `f.buf`.
fn as_ints(uids: &[Uid]) -> Vec<i32> {
    uids.iter().map(|&u| i32::from(u)).collect()
}

/// A jump target or an advanced `pc` stored in `Frame.pc`.
fn pc_of(x: i64) -> Result<u16, EngineError> {
    u16::try_from(x).map_err(|_| EngineError::Overflow { what: "frame pc" })
}

/// `f.pc = target`.
fn set_pc(st: &mut GameState, fi: usize, target: i64) -> Result<(), EngineError> {
    st.frame_mut(fi).pc = pc_of(target)?;
    Ok(())
}

/// `f.pc += n`.
fn add_pc(st: &mut GameState, fi: usize, n: usize) -> Result<(), EngineError> {
    let pc = i64::from(st.frames[fi].pc)
        + i64::try_from(n).map_err(|_| EngineError::Overflow { what: "frame pc" })?;
    set_pc(st, fi, pc)
}

/// `f.did = value`.
fn set_did(st: &mut GameState, fi: usize, value: bool) {
    st.frame_mut(fi).did = value;
}

/// `f.vars.get(var, ())` copied out of the frame arena.
fn var_items(st: &GameState, fi: usize, var: gcg_effects::VarId) -> Vec<i32> {
    st.var_get(fi, var).map(<[i32]>::to_vec).unwrap_or_default()
}

/// `f.vars[var] = tuple(uids)`.
fn set_var_uids(
    st: &mut GameState,
    fi: usize,
    var: gcg_effects::VarId,
    uids: &[Uid],
) -> Result<(), EngineError> {
    st.var_set(fi, var, &as_ints(uids))
}

/// The `RuleMod` of a rule instance (`r.rule`, `view.py:53-63`).
fn rule_mod(reg: &Registry, id: RuleModId) -> Result<&RuleMod, EngineError> {
    match reg.node(id.node()) {
        Node::RuleMod(m) => Ok(m),
        other => Err(EngineError::Internal(format!(
            "rule handle names a {} node",
            other.kind().py_name()
        ))),
    }
}

/// The `Sel` node of a selector handle.
fn sel_node(reg: &Registry, id: SelId) -> Result<&gcg_effects::Sel, EngineError> {
    match reg.node(id.node()) {
        Node::Sel(s) => Ok(s),
        other => Err(EngineError::Internal(format!(
            "selector handle names a {} node",
            other.kind().py_name()
        ))),
    }
}

/// `V.Ctx(controller, host)` (`engine/view.py:32-44`): no vars, no event, `card_uid` falls
/// back to `host`.
fn simple_ctx(controller: u8, host: i32) -> Ctx {
    Ctx {
        controller,
        host,
        card_uid: host,
        vars: CtxVars::Empty,
        event: crate::EventPayload::default(),
    }
}

/// A list index stored in an `Action` or a decision context.
fn idx_i32(i: usize) -> Result<i32, EngineError> {
    i32::try_from(i).map_err(|_| EngineError::Overflow { what: "list index" })
}

/// `ctx_of(f)` (`interp.py:57-58`): the frame's controller, host, card and event, sharing the
/// frame's vars (`CtxVars::Frame`, so hook writes are visible to later steps, Inv 06a §12.2).
/// `Ctx.__init__`'s `card_uid = card_uid if card_uid != -1 else host` (`view.py:42`) applies.
fn ctx_of(st: &GameState, fi: usize) -> Result<Ctx, EngineError> {
    let f = &st.frames[fi];
    let at = u16::try_from(fi).map_err(|_| EngineError::Capacity {
        what: "frames",
        capacity: usize::from(u16::MAX) + 1,
    })?;
    Ok(Ctx {
        controller: f.controller,
        host: f.host,
        card_uid: if f.card_uid != NO_ARG {
            f.card_uid
        } else {
            f.host
        },
        vars: CtxVars::Frame(at),
        event: f.event,
    })
}

/// `_refs(st, f, ref)` (`interp.py:481-482`): `V.resolve_ref` with the frame's context, which
/// computes the derived view only for references that select cards (`view.py:834-850`).
fn refs(st: &GameState, fi: usize, r: gcg_effects::RefId) -> Result<crate::UidBuf, EngineError> {
    crate::view::resolve_ref(st, &ctx_of(st, fi)?, r)
}

/// `_val(st, f, v)` (`interp.py:485-486`): `V.value(st, V.derived(st), ctx_of(f), v)`; the view
/// is computed (and cached) even for a literal, as in Python.
fn val(st: &GameState, fi: usize, v: gcg_effects::ValueRef) -> Result<i64, EngineError> {
    let dv = crate::view::derived(st)?;
    crate::view::value(st, &dv, &ctx_of(st, fi)?, v)
}

/// `_in_play(st, uids)` (`interp.py:477-478`): the uids in BATTLE or BASE, in order.
fn in_play(st: &GameState, uids: &[Uid]) -> Vec<Uid> {
    uids.iter()
        .copied()
        .filter(|&u| matches!(st.cards[usize::from(u)].zone, Zone::Battle | Zone::Base))
        .collect()
}

// ---------------------------------------------------------------------------------------------
// Frames and decisions

/// `push_frame(st, program_id, *, controller, host, card_uid=-1, kind, event=(), vars=None)`
/// (`interp.py:61-88`). `host_seq` is the host's `zone_seq` (0 without a host) and the vars
/// are copied into the new frame. Python also appends every event value whose key starts with
/// `"it"` followed by digits to `vars["it"]` (`:84-86`); no [`crate::EvKey`] has that form
/// (Inv 06a §12.2, unit-tested below), so that loop has nothing to do. Returns the new frame's
/// index (Python returns the frame).
///
/// # Errors
/// Python's `IndexError` for a host outside `st.cards`; arena overflow.
#[allow(
    dead_code,
    reason = "called by layer L5 (game, testkit) and C's tests (docs/port/contracts.md); they land after L4"
)]
pub(crate) fn push_frame(
    st: &mut GameState,
    program: ProgramId,
    f: PushFrame<'_>,
) -> Result<usize, EngineError> {
    let host_seq = if f.host >= 0 {
        st.cards[usize::from(to_uid(st, f.host)?)].zone_seq
    } else {
        0
    };
    let frame = Frame {
        program,
        controller: f.controller,
        host: f.host,
        host_seq,
        card_uid: if f.card_uid != NO_ARG {
            f.card_uid
        } else {
            f.host
        },
        kind: f.kind,
        event: f.event,
        pc: 0,
        vars: Span::EMPTY,
        did: true,
        answer: false,
        waiting: false,
        buf: Span::EMPTY,
        ints: Span::EMPTY,
        once_key: None,
        acted: false,
    };
    st.frame_push(frame, f.vars)
}

/// `_decide(st, f, player, kind, options, prompt, context=())` (`interp.py:91-102`): marks the
/// frame as waiting and sets `st.pending`; returns `Wait`.
fn decide(
    st: &mut GameState,
    fi: usize,
    player: u8,
    kind: DecisionKind,
    options: &[Action],
    prompt: Prompt,
    context: &[(CtxKey, i32)],
) -> Result<Status, EngineError> {
    st.frame_mut(fi).waiting = true;
    let mut hdr = DecisionHdr {
        player,
        kind,
        prompt,
        context: gcg_core::small::InlineVec::new(),
    };
    for &(key, value) in context {
        hdr.push_context(key, value)?;
    }
    st.set_pending(hdr, options);
    Ok(Status::Wait)
}

/// The index of the top frame (`st.frames[-1]`): Python's `IndexError` when there is none.
fn top_frame(st: &GameState) -> Result<usize, EngineError> {
    st.frames
        .len()
        .checked_sub(1)
        .ok_or_else(|| EngineError::Index("list index out of range".to_owned()))
}

/// The `acted` rule of `run_top_frame` and `resume` (`interp.py:120-123`, `:168-171`):
/// `PUSHED`, or `NEXT` with `f.did` after an instruction that is not in `NON_ACTIONS`.
fn mark_acted(st: &mut GameState, reg: &Registry, fi: usize, ins: NodeId, status: Status) {
    let f = st.frame_mut(fi);
    if status == Status::Pushed
        || (status == Status::Next && f.did && !is_non_action(reg.node(ins).kind()))
    {
        f.acted = true;
    }
}

/// `HOLD_RULES not in f.ints` (`interp.py:125`, `:174`).
fn rules_held(st: &GameState, fi: usize) -> bool {
    st.int_get(fi, IntKey::HoldRules).is_some()
}

/// `run_top_frame(st)` (`interp.py:109-132`): executes the top frame until it finishes, waits
/// for a decision or pushes a sub-frame. After every instruction: the `acted` rule, `pc += 1`
/// on `Next`, the game-over check, rules management unless the frame holds
/// `__hold_rules` (also after `Wait`, Inv 06a hazard 20), the game-over check again, and a
/// return on `Wait`/`Pushed`. A frame that runs off its end is popped and finished.
///
/// # Errors
/// `IndexError` without frames; any error of a handler or of rules management.
#[allow(
    dead_code,
    reason = "called by layer L5 (game, testkit) and C's tests (docs/port/contracts.md); they land after L4"
)]
pub(crate) fn run_top_frame(st: &mut GameState) -> Result<(), EngineError> {
    let reg = Arc::clone(st.registry());
    let fi = top_frame(st)?;
    let prog = reg.program(st.frames[fi].program);
    while usize::from(st.frames[fi].pc) < prog.instrs.len() {
        let ins = prog.instrs[usize::from(st.frames[fi].pc)];
        let status = execute(st, &reg, fi, ins)?;
        mark_acted(st, &reg, fi, ins, status);
        if status == Status::Next {
            add_pc(st, fi, 1)?;
        }
        if crate::core::over(st) {
            return Ok(());
        }
        if !rules_held(st, fi) {
            crate::core::rules_management(st)?;
        }
        if crate::core::over(st) {
            return Ok(());
        }
        if matches!(status, Status::Wait | Status::Pushed) {
            return Ok(());
        }
    }
    // The loop only continues after Next/Jumped, and no handler pushes a frame without
    // returning Pushed, so the frame that ran off its end is the top one (`interp.py:131`).
    if fi + 1 != st.frames.len() {
        return Err(EngineError::Internal(
            "the finished frame is not the top frame".to_owned(),
        ));
    }
    let Some(f) = st.frame_pop() else {
        return Err(EngineError::Index("pop from empty list".to_owned()));
    };
    finish_frame(st, &f)
}

/// `finish_frame(st, f)` (`interp.py:135-150`): consumes the once key of a frame that acted;
/// a finished Command or Burst still in RESOLVING goes to the trash (a Command also emits
/// COMMAND_RESOLVED); an `activate_main_sub` frame advances its parent and passes its `did`
/// up, a `play_sub` frame only advances its parent.
fn finish_frame(st: &mut GameState, f: &Frame) -> Result<(), EngineError> {
    if let Some(key) = f.once_key
        && !key.elems().is_empty()
        && f.acted
    {
        st.once_add(key);
    }
    match f.kind {
        FrameKind::Command | FrameKind::Burst => {
            let uid = to_uid(st, f.card_uid)?;
            if st.cards[usize::from(uid)].zone == Zone::Resolving {
                crate::core::move_card(st, uid, Zone::Trash, MoveOpts::default())?;
                if f.kind == FrameKind::Command {
                    crate::core::emit(
                        st,
                        gcg_effects::Ev::CommandResolved,
                        f.card_uid,
                        i32::from(f.controller),
                        None,
                        &[],
                    )?;
                }
            }
        }
        FrameKind::ActivateMainSub if !st.frames.is_empty() => {
            let parent = st.frames.len() - 1;
            add_pc(st, parent, 1)?;
            set_did(st, parent, f.did);
        }
        FrameKind::PlaySub if !st.frames.is_empty() => {
            let parent = st.frames.len() - 1;
            add_pc(st, parent, 1)?;
        }
        _ => {}
    }
    Ok(())
}

/// `execute(st, f, ins)` (`interp.py:153-157`): the handler of the instruction's exact type
/// (`_HANDLERS`, `interp.py:1597-1646`); any other node is `EngineError("no handler for
/// {type}")`.
fn execute(
    st: &mut GameState,
    reg: &Registry,
    fi: usize,
    ins: NodeId,
) -> Result<Status, EngineError> {
    match reg.node(ins) {
        Node::Jump(x) => h_jump(st, fi, x.target),
        Node::JumpIfNot(x) => h_jump_if_not(st, fi, x),
        Node::JumpIfNotDid(x) => h_jump_if_not_did(st, fi, x.target),
        Node::SetDid(x) => {
            set_did(st, fi, x.value);
            Ok(Status::Next)
        }
        Node::HoldRules(x) => h_hold_rules(st, fi, x.delta),
        Node::AskMay(x) => h_ask_may(st, reg, fi, x),
        Node::ModeSelect(x) => h_mode(st, fi, x),
        Node::LoopInit(x) => h_loop_init(st, fi, x),
        Node::LoopNext(x) => h_loop_next(st, reg, fi, x),
        Node::Choose(x) => choose::h_choose(st, reg, fi, x),
        Node::Draw(x) => cards::h_draw(st, fi, x),
        Node::Discard(x) => cards::h_discard(st, fi, x),
        Node::Damage(x) => cards::h_damage(st, fi, x),
        Node::DamageShieldArea(x) => {
            cards::h_damage_shield_area(st, fi, x.player, x.amount, x.cards)
        }
        Node::DamagePlayer(x) => cards::h_damage_shield_area(st, fi, x.player, x.amount, 1),
        Node::Destroy(x) => cards::h_destroy(st, fi, x),
        Node::Rest(x) => cards::h_rest_continue(st, reg, fi, x),
        Node::SetActive(x) => cards::h_set_active(st, fi, x),
        Node::ReturnToHand(x) => cards::h_return_to_hand(st, reg, fi, x),
        Node::ToDeck(x) => cards::h_to_deck(st, fi, x),
        Node::Exile(x) => cards::h_exile(st, fi, x),
        Node::ToTrash(x) => cards::h_to_trash(st, fi, x),
        Node::AddToHand(x) => cards::h_add_to_hand(st, fi, x),
        Node::DeployCard(x) => deploy::h_deploy_card(st, fi, x),
        Node::DeployToken(x) => deploy::h_deploy_token(st, reg, fi, x),
        Node::PlaceExResource(x) => deploy::h_place_ex(st, fi, x),
        Node::PlaceResource(x) => deploy::h_place_resource(st, fi, x),
        Node::SetResourcesActive(x) => deploy::h_set_resources_active(st, fi, x),
        Node::RestResources(x) => deploy::h_rest_resources(st, fi, x),
        Node::ShieldToHand(x) => cards::h_shield_to_hand(st, fi, x),
        Node::AddToShields(x) => cards::h_add_to_shields(st, fi, x),
        Node::Mill(x) => cards::h_mill(st, fi, x),
        Node::LookTop(x) => cards::h_look_top(st, fi, x),
        Node::Arrange(x) => cards::h_arrange(st, reg, fi, x),
        Node::Reveal(x) => cards::h_reveal(st, fi, x),
        Node::Shuffle(x) => cards::h_shuffle(st, fi, x),
        Node::Recover(x) => cards::h_recover(st, fi, x),
        Node::Apply(x) => lasting::h_apply(st, reg, fi, x),
        Node::ApplyPlayer(x) => lasting::h_apply_player(st, reg, fi, x),
        Node::Pair(x) => play::h_pair(st, fi, x),
        Node::PlayCard(x) => play::h_play_card(st, reg, fi, x),
        Node::ActivateMain(x) => play::h_activate_main(st, reg, fi, x),
        Node::StartBattle(x) => play::h_start_battle(st, fi, x),
        Node::ChangeAttackTarget(x) => play::h_change_target(st, fi, x),
        Node::BindVar(x) => play::h_bind(st, fi, x),
        Node::DelayedTrigger(_) => lasting::h_delayed(st, reg, fi, ins),
        Node::PayCost(x) => play::pay_cost(st, fi, x.amount, x.player),
        Node::CustomStep(_) => play::h_custom(st, reg, fi, ins),
        other => Err(EngineError::Internal(format!(
            "no handler for {}",
            other.kind().py_name()
        ))),
    }
}

/// A step a custom hook executes with `interp.execute(st, f, <node built at run time>)`
/// (`wp_gd03_a.py:107`, `:126`; PLAN §5.3): the same handler code as the IR instruction,
/// with the node's fields as plain values.
#[derive(Clone, Copy, Debug)]
enum Synth {
    /// `d.PayCost(cost)`: `player` is `P.YOU`.
    PayCost { amount: i64 },
    /// `d.Apply(d.Var(var), d.StatMod(ap=-n), Duration.THIS_TURN)` with `ap = -n`.
    ApplyVarStatMod { var: gcg_effects::VarId, ap: i32 },
}

/// `interp.execute(st, f, node)` for a [`Synth`] node.
fn execute_synth(
    st: &mut GameState,
    reg: &Registry,
    fi: usize,
    ins: Synth,
) -> Result<Status, EngineError> {
    match ins {
        Synth::PayCost { amount } => play::pay_cost(st, fi, amount, P::You),
        Synth::ApplyVarStatMod { var, ap } => {
            // _refs(st, f, Var(var)): a Var never selects, so resolve_ref reads the frame's
            // vars without the view (view.py:788-789, :834-850).
            let targets = to_uids(st, &var_items(st, fi, var))?;
            lasting::apply_to(
                st,
                reg,
                fi,
                &targets,
                None,
                lasting::Effect::StatModValue { ap, hp: 0 },
                gcg_effects::Duration::ThisTurn,
            )
        }
    }
}

/// `resume(st, action)` (`interp.py:160-175`), called by `game.apply` while the top frame
/// waits: clears `waiting`, runs the resumer of the instruction at `pc` (`_RESUMERS`,
/// `interp.py:1648-1658`), applies the `acted` rule and `pc += 1` on `Next`, then rules
/// management unless held; no game-over checks.
///
/// # Errors
/// `EngineError("no resumer for {type}")`; `IndexError` without frames or past the program's
/// end; any error of the resumer or of rules management.
#[allow(
    dead_code,
    reason = "called by layer L5 (game, testkit) and C's tests (docs/port/contracts.md); they land after L4"
)]
pub(crate) fn resume(st: &mut GameState, action: Action) -> Result<(), EngineError> {
    let reg = Arc::clone(st.registry());
    let fi = top_frame(st)?;
    st.frame_mut(fi).waiting = false;
    let prog = reg.program(st.frames[fi].program);
    let ins = *prog
        .instrs
        .get(usize::from(st.frames[fi].pc))
        .ok_or_else(|| EngineError::Index("tuple index out of range".to_owned()))?;
    let status = match reg.node(ins) {
        Node::AskMay(x) => r_ask_may(st, fi, x, action),
        Node::ModeSelect(x) => r_mode(st, fi, x, action),
        Node::Choose(x) => choose::r_choose(st, &reg, fi, x, action),
        Node::Discard(x) => cards::r_discard(st, fi, x, action),
        Node::DeployCard(_) | Node::DeployToken(_) => deploy::r_excess(st, &reg, fi, ins, action),
        Node::Arrange(x) => cards::r_arrange(st, &reg, fi, x, action),
        Node::Rest(x) => cards::r_rest(st, &reg, fi, x, action),
        Node::ToDeck(x) => cards::r_to_deck(st, fi, x, action),
        other => Err(EngineError::Internal(format!(
            "no resumer for {}",
            other.kind().py_name()
        ))),
    }?;
    mark_acted(st, &reg, fi, ins, status);
    if status == Status::Next {
        add_pc(st, fi, 1)?;
    }
    if !rules_held(st, fi) {
        crate::core::rules_management(st)?;
    }
    Ok(())
}

/// `system_program(name)` (`interp.py:1589-1594`): the IR's fixed program id of a system
/// program (`system:deploy`, `system:pair`, `system:hand_limit`; PLAN §5.2).
#[allow(
    dead_code,
    reason = "called by layer L5 (game, testkit) and C's tests (docs/port/contracts.md); they land after L4"
)]
pub(crate) fn system_program(st: &GameState, s: SystemProgram) -> ProgramId {
    st.registry().system_program(s)
}

// ---------------------------------------------------------------------------------------------
// Control flow (interp.py:178-311)

/// `_h_jump` (`interp.py:182-184`).
fn h_jump(st: &mut GameState, fi: usize, target: i64) -> Result<Status, EngineError> {
    set_pc(st, fi, target)?;
    Ok(Status::Jumped)
}

/// `_h_jump_if_not` (`interp.py:187-191`): the condition sees the frame's `did` (`DidLast`).
fn h_jump_if_not(
    st: &mut GameState,
    fi: usize,
    ins: &gcg_effects::JumpIfNot,
) -> Result<Status, EngineError> {
    let dv = crate::view::derived(st)?;
    let ctx = ctx_of(st, fi)?;
    if crate::view::cond(st, &dv, &ctx, ins.cond, st.frames[fi].did)? {
        return Ok(Status::Next);
    }
    h_jump(st, fi, ins.target)
}

/// `_h_jump_if_not_did` (`interp.py:194-198`).
fn h_jump_if_not_did(st: &mut GameState, fi: usize, target: i64) -> Result<Status, EngineError> {
    if st.frames[fi].did {
        return Ok(Status::Next);
    }
    h_jump(st, fi, target)
}

/// `_h_hold_rules` (`interp.py:206-210`): the `__hold_rules` depth, deleted at 0.
fn h_hold_rules(st: &mut GameState, fi: usize, delta: i64) -> Result<Status, EngineError> {
    let depth = i64::from(st.int_get(fi, IntKey::HoldRules).unwrap_or(0)) + delta;
    let depth = to_i32(depth, "__hold_rules")?;
    st.int_set(fi, IntKey::HoldRules, depth)?;
    if depth == 0 {
        st.int_del(fi, IntKey::HoldRules);
    }
    Ok(Status::Next)
}

/// `_h_ask_may` (`interp.py:213-216`): YES/NO, a BURST decision for the Burst prompt, the
/// prompt text `ins.prompt or "you may"`.
fn h_ask_may(
    st: &mut GameState,
    reg: &Registry,
    fi: usize,
    ins: &gcg_effects::AskMay,
) -> Result<Status, EngineError> {
    let player = player_of(st, &ctx_of(st, fi)?, ins.player);
    let text = reg.str(ins.prompt);
    let kind = if text == crate::BURST_PROMPT {
        DecisionKind::Burst
    } else {
        DecisionKind::YesNo
    };
    let prompt = if text.is_empty() {
        Prompt::Engine(crate::EnginePrompt::YouMay)
    } else {
        Prompt::Ask(ins.prompt)
    };
    decide(
        st,
        fi,
        player,
        kind,
        &[
            Action::of(crate::ActionKind::Yes),
            Action::of(crate::ActionKind::No),
        ],
        prompt,
        &[],
    )
}

/// `_r_ask_may` (`interp.py:219-226`): YES records the answer and continues into the body; NO
/// clears `did` and jumps past it.
fn r_ask_may(
    st: &mut GameState,
    fi: usize,
    ins: &gcg_effects::AskMay,
    action: Action,
) -> Result<Status, EngineError> {
    if action.kind == crate::ActionKind::Yes {
        st.frame_mut(fi).answer = true;
        return Ok(Status::Next);
    }
    let f = st.frame_mut(fi);
    f.answer = false;
    f.did = false;
    h_jump(st, fi, ins.target_if_no)
}

/// `_h_mode` (`interp.py:263-274`): one SELECT option per mode whose mandatory targets exist
/// (`_mode_available`, `:229-232`), always a decision when one is available; none clears
/// `did` and jumps to the end.
fn h_mode(
    st: &mut GameState,
    fi: usize,
    ins: &gcg_effects::ModeSelect,
) -> Result<Status, EngineError> {
    let player = player_of(st, &ctx_of(st, fi)?, ins.chooser);
    let program = st.frames[fi].program;
    crate::diag::note_interpretation(crate::diag::Interpretation::I11);
    let mut opts = Vec::new();
    for i in 0..ins.labels.len() {
        let start = *ins
            .targets
            .get(i)
            .ok_or_else(|| EngineError::Index("tuple index out of range".to_owned()))?;
        let ctx = ctx_of(st, fi)?;
        if choose::targets_available_at(st, program, start, &ctx)? {
            opts.push(Action::new(
                crate::ActionKind::Select,
                idx_i32(i)?,
                NO_ARG,
                NO_ARG,
            ));
        }
    }
    if opts.is_empty() {
        set_did(st, fi, false);
        return h_jump(st, fi, ins.end);
    }
    decide(
        st,
        fi,
        player,
        DecisionKind::Select,
        &opts,
        Prompt::Engine(crate::EnginePrompt::ChooseMode),
        &[(CtxKey::Mode, 1)],
    )
}

/// Python's `seq[i]` for an int index: negative indices count from the end; out of range is
/// `IndexError`.
fn py_index(len: usize, i: i64) -> Option<usize> {
    let len = i64::try_from(len).ok()?;
    let j = if i < 0 { len + i } else { i };
    (0..len)
        .contains(&j)
        .then(|| usize::try_from(j).ok())
        .flatten()
}

/// `_r_mode` (`interp.py:277-279`): jumps to the chosen mode's body.
fn r_mode(
    st: &mut GameState,
    fi: usize,
    ins: &gcg_effects::ModeSelect,
    action: Action,
) -> Result<Status, EngineError> {
    let i = py_index(ins.targets.len(), i64::from(action.a))
        .ok_or_else(|| EngineError::Index("tuple index out of range".to_owned()))?;
    h_jump(st, fi, ins.targets[i])
}

/// `_h_loop_init` (`interp.py:282-290`): ForEach binds the resolved items under the loop key
/// with counter 0; Repeat stores `-max(0, times)` and counts up to 0.
fn h_loop_init(
    st: &mut GameState,
    fi: usize,
    ins: &gcg_effects::LoopInit,
) -> Result<Status, EngineError> {
    if let Some(r) = ins.r#ref {
        let dv = crate::view::derived(st)?;
        let items = crate::view::resolve(st, &dv, &ctx_of(st, fi)?, r)?;
        set_var_uids(st, fi, ins.key, &items)?;
        st.int_set(fi, IntKey::Loop(ins.key), 0)?;
    } else {
        let Some(times) = ins.times else {
            return Err(EngineError::Assertion(String::new()));
        };
        let n = val(st, fi, times)?;
        st.int_set(
            fi,
            IntKey::Loop(ins.key),
            to_i32(-n.max(0), "loop counter")?,
        )?;
    }
    Ok(Status::Next)
}

/// `_h_loop_next` (`interp.py:293-307`): ForEach (a non-empty `var`) binds the next item or
/// jumps to the end; Repeat counts the negative counter up to 0.
fn h_loop_next(
    st: &mut GameState,
    reg: &Registry,
    fi: usize,
    ins: &gcg_effects::LoopNext,
) -> Result<Status, EngineError> {
    let i = st.int_get(fi, IntKey::Loop(ins.key)).unwrap_or(0);
    if !reg.var_name(ins.var).is_empty() {
        let items = var_items(st, fi, ins.key);
        if i64::from(i) >= i64::try_from(items.len()).unwrap_or(i64::MAX) {
            return h_jump(st, fi, ins.end);
        }
        let k = py_index(items.len(), i64::from(i))
            .ok_or_else(|| EngineError::Index("tuple index out of range".to_owned()))?;
        st.var_set(fi, ins.var, &[items[k]])?;
        st.int_set(
            fi,
            IntKey::Loop(ins.key),
            to_i32(i64::from(i) + 1, "loop counter")?,
        )?;
        return Ok(Status::Next);
    }
    if i >= 0 {
        return h_jump(st, fi, ins.end);
    }
    st.int_set(fi, IntKey::Loop(ins.key), i + 1)?;
    Ok(Status::Next)
}

#[cfg(test)]
mod tests;
