"""Card behaviour tests for GD01-073..GD01-130 (work package WP-GD01-B)."""

from __future__ import annotations

import pytest

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
    numbers_in,
    pass_,
    pass_all,
    play,
    select,
    to_next_turn,
    yes,
    zone_of,
)

A = ActionKind

# Vanilla helper Units (no effects): number -> colour, traits, Lv, AP/HP
EF_4_3 = "GD01-018"  # blue (Earth Federation) Lv3 4/3
ZEON_2_3 = "GD01-037"  # green (Zeon) Lv2 2/3
ZEON_2_2 = "GD01-035"  # green (Zeon) Lv2 2/2
ZAKU_2_2 = "GD01-060"  # red (Zeon) Lv2 2/2
EF_LINK_2_2 = "GD01-011"  # blue (Earth Federation) Lv2 2/2, link: (Earth Federation) trait
EF_LV4_3_4 = "GD01-013"  # blue Lv4 3/4
OZ_1_2 = "GD01-021"  # blue (OZ) Lv1 1/2
ZEON_LV4_4_3 = "GD01-031"  # green (Zeon) Lv4 4/3
NEOZEON_LINK_3_4 = "GD01-051"  # red Lv4 3/4, link: (Cyber-Newtype) trait
ZAFT_3_2 = "GD01-064"  # red (ZAFT) Lv2 3/2
EA_WHITE_2_2 = "GD01-079"  # white (Earth Alliance) Lv2 2/2
ACADEMY_2_2 = "GD01-083"  # white (Academy) Lv2 2/2, link: (Academy) trait
EF_LV5_4_4 = "GD04-048"  # red Lv5 4/4
ZAFT_LV5_5_4 = "GD04-055"  # purple (ZAFT) Lv5 5/4
NEOZEON_4_1 = "GD02-048"  # red Lv3 4/1
BANAGHER_LINK_5_4 = "GD03-016"  # blue Lv5 5/4, link: [Banagher Links]
BIG_6_6 = "GD03-010"  # blue Lv8 6/6 <Repair 3>, link: [Banagher Links]

EA_PILOT = "GD02-087"  # (Earth Alliance) Pilot 2/1; its 【When Linked】 needs a blue Unit
DESTROY_ACTION = "GD05-116"  # 【Main】/【Action】destroy 1 enemy Unit that is Lv.2 or lower


def hand_size(st: GameState, player: int) -> int:
    return len(st.zones[player][Zone.HAND])


def pass_until(st: GameState, player: int) -> None:
    """Pass action-step priority of the other player until ``player`` has to decide."""
    for _ in range(10):
        dec = st.pending
        if dec is None or dec.player == player or dec.kind is not DecisionKind.ACTION_STEP:
            return
        pass_(st)


def attack_my_shield(sc: Scenario, attacker: str = EF_4_3) -> tuple[GameState, int]:
    """Opponent (player 1, active) attacks player 0, whose top Shield was set up by the caller;
    returns the state at player 0's 【Burst】 decision."""
    raider = sc.add(1, attacker)
    st = sc.start()
    attack(st, raider)
    pass_all(st)
    assert st.pending is not None and st.pending.kind is DecisionKind.BURST
    return st, raider


# ---------------------------------------------------------------------------------------------
# GD01-073 Sword Strike Gundam


@pytest.mark.card("GD01-073")
@pytest.mark.faq("Q96")
@pytest.mark.rule("13-2-12-1")
def test_gd01_073_linked_attack_returns_enemy_with_2_or_less_current_hp() -> None:
    sc = Scenario()
    sword = sc.add(0, "GD01-073", pilot=EA_PILOT)
    damaged = sc.add(1, EF_4_3, damage=1)
    healthy = sc.add(1, ZEON_2_3)
    st = sc.start()
    attack(st, sword)
    assert zone_of(st, damaged) is Zone.HAND
    assert zone_of(st, healthy) is Zone.BATTLE


@pytest.mark.card("GD01-073")
def test_gd01_073_unlinked_attack_does_nothing() -> None:
    sc = Scenario()
    sword = sc.add(0, "GD01-073")
    target = sc.add(1, ZAKU_2_2)
    st = sc.start()
    attack(st, sword)
    assert zone_of(st, target) is Zone.BATTLE


# ---------------------------------------------------------------------------------------------
# GD01-074 Chuchu's Demi Trainer


@pytest.mark.card("GD01-074")
def test_gd01_074_attack_draws_then_discards() -> None:
    sc = Scenario()
    demi = sc.add(0, "GD01-074")
    kept = sc.add(0, ZAKU_2_2, Zone.HAND)
    sc.deck(0, "GD01-100")
    st = sc.start()
    attack(st, demi)
    assert st.pending is not None and st.pending.kind is DecisionKind.DISCARD
    assert numbers_in(st, 0, Zone.HAND) == [ZAKU_2_2, "GD01-100"]
    select(st, kept)
    assert zone_of(st, kept) is Zone.TRASH
    assert numbers_in(st, 0, Zone.HAND) == ["GD01-100"]


# ---------------------------------------------------------------------------------------------
# GD01-075 Darilbalde


@pytest.mark.card("GD01-075")
@pytest.mark.faq("Q96")
def test_gd01_075_deploy_returns_enemy_with_1_current_hp() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    darilbalde = sc.add(0, "GD01-075", Zone.HAND)
    one_left = sc.add(1, EF_4_3, damage=2)
    two_hp = sc.add(1, ZAKU_2_2)
    st = sc.start()
    play(st, darilbalde)
    assert zone_of(st, one_left) is Zone.HAND
    assert zone_of(st, two_hp) is Zone.BATTLE


@pytest.mark.card("GD01-075")
def test_gd01_075_no_enemy_with_1_hp_returns_nothing() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    darilbalde = sc.add(0, "GD01-075", Zone.HAND)
    foe = sc.add(1, EF_4_3, damage=1)
    st = sc.start()
    play(st, darilbalde)
    assert zone_of(st, foe) is Zone.BATTLE
    assert st.pending is not None and st.pending.kind is DecisionKind.MAIN


# ---------------------------------------------------------------------------------------------
# GD01-076 Michaelis

COMMANDS = ("GD01-100", "GD01-118", "GD01-115", "GD01-104")


@pytest.mark.card("GD01-076")
def test_gd01_076_four_commands_in_trash_give_ap_and_hp() -> None:
    sc = Scenario()
    michaelis = sc.add(0, "GD01-076")
    sc.trash(0, "GD01-100", "GD01-118", "GD01-115", "GD01-113")  # GD01-113 has 【Pilot】
    st = sc.start()
    assert (ap(st, michaelis), hp(st, michaelis)) == (4, 4)


@pytest.mark.card("GD01-076")
def test_gd01_076_three_commands_and_a_pilot_do_not_count() -> None:
    sc = Scenario()
    michaelis = sc.add(0, "GD01-076")
    sc.trash(0, "GD01-100", "GD01-118", "GD01-115", "GD01-089")
    sc.trash(1, *COMMANDS)
    st = sc.start()
    assert (ap(st, michaelis), hp(st, michaelis)) == (3, 3)


@pytest.mark.card("GD01-076")
@pytest.mark.ruling("GD01-076:Q140")
@pytest.mark.rule("10-1-5-3", "11-3-1")
def test_gd01_076_losing_the_condition_destroys_it_by_damage() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    michaelis = sc.add(0, "GD01-076", damage=3)
    trash = sc.trash(0, *COMMANDS)
    recycler = sc.add(0, "GD01-067")  # 【When Paired】adds a Command card from trash to hand
    pilot = sc.add(0, "GD01-089", Zone.HAND)
    st = sc.start()
    assert hp(st, michaelis) == 4 and zone_of(st, michaelis) is Zone.BATTLE
    play(st, pilot, onto=recycler)
    select(st, trash[0])
    assert zone_of(st, trash[0]) is Zone.HAND
    assert zone_of(st, michaelis) is Zone.TRASH


# ---------------------------------------------------------------------------------------------
# GD01-078 Mistral


@pytest.mark.card("GD01-078")
def test_gd01_078_deploy_gives_enemy_ap_minus_1_this_turn() -> None:
    sc = Scenario()
    sc.resources(0, 1)
    mistral = sc.add(0, "GD01-078", Zone.HAND)
    foe = sc.add(1, EF_4_3)
    st = sc.start()
    play(st, mistral)
    assert ap(st, foe) == 3
    to_next_turn(st)
    assert ap(st, foe) == 4


# ---------------------------------------------------------------------------------------------
# GD01-080 Cagalli's Skygrasper


@pytest.mark.card("GD01-080")
@pytest.mark.rule("13-2-8-1")
def test_gd01_080_destroyed_returns_enemy_lv2_or_lower() -> None:
    sc = Scenario()
    skygrasper = sc.add(0, "GD01-080")
    lv3 = sc.add(1, EF_4_3, rested=True)
    lv2 = sc.add(1, ZEON_2_2)
    st = sc.start()
    attack(st, skygrasper, lv3)
    pass_all(st)
    assert zone_of(st, skygrasper) is Zone.TRASH
    assert zone_of(st, lv2) is Zone.HAND
    assert zone_of(st, lv3) is Zone.BATTLE


