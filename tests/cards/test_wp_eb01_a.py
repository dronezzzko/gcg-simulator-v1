"""Behaviour tests for EB01-001..EB01-046 (work package WP-EB01-A)."""

from __future__ import annotations

import pytest

from gcg_sim.effects.registry import get_registry
from gcg_sim.engine.game import ALT_PLAY_BASE
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
    numbers_in,
    options,
    pass_all,
    play,
    select,
    to_next_turn,
    yes,
    zone_of,
)

A = ActionKind

VANILLA = "GD01-060"  # Zaku Mariner: vanilla Lv2 2/2 (Zeon)
GG_LV2 = "EB01-012"  # Dom Bein Nichts: vanilla Lv2 2/2 (G Generation)
GG_LV3 = "EB01-016"  # Tornado Gundam: vanilla Lv3 3/3 (G Generation)
GG_LV3_4AP = "EB01-032"  # Gundam Ez8 High Mobility Custom: vanilla Lv3 4/3 (G Generation)
GG_LV5 = "EB01-007"  # Gundam TR-1 "Hazel-Rah": vanilla Lv5 5/4 (G Generation)
BLOCKER_LV1 = "EB01-011"  # Beginning Gundam: Lv1 1/1 <Blocker>
BLOCKER_LV4 = "GD01-072"  # Launcher Strike Gundam: Lv4 3/4 <Blocker>
BLOCKER_LV6 = "EB01-044"  # Justice Gundam (EX): Lv6 5/4 <Blocker>
LV8_UNIT = "GD03-034"  # GQuuuuuuX (Omega Psycommu): Lv8 6/5
GG_PILOT = "EB01-071"  # Ittou Tsurugi: Lv4 cost 1 Pilot (G Generation)(Attack)
DURABILITY_PILOT = "EB01-069"  # Beside Pain: Pilot (G Generation)(Durability)
OTHER_PILOT = "GD01-089"  # Riddhe Marcenas: Lv3 cost 1 Pilot (Earth Federation)
COMMAND = "GD01-115"  # Zeon Remnant Forces: Lv2 cost 1, 【Main】/【Action】deal 1 damage


def hand_size(st: GameState, player: int) -> int:
    return len(st.zones[player][Zone.HAND])


def selectable(st: GameState) -> set[int]:
    return {o.a for o in options(st) if o.kind is A.SELECT}


def pending_kind(st: GameState) -> DecisionKind | None:
    return st.pending.kind if st.pending is not None else None


def ex_resources(st: GameState, player: int) -> int:
    ex = get_registry().db.ex_resource.card_number
    return numbers_in(st, player, Zone.RESOURCE_AREA).count(ex)


# ---------------------------------------------------------------------------------------------
# EB01-001 Gundam Astray Red Frame Custom (EX)


@pytest.mark.card("EB01-001")
@pytest.mark.rule("13-2-1-1", "13-2-13-1", "7-2-3-1")
def test_eb01_001_rests_damaged_enemy_and_freezes_it_through_the_next_start_phase() -> None:
    sc = Scenario()
    red = sc.add(0, "EB01-001")
    commands = sc.trash(0, COMMAND, COMMAND, COMMAND, COMMAND)
    target = sc.add(1, VANILLA, damage=1)
    healthy = sc.add(1, GG_LV3)
    control = sc.add(1, VANILLA, rested=True)
    st = sc.start()
    activate(st, red)
    select(st, commands[0], commands[1])
    assert [zone_of(st, u) for u in commands] == [Zone.REMOVAL] * 2 + [Zone.TRASH] * 2
    assert st.cards[target].rested
    assert not st.cards[healthy].rested
    assert not has_action(st, A.ACTIVATE, red)  # 【Once per Turn】
    to_next_turn(st)
    assert st.active == 1
    assert st.cards[target].rested
    assert not st.cards[control].rested
    to_next_turn(st)
    to_next_turn(st)
    assert st.active == 1
    assert not st.cards[target].rested


@pytest.mark.card("EB01-001")
def test_eb01_001_cost_needs_two_command_cards_in_trash() -> None:
    sc = Scenario()
    red = sc.add(0, "EB01-001")
    sc.trash(0, COMMAND, VANILLA)
    sc.add(1, VANILLA, damage=1)
    st = sc.start()
    assert not has_action(st, A.ACTIVATE, red)


@pytest.mark.card("EB01-001")
@pytest.mark.rule("10-2-2")
def test_eb01_001_needs_a_damaged_enemy_unit_of_lv7_or_lower() -> None:
    sc = Scenario()
    red = sc.add(0, "EB01-001")
    sc.trash(0, COMMAND, COMMAND)
    sc.add(1, LV8_UNIT, damage=1)
    sc.add(1, VANILLA)
    st = sc.start()
    assert not has_action(st, A.ACTIVATE, red)


@pytest.mark.card("EB01-001")
def test_eb01_001_never_freezes_its_own_side() -> None:
    sc = Scenario(active=1)
    red = sc.add(0, "EB01-001", rested=True)
    sc.trash(0, "EB01-001")
    ally = sc.add(0, VANILLA, rested=True)
    st = sc.start()
    to_next_turn(st)
    assert st.active == 0
    assert not st.cards[red].rested
    assert not st.cards[ally].rested


@pytest.mark.card("EB01-001", "EB01-005")
def test_eb01_001_freeze_only_blocks_the_start_phase_untap() -> None:
    sc = Scenario()
    sc.resources(0, 7)
    red = sc.add(0, "EB01-001")
    sc.trash(0, COMMAND, COMMAND)
    target = sc.add(1, VANILLA, damage=1)
    zeta = sc.add(0, "EB01-005", Zone.HAND)
    st = sc.start()
    activate(st, red)
    assert st.cards[target].rested
    play(st, zeta)
    assert not st.cards[target].rested


# ---------------------------------------------------------------------------------------------
# EB01-002 Hi-Nu Gundam (EX)


@pytest.mark.card("EB01-002")
def test_eb01_002_deploy_rests_an_enemy_unit_when_another_gg_unit_is_in_play() -> None:
    sc = Scenario()
    sc.resources(0, 8)
    hinu = sc.add(0, "EB01-002", Zone.HAND)
    mine = sc.add(0, GG_LV3)
    e1 = sc.add(1, VANILLA)
    e2 = sc.add(1, VANILLA)
    st = sc.start()
    play(st, hinu)
    assert pending_kind(st) is DecisionKind.SELECT
    assert selectable(st) == {e1, e2}
    select(st, e2)
    assert st.cards[e2].rested
    assert not st.cards[e1].rested
    assert not st.cards[mine].rested


@pytest.mark.card("EB01-002")
def test_eb01_002_deploy_does_nothing_without_another_gg_unit() -> None:
    sc = Scenario()
    sc.resources(0, 8)
    hinu = sc.add(0, "EB01-002", Zone.HAND)
    sc.add(0, VANILLA)
    e1 = sc.add(1, VANILLA)
    st = sc.start()
    play(st, hinu)
    assert pending_kind(st) is DecisionKind.MAIN
    assert not st.cards[e1].rested


@pytest.mark.card("EB01-002")
@pytest.mark.ruling("EB01-002:Q310")
@pytest.mark.rule("13-2-12-1", "13-2-13-1")
def test_eb01_002_linked_attack_sets_itself_active_counting_both_sides() -> None:
    sc = Scenario()
    hinu = sc.add(0, "EB01-002", pilot=GG_PILOT)
    sc.add(0, GG_LV3, rested=True)
    sc.add(1, VANILLA, rested=True)
    sc.add(1, VANILLA, rested=True)
    sc.shields(1, VANILLA, VANILLA, VANILLA)
    st = sc.start()
    attack(st, hinu)
    pass_all(st)
    assert not st.cards[hinu].rested
    attack(st, hinu)
    pass_all(st)
    assert st.cards[hinu].rested  # 【Once per Turn】


