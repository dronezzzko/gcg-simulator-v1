"""Behaviour tests for GD04-001..GD04-074 (work package WP-GD04-A)."""

from __future__ import annotations

import pytest

from gcg_sim.cards.model import CardType
from gcg_sim.effects import dsl as d
from gcg_sim.engine import view as V
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
    numbers_in,
    pass_all,
    play,
    select,
    to_next_turn,
    yes,
    zone_of,
)

A = ActionKind

VANILLA = "GD01-060"  # Zaku Mariner, red Lv2, 2/2, (Zeon)
BIGRO = "GD04-027"  # green Lv5, 5/4, (Zeon)
BLOCKER = "GD01-072"  # Launcher Strike Gundam, white Lv4, 3/4, <Blocker>
GM3 = "GD04-005"  # blue Lv2, 3/2, (Earth Federation)
SHOKEW = "GD04-014"  # blue Lv2, 2/3, (League Militaire)
HEINDREE = "GD04-031"  # green Lv2, 3/2, (Academy)
VIRTUE = "GD04-047"  # red Lv3, 3/1, (CB), "Gundam Virtue"
EXIA = "GD04-064"  # purple Lv2, 2/3, (CB)
SCHUZRUM = "GD04-040"  # red Lv4, 4/3, (Neo Zeon)
MESSER = "ST08-003"  # red Lv4, 4/3, (Mafty)
PARTS = "T-021"  # [Parts]((League Militaire)･AP1･HP1･can't choose the enemy player)

AMURO = "ST01-010"  # blue Pilot, Amuro Ray
AMURO_GREEN = "GD05-085"
RIDDHE = "GD01-089"  # blue Pilot, (Earth Federation)
FOUR = "GD02-085"  # blue Pilot, (Cyber-Newtype)
CHALLIA = "GD02-090"  # green Pilot, (Newtype)
HALLELUJAH = "GD04-090"  # red Pilot, (CB)(Super Soldier)
DEUX = "GD04-091"  # red Pilot, (Cyber-Newtype)
PALA = "GD04-094"  # purple Pilot, (Vulture)
REY = "GD04-093"  # purple Pilot, (Minerva Squad)
LORAN = "GD04-097"  # white Pilot, (Militia)
ALI = "GD04-099"  # white Pilot, (Superpower Bloc)(UN)
FULL_FRONTAL = "ST03-010"  # red Pilot, (Neo Zeon)
TIERIA = "ST07-010"  # purple Pilot, (CB)
KAI = "ST01-013"  # blue Command, 【Pilot】[Kai Shiden]

SIEGE_PLOY = "ST02-014"  # 【Main】/【Action】rest an enemy Unit with 5 or less HP
UNFORESEEN = "ST01-014"  # 【Burst】Activate 【Main】; 【Main】/【Action】enemy Unit AP-3
IMPROVED = "GD03-109"  # 【Main】/【Action】3 damage to an enemy Unit Lv.4 or lower
DARKNESS_FINGER = "GD05-110"  # 【Main】/【Action】2 damage to an enemy Unit
AWAKENED_POWER = "GD02-110"  # 【Main】pay cost to deploy a Lv.5 or lower Unit card from trash
VIOLENCE = "GD04-106"  # (Dawn of Fold) Command, 【Pilot】[Norea Du Noc]


def pilot_of(sc: Scenario, unit: int) -> int:
    return sc.st.cards[unit].pair


def hand_size(st: GameState, p: int = 0) -> int:
    return len(st.zones[p][Zone.HAND])


def ex_resources(st: GameState, p: int = 0) -> int:
    return sum(
        1
        for u in st.zones[p][Zone.RESOURCE_AREA]
        if V.cdef(st, u).card_type is CardType.EX_RESOURCE
    )


def active_resources(st: GameState, p: int = 0) -> int:
    return sum(1 for u in st.zones[p][Zone.RESOURCE_AREA] if not st.cards[u].rested)


def pending(st: GameState) -> DecisionKind | None:
    return st.pending.kind if st.pending else None


def option_uids(st: GameState) -> set[int]:
    assert st.pending is not None
    return {o.a for o in st.pending.options if o.kind is A.SELECT}


def end_turn(st: GameState) -> None:
    """End the main phase and pass until the next decision that is not an action-step pass."""
    end_main(st)
    pass_all(st)


# ---------------------------------------------------------------------------------------------
# GD04-001 Gundam


@pytest.mark.card("GD04-001")
@pytest.mark.ruling("GD04-001:Q262")
@pytest.mark.rule("3-2-6-3", "13-2-12-1")
def test_gd04_001_returning_the_pilot_does_not_end_a_deploy_turn_attack() -> None:
    sc = Scenario()
    gundam = sc.add(0, "GD04-001", pilot=AMURO, deployed_this_turn=True)
    amuro = pilot_of(sc, gundam)
    target = sc.add(1, VANILLA, rested=True)
    st = sc.start()
    attack(st, gundam, target)
    assert pending(st) is DecisionKind.YES_NO
    yes(st)
    assert zone_of(st, amuro) is Zone.HAND
    assert ap(st, gundam) == 6
    pass_all(st)
    assert zone_of(st, target) is Zone.TRASH
    assert st.cards[gundam].damage == 2


@pytest.mark.card("GD04-001")
def test_gd04_001_no_offer_when_attacking_the_player_or_with_a_non_blue_pilot() -> None:
    sc = Scenario()
    blue = sc.add(0, "GD04-001", pilot=AMURO)
    green = sc.add(0, "GD04-001", pilot=AMURO_GREEN)
    target = sc.add(1, VANILLA, rested=True)
    sc.shields(1, VANILLA, VANILLA)
    st = sc.start()
    attack(st, blue, PLAYER_TARGET)
    assert pending(st) is not DecisionKind.YES_NO
    pass_all(st)
    attack(st, green, target)
    assert pending(st) is not DecisionKind.YES_NO
    assert st.cards[green].pair >= 0


@pytest.mark.card("GD04-001")
def test_gd04_001_declining_keeps_the_pilot() -> None:
    sc = Scenario()
    gundam = sc.add(0, "GD04-001", pilot=AMURO)
    amuro = pilot_of(sc, gundam)
    target = sc.add(1, VANILLA, rested=True)
    st = sc.start()
    attack(st, gundam, target)
    no(st)
    assert zone_of(st, amuro) is Zone.PAIRED
    assert ap(st, gundam) == 8


# ---------------------------------------------------------------------------------------------
# GD04-002 Penelope (Flight Form)


@pytest.mark.card("GD04-002")
def test_gd04_002_earth_federation_units_get_ap_during_your_turn_only() -> None:
    sc = Scenario()
    penelope = sc.add(0, "GD04-002")
    gm = sc.add(0, GM3)
    zaku = sc.add(0, VANILLA)
    enemy_gm = sc.add(1, GM3)
    st = sc.start()
    assert (ap(st, penelope), ap(st, gm), ap(st, zaku), ap(st, enemy_gm)) == (4, 4, 2, 3)
    to_next_turn(st)
    assert (ap(st, penelope), ap(st, gm)) == (3, 3)


@pytest.mark.card("GD04-002")
def test_gd04_002_deploy_rests_an_enemy_when_an_earth_federation_unit_destroys() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    penelope = sc.hand(0, "GD04-002")[0]
    gm = sc.add(0, GM3)
    victim = sc.add(1, VANILLA, rested=True)
    big = sc.add(1, BIGRO)
    st = sc.start()
    play(st, penelope)
    attack(st, gm, victim)
    pass_all(st)
    assert zone_of(st, victim) is Zone.TRASH
    assert st.cards[big].rested


@pytest.mark.card("GD04-002")
def test_gd04_002_no_rest_without_the_deploy_effect_this_turn() -> None:
    sc = Scenario()
    sc.add(0, "GD04-002")
    gm = sc.add(0, GM3)
    victim = sc.add(1, VANILLA, rested=True)
    big = sc.add(1, BIGRO)
    st = sc.start()
    attack(st, gm, victim)
    pass_all(st)
    assert zone_of(st, victim) is Zone.TRASH
    assert not st.cards[big].rested


@pytest.mark.card("GD04-002")
@pytest.mark.rule("10-1-6-1-1")
def test_gd04_002_delayed_effect_fires_for_every_destruction_this_turn() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    penelope = sc.hand(0, "GD04-002")[0]
    gm1 = sc.add(0, GM3)
    gm2 = sc.add(0, GM3)
    victim1 = sc.add(1, VANILLA, rested=True)
    victim2 = sc.add(1, VANILLA, rested=True)
    big1 = sc.add(1, BIGRO)
    big2 = sc.add(1, BIGRO)
    st = sc.start()
    play(st, penelope)
    attack(st, gm1, victim1)
    pass_all(st)
    select(st, big1)
    attack(st, gm2, victim2)
    pass_all(st)
    select(st, big2)
    assert st.cards[big1].rested
    assert st.cards[big2].rested


# ---------------------------------------------------------------------------------------------
# GD04-003 Victory Gundam


@pytest.mark.card("GD04-003")
def test_gd04_003_draws_with_three_league_militaire_units() -> None:
    sc = Scenario()
    victory = sc.add(0, "GD04-003")
    sc.add(0, SHOKEW)
    sc.add(0, SHOKEW)
    sc.shields(1, VANILLA)
    st = sc.start()
    before = hand_size(st)
    attack(st, victory)
    assert hand_size(st) == before + 1


@pytest.mark.card("GD04-003")
def test_gd04_003_no_draw_with_two_league_militaire_units() -> None:
    sc = Scenario()
    victory = sc.add(0, "GD04-003")
    sc.add(0, SHOKEW)
    sc.add(0, VANILLA)
    sc.shields(1, VANILLA)
    st = sc.start()
    before = hand_size(st)
    attack(st, victory)
    assert hand_size(st) == before


# ---------------------------------------------------------------------------------------------
# GD04-004 Psycho Gundam Mk-Ⅱ


@pytest.mark.card("GD04-004")
def test_gd04_004_repair_and_draw_when_pairing_cyber_newtype_with_blue_unit() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    psycho = sc.add(0, "GD04-004", damage=3)
    gm = sc.add(0, GM3)
    four = sc.hand(0, FOUR)[0]
    st = sc.start()
    assert keywords(st, psycho) == {"Repair": 2}
    before = hand_size(st)
    play(st, four, onto=gm)
    assert hand_size(st) == before
    to_next_turn(st)
    assert st.cards[psycho].damage == 1


