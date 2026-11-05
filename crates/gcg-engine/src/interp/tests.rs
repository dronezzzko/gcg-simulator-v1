//! Unit tests of the interpreter (layer L4).
//!
//! The tests in the first part need only layer L1 (state, arenas, canonical form): frames,
//! decisions, control flow, frame bookkeeping, the deck reorders, the L1-only hooks, delayed
//! triggers, token keys and choice groups, on minimal states or on the Python-exported
//! fixture states of `tests/fixtures/states.ndjson` (`oracle/export_state_fixtures.py`).
//! Expected values are Python's (the RNG vectors were computed with `gcg_sim.rng`).
//!
//! The two corpus tests at the end (`effect_decisions_reraise_*` and
//! `effect_decisions_replay_*`) also run the derived view (L2) and the core primitives (L3):
//! they are the L4 gate against Python's traces and pass once those layers land.

use std::path::PathBuf;
use std::sync::Arc;

use gcg_data::DefId;
use gcg_effects::{
    ContRef, Duration, Node, NodeId, NodeKind, P, ProgramId, Registry, StepParams, SystemProgram,
    VarId,
};
use serde_json::Value;

use super::{
    Status, choose, deploy, execute, finish_frame, hooks, is_non_action, push_frame, py_index,
    r_ask_may, r_mode,
};
use crate::canon::{fixtures, from_canonical, state_digest};
use crate::{
    Action, ActionKind, CardInstance, CtxKey, DecisionKind, EngineError, EnginePrompt, EvKey,
    EventPayload, Frame, FrameKind, GameConst, GameState, IntKey, OnceElem, OnceKey, Prompt,
    PushFrame, StateSnapshot, Uid, Zone,
};

// ---------------------------------------------------------------------------------------------
// Helpers

fn reg() -> Arc<Registry> {
    Arc::clone(fixtures::registry())
}

/// `GameState(seed)` over the embedded registry with empty decklists.
fn bare_state(seed: u64) -> GameState {
    let empty = || -> Box<[DefId]> { Box::new([]) };
    let konst = GameConst::new(reg(), [empty(), empty()], [empty(), empty()], 200, 20000)
        .expect("a game constant");
    GameState::new(Arc::new(konst), seed)
}

/// Appends a card of `number` to `zones[owner][zone]`.
fn add_card(st: &mut GameState, number: &str, owner: u8, zone: Zone) -> Uid {
    let def = st.db().get(number).expect("a card number").def_id;
    let uid = Uid::try_from(st.cards.len()).expect("a uid");
    st.cards.push(CardInstance {
        def_id: def,
        owner,
        zone,
        rested: false,
        damage: 0,
        pair: -1,
        zone_seq: 0,
        entered_turn: 0,
        known: 0,
    });
    st.zones.push(owner, zone, uid).expect("zone room");
    uid
}

/// Every program id the registry reaches: the abilities' programs and cost programs, the
/// delayed-trigger bodies, the keyword and system programs.
fn programs(reg: &Registry) -> Vec<ProgramId> {
    let mut out = Vec::new();
    for def in 0..reg.db().len() {
        let entry = reg.card(DefId(u16::try_from(def).expect("a def id")));
        for &aid in entry.own.iter().chain(entry.unit.iter()) {
            let a = reg.ability(aid);
            out.extend(a.program);
            out.extend(a.cost_program);
        }
        out.extend(entry.delayed.iter().map(|&(_, _, p)| p));
    }
    for kw in gcg_effects::KeywordProgram::ALL {
        out.push(reg.keyword_program(kw));
    }
    for s in SystemProgram::ALL {
        out.push(reg.system_program(s));
    }
    out.sort();
    out.dedup();
    out
}

/// The program labelled `label` (`Program.label`, the first registrant's).
fn program_labelled(reg: &Registry, label: &str) -> ProgramId {
    programs(reg)
        .into_iter()
        .find(|&p| reg.str(reg.program(p).label) == label)
        .unwrap_or_else(|| panic!("no program labelled {label}"))
}

/// A frame of `program` on `host` (controller 0, no event), pushed with `push_frame`.
fn frame_on(st: &mut GameState, program: ProgramId, host: i32, kind: FrameKind) -> usize {
    push_frame(
        st,
        program,
        PushFrame {
            controller: 0,
            host,
            card_uid: crate::NO_ARG,
            kind,
            event: EventPayload::default(),
            vars: &[],
        },
    )
    .expect("a frame")
}

/// The instruction at `pc` of the frame's program.
fn instr(st: &GameState, reg: &Registry, fi: usize, pc: u16) -> NodeId {
    reg.program(st.frames[fi].program).instrs[usize::from(pc)]
}

fn var(reg: &Registry, name: &str) -> VarId {
    reg.var_id(name)
        .unwrap_or_else(|| panic!("no frame variable {name}"))
}

// ---------------------------------------------------------------------------------------------
// Pure helpers

#[test]
fn no_event_key_feeds_the_it_vars() {
    // push_frame appends every event value whose key is "it" + digits to vars["it"]
    // (interp.py:84-86); the Rust engine relies on no such key existing.
    for k in EvKey::ALL {
        let s = k.as_str();
        let it_digits =
            s.len() > 2 && s.starts_with("it") && s[2..].chars().all(|c| c.is_ascii_digit());
        assert!(!it_digits, "event key {s} would feed vars[\"it\"]");
    }
}

#[test]
fn non_actions_are_pythons_twelve() {
    // NON_ACTIONS (interp.py:41-54).
    let mut names: Vec<&str> = NodeKind::ALL
        .into_iter()
        .filter(|&k| is_non_action(k))
        .map(NodeKind::py_name)
        .collect();
    names.sort();
    assert_eq!(
        names,
        [
            "AskMay",
            "BindVar",
            "Choose",
            "ChooseMode",
            "HoldRules",
            "Jump",
            "JumpIfNot",
            "JumpIfNotDid",
            "LoopInit",
            "LoopNext",
            "ModeSelect",
            "SetDid"
        ]
    );
}

#[test]
fn indices_follow_python() {
    assert_eq!(py_index(3, 0), Some(0));
    assert_eq!(py_index(3, 2), Some(2));
    assert_eq!(py_index(3, 3), None);
    assert_eq!(py_index(3, -1), Some(2));
    assert_eq!(py_index(3, -3), Some(0));
    assert_eq!(py_index(3, -4), None);
    assert_eq!(py_index(0, 0), None);
}

