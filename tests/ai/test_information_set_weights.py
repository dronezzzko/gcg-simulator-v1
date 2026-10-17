"""The information-set check of ``test_information_set`` with the alternative evaluation weights:
a fresh agent with the same seed returns the same move and statistics on every state of the
same information set."""

from __future__ import annotations

import pytest
from tests.ai.test_information_set import (
    AGENT_SEEDS,
    GAME_SEEDS,
    PERMUTATION_SEEDS,
    STATES_PER_GAME,
    midgame_states,
)

from gcg_sim.ai import make_agent
from gcg_sim.ai.compare import WEIGHTS
from gcg_sim.engine.observe import permute_hidden
from gcg_sim.engine.state import GameState

ALTERNATIVES = ("roles",)


def _decision(kind: str, weights: str, seed: int, st: GameState) -> tuple[object, object]:
    agent = make_agent(kind, seed=seed, log=True, weights=WEIGHTS[weights])
    assert st.pending is not None
    action = agent.choose(st, st.pending.player)
    return action, agent.decision_log()[-1]["alternatives"]


@pytest.mark.parametrize("weights", ALTERNATIVES)
@pytest.mark.parametrize("kind", ["mcts", "greedy"])
@pytest.mark.parametrize("index", range(len(GAME_SEEDS) * STATES_PER_GAME))
def test_decision_invariant_under_hidden_permutation(kind: str, weights: str, index: int) -> None:
    st = midgame_states()[index]
    assert st.pending is not None
    p = st.pending.player
    seed = AGENT_SEEDS[index % len(AGENT_SEEDS)]
    expected = _decision(kind, weights, seed, st)
    for s in PERMUTATION_SEEDS:
        assert _decision(kind, weights, seed, permute_hidden(st, p, s)) == expected
