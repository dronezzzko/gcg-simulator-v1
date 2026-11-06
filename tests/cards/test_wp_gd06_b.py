"""Card behaviour and ruling tests for GD06-066..GD06-130 and the GD06 tokens T-030..T-037
(work package WP-GD06-B)."""

from __future__ import annotations

import pytest

from gcg_sim.cards.model import CardType
from gcg_sim.deck.model import Deck
from gcg_sim.deck.validate import check_card_types
from gcg_sim.engine import view as V
from gcg_sim.engine.game import apply
from gcg_sim.engine.state import GameState
from gcg_sim.engine.types import PLAYER_TARGET, ActionKind, DecisionKind, Step, Zone
from gcg_sim.testkit import (
    Scenario,
    activate,
    ap,
    attack,
    block,
    has_action,
    keywords,
    options,
    order,
    pass_all,
    play,
    select,
    select_if_asked,
    to_next_turn,
    yes,
    zone_of,
)

A = ActionKind
VANILLA = "GD01-060"  # Zaku Mariner: Lv2, cost 1, AP2 HP2, no effects (Zeon)
BIG = "GD03-047"  # DINN (Commander Type): Lv4, AP3 HP4, no effects
HUGE = "GD04-055"  # Heine's Gouf Ignited: Lv5, AP5 HP4, no effects
BLOCKER = "GD01-072"  # Launcher Strike Gundam: AP3 HP4 <Blocker>
RIDDHE = "GD01-089"  # Pilot Lv3, AP+1 HP+1, no trigger
DRAW_TWO = "GD01-100"  # Command: 【Main】Draw 2.
GYUNEI = "GD05-057"  # 【Activate･Main】destroy 1 of your other Units
MIDAIR = "GD01-121"  # Command: set 1 rested Unit with <Blocker> active
DARKNESS_FINGER = "GD05-110"  # (Special Move) Command: deal 2 damage to 1 enemy Unit
DRAGON = "GD05-035"  # Dragon Gundam: 【During Link】【Attack】activate 【Main】 on the paired card
HOKA = "GD05-112"  # (Special Move) Command, 【Pilot】[Sai Saici]
FLAT = "GD06-074"  # vanilla (Dianna Counter) Unit, Lv4 4/3
MUTTOWOOO = "GD06-079"  # vanilla (Dianna Counter) Unit, Lv2 3/2
FLEET = "GD06-031"  # vanilla (Ghingnham Fleet) Unit, Lv2 3/2
NEO_ZEON = "GD06-028"  # vanilla (Neo Zeon) Unit, Lv2 2/2
KSHATRIYA = "GD01-051"  # vanilla (Neo Zeon) Unit, Lv4 3/4
ALPHACIDE = "GD06-063"  # vanilla (Zeon)(Alphacide) Unit, Lv2 2/2
NEMO = "GD02-080"  # vanilla (AEUG) Unit, Lv2 2/2
MK2 = "GD06-040"  # vanilla (AEUG)(Gundam Team) Unit, Lv4 3/4
GRAZE = "ST05-009"  # vanilla (Gjallarhorn) Unit, Lv2 2/2
DOM_TROPEN = "GD06-059"  # vanilla (Zeon)(Delaz Fleet) Unit, Lv3 3/3
SERPENT = "GD06-075"  # vanilla (Mariemaia Army) Unit, Lv3 4/3
GOOHN = "GD01-062"  # vanilla (ZAFT) Unit, Lv1 1/2
ALT_PLAY = 100  # the first play modifier's action ("When playing this card ... cost -1")


def hand_count(st: GameState, player: int = 0) -> int:
    return len(st.zones[player][Zone.HAND])


def pending(st: GameState) -> DecisionKind | None:
    return st.pending.kind if st.pending is not None else None


def select_options(st: GameState) -> set[int]:
    return {o.a for o in options(st) if o.kind is A.SELECT}


def settle(st: GameState) -> None:
    """Pass action steps and blocks and resolve waiting triggers in their default order."""
    for _ in range(20):
        pass_all(st)
        if pending(st) is DecisionKind.ORDER_TRIGGER:
            order(st)
        else:
            return


def battle(st: GameState, attacker: int, target: int = PLAYER_TARGET) -> None:
    attack(st, attacker, target)
    settle(st)


def act_pass(st: GameState) -> None:
    for o in options(st):
        if o.kind is A.PASS:
            apply(st, o)
            return
    raise AssertionError("no pass")


def pass_until(st: GameState, player: int) -> None:
    """Pass action-step priority until ``player`` holds it."""
    for _ in range(10):
        if pending(st) is DecisionKind.ACTION_STEP and st.pending is not None:
            if st.pending.player == player:
                return
            act_pass(st)
        else:
            return


def hold_action(sc: Scenario) -> None:
    """Give player 0 a playable 【Action】 card, so the action step of a battle happens."""
    sc.resources(0, 2)
    sc.hand(0, "GD06-110")


def destroy_with_gyunei(st: GameState, gyunei: int, victim: int) -> None:
    activate(st, gyunei)
    select_if_asked(st, victim, done=False)


def names_in(st: GameState, player: int, zone: Zone) -> list[str]:
    return sorted(V.cdef(st, u).name for u in st.zones[player][zone])


# ---------------------------------------------------------------------------------------------
# GD06-066 Shadow Gundam


@pytest.mark.card("GD06-066")
@pytest.mark.ruling("GD06-066:Q526", "GD06-066:Q527")
def test_gd06_066_linked_attack_activates_the_paired_commands_action() -> None:
    sc = Scenario()
    res = sc.resources(0, 3)
    shadow = sc.add(0, "GD06-066", pilot="GD06-119")
    small = sc.add(1, VANILLA)
    st = sc.start()
    pilot = st.cards[shadow].pair
    assert "Blocker" in keywords(st, shadow)
    attack(st, shadow)
    select_if_asked(st, small)
    assert st.cards[small].rested
    assert st.cards[shadow].pair == pilot and zone_of(st, pilot) is Zone.PAIRED
    assert not any(st.cards[r].rested for r in res)
    settle(st)


@pytest.mark.card("GD06-066")
def test_gd06_066_unlinked_attack_activates_nothing() -> None:
    sc = Scenario()
    shadow = sc.add(0, "GD06-066", pilot=RIDDHE)
    small = sc.add(1, VANILLA)
    st = sc.start()
    battle(st, shadow)
    assert not st.cards[small].rested


# ---------------------------------------------------------------------------------------------
# GD06-067 Gundam Sandrock Custom (EW)


@pytest.mark.card("GD06-067")
@pytest.mark.ruling("GD06-067:Q528")
def test_gd06_067_rested_enemy_skips_the_next_start_phase_untap_only() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    sandrock = sc.add(0, "GD06-067")
    pilot = sc.add(0, "GD06-080", Zone.HAND)
    frozen = sc.add(1, BLOCKER, rested=True, damage=1)
    sc.resources(1, 3)
    midair = sc.add(1, MIDAIR, Zone.HAND)
    st = sc.start()
    play(st, pilot, onto=sandrock)
    select_if_asked(st, frozen)
    to_next_turn(st)
    assert st.active == 1
    assert st.cards[frozen].rested
    play(st, midair)
    select_if_asked(st, frozen)
    assert not st.cards[frozen].rested


