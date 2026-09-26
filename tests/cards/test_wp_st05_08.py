"""Card behaviour tests for starter decks ST05-ST08 (work package WP-ST05-08)."""

from __future__ import annotations

import pytest

from gcg_sim.engine import view as V
from gcg_sim.engine.state import GameState
from gcg_sim.engine.types import PLAYER_TARGET, ActionKind, DecisionKind, Zone
from gcg_sim.testkit import (
    Scenario,
    act,
    activate,
    ap,
    attack,
    block,
    end_main,
    has_action,
    hp,
    keywords,
    no,
    options,
    order,
    pass_all,
    play,
    select,
    to_next_turn,
    yes,
    zone_of,
)

A = ActionKind

FILLER = "GD01-060"  # Zaku Mariner: Lv2 cost1 2/2 (Zeon), red, no effects
NEUTRAL_PILOT = "GD01-089"  # Riddhe Marcenas: Lv3 cost1 +1/+1 (Earth Federation), no trigger
ENEMY_BLOCKER = "GD01-072"  # Launcher Strike Gundam: Lv4 3/4 <Blocker>
LV2_BLOCKER = "ST02-008"  # Aries: Lv2 2/1 <Blocker>
MIDAIR_MODIFICATIONS = "GD01-121"  # 【Burst】runs its 【Main】: set a rested Blocker active
DRAW_2_COMMAND = "GD01-100"  # A Show of Resolve: 【Main】Draw 2. (Lv4 cost3)
LV3_BREACH_2 = "GD01-030"  # Rick Dom: Lv3 3/3 <Breach 2>
GUNDAM_TOKEN = "T-001"  # [Gundam] Unit token: Lv0 3/3

BARBATOS_4TH = "ST05-001"
BARBATOS_2ND = "ST05-002"
MOBILE_WORKER = "ST05-003"  # Lv1 0/2 Tekkadan
GRAZE_CUSTOM = "ST05-004"  # Lv2 2/2 Tekkadan, vanilla
GUSION_REBAKE = "ST05-005"  # Lv4 3/4 Tekkadan, link Akihiro Altland
HYAKUREN = "ST05-006"  # Lv3 4/3 Teiwaz, vanilla
SCHWALBE_GRAZE = "ST05-007"
GRAZE_COMMANDER = "ST05-008"  # Lv3 3/2 Gjallarhorn <Blocker>
GRAZE = "ST05-009"  # Lv2 2/2 Gjallarhorn, vanilla
MIKAZUKI = "ST05-010"
AKIHIRO = "ST05-011"
MCGILLIS = "ST05-012"
IRON_AND_BLOOD = "ST05-013"
FATAL_STRIKE = "ST05-014"
ISARIBI = "ST05-015"
GQ_5 = "ST06-001"
GQ_4 = "ST06-002"
GAIA_RICK_DOM = "ST06-003"  # Lv2 2/2 Clan, link Gaia
RED_GUNDAM_BREACH = "ST06-005"
RED_GUNDAM = "ST06-006"  # Lv4 3/4 Clan, vanilla, link Shuji Itō
ORTEGA_RICK_DOM = "ST06-007"
SUGAI_GELGOOG = "ST06-008"  # Lv3 3/3 Clan, vanilla
AMATE = "ST06-009"
SHUJI = "ST06-010"
RUTHLESS_TACTICS = "ST06-011"
SCHOOLGIRL = "ST06-012"
FIERCE_UNITY = "ST06-013"
CLAN_BATTLE = "ST06-014"
KANEBAN = "ST06-015"
EXIA_5 = "ST07-001"
EXIA = "ST07-002"  # Lv4 4/3 CB, vanilla
VIRTUE = "ST07-003"  # Lv5 5/4 CB, vanilla
VIRTUE_BLOCKER = "ST07-004"
DYNAMES_4 = "ST07-005"
DYNAMES = "ST07-006"  # Lv3 3/3 CB, vanilla, link Lockon Stratos
KYRIOS = "ST07-007"
KYRIOS_FLIGHT = "ST07-008"  # Lv2 3/1 CB, vanilla, link Allelujah Haptism
SETSUNA = "ST07-009"
TIERIA = "ST07-010"
LOCKON = "ST07-011"
ALLELUJAH = "ST07-012"
ARMED_INTERVENTION = "ST07-013"
TACTICAL_VISIONARY = "ST07-014"
PTOLEMAIOS = "ST07-015"
XI_GUNDAM_9 = "ST08-001"
XI_GUNDAM_5 = "ST08-002"
MESSER_COMMANDER = "ST08-003"  # Lv4 4/3 Mafty, vanilla, link trait Mafty
MESSER_F01 = "ST08-004"
MINELAYER = "ST08-005"  # Lv3 4/3 Mafty, vanilla
PENELOPE_7 = "ST08-006"
PENELOPE = "ST08-007"  # Lv5 5/4 Earth Federation, blue, vanilla
GUSTAV_KARL = "ST08-008"
JEGAN = "ST08-009"
HATHAWAY = "ST08-010"
LANE_AIM = "ST08-011"
WORDS_FOR_HATHAWAY = "ST08-012"
LADY_LUCK = "ST08-013"
VALIANT = "ST08-014"
DAVAO = "ST08-015"


def resolve_orders(st: GameState) -> None:
    while st.pending is not None and st.pending.kind is DecisionKind.ORDER_TRIGGER:
        order(st)


def select_options(st: GameState) -> set[int]:
    return {o.a for o in options(st) if o.kind is A.SELECT}


def pending_kind(st: GameState) -> DecisionKind | None:
    return st.pending.kind if st.pending is not None else None


def hand_size(st: GameState, player: int) -> int:
    return len(st.zones[player][Zone.HAND])


# ---------------------------------------------------------------------------------------------
# shared 【Burst】 lines


@pytest.mark.rule("13-2-5-1", "3-3-9-1")
@pytest.mark.parametrize(
    "number",
    [
        pytest.param(MIKAZUKI, marks=pytest.mark.card("ST05-010")),
        pytest.param(AKIHIRO, marks=pytest.mark.card("ST05-011")),
        pytest.param(MCGILLIS, marks=pytest.mark.card("ST05-012")),
        pytest.param(AMATE, marks=pytest.mark.card("ST06-009")),
        pytest.param(SHUJI, marks=pytest.mark.card("ST06-010")),
        pytest.param(SETSUNA, marks=pytest.mark.card("ST07-009")),
        pytest.param(TIERIA, marks=pytest.mark.card("ST07-010")),
        pytest.param(LOCKON, marks=pytest.mark.card("ST07-011")),
        pytest.param(ALLELUJAH, marks=pytest.mark.card("ST07-012")),
        pytest.param(HATHAWAY, marks=pytest.mark.card("ST08-010")),
        pytest.param(LANE_AIM, marks=pytest.mark.card("ST08-011")),
    ],
)
def test_pilot_burst_adds_itself_to_hand(number: str) -> None:
    sc = Scenario()
    unit = sc.add(0, FILLER)
    shield, _ = sc.shields(1, number, FILLER)
    st = sc.start()
    attack(st, unit)
    pass_all(st)
    assert pending_kind(st) is DecisionKind.BURST
    yes(st)
    assert zone_of(st, shield) is Zone.HAND


@pytest.mark.rule("13-2-5-1")
@pytest.mark.parametrize(
    "number",
    [
        pytest.param(ISARIBI, marks=pytest.mark.card("ST05-015")),
        pytest.param(CLAN_BATTLE, marks=pytest.mark.card("ST06-014")),
        pytest.param(KANEBAN, marks=pytest.mark.card("ST06-015")),
        pytest.param(PTOLEMAIOS, marks=pytest.mark.card("ST07-015")),
        pytest.param(VALIANT, marks=pytest.mark.card("ST08-014")),
        pytest.param(DAVAO, marks=pytest.mark.card("ST08-015")),
    ],
)
def test_base_burst_deploys_it_and_its_deploy_adds_a_shield(number: str) -> None:
    sc = Scenario()
    unit = sc.add(0, FILLER)
    base_shield, next_shield = sc.shields(1, number, FILLER)
    st = sc.start()
    attack(st, unit)
    pass_all(st)
    assert pending_kind(st) is DecisionKind.BURST
    yes(st)
    assert zone_of(st, base_shield) is Zone.BASE
    assert zone_of(st, next_shield) is Zone.HAND


@pytest.mark.parametrize(
    "number",
    [
        pytest.param(ISARIBI, marks=pytest.mark.card("ST05-015")),
        pytest.param(CLAN_BATTLE, marks=pytest.mark.card("ST06-014")),
        pytest.param(KANEBAN, marks=pytest.mark.card("ST06-015")),
        pytest.param(PTOLEMAIOS, marks=pytest.mark.card("ST07-015")),
        pytest.param(DAVAO, marks=pytest.mark.card("ST08-015")),
    ],
)
def test_base_deploy_adds_top_shield_to_hand(number: str) -> None:
    sc = Scenario()
    sc.resources(0, 4)
    base = sc.add(0, number, Zone.HAND)
    top, second = sc.shields(0, FILLER, FILLER)
    st = sc.start()
    play(st, base)
    assert zone_of(st, base) is Zone.BASE
    assert zone_of(st, top) is Zone.HAND
    assert zone_of(st, second) is Zone.SHIELD


