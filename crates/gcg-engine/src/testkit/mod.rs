//! Scenario builder and decision helpers for rule, card and AI tests (`testkit.py`;
//! docs/port/contracts.md, gcg-engine `testkit`; PLAN A18). Feature `testkit` only.
//!
//! A [`Scenario`] places specific cards in specific zones and starts the game at a step of the
//! active player's turn, so a test exercises one rule or card effect directly. The helpers
//! find the first matching legal action and apply it; when none matches they fail with
//! Python's `AssertionError` text, which lists the legal options.

pub mod engine;

use std::sync::Arc;

use gcg_core::pyjson::{DumpOptions, dumps};
use gcg_core::rng::SplitMix64;
use gcg_data::{CardDb, CardType, DeckList, DefId, ImplementationStatus};
use gcg_effects::{ContRef, Kw, Registry};

use crate::view;
use crate::{
    Action, ActionKind, AddOptions, ApplyError, BOTH_KNOW, DEFAULT_TURN_LIMIT, DecisionKind,
    EngineError, FILLER_RESOURCE, FILLER_UNIT, GameConst, GameState, MAX_ACTIONS_DEFAULT, OnceElem,
    Phase, Scenario, ScenarioOptions, StateSnapshot, Step, TestOverrides, TestkitError, Uid,
    UnimplementedCardError, ViewKey, Zone,
};

type A = ActionKind;

/// `GameState` constants with new decklists (`_record_decklists`, `testkit.py:144-158`): the
/// listened events follow the decklists (`core.py:295-313`); the test overrides stay.
fn with_decklists(
    old: &GameConst,
    decklists: [Box<[DefId]>; 2],
    resource_decklists: [Box<[DefId]>; 2],
) -> Result<GameConst, EngineError> {
    let mut k = GameConst::new(
        Arc::clone(&old.reg),
        decklists,
        resource_decklists,
        old.turn_limit,
        old.max_actions,
    )?;
    k.test_overrides.clone_from(&old.test_overrides);
    Ok(k)
}

/// A copy of `old` with other test overrides.
fn with_overrides(old: &GameConst, overrides: Option<Arc<TestOverrides>>) -> GameConst {
    GameConst {
        reg: Arc::clone(&old.reg),
        db: old.db,
        decklists: old.decklists.clone(),
        resource_decklists: old.resource_decklists.clone(),
        turn_limit: old.turn_limit,
        max_actions: old.max_actions,
        listened: old.listened,
        hp_reduction_possible: old.hp_reduction_possible,
        vars: old.vars,
        test_overrides: overrides,
    }
}

fn card_def<'a>(db: &'a CardDb, number: &str) -> Result<&'a gcg_data::CardDef, TestkitError> {
    db.get(number)
        .ok_or_else(|| TestkitError::UnknownCard(number.into()))
}

fn engine(e: EngineError) -> TestkitError {
    TestkitError::Engine(e)
}

impl Scenario {
    /// `Scenario(*, active=0, turn=3, seed=0, deck_size=20)` (`testkit.py:38-49`): an empty
    /// `GameState(seed)` at `turn` with `active` and the first player that `turn` implies.
    ///
    /// # Panics
    /// If `reg` lacks one of the engine's frame variables (an IR build defect).
    #[must_use]
    pub fn new(reg: &Arc<Registry>, opts: ScenarioOptions) -> Scenario {
        let konst = GameConst::new(
            Arc::clone(reg),
            [Box::default(), Box::default()],
            [Box::default(), Box::default()],
            DEFAULT_TURN_LIMIT,
            MAX_ACTIONS_DEFAULT,
        )
        .expect("the effect IR holds the engine's frame variables");
        let mut st = GameState::new(Arc::new(konst), opts.seed);
        st.turn = opts.turn;
        st.active = opts.active;
        st.first_player = if opts.turn.rem_euclid(2) == 1 {
            opts.active
        } else {
            1 - opts.active
        };
        Scenario {
            st,
            deck_size: opts.deck_size,
            decks: [Vec::new(), Vec::new()],
            custom_deck: [false, false],
            started: false,
        }
    }

