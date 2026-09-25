"""Card behaviour and ruling tests for GD05-001..GD05-070 (work package WP-GD05-A)."""

from __future__ import annotations

import pytest

from gcg_sim.cards.model import CardDef, CardType
from gcg_sim.effects import dsl as d
from gcg_sim.engine import view as V
from gcg_sim.engine.game import advance, apply
from gcg_sim.engine.interp import deploy_cards
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
VANILLA = "GD01-060"  # Zaku Mariner: Lv2, cost 1, AP2 HP2, no effects
BLOCKER = "GD01-072"  # Launcher Strike Gundam: AP3 HP4 <Blocker>
RIDDHE = "GD01-089"  # Pilot, AP+1 HP+1, no trigger
KIRA = "GD05-081"  # (Orb) Pilot
AMURO = "GD05-085"  # green (Earth Federation)(Londo Bell)(Newtype) Pilot
STELLAR = "GD05-090"  # (Phantom Pain)(Biological CPU) Pilot
STING = "GD05-091"  # (Phantom Pain)(Biological CPU) Pilot
AUEL = "GD05-092"  # (Phantom Pain) Pilot
SCIROCCO = "GD03-084"  # blue (Titans)(Newtype) Pilot
YAZAN = "GD03-086"  # (Titans) Pilot
ASEMU = "GD03-088"  # green (Earth Federation) Pilot
MIKAZUKI = "ST05-010"  # (Tekkadan) Pilot
HEERO_G_TEAM = "GD05-098"  # (G Team) Pilot, link for Wing Gundam Zero (EW)
SHINN = "ST09-008"  # Pilot card "Shinn Asuka"
JEGAN = "GD05-027"  # vanilla (Londo Bell) Unit, Lv2
LFRITH = "GD01-086"  # Gundam Lfrith, Lv3 <Blocker>
GUAIZ = "GD03-038"  # Lv4 (ZAFT) Unit: "During your turn, when this Unit is rested by an effect ..."
GRAZE = "ST05-009"  # vanilla (Gjallarhorn) Unit, Lv2
DARKNESS_FINGER = "GD05-110"  # (Special Move) Command: 【Main】/【Action】deal 2 to 1 enemy Unit
UNDYING = "GD02-109"  # Command with 【Pilot】: 【Main】/【Action】deal 1 damage to 1 enemy Unit
BECOME_A_SHIELD = "GD05-117"  # Command: deal 1 damage to 1 of your Units and 1 enemy Unit
IRON_FISTED = "GD01-119"  # Command: 【Main】/【Action】an enemy Unit Lv.4 or lower gets AP-2
BRIDGE_CREW = "GD03-105"  # Command: a friendly Unit may attack active unpaired enemy Units
SIMULTANEOUS_FIRE = "ST02-012"  # Command: 1 of your Units gains <Breach 3> this turn
AWAKENED_POWER = (
    "GD02-110"  # Command: pay the cost of a Lv.5 or lower Unit card in trash to deploy it
)
VETERANS_PRIDE = "GD05-116"  # (Neo Zeon) Command: destroy an enemy Unit that is Lv.2 or lower


def hand_count(st: GameState, player: int = 0) -> int:
    return len(st.zones[player][Zone.HAND])


def resolve_orders(st: GameState) -> None:
    while st.pending is not None and st.pending.kind is DecisionKind.ORDER_TRIGGER:
        order(st)


def select_options(st: GameState) -> set[int]:
    return {o.a for o in options(st) if o.kind is A.SELECT}


def active_resources(st: GameState, player: int = 0) -> int:
    return sum(1 for u in st.zones[player][Zone.RESOURCE_AREA] if not st.cards[u].rested)


def ex_resources(st: GameState, player: int = 0) -> int:
    return sum(
        1
        for u in st.zones[player][Zone.RESOURCE_AREA]
        if V.cdef(st, u).card_type is CardType.EX_RESOURCE
    )


def destroy_own_unit_with_gyunei(st: GameState, gyunei: int, victim: int) -> None:
    """GD05-057's 【Activate･Main】 destroys one of your other Units (a 'destroy' effect of a
    (Neo Zeon) card)."""
    activate(st, gyunei)
    if st.pending is not None and st.pending.kind is DecisionKind.SELECT:
        select(st, victim)


@pytest.fixture
def force_link(monkeypatch: pytest.MonkeyPatch) -> set[str]:
    """Treat the named Units as linked with any paired card. The partner Pilots/【Pilot】
    Commands that satisfy these link conditions (e.g. GD05-112 [Sai Saici], GD05-113 [George
    de Sand], GD05-121 [Chibodee Crocket], GD05-097 Domon Kasshu) belong to another work
    package and are not implemented in this package's tree."""
    linked_units: set[str] = set()
    original = V.link_satisfied

    def patched(unit_def: CardDef, pilot_def: CardDef) -> bool:
        return unit_def.card_number in linked_units or original(unit_def, pilot_def)

    monkeypatch.setattr(V, "link_satisfied", patched)
    return linked_units


# ---------------------------------------------------------------------------------------------
# GD05-001 V2 Gundam


@pytest.mark.card("GD05-001")
def test_gd05_001_rest_two_units_to_set_itself_active_once_per_turn() -> None:
    sc = Scenario()
    v2 = sc.add(0, "GD05-001", rested=True)
    a = sc.add(0, VANILLA)
    b = sc.add(0, VANILLA)
    c = sc.add(0, VANILLA)
    st = sc.start()
    activate(st, v2)
    select(st, a, b)
    assert not st.cards[v2].rested
    assert st.cards[a].rested and st.cards[b].rested and not st.cards[c].rested
    assert not has_action(st, A.ACTIVATE, v2)


@pytest.mark.card("GD05-001")
def test_gd05_001_cannot_activate_with_fewer_than_two_active_units() -> None:
    sc = Scenario()
    v2 = sc.add(0, "GD05-001", rested=True)
    sc.add(0, VANILLA)
    st = sc.start()
    assert not has_action(st, A.ACTIVATE, v2)


@pytest.mark.card("GD05-001")
@pytest.mark.rule("13-1-1-1")
def test_gd05_001_repair_2_recovers_at_end_of_turn() -> None:
    sc = Scenario()
    v2 = sc.add(0, "GD05-001", damage=3)
    st = sc.start()
    assert keywords(st, v2).get("Repair") == 2
    to_next_turn(st)
    assert st.cards[v2].damage == 1


# ---------------------------------------------------------------------------------------------
# GD05-002 Strike Freedom Gundam


def _strike_freedom_deployed(
    *, enemy_shields: int = 1, enemy_base_damage: int | None = None
) -> tuple[GameState, int, int]:
    sc = Scenario()
    sc.resources(0, 8)
    ally = sc.add(0, VANILLA)
    sf = sc.add(0, "GD05-002", Zone.HAND)
    if enemy_shields:
        sc.shields(1, *([VANILLA] * enemy_shields))
    if enemy_base_damage is not None:
        sc.base(1, damage=enemy_base_damage)
    st = sc.start()
    play(st, sf)
    return st, sf, ally


@pytest.mark.card("GD05-002")
@pytest.mark.ruling("GD05-002:Q336")
def test_gd05_002_chosen_unit_draws_when_it_destroys_a_shield() -> None:
    st, sf, ally = _strike_freedom_deployed()
    assert select_options(st) == {ally, sf}
    select(st, ally, sf)
    before = hand_count(st)
    attack(st, ally)
    pass_all(st)
    assert hand_count(st) == before + 1


@pytest.mark.card("GD05-002")
@pytest.mark.ruling("GD05-002:Q336")
def test_gd05_002_chosen_unit_draws_when_it_destroys_the_enemy_base() -> None:
    st, _sf, ally = _strike_freedom_deployed(enemy_shields=1, enemy_base_damage=1)
    select(st, ally)
    before = hand_count(st)
    attack(st, ally)
    pass_all(st)
    assert st.zones[1][Zone.BASE] == []
    assert hand_count(st) == before + 1


@pytest.mark.card("GD05-002")
@pytest.mark.ruling("GD05-002:Q336")
def test_gd05_002_chosen_unit_draws_when_it_destroys_an_enemy_unit() -> None:
    sc = Scenario()
    sc.resources(0, 8)
    ally = sc.add(0, VANILLA)
    sf = sc.add(0, "GD05-002", Zone.HAND)
    victim = sc.add(1, "GD05-015", rested=True)  # AP1 HP2
    st = sc.start()
    play(st, sf)
    select(st, ally)
    before = hand_count(st)
    attack(st, ally, victim)
    pass_all(st)
    assert zone_of(st, victim) is Zone.TRASH
    assert zone_of(st, ally) is Zone.BATTLE
    assert hand_count(st) == before + 1


@pytest.mark.card("GD05-002")
@pytest.mark.ruling("GD05-002:Q337")
def test_gd05_002_draws_even_when_both_units_are_destroyed() -> None:
    sc = Scenario()
    sc.resources(0, 8)
    ally = sc.add(0, VANILLA)
    sf = sc.add(0, "GD05-002", Zone.HAND)
    victim = sc.add(1, VANILLA, rested=True)
    st = sc.start()
    play(st, sf)
    select(st, ally)
    before = hand_count(st)
    attack(st, ally, victim)
    pass_all(st)
    assert zone_of(st, ally) is Zone.TRASH and zone_of(st, victim) is Zone.TRASH
    assert hand_count(st) == before + 1


@pytest.mark.card("GD05-002")
def test_gd05_002_unchosen_unit_does_not_draw_and_effect_ends_with_the_turn() -> None:
    st, sf, ally = _strike_freedom_deployed(enemy_shields=3)
    select(st, sf)
    before = hand_count(st)
    attack(st, ally)
    pass_all(st)
    assert hand_count(st) == before
    to_next_turn(st)
    to_next_turn(st)
    before = hand_count(st)
    attack(st, sf)
    pass_all(st)
    assert hand_count(st) == before


def _paired_strike_freedom(hand: int) -> tuple[GameState, int, list[int]]:
    sc = Scenario()
    sf = sc.add(0, "GD05-002", pilot=KIRA)
    sc.hand(0, *([VANILLA] * hand))
    low_a = sc.add(1, VANILLA)
    low_b = sc.add(1, VANILLA, pilot=RIDDHE)
    high = sc.add(1, "GD05-005")  # Lv4
    st = sc.start()
    return st, sf, [low_a, low_b, high]


@pytest.mark.card("GD05-002")
@pytest.mark.ruling("GD05-002:Q338")
def test_gd05_002_attack_discard_two_returns_a_lowest_level_enemy_unit() -> None:
    st, sf, (low_a, low_b, high) = _paired_strike_freedom(hand=2)
    attack(st, sf)
    yes(st)
    assert select_options(st) == {low_a, low_b}
    select(st, low_a)
    assert hand_count(st) == 0
    assert st.zones[1][Zone.DECK][-1] == low_a
    assert zone_of(st, high) is Zone.BATTLE and zone_of(st, low_b) is Zone.BATTLE


@pytest.mark.card("GD05-002")
@pytest.mark.ruling("GD05-002:Q339")
def test_gd05_002_paired_unit_and_pilot_go_to_the_bottom_of_their_owners_deck() -> None:
    st, sf, (_low_a, low_b, _high) = _paired_strike_freedom(hand=2)
    pilot = st.cards[low_b].pair
    my_deck = list(st.zones[0][Zone.DECK])
    attack(st, sf)
    yes(st)
    select(st, low_b)
    assert set(st.zones[1][Zone.DECK][-2:]) == {low_b, pilot}
    assert st.zones[0][Zone.DECK] == my_deck


@pytest.mark.card("GD05-002")
def test_gd05_002_attack_effect_needs_two_cards_in_hand_and_a_pilot() -> None:
    st, sf, units = _paired_strike_freedom(hand=1)
    attack(st, sf)
    assert st.pending is not None and st.pending.kind is not DecisionKind.YES_NO
    assert all(zone_of(st, u) is Zone.BATTLE for u in units)

    sc = Scenario()
    unpaired = sc.add(0, "GD05-002")
    sc.hand(0, VANILLA, VANILLA)
    sc.add(1, VANILLA)
    sc.shields(1, VANILLA)
    st = sc.start()
    attack(st, unpaired)
    assert st.pending is not None and st.pending.kind is not DecisionKind.YES_NO
    assert hand_count(st) == 2