# ---------------------------------------------------------------------------------------------
# ST05


@pytest.mark.card("ST05-001")
def test_st05_001_deploy_damages_and_pumps_another_unit() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    barbatos = sc.add(0, BARBATOS_4TH, Zone.HAND)
    graze = sc.add(0, GRAZE_CUSTOM)
    st = sc.start()
    play(st, barbatos)
    assert st.cards[graze].damage == 1
    assert ap(st, graze) == 3
    assert st.cards[barbatos].damage == 0
    to_next_turn(st)
    assert ap(st, graze) == 2


@pytest.mark.card("ST05-001")
def test_st05_001_deploy_never_chooses_itself() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    barbatos = sc.add(0, BARBATOS_4TH, Zone.HAND)
    st = sc.start()
    play(st, barbatos)
    assert st.cards[barbatos].damage == 0
    assert "Suppression" not in keywords(st, barbatos)


@pytest.mark.card("ST05-001")
@pytest.mark.rule("13-1-7-1")
def test_st05_001_damaged_gains_suppression_and_destroys_two_shields() -> None:
    sc = Scenario()
    barbatos = sc.add(0, BARBATOS_4TH, damage=1)
    s1, s2, s3 = sc.shields(1, FILLER, FILLER, FILLER)
    st = sc.start()
    assert "Suppression" in keywords(st, barbatos)
    attack(st, barbatos)
    pass_all(st)
    assert (zone_of(st, s1), zone_of(st, s2), zone_of(st, s3)) == (
        Zone.TRASH,
        Zone.TRASH,
        Zone.SHIELD,
    )


@pytest.mark.card("ST05-001")
def test_st05_001_undamaged_has_no_suppression() -> None:
    sc = Scenario()
    barbatos = sc.add(0, BARBATOS_4TH)
    s1, s2 = sc.shields(1, FILLER, FILLER)
    st = sc.start()
    assert "Suppression" not in keywords(st, barbatos)
    attack(st, barbatos)
    pass_all(st)
    assert (zone_of(st, s1), zone_of(st, s2)) == (Zone.TRASH, Zone.SHIELD)


@pytest.mark.card("ST05-002")
@pytest.mark.rule("10-1-5-3")
def test_st05_002_ap_plus_two_only_while_damaged() -> None:
    sc = Scenario()
    damaged = sc.add(0, BARBATOS_2ND, damage=1)
    healthy = sc.add(0, BARBATOS_2ND)
    st = sc.start()
    assert ap(st, damaged) == 4
    assert ap(st, healthy) == 2


@pytest.mark.card("ST05-003")
@pytest.mark.rule("10-1-7-2")
def test_st05_003_rest_to_damage_and_pump_a_friendly_unit() -> None:
    sc = Scenario()
    worker = sc.add(0, MOBILE_WORKER)
    graze = sc.add(0, GRAZE_CUSTOM)
    sc.add(1, FILLER)
    st = sc.start()
    activate(st, worker)
    assert select_options(st) == {worker, graze}
    select(st, graze)
    assert st.cards[worker].rested
    assert st.cards[graze].damage == 1
    assert ap(st, graze) == 3
    assert not has_action(st, A.ACTIVATE, worker)


@pytest.mark.card("ST05-003")
def test_st05_003_can_choose_itself() -> None:
    sc = Scenario()
    worker = sc.add(0, MOBILE_WORKER)
    st = sc.start()
    activate(st, worker)
    assert st.cards[worker].damage == 1
    assert ap(st, worker) == 1


@pytest.mark.card("ST05-005")
@pytest.mark.rule("13-2-8-1")
def test_st05_005_destroyed_rests_enemy_unit_with_4_or_less_ap() -> None:
    sc = Scenario()
    gusion = sc.add(0, GUSION_REBAKE, damage=1)
    victim = sc.add(1, HYAKUREN, rested=True)
    low_ap = sc.add(1, FILLER)
    high_ap = sc.add(1, VIRTUE)
    st = sc.start()
    attack(st, gusion, victim)
    pass_all(st)
    assert zone_of(st, gusion) is Zone.TRASH
    assert zone_of(st, victim) is Zone.TRASH
    assert st.cards[low_ap].rested
    assert not st.cards[high_ap].rested


@pytest.mark.card("ST05-005")
def test_st05_005_destroyed_cannot_rest_high_ap_unit() -> None:
    sc = Scenario()
    gusion = sc.add(0, GUSION_REBAKE, damage=1)
    victim = sc.add(1, HYAKUREN, rested=True)
    high_ap = sc.add(1, VIRTUE)
    st = sc.start()
    attack(st, gusion, victim)
    pass_all(st)
    assert zone_of(st, gusion) is Zone.TRASH
    assert not st.cards[high_ap].rested


@pytest.mark.card("ST05-007")
@pytest.mark.rule("13-2-9-1")
def test_st05_007_when_paired_gives_low_level_enemy_ap_minus_two() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    schwalbe = sc.add(0, SCHWALBE_GRAZE)
    pilot = sc.add(0, NEUTRAL_PILOT, Zone.HAND)
    low = sc.add(1, FILLER)
    high = sc.add(1, VIRTUE)
    st = sc.start()
    assert keywords(st, schwalbe).get("Blocker") == 1
    play(st, pilot, onto=schwalbe)
    assert ap(st, low) == 0
    assert ap(st, high) == 5
    to_next_turn(st)
    assert ap(st, low) == 2


@pytest.mark.card("ST05-008")
@pytest.mark.rule("13-1-4-1")
def test_st05_008_blocker_can_block() -> None:
    sc = Scenario(active=1)
    commander = sc.add(0, GRAZE_COMMANDER)
    (shield,) = sc.shields(0, FILLER)
    attacker = sc.add(1, FILLER)
    st = sc.start()
    assert keywords(st, commander).get("Blocker") == 1
    attack(st, attacker)
    assert pending_kind(st) is DecisionKind.BLOCK
    block(st, commander)
    pass_all(st)
    assert zone_of(st, attacker) is Zone.TRASH
    assert zone_of(st, commander) is Zone.TRASH
    assert zone_of(st, shield) is Zone.SHIELD


@pytest.mark.card("ST05-010")
@pytest.mark.rule("13-2-9-1", "3-3-9-2")
def test_st05_010_when_paired_damages_a_friendly_and_an_enemy_unit() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    graze = sc.add(0, GRAZE_CUSTOM)
    mikazuki = sc.add(0, MIKAZUKI, Zone.HAND)
    enemy = sc.add(1, SUGAI_GELGOOG)
    st = sc.start()
    play(st, mikazuki, onto=graze)
    assert st.cards[graze].damage == 1
    assert st.cards[enemy].damage == 1


@pytest.mark.card("ST05-010")
@pytest.mark.ruling("ST05-010:Q168")
@pytest.mark.rule("10-2-2")
def test_st05_010_does_nothing_without_an_enemy_unit() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    graze = sc.add(0, GRAZE_CUSTOM)
    other = sc.add(0, GRAZE)
    mikazuki = sc.add(0, MIKAZUKI, Zone.HAND)
    st = sc.start()
    play(st, mikazuki, onto=graze)
    assert pending_kind(st) is DecisionKind.MAIN
    assert st.cards[graze].damage == 0
    assert st.cards[other].damage == 0


@pytest.mark.card("ST05-011")
@pytest.mark.rule("13-2-12-1")
def test_st05_011_linked_battle_kill_returns_tekkadan_card_from_trash() -> None:
    sc = Scenario()
    gusion = sc.add(0, GUSION_REBAKE, pilot=AKIHIRO)
    low, high_lv, not_tekkadan = sc.trash(0, GRAZE_CUSTOM, BARBATOS_2ND, HYAKUREN)
    victim = sc.add(1, FILLER, rested=True)
    st = sc.start()
    attack(st, gusion, victim)
    pass_all(st)
    assert zone_of(st, victim) is Zone.TRASH
    assert zone_of(st, low) is Zone.HAND
    assert zone_of(st, high_lv) is Zone.TRASH
    assert zone_of(st, not_tekkadan) is Zone.TRASH


@pytest.mark.card("ST05-011")
@pytest.mark.ruling("ST05-011:Q169")
@pytest.mark.rule("10-1-6-4")
def test_st05_011_triggers_when_both_units_are_destroyed() -> None:
    sc = Scenario()
    gusion = sc.add(0, GUSION_REBAKE, pilot=AKIHIRO, damage=3)
    (low,) = sc.trash(0, GRAZE_CUSTOM)
    victim = sc.add(1, HYAKUREN, rested=True)
    st = sc.start()
    attack(st, gusion, victim)
    pass_all(st)
    resolve_orders(st)
    assert zone_of(st, gusion) is Zone.TRASH
    assert zone_of(st, victim) is Zone.TRASH
    assert zone_of(st, low) is Zone.HAND


@pytest.mark.card("ST05-011")
def test_st05_011_needs_link() -> None:
    sc = Scenario()
    barbatos = sc.add(0, BARBATOS_2ND, pilot=AKIHIRO)
    (low,) = sc.trash(0, GRAZE_CUSTOM)
    victim = sc.add(1, FILLER, rested=True)
    st = sc.start()
    attack(st, barbatos, victim)
    pass_all(st)
    assert zone_of(st, victim) is Zone.TRASH
    assert zone_of(st, low) is Zone.TRASH