@pytest.mark.card("GD06-067")
def test_gd06_067_low_level_pilot_does_not_trigger() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    sandrock = sc.add(0, "GD06-067")
    riddhe = sc.add(0, RIDDHE, Zone.HAND)
    frozen = sc.add(1, BLOCKER, rested=True, damage=1)
    st = sc.start()
    play(st, riddhe, onto=sandrock)
    to_next_turn(st)
    assert not st.cards[frozen].rested


# ---------------------------------------------------------------------------------------------
# GD06-068 Gundam Heavyarms Custom (EW) / GD06-069 SUMO Silver Type


@pytest.mark.card("GD06-068")
@pytest.mark.ruling("GD06-068:Q529")
def test_gd06_068_rests_and_weakens_with_another_g_team_unit() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    heavyarms = sc.add(0, "GD06-068", Zone.HAND)
    sc.add(0, "GD06-064")
    already = sc.add(1, VANILLA, rested=True)
    st = sc.start()
    play(st, heavyarms)
    select_if_asked(st, already)
    assert st.cards[already].rested
    assert ap(st, already) == 0


@pytest.mark.card("GD06-068")
def test_gd06_068_without_a_partner_only_rests() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    heavyarms = sc.add(0, "GD06-068", Zone.HAND)
    enemy = sc.add(1, VANILLA)
    st = sc.start()
    play(st, heavyarms)
    select_if_asked(st, enemy)
    assert st.cards[enemy].rested
    assert ap(st, enemy) == 2


@pytest.mark.card("GD06-069")
def test_gd06_069_pays_1_to_heal_a_damaged_dianna_counter_unit_in_battle() -> None:
    sc = Scenario()
    res = sc.resources(0, 1)
    silver = sc.add(0, "GD06-069", damage=2)
    attacker = sc.add(0, FLAT)
    st = sc.start()
    attack(st, attacker)
    pass_until(st, 0)
    activate(st, silver)
    select_if_asked(st, silver)
    assert st.cards[silver].damage == 1
    assert st.cards[res[0]].rested
    assert "Blocker" in keywords(st, silver)
    settle(st)


# ---------------------------------------------------------------------------------------------
# GD06-070 Eagail


@pytest.mark.card("GD06-070", "T-030")
def test_gd06_070_deploys_a_gozzo_token_with_blocker() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    eagail = sc.add(0, "GD06-070", Zone.HAND)
    st = sc.start()
    play(st, eagail)
    (gozzo,) = (u for u in st.zones[0][Zone.BATTLE] if u != eagail)
    assert V.cdef(st, gozzo).card_number == "T-030"
    assert "Blocker" in keywords(st, gozzo)
    assert ap(st, gozzo) == 0


@pytest.mark.card("GD06-070")
def test_gd06_070_attack_after_paying_three_for_unit_effects_weakens_an_enemy() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    eagail = sc.add(0, "GD06-070")
    wad = sc.add(0, "GD06-078")
    sc.add(0, FLAT)
    small = sc.add(1, VANILLA)
    enemy = sc.add(1, HUGE, rested=True)
    st = sc.start()
    activate(st, wad)
    select_if_asked(st, small)
    attack(st, eagail, enemy)
    select_if_asked(st, enemy)
    settle(st)
    assert zone_of(st, enemy) is Zone.TRASH
    assert zone_of(st, eagail) is Zone.BATTLE  # the enemy fought with AP 5-5
    assert st.cards[eagail].damage == 0


@pytest.mark.card("GD06-070")
def test_gd06_070_attack_after_paying_two_for_unit_effects_does_nothing() -> None:
    sc = Scenario()
    res = sc.resources(0, 2)
    eagail = sc.add(0, "GD06-070")
    first = sc.add(0, FLAT)
    silvers = [sc.add(0, "GD06-069", damage=1), sc.add(0, "GD06-069", damage=1)]
    sc.shields(1, VANILLA, VANILLA)
    enemy = sc.add(1, HUGE, rested=True)
    st = sc.start()
    attack(st, first, PLAYER_TARGET)
    for silver in silvers:  # in the action step of an earlier battle this turn
        pass_until(st, 0)
        activate(st, silver)
        select_if_asked(st, silver)
        assert st.cards[silver].damage == 0
    assert all(st.cards[r].rested for r in res)  # paid (1)(1) for Unit effects this turn
    settle(st)
    attack(st, eagail, enemy)
    settle(st)
    assert zone_of(st, eagail) is Zone.TRASH  # the enemy fought with its full AP 5


@pytest.mark.card("GD06-070")
def test_gd06_070_paying_after_the_attack_trigger_does_not_count_for_it() -> None:
    sc = Scenario()
    sc.resources(0, 1)
    eagail = sc.add(0, "GD06-070")
    silver = sc.add(0, "GD06-069", damage=1)
    enemy = sc.add(1, HUGE, rested=True)
    st = sc.start()
    attack(st, eagail, enemy)
    pass_until(st, 0)
    activate(st, silver)
    select_if_asked(st, silver)
    assert ap(st, enemy) == 5
    settle(st)


# ---------------------------------------------------------------------------------------------
# GD06-071 Shining Gundam (Meikyo Shisui)


@pytest.mark.card("GD06-071")
def test_gd06_071_special_move_weakens_an_enemy_and_boosts_itself_once_per_turn() -> None:
    sc = Scenario()
    sc.resources(0, 8)
    shining = sc.add(0, "GD06-071")
    fingers = sc.hand(0, DARKNESS_FINGER, DARKNESS_FINGER)
    enemy = sc.add(1, HUGE)
    other = sc.add(1, HUGE)
    st = sc.start()
    play(st, fingers[0])
    select(st, enemy)
    select_if_asked(st, other)
    assert ap(st, shining) == 4 + 2
    assert ap(st, enemy) + ap(st, other) == 5 + 3
    play(st, fingers[1])
    select_if_asked(st, other)
    assert ap(st, shining) == 6


@pytest.mark.card("GD06-071")
@pytest.mark.ruling("GD06-071:Q530")
def test_gd06_071_counts_a_paired_special_move_activated_by_dragon_gundam() -> None:
    sc = Scenario()
    shining = sc.add(0, "GD06-071")
    dragon = sc.add(0, DRAGON, pilot=HOKA)
    enemy = sc.add(1, HUGE)
    st = sc.start()
    attack(st, dragon)
    settle(st)
    for _ in range(4):
        if pending(st) is DecisionKind.SELECT:
            offered = select_options(st)
            select(st, enemy if enemy in offered else min(offered))
            settle(st)
    assert ap(st, shining) == 4 + 2
    assert ap(st, enemy) == 3


@pytest.mark.card("GD06-071")
def test_gd06_071_not_on_the_opponents_turn() -> None:
    sc = Scenario(active=1)
    sc.resources(0, 4)
    shining = sc.add(0, "GD06-071")
    finger = sc.add(0, DARKNESS_FINGER, Zone.HAND)
    attacker = sc.add(1, HUGE)
    st = sc.start()
    attack(st, attacker, PLAYER_TARGET)
    pass_until(st, 0)
    play(st, finger)
    select_if_asked(st, attacker)
    assert ap(st, shining) == 4
    settle(st)


# ---------------------------------------------------------------------------------------------
# GD06-072 Gundam Deathscythe Hell (EW) / GD06-073 Wing Gundam Zero (EW)