    /// `Scenario.add(player, number, zone, *, rested, damage, pilot, deployed_this_turn, known)`
    /// (`testkit.py:52-82`): a new card (its script required) with the given state; `pilot`
    /// pairs a Pilot card with it. After [`Scenario::start`] the decklists are recorded again.
    ///
    /// # Errors
    /// An unknown card number, an unimplemented card, or an engine error.
    pub fn add(
        &mut self,
        player: u8,
        number: &str,
        zone: Zone,
        opts: AddOptions<'_>,
    ) -> Result<Uid, TestkitError> {
        let reg = Arc::clone(self.st.registry());
        let cdef = card_def(reg.db(), number)?;
        if let Some(reason) = reg.unimplemented_error(cdef.def_id) {
            return Err(engine(EngineError::Unimplemented(UnimplementedCardError {
                card_number: cdef.card_number.clone(),
                reason: reason.into(),
            })));
        }
        let uid = crate::core::new_card(&mut self.st, cdef.def_id, player, zone, opts.rested)
            .map_err(engine)?;
        let turn = self.st.turn;
        let c = &mut self.st.cards[usize::from(uid)];
        c.damage = opts.damage;
        c.entered_turn = if opts.deployed_this_turn {
            turn
        } else {
            turn - 2
        };
        c.known = if opts.known || zone.is_public() {
            BOTH_KNOW
        } else {
            1 << player
        };
        if zone == Zone::Hand {
            c.known = if opts.known { BOTH_KNOW } else { 1 << player };
        }
        if let Some(pilot) = opts.pilot {
            let p = self.add(player, pilot, Zone::Paired, AddOptions::default())?;
            self.st.cards[usize::from(p)].pair = i32::from(uid);
            self.st.cards[usize::from(uid)].pair = i32::from(p);
        }
        self.st.touch();
        if self.started {
            self.record_decklists()?;
        }
        Ok(uid)
    }

    /// `Scenario.add_raw(player, number, zone)` (`testkit.py:84-91`): a new card without
    /// requiring its script.
    ///
    /// # Errors
    /// An unknown card number or an engine error.
    pub fn add_raw(&mut self, player: u8, number: &str, zone: Zone) -> Result<Uid, TestkitError> {
        let def = card_def(self.st.db(), number)?.def_id;
        let uid = crate::core::new_card(&mut self.st, def, player, zone, false).map_err(engine)?;
        if self.started {
            self.record_decklists()?;
        }
        Ok(uid)
    }

    fn add_each(
        &mut self,
        player: u8,
        numbers: &[&str],
        zone: Zone,
    ) -> Result<Vec<Uid>, TestkitError> {
        numbers
            .iter()
            .map(|n| self.add(player, n, zone, AddOptions::default()))
            .collect()
    }

    /// `Scenario.hand(player, *numbers)` (`testkit.py:93-94`).
    ///
    /// # Errors
    /// As [`Scenario::add`].
    pub fn hand(&mut self, player: u8, numbers: &[&str]) -> Result<Vec<Uid>, TestkitError> {
        self.add_each(player, numbers, Zone::Hand)
    }

    /// `Scenario.trash(player, *numbers)` (`testkit.py:96-97`).
    ///
    /// # Errors
    /// As [`Scenario::add`].
    pub fn trash(&mut self, player: u8, numbers: &[&str]) -> Result<Vec<Uid>, TestkitError> {
        self.add_each(player, numbers, Zone::Trash)
    }

    /// `Scenario.shields(player, *numbers)` (`testkit.py:99-106`): the first number is the top
    /// Shield; nobody knows them.
    ///
    /// # Errors
    /// As [`Scenario::add`].
    pub fn shields(&mut self, player: u8, numbers: &[&str]) -> Result<Vec<Uid>, TestkitError> {
        let mut out = Vec::with_capacity(numbers.len());
        for n in numbers {
            let opts = AddOptions {
                known: false,
                ..AddOptions::default()
            };
            let uid = self.add(player, n, Zone::Shield, opts)?;
            self.st.cards[usize::from(uid)].known = 0;
            out.push(uid);
        }
        Ok(out)
    }

    /// `Scenario.deck(player, *numbers)` (`testkit.py:108-111`): the deck's contents, first =
    /// top; `deck_size` filler Units go below them at the start.
    pub fn deck(&mut self, player: u8, numbers: &[&str]) {
        let p = usize::from(player);
        self.decks[p] = numbers.iter().map(|&n| Box::from(n)).collect();
        self.custom_deck[p] = true;
    }

