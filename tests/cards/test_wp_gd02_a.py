"""Behaviour tests for GD02-001..GD02-085 (work package WP-GD02-A)."""

from __future__ import annotations

import pytest

from gcg_sim.engine import core
from gcg_sim.engine import view as V
from gcg_sim.engine.state import GameState
from gcg_sim.engine.types import PLAYER_TARGET, ActionKind, DecisionKind, Zone
from gcg_sim.testkit import (
    Scenario,
    activate,
    ap,
    attack,
    block,
    end_main,
    has_action,
    hp,
    keywords,
    no,
    numbers_in,
    order,
    pass_,
    play,
    select,
    to_next_turn,
    yes,
    zone_of,
)

A = ActionKind
VANILLA = "GD01-060"  # Zaku Mariner, Red Lv2 2/2, no effects
VANILLA_3HP = "GD02-015"  # Marasai, Blue Lv3 3/3 (Titans), no effects
BIG = "GD01-031"  # Gelgoog, Green Lv4 4/3, no effects
LV5_UNIT = "GD04-027"  # Bigro, Green Lv5 5/4, no effects
BLOCKER_3HP = "GD02-079"  # Rick Dias, White Lv3 3/3 <Blocker>
PILOT_LV3 = "GD01-089"  # Riddhe Marcenas, Blue Lv3 Pilot +1/+1 (Earth Federation)
PILOT_LV4 = "GD01-096"  # Cagalli Yula Athha, White Lv4 Pilot +1/+1 (Orb)
DAMAGE_1 = "GD01-115"  # 【Main】/【Action】Choose 1 enemy Unit. Deal 1 damage to it.
AP_MINUS_3 = "ST01-014"  # 【Main】/【Action】Choose 1 enemy Unit. It gets AP-3 during this turn.
RECOVER_3 = "ST01-013"  # 【Main】Choose 1 friendly Unit. It recovers 3 HP.
MIDAIR = "GD01-121"  # 【Burst】Activate 【Main】; 【Main】set 1 rested <Blocker> Unit active.
WHITE_BASE = "GD02-129"  # Argama, White Base
LINK_WATCH_BASE = "ST14-016"  # Gryphios 2, White Base: when a friendly Unit links, AP-1
ENEMY_TOKEN = "T-007"  # [Zaku Ⅱ]((Zeon)･AP1･HP1) Unit token
CYBER_NEWTYPE = "GD04-082"  # Rosamia Badam: (Titans)(Cyber-Newtype) Pilot +2/+0, no link here


def hand_size(st: GameState, player: int = 0) -> int:
    return len(st.zones[player][Zone.HAND])


def pending_kind(st: GameState) -> DecisionKind | None:
    return st.pending.kind if st.pending is not None else None


def settle(st: GameState) -> None:
    """Pass action steps, decline blocks and resolve simultaneous triggers in listed order
    until another kind of decision (or the end of the game) is reached."""
    for _ in range(100):
        kind = pending_kind(st)
        if kind is DecisionKind.ACTION_STEP:
            pass_(st)
        elif kind is DecisionKind.BLOCK:
            block(st, None)
        elif kind is DecisionKind.ORDER_TRIGGER:
            order(st)
        else:
            return


def pair_unchecked(sc: Scenario, player: int, unit: int, pilot_number: str) -> int:
    """Pair a Pilot card whose own text is not implemented in this package (only its name and
    stats matter to the test)."""
    pilot = core.new_card(sc.st, sc.db[pilot_number].def_id, player, Zone.PAIRED)
    sc.st.cards[pilot].pair = unit
    sc.st.cards[unit].pair = pilot
    sc.st.touch()
    return pilot


def fill_trash(sc: Scenario, player: int, n: int, number: str = VANILLA) -> None:
    sc.trash(player, *([number] * n))


# ---------------------------------------------------------------------------------------------
# GD02-001 Psycho Gundam


@pytest.mark.card("GD02-001")
@pytest.mark.ruling("GD02-001:Q171")
def test_gd02_001_recovers_when_it_destroys_a_shield() -> None:
    sc = Scenario()
    psycho = sc.add(0, "GD02-001", damage=3, pilot=CYBER_NEWTYPE)
    top, _ = sc.shields(1, VANILLA, VANILLA)
    st = sc.start()
    attack(st, psycho)
    settle(st)
    assert zone_of(st, top) is Zone.TRASH
    assert st.cards[psycho].damage == 1


@pytest.mark.card("GD02-001")
@pytest.mark.ruling("GD02-001:Q171")
def test_gd02_001_recovers_when_it_destroys_the_enemy_base() -> None:
    sc = Scenario()
    psycho = sc.add(0, "GD02-001", damage=3, pilot=CYBER_NEWTYPE)
    base = sc.base(1)
    sc.shields(1, VANILLA)
    st = sc.start()
    attack(st, psycho)
    settle(st)
    assert zone_of(st, base) is Zone.OUTSIDE  # the EX Base token left the game
    assert st.cards[psycho].damage == 1


@pytest.mark.card("GD02-001")
def test_gd02_001_another_titans_unit_destroying_a_shield_also_counts() -> None:
    sc = Scenario()
    psycho = sc.add(0, "GD02-001", damage=3, pilot=CYBER_NEWTYPE)
    hizack = sc.add(0, "GD02-013")
    sc.shields(1, VANILLA, VANILLA)
    st = sc.start()
    attack(st, hizack)
    settle(st)
    assert st.cards[psycho].damage == 1


@pytest.mark.card("GD02-001")
def test_gd02_001_non_titans_attacker_does_not_count() -> None:
    sc = Scenario()
    psycho = sc.add(0, "GD02-001", damage=3, pilot=CYBER_NEWTYPE)
    other = sc.add(0, VANILLA)
    sc.shields(1, VANILLA, VANILLA)
    st = sc.start()
    attack(st, other)
    settle(st)
    assert st.cards[psycho].damage == 3


@pytest.mark.card("GD02-001")
def test_gd02_001_needs_a_cyber_newtype_pilot() -> None:
    sc = Scenario()
    psycho = sc.add(0, "GD02-001", damage=3, pilot="GD02-086")  # Jerid Messa (Titans)
    sc.shields(1, VANILLA, VANILLA)
    st = sc.start()
    attack(st, psycho)
    settle(st)
    assert st.cards[psycho].damage == 3


@pytest.mark.card("GD02-001")
@pytest.mark.rule("13-1-2")
def test_gd02_001_breach_destroying_a_shield_is_destruction_with_damage() -> None:
    sc = Scenario()
    psycho = sc.add(0, "GD02-001", damage=1, pilot=CYBER_NEWTYPE)
    victim = sc.add(1, VANILLA, rested=True)
    top, _ = sc.shields(1, VANILLA, VANILLA)
    st = sc.start()
    assert keywords(st, psycho).get("Breach") == 3
    attack(st, psycho, victim)
    settle(st)
    assert zone_of(st, victim) is Zone.TRASH
    assert zone_of(st, top) is Zone.TRASH
    assert st.cards[psycho].damage == 1  # 1 + 2 battle damage, then recovered 2


@pytest.mark.card("GD02-001")
@pytest.mark.ruling("GD02-001:Q171")
@pytest.mark.xfail(
    strict=True,
    reason="ENGINE: a Base destroyed by effect damage (rules management) emits no "
    "destroys_shield_card event for the damage source",
)
def test_gd02_001_breach_destroying_the_base_recovers() -> None:
    sc = Scenario()
    psycho = sc.add(0, "GD02-001", damage=1, pilot=CYBER_NEWTYPE)
    victim = sc.add(1, VANILLA, rested=True)
    base = sc.base(1)
    st = sc.start()
    attack(st, psycho, victim)
    settle(st)
    assert zone_of(st, base) is Zone.OUTSIDE
    assert st.cards[psycho].damage == 1


# ---------------------------------------------------------------------------------------------
# GD02-002 Gundam Epyon


@pytest.mark.card("GD02-002")
def test_gd02_002_set_active_after_destroying_a_unit_in_battle() -> None:
    sc = Scenario()
    epyon = sc.add(0, "GD02-002", pilot="ST02-011")  # Zechs Merquise: linked
    victim = sc.add(1, VANILLA, rested=True)
    st = sc.start()
    attack(st, epyon, victim)
    settle(st)
    assert zone_of(st, victim) is Zone.TRASH
    assert not st.cards[epyon].rested


@pytest.mark.card("GD02-002")
def test_gd02_002_requires_link() -> None:
    sc = Scenario()
    epyon = sc.add(0, "GD02-002", pilot=PILOT_LV3)
    victim = sc.add(1, VANILLA, rested=True)
    st = sc.start()
    attack(st, epyon, victim)
    settle(st)
    assert zone_of(st, victim) is Zone.TRASH
    assert st.cards[epyon].rested


@pytest.mark.card("GD02-002")
@pytest.mark.ruling("GD02-002:Q197")
def test_gd02_002_q197_already_active_does_not_use_once_per_turn() -> None:
    sc = Scenario()
    epyon = sc.add(0, "GD02-002", pilot="ST02-011")
    ally = sc.add(0, "GD02-051")  # 01 Gundam 2/3
    first = sc.add(1, VANILLA, rested=True)
    second = sc.add(1, VANILLA, rested=True)
    st = sc.start()
    attack(st, ally, first)
    settle(st)
    assert zone_of(st, first) is Zone.TRASH
    assert not st.cards[epyon].rested
    attack(st, epyon, second)
    settle(st)
    assert zone_of(st, second) is Zone.TRASH
    assert not st.cards[epyon].rested  # the 【Once per Turn】 use was still available


@pytest.mark.card("GD02-002")
def test_gd02_002_once_per_turn() -> None:
    sc = Scenario()
    epyon = sc.add(0, "GD02-002", pilot="ST02-011")
    first = sc.add(1, VANILLA, rested=True)
    second = sc.add(1, VANILLA, rested=True)
    st = sc.start()
    attack(st, epyon, first)
    settle(st)
    assert not st.cards[epyon].rested
    attack(st, epyon, second)
    settle(st)
    assert zone_of(st, second) is Zone.TRASH
    assert st.cards[epyon].rested


# ---------------------------------------------------------------------------------------------
# GD02-003 Gundam Mk-II (Titans)


def _mk2_trade(pilot: str, hand: tuple[str, ...]) -> tuple[GameState, int, int]:
    sc = Scenario()
    mk2 = sc.add(0, "GD02-003", pilot=pilot)
    pilot_uid = sc.st.cards[mk2].pair
    victim = sc.add(1, BIG, rested=True)
    sc.hand(0, *hand)
    st = sc.start()
    attack(st, mk2, victim)
    settle(st)
    assert zone_of(st, mk2) is Zone.TRASH
    return st, mk2, pilot_uid


