//! The derived view and its cache (`engine/view.py:141-344`, `:347-483`; PLAN §4.6): the
//! base view, host abilities and gates, `_apply_cont`, the fixpoint, `_finalize`,
//! `same_values`, the trigger index and the cache policy of `derived`.
//!
//! Views are sparse ([`ViewSlot`] per written uid) and built in recycled buffers
//! (`view::pool`, PLAN §11.4 step 1); both are invisible to every reader.

use std::sync::Arc;
use std::sync::atomic::AtomicU16;
use std::sync::atomic::Ordering::Relaxed;

use gcg_effects::{
    AbilityId, ContRef, ContView, FilterId, Gate, Kw, KwAmounts, Node, Registry, RuleKind,
    RuleModId, StrId, ValueRef,
};

use super::eval::{cond, matches, resolve, value};
use super::pool;
use super::query::link_satisfied;
use super::{UNREAD_KEY, ir_bug, uid_of};
use crate::state::region;
use crate::{
    Ctx, CtxVars, Derived, EngineError, EventPayload, FIXPOINT_PASSES, GameState, N_EVENTS, NO_ARG,
    RuleInst, RuleKey, Span, TraitsRef, TriggerIndex, Uid, ViewKey, ViewSlot, Zone,
};

/// `STACKING_KEYWORDS` (`effects/dsl.py:72`).
const fn stacking(kw: Kw) -> bool {
    matches!(kw, Kw::Repair | Kw::Breach | Kw::Support)
}

const OVERFLOW: EngineError = EngineError::Overflow {
    what: "derived view value",
};

fn to_i32(x: i64) -> Result<i32, EngineError> {
    i32::try_from(x).map_err(|_| OVERFLOW)
}

/// `granted: dict[int, list[int]]` (`engine/view.py:411`, `:423`): per uid, the aids an
/// `AbilityGrant` gave it, in grant order (duplicates kept, `view.py:406`). Entries keep the
/// first-grant order; equality is dict equality (keys unordered, lists ordered).
#[derive(Clone, Debug, Default)]
struct Granted(Vec<(Uid, Vec<AbilityId>)>);

impl Granted {
    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// `granted.get(uid)`; empty when absent (a present list is never empty).
    fn get(&self, uid: Uid) -> &[AbilityId] {
        self.0
            .iter()
            .find(|(u, _)| *u == uid)
            .map_or(&[], |(_, l)| l.as_slice())
    }

    /// `uid in granted`.
    fn contains(&self, uid: Uid) -> bool {
        self.0.iter().any(|(u, _)| *u == uid)
    }

    /// `granted_out.setdefault(t, []).append(aid)` (`engine/view.py:406`).
    fn push(&mut self, uid: Uid, aid: AbilityId) {
        if let Some((_, l)) = self.0.iter_mut().find(|(u, _)| *u == uid) {
            l.push(aid);
        } else {
            self.0.push((uid, vec![aid]));
        }
    }
}

impl PartialEq for Granted {
    /// `new_granted == granted` (`engine/view.py:456`): dict-of-lists equality.
    fn eq(&self, other: &Self) -> bool {
        self.0.len() == other.0.len()
            && self
                .0
                .iter()
                .all(|(u, l)| other.contains(*u) && other.get(*u) == l.as_slice())
    }
}

/// A view with every key absent over `n` uids (`Derived()`, `engine/view.py:81-92`), in a
/// recycled buffer.
#[cfg(test)]
pub(crate) fn blank(n: usize, db: Option<&'static gcg_data::CardDb>, key: ViewKey) -> Derived {
    let mut dv = Derived::unallocated();
    dv.reset(n, db);
    dv.key = key;
    dv
}

// ---------------------------------------------------------------------------------------------
// Read access: every accessor treats a uid outside the view as an absent key and returns
// Python's `.get` fallback (PLAN §4.6).

/// The groups of a uid's entries that a recording view notes as read (`Derived::reads`):
/// each accessor reads one group, and two views agree on a read when they agree on its group.
const READ_PRESENT: u16 = 1;
const READ_AP: u16 = 1 << 1;
const READ_HP: u16 = 1 << 2;
const READ_KW: u16 = 1 << 3;
const READ_TRAITS: u16 = 1 << 4;
const READ_RULES: u16 = 1 << 5;
const READ_COST: u16 = 1 << 6;
const READ_LEVEL: u16 = 1 << 7;
const READ_LINKED: u16 = 1 << 8;
const READ_ABILITIES: u16 = 1 << 9;

impl Derived {
    /// The uid's slot, or `None` when the view holds nothing for it.
    #[inline]
    fn entry(&self, uid: Uid) -> Option<&ViewSlot> {
        match self.index.get(usize::from(uid)) {
            Some(&k) if k != 0 => self.slots.get(usize::from(k) - 1),
            _ => None,
        }
    }

    /// [`Derived::entry`] for an accessor that reads the `group` of the uid's entries,
    /// noted while the view records.
    #[inline]
    fn read(&self, uid: Uid, group: u16) -> Option<&ViewSlot> {
        if self.recording.load(Relaxed) {
            self.note(uid, group);
        }
        self.entry(uid)
    }

    #[cold]
    #[inline(never)]
    fn note(&self, uid: Uid, group: u16) {
        if let Some(r) = self.reads.get(usize::from(uid)) {
            r.store(r.load(Relaxed) | group, Relaxed);
        }
    }

    /// The uid's slot when its `ap`/`hp`/`kw`/`traits` keys exist.
    #[inline]
    fn present_read(&self, uid: Uid, group: u16) -> Option<&ViewSlot> {
        self.read(uid, group).filter(|s| s.present)
    }

