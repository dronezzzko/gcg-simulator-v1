"""WP-TOKENS: Unit token cards T-001..T-029 (rule 5-17)."""

from __future__ import annotations

import pytest

from gcg_sim.cards.tokens import parse_token_specs
from gcg_sim.effects.registry import get_registry
from gcg_sim.engine.state import GameState
from gcg_sim.engine.types import PLAYER_TARGET, ActionKind, DecisionKind, Zone
from gcg_sim.testkit import (
    Scenario,
    ap,
    attack,
    block,
    has_action,
    hp,
    keywords,
    options,
    pass_all,
    play,
    to_next_turn,
    zone_of,
)

A = ActionKind
VANILLA_2_2 = "GD01-060"  # Zaku Mariner, Lv2 cost1 AP2/HP2, no effects
PILOT = "GD01-089"  # Riddhe Marcenas, Lv3 cost1 Pilot (AP+1/HP+1)
PILOT_COMMAND = "EB01-084"  # Lv4 cost1 Command with 【Pilot】[Demeziere Sonnen] (rule 3-4-6-2)
SET_ACTIVE_ON_DEPLOY = "EB01-005"  # Lv7 cost6: 【Deploy】Choose 1 rested Unit ... Set it as active.
ZAKU_TOKEN = "T-007"  # vanilla Unit token (Zeon) AP1/HP1, the unrestricted control

AD_BALLOON = "T-014"
PARTS = "T-021"
WIRE_GUIDED_ARM = "T-022"
GUNDNODE = "T-026"
BIT_FUNNEL = "T-029"


def _attack_targets(st: GameState, uid: int) -> set[int]:
    return {o.b for o in options(st) if o.kind is A.ATTACK and o.a == uid}


def _pair_units(st: GameState, pilot: int) -> set[int]:
    return {o.b for o in options(st) if o.kind is A.PAIR and o.a == pilot}


# ---------------------------------------------------------------------------------------------
# token definitions embedded in creating cards resolve to these token cards


@pytest.mark.card("T-008", "T-009", "T-010", "T-011", "T-014", "T-021", "T-022", "T-026", "T-029")
@pytest.mark.rule("5-17-1")
@pytest.mark.parametrize(
    ("creator", "name", "token"),
    [
        ("GD03-020", "Ad Balloon", AD_BALLOON),
        ("ST13-002", "Bit / Funnel", BIT_FUNNEL),
        ("GD04-017", "Wire-Guided Arm", WIRE_GUIDED_ARM),
        ("GD04-011", "Parts", PARTS),
        ("ST04-012", "Aile Strike Gundam", "T-008"),
        ("ST04-012", "Launcher Strike Gundam", "T-009"),
        ("ST04-012", "Sword Strike Gundam", "T-010"),
        ("GD01-066", "Fatum-00", "T-011"),
        ("GD05-126", "Gundnode", GUNDNODE),
    ],
)
def test_inline_token_definition_deploys_the_token_card(
    creator: str, name: str, token: str
) -> None:
    reg = get_registry()
    (spec,) = [s for s in parse_token_specs(reg.db[creator].effect) if s.name == name]
    tdef = reg.db.token_for(spec)
    assert tdef.card_number == token
    assert (tdef.ap, tdef.hp) == (spec.ap, spec.hp)
    entry = reg.cards[tdef.def_id]
    assert entry.script is not None and entry.script.abilities


@pytest.mark.card("T-001", "T-002", "T-003", "T-006", "T-013")
@pytest.mark.rule("5-17-2-1")
@pytest.mark.parametrize(
    ("creator", "name", "token", "stats", "traits"),
    [
        ("ST01-015", "Gundam", "T-001", (3, 3), ("White Base Team",)),
        ("ST01-015", "Guncannon", "T-002", (2, 2), ("White Base Team",)),
        ("ST01-015", "Guntank", "T-003", (1, 1), ("White Base Team",)),
        ("GD01-026", "Char's Zaku Ⅱ", "T-006", (3, 1), ("Zeon",)),
        ("GD03-024", "Hy-Gogg", "T-013", (2, 1), ("Cyclops Team",)),
    ],
)
def test_divergent_token_printings_use_the_creating_effect_stats(
    creator: str, name: str, token: str, stats: tuple[int, int], traits: tuple[str, ...]
) -> None:
    reg = get_registry()
    (spec,) = [s for s in parse_token_specs(reg.db[creator].effect) if s.name == name]
    assert ((spec.ap, spec.hp), spec.traits) == (stats, traits)
    assert reg.db.token_for(spec).card_number == token
    sc = Scenario()
    uid = sc.add(0, token)
    st = sc.start()
    assert (ap(st, uid), hp(st, uid)) == stats
    assert reg.db[token].traits == traits


