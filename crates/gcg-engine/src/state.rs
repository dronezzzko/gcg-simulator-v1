//! Layer L1 of the engine (docs/dev/PLAN.md §4.1-§4.5, §10.3.3-§10.3.4): `GameState`
//! construction (`engine/state.py:475-511`), the read-only accessors, `touch`
//! (`state.py:529-531`), `clone_from_state` (`state.py:534-572`, PLAN §4.5) and the arena
//! helpers through which every other layer changes the zone lists, frames, lasting effects,
//! triggers, once keys and the pending decision (PLAN §4.3).
//!
//! The arena logic lives in small borrowed views ([`FrameArena`], [`TriggerArena`],
//! [`LastingArena`]) over the frozen `GameState` fields, so it is tested against list models
//! without building a whole game.

use std::cell::RefCell;
use std::sync::Arc;

use gcg_core::rng::SplitMix64;
use gcg_data::{CardDb, CardDef, DefId};
use gcg_effects::{EvSet, Registry, VarId};

use crate::canon::StateBody;
use crate::{
    Battle, CardInstance, DecisionHdr, DecisionView, EndReason, EngineError, EngineVars, Frame,
    FrameVars, GameConst, GameState, HistoryEvent, IntEntry, IntKey, Lasting, OnceKey, Phase, Span,
    Step, TestOverrides, TriggerInst, Uid, VarEntry, Zone, ZoneLists,
};

/// `u32` arena offsets: a longer arena is [`EngineError::Overflow`] (PLAN §4.1 item 6).
fn offset(n: usize, what: &'static str) -> Result<u32, EngineError> {
    u32::try_from(n).map_err(|_| EngineError::Overflow { what })
}

/// The region `start..start + len`. An empty region is always [`Span::EMPTY`], so that no
/// empty span can point past an arena's end after a tail region shrinks or the arena is
/// truncated; an empty region never counts as being at the tail of a non-empty arena.
pub(crate) fn region(start: usize, len: usize, what: &'static str) -> Result<Span, EngineError> {
    if len == 0 {
        return Ok(Span::EMPTY);
    }
    offset(start + len, what)?;
    Ok(Span {
        start: offset(start, what)?,
        len: offset(len, what)?,
    })
}

/// Writes `data` as the whole content of region `span` of `arena`: in place when the region
/// ends the arena or `data` fits, otherwise relocated to the tail (the old region becomes
/// garbage). Returns the new span.
fn write_region<T: Copy>(
    arena: &mut Vec<T>,
    span: Span,
    data: &[T],
    what: &'static str,
) -> Result<Span, EngineError> {
    if span.len > 0 && span.end() == arena.len() {
        region(span.start as usize, data.len(), what)?;
        arena.truncate(span.start as usize);
        arena.extend_from_slice(data);
        region(span.start as usize, data.len(), what)
    } else if data.len() <= span.len as usize {
        let at = span.start as usize;
        arena[at..at + data.len()].copy_from_slice(data);
        region(at, data.len(), what)
    } else {
        let start = arena.len();
        let span = region(start, data.len(), what)?;
        arena.extend_from_slice(data);
        Ok(span)
    }
}

/// Appends `x` to region `span` of `arena`, in place when the region ends the arena,
/// otherwise after relocating the region to the tail.
fn append_region<T: Copy>(
    arena: &mut Vec<T>,
    span: Span,
    x: T,
    what: &'static str,
) -> Result<Span, EngineError> {
    let len = span.len as usize + 1;
    if span.len > 0 && span.end() == arena.len() {
        let grown = region(span.start as usize, len, what)?;
        arena.push(x);
        Ok(grown)
    } else {
        let moved = region(arena.len(), len, what)?;
        arena.extend_from_within(span.range());
        arena.push(x);
        Ok(moved)
    }
}

/// Removes element `i` of region `span`, keeping the order of the others.
fn remove_in_region<T: Copy>(arena: &mut Vec<T>, span: Span, i: usize) -> Span {
    let r = span.range();
    arena.copy_within(r.start + i + 1..r.end, r.start + i);
    if span.end() == arena.len() {
        arena.truncate(span.end() - 1);
    }
    if span.len == 1 {
        Span::EMPTY
    } else {
        Span {
            start: span.start,
            len: span.len - 1,
        }
    }
}

/// `s.add(key)` on a Python set kept as an insertion-ordered list: nothing when an equal key
/// is present (`st.once_used`, `engine/state.py:497`; PLAN §4.3). The only writer of
/// `once_used`, so the list never holds a duplicate that `canonical_state` would serialise.
pub(crate) fn set_add(keys: &mut Vec<OnceKey>, key: OnceKey) {
    if !keys.contains(&key) {
        keys.push(key);
    }
}

/// Garbage above this many elements is compacted when it also exceeds the live size.
const COMPACT_MIN_GARBAGE: usize = 64;

const FRAME_VARS: &str = "frame vars arena";
const FRAME_INTS: &str = "frame ints arena";
const FRAME_ITEMS: &str = "frame items arena";
const LASTING_TARGETS: &str = "lasting targets arena";

/// `st.frames` with the arenas of their `vars`, `ints` and `buf` (PLAN §4.3). Regions of the
/// top frame sit at the arena tails and grow in place; a region that must grow elsewhere is
/// relocated to the tail. Popping truncates every arena to what the remaining frames use and
/// compacts when garbage exceeds the live size.
pub(crate) struct FrameArena<'a> {
    pub(crate) frames: &'a mut Vec<Frame>,
    pub(crate) vars: &'a mut Vec<VarEntry>,
    pub(crate) ints: &'a mut Vec<IntEntry>,
    pub(crate) items: &'a mut Vec<i32>,
}

impl FrameArena<'_> {
    /// `st.frames.append(f)` with `f.vars = dict(vars)` (`interp.py:72-87`); `f`'s spans are
    /// ignored, and its `ints` and `buf` start empty. A repeated name keeps its first position
    /// and the last value, as `dict` does. Returns the frame's index.
    pub(crate) fn push(
        &mut self,
        mut f: Frame,
        vars: &[(VarId, &[i32])],
    ) -> Result<usize, EngineError> {
        let start = self.vars.len();
        f.vars = Span::EMPTY;
        for &(var, data) in vars {
            if let Some(k) = self.vars[start..].iter().position(|e| e.var == var) {
                let at = start + k;
                self.vars[at].items =
                    write_region(self.items, self.vars[at].items, data, FRAME_ITEMS)?;
            } else {
                let items = write_region(self.items, Span::EMPTY, data, FRAME_ITEMS)?;
                f.vars = append_region(self.vars, f.vars, VarEntry { var, items }, FRAME_VARS)?;
            }
        }
        f.ints = Span::EMPTY;
        f.buf = Span::EMPTY;
        self.frames.push(f);
        Ok(self.frames.len() - 1)
    }

    /// `st.frames.pop()` (`interp.py:131`). The returned frame's spans are emptied: its
    /// regions are freed with it.
    pub(crate) fn pop(&mut self) -> Option<Frame> {
        let mut f = self.frames.pop()?;
        f.vars = Span::EMPTY;
        f.ints = Span::EMPTY;
        f.buf = Span::EMPTY;
        let (mut vars_end, mut ints_end, mut items_end) = (0, 0, 0);
        let (mut vars_live, mut ints_live, mut items_live) = (0, 0, 0);
        for g in self.frames.iter() {
            vars_end = vars_end.max(g.vars.end());
            ints_end = ints_end.max(g.ints.end());
            items_end = items_end.max(g.buf.end());
            vars_live += g.vars.len as usize;
            ints_live += g.ints.len as usize;
            items_live += g.buf.len as usize;
            for e in &self.vars[g.vars.range()] {
                items_end = items_end.max(e.items.end());
                items_live += e.items.len as usize;
            }
        }
        self.vars.truncate(vars_end);
        self.ints.truncate(ints_end);
        self.items.truncate(items_end);
        let garbage =
            |len: usize, live: usize| len.saturating_sub(live) > live.max(COMPACT_MIN_GARBAGE - 1);
        if garbage(vars_end, vars_live)
            || garbage(ints_end, ints_live)
            || garbage(items_end, items_live)
        {
            self.compact();
        }
        Some(f)
    }

    /// Rebuilds the three arenas in frame order with no garbage.
    fn compact(&mut self) {
        let old_vars = std::mem::take(self.vars);
        let old_ints = std::mem::take(self.ints);
        let old_items = std::mem::take(self.items);
        for f in self.frames.iter_mut() {
            let vstart = self.vars.len();
            for e in &old_vars[f.vars.range()] {
                let istart = self.items.len();
                self.items.extend_from_slice(&old_items[e.items.range()]);
                self.vars.push(VarEntry {
                    var: e.var,
                    items: span_of(istart, e.items.len),
                });
            }
            f.vars = span_of(vstart, f.vars.len);
            let bstart = self.items.len();
            self.items.extend_from_slice(&old_items[f.buf.range()]);
            f.buf = span_of(bstart, f.buf.len);
            let istart = self.ints.len();
            self.ints.extend_from_slice(&old_ints[f.ints.range()]);
            f.ints = span_of(istart, f.ints.len);
        }
    }

    /// `f.vars[var] = tuple(data)` (`interp.py:285`, `:445`, ...).
    pub(crate) fn var_set(
        &mut self,
        i: usize,
        var: VarId,
        data: &[i32],
    ) -> Result<(), EngineError> {
        let f = self.frames[i];
        if let Some(k) = self.vars[f.vars.range()].iter().position(|e| e.var == var) {
            let at = f.vars.start as usize + k;
            self.vars[at].items = write_region(self.items, self.vars[at].items, data, FRAME_ITEMS)?;
        } else {
            let items = write_region(self.items, Span::EMPTY, data, FRAME_ITEMS)?;
            self.frames[i].vars =
                append_region(self.vars, f.vars, VarEntry { var, items }, FRAME_VARS)?;
        }
        Ok(())
    }

    /// `del f.vars[var]` (`interp.py:828`); nothing when `var` is absent.
    pub(crate) fn var_del(&mut self, i: usize, var: VarId) {
        let f = self.frames[i];
        if let Some(k) = self.vars[f.vars.range()].iter().position(|e| e.var == var) {
            self.frames[i].vars = remove_in_region(self.vars, f.vars, k);
        }
    }

    /// `f.ints[key] = value` (`interp.py:207`, `:286-306`, `:671-675`, `game.py:933`).
    pub(crate) fn int_set(&mut self, i: usize, key: IntKey, value: i32) -> Result<(), EngineError> {
        let f = self.frames[i];
        let r = f.ints.range();
        if let Some(k) = self.ints[r.clone()].iter().position(|e| e.key == key) {
            self.ints[r.start + k].value = value;
        } else {
            self.frames[i].ints =
                append_region(self.ints, f.ints, IntEntry { key, value }, FRAME_INTS)?;
        }
        Ok(())
    }

    /// `del f.ints[key]` / `f.ints.pop(key, None)` (`interp.py:209`, `:701-702`): the removed
    /// value, or `None` when the key is absent.
    pub(crate) fn int_del(&mut self, i: usize, key: IntKey) -> Option<i32> {
        let f = self.frames[i];
        let k = self.ints[f.ints.range()]
            .iter()
            .position(|e| e.key == key)?;
        let value = self.ints[f.ints.start as usize + k].value;
        self.frames[i].ints = remove_in_region(self.ints, f.ints, k);
        Some(value)
    }

    /// `f.buf = list(data)` (`interp.py:414`, `:809`, `:829`, ...).
    pub(crate) fn buf_set(&mut self, i: usize, data: &[i32]) -> Result<(), EngineError> {
        self.frames[i].buf = write_region(self.items, self.frames[i].buf, data, FRAME_ITEMS)?;
        Ok(())
    }

    /// `f.buf.append(x)` (`interp.py:454`, `:1215`).
    pub(crate) fn buf_push(&mut self, i: usize, x: i32) -> Result<(), EngineError> {
        self.frames[i].buf = append_region(self.items, self.frames[i].buf, x, FRAME_ITEMS)?;
        Ok(())
    }
}