#[test]
fn every_token_key_round_trips_to_a_token() {
    // token_spec(key).key (interp.py:1018-1024, cards/tokens.py:27-30) and CardDB.token_for.
    let reg = reg();
    let mut keys = Vec::new();
    for p in programs(&reg) {
        for &ins in &reg.program(p).instrs {
            if let Node::DeployToken(t) = reg.node(ins) {
                keys.push(t.token_key);
                keys.extend(t.also.iter().map(|&(k, _)| k));
            }
        }
    }
    keys.sort();
    keys.dedup();
    assert_eq!(keys.len(), 29, "the IR's distinct token keys (Python: 29)");
    for k in keys {
        let key = reg.str(k);
        let spec = deploy::token_key(key).expect("a token key");
        assert_eq!(spec, key, "TokenSpec.key of a compiled key is the key");
        let def = reg.db().token_for_key(&spec).expect("a token definition");
        assert!(def.is_token(), "{key} names a token");
    }
}

#[test]
fn token_keys_parse_like_python() {
    // `name, traits, ap, hp, text = key.split("|", 4)`, empty traits dropped, int() stats.
    let key = deploy::token_key;
    assert_eq!(key("Zaku|Zeon//A|1|2|x|y").unwrap(), "Zaku|Zeon/A|1|2|x|y");
    assert_eq!(key("Z|| 3 |+4|").unwrap(), "Z||3|4|");
    assert_eq!(key("Z||1_0|-0|").unwrap(), "Z||10|0|");
    assert_eq!(
        key("a|b"),
        Err(EngineError::Value(
            "not enough values to unpack (expected 5, got 2)".to_owned()
        ))
    );
    assert_eq!(
        key("Z||x|1|"),
        Err(EngineError::Value(
            "invalid literal for int() with base 10: 'x'".to_owned()
        ))
    );
    for bad in ["Z||1__0|1|", "Z||_1|1|", "Z||1_|1|", "Z|||1|", "Z||+|1|"] {
        assert!(
            matches!(key(bad), Err(EngineError::Value(_))),
            "{bad} is not an int"
        );
    }
}

#[test]
fn chooses_tied_by_distinct_from_form_one_group() {
    // choice_group (interp.py:391-404) on the eight programs where Python finds a group of
    // two (vars t1, t2), and a lone Choose elsewhere.
    let reg = reg();
    for (label, pc) in [
        ("GD01-103#a0", 0),
        ("GD02-055#a1", 0),
        ("GD03-056#a0", 1),
        ("GD03-116#a0", 0),
        ("GD04-015#a0", 1),
        ("GD04-071#a1", 0),
        ("ST05-010#u0", 1),
        ("ST13-001#a1", 0),
    ] {
        let p = program_labelled(&reg, label);
        let group = choose::choice_group(&reg, &reg.program(p).instrs, pc).unwrap();
        let vars: Vec<&str> = group.iter().map(|c| reg.var_name(c.var)).collect();
        assert_eq!(vars, ["t1", "t2"], "{label}");
    }
    let mut groups = 0;
    for p in programs(&reg) {
        let instrs = &reg.program(p).instrs;
        for (pc, &i) in instrs.iter().enumerate() {
            if matches!(reg.node(i), Node::Choose(_)) {
                let n = choose::choice_group(&reg, instrs, pc).unwrap().len();
                assert!(n <= 2, "{}", reg.str(reg.program(p).label));
                groups += usize::from(n > 1);
            }
        }
    }
    assert_eq!(groups, 8, "Python finds 8 groups of two");
    let p = program_labelled(&reg, "GD03-056#a0");
    assert_eq!(
        choose::choice_group(&reg, &reg.program(p).instrs, 0).map(|g| g.len()),
        Err(EngineError::Assertion(String::new())),
        "the instruction at pc 0 is not a Choose"
    );
}

// ---------------------------------------------------------------------------------------------
// Frames and decisions

#[test]
fn push_frame_takes_the_host_seq_and_copies_the_vars() {
    let reg = reg();
    let mut st = bare_state(1);
    let u = add_card(&mut st, "GD01-001", 1, Zone::Battle);
    st.cards[usize::from(u)].zone_seq = 5;
    let card = var(&reg, "card");
    let deploy_p = reg.system_program(SystemProgram::Deploy);
    let fi = push_frame(
        &mut st,
        deploy_p,
        PushFrame {
            controller: 1,
            host: i32::from(u),
            card_uid: crate::NO_ARG,
            kind: FrameKind::PlaySub,
            event: EventPayload::default(),
            vars: &[(card, &[i32::from(u)])],
        },
    )
    .unwrap();
    assert_eq!(fi, 0);
    let f: Frame = st.frames[0];
    assert_eq!(
        (
            f.program,
            f.controller,
            f.host,
            f.host_seq,
            f.card_uid,
            f.kind
        ),
        (
            deploy_p,
            1,
            i32::from(u),
            5,
            i32::from(u),
            FrameKind::PlaySub
        )
    );
    assert_eq!(
        (f.pc, f.did, f.answer, f.waiting, f.acted),
        (0, true, false, false, false)
    );
    assert!(f.once_key.is_none());
    assert_eq!(st.var_get(0, card), Some(&[i32::from(u)][..]));
    assert!(st.buf(0).is_empty());
    // A hostless frame (game.py:308) has host_seq 0 and card_uid -1.
    let fi = frame_on(&mut st, deploy_p, -1, FrameKind::System);
    assert_eq!(
        (fi, st.frames[1].host_seq, st.frames[1].card_uid),
        (1, 0, -1)
    );
    // Python's IndexError for a host past the cards.
    let err = push_frame(
        &mut st,
        deploy_p,
        PushFrame {
            controller: 0,
            host: 99,
            card_uid: 3,
            kind: FrameKind::Trigger,
            event: EventPayload::default(),
            vars: &[],
        },
    );
    assert_eq!(
        err,
        Err(EngineError::Index("list index out of range".to_owned()))
    );
    assert_eq!(st.frames.len(), 2);
}

