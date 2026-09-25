"""Card behaviour tests for the starter decks ST01-ST04 (work package WP-ST01-04)."""

from __future__ import annotations

import pytest

from gcg_sim.engine import view as V
from gcg_sim.engine.game import SUPPORT_AID
from gcg_sim.engine.state import GameState
from gcg_sim.engine.types import PLAYER_TARGET, ActionKind, DecisionKind, Zone
from gcg_sim.testkit import (
    Scenario,
    act,
    activate,
    ap,
    attack,
    block,
    card_numbers,
    choose_option,
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
    uids_in,
    yes,
    zone_of,
)

A = ActionKind

FILLER = "GD01-060"  # Zaku Mariner: vanilla Lv2 2/2 (Zeon)
GM = "ST01-005"  # vanilla Lv2 2/2 (Earth Federation)
GUNCANNON = "ST01-003"  # vanilla Lv3 2/4, link [Kai Shiden]
AERIAL_BIT = "ST01-007"  # vanilla Lv4 3/4, link [Suletta Mercury]
SANDROCK = "ST02-004"  # vanilla Lv4 4/3
DRA_C = "ST03-005"  # vanilla Lv1 1/2 (Neo Zeon)
ZAKU_I = "ST03-007"  # vanilla Lv1 1/2 (Zeon)
MOEBIUS_ZERO = "ST04-003"  # vanilla Lv3 2/4
TALLGEESE = "ST02-006"  # Lv5 4/4
WING = "ST02-001"  # Lv6 4/5
SINANJU = "ST03-001"  # Lv6 5/4
GEARA_DOGA_NZ = "GD05-061"  # Lv2 3/1: <Blocker> while you have another (Neo Zeon) Unit
GEARA_DOGA_SLEEVES = "GD01-056"  # Lv3 2/3: 【Destroyed】1 damage to an enemy Unit with ≤5 AP


def settle(st: GameState) -> None:
    while st.pending is not None and st.pending.kind is DecisionKind.ORDER_TRIGGER:
        order(st)


def rested_resources(st: GameState, player: int) -> int:
    return sum(1 for u in st.zones[player][Zone.RESOURCE_AREA] if st.cards[u].rested)


def tokens_in_play(st: GameState, player: int) -> list[str]:
    return [V.cdef(st, u).name for u in st.zones[player][Zone.BATTLE] if V.cdef(st, u).is_token]


def token_uids(st: GameState, player: int) -> list[int]:
    return [u for u in st.zones[player][Zone.BATTLE] if V.cdef(st, u).is_token]


def burst_from_shield(sc: Scenario, burst_card: str, *, attacker: str = FILLER) -> GameState:
    """Player 0 attacks player 1, whose top Shield is ``burst_card``; stops at the Burst prompt."""
    unit = sc.add(0, attacker)
    sc.shields(1, burst_card, FILLER, FILLER)
    st = sc.start()
    attack(st, unit)
    pass_all(st)
    assert st.pending is not None and st.pending.kind is DecisionKind.BURST
    return st


def enemy_attacks_into(sc: Scenario, attacker: str, target: str) -> tuple[GameState, int, int]:
    """Player 0 attacks player 1's rested ``target``; stops at player 1's battle action step."""
    a = sc.add(0, attacker)
    t = sc.add(1, target, rested=True)
    st = sc.start()
    attack(st, a, t)
    assert st.pending is not None and st.pending.kind is DecisionKind.ACTION_STEP
    assert st.pending.player == 1
    return st, a, t


# ---------------------------------------------------------------------------------------------
# ST01


@pytest.mark.card("ST01-001")
@pytest.mark.rule("13-1-1-1")
def test_st01_001_repair_2_recovers_two_at_end_of_turn() -> None:
    sc = Scenario()
    gundam = sc.add(0, "ST01-001", damage=3)
    st = sc.start()
    assert keywords(st, gundam).get("Repair") == 2
    to_next_turn(st)
    assert st.cards[gundam].damage == 1


@pytest.mark.card("ST01-001")
@pytest.mark.ruling("ST01-001:Q113")
@pytest.mark.rule("13-2-10")
def test_st01_001_during_pair_all_your_units_get_ap_during_your_turn() -> None:
    sc = Scenario()
    gundam = sc.add(0, "ST01-001", pilot="ST01-010")
    gm = sc.add(0, GM)
    enemy = sc.add(1, GM)
    st = sc.start()
    assert ap(st, gundam) == 3 + 2 + 1  # Q113: the source Unit itself gets AP+1
    assert ap(st, gm) == 3
    assert ap(st, enemy) == 2
    to_next_turn(st)
    assert ap(st, gundam) == 5
    assert ap(st, gm) == 2


@pytest.mark.card("ST01-001")
@pytest.mark.rule("10-1-5")
def test_st01_001_constant_applies_to_units_deployed_later() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    sc.add(0, "ST01-001", pilot="ST01-010")
    gm = sc.add(0, GM, Zone.HAND)
    st = sc.start()
    play(st, gm)
    assert ap(st, gm) == 3


@pytest.mark.card("ST01-001")
def test_st01_001_unpaired_gives_no_ap() -> None:
    sc = Scenario()
    gundam = sc.add(0, "ST01-001")
    gm = sc.add(0, GM)
    st = sc.start()
    assert ap(st, gundam) == 3
    assert ap(st, gm) == 2


@pytest.mark.card("ST01-002")
@pytest.mark.rule("13-2-9-2")
def test_st01_002_white_base_team_pilot_draws() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    ma = sc.add(0, "ST01-002")
    amuro = sc.add(0, "ST01-010", Zone.HAND)
    st = sc.start()
    play(st, amuro, onto=ma)
    settle(st)
    assert len(st.zones[0][Zone.HAND]) == 1


@pytest.mark.card("ST01-002")
def test_st01_002_other_pilot_does_not_draw() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    ma = sc.add(0, "ST01-002")
    suletta = sc.add(0, "ST01-011", Zone.HAND)
    st = sc.start()
    play(st, suletta, onto=ma)
    settle(st)
    assert len(st.zones[0][Zone.HAND]) == 0


@pytest.mark.card("ST01-004")
@pytest.mark.faq("Q96")
def test_st01_004_deploy_rests_enemy_with_current_hp_2_or_less() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    guntank = sc.add(0, "ST01-004", Zone.HAND)
    damaged = sc.add(1, GUNCANNON, damage=2)  # 4 HP - 2 damage = 2 current HP
    healthy = sc.add(1, MOEBIUS_ZERO)  # 4 HP
    st = sc.start()
    play(st, guntank)
    assert st.cards[damaged].rested
    assert not st.cards[healthy].rested


@pytest.mark.card("ST01-004")
def test_st01_004_no_eligible_enemy_rests_nothing() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    guntank = sc.add(0, "ST01-004", Zone.HAND)
    enemy = sc.add(1, MOEBIUS_ZERO)
    st = sc.start()
    play(st, guntank)
    assert zone_of(st, guntank) is Zone.BATTLE
    assert not st.cards[enemy].rested


@pytest.mark.card("ST01-006")
def test_st01_006_when_paired_enemy_lv5_or_lower_gets_ap_minus_3_this_turn() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    aerial = sc.add(0, "ST01-006")
    suletta = sc.add(0, "ST01-011", Zone.HAND)
    lv5 = sc.add(1, TALLGEESE)
    lv6 = sc.add(1, WING)
    st = sc.start()
    play(st, suletta, onto=aerial)
    assert ap(st, lv5) == 1
    assert ap(st, lv6) == 4
    to_next_turn(st)
    assert ap(st, lv5) == 4


@pytest.mark.card("ST01-008", "ST02-008", "ST02-009", "ST04-004", "ST01-009", "ST04-001")
@pytest.mark.rule("13-1-4-1")
@pytest.mark.parametrize(
    "blocker_card", ["ST01-008", "ST02-008", "ST02-009", "ST04-004", "ST01-009", "ST04-001"]
)
def test_blocker_units_can_block(blocker_card: str) -> None:
    sc = Scenario()
    attacker = sc.add(0, FILLER)
    blocker = sc.add(1, blocker_card)
    (shield,) = sc.shields(1, FILLER)
    st = sc.start()
    assert keywords(st, blocker).get("Blocker") == 1
    attack(st, attacker)
    assert has_action(st, A.BLOCK, blocker)
    block(st, blocker)
    pass_all(st)
    assert any(h.kind == "block" and h.uid == blocker for h in st.history)
    assert zone_of(st, shield) is Zone.SHIELD
    assert st.cards[attacker].damage > 0 or zone_of(st, attacker) is Zone.TRASH


