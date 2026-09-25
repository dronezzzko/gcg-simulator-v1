"""Behaviour and ruling tests for GD05-071..GD05-130 (work package WP-GD05-B)."""

from __future__ import annotations

import pytest

from gcg_sim.effects import dsl as d
from gcg_sim.engine import view as V
from gcg_sim.engine.interp import add_lasting
from gcg_sim.engine.state import Action, GameState
from gcg_sim.engine.types import PLAYER_TARGET, ActionKind, DecisionKind, Duration, Step, Zone
from gcg_sim.testkit import (
    Scenario,
    act,
    activate,
    ap,
    attack,
    block,
    choose_option,
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
VANILLA = "GD01-060"  # Zaku Mariner, Lv2 2/2, Zeon
BLOCKER_3_4 = "GD01-072"  # Launcher Strike Gundam, Lv4 3/4 <Blocker>
VANILLA_4_5 = "GD04-032"  # Xavier's Gyan Hakuji-Packs (GQ), Lv5 4/5
VANILLA_6_4 = "ST14-008"  # Gundam Heavyarms Custom (EW), Lv6 6/4
NEO_ZEON_2_3 = "GD01-057"  # Dreissen (Sleeves), Neo Zeon Lv2 2/3
NEO_ZEON_4_1 = "GD02-048"  # Zaku III (Sleeves), Neo Zeon Lv3 4/1
NEO_ZEON_3_4 = "GD05-056"  # Rezin's Geara Doga, Neo Zeon Lv4 3/4
ACADEMY_2_2 = "GD01-085"  # Demi Garrison, Academy Lv2 2/2
LEO = "GD05-077"  # G Team Lv2 2/2
LM_TOKEN = "T-021"  # [Parts] League Militaire Unit token 1/1
# Warped Intent: a target-free 【Main】/【Action】 Command that keeps an action step open
ANCHOR = "GD03-112"


def attack_targets(st: GameState, attacker: int) -> set[int]:
    return {o.b for o in options(st) if o.kind is A.ATTACK and o.a == attacker}


def rested_resources(st: GameState, player: int) -> int:
    return sum(1 for u in st.zones[player][Zone.RESOURCE_AREA] if st.cards[u].rested)


def linked(st: GameState, uid: int) -> bool:
    return V.is_linked(V.derived(st), uid)


def hand_size(st: GameState, player: int) -> int:
    return len(st.zones[player][Zone.HAND])


def with_anchor(sc: Scenario, player: int = 0) -> int:
    """Give ``player`` a playable 【Action】 card so the engine stops in the action step."""
    sc.resources(player, 4)
    return sc.add(player, ANCHOR, Zone.HAND)


def pending(st: GameState) -> DecisionKind | None:
    return st.pending.kind if st.pending is not None else None


# ---------------------------------------------------------------------------------------------
# Bursts shared by many cards (rule 13-2-5)


@pytest.mark.card(
    "GD05-081",
    "GD05-082",
    "GD05-083",
    "GD05-084",
    "GD05-085",
    "GD05-086",
    "GD05-087",
    "GD05-088",
    "GD05-089",
    "GD05-090",
    "GD05-091",
    "GD05-092",
    "GD05-093",
    "GD05-094",
    "GD05-095",
    "GD05-096",
    "GD05-097",
    "GD05-098",
    "GD05-099",
    "GD05-100",
    "GD05-101",
    "GD05-120",
)
@pytest.mark.rule("13-2-5-1")
@pytest.mark.parametrize(
    "number",
    [f"GD05-{n:03d}" for n in range(81, 102)] + ["GD05-120"],
)
def test_burst_adds_this_card_to_hand(number: str) -> None:
    sc = Scenario()
    attacker = sc.add(0, VANILLA)
    (shield,) = sc.shields(1, number)
    st = sc.start()
    attack(st, attacker)
    assert pending(st) is DecisionKind.BURST
    yes(st)
    assert zone_of(st, shield) is Zone.HAND
    assert st.cards[shield].owner == 1


@pytest.mark.card(
    "GD05-123", "GD05-124", "GD05-125", "GD05-126", "GD05-127", "GD05-128", "GD05-129", "GD05-130"
)
@pytest.mark.rule("13-2-5-1")
@pytest.mark.parametrize("number", [f"GD05-{n:03d}" for n in range(123, 131)])
def test_base_burst_deploys_and_deploy_adds_a_shield(number: str) -> None:
    sc = Scenario()
    attacker = sc.add(0, VANILLA)
    base, next_shield = sc.shields(1, number, VANILLA)
    st = sc.start()
    attack(st, attacker)
    yes(st)
    assert zone_of(st, base) is Zone.BASE
    assert zone_of(st, next_shield) is Zone.HAND


# ---------------------------------------------------------------------------------------------
# GD05-071 Gundam Sandrock Custom (EW)


@pytest.mark.card("GD05-071")
@pytest.mark.parametrize("ally", [LEO, "GD05-074"])  # (G Team), (Preventer)
def test_gd05_071_attack_gives_enemy_ap_minus_2_with_team_ally(ally: str) -> None:
    sc = Scenario()
    sandrock = sc.add(0, "GD05-071")
    sc.add(0, ally)
    enemy = sc.add(1, VANILLA)
    sc.shields(1, VANILLA)
    st = sc.start()
    attack(st, sandrock)
    assert ap(st, enemy) == 0


@pytest.mark.card("GD05-071")
def test_gd05_071_no_effect_without_another_team_unit() -> None:
    sc = Scenario()
    sandrock = sc.add(0, "GD05-071")
    sc.add(0, VANILLA)
    enemy = sc.add(1, VANILLA)
    sc.shields(1, VANILLA)
    st = sc.start()
    attack(st, sandrock)
    assert ap(st, enemy) == 2


# ---------------------------------------------------------------------------------------------
# GD05-072 Rising Gundam


@pytest.mark.card("GD05-072")
def test_gd05_072_when_linked_rests_enemy_with_current_hp_4_or_less() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    rising = sc.add(0, "GD05-072")
    hammer = sc.add(0, "GD05-122", Zone.HAND)  # (Gundam Fighter) Command-Pilot
    weakened = sc.add(1, VANILLA_4_5, damage=1)  # current HP 4
    healthy = sc.add(1, VANILLA_4_5)  # current HP 5
    st = sc.start()
    play(st, hammer, onto=rising)
    assert linked(st, rising)
    assert st.cards[weakened].rested
    assert not st.cards[healthy].rested


@pytest.mark.card("GD05-072")
def test_gd05_072_no_rest_when_paired_without_link() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    rising = sc.add(0, "GD05-072")
    pilot = sc.add(0, "GD05-091", Zone.HAND)
    enemy = sc.add(1, VANILLA)
    st = sc.start()
    play(st, pilot, onto=rising)
    assert not linked(st, rising)
    assert not st.cards[enemy].rested


# ---------------------------------------------------------------------------------------------
# GD05-073 Altron Gundam (EW)


@pytest.mark.card("GD05-073")
@pytest.mark.rule("7-2-3-1")
def test_gd05_073_chosen_unit_is_not_set_active_in_opponents_start_phase() -> None:
    sc = Scenario()
    sc.resources(0, 7)
    altron = sc.add(0, "GD05-073", Zone.HAND)
    frozen = sc.add(1, VANILLA, rested=True)
    other = sc.add(1, VANILLA, rested=True)
    st = sc.start()
    play(st, altron)
    select(st, frozen)
    to_next_turn(st)
    assert st.active == 1
    assert st.cards[frozen].rested
    assert not st.cards[other].rested
    to_next_turn(st)
    to_next_turn(st)
    assert st.active == 1
    assert not st.cards[frozen].rested


@pytest.mark.card("GD05-073")
@pytest.mark.ruling("GD05-073:Q381")
def test_gd05_073_burst_effect_can_set_the_unit_active_during_your_turn() -> None:
    sc = Scenario()
    sc.resources(0, 7)
    altron = sc.add(0, "GD05-073", Zone.HAND)
    attacker = sc.add(0, VANILLA)
    frozen = sc.add(1, BLOCKER_3_4, rested=True)
    # Midair Modifications: 【Burst】activates its 【Main】, which sets a rested Blocker active
    sc.shields(1, "GD01-121")
    st = sc.start()
    play(st, altron)
    attack(st, attacker)
    assert pending(st) is DecisionKind.BURST
    yes(st)
    assert not st.cards[frozen].rested


@pytest.mark.card("GD05-073")
@pytest.mark.ruling("GD05-073:Q381")
def test_gd05_073_effects_can_set_the_unit_active_after_the_start_phase() -> None:
    sc = Scenario()
    sc.resources(0, 7)
    sc.resources(1, 3)
    altron = sc.add(0, "GD05-073", Zone.HAND)
    frozen = sc.add(1, BLOCKER_3_4, rested=True)
    midair = sc.add(1, "GD01-121", Zone.HAND)
    st = sc.start()
    play(st, altron)
    to_next_turn(st)
    assert st.active == 1
    assert st.cards[frozen].rested
    play(st, midair)
    assert not st.cards[frozen].rested


# ---------------------------------------------------------------------------------------------
# GD05-074 Noin's Taurus


@pytest.mark.card("GD05-074")
def test_gd05_074_destroyed_draws_then_discards() -> None:
    sc = Scenario()
    taurus = sc.add(0, "GD05-074")  # 3/1
    keep = sc.add(0, VANILLA, Zone.HAND)
    sc.deck(0, LEO)
    enemy = sc.add(1, VANILLA, rested=True)
    st = sc.start()
    top = st.zones[0][Zone.DECK][0]
    attack(st, taurus, enemy)
    assert zone_of(st, taurus) is Zone.TRASH
    assert pending(st) is DecisionKind.DISCARD
    select(st, keep)
    assert zone_of(st, top) is Zone.HAND
    assert zone_of(st, keep) is Zone.TRASH


# ---------------------------------------------------------------------------------------------
# GD05-075 Royal Gundam


@pytest.mark.card("GD05-075")
@pytest.mark.ruling("GD05-075:Q382")
@pytest.mark.rule("8-2-1")
def test_gd05_075_cannot_attack_the_player_base_or_shields() -> None:
    sc = Scenario()
    royal = sc.add(0, "GD05-075")
    enemy = sc.add(1, VANILLA, rested=True)
    sc.base(1)
    sc.shields(1, VANILLA)
    st = sc.start()
    assert attack_targets(st, royal) == {enemy}
    assert keywords(st, royal).get("Blocker") == 1


# ---------------------------------------------------------------------------------------------
# GD05-076 Bolt Gundam


@pytest.mark.card("GD05-076")
@pytest.mark.ruling("GD05-076:Q383", "GD05-076:Q384")
@pytest.mark.rule("3-4-6-3-1")
def test_gd05_076_linked_attack_activates_paired_command_free_and_it_stays_paired() -> None:
    sc = Scenario()
    sc.resources(0, 3, rested=3)
    bolt = sc.add(0, "GD05-076", pilot="GD05-122")  # Graviton Hammer as [Argo Gulskii]
    enemy = sc.add(1, VANILLA)
    sc.shields(1, VANILLA)
    st = sc.start()
    hammer = st.cards[bolt].pair
    assert linked(st, bolt)
    attack(st, bolt)
    assert st.cards[enemy].rested
    assert zone_of(st, hammer) is Zone.PAIRED
    assert st.cards[bolt].pair == hammer


@pytest.mark.card("GD05-076")
def test_gd05_076_unlinked_attack_does_not_activate() -> None:
    sc = Scenario()
    bolt = sc.add(0, "GD05-076", pilot="GD05-121")  # [Chibodee Crocket]: no link
    enemy = sc.add(1, VANILLA)
    sc.shields(1, VANILLA)
    st = sc.start()
    attack(st, bolt)
    assert ap(st, enemy) == 2
    assert not st.cards[enemy].rested


# ---------------------------------------------------------------------------------------------
# GD05-078 Gundam Deathscythe Hell (EW)


@pytest.mark.card("GD05-078")
@pytest.mark.rule("3-2-4")
def test_gd05_078_deploy_turn_may_attack_only_a_rested_enemy_unit() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    scythe = sc.add(0, "GD05-078", Zone.HAND)
    rested = sc.add(1, VANILLA, rested=True)
    sc.add(1, VANILLA)
    st = sc.start()
    play(st, scythe)
    assert attack_targets(st, scythe) == {rested}
    attack(st, scythe, rested)
    pass_all(st)
    assert zone_of(st, rested) is Zone.TRASH


@pytest.mark.card("GD05-078")
@pytest.mark.ruling("GD05-078:Q385")
def test_gd05_078_bridge_crew_does_not_extend_the_deploy_turn_attack() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    scythe = sc.add(0, "GD05-078", Zone.HAND)
    crew = sc.add(0, "GD03-105", Zone.HAND)
    rested = sc.add(1, VANILLA, rested=True)
    sc.add(1, VANILLA)  # active, no paired Pilot
    st = sc.start()
    play(st, scythe)
    play(st, crew)
    assert attack_targets(st, scythe) == {rested}


# ---------------------------------------------------------------------------------------------
# GD05-079 Gundam Heavyarms Custom (EW)


@pytest.mark.card("GD05-079")
def test_gd05_079_once_per_turn_reduces_lv4_or_lower_enemy_ap() -> None:
    sc = Scenario()
    heavyarms = sc.add(0, "GD05-079")
    sc.add(0, LEO)
    low = sc.add(1, VANILLA)
    high = sc.add(1, VANILLA_4_5)  # Lv5: not a legal choice
    st = sc.start()
    activate(st, heavyarms)
    assert ap(st, low) == 1
    assert ap(st, high) == 4
    assert not has_action(st, A.ACTIVATE, heavyarms)


@pytest.mark.card("GD05-079")
def test_gd05_079_no_effect_without_another_team_unit() -> None:
    sc = Scenario()
    heavyarms = sc.add(0, "GD05-079")
    low = sc.add(1, VANILLA)
    st = sc.start()
    activate(st, heavyarms)
    assert ap(st, low) == 2


# ---------------------------------------------------------------------------------------------
# GD05-081 Kira Yamato


@pytest.mark.card("GD05-081")
@pytest.mark.parametrize(("unit", "draws"), [("GD05-010", 1), ("GD01-077", 0)])
def test_gd05_081_when_linked_draws_only_for_orb_or_tsa_unit(unit: str, draws: int) -> None:
    sc = Scenario()
    sc.resources(0, 5)
    host = sc.add(0, unit)
    kira = sc.add(0, "GD05-081", Zone.HAND)
    st = sc.start()
    before = hand_size(st, 0)
    play(st, kira, onto=host)
    assert linked(st, host)
    assert hand_size(st, 0) == before - 1 + draws


# ---------------------------------------------------------------------------------------------
# GD05-082 Andrew Waldfeld


@pytest.mark.card("GD05-082")
@pytest.mark.rule("13-1-1-1")
def test_gd05_082_linked_unit_gains_repair_2() -> None:
    sc = Scenario()
    murasame = sc.add(0, "GD05-003", pilot="GD05-082", damage=3)
    unlinked = sc.add(0, VANILLA, pilot="GD05-082")
    st = sc.start()
    assert keywords(st, murasame).get("Repair") == 2
    assert "Repair" not in keywords(st, unlinked)
    to_next_turn(st)
    assert st.cards[murasame].damage == 1


# ---------------------------------------------------------------------------------------------
# GD05-083 Cagalli Yula Athha


@pytest.mark.card("GD05-083")
def test_gd05_083_when_paired_returns_enemy_with_current_hp_1() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    host = sc.add(0, VANILLA)
    cagalli = sc.add(0, "GD05-083", Zone.HAND)
    weakened = sc.add(1, VANILLA, damage=1)  # current HP 1
    healthy = sc.add(1, VANILLA)
    st = sc.start()
    play(st, cagalli, onto=host)
    assert zone_of(st, weakened) is Zone.HAND
    assert zone_of(st, healthy) is Zone.BATTLE


# ---------------------------------------------------------------------------------------------
# GD05-084 Odelo Henrik


def _odelo_scenario(*, odelo: bool, token_pilot: str | None = None) -> tuple[Scenario, int]:
    sc = Scenario(active=1)
    sc.add(0, VANILLA, pilot="GD05-084" if odelo else None)
    token = sc.add(0, LM_TOKEN, pilot=token_pilot)
    sc.add(1, VANILLA)
    sc.resources(1, 4)
    return sc, token


@pytest.mark.card("GD05-084")
def test_gd05_084_reduces_enemy_effect_damage_to_league_militaire_tokens() -> None:
    sc, token = _odelo_scenario(odelo=True)
    shield = sc.add(1, "GD05-117", Zone.HAND)  # 1 damage to 1 of your Units and 1 enemy Unit
    st = sc.start()
    play(st, shield)
    select(st, token)
    assert zone_of(st, token) is Zone.BATTLE
    assert st.cards[token].damage == 0


@pytest.mark.card("GD05-084")
def test_gd05_084_without_odelo_the_token_is_destroyed() -> None:
    sc, token = _odelo_scenario(odelo=False)
    shield = sc.add(1, "GD05-117", Zone.HAND)
    st = sc.start()
    play(st, shield)
    select(st, token)
    assert zone_of(st, token) is Zone.OUTSIDE


@pytest.mark.card("GD05-084")
def test_gd05_084_does_not_reduce_friendly_effect_damage() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    sc.add(0, VANILLA, pilot="GD05-084")
    token = sc.add(0, LM_TOKEN)
    enemy = sc.add(1, VANILLA)
    shield = sc.add(0, "GD05-117", Zone.HAND)
    st = sc.start()
    play(st, shield)
    select(st, token)
    assert zone_of(st, token) is Zone.OUTSIDE
    assert st.cards[enemy].damage == 1


@pytest.mark.card("GD05-084")
@pytest.mark.ruling("GD05-084:Q386")
def test_gd05_084_reduce_by_1_lowers_the_damage_received() -> None:
    sc, token = _odelo_scenario(odelo=True, token_pilot="GD05-085")  # token HP 1+2
    finger = sc.add(1, "GD05-110", Zone.HAND)  # 2 damage
    st = sc.start()
    play(st, finger)
    select(st, token)
    assert st.cards[token].damage == 1


# ---------------------------------------------------------------------------------------------
# GD05-085 Amuro Ray


@pytest.mark.card("GD05-085")
def test_gd05_085_recovers_2_after_destroying_enemy_in_battle() -> None:
    sc = Scenario()
    unit = sc.add(0, VANILLA, pilot="GD05-085")  # 4/4
    enemy = sc.add(1, VANILLA, rested=True)
    st = sc.start()
    attack(st, unit, enemy)
    pass_all(st)
    assert zone_of(st, enemy) is Zone.TRASH
    assert st.cards[unit].damage == 0


@pytest.mark.card("GD05-085")
def test_gd05_085_no_recovery_during_opponents_turn() -> None:
    sc = Scenario(active=1)
    unit = sc.add(0, VANILLA, pilot="GD05-085", rested=True)
    enemy = sc.add(1, VANILLA)
    st = sc.start()
    attack(st, enemy, unit)
    pass_all(st)
    assert zone_of(st, enemy) is Zone.TRASH
    assert st.cards[unit].damage == 2


@pytest.mark.card("GD05-085")
@pytest.mark.ruling("GD05-085:Q387")
def test_gd05_085_both_destroyed_the_unit_does_not_stay() -> None:
    sc = Scenario()
    unit = sc.add(0, VANILLA, pilot="GD05-085")  # 4/4
    enemy = sc.add(1, VANILLA_6_4, rested=True)  # 6/4
    st = sc.start()
    attack(st, unit, enemy)
    pass_all(st)
    assert zone_of(st, enemy) is Zone.TRASH
    assert zone_of(st, unit) is Zone.TRASH


# ---------------------------------------------------------------------------------------------
# GD05-086 Kayra Su


@pytest.mark.card("GD05-086")
@pytest.mark.rule("8-2-1")
def test_gd05_086_non_link_enemies_must_attack_the_rested_linked_unit() -> None:
    sc = Scenario(active=1)
    kayra = sc.add(0, "GD05-029", pilot="GD05-086", rested=True)
    sc.add(0, VANILLA, rested=True)
    attacker = sc.add(1, VANILLA)
    st = sc.start()
    assert linked(st, kayra)
    assert attack_targets(st, attacker) == {kayra}


@pytest.mark.card("GD05-086")
@pytest.mark.parametrize(("unit", "rested"), [(VANILLA, True), ("GD05-029", False)])
def test_gd05_086_no_attraction_unless_linked_and_rested(unit: str, rested: bool) -> None:
    sc = Scenario(active=1)
    host = sc.add(0, unit, pilot="GD05-086", rested=rested)
    other = sc.add(0, VANILLA, rested=True)
    attacker = sc.add(1, VANILLA)
    st = sc.start()
    targets = attack_targets(st, attacker)
    assert PLAYER_TARGET in targets
    assert other in targets
    assert (host in targets) == rested


@pytest.mark.card("GD05-086")
@pytest.mark.xfail(
    strict=True,
    reason="ENGINE: game._attack_options applies FORCE_ATTACK_TARGET to every attacker (ignores source_filters)",
)
def test_gd05_086_link_unit_attackers_are_not_attracted() -> None:
    sc = Scenario(active=1)
    kayra = sc.add(0, "GD05-029", pilot="GD05-086", rested=True)
    attacker = sc.add(1, "GD05-010", pilot="GD05-081")  # enemy Link Unit
    st = sc.start()
    assert linked(st, attacker)
    assert attack_targets(st, attacker) == {PLAYER_TARGET, kayra}


@pytest.mark.card("GD05-086")
@pytest.mark.ruling("GD05-086:Q388")
def test_gd05_086_several_attractors_attacker_chooses_one_of_them() -> None:
    sc = Scenario(active=1)
    first = sc.add(0, "GD05-029", pilot="GD05-086", rested=True)
    second = sc.add(0, "GD05-028", pilot="GD05-086", rested=True)
    sc.add(0, VANILLA, rested=True)
    attacker = sc.add(1, VANILLA)
    st = sc.start()
    assert attack_targets(st, attacker) == {first, second}


# ---------------------------------------------------------------------------------------------
# GD05-087 Lauda Neill


@pytest.mark.card("GD05-087")
@pytest.mark.parametrize(("unit", "expected"), [(ACADEMY_2_2, True), (VANILLA, False)])
def test_gd05_087_academy_unit_gains_high_maneuver(unit: str, expected: bool) -> None:
    sc = Scenario()
    host = sc.add(0, unit, pilot="GD05-087")
    st = sc.start()
    assert ("High-Maneuver" in keywords(st, host)) == expected


# ---------------------------------------------------------------------------------------------
# GD05-088 Prospera Mercury


@pytest.mark.card("GD05-088")
def test_gd05_088_this_unit_and_lfrith_gundnode_units_get_ap_plus_1() -> None:
    sc = Scenario()
    host = sc.add(0, VANILLA, pilot="GD05-088")  # 2+1 AP
    lfrith = sc.add(0, "GD01-086")  # Gundam Lfrith 2/4
    gundnode = sc.add(0, "T-026")  # Gundnode token 2/2
    other = sc.add(0, VANILLA)
    enemy_lfrith = sc.add(1, "GD01-086")
    st = sc.start()
    assert ap(st, host) == 4
    assert ap(st, lfrith) == 3
    assert ap(st, gundnode) == 3
    assert ap(st, other) == 2
    assert ap(st, enemy_lfrith) == 2


@pytest.mark.card("GD05-088")
def test_gd05_088_paired_lfrith_gets_the_bonus_once() -> None:
    sc = Scenario()
    host = sc.add(0, "GD01-086", pilot="GD05-088")  # 2+1 AP, itself a Lfrith
    st = sc.start()
    assert ap(st, host) == 4


# ---------------------------------------------------------------------------------------------
# GD05-089 Master Asia

HAOW = "GD05-036"  # Haow Gundam, (MF) Lv6 4/4, link [Master Asia]


def _master_asia_scenario() -> tuple[Scenario, int, int]:
    sc = Scenario()
    sc.resources(0, 4)
    haow = sc.add(0, HAOW, pilot="GD05-089")
    enemy = sc.add(1, NEO_ZEON_2_3)
    sc.shields(1, VANILLA, VANILLA)
    return sc, haow, enemy


@pytest.mark.card("GD05-089")
def test_gd05_089_linked_attack_deals_2_after_a_special_move_command() -> None:
    sc, haow, enemy = _master_asia_scenario()
    punch = sc.add(0, "GD05-121", Zone.HAND)  # (Special Move) 【Main】
    st = sc.start()
    assert linked(st, haow)
    play(st, punch)
    attack(st, haow)
    assert st.cards[enemy].damage == 2


@pytest.mark.card("GD05-089")
@pytest.mark.parametrize("command", [None, "GD05-111"])
def test_gd05_089_no_damage_without_a_special_move_command(command: str | None) -> None:
    sc, haow, enemy = _master_asia_scenario()
    if command is not None:
        sc.add(0, command, Zone.HAND)
        sc.add(0, VANILLA, Zone.HAND)
    st = sc.start()
    if command is not None:
        (cmd,) = [u for u in st.zones[0][Zone.HAND] if V.cdef(st, u).card_number == command]
        play(st, cmd)
    attack(st, haow)
    assert st.cards[enemy].damage == 0


def _master_asia_burst(sc: Scenario | None = None) -> tuple[GameState, int]:
    sc = sc or Scenario()
    attacker = sc.add(0, VANILLA)
    (asia,) = sc.shields(1, "GD05-089")
    sc.trash(1, "GD05-042", "GD05-043", "GD05-042")  # 3 (MF) cards
    st = sc.start()
    attack(st, attacker)
    yes(st)
    if pending(st) is DecisionKind.YES_NO:
        yes(st)
    return st, asia


@pytest.mark.card("GD05-089")
@pytest.mark.xfail(
    strict=True,
    reason="DSL: no step deploys a Pilot card as an (AP3･HP3) Unit (card-type override)",
)
def test_gd05_089_burst_may_deploy_it_as_ap3_hp3_unit() -> None:
    st, asia = _master_asia_burst()
    assert zone_of(st, asia) is Zone.BATTLE
    assert (ap(st, asia), hp(st, asia)) == (3, 3)


@pytest.mark.card("GD05-089")
@pytest.mark.ruling("GD05-089:Q389")
@pytest.mark.xfail(
    strict=True,
    reason="DSL: no step deploys a Pilot card as an (AP3･HP3) Unit (card-type override)",
)
def test_gd05_089_deployed_as_unit_is_lv6() -> None:
    st, asia = _master_asia_burst()
    assert zone_of(st, asia) is Zone.BATTLE
    assert V.stat(st, V.derived(st), asia, d.Stat.LV) == 6


@pytest.mark.card("GD05-089")
@pytest.mark.ruling("GD05-089:Q390")
@pytest.mark.xfail(
    strict=True,
    reason="DSL: no step deploys a Pilot card as an (AP3･HP3) Unit (card-type override)",
)
def test_gd05_089_deployed_as_unit_can_be_paired() -> None:
    sc = Scenario()
    sc.resources(1, 5)
    pilot = sc.add(1, "GD05-091", Zone.HAND)
    st, asia = _master_asia_burst(sc)
    assert zone_of(st, asia) is Zone.BATTLE
    to_next_turn(st)
    assert st.active == 1
    assert has_action(st, A.PAIR, pilot, asia)


@pytest.mark.card("GD05-089")
@pytest.mark.ruling("GD05-089:Q391")
def test_gd05_089_counts_main_activated_on_a_paired_special_move_command() -> None:
    sc, haow, enemy = _master_asia_scenario()
    dragon = sc.add(0, "GD05-035", pilot="GD05-112")  # activates paired [Sai Saici] 【Main】
    fodder = sc.add(1, VANILLA, rested=True)
    st = sc.start()
    attack(st, dragon, fodder)
    select(st, haow)  # Hoka Kyoten Juzetsujin: <Breach 3> to an (MF) Unit
    pass_all(st)
    assert zone_of(st, fodder) is Zone.TRASH
    assert st.cards[enemy].damage == 0
    attack(st, haow)
    assert st.cards[enemy].damage == 2


# ---------------------------------------------------------------------------------------------
# GD05-090 Stellar Loussier


@pytest.mark.card("GD05-090")
def test_gd05_090_destroyed_may_add_phantom_pain_top_card() -> None:
    sc = Scenario()
    unit = sc.add(0, VANILLA, pilot="GD05-090")  # 4/2
    enemy = sc.add(1, VANILLA, rested=True)
    sc.deck(0, "GD05-047")  # Exass (Phantom Pain)
    st = sc.start()
    top = st.zones[0][Zone.DECK][0]
    attack(st, unit, enemy)
    pass_all(st)
    assert zone_of(st, unit) is Zone.TRASH
    select(st, top)
    assert zone_of(st, top) is Zone.HAND


@pytest.mark.card("GD05-090")
def test_gd05_090_other_top_card_goes_to_the_bottom() -> None:
    sc = Scenario()
    unit = sc.add(0, VANILLA, pilot="GD05-090")
    enemy = sc.add(1, VANILLA, rested=True)
    sc.deck(0, LEO)
    st = sc.start()
    top = st.zones[0][Zone.DECK][0]
    attack(st, unit, enemy)
    pass_all(st)
    assert zone_of(st, top) is Zone.DECK
    assert st.zones[0][Zone.DECK][-1] == top


# ---------------------------------------------------------------------------------------------
# GD05-091 Sting Oakley


@pytest.mark.card("GD05-091")
@pytest.mark.parametrize(("trash", "stats"), [(7, (4, 4)), (6, (3, 3))])
def test_gd05_091_bonus_while_enemy_trash_has_7_cards(trash: int, stats: tuple[int, int]) -> None:
    sc = Scenario()
    unit = sc.add(0, VANILLA, pilot="GD05-091")
    sc.trash(1, *([VANILLA] * trash))
    st = sc.start()
    assert (ap(st, unit), hp(st, unit)) == stats


# ---------------------------------------------------------------------------------------------
# GD05-092 Auel Neider

ABYSS = "GD05-040"  # Abyss Gundam 2/4, link [Auel Neider]


@pytest.mark.card("GD05-092")
def test_gd05_092_linked_attack_on_player_gets_ap_plus_2_this_battle() -> None:
    sc = Scenario()
    abyss = sc.add(0, ABYSS, pilot="GD05-092")  # 3/5
    sc.shields(1, VANILLA)
    with_anchor(sc)
    st = sc.start()
    attack(st, abyss)
    assert pending(st) is DecisionKind.ACTION_STEP
    assert ap(st, abyss) == 5
    pass_all(st)
    assert ap(st, abyss) == 3


@pytest.mark.card("GD05-092")
@pytest.mark.parametrize("unit", [ABYSS, VANILLA])
def test_gd05_092_no_bonus_when_attacking_a_unit_or_unlinked(unit: str) -> None:
    sc = Scenario()
    host = sc.add(0, unit, pilot="GD05-092")
    enemy = sc.add(1, NEO_ZEON_2_3, rested=True)
    sc.shields(1, VANILLA)
    with_anchor(sc)
    st = sc.start()
    base_ap = ap(st, host)
    attack(st, host, enemy if unit == ABYSS else PLAYER_TARGET)
    assert pending(st) is DecisionKind.ACTION_STEP
    assert ap(st, host) == base_ap


@pytest.mark.card("GD05-092")
@pytest.mark.ruling("GD05-092:Q392")
def test_gd05_092_bonus_stays_after_being_blocked() -> None:
    sc = Scenario()
    abyss = sc.add(0, ABYSS, pilot="GD05-092")
    blocker = sc.add(1, BLOCKER_3_4)
    sc.shields(1, VANILLA)
    with_anchor(sc)
    st = sc.start()
    attack(st, abyss)
    block(st, blocker)
    assert pending(st) is DecisionKind.ACTION_STEP
    assert ap(st, abyss) == 5


# ---------------------------------------------------------------------------------------------
# GD05-093 Char Aznable


@pytest.mark.card("GD05-093")
@pytest.mark.ruling("GD05-093:Q393", "GD05-093:Q394")
def test_gd05_093_when_linked_deploys_neo_zeon_base_from_trash_for_free() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    white = sc.add(0, "GD02-032")  # link [Char Aznable]
    char = sc.add(0, "GD05-093", Zone.HAND)
    (palau,) = sc.trash(0, "GD01-128")  # (Neo Zeon) Base: 【Deploy】 add 1 Shield to hand
    (shield,) = sc.shields(0, VANILLA)
    st = sc.start()
    play(st, char, onto=white)
    select(st, palau)
    assert zone_of(st, palau) is Zone.BASE
    assert zone_of(st, shield) is Zone.HAND
    assert rested_resources(st, 0) == 1


@pytest.mark.card("GD05-093")
def test_gd05_093_no_deploy_without_link() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    host = sc.add(0, VANILLA)
    char = sc.add(0, "GD05-093", Zone.HAND)
    (palau,) = sc.trash(0, "GD01-128")
    st = sc.start()
    play(st, char, onto=host)
    assert pending(st) is DecisionKind.MAIN
    assert zone_of(st, palau) is Zone.TRASH


# ---------------------------------------------------------------------------------------------
# GD05-094 Quess Paraya


@pytest.mark.card("GD05-094")
@pytest.mark.ruling("GD05-094:Q395")
def test_gd05_094_destroyed_reduces_enemy_battle_damage_to_neo_zeon_unit() -> None:
    sc = Scenario()
    quess_unit = sc.add(0, VANILLA, pilot="GD05-094")  # 3/3
    dreissen = sc.add(0, NEO_ZEON_2_3)
    zaku3 = sc.add(1, NEO_ZEON_4_1, rested=True)
    geara = sc.add(1, NEO_ZEON_3_4, rested=True)
    st = sc.start()
    attack(st, quess_unit, zaku3)
    pass_all(st)
    assert zone_of(st, quess_unit) is Zone.TRASH
    attack(st, dreissen, geara)
    pass_all(st)
    assert zone_of(st, dreissen) is Zone.BATTLE
    assert st.cards[dreissen].damage == 1


@pytest.mark.card("GD05-094")
def test_gd05_094_without_quess_the_battle_damage_is_not_reduced() -> None:
    sc = Scenario()
    dreissen = sc.add(0, NEO_ZEON_2_3)
    geara = sc.add(1, NEO_ZEON_3_4, rested=True)
    st = sc.start()
    attack(st, dreissen, geara)
    pass_all(st)
    assert zone_of(st, dreissen) is Zone.TRASH


# ---------------------------------------------------------------------------------------------
# GD05-095 Gyunei Guss


@pytest.mark.card("GD05-095")
@pytest.mark.parametrize(("unit", "expected"), [(NEO_ZEON_2_3, True), (VANILLA, False)])
def test_gd05_095_neo_zeon_unit_gains_blocker(unit: str, expected: bool) -> None:
    sc = Scenario()
    host = sc.add(0, unit, pilot="GD05-095")
    st = sc.start()
    assert ("Blocker" in keywords(st, host)) == expected


# ---------------------------------------------------------------------------------------------
# GD05-096 Chad Chadan


def _chad() -> tuple[GameState, int, int]:
    sc = Scenario()
    graze = sc.add(0, "ST05-004", pilot="GD05-096")  # (Tekkadan) 2/2 + 0/2
    barbatos = sc.add(0, "GD03-066", damage=2)  # (Tekkadan) 5/4
    sc.shields(1, VANILLA)
    st = sc.start()
    attack(st, graze)
    return st, graze, barbatos


@pytest.mark.card("GD05-096")
def test_gd05_096_damage_self_to_heal_another_tekkadan_unit() -> None:
    st, graze, barbatos = _chad()
    assert pending(st) is DecisionKind.YES_NO
    yes(st)
    assert st.cards[graze].damage == 1
    assert st.cards[barbatos].damage == 1


@pytest.mark.card("GD05-096")
def test_gd05_096_declining_does_nothing() -> None:
    st, graze, barbatos = _chad()
    no(st)
    assert st.cards[graze].damage == 0
    assert st.cards[barbatos].damage == 2


# ---------------------------------------------------------------------------------------------
# GD05-097 Domon Kasshu


def _domon(top: str) -> tuple[Scenario, int, int, int, int]:
    sc = Scenario()
    sc.resources(0, 4)
    host = sc.add(0, VANILLA)
    domon = sc.add(0, "GD05-097", Zone.HAND)
    keep = sc.add(0, VANILLA, Zone.HAND)
    enemy = sc.add(1, NEO_ZEON_2_3)
    sc.deck(0, top)
    return sc, host, domon, keep, enemy


@pytest.mark.card("GD05-097")
@pytest.mark.ruling("GD05-097:Q396")
def test_gd05_097_discarded_special_move_main_may_be_activated_for_free() -> None:
    sc, host, domon, _keep, enemy = _domon("GD05-110")  # Darkness Finger: 2 damage
    st = sc.start()
    finger = st.zones[0][Zone.DECK][0]
    play(st, domon, onto=host)
    assert pending(st) is DecisionKind.DISCARD
    select(st, finger)
    assert pending(st) is DecisionKind.YES_NO
    yes(st)
    assert st.cards[enemy].damage == 2
    assert zone_of(st, finger) is Zone.TRASH
    assert rested_resources(st, 0) == 1


@pytest.mark.card("GD05-097")
def test_gd05_097_discarding_another_card_offers_nothing() -> None:
    sc, host, domon, keep, enemy = _domon("GD05-110")
    st = sc.start()
    play(st, domon, onto=host)
    select(st, keep)
    assert pending(st) is DecisionKind.MAIN
    assert st.cards[enemy].damage == 0


@pytest.mark.card("GD05-097")
@pytest.mark.ruling("GD05-097:Q397")
def test_gd05_097_activation_counts_as_activating_a_special_move_main() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    haow = sc.add(0, HAOW, pilot="GD05-089")  # observer: needs a (Special Move) activation
    host = sc.add(0, VANILLA)
    domon = sc.add(0, "GD05-097", Zone.HAND)
    sc.add(0, VANILLA, Zone.HAND)
    enemy = sc.add(1, NEO_ZEON_3_4)
    sc.shields(1, VANILLA, VANILLA)
    sc.deck(0, "GD05-121")  # Cyclone Punch (Special Move)
    st = sc.start()
    punch = st.zones[0][Zone.DECK][0]
    play(st, domon, onto=host)
    select(st, punch)
    yes(st)
    attack(st, haow)
    assert st.cards[enemy].damage == 2


# ---------------------------------------------------------------------------------------------
# GD05-098 Heero Yuy


@pytest.mark.card("GD05-098")
@pytest.mark.ruling("GD05-098:Q398")
@pytest.mark.parametrize("with_base", [False, True])
def test_gd05_098_destroying_shield_or_base_gives_enemy_ap_minus_2(with_base: bool) -> None:
    sc = Scenario()
    unit = sc.add(0, VANILLA, pilot="GD05-098")  # 4/3
    enemy = sc.add(1, NEO_ZEON_2_3)
    base = sc.base(1) if with_base else None
    (shield,) = sc.shields(1, VANILLA)
    st = sc.start()
    attack(st, unit)
    pass_all(st)
    if base is not None:
        assert zone_of(st, base) is not Zone.BASE
        assert zone_of(st, shield) is Zone.SHIELD
    else:
        assert zone_of(st, shield) is Zone.TRASH
    assert ap(st, enemy) == 0


# ---------------------------------------------------------------------------------------------
# GD05-099 Trowa Barton


@pytest.mark.card("GD05-099")
@pytest.mark.ruling("GD05-099:Q399")
@pytest.mark.parametrize("enemy_card", [VANILLA, NEO_ZEON_4_1])  # survive / both destroyed
def test_gd05_099_destroying_enemy_in_battle_draws_then_discards(enemy_card: str) -> None:
    sc = Scenario()
    unit = sc.add(0, VANILLA, pilot="GD05-099")  # 3/4 (4/1 enemy deals 4)
    keep = sc.add(0, VANILLA, Zone.HAND)
    enemy = sc.add(1, enemy_card, rested=True)
    st = sc.start()
    attack(st, unit, enemy)
    pass_all(st)
    assert zone_of(st, enemy) is Zone.TRASH
    assert (zone_of(st, unit) is Zone.TRASH) == (enemy_card == NEO_ZEON_4_1)
    assert pending(st) is DecisionKind.DISCARD
    select(st, keep)
    assert hand_size(st, 0) == 1


@pytest.mark.card("GD05-099")
def test_gd05_099_no_trigger_during_opponents_turn() -> None:
    sc = Scenario(active=1)
    unit = sc.add(0, VANILLA, pilot="GD05-099", rested=True)
    sc.add(0, VANILLA, Zone.HAND)
    enemy = sc.add(1, VANILLA)
    st = sc.start()
    attack(st, enemy, unit)
    pass_all(st)
    assert zone_of(st, enemy) is Zone.TRASH
    assert pending(st) is DecisionKind.MAIN
    assert hand_size(st, 0) == 1


# ---------------------------------------------------------------------------------------------
# GD05-100 Quatre Raberba Winner


@pytest.mark.card("GD05-100")
def test_gd05_100_when_paired_rests_enemy_lv5_or_lower() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    host = sc.add(0, VANILLA)
    quatre = sc.add(0, "GD05-100", Zone.HAND)
    low = sc.add(1, VANILLA_4_5)  # Lv5
    high = sc.add(1, VANILLA_6_4)  # Lv6
    st = sc.start()
    play(st, quatre, onto=host)
    assert st.cards[low].rested
    assert not st.cards[high].rested


# ---------------------------------------------------------------------------------------------
# GD05-101 Gavane Goonny

TURN_A = "GD04-073"  # ∀ Gundam (Militia): 【Activate･Main】【Once per Turn】①：AP+2


@pytest.mark.card("GD05-101")
def test_gd05_101_paying_for_a_unit_effect_lets_militia_unit_recover_2() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    borjarnon = sc.add(0, "GD05-080", pilot="GD05-101", damage=3)  # (Militia) HP 4
    first = sc.add(0, TURN_A)
    second = sc.add(0, TURN_A)
    st = sc.start()
    activate(st, first)
    assert pending(st) is DecisionKind.YES_NO
    yes(st)
    assert st.cards[borjarnon].damage == 1
    activate(st, second)
    assert pending(st) is DecisionKind.MAIN
    assert st.cards[borjarnon].damage == 1


@pytest.mark.card("GD05-101")
def test_gd05_101_non_militia_unit_does_not_trigger() -> None:
    sc = Scenario()
    sc.resources(0, 1)
    host = sc.add(0, VANILLA, pilot="GD05-101", damage=2)
    turn_a = sc.add(0, TURN_A)
    st = sc.start()
    activate(st, turn_a)
    assert pending(st) is DecisionKind.MAIN
    assert st.cards[host].damage == 2


@pytest.mark.card("GD05-101")
def test_gd05_101_paying_for_a_base_effect_does_not_trigger() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    borjarnon = sc.add(0, "GD05-080", pilot="GD05-101", damage=3)
    quiet_zero = sc.base(0, "GD05-126")
    st = sc.start()
    activate(st, quiet_zero)
    assert rested_resources(st, 0) == 2
    assert pending(st) is DecisionKind.MAIN
    assert st.cards[borjarnon].damage == 3


@pytest.mark.card("GD05-101")
@pytest.mark.ruling("GD05-101:Q400")
def test_gd05_101_paying_to_deploy_with_x_divider_counts() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    borjarnon = sc.add(0, "GD05-080", pilot="GD05-101", damage=3)
    divider = sc.add(0, "GD03-051")  # 【When Linked】pay its cost to deploy a Unit from trash
    jamil = sc.add(0, "GD03-096", Zone.HAND)
    (revived,) = sc.trash(0, VANILLA)
    st = sc.start()
    play(st, jamil, onto=divider)
    select(st, revived)
    assert zone_of(st, revived) is Zone.BATTLE
    assert pending(st) is DecisionKind.YES_NO
    yes(st)
    assert st.cards[borjarnon].damage == 1


# ---------------------------------------------------------------------------------------------
# GD05-102 Wings of Light (【Action】)


FREEDOM_4_6 = "GD01-065"  # Freedom Gundam, Lv7 4/6


def _wings(
    *enemy: str, mine_damage: int = 0, enemy_damage: int = 0
) -> tuple[GameState, int, int | None, list[int]]:
    sc = Scenario(active=1)
    sc.resources(0, 5)
    wings = sc.add(0, "GD05-102", Zone.HAND)
    with_anchor(sc)
    mine = sc.add(0, VANILLA_4_5, damage=mine_damage) if mine_damage else None
    enemies = [sc.add(1, n, damage=enemy_damage) for n in enemy]
    st = sc.start(Step.END_ACTION)
    assert pending(st) is DecisionKind.ACTION_STEP
    return st, wings, mine, enemies


@pytest.mark.card("GD05-102")
def test_gd05_102_return_mode_bounces_enemy_with_current_hp_5_or_less() -> None:
    st, wings, _, (target,) = _wings(FREEDOM_4_6, enemy_damage=1)  # current HP 5
    play(st, wings)
    assert set(options(st)) == {Action(A.SELECT, 0), Action(A.SELECT, 1)}
    choose_option(st, 0)
    assert zone_of(st, target) is Zone.HAND


@pytest.mark.card("GD05-102")
@pytest.mark.ruling("GD05-102:Q401")
def test_gd05_102_return_mode_unavailable_without_a_target() -> None:
    st, wings, mine, _ = _wings(FREEDOM_4_6, mine_damage=3)
    assert mine is not None
    play(st, wings)
    assert options(st) == (Action(A.SELECT, 1),)
    choose_option(st, 1)
    select(st, mine)
    assert st.cards[mine].damage == 0


@pytest.mark.card("GD05-102")
@pytest.mark.ruling("GD05-102:Q402")
def test_gd05_102_cannot_be_played_without_units() -> None:
    st, wings, _, _ = _wings()
    assert not has_action(st, A.PLAY_COMMAND, wings)


@pytest.mark.card("GD05-102")
def test_gd05_102_is_action_only() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    wings = sc.add(0, "GD05-102", Zone.HAND)
    sc.add(1, VANILLA)
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, wings)


