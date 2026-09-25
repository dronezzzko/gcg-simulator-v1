"""Behaviour and ruling tests for GD01-001..GD01-072 (work package WP-GD01-A)."""

from __future__ import annotations

import pytest

from gcg_sim.engine import view as V
from gcg_sim.engine.game import ALT_PLAY_BASE, SUPPORT_AID
from gcg_sim.engine.state import GameState
from gcg_sim.engine.types import PLAYER_TARGET, ActionKind, DecisionKind, Step, Zone
from gcg_sim.testkit import (
    Scenario,
    act,
    activate,
    ap,
    attack,
    block,
    has_action,
    hp,
    keywords,
    no,
    numbers_in,
    options,
    order,
    pass_,
    pass_all,
    play,
    select,
    to_next_turn,
    yes,
    zone_of,
)

A = ActionKind

ZAKU_MARINER = "GD01-060"  # Zeon, red, Lv2 2/2, vanilla
LOTO = "GD01-011"  # Earth Federation, blue, Lv2 2/2, vanilla
GUNDAM_WBT = "GD01-013"  # Earth Federation / White Base Team, blue, Lv4 3/4, vanilla
PISCES = "GD01-021"  # OZ, blue, Lv1 1/2, vanilla
CANCER = "GD01-022"  # OZ, blue, Lv2 2/3, vanilla
ZAKU_II = "GD01-035"  # Zeon, green, Lv2 2/2, vanilla
WING_GUNDAM = "GD01-040"  # Operation Meteor, green, Lv5 4/3, vanilla
DINN = "GD01-064"  # ZAFT, red, Lv2 3/2, vanilla
REZEL = "GD01-018"  # Earth Federation, blue, Lv3 4/3, vanilla
SHAMBLO = "GD01-047"  # Zeon, red, Lv8 6/5
BALL = "GD01-015"  # Lv1 1/1
LAUNCHER_STRIKE = "GD01-072"  # white, Lv4 3/4, <Blocker>
ARIES_BLOCKER = "ST02-008"  # OZ, blue, Lv2 2/1, <Blocker>
LFRITH_BLOCKER = "GD01-086"  # white, Lv3 2/4, <Blocker>
M1_ASTRAY = "GD01-081"  # white TSA, Lv2 2/2; AP+1 and <Blocker> with another TSA Unit
GRAZE_COMMANDER = "ST05-008"  # white, Lv3 3/2, <Blocker>

RIDDHE = "GD01-089"  # Pilot (Earth Federation) Lv3 +1/+1
RAU = "GD03-091"  # Pilot (ZAFT) Lv4 +2/+1
BANAGHER = "GD01-088"  # Pilot (Civilian, Newtype) Lv5 +2/+2, links Unicorn Gundam
MARIDA = "GD01-093"  # Pilot (Neo Zeon, Cyber-Newtype) Lv4 +2/+1
SAYLA = "GD01-087"  # Pilot (Earth Federation, White Base Team, Newtype) Lv3 +1/+1
MQUVE = "GD01-092"  # Pilot (Zeon) Lv3 +1/+1
ATHRUN = "ST04-011"  # Pilot (ZAFT, Coordinator) Lv4 +1/+2
HEERO = "ST02-010"  # Pilot (Operation Meteor) Lv4 +2/+1
GUEL = "GD01-097"  # Pilot (Academy) Lv3 +1/+1

ZEON_REMNANT = "GD01-115"  # Command Lv2 【Main】/【Action】1 damage to 1 enemy Unit
UNFORESEEN = "ST01-014"  # Command Lv3 【Main】/【Action】1 enemy Unit AP-3 this turn
VALEDICTORIAN = "GD02-105"  # Command Lv2 with a (Newtype) 【Pilot】 effect
EXTREME_HATRED = "GD01-112"  # Command Lv6


def hand_size(st: GameState, player: int = 0) -> int:
    return len(st.zones[player][Zone.HAND])


def rested_resources(st: GameState, player: int = 0) -> int:
    return sum(1 for u in st.zones[player][Zone.RESOURCE_AREA] if st.cards[u].rested)


def active_resources(st: GameState, player: int = 0) -> int:
    return sum(1 for u in st.zones[player][Zone.RESOURCE_AREA] if not st.cards[u].rested)


def pending_kind(st: GameState) -> DecisionKind | None:
    return st.pending.kind if st.pending is not None else None


def resolve_trigger_order(st: GameState) -> None:
    while pending_kind(st) is DecisionKind.ORDER_TRIGGER:
        order(st)


def play_cost(st: GameState, uid: int) -> int:
    return V.play_cost(st, V.derived(st), uid)


def own_action_step(st: GameState, player: int = 0) -> bool:
    return (
        st.pending is not None
        and st.pending.kind is DecisionKind.ACTION_STEP
        and st.pending.player == player
    )


def give_action_command(sc: Scenario, player: int = 1) -> int:
    """Give ``player`` a playable 【Action】 Command so battle action steps stop for a decision."""
    sc.resources(player, 2)
    return sc.add(player, ZEON_REMNANT, Zone.HAND)


def support(st: GameState, host: int, target: int) -> None:
    activate(st, host, SUPPORT_AID)
    if pending_kind(st) is DecisionKind.SELECT:
        select(st, target)


# ---------------------------------------------------------------------------------------------
# GD01-001 Gundam


@pytest.mark.card("GD01-001")
@pytest.mark.ruling("GD01-001:Q119")
def test_gd01_001_white_base_team_units_including_itself_gain_repair() -> None:
    sc = Scenario()
    gundam = sc.add(0, "GD01-001")
    wbt = sc.add(0, GUNDAM_WBT)
    not_wbt = sc.add(0, LOTO)
    enemy_wbt = sc.add(1, GUNDAM_WBT)
    st = sc.start()
    assert keywords(st, gundam).get("Repair") == 1
    assert keywords(st, wbt).get("Repair") == 1
    assert "Repair" not in keywords(st, not_wbt)
    assert "Repair" not in keywords(st, enemy_wbt)


@pytest.mark.card("GD01-001")
@pytest.mark.rule("13-1-1-1")
def test_gd01_001_granted_repair_recovers_another_unit_at_end_of_turn() -> None:
    sc = Scenario()
    sc.add(0, "GD01-001")
    wbt = sc.add(0, GUNDAM_WBT, damage=2)
    st = sc.start()
    to_next_turn(st)
    assert st.cards[wbt].damage == 1


@pytest.mark.card("GD01-001")
@pytest.mark.rule("13-2-9-1")
def test_gd01_001_when_paired_draws_with_two_other_units() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    gundam = sc.add(0, "GD01-001")
    sc.add(0, LOTO)
    sc.add(1, LOTO)
    sc.add(0, LOTO)
    pilot = sc.add(0, RIDDHE, Zone.HAND)
    st = sc.start()
    before = hand_size(st)
    play(st, pilot, onto=gundam)
    assert hand_size(st) == before


@pytest.mark.card("GD01-001")
def test_gd01_001_when_paired_does_not_draw_with_one_other_unit() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    gundam = sc.add(0, "GD01-001")
    sc.add(0, LOTO)
    sc.add(1, LOTO)
    sc.add(1, LOTO)
    pilot = sc.add(0, RIDDHE, Zone.HAND)
    st = sc.start()
    before = hand_size(st)
    play(st, pilot, onto=gundam)
    assert hand_size(st) == before - 1


# ---------------------------------------------------------------------------------------------
# GD01-002 Unicorn Gundam (Destroy Mode)


def _destroy_mode_scenario(*, linked: bool, resources: int = 0) -> tuple[GameState, int, int]:
    sc = Scenario()
    sc.resources(0, resources)
    unicorn = sc.add(0, "GD01-005", pilot=BANAGHER if linked else RAU)
    destroy_mode = sc.add(0, "GD01-002", Zone.HAND)
    sc.add(0, ZAKU_MARINER, Zone.HAND)
    st = sc.start()
    return st, unicorn, destroy_mode


def _play_destroy_mode_for_free(st: GameState, destroy_mode: int) -> None:
    act(st, A.PLAY_UNIT, destroy_mode, None, ALT_PLAY_BASE)
    if pending_kind(st) is DecisionKind.DISCARD:
        select(st, st.zones[0][Zone.HAND][-1])


@pytest.mark.card("GD01-002")
@pytest.mark.ruling("GD01-002:Q120")
def test_gd01_002_played_for_zero_cost_by_destroying_linked_unicorn_mode() -> None:
    st, unicorn, destroy_mode = _destroy_mode_scenario(linked=True, resources=7)
    _play_destroy_mode_for_free(st, destroy_mode)
    assert zone_of(st, destroy_mode) is Zone.BATTLE
    assert zone_of(st, unicorn) is Zone.TRASH
    assert active_resources(st) == 7


@pytest.mark.card("GD01-002")
@pytest.mark.ruling("GD01-002:Q120")
@pytest.mark.rule("2-9-1")
def test_gd01_002_free_play_ignores_level_and_resources() -> None:
    st, unicorn, destroy_mode = _destroy_mode_scenario(linked=True, resources=0)
    plays = [o for o in options(st) if o.kind is A.PLAY_UNIT and o.a == destroy_mode]
    assert [o.c for o in plays] == [ALT_PLAY_BASE]
    _play_destroy_mode_for_free(st, destroy_mode)
    assert zone_of(st, destroy_mode) is Zone.BATTLE
    assert zone_of(st, unicorn) is Zone.TRASH