#[test]
fn decisions_wait_with_pythons_header() {
    let reg = reg();
    let mut st = bare_state(2);
    let fi = frame_on(
        &mut st,
        reg.system_program(SystemProgram::HandLimit),
        -1,
        FrameKind::System,
    );
    let opts = [
        Action::new(ActionKind::Select, 4, -1, -1),
        Action::of(ActionKind::Done),
    ];
    let status = super::decide(
        &mut st,
        fi,
        1,
        DecisionKind::Select,
        &opts,
        Prompt::Engine(EnginePrompt::RestInstead),
        &[(CtxKey::RestSub, 7)],
    )
    .unwrap();
    assert_eq!(status, Status::Wait);
    assert!(st.frames[fi].waiting);
    let d = st.pending().expect("a decision");
    assert_eq!((d.hdr.player, d.hdr.kind), (1, DecisionKind::Select));
    assert_eq!(d.hdr.prompt, Prompt::Engine(EnginePrompt::RestInstead));
    assert_eq!(d.hdr.ctx(CtxKey::RestSub), 7);
    assert_eq!(d.hdr.ctx(CtxKey::Min), -1);
    assert_eq!(d.options, opts);
    assert_eq!(st.version, 0, "a decision does not touch");
}

// ---------------------------------------------------------------------------------------------
// Control flow

#[test]
fn jumps_follow_their_targets() {
    let reg = reg();
    let mut st = bare_state(3);
    // GD03-115#a0: [Choose, JumpIfNot(Cmp)->4, Apply, Jump->5, Apply]
    let fi = frame_on(
        &mut st,
        program_labelled(&reg, "GD03-115#a0"),
        -1,
        FrameKind::Trigger,
    );
    st.frame_mut(fi).pc = 3;
    let ins = instr(&st, &reg, fi, 3);
    assert_eq!(execute(&mut st, &reg, fi, ins), Ok(Status::Jumped));
    assert_eq!(st.frames[fi].pc, 5);
    // EB01-008#a0 pc 4: JumpIfNotDid(target=7)
    let fi = frame_on(
        &mut st,
        program_labelled(&reg, "EB01-008#a0"),
        -1,
        FrameKind::Trigger,
    );
    st.frame_mut(fi).pc = 4;
    let ins = instr(&st, &reg, fi, 4);
    assert_eq!(execute(&mut st, &reg, fi, ins), Ok(Status::Next));
    assert_eq!(st.frames[fi].pc, 4, "Next leaves pc to run_top_frame");
    st.frame_mut(fi).did = false;
    assert_eq!(execute(&mut st, &reg, fi, ins), Ok(Status::Jumped));
    assert_eq!(st.frames[fi].pc, 7);
    assert_eq!(st.version, 0);
}

#[test]
fn hold_rules_nests_and_is_deleted_at_zero() {
    let reg = reg();
    let mut st = bare_state(4);
    // EB01-023#a0: HoldRules(1) at 0 and HoldRules(-1) at 11.
    let fi = frame_on(
        &mut st,
        program_labelled(&reg, "EB01-023#a0"),
        -1,
        FrameKind::Command,
    );
    let (open, close) = (instr(&st, &reg, fi, 0), instr(&st, &reg, fi, 11));
    assert_eq!(execute(&mut st, &reg, fi, open), Ok(Status::Next));
    assert_eq!(st.int_get(fi, IntKey::HoldRules), Some(1));
    assert!(super::rules_held(&st, fi));
    assert_eq!(execute(&mut st, &reg, fi, open), Ok(Status::Next));
    assert_eq!(st.int_get(fi, IntKey::HoldRules), Some(2));
    assert_eq!(execute(&mut st, &reg, fi, close), Ok(Status::Next));
    assert_eq!(execute(&mut st, &reg, fi, close), Ok(Status::Next));
    assert_eq!(st.int_get(fi, IntKey::HoldRules), None);
    assert!(!super::rules_held(&st, fi));
    assert!(st.frame_ints(&st.frames[fi]).is_empty());
}

#[test]
fn repeat_counts_up_to_zero() {
    let reg = reg();
    let mut st = bare_state(5);
    // GD04-100#u0 pc 2: LoopNext(key='__loop0', var='', end=5)
    let fi = frame_on(
        &mut st,
        program_labelled(&reg, "GD04-100#u0"),
        -1,
        FrameKind::Trigger,
    );
    st.frame_mut(fi).pc = 2;
    let ins = instr(&st, &reg, fi, 2);
    let Node::LoopNext(ln) = reg.node(ins) else {
        panic!("not a LoopNext")
    };
    let key = IntKey::Loop(ln.key);
    st.int_set(fi, key, -2).unwrap();
    assert_eq!(execute(&mut st, &reg, fi, ins), Ok(Status::Next));
    assert_eq!(st.int_get(fi, key), Some(-1));
    assert_eq!(execute(&mut st, &reg, fi, ins), Ok(Status::Next));
    assert_eq!(st.int_get(fi, key), Some(0));
    assert_eq!(execute(&mut st, &reg, fi, ins), Ok(Status::Jumped));
    assert_eq!((st.frames[fi].pc, st.int_get(fi, key)), (5, Some(0)));
    // An absent counter reads as 0: the loop ends at once.
    let fi = frame_on(
        &mut st,
        program_labelled(&reg, "GD04-100#u0"),
        -1,
        FrameKind::Trigger,
    );
    st.frame_mut(fi).pc = 2;
    assert_eq!(execute(&mut st, &reg, fi, ins), Ok(Status::Jumped));
    assert_eq!(st.frames[fi].pc, 5);
}

