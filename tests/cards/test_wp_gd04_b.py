"""Card behaviour and ruling tests for GD04-075..GD04-130 (work package WP-GD04-B)."""

from __future__ import annotations

from typing import Any

import pytest

from gcg_sim.effects import dsl as d
from gcg_sim.engine import core
from gcg_sim.engine import view as V
from gcg_sim.engine.game import SUPPORT_AID
from gcg_sim.engine.state import GameState
from gcg_sim.engine.types import PLAYER_TARGET, ActionKind, DecisionKind, Zone
from gcg_sim.testkit import (
    Scenario,
    act,
    activate,
    ap,
    attack,
    block,
    card_numbers,
    has_action,
    keywords,
    no,
    options,
    pass_all,
    play,
    select,
    to_next_turn,
    yes,
    zone_of,
)

A = ActionKind

VANILLA = "GD01-060"  # Zaku Mariner, Lv2 2/2
LAUNCHER = "GD01-072"  # Launcher Strike Gundam, Lv4 3/4 <Blocker>
BLAST_IMPULSE = "ST09-007"  # Lv5 5/3 <Blocker>
GN_ARMOR_E = "GD03-057"  # Lv6 5/4 (CB) <Blocker>
AGRISSA = "GD04-079"  # Lv5 5/4 (Superpower Bloc), links Ali al-Saachez
GUEL_DILANZA = "GD01-083"  # Lv2 2/2 (Academy), links Suletta / Elan
BEGUIR = "GD01-084"  # Lv2 2/3 (Academy)
LUNA_ZAKU = "GD04-062"  # Lv3 2/4 (ZAFT)(Minerva Squad), links Rey / Lunamaria
ZAKU_WARRIOR = "ST09-005"  # Lv2 2/2 (ZAFT)(Minerva Squad)
WISE_WALLABY = "GD04-059"  # Lv2 2/2 (Vulture), links Pala Sys / Ennil El
ESPERANSA = "GD04-060"  # Lv3 1/4 (Vulture), links Ennil El
SNIPER = "GD01-048"  # Zaku I Sniper Type, Lv2 0/1 (Zeon)
RICK_DOM = "GD01-030"  # Lv3 3/3 (Zeon) <Breach 2>
SHOKEW = "GD04-014"  # Lv2 2/3 (League Militaire)
CORE_BOOSTER = "GD02-012"  # Lv2 2/2 (Earth Federation)(White Base Team)
ZERO_GUNDAM = "GD03-063"  # Lv2 2/2 (CB)
EXIA = "GD04-064"  # Lv2 2/3 (CB)
KYRIOS = "ST07-008"  # Lv2 3/1 (CB), links Hallelujah
PARTS = "T-021"  # [Parts] (League Militaire) 1/1 token
ALVAARON = "T-024"  # [Alvaaron] (UN) 4/1 token
UN_COMMAND = "GD03-122"  # Veteran Tactics, a (Superpower Bloc)(UN) Command
RIDDHE_OLD = "GD01-089"  # a non-Newtype Pilot


def ex_count(st: GameState, p: int) -> int:
    return sum(
        1 for u in st.zones[p][Zone.RESOURCE_AREA] if V.cdef(st, u).card_number.startswith("EXR")
    )


def tokens(st: GameState, p: int, number: str) -> list[int]:
    return [u for u in st.zones[p][Zone.BATTLE] if V.cdef(st, u).card_number == number]


def in_hand(st: GameState, p: int, number: str) -> list[int]:
    return [u for u in st.zones[p][Zone.HAND] if V.cdef(st, u).card_number == number]


# ---------------------------------------------------------------------------------------------
# GD04-075 GN-X


@pytest.mark.card("GD04-075")
def test_gd04_075_cost_reduced_by_un_superpower_commands_in_trash() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    gnx = sc.add(0, "GD04-075", Zone.HAND)
    sc.trash(0, UN_COMMAND, "GD04-118", "GD04-109", LAUNCHER)
    st = sc.start()
    assert V.play_cost(st, V.derived(st), gnx) == 4
    play(st, gnx)
    assert zone_of(st, gnx) is Zone.BATTLE
    assert all(st.cards[u].rested for u in st.zones[0][Zone.RESOURCE_AREA])


@pytest.mark.card("GD04-075")
def test_gd04_075_no_reduction_without_matching_commands() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    gnx = sc.add(0, "GD04-075", Zone.HAND)
    sc.trash(0, "GD04-109", "GD04-080")
    st = sc.start()
    assert V.play_cost(st, V.derived(st), gnx) == 6
    assert not has_action(st, A.PLAY_UNIT, gnx)


# ---------------------------------------------------------------------------------------------
# GD04-077 Flat (Militia)


@pytest.mark.card("GD04-077")
@pytest.mark.rule("13-1-4-1")
def test_gd04_077_blocker_redirects_attack() -> None:
    sc = Scenario()
    attacker = sc.add(0, VANILLA)
    flat = sc.add(1, "GD04-077")
    st = sc.start()
    assert keywords(st, flat) == {"Blocker": 1}
    attack(st, attacker)
    block(st, flat)
    pass_all(st)
    assert st.cards[flat].damage == 2
    assert zone_of(st, attacker) is Zone.TRASH


# ---------------------------------------------------------------------------------------------
# GD04-080 Alvatore


def _alvatore_dies(other_friendly: str) -> tuple[GameState, int]:
    sc = Scenario()
    alvatore = sc.add(0, "GD04-080")
    sc.add(0, other_friendly)
    sc.add(1, AGRISSA, rested=True)
    st = sc.start()
    attack(st, alvatore, st.zones[1][Zone.BATTLE][0])
    pass_all(st)
    return st, alvatore


@pytest.mark.card("GD04-080")
@pytest.mark.rule("13-2-8-1", "5-17-2-1")
def test_gd04_080_destroyed_with_another_un_unit_deploys_rested_alvaaron() -> None:
    st, alvatore = _alvatore_dies(AGRISSA)
    assert zone_of(st, alvatore) is Zone.TRASH
    (token,) = tokens(st, 0, ALVAARON)
    assert st.cards[token].rested
    assert ap(st, token) == 4


@pytest.mark.card("GD04-080")
def test_gd04_080_this_unit_does_not_count_as_another() -> None:
    st, alvatore = _alvatore_dies(VANILLA)
    assert zone_of(st, alvatore) is Zone.TRASH
    assert tokens(st, 0, ALVAARON) == []


# ---------------------------------------------------------------------------------------------
# GD04-081 Üso Ewin


@pytest.mark.card("GD04-081")
@pytest.mark.rule("3-3-9-2")
def test_gd04_081_paired_with_league_militaire_unit_deploys_parts() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    unit = sc.add(0, SHOKEW)
    uso = sc.add(0, "GD04-081", Zone.HAND)
    st = sc.start()
    play(st, uso, onto=unit)
    (parts,) = tokens(st, 0, PARTS)
    assert not st.cards[parts].rested
    assert V.traits_of(st, V.derived(st), parts) == ("League Militaire",)


@pytest.mark.card("GD04-081")
def test_gd04_081_paired_with_other_unit_deploys_nothing() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    unit = sc.add(0, VANILLA)
    uso = sc.add(0, "GD04-081", Zone.HAND)
    st = sc.start()
    play(st, uso, onto=unit)
    assert tokens(st, 0, PARTS) == []


@pytest.mark.card("GD04-081")
@pytest.mark.rule("13-2-5-1")
def test_gd04_081_burst_adds_to_hand() -> None:
    sc = Scenario()
    attacker = sc.add(0, VANILLA)
    (shield,) = sc.shields(1, "GD04-081")
    st = sc.start()
    attack(st, attacker)
    pass_all(st)
    yes(st)
    assert zone_of(st, shield) is Zone.HAND


# ---------------------------------------------------------------------------------------------
# GD04-082 Rosamia Badam


@pytest.mark.card("GD04-082")
def test_gd04_082_when_linked_damages_a_rested_unit() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    unit = sc.add(0, "GD02-015")
    enemy = sc.add(1, VANILLA, rested=True)
    sc.add(1, VANILLA)
    rosamia = sc.add(0, "GD04-082", Zone.HAND)
    st = sc.start()
    play(st, rosamia, onto=unit)
    assert st.cards[enemy].damage == 1


@pytest.mark.card("GD04-082")
def test_gd04_082_rested_unit_may_be_friendly() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    unit = sc.add(0, "GD02-015")
    mine = sc.add(0, BEGUIR, rested=True)
    rosamia = sc.add(0, "GD04-082", Zone.HAND)
    st = sc.start()
    play(st, rosamia, onto=unit)
    assert st.cards[mine].damage == 1


@pytest.mark.card("GD04-082")
def test_gd04_082_no_link_no_damage() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    unit = sc.add(0, VANILLA)
    enemy = sc.add(1, VANILLA, rested=True)
    rosamia = sc.add(0, "GD04-082", Zone.HAND)
    st = sc.start()
    play(st, rosamia, onto=unit)
    assert st.cards[enemy].damage == 0


# ---------------------------------------------------------------------------------------------
# GD04-083 Marbet Fingerhat


@pytest.mark.card("GD04-083")
@pytest.mark.rule("3-3-9-2")
def test_gd04_083_paired_unit_buffs_your_league_militaire_tokens() -> None:
    sc = Scenario()
    sc.add(0, VANILLA, pilot="GD04-083")
    my_parts = sc.add(0, PARTS)
    league_unit = sc.add(0, SHOKEW)
    enemy_parts = sc.add(1, PARTS)
    st = sc.start()
    assert ap(st, my_parts) == 2
    assert ap(st, league_unit) == 2
    assert ap(st, enemy_parts) == 1


@pytest.mark.card("GD04-083")
def test_gd04_083_no_effect_while_not_paired() -> None:
    sc = Scenario()
    sc.add(0, "GD04-083", Zone.HAND)
    my_parts = sc.add(0, PARTS)
    st = sc.start()
    assert ap(st, my_parts) == 1


# ---------------------------------------------------------------------------------------------
# GD04-084 Sleggar Law