@pytest.mark.card("GD02-003")
def test_gd02_003_discard_unit_card_returns_pilot() -> None:
    st, _, pilot = _mk2_trade(PILOT_LV3, (VANILLA,))
    assert pending_kind(st) is DecisionKind.YES_NO
    yes(st)
    assert zone_of(st, pilot) is Zone.HAND
    assert numbers_in(st, 0, Zone.HAND) == [PILOT_LV3]


@pytest.mark.card("GD02-003")
def test_gd02_003_declining_keeps_pilot_in_trash() -> None:
    st, _, pilot = _mk2_trade(PILOT_LV3, (VANILLA,))
    no(st)
    assert zone_of(st, pilot) is Zone.TRASH
    assert numbers_in(st, 0, Zone.HAND) == [VANILLA]


@pytest.mark.card("GD02-003")
def test_gd02_003_no_unit_card_to_discard_returns_nothing() -> None:
    st, _, pilot = _mk2_trade(PILOT_LV3, (DAMAGE_1,))
    yes(st)
    assert zone_of(st, pilot) is Zone.TRASH
    assert numbers_in(st, 0, Zone.HAND) == [DAMAGE_1]


@pytest.mark.card("GD02-003")
def test_gd02_003_lv4_pilot_does_not_trigger() -> None:
    st, _, pilot = _mk2_trade(PILOT_LV4, (VANILLA,))
    assert pending_kind(st) is DecisionKind.MAIN
    assert zone_of(st, pilot) is Zone.TRASH


# ---------------------------------------------------------------------------------------------
# GD02-004 Byarlant


def _byarlant(target: str = BLOCKER_3HP) -> tuple[Scenario, int, int, int]:
    sc = Scenario()
    sc.resources(0, 3)
    sc.resources(1, 3)
    byarlant = sc.add(0, "GD02-004")
    pilot = sc.add(0, PILOT_LV3, Zone.HAND)
    enemy = sc.add(1, target, rested=True)
    return sc, byarlant, pilot, enemy


@pytest.mark.card("GD02-004")
def test_gd02_004_chosen_unit_stays_rested_in_opponents_start_phase() -> None:
    sc, byarlant, pilot, enemy = _byarlant()
    other = sc.add(1, VANILLA, rested=True)
    st = sc.start()
    play(st, pilot, onto=byarlant)
    select(st, enemy)
    to_next_turn(st)
    assert st.active == 1
    assert st.cards[enemy].rested
    assert not st.cards[other].rested
    to_next_turn(st)
    to_next_turn(st)
    assert st.active == 1
    assert not st.cards[enemy].rested  # only the next turn's start phase


@pytest.mark.card("GD02-004")
def test_gd02_004_only_rested_units_with_3_or_less_hp() -> None:
    sc, byarlant, pilot, enemy = _byarlant(target=LV5_UNIT)
    st = sc.start()
    play(st, pilot, onto=byarlant)
    to_next_turn(st)
    assert not st.cards[enemy].rested


@pytest.mark.card("GD02-004")
@pytest.mark.ruling("GD02-004:Q172")
def test_gd02_004_q172_effect_in_main_phase_can_set_it_active() -> None:
    sc, byarlant, pilot, enemy = _byarlant()
    midair = sc.add(1, MIDAIR, Zone.HAND)
    st = sc.start()
    play(st, pilot, onto=byarlant)
    to_next_turn(st)
    assert st.cards[enemy].rested
    play(st, midair)
    assert not st.cards[enemy].rested


@pytest.mark.card("GD02-004")
@pytest.mark.ruling("GD02-004:Q172")
def test_gd02_004_q172_burst_activated_main_sets_it_active() -> None:
    sc, byarlant, pilot, enemy = _byarlant()
    attacker = sc.add(0, VANILLA)
    sc.shields(1, MIDAIR)
    st = sc.start()
    play(st, pilot, onto=byarlant)
    assert st.cards[enemy].rested
    attack(st, attacker)
    settle(st)
    assert pending_kind(st) is DecisionKind.BURST
    yes(st)
    assert not st.cards[enemy].rested


# ---------------------------------------------------------------------------------------------
# GD02-005 Tallgeese


@pytest.mark.card("GD02-005")
def test_gd02_005_linked_attack_rests_enemy_with_2_or_less_hp() -> None:
    sc = Scenario()
    tallgeese = sc.add(0, "GD02-005", pilot="ST02-011")  # Zechs Merquise (OZ): linked
    small = sc.add(1, VANILLA)
    big = sc.add(1, VANILLA_3HP)
    sc.shields(1, VANILLA)
    st = sc.start()
    attack(st, tallgeese)
    assert st.cards[small].rested
    assert not st.cards[big].rested


@pytest.mark.card("GD02-005")
def test_gd02_005_not_linked_does_nothing() -> None:
    sc = Scenario()
    tallgeese = sc.add(0, "GD02-005", pilot=PILOT_LV3)
    small = sc.add(1, VANILLA)
    sc.shields(1, VANILLA)
    st = sc.start()
    attack(st, tallgeese)
    assert not st.cards[small].rested


# ---------------------------------------------------------------------------------------------
# GD02-006 Forbidden Gundam


@pytest.mark.card("GD02-006")
def test_gd02_006_no_battle_damage_from_lv2_units_during_your_turn() -> None:
    sc = Scenario()
    forbidden = sc.add(0, "GD02-006")
    lv2 = sc.add(1, VANILLA, rested=True)
    st = sc.start()
    assert "Blocker" in keywords(st, forbidden)
    attack(st, forbidden, lv2)
    settle(st)
    assert zone_of(st, lv2) is Zone.TRASH
    assert st.cards[forbidden].damage == 0


@pytest.mark.card("GD02-006")
def test_gd02_006_lv3_units_still_deal_battle_damage() -> None:
    sc = Scenario()
    forbidden = sc.add(0, "GD02-006")
    lv3 = sc.add(1, VANILLA_3HP, rested=True)
    st = sc.start()
    attack(st, forbidden, lv3)
    settle(st)
    assert st.cards[forbidden].damage == 3


@pytest.mark.card("GD02-006")
def test_gd02_006_not_during_opponents_turn() -> None:
    sc = Scenario(active=1)
    forbidden = sc.add(0, "GD02-006", rested=True)
    lv2 = sc.add(1, VANILLA)
    st = sc.start()
    attack(st, lv2, forbidden)
    settle(st)
    assert st.cards[forbidden].damage == 2


# ---------------------------------------------------------------------------------------------
# GD02-007 Psycho Gundam (MA Mode), GD02-017 Delta Plus (Waverider Mode)


@pytest.mark.card("GD02-007", "GD02-017")
@pytest.mark.rule("13-1-1-1")
@pytest.mark.parametrize("number", ["GD02-007", "GD02-017"])
def test_repair_2_recovers_at_end_of_turn(number: str) -> None:
    sc = Scenario()
    unit = sc.add(0, number, damage=2)
    st = sc.start()
    assert keywords(st, unit) == {"Repair": 2}
    to_next_turn(st)
    assert st.cards[unit].damage == 0


# ---------------------------------------------------------------------------------------------
# GD02-008 Gabthley


@pytest.mark.card("GD02-008")
def test_gd02_008_when_linked_damages_rested_enemy() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    gabthley = sc.add(0, "GD02-008")
    jerid = sc.add(0, "GD02-086", Zone.HAND)  # Jerid Messa (Titans)
    rested = sc.add(1, VANILLA, rested=True)
    active = sc.add(1, VANILLA)
    st = sc.start()
    play(st, jerid, onto=gabthley)
    assert st.cards[rested].damage == 1
    assert st.cards[active].damage == 0


@pytest.mark.card("GD02-008")
def test_gd02_008_pairing_without_link_does_nothing() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    gabthley = sc.add(0, "GD02-008")
    pilot = sc.add(0, PILOT_LV3, Zone.HAND)
    rested = sc.add(1, VANILLA, rested=True)
    st = sc.start()
    play(st, pilot, onto=gabthley)
    assert st.cards[rested].damage == 0


# ---------------------------------------------------------------------------------------------
# GD02-009 Calamity Gundam


def _calamity(rested_enemy: bool = True) -> tuple[Scenario, int, int]:
    sc = Scenario(active=1)
    sc.resources(1, 6)
    sc.shields(0, VANILLA)
    calamity = sc.add(0, "GD02-009", pilot=PILOT_LV3)  # AP 4: two AP-3 effects both reduce it
    enemy = sc.add(1, VANILLA_3HP, rested=rested_enemy)
    sc.hand(1, AP_MINUS_3, AP_MINUS_3)
    return sc, calamity, enemy


@pytest.mark.card("GD02-009")
def test_gd02_009_enemy_ap_reduction_deals_2_to_rested_enemy() -> None:
    sc, calamity, enemy = _calamity()
    st = sc.start()
    play(st, st.zones[1][Zone.HAND][0])
    assert ap(st, calamity) == 1
    assert st.cards[enemy].damage == 2


@pytest.mark.card("GD02-009")
def test_gd02_009_once_per_turn() -> None:
    sc, _, enemy = _calamity()
    st = sc.start()
    play(st, st.zones[1][Zone.HAND][0])
    play(st, st.zones[1][Zone.HAND][0])
    assert st.cards[enemy].damage == 2


@pytest.mark.card("GD02-009")
@pytest.mark.rule("10-3-3-1", "13-2-13-1")
def test_gd02_009_without_target_it_does_not_activate_or_use_once_per_turn() -> None:
    sc, _, enemy = _calamity(rested_enemy=False)
    st = sc.start()
    play(st, st.zones[1][Zone.HAND][0])
    assert st.cards[enemy].damage == 0
    attack(st, enemy)  # the attacker is now a rested enemy Unit
    assert st.pending is not None and st.pending.player == 1
    play(st, st.zones[1][Zone.HAND][0])
    assert st.cards[enemy].damage == 2


# ---------------------------------------------------------------------------------------------
# GD02-010 Raider Gundam


@pytest.mark.card("GD02-010")
def test_gd02_010_draws_once_when_receiving_enemy_effect_damage() -> None:
    sc = Scenario(active=1)
    sc.resources(1, 2)
    raider = sc.add(0, "GD02-010")
    sc.hand(1, DAMAGE_1, DAMAGE_1)
    st = sc.start()
    before = hand_size(st, 0)
    play(st, st.zones[1][Zone.HAND][0])
    assert st.cards[raider].damage == 1
    assert hand_size(st, 0) == before + 1
    play(st, st.zones[1][Zone.HAND][0])
    assert st.cards[raider].damage == 2
    assert hand_size(st, 0) == before + 1


@pytest.mark.card("GD02-010")
def test_gd02_010_battle_damage_does_not_draw() -> None:
    sc = Scenario(active=1)
    raider = sc.add(0, "GD02-010", rested=True)
    enemy = sc.add(1, VANILLA)
    st = sc.start()
    before = hand_size(st, 0)
    attack(st, enemy, raider)
    settle(st)
    assert st.cards[raider].damage == 2
    assert hand_size(st, 0) == before