# ---------------------------------------------------------------------------------------------
# GD05-103 Not with Scattershot!


@pytest.mark.card("GD05-103")
@pytest.mark.ruling("GD05-103:Q403")
@pytest.mark.parametrize("damage", [2, 0])
def test_gd05_103_recovers_1_and_gives_ap_plus_2(damage: int) -> None:
    sc = Scenario()
    sc.resources(0, 4)
    cmd = sc.add(0, "GD05-103", Zone.HAND)
    unit = sc.add(0, VANILLA_4_5, damage=damage)
    st = sc.start()
    play(st, cmd)
    assert st.cards[unit].damage == max(0, damage - 1)
    assert ap(st, unit) == 6


# ---------------------------------------------------------------------------------------------
# GD05-104 At the Risk of One's Life

GUN_EZ_LINKABLE = "GD04-015"  # (League Militaire)(Shrike Team) 2/3, link (Shrike Team)
GUN_EZ_BLOCKER = "GD05-013"  # (League Militaire)(Shrike Team) 2/1 <Blocker>


def _risk(*, play_it: bool, pilot: str) -> tuple[GameState, int, int]:
    sc = Scenario()
    sc.resources(0, 3)
    gun_ez = sc.add(0, GUN_EZ_LINKABLE, pilot=pilot)
    sleeper = sc.add(0, GUN_EZ_BLOCKER, rested=True)
    cmd = sc.add(0, "GD05-104", Zone.HAND)
    enemy = sc.add(1, VANILLA_6_4, rested=True)
    st = sc.start()
    attack(st, gun_ez, enemy)
    assert pending(st) is DecisionKind.ACTION_STEP
    if play_it:
        play(st, cmd)
        select(st, gun_ez)
    pass_all(st)
    return st, gun_ez, sleeper


