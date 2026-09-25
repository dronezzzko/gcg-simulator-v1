"""Behaviour tests for GD03-084..GD03-132 (package WP-GD03-B)."""

from __future__ import annotations

from typing import Any

import pytest

from gcg_sim.effects import dsl as d
from gcg_sim.engine import core
from gcg_sim.engine.game import ALT_PLAY_BASE
from gcg_sim.engine.state import GameState
from gcg_sim.engine.types import NO_ARG, PLAYER_TARGET, ActionKind, DecisionKind, Zone
from gcg_sim.testkit import (
    Scenario,
    act,
    ap,
    attack,
    has_action,
    hp,
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

ZAKU = "GD01-060"  # Zaku Mariner, vanilla Lv2 2/2 (Zeon); also the deck filler
HIZACK = "GD02-013"  # vanilla Lv2 2/2 (Titans)
MARASAI = "GD02-015"  # vanilla Lv3 3/3 (Titans), links with a (Titans) Pilot
GAPLANT = "GD04-010"  # vanilla Lv4 3/4 (Titans), links with a (Titans) Pilot
THE_O = "GD03-002"  # Lv7 5/5 (Titans)(Jupitris)
BOLINOAK = "GD03-008"  # Lv4 3/3 (Titans)(Jupitris), links with a (Jupitris) Pilot
AGE1 = "GD02-029"  # vanilla Lv3 3/3 (AGE System), links with an (Asuno Family) Pilot
LOTO = "GD01-011"  # vanilla Lv2 2/2 (Earth Federation), links with an (Earth Federation) Pilot
ZGOK = "GD03-027"  # vanilla Lv3 3/3 (Zeon)(Cyclops Team)
DINN_CMD = "GD03-047"  # vanilla Lv4 3/4 (ZAFT), links with Rau Le Creuset
GELGOOG = "GD01-031"  # vanilla Lv4 4/3 (Zeon), links with a (Zeon) Pilot
ZEDAS_M = "GD03-065"  # vanilla Lv3 3/3 (Vagan)
AIRMASTER = "GD02-063"  # vanilla Lv3 3/3 (Vulture), links with a (Vulture) Pilot
HAJIROBOSHI = "GD03-068"  # Lv3 3/3, links with Wistario Afam
UNION_FLAG = "GD04-071"  # Lv5 4/3 (UN), links with Graham Aker
AGRISSA = "GD04-079"  # vanilla Lv5 5/4 (Superpower Bloc)
MK2_AEUG = "GD02-071"  # Lv4 3/4 (AEUG), links with an (AEUG) Pilot
NT1_FA = "ST14-002"  # Gundam NT-1 Full Armor, Lv5 4/4 <Blocker>
MESSER = "ST08-003"  # vanilla Lv4 4/3 (Mafty)
HEAVYARMS_EW = "ST14-008"  # vanilla Lv6 6/4 (G Team)
HYAKUREN = "GD02-062"  # vanilla Lv4 3/4 (Teiwaz)
BARBATOS_3RD = "GD02-068"  # Lv4 3/5 (Tekkadan): 【Deploy】Deal 2 damage to this Unit.
NEMO = "GD02-080"  # vanilla Lv2 2/2 (AEUG)

PAPTIMUS = "GD03-084"
CHRISTINA = "GD03-085"
YAZAN = "GD03-086"
SARAH = "GD03-087"
ASEMU = "GD03-088"
BERNARD = "GD03-089"
MIKHAIL = "GD03-090"
RAU = "GD03-091"
NYAAN = "GD03-092"
CARRIS = "GD03-093"
ZEHEART = "GD03-094"
AZEE = "GD03-095"
JAMIL = "GD03-096"
WISTARIO = "GD03-097"
GRAHAM = "GD03-098"
EMMA = "GD03-099"
SOMA = "GD03-100"
HEALTHY_CURIOSITY = "GD03-101"
PRIVILEGED_POSITION = "GD03-102"
FIELD_DIRECTIVE = "GD03-103"
RECCOAS_SHADOW = "GD03-104"
BRIDGE_CREW = "GD03-105"
MAV_TACTICS = "GD03-106"
OVER_THE_RIVER = "GD03-107"
HOW_MANY_MILES = "GD03-108"
IMPROVED_TECHNIQUE = "GD03-109"
ELIMINATE_TARGET = "GD03-110"
INFILTRATOR_PRESENT = "GD03-111"
WARPED_INTENT = "GD03-112"
HUMAN_KARMA = "GD03-113"
LOOK_OF_DETERMINATION = "GD03-114"
DISTANT_REUNION = "GD03-115"
TOWARDS_DESTINY = "GD03-116"
ORGAS_ORDER = "GD03-117"
AWAKENED_POTENTIAL = "GD03-118"
AWKWARD_APPROACH = "GD03-119"
IMMORTAL_COLASOUR = "GD03-120"
UNHERALDED_ATTACK = "GD03-121"
VETERAN_TACTICS = "GD03-122"
JUPITRIS = "GD03-123"
RIBO_COLONY = "GD03-124"
PEACEMILLION = "GD03-125"
CYCLOPS_TEAM = "GD03-126"
JACHIN_DUE = "GD03-127"
DORITEA = "GD03-128"
HOTARUBI = "GD03-129"
DOWNES = "GD03-130"
ETERNAL = "GD03-131"
RADISH = "GD03-132"


def hand(st: GameState, player: int = 0) -> list[int]:
    return list(st.zones[player][Zone.HAND])


def select_options(st: GameState) -> set[int]:
    return {o.a for o in options(st) if o.kind is A.SELECT}


def pending_kind(st: GameState) -> DecisionKind | None:
    return st.pending.kind if st.pending is not None else None


# ---------------------------------------------------------------------------------------------
# 【Burst】 lines shared by many cards


@pytest.mark.parametrize(
    "number",
    [
        pytest.param(n, marks=pytest.mark.card(n))
        for n in (
            "GD03-084",
            "GD03-085",
            "GD03-086",
            "GD03-087",
            "GD03-088",
            "GD03-089",
            "GD03-090",
            "GD03-091",
            "GD03-092",
            "GD03-093",
            "GD03-094",
            "GD03-095",
            "GD03-096",
            "GD03-097",
            "GD03-098",
            "GD03-099",
            "GD03-100",
            "GD03-105",
            "GD03-112",
            "GD03-118",
        )
    ],
)
@pytest.mark.rule("13-2-5-1")
def test_burst_add_this_card_to_hand(number: str) -> None:
    sc = Scenario()
    attacker = sc.add(0, ZAKU)
    (shield,) = sc.shields(1, number)
    st = sc.start()
    attack(st, attacker)
    pass_all(st)
    assert pending_kind(st) is DecisionKind.BURST
    yes(st)
    assert zone_of(st, shield) is Zone.HAND


@pytest.mark.parametrize(
    "number",
    [
        pytest.param(n, marks=pytest.mark.card(n))
        for n in (
            "GD03-123",
            "GD03-124",
            "GD03-125",
            "GD03-126",
            "GD03-127",
            "GD03-128",
            "GD03-129",
            "GD03-130",
            "GD03-131",
            "GD03-132",
        )
    ],
)
@pytest.mark.rule("13-2-5-1", "13-2-6-1")
def test_burst_deploy_base_then_deploy_adds_shield(number: str) -> None:
    sc = Scenario()
    attacker = sc.add(0, ZAKU)
    base, second = sc.shields(1, number, ZAKU)
    st = sc.start()
    attack(st, attacker)
    pass_all(st)
    yes(st)
    pass_all(st)
    assert zone_of(st, base) is Zone.BASE
    assert zone_of(st, second) is Zone.HAND  # 【Deploy】Add 1 of your Shields to your hand.


# ---------------------------------------------------------------------------------------------
# Pilots


@pytest.mark.card("GD03-084")
@pytest.mark.rule("13-2-11-1", "3-3-9-2")
def test_gd03_084_when_linked_repair_and_draw_for_jupitris() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    host = sc.add(0, MARASAI)
    other = sc.add(0, BOLINOAK, damage=2)
    pilot = sc.add(0, PAPTIMUS, Zone.HAND)
    st = sc.start()
    before = len(hand(st))
    play(st, pilot, onto=host)
    assert keywords(st, other).get("Repair") == 2
    assert "Repair" not in keywords(st, host)  # "other" Units only
    assert len(hand(st)) == before  # the Pilot left the hand, then 1 card was drawn
    to_next_turn(st)
    assert st.cards[other].damage == 0  # <Repair 2> at the end of the turn (13-1-1-1)
    assert "Repair" not in keywords(st, other)  # only during this turn


@pytest.mark.card("GD03-084")
def test_gd03_084_no_draw_when_chosen_unit_is_not_jupitris() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    host = sc.add(0, MARASAI)
    other = sc.add(0, HIZACK)
    pilot = sc.add(0, PAPTIMUS, Zone.HAND)
    st = sc.start()
    before = len(hand(st))
    play(st, pilot, onto=host)
    assert keywords(st, other).get("Repair") == 2
    assert len(hand(st)) == before - 1


@pytest.mark.card("GD03-085")
def test_gd03_085_pairs_for_zero_cost_with_nt1_only() -> None:
    sc = Scenario()
    sc.resources(0, 3, rested=2)
    nt1 = sc.add(0, NT1_FA)
    other = sc.add(0, MARASAI)
    pilot = sc.add(0, CHRISTINA, Zone.HAND)
    st = sc.start()
    assert has_action(st, A.PAIR, pilot, other)  # regular pairing at cost 1
    free_targets = {o.b for o in options(st) if o.kind is A.PAIR and o.c >= ALT_PLAY_BASE}
    assert free_targets == {nt1}
    act(st, A.PAIR, pilot, nt1, ALT_PLAY_BASE)
    assert st.cards[nt1].pair == pilot
    active = [r for r in st.zones[0][Zone.RESOURCE_AREA] if not st.cards[r].rested]
    assert len(active) == 1  # nothing was paid


@pytest.mark.card("GD03-085")
def test_gd03_085_free_pairing_offered_with_no_active_resources() -> None:
    sc = Scenario()
    sc.resources(0, 3, rested=3)
    nt1 = sc.add(0, NT1_FA)
    pilot = sc.add(0, CHRISTINA, Zone.HAND)
    st = sc.start()
    act(st, A.PAIR, pilot, nt1, ALT_PLAY_BASE)
    assert st.cards[nt1].pair == pilot


@pytest.mark.card("GD03-085")
@pytest.mark.ruling("GD03-085:Q238")
@pytest.mark.rule("2-9-1")
def test_gd03_085_level_is_not_reduced() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    nt1 = sc.add(0, NT1_FA)
    pilot = sc.add(0, CHRISTINA, Zone.HAND)
    st = sc.start()
    assert not has_action(st, A.PAIR, pilot, nt1)  # Lv.3 still requires 3 Resources


@pytest.mark.card("GD03-086")
def test_gd03_086_attack_chooses_titans_unit_up_to_own_level() -> None:
    sc = Scenario()
    host = sc.add(0, GAPLANT, pilot=YAZAN)  # Lv.4
    low = sc.add(0, MARASAI)  # Lv.3 (Titans)
    high = sc.add(0, THE_O)  # Lv.7 (Titans)
    sc.add(0, ZAKU)  # Lv.2, not (Titans)
    st = sc.start()
    attack(st, host)
    assert select_options(st) == {host, low}
    assert high not in select_options(st)
    select(st, low)
    assert ap(st, low) == 4


@pytest.mark.card("GD03-086")
@pytest.mark.ruling("GD03-086:Q239")
def test_gd03_086_lv7_unit_can_choose_lv7_titans() -> None:
    sc = Scenario()
    host = sc.add(0, THE_O, pilot=YAZAN)
    other = sc.add(0, THE_O)
    st = sc.start()
    attack(st, host)
    assert select_options(st) == {host, other}
    select(st, other)
    assert ap(st, other) == 6


@pytest.mark.card("GD03-087")
@pytest.mark.rule("13-2-11-1")
def test_gd03_087_when_linked_rests_enemy_lv3_or_lower() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    host = sc.add(0, MARASAI)
    low = sc.add(1, ZAKU)
    high = sc.add(1, GAPLANT)
    pilot = sc.add(0, SARAH, Zone.HAND)
    st = sc.start()
    play(st, pilot, onto=host)
    assert st.cards[low].rested
    assert not st.cards[high].rested


@pytest.mark.card("GD03-087")
def test_gd03_087_no_trigger_when_not_linked() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    host = sc.add(0, ZAKU)  # no link condition
    low = sc.add(1, ZAKU)
    pilot = sc.add(0, SARAH, Zone.HAND)
    st = sc.start()
    play(st, pilot, onto=host)
    assert not st.cards[low].rested


@pytest.mark.card("GD03-088")
@pytest.mark.rule("13-2-12-1")
def test_gd03_088_linked_age_system_gets_ap_and_breach() -> None:
    sc = Scenario()
    unit = sc.add(0, AGE1, pilot=ASEMU)
    st = sc.start()
    assert ap(st, unit) == 3 + 2 + 1
    assert keywords(st, unit).get("Breach") == 1


@pytest.mark.card("GD03-088")
def test_gd03_088_linked_non_age_system_gets_nothing() -> None:
    sc = Scenario()
    unit = sc.add(0, LOTO, pilot=ASEMU)
    st = sc.start()
    assert ap(st, unit) == 2 + 2
    assert "Breach" not in keywords(st, unit)


@pytest.mark.card("GD03-089")
@pytest.mark.ruling("GD03-089:Q240")
def test_gd03_089_ap_counts_unique_cyclops_pilot_and_command_names() -> None:
    sc = Scenario()
    unit = sc.add(0, ZGOK, pilot=BERNARD)
    sc.trash(0, MIKHAIL, MIKHAIL, OVER_THE_RIVER, HOW_MANY_MILES, HOW_MANY_MILES, HOW_MANY_MILES)
    sc.trash(
        0, ZGOK, FIELD_DIRECTIVE
    )  # a (Cyclops Team) Unit card and a non-(Cyclops Team) Command
    st = sc.start()
    assert ap(st, unit) == 3 + 0 + 3


@pytest.mark.card("GD03-089")
def test_gd03_089_empty_trash_gives_no_bonus() -> None:
    sc = Scenario()
    unit = sc.add(0, ZGOK, pilot=BERNARD)
    st = sc.start()
    assert ap(st, unit) == 3


@pytest.mark.card("GD03-090")
def test_gd03_090_attack_grants_breach_to_cyclops_unit() -> None:
    sc = Scenario()
    host = sc.add(0, ZGOK, pilot=MIKHAIL)
    other = sc.add(0, ZGOK)
    sc.add(0, ZAKU)
    st = sc.start()
    attack(st, host)
    assert select_options(st) == {host, other}
    select(st, other)
    assert keywords(st, other).get("Breach") == 1


@pytest.mark.card("GD03-091")
def test_gd03_091_when_linked_adds_zaft_base_from_trash() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    host = sc.add(0, DINN_CMD)
    (zaft_base, other_base) = sc.trash(0, JACHIN_DUE, RADISH)
    pilot = sc.add(0, RAU, Zone.HAND)
    st = sc.start()
    play(st, pilot, onto=host)
    assert zone_of(st, zaft_base) is Zone.HAND
    assert zone_of(st, other_base) is Zone.TRASH


@pytest.mark.card("GD03-092")
def test_gd03_092_milled_zeon_card_deals_1_damage() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    host = sc.add(0, GELGOOG)
    enemy = sc.add(1, MARASAI)
    pilot = sc.add(0, NYAAN, Zone.HAND)
    sc.deck(0, ZAKU)
    st = sc.start()
    top = st.zones[0][Zone.DECK][0]
    play(st, pilot, onto=host)
    assert zone_of(st, top) is Zone.TRASH
    assert st.cards[enemy].damage == 1


@pytest.mark.card("GD03-092")
def test_gd03_092_milled_other_card_deals_no_damage() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    host = sc.add(0, GELGOOG)
    enemy = sc.add(1, MARASAI)
    pilot = sc.add(0, NYAAN, Zone.HAND)
    sc.deck(0, MARASAI)
    st = sc.start()
    top = st.zones[0][Zone.DECK][0]
    play(st, pilot, onto=host)
    assert zone_of(st, top) is Zone.TRASH
    assert st.cards[enemy].damage == 0


@pytest.mark.card("GD03-093")
def test_gd03_093_ap_bonus_while_no_enemy_base() -> None:
    sc = Scenario()
    unit = sc.add(0, ZAKU, pilot=CARRIS)
    st = sc.start()
    assert ap(st, unit) == 2 + 2 + 1


@pytest.mark.card("GD03-093")
@pytest.mark.rule("5-17-3-1-1")
def test_gd03_093_enemy_ex_base_turns_bonus_off() -> None:
    sc = Scenario()
    unit = sc.add(0, ZAKU, pilot=CARRIS)
    sc.base(1)
    st = sc.start()
    assert ap(st, unit) == 2 + 2


@pytest.mark.card("GD03-094")
@pytest.mark.rule("13-2-9-1")
def test_gd03_094_when_paired_mills_2_and_vagan_gives_ap_minus_2() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    host = sc.add(0, ZAKU)
    enemy = sc.add(1, GAPLANT)
    pilot = sc.add(0, ZEHEART, Zone.HAND)
    sc.deck(0, ZAKU, ZEDAS_M)
    st = sc.start()
    top2 = list(st.zones[0][Zone.DECK][:2])
    play(st, pilot, onto=host)
    assert all(zone_of(st, u) is Zone.TRASH for u in top2)
    assert ap(st, enemy) == 1


@pytest.mark.card("GD03-094")
def test_gd03_094_no_vagan_milled_no_ap_change() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    host = sc.add(0, ZAKU)
    enemy = sc.add(1, GAPLANT)
    pilot = sc.add(0, ZEHEART, Zone.HAND)
    sc.deck(0, ZAKU, ZAKU, ZEDAS_M)
    st = sc.start()
    play(st, pilot, onto=host)
    assert ap(st, enemy) == 3


@pytest.mark.card("GD03-095")
@pytest.mark.rule("13-2-13-1")
def test_gd03_095_effect_damage_reduces_enemy_ap_once_per_turn() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    host = sc.add(0, ZEDAS_M, pilot=AZEE)  # 4/5
    enemy = sc.add(1, AGRISSA)  # 5/4
    first, second = sc.hand(0, TOWARDS_DESTINY, TOWARDS_DESTINY)
    st = sc.start()
    play(st, first)
    assert st.cards[host].damage == 2
    assert ap(st, enemy) == 4
    play(st, second)
    assert st.cards[host].damage == 4
    assert pending_kind(st) is DecisionKind.MAIN
    assert zone_of(st, enemy) is Zone.TRASH
    assert ap(st, host) == 4


@pytest.mark.card("GD03-095")
def test_gd03_095_battle_damage_does_not_trigger() -> None:
    sc = Scenario()
    host = sc.add(0, ZEDAS_M, pilot=AZEE)
    target = sc.add(1, ZAKU, rested=True)
    other = sc.add(1, GAPLANT)
    st = sc.start()
    attack(st, host, target)
    pass_all(st)
    assert st.cards[host].damage == 2
    assert ap(st, other) == 3


@pytest.mark.card("GD03-096")
@pytest.mark.rule("5-20-1")
def test_gd03_096_linked_attack_may_discard_to_draw() -> None:
    sc = Scenario()
    host = sc.add(0, AIRMASTER, pilot=JAMIL)
    (card,) = sc.hand(0, ZAKU)
    st = sc.start()
    attack(st, host)
    assert pending_kind(st) is DecisionKind.YES_NO
    yes(st)
    assert zone_of(st, card) is Zone.TRASH
    assert len(hand(st)) == 1


@pytest.mark.card("GD03-096")
def test_gd03_096_declining_does_not_draw() -> None:
    sc = Scenario()
    host = sc.add(0, AIRMASTER, pilot=JAMIL)
    (card,) = sc.hand(0, ZAKU)
    st = sc.start()
    attack(st, host)
    no(st)
    assert hand(st) == [card]


@pytest.mark.card("GD03-096")
def test_gd03_096_not_linked_has_no_attack_effect() -> None:
    sc = Scenario()
    host = sc.add(0, MARASAI, pilot=JAMIL)
    (card,) = sc.hand(0, ZAKU)
    sc.shields(1, ZAKU)
    st = sc.start()
    attack(st, host)
    assert pending_kind(st) is not DecisionKind.YES_NO
    assert hand(st) == [card]


@pytest.mark.card("GD03-097")
def test_gd03_097_battle_kill_keeps_one_on_top_and_trashes_other() -> None:
    sc = Scenario()
    host = sc.add(0, HAJIROBOSHI, pilot=WISTARIO)  # 4/5, linked
    enemy = sc.add(1, ZAKU, rested=True)
    sc.deck(0, MARASAI, HIZACK)
    st = sc.start()
    first, second = st.zones[0][Zone.DECK][:2]
    attack(st, host, enemy)
    pass_all(st)
    assert zone_of(st, enemy) is Zone.TRASH
    assert select_options(st) == {first, second}
    select(st, second)
    assert st.zones[0][Zone.DECK][0] == second
    assert zone_of(st, first) is Zone.TRASH


@pytest.mark.card("GD03-097")
@pytest.mark.ruling("GD03-097:Q241")
@pytest.mark.rule("10-1-6-4")
def test_gd03_097_triggers_when_both_units_are_destroyed() -> None:
    sc = Scenario()
    host = sc.add(0, HAJIROBOSHI, pilot=WISTARIO, damage=3)
    enemy = sc.add(1, ZAKU, rested=True)
    sc.deck(0, MARASAI, HIZACK)
    st = sc.start()
    first, second = st.zones[0][Zone.DECK][:2]
    attack(st, host, enemy)
    pass_all(st)
    assert zone_of(st, host) is Zone.TRASH
    assert zone_of(st, enemy) is Zone.TRASH
    select(st, first)
    assert st.zones[0][Zone.DECK][0] == first
    assert zone_of(st, second) is Zone.TRASH


@pytest.mark.card("GD03-097")
def test_gd03_097_not_linked_does_not_trigger() -> None:
    sc = Scenario()
    host = sc.add(0, MARASAI, pilot=WISTARIO)
    enemy = sc.add(1, ZAKU, rested=True)
    st = sc.start()
    deck_before = list(st.zones[0][Zone.DECK])
    attack(st, host, enemy)
    pass_all(st)
    assert zone_of(st, enemy) is Zone.TRASH
    assert list(st.zones[0][Zone.DECK]) == deck_before


@pytest.mark.card("GD03-098")
def test_gd03_098_set_active_by_effect_returns_enemy_unit() -> None:
    sc = Scenario(active=1)
    sc.resources(1, 7)
    unit = sc.add(0, UNION_FLAG, pilot=GRAHAM, rested=True)  # linked
    dented = sc.add(1, GAPLANT, damage=1)  # 4 HP, 3 current HP (FAQ Q96)
    zeta = sc.add(1, "EB01-005", Zone.HAND)  # 【Deploy】Choose 1 rested Unit ... Set it as active.
    st = sc.start()
    play(st, zeta)
    pass_all(st)
    assert not st.cards[unit].rested
    assert zone_of(st, dented) is Zone.HAND
    assert zone_of(st, zeta) is Zone.BATTLE  # 4 current HP: not a legal choice


@pytest.mark.card("GD03-098")
@pytest.mark.ruling("GD03-098:Q242")
@pytest.mark.rule("7-2-3-1")
def test_gd03_098_start_phase_set_active_does_not_trigger() -> None:
    sc = Scenario()
    unit = sc.add(0, UNION_FLAG, pilot=GRAHAM, rested=True)
    small = sc.add(1, ZAKU)
    st = sc.start()
    to_next_turn(st)
    to_next_turn(st)
    assert st.active == 0
    assert not st.cards[unit].rested
    assert zone_of(st, small) is Zone.BATTLE


@pytest.mark.card("GD03-099")
@pytest.mark.ruling("GD03-099:Q243")
@pytest.mark.rule("13-2-8-1")
def test_gd03_099_destroyed_with_white_base_returns_enemy_up_to_own_lv() -> None:
    sc = Scenario()
    unit = sc.add(0, MK2_AEUG, pilot=EMMA, damage=4)  # Lv.4, 4/5, linked
    sc.base(0, RADISH)
    target = sc.add(1, ZAKU, rested=True)
    low = sc.add(1, GAPLANT)  # Lv.4
    high = sc.add(1, THE_O)  # Lv.7
    st = sc.start()
    attack(st, unit, target)
    pass_all(st)
    assert zone_of(st, unit) is Zone.TRASH
    assert zone_of(st, low) is Zone.HAND
    assert zone_of(st, high) is Zone.BATTLE


@pytest.mark.card("GD03-099")
def test_gd03_099_no_white_base_no_return() -> None:
    sc = Scenario()
    unit = sc.add(0, MK2_AEUG, pilot=EMMA, damage=4)
    sc.base(0)  # EX Base: a token has no color (5-17-2-3)
    target = sc.add(1, ZAKU, rested=True)
    low = sc.add(1, GAPLANT)
    st = sc.start()
    attack(st, unit, target)
    pass_all(st)
    assert zone_of(st, unit) is Zone.TRASH
    assert zone_of(st, low) is Zone.BATTLE


@pytest.mark.card("GD03-100")
@pytest.mark.rule("13-2-8-1", "3-3-9-2")
def test_gd03_100_destroyed_gives_enemy_ap_minus_3() -> None:
    sc = Scenario()
    unit = sc.add(0, MARASAI, pilot=SOMA, damage=3)  # 4/4
    target = sc.add(1, GAPLANT, rested=True)
    other = sc.add(1, THE_O)
    st = sc.start()
    attack(st, unit, target)
    pass_all(st)
    assert zone_of(st, unit) is Zone.TRASH
    assert zone_of(st, target) is Zone.TRASH
    assert ap(st, other) == 2


# ---------------------------------------------------------------------------------------------
# Commands


@pytest.mark.card("GD03-101")
@pytest.mark.rule("5-20-2")
def test_gd03_101_draws_then_rests_with_two_copies_in_trash() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    sc.trash(0, HEALTHY_CURIOSITY, HEALTHY_CURIOSITY)
    dented = sc.add(1, THE_O, damage=1)  # 5 HP, 4 current HP (FAQ Q96)
    small = sc.add(1, GAPLANT)  # 4 HP
    big = sc.add(1, THE_O)  # 5 current HP
    cmd = sc.add(0, HEALTHY_CURIOSITY, Zone.HAND)
    st = sc.start()
    before = len(hand(st))
    play(st, cmd)
    assert len(hand(st)) == before  # the Command left, 1 card drawn
    assert select_options(st) == {dented, small}
    select(st, dented)
    assert st.cards[dented].rested
    assert not st.cards[small].rested
    assert not st.cards[big].rested


@pytest.mark.card("GD03-101")
@pytest.mark.ruling("GD03-101:Q244")
def test_gd03_101_one_copy_in_trash_only_draws() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    sc.trash(0, HEALTHY_CURIOSITY)
    enemy = sc.add(1, ZAKU)
    cmd = sc.add(0, HEALTHY_CURIOSITY, Zone.HAND)
    st = sc.start()
    before = len(hand(st))
    play(st, cmd)
    assert len(hand(st)) == before
    assert not st.cards[enemy].rested
    assert zone_of(st, cmd) is Zone.TRASH


@pytest.mark.card("GD03-102")
@pytest.mark.rule("13-2-4-1")
def test_gd03_102_sets_battling_titans_link_unit_active() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    host = sc.add(0, MARASAI, pilot=SARAH)  # (Titans) Link Unit
    sc.add(0, GAPLANT, rested=True)  # (Titans), not battling
    enemy = sc.add(1, ZAKU, rested=True)
    cmd = sc.add(0, PRIVILEGED_POSITION, Zone.HAND)
    st = sc.start()
    attack(st, host, enemy)
    assert pending_kind(st) is DecisionKind.ACTION_STEP
    assert st.pending is not None and st.pending.player == 0
    play(st, cmd)
    assert not st.cards[host].rested
    pass_all(st)
    assert zone_of(st, enemy) is Zone.TRASH


@pytest.mark.card("GD03-102")
def test_gd03_102_sets_defending_titans_link_unit_active() -> None:
    sc = Scenario(active=1)
    sc.resources(0, 6)
    host = sc.add(0, MARASAI, pilot=SARAH, rested=True)
    attacker = sc.add(1, ZAKU)
    cmd = sc.add(0, PRIVILEGED_POSITION, Zone.HAND)
    st = sc.start()
    attack(st, attacker, host)
    assert pending_kind(st) is DecisionKind.ACTION_STEP
    assert st.pending is not None and st.pending.player == 0
    play(st, cmd)
    assert not st.cards[host].rested
    pass_all(st)
    assert zone_of(st, attacker) is Zone.TRASH


@pytest.mark.card("GD03-102")
def test_gd03_102_cannot_target_unit_attacking_the_player() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    host = sc.add(0, MARASAI, pilot=SARAH)
    cmd = sc.add(0, PRIVILEGED_POSITION, Zone.HAND)
    sc.shields(1, ZAKU)
    st = sc.start()
    attack(st, host, PLAYER_TARGET)
    assert not has_action(st, A.PLAY_COMMAND, cmd)
    assert st.cards[host].rested


@pytest.mark.card("GD03-102")
def test_gd03_102_unlinked_titans_unit_is_not_a_target() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    host = sc.add(0, HIZACK)  # (Titans), no Pilot
    enemy = sc.add(1, GAPLANT, rested=True)
    cmd = sc.add(0, PRIVILEGED_POSITION, Zone.HAND)
    st = sc.start()
    attack(st, host, enemy)
    assert not has_action(st, A.PLAY_COMMAND, cmd)


@pytest.mark.card("GD03-102")
@pytest.mark.rule("13-2-5-1")
def test_gd03_102_burst_draws_1() -> None:
    sc = Scenario()
    attacker = sc.add(0, ZAKU)
    (shield,) = sc.shields(1, PRIVILEGED_POSITION)
    st = sc.start()
    before = len(hand(st, 1))
    attack(st, attacker)
    pass_all(st)
    yes(st)
    assert len(hand(st, 1)) == before + 1
    assert zone_of(st, shield) is Zone.TRASH


@pytest.mark.card("GD03-103")
def test_gd03_103_deals_2_to_rested_enemy_with_three_enemy_units() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    rested = sc.add(1, GAPLANT, rested=True)
    sc.add(1, ZAKU)
    sc.add(1, ZAKU)
    cmd = sc.add(0, FIELD_DIRECTIVE, Zone.HAND)
    st = sc.start()
    play(st, cmd)
    assert st.cards[rested].damage == 2


@pytest.mark.card("GD03-103")
@pytest.mark.ruling("GD03-103:Q245")
@pytest.mark.rule("10-2-1")
def test_gd03_103_cannot_be_played_with_two_enemy_units() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    sc.add(1, GAPLANT, rested=True)
    sc.add(1, ZAKU)
    cmd = sc.add(0, FIELD_DIRECTIVE, Zone.HAND)
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, cmd)