#[test]
fn ask_may_asks_yes_no_and_jumps_on_no() {
    let reg = reg();
    let mut st = bare_state(6);
    // EB01-008#a0 pc 1: AskMay('Development 1: ...?', you, target_if_no=7)
    let fi = frame_on(
        &mut st,
        program_labelled(&reg, "EB01-008#a0"),
        -1,
        FrameKind::Trigger,
    );
    st.frame_mut(fi).pc = 1;
    let ins = instr(&st, &reg, fi, 1);
    let Node::AskMay(ask) = reg.node(ins) else {
        panic!("not an AskMay")
    };
    assert_eq!(execute(&mut st, &reg, fi, ins), Ok(Status::Wait));
    let d = st.pending().unwrap();
    assert_eq!((d.hdr.player, d.hdr.kind), (0, DecisionKind::YesNo));
    assert_eq!(d.hdr.prompt, Prompt::Ask(ask.prompt));
    assert_eq!(
        d.hdr.prompt.text(&reg),
        "Development 1: exile 1 (G Generation) cards from your trash?"
    );
    assert_eq!(
        d.options,
        [Action::of(ActionKind::Yes), Action::of(ActionKind::No)]
    );
    assert!(d.hdr.context.as_slice().is_empty());
    assert_eq!(
        r_ask_may(&mut st, fi, ask, Action::of(ActionKind::Yes)),
        Ok(Status::Next)
    );
    assert_eq!(
        (st.frames[fi].answer, st.frames[fi].did, st.frames[fi].pc),
        (true, true, 1)
    );
    assert_eq!(
        r_ask_may(&mut st, fi, ask, Action::of(ActionKind::No)),
        Ok(Status::Jumped)
    );
    assert_eq!(
        (st.frames[fi].answer, st.frames[fi].did, st.frames[fi].pc),
        (false, false, 7)
    );
}

#[test]
fn burst_prompts_raise_burst_decisions_and_empty_prompts_say_you_may() {
    let reg = reg();
    let mut st = bare_state(7);
    let mut seen = (false, false);
    for p in programs(&reg) {
        for (pc, &ins) in reg.program(p).instrs.iter().enumerate() {
            let Node::AskMay(a) = reg.node(ins) else {
                continue;
            };
            let text = reg.str(a.prompt);
            if !(text.is_empty() || text == crate::BURST_PROMPT) {
                continue;
            }
            let fi = frame_on(&mut st, p, -1, FrameKind::Burst);
            st.frame_mut(fi).pc = u16::try_from(pc).unwrap();
            assert_eq!(execute(&mut st, &reg, fi, ins), Ok(Status::Wait));
            let d = st.pending().unwrap();
            if text.is_empty() {
                assert_eq!(d.hdr.kind, DecisionKind::YesNo);
                assert_eq!(d.hdr.prompt, Prompt::Engine(EnginePrompt::YouMay));
                seen.0 = true;
            } else {
                assert_eq!(d.hdr.kind, DecisionKind::Burst);
                assert_eq!(d.hdr.prompt.text(&reg), "activate Burst?");
                seen.1 = true;
            }
            st.frame_pop();
        }
    }
    assert_eq!(seen, (true, true));
}

#[test]
fn mode_resumes_at_the_chosen_body() {
    let reg = reg();
    let mut st = bare_state(8);
    // GD05-102#a0 pc 0: ModeSelect(('return', 'recover'), (1, 4), you, end=7)
    let fi = frame_on(
        &mut st,
        program_labelled(&reg, "GD05-102#a0"),
        -1,
        FrameKind::Trigger,
    );
    let ins = instr(&st, &reg, fi, 0);
    let Node::ModeSelect(m) = reg.node(ins) else {
        panic!("not a ModeSelect")
    };
    let pick = |i| Action::new(ActionKind::Select, i, -1, -1);
    assert_eq!(r_mode(&mut st, fi, m, pick(1)), Ok(Status::Jumped));
    assert_eq!(st.frames[fi].pc, 4);
    assert_eq!(r_mode(&mut st, fi, m, pick(0)), Ok(Status::Jumped));
    assert_eq!(st.frames[fi].pc, 1);
    assert_eq!(
        r_mode(&mut st, fi, m, pick(-1)),
        Ok(Status::Jumped),
        "Python's targets[-1]"
    );
    assert_eq!(st.frames[fi].pc, 4);
    assert_eq!(
        r_mode(&mut st, fi, m, pick(2)),
        Err(EngineError::Index("tuple index out of range".to_owned()))
    );
}

#[test]
fn only_instructions_have_handlers() {
    let reg = reg();
    let mut st = bare_state(9);
    let fi = frame_on(
        &mut st,
        program_labelled(&reg, "GD03-115#a0"),
        -1,
        FrameKind::Trigger,
    );
    // The Choose's selector is a node without a handler.
    let Node::Choose(c) = reg.node(instr(&st, &reg, fi, 0)) else {
        panic!("not a Choose")
    };
    assert_eq!(
        execute(&mut st, &reg, fi, c.sel.node()),
        Err(EngineError::Internal("no handler for Sel".to_owned()))
    );
}

// ---------------------------------------------------------------------------------------------
// finish_frame

#[test]
fn finished_sub_frames_advance_their_parents() {
    let reg = reg();
    let mut st = bare_state(10);
    let parent = frame_on(
        &mut st,
        program_labelled(&reg, "GD03-115#a0"),
        -1,
        FrameKind::Trigger,
    );
    st.frame_mut(parent).pc = 2;
    let child = frame_on(
        &mut st,
        reg.system_program(SystemProgram::Deploy),
        -1,
        FrameKind::PlaySub,
    );
    st.frame_mut(child).did = false;
    let f = st.frame_pop().unwrap();
    finish_frame(&mut st, &f).unwrap();
    assert_eq!(
        (st.frames[parent].pc, st.frames[parent].did),
        (3, true),
        "play_sub keeps did"
    );
    let child = frame_on(
        &mut st,
        reg.system_program(SystemProgram::Deploy),
        -1,
        FrameKind::ActivateMainSub,
    );
    st.frame_mut(child).did = false;
    let f = st.frame_pop().unwrap();
    finish_frame(&mut st, &f).unwrap();
    assert_eq!(
        (st.frames[parent].pc, st.frames[parent].did),
        (4, false),
        "activate_main_sub passes did up"
    );
    // Without a parent nothing is advanced.
    let f = st.frame_pop().unwrap();
    let mut lone = f;
    lone.kind = FrameKind::PlaySub;
    finish_frame(&mut st, &lone).unwrap();
    assert!(st.frames.is_empty());
    assert_eq!(st.version, 0);
}

