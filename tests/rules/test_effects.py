"""Effect activation and resolution: rules 1-3-7, 5-16, 5-20 and section 10.

Each test builds a focused Scenario with real cards and asserts observable behaviour: legal
actions, zones, stats, the order in which decisions reach the players, and who makes them.
"""

from __future__ import annotations

import pytest

from gcg_sim.cards.model import CardType
from gcg_sim.effects import dsl as d
from gcg_sim.effects.registry import get_registry
from gcg_sim.engine import view as V
from gcg_sim.engine.state import Battle, GameState
from gcg_sim.engine.types import ActionKind, DecisionKind, Zone
from gcg_sim.testkit import (
    Scenario,
    act,
    activate,
    ap,
    attack,
    end_main,
    has_action,
    keywords,
    no,
    options,
    pass_all,
    play,
    select,
    to_next_turn,
    yes,
    zone_of,
)

A = ActionKind

# vanilla Units
ZAKU_MARINER = "GD01-060"  # Lv2 cost1 2/2 (Zeon)
WBT_GUNDAM = "GD01-013"  # Lv4 cost2 3/4 (Earth Federation)(White Base Team)
BIGRO = "GD04-027"  # Lv5 cost3 5/4
DEMI_GARRISON = "GD01-085"  # Lv2 cost1 2/2 (Academy)
GUELS_DILANZA = "GD01-083"  # Lv2 cost2 2/2 (Academy)
AGE1_NORMAL = "GD02-029"  # green Lv3 cost2 3/3 (Earth Federation)(AGE System)
AGE1_FLAT = "GD03-031"  # Lv4 cost2 4/3 (AGE System)
GM = "ST01-005"  # Lv2 cost1 2/2 (Earth Federation)
LEO_G_TEAM = "GD05-077"  # (G Team)
EXIA = "ST07-002"  # 4/3 (CB)
ZERO_GUNDAM = "GD03-063"  # 2/2 (CB)
PISCES = "GD01-021"  # (OZ)
CLAN_ZAKU = "GD03-032"  # (Clan)

# Units with effects
REPAIR_AURA = "GD01-001"  # All your (White Base Team) Units gain <Repair 1>.
STRIKE_DRAW_DISCARD = "ST04-002"  # Deploy: Draw 1. Then, discard 1.
DEATHSCYTHE = "GD01-025"  # When Paired (Operation Meteor): place a Resource. Then, First Strike
AILE_STRIKE = "GD03-072"  # Deploy: If another (Triple Ship Alliance) Unit, draw 1. Then, discard 1.
AGE1_DISCARD = "GD02-021"  # Deploy: may discard; If you do, EX Resource. Then, if Lv.7+, draw 1.
TURN_A = "GD04-073"  # Activate-Main, once per turn, (1): This Unit gets AP+2 during this turn.
WING_BIRD = "ST02-002"  # Lv3 cost3; Deploy: Place 1 EX Resource.
G_EXES = "GD02-022"  # Once per turn, when you place an EX Resource: (AGE System) Unit <Breach 2>
JEGAN = "GD01-016"  # While 2+ (Earth Federation) Units in play, this card in hand gets cost -1.
TITUS = "GD02-031"  # While you are Lv.7 or higher, this Unit gets AP+2.
DEMI_BARDING = "GD05-025"  # Deploy: look at the top 3; may add 1 Command card among them to hand.
PSYCHO_HARO = "EB01-042"  # While rested, all Units gain <Blocker>; Attack: Lv.7- can't use it.
ALPHA_AZIERU = "GD05-054"  # Lv8 <Blocker>
ZEE_ZULU = "GD01-059"  # Attack: if attacking the enemy player, AP+2 during this battle.
LAUNCHER_STRIKE = "GD01-072"  # <Blocker>
PSYCHO_ZAKU = "EB01-045"  # <Suppression>
DUEL_ASSAULT = "ST14-009"  # 3/2; Destroyed: Place 1 EX Resource.
GEARA_DOGA_SLEEVES = "GD01-056"  # 2/3; Destroyed: choose 1 enemy Unit with 5- AP, 1 damage.
GALLUSS_K = "GD01-058"  # Activate-Action, once per turn, (1): 1 Lv.4+ Unit gets AP+1.
GFRED = "GD03-035"  # Activate-Main, (1) + exile 1 Pilot card from trash: 1 damage to all enemies.
HEAVYARMS_EW = "GD05-079"  # Activate-Main (no cost): if another (G Team) Unit, 1 enemy AP-1.
NOINS_ARIES = "GD01-007"  # Destroyed: If you have another (OZ) Unit in play, draw 1.
BARBATOS_ADAPT = "GD03-056"  # Deploy: Choose 1 of your Units and 1 enemy Unit. 1 damage to them.
RED_GUNDAM = "GD03-039"  # Deploy: rest another active (Clan) Unit. If you do, 2 damage to ...
JEGAN_MAN_HUNTER = "ST08-009"  # Deploy: Choose 1 rested enemy Unit that is Lv.2 or lower. ...
NOINS_TAURUS = "GD05-074"  # 3/1; Destroyed: Draw 1. Then, discard 1.

# Bases
TESTING_SECTOR = "GD04-124"  # When you place an EX Resource, 1 friendly (Academy) Unit AP+2.
VESALIUS = "ST04-016"  # Activate-Main, Rest this Base: 1 friendly Unit gets AP+1.

# Pilots
RIDDHE = "GD01-089"  # gives: While this Unit has <Repair>, it gets AP+1.
HEERO = "GD05-098"  # (Operation Meteor); gives: destroys a shield area card -> 1 enemy AP-2
SETSUNA = "ST07-009"  # gives Attack: AP+1; 7+ (CB) in trash: all (CB) Units AP+1 instead
AZEE = "GD03-095"  # purple Pilot

# Commands
AIRFRAME_SEIZURE = "GD05-111"  # Lv1 cost1 Main: Discard 1. If you do, draw 2.
DAMAGE_CONTROL = "GD04-113"  # Lv3 cost1 Action only
BATTLE_OF_ACES = "GD01-111"  # Lv3 cost2 Main/Action: 1 damaged enemy Unit, 3 damage.
EXTREME_HATRED = "GD01-112"  # Lv6: Choose 2 of your active Units. Rest them. If you do, ...
DRAMATIC_TURNABOUT = "GD02-100"  # Lv5: 1 friendly damaged Unit recovers 2 HP. Then, draw 1.
HEALTHY_CURIOSITY = "GD03-101"  # Lv3: Draw 1. Then, if 2+ in trash, choose 1 enemy Unit ...
STUBBORN_COG = "GD01-103"  # Lv1: 1 active friendly (EF) Unit and 1 active enemy Unit. Rest them.
IMPROVED_TECHNIQUE = "GD03-109"  # Lv3 cost3: 1 enemy Lv.4- Unit, 3 damage; 2+ in trash: any instead
MOMENTARY_RESPITE = "GD02-112"  # Lv4 cost3: Choose 1 purple Pilot card from your trash.
ENCOUNTER = "GD04-105"  # Lv5: Look at the top 5 cards; may add 1 Pilot card among them.
KINDHEARTED = "GD04-101"  # Lv3: friendly Units can't be destroyed by enemy effects this turn.
LOOK_OF_DETERMINATION = "GD03-114"  # Lv2 Action: 1 active enemy Lv.2- Unit. Destroy it.
POORLY_PLANNED = "ST11-013"  # Burst: 1 rested enemy Unit with 3- HP returns to hand.
CHARACTER_REQUESTS = "EB01-073"  # Burst: Draw 1.


