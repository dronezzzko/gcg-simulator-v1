//! Per-card instrumentation of the real game, never of search copies (`runner/instrument.py`;
//! docs/port/inventory/01-rng-runner-cli.md §2.7).
//!
//! The observer sees every decision of the real game: the action about to be applied and the
//! state after it. Plays come from the action stream; draws from the hand and the engine's
//! per-turn event history, which also records draws whose cards leave the hand before the next
//! decision. Benchmark cards count as seen once they are played, deployed, paired, revealed as
//! a Burst, or present in a public in-play zone. Every card-number output is sorted (Python's
//! `sorted` of `str`, code point order), so the sets need only Python's membership semantics.

use std::collections::BTreeSet;

use gcg_engine::{Action, ActionKind, DecisionKind, GameState, HistKind, Uid, Zone};

use crate::config::own_turn;
use crate::{DutCardEvents, PlayEvent};

/// `PLAY_KINDS` (`runner/instrument.py:20-22`).
const PLAY_KINDS: [ActionKind; 4] = [
    ActionKind::PlayUnit,
    ActionKind::PlayBase,
    ActionKind::PlayCommand,
    ActionKind::Pair,
];
/// `IN_PLAY_ZONES` (`runner/instrument.py:23`), in Python's order.
const IN_PLAY_ZONES: [Zone; 4] = [Zone::Battle, Zone::Paired, Zone::Base, Zone::Resolving];
/// `DRAW_EVENTS` (`runner/instrument.py:24`).
const DRAW_EVENTS: [HistKind; 2] = [HistKind::Draw, HistKind::ShieldToHand];
/// `SEEN_EVENTS` (`runner/instrument.py:25`).
const SEEN_EVENTS: [HistKind; 5] = [
    HistKind::Play,
    HistKind::Deployed,
    HistKind::Paired,
    HistKind::BurstRevealed,
    HistKind::CommandActivated,
];

/// A Python exception raised inside the observer, which `play_game` reports as
/// `"... failed: {class}: {message}"` (`runner/game.py:130-136`).
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct PyFailure {
    pub(crate) class: &'static str,
    pub(crate) message: String,
}

/// `st.cards[uid]` with Python's list indexing: a negative index counts from the end, and an
/// index outside the list raises `IndexError` (`V.cdef`, `engine/view.py:23-24`).
pub(crate) fn py_card_index(st: &GameState, uid: i32) -> Result<Uid, PyFailure> {
    let len = i64::try_from(st.cards().len()).unwrap_or(i64::MAX);
    let i = i64::from(uid);
    let i = if i < 0 { i + len } else { i };
    if (0..len).contains(&i) {
        Uid::try_from(i).map_err(|_| index_error())
    } else {
        Err(index_error())
    }
}

fn index_error() -> PyFailure {
    PyFailure {
        class: "IndexError",
        message: "list index out of range".to_owned(),
    }
}

/// `card_number(st, uid)` (`runner/instrument.py:28-29`).
fn card_number(st: &GameState, uid: Uid) -> Box<str> {
    st.def_of(uid).card_number.clone()
}

/// `GameObserver(st, dut_seat)` (`runner/instrument.py:32-107`).
#[derive(Debug)]
pub(crate) struct GameObserver {
    dut: u8,
    bench: u8,
    initial_hand: Vec<Uid>,
    redrew: bool,
    opening_hand: Vec<Uid>,
    tracking: bool,
    /// `drawn` (an insertion-ordered dict used as a set; only membership reaches the output).
    drawn: BTreeSet<Uid>,
    plays: Vec<PlayEvent>,
    played_uids: BTreeSet<Uid>,
    bench_seen: BTreeSet<Box<str>>,
    /// The identity of `st.history` (`self._history`): the engine replaces the list at the
    /// end of setup and at each new turn (`engine/game.py:190`, `:336`; Inv 01 hazard 18).
    history_epoch: u32,
    history_done: usize,
}

impl GameObserver {
    /// `GameObserver.__init__` (`runner/instrument.py:35-47`).
    pub(crate) fn new(st: &GameState, dut_seat: u8) -> Self {
        Self {
            dut: dut_seat,
            bench: 1 - dut_seat,
            initial_hand: Vec::new(),
            redrew: false,
            opening_hand: Vec::new(),
            tracking: false,
            drawn: BTreeSet::new(),
            plays: Vec::new(),
            played_uids: BTreeSet::new(),
            bench_seen: BTreeSet::new(),
            history_epoch: st.history_epoch(),
            history_done: 0,
        }
    }

