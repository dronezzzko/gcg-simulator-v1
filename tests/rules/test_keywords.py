"""Rules section 13: keyword effects (13-1) and keywords (13-2)."""

from __future__ import annotations

import pytest

from gcg_sim.cards.model import CardType
from gcg_sim.effects import dsl as d
from gcg_sim.engine import interp
from gcg_sim.engine import view as V
from gcg_sim.engine.state import GameState
from gcg_sim.engine.types import (
    PLAYER_TARGET,
    ActionKind,
    DecisionKind,
    Duration,
    Step,
    Zone,
)
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
    options,
    pass_,
    pass_all,
    play,
    select,
    select_if_asked,
    to_next_turn,
    yes,
    yes_if_asked,
    zone_of,
)

A = ActionKind

VANILLA = "GD01-060"  # Zaku Mariner, Lv2 C1 2/2
VANILLA_3_3 = "GD02-015"  # Marasai, Lv3 C2 3/3
VANILLA_3_4 = "GD01-013"  # Gundam, Lv4 C2 3/4
VANILLA_6_4 = "ST14-008"  # Gundam Heavyarms Custom (EW), Lv6 C4 6/4

REPAIR_1 = "GD01-017"  # Stark Jegan, <Repair 1>, 3/3
REPAIR_GRANT = "GD04-103"  # Spiritual Support: 【Main】 one of your Units gains <Repair 2>
PAIR_REPAIR_2 = "GD03-008"  # Bolinoak Sammahn (blue): 【During Pair】 gains <Repair 2>
SAYLA = "GD01-087"  # Pilot: while this Unit is blue, it gains <Repair 1>; 【Burst】 add to hand
BREACH_2 = "GD01-030"  # Rick Dom (Zeon), <Breach 2>, 3/3
MQUVE = "GD01-092"  # Pilot M'Quve: while this Unit is (Zeon), it gains <Breach 1>
AMURO_RECOVER = "GD05-085"  # Pilot: during your turn, when this Unit destroys ... recovers 2 HP
BASE_0_6 = "GD01-126"  # Underground Desert Base 0/6: 【Burst】 deploy; 【Deploy】 Shield to hand
SUPPORT_2 = "GD01-055"  # BuCUE, 【Activate･Main】<Support 2>, 2/3
SUPPORT_1 = "GD01-061"  # ZuOOT, 【Activate･Main】<Support 1>, 0/2
NENA = "GD04-089"  # Pilot Nena Trinity: 【Activate･Main】<Support 2>
BLOCKER = "GD01-072"  # Launcher Strike Gundam (white), <Blocker>, 3/4
BLOCKER_2_1 = "ST02-008"  # Aries, <Blocker>, 2/1
CAGALLI = "GD01-096"  # Pilot: while this Unit is white, it gains <Blocker>
FIRST_STRIKE_CMD = "ST04-014"  # 【Main】/【Action】 Lv.2- Unit gains <First Strike>; 【Pilot】
HIGH_MANEUVER = "GD01-024"  # Wing Gundam Zero, <High-Maneuver>, 5/7
HM_GRANTER = "GD02-042"  # Gundam Ashtaron (MA Mode): 【Deploy】 a (New UNE) Unit gains <HM>
SUPPRESSION = "GD03-034"  # GQuuuuuuX (Omega Psycommu), <Suppression>, 6/5
BANSHEE = "ST12-006"  # 【Activate･Action】 exile 4 from trash: gains <Suppression> this battle
NU_GUNDAM = "GD05-017"  # 【When Paired】 ... begin a battle ... only perform the damage step
LONDO_BELL = ("GD05-019", "GD05-023", "GD05-029")
RIDDHE = "GD01-089"  # Pilot (Earth Federation): while this Unit has <Repair>, AP+1
DEVELOPMENT = "EB01-008"  # Gundam Delta Kai: 【Deploy･Development 1】 ■ recover 2
G_GENERATION = "EB01-011"  # Beginning Gundam, a (G Generation) card

TURN_A = "GD04-073"  # ∀ Gundam: 【Activate･Main】【Once per Turn】①: this Unit gets AP+2
GALLUSS = "GD01-058"  # Galluss-K: 【Activate･Action】【Once per Turn】①: Lv.4+ Unit AP+1
MAIN_ONLY = "GD01-100"  # A Show of Resolve: 【Main】 Draw 2
MAIN_ACTION = "GD01-115"  # Zeon Remnant Forces: 【Main】/【Action】 1 damage to an enemy Unit
ACTION_PILOT = "GD01-114"  # Assault on Torrington Base: 【Action】 2 friendly Units AP+1; 【Pilot】
BURST_ADD = "GD01-097"  # Guel Jeturk (Pilot): 【Burst】 add this card to your hand
SIEGE_PLOY = "ST02-014"  # 【Burst】 activate this card's 【Main】: rest an enemy Unit (5- HP)
DEPLOY_SELF_DAMAGE = "GD02-068"  # Barbatos 3rd Form: 【Deploy】 deal 2 damage to this Unit, 3/5
ATTACK_AP = "ST03-008"  # Zaku II: 【Attack】 this Unit gets AP+2 during this turn, 1/2
ATTACK_PILOT = "ST04-010"  # Pilot Kira Yamato: 【Attack】 an enemy Unit gets AP-2 this battle
DESTROYED_EX = "ST14-009"  # Duel Gundam (Assault Shroud): 【Destroyed】 place 1 EX Resource, 3/2
DESTROYED_DAMAGE = "GD01-056"  # Geara Doga (Sleeves): 【Destroyed】 1 damage to enemy (5- AP), 2/3
DESTROY_CMD = "ST09-009"  # Giant Killing: destroy an active enemy Unit with 4 or less AP
BOUNCE_CMD = "ST04-013"  # Hawk of Endymion: return an enemy Unit with 3 or less HP to hand
UNICORN = "GD01-005"  # 【During Link】【Destroyed】 return paired Pilot to hand, then discard 1
BANAGHER = "GD01-088"  # Pilot Banagher Links (Unicorn's link Pilot)
WHEN_PAIRED = "GD03-043"  # Messer Type-F02: 【When Paired】 1 damage to an enemy Unit, 3/2
DEATHSCYTHE = "GD01-025"  # 【When Paired･(Operation Meteor) Pilot】 rested Resource, <First Strike>
HEERO = "ST02-010"  # Pilot Heero Yuy (Operation Meteor)
HEAVYARMS = "GD01-034"  # 【During Pair】 this Unit gains <Breach 3>
QUBELEY = "GD02-036"  # 【During Pair･(Neo Zeon) Pilot】【Attack】 2 damage to a damaged enemy Unit
MARIDA = "GD01-093"  # Pilot Marida Cruz (Neo Zeon)
GYAN = "ST12-007"  # 【When Linked】 this Unit gains <First Strike> during this turn; link M'Quve
LINK_HM = "GD04-008"  # Gundam: 【During Link】 this Unit gains <High-Maneuver>; link Amuro Ray
PHARACT = "GD01-071"  # 【During Link】【Attack】 an enemy Unit gets AP-2 during this battle
GUEL = "GD01-097"  # Pilot Guel Jeturk (Academy): Pharact's link trait
RAIDER = "GD02-010"  # 【Once per Turn】 when this Unit receives enemy effect damage, draw 1, 4/4