@pytest.mark.card("GD02-010")
def test_gd02_010_own_effect_damage_does_not_draw() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    raider = sc.add(0, "GD02-010")
    barbatos = sc.add(0, "GD02-068", Zone.HAND)  # 【Deploy】Deal 2 damage to this Unit.
    st = sc.start()
    before = hand_size(st, 0)
    play(st, barbatos)
    assert st.cards[barbatos].damage == 2
    assert st.cards[raider].damage == 0
    assert hand_size(st, 0) == before - 1


# ---------------------------------------------------------------------------------------------
# GD02-011 Moebius (Peacemaker Team)


def _moebius_attack(*, base: bool) -> tuple[GameState, int, list[int], int]:
    sc = Scenario()
    moebius = sc.add(0, "GD02-011")
    shields = sc.shields(1, VANILLA, VANILLA)
    base_uid = sc.base(1) if base else -1
    st = sc.start()
    attack(st, moebius)
    return st, moebius, shields, base_uid


@pytest.mark.card("GD02-011")
def test_gd02_011_destroys_itself_to_deal_6_to_the_battled_shield() -> None:
    st, moebius, (top, second), _ = _moebius_attack(base=False)
    assert pending_kind(st) is DecisionKind.ACTION_STEP and st.pending is not None
    assert st.pending.player == 0
    activate(st, moebius)
    assert zone_of(st, moebius) is Zone.TRASH
    assert zone_of(st, top) is Zone.TRASH
    assert zone_of(st, second) is Zone.SHIELD
    settle(st)
    assert zone_of(st, second) is Zone.SHIELD  # the battle ended with its attacker


@pytest.mark.card("GD02-011")
def test_gd02_011_battled_base_takes_the_damage() -> None:
    st, moebius, (top, _), base = _moebius_attack(base=True)
    activate(st, moebius)
    assert zone_of(st, base) is Zone.OUTSIDE  # the EX Base token was destroyed
    assert zone_of(st, top) is Zone.SHIELD


@pytest.mark.card("GD02-011")
@pytest.mark.ruling("GD02-011:Q173")
def test_gd02_011_q173_cannot_activate_after_being_blocked() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    moebius = sc.add(0, "GD02-011")
    blocker = sc.add(1, BLOCKER_3HP)
    sc.shields(1, VANILLA)
    sc.hand(0, DAMAGE_1)
    st = sc.start()
    attack(st, moebius)
    block(st, blocker)
    assert pending_kind(st) is DecisionKind.ACTION_STEP
    assert has_action(st, A.PLAY_COMMAND)
    assert not has_action(st, A.ACTIVATE, moebius)


@pytest.mark.card("GD02-011")
def test_gd02_011_cannot_activate_when_not_attacking() -> None:
    sc = Scenario(active=1)
    sc.resources(0, 2)
    moebius = sc.add(0, "GD02-011")
    enemy = sc.add(1, VANILLA)
    sc.shields(0, VANILLA)
    sc.shields(1, VANILLA)
    sc.hand(0, DAMAGE_1)
    st = sc.start()
    attack(st, enemy)
    assert st.pending is not None and st.pending.player == 0
    assert has_action(st, A.PLAY_COMMAND)
    assert not has_action(st, A.ACTIVATE, moebius)


# ---------------------------------------------------------------------------------------------
# GD02-014 Galbaldy Beta, GD02-016 Barzam


@pytest.mark.card("GD02-014", "GD02-016")
@pytest.mark.parametrize("number", ["GD02-014", "GD02-016"])
def test_deploy_gives_a_titans_unit_ap_plus_1_this_turn(number: str) -> None:
    sc = Scenario()
    sc.resources(0, 3)
    hizack = sc.add(0, "GD02-013")  # Titans
    other = sc.add(0, VANILLA)  # not Titans
    card_uid = sc.add(0, number, Zone.HAND)
    st = sc.start()
    play(st, card_uid)
    assert st.pending is not None
    assert {o.a for o in st.pending.options if o.kind is A.SELECT} == {hizack, card_uid}
    select(st, hizack)
    assert ap(st, hizack) == 3
    assert ap(st, other) == 2
    to_next_turn(st)
    assert ap(st, hizack) == 2


# ---------------------------------------------------------------------------------------------
# GD02-018 Taurus, GD02-035 Police Zaku, GD02-066 Gafran


@pytest.mark.card("GD02-018", "GD02-035", "GD02-066")
@pytest.mark.ruling("GD02-018:Q174", "GD02-035:Q176", "GD02-066:Q181")
@pytest.mark.parametrize("number", ["GD02-018", "GD02-035", "GD02-066"])
def test_cannot_attack_the_player_base_or_shields(number: str) -> None:
    sc = Scenario()
    unit = sc.add(0, number)
    rested = sc.add(1, VANILLA, rested=True)
    sc.base(1)
    sc.shields(1, VANILLA)
    st = sc.start()
    assert not has_action(st, A.ATTACK, unit, PLAYER_TARGET)
    assert has_action(st, A.ATTACK, unit, rested)


# ---------------------------------------------------------------------------------------------
# GD02-020 Elmeth

LALAH = "GD02-089"  # Lalah Sune: green (Zeon) Pilot


def _elmeth(deck: tuple[str, ...]) -> tuple[GameState, int]:
    sc = Scenario()
    sc.resources(0, 6)
    elmeth = sc.add(0, "GD02-020", Zone.HAND)
    sc.deck(0, *deck)
    st = sc.start()
    play(st, elmeth)
    return st, elmeth


@pytest.mark.card("GD02-020")
def test_gd02_020_adds_a_green_zeon_pilot_from_top_5() -> None:
    top5 = (VANILLA, PILOT_LV3, LALAH, VANILLA_3HP, BIG)
    st, _ = _elmeth((*top5, "GD02-032"))
    assert pending_kind(st) is DecisionKind.YES_NO
    yes(st)
    assert numbers_in(st, 0, Zone.HAND) == [LALAH]
    deck = numbers_in(st, 0, Zone.DECK)
    assert deck[0] == "GD02-032"
    assert sorted(deck[-4:]) == sorted([VANILLA, PILOT_LV3, VANILLA_3HP, BIG])


@pytest.mark.card("GD02-020")
@pytest.mark.ruling("GD02-020:Q472")
def test_gd02_020_q472_look_is_mandatory_adding_is_optional() -> None:
    top5 = (VANILLA, PILOT_LV3, LALAH, VANILLA_3HP, BIG)
    st, _ = _elmeth((*top5, "GD02-032"))
    no(st)
    assert numbers_in(st, 0, Zone.HAND) == []
    deck = numbers_in(st, 0, Zone.DECK)
    assert deck[0] == "GD02-032"  # the 5 looked-at cards went to the bottom
    assert sorted(deck[-5:]) == sorted(top5)


@pytest.mark.card("GD02-020")
def test_gd02_020_during_link_ap_plus_2() -> None:
    sc = Scenario()
    linked = sc.add(0, "GD02-020", pilot=LALAH)
    unlinked = sc.add(0, "GD02-020", pilot=PILOT_LV3)
    st = sc.start()
    assert ap(st, linked) == 4 + 1 + 2
    assert ap(st, unlinked) == 4 + 1


# ---------------------------------------------------------------------------------------------
# GD02-021 Gundam AGE-1 Normal

GREEN_EF_UNIT = "GD02-030"  # Genoace, green (Earth Federation) Unit


def _age1(resources: int, hand: tuple[str, ...]) -> tuple[GameState, int]:
    sc = Scenario()
    sc.resources(0, resources)
    age1 = sc.add(0, "GD02-021", Zone.HAND)
    sc.hand(0, *hand)
    st = sc.start()
    play(st, age1)
    return st, age1


def ex_count(st: GameState, player: int = 0) -> int:
    return sum(1 for u in st.zones[player][Zone.RESOURCE_AREA] if V.cdef(st, u).is_token)


@pytest.mark.card("GD02-021")
def test_gd02_021_discard_places_ex_and_draws_at_lv7() -> None:
    st, _ = _age1(6, (GREEN_EF_UNIT,))
    yes(st)
    assert ex_count(st) == 1
    assert numbers_in(st, 0, Zone.TRASH) == [GREEN_EF_UNIT]
    assert numbers_in(st, 0, Zone.HAND) == [VANILLA]  # drew 1 (Lv.7 after the EX Resource)


@pytest.mark.card("GD02-021")
def test_gd02_021_below_lv7_no_draw() -> None:
    st, _ = _age1(5, (GREEN_EF_UNIT,))
    yes(st)
    assert ex_count(st) == 1
    assert numbers_in(st, 0, Zone.HAND) == []


@pytest.mark.card("GD02-021")
@pytest.mark.ruling("GD02-021:Q175")
def test_gd02_021_q175_no_discard_no_draw_even_at_lv7() -> None:
    st, _ = _age1(7, (GREEN_EF_UNIT,))
    no(st)
    assert ex_count(st) == 0
    assert numbers_in(st, 0, Zone.HAND) == [GREEN_EF_UNIT]


@pytest.mark.card("GD02-021")
def test_gd02_021_only_green_earth_federation_unit_cards_can_be_discarded() -> None:
    st, _ = _age1(7, (VANILLA, "GD02-032"))  # red Zeon / green Zeon
    yes(st)
    assert ex_count(st) == 0
    assert sorted(numbers_in(st, 0, Zone.HAND)) == sorted([VANILLA, "GD02-032"])


# ---------------------------------------------------------------------------------------------
# GD02-022 G-Exes

AGE_UNIT = "GD02-029"  # Gundam AGE-1 Normal (AGE System), no effects
AGE_DEVICE = "GD02-103"  # 【Main】If you have an (AGE System) Unit in play, place 1 EX Resource.


@pytest.mark.card("GD02-022")
def test_gd02_022_placing_ex_resource_gives_breach_2() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    sc.add(0, "GD02-022")
    age = sc.add(0, AGE_UNIT)
    device = sc.add(0, AGE_DEVICE, Zone.HAND)
    st = sc.start()
    play(st, device)
    assert ex_count(st) == 1
    assert keywords(st, age) == {"Breach": 2}
    to_next_turn(st)
    assert keywords(st, age) == {}


@pytest.mark.card("GD02-022")
def test_gd02_022_once_per_turn() -> None:
    sc = Scenario()
    sc.resources(0, 8)
    sc.add(0, "GD02-022")
    age = sc.add(0, AGE_UNIT)
    first, second = sc.hand(0, AGE_DEVICE, AGE_DEVICE)
    st = sc.start()
    play(st, first)
    play(st, second)
    assert ex_count(st) == 2
    assert keywords(st, age) == {"Breach": 2}


