"""Damage, HP recovery, counters, reduction (5-5, 5-6, 5-18, 5-21), tokens (2-9-3, 2-10-3, 5-17)
and rules management (11)."""

from __future__ import annotations

import pytest

from gcg_sim.cards.model import CardType
from gcg_sim.cards.tokens import parse_token_specs
from gcg_sim.effects import dsl as d
from gcg_sim.effects.registry import get_registry
from gcg_sim.engine import interp as I
from gcg_sim.engine import view as V
from gcg_sim.engine.game import new_game
from gcg_sim.engine.observe import information_set_key
from gcg_sim.engine.state import GameState
from gcg_sim.engine.types import (
    NO_ARG,
    PLAYER_TARGET,
    ActionKind,
    DecisionKind,
    EndReason,
    Step,
    Zone,
)
from gcg_sim.testkit import (
    Scenario,
    act,
    activate,
    ap,
    attack,
    has_action,
    hp,
    keywords,
    options,
    pass_all,
    play,
    select,
    select_if_asked,
    to_next_turn,
    uids_in,
    vanilla_deck,
    yes_if_asked,
    zone_of,
)

A = ActionKind

VANILLA_2_2 = "GD01-060"  # Zaku Mariner, red Lv2 cost1 2/2
PISCES = "GD01-021"  # blue Lv1 cost1 1/2, no effects
CANCER = "GD01-022"  # blue Lv2 cost2 2/3, no effects
MARASAI = "GD02-015"  # blue Lv3 cost2 3/3, no effects
GUNDAM_3_4 = "GD01-013"  # blue Lv4 cost2 3/4, no effects
KSHATRIYA = "GD01-051"  # red Lv4 cost2 3/4, no effects
HAZEL_RAH = "EB01-007"  # blue Lv5 cost3 5/4, no effects
ZAKU_III = "GD02-048"  # red Lv3 cost2 4/1, no effects
AGE_1 = "GD02-029"  # (AGE System) 3/3, no effects
BEGUIR_PENTE = "GD01-084"  # (Academy) 2/3, no effects
ZUOOT = "GD01-061"  # 0 AP / 2 HP, only an activated <Support>
PILOT = "GD01-089"  # Riddhe Marcenas, Pilot +1/+1

ZEON_REMNANT = "GD01-115"  # 【Main】/【Action】Choose 1 enemy Unit. Deal 1 damage to it.
CLOSE_COMBAT = "ST03-013"  # 【Main】/【Action】Choose 1 enemy Unit. Deal 2 damage to it.
OVERWHELMING = "GD04-109"  # Choose 1 enemy Unit that is Lv.6 or lower. Deal 4 damage to it.
BATTLE_OF_ACES = "GD01-111"  # Choose 1 damaged enemy Unit. Deal 3 damage to it.
OVER_THE_RIVER = "GD03-107"  # damage equal to the number of friendly Unit tokens
INSPECTOR = "GD04-112"  # Deal 1 damage to all Units that are Lv.2 or lower.
FATAL_STRIKE = "ST05-014"  # Choose 1 enemy Unit that is Lv.3 or lower. Destroy it.
BOUNCE = "GD05-105"  # Choose 1 enemy Unit that is Lv.3 or lower. Return it to hand.
FINANCIER = "GD04-110"  # Deploy 1 EX Base.
AGE_DEVICE = "GD02-103"  # If you have an (AGE System) Unit in play, place 1 EX Resource.
SECURING = "GD01-102"  # All friendly Units that are Lv.4 or lower recover 2 HP.
DOMINION = "GD02-121"  # Base; 【Deploy】... choose 1 friendly blue Unit. It recovers 2 HP.
SIDE_7 = "GD01-124"  # Base; 【Activate･Main】Rest this Base: 1 friendly Unit recovers 1 HP.
ARMORY_ONE = "GD04-128"  # Base 0/6; 【Destroyed】All players draw 1.

RAIDER = "GD02-010"  # 【Once per Turn】When this Unit receives enemy effect damage, draw 1.
PHARACT = "GD04-018"  # another (Academy) Unit receives damage from an enemy: EX Resource
SILVER_BULLET = "GD04-068"  # 4/4; enemy effect damage to it is reduced by 3
DESTINY = "GD05-055"  # 5/6; 【Once per Turn】enemy battle damage to it is reduced by 2
LUPUS_REX = "EB01-004"  # during your turn, when this Unit recovers HP: 1 damage to rested enemy
MASTER_GUNDAM = "GD05-033"  # 【Attack】exile 2 (Special Move) Commands: 5 damage to shield area
DARKNESS_FINGER = "GD05-110"  # a (Special Move) Command card
ALPHA_AZIERU = "GD05-054"  # When one of your Units is destroyed by an effect, draw 1.
DUEL_ASSAULT = "ST14-009"  # 【Destroyed】Place 1 EX Resource.
NT1 = "GD03-001"  # 【When Paired】1 damage to a rested enemy; draw if it destroyed it
WHITE_BASE_GUNDAM = "GD01-001"  # All your (White Base Team) Units gain <Repair 1>.
TOKEN_ZAKU_DEPLOYER = "GD01-106"  # 【Main】Deploy 2 [Zaku Ⅱ]((Zeon)･AP1･HP1) Unit tokens.