@pytest.mark.card("GD03-103")
@pytest.mark.rule("10-1-8-1-1")
def test_gd03_103_needs_a_rested_enemy_unit() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    for _ in range(3):
        sc.add(1, ZAKU)
    cmd = sc.add(0, FIELD_DIRECTIVE, Zone.HAND)
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, cmd)


@pytest.mark.card("GD03-103")
@pytest.mark.rule("13-2-5-1")
def test_gd03_103_burst_rests_enemy_with_2_or_less_current_hp() -> None:
    sc = Scenario()
    attacker = sc.add(0, ZAKU)
    dented = sc.add(0, MARASAI, damage=1)  # 3 HP, 2 current HP
    healthy = sc.add(0, MARASAI)
    sc.shields(1, FIELD_DIRECTIVE)
    st = sc.start()
    attack(st, attacker)
    pass_all(st)
    yes(st)
    assert select_options(st) == {attacker, dented}  # the attacker is also a 2-HP enemy Unit
    select(st, dented)
    assert st.cards[dented].rested
    assert not st.cards[healthy].rested


@pytest.mark.card("GD03-104")
def test_gd03_104_rests_one_enemy_with_3_or_less_hp() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    a = sc.add(1, ZAKU)
    b = sc.add(1, MARASAI)
    big = sc.add(1, GAPLANT)
    cmd = sc.add(0, RECCOAS_SHADOW, Zone.HAND)
    st = sc.start()
    play(st, cmd)
    assert select_options(st) == {a, b}
    assert not has_action(st, A.DONE)
    select(st, b)
    assert st.cards[b].rested
    assert not st.cards[a].rested
    assert not st.cards[big].rested