@pytest.mark.card("EB01-002")
def test_eb01_002_attack_needs_three_other_rested_units() -> None:
    sc = Scenario()
    hinu = sc.add(0, "EB01-002", pilot=GG_PILOT)
    sc.add(1, VANILLA, rested=True)
    sc.add(1, VANILLA, rested=True)
    sc.shields(1, VANILLA)
    st = sc.start()
    attack(st, hinu)
    pass_all(st)
    assert st.cards[hinu].rested


@pytest.mark.card("EB01-002")
def test_eb01_002_attack_effect_needs_link() -> None:
    sc = Scenario()
    hinu = sc.add(0, "EB01-002", pilot=OTHER_PILOT)
    sc.add(0, GG_LV3, rested=True)
    sc.add(1, VANILLA, rested=True)
    sc.add(1, VANILLA, rested=True)
    sc.shields(1, VANILLA)
    st = sc.start()
    attack(st, hinu)
    pass_all(st)
    assert st.cards[hinu].rested


# ---------------------------------------------------------------------------------------------
# EB01-003 Narrative Gundam A-Packs (EX)


@pytest.mark.card("EB01-003")
@pytest.mark.ruling("EB01-003:Q311")
def test_eb01_003_end_of_turn_rests_all_units_and_draws() -> None:
    sc = Scenario()
    narrative = sc.add(0, "EB01-003", rested=True)
    f1 = sc.add(0, VANILLA)
    f2 = sc.add(0, VANILLA)
    sc.add(1, VANILLA)
    st = sc.start()
    assert keywords(st, narrative).get("Repair") == 2
    before = hand_size(st, 0)
    to_next_turn(st)
    assert st.active == 1
    assert st.cards[f1].rested and st.cards[f2].rested  # Q311: friendly Units too
    assert hand_size(st, 0) == before + 1  # three Units rested (two friendly, one enemy)


@pytest.mark.card("EB01-003")
def test_eb01_003_no_draw_when_fewer_than_three_units_were_rested() -> None:
    sc = Scenario()
    sc.add(0, "EB01-003", rested=True)
    f1 = sc.add(0, VANILLA)
    sc.add(0, VANILLA, rested=True)
    sc.add(1, VANILLA)
    sc.add(1, VANILLA, rested=True)
    st = sc.start()
    before = hand_size(st, 0)
    to_next_turn(st)
    assert st.cards[f1].rested
    assert hand_size(st, 0) == before


@pytest.mark.card("EB01-003")
def test_eb01_003_does_nothing_while_active() -> None:
    sc = Scenario()
    sc.add(0, "EB01-003")
    f1 = sc.add(0, VANILLA)
    f2 = sc.add(0, VANILLA)
    f3 = sc.add(0, VANILLA)
    st = sc.start()
    before = hand_size(st, 0)
    to_next_turn(st)
    assert not any(st.cards[u].rested for u in (f1, f2, f3))
    assert hand_size(st, 0) == before


# ---------------------------------------------------------------------------------------------
# EB01-004 Gundam Barbatos Lupus Rex (EX)


@pytest.mark.card("EB01-004")
@pytest.mark.rule("13-1-1-1")
def test_eb01_004_repair_recovery_deals_one_damage_to_a_rested_enemy() -> None:
    sc = Scenario()
    rex = sc.add(0, "EB01-004", damage=2)
    enemy = sc.add(1, BLOCKER_LV4, rested=True)
    st = sc.start()
    assert keywords(st, rex).get("Repair") == 2
    to_next_turn(st)
    assert st.cards[rex].damage == 0
    assert st.cards[enemy].damage == 1


@pytest.mark.card("EB01-004", "EB01-018")
@pytest.mark.rule("13-2-13-1")
def test_eb01_004_triggers_on_any_recovery_once_per_turn() -> None:
    sc = Scenario()
    rex = sc.add(0, "EB01-004", damage=2)
    blue = sc.add(0, "EB01-018")
    enemy = sc.add(1, BLOCKER_LV4, rested=True)
    sc.shields(1, VANILLA, VANILLA)
    st = sc.start()
    attack(st, blue)
    assert selectable(st) == {rex, blue}
    select(st, rex)
    assert st.cards[rex].damage == 1
    assert st.cards[enemy].damage == 1
    pass_all(st)
    to_next_turn(st)
    assert st.cards[rex].damage == 0
    assert st.cards[enemy].damage == 1


@pytest.mark.card("EB01-004", "EB01-009", "EB01-018")
@pytest.mark.rule("10-2-2", "10-3-3-1", "13-2-13-1")
def test_eb01_004_once_per_turn_is_not_used_when_no_target_could_be_chosen() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    rex = sc.add(0, "EB01-004", damage=3)
    blue = sc.add(0, "EB01-018")
    full_armor = sc.add(0, "EB01-009", Zone.HAND)
    enemy = sc.add(1, VANILLA)
    sc.shields(1, VANILLA, VANILLA)
    st = sc.start()
    attack(st, blue)
    select(st, rex)
    assert st.cards[rex].damage == 2
    pass_all(st)
    play(st, full_armor)
    assert st.cards[enemy].rested
    to_next_turn(st)
    assert st.cards[rex].damage == 0
    assert st.cards[enemy].damage == 1


@pytest.mark.card("EB01-004")
def test_eb01_004_no_trigger_without_recovery() -> None:
    sc = Scenario()
    sc.add(0, "EB01-004")
    enemy = sc.add(1, BLOCKER_LV4, rested=True)
    st = sc.start()
    to_next_turn(st)
    assert st.cards[enemy].damage == 0


# ---------------------------------------------------------------------------------------------
# EB01-005 Zeta Gundam Ⅲ P2 Type


@pytest.mark.card("EB01-005")
def test_eb01_005_sets_a_rested_enemy_unit_active_and_draws() -> None:
    sc = Scenario()
    sc.resources(0, 7)
    zeta = sc.add(0, "EB01-005", Zone.HAND)
    mine = sc.add(0, VANILLA, rested=True)
    enemy = sc.add(1, VANILLA, rested=True)
    st = sc.start()
    before = hand_size(st, 0)
    play(st, zeta)
    assert not st.cards[enemy].rested
    assert st.cards[mine].rested
    assert hand_size(st, 0) == before  # played 1, drew 1


@pytest.mark.card("EB01-005")
def test_eb01_005_draws_even_without_a_rested_enemy_unit() -> None:
    sc = Scenario()
    sc.resources(0, 7)
    zeta = sc.add(0, "EB01-005", Zone.HAND)
    mine = sc.add(0, VANILLA, rested=True)
    st = sc.start()
    before = hand_size(st, 0)
    play(st, zeta)
    assert st.cards[mine].rested
    assert hand_size(st, 0) == before


# ---------------------------------------------------------------------------------------------
# EB01-006 Gundam Astray Gold Frame Amatsu


@pytest.mark.card("EB01-006")
@pytest.mark.rule("13-1-1-1")
def test_eb01_006_grants_repair_1_this_turn() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    amatsu = sc.add(0, "EB01-006", Zone.HAND)
    hurt = sc.add(0, VANILLA, damage=1)
    sc.add(1, VANILLA)
    st = sc.start()
    play(st, amatsu)
    assert selectable(st) == {amatsu, hurt}
    select(st, hurt)
    assert keywords(st, hurt).get("Repair") == 1
    to_next_turn(st)
    assert st.cards[hurt].damage == 0
    assert "Repair" not in keywords(st, hurt)


# ---------------------------------------------------------------------------------------------
# <Development> cards: EB01-008, EB01-010, EB01-025, EB01-027


