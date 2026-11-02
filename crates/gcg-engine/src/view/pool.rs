//! Per-thread recycling of derived-view buffers (docs/port/PLAN.md §11.4 step 1).
//!
//! A view is computed hundreds of thousands of times per game. Each compute works in a
//! [`Work`] set of three buffers taken from the calling thread's pool (the base view and the
//! two fixpoint passes), and the result's contents are swapped into a shared view whose
//! allocation is reused when the pool has one. A view leaving a state's cache (`touch`, a
//! cache replacement, `clone_from_state`) returns here: at once when nothing else holds it,
//! otherwise it waits until its last other holder lets go. Every reused buffer is reset to the
//! state of a fresh view before it is written ([`Derived::reset`]), so reuse can never change
//! a value: the pool is invisible to every reader. Nothing here depends on threads beyond
//! "the pool of the thread that computes or releases".

use std::cell::RefCell;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, OnceLock};

use gcg_data::CardDb;
use gcg_effects::AbilityId;

use super::UNREAD_KEY;
use crate::{Derived, Span, TraitsRef, Uid, ViewSlot};

/// At most this many work sets, shared views (free, or waiting for their other holders) and
/// trigger lists wait in one thread's pool.
const MAX_WORKS: usize = 8;
const MAX_VIEWS: usize = 16;
const MAX_WAITING: usize = 32;
const MAX_INDEX_LISTS: usize = 16;

/// The buffers of one compute: the base view and the two fixpoint passes.
pub(crate) struct Work {
    pub(crate) base: Derived,
    pub(crate) a: Derived,
    pub(crate) b: Derived,
}

struct Pool {
    /// Boxed so that taking and returning a set moves a pointer, not three views.
    #[allow(
        clippy::vec_box,
        reason = "a work set moves in and out of the pool by pointer"
    )]
    works: Vec<Box<Work>>,
    /// Views nothing else holds.
    views: Vec<Arc<Derived>>,
    /// Views that left a cache while something else still held them, oldest first.
    waiting: Vec<Arc<Derived>>,
    index_lists: Vec<Vec<(Uid, AbilityId)>>,
}

thread_local! {
    static POOL: RefCell<Pool> = const {
        RefCell::new(Pool {
            works: Vec::new(),
            views: Vec::new(),
            waiting: Vec::new(),
            index_lists: Vec::new(),
        })
    };
}

/// Runs `f` on this thread's pool, or returns `None` when the pool is unavailable (the thread
/// is exiting, or the pool is already borrowed further up the stack).
fn with_pool<R>(f: impl FnOnce(&mut Pool) -> R) -> Option<R> {
    POOL.try_with(|p| p.try_borrow_mut().ok().map(|mut p| f(&mut p)))
        .ok()
        .flatten()
}

impl ViewSlot {
    /// A slot with every key absent.
    pub(crate) const fn blank(uid: Uid) -> ViewSlot {
        ViewSlot {
            uid,
            present: false,
            linked: false,
            ap: 0,
            hp: 0,
            kw: gcg_effects::KwAmounts([None; 7]),
            traits: TraitsRef {
                def: gcg_data::DefId(0),
                granted: Span::EMPTY,
            },
            rules: Span::EMPTY,
            cost_mod: None,
            level_mod: None,
            abilities: Span::EMPTY,
        }
    }
}

impl Derived {
    /// A view over no uids that holds no buffer (`Derived()`, `engine/view.py:81-92`).
    pub(crate) const fn unallocated() -> Derived {
        Derived {
            key: UNREAD_KEY,
            index: Vec::new(),
            slots: Vec::new(),
            trait_pool: Vec::new(),
            rule_pool: Vec::new(),
            rule_kinds: Vec::new(),
            ability_pool: Vec::new(),
            hosts: Vec::new(),
            hosts_split: 0,
            converged: true,
            db: None,
            by_event: OnceLock::new(),
            scratch_aids: Vec::new(),
            scratch_uids: Vec::new(),
            recording: AtomicBool::new(false),
            reads: Vec::new(),
        }
    }

    /// Turns this buffer into `Derived()` over `n` uids (`engine/view.py:81-92`): every key
    /// absent, no trigger index, converged, keeping the capacities.
    pub(crate) fn reset(&mut self, n: usize, db: Option<&'static CardDb>) {
        self.key = UNREAD_KEY;
        self.index.clear();
        self.index.resize(n, 0);
        self.slots.clear();
        self.trait_pool.clear();
        self.rule_pool.clear();
        self.rule_kinds.clear();
        self.ability_pool.clear();
        self.hosts.clear();
        self.hosts_split = 0;
        self.converged = true;
        self.db = db;
        *self.recording.get_mut() = false;
        self.drop_index();
    }

    /// Empties `by_event`, keeping its list for the next trigger index.
    fn drop_index(&mut self) {
        if let Some(idx) = self.by_event.take() {
            give_index_list(idx.items);
        }
    }

