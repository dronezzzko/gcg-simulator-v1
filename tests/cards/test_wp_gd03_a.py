"""Behaviour and ruling tests for WP-GD03-A (GD03-001..GD03-083)."""

from __future__ import annotations

import pytest

from gcg_sim.engine import core
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
    has_action,
    keywords,
    no,
    order,
    pass_all,
    play,
    to_next_turn,
    yes,
    zone_of,
)

A = ActionKind
K = DecisionKind

NEUTRAL_PILOT = "GD05-087"  # Lauda Neill: Lv3 cost1 +1/+1, text only affects (Academy) Units
ZAKU = "GD01-060"  # Zaku Mariner: Lv2 2/2 (Zeon), vanilla
HIZACK = "GD02-013"  # Lv2 2/2 (Titans), vanilla
GAPLANT = "GD04-010"  # Lv4 3/4 (Titans), vanilla
UNICORN = "GD03-016"  # Lv5 5/4, vanilla
UNICORN_DM = "GD03-010"  # Lv8 6/6 <Repair 3>
AGRISSA = "GD04-079"  # Lv5 5/4 (Superpower Bloc), vanilla
GYAN = "GD04-032"  # Lv5 4/5 (Zeon), vanilla


def pair_raw(sc: Scenario, unit: int, number: str) -> int:
    """Pair a pilot-capable card beneath ``unit`` without requiring its script (some linking
    Pilots belong to other work packages)."""
    st = sc.st
    uid = core.new_card(st, sc.db[number].def_id, st.cards[unit].owner, Zone.PAIRED)
    st.cards[uid].pair = unit
    st.cards[unit].pair = uid
    st.touch()
    return uid


def raw_hand(sc: Scenario, player: int, number: str) -> int:
    """Put a card in hand without requiring its script."""
    return core.new_card(sc.st, sc.db[number].def_id, player, Zone.HAND)


def resolve_orders(st: GameState) -> None:
    while st.pending is not None and st.pending.kind is K.ORDER_TRIGGER:
        order(st, 0)


def pick(st: GameState, *uids: int) -> None:
    for u in uids:
        act(st, A.SELECT, u)


def done(st: GameState) -> None:
    act(st, A.DONE)


def pending(st: GameState) -> DecisionKind | None:
    return st.pending.kind if st.pending is not None else None


def hand_size(st: GameState, player: int = 0) -> int:
    return len(st.zones[player][Zone.HAND])


def rested(st: GameState, uid: int) -> bool:
    return st.cards[uid].rested


def select_options(st: GameState) -> set[int]:
    assert st.pending is not None
    return {o.a for o in st.pending.options if o.kind is A.SELECT}


def attack_targets(st: GameState, attacker: int) -> set[int]:
    assert st.pending is not None
    return {o.b for o in st.pending.options if o.kind is A.ATTACK and o.a == attacker}


# ---------------------------------------------------------------------------------------------
# GD03-001 Gundam NT-1


@pytest.mark.card("GD03-001")
def test_gd03_001_when_paired_destroying_a_rested_unit_draws() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    nt1 = sc.add(0, "GD03-001")
    victim = sc.add(1, ZAKU, rested=True, damage=1)
    active = sc.add(1, ZAKU, damage=1)
    pilot = sc.add(0, NEUTRAL_PILOT, Zone.HAND)
    st = sc.start()
    assert keywords(st, nt1)["Repair"] == 2
    before = hand_size(st)
    play(st, pilot, onto=nt1)
    assert zone_of(st, victim) is Zone.TRASH
    assert st.cards[active].damage == 1  # only a rested enemy Unit can be chosen
    assert hand_size(st) == before  # pilot left the hand, 1 card drawn


@pytest.mark.card("GD03-001")
@pytest.mark.ruling("GD03-001:Q209")
def test_gd03_001_no_draw_when_the_unit_is_destroyed_later() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    nt1 = sc.add(0, "GD03-001")
    victim = sc.add(1, ZAKU, rested=True)
    pilot = sc.add(0, NEUTRAL_PILOT, Zone.HAND)
    st = sc.start()
    before = hand_size(st)
    play(st, pilot, onto=nt1)
    assert st.cards[victim].damage == 1
    assert hand_size(st) == before - 1
    attack(st, nt1, victim)
    pass_all(st)
    assert zone_of(st, victim) is Zone.TRASH  # destroyed in battle, not by the effect
    assert hand_size(st) == before - 1


# ---------------------------------------------------------------------------------------------
# GD03-002 The-O


@pytest.mark.card("GD03-002")
@pytest.mark.ruling("GD03-002:Q210")
def test_gd03_002_rests_enemy_with_lv_up_to_the_attacking_repair_unit() -> None:
    sc = Scenario()
    the_o = sc.add(0, "GD03-002", pilot=NEUTRAL_PILOT)
    gundam = sc.add(0, "GD01-001")  # Lv4, gains <Repair 1> from its own text
    lv4 = sc.add(1, GAPLANT)
    lv5 = sc.add(1, UNICORN)
    st = sc.start()
    assert keywords(st, the_o)["Repair"] == 3
    attack(st, gundam)
    assert rested(st, lv4)
    assert not rested(st, lv5)


@pytest.mark.card("GD03-002")
def test_gd03_002_needs_a_pilot_and_another_repair_unit() -> None:
    sc = Scenario()
    the_o = sc.add(0, "GD03-002")
    gundam = sc.add(0, "GD01-001")
    zaku = sc.add(0, ZAKU)
    lv4 = sc.add(1, GAPLANT)
    sc.shields(1, ZAKU, ZAKU, ZAKU)
    st = sc.start()
    attack(st, gundam)  # The-O is not paired
    pass_all(st)
    assert not rested(st, lv4)
    attack(st, zaku)
    pass_all(st)
    attack(st, the_o)
    pass_all(st)
    assert not rested(st, lv4)


# ---------------------------------------------------------------------------------------------
# GD03-003 Messala, GD03-010, GD03-012 (Repair)


@pytest.mark.card("GD03-003")
@pytest.mark.rule("13-1-1-1", "13-1-4-1")
def test_gd03_003_blocks_and_repairs() -> None:
    sc = Scenario(active=1)
    attacker = sc.add(1, ZAKU)
    messala = sc.add(0, "GD03-003", damage=1)
    st = sc.start()
    assert keywords(st, messala) == {"Blocker": 1, "Repair": 1}
    attack(st, attacker)
    block(st, messala)
    pass_all(st)
    assert zone_of(st, attacker) is Zone.TRASH
    assert st.cards[messala].damage == 3
    to_next_turn(st)  # Messala's controller's turn
    to_next_turn(st)
    assert st.cards[messala].damage == 2  # <Repair 1> at the end of its controller's turn


@pytest.mark.card("GD03-010")
@pytest.mark.rule("13-1-1-1")
def test_gd03_010_repair_3() -> None:
    sc = Scenario()
    unit = sc.add(0, UNICORN_DM, damage=4)
    st = sc.start()
    to_next_turn(st)
    assert st.cards[unit].damage == 1


@pytest.mark.card("GD03-012")
@pytest.mark.rule("13-1-1-1")
def test_gd03_012_repair_1() -> None:
    sc = Scenario()
    unit = sc.add(0, "GD03-012", damage=1)
    st = sc.start()
    assert keywords(st, unit) == {"Repair": 1}
    to_next_turn(st)
    assert st.cards[unit].damage == 0


# ---------------------------------------------------------------------------------------------
# GD03-004 Hambrabi


@pytest.mark.card("GD03-004")
@pytest.mark.faq("Q96")
def test_gd03_004_rests_unit_with_5_or_less_current_hp() -> None:
    sc = Scenario()
    hambrabi = sc.add(0, "GD03-004")
    sc.add(0, HIZACK)
    sc.add(0, HIZACK)
    damaged = sc.add(1, UNICORN_DM, damage=1)  # 6 HP - 1 damage = 5
    healthy = sc.add(1, UNICORN_DM)
    st = sc.start()
    attack(st, hambrabi)
    assert rested(st, damaged)
    assert not rested(st, healthy)


@pytest.mark.card("GD03-004")
def test_gd03_004_needs_two_other_titans_units() -> None:
    sc = Scenario()
    hambrabi = sc.add(0, "GD03-004")
    sc.add(0, HIZACK)
    target = sc.add(1, ZAKU)
    st = sc.start()
    attack(st, hambrabi)
    assert not rested(st, target)


# ---------------------------------------------------------------------------------------------
# GD03-005 Kshatriya Besserung


@pytest.mark.card("GD03-005")
def test_gd03_005_deploy_draws_one() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    card = sc.add(0, "GD03-005", Zone.HAND)
    st = sc.start()
    before = hand_size(st)
    play(st, card)
    assert hand_size(st) == before
    assert keywords(st, card) == {"Repair": 1}


# ---------------------------------------------------------------------------------------------
# GD03-006 Penelope (Middle Form)


@pytest.mark.card("GD03-006")
@pytest.mark.faq("Q96")
def test_gd03_006_rests_one_to_two_units_with_3_or_less_current_hp() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    card = sc.add(0, "GD03-006", Zone.HAND)
    small = sc.add(1, ZAKU)
    damaged = sc.add(1, UNICORN, damage=1)  # 4 HP - 1 = 3
    healthy = sc.add(1, UNICORN)
    st = sc.start()
    play(st, card)
    assert select_options(st) == {small, damaged}
    pick(st, small, damaged)
    assert rested(st, small) and rested(st, damaged)
    assert not rested(st, healthy)


# ---------------------------------------------------------------------------------------------
# GD03-007 Gundam NT-1 Full Armor


@pytest.mark.card("GD03-007")
def test_gd03_007_destroyed_rests_unit_with_3_or_less_current_hp() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    nt1fa = sc.add(0, "GD03-007", damage=2)
    rouei = sc.add(0, "GD03-067", Zone.HAND)
    damaged = sc.add(1, UNICORN, damage=1)
    healthy = sc.add(1, UNICORN)
    st = sc.start()
    play(st, rouei)
    pick(st, nt1fa)  # Rouei: deal 1 damage to one of your Units
    assert zone_of(st, nt1fa) is Zone.TRASH
    assert rested(st, damaged)
    assert not rested(st, healthy)


# ---------------------------------------------------------------------------------------------
# GD03-008 Bolinoak Sammahn


@pytest.mark.card("GD03-008")
def test_gd03_008_repair_2_only_while_paired() -> None:
    sc = Scenario()
    paired = sc.add(0, "GD03-008", pilot=NEUTRAL_PILOT, damage=2)
    unpaired = sc.add(0, "GD03-008", damage=2)
    st = sc.start()
    assert keywords(st, paired) == {"Repair": 2}
    assert keywords(st, unpaired) == {}
    to_next_turn(st)
    assert st.cards[paired].damage == 0
    assert st.cards[unpaired].damage == 2


# ---------------------------------------------------------------------------------------------
# GD03-009 Palace Athene