@pytest.mark.card("GD02-022")
@pytest.mark.rule("10-3-3-1", "13-2-13-1")
def test_gd02_022_without_age_unit_once_per_turn_is_not_used() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    sc.add(0, "GD02-022")
    wing = sc.add(0, "ST02-002", Zone.HAND)  # 【Deploy】Place 1 EX Resource. (not AGE System)
    age1 = sc.add(0, "GD02-021", Zone.HAND)
    sc.hand(0, GREEN_EF_UNIT)
    st = sc.start()
    play(st, wing)
    assert ex_count(st) == 1
    play(st, age1)
    yes(st)
    assert ex_count(st) == 2
    assert keywords(st, age1) == {"Breach": 2}


# ---------------------------------------------------------------------------------------------
# GD02-023 Gundam AGE-1 Spallow

FLIT = "GD02-088"  # Flit Asuno


@pytest.mark.card("GD02-023")
@pytest.mark.parametrize(
    ("resources", "pilot", "expected"),
    [(7, FLIT, True), (6, FLIT, False), (7, PILOT_LV3, False)],
)
def test_gd02_023_first_strike_when_linked_at_lv7(
    resources: int, pilot: str, expected: bool
) -> None:
    sc = Scenario()
    sc.resources(0, resources)
    spallow = sc.add(0, "GD02-023")
    pair_unchecked(sc, 0, spallow, pilot)
    st = sc.start()
    assert ("First Strike" in keywords(st, spallow)) is expected


@pytest.mark.card("GD02-023")
@pytest.mark.rule("13-1-5-2")
def test_gd02_023_first_strike_destroys_before_damage_back() -> None:
    sc = Scenario()
    sc.resources(0, 7)
    spallow = sc.add(0, "GD02-023")
    pair_unchecked(sc, 0, spallow, FLIT)
    victim = sc.add(1, BIG, rested=True)  # 4/3
    st = sc.start()
    attack(st, spallow, victim)
    settle(st)
    assert zone_of(st, victim) is Zone.TRASH
    assert st.cards[spallow].damage == 0


# ---------------------------------------------------------------------------------------------
# GD02-024 Red Gundam


@pytest.mark.card("GD02-024")
def test_gd02_024_linked_cannot_be_blocked() -> None:
    sc = Scenario()
    red = sc.add(0, "GD02-024", pilot="ST06-009")  # Amate Yuzuriha (Machu) (Clan)
    sc.add(1, BLOCKER_3HP)
    sc.shields(1, VANILLA)
    st = sc.start()
    assert "High-Maneuver" in keywords(st, red)
    attack(st, red)
    assert pending_kind(st) is not DecisionKind.BLOCK


@pytest.mark.card("GD02-024")
def test_gd02_024_unlinked_can_be_blocked() -> None:
    sc = Scenario()
    red = sc.add(0, "GD02-024", pilot=PILOT_LV3)
    sc.add(1, BLOCKER_3HP)
    sc.shields(1, VANILLA)
    st = sc.start()
    assert "High-Maneuver" not in keywords(st, red)
    attack(st, red)
    assert pending_kind(st) is DecisionKind.BLOCK


# ---------------------------------------------------------------------------------------------
# GD02-025 Gundam Heavyarms


@pytest.mark.card("GD02-025")
@pytest.mark.parametrize(("choice", "top_after"), [(0, "GD02-032"), (1, VANILLA_3HP)])
def test_gd02_025_top_card_to_top_or_bottom(choice: int, top_after: str) -> None:
    sc = Scenario()
    sc.resources(0, 4)
    heavyarms = sc.add(0, "GD02-025", Zone.HAND)
    sc.deck(0, "GD02-032", VANILLA_3HP)
    st = sc.start()
    play(st, heavyarms)
    assert pending_kind(st) is DecisionKind.ARRANGE
    select(st, choice)
    deck = numbers_in(st, 0, Zone.DECK)
    assert deck[0] == top_after
    if choice == 1:
        assert deck[-1] == "GD02-032"


# ---------------------------------------------------------------------------------------------
# GD02-026 Genoace Custom


@pytest.mark.card("GD02-026")
@pytest.mark.parametrize(("resources", "bonus"), [(7, 2), (6, 0)])
def test_gd02_026_lv7_gives_age_unit_ap_plus_2(resources: int, bonus: int) -> None:
    sc = Scenario()
    sc.resources(0, resources)
    age = sc.add(0, AGE_UNIT)
    genoace = sc.add(0, "GD02-026", Zone.HAND)
    st = sc.start()
    play(st, genoace)
    assert ap(st, age) == 3 + bonus
    assert ap(st, genoace) == 2


# ---------------------------------------------------------------------------------------------
# GD02-027 Gundam AGE-1 Titus (Lv.7)


@pytest.mark.card("GD02-027")
@pytest.mark.rule("13-1-2")
def test_gd02_027_breach_3_after_destroying_a_unit() -> None:
    sc = Scenario()
    titus = sc.add(0, "GD02-027")
    victim = sc.add(1, VANILLA, rested=True)
    base = sc.base(1)
    st = sc.start()
    assert keywords(st, titus) == {"Breach": 3}
    attack(st, titus, victim)
    settle(st)
    assert zone_of(st, victim) is Zone.TRASH
    assert zone_of(st, base) is Zone.OUTSIDE  # EX Base (3 HP) destroyed by Breach 3


# ---------------------------------------------------------------------------------------------
# GD02-031 Gundam AGE-1 Titus (Lv.4)


@pytest.mark.card("GD02-031")
@pytest.mark.parametrize(("resources", "expected"), [(7, 4), (6, 2)])
def test_gd02_031_ap_plus_2_at_lv7(resources: int, expected: int) -> None:
    sc = Scenario()
    sc.resources(0, resources)
    titus = sc.add(0, "GD02-031")
    st = sc.start()
    assert ap(st, titus) == expected


# ---------------------------------------------------------------------------------------------
# GD02-033 Kikeroga (MA Mode) (GQ)


@pytest.mark.card("GD02-033")
def test_gd02_033_breach_5_with_another_zeon_link_unit() -> None:
    sc = Scenario()
    kikeroga = sc.add(0, "GD02-033")
    sc.add(0, "GD02-032", pilot="ST11-011")  # White Gundam linked with Char Aznable (Zeon)
    st = sc.start()
    assert keywords(st, kikeroga) == {"Breach": 5}


@pytest.mark.card("GD02-033")
def test_gd02_033_itself_linked_or_unlinked_zeon_units_do_not_count() -> None:
    sc = Scenario()
    kikeroga = sc.add(0, "GD02-033", pilot="GD02-090")  # linked with Challia Bull
    sc.add(0, "GD02-032", pilot=PILOT_LV3)  # Zeon but not linked
    sc.add(0, "GD02-012", pilot="GD01-087")  # Core Booster linked (White Base Team), not Zeon
    st = sc.start()
    assert V.is_linked(V.derived(st), kikeroga)
    assert keywords(st, kikeroga) == {}


# ---------------------------------------------------------------------------------------------
# GD02-034 GQuuuuuuX


@pytest.mark.card("GD02-034")
def test_gd02_034_red_pilot_gives_ap_plus_2() -> None:
    sc = Scenario()
    red = sc.add(0, "GD02-034", pilot="ST04-011")  # Athrun Zala, red Pilot +1 AP
    blue = sc.add(0, "GD02-034", pilot=PILOT_LV3)  # blue Pilot +1 AP
    alone = sc.add(0, "GD02-034")
    st = sc.start()
    assert ap(st, red) == 3
    assert ap(st, blue) == 1
    assert ap(st, alone) == 0


# ---------------------------------------------------------------------------------------------
# GD02-036 Qubeley

HAMAN = "GD02-091"  # Haman Karn: red (Neo Zeon) Pilot, 【When Paired】If red, 1 damage


@pytest.mark.card("GD02-036")
def test_gd02_036_when_linked_gains_suppression_this_turn() -> None:
    sc = Scenario()
    sc.resources(0, 7)
    qubeley = sc.add(0, "GD02-036")
    haman = sc.add(0, HAMAN, Zone.HAND)
    st = sc.start()
    play(st, haman, onto=qubeley)
    settle(st)
    assert "Suppression" in keywords(st, qubeley)
    to_next_turn(st)
    assert "Suppression" not in keywords(st, qubeley)


@pytest.mark.card("GD02-036")
def test_gd02_036_attack_with_neo_zeon_pilot_deals_2_to_damaged_enemy() -> None:
    sc = Scenario()
    qubeley = sc.add(0, "GD02-036", pilot=HAMAN)
    damaged = sc.add(1, LV5_UNIT, damage=1)
    healthy = sc.add(1, VANILLA)
    sc.shields(1, VANILLA)
    st = sc.start()
    attack(st, qubeley)
    assert st.cards[damaged].damage == 3
    assert st.cards[healthy].damage == 0


@pytest.mark.card("GD02-036")
def test_gd02_036_attack_needs_a_neo_zeon_pilot() -> None:
    sc = Scenario()
    qubeley = sc.add(0, "GD02-036", pilot=PILOT_LV3)
    damaged = sc.add(1, BIG, damage=1)
    sc.shields(1, VANILLA)
    st = sc.start()
    attack(st, qubeley)
    assert st.cards[damaged].damage == 1


# ---------------------------------------------------------------------------------------------
# GD02-037 Gundam Virsago


def _virsago(shields: int, *, base: bool = False) -> tuple[GameState, int, int]:
    sc = Scenario()
    sc.resources(0, 5)
    virsago = sc.add(0, "GD02-037", Zone.HAND)
    target = sc.add(1, LV5_UNIT)  # 5 AP
    sc.add(1, "GD04-049")  # Gundam DX, 6 AP: not a legal choice
    sc.shields(1, *([VANILLA] * shields))
    if base:
        sc.base(1)
    st = sc.start()
    play(st, virsago)
    return st, virsago, target


@pytest.mark.card("GD02-037")
def test_gd02_037_three_shields_deals_2_to_enemy_with_5_or_less_ap() -> None:
    st, virsago, target = _virsago(3)
    assert st.cards[target].damage == 2
    assert keywords(st, virsago) == {"Breach": 1}


@pytest.mark.card("GD02-037")
def test_gd02_037_four_shields_does_nothing() -> None:
    st, _, target = _virsago(4)
    assert st.cards[target].damage == 0


@pytest.mark.card("GD02-037")
@pytest.mark.ruling("GD02-037:Q177")
def test_gd02_037_q177_base_is_not_a_shield() -> None:
    st, _, target = _virsago(3, base=True)
    assert st.cards[target].damage == 2


# ---------------------------------------------------------------------------------------------
# GD02-038 GQuuuuuuX (Omega Psycommu)

CLAN_LV2 = "GD03-032"  # Zaku (Four Snake Eyes') [YETI] (GQ): (Clan) Lv.2, no effects
CLAN_LV4_DEPLOY = "GD02-041"  # Sugai's Gelgoog (GQ): (Clan) Lv.4, 【Deploy】2 damage to Lv.5+