# ---------------------------------------------------------------------------------------------
# helpers


def _kind(st: GameState) -> DecisionKind | None:
    return st.pending.kind if st.pending else None


def _decider(st: GameState) -> int:
    assert st.pending is not None
    return st.pending.player


def _select_options(st: GameState) -> set[int]:
    return {o.a for o in options(st) if o.kind is A.SELECT}


def _order_trigger_of(st: GameState, host: int) -> None:
    """Answer an ORDER_TRIGGER decision by picking the pending trigger hosted by ``host``."""
    assert _kind(st) is DecisionKind.ORDER_TRIGGER
    batch = st.batches[-1]
    choice = next(o for o in options(st) if batch[o.a].host == host)
    act(st, A.ORDER, choice.a)


def _resolve_orders(st: GameState) -> None:
    while _kind(st) is DecisionKind.ORDER_TRIGGER:
        act(st, A.ORDER, options(st)[0].a)


def _ex_resources(st: GameState, player: int) -> int:
    return sum(
        1
        for u in st.zones[player][Zone.RESOURCE_AREA]
        if V.cdef(st, u).card_type is CardType.EX_RESOURCE
    )


def _abilities(number: str) -> tuple[d.Ability, ...]:
    reg = get_registry()
    script = reg.cards[reg.db[number].def_id].script
    assert script is not None
    return script.abilities + script.unit_abilities


# ---------------------------------------------------------------------------------------------
# 1-3-7 text order, 5-16 gain, 5-20 "If you do" / "Then"


@pytest.mark.rule("1-3-7")
def test_card_effects_are_performed_in_printed_order() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    sc.deck(0, WBT_GUNDAM)
    strike = sc.add(0, STRIKE_DRAW_DISCARD, Zone.HAND)
    st = sc.start()
    top = st.zones[0][Zone.DECK][0]
    play(st, strike)
    # "Draw 1. Then, discard 1." with an otherwise empty hand: the discard can only take the
    # card the draw just produced, so the draw was performed first.
    assert st.zones[0][Zone.HAND] == []
    assert zone_of(st, top) is Zone.TRASH
    assert _kind(st) is DecisionKind.MAIN


@pytest.mark.rule("5-16-1")
def test_gain_adds_an_effect_the_card_does_not_print() -> None:
    assert _abilities(WBT_GUNDAM) == ()
    sc = Scenario()
    sc.resources(0, 4)
    sc.add(0, REPAIR_AURA)
    wbt = sc.add(0, WBT_GUNDAM)
    deathscythe = sc.add(0, DEATHSCYTHE)
    heero = sc.add(0, HEERO, Zone.HAND)
    st = sc.start()
    assert keywords(st, wbt) == {"Repair": 1}  # gained from a constant effect
    assert "First Strike" not in keywords(st, deathscythe)
    play(st, heero, onto=deathscythe)
    assert "First Strike" in keywords(st, deathscythe)  # gained from a triggered effect


@pytest.mark.rule("5-20-1")
def test_if_you_do_part_is_skipped_when_the_preceding_part_cannot_resolve() -> None:
    sc = Scenario()
    sc.resources(0, 1)
    seizure = sc.add(0, AIRFRAME_SEIZURE, Zone.HAND)
    st = sc.start()
    deck_before = len(st.zones[0][Zone.DECK])
    play(st, seizure)  # "Discard 1. If you do, draw 2." with nothing left to discard
    assert st.zones[0][Zone.HAND] == []
    assert len(st.zones[0][Zone.DECK]) == deck_before
    assert zone_of(st, seizure) is Zone.TRASH


@pytest.mark.rule("5-20-1")
def test_if_you_do_part_resolves_when_the_preceding_part_resolved() -> None:
    sc = Scenario()
    sc.resources(0, 1)
    fodder = sc.add(0, ZAKU_MARINER, Zone.HAND)
    seizure = sc.add(0, AIRFRAME_SEIZURE, Zone.HAND)
    st = sc.start()
    deck_before = len(st.zones[0][Zone.DECK])
    play(st, seizure)
    assert zone_of(st, fodder) is Zone.TRASH
    assert len(st.zones[0][Zone.HAND]) == 2
    assert len(st.zones[0][Zone.DECK]) == deck_before - 2


@pytest.mark.rule("5-20-2")
@pytest.mark.ruling("GD01-025:Q127")
def test_then_part_resolves_even_if_the_preceding_part_cannot() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    deathscythe = sc.add(0, DEATHSCYTHE)
    heero = sc.add(0, HEERO, Zone.HAND)
    st = sc.start()
    assert st.zones[0][Zone.RESOURCE_DECK] == []
    play(st, heero, onto=deathscythe)  # "Place 1 rested Resource" is impossible
    assert len(st.zones[0][Zone.RESOURCE_AREA]) == 4
    assert "First Strike" in keywords(st, deathscythe)


@pytest.mark.rule("5-20-2")
@pytest.mark.ruling("GD03-072:Q233")
def test_leading_condition_also_governs_the_then_part() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    kept = sc.add(0, ZAKU_MARINER, Zone.HAND)
    aile = sc.add(0, AILE_STRIKE, Zone.HAND)
    st = sc.start()
    play(st, aile)  # no other (Triple Ship Alliance) Unit: neither draw nor discard
    assert zone_of(st, kept) is Zone.HAND


@pytest.mark.rule("5-20-1", "5-20-2")
@pytest.mark.ruling("GD02-021:Q175")
def test_if_you_do_also_governs_the_following_then_part() -> None:
    sc = Scenario()
    sc.resources(0, 7)
    fodder = sc.add(0, AGE1_NORMAL, Zone.HAND)
    age1 = sc.add(0, AGE1_DISCARD, Zone.HAND)
    st = sc.start()
    play(st, age1)
    assert _kind(st) is DecisionKind.YES_NO
    no(st)  # decline the discard while at Lv.7
    assert st.zones[0][Zone.HAND] == [fodder]


# ---------------------------------------------------------------------------------------------
# 10-1-1 .. 10-1-4 effects in general


