"""Rules 4-1 to 4-9: game locations, their limits, and what each player may see.

Information rules are checked against the engine's information sets
(:mod:`gcg_sim.engine.observe`): a card a player may not view must be hidden from that player,
and every determinization for an observer must agree with what that observer may know.
"""

from __future__ import annotations

from collections import Counter
from typing import Any

import pytest

from gcg_sim.cards.model import CardType
from gcg_sim.effects import dsl as d
from gcg_sim.effects.compiler import compile_parts
from gcg_sim.engine import core
from gcg_sim.engine import interp as I
from gcg_sim.engine import observe as O
from gcg_sim.engine import view as V
from gcg_sim.engine.game import DeckList, apply, new_game
from gcg_sim.engine.state import GameState
from gcg_sim.engine.types import (
    FIELD_ZONES,
    NO_ARG,
    PLAYER_TARGET,
    PUBLIC_ZONES,
    ActionKind,
    DecisionKind,
    EndReason,
    Zone,
)
from gcg_sim.rng import SplitMix64
from gcg_sim.testkit import (
    Scenario,
    act,
    ap,
    attack,
    card_numbers,
    end_main,
    has_action,
    hp,
    no,
    options,
    order,
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

VANILLA = "GD01-060"  # Zaku Mariner, Lv2 cost1 2/2, no text
AP1_UNIT = "GD01-062"  # GOOhN, Lv1 cost1 1/2, no text
AP3_UNIT = "GD01-064"  # Lv2 cost2 3/2, no text
AP0_UNIT = "GD01-061"  # ZuOOT, Lv1 cost1 0/2, only <Support 1>
DESTROYED_PING = "GD01-056"  # Destroyed: deal 1 damage to an enemy Unit
MARINE = "ST11-008"  # Daughseat, (Marine) Lv2 cost1 2/2, no text
PILOT = "GD01-089"  # Riddhe Marcenas, Pilot Lv3 cost1 1/1
MARIDA = "GD01-093"  # Marida Cruz, Pilot Lv4 cost1 2/1 (link Pilot of GD01-003)
BANSHEE = "GD01-003"  # During Link, Attack: return 12 trash cards to the deck and shuffle
AP_PLUS_2 = "ST03-012"  # Main/Action: 1 friendly Unit gets AP+2 during this turn
PING = "GD01-115"  # Main/Action: deal 1 damage to an enemy Unit
DRAW_2 = "GD01-100"  # Main: draw 2
DRAW_2_DISCARD_1 = "GD01-118"  # Main: draw 2, then discard 1
PLACE_RESOURCE = "GD01-107"  # Main: place 1 rested Resource
PLACE_EX_RESOURCE = "EB01-038"  # G-Self, Unit Lv4 cost3; Deploy: place 1 EX Resource
REVIVE_MARINE = "ST11-015"  # Main: deploy a (Marine) Unit card from your trash, rested
LOOK_3_RANDOM_BOTTOM = "ST06-012"  # Main: look at the top 3, rest randomly to the bottom
LOOK_TOP_OR_BOTTOM = "GD01-039"  # Dopp; Deploy: look at the top card, return it top or bottom
EXILE_TITANS = "GD03-009"  # Deploy: you may exile 2 (Titans) cards from your trash
TITANS_A = "GD02-013"  # (Titans) Unit, no text
TITANS_B = "GD02-015"  # (Titans) Unit, no text
BASE_A = "GD01-128"  # Base 0/6; Deploy: add 1 of your Shields to your hand
BASE_B = "GD01-126"  # Base 0/6; Deploy: add 1 of your Shields to your hand
BURST_ADD = "GD01-097"  # Burst: add this card to your hand
BURST_DAMAGE = "GD01-111"  # Burst: deal 2 damage to an enemy Unit
RETURN_ENEMY = "GD05-105"  # Main/Action: return an enemy Unit Lv.3 or lower to its owner's hand
RETURN_TO_DECK_BOTTOM = "ST11-001"  # Deploy: return an enemy Unit to the bottom of its owner's deck

LOCATIONS = (
    Zone.DECK,
    Zone.RESOURCE_DECK,
    Zone.RESOURCE_AREA,
    Zone.BATTLE,
    Zone.SHIELD,
    Zone.BASE,
    Zone.REMOVAL,
    Zone.HAND,
    Zone.TRASH,
)
"""Rule 4-1-1 (the shield area is split into its shield and base sections, rule 4-6-2)."""

IN_A_LOCATION = (*LOCATIONS, Zone.PAIRED)
"""Paired Pilots lie beneath their Unit in the battle area (rule 4-5-3)."""

PUBLIC_LOCATIONS = {Zone.RESOURCE_AREA, Zone.BATTLE, Zone.BASE, Zone.REMOVAL, Zone.TRASH}
PRIVATE_LOCATIONS = {Zone.DECK, Zone.RESOURCE_DECK, Zone.SHIELD, Zone.HAND}

RED_DECK = DeckList(
    main=(
        *["GD01-060"] * 4,
        *["GD01-062"] * 4,
        *["GD01-064"] * 4,
        *["GD01-057"] * 4,
        *["GD01-051"] * 4,
        *["ST03-003"] * 4,
        *["ST03-005"] * 4,
        *["GD01-095"] * 4,
        *["GD01-093"] * 4,
        *["ST03-012"] * 4,
        *["GD01-115"] * 4,
        *["ST03-013"] * 2,
        *["GD01-128"] * 4,
    ),
    resources=("R-001",) * 10,
)


# ---------------------------------------------------------------------------------------------
# helpers


def _hidden(st: GameState, uid: int, observer: int) -> bool:
    return O.is_hidden_from(st, uid, observer)


def _in_no_location(st: GameState, uid: int) -> bool:
    return all(uid not in st.zones[p][z] for p in (0, 1) for z in IN_A_LOCATION)


def _zone_sizes(st: GameState) -> list[list[int]]:
    return [[len(z) for z in pz] for pz in st.zones]


def _swap_identities(st: GameState, a: int, b: int) -> GameState:
    """The same state with the identities of cards ``a`` and ``b`` exchanged."""
    s = st.clone()
    s.cards[a].def_id, s.cards[b].def_id = s.cards[b].def_id, s.cards[a].def_id
    s.touch()
    return s


def _ex_count(st: GameState, player: int) -> int:
    return sum(
        1
        for u in st.zones[player][Zone.RESOURCE_AREA]
        if V.cdef(st, u).card_type is CardType.EX_RESOURCE
    )


def _keep_both(st: GameState, first: int = 0) -> None:
    """Setup: ``first`` becomes Player One, then both players keep their hands."""
    act(st, A.GO_FIRST, first)
    act(st, A.KEEP)
    act(st, A.KEEP)


def _return_to_deck_bottom_program() -> int:
    """The compiled 【Deploy】 of ST11-001 ("If another friendly (Marine) Unit is in play, choose 1
    enemy Unit that is Lv.2 or lower. Return it to the bottom of its owner's deck."). The card's
    other ability does not compile yet, so the 【Deploy】 is run on its own."""
    reg = V.reg()
    parts = compile_parts(reg.db[RETURN_TO_DECK_BOTTOM])
    (abilities,) = [res for text, res in parts if text.startswith("【Deploy】")]
    assert isinstance(abilities, list)
    (trigger,) = abilities
    assert isinstance(trigger, d.Triggered)
    return reg.program(trigger.steps, f"{RETURN_TO_DECK_BOTTOM}#deploy")


def _paired_enemy_returned_to_deck_bottom() -> tuple[GameState, int, int]:
    """Player 0 resolves ST11-001's 【Deploy】 against player 1's Unit with a paired Pilot: both
    public cards go to the bottom of player 1's deck at the same time (rule 3-3-6)."""
    sc = Scenario()
    host = sc.add(0, MARINE)
    sc.add(0, MARINE)
    unit = sc.add(1, VANILLA, pilot=PILOT)
    pilot = sc.st.cards[unit].pair
    I.push_frame(sc.st, _return_to_deck_bottom_program(), controller=0, host=host, kind="trigger")
    return sc.start(), unit, pilot


# ---------------------------------------------------------------------------------------------
# 4-1 Locations


def _check_locations(st: GameState) -> None:
    placed: dict[int, tuple[int, Zone]] = {}
    for p in (0, 1):
        for z in Zone:
            for uid in st.zones[p][z]:
                assert uid not in placed, f"card {uid} is in two locations"
                placed[uid] = (p, z)
    for c in st.cards:
        if c.zone is Zone.OUTSIDE:  # a token that left the game (rule 5-17-2-5)
            assert c.uid not in placed
            continue
        assert c.zone in (*IN_A_LOCATION, Zone.RESOLVING), c.zone
        assert placed[c.uid] == (c.owner, c.zone), "a card sits in its owner's location"


@pytest.mark.rule("4-1-1", "4-1-1-1")
@pytest.mark.parametrize("seed", [1, 2, 3])
def test_every_card_is_in_exactly_one_location_of_its_owner(seed: int) -> None:
    st = new_game((RED_DECK, RED_DECK), seed)
    for z in LOCATIONS:
        assert st.zones[0][z] is not st.zones[1][z]
    rng = SplitMix64(seed)
    for _ in range(800):
        _check_locations(st)
        if st.pending is None:
            break
        opts = st.pending.options
        apply(st, opts[rng.randrange(len(opts))])
    _check_locations(st)
    assert st.turn > 3  # the walk went through several full turns


@pytest.mark.rule("4-1-1-1")
def test_effects_put_cards_into_their_owners_locations() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    attacker = sc.add(0, AP3_UNIT)
    victim = sc.add(1, VANILLA, rested=True, pilot=PILOT)
    victim_pilot = sc.st.cards[victim].pair
    bounced = sc.add(1, AP1_UNIT)
    bounce = sc.add(0, RETURN_ENEMY, Zone.HAND)
    st = sc.start()
    play(st, bounce)
    select(st, bounced)
    assert bounced in st.zones[1][Zone.HAND]
    assert bounced not in st.zones[0][Zone.HAND]
    attack(st, attacker, victim)
    pass_all(st)
    assert {victim, victim_pilot} <= set(st.zones[1][Zone.TRASH])
    assert attacker in st.zones[0][Zone.TRASH]
    assert not {victim, victim_pilot} & set(st.zones[0][Zone.TRASH])


@pytest.mark.rule("4-1-1-2")
def test_field_is_the_resource_battle_and_shield_areas() -> None:
    field = {z for z in Zone if z.is_field}
    assert field == {Zone.RESOURCE_AREA, Zone.BATTLE, Zone.PAIRED, Zone.SHIELD, Zone.BASE}
    assert field == FIELD_ZONES
    assert not field & {
        Zone.DECK,
        Zone.RESOURCE_DECK,
        Zone.REMOVAL,
        Zone.HAND,
        Zone.TRASH,
    }
    # A token exists only on the field: the EX Base stays a card while in the base section and
    # leaves the game once it is destroyed and leaves the field.
    sc = Scenario()
    attacker = sc.add(0, AP3_UNIT)
    ex_base = sc.base(1, damage=2)
    st = sc.start()
    assert zone_of(st, ex_base) is Zone.BASE
    attack(st, attacker)
    pass_all(st)
    assert zone_of(st, ex_base) is Zone.OUTSIDE
    assert ex_base not in st.zones[1][Zone.TRASH]


@pytest.mark.rule("4-1-2")
def test_command_whose_effect_is_active_is_in_no_location() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    first = sc.add(0, VANILLA)
    sc.add(0, VANILLA)
    cmd = sc.add(0, AP_PLUS_2, Zone.HAND)
    st = sc.start()
    hand_before = len(st.zones[0][Zone.HAND])
    play(st, cmd)
    assert st.pending is not None and st.pending.kind is DecisionKind.SELECT
    assert zone_of(st, cmd) is Zone.RESOLVING
    assert _in_no_location(st, cmd)
    assert len(st.zones[0][Zone.HAND]) == hand_before - 1
    assert not st.zones[0][Zone.TRASH]
    select(st, first)
    assert ap(st, first) == 4
    assert zone_of(st, cmd) is Zone.TRASH


@pytest.mark.rule("4-1-2")
def test_card_whose_burst_is_active_is_in_no_location() -> None:
    sc = Scenario()
    attacker = sc.add(0, VANILLA)
    other = sc.add(0, VANILLA)
    (shield,) = sc.shields(1, BURST_DAMAGE)
    st = sc.start()
    attack(st, attacker)
    pass_all(st)
    assert st.pending is not None and st.pending.kind is DecisionKind.BURST
    yes(st)
    assert st.pending is not None and st.pending.kind is DecisionKind.SELECT
    assert st.pending.player == 1
    assert zone_of(st, shield) is Zone.RESOLVING
    assert _in_no_location(st, shield)
    assert not st.zones[1][Zone.SHIELD] and not st.zones[1][Zone.TRASH]
    select(st, other)
    assert zone_of(st, other) is Zone.TRASH
    assert zone_of(st, shield) is Zone.TRASH


@pytest.mark.rule("4-1-3")
def test_number_of_cards_in_every_location_is_known_to_both_players() -> None:
    st = new_game((RED_DECK, RED_DECK), seed=21)
    _keep_both(st)
    sizes = _zone_sizes(st)
    assert sizes[0][Zone.HAND] == 6 and sizes[1][Zone.HAND] == 5
    assert sizes[0][Zone.SHIELD] == sizes[1][Zone.SHIELD] == 6
    for observer in (0, 1):
        for seed in range(5):
            assert _zone_sizes(O.determinize(st, observer, seed)) == sizes


@pytest.mark.rule("4-1-4")
def test_public_locations_are_visible_and_private_ones_are_not() -> None:
    assert PUBLIC_LOCATIONS <= PUBLIC_ZONES
    assert not PRIVATE_LOCATIONS & PUBLIC_ZONES
    sc = Scenario()
    sc.resource_deck(1, 2)
    sc.resources(1, 2)
    sc.add(1, VANILLA, pilot=PILOT)
    sc.shields(1, VANILLA)
    sc.base(1, BASE_A)
    sc.add(1, AP3_UNIT, Zone.REMOVAL)
    sc.add(1, AP3_UNIT, Zone.HAND, known=False)
    sc.trash(1, AP1_UNIT)
    st = sc.start()
    checked = set()
    for z in (*LOCATIONS, Zone.PAIRED):
        for uid in st.zones[1][z]:
            checked.add(z)
            assert _hidden(st, uid, 0) is (z in PRIVATE_LOCATIONS), (z, uid)
    assert checked == {*LOCATIONS, Zone.PAIRED}


@pytest.mark.rule("4-1-5")
def test_card_that_changes_location_is_a_new_card() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    marine = sc.add(0, MARINE)
    enemy = sc.add(1, VANILLA, rested=True)
    buff = sc.add(0, AP_PLUS_2, Zone.HAND)
    revive = sc.add(0, REVIVE_MARINE, Zone.HAND)
    st = sc.start()
    play(st, buff)
    assert ap(st, marine) == 4
    attack(st, marine, enemy)
    pass_all(st)
    assert zone_of(st, marine) is Zone.TRASH
    play(st, revive)
    assert zone_of(st, marine) is Zone.BATTLE
    assert ap(st, marine) == 2  # the AP+2 "during this turn" stayed with the old card
    assert st.cards[marine].damage == 0


@pytest.mark.rule("4-1-6")
def test_owner_orders_cards_placed_into_a_location_together() -> None:
    """Ruling ST11-001 Q427: the owner of the returned Unit and Pilot orders them."""
    st, unit, pilot = _paired_enemy_returned_to_deck_bottom()
    dec = st.pending
    assert dec is not None and dec.player == 1, "the owner (player 1) decides the order"
    outcomes = set()
    for opt in dec.options:
        s = st.clone()
        apply(s, opt)
        while s.pending is not None and s.pending.player == 1:
            apply(s, s.pending.options[0])
        outcomes.add(tuple(s.zones[1][Zone.DECK][-2:]))
    assert outcomes == {(unit, pilot), (pilot, unit)}


@pytest.mark.rule("4-1-6")
def test_randomly_placed_cards_are_not_ordered_by_anyone() -> None:
    """ "Return the remaining cards randomly" specifies the order, so nobody chooses it."""
    sc = Scenario()
    sc.resources(0, 1)
    cmd = sc.add(0, LOOK_3_RANDOM_BOTTOM, Zone.HAND)
    sc.deck(0, VANILLA, AP1_UNIT, AP3_UNIT)
    st = sc.start()
    looked = st.zones[0][Zone.DECK][:3]
    play(st, cmd)
    no(st)  # no (Clan) card to reveal
    assert st.pending is not None and st.pending.kind is DecisionKind.MAIN
    assert set(st.zones[0][Zone.DECK][-3:]) == set(looked)
    assert zone_of(st, cmd) is Zone.TRASH


@pytest.mark.rule("4-1-7")
def test_order_of_public_cards_placed_into_a_private_location_is_hidden() -> None:
    st, unit, pilot = _paired_enemy_returned_to_deck_bottom()
    while st.pending is not None and st.pending.player == 1:
        apply(st, st.pending.options[0])
    bottom = st.zones[1][Zone.DECK][-2:]
    assert set(bottom) == {unit, pilot}
    swapped = _swap_identities(st, unit, pilot)
    assert O.information_set_key(st, 0) == O.information_set_key(swapped, 0)


@pytest.mark.rule("4-1-7", "4-2-4")
def test_trash_cards_shuffled_into_the_deck_lose_their_position() -> None:
    sc = Scenario()
    banshee = sc.add(0, BANSHEE, pilot=MARIDA)
    returned = sc.trash(0, *[VANILLA, AP1_UNIT, AP3_UNIT, AP0_UNIT, MARINE, TITANS_A] * 2)
    sc.shields(1, VANILLA)
    st = sc.start()
    deck_before = Counter(card_numbers(st, st.zones[0][Zone.DECK]))
    attack(st, banshee)
    order(st)  # Banshee's and Marida Cruz's 【Attack】 effects trigger together
    pass_all(st)
    assert set(returned).isdisjoint(st.zones[0][Zone.TRASH])
    deck = st.zones[0][Zone.DECK]
    assert set(returned) <= set(deck)
    assert Counter(card_numbers(st, deck)) == deck_before + Counter(card_numbers(st, returned))
    for uid in deck:
        assert _hidden(st, uid, 0) and _hidden(st, uid, 1)


# ---------------------------------------------------------------------------------------------
# 4-2 Deck area, 4-3 Resource deck area


@pytest.mark.rule("4-2-1", "4-2-2", "4-3-1", "4-3-2")
def test_decks_start_face_down_in_their_areas() -> None:
    st = new_game((RED_DECK, RED_DECK), seed=4)
    assert st.pending is not None and st.pending.kind is DecisionKind.CHOOSE_FIRST
    for p in (0, 1):
        deck = st.zones[p][Zone.DECK]
        rdeck = st.zones[p][Zone.RESOURCE_DECK]
        assert sorted(card_numbers(st, deck)) == sorted(RED_DECK.main)
        assert card_numbers(st, rdeck) == list(RED_DECK.resources)
        for z in LOCATIONS:
            if z not in (Zone.DECK, Zone.RESOURCE_DECK):
                assert not st.zones[p][z]
        for uid in deck:
            assert _hidden(st, uid, 0) and _hidden(st, uid, 1)
        for uid in rdeck:
            assert _hidden(st, uid, 1 - p)


@pytest.mark.rule("4-2-2")
def test_deck_stays_hidden_from_both_players_during_play() -> None:
    st = new_game((RED_DECK, RED_DECK), seed=8)
    _keep_both(st)
    assert st.pending is not None and st.pending.kind is DecisionKind.MAIN
    for p in (0, 1):
        for uid in st.zones[p][Zone.DECK]:
            assert _hidden(st, uid, 0) and _hidden(st, uid, 1)


@pytest.mark.rule("4-2-2", "4-3-2", "4-6-4-1")
@pytest.mark.parametrize("seed", [2, 5])
def test_no_action_reorders_a_deck_or_the_shields(seed: int) -> None:
    """No card in these decks puts cards into a deck or the shield section, so a deck, a resource
    deck and a shield section may only lose cards from the top: nothing reorders them."""
    st = new_game((RED_DECK, RED_DECK), seed)
    _keep_both(st)
    start = {
        (p, z): list(st.zones[p][z])
        for p in (0, 1)
        for z in (Zone.DECK, Zone.RESOURCE_DECK, Zone.SHIELD)
    }
    rng = SplitMix64(seed)
    for _ in range(800):
        for (p, z), initial in start.items():
            now = st.zones[p][z]
            assert now == initial[len(initial) - len(now) :], (p, z)
        if st.pending is None:
            break
        opts = st.pending.options
        apply(st, opts[rng.randrange(len(opts))])
    assert st.turn > 3
    assert any(len(st.zones[p][Zone.SHIELD]) < 6 for p in (0, 1))


@pytest.mark.rule("4-2-2")
def test_looking_at_the_top_card_shows_it_only_to_the_looker() -> None:
    sc = Scenario()
    sc.resources(0, 1)
    dopp = sc.add(0, LOOK_TOP_OR_BOTTOM, Zone.HAND)
    sc.deck(0, AP3_UNIT)
    st = sc.start()
    top = st.zones[0][Zone.DECK][0]
    assert _hidden(st, top, 0)
    play(st, dopp)
    assert st.pending is not None and st.pending.kind is DecisionKind.ARRANGE
    assert st.pending.player == 0
    act(st, A.SELECT, 0)  # keep it on top
    assert st.zones[0][Zone.DECK][0] == top
    assert not _hidden(st, top, 0)
    assert _hidden(st, top, 1)
    assert all(_hidden(st, u, 0) for u in st.zones[0][Zone.DECK][1:])


@pytest.mark.rule("4-2-2")
def test_cards_returned_randomly_to_the_deck_do_not_reveal_their_order() -> None:
    sc = Scenario()
    sc.resources(0, 1)
    cmd = sc.add(0, LOOK_3_RANDOM_BOTTOM, Zone.HAND)
    sc.deck(0, VANILLA, AP1_UNIT, AP3_UNIT)
    st = sc.start()
    play(st, cmd)
    no(st)
    a, b, c = st.zones[0][Zone.DECK][-3:]
    for x, y in ((a, b), (b, c), (a, c)):
        assert O.information_set_key(st, 0) == O.information_set_key(_swap_identities(st, x, y), 0)


@pytest.mark.rule("4-2-3")
def test_cards_leaving_the_deck_together_move_one_at_a_time_but_simultaneously(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    sc = Scenario()
    sc.resources(0, 4)
    cmd = sc.add(0, DRAW_2, Zone.HAND)
    st = sc.start()
    top_two = st.zones[0][Zone.DECK][:2]
    drawn: list[tuple[int, object]] = []
    real_emit = core.emit

    def spy(state: GameState, ev: d.Ev, subject: int = NO_ARG, **kwargs: Any) -> None:
        if ev is d.Ev.DRAWN:
            drawn.append((subject, kwargs["group"]))
        real_emit(state, ev, subject, **kwargs)

    monkeypatch.setattr(core, "emit", spy)
    play(st, cmd)
    assert [u for u, _ in drawn] == top_two  # top card first
    assert len({g for _, g in drawn}) == 1  # one simultaneous event
    assert set(top_two) <= set(st.zones[0][Zone.HAND])


@pytest.mark.rule("4-2-3", "4-6-2", "4-6-4")
@pytest.mark.faq("Q8")
def test_shields_are_taken_from_the_deck_one_at_a_time_and_placed_face_down() -> None:
    st = new_game((RED_DECK, RED_DECK), seed=5, chooser=0)
    act(st, A.GO_FIRST, 0)
    act(st, A.KEEP)
    tops = {p: list(st.zones[p][Zone.DECK][:6]) for p in (0, 1)}
    act(st, A.KEEP)
    for p in (0, 1):
        # FAQ: the top card of the deck becomes the bottom Shield
        assert st.zones[p][Zone.SHIELD] == tops[p][::-1]
        for uid in st.zones[p][Zone.SHIELD]:
            assert _hidden(st, uid, 0) and _hidden(st, uid, 1)
        (base,) = st.zones[p][Zone.BASE]
        assert V.cdef(st, base).card_type is CardType.EX_BASE


@pytest.mark.rule("4-2-4")
def test_shuffling_reorders_the_deck_randomly() -> None:
    st = new_game((RED_DECK, RED_DECK), seed=9, chooser=0)
    act(st, A.GO_FIRST, 0)
    hand = list(st.zones[0][Zone.HAND])
    deck = list(st.zones[0][Zone.DECK])
    for uid in hand:
        assert not _hidden(st, uid, 0)
    act(st, A.REDRAW)
    # the hand went to the bottom, five were drawn from the top, then the deck was shuffled
    assert st.zones[0][Zone.HAND] == deck[:5]
    unshuffled = deck[5:] + hand
    shuffled = st.zones[0][Zone.DECK]
    assert sorted(shuffled) == sorted(unshuffled)
    assert shuffled != unshuffled
    for uid in shuffled:
        assert _hidden(st, uid, 0) and _hidden(st, uid, 1)


# ---------------------------------------------------------------------------------------------
# 4-4 Resource area


@pytest.mark.rule("4-4-1", "4-4-3", "4-3-2")
def test_resource_step_places_the_top_resource_card_face_up() -> None:
    sc = Scenario(active=1)
    sc.resource_deck(0, 3)
    st = sc.start()
    top = st.zones[0][Zone.RESOURCE_DECK][0]
    assert _hidden(st, top, 1)
    to_next_turn(st)
    assert st.active == 0
    assert zone_of(st, top) is Zone.RESOURCE_AREA
    assert not st.cards[top].rested
    assert len(st.zones[0][Zone.RESOURCE_DECK]) == 2
    assert not _hidden(st, top, 1)


@pytest.mark.rule("4-4-2")
@pytest.mark.faq("Q19")
@pytest.mark.parametrize(
    ("normal", "ex", "placed"), [(14, 0, True), (15, 0, False), (10, 5, False)]
)
def test_resource_step_respects_the_limit_of_fifteen(normal: int, ex: int, placed: bool) -> None:
    sc = Scenario(active=1)
    sc.resources(0, normal, ex=ex)
    sc.resource_deck(0, 2)
    st = sc.start()
    to_next_turn(st)
    assert st.active == 0
    assert len(st.zones[0][Zone.RESOURCE_AREA]) == min(15, normal + ex + 1)
    assert len(st.zones[0][Zone.RESOURCE_DECK]) == (1 if placed else 2)


@pytest.mark.rule("4-4-2")
def test_effect_cannot_place_a_sixteenth_resource() -> None:
    sc = Scenario()
    sc.resources(0, 15)
    sc.resource_deck(0, 2)
    cmd = sc.add(0, PLACE_RESOURCE, Zone.HAND)
    st = sc.start()
    play(st, cmd)
    assert len(st.zones[0][Zone.RESOURCE_AREA]) == 15
    assert len(st.zones[0][Zone.RESOURCE_DECK]) == 2
    assert zone_of(st, cmd) is Zone.TRASH


@pytest.mark.rule("4-4-2-1")
@pytest.mark.parametrize(("ex_before", "ex_after"), [(4, 5), (5, 5)])
def test_resource_area_holds_at_most_five_ex_resources(ex_before: int, ex_after: int) -> None:
    sc = Scenario()
    sc.resources(0, 3, ex=ex_before)
    unit = sc.add(0, PLACE_EX_RESOURCE, Zone.HAND)
    st = sc.start()
    play(st, unit)  # paid with the three normal Resources
    assert zone_of(st, unit) is Zone.BATTLE
    assert _ex_count(st, 0) == ex_after


# ---------------------------------------------------------------------------------------------
# 4-5 Battle area


@pytest.mark.rule("4-5-1", "4-5-2", "4-5-3")
def test_units_and_pilots_are_placed_face_up_in_the_battle_area() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    unit = sc.add(0, VANILLA, Zone.HAND, known=False)
    pilot = sc.add(0, PILOT, Zone.HAND, known=False)
    st = sc.start()
    assert _hidden(st, unit, 1) and _hidden(st, pilot, 1)
    play(st, unit)
    assert zone_of(st, unit) is Zone.BATTLE
    assert not _hidden(st, unit, 1)
    play(st, pilot, onto=unit)
    assert zone_of(st, pilot) is Zone.PAIRED  # beneath the Unit, in the battle area
    assert st.cards[unit].pair == pilot and st.cards[pilot].pair == unit
    assert not _hidden(st, pilot, 1)
    assert (ap(st, unit), hp(st, unit)) == (3, 3)


@pytest.mark.rule("4-5-1")
def test_bases_and_commands_do_not_enter_the_battle_area() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    enemy = sc.add(1, VANILLA)
    base = sc.add(0, BASE_A, Zone.HAND)
    cmd = sc.add(0, PING, Zone.HAND)
    st = sc.start()
    play(st, base)
    play(st, cmd)
    assert zone_of(st, base) is Zone.BASE
    assert zone_of(st, cmd) is Zone.TRASH
    assert st.cards[enemy].damage == 1
    assert not st.zones[0][Zone.BATTLE]


@pytest.mark.rule("4-5-4")
@pytest.mark.faq("Q26", "Q27")
def test_battle_area_holds_at_most_six_units() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    replaced = sc.add(0, DESTROYED_PING)
    others = [sc.add(0, VANILLA) for _ in range(5)]
    enemy = sc.add(1, VANILLA)
    new = sc.add(0, AP1_UNIT, Zone.HAND)
    st = sc.start()
    assert has_action(st, A.PLAY_UNIT, new)  # FAQ: a full battle area does not stop deploying
    play(st, new)
    assert st.pending is not None and st.pending.kind is DecisionKind.EXCESS
    assert st.pending.player == 0
    assert {o.a for o in options(st)} == {replaced, *others}
    select(st, replaced)
    assert zone_of(st, replaced) is Zone.TRASH
    assert set(st.zones[0][Zone.BATTLE]) == {new, *others}
    assert st.cards[enemy].damage == 0  # placed into the trash, not destroyed: no 【Destroyed】


@pytest.mark.rule("4-5-4")
def test_paired_pilots_do_not_count_toward_the_six_units() -> None:
    sc = Scenario()
    sc.resources(0, 1)
    for _ in range(5):
        sc.add(0, VANILLA, pilot=PILOT)
    new = sc.add(0, AP1_UNIT, Zone.HAND)
    st = sc.start()
    play(st, new)
    assert st.pending is not None and st.pending.kind is DecisionKind.MAIN
    assert len(st.zones[0][Zone.BATTLE]) == 6
    assert len(st.zones[0][Zone.PAIRED]) == 5


@pytest.mark.rule("4-5-5", "4-4-3", "4-6-3-1", "4-7-2", "4-9-2")
def test_cards_in_public_locations_are_known_to_the_opponent() -> None:
    sc = Scenario()
    battle = sc.add(1, VANILLA, pilot=PILOT)
    public = [
        battle,
        sc.st.cards[battle].pair,
        *sc.resources(1, 1),
        sc.base(1, BASE_A),
        sc.add(1, AP3_UNIT, Zone.REMOVAL),
        *sc.trash(1, AP1_UNIT),
    ]
    secret = sc.add(1, AP3_UNIT, Zone.HAND, known=False)
    st = sc.start()
    identities = {u: st.cards[u].def_id for u in public}
    seen_secret = set()
    for seed in range(30):
        det = O.determinize(st, 0, seed)
        assert {u: det.cards[u].def_id for u in public} == identities
        seen_secret.add(det.cards[secret].def_id)
    assert len(seen_secret) > 1  # the hand card, by contrast, is unknown to player 0


# ---------------------------------------------------------------------------------------------
# 4-6 Shield area


@pytest.mark.rule("4-6-1")
@pytest.mark.faq("Q32", "Q36")
@pytest.mark.parametrize("defence", ["base", "shields", "nothing"])
def test_shield_area_is_checked_when_a_player_is_attacked(defence: str) -> None:
    sc = Scenario()
    attacker = sc.add(0, AP3_UNIT)
    base = sc.base(1, BASE_A) if defence == "base" else None
    shields = sc.shields(1, VANILLA, VANILLA) if defence != "nothing" else []
    st = sc.start()
    attack(st, attacker, PLAYER_TARGET)
    pass_all(st)
    if defence == "base":
        assert base is not None and st.cards[base].damage == 3
        assert [zone_of(st, s) for s in shields] == [Zone.SHIELD, Zone.SHIELD]
        assert st.winner is None
    elif defence == "shields":
        assert [zone_of(st, s) for s in shields] == [Zone.TRASH, Zone.SHIELD]
        assert st.winner is None
    else:
        assert st.winner == 0 and st.end_reason is EndReason.BATTLE_DAMAGE


@pytest.mark.rule("4-6-2", "4-6-4-1")
def test_base_goes_to_the_base_section_and_takes_the_top_shield() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    shields = sc.shields(0, VANILLA, AP1_UNIT, AP3_UNIT)
    base = sc.add(0, BASE_A, Zone.HAND)
    st = sc.start()
    for s in shields:
        assert _hidden(st, s, 0) and _hidden(st, s, 1)
    play(st, base)
    assert st.pending is not None and st.pending.kind is DecisionKind.MAIN  # no choice of Shield
    assert st.zones[0][Zone.BASE] == [base]
    assert zone_of(st, shields[0]) is Zone.HAND
    assert st.zones[0][Zone.SHIELD] == shields[1:]
    assert not _hidden(st, shields[0], 0)
    assert _hidden(st, shields[0], 1)
    assert _hidden(st, shields[1], 0)


@pytest.mark.rule("4-6-3", "4-6-3-1")
@pytest.mark.faq("Q28", "Q29")
@pytest.mark.parametrize("old_is_token", [False, True])
def test_base_section_holds_one_base_placed_face_up(old_is_token: bool) -> None:
    sc = Scenario()
    sc.resources(0, 2)
    old = sc.base(0) if old_is_token else sc.base(0, BASE_A)
    sc.shields(0, VANILLA)
    new = sc.add(0, BASE_B, Zone.HAND, known=False)
    st = sc.start()
    assert _hidden(st, new, 1)
    assert has_action(st, A.PLAY_BASE, new)
    play(st, new)
    assert st.pending is not None and st.pending.kind is DecisionKind.MAIN
    assert st.zones[0][Zone.BASE] == [new]
    assert zone_of(st, old) is (Zone.OUTSIDE if old_is_token else Zone.TRASH)
    assert not any(h.kind == "destroyed" and h.uid == old for h in st.history)
    assert not _hidden(st, new, 1)


@pytest.mark.rule("4-6-4-2")
@pytest.mark.faq("Q35")
@pytest.mark.parametrize(("attacker_number", "destroyed"), [(AP1_UNIT, True), (AP0_UNIT, False)])
def test_each_shield_has_one_hp(attacker_number: str, destroyed: bool) -> None:
    sc = Scenario()
    attacker = sc.add(0, attacker_number)
    (shield,) = sc.shields(1, VANILLA)
    st = sc.start()
    assert hp(st, shield) == 1
    attack(st, attacker)
    pass_all(st)
    assert zone_of(st, shield) is (Zone.TRASH if destroyed else Zone.SHIELD)


# ---------------------------------------------------------------------------------------------
# 4-7 Removal area


@pytest.mark.rule("4-7-1", "4-7-2")
@pytest.mark.ruling("GD03-009:Q211")
def test_removed_cards_go_face_up_to_their_owners_removal_area() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    titans = sc.trash(0, TITANS_A, TITANS_B)
    palace = sc.add(0, EXILE_TITANS, Zone.HAND)
    st = sc.start()
    play(st, palace)
    yes_if_asked(st)
    select_if_asked(st, *titans)
    assert st.zones[0][Zone.REMOVAL] == titans
    assert not st.zones[0][Zone.TRASH]
    for uid in titans:
        assert not _hidden(st, uid, 1)
    assert Zone.REMOVAL.is_public


# ---------------------------------------------------------------------------------------------
# 4-8 Hand


@pytest.mark.rule("4-8-1", "4-8-2", "4-8-3")
@pytest.mark.faq("Q44")
def test_drawn_card_goes_to_the_hand_seen_only_by_its_owner() -> None:
    sc = Scenario(active=1)
    st = sc.start()
    top = st.zones[0][Zone.DECK][0]
    assert _hidden(st, top, 0) and _hidden(st, top, 1)
    to_next_turn(st)
    assert st.active == 0
    assert zone_of(st, top) is Zone.HAND
    assert not _hidden(st, top, 0)
    assert _hidden(st, top, 1)


@pytest.mark.rule("4-8-2", "4-8-3")
def test_opponents_hand_is_not_part_of_a_players_information() -> None:
    sc = Scenario()
    mine = sc.add(0, AP3_UNIT, Zone.HAND, known=False)
    theirs = sc.add(1, AP3_UNIT, Zone.HAND, known=False)
    st = sc.start()
    seen_theirs = set()
    for seed in range(30):
        det = O.determinize(st, 0, seed)
        assert det.cards[mine].def_id == st.cards[mine].def_id
        seen_theirs.add(det.cards[theirs].def_id)
    assert len(seen_theirs) > 1
    assert all(
        O.determinize(st, 1, s).cards[theirs].def_id == st.cards[theirs].def_id for s in (0, 1)
    )


@pytest.mark.rule("4-8-3")
def test_revealed_card_added_to_the_hand_is_known_to_both_players() -> None:
    sc = Scenario()
    attacker = sc.add(0, VANILLA)
    (shield,) = sc.shields(1, BURST_ADD)
    st = sc.start()
    assert _hidden(st, shield, 0)
    attack(st, attacker)
    pass_all(st)
    yes(st)
    assert zone_of(st, shield) is Zone.HAND
    assert not _hidden(st, shield, 0)


@pytest.mark.rule("4-8-4")
@pytest.mark.faq("Q16")
def test_hand_may_exceed_ten_until_the_owners_end_phase() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    sc.hand(0, *[VANILLA] * 9)
    cmd = sc.add(0, DRAW_2, Zone.HAND)
    st = sc.start()
    play(st, cmd)
    assert st.pending is not None and st.pending.kind is DecisionKind.MAIN
    assert len(st.zones[0][Zone.HAND]) == 11
    end_main(st)
    pass_all(st)
    assert st.pending is not None and st.pending.kind is DecisionKind.DISCARD
    assert st.pending.player == 0
    discarded = st.pending.options[-1].a
    act(st, A.SELECT, discarded)
    assert len(st.zones[0][Zone.HAND]) == 10
    assert st.zones[0][Zone.TRASH] == [cmd, discarded]
    assert st.pending is not None and st.pending.kind is DecisionKind.MAIN
    assert st.active == 1


@pytest.mark.rule("4-8-4")
def test_hand_limit_is_not_checked_at_ten_or_for_the_standby_player() -> None:
    sc = Scenario()
    sc.hand(0, *[VANILLA] * 10)
    sc.hand(1, *[VANILLA] * 12)
    st = sc.start()
    end_main(st)
    pass_all(st)
    assert st.pending is not None and st.pending.kind is DecisionKind.MAIN
    assert st.active == 1
    assert len(st.zones[0][Zone.HAND]) == 10
    assert len(st.zones[1][Zone.HAND]) == 13  # 12 plus the draw-phase draw
    assert not st.zones[0][Zone.TRASH] and not st.zones[1][Zone.TRASH]


# ---------------------------------------------------------------------------------------------
# 4-9 Trash


@pytest.mark.rule("4-9-1")
@pytest.mark.faq("Q41")
def test_destroyed_unit_and_its_pilot_go_to_the_trash() -> None:
    sc = Scenario()
    attacker = sc.add(0, AP3_UNIT)
    victim = sc.add(1, VANILLA, rested=True, pilot=PILOT)
    pilot = sc.st.cards[victim].pair
    st = sc.start()
    attack(st, attacker, victim)
    pass_all(st)
    assert zone_of(st, victim) is Zone.TRASH
    assert zone_of(st, pilot) is Zone.TRASH


@pytest.mark.rule("4-9-1")
def test_destroyed_base_goes_to_the_trash() -> None:
    sc = Scenario()
    attacker = sc.add(0, VANILLA)
    base = sc.base(1, BASE_A, damage=5)
    st = sc.start()
    attack(st, attacker)
    pass_all(st)
    assert zone_of(st, base) is Zone.TRASH


@pytest.mark.rule("4-9-1")
def test_command_goes_to_the_trash_after_its_effect() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    enemy = sc.add(1, VANILLA)
    cmd = sc.add(0, PING, Zone.HAND)
    st = sc.start()
    play(st, cmd)
    assert st.cards[enemy].damage == 1
    assert st.zones[0][Zone.TRASH] == [cmd]


@pytest.mark.rule("4-9-2")
@pytest.mark.faq("Q45")
def test_discarded_card_becomes_visible_in_the_trash() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    cmd = sc.add(0, DRAW_2_DISCARD_1, Zone.HAND)
    secret = sc.add(0, AP3_UNIT, Zone.HAND, known=False)
    st = sc.start()
    assert _hidden(st, secret, 1)
    play(st, cmd)
    assert st.pending is not None and st.pending.kind is DecisionKind.DISCARD
    act(st, A.SELECT, secret)
    assert zone_of(st, secret) is Zone.TRASH
    assert not _hidden(st, secret, 1)
    assert all(
        O.determinize(st, 1, s).cards[secret].def_id == st.cards[secret].def_id for s in (0, 1)
    )