/// A region of a compacted arena. Compaction never grows an arena, so every region fits the
/// `u32` offsets it had before; an empty one is [`Span::EMPTY`].
fn span_of(start: usize, len: u32) -> Span {
    if len == 0 {
        Span::EMPTY
    } else {
        Span {
            start: u32::try_from(start).unwrap_or(u32::MAX),
            len,
        }
    }
}

/// `st.pending_triggers` and `st.batches` as one list (PLAN §4.3): `[batch 0 | ... | batch k |
/// pending_triggers]`, with `batch_starts[i]` the start of batch `i` and `pending_start` the
/// start of the pending tail. Empty batches are representable.
#[allow(
    dead_code,
    reason = "called by layers L3-L5 (docs/dev/contracts.md); they land after L1"
)]
pub(crate) struct TriggerArena<'a> {
    pub(crate) triggers: &'a mut Vec<TriggerInst>,
    pub(crate) batch_starts: &'a mut Vec<u32>,
    pub(crate) pending_start: &'a mut u32,
    /// W-162 (`paranoid`): the side table kept parallel to `triggers`, and the cards whose
    /// `zone_seq` it records at queue time.
    #[cfg(feature = "paranoid")]
    pub(crate) seqs: &'a mut crate::paranoid::TriggerSeqs,
    #[cfg(feature = "paranoid")]
    pub(crate) cards: &'a [CardInstance],
}

/// The end of batch `i`: the next batch's start, or the start of the pending tail.
fn batch_end(batch_starts: &[u32], pending_start: u32, i: usize) -> usize {
    batch_starts.get(i + 1).copied().unwrap_or(pending_start) as usize
}

#[allow(
    dead_code,
    reason = "called by layers L3-L5 (docs/dev/contracts.md); they land after L1"
)]
impl TriggerArena<'_> {
    /// `st.pending_triggers.append(t)` (`core.py:273`, `:393`, `:449`, `:472`, `:714`).
    pub(crate) fn push_pending(&mut self, t: TriggerInst) -> Result<(), EngineError> {
        offset(self.triggers.len() + 1, "trigger list")?;
        #[cfg(feature = "paranoid")]
        {
            let seq = usize::try_from(t.host)
                .ok()
                .and_then(|u| self.cards.get(u))
                .map_or(crate::NO_ARG, |c| c.zone_seq);
            self.seqs.queued.push(seq);
        }
        self.triggers.push(t);
        Ok(())
    }

    /// `_form_batch` (`game.py:667-670`): the pending triggers become the new top batch.
    pub(crate) fn form_batch(&mut self) -> Result<(), EngineError> {
        self.batch_starts.push(*self.pending_start);
        *self.pending_start = offset(self.triggers.len(), "trigger list")?;
        Ok(())
    }

    /// `st.batches[-1]`, or `None` without batches.
    pub(crate) fn top_batch(&self) -> Option<&[TriggerInst]> {
        let &start = self.batch_starts.last()?;
        Some(&self.triggers[start as usize..*self.pending_start as usize])
    }

    /// `st.batches[-1].pop(i)` (`game.py:707`, `:815`).
    ///
    /// # Errors
    /// Python's `IndexError` without batches or with `i` outside the top batch.
    pub(crate) fn take_from_top(&mut self, i: usize) -> Result<TriggerInst, EngineError> {
        let Some(&start) = self.batch_starts.last() else {
            return Err(EngineError::Index("list index out of range".to_owned()));
        };
        let at = start as usize + i;
        if at >= *self.pending_start as usize {
            return Err(EngineError::Index("pop index out of range".to_owned()));
        }
        *self.pending_start -= 1;
        #[cfg(feature = "paranoid")]
        {
            self.seqs.taken = self.seqs.queued.remove(at);
        }
        Ok(self.triggers.remove(at))
    }

    /// `st.batches.pop()` (`game.py:687`): drops the top batch with whatever it holds.
    pub(crate) fn pop_batch(&mut self) {
        if let Some(start) = self.batch_starts.pop() {
            let end = *self.pending_start as usize;
            self.triggers.drain(start as usize..end);
            #[cfg(feature = "paranoid")]
            self.seqs.queued.drain(start as usize..end);
            *self.pending_start = start;
        }
    }

    /// `st.batches = []; st.pending_triggers = []` (`game.py:769-770`).
    pub(crate) fn clear(&mut self) {
        self.triggers.clear();
        #[cfg(feature = "paranoid")]
        self.seqs.queued.clear();
        self.batch_starts.clear();
        *self.pending_start = 0;
    }
}

/// `st.lasting` and `st.delayed` with their shared `targets` arena (PLAN §4.3). Targets are
/// written once when an entry is created (Python's tuples are immutable), so the regions lie
/// in creation order, and the order-preserving filters compact the arena in place.
pub(crate) struct LastingArena<'a> {
    pub(crate) lasting: &'a mut Vec<Lasting>,
    pub(crate) delayed: &'a mut Vec<Lasting>,
    pub(crate) targets: &'a mut Vec<(Uid, i32)>,
}

#[allow(
    dead_code,
    reason = "called by layers L3-L5 (docs/dev/contracts.md); they land after L1"
)]
impl LastingArena<'_> {
    fn store(&mut self, mut l: Lasting, targets: &[(Uid, i32)]) -> Result<Lasting, EngineError> {
        l.targets = region(self.targets.len(), targets.len(), LASTING_TARGETS)?;
        self.targets.extend_from_slice(targets);
        Ok(l)
    }

    /// `st.lasting.append(Lasting(...))` (`interp.py:1303`); `l.targets` is ignored.
    pub(crate) fn push_lasting(
        &mut self,
        l: Lasting,
        targets: &[(Uid, i32)],
    ) -> Result<(), EngineError> {
        let l = self.store(l, targets)?;
        self.lasting.push(l);
        Ok(())
    }

    /// `st.delayed.append(le)` (`interp.py:1557`); `l.targets` is ignored.
    pub(crate) fn push_delayed(
        &mut self,
        l: Lasting,
        targets: &[(Uid, i32)],
    ) -> Result<(), EngineError> {
        let l = self.store(l, targets)?;
        self.delayed.push(l);
        Ok(())
    }

    /// `st.lasting = [le for le in st.lasting if keep(le)]` (`game.py:218`, `battle.py:246`,
    /// `game.py:870`).
    pub(crate) fn retain_lasting(&mut self, mut keep: impl FnMut(&Lasting, &[(Uid, i32)]) -> bool) {
        let targets = &*self.targets;
        self.lasting
            .retain(|l| keep(l, &targets[l.targets.range()]));
        self.compact();
    }

    /// `st.delayed = [le for le in st.delayed if keep(le)]` (`game.py:219`, `battle.py:251`).
    pub(crate) fn retain_delayed(&mut self, mut keep: impl FnMut(&Lasting, &[(Uid, i32)]) -> bool) {
        let targets = &*self.targets;
        self.delayed
            .retain(|l| keep(l, &targets[l.targets.range()]));
        self.compact();
    }

    /// Moves every live region left over the garbage, in creation (= start) order.
    fn compact(&mut self) {
        let live = self
            .lasting
            .iter()
            .chain(self.delayed.iter())
            .map(|l| l.targets.len as usize)
            .sum::<usize>();
        if live == self.targets.len() {
            return;
        }
        let (mut i, mut j, mut cursor) = (0, 0, 0usize);
        while i < self.lasting.len() || j < self.delayed.len() {
            let take_lasting = match (self.lasting.get(i), self.delayed.get(j)) {
                (Some(a), Some(b)) => a.targets.start <= b.targets.start,
                (Some(_), None) => true,
                _ => false,
            };
            let l = if take_lasting {
                i += 1;
                &mut self.lasting[i - 1]
            } else {
                j += 1;
                &mut self.delayed[j - 1]
            };
            let r = l.targets.range();
            self.targets.copy_within(r.clone(), cursor);
            l.targets = span_of(cursor, l.targets.len);
            cursor += r.len();
        }
        self.targets.truncate(cursor);
    }
}