def _pending(st: GameState) -> DecisionKind | None:
    return st.pending.kind if st.pending else None


def _option_args(st: GameState, kind: ActionKind) -> set[int]:
    return {o.a for o in options(st) if o.kind is kind}


def _grant(st: GameState, uid: int, keyword: d.Kw) -> None:
    """Give ``uid`` a keyword for this turn, as a resolved "gains <keyword>" effect does.

    No implemented card can give <First Strike> to a Unit that begins a battle by effect."""
    owner = st.cards[uid].owner
    targets = ((uid, st.cards[uid].zone_seq),)
    interp.add_lasting(st, d.KeywordGrant(keyword), owner, uid, targets, Duration.THIS_TURN)


# ---------------------------------------------------------------------------------------------
# 13-1-1 <Repair>


@pytest.mark.rule("13-1-1-1")
def test_repair_recovers_at_the_end_of_your_turn_only() -> None:
    sc = Scenario()
    mine = sc.add(0, REPAIR_1, damage=2)
    theirs = sc.add(1, REPAIR_1, damage=2)
    st = sc.start()
    assert keywords(st, mine) == {"Repair": 1}
    to_next_turn(st)
    assert st.active == 1
    assert st.cards[mine].damage == 1
    assert st.cards[theirs].damage == 2
    to_next_turn(st)
    assert st.active == 0
    assert st.cards[mine].damage == 1
    assert st.cards[theirs].damage == 1


@pytest.mark.rule("13-1-1-2")
def test_gaining_repair_adds_to_the_existing_amount() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    jegan = sc.add(0, REPAIR_1)
    grant = sc.add(0, REPAIR_GRANT, Zone.HAND)
    st = sc.start()
    play(st, grant)
    assert keywords(st, jegan) == {"Repair": 3}


@pytest.mark.rule("13-1-1-2", "13-1-1-1")
def test_combined_repair_recovers_the_summed_amount() -> None:
    sc = Scenario()
    unit = sc.add(0, PAIR_REPAIR_2, pilot=SAYLA, damage=3)
    st = sc.start()
    assert keywords(st, unit) == {"Repair": 3}
    assert hp(st, unit) == 4
    to_next_turn(st)
    assert st.cards[unit].damage == 0


# ---------------------------------------------------------------------------------------------
# 13-1-2 <Breach>


@pytest.mark.rule("13-1-2-1", "13-1-2-2")
def test_breach_hits_the_top_shield_when_there_is_no_base() -> None:
    sc = Scenario()
    rick = sc.add(0, BREACH_2)
    victim = sc.add(1, VANILLA, rested=True)
    top, second = sc.shields(1, VANILLA, VANILLA)
    st = sc.start()
    attack(st, rick, victim)
    pass_all(st)
    assert zone_of(st, victim) is Zone.TRASH
    assert zone_of(st, top) is Zone.TRASH
    assert zone_of(st, second) is Zone.SHIELD


@pytest.mark.rule("13-1-2-2")
def test_breach_damages_the_base_before_any_shield() -> None:
    sc = Scenario()
    rick = sc.add(0, BREACH_2)
    victim = sc.add(1, VANILLA, rested=True)
    base = sc.base(1, BASE_0_6)
    shields = sc.shields(1, VANILLA, VANILLA)
    st = sc.start()
    attack(st, rick, victim)
    pass_all(st)
    assert st.cards[base].damage == 2
    assert all(zone_of(st, s) is Zone.SHIELD for s in shields)


@pytest.mark.rule("13-1-2-1")
def test_breach_activates_when_destroying_a_blocker() -> None:
    sc = Scenario()
    rick = sc.add(0, BREACH_2)
    blocker = sc.add(1, BLOCKER_2_1)
    top, second = sc.shields(1, VANILLA, VANILLA)
    st = sc.start()
    attack(st, rick, PLAYER_TARGET)
    block(st, blocker)
    pass_all(st)
    assert zone_of(st, blocker) is Zone.TRASH
    assert zone_of(st, top) is Zone.TRASH
    assert zone_of(st, second) is Zone.SHIELD


@pytest.mark.rule("13-1-2-1")
def test_breach_does_not_activate_during_the_opponents_turn() -> None:
    sc = Scenario(active=1)
    rick = sc.add(0, BREACH_2, rested=True)
    attacker = sc.add(1, VANILLA)
    shields = sc.shields(1, VANILLA, VANILLA)
    st = sc.start()
    attack(st, attacker, rick)
    pass_all(st)
    assert zone_of(st, attacker) is Zone.TRASH
    assert zone_of(st, rick) is Zone.BATTLE
    assert all(zone_of(st, s) is Zone.SHIELD for s in shields)


@pytest.mark.rule("13-1-2-1")
def test_breach_does_not_activate_when_destroying_a_base() -> None:
    sc = Scenario()
    rick = sc.add(0, BREACH_2)
    sc.base(1, damage=2)
    shields = sc.shields(1, VANILLA, VANILLA)
    st = sc.start()
    attack(st, rick, PLAYER_TARGET)
    pass_all(st)
    assert st.zones[1][Zone.BASE] == []
    assert all(zone_of(st, s) is Zone.SHIELD for s in shields)


@pytest.mark.rule("13-1-2-3")
def test_breach_activates_when_both_units_are_destroyed() -> None:
    sc = Scenario()
    rick = sc.add(0, BREACH_2)
    enemy = sc.add(1, BREACH_2, rested=True)
    (mine,) = sc.shields(0, VANILLA)
    top, second = sc.shields(1, VANILLA, VANILLA)
    st = sc.start()
    attack(st, rick, enemy)
    pass_all(st)
    assert zone_of(st, rick) is Zone.TRASH
    assert zone_of(st, enemy) is Zone.TRASH
    assert zone_of(st, top) is Zone.TRASH
    assert zone_of(st, second) is Zone.SHIELD
    assert zone_of(st, mine) is Zone.SHIELD


def _breach_with_second_trigger(shield_area: bool) -> GameState:
    """Rick Dom paired with Amuro Ray: destroying a Unit triggers Amuro's recovery and,
    when it activates, <Breach>. Two simultaneous triggers of one player need an order."""
    sc = Scenario()
    rick = sc.add(0, BREACH_2, pilot=AMURO_RECOVER)
    victim = sc.add(1, VANILLA, rested=True)
    if shield_area:
        sc.shields(1, VANILLA, VANILLA)
    st = sc.start()
    attack(st, rick, victim)
    pass_all(st)
    assert zone_of(st, victim) is Zone.TRASH
    return st


@pytest.mark.rule("13-1-2-4")
def test_breach_does_not_activate_with_an_empty_shield_area() -> None:
    st = _breach_with_second_trigger(shield_area=False)
    assert _pending(st) is DecisionKind.MAIN
    assert st.winner is None
    rick = st.zones[0][Zone.BATTLE][0]
    assert st.cards[rick].damage == 0


@pytest.mark.rule("13-1-2-4", "13-1-2-1")
def test_breach_activates_alongside_other_triggers_when_the_shield_area_has_cards() -> None:
    st = _breach_with_second_trigger(shield_area=True)
    assert _pending(st) is DecisionKind.ORDER_TRIGGER
    assert st.pending is not None and st.pending.player == 0
    assert len(options(st)) == 2