@pytest.mark.card("ST05-011")
def test_st05_011_not_during_opponents_turn() -> None:
    sc = Scenario(active=1)
    gusion = sc.add(0, GUSION_REBAKE, pilot=AKIHIRO, rested=True)
    (low,) = sc.trash(0, GRAZE_CUSTOM)
    attacker = sc.add(1, FILLER)
    st = sc.start()
    attack(st, attacker, gusion)
    pass_all(st)
    assert zone_of(st, attacker) is Zone.TRASH
    assert zone_of(st, low) is Zone.TRASH


@pytest.mark.card("ST05-012")
def test_st05_012_rests_enemy_with_3_or_less_hp_with_two_other_units() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    graze = sc.add(0, GRAZE)
    sc.add(0, GRAZE_CUSTOM)
    sc.add(0, GRAZE_COMMANDER)
    mcgillis = sc.add(0, MCGILLIS, Zone.HAND)
    low_hp = sc.add(1, FILLER)
    high_hp = sc.add(1, VIRTUE)
    st = sc.start()
    play(st, mcgillis, onto=graze)
    assert st.cards[low_hp].rested
    assert not st.cards[high_hp].rested


@pytest.mark.card("ST05-012")
def test_st05_012_paired_unit_is_not_one_of_the_other_units() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    graze = sc.add(0, GRAZE)
    sc.add(0, GRAZE_COMMANDER)
    sc.add(0, FILLER)
    mcgillis = sc.add(0, MCGILLIS, Zone.HAND)
    low_hp = sc.add(1, FILLER)
    st = sc.start()
    play(st, mcgillis, onto=graze)
    assert not st.cards[low_hp].rested


@pytest.mark.card("ST05-013")
def test_st05_013_main_damages_and_pumps_own_unit() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    graze = sc.add(0, GRAZE_CUSTOM)
    cmd = sc.add(0, IRON_AND_BLOOD, Zone.HAND)
    st = sc.start()
    play(st, cmd)
    assert st.cards[graze].damage == 1
    assert ap(st, graze) == 5
    assert zone_of(st, cmd) is Zone.TRASH


@pytest.mark.card("ST05-013")
@pytest.mark.rule("10-1-8-1-1")
def test_st05_013_needs_a_friendly_unit() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    cmd = sc.add(0, IRON_AND_BLOOD, Zone.HAND)
    sc.add(1, FILLER)
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, cmd)


@pytest.mark.card("ST05-014")
def test_st05_014_main_destroys_enemy_lv3_or_lower() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    cmd = sc.add(0, FATAL_STRIKE, Zone.HAND)
    low = sc.add(1, SUGAI_GELGOOG)
    high = sc.add(1, RED_GUNDAM)
    st = sc.start()
    play(st, cmd)
    assert zone_of(st, low) is Zone.TRASH
    assert zone_of(st, high) is Zone.BATTLE


@pytest.mark.card("ST05-014")
@pytest.mark.rule("10-1-8-1-1")
def test_st05_014_not_playable_without_a_legal_target() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    cmd = sc.add(0, FATAL_STRIKE, Zone.HAND)
    sc.add(1, RED_GUNDAM)
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, cmd)


@pytest.mark.card("ST05-014")
@pytest.mark.rule("13-2-5-1")
def test_st05_014_burst_deals_one_damage_to_enemy_unit() -> None:
    sc = Scenario()
    attacker = sc.add(0, SUGAI_GELGOOG)
    sc.shields(1, FATAL_STRIKE, FILLER)
    st = sc.start()
    attack(st, attacker)
    pass_all(st)
    yes(st)
    assert st.cards[attacker].damage == 1


@pytest.mark.card("ST05-015")
@pytest.mark.rule("10-1-7-2")
def test_st05_015_rest_to_pump_a_damaged_unit() -> None:
    sc = Scenario()
    isaribi = sc.base(0, ISARIBI)
    damaged = sc.add(0, GRAZE_CUSTOM, damage=1)
    healthy = sc.add(0, GRAZE)
    st = sc.start()
    activate(st, isaribi)
    assert st.cards[isaribi].rested
    assert ap(st, damaged) == 4
    assert ap(st, healthy) == 2


@pytest.mark.card("ST05-015")
def test_st05_015_cannot_activate_without_a_damaged_unit() -> None:
    sc = Scenario()
    isaribi = sc.base(0, ISARIBI)
    sc.add(0, GRAZE)
    st = sc.start()
    assert not has_action(st, A.ACTIVATE, isaribi)


@pytest.mark.card("ST05-015")
@pytest.mark.ruling("ST05-015:Q170")
@pytest.mark.rule("10-3-3")
def test_st05_015_ap_bonus_stays_after_recovery() -> None:
    sc = Scenario()
    isaribi = sc.base(0, ISARIBI)
    dynames = sc.add(0, DYNAMES_4, damage=1)
    victim = sc.add(1, MOBILE_WORKER, rested=True)
    st = sc.start()
    activate(st, isaribi)
    assert ap(st, dynames) == 4
    attack(st, dynames, victim)
    pass_all(st)
    assert zone_of(st, victim) is Zone.TRASH
    assert st.cards[dynames].damage == 0
    assert ap(st, dynames) == 4


# ---------------------------------------------------------------------------------------------
# ST06


@pytest.mark.card("ST06-001")
@pytest.mark.rule("13-2-11-1")
def test_st06_001_when_linked_with_another_clan_unit_gains_first_strike() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    gq = sc.add(0, GQ_5)
    sc.add(0, RED_GUNDAM)
    amate = sc.add(0, AMATE, Zone.HAND)
    sc.deck(0, FILLER)
    st = sc.start()
    assert "First Strike" not in keywords(st, gq)
    play(st, amate, onto=gq)
    resolve_orders(st)
    assert keywords(st, gq).get("First Strike") == 1
    to_next_turn(st)
    assert "First Strike" not in keywords(st, gq)


@pytest.mark.card("ST06-001")
def test_st06_001_no_first_strike_without_another_clan_unit() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    gq = sc.add(0, GQ_5)
    sc.add(0, FILLER)
    amate = sc.add(0, AMATE, Zone.HAND)
    sc.deck(0, FILLER)
    st = sc.start()
    play(st, amate, onto=gq)
    resolve_orders(st)
    assert "First Strike" not in keywords(st, gq)


@pytest.mark.card("ST06-002")
def test_st06_002_deploy_with_another_clan_unit_damages_enemy() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    gq = sc.add(0, GQ_4, Zone.HAND)
    sc.add(0, RED_GUNDAM)
    enemy = sc.add(1, FILLER)
    st = sc.start()
    play(st, gq)
    assert st.cards[enemy].damage == 1


@pytest.mark.card("ST06-002")
def test_st06_002_deploy_without_another_clan_unit_does_nothing() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    gq = sc.add(0, GQ_4, Zone.HAND)
    sc.add(0, FILLER)
    enemy = sc.add(1, FILLER)
    st = sc.start()
    play(st, gq)
    assert st.cards[enemy].damage == 0


@pytest.mark.card("ST06-003")
@pytest.mark.rule("13-1-3-1")
def test_st06_003_support_gives_another_unit_ap_plus_one() -> None:
    sc = Scenario()
    rick_dom = sc.add(0, GAIA_RICK_DOM)
    other = sc.add(0, FILLER)
    st = sc.start()
    activate(st, rick_dom)
    assert st.cards[rick_dom].rested
    assert ap(st, other) == 3


@pytest.mark.card("ST06-005")
@pytest.mark.rule("13-1-2-1")
def test_st06_005_attack_pumps_one_to_two_clan_units_and_breach() -> None:
    sc = Scenario()
    red = sc.add(0, RED_GUNDAM_BREACH)
    other = sc.add(0, RED_GUNDAM)
    zeon = sc.add(0, FILLER)
    victim = sc.add(1, FILLER, rested=True)
    top, second = sc.shields(1, FILLER, FILLER)
    st = sc.start()
    assert keywords(st, red).get("Breach") == 1
    attack(st, red, victim)
    assert select_options(st) == {red, other}
    select(st, red, other)
    assert ap(st, red) == 6
    assert ap(st, other) == 5
    assert ap(st, zeon) == 2
    pass_all(st)
    assert zone_of(st, victim) is Zone.TRASH
    assert zone_of(st, top) is Zone.TRASH
    assert zone_of(st, second) is Zone.SHIELD


@pytest.mark.card("ST06-005")
def test_st06_005_may_choose_only_one() -> None:
    sc = Scenario()
    red = sc.add(0, RED_GUNDAM_BREACH)
    other = sc.add(0, RED_GUNDAM)
    sc.shields(1, FILLER)
    st = sc.start()
    attack(st, red)
    select(st, other)
    assert ap(st, other) == 5
    assert ap(st, red) == 4