@pytest.mark.card("GD03-009")
@pytest.mark.ruling("GD03-009:Q211")
def test_gd03_009_exile_two_titans_cards_to_rest_an_enemy() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    card = sc.add(0, "GD03-009", Zone.HAND)
    t1, t2, t3 = sc.trash(0, HIZACK, HIZACK, HIZACK)
    target = sc.add(1, GAPLANT)
    big = sc.add(1, UNICORN)
    st = sc.start()
    play(st, card)
    assert pending(st) is K.YES_NO
    yes(st)
    pick(st, t1, t3)
    assert zone_of(st, t1) is Zone.REMOVAL and zone_of(st, t3) is Zone.REMOVAL
    assert zone_of(st, t2) is Zone.TRASH
    assert rested(st, target)  # the only enemy Unit that is Lv.4 or lower
    assert not rested(st, big)


@pytest.mark.card("GD03-009")
def test_gd03_009_needs_two_titans_cards() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    card = sc.add(0, "GD03-009", Zone.HAND)
    (only,) = sc.trash(0, HIZACK)
    target = sc.add(1, GAPLANT)
    st = sc.start()
    play(st, card)
    assert pending(st) is K.MAIN  # no prompt: 2 cards cannot be chosen
    assert zone_of(st, only) is Zone.TRASH
    assert not rested(st, target)


@pytest.mark.card("GD03-009")
def test_gd03_009_optional() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    card = sc.add(0, "GD03-009", Zone.HAND)
    trash = sc.trash(0, HIZACK, HIZACK)
    target = sc.add(1, GAPLANT)
    st = sc.start()
    play(st, card)
    no(st)
    assert all(zone_of(st, u) is Zone.TRASH for u in trash)
    assert not rested(st, target)


# ---------------------------------------------------------------------------------------------
# GD03-013 Hizack


@pytest.mark.card("GD03-013")
def test_gd03_013_ap_and_repair_with_another_jupitris_unit() -> None:
    sc = Scenario()
    with_ally = sc.add(0, "GD03-013")
    sc.add(0, "GD03-012")  # Messala (MA Mode): (Titans) (Jupitris)
    lonely = sc.add(1, "GD03-013")
    st = sc.start()
    assert ap(st, with_ally) == 3
    assert keywords(st, with_ally) == {"Repair": 1}
    assert ap(st, lonely) == 2
    assert keywords(st, lonely) == {}


# ---------------------------------------------------------------------------------------------
# GD03-014 Hizack Custom


@pytest.mark.card("GD03-014")
def test_gd03_014_costs_one_less_with_two_titans_units() -> None:
    sc = Scenario()
    sc.resources(0, 3, rested=2)
    card = sc.add(0, "GD03-014", Zone.HAND)
    sc.add(0, HIZACK)
    sc.add(0, HIZACK)
    st = sc.start()
    play(st, card)
    assert zone_of(st, card) is Zone.BATTLE


@pytest.mark.card("GD03-014")
def test_gd03_014_full_cost_with_one_titans_unit() -> None:
    sc = Scenario()
    sc.resources(0, 3, rested=2)
    card = sc.add(0, "GD03-014", Zone.HAND)
    sc.add(0, HIZACK)
    st = sc.start()
    assert not has_action(st, A.PLAY_UNIT, card)


@pytest.mark.card("GD03-014")
@pytest.mark.ruling("GD03-014:Q212")
@pytest.mark.rule("2-9-1")
def test_gd03_014_level_is_not_reduced() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    card = sc.add(0, "GD03-014", Zone.HAND)
    sc.add(0, HIZACK)
    sc.add(0, HIZACK)
    st = sc.start()
    assert V.play_cost(st, V.derived(st), card) == 1
    assert not has_action(st, A.PLAY_UNIT, card)  # Lv.3 needs 3 Resources


# ---------------------------------------------------------------------------------------------
# GD03-015 Baund Doc


@pytest.mark.card("GD03-015")
@pytest.mark.ruling("GD03-015:Q213")
def test_gd03_015_exile_three_titans_cards_for_breach_4() -> None:
    sc = Scenario()
    baund = sc.add(0, "GD03-015")
    trash = sc.trash(0, HIZACK, HIZACK, HIZACK, HIZACK, HIZACK, HIZACK)
    st = sc.start()
    activate(st, baund)
    pick(st, *trash[:3])
    assert all(zone_of(st, u) is Zone.REMOVAL for u in trash[:3])
    assert keywords(st, baund) == {"Breach": 4}
    assert not has_action(st, A.ACTIVATE, baund)  # 【Once per Turn】
    to_next_turn(st)
    assert keywords(st, baund) == {}


@pytest.mark.card("GD03-015")
def test_gd03_015_needs_three_titans_cards() -> None:
    sc = Scenario()
    baund = sc.add(0, "GD03-015")
    sc.trash(0, HIZACK, HIZACK, ZAKU)
    st = sc.start()
    assert not has_action(st, A.ACTIVATE, baund)


# ---------------------------------------------------------------------------------------------
# GD03-017 Kämpfer

MIKHAIL = "GD03-090"  # Mikhail Kaminsky: (Zeon) (Cyclops Team) Pilot, Lv4 cost1


@pytest.mark.card("GD03-017")
@pytest.mark.rule("13-2-5-1")
def test_gd03_017_burst_adds_cyclops_team_pilot_card_from_trash() -> None:
    sc = Scenario()
    attacker = sc.add(0, ZAKU)
    (kampfer,) = sc.shields(1, "GD03-017")
    pilot, command_pilot = sc.trash(1, MIKHAIL, "GD03-107")
    st = sc.start()
    attack(st, attacker)
    pass_all(st)
    assert pending(st) is K.BURST
    yes(st)
    assert zone_of(st, pilot) is Zone.HAND
    assert zone_of(st, command_pilot) is Zone.TRASH  # a Command with 【Pilot】 is not a Pilot card
    assert zone_of(st, kampfer) is Zone.TRASH


@pytest.mark.card("GD03-017")
def test_gd03_017_cyclops_units_may_attack_active_units_with_5_or_less_ap() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    kampfer = sc.add(0, "GD03-017")
    zgok = sc.add(0, "GD03-027")
    pilot = sc.add(0, MIKHAIL, Zone.HAND)
    ap5 = sc.add(1, UNICORN)
    ap6 = sc.add(1, UNICORN_DM)
    st = sc.start()
    assert ap5 not in attack_targets(st, zgok)
    play(st, pilot, onto=kampfer)
    assert attack_targets(st, zgok) == {PLAYER_TARGET, ap5}
    assert ap6 not in attack_targets(st, kampfer)


@pytest.mark.card("GD03-017")
def test_gd03_017_needs_a_cyclops_team_pilot() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    kampfer = sc.add(0, "GD03-017")
    zgok = sc.add(0, "GD03-027")
    pilot = sc.add(0, NEUTRAL_PILOT, Zone.HAND)
    ap5 = sc.add(1, UNICORN)
    st = sc.start()
    play(st, pilot, onto=kampfer)
    assert ap5 not in attack_targets(st, zgok)


# ---------------------------------------------------------------------------------------------
# GD03-018 Altron Gundam


@pytest.mark.card("GD03-018")
def test_gd03_018_attack_deals_5_to_a_blocker() -> None:
    sc = Scenario()
    altron = sc.add(0, "GD03-018")
    blocker = sc.add(1, "GD03-057")  # GN Armor (Type-E), <Blocker> 5/4
    other = sc.add(1, UNICORN)
    sc.shields(1, ZAKU)
    st = sc.start()
    assert keywords(st, altron) == {"Breach": 5}
    attack(st, altron)
    assert zone_of(st, blocker) is Zone.TRASH
    assert st.cards[other].damage == 0


# ---------------------------------------------------------------------------------------------
# GD03-019 Gundam AGE-2 Normal

ASEMU = "GD03-088"  # Asemu Asuno: links AGE-1/AGE-2


@pytest.mark.card("GD03-019")
def test_gd03_019_when_linked_places_ex_resource() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    age2 = sc.add(0, "GD03-019")
    pilot = sc.add(0, ASEMU, Zone.HAND)
    st = sc.start()
    play(st, pilot, onto=age2)
    area = st.zones[0][Zone.RESOURCE_AREA]
    assert len(area) == 5
    assert V.cdef(st, area[-1]).is_token


@pytest.mark.card("GD03-019")
@pytest.mark.rule("8-2-1")
def test_gd03_019_attracts_attacks_while_paired_and_rested() -> None:
    sc = Scenario(active=1)
    attacker = sc.add(1, ZAKU)
    age2 = sc.add(0, "GD03-019", pilot=NEUTRAL_PILOT, rested=True)
    other = sc.add(0, ZAKU, rested=True)
    st = sc.start()
    assert attack_targets(st, attacker) == {age2}
    assert other not in attack_targets(st, attacker)


@pytest.mark.card("GD03-019")
def test_gd03_019_no_attraction_without_pilot_or_when_active() -> None:
    sc = Scenario(active=1)
    attacker = sc.add(1, ZAKU)
    unpaired = sc.add(0, "GD03-019", rested=True)
    sc.add(0, "GD03-019", pilot=NEUTRAL_PILOT)  # active: cannot be attacked at all
    st = sc.start()
    assert attack_targets(st, attacker) == {PLAYER_TARGET, unpaired}


@pytest.mark.card("GD03-019", "GD03-074")
@pytest.mark.ruling("GD03-019:Q214", "GD03-074:Q234")
def test_gd03_019_and_074_several_attractors_attacker_picks_one() -> None:
    sc = Scenario(active=1)
    attacker = sc.add(1, ZAKU)
    age2 = sc.add(0, "GD03-019", pilot=NEUTRAL_PILOT, rested=True)
    taozi = sc.add(0, "GD03-074", pilot=NEUTRAL_PILOT, rested=True)
    sc.add(0, AGRISSA)  # another (Superpower Bloc) Unit for Tieren Taozi
    st = sc.start()
    assert attack_targets(st, attacker) == {age2, taozi}
    attack(st, attacker, taozi)
    pass_all(st)
    assert zone_of(st, taozi) is Zone.TRASH  # 4/2 with its Pilot, takes 2
    assert st.cards[age2].damage == 0


# ---------------------------------------------------------------------------------------------
# GD03-020 Zaku Ⅱ FZ


@pytest.mark.card("GD03-020")
def test_gd03_020_deploys_ad_balloons_and_ignores_enemy_battle_damage() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    fz = sc.add(0, "GD03-020")
    sc.trash(0, "GD03-027", "GD03-027", "GD03-027", "GD03-027")
    pilot = sc.add(0, NEUTRAL_PILOT, Zone.HAND)
    enemy = sc.add(1, "GD02-015", rested=True)  # Marasai 3/3
    st = sc.start()
    play(st, pilot, onto=fz)
    tokens = [u for u in st.zones[0][Zone.BATTLE] if V.cdef(st, u).name == "Ad Balloon"]
    assert len(tokens) == 2
    assert all(rested(st, u) for u in tokens)
    attack(st, fz, enemy)
    pass_all(st)
    assert zone_of(st, fz) is Zone.BATTLE
    assert st.cards[fz].damage == 0