GUNDAM_TOKEN = "T-001"  # Unit token [Gundam] (White Base Team) 3/3
ZAKU_TOKEN = "T-007"  # Unit token [Zaku Ⅱ] (Zeon) 1/1


def _hand_size(st: GameState, player: int) -> int:
    return len(st.zones[player][Zone.HAND])


def _ex_resources(st: GameState, player: int) -> list[int]:
    return [
        u
        for u in st.zones[player][Zone.RESOURCE_AREA]
        if V.cdef(st, u).card_type is CardType.EX_RESOURCE
    ]


def _history_kinds(st: GameState) -> list[str]:
    return [h.kind for h in st.history]


def _left_the_game(st: GameState, uid: int) -> bool:
    c = st.cards[uid]
    return c.zone is Zone.OUTSIDE and all(uid not in z for pz in st.zones for z in pz)


def _queue_effect(sc: Scenario, controller: int, *steps: d.Step) -> None:
    """Queue an effect (as compiled DSL steps) to resolve as soon as the scenario starts."""
    pid = get_registry().program(tuple(steps), f"test_damage:{steps!r}")
    I.push_frame(sc.st, pid, controller=controller, host=NO_ARG, kind="system")


def _deploy_two_zaku_tokens() -> d.DeployToken:
    """GD01-106's 【Main】 "Deploy 2 [Zaku Ⅱ]((Zeon)･AP1･HP1) Unit tokens." as a DSL step."""
    spec = parse_token_specs(get_registry().db[TOKEN_ZAKU_DEPLOYER].effect)[0]
    return d.DeployToken(token_key=spec.key, count=2)


# ---------------------------------------------------------------------------------------------
# 5-5 / 5-18: damage and damage counters


@pytest.mark.rule("5-5-1", "5-5-1-1", "5-5-4", "5-18-1")
def test_effect_damage_places_counters_that_accumulate_and_persist() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    target = sc.add(1, GUNDAM_3_4)
    close_combat, zeon_remnant = sc.hand(0, CLOSE_COMBAT, ZEON_REMNANT)
    st = sc.start()
    play(st, close_combat)
    assert st.cards[target].damage == 2
    play(st, zeon_remnant)
    assert st.cards[target].damage == 3
    assert zone_of(st, target) is Zone.BATTLE
    to_next_turn(st)
    to_next_turn(st)
    assert st.cards[target].damage == 3


@pytest.mark.rule("5-5-1", "5-5-3", "5-18-1-1", "5-17-3-1-1")
def test_battle_damage_to_a_base_is_shown_with_counters_on_the_base() -> None:
    sc = Scenario()
    attacker = sc.add(0, VANILLA_2_2)
    ex_base = sc.base(1)
    shields = sc.shields(1, VANILLA_2_2, VANILLA_2_2)
    st = sc.start()
    attack(st, attacker)
    pass_all(st)
    assert st.cards[ex_base].damage == 2
    assert zone_of(st, ex_base) is Zone.BASE
    assert uids_in(st, 1, Zone.SHIELD) == shields
    assert st.cards[attacker].damage == 0


@pytest.mark.rule("5-5-2", "11-1-1", "11-3-1")
def test_damage_equal_to_hp_destroys_through_rules_management() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    zaku = sc.add(1, VANILLA_2_2)
    cancer = sc.add(1, CANCER)
    close_combat, zeon_remnant = sc.hand(0, CLOSE_COMBAT, ZEON_REMNANT)
    st = sc.start()
    play(st, close_combat)
    select(st, zaku)
    assert zone_of(st, zaku) is Zone.TRASH
    assert st.pending is not None and st.pending.kind is DecisionKind.MAIN
    assert st.pending.player == 0
    play(st, zeon_remnant)
    assert zone_of(st, cancer) is Zone.BATTLE
    assert st.cards[cancer].damage == 1


@pytest.mark.rule("5-5-2", "11-3-1")
def test_existing_damage_plus_new_damage_reaching_hp_destroys() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    target = sc.add(1, KSHATRIYA, damage=2)
    close_combat = sc.add(0, CLOSE_COMBAT, Zone.HAND)
    st = sc.start()
    play(st, close_combat)
    assert zone_of(st, target) is Zone.TRASH


