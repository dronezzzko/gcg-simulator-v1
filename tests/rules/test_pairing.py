"""Rules tests for card types (rule 3) and pairing / linking (2-4-3 .. 2-12, 3-2-6, 3-3, 3-4-6, 5-9)."""

from __future__ import annotations

import pytest

from gcg_sim.cards.db import load_raw_printings
from gcg_sim.cards.model import CardType
from gcg_sim.effects import dsl as d
from gcg_sim.effects.registry import get_registry
from gcg_sim.engine import view as V
from gcg_sim.engine.state import GameState
from gcg_sim.engine.types import PLAYER_TARGET, ActionKind, DecisionKind, Step, Zone
from gcg_sim.testkit import (
    Scenario,
    act,
    ap,
    attack,
    block,
    end_main,
    has_action,
    hp,
    keywords,
    no,
    options,
    pass_all,
    play,
    select,
    to_next_turn,
    uids_in,
    yes,
    zone_of,
)

A = ActionKind

# Units
ZAKU_MARINER = "GD01-060"  # Red Lv2 C1 2/2 (Zeon), no effects
PISCES = "GD01-021"  # Blue Lv1 C1 1/2 (OZ), no effects
DEMI_GARRISON = "GD01-085"  # White Lv2 C1 2/2 (Academy), no effects
REZEL = "GD01-018"  # Blue Lv3 C2 4/3 (Earth Federation), no effects
CANCER = "GD01-022"  # Blue Lv2 C2 2/3 (OZ), no effects
DINN = "GD01-064"  # Red Lv2 C2 3/2 (ZAFT), no effects
GUNDAM = "GD01-013"  # Blue Lv4 C2 3/4, link [Amuro Ray], no effects
GELGOOG = "GD01-031"  # Green Lv4 C2 4/3, link (Zeon) trait, no effects
WING_GUNDAM = "GD01-040"  # Green Lv5 C2 4/3, link [Heero Yuy], no effects
GUNCANNON = "ST01-003"  # Blue Lv3 C2 2/4, link [Kai Shiden], no effects
GUNDAM_X = "GD02-056"  # Purple Lv4 C3 3/4, link [Garrod Ran]
AGRISSA = "GD04-079"  # White Lv5 C3 5/4, link [Ali al-Saachez], no effects
FA_UNICORN = "GD03-016"  # Blue Lv5 C3 5/4, link [Banagher Links], no effects
GUNDAM_WHEN_PAIRED = "GD01-001"  # 【When Paired】If you have 2 or more other Units in play, draw 1.
MICHAELIS = "GD05-030"  # may attack a rested enemy Unit on the turn it is deployed
CHARS_GELGOOG = "GD01-023"  # 【Activate･Main】pair a (Newtype) Pilot card Lv.3- from the trash
NT1_FULL_ARMOR = "GD03-007"  # "Gundam NT-1 Full Armor", Blue Lv3 C2 2/3
GUNDAM_ROSE = "GD05-044"  # activates 【Main】 on its paired card when it attacks, if linked

# Pilot cards
AMURO = "ST01-010"  # Blue Lv4 C1 +2/+1; 【When Paired】rest an enemy Unit with 5 or less HP
RIDDHE = "GD01-089"  # Blue Lv3 C1 +1/+1 (Earth Federation)
MQUVE = "GD01-092"  # Green Lv3 C1 +1/+1 (Zeon); While this Unit is (Zeon), it gains <Breach 1>
CAGALLI = "GD01-096"  # White Lv4 C1 +1/+1 (Orb); While this Unit is white, it gains <Blocker>
SAYLA = "GD01-087"  # Blue Lv3 C1 +1/+1; While this Unit is blue, it gains <Repair 1>
HEERO = "ST02-010"  # Green Lv4 C1 +2/+1; 【During Link】This Unit gets AP+1 and HP+1
SETSUNA = "ST07-009"  # Purple Lv4 C1 +2/+1; 【Attack】This Unit gets AP+1 during this turn
GARROD = "GD02-094"  # "Garrod Ran & Tiffa Adill", Purple Lv4 C1 +1/+2; optional 【When Paired】
JAMIL = "GD03-096"  # Purple Lv4 C1 +2/+1 (Vulture)
ATHRUN = "ST04-011"  # Red Lv4 C1 +1/+2 (ZAFT, Coordinator)
ALI = "GD04-099"  # White Lv4 C1 +2/+1; 【During Link】【Attack】may return 1 enemy Pilot to hand
BANAGHER = "GD01-088"  # Blue Lv5 C1 +2/+2; 【When Linked】Draw 1.
LALAH = "GD02-089"  # Green Lv3 C1 (Zeon, Newtype)
SOMA = "GD03-100"  # White Lv3 C1 +1/+1; 【Destroyed】Choose 1 enemy Unit. It gets AP-3 this turn
CHRISTINA = "GD03-085"  # hand-location text: 0 cost when paired with a "Gundam NT-1" Unit

# Command cards
KAIS_RESOLVE = "ST01-013"  # Blue Lv3 C1 【Main】a friendly Unit recovers 3; Pilot +1/+0
THOROUGHLY_DAMAGED = "ST01-012"  # Blue Lv2 C1 【Main】1 damage to a rested enemy; 【Pilot】 +0/+1
DESERT_TIGER = "GD01-113"  # Red Lv3 C1 【Main】/【Action】friendly (ZAFT) Unit AP+3; 【Pilot】
TORRINGTON = "GD01-114"  # Red Lv1 C1 【Action】2 friendly Units AP+1; 【Pilot】
VALEDICTORIAN = "GD02-105"  # Green Lv2 C1 【Action】only, (Zeon, Newtype); 【Pilot】 +1/+0
BLAZING_RIDER = "ST14-014"  # White Lv5 C1; 【Pilot】[Garrod Ran & Tiffa Adill] +2/+1
HAWK = "ST04-013"  # White Lv2 C1 【Main】/【Action】return an enemy Unit with 3 or less HP to hand
VETERANS_PRIDE = "GD05-116"  # Purple Lv2 C1 【Main】/【Action】destroy an enemy Unit Lv.2 or lower
ALL_RANGE_ATTACK = "GD02-107"  # Red Lv4 C2 【Main】1 damage to all non-Link enemy Units
HEALTHY_CURIOSITY = "GD03-101"  # Blue Lv3 C1 【Main】Draw 1. Then, if 2+ in trash, rest an enemy
SIGNS_OF_REVOLUTION = "GD01-104"  # Blue Lv3 C2 【Burst】Draw 1.
FIRST_CONTACT = "GD01-107"  # Green Lv3 C3 【Main】Place 1 rested Resource.
ROSE_SCREAMER = "GD05-113"  # 【Main】an (MF) Unit with AP 4- gets AP+2; Pilot [George de Sand]

# Bases
DESERT_BASE = "GD01-126"  # Green Lv2 C1 0/6; 【Deploy】Add 1 of your Shields to your hand.
KUSANAGI = "GD01-129"  # White Lv4 C2 0/4


def _linked(st: GameState, unit: int) -> bool:
    return V.is_linked(V.derived(st), unit)


def _traits(st: GameState, uid: int) -> tuple[str, ...]:
    return V.traits_of(st, V.derived(st), uid)


def _decline_optional(st: GameState) -> None:
    """Answer every optional prompt ("You may ...") with no until a main-phase decision."""
    for _ in range(20):
        dec = st.pending
        if dec is None or dec.kind is DecisionKind.MAIN:
            return
        if dec.kind is DecisionKind.YES_NO:
            no(st)
        elif dec.kind is DecisionKind.SELECT and has_action(st, A.DONE):
            act(st, A.DONE)
        else:
            return