@pytest.mark.card("GD04-084")
def test_gd04_084_attack_gives_white_base_team_unit_ap() -> None:
    sc = Scenario()
    attacker = sc.add(0, VANILLA, pilot="GD04-084")
    wbt = sc.add(0, CORE_BOOSTER)
    st = sc.start()
    attack(st, attacker)
    assert ap(st, wbt) == 3
    assert ap(st, attacker) == 3


# ---------------------------------------------------------------------------------------------
# GD04-085 Suletta Mercury


def _suletta(ex: int, normal: int) -> tuple[GameState, int]:
    sc = Scenario()
    sc.resources(0, normal, ex=ex)
    sc.add(0, GUEL_DILANZA, pilot="GD04-085")
    witches = sc.add(0, "GD04-108", Zone.HAND)
    st = sc.start()
    return st, witches


@pytest.mark.card("GD04-085")
@pytest.mark.rule("5-17-3-2-3", "13-2-12-1")
def test_gd04_085_academy_command_paid_with_last_ex_places_rested_ex() -> None:
    st, witches = _suletta(ex=1, normal=3)
    play(st, witches, ex=1)
    assert ex_count(st, 0) == 1
    (ex_uid,) = [
        u for u in st.zones[0][Zone.RESOURCE_AREA] if V.cdef(st, u).card_number.startswith("EXR")
    ]
    assert st.cards[ex_uid].rested


@pytest.mark.card("GD04-085")
def test_gd04_085_not_when_paid_without_ex() -> None:
    st, witches = _suletta(ex=1, normal=3)
    play(st, witches, ex=0)
    assert ex_count(st, 0) == 1
    assert all(
        not st.cards[u].rested
        for u in st.zones[0][Zone.RESOURCE_AREA]
        if V.cdef(st, u).card_number.startswith("EXR")
    )


@pytest.mark.card("GD04-085")
def test_gd04_085_not_when_ex_resources_remain() -> None:
    st, witches = _suletta(ex=2, normal=2)
    play(st, witches, ex=1)
    assert ex_count(st, 0) == 1


@pytest.mark.card("GD04-085")
def test_gd04_085_requires_link() -> None:
    sc = Scenario()
    sc.resources(0, 3, ex=1)
    sc.add(0, VANILLA, pilot="GD04-085")
    witches = sc.add(0, "GD04-108", Zone.HAND)
    sc.add(0, GUEL_DILANZA)
    st = sc.start()
    play(st, witches, ex=1)
    assert ex_count(st, 0) == 0


# ---------------------------------------------------------------------------------------------
# GD04-086 Garma Zabi


def _garma(unit: str, ex: int) -> GameState:
    sc = Scenario()
    sc.resources(0, 1, ex=ex)
    gelgoog = sc.add(0, unit, pilot="GD04-086")
    sc.add(1, AGRISSA, rested=True)
    st = sc.start()
    attack(st, gelgoog, st.zones[1][Zone.BATTLE][0])
    pass_all(st)
    assert zone_of(st, gelgoog) is Zone.TRASH
    return st


@pytest.mark.card("GD04-086")
@pytest.mark.rule("13-2-8-2", "13-2-12-1")
def test_gd04_086_linked_unit_destroyed_places_ex_resource() -> None:
    st = _garma("GD01-031", ex=0)
    assert ex_count(st, 0) == 1


@pytest.mark.card("GD04-086")
def test_gd04_086_nothing_when_you_have_an_ex_resource() -> None:
    st = _garma("GD01-031", ex=1)
    assert ex_count(st, 0) == 1
    assert len(st.zones[0][Zone.RESOURCE_AREA]) == 2


@pytest.mark.card("GD04-086")
def test_gd04_086_requires_link() -> None:
    st = _garma(VANILLA, ex=0)
    assert ex_count(st, 0) == 0


# ---------------------------------------------------------------------------------------------
# GD04-087 Elan Ceres (Enhanced Person Number 5)


def _elan_attack() -> tuple[GameState, int, int, int]:
    sc = Scenario()
    sc.resources(0, 3)
    elan_unit = sc.add(0, GUEL_DILANZA, pilot="GD04-087")
    beguir = sc.add(0, BEGUIR)
    enemy = sc.add(1, AGRISSA, rested=True)
    dc = sc.add(0, "GD04-113", Zone.HAND)
    st = sc.start()
    attack(st, elan_unit, enemy)
    assert st.pending is not None and st.pending.kind is DecisionKind.SELECT
    return st, elan_unit, beguir, dc


@pytest.mark.card("GD04-087")
@pytest.mark.rule("10-1-9-1")
def test_gd04_087_battle_damage_redirected_to_chosen_academy_unit() -> None:
    st, elan_unit, beguir, _ = _elan_attack()
    select(st, beguir)
    pass_all(st)
    assert zone_of(st, elan_unit) is Zone.BATTLE
    assert st.cards[elan_unit].damage == 0
    assert zone_of(st, beguir) is Zone.TRASH


@pytest.mark.card("GD04-087")
def test_gd04_087_choice_is_optional() -> None:
    st, elan_unit, beguir, _ = _elan_attack()
    act(st, A.DONE)
    pass_all(st)
    assert zone_of(st, elan_unit) is Zone.TRASH
    assert st.cards[beguir].damage == 0


@pytest.mark.card("GD04-087")
@pytest.mark.ruling("GD04-087:Q280")
def test_gd04_087_q280_redirected_damage_is_battle_damage() -> None:
    st, _, beguir, dc = _elan_attack()
    select(st, beguir)
    assert st.pending is not None and st.pending.kind is DecisionKind.ACTION_STEP
    play(st, dc)
    select(st, beguir)
    pass_all(st)
    assert zone_of(st, beguir) is Zone.BATTLE
    assert st.cards[beguir].damage == 2  # 5 battle damage reduced by 3


# ---------------------------------------------------------------------------------------------
# GD04-088 Tokwan


@pytest.mark.card("GD04-088")
@pytest.mark.rule("13-1-4-1")
def test_gd04_088_blocked_by_lv4_or_lower_takes_no_battle_damage() -> None:
    sc = Scenario()
    unit = sc.add(0, VANILLA, pilot="GD04-088")
    blocker = sc.add(1, LAUNCHER)
    st = sc.start()
    attack(st, unit)
    block(st, blocker)
    pass_all(st)
    assert zone_of(st, unit) is Zone.BATTLE
    assert st.cards[unit].damage == 0
    assert zone_of(st, blocker) is Zone.TRASH


@pytest.mark.card("GD04-088")
def test_gd04_088_blocked_by_lv5_takes_damage() -> None:
    sc = Scenario()
    unit = sc.add(0, VANILLA, pilot="GD04-088")
    blocker = sc.add(1, BLAST_IMPULSE)
    st = sc.start()
    attack(st, unit)
    block(st, blocker)
    pass_all(st)
    assert zone_of(st, unit) is Zone.TRASH


@pytest.mark.card("GD04-088")
def test_gd04_088_unblocked_attack_on_unit_takes_damage() -> None:
    sc = Scenario()
    unit = sc.add(0, VANILLA, pilot="GD04-088")
    target = sc.add(1, LAUNCHER, rested=True)
    st = sc.start()
    attack(st, unit, target)
    pass_all(st)
    assert zone_of(st, unit) is Zone.TRASH


# ---------------------------------------------------------------------------------------------
# GD04-089 Nena Trinity


@pytest.mark.card("GD04-089")
@pytest.mark.rule("13-1-3-1")
def test_gd04_089_paired_unit_gains_support_2() -> None:
    sc = Scenario()
    unit = sc.add(0, VANILLA, pilot="GD04-089")
    other = sc.add(0, VANILLA)
    st = sc.start()
    assert keywords(st, unit) == {"Support": 2}
    activate(st, unit, SUPPORT_AID)
    assert st.cards[unit].rested
    assert ap(st, other) == 4


# ---------------------------------------------------------------------------------------------
# GD04-090 Hallelujah Haptism


def _hallelujah(top: str, target: str) -> tuple[GameState, int, int]:
    sc = Scenario()
    kyrios = sc.add(0, KYRIOS, pilot="GD04-090")
    enemy = sc.add(1, target, rested=True)
    sc.deck(0, top)
    st = sc.start()
    top_uid = st.zones[0][Zone.DECK][0]
    attack(st, kyrios, enemy)
    pass_all(st)
    return st, kyrios, top_uid


@pytest.mark.card("GD04-090")
def test_gd04_090_destroying_enemy_adds_top_cb_card() -> None:
    st, _, top = _hallelujah(ZERO_GUNDAM, VANILLA)
    assert st.pending is not None and st.pending.kind is DecisionKind.SELECT
    select(st, top)
    assert zone_of(st, top) is Zone.HAND


@pytest.mark.card("GD04-090")
def test_gd04_090_non_cb_top_card_goes_to_bottom() -> None:
    st, _, top = _hallelujah(VANILLA, VANILLA)
    assert zone_of(st, top) is Zone.DECK
    assert st.zones[0][Zone.DECK][-1] == top


@pytest.mark.card("GD04-090")
@pytest.mark.ruling("GD04-090:Q281")
@pytest.mark.rule("10-1-6-4")
def test_gd04_090_q281_triggers_when_both_units_are_destroyed() -> None:
    st, kyrios, top = _hallelujah(ZERO_GUNDAM, AGRISSA)
    assert zone_of(st, kyrios) is Zone.TRASH
    select(st, top)
    assert zone_of(st, top) is Zone.HAND


# ---------------------------------------------------------------------------------------------
# GD04-091 Deux Murasame


@pytest.mark.card("GD04-091")
@pytest.mark.rule("13-2-8-2")
def test_gd04_091_destroyed_deals_1_to_an_undamaged_enemy() -> None:
    sc = Scenario()
    unit = sc.add(0, SNIPER, pilot="GD04-091")
    damaged_target = sc.add(1, LAUNCHER, rested=True)
    undamaged = sc.add(1, VANILLA)
    st = sc.start()
    attack(st, unit, damaged_target)
    pass_all(st)
    assert zone_of(st, unit) is Zone.TRASH
    assert st.cards[damaged_target].damage == 2
    assert st.cards[undamaged].damage == 1