@pytest.mark.rule("13-1-2-5")
def test_gaining_breach_adds_to_the_existing_amount() -> None:
    sc = Scenario()
    rick = sc.add(0, BREACH_2, pilot=MQUVE)
    victim = sc.add(1, VANILLA, rested=True)
    base = sc.base(1, BASE_0_6)
    st = sc.start()
    assert keywords(st, rick) == {"Breach": 3}
    attack(st, rick, victim)
    pass_all(st)
    assert st.cards[base].damage == 3


# ---------------------------------------------------------------------------------------------
# 13-1-3 <Support>


@pytest.mark.rule("13-1-3-1")
def test_support_rests_the_unit_and_boosts_another_friendly_unit_this_turn() -> None:
    sc = Scenario()
    bucue = sc.add(0, SUPPORT_2)
    ally = sc.add(0, VANILLA)
    enemy = sc.add(1, VANILLA)
    st = sc.start()
    assert keywords(st, bucue) == {"Support": 2}
    activate(st, bucue)
    assert st.cards[bucue].rested
    assert ap(st, ally) == 4
    assert ap(st, bucue) == 2
    assert ap(st, enemy) == 2
    assert not has_action(st, A.ACTIVATE, bucue)
    to_next_turn(st)
    assert ap(st, ally) == 2


@pytest.mark.rule("13-1-3-1")
def test_support_chooses_only_another_friendly_unit() -> None:
    sc = Scenario()
    bucue = sc.add(0, SUPPORT_2)
    first = sc.add(0, VANILLA)
    second = sc.add(0, VANILLA)
    sc.add(1, VANILLA)
    st = sc.start()
    activate(st, bucue)
    assert _pending(st) is DecisionKind.SELECT
    assert _option_args(st, A.SELECT) == {first, second}
    select(st, second)
    assert ap(st, second) == 4
    assert ap(st, first) == 2


@pytest.mark.rule("13-1-3-1")
def test_support_needs_another_friendly_unit() -> None:
    sc = Scenario()
    bucue = sc.add(0, SUPPORT_2)
    sc.add(1, VANILLA)
    st = sc.start()
    assert not has_action(st, A.ACTIVATE, bucue)


@pytest.mark.rule("13-1-3-2")
def test_gaining_support_adds_to_the_existing_amount() -> None:
    sc = Scenario()
    zuoot = sc.add(0, SUPPORT_1, pilot=NENA)
    ally = sc.add(0, VANILLA)
    st = sc.start()
    assert keywords(st, zuoot) == {"Support": 3}
    assert sum(1 for o in options(st) if o.kind is A.ACTIVATE and o.a == zuoot) == 1
    activate(st, zuoot)
    assert ap(st, ally) == 5


# ---------------------------------------------------------------------------------------------
# 13-1-4 <Blocker>


@pytest.mark.rule("13-1-4-1")
def test_blocker_rests_and_becomes_the_attack_target() -> None:
    sc = Scenario(active=1)
    attacker = sc.add(1, VANILLA)
    blocker = sc.add(0, BLOCKER)
    sc.add(0, VANILLA)
    (shield,) = sc.shields(0, VANILLA)
    sc.resources(0, 2)
    sc.hand(0, MAIN_ACTION)
    st = sc.start()
    attack(st, attacker)
    assert _pending(st) is DecisionKind.BLOCK
    assert st.pending is not None and st.pending.player == 0
    assert _option_args(st, A.BLOCK) == {blocker}
    block(st, blocker)
    assert st.cards[blocker].rested
    assert _pending(st) is DecisionKind.ACTION_STEP
    assert st.battle is not None and st.battle.target == blocker
    pass_all(st)
    assert zone_of(st, attacker) is Zone.TRASH
    assert st.cards[blocker].damage == 2
    assert zone_of(st, shield) is Zone.SHIELD


@pytest.mark.rule("13-1-4-1")
def test_rested_blocker_cannot_block() -> None:
    sc = Scenario(active=1)
    attacker = sc.add(1, VANILLA)
    sc.add(0, BLOCKER, rested=True)
    (shield,) = sc.shields(0, VANILLA)
    st = sc.start()
    attack(st, attacker)
    assert _pending(st) is DecisionKind.MAIN
    assert zone_of(st, shield) is Zone.TRASH


@pytest.mark.rule("13-1-4-2")
def test_blocker_is_not_given_twice() -> None:
    sc = Scenario(active=1)
    attacker = sc.add(1, VANILLA)
    blocker = sc.add(0, BLOCKER, pilot=CAGALLI)
    sc.shields(0, VANILLA)
    st = sc.start()
    assert keywords(st, blocker) == {"Blocker": 1}
    attack(st, attacker)
    assert [o.a for o in options(st) if o.kind is A.BLOCK] == [blocker]


# ---------------------------------------------------------------------------------------------
# 13-1-5 <First Strike>


def _first_strike_attacker(target: str) -> tuple[GameState, int, int]:
    sc = Scenario()
    sc.resources(0, 3)
    attacker = sc.add(0, VANILLA)
    enemy = sc.add(1, target, rested=True)
    bullet = sc.add(0, FIRST_STRIKE_CMD, Zone.HAND)
    st = sc.start()
    play(st, bullet)
    assert keywords(st, attacker) == {"First Strike": 1}
    attack(st, attacker, enemy)
    pass_all(st)
    return st, attacker, enemy


@pytest.mark.rule("13-1-5-1", "13-1-5-2")
def test_first_strike_destroys_the_target_before_it_deals_damage() -> None:
    st, attacker, enemy = _first_strike_attacker(VANILLA)
    assert zone_of(st, enemy) is Zone.TRASH
    assert zone_of(st, attacker) is Zone.BATTLE
    assert st.cards[attacker].damage == 0


@pytest.mark.rule("13-1-5-2")
def test_surviving_target_deals_its_damage_after_first_strike() -> None:
    st, attacker, enemy = _first_strike_attacker(VANILLA_3_3)
    assert zone_of(st, enemy) is Zone.BATTLE
    assert st.cards[enemy].damage == 2
    assert zone_of(st, attacker) is Zone.TRASH


@pytest.mark.rule("13-1-5-1")
def test_first_strike_does_not_apply_to_the_unit_being_attacked() -> None:
    sc = Scenario(active=1)
    sc.resources(0, 3)
    sc.resources(1, 2)
    attacker = sc.add(1, VANILLA)
    defender = sc.add(0, VANILLA, rested=True)
    bullet = sc.add(0, FIRST_STRIKE_CMD, Zone.HAND)
    sc.hand(1, MAIN_ACTION)
    st = sc.start()
    attack(st, attacker, defender)
    assert _pending(st) is DecisionKind.ACTION_STEP
    assert st.pending is not None and st.pending.player == 0
    act(st, A.PLAY_COMMAND, bullet)
    assert st.pending is not None and st.pending.player == 1
    assert keywords(st, defender) == {"First Strike": 1}
    pass_all(st)
    assert zone_of(st, attacker) is Zone.TRASH
    assert zone_of(st, defender) is Zone.TRASH


