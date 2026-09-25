"""AI strength (acceptance criterion 7a): the default MCTS preset against the baselines.

400 games per opponent in mirrored blocks of four (each deal is played with the MCTS agent
on both decks and in both seats). Games run in parallel processes; every seed derives from
the game index, so the result does not depend on the worker count. Set
``GCG_SIM_TEST_WORKERS`` to limit the processes.
"""

from __future__ import annotations

import os

import pytest
from tests.ai.decks import DECKS

from gcg_sim.ai.selfplay import AgentSpec, GameOutcome, mirrored_specs, play_all, wilson

GAMES = 400
MCTS = AgentSpec("mcts", "standard")


def _workers() -> int:
    configured = os.environ.get("GCG_SIM_TEST_WORKERS")
    return int(configured) if configured else max(1, (os.cpu_count() or 2) - 1)


def _match(opponent: AgentSpec, seed: int) -> tuple[float, tuple[float, float], list[GameOutcome]]:
    outcomes = play_all(mirrored_specs(GAMES, seed, MCTS, opponent, DECKS), _workers())
    score = sum(o.challenger_score for o in outcomes)
    lo, hi = wilson(score, len(outcomes))
    rate = score / len(outcomes)
    turns = sum(o.turns for o in outcomes) / len(outcomes)
    print(
        f"\nmcts-standard vs {opponent.kind}: {score}/{len(outcomes)} = {rate:.3f} "
        f"(Wilson 95% [{lo:.3f}, {hi:.3f}]), mean turns {turns:.1f}"
    )
    return rate, (lo, hi), outcomes


@pytest.mark.slow
def test_default_preset_beats_random() -> None:
    rate, _, outcomes = _match(AgentSpec("random"), seed=7001)
    assert len(outcomes) >= 400
    assert {o.challenger for o in outcomes} == {0, 1}
    assert rate >= 0.90


@pytest.mark.slow
def test_default_preset_beats_greedy() -> None:
    _, (lower, _), outcomes = _match(AgentSpec("greedy"), seed=7002)
    assert len(outcomes) >= 400
    assert {o.challenger for o in outcomes} == {0, 1}
    assert lower > 0.50