    /// Exchanges the contents of two views. Both trigger indexes must be empty (a compute
    /// never builds one), so `by_event` stays where it is.
    fn swap_contents(&mut self, other: &mut Derived) {
        std::mem::swap(&mut self.key, &mut other.key);
        std::mem::swap(&mut self.index, &mut other.index);
        std::mem::swap(&mut self.slots, &mut other.slots);
        std::mem::swap(&mut self.trait_pool, &mut other.trait_pool);
        std::mem::swap(&mut self.rule_pool, &mut other.rule_pool);
        std::mem::swap(&mut self.rule_kinds, &mut other.rule_kinds);
        std::mem::swap(&mut self.ability_pool, &mut other.ability_pool);
        std::mem::swap(&mut self.hosts, &mut other.hosts);
        std::mem::swap(&mut self.hosts_split, &mut other.hosts_split);
        std::mem::swap(&mut self.converged, &mut other.converged);
        std::mem::swap(&mut self.db, &mut other.db);
        std::mem::swap(&mut self.scratch_aids, &mut other.scratch_aids);
        std::mem::swap(&mut self.scratch_uids, &mut other.scratch_uids);
    }
}

/// A work set for one compute (its buffers are reset by the compute before use).
pub(crate) fn work() -> Box<Work> {
    with_pool(|p| p.works.pop()).flatten().unwrap_or_else(|| {
        Box::new(Work {
            base: Derived::unallocated(),
            a: Derived::unallocated(),
            b: Derived::unallocated(),
        })
    })
}

/// Returns a work set to the pool.
pub(crate) fn give_work(w: Box<Work>) {
    let rest = with_pool(|p| {
        if p.works.len() < MAX_WORKS {
            p.works.push(w);
            None
        } else {
            Some(w)
        }
    });
    drop(rest);
}

/// A uniquely owned view from the pool: a free one, else the oldest waiting view whose other
/// holders are gone.
fn pooled_view(p: &mut Pool) -> Option<Arc<Derived>> {
    if let Some(a) = p.views.pop() {
        return Some(a);
    }
    let i = p.waiting.iter().position(|a| Arc::strong_count(a) == 1)?;
    Some(p.waiting.remove(i))
}

/// A shared view holding the contents of `result`, which receives the reused view's old
/// buffers in exchange (or empty ones).
pub(crate) fn share(result: &mut Derived) -> Arc<Derived> {
    if let Some(mut a) = with_pool(pooled_view).flatten()
        && let Some(inner) = Arc::get_mut(&mut a)
    {
        inner.drop_index();
        inner.swap_contents(result);
        return a;
    }
    let mut out = Derived::unallocated();
    out.swap_contents(result);
    Arc::new(out)
}

/// Releases a view that leaves a state's cache. Nothing is observable: the view's contents
/// are never changed while anything else holds it.
pub(crate) fn recycle(mut a: Arc<Derived>) {
    let unique = Arc::get_mut(&mut a).is_some();
    let rest = with_pool(|p| {
        if unique {
            if p.views.len() < MAX_VIEWS {
                p.views.push(a);
                return None;
            }
            return Some(a);
        }
        if p.waiting.len() >= MAX_WAITING {
            let oldest = p.waiting.remove(0);
            p.waiting.push(a);
            return Some(oldest);
        }
        p.waiting.push(a);
        None
    });
    // A view that found no room is dropped here, after the pool's borrow ended.
    drop(rest);
}

/// An empty list for a trigger index (its capacity reused).
pub(crate) fn index_list() -> Vec<(Uid, AbilityId)> {
    with_pool(|p| p.index_lists.pop())
        .flatten()
        .unwrap_or_default()
}

fn give_index_list(mut items: Vec<(Uid, AbilityId)>) {
    if items.capacity() == 0 {
        return;
    }
    items.clear();
    let rest = with_pool(|p| {
        if p.index_lists.len() < MAX_INDEX_LISTS {
            p.index_lists.push(items);
            None
        } else {
            Some(items)
        }
    });
    drop(rest);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reused_buffer_reads_as_a_fresh_view() {
        let reg = gcg_effects::Registry::embedded();
        let db = reg.db();
        let mut a = Derived::unallocated();
        a.reset(4, Some(db));
        a.set_stats(1, 2, 3, gcg_data::DefId(0)).unwrap();
        a.set_abilities(2, &[AbilityId(5)]).unwrap();
        a.converged = false;
        a.reset(6, Some(db));
        assert_eq!(a.len(), 6);
        assert!(a.slots.is_empty() && a.hosts.is_empty() && a.ability_pool.is_empty());
        assert!(a.index.iter().all(|&k| k == 0));
        assert!(a.converged && a.by_event.get().is_none());
        assert!(super::super::same_values(&a, &Derived::unallocated()));
    }

    #[test]
    fn shared_views_are_reused_only_once_nothing_else_holds_them() {
        let reg = gcg_effects::Registry::embedded();
        let mut r = Derived::unallocated();
        r.reset(3, Some(reg.db()));
        r.set_stats(0, 1, 1, gcg_data::DefId(0)).unwrap();
        let a = share(&mut r);
        assert!(a.present(0) && a.len() == 3);
        assert_eq!(
            r.len(),
            0,
            "the result took the reused view's (empty) buffers"
        );
        let held = Arc::clone(&a);
        recycle(a);
        assert_eq!(Arc::strong_count(&held), 2, "a held view waits, unchanged");
        assert!(held.present(0));
        let mut s = Derived::unallocated();
        s.reset(2, Some(reg.db()));
        let b = share(&mut s);
        assert!(!Arc::ptr_eq(&b, &held), "a view still held is never reused");
        assert!(held.present(0) && held.len() == 3);
        drop(held);
        let mut t = Derived::unallocated();
        t.reset(5, Some(reg.db()));
        let c = share(&mut t);
        assert_eq!(c.len(), 5);
        assert!(!c.present(0));
    }
}
