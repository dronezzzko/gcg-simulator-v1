"""Rules section 2 (Card Information) and section 5 (Essential Game Terminology)."""

from __future__ import annotations

import dataclasses
import json
from pathlib import Path

import pytest

from gcg_sim.cards.db import CardDB, load_raw_printings
from gcg_sim.cards.model import CardType, Color
from gcg_sim.effects import dsl as d
from gcg_sim.effects.compiler import compile_card
from gcg_sim.effects.registry import get_registry
from gcg_sim.engine import core
from gcg_sim.engine import interp as I
from gcg_sim.engine import view as V
from gcg_sim.engine.game import SUPPORT_AID, DeckList, advance, apply, new_game
from gcg_sim.engine.observe import is_hidden_from
from gcg_sim.engine.state import Action, GameState
from gcg_sim.engine.types import NO_ARG, PLAYER_TARGET, ActionKind, DecisionKind, Zone
from gcg_sim.testkit import (
    Scenario,
    activate,
    ap,
    attack,
    block,
    has_action,
    hp,
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

ZAKU_MARINER = "GD01-060"  # Red Unit Lv2 cost1 2/2 (Zeon), vanilla, zone Earth
GOOHN = "GD01-062"  # Red Unit Lv1 cost1 1/2 (ZAFT), vanilla
DREISSEN = "GD01-057"  # Red Unit Lv2 cost2 2/3 (Neo Zeon), vanilla
DRA_C = "ST03-005"  # Red Unit Lv1 cost1 1/2 (Neo Zeon), vanilla, zone Space
LOTO = "GD01-011"  # Blue Unit 2/2 (Earth Federation), vanilla
CORE_BOOSTER = "GD02-012"  # Blue Unit 2/2 (Earth Federation)(White Base Team), vanilla
GUNDAM_VANILLA = "GD01-013"  # Blue Unit Lv4 cost2 3/4 (EF)(WBT), link [Amuro Ray], vanilla
KSHATRIYA = "GD01-051"  # Red Unit Lv4 3/4, vanilla
XAVIER_GYAN = "GD04-032"  # Green Unit Lv5 4/5, vanilla
GUNDAM_EXIA_GD04 = "GD04-064"  # "Gundam Exia", vanilla
GUNDAM_EXIA_ST07 = "ST07-002"  # another card number named "Gundam Exia", vanilla
MOEBIUS_ZERO = "ST04-003"  # White Unit Lv3 cost2 2/4, link [Mu La Flaga], vanilla
PISCES = "GD01-021"  # Blue Unit Lv1 1/2 (OZ), vanilla

GUNDAM_WBT_REPAIR = "GD01-001"  # All your (White Base Team) Units gain <Repair 1>.
JEGAN_HAND_DISCOUNT = "GD01-016"  # 2+ (Earth Federation) Units in play: this card in hand cost -1
KSHATRIYA_BESSERUNG = "GD03-005"  # <Repair 1> / 【Deploy】Draw 1.
NOINS_ARIES = "GD01-007"  # (OZ) 2/3 【Destroyed】If you have another (OZ) Unit in play, draw 1.
NOINS_TAURUS = "GD05-074"  # 【Destroyed】Draw 1. Then, discard 1.
STRIKE_DRAW_DISCARD = "ST04-002"  # 【Deploy】Draw 1. Then, discard 1.
GQUUUUUUX = "GD02-034"  # 0 AP; 【During Pair･Red Pilot】This Unit gets AP+2.
METHUSS = "GD02-081"  # 【Deploy】If a friendly white Base is in play, 1 enemy Unit gets AP-2.
TITUS = "GD02-031"  # 2 AP; While you are Lv.7 or higher, this Unit gets AP+2.
MICHAELIS = "GD01-076"  # 3/3; 4+ Command cards in your trash: AP+1 and HP+1.
RED_FRAME_EX = "EB01-001"  # 【Activate･Main】Exile 2 Command cards from trash: rest an enemy
LAUNCHER_STRIKE = "GD01-072"  # <Blocker> with reminder text
GEARA_ZULU_SUPPORT = "ST03-002"  # 【Activate･Main】<Support 2>

AMURO_RAY = "ST01-010"  # Blue Pilot named "Amuro Ray"
RIDDHE = "GD01-089"  # Blue Pilot Riddhe Marcenas, +1/+1
DEUX_MURASAME = "GD04-091"  # Red Pilot, +2/+1
FULL_FRONTAL = "ST03-010"  # 【When Paired】may deploy 1 (Neo Zeon)/(Zeon) Unit card from hand

ZEON_REMNANT = "GD01-115"  # Command 【Main】/【Action】Choose 1 enemy Unit. Deal 1 damage to it.
INDIGNATION = "ST03-012"  # Command: 1 friendly Unit gets AP+2 during this turn.
FATAL_STRIKE = "ST05-014"  # Command 【Main】Choose 1 enemy Unit that is Lv.3 or lower. Destroy it.
HAWK_OF_ENDYMION = "ST04-013"  # return enemy Unit to its owner's hand; 【Pilot】[Mu La Flaga]
REFORMATIONIST = "GD04-114"  # 【Burst】1 Unit card with "Trans-Am" in its name: trash to hand
AWAKENED_POWER = "GD02-110"  # 【Main】1 Unit card from your trash: Pay its cost to deploy it.
HEALTHY_CURIOSITY = "GD03-101"  # 【Main】Draw 1. Then, ...
ENCOUNTER = "GD04-105"  # Look at top 5 ... Return the remaining cards randomly to the bottom.
BURST_ADD = "GD01-097"  # 【Burst】Add this card to your hand.

GN_ARMOR_TRANS_AM = "GD04-019"  # "GN Armor Type-D (Trans-Am)"
KYRIOS_TRANS_AM = "GD04-037"  # "Gundam Kyrios (Trans-Am)"

SIDE_7 = "GD01-124"  # Blue Base Lv1 cost1; 【Deploy】Shield to hand; 【Activate･Main】Rest this Base: ...
ARMORY_ONE = "GD04-128"  # Base HP6; 【Destroyed】All players draw 1.
ARGAMA = "GD02-129"  # White Base

MAIN_DECK_TYPES = (CardType.UNIT, CardType.PILOT, CardType.COMMAND, CardType.BASE)
CONFLICTS = Path(__file__).resolve().parents[2] / "data" / "conflicts.json"


def _db() -> CardDB:
    return get_registry().db


def _resolve_steps(
    st: GameState,
    steps: tuple[d.Step, ...],
    *,
    controller: int,
    variables: dict[str, tuple[int, ...]] | None = None,
) -> None:
    """Resolve an effect body built from engine DSL steps at the current point of the game."""
    pid = get_registry().program(steps, "test:cardinfo")
    st.pending = None
    I.push_frame(st, pid, controller=controller, host=NO_ARG, kind="trigger", vars=variables)
    advance(st)


def _option_cards(st: GameState, kind: ActionKind) -> set[int]:
    return {o.a for o in options(st) if o.kind is kind}


def _hand_size(st: GameState, player: int) -> int:
    return len(st.zones[player][Zone.HAND])


def _deck_size(st: GameState, player: int) -> int:
    return len(st.zones[player][Zone.DECK])


# ---------------------------------------------------------------------------------------------
# 2-1 Card number


@pytest.mark.rule("2-1-1")
def test_every_printing_of_a_card_number_is_the_same_card() -> None:
    db = _db()
    printings: dict[str, list[str]] = {}
    for rec in load_raw_printings():
        printings.setdefault(rec["card_number"], []).append(rec["product_id"])
    reprinted = {n: ps for n, ps in printings.items() if len(ps) > 1}
    assert len(reprinted) > 100
    for number, product_ids in reprinted.items():
        for product_id in product_ids:
            assert db[product_id] is db[number]


@pytest.mark.rule("2-1-1")
def test_alternate_printing_behaves_as_the_same_card_in_game() -> None:
    sc = Scenario()
    base_print = sc.add(0, GUNDAM_WBT_REPAIR)
    alt_print = sc.add(0, "GD01-001_p1")
    booster = sc.add(0, CORE_BOOSTER)
    st = sc.start()
    assert st.cards[base_print].def_id == st.cards[alt_print].def_id
    assert keywords(st, alt_print) == {"Repair": 2}
    assert keywords(st, booster) == {"Repair": 2}


# ---------------------------------------------------------------------------------------------
# 2-2 Card name


@pytest.mark.rule("2-2-1")
def test_card_name_is_the_printed_name() -> None:
    db = _db()
    assert db[ZAKU_MARINER].name == "Zaku Mariner"
    assert db[ZAKU_MARINER].names == ("Zaku Mariner",)
    assert db[GUNDAM_VANILLA].name == "Gundam"
    sc = Scenario()
    zaku = sc.add(0, ZAKU_MARINER)
    st = sc.start()
    assert V.cdef(st, zaku).name == "Zaku Mariner"


@pytest.mark.rule("2-2-2")
def test_reference_by_card_name_selects_cards_with_that_name() -> None:
    """GD01-013 Gundam's link condition names [Amuro Ray]: a Pilot card named Amuro Ray
    satisfies it, a Pilot with another name does not."""
    for pilot, links in ((AMURO_RAY, True), (RIDDHE, False)):
        sc = Scenario()
        sc.resources(0, 4)
        gundam = sc.add(0, GUNDAM_VANILLA, deployed_this_turn=True)
        card = sc.add(0, pilot, Zone.HAND)
        st = sc.start()
        assert not has_action(st, A.ATTACK, gundam)
        play(st, card, onto=gundam)
        assert V.is_linked(V.derived(st), gundam) is links
        assert has_action(st, A.ATTACK, gundam) is links

    sc = Scenario()
    amuro = sc.add(0, AMURO_RAY, Zone.TRASH)
    riddhe = sc.add(0, RIDDHE, Zone.TRASH)
    st = sc.start()
    dv = V.derived(st)
    ctx = V.Ctx(0)
    by_name = (d.NameIs(("Amuro Ray",)),)
    assert V.matches(st, dv, ctx, amuro, by_name)
    assert not V.matches(st, dv, ctx, riddhe, by_name)
    assert not V.matches(st, dv, ctx, amuro, (d.NameIs(("Amuro",)),))


@pytest.mark.rule("2-2-3", "2-2-1")
def test_name_portion_reference_matches_every_name_containing_it() -> None:
    sc = Scenario()
    attacker = sc.add(0, ZAKU_MARINER)
    sc.shields(1, REFORMATIONIST)
    gn_armor, kyrios, exia = sc.trash(1, GN_ARMOR_TRANS_AM, KYRIOS_TRANS_AM, GUNDAM_EXIA_GD04)
    st = sc.start()
    attack(st, attacker)
    pass_all(st)
    assert st.pending is not None and st.pending.kind is DecisionKind.BURST
    yes(st)
    assert st.pending is not None and st.pending.player == 1
    assert _option_cards(st, A.SELECT) == {gn_armor, kyrios}
    select(st, gn_armor)
    assert zone_of(st, gn_armor) is Zone.HAND
    assert zone_of(st, kyrios) is Zone.TRASH
    assert zone_of(st, exia) is Zone.TRASH


@pytest.mark.rule("2-2-4")
def test_command_with_pilot_effect_has_two_card_names() -> None:
    db = _db()
    assert db[HAWK_OF_ENDYMION].names == ("Hawk of Endymion", "Mu La Flaga")
    sc = Scenario()
    sc.resources(0, 2)
    moebius = sc.add(0, MOEBIUS_ZERO, deployed_this_turn=True)
    hawk = sc.add(0, HAWK_OF_ENDYMION, Zone.HAND)
    st = sc.start()
    assert not has_action(st, A.ATTACK, moebius)
    play(st, hawk, onto=moebius)
    assert zone_of(st, hawk) is Zone.PAIRED
    assert V.is_linked(V.derived(st), moebius)
    assert has_action(st, A.ATTACK, moebius)


@pytest.mark.rule("2-2-5")
def test_cards_with_the_same_name_can_be_on_the_field_together() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    first = sc.add(0, ZAKU_MARINER)
    second = sc.add(0, ZAKU_MARINER, Zone.HAND)
    exia_gd04 = sc.add(0, GUNDAM_EXIA_GD04)
    exia_st07 = sc.add(0, GUNDAM_EXIA_ST07, Zone.HAND)
    st = sc.start()
    play(st, second)
    play(st, exia_st07)
    battle = st.zones[0][Zone.BATTLE]
    assert {first, second, exia_gd04, exia_st07} <= set(battle)
    assert V.cdef(st, exia_gd04).name == V.cdef(st, exia_st07).name == "Gundam Exia"


# ---------------------------------------------------------------------------------------------
# 2-3 Card type


@pytest.mark.rule("2-3-1")
def test_card_type_decides_how_a_card_is_played() -> None:
    db = _db()
    assert db[ZAKU_MARINER].card_type is CardType.UNIT
    assert db[SIDE_7].card_type is CardType.BASE
    assert db[RIDDHE].card_type is CardType.PILOT
    assert db[ZEON_REMNANT].card_type is CardType.COMMAND
    sc = Scenario()
    sc.resources(0, 4)
    mine = sc.add(0, GUNDAM_VANILLA)
    sc.add(1, ZAKU_MARINER)
    unit, base, pilot, command = sc.hand(0, ZAKU_MARINER, SIDE_7, RIDDHE, ZEON_REMNANT)
    st = sc.start()
    assert _option_cards(st, A.PLAY_UNIT) == {unit}
    assert _option_cards(st, A.PLAY_BASE) == {base}
    assert _option_cards(st, A.PAIR) == {pilot}
    assert {o.b for o in options(st) if o.kind is A.PAIR} == {mine}
    assert _option_cards(st, A.PLAY_COMMAND) == {command}
    play(st, unit)
    play(st, base)
    play(st, pilot, onto=mine)
    assert zone_of(st, unit) is Zone.BATTLE
    assert zone_of(st, base) is Zone.BASE
    assert zone_of(st, pilot) is Zone.PAIRED


@pytest.mark.rule("2-3-2")
def test_there_are_five_card_types() -> None:
    real = [c for c in _db().real_cards() if not c.is_token]
    assert {c.card_type for c in real} == {
        CardType.UNIT,
        CardType.PILOT,
        CardType.COMMAND,
        CardType.BASE,
        CardType.RESOURCE,
    }
    for token in (c for c in _db().real_cards() if c.is_token):
        kinds = (token.card_type.is_unit, token.card_type.is_base, token.card_type.is_resource)
        assert kinds.count(True) == 1


# ---------------------------------------------------------------------------------------------
# 2-4 Color


@pytest.mark.rule("2-4-1")
def test_color_referenced_by_card_text() -> None:
    db = _db()
    assert db[DEUX_MURASAME].color is Color.RED
    assert db[RIDDHE].color is Color.BLUE
    sc = Scenario()
    red_paired = sc.add(0, GQUUUUUUX, pilot=DEUX_MURASAME)
    blue_paired = sc.add(0, GQUUUUUUX, pilot=RIDDHE)
    st = sc.start()
    assert ap(st, red_paired) == db[GQUUUUUUX].ap + db[DEUX_MURASAME].ap + 2
    assert ap(st, blue_paired) == db[GQUUUUUUX].ap + db[RIDDHE].ap


@pytest.mark.rule("2-4-2")
def test_all_cards_except_resources_and_tokens_have_a_color() -> None:
    for card in _db().real_cards():
        if card.is_token or card.card_type is CardType.RESOURCE:
            assert card.color is None, card.card_number
        else:
            assert isinstance(card.color, Color), card.card_number


@pytest.mark.rule("2-4-2")
def test_ex_base_token_is_not_a_white_base() -> None:
    for base, enemy_ap in ((None, 2), (ARGAMA, 0)):
        sc = Scenario()
        sc.resources(0, 2)
        sc.base(0, base)
        enemy = sc.add(1, ZAKU_MARINER)
        methuss = sc.add(0, METHUSS, Zone.HAND)
        st = sc.start()
        play(st, methuss)
        assert ap(st, enemy) == enemy_ap


@pytest.mark.rule("2-4-2-1")
def test_there_are_five_card_colors() -> None:
    colors = {c.color for c in _db().real_cards() if c.color is not None}
    assert colors == {Color.BLUE, Color.GREEN, Color.RED, Color.WHITE, Color.PURPLE}


# ---------------------------------------------------------------------------------------------
# 2-5 Trait


@pytest.mark.rule("2-5-1", "2-5-3")
def test_trait_in_text_refers_to_cards_with_that_trait() -> None:
    db = _db()
    assert db[CORE_BOOSTER].traits == ("Earth Federation", "White Base Team")
    assert db[LOTO].traits == ("Earth Federation",)
    sc = Scenario()
    gundam = sc.add(0, GUNDAM_WBT_REPAIR)
    booster = sc.add(0, CORE_BOOSTER)
    loto = sc.add(0, LOTO)
    st = sc.start()
    assert keywords(st, gundam) == {"Repair": 1}
    assert keywords(st, booster) == {"Repair": 1}
    assert keywords(st, loto) == {}


@pytest.mark.rule("2-5-2", "2-5-3")
def test_card_with_several_traits_is_referred_to_by_each_of_them() -> None:
    """Core Booster is (Earth Federation) and (White Base Team): an (Earth Federation)
    count includes it just as a (White Base Team) reference does."""
    for partner, cost in ((LOTO, 1), (ZAKU_MARINER, 2)):
        sc = Scenario()
        sc.resources(0, 3, rested=2)
        sc.add(0, CORE_BOOSTER)
        sc.add(0, partner)
        jegan = sc.add(0, JEGAN_HAND_DISCOUNT, Zone.HAND)
        st = sc.start()
        assert V.play_cost(st, V.derived(st), jegan) == cost
        assert has_action(st, A.PLAY_UNIT, jegan) is (cost == 1)


# ---------------------------------------------------------------------------------------------
# 2-6 Zone


@pytest.mark.rule("2-6-1")
def test_zone_is_card_data_that_filters_can_refer_to() -> None:
    db = _db()
    assert db[CORE_BOOSTER].zones == ("Space", "Earth")
    assert db[ZAKU_MARINER].zones == ("Earth",)
    assert db[DRA_C].zones == ("Space",)
    sc = Scenario()
    booster = sc.add(0, CORE_BOOSTER)
    zaku = sc.add(0, ZAKU_MARINER)
    dra_c = sc.add(0, DRA_C)
    st = sc.start()
    dv = V.derived(st)
    space = (d.HasZone(("Space",)),)
    assert [u for u in (booster, zaku, dra_c) if V.matches(st, dv, V.Ctx(0), u, space)] == [
        booster,
        dra_c,
    ]


# ---------------------------------------------------------------------------------------------
# 2-7 AP, 2-8 HP


@pytest.mark.rule("2-7-1")
def test_ap_is_the_battle_damage_a_unit_deals() -> None:
    sc = Scenario()
    attacker = sc.add(0, GUNDAM_VANILLA)
    target = sc.add(1, XAVIER_GYAN, rested=True)
    st = sc.start()
    attack(st, attacker, target)
    pass_all(st)
    assert st.cards[target].damage == 3 == _db()[GUNDAM_VANILLA].ap
    assert zone_of(st, attacker) is Zone.TRASH


@pytest.mark.rule("2-7-1")
def test_modified_ap_is_the_battle_damage_a_unit_deals() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    attacker = sc.add(0, GUNDAM_VANILLA)
    target = sc.add(1, XAVIER_GYAN, rested=True)
    indignation = sc.add(0, INDIGNATION, Zone.HAND)
    st = sc.start()
    play(st, indignation)
    assert ap(st, attacker) == 5
    attack(st, attacker, target)
    pass_all(st)
    assert zone_of(st, target) is Zone.TRASH


@pytest.mark.rule("2-8-1", "2-8-2")
def test_unit_survives_damage_below_its_hp_and_is_destroyed_at_zero() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    target = sc.add(1, GUNDAM_VANILLA, damage=2)
    first, second = sc.hand(0, ZEON_REMNANT, ZEON_REMNANT)
    st = sc.start()
    assert hp(st, target) == 4
    play(st, first)
    assert zone_of(st, target) is Zone.BATTLE
    assert st.cards[target].damage == 3
    play(st, second)
    assert zone_of(st, target) is Zone.TRASH


@pytest.mark.rule("2-8-2")
def test_unit_is_destroyed_when_its_hp_drops_to_its_damage() -> None:
    sc = Scenario()
    red_frame = sc.add(0, RED_FRAME_EX)
    michaelis = sc.add(0, MICHAELIS, damage=3)
    commands = sc.trash(0, ZEON_REMNANT, ZEON_REMNANT, ZEON_REMNANT, ZEON_REMNANT)
    sc.add(1, ZAKU_MARINER, damage=1)
    st = sc.start()
    assert hp(st, michaelis) == 4
    assert zone_of(st, michaelis) is Zone.BATTLE
    activate(st, red_frame)
    select(st, commands[0], commands[1])
    assert zone_of(st, michaelis) is Zone.TRASH


# ---------------------------------------------------------------------------------------------
# 2-9 Lv., 2-10 Cost


@pytest.mark.rule("2-9-1")
def test_level_counts_rested_resources_too() -> None:
    sc = Scenario()
    sc.resources(0, 4, rested=2)
    gundam = sc.add(0, GUNDAM_VANILLA, Zone.HAND)
    st = sc.start()
    assert has_action(st, A.PLAY_UNIT, gundam)

    sc = Scenario()
    sc.resources(0, 3)
    gundam = sc.add(0, GUNDAM_VANILLA, Zone.HAND)
    st = sc.start()
    assert not has_action(st, A.PLAY_UNIT, gundam)


@pytest.mark.rule("2-9-2")
def test_all_cards_except_resources_and_tokens_have_a_level() -> None:
    for rec in load_raw_printings():
        ctype = CardType(rec["card_type"])
        if ctype in MAIN_DECK_TYPES:
            assert isinstance(rec["level"], int) and rec["level"] >= 1, rec["product_id"]
        else:
            assert rec["level"] is None, rec["product_id"]
    sc = Scenario()
    ex_resource = sc.resources(0, 0, ex=1)[0]
    token = sc.base(0)
    st = sc.start()
    assert V.level_of(st, ex_resource) == 0
    assert V.level_of(st, token) == 0


@pytest.mark.rule("2-9-4")
def test_player_level_is_the_number_of_resources_including_ex_resources() -> None:
    for normal, ex, titus_ap in ((6, 0, 2), (6, 1, 4), (7, 0, 4)):
        sc = Scenario()
        sc.resources(0, normal, rested=normal, ex=ex)
        titus = sc.add(0, TITUS)
        st = sc.start()
        assert ap(st, titus) == titus_ap


@pytest.mark.rule("2-10-1")
def test_cost_is_paid_by_resting_active_resources() -> None:
    sc = Scenario()
    res = sc.resources(0, 4)
    gundam = sc.add(0, GUNDAM_VANILLA, Zone.HAND)
    st = sc.start()
    play(st, gundam)
    assert sum(st.cards[r].rested for r in res) == _db()[GUNDAM_VANILLA].cost == 2

    sc = Scenario()
    sc.resources(0, 4, rested=3)
    gundam = sc.add(0, GUNDAM_VANILLA, Zone.HAND)
    st = sc.start()
    assert not has_action(st, A.PLAY_UNIT, gundam)


@pytest.mark.rule("2-10-2")
def test_all_cards_except_resources_and_tokens_have_a_cost() -> None:
    for rec in load_raw_printings():
        ctype = CardType(rec["card_type"])
        if ctype in MAIN_DECK_TYPES:
            assert isinstance(rec["cost"], int) and rec["cost"] >= 1, rec["product_id"]
        else:
            assert rec["cost"] is None, rec["product_id"]
    sc = Scenario()
    ex_resource = sc.resources(0, 0, ex=1)[0]
    token = sc.base(0)
    st = sc.start()
    assert V.cost_of(st, ex_resource) == 0
    assert V.cost_of(st, token) == 0


# ---------------------------------------------------------------------------------------------
# 2-11 Card text, 5-1 Effect


@pytest.mark.rule("2-11-1", "5-1-1")
def test_card_text_is_the_cards_effects() -> None:
    reg = get_registry()
    vanilla = reg.cards[reg.db[ZAKU_MARINER].def_id]
    assert reg.db[ZAKU_MARINER].effect == "-"
    assert vanilla.own == () and vanilla.unit == ()
    printed = reg.cards[reg.db[KSHATRIYA_BESSERUNG].def_id]
    abilities = [reg.abilities[a].ability for a in printed.own]
    assert [type(a) for a in abilities] == [d.Keyword, d.Triggered]

    sc = Scenario()
    sc.resources(0, 7)
    besserung, zaku = sc.hand(0, KSHATRIYA_BESSERUNG, ZAKU_MARINER)
    st = sc.start()
    deck = _deck_size(st, 0)
    play(st, zaku)
    assert _deck_size(st, 0) == deck
    play(st, besserung)
    assert _deck_size(st, 0) == deck - 1
    assert keywords(st, besserung) == {"Repair": 1}


@pytest.mark.rule("2-11-2")
def test_unit_text_only_works_in_the_battle_area() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    gundam = sc.add(0, GUNDAM_WBT_REPAIR, Zone.HAND)
    sc.add(0, GUNDAM_WBT_REPAIR, Zone.TRASH)
    sc.shields(0, GUNDAM_WBT_REPAIR)
    booster = sc.add(0, CORE_BOOSTER)
    st = sc.start()
    assert keywords(st, booster) == {}
    play(st, gundam)
    assert keywords(st, booster) == {"Repair": 1}


@pytest.mark.rule("2-11-2")
def test_base_text_only_works_in_the_base_section() -> None:
    sc = Scenario()
    sc.resources(0, 1)
    sc.shields(0, ZAKU_MARINER)
    sc.add(0, ZAKU_MARINER)
    side_7 = sc.add(0, SIDE_7, Zone.HAND)
    st = sc.start()
    assert not has_action(st, A.ACTIVATE, side_7)
    play(st, side_7)
    assert zone_of(st, side_7) is Zone.BASE
    assert has_action(st, A.ACTIVATE, side_7)


@pytest.mark.rule("2-11-2")
def test_text_that_names_the_hand_works_in_the_hand() -> None:
    sc = Scenario()
    sc.add(0, LOTO)
    sc.add(0, CORE_BOOSTER)
    jegan = sc.add(0, JEGAN_HAND_DISCOUNT, Zone.HAND)
    st = sc.start()
    assert V.play_cost(st, V.derived(st), jegan) == _db()[JEGAN_HAND_DISCOUNT].cost - 1


@pytest.mark.rule("2-11-4")
def test_reminder_text_does_not_change_compiled_effects() -> None:
    db = _db()
    gundam = db[GUNDAM_WBT_REPAIR]
    assert "(At the end of your turn" in gundam.effect
    without_reminder = gundam.effect.replace(
        "(At the end of your turn, this Unit recovers the specified number of HP.)", ""
    )
    assert compile_card(dataclasses.replace(gundam, effect=without_reminder)) == compile_card(
        gundam
    )


@pytest.mark.rule("2-11-4")
def test_printings_differing_only_in_reminder_text_compile_identically() -> None:
    reg = get_registry()
    conflicts = json.loads(CONFLICTS.read_text(encoding="utf-8"))["conflicts"]
    checked = []
    for conflict in conflicts:
        if conflict["kind"] != "divergent_printing":
            continue
        for field in conflict["details"]["fields"]:
            if field["field"] != "effect" or field["classification"] != "reminder_only":
                continue
            (number,) = conflict["card_numbers"]
            card = reg.db[number]
            script = reg.cards[card.def_id].script
            if script is None or script.source not in ("compiled", "vanilla"):
                continue
            canonical = compile_card(card)
            for variant in field["variants"]:
                assert compile_card(dataclasses.replace(card, effect=variant["value"])) == (
                    canonical
                ), (number, variant["product_ids"])
            checked.append(number)
    assert "GD01-030" in checked
    assert len(checked) >= 10


@pytest.mark.rule("2-11-4")
def test_blocker_reminder_text_adds_no_activated_effect() -> None:
    assert "(Rest this Unit" in _db()[LAUNCHER_STRIKE].effect
    sc = Scenario()
    launcher = sc.add(0, LAUNCHER_STRIKE)
    st = sc.start()
    assert keywords(st, launcher) == {"Blocker": 1}
    assert not has_action(st, A.ACTIVATE, launcher)


@pytest.mark.rule("5-1-2")
def test_keyword_effects_blocker_and_support() -> None:
    sc = Scenario()
    attacker = sc.add(0, ZAKU_MARINER)
    blocker = sc.add(1, LAUNCHER_STRIKE)
    st = sc.start()
    attack(st, attacker)
    assert st.pending is not None and st.pending.kind is DecisionKind.BLOCK
    assert _option_cards(st, A.BLOCK) == {blocker}

    sc = Scenario()
    geara = sc.add(0, GEARA_ZULU_SUPPORT)
    zaku = sc.add(0, ZAKU_MARINER)
    st = sc.start()
    assert keywords(st, geara) == {"Support": 2}
    assert Action(A.ACTIVATE, geara, SUPPORT_AID, 0) in options(st)
    activate(st, geara, SUPPORT_AID)
    assert st.cards[geara].rested
    assert ap(st, zaku) == 4


# ---------------------------------------------------------------------------------------------
# 5-2 Player / owner, 5-3 Active and standby player


def _vanilla_deck() -> DeckList:
    units = [
        ZAKU_MARINER,
        GOOHN,
        DREISSEN,
        DRA_C,
        LOTO,
        CORE_BOOSTER,
        GUNDAM_VANILLA,
        KSHATRIYA,
        XAVIER_GYAN,
        GUNDAM_EXIA_GD04,
        MOEBIUS_ZERO,
        PISCES,
        GUNDAM_EXIA_ST07,
    ]
    main = tuple(n for n in units for _ in range(4))[:50]
    return DeckList(main=main, resources=("R-001",) * 10)


def _keep_both(st: GameState) -> None:
    for _ in range(2):
        assert st.pending is not None and st.pending.kind is DecisionKind.REDRAW
        apply(st, Action(A.KEEP))


@pytest.mark.rule("5-2-1")
def test_every_card_is_owned_by_the_player_whose_deck_it_came_from() -> None:
    st = new_game((_vanilla_deck(), _vanilla_deck()), seed=7, chooser=0)
    apply(st, Action(A.GO_FIRST, 0))
    _keep_both(st)
    for player in (0, 1):
        for zone in Zone:
            for uid in st.zones[player][zone]:
                assert st.cards[uid].owner == player
    decks = [
        sorted(c.def_id for c in st.cards if c.owner == p and not V.cdef(st, c.uid).is_token)
        for p in (0, 1)
    ]
    assert decks[0] == sorted(st.decklists[0] + st.resource_decklists[0])
    assert decks[1] == sorted(st.decklists[1] + st.resource_decklists[1])


@pytest.mark.rule("5-2-1", "5-2-2")
def test_owner_in_card_text_is_the_player_who_owns_the_card() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    enemy = sc.add(1, ZAKU_MARINER)
    hawk = sc.add(0, HAWK_OF_ENDYMION, Zone.HAND)
    st = sc.start()
    play(st, hawk)
    assert enemy in st.zones[1][Zone.HAND]
    assert enemy not in st.zones[0][Zone.HAND]


@pytest.mark.rule("5-3-1")
def test_active_player_advances_the_turn() -> None:
    sc = Scenario(active=0)
    sc.resource_deck(1, 1)
    st = sc.start()
    assert st.pending is not None
    assert st.pending.kind is DecisionKind.MAIN and st.pending.player == st.active == 0
    hand_1 = _hand_size(st, 1)
    to_next_turn(st)
    assert st.active == 1
    assert st.pending is not None
    assert st.pending.kind is DecisionKind.MAIN and st.pending.player == 1
    assert _hand_size(st, 1) == hand_1 + 1
    assert len(st.zones[1][Zone.RESOURCE_AREA]) == 1


@pytest.mark.rule("5-3-2")
def test_standby_player_is_the_other_player() -> None:
    sc = Scenario(active=0)
    sc.resources(1, 2)
    attacker = sc.add(0, ZAKU_MARINER)
    sc.add(1, LAUNCHER_STRIKE)
    sc.add(1, ZEON_REMNANT, Zone.HAND)
    st = sc.start()
    assert st.standby == 1
    attack(st, attacker)
    assert st.pending is not None and st.pending.kind is DecisionKind.BLOCK
    assert st.pending.player == st.standby
    block(st, None)
    assert st.pending is not None and st.pending.kind is DecisionKind.ACTION_STEP
    assert st.pending.player == st.standby


# ---------------------------------------------------------------------------------------------
# 5-4 Active and rested


@pytest.mark.rule("5-4-1", "5-4-1-1", "5-4-1-2")
def test_active_and_rested_units() -> None:
    sc = Scenario()
    ready = sc.add(0, ZAKU_MARINER)
    tapped = sc.add(0, ZAKU_MARINER, rested=True)
    enemy_active = sc.add(1, ZAKU_MARINER)
    enemy_rested = sc.add(1, ZAKU_MARINER, rested=True)
    st = sc.start()
    attackers = _option_cards(st, A.ATTACK)
    assert ready in attackers
    assert tapped not in attackers
    targets = {o.b for o in options(st) if o.kind is A.ATTACK and o.a == ready}
    assert targets == {PLAYER_TARGET, enemy_rested}
    assert enemy_active not in targets
    attack(st, ready)
    assert st.cards[ready].rested


@pytest.mark.rule("5-4-1", "5-4-1-1", "5-4-1-2")
def test_active_and_rested_resources() -> None:
    sc = Scenario()
    sc.resources(0, 2, rested=2)
    zaku = sc.add(0, ZAKU_MARINER, Zone.HAND)
    st = sc.start()
    assert not has_action(st, A.PLAY_UNIT, zaku)

    sc = Scenario()
    res = sc.resources(0, 2, rested=1)
    zaku = sc.add(0, ZAKU_MARINER, Zone.HAND)
    st = sc.start()
    play(st, zaku)
    assert all(st.cards[r].rested for r in res)


@pytest.mark.rule("5-4-1", "5-4-1-1", "5-4-1-2")
def test_active_and_rested_base() -> None:
    sc = Scenario()
    side_7 = sc.base(0, SIDE_7)
    unit = sc.add(0, ZAKU_MARINER, damage=1)
    st = sc.start()
    assert not st.cards[side_7].rested
    activate(st, side_7)
    assert st.cards[side_7].rested
    assert st.cards[unit].damage == 0
    assert not has_action(st, A.ACTIVATE, side_7)


@pytest.mark.rule("5-4-2")
def test_resource_placed_in_resource_phase_is_active() -> None:
    sc = Scenario(active=1)
    sc.resource_deck(0, 1)
    st = sc.start()
    to_next_turn(st)
    (resource,) = st.zones[0][Zone.RESOURCE_AREA]
    assert not st.cards[resource].rested


@pytest.mark.rule("5-4-2")
def test_deployed_unit_and_base_are_active() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    sc.shields(0, ZAKU_MARINER)
    zaku, side_7 = sc.hand(0, ZAKU_MARINER, SIDE_7)
    st = sc.start()
    play(st, zaku)
    play(st, side_7)
    assert zone_of(st, zaku) is Zone.BATTLE and not st.cards[zaku].rested
    assert zone_of(st, side_7) is Zone.BASE and not st.cards[side_7].rested


@pytest.mark.rule("5-4-2")
def test_ex_base_and_ex_resource_start_active() -> None:
    st = new_game((_vanilla_deck(), _vanilla_deck()), seed=3, chooser=0)
    apply(st, Action(A.GO_FIRST, 0))
    _keep_both(st)
    for player in (0, 1):
        (base,) = st.zones[player][Zone.BASE]
        assert V.cdef(st, base).card_type is CardType.EX_BASE
        assert not st.cards[base].rested
    ex_resources = [
        u
        for u in st.zones[1][Zone.RESOURCE_AREA]
        if V.cdef(st, u).card_type is CardType.EX_RESOURCE
    ]
    assert len(ex_resources) == 1
    assert not st.cards[ex_resources[0]].rested


# ---------------------------------------------------------------------------------------------
# 5-7 Play, 5-8 Deploy


@pytest.mark.rule("5-7-1")
def test_playing_reveals_the_card_and_pays_its_cost() -> None:
    sc = Scenario()
    res = sc.resources(0, 3)
    sc.add(1, GUNDAM_VANILLA)
    command = sc.add(0, ZEON_REMNANT, Zone.HAND, known=False)
    in_trash = sc.add(0, ZAKU_MARINER, Zone.TRASH)
    st = sc.start()
    assert is_hidden_from(st, command, 1)
    assert not has_action(st, A.PLAY_UNIT, in_trash)
    play(st, command)
    assert st.cards[command].known == core.BOTH_KNOW
    assert sum(st.cards[r].rested for r in res) == _db()[ZEON_REMNANT].cost
    assert zone_of(st, command) is Zone.TRASH


@pytest.mark.rule("5-7-1-1", "5-8-1")
def test_effect_plays_a_unit_card_from_the_trash_paying_its_cost() -> None:
    sc = Scenario()
    res = sc.resources(0, 6)
    target = sc.add(0, GUNDAM_VANILLA, Zone.TRASH)
    power = sc.add(0, AWAKENED_POWER, Zone.HAND)
    st = sc.start()
    play(st, power)
    assert zone_of(st, target) is Zone.BATTLE
    paid = _db()[AWAKENED_POWER].cost + _db()[GUNDAM_VANILLA].cost
    assert sum(st.cards[r].rested for r in res) == paid


@pytest.mark.rule("5-8-1")
def test_deploy_places_units_in_the_battle_area_and_bases_in_the_base_section() -> None:
    sc = Scenario()
    sc.resources(0, 7)
    (shield,) = sc.shields(0, ZAKU_MARINER)
    ex_base = sc.base(0)
    besserung, side_7 = sc.hand(0, KSHATRIYA_BESSERUNG, SIDE_7)
    st = sc.start()
    deck = _deck_size(st, 0)
    play(st, besserung)
    assert zone_of(st, besserung) is Zone.BATTLE
    assert _deck_size(st, 0) == deck - 1
    play(st, side_7)
    assert st.zones[0][Zone.BASE] == [side_7]
    assert zone_of(st, ex_base) is Zone.OUTSIDE
    assert zone_of(st, shield) is Zone.HAND


# ---------------------------------------------------------------------------------------------
# 5-10 Destroy


@pytest.mark.rule("5-10-1", "5-10-2")
def test_effect_destruction_puts_unit_face_up_into_its_owners_trash() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    aries = sc.add(1, NOINS_ARIES)
    sc.add(1, PISCES)
    strike = sc.add(0, FATAL_STRIKE, Zone.HAND)
    st = sc.start()
    deck_1 = _deck_size(st, 1)
    play(st, strike)
    select(st, aries)
    assert aries in st.zones[1][Zone.TRASH]
    assert st.cards[aries].known == core.BOTH_KNOW
    assert _deck_size(st, 1) == deck_1 - 1


@pytest.mark.rule("5-10-1", "5-10-2")
def test_battle_damage_destruction_puts_unit_into_its_owners_trash() -> None:
    sc = Scenario()
    attacker = sc.add(0, GUNDAM_VANILLA)
    aries = sc.add(1, NOINS_ARIES, rested=True)
    sc.add(1, PISCES)
    st = sc.start()
    deck_1 = _deck_size(st, 1)
    attack(st, attacker, aries)
    pass_all(st)
    assert aries in st.zones[1][Zone.TRASH]
    assert _deck_size(st, 1) == deck_1 - 1


@pytest.mark.rule("5-10-1")
@pytest.mark.xfail(
    strict=True,
    reason="ENGINE: interp._h_to_trash moves a field Unit to the trash without destroying it",
)
def test_effect_placing_a_field_unit_into_the_trash_destroys_it() -> None:
    sc = Scenario()
    aries = sc.add(1, NOINS_ARIES)
    sc.add(1, PISCES)
    st = sc.start()
    deck_1 = _deck_size(st, 1)
    _resolve_steps(st, (d.ToTrash(d.Var("x")),), controller=0, variables={"x": (aries,)})
    assert aries in st.zones[1][Zone.TRASH]
    assert _deck_size(st, 1) == deck_1 - 1


@pytest.mark.rule("5-10-1", "5-10-2")
def test_destroyed_base_goes_face_up_into_its_owners_trash() -> None:
    sc = Scenario()
    attacker = sc.add(0, ZAKU_MARINER)
    armory = sc.base(1, ARMORY_ONE, damage=4)
    st = sc.start()
    hands = (_hand_size(st, 0), _hand_size(st, 1))
    attack(st, attacker)
    pass_all(st)
    assert armory in st.zones[1][Zone.TRASH]
    assert st.cards[armory].known == core.BOTH_KNOW
    assert (_hand_size(st, 0), _hand_size(st, 1)) == (hands[0] + 1, hands[1] + 1)


@pytest.mark.rule("5-10-3")
def test_destroyed_shield_is_revealed_and_burst_decided_before_trash() -> None:
    sc = Scenario()
    attacker = sc.add(0, ZAKU_MARINER)
    (shield,) = sc.shields(1, BURST_ADD)
    st = sc.start()
    assert is_hidden_from(st, shield, 0)
    attack(st, attacker)
    pass_all(st)
    assert st.pending is not None and st.pending.kind is DecisionKind.BURST
    assert st.pending.player == 1
    assert zone_of(st, shield) is not Zone.TRASH
    assert st.cards[shield].known == core.BOTH_KNOW
    no(st)
    assert zone_of(st, shield) is Zone.TRASH


@pytest.mark.rule("5-10-3")
def test_destroyed_shield_without_burst_is_revealed_into_trash() -> None:
    sc = Scenario()
    attacker = sc.add(0, ZAKU_MARINER)
    (shield,) = sc.shields(1, ZAKU_MARINER)
    st = sc.start()
    attack(st, attacker)
    pass_all(st)
    assert zone_of(st, shield) is Zone.TRASH
    assert st.cards[shield].known == core.BOTH_KNOW
    assert st.pending is not None and st.pending.kind is DecisionKind.MAIN


@pytest.mark.rule("5-10-4")
def test_battle_area_excess_is_not_destruction() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    taurus = sc.add(0, NOINS_TAURUS)
    for _ in range(5):
        sc.add(0, ZAKU_MARINER)
    new_unit = sc.add(0, ZAKU_MARINER, Zone.HAND)
    st = sc.start()
    deck = _deck_size(st, 0)
    play(st, new_unit)
    assert st.pending is not None and st.pending.kind is DecisionKind.EXCESS
    select(st, taurus)
    assert zone_of(st, taurus) is Zone.TRASH
    assert zone_of(st, new_unit) is Zone.BATTLE
    assert _deck_size(st, 0) == deck
    assert st.pending is not None and st.pending.kind is DecisionKind.MAIN
    assert not any(h.kind == "destroyed" for h in st.history)


@pytest.mark.rule("5-10-4")
def test_base_section_excess_is_not_destruction() -> None:
    sc = Scenario()
    sc.resources(0, 1)
    sc.shields(0, ZAKU_MARINER)
    armory = sc.base(0, ARMORY_ONE)
    side_7 = sc.add(0, SIDE_7, Zone.HAND)
    st = sc.start()
    decks = (_deck_size(st, 0), _deck_size(st, 1))
    play(st, side_7)
    assert zone_of(st, armory) is Zone.TRASH
    assert st.zones[0][Zone.BASE] == [side_7]
    assert (_deck_size(st, 0), _deck_size(st, 1)) == decks


# ---------------------------------------------------------------------------------------------
# 5-11 Discard, 5-12 Remove


@pytest.mark.rule("5-11-1")
def test_discard_places_a_card_from_the_hand_into_the_trash() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    strike, kept = sc.hand(0, STRIKE_DRAW_DISCARD, ZAKU_MARINER)
    st = sc.start()
    top = st.zones[0][Zone.DECK][0]
    play(st, strike)
    assert st.pending is not None and st.pending.kind is DecisionKind.DISCARD
    assert _option_cards(st, A.SELECT) == {kept, top}
    select(st, kept)
    assert kept in st.zones[0][Zone.TRASH]
    assert top in st.zones[0][Zone.HAND]


@pytest.mark.rule("5-12-1")
def test_exiled_cards_are_placed_into_the_removal_area() -> None:
    sc = Scenario()
    red_frame = sc.add(0, RED_FRAME_EX)
    commands = sc.trash(0, ZEON_REMNANT, ZEON_REMNANT)
    enemy = sc.add(1, ZAKU_MARINER, damage=1)
    st = sc.start()
    activate(st, red_frame)
    assert [zone_of(st, c) for c in commands] == [Zone.REMOVAL, Zone.REMOVAL]
    assert sorted(st.zones[0][Zone.REMOVAL]) == sorted(commands)
    assert st.cards[enemy].rested


@pytest.mark.rule("5-12-2")
def test_removed_unit_is_not_destroyed() -> None:
    for step, destroyed in ((d.Destroy, True), (d.Exile, False)):
        sc = Scenario()
        aries = sc.add(1, NOINS_ARIES)
        sc.add(1, PISCES)
        st = sc.start()
        deck_1 = _deck_size(st, 1)
        _resolve_steps(st, (step(d.Var("x")),), controller=0, variables={"x": (aries,)})
        assert zone_of(st, aries) is (Zone.TRASH if destroyed else Zone.REMOVAL)
        assert _deck_size(st, 1) == deck_1 - int(destroyed)
        assert any(h.kind == "destroyed" for h in st.history) is destroyed


# ---------------------------------------------------------------------------------------------
# 5-13 Randomly, 5-14 Draw, 5-15 Shuffle


@pytest.mark.rule("5-13-1")
def test_random_return_to_deck_bottom_is_not_chosen_by_the_player() -> None:
    orders = set()
    for seed in range(8):
        sc = Scenario(seed=seed)
        sc.resources(0, 5)
        sc.deck(0, ZAKU_MARINER, GOOHN, DREISSEN, LOTO, CORE_BOOSTER)
        encounter = sc.add(0, ENCOUNTER, Zone.HAND)
        st = sc.start()
        deck_before = list(st.zones[0][Zone.DECK])
        looked = deck_before[:5]
        play(st, encounter)
        seen_kinds = set()
        while st.pending is not None and st.pending.kind is not DecisionKind.MAIN:
            seen_kinds.add(st.pending.kind)
            no(st)
        assert seen_kinds <= {DecisionKind.YES_NO}
        deck_after = st.zones[0][Zone.DECK]
        assert deck_after[:-5] == deck_before[5:]
        assert sorted(deck_after[-5:]) == sorted(looked)
        orders.add(tuple(deck_after[-5:]))
    assert len(orders) > 1


@pytest.mark.rule("5-14-1")
def test_draw_phase_takes_the_top_card_without_showing_it() -> None:
    sc = Scenario(active=1)
    sc.deck(0, GOOHN)
    st = sc.start()
    top = st.zones[0][Zone.DECK][0]
    to_next_turn(st)
    assert st.active == 0
    assert top in st.zones[0][Zone.HAND]
    assert not is_hidden_from(st, top, 0)
    assert is_hidden_from(st, top, 1)


@pytest.mark.rule("5-14-1-1")
def test_draw_1_moves_the_top_card_to_the_hand_without_showing_it() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    sc.deck(0, GOOHN)
    curiosity = sc.add(0, HEALTHY_CURIOSITY, Zone.HAND)
    st = sc.start()
    top, second = st.zones[0][Zone.DECK][:2]
    hand = _hand_size(st, 0)
    play(st, curiosity)
    assert top in st.zones[0][Zone.HAND]
    assert st.zones[0][Zone.DECK][0] == second
    assert _hand_size(st, 0) == hand
    assert is_hidden_from(st, top, 1)


@pytest.mark.rule("5-15-1")
def test_shuffling_reorders_the_deck_randomly() -> None:
    st = new_game((_vanilla_deck(), _vanilla_deck()), seed=11, chooser=0)
    apply(st, Action(A.GO_FIRST, 0))
    assert st.pending is not None and st.pending.kind is DecisionKind.REDRAW
    deck = list(st.zones[0][Zone.DECK])
    hand = list(st.zones[0][Zone.HAND])
    unshuffled = deck[5:] + hand
    apply(st, Action(A.REDRAW))
    new_deck = st.zones[0][Zone.DECK]
    assert sorted(new_deck) == sorted(unshuffled)
    assert new_deck != unshuffled
    assert st.zones[0][Zone.HAND] == deck[:5]
    assert all(is_hidden_from(st, u, 0) for u in new_deck)


@pytest.mark.rule("5-15-1-1")
def test_shuffling_a_one_card_deck_counts_as_shuffled() -> None:
    sc = Scenario(deck_size=0)
    sc.deck(0, GOOHN)
    sc.deck(1, GOOHN)
    st = sc.start()
    (only,) = st.zones[0][Zone.DECK]
    _resolve_steps(st, (d.Shuffle(), d.IfYouDo((d.PlaceExResource(),))), controller=0)
    assert st.zones[0][Zone.DECK] == [only]
    placed = [
        u
        for u in st.zones[0][Zone.RESOURCE_AREA]
        if V.cdef(st, u).card_type is CardType.EX_RESOURCE
    ]
    assert len(placed) == 1
    assert st.winner is None


# ---------------------------------------------------------------------------------------------
# 5-19 Forward slash


@pytest.mark.rule("5-19-1")
def test_slash_between_traits_means_or() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    host = sc.add(0, GOOHN)
    zeon, neo_zeon, zaft = sc.hand(0, ZAKU_MARINER, DRA_C, GOOHN)
    frontal = sc.add(0, FULL_FRONTAL, Zone.HAND)
    st = sc.start()
    play(st, frontal, onto=host)
    assert st.pending is not None and st.pending.kind is DecisionKind.YES_NO
    yes(st)
    assert _option_cards(st, A.SELECT) == {zeon, neo_zeon}
    select(st, neo_zeon)
    assert zone_of(st, neo_zeon) is Zone.BATTLE
    assert zone_of(st, zeon) is Zone.HAND
    assert zone_of(st, zaft) is Zone.HAND