@pytest.mark.card("GD03-020")
def test_gd03_020_needs_four_cyclops_team_cards_in_trash() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    fz = sc.add(0, "GD03-020")
    sc.trash(0, "GD03-027", "GD03-027", "GD03-027")
    pilot = sc.add(0, NEUTRAL_PILOT, Zone.HAND)
    enemy = sc.add(1, "GD02-015", rested=True)
    st = sc.start()
    play(st, pilot, onto=fz)
    assert len(st.zones[0][Zone.BATTLE]) == 1
    attack(st, fz, enemy)
    pass_all(st)
    assert zone_of(st, fz) is Zone.TRASH  # 3 damage to a 2+1 HP Unit


# ---------------------------------------------------------------------------------------------
# GD03-021 Gundam Deathscythe Hell


@pytest.mark.card("GD03-021")
def test_gd03_021_chosen_unit_may_attack_active_enemy_units() -> None:
    sc = Scenario()
    sc.resources(0, 8)
    card = sc.add(0, "GD03-021", Zone.HAND)
    leo = sc.add(0, "GD05-077")  # (G Team)
    zaku = sc.add(0, ZAKU)
    target = sc.add(1, UNICORN)
    st = sc.start()
    play(st, card)
    assert select_options(st) == {card, leo}
    pick(st, leo)
    assert target in attack_targets(st, leo)
    assert target not in attack_targets(st, zaku)


# ---------------------------------------------------------------------------------------------
# GD03-022 Gundam Kyrios

HALLELUJAH = "GD04-090"  # links Gundam Kyrios; its own trigger shares the event


@pytest.mark.card("GD03-022")
@pytest.mark.ruling("GD03-022:Q215")
def test_gd03_022_triggers_even_when_both_units_are_destroyed() -> None:
    sc = Scenario()
    kyrios = sc.add(0, "GD03-022", pilot=HALLELUJAH)  # 7/4 linked
    victim = sc.add(1, UNICORN, rested=True)  # 5/4: both Units are destroyed
    small = sc.add(1, ZAKU)
    lv3 = sc.add(1, "GD03-027")
    lv4 = sc.add(1, GAPLANT)
    st = sc.start()
    attack(st, kyrios, victim)
    pass_all(st)
    resolve_orders(st)
    assert zone_of(st, kyrios) is Zone.TRASH and zone_of(st, victim) is Zone.TRASH
    assert st.cards[small].damage == 1
    assert st.cards[lv3].damage == 1
    assert st.cards[lv4].damage == 0


@pytest.mark.card("GD03-022")
def test_gd03_022_needs_link() -> None:
    sc = Scenario()
    kyrios = sc.add(0, "GD03-022", pilot=NEUTRAL_PILOT)
    victim = sc.add(1, ZAKU, rested=True)
    small = sc.add(1, ZAKU)
    st = sc.start()
    attack(st, kyrios, victim)
    pass_all(st)
    assert zone_of(st, victim) is Zone.TRASH
    assert st.cards[small].damage == 0


# ---------------------------------------------------------------------------------------------
# GD03-023 G-Bouncer


@pytest.mark.card("GD03-023")
def test_gd03_023_ex_resource_grants_high_maneuver_to_age_system_unit() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    sc.add(0, "GD03-023")
    age2 = sc.add(0, "GD03-019")
    pilot = sc.add(0, ASEMU, Zone.HAND)
    st = sc.start()
    assert "High-Maneuver" not in keywords(st, age2)
    play(st, pilot, onto=age2)  # 【When Linked】Place 1 EX Resource.
    assert "High-Maneuver" in keywords(st, age2)
    to_next_turn(st)
    assert "High-Maneuver" not in keywords(st, age2)


# ---------------------------------------------------------------------------------------------
# GD03-024 Hy-Gogg


@pytest.mark.card("GD03-024")
def test_gd03_024_when_linked_deploys_rested_hy_gogg_token() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    hygogg = sc.add(0, "GD03-024")
    sc.add(0, "GD03-027")
    pilot = sc.add(0, MIKHAIL, Zone.HAND)
    st = sc.start()
    play(st, pilot, onto=hygogg)
    tokens = [u for u in st.zones[0][Zone.BATTLE] if V.cdef(st, u).is_token]
    assert len(tokens) == 1
    (token,) = tokens
    assert V.cdef(st, token).name == "Hy-Gogg" and rested(st, token)
    assert (ap(st, token), V.hp_of(st, V.derived(st), token)) == (2, 1)


@pytest.mark.card("GD03-024")
def test_gd03_024_needs_another_cyclops_team_unit() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    hygogg = sc.add(0, "GD03-024")
    pilot = sc.add(0, MIKHAIL, Zone.HAND)
    st = sc.start()
    play(st, pilot, onto=hygogg)
    assert st.zones[0][Zone.BATTLE] == [hygogg]


# ---------------------------------------------------------------------------------------------
# GD03-025 Gundam Sandrock Custom


@pytest.mark.card("GD03-025")
@pytest.mark.ruling("GD03-025:Q216")
def test_gd03_025_rested_maganac_units_attract_attacks() -> None:
    sc = Scenario(active=1)
    attacker = sc.add(1, ZAKU)
    sc.add(0, "GD03-025")
    m1 = sc.add(0, "GD02-028", rested=True)
    m2 = sc.add(0, "GD02-028", rested=True)
    other = sc.add(0, ZAKU, rested=True)
    st = sc.start()
    assert attack_targets(st, attacker) == {m1, m2}
    assert other not in attack_targets(st, attacker)
    attack(st, attacker, m2)
    pass_all(st)
    assert st.cards[m2].damage == 2
    assert st.cards[m1].damage == 0


@pytest.mark.card("GD03-025")
def test_gd03_025_active_maganac_units_do_not_attract() -> None:
    sc = Scenario(active=1)
    attacker = sc.add(1, ZAKU)
    sc.add(0, "GD03-025")
    sc.add(0, "GD02-028")
    other = sc.add(0, ZAKU, rested=True)
    st = sc.start()
    assert attack_targets(st, attacker) == {PLAYER_TARGET, other}


# ---------------------------------------------------------------------------------------------
# GD03-026 Gundam Dynames


@pytest.mark.card("GD03-026")
@pytest.mark.rule("13-1-2-1")
def test_gd03_026_breach_3() -> None:
    sc = Scenario()
    dynames = sc.add(0, "GD03-026")
    victim = sc.add(1, ZAKU, rested=True)
    top, second = sc.shields(1, ZAKU, ZAKU)
    st = sc.start()
    attack(st, dynames, victim)
    pass_all(st)
    assert zone_of(st, victim) is Zone.TRASH
    assert zone_of(st, top) is Zone.TRASH
    assert zone_of(st, second) is Zone.SHIELD


# ---------------------------------------------------------------------------------------------
# GD03-028 Auda's Maganac


@pytest.mark.card("GD03-028")
def test_gd03_028_ap_plus_2_when_attacking_a_unit() -> None:
    sc = Scenario()
    maganac = sc.add(0, "GD03-028")
    target = sc.add(1, "GD03-064", rested=True)  # Defurse 2/5
    st = sc.start()
    attack(st, maganac, target)
    pass_all(st)
    assert st.cards[target].damage == 4
    assert ap(st, maganac) == 2  # "during this battle" has ended


@pytest.mark.card("GD03-028")
def test_gd03_028_no_bonus_when_attacking_the_player() -> None:
    sc = Scenario()
    maganac = sc.add(0, "GD03-028")
    sc.shields(1, ZAKU)
    st = sc.start()
    attack(st, maganac)
    assert ap(st, maganac) == 2


# ---------------------------------------------------------------------------------------------
# GD03-029 Gundam Heavyarms Custom


@pytest.mark.card("GD03-029")
@pytest.mark.ruling("GD03-029:Q217")
def test_gd03_029_deals_2_to_enemy_blockers_even_if_destroyed_too() -> None:
    sc = Scenario()
    heavyarms = sc.add(0, "GD03-029")  # 4/5
    victim = sc.add(1, AGRISSA, rested=True)  # 5/4
    blocker = sc.add(1, "GD03-057")
    other = sc.add(1, ZAKU)
    st = sc.start()
    attack(st, heavyarms, victim)
    pass_all(st)
    assert zone_of(st, heavyarms) is Zone.TRASH and zone_of(st, victim) is Zone.TRASH
    assert st.cards[blocker].damage == 2
    assert st.cards[other].damage == 0


@pytest.mark.card("GD03-029")
def test_gd03_029_only_during_your_turn() -> None:
    sc = Scenario(active=1)
    attacker = sc.add(1, "GD03-047")  # DINN (Commander Type) 3/4
    enemy_blocker = sc.add(1, "GD03-057", rested=True)
    heavyarms = sc.add(0, "GD03-029", rested=True)
    st = sc.start()
    attack(st, attacker, heavyarms)
    pass_all(st)
    assert zone_of(st, attacker) is Zone.TRASH  # Heavyarms destroyed it on the opponent's turn
    assert st.cards[enemy_blocker].damage == 0
    assert zone_of(st, heavyarms) is Zone.BATTLE


# ---------------------------------------------------------------------------------------------
# GD03-030 Gundam Kyrios (Tail Unit Flight Mode)


@pytest.mark.card("GD03-030")
@pytest.mark.ruling("GD03-030:Q218")
def test_gd03_030_costs_one_less_with_a_cb_link_unit() -> None:
    sc = Scenario()
    sc.resources(0, 3, rested=1)
    card = sc.add(0, "GD03-030", Zone.HAND)
    sc.add(0, "GD03-022", pilot=HALLELUJAH)  # (CB) Link Unit
    st = sc.start()
    assert V.play_cost(st, V.derived(st), card) == 2
    play(st, card)
    assert zone_of(st, card) is Zone.BATTLE


@pytest.mark.card("GD03-030")
@pytest.mark.ruling("GD03-030:Q218")
def test_gd03_030_level_unchanged_and_needs_link() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    card = sc.add(0, "GD03-030", Zone.HAND)
    sc.add(0, "GD03-022", pilot=NEUTRAL_PILOT)  # (CB) but not linked
    st = sc.start()
    assert V.play_cost(st, V.derived(st), card) == 3
    assert not has_action(st, A.PLAY_UNIT, card)


# ---------------------------------------------------------------------------------------------
# GD03-033 Providence Gundam

DEARKA = "GD01-095"  # (ZAFT) Pilot +1/+1, does not link Providence
RAU = "GD03-091"  # Rau Le Creuset: (ZAFT) Pilot +2/+1, links Providence


@pytest.mark.card("GD03-033")
def test_gd03_033_zaft_pilot_gives_zaft_units_ap_during_your_turn() -> None:
    sc = Scenario()
    providence = sc.add(0, "GD03-033", pilot=DEARKA)
    cgue = sc.add(0, "GD03-046")  # (ZAFT) 2/3
    zaku = sc.add(0, ZAKU)
    st = sc.start()
    assert ap(st, providence) == 8
    assert ap(st, cgue) == 4
    assert ap(st, zaku) == 2
    to_next_turn(st)
    assert ap(st, cgue) == 2


@pytest.mark.card("GD03-033")
def test_gd03_033_non_zaft_pilot_gives_nothing() -> None:
    sc = Scenario()
    sc.add(0, "GD03-033", pilot=NEUTRAL_PILOT)
    cgue = sc.add(0, "GD03-046")
    st = sc.start()
    assert ap(st, cgue) == 2