// ---------------------------------------------------------------------------------------------
// The data a game shares among its clones

/// `listened_events(st)` (`engine/core.py:295-313`): the registry's `always_events`, plus
/// `events_by_def[d]` and the events of `d`'s Unit variant for every def_id `d` of the four
/// decklists. Python memoises it per decklist pair (`_LISTEN_CACHE`); the set is a pure
/// function of the registry and the decklists, so Rust computes it once per game
/// (`GameConst.listened`). The testkit recomputes it when it changes the decklists.
pub(crate) fn listened_events(
    reg: &Registry,
    decklists: &[Box<[DefId]>; 2],
    resource_decklists: &[Box<[DefId]>; 2],
) -> EvSet {
    let mut evs = reg.always_events();
    for dl in decklists.iter().chain(resource_decklists) {
        for &def in dl.iter() {
            evs = evs.union(reg.events_of(def));
            if let Some(variant) = reg.db().unit_variant(def) {
                evs = evs.union(reg.events_of(variant.def_id));
            }
        }
    }
    evs
}

impl EngineVars {
    /// The `VarId`s of the names the engine writes literally (`game.py:931`, `:941`,
    /// `interp.py:808`, `:1409`, `:1416`, `builtins.py:16`, `:68`); the exporter puts every one
    /// into the IR's `var_names` (PLAN §3.3).
    ///
    /// # Errors
    /// [`EngineError::Internal`] for a registry whose IR lacks one of them.
    pub(crate) fn of(reg: &Registry) -> Result<Self, EngineError> {
        let var = |name: &str| {
            reg.var_id(name).ok_or_else(|| {
                EngineError::Internal(format!(
                    "the effect registry has no frame variable {name:?} (IR var_names)"
                ))
            })
        };
        Ok(Self {
            card: var("card")?,
            pilot: var("pilot")?,
            unit: var("unit")?,
            todeck: var("__todeck")?,
            play: var("__play")?,
            looked: var("looked")?,
            t: var("t")?,
        })
    }
}

impl GameConst {
    /// The part of a game its clones share (`engine/state.py:561-562`): `reg` and its card
    /// database, the sorted decklists (`game.py:151-152`), `turn_limit` and `max_actions`,
    /// [`listened_events`], the registry's `hp_reduction_possible` (`core.py:868`) and
    /// [`EngineVars`]; no test overrides.
    ///
    /// # Errors
    /// [`EngineError::Internal`] for a def_id outside the registry's card database, or a
    /// registry without one of the engine's frame variables.
    pub(crate) fn new(
        reg: Arc<Registry>,
        decklists: [Box<[DefId]>; 2],
        resource_decklists: [Box<[DefId]>; 2],
        turn_limit: i32,
        max_actions: i64,
    ) -> Result<Self, EngineError> {
        let db = reg.db();
        if let Some(def) = decklists
            .iter()
            .chain(&resource_decklists)
            .flat_map(|dl| dl.iter())
            .find(|d| usize::from(d.0) >= db.len())
        {
            return Err(EngineError::Internal(format!(
                "def_id {} is not a definition of the card database",
                def.0
            )));
        }
        let listened = listened_events(&reg, &decklists, &resource_decklists);
        let vars = EngineVars::of(&reg)?;
        let hp_reduction_possible = reg.hp_reduction_possible();
        Ok(Self {
            reg,
            db,
            decklists,
            resource_decklists,
            turn_limit,
            max_actions,
            listened,
            hp_reduction_possible,
            vars,
            test_overrides: None,
        })
    }
}

// ---------------------------------------------------------------------------------------------
// Construction and the read-only accessors (docs/dev/contracts.md, gcg-engine public API)