def _kinds_for(st: GameState, uid: int) -> set[ActionKind]:
    return {o.kind for o in options(st) if o.a == uid}


def _pair_targets(st: GameState, card: int) -> set[int]:
    return {o.b for o in options(st) if o.kind is A.PAIR and o.a == card}


def _rested_resources(st: GameState, player: int) -> int:
    return sum(st.cards[r].rested for r in uids_in(st, player, Zone.RESOURCE_AREA))


def _deploy_and_pair(st: GameState, unit: int, pilot: int) -> None:
    """Deploy ``unit`` from the hand this turn, then pair ``pilot`` from the hand with it."""
    play(st, unit)
    play(st, pilot, onto=unit)
    _decline_optional(st)


# =============================================================================================
# 3-1 card types


@pytest.mark.rule("3-1")
def test_there_are_exactly_five_printed_card_types() -> None:
    printed = {c.card_type for c in get_registry().db.real_cards() if not c.is_token}
    assert printed == {
        CardType.UNIT,
        CardType.PILOT,
        CardType.COMMAND,
        CardType.BASE,
        CardType.RESOURCE,
    }


@pytest.mark.rule("3-1", "3-2-1", "3-3-1", "3-4-1", "3-5-1")
def test_each_card_type_is_played_its_own_way() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    own_unit = sc.add(0, ZAKU_MARINER)
    sc.add(1, ZAKU_MARINER)  # an enemy Unit is not a pairing target
    unit = sc.add(0, ZAKU_MARINER, Zone.HAND)
    pilot = sc.add(0, RIDDHE, Zone.HAND)
    command = sc.add(0, SIGNS_OF_REVOLUTION, Zone.HAND)
    base = sc.add(0, DESERT_BASE, Zone.HAND)
    sc.add(1, CANCER, rested=True)
    st = sc.start()
    assert _kinds_for(st, unit) == {A.PLAY_UNIT}
    assert _kinds_for(st, pilot) == {A.PAIR}
    assert _pair_targets(st, pilot) == {own_unit}
    assert _kinds_for(st, command) == {A.PLAY_COMMAND}
    assert _kinds_for(st, base) == {A.PLAY_BASE}


# =============================================================================================
# 3-2 Units


@pytest.mark.rule("3-2-1")
def test_played_unit_card_is_deployed_into_the_battle_area() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    unit = sc.add(0, ZAKU_MARINER, Zone.HAND)
    st = sc.start()
    play(st, unit)
    assert zone_of(st, unit) is Zone.BATTLE
    assert unit in uids_in(st, 0, Zone.BATTLE)
    assert unit not in uids_in(st, 0, Zone.HAND)


@pytest.mark.rule("3-2-1")
def test_unit_cards_outside_the_battle_area_are_not_units() -> None:
    """GD05-116 "Choose 1 enemy Unit that is Lv.2 or lower. Destroy it." cannot pick Unit cards
    in the hand or trash, so it is not playable until an enemy Unit is in the battle area."""
    sc = Scenario()
    sc.resources(0, 2)
    sc.add(1, ZAKU_MARINER, Zone.HAND)
    sc.add(1, ZAKU_MARINER, Zone.TRASH)
    cmd = sc.add(0, VETERANS_PRIDE, Zone.HAND)
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, cmd)

    sc = Scenario()
    sc.resources(0, 2)
    in_hand = sc.add(1, ZAKU_MARINER, Zone.HAND)
    enemy = sc.add(1, ZAKU_MARINER)
    cmd = sc.add(0, VETERANS_PRIDE, Zone.HAND)
    st = sc.start()
    play(st, cmd)
    assert zone_of(st, enemy) is Zone.TRASH
    assert zone_of(st, in_hand) is Zone.HAND


@pytest.mark.rule("3-2-3")
def test_only_units_can_attack() -> None:
    sc = Scenario()
    unit = sc.add(0, ZAKU_MARINER, pilot=RIDDHE)
    base = sc.base(0, DESERT_BASE)
    sc.add(1, CANCER, rested=True)
    st = sc.start()
    pilot = st.cards[unit].pair
    attackers = {o.a for o in options(st) if o.kind is A.ATTACK}
    assert attackers == {unit}
    assert pilot not in attackers and base not in attackers


@pytest.mark.rule("3-2-4")
def test_new_unit_cannot_attack_until_its_next_turn() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    unit = sc.add(0, ZAKU_MARINER, Zone.HAND)
    sc.add(1, CANCER, rested=True)
    st = sc.start()
    play(st, unit)
    assert not has_action(st, A.ATTACK, unit)
    to_next_turn(st)
    to_next_turn(st)
    assert st.active == 0
    assert has_action(st, A.ATTACK, unit, PLAYER_TARGET)


@pytest.mark.rule("3-2-4")
def test_unless_specified_otherwise_a_new_unit_may_attack() -> None:
    """GD05-030: "On the turn this Unit is deployed, it may choose a rested enemy Unit as its
    attack target and attack it." Only that attack is allowed, not one on the player."""
    sc = Scenario()
    sc.resources(0, 4)
    unit = sc.add(0, MICHAELIS, Zone.HAND)
    rested_enemy = sc.add(1, CANCER, rested=True)
    active_enemy = sc.add(1, ZAKU_MARINER)
    st = sc.start()
    play(st, unit)
    assert has_action(st, A.ATTACK, unit, rested_enemy)
    assert not has_action(st, A.ATTACK, unit, PLAYER_TARGET)
    assert not has_action(st, A.ATTACK, unit, active_enemy)


@pytest.mark.rule("3-2-5", "3-2-5-1", "3-2-5-2")
def test_units_deal_their_ap_as_battle_damage_and_survive_while_hp_remains() -> None:
    sc = Scenario()
    attacker = sc.add(0, ZAKU_MARINER)  # 2 AP / 2 HP
    target = sc.add(1, GUNDAM, rested=True)  # 3 AP / 4 HP
    st = sc.start()
    assert (ap(st, attacker), hp(st, attacker)) == (2, 2)
    assert (ap(st, target), hp(st, target)) == (3, 4)
    attack(st, attacker, target)
    pass_all(st)
    assert st.cards[target].damage == 2  # the attacker's AP
    assert zone_of(st, target) is Zone.BATTLE  # 4 HP - 2 damage = 2 remaining
    assert zone_of(st, attacker) is Zone.TRASH  # 3 damage on 2 HP


@pytest.mark.rule("3-2-5-1", "3-2-5-2")
def test_unit_whose_hp_reaches_zero_is_destroyed() -> None:
    sc = Scenario()
    attacker = sc.add(0, REZEL)  # 4 AP / 3 HP
    target = sc.add(1, CANCER, rested=True)  # 2 AP / 3 HP
    st = sc.start()
    attack(st, attacker, target)
    pass_all(st)
    assert zone_of(st, target) is Zone.TRASH  # 4 damage on 3 HP
    assert zone_of(st, attacker) is Zone.BATTLE
    assert st.cards[attacker].damage == 2  # 2 damage on 3 HP


# =============================================================================================
# 2-12 / 3-2-6 link conditions and Link Units


@pytest.mark.rule("2-12-2", "3-2-6")
def test_only_unit_cards_have_link_conditions() -> None:
    with_link = {
        r["card_type"] for r in load_raw_printings() if r.get("link") not in (None, "", "-")
    }
    assert with_link == {"UNIT"}
    db = get_registry().db
    assert {c.card_type for c in db if c.link is not None} == {CardType.UNIT}
    for number in (AMURO, KAIS_RESOLVE, DESERT_BASE):
        assert db[number].link is None