@pytest.mark.card("GD05-002")
def test_gd05_002_declining_the_discard_returns_nothing() -> None:
    st, sf, units = _paired_strike_freedom(hand=3)
    attack(st, sf)
    no(st)
    assert hand_count(st) == 3
    assert all(zone_of(st, u) is Zone.BATTLE for u in units)


# ---------------------------------------------------------------------------------------------
# GD05-003 Waldfeld's Murasame


@pytest.mark.card("GD05-003")
@pytest.mark.ruling("GD05-003:Q340")
def test_gd05_003_draws_when_destroyed_while_paired_with_an_orb_pilot() -> None:
    sc = Scenario()
    murasame = sc.add(0, "GD05-003", pilot=KIRA)
    gyunei = sc.add(0, "GD05-057")
    st = sc.start()
    before = hand_count(st)
    destroy_own_unit_with_gyunei(st, gyunei, murasame)
    assert zone_of(st, murasame) is Zone.TRASH
    assert hand_count(st) == before + 1


@pytest.mark.card("GD05-003")
def test_gd05_003_draws_with_another_orb_pilot_in_play() -> None:
    sc = Scenario()
    murasame = sc.add(0, "GD05-003")
    sc.add(0, VANILLA, pilot=KIRA)
    gyunei = sc.add(0, "GD05-057")
    st = sc.start()
    before = hand_count(st)
    destroy_own_unit_with_gyunei(st, gyunei, murasame)
    assert hand_count(st) == before + 1


@pytest.mark.card("GD05-003")
def test_gd05_003_no_draw_without_an_orb_pilot() -> None:
    sc = Scenario()
    murasame = sc.add(0, "GD05-003", pilot=RIDDHE)
    gyunei = sc.add(0, "GD05-057")
    st = sc.start()
    before = hand_count(st)
    destroy_own_unit_with_gyunei(st, gyunei, murasame)
    assert zone_of(st, murasame) is Zone.TRASH
    assert hand_count(st) == before


# ---------------------------------------------------------------------------------------------
# GD05-004 Akatsuki (Oowashi)


@pytest.mark.card("GD05-004")
def test_gd05_004_hand_cost_and_level_drop_per_orb_unit() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    sc.add(0, "GD05-015")
    sc.add(0, "GD05-015")
    akatsuki = sc.add(0, "GD05-004", Zone.HAND)
    st = sc.start()
    dv = V.derived(st)
    assert V.play_cost(st, dv, akatsuki) == 4
    assert V.play_level(st, dv, akatsuki) == 4
    play(st, akatsuki)
    assert zone_of(st, akatsuki) is Zone.BATTLE


@pytest.mark.card("GD05-004")
def test_gd05_004_no_discount_while_a_level_6_unit_is_in_play() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    sc.add(0, "GD05-015")
    sc.add(0, "GD05-015")
    sc.add(0, "GD05-001")  # Lv6
    akatsuki = sc.add(0, "GD05-004", Zone.HAND)
    st = sc.start()
    dv = V.derived(st)
    assert V.play_cost(st, dv, akatsuki) == 6
    assert not has_action(st, A.PLAY_UNIT, akatsuki)


@pytest.mark.card("GD05-004")
@pytest.mark.ruling("GD05-004:Q341")
@pytest.mark.rule("11-4-2")
def test_gd05_004_six_orb_units_make_it_free_and_force_battle_area_excess() -> None:
    sc = Scenario()
    orbs = [sc.add(0, "GD05-015") for _ in range(6)]
    akatsuki = sc.add(0, "GD05-004", Zone.HAND)
    st = sc.start()
    play(st, akatsuki)
    assert st.pending is not None and st.pending.kind is DecisionKind.EXCESS
    select(st, orbs[0])
    assert zone_of(st, akatsuki) is Zone.BATTLE
    assert zone_of(st, orbs[0]) is Zone.TRASH
    assert len(st.zones[0][Zone.BATTLE]) == 6


@pytest.mark.card("GD05-004")
def test_gd05_004_when_linked_returns_enemy_unit_lv4_or_lower() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    akatsuki = sc.add(0, "GD05-004")
    kira = sc.add(0, KIRA, Zone.HAND)
    small = sc.add(1, "GD05-005")  # Lv4
    big = sc.add(1, "GD05-001")  # Lv6
    st = sc.start()
    play(st, kira, onto=akatsuki)
    resolve_orders(st)
    assert zone_of(st, small) is Zone.HAND
    assert zone_of(st, big) is Zone.BATTLE


# ---------------------------------------------------------------------------------------------
# GD05-005 Strike Rouge (Ootori), GD05-013 Gun EZ, GD05-058 Shiden Custom (Ryusei-Go)


@pytest.mark.card("GD05-005", "GD05-013", "GD05-058")
@pytest.mark.rule("13-1-4-1")
@pytest.mark.parametrize("number", ["GD05-005", "GD05-013", "GD05-058"])
def test_blocker_units_can_block(number: str) -> None:
    sc = Scenario(active=1)
    attacker = sc.add(1, VANILLA)
    blocker = sc.add(0, number)
    (shield,) = sc.shields(0, VANILLA)
    st = sc.start()
    attack(st, attacker)
    assert st.pending is not None and st.pending.kind is DecisionKind.BLOCK
    block(st, blocker)
    assert zone_of(st, shield) is Zone.SHIELD
    assert zone_of(st, blocker) is Zone.TRASH or st.cards[blocker].damage == 2


# ---------------------------------------------------------------------------------------------
# GD05-006 Hashmal


def _ready_again(st: GameState, uid: int) -> None:
    """Set a rested Unit active as a set-active effect would (no such effect is implemented for
    a Unit without <Blocker> in this package's tree)."""
    st.cards[uid].rested = False
    st.touch()
    st.pending = None
    advance(st)


def _plumas(st: GameState, player: int = 0) -> list[int]:
    return [u for u in st.zones[player][Zone.BATTLE] if V.cdef(st, u).name == "Pluma"]


@pytest.mark.card("GD05-006")
@pytest.mark.ruling("GD05-006:Q342")
def test_gd05_006_destroying_a_shield_deploys_pluma_and_grants_repair() -> None:
    sc = Scenario()
    hashmal = sc.add(0, "GD05-006")
    sc.shields(1, VANILLA, VANILLA)
    st = sc.start()
    assert "Repair" not in keywords(st, hashmal)
    attack(st, hashmal)
    pass_all(st)
    (pluma,) = _plumas(st)
    assert V.cdef(st, pluma).traits == ("Calamity War",)
    assert (ap(st, pluma), hp(st, pluma)) == (2, 1)
    assert keywords(st, hashmal).get("Repair") == 1


@pytest.mark.card("GD05-006")
@pytest.mark.ruling("GD05-006:Q342")
def test_gd05_006_destroying_the_base_or_a_unit_deploys_pluma() -> None:
    sc = Scenario()
    hashmal = sc.add(0, "GD05-006")
    base = sc.base(1)
    st = sc.start()
    attack(st, hashmal)
    pass_all(st)
    assert zone_of(st, base) is not Zone.BASE
    assert len(_plumas(st)) == 1

    sc = Scenario()
    hashmal = sc.add(0, "GD05-006")
    victim = sc.add(1, VANILLA, rested=True)
    st = sc.start()
    attack(st, hashmal, victim)
    pass_all(st)
    assert zone_of(st, victim) is Zone.TRASH
    assert len(_plumas(st)) == 1


@pytest.mark.card("GD05-006")
@pytest.mark.ruling("GD05-006:Q343")
def test_gd05_006_deploys_pluma_even_when_both_units_are_destroyed() -> None:
    sc = Scenario()
    hashmal = sc.add(0, "GD05-006")
    victim = sc.add(1, "GD05-038", rested=True)  # AP6 HP4
    st = sc.start()
    attack(st, hashmal, victim)
    pass_all(st)
    assert zone_of(st, hashmal) is Zone.TRASH and zone_of(st, victim) is Zone.TRASH
    assert len(_plumas(st)) == 1


@pytest.mark.card("GD05-006")
def test_gd05_006_first_effect_is_once_per_turn_across_unit_and_shield_destruction() -> None:
    sc = Scenario()
    hashmal = sc.add(0, "GD05-006")
    victim = sc.add(1, VANILLA, rested=True)
    sc.shields(1, VANILLA, VANILLA)
    st = sc.start()
    attack(st, hashmal, victim)
    pass_all(st)
    assert len(_plumas(st)) == 1
    _ready_again(st, hashmal)
    attack(st, hashmal)
    pass_all(st)
    assert len(st.zones[1][Zone.SHIELD]) == 1
    assert len(_plumas(st)) == 1
    to_next_turn(st)
    to_next_turn(st)
    attack(st, hashmal)
    pass_all(st)
    assert len(_plumas(st)) == 2
    assert keywords(st, hashmal).get("Repair") == 2


@pytest.mark.card("GD05-006")
def test_gd05_006_no_token_during_the_opponents_turn() -> None:
    sc = Scenario(active=1)
    hashmal = sc.add(0, "GD05-006", rested=True)
    attacker = sc.add(1, VANILLA)
    st = sc.start()
    attack(st, attacker, hashmal)
    pass_all(st)
    assert zone_of(st, attacker) is Zone.TRASH
    assert _plumas(st) == []


# ---------------------------------------------------------------------------------------------
# GD05-007 Asshimar


@pytest.mark.card("GD05-007")
def test_gd05_007_during_link_gets_ap_and_repair() -> None:
    sc = Scenario()
    linked = sc.add(0, "GD05-007", pilot=YAZAN)
    unlinked = sc.add(0, "GD05-007", pilot=RIDDHE)
    st = sc.start()
    assert ap(st, linked) == 1 + 1 + 2
    assert keywords(st, linked).get("Repair") == 1
    assert ap(st, unlinked) == 1 + 1
    assert "Repair" not in keywords(st, unlinked)


# ---------------------------------------------------------------------------------------------
# GD05-008 Dijeh


@pytest.mark.card("GD05-008")
def test_gd05_008_costs_two_less_with_a_non_blue_newtype_pilot() -> None:
    sc = Scenario()
    sc.resources(0, 4, rested=3)
    sc.add(0, VANILLA, pilot=AMURO)
    dijeh = sc.add(0, "GD05-008", Zone.HAND)
    st = sc.start()
    assert V.play_cost(st, V.derived(st), dijeh) == 1
    play(st, dijeh)
    assert zone_of(st, dijeh) is Zone.BATTLE


@pytest.mark.card("GD05-008")
def test_gd05_008_blue_newtype_pilot_gives_no_discount() -> None:
    sc = Scenario()
    sc.resources(0, 4, rested=3)
    sc.add(0, VANILLA, pilot=SCIROCCO)
    dijeh = sc.add(0, "GD05-008", Zone.HAND)
    st = sc.start()
    assert V.play_cost(st, V.derived(st), dijeh) == 3
    assert not has_action(st, A.PLAY_UNIT, dijeh)


# ---------------------------------------------------------------------------------------------
# GD05-011 Calamity Gundam & Raider Gundam


@pytest.mark.card("GD05-011")
def test_gd05_011_rest_another_earth_alliance_unit_to_deal_2_to_rested_enemy() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    forbidden = sc.add(0, "GD05-012")
    card = sc.add(0, "GD05-011", Zone.HAND)
    victim = sc.add(1, VANILLA, rested=True)
    active_enemy = sc.add(1, VANILLA)
    st = sc.start()
    play(st, card)
    assert select_options(st) == {forbidden}
    select(st, forbidden)
    assert st.cards[forbidden].rested
    assert zone_of(st, victim) is Zone.TRASH
    assert st.cards[active_enemy].damage == 0


@pytest.mark.card("GD05-011")
def test_gd05_011_declining_deals_no_damage() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    forbidden = sc.add(0, "GD05-012")
    card = sc.add(0, "GD05-011", Zone.HAND)
    victim = sc.add(1, VANILLA, rested=True)
    st = sc.start()
    play(st, card)
    select(st)
    assert not st.cards[forbidden].rested
    assert st.cards[victim].damage == 0


# ---------------------------------------------------------------------------------------------
# GD05-012 Forbidden Gundam