def _omega(top3: tuple[str, ...], resources: int = 7) -> tuple[Scenario, GameState, int]:
    sc = Scenario()
    sc.resources(0, resources)
    omega = sc.add(0, "GD02-038", Zone.HAND)
    enemy = sc.add(1, LV5_UNIT)
    sc.deck(0, *top3, "GD02-032")
    st = sc.start()
    play(st, omega)
    return sc, st, enemy


@pytest.mark.card("GD02-038")
@pytest.mark.ruling("GD02-038:Q178")
def test_gd02_038_deploys_a_clan_unit_for_free() -> None:
    _, st, _ = _omega((CLAN_LV2, VANILLA, "GD02-024"))  # GD02-024: (Clan) but Lv.5
    yes(st)
    assert numbers_in(st, 0, Zone.BATTLE) == ["GD02-038", CLAN_LV2]
    assert sum(1 for u in st.zones[0][Zone.RESOURCE_AREA] if not st.cards[u].rested) == 2
    deck = numbers_in(st, 0, Zone.DECK)
    assert deck[0] == "GD02-032"
    assert sorted(deck[-2:]) == sorted([VANILLA, "GD02-024"])


@pytest.mark.card("GD02-038")
@pytest.mark.ruling("GD02-038:Q179")
def test_gd02_038_q179_deployed_units_deploy_effect_activates() -> None:
    _, st, enemy = _omega((CLAN_LV4_DEPLOY, VANILLA, VANILLA))
    yes(st)
    assert CLAN_LV4_DEPLOY in numbers_in(st, 0, Zone.BATTLE)
    assert st.cards[enemy].damage == 2


@pytest.mark.card("GD02-038")
@pytest.mark.ruling("GD02-038:Q473")
def test_gd02_038_q473_look_is_mandatory_deploying_is_optional() -> None:
    _, st, _ = _omega((CLAN_LV2, VANILLA, VANILLA_3HP))
    no(st)
    assert numbers_in(st, 0, Zone.BATTLE) == ["GD02-038"]
    deck = numbers_in(st, 0, Zone.DECK)
    assert deck[0] == "GD02-032"
    assert sorted(deck[-3:]) == sorted([CLAN_LV2, VANILLA, VANILLA_3HP])


# ---------------------------------------------------------------------------------------------
# GD02-039 Haman Karn's Gaza C


@pytest.mark.card("GD02-039")
def test_gd02_039_when_paired_deals_1_to_lv3_or_lower() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    gaza = sc.add(0, "GD02-039")
    pilot = sc.add(0, PILOT_LV3, Zone.HAND)
    low = sc.add(1, VANILLA_3HP)  # Lv.3
    high = sc.add(1, BIG)  # Lv.4
    st = sc.start()
    play(st, pilot, onto=gaza)
    assert st.cards[low].damage == 1
    assert st.cards[high].damage == 0


# ---------------------------------------------------------------------------------------------
# GD02-040 Gundam Ashtaron

DAUGHTRESS = "GD02-049"  # Daughtress: (New UNE) 1/1, <Support 1>


@pytest.mark.card("GD02-040")
def test_gd02_040_other_new_une_unit_ignores_battle_damage_from_2_hp_units() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    daughtress = sc.add(0, DAUGHTRESS)
    ashtaron = sc.add(0, "GD02-040", Zone.HAND)
    weak = sc.add(1, VANILLA, rested=True)  # 2 HP
    st = sc.start()
    play(st, ashtaron)
    attack(st, daughtress, weak)
    settle(st)
    assert zone_of(st, daughtress) is Zone.BATTLE
    assert st.cards[daughtress].damage == 0


@pytest.mark.card("GD02-040")
def test_gd02_040_units_with_3_hp_still_deal_damage() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    daughtress = sc.add(0, DAUGHTRESS)
    ashtaron = sc.add(0, "GD02-040", Zone.HAND)
    sturdy = sc.add(1, VANILLA_3HP, rested=True)
    st = sc.start()
    play(st, ashtaron)
    attack(st, daughtress, sturdy)
    settle(st)
    assert zone_of(st, daughtress) is Zone.TRASH


@pytest.mark.card("GD02-040")
@pytest.mark.rule("13-1-3")
def test_gd02_040_support_2() -> None:
    sc = Scenario()
    ashtaron = sc.add(0, "GD02-040")
    other = sc.add(0, VANILLA)
    st = sc.start()
    assert keywords(st, ashtaron) == {"Support": 2}
    activate(st, ashtaron)
    assert st.cards[ashtaron].rested
    assert ap(st, other) == 4


# ---------------------------------------------------------------------------------------------
# GD02-041 Sugai's Gelgoog (GQ)


@pytest.mark.card("GD02-041")
def test_gd02_041_deploy_deals_2_to_lv5_or_higher() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    gelgoog = sc.add(0, "GD02-041", Zone.HAND)
    high = sc.add(1, LV5_UNIT)
    low = sc.add(1, BIG)
    st = sc.start()
    play(st, gelgoog)
    assert st.cards[high].damage == 2
    assert st.cards[low].damage == 0


# ---------------------------------------------------------------------------------------------
# GD02-042 Gundam Ashtaron (MA Mode)


@pytest.mark.card("GD02-042")
def test_gd02_042_new_une_unit_gains_high_maneuver_this_turn() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    daughtress = sc.add(0, DAUGHTRESS)
    ma = sc.add(0, "GD02-042", Zone.HAND)
    st = sc.start()
    play(st, ma)
    select(st, daughtress)
    assert "High-Maneuver" in keywords(st, daughtress)
    assert "High-Maneuver" not in keywords(st, ma)
    to_next_turn(st)
    assert "High-Maneuver" not in keywords(st, daughtress)


# ---------------------------------------------------------------------------------------------
# GD02-043 Daughtress Weapon, GD02-044 Daughtress Command


def _tokens(st: GameState) -> list[int]:
    return [u for u in st.zones[0][Zone.BATTLE] if V.cdef(st, u).is_token]


@pytest.mark.card("GD02-043")
def test_gd02_043_deploy_with_another_new_une_unit_deploys_rested_token() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    sc.add(0, DAUGHTRESS)
    weapon = sc.add(0, "GD02-043", Zone.HAND)
    st = sc.start()
    play(st, weapon)
    (token,) = _tokens(st)
    cd = V.cdef(st, token)
    assert (cd.name, cd.traits, ap(st, token), hp(st, token)) == ("Daughtress", ("New UNE",), 0, 1)
    assert st.cards[token].rested


@pytest.mark.card("GD02-043")
def test_gd02_043_alone_deploys_nothing() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    sc.add(0, VANILLA)
    weapon = sc.add(0, "GD02-043", Zone.HAND)
    st = sc.start()
    play(st, weapon)
    assert _tokens(st) == []


@pytest.mark.card("GD02-044")
def test_gd02_044_destroyed_with_another_new_une_unit_deploys_rested_token() -> None:
    sc = Scenario()
    command = sc.add(0, "GD02-044")
    sc.add(0, DAUGHTRESS)
    victim = sc.add(1, VANILLA, rested=True)
    st = sc.start()
    attack(st, command, victim)
    settle(st)
    assert zone_of(st, command) is Zone.TRASH
    (token,) = _tokens(st)
    assert V.cdef(st, token).name == "Daughtress"
    assert st.cards[token].rested


@pytest.mark.card("GD02-044")
def test_gd02_044_destroyed_alone_deploys_nothing() -> None:
    sc = Scenario()
    command = sc.add(0, "GD02-044")
    victim = sc.add(1, VANILLA, rested=True)
    st = sc.start()
    attack(st, command, victim)
    settle(st)
    assert zone_of(st, command) is Zone.TRASH
    assert _tokens(st) == []


# ---------------------------------------------------------------------------------------------
# GD02-045 GINN Long-Range Reconnaissance Type

DESERT_TIGER = "GD01-113"  # 【Main】/【Action】Choose 1 friendly (ZAFT) Unit. It gets AP+3.


@pytest.mark.card("GD02-045")
def test_gd02_045_attacking_a_unit_with_5_ap_draws() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    ginn = sc.add(0, "GD02-045", pilot=PILOT_LV3)
    tiger = sc.add(0, DESERT_TIGER, Zone.HAND)
    victim = sc.add(1, VANILLA, rested=True)
    st = sc.start()
    play(st, tiger)
    assert ap(st, ginn) == 5
    before = hand_size(st)
    attack(st, ginn, victim)
    assert hand_size(st) == before + 1


@pytest.mark.card("GD02-045")
def test_gd02_045_less_than_5_ap_or_attacking_the_player_does_not_draw() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    weak = sc.add(0, "GD02-045")
    strong = sc.add(0, "GD02-045", pilot=PILOT_LV3)
    tiger = sc.add(0, DESERT_TIGER, Zone.HAND)
    victim = sc.add(1, VANILLA, rested=True)
    sc.shields(1, VANILLA, VANILLA)
    st = sc.start()
    play(st, tiger)
    select(st, strong)
    assert ap(st, strong) == 5
    before = hand_size(st)
    attack(st, strong)
    settle(st)
    assert hand_size(st) == before
    attack(st, weak, victim)
    assert hand_size(st) == before


# ---------------------------------------------------------------------------------------------
# GD02-046 Sayla's Light-Type Guncannon


@pytest.mark.card("GD02-046")
def test_gd02_046_deploy_deals_2_to_enemy_unit_token() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    guncannon = sc.add(0, "GD02-046", Zone.HAND)
    token = sc.add(1, ENEMY_TOKEN)
    unit = sc.add(1, VANILLA)
    st = sc.start()
    play(st, guncannon)
    assert zone_of(st, token) is Zone.OUTSIDE  # 1 HP token destroyed, leaves the game
    assert st.cards[unit].damage == 0


# ---------------------------------------------------------------------------------------------
# GD02-047 Gaza C


@pytest.mark.card("GD02-047")
def test_gd02_047_rest_destroy_itself_and_deal_1() -> None:
    sc = Scenario()
    gaza = sc.add(0, "GD02-047")
    target = sc.add(1, LV5_UNIT)
    st = sc.start()
    activate(st, gaza)
    assert zone_of(st, gaza) is Zone.TRASH
    assert st.cards[target].damage == 1


@pytest.mark.card("GD02-047")
@pytest.mark.rule("10-2-2")
def test_gd02_047_needs_enemy_lv5_or_lower_target() -> None:
    sc = Scenario()
    gaza = sc.add(0, "GD02-047")
    sc.add(1, "GD04-049")  # Gundam DX, Lv.8
    st = sc.start()
    assert not has_action(st, A.ACTIVATE, gaza)


# ---------------------------------------------------------------------------------------------
# GD02-049 Daughtress


