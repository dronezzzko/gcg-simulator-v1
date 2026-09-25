"""Running a benchmark: matches in worker processes, aggregated in match order.

Every match is a pure function of (configuration, master seed, match index), so the results
never depend on the number of workers or on completion order. Workers are started with the
``spawn`` method and ignore SIGINT; on Ctrl-C (or any failure) the parent cancels pending
matches, terminates the workers, and re-raises, so no partial results are returned.
"""

from __future__ import annotations

import datetime as dt
import multiprocessing
import signal
import time
from collections.abc import Callable
from concurrent.futures import FIRST_COMPLETED, Future, ProcessPoolExecutor, wait

from gcg_sim.runner.agents import AgentFactory, default_agent_factory, describe_factory
from gcg_sim.runner.config import BenchmarkConfig
from gcg_sim.runner.match import play_match
from gcg_sim.runner.records import BenchmarkRun, MatchRecord, RunTiming

Progress = Callable[[int, int], None]
TERMINATE_TIMEOUT_S = 10.0


def _utc_now() -> str:
    return dt.datetime.now(dt.UTC).isoformat(timespec="seconds")


def _timed_match(
    config: BenchmarkConfig, match_index: int, factory: AgentFactory
) -> tuple[MatchRecord, float]:
    start = time.perf_counter()
    record = play_match(config, match_index, agent_factory=factory)
    return record, time.perf_counter() - start


def _ignore_sigint() -> None:
    signal.signal(signal.SIGINT, signal.SIG_IGN)


def _run_serial(
    config: BenchmarkConfig, factory: AgentFactory, progress: Progress | None
) -> list[tuple[MatchRecord, float]]:
    out = []
    for m in range(config.matches):
        out.append(_timed_match(config, m, factory))
        if progress is not None:
            progress(m + 1, config.matches)
    return out


def _terminate(executor: ProcessPoolExecutor) -> None:
    processes = list(executor._processes.values())
    executor.shutdown(wait=False, cancel_futures=True)
    for p in processes:
        p.terminate()
    for p in processes:
        p.join(TERMINATE_TIMEOUT_S)


def _collect(
    futures: dict[Future[tuple[MatchRecord, float]], int],
    total: int,
    progress: Progress | None,
) -> dict[int, tuple[MatchRecord, float]]:
    results: dict[int, tuple[MatchRecord, float]] = {}
    pending = set(futures)
    while pending:
        done, pending = wait(pending, return_when=FIRST_COMPLETED)
        for fut in sorted(done, key=futures.__getitem__):
            results[futures[fut]] = fut.result()
            if progress is not None:
                progress(len(results), total)
    return results


def _run_parallel(
    config: BenchmarkConfig, factory: AgentFactory, progress: Progress | None
) -> list[tuple[MatchRecord, float]]:
    workers = min(config.workers, config.matches)
    executor = ProcessPoolExecutor(
        max_workers=workers,
        mp_context=multiprocessing.get_context("spawn"),
        initializer=_ignore_sigint,
    )
    try:
        futures = {
            executor.submit(_timed_match, config, m, factory): m for m in range(config.matches)
        }
        results = _collect(futures, config.matches, progress)
    except BaseException:
        _terminate(executor)
        raise
    executor.shutdown(wait=True)
    return [results[m] for m in range(config.matches)]


def run_benchmark(
    config: BenchmarkConfig,
    progress: Progress | None = None,
    *,
    agent_factory: AgentFactory | None = None,
) -> BenchmarkRun:
    """Play ``config.matches`` matches and return them in match order.

    ``progress(done, total)`` is called in the calling process after each finished match.
    ``agent_factory`` replaces the search AI (tests); with ``workers > 1`` it must be
    picklable, e.g. a module-level function or a :class:`FactoryRef`.
    """
    factory = agent_factory or default_agent_factory
    started_at = _utc_now()
    start = time.perf_counter()
    if config.workers <= 1 or config.matches == 1:
        results = _run_serial(config, factory, progress)
    else:
        results = _run_parallel(config, factory, progress)
    timing = RunTiming(
        started_at=started_at,
        finished_at=_utc_now(),
        wall_seconds=time.perf_counter() - start,
        match_seconds=tuple(seconds for _, seconds in results),
        workers=config.workers,
    )
    return BenchmarkRun(
        config=config,
        matches=tuple(record for record, _ in results),
        timing=timing,
        agent_factory=describe_factory(factory),
    )
