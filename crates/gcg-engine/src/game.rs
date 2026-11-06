//! Layer L5 of the engine (docs/dev/PLAN.md §10.1, docs/dev/contracts.md): the game procedure
//! of `engine/game.py` — deck construction checks (rule 6-1), `new_game` and setup (rule 6-2),
//! the step machine of the turn (rules 7-8), action steps (rule 9), legal options in Python's
//! exact order per decision kind, `apply` with payment and alt-play decoding, trigger batches
//! (rule 10-1-6), the `advance` loop and its caps.
//!
//! Every function cites the Python lines it mirrors; `touch` calls, RNG draws, event groups and
//! iteration orders are Python's one for one.

use std::collections::BTreeMap;
use std::sync::Arc;

use gcg_core::rng::SplitMix64;
use gcg_core::small::InlineVec;
use gcg_data::{CardDb, CardType, DeckList, DefId, ImplementationStatus, MAIN_DECK_TYPES};
use gcg_effects::{
    AbilityEntry, AbilityId, Activated, CondId, ContView, CostId, Ev, FilterId, KeywordProgram, Kw,
    Node, PlayModifier, ProgramId, Registry, RuleKind, SelId, SystemProgram, Timing,
};

use crate::view;
use crate::{
    ALT_PLAY_BASE, Action, ActionKind, ApplyError, BOTH_KNOW, Battle, Ctx, CtxVars, DecisionHdr,
    DecisionKind, DeckConstructionError, Derived, Duration, EndReason, EngineError, EnginePrompt,
    EvKey, EventPayload, FrameKind, GameConst, GameState, HAND_LIMIT, HistKind, IllegalActionError,
    IntKey, MAIN_DECK_SIZE, MAX_COLORS, MAX_COPIES, MoveOpts, NO_ARG, NO_PROGRESS_STEPS,
    NewGameError, NewGameOptions, OnceElem, OnceKey, PLAYER_TARGET, Phase, Prompt, PushFrame,
    RESOURCE_DECK_SIZE, SHIELD_COUNT, START_HAND, SUPPORT_AID, Step, TriggerInst, Uid,
    UnimplementedCardError, Zone,
};

type A = ActionKind;

// ---------------------------------------------------------------------------------------------
// small helpers

/// `st.zones[p][z][0]` (`game.py:172`, `:182`, `:199`): Python's `IndexError` on an empty list.
fn top_of(st: &GameState, p: u8, z: Zone) -> Result<Uid, EngineError> {
    st.zone(p, z)
        .first()
        .copied()
        .ok_or_else(|| EngineError::Index("list index out of range".to_owned()))
}

/// An `Action` parameter used as a uid (`st.cards[x]`): Python's `IndexError` outside the card
/// list. A negative parameter only reaches here through `apply(check=False)` with an action no
/// decision offered.
pub(crate) fn uid_at(st: &GameState, x: i32) -> Result<Uid, EngineError> {
    usize::try_from(x)
        .ok()
        .filter(|&i| i < st.cards.len())
        .and_then(|i| Uid::try_from(i).ok())
        .ok_or_else(|| EngineError::Index("list index out of range".to_owned()))
}

/// A player index held in an `Action` parameter (`game.py:793`).
fn player_at(x: i32) -> Result<u8, EngineError> {
    u8::try_from(x)
        .ok()
        .filter(|&p| p < 2)
        .ok_or_else(|| EngineError::Internal(format!("player {x} out of range")))
}

fn to_i32(x: i64, what: &'static str) -> Result<i32, EngineError> {
    i32::try_from(x).map_err(|_| EngineError::Overflow { what })
}

/// `st.pending = Decision(player, kind, options, prompt)` with `context=()` (every decision of
/// `game.py`).
fn decide(
    st: &mut GameState,
    player: u8,
    kind: DecisionKind,
    options: &[Action],
    prompt: EnginePrompt,
) {
    let hdr = DecisionHdr {
        player,
        kind,
        prompt: Prompt::Engine(prompt),
        context: InlineVec::new(),
    };
    st.set_pending(hdr, options);
}

/// `Ctx(controller, host, card_uid)` with no vars and no event (`view.py:32-44`).
fn ctx(controller: u8, host: i32, card_uid: i32) -> Ctx {
    Ctx::new(
        controller,
        host,
        card_uid,
        CtxVars::Empty,
        EventPayload::default(),
    )
}

/// `c is not d.TRUE` (`game.py:405`, `:443`, `:623`): `TRUE = And(())` (`effects/dsl.py:579`);
/// the IR is hash-consed, so every `And(())` is that node, and evaluating one is `True` anyway.
fn is_true(reg: &Registry, c: CondId) -> bool {
    matches!(reg.node(c.node()), Node::And(x) if x.conds.is_empty())
}

fn activated_of<'r>(reg: &'r Registry, a: &AbilityEntry) -> Option<&'r Activated> {
    match reg.node(a.ability.node()) {
        Node::Activated(x) => Some(x),
        _ => None,
    }
}

fn play_modifier_of<'r>(reg: &'r Registry, a: &AbilityEntry) -> Option<&'r PlayModifier> {
    match reg.node(a.ability.node()) {
        Node::PlayModifier(x) => Some(x),
        _ => None,
    }
}

/// `assert isinstance(ab, d.Activated)` (`game.py:438`, `:887`).
fn expect_activated<'r>(reg: &'r Registry, a: &AbilityEntry) -> Result<&'r Activated, EngineError> {
    activated_of(reg, a).ok_or_else(|| EngineError::Assertion(String::new()))
}

/// `program_id` of an ability that has one (`R.abilities[aid].program_id`); Activated and
/// Command abilities always register a program (`effects/registry.py:229-236`).
fn program_of(a: &AbilityEntry) -> Result<ProgramId, EngineError> {
    a.program
        .ok_or_else(|| EngineError::Internal(format!("ability {} has no program", a.aid.0)))
}

/// The once-per-turn key `(aid, host, zone_seq)` of an Activated ability (`game.py:439`,
/// `:892`).
fn activation_once_key(aid: AbilityId, host: Uid, zone_seq: i32) -> Result<OnceKey, EngineError> {
    OnceKey::from_elems(&[
        OnceElem::Int(i32::from(aid.0)),
        OnceElem::Int(i32::from(host)),
        OnceElem::Int(zone_seq),
    ])
}

fn n_pending_triggers(st: &GameState) -> usize {
    st.triggers.len() - st.pending_start as usize
}

/// `R.require(def_id)` (`effects/registry.py:143-147`).
fn require(reg: &Registry, def: DefId) -> Result<(), UnimplementedCardError> {
    match reg.unimplemented_error(def) {
        None => Ok(()),
        Some(reason) => Err(UnimplementedCardError {
            card_number: reg.db().by_id(def).card_number.clone(),
            reason: reason.into(),
        }),
    }
}

// ---------------------------------------------------------------------------------------------
// deck construction (engine/game.py:58-100)

/// `deck_construction_problems(deck)` (`engine/game.py:58-100`) on the card numbers of `deck`'s
/// definitions.
#[must_use]
pub fn deck_construction_problems(db: &CardDb, deck: &DeckList) -> Vec<String> {
    let main: Vec<&str> = deck
        .main
        .iter()
        .map(|&d| &*db.by_id(d).card_number)
        .collect();
    let res: Vec<&str> = deck
        .resources
        .iter()
        .map(|&d| &*db.by_id(d).card_number)
        .collect();
    deck_construction_problems_by_number(db, &main, &res)
}

