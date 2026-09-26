"""Seeded uniform-random agent injected by runner, report, and CLI tests (not the real AI).

Referenced by file path (``FactoryRef("<this file>:make_random_agent")`` or the
``GCG_SIM_AGENT_FACTORY`` environment variable) so spawned worker processes can load it.
"""

from __future__ import annotations

from typing import Any

from gcg_sim.engine.state import Action, GameState
from gcg_sim.engine.types import ActionKind
from gcg_sim.rng import SplitMix64
from gcg_sim.runner.agents import AgentSpec


class SeededRandomAgent:
    def __init__(self, seed: int, *, log: bool = False) -> None:
        self.name = "test-random"
        self._rng = SplitMix64(seed)
        self._log_enabled = log
        self._log: list[dict[str, Any]] = []

    def choose(self, st: GameState, player: int) -> Action:
        assert st.pending is not None
        assert st.pending.player == player
        options = st.pending.options
        action = options[self._rng.randrange(len(options))]
        if self._log_enabled:
            self._log.append(
                {
                    "turn": st.turn,
                    "player": player,
                    "decision": st.pending.kind.value,
                    "chosen": action.to_json(),
                    "alternatives": [
                        {"action": o.to_json(), "value": 0.0, "visits": 0} for o in options
                    ],
                }
            )
        return action

    def decision_log(self) -> list[dict[str, Any]]:
        return list(self._log)


def make_random_agent(spec: AgentSpec) -> SeededRandomAgent:
    return SeededRandomAgent(spec.seed, log=spec.log)


class IllegalActionAgent:
    """Always answers with an action that is never legal (to test failure propagation)."""

    name = "test-illegal"

    def choose(self, st: GameState, player: int) -> Action:
        return Action(ActionKind.ATTACK, 10**6, 10**6)

    def decision_log(self) -> list[dict[str, Any]]:
        return []


def make_illegal_agent(spec: AgentSpec) -> IllegalActionAgent:
    return IllegalActionAgent()