#[test]
fn once_keys_are_used_only_by_frames_that_acted() {
    let reg = reg();
    let mut st = bare_state(11);
    let key = OnceKey::from_elems(&[OnceElem::Int(3), OnceElem::Int(1), OnceElem::Int(2)]).unwrap();
    let fi = frame_on(
        &mut st,
        program_labelled(&reg, "GD03-115#a0"),
        -1,
        FrameKind::Trigger,
    );
    st.frame_mut(fi).once_key = Some(key);
    let f = st.frame_pop().unwrap();
    finish_frame(&mut st, &f).unwrap();
    assert!(
        st.once_used.is_empty(),
        "a frame that did not act keeps its once key"
    );
    let mut acted = f;
    acted.acted = true;
    finish_frame(&mut st, &acted).unwrap();
    finish_frame(&mut st, &acted).unwrap();
    assert_eq!(st.once_used, [key], "set semantics");
    let mut empty = acted;
    empty.once_key = Some(OnceKey::default());
    finish_frame(&mut st, &empty).unwrap();
    assert_eq!(
        st.once_used.len(),
        1,
        "an empty once key is Python's falsy ()"
    );
}

#[test]
fn a_command_that_left_resolving_stays_where_it_is() {
    let reg = reg();
    let mut st = bare_state(12);
    let u = add_card(&mut st, "GD01-001", 0, Zone::Hand);
    let fi = frame_on(
        &mut st,
        program_labelled(&reg, "GD03-115#a0"),
        i32::from(u),
        FrameKind::Command,
    );
    let f = st.frames[fi];
    st.frame_pop();
    finish_frame(&mut st, &f).unwrap();
    assert_eq!(st.cards[usize::from(u)].zone, Zone::Hand);
    assert_eq!(st.version, 0);
}

// ---------------------------------------------------------------------------------------------
// Deck reorders, ToDeck ordering

#[test]
fn ordered_cards_go_to_the_top_or_the_bottom() {
    let mut st = bare_state(13);
    let deck: Vec<Uid> = (0..5)
        .map(|_| add_card(&mut st, "GD01-001", 0, Zone::Deck))
        .collect();
    super::cards::place_ordered(&mut st, 0, &[deck[3], deck[0]], false).unwrap();
    assert_eq!(
        st.zone(0, Zone::Deck),
        [deck[3], deck[0], deck[1], deck[2], deck[4]]
    );
    assert_eq!(st.version, 1);
    super::cards::place_ordered(&mut st, 0, &[deck[3], deck[2]], true).unwrap();
    assert_eq!(
        st.zone(0, Zone::Deck),
        [deck[0], deck[1], deck[4], deck[3], deck[2]]
    );
    assert_eq!(st.version, 2);
    let hand = add_card(&mut st, "GD01-001", 0, Zone::Hand);
    assert_eq!(
        super::cards::place_ordered(&mut st, 0, &[hand], false),
        Err(EngineError::Value(
            "list.remove(x): x not in list".to_owned()
        ))
    );
}

#[test]
fn to_deck_orders_each_owners_group() {
    let reg = reg();
    let mut st = bare_state(14);
    let d0: Vec<Uid> = (0..4)
        .map(|_| add_card(&mut st, "GD01-001", 0, Zone::Deck))
        .collect();
    let d1: Vec<Uid> = (0..2)
        .map(|_| add_card(&mut st, "GD01-001", 1, Zone::Deck))
        .collect();
    // GD05-002#a1 pc 5: ToDeck(..., bottom=True, shuffle=False); the cards were already moved
    // and the owners' groups recorded in __todeck (interp.py:781-812).
    let fi = frame_on(
        &mut st,
        program_labelled(&reg, "GD05-002#a1"),
        -1,
        FrameKind::Trigger,
    );
    st.frame_mut(fi).pc = 5;
    let ins = instr(&st, &reg, fi, 5);
    let Node::ToDeck(td) = reg.node(ins) else {
        panic!("not a ToDeck")
    };
    assert!(td.bottom && !td.shuffle);
    let todeck = st.engine_vars().todeck;
    let group = [d0[1], d0[2], d1[0], d1[1]].map(i32::from);
    st.var_set(fi, todeck, &group).unwrap();
    assert_eq!(execute(&mut st, &reg, fi, ins), Ok(Status::Wait));
    let d = st.pending().unwrap();
    assert_eq!((d.hdr.player, d.hdr.kind), (0, DecisionKind::Arrange));
    assert_eq!(d.hdr.prompt, Prompt::Engine(EnginePrompt::OrderTopFirst));
    assert_eq!(d.hdr.ctx(CtxKey::Owner), 0);
    let sel = |u: Uid| Action::new(ActionKind::Select, i32::from(u), -1, -1);
    assert_eq!(d.options, [sel(d0[1]), sel(d0[2])]);
    // Owner 0 puts d0[2] first: placed, then owner 1 is asked.
    assert_eq!(
        super::cards::r_to_deck(&mut st, fi, td, sel(d0[2])),
        Ok(Status::Wait)
    );
    assert_eq!(st.zone(0, Zone::Deck), [d0[0], d0[3], d0[2], d0[1]]);
    let d = st.pending().unwrap();
    assert_eq!((d.hdr.player, d.hdr.ctx(CtxKey::Owner)), (1, 1));
    assert_eq!(d.options, [sel(d1[0]), sel(d1[1])]);
    assert_eq!(st.buf(fi), [d0[2], d0[1]].map(i32::from));
    assert_eq!(
        super::cards::r_to_deck(&mut st, fi, td, sel(d1[1])),
        Ok(Status::Next)
    );
    assert_eq!(st.zone(1, Zone::Deck), [d1[1], d1[0]]);
    assert!(st.var_get(fi, todeck).is_none(), "__todeck is deleted");
    assert!(st.buf(fi).is_empty());
    assert_eq!(st.version, 2, "one touch per placed group");
}

// ---------------------------------------------------------------------------------------------
// Step hooks that need only layer L1