/// `deck_construction_problems(DeckList(main, resources))` (`engine/game.py:58-100`) verbatim on
/// the card numbers as written: the size problems, then per `normalize_id(n) or n` key in
/// sorted order the unknown, type and copy problems, then the colours, then per
/// `sorted(set(resources))` the resource problems.
#[must_use]
pub fn deck_construction_problems_by_number(
    db: &CardDb,
    main: &[&str],
    resources: &[&str],
) -> Vec<String> {
    let mut problems = Vec::new();
    if main.len() != MAIN_DECK_SIZE {
        problems.push(format!(
            "deck has {} cards; exactly {MAIN_DECK_SIZE} required (6-1-1)",
            main.len()
        ));
    }
    if resources.len() != RESOURCE_DECK_SIZE {
        problems.push(format!(
            "resource deck has {} cards; exactly {RESOURCE_DECK_SIZE} required (6-1-1)",
            resources.len()
        ));
    }
    let mut colors = Vec::new();
    // Counter(db.normalize_id(n) or n for n in deck.main), iterated sorted (rule 2-1-1)
    let mut printings: BTreeMap<&str, usize> = BTreeMap::new();
    for &n in main {
        *printings
            .entry(db.normalize_id(n).unwrap_or(n))
            .or_insert(0) += 1;
    }
    for (number, n) in printings {
        let Some(cd) = db.get(number) else {
            problems.push(format!("unknown card {number}"));
            continue;
        };
        if !MAIN_DECK_TYPES.contains(&cd.card_type) {
            problems.push(format!(
                "{number} is a {} card and cannot be in the deck (6-1-1-1)",
                cd.card_type.as_str()
            ));
        }
        if n > MAX_COPIES {
            problems.push(format!(
                "{n} copies of {number}; at most {MAX_COPIES} allowed (6-1-1-3)"
            ));
        }
        if let Some(c) = cd.color
            && !colors.contains(&c)
        {
            colors.push(c);
        }
    }
    if colors.len() > MAX_COLORS {
        let mut names: Vec<&str> = colors.iter().map(|c| c.as_str()).collect();
        names.sort();
        problems.push(format!(
            "deck uses {} colours ({}); at most {MAX_COLORS} (6-1-1-2)",
            colors.len(),
            names.join(", ")
        ));
    }
    let mut res: Vec<&str> = resources.to_vec();
    res.sort();
    res.dedup();
    for number in res {
        let ok = db
            .get(number)
            .is_some_and(|cd| cd.card_type == CardType::Resource);
        if !ok {
            problems.push(format!(
                "{number} is not a Resource card and cannot be in the resource deck (6-1-1-4)"
            ));
        }
    }
    problems
}

// ---------------------------------------------------------------------------------------------
// setup (engine/game.py:107-201)

/// `new_game(decks, seed, *, chooser, turn_limit, max_actions, validate)`
/// (`engine/game.py:107-158`). RNG order: player 0's shuffle and cut, player 1's shuffle and
/// cut, then the die roll only when `chooser` is `None`. Uids are assigned after
/// the shuffles: player 0's deck, player 1's deck, then the two resource decks.
///
/// # Errors
/// [`NewGameError::Construction`] for the first player whose deck breaks rule 6-1 (with
/// `validate`), [`NewGameError::Unimplemented`] for a main-deck card without a script
/// (`R.require`), [`NewGameError::Engine`] for anything `advance` raises.
pub fn new_game(
    reg: &Arc<Registry>,
    decks: [&DeckList; 2],
    seed: u64,
    opts: NewGameOptions,
) -> Result<GameState, NewGameError> {
    let db = reg.db();
    if opts.validate {
        for (p, deck) in (0u8..).zip(decks) {
            let problems = deck_construction_problems(db, deck);
            if !problems.is_empty() {
                return Err(NewGameError::Construction(DeckConstructionError {
                    player: p,
                    problems,
                }));
            }
        }
    }
    let mut rng = SplitMix64::new(seed);
    let mut main_ids: [Vec<DefId>; 2] = [Vec::new(), Vec::new()];
    let mut res_ids: [Vec<DefId>; 2] = [Vec::new(), Vec::new()];
    for p in 0..2 {
        let mut ids = decks[p].main.to_vec();
        for &i in &ids {
            require(reg, i).map_err(NewGameError::Unimplemented)?;
        }
        rng.shuffle(&mut ids);
        if ids.len() > 1 {
            let k = 1 + rng.randrange_usize(ids.len() - 1);
            ids.rotate_left(k);
        }
        main_ids[p] = ids;
        res_ids[p] = decks[p].resources.to_vec();
    }
    let sorted = |v: &[DefId]| -> Box<[DefId]> {
        let mut s = v.to_vec();
        s.sort();
        s.into_boxed_slice()
    };
    let konst = GameConst::new(
        Arc::clone(reg),
        [sorted(&main_ids[0]), sorted(&main_ids[1])],
        [sorted(&res_ids[0]), sorted(&res_ids[1])],
        opts.turn_limit,
        opts.max_actions,
    )?;
    let mut st = GameState::new(Arc::new(konst), seed);
    st.rng = rng;
    crate::diag::note_interpretation(crate::diag::Interpretation::I05);
    for p in 0u8..2 {
        for &def in &main_ids[usize::from(p)] {
            let uid = crate::core::new_card(&mut st, def, p, Zone::Deck, false)?;
            st.cards[usize::from(uid)].known = 0;
        }
    }
    for p in 0u8..2 {
        for &def in &res_ids[usize::from(p)] {
            let uid = crate::core::new_card(&mut st, def, p, Zone::ResourceDeck, false)?;
            st.cards[usize::from(uid)].known = 1 << p;
        }
    }
    st.setup_chooser = match opts.chooser {
        Some(c) => c,
        None => u8::from(st.rng.randrange(2) == 1),
    };
    st.phase = Phase::Setup;
    st.step = Step::SetupChooseFirst;
    advance(&mut st)?;
    Ok(st)
}

/// `new_game` on Python's string `DeckList` (`engine/game.py:107-158`): with `opts.validate`
/// the construction problems per player (player 0 first) before any lookup; then per player in
/// Python's order `R.db[n]` for each main number in caller order (unknown is Python's
/// `KeyError`, reachable only without `validate`), `R.require`, the resource numbers; then
/// exactly [`new_game`].
///
/// # Errors
/// As [`new_game`], plus [`EngineError::Key`] for an unknown number.
pub fn new_game_by_number(
    reg: &Arc<Registry>,
    decks: [(&[&str], &[&str]); 2],
    seed: u64,
    opts: NewGameOptions,
) -> Result<GameState, NewGameError> {
    let db = reg.db();
    if opts.validate {
        for (p, (main, res)) in (0u8..).zip(decks) {
            let problems = deck_construction_problems_by_number(db, main, res);
            if !problems.is_empty() {
                return Err(NewGameError::Construction(DeckConstructionError {
                    player: p,
                    problems,
                }));
            }
        }
    }
    let lookup = |n: &str| -> Result<DefId, NewGameError> {
        db.get(n)
            .map(|cd| cd.def_id)
            .ok_or_else(|| NewGameError::Engine(EngineError::Key(n.to_owned())))
    };
    let mut lists: Vec<DeckList> = Vec::with_capacity(2);
    for (main, res) in decks {
        let ids = main
            .iter()
            .map(|n| lookup(n))
            .collect::<Result<Vec<_>, _>>()?;
        for &i in &ids {
            require(reg, i).map_err(NewGameError::Unimplemented)?;
        }
        let rids = res
            .iter()
            .map(|n| lookup(n))
            .collect::<Result<Vec<_>, _>>()?;
        lists.push(DeckList {
            main: ids.into_boxed_slice(),
            resources: rids.into_boxed_slice(),
        });
    }
    let opts = NewGameOptions {
        validate: false,
        ..opts
    };
    new_game(reg, [&lists[0], &lists[1]], seed, opts)
}

/// `_setup_step(st)` (`engine/game.py:161-191`, rule 6-2); a setup step without a row does
/// nothing, as in Python.
fn setup_step(st: &mut GameState) -> Result<(), EngineError> {
    match st.step {
        Step::SetupChooseFirst => {
            let c = st.setup_chooser;
            let opts = [
                Action::new(A::GoFirst, i32::from(c), NO_ARG, NO_ARG),
                Action::new(A::GoFirst, i32::from(1 - c), NO_ARG, NO_ARG),
            ];
            decide(
                st,
                c,
                DecisionKind::ChooseFirst,
                &opts,
                EnginePrompt::ChoosePlayerOne,
            );
        }
        Step::SetupDraw => {
            let first = st.first_player;
            for p in [first, 1 - first] {
                for _ in 0..START_HAND {
                    let uid = top_of(st, p, Zone::Deck)?;
                    crate::core::move_card(st, uid, Zone::Hand, MoveOpts::default())?;
                }
            }
            st.step = Step::SetupRedrawP1;
        }
        Step::SetupRedrawP1 | Step::SetupRedrawP2 => {
            let p = if st.step == Step::SetupRedrawP1 {
                st.first_player
            } else {
                1 - st.first_player
            };
            let opts = [Action::of(A::Keep), Action::of(A::Redraw)];
            decide(st, p, DecisionKind::Redraw, &opts, EnginePrompt::Redraw);
        }
        Step::SetupShields => {
            let first = st.first_player;
            let db = st.db();
            for p in [first, 1 - first] {
                for _ in 0..SHIELD_COUNT {
                    let uid = top_of(st, p, Zone::Deck)?;
                    crate::core::move_card(st, uid, Zone::Shield, MoveOpts::default())?;
                }
                crate::core::new_card(st, db.ex_base().def_id, p, Zone::Base, false)?;
            }
            crate::core::new_card(
                st,
                db.ex_resource().def_id,
                1 - first,
                Zone::ResourceArea,
                false,
            )?;
            st.turn = 1;
            st.active = first;
            st.phase = Phase::Start;
            st.step = Step::ActiveStep;
            st.history_reset();
            st.touch();
        }
        _ => {}
    }
    Ok(())
}