    /// `Scenario.resources(player, n, *, rested=0, ex=0)` (`testkit.py:113-119`): `n` filler
    /// Resources (the first `rested` of them rested), then `ex` EX Resources.
    ///
    /// # Errors
    /// As [`Scenario::add`].
    pub fn resources(
        &mut self,
        player: u8,
        n: usize,
        rested: usize,
        ex: usize,
    ) -> Result<Vec<Uid>, TestkitError> {
        let mut out = Vec::with_capacity(n + ex);
        for i in 0..n {
            let opts = AddOptions {
                rested: i < rested,
                ..AddOptions::default()
            };
            out.push(self.add(player, FILLER_RESOURCE, Zone::ResourceArea, opts)?);
        }
        let ex_number = self.st.db().ex_resource().card_number.clone();
        for _ in 0..ex {
            out.push(self.add(
                player,
                &ex_number,
                Zone::ResourceArea,
                AddOptions::default(),
            )?);
        }
        Ok(out)
    }

    /// `Scenario.base(player, number=None, *, damage=0)` (`testkit.py:121-123`): the EX Base
    /// when `number` is `None` or empty.
    ///
    /// # Errors
    /// As [`Scenario::add`].
    pub fn base(
        &mut self,
        player: u8,
        number: Option<&str>,
        damage: i32,
    ) -> Result<Uid, TestkitError> {
        let ex_base = self.st.db().ex_base().card_number.clone();
        let n = number.filter(|s| !s.is_empty()).unwrap_or(&ex_base);
        let opts = AddOptions {
            damage,
            ..AddOptions::default()
        };
        self.add(player, n, Zone::Base, opts)
    }

    /// `Scenario.resource_deck(player, n)` (`testkit.py:125-127`): `n` hidden filler Resources.
    ///
    /// # Errors
    /// As [`Scenario::add`].
    pub fn resource_deck(&mut self, player: u8, n: usize) -> Result<(), TestkitError> {
        for _ in 0..n {
            let opts = AddOptions {
                known: false,
                ..AddOptions::default()
            };
            self.add(player, FILLER_RESOURCE, Zone::ResourceDeck, opts)?;
        }
        Ok(())
    }

    /// The `force_link` fixture of `tests/cards/test_wp_gd05_a.py:101-113`: the named Units
    /// count as linked with any paired card ([`set_force_link`]).
    pub fn force_link(&mut self, unit_numbers: &[&str]) {
        set_force_link(&mut self.st, unit_numbers);
    }

    /// `_build_decks()` (`testkit.py:130-142`): each player's custom deck (first = top) or
    /// filler Units up to `deck_size`, hidden from both players; then the decklists.
    fn build_decks(&mut self) -> Result<(), TestkitError> {
        for p in 0u8..2 {
            let i = usize::from(p);
            let mut numbers: Vec<Box<str>> = self.decks[i].clone();
            let filler = if self.custom_deck[i] {
                self.deck_size
            } else {
                self.deck_size.saturating_sub(numbers.len())
            };
            numbers.extend(std::iter::repeat_n(Box::from(FILLER_UNIT), filler));
            for n in &numbers {
                let def = card_def(self.st.db(), n)?.def_id;
                let uid = crate::core::new_card(&mut self.st, def, p, Zone::Deck, false)
                    .map_err(engine)?;
                self.st.cards[usize::from(uid)].known = 0;
            }
        }
        self.record_decklists()
    }

    /// `_record_decklists()` (`testkit.py:144-158`): every non-token card joins its owner's
    /// decklist (Resources the resource decklist), sorted.
    fn record_decklists(&mut self) -> Result<(), TestkitError> {
        let db = self.st.db();
        let mut decks: [Vec<DefId>; 2] = [Vec::new(), Vec::new()];
        let mut res: [Vec<DefId>; 2] = [Vec::new(), Vec::new()];
        for c in self.st.cards() {
            let cd = db.by_id(c.def_id);
            if cd.is_token() {
                continue;
            }
            let owner = usize::from(c.owner);
            if cd.card_type == CardType::Resource {
                res[owner].push(c.def_id);
            } else {
                decks[owner].push(c.def_id);
            }
        }
        let sorted = |mut v: Vec<DefId>| -> Box<[DefId]> {
            v.sort();
            v.into_boxed_slice()
        };
        let [d0, d1] = decks;
        let [r0, r1] = res;
        let konst = with_decklists(
            &self.st.konst,
            [sorted(d0), sorted(d1)],
            [sorted(r0), sorted(r1)],
        )
        .map_err(engine)?;
        self.st.konst = Arc::new(konst);
        Ok(())
    }

