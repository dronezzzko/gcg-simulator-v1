"""Card behaviour tests for GD02-086..GD02-130 (work package WP-GD02-B)."""

from __future__ import annotations

import pytest

from gcg_sim.engine import view as V
from gcg_sim.engine.state import GameState
from gcg_sim.engine.types import PLAYER_TARGET, ActionKind, DecisionKind, Step, Zone
from gcg_sim.testkit import (
    Scenario,
    act,
    ap,
    attack,
    block,
    card_numbers,
    has_action,
    keywords,
    no,
    numbers_in,
    pass_all,
    play,
    select,
    to_next_turn,
    yes,
    zone_of,
)

A = ActionKind

VANILLA = "GD01-060"  # Zaku Mariner: red (Zeon) Lv2 cost 1, 2/2, no effects
BLOCKER = "GD01-072"  # Launcher Strike Gundam: white Lv4, 3/4, <Blocker>
RAIDER_MA = "GD02-019"  # blue Lv4 4/3, link (Biological CPU), no effects
HIZACK = "GD02-013"  # blue (Titans) Lv2 2/2, no link, no effects
MARASAI = "GD02-015"  # blue (Titans) Lv3 3/3, no effects
JENICE = "GD02-065"  # purple (Vulture) Lv1 1/2, no effects
BAQTO = "GD02-067"  # purple (Vagan) Lv2 cost 2, 2/3, no effects
GENOACE = "GD02-030"  # green (Earth Federation) Lv1 1/2, no effects
SAYLA = "GD01-087"  # (Newtype) Pilot
TOKEN_GUNDAM = "T-001"  # [Gundam] Unit token 3/3
ACTION_STOPPER = "GD02-109"  # a playable 【Action】 Command keeps the action step waiting for us


def _hand(st: GameState, player: int = 0) -> list[int]:
    return list(st.zones[player][Zone.HAND])


def _active_resources(st: GameState, player: int = 0) -> int:
    return sum(1 for u in st.zones[player][Zone.RESOURCE_AREA] if not st.cards[u].rested)


def _attack_into_burst(sc: Scenario) -> tuple[GameState, int]:
    """Player 0 attacks player 1 with a vanilla Unit; stop at player 1's 【Burst】 decision."""
    attacker = sc.add(0, VANILLA)
    st = sc.start()
    attack(st, attacker)
    pass_all(st)
    assert st.pending is not None and st.pending.kind is DecisionKind.BURST
    return st, attacker


def _enemy_attacks(sc: Scenario, attacker: int, target: int = PLAYER_TARGET) -> GameState:
    """Player 1 (active) attacks; stop at player 0's first action-step decision."""
    st = sc.start()
    attack(st, attacker, target)
    assert st.pending is not None and st.pending.kind is DecisionKind.ACTION_STEP
    assert st.pending.player == 0
    return st


# ---------------------------------------------------------------------------------------------
# 【Burst】 lines shared by every Pilot card and every Base card of the package


BURST_ADD_PILOTS = [
    "GD02-086",
    "GD02-087",
    "GD02-088",
    "GD02-089",
    "GD02-090",
    "GD02-091",
    "GD02-092",
    "GD02-093",
    "GD02-094",
    "GD02-095",
    "GD02-096",
    "GD02-097",
    "GD02-098",
    "GD02-099",
]


@pytest.mark.rule("13-2-5-1", "3-3-9-1")
@pytest.mark.parametrize(
    "number", [pytest.param(n, marks=pytest.mark.card(n), id=n) for n in BURST_ADD_PILOTS]
)
def test_pilot_burst_adds_this_card_to_hand(number: str) -> None:
    sc = Scenario()
    (shield,) = sc.shields(1, number)
    st, _ = _attack_into_burst(sc)
    yes(st)
    assert zone_of(st, shield) is Zone.HAND
    assert st.cards[shield].owner == 1


BURST_DEPLOY_BASES = [
    "GD02-121",
    "GD02-122",
    "GD02-123",
    "GD02-124",
    "GD02-125",
    "GD02-126",
    "GD02-127",
    "GD02-128",
    "GD02-129",
    "GD02-130",
]


@pytest.mark.rule("13-2-5-1", "13-2-5-3")
@pytest.mark.parametrize(
    "number", [pytest.param(n, marks=pytest.mark.card(n), id=n) for n in BURST_DEPLOY_BASES]
)
def test_base_burst_deploys_and_deploy_adds_a_shield(number: str) -> None:
    sc = Scenario()
    base, second = sc.shields(1, number, VANILLA)
    st, _ = _attack_into_burst(sc)
    yes(st)
    assert zone_of(st, base) is Zone.BASE
    assert zone_of(st, second) is Zone.HAND  # 【Deploy】Add 1 of your Shields to your hand.
    assert st.zones[1][Zone.SHIELD] == []


# ---------------------------------------------------------------------------------------------
# Pilots (rule 3-3-9-2: text below the name is gained by the paired Unit)


@pytest.mark.card("GD02-086")
@pytest.mark.rule("3-3-9-2")
def test_gd02_086_ap_plus_one_with_another_titans_unit() -> None:
    sc = Scenario()
    unit = sc.add(0, VANILLA, pilot="GD02-086")
    sc.add(0, MARASAI)
    st = sc.start()
    assert ap(st, unit) == 2 + 1 + 1


@pytest.mark.card("GD02-086")
def test_gd02_086_paired_titans_unit_itself_is_not_another() -> None:
    sc = Scenario()
    unit = sc.add(0, HIZACK, pilot="GD02-086")
    sc.add(1, MARASAI)
    st = sc.start()
    assert ap(st, unit) == 2 + 1


@pytest.mark.card("GD02-087")
def test_gd02_087_linked_blue_unit_rests_enemy_blocker() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    unit = sc.add(0, RAIDER_MA)
    blocker = sc.add(1, BLOCKER)
    other = sc.add(1, VANILLA)
    pilot = sc.add(0, "GD02-087", Zone.HAND)
    st = sc.start()
    play(st, pilot, onto=unit)
    assert V.is_linked(V.derived(st), unit)
    assert st.cards[blocker].rested
    assert not st.cards[other].rested


@pytest.mark.card("GD02-087")
def test_gd02_087_linked_non_blue_unit_does_nothing() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    unit = sc.add(0, "GD05-037")  # red, link (Biological CPU)
    blocker = sc.add(1, BLOCKER)
    pilot = sc.add(0, "GD02-087", Zone.HAND)
    st = sc.start()
    play(st, pilot, onto=unit)
    assert V.is_linked(V.derived(st), unit)
    assert not st.cards[blocker].rested


@pytest.mark.card("GD02-087")
@pytest.mark.rule("13-2-11-1")
def test_gd02_087_pairing_without_link_does_not_trigger() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    unit = sc.add(0, HIZACK)  # blue, no link condition
    blocker = sc.add(1, BLOCKER)
    pilot = sc.add(0, "GD02-087", Zone.HAND)
    st = sc.start()
    play(st, pilot, onto=unit)
    assert not st.cards[blocker].rested


def _flit_scenario() -> tuple[Scenario, int, int]:
    sc = Scenario()
    sc.resources(0, 3)
    unit = sc.add(0, "GD02-023")  # AGE-1 Spallow, link [Flit Asuno]
    pilot = sc.add(0, "GD02-088", Zone.HAND)
    sc.deck(0, "GD02-103", VANILLA, GENOACE)
    return sc, unit, pilot


@pytest.mark.card("GD02-088")
def test_gd02_088_adds_age_device_card_and_bottoms_the_rest() -> None:
    sc, unit, pilot = _flit_scenario()
    st = sc.start()
    top3 = st.zones[0][Zone.DECK][:3]
    play(st, pilot, onto=unit)
    yes(st)
    assert st.pending is not None and st.pending.kind is DecisionKind.SELECT
    offered = {o.a for o in st.pending.options if o.kind is A.SELECT}
    assert card_numbers(st, sorted(offered)) == ["GD02-103", GENOACE]
    age_device = top3[0]
    select(st, age_device)
    assert zone_of(st, age_device) is Zone.HAND
    assert set(st.zones[0][Zone.DECK][-2:]) == {top3[1], top3[2]}