@pytest.mark.card("GD03-104")
def test_gd03_104_jupitris_link_unit_allows_two_targets() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    sc.add(0, BOLINOAK, pilot=SARAH)  # (Jupitris) Link Unit
    a = sc.add(1, ZAKU)
    b = sc.add(1, GAPLANT, damage=1)  # 3 current HP
    cmd = sc.add(0, RECCOAS_SHADOW, Zone.HAND)
    st = sc.start()
    play(st, cmd)
    select(st, a, b)
    assert st.cards[a].rested
    assert st.cards[b].rested


@pytest.mark.card("GD03-104")
@pytest.mark.rule("10-1-8-1-1")
def test_gd03_104_unplayable_without_target() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    sc.add(1, GAPLANT)
    cmd = sc.add(0, RECCOAS_SHADOW, Zone.HAND)
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, cmd)


@pytest.mark.card("GD03-105")
def test_gd03_105_may_attack_active_unpaired_enemy_unit() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    unit = sc.add(0, GAPLANT)
    unpaired = sc.add(1, ZAKU)
    paired = sc.add(1, MARASAI, pilot=SOMA)
    cmd = sc.add(0, BRIDGE_CREW, Zone.HAND)
    st = sc.start()
    assert not has_action(st, A.ATTACK, unit, unpaired)
    play(st, cmd)
    assert has_action(st, A.ATTACK, unit, unpaired)
    assert not has_action(st, A.ATTACK, unit, paired)
    attack(st, unit, unpaired)
    pass_all(st)
    assert zone_of(st, unpaired) is Zone.TRASH


