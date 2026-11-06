"""Tests for the card rulings of October 16, 2026 on cards released before GD06 (data refresh
2026-11-06). The GD06 rulings of that date are in ``test_wp_gd06_a.py`` / ``test_wp_gd06_b.py``."""

from __future__ import annotations

import pytest

from gcg_sim.engine import view as V
from gcg_sim.engine.game import apply
from gcg_sim.engine.state import GameState
from gcg_sim.engine.types import PLAYER_TARGET, ActionKind, DecisionKind, Zone
from gcg_sim.testkit import (
    Scenario,
    attack,
    options,
    order,
    pass_all,
    play,
    select_if_asked,
    yes,
    zone_of,
)

A = ActionKind
VANILLA = "GD01-060"  # Zaku Mariner: Lv2, AP2 HP2, no effects
BIG = "GD03-047"  # DINN (Commander Type): Lv4, AP3 HP4, no effects
SETSUNA = "ST07-009"  # Pilot AP+2: 【Attack】This Unit gets AP+1 during this turn
CHAR = "ST03-011"  # Pilot AP+1: 【Attack】During this turn, this Unit gets AP+1
KELLEY = "GD06-095"  # Pilot: 【Attack】You may deal 1 damage to this Unit. If you do, ...
INDIGNATION = "ST03-012"  # Command: 1 friendly Unit gets AP+2 during this turn


def pending(st: GameState) -> DecisionKind | None:
    return st.pending.kind if st.pending is not None else None


def hand_count(st: GameState, player: int = 0) -> int:
    return len(st.zones[player][Zone.HAND])


def order_first(st: GameState, card_number: str) -> None:
    """Resolve the waiting trigger of ``card_number`` first (an ORDER_TRIGGER decision)."""
    def_id = V.reg().db[card_number].def_id
    batch = st.batches[-1]
    for o in options(st):
        if o.kind is A.ORDER and batch[o.a].ability_key[0] == def_id:
            apply(st, o)
            return
    raise AssertionError(f"no waiting trigger of {card_number}")


def settle(st: GameState) -> None:
    for _ in range(20):
        pass_all(st)
        if pending(st) is DecisionKind.ORDER_TRIGGER:
            order(st)
        else:
            return


def pass_until(st: GameState, player: int) -> None:
    for _ in range(10):
        if pending(st) is DecisionKind.ACTION_STEP and st.pending is not None:
            if st.pending.player == player:
                return
            pass_once(st)
        else:
            return


def pass_once(st: GameState) -> None:
    for o in options(st):
        if o.kind is A.PASS:
            apply(st, o)
            return
    raise AssertionError("no pass")


def play_action(st: GameState, uid: int) -> None:
    for o in options(st):
        if o.kind is A.PLAY_COMMAND and o.a == uid:
            apply(st, o)
            return
    raise AssertionError("command not playable")


# ---------------------------------------------------------------------------------------------
# 【Attack】 conditions read after the Pilot's 【Attack】 effect resolved


@pytest.mark.card("GD01-050")
@pytest.mark.ruling("GD01-050:Q550")
def test_gd01_050_pilot_attack_effect_can_lift_it_to_5_ap_first() -> None:
    sc = Scenario()
    lagowe = sc.add(0, "GD01-050", pilot=SETSUNA)
    target = sc.add(1, BIG, rested=True)
    other = sc.add(1, BIG)
    st = sc.start()
    assert V.ap_of(st, V.derived(st), lagowe) == 4
    attack(st, lagowe, target)
    order_first(st, SETSUNA)
    assert V.ap_of(st, V.derived(st), lagowe) == 5
    select_if_asked(st, other)
    assert st.cards[other].damage == 2
    settle(st)