@pytest.mark.rule("10-1-1")
def test_effect_directive_and_compensation_come_from_card_text() -> None:
    (turn_a,) = _abilities(TURN_A)  # "①:This Unit gets AP+2 during this turn."
    assert isinstance(turn_a, d.Activated)
    assert turn_a.costs == (d.PayResources(1),)
    assert turn_a.steps == (d.Apply(d.This(), d.StatMod(ap=2)),)
    assert _abilities(ZAKU_MARINER) == ()

    sc = Scenario()
    sc.resources(0, 3)
    zaku = sc.add(0, ZAKU_MARINER, Zone.HAND)
    st = sc.start()
    play(st, zaku)  # a card without text generates no effect at all
    assert _kind(st) is DecisionKind.MAIN
    assert V.derived(st).abilities.get(zaku) is None
    assert not has_action(st, A.ACTIVATE, zaku)


@pytest.mark.rule("10-1-2")
def test_effects_function_only_on_the_field_unless_the_text_says_otherwise() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    wbt = sc.add(0, WBT_GUNDAM)
    sc.add(0, REPAIR_AURA, Zone.HAND)
    sc.trash(0, REPAIR_AURA)
    academy = sc.add(0, DEMI_GARRISON)
    sc.add(0, TESTING_SECTOR, Zone.HAND)
    wing = sc.add(0, WING_BIRD, Zone.HAND)
    st = sc.start()
    assert "Repair" not in keywords(st, wbt)
    play(st, wing)  # places an EX Resource; the Base in the hand does not react
    assert _ex_resources(st, 0) == 1
    assert ap(st, academy) == 2

    sc = Scenario()
    sc.resources(0, 3)
    sc.base(0, TESTING_SECTOR)
    academy = sc.add(0, DEMI_GARRISON)
    wing = sc.add(0, WING_BIRD, Zone.HAND)
    st = sc.start()
    play(st, wing)  # in the base section (shield area) the same effect is active
    assert ap(st, academy) == 4


@pytest.mark.rule("10-1-2", "10-1-5-3")
def test_hand_effect_applies_where_its_text_places_it() -> None:
    sc = Scenario()
    sc.resources(0, 3, rested=1)
    sc.add(0, WBT_GUNDAM)
    jegan = sc.add(0, JEGAN, Zone.HAND)
    gm = sc.add(0, GM, Zone.HAND)
    st = sc.start()
    assert V.play_cost(st, V.derived(st), jegan) == 2
    play(st, gm)  # second (Earth Federation) Unit: the hand card now costs 1
    assert V.play_cost(st, V.derived(st), jegan) == 1
    assert has_action(st, A.PLAY_UNIT, jegan)  # only 1 active Resource remains

    sc = Scenario()
    sc.resources(0, 3, rested=1)
    sc.add(0, WBT_GUNDAM)
    jegan = sc.add(0, JEGAN, Zone.HAND)
    zaku = sc.add(0, ZAKU_MARINER, Zone.HAND)
    st = sc.start()
    play(st, zaku)  # not (Earth Federation): the condition stays unfulfilled
    assert V.play_cost(st, V.derived(st), jegan) == 2
    assert not has_action(st, A.PLAY_UNIT, jegan)


@pytest.mark.rule("10-1-3")
def test_effects_do_as_much_as_possible_and_you_may_declines() -> None:
    sc = Scenario(deck_size=0)
    sc.resources(0, 4)
    sc.deck(0, AIRFRAME_SEIZURE, ZAKU_MARINER)
    sc.deck(1, *[ZAKU_MARINER] * 5)
    demi = sc.add(0, DEMI_BARDING, Zone.HAND)
    st = sc.start()
    command, other = st.zones[0][Zone.DECK]
    play(st, demi)  # "Look at the top 3 cards" with only 2 cards: look at both
    assert _kind(st) is DecisionKind.YES_NO and _decider(st) == 0
    assert st.cards[command].known & 1 and st.cards[other].known & 1

    declined = st.clone()
    no(declined)  # "you may": choosing not to leaves the hand unchanged
    assert declined.zones[0][Zone.HAND] == []
    assert sorted(declined.zones[0][Zone.DECK]) == sorted([command, other])

    yes(st)
    assert zone_of(st, command) is Zone.HAND
    assert st.zones[0][Zone.DECK] == [other]
    assert st.winner is None


@pytest.mark.rule("10-1-4")
def test_effects_divide_into_five_types() -> None:
    assert [type(a) for a in _abilities(REPAIR_AURA)] == [d.Constant, d.Triggered]
    assert [type(a) for a in _abilities(TURN_A)] == [d.Activated]
    assert [type(a) for a in _abilities(AIRFRAME_SEIZURE)] == [d.Command]
    (attack_effect,) = [a for a in _abilities(SETSUNA) if isinstance(a, d.Triggered)]
    (substitution,) = attack_effect.steps  # "... all your (CB) Units get AP+1 instead."
    assert isinstance(substitution, d.If)
    assert substitution.then and substitution.otherwise


# ---------------------------------------------------------------------------------------------
# 10-1-5 constant effects


@pytest.mark.rule("10-1-5-1")
def test_constant_effect_stays_active_through_turns() -> None:
    sc = Scenario()
    sc.add(0, REPAIR_AURA)
    wbt = sc.add(0, WBT_GUNDAM)
    st = sc.start()
    assert keywords(st, wbt) == {"Repair": 1}
    to_next_turn(st)
    assert st.active == 1
    assert keywords(st, wbt) == {"Repair": 1}
    to_next_turn(st)
    assert st.active == 0
    assert keywords(st, wbt) == {"Repair": 1}


@pytest.mark.rule("10-1-5-2")
def test_constant_effect_ends_when_its_card_leaves_the_location() -> None:
    sc = Scenario()
    aura = sc.add(0, REPAIR_AURA)
    wbt = sc.add(0, WBT_GUNDAM)
    bigro = sc.add(1, BIGRO, rested=True)
    st = sc.start()
    assert keywords(st, wbt) == {"Repair": 1}
    attack(st, aura, bigro)
    pass_all(st)
    assert zone_of(st, aura) is Zone.TRASH
    assert "Repair" not in keywords(st, wbt)


@pytest.mark.rule("10-1-5-3")
def test_conditional_constant_effect_is_active_only_while_its_condition_holds() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    titus = sc.add(0, TITUS)  # "While you are Lv.7 or higher, this Unit gets AP+2."
    wing = sc.add(0, WING_BIRD, Zone.HAND)
    zaku = sc.add(0, ZAKU_MARINER, Zone.HAND)
    st = sc.start()
    assert ap(st, titus) == 2
    play(st, wing)  # 【Deploy】places an EX Resource: Lv.7
    assert len(st.zones[0][Zone.RESOURCE_AREA]) == 7
    assert ap(st, titus) == 4
    play(st, zaku, ex=1)  # paying with the EX Resource removes it: Lv.6
    assert len(st.zones[0][Zone.RESOURCE_AREA]) == 6
    assert ap(st, titus) == 2