# ---------------------------------------------------------------------------------------------
# T-014 Ad Balloon: "This Unit can't be set as active or paired with a Pilot."


@pytest.mark.card("T-014")
@pytest.mark.rule("7-2-3-1", "10-1-5-6")
def test_t014_stays_rested_through_its_controllers_active_step() -> None:
    sc = Scenario(active=1)
    balloon = sc.add(0, AD_BALLOON, rested=True)
    control = sc.add(0, ZAKU_TOKEN, rested=True)
    st = sc.start()
    to_next_turn(st)
    assert st.active == 0
    assert st.cards[balloon].rested
    assert not st.cards[control].rested


@pytest.mark.card("T-014")
@pytest.mark.rule("10-1-5-6")
def test_t014_effect_cannot_set_it_active() -> None:
    sc = Scenario()
    sc.resources(0, 7)
    zeta = sc.add(0, SET_ACTIVE_ON_DEPLOY, Zone.HAND)
    balloon = sc.add(1, AD_BALLOON, rested=True)
    st = sc.start()
    hand_before = len(st.zones[0][Zone.HAND])
    play(st, zeta)
    assert zone_of(st, zeta) is Zone.BATTLE
    assert st.cards[balloon].rested
    assert len(st.zones[0][Zone.HAND]) == hand_before  # played 1, drew 1


@pytest.mark.card("T-014")
def test_t014_control_unrestricted_token_is_set_active_by_the_same_effect() -> None:
    sc = Scenario()
    sc.resources(0, 7)
    zeta = sc.add(0, SET_ACTIVE_ON_DEPLOY, Zone.HAND)
    zaku = sc.add(1, ZAKU_TOKEN, rested=True)
    st = sc.start()
    play(st, zeta)
    assert not st.cards[zaku].rested


@pytest.mark.card("T-014")
@pytest.mark.rule("5-9-1", "3-4-6-2", "10-1-5-6")
def test_t014_cannot_be_paired_with_a_pilot_or_pilot_command() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    sc.add(0, AD_BALLOON, rested=True)
    unit = sc.add(0, VANILLA_2_2)
    pilot = sc.add(0, PILOT, Zone.HAND)
    command = sc.add(0, PILOT_COMMAND, Zone.HAND)
    st = sc.start()
    assert _pair_units(st, pilot) == {unit}
    assert _pair_units(st, command) == {unit}


@pytest.mark.card("T-007")
@pytest.mark.rule("5-17-2-2")
@pytest.mark.faq("Q103")
def test_unrestricted_unit_token_can_be_paired() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    zaku = sc.add(0, ZAKU_TOKEN, rested=True)
    pilot = sc.add(0, PILOT, Zone.HAND)
    st = sc.start()
    play(st, pilot, onto=zaku)
    assert st.cards[zaku].pair == pilot
    assert (ap(st, zaku), hp(st, zaku)) == (2, 2)


# ---------------------------------------------------------------------------------------------
# T-029 Bit / Funnel: "This Unit can't be paired with a Pilot or attack."


@pytest.mark.card("T-029")
@pytest.mark.rule("8-2-1", "10-1-5-6")
def test_t029_cannot_attack() -> None:
    sc = Scenario()
    bit = sc.add(0, BIT_FUNNEL)
    control = sc.add(0, ZAKU_TOKEN)
    enemy = sc.add(1, VANILLA_2_2, rested=True)
    sc.shields(1, VANILLA_2_2)
    st = sc.start()
    assert not has_action(st, A.ATTACK, bit)
    assert _attack_targets(st, control) == {PLAYER_TARGET, enemy}


@pytest.mark.card("T-029")
@pytest.mark.rule("5-9-1", "3-4-6-2")
def test_t029_cannot_be_paired_with_a_pilot_or_pilot_command() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    sc.add(0, BIT_FUNNEL)
    unit = sc.add(0, VANILLA_2_2)
    pilot = sc.add(0, PILOT, Zone.HAND)
    command = sc.add(0, PILOT_COMMAND, Zone.HAND)
    st = sc.start()
    assert _pair_units(st, pilot) == {unit}
    assert _pair_units(st, command) == {unit}


