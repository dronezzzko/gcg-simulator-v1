"""The puzzles added with the 2026-10 settings, played by the shipped standard preset.

The setting-level tests (``test_root_moves.py``, ``test_policy_knobs.py``,
``test_puzzles_targets.py``) switch one setting on in the pre-change configuration. These
play the same positions with ``make_agent("mcts")`` (the default preset and evaluation),
with the two agent seeds of ``test_puzzles.py``, so the AI that ships solves them too.
"""

from __future__ import annotations

import pytest
from tests.ai.test_policy_knobs import _protection
from tests.ai.test_puzzles_targets import rest_the_active_unit
from tests.ai.test_root_moves import amuro_rests, rests_the_blocker

from gcg_sim.ai import make_agent
from gcg_sim.ai.base import Agent
from gcg_sim.engine.game import apply
from gcg_sim.engine.state import Action, GameState
from gcg_sim.engine.types import ActionKind, Zone

A = ActionKind
SEEDS = (1, 2)


def agents(seed: int) -> tuple[Agent, Agent]:
    return make_agent("mcts", seed=seed), make_agent("mcts", seed=seed + 100)


def play_turn(st: GameState, players: tuple[Agent, Agent]) -> None:
    stop = st.turn + 1
    while st.pending is not None and st.turn < stop:
        p = st.pending.player
        apply(st, players[p].choose(st, p))


@pytest.mark.parametrize("seed", SEEDS)
def test_rests_the_blocker_while_a_bigger_unit_is_already_rested(seed: int) -> None:
    st = rests_the_blocker(rested_decoy=True)
    play_turn(st, agents(seed))
    assert st.winner == 0


@pytest.mark.parametrize("seed", SEEDS)
@pytest.mark.parametrize("hidden", [0, 4])
def test_rests_the_active_unit_not_the_rested_one(seed: int, hidden: int) -> None:
    extra = ("ST01-005",) if hidden else ()
    st, _, zaku = amuro_rests(extra_units=extra, hidden=hidden)
    assert agents(seed)[0].choose(st, 0) == Action(A.SELECT, zaku)


@pytest.mark.parametrize("seed", SEEDS)
def test_rests_the_active_unit_with_a_hidden_hand(seed: int) -> None:
    st, want = rest_the_active_unit()
    assert st.pending is not None
    assert agents(seed)[0].choose(st, st.pending.player) == want


@pytest.mark.parametrize("seed", SEEDS)
@pytest.mark.parametrize("attacked_is_big", [True, False])
def test_protects_the_attacked_unit(seed: int, attacked_is_big: bool) -> None:
    st, target = _protection(attacked_is_big)
    players = agents(seed)
    while st.pending is not None and st.battle is not None:
        p = st.pending.player
        apply(st, players[p].choose(st, p))
    assert st.cards[target].zone is Zone.BATTLE