@pytest.mark.card("ST06-007")
def test_st06_007_other_clan_unit_may_attack_active_low_ap_enemy() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    ortega = sc.add(0, ORTEGA_RICK_DOM, Zone.HAND)
    red = sc.add(0, RED_GUNDAM)
    low = sc.add(1, SUGAI_GELGOOG)
    high = sc.add(1, HYAKUREN)
    st = sc.start()
    assert not has_action(st, A.ATTACK, red, low)
    play(st, ortega)
    assert has_action(st, A.ATTACK, red, low)
    assert not has_action(st, A.ATTACK, red, high)
    assert not has_action(st, A.ATTACK, ortega, low)


@pytest.mark.card("ST06-009")
@pytest.mark.rule("13-2-11-1")
def test_st06_009_when_linked_may_add_clan_top_card() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    gq = sc.add(0, GQ_4)
    amate = sc.add(0, AMATE, Zone.HAND)
    sc.deck(0, RED_GUNDAM)
    st = sc.start()
    top = st.zones[0][Zone.DECK][0]
    play(st, amate, onto=gq)
    assert select_options(st) == {top}
    select(st, top)
    assert zone_of(st, top) is Zone.HAND


@pytest.mark.card("ST06-009")
def test_st06_009_declined_clan_card_goes_to_bottom() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    gq = sc.add(0, GQ_4)
    amate = sc.add(0, AMATE, Zone.HAND)
    sc.deck(0, RED_GUNDAM)
    st = sc.start()
    top = st.zones[0][Zone.DECK][0]
    play(st, amate, onto=gq)
    select(st, done=True)
    assert st.zones[0][Zone.DECK][-1] == top


@pytest.mark.card("ST06-009")
def test_st06_009_non_clan_top_card_goes_to_bottom() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    gq = sc.add(0, GQ_4)
    amate = sc.add(0, AMATE, Zone.HAND)
    sc.deck(0, GRAZE, RED_GUNDAM)
    st = sc.start()
    top = st.zones[0][Zone.DECK][0]
    play(st, amate, onto=gq)
    assert pending_kind(st) is DecisionKind.MAIN
    assert st.zones[0][Zone.DECK][-1] == top


@pytest.mark.card("ST06-009")
def test_st06_009_needs_link() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    red = sc.add(0, RED_GUNDAM)
    amate = sc.add(0, AMATE, Zone.HAND)
    sc.deck(0, RED_GUNDAM)
    st = sc.start()
    top = st.zones[0][Zone.DECK][0]
    play(st, amate, onto=red)
    assert pending_kind(st) is DecisionKind.MAIN
    assert st.zones[0][Zone.DECK][0] == top


@pytest.mark.card("ST06-010")
@pytest.mark.rule("13-2-12-1")
def test_st06_010_linked_attack_looks_and_may_bottom_top_card() -> None:
    sc = Scenario()
    red = sc.add(0, RED_GUNDAM, pilot=SHUJI)
    sc.shields(1, FILLER)
    sc.deck(0, GRAZE)
    st = sc.start()
    top = st.zones[0][Zone.DECK][0]
    attack(st, red)
    assert pending_kind(st) is DecisionKind.ARRANGE
    act(st, A.SELECT, 1)
    assert st.zones[0][Zone.DECK][-1] == top


@pytest.mark.card("ST06-010")
def test_st06_010_not_linked_no_look() -> None:
    sc = Scenario()
    gq = sc.add(0, GQ_4, pilot=SHUJI)
    sc.shields(1, FILLER)
    st = sc.start()
    attack(st, gq)
    assert pending_kind(st) is not DecisionKind.ARRANGE


@pytest.mark.card("ST06-011")
def test_st06_011_command_pumps_up_to_two_clan_units() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    a = sc.add(0, RED_GUNDAM)
    b = sc.add(0, SUGAI_GELGOOG)
    zeon = sc.add(0, FILLER)
    cmd = sc.add(0, RUTHLESS_TACTICS, Zone.HAND)
    st = sc.start()
    play(st, cmd)
    assert select_options(st) == {a, b}
    select(st, a, b)
    assert (ap(st, a), ap(st, b), ap(st, zeon)) == (5, 5, 2)


@pytest.mark.card("ST06-011")
@pytest.mark.rule("3-4-6-2")
def test_st06_011_pilot_gaia_links_gaias_rick_dom() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    rick_dom = sc.add(0, GAIA_RICK_DOM)
    cmd = sc.add(0, RUTHLESS_TACTICS, Zone.HAND)
    st = sc.start()
    play(st, cmd, onto=rick_dom)
    assert zone_of(st, cmd) is Zone.PAIRED
    assert V.is_linked(V.derived(st), rick_dom)
    assert (ap(st, rick_dom), hp(st, rick_dom)) == (3, 2)


@pytest.mark.card("ST06-012")
def test_st06_012_may_add_clan_unit_or_pilot_and_bottoms_the_rest() -> None:
    sc = Scenario()
    sc.resources(0, 1)
    cmd = sc.add(0, SCHOOLGIRL, Zone.HAND)
    sc.deck(0, FILLER, AMATE, RUTHLESS_TACTICS, GRAZE)
    st = sc.start()
    zaku, amate, tactics, fourth = st.zones[0][Zone.DECK][:4]
    play(st, cmd)
    yes(st)
    assert zone_of(st, amate) is Zone.HAND
    assert set(st.zones[0][Zone.DECK][-2:]) == {zaku, tactics}
    assert st.zones[0][Zone.DECK][0] == fourth


@pytest.mark.card("ST06-012")
def test_st06_012_declining_returns_all_three() -> None:
    sc = Scenario()
    sc.resources(0, 1)
    cmd = sc.add(0, SCHOOLGIRL, Zone.HAND)
    sc.deck(0, FILLER, AMATE, RED_GUNDAM)
    st = sc.start()
    looked = st.zones[0][Zone.DECK][:3]
    play(st, cmd)
    no(st)
    assert set(st.zones[0][Zone.DECK][-3:]) == set(looked)


@pytest.mark.card("ST06-013")
@pytest.mark.rule("9-3-1")
def test_st06_013_clan_unit_takes_no_battle_damage_from_lv2_enemy() -> None:
    sc = Scenario(active=1)
    sc.resources(0, 3)
    red = sc.add(0, RED_GUNDAM, rested=True)
    cmd = sc.add(0, FIERCE_UNITY, Zone.HAND)
    attacker = sc.add(1, FILLER)
    st = sc.start()
    attack(st, attacker, red)
    assert pending_kind(st) is DecisionKind.ACTION_STEP
    play(st, cmd)
    pass_all(st)
    assert st.cards[red].damage == 0
    assert zone_of(st, attacker) is Zone.TRASH


@pytest.mark.card("ST06-013")
def test_st06_013_does_not_stop_lv3_enemy() -> None:
    sc = Scenario(active=1)
    sc.resources(0, 3)
    red = sc.add(0, RED_GUNDAM, rested=True)
    cmd = sc.add(0, FIERCE_UNITY, Zone.HAND)
    attacker = sc.add(1, SUGAI_GELGOOG)
    st = sc.start()
    attack(st, attacker, red)
    play(st, cmd)
    pass_all(st)
    assert st.cards[red].damage == 3


@pytest.mark.card("ST06-013")
@pytest.mark.rule("10-1-8-1")
def test_st06_013_action_only_but_pairs_as_ortega() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    ortega = sc.add(0, ORTEGA_RICK_DOM)
    cmd = sc.add(0, FIERCE_UNITY, Zone.HAND)
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, cmd)
    play(st, cmd, onto=ortega)
    assert V.is_linked(V.derived(st), ortega)


@pytest.mark.card("ST06-014")
def test_st06_014_with_clan_link_unit_pumps_a_friendly_unit() -> None:
    sc = Scenario()
    clan_battle = sc.base(0, CLAN_BATTLE)
    red = sc.add(0, RED_GUNDAM, pilot=SHUJI)
    zaku = sc.add(0, FILLER)
    st = sc.start()
    activate(st, clan_battle)
    assert select_options(st) == {red, zaku}
    select(st, zaku)
    assert st.cards[clan_battle].rested
    assert ap(st, zaku) == 4


@pytest.mark.card("ST06-014")
def test_st06_014_without_clan_link_unit_does_nothing() -> None:
    sc = Scenario()
    clan_battle = sc.base(0, CLAN_BATTLE)
    red = sc.add(0, RED_GUNDAM)
    st = sc.start()
    activate(st, clan_battle)
    assert st.cards[clan_battle].rested
    assert ap(st, red) == 3


@pytest.mark.card("ST06-015")
@pytest.mark.rule("13-2-13-1")
def test_st06_015_linking_clan_unit_gains_breach_3_once_per_turn() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    sc.base(0, KANEBAN)
    red = sc.add(0, RED_GUNDAM)
    red_breach = sc.add(0, RED_GUNDAM_BREACH)
    shuji_a, shuji_b = sc.hand(0, SHUJI, SHUJI)
    st = sc.start()
    play(st, shuji_a, onto=red)
    assert keywords(st, red).get("Breach") == 3
    play(st, shuji_b, onto=red_breach)
    assert keywords(st, red_breach).get("Breach") == 1
    to_next_turn(st)
    assert "Breach" not in keywords(st, red)


