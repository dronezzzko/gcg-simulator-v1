//! Choosing (`interp.py:229-470`): target legality (`targets_available`, `choice_short`,
//! `choice_group`), the `Choose` handler and resumer, and their candidate lists, bounds and
//! decision options (Inv 06a §13.2-§13.3).

use gcg_effects::{Choose, Loc, Node, NodeId, ProgramId, Registry, RuleKind, Side};

use super::{Status, add_pc, ctx_of, decide, player_of, rule_mod, sel_node, set_did, to_i32};
use crate::view;
use crate::{
    Action, ActionKind, Ctx, CtxKey, DecisionKind, Derived, EngineError, GameState, NO_ARG, Prompt,
    Uid,
};

/// `targets_available(instrs, start, st, ctx)` (`interp.py:235-260`): walks a program from
/// `start` along the branches the current state takes and checks every mandatory targeted
/// choice before "Then" (rules 10-1-8-1-1, 10-1-8-1-2, 10-2-2, Q100). Read-only apart from the
/// view cache it fills. Python passes the program's instruction tuple; the frozen signature
/// takes the program id (contracts.md).
///
/// # Errors
/// Any error of the view, a condition or a value.
#[allow(
    dead_code,
    reason = "called by layer L5 (game, testkit) and C's tests (docs/port/contracts.md); they land after L4"
)]
pub(crate) fn targets_available(
    st: &GameState,
    program: ProgramId,
    start: u16,
    ctx: &Ctx,
) -> Result<bool, EngineError> {
    targets_available_at(st, program, i64::from(start), ctx)
}

/// [`targets_available`] from an int start (a `ModeSelect` target).
pub(super) fn targets_available_at(
    st: &GameState,
    program: ProgramId,
    start: i64,
    ctx: &Ctx,
) -> Result<bool, EngineError> {
    let reg = st.registry();
    walk(st, reg, &reg.program(program).instrs, start, ctx)
}

fn walk(
    st: &GameState,
    reg: &Registry,
    instrs: &[NodeId],
    start: i64,
    ctx: &Ctx,
) -> Result<bool, EngineError> {
    let dv = view::derived(st)?;
    let len = i64::try_from(instrs.len()).map_err(|_| EngineError::Overflow { what: "program" })?;
    let mut pc = start;
    for _ in 0..=instrs.len() {
        if pc >= len {
            return Ok(true);
        }
        let ins = usize::try_from(pc)
            .ok()
            .and_then(|i| instrs.get(i))
            .copied()
            .ok_or_else(|| EngineError::Internal(format!("negative program counter {pc}")))?;
        match reg.node(ins) {
            Node::Jump(j) => {
                pc = j.target;
                continue;
            }
            Node::JumpIfNot(j) => {
                pc = if view::cond(st, &dv, ctx, j.cond, true)? {
                    pc + 1
                } else {
                    j.target
                };
                continue;
            }
            Node::ModeSelect(m) => {
                for &t in &m.targets {
                    if walk(st, reg, instrs, t, ctx)? {
                        return Ok(true);
                    }
                }
                return Ok(false);
            }
            Node::AskMay(_) | Node::JumpIfNotDid(_) | Node::LoopInit(_) => return Ok(true),
            Node::Choose(c) => {
                if c.after_then {
                    return Ok(true);
                }
                if choice_short(st, reg, ctx, c)? {
                    return Ok(false);
                }
            }
            _ => {}
        }
        pc += 1;
    }
    Ok(true)
}

/// `PUBLIC_TARGET_LOCS` (`interp.py:354-363`).
const fn is_public_target_loc(loc: Loc) -> bool {
    matches!(
        loc,
        Loc::Battle
            | Loc::Base
            | Loc::ResourceArea
            | Loc::Trash
            | Loc::Paired
            | Loc::FieldUnitsAndBases
    )
}

