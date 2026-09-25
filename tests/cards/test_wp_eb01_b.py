"""Behaviour tests for EB01-047..EB01-090 (work package WP-EB01-B)."""

from __future__ import annotations

import pytest

from gcg_sim.engine import view as V
from gcg_sim.engine.game import advance
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
    keywords,
    no,
    pass_all,
    play,
    select,
    to_next_turn,
    yes,
    zone_of,
)

A = ActionKind

GG_1 = "EB01-051"  # Ze'Gok, White Lv1 1/2 (G Generation), vanilla
GG_2 = "EB01-053"  # Gundam GP00, White Lv2 3/2 (G Generation), vanilla
GG_3 = "EB01-056"  # Gundam Geminass 01, White Lv3 4/3 (G Generation), vanilla
BLUE_GG_3 = "EB01-016"  # Tornado Gundam, Blue Lv3 3/3 (G Generation), link (G Generation)
GREEN_GG_3 = "EB01-032"  # Gundam Ez8 High Mobility Custom, Green Lv3 4/3 (G Generation)
S_GUNDAM = "EB01-048"  # White Lv4 4/3 (G Generation) <Blocker>, link (Attack)
GAPLANT = "EB01-054"  # White Lv3 3/3 (G Generation) <Blocker>
CASVAL = "EB01-047"  # White Lv4 3/4 (G Generation), link (G Generation)
PSYCHO_ZAKU = "EB01-059"  # White Lv5 4/4 (G Generation), link [Daryl Lorenz]
ZAKU = "GD01-060"  # Zaku Mariner, Red Lv2 2/2 (Zeon), vanilla
GOOHN = "GD01-062"  # GOOhN, Red Lv1 1/2 (ZAFT), vanilla
REZEL = "GD01-018"  # ReZEL, Blue Lv3 4/3 (Earth Federation), vanilla
GELGOOG = "GD01-031"  # Gelgoog, Green Lv4 4/3 (Zeon), vanilla
AGRISSA = "GD04-079"  # Agrissa, White Lv5 5/4, vanilla
LAUNCHER = "GD01-072"  # Launcher Strike Gundam, White Lv4 3/4 (Earth Alliance) <Blocker>
RIDDHE = "GD01-089"  # Riddhe Marcenas, Pilot (Earth Federation) 1/1: no link with EB01 Units
REPAIR_UNIT = "GD01-001"  # Gundam, Blue Lv4 3/3: gains <Repair 1> from its own text
ITTOU = "EB01-071"  # Ittou Tsurugi, Pilot (G Generation)(Attack) 2/1
DARYL = "EB01-070"  # Daryl Lorenz, Pilot (G Generation)(Support) 2/1
COMMAND_LV3 = "EB01-075"  # Fierce Enemy Assault, a Command card (not a Unit card)


def _pending(st: GameState) -> DecisionKind | None:
    return st.pending.kind if st.pending is not None else None


def _decider(st: GameState) -> int:
    assert st.pending is not None
    return st.pending.player


def _select_options(st: GameState) -> set[int]:
    assert st.pending is not None
    return {o.a for o in st.pending.options if o.kind is A.SELECT}


def _burst(number: str) -> tuple[GameState, int]:
    """Player 0 attacks; player 1's only Shield is ``number``; player 1 activates its 【Burst】."""
    sc = Scenario()
    attacker = sc.add(0, ZAKU)
    (shield,) = sc.shields(1, number)
    sc.shields(1, ZAKU)
    st = sc.start()
    attack(st, attacker)
    pass_all(st)
    assert _pending(st) is DecisionKind.BURST and _decider(st) == 1
    yes(st)
    return st, shield


def _set_active_and_refresh(st: GameState, uid: int) -> None:
    """Stand in for an effect that sets ``uid`` active in the main phase, then recompute the
    pending main-phase options."""
    st.cards[uid].rested = False
    st.pending = None
    st.touch()
    advance(st)


def _opponent_end_action(sc: Scenario) -> GameState:
    """Start in the end-phase action step of player 1's turn: player 0 has priority first."""
    st = sc.start(Step.END_ACTION)
    assert _pending(st) is DecisionKind.ACTION_STEP and _decider(st) == 0
    return st


# ---------------------------------------------------------------------------------------------
# EB01-047 Casval's Gundam


@pytest.mark.card("EB01-047")
@pytest.mark.rule("13-1-8-1", "13-1-8-2", "13-1-6-1")
def test_eb01_047_development_exiles_one_and_gains_high_maneuver() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    casval = sc.add(0, CASVAL)
    (gg,) = sc.trash(0, GG_1)
    pilot = sc.add(0, ITTOU, Zone.HAND)
    blocker = sc.add(1, GAPLANT)
    sc.shields(1, ZAKU, ZAKU)
    st = sc.start()
    play(st, pilot, onto=casval)
    assert _pending(st) is DecisionKind.YES_NO
    yes(st)
    assert zone_of(st, gg) is Zone.REMOVAL
    assert "High-Maneuver" in keywords(st, casval)
    attack(st, casval)
    assert _pending(st) is not DecisionKind.BLOCK  # 13-1-6-1: the Blocker cannot block
    assert not st.cards[blocker].rested
    pass_all(st)
    to_next_turn(st)
    assert "High-Maneuver" not in keywords(st, casval)


@pytest.mark.card("EB01-047")
@pytest.mark.rule("13-1-8-1")
def test_eb01_047_development_is_optional() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    casval = sc.add(0, CASVAL)
    (gg,) = sc.trash(0, GG_1)
    pilot = sc.add(0, ITTOU, Zone.HAND)
    st = sc.start()
    play(st, pilot, onto=casval)
    no(st)
    assert zone_of(st, gg) is Zone.TRASH
    assert "High-Maneuver" not in keywords(st, casval)


@pytest.mark.card("EB01-047")
@pytest.mark.rule("13-1-8-1")
def test_eb01_047_development_needs_a_g_generation_card_in_trash() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    casval = sc.add(0, CASVAL)
    (other,) = sc.trash(0, ZAKU)
    pilot = sc.add(0, ITTOU, Zone.HAND)
    st = sc.start()
    play(st, pilot, onto=casval)
    assert _pending(st) is DecisionKind.MAIN
    assert zone_of(st, other) is Zone.TRASH
    assert "High-Maneuver" not in keywords(st, casval)


@pytest.mark.card("EB01-047")
@pytest.mark.rule("13-1-8-1")
def test_eb01_047_development_chooses_which_card_to_exile() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    casval = sc.add(0, CASVAL)
    first, second = sc.trash(0, GG_1, GG_2)
    pilot = sc.add(0, ITTOU, Zone.HAND)
    st = sc.start()
    play(st, pilot, onto=casval)
    yes(st)
    assert _select_options(st) == {first, second}
    select(st, second)
    assert zone_of(st, second) is Zone.REMOVAL
    assert zone_of(st, first) is Zone.TRASH
    assert "High-Maneuver" in keywords(st, casval)


# ---------------------------------------------------------------------------------------------
# EB01-048 S Gundam, EB01-054 Gaplant TR-5 "Hrairoo" Unit 1


@pytest.mark.card("EB01-048", "EB01-054")
@pytest.mark.rule("13-1-4-1")
@pytest.mark.parametrize("number", [S_GUNDAM, GAPLANT])
def test_blocker_units_can_block(number: str) -> None:
    sc = Scenario(active=1)
    attacker = sc.add(1, ZAKU)
    blocker = sc.add(0, number)
    st = sc.start()
    assert keywords(st, blocker).get("Blocker") == 1
    attack(st, attacker)
    assert _pending(st) is DecisionKind.BLOCK
    block(st, blocker)
    assert st.cards[blocker].rested
    assert zone_of(st, attacker) is Zone.TRASH
    assert st.winner is None  # player 0 has no Shields: the attack was redirected


# ---------------------------------------------------------------------------------------------
# EB01-049 Pale Rider (Ground Heavy Equipment Type)


@pytest.mark.card("EB01-049")
@pytest.mark.rule("13-1-7-1")
def test_eb01_049_gains_suppression_with_friendly_gg_blocker() -> None:
    sc = Scenario()
    pale = sc.add(0, "EB01-049")
    sc.add(0, GAPLANT)
    top, second, third = sc.shields(1, ZAKU, ZAKU, ZAKU)
    st = sc.start()
    assert "Suppression" in keywords(st, pale)
    attack(st, pale)
    pass_all(st)
    assert zone_of(st, top) is Zone.TRASH
    assert zone_of(st, second) is Zone.TRASH
    assert zone_of(st, third) is Zone.SHIELD