#[test]
fn looked_cards_return_to_the_bottom_in_pythons_random_order() {
    let reg = reg();
    let mut st = bare_state(7);
    let deck: Vec<Uid> = (0..6)
        .map(|_| add_card(&mut st, "GD01-001", 0, Zone::Deck))
        .collect();
    let hand = add_card(&mut st, "GD01-001", 0, Zone::Hand);
    for &u in &deck {
        st.cards[usize::from(u)].known = 3;
    }
    let looked = st.engine_vars().looked;
    let fi = frame_on(
        &mut st,
        reg.system_program(SystemProgram::Pair),
        -1,
        FrameKind::Trigger,
    );
    st.var_set(fi, looked, &[1, i32::from(hand), 3, 4]).unwrap();
    assert_eq!(
        hooks::run_step(&mut st, &reg, fi, StepParams::ReturnLookedBottom),
        Ok(true)
    );
    // Python: SplitMix64(7).shuffle([1, 3, 4]) == [3, 4, 1], state 4354685564936845361.
    assert_eq!(st.zone(0, Zone::Deck), [0, 2, 5, 3, 4, 1]);
    assert_eq!(st.rng.state, 4_354_685_564_936_845_361);
    let known: Vec<u8> = deck
        .iter()
        .map(|&u| st.cards[usize::from(u)].known)
        .collect();
    assert_eq!(
        known,
        [3, 0, 3, 0, 0, 3],
        "nobody knows the random order (rule 4-2-2)"
    );
    assert_eq!(st.version, 1);
    // One looked card: no randrange draw and no knowledge change.
    st.var_set(fi, looked, &[2]).unwrap();
    let rng = st.rng.state;
    assert_eq!(
        hooks::run_step(&mut st, &reg, fi, StepParams::ReturnLookedBottom),
        Ok(true)
    );
    assert_eq!(st.zone(0, Zone::Deck), [0, 5, 3, 4, 1, 2]);
    assert_eq!((st.rng.state, st.cards[2].known, st.version), (rng, 3, 2));
    // None left in the deck: false and no touch.
    st.var_set(fi, looked, &[i32::from(hand)]).unwrap();
    assert_eq!(
        hooks::run_step(&mut st, &reg, fi, StepParams::ReturnLookedBottom),
        Ok(false)
    );
    assert_eq!(st.version, 2);
}

#[test]
fn a_player_looks_at_their_own_top_card() {
    let reg = reg();
    let mut st = bare_state(15);
    st.active = 1;
    let top = add_card(&mut st, "GD01-001", 1, Zone::Deck);
    let _second = add_card(&mut st, "GD01-001", 1, Zone::Deck);
    let t1 = var(&reg, "t1");
    let fi = frame_on(
        &mut st,
        reg.system_program(SystemProgram::Pair),
        -1,
        FrameKind::Trigger,
    );
    let looks = |player| StepParams::EB01aLookOwnTop { player, var: t1 };
    assert_eq!(
        hooks::run_step(&mut st, &reg, fi, looks(P::Active)),
        Ok(true)
    );
    assert_eq!(st.var_get(fi, t1), Some(&[i32::from(top)][..]));
    assert_eq!(st.cards[usize::from(top)].known, 0b10);
    assert_eq!(st.version, 0, "knowledge changes do not touch");
    // #7 shares the body; the controller's (player 0's) deck is empty.
    let own = StepParams::EB01bLookTopOwn {
        player: P::You,
        var: t1,
    };
    assert_eq!(hooks::run_step(&mut st, &reg, fi, own), Ok(false));
    assert_eq!(st.var_get(fi, t1), Some(&[][..]));
}

#[test]
fn shared_once_keys_are_added_once() {
    let reg = reg();
    let mut st = bare_state(16);
    let u = add_card(&mut st, "GD01-001", 0, Zone::Battle);
    st.cards[usize::from(u)].zone_seq = 4;
    let fi = frame_on(
        &mut st,
        reg.system_program(SystemProgram::Pair),
        i32::from(u),
        FrameKind::Trigger,
    );
    let use_once = StepParams::GD05aUseOnce { tag: -205_006 };
    assert_eq!(hooks::run_step(&mut st, &reg, fi, use_once), Ok(true));
    assert_eq!(hooks::run_step(&mut st, &reg, fi, use_once), Ok(true));
    let key = OnceKey::from_elems(&[
        OnceElem::Int(-205_006),
        OnceElem::Int(i32::from(u)),
        OnceElem::Int(4),
    ])
    .unwrap();
    assert_eq!(st.once_used, [key]);
}

#[test]
fn support_without_a_target_is_not_used() {
    let reg = reg();
    let mut st = bare_state(17);
    let fi = frame_on(
        &mut st,
        reg.system_program(SystemProgram::Pair),
        -1,
        FrameKind::Activated,
    );
    assert_eq!(
        hooks::run_step(&mut st, &reg, fi, StepParams::SupportUsed),
        Ok(false)
    );
    let t = st.engine_vars().t;
    st.var_set(fi, t, &[]).unwrap();
    assert_eq!(
        hooks::run_step(&mut st, &reg, fi, StepParams::SupportUsed),
        Ok(false)
    );
}

#[test]
fn a_resolving_command_hook_ignores_cards_elsewhere() {
    let reg = reg();
    let mut st = bare_state(18);
    let u = add_card(&mut st, "GD01-001", 0, Zone::Trash);
    let fi = frame_on(
        &mut st,
        reg.system_program(SystemProgram::Pair),
        i32::from(u),
        FrameKind::Command,
    );
    assert_eq!(
        hooks::run_step(&mut st, &reg, fi, StepParams::GD05bResolvingCommandToTrash),
        Ok(false)
    );
    assert_eq!(st.version, 0);
}

// ---------------------------------------------------------------------------------------------
// Delayed triggers

/// The program of `number`'s ability that holds its first delayed trigger, the instruction's
/// pc, and the trigger's walk index (`delayed` of the card entry).
fn delayed_site(reg: &Registry, number: &str) -> (ProgramId, u16, NodeId, u16) {
    let def = reg.db().get(number).unwrap().def_id;
    let entry = reg.card(def);
    let &(n, node, _) = entry.delayed.first().expect("a delayed trigger");
    for &aid in entry.own.iter().chain(entry.unit.iter()) {
        if let Some(p) = reg.ability(aid).program
            && let Some(pc) = reg.program(p).instrs.iter().position(|&i| i == node)
        {
            return (p, u16::try_from(pc).unwrap(), node, n);
        }
    }
    panic!("{number}: no program holds the delayed trigger")
}

