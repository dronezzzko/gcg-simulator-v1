"""W-60: the stripped parentheticals whose meaning the bindings implement, on the Python
testkit. ``crates/gcg-engine/tests/p6k_behaviour.rs`` runs the same scenarios on the Rust
testkit (``w60_gd05_089_deployed_as_a_unit_is_not_a_pilot``,
``w60_st12_013_you_choose_first_then_your_opponent``, ``w60_st14_014_does_not_count_itself``).

    uv run pytest oracle/tests -q
"""

from __future__ import annotations

import pytest

from gcg_sim.engine import view as V
from gcg_sim.engine.state import GameState
from gcg_sim.engine.types import PLAYER_TARGET, ActionKind, DecisionKind, Zone
from gcg_sim.testkit import Scenario, ap, attack, has_action, play, select, to_next_turn, yes

VANILLA_2_2 = "GD01-060"  # Zaku Mariner, Lv2 2/2
VANILLA_3_4 = "GD01-013"  # Gundam, Lv4 3/4
VANILLA_5_4 = "ST07-003"  # Gundam Virtue, Lv5 5/4
VANILLA_6_4 = "ST14-008"  # Gundam Heavyarms Custom (EW), Lv6 6/4
MF_CARD = "GD05-042"
PILOT = "GD05-091"
FINAL_VICTOR = "ST12-013"
BLAZING = "ST14-014"


def _kind(st: GameState) -> tuple[DecisionKind, int] | None:
    return (st.pending.kind, st.pending.player) if st.pending is not None else None


@pytest.mark.card("GD05-089")
@pytest.mark.ruling("GD05-089:Q390")
def test_gd05_089_deployed_as_a_unit_is_not_a_pilot() -> None:
    sc = Scenario()
    sc.resources(1, 5)
    attacker = sc.add(0, VANILLA_2_2)
    pilot = sc.add(1, PILOT, Zone.HAND)
    other = sc.add(1, VANILLA_3_4)
    (asia,) = sc.shields(1, "GD05-089")
    sc.trash(1, MF_CARD, "GD05-043", MF_CARD)
    st = sc.start()
    attack(st, attacker, PLAYER_TARGET)
    yes(st)
    if st.pending is not None and st.pending.kind is DecisionKind.YES_NO:
        yes(st)
    cdef = V.cdef(st, asia)
    assert cdef.card_number.startswith("UNIT:")
    assert cdef.card_type.is_unit
    assert not cdef.is_pilot_capable
    to_next_turn(st)
    assert st.active == 1
    assert has_action(st, ActionKind.PAIR, pilot, asia)
    assert not has_action(st, ActionKind.PAIR, asia, other)


@pytest.mark.card("ST12-013")
def test_st12_013_you_choose_first_then_your_opponent() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    mine = (sc.add(0, VANILLA_6_4), sc.add(0, VANILLA_5_4))
    theirs = (sc.add(1, VANILLA_2_2), sc.add(1, VANILLA_3_4))
    victor = sc.add(0, FINAL_VICTOR, Zone.HAND)
    st = sc.start()
    play(st, victor)
    assert _kind(st) == (DecisionKind.SELECT, 0)
    select(st, mine[0])
    assert _kind(st) == (DecisionKind.SELECT, 1)
    select(st, theirs[1])
    assert st.cards[mine[0]].damage == 3
    assert st.cards[mine[1]].damage == 0
    assert st.cards[theirs[0]].damage == 0


def _blazing(commands: int) -> tuple[GameState, int]:
    sc = Scenario()
    sc.resources(0, 5)
    card = sc.add(0, BLAZING, Zone.HAND)
    sc.add(1, VANILLA_6_4)
    sc.trash(0, *([FINAL_VICTOR] * commands), VANILLA_2_2)
    return sc.start(), card


@pytest.mark.card("ST14-014")
def test_st14_014_does_not_count_itself() -> None:
    st, card = _blazing(3)
    assert not has_action(st, ActionKind.PLAY_COMMAND, card)
    st, card = _blazing(4)
    play(st, card)
    (enemy,) = st.zones[1][Zone.BATTLE]
    assert ap(st, enemy) == 3