@pytest.mark.card("EB01-049")
@pytest.mark.parametrize("other", [LAUNCHER, GG_3])
def test_eb01_049_no_suppression_without_gg_blocker(other: str) -> None:
    sc = Scenario()
    pale = sc.add(0, "EB01-049")
    sc.add(0, other)  # a non-(G Generation) Blocker, or a (G Generation) Unit without <Blocker>
    top, second = sc.shields(1, ZAKU, ZAKU)
    st = sc.start()
    assert "Suppression" not in keywords(st, pale)
    attack(st, pale)
    pass_all(st)
    assert zone_of(st, top) is Zone.TRASH
    assert zone_of(st, second) is Zone.SHIELD


# ---------------------------------------------------------------------------------------------
# EB01-050 Saikoro Gundam


@pytest.mark.card("EB01-050")
def test_eb01_050_milled_lv3_card_gives_enemy_ap_minus_2_during_battle() -> None:
    sc = Scenario()
    saikoro = sc.add(0, "EB01-050")
    enemy = sc.add(1, LAUNCHER)
    sc.shields(1, ZAKU)
    sc.deck(0, GG_3)
    st = sc.start()
    top = st.zones[0][Zone.DECK][0]
    attack(st, saikoro)
    assert zone_of(st, top) is Zone.TRASH
    assert _pending(st) is DecisionKind.BLOCK
    assert ap(st, enemy) == 1
    block(st, None)
    pass_all(st)
    assert _pending(st) is DecisionKind.MAIN
    assert ap(st, enemy) == 3  # "during this battle" ended (8-6-1)


@pytest.mark.card("EB01-050")
def test_eb01_050_milled_low_level_card_does_nothing_more() -> None:
    sc = Scenario()
    saikoro = sc.add(0, "EB01-050")
    enemy = sc.add(1, LAUNCHER)
    sc.shields(1, ZAKU)
    sc.deck(0, GG_1)
    st = sc.start()
    top = st.zones[0][Zone.DECK][0]
    attack(st, saikoro)
    assert zone_of(st, top) is Zone.TRASH
    assert _pending(st) is DecisionKind.BLOCK
    assert ap(st, enemy) == 3


@pytest.mark.card("EB01-050")
def test_eb01_050_milled_lv3_command_counts() -> None:
    sc = Scenario()
    saikoro = sc.add(0, "EB01-050")
    enemy = sc.add(1, LAUNCHER)
    sc.shields(1, ZAKU)
    sc.deck(0, COMMAND_LV3)
    st = sc.start()
    attack(st, saikoro)
    assert _pending(st) is DecisionKind.BLOCK
    assert ap(st, enemy) == 1


# ---------------------------------------------------------------------------------------------
# EB01-052 Hildolfr


@pytest.mark.card("EB01-052")
def test_eb01_052_returns_enemy_with_2_or_less_hp_when_3_enemies() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    hildolfr = sc.add(0, "EB01-052", Zone.HAND)
    small_a = sc.add(1, ZAKU)
    small_b = sc.add(1, GOOHN)
    big = sc.add(1, REZEL)
    st = sc.start()
    play(st, hildolfr)
    assert _select_options(st) == {small_a, small_b}
    select(st, small_b)
    assert zone_of(st, small_b) is Zone.HAND
    assert zone_of(st, small_a) is Zone.BATTLE
    assert zone_of(st, big) is Zone.BATTLE


@pytest.mark.card("EB01-052")
def test_eb01_052_nothing_with_only_2_enemy_units() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    hildolfr = sc.add(0, "EB01-052", Zone.HAND)
    small = sc.add(1, ZAKU)
    sc.add(1, REZEL)
    st = sc.start()
    play(st, hildolfr)
    assert _pending(st) is DecisionKind.MAIN
    assert zone_of(st, small) is Zone.BATTLE


# ---------------------------------------------------------------------------------------------
# EB01-055 Dom Gross Beil, EB01-058 Extreme Gundam (2 or more enemy players: never in 1v1)


@pytest.mark.card("EB01-055")
def test_eb01_055_shields_still_take_battle_damage_in_1v1() -> None:
    sc = Scenario(active=1)
    sc.add(0, "EB01-055", rested=True)
    attacker = sc.add(1, ZAKU)
    (shield,) = sc.shields(0, ZAKU)
    st = sc.start()
    attack(st, attacker)
    pass_all(st)
    assert zone_of(st, shield) is Zone.TRASH


@pytest.mark.card("EB01-058")
def test_eb01_058_has_no_blocker_in_1v1() -> None:
    sc = Scenario(active=1)
    extreme = sc.add(0, "EB01-058")
    attacker = sc.add(1, ZAKU)
    sc.shields(0, ZAKU)
    st = sc.start()
    assert "Blocker" not in keywords(st, extreme)
    attack(st, attacker)
    assert _pending(st) is not DecisionKind.BLOCK


# ---------------------------------------------------------------------------------------------
# EB01-057 Gundam Geminass 02


@pytest.mark.card("EB01-057")
@pytest.mark.rule("5-20-1")
def test_eb01_057_rest_lv3_then_return_enemy_lv2() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    geminass = sc.add(0, "EB01-057", Zone.HAND)
    lv3 = sc.add(0, GG_3)
    sc.add(0, GG_2)  # Lv.2: not eligible
    enemy_lv2 = sc.add(1, ZAKU)
    enemy_lv3 = sc.add(1, REZEL)
    st = sc.start()
    play(st, geminass)
    assert _select_options(st) == {lv3}
    act(st, A.SELECT, lv3)
    assert st.cards[lv3].rested
    assert zone_of(st, enemy_lv2) is Zone.HAND
    assert zone_of(st, enemy_lv3) is Zone.BATTLE


@pytest.mark.card("EB01-057")
@pytest.mark.rule("5-20-1")
def test_eb01_057_declining_the_rest_returns_nothing() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    geminass = sc.add(0, "EB01-057", Zone.HAND)
    lv3 = sc.add(0, GG_3)
    enemy_lv2 = sc.add(1, ZAKU)
    st = sc.start()
    play(st, geminass)
    act(st, A.DONE)
    assert not st.cards[lv3].rested
    assert zone_of(st, enemy_lv2) is Zone.BATTLE


@pytest.mark.card("EB01-057")
def test_eb01_057_rested_lv3_unit_cannot_be_chosen() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    geminass = sc.add(0, "EB01-057", Zone.HAND)
    sc.add(0, GG_3, rested=True)
    enemy_lv2 = sc.add(1, ZAKU)
    st = sc.start()
    play(st, geminass)
    assert _pending(st) is DecisionKind.MAIN
    assert zone_of(st, enemy_lv2) is Zone.BATTLE


# ---------------------------------------------------------------------------------------------
# EB01-059 Psycho Zaku


def _psycho_zaku(*, ex: int = 0, pilot: str = DARYL) -> tuple[Scenario, int, list[int], list[int]]:
    sc = Scenario()
    zaku = sc.add(0, PSYCHO_ZAKU, pilot=pilot)
    mine = sc.resources(0, 4, rested=3, ex=ex)
    theirs = sc.resources(1, 3, rested=2)
    sc.shields(1, ZAKU, ZAKU)
    return sc, zaku, mine, theirs


@pytest.mark.card("EB01-059")
@pytest.mark.ruling("EB01-059:Q321")
def test_eb01_059_each_player_sets_one_resource_active_active_player_first() -> None:
    sc, zaku, mine, theirs = _psycho_zaku()
    st = sc.start()
    attack(st, zaku)
    assert _pending(st) is DecisionKind.SELECT and _decider(st) == 0
    assert _select_options(st) == set(mine)
    select(st, mine[0])
    assert _pending(st) is DecisionKind.SELECT and _decider(st) == 1
    assert _select_options(st) == set(theirs)
    select(st, theirs[1])
    assert not st.cards[mine[0]].rested
    assert not st.cards[theirs[1]].rested
    assert sum(st.cards[u].rested for u in mine) == 2
    assert sum(st.cards[u].rested for u in theirs) == 1


@pytest.mark.card("EB01-059")
@pytest.mark.ruling("EB01-059:Q322")
def test_eb01_059_choosing_an_active_resource_does_nothing() -> None:
    sc, zaku, mine, theirs = _psycho_zaku()
    st = sc.start()
    attack(st, zaku)
    select(st, mine[3])  # already active
    select(st, theirs[2])  # already active
    assert sum(st.cards[u].rested for u in mine) == 3
    assert sum(st.cards[u].rested for u in theirs) == 2