@pytest.mark.card("GD02-049")
@pytest.mark.rule("13-1-3")
def test_gd02_049_support_1() -> None:
    sc = Scenario()
    daughtress = sc.add(0, DAUGHTRESS)
    other = sc.add(0, VANILLA)
    st = sc.start()
    activate(st, daughtress)
    assert ap(st, other) == 3


# ---------------------------------------------------------------------------------------------
# GD02-053 Gundam X (Lv.7)

GARROD = "GD02-094"  # Garrod Ran & Tiffa Adill (Vulture) Pilot
VULTURE = "GD02-063"  # Gundam Airmaster (Fighter Mode): (Vulture) 3/3, no effects


def _gundam_x(trash: int, *, active: int = 0, pilot: str = GARROD) -> tuple[GameState, int, int]:
    sc = Scenario(active=active)
    gx = sc.add(0, "GD02-053", pilot=pilot)
    ally = sc.add(0, VULTURE)
    fill_trash(sc, 0, trash)
    st = sc.start()
    return st, gx, ally


@pytest.mark.card("GD02-053")
def test_gd02_053_linked_with_7_trash_other_vulture_units_get_ap_plus_2() -> None:
    st, gx, ally = _gundam_x(7)
    assert ap(st, ally) == 5
    assert ap(st, gx) == 6 + 1
    assert "Suppression" in keywords(st, gx)


@pytest.mark.card("GD02-053")
@pytest.mark.parametrize(
    ("trash", "active", "pilot"), [(6, 0, GARROD), (7, 1, GARROD), (7, 0, PILOT_LV3)]
)
def test_gd02_053_needs_link_your_turn_and_7_trash(trash: int, active: int, pilot: str) -> None:
    st, _, ally = _gundam_x(trash, active=active, pilot=pilot)
    assert ap(st, ally) == 3


# ---------------------------------------------------------------------------------------------
# GD02-054 Gundam Barbatos 1st Form


@pytest.mark.card("GD02-054")
@pytest.mark.parametrize(("damage", "drawn"), [(1, 1), (0, 0)])
def test_gd02_054_attack_while_damaged_draws(damage: int, drawn: int) -> None:
    sc = Scenario()
    barbatos = sc.add(0, "GD02-054", damage=damage)
    sc.shields(1, VANILLA)
    st = sc.start()
    before = hand_size(st)
    attack(st, barbatos)
    assert hand_size(st) == before + drawn


# ---------------------------------------------------------------------------------------------
# GD02-055 Gundam Gusion Rebake


@pytest.mark.card("GD02-055")
def test_gd02_055_deals_1_to_a_friendly_and_an_enemy_unit() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    friend = sc.add(0, VANILLA_3HP)
    gusion = sc.add(0, "GD02-055", Zone.HAND)
    enemy = sc.add(1, VANILLA)
    st = sc.start()
    play(st, gusion)
    select(st, friend)
    assert st.cards[friend].damage == 1
    assert st.cards[enemy].damage == 1
    assert st.cards[gusion].damage == 0
    assert "Blocker" in keywords(st, gusion)


# ---------------------------------------------------------------------------------------------
# GD02-056 Gundam X (Lv.4)


@pytest.mark.card("GD02-056")
@pytest.mark.parametrize(("pilot", "returned"), [(GARROD, True), (PILOT_LV3, False)])
def test_gd02_056_destroyed_returns_vulture_lv5_unit_card(pilot: str, returned: bool) -> None:
    sc = Scenario()
    gx = sc.add(0, "GD02-056", pilot=pilot)
    big, small = sc.trash(0, "GD02-060", VULTURE)  # Gundam Leopard Lv.5 / Airmaster Lv.3
    victim = sc.add(1, "GD04-049", rested=True)  # Gundam DX, 6 AP
    st = sc.start()
    attack(st, gx, victim)
    settle(st)
    assert zone_of(st, gx) is Zone.TRASH
    assert (zone_of(st, big) is Zone.HAND) is returned
    assert zone_of(st, small) is Zone.TRASH


# ---------------------------------------------------------------------------------------------
# GD02-057 Zedas


@pytest.mark.card("GD02-057")
def test_gd02_057_destroying_another_unit_deals_2() -> None:
    sc = Scenario()
    zedas = sc.add(0, "GD02-057", pilot=PILOT_LV3)
    fodder = sc.add(0, VANILLA)
    target = sc.add(1, BIG)  # Lv.4
    sc.add(1, LV5_UNIT)  # Lv.5: not choosable
    sc.shields(1, VANILLA)
    st = sc.start()
    attack(st, zedas)
    select(st, fodder)
    assert zone_of(st, fodder) is Zone.TRASH
    assert st.cards[target].damage == 2


@pytest.mark.card("GD02-057")
def test_gd02_057_may_decline() -> None:
    sc = Scenario()
    zedas = sc.add(0, "GD02-057", pilot=PILOT_LV3)
    fodder = sc.add(0, VANILLA)
    target = sc.add(1, BIG)
    sc.shields(1, VANILLA)
    st = sc.start()
    attack(st, zedas)
    select(st)
    assert zone_of(st, fodder) is Zone.BATTLE
    assert st.cards[target].damage == 0


@pytest.mark.card("GD02-057")
def test_gd02_057_requires_pair() -> None:
    sc = Scenario()
    zedas = sc.add(0, "GD02-057")
    fodder = sc.add(0, VANILLA)
    sc.add(1, BIG)
    sc.shields(1, VANILLA)
    st = sc.start()
    attack(st, zedas)
    assert pending_kind(st) is not DecisionKind.SELECT
    assert zone_of(st, fodder) is Zone.BATTLE


# ---------------------------------------------------------------------------------------------
# GD02-058 Ryusei-Go (Graze Custom Ⅱ)


@pytest.mark.card("GD02-058")
def test_gd02_058_damage_own_unit_then_draw_and_discard() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    ryusei = sc.add(0, "GD02-058", Zone.HAND)
    keep = sc.add(0, VANILLA_3HP, Zone.HAND)
    sc.deck(0, BIG)
    st = sc.start()
    play(st, ryusei)
    assert st.cards[ryusei].damage == 1
    assert pending_kind(st) is DecisionKind.DISCARD
    select(st, keep)
    assert numbers_in(st, 0, Zone.HAND) == [BIG]
    assert numbers_in(st, 0, Zone.TRASH) == [VANILLA_3HP]


@pytest.mark.card("GD02-058")
@pytest.mark.rule("5-20-1", "5-20-2")
def test_gd02_058_no_damage_dealt_means_no_draw_and_no_discard() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    zaku = sc.add(0, "GD04-062")  # (ZAFT) Unit that links with a (Minerva Squad) Pilot
    rey = sc.add(0, "GD04-093", Zone.HAND)  # 【When Linked】reduce the next damage it receives by 2
    ryusei = sc.add(0, "GD02-058", Zone.HAND)
    keep = sc.add(0, VANILLA_3HP, Zone.HAND)
    st = sc.start()
    play(st, rey, onto=zaku)
    play(st, ryusei)
    select(st, zaku)
    assert st.cards[zaku].damage == 0
    assert pending_kind(st) is DecisionKind.MAIN
    assert st.zones[0][Zone.HAND] == [keep]


# ---------------------------------------------------------------------------------------------
# GD02-059 Gundam Airmaster, GD02-079 Rick Dias


@pytest.mark.card("GD02-059", "GD02-079")
@pytest.mark.rule("13-1-4-1")
@pytest.mark.parametrize("number", ["GD02-059", "GD02-079"])
def test_blocker_can_block(number: str) -> None:
    sc = Scenario(active=1)
    blocker = sc.add(0, number)
    enemy = sc.add(1, VANILLA)
    sc.shields(0, VANILLA)
    st = sc.start()
    attack(st, enemy)
    assert pending_kind(st) is DecisionKind.BLOCK
    block(st, blocker)
    settle(st)
    assert zone_of(st, enemy) is Zone.TRASH
    assert len(st.zones[0][Zone.SHIELD]) == 1


# ---------------------------------------------------------------------------------------------
# GD02-060 Gundam Leopard (Lv.5, 【Deploy】)


@pytest.mark.card("GD02-060")
@pytest.mark.parametrize(("trash", "rested"), [(7, True), (6, False)])
def test_gd02_060_seven_trash_rests_lv4_or_lower(trash: int, rested: bool) -> None:
    sc = Scenario()
    sc.resources(0, 5)
    leopard = sc.add(0, "GD02-060", Zone.HAND)
    target = sc.add(1, BIG)
    high = sc.add(1, LV5_UNIT)
    fill_trash(sc, 0, trash)
    st = sc.start()
    play(st, leopard)
    assert st.cards[target].rested is rested
    assert not st.cards[high].rested


# ---------------------------------------------------------------------------------------------
# GD02-061 Hyakuri

JAMIL = "GD03-096"  # Jamil Neate: purple Pilot
TEIWAZ = "GD02-062"  # Amida's Hyakuren: (Teiwaz) Unit


@pytest.mark.card("GD02-061")
@pytest.mark.parametrize(
    ("pilot", "teiwaz", "rested"),
    [(JAMIL, 3, True), (JAMIL, 2, False), (PILOT_LV3, 3, False)],
)
def test_gd02_061_purple_pilot_and_three_teiwaz_cards_rest_enemy(
    pilot: str, teiwaz: int, rested: bool
) -> None:
    sc = Scenario()
    sc.resources(0, 4)
    hyakuri = sc.add(0, "GD02-061")
    pilot_uid = sc.add(0, pilot, Zone.HAND)
    target = sc.add(1, VANILLA_3HP)  # 3 AP
    sc.add(1, BIG)  # 4 AP: not choosable
    sc.trash(0, *([TEIWAZ] * teiwaz))
    st = sc.start()
    play(st, pilot_uid, onto=hyakuri)
    assert st.cards[target].rested is rested


# ---------------------------------------------------------------------------------------------
# GD02-064 Gundam Leopard (Lv.5, constant)


def _leopard_vs_command(trash: int) -> tuple[GameState, int]:
    """The opponent plays a damage 【Action】 Command during our end-phase action step."""
    sc = Scenario()
    sc.resources(1, 2)
    leopard = sc.add(0, "GD02-064")
    sc.hand(1, DAMAGE_1)
    fill_trash(sc, 0, trash)
    st = sc.start()
    end_main(st)
    assert st.pending is not None and st.pending.player == 1
    play(st, st.zones[1][Zone.HAND][0])
    return st, leopard


@pytest.mark.card("GD02-064")
def test_gd02_064_seven_trash_ignores_enemy_command_damage_on_your_turn() -> None:
    st, leopard = _leopard_vs_command(7)
    assert st.cards[leopard].damage == 0


@pytest.mark.card("GD02-064")
def test_gd02_064_six_trash_takes_the_damage() -> None:
    st, leopard = _leopard_vs_command(6)
    assert st.cards[leopard].damage == 1