@pytest.mark.card("GD03-106")
@pytest.mark.rule("5-17-1")
def test_gd03_106_deploys_two_rested_clan_tokens() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    cmd = sc.add(0, MAV_TACTICS, Zone.HAND)
    st = sc.start()
    play(st, cmd)
    tokens = st.zones[0][Zone.BATTLE]
    assert sorted((ap(st, u), hp(st, u)) for u in tokens) == [(2, 3), (3, 2)]
    assert all(st.cards[u].rested for u in tokens)
    assert {st.cards[u].def_id for u in tokens} == {sc.db["T-018"].def_id, sc.db["T-019"].def_id}


@pytest.mark.card("GD03-107")
@pytest.mark.ruling("GD03-107:Q246")
def test_gd03_107_damage_equals_friendly_unit_tokens() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    for _ in range(3):
        sc.add(0, "T-013")
    sc.add(0, ZGOK)  # not a token
    enemy_token = sc.add(1, "T-013")  # an enemy token does not count
    target = sc.add(1, AGRISSA)  # Lv.5
    sc.add(1, THE_O)  # Lv.7: not choosable
    cmd = sc.add(0, OVER_THE_RIVER, Zone.HAND)
    st = sc.start()
    play(st, cmd)
    assert select_options(st) == {target, enemy_token}
    select(st, target)
    assert st.cards[target].damage == 3


@pytest.mark.card("GD03-108")
def test_gd03_108_deploys_active_hy_gogg_token() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    cmd = sc.add(0, HOW_MANY_MILES, Zone.HAND)
    st = sc.start()
    play(st, cmd)
    (token,) = st.zones[0][Zone.BATTLE]
    assert st.cards[token].def_id == sc.db["T-013"].def_id
    assert not st.cards[token].rested
    assert (ap(st, token), hp(st, token)) == (2, 1)