/// `_redraw(st, p)` (`engine/game.py:194-201`, rule 6-2-1-6-1): the hand to the bottom of the
/// deck in hand order, five new cards from the top, then a shuffle of the whole deck.
fn redraw(st: &mut GameState, p: u8) -> Result<(), EngineError> {
    let hand = st.zone(p, Zone::Hand).to_vec();
    let bottom = MoveOpts {
        bottom: true,
        ..MoveOpts::default()
    };
    for uid in hand {
        crate::core::move_card(st, uid, Zone::Deck, bottom)?;
    }
    for _ in 0..START_HAND {
        let uid = top_of(st, p, Zone::Deck)?;
        crate::core::move_card(st, uid, Zone::Hand, MoveOpts::default())?;
    }
    crate::core::shuffle_deck(st, p);
    st.redraws[usize::from(p)] = true;
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// turn procedure (engine/game.py:208-353)

/// `keep(le.duration, le.created_turn, le.controller)` of `_expire` (`engine/game.py:209-216`).
fn keep_lasting(turn: i32, active: u8, duration: Duration, created: i32, controller: u8) -> bool {
    match duration {
        Duration::ThisTurn | Duration::ThisBattle => false,
        Duration::OpponentNextTurn => !(turn > created && active != controller),
        Duration::YourNextTurn => !(turn > created && active == controller),
        Duration::WhileOnField => true,
    }
}

/// `_expire(st)` (`engine/game.py:208-220`, rule 7-6-6-1): always touches.
fn expire(st: &mut GameState) {
    let (turn, active) = (st.turn, st.active);
    let mut arena = st.lasting_arena();
    arena.retain_lasting(|l, _| {
        keep_lasting(turn, active, l.duration, l.created_turn, l.controller)
    });
    arena.retain_delayed(|l, _| {
        keep_lasting(turn, active, l.duration, l.created_turn, l.controller)
    });
    st.touch();
}

/// `st.battle` with `assert b is not None` (`engine/game.py:261-262`).
fn current_battle(st: &GameState) -> Result<Battle, EngineError> {
    st.battle()
        .copied()
        .ok_or_else(|| EngineError::Assertion(String::new()))
}

fn has_rule(dv: &Derived, uid: Uid, kind: RuleKind) -> bool {
    view::rules_of(dv, uid, kind).next().is_some()
}

/// `_turn_step(st)` (`engine/game.py:223-322`): one row of the step machine.
fn turn_step(st: &mut GameState) -> Result<(), EngineError> {
    let s = st.step;
    let p = st.active;
    match s {
        Step::ActiveStep => {
            let dv = view::derived(st)?;
            let reg = st.registry().clone();
            for z in [Zone::Battle, Zone::ResourceArea, Zone::Base] {
                for i in 0..st.zone(p, z).len() {
                    let uid = st.zone(p, z)[i];
                    let c = &st.cards[usize::from(uid)];
                    let mut locked = false;
                    for r in view::rules_of(&dv, uid, RuleKind::CantBeSetActive) {
                        locked |= view::side_allows(rule_mod(&reg, r)?, c.owner, NO_ARG);
                    }
                    if c.rested && !locked && !has_rule(&dv, uid, RuleKind::StaysRestedInStartPhase)
                    {
                        st.cards[usize::from(uid)].rested = false;
                    }
                }
            }
            st.touch();
            st.step = Step::StartStep;
        }
        Step::StartStep => {
            crate::core::next_group(st);
            crate::core::emit(st, Ev::TurnStart, NO_ARG, i32::from(p), None, &[])?;
            st.step = Step::StartStepDone;
        }
        Step::StartStepDone => {
            st.phase = Phase::Draw;
            st.step = Step::DrawStep;
        }
        Step::DrawStep => {
            crate::interp::draw(st, p, 1, false)?;
            st.step = Step::DrawStepDone;
        }
        Step::DrawStepDone => {
            st.phase = Phase::Resource;
            st.step = Step::ResourceStep;
        }
        Step::ResourceStep => {
            crate::interp::place_resource(st, p, false, i32::from(p))?;
            st.step = Step::ResourceStepDone;
        }
        Step::ResourceStepDone => {
            st.phase = Phase::Main;
            st.step = Step::Main;
        }
        Step::Main => {
            let opts = main_options(st)?;
            decide(st, p, DecisionKind::Main, &opts, EnginePrompt::MainPhase);
        }
        Step::AttackEndCheck => {
            let b = current_battle(st)?;
            st.step = if crate::battle::battle_broken(st, &b)? {
                Step::BattleEnd
            } else {
                Step::Block
            };
        }
        Step::Block => {
            let b = current_battle(st)?;
            let blockers = crate::battle::eligible_blockers(st, &b)?;
            if blockers.is_empty() {
                st.step = Step::BlockEndCheck;
            } else {
                let mut opts: Vec<Action> = blockers
                    .iter()
                    .map(|&u| Action::new(A::Block, i32::from(u), NO_ARG, NO_ARG))
                    .collect();
                opts.push(Action::of(A::NoBlock));
                decide(
                    st,
                    b.defender,
                    DecisionKind::Block,
                    &opts,
                    EnginePrompt::Block,
                );
            }
        }
        Step::BlockEndCheck => {
            let b = current_battle(st)?;
            if crate::battle::battle_broken(st, &b)? {
                st.step = Step::BattleEnd;
            } else {
                st.step = Step::BattleAction;
                st.priority = st.standby();
                st.passes = 0;
            }
        }
        Step::BattleAction | Step::EndAction => action_step(st)?,
        Step::BattleActionEndCheck => {
            let b = current_battle(st)?;
            st.step = if crate::battle::battle_broken(st, &b)? {
                Step::BattleEnd
            } else {
                Step::Damage
            };
        }
        Step::Damage => {
            st.step = Step::DamageDone;
            crate::battle::damage_step(st)?;
        }
        Step::DamageDone => st.step = Step::BattleEnd,
        Step::BattleEnd => {
            st.step = Step::BattleEndDone;
            crate::battle::end_battle(st)?;
        }
        Step::BattleEndDone => {
            if st.battles.pop().is_none() {
                return Err(EngineError::Index("pop from empty list".to_owned()));
            }
            st.touch();
            st.step = Step::Main;
        }
        Step::EndStep => {
            crate::core::next_group(st);
            crate::core::emit(st, Ev::TurnEnd, NO_ARG, i32::from(p), None, &[])?;
            st.step = Step::EndStepDone;
        }
        Step::EndStepDone => st.step = Step::HandStep,
        Step::HandStep => {
            if st.zone(p, Zone::Hand).len() > HAND_LIMIT {
                let program = crate::interp::system_program(st, SystemProgram::HandLimit);
                let f = PushFrame {
                    controller: p,
                    host: NO_ARG,
                    card_uid: NO_ARG,
                    kind: FrameKind::System,
                    event: EventPayload::default(),
                    vars: &[],
                };
                crate::interp::push_frame(st, program, f)?;
            }
            st.step = Step::HandStepDone;
        }
        Step::HandStepDone => st.step = Step::CleanupStep,
        Step::CleanupStep => {
            expire(st);
            st.step = Step::CleanupStepDone;
        }
        Step::CleanupStepDone => st.step = Step::TurnEnd,
        Step::TurnEnd => next_turn(st),
        _ => {
            return Err(EngineError::Internal(format!(
                "unexpected step {}",
                s.value()
            )));
        }
    }
    Ok(())
}

/// `_next_turn(st)` (`engine/game.py:325-339`, rule 7-6-7): the turn cap ends the game without
/// a touch; otherwise the turn passes, once keys and history reset, and the state touches.
fn next_turn(st: &mut GameState) {
    if st.turn >= st.turn_limit() {
        st.winner = Some(-1);
        st.end_reason = Some(EndReason::TurnLimit);
        st.phase = Phase::GameOver;
        st.step = Step::GameOver;
        return;
    }
    st.turn += 1;
    st.active = 1 - st.active;
    st.once_clear();
    st.history_reset();
    st.phase = Phase::Start;
    st.step = Step::ActiveStep;
    st.touch();
}

/// `_action_step(st)` (`engine/game.py:342-353`, rules 9-2..9-5): a player without options
/// passes silently (no decision, no action count, no touch).
fn action_step(st: &mut GameState) -> Result<(), EngineError> {
    if st.passes >= 2 {
        st.step = if st.step == Step::BattleAction {
            Step::BattleActionEndCheck
        } else {
            Step::EndStep
        };
        return Ok(());
    }
    let p = st.priority;
    let mut opts = action_step_options(st, p)?;
    if opts.is_empty() {
        st.passes += 1;
        st.priority = 1 - p;
        return Ok(());
    }
    opts.push(Action::of(A::Pass));
    decide(
        st,
        p,
        DecisionKind::ActionStep,
        &opts,
        EnginePrompt::ActionStep,
    );
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// legal options (engine/game.py:360-660)

/// `_resources(st, p)` (`engine/game.py:360-369`): `(normal, ex)` active Resources.
fn resources(st: &GameState, p: u8) -> (i64, i64) {
    let (mut normal, mut ex) = (0, 0);
    for &uid in st.zone(p, Zone::ResourceArea) {
        if st.card(uid).rested {
            continue;
        }
        if crate::core::card_type(st, uid) == CardType::ExResource {
            ex += 1;
        } else {
            normal += 1;
        }
    }
    (normal, ex)
}

/// The result of [`payment_choices`]: the counts `lo..=hi` in ascending order, none when
/// `lo > hi` (always a contiguous run, so it needs no list).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Pays {
    lo: i32,
    hi: i32,
}

impl Pays {
    const NONE: Pays = Pays { lo: 1, hi: 0 };

    /// No count pays.
    pub(crate) fn is_empty(self) -> bool {
        self.lo > self.hi
    }

    /// The counts, ascending.
    pub(crate) fn iter(self) -> std::ops::RangeInclusive<i32> {
        self.lo..=self.hi
    }
}

impl<const N: usize> PartialEq<[i32; N]> for Pays {
    fn eq(&self, other: &[i32; N]) -> bool {
        self.iter().eq(other.iter().copied())
    }
}

/// `payment_choices(st, p, cost, resources=None)` (`engine/game.py:372-382`): the numbers of
/// EX Resources that can pay `cost`, ascending.
pub(crate) fn payment_choices(st: &GameState, p: u8, cost: i64, known: Option<(i64, i64)>) -> Pays {
    if cost <= 0 {
        return Pays { lo: 0, hi: 0 };
    }
    let (normal, ex) = known.unwrap_or_else(|| resources(st, p));
    let lo = (cost - normal).max(0);
    let hi = cost.min(ex);
    // lo and hi are bounded by the resource count (at most a few hundred cards); a count that
    // is no i32 is dropped, as the list form dropped it.
    match (
        i32::try_from(lo),
        i32::try_from(hi.min(i64::from(i32::MAX))),
    ) {
        (Ok(lo), Ok(hi)) => Pays { lo, hi },
        _ => Pays::NONE,
    }
}

/// `_level_ok(st, dv, p, uid)` (`engine/game.py:385-386`, rule 2-9-1).
fn level_ok(st: &GameState, dv: &Derived, p: u8, uid: Uid) -> bool {
    st.zone(p, Zone::ResourceArea).len() as i64 >= i64::from(view::play_level(st, dv, uid))
}

/// `_required_targets_ok(st, program_id, ctx)` (`engine/game.py:389-392`).
fn required_targets_ok(st: &GameState, program: ProgramId, c: &Ctx) -> Result<bool, EngineError> {
    crate::interp::targets_available(st, program, 0, c)
}

/// `_command_playable(st, p, uid, timing)` (`engine/game.py:395-407`).
pub(crate) fn command_playable(
    st: &GameState,
    p: u8,
    uid: Uid,
    timing: Timing,
) -> Result<bool, EngineError> {
    let reg = st.registry();
    let entry = reg.card(st.card(uid).def_id);
    let Some(aid) = entry.command_aid else {
        return Ok(false);
    };
    let a = reg.ability(aid);
    let Node::Command(ab) = reg.node(a.ability.node()) else {
        return Err(EngineError::Assertion(String::new()));
    };
    if ab.timing != Timing::MainOrAction && ab.timing != timing {
        return Ok(false);
    }
    let c = ctx(p, i32::from(uid), NO_ARG);
    if !is_true(reg, ab.cond) && !view::cond(st, &*view::derived(st)?, &c, ab.cond, true)? {
        return Ok(false);
    }
    required_targets_ok(st, program_of(a)?, &c)
}

/// `_activated_list(st, p, timing)` (`engine/game.py:410-433`): `(host, aid or SUPPORT_AID,
/// resource cost)` per activatable ability, by zone (BATTLE, BASE, HAND, TRASH, PAIRED), list
/// order and derived ability order; a <Support> entry follows its host's own abilities.
pub(crate) fn activated_list(
    st: &GameState,
    p: u8,
    timing: Timing,
) -> Result<Vec<(Uid, i32, i32)>, EngineError> {
    let dv = view::derived(st)?;
    let reg = st.registry();
    let mut out = Vec::new();
    for z in [
        Zone::Battle,
        Zone::Base,
        Zone::Hand,
        Zone::Trash,
        Zone::Paired,
    ] {
        for &host in st.zone(p, z) {
            for &aid in view::abilities_of(&dv, host) {
                let a = reg.ability(aid);
                let Some(ab) = activated_of(reg, a) else {
                    continue;
                };
                if ab.timing != Timing::MainOrAction && ab.timing != timing {
                    continue;
                }
                let cost = activation_ok(st, &dv, p, host, a, ab)?;
                if cost >= 0 {
                    out.push((host, i32::from(a.aid.0), cost));
                }
            }
            if timing == Timing::Main
                && z == Zone::Battle
                && view::kw_amount(&dv, host, Kw::Support) > 0
                && !st.card(host).rested
                && st.zone(p, Zone::Battle).iter().any(|&u| u != host)
            {
                out.push((host, SUPPORT_AID, 0));
            }
        }
    }
    Ok(out)
}

/// The candidates of a RestCards/DestroyCards/DiscardCards/ExileCards cost: `V.select(st, dv,
/// ctx, sel)`, with `IsRested(False)` appended to the filters for RestCards
/// (`engine/game.py:453-457`, `:597-601`). `all()` evaluates that filter last, so it is the
/// selection restricted to active cards.
fn cost_candidates(
    st: &GameState,
    dv: &Derived,
    c: &Ctx,
    sel: SelId,
    active_only: bool,
) -> Result<crate::UidBuf, EngineError> {
    let mut cands = view::select(st, dv, c, sel)?;
    if active_only {
        cands.retain(|&u| !st.card(u).rested);
    }
    Ok(cands)
}

/// A selection cost (`d.RestCards`, `d.DestroyCards`, `d.DiscardCards`, `d.ExileCards`): its
/// `sel`, `count` and whether it rests (the `IsRested(False)` filter).
fn selection_cost(reg: &Registry, c: CostId) -> Option<(SelId, i64, bool)> {
    match reg.node(c.node()) {
        Node::RestCards(x) => Some((x.sel, x.count, true)),
        Node::DestroyCards(x) => Some((x.sel, x.count, false)),
        Node::DiscardCards(x) => Some((x.sel, x.count, false)),
        Node::ExileCards(x) => Some((x.sel, x.count, false)),
        _ => None,
    }
}

/// `_activation_ok(st, dv, p, host, a)` (`engine/game.py:436-472`): the resource cost, or -1.
fn activation_ok(
    st: &GameState,
    dv: &Derived,
    p: u8,
    host: Uid,
    a: &AbilityEntry,
    ab: &Activated,
) -> Result<i32, EngineError> {
    let reg = st.registry();
    if ab.once_per_turn {
        let key = activation_once_key(a.aid, host, st.card(host).zone_seq)?;
        if st.once_contains(&key) {
            return Ok(-1);
        }
    }
    let card_uid = crate::core::card_uid_for(st, a, host);
    let c = ctx(p, i32::from(host), card_uid);
    if !is_true(reg, ab.cond) && !view::cond(st, dv, &c, ab.cond, true)? {
        return Ok(-1);
    }
    let mut res: i64 = 0;
    for &cost in ab.costs.iter() {
        match reg.node(cost.node()) {
            Node::RestSelf(_) => {
                let hc = st.card(host);
                if hc.rested || !matches!(hc.zone, Zone::Battle | Zone::Base) {
                    return Ok(-1);
                }
            }
            Node::PayResources(x) => res += x.amount,
            Node::ReturnSelf(_) | Node::DestroySelf(_) | Node::TrashSelf(_) => {
                if !matches!(
                    st.card(host).zone,
                    Zone::Battle | Zone::Base | Zone::Paired | Zone::Hand | Zone::Trash
                ) {
                    return Ok(-1);
                }
            }
            _ => {
                if let Some((sel, count, rests)) = selection_cost(reg, cost) {
                    let n = cost_candidates(st, dv, &c, sel, rests)?.len();
                    if (n as i64) < count {
                        return Ok(-1);
                    }
                }
            }
        }
    }
    if res != 0 && payment_choices(st, p, res, None).is_empty() {
        return Ok(-1);
    }
    if !required_targets_ok(st, program_of(a)?, &c)? {
        return Ok(-1);
    }
    to_i32(res, "activation cost")
}

/// The `RuleMod` of a rule instance (`r.rule`).
fn rule_mod<'r>(
    reg: &'r Registry,
    r: &crate::RuleInst,
) -> Result<&'r gcg_effects::RuleMod, EngineError> {
    match reg.node(r.rule.node()) {
        Node::RuleMod(x) => Ok(x),
        _ => Err(EngineError::Internal(
            "a rule instance is not a RuleMod".to_owned(),
        )),
    }
}