@pytest.mark.card("GD02-088")
def test_gd02_088_adds_green_earth_federation_unit_card() -> None:
    sc, unit, pilot = _flit_scenario()
    st = sc.start()
    genoace = st.zones[0][Zone.DECK][2]
    play(st, pilot, onto=unit)
    yes(st)
    select(st, genoace)
    assert zone_of(st, genoace) is Zone.HAND


@pytest.mark.card("GD02-088")
@pytest.mark.ruling("GD02-088:Q474")
def test_gd02_088_look_is_forced_adding_is_optional() -> None:
    sc, unit, pilot = _flit_scenario()
    st = sc.start()
    top3 = st.zones[0][Zone.DECK][:3]
    hand_before = len(_hand(st))
    play(st, pilot, onto=unit)
    assert st.pending is not None and st.pending.kind is DecisionKind.YES_NO
    assert all(st.cards[u].known & 1 for u in top3)  # the 3 cards were looked at
    no(st)
    assert len(_hand(st)) == hand_before - 1
    assert set(st.zones[0][Zone.DECK][-3:]) == set(top3)


@pytest.mark.card("GD02-089")
def test_gd02_089_other_zeon_link_unit_gains_breach() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    link_unit = sc.add(0, "GD01-031", pilot="GD02-089")  # Gelgoog, link (Zeon)
    unit = sc.add(0, VANILLA)
    pilot = sc.add(0, "GD02-089", Zone.HAND)
    st = sc.start()
    assert V.is_linked(V.derived(st), link_unit)
    play(st, pilot, onto=unit)
    assert keywords(st, link_unit).get("Breach") == 1
    assert "Breach" not in keywords(st, unit)
    to_next_turn(st)
    assert "Breach" not in keywords(st, link_unit)


@pytest.mark.card("GD02-089")
def test_gd02_089_unlinked_zeon_unit_is_not_chosen() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    other = sc.add(0, "GD01-031")  # Zeon, but no Pilot so not a Link Unit
    unit = sc.add(0, VANILLA)
    pilot = sc.add(0, "GD02-089", Zone.HAND)
    st = sc.start()
    play(st, pilot, onto=unit)
    assert "Breach" not in keywords(st, other)
    assert st.pending is not None and st.pending.kind is DecisionKind.MAIN


@pytest.mark.card("GD02-090")
def test_gd02_090_ap_plus_one_with_another_high_maneuver_unit() -> None:
    sc = Scenario()
    unit = sc.add(0, VANILLA, pilot="GD02-090")
    sc.add(0, "GD03-054")  # Zeydra, <High-Maneuver>
    st = sc.start()
    assert ap(st, unit) == 2 + 1 + 1


@pytest.mark.card("GD02-090")
def test_gd02_090_no_bonus_without_another_high_maneuver_unit() -> None:
    sc = Scenario()
    unit = sc.add(0, VANILLA, pilot="GD02-090")
    sc.add(0, BLOCKER)
    st = sc.start()
    assert ap(st, unit) == 2 + 1


@pytest.mark.card("GD02-091")
@pytest.mark.ruling("GD02-091:Q189")
def test_gd02_091_red_lv7_unit_damages_enemy_up_to_lv7() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    unit = sc.add(0, "GD03-041")  # Patulia: red Lv7
    lv7 = sc.add(1, "GD02-036")  # Qubeley: Lv7
    lv8 = sc.add(1, "GD03-034")  # GQuuuuuuX: Lv8
    pilot = sc.add(0, "GD02-091", Zone.HAND)
    st = sc.start()
    play(st, pilot, onto=unit)
    assert st.cards[lv7].damage == 1
    assert st.cards[lv8].damage == 0


@pytest.mark.card("GD02-091")
def test_gd02_091_non_red_unit_deals_no_damage() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    unit = sc.add(0, RAIDER_MA)  # blue Lv4
    enemy = sc.add(1, VANILLA)
    pilot = sc.add(0, "GD02-091", Zone.HAND)
    st = sc.start()
    play(st, pilot, onto=unit)
    assert st.cards[enemy].damage == 0


@pytest.mark.card("GD02-092")
@pytest.mark.rule("13-2-12-1")
def test_gd02_092_linked_attack_gives_new_une_unit_ap_plus_two() -> None:
    sc = Scenario()
    unit = sc.add(0, "GD02-037", pilot="GD02-092")  # Gundam Virsago, link [Shagia Frost]
    st = sc.start()
    assert ap(st, unit) == 4 + 1
    attack(st, unit)
    assert ap(st, unit) == 4 + 1 + 2


@pytest.mark.card("GD02-092")
def test_gd02_092_unlinked_unit_gets_no_bonus() -> None:
    sc = Scenario()
    unit = sc.add(0, VANILLA, pilot="GD02-092")
    st = sc.start()
    attack(st, unit)
    assert ap(st, unit) == 2 + 1


def _olba_attack(enemy_pilot: str | None, enemy_unit: str = JENICE) -> tuple[GameState, int, int]:
    sc = Scenario()
    unit = sc.add(0, VANILLA, pilot="GD02-093")  # 3/3
    enemy = sc.add(1, enemy_unit, rested=True, pilot=enemy_pilot)
    st = sc.start()
    hand = len(_hand(st))
    attack(st, unit, enemy)
    pass_all(st)
    return st, enemy, hand


@pytest.mark.card("GD02-093")
def test_gd02_093_destroying_newtype_paired_unit_draws() -> None:
    st, enemy, hand = _olba_attack(SAYLA)  # Jenice 1/2 + Sayla 1/1
    assert zone_of(st, enemy) is Zone.TRASH
    assert len(_hand(st)) == hand + 1


@pytest.mark.card("GD02-093")
def test_gd02_093_non_newtype_pilot_does_not_draw() -> None:
    st, enemy, hand = _olba_attack("GD02-086")  # Jerid Messa is (Titans)
    assert zone_of(st, enemy) is Zone.TRASH
    assert len(_hand(st)) == hand


@pytest.mark.card("GD02-093")
def test_gd02_093_unpaired_enemy_does_not_draw() -> None:
    st, enemy, hand = _olba_attack(None, VANILLA)
    assert zone_of(st, enemy) is Zone.TRASH
    assert len(_hand(st)) == hand


@pytest.mark.card("GD02-093")
@pytest.mark.ruling("GD02-093:Q190")
@pytest.mark.rule("8-5-3-2-3")
def test_gd02_093_draws_when_both_units_are_destroyed() -> None:
    st, enemy, hand = _olba_attack(SAYLA, VANILLA)  # 3/3 each: both destroyed
    assert zone_of(st, enemy) is Zone.TRASH
    assert numbers_in(st, 0, Zone.BATTLE) == []
    assert len(_hand(st)) == hand + 1


@pytest.mark.card("GD02-093")
def test_gd02_093_does_not_draw_during_opponents_turn() -> None:
    sc = Scenario(active=1)
    mine = sc.add(0, VANILLA, rested=True, pilot="GD02-093")  # 3/3
    enemy = sc.add(1, JENICE, pilot=SAYLA)  # 2/3
    st = sc.start()
    hand = len(_hand(st))
    attack(st, enemy, mine)
    pass_all(st)
    assert zone_of(st, enemy) is Zone.TRASH
    assert len(_hand(st)) == hand


def _garrod_scenario() -> tuple[Scenario, int, int, int]:
    sc = Scenario()
    sc.resources(0, 4)
    unit = sc.add(0, VANILLA)
    pilot = sc.add(0, "GD02-094", Zone.HAND)
    fodder = sc.add(0, VANILLA, Zone.HAND)
    sc.deck(0, "GD02-063", VANILLA, "GD02-114")  # Airmaster (Fighter Mode) is (Vulture)
    return sc, unit, pilot, fodder


