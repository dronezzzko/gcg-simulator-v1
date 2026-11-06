//! P6-C's mechanisms (docs/dev/PLAN.md §12.2, §12.3 S4, S5): hidden information.
//!
//! | Flag | Item | Module |
//! | --- | --- | --- |
//! | `opponent_buckets{m}` | W-05 | `buckets.rs`: opponent nodes select by the statistics of their world's bucket once it has `m` visits |
//! | `belief_worlds{M}`, `belief_last_n{N}` | W-07 | `belief.rs`: M candidate worlds per world, weighted by the opponent's last public decision under the playout policy's softmax |
//! | `opponent_view_share` | W-139 | `view.rs`: a share of worlds re-determinises the searcher's hidden cards from the opponent's view (keeping the cards the root options name); opponent nodes select by those iterations |
//! | `exact_pools` | W-08 | every world (and every candidate and opponent-view world) is determinised with conservation-exact pools (`observe::determinize_exact_into`), never padded or trimmed |
//! | `opponent_list = archetype` | W-134 | `archetype.rs`: the opponent's unseen cards come from a posterior over a prior of candidate decklists, never from its true decklist; the root's Burst density follows the posterior |
//!
//! The world of index k is built in this order: the source state (the information-set state,
//! or with `archetype` that state with the opponent decklists of the candidate world k draws),
//! the candidate draw of `belief_worlds` (or one world) determinised from that source (exactly
//! with `exact_pools`), then for an opponent-view index the redraw of `view.rs`.
//!
//! Everything here reads the information-set state, the worlds determinised from it and the
//! world seeds `W[0:cap]` the core drew from the per-decision generator, so the results depend
//! on the information set and the seed alone, for any worker count. With every P6-C flag off
//! no function of this module runs (`Composed` calls the reference), so the null configuration
//! is untouched.

pub(crate) mod archetype;
pub(crate) mod belief;
pub(crate) mod buckets;
pub(crate) mod view;

#[cfg(test)]
mod gapb_tests;
#[cfg(test)]
mod tests;

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::Arc;

use gcg_ai::Density;
use gcg_engine::observe::{determinize_exact_into, determinize_into};
use gcg_engine::{EngineError, GameState, Uid};

use super::{BackupCx, ChildIdx, NodeCx, RootCx, WorldCx, reference};
use crate::tree::{Child, ChildKey};
use crate::{MechanismFlags, OpponentList};

/// The mechanism flags this package implements.
pub(crate) const PROVIDES: [&str; 6] = [
    "belief_last_n",
    "belief_worlds",
    "exact_pools",
    "opponent_buckets",
    "opponent_list",
    "opponent_view_share",
];

/// Refuses P6-C settings that do nothing: `belief_last_n` without `belief_worlds` or with no
/// decision weighed, and fewer than two candidate worlds.
///
/// # Errors
/// The first such setting, as a message.
pub(crate) fn check(f: &MechanismFlags) -> Result<(), String> {
    if f.belief_last_n.is_some() && f.belief_worlds.is_none() {
        return Err("belief_last_n needs belief_worlds".to_owned());
    }
    if f.belief_last_n == Some(0) {
        return Err("belief_last_n must weigh at least 1 decision".to_owned());
    }
    if f.belief_worlds.is_some_and(|m| m < 2) {
        return Err("belief_worlds needs at least 2 candidate worlds".to_owned());
    }
    Ok(())
}

/// Whether a P6-C flag changes how worlds are built.
pub(crate) fn shapes_worlds(f: &MechanismFlags) -> bool {
    f.belief_worlds.is_some() || f.opponent_view_share > 0.0 || f.exact_pools || archetype_on(f)
}

/// Whether the opponent's unseen cards come from a prior of candidate lists (W-134).
pub(crate) fn archetype_on(f: &MechanismFlags) -> bool {
    f.opponent_list == OpponentList::Archetype
}

/// Whether a P6-C flag changes the selection or the backup of opponent nodes.
pub(crate) fn shapes_tree(f: &MechanismFlags) -> bool {
    f.opponent_buckets.is_some() || f.opponent_view_share > 0.0
}