@pytest.mark.rule("2-12-1", "3-2-6-1", "3-2-6-2")
def test_link_condition_by_pilot_name() -> None:
    link = get_registry().db[GUNDAM].link
    assert link is not None and link.name_parts == ("Amuro Ray",)
    sc = Scenario()
    linked_unit = sc.add(0, GUNDAM, pilot=AMURO)
    other_pilot = sc.add(0, GUNDAM, pilot=RIDDHE)
    no_pilot = sc.add(0, GUNDAM)
    st = sc.start()
    assert _linked(st, linked_unit)
    assert not _linked(st, other_pilot)
    assert not _linked(st, no_pilot)


@pytest.mark.rule("2-12-1", "3-2-6-1", "3-2-6-2")
def test_link_condition_by_pilot_trait() -> None:
    db = get_registry().db
    link = db[GELGOOG].link
    assert link is not None and link.traits == ("Zeon",)
    assert "Zeon" in db[MQUVE].traits and "Zeon" not in db[RIDDHE].traits
    sc = Scenario()
    zeon_pilot = sc.add(0, GELGOOG, pilot=MQUVE)
    other_pilot = sc.add(0, GELGOOG, pilot=RIDDHE)
    st = sc.start()
    assert _linked(st, zeon_pilot)
    assert not _linked(st, other_pilot)


@pytest.mark.rule("3-2-6-2")
def test_only_units_whose_pilot_satisfies_the_link_condition_are_link_units() -> None:
    """GD02-107: "Deal 1 damage to all enemy Units other than Link Units." """
    sc = Scenario()
    sc.resources(0, 4)
    link_unit = sc.add(1, GUNDAM, pilot=AMURO)
    paired_not_linked = sc.add(1, GUNDAM, pilot=RIDDHE)
    unpaired = sc.add(1, GUNDAM)
    cmd = sc.add(0, ALL_RANGE_ATTACK, Zone.HAND)
    st = sc.start()
    play(st, cmd)
    assert st.cards[link_unit].damage == 0
    assert st.cards[paired_not_linked].damage == 1
    assert st.cards[unpaired].damage == 1


@pytest.mark.rule("3-2-6-3", "3-2-6-2")
@pytest.mark.faq("Q48")
def test_link_unit_can_attack_on_the_turn_it_is_deployed() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    unit = sc.add(0, GUNDAM, Zone.HAND)
    pilot = sc.add(0, AMURO, Zone.HAND)
    st = sc.start()
    play(st, unit)
    assert not has_action(st, A.ATTACK, unit)
    play(st, pilot, onto=unit)
    assert _linked(st, unit)
    assert has_action(st, A.ATTACK, unit, PLAYER_TARGET)


@pytest.mark.rule("3-2-6-3")
def test_paired_unit_that_is_not_linked_cannot_attack_on_the_turn_it_is_deployed() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    unit = sc.add(0, GUNDAM, Zone.HAND)
    pilot = sc.add(0, RIDDHE, Zone.HAND)
    st = sc.start()
    _deploy_and_pair(st, unit, pilot)
    assert st.cards[unit].pair == pilot
    assert not _linked(st, unit)
    assert not has_action(st, A.ATTACK, unit)


@pytest.mark.rule("3-2-6-4")
def test_bracketed_link_name_matches_a_pilot_name_containing_it() -> None:
    """Rule 3-2-6-4 example: Garrod Ran & Tiffa Adill satisfies a Gundam X's [Garrod Ran]."""
    db = get_registry().db
    link = db[GUNDAM_X].link
    assert link is not None and link.name_parts == ("Garrod Ran",)
    assert db[GARROD].name == "Garrod Ran & Tiffa Adill"
    sc = Scenario()
    sc.resources(0, 4)
    unit = sc.add(0, GUNDAM_X, Zone.HAND)
    pilot = sc.add(0, GARROD, Zone.HAND)
    st = sc.start()
    _deploy_and_pair(st, unit, pilot)
    assert _linked(st, unit)
    assert has_action(st, A.ATTACK, unit, PLAYER_TARGET)


@pytest.mark.rule("3-2-6-4")
def test_bracketed_link_name_rejects_a_pilot_without_it_in_its_name() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    unit = sc.add(0, GUNDAM_X, Zone.HAND)
    pilot = sc.add(0, JAMIL, Zone.HAND)
    st = sc.start()
    _deploy_and_pair(st, unit, pilot)
    assert st.cards[unit].pair == pilot
    assert not _linked(st, unit)
    assert not has_action(st, A.ATTACK, unit)


# =============================================================================================
# 3-3 Pilots


@pytest.mark.rule("3-3-1", "5-9-1")
@pytest.mark.faq("Q47")
def test_playing_a_pilot_places_it_beneath_a_unit_and_pairs_them() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    unit = sc.add(0, ZAKU_MARINER)
    pilot = sc.add(0, RIDDHE, Zone.HAND)
    st = sc.start()
    play(st, pilot, onto=unit)
    assert zone_of(st, pilot) is Zone.PAIRED
    assert uids_in(st, 0, Zone.PAIRED) == [pilot]
    assert st.cards[pilot].pair == unit and st.cards[unit].pair == pilot
    assert zone_of(st, unit) is Zone.BATTLE
    assert _rested_resources(st, 0) == 1  # its cost was paid


@pytest.mark.rule("3-3-1")
def test_only_paired_pilots_are_pilots() -> None:
    """GD04-099 "choose 1 enemy Pilot": enemy Pilot cards in the hand or trash are not Pilots."""
    sc = Scenario()
    attacker = sc.add(0, AGRISSA, pilot=ALI)
    enemy_unit = sc.add(1, ZAKU_MARINER, pilot=RIDDHE)
    sc.add(1, MQUVE, Zone.TRASH)
    sc.add(1, SAYLA, Zone.HAND)
    sc.shields(1, ZAKU_MARINER)
    st = sc.start()
    enemy_pilot = st.cards[enemy_unit].pair
    attack(st, attacker)
    assert st.pending is not None and st.pending.kind is DecisionKind.SELECT
    assert {o.a for o in options(st) if o.kind is A.SELECT} == {enemy_pilot}


@pytest.mark.rule("3-3-3")
@pytest.mark.faq("Q85")
def test_pilot_cannot_be_placed_in_the_battle_area_by_itself() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    pilot = sc.add(0, RIDDHE, Zone.HAND)
    command_pilot = sc.add(0, KAIS_RESOLVE, Zone.HAND)
    st = sc.start()
    assert _kinds_for(st, pilot) == set()
    assert _kinds_for(st, command_pilot) == set()
    end_main(st)
    assert not uids_in(st, 0, Zone.PAIRED)


@pytest.mark.rule("3-3-3", "3-3-8-1")
def test_pilot_taken_off_its_unit_leaves_the_battle_area() -> None:
    """GD04-099 returns an enemy Pilot to its owner's hand: the Pilot leaves the battle area, the
    Unit stays unpaired, and the Pilot's AP/HP stop being added."""
    sc = Scenario()
    attacker = sc.add(0, AGRISSA, pilot=ALI)
    enemy_unit = sc.add(1, ZAKU_MARINER, pilot=MQUVE)
    sc.shields(1, ZAKU_MARINER)
    st = sc.start()
    enemy_pilot = st.cards[enemy_unit].pair
    assert (ap(st, enemy_unit), hp(st, enemy_unit)) == (3, 3)
    attack(st, attacker)
    select(st, enemy_pilot)
    assert zone_of(st, enemy_pilot) is Zone.HAND
    assert not uids_in(st, 1, Zone.PAIRED)
    assert zone_of(st, enemy_unit) is Zone.BATTLE
    assert st.cards[enemy_unit].pair < 0
    assert (ap(st, enemy_unit), hp(st, enemy_unit)) == (2, 2)