    /// `uid in dv.ap` (equivalently `hp`, `kw`, `traits`: the four share one key set, set
    /// together for BATTLE and BASE cards in `_base_view`, `view.py:259-280`).
    pub(crate) fn present(&self, uid: Uid) -> bool {
        self.present_read(uid, READ_PRESENT).is_some()
    }

    /// `dv.ap.get(uid)`.
    pub(crate) fn ap_get(&self, uid: Uid) -> Option<i32> {
        self.present_read(uid, READ_AP).map(|s| s.ap)
    }

    /// `dv.hp.get(uid)`.
    pub(crate) fn hp_get(&self, uid: Uid) -> Option<i32> {
        self.present_read(uid, READ_HP).map(|s| s.hp)
    }

    /// `dv.kw.get(uid)`.
    pub(crate) fn kw_get(&self, uid: Uid) -> Option<&KwAmounts> {
        self.present_read(uid, READ_KW).map(|s| &s.kw)
    }

    /// `dv.traits.get(uid)`: the card's printed traits (by def) and its granted ones.
    pub(crate) fn traits_get(&self, uid: Uid) -> Option<(TraitsRef, &[StrId])> {
        self.present_read(uid, READ_TRAITS)
            .map(|s| (s.traits, &self.trait_pool[s.traits.granted.range()]))
    }

    /// `dv.rules.get(uid, ())`.
    pub(crate) fn rules_slice(&self, uid: Uid) -> &[RuleInst] {
        self.read(uid, READ_RULES)
            .map_or(&[], |s| &self.rule_pool[s.rules.range()])
    }

    /// The `RuleMod.kind` of each entry of [`Derived::rules_slice`].
    pub(crate) fn rule_kinds_slice(&self, uid: Uid) -> &[RuleKind] {
        self.read(uid, READ_RULES)
            .map_or(&[], |s| &self.rule_kinds[s.rules.range()])
    }

    /// `dv.cost_mod.get(uid)`.
    pub(crate) fn cost_mod_get(&self, uid: Uid) -> Option<i32> {
        self.read(uid, READ_COST).and_then(|s| s.cost_mod)
    }

    /// `dv.level_mod.get(uid)`.
    pub(crate) fn level_mod_get(&self, uid: Uid) -> Option<i32> {
        self.read(uid, READ_LEVEL).and_then(|s| s.level_mod)
    }

    /// `uid in dv.linked`.
    pub(crate) fn linked_get(&self, uid: Uid) -> bool {
        self.read(uid, READ_LINKED).is_some_and(|s| s.linked)
    }

    /// `dv.abilities.get(uid, ())`.
    pub(crate) fn abilities_slice(&self, uid: Uid) -> &[AbilityId] {
        self.read(uid, READ_ABILITIES)
            .map_or(&[], |s| &self.ability_pool[s.abilities.range()])
    }

    /// The uids of `dv.abilities` in insertion order (`engine/view.py:425`).
    pub(crate) fn hosts(&self) -> &[Uid] {
        &self.hosts
    }

    /// The number of uids the view covers (`len(st.cards)` when it was computed).
    pub(crate) fn len(&self) -> usize {
        self.index.len()
    }

    /// `sorted(dv.linked)`.
    pub(crate) fn linked_sorted(&self) -> impl Iterator<Item = usize> + '_ {
        self.index
            .iter()
            .enumerate()
            .filter_map(|(u, &k)| (k != 0 && self.slots[usize::from(k) - 1].linked).then_some(u))
    }

    /// The printed traits of a [`TraitsRef`] of this view.
    pub(crate) fn printed_traits(&self, t: TraitsRef) -> &'static [Box<str>] {
        self.db.map_or(&[], |db| &db.by_id(t.def).traits)
    }
}

// ---------------------------------------------------------------------------------------------
// Writes, used only while a view is computed.

impl Derived {
    /// The position in `slots` of the uid's slot, created with every key absent on first
    /// write.
    pub(super) fn write_slot(&mut self, uid: Uid) -> Result<usize, EngineError> {
        let n = self.len();
        let Some(k) = self.index.get_mut(usize::from(uid)) else {
            return Err(ir_bug(&format!("uid {uid} outside a view of {n} cards")));
        };
        if *k != 0 {
            return Ok(usize::from(*k) - 1);
        }
        let i = self.slots.len();
        *k = u16::try_from(i + 1).map_err(|_| OVERFLOW)?;
        self.slots.push(ViewSlot::blank(uid));
        Ok(i)
    }

    /// The uid's slot for writing when its `ap`/`hp`/`kw`/`traits` keys exist.
    fn present_mut(&mut self, uid: Uid) -> Option<&mut ViewSlot> {
        match self.index.get(usize::from(uid)) {
            Some(&k) if k != 0 => self.slots.get_mut(usize::from(k) - 1).filter(|s| s.present),
            _ => None,
        }
    }

    /// `aps[uid] = ap; hps[uid] = hp; traits[uid] = d.traits; kws[uid] = {}`
    /// (`engine/view.py:271-280`).
    pub(super) fn set_stats(
        &mut self,
        uid: Uid,
        ap: i32,
        hp: i32,
        def: gcg_data::DefId,
    ) -> Result<(), EngineError> {
        let i = self.write_slot(uid)?;
        let s = &mut self.slots[i];
        s.present = true;
        s.ap = ap;
        s.hp = hp;
        s.kw = KwAmounts::default();
        s.traits = TraitsRef {
            def,
            granted: Span::EMPTY,
        };
        Ok(())
    }

    /// `abilities[uid] = abil` (`engine/view.py:287`, `:300`, `:305`, `:310`).
    pub(super) fn set_abilities(
        &mut self,
        uid: Uid,
        list: &[AbilityId],
    ) -> Result<(), EngineError> {
        let i = self.write_slot(uid)?;
        let span = region(
            self.ability_pool.len(),
            list.len(),
            "derived view abilities",
        )?;
        self.ability_pool.extend_from_slice(list);
        if self.slots[i].abilities.len == 0 {
            self.hosts.push(uid);
        }
        self.slots[i].abilities = span;
        Ok(())
    }