@pytest.mark.card("ST01-009")
@pytest.mark.ruling("ST01-009:Q114")
def test_st01_009_cannot_attack_player_base_or_shields() -> None:
    sc = Scenario()
    zowort = sc.add(0, "ST01-009")
    target = sc.add(1, FILLER, rested=True)
    sc.base(1)
    sc.shields(1, FILLER)
    st = sc.start()
    assert not has_action(st, A.ATTACK, zowort, PLAYER_TARGET)
    assert has_action(st, A.ATTACK, zowort, target)


@pytest.mark.card("ST01-010", "ST01-011", "ST02-010", "ST02-011", "ST03-010", "ST03-011")
@pytest.mark.card("ST04-010", "ST04-011")
@pytest.mark.rule("13-2-5-1")
@pytest.mark.parametrize(
    "pilot",
    [
        "ST01-010",
        "ST01-011",
        "ST02-010",
        "ST02-011",
        "ST03-010",
        "ST03-011",
        "ST04-010",
        "ST04-011",
    ],
)
def test_pilot_burst_adds_to_hand(pilot: str) -> None:
    st = burst_from_shield(Scenario(), pilot)
    yes(st)
    assert card_numbers(st, uids_in(st, 1, Zone.HAND)) == [pilot]


@pytest.mark.card("ST01-010")
def test_st01_010_when_paired_rests_enemy_with_5_or_less_hp() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    gm = sc.add(0, GM)
    amuro = sc.add(0, "ST01-010", Zone.HAND)
    five_hp = sc.add(1, WING)
    seven_hp = sc.add(1, WING, pilot="ST02-010")  # Heero linked: 5 + 1 + 1 HP
    st = sc.start()
    assert hp(st, seven_hp) == 7
    play(st, amuro, onto=gm)
    assert st.cards[five_hp].rested
    assert not st.cards[seven_hp].rested


@pytest.mark.card("ST01-010")
@pytest.mark.faq("Q96")
def test_st01_010_when_paired_uses_current_hp() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    gm = sc.add(0, GM)
    amuro = sc.add(0, "ST01-010", Zone.HAND)
    damaged = sc.add(1, WING, pilot="ST02-010", damage=2)  # 7 HP - 2 damage
    st = sc.start()
    play(st, amuro, onto=gm)
    assert st.cards[damaged].rested


@pytest.mark.card("ST01-011")
@pytest.mark.ruling("ST01-011:Q142", "ST01-011:Q143")
def test_st01_011_attack_may_choose_active_or_ex_resource() -> None:
    sc = Scenario()
    unit = sc.add(0, AERIAL_BIT, pilot="ST01-011")
    rested = sc.resources(0, 2, rested=2)
    active = sc.resources(0, 1)
    ex = sc.add(0, sc.db.ex_resource.card_number, Zone.RESOURCE_AREA, rested=True)
    sc.shields(1, FILLER)
    st = sc.start()
    attack(st, unit)
    assert st.pending is not None and st.pending.kind is DecisionKind.SELECT
    choices = {o.a for o in options(st) if o.kind is A.SELECT}
    assert choices == {*rested, *active, ex}  # Q142: active Resources; Q143: EX Resources
    select(st, ex)
    assert not st.cards[ex].rested
    assert rested_resources(st, 0) == 2


@pytest.mark.card("ST01-011", "ST02-006")
@pytest.mark.rule("13-2-7-1", "13-2-13-1")
def test_st01_011_attack_effect_once_per_turn() -> None:
    sc = Scenario()
    tallgeese = sc.add(0, TALLGEESE, pilot="ST01-011")
    res = sc.resources(0, 5, rested=1)
    sc.shields(1, FILLER, FILLER)
    st = sc.start()
    attack(st, tallgeese)
    select(st, res[0])
    pass_all(st)
    assert rested_resources(st, 0) == 0
    activate(st, tallgeese)
    assert not st.cards[tallgeese].rested
    assert rested_resources(st, 0) == 4
    attack(st, tallgeese)
    assert st.pending is not None and st.pending.kind is DecisionKind.MAIN  # no choice offered
    assert rested_resources(st, 0) == 4


@pytest.mark.card("ST01-012")
def test_st01_012_deals_1_damage_to_rested_enemy() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    cmd = sc.add(0, "ST01-012", Zone.HAND)
    rested = sc.add(1, GUNCANNON, rested=True)
    active = sc.add(1, GUNCANNON)
    st = sc.start()
    play(st, cmd)
    assert st.cards[rested].damage == 1
    assert st.cards[active].damage == 0
    assert zone_of(st, cmd) is Zone.TRASH


@pytest.mark.card("ST01-012")
@pytest.mark.rule("10-1-8-1-1")
def test_st01_012_unplayable_without_rested_enemy_but_pairable() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    cmd = sc.add(0, "ST01-012", Zone.HAND)
    mine = sc.add(0, GM)
    sc.add(1, GUNCANNON)
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, cmd)
    assert has_action(st, A.PAIR, cmd, mine)


@pytest.mark.card("ST01-013")
def test_st01_013_friendly_unit_recovers_3() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    cmd = sc.add(0, "ST01-013", Zone.HAND)
    unit = sc.add(0, WING, damage=4)  # 4/5
    st = sc.start()
    play(st, cmd)
    assert st.cards[unit].damage == 1


@pytest.mark.card("ST01-013")
@pytest.mark.ruling("ST01-013:Q159", "ST01-013:Q160")
@pytest.mark.rule("5-6-3")
def test_st01_013_undamaged_unit_can_be_chosen_and_hp_does_not_increase() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    cmd = sc.add(0, "ST01-013", Zone.HAND)
    damaged = sc.add(0, GUNCANNON, damage=1)
    undamaged = sc.add(0, GUNCANNON)
    st = sc.start()
    play(st, cmd)
    assert st.pending is not None and st.pending.kind is DecisionKind.SELECT
    assert has_action(st, A.SELECT, undamaged)  # Q159
    select(st, undamaged)
    assert st.cards[undamaged].damage == 0
    assert hp(st, undamaged) == 4  # Q160: no HP beyond the printed value
    assert st.cards[damaged].damage == 1


@pytest.mark.card("ST01-014")
def test_st01_014_main_enemy_gets_ap_minus_3_this_turn() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    cmd = sc.add(0, "ST01-014", Zone.HAND)
    enemy = sc.add(1, SANDROCK)
    st = sc.start()
    play(st, cmd)
    assert ap(st, enemy) == 1
    to_next_turn(st)
    assert ap(st, enemy) == 4


@pytest.mark.card("ST01-014")
@pytest.mark.rule("13-2-5-1")
def test_st01_014_burst_activates_main_on_the_attacker() -> None:
    st = burst_from_shield(Scenario(), "ST01-014", attacker=SANDROCK)
    attacker = st.zones[0][Zone.BATTLE][0]
    yes(st)
    assert ap(st, attacker) == 1
    assert "ST01-014" in card_numbers(st, st.zones[1][Zone.TRASH])


@pytest.mark.card("ST01-014")
def test_st01_014_playable_in_action_step() -> None:
    sc = Scenario()
    sc.resources(1, 3)
    cmd = sc.add(1, "ST01-014", Zone.HAND)
    st, attacker, target = enemy_attacks_into(sc, SANDROCK, GUNCANNON)
    act(st, A.PLAY_COMMAND, cmd)
    assert ap(st, attacker) == 1
    pass_all(st)
    assert st.cards[target].damage == 1