@pytest.mark.card("GD04-091")
def test_gd04_091_no_undamaged_enemy_no_damage() -> None:
    sc = Scenario()
    unit = sc.add(0, SNIPER, pilot="GD04-091")
    target = sc.add(1, LAUNCHER, rested=True)
    other = sc.add(1, BEGUIR, damage=1)
    st = sc.start()
    attack(st, unit, target)
    pass_all(st)
    assert zone_of(st, unit) is Zone.TRASH
    assert st.cards[target].damage == 2
    assert st.cards[other].damage == 1


# ---------------------------------------------------------------------------------------------
# GD04-092 Michael Trinity


@pytest.mark.card("GD04-092")
def test_gd04_092_when_linked_damages_a_damaged_enemy() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    unit = sc.add(0, GN_ARMOR_E)
    damaged = sc.add(1, LAUNCHER, damage=1)
    undamaged = sc.add(1, VANILLA)
    michael = sc.add(0, "GD04-092", Zone.HAND)
    st = sc.start()
    play(st, michael, onto=unit)
    assert st.cards[damaged].damage == 2
    assert st.cards[undamaged].damage == 0


@pytest.mark.card("GD04-092")
def test_gd04_092_no_link_no_damage() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    unit = sc.add(0, VANILLA)
    damaged = sc.add(1, LAUNCHER, damage=1)
    michael = sc.add(0, "GD04-092", Zone.HAND)
    st = sc.start()
    play(st, michael, onto=unit)
    assert st.cards[damaged].damage == 1


# ---------------------------------------------------------------------------------------------
# GD04-093 Rey Za Burrel


def _rey_linked() -> tuple[GameState, int, int]:
    sc = Scenario()
    sc.resources(0, 6)
    unit = sc.add(0, LUNA_ZAKU)
    enemy = sc.add(1, AGRISSA, rested=True)
    rey = sc.add(0, "GD04-093", Zone.HAND)
    sc.hand(0, "GD04-114")
    st = sc.start()
    play(st, rey, onto=unit)
    return st, unit, enemy


@pytest.mark.card("GD04-093")
@pytest.mark.ruling("GD04-093:Q282")
@pytest.mark.rule("5-21-1")
def test_gd04_093_q282_next_damage_reduced_by_2() -> None:
    st, unit, enemy = _rey_linked()
    attack(st, unit, enemy)
    pass_all(st)
    assert st.cards[unit].damage == 3  # 5 battle damage reduced by 2


@pytest.mark.card("GD04-093")
def test_gd04_093_only_the_next_damage_is_reduced() -> None:
    st, unit, enemy = _rey_linked()
    attack(st, unit, enemy)
    pass_all(st)
    (reformationist,) = in_hand(st, 0, "GD04-114")
    play(st, reformationist)
    assert st.cards[unit].damage == 4


@pytest.mark.card("GD04-093")
def test_gd04_093_requires_zaft_link_unit() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    unit = sc.add(0, VANILLA)
    enemy = sc.add(1, AGRISSA, rested=True)
    rey = sc.add(0, "GD04-093", Zone.HAND)
    st = sc.start()
    play(st, rey, onto=unit)
    attack(st, unit, enemy)
    pass_all(st)
    assert zone_of(st, unit) is Zone.TRASH


# ---------------------------------------------------------------------------------------------
# GD04-094 Pala Sys


@pytest.mark.card("GD04-094")
def test_gd04_094_when_linked_adds_purple_suppression_unit_from_trash() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    unit = sc.add(0, WISE_WALLABY)
    (dx,) = sc.trash(0, "GD04-049")
    pala = sc.add(0, "GD04-094", Zone.HAND)
    st = sc.start()
    play(st, pala, onto=unit)
    assert zone_of(st, dx) is Zone.HAND


@pytest.mark.card("GD04-094")
def test_gd04_094_units_without_suppression_stay_in_trash() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    unit = sc.add(0, WISE_WALLABY)
    trash = sc.trash(0, ESPERANSA, "GD01-041")
    pala = sc.add(0, "GD04-094", Zone.HAND)
    st = sc.start()
    play(st, pala, onto=unit)
    assert [zone_of(st, u) for u in trash] == [Zone.TRASH, Zone.TRASH]


# ---------------------------------------------------------------------------------------------
# GD04-095 Lunamaria Hawke


def _luna_linked(luna_damage: int = 0) -> tuple[GameState, int, int, int]:
    sc = Scenario()
    sc.resources(0, 4)
    luna_unit = sc.add(0, LUNA_ZAKU, damage=luna_damage)
    warrior = sc.add(0, ZAKU_WARRIOR)
    enemy = sc.add(1, VANILLA, rested=True)
    luna = sc.add(0, "GD04-095", Zone.HAND)
    sc.hand(0, "GD04-113")
    st = sc.start()
    play(st, luna, onto=luna_unit)
    assert st.pending is not None and st.pending.kind is DecisionKind.SELECT
    select(st, warrior)
    return st, luna_unit, warrior, enemy


@pytest.mark.card("GD04-095")
@pytest.mark.rule("10-1-9-1")
def test_gd04_095_battle_damage_to_chosen_unit_dealt_to_this_unit() -> None:
    st, luna_unit, warrior, enemy = _luna_linked()
    attack(st, warrior, enemy)
    pass_all(st)
    assert zone_of(st, enemy) is Zone.TRASH
    assert st.cards[warrior].damage == 0
    assert st.cards[luna_unit].damage == 2


@pytest.mark.card("GD04-095")
@pytest.mark.ruling("GD04-095:Q283")
def test_gd04_095_q283_redirected_damage_is_battle_damage() -> None:
    st, luna_unit, warrior, enemy = _luna_linked()
    attack(st, warrior, enemy)
    (dc,) = in_hand(st, 0, "GD04-113")
    play(st, dc)
    select(st, luna_unit)
    pass_all(st)
    assert st.cards[luna_unit].damage == 0  # 2 battle damage reduced by 3
    assert st.cards[warrior].damage == 0