@pytest.mark.card("GD03-033")
@pytest.mark.ruling("GD03-033:Q219")
def test_gd03_033_attack_deals_1_per_4_ap() -> None:
    sc = Scenario()
    providence = sc.add(0, "GD03-033", pilot=RAU)
    target = sc.add(1, GYAN)
    sc.shields(1, ZAKU)
    st = sc.start()
    assert ap(st, providence) == 9
    attack(st, providence)
    assert st.cards[target].damage == 2


@pytest.mark.card("GD03-033")
def test_gd03_033_five_ap_deals_1() -> None:
    sc = Scenario()
    providence = sc.add(0, "GD03-033")
    target = sc.add(1, GYAN)
    sc.shields(1, ZAKU)
    st = sc.start()
    attack(st, providence)
    assert st.cards[target].damage == 1


# ---------------------------------------------------------------------------------------------
# GD03-034 GQuuuuuuX (Omega Psycommu)


@pytest.mark.card("GD03-034")
@pytest.mark.rule("13-1-7-1")
def test_gd03_034_deploy_deals_3_and_suppression() -> None:
    sc = Scenario()
    sc.resources(0, 8)
    card = sc.add(0, "GD03-034", Zone.HAND)
    target = sc.add(1, GYAN)
    st = sc.start()
    play(st, card)
    assert st.cards[target].damage == 3
    assert keywords(st, card) == {"Suppression": 1}


# ---------------------------------------------------------------------------------------------
# GD03-035 GFreD (Lv6)

NYAAN = "GD03-092"  # links GFreD; +1/+2


@pytest.mark.card("GD03-035")
@pytest.mark.ruling("GD03-035:Q220")
def test_gd03_035_activate_exiles_pilot_card_and_deals_1_to_all_enemy_units() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    gfred = sc.add(0, "GD03-035")
    pilot1, pilot2 = sc.trash(0, NEUTRAL_PILOT, NEUTRAL_PILOT)
    e1 = sc.add(1, ZAKU)
    e2 = sc.add(1, GYAN)
    own = sc.add(0, ZAKU)
    st = sc.start()
    activate(st, gfred)
    pick(st, pilot1)
    assert zone_of(st, pilot1) is Zone.REMOVAL
    assert zone_of(st, pilot2) is Zone.TRASH
    assert st.cards[e1].damage == 1 and st.cards[e2].damage == 1
    assert st.cards[own].damage == 0
    assert sum(1 for u in st.zones[0][Zone.RESOURCE_AREA] if rested(st, u)) == 1
    assert not has_action(st, A.ACTIVATE, gfred)  # 【Once per Turn】


@pytest.mark.card("GD03-035")
def test_gd03_035_activate_needs_a_pilot_card_in_trash() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    gfred = sc.add(0, "GD03-035")
    sc.trash(0, "GD03-107")  # a Command with 【Pilot】 is not a Pilot card
    st = sc.start()
    assert not has_action(st, A.ACTIVATE, gfred)


@pytest.mark.card("GD03-035")
@pytest.mark.ruling("GD03-035:Q221")
def test_gd03_035_when_linked_may_attack_active_unit_with_ap_up_to_its_own() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    sc.deck(0, "GD03-011")  # keeps Nyaan's own 【When Linked】 inert
    gfred = sc.add(0, "GD03-035")
    nyaan = raw_hand(sc, 0, NYAAN)
    ap5 = sc.add(1, UNICORN)
    ap6 = sc.add(1, UNICORN_DM)
    st = sc.start()
    assert ap5 not in attack_targets(st, gfred)
    play(st, nyaan, onto=gfred)
    resolve_orders(st)
    assert ap(st, gfred) == 5
    targets = attack_targets(st, gfred)
    assert ap5 in targets
    assert ap6 not in targets


# ---------------------------------------------------------------------------------------------
# GD03-036 Ξ Gundam (Flight Form)

HATHAWAY = "ST08-010"


@pytest.mark.card("GD03-036")
def test_gd03_036_when_linked_deals_1_to_all_enemy_units() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    xi = sc.add(0, "GD03-036")
    pilot = sc.add(0, HATHAWAY, Zone.HAND)
    e1 = sc.add(1, ZAKU)
    e2 = sc.add(1, GYAN)
    st = sc.start()
    play(st, pilot, onto=xi)
    resolve_orders(st)
    assert st.cards[e1].damage == 1 and st.cards[e2].damage == 1
    assert st.cards[xi].damage == 0


@pytest.mark.card("GD03-036")
def test_gd03_036_no_damage_without_link() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    xi = sc.add(0, "GD03-036")
    pilot = sc.add(0, NEUTRAL_PILOT, Zone.HAND)
    e1 = sc.add(1, ZAKU)
    st = sc.start()
    play(st, pilot, onto=xi)
    assert st.cards[e1].damage == 0


# ---------------------------------------------------------------------------------------------
# GD03-037 Bertigo

CHALLIA = "GD02-090"  # (Newtype) Pilot +1/+2, links Bertigo


@pytest.mark.card("GD03-037")
@pytest.mark.rule("13-1-5-2")
def test_gd03_037_first_strike_against_unit_with_destroyed_effect() -> None:
    sc = Scenario()
    bertigo = sc.add(0, "GD03-037", pilot=CHALLIA)  # 5/6
    victim = sc.add(1, "GD03-007", rested=True)  # 2/3 with 【Destroyed】
    st = sc.start()
    attack(st, bertigo, victim)
    pass_all(st)
    assert zone_of(st, victim) is Zone.TRASH
    assert st.cards[bertigo].damage == 0


@pytest.mark.card("GD03-037")
def test_gd03_037_pilot_granted_destroyed_effect_counts() -> None:
    sc = Scenario()
    bertigo = sc.add(0, "GD03-037", pilot=CHALLIA)
    victim = sc.add(1, ZAKU, rested=True, pilot="GD03-100")  # Soma Peries grants 【Destroyed】
    st = sc.start()
    attack(st, bertigo, victim)
    pass_all(st)
    assert zone_of(st, victim) is Zone.TRASH
    assert st.cards[bertigo].damage == 0


@pytest.mark.card("GD03-037")
@pytest.mark.ruling("GD03-037:Q222")
def test_gd03_037_gated_destroyed_effect_does_not_count() -> None:
    sc = Scenario()
    bertigo = sc.add(0, "GD03-037", pilot=CHALLIA)
    victim = sc.add(1, "GD03-078", rested=True)  # 【During Link】【Destroyed】, unpaired, 3/1
    st = sc.start()
    attack(st, bertigo, victim)
    pass_all(st)
    assert zone_of(st, victim) is Zone.TRASH
    assert st.cards[bertigo].damage == 3


@pytest.mark.card("GD03-037")
def test_gd03_037_needs_link() -> None:
    sc = Scenario()
    bertigo = sc.add(0, "GD03-037", pilot=NEUTRAL_PILOT)  # 5/5, not linked
    victim = sc.add(1, "GD03-007", rested=True)
    st = sc.start()
    attack(st, bertigo, victim)
    pass_all(st)
    assert st.cards[bertigo].damage == 2


# ---------------------------------------------------------------------------------------------
# GD03-038 GuAIZ (Commander Type)


@pytest.mark.card("GD03-038")
@pytest.mark.rule("13-1-3-1")
def test_gd03_038_support_rest_triggers_zaft_ap_bonus() -> None:
    sc = Scenario()
    guaiz = sc.add(0, "GD03-038")
    cgue = sc.add(0, "GD03-046")
    st = sc.start()
    act(st, A.ACTIVATE, guaiz, SUPPORT_AID)
    assert rested(st, guaiz)
    assert select_options(st) == {guaiz, cgue}
    pick(st, cgue)
    assert ap(st, cgue) == 5  # <Support 1> +1, then +2


@pytest.mark.card("GD03-038")
def test_gd03_038_resting_by_attacking_is_not_by_an_effect() -> None:
    sc = Scenario()
    guaiz = sc.add(0, "GD03-038")
    cgue = sc.add(0, "GD03-046")
    sc.shields(1, ZAKU, ZAKU)
    st = sc.start()
    attack(st, guaiz)
    pass_all(st)
    assert pending(st) is K.MAIN
    assert ap(st, cgue) == 2


@pytest.mark.card("GD03-038")
def test_gd03_038_not_during_opponents_turn() -> None:
    sc = Scenario(active=1)
    sc.resources(1, 6)
    penelope = sc.add(1, "GD03-006", Zone.HAND)
    guaiz = sc.add(0, "GD03-038")
    cgue = sc.add(0, "GD03-046")
    st = sc.start()
    play(st, penelope)
    pick(st, guaiz)
    done(st)
    assert rested(st, guaiz)
    assert pending(st) is K.MAIN and st.pending is not None and st.pending.player == 1
    assert ap(st, cgue) == 2


# ---------------------------------------------------------------------------------------------
# GD03-039 Red Gundam


@pytest.mark.card("GD03-039")
def test_gd03_039_rest_clan_unit_to_deal_2() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    card = sc.add(0, "GD03-039", Zone.HAND)
    clan = sc.add(0, "GD03-032")
    victim = sc.add(1, ZAKU)
    st = sc.start()
    play(st, card)
    assert rested(st, clan)
    assert zone_of(st, victim) is Zone.TRASH


@pytest.mark.card("GD03-039")
@pytest.mark.ruling("GD03-039:Q223")
def test_gd03_039_rests_friendly_even_without_enemy_target() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    card = sc.add(0, "GD03-039", Zone.HAND)
    clan = sc.add(0, "GD03-032")
    big = sc.add(1, UNICORN)
    st = sc.start()
    play(st, card)
    assert rested(st, clan)
    assert st.cards[big].damage == 0


# ---------------------------------------------------------------------------------------------
# GD03-040 Gundam Virsago & Gundam Ashtaron


@pytest.mark.card("GD03-040")
@pytest.mark.rule("13-1-6-1")
def test_gd03_040_high_maneuver_while_linked() -> None:
    sc = Scenario()
    linked = sc.add(0, "GD03-040", pilot="GD02-092")  # Shagia Frost
    unlinked = sc.add(0, "GD03-040", pilot=NEUTRAL_PILOT)
    sc.add(1, "GD03-083")  # AEU Hellion <Blocker>
    sc.shields(1, ZAKU, ZAKU)
    st = sc.start()
    assert "High-Maneuver" in keywords(st, linked)
    assert "High-Maneuver" not in keywords(st, unlinked)
    attack(st, linked)
    assert pending(st) is not K.BLOCK


# ---------------------------------------------------------------------------------------------
# GD03-041 Patulia


@pytest.mark.card("GD03-041")
def test_gd03_041_deals_3_to_all_bases() -> None:
    sc = Scenario()
    sc.resources(0, 7)
    card = sc.add(0, "GD03-041", Zone.HAND)
    own = sc.base(0)
    enemy = sc.base(1)
    unit = sc.add(1, ZAKU)
    st = sc.start()
    play(st, card)
    assert zone_of(st, own) is not Zone.BASE
    assert zone_of(st, enemy) is not Zone.BASE
    assert st.cards[unit].damage == 0