@pytest.mark.card("GD01-002")
def test_gd01_002_no_free_play_without_a_linked_unicorn_mode() -> None:
    st, _, destroy_mode = _destroy_mode_scenario(linked=False, resources=6)
    assert not has_action(st, A.PLAY_UNIT, destroy_mode)


@pytest.mark.card("GD01-002")
def test_gd01_002_attack_rests_an_enemy_unit() -> None:
    sc = Scenario()
    destroy_mode = sc.add(0, "GD01-002")
    enemy = sc.add(1, ZAKU_MARINER)
    sc.shields(1, ZAKU_MARINER, ZAKU_MARINER)
    st = sc.start()
    attack(st, destroy_mode)
    assert st.cards[enemy].rested


# ---------------------------------------------------------------------------------------------
# GD01-003 Unicorn Gundam 02 Banshee (Destroy Mode)


def _banshee_attack(*, trash: int, linked: bool = True) -> tuple[GameState, int, list[int]]:
    sc = Scenario()
    banshee = sc.add(0, "GD01-003", pilot=MARIDA if linked else RAU)
    cards = sc.trash(0, *([ZAKU_MARINER] * trash))
    sc.shields(1, ZAKU_MARINER, ZAKU_MARINER)
    st = sc.start()
    attack(st, banshee)
    resolve_trigger_order(st)
    return st, banshee, cards


@pytest.mark.card("GD01-003")
@pytest.mark.rule("13-2-12-1", "5-20-1")
def test_gd01_003_returns_twelve_trash_cards_sets_active_and_gains_first_strike() -> None:
    st, banshee, cards = _banshee_attack(trash=12)
    assert all(zone_of(st, u) is Zone.DECK for u in cards)
    assert st.zones[0][Zone.TRASH] == []
    assert not st.cards[banshee].rested
    assert "First Strike" in keywords(st, banshee)


@pytest.mark.card("GD01-003")
@pytest.mark.ruling("GD01-003:Q121")
def test_gd01_003_does_nothing_with_fewer_than_twelve_trash_cards() -> None:
    st, banshee, cards = _banshee_attack(trash=10)
    assert all(zone_of(st, u) is Zone.TRASH for u in cards)
    assert st.cards[banshee].rested
    assert "First Strike" not in keywords(st, banshee)


@pytest.mark.card("GD01-003")
def test_gd01_003_does_nothing_when_not_linked() -> None:
    st, banshee, cards = _banshee_attack(trash=12, linked=False)
    assert all(zone_of(st, u) is Zone.TRASH for u in cards)
    assert st.cards[banshee].rested


# ---------------------------------------------------------------------------------------------
# GD01-004 Guncannon


@pytest.mark.card("GD01-004")
def test_gd01_004_has_repair_and_rests_enemy_with_two_or_less_hp_when_paired() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    guncannon = sc.add(0, "GD01-004")
    low = sc.add(1, ZAKU_MARINER)
    high = sc.add(1, CANCER)
    pilot = sc.add(0, RIDDHE, Zone.HAND)
    st = sc.start()
    assert keywords(st, guncannon).get("Repair") == 1
    play(st, pilot, onto=guncannon)
    assert st.cards[low].rested
    assert not st.cards[high].rested


@pytest.mark.card("GD01-004")
@pytest.mark.faq("Q96")
def test_gd01_004_hp_filter_uses_hp_after_damage() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    guncannon = sc.add(0, "GD01-004")
    damaged = sc.add(1, CANCER, damage=1)
    healthy = sc.add(1, CANCER)
    pilot = sc.add(0, RIDDHE, Zone.HAND)
    st = sc.start()
    play(st, pilot, onto=guncannon)
    assert st.cards[damaged].rested
    assert not st.cards[healthy].rested


# ---------------------------------------------------------------------------------------------
# GD01-005 Unicorn Gundam (Unicorn Mode)


def _destroy_unicorn_mode(pilot: str, damage: int) -> tuple[GameState, int, int, int]:
    sc = Scenario()
    unicorn = sc.add(0, "GD01-005", pilot=pilot, damage=damage)
    pilot_uid = sc.st.cards[unicorn].pair
    other = sc.add(0, ZAKU_MARINER, Zone.HAND)
    target = sc.add(1, ZAKU_MARINER, rested=True)
    st = sc.start()
    attack(st, unicorn, target)
    pass_all(st)
    return st, unicorn, pilot_uid, other


@pytest.mark.card("GD01-005")
@pytest.mark.rule("13-2-8-2-1", "5-20-2")
def test_gd01_005_linked_destroyed_returns_pilot_then_discards() -> None:
    st, unicorn, pilot, other = _destroy_unicorn_mode(BANAGHER, damage=3)
    assert zone_of(st, unicorn) is Zone.TRASH
    assert pending_kind(st) is DecisionKind.DISCARD
    select(st, other)
    assert zone_of(st, pilot) is Zone.HAND
    assert zone_of(st, other) is Zone.TRASH


@pytest.mark.card("GD01-005")
def test_gd01_005_not_linked_destroyed_keeps_pilot_in_trash() -> None:
    st, unicorn, pilot, other = _destroy_unicorn_mode(RAU, damage=2)
    assert zone_of(st, unicorn) is Zone.TRASH
    assert zone_of(st, pilot) is Zone.TRASH
    assert zone_of(st, other) is Zone.HAND


# ---------------------------------------------------------------------------------------------
# GD01-006 Delta Plus


@pytest.mark.card("GD01-006")
@pytest.mark.rule("13-2-12-1")
def test_gd01_006_gets_hp_plus_one_only_while_linked() -> None:
    sc = Scenario()
    linked = sc.add(0, "GD01-006", pilot=RIDDHE)
    paired = sc.add(0, "GD01-006", pilot=RAU)
    st = sc.start()
    assert keywords(st, linked).get("Repair") == 1
    assert hp(st, linked) == 3 + 1 + 1
    assert hp(st, paired) == 3 + 1


# ---------------------------------------------------------------------------------------------
# GD01-007 Noin's Aries


def _destroy_noins_aries(*, other_oz: bool) -> tuple[GameState, int, int]:
    sc = Scenario()
    aries = sc.add(0, "GD01-007", damage=2)
    if other_oz:
        sc.add(0, PISCES)
    else:
        sc.add(0, ZAKU_MARINER)
    target = sc.add(1, ZAKU_MARINER, rested=True)
    st = sc.start()
    before = hand_size(st)
    attack(st, aries, target)
    pass_all(st)
    return st, aries, before


@pytest.mark.card("GD01-007")
def test_gd01_007_destroyed_draws_with_another_oz_unit() -> None:
    st, aries, before = _destroy_noins_aries(other_oz=True)
    assert zone_of(st, aries) is Zone.TRASH
    assert hand_size(st) == before + 1


@pytest.mark.card("GD01-007")
def test_gd01_007_destroyed_without_another_oz_unit_does_not_draw() -> None:
    st, aries, before = _destroy_noins_aries(other_oz=False)
    assert zone_of(st, aries) is Zone.TRASH
    assert hand_size(st) == before


# ---------------------------------------------------------------------------------------------
# GD01-008 Guntank / GD01-020 Anksha


@pytest.mark.card("GD01-008")
def test_gd01_008_deploy_deals_one_damage_to_a_rested_enemy_unit() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    guntank = sc.add(0, "GD01-008", Zone.HAND)
    rested = sc.add(1, ZAKU_MARINER, rested=True)
    active = sc.add(1, ZAKU_MARINER)
    st = sc.start()
    play(st, guntank)
    assert st.cards[rested].damage == 1
    assert st.cards[active].damage == 0


@pytest.mark.card("GD01-020")
def test_gd01_020_deploy_deals_one_damage_to_a_rested_enemy_unit() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    anksha = sc.add(0, "GD01-020", Zone.HAND)
    rested = sc.add(1, CANCER, rested=True)
    active = sc.add(1, CANCER)
    st = sc.start()
    play(st, anksha)
    assert st.cards[rested].damage == 1
    assert st.cards[active].damage == 0


@pytest.mark.card("GD01-020")
def test_gd01_020_deploy_without_rested_enemy_does_nothing() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    anksha = sc.add(0, "GD01-020", Zone.HAND)
    active = sc.add(1, CANCER)
    st = sc.start()
    play(st, anksha)
    assert zone_of(st, anksha) is Zone.BATTLE
    assert st.cards[active].damage == 0


# ---------------------------------------------------------------------------------------------
# GD01-009 G-Fighter


