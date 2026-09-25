"""Rules 5-22 (Battle), 7-5-4 / 8 (Attacking and Battles), 7-6-3 / 8-4 / 9 (Action Steps)."""

from __future__ import annotations

import pytest

from gcg_sim.effects import dsl as d
from gcg_sim.effects.registry import get_registry
from gcg_sim.engine import interp as I
from gcg_sim.engine.core import BOTH_KNOW
from gcg_sim.engine.game import advance
from gcg_sim.engine.state import GameState
from gcg_sim.engine.types import PLAYER_TARGET, ActionKind, DecisionKind, EndReason, Phase, Step
from gcg_sim.testkit import (
    Scenario,
    Zone,
    act,
    activate,
    ap,
    attack,
    block,
    end_main,
    has_action,
    legal_kinds,
    no,
    options,
    pass_,
    pass_all,
    play,
    select_if_asked,
    to_next_turn,
    uids_in,
    yes,
    yes_if_asked,
    zone_of,
)

A = ActionKind

VANILLA_2_2 = "GD01-060"  # Zaku Mariner, Lv2 2/2, no effects
VANILLA_3_4 = "GD01-013"  # Gundam, Lv4 3/4, no effects
VANILLA_2_3 = "GD01-057"  # Dreissen (Sleeves), Lv2 2/3, no effects
VANILLA_4_5 = "GD04-032"  # Xavier's Gyan Hakuji-Packs (GQ), Lv5 4/5, no effects
VANILLA_2_4 = "GD04-062"  # Lunamaria's Gunner Zaku Warrior, Lv3 2/4, no effects
VANILLA_4_1 = "GD02-048"  # Zaku III (Sleeves), Lv3 4/1, no effects
VANILLA_3_1 = "GD04-047"  # Gundam Virtue, Lv3 3/1, no effects
VANILLA_3_1_LV2 = "ST07-008"  # Gundam Kyrios (Flight Mode), Lv2 3/1, no effects
VANILLA_1_2 = "GD01-021"  # Pisces, Lv1 1/2, no effects
VANILLA_1_2_ALT = "GD02-030"  # Genoace, Lv1 1/2, no effects
BLOCKER_2_4 = "GD01-086"  # Lv3 2/4 <Blocker>
BLOCKER_3_4 = "GD01-072"  # Launcher Strike Gundam, Lv4 3/4 <Blocker>
CB_BLOCKER_5_4 = "GD03-057"  # (CB) Lv6 5/4 <Blocker>
ZERO_AP = "EB01-013"  # Lv2 AP 0 (no printed AP, data-null:UNIT:ap), 【Attack】needs 6+ enemy hand
ZEE_ZULU = "GD01-059"  # 【Attack】If attacking the enemy player, AP+2 during this battle (2/2)
ZAKU_I_ATTACK = "ST03-008"  # 【Attack】This Unit gets AP+2 during this turn (1/2)
DESTROY_LV2_ON_ATTACK = "GD05-060"  # 【Deploy】/【Attack】Destroy 1 enemy Unit Lv.2 or lower (5/3)
NU_GUNDAM = "GD05-017"  # (Londo Bell) 【When Paired】... begin a battle, only the damage step
ATTACK_PILOT = "ST07-009"  # Pilot: 【Attack】This Unit gets AP+1 during this turn
FIRST_STRIKE_1_4 = "GD04-034"  # <First Strike> 1/4
FIRST_STRIKE_5_6 = "GD05-055"  # <First Strike> 5/6
DESTROYED_PING = "GD01-056"  # 【Destroyed】Deal 1 damage to an enemy Unit with 5 or less AP (2/3)
TAURUS = "EB01-033"  # 【Activate･Action】①: other Unit being attacked gets AP+1 during this battle
GALLUSS = "GD01-058"  # 【Activate･Action】①: a Lv.4+ Unit gets AP+1 during this battle (3/2)
BASE_HP5 = "GD01-127"  # Base, HP 5
BURST_ADD = "GD01-097"  # Guel Jeturk, 【Burst】Add this card to your hand
PING_1 = "GD01-115"  # Lv2 C1 【Main】/【Action】Deal 1 damage to an enemy Unit
DRAW_MAIN_ONLY = "GD01-118"  # Lv2 C1 【Main】Draw 2. Then, discard 1.
BOUNCE_ACTION = "GD03-122"  # Lv2 C1 【Action】Return an enemy Lv.3- Unit to hand; 【Pilot】
AP_MINUS_3 = "ST01-014"  # Lv3 C1 【Main】/【Action】An enemy Unit gets AP-3 during this turn
REDUCE_3_BATTLE = "GD04-113"  # Lv3 C1 【Action】During this battle, reduce battle damage by 3
DEPLOY_EX_BASE = "GD04-110"  # Lv6 C3 【Main】/【Action】Deploy 1 EX Base
REDIRECT_TO_CB = "ST07-013"  # Lv4 C1 【Action】Change the battling enemy Unit's target to a CB Unit
ATTACK_ACTIVE_OK = "GD01-110"  # Lv3 C1 【Main】/【Action】A Lv.4+ Unit may attack active enemies


def _effect_program(steps: tuple[d.Step, ...]) -> int:
    return get_registry().program(steps, "tests/rules/test_battle")


def _queue_effect(
    st: GameState, controller: int, host: int, steps: tuple[d.Step, ...], **refs: int
) -> None:
    """Put an effect on the resolution stack; it resolves right after the next applied action.

    Models card wording that no implemented card carries (a battle between two chosen Units,
    an effect that fires in the block step), using the engine's own effect instructions.
    """
    I.push_frame(
        st,
        _effect_program(steps),
        controller=controller,
        host=host,
        kind="trigger",
        vars={k: (v,) for k, v in refs.items()},
    )


def _resolve_effect_now(
    st: GameState, controller: int, host: int, steps: tuple[d.Step, ...], **refs: int
) -> None:
    _queue_effect(st, controller, host, steps, **refs)
    st.pending = None
    advance(st)


def _decider(st: GameState) -> tuple[DecisionKind, int] | None:
    return (st.pending.kind, st.pending.player) if st.pending else None


def _attack_targets(st: GameState, attacker: int) -> set[int]:
    return {o.b for o in options(st) if o.kind is A.ATTACK and o.a == attacker}


# ---------------------------------------------------------------------------------------------
# 5-22 Battle


@pytest.mark.rule("5-22-1", "8-5-3-1")
def test_attacker_and_attacked_unit_battle_each_other() -> None:
    sc = Scenario()
    attacker = sc.add(0, VANILLA_3_4)
    target = sc.add(1, VANILLA_2_3, rested=True)
    bystander = sc.add(1, VANILLA_2_2, rested=True)
    st = sc.start()
    attack(st, attacker, target)
    pass_all(st)
    assert zone_of(st, target) is Zone.TRASH
    assert st.cards[attacker].damage == 2
    assert st.cards[bystander].damage == 0


