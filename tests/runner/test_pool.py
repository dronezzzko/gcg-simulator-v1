"""Worker processes: results independent of workers, progress, failures, and interrupts."""

from __future__ import annotations

import dataclasses
import multiprocessing
from pathlib import Path

import pytest

from gcg_sim.reports import build_results
from gcg_sim.reports.writer import to_json_text
from gcg_sim.runner import BenchmarkConfig, FactoryRef, GameFailure, run_benchmark

ILLEGAL_AGENT = f"{Path(__file__).resolve().with_name('random_agent.py')}:make_illegal_agent"


def test_results_do_not_depend_on_workers(
    config: BenchmarkConfig, random_factory: FactoryRef
) -> None:
    serial = run_benchmark(config, agent_factory=random_factory)
    parallel = run_benchmark(dataclasses.replace(config, workers=3), agent_factory=random_factory)
    assert serial.matches == parallel.matches
    assert to_json_text(build_results(serial)) == to_json_text(build_results(parallel))
    assert serial.timing.workers == 1
    assert parallel.timing.workers == 3


@pytest.mark.parametrize("workers", [1, 2])
def test_progress_reports_every_match(
    config: BenchmarkConfig, random_factory: FactoryRef, workers: int
) -> None:
    calls: list[tuple[int, int]] = []
    run_benchmark(
        dataclasses.replace(config, workers=workers),
        lambda done, total: calls.append((done, total)),
        agent_factory=random_factory,
    )
    assert calls == [(i, config.matches) for i in range(1, config.matches + 1)]


def test_worker_failures_propagate_and_stop_the_pool(config: BenchmarkConfig) -> None:
    with pytest.raises(GameFailure, match=r"^match \d+ game 1 \(seed \d+\) failed: Illegal"):
        run_benchmark(
            dataclasses.replace(config, workers=2), agent_factory=FactoryRef(ILLEGAL_AGENT)
        )
    assert multiprocessing.active_children() == []


def test_interrupt_terminates_workers_and_returns_nothing(
    random_factory: FactoryRef, config: BenchmarkConfig
) -> None:
    def interrupt(done: int, total: int) -> None:
        raise KeyboardInterrupt

    long_run = dataclasses.replace(config, matches=2000, workers=2)
    with pytest.raises(KeyboardInterrupt):
        run_benchmark(long_run, interrupt, agent_factory=random_factory)
    assert multiprocessing.active_children() == []