@pytest.mark.card("GD01-009")
@pytest.mark.rule("13-1-6-1")
def test_gd01_009_deploy_grants_high_maneuver_this_turn() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    fighter = sc.add(0, "GD01-009", Zone.HAND)
    gundam = sc.add(0, GUNDAM_WBT)
    sc.add(1, LAUNCHER_STRIKE)
    sc.shields(1, ZAKU_MARINER, ZAKU_MARINER)
    st = sc.start()
    play(st, fighter)
    select(st, gundam)
    assert "High-Maneuver" in keywords(st, gundam)
    attack(st, gundam)
    assert pending_kind(st) is not DecisionKind.BLOCK
    pass_all(st)
    to_next_turn(st)
    assert "High-Maneuver" not in keywords(st, gundam)


# ---------------------------------------------------------------------------------------------
# GD01-010 Banshee (Unicorn Mode) / GD01-012 Zechs' Leo


@pytest.mark.card("GD01-010")
@pytest.mark.card("GD01-012")
@pytest.mark.parametrize("number", ["GD01-010", "GD01-012"])
def test_when_paired_rests_enemy_with_three_or_less_hp(number: str) -> None:
    sc = Scenario()
    sc.resources(0, 4)
    unit = sc.add(0, number)
    low = sc.add(1, CANCER)
    high = sc.add(1, GUNDAM_WBT)
    pilot = sc.add(0, RAU, Zone.HAND)
    st = sc.start()
    play(st, pilot, onto=unit)
    assert st.cards[low].rested
    assert not st.cards[high].rested


@pytest.mark.card("GD01-010")
@pytest.mark.card("GD01-012")
@pytest.mark.faq("Q96")
@pytest.mark.parametrize("number", ["GD01-010", "GD01-012"])
def test_when_paired_hp_filter_uses_hp_after_damage(number: str) -> None:
    sc = Scenario()
    sc.resources(0, 4)
    unit = sc.add(0, number)
    damaged = sc.add(1, GUNDAM_WBT, damage=1)
    healthy = sc.add(1, GUNDAM_WBT)
    pilot = sc.add(0, RAU, Zone.HAND)
    st = sc.start()
    play(st, pilot, onto=unit)
    assert st.cards[damaged].rested
    assert not st.cards[healthy].rested


# ---------------------------------------------------------------------------------------------
# GD01-014 G-Sky Easy


@pytest.mark.card("GD01-014")
@pytest.mark.ruling("GD01-014:Q122")
@pytest.mark.rule("13-2-2-1", "13-2-13-1", "13-2-13-2")
def test_gd01_014_linked_action_recovers_an_enemy_unit_once_per_turn() -> None:
    sc = Scenario()
    gsky = sc.add(0, "GD01-014", pilot=SAYLA)
    other_gsky = sc.add(0, "GD01-014", pilot=SAYLA)
    enemy = sc.add(1, CANCER, damage=2)
    st = sc.start(Step.END_ACTION)
    assert own_action_step(st)
    activate(st, gsky)
    select(st, enemy)
    assert st.cards[enemy].damage == 1
    assert own_action_step(st)
    assert not has_action(st, A.ACTIVATE, gsky)
    assert has_action(st, A.ACTIVATE, other_gsky)


@pytest.mark.card("GD01-014")
@pytest.mark.rule("13-2-12-1")
def test_gd01_014_not_linked_cannot_activate() -> None:
    sc = Scenario()
    linked = sc.add(0, "GD01-014", pilot=SAYLA)
    unlinked = sc.add(0, "GD01-014", pilot=RIDDHE)
    sc.add(1, CANCER, damage=2)
    st = sc.start(Step.END_ACTION)
    assert own_action_step(st)
    assert has_action(st, A.ACTIVATE, linked)
    assert not has_action(st, A.ACTIVATE, unlinked)


# ---------------------------------------------------------------------------------------------
# GD01-015 Ball


@pytest.mark.card("GD01-015")
def test_gd01_015_attack_recovers_one_of_your_units() -> None:
    sc = Scenario()
    ball = sc.add(0, BALL)
    damaged = sc.add(0, GUNDAM_WBT, damage=2)
    sc.shields(1, ZAKU_MARINER)
    st = sc.start()
    attack(st, ball)
    select(st, damaged)
    assert st.cards[damaged].damage == 1


# ---------------------------------------------------------------------------------------------
# GD01-016 Jegan


@pytest.mark.card("GD01-016")
def test_gd01_016_costs_one_less_with_two_earth_federation_units() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    sc.add(0, LOTO)
    sc.add(0, LOTO)
    jegan = sc.add(0, "GD01-016", Zone.HAND)
    st = sc.start()
    assert play_cost(st, jegan) == 1
    play(st, jegan)
    assert rested_resources(st) == 1


@pytest.mark.card("GD01-016")
def test_gd01_016_full_cost_with_one_earth_federation_unit() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    sc.add(0, LOTO)
    sc.add(0, ZAKU_MARINER)
    jegan = sc.add(0, "GD01-016", Zone.HAND)
    st = sc.start()
    assert play_cost(st, jegan) == 2
    play(st, jegan)
    assert rested_resources(st) == 2


@pytest.mark.card("GD01-016")
@pytest.mark.ruling("GD01-016:Q123")
def test_gd01_016_level_is_not_reduced() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    sc.add(0, LOTO)
    sc.add(0, LOTO)
    jegan = sc.add(0, "GD01-016", Zone.HAND)
    st = sc.start()
    assert play_cost(st, jegan) == 1
    assert not has_action(st, A.PLAY_UNIT, jegan)


# ---------------------------------------------------------------------------------------------
# Repair / Breach / Support / Blocker keyword cards


@pytest.mark.card("GD01-017")
@pytest.mark.card("GD01-033")
@pytest.mark.parametrize("number", ["GD01-017", "GD01-033"])
@pytest.mark.rule("13-1-1-1")
def test_repair_one_recovers_at_end_of_turn(number: str) -> None:
    sc = Scenario()
    unit = sc.add(0, number, damage=2)
    st = sc.start()
    assert keywords(st, unit).get("Repair") == 1
    to_next_turn(st)
    assert st.cards[unit].damage == 1


@pytest.mark.card("GD01-030")
@pytest.mark.card("GD01-041")
@pytest.mark.rule("13-1-2-1")
@pytest.mark.parametrize(("number", "amount"), [("GD01-030", 2), ("GD01-041", 3)])
def test_breach_damages_shield_area_after_destroying_a_unit(number: str, amount: int) -> None:
    sc = Scenario()
    unit = sc.add(0, number)
    target = sc.add(1, ZAKU_MARINER, rested=True)
    top, second = sc.shields(1, ZAKU_MARINER, ZAKU_MARINER)
    st = sc.start()
    assert keywords(st, unit).get("Breach") == amount
    attack(st, unit, target)
    pass_all(st)
    assert zone_of(st, target) is Zone.TRASH
    assert zone_of(st, top) is Zone.TRASH
    assert zone_of(st, second) is Zone.SHIELD


@pytest.mark.card("GD01-030")
@pytest.mark.card("GD01-041")
@pytest.mark.parametrize(("number", "amount"), [("GD01-030", 2), ("GD01-041", 3)])
def test_breach_amount_damages_enemy_base(number: str, amount: int) -> None:
    sc = Scenario()
    unit = sc.add(0, number)
    target = sc.add(1, ZAKU_MARINER, rested=True)
    base = sc.base(1)
    (shield,) = sc.shields(1, ZAKU_MARINER)
    st = sc.start()
    attack(st, unit, target)
    pass_all(st)
    ex_base_hp = 3
    remaining = ex_base_hp - st.cards[base].damage if zone_of(st, base) is Zone.BASE else 0
    assert remaining == ex_base_hp - amount
    assert zone_of(st, shield) is Zone.SHIELD


@pytest.mark.card("GD01-055")
@pytest.mark.card("GD01-061")
@pytest.mark.card("GD01-048")
@pytest.mark.rule("13-1-3-1")
@pytest.mark.parametrize(("number", "amount"), [("GD01-055", 2), ("GD01-061", 1), ("GD01-048", 1)])
def test_support_increases_another_units_ap(number: str, amount: int) -> None:
    sc = Scenario()
    supporter = sc.add(0, number)
    ally = sc.add(0, ZAKU_MARINER)
    st = sc.start()
    support(st, supporter, ally)
    assert st.cards[supporter].rested
    assert ap(st, ally) == 2 + amount


@pytest.mark.card("GD01-072")
@pytest.mark.card("GD01-068")
@pytest.mark.rule("13-1-4-1")
@pytest.mark.parametrize("number", ["GD01-072", "GD01-068"])
def test_blocker_changes_the_attack_target(number: str) -> None:
    sc = Scenario()
    attacker = sc.add(0, ZAKU_MARINER)
    blocker = sc.add(1, number)
    (shield,) = sc.shields(1, ZAKU_MARINER)
    st = sc.start()
    attack(st, attacker)
    block(st, blocker)
    pass_all(st)
    assert st.cards[blocker].rested
    assert st.cards[blocker].damage == 2
    assert zone_of(st, shield) is Zone.SHIELD


# ---------------------------------------------------------------------------------------------
# GD01-019 Byarlant Custom


@pytest.mark.card("GD01-019")
@pytest.mark.parametrize(("enemies", "expected"), [(4, True), (3, False)])
def test_gd01_019_blocker_while_four_or_more_enemy_units(enemies: int, expected: bool) -> None:
    sc = Scenario()
    byarlant = sc.add(1, "GD01-019")
    for _ in range(enemies):
        sc.add(0, ZAKU_MARINER)
    st = sc.start()
    assert ("Blocker" in keywords(st, byarlant)) is expected


