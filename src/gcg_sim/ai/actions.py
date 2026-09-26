"""Canonical move keys for the search tree.

Two legal actions are the same move when they differ only in which copy of a card in a
private or public pile is used (two copies of one card in hand) or in how many EX Resources
pay for them. Keys name hand, trash, and other off-field cards by card definition (as known to
the deciding player) and field cards by instance, so a key means the same move in every
determinization of the decider's information set.
"""

from __future__ import annotations

from gcg_sim.ai.base import uid_argument
from gcg_sim.effects import dsl as d
from gcg_sim.engine import view as V
from gcg_sim.engine.game import ALT_PLAY_BASE, _activated_list, _command_playable
from gcg_sim.engine.state import Action, Decision, GameState
from gcg_sim.engine.types import NO_ARG, ActionKind, DecisionKind, Zone

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


_ADD_SELF_TO_HAND = (d.AddToHand(d.ThisCard()),)


def _free_card_burst(st: GameState) -> bool:
    """The resolving 【Burst】 only adds its own card to the hand: accepting costs nothing."""
    if not st.frames:
        return False
    uid = st.frames[-1].card_uid
    if uid < 0:
        return False
    R = V.reg()
    entry = R.cards[st.cards[uid].def_id]
    if entry.burst_aid < 0:
        return False
    burst = R.abilities[entry.burst_aid].ability
    return isinstance(burst, d.Burst) and burst.steps == _ADD_SELF_TO_HAND


def _attack_can_matter(st: GameState, player: int, attacker: int) -> bool:
    """A Unit with AP 0 or less deals no battle damage (5-5-5); its attack can still matter
    through attack-triggered effects or an AP boost played in the battle's action step."""
    dv = V.derived(st)
    if V.ap_of(st, dv, attacker) > 0:
        return True
    if any(
        st.cards[host].owner == player for host, _ in V.trigger_index(st, dv).get(d.Ev.ATTACKS, ())
    ):
        return True
    if _activated_list(st, player, d.Timing.ACTION):
        return True
    return any(
        _command_playable(st, player, uid, d.Timing.ACTION) for uid in st.zones[player][Zone.HAND]
    )


def prune_dominated(
    st: GameState, dec: Decision, moves: list[tuple[Key, Action]]
) -> list[tuple[Key, Action]]:
    """Drop moves that can only be worse than another legal move: declining a free
    "【Burst】Add this card to your hand", and attacking with a Unit whose attack cannot deal
    damage or trigger anything. At least one move is always kept."""
    if dec.kind is DecisionKind.BURST and _free_card_burst(st):
        kept = [m for m in moves if m[1].kind is A.YES]
    elif dec.kind is DecisionKind.MAIN:
        kept = [
            m
            for m in moves
            if m[1].kind is not A.ATTACK or _attack_can_matter(st, dec.player, m[1].a)
        ]
    else:
        return moves
    return kept or moves