@pytest.mark.rule("5-22-1")
@pytest.mark.card("EB01-033")
def test_only_the_attack_target_is_being_attacked() -> None:
    sc = Scenario()
    attacker = sc.add(0, VANILLA_3_4)
    target = sc.add(1, VANILLA_2_2, rested=True)
    bystander = sc.add(1, VANILLA_2_2, rested=True)
    taurus = sc.add(1, TAURUS)
    sc.resources(1, 1)
    sc.resources(0, 2)
    sc.hand(0, PING_1)
    st = sc.start()
    attack(st, attacker, target)
    activate(st, taurus)
    assert _decider(st) == (DecisionKind.ACTION_STEP, 0)  # single candidate, no choice
    pass_all(st)
    assert st.cards[attacker].damage == 3  # the attacked Unit fought with AP 2+1
    assert st.cards[bystander].damage == 0


@pytest.mark.rule("5-22-2", "8-3-1")
@pytest.mark.card("EB01-033")
def test_blocker_becomes_the_battling_target_and_old_target_stops_battling() -> None:
    sc = Scenario()
    attacker = sc.add(0, VANILLA_3_4)
    original = sc.add(1, VANILLA_2_2, rested=True)
    blocker = sc.add(1, BLOCKER_2_4)
    taurus = sc.add(1, TAURUS)
    sc.resources(1, 1)
    sc.resources(0, 2)
    sc.hand(0, PING_1)
    st = sc.start()
    attack(st, attacker, original)
    block(st, blocker)
    activate(st, taurus)
    assert _decider(st) == (DecisionKind.ACTION_STEP, 0)  # only the Blocker is being attacked
    pass_all(st)
    assert st.cards[attacker].damage == 3  # the Blocker (AP 2+1) dealt the damage
    assert st.cards[blocker].damage == 3
    assert st.cards[original].damage == 0
    assert zone_of(st, original) is Zone.BATTLE


@pytest.mark.rule("5-22-2", "8-5-1")
@pytest.mark.card("ST07-013")
def test_effect_that_changes_attack_target_makes_new_target_battle() -> None:
    sc = Scenario()
    attacker = sc.add(0, VANILLA_3_4)
    cb_unit = sc.add(1, CB_BLOCKER_5_4, rested=True)
    sc.resources(1, 4)
    redirect = sc.add(1, REDIRECT_TO_CB, Zone.HAND)
    (shield,) = sc.shields(1, VANILLA_2_2)
    st = sc.start()
    attack(st, attacker, PLAYER_TARGET)
    act(st, A.PLAY_COMMAND, redirect)
    pass_all(st)
    assert zone_of(st, shield) is Zone.SHIELD  # the attack on the player no longer happens
    assert st.cards[cb_unit].damage == 3
    assert zone_of(st, attacker) is Zone.TRASH  # the new target dealt 5 damage back


@pytest.mark.rule("5-22-3", "5-22-3-1")
@pytest.mark.card("GD05-017")
@pytest.mark.ruling("GD05-017:Q345")
def test_units_own_effect_battle_runs_only_the_damage_step() -> None:
    sc = Scenario()
    sc.resources(0, 7)
    nu = sc.add(0, NU_GUNDAM)
    londo_bell = sc.trash(0, NU_GUNDAM, NU_GUNDAM, NU_GUNDAM)
    pilot = sc.add(0, ATTACK_PILOT, Zone.HAND)
    target = sc.add(1, BLOCKER_3_4)
    sc.add(1, BLOCKER_2_4)
    sc.resources(1, 2)
    sc.hand(1, PING_1)
    st = sc.start()
    play(st, pilot, onto=nu)
    assert ap(st, nu) == 7
    yes_if_asked(st)
    select_if_asked(st, *londo_bell)
    act(st, A.SELECT, target)
    assert zone_of(st, target) is Zone.TRASH  # 7 damage to the chosen Unit
    assert st.cards[nu].damage == 3  # the chosen Unit battled back as the attack target
    assert ap(st, nu) == 7  # the paired Pilot's 【Attack】 did not trigger
    assert not st.cards[nu].rested  # no attack step: the Unit was never rested to attack
    assert _decider(st) == (DecisionKind.MAIN, 0)  # no block or action step was offered
    assert st.battle is None


@pytest.mark.rule("5-22-4")
def test_effect_battle_between_two_chosen_units_first_chosen_attacks() -> None:
    sc = Scenario()
    first_striker = sc.add(0, FIRST_STRIKE_1_4)
    enemy = sc.add(1, VANILLA_2_2, damage=1)
    st = sc.start()
    battle = (d.StartBattle(d.Var("first"), d.Var("second")),)
    _resolve_effect_now(st, 0, first_striker, battle, first=first_striker, second=enemy)
    assert zone_of(st, enemy) is Zone.TRASH
    assert st.cards[first_striker].damage == 0  # it attacked, so <First Strike> applied


@pytest.mark.rule("5-22-4")
def test_effect_battle_second_chosen_unit_is_the_attack_target() -> None:
    sc = Scenario()
    first_striker = sc.add(0, FIRST_STRIKE_1_4)
    enemy = sc.add(1, VANILLA_2_2, damage=1)
    st = sc.start()
    battle = (d.StartBattle(d.Var("first"), d.Var("second")),)
    _resolve_effect_now(st, 0, first_striker, battle, first=enemy, second=first_striker)
    assert zone_of(st, enemy) is Zone.TRASH
    assert st.cards[first_striker].damage == 2  # as attack target it gets no <First Strike>


@pytest.mark.rule("5-22-4-1")
def test_effect_battle_between_two_chosen_units_skips_attack_triggers() -> None:
    sc = Scenario()
    zaku = sc.add(0, ZAKU_I_ATTACK)
    enemy = sc.add(1, VANILLA_1_2, damage=1)
    st = sc.start()
    battle = (d.StartBattle(d.Var("first"), d.Var("second")),)
    _resolve_effect_now(st, 0, zaku, battle, first=zaku, second=enemy)
    assert zone_of(st, enemy) is Zone.TRASH
    assert st.cards[zaku].damage == 1
    assert ap(st, zaku) == 1  # 【Attack】AP+2 was not triggered
    assert _decider(st) == (DecisionKind.MAIN, 0)


# ---------------------------------------------------------------------------------------------
# 7-5-4 / 8-1 / 8-2 Attack step


@pytest.mark.rule("7-5-4-1", "8-2-1")
def test_attack_targets_are_opposing_player_or_rested_enemy_units() -> None:
    sc = Scenario()
    unit = sc.add(0, VANILLA_2_2)
    rested_enemy = sc.add(1, VANILLA_2_2, rested=True)
    active_enemy = sc.add(1, VANILLA_2_2)
    rested_friendly = sc.add(0, VANILLA_2_2, rested=True)
    sc.shields(1, VANILLA_2_2)
    st = sc.start()
    targets = _attack_targets(st, unit)
    assert targets == {PLAYER_TARGET, rested_enemy}
    assert active_enemy not in targets and rested_friendly not in targets