@pytest.mark.card("GD01-080")
def test_gd01_080_destroyed_without_low_level_enemy_returns_nothing() -> None:
    sc = Scenario()
    skygrasper = sc.add(0, "GD01-080")
    lv3 = sc.add(1, EF_4_3, rested=True)
    st = sc.start()
    attack(st, skygrasper, lv3)
    pass_all(st)
    assert zone_of(st, skygrasper) is Zone.TRASH
    assert zone_of(st, lv3) is Zone.BATTLE


# ---------------------------------------------------------------------------------------------
# GD01-081 M1 Astray


@pytest.mark.card("GD01-081")
def test_gd01_081_another_triple_ship_alliance_unit_gives_ap_and_blocker() -> None:
    sc = Scenario()
    m1 = sc.add(0, "GD01-081")
    sc.add(0, "GD01-081")
    st = sc.start()
    assert ap(st, m1) == 3
    assert "Blocker" in keywords(st, m1)


@pytest.mark.card("GD01-081")
def test_gd01_081_without_another_triple_ship_alliance_unit() -> None:
    sc = Scenario()
    m1 = sc.add(0, "GD01-081")
    sc.add(0, EA_WHITE_2_2)
    st = sc.start()
    assert ap(st, m1) == 2
    assert "Blocker" not in keywords(st, m1)


@pytest.mark.card("GD01-081")
@pytest.mark.ruling("GD01-081:Q141")
@pytest.mark.rule("8-3-1", "13-1-4-1")
def test_gd01_081_block_stands_after_losing_blocker() -> None:
    sc = Scenario(active=1)
    sc.resources(1, 5)
    bounce = sc.add(1, "GD01-117", Zone.HAND)
    raider = sc.add(1, EF_4_3)
    m1 = sc.add(0, "GD01-081")
    partner = sc.add(0, "GD01-081")
    (shield,) = sc.shields(0, ZAKU_2_2)
    st = sc.start()
    attack(st, raider)
    block(st, m1)
    pass_until(st, 1)
    act(st, A.PLAY_COMMAND, bounce)
    select(st, partner)
    assert zone_of(st, partner) is Zone.HAND
    assert "Blocker" not in keywords(st, m1)
    pass_all(st)
    assert zone_of(st, m1) is Zone.TRASH
    assert st.cards[raider].damage == 2
    assert zone_of(st, shield) is Zone.SHIELD


# ---------------------------------------------------------------------------------------------
# GD01-082 Gundam Aerial (Mirasoul Flight Unit)


@pytest.mark.card("GD01-082")
@pytest.mark.rule("13-2-10-1", "13-2-2-1", "13-2-13-1")
def test_gd01_082_paired_action_gives_enemy_ap_minus_1_during_battle() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    aerial = sc.add(0, "GD01-082", pilot="GD01-089")  # 5/4
    sc.add(0, "GD01-115", Zone.HAND)  # keeps an action-step option open after activating
    foe = sc.add(1, EF_4_3, rested=True)
    st = sc.start()
    attack(st, aerial, foe)
    pass_until(st, 0)
    activate(st, aerial)
    pass_until(st, 0)
    assert ap(st, foe) == 3
    assert not has_action(st, A.ACTIVATE, aerial)  # 【Once per Turn】
    pass_all(st)
    assert zone_of(st, foe) is Zone.TRASH
    assert st.cards[aerial].damage == 3
    assert zone_of(st, aerial) is Zone.BATTLE


@pytest.mark.card("GD01-082")
@pytest.mark.rule("8-6-1")
def test_gd01_082_ap_reduction_ends_with_the_battle() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    aerial = sc.add(0, "GD01-082", pilot="GD01-089")
    sc.add(0, "GD01-115", Zone.HAND)
    bystander = sc.add(1, EF_4_3)
    sc.shields(1, ZAKU_2_2)
    st = sc.start()
    attack(st, aerial)
    pass_until(st, 0)
    activate(st, aerial)
    pass_until(st, 0)
    assert ap(st, bystander) == 3
    pass_all(st)
    assert st.pending is not None and st.pending.kind is DecisionKind.MAIN
    assert ap(st, bystander) == 4


@pytest.mark.card("GD01-082")
def test_gd01_082_unpaired_cannot_activate() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    aerial = sc.add(0, "GD01-082")
    sc.add(1, EF_4_3)
    sc.shields(1, ZAKU_2_2)
    st = sc.start()
    attack(st, aerial)
    pass_until(st, 0)
    assert not has_action(st, A.ACTIVATE, aerial)


@pytest.mark.card("GD01-082")
@pytest.mark.xfail(
    strict=True,
    reason="ENGINE: 'during this battle' lasting effects created outside a battle never expire",
)
def test_gd01_082_used_in_end_phase_does_not_outlast_the_turn() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    sc.add(0, "GD01-082", pilot="GD01-089")
    foe = sc.add(1, EF_4_3)
    st = sc.start(Step.END_ACTION)
    aerial = st.zones[0][Zone.BATTLE][0]
    activate(st, aerial)
    to_next_turn(st)
    assert ap(st, foe) == 4


# ---------------------------------------------------------------------------------------------
# GD01-086 Gundam Lfrith


@pytest.mark.card("GD01-086")
@pytest.mark.rule("13-1-4-1")
def test_gd01_086_blocker() -> None:
    sc = Scenario(active=1)
    raider = sc.add(1, ZAFT_3_2)
    lfrith = sc.add(0, "GD01-086")
    (shield,) = sc.shields(0, ZAKU_2_2)
    st = sc.start()
    assert "Blocker" in keywords(st, lfrith)
    attack(st, raider)
    block(st, lfrith)
    pass_all(st)
    assert st.cards[lfrith].damage == 3
    assert zone_of(st, raider) is Zone.TRASH
    assert zone_of(st, shield) is Zone.SHIELD


# ---------------------------------------------------------------------------------------------
# Pilot 【Burst】Add this card to your hand. (and GD01-105)


@pytest.mark.parametrize(
    "number",
    [
        pytest.param("GD01-087", marks=pytest.mark.card("GD01-087")),
        pytest.param("GD01-088", marks=pytest.mark.card("GD01-088")),
        pytest.param("GD01-089", marks=pytest.mark.card("GD01-089")),
        pytest.param("GD01-090", marks=pytest.mark.card("GD01-090")),
        pytest.param("GD01-091", marks=pytest.mark.card("GD01-091")),
        pytest.param("GD01-092", marks=pytest.mark.card("GD01-092")),
        pytest.param("GD01-093", marks=pytest.mark.card("GD01-093")),
        pytest.param("GD01-094", marks=pytest.mark.card("GD01-094")),
        pytest.param("GD01-095", marks=pytest.mark.card("GD01-095")),
        pytest.param("GD01-096", marks=pytest.mark.card("GD01-096")),
        pytest.param("GD01-097", marks=pytest.mark.card("GD01-097")),
        pytest.param("GD01-098", marks=pytest.mark.card("GD01-098")),
        pytest.param("GD01-105", marks=pytest.mark.card("GD01-105")),
    ],
)
@pytest.mark.rule("13-2-5-1")
def test_burst_adds_card_to_hand(number: str) -> None:
    sc = Scenario(active=1)
    (shield,) = sc.shields(0, number)
    st, _ = attack_my_shield(sc)
    yes(st)
    assert zone_of(st, shield) is Zone.HAND


# ---------------------------------------------------------------------------------------------
# GD01-087 Sayla Mass


@pytest.mark.card("GD01-087")
@pytest.mark.rule("3-3-9-2", "13-1-1-1")
def test_gd01_087_blue_unit_gains_repair_1() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    unit = sc.add(0, EF_4_3, damage=2)
    sayla = sc.add(0, "GD01-087", Zone.HAND)
    st = sc.start()
    play(st, sayla, onto=unit)
    assert keywords(st, unit).get("Repair") == 1
    to_next_turn(st)
    assert st.cards[unit].damage == 1


@pytest.mark.card("GD01-087")
def test_gd01_087_non_blue_unit_does_not_gain_repair() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    unit = sc.add(0, ZAKU_2_2)
    sayla = sc.add(0, "GD01-087", Zone.HAND)
    st = sc.start()
    play(st, sayla, onto=unit)
    assert "Repair" not in keywords(st, unit)


# ---------------------------------------------------------------------------------------------
# GD01-088 Banagher Links


@pytest.mark.card("GD01-088")
@pytest.mark.rule("13-2-11-1")
def test_gd01_088_when_linked_draws_1() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    unit = sc.add(0, BANAGHER_LINK_5_4)
    banagher = sc.add(0, "GD01-088", Zone.HAND)
    st = sc.start()
    play(st, banagher, onto=unit)
    assert hand_size(st, 0) == 1


@pytest.mark.card("GD01-088")
def test_gd01_088_pairing_without_link_does_not_draw() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    unit = sc.add(0, EF_4_3)
    banagher = sc.add(0, "GD01-088", Zone.HAND)
    st = sc.start()
    play(st, banagher, onto=unit)
    assert hand_size(st, 0) == 0


# ---------------------------------------------------------------------------------------------
# GD01-089 Riddhe Marcenas


@pytest.mark.card("GD01-089")
def test_gd01_089_unit_with_repair_gets_ap_plus_1() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    unit = sc.add(0, "GD02-017")  # <Repair 2> 2/3
    riddhe = sc.add(0, "GD01-089", Zone.HAND)
    st = sc.start()
    play(st, riddhe, onto=unit)
    assert ap(st, unit) == 2 + 1 + 1