@pytest.mark.card("EB01-008")
@pytest.mark.rule("13-1-8-1", "13-1-8-2")
def test_eb01_008_development_1_recovers_two_hp() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    delta = sc.add(0, "EB01-008", Zone.HAND)
    hurt = sc.add(0, GG_LV5, damage=3)
    (gg,) = sc.trash(0, GG_LV3)
    st = sc.start()
    play(st, delta)
    assert pending_kind(st) is DecisionKind.YES_NO
    yes(st)
    assert zone_of(st, gg) is Zone.REMOVAL
    select(st, hurt)
    assert st.cards[hurt].damage == 1


@pytest.mark.card("EB01-008")
def test_eb01_008_development_can_be_declined() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    delta = sc.add(0, "EB01-008", Zone.HAND)
    hurt = sc.add(0, GG_LV5, damage=3)
    (gg,) = sc.trash(0, GG_LV3)
    st = sc.start()
    play(st, delta)
    no(st)
    assert zone_of(st, gg) is Zone.TRASH
    assert st.cards[hurt].damage == 3


@pytest.mark.card("EB01-008")
def test_eb01_008_development_needs_g_generation_cards_in_trash() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    delta = sc.add(0, "EB01-008", Zone.HAND)
    hurt = sc.add(0, GG_LV5, damage=3)
    sc.trash(0, VANILLA, COMMAND)
    st = sc.start()
    play(st, delta)
    assert pending_kind(st) is DecisionKind.MAIN
    assert st.cards[hurt].damage == 3


@pytest.mark.card("EB01-010")
@pytest.mark.rule("13-1-8-1")
def test_eb01_010_development_3_deals_two_damage_to_a_rested_enemy() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    barbatos = sc.add(0, "EB01-010", Zone.HAND)
    gg = sc.trash(0, GG_LV3, GG_LV2, GG_LV5, GG_LV3)
    enemy = sc.add(1, BLOCKER_LV4, rested=True)
    active_enemy = sc.add(1, VANILLA)
    st = sc.start()
    play(st, barbatos)
    yes(st)
    select(st, gg[0], gg[1], gg[2])
    assert [zone_of(st, u) for u in gg] == [Zone.REMOVAL] * 3 + [Zone.TRASH]
    assert st.cards[enemy].damage == 2
    assert st.cards[active_enemy].damage == 0


@pytest.mark.card("EB01-010")
@pytest.mark.ruling("EB01-010:Q315")
def test_eb01_010_development_may_be_paid_without_a_rested_enemy() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    barbatos = sc.add(0, "EB01-010", Zone.HAND)
    gg = sc.trash(0, GG_LV3, GG_LV2, GG_LV5)
    enemy = sc.add(1, VANILLA)
    st = sc.start()
    play(st, barbatos)
    assert pending_kind(st) is DecisionKind.YES_NO
    yes(st)
    assert all(zone_of(st, u) is Zone.REMOVAL for u in gg)
    assert st.cards[enemy].damage == 0


@pytest.mark.card("EB01-010")
def test_eb01_010_development_needs_three_g_generation_cards() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    barbatos = sc.add(0, "EB01-010", Zone.HAND)
    gg = sc.trash(0, GG_LV3, GG_LV2)
    enemy = sc.add(1, BLOCKER_LV4, rested=True)
    st = sc.start()
    play(st, barbatos)
    assert pending_kind(st) is DecisionKind.MAIN
    assert all(zone_of(st, u) is Zone.TRASH for u in gg)
    assert st.cards[enemy].damage == 0


@pytest.mark.card("EB01-025")
@pytest.mark.ruling("EB01-025:Q320")
def test_eb01_025_development_2_every_player_places_an_ex_resource() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    tallgeese = sc.add(0, "EB01-025", Zone.HAND)
    gg = sc.trash(0, GG_LV3, GG_LV2)
    st = sc.start()
    play(st, tallgeese)
    yes(st)
    assert all(zone_of(st, u) is Zone.REMOVAL for u in gg)
    assert ex_resources(st, 0) == 1
    assert ex_resources(st, 1) == 1


@pytest.mark.card("EB01-025")
@pytest.mark.rule("13-2-10-1")
def test_eb01_025_paired_is_immune_to_battle_damage_from_low_level_enemies() -> None:
    sc = Scenario(active=1)
    tallgeese = sc.add(0, "EB01-025", rested=True, pilot=OTHER_PILOT)
    sc.resources(1, 0, ex=1)
    attacker = sc.add(1, VANILLA)
    st = sc.start()
    attack(st, attacker, tallgeese)
    pass_all(st)
    assert st.cards[tallgeese].damage == 0
    assert zone_of(st, attacker) is Zone.TRASH


@pytest.mark.card("EB01-025")
def test_eb01_025_immunity_needs_an_enemy_ex_resource() -> None:
    sc = Scenario(active=1)
    tallgeese = sc.add(0, "EB01-025", rested=True, pilot=OTHER_PILOT)
    attacker = sc.add(1, VANILLA)
    st = sc.start()
    attack(st, attacker, tallgeese)
    pass_all(st)
    assert st.cards[tallgeese].damage == 2


@pytest.mark.card("EB01-025")
def test_eb01_025_immunity_needs_a_paired_pilot() -> None:
    sc = Scenario(active=1)
    tallgeese = sc.add(0, "EB01-025", rested=True)
    sc.resources(1, 0, ex=1)
    attacker = sc.add(1, VANILLA)
    st = sc.start()
    attack(st, attacker, tallgeese)
    pass_all(st)
    assert st.cards[tallgeese].damage == 2


@pytest.mark.card("EB01-025")
def test_eb01_025_immunity_does_not_cover_lv6_attackers() -> None:
    sc = Scenario(active=1)
    tallgeese = sc.add(0, "EB01-025", rested=True, pilot=OTHER_PILOT)
    sc.resources(1, 0, ex=1)
    attacker = sc.add(1, BLOCKER_LV6)
    st = sc.start()
    attack(st, attacker, tallgeese)
    pass_all(st)
    assert zone_of(st, tallgeese) is Zone.TRASH


@pytest.mark.card("EB01-027")
@pytest.mark.rule("13-1-8-1", "13-1-2-5")
def test_eb01_027_development_2_grants_breach_1_this_turn() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    tallgeese = sc.add(0, "EB01-027", Zone.HAND)
    gg_unit = sc.add(0, GG_LV3)
    sc.add(0, VANILLA)
    sc.trash(0, GG_LV3, GG_LV2)
    st = sc.start()
    play(st, tallgeese)
    yes(st)
    assert selectable(st) == {tallgeese, gg_unit}
    select(st, gg_unit)
    assert keywords(st, gg_unit).get("Breach") == 1
    to_next_turn(st)
    assert "Breach" not in keywords(st, gg_unit)


# ---------------------------------------------------------------------------------------------
# EB01-009 Gundam Full Armor (Thunderbolt) (EX)


@pytest.mark.card("EB01-009")
@pytest.mark.ruling("EB01-009:Q313")
def test_eb01_009_opponent_chooses_one_of_their_active_units_to_rest() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    full_armor = sc.add(0, "EB01-009", Zone.HAND)
    mine = sc.add(0, VANILLA)
    a1 = sc.add(1, VANILLA)
    a2 = sc.add(1, GG_LV3)
    sc.add(1, VANILLA, rested=True)
    st = sc.start()
    play(st, full_armor)
    assert st.pending is not None and st.pending.player == 1
    assert selectable(st) == {a1, a2}
    select(st, a2)
    assert st.cards[a2].rested
    assert not st.cards[a1].rested
    assert not st.cards[mine].rested


# ---------------------------------------------------------------------------------------------
# EB01-011 Beginning Gundam