@pytest.mark.rule("13-1-5-3")
def test_first_strike_is_not_given_twice() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    unit = sc.add(0, VANILLA)
    first, second = sc.hand(0, FIRST_STRIKE_CMD, FIRST_STRIKE_CMD)
    st = sc.start()
    play(st, first)
    play(st, second)
    assert zone_of(st, second) is Zone.TRASH
    assert keywords(st, unit) == {"First Strike": 1}


def _damage_step_battle(first_strike: bool) -> tuple[GameState, int, int]:
    sc = Scenario()
    sc.resources(0, 3)
    nu = sc.add(0, NU_GUNDAM)
    trash = sc.trash(0, *LONDO_BELL)
    pilot = sc.add(0, RIDDHE, Zone.HAND)
    enemy = sc.add(1, VANILLA_6_4)
    st = sc.start()
    if first_strike:
        _grant(st, nu, d.Kw.FIRST_STRIKE)
    play(st, pilot, onto=nu)
    yes_if_asked(st)
    select_if_asked(st, *trash)
    assert all(zone_of(st, u) is Zone.REMOVAL for u in trash)
    return st, nu, enemy


@pytest.mark.rule("13-1-5-4")
def test_first_strike_applies_in_a_battle_that_only_performs_the_damage_step() -> None:
    st, nu, enemy = _damage_step_battle(first_strike=True)
    assert zone_of(st, enemy) is Zone.TRASH
    assert zone_of(st, nu) is Zone.BATTLE
    assert st.cards[nu].damage == 0


@pytest.mark.rule("13-1-5-4")
def test_damage_step_battle_without_first_strike_is_simultaneous() -> None:
    st, nu, enemy = _damage_step_battle(first_strike=False)
    assert zone_of(st, enemy) is Zone.TRASH
    assert zone_of(st, nu) is Zone.TRASH


# ---------------------------------------------------------------------------------------------
# 13-1-6 <High-Maneuver>


@pytest.mark.rule("13-1-6-1")
def test_high_maneuver_attacker_cannot_be_blocked() -> None:
    sc = Scenario()
    wing = sc.add(0, HIGH_MANEUVER)
    sc.add(1, BLOCKER)
    top, second = sc.shields(1, VANILLA, VANILLA)
    st = sc.start()
    attack(st, wing)
    assert _pending(st) is DecisionKind.MAIN
    assert zone_of(st, top) is Zone.TRASH
    assert zone_of(st, second) is Zone.SHIELD


@pytest.mark.rule("13-1-6-1")
def test_attacker_without_high_maneuver_can_be_blocked() -> None:
    sc = Scenario()
    zaku = sc.add(0, VANILLA)
    sc.add(0, HIGH_MANEUVER)
    blocker = sc.add(1, BLOCKER)
    sc.shields(1, VANILLA)
    st = sc.start()
    attack(st, zaku)
    assert _pending(st) is DecisionKind.BLOCK
    assert _option_args(st, A.BLOCK) == {blocker}


@pytest.mark.rule("13-1-6-1")
def test_high_maneuver_only_restricts_enemy_blockers_while_attacking() -> None:
    sc = Scenario(active=1)
    attacker = sc.add(1, VANILLA)
    sc.add(0, HIGH_MANEUVER)
    blocker = sc.add(0, BLOCKER)
    sc.shields(0, VANILLA)
    st = sc.start()
    attack(st, attacker)
    assert _pending(st) is DecisionKind.BLOCK
    assert _option_args(st, A.BLOCK) == {blocker}


@pytest.mark.rule("13-1-6-2")
def test_high_maneuver_is_not_given_twice() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    target = sc.add(0, HM_GRANTER)
    second, third = sc.hand(0, HM_GRANTER, HM_GRANTER)
    st = sc.start()
    play(st, second)
    select(st, target)
    play(st, third)
    select(st, target)
    assert keywords(st, target) == {"High-Maneuver": 1}
    assert keywords(st, second) == {}


# ---------------------------------------------------------------------------------------------
# 13-1-7 <Suppression>


@pytest.mark.rule("13-1-7-1")
def test_suppression_damages_the_first_two_shields() -> None:
    sc = Scenario()
    unit = sc.add(0, SUPPRESSION)
    first, second, third = sc.shields(1, VANILLA, VANILLA, VANILLA)
    st = sc.start()
    attack(st, unit)
    pass_all(st)
    assert zone_of(st, first) is Zone.TRASH
    assert zone_of(st, second) is Zone.TRASH
    assert zone_of(st, third) is Zone.SHIELD


@pytest.mark.rule("13-1-7-1")
def test_suppression_does_not_apply_while_a_base_takes_the_damage() -> None:
    sc = Scenario()
    unit = sc.add(0, SUPPRESSION)
    base = sc.base(1, BASE_0_6)
    shields = sc.shields(1, VANILLA, VANILLA)
    st = sc.start()
    attack(st, unit)
    pass_all(st)
    assert zone_of(st, base) is Zone.TRASH
    assert all(zone_of(st, s) is Zone.SHIELD for s in shields)


@pytest.mark.rule("13-1-7-2")
def test_suppression_is_not_given_twice() -> None:
    sc = Scenario()
    banshee = sc.add(0, BANSHEE)
    trash = sc.trash(0, *([VANILLA] * 8))
    sc.resources(1, 2)
    sc.add(1, MAIN_ACTION, Zone.HAND)
    shields = sc.shields(1, VANILLA, VANILLA, VANILLA, VANILLA)
    st = sc.start()
    attack(st, banshee)
    assert _pending(st) is DecisionKind.ACTION_STEP
    assert st.pending is not None and st.pending.player == 1
    pass_(st)
    activate(st, banshee)
    select(st, *trash[:4])
    assert keywords(st, banshee) == {"Suppression": 1}
    pass_(st)
    activate(st, banshee)
    assert all(zone_of(st, u) is Zone.REMOVAL for u in trash)
    assert st.pending is not None and st.pending.player == 1
    assert keywords(st, banshee) == {"Suppression": 1}
    pass_all(st)
    assert [zone_of(st, s) for s in shields] == [
        Zone.TRASH,
        Zone.TRASH,
        Zone.SHIELD,
        Zone.SHIELD,
    ]


@pytest.mark.rule("13-1-7-3")
def test_suppression_with_one_shield_damages_only_that_shield() -> None:
    sc = Scenario()
    unit = sc.add(0, SUPPRESSION)
    (shield,) = sc.shields(1, VANILLA)
    st = sc.start()
    attack(st, unit)
    pass_all(st)
    assert zone_of(st, shield) is Zone.TRASH
    assert st.winner is None
    assert _pending(st) is DecisionKind.MAIN