#[test]
fn delayed_triggers_are_keyed_by_their_card_and_index() {
    let reg = reg();
    let mut st = bare_state(19);
    st.turn = 4;
    // GD04-035: DelayedTrigger(..., this_turn, bind_vars=('t1',)), walk index 0.
    let (p, pc, node, n) = delayed_site(&reg, "GD04-035");
    assert_eq!(n, 0);
    let host = add_card(&mut st, "GD04-035", 0, Zone::Battle);
    let bound = add_card(&mut st, "GD01-001", 1, Zone::Battle);
    st.cards[usize::from(bound)].zone_seq = 3;
    let fi = frame_on(&mut st, p, i32::from(host), FrameKind::Trigger);
    st.frame_mut(fi).pc = pc;
    st.var_set(fi, var(&reg, "t1"), &[i32::from(bound)])
        .unwrap();
    assert_eq!(execute(&mut st, &reg, fi, node), Ok(Status::Next));
    assert!(st.frames[fi].did);
    assert_eq!(st.delayed.len(), 1);
    let le = st.delayed[0];
    assert_eq!(
        le.effect,
        ContRef::AbilityGrant {
            card: reg.str_id("GD04-035").unwrap(),
            index: 0
        }
    );
    assert_eq!(
        (
            le.controller,
            le.source_uid,
            le.duration,
            le.created_turn,
            le.battle_id
        ),
        (0, i32::from(host), Duration::ThisTurn, 4, -1)
    );
    assert_eq!(
        (le.player, le.uses, le.filters, le.aux, le.serial),
        (-1, 0, None, -1, -1)
    );
    assert_eq!(st.lasting_targets(&le), [(bound, 3)]);
    assert_eq!(
        (st.version, st.next_lasting_serial),
        (0, 0),
        "no touch, no serial"
    );
    assert!(st.lasting.is_empty());
}

#[test]
fn delayed_triggers_need_their_card() {
    let reg = reg();
    let mut st = bare_state(20);
    let (p, pc, node, _) = delayed_site(&reg, "GD03-120");
    // A hostless frame, and a host whose card does not carry the trigger.
    let other = add_card(&mut st, "GD01-001", 0, Zone::Battle);
    for host in [-1, i32::from(other)] {
        let fi = frame_on(&mut st, p, host, FrameKind::Trigger);
        st.frame_mut(fi).pc = pc;
        assert_eq!(
            execute(&mut st, &reg, fi, node),
            Err(EngineError::Internal(
                "delayed trigger not registered on its card".to_owned()
            ))
        );
        st.frame_pop();
    }
    assert!(st.delayed.is_empty());
}

// ---------------------------------------------------------------------------------------------
// Python-exported states

/// Whether the top frame of `st` waits on the pending decision (a game that ended during the
/// rules management after a `WAIT` keeps the waiting frame without a decision).
fn asks(st: &GameState) -> bool {
    st.pending.is_some() && st.frames.last().is_some_and(|f| f.waiting)
}

/// The fixture states whose top frame waits on the pending decision, with the waiting
/// instruction.
fn waiting_fixtures() -> Vec<(&'static Value, GameState, NodeId)> {
    fixtures::states()
        .into_iter()
        .filter_map(|(rec, st)| {
            let f = *st.frames.last()?;
            if !asks(&st) {
                return None;
            }
            let ins = st.registry().program(f.program).instrs[usize::from(f.pc)];
            Some((rec, st, ins))
        })
        .collect()
}

/// `st` with the waiting decision taken back: `waiting = False`, `pending = None`.
fn unasked(st: &GameState) -> GameState {
    let mut c = st.clone();
    let top = c.frames.len() - 1;
    c.frame_mut(top).waiting = false;
    c.clear_pending();
    c
}

/// Whether re-running the waiting instruction's handler reproduces Python's decision: every
/// effect decision except the follow-up picks of a Choose or a Discard, which the resumers
/// raise (checked in Python on 300 fixture games, 2,547 decisions).
fn reraisable(st: &GameState, reg: &Registry, ins: NodeId) -> bool {
    let top = st.frames.len() - 1;
    match reg.node(ins) {
        Node::Choose(_) | Node::Discard(_) => st.buf(top).is_empty(),
        _ => true,
    }
}

#[test]
fn ask_may_decisions_of_python_states_are_reraised_exactly() {
    let mut n = 0;
    for (rec, st, ins) in waiting_fixtures() {
        let reg = Arc::clone(st.registry());
        if !matches!(reg.node(ins), Node::AskMay(_)) {
            continue;
        }
        let mut c = unasked(&st);
        let top = c.frames.len() - 1;
        assert_eq!(
            execute(&mut c, &reg, top, ins),
            Ok(Status::Wait),
            "{}",
            rec["id"]
        );
        assert_eq!(
            state_digest(&c),
            rec["digest"].as_str().unwrap(),
            "{}",
            rec["id"]
        );
        n += 1;
    }
    assert!(n >= 5, "the fixtures hold {n} AskMay decisions");
}

/// L4 gate (needs the view, L2, and the core primitives, L3): every effect decision of the
/// Python fixture states and of the tier-1 corpus snapshots is raised again by its handler
/// with Python's options, prompt and context, leaving the state byte-identical (digest).
#[test]
fn effect_decisions_reraise_on_python_states() {
    let mut n = 0;
    let mut kinds = std::collections::BTreeMap::new();
    let mut check = |id: &str, st: &GameState, digest: &str| {
        let reg = Arc::clone(st.registry());
        let f = *st.frames.last().unwrap();
        let ins = reg.program(f.program).instrs[usize::from(f.pc)];
        if !reraisable(st, &reg, ins) {
            return;
        }
        let mut c = unasked(st);
        let top = c.frames.len() - 1;
        let status = execute(&mut c, &reg, top, ins);
        assert_eq!(status, Ok(Status::Wait), "{id}");
        assert_eq!(
            state_digest(&c),
            digest,
            "{id}: the re-raised decision differs"
        );
        *kinds.entry(reg.node(ins).kind().py_name()).or_insert(0) += 1;
        n += 1;
    };
    for (rec, st, _) in waiting_fixtures() {
        check(
            rec["id"].as_str().unwrap(),
            &st,
            rec["digest"].as_str().unwrap(),
        );
    }
    for (id, snap) in corpus_snapshots() {
        let st =
            from_canonical(fixtures::registry(), &snap).unwrap_or_else(|e| panic!("{id}: {e}"));
        if asks(&st) {
            let digest = state_digest(&st);
            check(&id, &st, &digest);
        }
    }
    println!("re-raised {n} effect decisions: {kinds:?}");
    assert!(n >= 100, "only {n} effect decisions were re-raised");
}