@pytest.mark.card("GD02-045")
@pytest.mark.ruling("GD02-045:Q551")
def test_gd02_045_pilot_attack_effect_can_lift_it_to_5_ap_first() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    ginn = sc.add(0, "GD02-045", pilot=CHAR)
    indignation = sc.add(0, INDIGNATION, Zone.HAND)
    target = sc.add(1, BIG, rested=True)
    st = sc.start()
    play(st, indignation)
    select_if_asked(st, ginn)
    assert V.ap_of(st, V.derived(st), ginn) == 4
    before = hand_count(st)
    attack(st, ginn, target)
    order_first(st, CHAR)
    assert hand_count(st) == before + 1
    settle(st)


@pytest.mark.card("GD02-054")
@pytest.mark.ruling("GD02-054:Q552")
def test_gd02_054_pilot_attack_effect_can_damage_it_first() -> None:
    sc = Scenario()
    barbatos = sc.add(0, "GD02-054", pilot=KELLEY)
    sc.shields(1, VANILLA, VANILLA)
    st = sc.start()
    before = hand_count(st)
    attack(st, barbatos, PLAYER_TARGET)
    order_first(st, KELLEY)
    yes(st)
    assert st.cards[barbatos].damage == 1
    assert hand_count(st) == before + 1
    settle(st)


# ---------------------------------------------------------------------------------------------
# "Choose 1 rested friendly (X) Unit. Change a battling enemy Unit's attack target to it."


@pytest.mark.card("EB01-077", "GD05-108", "ST07-013")
@pytest.mark.ruling("EB01-077:Q555", "GD05-108:Q553", "ST07-013:Q549")
@pytest.mark.parametrize(
    ("command", "defender"),
    [("EB01-077", "EB01-016"), ("GD05-108", "GD01-084"), ("ST07-013", "GD04-064")],
)
def test_the_attacked_rested_unit_itself_may_be_chosen(command: str, defender: str) -> None:
    sc = Scenario(active=1)
    sc.resources(0, 4)
    cmd = sc.add(0, command, Zone.HAND)
    target = sc.add(0, defender, rested=True)
    attacker = sc.add(1, VANILLA)
    st = sc.start()
    attack(st, attacker, target)
    pass_until(st, 0)
    play_action(st, cmd)
    select_if_asked(st, target)
    battle_ = st.battle
    assert battle_ is None or battle_.target == target
    settle(st)
    assert zone_of(st, attacker) is Zone.TRASH


# ---------------------------------------------------------------------------------------------
# GD05-120 Shining Finger


@pytest.mark.card("GD05-120")
@pytest.mark.ruling("GD05-120:Q554")
def test_gd05_120_may_choose_an_enemy_unit_that_is_already_rested() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    finger = sc.add(0, "GD05-120", Zone.HAND)
    rested = sc.add(1, BIG, rested=True)
    st = sc.start()
    play(st, finger)
    assert st.pending is not None
    select_if_asked(st, rested)
    assert st.cards[rested].rested
    assert zone_of(st, finger) is Zone.TRASH


# ---------------------------------------------------------------------------------------------
# "That player chooses 2 Unit cards from their trash. Exile them from the game."


@pytest.mark.card("ST13-009")
@pytest.mark.ruling("ST13-009:Q556")
def test_st13_009_one_unit_card_in_the_trash_is_not_exiled() -> None:
    sc = Scenario()
    pharact = sc.add(0, "ST13-009")
    sc.shields(1, VANILLA)
    (lone,) = sc.trash(1, VANILLA)
    st = sc.start()
    attack(st, pharact, PLAYER_TARGET)
    settle(st)
    assert zone_of(st, lone) is Zone.TRASH


@pytest.mark.card("ST14-003")
@pytest.mark.ruling("ST14-003:Q557")
def test_st14_003_one_unit_card_in_the_trash_is_not_exiled() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    athene = sc.add(0, "ST14-003", Zone.HAND)
    (lone,) = sc.trash(1, VANILLA)
    st = sc.start()
    play(st, athene)
    assert zone_of(st, lone) is Zone.TRASH