/// One statistic of an opponent child over a subset of the iterations (`_Child`'s `visits`,
/// `total` and `avail`, `ai/mcts.py:47-57`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Slot {
    pub(crate) visits: u32,
    pub(crate) total: f64,
    pub(crate) avail: u32,
}

impl Slot {
    fn add(&mut self, value: f64) {
        self.visits += 1;
        self.total += value;
    }
}

/// The statistics of one opponent child besides its pooled `Child`: one slot per bucket of the
/// opponent's determinised hand (W-05) and one over the opponent-view iterations (W-139).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Slices {
    pub(crate) buckets: [Slot; buckets::BUCKETS],
    pub(crate) view: Slot,
    /// The child's key (the opponent's decision kind and move), for diagnostics.
    pub(crate) key: Option<ChildKey>,
}

/// The per-search state of the package (one search per `Composed`; a root tree of S1 carries
/// its own through [`Shared::tree_copy`]).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Search {
    /// The searching player, from the root or the first world.
    pub(crate) player: Option<u8>,
    /// `opponent_view_share` is on: opponent-view worlds are built at this decision.
    pub(crate) view_ok: bool,
    /// The cards the root options name that the opponent cannot identify: an opponent-view
    /// world keeps their identities so that every root move plays the card it names
    /// (`view.rs`).
    pub(crate) view_keep: Vec<Uid>,
    /// The root node and its children's visits when the tree started (`subtree_reuse` keeps
    /// visits; the k-th new visit of a root child plays in world k).
    pub(crate) root: Option<u32>,
    pub(crate) base: Vec<(u32, u32)>,
    /// The running iteration plays in an opponent-view world.
    pub(crate) iter_view: bool,
    /// The opponent children the running iteration selected, with their world's bucket.
    pub(crate) walk: Vec<(u32, u8)>,
    /// The slices of every opponent child that has been selected, by child index.
    pub(crate) slices: BTreeMap<u32, Slices>,
    /// `belief_worlds`: the opponent's reconstructed decision at this decision, once computed.
    pub(crate) evidence: Option<Option<belief::Evidence>>,
    /// Candidate worlds scored by `belief_worlds` (counted as auxiliary applies).
    pub(crate) applies: u64,
    /// Worlds whose candidate draw `belief_worlds` changed from the reference's (diagnostic).
    pub(crate) reweighted: u64,
    /// Opponent-view worlds built (diagnostic).
    pub(crate) view_worlds: u64,
    /// Opponent-view worlds that kept the identities of cards the root options name
    /// (diagnostic).
    pub(crate) view_kept_worlds: u64,
    /// `opponent_list = archetype`: the prior (the packaged lists unless a test set one).
    pub(crate) prior: Option<Arc<archetype::Prior>>,
    /// `opponent_list = archetype`: the posterior of this decision, once computed.
    pub(crate) posterior: Option<Box<archetype::Posterior>>,
}

/// [`Search`] behind a `RefCell`: the mechanisms take `&self`, and each search runs on one
/// thread.
#[derive(Debug, Default)]
pub(crate) struct Shared(RefCell<Search>);

impl Shared {
    pub(crate) const fn new() -> Self {
        Self(RefCell::new(Search {
            player: None,
            view_ok: false,
            view_keep: Vec::new(),
            root: None,
            base: Vec::new(),
            iter_view: false,
            walk: Vec::new(),
            slices: BTreeMap::new(),
            evidence: None,
            applies: 0,
            reweighted: 0,
            view_worlds: 0,
            view_kept_worlds: 0,
            prior: None,
            posterior: None,
        }))
    }

    /// A search whose `opponent_list = archetype` mode uses `prior` (tests).
    #[cfg(test)]
    pub(crate) fn with_prior(prior: archetype::Prior) -> Self {
        let s = Self::new();
        s.with_mut(|x| x.prior = Some(Arc::new(prior)));
        s
    }

    pub(crate) const fn from_search(s: Search) -> Self {
        Self(RefCell::new(s))
    }

    pub(crate) fn into_search(self) -> Search {
        self.0.into_inner()
    }