@pytest.mark.card("GD05-104")
def test_gd05_104_granted_destroyed_effect_sets_league_militaire_unit_active() -> None:
    st, gun_ez, sleeper = _risk(play_it=True, pilot="GD05-104")  # [Helen Jackson] (Shrike Team)
    assert zone_of(st, gun_ez) is Zone.TRASH
    assert not st.cards[sleeper].rested


@pytest.mark.card("GD05-104")
@pytest.mark.parametrize(("play_it", "pilot"), [(False, "GD05-104"), (True, "GD05-091")])
def test_gd05_104_needs_the_command_and_a_link(play_it: bool, pilot: str) -> None:
    st, gun_ez, sleeper = _risk(play_it=play_it, pilot=pilot)
    assert zone_of(st, gun_ez) is Zone.TRASH
    assert st.cards[sleeper].rested


@pytest.mark.card("GD05-104")
def test_gd05_104_granted_effect_ends_with_the_turn() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    gun_ez = sc.add(0, GUN_EZ_LINKABLE, pilot="GD05-104")
    sc.add(0, GUN_EZ_BLOCKER)
    cmd = sc.add(0, "GD05-104", Zone.HAND)
    sc.add(1, VANILLA)
    with_anchor(sc, 1)
    st = sc.start(Step.END_ACTION)
    pass_all(st, max_steps=1)
    play(st, cmd)
    select(st, gun_ez)

    def granted() -> list[str]:
        abilities = V.derived(st).abilities.get(gun_ez, ())
        return [a.card_number for a in abilities if a.card_number == "GD05-104"]

    assert granted() == ["GD05-104"]
    to_next_turn(st)
    assert granted() == []