# ---------------------------------------------------------------------------------------------
# GD03-042 Duel Gundam (Assault Shroud)


@pytest.mark.card("GD03-042")
def test_gd03_042_with_5_ap_may_attack_active_lv5_or_lower() -> None:
    sc = Scenario()
    duel = sc.add(0, "GD03-042", pilot=RAU)  # 3+2 AP
    weak = sc.add(0, "GD03-042")
    lv5 = sc.add(1, UNICORN)
    lv8 = sc.add(1, UNICORN_DM)
    st = sc.start()
    assert ap(st, duel) == 5
    assert attack_targets(st, duel) == {PLAYER_TARGET, lv5}
    assert lv8 not in attack_targets(st, duel)
    assert attack_targets(st, weak) == {PLAYER_TARGET}


# ---------------------------------------------------------------------------------------------
# GD03-043 Messer Type-F02


@pytest.mark.card("GD03-043")
def test_gd03_043_when_paired_deals_1() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    messer = sc.add(0, "GD03-043")
    pilot = sc.add(0, NEUTRAL_PILOT, Zone.HAND)
    target = sc.add(1, GYAN)
    st = sc.start()
    play(st, pilot, onto=messer)
    assert st.cards[target].damage == 1


# ---------------------------------------------------------------------------------------------
# GD03-044 Daughtress Flyer, GD03-045 Balient


@pytest.mark.card("GD03-044")
@pytest.mark.rule("5-17-1")
def test_gd03_044_deploys_rested_daughtress_token() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    card = sc.add(0, "GD03-044", Zone.HAND)
    st = sc.start()
    play(st, card)
    tokens = [u for u in st.zones[0][Zone.BATTLE] if u != card]
    assert len(tokens) == 1
    (token,) = tokens
    cd = V.cdef(st, token)
    assert cd.is_token and cd.name == "Daughtress" and cd.traits == ("New UNE",)
    assert rested(st, token)
    assert (ap(st, token), V.hp_of(st, V.derived(st), token)) == (0, 1)


@pytest.mark.card("GD03-045")
def test_gd03_045_ap_plus_1_while_you_have_a_unit_token() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    balient = sc.add(0, "GD03-045")
    flyer = sc.add(0, "GD03-044", Zone.HAND)
    enemy_flyer = sc.add(1, "GD03-044")
    st = sc.start()
    assert ap(st, balient) == 2
    play(st, flyer)
    assert ap(st, balient) == 3
    assert zone_of(st, enemy_flyer) is Zone.BATTLE


# ---------------------------------------------------------------------------------------------
# GD03-048 GFreD (Lv4)


@pytest.mark.card("GD03-048")
@pytest.mark.rule("13-2-5-1")
def test_gd03_048_burst_deploys_gfred_token_with_3_or_less_enemy_shields() -> None:
    sc = Scenario()
    attacker = sc.add(0, ZAKU)
    sc.shields(0, ZAKU, ZAKU, ZAKU)
    sc.shields(1, "GD03-048")
    st = sc.start()
    attack(st, attacker)
    pass_all(st)
    yes(st)
    tokens = [u for u in st.zones[1][Zone.BATTLE] if V.cdef(st, u).is_token]
    assert len(tokens) == 1
    assert V.cdef(st, tokens[0]).name == "GFreD" and rested(st, tokens[0])
    assert ap(st, tokens[0]) == 4


@pytest.mark.card("GD03-048")
def test_gd03_048_burst_needs_3_or_less_enemy_shields() -> None:
    sc = Scenario()
    attacker = sc.add(0, ZAKU)
    sc.shields(0, ZAKU, ZAKU, ZAKU, ZAKU)
    sc.shields(1, "GD03-048")
    st = sc.start()
    attack(st, attacker)
    pass_all(st)
    yes(st)
    assert st.zones[1][Zone.BATTLE] == []


# ---------------------------------------------------------------------------------------------
# GD03-049 Gundam Exia (Trans-Am)


@pytest.mark.card("GD03-049")
@pytest.mark.ruling("GD03-049:Q224")
@pytest.mark.rule("13-1-7-1")
def test_gd03_049_destroys_enemy_with_lowest_current_hp() -> None:
    sc = Scenario()
    exia = sc.add(0, "GD03-049")
    sc.trash(0, *(["GD03-063"] * 10))
    low_current = sc.add(1, UNICORN, damage=3)  # 4 HP - 3 = 1
    low_base = sc.add(1, ZAKU)  # 2 HP
    shields = sc.shields(1, ZAKU, ZAKU, ZAKU)
    st = sc.start()
    attack(st, exia)
    pass_all(st)
    assert [zone_of(st, s) for s in shields] == [Zone.TRASH, Zone.TRASH, Zone.SHIELD]
    assert zone_of(st, low_current) is Zone.TRASH
    assert zone_of(st, low_base) is Zone.BATTLE


@pytest.mark.card("GD03-049")
@pytest.mark.ruling("GD03-049:Q225")
def test_gd03_049_tied_lowest_hp_chooser_picks_one() -> None:
    sc = Scenario()
    exia = sc.add(0, "GD03-049")
    sc.trash(0, *(["GD03-063"] * 10))
    a = sc.add(1, ZAKU)
    b = sc.add(1, "GD03-032")  # also 2 HP
    c = sc.add(1, UNICORN)
    sc.shields(1, ZAKU, ZAKU, ZAKU)
    st = sc.start()
    attack(st, exia)
    pass_all(st)
    assert select_options(st) == {a, b}
    pick(st, b)
    assert zone_of(st, b) is Zone.TRASH
    assert zone_of(st, a) is Zone.BATTLE and zone_of(st, c) is Zone.BATTLE


@pytest.mark.card("GD03-049")
def test_gd03_049_needs_10_cb_cards_in_trash() -> None:
    sc = Scenario()
    exia = sc.add(0, "GD03-049")
    sc.trash(0, *(["GD03-063"] * 9))
    target = sc.add(1, ZAKU)
    sc.shields(1, ZAKU, ZAKU, ZAKU)
    st = sc.start()
    attack(st, exia)
    pass_all(st)
    assert zone_of(st, target) is Zone.BATTLE


# ---------------------------------------------------------------------------------------------
# GD03-050 Gundam Barbatos Lupus


@pytest.mark.card("GD03-050")
@pytest.mark.ruling("GD03-050:Q226")
def test_gd03_050_exile_three_unit_cards_to_deal_2() -> None:
    sc = Scenario()
    lupus = sc.add(0, "GD03-050")
    cards = sc.trash(0, "ST05-004", "ST05-004", "ST05-006")
    target = sc.add(1, GYAN)
    st = sc.start()
    activate(st, lupus)
    assert all(zone_of(st, u) is Zone.REMOVAL for u in cards)
    assert st.cards[target].damage == 2


@pytest.mark.card("GD03-050")
def test_gd03_050_needs_three_unit_cards() -> None:
    sc = Scenario()
    lupus = sc.add(0, "GD03-050")
    sc.trash(0, "ST05-004", "ST05-006", "ST05-010")  # the Pilot card is not a Unit card
    sc.add(1, GYAN)
    st = sc.start()
    assert not has_action(st, A.ACTIVATE, lupus)


# ---------------------------------------------------------------------------------------------
# GD03-051 Gundam X Divider, GD03-058 Farsia, GD03-062 GX-Bit

JAMIL = "GD03-096"  # Jamil Neate: Purple Pilot, links Gundam X Divider


def _active_resources(st: GameState, player: int = 0) -> int:
    return sum(1 for u in st.zones[player][Zone.RESOURCE_AREA] if not rested(st, u))


@pytest.mark.card("GD03-051", "GD03-062")
@pytest.mark.ruling("GD03-051:Q227")
def test_gd03_051_pays_cost_to_deploy_from_trash_and_its_deploy_triggers() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    divider = sc.add(0, "GD03-051")
    pilot = sc.add(0, JAMIL, Zone.HAND)
    (gxbit,) = sc.trash(0, "GD03-062")
    victim = sc.add(1, ZAKU)
    st = sc.start()
    play(st, pilot, onto=divider)
    assert select_options(st) == {gxbit}
    pick(st, gxbit)
    assert zone_of(st, gxbit) is Zone.BATTLE
    assert _active_resources(st) == 2  # 1 for Jamil, 2 for GX-Bit
    assert zone_of(st, victim) is Zone.TRASH  # GX-Bit deployed from the trash deals 2
    assert any(h.kind == "cost_paid" and h.uid == divider for h in st.history)


@pytest.mark.card("GD03-051")
def test_gd03_051_not_deployed_when_cost_cannot_be_paid() -> None:
    sc = Scenario()
    sc.resources(0, 4, rested=2)
    divider = sc.add(0, "GD03-051")
    pilot = sc.add(0, JAMIL, Zone.HAND)
    (gaplant,) = sc.trash(0, GAPLANT)
    st = sc.start()
    play(st, pilot, onto=divider)
    pick(st, gaplant)
    assert zone_of(st, gaplant) is Zone.TRASH
    assert _active_resources(st) == 1


@pytest.mark.card("GD03-051", "GD03-058")
@pytest.mark.ruling("GD03-058:Q231")
def test_gd03_058_costs_one_less_from_trash_level_unchanged() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    divider = sc.add(0, "GD03-051")
    pilot = sc.add(0, JAMIL, Zone.HAND)
    (farsia,) = sc.trash(0, "GD03-058")
    in_hand = sc.add(0, "GD03-058", Zone.HAND)
    st = sc.start()
    dv = V.derived(st)
    assert V.play_cost(st, dv, in_hand) == 2
    assert V.play_cost(st, dv, farsia) == 1
    assert V.play_level(st, dv, farsia) == 2
    play(st, pilot, onto=divider)
    pick(st, farsia)
    assert zone_of(st, farsia) is Zone.BATTLE
    assert _active_resources(st) == 3


@pytest.mark.card("GD03-062")
def test_gd03_062_no_damage_when_deployed_from_hand() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    card = sc.add(0, "GD03-062", Zone.HAND)
    victim = sc.add(1, ZAKU)
    st = sc.start()
    play(st, card)
    assert st.cards[victim].damage == 0


# ---------------------------------------------------------------------------------------------
# GD03-052 Gundam Virtue

TIERIA = "ST07-010"  # (CB) Pilot +1/+1
ALPHA_AZIERU = "GD05-054"  # "When one of your Units is destroyed by an effect, draw 1."


@pytest.mark.card("GD03-052")
@pytest.mark.ruling("GD03-052:Q228")
def test_gd03_052_destroys_damaged_enemy_by_effect_with_cb_pilot() -> None:
    sc = Scenario()
    virtue = sc.add(0, "GD03-052", pilot=TIERIA)  # 4/4
    victim = sc.add(1, "GD03-064", rested=True)  # Defurse Lv5 2/5
    sc.add(1, ALPHA_AZIERU, rested=True)
    st = sc.start()
    before = hand_size(st, 1)
    attack(st, virtue, victim)
    pass_all(st)
    assert zone_of(st, victim) is Zone.TRASH
    assert hand_size(st, 1) == before + 1  # destroyed by an effect, not by battle damage