/// `is_target_choice(ins)` (`interp.py:366-368`): rule 10-2-2-1, choosing a card in a public
/// location for "choose"/"you may choose".
fn is_target_choice(reg: &Registry, ins: &Choose) -> Result<bool, EngineError> {
    let loc = sel_node(reg, ins.sel)?.loc;
    if ins.targeting && loc == Loc::Removal {
        crate::diag::note_interpretation(crate::diag::Interpretation::I14);
    }
    Ok(ins.targeting && is_public_target_loc(loc))
}

/// `choice_short(st, ctx, ins, exclude=frozenset())` (`interp.py:371-388`): a mandatory
/// targeted choice of N needs N choosable cards (rule 10-2-2; ruling GD01-003:Q121). Every
/// caller passes the default `exclude`, so the parameter is not ported. `distinct_from` is not
/// applied.
fn choice_short(
    st: &GameState,
    reg: &Registry,
    ctx: &Ctx,
    ins: &Choose,
) -> Result<bool, EngineError> {
    if !is_target_choice(reg, ins)? || ins.optional {
        return Ok(false);
    }
    let dv = view::derived(st)?;
    let n = view::value(st, &dv, ctx, ins.count)?;
    let need = match ins.min_count {
        None => n,
        Some(m) => view::value(st, &dv, ctx, m)?,
    };
    if need <= 0 {
        return Ok(false);
    }
    let mut count: i64 = 0;
    for u in view::select(st, &dv, ctx, ins.sel)? {
        if !cant_be_chosen(st, reg, &dv, u, ctx.controller)? {
            count += 1;
        }
    }
    Ok(count < need)
}

/// `_cant_be_chosen(st, dv, uid, chooser_side)` (`interp.py:326-332`): any CANT_BE_CHOSEN rule
/// protects the card, except a rule limited to enemy sources when the chooser owns it.
fn cant_be_chosen(
    st: &GameState,
    reg: &Registry,
    dv: &Derived,
    uid: Uid,
    chooser_side: u8,
) -> Result<bool, EngineError> {
    let owner = st.cards[usize::from(uid)].owner;
    for r in view::rules_of(dv, uid, RuleKind::CantBeChosen) {
        if rule_mod(reg, r.rule)?.source_side == Side::Enemy && chooser_side == owner {
            continue;
        }
        return Ok(true);
    }
    Ok(false)
}

/// `choice_group(instrs, pc)` (`interp.py:391-404`): the `Choose` at `pc` plus the following
/// Chooses tied to it by `distinct_from` ("choose 1 X and 1 Y" activate only together, ruling
/// GD03-056:Q230); stops at the first instruction that is not tied.
pub(super) fn choice_group<'r>(
    reg: &'r Registry,
    instrs: &[NodeId],
    pc: usize,
) -> Result<Vec<&'r Choose>, EngineError> {
    let Some(Node::Choose(first)) = instrs.get(pc).map(|&i| reg.node(i)) else {
        return Err(EngineError::Assertion(String::new()));
    };
    let mut group = vec![first];
    let mut names = vec![first.var];
    for &i in &instrs[pc + 1..] {
        match reg.node(i) {
            Node::Choose(c) if c.distinct_from.iter().any(|v| names.contains(v)) => {
                group.push(c);
                names.push(c.var);
            }
            _ => break,
        }
    }
    Ok(group)
}

/// `_choose_candidates(st, f, ins)` (`interp.py:314-323`): the selection in `V.select` order,
/// minus the cards bound in any `distinct_from` var, minus unchoosable cards when targeting.
fn choose_candidates(
    st: &GameState,
    reg: &Registry,
    fi: usize,
    ins: &Choose,
) -> Result<crate::UidBuf, EngineError> {
    let dv = view::derived(st)?;
    let ctx = ctx_of(st, fi)?;
    let mut cands = view::select(st, &dv, &ctx, ins.sel)?;
    if !ins.distinct_from.is_empty() {
        let mut excl: Vec<i32> = Vec::new();
        for &v in &ins.distinct_from {
            if let Some(items) = st.var_get(fi, v) {
                excl.extend_from_slice(items);
            }
        }
        cands.retain(|&u| !excl.contains(&i32::from(u)));
    }
    if ins.targeting {
        let controller = st.frames[fi].controller;
        let mut kept = crate::UidBuf::new();
        for u in cands {
            if !cant_be_chosen(st, reg, &dv, u, controller)? {
                kept.push(u);
            }
        }
        cands = kept;
    }
    Ok(cands)
}