    /// `before(st, decision, action)` (`runner/instrument.py:49-60`): after `choose`, before
    /// `apply`.
    pub(crate) fn before(
        &mut self,
        st: &GameState,
        kind: DecisionKind,
        player: u8,
        action: Action,
    ) -> Result<(), PyFailure> {
        if kind == DecisionKind::Redraw && player == self.dut {
            self.initial_hand = st.zone(self.dut, Zone::Hand).to_vec();
        }
        if !PLAY_KINDS.contains(&action.kind) {
            return Ok(());
        }
        let uid = py_card_index(st, action.a)?;
        let number = card_number(st, uid);
        if player == self.dut {
            let turn = own_turn(st.turn(), self.dut, st.first_player());
            self.plays.push(PlayEvent {
                card_number: number,
                turn: st.turn(),
                own_turn: turn,
            });
            self.played_uids.insert(uid);
        } else {
            self.see_bench(st, uid);
        }
        Ok(())
    }

    /// `after(st, decision, action)` (`runner/instrument.py:62-73`).
    pub(crate) fn after(&mut self, st: &GameState, kind: DecisionKind, player: u8, action: Action) {
        if kind == DecisionKind::Redraw && player == self.dut {
            self.redrew = action.kind == ActionKind::Redraw;
            self.opening_hand = st.zone(self.dut, Zone::Hand).to_vec();
            self.tracking = true;
        }
        self.scan_history(st);
        if self.tracking {
            self.drawn
                .extend(st.zone(self.dut, Zone::Hand).iter().copied());
        }
        for zone in IN_PLAY_ZONES {
            for &uid in st.zone(self.bench, zone) {
                self.see_bench(st, uid);
            }
        }
    }

    /// `_see_bench(st, uid)` (`runner/instrument.py:75-78`).
    fn see_bench(&mut self, st: &GameState, uid: Uid) {
        let cdef = st.def_of(uid);
        if st.card(uid).owner == self.bench && !cdef.is_token() {
            self.bench_seen.insert(cdef.card_number.clone());
        }
    }

    /// `_scan_history(st)` (`runner/instrument.py:80-92`).
    fn scan_history(&mut self, st: &GameState) {
        if st.history_epoch() != self.history_epoch {
            self.history_epoch = st.history_epoch();
            self.history_done = 0;
        }
        let history = st.history();
        for h in history.get(self.history_done..).unwrap_or_default() {
            let Ok(uid) = Uid::try_from(h.uid) else {
                continue;
            };
            let owner = st.card(uid).owner;
            if self.tracking && DRAW_EVENTS.contains(&h.kind) && owner == self.dut {
                self.drawn.insert(uid);
            } else if SEEN_EVENTS.contains(&h.kind) && owner == self.bench {
                self.see_bench(st, uid);
            }
        }
        self.history_done = history.len();
    }

    /// `dut_events(st)` (`runner/instrument.py:94-107`).
    pub(crate) fn dut_events(&self, st: &GameState) -> DutCardEvents {
        let numbers = |uids: &mut dyn Iterator<Item = Uid>| -> Vec<Box<str>> {
            let mut out: Vec<Box<str>> = uids.map(|u| card_number(st, u)).collect();
            out.sort();
            out
        };
        DutCardEvents {
            initial_hand: numbers(&mut self.initial_hand.iter().copied()),
            redrew: self.redrew,
            opening_hand: numbers(&mut self.opening_hand.iter().copied()),
            drawn: numbers(&mut self.drawn.iter().copied()),
            played: self.plays.clone(),
            never_played: numbers(
                &mut self
                    .drawn
                    .iter()
                    .copied()
                    .filter(|u| !self.played_uids.contains(u)),
            ),
            in_hand_at_end: numbers(&mut st.zone(self.dut, Zone::Hand).iter().copied()),
        }
    }

    /// `sorted(observer.bench_seen)` (`runner/game.py:161`).
    pub(crate) fn bench_seen(&self) -> Vec<Box<str>> {
        self.bench_seen.iter().cloned().collect()
    }
}
