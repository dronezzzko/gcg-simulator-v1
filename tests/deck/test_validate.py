"""Deck legality: one test per violation class, each asserting its specific message.

Tagged with the deck-construction rules (Comprehensive Rules 2-1-2, 3-x-2, 6-1-1-x) and the
"Preparing to Play" rules-FAQ entries Q1-Q7.
"""

from __future__ import annotations

import dataclasses
from collections.abc import Callable
from types import SimpleNamespace

import pytest

from gcg_sim.cards.db import get_card_db
from gcg_sim.cards.model import MAIN_DECK_TYPES, CardType
from gcg_sim.deck import (
    Deck,
    DeckEntry,
    DeckError,
    official,
    parse_deck,
    require_legal,
    validate_deck,
)
from gcg_sim.deck import validate as validate_module
from gcg_sim.deck.validate import attribute_members
from gcg_sim.effects.registry import get_registry

MakeDeck = Callable[[dict[str, int]], Deck]
BL_DATE = "2026-09-25"
VANILLA = "a Unit card that is Lv.2 with cost 1, 2 AP, and 2 HP, and without effects"
NOT_APPLIED = "is not applied: the simulator cannot verify an unmodified starter deck"


def codes(deck: Deck, **kwargs: bool) -> list[str]:
    return [v.code for v in validate_deck(deck, **kwargs)]


def only(deck: Deck, code: str, **kwargs: bool) -> list[str]:
    return [v.message for v in validate_deck(deck, **kwargs) if v.code == code]


def swap(counts: dict[str, int], remove: dict[str, int], add: dict[str, int]) -> dict[str, int]:
    out = dict(counts)
    for n, c in remove.items():
        out[n] -= c
    for n, c in add.items():
        out[n] = out.get(n, 0) + c
    return {n: c for n, c in out.items() if c > 0}


def mono_color_deck(colors: tuple[str, ...]) -> Deck:
    """A 50-card deck of implemented, unrestricted cards of exactly ``colors``."""
    db, reg, bl = get_card_db(), get_registry(), official.banlist()
    avoid = set(bl.banned) | {r.card_number for r in bl.restricted}
    avoid |= {n for p in bl.banned_pairs for n in p.cards}
    for rule in bl.attribute_rules:
        avoid |= set(attribute_members(db, rule, (c.card_number for c in db.real_cards())))
    pools = [
        [
            c.card_number
            for c in db.real_cards()
            if c.card_type in MAIN_DECK_TYPES
            and c.color is not None
            and c.color.value == color
            and c.card_number not in avoid
            and reg.cards[c.def_id].script is not None
        ]
        for color in colors
    ]
    main: list[str] = []
    for i in range(13):
        for pool in pools:
            main += [pool[i]] * 4
    text = "\n".join(f"1 {n}" for n in main[:50]) + "\n10 R-001\n"
    return parse_deck(text, name="-".join(colors))


def test_base_deck_is_legal(base_counts: dict[str, int], make_deck: MakeDeck) -> None:
    assert validate_deck(make_deck(base_counts)) == []


# --- MAIN_SIZE / RESOURCE_SIZE -------------------------------------------------------------


@pytest.mark.rule("6-1-1")
@pytest.mark.faq("Q1")
@pytest.mark.parametrize("delta", [-1, 1])
def test_main_deck_must_have_exactly_50_cards(
    base_counts: dict[str, int], make_deck: MakeDeck, delta: int
) -> None:
    deck = make_deck(swap(base_counts, {}, {"ST01-005": delta}))
    violations = validate_deck(deck)
    assert [v.code for v in violations] == ["MAIN_SIZE"]
    assert violations[0].message == (
        f"the deck has {50 + delta} cards; it must have exactly 50 (rule 6-1-1)"
    )