    /// `Scenario.start(step)` up to its `advance` (`testkit.py:162-180`; open item O14): the
    /// decks are built and recorded, and the state is at `step` of the active player's turn
    /// (an END_ACTION start gives the standby player priority), touched once.
    ///
    /// # Errors
    /// An unknown deck card, an engine error, or a `step` Python's table has no phase for
    /// (its `KeyError`).
    pub fn prepare_start(&mut self, step: Step) -> Result<(), TestkitError> {
        self.build_decks()?;
        self.started = true;
        let phase = match step {
            Step::ActiveStep | Step::StartStep => Phase::Start,
            Step::DrawStep => Phase::Draw,
            Step::ResourceStep => Phase::Resource,
            Step::Main => Phase::Main,
            Step::EndAction | Step::EndStep | Step::HandStep => Phase::End,
            _ => {
                return Err(engine(EngineError::Key(format!(
                    "<Step.{}: {}>",
                    step.py_name(),
                    step.value()
                ))));
            }
        };
        let st = &mut self.st;
        st.phase = phase;
        st.step = step;
        if step == Step::EndAction {
            st.priority = st.standby();
            st.passes = 0;
        }
        st.touch();
        Ok(())
    }

    /// `Scenario.start(step=Step.MAIN)` (`testkit.py:160-182`): [`Scenario::prepare_start`],
    /// then `advance` to the first decision.
    ///
    /// # Errors
    /// As [`Scenario::prepare_start`], plus anything `advance` raises.
    pub fn start(&mut self, step: Step) -> Result<(), TestkitError> {
        self.prepare_start(step)?;
        engine::advance(&mut self.st).map_err(engine)
    }

    /// `sc.st`.
    #[must_use]
    pub fn state(&self) -> &GameState {
        &self.st
    }

    /// `sc.st`, for the helpers that apply actions.
    pub fn state_mut(&mut self) -> &mut GameState {
        &mut self.st
    }

    /// The scenario's state.
    #[must_use]
    pub fn into_state(self) -> GameState {
        self.st
    }
}

/// `vanilla_deck(color="Red", resource=FILLER_RESOURCE)` (`testkit.py:185-199`): a legal
/// 50-card deck of vanilla Units of one colour, four copies each in card-number order, and ten
/// copies of `resource`.
///
/// # Errors
/// Python's `ValueError` when fewer than 50 copies exist, or an unknown `resource` number.
pub fn vanilla_deck(db: &CardDb, color: &str, resource: &str) -> Result<DeckList, TestkitError> {
    let mut units: Vec<&gcg_data::CardDef> = db
        .real_cards()
        .filter(|c| {
            c.card_type == CardType::Unit
                && c.is_vanilla()
                && c.color.is_some_and(|k| k.as_str() == color)
        })
        .collect();
    units.sort_by(|a, b| a.card_number.cmp(&b.card_number));
    let main: Vec<DefId> = units
        .iter()
        .flat_map(|c| std::iter::repeat_n(c.def_id, 4))
        .take(50)
        .collect();
    if main.len() < 50 {
        return Err(TestkitError::Value(format!(
            "not enough vanilla {color} Units for a legal deck"
        )));
    }
    let res = card_def(db, resource)?.def_id;
    Ok(DeckList {
        main: main.into_boxed_slice(),
        resources: vec![res; 10].into_boxed_slice(),
    })
}

// ---------------------------------------------------------------------------------------------
// decision helpers (testkit.py:224-436)

/// `options(st)` (`testkit.py:224-225`).
fn options(st: &GameState) -> &[Action] {
    crate::legal_actions(st)
}

fn fmt_opt(x: Option<i32>) -> String {
    x.map_or_else(|| "None".to_owned(), |v| v.to_string())
}

/// `_fail(st, want)` (`testkit.py:228-234`).
fn fail(st: &GameState, want: &str) -> TestkitError {
    let opts: Vec<String> = options(st).iter().map(ToString::to_string).collect();
    let kind = st.pending().map_or("none", |d| d.hdr.kind.as_str());
    let winner = st
        .winner()
        .map_or_else(|| "None".to_owned(), |w| w.to_string());
    TestkitError::Assertion(format!(
        "no legal action {want}; pending={kind} options=[{}] winner={winner}",
        opts.join(", ")
    ))
}

