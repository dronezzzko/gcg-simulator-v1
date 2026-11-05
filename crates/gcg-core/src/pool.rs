//! Deterministic thread pool on `std::thread::scope` (docs/dev/PLAN.md §7.1, X8, X20).
//!
//! `Pool::map_indexed` runs one task per index and returns the results by index, so they are
//! identical for any thread count and schedule: nothing reads the time, the thread id or the
//! completion order. Workers claim indices in increasing order from one atomic counter. A
//! task can fork jobs with `TaskCx::fork_join`; the jobs go to one shared queue, and every
//! worker that has no task to claim runs them ("help first"), as does the forking task while
//! it waits for its own. Waiting is a plain blocking `Condvar` wait: no spinning, sleep,
//! back-off, timeout, QoS class or core pinning.
//!
//! Failure and interrupt are separate signals (W-149). A failing task lowers the run's
//! `lowest_failed` with `fetch_min`; from then on no index is claimed (every unclaimed index
//! is higher than the failed one, because claims go in index order). A running task above it
//! sees [`TaskCx::superseded`] and may stop between decisions, every lower one finishes, and
//! the pool reports the lowest failing index, which is what the serial path of
//! `runner/run.py:45-53` reports. [`Cancel`] is SIGINT only (PLAN §7.5).

use std::any::Any;
use std::collections::VecDeque;
use std::num::NonZeroUsize;
use std::panic::{self, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread;

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
    /// (docs/dev/contracts.md, gcg-ai section).
    pub fn run_inline<S, T>(f: impl FnOnce(&mut S, &TaskCx<'_, S>) -> T) -> T
    where
        S: Default + Send + 'static,
    {
        let shared = Shared::new(false);
        let mut scratch = S::default();
        f(
            &mut scratch,
            &TaskCx {
                shared: &shared,
                index: 0,
            },
        )
    }

    /// Runs `task(i)` for `i` in `0..n` and returns the results in index order, whatever the
    /// schedule (`runner/run.py:88-106`: `results[m] for m in range(config.matches)`).
    ///
    /// `min(threads, n)` workers play, as `processes_used` (`runner/run.py:83-85`); with one,
    /// every task runs inline on the calling thread in index order (`_run_serial`,
    /// `runner/run.py:45-53`). Each worker owns one scratch `S` for all of its tasks.
    /// `on_done(count)` runs on the calling thread after each successful completion, with the
    /// number of completions so far (`progress(len(results), total)`, `runner/run.py:67-80`),
    /// and stops once a failure or an interrupt is seen, as Python's progress stops when
    /// `fut.result()` raises.
    ///
    /// # Errors
    /// [`PoolError::Cancelled`] when `cancel` is set (checked before every claim and after the
    /// run, and it wins over task errors, as `KeyboardInterrupt` does in `cli.py:397-399`);
    /// otherwise [`PoolError::Task`] with the lowest failing index and its error (W-149).
    ///
    /// # Panics
    /// Re-raises, on the calling thread, the panic of the lowest panicking task after every
    /// worker has stopped.
    pub fn map_indexed<S, T, E>(
        &self,
        n: usize,
        cancel: &Cancel,
        task: impl Fn(usize, &mut S, &TaskCx<'_, S>) -> Result<T, E> + Sync,
        on_done: impl FnMut(usize),
    ) -> Result<Vec<T>, PoolError<E>>
    where
        S: Default + Send + 'static,
        T: Send,
        E: Send,
    {
        let workers = self.threads.get().min(n);
        if workers <= 1 {
            return run_serial(n, cancel, &task, on_done);
        }
        run_parallel(workers, n, cancel, &task, on_done)
    }
}

/// `_run_serial` (`runner/run.py:45-53`): tasks in index order on the calling thread; the
/// first failure stops the run.
fn run_serial<S, T, E>(
    n: usize,
    cancel: &Cancel,
    task: &(impl Fn(usize, &mut S, &TaskCx<'_, S>) -> Result<T, E> + Sync),
    mut on_done: impl FnMut(usize),
) -> Result<Vec<T>, PoolError<E>>
where
    S: Default + Send + 'static,
{
    let shared = Shared::new(false);
    let mut scratch = S::default();
    let mut out = Vec::with_capacity(n);
    for index in 0..n {
        if cancel.is_cancelled() {
            return Err(PoolError::Cancelled);
        }
        let cx = TaskCx {
            shared: &shared,
            index,
        };
        match task(index, &mut scratch, &cx) {
            Ok(value) => {
                out.push(value);
                on_done(index + 1);
            }
            Err(_) if cancel.is_cancelled() => return Err(PoolError::Cancelled),
            Err(error) => return Err(PoolError::Task { index, error }),
        }
    }
    if cancel.is_cancelled() {
        return Err(PoolError::Cancelled);
    }
    Ok(out)
}

/// The per-run state the workers of `run_parallel` share besides [`Shared`].
struct Run<'r, S, F> {
    n: usize,
    cancel: &'r Cancel,
    task: &'r F,
    shared: Shared<S>,
    /// The next index to claim (`fetch_add`), on its own cache line.
    next: CachePadded<AtomicUsize>,
    /// The lowest index whose task panicked, with its payload.
    panicked: Mutex<Option<(usize, Box<dyn Any + Send>)>>,
}

impl<S, F> Run<'_, S, F> {
    /// No index is claimed after a failure, a panic or an interrupt.
    fn stop_claiming(&self) -> bool {
        self.shared.lowest_failed.0.load(Ordering::SeqCst) != usize::MAX
            || self.cancel.is_cancelled()
    }

    /// The next thing a worker does: a queued job first (it shortens the running tasks, so
    /// fast cores never wait behind a straggler, PLAN §7.3), then a new index, else a blocking
    /// wait until a job is queued or the last running task ends.
    fn next_work(&self) -> Work<S> {
        let mut st = self.shared.lock();
        loop {
            if let Some(job) = st.jobs.pop_front() {
                return Work::Job(job);
            }
            if !st.claims_closed {
                if self.stop_claiming() {
                    st.claims_closed = true;
                } else {
                    let index = self.next.0.fetch_add(1, Ordering::SeqCst);
                    if index < self.n {
                        st.active += 1;
                        return Work::Task(index);
                    }
                    st.claims_closed = true;
                }
            }
            if st.active == 0 {
                self.shared.wake.notify_all();
                return Work::Done;
            }
            st = self.shared.wait(st);
        }
    }

    fn record_panic(&self, index: usize, payload: Box<dyn Any + Send>) {
        let mut slot = self.panicked.lock().unwrap_or_else(PoisonError::into_inner);
        if slot.as_ref().is_none_or(|(lowest, _)| index < *lowest) {
            *slot = Some((index, payload));
        }
        // A panic stops the run like a failure (no further claims).
        self.shared
            .lowest_failed
            .0
            .fetch_min(index, Ordering::SeqCst);
    }
}

enum Work<S> {
    Job(QueuedJob<S>),
    Task(usize),
    Done,
}

/// Marks a claimed task finished, also when it unwinds, so that idle workers never wait for
/// a task that will not end.
struct ActiveGuard<'a, S>(&'a Shared<S>);

impl<S> Drop for ActiveGuard<'_, S> {
    fn drop(&mut self) {
        let mut st = self.0.lock();
        st.active -= 1;
        drop(st);
        self.0.wake.notify_all();
    }
}

fn worker<S, T, E, F>(run: &Run<'_, S, F>, results: &mpsc::Sender<(usize, Result<T, E>)>)
where
    S: Default + Send + 'static,
    F: Fn(usize, &mut S, &TaskCx<'_, S>) -> Result<T, E> + Sync,
{
    let mut scratch = S::default();
    loop {
        match run.next_work() {
            Work::Job(job) => run.shared.run_job(job),
            Work::Task(index) => {
                let cx = TaskCx {
                    shared: &run.shared,
                    index,
                };
                let guard = ActiveGuard(&run.shared);
                let outcome =
                    panic::catch_unwind(AssertUnwindSafe(|| (run.task)(index, &mut scratch, &cx)));
                match outcome {
                    Ok(result) => {
                        if result.is_err() {
                            run.shared
                                .lowest_failed
                                .0
                                .fetch_min(index, Ordering::SeqCst);
                        }
                        drop(guard);
                        // The receiver lives until every worker has exited.
                        let _ = results.send((index, result));
                    }
                    Err(payload) => {
                        run.record_panic(index, payload);
                        // The scratch may be half-updated by the unwound task.
                        scratch = S::default();
                        drop(guard);
                    }
                }
            }
            Work::Done => return,
        }
    }
}

fn run_parallel<S, T, E, F>(
    workers: usize,
    n: usize,
    cancel: &Cancel,
    task: &F,
    mut on_done: impl FnMut(usize),
) -> Result<Vec<T>, PoolError<E>>
where
    S: Default + Send + 'static,
    T: Send,
    E: Send,
    F: Fn(usize, &mut S, &TaskCx<'_, S>) -> Result<T, E> + Sync,
{
    let run = Run {
        n,
        cancel,
        task,
        shared: Shared::new(true),
        next: CachePadded(AtomicUsize::new(0)),
        panicked: Mutex::new(None),
    };
    let mut slots: Vec<Option<T>> = std::iter::repeat_with(|| None).take(n).collect();
    let mut failure: Option<(usize, E)> = None;
    thread::scope(|scope| {
        let (tx, rx) = mpsc::channel();
        for _ in 0..workers {
            let tx = tx.clone();
            let run = &run;
            scope.spawn(move || worker(run, &tx));
        }
        drop(tx);
        let mut done = 0;
        let mut quiet = false;
        // Ends when every worker has exited and dropped its sender.
        for (index, result) in rx {
            match result {
                Ok(value) => {
                    slots[index] = Some(value);
                    if !quiet && run.cancel.is_cancelled() {
                        quiet = true;
                    }
                    if !quiet {
                        done += 1;
                        on_done(done);
                    }
                }
                Err(error) => {
                    quiet = true;
                    if failure.as_ref().is_none_or(|(lowest, _)| index < *lowest) {
                        failure = Some((index, error));
                    }
                }
            }
        }
    });
    let panicked = run
        .panicked
        .into_inner()
        .unwrap_or_else(PoisonError::into_inner);
    if let Some((_, payload)) = panicked {
        panic::resume_unwind(payload);
    }
    if cancel.is_cancelled() {
        return Err(PoolError::Cancelled);
    }
    if let Some((index, error)) = failure {
        return Err(PoolError::Task { index, error });
    }
    let mut out = Vec::with_capacity(n);
    for slot in slots {
        match slot {
            Some(value) => out.push(value),
            None => unreachable!("without a failure or an interrupt every index was claimed"),
        }
    }
    Ok(out)
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
/// already running worker (docs/dev/contracts.md, open item O1).
pub(crate) type Job<S> = Box<dyn FnOnce(&mut S) + Send>;

/// A queued job and the fork it belongs to, so that a waiting task can run its own first.
pub(crate) struct QueuedJob<S> {
    fork: u64,
    run: Job<S>,
}

/// What the queue mutex protects.
pub(crate) struct QueueState<S> {
    /// Queued fork-join jobs, oldest first (one queue, one mutex: tasks are >= 1 ms, X8).
    jobs: VecDeque<QueuedJob<S>>,
    /// Claimed tasks that have not finished.
    active: usize,
    /// No further index will be claimed (all claimed, or a failure or interrupt was seen).
    claims_closed: bool,
}

/// Aligns a hot shared value to the 128-byte cache line of the M2 Pro (`hw.cachelinesize`),
/// so that two workers never write the same line (PLAN §7.1).
#[repr(align(128))]
struct CachePadded<T>(T);

/// State shared by the workers of one run, and by every [`TaskCx`] handed out in it.
pub(crate) struct Shared<S> {
    /// `false` for inline runs (`threads == 1`, `run_inline`): `fork_join` then runs its jobs
    /// in input order on the calling thread.
    parallel: bool,
    /// The job queue and the task counts, on their own cache line.
    state: CachePadded<Mutex<QueueState<S>>>,
    /// Signalled when a job is queued or finishes, and when a task ends.
    wake: Condvar,
    /// Free scratch values for jobs; grows on demand and lives for the whole run.
    scratch: Mutex<Vec<Box<S>>>,
    /// Fork ids, so that a waiting task finds its own queued jobs.
    next_fork: AtomicU64,
    /// The lowest failing (or panicking) task index, `usize::MAX` while none failed (W-149);
    /// read by [`TaskCx::superseded`]. Inline runs never set it: their first failure ends the
    /// run.
    lowest_failed: CachePadded<AtomicUsize>,
}

impl<S> Shared<S> {
    /// An empty queue and an empty scratch free list.
    const fn new(parallel: bool) -> Self {
        Self {
            parallel,
            state: CachePadded(Mutex::new(QueueState {
                jobs: VecDeque::new(),
                active: 0,
                claims_closed: false,
            })),
            wake: Condvar::new(),
            scratch: Mutex::new(Vec::new()),
            next_fork: AtomicU64::new(0),
            lowest_failed: CachePadded(AtomicUsize::new(usize::MAX)),
        }
    }

    /// The queue lock. A poisoned lock still guards consistent data: every critical section
    /// below is a single push, pop or counter update.
    fn lock(&self) -> MutexGuard<'_, QueueState<S>> {
        self.state.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn wait<'g>(&self, guard: MutexGuard<'g, QueueState<S>>) -> MutexGuard<'g, QueueState<S>> {
        self.wake
            .wait(guard)
            .unwrap_or_else(PoisonError::into_inner)
    }
}

impl<S: Default> Shared<S> {
    /// Runs `f` on a scratch value borrowed from the free list (a new one if it is empty) and
    /// returns the value to the list.
    fn with_scratch<R>(&self, f: impl FnOnce(&mut S) -> R) -> R {
        let popped = self
            .scratch
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pop();
        let mut scratch = popped.unwrap_or_default();
        let result = f(&mut scratch);
        self.scratch
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(scratch);
        result
    }

    /// Runs a queued job, then wakes every waiter: the forking task may be waiting for it.
    fn run_job(&self, job: QueuedJob<S>) {
        self.with_scratch(job.run);
        let st = self.lock();
        drop(st);
        self.wake.notify_all();
    }
}

/// The results of one `fork_join` call, filled by whichever thread runs each job.
struct Fork<T> {
    slots: Mutex<Vec<Option<thread::Result<T>>>>,
    remaining: AtomicUsize,
}

impl<T> Fork<T> {
    fn new(n: usize) -> Self {
        Self {
            slots: Mutex::new(std::iter::repeat_with(|| None).take(n).collect()),
            remaining: AtomicUsize::new(n),
        }
    }

    fn complete(&self, index: usize, result: thread::Result<T>) {
        self.slots.lock().unwrap_or_else(PoisonError::into_inner)[index] = Some(result);
        self.remaining.fetch_sub(1, Ordering::SeqCst);
    }

    fn is_done(&self) -> bool {
        self.remaining.load(Ordering::SeqCst) == 0
    }

    /// The results in input order; the first panicked job (in input order) is re-raised.
    fn take_results(&self) -> Vec<T> {
        let slots = std::mem::take(&mut *self.slots.lock().unwrap_or_else(PoisonError::into_inner));
        let mut out = Vec::with_capacity(slots.len());
        for slot in slots {
            match slot {
                Some(Ok(value)) => out.push(value),
                Some(Err(payload)) => panic::resume_unwind(payload),
                None => unreachable!("fork_join returns only after every job completed"),
            }
        }
        out
    }
}

/// The fork-join handle a running task receives (PLAN §7.1; `AgentCx::par`, §6.1): the run's
/// shared queue and scratch free list, and the index of the task it was handed to.
pub struct TaskCx<'p, S> {
    pub(crate) shared: &'p Shared<S>,
    /// The task's index in `0..n` (`0` for `Pool::run_inline`).
    pub(crate) index: usize,
}

impl<S> TaskCx<'_, S> {
    /// Whether a lower index of this run has failed or panicked, so that this task's result
    /// can no longer be reported: `true` when the task's index is above the run's
    /// `lowest_failed` (W-149, PLAN §7.1). A running match checks it between decisions, as it
    /// checks [`Cancel`], and stops early; whatever it then returns is never reported, because
    /// the pool reports the lowest failing index (`runner/run.py:45-53`). Always `false` in an
    /// inline run (`threads == 1`, `Pool::run_inline`), where the first failure ends the run
    /// before any higher index starts. A relaxed load, cheap enough for every decision.
    #[must_use]
    pub fn superseded(&self) -> bool {
        self.index > self.shared.lowest_failed.0.load(Ordering::Relaxed)
    }
}

impl<S: Default + Send + 'static> TaskCx<'_, S> {
    /// Fork-join inside a task: queues jobs 1.. on the run's shared queue, runs job 0 itself,
    /// then runs queued jobs (its own first) until all of its jobs are done ("help first",
    /// blocking waits, no spinning or timeouts). Each job borrows a scratch `S` from the free
    /// list. Returns results in input order. In an inline run (`threads == 1`,
    /// `Pool::run_inline`) the jobs run on the calling thread in input order.
    ///
    /// Callers: the play/draw simulations of `first_player_choice` (`ai/setup.py:86-111`,
    /// seeds drawn up front at `:98`) and the strengthened search's root trees (PLAN §7.3,
    /// §12.3); each job owns its state clone and seed, so the results never depend on which
    /// thread ran which job.
    ///
    /// # Panics
    /// Re-raises the panic of the first panicking job (in input order) after all jobs ended.
    #[allow(
        clippy::type_complexity,
        reason = "the signature frozen in docs/dev/contracts.md (gcg-core, pool)"
    )]
    pub fn fork_join<I, T>(
        &self,
        inputs: Vec<I>,
        f: Arc<dyn Fn(usize, I, &mut S) -> T + Send + Sync>,
    ) -> Vec<T>
    where
        I: Send + 'static,
        T: Send + 'static,
    {
        let shared = self.shared;
        let n = inputs.len();
        if !shared.parallel || n <= 1 {
            return inputs
                .into_iter()
                .enumerate()
                .map(|(i, input)| shared.with_scratch(|s| f(i, input, s)))
                .collect();
        }
        let fork = Arc::new(Fork::new(n));
        let id = shared.next_fork.fetch_add(1, Ordering::Relaxed);
        let mut inputs = inputs.into_iter().enumerate();
        let first = inputs.next();
        {
            let mut st = shared.lock();
            for (i, input) in inputs {
                let fork = Arc::clone(&fork);
                let f = Arc::clone(&f);
                st.jobs.push_back(QueuedJob {
                    fork: id,
                    run: Box::new(move |s: &mut S| {
                        let result = panic::catch_unwind(AssertUnwindSafe(|| f(i, input, s)));
                        fork.complete(i, result);
                    }),
                });
            }
            drop(st);
            shared.wake.notify_all();
        }
        if let Some((i, input)) = first {
            let result =
                shared.with_scratch(|s| panic::catch_unwind(AssertUnwindSafe(|| f(i, input, s))));
            fork.complete(i, result);
        }
        loop {
            let job = {
                let mut st = shared.lock();
                loop {
                    if fork.is_done() {
                        break None;
                    }
                    if let Some(pos) = st.jobs.iter().position(|job| job.fork == id) {
                        break st.jobs.remove(pos);
                    }
                    if let Some(job) = st.jobs.pop_front() {
                        break Some(job);
                    }
                    st = shared.wait(st);
                }
            };
            match job {
                Some(job) => shared.run_job(job),
                None => break,
            }
        }
        fork.take_results()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::SplitMix64;
    use std::sync::atomic::AtomicU32;

    fn pool(threads: usize) -> Pool {
        Pool::new(NonZeroUsize::new(threads).expect("threads >= 1"))
    }

    /// A deterministic task: a few thousand draws seeded by the index, plus a nested fork of
    /// eight jobs whose results are summed in input order (as `first_player_choice` does).
    fn work(i: usize, scratch: &mut Vec<u64>, cx: &TaskCx<'_, Vec<u64>>) -> u64 {
        let mut rng = SplitMix64::new(crate::derive_seed!(7_u64, "pool", i));
        scratch.clear();
        for _ in 0..1000 {
            scratch.push(rng.next_u64());
        }
        let seeds: Vec<u64> = (0..8).map(|_| rng.next_u64()).collect();
        let jobs = cx.fork_join(
            seeds,
            Arc::new(|j: usize, seed: u64, s: &mut Vec<u64>| {
                let mut r = SplitMix64::new(seed);
                s.clear();
                s.extend((0..500).map(|_| r.next_u64() >> 3));
                s.iter()
                    .fold(j as u64, |acc, x| acc.wrapping_mul(31).wrapping_add(*x))
            }),
        );
        let own = scratch.iter().fold(0_u64, |acc, x| acc.wrapping_add(*x));
        jobs.iter().fold(own, |acc, x| acc.wrapping_add(*x))
    }

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

    #[test]
    fn run_inline_fork_join_runs_in_input_order() {
        let order = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&order);
        let out = Pool::run_inline(|_s: &mut u8, par: &TaskCx<'_, u8>| {
            par.fork_join(
                vec![10, 20, 30],
                Arc::new(move |i: usize, x: u32, _s: &mut u8| {
                    seen.lock().expect("lock").push(i);
                    x + 1
                }),
            )
        });
        assert_eq!(out, [11, 21, 31]);
        assert_eq!(*order.lock().expect("lock"), [0, 1, 2]);
    }

    #[test]
    fn results_are_identical_for_any_worker_count() {
        let cancel = Cancel::new();
        let reference: Vec<u64> = pool(1)
            .map_indexed(64, &cancel, |i, s, cx| Ok::<_, ()>(work(i, s, cx)), |_| {})
            .expect("serial run");
        for threads in [2, 3, 4, 7, 10, 16, 64, 100] {
            let parallel = pool(threads)
                .map_indexed(64, &cancel, |i, s, cx| Ok::<_, ()>(work(i, s, cx)), |_| {})
                .expect("parallel run");
            assert_eq!(parallel, reference, "threads = {threads}");
        }
    }

    #[test]
    fn progress_counts_every_completion_in_order() {
        for threads in [1, 2, 10] {
            let mut counts = Vec::new();
            let out = pool(threads)
                .map_indexed(
                    25,
                    &Cancel::new(),
                    |i, _s: &mut (), _cx| Ok::<_, ()>(i * 2),
                    |done| counts.push(done),
                )
                .expect("run");
            assert_eq!(out, (0..25).map(|i| i * 2).collect::<Vec<_>>());
            assert_eq!(counts, (1..=25).collect::<Vec<_>>(), "threads = {threads}");
        }
    }

    #[test]
    fn empty_runs_return_nothing() {
        for threads in [1, 4] {
            let mut calls = 0;
            let out = pool(threads)
                .map_indexed(
                    0,
                    &Cancel::new(),
                    |_, _: &mut (), _| Ok::<u8, ()>(1),
                    |_| calls += 1,
                )
                .expect("run");
            assert!(out.is_empty());
            assert_eq!(calls, 0);
        }
    }

    /// A one-shot gate: tasks block on it (a plain condvar wait) until it opens.
    #[derive(Default)]
    struct Gate {
        open: Mutex<bool>,
        cv: Condvar,
    }

    impl Gate {
        fn open(&self) {
            *self.open.lock().expect("lock") = true;
            self.cv.notify_all();
        }

        fn wait(&self) {
            let mut open = self.open.lock().expect("lock");
            while !*open {
                open = self.cv.wait(open).expect("wait");
            }
        }
    }

    #[test]
    fn the_lowest_failing_index_is_reported_even_when_a_higher_one_fails_first() {
        // Task 3 fails only after task 7 has failed, so the pool sees 7 first (T-074).
        let gate = Gate::default();
        let ran: Vec<AtomicBool> = (0..40).map(|_| AtomicBool::new(false)).collect();
        let result = pool(10).map_indexed(
            40,
            &Cancel::new(),
            |i, _s: &mut (), _cx| {
                ran[i].store(true, Ordering::SeqCst);
                match i {
                    3 => {
                        gate.wait();
                        Err(format!("failure {i}"))
                    }
                    7 => {
                        gate.open();
                        Err(format!("failure {i}"))
                    }
                    _ => Ok(i),
                }
            },
            |_| {},
        );
        assert_eq!(
            result,
            Err(PoolError::Task {
                index: 3,
                error: "failure 3".to_owned()
            })
        );
        // Lower indices always finish.
        for (i, flag) in ran.iter().enumerate().take(3) {
            assert!(flag.load(Ordering::SeqCst), "task {i} ran");
        }
    }

    #[test]
    fn the_serial_path_stops_at_the_first_failure() {
        let ran = AtomicU32::new(0);
        let result = pool(1).map_indexed(
            10,
            &Cancel::new(),
            |i, _s: &mut (), _cx| {
                ran.fetch_add(1, Ordering::SeqCst);
                if i == 3 || i == 7 { Err(i) } else { Ok(i) }
            },
            |_| {},
        );
        assert_eq!(result, Err(PoolError::Task { index: 3, error: 3 }));
        assert_eq!(ran.load(Ordering::SeqCst), 4);
    }

    #[test]
    fn failures_at_3_and_7_under_ten_workers_report_3() {
        for _ in 0..20 {
            let result = pool(10).map_indexed(
                200,
                &Cancel::new(),
                |i, _s: &mut (), _cx| if i == 3 || i == 7 { Err(i) } else { Ok(i) },
                |_| {},
            );
            assert_eq!(result, Err(PoolError::Task { index: 3, error: 3 }));
        }
    }

    #[test]
    fn no_index_is_claimed_after_a_failure_is_recorded() {
        let cancel = Cancel::new();
        let task = |_: usize, _: &mut (), _: &TaskCx<'_, ()>| Ok::<(), ()>(());
        let run = Run {
            n: 100,
            cancel: &cancel,
            task: &task,
            shared: Shared::<()>::new(true),
            next: CachePadded(AtomicUsize::new(0)),
            panicked: Mutex::new(None),
        };
        assert!(matches!(run.next_work(), Work::Task(0)));
        assert!(matches!(run.next_work(), Work::Task(1)));
        run.shared.lowest_failed.0.fetch_min(1, Ordering::SeqCst);
        // Both claimed tasks end; the next request closes the claims and finds nothing.
        drop(ActiveGuard(&run.shared));
        drop(ActiveGuard(&run.shared));
        assert!(matches!(run.next_work(), Work::Done));
        assert_eq!(run.next.0.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn superseded_is_true_only_above_the_lowest_failure() {
        let shared = Shared::<()>::new(true);
        let cx = |index| TaskCx {
            shared: &shared,
            index,
        };
        assert!(!cx(0).superseded());
        assert!(!cx(usize::MAX - 1).superseded());
        shared.lowest_failed.0.fetch_min(5, Ordering::SeqCst);
        assert!(!cx(4).superseded());
        assert!(!cx(5).superseded());
        assert!(cx(6).superseded());
        // A lower failure later lowers the bound again.
        shared.lowest_failed.0.fetch_min(2, Ordering::SeqCst);
        assert!(!cx(2).superseded());
        assert!(cx(3).superseded());
        // A higher failure never raises it.
        shared.lowest_failed.0.fetch_min(9, Ordering::SeqCst);
        assert!(cx(3).superseded());
    }

    #[test]
    fn a_running_task_above_a_failure_is_superseded_and_lower_ones_are_not() {
        for _ in 0..10 {
            // Ten workers: task 3 fails once task 7 runs. Task 7 plays "decisions" until it
            // sees that a lower index failed, as a match checks between decisions, so it can
            // only stop that way. Task 2 looks after task 7 stopped: lower indices are never
            // superseded and finish.
            let started = Gate::default();
            let stopped = Gate::default();
            let lower_superseded = AtomicBool::new(true);
            let result = pool(10).map_indexed(
                10,
                &Cancel::new(),
                |i, _s: &mut (), cx| match i {
                    2 => {
                        stopped.wait();
                        lower_superseded.store(cx.superseded(), Ordering::SeqCst);
                        Ok(i)
                    }
                    3 => {
                        started.wait();
                        Err(format!("failure {i}"))
                    }
                    7 => {
                        started.open();
                        let mut decisions = 0_u64;
                        while !cx.superseded() {
                            decisions += 1;
                            thread::yield_now();
                        }
                        stopped.open();
                        Err(format!("stopped after {decisions} decisions"))
                    }
                    _ => Ok(i),
                },
                |_| {},
            );
            assert_eq!(
                result,
                Err(PoolError::Task {
                    index: 3,
                    error: "failure 3".to_owned()
                })
            );
            assert!(!lower_superseded.load(Ordering::SeqCst));
        }
    }

    #[test]
    fn inline_tasks_are_never_superseded() {
        let seen = Mutex::new(Vec::new());
        let result = pool(1).map_indexed(
            6,
            &Cancel::new(),
            |i, _s: &mut (), cx| {
                seen.lock().expect("lock").push(cx.superseded());
                if i == 2 { Err(i) } else { Ok(i) }
            },
            |_| {},
        );
        assert_eq!(result, Err(PoolError::Task { index: 2, error: 2 }));
        assert_eq!(*seen.lock().expect("lock"), [false, false, false]);
        assert!(!Pool::run_inline(
            |_s: &mut (), cx: &TaskCx<'_, ()>| cx.superseded()
        ));
    }

    #[test]
    fn an_interrupt_stops_claiming_and_wins_over_failures() {
        for threads in [1, 4] {
            let cancel = Cancel::new();
            let ran = AtomicU32::new(0);
            // Tasks after 2 wait until 2 has set the flag, so at most one task per other
            // worker can be claimed before the interrupt is visible.
            let set = Gate::default();
            let result = pool(threads).map_indexed(
                1000,
                &cancel,
                |i, _s: &mut (), _cx| {
                    ran.fetch_add(1, Ordering::SeqCst);
                    if i == 2 {
                        cancel.flag().store(true, Ordering::Relaxed);
                        set.open();
                        return Err("interrupted");
                    }
                    if i > 2 {
                        set.wait();
                    }
                    Ok(i)
                },
                |_| {},
            );
            assert_eq!(result, Err(PoolError::Cancelled), "threads = {threads}");
            let ran = ran.load(Ordering::SeqCst) as usize;
            assert!(ran <= 3 + threads, "threads = {threads}: {ran} tasks ran");
        }
        let cancel = Cancel::new();
        cancel.flag().store(true, Ordering::Relaxed);
        let ran = AtomicU32::new(0);
        let result = pool(4).map_indexed(
            10,
            &cancel,
            |i, _s: &mut (), _cx| {
                ran.fetch_add(1, Ordering::SeqCst);
                Ok::<_, ()>(i)
            },
            |_| {},
        );
        assert_eq!(result, Err(PoolError::Cancelled));
        assert_eq!(ran.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn fork_join_is_identical_to_serial_and_reuses_scratch() {
        let created = Arc::new(AtomicU32::new(0));
        #[derive(Default)]
        struct Counted(u64);
        let reference: Vec<Vec<u64>> = (0..12)
            .map(|i| (0..9).map(|j| (i * 100 + j) as u64).collect())
            .collect();
        for threads in [1, 3, 10] {
            let made = Arc::clone(&created);
            made.store(0, Ordering::SeqCst);
            let out = pool(threads)
                .map_indexed(
                    12,
                    &Cancel::new(),
                    |i, _s: &mut Counted, cx| {
                        let made = Arc::clone(&made);
                        Ok::<_, ()>(cx.fork_join(
                            (0..9).collect(),
                            Arc::new(move |j: usize, x: usize, s: &mut Counted| {
                                if s.0 == 0 {
                                    made.fetch_add(1, Ordering::SeqCst);
                                }
                                s.0 += 1;
                                assert_eq!(j, x);
                                (i * 100 + x) as u64
                            }),
                        ))
                    },
                    |_| {},
                )
                .expect("run");
            assert_eq!(out, reference, "threads = {threads}");
            // A job never runs on a fresh scratch while a free one exists: at most one
            // scratch per thread that can run a job at the same time.
            assert!(
                created.load(Ordering::SeqCst) as usize <= threads.max(1),
                "threads = {threads}: {} scratch values",
                created.load(Ordering::SeqCst)
            );
        }
    }

    #[test]
    fn idle_workers_help_a_forking_task() {
        // One long task forks 32 jobs; the other workers have no task left and must take
        // jobs, so more than one thread runs them.
        let threads_seen = Arc::new(Mutex::new(Vec::<thread::ThreadId>::new()));
        let gate = Arc::new(Gate::default());
        let seen = Arc::clone(&threads_seen);
        let g = Arc::clone(&gate);
        let out = pool(4)
            .map_indexed(
                2,
                &Cancel::new(),
                move |i, _s: &mut (), cx| {
                    if i == 1 {
                        return Ok::<_, ()>(Vec::new());
                    }
                    let seen = Arc::clone(&seen);
                    let g = Arc::clone(&g);
                    Ok(cx.fork_join(
                        (0..32_u32).collect(),
                        Arc::new(move |j: usize, x: u32, _s: &mut ()| {
                            let id = thread::current().id();
                            let mut ids = seen.lock().expect("lock");
                            if !ids.contains(&id) {
                                ids.push(id);
                            }
                            let distinct = ids.len();
                            drop(ids);
                            // Job 0 (on the forking thread) waits until another thread has
                            // run a job, so the test cannot pass with one thread doing all.
                            if j == 0 && distinct < 2 {
                                g.wait();
                            } else if distinct >= 2 {
                                g.open();
                            }
                            x * 2
                        }),
                    ))
                },
                |_| {},
            )
            .expect("run");
        assert_eq!(out[0], (0..32).map(|x| x * 2).collect::<Vec<u32>>());
        assert!(threads_seen.lock().expect("lock").len() >= 2);
    }

    #[test]
    #[should_panic(expected = "job 5 panicked")]
    fn a_panicking_job_reaches_the_forking_task() {
        let _ = pool(3).map_indexed(
            1,
            &Cancel::new(),
            |_, _s: &mut (), _cx| Ok::<_, ()>(()),
            |_| {},
        );
        let _ = pool(3).map_indexed(
            3,
            &Cancel::new(),
            |i, _s: &mut (), cx| {
                let v = cx.fork_join(
                    (0..8).collect(),
                    Arc::new(move |j: usize, _x: i32, _s: &mut ()| {
                        assert!(!(i == 1 && j == 5), "job 5 panicked");
                        j
                    }),
                );
                Ok::<_, ()>(v.len())
            },
            |_| {},
        );
    }

    #[test]
    #[should_panic(expected = "task 4 panicked")]
    fn a_panicking_task_is_re_raised_after_the_workers_stop() {
        let _ = pool(4).map_indexed(
            50,
            &Cancel::new(),
            |i, _s: &mut (), _cx| {
                assert!(i != 4, "task 4 panicked");
                Ok::<_, ()>(i)
            },
            |_| {},
        );
    }
}