@pytest.mark.card("GD04-004")
@pytest.mark.ruling("GD04-004:Q263")
def test_gd04_004_draws_when_the_pilot_is_paired_with_itself() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    psycho = sc.add(0, "GD04-004")
    four = sc.hand(0, FOUR)[0]
    st = sc.start()
    before = hand_size(st)
    play(st, four, onto=psycho)
    assert hand_size(st) == before


@pytest.mark.card("GD04-004")
def test_gd04_004_no_draw_for_other_pilots_and_once_per_turn() -> None:
    sc = Scenario()
    sc.resources(0, 10)
    sc.add(0, "GD04-004")
    gm1 = sc.add(0, GM3)
    gm2 = sc.add(0, GM3)
    zaku = sc.add(0, VANILLA)
    riddhe = sc.hand(0, RIDDHE)[0]
    four1, four2 = sc.hand(0, FOUR, FOUR)
    st = sc.start()
    before = hand_size(st)
    play(st, riddhe, onto=gm1)
    assert hand_size(st) == before - 1
    play(st, four1, onto=zaku)
    assert hand_size(st) == before - 2
    play(st, four2, onto=gm2)
    assert hand_size(st) == before - 2


# ---------------------------------------------------------------------------------------------
# GD04-006 V-Dash Gundam


@pytest.mark.card("GD04-006")
def test_gd04_006_rest_another_league_militaire_unit_to_rest_an_enemy() -> None:
    sc = Scenario()
    vdash = sc.add(0, "GD04-006")
    shokew = sc.add(0, SHOKEW)
    enemy = sc.add(1, VANILLA)
    tough = sc.add(1, "GD04-049")
    st = sc.start()
    assert keywords(st, vdash) == {"Breach": 3}
    activate(st, vdash)
    assert st.cards[shokew].rested
    assert not st.cards[vdash].rested
    assert st.cards[enemy].rested
    assert not st.cards[tough].rested
    assert not has_action(st, A.ACTIVATE, vdash)


@pytest.mark.card("GD04-006")
def test_gd04_006_cannot_activate_without_another_active_league_militaire_unit() -> None:
    sc = Scenario()
    vdash = sc.add(0, "GD04-006")
    sc.add(0, SHOKEW, rested=True)
    sc.add(0, VANILLA)
    sc.add(1, VANILLA)
    st = sc.start()
    assert not has_action(st, A.ACTIVATE, vdash)


# ---------------------------------------------------------------------------------------------
# GD04-007 Victory Gundam Hexa and the [Parts] token


@pytest.mark.card("GD04-007")
def test_gd04_007_paired_attack_deploys_a_parts_token() -> None:
    sc = Scenario()
    hexa = sc.add(0, "GD04-007", pilot=RIDDHE)
    sc.shields(1, VANILLA)
    st = sc.start()
    attack(st, hexa)
    assert numbers_in(st, 0, Zone.BATTLE).count(PARTS) == 1


@pytest.mark.card("GD04-007")
def test_gd04_007_unpaired_attack_deploys_nothing() -> None:
    sc = Scenario()
    hexa = sc.add(0, "GD04-007")
    sc.shields(1, VANILLA)
    st = sc.start()
    attack(st, hexa)
    assert numbers_in(st, 0, Zone.BATTLE) == ["GD04-007"]


@pytest.mark.card("GD04-007", "GD04-011")
def test_parts_token_cannot_choose_the_enemy_player() -> None:
    sc = Scenario()
    parts = sc.add(0, PARTS)
    target = sc.add(1, VANILLA, rested=True)
    st = sc.start()
    assert not has_action(st, A.ATTACK, parts, PLAYER_TARGET)
    assert has_action(st, A.ATTACK, parts, target)


# ---------------------------------------------------------------------------------------------
# GD04-008 Gundam


@pytest.mark.card("GD04-008")
@pytest.mark.rule("13-1-6-1")
def test_gd04_008_linked_gains_high_maneuver_and_cannot_be_blocked() -> None:
    sc = Scenario()
    gundam = sc.add(0, "GD04-008", pilot=AMURO)
    sc.add(1, BLOCKER)
    sc.shields(1, VANILLA)
    st = sc.start()
    assert "High-Maneuver" in keywords(st, gundam)
    attack(st, gundam)
    assert pending(st) is not DecisionKind.BLOCK


@pytest.mark.card("GD04-008")
def test_gd04_008_unlinked_can_be_blocked() -> None:
    sc = Scenario()
    gundam = sc.add(0, "GD04-008", pilot=RIDDHE)
    sc.add(1, BLOCKER)
    sc.shields(1, VANILLA)
    st = sc.start()
    assert "High-Maneuver" not in keywords(st, gundam)
    attack(st, gundam)
    assert pending(st) is DecisionKind.BLOCK


# ---------------------------------------------------------------------------------------------
# GD04-009 Guncannon (108) & Guncannon (109)


@pytest.mark.card("GD04-009")
@pytest.mark.rule("13-2-11-1")
def test_gd04_009_when_linked_sets_a_lv4_white_base_team_unit_active() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    guncannon = sc.add(0, "GD04-009")
    gundam = sc.add(0, "GD04-008", rested=True)
    booster = sc.add(0, "GD04-012", rested=True)
    kai = sc.hand(0, KAI)[0]
    st = sc.start()
    play(st, kai, onto=guncannon)
    assert not st.cards[gundam].rested
    assert st.cards[booster].rested


@pytest.mark.card("GD04-009")
def test_gd04_009_no_trigger_when_paired_without_link() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    guncannon = sc.add(0, "GD04-009")
    gundam = sc.add(0, "GD04-008", rested=True)
    riddhe = sc.hand(0, RIDDHE)[0]
    st = sc.start()
    play(st, riddhe, onto=guncannon)
    assert st.cards[gundam].rested


# ---------------------------------------------------------------------------------------------
# GD04-011 Victory Gundam


@pytest.mark.card("GD04-011")
def test_gd04_011_destroyed_with_another_league_militaire_unit_deploys_parts() -> None:
    sc = Scenario()
    victory = sc.add(0, "GD04-011")
    sc.add(0, SHOKEW)
    big = sc.add(1, BIGRO, rested=True)
    st = sc.start()
    attack(st, victory, big)
    pass_all(st)
    assert zone_of(st, victory) is Zone.TRASH
    assert numbers_in(st, 0, Zone.BATTLE) == [SHOKEW, PARTS]


@pytest.mark.card("GD04-011")
def test_gd04_011_destroyed_alone_deploys_nothing() -> None:
    sc = Scenario()
    victory = sc.add(0, "GD04-011")
    sc.add(0, VANILLA)
    big = sc.add(1, BIGRO, rested=True)
    st = sc.start()
    attack(st, victory, big)
    pass_all(st)
    assert numbers_in(st, 0, Zone.BATTLE) == [VANILLA]


# ---------------------------------------------------------------------------------------------
# GD04-013 Core Fighter


@pytest.mark.card("GD04-013")
def test_gd04_013_rested_core_fighter_gives_league_militaire_tokens_blocker() -> None:
    sc = Scenario()
    fighter = sc.add(0, "GD04-013")
    parts = sc.add(0, PARTS)
    enemy_parts = sc.add(1, PARTS)
    sc.shields(1, VANILLA, VANILLA)
    st = sc.start()
    assert "Blocker" not in keywords(st, parts)
    attack(st, fighter)
    assert st.cards[fighter].rested
    assert "Blocker" in keywords(st, parts)
    assert "Blocker" not in keywords(st, fighter)
    assert "Blocker" not in keywords(st, enemy_parts)


@pytest.mark.card("GD04-013")
@pytest.mark.ruling("GD04-013:Q264")
@pytest.mark.rule("13-1-4-1")
def test_gd04_013_block_stands_after_the_token_loses_blocker() -> None:
    sc = Scenario(active=1)
    sc.resources(1, 3)
    attacker = sc.add(1, VANILLA)
    fighter = sc.add(0, "GD04-013", rested=True)
    parts = sc.add(0, PARTS)
    shield = sc.shields(0, VANILLA)[0]
    technique = sc.hand(1, IMPROVED)[0]
    st = sc.start()
    attack(st, attacker)
    block(st, parts)
    assert st.pending is not None and st.pending.player == 1
    play(st, technique)
    select(st, fighter)
    assert zone_of(st, fighter) is Zone.TRASH
    assert "Blocker" not in keywords(st, parts)
    pass_all(st)
    assert zone_of(st, parts) is Zone.OUTSIDE
    assert zone_of(st, shield) is Zone.SHIELD


# ---------------------------------------------------------------------------------------------
# GD04-015 Gun EZ


@pytest.mark.card("GD04-015")
def test_gd04_015_rests_one_of_your_active_units_and_one_enemy() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    gun_ez = sc.hand(0, "GD04-015")[0]
    shokew = sc.add(0, SHOKEW)
    enemy = sc.add(1, VANILLA)
    high = sc.add(1, BIGRO)
    st = sc.start()
    play(st, gun_ez)
    assert option_uids(st) == {gun_ez, shokew}
    select(st, shokew)
    assert st.cards[shokew].rested
    assert st.cards[enemy].rested
    assert not st.cards[gun_ez].rested
    assert not st.cards[high].rested


@pytest.mark.card("GD04-015")
@pytest.mark.ruling("GD04-015:Q265")
def test_gd04_015_needs_both_targets() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    gun_ez = sc.hand(0, "GD04-015")[0]
    shokew = sc.add(0, SHOKEW)
    sc.add(1, BIGRO)
    st = sc.start()
    play(st, gun_ez)
    assert pending(st) is DecisionKind.MAIN
    assert not st.cards[shokew].rested
    assert not st.cards[gun_ez].rested


# ---------------------------------------------------------------------------------------------
# GD04-016 Zoloat (League Militaire)


@pytest.mark.card("GD04-016")
@pytest.mark.ruling("GD04-016:Q266")
def test_gd04_016_cannot_attack_the_player_or_base_but_can_attack_units() -> None:
    sc = Scenario()
    zoloat = sc.add(0, "GD04-016")
    target = sc.add(1, VANILLA, rested=True)
    sc.base(1)
    sc.shields(1, VANILLA)
    st = sc.start()
    assert keywords(st, zoloat) == {"Blocker": 1}
    assert not has_action(st, A.ATTACK, zoloat, PLAYER_TARGET)
    assert has_action(st, A.ATTACK, zoloat, target)


# ---------------------------------------------------------------------------------------------
# GD04-017 Zeong