/// `_attack_options(st, p)` (`engine/game.py:475-516`; rules 3-2-4, 3-2-6-3, 7-5-4-1, 8-2-1).
fn attack_options(st: &GameState, p: u8) -> Result<Vec<Action>, EngineError> {
    let dv = view::derived(st)?;
    let reg = st.registry();
    let mut out = Vec::new();
    let enemy = 1 - p;
    let enemy_units = st.zone(enemy, Zone::Battle);
    let attractors: Vec<Uid> = enemy_units
        .iter()
        .copied()
        .filter(|&t| has_rule(&dv, t, RuleKind::ForceAttackTarget))
        .collect();
    for &uid in st.zone(p, Zone::Battle) {
        let c = st.card(uid);
        if c.rested || has_rule(&dv, uid, RuleKind::CantAttack) {
            continue;
        }
        let mut deploy_limits: Option<&[FilterId]> = None;
        if c.entered_turn == st.turn && !view::is_linked(&dv, uid) {
            let perms: Vec<&crate::RuleInst> =
                view::rules_of(&dv, uid, RuleKind::AttackOnDeployTurn).collect();
            if perms.is_empty() {
                continue;
            }
            let mut all_units_only = true;
            for r in &perms {
                if reg.str(rule_mod(reg, r)?.name) != "units_only" {
                    all_units_only = false;
                    break;
                }
            }
            if all_units_only {
                deploy_limits = Some(&*rule_mod(reg, perms[0])?.source_filters);
            }
        }
        let mut unit_targets: Vec<Uid> = Vec::new();
        if !has_rule(&dv, uid, RuleKind::CantAttackUnits) {
            let active_perms: Vec<&crate::RuleInst> =
                view::rules_of(&dv, uid, RuleKind::MayAttackActive).collect();
            let attacker_ctx = ctx(p, i32::from(uid), NO_ARG);
            for &t in enemy_units {
                if has_rule(&dv, t, RuleKind::CantBeAttacked) {
                    continue;
                }
                let mut ok = st.card(t).rested;
                if !ok {
                    for r in &active_perms {
                        let filters = &rule_mod(reg, r)?.source_filters;
                        if filters.is_empty() || view::matches(st, &dv, &attacker_ctx, t, filters)?
                        {
                            ok = true;
                            break;
                        }
                    }
                }
                if let Some(limits) = deploy_limits
                    && ok
                    && !limits.is_empty()
                {
                    ok = view::matches(st, &dv, &attacker_ctx, t, limits)?;
                }
                if ok {
                    unit_targets.push(t);
                }
            }
        }
        let forced = forced_targets(st, &dv, uid, &attractors, &unit_targets)?;
        if !forced.is_empty() {
            out.extend(
                forced
                    .iter()
                    .map(|&t| Action::new(A::Attack, i32::from(uid), i32::from(t), NO_ARG)),
            );
            continue;
        }
        if deploy_limits.is_none() && !has_rule(&dv, uid, RuleKind::CantAttackPlayer) {
            out.push(Action::new(
                A::Attack,
                i32::from(uid),
                PLAYER_TARGET,
                NO_ARG,
            ));
        }
        out.extend(
            unit_targets
                .iter()
                .map(|&t| Action::new(A::Attack, i32::from(uid), i32::from(t), NO_ARG)),
        );
    }
    Ok(out)
}