@pytest.mark.card("ST01-015", "ST01-016", "ST02-015", "ST02-016")
@pytest.mark.card("ST03-015", "ST03-016", "ST04-015", "ST04-016")
@pytest.mark.rule("13-2-5-1", "13-2-6")
@pytest.mark.parametrize(
    "base_card",
    [
        "ST01-015",
        "ST01-016",
        "ST02-015",
        "ST02-016",
        "ST03-015",
        "ST03-016",
        "ST04-015",
        "ST04-016",
    ],
)
def test_base_burst_deploys_and_adds_a_shield_to_hand(base_card: str) -> None:
    st = burst_from_shield(Scenario(), base_card)
    yes(st)
    settle(st)
    base = st.zones[1][Zone.BASE]
    assert card_numbers(st, base) == [base_card]
    assert len(st.zones[1][Zone.SHIELD]) == 1  # 3 Shields - 1 destroyed - 1 to hand
    assert len(st.zones[1][Zone.HAND]) == 1
    assert tokens_in_play(st, 1) == []  # 'if it is your turn' token parts do nothing here


@pytest.mark.card("ST01-015")
def test_st01_015_deploy_from_hand_adds_shield() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    wb = sc.add(0, "ST01-015", Zone.HAND)
    (shield,) = sc.shields(0, FILLER)
    st = sc.start()
    play(st, wb)
    assert zone_of(st, wb) is Zone.BASE
    assert zone_of(st, shield) is Zone.HAND


@pytest.mark.card("ST01-015")
@pytest.mark.parametrize(
    ("n_units", "token", "stats"),
    [
        (0, "Gundam", (3, 3)),
        (1, "Guncannon", (2, 2)),
        (2, "Guntank", (1, 1)),
        (3, "Guntank", (1, 1)),
    ],
)
def test_st01_015_activate_token_depends_on_unit_count(
    n_units: int, token: str, stats: tuple[int, int]
) -> None:
    sc = Scenario()
    wb = sc.base(0, "ST01-015")
    for _ in range(n_units):
        sc.add(0, GM)
    sc.resources(0, 2)
    st = sc.start()
    activate(st, wb)
    (tok,) = token_uids(st, 0)
    assert V.cdef(st, tok).name == token
    assert (ap(st, tok), hp(st, tok)) == stats
    assert V.cdef(st, tok).traits == ("White Base Team",)
    assert not st.cards[tok].rested
    assert rested_resources(st, 0) == 2


@pytest.mark.card("ST01-015")
@pytest.mark.rule("13-2-1", "13-2-13-1")
def test_st01_015_activate_once_per_turn() -> None:
    sc = Scenario()
    wb = sc.base(0, "ST01-015")
    sc.resources(0, 4)
    st = sc.start()
    activate(st, wb)
    assert not has_action(st, A.ACTIVATE, wb)
    to_next_turn(st)
    to_next_turn(st)
    assert has_action(st, A.ACTIVATE, wb)


@pytest.mark.card("ST01-016")
def test_st01_016_rest_gives_link_units_ap_plus_1() -> None:
    sc = Scenario()
    base = sc.base(0, "ST01-016")
    linked = sc.add(0, AERIAL_BIT, pilot="ST01-011")
    unlinked = sc.add(0, GM, pilot="ST01-011")
    st = sc.start()
    activate(st, base)
    assert st.cards[base].rested
    assert ap(st, linked) == 3 + 1 + 1
    assert ap(st, unlinked) == 2 + 1
    to_next_turn(st)
    assert ap(st, linked) == 4


@pytest.mark.card("ST01-016")
@pytest.mark.faq("Q105")
def test_st01_016_unit_linked_after_activation_gets_nothing() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    base = sc.base(0, "ST01-016")
    aerial = sc.add(0, AERIAL_BIT)
    suletta = sc.add(0, "ST01-011", Zone.HAND)
    st = sc.start()
    activate(st, base)
    play(st, suletta, onto=aerial)
    assert V.is_linked(V.derived(st), aerial)
    assert ap(st, aerial) == 3 + 1


@pytest.mark.card("ST01-016")
def test_st01_016_deploy_adds_shield() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    base = sc.add(0, "ST01-016", Zone.HAND)
    (shield,) = sc.shields(0, FILLER)
    st = sc.start()
    play(st, base)
    assert zone_of(st, shield) is Zone.HAND


# ---------------------------------------------------------------------------------------------
# ST02


@pytest.mark.card("ST02-001")
def test_st02_001_may_attack_active_enemy_lv4_or_lower() -> None:
    sc = Scenario()
    wing = sc.add(0, WING)
    lv4 = sc.add(1, SANDROCK)
    lv5 = sc.add(1, TALLGEESE)
    st = sc.start()
    assert has_action(st, A.ATTACK, wing, lv4)
    assert not has_action(st, A.ATTACK, wing, lv5)


@pytest.mark.card("ST02-001", "ST04-007")
@pytest.mark.rule("13-1-2")
@pytest.mark.parametrize(("unit", "amount"), [("ST02-001", 5), ("ST04-007", 3)])
def test_breach_damages_first_shield_area_card(unit: str, amount: int) -> None:
    sc = Scenario()
    attacker = sc.add(0, unit)
    victim = sc.add(1, FILLER, rested=True)
    base = sc.base(1)
    st = sc.start()
    assert keywords(st, attacker).get("Breach") == amount
    attack(st, attacker, victim)
    pass_all(st)
    assert zone_of(st, victim) is Zone.TRASH
    assert zone_of(st, base) is not Zone.BASE  # EX Base (3 HP) destroyed by Breach


@pytest.mark.card("ST02-002")
def test_st02_002_deploy_places_ex_resource() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    bird = sc.add(0, "ST02-002", Zone.HAND)
    st = sc.start()
    play(st, bird)
    area = card_numbers(st, st.zones[0][Zone.RESOURCE_AREA])
    assert area.count(sc.db.ex_resource.card_number) == 1
    assert len(area) == 4


@pytest.mark.card("ST02-003")
@pytest.mark.rule("13-2-10")
def test_st02_003_paired_destroying_enemy_damages_low_level_enemies() -> None:
    sc = Scenario()
    heavyarms = sc.add(0, "ST02-003", pilot="ST02-012")
    victim = sc.add(1, FILLER, rested=True)
    low = sc.add(1, GM)
    high = sc.add(1, WING)
    sc.shields(1, FILLER)
    st = sc.start()
    attack(st, heavyarms, victim)
    pass_all(st)
    assert zone_of(st, victim) is Zone.TRASH
    assert st.cards[low].damage == 1
    assert st.cards[high].damage == 0


@pytest.mark.card("ST02-003")
def test_st02_003_unpaired_does_nothing() -> None:
    sc = Scenario()
    heavyarms = sc.add(0, "ST02-003")
    victim = sc.add(1, FILLER, rested=True)
    low = sc.add(1, GM)
    st = sc.start()
    attack(st, heavyarms, victim)
    pass_all(st)
    assert zone_of(st, victim) is Zone.TRASH
    assert st.cards[low].damage == 0


@pytest.mark.card("ST02-003")
@pytest.mark.ruling("ST02-003:Q115")
def test_st02_003_triggers_when_both_units_are_destroyed() -> None:
    sc = Scenario()
    heavyarms = sc.add(0, "ST02-003", pilot="ST02-012")  # 4/5
    victim = sc.add(1, SINANJU, rested=True)  # 5/4
    low = sc.add(1, GM)
    st = sc.start()
    attack(st, heavyarms, victim)
    pass_all(st)
    assert zone_of(st, heavyarms) is Zone.TRASH
    assert zone_of(st, victim) is Zone.TRASH
    assert st.cards[low].damage == 1


@pytest.mark.card("ST02-003")
def test_st02_003_not_during_opponent_turn() -> None:
    sc = Scenario(active=1)
    attacker = sc.add(1, FILLER)
    heavyarms = sc.add(0, "ST02-003", pilot="ST02-012", rested=True)
    low = sc.add(1, GM)
    st = sc.start()
    attack(st, attacker, heavyarms)
    pass_all(st)
    assert zone_of(st, attacker) is Zone.TRASH
    assert st.cards[low].damage == 0


@pytest.mark.card("ST02-006")
@pytest.mark.rule("13-2-1", "13-2-13-1")
def test_st02_006_pay_4_to_set_active_once_per_turn() -> None:
    sc = Scenario()
    tallgeese = sc.add(0, TALLGEESE, rested=True)
    sc.resources(0, 8)
    st = sc.start()
    activate(st, tallgeese)
    assert not st.cards[tallgeese].rested
    assert rested_resources(st, 0) == 4
    assert not has_action(st, A.ACTIVATE, tallgeese)