@pytest.mark.card("GD06-072")
def test_gd06_072_paired_destroyed_returns_a_command_with_a_partner() -> None:
    sc = Scenario()
    hell = sc.add(0, "GD06-072", pilot=RIDDHE)
    sc.add(0, "GD06-064")
    gyunei = sc.add(0, GYUNEI)
    cmd = sc.add(0, DRAW_TWO, Zone.TRASH)
    st = sc.start()
    destroy_with_gyunei(st, gyunei, hell)
    select(st, cmd)
    assert zone_of(st, cmd) is Zone.HAND


@pytest.mark.card("GD06-072")
def test_gd06_072_unpaired_destroyed_does_nothing() -> None:
    sc = Scenario()
    hell = sc.add(0, "GD06-072")
    sc.add(0, "GD06-064")
    gyunei = sc.add(0, GYUNEI)
    cmd = sc.add(0, DRAW_TWO, Zone.TRASH)
    st = sc.start()
    destroy_with_gyunei(st, gyunei, hell)
    assert zone_of(st, cmd) is Zone.TRASH


@pytest.mark.card("GD06-073")
@pytest.mark.ruling("GD06-073:Q531")
def test_gd06_073_destroying_a_shield_or_the_base_rests_a_small_enemy() -> None:
    sc = Scenario()
    zero = sc.add(0, "GD06-073", pilot=RIDDHE)
    sc.base(1)
    small = sc.add(1, VANILLA)
    st = sc.start()
    st.cards[st.zones[1][Zone.BASE][0]].damage = 2
    st.touch()
    battle(st, zero)
    select_if_asked(st, small)
    assert st.cards[small].rested


# ---------------------------------------------------------------------------------------------
# GD06-076 WaDom / GD06-077 Cannon Illfuto / GD06-078 WaD


@pytest.mark.card("GD06-076")
def test_gd06_076_deploy_rests_an_enemy_with_4_or_less_hp() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    wadom = sc.add(0, "GD06-076", Zone.HAND)
    enemy = sc.add(1, BIG)
    sc.add(1, "GD06-007")
    st = sc.start()
    play(st, wadom)
    select_if_asked(st, enemy)
    assert st.cards[enemy].rested


@pytest.mark.card("GD06-077")
def test_gd06_077_has_blocker() -> None:
    sc = Scenario()
    cannon = sc.add(0, "GD06-077")
    st = sc.start()
    assert "Blocker" in keywords(st, cannon)


@pytest.mark.card("GD06-078")
def test_gd06_078_pays_3_and_rests_to_rest_an_enemy_with_a_partner() -> None:
    sc = Scenario()
    res = sc.resources(0, 3)
    wad = sc.add(0, "GD06-078")
    sc.add(0, FLAT)
    enemy = sc.add(1, BIG)
    st = sc.start()
    activate(st, wad)
    select_if_asked(st, enemy)
    assert st.cards[enemy].rested and st.cards[wad].rested
    assert all(st.cards[r].rested for r in res)


# ---------------------------------------------------------------------------------------------
# Pilots GD06-080..GD06-100


@pytest.mark.card("GD06-080")
def test_gd06_080_heals_after_an_effect_draw() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    unit = sc.add(0, BIG, pilot="GD06-080", damage=3)
    draw = sc.add(0, DRAW_TWO, Zone.HAND)
    attacker = sc.add(0, FLAT)
    st = sc.start()
    play(st, draw)
    attack(st, attacker)
    pass_until(st, 0)
    activate(st, unit)
    select_if_asked(st, unit)
    assert st.cards[unit].damage == 1
    pass_until(st, 0)
    assert not has_action(st, A.ACTIVATE, unit)
    settle(st)


@pytest.mark.card("GD06-080")
def test_gd06_080_draw_phase_draw_does_not_count() -> None:
    sc = Scenario()
    unit = sc.add(0, BIG, pilot="GD06-080", damage=3)
    attacker = sc.add(0, FLAT)
    st = sc.start(Step.DRAW_STEP)
    attack(st, attacker)
    pass_until(st, 0)
    assert not has_action(st, A.ACTIVATE, unit)
    settle(st)


@pytest.mark.card("GD06-081")
def test_gd06_081_alphacide_unit_gains_repair_2() -> None:
    sc = Scenario()
    alpha = sc.add(0, ALPHACIDE, pilot="GD06-081")
    other = sc.add(0, VANILLA, pilot="GD06-081")
    st = sc.start()
    assert keywords(st, alpha)["Repair"] == 2
    assert "Repair" not in keywords(st, other)


@pytest.mark.card("GD06-082")
def test_gd06_082_when_paired_damages_a_rested_enemy() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    unit = sc.add(0, VANILLA)
    athrun = sc.add(0, "GD06-082", Zone.HAND)
    rested = sc.add(1, BIG, rested=True)
    st = sc.start()
    play(st, athrun, onto=unit)
    select_if_asked(st, rested)
    assert st.cards[rested].damage == 1


@pytest.mark.card("GD06-083")
def test_gd06_083_linked_unit_with_repair_gains_high_maneuver() -> None:
    sc = Scenario()
    kikeroga = sc.add(0, "GD06-007", pilot="GD06-083")
    other = sc.add(0, BIG, pilot="GD06-083")
    st = sc.start()
    assert "High-Maneuver" in keywords(st, kikeroga)
    assert "High-Maneuver" not in keywords(st, other)


@pytest.mark.card("GD06-084")
def test_gd06_084_when_paired_gains_breach_per_neo_zeon_unit() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    unit = sc.add(0, NEO_ZEON)
    sc.add(0, NEO_ZEON)
    sc.add(0, "T-031")
    ple = sc.add(0, "GD06-084", Zone.HAND)
    st = sc.start()
    play(st, ple, onto=unit)
    assert keywords(st, unit)["Breach"] == 3
    sc.add(0, NEO_ZEON)
    st.touch()
    assert keywords(st, unit)["Breach"] == 3
    to_next_turn(st)
    assert "Breach" not in keywords(st, unit)


@pytest.mark.card("GD06-085")
def test_gd06_085_linked_fleet_units_get_ap_plus_1_on_the_opponents_turn() -> None:
    sc = Scenario()
    turn_x = sc.add(0, "GD06-019", pilot="GD06-085")
    fleet = sc.add(0, FLEET)
    st = sc.start()
    assert ap(st, fleet) == 3
    to_next_turn(st)
    assert st.active == 1
    assert ap(st, fleet) == 4
    assert ap(st, turn_x) == 5 + 2 + 1


@pytest.mark.card("GD06-086")
@pytest.mark.ruling("GD06-086:Q532")
def test_gd06_086_costs_0_on_a_mashymre_link_unit_but_keeps_its_level() -> None:
    sc = Scenario()
    res = sc.resources(0, 3)
    hamma = sc.add(0, "GD06-020")
    other = sc.add(0, KSHATRIYA)
    mashymre = sc.add(0, "GD06-086", Zone.HAND)
    st = sc.start()
    assert has_action(st, A.PAIR, mashymre, other)
    assert not any(o.kind is A.PAIR and o.b == other and o.c == ALT_PLAY for o in options(st))
    play(st, mashymre, onto=hamma, ex=ALT_PLAY)
    assert st.cards[hamma].pair == mashymre
    assert not any(st.cards[r].rested for r in res)


