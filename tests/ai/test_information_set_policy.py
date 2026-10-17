"""The information-set check of ``test_information_set.py`` with every policy setting on.

The policy options read only public card text and field state, so a search that uses them must
still return the same move and statistics for every true state in the deciding player's
information set.
"""

from __future__ import annotations

import dataclasses
from functools import cache

import pytest
from tests.ai.test_information_set import (
    AGENT_SEEDS,
    GAME_SEEDS,
    PERMUTATION_SEEDS,
    STATES_PER_GAME,
    midgame_states,
)

from gcg_sim.ai import make_agent
from gcg_sim.ai.config import PRESETS, SearchConfig
from gcg_sim.engine.observe import permute_hidden
from gcg_sim.engine.state import GameState

POLICY_OPTIONS = dataclasses.replace(
    PRESETS["standard"],
    policy_burst_yes_score=4.0,
    policy_rule_polarity_table=True,
    policy_zero_ap_face=True,
)


def _decision(config: SearchConfig, seed: int, st: GameState) -> tuple[object, object]:
    agent = make_agent("mcts", seed=seed, log=True, config=config)
    assert st.pending is not None
    action = agent.choose(st, st.pending.player)
    return action, agent.decision_log()[-1]["alternatives"]


@cache
def _with_options(index: int) -> tuple[object, object]:
    return _decision(POLICY_OPTIONS, AGENT_SEEDS[index % len(AGENT_SEEDS)], midgame_states()[index])


@pytest.mark.parametrize("index", range(len(GAME_SEEDS) * STATES_PER_GAME))
def test_decision_with_policy_options_invariant_under_hidden_permutation(index: int) -> None:
    st = midgame_states()[index]
    assert st.pending is not None
    p = st.pending.player
    seed = AGENT_SEEDS[index % len(AGENT_SEEDS)]
    expected = _with_options(index)
    for s in PERMUTATION_SEEDS:
        assert _decision(POLICY_OPTIONS, seed, permute_hidden(st, p, s)) == expected


def test_policy_options_change_the_search() -> None:
    """The options are exercised: on some states the statistics differ from the standard
    preset's with the same seed."""
    states = midgame_states()
    changed = sum(
        _with_options(i) != _decision(PRESETS["standard"], AGENT_SEEDS[i % len(AGENT_SEEDS)], st)
        for i, st in enumerate(states)
    )
    print(f"\npolicy options changed the search on {changed} of {len(states)} states")
    assert changed >= len(states) // 4