    /// `kws[kw] = kws.get(kw, 0) + amount` for a stacking keyword, else `kws[kw] = 1`
    /// (`engine/view.py:292-295`, `:376-379`).
    pub(super) fn add_keyword(&mut self, uid: Uid, kw: Kw, amount: i64) -> Result<(), EngineError> {
        let i = self.write_slot(uid)?;
        let slot = &mut self.slots[i].kw.0[kw as usize];
        *slot = Some(if stacking(kw) {
            let sum = i64::from(slot.unwrap_or(0))
                .checked_add(amount)
                .ok_or(OVERFLOW)?;
            i16::try_from(sum).map_err(|_| OVERFLOW)?
        } else {
            1
        });
        Ok(())
    }

    /// `dv.rules.setdefault(t, []).append(inst)` (`engine/view.py:387-389`): the uid's list
    /// moves to the pool's tail unless it already ends there.
    pub(super) fn push_rule(
        &mut self,
        uid: Uid,
        inst: RuleInst,
        kind: RuleKind,
    ) -> Result<(), EngineError> {
        let i = self.write_slot(uid)?;
        let old = self.slots[i].rules;
        let start = if old.len > 0 && old.end() == self.rule_pool.len() {
            old.start as usize
        } else {
            let start = self.rule_pool.len();
            self.rule_pool.extend_from_within(old.range());
            self.rule_kinds.extend_from_within(old.range());
            start
        };
        self.rule_pool.push(inst);
        self.rule_kinds.push(kind);
        self.slots[i].rules = region(start, old.len as usize + 1, "derived view rules")?;
        Ok(())
    }

    /// `dv.traits[t] = cur + new` (`engine/view.py:384`): the granted part moves to the pool's
    /// tail with `new` appended.
    pub(super) fn grant_traits(&mut self, uid: Uid, new: &[StrId]) -> Result<(), EngineError> {
        if new.is_empty() {
            return Ok(());
        }
        let i = self.write_slot(uid)?;
        let old = self.slots[i].traits.granted;
        let start = self.trait_pool.len();
        self.trait_pool.extend_from_within(old.range());
        self.trait_pool.extend_from_slice(new);
        self.slots[i].traits.granted =
            region(start, old.len as usize + new.len(), "derived view traits")?;
        Ok(())
    }

    /// Starts noting the reads of this buffer: every uid's groups empty.
    fn start_recording(&mut self) {
        let n = self.len();
        self.reads.clear();
        self.reads.resize_with(n, || AtomicU16::new(0));
        *self.recording.get_mut() = true;
    }

    fn stop_recording(&mut self) {
        *self.recording.get_mut() = false;
    }

    /// Whether `other` answers every read noted on this buffer exactly as this buffer does
    /// (and holds the same hosts, the only other thing a pass reads from its view).
    fn reads_agree(&self, other: &Derived) -> bool {
        self.len() == other.len()
            && self.hosts == other.hosts
            && self.hosts_split == other.hosts_split
            && self.reads.iter().enumerate().all(|(u, r)| {
                let groups = r.load(Relaxed);
                groups == 0 || Uid::try_from(u).is_ok_and(|u| groups_agree(self, other, u, groups))
            })
    }

    /// `_copy_base(base)` (`engine/view.py:336-344`) into this buffer: the stats, keywords and
    /// traits copied, `linked` and `abilities` the base's, `rules`, `cost_mod` and `level_mod`
    /// empty.
    fn copy_base(&mut self, base: &Derived) {
        self.key = UNREAD_KEY;
        self.index.clone_from(&base.index);
        self.slots.clone_from(&base.slots);
        for s in &mut self.slots {
            s.rules = Span::EMPTY;
            s.cost_mod = None;
            s.level_mod = None;
        }
        self.trait_pool.clone_from(&base.trait_pool);
        self.rule_pool.clear();
        self.rule_kinds.clear();
        self.ability_pool.clone_from(&base.ability_pool);
        self.hosts.clone_from(&base.hosts);
        self.hosts_split = base.hosts_split;
        self.converged = true;
        self.db = base.db;
        drop(self.by_event.take());
    }
}

/// Whether `a` and `b` answer every accessor of the read `groups` of `u` identically.
fn groups_agree(a: &Derived, b: &Derived, u: Uid, groups: u16) -> bool {
    let reads = |g: u16| groups & g != 0;
    (!reads(READ_PRESENT) || a.present(u) == b.present(u))
        && (!reads(READ_AP) || a.ap_get(u) == b.ap_get(u))
        && (!reads(READ_HP) || a.hp_get(u) == b.hp_get(u))
        && (!reads(READ_KW) || a.kw_get(u) == b.kw_get(u))
        && (!reads(READ_TRAITS)
            || a.traits_get(u).map(|(t, g)| (t.def, g)) == b.traits_get(u).map(|(t, g)| (t.def, g)))
        && (!reads(READ_RULES)
            || (a.rules_slice(u) == b.rules_slice(u)
                && a.rule_kinds_slice(u) == b.rule_kinds_slice(u)))
        && (!reads(READ_COST) || a.cost_mod_get(u) == b.cost_mod_get(u))
        && (!reads(READ_LEVEL) || a.level_mod_get(u) == b.level_mod_get(u))
        && (!reads(READ_LINKED) || a.linked_get(u) == b.linked_get(u))
        && (!reads(READ_ABILITIES) || a.abilities_slice(u) == b.abilities_slice(u))
}