@pytest.mark.rule("7-5-4-1")
def test_only_units_in_the_battle_area_can_attack() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    in_hand = sc.add(0, VANILLA_2_2, Zone.HAND)
    in_trash = sc.add(0, VANILLA_2_2, Zone.TRASH)
    st = sc.start()
    assert not has_action(st, A.ATTACK, in_hand)
    assert not has_action(st, A.ATTACK, in_trash)


@pytest.mark.rule("8-2-1")
def test_attacker_must_be_active_and_is_rested_on_declaration() -> None:
    sc = Scenario()
    unit = sc.add(0, VANILLA_2_2)
    tired = sc.add(0, VANILLA_2_2, rested=True)
    sc.add(1, BLOCKER_2_4)
    st = sc.start()
    assert not has_action(st, A.ATTACK, tired)
    attack(st, unit, PLAYER_TARGET)
    assert st.cards[unit].rested
    assert st.battle is not None
    assert (st.battle.attacker, st.battle.target) == (unit, PLAYER_TARGET)


@pytest.mark.rule("8-2-1")
@pytest.mark.faq("Q34", "Q35")
@pytest.mark.card("EB01-013")
def test_zero_ap_unit_may_attack_but_cannot_destroy_a_shield() -> None:
    sc = Scenario()
    zero = sc.add(0, ZERO_AP)
    (shield,) = sc.shields(1, VANILLA_2_2)
    st = sc.start()
    assert ap(st, zero) == 0
    attack(st, zero, PLAYER_TARGET)
    pass_all(st)
    assert st.cards[zero].rested
    assert zone_of(st, shield) is Zone.SHIELD


@pytest.mark.rule("8-1", "7-5-4-1")
def test_attacks_are_declared_only_by_the_active_player_in_the_main_phase() -> None:
    sc = Scenario()
    sc.add(0, VANILLA_2_2)
    sc.add(1, VANILLA_2_2)
    sc.resources(1, 2)
    sc.hand(1, PING_1)
    sc.shields(0, VANILLA_2_2)
    sc.shields(1, VANILLA_2_2)
    st = sc.start()
    assert A.ATTACK in legal_kinds(st)
    end_main(st)
    assert _decider(st) == (DecisionKind.ACTION_STEP, 1)
    assert A.ATTACK not in legal_kinds(st)


@pytest.mark.rule("8-1", "8-2-3", "8-5-4", "8-6-1", "8-6-2")
@pytest.mark.card("GD01-059", "GD01-097")
def test_battle_runs_attack_block_action_damage_and_battle_end_steps_in_order() -> None:
    sc = Scenario()
    zee = sc.add(0, ZEE_ZULU)
    sc.add(1, BLOCKER_2_4)
    sc.resources(1, 2)
    sc.hand(1, PING_1)
    (shield,) = sc.shields(1, BURST_ADD)
    st = sc.start()
    trace: list[tuple[Step, DecisionKind, int, int]] = []

    def snap() -> None:
        assert st.pending is not None
        trace.append((st.step, st.pending.kind, st.pending.player, ap(st, zee)))

    attack(st, zee, PLAYER_TARGET)
    snap()
    block(st, None)
    snap()
    pass_(st)
    snap()
    no(st)
    snap()
    assert trace == [
        (Step.BLOCK, DecisionKind.BLOCK, 1, 4),
        (Step.BATTLE_ACTION, DecisionKind.ACTION_STEP, 1, 4),
        (Step.DAMAGE_DONE, DecisionKind.BURST, 1, 4),
        (Step.MAIN, DecisionKind.MAIN, 0, 2),
    ]
    assert zone_of(st, shield) is Zone.TRASH
    assert st.battle is None


@pytest.mark.rule("8-2-2")
@pytest.mark.card("ST03-008")
def test_attack_effects_resolve_in_the_attack_step() -> None:
    sc = Scenario()
    zaku = sc.add(0, ZAKU_I_ATTACK)
    sc.add(1, BLOCKER_2_4)
    st = sc.start()
    assert ap(st, zaku) == 1
    attack(st, zaku, PLAYER_TARGET)
    assert _decider(st) == (DecisionKind.BLOCK, 1)
    assert ap(st, zaku) == 3


@pytest.mark.rule("8-2-3")
@pytest.mark.card("GD01-059")
def test_during_this_battle_effect_applies_from_the_attack_step() -> None:
    sc = Scenario()
    zee = sc.add(0, ZEE_ZULU)
    sc.add(1, BLOCKER_2_4)
    sc.resources(1, 2)
    sc.hand(1, PING_1)
    sc.shields(1, VANILLA_2_2, VANILLA_2_2)
    st = sc.start()
    assert ap(st, zee) == 2
    attack(st, zee, PLAYER_TARGET)
    assert st.pending is not None and st.pending.kind is DecisionKind.BLOCK
    assert ap(st, zee) == 4
    block(st, None)
    assert st.pending is not None and st.pending.kind is DecisionKind.ACTION_STEP
    assert ap(st, zee) == 4


@pytest.mark.rule("8-2-4")
@pytest.mark.faq("Q33")
@pytest.mark.card("GD05-060")
def test_target_destroyed_in_attack_step_skips_to_battle_end() -> None:
    sc = Scenario()
    attacker = sc.add(0, DESTROY_LV2_ON_ATTACK)
    target = sc.add(1, VANILLA_2_2, rested=True)
    blocker = sc.add(1, BLOCKER_3_4)
    sc.resources(1, 2)
    sc.hand(1, PING_1)
    st = sc.start()
    attack(st, attacker, target)
    assert zone_of(st, target) is Zone.TRASH
    assert _decider(st) == (DecisionKind.MAIN, 0)  # no block step, no action step
    assert not st.cards[blocker].rested
    assert st.cards[attacker].damage == 0
    assert st.battle is None


# ---------------------------------------------------------------------------------------------
# 8-3 Block step


@pytest.mark.rule("8-3-1")
@pytest.mark.faq("Q59")
def test_standby_player_may_block_with_an_active_blocker() -> None:
    sc = Scenario()
    attacker = sc.add(0, VANILLA_3_4)
    blocker = sc.add(1, BLOCKER_2_4)
    sc.add(1, BLOCKER_2_4, rested=True)
    sc.add(1, VANILLA_2_2)
    sc.resources(1, 2)
    sc.hand(1, PING_1)
    (shield,) = sc.shields(1, VANILLA_2_2)
    st = sc.start()
    attack(st, attacker, PLAYER_TARGET)
    assert _decider(st) == (DecisionKind.BLOCK, 1)
    assert {o.a for o in options(st) if o.kind is A.BLOCK} == {blocker}
    block(st, blocker)
    assert st.cards[blocker].rested
    assert st.battle is not None and st.battle.target == blocker
    pass_all(st)
    assert zone_of(st, shield) is Zone.SHIELD
    assert st.cards[blocker].damage == 3
    assert st.cards[attacker].damage == 2


