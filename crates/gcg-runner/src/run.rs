//! Running a benchmark: matches on the worker threads of `gcg_core::pool`, returned in match
//! order (`runner/run.py`; docs/port/inventory/01-rng-runner-cli.md §2.6; PLAN §7).
//!
//! Every match is a pure function of (configuration, master seed, match index), so the results
//! never depend on the number of workers or on completion order. One match is one pool task
//! (PLAN §7.3). The reported failure is the one with the lowest match index, as the serial path
//! reports it (W-149, `runner/run.py:45-53`); higher matches still running stop at their next
//! decision. SIGINT (`Cancel`) stops the run with [`RunError::Interrupted`] and no results.

use std::num::NonZeroUsize;

use gcg_ai::{AgentCx, Scratch};
use gcg_core::pool::{Cancel, Pool, PoolError};
use gcg_core::sys::topology::{self, Topology};

use crate::clock::{Stopwatch, utc_now};
use crate::config::workers_used;
use crate::matches::play_match;
use crate::{AgentFactory, BenchmarkConfig, BenchmarkRun, MatchRecord, RunError, RunTiming};

/// `run_benchmark(config, progress, agent_factory=...)` (`runner/run.py:109-140`): play
/// `cfg.matches` matches on [`workers_used`] threads and return them in match order.
/// `progress(done, total)` runs on the calling thread after each finished match, with the
/// number of matches finished so far (`runner/run.py:67-80`, never a match index).
///
/// # Errors
/// The lowest failing match's [`RunError`], or [`RunError::Interrupted`] once `cancel` is set.
pub fn run_benchmark(
    cfg: &BenchmarkConfig,
    f: &dyn AgentFactory,
    progress: &mut dyn FnMut(u32, u32),
    cancel: &Cancel,
) -> Result<BenchmarkRun, RunError> {
    let mut run = run_matches(
        cfg,
        &topology::detect(),
        f.describe(),
        progress,
        cancel,
        &|m, cx| play_match(cfg, m, f, cx),
    )?;
    run.search = f.search_block();
    Ok(run)
}

