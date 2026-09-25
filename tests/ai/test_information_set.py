"""Decisions depend only on the deciding player's information set (acceptance criterion 7c).

Mid-game states are taken from greedy self-play; for each one the hidden identities are
shuffled with ``permute_hidden`` (a different true state in the same information set) and a
freshly built agent with the same seed must return the same move with the same search
statistics.
"""

from __future__ import annotations

from functools import cache

import pytest
from tests.ai.decks import DECKS

from gcg_sim.ai import make_agent
from gcg_sim.engine.game import apply, new_game
from gcg_sim.engine.observe import information_set_key, permute_hidden, unknown_instances
from gcg_sim.engine.state import GameState

GAME_SEEDS = (101, 202, 303, 404, 505)
STATES_PER_GAME = 5
PERMUTATION_SEEDS = (1, 2, 3)
AGENT_SEEDS = (17, 29)


@cache
def midgame_states() -> tuple[GameState, ...]:
    """Every 4th contested decision from turn 3 on, five per game, from greedy self-play."""
    out: list[GameState] = []
    for seed in GAME_SEEDS:
        agents = (make_agent("greedy", seed=seed), make_agent("greedy", seed=seed + 1))
        st = new_game(DECKS, seed)
        taken = contested = 0
        while st.pending is not None and taken < STATES_PER_GAME:
            p = st.pending.player
            if st.turn >= 3 and len(st.pending.options) > 1:
                if contested % 4 == 0:
                    out.append(st.clone())
                    taken += 1
                contested += 1
            apply(st, agents[p].choose(st, p))
    return tuple(out)


def _hidden_ids(st: GameState, observer: int) -> list[int]:
    return [st.cards[u].def_id for owner in (0, 1) for u in unknown_instances(st, observer, owner)]


def test_enough_distinct_states_and_nontrivial_permutations() -> None:
    states = midgame_states()
    assert len(states) >= 20
    kinds = {st.pending.kind for st in states if st.pending is not None}
    assert len(kinds) >= 2
    changed = 0
    for st in states:
        assert st.pending is not None
        p = st.pending.player
        for s in PERMUTATION_SEEDS:
            variant = permute_hidden(st, p, s)
            assert information_set_key(variant, p) == information_set_key(st, p)
            changed += _hidden_ids(variant, p) != _hidden_ids(st, p)
    assert changed >= len(states) * len(PERMUTATION_SEEDS) * 0.9


def _decision(kind: str, seed: int, st: GameState) -> tuple[object, object]:
    agent = make_agent(kind, seed=seed, log=True)
    assert st.pending is not None
    action = agent.choose(st, st.pending.player)
    return action, agent.decision_log()[-1]["alternatives"]


@pytest.mark.parametrize("kind", ["mcts", "greedy"])
@pytest.mark.parametrize("index", range(len(GAME_SEEDS) * STATES_PER_GAME))
def test_decision_invariant_under_hidden_permutation(kind: str, index: int) -> None:
    st = midgame_states()[index]
    assert st.pending is not None
    p = st.pending.player
    seed = AGENT_SEEDS[index % len(AGENT_SEEDS)]
    expected = _decision(kind, seed, st)
    for s in PERMUTATION_SEEDS:
        assert _decision(kind, seed, permute_hidden(st, p, s)) == expected