@pytest.mark.card("GD01-019")
@pytest.mark.ruling("GD01-019:Q124")
@pytest.mark.rule("5-22-2")
def test_gd01_019_block_stands_after_losing_blocker() -> None:
    sc = Scenario()
    attacker = sc.add(0, ZAKU_MARINER)
    victim = sc.add(0, ZAKU_MARINER, damage=1)
    sc.add(0, ZAKU_MARINER)
    sc.add(0, ZAKU_MARINER)
    byarlant = sc.add(1, "GD01-019")
    (shield,) = sc.shields(1, ZAKU_MARINER)
    sc.resources(1, 2)
    command = sc.add(1, ZEON_REMNANT, Zone.HAND)
    st = sc.start()
    attack(st, attacker)
    block(st, byarlant)
    act(st, A.PLAY_COMMAND, command)
    select(st, victim)
    assert zone_of(st, victim) is Zone.TRASH
    assert "Blocker" not in keywords(st, byarlant)
    pass_all(st)
    assert st.cards[byarlant].damage == 2
    assert zone_of(st, attacker) is Zone.TRASH
    assert zone_of(st, shield) is Zone.SHIELD


# ---------------------------------------------------------------------------------------------
# GD01-023 Char's Gelgoog


def _gelgoog(trash: tuple[str, ...], *, paired: bool = False) -> tuple[GameState, int, list[int]]:
    sc = Scenario()
    sc.resources(0, 1)
    gelgoog = sc.add(0, "GD01-023", pilot=RIDDHE if paired else None)
    sc.hand(0, ZAKU_MARINER, RIDDHE)
    cards = sc.trash(0, *trash)
    st = sc.start()
    return st, gelgoog, cards


@pytest.mark.card("GD01-023")
@pytest.mark.ruling("GD01-023:Q125")
def test_gd01_023_pairs_a_newtype_pilot_from_trash_without_paying_its_cost() -> None:
    st, gelgoog, (sayla, mquve) = _gelgoog((SAYLA, MQUVE))
    activate(st, gelgoog)
    assert zone_of(st, sayla) is Zone.PAIRED
    assert st.cards[gelgoog].pair == sayla
    assert zone_of(st, mquve) is Zone.TRASH
    assert numbers_in(st, 0, Zone.HAND) == [RIDDHE]
    assert ZAKU_MARINER in numbers_in(st, 0, Zone.TRASH)
    assert active_resources(st) == 1


@pytest.mark.card("GD01-023")
@pytest.mark.ruling("GD01-023:Q162")
def test_gd01_023_cannot_choose_a_command_with_a_newtype_pilot_effect() -> None:
    st, gelgoog, (command,) = _gelgoog((VALEDICTORIAN,))
    # no Pilot card can be chosen, so the ability cannot be activated (rule 10-2-2, FAQ Q100)
    assert not has_action(st, A.ACTIVATE, gelgoog)
    assert zone_of(st, command) is Zone.TRASH
    assert st.cards[gelgoog].pair < 0


@pytest.mark.card("GD01-023")
def test_gd01_023_does_not_pair_when_already_paired() -> None:
    st, gelgoog, (sayla,) = _gelgoog((SAYLA,), paired=True)
    activate(st, gelgoog)
    assert zone_of(st, sayla) is Zone.TRASH


@pytest.mark.card("GD01-023")
def test_gd01_023_requires_a_zeon_unit_card_to_discard() -> None:
    sc = Scenario()
    gelgoog = sc.add(0, "GD01-023")
    sc.hand(0, LOTO, RIDDHE)
    sc.trash(0, SAYLA)
    st = sc.start()
    assert not has_action(st, A.ACTIVATE, gelgoog)


# ---------------------------------------------------------------------------------------------
# GD01-024 Wing Gundam Zero


@pytest.mark.card("GD01-024")
@pytest.mark.ruling("GD01-024:Q126")
def test_gd01_024_deploy_deals_three_damage_to_all_units_lv5_or_lower() -> None:
    sc = Scenario()
    sc.resources(0, 8)
    zero = sc.add(0, "GD01-024", Zone.HAND)
    own_low = sc.add(0, ZAKU_MARINER)
    own_high = sc.add(0, "GD01-025")
    enemy_lv5 = sc.add(1, WING_GUNDAM)
    enemy_lv4 = sc.add(1, GUNDAM_WBT)
    st = sc.start()
    play(st, zero)
    assert zone_of(st, own_low) is Zone.TRASH
    assert st.cards[own_high].damage == 0
    assert zone_of(st, enemy_lv5) is Zone.TRASH
    assert st.cards[enemy_lv4].damage == 3
    assert st.cards[zero].damage == 0
    assert "High-Maneuver" in keywords(st, zero)


# ---------------------------------------------------------------------------------------------
# GD01-025 Gundam Deathscythe (Lv6)


@pytest.mark.card("GD01-025")
@pytest.mark.rule("13-2-9-2", "5-20-2")
def test_gd01_025_operation_meteor_pilot_places_resource_and_first_strike() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    sc.resource_deck(0, 2)
    deathscythe = sc.add(0, "GD01-025")
    pilot = sc.add(0, HEERO, Zone.HAND)
    st = sc.start()
    play(st, pilot, onto=deathscythe)
    assert len(st.zones[0][Zone.RESOURCE_AREA]) == 5
    assert rested_resources(st) == 2
    assert "First Strike" in keywords(st, deathscythe)
    to_next_turn(st)
    assert "First Strike" not in keywords(st, deathscythe)


@pytest.mark.card("GD01-025")
@pytest.mark.ruling("GD01-025:Q127")
def test_gd01_025_gains_first_strike_with_empty_resource_deck() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    deathscythe = sc.add(0, "GD01-025")
    pilot = sc.add(0, HEERO, Zone.HAND)
    st = sc.start()
    play(st, pilot, onto=deathscythe)
    assert len(st.zones[0][Zone.RESOURCE_AREA]) == 4
    assert "First Strike" in keywords(st, deathscythe)


@pytest.mark.card("GD01-025")
def test_gd01_025_other_pilot_does_not_trigger() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    sc.resource_deck(0, 2)
    deathscythe = sc.add(0, "GD01-025")
    pilot = sc.add(0, RAU, Zone.HAND)
    st = sc.start()
    play(st, pilot, onto=deathscythe)
    assert len(st.zones[0][Zone.RESOURCE_AREA]) == 4
    assert "First Strike" not in keywords(st, deathscythe)


# ---------------------------------------------------------------------------------------------
# GD01-026 Char's Zaku II


@pytest.mark.card("GD01-026")
@pytest.mark.rule("13-2-10-1", "5-17-1")
def test_gd01_026_paired_destroyed_deploys_rested_char_zaku_token() -> None:
    sc = Scenario()
    zaku = sc.add(0, "GD01-026", pilot=RAU, damage=2)
    target = sc.add(1, ZAKU_MARINER, rested=True)
    st = sc.start()
    attack(st, zaku, target)
    pass_all(st)
    assert zone_of(st, zaku) is Zone.TRASH
    tokens = [u for u in st.zones[0][Zone.BATTLE] if V.cdef(st, u).card_number == "T-006"]
    assert len(tokens) == 1
    token = tokens[0]
    assert st.cards[token].rested
    assert (ap(st, token), hp(st, token)) == (3, 1)
    assert V.traits_of(st, V.derived(st), token) == ("Zeon",)


@pytest.mark.card("GD01-026")
def test_gd01_026_unpaired_destroyed_deploys_nothing() -> None:
    sc = Scenario()
    zaku = sc.add(0, "GD01-026", damage=1)
    target = sc.add(1, ZAKU_MARINER, rested=True)
    st = sc.start()
    attack(st, zaku, target)
    pass_all(st)
    assert zone_of(st, zaku) is Zone.TRASH
    assert st.zones[0][Zone.BATTLE] == []


# ---------------------------------------------------------------------------------------------
# GD01-027 Big Zam


def _big_zam(zeon_in_trash: int) -> tuple[GameState, int, int, int, int]:
    sc = Scenario()
    sc.resources(0, 7)
    big_zam = sc.add(0, "GD01-027", Zone.HAND)
    sc.trash(0, *([ZAKU_MARINER] * zeon_in_trash), LOTO, LOTO)
    own_blocker = sc.add(0, LAUNCHER_STRIKE)
    enemy_blocker = sc.add(1, "GD01-068")
    enemy_plain = sc.add(1, GUNDAM_WBT)
    st = sc.start()
    play(st, big_zam)
    return st, big_zam, own_blocker, enemy_blocker, enemy_plain


@pytest.mark.card("GD01-027")
@pytest.mark.ruling("GD01-027:Q128")
def test_gd01_027_deploy_with_ten_zeon_units_in_trash_damages_all_blockers() -> None:
    st, big_zam, own_blocker, enemy_blocker, enemy_plain = _big_zam(10)
    assert keywords(st, big_zam).get("Breach") == 4
    assert zone_of(st, own_blocker) is Zone.TRASH
    assert zone_of(st, enemy_blocker) is Zone.TRASH
    assert st.cards[enemy_plain].damage == 0