@pytest.mark.rule("3-3-8-1", "3-2-5-2")
def test_losing_the_pilot_hp_modifier_can_destroy_the_unit() -> None:
    sc = Scenario()
    attacker = sc.add(0, AGRISSA, pilot=ALI)
    enemy_unit = sc.add(1, ZAKU_MARINER, pilot=RIDDHE, damage=2)
    sc.shields(1, ZAKU_MARINER)
    st = sc.start()
    enemy_pilot = st.cards[enemy_unit].pair
    assert hp(st, enemy_unit) == 3
    attack(st, attacker)
    select(st, enemy_pilot)
    assert zone_of(st, enemy_pilot) is Zone.HAND
    assert zone_of(st, enemy_unit) is Zone.TRASH  # 2 damage, HP back to 2


@pytest.mark.rule("3-3-4")
def test_at_most_one_pilot_per_unit() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    paired = sc.add(0, ZAKU_MARINER, pilot=RIDDHE)
    free = sc.add(0, ZAKU_MARINER)
    sc.add(1, ZAKU_MARINER)  # an enemy Unit is never "one of your Units"
    pilot = sc.add(0, MQUVE, Zone.HAND)
    command_pilot = sc.add(0, KAIS_RESOLVE, Zone.HAND)
    st = sc.start()
    first_pilot = st.cards[paired].pair
    assert _pair_targets(st, pilot) == {free}
    assert _pair_targets(st, command_pilot) == {free}
    play(st, pilot, onto=free)
    assert not any(o.kind is A.PAIR for o in options(st))
    assert zone_of(st, command_pilot) is Zone.HAND
    assert st.cards[paired].pair == first_pilot


@pytest.mark.rule("3-3-5")
@pytest.mark.faq("Q86", "Q87")
def test_paired_pilot_cannot_be_removed_or_exchanged() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    unit = sc.add(0, ZAKU_MARINER, pilot=RIDDHE)
    other = sc.add(0, DEMI_GARRISON)
    new_pilot = sc.add(0, MQUVE, Zone.HAND)
    st = sc.start()
    paired_pilot = st.cards[unit].pair
    assert not any(o.a == paired_pilot for o in options(st))  # no move of the paired Pilot
    assert _pair_targets(st, new_pilot) == {other}  # no swap onto the paired Unit


@pytest.mark.rule("3-3-6")
@pytest.mark.faq("Q41")
def test_pilot_follows_its_destroyed_unit_to_the_trash() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    enemy_unit = sc.add(1, ZAKU_MARINER, pilot=RIDDHE)
    cmd = sc.add(0, VETERANS_PRIDE, Zone.HAND)
    st = sc.start()
    enemy_pilot = st.cards[enemy_unit].pair
    play(st, cmd)
    assert zone_of(st, enemy_unit) is Zone.TRASH
    assert zone_of(st, enemy_pilot) is Zone.TRASH
    assert {enemy_unit, enemy_pilot} <= set(uids_in(st, 1, Zone.TRASH))
    assert not uids_in(st, 1, Zone.PAIRED)


@pytest.mark.rule("3-3-6")
@pytest.mark.faq("Q41")
def test_pilot_follows_its_unit_back_to_the_hand() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    enemy_unit = sc.add(1, ZAKU_MARINER, pilot=RIDDHE)  # 3 HP with the Pilot
    cmd = sc.add(0, HAWK, Zone.HAND)
    st = sc.start()
    enemy_pilot = st.cards[enemy_unit].pair
    play(st, cmd)
    assert zone_of(st, enemy_unit) is Zone.HAND
    assert zone_of(st, enemy_pilot) is Zone.HAND
    assert {enemy_unit, enemy_pilot} <= set(uids_in(st, 1, Zone.HAND))
    assert not uids_in(st, 1, Zone.PAIRED)


@pytest.mark.rule("3-3-6")
def test_pilot_follows_its_unit_destroyed_in_battle() -> None:
    sc = Scenario()
    attacker = sc.add(0, ZAKU_MARINER, pilot=RIDDHE)  # 3 AP / 3 HP
    target = sc.add(1, REZEL, rested=True)  # 4 AP / 3 HP
    st = sc.start()
    pilot = st.cards[attacker].pair
    attack(st, attacker, target)
    pass_all(st)
    assert zone_of(st, target) is Zone.TRASH
    assert zone_of(st, attacker) is Zone.TRASH
    assert zone_of(st, pilot) is Zone.TRASH


@pytest.mark.rule("3-3-7", "2-5-5")
def test_pilot_traits_are_not_added_to_the_unit() -> None:
    """GD01-092 (a Zeon Pilot): "While this Unit is (Zeon), it gains <Breach 1>." On a non-Zeon
    Unit the condition stays false because the Pilot's trait is not added to the Unit."""
    sc = Scenario()
    zeon_unit = sc.add(0, ZAKU_MARINER, pilot=MQUVE)
    federation_unit = sc.add(0, REZEL, pilot=MQUVE)
    st = sc.start()
    assert _traits(st, federation_unit) == ("Earth Federation",)
    assert _traits(st, zeon_unit) == ("Zeon",)
    assert keywords(st, zeon_unit).get("Breach") == 1
    assert "Breach" not in keywords(st, federation_unit)


@pytest.mark.rule("3-3-7", "2-5-5")
def test_trait_filters_do_not_see_the_pilot_trait() -> None:
    """GD01-113 "Choose 1 friendly (ZAFT) Unit. It gets AP+3": a non-ZAFT Unit paired with a
    ZAFT Pilot is not a legal target."""
    assert "ZAFT" in get_registry().db[ATHRUN].traits
    sc = Scenario()
    sc.resources(0, 3)
    unit = sc.add(0, ZAKU_MARINER, pilot=ATHRUN)
    cmd = sc.add(0, DESERT_TIGER, Zone.HAND)
    st = sc.start()
    assert "ZAFT" not in _traits(st, unit)
    assert not has_action(st, A.PLAY_COMMAND, cmd)

    sc = Scenario()
    sc.resources(0, 3)
    unit = sc.add(0, ZAKU_MARINER, pilot=ATHRUN)
    zaft_unit = sc.add(0, DINN)
    cmd = sc.add(0, DESERT_TIGER, Zone.HAND)
    st = sc.start()
    play(st, cmd)  # the only legal target is chosen automatically
    assert ap(st, zaft_unit) == 6
    assert ap(st, unit) == 3


@pytest.mark.rule("3-3-7")
def test_pilot_cards_have_traits() -> None:
    db = get_registry().db
    pilots = [c for c in db.real_cards() if c.card_type is CardType.PILOT]
    assert pilots and all(c.traits for c in pilots)


@pytest.mark.rule("3-3-8", "3-3-8-1", "2-7-3", "2-8-4")
@pytest.mark.parametrize(
    ("card", "mods"),
    [
        (RIDDHE, (1, 1)),
        (HEERO, (2, 1)),
        (GARROD, (1, 2)),
        (KAIS_RESOLVE, (1, 0)),
        (THOROUGHLY_DAMAGED, (0, 1)),
        (BLAZING_RIDER, (2, 1)),
    ],
)
def test_pilot_ap_and_hp_are_added_to_the_paired_unit(card: str, mods: tuple[int, int]) -> None:
    cd = get_registry().db[card]
    assert (cd.ap, cd.hp) == mods
    sc = Scenario()
    sc.resources(0, 6)
    unit = sc.add(0, ZAKU_MARINER)  # 2 AP / 2 HP, no link condition
    other = sc.add(0, ZAKU_MARINER)
    pilot = sc.add(0, card, Zone.HAND)
    st = sc.start()
    play(st, pilot, onto=unit)
    _decline_optional(st)
    assert (ap(st, unit), hp(st, unit)) == (2 + mods[0], 2 + mods[1])
    assert (ap(st, other), hp(st, other)) == (2, 2)


