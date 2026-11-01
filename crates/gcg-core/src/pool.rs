//! Deterministic thread pool on `std::thread::scope` (docs/port/PLAN.md §7.1, X8, X20).
//!
//! Contract types and the lead's `Pool::run_inline`. Workstream E lands `Pool::map_indexed` and
//! `TaskCx::fork_join` with the signatures frozen in docs/port/contracts.md (gcg-core section).
//! Results are stored by index, so they are identical for any thread count and schedule;
//! nothing reads the time, the thread id or the completion order. Waiting is a plain blocking
//! wait: no spinning, sleep, back-off, timeout, QoS class or core pinning (brief
//! `<target_hardware>`).

use std::collections::VecDeque;
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};

/// The SIGINT flag (A3). Only [`crate::sys::sigint`] sets it; workers check it at match start
/// and between decisions, searches between iterations (PLAN §7.5). Failure of a match is a
/// separate signal (`lowest_failed`, W-149), never this flag.
#[derive(Clone, Debug, Default)]
pub struct Cancel {
    flag: Arc<AtomicBool>,
}

impl Cancel {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A relaxed load, cheap enough for every decision and iteration.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::Relaxed)
    }

    /// The flag that `signal_hook::flag::register(SIGINT, ..)` sets (crate-private, so that
    /// SIGINT stays its only writer).
    #[allow(
        dead_code,
        reason = "read by sys::sigint::install when workstream E lands it (docs/port/contracts.md)"
    )]
    pub(crate) fn flag(&self) -> &Arc<AtomicBool> {
        &self.flag
    }
}

/// A pool of `threads` workers, created per run (`--workers`). With `threads == 1` every task
/// and every fork-join job runs inline, in index order, on the calling thread.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Pool {
    threads: NonZeroUsize,
}

impl Pool {
    #[must_use]
    pub const fn new(threads: NonZeroUsize) -> Self {
        Self { threads }
    }

    #[must_use]
    pub const fn threads(&self) -> NonZeroUsize {
        self.threads
    }

    /// Runs `f` once, on the calling thread, as the only task of a one-thread run: `f` gets a
    /// fresh `S::default()` and a [`TaskCx`] whose `fork_join` runs its jobs inline in input
    /// order, as with `threads == 1`. Callers outside `map_indexed` (scenario and agent tests,
    /// `gcg-diff`'s agent replay, tools) build their `AgentCx` inside `f`
    /// (docs/port/contracts.md, gcg-ai section). Workstream E keeps it in step with its
    /// layout of `Shared`.
    pub fn run_inline<S, T>(f: impl FnOnce(&mut S, &TaskCx<'_, S>) -> T) -> T
    where
        S: Default + Send + 'static,
    {
        let shared = Shared::new();
        let mut scratch = S::default();
        f(&mut scratch, &TaskCx { shared: &shared })
    }
}

/// Why `Pool::map_indexed` returned no results.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum PoolError<E> {
    /// The lowest failing task index and its error, which is what the serial path reports
    /// (`runner/run.py:45-53`, W-149): higher indices stop, lower ones finish.
    Task { index: usize, error: E },
    /// [`Cancel`] was set (SIGINT).
    Cancelled,
}

/// A fork-join job: an owned closure that runs on a borrowed scratch value. Jobs own their
/// inputs (`'static`), because safe Rust cannot hand a borrow of the forking task's stack to an
/// already running worker (docs/port/contracts.md, open item O1).
pub(crate) type Job<S> = Box<dyn FnOnce(&mut S) + Send>;

/// State shared by the workers of one run. The layout is private to workstream E, which may
/// change it in its landing diff without the interface change protocol.
#[allow(
    dead_code,
    reason = "read by TaskCx::fork_join and Pool::map_indexed when workstream E lands them"
)]
pub(crate) struct Shared<S> {
    /// Queued fork-join jobs (one queue, one mutex: tasks are >= 1 ms, X8).
    pub(crate) jobs: Mutex<VecDeque<Job<S>>>,
    /// Signalled when a job is queued or the run ends.
    pub(crate) job_ready: Condvar,
    /// Free scratch values for nested jobs; grows on demand and lives for the whole run.
    pub(crate) scratch: Mutex<Vec<Box<S>>>,
}

impl<S> Shared<S> {
    /// An empty queue and an empty scratch free list.
    pub(crate) const fn new() -> Self {
        Self {
            jobs: Mutex::new(VecDeque::new()),
            job_ready: Condvar::new(),
            scratch: Mutex::new(Vec::new()),
        }
    }
}

/// The fork-join handle a running task receives (PLAN §7.1; `AgentCx::par`, §6.1).
pub struct TaskCx<'p, S> {
    #[allow(
        dead_code,
        reason = "read by TaskCx::fork_join when workstream E lands it"
    )]
    pub(crate) shared: &'p Shared<S>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_inline_runs_once_on_a_fresh_scratch() {
        let mut calls = 0;
        let len = Pool::run_inline(|scratch: &mut Vec<u32>, _par: &TaskCx<'_, Vec<u32>>| {
            calls += 1;
            assert!(scratch.is_empty());
            scratch.push(7);
            scratch.len()
        });
        assert_eq!((len, calls), (1, 1));
    }
}