@pytest.mark.card("EB01-059")
@pytest.mark.ruling("EB01-059:Q323")
def test_eb01_059_an_active_ex_resource_can_be_chosen() -> None:
    sc, zaku, mine, theirs = _psycho_zaku(ex=1)
    ex_resource = mine[-1]
    st = sc.start()
    attack(st, zaku)
    assert ex_resource in _select_options(st)
    select(st, ex_resource)
    select(st, theirs[0])
    assert zone_of(st, ex_resource) is Zone.RESOURCE_AREA
    assert sum(st.cards[u].rested for u in mine) == 3


@pytest.mark.card("EB01-059")
@pytest.mark.rule("13-2-12-1")
def test_eb01_059_needs_link() -> None:
    sc, zaku, mine, _ = _psycho_zaku(pilot=RIDDHE)
    st = sc.start()
    attack(st, zaku)
    assert _pending(st) is not DecisionKind.SELECT
    assert sum(st.cards[u].rested for u in mine) == 3


@pytest.mark.card("EB01-059")
@pytest.mark.rule("13-2-13-1")
def test_eb01_059_once_per_turn() -> None:
    sc, zaku, mine, theirs = _psycho_zaku()
    st = sc.start()
    attack(st, zaku)
    select(st, mine[0])
    select(st, theirs[0])
    pass_all(st)
    assert _pending(st) is DecisionKind.MAIN
    _set_active_and_refresh(st, zaku)
    attack(st, zaku)
    assert _pending(st) is not DecisionKind.SELECT


# ---------------------------------------------------------------------------------------------
# EB01-060 Gundam Aquarius


def _aquarius(trash: tuple[str, ...], enemy: str) -> tuple[GameState, int, list[int], int]:
    sc = Scenario()
    sc.resources(0, 5)
    aquarius = sc.add(0, "EB01-060")
    cards = sc.trash(0, *trash)
    target = sc.add(1, enemy)
    pilot = sc.add(0, ITTOU, Zone.HAND)
    st = sc.start()
    play(st, pilot, onto=aquarius)
    return st, aquarius, cards, target


@pytest.mark.card("EB01-060")
@pytest.mark.rule("13-1-8-1")
def test_eb01_060_development_3_returns_enemy_lv4_or_lower() -> None:
    st, _, cards, target = _aquarius((GG_1, GG_2, GG_3), REZEL)
    yes(st)
    assert all(zone_of(st, u) is Zone.REMOVAL for u in cards)
    assert zone_of(st, target) is Zone.HAND


@pytest.mark.card("EB01-060")
@pytest.mark.rule("13-1-8-1")
def test_eb01_060_needs_three_g_generation_cards() -> None:
    st, _, cards, target = _aquarius((GG_1, GG_2, ZAKU), REZEL)
    assert _pending(st) is DecisionKind.MAIN
    assert all(zone_of(st, u) is Zone.TRASH for u in cards)
    assert zone_of(st, target) is Zone.BATTLE


@pytest.mark.card("EB01-060")
@pytest.mark.ruling("EB01-060:Q324")
def test_eb01_060_development_may_be_paid_without_a_target() -> None:
    st, _, cards, target = _aquarius((GG_1, GG_2, GG_3), AGRISSA)
    assert _pending(st) is DecisionKind.YES_NO
    yes(st)
    assert all(zone_of(st, u) is Zone.REMOVAL for u in cards)
    assert zone_of(st, target) is Zone.BATTLE


# ---------------------------------------------------------------------------------------------
# Pilots and Commands whose 【Burst】 adds the card to its owner's hand


@pytest.mark.rule("13-2-5-1")
@pytest.mark.parametrize(
    "number",
    [
        pytest.param("EB01-061", marks=pytest.mark.card("EB01-061")),
        pytest.param("EB01-062", marks=pytest.mark.card("EB01-062")),
        pytest.param("EB01-063", marks=pytest.mark.card("EB01-063")),
        pytest.param("EB01-064", marks=pytest.mark.card("EB01-064")),
        pytest.param("EB01-065", marks=pytest.mark.card("EB01-065")),
        pytest.param("EB01-066", marks=pytest.mark.card("EB01-066")),
        pytest.param("EB01-067", marks=pytest.mark.card("EB01-067")),
        pytest.param("EB01-068", marks=pytest.mark.card("EB01-068")),
        pytest.param("EB01-069", marks=pytest.mark.card("EB01-069")),
        pytest.param("EB01-070", marks=pytest.mark.card("EB01-070")),
        pytest.param("EB01-071", marks=pytest.mark.card("EB01-071")),
        pytest.param("EB01-072", marks=pytest.mark.card("EB01-072")),
        pytest.param("EB01-077", marks=pytest.mark.card("EB01-077")),
        pytest.param("EB01-081", marks=pytest.mark.card("EB01-081")),
    ],
)
def test_burst_adds_this_card_to_hand(number: str) -> None:
    st, shield = _burst(number)
    assert zone_of(st, shield) is Zone.HAND


# ---------------------------------------------------------------------------------------------
# EB01-061 Ellis Claude


@pytest.mark.card("EB01-061")
@pytest.mark.rule("13-2-9-1", "3-3-9-2")
def test_eb01_061_rests_enemy_lv3_or_lower_with_friendly_gg_unit() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    unit = sc.add(0, GG_2)
    pilot = sc.add(0, "EB01-061", Zone.HAND)
    low = sc.add(1, ZAKU)
    high = sc.add(1, AGRISSA)
    st = sc.start()
    play(st, pilot, onto=unit)
    assert st.cards[low].rested
    assert not st.cards[high].rested


@pytest.mark.card("EB01-061")
def test_eb01_061_nothing_without_friendly_gg_unit() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    unit = sc.add(0, ZAKU)
    pilot = sc.add(0, "EB01-061", Zone.HAND)
    low = sc.add(1, ZAKU)
    st = sc.start()
    play(st, pilot, onto=unit)
    assert not st.cards[low].rested


# ---------------------------------------------------------------------------------------------
# EB01-062 Jona Basta


def _jona() -> tuple[GameState, int]:
    sc = Scenario()
    unit = sc.add(0, GG_2, pilot="EB01-062")
    sc.shields(1, ZAKU, ZAKU)
    return sc.start(), unit


@pytest.mark.card("EB01-062")
@pytest.mark.rule("5-20-1")
def test_eb01_062_opponent_draws_then_you_draw() -> None:
    st, unit = _jona()
    mine, theirs = len(st.zones[0][Zone.HAND]), len(st.zones[1][Zone.HAND])
    attack(st, unit)
    assert _pending(st) is DecisionKind.YES_NO and _decider(st) == 1
    yes(st)
    assert len(st.zones[1][Zone.HAND]) == theirs + 1
    assert len(st.zones[0][Zone.HAND]) == mine + 1


@pytest.mark.card("EB01-062")
@pytest.mark.rule("5-20-1")
def test_eb01_062_no_draw_when_opponent_declines() -> None:
    st, unit = _jona()
    mine, theirs = len(st.zones[0][Zone.HAND]), len(st.zones[1][Zone.HAND])
    attack(st, unit)
    no(st)
    assert len(st.zones[1][Zone.HAND]) == theirs
    assert len(st.zones[0][Zone.HAND]) == mine


@pytest.mark.card("EB01-062")
@pytest.mark.rule("13-2-13-1")
def test_eb01_062_once_per_turn() -> None:
    st, unit = _jona()
    attack(st, unit)
    yes(st)
    pass_all(st)
    _set_active_and_refresh(st, unit)
    attack(st, unit)
    assert _pending(st) is not DecisionKind.YES_NO


# ---------------------------------------------------------------------------------------------
# EB01-063 Io Fleming


@pytest.mark.card("EB01-063")
@pytest.mark.ruling("EB01-063:Q325")
@pytest.mark.rule("13-1-1-1")
def test_eb01_063_repair_2_with_friendly_and_enemy_rested_units() -> None:
    sc = Scenario()
    unit = sc.add(0, GG_2, pilot="EB01-063", damage=2)
    sc.add(0, GG_1, rested=True)
    sc.add(1, ZAKU, rested=True)
    st = sc.start()
    assert keywords(st, unit).get("Repair") == 2
    to_next_turn(st)
    assert st.cards[unit].damage == 0


@pytest.mark.card("EB01-063")
def test_eb01_063_this_unit_does_not_count_itself() -> None:
    sc = Scenario()
    unit = sc.add(0, GG_2, pilot="EB01-063", rested=True)
    sc.add(1, ZAKU, rested=True)
    sc.add(1, GOOHN)
    st = sc.start()
    assert "Repair" not in keywords(st, unit)


# ---------------------------------------------------------------------------------------------
# EB01-064 Rondo Gina Sahaku