@pytest.mark.rule("3-3-8", "3-3-8-1", "2-7-3", "2-8-4")
def test_pilot_modifiers_count_in_battle() -> None:
    """Zaku Mariner (2/2) with Heero Yuy (+2/+1) destroys a 3-HP target and survives 2 damage."""
    sc = Scenario()
    attacker = sc.add(0, ZAKU_MARINER, pilot=HEERO)
    target = sc.add(1, CANCER, rested=True)  # 2 AP / 3 HP
    st = sc.start()
    attack(st, attacker, target)
    pass_all(st)
    assert zone_of(st, target) is Zone.TRASH
    assert zone_of(st, attacker) is Zone.BATTLE
    assert st.cards[attacker].damage == 2


@pytest.mark.rule("3-3-8")
def test_every_pilot_capable_card_has_ap_and_hp_modifiers() -> None:
    capable = [c for c in get_registry().db.real_cards() if c.is_pilot_capable]
    assert {c.card_type for c in capable} == {CardType.PILOT, CardType.COMMAND}
    assert all(c.ap >= 0 and c.hp >= 0 for c in capable)
    assert any(c.ap > 0 for c in capable) and any(c.hp > 0 for c in capable)


@pytest.mark.rule("3-3-9", "3-3-9-1", "3-3-9-2", "2-11-3")
def test_pilot_text_splits_into_pilot_text_and_unit_text() -> None:
    R = get_registry()
    for number in (CAGALLI, HEERO, AMURO, BANAGHER):
        entry = R.cards[R.db[number].def_id]
        assert entry.script is not None
        assert [type(a) for a in entry.script.abilities] == [d.Burst]
        assert entry.script.unit_abilities
        assert not any(isinstance(a, d.Burst) for a in entry.script.unit_abilities)


@pytest.mark.rule("3-3-9-1", "2-11-3")
def test_text_above_the_name_belongs_to_the_pilot_card() -> None:
    """The 【Burst】 above the name is the Pilot card's own: it works from the shield and adds the
    Pilot card itself to the hand; a Unit paired with that Pilot does not gain it."""
    sc = Scenario()
    unit = sc.add(0, ZAKU_MARINER)
    (shield,) = sc.shields(1, CAGALLI)
    paired = sc.add(0, DEMI_GARRISON, pilot=CAGALLI)
    st = sc.start()
    assert not any(isinstance(a.ability, d.Burst) for a in V.derived(st).abilities.get(paired, ()))
    attack(st, unit)
    pass_all(st)
    assert st.pending is not None and st.pending.kind is DecisionKind.BURST
    assert st.pending.player == 1
    yes(st)
    assert zone_of(st, shield) is Zone.HAND


@pytest.mark.rule("3-3-9-2", "2-11-3")
@pytest.mark.faq("Q88")
def test_text_below_the_name_is_gained_by_the_paired_unit() -> None:
    """GD01-096: "While this Unit is white, it gains <Blocker>." — "this Unit" is the Unit it is
    paired with; the same Pilot card in the hand grants nothing."""
    sc = Scenario(active=1)
    attacker = sc.add(1, ZAKU_MARINER)
    white_unit = sc.add(0, DEMI_GARRISON, pilot=CAGALLI)
    sc.add(0, CAGALLI, Zone.HAND)
    plain = sc.add(0, DEMI_GARRISON)
    st = sc.start()
    assert keywords(st, white_unit).get("Blocker") == 1
    assert "Blocker" not in keywords(st, plain)
    attack(st, attacker)
    assert st.pending is not None and st.pending.kind is DecisionKind.BLOCK
    assert {o.a for o in options(st) if o.kind is A.BLOCK} == {white_unit}
    block(st, white_unit)
    pass_all(st)
    assert st.cards[white_unit].damage == 2


@pytest.mark.rule("3-3-9-2", "2-11-3")
def test_triggered_pilot_text_triggers_for_the_paired_unit() -> None:
    """ST01-010 "【When Paired】Choose 1 enemy Unit with 5 or less HP. Rest it." and GD01-088
    "【When Linked】Draw 1." trigger as the paired Unit's effects when the Pilot is paired."""
    sc = Scenario()
    sc.resources(0, 5)
    unit = sc.add(0, ZAKU_MARINER)
    enemy = sc.add(1, ZAKU_MARINER)
    amuro = sc.add(0, AMURO, Zone.HAND)
    st = sc.start()
    play(st, amuro, onto=unit)
    assert st.cards[enemy].rested

    sc = Scenario()
    sc.resources(0, 5)
    unit = sc.add(0, FA_UNICORN)
    banagher = sc.add(0, BANAGHER, Zone.HAND)
    st = sc.start()
    hand_before = len(uids_in(st, 0, Zone.HAND))
    play(st, banagher, onto=unit)
    assert len(uids_in(st, 0, Zone.HAND)) == hand_before  # Pilot left the hand, 1 card drawn


@pytest.mark.rule("3-3-9-2")
def test_destroyed_pilot_text_is_the_paired_units_effect() -> None:
    """GD03-100 "【Destroyed】Choose 1 enemy Unit. It gets AP-3 during this turn." belongs to the
    Unit it is paired with: it triggers when that Unit is destroyed. The same Pilot card lying in
    the trash does nothing when an unpaired Unit is destroyed."""
    sc = Scenario()
    sc.resources(0, 2)
    sc.add(1, ZAKU_MARINER, pilot=SOMA)
    mine = sc.add(0, REZEL)
    cmd = sc.add(0, VETERANS_PRIDE, Zone.HAND)
    st = sc.start()
    play(st, cmd)
    assert ap(st, mine) == 1

    sc = Scenario()
    sc.resources(0, 2)
    sc.add(1, ZAKU_MARINER)
    sc.trash(1, SOMA)
    mine = sc.add(0, REZEL)
    cmd = sc.add(0, VETERANS_PRIDE, Zone.HAND)
    st = sc.start()
    play(st, cmd)
    assert ap(st, mine) == 4


@pytest.mark.rule("3-3-9-2", "3-3-8-1")
@pytest.mark.faq("Q89")
def test_unit_text_and_modifiers_apply_without_a_link() -> None:
    """Q89: a Pilot that does not satisfy the link condition still adds its AP/HP and its
    【Attack】 effect still activates. ST07-009 on Gundam [Amuro Ray]: 3/4 + 2/1, then AP+1."""
    sc = Scenario()
    unit = sc.add(0, GUNDAM, pilot=SETSUNA)
    sc.shields(1, ZAKU_MARINER, ZAKU_MARINER)
    st = sc.start()
    assert not _linked(st, unit)
    assert (ap(st, unit), hp(st, unit)) == (5, 5)
    attack(st, unit)
    pass_all(st)
    assert st.pending is not None and st.pending.kind is DecisionKind.MAIN
    assert ap(st, unit) == 6


@pytest.mark.rule("3-3-9-2")
def test_during_link_unit_text_applies_to_the_paired_unit() -> None:
    """ST02-010 "【During Link】This Unit gets AP+1 and HP+1." on Wing Gundam [Heero Yuy]."""
    sc = Scenario()
    linked = sc.add(0, WING_GUNDAM, pilot=HEERO)  # 4/3 + 2/1 + 1/1
    unlinked = sc.add(0, ZAKU_MARINER, pilot=HEERO)  # 2/2 + 2/1
    st = sc.start()
    assert (ap(st, linked), hp(st, linked)) == (7, 5)
    assert (ap(st, unlinked), hp(st, unlinked)) == (4, 3)


