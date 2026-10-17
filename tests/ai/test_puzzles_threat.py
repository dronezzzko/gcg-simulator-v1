"""Defence under a lethal threat, and the tactical puzzles with the threat-aware settings on.

The defence puzzle is the position of an audited example-deck game (blue-white-federation vs
purple-white-tekkadan, match 1, decision 93), rebuilt with the testkit: Player 0 has no Shields
and no Base, and its only <Blocker> can trade itself for an attacker aimed at a rested Unit.
Blocking leaves the second enemy Unit an open lethal next turn; not blocking loses the rested
Unit, keeps the Blocker for that turn, and lets Player 0 destroy the rested attacker on its own
turn. In expectation over Player 0's determinizations the block loses on turn 18 in 24 of 24
worlds whichever version plays on, while not blocking loses later (turn 20, baseline agents) or
wins (24 of 24, agents with the guard and the extension).
"""

from __future__ import annotations

import dataclasses
from collections.abc import Callable
from typing import Any

import pytest
from tests.ai import test_puzzles

from gcg_sim.ai import make_agent
from gcg_sim.ai.base import Agent
from gcg_sim.ai.config import PRESETS, SearchConfig
from gcg_sim.engine.state import GameState
from gcg_sim.engine.types import ActionKind
from gcg_sim.testkit import Scenario, attack

A = ActionKind
FILLER = "GD01-060"
STANDARD = PRESETS["standard"]
GUARD_AND_EXTENSION = dataclasses.replace(STANDARD, rollout_guard=True, lethal_extension_turns=1)
ALL_ON = dataclasses.replace(GUARD_AND_EXTENSION, rollout_push="all")
DEFENCE_SEEDS = range(33040, 33048)
PUZZLES: dict[str, Callable[[int], None]] = {
    name: fn for name, fn in sorted(vars(test_puzzles).items()) if name.startswith("test_")
}


def last_blocker_under_threat() -> GameState:
    sc = Scenario(active=1, turn=16)
    sc.add(0, "ST05-003")  # CGS Mobile Worker 0/2
    sc.add(0, "ST05-002", rested=True)  # Gundam Barbatos 2nd Form 2/4
    graze = sc.add(0, "ST05-004", rested=True)  # Graze Custom 2/2
    sc.add(0, "ST05-001")  # Gundam Barbatos 4th Form 4/5
    sc.add(0, "GD05-058")  # Shiden Custom (Ryusei-Go) 4/3 <Blocker>
    sc.resources(0, 8, rested=7)
    ma_form = sc.add(1, "ST01-002")  # Gundam (MA Form) 4/3
    sc.add(1, "GD01-004", deployed_this_turn=True)  # Guncannon 2/3
    sc.resources(1, 8, rested=2)
    sc.shields(1, FILLER, FILLER, FILLER, FILLER)
    st = sc.start()
    attack(st, ma_form, graze)
    return st


def no_blocks(config: SearchConfig) -> int:
    count = 0
    for seed in DEFENCE_SEEDS:
        st = last_blocker_under_threat()
        assert st.pending is not None
        assert {o.kind for o in st.pending.options} == {A.BLOCK, A.NO_BLOCK}
        count += make_agent("mcts", seed=seed, config=config).choose(st, 0).kind is A.NO_BLOCK
    return count


@pytest.mark.parametrize("config", [GUARD_AND_EXTENSION, ALL_ON], ids=["guard+ext", "all"])
def test_keeps_the_last_blocker_for_the_lethal_turn(config: SearchConfig) -> None:
    assert no_blocks(config) >= 7


def test_negative_control_the_baseline_trades_the_last_blocker() -> None:
    assert no_blocks(STANDARD) <= 1
    assert no_blocks(dataclasses.replace(STANDARD, rollout_guard=True)) <= 1


@pytest.mark.parametrize("seed", test_puzzles.SEEDS)
@pytest.mark.parametrize("name", PUZZLES)
def test_puzzles_hold_with_the_threat_settings(
    monkeypatch: pytest.MonkeyPatch, name: str, seed: int
) -> None:
    def threat_agent(kind: str, **kwargs: Any) -> Agent:
        return make_agent(kind, config=ALL_ON, **kwargs)

    monkeypatch.setattr(test_puzzles, "make_agent", threat_agent)
    PUZZLES[name](seed)