fn apply_checked(st: &mut GameState, action: Action) -> Result<(), TestkitError> {
    crate::apply(st, action, true).map_err(TestkitError::Apply)
}

/// `act(st, kind, a=None, b=None, c=None)` (`testkit.py:237-254`): applies the first legal
/// action matching the given fields (`None` = any).
///
/// # Errors
/// Python's `AssertionError` when none matches, or an error of `apply`.
pub fn act(
    st: &mut GameState,
    kind: ActionKind,
    a: Option<i32>,
    b: Option<i32>,
    c: Option<i32>,
) -> Result<Action, TestkitError> {
    let found = options(st).iter().copied().find(|o| {
        o.kind == kind
            && a.is_none_or(|x| o.a == x)
            && b.is_none_or(|x| o.b == x)
            && c.is_none_or(|x| o.c == x)
    });
    match found {
        Some(o) => {
            apply_checked(st, o)?;
            Ok(o)
        }
        None => Err(fail(
            st,
            &format!(
                "{}(a={}, b={}, c={})",
                kind.as_str(),
                fmt_opt(a),
                fmt_opt(b),
                fmt_opt(c)
            ),
        )),
    }
}

/// `has_action(st, kind, a=None, b=None)` (`testkit.py:257-260`).
#[must_use]
pub fn has_action(st: &GameState, kind: ActionKind, a: Option<i32>, b: Option<i32>) -> bool {
    options(st)
        .iter()
        .any(|o| o.kind == kind && a.is_none_or(|x| o.a == x) && b.is_none_or(|x| o.b == x))
}

/// `play(st, uid, *, onto=None, ex=0)` (`testkit.py:263-272`): a Unit, Base or Command from the
/// hand, or a Pilot paired `onto` a Unit.
///
/// # Errors
/// As [`act`].
pub fn play(
    st: &mut GameState,
    uid: Uid,
    onto: Option<Uid>,
    ex: i32,
) -> Result<Action, TestkitError> {
    let t = st.def_of(uid).card_type;
    let u = Some(i32::from(uid));
    if let Some(unit) = onto {
        return act(st, A::Pair, u, Some(i32::from(unit)), Some(ex));
    }
    match t {
        CardType::Unit => act(st, A::PlayUnit, u, None, Some(ex)),
        CardType::Base => act(st, A::PlayBase, u, None, Some(ex)),
        _ => act(st, A::PlayCommand, u, None, Some(ex)),
    }
}

/// `activate(st, host, ability=None, ex=0)` (`testkit.py:275-276`).
///
/// # Errors
/// As [`act`].
pub fn activate(
    st: &mut GameState,
    host: Uid,
    ability: Option<i32>,
    ex: i32,
) -> Result<Action, TestkitError> {
    act(st, A::Activate, Some(i32::from(host)), ability, Some(ex))
}

/// `attack(st, attacker, target=PLAYER_TARGET)` (`testkit.py:279-280`).
///
/// # Errors
/// As [`act`].
pub fn attack(st: &mut GameState, attacker: Uid, target: i32) -> Result<Action, TestkitError> {
    act(st, A::Attack, Some(i32::from(attacker)), Some(target), None)
}

/// `end_main(st)` (`testkit.py:283-284`).
///
/// # Errors
/// As [`act`].
pub fn end_main(st: &mut GameState) -> Result<Action, TestkitError> {
    act(st, A::EndMain, None, None, None)
}

/// `block(st, blocker)` (`testkit.py:287-290`): `None` declines.
///
/// # Errors
/// As [`act`].
pub fn block(st: &mut GameState, blocker: Option<Uid>) -> Result<Action, TestkitError> {
    match blocker {
        None => act(st, A::NoBlock, None, None, None),
        Some(u) => act(st, A::Block, Some(i32::from(u)), None, None),
    }
}

/// `pass_(st)` (`testkit.py:293-294`).
///
/// # Errors
/// As [`act`].
pub fn pass(st: &mut GameState) -> Result<Action, TestkitError> {
    act(st, A::Pass, None, None, None)
}

/// `yes(st)` (`testkit.py:297-298`).
///
/// # Errors
/// As [`act`].
pub fn yes(st: &mut GameState) -> Result<Action, TestkitError> {
    act(st, A::Yes, None, None, None)
}

/// `no(st)` (`testkit.py:301-302`).
///
/// # Errors
/// As [`act`].
pub fn no(st: &mut GameState) -> Result<Action, TestkitError> {
    act(st, A::No, None, None, None)
}