@pytest.mark.card("GD02-094")
def test_gd02_094_discard_then_reveal_vulture_unit() -> None:
    sc, unit, pilot, fodder = _garrod_scenario()
    st = sc.start()
    top3 = st.zones[0][Zone.DECK][:3]
    play(st, pilot, onto=unit)
    yes(st)  # discard 1
    assert zone_of(st, fodder) is Zone.TRASH
    yes(st)  # reveal the (Vulture) Unit card
    assert zone_of(st, top3[0]) is Zone.HAND
    assert set(st.zones[0][Zone.DECK][-2:]) == {top3[1], top3[2]}


@pytest.mark.card("GD02-094")
@pytest.mark.rule("5-20-1")
def test_gd02_094_no_discard_means_no_look() -> None:
    sc, unit, pilot, fodder = _garrod_scenario()
    st = sc.start()
    deck = list(st.zones[0][Zone.DECK])
    play(st, pilot, onto=unit)
    no(st)
    assert st.pending is not None and st.pending.kind is DecisionKind.MAIN
    assert zone_of(st, fodder) is Zone.HAND
    assert st.zones[0][Zone.DECK] == deck


@pytest.mark.card("GD02-095")
@pytest.mark.rule("13-1-6-1")
def test_gd02_095_damaged_low_level_unit_cannot_be_blocked() -> None:
    sc = Scenario()
    unit = sc.add(0, VANILLA, damage=1, pilot="GD02-095")
    sc.add(1, BLOCKER)
    (shield,) = sc.shields(1, VANILLA)
    sc.resources(0, 4)
    sc.add(0, ACTION_STOPPER, Zone.HAND)
    st = sc.start()
    attack(st, unit)
    assert st.pending is not None and st.pending.kind is DecisionKind.ACTION_STEP
    assert "High-Maneuver" in keywords(st, unit)
    pass_all(st)
    assert zone_of(st, shield) is Zone.TRASH
    assert "High-Maneuver" not in keywords(st, unit)  # only during this battle


@pytest.mark.card("GD02-095")
def test_gd02_095_undamaged_unit_can_be_blocked() -> None:
    sc = Scenario()
    unit = sc.add(0, VANILLA, pilot="GD02-095")
    sc.add(1, BLOCKER)
    st = sc.start()
    attack(st, unit)
    assert st.pending is not None and st.pending.kind is DecisionKind.BLOCK


@pytest.mark.card("GD02-095")
def test_gd02_095_damaged_lv6_unit_can_be_blocked() -> None:
    sc = Scenario()
    unit = sc.add(0, "GD03-051", damage=1, pilot="GD02-095")  # Gundam X Divider: Lv6
    sc.add(1, BLOCKER)
    st = sc.start()
    attack(st, unit)
    assert st.pending is not None and st.pending.kind is DecisionKind.BLOCK


def _desil_scenario(n_resources: int, rested: int, trash: str) -> tuple[GameState, int, int]:
    sc = Scenario()
    sc.resources(0, n_resources, rested=rested)
    zedas = sc.add(0, "GD02-057")  # link [Desil Galette]
    pilot = sc.add(0, "GD02-096", Zone.HAND)
    (card,) = sc.trash(0, trash)
    sc.trash(0, "GD03-065")  # Zedas M: (Vagan) Lv3, not eligible
    st = sc.start()
    play(st, pilot, onto=zedas)
    return st, zedas, card


@pytest.mark.card("GD02-096")
def test_gd02_096_pays_cost_to_deploy_vagan_unit_from_trash() -> None:
    st, zedas, baqto = _desil_scenario(5, 0, BAQTO)
    assert st.pending is not None and st.pending.kind is DecisionKind.SELECT
    offered = {o.a for o in st.pending.options if o.kind is A.SELECT}
    assert offered == {baqto}
    select(st, baqto)
    assert zone_of(st, baqto) is Zone.BATTLE
    assert _active_resources(st) == 5 - 1 - 2
    assert any(h.kind == "cost_paid" and h.uid == zedas for h in st.history)


@pytest.mark.card("GD02-096")
def test_gd02_096_choice_is_optional() -> None:
    st, _, baqto = _desil_scenario(5, 0, BAQTO)
    act(st, A.DONE)
    assert zone_of(st, baqto) is Zone.TRASH
    assert _active_resources(st) == 4


@pytest.mark.card("GD02-096")
def test_gd02_096_unpayable_cost_leaves_card_in_trash() -> None:
    st, _, baqto = _desil_scenario(5, 3, BAQTO)
    select(st, baqto)
    assert zone_of(st, baqto) is Zone.TRASH
    assert _active_resources(st) == 1


@pytest.mark.card("GD02-096")
def test_gd02_096_pays_the_cost_as_modified_in_the_trash() -> None:
    st, _, farsia = _desil_scenario(4, 2, "GD03-058")  # Farsia: cost 2, -1 in the trash
    select(st, farsia)
    assert zone_of(st, farsia) is Zone.BATTLE
    assert _active_resources(st) == 0


@pytest.mark.card("GD02-097")
def test_gd02_097_ap_plus_two_with_friendly_white_base() -> None:
    sc = Scenario()
    unit = sc.add(0, VANILLA, pilot="GD02-097")
    sc.base(0, "GD02-129")  # Argama: white Base
    st = sc.start()
    assert ap(st, unit) == 2 + 1 + 2


@pytest.mark.card("GD02-097")
def test_gd02_097_no_bonus_with_ex_base_or_enemy_white_base() -> None:
    sc = Scenario()
    unit = sc.add(0, VANILLA, pilot="GD02-097")
    sc.base(0)
    sc.base(1, "GD02-129")
    st = sc.start()
    assert ap(st, unit) == 2 + 1


@pytest.mark.card("GD02-098")
@pytest.mark.ruling("GD02-098:Q191")
@pytest.mark.rule("2-2-4", "3-2-6-2")
def test_gd02_098_links_with_char_aznable_link_unit() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    zaku = sc.add(0, "ST03-006")  # Char's Zaku II, link [Char Aznable]
    pilot = sc.add(0, "GD02-098", Zone.HAND)
    sc.add(0, VANILLA, Zone.HAND)
    st = sc.start()
    hand = len(_hand(st))
    play(st, pilot, onto=zaku)
    assert V.is_linked(V.derived(st), zaku)
    assert len(_hand(st)) == hand - 1  # a (Zeon) Unit: no draw and no discard
    assert st.pending is not None and st.pending.kind is DecisionKind.MAIN


@pytest.mark.card("GD02-098")
def test_gd02_098_linked_aeug_unit_draws_then_discards() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    hyaku = sc.add(0, "GD02-072")  # Hyaku-Shiki (AEUG), link [Quattro Bajeena]
    pilot = sc.add(0, "GD02-098", Zone.HAND)
    kept = sc.add(0, VANILLA, Zone.HAND)
    st = sc.start()
    play(st, pilot, onto=hyaku)
    assert st.pending is not None and st.pending.kind is DecisionKind.DISCARD
    select(st, kept)
    assert zone_of(st, kept) is Zone.TRASH
    assert len(_hand(st)) == 1


@pytest.mark.card("GD02-099")
def test_gd02_099_four_gjallarhorn_cards_give_enemy_ap_minus_two() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    unit = sc.add(0, VANILLA)
    enemy = sc.add(1, BLOCKER)
    sc.trash(0, "ST05-009", "ST05-009", "ST05-009", "GD02-119")  # Graze x3, a (Gjallarhorn) Command
    pilot = sc.add(0, "GD02-099", Zone.HAND)
    st = sc.start()
    play(st, pilot, onto=unit)
    assert ap(st, enemy) == 3 - 2
    to_next_turn(st)
    assert ap(st, enemy) == 3


@pytest.mark.card("GD02-099")
def test_gd02_099_three_gjallarhorn_cards_do_nothing() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    unit = sc.add(0, VANILLA)
    enemy = sc.add(1, BLOCKER)
    sc.trash(0, "ST05-009", "ST05-009", "ST05-009", VANILLA)
    pilot = sc.add(0, "GD02-099", Zone.HAND)
    st = sc.start()
    play(st, pilot, onto=unit)
    assert ap(st, enemy) == 3


# ---------------------------------------------------------------------------------------------
# Commands