@pytest.mark.card("GD04-017")
@pytest.mark.rule("13-2-9-2", "5-17-2-2")
def test_gd04_017_newtype_pilot_deploys_two_wire_guided_arms() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    zeong = sc.add(0, "GD04-017")
    challia = sc.hand(0, CHALLIA)[0]
    riddhe = sc.hand(0, RIDDHE)[0]
    st = sc.start()
    play(st, challia, onto=zeong)
    arms = [u for u in st.zones[0][Zone.BATTLE] if V.cdef(st, u).name == "Wire-Guided Arm"]
    assert len(arms) == 2
    assert all((ap(st, u), hp(st, u)) == (2, 1) for u in arms)
    assert not any(has_action(st, A.PAIR, riddhe, u) for u in arms)


@pytest.mark.card("GD04-017")
def test_gd04_017_other_pilot_deploys_nothing() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    zeong = sc.add(0, "GD04-017")
    riddhe = sc.hand(0, RIDDHE)[0]
    st = sc.start()
    play(st, riddhe, onto=zeong)
    assert numbers_in(st, 0, Zone.BATTLE) == ["GD04-017"]


@pytest.mark.card("GD04-017")
def test_gd04_017_destroyed_deploys_a_rested_head() -> None:
    sc = Scenario()
    zeong = sc.add(0, "GD04-017")
    big = sc.add(1, "GD04-049", rested=True)
    st = sc.start()
    attack(st, zeong, big)
    pass_all(st)
    assert zone_of(st, zeong) is Zone.TRASH
    (head,) = st.zones[0][Zone.BATTLE]
    assert V.cdef(st, head).name == "Zeong (Head)"
    assert st.cards[head].rested
    assert (ap(st, head), hp(st, head)) == (3, 1)


# ---------------------------------------------------------------------------------------------
# GD04-018 Gundam Pharact


@pytest.mark.card("GD04-018")
def test_gd04_018_other_academy_unit_damaged_by_enemy_places_ex_resource_once() -> None:
    sc = Scenario()
    pharact = sc.add(0, "GD04-018")
    h1 = sc.add(0, HEINDREE)
    h2 = sc.add(0, HEINDREE)
    t1 = sc.add(1, VANILLA, rested=True)
    t2 = sc.add(1, VANILLA, rested=True)
    st = sc.start()
    assert keywords(st, pharact) == {"Breach": 5}
    attack(st, h1, t1)
    pass_all(st)
    assert ex_resources(st) == 1
    attack(st, h2, t2)
    pass_all(st)
    assert ex_resources(st) == 1


@pytest.mark.card("GD04-018")
def test_gd04_018_pharact_itself_or_opponent_turn_does_not_trigger() -> None:
    sc = Scenario()
    pharact = sc.add(0, "GD04-018")
    heindree = sc.add(0, HEINDREE, rested=True)
    target = sc.add(1, BIGRO, rested=True)
    enemy = sc.add(1, VANILLA)
    st = sc.start()
    attack(st, pharact, target)
    pass_all(st)
    assert ex_resources(st) == 0
    to_next_turn(st)
    attack(st, enemy, heindree)
    pass_all(st)
    assert zone_of(st, heindree) is Zone.TRASH
    assert ex_resources(st) == 0


# ---------------------------------------------------------------------------------------------
# GD04-019 GN Armor Type-D (Trans-Am)


def _gn_armor_destroyed(sc: Scenario) -> GameState:
    armor = sc.add(0, "GD04-019")
    big = sc.add(1, "GD04-049", rested=True)
    sc.deck(0, EXIA, "GD04-036", VIRTUE)
    st = sc.start()
    assert keywords(st, armor) == {"Breach": 3}
    attack(st, armor, big)
    pass_all(st)
    assert zone_of(st, armor) is Zone.TRASH
    return st


@pytest.mark.card("GD04-019")
def test_gd04_019_destroyed_adds_a_lv5_or_lower_cb_unit_and_bottoms_the_rest() -> None:
    st = _gn_armor_destroyed(Scenario())
    top3 = st.zones[0][Zone.DECK][:3]
    exia, eins, virtue = top3
    assert pending(st) is DecisionKind.YES_NO
    yes(st)
    assert option_uids(st) == {exia, virtue}
    select(st, exia)
    assert zone_of(st, exia) is Zone.HAND
    assert set(st.zones[0][Zone.DECK][-2:]) == {eins, virtue}


@pytest.mark.card("GD04-019")
@pytest.mark.ruling("GD04-019:Q475")
def test_gd04_019_declining_still_looks_and_bottoms_all_three() -> None:
    st = _gn_armor_destroyed(Scenario())
    top3 = st.zones[0][Zone.DECK][:3]
    assert all(st.cards[u].known & 1 for u in top3)
    no(st)
    assert set(st.zones[0][Zone.DECK][-3:]) == set(top3)


# ---------------------------------------------------------------------------------------------
# GD04-020 Gundam Lfrith Ur / GD04-021 Gundam Lfrith Thorn


def _play_violence(st: GameState, violence: int, *, ex: int) -> None:
    play(st, violence, ex=ex)
    if pending(st) is DecisionKind.SELECT:
        select(st, min(option_uids(st)))


@pytest.mark.card("GD04-020")
@pytest.mark.rule("13-2-13-1")
def test_gd04_020_draws_once_when_a_dawn_of_fold_command_uses_an_ex_resource() -> None:
    sc = Scenario()
    sc.resources(0, 5, ex=2)
    sc.add(0, "GD04-020")
    v1, v2 = sc.hand(0, VIOLENCE, VIOLENCE)
    st = sc.start()
    before = hand_size(st)
    _play_violence(st, v1, ex=1)
    assert hand_size(st) == before
    _play_violence(st, v2, ex=1)
    assert hand_size(st) == before - 1


@pytest.mark.card("GD04-020")
def test_gd04_020_no_draw_without_an_ex_resource() -> None:
    sc = Scenario()
    sc.resources(0, 5, ex=1)
    sc.add(0, "GD04-020")
    violence = sc.hand(0, VIOLENCE)[0]
    st = sc.start()
    before = hand_size(st)
    _play_violence(st, violence, ex=0)
    assert hand_size(st) == before - 1


@pytest.mark.card("GD04-021")
@pytest.mark.ruling("GD04-021:Q267")
@pytest.mark.rule("3-4-6")
def test_gd04_021_pairs_the_command_from_the_trash_after_it_resolves() -> None:
    sc = Scenario()
    sc.resources(0, 5, ex=1)
    thorn = sc.add(0, "GD04-021")
    violence = sc.hand(0, VIOLENCE)[0]
    st = sc.start()
    assert keywords(st, thorn) == {"Breach": 3}
    play(st, violence, ex=1)
    assert pending(st) is DecisionKind.YES_NO
    assert zone_of(st, violence) is Zone.TRASH
    yes(st)
    assert zone_of(st, violence) is Zone.PAIRED
    assert st.cards[thorn].pair == violence
    assert ap(st, thorn) == 6
    assert V.is_linked(V.derived(st), thorn)


@pytest.mark.card("GD04-021")
def test_gd04_021_no_offer_without_an_ex_resource() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    sc.add(0, "GD04-021")
    violence = sc.hand(0, VIOLENCE)[0]
    st = sc.start()
    play(st, violence)
    assert pending(st) is DecisionKind.MAIN
    assert zone_of(st, violence) is Zone.TRASH


# ---------------------------------------------------------------------------------------------
# GD04-022 Kikeroga (MS Mode) (GQ)


@pytest.mark.card("GD04-022")
def test_gd04_022_your_unit_tokens_gain_breach() -> None:
    sc = Scenario()
    sc.add(0, "GD04-022")
    parts = sc.add(0, PARTS)
    enemy_parts = sc.add(1, PARTS)
    st = sc.start()
    assert keywords(st, parts) == {"Breach": 1}
    assert keywords(st, enemy_parts) == {}


@pytest.mark.card("GD04-022")
def test_gd04_022_linked_makes_all_lv3_or_lower_units_deploy_rested() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    sc.add(1, "GD04-022", pilot=CHALLIA)
    zaku, schuzrum = sc.hand(0, VANILLA, SCHUZRUM)
    st = sc.start()
    play(st, zaku)
    play(st, schuzrum)
    assert st.cards[zaku].rested
    assert not st.cards[schuzrum].rested


@pytest.mark.card("GD04-022")
def test_gd04_022_unlinked_or_tokens_are_not_deployed_rested() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    sc.add(0, "GD04-022", pilot=RIDDHE)
    zaku = sc.hand(0, VANILLA)[0]
    hexa = sc.add(0, "GD04-007", pilot=RIDDHE)
    sc.shields(1, VANILLA)
    st = sc.start()
    play(st, zaku)
    assert not st.cards[zaku].rested
    attack(st, hexa)
    token = next(u for u in st.zones[0][Zone.BATTLE] if V.cdef(st, u).card_number == PARTS)
    assert not st.cards[token].rested


@pytest.mark.card("GD04-022")
def test_gd04_022_linked_tokens_are_not_deployed_rested() -> None:
    sc = Scenario()
    sc.add(0, "GD04-022", pilot=CHALLIA)
    hexa = sc.add(0, "GD04-007", pilot=RIDDHE)
    sc.shields(1, VANILLA)
    st = sc.start()
    attack(st, hexa)
    token = next(u for u in st.zones[0][Zone.BATTLE] if V.cdef(st, u).card_number == PARTS)
    assert not st.cards[token].rested


@pytest.mark.card("GD04-022", "GD04-015")
@pytest.mark.xfail(
    strict=True,
    reason="DSL: no 'deployed rested' static; the Unit is still active while its own 【Deploy】 resolves",
)
def test_gd04_022_unit_is_already_rested_when_its_deploy_effect_resolves() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    sc.add(1, "GD04-022", pilot=CHALLIA)
    enemy = sc.add(1, VANILLA)
    gun_ez = sc.hand(0, "GD04-015")[0]
    st = sc.start()
    play(st, gun_ez)
    assert st.cards[gun_ez].rested
    assert not st.cards[enemy].rested


# ---------------------------------------------------------------------------------------------
# GD04-023 Gundam Kyrios (Tail Booster)


@pytest.mark.card("GD04-023")
def test_gd04_023_super_soldier_unit_may_attack_active_lv4_or_lower() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    kyrios = sc.hand(0, "GD04-023")[0]
    virtue = sc.add(0, VIRTUE, pilot=HALLELUJAH)
    lv4 = sc.add(1, BLOCKER)
    lv5 = sc.add(1, BIGRO)
    st = sc.start()
    assert not has_action(st, A.ATTACK, virtue, lv4)
    play(st, kyrios)
    assert has_action(st, A.ATTACK, virtue, lv4)
    assert not has_action(st, A.ATTACK, virtue, lv5)