@pytest.mark.rule("5-5-3")
def test_battling_units_deal_damage_equal_to_their_ap_to_each_other_in_the_damage_step() -> None:
    sc = Scenario()
    attacker = sc.add(0, KSHATRIYA)
    target = sc.add(1, CANCER, rested=True)
    sc.resources(1, 2)
    sc.add(1, ZEON_REMNANT, Zone.HAND)
    st = sc.start()
    attack(st, attacker, target)
    assert st.pending is not None and st.pending.kind is DecisionKind.ACTION_STEP
    assert st.cards[attacker].damage == 0
    assert st.cards[target].damage == 0
    pass_all(st)
    assert zone_of(st, target) is Zone.TRASH
    assert st.cards[attacker].damage == 2


@pytest.mark.rule("5-5-4", "5-5-6", "11-3-1")
def test_effect_damage_to_the_base_is_not_carried_over_to_shields() -> None:
    sc = Scenario()
    master = sc.add(0, MASTER_GUNDAM)
    sc.trash(0, DARKNESS_FINGER, DARKNESS_FINGER)
    ex_base = sc.base(1)
    shields = sc.shields(1, VANILLA_2_2, VANILLA_2_2)
    sc.resources(1, 2)
    sc.add(1, ZEON_REMNANT, Zone.HAND)
    st = sc.start()
    attack(st, master)
    specials = uids_in(st, 0, Zone.TRASH)
    yes_if_asked(st)
    select_if_asked(st, *specials, done=False)
    assert st.pending is not None and st.pending.kind is DecisionKind.ACTION_STEP
    assert _left_the_game(st, ex_base)
    assert uids_in(st, 1, Zone.SHIELD) == shields
    pass_all(st)
    assert zone_of(st, shields[0]) is Zone.TRASH
    assert uids_in(st, 1, Zone.SHIELD) == shields[1:]


@pytest.mark.rule("5-5-4", "5-5-6", "11-3-1-1")
def test_effect_damage_to_a_shield_destroys_only_that_shield() -> None:
    sc = Scenario()
    master = sc.add(0, MASTER_GUNDAM)
    sc.trash(0, DARKNESS_FINGER, DARKNESS_FINGER)
    shields = sc.shields(1, VANILLA_2_2, VANILLA_2_2, VANILLA_2_2)
    sc.resources(1, 2)
    sc.add(1, ZEON_REMNANT, Zone.HAND)
    st = sc.start()
    attack(st, master)
    yes_if_asked(st)
    select_if_asked(st, *uids_in(st, 0, Zone.TRASH), done=False)
    assert st.pending is not None and st.pending.kind is DecisionKind.ACTION_STEP
    assert zone_of(st, shields[0]) is Zone.TRASH
    assert uids_in(st, 1, Zone.SHIELD) == shields[1:]


@pytest.mark.rule("5-5-5")
def test_zero_ap_attack_on_a_player_with_empty_shield_area_deals_no_damage() -> None:
    sc = Scenario()
    zuoot = sc.add(0, ZUOOT)
    st = sc.start()
    attack(st, zuoot, PLAYER_TARGET)
    pass_all(st)
    assert st.winner is None
    assert st.pending is not None and st.pending.kind is DecisionKind.MAIN


@pytest.mark.rule("5-5-5", "11-3-1-1")
def test_zero_ap_attack_does_not_destroy_a_shield() -> None:
    sc = Scenario()
    zuoot = sc.add(0, ZUOOT)
    (shield,) = sc.shields(1, VANILLA_2_2)
    st = sc.start()
    attack(st, zuoot)
    pass_all(st)
    assert zone_of(st, shield) is Zone.SHIELD


@pytest.mark.rule("5-5-5")
def test_zero_ap_attack_places_no_counter_on_a_base() -> None:
    sc = Scenario()
    zuoot = sc.add(0, ZUOOT)
    ex_base = sc.base(1)
    st = sc.start()
    attack(st, zuoot)
    pass_all(st)
    assert st.cards[ex_base].damage == 0
    assert zone_of(st, ex_base) is Zone.BASE


@pytest.mark.rule("5-5-5", "5-5-3")
@pytest.mark.parametrize(("defender", "receives"), [(ZUOOT, False), (CANCER, True)])
def test_zero_battle_damage_is_not_received(defender: str, receives: bool) -> None:
    sc = Scenario()
    sc.add(0, PHARACT)
    academy = sc.add(0, BEGUIR_PENTE)
    target = sc.add(1, defender, rested=True)
    st = sc.start()
    attack(st, academy, target)
    pass_all(st)
    assert st.cards[academy].damage == (2 if receives else 0)
    assert len(_ex_resources(st, 0)) == (1 if receives else 0)