@pytest.mark.card("GD03-109")
def test_gd03_109_deals_3_to_enemy_lv4_or_lower() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    low = sc.add(1, GAPLANT)
    high = sc.add(1, THE_O)
    cmd = sc.add(0, IMPROVED_TECHNIQUE, Zone.HAND)
    st = sc.start()
    play(st, cmd)
    assert st.cards[low].damage == 3
    assert st.cards[high].damage == 0


@pytest.mark.card("GD03-109")
@pytest.mark.ruling("GD03-109:Q247")
def test_gd03_109_two_copies_in_trash_any_enemy_unit_still_3_damage() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    sc.trash(0, IMPROVED_TECHNIQUE, IMPROVED_TECHNIQUE)
    low = sc.add(1, GAPLANT)
    high = sc.add(1, THE_O)
    cmd = sc.add(0, IMPROVED_TECHNIQUE, Zone.HAND)
    st = sc.start()
    play(st, cmd)
    assert select_options(st) == {low, high}
    select(st, high)
    assert st.cards[high].damage == 3
    assert st.cards[low].damage == 0


@pytest.mark.card("GD03-109")
@pytest.mark.ruling("GD03-109:Q248")
@pytest.mark.rule("10-1-8-1-1")
def test_gd03_109_one_copy_in_trash_cannot_target_high_level() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    sc.trash(0, IMPROVED_TECHNIQUE)
    sc.add(1, THE_O)
    cmd = sc.add(0, IMPROVED_TECHNIQUE, Zone.HAND)
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, cmd)


@pytest.mark.card("GD03-109")
@pytest.mark.rule("13-2-5-1")
def test_gd03_109_burst_activates_main() -> None:
    sc = Scenario()
    attacker = sc.add(0, GAPLANT)
    (shield,) = sc.shields(1, IMPROVED_TECHNIQUE)
    st = sc.start()
    attack(st, attacker)
    pass_all(st)
    yes(st)
    assert st.cards[attacker].damage == 3
    assert zone_of(st, shield) is Zone.TRASH


@pytest.mark.card("GD03-110")
@pytest.mark.ruling("GD03-110:Q426")
def test_gd03_110_targets_pilots_of_enemy_units_lv5_or_lower() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    low = sc.add(1, MARASAI, pilot=YAZAN)  # Unit Lv.3, Pilot Lv.4
    mid = sc.add(1, AGRISSA, pilot=PAPTIMUS)  # Unit Lv.5, Pilot Lv.5
    sc.add(1, THE_O, pilot=SOMA)  # Unit Lv.7, Pilot Lv.3
    sc.add(0, GAPLANT, pilot=SARAH)  # a friendly Pilot is not a target
    cmd = sc.add(0, ELIMINATE_TARGET, Zone.HAND)
    st = sc.start()
    play(st, cmd)
    assert select_options(st) == {st.cards[low].pair, st.cards[mid].pair}


@pytest.mark.card("GD03-110")
@pytest.mark.rule("10-1-8-1-1")
def test_gd03_110_unplayable_without_enemy_pilot() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    sc.add(1, MARASAI)
    sc.add(1, THE_O, pilot=SOMA)
    cmd = sc.add(0, ELIMINATE_TARGET, Zone.HAND)
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, cmd)


@pytest.mark.card("GD03-110")
@pytest.mark.rule("5-10-1")
def test_gd03_110_destroys_the_pilot_and_keeps_the_unit() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    unit = sc.add(1, MARASAI, pilot=YAZAN)
    cmd = sc.add(0, ELIMINATE_TARGET, Zone.HAND)
    st = sc.start()
    pilot = st.cards[unit].pair
    play(st, cmd)
    assert zone_of(st, pilot) is Zone.TRASH
    assert zone_of(st, unit) is Zone.BATTLE
    assert st.cards[unit].pair < 0
    assert ap(st, unit) == 3


@pytest.mark.card("GD03-110")
@pytest.mark.ruling("GD03-110:Q249")
@pytest.mark.rule("3-2-6-3")
def test_gd03_110_link_unit_deployed_this_turn_can_no_longer_attack() -> None:
    sc = Scenario(active=1)
    sc.resources(0, 6)
    fresh = sc.add(1, MARASAI, pilot=YAZAN, deployed_this_turn=True)  # Link Unit
    old = sc.add(1, GAPLANT, pilot=SARAH)  # deployed on a previous turn
    attacker = sc.add(1, ZAKU)
    cmd = sc.add(0, ELIMINATE_TARGET, Zone.HAND)
    sc.shields(0, ZAKU, ZAKU)
    st = sc.start()
    assert has_action(st, A.ATTACK, fresh)
    attack(st, attacker)
    assert pending_kind(st) is DecisionKind.ACTION_STEP
    play(st, cmd)
    select(st, st.cards[fresh].pair)
    pass_all(st)
    assert pending_kind(st) is DecisionKind.MAIN
    assert not has_action(st, A.ATTACK, fresh)
    assert has_action(st, A.ATTACK, old)


@pytest.mark.card("GD03-111")
def test_gd03_111_mafty_unit_gets_ap_plus_3_this_turn() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    unit = sc.add(0, MESSER)
    other = sc.add(0, ZAKU)
    cmd = sc.add(0, INFILTRATOR_PRESENT, Zone.HAND)
    st = sc.start()
    play(st, cmd)
    assert ap(st, unit) == 7
    assert ap(st, other) == 2
    to_next_turn(st)
    assert ap(st, unit) == 4


@pytest.mark.card("GD03-112")
def test_gd03_112_all_paired_units_on_both_sides_get_ap_plus_2() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    mine = sc.add(0, MARASAI, pilot=SOMA)  # 4 AP
    theirs = sc.add(1, GAPLANT, pilot=YAZAN)  # 4 AP
    unpaired = sc.add(0, ZAKU)
    cmd = sc.add(0, WARPED_INTENT, Zone.HAND)
    st = sc.start()
    play(st, cmd)
    assert ap(st, mine) == 6
    assert ap(st, theirs) == 6
    assert ap(st, unpaired) == 2


@pytest.mark.card("GD03-113")
@pytest.mark.ruling("GD03-113:Q250")
@pytest.mark.rule("5-20-1")
def test_gd03_113_rest_friendly_then_3_damage_up_to_its_level() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    mine = sc.add(0, HEAVYARMS_EW)  # Lv.6
    sc.add(0, ZAKU, rested=True)  # rested: not choosable
    same = sc.add(1, HEAVYARMS_EW)  # Lv.6
    high = sc.add(1, THE_O)  # Lv.7
    cmd = sc.add(0, HUMAN_KARMA, Zone.HAND)
    st = sc.start()
    play(st, cmd)
    assert st.cards[mine].rested
    assert st.cards[same].damage == 3
    assert st.cards[high].damage == 0


@pytest.mark.card("GD03-113")
def test_gd03_113_lower_level_rested_unit_limits_targets() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    mine = sc.add(0, ZAKU)  # Lv.2
    enemy = sc.add(1, MARASAI)  # Lv.3
    cmd = sc.add(0, HUMAN_KARMA, Zone.HAND)
    st = sc.start()
    play(st, cmd)
    assert st.cards[mine].rested
    assert st.cards[enemy].damage == 0


@pytest.mark.card("GD03-113")
@pytest.mark.rule("10-1-8-1-1")
def test_gd03_113_unplayable_without_active_friendly_unit() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    sc.add(0, ZAKU, rested=True)
    sc.add(1, ZAKU)
    cmd = sc.add(0, HUMAN_KARMA, Zone.HAND)
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, cmd)


def _battle_action_step(st: GameState, attacker: int) -> None:
    """Attack the opponent (who holds no 【Action】 cards) and reach the attacker's priority."""
    attack(st, attacker)
    assert pending_kind(st) is DecisionKind.ACTION_STEP
    assert st.pending is not None and st.pending.player == 0


def _end_phase_action_step(st: GameState) -> None:
    act(st, A.END_MAIN)
    assert pending_kind(st) is DecisionKind.ACTION_STEP
    assert st.pending is not None and st.pending.player == 0


@pytest.mark.card("GD03-114")
@pytest.mark.rule("13-2-4-1")
def test_gd03_114_destroys_active_enemy_lv2_or_lower() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    low = sc.add(1, ZAKU)
    rested_low = sc.add(1, ZAKU, rested=True)
    mid = sc.add(1, GAPLANT)
    cmd = sc.add(0, LOOK_OF_DETERMINATION, Zone.HAND)
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, cmd)  # 【Action】 only
    _end_phase_action_step(st)
    play(st, cmd)
    assert zone_of(st, low) is Zone.TRASH
    assert zone_of(st, rested_low) is Zone.BATTLE
    assert zone_of(st, mid) is Zone.BATTLE


@pytest.mark.card("GD03-114")
def test_gd03_114_ten_cards_in_trash_allows_lv4() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    sc.trash(0, *([ZAKU] * 10))
    mid = sc.add(1, GAPLANT)
    high = sc.add(1, AGRISSA)
    cmd = sc.add(0, LOOK_OF_DETERMINATION, Zone.HAND)
    st = sc.start()
    _end_phase_action_step(st)
    play(st, cmd)
    assert zone_of(st, mid) is Zone.TRASH
    assert zone_of(st, high) is Zone.BATTLE


