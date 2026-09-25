"""Detect spending moves that change nothing.

A Command play or an ability activation is "without effect" when, in one determinization of
the player's information set, playing it and then continuing with the deterministic policy
reaches the next turn in the same position as simply ending the main phase (or passing) at
once, apart from the card and Resources it spent. Such a move can only lose a card; for
example AP-3 "during this turn" on an enemy Unit that cannot battle any more this turn, or
"rest it" on Units that are already rested.
"""

from __future__ import annotations

from typing import Any

from gcg_sim.ai.actions import Key
from gcg_sim.ai.playout import play_until_turn
from gcg_sim.engine.game import apply
from gcg_sim.engine.observe import determinize
from gcg_sim.engine.state import Action, Decision, GameState
from gcg_sim.engine.types import ActionKind, DecisionKind, Zone

A = ActionKind
SPENDING = frozenset({A.PLAY_COMMAND, A.ACTIVATE})
IDLE = {DecisionKind.MAIN: A.END_MAIN, DecisionKind.ACTION_STEP: A.PASS}
MAX_DECISIONS = 400
_SPENT_ZONES = frozenset({Zone.RESOURCE_AREA, Zone.OUTSIDE})


def _signature(st: GameState, spent: int) -> tuple[Any, ...]:
    cards = tuple(
        (c.uid, c.zone, c.rested, c.damage, c.pair, c.known)
        for c in st.cards
        if c.uid != spent and c.zone not in _SPENT_ZONES
    )
    lasting = tuple(
        (le.effect_key, le.targets, le.duration, le.player) for le in (*st.lasting, *st.delayed)
    )
    decks = tuple(tuple(st.zones[p][Zone.DECK]) for p in (0, 1))
    return (st.winner, st.turn, st.active, cards, lasting, decks)


def _after(world: GameState, action: Action) -> GameState:
    s = world.clone()
    apply(s, action, check=False)
    play_until_turn(s, world.turn + 1, None, 0.0, MAX_DECISIONS)
    return s


def without_effect(
    st: GameState, player: int, dec: Decision, moves: list[tuple[Key, Action]], seed: int
) -> set[Key]:
    """Keys of the spending moves in ``moves`` that change nothing compared with idling."""
    idle_kind = IDLE.get(dec.kind)
    idle = next((a for _, a in moves if a.kind is idle_kind), None)
    spending = [(key, a) for key, a in moves if a.kind in SPENDING]
    if idle is None or not spending:
        return set()
    world = determinize(st, player, seed)
    idled = _after(world, idle)
    out: set[Key] = set()
    for key, action in spending:
        spent = action.a if action.kind is A.PLAY_COMMAND else -1
        if _signature(_after(world, action), spent) == _signature(idled, spent):
            out.add(key)
    return out