@pytest.mark.card("GD01-027")
def test_gd01_027_deploy_with_nine_zeon_units_in_trash_does_nothing() -> None:
    st, _, own_blocker, enemy_blocker, _ = _big_zam(9)
    assert st.cards[own_blocker].damage == 0
    assert st.cards[enemy_blocker].damage == 0


# ---------------------------------------------------------------------------------------------
# GD01-028 Gundam Sandrock


@pytest.mark.card("GD01-028")
@pytest.mark.ruling("GD01-028:Q129", "GD01-028:Q130")
def test_gd01_028_deploys_maganac_unit_from_hand_free_and_its_deploy_triggers() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    sandrock = sc.add(0, "GD01-028", Zone.HAND)
    rasid = sc.add(0, "GD01-043", Zone.HAND)
    zaku_ii = sc.add(0, ZAKU_II)
    enemy = sc.add(1, ZAKU_MARINER)
    st = sc.start()
    play(st, sandrock)
    yes(st)
    assert zone_of(st, rasid) is Zone.BATTLE
    assert active_resources(st) == 2
    select(st, zaku_ii)
    assert has_action(st, A.ATTACK, zaku_ii, enemy)


@pytest.mark.card("GD01-028")
def test_gd01_028_may_decline_to_deploy() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    sandrock = sc.add(0, "GD01-028", Zone.HAND)
    rasid = sc.add(0, "GD01-043", Zone.HAND)
    st = sc.start()
    play(st, sandrock)
    no(st)
    assert zone_of(st, rasid) is Zone.HAND


# ---------------------------------------------------------------------------------------------
# GD01-029 Shenlong Gundam (Lv5)


@pytest.mark.card("GD01-029")
def test_gd01_029_attack_destroys_enemy_blocker_lv3_or_lower() -> None:
    sc = Scenario()
    shenlong = sc.add(0, "GD01-029")
    low = sc.add(1, LFRITH_BLOCKER)
    high = sc.add(1, LAUNCHER_STRIKE)
    sc.shields(1, ZAKU_MARINER, ZAKU_MARINER)
    st = sc.start()
    assert keywords(st, shenlong).get("Breach") == 4
    attack(st, shenlong)
    assert zone_of(st, low) is Zone.TRASH
    assert zone_of(st, high) is Zone.BATTLE


# ---------------------------------------------------------------------------------------------
# GD01-032 Gyan


@pytest.mark.card("GD01-032")
@pytest.mark.parametrize(("pilot", "destroyed"), [(MQUVE, True), (RIDDHE, False)])
def test_gd01_032_zeon_pilot_destroys_enemy_blocker_lv2_or_lower(
    pilot: str, destroyed: bool
) -> None:
    sc = Scenario()
    sc.resources(0, 3)
    gyan = sc.add(0, "GD01-032")
    low = sc.add(1, ARIES_BLOCKER)
    lv3 = sc.add(1, LFRITH_BLOCKER)
    pilot_uid = sc.add(0, pilot, Zone.HAND)
    st = sc.start()
    play(st, pilot_uid, onto=gyan)
    assert (zone_of(st, low) is Zone.TRASH) is destroyed
    assert zone_of(st, lv3) is Zone.BATTLE


# ---------------------------------------------------------------------------------------------
# GD01-034 Gundam Heavyarms


@pytest.mark.card("GD01-034")
@pytest.mark.rule("13-2-10-1")
def test_gd01_034_gains_breach_three_only_while_paired() -> None:
    sc = Scenario()
    paired = sc.add(0, "GD01-034", pilot=RAU)
    unpaired = sc.add(0, "GD01-034")
    st = sc.start()
    assert keywords(st, paired).get("Breach") == 3
    assert "Breach" not in keywords(st, unpaired)


# ---------------------------------------------------------------------------------------------
# GD01-038 Adzam


@pytest.mark.card("GD01-038")
@pytest.mark.parametrize(("enemies", "damage"), [(5, 1), (4, 0)])
def test_gd01_038_deploy_damages_all_enemy_units_with_five_or_more(
    enemies: int, damage: int
) -> None:
    sc = Scenario()
    sc.resources(0, 5)
    adzam = sc.add(0, "GD01-038", Zone.HAND)
    own = sc.add(0, ZAKU_MARINER)
    foes = [sc.add(1, CANCER) for _ in range(enemies)]
    st = sc.start()
    play(st, adzam)
    assert all(st.cards[u].damage == damage for u in foes)
    assert st.cards[own].damage == 0


# ---------------------------------------------------------------------------------------------
# GD01-039 Dopp


@pytest.mark.card("GD01-039")
@pytest.mark.parametrize(("bottom", "index"), [(True, -1), (False, 0)])
def test_gd01_039_deploy_puts_top_card_on_top_or_bottom(bottom: bool, index: int) -> None:
    sc = Scenario()
    sc.resources(0, 1)
    dopp = sc.add(0, "GD01-039", Zone.HAND)
    sc.deck(0, GUNDAM_WBT, LOTO, LOTO)
    st = sc.start()
    top = st.zones[0][Zone.DECK][0]
    play(st, dopp)
    assert pending_kind(st) is DecisionKind.ARRANGE
    act(st, A.SELECT, 1 if bottom else 0)
    assert st.zones[0][Zone.DECK][index] == top


# ---------------------------------------------------------------------------------------------
# GD01-042 Duo's Leo / GD01-043 Rasid's Maganac


@pytest.mark.card("GD01-042")
def test_gd01_042_may_attack_active_enemy_lv2_or_lower() -> None:
    sc = Scenario()
    leo = sc.add(0, "GD01-042")
    lv2 = sc.add(1, ZAKU_MARINER)
    lv3 = sc.add(1, REZEL)
    st = sc.start()
    assert has_action(st, A.ATTACK, leo, lv2)
    assert not has_action(st, A.ATTACK, leo, lv3)


@pytest.mark.card("GD01-043")
def test_gd01_043_chosen_green_unit_may_attack_active_enemy_with_four_or_less_ap() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    rasid = sc.add(0, "GD01-043", Zone.HAND)
    zaku_ii = sc.add(0, ZAKU_II)
    red = sc.add(0, ZAKU_MARINER)
    ap4 = sc.add(1, REZEL)
    ap6 = sc.add(1, SHAMBLO)
    st = sc.start()
    play(st, rasid)
    select(st, zaku_ii)
    assert has_action(st, A.ATTACK, zaku_ii, ap4)
    assert not has_action(st, A.ATTACK, zaku_ii, ap6)
    assert not has_action(st, A.ATTACK, red, ap4)
    to_next_turn(st)
    to_next_turn(st)
    assert not has_action(st, A.ATTACK, zaku_ii, ap4)


# ---------------------------------------------------------------------------------------------
# GD01-044 Kshatriya (Lv5)


@pytest.mark.card("GD01-044")
def test_gd01_044_newtype_pilot_deals_one_damage_to_up_to_two_enemies() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    kshatriya = sc.add(0, "GD01-044")
    a, b, c = (sc.add(1, CANCER) for _ in range(3))
    pilot = sc.add(0, MARIDA, Zone.HAND)
    st = sc.start()
    play(st, pilot, onto=kshatriya)
    select(st, a, b)
    assert [st.cards[u].damage for u in (a, b, c)] == [1, 1, 0]


@pytest.mark.card("GD01-044")
def test_gd01_044_one_target_is_enough() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    kshatriya = sc.add(0, "GD01-044")
    a, b = (sc.add(1, CANCER) for _ in range(2))
    pilot = sc.add(0, BANAGHER, Zone.HAND)
    st = sc.start()
    play(st, pilot, onto=kshatriya)
    act(st, A.SELECT, a)
    act(st, A.DONE)
    assert [st.cards[u].damage for u in (a, b)] == [1, 0]


@pytest.mark.card("GD01-044")
def test_gd01_044_other_pilot_does_not_trigger() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    kshatriya = sc.add(0, "GD01-044")
    a = sc.add(1, CANCER)
    pilot = sc.add(0, RAU, Zone.HAND)
    st = sc.start()
    play(st, pilot, onto=kshatriya)
    assert st.cards[a].damage == 0


# ---------------------------------------------------------------------------------------------
# GD01-045 Duel Gundam (Assault Shroud)


def _duel_as() -> tuple[GameState, int, int]:
    sc = Scenario()
    sc.resources(0, 4)
    duel = sc.add(0, "GD01-045")
    sc.deck(0, "GD01-049", DINN, GUNDAM_WBT, LOTO, LOTO)
    pilot = sc.add(0, RAU, Zone.HAND)
    st = sc.start()
    return st, duel, pilot


@pytest.mark.card("GD01-045")
@pytest.mark.ruling("GD01-045:Q131", "GD01-045:Q132")
def test_gd01_045_deploys_zaft_unit_from_top_three_free_and_it_triggers() -> None:
    st, duel, pilot = _duel_as()
    top3 = st.zones[0][Zone.DECK][:3]
    blitz, dinn, other = top3
    play(st, pilot, onto=duel)
    assert active_resources(st) == 3
    yes(st)
    select(st, blitz)
    assert zone_of(st, blitz) is Zone.BATTLE
    assert active_resources(st) == 3
    assert "First Strike" in keywords(st, duel)
    assert set(st.zones[0][Zone.DECK][-2:]) == {dinn, other}