@pytest.mark.card("EB01-011")
@pytest.mark.rule("13-1-4-1")
def test_eb01_011_blocker() -> None:
    sc = Scenario(active=1)
    beginning = sc.add(0, "EB01-011")
    attacker = sc.add(1, VANILLA)
    sc.shields(0, VANILLA)
    st = sc.start()
    assert keywords(st, beginning).get("Blocker") == 1
    attack(st, attacker)
    assert pending_kind(st) is DecisionKind.BLOCK
    block(st, beginning)
    pass_all(st)
    assert zone_of(st, beginning) is Zone.TRASH


# ---------------------------------------------------------------------------------------------
# EB01-013 Red Gundam(0085)


@pytest.mark.card("EB01-013")
def test_eb01_013_gets_ap_2_when_the_enemy_has_six_cards_in_hand() -> None:
    sc = Scenario()
    red = sc.add(0, "EB01-013")
    sc.hand(1, *([VANILLA] * 6))
    (top, _) = sc.shields(1, VANILLA, VANILLA)
    st = sc.start()
    assert ap(st, red) == 0
    attack(st, red)
    pass_all(st)
    assert ap(st, red) == 2
    assert zone_of(st, top) is Zone.TRASH


@pytest.mark.card("EB01-013")
@pytest.mark.rule("5-5-5")
def test_eb01_013_no_bonus_with_five_cards_in_hand() -> None:
    sc = Scenario()
    red = sc.add(0, "EB01-013")
    sc.hand(1, *([VANILLA] * 5))
    (top, _) = sc.shields(1, VANILLA, VANILLA)
    st = sc.start()
    attack(st, red)
    pass_all(st)
    assert ap(st, red) == 0
    assert zone_of(st, top) is Zone.SHIELD


# ---------------------------------------------------------------------------------------------
# EB01-014 Gouf Vijayanta


@pytest.mark.card("EB01-014")
@pytest.mark.parametrize("dealer", ["GD01-052", "ST08-002"])
def test_eb01_014_ignores_effect_damage_from_lv5_or_lower_units_on_opponent_turn(
    dealer: str,
) -> None:
    sc = Scenario(active=1)
    gouf = sc.add(0, "EB01-014")
    sc.resources(1, 5)
    unit = sc.add(1, dealer, Zone.HAND)
    st = sc.start()
    play(st, unit)
    assert st.cards[gouf].damage == 0


@pytest.mark.card("EB01-014")
def test_eb01_014_takes_effect_damage_from_higher_level_units() -> None:
    sc = Scenario(active=1)
    gouf = sc.add(0, "EB01-014")
    sc.resources(1, 8)
    unit = sc.add(1, LV8_UNIT, Zone.HAND)
    st = sc.start()
    play(st, unit)
    assert zone_of(st, gouf) is Zone.TRASH


@pytest.mark.card("EB01-014")
def test_eb01_014_takes_effect_damage_from_commands() -> None:
    sc = Scenario(active=1)
    gouf = sc.add(0, "EB01-014")
    sc.resources(1, 2)
    cmd = sc.add(1, COMMAND, Zone.HAND)
    st = sc.start()
    play(st, cmd)
    assert st.cards[gouf].damage == 1


@pytest.mark.card("EB01-014")
def test_eb01_014_takes_battle_damage_on_opponent_turn() -> None:
    sc = Scenario(active=1)
    gouf = sc.add(0, "EB01-014", rested=True)
    attacker = sc.add(1, VANILLA)
    st = sc.start()
    attack(st, attacker, gouf)
    pass_all(st)
    assert st.cards[gouf].damage == 2


@pytest.mark.card("EB01-014")
def test_eb01_014_takes_effect_damage_from_units_during_own_turn() -> None:
    sc = Scenario()
    gouf = sc.add(0, "EB01-014")
    ez8 = sc.add(0, GG_LV3_4AP)
    sleeves = sc.add(1, "GD01-056", rested=True)
    st = sc.start()
    attack(st, ez8, sleeves)
    pass_all(st)
    assert zone_of(st, sleeves) is Zone.TRASH
    assert st.pending is not None and st.pending.player == 1
    select(st, gouf)
    assert st.cards[gouf].damage == 1


# ---------------------------------------------------------------------------------------------
# EB01-015 Prototype Asshimar TR-3 "Kehaar"


@pytest.mark.card("EB01-015")
@pytest.mark.rule("13-2-8-1")
def test_eb01_015_destroyed_deals_one_damage_with_two_other_rested_units() -> None:
    sc = Scenario()
    kehaar = sc.add(0, "EB01-015", damage=3)
    target = sc.add(1, VANILLA, rested=True)
    other = sc.add(1, VANILLA, rested=True)
    st = sc.start()
    attack(st, kehaar, target)
    pass_all(st)
    assert zone_of(st, kehaar) is Zone.TRASH
    assert selectable(st) == {target, other}
    select(st, other)
    assert st.cards[other].damage == 1
    assert st.cards[target].damage == 1


@pytest.mark.card("EB01-015")
@pytest.mark.ruling("EB01-015:Q316")
def test_eb01_015_counts_friendly_and_enemy_rested_units() -> None:
    sc = Scenario()
    kehaar = sc.add(0, "EB01-015", damage=3)
    sc.add(0, VANILLA, rested=True)
    target = sc.add(1, VANILLA, rested=True)
    st = sc.start()
    attack(st, kehaar, target)
    pass_all(st)
    assert zone_of(st, kehaar) is Zone.TRASH
    assert zone_of(st, target) is Zone.TRASH  # 1 battle damage + 1 effect damage


@pytest.mark.card("EB01-015")
def test_eb01_015_no_damage_with_one_other_rested_unit() -> None:
    sc = Scenario()
    kehaar = sc.add(0, "EB01-015", damage=3)
    target = sc.add(1, VANILLA, rested=True)
    st = sc.start()
    attack(st, kehaar, target)
    pass_all(st)
    assert zone_of(st, kehaar) is Zone.TRASH
    assert st.cards[target].damage == 1


# ---------------------------------------------------------------------------------------------
# EB01-017 Haro


@pytest.mark.card("EB01-017")
@pytest.mark.rule("13-2-8-1")
def test_eb01_017_battle_destruction_draws_for_both_players() -> None:
    sc = Scenario(active=1)
    haro = sc.add(0, "EB01-017", rested=True)
    attacker = sc.add(1, GG_LV5)
    st = sc.start()
    before = (hand_size(st, 0), hand_size(st, 1))
    attack(st, attacker, haro)
    pass_all(st)
    assert zone_of(st, haro) is Zone.TRASH
    assert (hand_size(st, 0), hand_size(st, 1)) == (before[0] + 1, before[1] + 1)


@pytest.mark.card("EB01-017")
def test_eb01_017_effect_destruction_draws_nothing() -> None:
    sc = Scenario(active=1)
    haro = sc.add(0, "EB01-017")
    sc.resources(1, 4)
    fatal_strike = sc.add(1, "ST05-014", Zone.HAND)
    st = sc.start()
    before = (hand_size(st, 0), hand_size(st, 1))
    play(st, fatal_strike)
    assert zone_of(st, haro) is Zone.TRASH
    assert (hand_size(st, 0), hand_size(st, 1)) == (before[0], before[1] - 1)


# ---------------------------------------------------------------------------------------------
# EB01-018 Gundam Astray Blue Frame Second L


@pytest.mark.card("EB01-018")
def test_eb01_018_attack_recovers_one_hp_on_a_friendly_unit() -> None:
    sc = Scenario()
    blue = sc.add(0, "EB01-018")
    hurt = sc.add(0, VANILLA, damage=1)
    enemy = sc.add(1, VANILLA, damage=1)
    sc.shields(1, VANILLA)
    st = sc.start()
    attack(st, blue)
    assert selectable(st) == {blue, hurt}
    select(st, hurt)
    assert st.cards[hurt].damage == 0
    assert st.cards[enemy].damage == 1