/// `_forced_targets(st, dv, attacker, attractors, legal)` (`engine/game.py:519-540`): "must"
/// targets outrank rested "if possible" ones (Q289); a forced Unit that is not a legal target
/// imposes nothing (Q300, Q301).
fn forced_targets(
    st: &GameState,
    dv: &Derived,
    attacker: Uid,
    attractors: &[Uid],
    legal: &[Uid],
) -> Result<Vec<Uid>, EngineError> {
    let reg = st.registry();
    let mut must = Vec::new();
    let mut if_possible: Vec<Uid> = Vec::new();
    for &t in attractors {
        if !legal.contains(&t) {
            continue;
        }
        for r in view::rules_of(dv, t, RuleKind::ForceAttackTarget) {
            let rule = rule_mod(reg, r)?;
            let owner = st.card(t).owner;
            if !rule.source_filters.is_empty()
                && !view::matches(
                    st,
                    dv,
                    &ctx(owner, i32::from(t), NO_ARG),
                    attacker,
                    &rule.source_filters,
                )?
            {
                continue;
            }
            if reg.str(rule.name) == "must" {
                must.push(t);
                break;
            }
            if st.card(t).rested && !if_possible.contains(&t) {
                if_possible.push(t);
            }
        }
    }
    Ok(if must.is_empty() { if_possible } else { must })
}

/// `main_options(st)` (`engine/game.py:543-583`, rule 7-5-1): per hand card its plays (each
/// card's play-modifier alternatives last), then activations, attacks, and END_MAIN.
pub(crate) fn main_options(st: &GameState) -> Result<Vec<Action>, EngineError> {
    let p = st.active;
    let dv = view::derived(st)?;
    let db = st.db();
    let mut out = Vec::new();
    let my_units: Vec<Uid> = st
        .zone(p, Zone::Battle)
        .iter()
        .copied()
        .filter(|&u| crate::interp::can_pair(st, &dv, u))
        .collect();
    let res = resources(st, p);
    for &uid in st.zone(p, Zone::Hand) {
        let cd = db.by_id(st.card(uid).def_id);
        let alternatives = alt_play_options(st, &dv, p, uid, &my_units)?;
        let pays = if level_ok(st, &dv, p, uid) {
            let cost = view::play_cost(st, &dv, uid)?;
            payment_choices(st, p, i64::from(cost), Some(res))
        } else {
            Pays::NONE
        };
        if pays.is_empty() {
            out.extend(alternatives);
            continue;
        }
        let u = i32::from(uid);
        match cd.card_type {
            CardType::Unit => {
                out.extend(pays.iter().map(|c| Action::new(A::PlayUnit, u, NO_ARG, c)))
            }
            CardType::Base => {
                out.extend(pays.iter().map(|c| Action::new(A::PlayBase, u, NO_ARG, c)))
            }
            CardType::Pilot => {
                for &m in &my_units {
                    out.extend(
                        pays.iter()
                            .map(|c| Action::new(A::Pair, u, i32::from(m), c)),
                    );
                }
            }
            CardType::Command => {
                if command_playable(st, p, uid, Timing::Main)? {
                    out.extend(
                        pays.iter()
                            .map(|c| Action::new(A::PlayCommand, u, NO_ARG, c)),
                    );
                }
                if cd.pilot_name.is_some() {
                    for &m in &my_units {
                        out.extend(
                            pays.iter()
                                .map(|c| Action::new(A::Pair, u, i32::from(m), c)),
                        );
                    }
                }
            }
            _ => {}
        }
        out.extend(alternatives);
    }
    for (host, aid, cost) in activated_list(st, p, Timing::Main)? {
        out.extend(
            payment_choices(st, p, i64::from(cost), Some(res))
                .iter()
                .map(|c| Action::new(A::Activate, i32::from(host), aid, c)),
        );
    }
    out.extend(attack_options(st, p)?);
    out.push(Action::of(A::EndMain));
    Ok(out)
}

/// `play_modifiers(st, uid)` (`engine/game.py:586-589`): the card's own PlayModifier abilities in
/// `own` order.
fn play_modifiers(st: &GameState, uid: Uid) -> Vec<&AbilityEntry> {
    let reg = st.registry();
    reg.card(st.card(uid).def_id)
        .own
        .iter()
        .map(|&a| reg.ability(a))
        .filter(|a| play_modifier_of(reg, a).is_some())
        .collect()
}