fn pending_kind(st: &GameState) -> Option<DecisionKind> {
    st.pending().map(|d| d.hdr.kind)
}

/// `yes_if_asked(st)` (`testkit.py:305-308`): accepts a pending "you may" prompt.
///
/// # Errors
/// As [`act`].
pub fn yes_if_asked(st: &mut GameState) -> Result<(), TestkitError> {
    if pending_kind(st) == Some(DecisionKind::YesNo) {
        act(st, A::Yes, None, None, None)?;
    }
    Ok(())
}

/// `select(st, *uids, done=None)` (`testkit.py:311-322`): picks the cards, then DONE when
/// offered and `done` is not `Some(false)` (with `None` only for a SELECT decision).
///
/// # Errors
/// As [`act`].
pub fn select(st: &mut GameState, uids: &[Uid], done: Option<bool>) -> Result<(), TestkitError> {
    for &u in uids {
        act(st, A::Select, Some(i32::from(u)), None, None)?;
    }
    if done != Some(false)
        && st.pending().is_some()
        && has_action(st, A::Done, None, None)
        && (done == Some(true) || pending_kind(st) == Some(DecisionKind::Select))
    {
        act(st, A::Done, None, None, None)?;
    }
    Ok(())
}

/// `arrange_if_asked(st)` (`testkit.py:325-329`): every pending ARRANGE takes its first option.
///
/// # Errors
/// An error of `apply`.
pub fn arrange_if_asked(st: &mut GameState) -> Result<(), TestkitError> {
    while pending_kind(st) == Some(DecisionKind::Arrange) {
        let first = options(st)[0];
        apply_checked(st, first)?;
    }
    Ok(())
}

/// `select_if_asked(st, *uids, done=None)` (`testkit.py:332-337`): [`select`] when one of
/// `uids` is offered.
///
/// # Errors
/// As [`select`].
pub fn select_if_asked(
    st: &mut GameState,
    uids: &[Uid],
    done: Option<bool>,
) -> Result<(), TestkitError> {
    let offered = uids.iter().any(|&u| {
        options(st)
            .iter()
            .any(|o| o.kind == A::Select && o.a == i32::from(u))
    });
    if st.pending().is_some() && offered {
        select(st, uids, done)?;
    }
    Ok(())
}

/// `choose_option(st, index)` (`testkit.py:340-341`).
///
/// # Errors
/// As [`act`].
pub fn choose_option(st: &mut GameState, index: i32) -> Result<Action, TestkitError> {
    act(st, A::Select, Some(index), None, None)
}

/// `order(st, index=0)` (`testkit.py:344-345`): the ORDER of the `index`-th option.
///
/// # Errors
/// Python's `IndexError` for an index past the options, or as [`act`].
pub fn order(st: &mut GameState, index: usize) -> Result<Action, TestkitError> {
    let a = options(st)
        .get(index)
        .map(|o| o.a)
        .ok_or_else(|| engine(EngineError::Index("tuple index out of range".to_owned())))?;
    act(st, A::Order, Some(a), None, None)
}

/// `pass_all(st, *, max_steps=50)` (`testkit.py:348-360`): passes action steps and declines
/// blocks until another decision (or the end of the game).
///
/// # Errors
/// As [`act`].
pub fn pass_all(st: &mut GameState, max_steps: usize) -> Result<(), TestkitError> {
    for _ in 0..max_steps {
        match pending_kind(st) {
            Some(DecisionKind::ActionStep) => {
                act(st, A::Pass, None, None, None)?;
            }
            Some(DecisionKind::Block) => {
                act(st, A::NoBlock, None, None, None)?;
            }
            _ => return Ok(()),
        }
    }
    Ok(())
}

/// `to_next_turn(st)` (`testkit.py:363-380`): ends the main phase and passes through the end
/// phase to the next MAIN decision.
///
/// # Errors
/// Python's `AssertionError` at a decision it cannot answer, or as [`act`].
pub fn to_next_turn(st: &mut GameState) -> Result<(), TestkitError> {
    if pending_kind(st) == Some(DecisionKind::Main) {
        end_main(st)?;
    }
    for _ in 0..200 {
        match pending_kind(st) {
            None | Some(DecisionKind::Main) => return Ok(()),
            Some(DecisionKind::ActionStep) => {
                act(st, A::Pass, None, None, None)?;
            }
            Some(DecisionKind::Block) => {
                act(st, A::NoBlock, None, None, None)?;
            }
            Some(DecisionKind::Discard) => {
                let a = options(st)[0].a;
                act(st, A::Select, Some(a), None, None)?;
            }
            Some(_) => return Err(fail(st, "to reach the next main phase")),
        }
    }
    Ok(())
}