@pytest.mark.card("GD04-095")
@pytest.mark.ruling("GD04-095:Q283")
@pytest.mark.rule("8-5-3-2-3")
def test_gd04_095_q283_unit_killed_by_redirected_damage_is_destroyed_in_battle(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    destroyed: list[tuple[int, int]] = []
    original = core.emit

    def spy(st: GameState, ev: d.Ev, subject: int = -1, **kw: Any) -> None:
        if ev is d.Ev.DESTROYED:
            destroyed.append((subject, kw.get("battle", 0)))
        original(st, ev, subject, **kw)

    monkeypatch.setattr(core, "emit", spy)
    st, luna_unit, warrior, enemy = _luna_linked(luna_damage=3)
    attack(st, warrior, enemy)
    pass_all(st)
    assert zone_of(st, luna_unit) is Zone.TRASH
    assert (enemy, 1) in destroyed
    assert (luna_unit, 1) in destroyed


# ---------------------------------------------------------------------------------------------
# GD04-096 Ennil El


def _ennil_attack(target: str) -> tuple[GameState, int, int]:
    sc = Scenario()
    unit = sc.add(0, ESPERANSA, pilot="GD04-096")
    enemy = sc.add(1, target, rested=True)
    st = sc.start()
    attack(st, unit, enemy)
    pass_all(st)
    return st, unit, enemy


@pytest.mark.card("GD04-096")
def test_gd04_096_battle_damage_to_lv5_or_lower_destroys_it() -> None:
    st, unit, enemy = _ennil_attack(LAUNCHER)
    assert zone_of(st, enemy) is Zone.TRASH
    assert zone_of(st, unit) is Zone.BATTLE


@pytest.mark.card("GD04-096")
def test_gd04_096_lv6_enemy_survives() -> None:
    st, _, enemy = _ennil_attack(GN_ARMOR_E)
    assert zone_of(st, enemy) is Zone.BATTLE
    assert st.cards[enemy].damage == 2


@pytest.mark.card("GD04-096")
def test_gd04_096_requires_link() -> None:
    sc = Scenario()
    unit = sc.add(0, VANILLA, pilot="GD04-096")
    enemy = sc.add(1, LAUNCHER, rested=True)
    st = sc.start()
    attack(st, unit, enemy)
    pass_all(st)
    assert zone_of(st, enemy) is Zone.BATTLE


@pytest.mark.card("GD04-096", "GD04-113")
@pytest.mark.ruling("GD04-096:Q284")
@pytest.mark.rule("5-5-5")
def test_gd04_096_q284_zero_ap_deals_no_damage_and_destroys_nothing() -> None:
    sc = Scenario()
    unit = sc.add(0, ESPERANSA, pilot="GD04-096")
    other = sc.add(0, VANILLA)
    enemy = sc.add(1, LAUNCHER, rested=True)
    sc.shields(1, "GD04-113")
    st = sc.start()
    attack(st, other)
    pass_all(st)
    yes(st)  # opponent's Damage Control 【Burst】: my Esperansa gets AP-2
    select(st, unit)
    assert ap(st, unit) == 0
    attack(st, unit, enemy)
    pass_all(st)
    assert zone_of(st, enemy) is Zone.BATTLE
    assert st.cards[enemy].damage == 0


# ---------------------------------------------------------------------------------------------
# GD04-097 Loran Cehack


@pytest.mark.card("GD04-097")
def test_gd04_097_when_linked_returns_enemy_with_3_or_less_hp() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    unit = sc.add(0, "GD04-074")
    small = sc.add(1, VANILLA)
    big = sc.add(1, LAUNCHER)
    loran = sc.add(0, "GD04-097", Zone.HAND)
    st = sc.start()
    play(st, loran, onto=unit)
    assert zone_of(st, small) is Zone.HAND
    assert zone_of(st, big) is Zone.BATTLE


# ---------------------------------------------------------------------------------------------
# GD04-098 Riddhe Marcenas


def _riddhe(unit: str) -> tuple[GameState, int]:
    sc = Scenario(active=1)
    sc.resources(1, 5)
    mine = sc.add(0, unit, pilot="GD04-098")
    pressure = sc.add(1, "GD04-109", Zone.HAND)
    st = sc.start()
    play(st, pressure)
    return st, mine


@pytest.mark.card("GD04-098")
@pytest.mark.ruling("GD04-098:Q285")
@pytest.mark.rule("5-21-1")
def test_gd04_098_q285_enemy_effect_damage_reduced_by_2() -> None:
    st, mine = _riddhe("GD01-011")
    assert zone_of(st, mine) is Zone.BATTLE
    assert st.cards[mine].damage == 2


@pytest.mark.card("GD04-098")
def test_gd04_098_requires_link() -> None:
    st, mine = _riddhe(VANILLA)
    assert zone_of(st, mine) is Zone.TRASH


@pytest.mark.card("GD04-098")
def test_gd04_098_battle_damage_not_reduced() -> None:
    sc = Scenario()
    mine = sc.add(0, "GD01-011", pilot="GD04-098")
    enemy = sc.add(1, LAUNCHER, rested=True)
    st = sc.start()
    attack(st, mine, enemy)
    pass_all(st)
    assert st.cards[mine].damage == 3


# ---------------------------------------------------------------------------------------------
# GD04-099 Ali al-Saachez


def _ali_attack() -> tuple[GameState, int, int]:
    sc = Scenario()
    agrissa = sc.add(0, AGRISSA, pilot="GD04-099")
    enemy = sc.add(1, VANILLA, pilot=RIDDHE_OLD)
    st = sc.start()
    attack(st, agrissa)
    pilot = st.cards[enemy].pair
    return st, enemy, pilot


@pytest.mark.card("GD04-099")
def test_gd04_099_attack_may_return_enemy_pilot() -> None:
    st, enemy, pilot = _ali_attack()
    select(st, pilot)
    assert zone_of(st, pilot) is Zone.HAND
    assert st.cards[enemy].pair < 0


@pytest.mark.card("GD04-099")
def test_gd04_099_return_is_optional() -> None:
    st, enemy, pilot = _ali_attack()
    act(st, A.DONE)
    assert st.cards[enemy].pair == pilot


# ---------------------------------------------------------------------------------------------
# GD04-100 Sochie Heim


def _sochie() -> tuple[GameState, int]:
    sc = Scenario()
    sc.resources(0, 4)
    turn_a = sc.add(0, "GD04-073", pilot="GD04-100")
    st = sc.start()
    return st, turn_a


@pytest.mark.card("GD04-100")
@pytest.mark.rule("10-1-7-3")
def test_gd04_100_paying_for_a_unit_effect_may_add_that_much_ap() -> None:
    st, turn_a = _sochie()
    activate(st, turn_a)
    yes(st)
    assert ap(st, turn_a) == 3 + 0 + 2 + 1


@pytest.mark.card("GD04-100")
def test_gd04_100_increase_is_optional() -> None:
    st, turn_a = _sochie()
    activate(st, turn_a)
    no(st)
    assert ap(st, turn_a) == 5


@pytest.mark.card("GD04-100")
def test_gd04_100_once_per_turn() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    kapool = sc.add(0, "GD04-074", pilot="GD04-100")
    turn_a = sc.add(0, "GD04-073")
    sc.shields(1, VANILLA)
    st = sc.start()
    activate(st, turn_a)
    yes(st)
    assert ap(st, kapool) == 4
    attack(st, kapool)
    yes(st)  # Kapool: pay ① to draw 1, then discard 1
    assert sum(st.cards[u].rested for u in st.zones[0][Zone.RESOURCE_AREA]) == 2
    assert ap(st, kapool) == 4


@pytest.mark.card("GD04-100")
def test_gd04_100_base_effect_costs_do_not_count() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    unit = sc.add(0, ZERO_GUNDAM, pilot="GD04-100")
    warship = sc.base(0, "GD04-125")
    sc.add(1, VANILLA)
    st = sc.start()
    activate(st, warship)
    assert st.pending is not None and st.pending.kind is DecisionKind.MAIN
    assert ap(st, unit) == 2


@pytest.mark.card("GD04-100")
@pytest.mark.ruling("GD04-100:Q286")
def test_gd04_100_q286_paying_to_deploy_with_x_divider_counts() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    sochie_unit = sc.add(0, VANILLA, pilot="GD04-100")
    divider = sc.add(0, "GD03-051")
    (revived,) = sc.trash(0, VANILLA)
    jamil = sc.add(0, "GD03-096", Zone.HAND)
    st = sc.start()
    play(st, jamil, onto=divider)
    select(st, revived)
    assert zone_of(st, revived) is Zone.BATTLE
    yes(st)
    assert ap(st, sochie_unit) == 3


# ---------------------------------------------------------------------------------------------
# GD04-101 Kindhearted


def _kindhearted_then_enemy_action(enemy_command: str, protect: bool) -> tuple[GameState, int]:
    sc = Scenario()
    sc.resources(0, 3)
    sc.resources(1, 5)
    attacker = sc.add(0, LAUNCHER)
    victim = sc.add(0, VANILLA)
    kind = sc.add(0, "GD04-101", Zone.HAND)
    enemy_cmd = sc.add(1, enemy_command, Zone.HAND)
    sc.shields(1, VANILLA)
    st = sc.start()
    if protect:
        hand_before = len(st.zones[0][Zone.HAND])
        play(st, kind)
        assert len(st.zones[0][Zone.HAND]) == hand_before  # Kindhearted left, 1 card drawn
    attack(st, attacker)
    assert st.pending is not None and st.pending.player == 1
    play(st, enemy_cmd)
    if st.pending is not None and st.pending.kind is DecisionKind.SELECT:
        select(st, victim)
    pass_all(st)
    return st, victim


@pytest.mark.card("GD04-101")
@pytest.mark.rule("5-20-2")
def test_gd04_101_friendly_units_survive_enemy_destroy_effects() -> None:
    st, victim = _kindhearted_then_enemy_action("GD05-116", protect=True)
    assert zone_of(st, victim) is Zone.BATTLE


@pytest.mark.card("GD04-101")
def test_gd04_101_without_it_the_destroy_effect_works() -> None:
    st, victim = _kindhearted_then_enemy_action("GD05-116", protect=False)
    assert zone_of(st, victim) is Zone.TRASH


@pytest.mark.card("GD04-101")
@pytest.mark.ruling("GD04-101:Q287")
@pytest.mark.rule("5-5-2")
def test_gd04_101_q287_effect_damage_still_destroys() -> None:
    st, victim = _kindhearted_then_enemy_action("GD04-109", protect=True)
    assert zone_of(st, victim) is Zone.TRASH


@pytest.mark.card("GD04-101")
@pytest.mark.rule("13-2-5-1")
def test_gd04_101_burst_protects_during_opponents_turn_and_draws() -> None:
    sc = Scenario(active=1)
    sc.resources(1, 2)
    enemy = sc.add(1, VANILLA)
    mine = sc.add(0, VANILLA)
    (shield,) = sc.shields(0, "GD04-101")
    pride = sc.add(1, "GD05-116", Zone.HAND)
    st = sc.start()
    hand_before = len(st.zones[0][Zone.HAND])
    attack(st, enemy)
    pass_all(st)
    yes(st)
    assert len(st.zones[0][Zone.HAND]) == hand_before + 1
    assert zone_of(st, shield) is Zone.TRASH
    play(st, pride)
    assert zone_of(st, mine) is Zone.BATTLE


# ---------------------------------------------------------------------------------------------
# GD04-102 Moment of Rest


def _moment_of_rest() -> tuple[GameState, int, int]:
    sc = Scenario()
    sc.resources(0, 4)
    frozen = sc.add(1, AGRISSA, rested=True)
    other = sc.add(1, VANILLA, rested=True)
    mor = sc.add(0, "GD04-102", Zone.HAND)
    st = sc.start()
    play(st, mor)
    select(st, frozen)
    return st, frozen, other


@pytest.mark.card("GD04-102")
@pytest.mark.rule("7-2-3-1")
def test_gd04_102_chosen_unit_stays_rested_through_opponents_start_phase() -> None:
    st, frozen, other = _moment_of_rest()
    to_next_turn(st)
    assert st.active == 1
    assert st.cards[frozen].rested
    assert not st.cards[other].rested


@pytest.mark.card("GD04-102")
def test_gd04_102_only_that_next_turn() -> None:
    st, frozen, _ = _moment_of_rest()
    to_next_turn(st)
    to_next_turn(st)
    to_next_turn(st)
    assert st.active == 1
    assert not st.cards[frozen].rested


@pytest.mark.card("GD04-102")
@pytest.mark.rule("10-1-8-1-1")
def test_gd04_102_needs_a_rested_enemy_lv5_or_lower() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    sc.add(1, GN_ARMOR_E, rested=True)
    sc.add(1, VANILLA)
    mor = sc.add(0, "GD04-102", Zone.HAND)
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, mor)


@pytest.mark.card("GD04-102")
def test_gd04_102_burst_draws_1() -> None:
    sc = Scenario(active=1)
    enemy = sc.add(1, VANILLA)
    sc.shields(0, "GD04-102")
    st = sc.start()
    hand_before = len(st.zones[0][Zone.HAND])
    attack(st, enemy)
    pass_all(st)
    yes(st)
    assert len(st.zones[0][Zone.HAND]) == hand_before + 1


@pytest.mark.card("GD04-102")
@pytest.mark.ruling("GD04-102:Q288")
def test_gd04_102_q288_effects_may_still_set_it_active() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    attacker = sc.add(0, VANILLA)
    frozen = sc.add(1, LAUNCHER, rested=True)
    sc.shields(1, "GD01-121")
    mor = sc.add(0, "GD04-102", Zone.HAND)
    st = sc.start()
    play(st, mor)
    attack(st, attacker)
    pass_all(st)
    yes(st)  # opponent's Midair Modifications 【Burst】 activates its 【Main】
    assert not st.cards[frozen].rested


# ---------------------------------------------------------------------------------------------
# GD04-103 Spiritual Support