@pytest.mark.card("GD01-089")
def test_gd01_089_unit_without_repair_gets_only_pilot_ap() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    unit = sc.add(0, EF_LINK_2_2)
    riddhe = sc.add(0, "GD01-089", Zone.HAND)
    st = sc.start()
    play(st, riddhe, onto=unit)
    assert ap(st, unit) == 2 + 1


# ---------------------------------------------------------------------------------------------
# GD01-090 Duo Maxwell


@pytest.mark.card("GD01-090")
@pytest.mark.ruling("GD01-090:Q147")
@pytest.mark.rule("13-2-12-1", "10-1-5-6")
def test_gd01_090_linked_ap_cannot_be_reduced_by_enemy_effects() -> None:
    sc = Scenario(active=1)
    sc.resources(1, 2)
    discipline = sc.add(1, "GD01-119", Zone.HAND)  # enemy Unit Lv.4 or lower gets AP-2
    deathscythe = sc.add(0, "GD01-033", pilot="GD01-090")  # link [Duo Maxwell] 4/3 + 1/2
    st = sc.start()
    assert ap(st, deathscythe) == 5
    play(st, discipline)
    assert ap(st, deathscythe) == 5


@pytest.mark.card("GD01-090")
def test_gd01_090_unlinked_ap_can_be_reduced() -> None:
    sc = Scenario(active=1)
    sc.resources(1, 2)
    discipline = sc.add(1, "GD01-119", Zone.HAND)
    unit = sc.add(0, EF_4_3, pilot="GD01-090")
    st = sc.start()
    play(st, discipline)
    assert ap(st, unit) == 5 - 2


# ---------------------------------------------------------------------------------------------
# GD01-091 Chang Wufei

WUFEI_UNIT = "GD01-041"  # <Breach 3>, link [Chang Wufei], 4/3 -> 6/4 with Wufei


@pytest.mark.card("GD01-091")
def test_gd01_091_breach_unit_takes_no_battle_damage_from_3_ap_enemy() -> None:
    sc = Scenario()
    shenlong = sc.add(0, WUFEI_UNIT, pilot="GD01-091")
    foe = sc.add(1, ZAFT_3_2, rested=True)
    st = sc.start()
    attack(st, shenlong, foe)
    pass_all(st)
    assert zone_of(st, foe) is Zone.TRASH
    assert st.cards[shenlong].damage == 0


@pytest.mark.card("GD01-091")
def test_gd01_091_takes_battle_damage_from_4_ap_enemy() -> None:
    sc = Scenario()
    shenlong = sc.add(0, WUFEI_UNIT, pilot="GD01-091")
    foe = sc.add(1, ZEON_LV4_4_3, rested=True)
    st = sc.start()
    attack(st, shenlong, foe)
    pass_all(st)
    assert zone_of(st, shenlong) is Zone.TRASH


@pytest.mark.card("GD01-091")
def test_gd01_091_without_breach_takes_damage() -> None:
    sc = Scenario()
    unit = sc.add(0, EF_4_3, pilot="GD01-091")
    foe = sc.add(1, ZAFT_3_2, rested=True)
    st = sc.start()
    attack(st, unit, foe)
    pass_all(st)
    assert st.cards[unit].damage == 3


@pytest.mark.card("GD01-091")
def test_gd01_091_takes_damage_during_opponent_turn() -> None:
    sc = Scenario(active=1)
    shenlong = sc.add(0, WUFEI_UNIT, pilot="GD01-091", rested=True)
    raider = sc.add(1, ZAFT_3_2)
    st = sc.start()
    attack(st, raider, shenlong)
    pass_all(st)
    assert st.cards[shenlong].damage == 3


# ---------------------------------------------------------------------------------------------
# GD01-092 M'Quve


@pytest.mark.card("GD01-092")
@pytest.mark.rule("13-1-2-1")
def test_gd01_092_zeon_unit_gains_breach_1() -> None:
    sc = Scenario()
    zaku = sc.add(0, ZEON_2_2, pilot="GD01-092")
    foe = sc.add(1, ZAKU_2_2, rested=True)
    top, second = sc.shields(1, ZAKU_2_2, ZAKU_2_2)
    st = sc.start()
    assert keywords(st, zaku).get("Breach") == 1
    attack(st, zaku, foe)
    pass_all(st)
    assert zone_of(st, foe) is Zone.TRASH
    assert zone_of(st, top) is Zone.TRASH
    assert zone_of(st, second) is Zone.SHIELD


@pytest.mark.card("GD01-092")
def test_gd01_092_non_zeon_unit_gains_nothing() -> None:
    sc = Scenario()
    unit = sc.add(0, EF_4_3, pilot="GD01-092")
    st = sc.start()
    assert "Breach" not in keywords(st, unit)


# ---------------------------------------------------------------------------------------------
# GD01-093 Marida Cruz


@pytest.mark.card("GD01-093")
@pytest.mark.ruling("GD01-093:Q148")
def test_gd01_093_linked_attack_damages_enemy_of_equal_or_lower_lv() -> None:
    sc = Scenario()
    kshatriya = sc.add(0, NEOZEON_LINK_3_4, pilot="GD01-093")  # Lv4
    same_lv = sc.add(1, EF_LV4_3_4)
    higher = sc.add(1, EF_LV5_4_4)
    st = sc.start()
    attack(st, kshatriya)
    assert st.cards[same_lv].damage == 1
    assert st.cards[higher].damage == 0


@pytest.mark.card("GD01-093")
def test_gd01_093_unlinked_attack_does_nothing() -> None:
    sc = Scenario()
    unit = sc.add(0, EF_4_3, pilot="GD01-093")
    foe = sc.add(1, ZAKU_2_2)
    st = sc.start()
    attack(st, unit)
    assert st.cards[foe].damage == 0


# ---------------------------------------------------------------------------------------------
# GD01-094 Yzak Jule

YZAK_UNIT = ZEON_LV4_4_3  # 4/3 -> 5/4 with Yzak Jule


def enemy_link_unit(sc: Scenario, *, rested: bool = False, damage: int = 0) -> int:
    """A Lv.2 3/3 enemy Link Unit ((Earth Federation) link trait, (Earth Federation) Pilot)."""
    return sc.add(1, EF_LINK_2_2, pilot="GD01-089", rested=rested, damage=damage)


@pytest.mark.card("GD01-094")
def test_gd01_094_enemy_link_unit_destroyed_by_battle_damage_draws_1() -> None:
    sc = Scenario()
    attacker = sc.add(0, YZAK_UNIT, pilot="GD01-094")
    link = enemy_link_unit(sc, rested=True)
    st = sc.start()
    attack(st, attacker, link)
    pass_all(st)
    assert zone_of(st, link) is Zone.TRASH
    assert hand_size(st, 0) == 1


@pytest.mark.card("GD01-094")
@pytest.mark.rule("8-5-3-2-3", "10-1-6-4")
def test_gd01_094_draws_when_both_units_are_destroyed_in_battle() -> None:
    sc = Scenario()
    attacker = sc.add(0, ZAKU_2_2, pilot="GD01-094")  # 3/3
    link = enemy_link_unit(sc, rested=True)  # 3/3
    st = sc.start()
    attack(st, attacker, link)
    pass_all(st)
    assert zone_of(st, attacker) is Zone.TRASH
    assert zone_of(st, link) is Zone.TRASH
    assert hand_size(st, 0) == 1


@pytest.mark.card("GD01-094")
def test_gd01_094_paired_non_link_enemy_destroyed_does_not_draw() -> None:
    sc = Scenario()
    attacker = sc.add(0, YZAK_UNIT, pilot="GD01-094")
    not_link = sc.add(1, ZAKU_2_2, pilot="GD01-089", rested=True)
    st = sc.start()
    attack(st, attacker, not_link)
    pass_all(st)
    assert zone_of(st, not_link) is Zone.TRASH
    assert hand_size(st, 0) == 0


@pytest.mark.card("GD01-094")
@pytest.mark.ruling("GD01-094:Q149")
def test_gd01_094_destroy_effect_on_damaged_link_unit_does_not_draw() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    attacker = sc.add(0, YZAK_UNIT, pilot="GD01-094")
    destroyer = sc.add(0, DESTROY_ACTION, Zone.HAND)
    link = enemy_link_unit(sc, damage=1)
    sc.shields(1, ZAKU_2_2)
    st = sc.start()
    attack(st, attacker)
    pass_until(st, 0)
    act(st, A.PLAY_COMMAND, destroyer)
    assert zone_of(st, link) is Zone.TRASH
    assert hand_size(st, 0) == 0


@pytest.mark.card("GD01-094")
def test_gd01_094_effect_damage_while_attacking_draws_once_per_turn() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    attacker = sc.add(0, YZAK_UNIT, pilot="GD01-094")
    remnant = sc.add(0, "GD01-115", Zone.HAND)  # deal 1 damage to an enemy Unit
    fought = enemy_link_unit(sc, rested=True)
    weakened = enemy_link_unit(sc, damage=2)
    st = sc.start()
    attack(st, attacker, fought)
    pass_until(st, 0)
    act(st, A.PLAY_COMMAND, remnant)
    select(st, weakened)
    assert zone_of(st, weakened) is Zone.TRASH
    assert hand_size(st, 0) == 1
    pass_all(st)
    assert zone_of(st, fought) is Zone.TRASH
    assert hand_size(st, 0) == 1  # 【Once per Turn】