# ---------------------------------------------------------------------------------------------
# EB01-019 Gundam Pixy


@pytest.mark.card("EB01-019")
@pytest.mark.ruling("EB01-019:Q317")
@pytest.mark.rule("13-1-6-1")
def test_eb01_019_gains_high_maneuver_with_two_other_rested_units() -> None:
    sc = Scenario()
    pixy = sc.add(0, "EB01-019")
    sc.add(0, VANILLA, rested=True)
    sc.add(1, VANILLA, rested=True)
    sc.add(1, BLOCKER_LV4)
    (top,) = sc.shields(1, VANILLA)
    st = sc.start()
    attack(st, pixy)
    assert pending_kind(st) is not DecisionKind.BLOCK
    pass_all(st)
    assert zone_of(st, top) is Zone.TRASH
    assert "High-Maneuver" not in keywords(st, pixy)  # only during this battle


@pytest.mark.card("EB01-019")
def test_eb01_019_can_be_blocked_with_one_other_rested_unit() -> None:
    sc = Scenario()
    pixy = sc.add(0, "EB01-019")
    sc.add(1, VANILLA, rested=True)
    sc.add(1, BLOCKER_LV4)
    sc.shields(1, VANILLA)
    st = sc.start()
    attack(st, pixy)
    assert pending_kind(st) is DecisionKind.BLOCK


# ---------------------------------------------------------------------------------------------
# EB01-020 Gundam Mk-Ⅲ


@pytest.mark.card("EB01-020")
@pytest.mark.ruling("EB01-020:Q318")
@pytest.mark.rule("13-2-2-1", "13-2-12-1")
def test_eb01_020_linked_action_recovers_any_unit() -> None:
    sc = Scenario()
    mk3 = sc.add(0, "EB01-020", pilot=DURABILITY_PILOT)
    attacker = sc.add(0, VANILLA)
    enemy = sc.add(1, VANILLA, damage=1, rested=True)
    sc.shields(1, VANILLA)
    st = sc.start()
    attack(st, attacker)
    assert pending_kind(st) is DecisionKind.ACTION_STEP
    activate(st, mk3)
    assert selectable(st) == {mk3, attacker, enemy}
    select(st, enemy)
    assert st.cards[enemy].damage == 0


@pytest.mark.card("EB01-020")
def test_eb01_020_needs_link() -> None:
    sc = Scenario()
    sc.add(0, "EB01-020", pilot=OTHER_PILOT)
    attacker = sc.add(0, VANILLA)
    enemy = sc.add(1, VANILLA, damage=1, rested=True)
    sc.shields(1, VANILLA)
    st = sc.start()
    attack(st, attacker)
    assert pending_kind(st) is DecisionKind.MAIN  # no action step option was offered
    assert st.cards[enemy].damage == 1


# ---------------------------------------------------------------------------------------------
# EB01-021 Build Strike Gundam (Full Package) (EX)


@pytest.mark.card("EB01-021")
@pytest.mark.rule("13-2-9-2")
def test_eb01_021_gg_pilot_places_a_rested_resource() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    sc.resource_deck(0, 2)
    build = sc.add(0, "EB01-021")
    pilot = sc.add(0, GG_PILOT, Zone.HAND)
    sc.trash(0, GG_LV3, GG_LV2)
    st = sc.start()
    assert keywords(st, build).get("Breach") == 4
    play(st, pilot, onto=build)
    area = st.zones[0][Zone.RESOURCE_AREA]
    assert len(area) == 5
    assert st.cards[area[-1]].rested


@pytest.mark.card("EB01-021")
def test_eb01_021_needs_two_gg_unit_cards_in_trash() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    sc.resource_deck(0, 2)
    build = sc.add(0, "EB01-021")
    pilot = sc.add(0, GG_PILOT, Zone.HAND)
    sc.trash(0, GG_LV3, VANILLA, GG_PILOT)
    st = sc.start()
    play(st, pilot, onto=build)
    assert len(st.zones[0][Zone.RESOURCE_AREA]) == 4


@pytest.mark.card("EB01-021")
def test_eb01_021_needs_a_gg_pilot() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    sc.resource_deck(0, 2)
    build = sc.add(0, "EB01-021")
    pilot = sc.add(0, OTHER_PILOT, Zone.HAND)
    sc.trash(0, GG_LV3, GG_LV2)
    st = sc.start()
    play(st, pilot, onto=build)
    assert len(st.zones[0][Zone.RESOURCE_AREA]) == 4


# ---------------------------------------------------------------------------------------------
# EB01-022 Gundam Exia (EX)


@pytest.mark.card("EB01-022")
@pytest.mark.rule("13-2-10-2", "5-17")
def test_eb01_022_end_of_turn_trade_for_three_tokens() -> None:
    sc = Scenario()
    exia = sc.add(0, "EB01-022", pilot=GG_PILOT)
    st = sc.start()
    assert keywords(st, exia).get("Breach") == 5
    end_main(st)
    pass_all(st)
    assert pending_kind(st) is DecisionKind.YES_NO and st.active == 0
    yes(st)
    assert zone_of(st, exia) is Zone.TRASH
    assert numbers_in(st, 0, Zone.BATTLE) == ["T-025"] * 3
    tokens = st.zones[0][Zone.BATTLE]
    assert [(ap(st, u), hp(st, u)) for u in tokens] == [(2, 2)] * 3


@pytest.mark.card("EB01-022")
def test_eb01_022_may_keep_the_unit() -> None:
    sc = Scenario()
    exia = sc.add(0, "EB01-022", pilot=GG_PILOT)
    st = sc.start()
    end_main(st)
    pass_all(st)
    no(st)
    assert zone_of(st, exia) is Zone.BATTLE
    assert numbers_in(st, 0, Zone.BATTLE) == ["EB01-022"]


@pytest.mark.card("EB01-022")
def test_eb01_022_needs_a_gg_pilot() -> None:
    sc = Scenario()
    exia = sc.add(0, "EB01-022", pilot=OTHER_PILOT)
    st = sc.start()
    to_next_turn(st)
    assert st.active == 1
    assert zone_of(st, exia) is Zone.BATTLE


# ---------------------------------------------------------------------------------------------
# EB01-023 Le Cygne (EX)


@pytest.mark.card("EB01-023")
@pytest.mark.ruling("EB01-023:Q319")
def test_eb01_023_each_player_looks_in_turn_order() -> None:
    sc = Scenario()
    cygne = sc.add(0, "EB01-023")
    sc.shields(1, VANILLA)
    sc.deck(0, GG_LV5)
    sc.deck(1, VANILLA)
    st = sc.start()
    my_top = st.zones[0][Zone.DECK][0]
    their_top = st.zones[1][Zone.DECK][0]
    attack(st, cygne)
    assert pending_kind(st) is DecisionKind.YES_NO and st.pending is not None
    assert st.pending.player == 0
    yes(st)
    assert zone_of(st, my_top) is Zone.HAND
    assert pending_kind(st) is DecisionKind.ARRANGE and st.pending is not None
    assert st.pending.player == 1
    assert st.cards[their_top].known & 0b10 and not st.cards[their_top].known & 0b01
    act(st, A.SELECT, 1)
    assert st.zones[1][Zone.DECK][-1] == their_top