@pytest.mark.rule("3-3-9-2-1", "2-11-3")
def test_pilot_text_with_a_location_is_activated_by_the_pilot_card() -> None:
    """GD03-085: "When playing this card from your hand and pairing it with a Unit with
    "Gundam NT-1" in its card name, play this card as if it has 0 cost." The text names the
    hand, so the Pilot card applies it from there: with every Resource rested it can still be
    paired, but only with a "Gundam NT-1" Unit."""
    sc = Scenario()
    sc.resources(0, 3, rested=3)
    nt1 = sc.add(0, NT1_FULL_ARMOR)
    sc.add(0, ZAKU_MARINER)
    pilot = sc.add(0, CHRISTINA, Zone.HAND)
    st = sc.start()
    assert _pair_targets(st, pilot) == {nt1}
    act(st, A.PAIR, pilot, nt1)
    assert st.cards[nt1].pair == pilot


# =============================================================================================
# 2-4-3 color


@pytest.mark.rule("2-4-3")
@pytest.mark.faq("Q93")
def test_unit_color_is_not_affected_by_the_pilot_color() -> None:
    """A white Pilot does not make a red Unit white (GD01-096 grants <Blocker> only to a white
    Unit), and a blue Pilot does not make it blue (GD01-087 grants <Repair 1> only if blue)."""
    db = get_registry().db
    assert db[ZAKU_MARINER].color is not None and db[ZAKU_MARINER].color.value == "Red"
    sc = Scenario()
    red_with_white = sc.add(0, ZAKU_MARINER, pilot=CAGALLI)
    red_with_blue = sc.add(0, ZAKU_MARINER, pilot=SAYLA)
    blue_with_blue = sc.add(0, PISCES, pilot=SAYLA)
    st = sc.start()
    assert "Blocker" not in keywords(st, red_with_white)
    assert "Repair" not in keywords(st, red_with_blue)
    assert keywords(st, blue_with_blue).get("Repair") == 1


# =============================================================================================
# 3-4 Commands


@pytest.mark.rule("3-4-1", "3-4-3", "3-4-4")
def test_command_is_in_no_location_while_its_effect_resolves_then_goes_to_the_trash() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    first = sc.add(1, ZAKU_MARINER)
    second = sc.add(1, ZAKU_MARINER)
    cmd = sc.add(0, VETERANS_PRIDE, Zone.HAND)
    st = sc.start()
    play(st, cmd)
    assert st.pending is not None and st.pending.kind is DecisionKind.SELECT
    assert zone_of(st, cmd) is Zone.RESOLVING
    for z in (Zone.HAND, Zone.TRASH, Zone.BATTLE, Zone.PAIRED):
        assert cmd not in uids_in(st, 0, z)
    select(st, first)
    assert zone_of(st, first) is Zone.TRASH  # the command effect was activated
    assert zone_of(st, second) is Zone.BATTLE
    assert zone_of(st, cmd) is Zone.TRASH
    assert st.pending is not None and st.pending.kind is DecisionKind.MAIN


@pytest.mark.rule("3-4-3")
@pytest.mark.ruling("GD03-101:Q244")
def test_resolving_command_is_not_counted_in_the_trash() -> None:
    """GD03-101 counts cards named "A Healthy Curiosity" in the trash; the copy being resolved
    is not there, so a single copy already in the trash is not enough."""
    sc = Scenario()
    sc.resources(0, 3)
    sc.trash(0, HEALTHY_CURIOSITY)
    enemy = sc.add(1, ZAKU_MARINER)
    cmd = sc.add(0, HEALTHY_CURIOSITY, Zone.HAND)
    st = sc.start()
    play(st, cmd)
    assert not st.cards[enemy].rested
    assert zone_of(st, cmd) is Zone.TRASH

    sc = Scenario()
    sc.resources(0, 3)
    sc.trash(0, HEALTHY_CURIOSITY, HEALTHY_CURIOSITY)
    enemy = sc.add(1, ZAKU_MARINER)
    cmd = sc.add(0, HEALTHY_CURIOSITY, Zone.HAND)
    st = sc.start()
    play(st, cmd)
    assert st.cards[enemy].rested


@pytest.mark.rule("3-4-5")
def test_main_only_command_is_playable_in_the_main_phase_but_not_in_an_action_step() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    sc.add(0, DINN)
    sc.add(1, ZAKU_MARINER, rested=True)
    main_only = sc.add(0, THOROUGHLY_DAMAGED, Zone.HAND)
    main_or_action = sc.add(0, DESERT_TIGER, Zone.HAND)
    st = sc.start()
    assert has_action(st, A.PLAY_COMMAND, main_only)
    assert has_action(st, A.PLAY_COMMAND, main_or_action)
    end_main(st)
    assert st.pending is not None and st.pending.kind is DecisionKind.ACTION_STEP
    assert st.pending.player == 0
    assert has_action(st, A.PLAY_COMMAND, main_or_action)
    assert not has_action(st, A.PLAY_COMMAND, main_only)


@pytest.mark.rule("3-4-5")
def test_action_only_command_is_playable_in_action_steps_but_not_the_main_phase() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    sc.resources(1, 2)
    for p in (0, 1):
        sc.add(p, ZAKU_MARINER)
        sc.add(p, ZAKU_MARINER)
    mine = sc.add(0, TORRINGTON, Zone.HAND)
    theirs = sc.add(1, TORRINGTON, Zone.HAND)
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, mine)
    end_main(st)
    # the standby player acts first in the end-phase action step, in the opponent's turn
    assert st.pending is not None and st.pending.kind is DecisionKind.ACTION_STEP
    assert st.pending.player == 1
    assert has_action(st, A.PLAY_COMMAND, theirs)
    act(st, A.PASS)
    assert st.pending is not None and st.pending.player == 0
    assert has_action(st, A.PLAY_COMMAND, mine)


@pytest.mark.rule("3-4-5")
def test_main_and_action_command_is_playable_at_both_timings() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    dinn = sc.add(0, DINN)
    cmd, spare = sc.hand(0, DESERT_TIGER, DESERT_TIGER)
    st = sc.start()
    assert has_action(st, A.PLAY_COMMAND, cmd)
    end_main(st)
    assert st.pending is not None and st.pending.kind is DecisionKind.ACTION_STEP
    act(st, A.PLAY_COMMAND, cmd)
    # the spare copy keeps an action-step decision open after the first one resolves
    assert st.pending is not None and st.pending.kind is DecisionKind.ACTION_STEP
    assert has_action(st, A.PLAY_COMMAND, spare)
    assert ap(st, dinn) == 6
    assert zone_of(st, cmd) is Zone.TRASH


@pytest.mark.rule("3-4-6")
def test_some_command_cards_have_pilot_effects() -> None:
    commands = [c for c in get_registry().db.real_cards() if c.card_type is CardType.COMMAND]
    with_pilot = [c for c in commands if "【Pilot】" in c.effect]
    assert with_pilot and len(with_pilot) < len(commands)
    for c in commands:
        assert c.is_pilot_capable == (c in with_pilot)
        assert (c.pilot_name is not None) == (c in with_pilot)

    sc = Scenario()
    sc.resources(0, 4)
    unit = sc.add(0, ZAKU_MARINER)
    sc.add(1, ZAKU_MARINER, rested=True)
    with_effect = sc.add(0, THOROUGHLY_DAMAGED, Zone.HAND)
    without = sc.add(0, SIGNS_OF_REVOLUTION, Zone.HAND)
    st = sc.start()
    assert _kinds_for(st, with_effect) == {A.PLAY_COMMAND, A.PAIR}
    assert _pair_targets(st, with_effect) == {unit}
    assert _kinds_for(st, without) == {A.PLAY_COMMAND}