@pytest.mark.card("GD01-094")
def test_gd01_094_does_not_draw_while_another_unit_attacks() -> None:
    sc = Scenario()
    sc.add(0, YZAK_UNIT, pilot="GD01-094")
    other = sc.add(0, EF_LV5_4_4)
    link = enemy_link_unit(sc, rested=True)
    st = sc.start()
    attack(st, other, link)
    pass_all(st)
    assert zone_of(st, link) is Zone.TRASH
    assert hand_size(st, 0) == 0


# ---------------------------------------------------------------------------------------------
# GD01-095 Dearka Elthman

DEARKA_LINK = "GD01-050"  # (ZAFT) link trait


@pytest.mark.card("GD01-095")
@pytest.mark.rule("5-20-1")
def test_gd01_095_when_linked_discards_then_draws() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    buster = sc.add(0, DEARKA_LINK)
    dearka = sc.add(0, "GD01-095", Zone.HAND)
    spare = sc.add(0, ZAKU_2_2, Zone.HAND)
    sc.deck(0, "GD01-100")
    st = sc.start()
    play(st, dearka, onto=buster)
    assert zone_of(st, spare) is Zone.TRASH
    assert numbers_in(st, 0, Zone.HAND) == ["GD01-100"]


@pytest.mark.card("GD01-095")
def test_gd01_095_empty_hand_means_no_draw() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    buster = sc.add(0, DEARKA_LINK)
    dearka = sc.add(0, "GD01-095", Zone.HAND)
    st = sc.start()
    play(st, dearka, onto=buster)
    assert hand_size(st, 0) == 0


@pytest.mark.card("GD01-095")
def test_gd01_095_pairing_without_link_does_nothing() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    unit = sc.add(0, EF_4_3)
    dearka = sc.add(0, "GD01-095", Zone.HAND)
    spare = sc.add(0, ZAKU_2_2, Zone.HAND)
    st = sc.start()
    play(st, dearka, onto=unit)
    assert zone_of(st, spare) is Zone.HAND


# ---------------------------------------------------------------------------------------------
# GD01-096 Cagalli Yula Athha


@pytest.mark.card("GD01-096")
def test_gd01_096_white_unit_gains_blocker() -> None:
    sc = Scenario(active=1)
    raider = sc.add(1, ZAKU_2_2)
    unit = sc.add(0, EA_WHITE_2_2, pilot="GD01-096")
    st = sc.start()
    assert "Blocker" in keywords(st, unit)
    attack(st, raider)
    assert has_action(st, A.BLOCK, unit)


@pytest.mark.card("GD01-096")
def test_gd01_096_non_white_unit_does_not_gain_blocker() -> None:
    sc = Scenario()
    unit = sc.add(0, ZAKU_2_2, pilot="GD01-096")
    st = sc.start()
    assert "Blocker" not in keywords(st, unit)


# ---------------------------------------------------------------------------------------------
# GD01-097 Guel Jeturk


@pytest.mark.card("GD01-097")
def test_gd01_097_sets_unit_active_but_it_cannot_attack() -> None:
    sc = Scenario()
    unit = sc.add(0, ACADEMY_2_2, rested=True, pilot="GD01-097")
    sc.hand(1, *[ZAKU_2_2] * 8)
    sc.add(1, ZAKU_2_2, rested=True)
    st = sc.start()
    activate(st, unit)
    assert not st.cards[unit].rested
    assert not has_action(st, A.ATTACK, unit)
    assert not has_action(st, A.ACTIVATE, unit)  # 【Once per Turn】


@pytest.mark.card("GD01-097")
def test_gd01_097_opponent_with_7_cards_cannot_use_it() -> None:
    sc = Scenario()
    unit = sc.add(0, ACADEMY_2_2, rested=True, pilot="GD01-097")
    sc.hand(1, *[ZAKU_2_2] * 7)
    st = sc.start()
    assert not has_action(st, A.ACTIVATE, unit)
    assert st.cards[unit].rested


@pytest.mark.card("GD01-097")
@pytest.mark.ruling("GD01-097:Q150")
def test_gd01_097_cannot_attack_after_opponent_hand_shrinks() -> None:
    sc = Scenario()
    sc.resources(1, 2)
    unit = sc.add(0, ACADEMY_2_2, rested=True, pilot="GD01-097")
    other = sc.add(0, ZAKU_2_2)
    remnant = sc.add(1, "GD01-115", Zone.HAND)
    sc.hand(1, *[ZAKU_2_2] * 7)
    sc.shields(1, ZAKU_2_2, ZAKU_2_2)
    st = sc.start()
    activate(st, unit)
    attack(st, other)
    pass_until(st, 1)
    act(st, A.PLAY_COMMAND, remnant)
    select(st, unit)
    pass_all(st)
    assert st.pending is not None and st.pending.kind is DecisionKind.MAIN
    assert hand_size(st, 1) == 7
    assert not st.cards[unit].rested
    assert not has_action(st, A.ATTACK, unit)


# ---------------------------------------------------------------------------------------------
# GD01-098 Elan Ceres (Enhanced Person Number 4)


@pytest.mark.card("GD01-098")
@pytest.mark.rule("13-2-2-1", "13-2-13-1")
def test_gd01_098_recovers_1_hp_when_enemy_has_1_or_less_ap() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    unit = sc.add(0, ACADEMY_2_2, damage=2, pilot="GD01-098")  # 4/3
    sc.add(0, "GD01-115", Zone.HAND)
    sc.add(1, OZ_1_2)
    st = sc.start(Step.END_ACTION)
    pass_until(st, 0)
    activate(st, unit)
    assert st.cards[unit].damage == 1
    pass_until(st, 0)
    assert not has_action(st, A.ACTIVATE, unit)


@pytest.mark.card("GD01-098")
def test_gd01_098_not_usable_without_low_ap_enemy_or_damage() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    damaged = sc.add(0, ACADEMY_2_2, damage=2, pilot="GD01-098")
    undamaged = sc.add(0, ACADEMY_2_2, pilot="GD01-098")
    sc.add(0, "GD01-115", Zone.HAND)
    sc.add(1, ZAKU_2_2)
    st = sc.start(Step.END_ACTION)
    pass_until(st, 0)
    assert st.pending is not None and st.pending.kind is DecisionKind.ACTION_STEP
    assert not has_action(st, A.ACTIVATE, damaged)
    assert not has_action(st, A.ACTIVATE, undamaged)


# ---------------------------------------------------------------------------------------------
# GD01-099 Intercept Orders


@pytest.mark.card("GD01-099")
@pytest.mark.faq("Q96")
def test_gd01_099_rests_1_to_2_enemies_with_3_or_less_current_hp() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    orders = sc.add(0, "GD01-099", Zone.HAND)
    damaged = sc.add(1, EF_LV5_4_4, damage=1)
    three = sc.add(1, ZEON_2_3)
    four = sc.add(1, EF_LV4_3_4)
    st = sc.start()
    play(st, orders)
    select(st, damaged, three)
    assert st.cards[damaged].rested and st.cards[three].rested
    assert not st.cards[four].rested


@pytest.mark.card("GD01-099")
def test_gd01_099_may_rest_only_one() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    orders = sc.add(0, "GD01-099", Zone.HAND)
    first = sc.add(1, ZEON_2_3)
    second = sc.add(1, ZAKU_2_2)
    st = sc.start()
    play(st, orders)
    select(st, first)
    assert st.cards[first].rested
    assert not st.cards[second].rested


@pytest.mark.card("GD01-099")
@pytest.mark.rule("10-1-8-1-1")
def test_gd01_099_needs_a_target_to_be_played() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    orders = sc.add(0, "GD01-099", Zone.HAND)
    sc.add(1, EF_LV4_3_4)
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, orders)


@pytest.mark.card("GD01-099")
@pytest.mark.rule("13-2-5-1")
def test_gd01_099_burst_rests_enemy_with_5_or_less_hp() -> None:
    sc = Scenario(active=1)
    (shield,) = sc.shields(0, "GD01-099")
    mid = sc.add(1, EF_LV5_4_4)
    big = sc.add(1, BIG_6_6)
    st, _ = attack_my_shield(sc)
    yes(st)
    select(st, mid)
    assert st.cards[mid].rested
    assert not st.cards[big].rested
    assert zone_of(st, shield) is Zone.TRASH


# ---------------------------------------------------------------------------------------------
# GD01-100 A Show of Resolve


@pytest.mark.card("GD01-100")
def test_gd01_100_draws_2() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    resolve = sc.add(0, "GD01-100", Zone.HAND)
    st = sc.start()
    play(st, resolve)
    assert hand_size(st, 0) == 2
    assert zone_of(st, resolve) is Zone.TRASH


# ---------------------------------------------------------------------------------------------
# GD01-101 Deep Devotion


@pytest.mark.card("GD01-101")
def test_gd01_101_link_unit_recovers_3_hp() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    devotion = sc.add(0, "GD01-101", Zone.HAND)
    link = sc.add(0, BANAGHER_LINK_5_4, damage=4, pilot="GD01-088")
    plain = sc.add(0, EF_4_3, damage=2)
    st = sc.start()
    play(st, devotion)
    assert st.cards[link].damage == 1
    assert st.cards[plain].damage == 2


