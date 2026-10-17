"""The information-set procedure of ``test_information_set.py`` with the threat-aware settings on.

The settings act in lethal-threat positions, which the mid-game states of that test rarely
reach, so the states here are late: every second contested decision from turn 11 on, four per
game, from greedy self-play. Each is searched with the settings on before and after
``permute_hidden``; a fresh agent with the same seed must return the same move and statistics.
"""

from __future__ import annotations

import dataclasses
from functools import cache

import pytest
from tests.ai.decks import DECKS
from tests.ai.test_information_set import PERMUTATION_SEEDS

from gcg_sim.ai import make_agent
from gcg_sim.ai.config import PRESETS, SearchConfig
from gcg_sim.engine.game import apply, new_game
from gcg_sim.engine.observe import permute_hidden
from gcg_sim.engine.state import GameState

GAME_SEEDS = (33031, 33032, 33033, 33034, 33035)
FIRST_TURN = 11
STATES_PER_GAME = 4
AGENT_SEED = 17
CONFIGS = (
    dataclasses.replace(
        PRESETS["standard"], lethal_extension_turns=1, rollout_push="all", rollout_guard=True
    ),
    dataclasses.replace(
        PRESETS["standard"],
        lethal_extension_turns=1,
        extension_trigger="threat",
        rollout_push="extension",
        rollout_guard=True,
    ),
)


@cache
def late_states() -> tuple[GameState, ...]:
    out: list[GameState] = []
    for seed in GAME_SEEDS:
        agents = (make_agent("greedy", seed=seed), make_agent("greedy", seed=seed + 1))
        st = new_game(DECKS, seed)
        taken = contested = 0
        while st.pending is not None and taken < STATES_PER_GAME:
            p = st.pending.player
            if st.turn >= FIRST_TURN and len(st.pending.options) > 1:
                if contested % 2 == 0:
                    out.append(st.clone())
                    taken += 1
                contested += 1
            apply(st, agents[p].choose(st, p))
    return tuple(out)


def _decision(config: SearchConfig, st: GameState) -> tuple[object, object]:
    agent = make_agent("mcts", seed=AGENT_SEED, log=True, config=config)
    assert st.pending is not None
    action = agent.choose(st, st.pending.player)
    return action, agent.decision_log()[-1]["alternatives"]


def test_the_settings_change_most_late_searches() -> None:
    """The check exercises the settings: on every third late state (the two configurations
    alternate, so both appear), most searches differ from the baseline's with the same seed."""
    states = late_states()
    assert len(states) == len(GAME_SEEDS) * STATES_PER_GAME
    sample = range(0, len(states), 3)
    changed = sum(
        _decision(CONFIGS[i % len(CONFIGS)], states[i]) != _decision(PRESETS["standard"], states[i])
        for i in sample
    )
    assert changed >= len(sample) // 2


@pytest.mark.parametrize("index", range(len(GAME_SEEDS) * STATES_PER_GAME))
def test_decision_invariant_under_hidden_permutation_with_threat_settings(index: int) -> None:
    st = late_states()[index]
    assert st.pending is not None
    config = CONFIGS[index % len(CONFIGS)]
    expected = _decision(config, st)
    for s in PERMUTATION_SEEDS:
        assert _decision(config, permute_hidden(st, st.pending.player, s)) == expected