# ---------------------------------------------------------------------------------------------
# GD05-105 Exclusively Defense-Oriented Policy


@pytest.mark.card("GD05-105")
def test_gd05_105_returns_enemy_lv3_or_lower() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    cmd = sc.add(0, "GD05-105", Zone.HAND)
    low = sc.add(1, VANILLA)
    high = sc.add(1, BLOCKER_3_4)  # Lv4
    st = sc.start()
    play(st, cmd)
    assert zone_of(st, low) is Zone.HAND
    assert zone_of(st, high) is Zone.BATTLE


@pytest.mark.card("GD05-105")
@pytest.mark.rule("13-2-5-1")
def test_gd05_105_burst_activates_main() -> None:
    sc = Scenario()
    attacker = sc.add(0, VANILLA)
    sc.shields(1, "GD05-105")
    st = sc.start()
    attack(st, attacker)
    yes(st)
    assert zone_of(st, attacker) is Zone.HAND


# ---------------------------------------------------------------------------------------------
# GD05-106 Mutual Attraction


@pytest.mark.card("GD05-106")
def test_gd05_106_place_rested_resource_mode() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    sc.resource_deck(0, 2)
    cmd = sc.add(0, "GD05-106", Zone.HAND)
    sc.trash(0, "GD05-085")
    st = sc.start()
    play(st, cmd)
    choose_option(st, 0)
    assert len(st.zones[0][Zone.RESOURCE_AREA]) == 4
    assert rested_resources(st, 0) == 4