@pytest.mark.card("GD03-052")
def test_gd03_052_needs_cb_pilot_and_lv5_or_lower() -> None:
    sc = Scenario()
    virtue = sc.add(0, "GD03-052")
    victim = sc.add(1, "GD03-064", rested=True)
    other = sc.add(0, "GD03-052", pilot=TIERIA)
    big = sc.add(1, UNICORN_DM, rested=True)
    st = sc.start()
    assert keywords(st, virtue) == {"Support": 2}
    attack(st, virtue, victim)  # a (CB) Pilot is in play (on the other Virtue): destroyed
    pass_all(st)
    assert zone_of(st, victim) is Zone.TRASH
    attack(st, other, big)  # Lv.8: not destroyed by the effect
    pass_all(st)
    assert zone_of(st, big) is Zone.BATTLE


@pytest.mark.card("GD03-052")
def test_gd03_052_no_cb_pilot_no_destroy() -> None:
    sc = Scenario()
    virtue = sc.add(0, "GD03-052", pilot=NEUTRAL_PILOT)
    victim = sc.add(1, "GD03-064", rested=True)
    st = sc.start()
    attack(st, virtue, victim)
    pass_all(st)
    assert zone_of(st, victim) is Zone.BATTLE
    assert st.cards[victim].damage == 4


# ---------------------------------------------------------------------------------------------
# GD03-053 Gundam Gusion Rebake Full City

ROUEI = "GD03-067"  # 【Deploy】You may choose 1 of your Units. Deal 1 damage to it. AP+1.


@pytest.mark.card("GD03-053", "GD03-067")
def test_gd03_053_effect_damage_to_tekkadan_unit_rests_enemy() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    gusion = sc.add(0, "GD03-053", pilot=NEUTRAL_PILOT)
    rouei = sc.add(0, ROUEI, Zone.HAND)
    lv4 = sc.add(1, GAPLANT)
    lv5 = sc.add(1, UNICORN)
    st = sc.start()
    assert keywords(st, gusion) == {"Blocker": 1}
    play(st, rouei)
    pick(st, gusion)
    assert st.cards[gusion].damage == 1
    assert ap(st, gusion) == 7  # 5 + pilot 1 + Rouei 1
    assert rested(st, lv4)
    assert not rested(st, lv5)


@pytest.mark.card("GD03-053")
def test_gd03_053_needs_pilot() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    gusion = sc.add(0, "GD03-053")
    rouei = sc.add(0, ROUEI, Zone.HAND)
    lv4 = sc.add(1, GAPLANT)
    st = sc.start()
    play(st, rouei)
    pick(st, gusion)
    assert not rested(st, lv4)


# ---------------------------------------------------------------------------------------------
# GD03-054 Zeydra

DESIL = "GD02-096"  # (Vagan) (X-Rounder) Pilot


@pytest.mark.card("GD03-054")
@pytest.mark.ruling("GD03-054:Q229")
def test_gd03_054_exile_four_vagan_cards_to_destroy() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    zeydra = sc.add(0, "GD03-054")
    pilot = sc.add(0, DESIL, Zone.HAND)
    cards = sc.trash(0, "GD02-067", "GD02-067", "GD02-067", "GD02-067")
    lv4 = sc.add(1, GAPLANT)
    lv5 = sc.add(1, UNICORN)
    st = sc.start()
    assert "High-Maneuver" in keywords(st, zeydra)
    play(st, pilot, onto=zeydra)
    yes(st)
    assert all(zone_of(st, u) is Zone.REMOVAL for u in cards)
    assert zone_of(st, lv4) is Zone.TRASH
    assert zone_of(st, lv5) is Zone.BATTLE


@pytest.mark.card("GD03-054")
def test_gd03_054_needs_four_vagan_cards() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    zeydra = sc.add(0, "GD03-054")
    pilot = sc.add(0, DESIL, Zone.HAND)
    sc.trash(0, "GD02-067", "GD02-067", "GD02-067")
    lv4 = sc.add(1, GAPLANT)
    st = sc.start()
    play(st, pilot, onto=zeydra)
    assert pending(st) is K.MAIN
    assert zone_of(st, lv4) is Zone.BATTLE


@pytest.mark.card("GD03-054")
def test_gd03_054_needs_an_x_rounder_pilot() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    zeydra = sc.add(0, "GD03-054")
    neutral = sc.add(0, NEUTRAL_PILOT, Zone.HAND)
    sc.trash(0, "GD02-067", "GD02-067", "GD02-067", "GD02-067")
    lv4 = sc.add(1, GAPLANT)
    st = sc.start()
    play(st, neutral, onto=zeydra)
    assert pending(st) is K.MAIN
    assert zone_of(st, lv4) is Zone.BATTLE


# ---------------------------------------------------------------------------------------------
# GD03-055 Gundam Hajiroboshi (2nd Form)


@pytest.mark.card("GD03-055")
def test_gd03_055_purple_pilot_destroys_lv2_or_lower() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    unit = sc.add(0, "GD03-055")
    pilot = sc.add(0, JAMIL, Zone.HAND)
    lv2 = sc.add(1, ZAKU)
    lv4 = sc.add(1, GAPLANT)
    st = sc.start()
    play(st, pilot, onto=unit)
    assert zone_of(st, lv2) is Zone.TRASH
    assert zone_of(st, lv4) is Zone.BATTLE


@pytest.mark.card("GD03-055")
def test_gd03_055_non_purple_pilot_does_nothing() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    unit = sc.add(0, "GD03-055")
    pilot = sc.add(0, NEUTRAL_PILOT, Zone.HAND)
    lv2 = sc.add(1, ZAKU)
    st = sc.start()
    play(st, pilot, onto=unit)
    assert zone_of(st, lv2) is Zone.BATTLE


# ---------------------------------------------------------------------------------------------
# GD03-056 Gundam Barbatos Adapt


@pytest.mark.card("GD03-056")
def test_gd03_056_deals_1_to_own_and_enemy_unit() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    card = sc.add(0, "GD03-056", Zone.HAND)
    own = sc.add(0, GYAN)
    enemy = sc.add(1, GYAN)
    st = sc.start()
    play(st, card)
    assert select_options(st) == {card, own}
    pick(st, own)
    assert st.cards[own].damage == 1
    assert st.cards[enemy].damage == 1
    assert st.cards[card].damage == 0


@pytest.mark.card("GD03-056")
@pytest.mark.ruling("GD03-056:Q230")
def test_gd03_056_needs_both_targets() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    card = sc.add(0, "GD03-056", Zone.HAND)
    own = sc.add(0, GYAN)
    st = sc.start()
    play(st, card)
    assert pending(st) is K.MAIN
    assert st.cards[own].damage == 0 and st.cards[card].damage == 0


# ---------------------------------------------------------------------------------------------
# GD03-057 GN Armor (Type-E), GD03-083 AEU Hellion


@pytest.mark.card("GD03-057", "GD03-083")
@pytest.mark.rule("13-1-4-1")
def test_gd03_057_and_083_blockers() -> None:
    sc = Scenario(active=1)
    attacker = sc.add(1, ZAKU)
    armor = sc.add(0, "GD03-057")
    hellion = sc.add(0, "GD03-083")
    st = sc.start()
    attack(st, attacker)
    assert pending(st) is K.BLOCK
    assert {o.a for o in st.pending.options if o.kind is A.BLOCK} == {armor, hellion}
    block(st, armor)
    pass_all(st)
    assert zone_of(st, attacker) is Zone.TRASH
    assert st.cards[armor].damage == 2


# ---------------------------------------------------------------------------------------------
# GD03-059 Zedas R


@pytest.mark.card("GD03-059")
@pytest.mark.ruling("GD03-059:Q232")
def test_gd03_059_exile_vagan_card_for_ap_plus_2() -> None:
    sc = Scenario()
    zedas_r = sc.add(0, "GD03-059")
    zedas_m = sc.add(0, "GD03-065")
    (baqto,) = sc.trash(0, "GD02-067")
    sc.shields(1, ZAKU)
    st = sc.start()
    attack(st, zedas_r)
    pick(st, baqto)
    assert zone_of(st, baqto) is Zone.REMOVAL
    assert select_options(st) == {zedas_r, zedas_m}
    pick(st, zedas_m)
    assert ap(st, zedas_m) == 5


@pytest.mark.card("GD03-059")
def test_gd03_059_optional() -> None:
    sc = Scenario()
    zedas_r = sc.add(0, "GD03-059")
    zedas_m = sc.add(0, "GD03-065")
    (baqto,) = sc.trash(0, "GD02-067")
    sc.shields(1, ZAKU)
    st = sc.start()
    attack(st, zedas_r)
    done(st)
    assert zone_of(st, baqto) is Zone.TRASH
    assert ap(st, zedas_m) == 3


# ---------------------------------------------------------------------------------------------
# GD03-060 CGS Mobile Worker (Commander Type)


def _tokens(st: GameState, player: int) -> list[int]:
    return [u for u in st.zones[player][Zone.BATTLE] if V.cdef(st, u).is_token]


@pytest.mark.card("GD03-060")
def test_gd03_060_effect_damage_deploys_token_once_per_turn() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    worker = sc.add(0, "GD03-060")
    r1 = sc.add(0, ROUEI, Zone.HAND)
    r2 = sc.add(0, ROUEI, Zone.HAND)
    st = sc.start()
    play(st, r1)
    pick(st, worker)
    tokens = _tokens(st, 0)
    assert len(tokens) == 1
    assert V.cdef(st, tokens[0]).name == "CGS Mobile Worker" and rested(st, tokens[0])
    play(st, r2)
    pick(st, worker)
    assert zone_of(st, worker) is Zone.TRASH
    assert len(_tokens(st, 0)) == 1


@pytest.mark.card("GD03-060")
def test_gd03_060_battle_damage_does_not_trigger() -> None:
    sc = Scenario()
    worker = sc.add(0, "GD03-060")  # 0/2
    enemy = sc.add(1, ZAKU, rested=True)
    st = sc.start()
    attack(st, worker, enemy)
    pass_all(st)
    assert zone_of(st, worker) is Zone.TRASH
    assert _tokens(st, 0) == []


@pytest.mark.card("GD03-060")
def test_gd03_060_not_on_opponents_turn() -> None:
    sc = Scenario(active=1)
    sc.resources(1, 4)
    adapt = sc.add(1, "GD03-056", Zone.HAND)
    worker = sc.add(0, "GD03-060", rested=True)
    st = sc.start()
    play(st, adapt)  # both targets are the only candidates
    assert st.cards[worker].damage == 1
    assert _tokens(st, 0) == []


# ---------------------------------------------------------------------------------------------
# GD03-061 Gundam Barbatos 6th Form


@pytest.mark.card("GD03-061")
@pytest.mark.faq("Q96")
def test_gd03_061_repair_3_while_at_1_current_hp() -> None:
    sc = Scenario()
    low = sc.add(0, "GD03-061", damage=3)  # 4 HP - 3 = 1
    high = sc.add(0, "GD03-061", damage=2)
    st = sc.start()
    assert keywords(st, low) == {"Repair": 3}
    assert keywords(st, high) == {}
    to_next_turn(st)
    assert st.cards[low].damage == 0
    assert st.cards[high].damage == 2