/// [`run_benchmark`] over any match player, so the pool orchestration (match order, progress
/// counts, the lowest failing index, interrupts) is testable on scripted matches. The run's
/// `search` is `None`; the caller sets the factory's search block.
pub(crate) fn run_matches(
    cfg: &BenchmarkConfig,
    topology: &Topology,
    agent_factory: String,
    progress: &mut dyn FnMut(u32, u32),
    cancel: &Cancel,
    play: &(dyn Fn(u32, &mut AgentCx<'_>) -> Result<MatchRecord, RunError> + Sync),
) -> Result<BenchmarkRun, RunError> {
    let workers = workers_used(cfg, topology);
    let threads = NonZeroUsize::new(usize::try_from(workers).unwrap_or(usize::MAX))
        .unwrap_or(NonZeroUsize::MIN);
    let n = usize::try_from(cfg.matches).unwrap_or(usize::MAX);
    let started_at = utc_now();
    let wall = Stopwatch::start();
    let results = Pool::new(threads).map_indexed(
        n,
        cancel,
        |index, scratch: &mut Scratch, par| {
            let m = u32::try_from(index).expect("match indices fit in u32 (matches is a u32)");
            let watch = Stopwatch::start();
            let mut cx = AgentCx {
                scratch,
                par,
                cancel,
            };
            let record = play(m, &mut cx)?;
            Ok((record, watch.seconds()))
        },
        |done| progress(u32::try_from(done).unwrap_or(u32::MAX), cfg.matches),
    );
    let results = results.map_err(|e| match e {
        PoolError::Task { error, .. } => error,
        PoolError::Cancelled => RunError::Interrupted,
    })?;
    let wall_seconds = wall.seconds();
    let (matches, match_seconds): (Vec<MatchRecord>, Vec<f64>) = results.into_iter().unzip();
    Ok(BenchmarkRun {
        config: cfg.clone(),
        matches,
        timing: RunTiming {
            started_at,
            finished_at: utc_now(),
            wall_seconds,
            match_seconds,
            workers,
            topology: topology.clone(),
        },
        agent_factory,
        search: None,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use gcg_core::sys::topology::TopologySource;
    use gcg_engine::EndReason;

    use super::*;
    use crate::config::tests::config;
    use crate::game::tests::fake_record;
    use crate::matches::play_match_with;
    use crate::reports::to_json_text;
    use crate::{Outcome, Role, Workers};

    fn topo() -> Topology {
        Topology {
            levels: Vec::new(),
            logical_cpus: 10,
            cache_line_bytes: Some(128),
            arm_features: Vec::new(),
            source: TopologySource::Fallback,
        }
    }

    /// A deterministic scripted match: its outcome depends on the match index only.
    fn scripted_match(cfg: &BenchmarkConfig, m: u32) -> Result<MatchRecord, RunError> {
        play_match_with(cfg, m, &mut |g, chooser| {
            let (winner, point) = match (m + g) % 3 {
                0 => (Outcome::Dut, Some(Role::Dut)),
                1 => (Outcome::Bench, Some(Role::Bench)),
                _ => (Outcome::Draw, None),
            };
            let mut r = fake_record(m, g, chooser, winner, point, EndReason::BattleDamage);
            r.seed = crate::config::game_seed(cfg.seed, m, g);
            r.turns = i32::try_from(m % 17).expect("small");
            Ok(r)
        })
    }

    fn run(
        cfg: &BenchmarkConfig,
        cancel: &Cancel,
        play: &(dyn Fn(u32, &mut AgentCx<'_>) -> Result<MatchRecord, RunError> + Sync),
    ) -> (Result<BenchmarkRun, RunError>, Vec<(u32, u32)>) {
        let mut calls = Vec::new();
        let result = run_matches(
            cfg,
            &topo(),
            "scripted".to_owned(),
            &mut |done, total| calls.push((done, total)),
            cancel,
            play,
        );
        (result, calls)
    }

    #[test]
    fn results_do_not_depend_on_workers() {
        // tests/runner/test_pool.py:18-26 and T-074's identity half: 1, 4 and 10 workers.
        let mut cfg = config();
        cfg.matches = 40;
        let mut docs = Vec::new();
        for workers in [1, 4, 10] {
            cfg.workers = Workers::Count(workers);
            let (result, calls) = run(&cfg, &Cancel::new(), &|m, _| scripted_match(&cfg, m));
            let result = result.expect("scripted matches succeed");
            assert_eq!(result.timing.workers, workers);
            assert_eq!(result.timing.match_seconds.len(), 40);
            let indices: Vec<u32> = result.matches.iter().map(|r| r.match_index).collect();
            assert_eq!(indices, (0..40).collect::<Vec<_>>());
            // tests/runner/test_pool.py:29-39: one call per match, counts in order.
            assert_eq!(calls, (1..=40).map(|i| (i, 40)).collect::<Vec<_>>());
            let games: Vec<String> = result.games().map(|g| to_json_text(&g.to_json())).collect();
            let matches: Vec<String> = result
                .matches
                .iter()
                .map(|m| to_json_text(&m.to_json()))
                .collect();
            docs.push((games, matches));
        }
        assert_eq!(docs[0], docs[1]);
        assert_eq!(docs[0], docs[2]);
    }

    #[test]
    fn the_lowest_failing_match_is_reported() {
        // T-074 / W-149: failures injected at matches 3 and 7 under 10 workers; the run reports
        // match 3 whatever the schedule, as the serial path does (runner/run.py:45-53).
        let mut cfg = config();
        cfg.matches = 40;
        let fail = |m: u32| RunError::GameFailure(format!("match {m} game 1 (seed 0) failed"));
        for workers in [1, 2, 4, 10] {
            cfg.workers = Workers::Count(workers);
            for _ in 0..20 {
                let (result, _) = run(&cfg, &Cancel::new(), &|m, _| {
                    if m == 3 || m == 7 {
                        Err(fail(m))
                    } else {
                        scripted_match(&cfg, m)
                    }
                });
                assert_eq!(result.map(|_| ()), Err(fail(3)), "{workers} workers");
            }
            // Only match 7 fails: it is reported, and the progress stops before the total.
            let (result, calls) = run(&cfg, &Cancel::new(), &|m, _| {
                if m == 7 {
                    Err(fail(m))
                } else {
                    scripted_match(&cfg, m)
                }
            });
            assert_eq!(result.map(|_| ()), Err(fail(7)));
            assert!(calls.len() < 40);
        }
    }

    #[test]
    fn a_lower_failure_supersedes_running_higher_matches() {
        // A match above the failed one sees TaskCx::superseded between decisions and stops;
        // matches below it still finish (W-149). Match 1 fails only after match 2 has seen
        // the claim of match 3 start, so 2 is running when 1 fails.
        let mut cfg = config();
        cfg.matches = 3;
        cfg.workers = Workers::Count(3);
        let started = AtomicUsize::new(0);
        let superseded = Mutex::new(Vec::new());
        let (result, _) = run(&cfg, &Cancel::new(), &|m, cx| {
            started.fetch_add(1, Ordering::SeqCst);
            match m {
                1 => {
                    while started.load(Ordering::SeqCst) < 3 {
                        std::hint::spin_loop();
                    }
                    Err(RunError::GameFailure("match 1 failed".to_owned()))
                }
                2 => {
                    while !cx.par.superseded() {
                        std::hint::spin_loop();
                    }
                    superseded.lock().expect("lock").push(m);
                    Err(RunError::GameFailure("superseded".to_owned()))
                }
                _ => scripted_match(&cfg, m),
            }
        });
        assert_eq!(
            result.map(|_| ()),
            Err(RunError::GameFailure("match 1 failed".to_owned()))
        );
        assert_eq!(*superseded.lock().expect("lock"), [2]);
    }

    #[test]
    fn an_interrupt_returns_no_results() {
        // tests/runner/test_pool.py:50-59: interrupted runs return nothing.
        let mut cfg = config();
        cfg.matches = 2_000;
        for workers in [1, 2] {
            cfg.workers = Workers::Count(workers);
            let cancel = Cancel::new();
            let flagged = AtomicUsize::new(0);
            let (result, calls) = run(&cfg, &cancel, &|m, cx| {
                if m == 5 {
                    cancel_from_test(&cancel);
                    flagged.fetch_add(1, Ordering::SeqCst);
                }
                if m >= 5 {
                    // A match running when SIGINT arrives stops at its next decision.
                    while !cx.cancel.is_cancelled() {
                        std::hint::spin_loop();
                    }
                    return Err(RunError::Interrupted);
                }
                scripted_match(&cfg, m)
            });
            assert_eq!(result.map(|_| ()), Err(RunError::Interrupted));
            assert_eq!(flagged.load(Ordering::SeqCst), 1);
            assert!(calls.len() <= 5, "{calls:?}");
        }
    }

    /// Sets the SIGINT flag the way the signal handler does: by raising SIGINT at this
    /// process after `sys::sigint::install` registered `cancel`.
    fn cancel_from_test(cancel: &Cancel) {
        gcg_core::sys::sigint::install(cancel).expect("handler");
        let pid = std::process::id().to_string();
        let status = std::process::Command::new("kill")
            .args(["-INT", &pid])
            .status()
            .expect("kill runs");
        assert!(status.success());
        while !cancel.is_cancelled() {
            std::hint::spin_loop();
        }
    }

    #[test]
    fn timing_records_the_workers_that_played() {
        // tests/runner/test_pool.py:62-68
        let mut cfg = config();
        cfg.workers = Workers::Count(4);
        for (matches, used) in [(1, 1), (2, 2), (6, 4)] {
            cfg.matches = matches;
            let (result, _) = run(&cfg, &Cancel::new(), &|m, _| scripted_match(&cfg, m));
            let result = result.expect("scripted");
            assert_eq!(result.timing.workers, used);
            assert_eq!(result.agent_factory, "scripted");
            assert_eq!(result.config, cfg);
            assert!(result.timing.started_at <= result.timing.finished_at);
            assert!(result.timing.wall_seconds >= 0.0);
        }
    }
}
