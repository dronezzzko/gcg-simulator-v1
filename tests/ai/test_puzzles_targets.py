"""Effect-target puzzles that the baseline search gets wrong for many agent seeds.

Both positions hide the defender's hand and deck among varied cards, so determinizations
differ and the search's values for the two targets are noisy: with the baseline budget the
better target is played for only some agent seeds. More iterations for close target calls
(``target_extension``) make the choice reliable. Each test plays a seed sweep with the setting
and with the baseline configuration, which must fall short of the same pass rate.
"""

from __future__ import annotations

import dataclasses
from collections.abc import Callable

import pytest

from gcg_sim.ai import make_agent
from gcg_sim.ai.baseline import BASELINE_PRESETS, BASELINE_WEIGHTS
from gcg_sim.ai.config import SearchConfig
from gcg_sim.engine.state import Action, GameState
from gcg_sim.engine.types import ActionKind, DecisionKind, Zone
from gcg_sim.testkit import Scenario, play

FILLER = "GD01-060"
ZEON = ["ST03-008", "GD01-059", "GD01-035", "GD01-030", "GD01-031", "ST03-013", "GD01-115"]
FEDERATION = [
    "GD01-008",
    "ST01-005",
    "GD01-004",
    "ST01-004",
    "ST01-001",
    "GD01-001",
    "GD04-008",
    "ST01-010",
    "GD01-087",
    "ST01-012",
    "ST01-014",
    "ST04-013",
    "GD01-099",
    "GD04-122",
]
BASELINE = BASELINE_PRESETS["standard"]
EXTENDED = dataclasses.replace(BASELINE, target_extension=3.0)


def rest_the_active_unit() -> tuple[GameState, Action]:
    """Amuro Ray pairs: 【When Paired】rest 1 enemy Unit with 5 or less HP. Resting the already
    rested Schuzrum-Galluss does nothing; resting the active Zaku II stops it attacking or
    blocking."""
    sc = Scenario()
    sc.resources(0, 4)
    host = sc.add(0, "GD01-013")  # Gundam 3/4
    sc.add(0, "ST01-005")  # GM 2/2
    amuro = sc.add(0, "ST01-010", Zone.HAND)
    sc.add(1, "GD04-040", rested=True)  # Schuzrum-Galluss 4/3
    zaku = sc.add(1, "GD01-035")  # Zaku II 2/2
    sc.resources(1, 4)
    for i in range(4):
        sc.add(1, ZEON[(2 * i) % len(ZEON)], Zone.HAND, known=False)
    sc.deck(1, *[ZEON[i % len(ZEON)] for i in range(28)])
    sc.shields(0, *[FILLER] * 5)
    sc.shields(1, *[ZEON[(3 * i) % len(ZEON)] for i in range(5)])
    st = sc.start()
    play(st, amuro, onto=host)
    return st, Action(ActionKind.SELECT, zaku)


def damage_the_unit_that_cannot_repair() -> tuple[GameState, Action]:
    """Mikazuki pairs with Barbatos 2nd Form: 【When Paired】1 damage to one enemy Unit. Both
    enemy Units are rested; the damage on the Repair-1 Guncannon heals at the end of its
    owner's turn, the damage on the GM stays."""
    sc = Scenario()
    sc.resources(0, 4)
    barbatos = sc.add(0, "ST05-002")  # Barbatos 2nd Form 2/4, AP+2 while damaged
    mikazuki = sc.add(0, "ST05-010", Zone.HAND)
    sc.add(1, "GD01-004", rested=True)  # Guncannon 2/3 <Repair 1>
    gm = sc.add(1, "ST01-005", rested=True)  # GM 2/2
    sc.resources(1, 4)
    for i in range(5):
        sc.add(1, FEDERATION[(3 * i + 1) % len(FEDERATION)], Zone.HAND, known=False)
    sc.deck(1, *[FEDERATION[i % len(FEDERATION)] for i in range(28)])
    sc.shields(0, *[FILLER] * 5)
    sc.shields(1, *[FEDERATION[(5 * i) % len(FEDERATION)] for i in range(5)])
    st = sc.start()
    play(st, mikazuki, onto=barbatos)
    return st, Action(ActionKind.SELECT, gm)


PUZZLES: dict[str, tuple[Callable[[], tuple[GameState, Action]], int, int]] = {
    # name: (position, agent seeds 1..n, passes required)
    "rest_the_active_unit": (rest_the_active_unit, 16, 14),
    "damage_the_unit_that_cannot_repair": (damage_the_unit_that_cannot_repair, 24, 16),
}


def passes(build: Callable[[], tuple[GameState, Action]], seeds: int, config: SearchConfig) -> int:
    start, want = build()
    assert start.pending is not None and start.pending.kind is DecisionKind.SELECT
    player = start.pending.player
    solved = 0
    for seed in range(1, seeds + 1):
        agent = make_agent("mcts", seed=seed, config=config, weights=BASELINE_WEIGHTS)
        solved += agent.choose(start.clone(), player) == want
    return solved


@pytest.mark.parametrize("name", sorted(PUZZLES))
def test_close_target_calls_get_more_iterations(name: str) -> None:
    build, seeds, required = PUZZLES[name]
    assert passes(build, seeds, EXTENDED) >= required


@pytest.mark.parametrize("name", sorted(PUZZLES))
def test_negative_control_the_baseline_misses_the_pass_rate(name: str) -> None:
    build, seeds, required = PUZZLES[name]
    assert passes(build, seeds, BASELINE) < required