@pytest.mark.rule("5-5-5", "5-5-4")
@pytest.mark.parametrize("tokens", [0, 1])
def test_zero_effect_damage_is_not_dealt(tokens: int) -> None:
    sc = Scenario()
    sc.resources(0, 4)
    for _ in range(tokens):
        sc.add(0, ZAKU_TOKEN)
    raider = sc.add(1, RAIDER)
    command = sc.add(0, OVER_THE_RIVER, Zone.HAND)
    st = sc.start()
    hand_before = _hand_size(st, 1)
    play(st, command)
    assert st.cards[raider].damage == tokens
    assert _hand_size(st, 1) == hand_before + tokens


@pytest.mark.rule("5-5-6", "5-5-3", "11-3-1")
def test_excess_battle_damage_to_a_base_is_not_dealt_to_a_shield() -> None:
    sc = Scenario()
    attacker = sc.add(0, HAZEL_RAH)
    ex_base = sc.base(1)
    shields = sc.shields(1, VANILLA_2_2, VANILLA_2_2)
    st = sc.start()
    attack(st, attacker)
    pass_all(st)
    assert _left_the_game(st, ex_base)
    assert uids_in(st, 1, Zone.SHIELD) == shields


@pytest.mark.rule("5-5-6", "5-5-3", "11-3-1-1")
def test_excess_battle_damage_to_a_shield_is_not_dealt_to_the_next_shield() -> None:
    sc = Scenario()
    attacker = sc.add(0, HAZEL_RAH)
    shields = sc.shields(1, VANILLA_2_2, VANILLA_2_2)
    st = sc.start()
    attack(st, attacker)
    pass_all(st)
    assert zone_of(st, shields[0]) is Zone.TRASH
    assert uids_in(st, 1, Zone.SHIELD) == shields[1:]


# ---------------------------------------------------------------------------------------------
# 5-6 / 5-18: HP recovery removes damage counters


@pytest.mark.rule("5-6-1", "5-18-1", "5-18-3")
def test_recovery_removes_that_many_damage_counters() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    unit = sc.add(0, GUNDAM_3_4, damage=3)
    command = sc.add(0, SECURING, Zone.HAND)
    st = sc.start()
    play(st, command)
    assert st.cards[unit].damage == 1


@pytest.mark.rule("5-6-2")
def test_recovery_beyond_the_damage_removes_every_counter_without_raising_hp() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    unit = sc.add(0, CANCER, damage=1)
    command = sc.add(0, SECURING, Zone.HAND)
    st = sc.start()
    play(st, command)
    assert st.cards[unit].damage == 0
    assert hp(st, unit) == 3


@pytest.mark.rule("5-6-3")
@pytest.mark.parametrize("damage", [0, 1])
def test_a_unit_without_damage_cannot_recover_hp(damage: int) -> None:
    sc = Scenario()
    rex = sc.add(0, LUPUS_REX, damage=damage)
    side_7 = sc.base(0, SIDE_7)
    enemy = sc.add(1, CANCER, rested=True)
    st = sc.start()
    activate(st, side_7)
    assert st.cards[rex].damage == 0
    assert st.cards[enemy].damage == damage


# ---------------------------------------------------------------------------------------------
# 5-21: reducing damage


@pytest.mark.rule("5-21-1")
def test_effect_damage_is_reduced_before_it_is_received() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    bullet = sc.add(1, SILVER_BULLET)
    command = sc.add(0, OVERWHELMING, Zone.HAND)
    st = sc.start()
    play(st, command)
    assert zone_of(st, bullet) is Zone.BATTLE
    assert st.cards[bullet].damage == 1


@pytest.mark.rule("5-21-1")
def test_battle_damage_is_reduced_before_it_is_received() -> None:
    sc = Scenario()
    attacker = sc.add(0, HAZEL_RAH)
    destiny = sc.add(1, DESTINY, rested=True)
    st = sc.start()
    attack(st, attacker, destiny)
    pass_all(st)
    assert st.cards[destiny].damage == 3
    assert zone_of(st, attacker) is Zone.TRASH


@pytest.mark.rule("5-21-2", "5-21-2-1")
def test_damage_reduced_to_zero_is_neither_dealt_nor_received() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    bullet = sc.add(1, SILVER_BULLET)
    close_combat, battle_of_aces = sc.hand(0, CLOSE_COMBAT, BATTLE_OF_ACES)
    st = sc.start()
    play(st, close_combat)
    assert st.cards[bullet].damage == 0
    assert not has_action(st, A.PLAY_COMMAND, battle_of_aces)


# ---------------------------------------------------------------------------------------------
# 2-9-3, 2-10-3, 5-17: tokens


