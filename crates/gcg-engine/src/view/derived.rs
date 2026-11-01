//! The derived view and its cache (`engine/view.py:141-344`, `:347-483`; PLAN §4.6): the
//! base view, host abilities and gates, `_apply_cont`, the fixpoint, `_finalize`,
//! `same_values`, the trigger index and the cache policy of `derived`.

use std::sync::{Arc, OnceLock};

use gcg_data::CardDb;
use gcg_effects::{
    AbilityId, ContRef, ContView, FilterId, Gate, Kw, KwAmounts, Node, Registry, RuleKind,
    RuleModId, StrId, ValueRef,
};

use super::eval::{cond, matches, resolve, value};
use super::query::link_satisfied;
use super::{UNREAD_KEY, ir_bug, uid_of};
use crate::state::region;
use crate::{
    Ctx, CtxVars, Derived, EngineError, EventPayload, FIXPOINT_PASSES, GameState, N_EVENTS, NO_ARG,
    RuleInst, RuleKey, Span, TraitsRef, TriggerIndex, Uid, ViewKey, Zone,
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

/// A view with every key absent over `n` uids (`Derived()`, `engine/view.py:81-92`).
pub(crate) fn blank(n: usize, db: Option<&'static CardDb>, key: ViewKey) -> Derived {
    Derived {
        key,
        present: vec![false; n],
        ap: vec![0; n],
        hp: vec![0; n],
        kw: vec![KwAmounts::default(); n],
        traits: vec![
            TraitsRef {
                def: gcg_data::DefId(0),
                granted: Span::EMPTY,
            };
            n
        ],
        trait_pool: Vec::new(),
        rules: vec![Span::EMPTY; n],
        rule_pool: Vec::new(),
        rule_kinds: Vec::new(),
        cost_mod: vec![None; n],
        level_mod: vec![None; n],
        linked: vec![false; n],
        abilities: vec![Span::EMPTY; n],
        ability_pool: Vec::new(),
        hosts: Vec::new(),
        converged: true,
        db,
        by_event: OnceLock::new(),
    }
}

// ---------------------------------------------------------------------------------------------
// Read access: every accessor treats a uid outside the view as an absent key and returns
// Python's `.get` fallback (PLAN §4.6).

impl Derived {
    fn slot(&self, uid: Uid) -> Option<usize> {
        let i = usize::from(uid);
        (i < self.present.len()).then_some(i)
    }

    /// `uid in dv.ap` (equivalently `hp`, `kw`, `traits`: the four share one key set, set
    /// together for BATTLE and BASE cards in `_base_view`, `view.py:259-280`).
    pub(crate) fn present(&self, uid: Uid) -> bool {
        self.slot(uid).is_some_and(|i| self.present[i])
    }

    /// `dv.ap.get(uid)`.
    pub(crate) fn ap_get(&self, uid: Uid) -> Option<i32> {
        self.slot(uid)
            .filter(|&i| self.present[i])
            .map(|i| self.ap[i])
    }

    /// `dv.hp.get(uid)`.
    pub(crate) fn hp_get(&self, uid: Uid) -> Option<i32> {
        self.slot(uid)
            .filter(|&i| self.present[i])
            .map(|i| self.hp[i])
    }

    /// `dv.kw.get(uid)`.
    pub(crate) fn kw_get(&self, uid: Uid) -> Option<&KwAmounts> {
        self.slot(uid)
            .filter(|&i| self.present[i])
            .map(|i| &self.kw[i])
    }

    /// `dv.traits.get(uid)`: the card's printed traits (by def) and its granted ones.
    pub(crate) fn traits_get(&self, uid: Uid) -> Option<(TraitsRef, &[StrId])> {
        self.slot(uid).filter(|&i| self.present[i]).map(|i| {
            (
                self.traits[i],
                &self.trait_pool[self.traits[i].granted.range()],
            )
        })
    }

    /// `dv.rules.get(uid, ())`.
    pub(crate) fn rules_slice(&self, uid: Uid) -> &[RuleInst] {
        self.slot(uid)
            .map_or(&[], |i| &self.rule_pool[self.rules[i].range()])
    }

    /// The `RuleMod.kind` of each entry of [`Derived::rules_slice`].
    pub(crate) fn rule_kinds_slice(&self, uid: Uid) -> &[RuleKind] {
        self.slot(uid)
            .map_or(&[], |i| &self.rule_kinds[self.rules[i].range()])
    }

    /// `dv.cost_mod.get(uid)`.
    pub(crate) fn cost_mod_get(&self, uid: Uid) -> Option<i32> {
        self.slot(uid).and_then(|i| self.cost_mod[i])
    }

    /// `dv.level_mod.get(uid)`.
    pub(crate) fn level_mod_get(&self, uid: Uid) -> Option<i32> {
        self.slot(uid).and_then(|i| self.level_mod[i])
    }

    /// `uid in dv.linked`.
    pub(crate) fn linked_get(&self, uid: Uid) -> bool {
        self.slot(uid).is_some_and(|i| self.linked[i])
    }

    /// `dv.abilities.get(uid, ())`.
    pub(crate) fn abilities_slice(&self, uid: Uid) -> &[AbilityId] {
        self.slot(uid)
            .map_or(&[], |i| &self.ability_pool[self.abilities[i].range()])
    }

    /// The uids of `dv.abilities` in insertion order (`engine/view.py:425`).
    pub(crate) fn hosts(&self) -> &[Uid] {
        &self.hosts
    }

    /// The number of uids the view covers (`len(st.cards)` when it was computed).
    pub(crate) fn len(&self) -> usize {
        self.present.len()
    }

    /// `sorted(dv.linked)`.
    pub(crate) fn linked_sorted(&self) -> impl Iterator<Item = usize> + '_ {
        self.linked
            .iter()
            .enumerate()
            .filter_map(|(i, &l)| l.then_some(i))
    }

    /// The printed traits of a [`TraitsRef`] of this view.
    pub(crate) fn printed_traits(&self, t: TraitsRef) -> &'static [Box<str>] {
        self.db.map_or(&[], |db| &db.by_id(t.def).traits)
    }
}