@pytest.mark.card("ST06-015")
def test_st06_015_pairing_without_link_does_not_trigger() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    sc.base(0, KANEBAN)
    red = sc.add(0, RED_GUNDAM)
    amate = sc.add(0, AMATE, Zone.HAND)
    st = sc.start()
    play(st, amate, onto=red)
    assert "Breach" not in keywords(st, red)


@pytest.mark.card("ST06-015")
def test_st06_015_breach_3_from_kaneban_hits_the_shield_area() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    sc.base(0, KANEBAN)
    red = sc.add(0, RED_GUNDAM)
    shuji = sc.add(0, SHUJI, Zone.HAND)
    victim = sc.add(1, FILLER, rested=True)
    enemy_base = sc.base(1)
    sc.deck(0, GRAZE)
    st = sc.start()
    play(st, shuji, onto=red)
    attack(st, red, victim)
    act(st, A.SELECT, 0)
    pass_all(st)
    assert zone_of(st, victim) is Zone.TRASH
    assert zone_of(st, enemy_base) is not Zone.BASE


# ---------------------------------------------------------------------------------------------
# ST07


def _end_turn_to_resource_choice(st: GameState) -> None:
    end_main(st)
    pass_all(st)


@pytest.mark.card("ST07-001")
@pytest.mark.rule("7-6-4-1")
def test_st07_001_end_of_turn_sets_a_resource_active_with_7_cb_cards() -> None:
    sc = Scenario()
    sc.add(0, EXIA_5)
    sc.trash(0, *([EXIA] * 7))
    r1, r2, r3 = sc.resources(0, 3, rested=3)
    st = sc.start()
    _end_turn_to_resource_choice(st)
    assert pending_kind(st) is DecisionKind.SELECT
    select(st, r2)
    assert st.active == 1
    assert (st.cards[r1].rested, st.cards[r2].rested, st.cards[r3].rested) == (True, False, True)


@pytest.mark.card("ST07-001")
def test_st07_001_end_of_turn_needs_7_cb_cards() -> None:
    sc = Scenario()
    sc.add(0, EXIA_5)
    sc.trash(0, *([EXIA] * 6), FILLER)
    resources = sc.resources(0, 2, rested=2)
    st = sc.start()
    _end_turn_to_resource_choice(st)
    assert st.active == 1
    assert pending_kind(st) is DecisionKind.MAIN
    assert all(st.cards[r].rested for r in resources)


@pytest.mark.card("ST07-001")
@pytest.mark.ruling("ST07-001:Q198")
@pytest.mark.rule("1-3-2-1")
def test_st07_001_may_choose_an_active_resource() -> None:
    sc = Scenario()
    sc.add(0, EXIA_5)
    sc.trash(0, *([EXIA] * 7))
    rested, active = sc.resources(0, 2, rested=1)
    st = sc.start()
    _end_turn_to_resource_choice(st)
    assert select_options(st) == {rested, active}
    select(st, active)
    assert st.active == 1
    assert st.cards[rested].rested
    assert not st.cards[active].rested


@pytest.mark.card("ST07-001")
@pytest.mark.ruling("ST07-001:Q199")
def test_st07_001_may_choose_an_ex_resource() -> None:
    sc = Scenario()
    sc.add(0, EXIA_5)
    sc.trash(0, *([EXIA] * 7))
    normal, ex = sc.resources(0, 1, rested=1, ex=1)
    st = sc.start()
    _end_turn_to_resource_choice(st)
    assert ex in select_options(st)
    select(st, ex)
    assert st.active == 1
    assert st.cards[normal].rested


@pytest.mark.card("ST07-001")
def test_st07_001_when_paired_mills_two_and_draws_on_cb() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    exia = sc.add(0, EXIA_5)
    pilot = sc.add(0, NEUTRAL_PILOT, Zone.HAND)
    sc.deck(0, FILLER, EXIA, GRAZE)
    st = sc.start()
    first, second, third = st.zones[0][Zone.DECK][:3]
    play(st, pilot, onto=exia)
    assert (zone_of(st, first), zone_of(st, second)) == (Zone.TRASH, Zone.TRASH)
    assert zone_of(st, third) is Zone.HAND


@pytest.mark.card("ST07-001")
def test_st07_001_when_paired_no_draw_without_cb() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    exia = sc.add(0, EXIA_5)
    pilot = sc.add(0, NEUTRAL_PILOT, Zone.HAND)
    sc.deck(0, FILLER, GRAZE, EXIA)
    st = sc.start()
    first, second, third = st.zones[0][Zone.DECK][:3]
    play(st, pilot, onto=exia)
    assert (zone_of(st, first), zone_of(st, second)) == (Zone.TRASH, Zone.TRASH)
    assert zone_of(st, third) is Zone.DECK
    assert hand_size(st, 0) == 0


@pytest.mark.card("ST07-004")
def test_st07_004_blocker_only_while_you_have_a_cb_pilot() -> None:
    sc = Scenario()
    virtue = sc.add(0, VIRTUE_BLOCKER)
    sc.add(0, GRAZE, pilot=NEUTRAL_PILOT)
    st = sc.start()
    assert "Blocker" not in keywords(st, virtue)
    sc2 = Scenario()
    virtue2 = sc2.add(0, VIRTUE_BLOCKER)
    sc2.add(0, GRAZE, pilot=SETSUNA)
    st2 = sc2.start()
    assert keywords(st2, virtue2).get("Blocker") == 1


@pytest.mark.card("ST07-004", "ST05-013")
@pytest.mark.ruling("ST07-004:Q200")
@pytest.mark.rule("9-3-1")
def test_st07_004_block_stands_after_losing_blocker() -> None:
    sc = Scenario(active=1)
    sc.resources(0, 2)
    virtue = sc.add(0, VIRTUE_BLOCKER)
    kyrios = sc.add(0, KYRIOS_FLIGHT, pilot=SETSUNA, damage=1)
    cmd = sc.add(0, IRON_AND_BLOOD, Zone.HAND)
    attacker = sc.add(1, FILLER)
    st = sc.start()
    attack(st, attacker)
    block(st, virtue)
    play(st, cmd)
    select(st, kyrios)
    assert zone_of(st, kyrios) is Zone.TRASH
    assert "Blocker" not in keywords(st, virtue)
    pass_all(st)
    assert zone_of(st, attacker) is Zone.TRASH
    assert st.cards[virtue].damage == 2
    assert len(st.zones[0][Zone.SHIELD]) == 0
    assert st.winner is None


@pytest.mark.card("ST07-005")
@pytest.mark.rule("13-2-12-1")
def test_st07_005_link_ap_and_recovers_after_battle_kill() -> None:
    sc = Scenario()
    dynames = sc.add(0, DYNAMES_4, pilot=LOCKON, damage=1)
    victim = sc.add(1, FILLER, rested=True)
    st = sc.start()
    assert ap(st, dynames) == 5
    attack(st, dynames, victim)
    pass_all(st)
    assert zone_of(st, victim) is Zone.TRASH
    assert st.cards[dynames].damage == 1


@pytest.mark.card("ST07-005")
def test_st07_005_unlinked_ap_and_no_recovery_on_opponents_turn() -> None:
    sc = Scenario(active=1)
    dynames = sc.add(0, DYNAMES_4, rested=True, damage=1)
    attacker = sc.add(1, FILLER)
    st = sc.start()
    assert ap(st, dynames) == 2
    attack(st, attacker, dynames)
    pass_all(st)
    assert zone_of(st, attacker) is Zone.TRASH
    assert st.cards[dynames].damage == 3


@pytest.mark.card("ST07-005")
@pytest.mark.ruling("ST07-005:Q201")
def test_st07_005_destroyed_in_the_same_battle_stays_destroyed() -> None:
    sc = Scenario()
    dynames = sc.add(0, DYNAMES_4, damage=2)
    victim = sc.add(1, FILLER, rested=True)
    st = sc.start()
    attack(st, dynames, victim)
    pass_all(st)
    assert zone_of(st, victim) is Zone.TRASH
    assert zone_of(st, dynames) is Zone.TRASH


@pytest.mark.card("ST07-007")
def test_st07_007_ap_plus_two_on_your_turn_with_cb_pilot() -> None:
    sc = Scenario()
    kyrios = sc.add(0, KYRIOS)
    sc.add(0, GRAZE, pilot=SETSUNA)
    st = sc.start()
    assert ap(st, kyrios) == 4


@pytest.mark.card("ST07-007")
def test_st07_007_no_bonus_on_opponents_turn_or_without_cb_pilot() -> None:
    sc = Scenario(active=1)
    kyrios = sc.add(0, KYRIOS)
    sc.add(0, GRAZE, pilot=SETSUNA)
    st = sc.start()
    assert ap(st, kyrios) == 2
    sc2 = Scenario()
    kyrios2 = sc2.add(0, KYRIOS)
    sc2.add(0, GRAZE, pilot=NEUTRAL_PILOT)
    st2 = sc2.start()
    assert ap(st2, kyrios2) == 2


@pytest.mark.card("ST07-009")
def test_st07_009_attack_gives_this_unit_ap_plus_one() -> None:
    sc = Scenario()
    exia = sc.add(0, EXIA, pilot=SETSUNA)
    dynames = sc.add(0, DYNAMES)
    sc.shields(1, FILLER, FILLER)
    st = sc.start()
    assert ap(st, exia) == 6
    attack(st, exia)
    assert ap(st, exia) == 7
    assert ap(st, dynames) == 3