@pytest.mark.rule("2-9-3", "5-17-2-4")
def test_a_token_is_treated_as_level_zero() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    token = sc.add(1, GUNDAM_TOKEN)
    lv2 = sc.add(1, VANILLA_2_2)
    lv3 = sc.add(1, MARASAI)
    command = sc.add(0, INSPECTOR, Zone.HAND)
    st = sc.start()
    assert V.level_of(st, token) == 0
    play(st, command)
    assert st.cards[token].damage == 1
    assert st.cards[lv2].damage == 1
    assert st.cards[lv3].damage == 0


@pytest.mark.rule("2-10-3", "5-17-2-4")
def test_a_token_is_treated_as_cost_zero() -> None:
    sc = Scenario()
    unit_token = sc.add(0, GUNDAM_TOKEN)
    ex_base = sc.base(0)
    (ex_resource,) = sc.resources(0, 0, ex=1)
    printed = sc.add(0, VANILLA_2_2)
    st = sc.start()
    dv = V.derived(st)
    cost_zero = (d.StatCmp(d.Stat.COST, d.Op.EQ, 0),)
    for tok in (unit_token, ex_base, ex_resource):
        assert V.cost_of(st, tok) == 0
        assert V.matches(st, dv, V.Ctx(0), tok, cost_zero)
    assert not V.matches(st, dv, V.Ctx(0), printed, cost_zero)


@pytest.mark.rule("5-17-2-3")
def test_tokens_have_no_color() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    token = sc.add(0, GUNDAM_TOKEN, damage=2)
    blue = sc.add(0, GUNDAM_3_4, damage=2)
    dominion = sc.add(0, DOMINION, Zone.HAND)
    st = sc.start()
    assert V.cdef(st, token).color is None
    assert V.cdef(st, blue).color is not None
    play(st, dominion)
    assert st.pending is not None and st.pending.kind is DecisionKind.MAIN
    assert st.cards[blue].damage == 0
    assert st.cards[token].damage == 2


@pytest.mark.rule("5-17-2-1")
def test_a_unit_token_gains_effects_like_a_real_unit() -> None:
    sc = Scenario()
    sc.add(0, WHITE_BASE_GUNDAM)
    token = sc.add(0, GUNDAM_TOKEN, damage=2)
    st = sc.start()
    assert keywords(st, token).get("Repair") == 1
    to_next_turn(st)
    assert st.cards[token].damage == 1


@pytest.mark.rule("5-17-2-1", "5-17-2-5", "5-17-4")
def test_a_unit_token_attacks_and_is_destroyed_by_battle_damage_like_a_real_unit() -> None:
    sc = Scenario()
    token = sc.add(0, ZAKU_TOKEN)
    target = sc.add(1, CANCER, rested=True)
    st = sc.start()
    assert has_action(st, A.ATTACK, token, PLAYER_TARGET)
    attack(st, token, target)
    pass_all(st)
    assert st.cards[target].damage == 1
    assert _left_the_game(st, token)
    assert token not in st.zones[0][Zone.TRASH]
    assert not st.zones[0][Zone.REMOVAL]


@pytest.mark.rule("5-17-2-2", "5-17-2-5")
def test_a_unit_token_can_be_paired_with_a_pilot() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    token = sc.add(0, GUNDAM_TOKEN)
    pilot = sc.add(0, PILOT, Zone.HAND)
    st = sc.start()
    assert has_action(st, A.PAIR, pilot, token)
    play(st, pilot, onto=token)
    assert zone_of(st, pilot) is Zone.PAIRED
    assert st.cards[token].pair == pilot
    assert (ap(st, token), hp(st, token)) == (4, 4)


@pytest.mark.rule("5-17-2-5", "5-17-2-2", "5-17-4")
def test_destroyed_paired_unit_token_leaves_the_game_and_its_pilot_is_trashed() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    token = sc.add(1, ZAKU_TOKEN, pilot=PILOT)
    close_combat = sc.add(0, CLOSE_COMBAT, Zone.HAND)
    st = sc.start()
    pilot = st.cards[token].pair
    assert hp(st, token) == 2
    play(st, close_combat)
    assert _left_the_game(st, token)
    assert zone_of(st, pilot) is Zone.TRASH
    assert not st.zones[1][Zone.REMOVAL]


@pytest.mark.rule("5-17-2-5", "2-9-3")
def test_a_unit_token_returned_to_the_hand_leaves_the_game() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    token = sc.add(1, GUNDAM_TOKEN)
    bounce = sc.add(0, BOUNCE, Zone.HAND)
    st = sc.start()
    hand_before = _hand_size(st, 1)
    play(st, bounce)
    assert _left_the_game(st, token)
    assert _hand_size(st, 1) == hand_before