@pytest.mark.card("EB01-064")
@pytest.mark.rule("13-1-2-1")
def test_eb01_064_breach_1_while_unit_has_repair() -> None:
    sc = Scenario()
    unit = sc.add(0, REPAIR_UNIT, pilot="EB01-064")
    enemy = sc.add(1, ZAKU, rested=True)
    top, second = sc.shields(1, ZAKU, ZAKU)
    st = sc.start()
    assert keywords(st, unit).get("Breach") == 1
    attack(st, unit, enemy)
    pass_all(st)
    assert zone_of(st, enemy) is Zone.TRASH
    assert zone_of(st, top) is Zone.TRASH
    assert zone_of(st, second) is Zone.SHIELD


@pytest.mark.card("EB01-064")
def test_eb01_064_no_breach_without_repair() -> None:
    sc = Scenario()
    unit = sc.add(0, GG_2, pilot="EB01-064")
    st = sc.start()
    assert "Breach" not in keywords(st, unit)


# ---------------------------------------------------------------------------------------------
# EB01-065 Meir Siva


@pytest.mark.card("EB01-065")
@pytest.mark.rule("13-2-11-1")
def test_eb01_065_when_linked_grants_breach_1_this_turn() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    s_gundam = sc.add(0, S_GUNDAM)
    other = sc.add(0, GG_2)
    sc.add(0, ZAKU)
    pilot = sc.add(0, "EB01-065", Zone.HAND)
    st = sc.start()
    play(st, pilot, onto=s_gundam)
    assert _select_options(st) == {s_gundam, other}
    select(st, other)
    assert keywords(st, other).get("Breach") == 1
    to_next_turn(st)
    assert "Breach" not in keywords(st, other)


@pytest.mark.card("EB01-065")
def test_eb01_065_no_effect_without_link() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    unit = sc.add(0, GG_2)
    pilot = sc.add(0, "EB01-065", Zone.HAND)
    st = sc.start()
    play(st, pilot, onto=unit)
    assert _pending(st) is DecisionKind.MAIN
    assert "Breach" not in keywords(st, unit)


# ---------------------------------------------------------------------------------------------
# EB01-066 Reiji


@pytest.mark.card("EB01-066")
def test_eb01_066_may_attack_active_enemy_blocker() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    unit = sc.add(0, GG_2)
    pilot = sc.add(0, "EB01-066", Zone.HAND)
    blocker = sc.add(1, GAPLANT)
    plain = sc.add(1, ZAKU)
    st = sc.start()
    play(st, pilot, onto=unit)
    assert has_action(st, A.ATTACK, unit, blocker)
    assert not has_action(st, A.ATTACK, unit, plain)


@pytest.mark.card("EB01-066")
def test_eb01_066_without_it_active_blocker_is_not_a_target() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    unit = sc.add(0, GG_2)
    pilot = sc.add(0, RIDDHE, Zone.HAND)
    blocker = sc.add(1, GAPLANT)
    st = sc.start()
    play(st, pilot, onto=unit)
    assert not has_action(st, A.ATTACK, unit, blocker)


# ---------------------------------------------------------------------------------------------
# EB01-067 Asuna Elmarit


def _asuna(*top: str) -> tuple[GameState, list[int]]:
    sc = Scenario()
    sc.resources(0, 4)
    unit = sc.add(0, GG_2)
    pilot = sc.add(0, "EB01-067", Zone.HAND)
    sc.deck(0, *top)
    st = sc.start()
    looked = list(st.zones[0][Zone.DECK][:3])
    play(st, pilot, onto=unit)
    return st, looked


@pytest.mark.card("EB01-067")
@pytest.mark.ruling("EB01-067:Q482")
def test_eb01_067_reveal_gg_unit_to_top_rest_to_bottom() -> None:
    st, looked = _asuna(ZAKU, GG_2, COMMAND_LV3)
    zaku, gg, command = looked
    assert _pending(st) is DecisionKind.SELECT
    assert _select_options(st) == {gg}
    select(st, gg)
    deck = st.zones[0][Zone.DECK]
    assert deck[0] == gg
    assert set(deck[-2:]) == {zaku, command}
    assert st.cards[gg].known == 0b11  # revealed to both players


@pytest.mark.card("EB01-067")
@pytest.mark.ruling("EB01-067:Q482")
def test_eb01_067_look_is_mandatory_reveal_is_optional() -> None:
    st, looked = _asuna(ZAKU, GG_2, COMMAND_LV3)
    assert _pending(st) is DecisionKind.SELECT  # no yes/no for the effect itself
    act(st, A.DONE)
    deck = st.zones[0][Zone.DECK]
    assert set(deck[-3:]) == set(looked)
    assert not set(deck[:3]) & set(looked)


@pytest.mark.card("EB01-067")
def test_eb01_067_no_eligible_card_all_go_to_bottom() -> None:
    st, looked = _asuna(ZAKU, GOOHN, COMMAND_LV3)
    assert _pending(st) is DecisionKind.MAIN
    assert set(st.zones[0][Zone.DECK][-3:]) == set(looked)


# ---------------------------------------------------------------------------------------------
# EB01-068 Chall Acustica


def _chall(unit_number: str, damage: int) -> tuple[GameState, int, int]:
    """Pair Chall Acustica, then attack a rested 3/2 so both Units are destroyed in battle."""
    sc = Scenario()
    sc.resources(0, 4)
    unit = sc.add(0, unit_number, damage=damage)
    pilot = sc.add(0, "EB01-068", Zone.HAND)
    enemy = sc.add(1, GG_2, rested=True)
    st = sc.start()
    play(st, pilot, onto=unit)
    attack(st, unit, enemy)
    pass_all(st)
    assert zone_of(st, unit) is Zone.TRASH
    assert zone_of(st, enemy) is Zone.TRASH
    return st, unit, pilot


@pytest.mark.card("EB01-068")
@pytest.mark.rule("13-2-8-2-1", "13-2-12-1")
def test_eb01_068_linked_destroyed_returns_pilot_to_deck_top() -> None:
    st, _, pilot = _chall(S_GUNDAM, 2)
    assert _pending(st) is DecisionKind.YES_NO
    yes(st)
    assert zone_of(st, pilot) is Zone.DECK
    assert st.zones[0][Zone.DECK][0] == pilot


@pytest.mark.card("EB01-068")
def test_eb01_068_declined_pilot_stays_in_trash() -> None:
    st, _, pilot = _chall(S_GUNDAM, 2)
    no(st)
    assert zone_of(st, pilot) is Zone.TRASH


@pytest.mark.card("EB01-068")
def test_eb01_068_not_linked_no_effect() -> None:
    st, _, pilot = _chall(GG_2, 1)
    assert _pending(st) is DecisionKind.MAIN
    assert zone_of(st, pilot) is Zone.TRASH


# ---------------------------------------------------------------------------------------------
# EB01-069 Beside Pain


@pytest.mark.card("EB01-069")
def test_eb01_069_friendly_blocker_gets_ap_plus_2_this_turn() -> None:
    sc = Scenario()
    unit = sc.add(0, GG_2, pilot="EB01-069")
    blocker = sc.add(0, S_GUNDAM)
    enemy_blocker = sc.add(1, GAPLANT)
    sc.shields(1, ZAKU, ZAKU)
    st = sc.start()
    attack(st, unit)
    assert ap(st, blocker) == 6
    assert ap(st, enemy_blocker) == 3
    block(st, None)
    pass_all(st)
    assert ap(st, blocker) == 6
    to_next_turn(st)
    assert ap(st, blocker) == 4


@pytest.mark.card("EB01-069")
def test_eb01_069_no_friendly_blocker_no_effect() -> None:
    sc = Scenario()
    unit = sc.add(0, GG_2, pilot="EB01-069")
    other = sc.add(0, GG_3)
    sc.shields(1, ZAKU, ZAKU)
    st = sc.start()
    attack(st, unit)
    pass_all(st)
    assert ap(st, other) == 4
    assert ap(st, unit) == 4


# ---------------------------------------------------------------------------------------------
# EB01-070 Daryl Lorenz


def _daryl_defending() -> tuple[GameState, int, int, list[int]]:
    """Opponent's turn: player 1 attacks player 0 whose Psycho Zaku is linked with Daryl."""
    sc = Scenario(active=1)
    zaku = sc.add(0, PSYCHO_ZAKU, pilot=DARYL)
    res = sc.resources(0, 4)
    sc.add(0, "EB01-083", Zone.HAND)  # an 【Action】 Command keeps player 0 deciding
    attacker = sc.add(1, ZAKU)
    sc.shields(0, ZAKU, ZAKU)
    st = sc.start()
    attack(st, attacker)
    assert _pending(st) is DecisionKind.ACTION_STEP and _decider(st) == 0
    return st, zaku, attacker, res