@pytest.mark.rule("13-1-7-4")
def test_suppression_reveals_both_shields_and_their_owner_orders_the_bursts() -> None:
    sc = Scenario()
    unit = sc.add(0, SUPPRESSION)
    guel, sayla, third = sc.shields(1, BURST_ADD, SAYLA, VANILLA)
    st = sc.start()
    attack(st, unit)
    pass_all(st)
    assert _pending(st) is DecisionKind.ORDER_TRIGGER
    assert st.pending is not None and st.pending.player == 1
    for uid in (guel, sayla):
        assert st.cards[uid].known == 0b11
        assert zone_of(st, uid) is not Zone.SHIELD
    assert st.zones[1][Zone.SHIELD] == [third]
    batch = st.batches[-1]
    pick = next(o for o in options(st) if batch[o.a].card_uid == sayla)
    act(st, A.ORDER, pick.a)
    assert _pending(st) is DecisionKind.BURST
    assert st.frames[-1].card_uid == sayla
    yes(st)
    assert zone_of(st, sayla) is Zone.HAND
    assert _pending(st) is DecisionKind.BURST
    assert st.frames[-1].card_uid == guel
    yes(st)
    assert zone_of(st, guel) is Zone.HAND
    assert zone_of(st, third) is Zone.SHIELD


# ---------------------------------------------------------------------------------------------
# 13-1-8 <Development>


def _development_scenario() -> tuple[GameState, int, int, int]:
    sc = Scenario()
    sc.resources(0, 5)
    damaged = sc.add(0, REPAIR_1, damage=2)
    (fuel,) = sc.trash(0, G_GENERATION)
    delta = sc.add(0, DEVELOPMENT, Zone.HAND)
    st = sc.start()
    play(st, delta)
    return st, damaged, fuel, delta


@pytest.mark.rule("13-1-8-1", "13-1-8-2")
def test_development_exiles_cards_then_performs_the_following_effect() -> None:
    st, damaged, fuel, delta = _development_scenario()
    assert _pending(st) is DecisionKind.YES_NO
    yes(st)
    assert zone_of(st, fuel) is Zone.REMOVAL
    assert _pending(st) is DecisionKind.SELECT
    select(st, damaged)
    assert st.cards[damaged].damage == 0
    assert zone_of(st, delta) is Zone.BATTLE


@pytest.mark.rule("13-1-8-1", "13-1-8-2")
def test_declining_development_skips_the_following_effect() -> None:
    st, damaged, fuel, _ = _development_scenario()
    no(st)
    assert zone_of(st, fuel) is Zone.TRASH
    assert st.cards[damaged].damage == 2
    assert _pending(st) is DecisionKind.MAIN


# ---------------------------------------------------------------------------------------------
# 13-2-1 【Activate･Main】 and 13-2-2 【Activate･Action】


@pytest.mark.rule("13-2-1-1")
def test_activate_main_only_in_your_main_phase_while_not_attacking() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    turn_a = sc.add(0, TURN_A)
    zaku = sc.add(0, VANILLA)
    sc.add(0, MAIN_ACTION, Zone.HAND)
    sc.add(1, VANILLA)
    sc.shields(1, VANILLA, VANILLA)
    st = sc.start()
    assert has_action(st, A.ACTIVATE, turn_a)
    attack(st, turn_a)
    assert _pending(st) is DecisionKind.ACTION_STEP
    assert st.pending is not None and st.pending.player == 0
    assert has_action(st, A.PLAY_COMMAND)
    assert not has_action(st, A.ACTIVATE, turn_a)
    pass_all(st)
    assert _pending(st) is DecisionKind.MAIN
    assert has_action(st, A.ACTIVATE, turn_a)
    activate(st, turn_a)
    assert ap(st, turn_a) == 5
    attack(st, zaku)
    assert _pending(st) is DecisionKind.ACTION_STEP
    assert not has_action(st, A.ACTIVATE)


@pytest.mark.rule("13-2-1-1", "13-1-3-1")
def test_activate_main_not_in_end_phase_or_opponents_turn() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    turn_a = sc.add(0, TURN_A)
    support = sc.add(0, SUPPORT_2)
    sc.add(0, MAIN_ACTION, Zone.HAND)
    sc.add(1, VANILLA)
    st = sc.start(Step.END_ACTION)
    assert _pending(st) is DecisionKind.ACTION_STEP
    assert st.pending is not None and st.pending.player == 0
    assert not has_action(st, A.ACTIVATE, turn_a)
    assert not has_action(st, A.ACTIVATE, support)
    pass_(st)
    assert st.active == 1
    assert _pending(st) is DecisionKind.MAIN
    assert not has_action(st, A.ACTIVATE, turn_a)
    assert not has_action(st, A.ACTIVATE, support)


@pytest.mark.rule("13-2-2-1")
def test_activate_action_only_during_action_steps() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    galluss = sc.add(0, GALLUSS)
    target = sc.add(0, VANILLA_3_4)
    zaku = sc.add(0, VANILLA)
    sc.shields(1, VANILLA, VANILLA)
    sc.resources(1, 2)
    sc.hand(1, MAIN_ACTION)
    st = sc.start()
    assert not has_action(st, A.ACTIVATE, galluss)
    attack(st, zaku)
    assert _pending(st) is DecisionKind.ACTION_STEP
    assert st.pending is not None and st.pending.player == 1
    pass_(st)
    assert st.pending is not None and st.pending.player == 0
    activate(st, galluss)
    assert st.pending is not None and st.pending.player == 1
    assert ap(st, target) == 4


@pytest.mark.rule("13-2-2-1")
def test_activate_action_for_the_standby_player_and_in_the_end_phase() -> None:
    sc = Scenario(active=1)
    sc.resources(0, 3)
    galluss = sc.add(0, GALLUSS)
    sc.add(0, VANILLA_3_4)
    attacker = sc.add(1, VANILLA)
    sc.shields(0, VANILLA, VANILLA)
    st = sc.start()
    attack(st, attacker)
    assert _pending(st) is DecisionKind.ACTION_STEP
    assert st.pending is not None and st.pending.player == 0
    assert has_action(st, A.ACTIVATE, galluss)
    pass_all(st)
    assert _pending(st) is DecisionKind.MAIN
    end_main(st)
    assert _pending(st) is DecisionKind.ACTION_STEP
    assert st.pending is not None and st.pending.player == 0
    assert has_action(st, A.ACTIVATE, galluss)


# ---------------------------------------------------------------------------------------------
# 13-2-3 【Main】 and 13-2-4 【Action】


@pytest.mark.rule("13-2-3-1")
def test_main_command_is_played_in_your_main_phase() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    cmd = sc.add(0, MAIN_ONLY, Zone.HAND)
    st = sc.start()
    hand_before = len(st.zones[0][Zone.HAND])
    play(st, cmd)
    assert zone_of(st, cmd) is Zone.TRASH
    assert len(st.zones[0][Zone.HAND]) == hand_before + 1


@pytest.mark.rule("13-2-3-1")
def test_main_command_not_playable_during_a_battle_or_action_steps() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    sc.resources(1, 5)
    mine = sc.add(0, MAIN_ONLY, Zone.HAND)
    theirs = sc.add(1, MAIN_ONLY, Zone.HAND)
    sc.hand(0, MAIN_ACTION)
    sc.hand(1, MAIN_ACTION)
    zaku = sc.add(0, VANILLA)
    sc.add(1, VANILLA)
    sc.shields(1, VANILLA, VANILLA)
    st = sc.start()
    attack(st, zaku)
    assert _pending(st) is DecisionKind.ACTION_STEP
    assert st.pending is not None and st.pending.player == 1
    assert has_action(st, A.PLAY_COMMAND)
    assert not has_action(st, A.PLAY_COMMAND, theirs)
    pass_(st)
    assert st.pending is not None and st.pending.player == 0
    assert has_action(st, A.PLAY_COMMAND)
    assert not has_action(st, A.PLAY_COMMAND, mine)
    pass_all(st)
    end_main(st)
    assert _pending(st) is DecisionKind.ACTION_STEP
    assert not has_action(st, A.PLAY_COMMAND, theirs)
    pass_(st)
    assert _pending(st) is DecisionKind.ACTION_STEP
    assert not has_action(st, A.PLAY_COMMAND, mine)