@pytest.mark.rule("5-17-2-5-1", "5-17-2-5", "2-9-3")
def test_a_destroyed_token_still_fulfils_when_destroyed_triggers() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    sc.add(1, ALPHA_AZIERU)
    token = sc.add(1, GUNDAM_TOKEN)
    fatal = sc.add(0, FATAL_STRIKE, Zone.HAND)
    st = sc.start()
    hand_before = _hand_size(st, 1)
    play(st, fatal)
    assert _left_the_game(st, token)
    assert _hand_size(st, 1) == hand_before + 1


@pytest.mark.rule("5-17-1")
def test_effects_place_base_and_resource_tokens_on_the_field_from_outside_the_game() -> None:
    sc = Scenario()
    sc.resources(0, 9)
    sc.add(0, AGE_1)
    financier, age_device = sc.hand(0, FINANCIER, AGE_DEVICE)
    st = sc.start()
    before = len(st.cards)
    play(st, financier)
    (base,) = uids_in(st, 0, Zone.BASE)
    assert base >= before
    assert V.cdef(st, base).card_type is CardType.EX_BASE
    play(st, age_device)
    (ex,) = _ex_resources(st, 0)
    assert ex >= before
    assert V.cdef(st, ex).is_token


@pytest.mark.rule("5-17-1", "11-4-2", "11-4-2-2")
def test_deploying_two_tokens_into_a_full_battle_area_trashes_two_units() -> None:
    sc = Scenario()
    units = [sc.add(0, VANILLA_2_2) for _ in range(6)]
    _queue_effect(sc, 0, _deploy_two_zaku_tokens())
    st = sc.start()
    for victim in units[:2]:
        assert st.pending is not None and st.pending.kind is DecisionKind.EXCESS
        assert st.pending.player == 0
        act(st, A.SELECT, victim)
    assert [zone_of(st, u) for u in units[:2]] == [Zone.TRASH, Zone.TRASH]
    battle = uids_in(st, 0, Zone.BATTLE)
    assert len(battle) == 6
    tokens = [u for u in battle if V.cdef(st, u).is_token]
    assert len(tokens) == 2
    assert all(V.cdef(st, u).card_number == ZAKU_TOKEN for u in tokens)


@pytest.mark.rule("11-4-2-2")
def test_deploying_two_tokens_with_one_free_slot_trashes_one_unit() -> None:
    sc = Scenario()
    units = [sc.add(0, VANILLA_2_2) for _ in range(5)]
    _queue_effect(sc, 0, _deploy_two_zaku_tokens())
    st = sc.start()
    assert st.pending is not None and st.pending.kind is DecisionKind.EXCESS
    act(st, A.SELECT, units[0])
    assert st.pending is not None and st.pending.kind is DecisionKind.MAIN
    assert len(uids_in(st, 0, Zone.BATTLE)) == 6
    assert [zone_of(st, u) for u in units].count(Zone.TRASH) == 1


@pytest.mark.rule("5-17-3-1-1")
def test_ex_base_is_a_base_token_with_zero_ap_and_three_hp() -> None:
    ex_base = get_registry().db.ex_base
    assert ex_base.card_type is CardType.EX_BASE
    assert ex_base.is_token
    assert (ex_base.ap, ex_base.hp) == (0, 3)

    sc = Scenario()
    two = sc.add(0, VANILLA_2_2)
    one = sc.add(0, PISCES)
    base = sc.base(1)
    shields = sc.shields(1, VANILLA_2_2)
    st = sc.start()
    assert (ap(st, base), hp(st, base)) == (0, 3)
    attack(st, two)
    pass_all(st)
    assert st.cards[base].damage == 2
    assert st.cards[two].damage == 0
    attack(st, one)
    pass_all(st)
    assert _left_the_game(st, base)
    assert uids_in(st, 1, Zone.SHIELD) == shields


def _setup_game(first: int) -> GameState:
    deck = vanilla_deck()
    st = new_game((deck, deck), seed=7, chooser=0)
    act(st, A.GO_FIRST, first)
    act(st, A.KEEP)
    act(st, A.KEEP)
    return st


@pytest.mark.rule("5-17-3-1-2", "5-17-3-2-2")
@pytest.mark.parametrize("first", [0, 1])
def test_setup_places_ex_bases_for_both_and_an_ex_resource_for_player_two(first: int) -> None:
    st = _setup_game(first)
    second = 1 - first
    assert st.pending is not None and st.pending.kind is DecisionKind.MAIN
    for p in (0, 1):
        (base,) = uids_in(st, p, Zone.BASE)
        assert V.cdef(st, base).card_type is CardType.EX_BASE
    assert len(_ex_resources(st, second)) == 1
    assert _ex_resources(st, first) == []