@pytest.mark.card("EB01-070")
@pytest.mark.rule("13-2-2-1", "13-2-12-1", "13-2-13-1")
def test_eb01_070_opponents_turn_ap_plus_1_during_battle() -> None:
    st, zaku, _, res = _daryl_defending()
    activate(st, zaku)
    assert sum(st.cards[u].rested for u in res) == 1
    select(st, zaku)
    assert ap(st, zaku) == 7
    assert _pending(st) is DecisionKind.ACTION_STEP and _decider(st) == 0
    assert not has_action(st, A.ACTIVATE, zaku)
    pass_all(st)
    assert ap(st, zaku) == 6


@pytest.mark.card("EB01-070")
@pytest.mark.ruling("EB01-070:Q326")
def test_eb01_070_can_choose_an_enemy_unit() -> None:
    st, zaku, attacker, _ = _daryl_defending()
    activate(st, zaku)
    assert _select_options(st) == {zaku, attacker}
    select(st, attacker)
    assert ap(st, attacker) == 3


@pytest.mark.card("EB01-070")
def test_eb01_070_on_your_turn_the_cost_is_paid_for_nothing() -> None:
    sc = Scenario()
    zaku = sc.add(0, PSYCHO_ZAKU, pilot=DARYL)
    res = sc.resources(0, 4)
    sc.add(0, "EB01-083", Zone.HAND)
    st = sc.start(Step.END_ACTION)
    assert _pending(st) is DecisionKind.ACTION_STEP and _decider(st) == 0
    activate(st, zaku)
    assert _pending(st) is not DecisionKind.SELECT
    assert sum(st.cards[u].rested for u in res) == 1
    assert ap(st, zaku) == 6


@pytest.mark.card("EB01-070")
def test_eb01_070_needs_link() -> None:
    sc = Scenario(active=1)
    unit = sc.add(0, GG_2, pilot=DARYL)
    sc.resources(0, 4)
    sc.add(0, "EB01-083", Zone.HAND)
    attacker = sc.add(1, ZAKU)
    sc.shields(0, ZAKU)
    st = sc.start()
    attack(st, attacker)
    assert _pending(st) is DecisionKind.ACTION_STEP and _decider(st) == 0
    assert not has_action(st, A.ACTIVATE, unit)


@pytest.mark.card("EB01-070")
@pytest.mark.rule("8-6-1", "7-6-6-1")
@pytest.mark.xfail(
    strict=True,
    reason="ENGINE: a 'during this battle' lasting effect created outside a battle never expires",
)
def test_eb01_070_outside_a_battle_the_bonus_does_not_outlive_the_turn() -> None:
    sc = Scenario(active=1)
    zaku = sc.add(0, PSYCHO_ZAKU, pilot=DARYL)
    sc.resources(0, 4)
    sc.add(0, "EB01-083", Zone.HAND)
    st = _opponent_end_action(sc)
    activate(st, zaku)  # the only Unit in play is chosen
    assert ap(st, zaku) == 7
    to_next_turn(st)
    assert st.active == 0
    assert ap(st, zaku) == 6


# ---------------------------------------------------------------------------------------------
# EB01-071 Ittou Tsurugi


@pytest.mark.card("EB01-071")
@pytest.mark.rule("13-2-12-1")
def test_eb01_071_ap_plus_1_during_link() -> None:
    sc = Scenario()
    linked = sc.add(0, S_GUNDAM, pilot=ITTOU)
    unlinked = sc.add(0, GG_2, pilot=ITTOU)
    st = sc.start()
    assert ap(st, linked) == 4 + 2 + 1
    assert ap(st, unlinked) == 3 + 2


# ---------------------------------------------------------------------------------------------
# EB01-072 Yuu Kajima


@pytest.mark.card("EB01-072")
def test_eb01_072_rests_friendly_blocker_and_enemy_lv4_or_lower() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    unit = sc.add(0, GG_2)
    blocker = sc.add(0, S_GUNDAM)
    enemy = sc.add(1, GELGOOG)
    too_high = sc.add(1, AGRISSA)
    pilot = sc.add(0, "EB01-072", Zone.HAND)
    st = sc.start()
    play(st, pilot, onto=unit)
    assert st.cards[blocker].rested
    assert st.cards[enemy].rested
    assert not st.cards[too_high].rested


@pytest.mark.card("EB01-072")
@pytest.mark.ruling("EB01-072:Q327")
def test_eb01_072_needs_both_targets() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    unit = sc.add(0, GG_2)
    sc.add(0, S_GUNDAM, rested=True)  # not active: cannot be chosen
    enemy = sc.add(1, GELGOOG)
    pilot = sc.add(0, "EB01-072", Zone.HAND)
    st = sc.start()
    play(st, pilot, onto=unit)
    assert not st.cards[enemy].rested


# ---------------------------------------------------------------------------------------------
# EB01-073 Character Requests


def _character_requests(friendly_rested: int, enemy_rested: int) -> tuple[GameState, int]:
    sc = Scenario()
    sc.resources(0, 6)
    command = sc.add(0, "EB01-073", Zone.HAND)
    for _ in range(friendly_rested):
        sc.add(0, GG_2, rested=True)
    for _ in range(enemy_rested):
        sc.add(1, ZAKU, rested=True)
    sc.add(1, ZAKU)
    return sc.start(), command


@pytest.mark.card("EB01-073")
@pytest.mark.ruling("EB01-073:Q328")
def test_eb01_073_draws_2_with_6_rested_units_on_either_side() -> None:
    st, command = _character_requests(3, 3)
    hand = len(st.zones[0][Zone.HAND])
    play(st, command)
    assert zone_of(st, command) is Zone.TRASH
    assert len(st.zones[0][Zone.HAND]) == hand - 1 + 2


@pytest.mark.card("EB01-073")
def test_eb01_073_no_draw_with_5_rested_units() -> None:
    st, command = _character_requests(3, 2)
    hand = len(st.zones[0][Zone.HAND])
    play(st, command)
    assert len(st.zones[0][Zone.HAND]) == hand - 1


@pytest.mark.card("EB01-073")
@pytest.mark.rule("13-2-5-1")
def test_eb01_073_burst_draws_1() -> None:
    sc = Scenario()
    attacker = sc.add(0, ZAKU)
    (shield,) = sc.shields(1, "EB01-073")
    sc.shields(1, ZAKU)
    st = sc.start()
    hand = len(st.zones[1][Zone.HAND])
    attack(st, attacker)
    pass_all(st)
    yes(st)
    assert len(st.zones[1][Zone.HAND]) == hand + 1
    assert zone_of(st, shield) is Zone.TRASH


# ---------------------------------------------------------------------------------------------
# EB01-074 Eternal Road


@pytest.mark.card("EB01-074")
@pytest.mark.ruling("EB01-074:Q329")
@pytest.mark.rule("5-20-1")
def test_eb01_074_rest_friendly_gg_then_opponent_rests_one_of_theirs() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    command = sc.add(0, "EB01-074", Zone.HAND)
    mine = sc.add(0, GG_2)
    sc.add(0, GG_3, rested=True)
    zaku = sc.add(1, ZAKU)
    goohn = sc.add(1, GOOHN)
    sc.add(1, REZEL, rested=True)
    st = sc.start()
    play(st, command)
    assert st.cards[mine].rested
    assert _pending(st) is DecisionKind.SELECT and _decider(st) == 1
    assert _select_options(st) == {zaku, goohn}
    select(st, goohn)
    assert st.cards[goohn].rested
    assert not st.cards[zaku].rested


@pytest.mark.card("EB01-074")
@pytest.mark.rule("10-1-8-1-1")
def test_eb01_074_needs_an_active_friendly_gg_unit() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    command = sc.add(0, "EB01-074", Zone.HAND)
    sc.add(0, GG_2, rested=True)
    sc.add(0, ZAKU)
    sc.add(1, ZAKU)
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, command)


@pytest.mark.card("EB01-074")
@pytest.mark.rule("13-2-4-1")
def test_eb01_074_action_timing_in_opponents_turn() -> None:
    sc = Scenario(active=1)
    sc.resources(0, 3)
    command = sc.add(0, "EB01-074", Zone.HAND)
    sc.add(0, "EB01-083", Zone.HAND)  # keeps player 0 deciding in this action step
    sc.resources(0, 1)
    mine = sc.add(0, GG_2)
    theirs = sc.add(1, ZAKU)
    st = _opponent_end_action(sc)
    play(st, command)
    assert _pending(st) is DecisionKind.ACTION_STEP and st.active == 1
    assert st.cards[mine].rested
    assert st.cards[theirs].rested