@pytest.mark.card("GD02-064")
def test_gd02_064_not_during_opponents_turn() -> None:
    sc = Scenario(active=1)
    sc.resources(1, 2)
    leopard = sc.add(0, "GD02-064")
    command = sc.add(1, DAMAGE_1, Zone.HAND)
    fill_trash(sc, 0, 7)
    st = sc.start()
    play(st, command)
    assert st.cards[leopard].damage == 1


@pytest.mark.card("GD02-064")
def test_gd02_064_enemy_unit_effect_damage_still_applies() -> None:
    sc = Scenario()
    leopard = sc.add(0, "GD02-064")  # 4 AP
    attacker = sc.add(0, BIG)
    doga = sc.add(
        1, "GD01-056", rested=True
    )  # 【Destroyed】1 damage to an enemy Unit, 5 or less AP
    fill_trash(sc, 0, 7)
    st = sc.start()
    attack(st, attacker, doga)
    settle(st)
    assert zone_of(st, doga) is Zone.TRASH
    if pending_kind(st) is DecisionKind.SELECT:
        select(st, leopard)
    assert st.cards[leopard].damage == 1


# ---------------------------------------------------------------------------------------------
# GD02-068 Gundam Barbatos 3rd Form


@pytest.mark.card("GD02-068")
def test_gd02_068_deploy_deals_2_to_itself() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    barbatos = sc.add(0, "GD02-068", Zone.HAND)
    st = sc.start()
    play(st, barbatos)
    assert st.cards[barbatos].damage == 2
    assert zone_of(st, barbatos) is Zone.BATTLE


# ---------------------------------------------------------------------------------------------
# GD02-069 Zeta Gundam

KAMILLE = "GD02-097"  # Kamille Bidan (AEUG) Pilot


def _zeta(pilot: str = KAMILLE) -> tuple[GameState, int, int, int]:
    sc = Scenario()
    zeta = sc.add(0, "GD02-069", pilot=pilot, rested=True)
    base = sc.base(0)
    rested_enemy = sc.add(1, VANILLA, rested=True)
    sc.shields(1, VANILLA)
    st = sc.start()
    return st, zeta, base, rested_enemy


@pytest.mark.card("GD02-069")
@pytest.mark.ruling("GD02-069:Q182")
def test_gd02_069_rest_base_to_set_active_but_not_attack_player() -> None:
    st, zeta, base, rested_enemy = _zeta()
    activate(st, zeta)
    assert st.cards[base].rested
    assert not st.cards[zeta].rested
    assert not has_action(st, A.ATTACK, zeta, PLAYER_TARGET)
    assert has_action(st, A.ATTACK, zeta, rested_enemy)
    to_next_turn(st)
    to_next_turn(st)
    assert has_action(st, A.ATTACK, zeta, PLAYER_TARGET)


@pytest.mark.card("GD02-069")
def test_gd02_069_requires_link() -> None:
    st, zeta, _, _ = _zeta(pilot=PILOT_LV3)
    assert not has_action(st, A.ACTIVATE, zeta)


@pytest.mark.card("GD02-069")
def test_gd02_069_once_per_turn() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    zeta = sc.add(0, "GD02-069", pilot=KAMILLE)
    base = sc.base(0)
    reset = sc.add(0, "GD03-119", Zone.HAND)  # Awkward Approach: set 1 rested friendly Base active
    st = sc.start()
    activate(st, zeta)
    assert st.cards[base].rested
    play(st, reset)
    assert not st.cards[base].rested
    assert not has_action(st, A.ACTIVATE, zeta)


# ---------------------------------------------------------------------------------------------
# GD02-070 Gundam Kimaris

GJALLARHORN = "GD02-077"  # Ein's Schwalbe Graze: (Gjallarhorn), no effects


@pytest.mark.card("GD02-070")
def test_gd02_070_four_gjallarhorn_cards_draw_2_then_discard_2() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    kimaris = sc.add(0, "GD02-070", Zone.HAND)
    sc.hand(0, VANILLA_3HP)
    sc.trash(0, *([GJALLARHORN] * 4))
    sc.deck(0, BIG, LV5_UNIT)
    st = sc.start()
    play(st, kimaris)
    assert sorted(numbers_in(st, 0, Zone.HAND)) == sorted([VANILLA_3HP, BIG, LV5_UNIT])
    assert pending_kind(st) is DecisionKind.DISCARD
    drawn = [u for u in st.zones[0][Zone.HAND] if V.cdef(st, u).card_number != VANILLA_3HP]
    select(st, *drawn)
    assert numbers_in(st, 0, Zone.HAND) == [VANILLA_3HP]


@pytest.mark.card("GD02-070")
def test_gd02_070_three_gjallarhorn_cards_no_draw_and_no_discard() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    kimaris = sc.add(0, "GD02-070", Zone.HAND)
    sc.hand(0, VANILLA_3HP, BIG)
    sc.trash(0, *([GJALLARHORN] * 3), VANILLA)
    st = sc.start()
    play(st, kimaris)
    assert pending_kind(st) is DecisionKind.MAIN
    assert sorted(numbers_in(st, 0, Zone.HAND)) == sorted([VANILLA_3HP, BIG])


# ---------------------------------------------------------------------------------------------
# GD02-071 Gundam Mk-II (AEUG)


@pytest.mark.card("GD02-071")
@pytest.mark.ruling("GD02-071:Q183", "GD02-071:Q184")
def test_gd02_071_pairs_aeug_pilot_from_hand_for_free_and_link_triggers() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    mk2 = sc.add(0, "GD02-071", Zone.HAND)
    kamille = sc.add(0, KAMILLE, Zone.HAND)
    sc.base(0, LINK_WATCH_BASE)  # white Base: "when a friendly Unit links, ... AP-1"
    enemy = sc.add(1, VANILLA)
    st = sc.start()
    play(st, mk2)
    assert pending_kind(st) is DecisionKind.YES_NO
    yes(st)
    settle(st)
    assert st.cards[mk2].pair == kamille
    assert zone_of(st, kamille) is Zone.PAIRED
    assert sum(1 for u in st.zones[0][Zone.RESOURCE_AREA] if not st.cards[u].rested) == 2  # Q183
    assert ap(st, enemy) == 1  # Q184: the link triggered Gryphios 2
    assert V.is_linked(V.derived(st), mk2)


@pytest.mark.card("GD02-071")
def test_gd02_071_without_white_base_no_pairing() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    mk2 = sc.add(0, "GD02-071", Zone.HAND)
    kamille = sc.add(0, KAMILLE, Zone.HAND)
    sc.base(0)  # EX Base: no colour
    st = sc.start()
    play(st, mk2)
    assert pending_kind(st) is DecisionKind.MAIN
    assert zone_of(st, kamille) is Zone.HAND


@pytest.mark.card("GD02-071")
def test_gd02_071_only_aeug_pilot_cards() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    mk2 = sc.add(0, "GD02-071", Zone.HAND)
    other = sc.add(0, PILOT_LV3, Zone.HAND)
    sc.base(0, WHITE_BASE)
    st = sc.start()
    play(st, mk2)
    assert pending_kind(st) is DecisionKind.YES_NO
    yes(st)
    assert zone_of(st, other) is Zone.HAND
    assert st.cards[mk2].pair < 0


# ---------------------------------------------------------------------------------------------
# GD02-072 Hyaku-Shiki


@pytest.mark.card("GD02-072")
@pytest.mark.parametrize(
    ("base", "expected"), [(WHITE_BASE, {"Blocker": 1, "Repair": 1}), (None, {"Blocker": 1})]
)
def test_gd02_072_repair_1_with_white_base(base: str | None, expected: dict[str, int]) -> None:
    sc = Scenario()
    hyaku = sc.add(0, "GD02-072")
    sc.base(0, base)
    st = sc.start()
    assert keywords(st, hyaku) == expected


# ---------------------------------------------------------------------------------------------
# GD02-073 Carta's Graze Ritter (Ground Type)


@pytest.mark.card("GD02-073")
@pytest.mark.rule("13-1-5-2")
def test_gd02_073_attacker_gains_first_strike_on_opponents_turn() -> None:
    sc = Scenario(active=1)
    sc.resources(0, 2)
    carta = sc.add(0, "GD02-073", rested=True)  # 5/4
    enemy = sc.add(1, BIG)  # 4/3
    sc.hand(0, DAMAGE_1)  # gives the defending player a decision in the battle action step
    st = sc.start()
    attack(st, enemy, carta)
    assert pending_kind(st) is DecisionKind.ACTION_STEP
    assert "First Strike" in keywords(st, enemy)
    settle(st)
    assert zone_of(st, carta) is Zone.TRASH
    assert zone_of(st, enemy) is Zone.BATTLE and st.cards[enemy].damage == 0


@pytest.mark.card("GD02-073")
@pytest.mark.ruling("GD02-073:Q185")
def test_gd02_073_q185_blocked_by_another_unit_loses_first_strike() -> None:
    sc = Scenario(active=1)
    carta = sc.add(0, "GD02-073", rested=True)
    blocker = sc.add(0, BLOCKER_3HP)  # 3/3
    enemy = sc.add(1, BIG)  # 4/3
    st = sc.start()
    attack(st, enemy, carta)
    assert "First Strike" in keywords(st, enemy)
    block(st, blocker)
    assert "First Strike" not in keywords(st, enemy)
    settle(st)
    assert zone_of(st, blocker) is Zone.TRASH
    assert zone_of(st, enemy) is Zone.TRASH  # damage was simultaneous (3 AP vs 3 HP)
    assert zone_of(st, carta) is Zone.BATTLE


@pytest.mark.card("GD02-073")
def test_gd02_073_not_on_your_turn() -> None:
    sc = Scenario()
    sc.resources(1, 2)
    carta = sc.add(0, "GD02-073")
    enemy = sc.add(1, BIG, rested=True)
    sc.hand(1, DAMAGE_1)
    st = sc.start()
    attack(st, carta, enemy)
    assert pending_kind(st) is DecisionKind.ACTION_STEP
    assert "First Strike" not in keywords(st, enemy)


# ---------------------------------------------------------------------------------------------
# GD02-074 Gundam Aerial Rebuild


@pytest.mark.card("GD02-074")
@pytest.mark.parametrize(
    ("pilot", "commands", "blocker"),
    [(PILOT_LV3, 4, True), (PILOT_LV3, 3, False), (None, 4, False)],
)
def test_gd02_074_paired_with_four_commands_in_trash_gains_blocker(
    pilot: str | None, commands: int, blocker: bool
) -> None:
    sc = Scenario()
    aerial = sc.add(0, "GD02-074", pilot=pilot)
    sc.trash(0, *([DAMAGE_1] * commands), VANILLA, VANILLA)
    st = sc.start()
    kws = keywords(st, aerial)
    assert kws.get("High-Maneuver") == 1
    assert ("Blocker" in kws) is blocker


# ---------------------------------------------------------------------------------------------
# GD02-075 Rick Dias (Red)


