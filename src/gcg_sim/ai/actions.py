"""Canonical move keys for the search tree.

Two legal actions are the same move when they differ only in which copy of a card in a
private or public pile is used (two copies of one card in hand) or in how many EX Resources
pay for them. Keys name hand, trash, and other off-field cards by card definition (as known to
the deciding player) and field cards by instance, so a key means the same move in every
determinization of the decider's information set.
"""

from __future__ import annotations

from gcg_sim.ai.base import uid_argument
from gcg_sim.engine.game import ALT_PLAY_BASE
from gcg_sim.engine.state import Action, Decision, GameState
from gcg_sim.engine.types import NO_ARG, ActionKind, Zone

A = ActionKind
Key = tuple[str, int, int, int]

_BY_DEFINITION = frozenset(
    {Zone.HAND, Zone.TRASH, Zone.DECK, Zone.REMOVAL, Zone.SHIELD, Zone.RESOURCE_DECK}
)
_DEF_OFFSET = 1 << 24
_PAID_KINDS = frozenset({A.PLAY_UNIT, A.PLAY_BASE, A.PAIR, A.PLAY_COMMAND, A.ACTIVATE})


def ex_used(action: Action) -> int:
    """EX Resources an action spends (0 for actions without a payment)."""
    if action.kind not in _PAID_KINDS or action.c < 0:
        return 0
    return action.c % ALT_PLAY_BASE


def _ident(st: GameState, uid: int, decider: int) -> int:
    if not 0 <= uid < len(st.cards):
        return uid
    c = st.cards[uid]
    if c.zone in _BY_DEFINITION and c.known & (1 << decider):
        return _DEF_OFFSET + c.def_id
    return uid


def action_key(st: GameState, dec: Decision, action: Action) -> Key:
    k = action.kind
    p = dec.player
    if k is A.SELECT:
        a = _ident(st, action.a, p) if uid_argument(dec) else action.a
        return (k.value, a, action.b, NO_ARG)
    if k in _PAID_KINDS:
        variant = action.c // ALT_PLAY_BASE if action.c >= 0 else NO_ARG
        b = action.b if k is A.ACTIVATE else _ident(st, action.b, p)
        return (k.value, _ident(st, action.a, p), b, variant)
    if k in (A.ATTACK, A.BLOCK):
        return (k.value, action.a, action.b, NO_ARG)
    return (k.value, action.a, action.b, action.c)


def canonical(st: GameState, dec: Decision) -> list[tuple[Key, Action]]:
    """One representative action per distinct move, in option order; the representative
    spends the fewest EX Resources."""
    best: dict[Key, Action] = {}
    order: list[Key] = []
    for action in dec.options:
        key = action_key(st, dec, action)
        cur = best.get(key)
        if cur is None:
            best[key] = action
            order.append(key)
        elif ex_used(action) < ex_used(cur):
            best[key] = action
    return [(key, best[key]) for key in order]