@pytest.mark.rule("8-3-2")
def test_only_one_blocker_per_attack() -> None:
    sc = Scenario()
    attacker = sc.add(0, VANILLA_3_4)
    first = sc.add(1, BLOCKER_2_4)
    second = sc.add(1, BLOCKER_2_4)
    sc.shields(1, VANILLA_2_2)
    st = sc.start()
    attack(st, attacker, PLAYER_TARGET)
    block(st, first)
    assert _decider(st) == (DecisionKind.MAIN, 0)
    assert st.cards[first].damage == 3
    assert not st.cards[second].rested
    assert st.cards[second].damage == 0


@pytest.mark.rule("8-3-3")
@pytest.mark.card("GD01-110")
def test_original_attack_target_cannot_block_for_itself() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    attacker = sc.add(0, VANILLA_3_4)
    enabler = sc.add(0, ATTACK_ACTIVE_OK, Zone.HAND)
    target = sc.add(1, BLOCKER_3_4)
    other = sc.add(1, BLOCKER_2_4)
    st = sc.start()
    play(st, enabler)
    act(st, A.SELECT, attacker)
    attack(st, attacker, target)
    assert _decider(st) == (DecisionKind.BLOCK, 1)
    assert {o.a for o in options(st) if o.kind is A.BLOCK} == {other}


@pytest.mark.rule("8-3-3")
@pytest.mark.card("GD01-110")
def test_lone_attacked_blocker_gets_no_block_step() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    attacker = sc.add(0, VANILLA_3_4)
    enabler = sc.add(0, ATTACK_ACTIVE_OK, Zone.HAND)
    target = sc.add(1, BLOCKER_3_4)
    st = sc.start()
    play(st, enabler)
    act(st, A.SELECT, attacker)
    attack(st, attacker, target)
    assert _decider(st) == (DecisionKind.MAIN, 0)
    assert st.cards[target].damage == 3
    assert st.cards[attacker].damage == 3


@pytest.mark.rule("8-3-4")
def test_declining_to_block_keeps_the_attack_target() -> None:
    sc = Scenario()
    attacker = sc.add(0, VANILLA_3_4)
    blocker = sc.add(1, BLOCKER_2_4)
    (shield,) = sc.shields(1, VANILLA_2_2)
    st = sc.start()
    attack(st, attacker, PLAYER_TARGET)
    assert has_action(st, A.NO_BLOCK)
    block(st, None)
    assert zone_of(st, shield) is Zone.TRASH
    assert not st.cards[blocker].rested
    assert st.cards[blocker].damage == 0


@pytest.mark.rule("8-3-5")
def test_attacker_leaving_during_block_step_skips_to_battle_end() -> None:
    sc = Scenario()
    attacker = sc.add(0, VANILLA_2_2)
    blocker = sc.add(1, BLOCKER_3_4)
    sc.resources(1, 2)
    sc.hand(1, PING_1)
    (shield,) = sc.shields(1, VANILLA_2_2)
    st = sc.start()
    attack(st, attacker, PLAYER_TARGET)
    _queue_effect(st, 1, blocker, (d.Destroy(d.Var("x")),), x=attacker)
    block(st, blocker)
    assert zone_of(st, attacker) is Zone.TRASH
    assert _decider(st) == (DecisionKind.MAIN, 0)  # the defender got no action step
    assert st.cards[blocker].damage == 0
    assert zone_of(st, shield) is Zone.SHIELD
    assert st.battle is None


@pytest.mark.rule("8-3-5")
def test_blocker_leaving_during_block_step_skips_to_battle_end() -> None:
    sc = Scenario()
    attacker = sc.add(0, VANILLA_2_2)
    original = sc.add(1, VANILLA_2_2, rested=True)
    blocker = sc.add(1, BLOCKER_3_4)
    sc.resources(0, 2)
    sc.hand(0, PING_1)
    st = sc.start()
    attack(st, attacker, original)
    _queue_effect(st, 1, blocker, (d.Destroy(d.Var("x")),), x=blocker)
    block(st, blocker)
    assert zone_of(st, blocker) is Zone.TRASH
    assert _decider(st) == (DecisionKind.MAIN, 0)
    assert st.cards[attacker].damage == 0
    assert st.cards[original].damage == 0


@pytest.mark.rule("8-3-5", "9-1")
def test_block_step_continues_to_action_step_when_nobody_left() -> None:
    sc = Scenario()
    attacker = sc.add(0, VANILLA_2_2)
    blocker = sc.add(1, BLOCKER_3_4)
    sc.resources(1, 2)
    sc.hand(1, PING_1)
    st = sc.start()
    attack(st, attacker, PLAYER_TARGET)
    block(st, blocker)
    assert _decider(st) == (DecisionKind.ACTION_STEP, 1)
    assert st.step is Step.BATTLE_ACTION


# ---------------------------------------------------------------------------------------------
# 8-4 Action step (in battle)


@pytest.mark.rule("8-4-1", "9-2")
def test_battle_action_step_starts_with_the_standby_player() -> None:
    sc = Scenario()
    attacker = sc.add(0, VANILLA_3_4)
    sc.add(1, VANILLA_4_5, rested=True)
    sc.resources(0, 2)
    sc.resources(1, 2)
    mine = sc.hand(0, PING_1)
    theirs = sc.hand(1, PING_1)
    sc.shields(1, VANILLA_2_2)
    st = sc.start()
    attack(st, attacker, PLAYER_TARGET)
    assert _decider(st) == (DecisionKind.ACTION_STEP, 1)
    assert has_action(st, A.PLAY_COMMAND, theirs[0])
    pass_(st)
    assert _decider(st) == (DecisionKind.ACTION_STEP, 0)
    assert has_action(st, A.PLAY_COMMAND, mine[0])


@pytest.mark.rule("8-4-2", "9-4")
@pytest.mark.faq("Q33")
@pytest.mark.card("GD03-122")
def test_attacker_returned_in_action_step_skips_damage_step() -> None:
    sc = Scenario()
    attacker = sc.add(0, VANILLA_2_2)
    sc.add(1, VANILLA_4_5, rested=True)
    sc.resources(0, 2)
    sc.hand(0, PING_1)
    sc.resources(1, 2)
    bounce = sc.add(1, BOUNCE_ACTION, Zone.HAND)
    (shield,) = sc.shields(1, VANILLA_2_2)
    st = sc.start()
    attack(st, attacker, PLAYER_TARGET)
    act(st, A.PLAY_COMMAND, bounce)
    assert zone_of(st, attacker) is Zone.HAND
    assert _decider(st) == (DecisionKind.ACTION_STEP, 0)  # the action step still continues
    pass_(st)
    assert _decider(st) == (DecisionKind.MAIN, 0)
    assert zone_of(st, shield) is Zone.SHIELD
    assert st.battle is None


