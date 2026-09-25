"""The Agent protocol, shared agent bookkeeping, and decision-log formatting."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Protocol

from gcg_sim.engine import view as V
from gcg_sim.engine.observe import is_hidden_from
from gcg_sim.engine.state import Action, Decision, GameState
from gcg_sim.engine.types import PLAYER_TARGET, ActionKind, DecisionKind
from gcg_sim.rng import SplitMix64, derive_seed

A = ActionKind
LOG_ALTERNATIVES = 5

_CARD_ARG_KINDS = frozenset(
    {A.PLAY_UNIT, A.PLAY_BASE, A.PAIR, A.PLAY_COMMAND, A.ACTIVATE, A.ATTACK, A.BLOCK}
)


class Agent(Protocol):
    """A player. ``choose`` returns one of ``st.pending.options`` for ``player``."""

    name: str

    def choose(self, st: GameState, player: int) -> Action:
        """Pick an action for the pending decision (``st.pending.player == player``)."""

    def decision_log(self) -> list[dict[str, Any]]:
        """Logged decisions; empty unless the agent was created with ``log=True``."""


@dataclass(frozen=True, slots=True)
class Alternative:
    """One candidate move with its search statistics (value from the chooser's view)."""

    action: Action
    value: float
    visits: int


class AgentCore:
    """Seed, decision counter, and decision log shared by every agent.

    Each decision gets its own generator seeded from the construction seed and the number of
    decisions made so far, so choices depend only on the seed and the sequence of calls.
    """

    def __init__(self, name: str, seed: int, log: bool) -> None:
        self.name = name
        self.seed = seed
        self._log_enabled = log
        self._entries: list[dict[str, Any]] = []
        self._decisions = 0

    def decision_log(self) -> list[dict[str, Any]]:
        return list(self._entries)

    def _begin(self, st: GameState, player: int) -> tuple[Decision, SplitMix64]:
        dec = st.pending
        if dec is None or dec.player != player:
            raise ValueError(f"{self.name}: no decision pending for player {player}")
        self._decisions += 1
        return dec, SplitMix64(derive_seed(self.seed, self.name, "decision", self._decisions))

    def _finish(
        self,
        st: GameState,
        player: int,
        chosen: Action,
        alternatives: list[Alternative],
    ) -> Action:
        if self._log_enabled:
            self._entries.append(log_entry(st, player, chosen, alternatives))
        return chosen


def log_entry(
    st: GameState, player: int, chosen: Action, alternatives: list[Alternative]
) -> dict[str, Any]:
    dec = st.pending
    assert dec is not None
    ranked = sorted(alternatives, key=lambda x: (-x.visits, -x.value))[:LOG_ALTERNATIVES]
    return {
        "turn": st.turn,
        "player": player,
        "decision": dec.kind.value,
        "chosen": chosen.to_json(),
        "label": describe(st, dec, chosen, player),
        "alternatives": [
            {
                "action": alt.action.to_json(),
                "label": describe(st, dec, alt.action, player),
                "value": round(alt.value, 4),
                "visits": alt.visits,
            }
            for alt in ranked
        ],
    }


def card_label(st: GameState, uid: int, observer: int) -> str:
    if uid == PLAYER_TARGET:
        return "player"
    if not 0 <= uid < len(st.cards) or is_hidden_from(st, uid, observer):
        return f"#{uid}"
    cd = V.cdef(st, uid)
    return f"{cd.card_number} {cd.name}"


def describe(st: GameState, dec: Decision, action: Action, observer: int) -> str:
    """Readable form of ``action`` using only what ``observer`` knows."""
    k = action.kind
    if k in _CARD_ARG_KINDS:
        text = f"{k.value} {card_label(st, action.a, observer)}"
        if k in (A.PAIR, A.ATTACK):
            text += f" -> {card_label(st, action.b, observer)}"
        return text
    if k is A.SELECT and uid_argument(dec):
        return f"select {card_label(st, action.a, observer)}"
    return str(action)


def uid_argument(dec: Decision) -> bool:
    """Whether SELECT options of ``dec`` name cards (uids) rather than option indices."""
    if dec.kind in (DecisionKind.EXCESS, DecisionKind.DISCARD):
        return True
    if dec.kind is DecisionKind.ARRANGE:
        return dec.ctx("order") == 1
    return dec.kind is DecisionKind.SELECT and dec.ctx("mode") != 1