@pytest.mark.card("GD01-045")
@pytest.mark.ruling("GD01-045:Q471")
def test_gd01_045_look_is_forced_but_deploy_is_optional() -> None:
    st, duel, pilot = _duel_as()
    top3 = st.zones[0][Zone.DECK][:3]
    play(st, pilot, onto=duel)
    assert pending_kind(st) is DecisionKind.YES_NO
    no(st)
    assert set(st.zones[0][Zone.DECK][-3:]) == set(top3)
    assert all(zone_of(st, u) is Zone.DECK for u in top3)


# ---------------------------------------------------------------------------------------------
# GD01-046 Buster Gundam


def _buster(pilot: str | None, ally: str) -> tuple[GameState, int, int]:
    sc = Scenario()
    buster = sc.add(0, "GD01-046", pilot=pilot)
    target = sc.add(0, ally)
    st = sc.start()
    return st, buster, target


@pytest.mark.card("GD01-046")
@pytest.mark.rule("13-1-3-1", "13-2-10-2", "13-2-13-1")
def test_gd01_046_coordinator_pilot_sets_active_after_supporting_zaft_once_per_turn() -> None:
    st, buster, dinn = _buster(ATHRUN, DINN)
    support(st, buster, dinn)
    assert ap(st, dinn) == 3 + 3
    assert not st.cards[buster].rested
    support(st, buster, dinn)
    assert ap(st, dinn) == 3 + 6
    assert st.cards[buster].rested


@pytest.mark.card("GD01-046")
def test_gd01_046_supporting_a_non_zaft_unit_does_not_set_active() -> None:
    st, buster, zaku = _buster(ATHRUN, ZAKU_MARINER)
    support(st, buster, zaku)
    assert ap(st, zaku) == 2 + 3
    assert st.cards[buster].rested


@pytest.mark.card("GD01-046")
@pytest.mark.parametrize("pilot", [RAU, None])
def test_gd01_046_needs_a_coordinator_pilot(pilot: str | None) -> None:
    st, buster, dinn = _buster(pilot, DINN)
    support(st, buster, dinn)
    assert ap(st, dinn) == 3 + 3
    assert st.cards[buster].rested


# ---------------------------------------------------------------------------------------------
# GD01-047 Shamblo


@pytest.mark.card("GD01-047")
@pytest.mark.parametrize(("rested_allies", "damage"), [(2, 3), (1, 0)])
def test_gd01_047_attack_deals_three_with_two_other_rested_friendly_units(
    rested_allies: int, damage: int
) -> None:
    sc = Scenario()
    shamblo = sc.add(0, SHAMBLO)
    for _ in range(rested_allies):
        sc.add(0, ZAKU_MARINER, rested=True)
    sc.add(0, ZAKU_MARINER)
    enemy = sc.add(1, GUNDAM_WBT)
    sc.shields(1, ZAKU_MARINER, ZAKU_MARINER)
    st = sc.start()
    attack(st, shamblo)
    assert st.cards[enemy].damage == damage


# ---------------------------------------------------------------------------------------------
# GD01-048 Zaku I Sniper Type


@pytest.mark.card("GD01-048")
def test_gd01_048_deploy_may_add_top_zeon_unit_to_hand() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    sniper = sc.add(0, "GD01-048", Zone.HAND)
    sc.deck(0, ZAKU_MARINER, LOTO, LOTO)
    st = sc.start()
    top = st.zones[0][Zone.DECK][0]
    play(st, sniper)
    select(st, top)
    assert zone_of(st, top) is Zone.HAND
    assert st.cards[top].known == 0b11


@pytest.mark.card("GD01-048")
def test_gd01_048_non_zeon_top_card_goes_to_bottom() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    sniper = sc.add(0, "GD01-048", Zone.HAND)
    sc.deck(0, LOTO, ZAKU_MARINER, ZAKU_MARINER)
    st = sc.start()
    top = st.zones[0][Zone.DECK][0]
    play(st, sniper)
    assert st.zones[0][Zone.DECK][-1] == top


@pytest.mark.card("GD01-048")
def test_gd01_048_declined_zeon_card_goes_to_bottom() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    sniper = sc.add(0, "GD01-048", Zone.HAND)
    sc.deck(0, ZAKU_MARINER, LOTO, LOTO)
    st = sc.start()
    top = st.zones[0][Zone.DECK][0]
    play(st, sniper)
    act(st, A.DONE)
    assert st.zones[0][Zone.DECK][-1] == top


# ---------------------------------------------------------------------------------------------
# GD01-049 Blitz Gundam


@pytest.mark.card("GD01-049")
@pytest.mark.parametrize(("pilot", "gains"), [(RAU, True), (None, False)])
def test_gd01_049_deploy_grants_first_strike_to_zaft_unit_with_five_ap(
    pilot: str | None, gains: bool
) -> None:
    sc = Scenario()
    sc.resources(0, 4)
    blitz = sc.add(0, "GD01-049", Zone.HAND)
    duel = sc.add(0, "GD01-045", pilot=pilot)
    st = sc.start()
    play(st, blitz)
    assert ("First Strike" in keywords(st, duel)) is gains
    assert "First Strike" not in keywords(st, blitz)


# ---------------------------------------------------------------------------------------------
# GD01-050 LaGOWE


def _lagowe(supporter: str, target_unit: bool) -> tuple[GameState, int, int]:
    sc = Scenario()
    lagowe = sc.add(0, "GD01-050")
    helper = sc.add(0, supporter)
    target = sc.add(1, CANCER, rested=True)
    other = sc.add(1, GUNDAM_WBT)
    sc.shields(1, ZAKU_MARINER, ZAKU_MARINER)
    st = sc.start()
    support(st, helper, lagowe)
    attack(st, lagowe, target if target_unit else PLAYER_TARGET)
    if pending_kind(st) is DecisionKind.SELECT:
        select(st, other)
    return st, lagowe, other


@pytest.mark.card("GD01-050")
def test_gd01_050_five_ap_attacking_a_unit_deals_two_damage() -> None:
    st, lagowe, other = _lagowe("GD01-046", target_unit=True)
    assert ap(st, lagowe) == 5
    assert st.cards[other].damage == 2


@pytest.mark.card("GD01-050")
def test_gd01_050_four_ap_does_nothing() -> None:
    st, lagowe, other = _lagowe("GD01-055", target_unit=True)
    assert ap(st, lagowe) == 4
    assert st.cards[other].damage == 0


@pytest.mark.card("GD01-050")
def test_gd01_050_attacking_the_player_does_nothing() -> None:
    st, _, other = _lagowe("GD01-046", target_unit=False)
    assert st.cards[other].damage == 0


# ---------------------------------------------------------------------------------------------
# GD01-052 Geara Zulu (Guards Type) / GD01-053 Geara Doga (Heavy Armed Type)


@pytest.mark.card("GD01-052")
def test_gd01_052_deploy_deals_one_damage_to_an_enemy_unit() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    zulu = sc.add(0, "GD01-052", Zone.HAND)
    enemy = sc.add(1, GUNDAM_WBT)
    st = sc.start()
    play(st, zulu)
    assert st.cards[enemy].damage == 1


@pytest.mark.card("GD01-053")
@pytest.mark.rule("10-1-7-3", "13-2-13-1")
def test_gd01_053_pay_one_to_damage_enemy_with_two_or_less_ap_once_per_turn() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    doga = sc.add(0, "GD01-053")
    low = sc.add(1, ZAKU_MARINER)
    high = sc.add(1, GUNDAM_WBT)
    st = sc.start()
    activate(st, doga)
    assert st.cards[low].damage == 1
    assert st.cards[high].damage == 0
    assert rested_resources(st) == 1
    assert not has_action(st, A.ACTIVATE, doga)


# ---------------------------------------------------------------------------------------------
# GD01-054 Duel Gundam


@pytest.mark.card("GD01-054")
def test_gd01_054_gains_breach_three_with_five_ap() -> None:
    sc = Scenario()
    duel = sc.add(0, "GD01-054")
    bucue = sc.add(0, "GD01-055")
    st = sc.start()
    assert "Breach" not in keywords(st, duel)
    support(st, bucue, duel)
    assert keywords(st, duel).get("Breach") == 3


@pytest.mark.card("GD01-054")
@pytest.mark.ruling("GD01-054:Q133")
@pytest.mark.rule("10-1-5-3")
def test_gd01_054_loses_breach_when_ap_is_reduced_below_five() -> None:
    sc = Scenario()
    duel = sc.add(0, "GD01-054")
    bucue = sc.add(0, "GD01-055")
    sc.shields(1, ZAKU_MARINER, ZAKU_MARINER)
    sc.resources(1, 3)
    command = sc.add(1, UNFORESEEN, Zone.HAND)
    st = sc.start()
    support(st, bucue, duel)
    attack(st, duel)
    act(st, A.PLAY_COMMAND, command)
    if pending_kind(st) is DecisionKind.SELECT:
        select(st, duel)
    assert ap(st, duel) == 2
    assert "Breach" not in keywords(st, duel)