@pytest.mark.card("GD04-023")
def test_gd04_023_no_unit_paired_with_a_super_soldier() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    kyrios = sc.hand(0, "GD04-023")[0]
    virtue = sc.add(0, VIRTUE, pilot=TIERIA)
    lv4 = sc.add(1, BLOCKER)
    st = sc.start()
    play(st, kyrios)
    assert not has_action(st, A.ATTACK, virtue, lv4)


# ---------------------------------------------------------------------------------------------
# GD04-024 Gundam Aerial Rebuild


def _aerial(sc: Scenario) -> tuple[GameState, list[int]]:
    sc.resources(0, 7)
    aerial = sc.hand(0, "GD04-024")[0]
    sc.deck(0, VIOLENCE, VANILLA, HEINDREE)
    st = sc.start()
    top3 = list(st.zones[0][Zone.DECK][:3])
    play(st, aerial)
    return st, top3


@pytest.mark.card("GD04-024")
def test_gd04_024_adds_an_academy_unit_or_command_card() -> None:
    st, top3 = _aerial(Scenario())
    violence, zaku, heindree = top3
    yes(st)
    assert option_uids(st) == {violence, heindree}
    select(st, violence)
    assert zone_of(st, violence) is Zone.HAND
    assert set(st.zones[0][Zone.DECK][-2:]) == {zaku, heindree}


@pytest.mark.card("GD04-024")
@pytest.mark.ruling("GD04-024:Q476")
def test_gd04_024_declining_still_bottoms_the_looked_cards() -> None:
    st, top3 = _aerial(Scenario())
    no(st)
    assert set(st.zones[0][Zone.DECK][-3:]) == set(top3)


# ---------------------------------------------------------------------------------------------
# GD04-025 Gundvölva


@pytest.mark.card("GD04-025")
def test_gd04_025_destroyed_on_your_turn_with_another_dawn_of_fold_places_ex() -> None:
    sc = Scenario()
    gundvolva = sc.add(0, "GD04-025")
    sc.add(0, "GD05-032")
    big = sc.add(1, BIGRO, rested=True)
    st = sc.start()
    attack(st, gundvolva, big)
    pass_all(st)
    assert zone_of(st, gundvolva) is Zone.TRASH
    assert ex_resources(st) == 1


@pytest.mark.card("GD04-025")
def test_gd04_025_no_ex_without_another_dawn_of_fold() -> None:
    sc = Scenario()
    gundvolva = sc.add(0, "GD04-025")
    big = sc.add(1, BIGRO, rested=True)
    st = sc.start()
    attack(st, gundvolva, big)
    pass_all(st)
    assert zone_of(st, gundvolva) is Zone.TRASH
    assert ex_resources(st) == 0


@pytest.mark.card("GD04-025")
def test_gd04_025_no_ex_when_destroyed_on_the_opponent_turn() -> None:
    sc = Scenario(active=1)
    target = sc.add(0, "GD04-025", rested=True)
    sc.add(0, "GD05-032")
    attacker = sc.add(1, BIGRO)
    st = sc.start()
    attack(st, attacker, target)
    pass_all(st)
    assert zone_of(st, target) is Zone.TRASH
    assert ex_resources(st, 0) == 0


# ---------------------------------------------------------------------------------------------
# GD04-026 Garma's Dopp


@pytest.mark.card("GD04-026")
def test_gd04_026_look_at_top_card_and_trash_it() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    dopp = sc.hand(0, "GD04-026")[0]
    sc.deck(0, BIGRO)
    st = sc.start()
    top = st.zones[0][Zone.DECK][0]
    play(st, dopp)
    assert st.cards[top].known & 1
    yes(st)
    assert zone_of(st, top) is Zone.TRASH


@pytest.mark.card("GD04-026")
def test_gd04_026_look_at_top_card_and_keep_it_on_top() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    dopp = sc.hand(0, "GD04-026")[0]
    sc.deck(0, BIGRO)
    st = sc.start()
    top = st.zones[0][Zone.DECK][0]
    play(st, dopp)
    no(st)
    assert st.zones[0][Zone.DECK][0] == top


# ---------------------------------------------------------------------------------------------
# GD04-028 Zakrello


@pytest.mark.card("GD04-028")
def test_gd04_028_attack_gives_an_active_enemy_blocker() -> None:
    sc = Scenario()
    zakrello = sc.add(0, "GD04-028")
    enemy = sc.add(1, VANILLA)
    sc.add(1, BIGRO, rested=True)
    sc.shields(1, VANILLA)
    st = sc.start()
    attack(st, zakrello)
    assert keywords(st, enemy) == {"Blocker": 1}
    assert pending(st) is DecisionKind.BLOCK
    assert has_action(st, A.BLOCK, enemy)


# ---------------------------------------------------------------------------------------------
# GD04-029 Gundam Dynames (GN Full Shield)


@pytest.mark.card("GD04-029")
@pytest.mark.ruling("GD04-029:Q268")
@pytest.mark.rule("5-21-1")
def test_gd04_029_reduces_enemy_damage_by_one_with_a_cb_pilot_in_play() -> None:
    sc = Scenario()
    dynames = sc.add(0, "GD04-029")
    sc.add(0, VIRTUE, pilot=TIERIA)
    target = sc.add(1, "GD04-003", rested=True)
    st = sc.start()
    attack(st, dynames, target)
    pass_all(st)
    assert st.cards[dynames].damage == 2
    assert zone_of(st, dynames) is Zone.BATTLE


@pytest.mark.card("GD04-029")
def test_gd04_029_no_reduction_without_a_cb_pilot() -> None:
    sc = Scenario()
    dynames = sc.add(0, "GD04-029")
    sc.add(0, VIRTUE, pilot=RIDDHE)
    target = sc.add(1, "GD04-003", rested=True)
    st = sc.start()
    attack(st, dynames, target)
    pass_all(st)
    assert zone_of(st, dynames) is Zone.TRASH


@pytest.mark.card("GD04-029")
@pytest.mark.rule("13-2-13-1")
def test_gd04_029_reduction_is_once_per_turn() -> None:
    sc = Scenario(active=1)
    sc.resources(1, 4)
    dynames = sc.add(0, "GD04-029", rested=True)
    sc.add(0, VIRTUE, pilot=TIERIA)
    attacker = sc.add(1, VANILLA)
    finger = sc.hand(1, DARKNESS_FINGER)[0]
    st = sc.start()
    attack(st, attacker, dynames)
    pass_all(st)
    assert st.cards[dynames].damage == 1
    play(st, finger)
    select(st, dynames)
    assert zone_of(st, dynames) is Zone.TRASH


# ---------------------------------------------------------------------------------------------
# GD04-030 Chuchu's Demi Trainer


@pytest.mark.card("GD04-030")
def test_gd04_030_other_academy_unit_may_attack_active_lv3_or_lower() -> None:
    sc = Scenario()
    chuchu = sc.add(0, "GD04-030")
    heindree = sc.add(0, HEINDREE)
    lv2 = sc.add(1, VANILLA)
    lv4 = sc.add(1, BLOCKER, rested=True)
    lv5 = sc.add(1, BIGRO)
    st = sc.start()
    assert not has_action(st, A.ATTACK, heindree, lv2)
    attack(st, chuchu, lv4)
    pass_all(st)
    assert has_action(st, A.ATTACK, heindree, lv2)
    assert not has_action(st, A.ATTACK, heindree, lv5)


# ---------------------------------------------------------------------------------------------
# GD04-033 Neo Zeong


@pytest.mark.card("GD04-033")
def test_gd04_033_deploying_itself_deals_3_damage() -> None:
    sc = Scenario()
    sc.resources(0, 9)
    neo = sc.hand(0, "GD04-033")[0]
    enemy = sc.add(1, BIGRO)
    st = sc.start()
    play(st, neo)
    assert st.cards[enemy].damage == 3


@pytest.mark.card("GD04-033")
def test_gd04_033_another_neo_zeon_unit_deployed_deals_3_damage() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    sc.add(0, "GD04-033")
    schuzrum, zaku = sc.hand(0, SCHUZRUM, VANILLA)
    enemy = sc.add(1, "GD04-049")
    st = sc.start()
    play(st, schuzrum)
    assert st.cards[enemy].damage == 3
    play(st, zaku)
    assert st.cards[enemy].damage == 3


@pytest.mark.card("GD04-033")
@pytest.mark.ruling("GD04-033:Q269")
@pytest.mark.rule("10-1-5-4")
def test_gd04_033_linked_grants_neo_zeon_so_any_deployed_unit_triggers() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    neo = sc.add(0, "GD04-033", pilot=FULL_FRONTAL)
    zaku = sc.hand(0, VANILLA)[0]
    enemy = sc.add(1, "GD04-049")
    enemy_zaku = sc.add(1, VANILLA)
    st = sc.start()
    assert "Neo Zeon" in V.traits_of(st, V.derived(st), neo)
    assert "Neo Zeon" not in V.traits_of(st, V.derived(st), enemy_zaku)
    play(st, zaku)
    select(st, enemy)
    assert st.cards[enemy].damage == 3


@pytest.mark.card("GD04-033", "GD04-039")
@pytest.mark.ruling("GD04-033:Q335")
def test_gd04_033_trait_grant_does_not_reach_cards_in_hand_or_trash() -> None:
    sc = Scenario()
    sc.add(0, "GD04-033", pilot=FULL_FRONTAL)
    sc.trash(0, *([SCHUZRUM] * 7), VANILLA)
    rozen = sc.hand(0, "GD04-039")[0]
    zaku = sc.hand(0, VANILLA)[0]
    st = sc.start()
    assert V.play_cost(st, V.derived(st), rozen) == 6
    trash_zaku = st.zones[0][Zone.TRASH][-1]
    has_nz = d.HasTrait(("Neo Zeon",))
    ctx = V.Ctx(0)
    assert not V.matches(st, V.derived(st), ctx, trash_zaku, (has_nz,))
    assert not V.matches(st, V.derived(st), ctx, zaku, (has_nz,))


# ---------------------------------------------------------------------------------------------
# GD04-034 Gundam Kyrios


@pytest.mark.card("GD04-034")
def test_gd04_034_linked_gets_ap_plus_2_per_rested_cb_unit() -> None:
    sc = Scenario()
    kyrios = sc.add(0, "GD04-034", pilot=HALLELUJAH)
    sc.add(0, VIRTUE, rested=True)
    sc.add(0, EXIA)
    sc.add(0, VANILLA, rested=True)
    sc.shields(1, VANILLA)
    st = sc.start()
    assert keywords(st, kyrios) == {"First Strike": 1}
    assert ap(st, kyrios) == 1 + 2 + 2
    attack(st, kyrios)
    assert ap(st, kyrios) == 1 + 2 + 4


