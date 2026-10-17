"""Engine caches never change a game: every cached derived view equals a fresh computation, so
where and how often the view is computed cannot change any result."""

from __future__ import annotations

import pytest
from tests.paranoid import ViewChecker

from gcg_sim.ai import make_agent
from gcg_sim.ai.selfplay import run_game
from gcg_sim.ai.tuning import TUNING_DECKS
from gcg_sim.tools import fuzz

FUZZ_GAMES = 16


def play_games() -> None:
    """Random-agent games over decks that include every card, plus two greedy games
    (determinized look-aheads on clones that share the cached view)."""
    res = fuzz.run(FUZZ_GAMES, seed=11, workers=1)
    assert not res.failures, res.failures[:1]
    for seed in (1, 2):
        agents = (make_agent("greedy", seed=seed), make_agent("greedy", seed=seed + 10))
        st, _ = run_game(agents, (TUNING_DECKS[0], TUNING_DECKS[1]), seed)
        assert st.winner is not None


@pytest.mark.rule("10-1-5")
def test_every_cached_derived_view_equals_a_fresh_computation(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    checker = ViewChecker()
    checker.install(monkeypatch)
    play_games()
    assert checker.hits > 10_000
    assert not checker.stale, checker.stale[:5]