@pytest.mark.rule("10-1-5-4")
@pytest.mark.ruling("GD01-001:Q119")
def test_constant_effect_is_active_the_moment_its_card_enters() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    wbt = sc.add(0, WBT_GUNDAM)
    aura = sc.add(0, REPAIR_AURA, Zone.HAND)
    st = sc.start()
    assert "Repair" not in keywords(st, wbt)
    play(st, aura)
    assert keywords(st, aura) == {"Repair": 1}
    assert keywords(st, wbt) == {"Repair": 1}


@pytest.mark.rule("10-1-5-5")
def test_multiple_constant_effects_overlap() -> None:
    sc = Scenario()
    sc.add(0, REPAIR_AURA)
    sc.add(0, REPAIR_AURA)
    wbt = sc.add(0, WBT_GUNDAM, pilot=RIDDHE)
    st = sc.start()
    assert keywords(st, wbt) == {"Repair": 2}  # both auras apply
    # printed 3 + Pilot 1 + Riddhe's "While this Unit has <Repair>, it gets AP+1"
    assert ap(st, wbt) == 5


@pytest.mark.rule("10-1-5-6")
def test_cant_effect_takes_precedence_over_a_granted_keyword() -> None:
    sc = Scenario()
    haro = sc.add(0, PSYCHO_HARO)
    low = sc.add(1, ZAKU_MARINER)
    high = sc.add(1, ALPHA_AZIERU)
    sc.shields(1, ZAKU_MARINER)
    st = sc.start()
    assert "Blocker" not in keywords(st, low)
    attack(st, haro)  # rested Psycho Haro: "all Units gain <Blocker>"
    assert _kind(st) is DecisionKind.BLOCK and _decider(st) == 1
    assert "Blocker" in keywords(st, low)
    assert not has_action(st, A.BLOCK, low)  # "can't activate <Blocker>" wins
    assert has_action(st, A.BLOCK, high)


@pytest.mark.rule("10-1-5-7")
def test_constant_effect_applies_the_moment_a_matching_target_appears() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    sc.add(0, REPAIR_AURA)
    wbt = sc.add(0, WBT_GUNDAM, Zone.HAND)
    zaku = sc.add(0, ZAKU_MARINER, Zone.HAND)
    st = sc.start()
    play(st, wbt)
    assert keywords(st, wbt) == {"Repair": 1}
    play(st, zaku)
    assert "Repair" not in keywords(st, zaku)


# ---------------------------------------------------------------------------------------------
# 10-1-6 triggered effects


@pytest.mark.rule("10-1-6-1")
def test_triggered_effects_activate_automatically() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    sc.base(0, TESTING_SECTOR)
    academy = sc.add(0, DEMI_GARRISON)
    wing = sc.add(0, WING_BIRD, Zone.HAND)
    st = sc.start()
    play(st, wing)  # 【Deploy】, then "When you place an EX Resource", with no further input
    assert _kind(st) is DecisionKind.MAIN
    assert _ex_resources(st, 0) == 1
    assert ap(st, academy) == 4

    sc = Scenario()
    zee = sc.add(0, ZEE_ZULU)
    sc.add(1, LAUNCHER_STRIKE)
    sc.shields(1, ZAKU_MARINER)
    st = sc.start()
    attack(st, zee)  # 【Attack】 resolves before the block step
    assert _kind(st) is DecisionKind.BLOCK
    assert ap(st, zee) == 4


@pytest.mark.rule("10-1-6-1-1")
def test_triggered_effect_activates_every_time_unless_once_per_turn() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    sc.base(0, TESTING_SECTOR)
    sc.add(0, G_EXES)
    academy = sc.add(0, DEMI_GARRISON)
    age1 = sc.add(0, AGE1_NORMAL)
    first, second = sc.hand(0, WING_BIRD, WING_BIRD)
    st = sc.start()
    play(st, first)
    _resolve_orders(st)
    assert ap(st, academy) == 4
    assert keywords(st, age1) == {"Breach": 2}
    play(st, second)
    _resolve_orders(st)
    assert _ex_resources(st, 0) == 2
    assert ap(st, academy) == 6  # triggered again
    assert keywords(st, age1) == {"Breach": 2}  # 【Once per Turn】: not again


@pytest.mark.rule("10-1-6-2")
def test_trigger_needs_its_condition() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    sc.resource_deck(0, 2)
    first = sc.add(0, DEATHSCYTHE)
    second = sc.add(0, DEATHSCYTHE)
    riddhe = sc.add(0, RIDDHE, Zone.HAND)
    heero = sc.add(0, HEERO, Zone.HAND)
    st = sc.start()
    play(st, riddhe, onto=first)  # not an (Operation Meteor) Pilot
    assert "First Strike" not in keywords(st, first)
    assert len(st.zones[0][Zone.RESOURCE_AREA]) == 4
    play(st, heero, onto=second)
    assert "First Strike" in keywords(st, second)
    assert len(st.zones[0][Zone.RESOURCE_AREA]) == 5


@pytest.mark.rule("10-1-6-3")
def test_simultaneous_events_trigger_an_effect_only_once() -> None:
    sc = Scenario()
    zaku = sc.add(0, PSYCHO_ZAKU, pilot=HEERO)  # <Suppression>; Heero: AP-2 per trigger
    bigro = sc.add(1, BIGRO)
    shields = sc.shields(1, ZAKU_MARINER, ZAKU_MARINER, ZAKU_MARINER)
    st = sc.start()
    attack(st, zaku)
    pass_all(st)
    assert [zone_of(st, s) for s in shields] == [Zone.TRASH, Zone.TRASH, Zone.SHIELD]
    assert ap(st, bigro) == 3  # two Shields destroyed at once: one AP-2, not two


@pytest.mark.rule("10-1-6-4", "10-1-6-8")
def test_waiting_trigger_resolves_after_its_card_left() -> None:
    sc = Scenario()
    attacker = sc.add(0, ZAKU_MARINER, pilot=HEERO)  # 4/3 while paired
    bigro = sc.add(1, BIGRO)
    gundam = sc.add(1, WBT_GUNDAM)
    (shield,) = sc.shields(1, POORLY_PLANNED)
    st = sc.start()
    pilot = st.cards[attacker].pair
    attack(st, attacker)
    pass_all(st)
    # Burst goes first although the active player's Heero trigger is waiting too
    assert _kind(st) is DecisionKind.BURST and _decider(st) == 1
    yes(st)  # returns the rested attacker (3 HP) to its owner's hand
    assert zone_of(st, attacker) is Zone.HAND
    assert zone_of(st, pilot) is Zone.HAND
    assert zone_of(st, shield) is Zone.TRASH
    # the waiting trigger still resolves
    assert _kind(st) is DecisionKind.SELECT and _decider(st) == 0
    assert _select_options(st) == {bigro, gundam}
    select(st, bigro)
    assert ap(st, bigro) == 3
    assert ap(st, gundam) == 3