@pytest.mark.card("GD06-086")
@pytest.mark.ruling("GD06-086:Q532")
def test_gd06_086_level_is_not_reduced() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    hamma = sc.add(0, "GD06-020")
    mashymre = sc.add(0, "GD06-086", Zone.HAND)
    st = sc.start()
    assert not has_action(st, A.PAIR, mashymre, hamma)
    assert not any(o.kind is A.PAIR and o.c == ALT_PLAY for o in options(st))


@pytest.mark.card("GD06-086")
def test_gd06_086_burst_adds_it_to_the_hand() -> None:
    sc = Scenario(active=1)
    (mashymre,) = sc.shields(0, "GD06-086")
    sc.shields(0, VANILLA)
    attacker = sc.add(1, VANILLA)
    st = sc.start()
    attack(st, attacker, PLAYER_TARGET)
    settle(st)
    assert pending(st) is DecisionKind.BURST
    yes(st)
    settle(st)
    assert zone_of(st, mashymre) is Zone.HAND


@pytest.mark.card("GD06-087")
def test_gd06_087_attack_lets_a_fleet_unit_attack_an_active_low_level_enemy() -> None:
    sc = Scenario()
    unit = sc.add(0, FLEET, pilot="GD06-087")
    sc.shields(1, VANILLA, VANILLA)
    other = sc.add(0, FLEET)
    small = sc.add(1, VANILLA)
    st = sc.start()
    attack(st, unit)
    select_if_asked(st, other)
    settle(st)
    assert has_action(st, A.ATTACK, other, small)


@pytest.mark.card("GD06-088")
def test_gd06_088_linked_destroyed_returns_a_small_gundam_team_unit() -> None:
    sc = Scenario()
    zz = sc.add(0, "GD06-033", pilot="GD06-088")
    gyunei = sc.add(0, GYUNEI)
    fighter = sc.add(0, "GD06-039", Zone.TRASH)
    st = sc.start()
    destroy_with_gyunei(st, gyunei, zz)
    select_if_asked(st, fighter)
    assert zone_of(st, fighter) is Zone.HAND


@pytest.mark.card("GD06-089")
@pytest.mark.ruling("GD06-089:Q533")
def test_gd06_089_linked_destroying_a_shield_lets_a_gjallarhorn_unit_attack_active() -> None:
    sc = Scenario()
    bael = sc.add(0, "GD06-034", pilot="GD06-089")
    graze = sc.add(0, GRAZE)
    sc.shields(1, VANILLA)
    enemy = sc.add(1, BIG)
    st = sc.start()
    battle(st, bael)
    select_if_asked(st, graze)
    settle(st)
    assert has_action(st, A.ATTACK, graze, enemy)


@pytest.mark.card("GD06-090")
def test_gd06_090_attack_damages_with_two_gjallarhorn_units() -> None:
    sc = Scenario()
    unit = sc.add(0, GRAZE, pilot="GD06-090")
    sc.add(0, GRAZE)
    enemy = sc.add(1, BIG)
    st = sc.start()
    attack(st, unit)
    select_if_asked(st, enemy)
    assert st.cards[enemy].damage == 1
    settle(st)


@pytest.mark.card("GD06-090")
def test_gd06_090_name_is_also_gaelio_bauduin() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    vidar = sc.add(0, "GD06-036")
    pilot = sc.add(0, "GD06-090", Zone.HAND)
    enemy = sc.add(1, BIG)
    st = sc.start()
    play(st, pilot, onto=vidar)
    select_if_asked(st, enemy)
    assert vidar in V.derived(st).linked


@pytest.mark.card("GD06-091")
def test_gd06_091_aeug_unit_gains_breach_1() -> None:
    sc = Scenario()
    aeug = sc.add(0, NEMO, pilot="GD06-091")
    other = sc.add(0, VANILLA, pilot="GD06-091")
    st = sc.start()
    assert keywords(st, aeug)["Breach"] == 1
    assert "Breach" not in keywords(st, other)


@pytest.mark.card("GD06-092")
def test_gd06_092_phantom_pain_unit_damages_an_enemy_with_no_more_ap() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    windam = sc.add(0, "GD06-047")
    neo = sc.add(0, "GD06-092", Zone.HAND)
    weaker = sc.add(1, VANILLA)
    stronger = sc.add(1, HUGE)
    st = sc.start()
    play(st, neo, onto=windam)
    assert st.cards[weaker].damage == 1
    assert st.cards[stronger].damage == 0


@pytest.mark.card("GD06-092")
def test_gd06_092_other_units_do_nothing() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    unit = sc.add(0, VANILLA)
    neo = sc.add(0, "GD06-092", Zone.HAND)
    enemy = sc.add(1, GOOHN)
    st = sc.start()
    play(st, neo, onto=unit)
    assert st.cards[enemy].damage == 0


@pytest.mark.card("GD06-093")
@pytest.mark.ruling("GD06-093:Q534")
def test_gd06_093_battle_damage_from_a_lower_level_enemy_is_reduced_by_2() -> None:
    sc = Scenario()
    unit = sc.add(0, HUGE, pilot="GD06-093")
    lower = sc.add(1, BIG, rested=True)
    st = sc.start()
    battle(st, unit, lower)
    assert st.cards[unit].damage == 3 - 2


@pytest.mark.card("GD06-093")
def test_gd06_093_higher_level_enemy_is_not_reduced() -> None:
    sc = Scenario()
    unit = sc.add(0, GOOHN, pilot="GD06-093")
    higher = sc.add(1, BIG, rested=True)
    st = sc.start()
    battle(st, unit, higher)
    assert zone_of(st, unit) is Zone.TRASH


@pytest.mark.card("GD06-094")
@pytest.mark.ruling("GD06-094:Q535")
def test_gd06_094_granted_unit_destroys_what_it_battle_damages() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    unit = sc.add(0, VANILLA)
    goohn = sc.add(0, GOOHN)
    nyaan = sc.add(0, "GD06-094", Zone.HAND)
    victim = sc.add(1, BIG, rested=True)
    st = sc.start()
    play(st, nyaan, onto=unit)
    select(st, goohn)
    battle(st, goohn, victim)
    assert zone_of(st, victim) is Zone.TRASH  # 1 battle damage against HP 4, then destroyed


@pytest.mark.card("GD06-094")
@pytest.mark.ruling("GD06-094:Q535")
def test_gd06_094_a_zero_ap_unit_destroys_nothing() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    unit = sc.add(0, VANILLA)
    zero = sc.add(0, "GD06-032")
    nyaan = sc.add(0, "GD06-094", Zone.HAND)
    victim = sc.add(1, BIG, rested=True)
    st = sc.start()
    play(st, nyaan, onto=unit)
    select(st, zero)
    battle(st, zero, victim)
    assert zone_of(st, victim) is Zone.BATTLE


@pytest.mark.card("GD06-095")
def test_gd06_095_attack_may_take_damage_to_boost_a_zeon_unit() -> None:
    sc = Scenario()
    unit = sc.add(0, VANILLA, pilot="GD06-095")
    other = sc.add(0, DOM_TROPEN)
    st = sc.start()
    attack(st, unit)
    yes(st)
    select_if_asked(st, other)
    assert st.cards[unit].damage == 1
    assert ap(st, other) == 5
    settle(st)