// ---------------------------------------------------------------------------------------------
// Writes, used only while a view is computed.

impl Derived {
    pub(super) fn write_slot(&self, uid: Uid) -> Result<usize, EngineError> {
        self.slot(uid)
            .ok_or_else(|| ir_bug(&format!("uid {uid} outside a view of {} cards", self.len())))
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
        self.present[i] = true;
        self.ap[i] = ap;
        self.hp[i] = hp;
        self.kw[i] = KwAmounts::default();
        self.traits[i] = TraitsRef {
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
        if self.abilities[i].len == 0 {
            self.hosts.push(uid);
        }
        self.abilities[i] = span;
        Ok(())
    }

    /// `kws[kw] = kws.get(kw, 0) + amount` for a stacking keyword, else `kws[kw] = 1`
    /// (`engine/view.py:292-295`, `:376-379`).
    pub(super) fn add_keyword(&mut self, i: usize, kw: Kw, amount: i64) -> Result<(), EngineError> {
        let slot = &mut self.kw[i].0[kw as usize];
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
        let old = self.rules[i];
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
        self.rules[i] = region(start, old.len as usize + 1, "derived view rules")?;
        Ok(())
    }

    /// `dv.traits[t] = cur + new` (`engine/view.py:384`): the granted part moves to the pool's
    /// tail with `new` appended.
    pub(super) fn grant_traits(&mut self, i: usize, new: &[StrId]) -> Result<(), EngineError> {
        if new.is_empty() {
            return Ok(());
        }
        let old = self.traits[i].granted;
        let start = self.trait_pool.len();
        self.trait_pool.extend_from_within(old.range());
        self.trait_pool.extend_from_slice(new);
        self.traits[i].granted =
            region(start, old.len as usize + new.len(), "derived view traits")?;
        Ok(())
    }
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
/// is cloned out of the cell and the borrow released, the key is computed **before** any
/// compute, the view is recomputed with no borrow held when the cache is empty or its key
/// differs, and the result is stored with its key. `touch()` is the only invalidation. It
/// fills the cache of whatever state it is called on, the AI's true states included.
///
/// # Errors
/// What `_compute` raises (`MinOf([])` `ValueError`, `view.py:878`; a hook's error; an
/// integer that leaves its Rust type). The cache is then unchanged, as in Python.
pub fn derived(st: &GameState) -> Result<Arc<Derived>, EngineError> {
    let cached = st.cache.borrow().clone();
    let key = view_key(st);
    if let Some(dv) = cached
        && dv.key == key
    {
        #[cfg(feature = "paranoid")]
        super::paranoid::cached_view(st, &dv)?;
        return Ok(dv);
    }
    let mut dv = compute(st)?;
    dv.key = key;
    let dv = Arc::new(dv);
    *st.cache.borrow_mut() = Some(Arc::clone(&dv));
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

/// `_base_view(st, granted, prev)` (`engine/view.py:245-311`). Pass 1 sets the stats,
/// printed traits, links and empty keyword maps of BATTLE then BASE cards, p0 then p1;
/// pass 2 sets the abilities (BATTLE, BASE, PAIRED, HAND, TRASH per player) and the keywords
/// of BATTLE/BASE hosts. Gates read `prev`, or the view being built when there is none
/// (Inv 05 §5.5).
fn base_view(
    st: &GameState,
    granted: &Granted,
    prev: Option<&Derived>,
) -> Result<Derived, EngineError> {
    let reg = &**st.registry();
    let db = st.db();
    let mut dv = blank(st.cards.len(), Some(db), UNREAD_KEY);
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
                    dv.linked[i] = true;
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
    let mut abil = Vec::new();
    for p in [0u8, 1] {
        let battle = st.zone(p, Zone::Battle);
        let bases = st.zone(p, Zone::Base);
        for &uid in battle.iter().chain(bases) {
            host_abilities(st, prev.unwrap_or(&dv), uid, granted, &mut abil)?;
            if abil.is_empty() {
                continue;
            }
            dv.set_abilities(uid, &abil)?;
            let i = dv.write_slot(uid)?;
            for &aid in &abil {
                if let Node::Keyword(k) = ability_node(reg, aid) {
                    dv.add_keyword(i, k.keyword, k.amount)?;
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
                host_abilities(st, prev.unwrap_or(&dv), uid, granted, &mut abil)?;
                if !abil.is_empty() {
                    dv.set_abilities(uid, &abil)?;
                }
            }
        }
    }
    Ok(dv)
}

/// `_copy_base(base)` (`engine/view.py:336-344`): the stats, keywords and traits copied,
/// `linked` and `abilities` the base's, `rules`, `cost_mod` and `level_mod` empty.
fn copy_base(base: &Derived) -> Derived {
    let n = base.len();
    Derived {
        key: UNREAD_KEY,
        present: base.present.clone(),
        ap: base.ap.clone(),
        hp: base.hp.clone(),
        kw: base.kw.clone(),
        traits: base.traits.clone(),
        trait_pool: base.trait_pool.clone(),
        rules: vec![Span::EMPTY; n],
        rule_pool: Vec::new(),
        rule_kinds: Vec::new(),
        cost_mod: vec![None; n],
        level_mod: vec![None; n],
        linked: base.linked.clone(),
        abilities: base.abilities.clone(),
        ability_pool: base.ability_pool.clone(),
        hosts: base.hosts.clone(),
        converged: true,
        db: base.db,
        by_event: OnceLock::new(),
    }
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
                    if dv.present(t) {
                        let i = dv.write_slot(t)?;
                        dv.hp[i] = add(dv.hp[i], hp)?;
                    }
                    continue;
                }
                if dv.present(t) {
                    let i = dv.write_slot(t)?;
                    dv.ap[i] = add(dv.ap[i], ap)?;
                    dv.hp[i] = add(dv.hp[i], hp)?;
                }
            }
        }
        ContView::KeywordGrant { keyword, amount: a } => {
            let amt = amount(st, view, ctx, a)?;
            for &t in targets {
                if dv.present(t) {
                    let i = dv.write_slot(t)?;
                    dv.add_keyword(i, keyword, amt)?;
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
                let i = dv.write_slot(t)?;
                dv.grant_traits(i, &new)?;
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
                if cm != 0 {
                    let v = i64::from(dv.cost_mod[i].unwrap_or(0)) + cm;
                    dv.cost_mod[i] = Some(to_i32(v)?);
                }
                if lm != 0 {
                    let v = i64::from(dv.level_mod[i].unwrap_or(0)) + lm;
                    dv.level_mod[i] = Some(to_i32(v)?);
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
    let mut live: Vec<Uid> = Vec::new();
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
    Ok(())
}

/// `_finalize(dv)` (`engine/view.py:477-483`): negative AP and HP become 0, once, at the end.
fn finalize(dv: &mut Derived) {
    for i in 0..dv.len() {
        if dv.present[i] {
            dv.ap[i] = dv.ap[i].max(0);
            dv.hp[i] = dv.hp[i].max(0);
        }
    }
}

/// `_compute(st)` (`engine/view.py:409-463`), ported literally: the base view; the early
/// exit when nothing is lasting and no constant exists; then at most [`FIXPOINT_PASSES`]
/// Jacobi passes that read the previous pass (the base in pass 0) and write a fresh view,
/// converged when `same_values` and the granted map both repeat (never in pass 0); the last
/// pass wins at the cap (W-48: `converged` is then false); `_finalize` runs once.
///
/// # Errors
/// What a condition, value, reference or hook raises.
pub(crate) fn compute(st: &GameState) -> Result<Derived, EngineError> {
    let reg = &**st.registry();
    let mut granted = Granted::default();
    let base = base_view(st, &granted, None)?;
    let has_constants = base.hosts().iter().any(|&h| {
        base.abilities_slice(h)
            .iter()
            .any(|&aid| matches!(ability_node(reg, aid), Node::Constant(_)))
    });
    if st.lasting.is_empty() && !has_constants {
        let mut base = base;
        finalize(&mut base);
        return Ok(base);
    }
    let mut prev: Option<Derived> = None;
    let mut converged = false;
    for _ in 0..FIXPOINT_PASSES {
        let view = prev.as_ref().unwrap_or(&base);
        let mut new_granted = Granted::default();
        let mut cur = if granted.is_empty() {
            copy_base(&base)
        } else {
            base_view(st, &granted, prev.as_ref())?
        };
        apply_constants(st, &mut cur, view, &mut new_granted)?;
        apply_lasting(st, &mut cur, view, &mut new_granted)?;
        let done = prev.as_ref().is_some_and(|p| same_values(&cur, p)) && new_granted == granted;
        prev = Some(cur);
        if done {
            converged = true;
            break;
        }
        granted = new_granted;
    }
    let mut dv = prev.unwrap_or(base);
    dv.converged = converged;
    finalize(&mut dv);
    Ok(dv)
}

// ---------------------------------------------------------------------------------------------
// same_values and the trigger index

/// `dv.traits[u] == other.traits[u]` for a uid present in both: the printed traits of the
/// two defs as strings (equal defs need no lookup), then the granted traits. Granted traits
/// are `StrId`s of one registry, where an id names one string; both callers compare views of
/// one game (the fixpoint, `ai/actions.py:230`), so the printed/granted split is the same
/// whenever the defs are.
fn traits_equal(
    a: &Derived,
    ta: TraitsRef,
    ga: &[StrId],
    b: &Derived,
    tb: TraitsRef,
    gb: &[StrId],
) -> bool {
    (ta.def == tb.def || a.printed_traits(ta) == b.printed_traits(tb)) && ga == gb
}

/// `a.same_values(b)` (`engine/view.py:94-106`): `ap`, `hp`, `kw`, `traits`, `cost_mod` and
/// `level_mod` equal as dicts (presence included), the per-uid rule lists equal by `RuleMod`
/// (controller, source, key and aux ignored) and the per-uid aid lists equal. `linked`,
/// `by_event` and `key` are not compared. Uids outside a view are absent keys.
#[must_use]
pub fn same_values(a: &Derived, b: &Derived) -> bool {
    let n = a.len().max(b.len());
    (0..n).all(|i| {
        let Ok(u) = Uid::try_from(i) else {
            return false;
        };
        if a.present(u) != b.present(u) {
            return false;
        }
        if let (Some((ta, ga)), Some((tb, gb))) = (a.traits_get(u), b.traits_get(u))
            && (a.ap_get(u) != b.ap_get(u)
                || a.hp_get(u) != b.hp_get(u)
                || a.kw_get(u) != b.kw_get(u)
                || !traits_equal(a, ta, ga, b, tb, gb))
        {
            return false;
        }
        a.cost_mod_get(u) == b.cost_mod_get(u)
            && a.level_mod_get(u) == b.level_mod_get(u)
            && a.rules_slice(u).len() == b.rules_slice(u).len()
            && a.rules_slice(u)
                .iter()
                .zip(b.rules_slice(u))
                .all(|(x, y)| x.rule == y.rule)
            && a.abilities_slice(u) == b.abilities_slice(u)
    })
}

/// Builds `trigger_index(st, dv)` (`engine/view.py:320-331`): for the active player, then the
/// other, each of BATTLE, BASE, PAIRED, HAND, TRASH in list order, each host's
/// `dv.abilities` with a trigger event, grouped by event (stable).
pub(crate) fn build_index(st: &GameState, dv: &Derived) -> TriggerIndex {
    let reg = &**st.registry();
    let mut found: Vec<(usize, Uid, AbilityId)> = Vec::new();
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
                        found.push((ev as usize, uid, aid));
                    }
                }
            }
        }
    }
    let mut offs = [0u32; N_EVENTS + 1];
    for &(ev, _, _) in &found {
        offs[ev + 1] += 1;
    }
    for i in 0..N_EVENTS {
        offs[i + 1] += offs[i];
    }
    let mut next = offs;
    let mut items = vec![(0, AbilityId(0)); found.len()];
    for &(ev, uid, aid) in &found {
        items[next[ev] as usize] = (uid, aid);
        next[ev] += 1;
    }
    TriggerIndex {
        offs,
        items: items.into_boxed_slice(),
    }
}

/// `trigger_index(st, dv)` (`engine/view.py:314-333`): built once from the zones of the
/// state that asks first and kept on the view (`dv.by_event`, shared by every holder of the
/// view, Inv 11 hazard H2).
pub fn trigger_index<'a>(st: &GameState, dv: &'a Derived) -> &'a TriggerIndex {
    dv.by_event.get_or_init(|| build_index(st, dv))
}