@pytest.mark.rule("8-4-2")
def test_target_destroyed_in_action_step_skips_damage_step() -> None:
    sc = Scenario()
    attacker = sc.add(0, VANILLA_2_2)
    target = sc.add(1, VANILLA_4_1, rested=True)
    sc.resources(0, 2)
    ping = sc.add(0, PING_1, Zone.HAND)
    st = sc.start()
    attack(st, attacker, target)
    assert _decider(st) == (DecisionKind.ACTION_STEP, 0)
    act(st, A.PLAY_COMMAND, ping)
    assert zone_of(st, target) is Zone.TRASH
    pass_all(st)
    assert _decider(st) == (DecisionKind.MAIN, 0)
    assert zone_of(st, attacker) is Zone.BATTLE
    assert st.cards[attacker].damage == 0  # the 4-AP target never dealt battle damage


# ---------------------------------------------------------------------------------------------
# 8-5 Damage step


@pytest.mark.rule("8-5-1", "8-5-2-1", "8-5-2-4")
@pytest.mark.card("GD04-110")
def test_shield_area_is_checked_when_the_damage_step_begins() -> None:
    sc = Scenario()
    attacker = sc.add(0, VANILLA_2_2)
    sc.resources(1, 6)
    financier = sc.add(1, DEPLOY_EX_BASE, Zone.HAND)
    st = sc.start()
    attack(st, attacker, PLAYER_TARGET)
    act(st, A.PLAY_COMMAND, financier)
    (base,) = uids_in(st, 1, Zone.BASE)
    pass_all(st)
    assert st.winner is None
    assert st.cards[base].damage == 2


@pytest.mark.rule("8-5-2-1", "8-5-2-2")
@pytest.mark.faq("Q32")
def test_attack_on_player_with_empty_shield_area_defeats_the_player() -> None:
    sc = Scenario(active=1)
    unit = sc.add(1, VANILLA_2_2)
    st = sc.start()
    attack(st, unit, PLAYER_TARGET)
    pass_all(st)
    assert st.winner == 1
    assert st.end_reason is EndReason.BATTLE_DAMAGE
    assert st.phase is Phase.GAME_OVER


@pytest.mark.rule("8-5-2-2")
@pytest.mark.card("ST01-014")
def test_zero_ap_attack_on_empty_shield_area_deals_no_damage() -> None:
    sc = Scenario()
    unit = sc.add(0, VANILLA_2_2)
    sc.resources(1, 3)
    weaken = sc.add(1, AP_MINUS_3, Zone.HAND)
    st = sc.start()
    attack(st, unit, PLAYER_TARGET)
    act(st, A.PLAY_COMMAND, weaken)
    assert ap(st, unit) == 0
    pass_all(st)
    assert st.winner is None
    assert _decider(st) == (DecisionKind.MAIN, 0)


@pytest.mark.rule("8-5-2-1", "8-5-2-3")
@pytest.mark.faq("Q36")
def test_attack_on_player_without_base_hits_only_the_top_shield() -> None:
    sc = Scenario()
    unit = sc.add(0, VANILLA_4_5)
    top, second, third = sc.shields(1, VANILLA_2_2, VANILLA_2_2, VANILLA_2_2)
    st = sc.start()
    attack(st, unit, PLAYER_TARGET)
    pass_all(st)
    assert zone_of(st, top) is Zone.TRASH
    assert uids_in(st, 1, Zone.SHIELD) == [second, third]
    assert st.winner is None


@pytest.mark.rule("8-5-2-3-1")
@pytest.mark.faq("Q40")
@pytest.mark.card("GD01-097")
def test_destroyed_burst_shield_is_revealed_and_offered_before_trash() -> None:
    sc = Scenario()
    unit = sc.add(0, VANILLA_2_2)
    burst, _ = sc.shields(1, BURST_ADD, VANILLA_2_2)
    st = sc.start()
    assert st.cards[burst].known == 0
    attack(st, unit, PLAYER_TARGET)
    pass_all(st)
    assert _decider(st) == (DecisionKind.BURST, 1)
    assert st.cards[burst].known == BOTH_KNOW
    assert zone_of(st, burst) not in (Zone.SHIELD, Zone.TRASH)
    no(st)
    assert zone_of(st, burst) is Zone.TRASH


@pytest.mark.rule("8-5-2-3-1")
def test_destroyed_shield_without_burst_is_revealed_into_trash() -> None:
    sc = Scenario()
    unit = sc.add(0, VANILLA_2_2)
    shield, _ = sc.shields(1, VANILLA_3_4, VANILLA_2_2)
    st = sc.start()
    attack(st, unit, PLAYER_TARGET)
    pass_all(st)
    assert zone_of(st, shield) is Zone.TRASH
    assert st.cards[shield].known == BOTH_KNOW
    assert _decider(st) == (DecisionKind.MAIN, 0)


@pytest.mark.rule("8-5-2-3-1")
@pytest.mark.faq("Q35")
@pytest.mark.card("ST01-014")
def test_zero_damage_does_not_destroy_a_shield() -> None:
    sc = Scenario()
    unit = sc.add(0, VANILLA_2_2)
    sc.resources(1, 3)
    weaken = sc.add(1, AP_MINUS_3, Zone.HAND)
    (shield,) = sc.shields(1, VANILLA_2_2)
    st = sc.start()
    attack(st, unit, PLAYER_TARGET)
    act(st, A.PLAY_COMMAND, weaken)
    pass_all(st)
    assert zone_of(st, shield) is Zone.SHIELD


@pytest.mark.rule("8-5-2-1", "8-5-2-4")
@pytest.mark.faq("Q37")
def test_base_takes_the_damage_of_an_attack_on_the_player() -> None:
    sc = Scenario()
    unit = sc.add(0, VANILLA_2_2)
    base = sc.base(1)
    shields = sc.shields(1, VANILLA_2_2, VANILLA_2_2)
    st = sc.start()
    attack(st, unit, PLAYER_TARGET)
    pass_all(st)
    assert st.cards[base].damage == 2
    assert zone_of(st, base) is Zone.BASE
    assert uids_in(st, 1, Zone.SHIELD) == shields
    assert st.cards[unit].damage == 0  # the Base deals no battle damage back


@pytest.mark.rule("8-5-2-4-1")
def test_base_damage_counters_accumulate_until_the_base_is_destroyed() -> None:
    sc = Scenario()
    first = sc.add(0, VANILLA_2_2)
    second = sc.add(0, VANILLA_2_2)
    base = sc.base(1, BASE_HP5)
    shields = sc.shields(1, VANILLA_2_2)
    st = sc.start()
    attack(st, first, PLAYER_TARGET)
    pass_all(st)
    attack(st, second, PLAYER_TARGET)
    pass_all(st)
    assert st.cards[base].damage == 4
    to_next_turn(st)
    to_next_turn(st)
    assert st.cards[base].damage == 4
    attack(st, first, PLAYER_TARGET)
    pass_all(st)
    assert zone_of(st, base) is Zone.TRASH
    assert uids_in(st, 1, Zone.SHIELD) == shields