@pytest.mark.rule("13-2-3-2", "13-2-4-3")
def test_main_action_command_can_be_played_at_either_time() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    sc.resources(1, 3)
    mine_main, mine_action = sc.hand(0, MAIN_ACTION, MAIN_ACTION)
    theirs = sc.add(1, MAIN_ACTION, Zone.HAND)
    zaku = sc.add(0, VANILLA)
    enemy = sc.add(1, VANILLA_3_4)
    sc.shields(1, VANILLA, VANILLA)
    st = sc.start()
    assert has_action(st, A.PLAY_COMMAND, mine_main)
    play(st, mine_main)
    assert st.cards[enemy].damage == 1
    attack(st, zaku)
    assert _pending(st) is DecisionKind.ACTION_STEP
    assert st.pending is not None and st.pending.player == 1
    assert has_action(st, A.PLAY_COMMAND, theirs)
    act(st, A.PLAY_COMMAND, theirs)
    assert st.cards[zaku].damage == 1
    assert st.pending is not None and st.pending.player == 0
    act(st, A.PLAY_COMMAND, mine_action)
    assert st.cards[enemy].damage == 2


@pytest.mark.rule("13-2-4-1", "13-2-4-2")
def test_action_command_only_in_action_steps_and_never_paired_there() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    cmd = sc.add(0, ACTION_PILOT, Zone.HAND)
    zaku = sc.add(0, VANILLA)
    other = sc.add(0, VANILLA)
    sc.shields(1, VANILLA, VANILLA)
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, cmd)
    assert has_action(st, A.PAIR, cmd)
    attack(st, zaku)
    assert _pending(st) is DecisionKind.ACTION_STEP
    assert st.pending is not None and st.pending.player == 0
    assert has_action(st, A.PLAY_COMMAND, cmd)
    assert not has_action(st, A.PAIR)
    act(st, A.PLAY_COMMAND, cmd)
    assert ap(st, zaku) == 3
    assert ap(st, other) == 3
    assert zone_of(st, cmd) is Zone.TRASH


@pytest.mark.rule("13-2-4-2")
def test_main_action_pilot_command_cannot_be_paired_in_an_action_step() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    cmd = sc.add(0, FIRST_STRIKE_CMD, Zone.HAND)
    zaku = sc.add(0, VANILLA)
    sc.add(0, VANILLA)
    sc.shields(1, VANILLA, VANILLA)
    st = sc.start()
    assert has_action(st, A.PAIR, cmd)
    assert has_action(st, A.PLAY_COMMAND, cmd)
    attack(st, zaku)
    assert _pending(st) is DecisionKind.ACTION_STEP
    assert has_action(st, A.PLAY_COMMAND, cmd)
    assert not has_action(st, A.PAIR)


# ---------------------------------------------------------------------------------------------
# 13-2-5 【Burst】


@pytest.mark.rule("13-2-5-1", "13-2-5-3")
def test_burst_resolves_without_cost_before_the_card_reaches_the_trash() -> None:
    sc = Scenario()
    zaku = sc.add(0, VANILLA)
    other = sc.add(0, VANILLA)
    (ploy,) = sc.shields(1, SIEGE_PLOY)
    st = sc.start()
    attack(st, zaku)
    pass_all(st)
    assert _pending(st) is DecisionKind.BURST
    assert st.pending is not None and st.pending.player == 1
    yes(st)
    assert _pending(st) is DecisionKind.SELECT
    assert zone_of(st, ploy) not in (Zone.TRASH, Zone.SHIELD)
    select(st, other)
    assert st.cards[other].rested
    assert zone_of(st, ploy) is Zone.TRASH
    assert st.zones[1][Zone.RESOURCE_AREA] == []


@pytest.mark.rule("13-2-5-1")
def test_burst_on_a_shield_destroyed_by_an_effect() -> None:
    sc = Scenario()
    rick = sc.add(0, BREACH_2)
    victim = sc.add(1, VANILLA, rested=True)
    (shield,) = sc.shields(1, BURST_ADD)
    st = sc.start()
    attack(st, rick, victim)
    pass_all(st)
    assert _pending(st) is DecisionKind.BURST
    yes(st)
    assert zone_of(st, shield) is Zone.HAND


@pytest.mark.rule("13-2-5-1", "13-2-6-1")
def test_shield_added_to_hand_does_not_burst() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    base = sc.add(0, BASE_0_6, Zone.HAND)
    top, _ = sc.shields(0, BURST_ADD, VANILLA)
    st = sc.start()
    play(st, base)
    assert zone_of(st, base) is Zone.BASE
    assert zone_of(st, top) is Zone.HAND
    assert _pending(st) is DecisionKind.MAIN


@pytest.mark.rule("13-2-5-2")
def test_declined_burst_goes_to_the_trash() -> None:
    sc = Scenario()
    zaku = sc.add(0, VANILLA)
    (shield,) = sc.shields(1, BURST_ADD)
    st = sc.start()
    attack(st, zaku)
    pass_all(st)
    assert _pending(st) is DecisionKind.BURST
    no(st)
    assert zone_of(st, shield) is Zone.TRASH
    assert st.zones[1][Zone.HAND] == []


@pytest.mark.rule("13-2-5-3", "13-2-6-1")
def test_burst_card_that_moves_elsewhere_is_not_trashed() -> None:
    sc = Scenario()
    zaku = sc.add(0, VANILLA)
    base, below = sc.shields(1, BASE_0_6, VANILLA)
    st = sc.start()
    attack(st, zaku)
    pass_all(st)
    assert _pending(st) is DecisionKind.BURST
    yes(st)
    assert zone_of(st, base) is Zone.BASE
    assert zone_of(st, below) is Zone.HAND


# ---------------------------------------------------------------------------------------------
# 13-2-6 【Deploy】, 13-2-7 【Attack】, 13-2-8 【Destroyed】


@pytest.mark.rule("13-2-6-1")
def test_deploy_effect_activates_when_the_card_is_deployed() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    unit = sc.add(0, DEPLOY_SELF_DAMAGE, Zone.HAND)
    placed = sc.add(0, DEPLOY_SELF_DAMAGE)
    st = sc.start()
    assert st.cards[placed].damage == 0
    play(st, unit)
    assert zone_of(st, unit) is Zone.BATTLE
    assert st.cards[unit].damage == 2
    assert st.cards[placed].damage == 0


