"""The baseline configuration plays exactly as the pre-change ``standard`` AI.

Paired comparisons of AI versions (``gcg_sim.ai.compare``) measure every change against
``baseline-standard``, so it must stay a faithful copy: one full game, recorded with
``make_agent("mcts", preset="standard", log=True)`` at commit bc38ab4, must come out the same,
moves, winner and every logged search statistic included.
"""

from __future__ import annotations

import hashlib
import json

from tests.ai.decks import DECKS

from gcg_sim.ai import make_agent
from gcg_sim.ai.baseline import BASELINE_PRESETS, BASELINE_WEIGHTS
from gcg_sim.engine.game import apply, new_game

GAME_SEED = 4242
AGENT_SEED = 17
RECORDED_DIGEST = "5cff789d2a8bb26cfef4db98b1ef1ef8f37c3bf63210114781d357bd215f3b96"


def test_baseline_standard_replays_the_game_recorded_before_the_changes() -> None:
    agents = tuple(
        make_agent(
            "mcts",
            seed=AGENT_SEED + seat,
            log=True,
            config=BASELINE_PRESETS["standard"],
            weights=BASELINE_WEIGHTS,
        )
        for seat in (0, 1)
    )
    st = new_game(DECKS, GAME_SEED)
    actions = []
    while st.pending is not None:
        p = st.pending.player
        action = agents[p].choose(st, p)
        actions.append(action.to_json())
        apply(st, action)
    payload = {
        "actions": actions,
        "logs": [agent.decision_log() for agent in agents],
        "winner": st.winner,
        "turn": st.turn,
    }
    digest = hashlib.sha256(json.dumps(payload, sort_keys=True).encode()).hexdigest()
    assert (st.winner, st.turn, len(actions)) == (1, 20, 115)
    assert digest == RECORDED_DIGEST