@pytest.mark.card("GD01-101")
def test_gd01_101_without_link_unit_only_pairing_is_possible() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    devotion = sc.add(0, "GD01-101", Zone.HAND)
    plain = sc.add(0, EF_4_3, damage=2)
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, devotion)
    assert has_action(st, A.PAIR, devotion, plain)


# ---------------------------------------------------------------------------------------------
# Command cards with 【Pilot】[Name]


@pytest.mark.parametrize(
    ("number", "ap_mod", "hp_mod"),
    [
        pytest.param("GD01-101", 1, 0, marks=pytest.mark.card("GD01-101")),
        pytest.param("GD01-103", 0, 1, marks=pytest.mark.card("GD01-103")),
        pytest.param("GD01-106", 1, 0, marks=pytest.mark.card("GD01-106")),
        pytest.param("GD01-110", 0, 1, marks=pytest.mark.card("GD01-110")),
        pytest.param("GD01-112", 1, 0, marks=pytest.mark.card("GD01-112")),
        pytest.param("GD01-113", 1, 0, marks=pytest.mark.card("GD01-113")),
        pytest.param("GD01-114", 1, 0, marks=pytest.mark.card("GD01-114")),
        pytest.param("GD01-116", 0, 1, marks=pytest.mark.card("GD01-116")),
        pytest.param("GD01-119", 1, 0, marks=pytest.mark.card("GD01-119")),
        pytest.param("GD01-122", 1, 0, marks=pytest.mark.card("GD01-122")),
    ],
)
@pytest.mark.rule("3-4-6-2", "5-9-1")
def test_command_pilot_can_be_paired(number: str, ap_mod: int, hp_mod: int) -> None:
    sc = Scenario()
    sc.resources(0, 6)
    unit = sc.add(0, ZAKU_2_2)
    command = sc.add(0, number, Zone.HAND)
    st = sc.start()
    play(st, command, onto=unit)
    assert zone_of(st, command) is Zone.PAIRED
    assert (ap(st, unit), hp(st, unit)) == (2 + ap_mod, 2 + hp_mod)


# ---------------------------------------------------------------------------------------------
# GD01-102 Securing the Supply Line


@pytest.mark.card("GD01-102")
def test_gd01_102_friendly_units_lv4_or_lower_recover_2() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    supply = sc.add(0, "GD01-102", Zone.HAND)
    lv3 = sc.add(0, EF_4_3, damage=2)
    lv5 = sc.add(0, EF_LV5_4_4, damage=2)
    foe = sc.add(1, EF_4_3, damage=2)
    st = sc.start()
    play(st, supply)
    assert st.cards[lv3].damage == 0
    assert st.cards[lv5].damage == 2
    assert st.cards[foe].damage == 2


# ---------------------------------------------------------------------------------------------
# GD01-103 The Stubborn Cog


@pytest.mark.card("GD01-103")
def test_gd01_103_rests_friendly_earth_federation_unit_and_enemy_unit() -> None:
    sc = Scenario()
    sc.resources(0, 1)
    cog = sc.add(0, "GD01-103", Zone.HAND)
    mine = sc.add(0, EF_4_3)
    foe = sc.add(1, ZAKU_2_2)
    st = sc.start()
    play(st, cog)
    assert st.cards[mine].rested and st.cards[foe].rested


@pytest.mark.card("GD01-103")
@pytest.mark.ruling("GD01-103:Q153")
@pytest.mark.rule("10-1-8-1-1", "10-2-2")
def test_gd01_103_cannot_be_played_without_active_friendly_earth_federation_unit() -> None:
    sc = Scenario()
    sc.resources(0, 1)
    cog = sc.add(0, "GD01-103", Zone.HAND)
    sc.add(0, EF_4_3, rested=True)
    sc.add(0, ZAKU_2_2)
    sc.add(1, ZAKU_2_2)
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, cog)


@pytest.mark.card("GD01-103")
def test_gd01_103_cannot_be_played_without_active_enemy_unit() -> None:
    sc = Scenario()
    sc.resources(0, 1)
    cog = sc.add(0, "GD01-103", Zone.HAND)
    sc.add(0, EF_4_3)
    sc.add(1, ZAKU_2_2, rested=True)
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, cog)


# ---------------------------------------------------------------------------------------------
# GD01-104 Signs of a Revolution


@pytest.mark.card("GD01-104")
def test_gd01_104_deals_2_damage_to_rested_enemy() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    signs = sc.add(0, "GD01-104", Zone.HAND)
    rested = sc.add(1, EF_LV5_4_4, rested=True)
    active = sc.add(1, EF_LV4_3_4)
    st = sc.start()
    play(st, signs)
    assert st.cards[rested].damage == 2
    assert st.cards[active].damage == 0


@pytest.mark.card("GD01-104")
def test_gd01_104_needs_a_rested_enemy() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    signs = sc.add(0, "GD01-104", Zone.HAND)
    sc.add(1, EF_LV4_3_4)
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, signs)


@pytest.mark.card("GD01-104")
def test_gd01_104_burst_draws_1() -> None:
    sc = Scenario(active=1)
    (shield,) = sc.shields(0, "GD01-104")
    sc.deck(0, "GD01-100")
    st, _ = attack_my_shield(sc)
    yes(st)
    assert numbers_in(st, 0, Zone.HAND) == ["GD01-100"]
    assert zone_of(st, shield) is Zone.TRASH


# ---------------------------------------------------------------------------------------------
# GD01-105 Citizens, Take a Stand!


@pytest.mark.card("GD01-105")
@pytest.mark.faq("Q105")
def test_gd01_105_units_in_play_get_ap_plus_2_this_turn() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    citizens = sc.add(0, "GD01-105", Zone.HAND)
    a = sc.add(0, ZAKU_2_2)
    b = sc.add(0, EF_4_3)
    late = sc.add(0, ZAKU_2_2, Zone.HAND)
    foe = sc.add(1, ZAKU_2_2)
    st = sc.start()
    play(st, citizens)
    play(st, late)
    assert (ap(st, a), ap(st, b), ap(st, late), ap(st, foe)) == (4, 6, 2, 2)
    to_next_turn(st)
    assert (ap(st, a), ap(st, b)) == (2, 4)


# ---------------------------------------------------------------------------------------------
# GD01-106 Fortress Defense


@pytest.mark.card("GD01-106")
@pytest.mark.rule("5-17-2")
def test_gd01_106_deploys_two_zaku_tokens() -> None:
    from gcg_sim.engine import view as V

    sc = Scenario()
    sc.resources(0, 5)
    fortress = sc.add(0, "GD01-106", Zone.HAND)
    st = sc.start()
    play(st, fortress)
    tokens = list(st.zones[0][Zone.BATTLE])
    assert len(tokens) == 2
    dv = V.derived(st)
    for t in tokens:
        cd = V.cdef(st, t)
        assert cd.is_token and cd.name == "Zaku Ⅱ"
        assert (ap(st, t), hp(st, t)) == (1, 1)
        assert V.traits_of(st, dv, t) == ("Zeon",)


# ---------------------------------------------------------------------------------------------
# GD01-107 First Contact


@pytest.mark.card("GD01-107")
def test_gd01_107_places_1_rested_resource() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    sc.resource_deck(0, 2)
    contact = sc.add(0, "GD01-107", Zone.HAND)
    st = sc.start()
    play(st, contact)
    area = st.zones[0][Zone.RESOURCE_AREA]
    assert len(area) == 4
    assert st.cards[area[-1]].rested
    assert len(st.zones[0][Zone.RESOURCE_DECK]) == 1


@pytest.mark.card("GD01-107")
def test_gd01_107_burst_places_ex_resource() -> None:
    sc = Scenario(active=1)
    sc.shields(0, "GD01-107")
    st, _ = attack_my_shield(sc)
    yes(st)
    assert numbers_in(st, 0, Zone.RESOURCE_AREA) == [sc.db.ex_resource.card_number]


# ---------------------------------------------------------------------------------------------
# GD01-108 Strategic Arms


@pytest.mark.card("GD01-108")
@pytest.mark.ruling("GD01-108:Q154")
def test_gd01_108_damages_every_blocker_including_mine() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    arms = sc.add(0, "GD01-108", Zone.HAND)
    mine = sc.add(0, "GD01-086")
    theirs = sc.add(1, "GD01-086")
    plain = sc.add(1, EF_4_3)
    st = sc.start()
    play(st, arms)
    assert st.cards[mine].damage == 2
    assert st.cards[theirs].damage == 2
    assert st.cards[plain].damage == 0


# ---------------------------------------------------------------------------------------------
# GD01-109 The Path to Victory or Defeat

OM_PILOT = "ST02-010"  # (Operation Meteor) Pilot
G_TEAM_UNIT = "GD05-077"  # (G Team) Unit, vanilla
OM_PILOT_COMMAND = "ST02-013"  # (Operation Meteor) Command with 【Pilot】[Quatre Raberba Winner]