    /// The state a root tree of this decision starts from: the decision's findings (the
    /// player, the opponent-view test, the reconstructed decision), nothing of this tree.
    pub(crate) fn tree_copy(&self) -> Search {
        let s = self.0.borrow();
        Search {
            player: s.player,
            view_ok: s.view_ok,
            view_keep: s.view_keep.clone(),
            evidence: s.evidence,
            prior: s.prior.clone(),
            posterior: s.posterior.clone(),
            ..Search::default()
        }
    }

    /// The auxiliary applies this search spent (`SearchEffort::applies_aux`).
    pub(crate) fn applies(&self) -> u64 {
        self.0.borrow().applies
    }

    /// The reweighted and the opponent-view worlds this search built (`MechanismEffort`).
    pub(crate) fn worlds(&self) -> (u64, u64) {
        let s = self.0.borrow();
        (s.reweighted, s.view_worlds)
    }

    fn with<T>(&self, f: impl FnOnce(&Search) -> T) -> T {
        f(&self.0.borrow())
    }

    fn with_mut<T>(&self, f: impl FnOnce(&mut Search) -> T) -> T {
        f(&mut self.0.borrow_mut())
    }
}

/// Notes the root decision before its root moves: the searching player and, with
/// `opponent_view_share`, whether opponent-view worlds may be used at it.
pub(crate) fn note_root(f: &MechanismFlags, cx: &RootCx<'_>, shared: &Shared) {
    let view_ok = f.opponent_view_share > 0.0;
    let keep = if view_ok {
        view::named_hidden(cx.st, cx.dec, cx.player)
    } else {
        Vec::new()
    };
    shared.with_mut(|s| {
        s.player = Some(cx.player);
        s.view_ok = view_ok;
        s.view_keep = keep;
    });
}

/// Computes the posterior of `opponent_list = archetype` at the information-set state `st` of
/// `player`, once per search.
///
/// # Errors
/// The prior's or `with_decklists`' errors.
fn ensure_posterior(st: &GameState, player: u8, shared: &Shared) -> Result<(), EngineError> {
    if shared.with(|s| s.posterior.is_some()) {
        return Ok(());
    }
    let prior = match shared.with(|s| s.prior.clone()) {
        Some(p) => p,
        None => archetype::default_prior(st)?,
    };
    let p = Box::new(archetype::Posterior::of(st, player, &prior)?);
    shared.with_mut(|s| {
        s.prior = Some(prior);
        s.posterior = Some(p);
    });
    Ok(())
}

/// The root's Burst density (`ai/mcts.py:120`): with `opponent_list = archetype` the
/// posterior's (`archetype.rs`), else the reference's.
///
/// # Errors
/// The posterior's errors.
pub(crate) fn density(
    f: &MechanismFlags,
    cx: &RootCx<'_>,
    shared: &Shared,
) -> Result<Density, EngineError> {
    if archetype_on(f) {
        ensure_posterior(cx.st, cx.player, shared)?;
        Ok(shared.with(|s| {
            s.posterior
                .as_ref()
                .map_or_else(|| reference::density(cx), |p| p.density)
        }))
    } else {
        Ok(reference::density(cx))
    }
}

/// One world of `source` with seed `seed` into `out`: `determinize` (the reference), or with
/// `exact_pools` its conservation-exact form.
///
/// # Errors
/// `determinize`'s errors, or the exact pools' conservation failure.
fn base_world(
    exact: bool,
    source: &GameState,
    player: u8,
    seed: u64,
    out: &mut GameState,
) -> Result<(), EngineError> {
    if exact {
        determinize_exact_into(source, player, seed, &[], out)
    } else {
        determinize_into(source, player, seed, out)
    }
}