@pytest.mark.card("EB01-074")
@pytest.mark.rule("13-2-5-1")
def test_eb01_074_burst_rests_enemy_unit_with_3_or_less_hp() -> None:
    sc = Scenario()
    attacker = sc.add(0, ZAKU)
    small = sc.add(0, GOOHN)
    big = sc.add(0, AGRISSA)
    sc.shields(1, "EB01-074", ZAKU)
    st = sc.start()
    attack(st, attacker)
    pass_all(st)
    yes(st)
    assert _select_options(st) == {attacker, small}
    select(st, small)
    assert st.cards[small].rested
    assert not st.cards[big].rested


# ---------------------------------------------------------------------------------------------
# EB01-075 Fierce Enemy Assault


@pytest.mark.card("EB01-075")
def test_eb01_075_rests_up_to_2_enemy_units_with_2_or_less_hp() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    command = sc.add(0, "EB01-075", Zone.HAND)
    zaku = sc.add(1, ZAKU)
    goohn = sc.add(1, GOOHN)
    rezel = sc.add(1, REZEL)
    st = sc.start()
    play(st, command)
    assert _select_options(st) == {zaku, goohn}
    select(st, zaku, goohn)
    assert st.cards[zaku].rested and st.cards[goohn].rested
    assert not st.cards[rezel].rested


@pytest.mark.card("EB01-075")
@pytest.mark.rule("10-1-8-1-1")
def test_eb01_075_needs_a_target() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    command = sc.add(0, "EB01-075", Zone.HAND)
    sc.add(1, REZEL)
    sc.add(0, ZAKU)
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, command)


# ---------------------------------------------------------------------------------------------
# EB01-076 Gerbera Straight


@pytest.mark.card("EB01-076")
@pytest.mark.rule("5-6-3")
def test_eb01_076_friendly_gg_unit_recovers_3() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    command = sc.add(0, "EB01-076", Zone.HAND)
    unit = sc.add(0, CASVAL, damage=3)
    other = sc.add(0, ZAKU, damage=1)
    st = sc.start()
    play(st, command)
    assert st.cards[unit].damage == 0
    assert st.cards[other].damage == 1


@pytest.mark.card("EB01-076")
@pytest.mark.rule("3-4-6-2")
def test_eb01_076_can_be_paired_as_a_pilot_without_gg_unit() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    command = sc.add(0, "EB01-076", Zone.HAND)
    unit = sc.add(0, ZAKU)
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, command)
    play(st, command, onto=unit)
    assert zone_of(st, command) is Zone.PAIRED
    assert ap(st, unit) == 3


# ---------------------------------------------------------------------------------------------
# EB01-077 Master League Begins


@pytest.mark.card("EB01-077")
@pytest.mark.rule("13-2-4-1")
def test_eb01_077_redirects_the_attacking_enemy_to_rested_gg_unit() -> None:
    sc = Scenario(active=1)
    sc.resources(0, 3)
    command = sc.add(0, "EB01-077", Zone.HAND)
    mine = sc.add(0, GG_2, rested=True)
    attacker = sc.add(1, ZAKU)
    (shield,) = sc.shields(0, ZAKU)
    st = sc.start()
    attack(st, attacker)
    assert _pending(st) is DecisionKind.ACTION_STEP and _decider(st) == 0
    play(st, command)
    pass_all(st)
    assert zone_of(st, shield) is Zone.SHIELD
    assert zone_of(st, attacker) is Zone.TRASH
    assert zone_of(st, mine) is Zone.TRASH


@pytest.mark.card("EB01-077")
def test_eb01_077_does_not_redirect_a_friendly_attacker() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    command = sc.add(0, "EB01-077", Zone.HAND)
    mine = sc.add(0, GG_2, rested=True)
    attacker = sc.add(0, ZAKU)
    sc.add(1, GOOHN)
    (shield,) = sc.shields(1, ZAKU)
    sc.shields(1, ZAKU)
    st = sc.start()
    attack(st, attacker)
    assert _pending(st) is DecisionKind.ACTION_STEP and _decider(st) == 0
    play(st, command)
    pass_all(st)
    assert zone_of(st, shield) is Zone.TRASH
    assert st.cards[mine].damage == 0
    assert zone_of(st, attacker) is Zone.BATTLE


@pytest.mark.card("EB01-077")
@pytest.mark.rule("10-1-8-1-1")
def test_eb01_077_needs_a_rested_friendly_gg_unit() -> None:
    sc = Scenario(active=1)
    sc.resources(0, 3)
    command = sc.add(0, "EB01-077", Zone.HAND)
    sc.add(0, "EB01-083", Zone.HAND)  # another 【Action】 so player 0 gets an action decision
    sc.add(0, GG_2)
    attacker = sc.add(1, ZAKU)
    sc.shields(0, ZAKU)
    st = sc.start()
    attack(st, attacker)
    assert _pending(st) is DecisionKind.ACTION_STEP and _decider(st) == 0
    assert not has_action(st, A.PLAY_COMMAND, command)


@pytest.mark.rule("10-1-8-1-1")
@pytest.mark.parametrize(
    ("number", "unit"),
    [
        pytest.param("EB01-079", ZAKU, marks=pytest.mark.card("EB01-079")),
        pytest.param("EB01-080", ZAKU, marks=pytest.mark.card("EB01-080")),
        pytest.param("EB01-084", GG_2, marks=pytest.mark.card("EB01-084")),
    ],
)
def test_command_needs_a_target_to_be_played(number: str, unit: str) -> None:
    sc = Scenario()
    sc.resources(0, 4)
    command = sc.add(0, number, Zone.HAND)
    sc.add(0, unit)
    sc.add(1, ZAKU)
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, command)


# ---------------------------------------------------------------------------------------------
# EB01-078 Premium Unit Assembly


def _premium(my_top: str, their_top: str) -> tuple[GameState, int, int]:
    sc = Scenario()
    sc.resources(0, 1)
    command = sc.add(0, "EB01-078", Zone.HAND)
    sc.deck(0, my_top)
    sc.deck(1, their_top)
    st = sc.start()
    mine, theirs = st.zones[0][Zone.DECK][0], st.zones[1][Zone.DECK][0]
    play(st, command)
    return st, mine, theirs


@pytest.mark.card("EB01-078")
@pytest.mark.ruling("EB01-078:Q331")
def test_eb01_078_each_player_may_take_a_unit_active_player_first() -> None:
    st, mine, theirs = _premium(GG_2, ZAKU)
    assert _pending(st) is DecisionKind.YES_NO and _decider(st) == 0
    yes(st)
    assert zone_of(st, mine) is Zone.HAND
    assert st.cards[mine].known == 0b11
    assert _pending(st) is DecisionKind.YES_NO and _decider(st) == 1
    no(st)
    assert _pending(st) is DecisionKind.ARRANGE and _decider(st) == 1
    act(st, A.SELECT, 1)  # bottom
    assert st.zones[1][Zone.DECK][-1] == theirs


@pytest.mark.card("EB01-078")
def test_eb01_078_non_unit_goes_back_and_stays_private() -> None:
    st, mine, theirs = _premium(COMMAND_LV3, "EB01-076")
    assert _pending(st) is DecisionKind.ARRANGE and _decider(st) == 0
    act(st, A.SELECT, 0)  # top
    assert st.zones[0][Zone.DECK][0] == mine
    assert _pending(st) is DecisionKind.ARRANGE and _decider(st) == 1
    act(st, A.SELECT, 0)
    assert st.zones[1][Zone.DECK][0] == theirs
    assert st.cards[mine].known == 0b01  # only its owner looked at it
    assert st.cards[theirs].known == 0b10


@pytest.mark.card("EB01-078")
def test_eb01_078_opponent_may_add_their_unit() -> None:
    st, _, theirs = _premium(COMMAND_LV3, ZAKU)
    act(st, A.SELECT, 1)
    assert _decider(st) == 1
    yes(st)
    assert zone_of(st, theirs) is Zone.HAND
    assert st.cards[theirs].owner == 1


# ---------------------------------------------------------------------------------------------
# EB01-079 Modification


def _modification(enemy: str) -> tuple[GameState, int, int]:
    sc = Scenario()
    sc.resources(0, 3)
    command = sc.add(0, "EB01-079", Zone.HAND)
    unit = sc.add(0, GG_2)
    target = sc.add(1, enemy, rested=True)
    st = sc.start()
    play(st, command)
    attack(st, unit, target)
    pass_all(st)
    return st, unit, target