@pytest.mark.rule("3-4-6-1")
def test_pilot_effect_carries_traits_ap_hp_and_a_pilot_name() -> None:
    cd = get_registry().db[KAIS_RESOLVE]
    assert cd.effect.splitlines()[-1] == "【Pilot】[Kai Shiden]"
    assert cd.pilot_name == "Kai Shiden"
    assert cd.names == ("Kai's Resolve", "Kai Shiden")
    assert cd.traits == ("Earth Federation", "White Base Team")
    assert (cd.ap, cd.hp) == (1, 0)

    sc = Scenario()
    sc.resources(0, 3)
    unit = sc.add(0, GUNCANNON, Zone.HAND)  # link [Kai Shiden]
    card = sc.add(0, KAIS_RESOLVE, Zone.HAND)
    st = sc.start()
    play(st, unit)
    play(st, card, onto=unit)
    assert _linked(st, unit)  # the Pilot's name satisfies [Kai Shiden]
    assert (ap(st, unit), hp(st, unit)) == (3, 4)


@pytest.mark.rule("3-4-6-1", "3-4-6-2", "3-4-6-4")
def test_pilot_effect_traits_satisfy_a_trait_link_condition() -> None:
    """GD02-105 is an 【Action】-only Command with a (Zeon) 【Pilot】 effect. In the main phase it can
    still be paired, and as a Pilot it links Gelgoog (link: (Zeon)), which may then attack."""
    sc = Scenario()
    sc.resources(0, 4)
    unit = sc.add(0, GELGOOG, Zone.HAND)
    card = sc.add(0, VALEDICTORIAN, Zone.HAND)
    st = sc.start()
    play(st, unit)
    assert _kinds_for(st, card) == {A.PAIR}
    play(st, card, onto=unit)
    assert _linked(st, unit)
    assert has_action(st, A.ATTACK, unit, PLAYER_TARGET)


@pytest.mark.rule("3-4-6-2")
@pytest.mark.faq("Q23")
def test_command_with_pilot_effect_can_be_paired_instead_of_activated() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    unit = sc.add(0, ZAKU_MARINER)
    enemy = sc.add(1, ZAKU_MARINER, rested=True)
    card = sc.add(0, THOROUGHLY_DAMAGED, Zone.HAND)
    st = sc.start()
    assert has_action(st, A.PLAY_COMMAND, card)
    assert has_action(st, A.PAIR, card, unit)
    play(st, card, onto=unit)
    assert zone_of(st, card) is Zone.PAIRED
    assert st.cards[unit].pair == card
    assert st.cards[enemy].damage == 0  # the command effect was not activated
    assert _rested_resources(st, 0) == 1  # its cost was paid as usual


@pytest.mark.rule("3-4-6-3")
@pytest.mark.faq("Q163")
def test_paired_command_is_a_pilot() -> None:
    """GD04-099 "choose 1 enemy Pilot" can choose a Command card paired as a Pilot."""
    sc = Scenario()
    attacker = sc.add(0, AGRISSA, pilot=ALI)
    enemy_unit = sc.add(1, ZAKU_MARINER, pilot=KAIS_RESOLVE)
    sc.add(1, ZAKU_MARINER)
    sc.shields(1, ZAKU_MARINER)
    st = sc.start()
    paired_command = st.cards[enemy_unit].pair
    attack(st, attacker)
    assert {o.a for o in options(st) if o.kind is A.SELECT} == {paired_command}
    select(st, paired_command)
    assert zone_of(st, paired_command) is Zone.HAND
    assert zone_of(st, enemy_unit) is Zone.BATTLE


@pytest.mark.rule("3-4-6-3")
@pytest.mark.ruling("GD01-023:Q162")
@pytest.mark.faq("Q163")
def test_command_with_pilot_effect_in_the_trash_is_not_a_pilot_card() -> None:
    """GD01-023 chooses "1 (Newtype) Pilot card that is Lv.3 or lower from your trash"; a
    Command card with a (Newtype) 【Pilot】 effect in the trash is only a Command card."""
    assert "Newtype" in get_registry().db[VALEDICTORIAN].traits
    sc = Scenario()
    unit = sc.add(0, CHARS_GELGOOG)
    sc.add(0, ZAKU_MARINER, Zone.HAND)  # cost: discard 1 (Zeon) Unit card
    (command,) = sc.trash(0, VALEDICTORIAN)
    st = sc.start()
    # the only (Newtype) card in the trash is a Command card, so there is no Pilot-card target
    # and the ability cannot be activated (rule 10-2-2, FAQ Q100)
    assert not has_action(st, A.ACTIVATE, unit)
    assert st.cards[unit].pair < 0
    assert zone_of(st, command) is Zone.TRASH

    sc = Scenario()
    unit = sc.add(0, CHARS_GELGOOG)
    sc.add(0, ZAKU_MARINER, Zone.HAND)
    (command,) = sc.trash(0, VALEDICTORIAN)
    (pilot,) = sc.trash(0, LALAH)
    st = sc.start()
    act(st, A.ACTIVATE, unit)
    _decline_optional(st)
    assert st.cards[unit].pair == pilot
    assert zone_of(st, command) is Zone.TRASH


@pytest.mark.rule("3-4-6-3-1")
def test_paired_command_effect_activated_by_an_effect_resolves_as_a_command_effect() -> None:
    """GD05-044: "【During Link】【Attack】Activate 【Main】 on the card paired with this Unit."
    GD05-113's 【Main】 ("Choose 1 of your (MF) Units with 4 or less AP. It gets AP+2 during this
    turn.") resolves as a Command effect: no cost is paid (Q362) and it stays paired (Q363)."""
    sc = Scenario()
    sc.resources(0, 3)
    rose = sc.add(0, GUNDAM_ROSE, pilot=ROSE_SCREAMER)  # 3/3 + 1/1, linked
    sc.shields(1, ZAKU_MARINER, ZAKU_MARINER)
    st = sc.start()
    card = st.cards[rose].pair
    assert ap(st, rose) == 4
    attack(st, rose)
    _decline_optional(st)
    pass_all(st)
    assert ap(st, rose) == 6
    assert st.cards[rose].pair == card and zone_of(st, card) is Zone.PAIRED
    assert _rested_resources(st, 0) == 0


@pytest.mark.rule("3-4-6-4", "3-3-4")
def test_rules_for_pilots_apply_to_a_paired_command() -> None:
    """Paired as a Pilot, a Command card adds its AP/HP, satisfies the link condition with its
    Pilot name, and is the one Pilot its Unit may have."""
    sc = Scenario()
    sc.resources(0, 6)
    unit = sc.add(0, GUNDAM_X, Zone.HAND)
    card = sc.add(0, BLAZING_RIDER, Zone.HAND)
    other_pilot = sc.add(0, JAMIL, Zone.HAND)
    target = sc.add(1, REZEL, rested=True)  # 4 AP / 3 HP
    st = sc.start()
    play(st, unit)
    play(st, card, onto=unit)
    assert (ap(st, unit), hp(st, unit)) == (5, 5)
    assert _linked(st, unit)
    assert not has_action(st, A.PAIR, other_pilot, unit)
    assert has_action(st, A.ATTACK, unit, target)
    attack(st, unit, target)
    pass_all(st)
    assert zone_of(st, target) is Zone.TRASH
    assert st.cards[unit].damage == 4
    assert zone_of(st, unit) is Zone.BATTLE  # 5 HP including the +1 modifier


@pytest.mark.rule("3-4-6-4", "3-3-6")
def test_paired_command_follows_its_destroyed_unit_to_the_trash() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    enemy_unit = sc.add(1, ZAKU_MARINER, pilot=KAIS_RESOLVE)
    cmd = sc.add(0, VETERANS_PRIDE, Zone.HAND)
    st = sc.start()
    paired_command = st.cards[enemy_unit].pair
    play(st, cmd)
    assert zone_of(st, enemy_unit) is Zone.TRASH
    assert zone_of(st, paired_command) is Zone.TRASH