@pytest.mark.rule("10-1-6-5")
def test_you_choose_the_order_of_your_simultaneous_triggers() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    base = sc.base(0, TESTING_SECTOR)
    g_exes = sc.add(0, G_EXES)
    academy = {sc.add(0, DEMI_GARRISON), sc.add(0, GUELS_DILANZA)}
    age = {sc.add(0, AGE1_NORMAL), sc.add(0, AGE1_FLAT)}
    wing = sc.add(0, WING_BIRD, Zone.HAND)
    st = sc.start()
    play(st, wing)
    assert _kind(st) is DecisionKind.ORDER_TRIGGER and _decider(st) == 0

    g_exes_first = st.clone()
    _order_trigger_of(g_exes_first, g_exes)
    assert _select_options(g_exes_first) == age
    select(g_exes_first, min(age))
    assert _select_options(g_exes_first) == academy

    base_first = st.clone()
    _order_trigger_of(base_first, base)
    assert _select_options(base_first) == academy
    select(base_first, min(academy))
    assert _select_options(base_first) == age


@pytest.mark.rule("10-1-6-6")
def test_active_players_triggers_resolve_before_standby_players() -> None:
    sc = Scenario()
    geara = sc.add(1, GEARA_DOGA_SLEEVES, rested=True)  # standby: choose 1 enemy Unit, 1 damage
    duel = sc.add(0, DUEL_ASSAULT)  # active: place 1 EX Resource
    targets = {sc.add(0, DEMI_GARRISON), sc.add(0, GUELS_DILANZA)}
    st = sc.start()
    attack(st, duel, geara)
    pass_all(st)
    assert zone_of(st, duel) is Zone.TRASH and zone_of(st, geara) is Zone.TRASH
    # no ordering decision across players: the active player's effect already resolved
    assert _kind(st) is DecisionKind.SELECT and _decider(st) == 1
    assert _ex_resources(st, 0) == 1
    assert _select_options(st) == targets


@pytest.mark.rule("10-1-6-7")
def test_new_trigger_during_resolution_takes_priority() -> None:
    sc = Scenario()
    geara = sc.add(1, GEARA_DOGA_SLEEVES, rested=True)
    duel = sc.add(0, DUEL_ASSAULT)
    sc.base(0, TESTING_SECTOR)  # the EX Resource placed by Duel Gundam triggers this Base
    academy = {sc.add(0, DEMI_GARRISON), sc.add(0, GUELS_DILANZA)}
    st = sc.start()
    attack(st, duel, geara)
    pass_all(st)
    # the new trigger is resolved before the standby player's waiting 【Destroyed】
    assert _kind(st) is DecisionKind.SELECT and _decider(st) == 0
    assert _select_options(st) == academy
    chosen = min(academy)
    select(st, chosen)
    assert ap(st, chosen) == 4
    assert _kind(st) is DecisionKind.SELECT and _decider(st) == 1


@pytest.mark.rule("10-1-6-8")
def test_burst_resolves_before_other_triggered_effects() -> None:
    sc = Scenario()
    attacker = sc.add(0, ZAKU_MARINER, pilot=HEERO)
    bigro = sc.add(1, BIGRO)
    gundam = sc.add(1, WBT_GUNDAM)
    sc.shields(1, CHARACTER_REQUESTS)
    st = sc.start()
    attack(st, attacker)
    pass_all(st)
    assert _kind(st) is DecisionKind.BURST and _decider(st) == 1
    assert ap(st, bigro) == 5  # the active player's trigger has not resolved yet
    hand_before = len(st.zones[1][Zone.HAND])
    yes(st)
    assert len(st.zones[1][Zone.HAND]) == hand_before + 1
    assert _kind(st) is DecisionKind.SELECT and _decider(st) == 0
    select(st, gundam)
    assert ap(st, gundam) == 1


@pytest.mark.rule("10-1-6-8-1")
def test_new_trigger_during_bursts_resolves_before_the_next_burst() -> None:
    sc = Scenario()
    zaku = sc.add(0, PSYCHO_ZAKU)  # <Suppression>: two Shields at once
    duel = sc.add(0, DUEL_ASSAULT)
    aces, requests, _ = sc.shields(1, BATTLE_OF_ACES, CHARACTER_REQUESTS, ZAKU_MARINER)
    st = sc.start()
    attack(st, zaku)
    pass_all(st)
    assert _kind(st) is DecisionKind.ORDER_TRIGGER and _decider(st) == 1  # owner orders Bursts
    _order_trigger_of(st, aces)
    yes(st)
    assert _kind(st) is DecisionKind.SELECT and _decider(st) == 1
    select(st, duel)  # 2 damage destroys Duel Gundam: its 【Destroyed】 triggers
    assert zone_of(st, duel) is Zone.TRASH
    assert _kind(st) is DecisionKind.BURST and _decider(st) == 1
    assert _ex_resources(st, 0) == 1  # the new trigger resolved before the second Burst
    yes(st)
    assert zone_of(st, requests) is Zone.TRASH


# ---------------------------------------------------------------------------------------------
# 10-1-7 activated effects


@pytest.mark.rule("10-1-7-1")
def test_activated_effects_are_used_at_the_players_discretion() -> None:
    sc = Scenario()
    sc.resources(0, 1)
    turn_a = sc.add(0, TURN_A)  # 【Activate･Main】
    galluss = sc.add(0, GALLUSS_K)  # 【Activate･Action】
    sc.add(0, WBT_GUNDAM)  # Lv.4 target for Galluss-K
    st = sc.start()
    assert has_action(st, A.ACTIVATE, turn_a)
    assert not has_action(st, A.ACTIVATE, galluss)
    assert has_action(st, A.END_MAIN)
    end_main(st)  # the player may simply not activate
    assert _kind(st) is DecisionKind.ACTION_STEP and _decider(st) == 0
    assert has_action(st, A.ACTIVATE, galluss)
    assert not has_action(st, A.ACTIVATE, turn_a)
    assert has_action(st, A.PASS)


@pytest.mark.rule("10-1-7-2", "10-3-1-1")
def test_satisfying_the_part_before_the_colon_activates_the_effect() -> None:
    sc = Scenario()
    vesalius = sc.base(0, VESALIUS)  # "Rest this Base:Choose 1 friendly Unit. AP+1."
    zaku = sc.add(0, ZAKU_MARINER)
    st = sc.start()
    activate(st, vesalius)
    assert st.cards[vesalius].rested
    assert ap(st, zaku) == 3
    assert not has_action(st, A.ACTIVATE, vesalius)  # the cost can no longer be paid

    sc = Scenario()
    vesalius = sc.add(0, VESALIUS, Zone.BASE, rested=True)
    sc.add(0, ZAKU_MARINER)
    st = sc.start()
    assert not has_action(st, A.ACTIVATE, vesalius)