@pytest.mark.card("T-029")
@pytest.mark.rule("5-17-2-1", "5-17-2-5")
def test_t029_can_be_attacked_and_leaves_the_game_when_destroyed() -> None:
    sc = Scenario(active=1)
    bit = sc.add(0, BIT_FUNNEL, rested=True)
    attacker = sc.add(1, VANILLA_2_2)
    st = sc.start()
    attack(st, attacker, bit)
    pass_all(st)
    assert zone_of(st, bit) is Zone.OUTSIDE
    assert bit not in st.zones[0][Zone.TRASH]
    assert zone_of(st, attacker) is Zone.TRASH  # the Bit dealt its 2 AP as battle damage


# ---------------------------------------------------------------------------------------------
# T-022 Wire-Guided Arm: "This Unit can't be paired with a Pilot."


@pytest.mark.card("T-022")
@pytest.mark.rule("5-9-1", "3-4-6-2")
def test_t022_cannot_be_paired_with_a_pilot_or_pilot_command() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    sc.add(0, WIRE_GUIDED_ARM)
    unit = sc.add(0, VANILLA_2_2)
    pilot = sc.add(0, PILOT, Zone.HAND)
    command = sc.add(0, PILOT_COMMAND, Zone.HAND)
    st = sc.start()
    assert _pair_units(st, pilot) == {unit}
    assert _pair_units(st, command) == {unit}


@pytest.mark.card("T-022")
@pytest.mark.rule("8-2-1")
def test_t022_can_still_attack_the_enemy_player() -> None:
    sc = Scenario()
    arm = sc.add(0, WIRE_GUIDED_ARM)
    (shield,) = sc.shields(1, VANILLA_2_2)
    st = sc.start()
    attack(st, arm, PLAYER_TARGET)
    pass_all(st)
    assert zone_of(st, shield) is Zone.TRASH


# ---------------------------------------------------------------------------------------------
# T-021 Parts: "This Unit can't choose the enemy player as its attack target."


@pytest.mark.card("T-021")
@pytest.mark.rule("8-2-1")
def test_t021_can_only_attack_rested_enemy_units() -> None:
    sc = Scenario()
    parts = sc.add(0, PARTS)
    enemy = sc.add(1, VANILLA_2_2, rested=True)
    sc.shields(1, VANILLA_2_2)
    st = sc.start()
    assert _attack_targets(st, parts) == {enemy}
    attack(st, parts, enemy)
    pass_all(st)
    assert st.cards[enemy].damage == 1
    assert zone_of(st, parts) is Zone.OUTSIDE  # 1 HP token destroyed, removed (5-17-2-5)


@pytest.mark.card("T-021")
@pytest.mark.rule("8-2-1")
def test_t021_has_no_attack_without_a_rested_enemy_unit() -> None:
    sc = Scenario()
    parts = sc.add(0, PARTS)
    control = sc.add(0, ZAKU_TOKEN)
    sc.add(1, VANILLA_2_2)
    sc.shields(1, VANILLA_2_2)
    st = sc.start()
    assert not has_action(st, A.ATTACK, parts)
    assert _attack_targets(st, control) == {PLAYER_TARGET}


# ---------------------------------------------------------------------------------------------
# T-008..T-011 <Blocker> tokens


def _block_with(token: str) -> tuple[GameState, int, int]:
    sc = Scenario(active=1)
    blocker = sc.add(0, token)
    sc.shields(0, VANILLA_2_2)
    attacker = sc.add(1, VANILLA_2_2)
    st = sc.start()
    assert keywords(st, blocker) == {"Blocker": 1}
    attack(st, attacker, PLAYER_TARGET)
    assert st.pending is not None and st.pending.kind is DecisionKind.BLOCK
    assert has_action(st, A.BLOCK, blocker)
    block(st, blocker)
    pass_all(st)
    assert len(st.zones[0][Zone.SHIELD]) == 1
    return st, blocker, attacker


@pytest.mark.card("T-008")
@pytest.mark.rule("13-1-4-1")
def test_t008_aile_strike_blocks() -> None:
    st, blocker, attacker = _block_with("T-008")
    assert st.cards[blocker].rested
    assert st.cards[blocker].damage == 2
    assert zone_of(st, attacker) is Zone.TRASH


