"""Engine caches and shortcuts never change a game: every cached derived view equals a fresh
computation, so where and how often the view is computed cannot change any result; every
event skipped as unlistened has no trigger candidate; rules management changes nothing
whenever it skips the check because no damage reaches printed HP or the state is unchanged
since such a check; and references resolved without the view resolve the same with it."""

from __future__ import annotations

import pytest
from tests.paranoid import CacheChecker

from gcg_sim.ai import make_agent
from gcg_sim.ai.selfplay import run_game
from gcg_sim.ai.tuning import TUNING_DECKS
from gcg_sim.engine.types import Zone
from gcg_sim.testkit import Scenario, play, select_if_asked
from gcg_sim.tools import fuzz

FUZZ_GAMES = 16
ZEON_REMNANT = "GD01-115"  # 【Main】/【Action】Choose 1 enemy Unit. Deal 1 damage to it.
RAIDER = "GD02-010"  # 4/4, 【Once per Turn】When this Unit receives enemy effect damage, draw 1.
ZAKU_MARINER = "GD01-060"  # vanilla 2/2


def play_games() -> None:
    """Random-agent games over decks that include every card, plus two greedy games
    (determinized look-aheads on clones that share the cached view)."""
    res = fuzz.run(FUZZ_GAMES, seed=11, workers=1)
    assert not res.failures, res.failures[:1]
    for seed in (1, 2):
        agents = (make_agent("greedy", seed=seed), make_agent("greedy", seed=seed + 10))
        st, _ = run_game(agents, (TUNING_DECKS[0], TUNING_DECKS[1]), seed)
        assert st.winner is not None


@pytest.mark.rule("10-1-5", "10-1-6", "11-3")
def test_engine_caches_and_shortcuts_match_a_full_computation(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    checker = CacheChecker()
    checker.install(monkeypatch)
    play_games()
    assert checker.checks["cached view"] > 10_000
    assert checker.checks["unlistened event"] > 1_000
    assert checker.checks["no printed lethal damage"] > 1_000
    assert checker.checks["unchanged state"] > 1_000
    assert checker.checks["view-free reference"] > 1_000
    assert not checker.stale, checker.stale[:5]


@pytest.mark.rule("10-1-6-1")
@pytest.mark.parametrize("raw", [False, True])
def test_a_unit_added_after_a_scenario_starts_still_triggers(raw: bool) -> None:
    """The engine skips events no card in the game's decklists can trigger on, so a card added
    after the scenario starts joins its decklists, also when added without requiring its
    script."""
    sc = Scenario()
    sc.resources(0, 2)
    zeon_remnant = sc.add(0, ZEON_REMNANT, Zone.HAND)
    sc.add(1, ZAKU_MARINER)
    st = sc.start()
    raider = sc.add_raw(1, RAIDER, Zone.BATTLE) if raw else sc.add(1, RAIDER)
    play(st, zeon_remnant)
    select_if_asked(st, raider)
    assert st.cards[raider].damage == 1
    assert len(st.zones[1][Zone.HAND]) == 1