@pytest.mark.card("EB01-079")
def test_eb01_079_no_battle_damage_from_enemy_lv3_or_lower() -> None:
    st, unit, target = _modification(REZEL)
    assert zone_of(st, unit) is Zone.BATTLE
    assert st.cards[unit].damage == 0
    assert zone_of(st, target) is Zone.TRASH


@pytest.mark.card("EB01-079")
def test_eb01_079_lv4_enemy_still_deals_damage() -> None:
    st, unit, _ = _modification(GELGOOG)
    assert zone_of(st, unit) is Zone.TRASH


# ---------------------------------------------------------------------------------------------
# EB01-080 Sturm Faust


@pytest.mark.card("EB01-080")
@pytest.mark.ruling("EB01-080:Q332")
def test_eb01_080_gg_unit_may_attack_active_enemy_units() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    command = sc.add(0, "EB01-080", Zone.HAND)
    mine = sc.add(0, GG_2)
    enemy_gg = sc.add(1, GG_3)
    plain = sc.add(1, ZAKU)
    st = sc.start()
    assert not has_action(st, A.ATTACK, mine, plain)
    play(st, command)
    assert _select_options(st) == {mine, enemy_gg}
    select(st, mine)
    assert has_action(st, A.ATTACK, mine, plain)
    assert has_action(st, A.ATTACK, mine, enemy_gg)


@pytest.mark.card("EB01-080")
@pytest.mark.rule("3-4-6-2")
def test_eb01_080_can_be_paired_as_a_pilot() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    command = sc.add(0, "EB01-080", Zone.HAND)
    unit = sc.add(0, ZAKU)
    st = sc.start()
    play(st, command, onto=unit)
    assert zone_of(st, command) is Zone.PAIRED
    assert ap(st, unit) == 3


# ---------------------------------------------------------------------------------------------
# EB01-081 MAP Weapon


@pytest.mark.card("EB01-081")
def test_eb01_081_returns_up_to_2_enemy_units_with_2_or_less_hp() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    command = sc.add(0, "EB01-081", Zone.HAND)
    zaku = sc.add(1, ZAKU)
    goohn = sc.add(1, GOOHN)
    rezel = sc.add(1, REZEL)
    st = sc.start()
    play(st, command)
    assert _select_options(st) == {zaku, goohn}
    select(st, zaku, goohn)
    assert zone_of(st, zaku) is Zone.HAND and zone_of(st, goohn) is Zone.HAND
    assert zone_of(st, rezel) is Zone.BATTLE


@pytest.mark.card("EB01-081")
@pytest.mark.rule("10-1-8-1-1")
def test_eb01_081_needs_a_target() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    command = sc.add(0, "EB01-081", Zone.HAND)
    sc.add(1, REZEL)
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, command)


# ---------------------------------------------------------------------------------------------
# EB01-082 Warship Cruise


@pytest.mark.card("EB01-082")
def test_eb01_082_returns_enemy_unit_lv3_or_lower() -> None:
    sc = Scenario(active=1)
    sc.resources(0, 3)
    command = sc.add(0, "EB01-082", Zone.HAND)
    mine = sc.add(0, GOOHN)
    low = sc.add(1, ZAKU)
    high = sc.add(1, AGRISSA)
    st = _opponent_end_action(sc)
    play(st, command)
    assert zone_of(st, low) is Zone.HAND
    assert zone_of(st, high) is Zone.BATTLE
    assert zone_of(st, mine) is Zone.BATTLE


@pytest.mark.card("EB01-082")
@pytest.mark.rule("10-1-8-1-1")
def test_eb01_082_cannot_choose_your_own_unit() -> None:
    sc = Scenario(active=1)
    sc.resources(0, 3)
    command = sc.add(0, "EB01-082", Zone.HAND)
    sc.add(0, GOOHN)
    sc.add(1, AGRISSA)
    sc.add(0, "EB01-083", Zone.HAND)
    st = _opponent_end_action(sc)
    assert not has_action(st, A.PLAY_COMMAND, command)


@pytest.mark.card("EB01-082")
@pytest.mark.rule("13-2-5-1")
def test_eb01_082_burst_activates_its_action() -> None:
    sc = Scenario()
    attacker = sc.add(0, ZAKU)
    big = sc.add(0, AGRISSA)
    (shield,) = sc.shields(1, "EB01-082")
    sc.shields(1, ZAKU)
    st = sc.start()
    attack(st, attacker)
    pass_all(st)
    yes(st)
    assert zone_of(st, attacker) is Zone.HAND
    assert st.cards[attacker].owner == 0
    assert zone_of(st, big) is Zone.BATTLE
    assert zone_of(st, shield) is Zone.TRASH


# ---------------------------------------------------------------------------------------------
# EB01-083 SP Conversion Chips


@pytest.mark.card("EB01-083")
@pytest.mark.ruling("EB01-083:Q333")
def test_eb01_083_opponents_turn_any_unit_gets_ap_plus_3() -> None:
    sc = Scenario(active=1)
    sc.resources(0, 3)
    command, _ = sc.hand(0, "EB01-083", "EB01-083")
    sc.resources(0, 1)
    mine = sc.add(0, GG_2)
    theirs = sc.add(1, ZAKU)
    st = _opponent_end_action(sc)
    play(st, command)
    assert _select_options(st) == {mine, theirs}
    select(st, theirs)
    assert _pending(st) is DecisionKind.ACTION_STEP and st.active == 1
    assert ap(st, theirs) == 5
    assert ap(st, mine) == 3


@pytest.mark.card("EB01-083")
def test_eb01_083_your_turn_does_nothing() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    command = sc.add(0, "EB01-083", Zone.HAND)
    mine = sc.add(0, GG_2)
    st = sc.start(Step.END_ACTION)
    assert _decider(st) == 0
    play(st, command)
    assert zone_of(st, command) is Zone.TRASH
    assert ap(st, mine) == 3


# ---------------------------------------------------------------------------------------------
# EB01-084 30cm Cannon (APFSDS Round)


@pytest.mark.card("EB01-084")
def test_eb01_084_sets_blocker_active_but_it_cannot_attack() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    command = sc.add(0, "EB01-084", Zone.HAND)
    blocker = sc.add(0, S_GUNDAM, rested=True)
    st = sc.start()
    play(st, command)
    assert not st.cards[blocker].rested
    assert not has_action(st, A.ATTACK, blocker)
    to_next_turn(st)
    to_next_turn(st)
    assert has_action(st, A.ATTACK, blocker, PLAYER_TARGET)


@pytest.mark.card("EB01-084")
@pytest.mark.ruling("EB01-084:Q334")
def test_eb01_084_can_choose_an_enemy_blocker() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    command = sc.add(0, "EB01-084", Zone.HAND)
    mine = sc.add(0, S_GUNDAM, rested=True)
    theirs = sc.add(1, GAPLANT, rested=True)
    st = sc.start()
    play(st, command)
    assert _select_options(st) == {mine, theirs}
    select(st, theirs)
    assert not st.cards[theirs].rested
    assert st.cards[mine].rested


# ---------------------------------------------------------------------------------------------
# Bases: EB01-085 .. EB01-090


def _base_burst(number: str, *, p0_units: tuple[str, ...] = ()) -> tuple[GameState, int, int]:
    """Player 0 attacks; player 1's top Shield is the Base: its 【Burst】 deploys it and its
    【Deploy】 adds player 1's next Shield to their hand."""
    sc = Scenario()
    attacker = sc.add(0, ZAKU)
    for n in p0_units:
        sc.add(0, n)
    base, nxt = sc.shields(1, number, ZAKU)
    sc.shields(1, ZAKU)
    st = sc.start()
    attack(st, attacker)
    pass_all(st)
    assert _pending(st) is DecisionKind.BURST
    yes(st)
    assert zone_of(st, base) is Zone.BASE
    assert zone_of(st, nxt) is Zone.HAND
    return st, attacker, base


@pytest.mark.rule("13-2-5-1", "13-2-6-1")
@pytest.mark.parametrize(
    "number",
    [
        pytest.param("EB01-086", marks=pytest.mark.card("EB01-086")),
        pytest.param("EB01-087", marks=pytest.mark.card("EB01-087")),
        pytest.param("EB01-088", marks=pytest.mark.card("EB01-088")),
    ],
)
def test_base_burst_deploys_and_adds_a_shield(number: str) -> None:
    _base_burst(number)


def _play_base(
    number: str, resources: int, *units: tuple[int, str, bool]
) -> tuple[GameState, int, list[int], list[int]]:
    sc = Scenario()
    sc.resources(0, resources)
    base = sc.add(0, number, Zone.HAND)
    uids = [sc.add(p, n, rested=r) for p, n, r in units]
    shields = sc.shields(0, ZAKU, ZAKU)
    st = sc.start()
    play(st, base)
    assert zone_of(st, base) is Zone.BASE
    return st, base, uids, shields