/// `zone_of(st, uid)` (`testkit.py:383-384`).
#[must_use]
pub fn zone_of(st: &GameState, uid: Uid) -> Zone {
    st.card(uid).zone
}

/// `ap(st, uid)` (`testkit.py:387-388`).
///
/// # Errors
/// An error of the derived view.
pub fn ap(st: &GameState, uid: Uid) -> Result<i32, TestkitError> {
    let dv = view::derived(st).map_err(engine)?;
    Ok(view::ap_of(st, &dv, uid))
}

/// `hp(st, uid)` (`testkit.py:391-392`).
///
/// # Errors
/// An error of the derived view.
pub fn hp(st: &GameState, uid: Uid) -> Result<i32, TestkitError> {
    let dv = view::derived(st).map_err(engine)?;
    Ok(view::hp_of(st, &dv, uid))
}

/// `keywords(st, uid)` (`testkit.py:395-396`): the derived keywords with their amounts, in
/// [`Kw`] declaration order.
///
/// # Errors
/// An error of the derived view.
pub fn keywords(st: &GameState, uid: Uid) -> Result<Vec<(Kw, i32)>, TestkitError> {
    let dv = view::derived(st).map_err(engine)?;
    let kws = view::keywords(&dv, uid);
    Ok(Kw::ALL
        .iter()
        .filter_map(|&k| kws.get(k).map(|n| (k, i32::from(n))))
        .collect())
}

/// `uids_in(st, player, zone)` (`testkit.py:399-400`).
#[must_use]
pub fn uids_in(st: &GameState, player: u8, zone: Zone) -> Vec<Uid> {
    st.zone(player, zone).to_vec()
}

/// `numbers_in(st, player, zone)` (`testkit.py:403-404`).
#[must_use]
pub fn numbers_in(st: &GameState, player: u8, zone: Zone) -> Vec<&'static str> {
    card_numbers(st, st.zone(player, zone))
}

/// `legal_kinds(st)` (`testkit.py:407-408`): a set of kinds, here deduplicated in
/// [`ActionKind`] order.
#[must_use]
pub fn legal_kinds(st: &GameState) -> Vec<ActionKind> {
    let mut kinds: Vec<ActionKind> = options(st).iter().map(|o| o.kind).collect();
    kinds.sort();
    kinds.dedup();
    kinds
}

/// `expect_illegal(st, action)` (`testkit.py:411-416`).
///
/// # Errors
/// Python's `AssertionError` when the action is accepted; an engine error of `apply`.
pub fn expect_illegal(st: &mut GameState, action: Action) -> Result<(), TestkitError> {
    match crate::apply(st, action, true) {
        Err(ApplyError::Illegal(_)) => Ok(()),
        Err(e @ ApplyError::Engine(_)) => Err(TestkitError::Apply(e)),
        Ok(()) => Err(TestkitError::Assertion(format!(
            "{action} was accepted but should be illegal"
        ))),
    }
}

/// `run_random(st, seed, *, max_actions=5000)` (`testkit.py:419-428`): uniformly random legal
/// actions with `SplitMix64(seed)`.
///
/// # Errors
/// An error of `apply`.
pub fn run_random(st: &mut GameState, seed: u64, max_actions: usize) -> Result<(), TestkitError> {
    let mut rng = SplitMix64::new(seed);
    for _ in 0..max_actions {
        if st.pending().is_none() {
            return Ok(());
        }
        let n = options(st).len();
        if n == 0 {
            return Err(TestkitError::Value("randrange() requires n > 0".to_owned()));
        }
        let a = options(st)[rng.randrange_usize(n)];
        apply_checked(st, a)?;
    }
    Ok(())
}

/// `names(st, uids)` (`testkit.py:431-432`).
#[must_use]
pub fn names(st: &GameState, uids: &[Uid]) -> Vec<&'static str> {
    uids.iter().map(|&u| &*st.def_of(u).name).collect()
}