@pytest.mark.card("GD04-034")
def test_gd04_034_unlinked_gets_no_bonus() -> None:
    sc = Scenario()
    kyrios = sc.add(0, "GD04-034", pilot=TIERIA)
    sc.add(0, VIRTUE, rested=True)
    st = sc.start()
    assert ap(st, kyrios) == 1 + 1


# ---------------------------------------------------------------------------------------------
# GD04-035 Ξ Gundam


def _xi_gundam(sc: Scenario, victim: str, hand: int) -> tuple[GameState, int, int]:
    sc.resources(0, 5)
    xi = sc.hand(0, "GD04-035")[0]
    sc.hand(0, *([VANILLA] * hand))
    messer = sc.add(0, MESSER)
    target = sc.add(1, victim, rested=True)
    st = sc.start()
    play(st, xi)
    select(st, messer)
    return st, messer, target


@pytest.mark.card("GD04-035")
def test_gd04_035_chosen_unit_destroying_an_enemy_draws_with_small_hand() -> None:
    st, messer, target = _xi_gundam(Scenario(), VANILLA, 3)
    assert hand_size(st) == 3
    attack(st, messer, target)
    pass_all(st)
    assert zone_of(st, target) is Zone.TRASH
    assert hand_size(st) == 4


@pytest.mark.card("GD04-035")
def test_gd04_035_no_draw_with_four_cards_in_hand() -> None:
    st, messer, target = _xi_gundam(Scenario(), VANILLA, 4)
    attack(st, messer, target)
    pass_all(st)
    assert zone_of(st, target) is Zone.TRASH
    assert hand_size(st) == 4


@pytest.mark.card("GD04-035")
@pytest.mark.ruling("GD04-035:Q270")
@pytest.mark.rule("8-5-3-2-3", "10-1-6-4")
def test_gd04_035_draws_when_both_units_are_destroyed() -> None:
    st, messer, target = _xi_gundam(Scenario(), BIGRO, 3)
    attack(st, messer, target)
    pass_all(st)
    assert zone_of(st, messer) is Zone.TRASH
    assert zone_of(st, target) is Zone.TRASH
    assert hand_size(st) == 4


# ---------------------------------------------------------------------------------------------
# GD04-036 Gundam Throne Eins


@pytest.mark.card("GD04-036")
def test_gd04_036_rest_two_cb_units_to_deal_2_to_each_lv6_or_lower_enemy() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    eins = sc.hand(0, "GD04-036")[0]
    virtue = sc.add(0, VIRTUE)
    exia = sc.add(0, EXIA)
    low = sc.add(1, BIGRO)
    high = sc.add(1, "GD04-049")
    st = sc.start()
    play(st, eins)
    select(st, virtue, exia)
    assert st.cards[virtue].rested and st.cards[exia].rested
    assert not st.cards[eins].rested
    assert st.cards[low].damage == 2
    assert st.cards[high].damage == 0


@pytest.mark.card("GD04-036")
def test_gd04_036_rest_one_unit_deals_1() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    eins = sc.hand(0, "GD04-036")[0]
    virtue = sc.add(0, VIRTUE)
    exia = sc.add(0, EXIA)
    low = sc.add(1, BIGRO)
    st = sc.start()
    play(st, eins)
    select(st, exia, done=True)
    assert not st.cards[virtue].rested
    assert st.cards[low].damage == 1


@pytest.mark.card("GD04-036")
def test_gd04_036_choosing_nothing_deals_no_damage() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    eins = sc.hand(0, "GD04-036")[0]
    virtue = sc.add(0, VIRTUE)
    low = sc.add(1, BIGRO)
    st = sc.start()
    play(st, eins)
    select(st, done=True)
    assert not st.cards[virtue].rested
    assert st.cards[low].damage == 0


# ---------------------------------------------------------------------------------------------
# GD04-037 Gundam Kyrios (Trans-Am)


@pytest.mark.card("GD04-037")
def test_gd04_037_red_super_soldier_pilot_grants_first_strike_only() -> None:
    sc = Scenario()
    kyrios = sc.add(0, "GD04-037")
    sc.add(0, VIRTUE, pilot=HALLELUJAH)
    st = sc.start()
    assert keywords(st, kyrios) == {"First Strike": 1}


@pytest.mark.card("GD04-037")
def test_gd04_037_no_keywords_without_a_super_soldier_pilot() -> None:
    sc = Scenario()
    kyrios = sc.add(0, "GD04-037", pilot=TIERIA)
    sc.add(1, VIRTUE, pilot=HALLELUJAH)
    st = sc.start()
    assert keywords(st, kyrios) == {}


# ---------------------------------------------------------------------------------------------
# GD04-038 Gundam Exia


@pytest.mark.card("GD04-038")
def test_gd04_038_two_enemy_units_deal_2_to_a_lv2_or_lower() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    exia = sc.hand(0, "GD04-038")[0]
    lv2 = sc.add(1, VANILLA)
    lv5 = sc.add(1, BIGRO)
    st = sc.start()
    play(st, exia)
    assert zone_of(st, lv2) is Zone.TRASH
    assert st.cards[lv5].damage == 0


@pytest.mark.card("GD04-038")
def test_gd04_038_one_enemy_unit_no_damage() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    exia = sc.hand(0, "GD04-038")[0]
    lv2 = sc.add(1, VANILLA)
    st = sc.start()
    play(st, exia)
    assert st.cards[lv2].damage == 0


# ---------------------------------------------------------------------------------------------
# GD04-039 Rozen Zulu


@pytest.mark.card("GD04-039")
def test_gd04_039_costs_4_less_with_8_neo_zeon_cards_in_trash() -> None:
    sc = Scenario()
    sc.trash(0, *([SCHUZRUM] * 8))
    rozen = sc.hand(0, "GD04-039")[0]
    sc2 = Scenario()
    sc2.trash(0, *([SCHUZRUM] * 7))
    rozen2 = sc2.hand(0, "GD04-039")[0]
    st = sc.start()
    st2 = sc2.start()
    assert V.play_cost(st, V.derived(st), rozen) == 2
    assert V.play_cost(st2, V.derived(st2), rozen2) == 6


@pytest.mark.card("GD04-039")
def test_gd04_039_deals_3_to_a_unit_with_repair_else_1() -> None:
    sc = Scenario()
    sc.resources(0, 12)
    r1, r2 = sc.hand(0, "GD04-039", "GD04-039")
    psycho = sc.add(1, "GD04-004")
    big = sc.add(1, BIGRO)
    st = sc.start()
    play(st, r1)
    select(st, psycho)
    assert st.cards[psycho].damage == 3
    play(st, r2)
    select(st, big)
    assert st.cards[big].damage == 1


# ---------------------------------------------------------------------------------------------
# GD04-041 Gundam Throne Drei


@pytest.mark.card("GD04-041", "GD04-036")
@pytest.mark.rule("13-2-13-1")
def test_gd04_041_rested_by_an_effect_sets_itself_active_once_per_turn() -> None:
    sc = Scenario()
    sc.resources(0, 12)
    drei = sc.add(0, "GD04-041")
    e1, e2 = sc.hand(0, "GD04-036", "GD04-036")
    enemy = sc.add(1, BIGRO)
    st = sc.start()
    play(st, e1)
    select(st, drei, done=True)
    assert not st.cards[drei].rested
    assert st.cards[enemy].damage == 1
    play(st, e2)
    select(st, drei, done=True)
    assert st.cards[drei].rested
    assert st.cards[enemy].damage == 2


@pytest.mark.card("GD04-041")
def test_gd04_041_attacking_is_not_rested_by_an_effect() -> None:
    sc = Scenario()
    drei = sc.add(0, "GD04-041")
    sc.shields(1, VANILLA)
    st = sc.start()
    attack(st, drei)
    pass_all(st)
    assert st.cards[drei].rested


# ---------------------------------------------------------------------------------------------
# GD04-042 Psycho Gundam (GQ)


@pytest.mark.card("GD04-042")
def test_gd04_042_linked_shield_destruction_deals_2_to_a_5_ap_or_less_enemy() -> None:
    sc = Scenario()
    psycho = sc.add(0, "GD04-042", pilot=DEUX)
    small = sc.add(1, BIGRO)
    sc.add(1, "GD04-001")
    sc.shields(1, VANILLA, VANILLA)
    st = sc.start()
    attack(st, psycho)
    pass_all(st)
    assert st.cards[small].damage == 2


@pytest.mark.card("GD04-042")
@pytest.mark.rule("13-2-13-1")
def test_gd04_042_once_per_turn() -> None:
    sc = Scenario()
    psycho = sc.add(0, "GD04-042", pilot=DEUX)
    other = sc.add(0, "GD04-008", pilot=FOUR)
    small = sc.add(1, "GD04-002")
    sc.add(1, "GD04-049")
    sc.shields(1, VANILLA, VANILLA, VANILLA)
    st = sc.start()
    attack(st, psycho)
    pass_all(st)
    assert st.cards[small].damage == 2
    attack(st, other)
    pass_all(st)
    assert st.cards[small].damage == 2


@pytest.mark.card("GD04-042")
def test_gd04_042_unlinked_does_not_trigger() -> None:
    sc = Scenario()
    psycho = sc.add(0, "GD04-042", pilot=FOUR)
    small = sc.add(1, BIGRO)
    sc.shields(1, VANILLA, VANILLA)
    st = sc.start()
    attack(st, psycho)
    pass_all(st)
    assert st.cards[small].damage == 0


@pytest.mark.card("GD04-042", "GD04-018")
@pytest.mark.rule("13-1-2-1")
def test_gd04_042_breach_destroying_the_base_triggers() -> None:
    sc = Scenario()
    sc.add(0, "GD04-042", pilot=DEUX)
    pharact = sc.add(0, "GD04-018", pilot=FOUR)
    victim = sc.add(1, VANILLA, rested=True)
    small = sc.add(1, BIGRO)
    base = sc.base(1)
    st = sc.start()
    attack(st, pharact, victim)
    pass_all(st)
    assert zone_of(st, base) is not Zone.BASE
    assert st.cards[small].damage == 2


# ---------------------------------------------------------------------------------------------
# GD04-043 Zssa (Sleeves)


@pytest.mark.card("GD04-043")
def test_gd04_043_deals_1_damage_to_the_enemy_base() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    zssa = sc.hand(0, "GD04-043")[0]
    base = sc.base(1)
    st = sc.start()
    play(st, zssa)
    assert st.cards[base].damage == 1


# ---------------------------------------------------------------------------------------------
# GD04-044 Gadeel