@pytest.mark.rule("5-17-3-2-1", "5-17-3-2-3", "5-17-4")
def test_an_ex_resource_pays_a_cost_once_and_is_then_removed_from_the_game() -> None:
    sc = Scenario()
    (ex,) = sc.resources(0, 0, ex=1)
    pisces = sc.add(0, PISCES, Zone.HAND)
    st = sc.start()
    assert has_action(st, A.PLAY_UNIT, pisces)
    assert all(o.c == 1 for o in options(st) if o.kind is A.PLAY_UNIT and o.a == pisces)
    play(st, pisces, ex=1)
    assert zone_of(st, pisces) is Zone.BATTLE
    assert _left_the_game(st, ex)
    assert not st.zones[0][Zone.RESOURCE_AREA]
    assert not st.zones[0][Zone.REMOVAL]
    assert not st.zones[0][Zone.TRASH]


@pytest.mark.rule("5-17-3-2-3")
def test_a_used_ex_resource_no_longer_counts_toward_the_players_level() -> None:
    sc = Scenario()
    (_, ex) = sc.resources(0, 1, ex=1)
    first, second = sc.hand(0, VANILLA_2_2, VANILLA_2_2)
    st = sc.start()
    play(st, first, ex=1)
    assert _left_the_game(st, ex)
    assert len(st.zones[0][Zone.RESOURCE_AREA]) == 1
    assert not has_action(st, A.PLAY_UNIT, second)


@pytest.mark.rule("5-17-4-2")
def test_the_number_of_tokens_outside_the_game_is_known_to_both_players() -> None:
    sc = Scenario()
    (ex,) = sc.resources(0, 0, ex=1)
    pisces = sc.add(0, PISCES, Zone.HAND)
    st = sc.start()
    play(st, pisces, ex=1)
    views = []
    for observer in (0, 1):
        cards = information_set_key(st, observer)[0]
        views.append([c for c in cards if c[3] == int(Zone.OUTSIDE)])
    assert views[0] == views[1]
    assert [c[0] for c in views[0]] == [ex]
    assert views[0][0][1] == st.cards[ex].def_id


# ---------------------------------------------------------------------------------------------
# 11: rules management


@pytest.mark.rule("11-1-2", "11-1-1")
@pytest.mark.parametrize(("victim", "destroyed"), [(ZAKU_III, True), (CANCER, False)])
def test_rules_management_runs_immediately_in_the_middle_of_an_effect(
    victim: str, destroyed: bool
) -> None:
    sc = Scenario()
    sc.resources(0, 3)
    nt1 = sc.add(0, NT1)
    target = sc.add(1, victim, rested=True)
    pilot = sc.add(0, PILOT, Zone.HAND)
    st = sc.start()
    hand_before = _hand_size(st, 0)
    play(st, pilot, onto=nt1)
    assert (zone_of(st, target) is Zone.TRASH) is destroyed
    assert _hand_size(st, 0) == hand_before - 1 + int(destroyed)


@pytest.mark.rule("11-2-1")
def test_rules_management_defeats_every_player_that_meets_a_defeat_condition() -> None:
    sc = Scenario(deck_size=0)
    st = sc.start()
    assert st.winner == -1
    assert st.end_reason is EndReason.BOTH_DEFEATED

    sc = Scenario(deck_size=0)
    sc.deck(1, VANILLA_2_2)
    st = sc.start()
    assert st.winner == 1
    assert st.end_reason is EndReason.DECK_OUT


@pytest.mark.rule("11-2-1", "11-2-1-2")
def test_a_symmetric_draw_that_empties_both_decks_defeats_both_players() -> None:
    sc = Scenario(deck_size=1)
    attacker = sc.add(0, VANILLA_2_2)
    sc.base(1, ARMORY_ONE, damage=5)
    st = sc.start()
    attack(st, attacker)
    pass_all(st)
    assert not st.zones[0][Zone.DECK]
    assert not st.zones[1][Zone.DECK]
    assert st.winner == -1
    assert st.end_reason is EndReason.BOTH_DEFEATED


@pytest.mark.rule("11-2-1-1", "5-5-3")
def test_battle_damage_defeats_a_player_only_once_their_shield_area_is_empty() -> None:
    sc = Scenario()
    first = sc.add(0, PISCES)
    second = sc.add(0, VANILLA_2_2)
    (shield,) = sc.shields(1, VANILLA_2_2)
    st = sc.start()
    attack(st, first)
    pass_all(st)
    assert zone_of(st, shield) is Zone.TRASH
    assert st.winner is None
    attack(st, second)
    pass_all(st)
    assert st.winner == 0
    assert st.end_reason is EndReason.BATTLE_DAMAGE