@pytest.mark.card("ST02-006")
def test_st02_006_needs_four_resources() -> None:
    sc = Scenario()
    tallgeese = sc.add(0, TALLGEESE, rested=True)
    sc.resources(0, 3)
    st = sc.start()
    assert not has_action(st, A.ACTIVATE, tallgeese)


@pytest.mark.card("ST02-010")
@pytest.mark.rule("13-2-12")
def test_st02_010_during_link_ap_and_hp_plus_1() -> None:
    sc = Scenario()
    linked = sc.add(0, WING, pilot="ST02-010")
    unlinked = sc.add(0, "ST02-005", pilot="ST02-010")  # Maganac 3/2
    st = sc.start()
    assert (ap(st, linked), hp(st, linked)) == (4 + 2 + 1, 5 + 1 + 1)
    assert (ap(st, unlinked), hp(st, unlinked)) == (3 + 2, 2 + 1)


@pytest.mark.card("ST02-011")
def test_st02_011_linked_destroys_enemy_draws() -> None:
    sc = Scenario()
    tallgeese = sc.add(0, TALLGEESE, pilot="ST02-011")
    victim = sc.add(1, FILLER, rested=True)
    st = sc.start()
    attack(st, tallgeese, victim)
    pass_all(st)
    assert zone_of(st, victim) is Zone.TRASH
    assert len(st.zones[0][Zone.HAND]) == 1


@pytest.mark.card("ST02-011")
def test_st02_011_unlinked_does_not_draw() -> None:
    sc = Scenario()
    sandrock = sc.add(0, SANDROCK, pilot="ST02-011")
    victim = sc.add(1, FILLER, rested=True)
    st = sc.start()
    attack(st, sandrock, victim)
    pass_all(st)
    assert zone_of(st, victim) is Zone.TRASH
    assert len(st.zones[0][Zone.HAND]) == 0


@pytest.mark.card("ST02-011")
@pytest.mark.ruling("ST02-011:Q144")
def test_st02_011_draws_when_both_units_are_destroyed() -> None:
    sc = Scenario()
    tallgeese = sc.add(0, TALLGEESE, pilot="ST02-011")  # 6/5 linked
    victim = sc.add(1, SINANJU, rested=True)  # 5/4
    st = sc.start()
    attack(st, tallgeese, victim)
    pass_all(st)
    assert zone_of(st, tallgeese) is Zone.TRASH
    assert zone_of(st, victim) is Zone.TRASH
    assert len(st.zones[0][Zone.HAND]) == 1


@pytest.mark.card("ST02-012")
def test_st02_012_friendly_unit_gains_breach_3_this_turn() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    cmd = sc.add(0, "ST02-012", Zone.HAND)
    gm = sc.add(0, GM)
    victim = sc.add(1, "ST03-005", rested=True)  # Dra-C 1/2
    top, second = sc.shields(1, FILLER, FILLER)
    st = sc.start()
    play(st, cmd)
    assert keywords(st, gm).get("Breach") == 3
    attack(st, gm, victim)
    pass_all(st)
    assert zone_of(st, victim) is Zone.TRASH
    assert zone_of(st, top) is Zone.TRASH
    assert zone_of(st, second) is Zone.SHIELD
    to_next_turn(st)
    assert "Breach" not in keywords(st, gm)


@pytest.mark.card("ST02-012", "ST04-007")
@pytest.mark.rule("13-1-2-5")
def test_st02_012_breach_adds_to_existing_breach() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    cmd = sc.add(0, "ST02-012", Zone.HAND)
    aegis_ma = sc.add(0, "ST04-007")
    st = sc.start()
    play(st, cmd)
    assert keywords(st, aegis_ma).get("Breach") == 6


@pytest.mark.card("ST02-013")
@pytest.mark.ruling("ST02-013:Q151")
def test_st02_013_protects_shield_area_from_battle_damage() -> None:
    sc = Scenario()
    sc.resources(1, 4)
    cmd = sc.add(1, "ST02-013", Zone.HAND)
    attacker = sc.add(0, SANDROCK)  # Lv4
    (shield,) = sc.shields(1, FILLER)
    st = sc.start()
    attack(st, attacker)
    act(st, A.PLAY_COMMAND, cmd)
    pass_all(st)
    assert zone_of(st, shield) is Zone.SHIELD
    assert st.winner is None


@pytest.mark.card("ST02-013")
@pytest.mark.ruling("ST02-013:Q151")
@pytest.mark.rule("13-1-2")
def test_st02_013_protects_shield_area_from_breach() -> None:
    sc = Scenario()
    sc.resources(1, 4)
    cmd = sc.add(1, "ST02-013", Zone.HAND)
    (shield,) = sc.shields(1, FILLER)
    st, _, victim = enemy_attacks_into(sc, "ST04-007", FILLER)  # Lv4, <Breach 3>
    act(st, A.PLAY_COMMAND, cmd)
    pass_all(st)
    assert zone_of(st, victim) is Zone.TRASH
    assert zone_of(st, shield) is Zone.SHIELD


@pytest.mark.card("ST02-013")
def test_st02_013_does_not_protect_from_lv5_units() -> None:
    sc = Scenario()
    sc.resources(1, 4)
    cmd = sc.add(1, "ST02-013", Zone.HAND)
    attacker = sc.add(0, TALLGEESE)  # Lv5
    top, _ = sc.shields(1, FILLER, FILLER)
    st = sc.start()
    attack(st, attacker)
    act(st, A.PLAY_COMMAND, cmd)
    pass_all(st)
    assert zone_of(st, top) is Zone.TRASH


@pytest.mark.card("ST02-013")
@pytest.mark.rule("8-2-3", "8-6-1", "9-1")
@pytest.mark.xfail(
    strict=True,
    reason="ENGINE: a 'this_battle' lasting created outside a battle never expires",
)
def test_st02_013_played_outside_a_battle_has_no_lasting_effect() -> None:
    sc = Scenario()
    sc.resources(1, 4)
    cmd = sc.add(1, "ST02-013", Zone.HAND)
    attacker = sc.add(0, SANDROCK)  # Lv4
    top, _ = sc.shields(1, FILLER, FILLER)
    st = sc.start()
    end_main(st)
    assert st.pending is not None and st.pending.kind is DecisionKind.ACTION_STEP
    act(st, A.PLAY_COMMAND, cmd)  # end-phase action step: there is no battle
    pass_all(st)
    to_next_turn(st)
    assert st.active == 0
    attack(st, attacker)
    pass_all(st)
    assert zone_of(st, top) is Zone.TRASH


@pytest.mark.card("ST02-013")
@pytest.mark.rule("13-2-3-1", "13-2-4-1")
def test_st02_013_action_only_not_in_main_phase() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    cmd = sc.add(0, "ST02-013", Zone.HAND)
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, cmd)


@pytest.mark.card("ST02-014")
@pytest.mark.faq("Q96")
def test_st02_014_rests_enemy_with_current_hp_5_or_less() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    cmd = sc.add(0, "ST02-014", Zone.HAND)
    healthy = sc.add(1, WING, pilot="ST02-010")  # 7 HP
    damaged = sc.add(1, WING, pilot="ST02-010", damage=2)  # 5 current HP
    st = sc.start()
    play(st, cmd)
    assert st.cards[damaged].rested
    assert not st.cards[healthy].rested


@pytest.mark.card("ST02-014")
@pytest.mark.rule("10-1-8-1-1")
def test_st02_014_unplayable_without_target() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    cmd = sc.add(0, "ST02-014", Zone.HAND)
    sc.add(1, WING, pilot="ST02-010")
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, cmd)


@pytest.mark.card("ST02-014")
@pytest.mark.rule("13-2-5-1")
def test_st02_014_burst_rests_an_enemy_unit() -> None:
    sc = Scenario()
    other = sc.add(0, GM)
    st = burst_from_shield(sc, "ST02-014", attacker=SANDROCK)
    attacker = next(u for u in st.zones[0][Zone.BATTLE] if u != other)
    yes(st)
    assert {o.a for o in options(st) if o.kind is A.SELECT} == {attacker, other}
    select(st, other)
    assert st.cards[other].rested