@pytest.mark.card("GD01-109")
@pytest.mark.rule("10-3-5")
def test_gd01_109_adds_a_matching_unit_and_bottoms_the_rest() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    path = sc.add(0, "GD01-109", Zone.HAND)
    sc.deck(0, ZAKU_2_2, G_TEAM_UNIT, ZAKU_2_2, EF_4_3, ZAKU_2_2, "GD01-100")
    st = sc.start()
    play(st, path)
    yes(st)
    assert numbers_in(st, 0, Zone.HAND) == [G_TEAM_UNIT]
    deck = numbers_in(st, 0, Zone.DECK)
    assert deck[0] == "GD01-100"
    assert sorted(deck[-4:]) == sorted([ZAKU_2_2, ZAKU_2_2, EF_4_3, ZAKU_2_2])


@pytest.mark.card("GD01-109")
@pytest.mark.ruling("GD01-109:Q161")
@pytest.mark.rule("3-4-6-3")
def test_gd01_109_cannot_take_command_with_operation_meteor_pilot_effect() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    path = sc.add(0, "GD01-109", Zone.HAND)
    sc.deck(0, OM_PILOT_COMMAND, ZAKU_2_2, ZAKU_2_2, ZAKU_2_2, ZAKU_2_2, "GD01-100")
    st = sc.start()
    play(st, path)
    yes(st)
    assert hand_size(st, 0) == 0
    assert OM_PILOT_COMMAND in numbers_in(st, 0, Zone.DECK)[-5:]


@pytest.mark.card("GD01-109")
@pytest.mark.ruling("GD01-109:Q196")
def test_gd01_109_adds_only_one_card_even_with_unit_and_pilot() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    path = sc.add(0, "GD01-109", Zone.HAND)
    sc.deck(0, G_TEAM_UNIT, OM_PILOT, ZAKU_2_2, ZAKU_2_2, ZAKU_2_2, "GD01-100")
    st = sc.start()
    play(st, path)
    yes(st)
    assert st.pending is not None and st.pending.kind is DecisionKind.SELECT
    pilot = next(u for u in st.zones[0][Zone.DECK] if st.cards[u].def_id == sc.db[OM_PILOT].def_id)
    select(st, pilot)
    assert numbers_in(st, 0, Zone.HAND) == [OM_PILOT]
    assert G_TEAM_UNIT in numbers_in(st, 0, Zone.DECK)[-4:]


@pytest.mark.card("GD01-109")
def test_gd01_109_may_decline() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    path = sc.add(0, "GD01-109", Zone.HAND)
    sc.deck(0, G_TEAM_UNIT, ZAKU_2_2, ZAKU_2_2, ZAKU_2_2, ZAKU_2_2, "GD01-100")
    st = sc.start()
    play(st, path)
    act(st, A.NO)
    assert hand_size(st, 0) == 0
    assert numbers_in(st, 0, Zone.DECK)[0] == "GD01-100"


# ---------------------------------------------------------------------------------------------
# GD01-110 Rasid's Orders


@pytest.mark.card("GD01-110")
def test_gd01_110_lv4_unit_may_attack_active_enemy_with_6_or_less_ap() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    orders = sc.add(0, "GD01-110", Zone.HAND)
    mine = sc.add(0, EF_LV4_3_4)
    small = sc.add(0, ZAKU_2_2)
    foe = sc.add(1, EF_4_3)
    st = sc.start()
    assert not has_action(st, A.ATTACK, mine, foe)
    play(st, orders)
    assert has_action(st, A.ATTACK, mine, foe)
    assert not has_action(st, A.ATTACK, small, foe)
    to_next_turn(st)
    to_next_turn(st)
    assert not has_action(st, A.ATTACK, mine, foe)


@pytest.mark.card("GD01-110")
def test_gd01_110_active_enemy_with_7_or_more_ap_is_not_a_legal_target() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    orders = sc.add(0, "GD01-110", Zone.HAND)
    mine = sc.add(0, EF_LV4_3_4)
    huge = sc.add(1, BIG_6_6, pilot="GD01-088")  # 8/8
    st = sc.start()
    play(st, orders)
    select(st, mine)
    assert not has_action(st, A.ATTACK, mine, huge)
    assert has_action(st, A.ATTACK, mine, PLAYER_TARGET)


# ---------------------------------------------------------------------------------------------
# GD01-111 Battle of Aces


@pytest.mark.card("GD01-111")
def test_gd01_111_deals_3_damage_to_damaged_enemy() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    aces = sc.add(0, "GD01-111", Zone.HAND)
    damaged = sc.add(1, BIG_6_6, damage=1)
    healthy = sc.add(1, EF_LV5_4_4)
    st = sc.start()
    play(st, aces)
    assert st.cards[damaged].damage == 4
    assert st.cards[healthy].damage == 0


@pytest.mark.card("GD01-111")
def test_gd01_111_needs_a_damaged_enemy() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    aces = sc.add(0, "GD01-111", Zone.HAND)
    sc.add(1, EF_LV5_4_4)
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, aces)


@pytest.mark.card("GD01-111")
def test_gd01_111_burst_deals_2_damage() -> None:
    sc = Scenario(active=1)
    sc.shields(0, "GD01-111")
    st, raider = attack_my_shield(sc)
    yes(st)
    assert st.cards[raider].damage == 2


# ---------------------------------------------------------------------------------------------
# GD01-112 Extreme Hatred


@pytest.mark.card("GD01-112")
@pytest.mark.rule("5-20-1")
def test_gd01_112_rests_two_units_then_deals_3_damage() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    hatred = sc.add(0, "GD01-112", Zone.HAND)
    a = sc.add(0, ZAKU_2_2)
    b = sc.add(0, EF_4_3)
    c = sc.add(0, ZEON_2_3)
    foe = sc.add(1, EF_LV5_4_4)
    st = sc.start()
    play(st, hatred)
    select(st, a, c)
    assert st.cards[a].rested and st.cards[c].rested
    assert not st.cards[b].rested
    assert st.cards[foe].damage == 3


@pytest.mark.card("GD01-112")
@pytest.mark.rule("10-1-8-1-1")
def test_gd01_112_needs_two_active_units() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    hatred = sc.add(0, "GD01-112", Zone.HAND)
    sc.add(0, ZAKU_2_2)
    sc.add(0, EF_4_3, rested=True)
    sc.add(1, EF_LV5_4_4)
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, hatred)


@pytest.mark.card("GD01-112")
@pytest.mark.rule("10-1-8-1-2")
def test_gd01_112_enemy_target_is_not_required_to_play() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    hatred = sc.add(0, "GD01-112", Zone.HAND)
    a = sc.add(0, ZAKU_2_2)
    b = sc.add(0, EF_4_3)
    st = sc.start()
    play(st, hatred)
    assert st.cards[a].rested and st.cards[b].rested


# ---------------------------------------------------------------------------------------------
# GD01-113 The Desert Tiger


@pytest.mark.card("GD01-113")
def test_gd01_113_zaft_unit_gets_ap_plus_3() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    tiger = sc.add(0, "GD01-113", Zone.HAND)
    zaft = sc.add(0, ZAFT_3_2)
    other = sc.add(0, ZAKU_2_2)
    st = sc.start()
    play(st, tiger)
    assert (ap(st, zaft), ap(st, other)) == (6, 2)
    to_next_turn(st)
    assert ap(st, zaft) == 3


@pytest.mark.card("GD01-113")
def test_gd01_113_needs_a_zaft_unit() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    tiger = sc.add(0, "GD01-113", Zone.HAND)
    sc.add(0, ZAKU_2_2)
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, tiger)


# ---------------------------------------------------------------------------------------------
# GD01-114 Assault on Torrington Base


@pytest.mark.card("GD01-114")
@pytest.mark.rule("13-2-4-1")
def test_gd01_114_two_friendly_units_get_ap_plus_1_in_action_step() -> None:
    sc = Scenario()
    sc.resources(0, 1)
    assault = sc.add(0, "GD01-114", Zone.HAND)
    a = sc.add(0, ZAKU_2_2)
    b = sc.add(0, EF_4_3)
    sc.shields(1, ZAKU_2_2)
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, assault)
    attack(st, a)
    pass_until(st, 0)
    act(st, A.PLAY_COMMAND, assault)
    pass_all(st)
    assert (ap(st, a), ap(st, b)) == (3, 5)
    to_next_turn(st)
    assert (ap(st, a), ap(st, b)) == (2, 4)


@pytest.mark.card("GD01-114")
def test_gd01_114_needs_two_friendly_units() -> None:
    sc = Scenario()
    sc.resources(0, 1)
    assault = sc.add(0, "GD01-114", Zone.HAND)
    a = sc.add(0, ZAKU_2_2)
    sc.shields(1, ZAKU_2_2)
    sc.add(1, ZAKU_2_2)
    st = sc.start()
    attack(st, a)
    pass_until(st, 0)
    assert not has_action(st, A.PLAY_COMMAND, assault)


# ---------------------------------------------------------------------------------------------
# GD01-115 Zeon Remnant Forces


@pytest.mark.card("GD01-115")
def test_gd01_115_deals_1_damage_to_enemy_unit() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    remnant = sc.add(0, "GD01-115", Zone.HAND)
    foe = sc.add(1, EF_4_3)
    st = sc.start()
    play(st, remnant)
    assert st.cards[foe].damage == 1


# ---------------------------------------------------------------------------------------------
# GD01-116 Stealth Stratagem