@pytest.mark.card("GD02-100")
def test_gd02_100_recovers_two_hp_then_draws() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    unit = sc.add(0, BLOCKER, damage=3)
    cmd = sc.add(0, "GD02-100", Zone.HAND)
    st = sc.start()
    hand = len(_hand(st))
    play(st, cmd)
    assert st.cards[unit].damage == 1
    assert len(_hand(st)) == hand - 1 + 1
    assert zone_of(st, cmd) is Zone.TRASH


@pytest.mark.card("GD02-100")
@pytest.mark.rule("10-1-8-1-1")
def test_gd02_100_needs_a_damaged_friendly_unit() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    sc.add(0, BLOCKER)
    sc.add(1, BLOCKER, damage=1)
    cmd = sc.add(0, "GD02-100", Zone.HAND)
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, cmd)


@pytest.mark.card("GD02-100")
def test_gd02_100_burst_draws_one() -> None:
    sc = Scenario()
    (shield,) = sc.shields(1, "GD02-100")
    st, _ = _attack_into_burst(sc)
    hand = len(_hand(st, 1))
    yes(st)
    assert len(_hand(st, 1)) == hand + 1
    assert zone_of(st, shield) is Zone.TRASH


@pytest.mark.card("GD02-101")
def test_gd02_101_rests_up_to_two_enemy_units_lv2_or_lower() -> None:
    sc = Scenario()
    sc.resources(0, 1)
    a = sc.add(1, VANILLA)
    b = sc.add(1, HIZACK)
    big = sc.add(1, BLOCKER)
    cmd = sc.add(0, "GD02-101", Zone.HAND)
    st = sc.start()
    play(st, cmd)
    assert st.pending is not None
    assert {o.a for o in st.pending.options if o.kind is A.SELECT} == {a, b}
    select(st, a, b)
    assert st.cards[a].rested and st.cards[b].rested
    assert not st.cards[big].rested


@pytest.mark.card("GD02-101")
@pytest.mark.rule("9-3-1")
def test_gd02_101_can_rest_one_unit_as_an_action() -> None:
    sc = Scenario(active=1)
    sc.resources(0, 1)
    attacker = sc.add(1, VANILLA)
    other = sc.add(1, VANILLA)
    sc.shields(0, VANILLA)
    cmd = sc.add(0, "GD02-101", Zone.HAND)
    st = _enemy_attacks(sc, attacker)
    act(st, A.PLAY_COMMAND, cmd)
    select(st, other, done=True)
    assert st.cards[other].rested


@pytest.mark.card("GD02-102")
def test_gd02_102_titans_unit_gets_ap_plus_two() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    titans = sc.add(0, HIZACK)
    other = sc.add(0, VANILLA)
    cmd = sc.add(0, "GD02-102", Zone.HAND)
    st = sc.start()
    play(st, cmd)
    assert ap(st, titans) == 2 + 2
    assert ap(st, other) == 2


@pytest.mark.card("GD02-102")
@pytest.mark.rule("3-4-6-2")
def test_gd02_102_pairs_as_mouar_pharaoh() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    unit = sc.add(0, VANILLA)
    cmd = sc.add(0, "GD02-102", Zone.HAND)
    st = sc.start()
    play(st, cmd, onto=unit)
    assert zone_of(st, cmd) is Zone.PAIRED
    assert ap(st, unit) == 2 + 1


def _ex_resources(st: GameState, player: int = 0) -> int:
    ex = V.reg().db.ex_resource.card_number
    return sum(1 for u in st.zones[player][Zone.RESOURCE_AREA] if V.cdef(st, u).card_number == ex)


@pytest.mark.card("GD02-103")
def test_gd02_103_places_ex_resource_with_age_system_unit() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    sc.add(0, "GD02-029")  # Gundam AGE-1 Normal (AGE System)
    cmd = sc.add(0, "GD02-103", Zone.HAND)
    st = sc.start()
    play(st, cmd)
    assert _ex_resources(st) == 1


@pytest.mark.card("GD02-103")
def test_gd02_103_without_age_system_unit_places_nothing() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    sc.add(0, GENOACE)
    cmd = sc.add(0, "GD02-103", Zone.HAND)
    st = sc.start()
    play(st, cmd)
    assert _ex_resources(st) == 0
    assert zone_of(st, cmd) is Zone.TRASH


@pytest.mark.card("GD02-103")
def test_gd02_103_burst_adds_asuno_family_pilot_card_from_trash() -> None:
    sc = Scenario()
    sc.shields(1, "GD02-103")
    (flit,) = sc.trash(1, "GD02-088")
    st, _ = _attack_into_burst(sc)
    yes(st)
    assert zone_of(st, flit) is Zone.HAND


def _turning_point(pilot: str | None) -> tuple[GameState, list[int]]:
    sc = Scenario()
    sc.resources(0, 1)
    sc.add(0, VANILLA, pilot=pilot)
    cmd = sc.add(0, "GD02-104", Zone.HAND)
    sc.deck(0, GENOACE, BAQTO, JENICE)
    st = sc.start()
    top3 = st.zones[0][Zone.DECK][:3]
    play(st, cmd)
    assert st.pending is not None and st.pending.kind is DecisionKind.SELECT
    select(st, top3[1])
    assert st.pending is not None and st.pending.kind is DecisionKind.ARRANGE
    act(st, A.SELECT, top3[2])
    return st, top3


@pytest.mark.card("GD02-104")
@pytest.mark.rule("4-1-7")
def test_gd02_104_keeps_one_on_top_bottoms_rest_then_draws_with_newtype_pilot() -> None:
    st, (a, b, c) = _turning_point(SAYLA)
    assert zone_of(st, b) is Zone.HAND  # returned to the top, then drawn
    assert st.zones[0][Zone.DECK][-2:] == [c, a]


@pytest.mark.card("GD02-104")
def test_gd02_104_no_draw_without_newtype_pilot() -> None:
    st, (a, b, c) = _turning_point("GD02-086")  # Jerid Messa is not (Newtype)
    assert st.zones[0][Zone.DECK][0] == b
    assert st.zones[0][Zone.DECK][-2:] == [c, a]


@pytest.mark.card("GD02-105")
@pytest.mark.rule("9-3-1")
def test_gd02_105_token_takes_no_battle_damage_from_enemy_unit() -> None:
    sc = Scenario(active=1)
    sc.resources(0, 2)
    token = sc.add(0, TOKEN_GUNDAM, rested=True)
    attacker = sc.add(1, RAIDER_MA)  # 4/3
    cmd = sc.add(0, "GD02-105", Zone.HAND)
    st = _enemy_attacks(sc, attacker, token)
    act(st, A.PLAY_COMMAND, cmd)
    pass_all(st)
    assert zone_of(st, token) is Zone.BATTLE
    assert st.cards[token].damage == 0
    assert zone_of(st, attacker) is Zone.TRASH


@pytest.mark.card("GD02-105")
@pytest.mark.rule("10-1-8-1")
def test_gd02_105_cannot_be_played_in_the_main_phase() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    sc.add(0, TOKEN_GUNDAM)
    cmd = sc.add(0, "GD02-105", Zone.HAND)
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, cmd)


@pytest.mark.card("GD02-105")
@pytest.mark.rule("10-1-8-1-1")
def test_gd02_105_needs_a_unit_token() -> None:
    sc = Scenario(active=1)
    sc.resources(0, 4)
    mine = sc.add(0, VANILLA, rested=True)
    attacker = sc.add(1, RAIDER_MA)
    cmd = sc.add(0, "GD02-105", Zone.HAND)
    sc.add(0, ACTION_STOPPER, Zone.HAND)
    st = _enemy_attacks(sc, attacker, mine)
    assert not has_action(st, A.PLAY_COMMAND, cmd)


def _white_wolf(attacker_number: str) -> tuple[GameState, int]:
    sc = Scenario(active=1)
    sc.resources(0, 3)
    (shield,) = sc.shields(0, VANILLA)
    attacker = sc.add(1, attacker_number)
    cmd = sc.add(0, "GD02-106", Zone.HAND)
    st = _enemy_attacks(sc, attacker)
    act(st, A.PLAY_COMMAND, cmd)
    pass_all(st)
    return st, shield