// ---------------------------------------------------------------------------------------------
// The cache (engine/view.py:223-239)

/// `_view_key(st)` (`engine/view.py:223-229`): `(st.active, st.turn, len(st.battles),
/// (b.battle_id, b.target, b.ended) or (), len(st.history))`.
pub(crate) fn view_key(st: &GameState) -> ViewKey {
    ViewKey {
        active: st.active,
        turn: st.turn,
        n_battles: u32::try_from(st.battles.len()).unwrap_or(u32::MAX),
        battle: st.battles.last().map(|b| (b.battle_id, b.target, b.ended)),
        n_history: u32::try_from(st.history.len()).unwrap_or(u32::MAX),
    }
}

/// `derived(st)` (`engine/view.py:232-239`), in the four steps of PLAN §4.6: the cached `Arc`
/// is cloned out of the cell (only on a hit) and the borrow released, the key is computed
/// **before** any compute, the view is recomputed with no borrow held when the cache is empty
/// or its key differs, and the result is stored with its key. `touch()` is the only
/// invalidation. It fills the cache of whatever state it is called on, the AI's true states
/// included.
///
/// # Errors
/// What `_compute` raises (`MinOf([])` `ValueError`, `view.py:878`; a hook's error; an
/// integer that leaves its Rust type). The cache is then unchanged, as in Python.
pub fn derived(st: &GameState) -> Result<Arc<Derived>, EngineError> {
    let key = view_key(st);
    let hit = match &*st.cache.borrow() {
        Some(dv) if dv.key == key => Some(Arc::clone(dv)),
        _ => None,
    };
    if let Some(dv) = hit {
        #[cfg(feature = "paranoid")]
        super::paranoid::cached_view(st, &dv)?;
        return Ok(dv);
    }
    let mut w = pool::work();
    let dv = compute_in(st, &mut w).map(|dv| {
        dv.key = key;
        pool::share(dv)
    });
    pool::give_work(w);
    let dv = dv?;
    let old = st.cache.borrow_mut().replace(Arc::clone(&dv));
    if let Some(old) = old {
        pool::recycle(old);
    }
    Ok(dv)
}

// ---------------------------------------------------------------------------------------------
// Host abilities and gates (engine/view.py:141-216)

/// The `AbilityNode` of an aid (`R.abilities[aid].ability`).
fn ability_node(reg: &Registry, aid: AbilityId) -> &Node {
    reg.node(reg.ability(aid).ability.node())
}

/// `_ability_gate(a)` (`engine/view.py:163-168`).
fn ability_gate(reg: &Registry, aid: AbilityId) -> (Gate, &[FilterId]) {
    match ability_node(reg, aid) {
        Node::Constant(a) => (a.gate, &a.gate_filters),
        Node::Triggered(a) => (a.gate, &a.gate_filters),
        Node::Activated(a) => (a.gate, &a.gate_filters),
        Node::Keyword(a) => (a.gate, &a.gate_filters),
        Node::Replacement(a) => (a.gate, &[]),
        _ => (Gate::None, &[]),
    }
}

/// `_gate_ok(st, dv, host, gate, gate_filters)` (`engine/view.py:141-154`): the gate filters
/// test the paired card.
fn gate_ok(
    st: &GameState,
    dv: &Derived,
    host: Uid,
    gate: Gate,
    gate_filters: &[FilterId],
) -> Result<bool, EngineError> {
    if gate == Gate::None {
        return Ok(true);
    }
    let card = st.card(host);
    if card.zone != Zone::Battle || card.pair < 0 {
        return Ok(false);
    }
    if gate == Gate::Linked && !dv.linked_get(host) {
        return Ok(false);
    }
    if !gate_filters.is_empty() {
        let ctx = Ctx::new(
            card.owner,
            i32::from(host),
            NO_ARG,
            CtxVars::Empty,
            EventPayload::default(),
        );
        return matches(st, dv, &ctx, uid_of(st, card.pair)?, gate_filters);
    }
    Ok(true)
}