@pytest.mark.card("EB01-023")
def test_eb01_023_low_level_card_is_returned_to_top_or_bottom() -> None:
    sc = Scenario()
    cygne = sc.add(0, "EB01-023")
    sc.shields(1, VANILLA)
    sc.deck(0, GG_LV3)
    sc.deck(1, GG_LV5)
    st = sc.start()
    my_top = st.zones[0][Zone.DECK][0]
    their_top = st.zones[1][Zone.DECK][0]
    attack(st, cygne)
    assert pending_kind(st) is DecisionKind.ARRANGE and st.pending is not None
    assert st.pending.player == 0
    act(st, A.SELECT, 0)
    assert st.zones[0][Zone.DECK][0] == my_top
    assert pending_kind(st) is DecisionKind.YES_NO and st.pending is not None
    assert st.pending.player == 1
    no(st)
    assert pending_kind(st) is DecisionKind.ARRANGE
    act(st, A.SELECT, 0)
    assert st.zones[1][Zone.DECK][0] == their_top


# ---------------------------------------------------------------------------------------------
# EB01-024 GQuuuuuuX (Omega Psycommu)


@pytest.mark.card("EB01-024")
def test_eb01_024_attack_damages_a_low_level_enemy_blocker() -> None:
    sc = Scenario()
    gqx = sc.add(0, "EB01-024")
    blocker = sc.add(1, BLOCKER_LV4)
    plain = sc.add(1, VANILLA)
    sc.shields(1, VANILLA)
    st = sc.start()
    assert keywords(st, gqx).get("Breach") == 3
    attack(st, gqx)
    assert st.cards[blocker].damage == 2
    assert st.cards[plain].damage == 0


@pytest.mark.card("EB01-024")
def test_eb01_024_ignores_blockers_above_lv5() -> None:
    sc = Scenario()
    gqx = sc.add(0, "EB01-024")
    blocker = sc.add(1, BLOCKER_LV6)
    sc.shields(1, VANILLA)
    st = sc.start()
    attack(st, gqx)
    assert st.cards[blocker].damage == 0


# ---------------------------------------------------------------------------------------------
# EB01-028 Gundam Plutone


@pytest.mark.card("EB01-028")
@pytest.mark.rule("13-1-2-1", "13-2-13-1")
def test_eb01_028_rested_plutone_gives_breach_2_to_a_unit_attacking_a_unit() -> None:
    sc = Scenario()
    sc.add(0, "EB01-028", rested=True)
    ez8 = sc.add(0, GG_LV3_4AP)
    second = sc.add(0, GG_LV3_4AP)
    target = sc.add(1, VANILLA, rested=True)
    target2 = sc.add(1, VANILLA, rested=True)
    sc.add(1, BLOCKER_LV1)
    (top, _, _) = sc.shields(1, VANILLA, VANILLA, VANILLA)
    st = sc.start()
    attack(st, ez8, target)
    assert pending_kind(st) is DecisionKind.BLOCK
    assert keywords(st, ez8).get("Breach") == 2
    block(st, None)
    pass_all(st)
    assert zone_of(st, target) is Zone.TRASH
    assert zone_of(st, top) is Zone.TRASH
    assert "Breach" not in keywords(st, ez8)
    attack(st, second, target2)
    assert "Breach" not in keywords(st, second)  # 【Once per Turn】


@pytest.mark.card("EB01-028")
def test_eb01_028_needs_plutone_rested() -> None:
    sc = Scenario()
    sc.add(0, "EB01-028")
    ez8 = sc.add(0, GG_LV3_4AP)
    target = sc.add(1, VANILLA, rested=True)
    sc.add(1, BLOCKER_LV1)
    st = sc.start()
    attack(st, ez8, target)
    assert pending_kind(st) is DecisionKind.BLOCK
    assert "Breach" not in keywords(st, ez8)


@pytest.mark.card("EB01-028")
def test_eb01_028_not_when_attacking_the_player() -> None:
    sc = Scenario()
    sc.add(0, "EB01-028", rested=True)
    ez8 = sc.add(0, GG_LV3_4AP)
    sc.add(1, BLOCKER_LV1)
    sc.shields(1, VANILLA)
    st = sc.start()
    attack(st, ez8, PLAYER_TARGET)
    assert pending_kind(st) is DecisionKind.BLOCK
    assert "Breach" not in keywords(st, ez8)


# ---------------------------------------------------------------------------------------------
# EB01-029 Gundam Astaroth Rinascimento (EX)


@pytest.mark.card("EB01-029")
def test_eb01_029_five_enemy_units_damage_all_low_level_blockers() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    astaroth = sc.add(0, "EB01-029", Zone.HAND)
    my_blocker = sc.add(0, BLOCKER_LV1)
    enemy_blocker = sc.add(1, BLOCKER_LV4)
    big_blocker = sc.add(1, BLOCKER_LV6)
    plain = [sc.add(1, VANILLA) for _ in range(3)]
    st = sc.start()
    play(st, astaroth)
    assert zone_of(st, my_blocker) is Zone.TRASH
    assert st.cards[enemy_blocker].damage == 2
    assert st.cards[big_blocker].damage == 0
    assert all(st.cards[u].damage == 0 for u in plain)


@pytest.mark.card("EB01-029")
def test_eb01_029_nothing_with_four_enemy_units() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    astaroth = sc.add(0, "EB01-029", Zone.HAND)
    enemy_blocker = sc.add(1, BLOCKER_LV4)
    for _ in range(3):
        sc.add(1, VANILLA)
    st = sc.start()
    play(st, astaroth)
    assert st.cards[enemy_blocker].damage == 0


# ---------------------------------------------------------------------------------------------
# EB01-030 Big-Rang, EB01-034 Gundam Lfrith Ur


@pytest.mark.card("EB01-030")
@pytest.mark.ruling("EB01-030:Q480")
def test_eb01_030_deploy_look_top_three_and_take_a_gg_lv3_unit() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    big_rang = sc.add(0, "EB01-030", Zone.HAND)
    sc.deck(0, VANILLA, GG_LV3, GG_LV5)
    st = sc.start()
    looked = list(st.zones[0][Zone.DECK][:3])
    play(st, big_rang)
    assert pending_kind(st) is DecisionKind.YES_NO
    assert all(st.cards[u].known & 0b01 for u in looked)  # Q480: the look is not optional
    yes(st)
    assert zone_of(st, looked[1]) is Zone.HAND
    deck = st.zones[0][Zone.DECK]
    assert set(deck[-2:]) == {looked[0], looked[2]}


@pytest.mark.card("EB01-030")
def test_eb01_030_declining_still_returns_cards_to_the_bottom() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    big_rang = sc.add(0, "EB01-030", Zone.HAND)
    sc.deck(0, VANILLA, GG_LV3, GG_LV5)
    st = sc.start()
    looked = list(st.zones[0][Zone.DECK][:3])
    play(st, big_rang)
    no(st)
    assert set(st.zones[0][Zone.DECK][-3:]) == set(looked)


@pytest.mark.card("EB01-034")
@pytest.mark.ruling("EB01-034:Q481")
@pytest.mark.rule("13-2-11-1")
def test_eb01_034_when_linked_look_top_three() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    ur = sc.add(0, "EB01-034")
    pilot = sc.add(0, GG_PILOT, Zone.HAND)
    sc.deck(0, GG_LV2, GG_LV3, VANILLA)
    st = sc.start()
    looked = list(st.zones[0][Zone.DECK][:3])
    play(st, pilot, onto=ur)
    assert pending_kind(st) is DecisionKind.YES_NO
    assert all(st.cards[u].known & 0b01 for u in looked)
    yes(st)
    assert zone_of(st, looked[1]) is Zone.HAND
    assert set(st.zones[0][Zone.DECK][-2:]) == {looked[0], looked[2]}


@pytest.mark.card("EB01-034")
def test_eb01_034_no_effect_without_link() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    ur = sc.add(0, "EB01-034")
    pilot = sc.add(0, OTHER_PILOT, Zone.HAND)
    sc.deck(0, GG_LV3)
    st = sc.start()
    top = st.zones[0][Zone.DECK][0]
    play(st, pilot, onto=ur)
    assert pending_kind(st) is DecisionKind.MAIN
    assert st.zones[0][Zone.DECK][0] == top