@pytest.mark.card("GD05-012")
def test_gd05_012_when_linked_returns_rested_enemy_lv3_or_lower() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    forbidden = sc.add(0, "GD05-012")
    stellar = sc.add(0, STELLAR, Zone.HAND)
    rested_small = sc.add(1, VANILLA, rested=True)
    active_small = sc.add(1, VANILLA)
    rested_big = sc.add(1, "GD05-005", rested=True)  # Lv4
    st = sc.start()
    play(st, stellar, onto=forbidden)
    resolve_orders(st)
    assert zone_of(st, rested_small) is Zone.HAND
    assert zone_of(st, active_small) is Zone.BATTLE
    assert zone_of(st, rested_big) is Zone.BATTLE


# ---------------------------------------------------------------------------------------------
# GD05-015 M1 Astray Shrike


@pytest.mark.card("GD05-015")
def test_gd05_015_deploy_deals_1_to_a_rested_enemy_unit() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    m1 = sc.add(0, "GD05-015", Zone.HAND)
    rested = sc.add(1, VANILLA, rested=True)
    active = sc.add(1, VANILLA)
    st = sc.start()
    play(st, m1)
    assert st.cards[rested].damage == 1
    assert st.cards[active].damage == 0


# ---------------------------------------------------------------------------------------------
# GD05-016 Murasame


@pytest.mark.card("GD05-016")
@pytest.mark.ruling("GD05-016:Q344")
def test_gd05_016_gains_high_maneuver_when_itself_is_deployed() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    murasame = sc.add(0, "GD05-016", Zone.HAND)
    st = sc.start()
    play(st, murasame)
    assert "High-Maneuver" in keywords(st, murasame)


@pytest.mark.card("GD05-016")
def test_gd05_016_gains_high_maneuver_only_for_orb_deploys_this_turn() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    murasame = sc.add(0, "GD05-016")
    zaku = sc.add(0, VANILLA, Zone.HAND)
    m1 = sc.add(0, "GD05-015", Zone.HAND)
    st = sc.start()
    play(st, zaku)
    assert "High-Maneuver" not in keywords(st, murasame)
    play(st, m1)
    resolve_orders(st)
    assert "High-Maneuver" in keywords(st, murasame)
    to_next_turn(st)
    assert "High-Maneuver" not in keywords(st, murasame)


# ---------------------------------------------------------------------------------------------
# GD05-017 Nu Gundam (Lv7)


def _nu_gundam(londo_bell_in_trash: int, *enemy: str) -> tuple[GameState, int, int, list[int]]:
    sc = Scenario()
    sc.resources(0, 3)
    nu = sc.add(0, "GD05-017")
    pilot = sc.add(0, RIDDHE, Zone.HAND)
    sc.trash(0, *([JEGAN] * londo_bell_in_trash))
    units = [sc.add(1, n) for n in enemy]
    sc.shields(1, VANILLA)
    st = sc.start()
    return st, nu, pilot, units


@pytest.mark.card("GD05-017")
@pytest.mark.ruling("GD05-017:Q345")
@pytest.mark.rule("5-22-3", "5-22-3-1")
def test_gd05_017_exile_three_to_start_a_damage_only_battle() -> None:
    st, nu, pilot, (target, blocker) = _nu_gundam(3, VANILLA, BLOCKER)
    play(st, pilot, onto=nu)
    yes(st)
    select(st, target)
    assert len(st.zones[0][Zone.REMOVAL]) == 3
    assert zone_of(st, target) is Zone.TRASH
    assert st.cards[nu].damage == 2
    assert zone_of(st, blocker) is Zone.BATTLE and not st.cards[blocker].rested
    assert st.pending is not None and st.pending.kind is DecisionKind.MAIN
    assert st.battle is None


@pytest.mark.card("GD05-017")
@pytest.mark.ruling("GD05-017:Q347")
@pytest.mark.rule("13-1-2")
def test_gd05_017_effect_battle_destruction_triggers_breach() -> None:
    st, nu, pilot, (target,) = _nu_gundam(3, VANILLA)
    play(st, pilot, onto=nu)
    yes(st)
    assert zone_of(st, target) is Zone.TRASH
    assert st.zones[1][Zone.SHIELD] == []


@pytest.mark.card("GD05-017")
@pytest.mark.ruling("GD05-017:Q346")
def test_gd05_017_battle_damage_effects_apply_in_an_effect_battle() -> None:
    st, nu, pilot, (destiny,) = _nu_gundam(3, "GD05-055")
    play(st, pilot, onto=nu)
    yes(st)
    assert ap(st, nu) == 6
    assert st.cards[destiny].damage == 6 - 2
    assert st.cards[nu].damage == 5


@pytest.mark.card("GD05-017")
def test_gd05_017_needs_three_londo_bell_cards_in_trash() -> None:
    st, nu, pilot, (target,) = _nu_gundam(2, VANILLA)
    play(st, pilot, onto=nu)
    assert st.pending is not None and st.pending.kind is DecisionKind.MAIN
    assert st.zones[0][Zone.REMOVAL] == []
    assert st.cards[target].damage == 0 and st.cards[nu].damage == 0


@pytest.mark.card("GD05-017")
def test_gd05_017_declining_keeps_trash_and_starts_no_battle() -> None:
    st, nu, pilot, (target,) = _nu_gundam(4, VANILLA)
    play(st, pilot, onto=nu)
    no(st)
    assert len(st.zones[0][Zone.TRASH]) == 4
    assert st.cards[target].damage == 0
    assert keywords(st, nu).get("Breach") == 5


# ---------------------------------------------------------------------------------------------
# GD05-018 Gundam Calibarn


@pytest.mark.card("GD05-018")
def test_gd05_018_deploy_places_three_ex_resources() -> None:
    sc = Scenario()
    sc.resources(0, 8)
    calibarn = sc.add(0, "GD05-018", Zone.HAND)
    st = sc.start()
    play(st, calibarn)
    assert ex_resources(st) == 3


@pytest.mark.card("GD05-018")
@pytest.mark.ruling("GD05-018:Q350")
@pytest.mark.rule("4-4-2-1")
def test_gd05_018_ex_resources_capped_at_five() -> None:
    sc = Scenario()
    sc.resources(0, 8, ex=3)
    calibarn = sc.add(0, "GD05-018", Zone.HAND)
    st = sc.start()
    play(st, calibarn, ex=0)
    assert ex_resources(st) == 5


def _calibarns(n: int, *, normal: int, ex: int, rested: int | None = None) -> Scenario:
    sc = Scenario()
    for _ in range(n):
        sc.add(0, "GD05-018")
    sc.resources(0, normal, rested=normal if rested is None else rested, ex=ex)
    return sc


@pytest.mark.card("GD05-018")
@pytest.mark.ruling("GD05-018:Q348")
def test_gd05_018_exiled_ex_resource_reduces_enemy_damage_to_chosen_unit_by_3() -> None:
    sc = _calibarns(1, normal=1, ex=1)
    ally = sc.add(0, BLOCKER)
    zaku = sc.add(0, VANILLA, Zone.HAND)
    destiny = sc.add(1, "GD05-055", rested=True)  # AP5
    st = sc.start()
    play(st, zaku, ex=1)
    assert ex_resources(st) == 0
    select(st, ally)
    attack(st, ally, destiny)
    pass_all(st)
    assert st.cards[ally].damage == 5 - 3


@pytest.mark.card("GD05-018")
def test_gd05_018_choice_is_optional_and_lasts_only_this_turn() -> None:
    sc = _calibarns(1, normal=1, ex=2)
    ally = sc.add(0, BLOCKER)
    zaku = sc.add(0, VANILLA, Zone.HAND)
    zaku2 = sc.add(0, VANILLA, Zone.HAND)
    st = sc.start()
    play(st, zaku, ex=1)
    select(st)
    assert not V.rules_of(V.derived(st), ally, d.RuleKind.REDUCE_DAMAGE)
    play(st, zaku2, ex=1)
    select(st, ally)
    assert V.rules_of(V.derived(st), ally, d.RuleKind.REDUCE_DAMAGE)
    to_next_turn(st)
    assert not V.rules_of(V.derived(st), ally, d.RuleKind.REDUCE_DAMAGE)


@pytest.mark.card("GD05-018")
@pytest.mark.ruling("GD05-018:Q349")
def test_gd05_018_trigger_from_paying_for_a_unit_resolves_with_its_deploy_trigger() -> None:
    sc = _calibarns(1, normal=2, ex=1, rested=0)
    bws = sc.add(0, "GD05-023", Zone.HAND)  # Lv3 cost3, 【Deploy】Place 1 EX Resource.
    st = sc.start()
    play(st, bws, ex=1)
    assert st.pending is not None and st.pending.kind is DecisionKind.ORDER_TRIGGER
    assert len(st.pending.options) == 2


@pytest.mark.card("GD05-018")
@pytest.mark.ruling("GD05-018:Q349")
def test_gd05_018_trigger_from_paying_for_a_command_resolves_after_it() -> None:
    sc = _calibarns(1, normal=3, ex=1)
    finger = sc.add(0, DARKNESS_FINGER, Zone.HAND)
    enemy = sc.add(1, BLOCKER)
    st = sc.start()
    play(st, finger, ex=1)
    assert st.cards[enemy].damage == 2
    assert zone_of(st, finger) is Zone.TRASH
    assert st.pending is not None and st.pending.kind is DecisionKind.SELECT


@pytest.mark.card("GD05-018")
@pytest.mark.ruling("GD05-018:Q483", "GD05-018:Q484")
def test_gd05_018_two_copies_each_trigger_and_reductions_add_up() -> None:
    sc = _calibarns(2, normal=1, ex=1)
    ally = sc.add(0, BLOCKER)
    zaku = sc.add(0, VANILLA, Zone.HAND)
    eins = sc.add(1, "GD05-038", rested=True)  # AP6
    st = sc.start()
    play(st, zaku, ex=1)
    resolve_orders(st)
    select(st, ally, done=False)
    select(st, ally)
    assert len(V.rules_of(V.derived(st), ally, d.RuleKind.REDUCE_DAMAGE)) == 2
    attack(st, ally, eins)
    pass_all(st)
    assert st.cards[ally].damage == 0


@pytest.mark.card("GD05-018")
@pytest.mark.ruling("GD05-018:Q485")
@pytest.mark.rule("10-1-6-3")
def test_gd05_018_two_ex_resources_exiled_at_once_trigger_once() -> None:
    sc = _calibarns(1, normal=1, ex=2)
    ally = sc.add(0, BLOCKER)
    murasame = sc.add(0, "GD05-003", Zone.HAND)  # Lv3 cost2
    st = sc.start()
    play(st, murasame, ex=2)
    assert ex_resources(st) == 0
    select(st, ally)
    assert st.pending is not None and st.pending.kind is DecisionKind.MAIN
    assert len(V.rules_of(V.derived(st), ally, d.RuleKind.REDUCE_DAMAGE)) == 1


# ---------------------------------------------------------------------------------------------
# GD05-019 Re-GZ


@pytest.mark.card("GD05-019")
def test_gd05_019_destroyed_may_add_a_londo_bell_unit_from_top_three() -> None:
    sc = Scenario()
    regz = sc.add(0, "GD05-019")
    gyunei = sc.add(0, "GD05-057")
    sc.deck(0, VANILLA, JEGAN, VANILLA, VANILLA)
    st = sc.start()
    top3 = list(st.zones[0][Zone.DECK][:3])
    jegan = top3[1]
    destroy_own_unit_with_gyunei(st, gyunei, regz)
    yes(st)
    assert zone_of(st, jegan) is Zone.HAND
    assert set(st.zones[0][Zone.DECK][-2:]) == {top3[0], top3[2]}


@pytest.mark.card("GD05-019")
@pytest.mark.ruling("GD05-019:Q477")
def test_gd05_019_look_is_mandatory_even_when_declining_to_add() -> None:
    sc = Scenario()
    regz = sc.add(0, "GD05-019")
    gyunei = sc.add(0, "GD05-057")
    sc.deck(0, VANILLA, JEGAN, VANILLA, VANILLA)
    st = sc.start()
    top3 = list(st.zones[0][Zone.DECK][:3])
    fourth = st.zones[0][Zone.DECK][3]
    destroy_own_unit_with_gyunei(st, gyunei, regz)
    no(st)
    assert set(st.zones[0][Zone.DECK][-3:]) == set(top3)
    assert st.zones[0][Zone.DECK][0] == fourth


# ---------------------------------------------------------------------------------------------
# GD05-020 Nu Gundam (Lv5)