@pytest.mark.card("GD06-096")
def test_gd06_096_linked_attack_milling_zeon_weakens_every_enemy() -> None:
    sc = Scenario()
    gyan = sc.add(0, "GD06-053", pilot="GD06-096")
    sc.shields(1, VANILLA, VANILLA)
    hold_action(sc)
    sc.deck(0, VANILLA)
    a = sc.add(1, BIG)
    b = sc.add(1, HUGE)
    st = sc.start()
    attack(st, gyan)
    pass_until(st, 0)
    assert (ap(st, a), ap(st, b)) == (2, 4)
    settle(st)
    assert (ap(st, a), ap(st, b)) == (3, 5)


@pytest.mark.card("GD06-097")
def test_gd06_097_blocking_sets_the_unit_active_once_per_turn() -> None:
    sc = Scenario(active=1)
    unit = sc.add(0, BLOCKER, pilot="GD06-097")
    a = sc.add(1, VANILLA)
    b = sc.add(1, VANILLA)
    st = sc.start()
    attack(st, a, PLAYER_TARGET)
    block(st, unit)
    settle(st)
    assert not st.cards[unit].rested
    attack(st, b, PLAYER_TARGET)
    block(st, unit)
    settle(st)
    assert st.cards[unit].rested


@pytest.mark.card("GD06-098")
@pytest.mark.ruling("GD06-098:Q536")
def test_gd06_098_rested_linked_unit_protects_bases_from_lower_level_units() -> None:
    sc = Scenario(active=1)
    sc.add(0, "GD06-072", pilot="GD06-098", rested=True)
    base = sc.base(0)
    lower = sc.add(1, BIG)
    higher = sc.add(1, "GD06-019")
    st = sc.start()
    battle(st, lower, PLAYER_TARGET)
    assert zone_of(st, base) is Zone.BASE  # the Lv.4 attacker's 3 damage would destroy it
    assert st.cards[base].damage == 0
    battle(st, higher, PLAYER_TARGET)
    assert zone_of(st, base) is Zone.OUTSIDE  # the Lv.8 attacker's damage destroyed the EX Base


@pytest.mark.card("GD06-098")
def test_gd06_098_active_unit_does_not_protect() -> None:
    sc = Scenario(active=1)
    sc.add(0, "GD06-072", pilot="GD06-098")
    base = sc.base(0)
    attacker = sc.add(1, GOOHN)
    st = sc.start()
    battle(st, attacker, PLAYER_TARGET)
    assert st.cards[base].damage == 1


@pytest.mark.card("GD06-099")
def test_gd06_099_attack_weakens_a_lv5_or_higher_enemy() -> None:
    sc = Scenario()
    unit = sc.add(0, SERPENT, pilot="GD06-099")
    sc.shields(1, VANILLA, VANILLA)
    hold_action(sc)
    big = sc.add(1, HUGE)
    sc.add(1, BIG)
    st = sc.start()
    attack(st, unit)
    select_if_asked(st, big)
    pass_until(st, 0)
    assert ap(st, big) == 2
    settle(st)
    assert ap(st, big) == 5


@pytest.mark.card("GD06-100")
@pytest.mark.ruling("GD06-100:Q537")
def test_gd06_100_costs_0_on_a_lv5_dianna_counter_unit_without_lowering_its_level() -> None:
    sc = Scenario()
    res = sc.resources(0, 4)
    eagail = sc.add(0, "GD06-070")
    flat = sc.add(0, FLAT)
    corin = sc.add(0, "GD06-100", Zone.HAND)
    st = sc.start()
    assert has_action(st, A.PAIR, corin, flat)
    assert not any(o.kind is A.PAIR and o.b == flat and o.c == ALT_PLAY for o in options(st))
    play(st, corin, onto=eagail, ex=ALT_PLAY)
    assert not any(st.cards[r].rested for r in res)


@pytest.mark.card("GD06-100")
@pytest.mark.ruling("GD06-100:Q537")
def test_gd06_100_still_needs_lv4() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    eagail = sc.add(0, "GD06-070")
    corin = sc.add(0, "GD06-100", Zone.HAND)
    st = sc.start()
    assert not has_action(st, A.PAIR, corin, eagail)


@pytest.mark.card("GD06-100")
def test_gd06_100_burst_adds_it_to_the_hand() -> None:
    sc = Scenario(active=1)
    (corin,) = sc.shields(0, "GD06-100")
    sc.shields(0, VANILLA)
    attacker = sc.add(1, VANILLA)
    st = sc.start()
    attack(st, attacker, PLAYER_TARGET)
    settle(st)
    assert pending(st) is DecisionKind.BURST
    yes(st)
    settle(st)
    assert zone_of(st, corin) is Zone.HAND


# ---------------------------------------------------------------------------------------------
# Commands GD06-101..GD06-121


@pytest.mark.card("GD06-101")
def test_gd06_101_returns_an_enemy_and_draws_with_an_alphacide_link_unit() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    cmd = sc.add(0, "GD06-101", Zone.HAND)
    sc.add(0, "GD06-017", pilot="GD06-081")
    enemy = sc.add(1, BIG)
    st = sc.start()
    before = hand_count(st)
    play(st, cmd)
    select_if_asked(st, enemy)
    assert zone_of(st, enemy) is Zone.HAND
    assert hand_count(st) == before - 1 + 1


@pytest.mark.card("GD06-101")
@pytest.mark.ruling("GD06-101:Q538")
def test_gd06_101_cannot_be_played_without_a_target() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    cmd = sc.add(0, "GD06-101", Zone.HAND)
    sc.add(0, "GD06-017", pilot="GD06-081")
    sc.add(1, "GD06-019")
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, cmd)


@pytest.mark.card("GD06-102")
def test_gd06_102_draws_two_on_a_quiet_opponents_turn() -> None:
    sc = Scenario(active=1)
    sc.resources(0, 3)
    cmd = sc.add(0, "GD06-102", Zone.HAND)
    st = sc.start(Step.END_ACTION)
    play_action(st, cmd)
    assert st.active == 0  # the turn ended; the next turn's draw step drew 1 more
    assert hand_count(st) == 2 + 1


@pytest.mark.card("GD06-102")
def test_gd06_102_draws_nothing_after_an_enemy_attack() -> None:
    sc = Scenario(active=1)
    sc.resources(0, 3)
    cmd = sc.add(0, "GD06-102", Zone.HAND)
    sc.shields(0, VANILLA)
    attacker = sc.add(1, VANILLA)
    st = sc.start()
    attack(st, attacker, PLAYER_TARGET)
    pass_until(st, 0)
    play_action(st, cmd)
    assert hand_count(st) == 0
    settle(st)


def play_action(st: GameState, uid: int) -> None:
    for o in options(st):
        if o.kind is A.PLAY_COMMAND and o.a == uid:
            apply(st, o)
            return
    raise AssertionError("command not playable")


@pytest.mark.card("GD06-103")
def test_gd06_103_damages_a_rested_enemy() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    cmd = sc.add(0, "GD06-103", Zone.HAND)
    rested = sc.add(1, BIG, rested=True)
    sc.add(1, BIG)
    st = sc.start()
    play(st, cmd)
    select_if_asked(st, rested)
    assert st.cards[rested].damage == 1


@pytest.mark.card("GD06-104")
def test_gd06_104_returns_an_enemy_with_3_or_less_hp() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    cmd = sc.add(0, "GD06-104", Zone.HAND)
    small = sc.add(1, VANILLA)
    sc.add(1, BIG)
    st = sc.start()
    play(st, cmd)
    select_if_asked(st, small)
    assert zone_of(st, small) is Zone.HAND