# ---------------------------------------------------------------------------------------------
# EB01-031 Oggo


@pytest.mark.card("EB01-031")
def test_eb01_031_may_attack_active_enemy_units_of_lv3_or_lower() -> None:
    sc = Scenario()
    oggo = sc.add(0, "EB01-031")
    low = sc.add(1, GG_LV3)
    high = sc.add(1, BLOCKER_LV4)
    st = sc.start()
    assert has_action(st, A.ATTACK, oggo, low)
    assert not has_action(st, A.ATTACK, oggo, high)


# ---------------------------------------------------------------------------------------------
# EB01-033 Taurus (Sanc Kingdom)


@pytest.mark.card("EB01-033")
@pytest.mark.rule("13-2-2-1", "10-1-7-3")
def test_eb01_033_pay_one_to_boost_the_unit_being_attacked() -> None:
    sc = Scenario(active=1)
    sc.resources(0, 1)
    taurus = sc.add(0, "EB01-033")
    defender = sc.add(0, VANILLA, rested=True)
    attacker = sc.add(1, GG_LV3)
    st = sc.start()
    assert not has_action(st, A.ACTIVATE, taurus)
    attack(st, attacker, defender)
    assert pending_kind(st) is DecisionKind.ACTION_STEP and st.pending is not None
    assert st.pending.player == 0
    activate(st, taurus)
    pass_all(st)
    assert st.cards[st.zones[0][Zone.RESOURCE_AREA][0]].rested
    assert zone_of(st, attacker) is Zone.TRASH  # the 2-AP defender dealt 3 damage
    assert ap(st, taurus) == 2


@pytest.mark.card("EB01-033")
def test_eb01_033_without_activation_the_defender_deals_its_printed_ap() -> None:
    sc = Scenario(active=1)
    sc.resources(0, 1)
    sc.add(0, "EB01-033")
    defender = sc.add(0, VANILLA, rested=True)
    attacker = sc.add(1, GG_LV3)
    st = sc.start()
    attack(st, attacker, defender)
    pass_all(st)
    assert st.cards[attacker].damage == 2


@pytest.mark.card("EB01-033")
def test_eb01_033_cannot_target_itself() -> None:
    sc = Scenario(active=1)
    sc.resources(0, 1)
    taurus = sc.add(0, "EB01-033", rested=True)
    attacker = sc.add(1, GG_LV3)
    st = sc.start()
    attack(st, attacker, taurus)
    assert pending_kind(st) is DecisionKind.MAIN  # no action step option was offered
    assert zone_of(st, taurus) is Zone.TRASH
    assert st.cards[attacker].damage == 2


# ---------------------------------------------------------------------------------------------
# EB01-035 Gundam Lfrith Thorn


@pytest.mark.card("EB01-035")
def test_eb01_035_gains_breach_when_another_gg_lv3_unit_is_deployed() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    thorn = sc.add(0, "EB01-035")
    gg3 = sc.add(0, GG_LV3, Zone.HAND)
    st = sc.start()
    play(st, gg3)
    assert keywords(st, thorn).get("Breach") == 1
    to_next_turn(st)
    assert "Breach" not in keywords(st, thorn)


@pytest.mark.card("EB01-035")
def test_eb01_035_ignores_other_levels_and_itself() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    thorn = sc.add(0, "EB01-035")
    gg2 = sc.add(0, GG_LV2, Zone.HAND)
    second_thorn = sc.add(0, "EB01-035", Zone.HAND)
    st = sc.start()
    play(st, gg2)
    assert "Breach" not in keywords(st, thorn)
    play(st, second_thorn)
    assert keywords(st, thorn).get("Breach") == 1
    assert "Breach" not in keywords(st, second_thorn)


# ---------------------------------------------------------------------------------------------
# EB01-036 Darilbalde


@pytest.mark.card("EB01-036")
def test_eb01_036_other_gg_lv3_units_get_ap_1_during_your_turn() -> None:
    sc = Scenario()
    darilbalde = sc.add(0, "EB01-036")
    mine = sc.add(0, GG_LV3)
    theirs = sc.add(1, GG_LV3)
    lv2 = sc.add(0, GG_LV2)
    st = sc.start()
    assert ap(st, mine) == 4
    assert ap(st, theirs) == 4
    assert ap(st, lv2) == 2
    assert ap(st, darilbalde) == 4
    to_next_turn(st)
    assert ap(st, mine) == 3


# ---------------------------------------------------------------------------------------------
# EB01-037 Zudah Unit 1


@pytest.mark.card("EB01-037")
@pytest.mark.rule("8-5-3-2")
def test_eb01_037_no_battle_damage_while_battling_an_enemy_blocker() -> None:
    sc = Scenario()
    zudah = sc.add(0, "EB01-037")
    target = sc.add(1, BLOCKER_LV4, rested=True)
    st = sc.start()
    attack(st, zudah, target)
    pass_all(st)
    assert zone_of(st, target) is Zone.TRASH
    assert zone_of(st, zudah) is Zone.BATTLE
    assert st.cards[zudah].damage == 0


@pytest.mark.card("EB01-037")
def test_eb01_037_no_battle_damage_when_blocked() -> None:
    sc = Scenario()
    zudah = sc.add(0, "EB01-037")
    blocker = sc.add(1, BLOCKER_LV1)
    sc.shields(1, VANILLA)
    st = sc.start()
    attack(st, zudah)
    block(st, blocker)
    pass_all(st)
    assert zone_of(st, blocker) is Zone.TRASH
    assert st.cards[zudah].damage == 0


@pytest.mark.card("EB01-037")
def test_eb01_037_takes_damage_from_units_without_blocker() -> None:
    sc = Scenario()
    zudah = sc.add(0, "EB01-037")
    target = sc.add(1, VANILLA, rested=True)
    st = sc.start()
    attack(st, zudah, target)
    pass_all(st)
    assert zone_of(st, zudah) is Zone.TRASH


@pytest.mark.card("EB01-037")
def test_eb01_037_takes_damage_during_opponent_turn() -> None:
    sc = Scenario(active=1)
    zudah = sc.add(0, "EB01-037", rested=True)
    attacker = sc.add(1, BLOCKER_LV1)
    st = sc.start()
    attack(st, attacker, zudah)
    pass_all(st)
    assert zone_of(st, zudah) is Zone.TRASH


# ---------------------------------------------------------------------------------------------
# EB01-038 G-Self


@pytest.mark.card("EB01-038")
@pytest.mark.rule("5-17-3-2-1")
def test_eb01_038_deploy_places_an_ex_resource() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    gself = sc.add(0, "EB01-038", Zone.HAND)
    st = sc.start()
    play(st, gself)
    assert ex_resources(st, 0) == 1
    assert ex_resources(st, 1) == 0


# ---------------------------------------------------------------------------------------------
# EB01-039 Rising Freedom Gundam


@pytest.mark.card("EB01-039")
@pytest.mark.rule("2-9-1")
def test_eb01_039_plays_as_lv3_cost3_with_three_enemy_units() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    rising = sc.add(0, "EB01-039", Zone.HAND)
    for _ in range(3):
        sc.add(1, VANILLA)
    st = sc.start()
    act(st, A.PLAY_UNIT, rising, None, ALT_PLAY_BASE)
    assert zone_of(st, rising) is Zone.BATTLE
    assert all(st.cards[u].rested for u in st.zones[0][Zone.RESOURCE_AREA])


@pytest.mark.card("EB01-039")
def test_eb01_039_modified_play_costs_three() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    rising = sc.add(0, "EB01-039", Zone.HAND)
    for _ in range(3):
        sc.add(1, VANILLA)
    st = sc.start()
    act(st, A.PLAY_UNIT, rising, None, ALT_PLAY_BASE)
    assert zone_of(st, rising) is Zone.BATTLE
    rested = [st.cards[u].rested for u in st.zones[0][Zone.RESOURCE_AREA]]
    assert rested.count(True) == 3