@pytest.mark.rule("10-1-7-3", "10-3-1-3")
def test_numbered_symbol_is_paid_with_resources() -> None:
    sc = Scenario()
    resources = sc.resources(0, 2, rested=1)
    turn_a = sc.add(0, TURN_A)
    st = sc.start()
    activate(st, turn_a)
    assert all(st.cards[r].rested for r in resources)
    assert ap(st, turn_a) == 5

    sc = Scenario()
    sc.resources(0, 2, rested=2)
    turn_a = sc.add(0, TURN_A)
    st = sc.start()
    assert not has_action(st, A.ACTIVATE, turn_a)


@pytest.mark.rule("10-1-7-4", "10-3-1-1")
def test_every_listed_condition_must_be_satisfied() -> None:
    sc = Scenario()
    sc.resources(0, 1)
    gfred = sc.add(0, GFRED)  # "①, exile 1 Pilot card from your trash:"
    st = sc.start()
    assert not has_action(st, A.ACTIVATE, gfred)  # no Pilot card in the trash

    sc = Scenario()
    sc.resources(0, 1, rested=1)
    sc.trash(0, RIDDHE)
    gfred = sc.add(0, GFRED)
    st = sc.start()
    assert not has_action(st, A.ACTIVATE, gfred)  # ① cannot be paid

    sc = Scenario()
    (resource,) = sc.resources(0, 1)
    (pilot,) = sc.trash(0, RIDDHE)
    gfred = sc.add(0, GFRED)
    enemies = [sc.add(1, BIGRO), sc.add(1, WBT_GUNDAM)]
    st = sc.start()
    activate(st, gfred)
    assert st.cards[resource].rested
    assert zone_of(st, pilot) is Zone.REMOVAL
    assert [st.cards[e].damage for e in enemies] == [1, 1]


@pytest.mark.rule("10-1-7-5")
def test_effect_without_conditions_is_activated_by_declaring_it() -> None:
    sc = Scenario()
    heavyarms = sc.add(0, HEAVYARMS_EW)
    sc.add(0, LEO_G_TEAM)
    enemy = sc.add(1, WBT_GUNDAM)
    st = sc.start()
    assert st.zones[0][Zone.RESOURCE_AREA] == []
    activate(st, heavyarms)
    assert not st.cards[heavyarms].rested
    assert ap(st, enemy) == 2


# ---------------------------------------------------------------------------------------------
# 10-1-8 command effects


@pytest.mark.rule("10-1-8-1")
def test_command_effects_follow_their_timing() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    main_only = sc.add(0, AIRFRAME_SEIZURE, Zone.HAND)
    action_only = sc.add(0, DAMAGE_CONTROL, Zone.HAND)
    both = sc.add(0, BATTLE_OF_ACES, Zone.HAND)
    sc.add(0, ZAKU_MARINER)
    sc.add(1, BIGRO, damage=1)
    st = sc.start()
    assert has_action(st, A.PLAY_COMMAND, main_only)
    assert has_action(st, A.PLAY_COMMAND, both)
    assert not has_action(st, A.PLAY_COMMAND, action_only)
    end_main(st)
    assert _kind(st) is DecisionKind.ACTION_STEP and _decider(st) == 0
    assert not has_action(st, A.PLAY_COMMAND, main_only)
    assert has_action(st, A.PLAY_COMMAND, both)
    assert has_action(st, A.PLAY_COMMAND, action_only)


@pytest.mark.rule("10-1-8-1-1", "10-2-2")
@pytest.mark.ruling("GD01-103:Q153")
def test_command_cannot_be_played_without_a_choosable_target() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    aces = sc.add(0, BATTLE_OF_ACES, Zone.HAND)  # "Choose 1 damaged enemy Unit."
    sc.add(1, BIGRO)
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, aces)

    sc = Scenario()
    sc.resources(0, 3)
    aces = sc.add(0, BATTLE_OF_ACES, Zone.HAND)
    sc.add(1, BIGRO, damage=1)
    st = sc.start()
    assert has_action(st, A.PLAY_COMMAND, aces)

    sc = Scenario()
    sc.resources(0, 1)
    sc.add(0, WBT_GUNDAM)
    sc.add(1, BIGRO, rested=True)
    cog = sc.add(0, STUBBORN_COG, Zone.HAND)  # needs an active friendly AND an active enemy
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, cog)


@pytest.mark.rule("10-1-8-1-2")
def test_only_targets_before_then_or_if_you_do_are_required_to_play() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    ours = [sc.add(0, ZAKU_MARINER), sc.add(0, ZAKU_MARINER)]
    hatred = sc.add(0, EXTREME_HATRED, Zone.HAND)
    st = sc.start()
    assert has_action(st, A.PLAY_COMMAND, hatred)  # no enemy Unit after "If you do"
    play(st, hatred)
    assert all(st.cards[u].rested for u in ours)
    assert zone_of(st, hatred) is Zone.TRASH

    sc = Scenario()
    sc.resources(0, 6)
    sc.add(0, ZAKU_MARINER, rested=True)
    sc.add(1, BIGRO)
    hatred = sc.add(0, EXTREME_HATRED, Zone.HAND)
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, hatred)  # no active Unit before "If you do"

    sc = Scenario()
    sc.resources(0, 5)
    sc.add(0, ZAKU_MARINER)
    turnabout = sc.add(0, DRAMATIC_TURNABOUT, Zone.HAND)  # target before "Then"
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, turnabout)

    sc = Scenario()
    sc.resources(0, 3)
    sc.trash(0, HEALTHY_CURIOSITY, HEALTHY_CURIOSITY)
    curiosity = sc.add(0, HEALTHY_CURIOSITY, Zone.HAND)  # target only after "Then"
    st = sc.start()
    assert has_action(st, A.PLAY_COMMAND, curiosity)
    play(st, curiosity)
    assert len(st.zones[0][Zone.HAND]) == 1


@pytest.mark.rule("10-2-2", "10-1-8-1-1")
@pytest.mark.ruling("GD01-003:Q121")
def test_choose_two_needs_two_choosable_targets() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    sc.add(0, ZAKU_MARINER)
    sc.add(0, ZAKU_MARINER, rested=True)
    sc.add(1, BIGRO)
    hatred = sc.add(0, EXTREME_HATRED, Zone.HAND)  # "Choose 2 of your active Units."
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, hatred)


# ---------------------------------------------------------------------------------------------
# 10-1-9 substitution effects