@pytest.mark.card("ST02-015")
def test_st02_015_deploy_adds_shield_then_top_and_bottom() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    base = sc.add(0, "ST02-015", Zone.HAND)
    (shield,) = sc.shields(0, FILLER)
    sc.deck(0, GM, ZAKU_I, DRA_C)
    st = sc.start()
    play(st, base)
    assert zone_of(st, shield) is Zone.HAND
    assert st.pending is not None and st.pending.kind is DecisionKind.SELECT
    deck = st.zones[0][Zone.DECK]
    gm, zaku = deck[0], deck[1]
    assert {o.a for o in options(st) if o.kind is A.SELECT} == {gm, zaku}
    select(st, gm)
    deck = st.zones[0][Zone.DECK]
    assert deck[0] == zaku
    assert deck[-1] == gm
    assert V.cdef(st, deck[1]).card_number == DRA_C


@pytest.mark.card("ST02-015")
@pytest.mark.rule("5-20-2")
def test_st02_015_looks_even_without_shields() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    base = sc.add(0, "ST02-015", Zone.HAND)
    sc.deck(0, GM, ZAKU_I)
    st = sc.start()
    play(st, base)
    assert st.pending is not None and st.pending.kind is DecisionKind.SELECT
    deck = st.zones[0][Zone.DECK]
    zaku = deck[1]
    select(st, zaku)
    assert st.zones[0][Zone.DECK][-1] == zaku


@pytest.mark.card("ST02-016")
def test_st02_016_your_turn_deploys_tallgeese_token() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    base = sc.add(0, "ST02-016", Zone.HAND)
    (shield,) = sc.shields(0, FILLER)
    st = sc.start()
    play(st, base)
    assert zone_of(st, shield) is Zone.HAND
    (tok,) = token_uids(st, 0)
    assert V.cdef(st, tok).name == "Tallgeese"
    assert (ap(st, tok), hp(st, tok)) == (4, 2)
    assert V.cdef(st, tok).traits == ("OZ",)
    assert not st.cards[tok].rested


@pytest.mark.card("ST02-016")
def test_st02_016_corsica_in_trash_deploys_two_leo_instead() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    base = sc.add(0, "ST02-016", Zone.HAND)
    sc.trash(0, "ST02-016")
    st = sc.start()
    play(st, base)
    assert sorted(tokens_in_play(st, 0)) == ["Leo", "Leo"]
    for tok in token_uids(st, 0):
        assert (ap(st, tok), hp(st, tok)) == (1, 1)


@pytest.mark.card("ST02-016")
def test_st02_016_other_trash_cards_do_not_count() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    base = sc.add(0, "ST02-016", Zone.HAND)
    sc.trash(0, "ST01-015", "ST02-015")
    sc.trash(1, "ST02-016")
    st = sc.start()
    play(st, base)
    assert tokens_in_play(st, 0) == ["Tallgeese"]


# ---------------------------------------------------------------------------------------------
# ST03


@pytest.mark.card("ST03-001")
@pytest.mark.rule("13-1-6", "13-2-10")
def test_st03_001_during_pair_gains_high_maneuver() -> None:
    sc = Scenario()
    paired = sc.add(0, SINANJU, pilot="ST03-010")
    unpaired = sc.add(0, SINANJU)
    sc.add(1, "ST01-008")  # a <Blocker>
    sc.shields(1, FILLER, FILLER)
    st = sc.start()
    assert "High-Maneuver" in keywords(st, paired)
    assert "High-Maneuver" not in keywords(st, unpaired)
    attack(st, paired)
    assert not any(h.kind == "block" for h in st.history)
    assert len(st.zones[1][Zone.SHIELD]) == 1


@pytest.mark.card("ST03-001")
@pytest.mark.ruling("ST03-001:Q164")
def test_st03_001_destroying_shield_deals_2_even_unpaired() -> None:
    sc = Scenario()
    sinanju = sc.add(0, SINANJU)
    enemy = sc.add(1, GUNCANNON)
    sc.shields(1, FILLER, FILLER)
    st = sc.start()
    attack(st, sinanju)
    pass_all(st)
    assert st.cards[enemy].damage == 2


@pytest.mark.card("ST03-001")
@pytest.mark.ruling("ST03-001:Q116")
def test_st03_001_destroying_base_also_triggers() -> None:
    sc = Scenario()
    sinanju = sc.add(0, SINANJU)
    enemy = sc.add(1, GUNCANNON)
    base = sc.base(1)
    (shield,) = sc.shields(1, FILLER)
    st = sc.start()
    attack(st, sinanju)
    pass_all(st)
    assert zone_of(st, base) is not Zone.BASE
    assert zone_of(st, shield) is Zone.SHIELD
    assert st.cards[enemy].damage == 2


@pytest.mark.card("ST03-001", "ST02-012")
@pytest.mark.rule("13-1-2")
def test_st03_001_shield_destroyed_by_breach_is_not_battle_damage() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    cmd = sc.add(0, "ST02-012", Zone.HAND)
    sinanju = sc.add(0, SINANJU)
    victim = sc.add(1, FILLER, rested=True)
    other = sc.add(1, GUNCANNON)
    top, _ = sc.shields(1, FILLER, FILLER)
    st = sc.start()
    play(st, cmd)
    assert keywords(st, sinanju).get("Breach") == 3
    attack(st, sinanju, victim)
    pass_all(st)
    assert zone_of(st, top) is Zone.TRASH  # <Breach 3> effect damage
    assert st.cards[other].damage == 0


@pytest.mark.card("ST03-001")
def test_st03_001_battle_with_unit_does_not_trigger() -> None:
    sc = Scenario()
    sinanju = sc.add(0, SINANJU)
    victim = sc.add(1, FILLER, rested=True)
    other = sc.add(1, GUNCANNON)
    st = sc.start()
    attack(st, sinanju, victim)
    pass_all(st)
    assert zone_of(st, victim) is Zone.TRASH
    assert st.cards[other].damage == 0


@pytest.mark.card("ST03-002", "ST03-004")
@pytest.mark.rule("13-1-3")
@pytest.mark.parametrize("supporter", ["ST03-002", "ST03-004"])
def test_support_2_gives_another_unit_ap_plus_2(supporter: str) -> None:
    sc = Scenario()
    unit = sc.add(0, supporter)
    other = sc.add(0, GM)
    st = sc.start()
    assert keywords(st, unit).get("Support") == 2
    activate(st, unit, SUPPORT_AID)
    assert st.cards[unit].rested
    assert ap(st, other) == 4
    to_next_turn(st)
    assert ap(st, other) == 2


@pytest.mark.card("ST03-006")
def test_st03_006_destroyed_adds_zeon_unit_and_bottoms_rest() -> None:
    sc = Scenario()
    zaku = sc.add(0, "ST03-006")
    victim = sc.add(1, FILLER, rested=True)
    sc.deck(0, ZAKU_I, GM, DRA_C)
    st = sc.start()
    zaku_i, gm, dra_c = st.zones[0][Zone.DECK][:3]
    attack(st, zaku, victim)
    pass_all(st)
    assert zone_of(st, zaku) is Zone.TRASH
    assert st.pending is not None and st.pending.kind is DecisionKind.YES_NO
    yes(st)
    assert {o.a for o in options(st) if o.kind is A.SELECT} == {zaku_i, dra_c}
    select(st, dra_c)
    assert zone_of(st, dra_c) is Zone.HAND
    deck = st.zones[0][Zone.DECK]
    assert set(deck[-2:]) == {zaku_i, gm}
    assert V.cdef(st, deck[0]).card_number == FILLER


@pytest.mark.card("ST03-006")
@pytest.mark.ruling("ST03-006:Q470")
def test_st03_006_look_is_mandatory_add_is_optional() -> None:
    sc = Scenario()
    zaku = sc.add(0, "ST03-006")
    victim = sc.add(1, FILLER, rested=True)
    sc.deck(0, ZAKU_I, GM, "ST01-012")
    st = sc.start()
    top3 = list(st.zones[0][Zone.DECK][:3])
    attack(st, zaku, victim)
    pass_all(st)
    assert all(st.cards[u].known & 1 for u in top3)  # looked at before any choice
    no(st)
    assert len(st.zones[0][Zone.HAND]) == 0
    assert set(st.zones[0][Zone.DECK][-3:]) == set(top3)