@pytest.mark.card("EB01-085")
@pytest.mark.rule("5-20-2")
def test_eb01_085_shield_to_hand_then_rest_blue_gg_and_enemy() -> None:
    st, _, (blue, enemy), shields = _play_base(
        "EB01-085", 5, (0, BLUE_GG_3, False), (1, ZAKU, False)
    )
    assert zone_of(st, shields[0]) is Zone.HAND
    assert _pending(st) is DecisionKind.YES_NO
    yes(st)
    assert st.cards[blue].rested and st.cards[enemy].rested


@pytest.mark.card("EB01-085")
def test_eb01_085_resting_is_optional() -> None:
    st, _, (blue, enemy), _ = _play_base("EB01-085", 5, (0, BLUE_GG_3, False), (1, ZAKU, False))
    no(st)
    assert not st.cards[blue].rested and not st.cards[enemy].rested


@pytest.mark.card("EB01-085")
def test_eb01_085_needs_both_units() -> None:
    st, _, (white, enemy), shields = _play_base("EB01-085", 5, (0, GG_2, False), (1, ZAKU, False))
    assert zone_of(st, shields[0]) is Zone.HAND
    assert _pending(st) is DecisionKind.MAIN
    assert not st.cards[white].rested and not st.cards[enemy].rested
    st2, _, (blue,), _ = _play_base("EB01-085", 5, (0, BLUE_GG_3, False))
    assert _pending(st2) is DecisionKind.MAIN
    assert not st2.cards[blue].rested


@pytest.mark.card("EB01-085")
@pytest.mark.rule("13-2-5-1")
def test_eb01_085_burst_deploys_it() -> None:
    _base_burst("EB01-085")


@pytest.mark.card("EB01-086")
@pytest.mark.rule("13-2-11-1", "13-2-13-1", "13-1-1-2")
def test_eb01_086_first_gg_link_each_turn_gains_repair_2() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    sc.base(0, "EB01-086")
    tornado_a = sc.add(0, BLUE_GG_3)
    tornado_b = sc.add(0, BLUE_GG_3)
    first, second = sc.hand(0, ITTOU, ITTOU)
    st = sc.start()
    play(st, first, onto=tornado_a)
    assert keywords(st, tornado_a).get("Repair") == 2
    play(st, second, onto=tornado_b)
    assert V.is_linked(V.derived(st), tornado_b)
    assert "Repair" not in keywords(st, tornado_b)
    to_next_turn(st)
    assert "Repair" not in keywords(st, tornado_a)


@pytest.mark.card("EB01-086")
def test_eb01_086_pairing_without_link_does_nothing() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    sc.base(0, "EB01-086")
    tornado = sc.add(0, BLUE_GG_3)
    pilot = sc.add(0, RIDDHE, Zone.HAND)
    st = sc.start()
    play(st, pilot, onto=tornado)
    assert not V.is_linked(V.derived(st), tornado)
    assert "Repair" not in keywords(st, tornado)


@pytest.mark.card("EB01-086")
@pytest.mark.rule("13-2-6-1")
def test_eb01_086_deploy_adds_a_shield() -> None:
    st, _, _, shields = _play_base("EB01-086", 4)
    assert zone_of(st, shields[0]) is Zone.HAND
    assert zone_of(st, shields[1]) is Zone.SHIELD


@pytest.mark.card("EB01-087")
@pytest.mark.rule("13-2-13-1")
def test_eb01_087_green_gg_destroys_enemy_friendly_recovers_2_once_per_turn() -> None:
    sc = Scenario()
    sc.base(0, "EB01-087")
    green = sc.add(0, GREEN_GG_3)
    green2 = sc.add(0, "EB01-026")
    hurt = sc.add(0, GG_3, damage=2)
    first = sc.add(1, GOOHN, rested=True)
    second = sc.add(1, GOOHN, rested=True)
    st = sc.start()
    attack(st, green, first)
    pass_all(st)
    assert zone_of(st, first) is Zone.TRASH
    assert _pending(st) is DecisionKind.SELECT
    assert _select_options(st) == {green, green2, hurt}
    select(st, hurt)
    assert st.cards[hurt].damage == 0
    attack(st, green2, second)
    pass_all(st)
    assert zone_of(st, second) is Zone.TRASH
    assert _pending(st) is DecisionKind.MAIN
    assert st.cards[green2].damage == 1


@pytest.mark.card("EB01-087")
def test_eb01_087_white_unit_does_not_trigger() -> None:
    sc = Scenario()
    sc.base(0, "EB01-087")
    white = sc.add(0, GG_2)
    hurt = sc.add(0, GG_3, damage=2)
    enemy = sc.add(1, GOOHN, rested=True)
    st = sc.start()
    attack(st, white, enemy)
    pass_all(st)
    assert zone_of(st, enemy) is Zone.TRASH
    assert _pending(st) is DecisionKind.MAIN
    assert st.cards[hurt].damage == 2


@pytest.mark.card("EB01-087")
def test_eb01_087_not_during_opponents_turn() -> None:
    sc = Scenario(active=1)
    sc.base(0, "EB01-087")
    green = sc.add(0, GREEN_GG_3, rested=True)
    hurt = sc.add(0, GG_3, damage=2)
    attacker = sc.add(1, GOOHN)
    st = sc.start()
    attack(st, attacker, green)
    pass_all(st)
    assert zone_of(st, attacker) is Zone.TRASH
    assert st.cards[hurt].damage == 2
    assert _pending(st) is DecisionKind.MAIN and st.active == 1


@pytest.mark.card("EB01-087")
@pytest.mark.rule("13-2-6-1")
def test_eb01_087_deploy_adds_a_shield() -> None:
    st, _, _, shields = _play_base("EB01-087", 3)
    assert zone_of(st, shields[0]) is Zone.HAND


@pytest.mark.card("EB01-088")
def test_eb01_088_lv3_gg_units_get_ap_plus_1_in_opponents_turn() -> None:
    for active, bonus in ((0, 0), (1, 1)):
        sc = Scenario(active=active)
        sc.base(0, "EB01-088")
        lv3 = sc.add(0, GG_3)
        lv2 = sc.add(0, GG_2)
        other_lv3 = sc.add(0, REZEL)
        enemy_lv3 = sc.add(1, GG_3)
        st = sc.start()
        assert ap(st, lv3) == 4 + bonus
        assert ap(st, lv2) == 3
        assert ap(st, other_lv3) == 4
        assert ap(st, enemy_lv3) == 4


@pytest.mark.card("EB01-088")
@pytest.mark.rule("13-2-6-1")
def test_eb01_088_deploy_adds_a_shield() -> None:
    st, _, _, shields = _play_base("EB01-088", 3)
    assert zone_of(st, shields[0]) is Zone.HAND


@pytest.mark.card("EB01-089")
@pytest.mark.rule("5-20-2")
def test_eb01_089_sets_rested_white_gg_active_but_it_cannot_attack() -> None:
    st, _, (white, blue), shields = _play_base("EB01-089", 3, (0, GG_2, True), (0, BLUE_GG_3, True))
    assert zone_of(st, shields[0]) is Zone.HAND
    assert not st.cards[white].rested
    assert st.cards[blue].rested
    assert not has_action(st, A.ATTACK, white)


@pytest.mark.card("EB01-089")
@pytest.mark.rule("13-2-5-1")
def test_eb01_089_burst_deploys_it() -> None:
    _base_burst("EB01-089")


@pytest.mark.card("EB01-090")
def test_eb01_090_your_turn_returns_enemy_unit_with_2_or_less_hp() -> None:
    st, _, (mine, small, big), shields = _play_base(
        "EB01-090", 2, (0, GOOHN, False), (1, ZAKU, False), (1, REZEL, False)
    )
    assert zone_of(st, shields[0]) is Zone.HAND
    assert zone_of(st, small) is Zone.HAND
    assert zone_of(st, big) is Zone.BATTLE
    assert zone_of(st, mine) is Zone.BATTLE


@pytest.mark.card("EB01-090")
@pytest.mark.rule("13-2-5-1")
def test_eb01_090_burst_in_opponents_turn_returns_nothing() -> None:
    st, attacker, _ = _base_burst("EB01-090", p0_units=(GOOHN,))
    assert zone_of(st, attacker) is Zone.BATTLE
    assert all(zone_of(st, u) is Zone.BATTLE for u in st.zones[0][Zone.BATTLE])
    assert len(st.zones[0][Zone.BATTLE]) == 2