# ---------------------------------------------------------------------------------------------
# GD01-056 Geara Doga (Sleeves)


@pytest.mark.card("GD01-056")
def test_gd01_056_destroyed_deals_one_damage_to_enemy_with_five_or_less_ap() -> None:
    sc = Scenario()
    sleeves = sc.add(0, "GD01-056", damage=2)
    target = sc.add(1, ZAKU_MARINER, rested=True)
    big = sc.add(1, SHAMBLO)
    small = sc.add(1, LOTO)
    st = sc.start()
    attack(st, sleeves, target)
    pass_all(st)
    assert zone_of(st, sleeves) is Zone.TRASH
    assert st.cards[small].damage == 1
    assert st.cards[big].damage == 0


# ---------------------------------------------------------------------------------------------
# GD01-058 Galluss-K


@pytest.mark.card("GD01-058")
@pytest.mark.ruling("GD01-058:Q134")
@pytest.mark.rule("13-2-2-1", "8-6-1")
def test_gd01_058_action_gives_a_lv4_unit_of_either_side_ap_plus_one_during_battle() -> None:
    sc = Scenario()
    sc.resources(0, 1)
    galluss = sc.add(0, "GD01-058")
    attacker = sc.add(0, ZAKU_MARINER)
    own_lv4 = sc.add(0, GUNDAM_WBT)
    enemy = sc.add(1, GUNDAM_WBT)
    sc.shields(1, ZAKU_MARINER, ZAKU_MARINER)
    give_action_command(sc, 1)
    st = sc.start()
    attack(st, attacker)
    pass_(st)
    assert own_action_step(st)
    activate(st, galluss)
    assert {o.a for o in options(st) if o.kind is A.SELECT} == {own_lv4, enemy}
    select(st, enemy)
    assert own_action_step(st, 1)
    assert ap(st, enemy) == 4
    assert rested_resources(st) == 1
    pass_all(st)
    assert ap(st, enemy) == 3


@pytest.mark.card("GD01-058")
def test_gd01_058_outside_a_battle_the_bonus_does_not_last_into_later_turns() -> None:
    sc = Scenario()
    sc.resources(0, 1)
    galluss = sc.add(0, "GD01-058")
    ally = sc.add(0, GUNDAM_WBT)
    st = sc.start(Step.END_ACTION)
    assert own_action_step(st)
    activate(st, galluss)
    assert st.active == 1
    assert pending_kind(st) is DecisionKind.MAIN
    assert ap(st, ally) == 3


# ---------------------------------------------------------------------------------------------
# GD01-059 Zee Zulu


@pytest.mark.card("GD01-059")
@pytest.mark.ruling("GD01-059:Q135")
def test_gd01_059_attacking_player_gets_ap_plus_two_even_when_blocked() -> None:
    sc = Scenario()
    zee = sc.add(0, "GD01-059")
    blocker = sc.add(1, LAUNCHER_STRIKE)
    sc.shields(1, ZAKU_MARINER)
    give_action_command(sc, 1)
    st = sc.start()
    attack(st, zee)
    assert ap(st, zee) == 4
    block(st, blocker)
    assert own_action_step(st, 1)
    assert ap(st, zee) == 4
    pass_all(st)
    assert zone_of(st, blocker) is Zone.TRASH


@pytest.mark.card("GD01-059")
def test_gd01_059_attacking_a_unit_gets_no_bonus() -> None:
    sc = Scenario()
    zee = sc.add(0, "GD01-059")
    target = sc.add(1, GUNDAM_WBT, rested=True)
    st = sc.start()
    attack(st, zee, target)
    pass_all(st)
    assert st.cards[target].damage == 2


# ---------------------------------------------------------------------------------------------
# GD01-063 ZnO


@pytest.mark.card("GD01-063")
@pytest.mark.rule("13-1-5-2", "5-22-1")
def test_gd01_063_first_strike_when_battling_enemy_lv2_or_lower_on_your_turn() -> None:
    sc = Scenario()
    zno = sc.add(0, "GD01-063")
    target = sc.add(1, ZAKU_MARINER, rested=True)
    give_action_command(sc, 1)
    st = sc.start()
    assert "First Strike" not in keywords(st, zno)
    attack(st, zno, target)
    assert own_action_step(st, 1)
    assert "First Strike" in keywords(st, zno)
    pass_all(st)
    assert zone_of(st, target) is Zone.TRASH
    assert zone_of(st, zno) is Zone.BATTLE
    assert st.cards[zno].damage == 0
    assert "First Strike" not in keywords(st, zno)


@pytest.mark.card("GD01-063")
def test_gd01_063_no_first_strike_against_lv3() -> None:
    sc = Scenario()
    zno = sc.add(0, "GD01-063")
    target = sc.add(1, GRAZE_COMMANDER, rested=True)
    st = sc.start()
    attack(st, zno, target)
    pass_all(st)
    assert zone_of(st, target) is Zone.TRASH
    assert zone_of(st, zno) is Zone.TRASH


@pytest.mark.card("GD01-063")
@pytest.mark.ruling("GD01-063:Q136")
@pytest.mark.rule("5-22-2")
def test_gd01_063_loses_first_strike_when_blocked_by_lv4() -> None:
    sc = Scenario()
    zno = sc.add(0, "GD01-063")
    target = sc.add(1, ZAKU_MARINER, rested=True)
    blocker = sc.add(1, LAUNCHER_STRIKE, damage=2)
    give_action_command(sc, 1)
    st = sc.start()
    attack(st, zno, target)
    assert "First Strike" in keywords(st, zno)
    block(st, blocker)
    assert own_action_step(st, 1)
    assert "First Strike" not in keywords(st, zno)
    pass_all(st)
    assert zone_of(st, blocker) is Zone.TRASH
    assert zone_of(st, zno) is Zone.TRASH


@pytest.mark.card("GD01-063")
def test_gd01_063_no_first_strike_on_opponents_turn() -> None:
    sc = Scenario(active=1)
    zno = sc.add(0, "GD01-063", rested=True)
    attacker = sc.add(1, ZAKU_MARINER)
    st = sc.start()
    attack(st, attacker, zno)
    pass_all(st)
    assert zone_of(st, zno) is Zone.TRASH
    assert zone_of(st, attacker) is Zone.TRASH


# ---------------------------------------------------------------------------------------------
# GD01-065 Freedom Gundam


def _freedom(*, paired: bool) -> tuple[GameState, int, int, int]:
    sc = Scenario()
    sc.resources(0, 6)
    freedom = sc.add(0, "GD01-065", pilot=RAU if paired else None)
    white = sc.add(0, LAUNCHER_STRIKE)
    enemy = sc.add(1, GUNDAM_WBT)
    sc.hand(0, RIDDHE, RIDDHE)
    st = sc.start()
    return st, freedom, white, enemy


@pytest.mark.card("GD01-065")
@pytest.mark.rule("13-2-10-1")
def test_gd01_065_pairing_with_a_white_unit_gives_enemy_ap_minus_two_this_turn() -> None:
    st, freedom, white, enemy = _freedom(paired=True)
    assert "Blocker" in keywords(st, freedom)
    pilot_a, _ = st.zones[0][Zone.HAND]
    play(st, pilot_a, onto=white)
    assert ap(st, enemy) == 1
    to_next_turn(st)
    to_next_turn(st)
    assert ap(st, enemy) == 3


@pytest.mark.card("GD01-065")
@pytest.mark.rule("13-2-13-1")
def test_gd01_065_triggers_once_per_turn() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    sc.add(0, "GD01-065", pilot=RAU)
    white_a = sc.add(0, LAUNCHER_STRIKE)
    white_b = sc.add(0, LAUNCHER_STRIKE)
    enemy = sc.add(1, GUNDAM_WBT)
    pilot_a, pilot_b = sc.hand(0, RIDDHE, RIDDHE)
    st = sc.start()
    play(st, pilot_a, onto=white_a)
    play(st, pilot_b, onto=white_b)
    assert ap(st, enemy) == 1


@pytest.mark.card("GD01-065")
def test_gd01_065_pairing_with_freedom_itself_triggers() -> None:
    st, freedom, _, enemy = _freedom(paired=False)
    pilot_a, _ = st.zones[0][Zone.HAND]
    play(st, pilot_a, onto=freedom)
    assert ap(st, enemy) == 1


@pytest.mark.card("GD01-065")
def test_gd01_065_unpaired_freedom_does_not_trigger() -> None:
    st, _, white, enemy = _freedom(paired=False)
    pilot_a, _ = st.zones[0][Zone.HAND]
    play(st, pilot_a, onto=white)
    assert ap(st, enemy) == 3


@pytest.mark.card("GD01-065")
def test_gd01_065_pairing_with_a_non_white_unit_does_not_trigger() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    sc.add(0, "GD01-065", pilot=RAU)
    red = sc.add(0, ZAKU_MARINER)
    enemy = sc.add(1, GUNDAM_WBT)
    (pilot,) = sc.hand(0, RIDDHE)
    st = sc.start()
    play(st, pilot, onto=red)
    assert ap(st, enemy) == 3