@pytest.mark.card("ST07-009")
def test_st07_009_with_7_cb_cards_all_cb_units_get_ap_plus_one_this_turn() -> None:
    sc = Scenario()
    exia = sc.add(0, EXIA, pilot=SETSUNA)
    dynames = sc.add(0, DYNAMES)
    zaku = sc.add(0, FILLER)
    sc.trash(0, *([VIRTUE] * 7))
    sc.shields(1, FILLER, FILLER)
    st = sc.start()
    attack(st, exia)
    assert (ap(st, exia), ap(st, dynames), ap(st, zaku)) == (7, 4, 2)
    pass_all(st)
    to_next_turn(st)
    assert (ap(st, exia), ap(st, dynames)) == (6, 3)


@pytest.mark.card("ST07-010")
@pytest.mark.rule("13-2-8-2")
def test_st07_010_destroyed_on_opponents_turn_as_cb_unit_draws() -> None:
    sc = Scenario(active=1)
    virtue = sc.add(0, VIRTUE_BLOCKER, pilot=TIERIA, rested=True)
    attacker = sc.add(1, VIRTUE)
    st = sc.start()
    before = hand_size(st, 0)
    attack(st, attacker, virtue)
    pass_all(st)
    assert zone_of(st, virtue) is Zone.TRASH
    assert hand_size(st, 0) == before + 1


@pytest.mark.card("ST07-010")
def test_st07_010_no_draw_on_your_turn() -> None:
    sc = Scenario()
    virtue = sc.add(0, VIRTUE_BLOCKER, pilot=TIERIA)
    victim = sc.add(1, VIRTUE, rested=True)
    st = sc.start()
    before = hand_size(st, 0)
    attack(st, virtue, victim)
    pass_all(st)
    assert zone_of(st, virtue) is Zone.TRASH
    assert hand_size(st, 0) == before


@pytest.mark.card("ST07-010")
def test_st07_010_no_draw_for_non_cb_unit() -> None:
    sc = Scenario(active=1)
    zaku = sc.add(0, FILLER, pilot=TIERIA, rested=True)
    attacker = sc.add(1, VIRTUE)
    st = sc.start()
    before = hand_size(st, 0)
    attack(st, attacker, zaku)
    pass_all(st)
    assert zone_of(st, zaku) is Zone.TRASH
    assert hand_size(st, 0) == before


@pytest.mark.card("ST07-011")
@pytest.mark.ruling("ST07-011:Q202")
def test_st07_011_cb_unit_may_attack_active_enemy_up_to_its_level() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    dynames = sc.add(0, DYNAMES)
    lockon = sc.add(0, LOCKON, Zone.HAND)
    lv2 = sc.add(1, FILLER)
    lv3 = sc.add(1, SUGAI_GELGOOG)
    lv4 = sc.add(1, RED_GUNDAM)
    st = sc.start()
    assert not has_action(st, A.ATTACK, dynames, lv2)
    play(st, lockon, onto=dynames)
    assert has_action(st, A.ATTACK, dynames, lv2)
    assert has_action(st, A.ATTACK, dynames, lv3)
    assert not has_action(st, A.ATTACK, dynames, lv4)
    to_next_turn(st)
    to_next_turn(st)
    assert not has_action(st, A.ATTACK, dynames, lv2)


@pytest.mark.card("ST07-011")
def test_st07_011_non_cb_unit_gains_nothing() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    zaku = sc.add(0, FILLER)
    lockon = sc.add(0, LOCKON, Zone.HAND)
    lv2 = sc.add(1, FILLER)
    st = sc.start()
    play(st, lockon, onto=zaku)
    assert not has_action(st, A.ATTACK, zaku, lv2)


@pytest.mark.card("ST07-012")
def test_st07_012_linked_cb_unit_ignores_battle_damage_from_ap3_or_less() -> None:
    sc = Scenario()
    kyrios = sc.add(0, KYRIOS_FLIGHT, pilot=ALLELUJAH)
    victim = sc.add(1, SUGAI_GELGOOG, rested=True)
    st = sc.start()
    attack(st, kyrios, victim)
    pass_all(st)
    assert zone_of(st, victim) is Zone.TRASH
    assert zone_of(st, kyrios) is Zone.BATTLE
    assert st.cards[kyrios].damage == 0


@pytest.mark.card("ST07-012")
def test_st07_012_ap4_enemy_still_deals_damage() -> None:
    sc = Scenario()
    kyrios = sc.add(0, KYRIOS_FLIGHT, pilot=ALLELUJAH)
    victim = sc.add(1, HYAKUREN, rested=True)
    st = sc.start()
    attack(st, kyrios, victim)
    pass_all(st)
    assert zone_of(st, kyrios) is Zone.TRASH


@pytest.mark.card("ST07-012")
def test_st07_012_not_on_opponents_turn() -> None:
    sc = Scenario(active=1)
    kyrios = sc.add(0, KYRIOS_FLIGHT, pilot=ALLELUJAH, rested=True)
    attacker = sc.add(1, FILLER)
    st = sc.start()
    attack(st, attacker, kyrios)
    pass_all(st)
    assert zone_of(st, kyrios) is Zone.TRASH


@pytest.mark.card("ST07-012")
def test_st07_012_needs_a_cb_link_unit() -> None:
    sc = Scenario()
    virtue = sc.add(0, VIRTUE_BLOCKER, pilot=ALLELUJAH)
    victim = sc.add(1, SUGAI_GELGOOG, rested=True)
    st = sc.start()
    attack(st, virtue, victim)
    pass_all(st)
    assert zone_of(st, victim) is Zone.TRASH
    assert st.cards[virtue].damage == 3


@pytest.mark.card("ST07-013")
@pytest.mark.rule("5-22-2", "9-3-1")
def test_st07_013_redirects_enemy_attack_to_rested_cb_unit() -> None:
    sc = Scenario(active=1)
    sc.resources(0, 4)
    virtue = sc.add(0, VIRTUE, rested=True)
    cmd = sc.add(0, ARMED_INTERVENTION, Zone.HAND)
    (shield,) = sc.shields(0, FILLER)
    attacker = sc.add(1, FILLER)
    st = sc.start()
    attack(st, attacker)
    assert pending_kind(st) is DecisionKind.ACTION_STEP
    play(st, cmd)
    pass_all(st)
    assert zone_of(st, attacker) is Zone.TRASH
    assert st.cards[virtue].damage == 2
    assert zone_of(st, shield) is Zone.SHIELD


@pytest.mark.card("ST07-013")
def test_st07_013_does_not_redirect_your_own_attack() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    attacker = sc.add(0, FILLER)
    virtue = sc.add(0, VIRTUE, rested=True)
    cmd = sc.add(0, ARMED_INTERVENTION, Zone.HAND)
    (shield,) = sc.shields(1, FILLER)
    st = sc.start()
    attack(st, attacker)
    assert pending_kind(st) is DecisionKind.ACTION_STEP
    assert st.pending is not None and st.pending.player == 0
    play(st, cmd)
    pass_all(st)
    assert zone_of(st, shield) is Zone.TRASH
    assert st.cards[virtue].damage == 0
    assert zone_of(st, cmd) is Zone.TRASH


@pytest.mark.card("ST07-013")
@pytest.mark.rule("10-1-8-1-1")
def test_st07_013_needs_a_rested_cb_unit() -> None:
    sc = Scenario(active=1)
    sc.resources(0, 4)
    sc.add(0, VIRTUE)
    sc.add(0, FILLER, rested=True)
    cmd = sc.add(0, ARMED_INTERVENTION, Zone.HAND)
    sc.shields(0, FILLER)
    attacker = sc.add(1, FILLER)
    st = sc.start()
    attack(st, attacker)
    assert not has_action(st, A.PLAY_COMMAND, cmd)


@pytest.mark.card("ST07-013")
def test_st07_013_burst_draws_one() -> None:
    sc = Scenario()
    unit = sc.add(0, FILLER)
    burst, next_shield = sc.shields(1, ARMED_INTERVENTION, FILLER)
    st = sc.start()
    attack(st, unit)
    pass_all(st)
    yes(st)
    assert zone_of(st, burst) is Zone.TRASH
    assert zone_of(st, next_shield) is Zone.SHIELD
    assert hand_size(st, 1) == 1


@pytest.mark.card("ST07-014")
@pytest.mark.ruling("ST07-014:Q203")
@pytest.mark.parametrize("pick", [0, 1])
def test_st07_014_may_add_a_cb_unit_or_pilot(pick: int) -> None:
    sc = Scenario()
    sc.resources(0, 1)
    cmd = sc.add(0, TACTICAL_VISIONARY, Zone.HAND)
    sc.deck(0, SETSUNA, EXIA, FILLER)
    st = sc.start()
    looked = st.zones[0][Zone.DECK][:3]
    play(st, cmd)
    yes(st)
    assert select_options(st) == {looked[0], looked[1]}
    select(st, looked[pick])
    assert zone_of(st, looked[pick]) is Zone.HAND
    rest = {u for u in looked if u != looked[pick]}
    assert set(st.zones[0][Zone.DECK][-2:]) == rest