@pytest.mark.rule("8-5-2-4-2")
def test_first_strike_attacker_damages_the_base_and_takes_nothing() -> None:
    sc = Scenario()
    striker = sc.add(0, FIRST_STRIKE_1_4)
    base = sc.base(1)
    sc.shields(1, VANILLA_2_2)
    st = sc.start()
    attack(st, striker, PLAYER_TARGET)
    pass_all(st)
    assert st.cards[base].damage == 1
    assert st.cards[striker].damage == 0


@pytest.mark.rule("8-5-2-4-2", "8-5-2-4-1")
def test_first_strike_attack_destroys_base_without_reaching_shields() -> None:
    sc = Scenario()
    striker = sc.add(0, FIRST_STRIKE_5_6)
    base = sc.base(1)
    shields = sc.shields(1, VANILLA_2_2)
    st = sc.start()
    attack(st, striker, PLAYER_TARGET)
    pass_all(st)
    assert zone_of(st, base) is not Zone.BASE
    assert not uids_in(st, 1, Zone.BASE)
    assert uids_in(st, 1, Zone.SHIELD) == shields


@pytest.mark.rule("8-5-3-2", "8-5-3-2-3")
@pytest.mark.faq("Q39")
def test_units_deal_battle_damage_simultaneously() -> None:
    sc = Scenario()
    attacker = sc.add(0, VANILLA_2_2)
    target = sc.add(1, VANILLA_3_1_LV2, rested=True)
    st = sc.start()
    attack(st, attacker, target)
    pass_all(st)
    assert zone_of(st, target) is Zone.TRASH
    assert zone_of(st, attacker) is Zone.TRASH  # the destroyed target still dealt its damage


@pytest.mark.rule("8-5-3-2")
def test_each_unit_receives_the_others_ap_as_damage() -> None:
    sc = Scenario()
    attacker = sc.add(0, VANILLA_3_4)
    target = sc.add(1, VANILLA_2_4, rested=True)
    st = sc.start()
    attack(st, attacker, target)
    pass_all(st)
    assert st.cards[target].damage == 3
    assert st.cards[attacker].damage == 2


@pytest.mark.rule("8-5-3-2-1")
def test_unit_damage_counters_accumulate_until_hp_is_reached() -> None:
    sc = Scenario()
    attackers = [sc.add(0, VANILLA_2_2) for _ in range(3)]
    target = sc.add(1, VANILLA_4_5, rested=True)
    st = sc.start()
    attack(st, attackers[0], target)
    pass_all(st)
    assert st.cards[target].damage == 2
    attack(st, attackers[1], target)
    pass_all(st)
    assert st.cards[target].damage == 4
    assert zone_of(st, target) is Zone.BATTLE
    attack(st, attackers[2], target)
    pass_all(st)
    assert zone_of(st, target) is Zone.TRASH


@pytest.mark.rule("8-5-3-2-1")
@pytest.mark.faq("Q99")
def test_unit_damage_counters_remain_after_the_turn() -> None:
    sc = Scenario()
    attacker = sc.add(0, VANILLA_2_2)
    target = sc.add(1, VANILLA_4_5, rested=True)
    st = sc.start()
    attack(st, attacker, target)
    pass_all(st)
    to_next_turn(st)
    assert st.active == 1
    assert st.cards[target].damage == 2


@pytest.mark.rule("8-5-3-2-2")
@pytest.mark.faq("Q62")
def test_first_strike_attacker_takes_no_damage_from_a_destroyed_target() -> None:
    sc = Scenario()
    striker = sc.add(0, FIRST_STRIKE_5_6)
    target = sc.add(1, CB_BLOCKER_5_4, rested=True)
    st = sc.start()
    attack(st, striker, target)
    pass_all(st)
    assert zone_of(st, target) is Zone.TRASH
    assert st.cards[striker].damage == 0


@pytest.mark.rule("8-5-3-2-2")
def test_first_strike_target_that_survives_strikes_back() -> None:
    sc = Scenario()
    striker = sc.add(0, FIRST_STRIKE_1_4)
    target = sc.add(1, VANILLA_2_2, rested=True)
    st = sc.start()
    attack(st, striker, target)
    pass_all(st)
    assert st.cards[target].damage == 1
    assert st.cards[striker].damage == 2


@pytest.mark.rule("8-5-3-2-2")
def test_first_strike_on_the_attack_target_does_not_apply() -> None:
    sc = Scenario()
    attacker = sc.add(0, VANILLA_3_1)
    target = sc.add(1, FIRST_STRIKE_1_4, rested=True)
    st = sc.start()
    attack(st, attacker, target)
    pass_all(st)
    assert zone_of(st, attacker) is Zone.TRASH
    assert st.cards[target].damage == 3


@pytest.mark.rule("8-5-3-2-3")
@pytest.mark.card("GD01-056")
def test_both_battling_units_are_destroyed_at_the_same_time() -> None:
    sc = Scenario()
    attacker = sc.add(0, DESTROYED_PING, damage=1)
    target = sc.add(1, DESTROYED_PING, rested=True, damage=1)
    mine = sc.add(0, VANILLA_2_2)
    theirs = sc.add(1, VANILLA_2_2)
    st = sc.start()
    attack(st, attacker, target)
    pass_all(st)
    assert zone_of(st, attacker) is Zone.TRASH
    assert zone_of(st, target) is Zone.TRASH
    # each 【Destroyed】 found the other battling Unit already gone and hit the bystander
    assert st.cards[theirs].damage == 1
    assert st.cards[mine].damage == 1
    assert _decider(st) == (DecisionKind.MAIN, 0)


@pytest.mark.rule("8-5-4")
@pytest.mark.card("GD01-059", "GD01-097")
def test_damage_step_triggers_resolve_before_the_battle_ends() -> None:
    sc = Scenario()
    zee = sc.add(0, ZEE_ZULU)
    burst, _ = sc.shields(1, BURST_ADD, VANILLA_2_2)
    st = sc.start()
    attack(st, zee, PLAYER_TARGET)
    pass_all(st)
    assert _decider(st) == (DecisionKind.BURST, 1)
    assert st.battle is not None
    assert ap(st, zee) == 4  # "during this battle" still applies: battle end not reached
    yes(st)
    assert zone_of(st, burst) is Zone.HAND
    assert st.battle is None
    assert ap(st, zee) == 2
    assert _decider(st) == (DecisionKind.MAIN, 0)


# ---------------------------------------------------------------------------------------------
# 8-6 Battle end step