@pytest.mark.card("GD03-114")
@pytest.mark.ruling("GD03-114:Q251")
def test_gd03_114_nine_cards_in_trash_cannot_choose_lv4() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    sc.trash(0, *([ZAKU] * 9))
    sc.add(1, GAPLANT)
    cmd = sc.add(0, LOOK_OF_DETERMINATION, Zone.HAND)
    st = sc.start()
    act(st, A.END_MAIN)
    assert not has_action(st, A.PLAY_COMMAND, cmd)


@pytest.mark.card("GD03-114")
@pytest.mark.rule("13-2-5-1")
def test_gd03_114_burst_activates_action() -> None:
    sc = Scenario()
    attacker = sc.add(0, GAPLANT)
    low = sc.add(0, ZAKU)
    sc.shields(1, LOOK_OF_DETERMINATION)
    st = sc.start()
    attack(st, attacker)
    pass_all(st)
    yes(st)
    assert zone_of(st, low) is Zone.TRASH
    assert zone_of(st, attacker) is Zone.BATTLE


@pytest.mark.card("GD03-115")
@pytest.mark.rule("8-6-1")
def test_gd03_115_no_battle_damage_from_2_ap_enemy_during_this_battle() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    unit = sc.add(0, ZEDAS_M, pilot=ZEHEART)  # paired with an (X-Rounder) Pilot, 5/5
    enemy = sc.add(1, ZAKU, rested=True)  # 2 AP
    cmd = sc.add(0, DISTANT_REUNION, Zone.HAND)
    st = sc.start()
    attack(st, unit, enemy)
    play(st, cmd)
    pass_all(st)
    assert st.cards[unit].damage == 0
    assert zone_of(st, enemy) is Zone.TRASH
    assert st.lasting == []  # "during this battle" ended with the battle


@pytest.mark.card("GD03-115")
def test_gd03_115_3_ap_enemy_still_deals_damage_below_lv7() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    unit = sc.add(0, ZEDAS_M, pilot=ZEHEART)
    enemy = sc.add(1, MARASAI, rested=True)  # 3 AP
    cmd = sc.add(0, DISTANT_REUNION, Zone.HAND)
    st = sc.start()
    attack(st, unit, enemy)
    play(st, cmd)
    pass_all(st)
    assert st.cards[unit].damage == 3


@pytest.mark.card("GD03-115")
def test_gd03_115_lv7_protects_from_5_ap_enemy_during_this_battle_only() -> None:
    sc = Scenario()
    sc.resources(0, 7)
    unit = sc.add(0, ZEDAS_M, pilot=ZEHEART)
    enemy = sc.add(1, THE_O, rested=True)  # 5 AP, 5 HP
    cmd = sc.add(0, DISTANT_REUNION, Zone.HAND)
    st = sc.start()
    attack(st, unit, enemy)
    play(st, cmd)
    pass_all(st)
    assert st.cards[unit].damage == 0
    assert zone_of(st, enemy) is Zone.TRASH
    assert st.lasting == []


@pytest.mark.card("GD03-115")
@pytest.mark.rule("10-1-8-1-1")
def test_gd03_115_needs_unit_paired_with_x_rounder() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    unit = sc.add(0, ZEDAS_M, pilot=SOMA)
    enemy = sc.add(1, ZAKU, rested=True)
    cmd = sc.add(0, DISTANT_REUNION, Zone.HAND)
    st = sc.start()
    attack(st, unit, enemy)
    assert not has_action(st, A.PLAY_COMMAND, cmd)


@pytest.mark.card("GD03-115")
@pytest.mark.rule("8-6-1")
def test_gd03_115_played_outside_a_battle_grants_nothing_lasting() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    unit = sc.add(0, ZEDAS_M, pilot=ZEHEART, rested=True)
    enemy = sc.add(1, ZAKU)  # 2 AP
    cmd = sc.add(0, DISTANT_REUNION, Zone.HAND)
    st = sc.start()
    _end_phase_action_step(st)
    play(st, cmd)
    assert st.active == 1 and pending_kind(st) is DecisionKind.MAIN
    attack(st, enemy, unit)
    pass_all(st)
    assert st.cards[unit].damage == 2


@pytest.mark.card("GD03-116")
def test_gd03_116_deals_2_to_friendly_vagan_and_enemy() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    mine = sc.add(0, ZEDAS_M)
    sc.add(0, ZAKU)  # not (Vagan)
    enemy = sc.add(1, GAPLANT)
    cmd = sc.add(0, TOWARDS_DESTINY, Zone.HAND)
    st = sc.start()
    play(st, cmd)
    assert st.cards[mine].damage == 2
    assert st.cards[enemy].damage == 2


@pytest.mark.card("GD03-116")
@pytest.mark.rule("10-1-8-1-1")
def test_gd03_116_unplayable_without_friendly_vagan() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    sc.add(0, ZAKU)
    sc.add(1, GAPLANT)
    cmd = sc.add(0, TOWARDS_DESTINY, Zone.HAND)
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, cmd)


@pytest.mark.card("GD03-117")
def test_gd03_117_one_to_four_enemy_units_deploys_graze_custom() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    sc.add(1, ZAKU)
    sc.add(1, ZAKU)
    cmd = sc.add(0, ORGAS_ORDER, Zone.HAND)
    st = sc.start()
    play(st, cmd)
    (token,) = st.zones[0][Zone.BATTLE]
    assert st.cards[token].def_id == sc.db["T-016"].def_id


@pytest.mark.card("GD03-117")
def test_gd03_117_five_enemy_units_deploys_barbatos() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    for _ in range(5):
        sc.add(1, ZAKU)
    cmd = sc.add(0, ORGAS_ORDER, Zone.HAND)
    st = sc.start()
    play(st, cmd)
    (token,) = st.zones[0][Zone.BATTLE]
    assert st.cards[token].def_id == sc.db["T-017"].def_id
    assert (ap(st, token), hp(st, token)) == (4, 4)


@pytest.mark.card("GD03-117")
@pytest.mark.rule("10-2-1")
def test_gd03_117_unplayable_with_no_enemy_units() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    cmd = sc.add(0, ORGAS_ORDER, Zone.HAND)
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, cmd)


@pytest.mark.card("GD03-118")
def test_gd03_118_returns_rested_enemy_and_may_grant_blocker() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    sc.trash(0, AWAKENED_POTENTIAL, AWAKENED_POTENTIAL)
    attacker = sc.add(0, HIZACK)
    mine = sc.add(0, ZAKU)
    target = sc.add(1, GAPLANT, rested=True)
    active = sc.add(1, ZAKU)
    cmd = sc.add(0, AWAKENED_POTENTIAL, Zone.HAND)
    sc.shields(1, ZAKU)
    st = sc.start()
    _battle_action_step(st, attacker)
    play(st, cmd)
    assert zone_of(st, target) is Zone.HAND
    assert zone_of(st, active) is Zone.BATTLE
    assert select_options(st) == {attacker, mine}
    select(st, mine)
    pass_all(st)
    assert pending_kind(st) is DecisionKind.MAIN
    assert "Blocker" in keywords(st, mine)
    assert "Blocker" not in keywords(st, attacker)


@pytest.mark.card("GD03-118")
@pytest.mark.ruling("GD03-118:Q252")
def test_gd03_118_one_copy_in_trash_no_blocker() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    sc.trash(0, AWAKENED_POTENTIAL)
    attacker = sc.add(0, HIZACK)
    target = sc.add(1, GAPLANT, rested=True)
    cmd = sc.add(0, AWAKENED_POTENTIAL, Zone.HAND)
    sc.shields(1, ZAKU)
    st = sc.start()
    _battle_action_step(st, attacker)
    play(st, cmd)
    assert zone_of(st, target) is Zone.HAND
    assert pending_kind(st) is not DecisionKind.SELECT
    assert "Blocker" not in keywords(st, attacker)


@pytest.mark.card("GD03-119")
@pytest.mark.rule("5-20-1")
def test_gd03_119_sets_rested_base_active_and_enemy_units_lose_ap() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    base = sc.add(0, sc.db.ex_base.card_number, Zone.BASE, rested=True)
    enemy = sc.add(1, GAPLANT)
    mine = sc.add(0, ZAKU)
    cmd = sc.add(0, AWKWARD_APPROACH, Zone.HAND)
    st = sc.start()
    play(st, cmd)
    assert not st.cards[base].rested
    assert ap(st, enemy) == 2
    assert ap(st, mine) == 2


@pytest.mark.card("GD03-119")
@pytest.mark.rule("10-1-8-1-1")
def test_gd03_119_unplayable_without_rested_base() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    sc.base(0)
    sc.add(1, GAPLANT)
    cmd = sc.add(0, AWKWARD_APPROACH, Zone.HAND)
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, cmd)