@pytest.mark.card("GD04-044")
def test_gd04_044_attacking_a_damaged_unit_gains_breach_3_this_battle() -> None:
    sc = Scenario()
    gadeel = sc.add(0, "GD04-044")
    target = sc.add(1, VANILLA, rested=True, damage=1)
    sc.shields(1, VANILLA, VANILLA)
    st = sc.start()
    attack(st, gadeel, target)
    assert zone_of(st, target) is Zone.TRASH
    assert len(st.zones[1][Zone.SHIELD]) == 1
    assert keywords(st, gadeel) == {}


@pytest.mark.card("GD04-044")
def test_gd04_044_attacking_an_undamaged_unit_gains_nothing() -> None:
    sc = Scenario()
    gadeel = sc.add(0, "GD04-044")
    target = sc.add(1, VANILLA, rested=True)
    sc.shields(1, VANILLA, VANILLA)
    st = sc.start()
    attack(st, gadeel, target)
    assert zone_of(st, target) is Zone.TRASH
    assert len(st.zones[1][Zone.SHIELD]) == 2


# ---------------------------------------------------------------------------------------------
# GD04-045 Gundam Throne Zwei


@pytest.mark.card("GD04-045")
def test_gd04_045_when_linked_cb_unit_may_attack_damaged_active_enemy() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    zwei = sc.add(0, "GD04-045")
    virtue = sc.add(0, VIRTUE)
    ali = sc.hand(0, ALI)[0]
    damaged = sc.add(1, BIGRO, damage=1)
    healthy = sc.add(1, BIGRO)
    st = sc.start()
    play(st, ali, onto=zwei)
    select(st, virtue)
    assert has_action(st, A.ATTACK, virtue, damaged)
    assert not has_action(st, A.ATTACK, virtue, healthy)


# ---------------------------------------------------------------------------------------------
# GD04-046 Gundam Dynames


@pytest.mark.card("GD04-046")
def test_gd04_046_rest_itself_to_deal_2_to_a_lv3_or_lower_enemy() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    dynames = sc.hand(0, "GD04-046")[0]
    target = sc.add(1, VANILLA)
    st = sc.start()
    play(st, dynames)
    yes(st)
    assert st.cards[dynames].rested
    assert zone_of(st, target) is Zone.TRASH


@pytest.mark.card("GD04-046")
def test_gd04_046_declining_deals_nothing() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    dynames = sc.hand(0, "GD04-046")[0]
    target = sc.add(1, VANILLA)
    st = sc.start()
    play(st, dynames)
    no(st)
    assert not st.cards[dynames].rested
    assert st.cards[target].damage == 0


# ---------------------------------------------------------------------------------------------
# GD04-049 Gundam DX


@pytest.mark.card("GD04-049")
def test_gd04_049_exile_7_vulture_cards_to_destroy_a_lv8_or_lower_unit() -> None:
    sc = Scenario()
    dx = sc.add(0, "GD04-049", pilot=RIDDHE)
    vultures = sc.trash(0, *(["GD04-059"] * 7))
    victim = sc.add(1, "GD04-049")
    sc.shields(1, VANILLA, VANILLA, VANILLA)
    st = sc.start()
    assert keywords(st, dx) == {"Suppression": 1}
    attack(st, dx)
    yes(st)
    assert all(zone_of(st, u) is Zone.REMOVAL for u in vultures)
    assert zone_of(st, victim) is Zone.TRASH
    pass_all(st)
    assert len(st.zones[1][Zone.SHIELD]) == 1


@pytest.mark.card("GD04-049")
def test_gd04_049_six_vulture_cards_do_nothing() -> None:
    sc = Scenario()
    dx = sc.add(0, "GD04-049", pilot=RIDDHE)
    sc.trash(0, *(["GD04-059"] * 6))
    sc.shields(1, VANILLA, VANILLA, VANILLA)
    st = sc.start()
    attack(st, dx)
    assert pending(st) is not DecisionKind.YES_NO
    assert len(st.zones[0][Zone.TRASH]) == 6


@pytest.mark.card("GD04-049")
@pytest.mark.rule("13-2-10-1")
def test_gd04_049_unpaired_does_nothing() -> None:
    sc = Scenario()
    dx = sc.add(0, "GD04-049")
    sc.trash(0, *(["GD04-059"] * 7))
    sc.shields(1, VANILLA, VANILLA, VANILLA)
    st = sc.start()
    attack(st, dx)
    assert pending(st) is not DecisionKind.YES_NO
    assert len(st.zones[0][Zone.TRASH]) == 7


@pytest.mark.card("GD04-049")
def test_gd04_049_attacking_a_unit_does_nothing() -> None:
    sc = Scenario()
    dx = sc.add(0, "GD04-049", pilot=RIDDHE)
    sc.trash(0, *(["GD04-059"] * 7))
    target = sc.add(1, VANILLA, rested=True)
    st = sc.start()
    attack(st, dx, target)
    assert pending(st) is not DecisionKind.YES_NO


@pytest.mark.card("GD04-049")
def test_gd04_049_must_exile_exactly_seven() -> None:
    sc = Scenario()
    dx = sc.add(0, "GD04-049", pilot=RIDDHE)
    sc.trash(0, *(["GD04-059"] * 8))
    sc.add(1, BIGRO)
    sc.shields(1, VANILLA, VANILLA, VANILLA)
    st = sc.start()
    attack(st, dx)
    yes(st)
    assert st.pending is not None
    assert not has_action(st, A.DONE)
    for u in list(option_uids(st))[:7]:
        assert not has_action(st, A.DONE)
        select(st, u, done=False)
    assert len(st.zones[0][Zone.REMOVAL]) == 7


# ---------------------------------------------------------------------------------------------
# GD04-050 Destiny Gundam


@pytest.mark.card("GD04-050")
def test_gd04_050_paired_attack_pays_to_deploy_a_minerva_squad_unit_from_trash() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    destiny = sc.add(0, "GD04-050", pilot=RIDDHE)
    zaku = sc.trash(0, "GD04-062")[0]
    sc.shields(1, VANILLA)
    st = sc.start()
    assert keywords(st, destiny) == {"High-Maneuver": 1}
    attack(st, destiny)
    select(st, zaku)
    assert zone_of(st, zaku) is Zone.BATTLE
    assert active_resources(st) == 2


@pytest.mark.card("GD04-050")
def test_gd04_050_unpaired_attack_offers_nothing() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    destiny = sc.add(0, "GD04-050")
    zaku = sc.trash(0, "GD04-062")[0]
    sc.shields(1, VANILLA)
    st = sc.start()
    attack(st, destiny)
    assert zone_of(st, zaku) is Zone.TRASH
    assert pending(st) is not DecisionKind.SELECT


# ---------------------------------------------------------------------------------------------
# GD04-051 Gundam Airmaster Burst


@pytest.mark.card("GD04-051")
@pytest.mark.ruling("GD04-051:Q271")
def test_gd04_051_may_attack_active_units_with_a_keyword_effect() -> None:
    sc = Scenario()
    airmaster = sc.add(0, "GD04-051", pilot=PALA)
    sc.trash(0, *([VANILLA] * 7))
    blocker = sc.add(1, BLOCKER)
    breach = sc.add(1, "GD04-006")
    plain = sc.add(1, BIGRO)
    st = sc.start()
    assert has_action(st, A.ATTACK, airmaster, blocker)
    assert has_action(st, A.ATTACK, airmaster, breach)
    assert not has_action(st, A.ATTACK, airmaster, plain)


@pytest.mark.card("GD04-051")
def test_gd04_051_six_trash_cards_are_not_enough() -> None:
    sc = Scenario()
    airmaster = sc.add(0, "GD04-051", pilot=PALA)
    sc.trash(0, *([VANILLA] * 6))
    blocker = sc.add(1, BLOCKER)
    st = sc.start()
    assert not has_action(st, A.ATTACK, airmaster, blocker)


@pytest.mark.card("GD04-051")
@pytest.mark.rule("13-2-10-2")
def test_gd04_051_needs_a_vulture_pilot() -> None:
    sc = Scenario()
    airmaster = sc.add(0, "GD04-051", pilot=RIDDHE)
    sc.trash(0, *([VANILLA] * 7))
    blocker = sc.add(1, BLOCKER)
    st = sc.start()
    assert not has_action(st, A.ATTACK, airmaster, blocker)


# ---------------------------------------------------------------------------------------------
# GD04-052 Gundam Leopard Destroy


@pytest.mark.card("GD04-052")
def test_gd04_052_deals_2_to_the_chosen_enemy_and_itself() -> None:
    sc = Scenario()
    leopard = sc.add(0, "GD04-052", pilot=RIDDHE)
    enemy = sc.add(1, BIGRO)
    sc.shields(1, VANILLA)
    st = sc.start()
    attack(st, leopard)
    select(st, enemy)
    assert st.cards[enemy].damage == 2
    assert st.cards[leopard].damage == 2


@pytest.mark.card("GD04-052")
def test_gd04_052_choosing_nothing_deals_no_damage() -> None:
    sc = Scenario()
    leopard = sc.add(0, "GD04-052", pilot=RIDDHE)
    enemy = sc.add(1, BIGRO)
    sc.shields(1, VANILLA)
    st = sc.start()
    attack(st, leopard)
    select(st, done=True)
    assert st.cards[enemy].damage == 0
    assert st.cards[leopard].damage == 0


# ---------------------------------------------------------------------------------------------
# GD04-053 Rey's Blaze Zaku Phantom


@pytest.mark.card("GD04-053")
@pytest.mark.ruling("GD04-053:Q272")
def test_gd04_053_linked_reduces_enemy_damage_by_one_once_per_turn() -> None:
    sc = Scenario(active=1)
    sc.resources(1, 4)
    zaku = sc.add(0, "GD04-053", pilot=REY, rested=True)
    attacker = sc.add(1, "GD04-003")
    finger = sc.hand(1, DARKNESS_FINGER)[0]
    st = sc.start()
    assert (ap(st, zaku), hp(st, zaku)) == (5, 5)
    attack(st, attacker, zaku)
    pass_all(st)
    assert st.cards[zaku].damage == 2
    play(st, finger)
    assert st.cards[zaku].damage == 4


@pytest.mark.card("GD04-053")
def test_gd04_053_unlinked_takes_full_damage() -> None:
    sc = Scenario(active=1)
    zaku = sc.add(0, "GD04-053", pilot=RIDDHE, rested=True)
    attacker = sc.add(1, "GD04-003")
    st = sc.start()
    attack(st, attacker, zaku)
    pass_all(st)
    assert st.cards[zaku].damage == 3


# ---------------------------------------------------------------------------------------------
# GD04-054 Gundam Virtue (Trans-Am)