@pytest.mark.card("GD06-105", "T-035", "T-036")
@pytest.mark.parametrize("mode", [0, 1])
def test_gd06_105_deploys_calibarn_or_aerial_rebuild(mode: int) -> None:
    sc = Scenario()
    sc.resources(0, 5)
    cmd = sc.add(0, "GD06-105", Zone.HAND)
    st = sc.start()
    play(st, cmd)
    select(st, mode)
    (token,) = st.zones[0][Zone.BATTLE]
    number = V.cdef(st, token).card_number
    if mode == 0:
        assert number == "T-035" and "High-Maneuver" in keywords(st, token)
    else:
        assert number == "T-036" and keywords(st, token)["Breach"] == 3


@pytest.mark.card("T-036")
def test_t_036_aerial_rebuild_breach_3_destroys_the_first_shield_after_a_battle_kill() -> None:
    sc = Scenario()
    aerial = sc.add(0, "T-036")
    first, second = sc.shields(1, VANILLA, VANILLA)
    victim = sc.add(1, VANILLA, rested=True)
    st = sc.start()
    attack(st, aerial, victim)
    settle(st)
    assert zone_of(st, victim) is Zone.TRASH
    assert zone_of(st, first) is Zone.TRASH  # <Breach 3> on the first card of the shield area
    assert zone_of(st, second) is Zone.SHIELD


@pytest.mark.card("GD06-106")
@pytest.mark.ruling("GD06-106:Q539")
def test_gd06_106_shield_area_ignores_damage_from_small_enemy_units() -> None:
    sc = Scenario(active=1)
    sc.resources(0, 4)
    cmd = sc.add(0, "GD06-106", Zone.HAND)
    sc.shields(0, VANILLA)
    base = sc.base(0)
    attacker = sc.add(1, "GD06-029")
    victim = sc.add(0, VANILLA, rested=True)
    st = sc.start()
    attack(st, attacker, victim)
    pass_until(st, 0)
    play_action(st, cmd)
    settle(st)
    assert zone_of(st, victim) is Zone.TRASH
    assert st.cards[base].damage == 0
    assert len(st.zones[0][Zone.SHIELD]) == 1


@pytest.mark.card("GD06-107")
def test_gd06_107_destroys_a_small_blocker_with_a_fleet_unit() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    cmd = sc.add(0, "GD06-107", Zone.HAND)
    sc.add(0, FLEET)
    blocker = sc.add(1, BLOCKER)
    st = sc.start()
    play(st, cmd)
    select_if_asked(st, blocker)
    assert zone_of(st, blocker) is Zone.TRASH


@pytest.mark.card("GD06-108")
def test_gd06_108_low_level_neo_zeon_units_get_ap_plus_2() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    cmd = sc.add(0, "GD06-108", Zone.HAND)
    small = sc.add(0, NEO_ZEON)
    big = sc.add(0, "GD06-019")
    st = sc.start()
    play(st, cmd)
    assert ap(st, small) == 4
    assert ap(st, big) == 5


@pytest.mark.card("GD06-109", "T-031", "T-032")
@pytest.mark.parametrize("mode", [0, 1])
def test_gd06_109_deploys_a_rested_turn_x_part(mode: int) -> None:
    sc = Scenario()
    sc.resources(0, 4)
    cmd = sc.add(0, "GD06-109", Zone.HAND)
    st = sc.start()
    play(st, cmd)
    select(st, mode)
    (token,) = st.zones[0][Zone.BATTLE]
    assert st.cards[token].rested
    assert V.cdef(st, token).card_number == ("T-031" if mode == 0 else "T-032")


@pytest.mark.card("GD06-110")
def test_gd06_110_boosts_for_the_battle_and_draws_with_a_small_hand() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    cmd = sc.add(0, "GD06-110", Zone.HAND)
    unit = sc.add(0, VANILLA)
    sc.add(0, DOM_TROPEN)
    enemy = sc.add(1, BIG, rested=True)
    st = sc.start()
    attack(st, unit, enemy)
    pass_until(st, 0)
    play_action(st, cmd)
    select_if_asked(st, unit)
    settle(st)
    assert zone_of(st, enemy) is Zone.TRASH  # AP 2+2 against HP 4
    assert hand_count(st) == 1


@pytest.mark.card("GD06-111")
def test_gd06_111_deals_2_with_four_gjallarhorn_cards_in_trash() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    cmd = sc.add(0, "GD06-111", Zone.HAND)
    sc.trash(0, GRAZE, GRAZE, GRAZE, GRAZE)
    unit = sc.add(0, VANILLA)
    enemy = sc.add(1, BIG)
    st = sc.start()
    attack(st, unit)
    pass_until(st, 0)
    play_action(st, cmd)
    select_if_asked(st, enemy)
    assert st.cards[enemy].damage == 2
    settle(st)


@pytest.mark.card("GD06-112")
def test_gd06_112_damages_the_enemy_battling_an_aeug_unit() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    cmd = sc.add(0, "GD06-112", Zone.HAND)
    nemo = sc.add(0, NEMO)
    enemy = sc.add(1, BIG, rested=True)
    sc.add(1, BIG)
    st = sc.start()
    attack(st, nemo, enemy)
    pass_until(st, 0)
    play_action(st, cmd)
    settle(st)
    assert zone_of(st, enemy) is Zone.TRASH  # 2 effect damage + 2 battle damage, HP 4


@pytest.mark.card("GD06-112")
def test_gd06_112_unplayable_when_no_aeug_unit_battles() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    cmd = sc.add(0, "GD06-112", Zone.HAND)
    unit = sc.add(0, VANILLA)
    enemy = sc.add(1, BIG, rested=True)
    st = sc.start()
    attack(st, unit, enemy)
    pass_until(st, 0)
    assert not has_action(st, A.PLAY_COMMAND, cmd)
    settle(st)


@pytest.mark.card("GD06-113")
def test_gd06_113_deals_5_to_a_damaged_enemy() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    cmd = sc.add(0, "GD06-113", Zone.HAND)
    hurt = sc.add(1, "GD06-019", damage=1)
    sc.add(1, BIG)
    st = sc.start()
    play(st, cmd)
    select_if_asked(st, hurt)
    assert zone_of(st, hurt) is Zone.TRASH  # 1 + 5 damage, HP 6


@pytest.mark.card("GD06-114")
@pytest.mark.ruling("GD06-114:Q540", "GD06-114:Q541")
def test_gd06_114_modes_need_their_targets() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    cmd = sc.add(0, "GD06-114", Zone.HAND)
    unit = sc.add(0, VANILLA)
    enemy = sc.add(1, BIG, rested=True)
    st = sc.start()
    attack(st, unit, enemy)
    pass_until(st, 0)
    assert not has_action(st, A.PLAY_COMMAND, cmd)
    settle(st)


@pytest.mark.card("GD06-114")
def test_gd06_114_reduces_battle_damage_to_an_alphacide_unit() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    cmd = sc.add(0, "GD06-114", Zone.HAND)
    alpha = sc.add(0, "GD06-017")
    enemy = sc.add(1, HUGE, rested=True)
    st = sc.start()
    attack(st, alpha, enemy)
    pass_until(st, 0)
    play_action(st, cmd)
    select_if_asked(st, 0)
    select_if_asked(st, alpha)
    settle(st)
    assert st.cards[alpha].damage == 5 - 3