@pytest.mark.card("ST03-008")
def test_st03_008_attack_gets_ap_plus_2_this_turn() -> None:
    sc = Scenario()
    zaku = sc.add(0, "ST03-008")
    sc.shields(1, FILLER, FILLER)
    st = sc.start()
    attack(st, zaku)
    assert ap(st, zaku) == 3
    to_next_turn(st)
    assert ap(st, zaku) == 1


@pytest.mark.card("ST03-009")
@pytest.mark.rule("5-17")
def test_st03_009_deploy_deploys_rested_zaku_token() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    gouf = sc.add(0, "ST03-009", Zone.HAND)
    st = sc.start()
    play(st, gouf)
    (tok,) = token_uids(st, 0)
    assert V.cdef(st, tok).name == "Zaku Ⅱ"
    assert (ap(st, tok), hp(st, tok)) == (1, 1)
    assert V.cdef(st, tok).traits == ("Zeon",)
    assert st.cards[tok].rested


@pytest.mark.card("ST03-009")
@pytest.mark.ruling("ST03-009:Q117")
@pytest.mark.rule("11-4-2")
def test_st03_009_full_battle_area_trashes_a_unit_before_token() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    gouf = sc.add(0, "ST03-009", Zone.HAND)
    units = [sc.add(0, GM) for _ in range(5)]
    st = sc.start()
    play(st, gouf)
    assert st.pending is not None and st.pending.kind is DecisionKind.EXCESS
    assert {o.a for o in options(st)} == {*units, gouf}
    select(st, units[0])
    assert zone_of(st, units[0]) is Zone.TRASH
    assert len(st.zones[0][Zone.BATTLE]) == 6
    assert tokens_in_play(st, 0) == ["Zaku Ⅱ"]


@pytest.mark.card("ST03-010")
@pytest.mark.ruling("ST03-010:Q145", "ST03-010:Q146")
def test_st03_010_deploys_zeon_unit_free_and_its_deploy_triggers() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    gm = sc.add(0, GM)
    ff = sc.add(0, "ST03-010", Zone.HAND)
    gouf = sc.add(0, "ST03-009", Zone.HAND)  # (Zeon) Lv3
    sc.add(0, TALLGEESE, Zone.HAND)  # (OZ): not eligible
    sc.add(0, "ST03-001", Zone.HAND)  # (Neo Zeon) Lv6: not eligible
    st = sc.start()
    play(st, ff, onto=gm)
    yes(st)
    assert zone_of(st, gouf) is Zone.BATTLE
    assert rested_resources(st, 0) == 1  # Q145: only the Pilot's cost was paid
    assert tokens_in_play(st, 0) == ["Zaku Ⅱ"]  # Q146: 【Deploy】 of the deployed Unit


@pytest.mark.card("ST03-010")
def test_st03_010_deploy_is_optional() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    gm = sc.add(0, GM)
    ff = sc.add(0, "ST03-010", Zone.HAND)
    zaku_i = sc.add(0, ZAKU_I, Zone.HAND)
    st = sc.start()
    play(st, ff, onto=gm)
    no(st)
    assert zone_of(st, zaku_i) is Zone.HAND


@pytest.mark.card("ST03-011")
@pytest.mark.rule("13-1-6")
def test_st03_011_linked_attack_gains_ap_and_high_maneuver() -> None:
    sc = Scenario()
    zaku = sc.add(0, "ST03-006", pilot="ST03-011")  # link [Char Aznable]
    sc.add(1, "ST01-008")
    sc.shields(1, FILLER, FILLER)
    st = sc.start()
    assert ap(st, zaku) == 3 + 1
    attack(st, zaku)
    assert ap(st, zaku) == 5
    assert "High-Maneuver" in keywords(st, zaku)
    assert not any(h.kind == "block" for h in st.history)
    to_next_turn(st)
    assert ap(st, zaku) == 4
    assert "High-Maneuver" not in keywords(st, zaku)


@pytest.mark.card("ST03-011")
def test_st03_011_unlinked_attack_gains_only_ap() -> None:
    sc = Scenario()
    gm = sc.add(0, GM, pilot="ST03-011")
    blocker = sc.add(1, "ST01-008")
    sc.shields(1, FILLER, FILLER)
    st = sc.start()
    attack(st, gm)
    assert ap(st, gm) == 2 + 1 + 1
    assert "High-Maneuver" not in keywords(st, gm)
    assert has_action(st, A.BLOCK, blocker)


@pytest.mark.card("ST03-012")
def test_st03_012_friendly_unit_ap_plus_2_this_turn() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    cmd = sc.add(0, "ST03-012", Zone.HAND)
    gm = sc.add(0, GM)
    st = sc.start()
    play(st, cmd)
    assert ap(st, gm) == 4
    to_next_turn(st)
    assert ap(st, gm) == 2


@pytest.mark.card("ST03-013")
def test_st03_013_deals_2_damage_to_enemy() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    cmd = sc.add(0, "ST03-013", Zone.HAND)
    enemy = sc.add(1, GUNCANNON)
    st = sc.start()
    play(st, cmd)
    assert st.cards[enemy].damage == 2


@pytest.mark.card("ST03-013")
@pytest.mark.rule("13-2-5-1")
def test_st03_013_burst_damages_the_attacker() -> None:
    st = burst_from_shield(Scenario(), "ST03-013", attacker=SANDROCK)
    attacker = st.zones[0][Zone.BATTLE][0]
    yes(st)
    assert st.cards[attacker].damage == 2


@pytest.mark.card("ST03-014")
def test_st03_014_no_battle_damage_from_2_ap_enemy() -> None:
    sc = Scenario()
    sc.resources(1, 4)
    cmd = sc.add(1, "ST03-014", Zone.HAND)
    st, attacker, target = enemy_attacks_into(sc, FILLER, GUNCANNON)
    act(st, A.PLAY_COMMAND, cmd)
    pass_all(st)
    assert st.cards[target].damage == 0
    assert zone_of(st, attacker) is Zone.TRASH


@pytest.mark.card("ST03-014")
def test_st03_014_battle_damage_from_3_ap_enemy_still_dealt() -> None:
    sc = Scenario()
    sc.resources(1, 4)
    cmd = sc.add(1, "ST03-014", Zone.HAND)
    st, _, target = enemy_attacks_into(sc, "ST02-005", GUNCANNON)  # Maganac 3 AP
    act(st, A.PLAY_COMMAND, cmd)
    pass_all(st)
    assert st.cards[target].damage == 3


@pytest.mark.card("ST03-014")
@pytest.mark.rule("8-2-3", "8-6-1", "9-1")
@pytest.mark.xfail(
    strict=True,
    reason="ENGINE: a 'this_battle' lasting created outside a battle never expires",
)
def test_st03_014_played_outside_a_battle_has_no_lasting_effect() -> None:
    sc = Scenario()
    sc.resources(1, 4)
    cmd = sc.add(1, "ST03-014", Zone.HAND)
    attacker = sc.add(0, FILLER)  # 2 AP
    zowort = sc.add(1, "ST01-009")  # 3/2 <Blocker>
    sc.shields(1, FILLER)
    st = sc.start()
    end_main(st)
    act(st, A.PLAY_COMMAND, cmd)  # end-phase action step: there is no battle
    pass_all(st)
    to_next_turn(st)
    assert st.active == 0
    attack(st, attacker)
    block(st, zowort)
    pass_all(st)
    assert zone_of(st, zowort) is Zone.TRASH  # 2 battle damage from a 2-AP enemy Unit


@pytest.mark.card("ST03-014")
@pytest.mark.ruling("ST03-014:Q152")
def test_st03_014_effect_damage_from_low_ap_enemy_still_dealt() -> None:
    sc = Scenario()
    sc.resources(1, 4)
    cmd = sc.add(1, "ST03-014", Zone.HAND)
    st, attacker, target = enemy_attacks_into(sc, GEARA_DOGA_SLEEVES, SANDROCK)
    act(st, A.PLAY_COMMAND, cmd)
    pass_all(st)
    assert zone_of(st, attacker) is Zone.TRASH  # 4 battle damage from Sandrock
    assert st.cards[target].damage == 1  # only its 【Destroyed】 effect damage