@pytest.mark.card("GD02-106")
def test_gd02_106_shield_area_safe_from_lv3_or_lower_attacker() -> None:
    st, shield = _white_wolf(VANILLA)
    assert zone_of(st, shield) is Zone.SHIELD


@pytest.mark.card("GD02-106")
def test_gd02_106_lv4_attacker_still_destroys_shield() -> None:
    st, shield = _white_wolf(BLOCKER)
    assert zone_of(st, shield) is Zone.TRASH


@pytest.mark.card("GD02-106")
def test_gd02_106_base_is_protected_too() -> None:
    sc = Scenario(active=1)
    sc.resources(0, 3)
    base = sc.base(0, "GD02-129")
    attacker = sc.add(1, VANILLA)
    cmd = sc.add(0, "GD02-106", Zone.HAND)
    st = _enemy_attacks(sc, attacker)
    act(st, A.PLAY_COMMAND, cmd)
    pass_all(st)
    assert st.cards[base].damage == 0


@pytest.mark.card("GD02-106")
@pytest.mark.ruling("GD02-106:Q192")
@pytest.mark.rule("13-1-2-1")
def test_gd02_106_breach_damage_from_lv3_unit_is_prevented() -> None:
    sc = Scenario(active=1)
    sc.resources(0, 3)
    (shield,) = sc.shields(0, VANILLA)
    mine = sc.add(0, VANILLA, rested=True)  # 2/2
    rick_dom = sc.add(1, "GD01-030")  # Lv3 3/3 <Breach 2>
    cmd = sc.add(0, "GD02-106", Zone.HAND)
    st = _enemy_attacks(sc, rick_dom, mine)
    act(st, A.PLAY_COMMAND, cmd)
    pass_all(st)
    assert zone_of(st, mine) is Zone.TRASH
    assert zone_of(st, shield) is Zone.SHIELD


@pytest.mark.card("GD02-106")
def test_gd02_106_breach_control_without_white_wolf() -> None:
    sc = Scenario(active=1)
    (shield,) = sc.shields(0, VANILLA)
    mine = sc.add(0, VANILLA, rested=True)
    rick_dom = sc.add(1, "GD01-030")
    st = sc.start()
    attack(st, rick_dom, mine)
    pass_all(st)
    assert zone_of(st, shield) is Zone.TRASH


@pytest.mark.card("GD02-106")
@pytest.mark.rule("8-6-1", "7-6-6-1")
def test_gd02_106_played_in_end_phase_does_not_protect_later_battles() -> None:
    sc = Scenario(active=1)
    sc.resources(0, 3)
    (shield,) = sc.shields(0, VANILLA)
    attacker = sc.add(1, VANILLA)
    cmd = sc.add(0, "GD02-106", Zone.HAND)
    st = sc.start(Step.END_ACTION)
    act(st, A.PLAY_COMMAND, cmd)
    to_next_turn(st)
    assert (st.turn, st.active) == (5, 1)
    attack(st, attacker)
    pass_all(st)
    assert zone_of(st, shield) is Zone.TRASH


@pytest.mark.card("GD02-106")
@pytest.mark.rule("8-6-1")
def test_gd02_106_lasts_only_for_this_battle() -> None:
    sc = Scenario(active=1)
    sc.resources(0, 3)
    first, second = sc.shields(0, VANILLA, VANILLA)
    attacker = sc.add(1, VANILLA)
    attacker2 = sc.add(1, VANILLA)
    cmd = sc.add(0, "GD02-106", Zone.HAND)
    st = _enemy_attacks(sc, attacker)
    act(st, A.PLAY_COMMAND, cmd)
    pass_all(st)
    assert zone_of(st, first) is Zone.SHIELD
    attack(st, attacker2)
    pass_all(st)
    assert zone_of(st, first) is Zone.TRASH
    assert zone_of(st, second) is Zone.SHIELD


@pytest.mark.card("GD02-107")
def test_gd02_107_one_damage_to_all_enemy_units_other_than_link_units() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    link = sc.add(1, RAIDER_MA, pilot="GD02-087")  # linked through (Biological CPU)
    plain = sc.add(1, VANILLA)
    paired = sc.add(1, HIZACK, pilot="GD02-086")  # paired but not linked
    mine = sc.add(0, VANILLA)
    cmd = sc.add(0, "GD02-107", Zone.HAND)
    st = sc.start()
    play(st, cmd)
    assert st.cards[link].damage == 0
    assert st.cards[plain].damage == 1
    assert st.cards[paired].damage == 1
    assert st.cards[mine].damage == 0


@pytest.mark.card("GD02-107")
def test_gd02_107_burst_deals_one_damage_to_an_enemy_unit() -> None:
    sc = Scenario()
    sc.shields(1, "GD02-107")
    st, attacker = _attack_into_burst(sc)
    yes(st)
    assert st.cards[attacker].damage == 1


@pytest.mark.card("GD02-108")
@pytest.mark.rule("8-2-1")
def test_gd02_108_clan_unit_may_attack_active_lv4_or_lower() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    clan = sc.add(0, "GD03-032")  # Zaku [YETI] (GQ): (Clan)
    other = sc.add(0, VANILLA)
    lv4 = sc.add(1, BLOCKER)
    lv5 = sc.add(1, "GD02-057")
    cmd = sc.add(0, "GD02-108", Zone.HAND)
    st = sc.start()
    assert not has_action(st, A.ATTACK, clan, lv4)
    play(st, cmd)
    assert has_action(st, A.ATTACK, clan, lv4)
    assert not has_action(st, A.ATTACK, clan, lv5)
    assert not has_action(st, A.ATTACK, other, lv4)


@pytest.mark.card("GD02-109")
def test_gd02_109_deals_one_damage_to_enemy_unit() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    enemy = sc.add(1, BLOCKER)
    cmd = sc.add(0, "GD02-109", Zone.HAND)
    st = sc.start()
    play(st, cmd)
    assert st.cards[enemy].damage == 1


@pytest.mark.card("GD02-109")
def test_gd02_109_can_be_played_as_an_action() -> None:
    sc = Scenario(active=1)
    sc.resources(0, 4)
    sc.shields(0, VANILLA)
    attacker = sc.add(1, VANILLA)
    cmd = sc.add(0, "GD02-109", Zone.HAND)
    st = _enemy_attacks(sc, attacker)
    act(st, A.PLAY_COMMAND, cmd)
    assert st.cards[attacker].damage == 1


def _awakened_power(rested: int, *trash: str) -> tuple[GameState, list[int], int]:
    sc = Scenario()
    sc.resources(0, 8 if rested == 0 else 6, rested=rested)
    cards = sc.trash(0, *trash)
    cmd = sc.add(0, "GD02-110", Zone.HAND)
    st = sc.start()
    return st, cards, cmd


@pytest.mark.card("GD02-110")
@pytest.mark.ruling("GD02-110:Q193")
def test_gd02_110_pays_cost_to_deploy_and_deploy_effect_triggers() -> None:
    st, (barzam, divider), cmd = _awakened_power(0, "GD02-016", "GD03-051")
    play(st, cmd)
    assert zone_of(st, barzam) is Zone.BATTLE
    assert zone_of(st, divider) is Zone.TRASH  # Lv6 is not eligible
    assert ap(st, barzam) == 3 + 1  # its 【Deploy】: a (Titans) Unit gets AP+1
    assert _active_resources(st) == 8 - 2 - 2
    assert any(h.kind == "cost_paid" and h.uid == cmd for h in st.history)


@pytest.mark.card("GD02-110")
def test_gd02_110_unpayable_cost_leaves_unit_in_trash() -> None:
    st, (barzam,), cmd = _awakened_power(3, "GD02-016")
    play(st, cmd)
    assert zone_of(st, barzam) is Zone.TRASH
    assert _active_resources(st) == 1


@pytest.mark.card("GD02-110")
@pytest.mark.rule("10-1-8-1-1")
def test_gd02_110_needs_an_eligible_unit_card_in_trash() -> None:
    st, _, cmd = _awakened_power(0, "GD03-051")
    assert not has_action(st, A.PLAY_COMMAND, cmd)