impl GameState {
    /// `GameState(seed)` (`engine/state.py:475-511`); `turn_limit` and `max_actions` live in
    /// `konst`.
    #[allow(
        dead_code,
        reason = "called by L5 new_game and the testkit (docs/dev/contracts.md)"
    )]
    pub(crate) fn new(konst: Arc<GameConst>, seed: u64) -> Self {
        Self {
            seed,
            rng: SplitMix64::new(seed),
            cards: Vec::new(),
            zones: ZoneLists::new(),
            turn: 0,
            active: 0,
            first_player: 0,
            setup_chooser: 0,
            phase: Phase::Setup,
            step: Step::SetupChooseFirst,
            pending: None,
            pending_options: Vec::new(),
            battles: Vec::new(),
            next_battle_id: 1,
            next_lasting_serial: 0,
            priority: 0,
            passes: 0,
            lasting: Vec::new(),
            delayed: Vec::new(),
            lasting_targets: Vec::new(),
            triggers: Vec::new(),
            batch_starts: Vec::new(),
            pending_start: 0,
            #[cfg(feature = "paranoid")]
            trigger_seqs: crate::paranoid::TriggerSeqs::default(),
            frames: Vec::new(),
            frame_vars: Vec::new(),
            frame_ints: Vec::new(),
            frame_items: Vec::new(),
            once_used: Vec::new(),
            history: Vec::new(),
            winner: None,
            end_reason: None,
            konst,
            redraws: [false; 2],
            action_count: 0,
            event_group: 0,
            effect_hits: Vec::new(),
            version: 0,
            cache: RefCell::new(None),
            rules_checked: -1,
            history_epoch: 0,
        }
    }

    /// A state holding a decoded snapshot body (`canon::from_canonical`, PLAN §4.8), with no
    /// cached view and Python's `_rules_checked` from the snapshot. The body's decklists and
    /// limits are `konst`'s.
    pub(crate) fn from_body(konst: Arc<GameConst>, body: StateBody, rules_checked: i64) -> Self {
        let StateBody {
            seed,
            rng,
            cards,
            zones,
            turn,
            active,
            first_player,
            setup_chooser,
            phase,
            step,
            pending,
            pending_options,
            battles,
            next_battle_id,
            next_lasting_serial,
            priority,
            passes,
            lasting,
            delayed,
            lasting_targets,
            triggers,
            batch_starts,
            pending_start,
            frames,
            frame_vars,
            frame_ints,
            frame_items,
            once_used,
            history,
            winner,
            end_reason,
            decklists: _,
            resource_decklists: _,
            redraws,
            action_count,
            event_group,
            effect_hits,
            turn_limit: _,
            max_actions: _,
            version,
        } = body;
        #[cfg(feature = "paranoid")]
        let trigger_seqs = crate::paranoid::TriggerSeqs {
            queued: vec![crate::NO_ARG; triggers.len()],
            taken: crate::NO_ARG,
        };
        Self {
            seed,
            rng: SplitMix64::new(rng),
            cards,
            zones,
            turn,
            active,
            first_player,
            setup_chooser,
            phase,
            step,
            pending,
            pending_options,
            battles,
            next_battle_id,
            next_lasting_serial,
            priority,
            passes,
            lasting,
            delayed,
            lasting_targets,
            triggers,
            batch_starts,
            pending_start,
            #[cfg(feature = "paranoid")]
            trigger_seqs,
            frames,
            frame_vars,
            frame_ints,
            frame_items,
            once_used,
            history,
            winner,
            end_reason,
            konst,
            redraws,
            action_count,
            event_group,
            effect_hits,
            version,
            cache: RefCell::new(None),
            rules_checked,
            history_epoch: 0,
        }
    }

    /// `st.pending`: the header and the options in Python's order.
    #[must_use]
    pub fn pending(&self) -> Option<DecisionView<'_>> {
        self.pending.as_ref().map(|hdr| DecisionView {
            hdr,
            options: &self.pending_options,
        })
    }

    /// `st.cards`; the index is the uid.
    #[must_use]
    pub fn cards(&self) -> &[CardInstance] {
        &self.cards
    }

    /// `st.cards[uid]`.
    ///
    /// # Panics
    /// If `uid >= cards().len()` (an engine bug: uids come from the state).
    #[must_use]
    pub fn card(&self, uid: Uid) -> &CardInstance {
        &self.cards[usize::from(uid)]
    }

    /// `st.zone(p, z)` (`engine/state.py:514-515`).
    #[must_use]
    pub fn zone(&self, p: u8, z: Zone) -> &[Uid] {
        self.zones.get(p, z)
    }

    /// `st.seed`.
    #[must_use]
    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// `st.rng.state`.
    #[must_use]
    pub fn rng_state(&self) -> u64 {
        self.rng.state
    }

    #[must_use]
    pub fn turn(&self) -> i32 {
        self.turn
    }

    /// The turn player.
    #[must_use]
    pub fn active(&self) -> u8 {
        self.active
    }

    /// `1 - st.active` (`engine/state.py:517-519`).
    #[must_use]
    pub fn standby(&self) -> u8 {
        1 - self.active
    }

    #[must_use]
    pub fn first_player(&self) -> u8 {
        self.first_player
    }

    #[must_use]
    pub fn setup_chooser(&self) -> u8 {
        self.setup_chooser
    }

    #[must_use]
    pub fn phase(&self) -> Phase {
        self.phase
    }

    #[must_use]
    pub fn step(&self) -> Step {
        self.step
    }

    #[must_use]
    pub fn priority(&self) -> u8 {
        self.priority
    }

    #[must_use]
    pub fn passes(&self) -> i32 {
        self.passes
    }

    /// `st.battles[-1] if st.battles else None` (`engine/state.py:521-523`).
    #[must_use]
    pub fn battle(&self) -> Option<&Battle> {
        self.battles.last()
    }

    /// `st.battles`, a stack.
    #[must_use]
    pub fn battles(&self) -> &[Battle] {
        &self.battles
    }

    /// `st.frames`, bottom first.
    #[must_use]
    pub fn frames(&self) -> &[Frame] {
        &self.frames
    }

    /// `f.vars` of a frame of this state.
    #[must_use]
    pub fn frame_vars(&self, f: &Frame) -> FrameVars<'_> {
        FrameVars {
            entries: &self.frame_vars[f.vars.range()],
            items: &self.frame_items,
        }
    }

    /// `f.buf` of a frame of this state.
    #[must_use]
    pub fn frame_buf(&self, f: &Frame) -> &[i32] {
        &self.frame_items[f.buf.range()]
    }

    /// `f.ints` of a frame of this state, in insertion order.
    #[must_use]
    pub fn frame_ints(&self, f: &Frame) -> &[IntEntry] {
        &self.frame_ints[f.ints.range()]
    }

    /// `st.pending_triggers`.
    #[must_use]
    pub fn pending_triggers(&self) -> &[TriggerInst] {
        &self.triggers[self.pending_start as usize..]
    }

    /// `st.batches`, index 0 first.
    pub fn batches(&self) -> impl Iterator<Item = &[TriggerInst]> + '_ {
        (0..self.batch_starts.len()).map(move |i| {
            let start = self.batch_starts[i] as usize;
            &self.triggers[start..batch_end(&self.batch_starts, self.pending_start, i)]
        })
    }

    #[must_use]
    pub fn lasting(&self) -> &[Lasting] {
        &self.lasting
    }

    #[must_use]
    pub fn delayed(&self) -> &[Lasting] {
        &self.delayed
    }

    /// `le.targets` of an entry of `lasting()` or `delayed()`.
    #[must_use]
    pub fn lasting_targets(&self, l: &Lasting) -> &[(Uid, i32)] {
        &self.lasting_targets[l.targets.range()]
    }

    /// `st.once_used`, in insertion order (a Python set: membership is all that matters).
    #[must_use]
    pub fn once_used(&self) -> &[OnceKey] {
        &self.once_used
    }

    #[must_use]
    pub fn history(&self) -> &[HistoryEvent] {
        &self.history
    }

    /// Changes whenever `history` is replaced (the list identity of `runner/instrument.py:81`).
    #[must_use]
    pub fn history_epoch(&self) -> u32 {
        self.history_epoch
    }

    /// `st.winner`: a player, `-1` for a draw, `None` while the game runs.
    #[must_use]
    pub fn winner(&self) -> Option<i8> {
        self.winner
    }

    /// `st.winner is not None` (`engine/state.py:525-527`).
    #[must_use]
    pub fn game_over(&self) -> bool {
        self.winner.is_some()
    }

    #[must_use]
    pub fn end_reason(&self) -> Option<EndReason> {
        self.end_reason
    }

    /// `st.decklists`: per player, the sorted main-deck def_ids (`game.py:151`).
    #[must_use]
    pub fn decklists(&self) -> [&[DefId]; 2] {
        [&self.konst.decklists[0], &self.konst.decklists[1]]
    }

    /// `st.resource_decklists`, sorted (`game.py:152`).
    #[must_use]
    pub fn resource_decklists(&self) -> [&[DefId]; 2] {
        [
            &self.konst.resource_decklists[0],
            &self.konst.resource_decklists[1],
        ]
    }

    #[must_use]
    pub fn redraws(&self) -> [bool; 2] {
        self.redraws
    }

    /// Incremented by every `touch` (`engine/state.py:529-531`).
    #[must_use]
    pub fn version(&self) -> u64 {
        self.version
    }

    #[must_use]
    pub fn action_count(&self) -> i64 {
        self.action_count
    }

    #[must_use]
    pub fn event_group(&self) -> i64 {
        self.event_group
    }

    #[must_use]
    pub fn turn_limit(&self) -> i32 {
        self.konst.turn_limit
    }

    #[must_use]
    pub fn max_actions(&self) -> i64 {
        self.konst.max_actions
    }

    /// Python's `_rules_checked` (`state.py:511`, `core.py:861`).
    #[must_use]
    pub fn rules_checked(&self) -> i64 {
        self.rules_checked
    }

    /// The effect registry of this game.
    #[must_use]
    pub fn registry(&self) -> &Arc<Registry> {
        &self.konst.reg
    }

    /// The card database of this game.
    #[must_use]
    pub fn db(&self) -> &'static CardDb {
        self.konst.db
    }

    /// `cdef(st, uid)` (`engine/view.py:23-24`): the definition of the card's current
    /// `def_id` (a Unit variant while it is one, `interp.py:930`).
    ///
    /// # Panics
    /// If `uid >= cards().len()` (an engine bug: uids come from the state).
    #[must_use]
    pub fn def_of(&self, uid: Uid) -> &'static CardDef {
        self.konst.db.by_id(self.cards[usize::from(uid)].def_id)
    }

    /// `GameState.clone()` (`engine/state.py:534-572`) into `self`, reusing `self`'s buffers
    /// (PLAN §4.5): every arena is copied into its existing capacity, the cached view and
    /// `konst` are shared as Python shares `_derived` and the decklists, and `version`,
    /// `rules_checked` and `history_epoch` are copied. Observably equal to `src.clone()`.
    pub fn clone_from_state(&mut self, src: &Self) {
        fn copy<T: Copy>(dst: &mut Vec<T>, src: &[T]) {
            dst.clear();
            dst.extend_from_slice(src);
        }
        // Exhaustive on purpose: a new field fails to compile here until it is copied.
        let Self {
            seed,
            rng,
            cards,
            zones,
            turn,
            active,
            first_player,
            setup_chooser,
            phase,
            step,
            pending,
            pending_options,
            battles,
            next_battle_id,
            next_lasting_serial,
            priority,
            passes,
            lasting,
            delayed,
            lasting_targets,
            triggers,
            batch_starts,
            pending_start,
            #[cfg(feature = "paranoid")]
            trigger_seqs,
            frames,
            frame_vars,
            frame_ints,
            frame_items,
            once_used,
            history,
            winner,
            end_reason,
            konst,
            redraws,
            action_count,
            event_group,
            effect_hits,
            version,
            cache,
            rules_checked,
            history_epoch,
        } = src;
        self.seed = *seed;
        self.rng = *rng;
        copy(&mut self.cards, cards);
        self.zones.copy_from(zones);
        self.turn = *turn;
        self.active = *active;
        self.first_player = *first_player;
        self.setup_chooser = *setup_chooser;
        self.phase = *phase;
        self.step = *step;
        self.pending = *pending;
        copy(&mut self.pending_options, pending_options);
        copy(&mut self.battles, battles);
        self.next_battle_id = *next_battle_id;
        self.next_lasting_serial = *next_lasting_serial;
        self.priority = *priority;
        self.passes = *passes;
        copy(&mut self.lasting, lasting);
        copy(&mut self.delayed, delayed);
        copy(&mut self.lasting_targets, lasting_targets);
        copy(&mut self.triggers, triggers);
        copy(&mut self.batch_starts, batch_starts);
        self.pending_start = *pending_start;
        #[cfg(feature = "paranoid")]
        {
            copy(&mut self.trigger_seqs.queued, &trigger_seqs.queued);
            self.trigger_seqs.taken = trigger_seqs.taken;
        }
        copy(&mut self.frames, frames);
        copy(&mut self.frame_vars, frame_vars);
        copy(&mut self.frame_ints, frame_ints);
        copy(&mut self.frame_items, frame_items);
        copy(&mut self.once_used, once_used);
        copy(&mut self.history, history);
        self.winner = *winner;
        self.end_reason = *end_reason;
        if !Arc::ptr_eq(&self.konst, konst) {
            self.konst = Arc::clone(konst);
        }
        self.redraws = *redraws;
        self.action_count = *action_count;
        self.event_group = *event_group;
        copy(&mut self.effect_hits, effect_hits);
        self.version = *version;
        let old = std::mem::replace(self.cache.get_mut(), cache.borrow().clone());
        if let Some(old) = old {
            crate::view::recycle(old);
        }
        self.rules_checked = *rules_checked;
        self.history_epoch = *history_epoch;
    }
}