/// `card_numbers(st, uids)` (`testkit.py:435-436`).
#[must_use]
pub fn card_numbers(st: &GameState, uids: &[Uid]) -> Vec<&'static str> {
    uids.iter().map(|&u| &*st.def_of(u).card_number).collect()
}

// ---------------------------------------------------------------------------------------------
// trace replay hooks (docs/port/contracts.md, gcg-engine testkit)

/// A `state` record with `"derived": "stale"`: `view::derived(view_from)` becomes `st`'s cached
/// view, so the next `view::derived(st)` returns it while `st`'s key equals `key`
/// (`view.py:232-239`). No touch.
///
/// # Errors
/// An error of the view, or [`EngineError::Internal`] when that view's key is not `key`.
pub fn install_view(
    st: &GameState,
    view_from: &GameState,
    key: ViewKey,
) -> Result<(), EngineError> {
    let dv = view::derived(view_from)?;
    if dv.key() != key {
        return Err(EngineError::Internal(format!(
            "the view to install has key {:?}, not {key:?}",
            dv.key()
        )));
    }
    *st.cache.borrow_mut() = Some(dv);
    Ok(())
}

/// A `force_link` record: `st`'s game constants get `TestOverrides { force_link: units }` (a
/// new `Arc`; clones share it from here on). No touch: the monkeypatch of
/// `tests/cards/test_wp_gd05_a.py:101-113` touches nothing.
pub fn set_force_link(st: &mut GameState, units: &[&str]) {
    let overrides = TestOverrides {
        force_link: units.iter().map(|&u| Box::from(u)).collect(),
    };
    let konst = with_overrides(&st.konst, Some(Arc::new(overrides)));
    st.konst = Arc::new(konst);
}

/// The inline side-table node of a continuous effect that no IR node spells (PLAN §4.8,
/// §5.6), in the `node_json` form of `port/oracle/traces.py`; `None` for an IR node, which
/// resolves by its content id.
fn value_node(reg: &Registry, c: ContRef) -> Option<serde_json::Value> {
    Some(match c {
        ContRef::StatMod { ap, hp } => serde_json::json!({"_": "StatMod", "ap": ap, "hp": hp}),
        ContRef::KeywordGrant { kw, amount } => {
            serde_json::json!({"_": "KeywordGrant", "keyword": kw.as_str(), "amount": amount})
        }
        ContRef::AbilityGrant { card, index } => serde_json::json!(
            {"_": "AbilityGrant", "card_number": reg.str(card), "ability_index": index}
        ),
        ContRef::Node(_) => return None,
    })
}

/// `st` as a trace snapshot (PLAN §4.8; the `snapshot` record of `port/oracle/traces.py`): the
/// canonical state, a `continuous` side table with the value effects (a `StatMod` with int
/// fields, a `KeywordGrant` with an int amount, an `AbilityGrant`) of its lasting effects,
/// delayed effects and once keys, and `rules_checked`. IR effects and filter sets resolve by
/// content id in the registry, so `canon::from_canonical` rebuilds `st` from it.
///
/// # Panics
/// Never: the canonical form always dumps to valid JSON.
#[must_use]
pub fn snapshot(st: &GameState) -> StateSnapshot {
    const COMPACT: DumpOptions = DumpOptions {
        indent: None,
        sort_keys: false,
        compact: true,
        ensure_ascii: true,
    };
    let reg = st.registry();
    let mut continuous = serde_json::Map::new();
    let named = st
        .lasting()
        .iter()
        .chain(st.delayed())
        .map(|l| l.effect)
        .chain(st.once_used().iter().flat_map(|k| {
            k.0.as_slice().iter().filter_map(|e| match e {
                OnceElem::Cont(c) => Some(*c),
                OnceElem::Int(_) => None,
            })
        }));
    for c in named {
        if let Some(node) = value_node(reg, c) {
            continuous.insert(c.content_id(reg).as_str().to_owned(), node);
        }
    }
    let text = dumps(&crate::canon::canonical_state(st), COMPACT);
    StateSnapshot {
        state: serde_json::from_str(&text).expect("the canonical state is JSON"),
        continuous: serde_json::Value::Object(continuous),
        filter_sets: serde_json::Value::Object(serde_json::Map::new()),
        rules_checked: st.rules_checked(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_arguments_print_as_none() {
        assert_eq!(fmt_opt(None), "None");
        assert_eq!(fmt_opt(Some(-2)), "-2");
    }
}