@pytest.mark.card("ST07-015")
@pytest.mark.rule("8-5-2-4")
def test_st07_015_no_battle_damage_from_lv3_enemy_while_cb_unit_rested() -> None:
    sc = Scenario(active=1)
    ptolemaios = sc.base(0, PTOLEMAIOS)
    sc.add(0, DYNAMES, rested=True)
    attacker = sc.add(1, FILLER)
    st = sc.start()
    attack(st, attacker)
    pass_all(st)
    assert st.cards[ptolemaios].damage == 0


@pytest.mark.card("ST07-015")
def test_st07_015_lv4_enemy_or_no_rested_cb_unit_deals_damage() -> None:
    sc = Scenario(active=1)
    ptolemaios = sc.base(0, PTOLEMAIOS)
    sc.add(0, DYNAMES, rested=True)
    attacker = sc.add(1, RED_GUNDAM)
    st = sc.start()
    attack(st, attacker)
    pass_all(st)
    assert st.cards[ptolemaios].damage == 3
    sc2 = Scenario(active=1)
    ptolemaios2 = sc2.base(0, PTOLEMAIOS)
    sc2.add(0, DYNAMES)
    attacker2 = sc2.add(1, FILLER)
    st2 = sc2.start()
    attack(st2, attacker2)
    pass_all(st2)
    assert st2.cards[ptolemaios2].damage == 2


@pytest.mark.card("ST07-015")
def test_st07_015_unit_tokens_are_not_excluded() -> None:
    sc = Scenario(active=1)
    ptolemaios = sc.base(0, PTOLEMAIOS)
    sc.add(0, DYNAMES, rested=True)
    token = sc.add(1, GUNDAM_TOKEN)
    st = sc.start()
    attack(st, token)
    pass_all(st)
    assert st.cards[ptolemaios].damage == 3


@pytest.mark.card("ST07-015")
@pytest.mark.ruling("ST07-015:Q204")
@pytest.mark.rule("13-1-2-2")
def test_st07_015_breach_damage_from_lv3_enemy_is_prevented() -> None:
    sc = Scenario(active=1)
    ptolemaios = sc.base(0, PTOLEMAIOS)
    sc.add(0, DYNAMES, rested=True)
    victim = sc.add(0, FILLER, rested=True)
    (shield,) = sc.shields(0, FILLER)
    attacker = sc.add(1, LV3_BREACH_2)
    st = sc.start()
    attack(st, attacker, victim)
    pass_all(st)
    assert zone_of(st, victim) is Zone.TRASH
    assert st.cards[ptolemaios].damage == 0
    assert zone_of(st, shield) is Zone.SHIELD


@pytest.mark.card("ST07-015")
def test_st07_015_breach_damage_without_rested_cb_unit() -> None:
    sc = Scenario(active=1)
    ptolemaios = sc.base(0, PTOLEMAIOS)
    victim = sc.add(0, FILLER, rested=True)
    attacker = sc.add(1, LV3_BREACH_2)
    st = sc.start()
    attack(st, attacker, victim)
    pass_all(st)
    assert zone_of(st, victim) is Zone.TRASH
    assert st.cards[ptolemaios].damage == 2


# ---------------------------------------------------------------------------------------------
# ST08


@pytest.mark.card("ST08-001")
@pytest.mark.ruling("ST08-001:Q205")
def test_st08_001_lv_and_cost_drop_per_enemy_unit() -> None:
    sc = Scenario()
    resources = sc.resources(0, 6)
    xi = sc.add(0, XI_GUNDAM_9, Zone.HAND)
    for _ in range(3):
        sc.add(1, FILLER)
    st = sc.start()
    dv = V.derived(st)
    assert (V.play_level(st, dv, xi), V.play_cost(st, dv, xi)) == (6, 5)
    play(st, xi)
    assert zone_of(st, xi) is Zone.BATTLE
    assert sum(st.cards[r].rested for r in resources) == 5


@pytest.mark.card("ST08-001")
def test_st08_001_not_enough_resources_for_reduced_level() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    xi = sc.add(0, XI_GUNDAM_9, Zone.HAND)
    for _ in range(3):
        sc.add(1, FILLER)
    st = sc.start()
    assert not has_action(st, A.PLAY_UNIT, xi)


@pytest.mark.card("ST08-001")
def test_st08_001_no_reduction_with_friendly_lv6_unit() -> None:
    sc = Scenario()
    sc.resources(0, 7)
    xi = sc.add(0, XI_GUNDAM_9, Zone.HAND)
    sc.add(0, PENELOPE_7)
    for _ in range(3):
        sc.add(1, FILLER)
    st = sc.start()
    dv = V.derived(st)
    assert (V.play_level(st, dv, xi), V.play_cost(st, dv, xi)) == (9, 8)
    assert not has_action(st, A.PLAY_UNIT, xi)


@pytest.mark.card("ST08-001")
@pytest.mark.ruling("ST08-001:Q206")
def test_st08_001_when_paired_deals_3_to_a_highest_level_enemy() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    xi = sc.add(0, XI_GUNDAM_9)
    pilot = sc.add(0, NEUTRAL_PILOT, Zone.HAND)
    virtue = sc.add(1, VIRTUE)
    penelope = sc.add(1, PENELOPE)
    low = sc.add(1, FILLER)
    st = sc.start()
    play(st, pilot, onto=xi)
    assert select_options(st) == {virtue, penelope}
    select(st, penelope)
    assert st.cards[penelope].damage == 3
    assert st.cards[virtue].damage == 0
    assert st.cards[low].damage == 0


@pytest.mark.card("ST08-002")
def test_st08_002_deploy_deals_1_damage_to_enemy() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    xi = sc.add(0, XI_GUNDAM_5, Zone.HAND)
    enemy = sc.add(1, VIRTUE)
    st = sc.start()
    play(st, xi)
    assert st.cards[enemy].damage == 1


@pytest.mark.card("ST08-004")
def test_st08_004_attacking_a_unit_damages_an_enemy_unit() -> None:
    sc = Scenario()
    messer = sc.add(0, MESSER_F01)
    target = sc.add(1, FILLER, rested=True)
    other = sc.add(1, SUGAI_GELGOOG)
    st = sc.start()
    attack(st, messer, target)
    assert select_options(st) == {target, other}
    select(st, other)
    assert st.cards[other].damage == 1


@pytest.mark.card("ST08-004")
def test_st08_004_attacking_the_player_does_nothing() -> None:
    sc = Scenario()
    messer = sc.add(0, MESSER_F01)
    other = sc.add(1, SUGAI_GELGOOG)
    sc.shields(1, FILLER)
    st = sc.start()
    attack(st, messer, PLAYER_TARGET)
    assert pending_kind(st) is not DecisionKind.SELECT
    assert st.cards[other].damage == 0


@pytest.mark.card("ST08-006")
@pytest.mark.rule("13-2-10-1", "5-20-1")
def test_st08_006_paired_attack_on_player_cycles_ef_unit_and_draws_two() -> None:
    sc = Scenario()
    penelope = sc.add(0, PENELOPE_7, pilot=NEUTRAL_PILOT)
    ef, zaku = sc.hand(0, PENELOPE, FILLER)
    sc.shields(1, FILLER, FILLER)
    st = sc.start()
    attack(st, penelope)
    assert st.zones[0][Zone.DECK][-1] == ef
    assert zone_of(st, zaku) is Zone.HAND
    assert hand_size(st, 0) == 3


@pytest.mark.card("ST08-006", "ST08-011")
def test_st08_006_draw_with_lane_aim_grants_high_maneuver_before_blocks() -> None:
    sc = Scenario()
    penelope = sc.add(0, PENELOPE_7, pilot=LANE_AIM)
    sc.hand(0, PENELOPE)
    sc.add(1, ENEMY_BLOCKER)
    top, _ = sc.shields(1, FILLER, FILLER)
    st = sc.start()
    attack(st, penelope)
    assert keywords(st, penelope).get("High-Maneuver") == 1
    assert hand_size(st, 0) == 2
    assert pending_kind(st) is not DecisionKind.BLOCK
    pass_all(st)
    assert zone_of(st, top) is Zone.TRASH


@pytest.mark.card("ST08-006")
def test_st08_006_no_draw_without_ef_unit_in_hand() -> None:
    sc = Scenario()
    penelope = sc.add(0, PENELOPE_7, pilot=NEUTRAL_PILOT)
    sc.hand(0, FILLER)
    sc.shields(1, FILLER, FILLER)
    st = sc.start()
    attack(st, penelope)
    assert hand_size(st, 0) == 1


@pytest.mark.card("ST08-006")
def test_st08_006_needs_pair_and_player_target() -> None:
    sc = Scenario()
    unpaired = sc.add(0, PENELOPE_7)
    sc.hand(0, PENELOPE)
    sc.shields(1, FILLER, FILLER)
    st = sc.start()
    attack(st, unpaired)
    assert hand_size(st, 0) == 1
    sc2 = Scenario()
    paired = sc2.add(0, PENELOPE_7, pilot=NEUTRAL_PILOT)
    sc2.hand(0, PENELOPE)
    target = sc2.add(1, FILLER, rested=True)
    st2 = sc2.start()
    attack(st2, paired, target)
    assert hand_size(st2, 0) == 1