@pytest.mark.card("EB01-039")
def test_eb01_039_needs_three_enemy_units() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    rising = sc.add(0, "EB01-039", Zone.HAND)
    for _ in range(2):
        sc.add(1, VANILLA)
    st = sc.start()
    assert not has_action(st, A.PLAY_UNIT, rising)


# ---------------------------------------------------------------------------------------------
# EB01-040 Gundam Epyon, EB01-044 Justice Gundam (EX): "2 or more enemy players" (1v1: never)


@pytest.mark.card("EB01-040")
def test_eb01_040_deploy_does_nothing_with_one_enemy_player() -> None:
    sc = Scenario()
    sc.resources(0, 8)
    epyon = sc.add(0, "EB01-040", Zone.HAND)
    mine = sc.add(0, GG_LV3)
    st = sc.start()
    play(st, epyon)
    assert pending_kind(st) is DecisionKind.MAIN
    assert "Breach" not in keywords(st, mine)
    assert "Breach" not in keywords(st, epyon)


@pytest.mark.card("EB01-044")
def test_eb01_044_blocker_and_deploy_does_nothing_with_one_enemy_player() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    justice = sc.add(0, "EB01-044", Zone.HAND)
    enemy = sc.add(1, VANILLA)
    st = sc.start()
    play(st, justice)
    assert pending_kind(st) is DecisionKind.MAIN
    assert zone_of(st, enemy) is Zone.BATTLE
    assert keywords(st, justice).get("Blocker") == 1


# ---------------------------------------------------------------------------------------------
# EB01-041 Strike Freedom Gundam (EX)


@pytest.mark.card("EB01-041")
def test_eb01_041_returns_an_enemy_unit_with_four_or_less_hp() -> None:
    sc = Scenario()
    sc.resources(0, 7)
    freedom = sc.add(0, "EB01-041", Zone.HAND)
    mine = sc.add(0, VANILLA)
    small = sc.add(1, VANILLA)
    four_hp = sc.add(1, GG_LV5)
    sc.add(1, LV8_UNIT)
    st = sc.start()
    play(st, freedom)
    assert selectable(st) == {small, four_hp}
    select(st, four_hp)
    assert zone_of(st, four_hp) is Zone.HAND and st.cards[four_hp].owner == 1
    assert zone_of(st, mine) is Zone.BATTLE
    assert keywords(st, freedom).get("High-Maneuver") == 1


# ---------------------------------------------------------------------------------------------
# EB01-042 Psycho Haro (EX)


@pytest.mark.card("EB01-042")
@pytest.mark.rule("13-1-4-1")
def test_eb01_042_rested_grants_blocker_to_all_units() -> None:
    sc = Scenario()
    haro = sc.add(0, "EB01-042", rested=True)
    mine = sc.add(0, VANILLA)
    theirs = sc.add(1, VANILLA)
    st = sc.start()
    assert keywords(st, mine).get("Blocker") == 1
    assert keywords(st, theirs).get("Blocker") == 1
    assert keywords(st, haro).get("Blocker") == 1


@pytest.mark.card("EB01-042")
def test_eb01_042_active_grants_nothing() -> None:
    sc = Scenario()
    haro = sc.add(0, "EB01-042")
    theirs = sc.add(1, VANILLA)
    st = sc.start()
    assert "Blocker" not in keywords(st, theirs)
    assert "Blocker" not in keywords(st, haro)


@pytest.mark.card("EB01-042")
def test_eb01_042_rested_enemy_units_can_block_on_their_turn() -> None:
    sc = Scenario(active=1)
    sc.add(0, "EB01-042", rested=True)
    mine = sc.add(0, VANILLA)
    attacker = sc.add(1, GG_LV3)
    sc.shields(0, VANILLA)
    st = sc.start()
    attack(st, attacker)
    assert pending_kind(st) is DecisionKind.BLOCK
    assert {o.a for o in options(st) if o.kind is A.BLOCK} == {mine}


@pytest.mark.card("EB01-042")
def test_eb01_042_attack_stops_lv7_or_lower_blockers() -> None:
    sc = Scenario()
    haro = sc.add(0, "EB01-042")
    sc.add(1, VANILLA)
    big = sc.add(1, LV8_UNIT)
    sc.shields(1, VANILLA)
    st = sc.start()
    attack(st, haro)
    assert pending_kind(st) is DecisionKind.BLOCK
    assert {o.a for o in options(st) if o.kind is A.BLOCK} == {big}


# ---------------------------------------------------------------------------------------------
# EB01-043 Blue Destiny Unit-1 (EX)


@pytest.mark.card("EB01-043")
def test_eb01_043_with_a_friendly_blocker_gives_ap_minus_2_for_the_battle() -> None:
    sc = Scenario()
    blue = sc.add(0, "EB01-043")
    sc.add(0, BLOCKER_LV1)
    enemy = sc.add(1, BLOCKER_LV4)
    sc.add(1, BLOCKER_LV6)
    sc.shields(1, VANILLA)
    st = sc.start()
    attack(st, blue)
    assert pending_kind(st) is DecisionKind.BLOCK
    assert ap(st, enemy) == 1
    block(st, None)
    pass_all(st)
    assert ap(st, enemy) == 3


@pytest.mark.card("EB01-043")
def test_eb01_043_needs_a_friendly_blocker() -> None:
    sc = Scenario()
    blue = sc.add(0, "EB01-043")
    enemy = sc.add(1, BLOCKER_LV4)
    sc.shields(1, VANILLA)
    st = sc.start()
    attack(st, blue)
    assert pending_kind(st) is DecisionKind.BLOCK
    assert ap(st, enemy) == 3


# ---------------------------------------------------------------------------------------------
# EB01-045 Psycho Zaku (EX)


@pytest.mark.card("EB01-045")
@pytest.mark.rule("13-2-9-1")
def test_eb01_045_when_paired_returns_an_enemy_unit_with_repair() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    zaku = sc.add(0, "EB01-045")
    pilot = sc.add(0, OTHER_PILOT, Zone.HAND)
    repair_unit = sc.add(1, "EB01-004")
    plain = sc.add(1, VANILLA)
    st = sc.start()
    assert keywords(st, zaku).get("Suppression") == 1
    play(st, pilot, onto=zaku)
    assert zone_of(st, repair_unit) is Zone.HAND
    assert zone_of(st, plain) is Zone.BATTLE


# ---------------------------------------------------------------------------------------------
# EB01-046 Striker Custom (EX)


@pytest.mark.card("EB01-046")
@pytest.mark.rule("13-2-10-1")
def test_eb01_046_paired_attack_gives_lv4_or_higher_enemy_ap_minus_2() -> None:
    sc = Scenario()
    striker = sc.add(0, "EB01-046", pilot=OTHER_PILOT)
    enemy = sc.add(1, BLOCKER_LV4)
    low = sc.add(1, VANILLA)
    sc.shields(1, VANILLA)
    st = sc.start()
    attack(st, striker)
    assert pending_kind(st) is DecisionKind.BLOCK
    assert ap(st, enemy) == 1
    assert ap(st, low) == 2


@pytest.mark.card("EB01-046")
def test_eb01_046_needs_a_paired_pilot() -> None:
    sc = Scenario()
    striker = sc.add(0, "EB01-046")
    enemy = sc.add(1, BLOCKER_LV4)
    sc.shields(1, VANILLA)
    st = sc.start()
    attack(st, striker)
    assert pending_kind(st) is DecisionKind.BLOCK
    assert ap(st, enemy) == 3