@pytest.mark.rule("3-4-7")
def test_command_burst_activates_when_it_is_a_destroyed_shield() -> None:
    sc = Scenario()
    unit = sc.add(0, ZAKU_MARINER)
    (shield,) = sc.shields(1, SIGNS_OF_REVOLUTION)
    st = sc.start()
    hand_before = len(uids_in(st, 1, Zone.HAND))
    attack(st, unit)
    pass_all(st)
    assert st.pending is not None and st.pending.kind is DecisionKind.BURST
    assert st.pending.player == 1
    yes(st)
    assert len(uids_in(st, 1, Zone.HAND)) == hand_before + 1  # 【Burst】Draw 1.
    assert zone_of(st, shield) is Zone.TRASH


# =============================================================================================
# 3-5 Bases


@pytest.mark.rule("3-5-1")
def test_played_base_card_is_deployed_into_the_base_section() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    sc.shields(0, ZAKU_MARINER, ZAKU_MARINER)
    base = sc.add(0, DESERT_BASE, Zone.HAND)
    st = sc.start()
    play(st, base)
    assert zone_of(st, base) is Zone.BASE
    assert uids_in(st, 0, Zone.BASE) == [base]
    assert base not in uids_in(st, 0, Zone.BATTLE)
    assert len(uids_in(st, 0, Zone.SHIELD)) == 1  # its 【Deploy】 resolved: it is in play


@pytest.mark.rule("3-5-3")
def test_battle_damage_to_the_shield_area_goes_to_the_base() -> None:
    sc = Scenario()
    attacker = sc.add(0, ZAKU_MARINER)
    shields = sc.shields(1, ZAKU_MARINER, ZAKU_MARINER)
    base = sc.base(1, DESERT_BASE)
    st = sc.start()
    attack(st, attacker)
    pass_all(st)
    assert st.cards[base].damage == 2
    assert uids_in(st, 1, Zone.SHIELD) == shields


@pytest.mark.rule("3-5-3")
def test_effect_damage_to_the_shield_area_goes_to_the_base() -> None:
    """<Breach 1> (gained from GD01-092 on a Zeon Unit) damages the Base, not a Shield."""
    sc = Scenario()
    attacker = sc.add(0, ZAKU_MARINER, pilot=MQUVE)  # 3/3, <Breach 1>
    target = sc.add(1, ZAKU_MARINER, rested=True)
    shields = sc.shields(1, ZAKU_MARINER, ZAKU_MARINER)
    base = sc.base(1, DESERT_BASE)
    st = sc.start()
    attack(st, attacker, target)
    pass_all(st)
    assert zone_of(st, target) is Zone.TRASH
    assert st.cards[base].damage == 1
    assert uids_in(st, 1, Zone.SHIELD) == shields


@pytest.mark.rule("3-5-4", "3-5-4-1", "3-5-4-2")
def test_base_with_zero_ap_is_destroyed_when_its_hp_reaches_zero() -> None:
    """Bases have AP and HP. With 0 AP the Base deals no battle damage back; 4 damage on 4 HP
    destroys it."""
    sc = Scenario()
    attacker = sc.add(0, REZEL)  # 4 AP
    shields = sc.shields(1, ZAKU_MARINER)
    base = sc.base(1, KUSANAGI)  # 0 AP / 4 HP
    st = sc.start()
    assert (ap(st, base), hp(st, base)) == (0, 4)
    attack(st, attacker)
    pass_all(st)
    assert zone_of(st, base) is Zone.TRASH
    assert st.cards[attacker].damage == 0
    assert uids_in(st, 1, Zone.SHIELD) == shields


@pytest.mark.rule("3-5-4-2")
def test_base_survives_damage_below_its_hp() -> None:
    sc = Scenario()
    attacker = sc.add(0, ZAKU_MARINER)
    base = sc.base(1, KUSANAGI, damage=1)
    st = sc.start()
    attack(st, attacker)
    pass_all(st)
    assert st.cards[base].damage == 3
    assert zone_of(st, base) is Zone.BASE


# =============================================================================================
# 3-6 Resources


@pytest.mark.rule("3-6-1")
def test_resource_is_placed_from_the_resource_deck_into_the_resource_area() -> None:
    sc = Scenario(active=1)
    sc.resource_deck(0, 3)
    lv2 = sc.add(0, ZAKU_MARINER, Zone.HAND)
    lv1 = sc.add(0, PISCES, Zone.HAND)
    st = sc.start()
    top = uids_in(st, 0, Zone.RESOURCE_DECK)[0]
    to_next_turn(st)
    assert st.active == 0 and st.step is Step.MAIN
    assert uids_in(st, 0, Zone.RESOURCE_AREA) == [top]
    assert len(uids_in(st, 0, Zone.RESOURCE_DECK)) == 2
    # Resource cards still in the resource deck are not Resources: Lv. counts only the area
    assert has_action(st, A.PLAY_UNIT, lv1)
    assert not has_action(st, A.PLAY_UNIT, lv2)


@pytest.mark.rule("3-6-1")
def test_effect_places_a_resource_directly_from_the_resource_deck() -> None:
    """GD01-107 "【Main】Place 1 rested Resource." takes the top card of the resource deck."""
    sc = Scenario()
    sc.resources(0, 3)
    sc.resource_deck(0, 2)
    cmd = sc.add(0, FIRST_CONTACT, Zone.HAND)
    st = sc.start()
    top = uids_in(st, 0, Zone.RESOURCE_DECK)[0]
    play(st, cmd)
    assert zone_of(st, top) is Zone.RESOURCE_AREA
    assert st.cards[top].rested
    assert len(uids_in(st, 0, Zone.RESOURCE_AREA)) == 4
    assert len(uids_in(st, 0, Zone.RESOURCE_DECK)) == 1
    assert top not in uids_in(st, 0, Zone.HAND)


# =============================================================================================
# 5-9 Pair


@pytest.mark.rule("5-9-1")
@pytest.mark.parametrize("card", [RIDDHE, KAIS_RESOLVE])
def test_pairing_a_pilot_or_a_pilot_command_is_a_pair(card: str) -> None:
    """GD01-001 "【When Paired】If you have 2 or more other Units in play, draw 1." triggers for a
    Pilot card and for a Command card with a 【Pilot】 effect placed beneath it."""
    sc = Scenario()
    sc.resources(0, 3)
    unit = sc.add(0, GUNDAM_WHEN_PAIRED)
    sc.add(0, ZAKU_MARINER)
    sc.add(0, ZAKU_MARINER)
    pilot = sc.add(0, card, Zone.HAND)
    st = sc.start()
    hand_before = len(uids_in(st, 0, Zone.HAND))
    play(st, pilot, onto=unit)
    assert zone_of(st, pilot) is Zone.PAIRED and st.cards[unit].pair == pilot
    assert len(uids_in(st, 0, Zone.HAND)) == hand_before  # card left the hand, 1 drawn


@pytest.mark.rule("5-9-1")
def test_playing_a_pilot_command_as_a_command_is_not_a_pair() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    unit = sc.add(0, GUNDAM_WHEN_PAIRED, damage=1)
    sc.add(0, ZAKU_MARINER)
    sc.add(0, ZAKU_MARINER)
    card = sc.add(0, KAIS_RESOLVE, Zone.HAND)
    st = sc.start()
    hand_before = len(uids_in(st, 0, Zone.HAND))
    play(st, card)
    assert st.pending is not None and st.pending.kind is DecisionKind.SELECT
    select(st, unit)
    assert st.cards[unit].damage == 0  # the command effect resolved instead
    assert st.cards[unit].pair < 0
    assert zone_of(st, card) is Zone.TRASH
    assert len(uids_in(st, 0, Zone.HAND)) == hand_before - 1  # no 【When Paired】 draw