@pytest.mark.card("GD05-106")
def test_gd05_106_pilot_from_trash_mode() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    cmd = sc.add(0, "GD05-106", Zone.HAND)
    (amuro,) = sc.trash(0, "GD05-085")  # Lv5 Pilot
    st = sc.start()
    play(st, cmd)
    choose_option(st, 1)
    assert zone_of(st, amuro) is Zone.HAND


@pytest.mark.card("GD05-106")
@pytest.mark.ruling("GD05-106:Q404")
def test_gd05_106_pilot_mode_unavailable_without_lv5_pilot() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    sc.resource_deck(0, 1)
    cmd = sc.add(0, "GD05-106", Zone.HAND)
    sc.trash(0, "GD05-087")  # Lv3 Pilot
    st = sc.start()
    play(st, cmd)
    assert options(st) == (Action(A.SELECT, 0),)


# ---------------------------------------------------------------------------------------------
# GD05-107 Interwoven Blessings


@pytest.mark.card("GD05-107")
@pytest.mark.ruling("GD05-107:Q405")
def test_gd05_107_destroys_base_and_top_shield() -> None:
    sc = Scenario()
    sc.resources(0, 10)
    cmd = sc.add(0, "GD05-107", Zone.HAND)
    base = sc.base(1, "GD01-126")
    top, second = sc.shields(1, VANILLA, VANILLA)
    st = sc.start()
    play(st, cmd)
    assert zone_of(st, base) is Zone.TRASH
    assert zone_of(st, top) is Zone.TRASH
    assert zone_of(st, second) is Zone.SHIELD