@pytest.mark.rule("13-2-7-1")
def test_attack_effect_resolves_when_the_attack_is_declared() -> None:
    sc = Scenario()
    zaku = sc.add(0, ATTACK_AP)
    sc.add(1, BLOCKER)
    top, second = sc.shields(1, VANILLA, VANILLA)
    st = sc.start()
    assert ap(st, zaku) == 1
    attack(st, zaku)
    assert _pending(st) is DecisionKind.BLOCK
    assert ap(st, zaku) == 3
    block(st, None)
    pass_all(st)
    assert zone_of(st, top) is Zone.TRASH
    assert zone_of(st, second) is Zone.SHIELD


@pytest.mark.rule("13-2-7-1")
def test_attack_effect_does_not_activate_when_the_unit_is_attacked() -> None:
    sc = Scenario(active=1)
    zaku = sc.add(0, ATTACK_AP, rested=True)
    attacker = sc.add(1, VANILLA)
    st = sc.start()
    attack(st, attacker, zaku)
    pass_all(st)
    assert zone_of(st, zaku) is Zone.TRASH
    assert st.cards[attacker].damage == 1


@pytest.mark.rule("13-2-7-1")
def test_attack_effect_does_not_activate_in_a_battle_begun_by_an_effect() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    nu = sc.add(0, NU_GUNDAM)
    trash = sc.trash(0, *LONDO_BELL)
    pilot = sc.add(0, ATTACK_PILOT, Zone.HAND)
    enemy = sc.add(1, VANILLA_6_4)
    st = sc.start()
    play(st, pilot, onto=nu)
    yes_if_asked(st)
    select_if_asked(st, *trash)
    assert zone_of(st, enemy) is Zone.TRASH
    assert zone_of(st, nu) is Zone.TRASH


def _ex_resources(st: GameState, player: int) -> int:
    area = st.zones[player][Zone.RESOURCE_AREA]
    return sum(1 for u in area if V.cdef(st, u).card_type is CardType.EX_RESOURCE)


@pytest.mark.rule("13-2-8-1")
def test_destroyed_effect_activates_on_battle_destruction() -> None:
    sc = Scenario()
    zaku = sc.add(0, VANILLA)
    duel = sc.add(1, DESTROYED_EX, rested=True)
    st = sc.start()
    attack(st, zaku, duel)
    pass_all(st)
    assert zone_of(st, duel) is Zone.TRASH
    assert _ex_resources(st, 1) == 1


@pytest.mark.rule("13-2-8-1")
def test_destroyed_effect_activates_on_destruction_by_an_effect() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    cmd = sc.add(0, DESTROY_CMD, Zone.HAND)
    duel = sc.add(1, DESTROYED_EX)
    st = sc.start()
    play(st, cmd)
    assert zone_of(st, duel) is Zone.TRASH
    assert _ex_resources(st, 1) == 1


@pytest.mark.rule("13-2-8-1")
def test_destroyed_effect_does_not_activate_when_returned_to_hand() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    cmd = sc.add(0, BOUNCE_CMD, Zone.HAND)
    duel = sc.add(1, DESTROYED_EX)
    st = sc.start()
    play(st, cmd)
    assert zone_of(st, duel) is Zone.HAND
    assert _ex_resources(st, 1) == 0


@pytest.mark.rule("13-2-8-2")
def test_destroyed_effect_resolves_from_the_trash_for_its_owner() -> None:
    sc = Scenario()
    launcher = sc.add(0, BLOCKER)
    zaku = sc.add(0, VANILLA)
    geara = sc.add(1, DESTROYED_DAMAGE, rested=True)
    st = sc.start()
    attack(st, launcher, geara)
    pass_all(st)
    assert _pending(st) is DecisionKind.SELECT
    assert st.pending is not None and st.pending.player == 1
    assert zone_of(st, geara) is Zone.TRASH
    assert _option_args(st, A.SELECT) == {launcher, zaku}
    select(st, zaku)
    assert st.cards[zaku].damage == 1


def _unicorn_destroyed(pilot: str) -> tuple[GameState, int, int, list[int]]:
    sc = Scenario(active=1)
    unicorn = sc.add(0, UNICORN, pilot=pilot, rested=True)
    paired = sc.st.cards[unicorn].pair
    spares = sc.hand(0, VANILLA, VANILLA)
    attacker = sc.add(1, VANILLA_6_4)
    st = sc.start()
    attack(st, attacker, unicorn)
    pass_all(st)
    assert zone_of(st, unicorn) is Zone.TRASH
    assert zone_of(st, attacker) is Zone.TRASH
    return st, unicorn, paired, spares


@pytest.mark.rule("13-2-8-2", "13-2-8-2-1")
def test_destroyed_effect_activates_from_the_trash_with_last_known_link_state() -> None:
    st, unicorn, _, _ = _unicorn_destroyed(BANAGHER)
    assert _pending(st) is DecisionKind.DISCARD
    assert st.pending is not None and st.pending.player == 0
    assert zone_of(st, unicorn) is Zone.TRASH


@pytest.mark.rule("13-2-8-2", "13-2-8-2-1")
def test_destroyed_effect_returns_the_last_known_paired_pilot() -> None:
    st, _, pilot, spares = _unicorn_destroyed(BANAGHER)
    assert _pending(st) is DecisionKind.DISCARD
    assert pilot in _option_args(st, A.SELECT)
    select(st, spares[0])
    assert zone_of(st, pilot) is Zone.HAND
    assert zone_of(st, spares[0]) is Zone.TRASH


@pytest.mark.rule("13-2-8-2-1")
def test_destroyed_effect_gated_by_link_needs_a_link_pilot() -> None:
    st, _, pilot, spares = _unicorn_destroyed(RIDDHE)
    assert _pending(st) is DecisionKind.MAIN
    assert zone_of(st, pilot) is Zone.TRASH
    assert all(zone_of(st, u) is Zone.HAND for u in spares)


# ---------------------------------------------------------------------------------------------
# 13-2-9 【When Paired】, 13-2-10 【During Pair】


@pytest.mark.rule("13-2-9-1")
def test_when_paired_activates_when_a_pilot_is_paired() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    messer = sc.add(0, WHEN_PAIRED)
    pilot = sc.add(0, RIDDHE, Zone.HAND)
    enemy = sc.add(1, VANILLA_3_4)
    st = sc.start()
    assert st.cards[enemy].damage == 0
    play(st, pilot, onto=messer)
    assert st.cards[messer].pair == pilot
    assert st.cards[enemy].damage == 1


def _pair_deathscythe(pilot_number: str) -> tuple[GameState, int]:
    sc = Scenario()
    sc.resources(0, 4)
    sc.resource_deck(0, 2)
    unit = sc.add(0, DEATHSCYTHE)
    pilot = sc.add(0, pilot_number, Zone.HAND)
    st = sc.start()
    play(st, pilot, onto=unit)
    return st, unit


@pytest.mark.rule("13-2-9-2")
def test_when_paired_with_qualification_activates_for_a_qualifying_pilot() -> None:
    st, unit = _pair_deathscythe(HEERO)
    area = st.zones[0][Zone.RESOURCE_AREA]
    assert len(area) == 5
    assert sum(1 for u in area if st.cards[u].rested) == 2
    assert "First Strike" in keywords(st, unit)


@pytest.mark.rule("13-2-9-2")
def test_when_paired_with_qualification_ignores_other_pilots() -> None:
    st, unit = _pair_deathscythe(RIDDHE)
    assert len(st.zones[0][Zone.RESOURCE_AREA]) == 4
    assert "First Strike" not in keywords(st, unit)