@pytest.mark.card("GD04-054")
def test_gd04_054_battle_damage_to_an_enemy_unit_destroys_it() -> None:
    sc = Scenario()
    virtue = sc.add(0, "GD04-054")
    target = sc.add(1, "GD04-049", rested=True)
    st = sc.start()
    attack(st, virtue, target)
    pass_all(st)
    assert zone_of(st, target) is Zone.TRASH


@pytest.mark.card("GD04-054")
def test_gd04_054_also_when_it_is_the_attack_target() -> None:
    sc = Scenario(active=1)
    virtue = sc.add(0, "GD04-054", rested=True)
    attacker = sc.add(1, "GD04-049")
    st = sc.start()
    attack(st, attacker, virtue)
    pass_all(st)
    assert zone_of(st, attacker) is Zone.TRASH


@pytest.mark.card("GD04-054")
@pytest.mark.ruling("GD04-054:Q273")
@pytest.mark.rule("5-5-5")
def test_gd04_054_zero_ap_deals_no_damage_and_destroys_nothing() -> None:
    sc = Scenario()
    sc.resources(1, 3)
    virtue = sc.add(0, "GD04-054")
    target = sc.add(1, "GD04-049", rested=True)
    incident = sc.hand(1, UNFORESEEN)[0]
    st = sc.start()
    attack(st, virtue, target)
    assert st.pending is not None and st.pending.player == 1
    play(st, incident)
    pass_all(st)
    assert zone_of(st, virtue) is Zone.TRASH
    assert zone_of(st, target) is Zone.BATTLE
    assert st.cards[target].damage == 0


# ---------------------------------------------------------------------------------------------
# GD04-056 Sword Impulse Gundam


@pytest.mark.card("GD04-056")
def test_gd04_056_damage_itself_to_rest_a_3_ap_or_less_enemy() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    impulse = sc.hand(0, "GD04-056")[0]
    weak = sc.add(1, VANILLA)
    strong = sc.add(1, BIGRO)
    st = sc.start()
    play(st, impulse)
    assert st.cards[impulse].damage == 1
    assert st.cards[weak].rested
    assert not st.cards[strong].rested


# ---------------------------------------------------------------------------------------------
# GD04-057 Gundam Nadleeh


@pytest.mark.card("GD04-057")
def test_gd04_057_ap_reduced_by_gundam_virtue_unit_cards_in_trash() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    nadleeh = sc.hand(0, "GD04-057")[0]
    sc.trash(0, VIRTUE, "GD04-054", VANILLA)
    enemy = sc.add(1, BIGRO)
    st = sc.start()
    play(st, nadleeh)
    assert ap(st, enemy) == 3
    to_next_turn(st)
    assert ap(st, enemy) == 5


@pytest.mark.card("GD04-057")
def test_gd04_057_amount_is_fixed_when_the_effect_resolves() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    nadleeh = sc.hand(0, "GD04-057")[0]
    sc.trash(0, VIRTUE, VIRTUE)
    virtue = sc.add(0, VIRTUE)
    enemy = sc.add(1, BIGRO)
    victim = sc.add(1, BIGRO, rested=True)
    st = sc.start()
    play(st, nadleeh)
    select(st, enemy)
    assert ap(st, enemy) == 3
    attack(st, virtue, victim)
    pass_all(st)
    assert zone_of(st, virtue) is Zone.TRASH
    assert ap(st, enemy) == 3


@pytest.mark.card("GD04-057")
def test_gd04_057_no_gundam_virtue_no_reduction() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    nadleeh = sc.hand(0, "GD04-057")[0]
    enemy = sc.add(1, BIGRO)
    st = sc.start()
    play(st, nadleeh)
    assert ap(st, enemy) == 5


# ---------------------------------------------------------------------------------------------
# GD04-058 Jamil's Gundam X


@pytest.mark.card("GD04-058")
def test_gd04_058_destroyed_on_your_turn_returns_the_vulture_pilot() -> None:
    sc = Scenario()
    gx = sc.add(0, "GD04-058", pilot=PALA)
    pala = pilot_of(sc, gx)
    big = sc.add(1, BIGRO, rested=True)
    st = sc.start()
    attack(st, gx, big)
    pass_all(st)
    assert zone_of(st, gx) is Zone.TRASH
    assert zone_of(st, pala) is Zone.HAND


@pytest.mark.card("GD04-058")
def test_gd04_058_destroyed_on_opponent_turn_keeps_pilot_in_trash() -> None:
    sc = Scenario(active=1)
    gx = sc.add(0, "GD04-058", pilot=PALA, rested=True)
    pala = pilot_of(sc, gx)
    attacker = sc.add(1, BIGRO)
    st = sc.start()
    attack(st, attacker, gx)
    pass_all(st)
    assert zone_of(st, pala) is Zone.TRASH


@pytest.mark.card("GD04-058")
def test_gd04_058_non_vulture_pilot_is_not_returned() -> None:
    sc = Scenario()
    gx = sc.add(0, "GD04-058", pilot=RIDDHE)
    riddhe = pilot_of(sc, gx)
    big = sc.add(1, BIGRO, rested=True)
    st = sc.start()
    attack(st, gx, big)
    pass_all(st)
    assert zone_of(st, riddhe) is Zone.TRASH


# ---------------------------------------------------------------------------------------------
# GD04-060 Esperansa


@pytest.mark.card("GD04-060")
def test_gd04_060_deployed_from_trash_draws() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    power = sc.hand(0, AWAKENED_POWER)[0]
    esperansa = sc.trash(0, "GD04-060")[0]
    st = sc.start()
    before = hand_size(st)
    play(st, power)
    assert zone_of(st, esperansa) is Zone.BATTLE
    assert hand_size(st) == before


@pytest.mark.card("GD04-060")
def test_gd04_060_deployed_from_hand_does_not_draw() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    esperansa = sc.hand(0, "GD04-060")[0]
    st = sc.start()
    before = hand_size(st)
    play(st, esperansa)
    assert hand_size(st) == before - 1


# ---------------------------------------------------------------------------------------------
# GD04-061 G-Falcon


@pytest.mark.card("GD04-061")
def test_gd04_061_cannot_attack_with_six_or_less_trash_cards() -> None:
    sc = Scenario()
    falcon = sc.add(0, "GD04-061")
    sc.trash(0, *([VANILLA] * 6))
    sc.add(1, VANILLA, rested=True)
    sc.shields(1, VANILLA)
    st = sc.start()
    assert keywords(st, falcon) == {"Blocker": 1}
    assert not has_action(st, A.ATTACK, falcon)


@pytest.mark.card("GD04-061")
def test_gd04_061_can_attack_with_seven_trash_cards() -> None:
    sc = Scenario()
    falcon = sc.add(0, "GD04-061")
    sc.trash(0, *([VANILLA] * 7))
    sc.shields(1, VANILLA)
    st = sc.start()
    assert has_action(st, A.ATTACK, falcon, PLAYER_TARGET)


# ---------------------------------------------------------------------------------------------
# GD04-063 GN Armor Type-E


@pytest.mark.card("GD04-063")
def test_gd04_063_destroys_an_enemy_lv1_or_lower_or_1_ap_or_less() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    armor = sc.hand(0, "GD04-063")[0]
    dopp = sc.add(1, "GD04-026")
    zaku = sc.add(1, VANILLA)
    st = sc.start()
    play(st, armor)
    assert zone_of(st, dopp) is Zone.TRASH
    assert zone_of(st, zaku) is Zone.BATTLE


@pytest.mark.card("GD04-063")
def test_gd04_063_tokens_are_lv0() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    armor = sc.hand(0, "GD04-063")[0]
    token = sc.add(1, "T-023")
    zaku = sc.add(1, VANILLA)
    st = sc.start()
    play(st, armor)
    assert zone_of(st, token) is Zone.OUTSIDE
    assert zone_of(st, zaku) is Zone.BATTLE


# ---------------------------------------------------------------------------------------------
# GD04-065 Unicorn Gundam 02 Banshee Norn (Destroy Mode)


@pytest.mark.card("GD04-065")
@pytest.mark.ruling("GD04-065:Q274")
def test_gd04_065_exile_3_blue_cards_to_stand_up_but_not_attack_the_player() -> None:
    sc = Scenario()
    banshee = sc.add(0, "GD04-065", pilot=RIDDHE, rested=True)
    blues = sc.trash(0, GM3, GM3, GM3)
    sc.trash(0, VANILLA)
    target = sc.add(1, VANILLA, rested=True)
    sc.base(1)
    sc.shields(1, VANILLA)
    st = sc.start()
    activate(st, banshee)
    assert all(zone_of(st, u) is Zone.REMOVAL for u in blues)
    assert not st.cards[banshee].rested
    assert not has_action(st, A.ATTACK, banshee, PLAYER_TARGET)
    assert has_action(st, A.ATTACK, banshee, target)


@pytest.mark.card("GD04-065")
def test_gd04_065_needs_link_and_three_blue_cards() -> None:
    sc = Scenario()
    unlinked = sc.add(0, "GD04-065", pilot=AMURO, rested=True)
    linked = sc.add(0, "GD04-065", pilot=RIDDHE, rested=True)
    sc.trash(0, GM3, GM3, VANILLA)
    st = sc.start()
    assert not has_action(st, A.ACTIVATE, unlinked)
    assert not has_action(st, A.ACTIVATE, linked)


@pytest.mark.card("GD04-065")
def test_gd04_065_attack_gives_all_enemy_units_ap_minus_1() -> None:
    sc = Scenario()
    banshee = sc.add(0, "GD04-065")
    e1 = sc.add(1, BIGRO)
    e2 = sc.add(1, VANILLA)
    sc.shields(1, VANILLA)
    st = sc.start()
    attack(st, banshee)
    assert (ap(st, e1), ap(st, e2)) == (4, 1)


# ---------------------------------------------------------------------------------------------
# GD04-066 Unicorn Gundam (Awakened)


@pytest.mark.card("GD04-066")
def test_gd04_066_activating_a_command_gives_an_enemy_ap_minus_2() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    unicorn = sc.add(0, "GD04-066")
    ploy = sc.hand(0, SIEGE_PLOY)[0]
    enemy = sc.add(1, BIGRO)
    st = sc.start()
    assert keywords(st, unicorn) == {"Suppression": 1}
    play(st, ploy)
    assert st.cards[enemy].rested
    assert ap(st, enemy) == 3


@pytest.mark.card("GD04-066")
def test_gd04_066_opponent_commands_do_not_trigger() -> None:
    sc = Scenario(active=1)
    sc.resources(1, 3)
    sc.add(0, "GD04-066")
    mine = sc.add(0, BIGRO)
    ploy = sc.hand(1, SIEGE_PLOY)[0]
    enemy = sc.add(1, BIGRO)
    st = sc.start()
    play(st, ploy)
    assert st.cards[mine].rested
    assert ap(st, enemy) == 5