/// `choose_bounds(st, f, ins)` (`interp.py:335-342`): `(max(0, n), max(0, min(mn, n)))` with
/// `mn = n` without a `min_count` and 0 for an optional choice (whose `min_count` is still
/// evaluated).
fn choose_bounds(st: &GameState, fi: usize, ins: &Choose) -> Result<(i64, i64), EngineError> {
    let dv = view::derived(st)?;
    let ctx = ctx_of(st, fi)?;
    let n = view::value(st, &dv, &ctx, ins.count)?;
    let mut mn = match ins.min_count {
        None => n,
        Some(m) => view::value(st, &dv, &ctx, m)?,
    };
    if ins.optional {
        mn = 0;
    }
    Ok((n.max(0), mn.min(n).max(0)))
}

/// `cands.index(x)` (`list.index`, first occurrence): Python's `ValueError` when absent.
fn index_of(cands: &[Uid], x: i32) -> Result<usize, EngineError> {
    cands
        .iter()
        .position(|&u| i32::from(u) == x)
        .ok_or_else(|| EngineError::Value("list.index(x): x not in list".to_owned()))
}

/// `_choose_options(st, f, cands, n, mn)` (`interp.py:345-351`): SELECT for every candidate
/// after the last pick (by candidate position) that is not picked, then DONE once the minimum
/// is met.
fn choose_options(
    st: &GameState,
    fi: usize,
    cands: &[Uid],
    mn: i64,
) -> Result<Vec<Action>, EngineError> {
    let picked = st.buf(fi);
    let last = match picked.last() {
        Some(&x) => i64::try_from(index_of(cands, x)?).unwrap_or(i64::MAX),
        None => -1,
    };
    let mut opts = Vec::with_capacity(cands.len() + 1);
    for (i, &u) in cands.iter().enumerate() {
        if i64::try_from(i).unwrap_or(i64::MAX) > last && !picked.contains(&i32::from(u)) {
            opts.push(Action::new(
                ActionKind::Select,
                i32::from(u),
                NO_ARG,
                NO_ARG,
            ));
        }
    }
    if i64::try_from(picked.len()).unwrap_or(i64::MAX) >= mn {
        opts.push(Action::of(ActionKind::Done));
    }
    Ok(opts)
}

/// The decision of a `Choose` (`interp.py:432-441`, `:460-469`): SELECT by the chooser, prompt
/// `f"choose {ins.var}"`, context `(("min", mn), ("max", n))`.
fn choose_decision(
    st: &mut GameState,
    fi: usize,
    ins: &Choose,
    cands: &[Uid],
    n: i64,
    mn: i64,
) -> Result<Status, EngineError> {
    let chooser = player_of(st, &ctx_of(st, fi)?, ins.chooser);
    let opts = choose_options(st, fi, cands, mn)?;
    decide(
        st,
        fi,
        chooser,
        DecisionKind::Select,
        &opts,
        Prompt::Choose(ins.var),
        &[
            (CtxKey::Min, to_i32(mn, "choice minimum")?),
            (CtxKey::Max, to_i32(n, "choice maximum")?),
        ],
    )
}