@pytest.mark.card("GD01-116")
def test_gd01_116_deals_2_damage_to_enemy_with_2_or_less_ap() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    stratagem = sc.add(0, "GD01-116", Zone.HAND)
    weak = sc.add(1, ZEON_2_3)
    strong = sc.add(1, EF_4_3)
    st = sc.start()
    play(st, stratagem)
    assert st.cards[weak].damage == 2
    assert st.cards[strong].damage == 0


# ---------------------------------------------------------------------------------------------
# GD01-117 The Witch and the Bride


@pytest.mark.card("GD01-117")
@pytest.mark.faq("Q96")
def test_gd01_117_returns_enemy_with_5_or_less_current_hp() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    witch = sc.add(0, "GD01-117", Zone.HAND)
    worn = sc.add(1, BIG_6_6, damage=1)
    fresh = sc.add(1, BIG_6_6)
    st = sc.start()
    play(st, witch)
    assert zone_of(st, worn) is Zone.HAND
    assert zone_of(st, fresh) is Zone.BATTLE


@pytest.mark.card("GD01-117")
def test_gd01_117_needs_a_target() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    witch = sc.add(0, "GD01-117", Zone.HAND)
    sc.add(1, BIG_6_6)
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, witch)


@pytest.mark.card("GD01-117")
@pytest.mark.rule("13-2-5-1")
def test_gd01_117_burst_activates_main_and_returns_attacker() -> None:
    sc = Scenario(active=1)
    (shield,) = sc.shields(0, "GD01-117")
    st, raider = attack_my_shield(sc)
    yes(st)
    assert zone_of(st, raider) is Zone.HAND
    assert zone_of(st, shield) is Zone.TRASH


# ---------------------------------------------------------------------------------------------
# GD01-118 Overflowing Affection


@pytest.mark.card("GD01-118")
def test_gd01_118_draws_2_then_discards_1() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    affection = sc.add(0, "GD01-118", Zone.HAND)
    kept = sc.add(0, ZAKU_2_2, Zone.HAND)
    sc.deck(0, "GD01-100", EF_4_3)
    st = sc.start()
    play(st, affection)
    assert st.pending is not None and st.pending.kind is DecisionKind.DISCARD
    select(st, kept)
    assert zone_of(st, kept) is Zone.TRASH
    assert sorted(numbers_in(st, 0, Zone.HAND)) == sorted(["GD01-100", EF_4_3])


# ---------------------------------------------------------------------------------------------
# GD01-119 Iron-Fisted Discipline


@pytest.mark.card("GD01-119")
def test_gd01_119_enemy_lv4_or_lower_gets_ap_minus_2_this_turn() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    discipline = sc.add(0, "GD01-119", Zone.HAND)
    lv4 = sc.add(1, EF_LV4_3_4)
    lv5 = sc.add(1, EF_LV5_4_4)
    st = sc.start()
    play(st, discipline)
    assert (ap(st, lv4), ap(st, lv5)) == (1, 4)
    to_next_turn(st)
    assert ap(st, lv4) == 3


# ---------------------------------------------------------------------------------------------
# GD01-120 Naval Bombardment


@pytest.mark.card("GD01-120")
def test_gd01_120_blocker_gets_ap_plus_3_while_blocking() -> None:
    sc = Scenario(active=1)
    sc.resources(0, 2)
    bombardment = sc.add(0, "GD01-120", Zone.HAND)
    lfrith = sc.add(0, "GD01-086")  # <Blocker> 2/4
    raider = sc.add(1, EF_LV5_4_4)
    st = sc.start()
    attack(st, raider)
    block(st, lfrith)
    pass_until(st, 0)
    act(st, A.PLAY_COMMAND, bombardment)
    pass_all(st)
    assert zone_of(st, raider) is Zone.TRASH  # 5 damage; 2 without the effect
    assert zone_of(st, lfrith) is Zone.TRASH


@pytest.mark.card("GD01-120")
def test_gd01_120_needs_a_friendly_blocker() -> None:
    sc = Scenario(active=1)
    sc.resources(0, 2)
    bombardment = sc.add(0, "GD01-120", Zone.HAND)
    sc.add(0, ZAKU_2_2)
    raider = sc.add(1, EF_LV5_4_4)
    sc.shields(0, ZAKU_2_2)
    st = sc.start()
    attack(st, raider)
    pass_until(st, 0)
    assert not has_action(st, A.PLAY_COMMAND, bombardment)


@pytest.mark.card("GD01-120")
def test_gd01_120_burst_gives_enemy_ap_minus_3() -> None:
    sc = Scenario(active=1)
    sc.shields(0, "GD01-120")
    st, raider = attack_my_shield(sc)
    yes(st)
    assert ap(st, raider) == 1


# ---------------------------------------------------------------------------------------------
# GD01-121 Midair Modifications


@pytest.mark.card("GD01-121")
def test_gd01_121_sets_rested_blocker_active_but_it_cannot_attack() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    midair = sc.add(0, "GD01-121", Zone.HAND)
    lfrith = sc.add(0, "GD01-086", rested=True)
    sc.add(1, ZAKU_2_2, rested=True)
    st = sc.start()
    play(st, midair)
    assert not st.cards[lfrith].rested
    assert not has_action(st, A.ATTACK, lfrith)


@pytest.mark.card("GD01-121")
@pytest.mark.ruling("GD01-121:Q155")
def test_gd01_121_still_cannot_attack_after_losing_blocker() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    midair = sc.add(0, "GD01-121", Zone.HAND)
    m1 = sc.add(0, "GD01-081", rested=True)
    partner = sc.add(0, "GD01-081")
    foe = sc.add(1, EF_LV5_4_4, rested=True)
    st = sc.start()
    play(st, midair)
    assert not st.cards[m1].rested
    attack(st, partner, foe)
    pass_all(st)
    assert zone_of(st, partner) is Zone.TRASH
    assert "Blocker" not in keywords(st, m1)
    assert not has_action(st, A.ATTACK, m1)


@pytest.mark.card("GD01-121")
def test_gd01_121_needs_a_rested_blocker() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    midair = sc.add(0, "GD01-121", Zone.HAND)
    sc.add(0, "GD01-086")
    sc.add(0, ZAKU_2_2, rested=True)
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, midair)


@pytest.mark.card("GD01-121")
@pytest.mark.rule("13-2-5-1")
def test_gd01_121_burst_sets_my_rested_blocker_active() -> None:
    sc = Scenario(active=1)
    sc.shields(0, "GD01-121")
    lfrith = sc.add(0, "GD01-086", rested=True)
    st, _ = attack_my_shield(sc)
    yes(st)
    assert not st.cards[lfrith].rested


# ---------------------------------------------------------------------------------------------
# GD01-122 Covert Operative


@pytest.mark.card("GD01-122")
def test_gd01_122_returns_enemy_with_2_or_less_hp() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    covert = sc.add(0, "GD01-122", Zone.HAND)
    small = sc.add(1, ZAKU_2_2)
    mid = sc.add(1, EF_4_3)
    st = sc.start()
    play(st, covert)
    assert zone_of(st, small) is Zone.HAND
    assert zone_of(st, mid) is Zone.BATTLE


@pytest.mark.card("GD01-122")
@pytest.mark.rule("10-1-9-1-1")
def test_gd01_122_with_link_unit_returns_enemy_with_4_or_less_hp_instead() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    covert = sc.add(0, "GD01-122", Zone.HAND)
    sc.add(0, BANAGHER_LINK_5_4, pilot="GD01-088")
    four = sc.add(1, EF_LV4_3_4)
    five = sc.add(1, BIG_6_6, damage=1)
    st = sc.start()
    play(st, covert)
    assert zone_of(st, four) is Zone.HAND
    assert zone_of(st, five) is Zone.BATTLE


@pytest.mark.card("GD01-122")
def test_gd01_122_without_link_unit_needs_a_2_hp_target() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    covert = sc.add(0, "GD01-122", Zone.HAND)
    sc.add(0, EF_4_3)
    sc.add(1, EF_4_3)
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, covert)


# ---------------------------------------------------------------------------------------------
# Bases: 【Burst】Deploy this card. / 【Deploy】Add 1 of your Shields to your hand.


@pytest.mark.parametrize(
    "number",
    [
        pytest.param("GD01-123", marks=pytest.mark.card("GD01-123")),
        pytest.param("GD01-124", marks=pytest.mark.card("GD01-124")),
        pytest.param("GD01-125", marks=pytest.mark.card("GD01-125")),
        pytest.param("GD01-126", marks=pytest.mark.card("GD01-126")),
        pytest.param("GD01-127", marks=pytest.mark.card("GD01-127")),
        pytest.param("GD01-128", marks=pytest.mark.card("GD01-128")),
        pytest.param("GD01-129", marks=pytest.mark.card("GD01-129")),
        pytest.param("GD01-130", marks=pytest.mark.card("GD01-130")),
    ],
)
@pytest.mark.rule("13-2-5-1", "13-2-6-1")
def test_base_burst_deploys_it_and_adds_next_shield_to_hand(number: str) -> None:
    sc = Scenario(active=1)
    base, second = sc.shields(0, number, ZAKU_2_2)
    sc.add(1, BIG_6_6)
    st, _ = attack_my_shield(sc)
    yes(st)
    assert zone_of(st, base) is Zone.BASE
    assert zone_of(st, second) is Zone.HAND