def _last_resort(trash: list[str], enemies: int = 1) -> tuple[GameState, list[int], list[int], int]:
    sc = Scenario()
    sc.resources(0, 5)
    cards = sc.trash(0, *trash)
    foes = [sc.add(1, BLOCKER) for _ in range(enemies)]
    cmd = sc.add(0, "GD02-111", Zone.HAND)
    st = sc.start()
    return st, cards, foes, cmd


@pytest.mark.card("GD02-111")
@pytest.mark.ruling("GD02-111:Q194")
@pytest.mark.rule("5-12-1")
def test_gd02_111_exiles_six_purple_units_to_removal_area_and_destroys() -> None:
    st, cards, (enemy,), cmd = _last_resort([JENICE] * 6)
    play(st, cmd)
    assert all(zone_of(st, u) is Zone.REMOVAL for u in cards)
    assert zone_of(st, enemy) is Zone.TRASH
    assert zone_of(st, cmd) is Zone.TRASH


@pytest.mark.card("GD02-111")
@pytest.mark.rule("10-1-8-1-2")
def test_gd02_111_needs_six_purple_unit_cards() -> None:
    st, _, _, cmd = _last_resort([JENICE] * 5 + [VANILLA, "GD02-114"])
    assert not has_action(st, A.PLAY_COMMAND, cmd)


@pytest.mark.card("GD02-111")
@pytest.mark.rule("10-1-8-1-2")
def test_gd02_111_enemy_target_after_if_you_do_is_not_required() -> None:
    st, cards, _, cmd = _last_resort([JENICE] * 6, enemies=0)
    play(st, cmd)
    assert all(zone_of(st, u) is Zone.REMOVAL for u in cards)


@pytest.mark.card("GD02-111")
def test_gd02_111_chooses_exactly_six_of_seven() -> None:
    st, cards, (enemy,), cmd = _last_resort([JENICE] * 7)
    play(st, cmd)
    select(st, *cards[:6])
    assert [zone_of(st, u) for u in cards].count(Zone.REMOVAL) == 6
    assert zone_of(st, cards[6]) is Zone.TRASH
    assert zone_of(st, enemy) is Zone.TRASH


@pytest.mark.card("GD02-111")
def test_gd02_111_burst_deals_two_damage_to_lv3_or_lower() -> None:
    sc = Scenario()
    sc.shields(1, "GD02-111")
    st, attacker = _attack_into_burst(sc)
    yes(st)
    assert zone_of(st, attacker) is Zone.TRASH


@pytest.mark.card("GD02-112")
def test_gd02_112_adds_purple_pilot_card_from_trash() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    (pilot,) = sc.trash(0, "GD02-094")
    cmd = sc.add(0, "GD02-112", Zone.HAND)
    st = sc.start()
    play(st, cmd)
    assert zone_of(st, pilot) is Zone.HAND


@pytest.mark.card("GD02-112")
@pytest.mark.rule("3-4-6-3", "10-1-8-1-1")
def test_gd02_112_purple_command_with_pilot_is_not_a_pilot_card() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    sc.trash(0, "GD02-114", "GD02-088")  # purple Command with 【Pilot】; green Pilot
    cmd = sc.add(0, "GD02-112", Zone.HAND)
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, cmd)


@pytest.mark.card("GD02-112")
def test_gd02_112_burst_draws_one() -> None:
    sc = Scenario()
    sc.shields(1, "GD02-112")
    st, _ = _attack_into_burst(sc)
    hand = len(_hand(st, 1))
    yes(st)
    assert len(_hand(st, 1)) == hand + 1


def _sisterly_care(pilot: str | None) -> tuple[GameState, int, int]:
    sc = Scenario()
    sc.resources(0, 4)
    sc.add(0, "GD02-062", pilot=pilot)  # Amida's Hyakuren: link (Teiwaz)
    weak = sc.add(1, VANILLA)
    strong = sc.add(1, BLOCKER)
    cmd = sc.add(0, "GD02-113", Zone.HAND)
    st = sc.start()
    play(st, cmd)
    return st, weak, strong


@pytest.mark.card("GD02-113")
def test_gd02_113_destroys_enemy_with_two_or_less_ap_with_teiwaz_link_unit() -> None:
    st, weak, strong = _sisterly_care("GD02-095")  # Lafter Frankland is (Teiwaz)
    assert zone_of(st, weak) is Zone.TRASH
    assert zone_of(st, strong) is Zone.BATTLE


@pytest.mark.card("GD02-113")
def test_gd02_113_without_teiwaz_link_unit_destroys_nothing() -> None:
    st, weak, _ = _sisterly_care(None)
    assert zone_of(st, weak) is Zone.BATTLE


@pytest.mark.card("GD02-114")
def test_gd02_114_damaged_friendly_unit_gets_ap_plus_two() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    hurt = sc.add(0, VANILLA, damage=1)
    fresh = sc.add(0, VANILLA)
    cmd = sc.add(0, "GD02-114", Zone.HAND)
    st = sc.start()
    play(st, cmd)
    assert ap(st, hurt) == 4
    assert ap(st, fresh) == 2


@pytest.mark.card("GD02-114")
@pytest.mark.rule("10-1-8-1-1")
def test_gd02_114_needs_a_damaged_friendly_unit() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    sc.add(0, VANILLA)
    sc.add(1, VANILLA, damage=1)
    cmd = sc.add(0, "GD02-114", Zone.HAND)
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, cmd)


@pytest.mark.card("GD02-115")
def test_gd02_115_vulture_unit_gets_ap_plus_two() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    vulture = sc.add(0, JENICE)
    other = sc.add(0, VANILLA)
    cmd = sc.add(0, "GD02-115", Zone.HAND)
    st = sc.start()
    play(st, cmd)
    assert ap(st, vulture) == 1 + 2
    assert ap(st, other) == 2


def _comrades(trash_cards: int) -> tuple[GameState, int, int]:
    sc = Scenario()
    sc.resources(0, 3)
    vulture = sc.add(0, "GD02-063")
    enemy = sc.add(1, BLOCKER)
    sc.trash(0, *([VANILLA] * trash_cards))
    cmd = sc.add(0, "GD02-116", Zone.HAND)
    st = sc.start()
    play(st, cmd)
    return st, vulture, enemy


@pytest.mark.card("GD02-116")
def test_gd02_116_seven_trash_cards_let_vulture_attack_active_unit() -> None:
    st, vulture, enemy = _comrades(7)
    assert has_action(st, A.ATTACK, vulture, enemy)


@pytest.mark.card("GD02-116")
def test_gd02_116_resolving_command_is_not_counted_in_trash() -> None:
    st, vulture, enemy = _comrades(6)
    assert len(st.zones[0][Zone.TRASH]) == 7  # the Command itself arrived after resolving
    assert not has_action(st, A.ATTACK, vulture, enemy)


@pytest.mark.card("GD02-117")
@pytest.mark.rule("5-20-2")
def test_gd02_117_draws_three_then_discards_two() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    cmd = sc.add(0, "GD02-117", Zone.HAND)
    sc.add(0, VANILLA, Zone.HAND)
    st = sc.start()
    hand = len(_hand(st))
    play(st, cmd)
    for _ in range(2):
        assert st.pending is not None and st.pending.kind is DecisionKind.DISCARD
        act(st, A.SELECT, st.pending.options[0].a)
    assert len(_hand(st)) == hand - 1 + 3 - 2


@pytest.mark.card("GD02-117")
def test_gd02_117_burst_adds_aeug_base_card_from_trash() -> None:
    sc = Scenario()
    sc.shields(1, "GD02-117")
    (argama,) = sc.trash(1, "GD02-129")
    st, _ = _attack_into_burst(sc)
    yes(st)
    assert zone_of(st, argama) is Zone.HAND


def _revenge(attacker_number: str, damage: int = 0) -> tuple[GameState, int, int]:
    sc = Scenario(active=1)
    sc.resources(0, 4)
    blocker = sc.add(0, BLOCKER)
    attacker = sc.add(1, attacker_number, damage=damage)
    cmd = sc.add(0, "GD02-118", Zone.HAND)
    sc.add(0, ACTION_STOPPER, Zone.HAND)
    st = sc.start()
    attack(st, attacker)
    block(st, blocker)
    return st, attacker, cmd