/// `_h_choose` (`interp.py:407-441`). A group with a short mandatory target choice binds every
/// var of the group to `()`, clears `did` and skips the group. Otherwise: no count or no
/// candidates finish empty; a random choice draws `randrange(len(pool))` `min(n, len(pool))`
/// times **before** the take-all shortcut (Inv 06a hazard 2); taking every candidate needs no
/// decision; else a SELECT decision.
pub(super) fn h_choose(
    st: &mut GameState,
    reg: &Registry,
    fi: usize,
    ins: &Choose,
) -> Result<Status, EngineError> {
    let program = reg.program(st.frames[fi].program);
    let group = choice_group(reg, &program.instrs, usize::from(st.frames[fi].pc))?;
    let ctx = ctx_of(st, fi)?;
    let mut short = false;
    for g in &group {
        if choice_short(st, reg, &ctx, g)? {
            short = true;
            break;
        }
    }
    if short {
        for g in &group {
            st.var_set(fi, g.var, &[])?;
        }
        set_did(st, fi, false);
        st.buf_set(fi, &[])?;
        add_pc(st, fi, group.len())?;
        return Ok(Status::Jumped);
    }
    let cands = choose_candidates(st, reg, fi, ins)?;
    let (n, mn) = choose_bounds(st, fi, ins)?;
    st.buf_set(fi, &[])?;
    if n == 0 || cands.is_empty() {
        return finish_choose(st, fi, ins);
    }
    if ins.random {
        let mut pool = cands;
        let draws = usize::try_from(n).map_or(pool.len(), |n| n.min(pool.len()));
        let mut chosen = Vec::with_capacity(draws);
        for _ in 0..draws {
            let j = st.rng.randrange_usize(pool.len());
            chosen.push(i32::from(pool.remove(j)));
        }
        st.buf_set(fi, &chosen)?;
        return finish_choose(st, fi, ins);
    }
    let len = i64::try_from(cands.len()).unwrap_or(i64::MAX);
    if len <= mn || (len <= n && mn == n) {
        st.buf_set(fi, &super::as_ints(&cands))?;
        return finish_choose(st, fi, ins);
    }
    choose_decision(st, fi, ins, &cands, n, mn)
}

/// `_finish_choose(f, ins)` (`interp.py:444-448`): binds the picks, `did = bool(picks)`,
/// clears the buffer.
fn finish_choose(st: &mut GameState, fi: usize, ins: &Choose) -> Result<Status, EngineError> {
    let picks = st.buf(fi).to_vec();
    st.var_set(fi, ins.var, &picks)?;
    set_did(st, fi, !picks.is_empty());
    st.buf_set(fi, &[])?;
    Ok(Status::Next)
}

/// `_r_choose` (`interp.py:451-470`): DONE finishes; a pick is appended and the choice
/// finishes once `n` cards are picked or no candidate after the pick remains, else the
/// decision is raised again (`WAIT`).
pub(super) fn r_choose(
    st: &mut GameState,
    reg: &Registry,
    fi: usize,
    ins: &Choose,
    action: Action,
) -> Result<Status, EngineError> {
    if action.kind == ActionKind::Done {
        return finish_choose(st, fi, ins);
    }
    st.buf_push(fi, action.a)?;
    let cands = choose_candidates(st, reg, fi, ins)?;
    let (n, mn) = choose_bounds(st, fi, ins)?;
    let buf = st.buf(fi).to_vec();
    let last = *buf.last().unwrap_or(&NO_ARG);
    let mut remaining = 0usize;
    for &u in &cands {
        // `u not in f.buf and cands.index(u) > cands.index(f.buf[-1])`, short-circuit
        // included: the index of the last pick is only looked up for an unpicked candidate.
        if !buf.contains(&i32::from(u)) && index_of(&cands, i32::from(u))? > index_of(&cands, last)?
        {
            remaining += 1;
        }
    }
    if i64::try_from(buf.len()).unwrap_or(i64::MAX) >= n || remaining == 0 {
        return finish_choose(st, fi, ins);
    }
    choose_decision(st, fi, ins, &cands, n, mn)?;
    Ok(Status::Wait)
}