@pytest.mark.rule("10-1-9-1")
def test_substitution_replaces_the_event() -> None:
    sc = Scenario()
    sc.trash(0, *[ZERO_GUNDAM] * 7)  # 7 (CB) cards
    exia = sc.add(0, EXIA, pilot=SETSUNA)  # 6/4 while paired
    other_cb = sc.add(0, ZERO_GUNDAM)
    non_cb = sc.add(0, ZAKU_MARINER)
    sc.shields(1, ZAKU_MARINER)
    st = sc.start()
    attack(st, exia)
    pass_all(st)
    assert ap(st, exia) == 7  # AP+1 once: "this Unit gets AP+1" did not happen as well
    assert ap(st, other_cb) == 3
    assert ap(st, non_cb) == 2

    sc = Scenario()
    sc.trash(0, *[ZERO_GUNDAM] * 6)
    exia = sc.add(0, EXIA, pilot=SETSUNA)
    other_cb = sc.add(0, ZERO_GUNDAM)
    sc.shields(1, ZAKU_MARINER)
    st = sc.start()
    attack(st, exia)
    pass_all(st)
    assert ap(st, exia) == 7
    assert ap(st, other_cb) == 2


@pytest.mark.rule("10-1-9-1")
def test_substituted_event_keeps_the_replaced_duration() -> None:
    sc = Scenario()
    sc.trash(0, *[ZERO_GUNDAM] * 7)
    exia = sc.add(0, EXIA, pilot=SETSUNA)
    other_cb = sc.add(0, ZERO_GUNDAM)
    sc.shields(1, ZAKU_MARINER, ZAKU_MARINER)
    st = sc.start()
    attack(st, exia)
    pass_all(st)
    assert ap(st, other_cb) == 3
    to_next_turn(st)
    assert ap(st, other_cb) == 2


@pytest.mark.rule("10-1-9-1-1")
@pytest.mark.ruling("GD03-109:Q248")
def test_replaced_portion_applies_when_the_substitution_does_not() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    sc.trash(0, IMPROVED_TECHNIQUE)
    sc.add(1, BIGRO)  # Lv.5
    technique = sc.add(0, IMPROVED_TECHNIQUE, Zone.HAND)
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, technique)

    sc = Scenario()
    sc.resources(0, 3)
    sc.trash(0, IMPROVED_TECHNIQUE)
    bigro = sc.add(1, BIGRO)
    gundam = sc.add(1, WBT_GUNDAM)  # Lv.4
    technique = sc.add(0, IMPROVED_TECHNIQUE, Zone.HAND)
    st = sc.start()
    play(st, technique)
    assert st.cards[gundam].damage == 3
    assert st.cards[bigro].damage == 0


@pytest.mark.rule("10-1-9-1-1")
@pytest.mark.ruling("GD03-109:Q247")
def test_instead_portion_replaces_only_what_it_names() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    sc.trash(0, IMPROVED_TECHNIQUE, IMPROVED_TECHNIQUE)
    bigro = sc.add(1, BIGRO)  # Lv.5: choosable only through the substitution
    technique = sc.add(0, IMPROVED_TECHNIQUE, Zone.HAND)
    st = sc.start()
    assert has_action(st, A.PLAY_COMMAND, technique)
    play(st, technique)
    assert st.cards[bigro].damage == 3


# ---------------------------------------------------------------------------------------------
# 10-2 effect conditions


@pytest.mark.rule("10-2-1")
def test_effect_does_not_activate_without_its_condition() -> None:
    for with_oz, expected_draws in ((False, 0), (True, 1)):
        sc = Scenario()
        aries = sc.add(0, NOINS_ARIES)  # 【Destroyed】If you have another (OZ) Unit, draw 1.
        if with_oz:
            sc.add(0, PISCES)
        bigro = sc.add(1, BIGRO, rested=True)
        st = sc.start()
        hand_before = len(st.zones[0][Zone.HAND])
        attack(st, aries, bigro)
        pass_all(st)
        assert zone_of(st, aries) is Zone.TRASH
        assert len(st.zones[0][Zone.HAND]) == hand_before + expected_draws


@pytest.mark.rule("10-2-2-1")
def test_targets_are_chosen_only_from_public_locations() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    respite = sc.add(0, MOMENTARY_RESPITE, Zone.HAND)  # choose a purple Pilot card from trash
    sc.add(0, AZEE, Zone.HAND)
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, respite)

    sc = Scenario()
    sc.resources(0, 4)
    respite = sc.add(0, MOMENTARY_RESPITE, Zone.HAND)
    sc.trash(0, AZEE)
    st = sc.start()
    assert has_action(st, A.PLAY_COMMAND, respite)

    # a card chosen from the hand or the deck is not a target
    sc = Scenario()
    sc.resources(0, 5)
    seizure = sc.add(0, AIRFRAME_SEIZURE, Zone.HAND)
    encounter = sc.add(0, ENCOUNTER, Zone.HAND)
    st = sc.start()
    assert not any(V.cdef(st, u).card_type is CardType.PILOT for u in st.zones[0][Zone.DECK])
    assert has_action(st, A.PLAY_COMMAND, seizure)
    assert has_action(st, A.PLAY_COMMAND, encounter)


@pytest.mark.rule("10-2-3")
def test_restriction_stops_an_effect_whose_conditions_are_met() -> None:
    for protected in (False, True):
        sc = Scenario()
        sc.resources(0, 3)
        sc.resources(1, 2)
        zaku = sc.add(0, ZAKU_MARINER)
        kindhearted = sc.add(0, KINDHEARTED, Zone.HAND)
        determination = sc.add(1, LOOK_OF_DETERMINATION, Zone.HAND)
        st = sc.start()
        if protected:
            play(st, kindhearted)  # friendly Units can't be destroyed by enemy effects
        end_main(st)
        assert _kind(st) is DecisionKind.ACTION_STEP and _decider(st) == 1
        play(st, determination)  # "Choose 1 active enemy Unit that is Lv.2 or lower. Destroy it."
        assert zone_of(st, determination) is Zone.TRASH
        assert zone_of(st, zaku) is (Zone.BATTLE if protected else Zone.TRASH)


# ---------------------------------------------------------------------------------------------
# 10-3 effect activation steps


@pytest.mark.rule("10-3-1-2")
def test_activation_is_declared_and_a_hand_card_is_revealed() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    aces = sc.add(0, BATTLE_OF_ACES, Zone.HAND, known=False)
    sc.add(1, BIGRO, damage=1)
    sc.add(1, WBT_GUNDAM, damage=1)
    st = sc.start()
    assert st.cards[aces].known == 0b01  # only its owner knows it
    play(st, aces)
    assert _kind(st) is DecisionKind.SELECT
    assert st.cards[aces].known == 0b11

    sc = Scenario()
    sc.resources(0, 1)
    turn_a = sc.add(0, TURN_A)
    st = sc.start()
    activate(st, turn_a)
    assert any(h.kind == "activate" and h.uid == turn_a and h.player == 0 for h in st.history)