@pytest.mark.card("GD06-114")
def test_gd06_114_destroys_a_lv2_or_lower_enemy() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    cmd = sc.add(0, "GD06-114", Zone.HAND)
    unit = sc.add(0, BIG)
    small = sc.add(1, VANILLA, rested=True)
    sc.shields(1, VANILLA, VANILLA)
    st = sc.start()
    attack(st, unit, PLAYER_TARGET)
    pass_until(st, 0)
    play_action(st, cmd)
    assert select_options(st) == {1}  # no friendly (Alphacide) Unit: only the second mode (Q540)
    select(st, 1)
    settle(st)
    assert zone_of(st, small) is Zone.TRASH


@pytest.mark.card("GD06-115", "T-033", "T-034")
@pytest.mark.ruling("GD06-115:Q542")
def test_gd06_115_both_players_get_a_token() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    cmd = sc.add(0, "GD06-115", Zone.HAND)
    victim = sc.add(1, BIG)
    st = sc.start()
    play(st, cmd)
    select_if_asked(st, victim)
    assert zone_of(st, victim) is Zone.TRASH
    assert names_in(st, 0, Zone.BATTLE) == ["Sazabi"]
    assert names_in(st, 1, Zone.BATTLE) == ["Nu Gundam"]
    (nu,) = st.zones[1][Zone.BATTLE]
    assert "First Strike" in keywords(st, nu)


@pytest.mark.card("GD06-116")
@pytest.mark.ruling("GD06-116:Q543", "GD06-116:Q544")
def test_gd06_116_deploys_a_purple_zeon_unit_from_trash_for_free() -> None:
    sc = Scenario()
    res = sc.resources(0, 5)
    cmd = sc.add(0, "GD06-116", Zone.HAND)
    gp02 = sc.add(0, "GD06-052", Zone.TRASH)
    sc.deck(0, VANILLA)
    enemy = sc.add(1, BIG)
    st = sc.start()
    play(st, cmd)
    select_if_asked(st, gp02)
    select_if_asked(st, enemy)
    assert zone_of(st, gp02) is Zone.BATTLE
    assert sum(st.cards[r].rested for r in res) == 5
    assert st.cards[enemy].rested


@pytest.mark.card("GD06-117")
@pytest.mark.ruling("GD06-117:Q545")
def test_gd06_117_may_choose_a_unit_already_battling() -> None:
    sc = Scenario(active=1)
    sc.resources(0, 3)
    cmd = sc.add(0, "GD06-117", Zone.HAND)
    target = sc.add(0, DOM_TROPEN, rested=True)
    attacker = sc.add(1, VANILLA)
    st = sc.start()
    attack(st, attacker, target)
    pass_until(st, 0)
    play_action(st, cmd)
    select_if_asked(st, target)
    settle(st)
    assert zone_of(st, attacker) is Zone.TRASH


@pytest.mark.card("GD06-117")
def test_gd06_117_changes_the_attack_target() -> None:
    sc = Scenario(active=1)
    sc.resources(0, 3)
    cmd = sc.add(0, "GD06-117", Zone.HAND)
    sc.shields(0, VANILLA)
    target = sc.add(0, DOM_TROPEN, rested=True)
    attacker = sc.add(1, VANILLA)
    st = sc.start()
    attack(st, attacker, PLAYER_TARGET)
    pass_until(st, 0)
    play_action(st, cmd)
    select_if_asked(st, target)
    settle(st)
    assert len(st.zones[0][Zone.SHIELD]) == 1
    assert zone_of(st, attacker) is Zone.TRASH


@pytest.mark.card("GD06-117")
def test_gd06_117_does_not_retarget_your_own_attack() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    cmd = sc.add(0, "GD06-117", Zone.HAND)
    sc.add(0, DOM_TROPEN, rested=True)
    attacker = sc.add(0, VANILLA)
    sc.shields(1, VANILLA, VANILLA)
    st = sc.start()
    attack(st, attacker, PLAYER_TARGET)
    pass_until(st, 0)
    play_action(st, cmd)
    settle(st)
    assert len(st.zones[1][Zone.SHIELD]) == 1  # the attack still hit the player


@pytest.mark.card("GD06-118")
def test_gd06_118_weakens_up_to_the_number_of_white_blockers() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    cmd = sc.add(0, "GD06-118", Zone.HAND)
    sc.add(0, "GD06-077")
    sc.add(0, "GD06-064")
    a = sc.add(1, BIG)
    b = sc.add(1, BIG)
    c = sc.add(1, BIG)
    st = sc.start()
    play(st, cmd)
    select(st, a, b)
    assert (ap(st, a), ap(st, b), ap(st, c)) == (1, 1, 3)


@pytest.mark.card("GD06-119")
@pytest.mark.ruling("GD06-119:Q546")
def test_gd06_119_rests_an_enemy_then_pairs_from_trash_with_an_mf_unit() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    cmd = sc.add(0, "GD06-119", Zone.HAND)
    mf = sc.add(0, "GD06-066")
    attacker = sc.add(0, VANILLA)
    small = sc.add(1, VANILLA)
    st = sc.start()
    attack(st, attacker)
    pass_until(st, 0)
    play_action(st, cmd)
    select_if_asked(st, small)
    for _ in range(3):
        if pending(st) is DecisionKind.YES_NO:
            yes(st)
        select_if_asked(st, mf)
    assert st.cards[small].rested
    assert st.cards[mf].pair == cmd
    settle(st)


@pytest.mark.card("GD06-120")
def test_gd06_120_rests_any_enemy() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    cmd = sc.add(0, "GD06-120", Zone.HAND)
    enemy = sc.add(1, "GD06-019")
    st = sc.start()
    play(st, cmd)
    select_if_asked(st, enemy)
    assert st.cards[enemy].rested


@pytest.mark.card("GD06-121")
@pytest.mark.parametrize("high_maneuver", [False, True])
def test_gd06_121_ap_minus_2_or_minus_6_against_high_maneuver(high_maneuver: bool) -> None:
    sc = Scenario()
    sc.resources(0, 3)
    cmd = sc.add(0, "GD06-121", Zone.HAND)
    unit = sc.add(0, VANILLA)
    enemy = sc.add(1, "GD06-034" if high_maneuver else "GD06-035")
    st = sc.start()
    attack(st, unit)
    pass_until(st, 0)
    play_action(st, cmd)
    select_if_asked(st, enemy)
    assert ap(st, enemy) == (0 if high_maneuver else 2)
    settle(st)


# ---------------------------------------------------------------------------------------------
# Bases GD06-122..GD06-130


@pytest.mark.card("GD06-122")
def test_gd06_122_deploy_takes_a_shield_and_returns_a_small_enemy() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    base = sc.add(0, "GD06-122", Zone.HAND)
    (shield,) = sc.shields(0, VANILLA)
    small = sc.add(1, VANILLA)
    st = sc.start()
    play(st, base)
    select_if_asked(st, small)
    assert zone_of(st, shield) is Zone.HAND
    assert zone_of(st, small) is Zone.HAND


@pytest.mark.card("GD06-123")
def test_gd06_123_destroys_a_small_blocker_with_a_low_level_neo_zeon_pilot() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    base = sc.add(0, "GD06-123", Zone.HAND)
    sc.shields(0, VANILLA)
    sc.add(0, NEO_ZEON, pilot="GD06-084")
    blocker = sc.add(1, BLOCKER)
    st = sc.start()
    play(st, base)
    select_if_asked(st, blocker)
    assert zone_of(st, blocker) is Zone.TRASH