/// `_host_abilities(st, dv, uid, granted)` (`engine/view.py:180-216`) into `out`: the base
/// entries of the zone, then the pilot's unit entries, then the granted aids (duplicates
/// kept), filtered by their gates when any is gated.
fn host_abilities(
    st: &GameState,
    dv: &Derived,
    uid: Uid,
    granted: &Granted,
    out: &mut Vec<AbilityId>,
) -> Result<(), EngineError> {
    out.clear();
    let reg = &**st.registry();
    let card = st.card(uid);
    let entry = reg.card(card.def_id);
    let base: &[AbilityId] = match card.zone {
        Zone::Battle | Zone::Base => &entry.field_entries,
        Zone::Hand => &entry.hand_entries,
        Zone::Trash => &entry.trash_entries,
        Zone::Paired => &entry.paired_entries,
        _ => return Ok(()),
    };
    out.extend_from_slice(base);
    let mut gated = entry.gated;
    if card.zone == Zone::Battle && card.pair >= 0 {
        let pentry = reg.card(st.card(uid_of(st, card.pair)?).def_id);
        if !pentry.unit_entries.is_empty() {
            out.extend_from_slice(&pentry.unit_entries);
            gated = gated || pentry.gated;
        }
    }
    let g = granted.get(uid);
    if !g.is_empty() {
        out.extend_from_slice(g);
        gated = true;
    }
    if !gated {
        return Ok(());
    }
    let mut kept = 0;
    for i in 0..out.len() {
        let aid = out[i];
        let (gate, gf) = ability_gate(reg, aid);
        if gate == Gate::None || gate_ok(st, dv, uid, gate, gf)? {
            out[kept] = aid;
            kept += 1;
        }
    }
    out.truncate(kept);
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// The base view and the fixpoint (engine/view.py:245-311, :336-463)

/// `_base_view(st, granted, prev)` (`engine/view.py:245-311`) into the buffer `dv`. Pass 1
/// sets the stats, printed traits, links and empty keyword maps of BATTLE then BASE cards, p0
/// then p1; pass 2 sets the abilities (BATTLE, BASE, PAIRED, HAND, TRASH per player) and the
/// keywords of BATTLE/BASE hosts. Gates read `prev`, or the view being built when there is
/// none (Inv 05 §5.5).
fn base_view(
    st: &GameState,
    granted: &Granted,
    prev: Option<&Derived>,
    dv: &mut Derived,
) -> Result<(), EngineError> {
    let reg = &**st.registry();
    let db = st.db();
    dv.reset(st.cards.len(), Some(db));
    for p in [0u8, 1] {
        for &uid in st.zone(p, Zone::Battle) {
            let c = st.card(uid);
            let u = db.by_id(c.def_id);
            let (mut ap, mut hp) = (i64::from(u.ap), i64::from(u.hp));
            if c.pair >= 0 {
                let pair = uid_of(st, c.pair)?;
                let pd = db.by_id(st.card(pair).def_id);
                ap += i64::from(pd.ap);
                hp += i64::from(pd.hp);
                if link_satisfied(st, u, pd) {
                    let i = dv.write_slot(uid)?;
                    dv.slots[i].linked = true;
                }
            }
            dv.set_stats(uid, to_i32(ap)?, to_i32(hp)?, c.def_id)?;
        }
        for &uid in st.zone(p, Zone::Base) {
            let def = st.card(uid).def_id;
            let b = db.by_id(def);
            dv.set_stats(uid, b.ap, b.hp, def)?;
        }
    }
    let mut abil = std::mem::take(&mut dv.scratch_aids);
    for p in [0u8, 1] {
        let battle = st.zone(p, Zone::Battle);
        let bases = st.zone(p, Zone::Base);
        for &uid in battle.iter().chain(bases) {
            host_abilities(st, prev.unwrap_or(dv), uid, granted, &mut abil)?;
            if abil.is_empty() {
                continue;
            }
            dv.set_abilities(uid, &abil)?;
            for &aid in &abil {
                if let Node::Keyword(k) = ability_node(reg, aid) {
                    dv.add_keyword(uid, k.keyword, k.amount)?;
                }
            }
        }
        for zone in [Zone::Paired, Zone::Hand, Zone::Trash] {
            for &uid in st.zone(p, zone) {
                let entry = reg.card(st.card(uid).def_id);
                let own: &[AbilityId] = match zone {
                    Zone::Paired => &entry.paired_entries,
                    Zone::Hand => &entry.hand_entries,
                    _ => &entry.trash_entries,
                };
                if own.is_empty() && !granted.contains(uid) {
                    continue;
                }
                host_abilities(st, prev.unwrap_or(dv), uid, granted, &mut abil)?;
                if !abil.is_empty() {
                    dv.set_abilities(uid, &abil)?;
                }
            }
        }
        if p == 0 {
            dv.hosts_split = u32::try_from(dv.hosts.len()).map_err(|_| OVERFLOW)?;
        }
    }
    dv.scratch_aids = abil;
    Ok(())
}

/// `value(st, view, ctx, v) if v != 0 else 0` (`engine/view.py:360-361`, `:371`, `:391-392`):
/// a node is never `== 0` (Inv 05 §7 hazard 11), so only the literal 0 is skipped.
fn amount(st: &GameState, view: &Derived, ctx: &Ctx, v: ValueRef) -> Result<i64, EngineError> {
    if v == ValueRef::Lit(0) {
        Ok(0)
    } else {
        value(st, view, ctx, v)
    }
}

/// `_ap_protected(view, uid)` (`engine/view.py:466-468`).
fn ap_protected(view: &Derived, uid: Uid) -> bool {
    view.rule_kinds_slice(uid)
        .contains(&RuleKind::ApCantBeReduced)
}

/// The `RuleMod.kind` of a rule node.
fn rule_kind(reg: &Registry, rule: RuleModId) -> Result<RuleKind, EngineError> {
    match reg.node(rule.node()) {
        Node::RuleMod(m) => Ok(m.kind),
        other => Err(ir_bug(&format!(
            "a RuleModId names a {} node",
            other.kind().py_name()
        ))),
    }
}

/// `aids[index]` with Python's negative indexing (`engine/view.py:404-406`: only
/// `index < len(aids)` is checked, so a negative index counts from the end).
fn py_index(aids: &[AbilityId], index: i64) -> Result<AbilityId, EngineError> {
    let len = i64::try_from(aids.len()).map_err(|_| OVERFLOW)?;
    let i = if index < 0 { index + len } else { index };
    usize::try_from(i)
        .ok()
        .and_then(|i| aids.get(i).copied())
        .ok_or_else(|| EngineError::Index("list index out of range".to_owned()))
}

/// The arguments of one `_apply_cont` call that identify the rule instances it creates.
#[derive(Clone, Copy)]
struct Source {
    rule_key: RuleKey,
    source: i32,
    aux: i32,
}

/// `_apply_cont(st, dv, view, ctx, eff, targets, rule_key, source, granted_out, aux)`
/// (`engine/view.py:347-406`): reads from `view`, writes to `dv`.
#[allow(
    clippy::too_many_arguments,
    reason = "Python's parameter list (engine/view.py:347-358)"
)]
fn apply_cont(
    st: &GameState,
    dv: &mut Derived,
    view: &Derived,
    ctx: &Ctx,
    eff: ContRef,
    targets: &[Uid],
    src: Source,
    granted_out: &mut Granted,
) -> Result<(), EngineError> {
    let reg = &**st.registry();
    match reg.cont_node(eff) {
        ContView::StatMod { ap, hp } => {
            let ap = amount(st, view, ctx, ap)?;
            let hp = amount(st, view, ctx, hp)?;
            for &t in targets {
                let add = |x: i32, d: i64| to_i32(i64::from(x) + d);
                if ap < 0 && st.card(t).owner != ctx.controller && ap_protected(view, t) {
                    if let Some(s) = dv.present_mut(t) {
                        s.hp = add(s.hp, hp)?;
                    }
                    continue;
                }
                if let Some(s) = dv.present_mut(t) {
                    s.ap = add(s.ap, ap)?;
                    s.hp = add(s.hp, hp)?;
                }
            }
        }
        ContView::KeywordGrant { keyword, amount: a } => {
            let amt = amount(st, view, ctx, a)?;
            for &t in targets {
                if dv.present(t) {
                    dv.add_keyword(t, keyword, amt)?;
                }
            }
        }
        ContView::TraitGrant { traits } => {
            let mut new: Vec<StrId> = Vec::new();
            for &t in targets {
                let Some((cur, granted)) = dv.traits_get(t) else {
                    continue;
                };
                let printed = dv.printed_traits(cur);
                new.clear();
                new.extend(traits.iter().copied().filter(|&x| {
                    let s = reg.str(x);
                    !printed.iter().any(|p| **p == *s) && !granted.contains(&x)
                }));
                dv.grant_traits(t, &new)?;
            }
        }
        ContView::RuleGrant { rule } => {
            let kind = rule_kind(reg, rule)?;
            for &t in targets {
                let inst = RuleInst {
                    rule,
                    controller: ctx.controller,
                    source: src.source,
                    key: src.rule_key,
                    aux: src.aux,
                };
                dv.push_rule(t, inst, kind)?;
            }
        }
        ContView::CostMod { cost, level } => {
            let cm = amount(st, view, ctx, cost)?;
            let lm = amount(st, view, ctx, level)?;
            for &t in targets {
                let i = dv.write_slot(t)?;
                let s = &mut dv.slots[i];
                if cm != 0 {
                    let v = i64::from(s.cost_mod.unwrap_or(0)) + cm;
                    s.cost_mod = Some(to_i32(v)?);
                }
                if lm != 0 {
                    let v = i64::from(s.level_mod.unwrap_or(0)) + lm;
                    s.level_mod = Some(to_i32(v)?);
                }
            }
        }
        ContView::AbilityGrant {
            card_number,
            ability_index,
        } => {
            let Some(entry) = reg.db().get(reg.str(card_number)) else {
                return Ok(());
            };
            let ce = reg.card(entry.def_id);
            let aids: Vec<AbilityId> = ce.own.iter().chain(ce.unit.iter()).copied().collect();
            let len = i64::try_from(aids.len()).map_err(|_| OVERFLOW)?;
            if ability_index < len {
                for &t in targets {
                    granted_out.push(t, py_index(&aids, ability_index)?);
                }
            }
        }
    }
    Ok(())
}