/// World `k` (`determinize(st, player, W[k])` in the reference): from the source state of
/// `opponent_list` (`archetype.rs`), with `belief_worlds` the candidate the opponent's
/// reconstructed decision weighs (`belief.rs`), each determinised exactly with `exact_pools`,
/// then, for an opponent-view index, re-determinised from the opponent's view (`view.rs`).
///
/// # Errors
/// `determinize`'s errors.
pub(crate) fn world(
    f: &MechanismFlags,
    cx: &mut WorldCx<'_>,
    k: usize,
    out: &mut GameState,
    shared: &Shared,
) -> Result<(), EngineError> {
    shared.with_mut(|s| s.player = Some(cx.player));
    if archetype_on(f) {
        ensure_posterior(cx.st, cx.player, shared)?;
    }
    let evidence = f.belief_worlds.and_then(|_| {
        shared.with_mut(|s| {
            *s.evidence
                .get_or_insert_with(|| belief::Evidence::of(cx.st, cx.player))
        })
    });
    let belief = shared.with(|s| -> Result<Option<(u64, bool)>, EngineError> {
        let source: &GameState = match &s.posterior {
            Some(p) => &p.entries[p.draw(cx.seeds[k])].state,
            None => cx.st,
        };
        let player = cx.player;
        let build =
            |seed: u64, out: &mut GameState| base_world(f.exact_pools, source, player, seed, out);
        match (f.belief_worlds, evidence) {
            (Some(m), Some(e)) => belief::world(cx, k, m, &e, out, &build).map(Some),
            _ => build(cx.seeds[k], out).map(|()| None),
        }
    })?;
    if let Some((scored, changed)) = belief {
        shared.with_mut(|s| {
            s.applies += scored;
            s.reweighted += u64::from(changed);
        });
    }
    let view = shared.with(|s| s.view_ok) && view::is_view_world(k, f.opponent_view_share);
    if view {
        let keep = shared.with(|s| s.view_keep.clone());
        view::redraw(out, cx.player, cx.seeds[k], &keep, f.exact_pools)?;
        shared.with_mut(|s| {
            s.view_worlds += 1;
            s.view_kept_worlds += u64::from(!keep.is_empty());
        });
    }
    Ok(())
}

/// Notes the root node of the tree and its children's visits at the tree's start, at the
/// first priors call of the search (`new_tree`'s root priors, after the children exist).
pub(crate) fn note_priors(cx: &NodeCx<'_>, shared: &Shared) {
    shared.with_mut(|s| {
        if s.root.is_some() {
            return;
        }
        s.root = Some(cx.node);
        s.base = cx
            .keys
            .iter()
            .filter_map(|k| cx.tree.get(cx.node, k))
            .map(|c| (c, cx.tree.children[c as usize].visits))
            .collect();
    });
}

/// Whether the node of `cx` is the opponent's.
fn opponent_node(cx: &NodeCx<'_>, s: &Search) -> bool {
    s.player.is_some_and(|p| cx.dec.hdr.player != p)
}

/// The selection at an opponent node, or `None` at another node: the opponent-view slice once
/// it has [`view::MIN_VISITS`] visits, else the bucket slice of this world once it has `m`
/// visits (`buckets.rs`), else the reference's selection over the pooled statistics. The
/// selected child joins the iteration's walk with its bucket.
pub(crate) fn select(f: &MechanismFlags, cx: &mut NodeCx<'_>, shared: &Shared) -> Option<ChildIdx> {
    if !shared.with(|s| opponent_node(cx, s)) {
        return None;
    }
    let bucket = f
        .opponent_buckets
        .map_or(0, |_| buckets::bucket(cx.s, cx.dec.hdr.player));
    let choice = shared.with_mut(|s| {
        let iter_view = s.iter_view;
        for (&c, &key) in cx.children.iter().zip(cx.keys) {
            let e = s.slices.entry(c).or_default();
            e.key = Some(key);
            e.buckets[usize::from(bucket)].avail += 1;
            if iter_view {
                e.view.avail += 1;
            }
        }
        let view_slots = (f.opponent_view_share > 0.0)
            .then(|| slots(cx.children, s, |e| e.view))
            .filter(|v| sum_visits(v) >= view::MIN_VISITS);
        let bucket_slots = f
            .opponent_buckets
            .and_then(|m| {
                let v = slots(cx.children, s, |e| e.buckets[usize::from(bucket)]);
                (sum_visits(&v) >= m).then_some(v)
            })
            .filter(|_| view_slots.is_none());
        view_slots.or(bucket_slots)
    });
    let i = match choice {
        Some(slots) => select_slots(cx, &slots),
        None => reference::select(cx),
    };
    let child = cx.children[i.0];
    shared.with_mut(|s| s.walk.push((child, bucket)));
    Some(i)
}