@pytest.mark.rule("6-1-1")
@pytest.mark.faq("Q5")
@pytest.mark.parametrize("resources", [9, 11])
def test_resource_deck_must_have_exactly_10_cards(
    base_counts: dict[str, int], make_deck: MakeDeck, resources: int
) -> None:
    deck = make_deck({**base_counts, "R-001": resources})
    violations = validate_deck(deck)
    assert [v.code for v in violations] == ["RESOURCE_SIZE"]
    assert violations[0].message == (
        f"the resource deck has {resources} cards; it must have exactly 10 Resource cards "
        "(rule 6-1-1)"
    )


# --- COPY_LIMIT ---------------------------------------------------------------------------


@pytest.mark.rule("6-1-1-3", "2-1-2")
@pytest.mark.faq("Q3")
def test_at_most_four_copies_per_card_number(
    base_counts: dict[str, int], make_deck: MakeDeck
) -> None:
    deck = make_deck(swap(base_counts, {"ST01-005": 1}, {"GD01-008": 1}))
    violations = validate_deck(deck)
    assert [v.code for v in violations] == ["COPY_LIMIT"]
    assert violations[0].message == (
        "5 copies of GD01-008 Guntank; at most 4 copies of cards with the same card number "
        "(all printings count together) may be in a deck (rules 6-1-1-3, 2-1-2)"
    )
    assert violations[0].cards == ("GD01-008",)


@pytest.mark.rule("6-1-1-3", "2-1-2")
def test_alternate_printings_count_as_the_same_card(
    base_counts: dict[str, int], make_deck: MakeDeck
) -> None:
    counts = swap(base_counts, {"ST01-005": 3, "GD01-001": 2}, {})
    text = "".join(f"{c} {n}\n" for n, c in counts.items()) + "3 GD01-001\n2 GD01-001_p1\n"
    violations = validate_deck(parse_deck(text))
    assert [v.code for v in violations] == ["COPY_LIMIT"]
    assert violations[0].message.startswith("5 copies of GD01-001 Gundam;")


@pytest.mark.rule("6-1-1-3")
@pytest.mark.faq("Q4")
def test_same_name_with_different_card_numbers_counts_separately(
    base_counts: dict[str, int], make_deck: MakeDeck
) -> None:
    deck = make_deck(swap(base_counts, {"GD01-001": 2, "GD04-008": 2}, {"GD01-013": 4}))
    names = {get_card_db()[n].name for n in ("ST01-001", "GD01-013")}
    assert names == {"Gundam"}
    assert deck.main.count("ST01-001") == 4
    assert deck.main.count("GD01-013") == 4
    assert validate_deck(deck) == []


@pytest.mark.rule("6-1-1-5")
@pytest.mark.faq("Q6")
def test_resource_deck_allows_any_number_of_one_card_number(
    base_counts: dict[str, int], make_deck: MakeDeck
) -> None:
    deck = make_deck(base_counts)
    assert deck.resources == ("R-001",) * 10
    assert validate_deck(deck) == []
    mixed = make_deck(swap(base_counts, {"R-001": 4}, {"R-002": 4}))
    assert validate_deck(mixed) == []


# --- COLORS -------------------------------------------------------------------------------


@pytest.mark.rule("6-1-1-2")
@pytest.mark.faq("Q2")
def test_at_most_two_colors(base_counts: dict[str, int], make_deck: MakeDeck) -> None:
    deck = make_deck(swap(base_counts, {"ST01-005": 3}, {"GD01-059": 3}))
    violations = validate_deck(deck)
    assert [v.code for v in violations] == ["COLORS"]
    assert violations[0].message == (
        "the deck uses 3 colors (Blue x41, White x6, Red x3); a deck must be constructed "
        "entirely using either one or two card colors (rule 6-1-1-2). Cards of the extra "
        "color(s): GD01-059 Zee Zulu"
    )
    assert violations[0].cards == ("GD01-059",)