# ---------------------------------------------------------------------------------------------
# GD03-064 Defurse


@pytest.mark.card("GD03-064")
def test_gd03_064_add_x_rounder_card_then_discard() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    card = sc.add(0, "GD03-064", Zone.HAND)
    zaku = sc.add(0, ZAKU, Zone.HAND)
    (desil,) = sc.trash(0, DESIL)
    st = sc.start()
    play(st, card)
    pick(st, desil)
    assert zone_of(st, desil) is Zone.HAND
    assert pending(st) is K.DISCARD
    act(st, A.SELECT, zaku)
    assert zone_of(st, zaku) is Zone.TRASH
    assert zone_of(st, desil) is Zone.HAND


@pytest.mark.card("GD03-064")
def test_gd03_064_declining_skips_the_discard() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    card = sc.add(0, "GD03-064", Zone.HAND)
    zaku = sc.add(0, ZAKU, Zone.HAND)
    (desil,) = sc.trash(0, DESIL)
    st = sc.start()
    play(st, card)
    done(st)
    assert zone_of(st, desil) is Zone.TRASH
    assert zone_of(st, zaku) is Zone.HAND


# ---------------------------------------------------------------------------------------------
# GD03-067 Rouei


@pytest.mark.card("GD03-067")
def test_gd03_067_damage_own_unit_for_ap_plus_1() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    card = sc.add(0, ROUEI, Zone.HAND)
    own = sc.add(0, GYAN)
    st = sc.start()
    play(st, card)
    assert select_options(st) == {card, own}
    pick(st, own)
    assert st.cards[own].damage == 1
    assert ap(st, own) == 5
    to_next_turn(st)
    assert ap(st, own) == 4


@pytest.mark.card("GD03-067")
def test_gd03_067_optional() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    card = sc.add(0, ROUEI, Zone.HAND)
    own = sc.add(0, GYAN)
    st = sc.start()
    play(st, card)
    done(st)
    assert st.cards[own].damage == 0 and ap(st, own) == 4


# ---------------------------------------------------------------------------------------------
# GD03-068 Gundam Hajiroboshi


@pytest.mark.card("GD03-068")
def test_gd03_068_blocker_while_a_friendly_base_is_in_play() -> None:
    sc = Scenario()
    with_base = sc.add(0, "GD03-068")
    sc.base(0)
    without_base = sc.add(1, "GD03-068")
    st = sc.start()
    assert keywords(st, with_base) == {"Blocker": 1}
    assert keywords(st, without_base) == {}


# ---------------------------------------------------------------------------------------------
# GD03-069 Graham's Union Flag Custom

GRAHAM = "GD03-098"  # Graham Aker: (Superpower Bloc) (UN) Pilot, links


@pytest.mark.card("GD03-069")
@pytest.mark.rule("7-6-4-1")
def test_gd03_069_set_active_at_end_of_turn_it_was_paired() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    flag = sc.add(0, "GD03-069")
    pilot = sc.add(0, GRAHAM, Zone.HAND)
    sc.shields(1, ZAKU, ZAKU)
    st = sc.start()
    assert "High-Maneuver" in keywords(st, flag)
    play(st, pilot, onto=flag)
    attack(st, flag)
    pass_all(st)
    assert rested(st, flag)
    to_next_turn(st)
    assert st.active == 1
    assert not rested(st, flag)


@pytest.mark.card("GD03-069")
def test_gd03_069_not_when_paired_on_an_earlier_turn() -> None:
    sc = Scenario()
    flag = sc.add(0, "GD03-069", pilot=GRAHAM)
    sc.shields(1, ZAKU, ZAKU)
    st = sc.start()
    attack(st, flag)
    pass_all(st)
    to_next_turn(st)
    assert rested(st, flag)


@pytest.mark.card("GD03-069")
def test_gd03_069_needs_link() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    flag = sc.add(0, "GD03-069")
    pilot = sc.add(0, NEUTRAL_PILOT, Zone.HAND)
    sc.shields(1, ZAKU, ZAKU)
    st = sc.start()
    play(st, pilot, onto=flag)
    attack(st, flag)
    pass_all(st)
    to_next_turn(st)
    assert rested(st, flag)


# ---------------------------------------------------------------------------------------------
# GD03-070 Freedom Gundam


@pytest.mark.card("GD03-070")
@pytest.mark.xfail(
    strict=True,
    reason="ENGINE: battle damage to Shields ignores CANT_RECEIVE_DAMAGE rules on the Shields",
)
def test_gd03_070_rested_freedom_protects_shields_from_enemy_battle_damage() -> None:
    sc = Scenario(active=1)
    attacker = sc.add(1, ZAKU)
    sc.add(0, "GD03-070", rested=True)
    top, _ = sc.shields(0, ZAKU, ZAKU)
    st = sc.start()
    attack(st, attacker)
    pass_all(st)
    assert zone_of(st, top) is Zone.SHIELD


@pytest.mark.card("GD03-070")
def test_gd03_070_active_freedom_does_not_protect() -> None:
    sc = Scenario(active=1)
    attacker = sc.add(1, ZAKU)
    sc.add(0, "GD03-070")
    top, second = sc.shields(0, ZAKU, ZAKU)
    st = sc.start()
    attack(st, attacker)
    pass_all(st)
    assert zone_of(st, top) is Zone.TRASH and zone_of(st, second) is Zone.SHIELD


@pytest.mark.card("GD03-070")
def test_gd03_070_base_is_not_a_shield() -> None:
    sc = Scenario(active=1)
    attacker = sc.add(1, ZAKU)
    sc.add(0, "GD03-070", rested=True)
    base = sc.base(0)
    sc.shields(0, ZAKU)
    st = sc.start()
    attack(st, attacker)
    pass_all(st)
    assert st.cards[base].damage == 2


# ---------------------------------------------------------------------------------------------
# GD03-071 Z Gundam (Biosensor)


@pytest.mark.card("GD03-071")
def test_gd03_071_ap_minus_per_aeug_unit_card_fixed_at_resolution() -> None:
    sc = Scenario()
    sc.resources(0, 8)
    card = sc.add(0, "GD03-071", Zone.HAND)
    rouei = sc.add(0, ROUEI, Zone.HAND)
    sc.trash(0, "GD02-080", "GD02-080", "GD02-097")  # two AEUG Unit cards and an AEUG Pilot
    nemo = sc.add(0, "GD02-080", damage=1)
    target = sc.add(1, GYAN)
    st = sc.start()
    play(st, card)
    assert ap(st, target) == 2
    play(st, rouei)
    pick(st, nemo)
    assert zone_of(st, nemo) is Zone.TRASH  # a third AEUG Unit card in the trash
    assert ap(st, target) == 2
    to_next_turn(st)
    assert ap(st, target) == 4


# ---------------------------------------------------------------------------------------------
# GD03-072 Aile Strike Gundam


@pytest.mark.card("GD03-072")
def test_gd03_072_draw_then_discard_with_another_tsa_unit() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    card = sc.add(0, "GD03-072", Zone.HAND)
    zaku = sc.add(0, ZAKU, Zone.HAND)
    sc.add(0, "ST14-010")  # Perfect Strike Gundam (Triple Ship Alliance)
    st = sc.start()
    play(st, card)
    assert pending(st) is K.DISCARD
    act(st, A.SELECT, zaku)
    assert zone_of(st, zaku) is Zone.TRASH
    assert hand_size(st) == 1
    assert keywords(st, card) == {"Blocker": 1}


@pytest.mark.card("GD03-072")
@pytest.mark.ruling("GD03-072:Q233")
def test_gd03_072_no_discard_without_another_tsa_unit() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    card = sc.add(0, "GD03-072", Zone.HAND)
    zaku = sc.add(0, ZAKU, Zone.HAND)
    st = sc.start()
    play(st, card)
    assert pending(st) is K.MAIN
    assert zone_of(st, zaku) is Zone.HAND
    assert hand_size(st) == 1


# ---------------------------------------------------------------------------------------------
# GD03-073 Graze Ein

EIN_DALTON = "GD02-118"  # Command with 【Pilot】[Ein Dalton]


def _graze_ein_scenario(gjallarhorn: int, *, linked: bool = True) -> tuple[GameState, int, int]:
    sc = Scenario(active=1)
    attacker = sc.add(1, AGRISSA)  # 5/4
    graze = sc.add(0, "GD03-073", pilot=None if linked else NEUTRAL_PILOT)
    if linked:
        pair_raw(sc, graze, EIN_DALTON)
    sc.trash(0, *(["ST05-009"] * gjallarhorn))
    sc.shields(0, ZAKU)
    st = sc.start()
    attack(st, attacker)
    block(st, graze)
    return st, graze, attacker


@pytest.mark.card("GD03-073")
def test_gd03_073_action_gives_battling_enemy_ap_minus_3() -> None:
    st, graze, attacker = _graze_ein_scenario(6)
    assert keywords(st, graze) == {"Blocker": 1}
    assert pending(st) is K.ACTION_STEP and has_action(st, A.ACTIVATE, graze)
    activate(st, graze)
    pass_all(st)
    assert zone_of(st, attacker) is Zone.TRASH
    assert st.cards[graze].damage == 2  # 5 AP - 3; unreduced it would destroy the 7/4 Graze Ein


@pytest.mark.card("GD03-073")
def test_gd03_073_needs_six_gjallarhorn_cards_and_link() -> None:
    st, graze, _ = _graze_ein_scenario(5)
    assert not has_action(st, A.ACTIVATE, graze)
    st2, graze2, _ = _graze_ein_scenario(6, linked=False)
    assert not has_action(st2, A.ACTIVATE, graze2)


# ---------------------------------------------------------------------------------------------
# GD03-074 Tieren Taozi


@pytest.mark.card("GD03-074")
def test_gd03_074_needs_another_superpower_bloc_unit() -> None:
    sc = Scenario(active=1)
    attacker = sc.add(1, ZAKU)
    taozi = sc.add(0, "GD03-074", pilot=NEUTRAL_PILOT, rested=True)
    other = sc.add(0, ZAKU, rested=True)
    st = sc.start()
    assert attack_targets(st, attacker) == {PLAYER_TARGET, taozi, other}


@pytest.mark.card("GD03-074")
def test_gd03_074_attracts_with_another_superpower_bloc_unit() -> None:
    sc = Scenario(active=1)
    attacker = sc.add(1, ZAKU)
    taozi = sc.add(0, "GD03-074", pilot=NEUTRAL_PILOT, rested=True)
    sc.add(0, AGRISSA)
    sc.add(0, ZAKU, rested=True)
    st = sc.start()
    assert attack_targets(st, attacker) == {taozi}


# ---------------------------------------------------------------------------------------------
# GD03-075 Super Gundam


@pytest.mark.card("GD03-075")
def test_gd03_075_attack_gives_unpaired_enemy_ap_minus_2() -> None:
    sc = Scenario()
    sup = sc.add(0, "GD03-075", pilot="GD02-097")  # Kamille Bidan (AEUG) links
    unpaired = sc.add(1, GYAN)
    paired = sc.add(1, GYAN, pilot=NEUTRAL_PILOT)
    sc.shields(1, ZAKU)
    st = sc.start()
    attack(st, sup)
    assert ap(st, unpaired) == 2
    assert ap(st, paired) == 5