@pytest.mark.rule("11-2-1-2", "11-1-1")
def test_drawing_the_last_card_of_the_deck_defeats_that_player_at_once() -> None:
    sc = Scenario(deck_size=1)
    st = sc.start(Step.DRAW_STEP)
    assert not st.zones[0][Zone.DECK]
    assert len(st.zones[1][Zone.DECK]) == 1
    assert st.winner == 1
    assert st.end_reason is EndReason.DECK_OUT
    assert st.pending is None


@pytest.mark.rule("11-3-1")
def test_damage_exceeding_hp_destroys_a_unit_and_a_base_into_the_trash() -> None:
    sc = Scenario()
    attacker = sc.add(0, HAZEL_RAH)
    zaku = sc.add(0, VANILLA_2_2)
    target = sc.add(1, CANCER, rested=True)
    side_7 = sc.base(1, SIDE_7, damage=3)
    st = sc.start()
    attack(st, attacker, target)
    pass_all(st)
    assert zone_of(st, target) is Zone.TRASH
    attack(st, zaku)
    pass_all(st)
    assert zone_of(st, side_7) is Zone.TRASH


@pytest.mark.rule("11-3-1-1")
def test_each_shield_is_treated_as_having_one_hp() -> None:
    sc = Scenario()
    one_ap = sc.add(0, PISCES)
    shields = sc.shields(1, HAZEL_RAH, HAZEL_RAH)
    st = sc.start()
    assert [hp(st, s) for s in shields] == [1, 1]
    attack(st, one_ap)
    pass_all(st)
    assert zone_of(st, shields[0]) is Zone.TRASH
    assert zone_of(st, shields[1]) is Zone.SHIELD


@pytest.mark.rule("11-4-1", "11-4-2", "11-4-2-1")
def test_playing_a_unit_into_a_full_battle_area_trashes_a_chosen_unit_undestroyed() -> None:
    sc = Scenario()
    sc.resources(0, 1)
    units = [sc.add(0, VANILLA_2_2) for _ in range(5)]
    duel = sc.add(0, DUEL_ASSAULT)
    pisces = sc.add(0, PISCES, Zone.HAND)
    st = sc.start()
    play(st, pisces)
    assert st.pending is not None and st.pending.kind is DecisionKind.EXCESS
    assert st.pending.player == 0
    assert {o.a for o in options(st)} == {*units, duel}
    assert zone_of(st, pisces) is Zone.HAND
    act(st, A.SELECT, duel)
    assert zone_of(st, duel) is Zone.TRASH
    assert zone_of(st, pisces) is Zone.BATTLE
    assert len(uids_in(st, 0, Zone.BATTLE)) == 6
    assert _ex_resources(st, 0) == []
    assert "excess" in _history_kinds(st)
    assert "destroyed" not in _history_kinds(st)


@pytest.mark.rule("11-4-1", "5-17-2-1", "5-17-2-5")
def test_unit_tokens_count_toward_the_battle_area_limit() -> None:
    sc = Scenario()
    sc.resources(0, 1)
    for _ in range(5):
        sc.add(0, VANILLA_2_2)
    token = sc.add(0, GUNDAM_TOKEN)
    pisces = sc.add(0, PISCES, Zone.HAND)
    st = sc.start()
    play(st, pisces)
    assert st.pending is not None and st.pending.kind is DecisionKind.EXCESS
    assert token in {o.a for o in options(st)}
    act(st, A.SELECT, token)
    assert _left_the_game(st, token)
    assert len(uids_in(st, 0, Zone.BATTLE)) == 6


@pytest.mark.rule("11-5-1", "11-5-2", "11-5-2-1")
def test_playing_a_base_into_a_full_base_section_trashes_the_old_base_undestroyed() -> None:
    sc = Scenario()
    sc.resources(0, 1)
    armory = sc.base(0, ARMORY_ONE)
    side_7 = sc.add(0, SIDE_7, Zone.HAND)
    st = sc.start()
    decks_before = [len(st.zones[p][Zone.DECK]) for p in (0, 1)]
    play(st, side_7)
    assert uids_in(st, 0, Zone.BASE) == [side_7]
    assert zone_of(st, armory) is Zone.TRASH
    assert [len(st.zones[p][Zone.DECK]) for p in (0, 1)] == decks_before
    assert "destroyed" not in _history_kinds(st)


@pytest.mark.rule("11-5-1", "11-5-2", "5-17-2-5")
def test_an_effect_deploying_a_base_replaces_the_base_already_in_play() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    old = sc.base(0)
    financier = sc.add(0, FINANCIER, Zone.HAND)
    st = sc.start()
    play(st, financier)
    (new,) = uids_in(st, 0, Zone.BASE)
    assert new != old
    assert _left_the_game(st, old)