@pytest.mark.card("ST03-015")
def test_st03_015_deploy_adds_shield_then_damages_enemy_with_5_or_less_ap() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    base = sc.add(0, "ST03-015", Zone.HAND)
    (shield,) = sc.shields(0, FILLER)
    five_ap = sc.add(1, SINANJU)
    seven_ap = sc.add(1, WING, pilot="ST02-010")
    st = sc.start()
    play(st, base)
    assert zone_of(st, shield) is Zone.HAND
    assert st.cards[five_ap].damage == 1
    assert st.cards[seven_ap].damage == 0


@pytest.mark.card("ST03-015")
@pytest.mark.rule("5-20-2")
def test_st03_015_damage_happens_without_shields() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    base = sc.add(0, "ST03-015", Zone.HAND)
    enemy = sc.add(1, GUNCANNON)
    st = sc.start()
    play(st, base)
    assert st.cards[enemy].damage == 1


@pytest.mark.card("ST03-016")
def test_st03_016_your_turn_deploys_rested_chars_zaku_token() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    base = sc.add(0, "ST03-016", Zone.HAND)
    (shield,) = sc.shields(0, FILLER)
    st = sc.start()
    play(st, base)
    assert zone_of(st, shield) is Zone.HAND
    (tok,) = token_uids(st, 0)
    assert V.cdef(st, tok).name == "Char's Zaku Ⅱ"
    assert (ap(st, tok), hp(st, tok)) == (3, 1)
    assert st.cards[tok].rested


# ---------------------------------------------------------------------------------------------
# ST04


@pytest.mark.card("ST04-001")
@pytest.mark.rule("13-2-9-2")
@pytest.mark.faq("Q96")
def test_st04_001_lv4_pilot_returns_enemy_with_current_hp_4_or_less() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    aile = sc.add(0, "ST04-001")
    kira = sc.add(0, "ST04-010", Zone.HAND)
    damaged = sc.add(1, WING, damage=1)  # 5 HP - 1 damage
    healthy = sc.add(1, WING)
    st = sc.start()
    play(st, kira, onto=aile)
    assert zone_of(st, damaged) is Zone.HAND
    assert zone_of(st, healthy) is Zone.BATTLE


@pytest.mark.card("ST04-001")
def test_st04_001_lower_level_pilot_does_not_trigger() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    aile = sc.add(0, "ST04-001")
    char = sc.add(0, "ST03-011", Zone.HAND)  # Lv3
    enemy = sc.add(1, GUNCANNON)
    st = sc.start()
    play(st, char, onto=aile)
    assert zone_of(st, enemy) is Zone.BATTLE


@pytest.mark.card("ST04-002")
@pytest.mark.rule("5-20-2")
def test_st04_002_deploy_draws_then_discards() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    strike = sc.add(0, "ST04-002", Zone.HAND)
    keep = sc.add(0, GM, Zone.HAND)
    sc.deck(0, ZAKU_I)
    st = sc.start()
    drawn = st.zones[0][Zone.DECK][0]
    play(st, strike)
    assert st.pending is not None and st.pending.kind is DecisionKind.DISCARD
    select(st, keep)
    assert zone_of(st, keep) is Zone.TRASH
    assert uids_in(st, 0, Zone.HAND) == [drawn]


@pytest.mark.card("ST04-006", "ST04-011")
def test_st04_006_attack_with_5_ap_deals_3_to_lv5_or_higher() -> None:
    sc = Scenario()
    aegis = sc.add(0, "ST04-006", pilot="ST04-011")  # 4 + 1 AP
    lv5 = sc.add(1, TALLGEESE)
    lv4 = sc.add(1, SANDROCK)
    sc.shields(1, FILLER, FILLER)
    st = sc.start()
    assert ap(st, aegis) == 5
    attack(st, aegis)
    assert st.cards[lv5].damage == 3
    assert st.cards[lv4].damage == 0


@pytest.mark.card("ST04-006")
def test_st04_006_attack_with_4_ap_does_nothing() -> None:
    sc = Scenario()
    aegis = sc.add(0, "ST04-006")
    lv5 = sc.add(1, TALLGEESE)
    sc.shields(1, FILLER, FILLER)
    st = sc.start()
    attack(st, aegis)
    pass_all(st)
    assert st.cards[lv5].damage == 0


@pytest.mark.card("ST04-006", "ST03-012")
@pytest.mark.ruling("ST04-006:Q118")
def test_st04_006_ap_raised_after_attack_does_not_activate() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    aegis = sc.add(0, "ST04-006")
    indignation = sc.add(0, "ST03-012", Zone.HAND)
    lv5 = sc.add(1, TALLGEESE)
    sc.shields(1, FILLER, FILLER)
    st = sc.start()
    attack(st, aegis)
    assert st.pending is not None and st.pending.kind is DecisionKind.ACTION_STEP
    assert st.pending.player == 0  # player 1 has no action and passed
    act(st, A.PLAY_COMMAND, indignation)
    assert ap(st, aegis) == 6
    pass_all(st)
    assert st.cards[lv5].damage == 0


@pytest.mark.card("ST04-009")
@pytest.mark.rule("13-2-8", "13-2-10")
def test_st04_009_paired_destroyed_with_another_link_unit_draws() -> None:
    sc = Scenario()
    ginn = sc.add(0, "ST04-009", pilot="ST04-014")  # 3/2
    sc.add(0, "ST04-006", pilot="ST04-011")  # linked Aegis
    victim = sc.add(1, SANDROCK, rested=True)
    st = sc.start()
    attack(st, ginn, victim)
    pass_all(st)
    assert zone_of(st, ginn) is Zone.TRASH
    assert len(st.zones[0][Zone.HAND]) == 1


@pytest.mark.card("ST04-009")
def test_st04_009_no_other_link_unit_no_draw() -> None:
    sc = Scenario()
    ginn = sc.add(0, "ST04-009", pilot="ST04-014")
    sc.add(0, GM, pilot="ST04-011")  # paired but not linked
    victim = sc.add(1, SANDROCK, rested=True)
    st = sc.start()
    attack(st, ginn, victim)
    pass_all(st)
    assert zone_of(st, ginn) is Zone.TRASH
    assert len(st.zones[0][Zone.HAND]) == 0


@pytest.mark.card("ST04-009")
def test_st04_009_unpaired_no_draw() -> None:
    sc = Scenario()
    ginn = sc.add(0, "ST04-009")  # 3/1
    sc.add(0, "ST04-006", pilot="ST04-011")
    victim = sc.add(1, SANDROCK, rested=True)
    st = sc.start()
    attack(st, ginn, victim)
    pass_all(st)
    assert zone_of(st, ginn) is Zone.TRASH
    assert len(st.zones[0][Zone.HAND]) == 0


@pytest.mark.card("ST04-010")
@pytest.mark.rule("8-6-1")
def test_st04_010_attack_enemy_ap_minus_2_during_battle() -> None:
    sc = Scenario()
    strike = sc.add(0, "ST04-002", pilot="ST04-010")
    enemy = sc.add(1, SANDROCK)
    sc.shields(1, FILLER, FILLER)
    sc.resources(1, 2)
    sc.add(1, "ST03-012", Zone.HAND)  # gives player 1 an action-step option
    st = sc.start()
    attack(st, strike)
    assert st.pending is not None and st.pending.kind is DecisionKind.ACTION_STEP
    assert ap(st, enemy) == 2
    pass_all(st)
    assert st.battle is None
    assert ap(st, enemy) == 4


@pytest.mark.card("ST04-011")
@pytest.mark.rule("13-2-11")
def test_st04_011_when_linked_may_attack_active_lv5_or_lower() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    aegis = sc.add(0, "ST04-006")
    athrun = sc.add(0, "ST04-011", Zone.HAND)
    lv5 = sc.add(1, TALLGEESE)
    lv6 = sc.add(1, WING)
    st = sc.start()
    assert not has_action(st, A.ATTACK, aegis, lv5)
    play(st, athrun, onto=aegis)
    assert has_action(st, A.ATTACK, aegis, lv5)
    assert not has_action(st, A.ATTACK, aegis, lv6)