/// The constants of `view.abilities` in insertion order (`engine/view.py:425-436`): each
/// with `Ctx(owner, host)`, its condition, its scope resolved once, and its effects in order.
fn apply_constants(
    st: &GameState,
    cur: &mut Derived,
    view: &Derived,
    new_granted: &mut Granted,
) -> Result<(), EngineError> {
    let reg = &**st.registry();
    for &host in view.hosts() {
        let owner = st.card(host).owner;
        for &aid in view.abilities_slice(host) {
            let Node::Constant(ab) = ability_node(reg, aid) else {
                continue;
            };
            let ctx = Ctx::new(
                owner,
                i32::from(host),
                NO_ARG,
                CtxVars::Empty,
                EventPayload::default(),
            );
            // `ab.cond is not d.TRUE and not cond(...)` (view.py:432): evaluating `And(())`
            // gives True with no effect, so the identity test needs no counterpart.
            if !cond(st, view, &ctx, ab.cond, true)? {
                continue;
            }
            let targets = resolve(st, view, &ctx, ab.scope)?;
            let src = Source {
                rule_key: RuleKey::Constant { aid, host },
                source: i32::from(host),
                aux: NO_ARG,
            };
            for &eff in &ab.effects {
                apply_cont(
                    st,
                    cur,
                    view,
                    &ctx,
                    reg.cont_of(eff),
                    &targets,
                    src,
                    new_granted,
                )?;
            }
        }
    }
    Ok(())
}

/// The lasting effects in `st.lasting` order (`engine/view.py:437-455`): card-level ones
/// only, on their targets whose `zone_seq` still matches (no zone check), with
/// `Ctx(controller, source_uid)` and the rule key `(-1, effect_key, serial)`.
fn apply_lasting(
    st: &GameState,
    cur: &mut Derived,
    view: &Derived,
    new_granted: &mut Granted,
) -> Result<(), EngineError> {
    let mut live = std::mem::take(&mut cur.scratch_uids);
    for le in &st.lasting {
        if i32::from(le.player) != NO_ARG {
            continue;
        }
        live.clear();
        for &(uid, seq) in st.lasting_targets(le) {
            if st.card(uid).zone_seq == seq {
                live.push(uid);
            }
        }
        if live.is_empty() {
            continue;
        }
        let ctx = Ctx::new(
            le.controller,
            le.source_uid,
            NO_ARG,
            CtxVars::Empty,
            EventPayload::default(),
        );
        let src = Source {
            rule_key: RuleKey::Lasting {
                effect: le.effect,
                serial: le.serial,
            },
            source: le.source_uid,
            aux: le.aux,
        };
        apply_cont(st, cur, view, &ctx, le.effect, &live, src, new_granted)?;
    }
    live.clear();
    cur.scratch_uids = live;
    Ok(())
}