@pytest.mark.card("GD05-020")
def test_gd05_020_breach_3_only_while_paired() -> None:
    sc = Scenario()
    paired = sc.add(0, "GD05-020", pilot=RIDDHE)
    unpaired = sc.add(0, "GD05-020")
    st = sc.start()
    assert keywords(st, paired).get("Breach") == 3
    assert "Breach" not in keywords(st, unpaired)


@pytest.mark.card("GD05-020")
@pytest.mark.parametrize(("londo_bell", "placed"), [(2, 1), (1, 0)])
def test_gd05_020_deploy_places_ex_resource_with_two_londo_bell_in_trash(
    londo_bell: int, placed: int
) -> None:
    sc = Scenario()
    sc.resources(0, 5)
    nu = sc.add(0, "GD05-020", Zone.HAND)
    sc.trash(0, *([JEGAN] * londo_bell))
    st = sc.start()
    play(st, nu)
    assert ex_resources(st) == placed


# ---------------------------------------------------------------------------------------------
# GD05-021 Gundam AGE-2 Double Bullet


@pytest.mark.card("GD05-021")
@pytest.mark.rule("13-2-2")
def test_gd05_021_activate_action_gives_ap_4_during_this_battle() -> None:
    sc = Scenario()
    sc.resources(0, 1)
    age2 = sc.add(0, "GD05-021")
    victim = sc.add(1, "GD05-006", rested=True)  # AP5 HP6
    st = sc.start()
    attack(st, age2, victim)
    assert st.pending is not None and st.pending.kind is DecisionKind.ACTION_STEP
    activate(st, age2)
    pass_all(st)
    assert active_resources(st) == 0
    assert zone_of(st, victim) is Zone.TRASH
    assert zone_of(st, age2) is Zone.TRASH


@pytest.mark.card("GD05-021")
def test_gd05_021_without_activation_the_same_attack_does_not_destroy() -> None:
    sc = Scenario()
    sc.resources(0, 1)
    age2 = sc.add(0, "GD05-021")
    victim = sc.add(1, "GD05-006", rested=True)
    st = sc.start()
    attack(st, age2, victim)
    pass_(st)
    pass_all(st)
    assert st.cards[victim].damage == 4


@pytest.mark.card("GD05-021")
def test_gd05_021_activation_is_once_per_turn() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    age2 = sc.add(0, "GD05-021")
    sc.add(1, VANILLA, rested=True)
    sc.shields(1, VANILLA, VANILLA)
    st = sc.start()
    attack(st, age2)
    activate(st, age2)
    assert st.pending is not None and not has_action(st, A.ACTIVATE, age2)


@pytest.mark.card("GD05-021")
@pytest.mark.ruling("GD05-021:Q351")
def test_gd05_021_reduces_first_enemy_damage_each_turn_with_ef_pilot() -> None:
    sc = Scenario(active=1)
    age2 = sc.add(0, "GD05-021", rested=True, pilot=ASEMU)
    first = sc.add(1, "GD05-005")  # AP3
    second = sc.add(1, "GD05-005")
    st = sc.start()
    attack(st, first, age2)
    pass_all(st)
    assert st.cards[age2].damage == 3 - 2
    attack(st, second, age2)
    pass_all(st)
    assert st.cards[age2].damage == 1 + 3


@pytest.mark.card("GD05-021")
def test_gd05_021_no_reduction_without_an_earth_federation_pilot() -> None:
    sc = Scenario(active=1)
    age2 = sc.add(0, "GD05-021", rested=True, pilot=KIRA)
    attacker = sc.add(1, "GD05-005")
    st = sc.start()
    attack(st, attacker, age2)
    pass_all(st)
    assert st.cards[age2].damage == 3


@pytest.mark.card("GD05-021")
@pytest.mark.xfail(
    strict=True,
    reason="ENGINE: a 'during this battle' effect created outside a battle never expires",
)
def test_gd05_021_ap_boost_activated_outside_a_battle_does_not_persist() -> None:
    sc = Scenario()
    sc.resources(0, 1)
    age2 = sc.add(0, "GD05-021")
    st = sc.start()
    end_main(st)
    assert st.pending is not None and st.pending.kind is DecisionKind.ACTION_STEP
    activate(st, age2)
    to_next_turn(st)
    assert ap(st, age2) == 4


# ---------------------------------------------------------------------------------------------
# GD05-022 Gundam Schwarzette


def _schwarzette_defending(commands_in_trash: int) -> tuple[GameState, int, int, int]:
    sc = Scenario(active=1)
    schwarzette = sc.add(0, "GD05-022", rested=True)
    sc.trash(0, *([DARKNESS_FINGER] * commands_in_trash))
    first = sc.add(1, "GD05-005")  # AP3
    second = sc.add(1, "GD05-005")
    st = sc.start()
    return st, schwarzette, first, second


@pytest.mark.card("GD05-022")
@pytest.mark.ruling("GD05-022:Q352")
def test_gd05_022_exile_two_commands_to_reduce_enemy_damage_this_battle() -> None:
    st, schwarzette, first, second = _schwarzette_defending(2)
    assert keywords(st, schwarzette).get("Breach") == 3
    attack(st, first, schwarzette)
    assert st.pending is not None and st.pending.kind is DecisionKind.ACTION_STEP
    activate(st, schwarzette)
    assert len(st.zones[0][Zone.REMOVAL]) == 2
    pass_all(st)
    assert st.cards[schwarzette].damage == 3 - 2
    attack(st, second, schwarzette)
    pass_all(st)
    assert zone_of(st, schwarzette) is Zone.TRASH


@pytest.mark.card("GD05-022")
def test_gd05_022_needs_two_command_cards_in_trash() -> None:
    st, schwarzette, first, _second = _schwarzette_defending(1)
    attack(st, first, schwarzette)
    assert not has_action(st, A.ACTIVATE, schwarzette)


# ---------------------------------------------------------------------------------------------
# GD05-023 Re-GZ BWS


@pytest.mark.card("GD05-023")
def test_gd05_023_deploy_places_an_ex_resource() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    bws = sc.add(0, "GD05-023", Zone.HAND)
    st = sc.start()
    play(st, bws)
    assert ex_resources(st) == 1


# ---------------------------------------------------------------------------------------------
# GD05-024 Gundam AGE-2 Normal (SP Ver.)


@pytest.mark.card("GD05-024")
@pytest.mark.ruling("GD05-024:Q353")
def test_gd05_024_destroyed_returns_its_own_green_ef_pilot_then_discards() -> None:
    sc = Scenario()
    age2 = sc.add(0, "GD05-024", pilot=ASEMU)
    gyunei = sc.add(0, "GD05-057")
    (zaku,) = sc.hand(0, VANILLA)
    st = sc.start()
    asemu = st.cards[age2].pair
    destroy_own_unit_with_gyunei(st, gyunei, age2)
    assert zone_of(st, asemu) is Zone.HAND
    assert st.pending is not None and st.pending.kind is DecisionKind.DISCARD
    select(st, zaku)
    assert zone_of(st, zaku) is Zone.TRASH
    assert zone_of(st, asemu) is Zone.HAND


@pytest.mark.card("GD05-024")
def test_gd05_024_without_a_green_ef_pilot_in_trash_nothing_is_discarded() -> None:
    sc = Scenario()
    age2 = sc.add(0, "GD05-024", pilot=KIRA)
    gyunei = sc.add(0, "GD05-057")
    sc.hand(0, VANILLA)
    st = sc.start()
    destroy_own_unit_with_gyunei(st, gyunei, age2)
    assert hand_count(st) == 1


# ---------------------------------------------------------------------------------------------
# GD05-025 Demi Barding


@pytest.mark.card("GD05-025")
def test_gd05_025_deploy_may_add_a_command_from_top_three() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    demi = sc.add(0, "GD05-025", Zone.HAND)
    sc.deck(0, VANILLA, DARKNESS_FINGER, VANILLA, VANILLA)
    st = sc.start()
    top3 = list(st.zones[0][Zone.DECK][:3])
    play(st, demi)
    yes(st)
    assert zone_of(st, top3[1]) is Zone.HAND
    assert set(st.zones[0][Zone.DECK][-2:]) == {top3[0], top3[2]}


@pytest.mark.card("GD05-025")
@pytest.mark.ruling("GD05-025:Q478")
def test_gd05_025_look_happens_even_when_declining() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    demi = sc.add(0, "GD05-025", Zone.HAND)
    sc.deck(0, VANILLA, DARKNESS_FINGER, VANILLA, VANILLA)
    st = sc.start()
    top3 = list(st.zones[0][Zone.DECK][:3])
    play(st, demi)
    no(st)
    assert set(st.zones[0][Zone.DECK][-3:]) == set(top3)


# ---------------------------------------------------------------------------------------------
# GD05-026 Gundam Aerial Rebuild


def _aerial_rebuild(namesakes: int, *enemy_hand: str) -> tuple[GameState, int, list[int]]:
    sc = Scenario(active=1)
    aerial = sc.add(0, "GD05-026")
    for _ in range(namesakes):
        sc.add(0, LFRITH)
    sc.resources(1, 6)
    hand = [sc.add(1, n, Zone.HAND) for n in enemy_hand]
    st = sc.start()
    return st, aerial, hand


@pytest.mark.card("GD05-026")
def test_gd05_026_enemy_units_up_to_the_count_are_deployed_rested() -> None:
    st, _aerial, (zaku, murasame) = _aerial_rebuild(1, VANILLA, "GD05-003")
    play(st, zaku)
    assert st.cards[zaku].rested
    play(st, murasame)
    assert not st.cards[murasame].rested


@pytest.mark.card("GD05-026")
def test_gd05_026_counts_itself_only_without_lfrith_or_gundnode() -> None:
    st, _aerial, (zaku,) = _aerial_rebuild(0, VANILLA)
    play(st, zaku)
    assert not st.cards[zaku].rested


@pytest.mark.card("GD05-026")
def test_gd05_026_gundnode_tokens_count() -> None:
    sc = Scenario(active=1)
    sc.add(0, "GD05-026")
    sc.add(0, "T-026")
    sc.resources(1, 6)
    zaku = sc.add(1, VANILLA, Zone.HAND)
    st = sc.start()
    play(st, zaku)
    assert st.cards[zaku].rested


@pytest.mark.card("GD05-026")
@pytest.mark.ruling("GD05-026:Q354")
def test_gd05_026_deployed_rested_is_not_rested_by_an_effect() -> None:
    st, _aerial, (guaiz,) = _aerial_rebuild(3, GUAIZ)
    play(st, guaiz)
    assert st.cards[guaiz].rested
    assert ap(st, guaiz) == 4


@pytest.mark.card("GD05-026")
def test_gd05_026_units_deployed_together_are_all_deployed_rested() -> None:
    st, _aerial, (a, b) = _aerial_rebuild(1, VANILLA, VANILLA)
    deploy_cards(st, [a, b], rested=False, by=1)
    st.pending = None
    advance(st)
    assert st.cards[a].rested and st.cards[b].rested


@pytest.mark.card("GD05-026")
def test_gd05_026_friendly_units_are_not_affected() -> None:
    sc = Scenario()
    sc.add(0, "GD05-026")
    sc.resources(0, 2)
    zaku = sc.add(0, VANILLA, Zone.HAND)
    st = sc.start()
    play(st, zaku)
    assert not st.cards[zaku].rested


# ---------------------------------------------------------------------------------------------
# GD05-028 Kayra's Jegan


@pytest.mark.card("GD05-028")
def test_gd05_028_chosen_londo_bell_unit_may_attack_active_units_with_4_or_less_ap() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    jegan = sc.add(0, JEGAN)
    kayra = sc.add(0, "GD05-028", Zone.HAND)
    small = sc.add(1, VANILLA)
    big = sc.add(1, "GD05-055")  # AP5
    st = sc.start()
    assert not has_action(st, A.ATTACK, jegan, small)
    play(st, kayra)
    select(st, jegan)
    assert has_action(st, A.ATTACK, jegan, small)
    assert not has_action(st, A.ATTACK, jegan, big)


# ---------------------------------------------------------------------------------------------
# GD05-029 Kayra's Re-GZ