@pytest.mark.card("ST08-008")
def test_st08_008_blocker_while_3_enemy_units() -> None:
    sc = Scenario()
    gustav = sc.add(0, GUSTAV_KARL)
    sc.add(1, FILLER)
    sc.add(1, FILLER)
    st = sc.start()
    assert "Blocker" not in keywords(st, gustav)
    sc2 = Scenario()
    gustav2 = sc2.add(0, GUSTAV_KARL)
    for _ in range(3):
        sc2.add(1, FILLER)
    st2 = sc2.start()
    assert keywords(st2, gustav2).get("Blocker") == 1


@pytest.mark.card("ST08-008", "ST08-013")
@pytest.mark.ruling("ST08-008:Q207")
def test_st08_008_block_stands_after_losing_blocker() -> None:
    sc = Scenario(active=1)
    sc.resources(0, 5)
    gustav = sc.add(0, GUSTAV_KARL)
    lady_luck = sc.add(0, LADY_LUCK, Zone.HAND)
    attacker = sc.add(1, FILLER)
    fragile = sc.add(1, KYRIOS_FLIGHT)
    sc.add(1, FILLER)
    st = sc.start()
    attack(st, attacker)
    block(st, gustav)
    play(st, lady_luck)
    select(st, fragile)
    assert zone_of(st, fragile) is Zone.TRASH
    assert "Blocker" not in keywords(st, gustav)
    pass_all(st)
    assert zone_of(st, attacker) is Zone.TRASH
    assert st.cards[gustav].damage == 2
    assert st.winner is None


@pytest.mark.card("ST08-009")
@pytest.mark.rule("7-2-3-1")
def test_st08_009_chosen_unit_stays_rested_through_next_start_phase() -> None:
    sc = Scenario()
    sc.resources(0, 1)
    jegan = sc.add(0, JEGAN, Zone.HAND)
    low = sc.add(1, FILLER, rested=True)
    high = sc.add(1, VIRTUE, rested=True)
    st = sc.start()
    play(st, jegan)
    to_next_turn(st)
    assert st.active == 1
    assert st.cards[low].rested
    assert not st.cards[high].rested
    to_next_turn(st)
    to_next_turn(st)
    assert st.active == 1
    assert not st.cards[low].rested


@pytest.mark.card("ST08-009")
@pytest.mark.ruling("ST08-009:Q208")
def test_st08_009_effect_can_still_set_the_unit_active() -> None:
    sc = Scenario()
    sc.resources(0, 1)
    jegan = sc.add(0, JEGAN, Zone.HAND)
    attacker = sc.add(0, SUGAI_GELGOOG)
    aries = sc.add(1, LV2_BLOCKER, rested=True)
    sc.shields(1, MIDAIR_MODIFICATIONS, FILLER)
    st = sc.start()
    play(st, jegan)
    attack(st, attacker)
    pass_all(st)
    assert pending_kind(st) is DecisionKind.BURST
    yes(st)
    assert not st.cards[aries].rested


@pytest.mark.card("ST08-010")
def test_st08_010_mafty_unit_lets_a_mafty_unit_attack_damaged_active_enemy() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    messer = sc.add(0, MESSER_COMMANDER)
    minelayer = sc.add(0, MINELAYER)
    hathaway = sc.add(0, HATHAWAY, Zone.HAND)
    damaged = sc.add(1, FILLER, damage=1)
    healthy = sc.add(1, SUGAI_GELGOOG)
    st = sc.start()
    play(st, hathaway, onto=messer)
    assert select_options(st) == {messer, minelayer}
    select(st, minelayer)
    assert has_action(st, A.ATTACK, minelayer, damaged)
    assert not has_action(st, A.ATTACK, minelayer, healthy)
    assert not has_action(st, A.ATTACK, messer, damaged)


@pytest.mark.card("ST08-010")
def test_st08_010_non_mafty_unit_does_nothing() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    zaku = sc.add(0, FILLER)
    minelayer = sc.add(0, MINELAYER)
    hathaway = sc.add(0, HATHAWAY, Zone.HAND)
    damaged = sc.add(1, FILLER, damage=1)
    st = sc.start()
    play(st, hathaway, onto=zaku)
    assert pending_kind(st) is DecisionKind.MAIN
    assert not has_action(st, A.ATTACK, minelayer, damaged)


@pytest.mark.card("ST08-011")
@pytest.mark.rule("13-1-6-1")
def test_st08_011_effect_draw_gives_blue_unit_high_maneuver() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    penelope = sc.add(0, PENELOPE, pilot=LANE_AIM)
    cmd = sc.add(0, DRAW_2_COMMAND, Zone.HAND)
    sc.add(1, ENEMY_BLOCKER)
    sc.shields(1, FILLER, FILLER)
    st = sc.start()
    assert "High-Maneuver" not in keywords(st, penelope)
    play(st, cmd)
    assert keywords(st, penelope).get("High-Maneuver") == 1
    attack(st, penelope)
    assert pending_kind(st) is not DecisionKind.BLOCK


@pytest.mark.card("ST08-011")
def test_st08_011_non_blue_unit_gains_nothing() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    zaku = sc.add(0, FILLER, pilot=LANE_AIM)
    cmd = sc.add(0, DRAW_2_COMMAND, Zone.HAND)
    st = sc.start()
    play(st, cmd)
    assert "High-Maneuver" not in keywords(st, zaku)


@pytest.mark.card("ST08-011")
@pytest.mark.rule("7-3-1")
def test_st08_011_draw_phase_draw_is_not_an_effect() -> None:
    sc = Scenario(active=1)
    penelope = sc.add(0, PENELOPE, pilot=LANE_AIM)
    st = sc.start()
    to_next_turn(st)
    assert st.active == 0
    assert "High-Maneuver" not in keywords(st, penelope)


@pytest.mark.card("ST08-012")
@pytest.mark.rule("13-1-2-1")
def test_st08_012_link_unit_gains_breach_1() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    messer = sc.add(0, MESSER_COMMANDER, pilot=HATHAWAY)
    cmd = sc.add(0, WORDS_FOR_HATHAWAY, Zone.HAND)
    victim = sc.add(1, FILLER, rested=True)
    top, second = sc.shields(1, FILLER, FILLER)
    st = sc.start()
    play(st, cmd)
    assert keywords(st, messer).get("Breach") == 1
    attack(st, messer, victim)
    pass_all(st)
    assert zone_of(st, top) is Zone.TRASH
    assert zone_of(st, second) is Zone.SHIELD


@pytest.mark.card("ST08-012")
@pytest.mark.rule("10-1-8-1-1", "3-4-6-2")
def test_st08_012_needs_a_link_unit_and_pairs_as_gawman() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    messer = sc.add(0, MESSER_COMMANDER)
    cmd = sc.add(0, WORDS_FOR_HATHAWAY, Zone.HAND)
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, cmd)
    play(st, cmd, onto=messer)
    assert V.is_linked(V.derived(st), messer)


@pytest.mark.card("ST08-013")
def test_st08_013_deals_1_damage_without_mafty_link() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    sc.add(0, MESSER_COMMANDER)
    cmd = sc.add(0, LADY_LUCK, Zone.HAND)
    enemy = sc.add(1, SUGAI_GELGOOG)
    st = sc.start()
    play(st, cmd)
    assert st.cards[enemy].damage == 1


@pytest.mark.card("ST08-013")
def test_st08_013_deals_2_damage_instead_with_mafty_link() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    sc.add(0, MESSER_COMMANDER, pilot=HATHAWAY)
    cmd = sc.add(0, LADY_LUCK, Zone.HAND)
    enemy = sc.add(1, SUGAI_GELGOOG)
    st = sc.start()
    play(st, cmd)
    assert st.cards[enemy].damage == 2


@pytest.mark.card("ST08-014")
@pytest.mark.rule("5-20-2")
def test_st08_014_deploy_adds_shield_then_pumps_a_unit() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    valiant = sc.add(0, VALIANT, Zone.HAND)
    zaku = sc.add(0, FILLER)
    (shield,) = sc.shields(0, FILLER)
    st = sc.start()
    play(st, valiant)
    assert zone_of(st, shield) is Zone.HAND
    assert ap(st, zaku) == 4


@pytest.mark.card("ST08-014")
def test_st08_014_pump_happens_even_without_a_shield() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    valiant = sc.add(0, VALIANT, Zone.HAND)
    zaku = sc.add(0, FILLER)
    st = sc.start()
    play(st, valiant)
    assert ap(st, zaku) == 4


@pytest.mark.card("ST08-015")
@pytest.mark.rule("13-2-13-1", "5-6-1")
def test_st08_015_pay_two_to_recover_two_once_per_turn() -> None:
    sc = Scenario()
    resources = sc.resources(0, 4)
    davao = sc.base(0, DAVAO)
    gustav = sc.add(0, GUSTAV_KARL, damage=3)
    st = sc.start()
    activate(st, davao)
    assert st.cards[gustav].damage == 1
    assert sum(st.cards[r].rested for r in resources) == 2
    assert not st.cards[davao].rested
    assert not has_action(st, A.ACTIVATE, davao)