@pytest.mark.card("GD04-103")
@pytest.mark.rule("13-1-1-1")
def test_gd04_103_gains_repair_2_this_turn() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    unit = sc.add(0, LAUNCHER, damage=3)
    ss = sc.add(0, "GD04-103", Zone.HAND)
    st = sc.start()
    play(st, ss)
    assert keywords(st, unit) == {"Blocker": 1, "Repair": 2}
    to_next_turn(st)
    assert st.cards[unit].damage == 1
    assert "Repair" not in keywords(st, unit)


# ---------------------------------------------------------------------------------------------
# GD04-104 Shrike Team's Bulwark


@pytest.mark.card("GD04-104")
def test_gd04_104_rests_1_to_2_enemy_units_lv2_or_lower() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    low1 = sc.add(1, VANILLA)
    low2 = sc.add(1, ZAKU_WARRIOR)
    high = sc.add(1, LAUNCHER)
    stb = sc.add(0, "GD04-104", Zone.HAND)
    st = sc.start()
    play(st, stb)
    assert {o.a for o in options(st) if o.kind is A.SELECT} == {low1, low2}
    select(st, low1, low2)
    assert st.cards[low1].rested and st.cards[low2].rested
    assert not st.cards[high].rested


# ---------------------------------------------------------------------------------------------
# GD04-105 Encounter


def _encounter() -> tuple[GameState, list[int]]:
    sc = Scenario()
    sc.resources(0, 5)
    sc.deck(0, VANILLA, "GD04-084", LAUNCHER, "GD04-109", VANILLA, BEGUIR, AGRISSA)
    enc = sc.add(0, "GD04-105", Zone.HAND)
    st = sc.start()
    top5 = list(st.zones[0][Zone.DECK][:5])
    play(st, enc)
    return st, top5


@pytest.mark.card("GD04-105")
def test_gd04_105_adds_a_pilot_and_bottoms_the_rest() -> None:
    st, top5 = _encounter()
    yes(st)
    pilot = top5[1]
    assert zone_of(st, pilot) is Zone.HAND
    deck = st.zones[0][Zone.DECK]
    assert card_numbers(st, deck[:2]) == [BEGUIR, AGRISSA]
    assert set(deck[-4:]) == {u for u in top5 if u != pilot}


@pytest.mark.card("GD04-105")
def test_gd04_105_adding_is_optional() -> None:
    st, top5 = _encounter()
    no(st)
    assert set(st.zones[0][Zone.DECK][-5:]) == set(top5)


# ---------------------------------------------------------------------------------------------
# GD04-106 Indiscriminate Violence


def _violence(ex: int) -> tuple[GameState, int, int, int, int]:
    sc = Scenario()
    sc.resources(0, 5 - ex, ex=ex)
    g1 = sc.add(0, GUEL_DILANZA)
    g2 = sc.add(0, BEGUIR)
    ap5 = sc.add(1, AGRISSA)
    ap6 = sc.add(1, "GD04-049")
    iv = sc.add(0, "GD04-106", Zone.HAND)
    st = sc.start()
    play(st, iv, ex=ex)
    return st, g1, g2, ap5, ap6


@pytest.mark.card("GD04-106")
@pytest.mark.rule("8-2-1")
def test_gd04_106_chosen_unit_may_attack_active_enemy_with_5_or_less_ap() -> None:
    st, g1, g2, ap5, ap6 = _violence(ex=0)
    select(st, g1)
    assert has_action(st, A.ATTACK, g1, ap5)
    assert not has_action(st, A.ATTACK, g1, ap6)
    assert not has_action(st, A.ATTACK, g2, ap5)


@pytest.mark.card("GD04-106")
def test_gd04_106_with_ex_resource_choose_up_to_2_units() -> None:
    st, g1, g2, ap5, _ = _violence(ex=1)
    select(st, g1, done=False)
    select(st, g2)
    assert has_action(st, A.ATTACK, g1, ap5)
    assert has_action(st, A.ATTACK, g2, ap5)


@pytest.mark.card("GD04-106")
def test_gd04_106_with_ex_resource_second_unit_is_optional() -> None:
    st, g1, g2, ap5, _ = _violence(ex=1)
    select(st, g1, done=False)
    act(st, A.DONE)
    assert has_action(st, A.ATTACK, g1, ap5)
    assert not has_action(st, A.ATTACK, g2, ap5)


@pytest.mark.card("GD04-106")
@pytest.mark.rule("10-1-8-1-1")
def test_gd04_106_needs_a_friendly_academy_unit() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    sc.add(0, VANILLA)
    iv = sc.add(0, "GD04-106", Zone.HAND)
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, iv)


# ---------------------------------------------------------------------------------------------
# GD04-107 Destined Battle


def _destined(
    *, first_target: str = "player", my_r: str = VANILLA, shield: str = VANILLA, copies: int = 1
) -> tuple[GameState, dict[str, int]]:
    sc = Scenario(active=1)
    sc.resources(0, 2)
    sc.resources(1, 5)
    a = sc.add(1, AGRISSA)
    b = sc.add(1, BEGUIR)
    r = sc.add(0, my_r, rested=True)
    s = sc.add(0, VANILLA, rested=True)
    sc.shields(0, shield, VANILLA)
    cards = [sc.add(0, "GD04-107", Zone.HAND) for _ in range(copies)]
    iv = sc.add(1, "GD04-106", Zone.HAND)
    st = sc.start()
    ids = {"a": a, "b": b, "r": r, "s": s, "iv": iv}
    for i, c in enumerate(cards):
        ids[f"db{i}"] = c
    target = PLAYER_TARGET if first_target == "player" else ids[first_target]
    ids["a_target"] = target
    return st, ids


def _play_destined(st: GameState, db: int, chosen: int) -> None:
    assert st.pending is not None and st.pending.player == 0
    assert st.pending.kind is DecisionKind.ACTION_STEP
    play(st, db)
    select(st, chosen)


def _attack_targets(st: GameState, attacker: int) -> set[int]:
    return {o.b for o in options(st) if o.kind is A.ATTACK and o.a == attacker}


@pytest.mark.card("GD04-107")
@pytest.mark.ruling("GD04-107:Q298")
@pytest.mark.rule("8-2-1")
def test_gd04_107_q298_later_attacks_must_target_chosen_unit_current_one_unchanged() -> None:
    st, u = _destined()
    top_shield = st.zones[0][Zone.SHIELD][0]
    attack(st, u["a"])
    _play_destined(st, u["db0"], u["r"])
    pass_all(st)
    assert zone_of(st, top_shield) is Zone.TRASH
    assert st.cards[u["r"]].damage == 0
    assert _attack_targets(st, u["b"]) == {u["r"]}


@pytest.mark.card("GD04-107")
@pytest.mark.ruling("GD04-107:Q290")
def test_gd04_107_q290_several_chosen_units_attacker_picks_one() -> None:
    st, u = _destined(copies=2)
    attack(st, u["a"])
    _play_destined(st, u["db0"], u["r"])
    _play_destined(st, u["db1"], u["s"])
    pass_all(st)
    assert _attack_targets(st, u["b"]) == {u["r"], u["s"]}


@pytest.mark.card("GD04-107")
@pytest.mark.ruling("GD04-107:Q299")
def test_gd04_107_q299_chosen_unit_gone_frees_attackers() -> None:
    st, u = _destined(first_target="r")
    attack(st, u["a"], u["r"])
    _play_destined(st, u["db0"], u["r"])
    pass_all(st)
    assert zone_of(st, u["r"]) is Zone.TRASH
    assert _attack_targets(st, u["b"]) == {PLAYER_TARGET, u["s"]}


@pytest.mark.card("GD04-107")
@pytest.mark.ruling("GD04-107:Q300")
def test_gd04_107_q300_active_chosen_unit_frees_attackers() -> None:
    st, u = _destined(my_r=LAUNCHER, shield="GD01-121")
    attack(st, u["a"])
    _play_destined(st, u["db0"], u["r"])
    pass_all(st)
    yes(st)  # my Midair Modifications 【Burst】 sets the chosen Launcher active
    assert not st.cards[u["r"]].rested
    assert _attack_targets(st, u["b"]) == {PLAYER_TARGET, u["s"]}


@pytest.mark.card("GD04-107")
@pytest.mark.ruling("GD04-107:Q301")
def test_gd04_107_q301_active_chosen_unit_binds_attackers_allowed_to_hit_active() -> None:
    st, u = _destined(my_r=LAUNCHER, shield="GD01-121")
    play(st, u["iv"])  # the opponent's Beguir-Pente may attack active Units with 5 or less AP
    attack(st, u["a"])
    _play_destined(st, u["db0"], u["r"])
    pass_all(st)
    yes(st)
    assert not st.cards[u["r"]].rested
    assert _attack_targets(st, u["b"]) == {u["r"]}


@pytest.mark.card("GD04-107")
@pytest.mark.ruling("GD04-107:Q289")
def test_gd04_107_q289_outranks_if_possible_attractors() -> None:
    sc = Scenario(active=1)
    sc.resources(0, 2)
    a = sc.add(1, LAUNCHER)
    b = sc.add(1, BEGUIR)
    r = sc.add(0, VANILLA, rested=True)
    age2 = sc.add(0, "GD03-019", rested=True, pilot=RIDDHE_OLD)
    sc.shields(0, VANILLA, VANILLA)
    db = sc.add(0, "GD04-107", Zone.HAND)
    st = sc.start()
    assert _attack_targets(st, a) == {age2}
    attack(st, a, age2)
    _play_destined(st, db, r)
    pass_all(st)
    assert _attack_targets(st, b) == {r}


@pytest.mark.card("GD04-107")
def test_gd04_107_burst_adds_to_hand() -> None:
    sc = Scenario(active=1)
    enemy = sc.add(1, VANILLA)
    (shield,) = sc.shields(0, "GD04-107")
    st = sc.start()
    attack(st, enemy)
    pass_all(st)
    yes(st)
    assert zone_of(st, shield) is Zone.HAND


# ---------------------------------------------------------------------------------------------
# GD04-108 Witches from Earth