@pytest.mark.card("GD05-029")
def test_gd05_029_deploy_puts_top_card_on_top_or_bottom() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    regz = sc.add(0, "GD05-029", Zone.HAND)
    st = sc.start()
    top = st.zones[0][Zone.DECK][0]
    play(st, regz)
    assert st.pending is not None and st.pending.kind is DecisionKind.ARRANGE
    apply(st, next(o for o in options(st) if o.a == 1))
    assert st.zones[0][Zone.DECK][-1] == top


# ---------------------------------------------------------------------------------------------
# GD05-030 Michaelis, GD05-048 Gundam Kyrios (Flight Mode)


@pytest.mark.card("GD05-030", "GD05-048")
@pytest.mark.ruling("GD05-030:Q355", "GD05-048:Q366")
@pytest.mark.parametrize("number", ["GD05-030", "GD05-048"])
def test_deploy_turn_attack_only_against_rested_units_even_with_bridge_crew(number: str) -> None:
    sc = Scenario()
    sc.resources(0, 4)
    unit = sc.add(0, number, Zone.HAND)
    crew = sc.add(0, BRIDGE_CREW, Zone.HAND)
    rested = sc.add(1, VANILLA, rested=True)
    active = sc.add(1, VANILLA)
    sc.shields(1, VANILLA)
    st = sc.start()
    play(st, unit)
    play(st, crew)
    assert has_action(st, A.ATTACK, unit, rested)
    assert not has_action(st, A.ATTACK, unit, active)
    assert not has_action(st, A.ATTACK, unit, PLAYER_TARGET)


# ---------------------------------------------------------------------------------------------
# GD05-033 Master Gundam


def _master_gundam(special_moves: int, *, base: bool) -> tuple[GameState, int, list[int], int]:
    sc = Scenario()
    master = sc.add(0, "GD05-033")
    sc.trash(0, *([DARKNESS_FINGER] * special_moves))
    shields = sc.shields(1, VANILLA, VANILLA, VANILLA)
    base_uid = sc.base(1) if base else -1
    st = sc.start()
    return st, master, shields, base_uid


@pytest.mark.card("GD05-033")
@pytest.mark.ruling("GD05-033:Q356")
def test_gd05_033_exile_two_special_moves_to_deal_5_to_the_base_first() -> None:
    st, master, (top, second, third), base = _master_gundam(2, base=True)
    attack(st, master)
    yes(st)
    assert len(st.zones[0][Zone.REMOVAL]) == 2
    assert zone_of(st, base) is not Zone.BASE
    assert zone_of(st, top) is Zone.TRASH
    assert zone_of(st, second) is Zone.SHIELD and zone_of(st, third) is Zone.SHIELD


@pytest.mark.card("GD05-033")
@pytest.mark.ruling("GD05-033:Q356")
def test_gd05_033_without_a_base_the_top_shield_takes_the_damage() -> None:
    st, master, (top, second, third), _ = _master_gundam(3, base=False)
    attack(st, master)
    yes(st)
    select(st, *st.zones[0][Zone.TRASH][:2])
    assert len(st.zones[0][Zone.TRASH]) == 1
    assert zone_of(st, top) is Zone.TRASH and zone_of(st, second) is Zone.TRASH
    assert zone_of(st, third) is Zone.SHIELD


@pytest.mark.card("GD05-033")
def test_gd05_033_needs_two_special_move_commands_in_trash() -> None:
    st, master, (top, second, third), _ = _master_gundam(1, base=False)
    attack(st, master)
    assert st.pending is not None and st.pending.kind is DecisionKind.MAIN
    assert zone_of(st, top) is Zone.TRASH
    assert zone_of(st, second) is Zone.SHIELD and zone_of(st, third) is Zone.SHIELD
    assert st.zones[0][Zone.REMOVAL] == []


# ---------------------------------------------------------------------------------------------
# GD05-034 Gaia Gundam


def _gaia(*, paired: bool = True, enemy_hand: int = 1, base: bool = False) -> Scenario:
    sc = Scenario()
    sc.resources(0, 2)
    sc.add(0, "GD05-034", pilot=STELLAR if paired else None)
    sc.shields(1, VANILLA, VANILLA)
    if base:
        sc.base(1)
    sc.hand(1, *([VANILLA] * enemy_hand))
    return sc


@pytest.mark.card("GD05-034")
@pytest.mark.ruling("GD05-034:Q357", "GD05-034:Q358")
def test_gd05_034_opponent_declines_discard_so_deploy_phantom_pain_unit_free() -> None:
    sc = _gaia()
    exass = sc.add(0, "GD05-047", Zone.HAND)
    sc.add(0, "GD05-045", Zone.HAND)  # Lv4 (Phantom Pain), also eligible
    sc.add(0, "GD05-037", Zone.HAND)  # Lv9 (Phantom Pain), not eligible
    st = sc.start()
    gaia = st.zones[0][Zone.BATTLE][0]
    attack(st, gaia)
    pass_all(st)
    assert st.pending is not None and st.pending.player == 1
    no(st)
    assert len(select_options(st)) == 2
    select(st, exass)
    assert zone_of(st, exass) is Zone.BATTLE
    assert active_resources(st) == 2
    assert hand_count(st, 1) == 1


@pytest.mark.card("GD05-034")
def test_gd05_034_opponent_discards_so_no_deploy() -> None:
    sc = _gaia()
    exass = sc.add(0, "GD05-047", Zone.HAND)
    st = sc.start()
    gaia = st.zones[0][Zone.BATTLE][0]
    attack(st, gaia)
    pass_all(st)
    yes(st)
    assert hand_count(st, 1) == 0
    assert zone_of(st, exass) is Zone.HAND
    assert st.pending is not None and st.pending.kind is DecisionKind.MAIN


@pytest.mark.card("GD05-034")
@pytest.mark.ruling("GD05-034:Q357")
def test_gd05_034_destroying_the_base_also_triggers() -> None:
    sc = _gaia(base=True)
    exass = sc.add(0, "GD05-047", Zone.HAND)
    st = sc.start()
    gaia = st.zones[0][Zone.BATTLE][0]
    attack(st, gaia)
    pass_all(st)
    assert st.zones[1][Zone.BASE] == []
    no(st)
    select(st, exass)
    assert zone_of(st, exass) is Zone.BATTLE


@pytest.mark.card("GD05-034")
@pytest.mark.ruling("GD05-034:Q359")
def test_gd05_034_unit_deployed_by_the_effect_is_deployed() -> None:
    sc = _gaia()
    exass = sc.add(0, "GD05-047", Zone.HAND)
    sc.add(1, "GD05-026")
    sc.add(1, LFRITH)
    sc.add(1, LFRITH)
    st = sc.start()
    gaia = st.zones[0][Zone.BATTLE][0]
    attack(st, gaia)
    pass_all(st)
    no(st)
    select(st, exass)
    assert zone_of(st, exass) is Zone.BATTLE
    assert st.cards[exass].rested


@pytest.mark.card("GD05-034")
def test_gd05_034_requires_pair_and_is_once_per_turn() -> None:
    sc = _gaia(paired=False)
    sc.add(0, "GD05-047", Zone.HAND)
    st = sc.start()
    gaia = st.zones[0][Zone.BATTLE][0]
    attack(st, gaia)
    pass_all(st)
    assert st.pending is not None and st.pending.kind is DecisionKind.MAIN

    sc = _gaia()
    sc.add(0, "GD05-047", Zone.HAND)
    st = sc.start()
    gaia = st.zones[0][Zone.BATTLE][0]
    attack(st, gaia)
    pass_all(st)
    yes(st)
    _ready_again(st, gaia)
    attack(st, gaia)
    pass_all(st)
    assert st.zones[1][Zone.SHIELD] == []
    assert st.pending is not None and st.pending.kind is DecisionKind.MAIN


# ---------------------------------------------------------------------------------------------
# GD05-035 Dragon Gundam


@pytest.mark.card("GD05-035")
@pytest.mark.ruling("GD05-035:Q360")
def test_gd05_035_destroying_a_shield_or_base_deals_2_to_low_ap_enemy() -> None:
    for base in (False, True):
        sc = Scenario()
        dragon = sc.add(0, "GD05-035")
        sc.shields(1, VANILLA)
        if base:
            sc.base(1)
        small = sc.add(1, VANILLA, rested=True)
        big = sc.add(1, "GD05-055", rested=True)
        st = sc.start()
        attack(st, dragon)
        pass_all(st)
        if base:
            assert st.zones[1][Zone.BASE] == []
        assert zone_of(st, small) is Zone.TRASH
        assert st.cards[big].damage == 0


@pytest.mark.card("GD05-035")
@pytest.mark.ruling("GD05-035:Q361")
@pytest.mark.rule("13-1-2")
def test_gd05_035_breach_destroying_a_shield_triggers() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    dragon = sc.add(0, "GD05-035")
    fire = sc.add(0, SIMULTANEOUS_FIRE, Zone.HAND)
    victim = sc.add(1, VANILLA, rested=True)
    small = sc.add(1, "GD05-015")  # AP1 HP2
    (shield,) = sc.shields(1, VANILLA)
    st = sc.start()
    play(st, fire)
    attack(st, dragon, victim)
    pass_all(st)
    assert zone_of(st, victim) is Zone.TRASH
    assert zone_of(st, shield) is Zone.TRASH
    assert zone_of(st, small) is Zone.TRASH


@pytest.mark.card("GD05-035")
@pytest.mark.ruling("GD05-035:Q361")
@pytest.mark.xfail(
    strict=True,
    reason="ENGINE: a Base destroyed by effect damage emits no 'destroys shield area card' event",
)
def test_gd05_035_breach_destroying_the_base_triggers() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    dragon = sc.add(0, "GD05-035")
    fire = sc.add(0, SIMULTANEOUS_FIRE, Zone.HAND)
    victim = sc.add(1, VANILLA, rested=True)
    small = sc.add(1, "GD05-015")
    sc.shields(1, VANILLA)
    sc.base(1)
    st = sc.start()
    play(st, fire)
    attack(st, dragon, victim)
    pass_all(st)
    assert st.zones[1][Zone.BASE] == []
    assert zone_of(st, small) is Zone.TRASH


@pytest.mark.card("GD05-035")
@pytest.mark.ruling("GD05-035:Q362", "GD05-035:Q363")
def test_gd05_035_linked_attack_activates_paired_command_main_for_free(
    force_link: set[str],
) -> None:
    force_link.add("GD05-035")
    sc = Scenario()
    sc.resources(0, 2)
    dragon = sc.add(0, "GD05-035", pilot=UNDYING)
    enemy = sc.add(1, "GD05-005")
    sc.shields(1, VANILLA)
    st = sc.start()
    pilot = st.cards[dragon].pair
    attack(st, dragon)
    assert st.cards[enemy].damage == 1
    assert active_resources(st) == 2
    assert zone_of(st, pilot) is Zone.PAIRED and st.cards[dragon].pair == pilot


@pytest.mark.card("GD05-035")
def test_gd05_035_unlinked_attack_does_not_activate_paired_command() -> None:
    sc = Scenario()
    dragon = sc.add(0, "GD05-035", pilot=UNDYING)
    enemy = sc.add(1, "GD05-005")
    sc.shields(1, VANILLA)
    st = sc.start()
    attack(st, dragon)
    assert st.cards[enemy].damage == 0


# ---------------------------------------------------------------------------------------------
# GD05-036 Haow Gundam


@pytest.mark.card("GD05-036")
def test_gd05_036_rest_mf_unit_to_damage_enemies_up_to_its_level() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    haow = sc.add(0, "GD05-036")
    dragon = sc.add(0, "GD05-035")  # (MF) Lv5
    pilot = sc.add(0, RIDDHE, Zone.HAND)
    lv2 = sc.add(1, VANILLA)
    lv4 = sc.add(1, "GD05-005")
    lv6 = sc.add(1, "GD05-001")
    st = sc.start()
    play(st, pilot, onto=haow)
    select(st, dragon)
    assert st.cards[dragon].rested
    assert zone_of(st, lv2) is Zone.TRASH
    assert st.cards[lv4].damage == 2
    assert st.cards[lv6].damage == 0


@pytest.mark.card("GD05-036")
def test_gd05_036_declining_deals_no_damage() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    haow = sc.add(0, "GD05-036")
    dragon = sc.add(0, "GD05-035")
    pilot = sc.add(0, RIDDHE, Zone.HAND)
    enemy = sc.add(1, "GD05-005")
    st = sc.start()
    play(st, pilot, onto=haow)
    select(st)
    assert not st.cards[dragon].rested
    assert st.cards[enemy].damage == 0