@pytest.mark.rule("6-1-1-2", "6-1-1-2-1")
@pytest.mark.parametrize("colors", [("Red",), ("Green", "White"), ("Blue",), ("Purple", "Red")])
def test_one_or_two_colors_are_legal(colors: tuple[str, ...]) -> None:
    deck = mono_color_deck(colors)
    db = get_card_db()
    used = {db[n].color.value for n in deck.main if db[n].color is not None}
    assert used == set(colors)
    assert validate_deck(deck) == []


# --- CARD_TYPE ----------------------------------------------------------------------------


@pytest.mark.rule("6-1-1-1", "3-2-2", "3-3-2", "3-4-2", "3-5-2", "3-6-2", "6-1-1-4")
@pytest.mark.faq("Q2", "Q6")
def test_card_types_route_to_the_deck_they_compose(
    base_counts: dict[str, int], make_deck: MakeDeck
) -> None:
    deck = make_deck(base_counts)
    db = get_card_db()
    assert {db[n].card_type for n in deck.main} == set(MAIN_DECK_TYPES)
    assert {db[n].card_type for n in deck.resources} == {CardType.RESOURCE}
    assert "CARD_TYPE" not in codes(deck)


@pytest.mark.rule("6-1-1-1")
@pytest.mark.faq("Q7")
def test_tokens_are_not_part_of_the_deck(base_counts: dict[str, int], make_deck: MakeDeck) -> None:
    deck = make_deck(swap(base_counts, {"ST01-005": 3}, {"T-001": 2, "EXB-001": 1}))
    violations = validate_deck(deck)
    assert [v.code for v in violations] == ["CARD_TYPE", "CARD_TYPE"]
    assert [v.message for v in violations] == [
        "EXB-001 EX Base is a token (EX BASE); tokens are prepared outside the game and are "
        "never part of a deck or resource deck (rule 6-1-2, FAQ Q7)",
        "T-001 Gundam is a token (UNIT TOKEN); tokens are prepared outside the game and are "
        "never part of a deck or resource deck (rule 6-1-2, FAQ Q7)",
    ]


@pytest.mark.rule("6-1-1-4")
@pytest.mark.faq("Q6", "Q7")
def test_resource_deck_holds_only_resource_cards(
    base_counts: dict[str, int], make_deck: MakeDeck
) -> None:
    deck = make_deck(swap(base_counts, {"R-001": 1}, {"EXR-001": 1}))
    assert only(deck, "CARD_TYPE") == [
        "EXR-001 EX Resource is a token (EX RESOURCE); tokens are prepared outside the game "
        "and are never part of a deck or resource deck (rule 6-1-2, FAQ Q7)"
    ]
    base = make_deck(base_counts)
    unit_in_resources = dataclasses.replace(base, resources=("GD01-008", *base.resources[1:]))
    assert only(unit_in_resources, "CARD_TYPE") == [
        "GD01-008 Guntank is a UNIT card; a resource deck is constructed with Resource cards "
        "only (rule 6-1-1-4)"
    ]


@pytest.mark.rule("6-1-1-1")
@pytest.mark.faq("Q2")
def test_main_deck_holds_only_unit_pilot_command_base(
    base_counts: dict[str, int], make_deck: MakeDeck
) -> None:
    base = make_deck(base_counts)
    resource_in_main = dataclasses.replace(base, main=tuple(sorted(("R-001", *base.main[1:]))))
    assert only(resource_in_main, "CARD_TYPE") == [
        "R-001 Resource is a RESOURCE card; a deck is constructed with Unit, Pilot, Command, "
        "and Base cards only (rule 6-1-1-1)"
    ]


# --- Banned & Restricted list --------------------------------------------------------------


def test_banned_card(base_counts: dict[str, int], make_deck: MakeDeck) -> None:
    deck = make_deck(swap(base_counts, {"GD01-001": 2}, {"GD01-020": 2}))
    violations = validate_deck(deck)
    assert [v.code for v in violations] == ["BANNED"]
    assert violations[0].message == (
        f"GD01-020 Anksha is banned: no copies may be in a deck (Banned & Restricted list "
        f"effective {BL_DATE})"
    )
    assert codes(deck, banlist=False) == []