@pytest.mark.rule("8-6-1")
@pytest.mark.card("GD04-113")
def test_during_this_battle_effect_does_not_reach_the_next_battle() -> None:
    sc = Scenario()
    first = sc.add(0, VANILLA_3_4)
    second = sc.add(0, VANILLA_3_4)
    target = sc.add(1, VANILLA_4_5, rested=True)
    sc.resources(1, 3)
    shield_cmd = sc.add(1, REDUCE_3_BATTLE, Zone.HAND)
    st = sc.start()
    attack(st, first, target)
    act(st, A.PLAY_COMMAND, shield_cmd)
    pass_all(st)
    assert st.cards[target].damage == 0
    attack(st, second, target)
    pass_all(st)
    assert st.cards[target].damage == 3


@pytest.mark.rule("8-6-1")
@pytest.mark.card("GD01-058")
def test_activated_during_this_battle_bonus_ends_with_the_battle() -> None:
    sc = Scenario()
    attacker = sc.add(0, VANILLA_3_4)
    galluss = sc.add(0, GALLUSS)
    sc.resources(0, 1)
    sc.resources(1, 2)
    sc.hand(1, PING_1)
    sc.shields(1, VANILLA_2_2, VANILLA_2_2)
    st = sc.start()
    attack(st, attacker, PLAYER_TARGET)
    pass_(st)
    activate(st, galluss)
    assert _decider(st) == (DecisionKind.ACTION_STEP, 1)
    assert ap(st, attacker) == 4
    pass_all(st)
    assert st.battle is None
    assert ap(st, attacker) == 3


@pytest.mark.rule("8-6-1")
@pytest.mark.card("GD01-058")
def test_during_this_battle_effect_outside_a_battle_does_not_outlast_the_turn() -> None:
    sc = Scenario()
    galluss = sc.add(1, GALLUSS)
    sc.resources(1, 1)
    big = sc.add(0, VANILLA_3_4)
    st = sc.start()
    end_main(st)
    activate(st, galluss)
    pass_all(st)
    assert (st.turn, st.active) == (4, 1)
    assert _decider(st) == (DecisionKind.MAIN, 1)
    assert ap(st, big) == 3


@pytest.mark.rule("8-6-2")
def test_after_the_battle_the_main_phase_resumes() -> None:
    sc = Scenario()
    first = sc.add(0, VANILLA_2_2)
    second = sc.add(0, VANILLA_2_2)
    sc.shields(1, VANILLA_2_2, VANILLA_2_2)
    st = sc.start()
    attack(st, first, PLAYER_TARGET)
    pass_all(st)
    assert _decider(st) == (DecisionKind.MAIN, 0)
    assert st.phase is Phase.MAIN and st.step is Step.MAIN
    assert st.battle is None
    assert has_action(st, A.ATTACK, second)
    assert has_action(st, A.END_MAIN)


# ---------------------------------------------------------------------------------------------
# 7-6-3 / 9 Action steps


@pytest.mark.rule("7-6-3-1", "9-1", "9-2")
def test_end_phase_action_step_starts_with_the_standby_player() -> None:
    sc = Scenario()
    sc.add(0, VANILLA_2_2)
    sc.add(1, VANILLA_2_2)
    sc.resources(0, 2)
    sc.resources(1, 2)
    mine = sc.hand(0, PING_1)
    theirs = sc.hand(1, PING_1)
    st = sc.start()
    end_main(st)
    assert st.phase is Phase.END and st.step is Step.END_ACTION
    assert _decider(st) == (DecisionKind.ACTION_STEP, 1)
    assert has_action(st, A.PLAY_COMMAND, theirs[0])
    pass_(st)
    assert _decider(st) == (DecisionKind.ACTION_STEP, 0)
    assert has_action(st, A.PLAY_COMMAND, mine[0])


@pytest.mark.rule("9-1")
def test_action_steps_occur_only_after_the_block_step_and_in_the_end_phase() -> None:
    sc = Scenario()
    attacker = sc.add(0, VANILLA_2_2)
    sc.add(1, BLOCKER_3_4)
    sc.resources(0, 2)
    sc.resources(1, 2)
    sc.hand(0, PING_1, PING_1)
    sc.hand(1, PING_1, PING_1)
    sc.shields(0, VANILLA_2_2)
    sc.shields(1, VANILLA_2_2)
    st = sc.start()
    assert A.PLAY_COMMAND in legal_kinds(st)  # 【Main】/【Action】 is a main-phase play here
    attack(st, attacker, PLAYER_TARGET)
    assert _decider(st) == (DecisionKind.BLOCK, 1)
    assert legal_kinds(st) == {A.BLOCK, A.NO_BLOCK}
    block(st, None)
    action_steps: list[Step] = []
    for _ in range(200):
        dec = st.pending
        if dec is None or (dec.kind is DecisionKind.MAIN and st.turn > 3):
            break
        if dec.kind is DecisionKind.ACTION_STEP:
            action_steps.append(st.step)
            pass_(st)
        elif dec.kind is DecisionKind.MAIN:
            end_main(st)
        else:
            raise AssertionError(f"unexpected decision {dec.kind}")
    assert st.turn == 4
    assert set(action_steps) == {Step.BATTLE_ACTION, Step.END_ACTION}


@pytest.mark.rule("9-3", "9-4")
@pytest.mark.card("GD01-058")
@pytest.mark.faq("Q38")
def test_action_step_choices_are_action_commands_activate_action_or_pass() -> None:
    sc = Scenario()
    attacker = sc.add(0, VANILLA_3_4)
    sc.resources(0, 3)
    my_ping = sc.add(0, PING_1, Zone.HAND)
    my_galluss = sc.add(0, GALLUSS)
    sc.resources(1, 3)
    their_ping = sc.add(1, PING_1, Zone.HAND)
    their_draw = sc.add(1, DRAW_MAIN_ONLY, Zone.HAND)
    their_bounce = sc.add(1, BOUNCE_ACTION, Zone.HAND)
    their_unit_card = sc.add(1, VANILLA_2_2, Zone.HAND)
    their_galluss = sc.add(1, GALLUSS)
    sc.shields(1, VANILLA_2_2)
    st = sc.start()
    attack(st, attacker, PLAYER_TARGET)
    assert _decider(st) == (DecisionKind.ACTION_STEP, 1)
    assert legal_kinds(st) == {A.PLAY_COMMAND, A.ACTIVATE, A.PASS}
    played = {o.a for o in options(st) if o.kind is A.PLAY_COMMAND}
    assert played == {their_ping, their_bounce}
    assert their_draw not in played and their_unit_card not in played
    assert {o.a for o in options(st) if o.kind is A.ACTIVATE} == {their_galluss}
    pass_(st)
    assert _decider(st) == (DecisionKind.ACTION_STEP, 0)
    assert legal_kinds(st) == {A.PLAY_COMMAND, A.ACTIVATE, A.PASS}
    assert has_action(st, A.PLAY_COMMAND, my_ping)
    assert has_action(st, A.ACTIVATE, my_galluss)