@pytest.mark.card("ST04-011")
def test_st04_011_not_linked_gives_no_permission() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    gm = sc.add(0, GM)
    athrun = sc.add(0, "ST04-011", Zone.HAND)
    lv5 = sc.add(1, TALLGEESE)
    st = sc.start()
    play(st, athrun, onto=gm)
    assert not has_action(st, A.ATTACK, gm, lv5)


@pytest.mark.card("ST04-012")
@pytest.mark.parametrize(
    ("mode", "name", "stats"),
    [(0, "Sword Strike Gundam", (4, 2)), (1, "Launcher Strike Gundam", (2, 4))],
)
def test_st04_012_main_deploys_chosen_strike_token(
    mode: int, name: str, stats: tuple[int, int]
) -> None:
    sc = Scenario()
    sc.resources(0, 4)
    cmd = sc.add(0, "ST04-012", Zone.HAND)
    st = sc.start()
    play(st, cmd)
    choose_option(st, mode)
    (tok,) = token_uids(st, 0)
    assert V.cdef(st, tok).name == name
    assert (ap(st, tok), hp(st, tok)) == stats
    assert keywords(st, tok).get("Blocker") == 1
    assert V.cdef(st, tok).traits == ("Earth Alliance",)


@pytest.mark.card("ST04-012")
def test_st04_012_main_does_nothing_with_earth_alliance_token() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    cmd = sc.add(0, "ST04-012", Zone.HAND)
    sc.add(0, "T-008")
    st = sc.start()
    play(st, cmd)
    assert zone_of(st, cmd) is Zone.TRASH
    assert tokens_in_play(st, 0) == ["Aile Strike Gundam"]


@pytest.mark.card("ST04-012")
def test_st04_012_other_tokens_do_not_prevent() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    cmd = sc.add(0, "ST04-012", Zone.HAND)
    sc.add(0, "T-007")  # (Zeon) token
    sc.add(0, "ST04-005")  # (Earth Alliance) Unit card, not a token
    st = sc.start()
    play(st, cmd)
    choose_option(st, 0)
    assert sorted(tokens_in_play(st, 0)) == ["Sword Strike Gundam", "Zaku Ⅱ"]


@pytest.mark.card("ST04-012")
@pytest.mark.rule("13-2-5-1")
def test_st04_012_burst_deploys_aile_strike_token() -> None:
    st = burst_from_shield(Scenario(), "ST04-012")
    yes(st)
    (tok,) = token_uids(st, 1)
    assert V.cdef(st, tok).name == "Aile Strike Gundam"
    assert (ap(st, tok), hp(st, tok)) == (3, 3)
    assert keywords(st, tok).get("Blocker") == 1


@pytest.mark.card("ST04-012")
def test_st04_012_burst_does_nothing_with_earth_alliance_token() -> None:
    sc = Scenario()
    sc.add(1, "T-009")
    st = burst_from_shield(sc, "ST04-012")
    yes(st)
    assert tokens_in_play(st, 1) == ["Launcher Strike Gundam"]


@pytest.mark.card("ST04-013")
@pytest.mark.faq("Q96")
def test_st04_013_returns_enemy_with_current_hp_3_or_less() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    cmd = sc.add(0, "ST04-013", Zone.HAND)
    damaged = sc.add(1, MOEBIUS_ZERO, damage=1)  # 4 HP - 1 damage
    healthy = sc.add(1, MOEBIUS_ZERO)
    st = sc.start()
    play(st, cmd)
    assert zone_of(st, damaged) is Zone.HAND
    assert zone_of(st, healthy) is Zone.BATTLE


@pytest.mark.card("ST04-013")
@pytest.mark.rule("10-1-8-1-1")
def test_st04_013_unplayable_without_target() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    cmd = sc.add(0, "ST04-013", Zone.HAND)
    sc.add(1, MOEBIUS_ZERO)
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, cmd)


@pytest.mark.card("ST04-014")
@pytest.mark.rule("13-1-5")
def test_st04_014_lv2_or_lower_friendly_gains_first_strike() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    cmd = sc.add(0, "ST04-014", Zone.HAND)
    lv2 = sc.add(0, GM)
    lv3 = sc.add(0, GUNCANNON)
    st = sc.start()
    play(st, cmd)
    assert "First Strike" in keywords(st, lv2)
    assert "First Strike" not in keywords(st, lv3)
    to_next_turn(st)
    assert "First Strike" not in keywords(st, lv2)


@pytest.mark.card("ST04-015")
def test_st04_015_sets_blocker_active_and_it_cannot_attack() -> None:
    sc = Scenario()
    base = sc.base(0, "ST04-015")
    moebius = sc.add(0, "ST04-004", rested=True)
    other = sc.add(0, "ST04-004")
    sc.resources(0, 2)
    st = sc.start()
    activate(st, base)
    select(st, moebius)
    assert not st.cards[moebius].rested
    assert not has_action(st, A.ATTACK, moebius)
    assert has_action(st, A.ATTACK, other)
    assert rested_resources(st, 0) == 2
    assert not has_action(st, A.ACTIVATE, base)


@pytest.mark.card("ST04-015")
@pytest.mark.rule("10-2-2")
@pytest.mark.faq("Q100")
def test_st04_015_needs_a_friendly_blocker() -> None:
    sc = Scenario()
    base = sc.base(0, "ST04-015")
    sc.add(0, GM, rested=True)
    sc.resources(0, 2)
    st = sc.start()
    assert not has_action(st, A.ACTIVATE, base)


@pytest.mark.card("ST04-015")
@pytest.mark.ruling("ST04-015:Q156")
def test_st04_015_losing_blocker_still_cannot_attack() -> None:
    sc = Scenario()
    base = sc.base(0, "ST04-015")
    doga = sc.add(0, GEARA_DOGA_NZ, rested=True)
    dra_c = sc.add(0, DRA_C)
    victim = sc.add(1, FILLER, rested=True)
    sc.resources(0, 2)
    st = sc.start()
    assert "Blocker" in keywords(st, doga)
    activate(st, base)
    assert not st.cards[doga].rested
    attack(st, dra_c, victim)
    pass_all(st)
    assert zone_of(st, dra_c) is Zone.TRASH
    assert "Blocker" not in keywords(st, doga)
    assert not has_action(st, A.ATTACK, doga)


@pytest.mark.card("ST04-016")
def test_st04_016_rest_gives_friendly_unit_ap_plus_1() -> None:
    sc = Scenario()
    base = sc.base(0, "ST04-016")
    gm = sc.add(0, GM)
    st = sc.start()
    activate(st, base)
    assert st.cards[base].rested
    assert ap(st, gm) == 3
    to_next_turn(st)
    assert ap(st, gm) == 2


# ---------------------------------------------------------------------------------------------
# 【Pilot】 Commands (rule 3-4-6)


@pytest.mark.card("ST01-012", "ST01-013", "ST02-012", "ST02-013")
@pytest.mark.card("ST03-012", "ST03-014", "ST04-013", "ST04-014")
@pytest.mark.rule("3-4-6", "3-2-6-2")
@pytest.mark.parametrize(
    ("command", "link_unit"),
    [
        ("ST01-012", "ST01-004"),
        ("ST01-013", "ST01-003"),
        ("ST02-012", "ST02-003"),
        ("ST02-013", "ST02-004"),
        ("ST03-012", "ST03-002"),
        ("ST03-014", "ST03-009"),
        ("ST04-013", "ST04-003"),
        ("ST04-014", "ST04-009"),
    ],
)
def test_pilot_command_pairs_and_links(command: str, link_unit: str) -> None:
    sc = Scenario()
    sc.resources(0, 4)
    unit = sc.add(0, link_unit)
    cmd = sc.add(0, command, Zone.HAND)
    st = sc.start()
    base_ap, base_hp = ap(st, unit), hp(st, unit)
    play(st, cmd, onto=unit)
    cd = V.cdef(st, cmd)
    assert zone_of(st, cmd) is Zone.PAIRED
    assert V.is_linked(V.derived(st), unit)
    assert (ap(st, unit), hp(st, unit)) == (base_ap + cd.ap, base_hp + cd.hp)
