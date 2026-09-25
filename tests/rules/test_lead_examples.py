"""Lead-authored reference tests for core procedure (templates for rule-test authors)."""

from __future__ import annotations

import pytest

from gcg_sim.engine.types import PLAYER_TARGET, ActionKind, DecisionKind, EndReason, Zone
from gcg_sim.testkit import (
    Scenario,
    attack,
    block,
    end_main,
    has_action,
    no,
    pass_all,
    play,
    select,
    to_next_turn,
    yes,
    zone_of,
)

VANILLA_2_2 = "GD01-060"  # Zaku Mariner, Lv2 cost1 2/2, no effects
BLOCKER = "GD01-072"  # Launcher Strike Gundam, <Blocker>
BURST_ADD = "GD01-097"  # Guel Jeturk, 【Burst】Add this card to your hand.


@pytest.mark.rule("3-2-4", "3-2-6-3")
def test_newly_deployed_unit_cannot_attack() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    card = sc.add(0, VANILLA_2_2, Zone.HAND)
    st = sc.start()
    play(st, card)
    assert zone_of(st, card) is Zone.BATTLE
    assert not has_action(st, ActionKind.ATTACK, card)


@pytest.mark.rule("8-5-2-2", "1-2-2-1", "11-2-1-1")
def test_battle_damage_with_empty_shield_area_wins() -> None:
    sc = Scenario()
    unit = sc.add(0, VANILLA_2_2)
    st = sc.start()
    attack(st, unit, PLAYER_TARGET)
    pass_all(st)
    assert st.winner == 0
    assert st.end_reason is EndReason.BATTLE_DAMAGE


@pytest.mark.rule("8-5-2-3", "8-5-2-3-1", "4-6-4-2")
def test_attack_destroys_top_shield() -> None:
    sc = Scenario()
    unit = sc.add(0, VANILLA_2_2)
    top, second = sc.shields(1, VANILLA_2_2, VANILLA_2_2)
    st = sc.start()
    attack(st, unit)
    pass_all(st)
    assert zone_of(st, top) is Zone.TRASH
    assert zone_of(st, second) is Zone.SHIELD


@pytest.mark.rule("8-3-1", "13-1-4-1", "8-5-3-2")
def test_blocker_redirects_and_units_trade() -> None:
    sc = Scenario()
    attacker = sc.add(0, VANILLA_2_2)
    blocker = sc.add(1, BLOCKER)
    st = sc.start()
    attack(st, attacker)
    assert st.pending is not None and st.pending.kind is DecisionKind.BLOCK
    block(st, blocker)
    pass_all(st)
    assert st.cards[blocker].rested
    assert zone_of(st, attacker) is Zone.TRASH  # Launcher Strike deals 2 to the 2-HP Zaku


@pytest.mark.rule("13-2-5-1", "5-10-3")
@pytest.mark.card("GD01-097")
def test_burst_add_to_hand() -> None:
    sc = Scenario()
    unit = sc.add(0, VANILLA_2_2)
    (shield,) = sc.shields(1, BURST_ADD)
    st = sc.start()
    attack(st, unit)
    pass_all(st)
    assert st.pending is not None and st.pending.kind is DecisionKind.BURST
    yes(st)
    assert zone_of(st, shield) is Zone.HAND


@pytest.mark.rule("13-2-5-2")
def test_declining_burst_trashes_shield() -> None:
    sc = Scenario()
    unit = sc.add(0, VANILLA_2_2)
    (shield,) = sc.shields(1, BURST_ADD)
    st = sc.start()
    attack(st, unit)
    pass_all(st)
    no(st)
    assert zone_of(st, shield) is Zone.TRASH


@pytest.mark.rule("7-6-5-1", "4-8-4")
def test_hand_limit_discards_down_to_ten() -> None:
    sc = Scenario()
    sc.hand(0, *([VANILLA_2_2] * 12))
    st = sc.start()
    end_main(st)
    pass_all(st)
    assert st.pending is not None and st.pending.kind is DecisionKind.DISCARD
    hand = list(st.zones[0][Zone.HAND])
    select(st, hand[0])
    select(st, hand[1])
    assert len(st.zones[0][Zone.HAND]) == 10


@pytest.mark.rule("7-3-1", "7-4-1")
def test_draw_and_resource_phases() -> None:
    sc = Scenario(active=1)
    sc.resource_deck(0, 3)
    st = sc.start()
    hand_before = len(st.zones[0][Zone.HAND])
    to_next_turn(st)
    assert st.active == 0
    assert len(st.zones[0][Zone.HAND]) == hand_before + 1
    assert len(st.zones[0][Zone.RESOURCE_AREA]) == 1
