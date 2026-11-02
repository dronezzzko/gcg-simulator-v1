//! Feature `paranoid` (W-49, W-124): how many times each self-check of `tests/paranoid.py`
//! `CacheChecker` ran on the current thread (`CacheChecker.checks`, `:52`). The checks live in
//! `view/paranoid.rs` and in `core.rs`; these counts let a run show that every check was
//! exercised, as `tests/rules/test_engine_caches.py:41-46` asserts. They never change a game.
//! Gate diagnostics only, never in a shipped build.

use std::cell::Cell;

/// One self-check, named as `CacheChecker.checks` counts it (`tests/paranoid.py:59-132`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Check {
    /// A cache hit of `derived` recomputed and compared (`:59-73`).
    CachedView,
    /// An event `emit` skipped as unlistened, shown to have no trigger candidate (`:86-100`).
    UnlistenedEvent,
    /// A damage scan with damage but none reaching printed HP, shown to destroy nothing
    /// (`:102-111`).
    NoPrintedLethalDamage,
    /// A rules-management check skipped on an unchanged state, run in full (`:113-125`).
    UnchangedState,
    /// A reference resolved without the view, resolved again with it (`:127-132`).
    ViewFreeReference,
    /// A second fixpoint pass that the derived view skipped because every read of the first
    /// pass answers the same in its result, run in full and compared (Rust only: PLAN §11.4
    /// step 4, the transparency check of that shortcut).
    SkippedPass,
}

impl Check {
    /// Every check, in the order of [`counts`].
    pub const ALL: [Self; 6] = [
        Self::CachedView,
        Self::UnlistenedEvent,
        Self::NoPrintedLethalDamage,
        Self::UnchangedState,
        Self::ViewFreeReference,
        Self::SkippedPass,
    ];

    /// The `CacheChecker.checks` key.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::CachedView => "cached view",
            Self::UnlistenedEvent => "unlistened event",
            Self::NoPrintedLethalDamage => "no printed lethal damage",
            Self::UnchangedState => "unchanged state",
            Self::ViewFreeReference => "view-free reference",
            Self::SkippedPass => "skipped fixpoint pass",
        }
    }

    const fn slot(self) -> usize {
        match self {
            Self::CachedView => 0,
            Self::UnlistenedEvent => 1,
            Self::NoPrintedLethalDamage => 2,
            Self::UnchangedState => 3,
            Self::ViewFreeReference => 4,
            Self::SkippedPass => 5,
        }
    }
}

thread_local! {
    static COUNTS: [Cell<u64>; 6] = const { [const { Cell::new(0) }; 6] };
    static MOVED_HOSTS: std::cell::RefCell<Vec<MovedHost>> = const {
        std::cell::RefCell::new(Vec::new())
    };
}

/// W-162: a trigger whose host moved again between queueing and resolution, so the trigger
/// resolves against the card in its new place (rule 4-1-5 makes it a new card there). The
/// engine keeps Python's behaviour; this only records where it happens.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct MovedHost {
    /// The host card's number when the trigger resolves.
    pub card_number: Box<str>,
    /// The host's `zone_seq` when the trigger was queued (after a leave-play move, so a
    /// 【Destroyed】 trigger whose card stays in the trash is not recorded).
    pub queued_seq: i32,
    /// The host's `zone_seq` when the trigger resolves.
    pub resolved_seq: i32,
}

/// W-162: the side table of the trigger list, outside the game's value (never serialised,
/// hashed or compared): for each entry of `GameState.triggers`, in the same order, the host's
/// `zone_seq` at the moment it was queued (`NO_ARG` without a host, or for a trigger loaded
/// from a snapshot), and the queued `zone_seq` of the trigger the list handed out last.
#[derive(Clone, Default, Debug)]
pub(crate) struct TriggerSeqs {
    pub(crate) queued: Vec<i32>,
    pub(crate) taken: i32,
}

/// W-162: `t` resolves now (`_resolve_trigger`, `engine/game.py:712-727`, past its once-key
/// check) with `queued`, the host's `zone_seq` when it was queued; a host whose `zone_seq`
/// changed since is recorded.
pub(crate) fn trigger_resolves(st: &crate::GameState, host: i32, queued: i32) {
    let Ok(uid) = usize::try_from(host) else {
        return;
    };
    let Some(c) = st.cards.get(uid) else {
        return;
    };
    if queued == crate::NO_ARG || c.zone_seq == queued {
        return;
    }
    MOVED_HOSTS.with(|m| {
        m.borrow_mut().push(MovedHost {
            card_number: st.db().by_id(c.def_id).card_number.clone(),
            queued_seq: queued,
            resolved_seq: c.zone_seq,
        });
    });
}

/// W-162: every trigger recorded on this thread so far whose host moved again before it
/// resolved, in resolution order.
#[must_use]
pub fn moved_hosts() -> Vec<MovedHost> {
    MOVED_HOSTS.with(|m| m.borrow().clone())
}

/// `self.checks[name] += 1`.
pub(crate) fn count(check: Check) {
    COUNTS.with(|n| {
        let c = &n[check.slot()];
        c.set(c.get().saturating_add(1));
    });
}

/// How many times each check of [`Check::ALL`] ran on this thread so far.
#[must_use]
pub fn counts() -> [u64; 6] {
    COUNTS.with(|n| n.each_ref().map(Cell::get))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_count_goes_to_its_own_check_on_this_thread() {
        let before = counts();
        count(Check::UnchangedState);
        count(Check::UnchangedState);
        let after = counts();
        for (i, c) in Check::ALL.iter().enumerate() {
            let want = u64::from(*c == Check::UnchangedState) * 2;
            assert_eq!(after[i] - before[i], want, "{}", c.name());
        }
        let other = std::thread::spawn(counts).join().expect("joins");
        assert_eq!(other, [0; Check::ALL.len()]);
    }
}
