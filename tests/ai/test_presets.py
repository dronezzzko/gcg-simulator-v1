"""Search presets: order, budgets, unchanged existing presets, and deterministic new presets."""

from __future__ import annotations

import dataclasses
from pathlib import Path

import pytest

from gcg_sim.ai import PRESET_SUMMARIES, PRESETS, SearchConfig, make_agent
from gcg_sim.deck import load_deck
from gcg_sim.reports import build_results
from gcg_sim.reports.writer import to_json_text
from gcg_sim.runner import BenchmarkConfig, run_benchmark
from gcg_sim.runner.config import AI_PRESETS, DEFAULT_TURN_LIMIT

EXAMPLES = Path(__file__).resolve().parents[2] / "examples" / "decks"
BY_BUDGET = ["fast", "standard", "strong", "expert"]


def test_presets_are_listed_from_smallest_to_largest_budget() -> None:
    assert list(PRESETS) == BY_BUDGET
    assert list(AI_PRESETS) == BY_BUDGET
    assert list(PRESET_SUMMARIES) == BY_BUDGET
    for name, config in PRESETS.items():
        assert config.name == name
        assert make_agent("mcts", preset=name).name == f"mcts-{name}"


@pytest.mark.parametrize("n_options", range(1, 41))
def test_budgets_grow_strictly_from_preset_to_preset(n_options: int) -> None:
    budgets = [PRESETS[p].budget(n_options) for p in BY_BUDGET]
    assert budgets == sorted(set(budgets))


def test_pre_game_samples_do_not_shrink_from_preset_to_preset() -> None:
    for field in ("first_player_games", "mulligan_samples"):
        values = [getattr(PRESETS[p], field) for p in BY_BUDGET]
        assert values == sorted(values), field


def test_standard_and_strong_are_unchanged() -> None:
    assert PRESETS["standard"] == SearchConfig(
        name="standard", max_iterations=320, min_iterations=80, iterations_per_option=32
    )
    assert PRESETS["strong"] == SearchConfig(
        name="strong",
        max_iterations=1280,
        min_iterations=320,
        iterations_per_option=128,
        exploration=0.15,
        rollout_temperature=0.3,
        first_player_games=48,
        mulligan_samples=192,
    )


@pytest.mark.parametrize(("preset", "turn_limit"), [("fast", DEFAULT_TURN_LIMIT), ("expert", 5)])
def test_new_presets_are_deterministic_and_independent_of_workers(
    preset: str, turn_limit: int
) -> None:
    config = BenchmarkConfig(
        load_deck(EXAMPLES / "blue-white-federation.txt"),
        load_deck(EXAMPLES / "red-green-zeon.txt"),
        matches=2,
        seed=2026,
        fmt="bo1",
        ai_preset=preset,
        turn_limit=turn_limit,
        replays=0,
    )
    serial = run_benchmark(config)
    again = run_benchmark(config)
    parallel = run_benchmark(dataclasses.replace(config, workers=2))
    assert parallel.timing.workers == 2
    results = to_json_text(build_results(serial))
    assert to_json_text(build_results(again)) == results
    assert to_json_text(build_results(parallel)) == results
    assert serial.games == again.games == parallel.games
    assert build_results(serial)["ai"]["preset"] == preset
    assert all(g.agents == (f"mcts-{preset}", f"mcts-{preset}") for g in serial.games)