@pytest.mark.card("GD05-107")
@pytest.mark.ruling("GD05-107:Q406", "GD05-107:Q407")
@pytest.mark.rule("10-1-6-8")
def test_gd05_107_two_shields_revealed_together_and_owner_orders_bursts() -> None:
    sc = Scenario()
    sc.resources(0, 10)
    cmd = sc.add(0, "GD05-107", Zone.HAND)
    first, second, third = sc.shields(1, "GD01-097", "GD05-081", VANILLA)
    st = sc.start()
    play(st, cmd)
    assert pending(st) is DecisionKind.ORDER_TRIGGER
    assert st.pending is not None and st.pending.player == 1
    assert len(options(st)) == 2
    assert st.cards[first].known == st.cards[second].known == 0b11
    order(st, 1)
    yes(st)
    yes(st)
    assert zone_of(st, first) is Zone.HAND
    assert zone_of(st, second) is Zone.HAND
    assert zone_of(st, third) is Zone.SHIELD


@pytest.mark.card("GD05-107")
@pytest.mark.rule("13-2-5-1")
def test_gd05_107_burst_places_an_ex_resource() -> None:
    sc = Scenario()
    attacker = sc.add(0, VANILLA)
    sc.shields(1, "GD05-107")
    st = sc.start()
    attack(st, attacker)
    yes(st)
    assert len(st.zones[1][Zone.RESOURCE_AREA]) == 1


# ---------------------------------------------------------------------------------------------
# GD05-108 Overcoming Hardships


@pytest.mark.card("GD05-108")
@pytest.mark.rule("5-22-2")
def test_gd05_108_redirects_enemy_attack_to_rested_academy_unit() -> None:
    sc = Scenario(active=1)
    sc.resources(0, 3)
    cmd = sc.add(0, "GD05-108", Zone.HAND)
    academy = sc.add(0, ACADEMY_2_2, rested=True)
    shields = sc.shields(0, VANILLA)
    attacker = sc.add(1, VANILLA)
    st = sc.start()
    attack(st, attacker)
    assert pending(st) is DecisionKind.ACTION_STEP
    play(st, cmd)
    pass_all(st)
    assert zone_of(st, academy) is Zone.TRASH
    assert zone_of(st, attacker) is Zone.TRASH
    assert zone_of(st, shields[0]) is Zone.SHIELD


@pytest.mark.card("GD05-108")
def test_gd05_108_does_not_retarget_your_own_attack() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    cmd = sc.add(0, "GD05-108", Zone.HAND)
    academy = sc.add(0, ACADEMY_2_2, rested=True)
    attacker = sc.add(0, VANILLA)
    (shield,) = sc.shields(1, VANILLA)
    st = sc.start()
    attack(st, attacker)
    play(st, cmd)
    pass_all(st)
    assert zone_of(st, shield) is Zone.TRASH
    assert zone_of(st, academy) is Zone.BATTLE


# ---------------------------------------------------------------------------------------------
# GD05-109 Felsi's Plea


@pytest.mark.card("GD05-109")
@pytest.mark.parametrize(("pilot", "draws"), [("GD05-087", 1), ("GD01-098", 0)])
def test_gd05_109_recovers_2_then_draws_with_lv3_or_lower_pilot(pilot: str, draws: int) -> None:
    sc = Scenario(active=1)
    sc.resources(0, 3)
    cmd = sc.add(0, "GD05-109", Zone.HAND)
    with_anchor(sc)
    unit = sc.add(0, "GD01-083", pilot=pilot, damage=2)  # (Academy) Guel's Dilanza
    st = sc.start(Step.END_ACTION)
    before = hand_size(st, 0)
    play(st, cmd)
    assert pending(st) is DecisionKind.ACTION_STEP
    assert st.cards[unit].damage == 0
    assert hand_size(st, 0) == before - 1 + draws


# ---------------------------------------------------------------------------------------------
# GD05-110 Darkness Finger


@pytest.mark.card("GD05-110")
@pytest.mark.parametrize(("master", "draws"), [(True, 1), (False, 0)])
def test_gd05_110_deals_2_and_draws_with_master_gundam(master: bool, draws: int) -> None:
    sc = Scenario()
    sc.resources(0, 4)
    finger = sc.add(0, "GD05-110", Zone.HAND)
    if master:
        sc.add(0, "GD05-033")
    enemy = sc.add(1, NEO_ZEON_2_3)
    st = sc.start()
    before = hand_size(st, 0)
    play(st, finger)
    assert st.cards[enemy].damage == 2
    assert hand_size(st, 0) == before - 1 + draws


@pytest.mark.card("GD05-110")
@pytest.mark.ruling("GD05-110:Q408")
@pytest.mark.rule("10-1-8-1-1")
def test_gd05_110_cannot_be_played_without_enemy_unit() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    finger = sc.add(0, "GD05-110", Zone.HAND)
    sc.add(0, "GD05-033")
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, finger)


@pytest.mark.card("GD05-110")
@pytest.mark.rule("13-2-5-1")
def test_gd05_110_burst_activates_main() -> None:
    sc = Scenario()
    attacker = sc.add(0, VANILLA)
    sc.shields(1, "GD05-110")
    st = sc.start()
    attack(st, attacker)
    yes(st)
    assert zone_of(st, attacker) is Zone.TRASH


# ---------------------------------------------------------------------------------------------
# GD05-111 Airframe Seizure


@pytest.mark.card("GD05-111")
@pytest.mark.rule("5-20-1")
def test_gd05_111_discard_then_draw_2() -> None:
    sc = Scenario()
    sc.resources(0, 1)
    cmd = sc.add(0, "GD05-111", Zone.HAND)
    junk = sc.add(0, VANILLA, Zone.HAND)
    st = sc.start()
    play(st, cmd)
    assert zone_of(st, junk) is Zone.TRASH
    assert hand_size(st, 0) == 2


@pytest.mark.card("GD05-111")
def test_gd05_111_no_draw_without_a_discard() -> None:
    sc = Scenario()
    sc.resources(0, 1)
    cmd = sc.add(0, "GD05-111", Zone.HAND)
    st = sc.start()
    play(st, cmd)
    assert hand_size(st, 0) == 0


# ---------------------------------------------------------------------------------------------
# GD05-112 / 113 / 121 / 122: pair this card from the trash after its 【Main】


@pytest.mark.card("GD05-112")
@pytest.mark.ruling("GD05-112:Q409")
@pytest.mark.rule("3-4-4", "5-9-1")
def test_gd05_112_breach_3_then_pairs_from_trash() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    cmd = sc.add(0, "GD05-112", Zone.HAND)
    dragon = sc.add(0, "GD05-035")  # (MF), link [Sai Saici]
    st = sc.start()
    play(st, cmd)
    assert keywords(st, dragon).get("Breach") == 3
    assert zone_of(st, cmd) is Zone.TRASH
    select(st, dragon)
    assert zone_of(st, cmd) is Zone.PAIRED
    assert st.cards[dragon].pair == cmd
    assert linked(st, dragon)
    assert rested_resources(st, 0) == 1


@pytest.mark.card("GD05-112")
def test_gd05_112_declining_the_pair_leaves_it_in_trash() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    cmd = sc.add(0, "GD05-112", Zone.HAND)
    dragon = sc.add(0, "GD05-035")
    st = sc.start()
    play(st, cmd)
    act(st, A.DONE)
    assert zone_of(st, cmd) is Zone.TRASH
    assert st.cards[dragon].pair < 0


@pytest.mark.card("GD05-113")
@pytest.mark.ruling("GD05-113:Q410")
def test_gd05_113_ap_plus_2_to_mf_with_4_or_less_ap_then_pairs() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    cmd = sc.add(0, "GD05-113", Zone.HAND)
    rose = sc.add(0, "GD05-044")  # (MF) 3/3, link [George de Sand]
    master = sc.add(0, "GD05-033")  # (MF) 5/5: not a legal target
    st = sc.start()
    play(st, cmd)
    assert ap(st, rose) == 5
    assert ap(st, master) == 5
    select(st, rose)
    assert st.cards[rose].pair == cmd
    assert linked(st, rose)


@pytest.mark.card("GD05-121")
@pytest.mark.ruling("GD05-121:Q414")
def test_gd05_121_enemy_ap_minus_2_then_pairs() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    cmd = sc.add(0, "GD05-121", Zone.HAND)
    maxter = sc.add(0, "GD05-069")  # (MF), link [Chibodee Crocket]
    enemy = sc.add(1, NEO_ZEON_3_4)
    st = sc.start()
    play(st, cmd)
    assert ap(st, enemy) == 1
    select(st, maxter)
    assert st.cards[maxter].pair == cmd
    assert linked(st, maxter)


@pytest.mark.card("GD05-122")
@pytest.mark.ruling("GD05-122:Q415")
def test_gd05_122_rests_enemy_lv4_or_lower_then_pairs() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    cmd = sc.add(0, "GD05-122", Zone.HAND)
    bolt = sc.add(0, "GD05-076")  # (MF), link [Argo Gulskii]
    low = sc.add(1, VANILLA)
    high = sc.add(1, VANILLA_4_5)
    st = sc.start()
    play(st, cmd)
    assert st.cards[low].rested
    assert not st.cards[high].rested
    select(st, bolt)
    assert st.cards[bolt].pair == cmd
    assert linked(st, bolt)


@pytest.mark.card("GD05-122")
def test_gd05_122_non_mf_units_cannot_receive_the_pair() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    cmd = sc.add(0, "GD05-122", Zone.HAND)
    sc.add(0, VANILLA)
    sc.add(1, VANILLA)
    st = sc.start()
    play(st, cmd)
    assert pending(st) is DecisionKind.MAIN
    assert zone_of(st, cmd) is Zone.TRASH


# ---------------------------------------------------------------------------------------------
# GD05-114 Widespread Annihilation