@pytest.mark.parametrize(
    "number",
    [
        pytest.param("GD01-124", marks=pytest.mark.card("GD01-124")),
        pytest.param("GD01-126", marks=pytest.mark.card("GD01-126")),
        pytest.param("GD01-127", marks=pytest.mark.card("GD01-127")),
        pytest.param("GD01-128", marks=pytest.mark.card("GD01-128")),
        pytest.param("GD01-130", marks=pytest.mark.card("GD01-130")),
    ],
)
@pytest.mark.rule("4-6-4-1", "11-5-2")
def test_base_played_replaces_ex_base_and_adds_top_shield_to_hand(number: str) -> None:
    sc = Scenario()
    sc.resources(0, 4)
    base = sc.add(0, number, Zone.HAND)
    ex_base = sc.base(0)
    top, second = sc.shields(0, ZAKU_2_2, ZEON_2_3)
    st = sc.start()
    play(st, base)
    assert zone_of(st, base) is Zone.BASE
    assert zone_of(st, ex_base) is not Zone.BASE
    assert zone_of(st, top) is Zone.HAND
    assert zone_of(st, second) is Zone.SHIELD


# ---------------------------------------------------------------------------------------------
# GD01-123 Nahel Argama


@pytest.mark.card("GD01-123")
@pytest.mark.faq("Q96")
def test_gd01_123_deploy_adds_shield_then_rests_enemy_with_3_or_less_hp() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    nahel = sc.add(0, "GD01-123", Zone.HAND)
    (top,) = sc.shields(0, ZAKU_2_2)
    worn = sc.add(1, EF_LV5_4_4, damage=1)
    fresh = sc.add(1, EF_LV5_4_4)
    st = sc.start()
    play(st, nahel)
    assert zone_of(st, top) is Zone.HAND
    assert st.cards[worn].rested
    assert not st.cards[fresh].rested


@pytest.mark.card("GD01-123")
@pytest.mark.rule("5-20-2")
def test_gd01_123_rests_enemy_even_without_shields() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    nahel = sc.add(0, "GD01-123", Zone.HAND)
    foe = sc.add(1, EF_4_3)
    st = sc.start()
    play(st, nahel)
    assert st.cards[foe].rested


# ---------------------------------------------------------------------------------------------
# GD01-124 Side 7


@pytest.mark.card("GD01-124")
@pytest.mark.rule("10-1-7-2")
def test_gd01_124_rest_to_recover_1_hp() -> None:
    sc = Scenario()
    side7 = sc.base(0, "GD01-124")
    unit = sc.add(0, EF_4_3, damage=2)
    st = sc.start()
    activate(st, side7)
    assert st.cards[unit].damage == 1
    assert st.cards[side7].rested
    assert not has_action(st, A.ACTIVATE, side7)


@pytest.mark.card("GD01-124")
def test_gd01_124_needs_a_friendly_unit() -> None:
    sc = Scenario()
    side7 = sc.base(0, "GD01-124")
    sc.add(1, EF_4_3)
    st = sc.start()
    assert not has_action(st, A.ACTIVATE, side7)


# ---------------------------------------------------------------------------------------------
# GD01-125 Zanzibar

DOPP = "GD01-039"  # (Zeon) Lv1 Unit with 【Deploy】Look at the top card of your deck ...


@pytest.mark.card("GD01-125")
@pytest.mark.ruling("GD01-125:Q157", "GD01-125:Q158")
def test_gd01_125_deploys_zeon_unit_from_hand_for_free_and_its_deploy_triggers() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    zanzibar = sc.add(0, "GD01-125", Zone.HAND)
    dopp = sc.add(0, DOPP, Zone.HAND)
    too_high = sc.add(0, "GD04-027", Zone.HAND)  # (Zeon) Lv5
    not_zeon = sc.add(0, EF_4_3, Zone.HAND)
    (top,) = sc.shields(0, EF_4_3)
    st = sc.start()
    play(st, zanzibar)
    assert zone_of(st, top) is Zone.HAND
    yes(st)
    assert zone_of(st, dopp) is Zone.BATTLE
    assert zone_of(st, too_high) is Zone.HAND and zone_of(st, not_zeon) is Zone.HAND
    active = [u for u in st.zones[0][Zone.RESOURCE_AREA] if not st.cards[u].rested]
    assert len(active) == 2
    assert st.pending is not None and st.pending.kind is DecisionKind.ARRANGE


@pytest.mark.card("GD01-125")
@pytest.mark.rule("10-1-3")
def test_gd01_125_deploy_is_optional() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    zanzibar = sc.add(0, "GD01-125", Zone.HAND)
    dopp = sc.add(0, DOPP, Zone.HAND)
    st = sc.start()
    play(st, zanzibar)
    act(st, A.NO)
    assert zone_of(st, dopp) is Zone.HAND


@pytest.mark.card("GD01-125")
def test_gd01_125_burst_on_opponent_turn_cannot_deploy_from_hand() -> None:
    sc = Scenario(active=1)
    base, _ = sc.shields(0, "GD01-125", ZAKU_2_2)
    dopp = sc.add(0, DOPP, Zone.HAND)
    st, _ = attack_my_shield(sc)
    yes(st)
    assert zone_of(st, base) is Zone.BASE
    assert zone_of(st, dopp) is Zone.HAND
    assert st.pending is None or st.pending.kind is not DecisionKind.YES_NO


# ---------------------------------------------------------------------------------------------
# GD01-127 Gamow


@pytest.mark.card("GD01-127")
@pytest.mark.rule("13-1-2-1")
def test_gd01_127_zaft_unit_gains_breach_3_during_battle() -> None:
    sc = Scenario()
    gamow = sc.base(0, "GD01-127")
    zaft = sc.add(0, ZAFT_LV5_5_4)
    foe = sc.add(1, ZAKU_2_2, rested=True)
    top, second = sc.shields(1, ZAKU_2_2, ZAKU_2_2)
    st = sc.start()
    attack(st, zaft, foe)
    pass_until(st, 0)
    activate(st, gamow)
    assert st.cards[gamow].rested
    pass_all(st)
    assert zone_of(st, foe) is Zone.TRASH
    assert zone_of(st, top) is Zone.TRASH
    assert zone_of(st, second) is Zone.SHIELD
    assert "Breach" not in keywords(st, zaft)


@pytest.mark.card("GD01-127")
def test_gd01_127_needs_zaft_unit_with_5_or_more_ap() -> None:
    sc = Scenario()
    gamow = sc.base(0, "GD01-127")
    zaft = sc.add(0, ZAFT_3_2)
    sc.shields(1, ZAKU_2_2)
    st = sc.start()
    attack(st, zaft)
    pass_until(st, 0)
    assert not has_action(st, A.ACTIVATE, gamow)


@pytest.mark.card("GD01-127")
@pytest.mark.xfail(
    strict=True,
    reason="ENGINE: 'during this battle' lasting effects created outside a battle never expire",
)
def test_gd01_127_used_in_end_phase_does_not_outlast_the_turn() -> None:
    sc = Scenario()
    gamow = sc.base(0, "GD01-127")
    zaft = sc.add(0, ZAFT_LV5_5_4)
    st = sc.start(Step.END_ACTION)
    activate(st, gamow)
    to_next_turn(st)
    assert "Breach" not in keywords(st, zaft)


# ---------------------------------------------------------------------------------------------
# GD01-129 Kusanagi


@pytest.mark.card("GD01-129")
@pytest.mark.faq("Q96")
def test_gd01_129_deploy_adds_shield_then_returns_enemy_with_3_or_less_hp() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    kusanagi = sc.add(0, "GD01-129", Zone.HAND)
    (top,) = sc.shields(0, ZAKU_2_2)
    worn = sc.add(1, EF_LV5_4_4, damage=1)
    fresh = sc.add(1, EF_LV5_4_4)
    st = sc.start()
    play(st, kusanagi)
    assert zone_of(st, top) is Zone.HAND
    assert zone_of(st, worn) is Zone.HAND
    assert zone_of(st, fresh) is Zone.BATTLE


# ---------------------------------------------------------------------------------------------
# GD01-130 13th Tactical Testing Sector


@pytest.mark.card("GD01-130")
def test_gd01_130_with_academy_unit_enemy_gets_ap_minus_1() -> None:
    sc = Scenario()
    sector = sc.base(0, "GD01-130")
    sc.add(0, ACADEMY_2_2)
    foe = sc.add(1, EF_4_3)
    st = sc.start()
    activate(st, sector)
    assert st.cards[sector].rested
    assert ap(st, foe) == 3
    to_next_turn(st)
    assert ap(st, foe) == 4


@pytest.mark.card("GD01-130")
def test_gd01_130_without_academy_unit_does_nothing() -> None:
    sc = Scenario()
    sector = sc.base(0, "GD01-130")
    sc.add(0, ZAKU_2_2)
    foe = sc.add(1, EF_4_3)
    st = sc.start()
    activate(st, sector)
    assert st.cards[sector].rested
    assert ap(st, foe) == 4


@pytest.mark.card("GD01-130")
@pytest.mark.rule("10-2-2")
def test_gd01_130_with_academy_unit_needs_an_enemy_target() -> None:
    sc = Scenario()
    sector = sc.base(0, "GD01-130")
    sc.add(0, ACADEMY_2_2)
    st = sc.start()
    assert not has_action(st, A.ACTIVATE, sector)