def test_restricted_card(base_counts: dict[str, int], make_deck: MakeDeck) -> None:
    deck = make_deck(swap(base_counts, {"ST08-015": 2, "GD04-122": 1}, {"ST02-016": 3}))
    assert codes(deck, implemented=False) == ["RESTRICTED"]
    assert only(deck, "RESTRICTED") == [
        f"3 copies of ST02-016 Corsica Base; it is restricted to 2 copies (Banned & Restricted "
        f"list effective {BL_DATE}). Exception 'st02-unmodified-starter-deck' {NOT_APPLIED}."
    ]
    two = make_deck(swap(base_counts, {"ST08-015": 2}, {"ST02-016": 2}))
    assert "RESTRICTED" not in codes(two)


@pytest.mark.parametrize(
    ("remove", "add", "message"),
    [
        (
            {"ST01-005": 3},
            {"GD05-015": 3},
            "GD01-008 Guntank, GD05-015 M1 Astray Shrike are a banned pair and cannot be in "
            f"the same deck (Banned & Restricted list effective {BL_DATE})",
        ),
        (
            {"GD04-084": 2},
            {"ST05-010": 2},
            "ST01-010 Amuro Ray, ST05-010 Mikazuki Augus are a banned pair and cannot be in "
            f"the same deck (Banned & Restricted list effective {BL_DATE})",
        ),
    ],
)
def test_banned_pairs(
    base_counts: dict[str, int],
    make_deck: MakeDeck,
    remove: dict[str, int],
    add: dict[str, int],
    message: str,
) -> None:
    deck = make_deck(swap(base_counts, remove, add))
    assert only(deck, "BANNED_PAIR") == [message]
    assert "BANNED_PAIR" not in codes(make_deck(base_counts))


def test_attribute_pair(base_counts: dict[str, int], make_deck: MakeDeck) -> None:
    deck = make_deck(swap(base_counts, {"ST01-005": 1}, {"GD05-014": 1}))
    violations = validate_deck(deck)
    assert [v.code for v in violations] == ["ATTRIBUTE_PAIR"]
    assert violations[0].message == (
        f'GD05-014 Javelin, ST01-005 GM are each "{VANILLA}"; every combination of two such '
        "card numbers is a banned pair, so a deck may contain only one of them, at most 4 "
        f"copies (Banned & Restricted list effective {BL_DATE}). Exception "
        f"'st05-unmodified-starter-deck' {NOT_APPLIED}."
    )
    assert violations[0].cards == ("GD05-014", "ST01-005")


def test_attribute_pair_starter_deck_exception_is_not_applied(
    base_counts: dict[str, int], make_deck: MakeDeck
) -> None:
    deck = make_deck(swap(base_counts, {"ST01-005": 3}, {"ST05-004": 2, "ST05-009": 1}))
    messages = only(deck, "ATTRIBUTE_PAIR")
    assert len(messages) == 1
    assert messages[0].startswith("ST05-004 Graze Custom, ST05-009 Graze are each")
    assert f"Exception 'st05-unmodified-starter-deck' {NOT_APPLIED}." in messages[0]


def test_one_attribute_card_number_with_four_copies_is_legal(
    base_counts: dict[str, int], make_deck: MakeDeck
) -> None:
    deck = make_deck(swap(base_counts, {"ST01-004": 1}, {"ST01-005": 1}))
    assert deck.main.count("ST01-005") == 4
    assert validate_deck(deck) == []


def test_attribute_predicate_matches_the_enumerated_list() -> None:
    db = get_card_db()
    (rule,) = official.banlist().attribute_rules
    by_predicate = {
        c.card_number for c in db.real_cards() if official.matches(rule.member_predicate, c)
    }
    assert by_predicate == set(rule.enumerated)
    assert len(by_predicate) == 22