@pytest.mark.rule("9-3-1")
@pytest.mark.faq("Q91")
def test_playing_an_action_command_pays_its_cost_and_resolves_it() -> None:
    sc = Scenario()
    attacker = sc.add(0, VANILLA_3_4)
    res = sc.resources(1, 2)
    ping = sc.add(1, PING_1, Zone.HAND)
    sc.shields(1, VANILLA_2_2)
    st = sc.start()
    attack(st, attacker, PLAYER_TARGET)
    act(st, A.PLAY_COMMAND, ping)
    assert sum(st.cards[r].rested for r in res) == 1
    assert zone_of(st, ping) is Zone.TRASH
    assert st.cards[attacker].damage == 1


@pytest.mark.rule("9-3-1")
def test_action_command_needs_its_level_and_an_affordable_cost() -> None:
    low = Scenario()
    low.add(0, VANILLA_3_4)
    low.resources(1, 1)
    low.hand(1, PING_1)
    low.shields(1, VANILLA_2_2)
    st = low.start()
    attack(st, uids_in(st, 0, Zone.BATTLE)[0], PLAYER_TARGET)
    assert _decider(st) == (DecisionKind.MAIN, 0)  # Lv.2 card, 1 Resource: no action step choice

    tapped = Scenario()
    tapped.add(0, VANILLA_3_4)
    tapped.resources(1, 2, rested=2)
    tapped.hand(1, PING_1)
    tapped.shields(1, VANILLA_2_2)
    st = tapped.start()
    attack(st, uids_in(st, 0, Zone.BATTLE)[0], PLAYER_TARGET)
    assert _decider(st) == (DecisionKind.MAIN, 0)  # no active Resource to pay the cost


@pytest.mark.rule("9-3-2")
@pytest.mark.card("GD01-058")
@pytest.mark.faq("Q71")
def test_activate_action_effect_pays_its_cost_and_resolves() -> None:
    sc = Scenario()
    attacker = sc.add(0, VANILLA_3_4)
    galluss = sc.add(1, GALLUSS)
    res = sc.resources(1, 1)
    sc.resources(0, 2)
    sc.hand(0, PING_1)
    sc.shields(1, VANILLA_2_2)
    st = sc.start()
    attack(st, attacker, PLAYER_TARGET)
    assert has_action(st, A.ACTIVATE, galluss)
    activate(st, galluss)
    assert _decider(st) == (DecisionKind.ACTION_STEP, 0)
    assert st.cards[res[0]].rested
    assert ap(st, attacker) == 4


@pytest.mark.rule("9-3-2")
@pytest.mark.card("GD01-058")
def test_activate_action_effect_is_unavailable_without_its_cost() -> None:
    sc = Scenario()
    attacker = sc.add(0, VANILLA_3_4)
    sc.add(1, GALLUSS)
    sc.resources(1, 1, rested=1)
    sc.shields(1, VANILLA_2_2)
    st = sc.start()
    attack(st, attacker, PLAYER_TARGET)
    assert _decider(st) == (DecisionKind.MAIN, 0)


@pytest.mark.rule("9-3-3")
def test_passing_hands_the_right_to_act_to_the_opponent() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    sc.resources(1, 2)
    mine = sc.hand(0, PING_1)
    theirs = sc.hand(1, PING_1)
    sc.add(0, VANILLA_2_2)
    sc.add(1, VANILLA_2_2)
    st = sc.start()
    end_main(st)
    assert _decider(st) == (DecisionKind.ACTION_STEP, 1)
    pass_(st)
    assert _decider(st) == (DecisionKind.ACTION_STEP, 0)
    assert zone_of(st, theirs[0]) is Zone.HAND
    assert zone_of(st, mine[0]) is Zone.HAND


@pytest.mark.rule("9-4-1")
def test_an_action_by_the_active_player_returns_the_right_to_the_standby_player() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    sc.resources(1, 4)
    mine = sc.hand(0, PING_1, PING_1)
    theirs = sc.hand(1, PING_1)
    sc.add(0, VANILLA_4_5)
    sc.add(1, VANILLA_4_5)
    st = sc.start()
    end_main(st)
    pass_(st)
    assert _decider(st) == (DecisionKind.ACTION_STEP, 0)
    act(st, A.PLAY_COMMAND, mine[0])
    assert _decider(st) == (DecisionKind.ACTION_STEP, 1)
    assert has_action(st, A.PLAY_COMMAND, theirs[0])


@pytest.mark.rule("9-4-2", "9-5")
def test_turns_alternate_until_both_players_pass_consecutively() -> None:
    sc = Scenario()
    attacker = sc.add(0, VANILLA_3_4)
    sc.add(1, VANILLA_4_5, rested=True)
    sc.resources(0, 4)
    sc.resources(1, 4)
    mine = sc.hand(0, PING_1, PING_1)
    theirs = sc.hand(1, PING_1, PING_1)
    (shield,) = sc.shields(1, VANILLA_2_2)
    st = sc.start()
    attack(st, attacker, PLAYER_TARGET)
    deciders: list[int] = []
    for move in ("pass", mine[0], theirs[0], "pass", "pass"):
        assert st.pending is not None and st.pending.kind is DecisionKind.ACTION_STEP
        deciders.append(st.pending.player)
        if move == "pass":
            pass_(st)
        else:
            act(st, A.PLAY_COMMAND, move)
    assert deciders == [1, 0, 1, 0, 1]
    assert zone_of(st, shield) is Zone.TRASH  # the action step ended; damage was dealt
    assert _decider(st) == (DecisionKind.MAIN, 0)


@pytest.mark.rule("9-5")
def test_a_single_pass_does_not_end_the_action_step() -> None:
    sc = Scenario()
    attacker = sc.add(0, VANILLA_3_4)
    sc.add(1, VANILLA_4_5, rested=True)
    sc.resources(0, 2)
    sc.resources(1, 2)
    sc.hand(0, PING_1)
    sc.hand(1, PING_1)
    (shield,) = sc.shields(1, VANILLA_2_2)
    st = sc.start()
    attack(st, attacker, PLAYER_TARGET)
    pass_(st)
    assert st.step is Step.BATTLE_ACTION
    assert zone_of(st, shield) is Zone.SHIELD
    pass_(st)
    assert zone_of(st, shield) is Zone.TRASH


@pytest.mark.rule("9-5")
def test_end_phase_action_step_ends_after_two_consecutive_passes() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    sc.resources(1, 4)
    mine = sc.hand(0, PING_1, PING_1)
    sc.hand(1, PING_1, PING_1)
    sc.add(0, VANILLA_4_5)
    sc.add(1, VANILLA_4_5)
    st = sc.start()
    end_main(st)
    pass_(st)
    act(st, A.PLAY_COMMAND, mine[0])
    pass_(st)
    assert st.step is Step.END_ACTION
    pass_(st)
    assert st.active == 1  # the end phase finished and the turn passed
    assert _decider(st) == (DecisionKind.MAIN, 1)