@pytest.mark.card("T-009")
@pytest.mark.rule("13-1-4-1")
def test_t009_launcher_strike_blocks() -> None:
    st, blocker, attacker = _block_with("T-009")
    assert st.cards[blocker].rested
    assert st.cards[blocker].damage == 2
    assert zone_of(st, attacker) is Zone.TRASH


@pytest.mark.card("T-010")
@pytest.mark.rule("13-1-4-1", "5-17-2-5")
def test_t010_sword_strike_blocks() -> None:
    st, blocker, attacker = _block_with("T-010")
    assert zone_of(st, blocker) is Zone.OUTSIDE
    assert zone_of(st, attacker) is Zone.TRASH


@pytest.mark.card("T-011")
@pytest.mark.rule("13-1-4-1", "5-17-2-5")
def test_t011_fatum_blocks() -> None:
    st, blocker, attacker = _block_with("T-011")
    assert zone_of(st, blocker) is Zone.OUTSIDE
    assert zone_of(st, attacker) is Zone.TRASH


@pytest.mark.card("T-008")
@pytest.mark.rule("13-1-4-1")
@pytest.mark.faq("Q59")
def test_blocker_token_cannot_block_while_rested() -> None:
    sc = Scenario(active=1)
    blocker = sc.add(0, "T-008", rested=True)
    (shield,) = sc.shields(0, VANILLA_2_2)
    attacker = sc.add(1, VANILLA_2_2)
    st = sc.start()
    attack(st, attacker, PLAYER_TARGET)
    assert not has_action(st, A.BLOCK, blocker)
    pass_all(st)
    assert zone_of(st, shield) is Zone.TRASH


# ---------------------------------------------------------------------------------------------
# T-026 Gundnode <Breach 1>


@pytest.mark.card("T-026")
@pytest.mark.rule("13-1-2-1", "13-1-2-2")
def test_t026_breach_damages_top_shield_on_your_turn() -> None:
    sc = Scenario()
    gundnode = sc.add(0, GUNDNODE)
    victim = sc.add(1, "T-012", rested=True)  # Daughtress AP0/HP1
    top, second = sc.shields(1, VANILLA_2_2, VANILLA_2_2)
    st = sc.start()
    assert keywords(st, gundnode) == {"Breach": 1}
    attack(st, gundnode, victim)
    pass_all(st)
    assert zone_of(st, victim) is Zone.OUTSIDE
    assert zone_of(st, top) is Zone.TRASH
    assert zone_of(st, second) is Zone.SHIELD


@pytest.mark.card("T-026")
@pytest.mark.rule("13-1-2-2")
@pytest.mark.faq("Q53")
def test_t026_breach_damages_the_base_first() -> None:
    sc = Scenario()
    gundnode = sc.add(0, GUNDNODE)
    victim = sc.add(1, "T-012", rested=True)
    base = sc.base(1)
    (shield,) = sc.shields(1, VANILLA_2_2)
    st = sc.start()
    attack(st, gundnode, victim)
    pass_all(st)
    assert st.cards[base].damage == 1
    assert zone_of(st, shield) is Zone.SHIELD


@pytest.mark.card("T-026")
@pytest.mark.rule("13-1-2-1")
def test_t026_no_breach_during_opponents_turn() -> None:
    sc = Scenario(active=1)
    gundnode = sc.add(0, GUNDNODE, rested=True)
    attacker = sc.add(1, ZAKU_TOKEN)
    (shield,) = sc.shields(1, VANILLA_2_2)
    st = sc.start()
    attack(st, attacker, gundnode)
    pass_all(st)
    assert zone_of(st, attacker) is Zone.OUTSIDE
    assert st.cards[gundnode].damage == 1
    assert zone_of(st, shield) is Zone.SHIELD


@pytest.mark.card("T-026")
@pytest.mark.rule("13-1-2-3")
@pytest.mark.faq("Q55")
def test_t026_breach_still_activates_when_both_units_are_destroyed() -> None:
    sc = Scenario()
    gundnode = sc.add(0, GUNDNODE)
    victim = sc.add(1, VANILLA_2_2, rested=True)
    top, second = sc.shields(1, VANILLA_2_2, VANILLA_2_2)
    st = sc.start()
    attack(st, gundnode, victim)
    pass_all(st)
    assert zone_of(st, victim) is Zone.TRASH
    assert zone_of(st, gundnode) is Zone.OUTSIDE
    assert zone_of(st, top) is Zone.TRASH
    assert zone_of(st, second) is Zone.SHIELD