@pytest.mark.card("GD05-114")
@pytest.mark.ruling("GD05-114:Q411")
def test_gd05_114_destroys_every_lv4_or_lower_unit() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    cmd = sc.add(0, "GD05-114", Zone.HAND)
    mine_low = sc.add(0, VANILLA)
    mine_high = sc.add(0, VANILLA_4_5)
    enemy_low = sc.add(1, BLOCKER_3_4)
    enemy_token = sc.add(1, LM_TOKEN)
    enemy_high = sc.add(1, VANILLA_6_4)
    st = sc.start()
    play(st, cmd)
    assert zone_of(st, mine_low) is Zone.TRASH
    assert zone_of(st, enemy_low) is Zone.TRASH
    assert zone_of(st, enemy_token) is Zone.OUTSIDE
    assert zone_of(st, mine_high) is Zone.BATTLE
    assert zone_of(st, enemy_high) is Zone.BATTLE


# ---------------------------------------------------------------------------------------------
# GD05-115 Newtype Labs Director


@pytest.mark.card("GD05-115")
def test_gd05_115_adds_neo_zeon_pilot_from_trash() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    cmd = sc.add(0, "GD05-115", Zone.HAND)
    (char,) = sc.trash(0, "GD05-093")
    st = sc.start()
    play(st, cmd)
    assert zone_of(st, char) is Zone.HAND


@pytest.mark.card("GD05-115")
@pytest.mark.rule("10-1-8-1-1")
def test_gd05_115_not_playable_without_neo_zeon_pilot_in_trash() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    cmd = sc.add(0, "GD05-115", Zone.HAND)
    sc.trash(0, "GD05-085")
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, cmd)


@pytest.mark.card("GD05-115")
@pytest.mark.rule("13-2-5-1")
def test_gd05_115_burst_draws_1() -> None:
    sc = Scenario()
    attacker = sc.add(0, VANILLA)
    sc.shields(1, "GD05-115")
    st = sc.start()
    attack(st, attacker)
    yes(st)
    assert hand_size(st, 1) == 1


# ---------------------------------------------------------------------------------------------
# GD05-116 Veteran's Pride


@pytest.mark.card("GD05-116")
def test_gd05_116_destroys_enemy_lv2_or_lower() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    cmd = sc.add(0, "GD05-116", Zone.HAND)
    low = sc.add(1, VANILLA)
    high = sc.add(1, "GD02-032")  # Lv3
    st = sc.start()
    play(st, cmd)
    assert zone_of(st, low) is Zone.TRASH
    assert zone_of(st, high) is Zone.BATTLE


@pytest.mark.card("GD05-116")
def test_gd05_116_not_playable_against_lv3_only() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    cmd = sc.add(0, "GD05-116", Zone.HAND)
    sc.add(1, "GD02-032")
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, cmd)


# ---------------------------------------------------------------------------------------------
# GD05-117 Become a Shield


@pytest.mark.card("GD05-117")
def test_gd05_117_deals_1_to_your_unit_and_an_enemy_unit() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    cmd = sc.add(0, "GD05-117", Zone.HAND)
    mine = sc.add(0, NEO_ZEON_2_3)
    enemy = sc.add(1, NEO_ZEON_2_3)
    st = sc.start()
    play(st, cmd)
    assert st.cards[mine].damage == 1
    assert st.cards[enemy].damage == 1


@pytest.mark.card("GD05-117")
@pytest.mark.ruling("GD05-117:Q412")
def test_gd05_117_needs_both_targets() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    cmd = sc.add(0, "GD05-117", Zone.HAND)
    sc.add(0, NEO_ZEON_2_3)
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, cmd)


# ---------------------------------------------------------------------------------------------
# GD05-118 Incendiary Spark


@pytest.mark.card("GD05-118")
@pytest.mark.parametrize("ex", [1, 0])
def test_gd05_118_ap_minus_2_and_rest_when_played_with_ex_resource(ex: int) -> None:
    sc = Scenario()
    sc.resources(0, 2, ex=1)
    cmd = sc.add(0, "GD05-118", Zone.HAND)
    enemy = sc.add(1, NEO_ZEON_3_4)
    st = sc.start()
    play(st, cmd, ex=ex)
    assert ap(st, enemy) == 1
    assert st.cards[enemy].rested == bool(ex)


# ---------------------------------------------------------------------------------------------
# GD05-119 A Wind Against Fires


def _wind(attacker_card: str) -> tuple[GameState, int, int, int]:
    sc = Scenario()
    sc.resources(0, 5)
    cmd = sc.add(0, "GD05-119", Zone.HAND)
    with_anchor(sc)
    attacker = sc.add(0, attacker_card)
    enemy = sc.add(1, "GD01-065", rested=True)  # Freedom Gundam 4/6
    st = sc.start()
    attack(st, attacker, enemy)
    assert pending(st) is DecisionKind.ACTION_STEP
    return st, cmd, attacker, enemy


@pytest.mark.card("GD05-119")
@pytest.mark.rule("8-6-1")
def test_gd05_119_enemy_battling_lv5_unit_gets_ap_minus_3_this_battle() -> None:
    st, cmd, attacker, enemy = _wind(VANILLA_4_5)
    play(st, cmd)
    assert ap(st, enemy) == 1
    pass_all(st)
    assert st.cards[attacker].damage == 1
    assert zone_of(st, enemy) is Zone.BATTLE
    assert ap(st, enemy) == 4


@pytest.mark.card("GD05-119")
def test_gd05_119_not_playable_when_your_unit_is_below_lv5() -> None:
    st, cmd, _attacker, _enemy = _wind(BLOCKER_3_4)  # Lv4
    assert not has_action(st, A.PLAY_COMMAND, cmd)


# ---------------------------------------------------------------------------------------------
# GD05-120 Shining Finger


@pytest.mark.card("GD05-120")
def test_gd05_120_rests_enemy_then_may_give_shining_gundam_first_strike() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    cmd = sc.add(0, "GD05-120", Zone.HAND)
    shining = sc.add(0, "GD05-042")  # Shining Gundam
    weakened = sc.add(1, VANILLA_4_5, damage=1)  # current HP 4
    st = sc.start()
    play(st, cmd)
    assert st.cards[weakened].rested
    select(st, shining)
    assert keywords(st, shining).get("First Strike") == 1


@pytest.mark.card("GD05-120")
@pytest.mark.ruling("GD05-120:Q413")
def test_gd05_120_not_playable_without_enemy_with_4_or_less_hp() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    cmd = sc.add(0, "GD05-120", Zone.HAND)
    sc.add(0, "GD05-042")
    sc.add(1, VANILLA_4_5)  # 5 HP
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, cmd)


# ---------------------------------------------------------------------------------------------
# GD05-123 Archangel

ORB = "GD05-010"  # Kira's Strike Rouge, (Orb) Lv4 3/4


def _archangel(command: str, *, active: int = 1) -> tuple[Scenario, int, int, int]:
    sc = Scenario(active=active)
    sc.base(0, "GD05-123")
    orb = sc.add(0, ORB)
    other = sc.add(0, VANILLA_4_5)
    sc.resources(1, 4)
    cmd = sc.add(1, command, Zone.HAND)
    return sc, orb, other, cmd


@pytest.mark.card("GD05-123")
def test_gd05_123_orb_units_ignore_2_enemy_effect_damage_on_opponents_turn() -> None:
    sc, orb, _other, finger = _archangel("GD05-110")
    st = sc.start()
    play(st, finger)
    select(st, orb)
    assert st.cards[orb].damage == 0


@pytest.mark.card("GD05-123")
def test_gd05_123_non_orb_units_are_not_protected() -> None:
    sc, _orb, other, finger = _archangel("GD05-110")
    st = sc.start()
    play(st, finger)
    select(st, other)
    assert st.cards[other].damage == 2


@pytest.mark.card("GD05-123")
def test_gd05_123_no_protection_during_your_turn() -> None:
    sc, orb, _other, finger = _archangel("GD05-110", active=0)
    st = sc.start(Step.END_ACTION)
    play(st, finger)
    select(st, orb)
    assert st.cards[orb].damage == 2


@pytest.mark.card("GD05-123")
@pytest.mark.xfail(
    strict=True,
    reason="DSL: CANT_RECEIVE_DAMAGE has no 'N or less' amount threshold (core._prevented ignores RuleMod.amount)",
)
def test_gd05_123_three_enemy_effect_damage_is_received() -> None:
    sc, orb, _other, technique = _archangel("GD03-109")  # 3 damage to a Lv.4 or lower Unit
    st = sc.start()
    play(st, technique)
    assert st.cards[orb].damage == 3


@pytest.mark.card("GD05-123")
@pytest.mark.ruling("GD05-123:Q416")
def test_gd05_123_damage_reduced_to_2_or_less_is_not_received() -> None:
    sc, orb, _other, technique = _archangel("GD03-109")
    st = sc.start()
    reduce1 = d.RuleGrant(d.RuleMod(d.RuleKind.REDUCE_DAMAGE, amount=1))
    add_lasting(st, reduce1, 0, orb, ((orb, st.cards[orb].zone_seq),), Duration.WHILE_ON_FIELD)
    play(st, technique)
    assert st.cards[orb].damage == 0


# ---------------------------------------------------------------------------------------------
# GD05-124 White Ark

V_DASH = "GD04-006"  # 【Activate･Main】Rest 1 of your other (League Militaire) Units：rest an enemy
V2 = "GD05-001"  # 【Activate･Main】Rest 2 of your Units：set this Unit as active
JAVELIN = "GD05-014"  # (League Militaire) Lv2 2/2


@pytest.mark.card("GD05-124")
@pytest.mark.rule("10-1-9-1")
def test_gd05_124_rest_this_base_instead_of_a_unit() -> None:
    sc = Scenario()
    ark = sc.base(0, "GD05-124")
    v_dash = sc.add(0, V_DASH)
    javelin = sc.add(0, JAVELIN)
    enemy = sc.add(1, VANILLA)
    st = sc.start()
    activate(st, v_dash)
    assert st.pending is not None and st.pending.prompt == "rest instead?"
    select(st, ark)
    assert st.cards[ark].rested
    assert not st.cards[javelin].rested
    assert st.cards[enemy].rested


@pytest.mark.card("GD05-124")
@pytest.mark.parametrize(("active", "expected"), [(0, True), (1, False)])
def test_gd05_124_substitution_only_during_your_turn(active: int, expected: bool) -> None:
    sc = Scenario(active=active)
    ark = sc.base(0, "GD05-124")
    st = sc.start()
    assert bool(V.rules_of(V.derived(st), ark, d.RuleKind.REST_SUBSTITUTE)) == expected