@pytest.mark.parametrize(
    ("enumerated_change", "add", "note"),
    [
        (
            lambda s: s - {"GD05-014"},
            {"GD05-014": 1},
            "Note: GD05-014 matches the description in the card data but is not on the "
            "official list.",
        ),
        (
            lambda s: s | {"GD01-008"},
            {},
            "Note: GD01-008 is on the official list but does not match the description in the "
            "data.",
        ),
    ],
)
def test_attribute_pair_cross_checks_predicate_and_list(
    base_counts: dict[str, int],
    make_deck: MakeDeck,
    monkeypatch: pytest.MonkeyPatch,
    enumerated_change: Callable[[frozenset[str]], frozenset[str]],
    add: dict[str, int],
    note: str,
) -> None:
    real = official.banlist()
    (rule,) = real.attribute_rules
    changed = dataclasses.replace(rule, enumerated=frozenset(enumerated_change(rule.enumerated)))
    monkeypatch.setattr(
        official, "banlist", lambda: dataclasses.replace(real, attribute_rules=(changed,))
    )
    deck = make_deck(swap(base_counts, {"ST01-005": sum(add.values())}, add))
    messages = only(deck, "ATTRIBUTE_PAIR")
    assert len(messages) == 1
    assert messages[0].endswith(note)


# --- UNKNOWN_ID / NAME_MISMATCH / UNIMPLEMENTED ----------------------------------------------


def test_unknown_id(base_counts: dict[str, int], make_deck: MakeDeck) -> None:
    base = make_deck(base_counts)
    deck = dataclasses.replace(base, main=(*base.main[:-1], "GD99-999"))
    violations = validate_deck(deck)
    assert [v.code for v in violations] == ["UNKNOWN_ID"]
    assert violations[0].message == "unknown card id 'GD99-999' (not in the card data)"


def test_name_mismatch(base_counts: dict[str, int], make_deck: MakeDeck) -> None:
    base = make_deck(base_counts)
    entry = DeckEntry(count=4, card_number="GD01-008", written="GD01-008", name="Gundam", line=3)
    deck = dataclasses.replace(base, entries=(entry,))
    assert only(deck, "NAME_MISMATCH") == [
        "line 3: GD01-008 is 'Guntank' in the card data, not 'Gundam'"
    ]


def test_unimplemented_card(
    base_counts: dict[str, int], make_deck: MakeDeck, monkeypatch: pytest.MonkeyPatch
) -> None:
    reg = get_registry()
    def_id = get_card_db()["GD01-008"].def_id
    cards = list(reg.cards)
    cards[def_id] = dataclasses.replace(cards[def_id], script=None, error="not compiled")
    monkeypatch.setattr(validate_module, "get_registry", lambda: SimpleNamespace(cards=cards))
    deck = make_deck(base_counts)
    violations = validate_deck(deck)
    assert [v.code for v in violations] == ["UNIMPLEMENTED"]
    assert violations[0].message == (
        "GD01-008 Guntank has no implemented effect in this simulator version, so games "
        "cannot use it (not compiled)"
    )
    assert validate_deck(deck, implemented=False) == []


# --- require_legal ------------------------------------------------------------------------


def test_require_legal_lists_every_violation(
    base_counts: dict[str, int], make_deck: MakeDeck
) -> None:
    require_legal(make_deck(base_counts))
    bad = make_deck(swap(base_counts, {"ST01-005": 3, "GD01-001": 2}, {"GD01-059": 1}))
    with pytest.raises(DeckError) as info:
        require_legal(bad)
    assert [v.code for v in info.value.violations] == ["MAIN_SIZE", "COLORS"]
    assert info.value.problems[0].startswith("[MAIN_SIZE] the deck has 46 cards")
    assert info.value.problems[1].startswith("[COLORS] the deck uses 3 colors")
    assert str(info.value).splitlines()[0] == "deck 'test-deck' has 2 problem(s):"