@pytest.mark.rule("10-3-1-4")
@pytest.mark.ruling("GD03-001:Q209")
def test_events_caused_by_an_activation_resolve_before_play_continues() -> None:
    sc = Scenario()
    sc.resources(0, 1)
    sc.trash(0, RIDDHE)
    gfred = sc.add(0, GFRED)
    duel = sc.add(1, DUEL_ASSAULT, damage=1)  # 【Destroyed】Place 1 EX Resource.
    st = sc.start()
    activate(st, gfred)
    assert zone_of(st, duel) is Zone.TRASH
    assert _ex_resources(st, 1) == 1
    assert _kind(st) is DecisionKind.MAIN and _decider(st) == 0


@pytest.mark.rule("10-3-2")
def test_command_card_is_presented_and_its_effect_performed() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    aces = sc.add(0, BATTLE_OF_ACES, Zone.HAND)
    bigro = sc.add(1, BIGRO, damage=1)
    gundam = sc.add(1, WBT_GUNDAM, damage=1)
    st = sc.start()
    play(st, aces)
    assert zone_of(st, aces) is Zone.RESOLVING
    assert _select_options(st) == {bigro, gundam}
    select(st, bigro)  # 1 + 3 damage reaches its 4 HP
    assert zone_of(st, bigro) is Zone.TRASH
    assert st.cards[gundam].damage == 1
    assert zone_of(st, aces) is Zone.TRASH


@pytest.mark.rule("10-3-3")
def test_targets_are_chosen_when_the_instruction_is_reached() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    sc.trash(0, HEALTHY_CURIOSITY, HEALTHY_CURIOSITY)
    curiosity = sc.add(0, HEALTHY_CURIOSITY, Zone.HAND)
    enemies = {sc.add(1, BIGRO), sc.add(1, WBT_GUNDAM)}
    st = sc.start()
    play(st, curiosity)  # "Draw 1. Then, ... choose 1 enemy Unit with 4 or less HP."
    assert _kind(st) is DecisionKind.SELECT and _select_options(st) == enemies
    assert len(st.zones[0][Zone.HAND]) == 1  # the draw already happened


@pytest.mark.rule("10-3-3")
@pytest.mark.ruling("GD03-039:Q223")
def test_triggered_effect_chooses_each_target_as_its_text_is_reached() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    clan = sc.add(0, CLAN_ZAKU)
    bigro = sc.add(1, BIGRO)  # no enemy Unit with 2 or less AP for the second choice
    red = sc.add(0, RED_GUNDAM, Zone.HAND)
    st = sc.start()
    play(st, red)
    assert st.cards[clan].rested
    assert st.cards[bigro].damage == 0
    assert _kind(st) is DecisionKind.MAIN


@pytest.mark.rule("10-3-3-1")
def test_triggered_effect_without_a_choosable_target_does_not_activate() -> None:
    sc = Scenario()
    sc.resources(0, 1)
    enemy = sc.add(1, ZAKU_MARINER)  # active, so not a legal "rested enemy Unit"
    hunter = sc.add(0, JEGAN_MAN_HUNTER, Zone.HAND)
    st = sc.start()
    play(st, hunter)
    assert _kind(st) is DecisionKind.MAIN
    assert st.lasting == []
    assert not st.cards[enemy].rested


@pytest.mark.rule("10-3-3-1", "10-2-2")
@pytest.mark.ruling("GD03-056:Q230")
def test_triggered_effect_needs_every_target_of_its_choice() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    ours = sc.add(0, WBT_GUNDAM)
    adapt = sc.add(0, BARBATOS_ADAPT, Zone.HAND)  # "Choose 1 of your Units and 1 enemy Unit."
    st = sc.start()
    play(st, adapt)
    assert _kind(st) is DecisionKind.MAIN
    assert st.cards[ours].damage == 0
    assert st.cards[adapt].damage == 0


@pytest.mark.rule("10-3-4")
def test_effect_without_a_stated_player_refers_to_the_cards_owner() -> None:
    sc = Scenario()
    first = sc.add(0, BIGRO)
    second = sc.add(0, BIGRO)
    duel = sc.add(1, DUEL_ASSAULT, rested=True)  # 【Destroyed】Place 1 EX Resource.
    taurus = sc.add(1, NOINS_TAURUS, rested=True)  # 【Destroyed】Draw 1. Then, discard 1.
    st = sc.start()
    p0_hand = len(st.zones[0][Zone.HAND])
    p1_deck = len(st.zones[1][Zone.DECK])
    attack(st, first, duel)
    pass_all(st)
    assert _ex_resources(st, 1) == 1
    assert _ex_resources(st, 0) == 0
    attack(st, second, taurus)
    pass_all(st)
    assert len(st.zones[1][Zone.DECK]) == p1_deck - 1
    assert len(st.zones[1][Zone.TRASH]) == 3  # Duel, Taurus, and the drawn-then-discarded card
    assert len(st.zones[0][Zone.HAND]) == p0_hand


@pytest.mark.rule("10-3-5")
def test_choosing_from_the_deck_confirms_the_top_cards_first() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    sc.deck(0, ZAKU_MARINER, AIRFRAME_SEIZURE, DAMAGE_CONTROL, BATTLE_OF_ACES)
    demi = sc.add(0, DEMI_BARDING, Zone.HAND)
    st = sc.start()
    top = st.zones[0][Zone.DECK][:4]
    play(st, demi)
    assert [bool(st.cards[u].known & 0b01) for u in top] == [True, True, True, False]
    assert not any(st.cards[u].known & 0b10 for u in top)
    yes(st)
    assert _kind(st) is DecisionKind.SELECT and _decider(st) == 0
    assert _select_options(st) == {top[1], top[2]}  # the 4th card is not among them
    select(st, top[1])
    assert zone_of(st, top[1]) is Zone.HAND
    assert st.zones[0][Zone.DECK][0] == top[3]
    assert set(st.zones[0][Zone.DECK][-2:]) == {top[0], top[2]}


@pytest.mark.rule("10-1-5", "5-22-4")
def test_derived_view_follows_battles_and_the_turn_player_without_explicit_invalidation() -> None:
    sc = Scenario()
    mine = sc.add(0, "GD01-041")
    carta = sc.add(1, "GD02-073")  # the enemy Unit battling it gains <First Strike> on my turn
    st = sc.start()
    assert not V.has_kw(V.derived(st), mine, d.Kw.FIRST_STRIKE)
    st.battles.append(
        Battle(
            attacker=mine,
            attacker_seq=st.cards[mine].zone_seq,
            target=carta,
            target_seq=st.cards[carta].zone_seq,
            defender=1,
            battle_id=st.next_battle_id,
        )
    )
    assert V.has_kw(V.derived(st), mine, d.Kw.FIRST_STRIKE)
    st.active = 1
    assert not V.has_kw(V.derived(st), mine, d.Kw.FIRST_STRIKE)