@pytest.mark.card("GD01-065")
@pytest.mark.rule("5-17-2-3")
def test_gd01_065_unit_tokens_are_not_white() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    sc.add(0, "GD01-065", pilot=RAU)
    token = sc.add(0, "T-011")
    enemy = sc.add(1, GUNDAM_WBT)
    (pilot,) = sc.hand(0, RIDDHE)
    st = sc.start()
    play(st, pilot, onto=token)
    assert st.cards[token].pair == pilot
    assert ap(st, enemy) == 3


# ---------------------------------------------------------------------------------------------
# GD01-066 Justice Gundam


def _fatum_tokens(st: GameState, player: int = 0) -> list[int]:
    return [u for u in st.zones[player][Zone.BATTLE] if V.cdef(st, u).card_number == "T-011"]


@pytest.mark.card("GD01-066")
@pytest.mark.rule("5-17-1", "5-17-2-4")
def test_gd01_066_deploy_creates_fatum_00_token_with_blocker() -> None:
    sc = Scenario()
    sc.resources(0, 7)
    justice = sc.add(0, "GD01-066", Zone.HAND)
    st = sc.start()
    play(st, justice)
    (token,) = _fatum_tokens(st)
    assert not st.cards[token].rested
    assert (ap(st, token), hp(st, token)) == (2, 2)
    assert "Blocker" in keywords(st, token)
    assert V.traits_of(st, V.derived(st), token) == ("Triple Ship Alliance",)


@pytest.mark.card("GD01-066")
@pytest.mark.rule("13-2-10-1")
def test_gd01_066_paired_attack_lets_new_token_attack_this_turn() -> None:
    sc = Scenario()
    justice = sc.add(0, "GD01-066", pilot=RAU)
    token = sc.add(0, "T-011", deployed_this_turn=True)
    sc.shields(1, ZAKU_MARINER, ZAKU_MARINER, ZAKU_MARINER)
    st = sc.start()
    assert not has_action(st, A.ATTACK, token)
    attack(st, justice)
    pass_all(st)
    assert has_action(st, A.ATTACK, token)


@pytest.mark.card("GD01-066")
def test_gd01_066_unpaired_attack_does_not_help_the_token() -> None:
    sc = Scenario()
    justice = sc.add(0, "GD01-066")
    token = sc.add(0, "T-011", deployed_this_turn=True)
    sc.shields(1, ZAKU_MARINER, ZAKU_MARINER, ZAKU_MARINER)
    st = sc.start()
    attack(st, justice)
    pass_all(st)
    assert not has_action(st, A.ATTACK, token)


@pytest.mark.card("GD01-066")
@pytest.mark.ruling("GD01-066:Q137")
@pytest.mark.rule("11-4-2")
def test_gd01_066_token_with_full_battle_area_trashes_an_existing_unit_first() -> None:
    sc = Scenario()
    sc.resources(0, 7)
    existing = [sc.add(0, ZAKU_MARINER) for _ in range(5)]
    justice = sc.add(0, "GD01-066", Zone.HAND)
    st = sc.start()
    play(st, justice)
    assert pending_kind(st) is DecisionKind.EXCESS
    select(st, existing[0])
    assert zone_of(st, existing[0]) is Zone.TRASH
    assert zone_of(st, justice) is Zone.BATTLE
    assert len(_fatum_tokens(st)) == 1
    assert len(st.zones[0][Zone.BATTLE]) == 6


# ---------------------------------------------------------------------------------------------
# GD01-067 Gundam Aerial Rebuild


@pytest.mark.card("GD01-067")
def test_gd01_067_when_paired_adds_command_lv5_or_lower_from_trash() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    aerial = sc.add(0, "GD01-067")
    low, high, unit = sc.trash(0, ZEON_REMNANT, EXTREME_HATRED, ZAKU_MARINER)
    pilot = sc.add(0, GUEL, Zone.HAND)
    st = sc.start()
    play(st, pilot, onto=aerial)
    assert zone_of(st, low) is Zone.HAND
    assert zone_of(st, high) is Zone.TRASH
    assert zone_of(st, unit) is Zone.TRASH


# ---------------------------------------------------------------------------------------------
# GD01-068 Perfect Strike Gundam


@pytest.mark.card("GD01-068")
def test_gd01_068_deploy_returns_enemy_unit_with_one_hp() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    strike = sc.add(0, "GD01-068", Zone.HAND)
    one_hp = sc.add(1, BALL)
    two_hp = sc.add(1, ZAKU_MARINER)
    st = sc.start()
    play(st, strike)
    assert zone_of(st, one_hp) is Zone.HAND
    assert zone_of(st, two_hp) is Zone.BATTLE


@pytest.mark.card("GD01-068")
@pytest.mark.faq("Q96")
def test_gd01_068_damaged_unit_with_one_hp_left_is_returned() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    strike = sc.add(0, "GD01-068", Zone.HAND)
    damaged = sc.add(1, ZAKU_MARINER, damage=1)
    healthy = sc.add(1, ZAKU_MARINER)
    st = sc.start()
    play(st, strike)
    assert zone_of(st, damaged) is Zone.HAND
    assert zone_of(st, healthy) is Zone.BATTLE


# ---------------------------------------------------------------------------------------------
# GD01-069 Strike Rouge


@pytest.mark.card("GD01-069")
def test_gd01_069_sets_rested_white_blocker_active_but_it_cannot_attack() -> None:
    sc = Scenario()
    sc.resources(0, 1)
    rouge = sc.add(0, "GD01-069")
    blocker = sc.add(0, LAUNCHER_STRIKE, rested=True)
    sc.shields(1, ZAKU_MARINER)
    st = sc.start()
    activate(st, rouge)
    assert not st.cards[blocker].rested
    assert not has_action(st, A.ATTACK, blocker)
    assert not has_action(st, A.ACTIVATE, rouge)
    to_next_turn(st)
    to_next_turn(st)
    assert has_action(st, A.ATTACK, blocker)


@pytest.mark.card("GD01-069")
def test_gd01_069_not_activatable_without_a_rested_white_blocker() -> None:
    sc = Scenario()
    sc.resources(0, 1)
    rouge = sc.add(0, "GD01-069")
    sc.add(0, LAUNCHER_STRIKE)
    sc.add(0, ARIES_BLOCKER, rested=True)
    st = sc.start()
    assert not has_action(st, A.ACTIVATE, rouge)


@pytest.mark.card("GD01-069")
@pytest.mark.ruling("GD01-069:Q138")
def test_gd01_069_still_cannot_attack_after_losing_blocker() -> None:
    sc = Scenario()
    sc.resources(0, 1)
    rouge = sc.add(0, "GD01-069")
    m1 = sc.add(0, M1_ASTRAY, rested=True)
    wall = sc.add(1, SHAMBLO, rested=True)
    st = sc.start()
    assert "Blocker" in keywords(st, m1)
    activate(st, rouge)
    assert not st.cards[m1].rested
    attack(st, rouge, wall)
    pass_all(st)
    assert zone_of(st, rouge) is Zone.TRASH
    assert "Blocker" not in keywords(st, m1)
    assert not has_action(st, A.ATTACK, m1)


# ---------------------------------------------------------------------------------------------
# GD01-070 Gundam Aerial


@pytest.mark.card("GD01-070")
@pytest.mark.parametrize(("commands", "cost"), [(4, 1), (3, 3)])
def test_gd01_070_costs_two_less_with_four_commands_in_trash(commands: int, cost: int) -> None:
    sc = Scenario()
    sc.resources(0, 5)
    aerial = sc.add(0, "GD01-070", Zone.HAND)
    sc.trash(0, *([ZEON_REMNANT] * commands), ZAKU_MARINER)
    st = sc.start()
    assert play_cost(st, aerial) == cost
    play(st, aerial)
    assert rested_resources(st) == cost


@pytest.mark.card("GD01-070")
@pytest.mark.ruling("GD01-070:Q139")
def test_gd01_070_level_is_not_reduced() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    aerial = sc.add(0, "GD01-070", Zone.HAND)
    sc.trash(0, *([ZEON_REMNANT] * 4))
    st = sc.start()
    assert play_cost(st, aerial) == 1
    assert not has_action(st, A.PLAY_UNIT, aerial)


# ---------------------------------------------------------------------------------------------
# GD01-071 Gundam Pharact


@pytest.mark.card("GD01-071")
@pytest.mark.parametrize(("pilot", "damage"), [(GUEL, 1), (RIDDHE, 3)])
def test_gd01_071_linked_attack_gives_enemy_ap_minus_two_during_battle(
    pilot: str, damage: int
) -> None:
    sc = Scenario()
    pharact = sc.add(0, "GD01-071", pilot=pilot)
    target = sc.add(1, GUNDAM_WBT, rested=True)
    other = sc.add(1, LOTO)
    st = sc.start()
    attack(st, pharact, target)
    if pending_kind(st) is DecisionKind.SELECT:
        select(st, target)
    pass_all(st)
    assert st.cards[pharact].damage == damage
    assert ap(st, other) == 2
