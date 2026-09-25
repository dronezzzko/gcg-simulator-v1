"""Heuristic worth of cards and combat outcomes, shared by the playout policy and setup choices."""

from __future__ import annotations

from dataclasses import dataclass

from gcg_sim.cards.model import CardDef, CardType
from gcg_sim.effects import dsl as d
from gcg_sim.engine import view as V
from gcg_sim.engine.state import GameState
from gcg_sim.engine.types import Zone

LINK_BONUS = 1.0
PAIR_BONUS = 0.5
KEYWORD_BONUS = 0.3


def hp_left(st: GameState, dv: V.Derived, uid: int) -> int:
    return max(0, V.hp_of(st, dv, uid) - st.cards[uid].damage)


def unit_value(st: GameState, dv: V.Derived, uid: int) -> float:
    """Rough worth of a Unit or Base on the field, in "stat points"."""
    c = st.cards[uid]
    v = 0.5 * (V.ap_of(st, dv, uid) + hp_left(st, dv, uid)) + 0.25 * V.level_of(st, uid)
    if uid in dv.linked:
        v += LINK_BONUS
    elif c.pair >= 0:
        v += PAIR_BONUS
    kws = dv.kw.get(uid)
    if kws:
        v += KEYWORD_BONUS * len(kws)
    return v


def card_worth(cd: CardDef) -> float:
    """Worth of a card held in hand (or picked from a pile), independent of the board."""
    t = cd.card_type
    if t is CardType.UNIT:
        return 1.0 + 0.3 * cd.cost + 0.1 * (cd.ap + cd.hp)
    if t is CardType.PILOT:
        return 1.3 + 0.1 * (cd.ap + cd.hp)
    if t is CardType.COMMAND:
        return 1.4 + 0.2 * cd.cost
    if t is CardType.BASE:
        return 1.2
    return 0.5


def pile_value(st: GameState, dv: V.Derived, uid: int) -> float:
    """Worth of a card wherever it is: field cards by board value, others by card worth."""
    if st.cards[uid].zone in (Zone.BATTLE, Zone.BASE):
        return unit_value(st, dv, uid)
    return card_worth(V.cdef(st, uid))


@dataclass(frozen=True, slots=True)
class Fight:
    """Outcome of ``attacker`` dealing battle damage to ``defender`` and back (rule 8-5)."""

    kills: bool
    dies: bool


def fight(st: GameState, dv: V.Derived, attacker: int, defender: int) -> Fight:
    a_ap = V.ap_of(st, dv, attacker)
    d_ap = V.ap_of(st, dv, defender)
    kills = a_ap >= hp_left(st, dv, defender) and a_ap > 0
    first_strike = d.Kw.FIRST_STRIKE in dv.kw.get(attacker, ())
    dies = d_ap >= hp_left(st, dv, attacker) and d_ap > 0 and not (first_strike and kills)
    return Fight(kills, dies)


def ready_blockers(st: GameState, dv: V.Derived, player: int, exclude: int = -1) -> list[int]:
    """Active Units of ``player`` with <Blocker> (candidates to intercept an attack)."""
    return [
        u
        for u in st.zones[player][Zone.BATTLE]
        if u != exclude and not st.cards[u].rested and d.Kw.BLOCKER in dv.kw.get(u, ())
    ]


def life(st: GameState, dv: V.Derived, player: int) -> float:
    """Shields plus the Base's remaining HP in Shield-equivalents."""
    zones = st.zones[player]
    base = sum(hp_left(st, dv, u) for u in zones[Zone.BASE])
    return len(zones[Zone.SHIELD]) + base / 3.0


def exposed(st: GameState, player: int) -> bool:
    """No Shields and no Base: the next battle damage to the player wins (rule 1-2-2-1)."""
    zones = st.zones[player]
    return not zones[Zone.SHIELD] and not zones[Zone.BASE]