/// L4 gate (needs L2 and L3): Python's `apply` of an effect decision is `pending = None;
/// action_count += 1; resume(st, action)` and then `advance`, which runs the top frame while
/// frames remain (`game.py:734-760`, `:780-818`). Starting from every snapshot of the tier-1
/// game and search traces whose top frame waits, the chosen actions are replayed through
/// `resume` and `run_top_frame` for as long as only frames run; whenever the next decision
/// is reached that way, its state digest must equal the trace's `digest16` (or, at the end of
/// the game, `final_digest`). A chain stops where the step machine of layer L5 would take
/// over (no frame left) or the action cap is reached.
#[test]
fn effect_decisions_replay_through_resume() {
    let mut checked = 0;
    for path in corpus_files() {
        let text = std::fs::read_to_string(&path).unwrap();
        let recs: Vec<Value> = text
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        let at = |k: usize| format!("{}:{}", path.display(), k + 1);
        // The position of the first decision or outcome record after `k`.
        let next_step = |k: usize| {
            (k + 1..recs.len()).find(|&n| recs[n]["t"] == "decision" || recs[n]["t"] == "outcome")
        };
        for (k, rec) in recs.iter().enumerate() {
            if rec["t"] != "snapshot" {
                continue;
            }
            let Some(mut st) = waiting_state(rec) else {
                continue;
            };
            let Some(mut d) = next_step(k) else {
                continue;
            };
            // A snapshot precedes the decision with its index (oracle/traces.py _play).
            assert_eq!(recs[d]["i"], rec["i"], "{}", at(k));
            assert_eq!(
                &state_digest(&st)[..16],
                recs[d]["digest16"].as_str().unwrap(),
                "{}",
                at(k)
            );
            while recs[d]["t"] == "decision" && asks(&st) {
                // game.apply (game.py:780-791), then advance while frames run (:734-745).
                st.clear_pending();
                st.action_count += 1;
                super::resume(&mut st, action_of(&recs[d]["chosen"]))
                    .unwrap_or_else(|e| panic!("{}: resume: {e}", at(d)));
                while st.pending.is_none()
                    && st.winner.is_none()
                    && !st.frames.is_empty()
                    && st.action_count <= st.max_actions()
                {
                    super::run_top_frame(&mut st)
                        .unwrap_or_else(|e| panic!("{}: run_top_frame: {e}", at(d)));
                }
                let Some(n) = next_step(d) else {
                    break;
                };
                if st.winner.is_some() {
                    if recs[n]["t"] == "outcome" {
                        let want = recs[n]["final_digest"].as_str().unwrap();
                        assert_eq!(state_digest(&st), want, "{}: final state", at(n));
                        checked += 1;
                    }
                    break;
                }
                if st.pending.is_none() || recs[n]["t"] != "decision" {
                    break;
                }
                let want = recs[n]["digest16"].as_str().unwrap();
                assert_eq!(
                    &state_digest(&st)[..16],
                    want,
                    "{}: state before the decision",
                    at(n)
                );
                // Python's `_rules_checked`, which decides `effect_hits.clear()` later.
                assert_eq!(
                    Some(st.rules_checked),
                    recs[n]["rc"].as_i64(),
                    "{}: rc",
                    at(n)
                );
                checked += 1;
                d = n;
            }
        }
    }
    println!("replayed {checked} effect decisions through resume");
    // The committed tier-1 corpus holds exactly 17 effect decisions that are reached through
    // frames alone (measured at the wave-2b integration; 126 waiting snapshots in total). The
    // floor pins that coverage so the test can never pass vacuously; the full games of the
    // differential corpus exercise the rest.
    assert!(
        checked >= 17,
        "only {checked} effect decisions were replayed"
    );
}

fn tier1() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../oracle/corpus/v1/tier1")
}

/// The tier-1 trace files of whole games (`kind` game or search; PLAN §8.2), in name order.
fn corpus_files() -> Vec<PathBuf> {
    let mut out = Vec::new();
    for set in [
        "full",
        "vanilla",
        "fuzz",
        "coverage",
        "worlds",
        "baselines",
        "search",
        "views",
    ] {
        let Ok(dir) = std::fs::read_dir(tier1().join(set)) else {
            continue;
        };
        let mut files: Vec<PathBuf> = dir
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "ndjson"))
            .collect();
        files.sort();
        out.extend(files);
    }
    assert!(
        !out.is_empty(),
        "no tier-1 corpus under {}",
        tier1().display()
    );
    out
}

fn snapshot_of(rec: &Value) -> StateSnapshot {
    StateSnapshot {
        state: rec["state"].clone(),
        continuous: rec["continuous"].clone(),
        filter_sets: rec["filter_sets"].clone(),
        rules_checked: rec["rules_checked"].as_i64().unwrap(),
    }
}

/// Every snapshot of the tier-1 game traces whose top frame waits, with an id.
fn corpus_snapshots() -> Vec<(String, StateSnapshot)> {
    let mut out = Vec::new();
    for path in corpus_files() {
        let text = std::fs::read_to_string(&path).unwrap();
        for (k, line) in text.lines().enumerate() {
            if !line.contains("\"t\":\"snapshot\"") {
                continue;
            }
            let rec: Value = serde_json::from_str(line).unwrap();
            let waiting = rec["state"]["frames"]
                .as_array()
                .and_then(|f| f.last())
                .is_some_and(|f| f["waiting"] == true);
            if waiting {
                out.push((format!("{}:{}", path.display(), k + 1), snapshot_of(&rec)));
            }
        }
    }
    out
}

/// The state of a snapshot record whose top frame waits on the pending decision.
fn waiting_state(rec: &Value) -> Option<GameState> {
    let st = from_canonical(fixtures::registry(), &snapshot_of(rec)).expect("a corpus snapshot");
    asks(&st).then_some(st)
}

/// A trace action `[kind, a, b, c]`.
fn action_of(v: &Value) -> Action {
    let a = v.as_array().expect("an action");
    let int = |x: &Value| i32::try_from(x.as_i64().unwrap()).unwrap();
    Action::new(
        ActionKind::from_value(a[0].as_str().unwrap()).unwrap(),
        int(&a[1]),
        int(&a[2]),
        int(&a[3]),
    )
}
