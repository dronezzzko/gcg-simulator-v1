"""Detect spending moves that change nothing.

A Command play or an ability activation is "without effect" when it changes nothing but what it
spent. In one determinization of the player's information set, the move is played and
resolved, and the deterministic policy continues to the next turn while its actions are
recorded. The same actions are then replayed in the same world without the move. The move is
without effect when every recorded action was also legal without it, and both lines end in the
same position apart from the card and Resources spent and the player's own cards rested to pay
for it (the line without the move passes the extra action steps it gets from still holding
the card). Examples: AP-3 "during this turn" on an enemy Unit that battles no more this turn,
"rest it" on a Unit that is already rested, or a cost paid for an effect that does nothing.
"""

from __future__ import annotations

from typing import Any

from gcg_sim.ai import policy
from gcg_sim.ai.actions import Key
from gcg_sim.engine.game import apply
from gcg_sim.engine.observe import determinize
from gcg_sim.engine.state import Action, Decision, GameState
from gcg_sim.engine.types import ActionKind, DecisionKind, Zone

A = ActionKind
SPENDING = frozenset({A.PLAY_COMMAND, A.ACTIVATE})
TOP_LEVEL = frozenset({DecisionKind.MAIN, DecisionKind.ACTION_STEP})
MAX_DECISIONS = 400
PASS = Action(ActionKind.PASS)
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


def _policy_step(st: GameState) -> Action:
    return policy.choose(st, None, 0.0)


def _resolve(st: GameState, action: Action) -> None:
    """Apply ``action`` and let the policy make the choices of its resolution (targets,
    optional parts, triggered effects) until play is back at a top-level decision."""
    apply(st, action, check=False)
    n = 0
    while (
        st.winner is None
        and st.pending is not None
        and (st.frames or st.pending.kind not in TOP_LEVEL)
        and n < MAX_DECISIONS
    ):
        apply(st, _policy_step(st), check=False)
        n += 1


def _continue(st: GameState, turn: int) -> list[Action]:
    actions: list[Action] = []
    while (st.winner is None and st.turn < turn and st.pending is not None) and len(
        actions
    ) < MAX_DECISIONS:
        action = _policy_step(st)
        apply(st, action, check=False)
        actions.append(action)
    return actions


def _replay(st: GameState, actions: list[Action], turn: int) -> bool:
    """Apply ``actions`` in order, passing extra action steps; False as soon as a recorded
    action cannot be taken (the lines diverged)."""
    queue = list(actions)
    n = 0
    while st.winner is None and st.turn < turn and st.pending is not None and n < MAX_DECISIONS:
        n += 1
        if queue and queue[0] in st.pending.options:
            apply(st, queue.pop(0), check=False)
        elif st.pending.kind is DecisionKind.ACTION_STEP and PASS in st.pending.options:
            apply(st, PASS, check=False)
        elif queue:
            return False
        else:
            apply(st, _policy_step(st), check=False)
    return not queue


def without_effect(
    st: GameState, player: int, dec: Decision, moves: list[tuple[Key, Action]], seed: int
) -> set[Key]:
    """Keys of the spending moves in ``moves`` that change nothing but what they spend."""
    spending = [(key, a) for key, a in moves if a.kind in SPENDING]
    if dec.kind not in TOP_LEVEL or not spending:
        return set()
    world = determinize(st, player, seed)
    turn = world.turn + 1
    out: set[Key] = set()
    for key, action in spending:
        played = world.clone()
        _resolve(played, action)
        actions = _continue(played, turn)
        idled = world.clone()
        if not _replay(idled, actions, turn):
            continue
        spent = action.a if action.kind is A.PLAY_COMMAND else -1
        if _only_costs_differ(played, idled, spent, player):
            out.add(key)
    return out


def _only_costs_differ(played: GameState, idled: GameState, spent: int, player: int) -> bool:
    """The lines differ at most in the player's own cards being rested: the price of an
    activation (e.g. "Rest 1 of your Units:") whose effect then did nothing."""
    a, b = _signature(played, spent), _signature(idled, spent)
    if a[:3] != b[:3] or a[4:] != b[4:] or len(a[3]) != len(b[3]):
        return False
    for x, y in zip(a[3], b[3], strict=True):
        if x == y:
            continue
        uid, zone, rested, *rest = x
        if uid != y[0] or zone != y[1] or rest != list(y[3:]):
            return False
        if played.cards[uid].owner != player or not rested or y[2]:
            return False
    return True