@pytest.mark.rule("13-2-10-1")
def test_during_pair_effect_exists_only_while_paired() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    heavyarms = sc.add(0, HEAVYARMS)
    pilot = sc.add(0, RIDDHE, Zone.HAND)
    st = sc.start()
    assert keywords(st, heavyarms) == {}
    play(st, pilot, onto=heavyarms)
    assert keywords(st, heavyarms) == {"Breach": 3}


def _qubeley_attack(pilot: str) -> tuple[GameState, int]:
    sc = Scenario()
    qubeley = sc.add(0, QUBELEY, pilot=pilot)
    damaged = sc.add(1, VANILLA_3_4, damage=1)
    sc.shields(1, VANILLA, VANILLA)
    st = sc.start()
    attack(st, qubeley)
    pass_all(st)
    return st, damaged


@pytest.mark.rule("13-2-10-2")
def test_during_pair_with_qualification_applies_for_a_qualifying_pilot() -> None:
    st, damaged = _qubeley_attack(MARIDA)
    assert st.cards[damaged].damage == 3


@pytest.mark.rule("13-2-10-2")
def test_during_pair_with_qualification_ignores_other_pilots() -> None:
    st, damaged = _qubeley_attack(RIDDHE)
    assert st.cards[damaged].damage == 1


# ---------------------------------------------------------------------------------------------
# 13-2-11 【When Linked】, 13-2-12 【During Link】


def _pair_gyan(pilot_number: str) -> tuple[GameState, int]:
    sc = Scenario()
    sc.resources(0, 3)
    gyan = sc.add(0, GYAN)
    pilot = sc.add(0, pilot_number, Zone.HAND)
    st = sc.start()
    play(st, pilot, onto=gyan)
    return st, gyan


@pytest.mark.rule("13-2-11-1")
def test_when_linked_activates_when_a_link_pilot_is_paired() -> None:
    st, gyan = _pair_gyan(MQUVE)
    assert "First Strike" in keywords(st, gyan)


@pytest.mark.rule("13-2-11-1")
def test_when_linked_does_not_activate_for_a_non_link_pilot() -> None:
    st, gyan = _pair_gyan(RIDDHE)
    assert "First Strike" not in keywords(st, gyan)


def _attack_with_link_hm(pilot: str) -> GameState:
    sc = Scenario()
    gundam = sc.add(0, LINK_HM, pilot=pilot)
    sc.add(1, BLOCKER)
    sc.shields(1, VANILLA, VANILLA)
    st = sc.start()
    attack(st, gundam)
    return st


@pytest.mark.rule("13-2-12-1")
def test_during_link_effect_applies_while_a_link_pilot_is_paired() -> None:
    st = _attack_with_link_hm(AMURO_RECOVER)
    gundam = st.zones[0][Zone.BATTLE][0]
    assert "High-Maneuver" in keywords(st, gundam)
    assert _pending(st) is DecisionKind.MAIN


@pytest.mark.rule("13-2-12-1")
def test_during_link_effect_absent_with_a_non_link_pilot() -> None:
    st = _attack_with_link_hm(RIDDHE)
    gundam = st.zones[0][Zone.BATTLE][0]
    assert "High-Maneuver" not in keywords(st, gundam)
    assert _pending(st) is DecisionKind.BLOCK


def _pharact_attack(pilot: str) -> tuple[GameState, int]:
    sc = Scenario()
    pharact = sc.add(0, PHARACT, pilot=pilot)
    blocker = sc.add(1, BLOCKER)
    sc.shields(1, VANILLA, VANILLA)
    st = sc.start()
    attack(st, pharact)
    assert _pending(st) is DecisionKind.BLOCK
    return st, blocker


@pytest.mark.rule("13-2-12-2")
def test_during_link_attack_effect_activates_with_a_link_pilot() -> None:
    st, blocker = _pharact_attack(GUEL)
    assert ap(st, blocker) == 1


@pytest.mark.rule("13-2-12-2")
def test_during_link_attack_effect_does_not_activate_without_link() -> None:
    st, blocker = _pharact_attack(RIDDHE)
    assert ap(st, blocker) == 3


# ---------------------------------------------------------------------------------------------
# 13-2-13 【Once per Turn】


@pytest.mark.rule("13-2-13-1")
def test_once_per_turn_activated_effect_once_each_turn() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    galluss = sc.add(0, GALLUSS)
    sc.add(0, VANILLA_3_4)
    sc.add(0, MAIN_ACTION, Zone.HAND)
    attacker = sc.add(1, VANILLA)
    sc.shields(0, VANILLA, VANILLA)
    st = sc.start(Step.END_ACTION)
    assert st.pending is not None and st.pending.player == 0
    activate(st, galluss)
    assert st.pending is not None and st.pending.player == 0
    assert has_action(st, A.PLAY_COMMAND)
    assert not has_action(st, A.ACTIVATE, galluss)
    pass_(st)
    assert st.active == 1
    attack(st, attacker)
    assert _pending(st) is DecisionKind.ACTION_STEP
    assert st.pending is not None and st.pending.player == 0
    assert has_action(st, A.ACTIVATE, galluss)


@pytest.mark.rule("13-2-13-1")
def test_once_per_turn_triggered_effect_resolves_once() -> None:
    sc = Scenario(active=1)
    sc.resources(1, 2)
    raider = sc.add(0, RAIDER)
    first, second = sc.hand(1, MAIN_ACTION, MAIN_ACTION)
    st = sc.start()
    hand = len(st.zones[0][Zone.HAND])
    play(st, first)
    assert st.cards[raider].damage == 1
    assert len(st.zones[0][Zone.HAND]) == hand + 1
    play(st, second)
    assert st.cards[raider].damage == 2
    assert len(st.zones[0][Zone.HAND]) == hand + 1


@pytest.mark.rule("13-2-13-2")
def test_once_per_turn_counts_each_card_separately() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    first = sc.add(0, GALLUSS)
    second = sc.add(0, GALLUSS)
    sc.add(0, VANILLA_3_4)
    st = sc.start(Step.END_ACTION)
    assert _option_args(st, A.ACTIVATE) == {first, second}
    activate(st, first)
    assert _option_args(st, A.ACTIVATE) == {second}
    activate(st, second)
    rested = sum(1 for u in st.zones[0][Zone.RESOURCE_AREA] if st.cards[u].rested)
    assert rested == 2


@pytest.mark.rule("13-2-13-2")
def test_once_per_turn_trigger_counts_each_card_separately() -> None:
    sc = Scenario(active=1)
    sc.resources(1, 2)
    first = sc.add(0, RAIDER)
    second = sc.add(0, RAIDER)
    cmd_a, cmd_b = sc.hand(1, MAIN_ACTION, MAIN_ACTION)
    st = sc.start()
    hand = len(st.zones[0][Zone.HAND])
    play(st, cmd_a)
    select(st, first)
    play(st, cmd_b)
    select(st, second)
    assert st.cards[first].damage == 1
    assert st.cards[second].damage == 1
    assert len(st.zones[0][Zone.HAND]) == hand + 2