@pytest.mark.card("GD02-118")
def test_gd02_118_returns_enemy_battling_friendly_blocker() -> None:
    st, attacker, cmd = _revenge(VANILLA)
    act(st, A.PLAY_COMMAND, cmd)
    assert zone_of(st, attacker) is Zone.HAND
    assert st.cards[attacker].owner == 1


@pytest.mark.card("GD02-118")
@pytest.mark.faq("Q96")
def test_gd02_118_hp_is_current_hp() -> None:
    st, attacker, cmd = _revenge("GD01-038")  # Adzam: 2/5
    assert not has_action(st, A.PLAY_COMMAND, cmd)
    st, attacker, cmd = _revenge("GD01-038", damage=1)
    act(st, A.PLAY_COMMAND, cmd)
    assert zone_of(st, attacker) is Zone.HAND


@pytest.mark.card("GD02-118")
@pytest.mark.rule("10-1-8-1-1")
def test_gd02_118_needs_enemy_battling_a_friendly_blocker() -> None:
    sc = Scenario(active=1)
    sc.resources(0, 4)
    sc.add(0, BLOCKER)
    sc.shields(0, VANILLA)
    attacker = sc.add(1, VANILLA)
    cmd = sc.add(0, "GD02-118", Zone.HAND)
    sc.add(0, ACTION_STOPPER, Zone.HAND)
    st = sc.start()
    attack(st, attacker)
    block(st, None)
    assert st.pending is not None and st.pending.kind is DecisionKind.ACTION_STEP
    assert not has_action(st, A.PLAY_COMMAND, cmd)


def _persistent(pilot: str | None) -> tuple[GameState, int]:
    sc = Scenario(active=1)
    sc.resources(0, 5)
    sc.add(0, "GD02-077", pilot=pilot)  # Ein's Schwalbe Graze: link (Gjallarhorn)
    sc.shields(0, VANILLA)
    attacker = sc.add(1, RAIDER_MA)  # 4 AP
    cmd = sc.add(0, "GD02-119", Zone.HAND)
    sc.add(0, ACTION_STOPPER, Zone.HAND)
    st = _enemy_attacks(sc, attacker)
    act(st, A.PLAY_COMMAND, cmd)
    return st, attacker


@pytest.mark.card("GD02-119")
@pytest.mark.rule("8-6-1")
def test_gd02_119_enemy_gets_ap_minus_three_during_this_battle() -> None:
    st, attacker = _persistent("GD02-099")  # Gaelio Bauduin is (Gjallarhorn)
    assert ap(st, attacker) == 1
    pass_all(st)
    assert ap(st, attacker) == 4


@pytest.mark.card("GD02-119")
def test_gd02_119_without_gjallarhorn_link_unit_does_nothing() -> None:
    st, attacker = _persistent(None)
    assert ap(st, attacker) == 4


@pytest.mark.card("GD02-119")
@pytest.mark.rule("8-6-1", "7-6-6-1")
def test_gd02_119_played_in_end_phase_does_not_outlive_the_turn() -> None:
    sc = Scenario(active=1)
    sc.resources(0, 2)
    sc.add(0, "GD02-077", pilot="GD02-099")
    enemy = sc.add(1, RAIDER_MA)
    cmd = sc.add(0, "GD02-119", Zone.HAND)
    st = sc.start(Step.END_ACTION)
    act(st, A.PLAY_COMMAND, cmd)
    assert st.pending is not None and st.pending.kind is DecisionKind.MAIN
    assert (st.turn, st.active) == (4, 0)
    assert ap(st, enemy) == 4


@pytest.mark.card("GD02-120")
def test_gd02_120_aeug_base_recovers_two_hp() -> None:
    sc = Scenario(active=1)
    sc.resources(0, 5)
    base = sc.base(0, "GD02-129", damage=3)
    attacker = sc.add(1, VANILLA)
    cmd = sc.add(0, "GD02-120", Zone.HAND)
    sc.add(0, ACTION_STOPPER, Zone.HAND)
    st = _enemy_attacks(sc, attacker)
    act(st, A.PLAY_COMMAND, cmd)
    assert st.cards[base].damage == 1
    pass_all(st)
    assert st.cards[base].damage == 1 + 2


@pytest.mark.card("GD02-120")
def test_gd02_120_aeug_unit_recovers_two_hp() -> None:
    sc = Scenario(active=1)
    sc.resources(0, 3)
    hyaku = sc.add(0, "GD02-072", damage=3, rested=True)  # Hyaku-Shiki (AEUG)
    other = sc.add(0, BLOCKER, damage=2, rested=True)
    sc.shields(0, VANILLA)
    attacker = sc.add(1, VANILLA)
    cmd = sc.add(0, "GD02-120", Zone.HAND)
    st = _enemy_attacks(sc, attacker)
    act(st, A.PLAY_COMMAND, cmd)
    assert st.cards[hyaku].damage == 1
    assert st.cards[other].damage == 2


# ---------------------------------------------------------------------------------------------
# Bases


@pytest.mark.card("GD02-121")
def test_gd02_121_deploy_adds_shield_then_blue_unit_recovers() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    blue = sc.add(0, RAIDER_MA, damage=2)
    red = sc.add(0, VANILLA, damage=1)
    (shield,) = sc.shields(0, GENOACE)
    base = sc.add(0, "GD02-121", Zone.HAND)
    st = sc.start()
    play(st, base)
    assert zone_of(st, base) is Zone.BASE
    assert zone_of(st, shield) is Zone.HAND
    assert st.cards[blue].damage == 0
    assert st.cards[red].damage == 1


@pytest.mark.card("GD02-121")
@pytest.mark.rule("5-20-2")
def test_gd02_121_then_part_resolves_without_a_shield() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    blue = sc.add(0, RAIDER_MA, damage=2)
    base = sc.add(0, "GD02-121", Zone.HAND)
    st = sc.start()
    play(st, base)
    assert st.cards[blue].damage == 0


@pytest.mark.card("GD02-122")
def test_gd02_122_deploy_damages_rested_enemy_lv4_or_lower() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    rested_low = sc.add(1, VANILLA, rested=True)
    active_low = sc.add(1, VANILLA)
    rested_high = sc.add(1, "GD02-057", rested=True)  # Lv5
    (shield,) = sc.shields(0, GENOACE)
    base = sc.add(0, "GD02-122", Zone.HAND)
    st = sc.start()
    play(st, base)
    assert zone_of(st, shield) is Zone.HAND
    assert st.cards[rested_low].damage == 1
    assert st.cards[active_low].damage == 0
    assert st.cards[rested_high].damage == 0


@pytest.mark.card("GD02-123")
def test_gd02_123_token_may_attack_active_enemy_with_5_or_less_ap() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    token = sc.add(0, "T-007")  # [Zaku Ⅱ] Unit token
    unit = sc.add(0, VANILLA)
    ap5 = sc.add(1, "GD02-057")  # Zedas: 5 AP
    ap6 = sc.add(1, "GD03-034")  # 6 AP
    (shield,) = sc.shields(0, GENOACE)
    base = sc.add(0, "GD02-123", Zone.HAND)
    st = sc.start()
    assert not has_action(st, A.ATTACK, token, ap5)
    play(st, base)
    assert zone_of(st, shield) is Zone.HAND
    assert has_action(st, A.ATTACK, token, ap5)
    assert not has_action(st, A.ATTACK, token, ap6)
    assert not has_action(st, A.ATTACK, unit, ap5)


def _diva(active: int, resources: int, ex: int = 0) -> tuple[GameState, int, int]:
    sc = Scenario(active=active)
    sc.resources(0, resources, ex=ex)
    sc.base(0, "GD02-124")
    green_ef = sc.add(0, GENOACE)
    red = sc.add(0, VANILLA)
    st = sc.start()
    return st, green_ef, red


@pytest.mark.card("GD02-124")
def test_gd02_124_lv7_your_turn_green_ef_units_get_ap_plus_one() -> None:
    st, green_ef, red = _diva(0, 7)
    assert ap(st, green_ef) == 1 + 1
    assert ap(st, red) == 2