def _witches(ex: int) -> tuple[GameState, int, int]:
    sc = Scenario()
    sc.resources(0, 4 - ex, ex=ex)
    beguir = sc.add(0, BEGUIR)
    enemy = sc.add(1, AGRISSA, rested=True)
    witches = sc.add(0, "GD04-108", Zone.HAND)
    st = sc.start()
    play(st, witches, ex=ex)
    attack(st, beguir, enemy)
    pass_all(st)
    return st, beguir, enemy


@pytest.mark.card("GD04-108")
@pytest.mark.ruling("GD04-108:Q291")
@pytest.mark.rule("5-21-1")
def test_gd04_108_q291_next_damage_reduced_by_2() -> None:
    st, beguir, _ = _witches(ex=0)
    assert zone_of(st, beguir) is Zone.TRASH  # 5 - 2 = 3 damage on a 3-HP Unit


@pytest.mark.card("GD04-108")
def test_gd04_108_with_ex_resource_reduced_by_4() -> None:
    st, beguir, _ = _witches(ex=1)
    assert zone_of(st, beguir) is Zone.BATTLE
    assert st.cards[beguir].damage == 1


@pytest.mark.card("GD04-108", "GD04-114")
def test_gd04_108_second_copy_reduces_a_later_damage() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    beguir = sc.add(0, BEGUIR)
    sc.add(1, VANILLA)
    w1, w2 = sc.hand(0, "GD04-108", "GD04-108")
    r1, r2 = sc.hand(0, "GD04-114", "GD04-114")
    st = sc.start()
    play(st, w1)
    play(st, r1)
    assert st.cards[beguir].damage == 0
    play(st, w2)
    play(st, r2)
    assert st.cards[beguir].damage == 0


# ---------------------------------------------------------------------------------------------
# GD04-109 Overwhelming Pressure


@pytest.mark.card("GD04-109")
def test_gd04_109_deals_4_to_enemy_lv6_or_lower() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    enemy = sc.add(1, LAUNCHER)
    sc.add(1, "GD04-049")
    op = sc.add(0, "GD04-109", Zone.HAND)
    st = sc.start()
    play(st, op)
    assert zone_of(st, enemy) is Zone.TRASH


@pytest.mark.card("GD04-109")
@pytest.mark.rule("10-1-8-1-1")
def test_gd04_109_not_playable_without_a_legal_target() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    sc.add(1, "GD04-049")
    op = sc.add(0, "GD04-109", Zone.HAND)
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, op)


# ---------------------------------------------------------------------------------------------
# GD04-110 Financier


@pytest.mark.card("GD04-110")
@pytest.mark.rule("5-17-3-1-1")
def test_gd04_110_deploys_an_ex_base() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    fin = sc.add(0, "GD04-110", Zone.HAND)
    st = sc.start()
    play(st, fin)
    (base,) = st.zones[0][Zone.BASE]
    assert V.cdef(st, base).card_number.startswith("EXB")
    assert st.cards[base].damage == 0


@pytest.mark.card("GD04-110")
@pytest.mark.rule("11-5-2", "11-5-2-1")
def test_gd04_110_replaces_the_existing_base() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    jaburo = sc.base(0, "GD04-122", damage=4)
    fin = sc.add(0, "GD04-110", Zone.HAND)
    st = sc.start()
    play(st, fin)
    assert zone_of(st, jaburo) is Zone.TRASH
    (base,) = st.zones[0][Zone.BASE]
    assert V.cdef(st, base).card_number.startswith("EXB")


# ---------------------------------------------------------------------------------------------
# GD04-111 Trinity


@pytest.mark.card("GD04-111")
def test_gd04_111_up_to_3_cb_units_get_ap_plus_2() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    zero = sc.add(0, ZERO_GUNDAM)
    exia = sc.add(0, EXIA)
    other = sc.add(0, VANILLA)
    trinity = sc.add(0, "GD04-111", Zone.HAND)
    st = sc.start()
    play(st, trinity)
    assert {o.a for o in options(st) if o.kind is A.SELECT} == {zero, exia}
    select(st, zero, exia)
    assert (ap(st, zero), ap(st, exia), ap(st, other)) == (4, 4, 2)


# ---------------------------------------------------------------------------------------------
# GD04-112 Inspector


@pytest.mark.card("GD04-112")
@pytest.mark.rule("5-17-2-4")
def test_gd04_112_deals_1_to_every_unit_lv2_or_lower() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    mine = sc.add(0, VANILLA)
    enemy_low = sc.add(1, BEGUIR)
    enemy_high = sc.add(1, LAUNCHER)
    parts = sc.add(1, PARTS)
    inspector = sc.add(0, "GD04-112", Zone.HAND)
    st = sc.start()
    play(st, inspector)
    assert st.cards[mine].damage == 1
    assert st.cards[enemy_low].damage == 1
    assert st.cards[enemy_high].damage == 0
    assert zone_of(st, parts) is Zone.OUTSIDE


# ---------------------------------------------------------------------------------------------
# GD04-113 Damage Control


@pytest.mark.card("GD04-113")
@pytest.mark.ruling("GD04-113:Q292")
@pytest.mark.rule("5-21-1", "13-2-4-1")
def test_gd04_113_q292_battle_damage_reduced_by_3_this_battle() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    beguir = sc.add(0, BEGUIR)
    enemy = sc.add(1, AGRISSA, rested=True)
    dc = sc.add(0, "GD04-113", Zone.HAND)
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, dc)
    attack(st, beguir, enemy)
    play(st, dc)
    pass_all(st)
    assert zone_of(st, beguir) is Zone.BATTLE
    assert st.cards[beguir].damage == 2


@pytest.mark.card("GD04-113")
@pytest.mark.rule("8-6-1")
def test_gd04_113_outside_a_battle_has_no_lasting_effect() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    mine = sc.add(0, LAUNCHER, rested=True)
    enemy = sc.add(1, AGRISSA)
    dc = sc.add(0, "GD04-113", Zone.HAND)
    sc.shields(0, VANILLA)
    st = sc.start()
    act(st, A.END_MAIN)
    assert st.pending is not None and st.pending.player == 0
    play(st, dc)  # both players then pass and the opponent's turn begins
    assert st.active == 1
    attack(st, enemy, mine)
    pass_all(st)
    assert zone_of(st, mine) is Zone.TRASH


# ---------------------------------------------------------------------------------------------
# GD04-114 Reformationist


@pytest.mark.card("GD04-114")
def test_gd04_114_damages_one_friendly_and_one_enemy_unit() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    mine = sc.add(0, LAUNCHER)
    enemy = sc.add(1, LAUNCHER)
    ref = sc.add(0, "GD04-114", Zone.HAND)
    st = sc.start()
    play(st, ref)
    assert st.cards[mine].damage == 1
    assert st.cards[enemy].damage == 1


@pytest.mark.card("GD04-114")
@pytest.mark.ruling("GD04-114:Q293")
@pytest.mark.rule("10-1-8-1-1")
def test_gd04_114_q293_not_playable_without_an_enemy_unit() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    sc.add(0, LAUNCHER)
    ref = sc.add(0, "GD04-114", Zone.HAND)
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, ref)


@pytest.mark.card("GD04-114")
@pytest.mark.rule("13-2-5-1")
def test_gd04_114_burst_returns_trans_am_unit_from_trash() -> None:
    sc = Scenario(active=1)
    attacker = sc.add(1, VANILLA)
    trans_am, other = sc.trash(0, "GD04-019", LAUNCHER)
    sc.shields(0, "GD04-114")
    st = sc.start()
    attack(st, attacker)
    pass_all(st)
    yes(st)
    assert zone_of(st, trans_am) is Zone.HAND
    assert zone_of(st, other) is Zone.TRASH


# ---------------------------------------------------------------------------------------------
# GD04-115 Backup


def _backup(attacker: str, target: str) -> tuple[GameState, int, int]:
    sc = Scenario()
    sc.resources(0, 3)
    unit = sc.add(0, attacker)
    enemy = sc.add(1, target, rested=True)
    backup = sc.add(0, "GD04-115", Zone.HAND)
    st = sc.start()
    play(st, backup)
    attack(st, unit, enemy)
    pass_all(st)
    return st, unit, enemy


@pytest.mark.card("GD04-115")
def test_gd04_115_battle_damage_to_lv5_or_lower_enemy_destroys_it() -> None:
    st, _, enemy = _backup(VANILLA, LAUNCHER)
    assert zone_of(st, enemy) is Zone.TRASH


@pytest.mark.card("GD04-115")
def test_gd04_115_lv6_enemy_survives() -> None:
    st, _, enemy = _backup(VANILLA, GN_ARMOR_E)
    assert zone_of(st, enemy) is Zone.BATTLE
    assert st.cards[enemy].damage == 2


@pytest.mark.card("GD04-115")
@pytest.mark.ruling("GD04-115:Q294")
@pytest.mark.rule("5-5-5")
def test_gd04_115_q294_zero_ap_unit_destroys_nothing() -> None:
    st, _, enemy = _backup(SNIPER, LAUNCHER)
    assert zone_of(st, enemy) is Zone.BATTLE
    assert st.cards[enemy].damage == 0


@pytest.mark.card("GD04-115")
def test_gd04_115_lasts_only_this_turn() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    unit = sc.add(0, VANILLA, rested=True)
    enemy = sc.add(1, LAUNCHER)
    sc.shields(0, VANILLA)
    backup = sc.add(0, "GD04-115", Zone.HAND)
    st = sc.start()
    play(st, backup)
    to_next_turn(st)
    attack(st, enemy, unit)
    pass_all(st)
    assert zone_of(st, enemy) is Zone.BATTLE
    assert st.cards[enemy].damage == 2


@pytest.mark.card("GD04-115")
@pytest.mark.rule("13-2-5-1")
def test_gd04_115_burst_deals_1_to_an_enemy_unit() -> None:
    sc = Scenario(active=1)
    attacker = sc.add(1, LAUNCHER)
    sc.shields(0, "GD04-115")
    st = sc.start()
    attack(st, attacker)
    pass_all(st)
    yes(st)
    assert st.cards[attacker].damage == 1