@pytest.mark.card("GD04-066")
@pytest.mark.rule("13-2-5-1")
def test_gd04_066_burst_activating_a_command_main_triggers() -> None:
    sc = Scenario(active=1)
    sc.add(0, "GD04-066")
    sc.shields(0, UNFORESEEN)
    attacker = sc.add(1, BIGRO)
    st = sc.start()
    attack(st, attacker)
    pass_all(st)
    yes(st)
    assert ap(st, attacker) == 5 - 3 - 2


# ---------------------------------------------------------------------------------------------
# GD04-067 ∀ Gundam


@pytest.mark.card("GD04-067")
@pytest.mark.ruling("GD04-067:Q275")
def test_gd04_067_copies_every_keyword_of_the_chosen_unit_card() -> None:
    sc = Scenario()
    sc.resources(0, 1)
    turn_a = sc.add(0, "GD04-067")
    sc.trash(0, "GD03-003")
    st = sc.start()
    activate(st, turn_a)
    assert ap(st, turn_a) == 5
    assert keywords(st, turn_a) == {"Blocker": 1, "Repair": 1}
    assert not has_action(st, A.ACTIVATE, turn_a)
    to_next_turn(st)
    assert keywords(st, turn_a) == {}
    assert ap(st, turn_a) == 4


@pytest.mark.card("GD04-067")
@pytest.mark.ruling("GD04-067:Q276")
def test_gd04_067_conditional_keywords_do_not_count() -> None:
    sc = Scenario()
    sc.resources(0, 1)
    turn_a = sc.add(0, "GD04-067")
    sc.trash(0, "GD03-040")
    st = sc.start()
    assert not has_action(st, A.ACTIVATE, turn_a)


@pytest.mark.card("GD04-067")
@pytest.mark.ruling("GD04-067:Q277")
def test_gd04_067_can_choose_from_the_opponent_trash() -> None:
    sc = Scenario()
    sc.resources(0, 1)
    turn_a = sc.add(0, "GD04-067")
    sc.trash(1, "GD04-018")
    st = sc.start()
    activate(st, turn_a)
    assert keywords(st, turn_a) == {"Breach": 5}


# ---------------------------------------------------------------------------------------------
# GD04-068 Silver Bullet


@pytest.mark.card("GD04-068")
@pytest.mark.ruling("GD04-068:Q278")
def test_gd04_068_enemy_effect_damage_reduced_by_3_battle_damage_not() -> None:
    sc = Scenario(active=1)
    sc.resources(1, 4)
    bullet = sc.add(0, "GD04-068", rested=True)
    finger = sc.hand(1, DARKNESS_FINGER)[0]
    attacker = sc.add(1, BIGRO)
    st = sc.start()
    assert keywords(st, bullet) == {"Blocker": 1}
    play(st, finger)
    assert st.cards[bullet].damage == 0
    attack(st, attacker, bullet)
    pass_all(st)
    assert zone_of(st, bullet) is Zone.TRASH


# ---------------------------------------------------------------------------------------------
# GD04-069 ∀ Gundam


@pytest.mark.card("GD04-069", "GD04-073")
def test_gd04_069_end_of_turn_after_paying_for_another_militia_unit() -> None:
    sc = Scenario()
    sc.resources(0, 1)
    turn_a = sc.add(0, "GD04-069", pilot=LORAN)
    other = sc.add(0, "GD04-073")
    kapool = sc.add(0, "GD04-074", rested=True)
    st = sc.start()
    assert keywords(st, turn_a) == {"Blocker": 1}
    activate(st, other)
    assert ap(st, other) == 5
    end_turn(st)
    assert option_uids(st) == {turn_a, other, kapool}
    select(st, kapool)
    assert not st.cards[kapool].rested


@pytest.mark.card("GD04-069")
def test_gd04_069_no_payment_no_trigger() -> None:
    sc = Scenario()
    sc.add(0, "GD04-069", pilot=LORAN)
    kapool = sc.add(0, "GD04-074", rested=True)
    st = sc.start()
    end_turn(st)
    assert st.active == 1 and pending(st) is DecisionKind.MAIN
    assert st.cards[kapool].rested


@pytest.mark.card("GD04-069")
def test_gd04_069_unlinked_does_not_trigger() -> None:
    sc = Scenario()
    sc.resources(0, 1)
    sc.add(0, "GD04-069", pilot=RIDDHE)
    other = sc.add(0, "GD04-073")
    kapool = sc.add(0, "GD04-074", rested=True)
    st = sc.start()
    activate(st, other)
    end_turn(st)
    assert st.active == 1 and pending(st) is DecisionKind.MAIN
    assert st.cards[kapool].rested


# ---------------------------------------------------------------------------------------------
# GD04-070 Al-Saachez's AEU Enact Custom Moralia Development Experiment Type


@pytest.mark.card("GD04-070")
@pytest.mark.ruling("GD04-070:Q279")
def test_gd04_070_pairs_ali_from_hand_for_free() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    enact = sc.hand(0, "GD04-070")[0]
    ali = sc.hand(0, ALI)[0]
    st = sc.start()
    play(st, enact)
    yes(st)
    assert st.cards[enact].pair == ali
    assert active_resources(st) == 0
    assert V.is_linked(V.derived(st), enact)


@pytest.mark.card("GD04-070")
def test_gd04_070_declining_keeps_the_pilot_in_hand() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    enact = sc.hand(0, "GD04-070")[0]
    ali = sc.hand(0, ALI)[0]
    st = sc.start()
    play(st, enact)
    no(st)
    assert zone_of(st, ali) is Zone.HAND


# ---------------------------------------------------------------------------------------------
# GD04-071 Graham's Union Flag Custom Ⅱ (GN Flag)


@pytest.mark.card("GD04-071")
def test_gd04_071_exile_superpower_bloc_and_un_cards_to_stand_but_not_attack() -> None:
    sc = Scenario()
    flag = sc.add(0, "GD04-071", rested=True)
    sb, un = sc.trash(0, "GD04-079", "GD04-118")
    sc.shields(1, VANILLA)
    st = sc.start()
    activate(st, flag)
    assert zone_of(st, sb) is Zone.REMOVAL
    assert zone_of(st, un) is Zone.REMOVAL
    assert not st.cards[flag].rested
    assert not has_action(st, A.ATTACK, flag)


@pytest.mark.card("GD04-071")
def test_gd04_071_needs_two_distinct_cards() -> None:
    sc = Scenario()
    flag = sc.add(0, "GD04-071", rested=True)
    sc.trash(0, ALI)
    st = sc.start()
    assert not has_action(st, A.ACTIVATE, flag)


@pytest.mark.card("GD04-071")
def test_gd04_071_needs_a_un_card() -> None:
    sc = Scenario()
    flag = sc.add(0, "GD04-071", rested=True)
    sc.trash(0, "GD04-079", "GD04-079")
    st = sc.start()
    assert not has_action(st, A.ACTIVATE, flag)


@pytest.mark.card("GD04-071")
def test_gd04_071_card_with_both_traits_leaves_the_other_choice_open() -> None:
    sc = Scenario()
    flag = sc.add(0, "GD04-071", rested=True)
    ali, sb = sc.trash(0, ALI, "GD04-079")
    st = sc.start()
    activate(st, flag)
    assert zone_of(st, ali) is Zone.REMOVAL
    assert zone_of(st, sb) is Zone.REMOVAL


@pytest.mark.card("GD04-071")
@pytest.mark.rule("13-2-5-1")
def test_gd04_071_burst_adds_to_hand_only_with_an_enemy_cb_unit() -> None:
    sc = Scenario(active=1)
    shield = sc.shields(0, "GD04-071")[0]
    attacker = sc.add(1, VIRTUE)
    st = sc.start()
    attack(st, attacker)
    pass_all(st)
    yes(st)
    assert zone_of(st, shield) is Zone.HAND
    sc2 = Scenario(active=1)
    shield2 = sc2.shields(0, "GD04-071")[0]
    attacker2 = sc2.add(1, VANILLA)
    st2 = sc2.start()
    attack(st2, attacker2)
    pass_all(st2)
    yes(st2)
    assert zone_of(st2, shield2) is Zone.TRASH


# ---------------------------------------------------------------------------------------------
# GD04-072 Unicorn Gundam 02 Banshee Norn (Unicorn Mode)


@pytest.mark.card("GD04-072")
def test_gd04_072_when_linked_returns_a_3_hp_or_less_enemy() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    banshee = sc.add(0, "GD04-072")
    riddhe = sc.hand(0, RIDDHE)[0]
    weak = sc.add(1, VANILLA)
    strong = sc.add(1, BIGRO)
    st = sc.start()
    play(st, riddhe, onto=banshee)
    assert zone_of(st, weak) is Zone.HAND
    assert zone_of(st, strong) is Zone.BATTLE


# ---------------------------------------------------------------------------------------------
# GD04-073 ∀ Gundam


@pytest.mark.card("GD04-073")
def test_gd04_073_pay_1_for_ap_plus_2_once_per_turn() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    turn_a = sc.add(0, "GD04-073")
    st = sc.start()
    activate(st, turn_a)
    assert ap(st, turn_a) == 5
    assert active_resources(st) == 1
    assert not has_action(st, A.ACTIVATE, turn_a)


# ---------------------------------------------------------------------------------------------
# GD04-074 Kapool


@pytest.mark.card("GD04-074")
def test_gd04_074_pay_1_to_draw_then_discard() -> None:
    sc = Scenario()
    sc.resources(0, 1)
    kapool = sc.add(0, "GD04-074")
    sc.hand(0, BIGRO)
    sc.shields(1, VANILLA)
    st = sc.start()
    attack(st, kapool)
    yes(st)
    assert active_resources(st) == 0
    assert pending(st) is DecisionKind.DISCARD
    select(st, st.zones[0][Zone.HAND][0])
    assert hand_size(st) == 1
    assert len(st.zones[0][Zone.TRASH]) == 1


@pytest.mark.card("GD04-074")
@pytest.mark.rule("5-20-1", "5-20-2")
def test_gd04_074_not_paying_skips_the_discard() -> None:
    sc = Scenario()
    sc.resources(0, 1)
    kapool = sc.add(0, "GD04-074")
    sc.hand(0, BIGRO)
    sc.shields(1, VANILLA)
    st = sc.start()
    attack(st, kapool)
    no(st)
    assert hand_size(st) == 1
    assert st.zones[0][Zone.TRASH] == []
    assert active_resources(st) == 1