@pytest.mark.card("GD03-120")
def test_gd03_120_battle_kill_sets_rested_sb_unit_active_but_it_cannot_attack() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    attacker = sc.add(0, AGRISSA)  # (Superpower Bloc) 5/4
    enemy = sc.add(1, ZAKU, rested=True)
    cmd = sc.add(0, IMMORTAL_COLASOUR, Zone.HAND)
    sc.shields(1, ZAKU)
    st = sc.start()
    play(st, cmd)
    attack(st, attacker, enemy)
    pass_all(st)
    assert zone_of(st, enemy) is Zone.TRASH
    assert not st.cards[attacker].rested
    assert not has_action(st, A.ATTACK, attacker)


@pytest.mark.card("GD03-120")
@pytest.mark.rule("10-1-6-1-1")
def test_gd03_120_applies_to_every_battle_kill_this_turn() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    first = sc.add(0, AGRISSA)
    second = sc.add(0, AGRISSA)
    e1 = sc.add(1, ZAKU, rested=True)
    e2 = sc.add(1, ZAKU, rested=True)
    cmd = sc.add(0, IMMORTAL_COLASOUR, Zone.HAND)
    st = sc.start()
    play(st, cmd)
    attack(st, first, e1)
    pass_all(st)
    assert not st.cards[first].rested
    attack(st, second, e2)
    pass_all(st)
    assert zone_of(st, e2) is Zone.TRASH
    assert not st.cards[second].rested
    assert not st.cards[first].rested


@pytest.mark.card("GD03-120")
def test_gd03_120_non_sb_un_kill_does_not_trigger() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    attacker = sc.add(0, GAPLANT)
    idle = sc.add(0, AGRISSA, rested=True)
    enemy = sc.add(1, ZAKU, rested=True)
    cmd = sc.add(0, IMMORTAL_COLASOUR, Zone.HAND)
    st = sc.start()
    play(st, cmd)
    attack(st, attacker, enemy)
    pass_all(st)
    assert zone_of(st, enemy) is Zone.TRASH
    assert st.cards[idle].rested


@pytest.mark.card("GD03-120")
@pytest.mark.rule("7-6-6-1")
def test_gd03_120_ends_with_the_turn() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    cmd = sc.add(0, IMMORTAL_COLASOUR, Zone.HAND)
    st = sc.start()
    play(st, cmd)
    assert st.delayed
    to_next_turn(st)
    assert st.delayed == []


@pytest.mark.card("GD03-121")
@pytest.mark.rule("13-2-4-1")
def test_gd03_121_rests_friendly_base_and_enemy_with_3_or_less_current_hp() -> None:
    sc = Scenario()
    sc.resources(0, 1)
    base = sc.base(0)
    attacker = sc.add(0, HIZACK)
    dented = sc.add(1, GAPLANT, damage=1)  # 3 current HP
    healthy = sc.add(1, AGRISSA)
    cmd = sc.add(0, UNHERALDED_ATTACK, Zone.HAND)
    sc.shields(1, ZAKU)
    st = sc.start()
    _battle_action_step(st, attacker)
    play(st, cmd)
    pass_all(st)
    assert pending_kind(st) is DecisionKind.MAIN
    assert st.cards[base].rested
    assert st.cards[dented].rested
    assert not st.cards[healthy].rested


@pytest.mark.card("GD03-121")
@pytest.mark.ruling("GD03-121:Q253")
def test_gd03_121_unplayable_without_friendly_base() -> None:
    sc = Scenario()
    sc.resources(0, 1)
    sc.add(1, ZAKU)
    cmd = sc.add(0, UNHERALDED_ATTACK, Zone.HAND)
    st = sc.start()
    act(st, A.END_MAIN)
    assert not has_action(st, A.PLAY_COMMAND, cmd)


@pytest.mark.card("GD03-122")
def test_gd03_122_returns_enemy_lv3_or_lower() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    low = sc.add(1, MARASAI)
    high = sc.add(1, GAPLANT)
    cmd = sc.add(0, VETERAN_TACTICS, Zone.HAND)
    st = sc.start()
    _end_phase_action_step(st)
    play(st, cmd)
    assert zone_of(st, low) is Zone.HAND
    assert zone_of(st, high) is Zone.BATTLE


# ---------------------------------------------------------------------------------------------
# Bases


@pytest.mark.card("GD03-123")
@pytest.mark.rule("13-2-6-1")
def test_gd03_123_deploy_adds_shield_then_rests_enemy_with_jupitris_unit() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    sc.add(0, BOLINOAK)  # (Jupitris) Unit
    low = sc.add(1, ZAKU)
    high = sc.add(1, GAPLANT)
    (shield,) = sc.shields(0, ZAKU)
    base = sc.add(0, JUPITRIS, Zone.HAND)
    st = sc.start()
    play(st, base)
    assert zone_of(st, base) is Zone.BASE
    assert zone_of(st, shield) is Zone.HAND
    assert st.cards[low].rested
    assert not st.cards[high].rested


@pytest.mark.card("GD03-123")
@pytest.mark.rule("5-20-2")
def test_gd03_123_then_part_resolves_without_shields() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    sc.add(0, BOLINOAK)
    low = sc.add(1, ZAKU)
    base = sc.add(0, JUPITRIS, Zone.HAND)
    st = sc.start()
    play(st, base)
    assert st.cards[low].rested


@pytest.mark.card("GD03-123")
def test_gd03_123_no_jupitris_unit_no_rest() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    sc.add(0, HIZACK)
    low = sc.add(1, ZAKU)
    (shield,) = sc.shields(0, ZAKU)
    base = sc.add(0, JUPITRIS, Zone.HAND)
    st = sc.start()
    play(st, base)
    assert zone_of(st, shield) is Zone.HAND
    assert not st.cards[low].rested


@pytest.mark.card("GD03-124")
@pytest.mark.rule("13-2-13-1")
def test_gd03_124_pairing_lv3_pilot_rests_enemy_once_per_turn() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    sc.base(0, RIBO_COLONY)
    u1 = sc.add(0, ZAKU)
    u2 = sc.add(0, HIZACK)
    e1 = sc.add(1, ZAKU)
    e2 = sc.add(1, GAPLANT, damage=1)  # 3 current HP
    big = sc.add(1, AGRISSA)
    p1, p2 = sc.hand(0, SOMA, CHRISTINA)
    st = sc.start()
    play(st, p1, onto=u1)
    assert select_options(st) == {e1, e2}
    select(st, e2)
    assert st.cards[e2].rested
    play(st, p2, onto=u2)
    assert pending_kind(st) is DecisionKind.MAIN
    assert not st.cards[e1].rested
    assert not st.cards[big].rested


@pytest.mark.card("GD03-124")
def test_gd03_124_lv4_pilot_does_not_trigger() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    sc.base(0, RIBO_COLONY)
    unit = sc.add(0, ZAKU)
    enemy = sc.add(1, ZAKU)
    pilot = sc.add(0, YAZAN, Zone.HAND)
    st = sc.start()
    play(st, pilot, onto=unit)
    assert not st.cards[enemy].rested


@pytest.mark.card("GD03-125")
def test_gd03_125_lv6_g_team_battle_kill_may_recover_2_once_per_turn() -> None:
    sc = Scenario()
    sc.base(0, PEACEMILLION)
    first = sc.add(0, HEAVYARMS_EW, damage=1)  # Lv.6 (G Team) 6/4
    second = sc.add(0, HEAVYARMS_EW, damage=1)
    e1 = sc.add(1, ZAKU, rested=True)  # 2 AP
    e2 = sc.add(1, ZAKU, rested=True)
    st = sc.start()
    attack(st, first, e1)
    pass_all(st)
    assert zone_of(st, e1) is Zone.TRASH
    assert pending_kind(st) is DecisionKind.YES_NO
    yes(st)
    assert st.cards[first].damage == 1  # 1 + 2 battle damage - 2 recovered
    attack(st, second, e2)
    pass_all(st)
    assert zone_of(st, e2) is Zone.TRASH
    assert pending_kind(st) is DecisionKind.MAIN
    assert st.cards[second].damage == 3


@pytest.mark.card("GD03-125")
@pytest.mark.ruling("GD03-125:Q254")
def test_gd03_125_unit_destroyed_in_the_same_battle_cannot_recover() -> None:
    sc = Scenario()
    sc.base(0, PEACEMILLION)
    unit = sc.add(0, HEAVYARMS_EW, damage=2)
    enemy = sc.add(1, MARASAI, rested=True)  # 3 AP: 2 + 3 >= 4 HP
    st = sc.start()
    attack(st, unit, enemy)
    pass_all(st)
    if pending_kind(st) is DecisionKind.YES_NO:
        yes(st)
    assert zone_of(st, unit) is Zone.TRASH
    assert zone_of(st, enemy) is Zone.TRASH


@pytest.mark.card("GD03-125")
def test_gd03_125_lower_level_unit_does_not_trigger() -> None:
    sc = Scenario()
    sc.base(0, PEACEMILLION)
    unit = sc.add(0, "GD05-077")  # Leo, Lv.2 (G Team) 2/2
    enemy = sc.add(1, "ST03-007", rested=True)  # Zaku I, 1/2
    st = sc.start()
    attack(st, unit, enemy)
    pass_all(st)
    assert zone_of(st, enemy) is Zone.TRASH
    assert pending_kind(st) is DecisionKind.MAIN
    assert st.cards[unit].damage == 1