@pytest.mark.card("GD06-123")
def test_gd06_123_needs_a_low_level_pilot() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    base = sc.add(0, "GD06-123", Zone.HAND)
    sc.shields(0, VANILLA)
    sc.add(0, NEO_ZEON, pilot="GD06-080")
    blocker = sc.add(1, BLOCKER)
    st = sc.start()
    play(st, base)
    assert zone_of(st, blocker) is Zone.BATTLE


@pytest.mark.card("GD06-124")
def test_gd06_124_rest_to_boost_fleet_units_with_three_in_play() -> None:
    sc = Scenario()
    aspite = sc.base(0, "GD06-124")
    fleet = [sc.add(0, FLEET), sc.add(0, FLEET), sc.add(0, "T-031")]
    st = sc.start()
    activate(st, aspite)
    assert [ap(st, u) for u in fleet] == [4, 4, 3]
    assert st.cards[aspite].rested


@pytest.mark.card("GD06-125")
@pytest.mark.ruling("GD06-125:Q547")
def test_gd06_125_battle_damage_with_a_rested_aeug_unit_strikes_back_once() -> None:
    sc = Scenario(active=1)
    argama = sc.base(0, "GD06-125")
    sc.add(0, NEMO, rested=True)
    a = sc.add(1, VANILLA)
    b = sc.add(1, VANILLA)
    st = sc.start()
    battle(st, a, PLAYER_TARGET)
    select_if_asked(st, a)
    assert st.cards[a].damage == 1
    battle(st, b, PLAYER_TARGET)
    assert st.cards[b].damage == 0
    assert st.cards[argama].damage == 4


@pytest.mark.card("GD06-125")
@pytest.mark.ruling("GD06-125:Q547")
def test_gd06_125_breach_damage_does_not_trigger() -> None:
    sc = Scenario(active=1)
    argama = sc.base(0, "GD06-125")
    sc.add(0, NEMO, rested=True)
    victim = sc.add(0, VANILLA, rested=True)
    attacker = sc.add(1, "GD06-029")
    st = sc.start()
    battle(st, attacker, victim)
    assert st.cards[argama].damage == 3
    assert st.cards[attacker].damage == 2  # only the battle damage; no Argama response


@pytest.mark.card("GD06-126")
def test_gd06_126_damages_a_damaged_enemy_with_a_gjallarhorn_unit() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    base = sc.add(0, "GD06-126", Zone.HAND)
    sc.shields(0, VANILLA)
    sc.add(0, GRAZE)
    hurt = sc.add(1, BIG, damage=1)
    st = sc.start()
    play(st, base)
    select_if_asked(st, hurt)
    assert st.cards[hurt].damage == 2


@pytest.mark.card("GD06-127")
def test_gd06_127_on_your_turn_mills_two_and_destroys_with_an_alphacide() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    base = sc.add(0, "GD06-127", Zone.HAND)
    sc.shields(0, VANILLA)
    sc.deck(0, BIG, ALPHACIDE)
    small = sc.add(1, VANILLA)
    st = sc.start()
    play(st, base)
    select_if_asked(st, small)
    assert zone_of(st, small) is Zone.TRASH
    assert len(st.zones[0][Zone.TRASH]) == 2


@pytest.mark.card("GD06-127")
def test_gd06_127_burst_on_the_opponents_turn_only_takes_a_shield() -> None:
    sc = Scenario(active=1)
    (yomagntho,) = sc.shields(0, "GD06-127")
    sc.shields(0, VANILLA)
    attacker = sc.add(1, VANILLA)
    st = sc.start()
    attack(st, attacker, PLAYER_TARGET)
    settle(st)
    assert pending(st) is DecisionKind.BURST
    yes(st)
    settle(st)
    assert zone_of(st, yomagntho) is Zone.BASE
    assert len(st.zones[0][Zone.TRASH]) == 0


@pytest.mark.card("GD06-128")
@pytest.mark.ruling("GD06-128:Q548")
def test_gd06_128_rest_to_reduce_battle_damage_to_a_zeon_unit() -> None:
    sc = Scenario()
    gwaden = sc.base(0, "GD06-128")
    zeon = sc.add(0, DOM_TROPEN)
    enemy = sc.add(1, BIG, rested=True)
    st = sc.start()
    activate(st, gwaden)
    select_if_asked(st, zeon)
    battle(st, zeon, enemy)
    assert st.cards[zeon].damage == 3 - 1


@pytest.mark.card("GD06-129", "T-037")
def test_gd06_129_destroyed_deploys_the_axis_base_token() -> None:
    sc = Scenario(active=1)
    luna = sc.base(0, "GD06-129")
    sc.shields(0, VANILLA)
    attacker = sc.add(1, BIG)
    st = sc.start()
    battle(st, attacker, PLAYER_TARGET)
    assert zone_of(st, luna) is Zone.TRASH
    (axis,) = st.zones[0][Zone.BASE]
    cd = V.cdef(st, axis)
    assert cd.card_number == "T-037" and cd.card_type is CardType.BASE_TOKEN
    assert V.hp_of(st, V.derived(st), axis) == 5
    assert V.level_of(st, axis) == 0 and V.cost_of(st, axis) == 0


@pytest.mark.card("GD06-129", "T-037")
def test_gd06_129_axis_token_is_removed_when_destroyed() -> None:
    sc = Scenario(active=1)
    sc.base(0, "GD06-129")
    sc.shields(0, VANILLA)
    a = sc.add(1, BIG)
    b = sc.add(1, HUGE)
    st = sc.start()
    battle(st, a, PLAYER_TARGET)
    (axis,) = st.zones[0][Zone.BASE]
    battle(st, b, PLAYER_TARGET)
    assert zone_of(st, axis) is Zone.OUTSIDE  # a destroyed token leaves the game


@pytest.mark.card("GD06-129")
def test_gd06_129_no_axis_without_shields() -> None:
    sc = Scenario(active=1)
    sc.base(0, "GD06-129")
    attacker = sc.add(1, BIG)
    st = sc.start()
    battle(st, attacker, PLAYER_TARGET)
    assert st.zones[0][Zone.BASE] == []


@pytest.mark.card("T-037")
def test_t_037_axis_is_a_token_and_not_a_deck_card() -> None:
    db = V.reg().db
    cd = db["T-037"]
    assert cd.is_token and cd.card_type.is_base
    deck = Deck(name="axis", main=("T-037",) * 4, resources=(), entries=(), source=None)
    (violation,) = check_card_types(db, deck)
    assert violation.cards == ("T-037",)
    assert "token" in violation.message


@pytest.mark.card("GD06-130")
def test_gd06_130_reduces_an_unpaired_enemys_ap_by_dianna_counter_count() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    soleil = sc.add(0, "GD06-130", Zone.HAND)
    sc.shields(0, VANILLA)
    sc.add(0, FLAT)
    sc.add(0, MUTTOWOOO)
    enemy = sc.add(1, HUGE)
    paired = sc.add(1, BIG, pilot=RIDDHE)
    st = sc.start()
    play(st, soleil)
    select_if_asked(st, enemy)
    assert ap(st, enemy) == 5 - 2
    assert ap(st, paired) == 4