# ---------------------------------------------------------------------------------------------
# GD05-037 Destroy Gundam


@pytest.mark.card("GD05-037")
@pytest.mark.parametrize(("enemy_trash", "level", "cost"), [(7, 6, 5), (6, 9, 8)])
def test_gd05_037_discount_with_seven_cards_in_enemy_trash(
    enemy_trash: int, level: int, cost: int
) -> None:
    sc = Scenario()
    destroy = sc.add(0, "GD05-037", Zone.HAND)
    sc.trash(1, *([VANILLA] * enemy_trash))
    st = sc.start()
    dv = V.derived(st)
    assert (V.play_level(st, dv, destroy), V.play_cost(st, dv, destroy)) == (level, cost)


@pytest.mark.card("GD05-037")
def test_gd05_037_breach_3_during_link() -> None:
    sc = Scenario()
    linked = sc.add(0, "GD05-037", pilot=STELLAR)
    unlinked = sc.add(0, "GD05-037", pilot=RIDDHE)
    st = sc.start()
    assert keywords(st, linked).get("Breach") == 3
    assert "Breach" not in keywords(st, unlinked)


# ---------------------------------------------------------------------------------------------
# GD05-038 Gundam Throne Eins (GN High Mega Launcher)


@pytest.mark.card("GD05-038")
def test_gd05_038_suppression_during_link() -> None:
    sc = Scenario()
    linked = sc.add(0, "GD05-038", pilot="GD04-111")
    unlinked = sc.add(0, "GD05-038", pilot=RIDDHE)
    st = sc.start()
    assert "Suppression" in keywords(st, linked)
    assert "Suppression" not in keywords(st, unlinked)


@pytest.mark.card("GD05-038")
def test_gd05_038_rest_three_cb_units_to_deal_4_once_per_turn() -> None:
    sc = Scenario()
    eins = sc.add(0, "GD05-038")
    kyrios = sc.add(0, "GD05-048")
    exia = sc.add(0, "GD05-050")
    target = sc.add(1, "GD05-001")  # HP5
    st = sc.start()
    activate(st, eins)
    assert st.cards[eins].rested and st.cards[kyrios].rested and st.cards[exia].rested
    assert st.cards[target].damage == 4
    assert not has_action(st, A.ACTIVATE, eins)


@pytest.mark.card("GD05-038")
def test_gd05_038_needs_three_active_cb_units() -> None:
    sc = Scenario()
    eins = sc.add(0, "GD05-038")
    sc.add(0, "GD05-048")
    sc.add(0, VANILLA)
    sc.add(1, "GD05-001")
    st = sc.start()
    assert not has_action(st, A.ACTIVATE, eins)


# ---------------------------------------------------------------------------------------------
# GD05-039 Chaos Gundam


@pytest.mark.card("GD05-039")
def test_gd05_039_attack_gives_a_linked_phantom_pain_unit_high_maneuver() -> None:
    sc = Scenario()
    chaos = sc.add(0, "GD05-039", pilot=STING)
    sc.shields(1, VANILLA)
    st = sc.start()
    attack(st, chaos)
    assert "High-Maneuver" in keywords(st, chaos)


@pytest.mark.card("GD05-039")
def test_gd05_039_no_linked_phantom_pain_unit_no_effect() -> None:
    sc = Scenario()
    chaos = sc.add(0, "GD05-039", pilot=RIDDHE)
    sc.shields(1, VANILLA)
    st = sc.start()
    attack(st, chaos)
    assert "High-Maneuver" not in keywords(st, chaos)


# ---------------------------------------------------------------------------------------------
# GD05-040 Abyss Gundam


@pytest.mark.card("GD05-040")
@pytest.mark.rule("13-1-3-1")
def test_gd05_040_support_2() -> None:
    sc = Scenario()
    abyss = sc.add(0, "GD05-040")
    ally = sc.add(0, VANILLA)
    st = sc.start()
    assert keywords(st, abyss).get("Support") == 2
    activate(st, abyss)
    assert st.cards[abyss].rested
    assert ap(st, ally) == 4


# ---------------------------------------------------------------------------------------------
# GD05-041 Gaia Gundam (MA Mode)


@pytest.mark.card("GD05-041")
def test_gd05_041_cost_minus_2_after_opponent_discarded_to_your_effect() -> None:
    sc = Scenario()
    sc.resources(0, 4, rested=2)
    abyss = sc.add(0, "GD05-046")
    auel = sc.add(0, AUEL, Zone.HAND)
    gaia, spare = sc.hand(0, "GD05-041", "GD05-041")
    sc.hand(1, VANILLA, VANILLA, VANILLA, VANILLA)
    st = sc.start()
    assert V.play_cost(st, V.derived(st), gaia) == 3
    play(st, auel, onto=abyss)
    select(st, st.zones[1][Zone.HAND][0])
    assert hand_count(st, 1) == 3
    assert V.play_cost(st, V.derived(st), gaia) == 1
    play(st, gaia)
    assert zone_of(st, gaia) is Zone.BATTLE
    to_next_turn(st)
    to_next_turn(st)
    assert V.play_cost(st, V.derived(st), spare) == 3


# ---------------------------------------------------------------------------------------------
# GD05-044 Gundam Rose, GD05-069 Gundam Maxter (【During Link】【Attack】Activate 【Main】 ...)


@pytest.mark.card("GD05-044")
@pytest.mark.ruling("GD05-044:Q364", "GD05-044:Q365")
def test_gd05_044_linked_attack_activates_paired_command_main_for_free(
    force_link: set[str],
) -> None:
    force_link.add("GD05-044")
    sc = Scenario()
    sc.resources(0, 2)
    rose = sc.add(0, "GD05-044", pilot=UNDYING)
    enemy = sc.add(1, "GD05-005")
    sc.shields(1, VANILLA)
    st = sc.start()
    pilot = st.cards[rose].pair
    attack(st, rose)
    assert st.cards[enemy].damage == 1
    assert active_resources(st) == 2
    assert zone_of(st, pilot) is Zone.PAIRED and st.cards[rose].pair == pilot


@pytest.mark.card("GD05-044")
def test_gd05_044_unlinked_attack_does_nothing() -> None:
    sc = Scenario()
    rose = sc.add(0, "GD05-044", pilot=UNDYING)
    enemy = sc.add(1, "GD05-005")
    sc.shields(1, VANILLA)
    st = sc.start()
    attack(st, rose)
    assert st.cards[enemy].damage == 0


@pytest.mark.card("GD05-069")
@pytest.mark.ruling("GD05-069:Q378", "GD05-069:Q379")
def test_gd05_069_linked_attack_activates_paired_command_main_for_free(
    force_link: set[str],
) -> None:
    force_link.add("GD05-069")
    sc = Scenario()
    sc.resources(0, 2)
    maxter = sc.add(0, "GD05-069", pilot=UNDYING)
    enemy = sc.add(1, "GD05-005")
    sc.shields(1, VANILLA)
    st = sc.start()
    pilot = st.cards[maxter].pair
    attack(st, maxter)
    assert st.cards[enemy].damage == 1
    assert active_resources(st) == 2
    assert zone_of(st, pilot) is Zone.PAIRED and st.cards[maxter].pair == pilot


# ---------------------------------------------------------------------------------------------
# GD05-045 Chaos Gundam (MA Mode)


@pytest.mark.card("GD05-045")
@pytest.mark.rule("13-1-2")
def test_gd05_045_breach_3_hits_the_shield_area_after_destroying_a_unit() -> None:
    sc = Scenario()
    chaos = sc.add(0, "GD05-045")
    victim = sc.add(1, VANILLA, rested=True)
    base = sc.base(1)
    st = sc.start()
    assert keywords(st, chaos).get("Breach") == 3
    attack(st, chaos, victim)
    pass_all(st)
    assert zone_of(st, victim) is Zone.TRASH
    assert zone_of(st, base) is not Zone.BASE


# ---------------------------------------------------------------------------------------------
# GD05-046 Abyss Gundam (MA Mode)


def _abyss_ma(pilot: str, enemy_hand: int) -> tuple[GameState, int, int]:
    sc = Scenario()
    sc.resources(0, 3)
    abyss = sc.add(0, "GD05-046")
    card = sc.add(0, pilot, Zone.HAND)
    sc.hand(1, *([VANILLA] * enemy_hand))
    st = sc.start()
    return st, abyss, card


@pytest.mark.card("GD05-046")
def test_gd05_046_phantom_pain_pilot_makes_opponent_with_4_cards_discard() -> None:
    st, abyss, auel = _abyss_ma(AUEL, 4)
    play(st, auel, onto=abyss)
    assert st.pending is not None and st.pending.kind is DecisionKind.DISCARD
    assert st.pending.player == 1
    select(st, st.zones[1][Zone.HAND][2])
    assert hand_count(st, 1) == 3


@pytest.mark.card("GD05-046")
@pytest.mark.parametrize(("pilot", "enemy_hand"), [(AUEL, 3), (RIDDHE, 5)])
def test_gd05_046_no_discard_below_4_cards_or_with_other_pilots(
    pilot: str, enemy_hand: int
) -> None:
    st, abyss, card = _abyss_ma(pilot, enemy_hand)
    play(st, card, onto=abyss)
    assert hand_count(st, 1) == enemy_hand


# ---------------------------------------------------------------------------------------------
# GD05-049 Sazabi (Lv7)


def _sazabi_attack() -> tuple[GameState, int, int, int, int, int]:
    sc = Scenario()
    sazabi = sc.add(0, "GD05-049")
    fodder = sc.add(0, VANILLA)
    target = sc.add(1, VANILLA, rested=True)
    bystander_a = sc.add(1, VANILLA)
    bystander_b = sc.add(1, "GD05-005")
    st = sc.start()
    attack(st, sazabi, target)
    return st, sazabi, fodder, target, bystander_a, bystander_b


@pytest.mark.card("GD05-049")
def test_gd05_049_destroy_own_unit_then_opponent_destroys_a_non_battling_unit() -> None:
    st, sazabi, fodder, target, bystander_a, bystander_b = _sazabi_attack()
    assert select_options(st) == {sazabi, fodder}
    select(st, fodder)
    assert st.pending is not None and st.pending.player == 1
    assert select_options(st) == {bystander_a, bystander_b}
    select(st, bystander_b)
    pass_all(st)
    assert zone_of(st, fodder) is Zone.TRASH
    assert zone_of(st, bystander_b) is Zone.TRASH
    assert zone_of(st, bystander_a) is Zone.BATTLE
    assert zone_of(st, target) is Zone.TRASH


@pytest.mark.card("GD05-049")
def test_gd05_049_declining_destroys_nothing() -> None:
    st, _sazabi, fodder, _target, bystander_a, bystander_b = _sazabi_attack()
    select(st)
    pass_all(st)
    assert zone_of(st, fodder) is Zone.BATTLE
    assert zone_of(st, bystander_a) is Zone.BATTLE and zone_of(st, bystander_b) is Zone.BATTLE


@pytest.mark.card("GD05-049")
@pytest.mark.rule("13-1-7")
def test_gd05_049_suppression_destroys_two_shields() -> None:
    sc = Scenario()
    sazabi = sc.add(0, "GD05-049")
    shields = sc.shields(1, VANILLA, VANILLA, VANILLA)
    st = sc.start()
    attack(st, sazabi)
    select(st)
    pass_all(st)
    assert [zone_of(st, s) for s in shields] == [Zone.TRASH, Zone.TRASH, Zone.SHIELD]


# ---------------------------------------------------------------------------------------------
# GD05-050 Gundam Exia Repair


@pytest.mark.card("GD05-050")
def test_gd05_050_battle_damage_destroys_unpaired_lv4_or_lower_enemy() -> None:
    sc = Scenario()
    exia = sc.add(0, "GD05-050")
    victim = sc.add(1, "GD05-005", rested=True)  # Lv4 AP3 HP4
    st = sc.start()
    trash_before = len(st.zones[0][Zone.TRASH])
    attack(st, exia, victim)
    pass_all(st)
    resolve_orders(st)
    assert zone_of(st, victim) is Zone.TRASH
    assert zone_of(st, exia) is Zone.TRASH
    assert len(st.zones[0][Zone.TRASH]) == trash_before + 1 + 2