@pytest.mark.card("GD03-126")
def test_gd03_126_friendly_unit_tokens_get_ap_plus_1_on_opponent_turn() -> None:
    sc = Scenario()
    sc.base(0, CYCLOPS_TEAM)
    token = sc.add(0, "T-013")
    unit = sc.add(0, ZGOK)
    enemy_token = sc.add(1, "T-013")
    st = sc.start()
    assert ap(st, token) == 2
    to_next_turn(st)
    assert st.active == 1
    assert ap(st, token) == 3
    assert ap(st, unit) == 3
    assert ap(st, enemy_token) == 2


@pytest.mark.card("GD03-127")
def test_gd03_127_deploy_adds_shield_then_zaft_unit_gets_ap_plus_3() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    zaft = sc.add(0, DINN_CMD)
    other = sc.add(0, ZAKU)
    (shield,) = sc.shields(0, ZAKU)
    base = sc.add(0, JACHIN_DUE, Zone.HAND)
    st = sc.start()
    play(st, base)
    assert zone_of(st, shield) is Zone.HAND
    assert ap(st, zaft) == 6
    assert ap(st, other) == 2
    to_next_turn(st)
    assert ap(st, zaft) == 3


@pytest.mark.card("GD03-128")
@pytest.mark.rule("13-2-13-1")
def test_gd03_128_opponent_effect_resting_my_unit_deals_1_damage_once() -> None:
    sc = Scenario(active=1)
    sc.resources(1, 6)
    sc.base(0, DORITEA)
    mine = sc.add(0, ZAKU)
    mine2 = sc.add(0, HIZACK)
    enemy = sc.add(1, GAPLANT)
    c1, c2 = sc.hand(1, RECCOAS_SHADOW, RECCOAS_SHADOW)
    st = sc.start()
    play(st, c1)
    select(st, mine)
    assert st.cards[mine].rested
    assert st.cards[enemy].damage == 1
    play(st, c2)
    select(st, mine2)
    assert st.cards[mine2].rested
    assert st.cards[enemy].damage == 1


@pytest.mark.card("GD03-128")
def test_gd03_128_own_effect_on_own_turn_does_not_trigger() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    sc.base(0, DORITEA)
    mine = sc.add(0, ZAKU)  # Lv.2
    enemy = sc.add(1, GAPLANT)  # Lv.4: no target for Human Karma's damage
    cmd = sc.add(0, HUMAN_KARMA, Zone.HAND)
    st = sc.start()
    play(st, cmd)
    assert st.cards[mine].rested
    assert st.cards[enemy].damage == 0


@pytest.mark.card("GD03-129")
@pytest.mark.rule("5-20-1")
def test_gd03_129_effect_damage_to_tekkadan_may_rest_base_to_mill() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    base = sc.base(0, HOTARUBI)
    barbatos = sc.add(0, BARBATOS_3RD, Zone.HAND)
    st = sc.start()
    top = st.zones[0][Zone.DECK][0]
    play(st, barbatos)
    assert st.cards[barbatos].damage == 2
    assert pending_kind(st) is DecisionKind.YES_NO
    yes(st)
    assert st.cards[base].rested
    assert zone_of(st, top) is Zone.TRASH


@pytest.mark.card("GD03-129")
def test_gd03_129_declining_keeps_base_active() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    base = sc.base(0, HOTARUBI)
    barbatos = sc.add(0, BARBATOS_3RD, Zone.HAND)
    st = sc.start()
    top = st.zones[0][Zone.DECK][0]
    play(st, barbatos)
    no(st)
    assert not st.cards[base].rested
    assert zone_of(st, top) is Zone.DECK


@pytest.mark.card("GD03-129")
def test_gd03_129_battle_damage_does_not_trigger() -> None:
    sc = Scenario()
    base = sc.base(0, HOTARUBI)
    unit = sc.add(0, HYAKUREN)  # (Teiwaz)
    enemy = sc.add(1, ZAKU, rested=True)
    st = sc.start()
    attack(st, unit, enemy)
    pass_all(st)
    assert st.cards[unit].damage == 2
    assert pending_kind(st) is DecisionKind.MAIN
    assert not st.cards[base].rested


@pytest.mark.card("GD03-130")
@pytest.mark.ruling("GD03-130:Q255")
@pytest.mark.rule("5-8-1")
def test_gd03_130_deploy_on_your_turn_pays_to_deploy_vagan_from_trash(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    deployed_events: list[int] = []
    real_emit = core.emit

    def spy(st: GameState, ev: d.Ev, subject: int = NO_ARG, **kw: Any) -> None:
        if ev is d.Ev.DEPLOYED:
            deployed_events.append(subject)
        real_emit(st, ev, subject, **kw)

    monkeypatch.setattr(core, "emit", spy)
    sc = Scenario()
    sc.resources(0, 7)
    (vagan,) = sc.trash(0, ZEDAS_M)  # Lv.3, cost 2
    (too_high,) = sc.trash(0, "GD03-064")  # Defurse, Lv.5
    (shield,) = sc.shields(0, ZAKU)
    base = sc.add(0, DOWNES, Zone.HAND)
    st = sc.start()
    play(st, base)
    assert zone_of(st, shield) is Zone.HAND
    assert select_options(st) == {vagan}
    assert has_action(st, A.DONE)  # "you may choose"
    select(st, vagan)
    assert zone_of(st, vagan) is Zone.BATTLE
    assert zone_of(st, too_high) is Zone.TRASH
    active = [r for r in st.zones[0][Zone.RESOURCE_AREA] if not st.cards[r].rested]
    assert len(active) == 7 - 2 - 2
    assert vagan in deployed_events  # the event every 【Deploy】 effect triggers on (13-2-6-1)


@pytest.mark.card("GD03-130")
def test_gd03_130_unpayable_cost_leaves_the_card_in_trash() -> None:
    sc = Scenario()
    sc.resources(0, 5, rested=3)
    (vagan,) = sc.trash(0, ZEDAS_M)
    base = sc.add(0, DOWNES, Zone.HAND)
    st = sc.start()
    play(st, base)
    select(st, vagan)
    assert zone_of(st, vagan) is Zone.TRASH
    assert zone_of(st, base) is Zone.BASE


@pytest.mark.card("GD03-130")
def test_gd03_130_burst_deploy_on_opponent_turn_offers_no_choice() -> None:
    sc = Scenario()
    attacker = sc.add(0, ZAKU)
    base, _ = sc.shields(1, DOWNES, ZAKU)
    (vagan,) = sc.trash(1, ZEDAS_M)
    sc.resources(1, 5)
    st = sc.start()
    attack(st, attacker)
    pass_all(st)
    yes(st)
    pass_all(st)
    assert zone_of(st, base) is Zone.BASE
    assert zone_of(st, vagan) is Zone.TRASH


@pytest.mark.card("GD03-131")
def test_gd03_131_two_tsa_units_return_enemy_lv4_or_lower() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    sc.add(0, "ST14-010")  # (Triple Ship Alliance)
    sc.add(0, "ST14-010")
    low = sc.add(1, GAPLANT)
    high = sc.add(1, AGRISSA)
    base = sc.add(0, ETERNAL, Zone.HAND)
    st = sc.start()
    play(st, base)
    assert zone_of(st, low) is Zone.HAND
    assert zone_of(st, high) is Zone.BATTLE


@pytest.mark.card("GD03-131")
def test_gd03_131_one_tsa_unit_returns_nothing() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    sc.add(0, "ST14-010")
    low = sc.add(1, GAPLANT)
    base = sc.add(0, ETERNAL, Zone.HAND)
    st = sc.start()
    play(st, base)
    assert zone_of(st, low) is Zone.BATTLE


@pytest.mark.card("GD03-132")
@pytest.mark.rule("13-2-8-1", "8-5-2-4")
def test_gd03_132_destroyed_with_aeug_link_unit_rests_enemy_with_4_or_less_hp() -> None:
    sc = Scenario(active=1)
    base = sc.base(0, RADISH, damage=4)
    sc.add(0, MK2_AEUG, pilot=EMMA)  # (AEUG) Link Unit
    attacker = sc.add(1, ZAKU)
    dented = sc.add(1, GAPLANT, damage=1)  # 3 current HP
    big = sc.add(1, THE_O)  # 5 HP
    st = sc.start()
    attack(st, attacker)
    pass_all(st)
    assert zone_of(st, base) is Zone.TRASH
    assert select_options(st) == {attacker, dented}
    select(st, dented)
    assert st.cards[dented].rested
    assert not st.cards[big].rested


@pytest.mark.card("GD03-132")
def test_gd03_132_no_aeug_link_unit_no_rest() -> None:
    sc = Scenario(active=1)
    base = sc.base(0, RADISH, damage=4)
    sc.add(0, NEMO)  # (AEUG), not a Link Unit
    attacker = sc.add(1, ZAKU)
    other = sc.add(1, GAPLANT)
    st = sc.start()
    attack(st, attacker)
    pass_all(st)
    assert zone_of(st, base) is Zone.TRASH
    assert pending_kind(st) is DecisionKind.MAIN
    assert not st.cards[other].rested