# ---------------------------------------------------------------------------------------------
# GD04-116 Reliable Big Brother


def _big_brother(*deck: str) -> tuple[GameState, int, int]:
    sc = Scenario()
    sc.resources(0, 4)
    sc.deck(0, *deck)
    small = sc.add(1, LAUNCHER)
    big = sc.add(1, AGRISSA)
    rbb = sc.add(0, "GD04-116", Zone.HAND)
    st = sc.start()
    play(st, rbb)
    return st, small, big


@pytest.mark.card("GD04-116")
@pytest.mark.rule("5-20-1")
def test_gd04_116_damage_equals_milled_minerva_squad_cards() -> None:
    st, small, big = _big_brother(ZAKU_WARRIOR, "GD04-116")
    assert card_numbers(st, st.zones[0][Zone.TRASH]) == [ZAKU_WARRIOR, "GD04-116", "GD04-116"]
    assert st.cards[small].damage == 2
    assert st.cards[big].damage == 0


@pytest.mark.card("GD04-116")
def test_gd04_116_non_minerva_cards_do_not_count() -> None:
    st, small, _ = _big_brother(ZAKU_WARRIOR, VANILLA)
    assert st.cards[small].damage == 1


@pytest.mark.card("GD04-116")
@pytest.mark.rule("10-1-8-1-2")
def test_gd04_116_playable_without_a_target_and_still_mills() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    sc.deck(0, ZAKU_WARRIOR, VANILLA)
    sc.add(1, AGRISSA)
    rbb = sc.add(0, "GD04-116", Zone.HAND)
    st = sc.start()
    play(st, rbb)
    assert card_numbers(st, st.zones[0][Zone.TRASH]) == [ZAKU_WARRIOR, VANILLA, "GD04-116"]


# ---------------------------------------------------------------------------------------------
# GD04-117 Graceful Demeanor


@pytest.mark.card("GD04-117")
@pytest.mark.rule("13-2-4-1")
def test_gd04_117_action_returns_1_to_2_enemy_units_lv3_or_lower() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    attacker = sc.add(0, VANILLA)
    low1 = sc.add(1, VANILLA)
    low2 = sc.add(1, LUNA_ZAKU)
    high = sc.add(1, LAUNCHER)
    sc.shields(1, VANILLA)
    gd = sc.add(0, "GD04-117", Zone.HAND)
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, gd)
    attack(st, attacker)
    block(st, None)
    play(st, gd)
    select(st, low1, low2)
    assert zone_of(st, low1) is Zone.HAND
    assert zone_of(st, low2) is Zone.HAND
    assert zone_of(st, high) is Zone.BATTLE


@pytest.mark.card("GD04-117")
@pytest.mark.rule("13-2-5-1")
def test_gd04_117_burst_activates_the_action_effect() -> None:
    sc = Scenario(active=1)
    attacker = sc.add(1, VANILLA)
    sc.shields(0, "GD04-117")
    st = sc.start()
    attack(st, attacker)
    pass_all(st)
    yes(st)
    assert zone_of(st, attacker) is Zone.HAND


# ---------------------------------------------------------------------------------------------
# GD04-118 World Distortion


def _world_distortion(*mine: str) -> tuple[GameState, int, int]:
    sc = Scenario()
    sc.resources(0, 3)
    for n in mine:
        sc.add(0, n)
    small = sc.add(1, AGRISSA)
    big = sc.add(1, "GD04-049")
    wd = sc.add(0, "GD04-118", Zone.HAND)
    st = sc.start()
    play(st, wd)
    return st, small, big


@pytest.mark.card("GD04-118")
def test_gd04_118_two_un_units_return_enemy_with_5_or_less_hp() -> None:
    st, small, big = _world_distortion("GD04-075", "GD04-080")
    assert zone_of(st, small) is Zone.HAND
    assert zone_of(st, big) is Zone.BATTLE


@pytest.mark.card("GD04-118")
def test_gd04_118_one_un_unit_does_nothing() -> None:
    st, small, _ = _world_distortion("GD04-080", AGRISSA)
    assert zone_of(st, small) is Zone.BATTLE


# ---------------------------------------------------------------------------------------------
# GD04-119 Fighting Alone


def _fighting_alone(protect: bool) -> tuple[GameState, int]:
    sc = Scenario()
    sc.resources(0, 4)
    attacker = sc.add(0, LAUNCHER)
    protected = sc.add(0, VANILLA, pilot="GD04-081")
    enemy = sc.add(1, SNIPER, rested=True, pilot="GD04-091")
    fa = sc.add(0, "GD04-119", Zone.HAND)
    st = sc.start()
    if protect:
        play(st, fa)
    attack(st, attacker, enemy)
    pass_all(st)
    assert zone_of(st, enemy) is Zone.TRASH
    return st, protected


@pytest.mark.card("GD04-119")
def test_gd04_119_no_effect_damage_from_enemy_units() -> None:
    st, protected = _fighting_alone(protect=True)
    assert st.cards[protected].damage == 0


@pytest.mark.card("GD04-119")
def test_gd04_119_unprotected_unit_takes_the_damage() -> None:
    st, protected = _fighting_alone(protect=False)
    assert st.cards[protected].damage == 1


@pytest.mark.card("GD04-119")
def test_gd04_119_enemy_command_damage_still_applies() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    protected = sc.add(0, VANILLA, pilot="GD04-081")
    sc.shields(1, "ST05-014")
    fa = sc.add(0, "GD04-119", Zone.HAND)
    st = sc.start()
    play(st, fa)
    attack(st, protected)
    pass_all(st)
    yes(st)  # the opponent's Fatal Strike 【Burst】: deal 1 damage to an enemy Unit
    assert st.cards[protected].damage == 1


@pytest.mark.card("GD04-119")
@pytest.mark.rule("10-1-8-1-1")
def test_gd04_119_needs_a_unit_paired_with_a_newtype_pilot() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    sc.add(0, VANILLA, pilot=RIDDHE_OLD)
    fa = sc.add(0, "GD04-119", Zone.HAND)
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, fa)


# ---------------------------------------------------------------------------------------------
# GD04-120 Machine Doll Squad


@pytest.mark.card("GD04-120")
def test_gd04_120_militia_unit_gets_ap_plus_2() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    militia = sc.add(0, "GD04-078")
    other = sc.add(0, VANILLA)
    mds = sc.add(0, "GD04-120", Zone.HAND)
    st = sc.start()
    play(st, mds)
    assert ap(st, militia) == 4
    assert ap(st, other) == 2


# ---------------------------------------------------------------------------------------------
# GD04-121 Reineforce Jr.


def _reineforce(unit: str) -> tuple[GameState, int]:
    sc = Scenario()
    sc.resources(0, 3)
    sc.add(0, unit)
    top, _ = sc.shields(0, VANILLA, VANILLA)
    base = sc.add(0, "GD04-121", Zone.HAND)
    st = sc.start()
    play(st, base)
    assert zone_of(st, top) is Zone.HAND
    return st, base


@pytest.mark.card("GD04-121")
@pytest.mark.rule("5-20-2")
def test_gd04_121_deploy_with_league_militaire_unit_makes_parts() -> None:
    st, _ = _reineforce(SHOKEW)
    assert len(tokens(st, 0, PARTS)) == 1


@pytest.mark.card("GD04-121")
def test_gd04_121_no_league_militaire_unit_no_token() -> None:
    st, _ = _reineforce(VANILLA)
    assert tokens(st, 0, PARTS) == []


@pytest.mark.card("GD04-121")
@pytest.mark.rule("13-2-5-1")
def test_gd04_121_burst_deploy_on_opponents_turn_makes_no_token() -> None:
    sc = Scenario(active=1)
    attacker = sc.add(1, VANILLA)
    sc.add(0, SHOKEW)
    shield, second = sc.shields(0, "GD04-121", VANILLA)
    st = sc.start()
    attack(st, attacker)
    pass_all(st)
    yes(st)
    assert zone_of(st, shield) is Zone.BASE
    assert zone_of(st, second) is Zone.HAND
    assert tokens(st, 0, PARTS) == []


# ---------------------------------------------------------------------------------------------
# GD04-122 Jaburo


@pytest.mark.card("GD04-122")
def test_gd04_122_deploy_adds_a_shield_to_hand() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    (top,) = sc.shields(0, VANILLA)
    jaburo = sc.add(0, "GD04-122", Zone.HAND)
    st = sc.start()
    play(st, jaburo)
    assert zone_of(st, jaburo) is Zone.BASE
    assert zone_of(st, top) is Zone.HAND


@pytest.mark.card("GD04-122")
@pytest.mark.rule("10-1-7-2")
def test_gd04_122_rest_federation_unit_to_rest_enemy_lv3_or_lower_once_per_turn() -> None:
    sc = Scenario()
    jaburo = sc.base(0, "GD04-122")
    loto = sc.add(0, "GD01-011")
    sc.add(0, "GD02-030")
    low = sc.add(1, VANILLA)
    high = sc.add(1, LAUNCHER)
    st = sc.start()
    activate(st, jaburo)
    select(st, loto)
    assert st.cards[loto].rested
    assert st.cards[low].rested
    assert not st.cards[high].rested
    assert not has_action(st, A.ACTIVATE, jaburo)


@pytest.mark.card("GD04-122")
@pytest.mark.rule("10-2-2")
def test_gd04_122_needs_a_federation_unit_and_a_target() -> None:
    sc = Scenario()
    jaburo = sc.base(0, "GD04-122")
    sc.add(0, VANILLA)
    sc.add(1, VANILLA)
    st = sc.start()
    assert not has_action(st, A.ACTIVATE, jaburo)


# ---------------------------------------------------------------------------------------------
# GD04-123 A Baoa Qu


def _baoa_qu(attacker: str, zeon_rested: bool) -> tuple[GameState, int]:
    sc = Scenario(active=1)
    enemy = sc.add(1, attacker)
    base = sc.base(0, "GD04-123")
    sc.add(0, "GD01-035", rested=zeon_rested)
    sc.shields(0, VANILLA)
    st = sc.start()
    attack(st, enemy)
    pass_all(st)
    return st, base