@pytest.mark.card("GD05-050")
@pytest.mark.parametrize(("number", "pilot"), [("GD05-005", RIDDHE), ("GD05-021", None)])
def test_gd05_050_paired_or_high_level_enemies_survive(number: str, pilot: str | None) -> None:
    sc = Scenario()
    exia = sc.add(0, "GD05-050")
    victim = sc.add(1, number, rested=True, pilot=pilot)
    st = sc.start()
    attack(st, exia, victim)
    pass_all(st)
    resolve_orders(st)
    assert zone_of(st, victim) is Zone.BATTLE
    assert st.cards[victim].damage == 2


@pytest.mark.card("GD05-050")
@pytest.mark.ruling("GD05-050:Q367")
def test_gd05_050_zero_ap_deals_no_damage_and_destroys_nothing() -> None:
    sc = Scenario()
    exia = sc.add(0, "GD05-050")
    victim = sc.add(1, "GD05-005", rested=True)
    sc.resources(1, 2)
    iron = sc.add(1, IRON_FISTED, Zone.HAND)
    st = sc.start()
    attack(st, exia, victim)
    assert st.pending is not None and st.pending.player == 1
    play(st, iron)
    pass_all(st)
    resolve_orders(st)
    assert zone_of(st, exia) is Zone.TRASH
    assert zone_of(st, victim) is Zone.BATTLE
    assert st.cards[victim].damage == 0


@pytest.mark.card("GD05-050")
def test_gd05_050_destroyed_mills_two() -> None:
    sc = Scenario()
    exia = sc.add(0, "GD05-050")
    gyunei = sc.add(0, "GD05-057")
    st = sc.start()
    top2 = list(st.zones[0][Zone.DECK][:2])
    destroy_own_unit_with_gyunei(st, gyunei, exia)
    assert all(zone_of(st, u) is Zone.TRASH for u in top2)


# ---------------------------------------------------------------------------------------------
# GD05-051 Gundam Barbatos Lupus Rex


@pytest.mark.card("GD05-051")
def test_gd05_051_ap_increases_by_damage_received() -> None:
    sc = Scenario()
    rex = sc.add(0, "GD05-051", damage=2)
    fresh = sc.add(0, "GD05-051")
    st = sc.start()
    assert ap(st, rex) == 6
    assert ap(st, fresh) == 4


@pytest.mark.card("GD05-051")
def test_gd05_051_end_of_turn_damage_a_tekkadan_unit_and_set_it_active() -> None:
    sc = Scenario()
    rex = sc.add(0, "GD05-051", rested=True)
    st = sc.start()
    end_main(st)
    assert st.pending is not None and st.pending.kind is DecisionKind.SELECT
    select(st, rex)
    assert st.cards[rex].damage == 1
    assert not st.cards[rex].rested
    assert ap(st, rex) == 5


@pytest.mark.card("GD05-051")
def test_gd05_051_end_of_turn_effect_is_optional() -> None:
    sc = Scenario()
    rex = sc.add(0, "GD05-051", rested=True)
    st = sc.start()
    end_main(st)
    select(st)
    assert st.cards[rex].damage == 0
    assert st.active == 1


# ---------------------------------------------------------------------------------------------
# GD05-052 Sazabi (Lv5)


@pytest.mark.card("GD05-052")
def test_gd05_052_destroy_other_unit_mill_three_add_neo_zeon_unit() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    fodder = sc.add(0, VANILLA)
    sazabi = sc.add(0, "GD05-052", Zone.HAND)
    sc.deck(0, "GD05-061", VANILLA, "GD05-062", VANILLA)
    st = sc.start()
    geara, zaku, hizack = st.zones[0][Zone.DECK][:3]
    play(st, sazabi)
    assert select_options(st) == {fodder}
    select(st, fodder)
    assert zone_of(st, fodder) is Zone.TRASH
    assert select_options(st) == {geara, hizack}
    select(st, hizack)
    assert zone_of(st, hizack) is Zone.HAND
    assert zone_of(st, geara) is Zone.TRASH and zone_of(st, zaku) is Zone.TRASH


@pytest.mark.card("GD05-052")
def test_gd05_052_declining_mills_nothing() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    fodder = sc.add(0, VANILLA)
    sazabi = sc.add(0, "GD05-052", Zone.HAND)
    st = sc.start()
    deck = list(st.zones[0][Zone.DECK])
    play(st, sazabi)
    select(st)
    assert zone_of(st, fodder) is Zone.BATTLE
    assert st.zones[0][Zone.DECK] == deck


# ---------------------------------------------------------------------------------------------
# GD05-053 Quess's Jagd Doga


@pytest.mark.card("GD05-053")
def test_gd05_053_destroyed_by_own_neo_zeon_effect_returns_to_hand() -> None:
    sc = Scenario()
    quess = sc.add(0, "GD05-053")
    gyunei = sc.add(0, "GD05-057")
    st = sc.start()
    destroy_own_unit_with_gyunei(st, gyunei, quess)
    assert zone_of(st, quess) is Zone.HAND


@pytest.mark.card("GD05-053")
def test_gd05_053_destroyed_by_effect_damage_stays_in_trash() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    quess = sc.add(0, "GD05-053", damage=1)
    shield_cmd = sc.add(0, BECOME_A_SHIELD, Zone.HAND)
    sc.add(1, "GD05-005")
    st = sc.start()
    play(st, shield_cmd)
    assert zone_of(st, quess) is Zone.TRASH


@pytest.mark.card("GD05-053")
def test_gd05_053_destroyed_in_battle_stays_in_trash() -> None:
    sc = Scenario()
    quess = sc.add(0, "GD05-053")
    victim = sc.add(1, "GD05-005", rested=True)
    st = sc.start()
    attack(st, quess, victim)
    pass_all(st)
    assert zone_of(st, quess) is Zone.TRASH


# ---------------------------------------------------------------------------------------------
# GD05-054 Alpha Azieru


@pytest.mark.card("GD05-054")
@pytest.mark.ruling("GD05-054:Q368")
def test_gd05_054_draws_once_per_turn_when_a_unit_is_destroyed_by_an_effect() -> None:
    sc = Scenario()
    sc.add(0, "GD05-054")
    gyunei_a = sc.add(0, "GD05-057")
    gyunei_b = sc.add(0, "GD05-057")
    first = sc.add(0, VANILLA)
    second = sc.add(0, VANILLA)
    st = sc.start()
    before = hand_count(st)
    activate(st, gyunei_a)
    select(st, first)
    assert hand_count(st) == before + 1
    activate(st, gyunei_b)
    select(st, second)
    assert zone_of(st, second) is Zone.TRASH
    assert hand_count(st) == before + 1


@pytest.mark.card("GD05-054")
@pytest.mark.ruling("GD05-054:Q368")
def test_gd05_054_no_draw_for_effect_damage_or_battle_destruction() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    sc.add(0, "GD05-054")
    frail = sc.add(0, VANILLA, damage=1)
    attacker = sc.add(0, VANILLA)
    shield_cmd = sc.add(0, BECOME_A_SHIELD, Zone.HAND)
    enemy = sc.add(1, "GD05-005", rested=True)
    st = sc.start()
    before = hand_count(st) - 1
    play(st, shield_cmd)
    select(st, frail)
    assert zone_of(st, frail) is Zone.TRASH
    assert hand_count(st) == before
    attack(st, attacker, enemy)
    pass_all(st)
    assert zone_of(st, attacker) is Zone.TRASH
    assert hand_count(st) == before


@pytest.mark.card("GD05-054")
def test_gd05_054_has_blocker() -> None:
    sc = Scenario()
    azieru = sc.add(0, "GD05-054")
    st = sc.start()
    assert "Blocker" in keywords(st, azieru)


# ---------------------------------------------------------------------------------------------
# GD05-055 Destiny Gundam


@pytest.mark.card("GD05-055")
@pytest.mark.ruling("GD05-055:Q369")
def test_gd05_055_reduces_first_enemy_battle_damage_each_turn_by_2() -> None:
    sc = Scenario(active=1)
    destiny = sc.add(0, "GD05-055", rested=True)
    first = sc.add(1, "GD05-005")  # AP3
    second = sc.add(1, "GD05-005")
    st = sc.start()
    attack(st, first, destiny)
    pass_all(st)
    assert st.cards[destiny].damage == 1
    attack(st, second, destiny)
    pass_all(st)
    assert st.cards[destiny].damage == 1 + 3


@pytest.mark.card("GD05-055")
@pytest.mark.rule("13-1-5-2")
def test_gd05_055_first_strike_takes_no_damage_from_destroyed_target() -> None:
    sc = Scenario()
    destiny = sc.add(0, "GD05-055")
    victim = sc.add(1, "GD05-001", rested=True)  # AP4 HP5
    st = sc.start()
    attack(st, destiny, victim)
    pass_all(st)
    assert zone_of(st, victim) is Zone.TRASH
    assert st.cards[destiny].damage == 0


# ---------------------------------------------------------------------------------------------
# GD05-057 Gyunei's Jagd Doga (Lv4)


@pytest.mark.card("GD05-057")
@pytest.mark.ruling("GD05-057:Q370", "GD05-057:Q371")
def test_gd05_057_active_unit_still_gets_the_attack_restriction() -> None:
    sc = Scenario()
    gyunei = sc.add(0, "GD05-057")
    fodder = sc.add(0, VANILLA)
    other = sc.add(0, VANILLA)
    rested_enemy = sc.add(1, VANILLA, rested=True)
    sc.shields(1, VANILLA)
    sc.base(1)
    st = sc.start()
    activate(st, gyunei)
    select(st, fodder)
    assert zone_of(st, fodder) is Zone.TRASH
    assert not has_action(st, A.ATTACK, gyunei, PLAYER_TARGET)
    assert has_action(st, A.ATTACK, gyunei, rested_enemy)
    assert has_action(st, A.ATTACK, other, PLAYER_TARGET)
    assert not has_action(st, A.ACTIVATE, gyunei)


@pytest.mark.card("GD05-057")
def test_gd05_057_rested_unit_is_set_active_after_destroying() -> None:
    sc = Scenario()
    gyunei = sc.add(0, "GD05-057", rested=True)
    fodder = sc.add(0, VANILLA)
    st = sc.start()
    activate(st, gyunei)
    assert zone_of(st, fodder) is Zone.TRASH
    assert not st.cards[gyunei].rested


# ---------------------------------------------------------------------------------------------
# GD05-059 Gundam Barbatos Lupus


@pytest.mark.card("GD05-059")
def test_gd05_059_rest_gjallarhorn_unit_to_draw_and_gain_high_maneuver() -> None:
    sc = Scenario()
    lupus = sc.add(0, "GD05-059")
    graze = sc.add(0, GRAZE)
    sc.shields(1, VANILLA)
    st = sc.start()
    before = hand_count(st)
    attack(st, lupus)
    assert st.cards[graze].rested
    assert hand_count(st) == before + 1
    assert "High-Maneuver" in keywords(st, lupus)


@pytest.mark.card("GD05-059")
def test_gd05_059_without_an_active_gjallarhorn_unit_nothing_happens() -> None:
    sc = Scenario()
    lupus = sc.add(0, "GD05-059")
    sc.add(0, GRAZE, rested=True)
    sc.shields(1, VANILLA)
    st = sc.start()
    before = hand_count(st)
    attack(st, lupus)
    assert hand_count(st) == before
    assert "High-Maneuver" not in keywords(st, lupus)


# ---------------------------------------------------------------------------------------------
# GD05-060 Gundam Flauros (Ryusei-Go)


@pytest.mark.card("GD05-060")
@pytest.mark.ruling("GD05-060:Q372", "GD05-060:Q373")
def test_gd05_060_deploy_and_attack_each_destroy_a_lv2_enemy() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    flauros = sc.add(0, "GD05-060", Zone.HAND)
    mikazuki = sc.add(0, MIKAZUKI, Zone.HAND)
    first = sc.add(1, VANILLA)
    second = sc.add(1, VANILLA)
    big = sc.add(1, "GD05-005")
    sc.shields(1, VANILLA)
    st = sc.start()
    play(st, flauros)
    assert select_options(st) == {first, second}
    select(st, first)
    assert zone_of(st, first) is Zone.TRASH
    play(st, mikazuki, onto=flauros)
    select(st, big)
    assert V.is_linked(V.derived(st), flauros)
    attack(st, flauros)
    assert zone_of(st, second) is Zone.TRASH
    assert zone_of(st, big) is Zone.BATTLE