@pytest.mark.card("GD02-075")
def test_gd02_075_rest_base_to_give_enemy_ap_minus_2_this_battle() -> None:
    sc = Scenario()
    dias = sc.add(0, "GD02-075")
    base = sc.base(0)
    enemy = sc.add(1, BIG, rested=True)  # 4/3, Lv.4
    st = sc.start()
    attack(st, dias, enemy)
    assert st.cards[base].rested
    settle(st)
    assert zone_of(st, enemy) is Zone.TRASH
    assert st.cards[dias].damage == 2


@pytest.mark.card("GD02-075")
def test_gd02_075_no_active_base_does_nothing() -> None:
    sc = Scenario()
    dias = sc.add(0, "GD02-075")
    enemy = sc.add(1, BIG, rested=True)
    st = sc.start()
    attack(st, dias, enemy)
    settle(st)
    assert zone_of(st, enemy) is Zone.TRASH
    assert zone_of(st, dias) is Zone.TRASH  # traded at full 4 AP


@pytest.mark.card("GD02-075")
def test_gd02_075_ap_reduction_lasts_only_this_battle() -> None:
    sc = Scenario()
    sc.resources(1, 2)
    dias = sc.add(0, "GD02-075")
    sc.base(0)
    enemy = sc.add(1, LV5_UNIT)  # Lv.5: not choosable
    target = sc.add(1, BIG)
    sc.shields(1, VANILLA)
    sc.hand(1, DAMAGE_1)  # gives the defending player a decision in the battle action step
    st = sc.start()
    attack(st, dias)
    assert pending_kind(st) is DecisionKind.ACTION_STEP
    assert ap(st, target) == 2
    assert ap(st, enemy) == 5
    settle(st)
    assert ap(st, target) == 4


# ---------------------------------------------------------------------------------------------
# GD02-076 Buster Gundam


@pytest.mark.card("GD02-076")
def test_gd02_076_blocker_only_with_5_or_more_ap() -> None:
    sc = Scenario()
    boosted = sc.add(0, "GD02-076", pilot=PILOT_LV3)  # 5 AP
    plain = sc.add(0, "GD02-076")  # 4 AP
    st = sc.start()
    assert "Blocker" in keywords(st, boosted)
    assert "Blocker" not in keywords(st, plain)


@pytest.mark.card("GD02-076")
@pytest.mark.ruling("GD02-076:Q186")
def test_gd02_076_q186_block_stays_after_losing_blocker() -> None:
    sc = Scenario(active=1)
    sc.resources(0, 2)
    sc.resources(1, 3)
    buster = sc.add(0, "GD02-076", pilot=PILOT_LV3)  # 5/4
    attacker = sc.add(1, BIG)  # 4/3
    sc.hand(0, DAMAGE_1)
    sc.hand(1, AP_MINUS_3)
    shield = sc.shields(0, VANILLA)[0]
    st = sc.start()
    attack(st, attacker)
    block(st, buster)
    pass_(st)  # the defending player has priority first in the battle action step
    play(st, st.zones[1][Zone.HAND][0])
    assert pending_kind(st) is DecisionKind.ACTION_STEP
    assert ap(st, buster) == 2
    assert "Blocker" not in keywords(st, buster)
    settle(st)
    assert zone_of(st, shield) is Zone.SHIELD
    assert zone_of(st, buster) is Zone.TRASH  # still the attack target: took 4 damage
    assert st.cards[attacker].damage == 2


# ---------------------------------------------------------------------------------------------
# GD02-081 Methuss


@pytest.mark.card("GD02-081")
@pytest.mark.parametrize(("base", "expected"), [(WHITE_BASE, 0), (None, 2)])
def test_gd02_081_white_base_gives_enemy_ap_minus_2(base: str | None, expected: int) -> None:
    sc = Scenario()
    sc.resources(0, 2)
    methuss = sc.add(0, "GD02-081", Zone.HAND)
    sc.base(0, base)
    enemy = sc.add(1, VANILLA)
    st = sc.start()
    play(st, methuss)
    assert ap(st, enemy) == expected
    to_next_turn(st)
    assert ap(st, enemy) == 2


# ---------------------------------------------------------------------------------------------
# GD02-082 Gaelio's Schwalbe Graze


@pytest.mark.card("GD02-082")
def test_gd02_082_blocker_with_another_gjallarhorn_unit() -> None:
    sc = Scenario()
    gaelio = sc.add(0, "GD02-082")
    st = sc.start()
    assert "Blocker" not in keywords(st, gaelio)
    sc2 = Scenario()
    gaelio2 = sc2.add(0, "GD02-082")
    sc2.add(0, GJALLARHORN)
    st2 = sc2.start()
    assert "Blocker" in keywords(st2, gaelio2)


@pytest.mark.card("GD02-082")
@pytest.mark.ruling("GD02-082:Q187")
def test_gd02_082_q187_block_stays_after_losing_blocker() -> None:
    sc = Scenario(active=1)
    sc.resources(1, 4)
    gaelio = sc.add(0, "GD02-082")  # 3/3
    ally = sc.add(0, GJALLARHORN)  # 4 AP
    attacker = sc.add(1, BIG)  # 4/3
    sc.hand(1, "ST09-009")  # Giant Killing: destroy 1 active enemy Unit with 4 or less AP
    shield = sc.shields(0, VANILLA)[0]
    st = sc.start()
    attack(st, attacker)
    block(st, gaelio)
    assert st.pending is not None and st.pending.player == 1
    play(st, st.zones[1][Zone.HAND][0])
    assert zone_of(st, ally) is Zone.TRASH
    assert "Blocker" not in keywords(st, gaelio)
    settle(st)
    assert zone_of(st, shield) is Zone.SHIELD
    assert zone_of(st, gaelio) is Zone.TRASH
    assert zone_of(st, attacker) is Zone.TRASH


# ---------------------------------------------------------------------------------------------
# GD02-083 Graze Ritter (Ground Type)


@pytest.mark.card("GD02-083")
def test_gd02_083_destroyed_on_opponents_turn_sets_gjallarhorn_active() -> None:
    sc = Scenario(active=1)
    ritter = sc.add(0, "GD02-083", rested=True)  # 3/2
    ally = sc.add(0, GJALLARHORN, rested=True)
    enemy = sc.add(1, BIG)
    st = sc.start()
    attack(st, enemy, ritter)
    settle(st)
    assert zone_of(st, ritter) is Zone.TRASH
    assert not st.cards[ally].rested


@pytest.mark.card("GD02-083")
def test_gd02_083_destroyed_on_your_turn_does_nothing() -> None:
    sc = Scenario()
    ritter = sc.add(0, "GD02-083")
    ally = sc.add(0, GJALLARHORN, rested=True)
    enemy = sc.add(1, BIG, rested=True)
    st = sc.start()
    attack(st, ritter, enemy)
    settle(st)
    assert zone_of(st, ritter) is Zone.TRASH
    assert st.cards[ally].rested


# ---------------------------------------------------------------------------------------------
# GD02-085 Four Murasame

FOUR = "GD02-085"
PSYCHO_MA = "GD02-007"  # Psycho Gundam (MA Mode): links with a (Cyber-Newtype) Pilot, <Repair 2>


def _four(hand: tuple[str, ...], damage: int) -> tuple[Scenario, int]:
    sc = Scenario()
    sc.resources(0, 6)
    psycho = sc.add(0, PSYCHO_MA, pilot=FOUR, damage=damage)  # 5 HP
    sc.hand(0, *hand)
    return sc, psycho


def _play_first(st: GameState, number: str) -> None:
    play(st, next(u for u in st.zones[0][Zone.HAND] if V.cdef(st, u).card_number == number))


@pytest.mark.card("GD02-085")
def test_gd02_085_recovering_on_your_turn_draws_1_once_per_turn() -> None:
    sc, psycho = _four((RECOVER_3, RECOVER_3, VANILLA, VANILLA), damage=4)
    st = sc.start()
    _play_first(st, RECOVER_3)
    assert st.cards[psycho].damage == 1
    assert hand_size(st) == 4  # played 1, drew 1
    _play_first(st, RECOVER_3)
    assert st.cards[psycho].damage == 0
    assert hand_size(st) == 3  # 【Once per Turn】


@pytest.mark.card("GD02-085")
@pytest.mark.ruling("GD02-085:Q188")
def test_gd02_085_q188_undamaged_unit_does_not_recover() -> None:
    sc, _ = _four((RECOVER_3, VANILLA, VANILLA, VANILLA), damage=0)
    st = sc.start()
    _play_first(st, RECOVER_3)
    assert hand_size(st) == 3


@pytest.mark.card("GD02-085")
@pytest.mark.rule("13-2-13-1")
def test_gd02_085_large_hand_no_draw_and_once_per_turn_kept() -> None:
    sc, psycho = _four((RECOVER_3, RECOVER_3, VANILLA, VANILLA, VANILLA, VANILLA), damage=4)
    st = sc.start()
    _play_first(st, RECOVER_3)
    assert hand_size(st) == 5  # 5 cards in hand: nothing drawn
    _play_first(st, VANILLA)
    _play_first(st, VANILLA)
    _play_first(st, RECOVER_3)
    select(st, psycho)
    assert st.cards[psycho].damage == 0
    assert hand_size(st) == 3  # 2 left in hand, then drew 1


@pytest.mark.card("GD02-085")
@pytest.mark.rule("13-1-1-1")
def test_gd02_085_repair_at_end_of_your_turn_draws() -> None:
    sc, psycho = _four((), damage=2)
    st = sc.start()
    end_main(st)
    settle(st)
    assert st.active == 1
    assert st.cards[psycho].damage == 0
    assert hand_size(st) == 1


@pytest.mark.card("GD02-085")
def test_gd02_085_recovery_on_opponents_turn_does_not_draw() -> None:
    sc = Scenario(active=1)
    sc.resources(0, 2)
    psycho = sc.add(0, PSYCHO_MA, pilot=FOUR, damage=2)
    devotion = sc.add(0, "GD01-101", Zone.HAND)  # 【Action】1 friendly Link Unit recovers 3 HP
    enemy = sc.add(1, VANILLA)
    sc.shields(0, VANILLA)
    st = sc.start()
    attack(st, enemy)
    assert st.pending is not None and st.pending.player == 0
    play(st, devotion)
    assert st.cards[psycho].damage == 0
    assert hand_size(st) == 0


@pytest.mark.card("GD02-085")
@pytest.mark.rule("13-2-5-1")
def test_gd02_085_burst_adds_to_hand() -> None:
    sc = Scenario()
    unit = sc.add(0, VANILLA)
    (shield,) = sc.shields(1, FOUR)
    st = sc.start()
    attack(st, unit)
    settle(st)
    assert pending_kind(st) is DecisionKind.BURST
    yes(st)
    assert zone_of(st, shield) is Zone.HAND