@pytest.mark.card("GD05-124")
@pytest.mark.ruling("GD05-124:Q417")
def test_gd05_124_needs_the_unit_that_would_be_rested() -> None:
    sc = Scenario()
    sc.base(0, "GD05-124")
    v_dash = sc.add(0, V_DASH)
    sc.add(1, VANILLA)
    st = sc.start()
    assert not has_action(st, A.ACTIVATE, v_dash)


@pytest.mark.card("GD05-124")
@pytest.mark.ruling("GD05-124:Q418")
@pytest.mark.xfail(
    strict=True,
    reason="ENGINE: interp._rest_substitutes offers the same Base again for a second Unit of one Rest",
)
def test_gd05_124_replaces_only_one_of_two_units() -> None:
    sc = Scenario()
    ark = sc.base(0, "GD05-124")
    v2 = sc.add(0, V2, rested=True)
    first = sc.add(0, JAVELIN)
    second = sc.add(0, JAVELIN)
    st = sc.start()
    activate(st, v2)
    select(st, ark, done=False)
    assert pending(st) is DecisionKind.MAIN
    assert st.cards[ark].rested
    assert not st.cards[first].rested
    assert st.cards[second].rested
    assert not st.cards[v2].rested


# ---------------------------------------------------------------------------------------------
# GD05-125 Ra Cailum


@pytest.mark.card("GD05-125")
@pytest.mark.ruling("GD05-125:Q419")
def test_gd05_125_londo_bell_unit_takes_1_less_enemy_damage_this_turn() -> None:
    sc = Scenario()
    ra_cailum = sc.base(0, "GD05-125")
    jegan = sc.add(0, "GD05-027")  # (Londo Bell) 2/2
    enemy = sc.add(1, VANILLA, rested=True)
    st = sc.start()
    activate(st, ra_cailum)
    assert st.cards[ra_cailum].rested
    attack(st, jegan, enemy)
    pass_all(st)
    assert zone_of(st, enemy) is Zone.TRASH
    assert zone_of(st, jegan) is Zone.BATTLE
    assert st.cards[jegan].damage == 1


@pytest.mark.card("GD05-125")
def test_gd05_125_not_activatable_without_londo_bell_unit() -> None:
    sc = Scenario()
    ra_cailum = sc.base(0, "GD05-125")
    sc.add(0, VANILLA)
    st = sc.start()
    assert not has_action(st, A.ACTIVATE, ra_cailum)


# ---------------------------------------------------------------------------------------------
# GD05-126 Quiet Zero


@pytest.mark.card("GD05-126")
@pytest.mark.parametrize(("aerial", "tokens"), [("GD01-070", 1), ("GD01-082", 0)])  # Lv5 / Lv4
def test_gd05_126_deploys_gundnode_with_lv5_aerial(aerial: str, tokens: int) -> None:
    sc = Scenario()
    sc.resources(0, 4)
    quiet_zero = sc.base(0, "GD05-126")
    sc.add(0, aerial)
    st = sc.start()
    activate(st, quiet_zero)
    assert rested_resources(st, 0) == 2
    made = [u for u in st.zones[0][Zone.BATTLE] if V.cdef(st, u).name == "Gundnode"]
    assert len(made) == tokens
    for u in made:
        assert (ap(st, u), hp(st, u)) == (2, 2)
        assert keywords(st, u).get("Breach") == 1
    assert not has_action(st, A.ACTIVATE, quiet_zero)


# ---------------------------------------------------------------------------------------------
# GD05-127 Girty Lue

CHAOS = "GD05-039"  # (Phantom Pain), link [Sting Oakley]


@pytest.mark.card("GD05-127")
@pytest.mark.rule("13-1-4-1")
def test_gd05_127_phantom_pain_link_stops_an_enemy_blocker() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    sc.base(0, "GD05-127")
    chaos = sc.add(0, CHAOS)
    sting = sc.add(0, "GD05-091", Zone.HAND)
    attacker = sc.add(0, VANILLA)
    blocker = sc.add(1, BLOCKER_3_4)
    sc.shields(1, VANILLA)
    st = sc.start()
    play(st, sting, onto=chaos)
    assert linked(st, chaos)
    attack(st, attacker)
    assert pending(st) is not DecisionKind.BLOCK
    assert not st.cards[blocker].rested


@pytest.mark.card("GD05-127")
def test_gd05_127_unlinked_pairing_does_not_trigger() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    sc.base(0, "GD05-127")
    host = sc.add(0, ABYSS)  # (Phantom Pain), link [Auel Neider]
    sting = sc.add(0, "GD05-091", Zone.HAND)
    attacker = sc.add(0, VANILLA)
    sc.add(1, BLOCKER_3_4)
    sc.shields(1, VANILLA)
    st = sc.start()
    play(st, sting, onto=host)
    assert not linked(st, host)
    attack(st, attacker)
    assert pending(st) is DecisionKind.BLOCK


@pytest.mark.card("GD05-127")
def test_gd05_127_once_per_turn() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    sc.base(0, "GD05-127")
    chaos = sc.add(0, CHAOS)
    abyss = sc.add(0, ABYSS)
    sting = sc.add(0, "GD05-091", Zone.HAND)
    auel = sc.add(0, "GD05-092", Zone.HAND)
    first = sc.add(1, BLOCKER_3_4)
    second = sc.add(1, VANILLA)
    st = sc.start()
    play(st, sting, onto=chaos)
    select(st, first)
    play(st, auel, onto=abyss)
    assert linked(st, abyss)
    assert pending(st) is DecisionKind.MAIN
    assert not V.rules_of(V.derived(st), second, d.RuleKind.CANT_BLOCK)


@pytest.mark.card("GD05-127")
@pytest.mark.ruling("GD05-127:Q420")
def test_gd05_127_blocker_gained_later_still_cannot_be_activated() -> None:
    sc = Scenario()
    sc.resources(0, 7)
    sc.base(0, "GD05-127")
    chaos = sc.add(0, CHAOS)
    sting = sc.add(0, "GD05-091", Zone.HAND)
    hammer = sc.add(0, "GD05-122", Zone.HAND)
    attacker = sc.add(0, VANILLA)
    core_fighter = sc.add(1, "GD04-013")  # rested: League Militaire tokens gain <Blocker>
    parts = sc.add(1, LM_TOKEN)
    sc.shields(1, VANILLA)
    st = sc.start()
    play(st, sting, onto=chaos)
    select(st, parts)
    assert "Blocker" not in keywords(st, parts)
    play(st, hammer)
    select(st, core_fighter)
    assert "Blocker" in keywords(st, parts)
    attack(st, attacker)
    assert pending(st) is not DecisionKind.BLOCK


# ---------------------------------------------------------------------------------------------
# GD05-128 Gundam Fight


@pytest.mark.card("GD05-128")
@pytest.mark.parametrize(("pilot", "bonus"), [("GD05-122", 2), ("GD05-121", 0)])
def test_gd05_128_ap_plus_2_with_an_mf_link_unit(pilot: str, bonus: int) -> None:
    sc = Scenario()
    fight = sc.base(0, "GD05-128")
    bolt = sc.add(0, "GD05-076", pilot=pilot)
    st = sc.start()
    before = ap(st, bolt)
    activate(st, fight)
    assert st.cards[fight].rested
    assert ap(st, bolt) == before + bonus


# ---------------------------------------------------------------------------------------------
# GD05-129 Axis

JAGD_DOGA = "GD05-057"  # (Neo Zeon): 【Activate･Main】destroy 1 of your other Units
ZSSA = "GD04-043"  # (Neo Zeon) Lv3, 【Deploy】deal 1 damage to an enemy Base


@pytest.mark.card("GD05-129")
@pytest.mark.ruling("GD05-129:Q421", "GD05-129:Q422")
def test_gd05_129_deploys_neo_zeon_unit_after_neo_zeon_effect_destroyed_yours() -> None:
    sc = Scenario()
    axis = sc.base(0, "GD05-129")
    jagd = sc.add(0, JAGD_DOGA)
    victim = sc.add(0, VANILLA)
    zssa = sc.add(0, ZSSA, Zone.HAND)
    enemy_base = sc.base(1)
    st = sc.start()
    activate(st, jagd)
    assert zone_of(st, victim) is Zone.TRASH
    activate(st, axis)
    assert zone_of(st, zssa) is Zone.BATTLE
    assert st.cards[enemy_base].damage == 1


@pytest.mark.card("GD05-129")
def test_gd05_129_nothing_without_a_destroyed_unit() -> None:
    sc = Scenario()
    axis = sc.base(0, "GD05-129")
    zssa = sc.add(0, ZSSA, Zone.HAND)
    st = sc.start()
    activate(st, axis)
    assert st.cards[axis].rested
    assert zone_of(st, zssa) is Zone.HAND


@pytest.mark.card("GD05-129")
@pytest.mark.xfail(
    strict=True,
    reason="DSL: turn history does not record the destroying card, so '(Neo Zeon) card's effects' cannot be checked",
)
def test_gd05_129_destruction_by_a_non_neo_zeon_effect_does_not_count() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    axis = sc.base(0, "GD05-129")
    annihilation = sc.add(0, "GD05-114", Zone.HAND)
    sc.add(0, VANILLA)
    zssa = sc.add(0, ZSSA, Zone.HAND)
    st = sc.start()
    play(st, annihilation)
    activate(st, axis)
    assert zone_of(st, zssa) is Zone.HAND


# ---------------------------------------------------------------------------------------------
# GD05-130 Presidential Office


def _office_destroyed() -> tuple[GameState, int, int, int]:
    sc = Scenario(active=1)
    office = sc.base(0, "GD05-130")
    spare = sc.add(0, "GD05-130", Zone.HAND)
    (shield,) = sc.shields(0, VANILLA)
    attacker = sc.add(1, VANILLA_6_4)
    st = sc.start()
    attack(st, attacker)
    pass_all(st)
    return st, office, spare, shield


@pytest.mark.card("GD05-130")
@pytest.mark.ruling("GD05-130:Q423", "GD05-130:Q424")
def test_gd05_130_exile_from_trash_to_deploy_another_office_for_free() -> None:
    st, office, spare, shield = _office_destroyed()
    assert zone_of(st, office) is Zone.TRASH
    assert pending(st) is DecisionKind.YES_NO
    yes(st)
    select(st, spare)
    assert zone_of(st, office) is Zone.REMOVAL
    assert zone_of(st, spare) is Zone.BASE
    assert zone_of(st, shield) is Zone.HAND


@pytest.mark.card("GD05-130")
def test_gd05_130_declining_keeps_it_in_trash() -> None:
    st, office, spare, _shield = _office_destroyed()
    no(st)
    assert zone_of(st, office) is Zone.TRASH
    assert zone_of(st, spare) is Zone.HAND