@pytest.mark.card("GD04-123")
@pytest.mark.rule("8-5-2-4")
def test_gd04_123_no_battle_damage_from_lv4_or_lower_while_zeon_rested() -> None:
    st, base = _baoa_qu(LAUNCHER, zeon_rested=True)
    assert st.cards[base].damage == 0


@pytest.mark.card("GD04-123")
def test_gd04_123_damaged_without_a_rested_zeon_unit() -> None:
    st, base = _baoa_qu(LAUNCHER, zeon_rested=False)
    assert st.cards[base].damage == 3


@pytest.mark.card("GD04-123")
def test_gd04_123_lv5_attacker_still_deals_damage() -> None:
    st, base = _baoa_qu("GD04-071", zeon_rested=True)
    assert st.cards[base].damage == 4


@pytest.mark.card("GD04-123")
@pytest.mark.ruling("GD04-123:Q295")
@pytest.mark.rule("13-1-2-1")
def test_gd04_123_q295_breach_damage_is_received() -> None:
    sc = Scenario(active=1)
    rick_dom = sc.add(1, RICK_DOM)
    base = sc.base(0, "GD04-123")
    victim = sc.add(0, "GD01-035", rested=True)
    sc.add(0, "GD01-035", rested=True)
    sc.shields(0, VANILLA)
    st = sc.start()
    attack(st, rick_dom, victim)
    pass_all(st)
    assert zone_of(st, victim) is Zone.TRASH
    assert st.cards[base].damage == 2


# ---------------------------------------------------------------------------------------------
# GD04-124 9th Tactical Testing Sector


@pytest.mark.card("GD04-124")
def test_gd04_124_placing_ex_resource_gives_academy_unit_ap_plus_2() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    sc.base(0, "GD04-124")
    guel = sc.add(0, GUEL_DILANZA)
    age2 = sc.add(0, "GD03-019")
    asemu = sc.add(0, "GD03-088", Zone.HAND)
    st = sc.start()
    play(st, asemu, onto=age2)
    assert ex_count(st, 0) == 1
    assert ap(st, guel) == 4


@pytest.mark.card("GD04-124")
def test_gd04_124_deploy_adds_a_shield_to_hand() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    (top,) = sc.shields(0, VANILLA)
    base = sc.add(0, "GD04-124", Zone.HAND)
    st = sc.start()
    play(st, base)
    assert zone_of(st, top) is Zone.HAND


# ---------------------------------------------------------------------------------------------
# GD04-125 Trinity Warship


@pytest.mark.card("GD04-125")
@pytest.mark.rule("10-1-7-3")
def test_gd04_125_pay_and_rest_cb_unit_to_deal_1_once_per_turn() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    warship = sc.base(0, "GD04-125")
    cb = sc.add(0, ZERO_GUNDAM)
    sc.add(0, EXIA)
    enemy = sc.add(1, LAUNCHER)
    st = sc.start()
    activate(st, warship)
    select(st, cb)
    assert st.cards[cb].rested
    assert st.cards[enemy].damage == 1
    assert sum(st.cards[u].rested for u in st.zones[0][Zone.RESOURCE_AREA]) == 1
    assert not has_action(st, A.ACTIVATE, warship)


@pytest.mark.card("GD04-125")
def test_gd04_125_needs_an_enemy_lv5_or_lower() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    warship = sc.base(0, "GD04-125")
    sc.add(0, ZERO_GUNDAM)
    sc.add(1, GN_ARMOR_E)
    st = sc.start()
    assert not has_action(st, A.ACTIVATE, warship)


# ---------------------------------------------------------------------------------------------
# GD04-126 Izuma Colony


def _izuma(attacker: str) -> tuple[GameState, int, int]:
    sc = Scenario(active=1)
    enemy = sc.add(1, attacker)
    base = sc.base(0, "GD04-126")
    sc.shields(0, VANILLA)
    st = sc.start()
    attack(st, enemy)
    pass_all(st)
    return st, base, enemy


@pytest.mark.card("GD04-126")
def test_gd04_126_battle_damage_from_3_or_less_ap_unit_deals_1_back() -> None:
    st, base, enemy = _izuma(LAUNCHER)
    assert st.cards[base].damage == 3
    assert st.cards[enemy].damage == 1


@pytest.mark.card("GD04-126")
def test_gd04_126_attacker_with_4_ap_is_not_damaged() -> None:
    st, base, enemy = _izuma("GD01-041")
    assert zone_of(st, base) is Zone.TRASH
    assert st.cards[enemy].damage == 0


@pytest.mark.card("GD04-126")
@pytest.mark.ruling("GD04-126:Q296")
@pytest.mark.rule("13-1-2-1")
def test_gd04_126_q296_breach_effect_damage_does_not_trigger() -> None:
    sc = Scenario(active=1)
    rick_dom = sc.add(1, RICK_DOM)
    base = sc.base(0, "GD04-126")
    victim = sc.add(0, SNIPER, rested=True)
    sc.shields(0, VANILLA)
    st = sc.start()
    attack(st, rick_dom, victim)
    pass_all(st)
    assert zone_of(st, victim) is Zone.TRASH
    assert st.cards[base].damage == 2
    assert st.cards[rick_dom].damage == 0


# ---------------------------------------------------------------------------------------------
# GD04-127 Freeden Ⅱ


def _freeden(vulture_in_trash: int) -> tuple[GameState, int, int]:
    sc = Scenario()
    sc.resources(0, 4)
    sc.trash(0, *([WISE_WALLABY] * vulture_in_trash))
    sc.trash(0, VANILLA)
    low = sc.add(1, VANILLA)
    high = sc.add(1, LAUNCHER)
    (top,) = sc.shields(0, VANILLA)
    freeden = sc.add(0, "GD04-127", Zone.HAND)
    st = sc.start()
    play(st, freeden)
    assert zone_of(st, top) is Zone.HAND
    return st, low, high


@pytest.mark.card("GD04-127")
@pytest.mark.rule("5-20-2")
def test_gd04_127_seven_vulture_cards_destroy_enemy_with_2_or_less_ap() -> None:
    st, low, high = _freeden(7)
    assert zone_of(st, low) is Zone.TRASH
    assert zone_of(st, high) is Zone.BATTLE


@pytest.mark.card("GD04-127")
def test_gd04_127_six_vulture_cards_destroy_nothing() -> None:
    st, low, _ = _freeden(6)
    assert zone_of(st, low) is Zone.BATTLE


# ---------------------------------------------------------------------------------------------
# GD04-128 Armory One


@pytest.mark.card("GD04-128")
@pytest.mark.rule("13-2-8-1")
def test_gd04_128_destroyed_all_players_draw_1() -> None:
    sc = Scenario(active=1)
    enemy = sc.add(1, VANILLA)
    base = sc.base(0, "GD04-128", damage=5)
    sc.shields(0, VANILLA)
    st = sc.start()
    before = (len(st.zones[0][Zone.HAND]), len(st.zones[1][Zone.HAND]))
    attack(st, enemy)
    pass_all(st)
    assert zone_of(st, base) is Zone.TRASH
    assert (len(st.zones[0][Zone.HAND]), len(st.zones[1][Zone.HAND])) == (
        before[0] + 1,
        before[1] + 1,
    )


# ---------------------------------------------------------------------------------------------
# GD04-129 Willgem


@pytest.mark.card("GD04-129")
@pytest.mark.rule("5-20-2")
def test_gd04_129_deploy_adds_shield_then_damages_itself() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    (top,) = sc.shields(0, VANILLA)
    willgem = sc.add(0, "GD04-129", Zone.HAND)
    st = sc.start()
    play(st, willgem)
    assert zone_of(st, top) is Zone.HAND
    assert st.cards[willgem].damage == 3


@pytest.mark.card("GD04-129")
@pytest.mark.rule("5-6-1")
def test_gd04_129_paying_for_unit_effect_recovers_2_once_per_turn() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    willgem = sc.base(0, "GD04-129", damage=5)
    turn_a = sc.add(0, "GD04-073")
    kapool = sc.add(0, "GD04-074")
    sc.shields(1, VANILLA)
    st = sc.start()
    activate(st, turn_a)
    assert st.cards[willgem].damage == 3
    attack(st, kapool)
    yes(st)  # Kapool: pay ① to draw 1, then discard 1
    assert st.cards[willgem].damage == 3


@pytest.mark.card("GD04-129")
@pytest.mark.ruling("GD04-129:Q297")
def test_gd04_129_q297_paying_to_deploy_with_x_divider_counts() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    willgem = sc.base(0, "GD04-129", damage=3)
    divider = sc.add(0, "GD03-051")
    (revived,) = sc.trash(0, VANILLA)
    jamil = sc.add(0, "GD03-096", Zone.HAND)
    st = sc.start()
    play(st, jamil, onto=divider)
    select(st, revived)
    assert zone_of(st, revived) is Zone.BATTLE
    assert st.cards[willgem].damage == 1


# ---------------------------------------------------------------------------------------------
# GD04-130 Industrial 7


@pytest.mark.card("GD04-130")
@pytest.mark.rule("5-12")
def test_gd04_130_exile_command_from_trash_gives_enemy_ap_minus_1() -> None:
    sc = Scenario()
    industrial = sc.base(0, "GD04-130")
    (cmd,) = sc.trash(0, "GD04-109")
    enemy = sc.add(1, LAUNCHER)
    st = sc.start()
    activate(st, industrial)
    assert zone_of(st, cmd) is Zone.REMOVAL
    assert ap(st, enemy) == 2
    assert not has_action(st, A.ACTIVATE, industrial)


@pytest.mark.card("GD04-130")
def test_gd04_130_needs_a_command_card_in_trash() -> None:
    sc = Scenario()
    industrial = sc.base(0, "GD04-130")
    sc.trash(0, VANILLA)
    sc.add(1, LAUNCHER)
    st = sc.start()
    assert not has_action(st, A.ACTIVATE, industrial)