// ---------------------------------------------------------------------------------------------
// The internal layer boundary (docs/dev/contracts.md, "L1 (C), state.rs") and C's own helpers

#[allow(
    dead_code,
    reason = "called by layers L2-L5 (docs/dev/contracts.md internal boundary)"
)]
impl GameState {
    /// `st.touch()` (`engine/state.py:529-531`): `version += 1` and the cached view is dropped.
    /// Called exactly where Python calls it (PLAN §4.4).
    pub(crate) fn touch(&mut self) {
        self.version += 1;
        if let Some(old) = self.cache.get_mut().take() {
            crate::view::recycle(old);
        }
    }

    /// `st.once_used.add(key)`: nothing when an equal key is present (a Python set, PLAN §4.3).
    pub(crate) fn once_add(&mut self, key: OnceKey) {
        set_add(&mut self.once_used, key);
    }

    /// `key in st.once_used`.
    pub(crate) fn once_contains(&self, key: &OnceKey) -> bool {
        self.once_used.contains(key)
    }

    /// `st.once_used = set()` (`game.py:335`).
    pub(crate) fn once_clear(&mut self) {
        self.once_used.clear();
    }

    /// `st.pending = Decision(...)` (`interp.py:101`, `game.py:163`, ...).
    pub(crate) fn set_pending(&mut self, hdr: DecisionHdr, options: &[crate::Action]) {
        crate::diag::assert_live_decision(hdr.kind);
        self.pending = Some(hdr);
        self.pending_options.clear();
        self.pending_options.extend_from_slice(options);
    }

    /// `st.pending = None` (`game.py:787`).
    pub(crate) fn clear_pending(&mut self) {
        self.pending = None;
        self.pending_options.clear();
    }