@pytest.mark.card("GD03-075")
def test_gd03_075_needs_link() -> None:
    sc = Scenario()
    sup = sc.add(0, "GD03-075", pilot=NEUTRAL_PILOT)
    unpaired = sc.add(1, GYAN)
    sc.shields(1, ZAKU)
    st = sc.start()
    attack(st, sup)
    assert ap(st, unpaired) == 4


# ---------------------------------------------------------------------------------------------
# GD03-076 Freedom Gundam (METEOR)

PERFECT_STRIKE = "ST14-010"  # (Triple Ship Alliance) 4/3


@pytest.mark.card("GD03-076")
def test_gd03_076_may_return_unit_dealt_battle_damage() -> None:
    sc = Scenario()
    sc.add(0, "GD03-076")
    striker = sc.add(0, PERFECT_STRIKE)
    target = sc.add(1, GYAN, rested=True)  # 4/5 survives 4 damage
    st = sc.start()
    attack(st, striker, target)
    pass_all(st)
    assert pending(st) is K.YES_NO
    yes(st)
    assert zone_of(st, target) is Zone.HAND


@pytest.mark.card("GD03-076")
def test_gd03_076_optional() -> None:
    sc = Scenario()
    sc.add(0, "GD03-076")
    striker = sc.add(0, PERFECT_STRIKE)
    target = sc.add(1, GYAN, rested=True)
    st = sc.start()
    attack(st, striker, target)
    pass_all(st)
    no(st)
    assert zone_of(st, target) is Zone.BATTLE


@pytest.mark.card("GD03-076")
@pytest.mark.ruling("GD03-076:Q235")
def test_gd03_076_unit_destroyed_by_the_battle_damage_is_not_returned() -> None:
    sc = Scenario()
    sc.add(0, "GD03-076")
    striker = sc.add(0, PERFECT_STRIKE)
    target = sc.add(1, ZAKU, rested=True)
    st = sc.start()
    attack(st, striker, target)
    pass_all(st)
    assert pending(st) is K.MAIN
    assert zone_of(st, target) is Zone.TRASH


@pytest.mark.card("GD03-076")
@pytest.mark.xfail(
    strict=True,
    reason="ENGINE: 【Once per Turn】 is consumed when a triggered effect resolves without "
    "performing its action (resolution ruling:GD02-002:Q197)",
)
def test_gd03_076_once_per_turn_not_used_up_when_nothing_was_returned() -> None:
    sc = Scenario()
    sc.add(0, "GD03-076")
    s1 = sc.add(0, PERFECT_STRIKE)
    s2 = sc.add(0, PERFECT_STRIKE)
    small = sc.add(1, ZAKU, rested=True)
    big = sc.add(1, GYAN, rested=True)
    st = sc.start()
    attack(st, s1, small)
    pass_all(st)
    assert zone_of(st, small) is Zone.TRASH
    attack(st, s2, big)
    pass_all(st)
    assert pending(st) is K.YES_NO
    yes(st)
    assert zone_of(st, big) is Zone.HAND


@pytest.mark.card("GD03-076")
def test_gd03_076_once_per_turn() -> None:
    sc = Scenario()
    sc.add(0, "GD03-076")
    s1 = sc.add(0, PERFECT_STRIKE)
    s2 = sc.add(0, PERFECT_STRIKE)
    t1 = sc.add(1, GYAN, rested=True)
    t2 = sc.add(1, GYAN, rested=True)
    st = sc.start()
    attack(st, s1, t1)
    pass_all(st)
    yes(st)
    attack(st, s2, t2)
    pass_all(st)
    assert pending(st) is K.MAIN
    assert zone_of(st, t1) is Zone.HAND and zone_of(st, t2) is Zone.BATTLE


# ---------------------------------------------------------------------------------------------
# GD03-077 Justice Gundam (METEOR)

ATHRUN = "ST04-011"


@pytest.mark.card("GD03-077")
@pytest.mark.faq("Q96")
def test_gd03_077_when_linked_returns_units_with_3_or_less_current_hp() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    justice = sc.add(0, "GD03-077")
    pilot = sc.add(0, ATHRUN, Zone.HAND)
    small = sc.add(1, ZAKU)
    damaged = sc.add(1, UNICORN, damage=1)
    healthy = sc.add(1, UNICORN)
    st = sc.start()
    play(st, pilot, onto=justice)
    resolve_orders(st)
    assert select_options(st) == {small, damaged}
    pick(st, small, damaged)
    resolve_orders(st)
    assert zone_of(st, small) is Zone.HAND and zone_of(st, damaged) is Zone.HAND
    assert zone_of(st, healthy) is Zone.BATTLE


# ---------------------------------------------------------------------------------------------
# GD03-078 Tieren High Mobility Type

SERGEI = "GD03-122"  # Command with 【Pilot】[Sergei Smirnov]


@pytest.mark.card("GD03-078")
def test_gd03_078_destroyed_while_linked_returns_paired_card_to_hand() -> None:
    sc = Scenario()
    tieren = sc.add(0, "GD03-078", pilot=SERGEI)  # 3/2
    pilot = sc.st.cards[tieren].pair
    victim = sc.add(1, ZAKU, rested=True)
    st = sc.start()
    attack(st, tieren, victim)
    pass_all(st)
    assert zone_of(st, tieren) is Zone.TRASH
    assert zone_of(st, pilot) is Zone.HAND


@pytest.mark.card("GD03-078")
def test_gd03_078_needs_link() -> None:
    sc = Scenario()
    tieren = sc.add(0, "GD03-078", pilot=NEUTRAL_PILOT)
    pilot = sc.st.cards[tieren].pair
    victim = sc.add(1, ZAKU, rested=True)
    st = sc.start()
    attack(st, tieren, victim)
    pass_all(st)
    assert zone_of(st, tieren) is Zone.TRASH
    assert zone_of(st, pilot) is Zone.TRASH


# ---------------------------------------------------------------------------------------------
# GD03-079 G-Defenser

RICK_DIAS = "GD02-075"  # 【Attack】Choose 1 active friendly Base. Rest it. If you do, ...


@pytest.mark.card("GD03-079")
@pytest.mark.rule("10-1-9")
def test_gd03_079_rest_instead_of_the_base() -> None:
    sc = Scenario()
    dias = sc.add(0, RICK_DIAS)
    defenser = sc.add(0, "GD03-079")
    base = sc.base(0)
    target = sc.add(1, "GD03-072")  # Aile Strike Gundam Lv4 3/4 <Blocker>
    sc.shields(1, ZAKU)
    st = sc.start()
    attack(st, dias)
    assert pending(st) is K.SELECT and st.pending is not None
    assert st.pending.ctx("rest_sub") == base
    act(st, A.SELECT, defenser)
    assert rested(st, defenser)
    assert not rested(st, base)
    assert pending(st) is K.BLOCK
    assert ap(st, target) == 1  # "If you do" succeeded: AP-2 during this battle


@pytest.mark.card("GD03-079")
def test_gd03_079_substitution_is_optional() -> None:
    sc = Scenario()
    dias = sc.add(0, RICK_DIAS)
    defenser = sc.add(0, "GD03-079")
    base = sc.base(0)
    sc.shields(1, ZAKU)
    st = sc.start()
    attack(st, dias)
    done(st)
    assert rested(st, base)
    assert not rested(st, defenser)


@pytest.mark.card("GD03-079")
@pytest.mark.ruling("GD03-079:Q425")
def test_gd03_079_no_base_nothing_to_substitute() -> None:
    sc = Scenario()
    zeta = sc.add(0, "GD02-069", pilot="GD02-097")  # Zeta Gundam linked with Kamille Bidan
    sc.add(0, "GD03-079")
    st = sc.start()
    assert not has_action(st, A.ACTIVATE, zeta)


# ---------------------------------------------------------------------------------------------
# GD03-080 Gundam Kimaris Trooper (Trooper Mode)

GAELIO = "GD02-099"


@pytest.mark.card("GD03-080")
def test_gd03_080_when_linked_adds_gjallarhorn_command_from_trash() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    kimaris = sc.add(0, "GD03-080")
    pilot = sc.add(0, GAELIO, Zone.HAND)
    command, other = sc.trash(0, "GD02-119", "GD03-107")
    st = sc.start()
    play(st, pilot, onto=kimaris)
    resolve_orders(st)
    assert zone_of(st, command) is Zone.HAND
    assert zone_of(st, other) is Zone.TRASH


# ---------------------------------------------------------------------------------------------
# GD03-081 AEU Enact Demonstration Color


@pytest.mark.card("GD03-081")
def test_gd03_081_attacks_only_after_a_superpower_bloc_unit_is_deployed() -> None:
    sc = Scenario()
    sc.resources(0, 1)
    enact = sc.add(0, "GD03-081")
    hellion = sc.add(0, "GD03-083", Zone.HAND)
    sc.shields(1, ZAKU)
    st = sc.start()
    assert not has_action(st, A.ATTACK, enact)
    play(st, hellion)
    assert has_action(st, A.ATTACK, enact)
    to_next_turn(st)
    to_next_turn(st)
    assert not has_action(st, A.ATTACK, enact)


@pytest.mark.card("GD03-081")
@pytest.mark.ruling("GD03-081:Q236")
@pytest.mark.rule("3-2-6-3")
def test_gd03_081_own_deployment_counts_and_link_unit_attacks_that_turn() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    enact = sc.add(0, "GD03-081", Zone.HAND)
    pilot = sc.add(0, GRAHAM, Zone.HAND)
    sc.shields(1, ZAKU)
    st = sc.start()
    play(st, enact)
    assert not has_action(st, A.ATTACK, enact)  # deployed this turn, not a Link Unit yet
    play(st, pilot, onto=enact)
    assert has_action(st, A.ATTACK, enact)


# ---------------------------------------------------------------------------------------------
# GD03-082 Union Flag


@pytest.mark.card("GD03-082")
def test_gd03_082_costs_one_less_with_two_superpower_bloc_or_un_units() -> None:
    sc = Scenario()
    sc.resources(0, 3, rested=2)
    card = sc.add(0, "GD03-082", Zone.HAND)
    sc.add(0, AGRISSA)
    sc.add(0, "GD03-083")
    st = sc.start()
    play(st, card)
    assert zone_of(st, card) is Zone.BATTLE


@pytest.mark.card("GD03-082")
@pytest.mark.ruling("GD03-082:Q237")
def test_gd03_082_level_not_reduced_and_needs_two_units() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    card = sc.add(0, "GD03-082", Zone.HAND)
    sc.add(0, AGRISSA)
    sc.add(0, "GD03-083")
    st = sc.start()
    assert V.play_cost(st, V.derived(st), card) == 1
    assert not has_action(st, A.PLAY_UNIT, card)  # still Lv.3
    sc2 = Scenario()
    sc2.resources(0, 3)
    card2 = sc2.add(0, "GD03-082", Zone.HAND)
    sc2.add(0, AGRISSA)
    st2 = sc2.start()
    assert V.play_cost(st2, V.derived(st2), card2) == 2