@pytest.mark.card("GD02-124")
@pytest.mark.rule("2-9-4")
def test_gd02_124_ex_resources_count_toward_lv() -> None:
    st, green_ef, _ = _diva(0, 6, ex=1)
    assert ap(st, green_ef) == 1 + 1


@pytest.mark.card("GD02-124")
def test_gd02_124_below_lv7_no_bonus() -> None:
    st, green_ef, _ = _diva(0, 6)
    assert ap(st, green_ef) == 1


@pytest.mark.card("GD02-124")
def test_gd02_124_no_bonus_during_opponents_turn() -> None:
    st, green_ef, _ = _diva(1, 7)
    assert ap(st, green_ef) == 1


def _gwadan(*hand: str) -> tuple[GameState, int, list[int]]:
    sc = Scenario()
    sc.resources(0, 4)
    (shield,) = sc.shields(0, GENOACE)
    cards = sc.hand(0, *hand)
    base = sc.add(0, "GD02-125", Zone.HAND)
    st = sc.start()
    play(st, base)
    return st, shield, cards


@pytest.mark.card("GD02-125")
def test_gd02_125_your_turn_discard_red_card_to_draw() -> None:
    st, shield, (red,) = _gwadan(VANILLA)
    assert zone_of(st, shield) is Zone.HAND
    yes(st)
    assert zone_of(st, red) is Zone.TRASH
    assert len(_hand(st)) == 2  # the Shield and the drawn card


@pytest.mark.card("GD02-125")
def test_gd02_125_declining_the_discard_draws_nothing() -> None:
    st, _, (red,) = _gwadan(VANILLA)
    no(st)
    assert zone_of(st, red) is Zone.HAND
    assert len(_hand(st)) == 2


@pytest.mark.card("GD02-125")
@pytest.mark.rule("5-20-1")
def test_gd02_125_no_red_card_means_no_draw() -> None:
    st, _, (green,) = _gwadan(GENOACE)
    yes(st)
    assert zone_of(st, green) is Zone.HAND
    assert len(_hand(st)) == 2


@pytest.mark.card("GD02-125")
def test_gd02_125_burst_deploy_on_opponents_turn_neither_discards_nor_draws() -> None:
    sc = Scenario()
    sc.shields(1, "GD02-125", GENOACE)
    sc.add(1, VANILLA, Zone.HAND)
    st, _ = _attack_into_burst(sc)
    hand = len(_hand(st, 1))
    yes(st)
    assert st.pending is not None and st.pending.kind is DecisionKind.MAIN
    assert len(_hand(st, 1)) == hand + 1  # only the Shield


@pytest.mark.card("GD02-126")
@pytest.mark.rule("13-2-8-1")
def test_gd02_126_destroyed_deals_one_damage_to_lv4_or_lower_enemy() -> None:
    sc = Scenario(active=1)
    base = sc.base(0, "GD02-126", damage=4)
    attacker = sc.add(1, VANILLA)
    big = sc.add(1, "GD02-057")  # Lv5
    st = sc.start()
    attack(st, attacker)
    pass_all(st)
    assert zone_of(st, base) is Zone.TRASH
    assert st.cards[attacker].damage == 1
    assert st.cards[big].damage == 0


@pytest.mark.card("GD02-127")
def test_gd02_127_destroyed_mills_two() -> None:
    sc = Scenario(active=1)
    base = sc.base(0, "GD02-127", damage=4)
    attacker = sc.add(1, VANILLA)
    st = sc.start()
    deck = list(st.zones[0][Zone.DECK])
    attack(st, attacker)
    pass_all(st)
    assert zone_of(st, base) is Zone.TRASH
    assert st.zones[0][Zone.DECK] == deck[2:]
    assert all(zone_of(st, u) is Zone.TRASH for u in deck[:2])


def _hammerhead(pilot: str | None) -> tuple[GameState, int, int]:
    sc = Scenario()
    sc.resources(0, 4)
    sc.add(0, "GD02-062", pilot=pilot)  # Amida's Hyakuren: link (Teiwaz)
    weak = sc.add(1, VANILLA)
    strong = sc.add(1, BLOCKER)
    sc.shields(0, GENOACE)
    base = sc.add(0, "GD02-128", Zone.HAND)
    st = sc.start()
    play(st, base)
    return st, weak, strong


@pytest.mark.card("GD02-128")
def test_gd02_128_your_turn_with_teiwaz_link_unit_destroys_weak_enemy() -> None:
    st, weak, strong = _hammerhead("GD02-095")
    assert zone_of(st, weak) is Zone.TRASH
    assert zone_of(st, strong) is Zone.BATTLE


@pytest.mark.card("GD02-128")
def test_gd02_128_without_teiwaz_link_unit_destroys_nothing() -> None:
    st, weak, _ = _hammerhead(None)
    assert zone_of(st, weak) is Zone.BATTLE


@pytest.mark.card("GD02-128")
def test_gd02_128_burst_deploy_on_opponents_turn_destroys_nothing() -> None:
    sc = Scenario()
    sc.shields(1, "GD02-128", GENOACE)
    sc.add(1, "GD02-062", pilot="GD02-095")
    st, attacker = _attack_into_burst(sc)
    yes(st)
    assert zone_of(st, attacker) is Zone.BATTLE


@pytest.mark.card("GD02-129")
def test_gd02_129_enemy_effect_damage_is_prevented() -> None:
    sc = Scenario(active=1)
    argama = sc.base(0, "GD02-129")
    their_base = sc.base(1, "GD02-121")
    sc.resources(1, 7)
    patulia = sc.add(1, "GD03-041", Zone.HAND)  # 【Deploy】Deal 3 damage to all Bases.
    st = sc.start()
    play(st, patulia)
    assert st.cards[argama].damage == 0
    assert st.cards[their_base].damage == 3


@pytest.mark.card("GD02-129")
def test_gd02_129_friendly_effect_damage_is_received() -> None:
    sc = Scenario()
    argama = sc.base(0, "GD02-129")
    sc.resources(0, 7)
    patulia = sc.add(0, "GD03-041", Zone.HAND)
    st = sc.start()
    play(st, patulia)
    assert st.cards[argama].damage == 3


@pytest.mark.card("GD02-129")
def test_gd02_129_battle_damage_is_received() -> None:
    sc = Scenario(active=1)
    argama = sc.base(0, "GD02-129")
    attacker = sc.add(1, VANILLA)
    st = sc.start()
    attack(st, attacker)
    pass_all(st)
    assert st.cards[argama].damage == 2


@pytest.mark.card("GD02-129")
@pytest.mark.ruling("GD02-106:Q192")
def test_gd02_129_enemy_breach_damage_is_prevented() -> None:
    sc = Scenario(active=1)
    argama = sc.base(0, "GD02-129")
    mine = sc.add(0, VANILLA, rested=True)
    rick_dom = sc.add(1, "GD01-030")  # <Breach 2>
    st = sc.start()
    attack(st, rick_dom, mine)
    pass_all(st)
    assert zone_of(st, mine) is Zone.TRASH
    assert st.cards[argama].damage == 0


def _sleipnir(friend: str) -> tuple[GameState, int]:
    sc = Scenario()
    sc.resources(0, 4)
    sc.add(0, friend)
    enemy = sc.add(1, BLOCKER)
    sc.shields(0, GENOACE)
    base = sc.add(0, "GD02-130", Zone.HAND)
    st = sc.start()
    play(st, base)
    return st, enemy


@pytest.mark.card("GD02-130")
def test_gd02_130_with_gjallarhorn_unit_enemy_gets_ap_minus_two_this_turn() -> None:
    st, enemy = _sleipnir("ST05-009")  # Graze (Gjallarhorn)
    assert ap(st, enemy) == 3 - 2
    to_next_turn(st)
    assert ap(st, enemy) == 3


@pytest.mark.card("GD02-130")
def test_gd02_130_without_gjallarhorn_unit_does_nothing() -> None:
    st, enemy = _sleipnir(VANILLA)
    assert ap(st, enemy) == 3