    /// The frame arenas, for the frame helpers below and C's own frame code.
    pub(crate) fn frame_arena(&mut self) -> FrameArena<'_> {
        FrameArena {
            frames: &mut self.frames,
            vars: &mut self.frame_vars,
            ints: &mut self.frame_ints,
            items: &mut self.frame_items,
        }
    }

    /// `st.frames.append(f)` (`interp.py:87`); `f`'s spans are ignored. Returns its index.
    pub(crate) fn frame_push(
        &mut self,
        f: Frame,
        vars: &[(VarId, &[i32])],
    ) -> Result<usize, EngineError> {
        self.frame_arena().push(f, vars)
    }

    /// `st.frames.pop()` (`interp.py:131`); the returned frame's spans are empty.
    pub(crate) fn frame_pop(&mut self) -> Option<Frame> {
        self.frame_arena().pop()
    }

    /// `st.frames[i]` for its scalar fields (`pc`, `did`, `waiting`, ...); never its spans.
    pub(crate) fn frame_mut(&mut self, i: usize) -> &mut Frame {
        &mut self.frames[i]
    }

    /// `st.frames[frame].vars.get(var)`.
    pub(crate) fn var_get(&self, frame: usize, var: VarId) -> Option<&[i32]> {
        let f = &self.frames[frame];
        let items = &self.frame_items;
        self.frame_vars[f.vars.range()]
            .iter()
            .find(|e| e.var == var)
            .map(|e| &items[e.items.range()])
    }

    /// `f.vars[var] = tuple(items)`.
    pub(crate) fn var_set(
        &mut self,
        frame: usize,
        var: VarId,
        items: &[i32],
    ) -> Result<(), EngineError> {
        self.frame_arena().var_set(frame, var, items)
    }

    /// `del f.vars[var]` (`interp.py:828`); nothing when absent.
    pub(crate) fn var_del(&mut self, frame: usize, var: VarId) {
        self.frame_arena().var_del(frame, var);
    }

    /// `f.ints.get(key)`.
    pub(crate) fn int_get(&self, frame: usize, key: IntKey) -> Option<i32> {
        let f = &self.frames[frame];
        self.frame_ints[f.ints.range()]
            .iter()
            .find_map(|e| (e.key == key).then_some(e.value))
    }

    /// `f.ints[key] = value`.
    pub(crate) fn int_set(
        &mut self,
        frame: usize,
        key: IntKey,
        value: i32,
    ) -> Result<(), EngineError> {
        self.frame_arena().int_set(frame, key, value)
    }

    /// `del f.ints[key]` / `f.ints.pop(key, None)`.
    pub(crate) fn int_del(&mut self, frame: usize, key: IntKey) -> Option<i32> {
        self.frame_arena().int_del(frame, key)
    }

    /// `f.buf`.
    pub(crate) fn buf(&self, frame: usize) -> &[i32] {
        &self.frame_items[self.frames[frame].buf.range()]
    }

    /// `f.buf = [...]`.
    pub(crate) fn buf_set(&mut self, frame: usize, items: &[i32]) -> Result<(), EngineError> {
        self.frame_arena().buf_set(frame, items)
    }

    /// `f.buf.append(x)`.
    pub(crate) fn buf_push(&mut self, frame: usize, x: i32) -> Result<(), EngineError> {
        self.frame_arena().buf_push(frame, x)
    }

    /// The lasting-effect arenas, for C's order-preserving filters.
    pub(crate) fn lasting_arena(&mut self) -> LastingArena<'_> {
        LastingArena {
            lasting: &mut self.lasting,
            delayed: &mut self.delayed,
            targets: &mut self.lasting_targets,
        }
    }

    /// `st.lasting.append(...)` (`interp.py:1303`); `l.targets` is ignored.
    pub(crate) fn lasting_push(
        &mut self,
        l: Lasting,
        targets: &[(Uid, i32)],
    ) -> Result<(), EngineError> {
        self.lasting_arena().push_lasting(l, targets)
    }

    /// `st.delayed.append(le)` (`interp.py:1557`); `l.targets` is ignored.
    pub(crate) fn delayed_push(
        &mut self,
        l: Lasting,
        targets: &[(Uid, i32)],
    ) -> Result<(), EngineError> {
        self.lasting_arena().push_delayed(l, targets)
    }

    /// The trigger list, for C's `queue_trigger`, `_form_batch` and `_start_next_trigger`.
    pub(crate) fn trigger_arena(&mut self) -> TriggerArena<'_> {
        TriggerArena {
            triggers: &mut self.triggers,
            batch_starts: &mut self.batch_starts,
            pending_start: &mut self.pending_start,
            #[cfg(feature = "paranoid")]
            seqs: &mut self.trigger_seqs,
            #[cfg(feature = "paranoid")]
            cards: &self.cards,
        }
    }

    /// `st.effect_hits[uid] = source` (`core.py:610`): an existing key keeps its position.
    pub(crate) fn effect_hit_set(&mut self, uid: Uid, source: Uid) {
        if let Some(e) = self.effect_hits.iter_mut().find(|e| e.0 == uid) {
            e.1 = source;
        } else {
            self.effect_hits.push((uid, source));
        }
    }

    /// `st.effect_hits.get(uid)`.
    pub(crate) fn effect_hit_get(&self, uid: Uid) -> Option<Uid> {
        self.effect_hits
            .iter()
            .find_map(|&(u, s)| (u == uid).then_some(s))
    }

    /// `st.effect_hits.clear()` (`core.py:874`, `:885`).
    pub(crate) fn effect_hits_clear(&mut self) {
        self.effect_hits.clear();
    }

    /// `st.history = []` (`game.py:190`, `:336`): a new list identity.
    pub(crate) fn history_reset(&mut self) {
        self.history.clear();
        self.history_epoch = self.history_epoch.wrapping_add(1);
    }

    /// The `VarId`s of the engine's literal frame-variable names.
    pub(crate) fn engine_vars(&self) -> &EngineVars {
        &self.konst.vars
    }

    /// `force_link` for `view::link_satisfied` (test overrides only).
    pub(crate) fn test_overrides(&self) -> Option<&TestOverrides> {
        self.konst.test_overrides.as_deref()
    }

    /// `zones[p][z].remove(uid)` without a zone change (`interp.py:1194-1202`,
    /// `builtins.py:20-26`); no touch, as in Python.
    pub(crate) fn zone_remove(&mut self, p: u8, z: Zone, uid: Uid) -> Result<(), EngineError> {
        self.zones.remove(p, z, uid)
    }

    /// `zones[p][z].extend(uids)`; no touch.
    pub(crate) fn zone_extend(&mut self, p: u8, z: Zone, uids: &[Uid]) -> Result<(), EngineError> {
        self.zones.extend(p, z, uids)
    }

    /// `zones[p][z][0:0] = uids`; no touch.
    pub(crate) fn zone_insert_front(
        &mut self,
        p: u8,
        z: Zone,
        uids: &[Uid],
    ) -> Result<(), EngineError> {
        self.zones.insert_front(p, z, uids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AbilityKey, EventPayload, FrameKind, NO_ARG};
    use gcg_core::prop;
    use gcg_effects::{ContRef, Duration, ProgramId};

    fn frame(host: i32) -> Frame {
        Frame {
            program: ProgramId(1),
            controller: 0,
            host,
            host_seq: 0,
            card_uid: host,
            kind: FrameKind::Trigger,
            event: EventPayload::default(),
            pc: 0,
            vars: Span::EMPTY,
            did: true,
            answer: false,
            waiting: false,
            buf: Span::EMPTY,
            ints: Span::EMPTY,
            once_key: None,
            acted: false,
        }
    }

    /// Python's `Frame.vars`/`ints`/`buf` as a list model: insertion-ordered dicts and a list.
    #[derive(Clone, Default, PartialEq, Debug)]
    struct ModelFrame {
        vars: Vec<(VarId, Vec<i32>)>,
        ints: Vec<(IntKey, i32)>,
        buf: Vec<i32>,
    }

    #[derive(Default)]
    struct Arenas {
        frames: Vec<Frame>,
        vars: Vec<VarEntry>,
        ints: Vec<IntEntry>,
        items: Vec<i32>,
    }

    impl Arenas {
        fn arena(&mut self) -> FrameArena<'_> {
            FrameArena {
                frames: &mut self.frames,
                vars: &mut self.vars,
                ints: &mut self.ints,
                items: &mut self.items,
            }
        }

        fn view(&self) -> Vec<ModelFrame> {
            self.frames
                .iter()
                .map(|f| ModelFrame {
                    vars: self.vars[f.vars.range()]
                        .iter()
                        .map(|e| (e.var, self.items[e.items.range()].to_vec()))
                        .collect(),
                    ints: self.ints[f.ints.range()]
                        .iter()
                        .map(|e| (e.key, e.value))
                        .collect(),
                    buf: self.items[f.buf.range()].to_vec(),
                })
                .collect()
        }
    }

    #[test]
    fn push_copies_vars_like_dict_and_pop_frees_the_tail() {
        let mut a = Arenas::default();
        let i = a
            .arena()
            .push(
                frame(3),
                &[(VarId(2), &[1, 2]), (VarId(5), &[]), (VarId(2), &[9])],
            )
            .unwrap();
        assert_eq!(i, 0);
        assert_eq!(
            a.view()[0].vars,
            vec![(VarId(2), vec![9]), (VarId(5), vec![])]
        );
        a.arena().buf_push(0, 4).unwrap();
        let j = a.arena().push(frame(4), &[(VarId(1), &[7])]).unwrap();
        a.arena().int_set(j, IntKey::RestI, 2).unwrap();
        a.arena().buf_set(j, &[5, 6]).unwrap();
        // The lower frame grows while the top one sits at the tail: its buf is relocated.
        a.arena().buf_push(0, 8).unwrap();
        assert_eq!(a.view()[0].buf, vec![4, 8]);
        assert_eq!(a.view()[1].buf, vec![5, 6]);
        let top = a.arena().pop().unwrap();
        assert_eq!(
            (top.host, top.vars, top.buf, top.ints),
            (4, Span::EMPTY, Span::EMPTY, Span::EMPTY)
        );
        assert_eq!(a.view().len(), 1);
        assert_eq!(a.view()[0].buf, vec![4, 8]);
        assert!(a.ints.is_empty());
        a.arena().pop().unwrap();
        assert!(a.vars.is_empty() && a.items.is_empty() && a.frames.is_empty());
        assert!(a.arena().pop().is_none());
    }

    #[test]
    fn ints_and_vars_delete_and_keep_order() {
        let mut a = Arenas::default();
        a.arena().push(frame(0), &[]).unwrap();
        a.arena().int_set(0, IntKey::HoldRules, 1).unwrap();
        a.arena().int_set(0, IntKey::RestAs(10), 7).unwrap();
        a.arena().int_set(0, IntKey::RestAs(2), 8).unwrap();
        a.arena().int_set(0, IntKey::HoldRules, 3).unwrap();
        assert_eq!(a.arena().int_del(0, IntKey::RestAs(10)), Some(7));
        assert_eq!(a.arena().int_del(0, IntKey::RestAs(10)), None);
        assert_eq!(
            a.view()[0].ints,
            vec![(IntKey::HoldRules, 3), (IntKey::RestAs(2), 8)]
        );
        a.arena().var_set(0, VarId(1), &[1]).unwrap();
        a.arena().var_set(0, VarId(2), &[2, 2]).unwrap();
        a.arena().var_set(0, VarId(1), &[1, 1, 1]).unwrap();
        a.arena().var_del(0, VarId(9));
        a.arena().var_del(0, VarId(1));
        assert_eq!(a.view()[0].vars, vec![(VarId(2), vec![2, 2])]);
    }

    #[test]
    fn popping_compacts_garbage_left_by_relocations() {
        let mut a = Arenas::default();
        a.arena().push(frame(0), &[(VarId(0), &[1])]).unwrap();
        a.arena().push(frame(1), &[(VarId(1), &[2])]).unwrap();
        // The lower frame's growing var is relocated past the top frame's region each time.
        for n in 1..40 {
            let data: Vec<i32> = (0..n).collect();
            a.arena().var_set(0, VarId(0), &data).unwrap();
            a.arena().buf_push(0, n).unwrap();
        }
        a.arena().int_set(0, IntKey::ExUsed, 2).unwrap();
        let before = a.view()[0].clone();
        assert!(a.items.len() > 1000);
        a.arena().pop().unwrap();
        assert_eq!(a.view(), std::slice::from_ref(&before));
        let live = before.vars.iter().map(|(_, d)| d.len()).sum::<usize>() + before.buf.len();
        assert_eq!(a.items.len(), live);
        assert_eq!((a.vars.len(), a.ints.len()), (1, 1));
    }

    /// T-058 (frame part): random frame programs agree with Python-like frames.
    #[test]
    fn random_frame_edits_agree_with_a_dict_model() {
        prop::check("frame-arena-vs-model", 300, |rng| {
            let mut a = Arenas::default();
            let mut model: Vec<ModelFrame> = Vec::new();
            let mut peak = 0usize;
            for _ in 0..120 {
                let n = model.len();
                let var = VarId(u16::try_from(rng.randrange(4)).unwrap());
                let key = match rng.randrange(4) {
                    0 => IntKey::HoldRules,
                    1 => IntKey::RestI,
                    2 => IntKey::RestAs(u16::try_from(rng.randrange(3)).unwrap()),
                    _ => IntKey::Loop(var),
                };
                let data: Vec<i32> = (0..rng.randrange(4))
                    .map(|k| i32::try_from(k * 3).unwrap())
                    .collect();
                let i = if n == 0 { 0 } else { rng.randrange_usize(n) };
                let op = if n == 0 { 0 } else { rng.randrange(9) };
                let err = |e: EngineError| format!("{e:?}");
                match op {
                    0 if n < 6 => {
                        let vars: Vec<(VarId, Vec<i32>)> = (0..rng.randrange(3))
                            .map(|k| (VarId(u16::try_from(k).unwrap()), data.clone()))
                            .collect();
                        let refs: Vec<(VarId, &[i32])> =
                            vars.iter().map(|(v, d)| (*v, d.as_slice())).collect();
                        a.arena().push(frame(0), &refs).map_err(err)?;
                        model.push(ModelFrame {
                            vars,
                            ..ModelFrame::default()
                        });
                    }
                    0 | 1 => {
                        a.arena().pop();
                        model.pop();
                    }
                    2 => {
                        a.arena().var_set(i, var, &data).map_err(err)?;
                        match model[i].vars.iter_mut().find(|(v, _)| *v == var) {
                            Some(slot) => slot.1.clone_from(&data),
                            None => model[i].vars.push((var, data)),
                        }
                    }
                    3 => {
                        a.arena().var_del(i, var);
                        model[i].vars.retain(|(v, _)| *v != var);
                    }
                    4 => {
                        let value = i32::try_from(rng.randrange(50)).unwrap();
                        a.arena().int_set(i, key, value).map_err(err)?;
                        match model[i].ints.iter_mut().find(|(k, _)| *k == key) {
                            Some(slot) => slot.1 = value,
                            None => model[i].ints.push((key, value)),
                        }
                    }
                    5 => {
                        let got = a.arena().int_del(i, key);
                        let pos = model[i].ints.iter().position(|(k, _)| *k == key);
                        let want = pos.map(|p| model[i].ints.remove(p).1);
                        if got != want {
                            return Err(format!("int_del {got:?} != {want:?}"));
                        }
                    }
                    6 => {
                        a.arena().buf_set(i, &data).map_err(err)?;
                        model[i].buf.clone_from(&data);
                    }
                    _ => {
                        a.arena().buf_push(i, 99).map_err(err)?;
                        model[i].buf.push(99);
                    }
                }
                if a.view() != model {
                    return Err(format!("arena {:?} != model {model:?}", a.view()));
                }
                peak = peak.max(a.items.len());
            }
            if model.is_empty() && !(a.vars.is_empty() && a.ints.is_empty() && a.items.is_empty()) {
                return Err("an empty stack left arena contents".into());
            }
            if peak > 2000 {
                return Err(format!("items arena grew to {peak}"));
            }
            Ok(())
        });
    }

    /// T-150: `once_add` behaves as `set.add`, against a sorted-set model.
    #[test]
    fn once_add_is_set_add() {
        prop::check("once-add-vs-set", 200, |rng| {
            let mut keys: Vec<OnceKey> = Vec::new();
            let mut model = std::collections::BTreeSet::new();
            for _ in 0..40 {
                let k = [rng.randrange(3), rng.randrange(3)].map(|x| i32::try_from(x).unwrap());
                let key = crate::OnceKey::from_elems(&k.map(crate::OnceElem::Int)).unwrap();
                set_add(&mut keys, key);
                model.insert(k);
                let mut sorted: Vec<[i32; 2]> = keys
                    .iter()
                    .map(|k| match k.elems() {
                        [crate::OnceElem::Int(a), crate::OnceElem::Int(b)] => [*a, *b],
                        other => panic!("{other:?}"),
                    })
                    .collect();
                sorted.sort();
                if sorted != model.iter().copied().collect::<Vec<_>>() {
                    return Err(format!("{sorted:?} != {model:?}"));
                }
            }
            Ok(())
        });
    }

    fn trigger(host: i32) -> TriggerInst {
        TriggerInst {
            program: ProgramId(0),
            controller: 0,
            host,
            host_seq: 0,
            card_uid: host,
            ability_key: AbilityKey {
                def_id: -1,
                aid: -1,
                unit_text: false,
            },
            event: EventPayload::default(),
            burst: false,
            once_key: None,
            optional: false,
        }
    }

    #[derive(Default)]
    struct Triggers {
        triggers: Vec<TriggerInst>,
        batch_starts: Vec<u32>,
        pending_start: u32,
        #[cfg(feature = "paranoid")]
        seqs: crate::paranoid::TriggerSeqs,
    }

    impl Triggers {
        fn arena(&mut self) -> TriggerArena<'_> {
            TriggerArena {
                triggers: &mut self.triggers,
                batch_starts: &mut self.batch_starts,
                pending_start: &mut self.pending_start,
                #[cfg(feature = "paranoid")]
                seqs: &mut self.seqs,
                #[cfg(feature = "paranoid")]
                cards: &[],
            }
        }

        /// `(st.batches, st.pending_triggers)` as host lists.
        fn view(&self) -> (Vec<Vec<i32>>, Vec<i32>) {
            let batches = (0..self.batch_starts.len())
                .map(|i| {
                    let a = self.batch_starts[i] as usize;
                    self.triggers[a..batch_end(&self.batch_starts, self.pending_start, i)]
                        .iter()
                        .map(|t| t.host)
                        .collect()
                })
                .collect();
            let pending = self.triggers[self.pending_start as usize..]
                .iter()
                .map(|t| t.host)
                .collect();
            (batches, pending)
        }
    }

    #[test]
    fn trigger_batches_follow_python_lists() {
        let mut t = Triggers::default();
        // st.batches = [[]] is representable.
        t.arena().form_batch().unwrap();
        assert_eq!(t.view(), (vec![vec![]], vec![]));
        for h in [1, 2, 3] {
            t.arena().push_pending(trigger(h)).unwrap();
        }
        t.arena().form_batch().unwrap();
        t.arena().push_pending(trigger(4)).unwrap();
        assert_eq!(t.view(), (vec![vec![], vec![1, 2, 3]], vec![4]));
        assert_eq!(t.arena().top_batch().map(<[_]>::len), Some(3));
        assert_eq!(t.arena().take_from_top(1).unwrap().host, 2);
        assert!(t.arena().take_from_top(2).is_err());
        assert_eq!(t.view(), (vec![vec![], vec![1, 3]], vec![4]));
        t.arena().pop_batch();
        assert_eq!(t.view(), (vec![vec![]], vec![4]));
        t.arena().pop_batch();
        assert_eq!(t.view(), (vec![], vec![4]));
        t.arena().pop_batch();
        assert_eq!(t.view(), (vec![], vec![4]));
        t.arena().clear();
        assert_eq!(t.view(), (vec![], vec![]));
    }

    fn entry(source: i32, duration: Duration) -> Lasting {
        Lasting {
            effect: ContRef::StatMod { ap: 1, hp: 0 },
            controller: 0,
            source_uid: source,
            targets: Span::EMPTY,
            duration,
            created_turn: 1,
            battle_id: NO_ARG,
            player: -1,
            uses: 0,
            filters: None,
            aux: NO_ARG,
            serial: -1,
        }
    }

    #[test]
    fn lasting_filters_keep_order_and_compact_targets() {
        let mut ls = Vec::new();
        let mut ds = Vec::new();
        let mut targets = Vec::new();
        let mut a = LastingArena {
            lasting: &mut ls,
            delayed: &mut ds,
            targets: &mut targets,
        };
        a.push_lasting(entry(1, Duration::ThisTurn), &[(5, 1), (6, 2)])
            .unwrap();
        a.push_delayed(entry(2, Duration::ThisTurn), &[(7, 1)])
            .unwrap();
        a.push_lasting(entry(3, Duration::WhileOnField), &[(8, 3)])
            .unwrap();
        a.push_delayed(entry(4, Duration::WhileOnField), &[])
            .unwrap();
        a.push_lasting(entry(5, Duration::WhileOnField), &[(9, 4), (10, 5)])
            .unwrap();
        a.retain_lasting(|l, _| l.duration != Duration::ThisTurn);
        a.retain_delayed(|l, t| l.duration != Duration::ThisTurn && t.is_empty());
        let got: Vec<(i32, Vec<(Uid, i32)>)> = ls
            .iter()
            .chain(ds.iter())
            .map(|l| (l.source_uid, targets[l.targets.range()].to_vec()))
            .collect();
        assert_eq!(
            got,
            vec![(3, vec![(8, 3)]), (5, vec![(9, 4), (10, 5)]), (4, vec![])]
        );
        assert_eq!(targets.len(), 3);
    }

    // ---------------------------------------------------------------------------------------
    // Whole states (T-058): the Python fixture snapshots rebuilt by `canon::from_canonical`.

    use serde_json::Value;

    use crate::Derived;
    use crate::canon::{self, fixtures};

    fn int(v: &Value) -> i64 {
        v.as_i64().unwrap()
    }

    /// A cached view to watch `cache` sharing (its contents are never read here).
    fn a_view() -> Arc<Derived> {
        Arc::new(Derived::unallocated())
    }

    /// Everything a state holds, serialised or not, that `clone` must copy or share.
    fn same_state(a: &GameState, b: &GameState) -> Result<(), String> {
        let (ja, jb) = (canon::canonical_state(a), canon::canonical_state(b));
        if ja != jb {
            return Err("canonical states differ".into());
        }
        let cache = |s: &GameState| s.cache.borrow().clone();
        let shared = match (cache(a), cache(b)) {
            (None, None) => true,
            (Some(x), Some(y)) => Arc::ptr_eq(&x, &y),
            _ => false,
        };
        let same = shared
            && Arc::ptr_eq(&a.konst, &b.konst)
            && a.rules_checked == b.rules_checked
            && a.history_epoch == b.history_epoch
            && a.version == b.version
            && a.rng == b.rng;
        same.then_some(())
            .ok_or_else(|| "a field outside the canonical form differs".into())
    }

    #[test]
    fn accessors_read_the_snapshot() {
        for (rec, st) in fixtures::states() {
            let id = rec["id"].as_str().unwrap();
            let s = &rec["state"];
            assert_eq!(st.seed(), s["seed"].as_u64().unwrap(), "{id}");
            assert_eq!(st.rng_state(), s["rng"].as_u64().unwrap(), "{id}");
            let cards = s["cards"].as_array().unwrap();
            assert_eq!(st.cards().len(), cards.len(), "{id}");
            for (u, c) in cards.iter().enumerate() {
                let uid = Uid::try_from(u).unwrap();
                let card = st.card(uid);
                assert_eq!(i64::from(card.def_id.0), int(&c[1]), "{id} card {u}");
                assert_eq!(i64::from(card.owner), int(&c[2]), "{id} card {u}");
                assert_eq!(i64::from(card.zone.value()), int(&c[3]), "{id} card {u}");
                assert_eq!(i64::from(card.rested), int(&c[4]), "{id} card {u}");
                assert_eq!(
                    [card.damage, card.pair, card.zone_seq, card.entered_turn].map(i64::from),
                    [int(&c[5]), int(&c[6]), int(&c[7]), int(&c[8])],
                    "{id} card {u}"
                );
                assert_eq!(i64::from(card.known), int(&c[9]), "{id} card {u}");
                assert!(std::ptr::eq(st.def_of(uid), st.db().by_id(card.def_id)));
            }
            for p in 0..2u8 {
                for (zi, z) in Zone::ALL.iter().enumerate() {
                    let want: Vec<i64> = s["zones"][usize::from(p)][zi]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(int)
                        .collect();
                    let got: Vec<i64> = st.zone(p, *z).iter().map(|&u| i64::from(u)).collect();
                    assert_eq!(got, want, "{id} zone {p} {z:?}");
                }
            }
            assert_eq!(i64::from(st.turn()), int(&s["turn"]), "{id}");
            assert_eq!(i64::from(st.active()), int(&s["active"]), "{id}");
            assert_eq!(st.standby(), 1 - st.active(), "{id}");
            assert_eq!(i64::from(st.first_player()), int(&s["first_player"]));
            assert_eq!(i64::from(st.setup_chooser()), int(&s["setup_chooser"]));
            assert_eq!(i64::from(st.phase().value()), int(&s["phase"]), "{id}");
            assert_eq!(i64::from(st.step().value()), int(&s["step"]), "{id}");
            assert_eq!(i64::from(st.priority()), int(&s["priority"]), "{id}");
            assert_eq!(i64::from(st.passes()), int(&s["passes"]), "{id}");
            let battles = s["battles"].as_array().unwrap();
            assert_eq!(st.battles().len(), battles.len(), "{id}");
            assert_eq!(st.battle(), st.battles().last());
            if let Some(b) = st.battle() {
                assert_eq!(i64::from(b.battle_id), int(&battles[battles.len() - 1][5]));
            }
            let frames = s["frames"].as_array().unwrap();
            assert_eq!(st.frames().len(), frames.len(), "{id}");
            for (f, j) in st.frames().iter().zip(frames) {
                let buf: Vec<i64> = st.frame_buf(f).iter().map(|&x| i64::from(x)).collect();
                let want: Vec<i64> = j["buf"].as_array().unwrap().iter().map(int).collect();
                assert_eq!(buf, want, "{id} frame buf");
                let vars = j["vars"].as_object().unwrap();
                assert_eq!(st.frame_vars(f).iter().count(), vars.len(), "{id}");
                for (name, items) in vars {
                    let v = st.registry().var_id(name).unwrap();
                    let got: Vec<i64> = st
                        .frame_vars(f)
                        .get(v)
                        .unwrap()
                        .iter()
                        .map(|&x| i64::from(x))
                        .collect();
                    let want: Vec<i64> = items.as_array().unwrap().iter().map(int).collect();
                    assert_eq!(got, want, "{id} var {name}");
                }
                assert_eq!(
                    st.frame_ints(f).len(),
                    j["ints"].as_object().unwrap().len(),
                    "{id}"
                );
                assert_eq!(i64::from(f.pc), int(&j["pc"]), "{id}");
            }
            let pending = s["pending_triggers"].as_array().unwrap().len();
            assert_eq!(st.pending_triggers().len(), pending, "{id}");
            let batches: Vec<usize> = s["batches"]
                .as_array()
                .unwrap()
                .iter()
                .map(|b| b.as_array().unwrap().len())
                .collect();
            assert_eq!(
                st.batches().map(<[TriggerInst]>::len).collect::<Vec<_>>(),
                batches,
                "{id}"
            );
            for (name, list) in [("lasting", st.lasting()), ("delayed", st.delayed())] {
                let want = s[name].as_array().unwrap();
                assert_eq!(list.len(), want.len(), "{id} {name}");
                for (l, j) in list.iter().zip(want) {
                    let targets: Vec<(i64, i64)> = st
                        .lasting_targets(l)
                        .iter()
                        .map(|&(u, q)| (i64::from(u), i64::from(q)))
                        .collect();
                    let want: Vec<(i64, i64)> = j[3]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|t| (int(&t[0]), int(&t[1])))
                        .collect();
                    assert_eq!(targets, want, "{id} {name} targets");
                }
            }
            assert_eq!(
                st.once_used().len(),
                s["once_used"].as_array().unwrap().len()
            );
            assert_eq!(st.history().len(), s["history"].as_array().unwrap().len());
            assert_eq!(st.history_epoch(), 0);
            assert_eq!(st.winner().map(i64::from), s["winner"].as_i64(), "{id}");
            assert_eq!(st.game_over(), !s["winner"].is_null(), "{id}");
            assert_eq!(
                st.end_reason().map(EndReason::as_str),
                s["end_reason"].as_str(),
                "{id}"
            );
            for (name, lists) in [
                ("decklists", st.decklists()),
                ("resource_decklists", st.resource_decklists()),
            ] {
                for p in 0..2 {
                    let want: Vec<i64> = s[name][p].as_array().unwrap().iter().map(int).collect();
                    let got: Vec<i64> = lists[p].iter().map(|d| i64::from(d.0)).collect();
                    assert_eq!(got, want, "{id} {name}");
                }
            }
            assert_eq!(
                st.redraws().map(Value::Bool).to_vec(),
                *s["redraws"].as_array().unwrap()
            );
            assert_eq!(st.version(), s["version"].as_u64().unwrap(), "{id}");
            assert_eq!(st.action_count(), int(&s["action_count"]), "{id}");
            assert_eq!(st.event_group(), int(&s["event_group"]), "{id}");
            assert_eq!(i64::from(st.turn_limit()), int(&s["turn_limit"]), "{id}");
            assert_eq!(st.max_actions(), int(&s["max_actions"]), "{id}");
            assert_eq!(st.rules_checked(), int(&rec["rules_checked"]), "{id}");
            assert!(Arc::ptr_eq(st.registry(), fixtures::registry()));
            assert!(std::ptr::eq(st.db(), st.registry().db()));
            match (st.pending(), s["pending"].as_object()) {
                (None, None) => {}
                (Some(d), Some(j)) => {
                    assert_eq!(i64::from(d.hdr.player), int(&j["player"]), "{id}");
                    assert_eq!(d.hdr.kind.as_str(), j["kind"], "{id}");
                    assert_eq!(
                        d.hdr.prompt.text(st.registry()).as_ref(),
                        j["prompt"],
                        "{id}"
                    );
                    let want: Vec<Value> = j["options"].as_array().unwrap().clone();
                    let got: Vec<Value> = d
                        .options
                        .iter()
                        .map(|a| serde_json::json!([a.kind.as_str(), a.a, a.b, a.c]))
                        .collect();
                    assert_eq!(got, want, "{id}");
                }
                (a, b) => panic!("{id}: pending {a:?} vs {b:?}"),
            }
            assert!(st.cache.borrow().is_none(), "no cached view after a load");
        }
    }

    #[test]
    fn touch_bumps_the_version_and_drops_the_cached_view() {
        for (rec, mut st) in fixtures::states() {
            let before = canon::canonical_state(&st);
            *st.cache.get_mut() = Some(a_view());
            st.touch();
            assert_eq!(st.version(), rec["state"]["version"].as_u64().unwrap() + 1);
            assert!(st.cache.borrow().is_none());
            let mut after = canon::canonical_state(&st);
            after.set("version", before.get("version").unwrap().clone());
            assert_eq!(after, before, "touch changes nothing but version");
            st.touch();
            assert_eq!(st.version(), rec["state"]["version"].as_u64().unwrap() + 2);
        }
    }

    #[test]
    fn clone_from_equals_clone() {
        let states = fixtures::states();
        let n = states.len();
        prop::check("clone-from-equals-clone", 300, |rng| {
            let (_, src) = &states[rng.randrange_usize(n)];
            let mut src = src.clone();
            if rng.randrange(2) == 0 {
                *src.cache.get_mut() = Some(a_view());
            }
            // A recycled destination of another size, possibly with a view of its own.
            let mut dst = states[rng.randrange_usize(n)].1.clone();
            if rng.randrange(2) == 0 {
                *dst.cache.get_mut() = Some(a_view());
            }
            for _ in 0..rng.randrange(3) {
                dst.touch();
            }
            dst.history_reset();
            dst.clone_from_state(&src);
            let copy = src.clone();
            same_state(&src, &copy)?;
            same_state(&src, &dst)?;
            // Value semantics (W-52): writing the copy leaves the source alone.
            let digest = canon::state_digest(&src);
            let version = src.version();
            let had_view = src.cache.borrow().is_some();
            dst.touch();
            dst.cards[0].damage += 1;
            dst.once_clear();
            dst.history_reset();
            let mut copy = copy;
            copy.touch();
            copy.frame_arena().pop();
            if canon::state_digest(&src) != digest
                || src.version() != version
                || src.cache.borrow().is_some() != had_view
            {
                return Err("writing a clone changed its source".into());
            }
            Ok(())
        });
    }

    #[test]
    fn clones_share_the_game_constants_and_the_cached_view() {
        let (_, mut st) = fixtures::states().swap_remove(0);
        *st.cache.get_mut() = Some(a_view());
        let copy = st.clone();
        let (Some(a), Some(b)) = (st.cache.borrow().clone(), copy.cache.borrow().clone()) else {
            panic!("a clone keeps the cached view");
        };
        assert!(Arc::ptr_eq(&a, &b), "state.py:570 shares _derived");
        assert!(Arc::ptr_eq(&st.konst, &copy.konst));
        let mut copy = copy;
        copy.touch();
        assert!(copy.cache.borrow().is_none());
        assert!(
            st.cache.borrow().is_some(),
            "touch drops only its own state's view"
        );
    }

    #[test]
    fn game_constants_listen_to_the_decklists_events() {
        let reg = fixtures::registry();
        let (_, st) = fixtures::states().swap_remove(0);
        let k = &st.konst;
        let mut want = reg.always_events();
        for dl in k.decklists.iter().chain(&k.resource_decklists) {
            for &d in dl.iter() {
                want = want.union(reg.events_of(d));
                if let Some(v) = reg.db().unit_variant(d) {
                    want = want.union(reg.events_of(v.def_id));
                }
            }
        }
        assert_eq!(k.listened, want);
        assert_ne!(k.listened, reg.always_events(), "the decks add events");
        assert!(k.listened.contains(gcg_effects::Ev::TurnEnd));
        assert!(k.listened.contains(gcg_effects::Ev::DestroysByBattle));
        assert_eq!(k.hp_reduction_possible, reg.hp_reduction_possible());
        assert!(k.test_overrides.is_none());
        assert_eq!(reg.var_name(k.vars.todeck), "__todeck");
        assert_eq!(reg.var_name(k.vars.play), "__play");
        let empty: [Box<[DefId]>; 2] = [Box::new([]), Box::new([])];
        let none = GameConst::new(Arc::clone(reg), empty.clone(), empty, 200, 5).unwrap();
        assert_eq!(none.listened, reg.always_events());
        let bad = [Box::new([DefId(60_000)]) as Box<[DefId]>, Box::new([])];
        let err = GameConst::new(Arc::clone(reg), bad, [Box::new([]), Box::new([])], 200, 5);
        assert!(matches!(err, Err(EngineError::Internal(_))));
    }
}