/// `_finalize(dv)` (`engine/view.py:477-483`): negative AP and HP become 0, once, at the end.
fn finalize(dv: &mut Derived) {
    for s in &mut dv.slots {
        if s.present {
            s.ap = s.ap.max(0);
            s.hp = s.hp.max(0);
        }
    }
}

/// `_compute(st)` (`engine/view.py:409-463`), ported literally: the base view; the early
/// exit when nothing is lasting and no constant exists; then at most [`FIXPOINT_PASSES`]
/// Jacobi passes that read the previous pass (the base in pass 0) and write a fresh view,
/// converged when `same_values` and the granted map both repeat (never in pass 0); the last
/// pass wins at the cap (W-48: `converged` is then false); `_finalize` runs once. The base
/// and the passes live in the buffers of `w`; the two pass buffers swap roles each pass. The
/// result is the buffer that holds the view.
///
/// # Errors
/// What a condition, value, reference or hook raises.
fn compute_in<'w>(st: &GameState, w: &'w mut pool::Work) -> Result<&'w mut Derived, EngineError> {
    let reg = &**st.registry();
    let mut granted = Granted::default();
    let pool::Work { base, a, b } = w;
    base_view(st, &granted, None, base)?;
    let has_constants = base.hosts().iter().any(|&h| {
        base.abilities_slice(h)
            .iter()
            .any(|&aid| matches!(ability_node(reg, aid), Node::Constant(_)))
    });
    if st.lasting.is_empty() && !has_constants {
        finalize(base);
        return Ok(base);
    }
    let (mut prev, mut cur): (&mut Derived, &mut Derived) = (a, b);
    let mut have_prev = false;
    let mut converged = false;
    for _ in 0..FIXPOINT_PASSES {
        let mut new_granted = Granted::default();
        if granted.is_empty() {
            cur.copy_base(base);
        } else {
            base_view(st, &granted, have_prev.then_some(&*prev), cur)?;
        }
        let first = !have_prev;
        if first {
            base.start_recording();
        }
        let view: &Derived = if have_prev { prev } else { base };
        apply_constants(st, cur, view, &mut new_granted)?;
        apply_lasting(st, cur, view, &mut new_granted)?;
        // Pass 1 would start from the same copy of the base (no grant) and read its view where
        // pass 0 read the base. When every read of pass 0 answers the same in pass 0's result,
        // pass 1 runs exactly as pass 0 did and converges on a copy of it (same_values and the
        // granted map repeat), so pass 0's result is the view.
        let skip = first && {
            base.stop_recording();
            new_granted.is_empty() && base.reads_agree(cur)
        };
        #[cfg(feature = "paranoid")]
        if skip {
            check_skipped_pass(st, base, cur)?;
        }
        let done = have_prev && same_values(cur, prev) && new_granted == granted;
        std::mem::swap(&mut prev, &mut cur);
        have_prev = true;
        if done || skip {
            converged = true;
            break;
        }
        granted = new_granted;
    }
    let dv = if have_prev { prev } else { base };
    dv.converged = converged;
    if !converged {
        crate::diag::note_non_converged();
    }
    finalize(dv);
    Ok(dv)
}

/// The `paranoid` check of a skipped pass (`Check::SkippedPass`): pass 1 run in full on
/// `pass0` repeats it, with no grant.
///
/// # Errors
/// What the pass raises (pass 0 did not, so this is a failed check too).
///
/// # Panics
/// `paranoid: skipped fixpoint pass differs`.
#[cfg(feature = "paranoid")]
fn check_skipped_pass(st: &GameState, base: &Derived, pass0: &Derived) -> Result<(), EngineError> {
    const ALL: u16 = (READ_ABILITIES << 1) - 1;
    crate::paranoid::count(crate::paranoid::Check::SkippedPass);
    let mut again = Derived::unallocated();
    again.copy_base(base);
    let mut granted = Granted::default();
    apply_constants(st, &mut again, pass0, &mut granted)?;
    apply_lasting(st, &mut again, pass0, &mut granted)?;
    let same = granted.is_empty()
        && same_values(&again, pass0)
        && again.hosts == pass0.hosts
        && (0..again.len())
            .all(|u| Uid::try_from(u).is_ok_and(|u| groups_agree(&again, pass0, u, ALL)));
    assert!(
        same,
        "paranoid: skipped fixpoint pass differs, turn {}, step {:?}",
        st.turn, st.step
    );
    Ok(())
}

/// `_compute(st)` as an owned view (the `paranoid` checks compare it with a cached one).
///
/// # Errors
/// As [`compute_in`].
#[cfg(feature = "paranoid")]
pub(crate) fn compute(st: &GameState) -> Result<Derived, EngineError> {
    let mut w = pool::work();
    let dv = compute_in(st, &mut w).map(|dv| std::mem::replace(dv, Derived::unallocated()));
    pool::give_work(w);
    dv
}

// ---------------------------------------------------------------------------------------------
// same_values and the trigger index

/// `dv.traits[u] == other.traits[u]` for a uid present in both: the printed traits of the
/// two defs as strings (equal defs need no lookup), then the granted traits. Granted traits
/// are `StrId`s of one registry, where an id names one string; both callers compare views of
/// one game (the fixpoint, `ai/actions.py:230`), so the printed/granted split is the same
/// whenever the defs are.
fn traits_equal(a: &Derived, sa: &ViewSlot, b: &Derived, sb: &ViewSlot) -> bool {
    let (ta, tb) = (sa.traits, sb.traits);
    (ta.def == tb.def || a.printed_traits(ta) == b.printed_traits(tb))
        && a.trait_pool[ta.granted.range()] == b.trait_pool[tb.granted.range()]
}