# ---------------------------------------------------------------------------------------------
# GD05-061 Geara Doga


@pytest.mark.card("GD05-061")
def test_gd05_061_blocker_only_with_another_neo_zeon_unit() -> None:
    sc = Scenario()
    alone = sc.add(0, "GD05-061")
    st = sc.start()
    assert "Blocker" not in keywords(st, alone)

    sc = Scenario()
    geara = sc.add(0, "GD05-061")
    sc.add(0, "GD05-062")
    st = sc.start()
    assert "Blocker" in keywords(st, geara)


@pytest.mark.card("GD05-061")
@pytest.mark.ruling("GD05-061:Q374")
def test_gd05_061_block_stands_after_losing_blocker() -> None:
    sc = Scenario(active=1)
    sc.resources(0, 3)
    geara = sc.add(0, "GD05-061")
    hizack = sc.add(0, "GD05-062", damage=1)
    shield_cmd = sc.add(0, BECOME_A_SHIELD, Zone.HAND)
    (shield,) = sc.shields(0, VANILLA)
    attacker = sc.add(1, VANILLA)
    st = sc.start()
    attack(st, attacker)
    block(st, geara)
    assert st.pending is not None and st.pending.kind is DecisionKind.ACTION_STEP
    play(st, shield_cmd)
    select(st, hizack)
    assert zone_of(st, hizack) is Zone.TRASH
    assert "Blocker" not in keywords(st, geara)
    pass_all(st)
    assert zone_of(st, shield) is Zone.SHIELD
    assert zone_of(st, geara) is Zone.TRASH


# ---------------------------------------------------------------------------------------------
# GD05-064 Force Impulse Gundam


@pytest.mark.card("GD05-064")
def test_gd05_064_deployed_from_trash_fetches_shinn_asuka() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    power = sc.add(0, AWAKENED_POWER, Zone.HAND)
    impulse = sc.add(0, "GD05-064", Zone.TRASH)
    shinn = sc.add(0, SHINN, Zone.TRASH)
    st = sc.start()
    play(st, power)
    assert zone_of(st, impulse) is Zone.BATTLE
    assert zone_of(st, shinn) is Zone.HAND


@pytest.mark.card("GD05-064")
def test_gd05_064_deployed_from_hand_fetches_nothing() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    impulse = sc.add(0, "GD05-064", Zone.HAND)
    shinn = sc.add(0, SHINN, Zone.TRASH)
    st = sc.start()
    play(st, impulse)
    assert zone_of(st, shinn) is Zone.TRASH


# ---------------------------------------------------------------------------------------------
# GD05-065 Landman Rodi


@pytest.mark.card("GD05-065")
def test_gd05_065_linked_gets_ap_2_during_your_turn_only() -> None:
    sc = Scenario()
    linked = sc.add(0, "GD05-065", pilot=MIKAZUKI)
    unlinked = sc.add(0, "GD05-065", pilot=RIDDHE)
    st = sc.start()
    assert ap(st, linked) == 1 + 2 + 2
    assert ap(st, unlinked) == 1 + 1
    to_next_turn(st)
    assert st.active == 1
    assert ap(st, linked) == 1 + 2


# ---------------------------------------------------------------------------------------------
# GD05-066 Shining Gundam (Lv4)


@pytest.mark.card("GD05-066")
def test_gd05_066_exile_two_mf_units_to_fetch_a_special_move() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    shining = sc.add(0, "GD05-066", Zone.HAND)
    mf = sc.trash(0, "GD05-042", "GD05-043")
    finger = sc.add(0, DARKNESS_FINGER, Zone.TRASH)
    st = sc.start()
    play(st, shining)
    yes(st)
    assert all(zone_of(st, u) is Zone.REMOVAL for u in mf)
    assert zone_of(st, finger) is Zone.HAND


@pytest.mark.card("GD05-066")
def test_gd05_066_needs_two_mf_unit_cards_in_trash() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    shining = sc.add(0, "GD05-066", Zone.HAND)
    (mf,) = sc.trash(0, "GD05-042")
    finger = sc.add(0, DARKNESS_FINGER, Zone.TRASH)
    st = sc.start()
    play(st, shining)
    assert st.pending is not None and st.pending.kind is DecisionKind.MAIN
    assert zone_of(st, mf) is Zone.TRASH and zone_of(st, finger) is Zone.TRASH


@pytest.mark.card("GD05-066")
def test_gd05_066_attack_sets_a_rested_resource_active_once_per_turn() -> None:
    sc = Scenario()
    sc.resources(0, 2, rested=2)
    shining = sc.add(0, "GD05-066")
    sc.shields(1, VANILLA, VANILLA)
    st = sc.start()
    attack(st, shining)
    select(st, st.zones[0][Zone.RESOURCE_AREA][0])
    pass_all(st)
    assert active_resources(st) == 1
    _ready_again(st, shining)
    attack(st, shining)
    pass_all(st)
    assert active_resources(st) == 1


# ---------------------------------------------------------------------------------------------
# GD05-067 Wing Gundam Zero (EW)


@pytest.mark.card("GD05-067")
def test_gd05_067_attack_rests_an_enemy_and_suppression_follows() -> None:
    sc = Scenario()
    wing = sc.add(0, "GD05-067")
    enemy = sc.add(1, VANILLA)
    shields = sc.shields(1, VANILLA, VANILLA, VANILLA)
    st = sc.start()
    assert "Suppression" not in keywords(st, wing)
    attack(st, wing)
    pass_all(st)
    assert st.cards[enemy].rested
    assert [zone_of(st, s) for s in shields] == [Zone.TRASH, Zone.TRASH, Zone.SHIELD]


@pytest.mark.card("GD05-067")
@pytest.mark.ruling("GD05-067:Q375")
def test_gd05_067_loses_suppression_when_the_rested_enemy_leaves() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    wing = sc.add(0, "GD05-067")
    enemy = sc.add(1, VANILLA)
    pride = sc.add(0, VETERANS_PRIDE, Zone.HAND)
    shields = sc.shields(1, VANILLA, VANILLA, VANILLA)
    st = sc.start()
    attack(st, wing)
    assert st.pending is not None and st.pending.kind is DecisionKind.ACTION_STEP
    assert "Suppression" in keywords(st, wing)
    play(st, pride)
    assert zone_of(st, enemy) is Zone.TRASH
    assert "Suppression" not in keywords(st, wing)
    pass_all(st)
    assert [zone_of(st, s) for s in shields] == [Zone.TRASH, Zone.SHIELD, Zone.SHIELD]


# ---------------------------------------------------------------------------------------------
# GD05-068 Shining Gundam (Super Mode)


@pytest.mark.card("GD05-068")
def test_gd05_068_special_move_command_grants_suppression_this_turn() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    super_mode = sc.add(0, "GD05-068")
    undying = sc.add(0, UNDYING, Zone.HAND)
    finger = sc.add(0, DARKNESS_FINGER, Zone.HAND)
    sc.add(1, BLOCKER)
    st = sc.start()
    play(st, undying)
    assert "Suppression" not in keywords(st, super_mode)
    play(st, finger)
    assert "Suppression" in keywords(st, super_mode)
    to_next_turn(st)
    assert "Suppression" not in keywords(st, super_mode)


@pytest.mark.card("GD05-068")
@pytest.mark.ruling("GD05-068:Q376")
@pytest.mark.xfail(
    strict=True,
    reason="ENGINE: activating a Command's 【Main】 through an effect emits no command event",
)
def test_gd05_068_main_activated_by_an_effect_also_grants_suppression() -> None:
    sc = Scenario(active=1)
    super_mode = sc.add(0, "GD05-068")
    sc.shields(0, DARKNESS_FINGER)
    attacker = sc.add(1, VANILLA)
    other = sc.add(1, BLOCKER)
    st = sc.start()
    attack(st, attacker)
    pass_all(st)
    assert st.pending is not None and st.pending.kind is DecisionKind.BURST
    yes(st)
    select(st, other)
    assert st.cards[other].damage == 2
    assert "Suppression" in keywords(st, super_mode)


@pytest.mark.card("GD05-068")
def test_gd05_068_linked_attack_gets_ap_2_during_this_battle(force_link: set[str]) -> None:
    force_link.add("GD05-068")
    sc = Scenario()
    super_mode = sc.add(0, "GD05-068", pilot=RIDDHE)
    victim = sc.add(1, "GD05-006", rested=True)  # AP5 HP6
    st = sc.start()
    attack(st, super_mode, victim)
    pass_all(st)
    assert zone_of(st, victim) is Zone.TRASH
    assert ap(st, super_mode) == 4 + 1


@pytest.mark.card("GD05-068")
def test_gd05_068_unlinked_attack_gets_no_bonus() -> None:
    sc = Scenario()
    super_mode = sc.add(0, "GD05-068", pilot=RIDDHE)
    victim = sc.add(1, "GD05-006", rested=True)
    st = sc.start()
    attack(st, super_mode, victim)
    pass_all(st)
    assert zone_of(st, victim) is Zone.BATTLE
    assert st.cards[victim].damage == 5


# ---------------------------------------------------------------------------------------------
# GD05-069 Gundam Maxter


def _maxter(*, enemy: str, deck: tuple[str, ...]) -> tuple[GameState, int, int]:
    sc = Scenario()
    maxter = sc.add(0, "GD05-069")
    victim = sc.add(1, enemy, rested=True)
    sc.deck(0, *deck)
    st = sc.start()
    return st, maxter, victim


@pytest.mark.card("GD05-069")
@pytest.mark.ruling("GD05-069:Q377")
def test_gd05_069_destroying_a_unit_may_add_a_special_move_even_if_both_die() -> None:
    st, maxter, victim = _maxter(enemy=VANILLA, deck=(VANILLA, DARKNESS_FINGER, VANILLA, VANILLA))
    finger = st.zones[0][Zone.DECK][1]
    attack(st, maxter, victim)
    pass_all(st)
    assert zone_of(st, maxter) is Zone.TRASH and zone_of(st, victim) is Zone.TRASH
    yes(st)
    assert zone_of(st, finger) is Zone.HAND


@pytest.mark.card("GD05-069")
@pytest.mark.ruling("GD05-069:Q479")
def test_gd05_069_look_is_mandatory_even_when_declining() -> None:
    st, maxter, victim = _maxter(enemy="GD05-015", deck=(VANILLA,) * 5)
    top4 = list(st.zones[0][Zone.DECK][:4])
    attack(st, maxter, victim)
    pass_all(st)
    no(st)
    assert set(st.zones[0][Zone.DECK][-4:]) == set(top4)


@pytest.mark.card("GD05-069")
def test_gd05_069_no_trigger_during_the_opponents_turn() -> None:
    sc = Scenario(active=1)
    maxter = sc.add(0, "GD05-069", rested=True)
    attacker = sc.add(1, "GD05-015")  # AP1 HP2
    st = sc.start()
    attack(st, attacker, maxter)
    pass_all(st)
    assert zone_of(st, attacker) is Zone.TRASH
    assert st.pending is not None and st.pending.kind is DecisionKind.MAIN


# ---------------------------------------------------------------------------------------------
# GD05-070 Tallgeese Ⅲ


@pytest.mark.card("GD05-070")
@pytest.mark.ruling("GD05-070:Q380")
def test_gd05_070_must_set_a_rested_preventer_or_g_team_link_unit_active() -> None:
    sc = Scenario()
    tallgeese = sc.add(0, "GD05-070")
    wing = sc.add(0, "GD05-067", rested=True, pilot=HEERO_G_TEAM)
    victim = sc.add(1, VANILLA, rested=True)
    sc.shields(1, VANILLA)
    st = sc.start()
    attack(st, tallgeese, victim)
    pass_all(st)
    assert zone_of(st, victim) is Zone.TRASH
    assert not st.cards[wing].rested
    assert not has_action(st, A.ATTACK, wing)


@pytest.mark.card("GD05-070")
def test_gd05_070_unlinked_units_are_not_chosen() -> None:
    sc = Scenario()
    tallgeese = sc.add(0, "GD05-070")
    wing = sc.add(0, "GD05-067", rested=True)
    victim = sc.add(1, VANILLA, rested=True)
    st = sc.start()
    attack(st, tallgeese, victim)
    pass_all(st)
    assert st.cards[wing].rested