/// `_costs_payable(st, dv, p, host, costs)` (`engine/game.py:592-604`): the selection costs have
/// enough candidates other than the host.
fn costs_payable(
    st: &GameState,
    dv: &Derived,
    p: u8,
    host: Uid,
    costs: &[CostId],
) -> Result<bool, EngineError> {
    let reg = st.registry();
    let c = ctx(p, i32::from(host), NO_ARG);
    for &cost in costs {
        if let Some((sel, count, rests)) = selection_cost(reg, cost) {
            let n = cost_candidates(st, dv, &c, sel, rests)?
                .iter()
                .filter(|&&u| u != host)
                .count();
            if (n as i64) < count {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

/// `_alt_play_options(st, dv, p, uid, my_units)` (`engine/game.py:607-644`): each play modifier
/// offers the card at its Lv. and cost, encoded `c + ALT_PLAY_BASE * (i + 1)` where `i` counts
/// every modifier of the card.
fn alt_play_options(
    st: &GameState,
    dv: &Derived,
    p: u8,
    uid: Uid,
    my_units: &[Uid],
) -> Result<Vec<Action>, EngineError> {
    let mods = play_modifiers(st, uid);
    if mods.is_empty() {
        return Ok(Vec::new());
    }
    let reg = st.registry();
    let cd = st.def_of(uid);
    let u = i32::from(uid);
    let mut out = Vec::new();
    for (i, entry) in mods.iter().enumerate() {
        let Some(pm) = play_modifier_of(reg, entry) else {
            return Err(EngineError::Assertion(String::new()));
        };
        let c = ctx(p, u, NO_ARG);
        if !is_true(reg, pm.cond) && !view::cond(st, dv, &c, pm.cond, true)? {
            continue;
        }
        if !costs_payable(st, dv, p, uid, &pm.costs)? {
            continue;
        }
        let level = match pm.level {
            Some(l) => l,
            None => i64::from(view::play_level(st, dv, uid)),
        };
        if (st.zone(p, Zone::ResourceArea).len() as i64) < level {
            continue;
        }
        let cost = match pm.cost {
            Some(x) => x,
            None => i64::from(view::play_cost(st, dv, uid)?),
        };
        let base = ALT_PLAY_BASE * to_i32(i as i64 + 1, "play modifier index")?;
        let pays = payment_choices(st, p, cost, None);
        let enc = || pays.iter().map(move |x| x + base);
        match cd.card_type {
            CardType::Unit => out.extend(enc().map(|e| Action::new(A::PlayUnit, u, NO_ARG, e))),
            CardType::Base => out.extend(enc().map(|e| Action::new(A::PlayBase, u, NO_ARG, e))),
            CardType::Command if command_playable(st, p, uid, Timing::Main)? => {
                out.extend(enc().map(|e| Action::new(A::PlayCommand, u, NO_ARG, e)))
            }
            _ => {}
        }
        if cd.is_pilot_capable() {
            for &m in my_units {
                if !pm.pair_filters.is_empty() && !view::matches(st, dv, &c, m, &pm.pair_filters)? {
                    continue;
                }
                out.extend(enc().map(|e| Action::new(A::Pair, u, i32::from(m), e)));
            }
        }
    }
    Ok(out)
}

/// `action_step_options(st, p)` (`engine/game.py:647-660`, rule 9-3): 【Action】 Commands, then
/// 【Activate･Action】 abilities; no pairing (13-2-4-2).
pub(crate) fn action_step_options(st: &GameState, p: u8) -> Result<Vec<Action>, EngineError> {
    let dv = view::derived(st)?;
    let db = st.db();
    let mut out = Vec::new();
    for &uid in st.zone(p, Zone::Hand) {
        let cd = db.by_id(st.card(uid).def_id);
        if cd.card_type != CardType::Command || !level_ok(st, &dv, p, uid) {
            continue;
        }
        let pays = payment_choices(st, p, i64::from(view::play_cost(st, &dv, uid)?), None);
        if !pays.is_empty() && command_playable(st, p, uid, Timing::Action)? {
            let u = i32::from(uid);
            out.extend(
                pays.iter()
                    .map(|c| Action::new(A::PlayCommand, u, NO_ARG, c)),
            );
        }
    }
    for (host, aid, cost) in activated_list(st, p, Timing::Action)? {
        out.extend(
            payment_choices(st, p, i64::from(cost), None)
                .iter()
                .map(|c| Action::new(A::Activate, i32::from(host), aid, c)),
        );
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------
// triggers (engine/game.py:667-727)

/// `_trigger_candidates(st, batch)` (`engine/game.py:673-681`, rules 10-1-6-5, 10-1-6-6,
/// 10-1-6-8): Bursts first, then the active player's, then all.
fn trigger_candidates(active: u8, batch: &[TriggerInst]) -> Vec<usize> {
    let bursts: Vec<usize> = (0..batch.len()).filter(|&i| batch[i].burst).collect();
    if !bursts.is_empty() {
        return bursts;
    }
    let act: Vec<usize> = (0..batch.len())
        .filter(|&i| batch[i].controller == active)
        .collect();
    if !act.is_empty() {
        return act;
    }
    (0..batch.len()).collect()
}

/// `st.batches[-1]`, or `None` without batches.
fn top_batch(st: &GameState) -> Option<&[TriggerInst]> {
    let &start = st.batch_starts.last()?;
    Some(&st.triggers[start as usize..st.pending_start as usize])
}

/// `_start_next_trigger(st)` (`engine/game.py:684-709`).
fn start_next_trigger(st: &mut GameState) -> Result<(), EngineError> {
    let Some(batch) = top_batch(st) else {
        return Err(EngineError::Index("list index out of range".to_owned()));
    };
    if batch.is_empty() {
        st.trigger_arena().pop_batch();
        return Ok(());
    }
    let mut cands = trigger_candidates(st.active, batch);
    if cands.len() > 1 {
        let first = &batch[cands[0]];
        if cands.iter().all(|&i| {
            let t = &batch[i];
            t.program == first.program
                && t.ability_key == first.ability_key
                && t.burst == first.burst
                && t.host == first.host
        }) {
            cands.truncate(1);
        }
    }
    if cands.len() == 1 {
        let t = st.trigger_arena().take_from_top(cands[0])?;
        return resolve_trigger(st, &t);
    }
    let player = batch[cands[0]].controller;
    let mut opts = Vec::with_capacity(cands.len());
    for i in cands {
        opts.push(Action::new(
            A::Order,
            to_i32(i as i64, "trigger index")?,
            NO_ARG,
            NO_ARG,
        ));
    }
    decide(
        st,
        player,
        DecisionKind::OrderTrigger,
        &opts,
        EnginePrompt::OrderTriggers,
    );
    Ok(())
}

/// `_resolve_trigger(st, t)` (`engine/game.py:712-727`): a trigger whose once key is used is
/// dropped; a Burst records `burst_revealed`; the frame takes the trigger's once key.
fn resolve_trigger(st: &mut GameState, t: &TriggerInst) -> Result<(), EngineError> {
    if let Some(k) = &t.once_key
        && !k.0.is_empty()
        && st.once_contains(k)
    {
        return Ok(());
    }
    #[cfg(feature = "paranoid")]
    crate::paranoid::trigger_resolves(st, t.host, st.trigger_seqs.taken);
    let kind = if t.burst {
        FrameKind::Burst
    } else {
        FrameKind::Trigger
    };
    if t.burst {
        let c = t.controller;
        crate::core::record(st, HistKind::BurstRevealed, c, c as i8, t.card_uid, NO_ARG);
    }
    let f = PushFrame {
        controller: t.controller,
        host: t.host,
        card_uid: t.card_uid,
        kind,
        event: t.event,
        vars: &[],
    };
    let i = crate::interp::push_frame(st, t.program, f)?;
    st.frame_mut(i).once_key = t.once_key;
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// main loop and the public API (engine/game.py:734-965)

/// `advance(st)` (`engine/game.py:734-760`): frames, then queued triggers, then trigger
/// batches, then the procedure (setup, or rules management and the turn step), until a
/// decision is pending or the game is over. The decision cap is checked first in every
/// iteration (strict `>`). W-47: [`EngineError::NoProgress`] after [`NO_PROGRESS_STEPS`]
/// iterations without a decision, where Python would loop forever.
pub(crate) fn advance(st: &mut GameState) -> Result<(), EngineError> {
    let mut steps: u64 = 0;
    while st.pending.is_none() && st.winner.is_none() {
        if steps == NO_PROGRESS_STEPS {
            return Err(EngineError::NoProgress { steps });
        }
        steps += 1;
        crate::diag::assert_live_step(st.step);
        if st.action_count > st.max_actions() {
            st.winner = Some(-1);
            st.end_reason = Some(EndReason::TurnLimit);
            st.phase = Phase::GameOver;
            st.step = Step::GameOver;
            return Ok(());
        }
        if !st.frames.is_empty() {
            crate::interp::run_top_frame(st)?;
            continue;
        }
        if n_pending_triggers(st) > 0 {
            st.trigger_arena().form_batch()?;
            continue;
        }
        if !st.batch_starts.is_empty() {
            start_next_trigger(st)?;
            continue;
        }
        if st.phase == Phase::Setup {
            setup_step(st)?;
        } else {
            crate::core::rules_management(st)?;
            if crate::core::over(st) {
                return Ok(());
            }
            if !st.frames.is_empty() || n_pending_triggers(st) > 0 {
                continue;
            }
            turn_step(st)?;
        }
    }
    Ok(())
}

/// `concede(st, player)` (`engine/game.py:763-771`, rules 1-2-4, 1-2-5): nothing once the game
/// is over; otherwise frames, batches and pending triggers are dropped and `player` loses.
pub fn concede(st: &mut GameState, player: u8) {
    if st.winner.is_some() {
        return;
    }
    st.frames.clear();
    st.frame_vars.clear();
    st.frame_ints.clear();
    st.frame_items.clear();
    st.trigger_arena().clear();
    crate::core::set_winner(st, &[player], EndReason::Concede);
}

/// `legal_actions(st)` (`engine/game.py:774-777`): the pending decision's options, or none.
#[must_use]
pub fn legal_actions(st: &GameState) -> &[Action] {
    if st.pending.is_none() {
        return &[];
    }
    &st.pending_options
}

/// `apply(st, action, *, check=True)` (`engine/game.py:780-818`): applies `action` for the
/// deciding player and advances to the next decision, in place. A waiting frame takes the
/// action before the decision kind is looked at.
///
/// # Errors
/// [`ApplyError::Illegal`] without a decision, or with `check` for an action the decision does
/// not offer (nothing changes then); [`ApplyError::Engine`] for anything the dispatch or
/// `advance` raises.
pub fn apply(st: &mut GameState, action: Action, check: bool) -> Result<(), ApplyError> {
    let Some(dec) = st.pending else {
        return Err(IllegalActionError::NoDecision.into());
    };
    if check && !st.pending_options.contains(&action) {
        return Err(IllegalActionError::Illegal {
            action,
            kind: dec.kind,
        }
        .into());
    }
    st.clear_pending();
    st.action_count += 1;
    let waiting = st.frames.last().is_some_and(|f| f.waiting);
    if waiting {
        crate::interp::resume(st, action)?;
    } else {
        match dec.kind {
            DecisionKind::ChooseFirst => {
                let p = player_at(action.a)?;
                st.first_player = p;
                st.active = p;
                st.step = Step::SetupDraw;
            }
            DecisionKind::Redraw => {
                if action.kind == A::Redraw {
                    redraw(st, dec.player)?;
                }
                st.step = if st.step == Step::SetupRedrawP1 {
                    Step::SetupRedrawP2
                } else {
                    Step::SetupShields
                };
            }
            DecisionKind::Main => apply_main(st, action)?,
            DecisionKind::Block => {
                if action.kind == A::Block {
                    let blocker = uid_at(st, action.a)?;
                    crate::battle::block(st, blocker)?;
                }
                st.step = Step::BlockEndCheck;
            }
            DecisionKind::ActionStep => {
                if action.kind == A::Pass {
                    st.passes += 1;
                } else {
                    st.passes = 0;
                    play_or_activate(st, dec.player, action)?;
                }
                st.priority = 1 - dec.player;
            }
            DecisionKind::OrderTrigger => {
                let i = top_batch_index(st, action.a)?;
                let t = st.trigger_arena().take_from_top(i)?;
                resolve_trigger(st, &t)?;
            }
            k => {
                return Err(
                    EngineError::Internal(format!("unhandled decision {}", k.as_str())).into(),
                );
            }
        }
    }
    advance(st)?;
    Ok(())
}

/// The index `batch.pop(i)` removes from the top batch (`engine/game.py:814-815`), with
/// Python's negative indexing from the end.
fn top_batch_index(st: &GameState, i: i32) -> Result<usize, EngineError> {
    let len = top_batch(st).map_or(0, <[TriggerInst]>::len);
    let at = if i < 0 {
        len.checked_sub(i.unsigned_abs() as usize)
    } else {
        usize::try_from(i).ok()
    };
    at.filter(|&k| k < len)
        .ok_or_else(|| EngineError::Index("pop index out of range".to_owned()))
}

/// `_apply_main(st, action)` (`engine/game.py:821-831`).
fn apply_main(st: &mut GameState, action: Action) -> Result<(), EngineError> {
    let p = st.active;
    match action.kind {
        A::EndMain => {
            st.phase = Phase::End;
            st.step = Step::EndAction;
            st.priority = st.standby();
            st.passes = 0;
            Ok(())
        }
        A::Attack => {
            let attacker = uid_at(st, action.a)?;
            crate::battle::declare_attack(st, attacker, action.b)
        }
        _ => play_or_activate(st, p, action),
    }
}

/// `_pay(st, p, uid, ex_used, cost)` (`engine/game.py:834-837`).
fn pay(st: &mut GameState, p: u8, uid: Uid, ex_used: i32, cost: i64) -> Result<(), EngineError> {
    if !crate::interp::pay_generic(st, p, cost, ex_used)? {
        return Err(EngineError::Internal(
            "payment failed for a legal action".to_owned(),
        ));
    }
    consume_player_cost_mods(st, uid)
}

/// `_consume_player_cost_mods(st, uid)` (`engine/game.py:840-871`): every player-level CostMod
/// of the card's owner with uses left that matches the card loses one use, in place; one that
/// reaches 0 is dropped. The filter sees the post-payment state, and the view is asked for
/// where Python asks for it.
fn consume_player_cost_mods(st: &mut GameState, uid: Uid) -> Result<(), EngineError> {
    if st.lasting.is_empty() {
        return Ok(());
    }
    let reg = Arc::clone(st.registry());
    let owner = st.card(uid).owner;
    let mut changed = false;
    let mut dropped = Vec::with_capacity(st.lasting.len());
    for i in 0..st.lasting.len() {
        let le = st.lasting[i];
        let mut applies = i16::from(le.player) == i16::from(owner)
            && le.uses > 0
            && matches!(reg.cont_node(le.effect), ContView::CostMod { .. });
        if applies && let Some(f) = le.filters {
            let dv = view::derived(st)?;
            let c = ctx(le.controller, le.source_uid, NO_ARG);
            applies = view::matches(st, &dv, &c, uid, &reg.filter_set(f).filters)?;
        }
        let mut gone = false;
        if applies {
            st.lasting[i].uses -= 1;
            changed = true;
            gone = st.lasting[i].uses == 0;
        }
        dropped.push(gone);
    }
    if changed {
        let mut k = 0;
        st.lasting_arena().retain_lasting(|_, _| {
            let keep = !dropped[k];
            k += 1;
            keep
        });
        st.touch();
    }
    Ok(())
}

/// `_play_or_activate(st, p, action)` (`engine/game.py:874-954`; rules 7-5-2-2, 9-3-1,
/// 10-3-1). The view is taken at entry, before the reveal and the payment.
fn play_or_activate(st: &mut GameState, p: u8, action: Action) -> Result<(), EngineError> {
    let reg = Arc::clone(st.registry());
    let dv = view::derived(st)?;
    let k = action.kind;
    if k == A::Activate {
        let host = uid_at(st, action.a)?;
        let (aid, ex) = (action.b, action.c);
        if aid == SUPPORT_AID {
            crate::core::record(st, HistKind::Activate, p, p as i8, i32::from(host), NO_ARG);
            let f = PushFrame {
                controller: p,
                host: i32::from(host),
                card_uid: NO_ARG,
                kind: FrameKind::Activated,
                event: EventPayload::default(),
                vars: &[],
            };
            crate::interp::push_frame(st, reg.keyword_program(KeywordProgram::Support), f)?;
            return Ok(());
        }
        let aid = u16::try_from(aid)
            .map(AbilityId)
            .map_err(|_| EngineError::Index("list index out of range".to_owned()))?;
        let a = reg.ability(aid);
        let ab = expect_activated(&reg, a)?;
        let mut res: i64 = 0;
        for &c in ab.costs.iter() {
            if let Node::PayResources(x) = reg.node(c.node()) {
                res += x.amount;
            }
        }
        if res != 0 && !crate::interp::pay_generic(st, p, res, ex)? {
            return Err(EngineError::Internal(
                "activation payment failed".to_owned(),
            ));
        }
        if ab.once_per_turn {
            let key = activation_once_key(a.aid, host, st.card(host).zone_seq)?;
            st.once_add(key);
        }
        let h = i32::from(host);
        crate::core::record(st, HistKind::Activate, p, p as i8, h, NO_ARG);
        if res != 0 {
            crate::core::record_cost_paid(st, p, p as i8, h, res);
            let amount = to_i32(res, "activation cost")?;
            crate::core::emit(
                st,
                Ev::CostPaid,
                h,
                i32::from(p),
                None,
                &[(EvKey::Amount, amount)],
            )?;
        }
        let f = PushFrame {
            controller: p,
            host: h,
            card_uid: crate::core::card_uid_for(st, a, host),
            kind: FrameKind::Activated,
            event: EventPayload::default(),
            vars: &[],
        };
        crate::interp::push_frame(st, program_of(a)?, f)?;
        return Ok(());
    }
    let uid = uid_at(st, action.a)?;
    let mut alt_cost_program: Option<ProgramId> = None;
    let mut ex = action.c;
    let cost: i64;
    if action.c >= ALT_PLAY_BASE {
        let alt = action.c / ALT_PLAY_BASE - 1;
        ex = action.c % ALT_PLAY_BASE;
        let mods = play_modifiers(st, uid);
        let entry_pm = usize::try_from(alt)
            .ok()
            .and_then(|i| mods.get(i).copied())
            .ok_or_else(|| EngineError::Index("list index out of range".to_owned()))?;
        let Some(pm) = play_modifier_of(&reg, entry_pm) else {
            return Err(EngineError::Assertion(String::new()));
        };
        cost = match pm.cost {
            Some(x) => x,
            None => i64::from(view::play_cost(st, &dv, uid)?),
        };
        alt_cost_program = entry_pm.cost_program;
    } else {
        cost = i64::from(view::play_cost(st, &dv, uid)?);
    }
    drop(dv);
    st.cards[usize::from(uid)].known = BOTH_KNOW;
    pay(st, p, uid, ex, cost)?;
    let u = i32::from(uid);
    crate::core::record(st, HistKind::Play, p, p as i8, u, NO_ARG);
    if ex > 0 {
        crate::core::record(st, HistKind::PlayedWithEx, p, p as i8, u, NO_ARG);
    }
    let vars = *st.engine_vars();
    match k {
        A::PlayUnit | A::PlayBase => {
            let card = [u];
            let f = PushFrame {
                controller: p,
                host: u,
                card_uid: NO_ARG,
                kind: FrameKind::System,
                event: EventPayload::default(),
                vars: &[(vars.card, &card[..])],
            };
            let program = crate::interp::system_program(st, SystemProgram::Deploy);
            let i = crate::interp::push_frame(st, program, f)?;
            st.int_set(i, IntKey::ExUsed, ex)?;
        }
        A::Pair => {
            let (pilot, unit) = ([u], [action.b]);
            let f = PushFrame {
                controller: p,
                host: u,
                card_uid: NO_ARG,
                kind: FrameKind::System,
                event: EventPayload::default(),
                vars: &[(vars.pilot, &pilot[..]), (vars.unit, &unit[..])],
            };
            let program = crate::interp::system_program(st, SystemProgram::Pair);
            crate::interp::push_frame(st, program, f)?;
        }
        A::PlayCommand => {
            let command_aid = reg.card(st.card(uid).def_id).command_aid.ok_or_else(|| {
                EngineError::Internal("a played Command has no command ability".to_owned())
            })?;
            let reveal = MoveOpts {
                reveal: true,
                ..MoveOpts::default()
            };
            crate::core::move_card(st, uid, Zone::Resolving, reveal)?;
            crate::core::record(st, HistKind::CommandActivated, p, p as i8, u, NO_ARG);
            crate::core::emit(
                st,
                Ev::CommandPlayed,
                u,
                i32::from(p),
                None,
                &[(EvKey::ExUsed, ex)],
            )?;
            let f = PushFrame {
                controller: p,
                host: u,
                card_uid: NO_ARG,
                kind: FrameKind::Command,
                event: EventPayload::default(),
                vars: &[],
            };
            crate::interp::push_frame(st, program_of(reg.ability(command_aid))?, f)?;
        }
        _ => {
            return Err(EngineError::Internal(format!(
                "unexpected play action {action}"
            )));
        }
    }
    if let Some(program) = alt_cost_program {
        let f = PushFrame {
            controller: p,
            host: u,
            card_uid: NO_ARG,
            kind: FrameKind::System,
            event: EventPayload::default(),
            vars: &[],
        };
        crate::interp::push_frame(st, program, f)?;
    }
    Ok(())
}

/// `step(st, action)` (`engine/game.py:961-965`): a clone with `action` applied (checked); the
/// input is unchanged.
///
/// # Errors
/// As [`apply`].
pub fn step(st: &GameState, action: Action) -> Result<GameState, ApplyError> {
    let mut s = st.clone();
    apply(&mut s, action, true)?;
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AbilityKey;

    fn empty_state() -> GameState {
        let konst = GameConst::new(
            Registry::embedded(),
            [Box::default(), Box::default()],
            [Box::default(), Box::default()],
            crate::DEFAULT_TURN_LIMIT,
            crate::MAX_ACTIONS_DEFAULT,
        )
        .unwrap();
        GameState::new(Arc::new(konst), 0)
    }

    #[test]
    fn payment_choices_count_ex_resources_like_python() {
        let st = empty_state();
        let pc = |cost, normal, ex| payment_choices(&st, 0, cost, Some((normal, ex)));
        assert_eq!(pc(0, 0, 0), [0]);
        assert_eq!(pc(-2, 0, 3), [0]);
        assert_eq!(pc(2, 5, 2), [0, 1, 2]);
        assert_eq!(pc(3, 2, 1), [1]);
        assert_eq!(pc(4, 1, 5), [3, 4]);
        assert!(pc(2, 0, 1).is_empty());
        assert!(payment_choices(&st, 1, 1, None).is_empty());
    }

    fn trigger(controller: u8, burst: bool) -> TriggerInst {
        TriggerInst {
            program: ProgramId(0),
            controller,
            host: 1,
            host_seq: 0,
            card_uid: 1,
            ability_key: AbilityKey {
                def_id: 0,
                aid: 0,
                unit_text: false,
            },
            event: EventPayload::default(),
            burst,
            once_key: None,
            optional: false,
        }
    }

    #[test]
    fn trigger_candidates_put_bursts_then_the_active_player_first() {
        let batch = [
            trigger(1, false),
            trigger(0, false),
            trigger(1, true),
            trigger(0, true),
        ];
        assert_eq!(trigger_candidates(0, &batch), [2, 3]);
        let batch = [trigger(1, false), trigger(0, false), trigger(0, false)];
        assert_eq!(trigger_candidates(0, &batch), [1, 2]);
        assert_eq!(trigger_candidates(1, &batch), [0]);
        let batch = [trigger(1, false), trigger(1, false)];
        assert_eq!(trigger_candidates(0, &batch), [0, 1]);
    }

    #[test]
    fn expiry_keeps_what_python_keeps() {
        // _expire's keep (game.py:209-216) at turn 5 with player 1 active.
        let keep = |d, created, controller| keep_lasting(5, 1, d, created, controller);
        assert!(!keep(Duration::ThisTurn, 5, 1));
        assert!(!keep(Duration::ThisBattle, 5, 0));
        assert!(!keep(Duration::OpponentNextTurn, 4, 0));
        assert!(keep(Duration::OpponentNextTurn, 5, 0));
        assert!(keep(Duration::OpponentNextTurn, 4, 1));
        assert!(!keep(Duration::YourNextTurn, 3, 1));
        assert!(keep(Duration::YourNextTurn, 3, 0));
        assert!(keep(Duration::WhileOnField, 1, 0));
    }

    #[test]
    fn order_indices_count_from_the_end_when_negative() {
        let mut st = empty_state();
        st.trigger_arena().push_pending(trigger(0, false)).unwrap();
        st.trigger_arena().push_pending(trigger(1, false)).unwrap();
        st.trigger_arena().form_batch().unwrap();
        assert_eq!(top_batch_index(&st, 0), Ok(0));
        assert_eq!(top_batch_index(&st, -1), Ok(1));
        assert_eq!(top_batch_index(&st, -2), Ok(0));
        assert!(top_batch_index(&st, 2).is_err());
        assert!(top_batch_index(&st, -3).is_err());
    }

    #[test]
    fn decisions_and_uids_reject_values_outside_python_lists() {
        let st = empty_state();
        assert_eq!(
            uid_at(&st, 0),
            Err(EngineError::Index("list index out of range".to_owned()))
        );
        assert!(player_at(2).is_err() && player_at(-1).is_err());
        assert_eq!(player_at(1), Ok(1));
    }
}
