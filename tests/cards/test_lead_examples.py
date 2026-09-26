"""Lead-authored reference card tests (templates for per-set card-test authors)."""

from __future__ import annotations

import pytest

from gcg_sim.engine.types import Zone
from gcg_sim.testkit import Scenario, keywords, play, to_next_turn

PILOT = "GD01-089"  # Riddhe Marcenas, a Pilot card


@pytest.mark.card("GD01-001")
@pytest.mark.ruling("GD01-001:Q119")
def test_gd01_001_gains_repair_itself() -> None:
    sc = Scenario()
    gundam = sc.add(0, "GD01-001")
    st = sc.start()
    assert keywords(st, gundam).get("Repair") == 1  # Q119: the first effect applies to itself


@pytest.mark.card("GD01-001")
def test_gd01_001_when_paired_draws_with_two_other_units() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    gundam = sc.add(0, "GD01-001")
    sc.add(0, "GD01-060")
    sc.add(0, "GD01-060")
    pilot = sc.add(0, PILOT, Zone.HAND)
    st = sc.start()
    hand_before = len(st.zones[0][Zone.HAND])
    play(st, pilot, onto=gundam)
    assert len(st.zones[0][Zone.HAND]) == hand_before  # pilot left the hand, 1 card drawn


@pytest.mark.card("GD01-001")
def test_gd01_001_when_paired_no_draw_with_one_other_unit() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    gundam = sc.add(0, "GD01-001")
    sc.add(0, "GD01-060")
    pilot = sc.add(0, PILOT, Zone.HAND)
    st = sc.start()
    hand_before = len(st.zones[0][Zone.HAND])
    play(st, pilot, onto=gundam)
    assert len(st.zones[0][Zone.HAND]) == hand_before - 1


@pytest.mark.card("GD01-001")
@pytest.mark.rule("13-1-1-1")
def test_gd01_001_repair_recovers_at_end_of_turn() -> None:
    sc = Scenario()
    gundam = sc.add(0, "GD01-001", damage=2)
    st = sc.start()
    to_next_turn(st)
    assert st.cards[gundam].damage == 1