/// One uid of `same_values`: `sa` and `sb` are its slots in `a` and `b`, a missing slot
/// reading as every key absent.
fn slot_equal(a: &Derived, sa: Option<&ViewSlot>, b: &Derived, sb: Option<&ViewSlot>) -> bool {
    const BLANK: ViewSlot = ViewSlot::blank(0);
    let sa = sa.unwrap_or(&BLANK);
    let sb = sb.unwrap_or(&BLANK);
    if sa.present != sb.present {
        return false;
    }
    if sa.present
        && (sa.ap != sb.ap || sa.hp != sb.hp || sa.kw != sb.kw || !traits_equal(a, sa, b, sb))
    {
        return false;
    }
    sa.cost_mod == sb.cost_mod
        && sa.level_mod == sb.level_mod
        && sa.rules.len == sb.rules.len
        && a.rule_pool[sa.rules.range()]
            .iter()
            .zip(&b.rule_pool[sb.rules.range()])
            .all(|(x, y)| x.rule == y.rule)
        && a.ability_pool[sa.abilities.range()] == b.ability_pool[sb.abilities.range()]
}

/// `a.same_values(b)` (`engine/view.py:94-106`): `ap`, `hp`, `kw`, `traits`, `cost_mod` and
/// `level_mod` equal as dicts (presence included), the per-uid rule lists equal by `RuleMod`
/// (controller, source, key and aux ignored) and the per-uid aid lists equal. `linked`,
/// `by_event` and `key` are not compared. Uids outside a view, and uids without a slot, are
/// absent keys; so every uid outside both views' slots is equal, and only the slots of each
/// side are compared against the other side.
#[must_use]
pub fn same_values(a: &Derived, b: &Derived) -> bool {
    a.slots
        .iter()
        .all(|sa| slot_equal(a, Some(sa), b, b.entry(sa.uid)))
        && b.slots
            .iter()
            .all(|sb| a.entry(sb.uid).is_some() || slot_equal(a, None, b, Some(sb)))
}

/// The hosts' Triggered abilities of `trigger_index(st, dv)` (`engine/view.py:320-331`) in
/// scan order: the active player, then the other, each of BATTLE, BASE, PAIRED, HAND, TRASH in
/// list order, each host's `dv.abilities` in order.
fn scan_triggers(st: &GameState, dv: &Derived, mut f: impl FnMut(usize, Uid, AbilityId)) {
    let reg = &**st.registry();
    for p in [st.active, 1 - st.active] {
        for z in [
            Zone::Battle,
            Zone::Base,
            Zone::Paired,
            Zone::Hand,
            Zone::Trash,
        ] {
            for &uid in st.zone(p, z) {
                for &aid in dv.abilities_slice(uid) {
                    if let Some(ev) = reg.ability(aid).trigger_event {
                        f(ev as usize, uid, aid);
                    }
                }
            }
        }
    }
}

/// The same scan over `dv.hosts` (no zone lists): exact when `st`'s zones are those `dv` was
/// computed from, because `hosts` holds, per player, every uid of those zones with abilities
/// in the scan's zone and list order (`_base_view`, `view.py:245-311`).
fn scan_hosts(st: &GameState, dv: &Derived, mut f: impl FnMut(usize, Uid, AbilityId)) {
    let reg = &**st.registry();
    let (p0, p1) = dv
        .hosts
        .split_at((dv.hosts_split as usize).min(dv.hosts.len()));
    let (first, second) = if st.active == 0 { (p0, p1) } else { (p1, p0) };
    for &uid in first.iter().chain(second) {
        for &aid in dv.abilities_slice(uid) {
            if let Some(ev) = reg.ability(aid).trigger_event {
                f(ev as usize, uid, aid);
            }
        }
    }
}

/// The trigger index of one scan grouped by event (stable), counted in a first run of the
/// scan and placed in a second.
fn group_index(scan: impl Fn(&mut dyn FnMut(usize, Uid, AbilityId))) -> TriggerIndex {
    let mut offs = [0u32; N_EVENTS + 1];
    scan(&mut |ev, _, _| offs[ev + 1] += 1);
    for i in 0..N_EVENTS {
        offs[i + 1] += offs[i];
    }
    let mut next = offs;
    let mut items = pool::index_list();
    items.resize(offs[N_EVENTS] as usize, (0, AbilityId(0)));
    scan(&mut |ev, uid, aid| {
        items[next[ev] as usize] = (uid, aid);
        next[ev] += 1;
    });
    TriggerIndex { offs, items }
}

/// Builds `trigger_index(st, dv)` (`engine/view.py:320-331`) from `st`'s zones: the scan of
/// [`scan_triggers`] grouped by event (stable).
pub(crate) fn build_index(st: &GameState, dv: &Derived) -> TriggerIndex {
    group_index(|f| scan_triggers(st, dv, f))
}

/// `trigger_index(st, dv)` (`engine/view.py:314-333`): built once from the zones of the
/// state that asks first and kept on the view (`dv.by_event`, shared by every holder of the
/// view, Inv 11 hazard H2). When `dv` is the view `st` has cached, `st`'s zones are the ones
/// `dv` was computed from (every zone change touches, which empties the cache, and a clone
/// copies the zones with the cache), so the index is read off `dv.hosts` instead of the zone
/// lists; `paranoid` compares every filled index with a zone scan of a fresh compute.
pub fn trigger_index<'a>(st: &GameState, dv: &'a Derived) -> &'a TriggerIndex {
    dv.by_event.get_or_init(|| {
        let cached = st
            .cache
            .borrow()
            .as_ref()
            .is_some_and(|c| std::ptr::eq(&**c, dv));
        if cached {
            group_index(|f| scan_hosts(st, dv, f))
        } else {
            build_index(st, dv)
        }
    })
}