/// After the selection of a node that is not the opponent's: at the root, the running
/// iteration starts, plays in world `k = visits - base` of the chosen child, and is an
/// opponent-view iteration when world k is one.
pub(crate) fn after_select(f: &MechanismFlags, cx: &NodeCx<'_>, i: ChildIdx, shared: &Shared) {
    shared.with_mut(|s| {
        if s.root != Some(cx.node) {
            return;
        }
        let child = cx.children[i.0];
        let base = s
            .base
            .iter()
            .find_map(|&(c, v)| (c == child).then_some(v))
            .unwrap_or(0);
        let k = cx.tree.children[child as usize].visits.saturating_sub(base) as usize;
        s.iter_view = s.view_ok && view::is_view_world(k, f.opponent_view_share);
        s.walk.clear();
    });
}

/// The backup of one line (`ai/mcts.py:214-217`, with `visited_set_backup`'s once-per-iteration
/// rule when it is on). In an opponent-view iteration the searcher's own children do not learn
/// from the line, which was played with hidden cards the searcher does not hold (W-139): the
/// root child gains its visit (it indexes the worlds) at its current mean, deeper searcher
/// children are left as they were. The opponent's children always gain the value; afterwards
/// the slices of the walk's opponent children gain it too.
///
/// Returns whether this function backed the pooled statistics up (an opponent-view iteration);
/// otherwise the caller runs the pooled backup.
pub(crate) fn backup(f: &MechanismFlags, cx: &mut BackupCx<'_>, shared: &Shared) -> bool {
    let view_iteration = shared.with(|s| s.iter_view);
    if view_iteration {
        let root = cx.path.first().copied();
        for (i, &c) in cx.path.iter().enumerate() {
            if f.visited_set_backup && cx.path[..i].contains(&c) {
                continue;
            }
            let child = &mut cx.tree.children[c as usize];
            if child.mover != cx.player {
                child.visits += 1;
                child.total += 1.0 - cx.value;
            } else if Some(c) == root {
                let mean = Child::mean(child);
                child.visits += 1;
                child.total += mean;
            }
        }
    }
    let value = 1.0 - cx.value;
    shared.with_mut(|s| {
        let walk = std::mem::take(&mut s.walk);
        let mut done: Vec<u32> = Vec::with_capacity(walk.len());
        for &(c, b) in &walk {
            if f.visited_set_backup {
                if done.contains(&c) {
                    continue;
                }
                done.push(c);
            }
            let e = s.slices.entry(c).or_default();
            e.buckets[usize::from(b)].add(value);
            if view_iteration {
                e.view.add(value);
            }
        }
        s.walk = walk;
        s.walk.clear();
    });
    view_iteration
}

/// The slot `pick` of every child, with a default slot for a child never selected.
fn slots(children: &[u32], s: &Search, pick: impl Fn(&Slices) -> Slot) -> Vec<Slot> {
    children
        .iter()
        .map(|c| s.slices.get(c).map(&pick).unwrap_or_default())
        .collect()
}

fn sum_visits(slots: &[Slot]) -> u32 {
    slots.iter().map(|s| s.visits).sum()
}

/// `_select`'s UCB with the progressive prior (`ai/mcts.py:290-302`) over one slice: a child
/// the slice has not seen is valued by its pooled mean and explored as one visit. The first
/// maximum wins, as in the reference.
fn select_slots(cx: &NodeCx<'_>, slots: &[Slot]) -> ChildIdx {
    let (c, w, math) = (cx.cfg.exploration, cx.cfg.prior_weight, cx.math);
    let mut best: Option<(usize, f64)> = None;
    for (i, (&ci, slot)) in cx.children.iter().zip(slots).enumerate() {
        let ch = &cx.tree.children[ci as usize];
        let mean = if slot.visits == 0 {
            ch.mean()
        } else {
            slot.total / f64::from(slot.visits)
        };
        let n = f64::from(slot.visits.max(1));
        let avail = f64::from(slot.avail.max(1));
        let v = mean
            + c * math.sqrt(math.ln(avail) / n)
            + w * ch.prior / (f64::from(slot.visits) + 1.0);
        if best.is_none_or(|(_, bv)| v > bv) {
            best = Some((i, v));
        }
    }
    ChildIdx(best.expect("a decision with options").0)
}
