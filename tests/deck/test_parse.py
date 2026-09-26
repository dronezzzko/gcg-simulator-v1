"""Deck-file parsing: format, comments, alt-art ids, name checks, and parse errors."""

from __future__ import annotations

from collections import Counter
from pathlib import Path

import pytest
from hypothesis import given, settings
from hypothesis import strategies as st

from gcg_sim.cards.db import get_card_db
from gcg_sim.deck import DeckError, deck_digest, load_deck, parse_deck, to_decklist
from gcg_sim.deck.names import normalize_name
from gcg_sim.engine.game import DeckList

SAMPLE = """\
# a comment line
4 GD01-008 Guntank

2 GD01-001_p1 Gundam
3x st01-005 GM
   # indented comment
10 R-001 Resource
"""


def test_parses_counts_comments_and_blank_lines() -> None:
    deck = parse_deck(SAMPLE, name="sample", source="sample.txt")
    assert deck.name == "sample"
    assert deck.source == "sample.txt"
    assert Counter(deck.main) == {"GD01-008": 4, "GD01-001": 2, "ST01-005": 3}
    assert deck.resources == ("R-001",) * 10
    assert list(deck.main) == sorted(deck.main)
    assert [e.line for e in deck.entries] == [2, 4, 5, 7]


def test_alt_art_product_ids_normalize_to_the_card_number() -> None:
    deck = parse_deck("2 GD01-001_p1\n1 gd01-001_P2\n1 GD01-001\n")
    assert deck.main == ("GD01-001",) * 4
    assert [e.written for e in deck.entries] == ["GD01-001_p1", "gd01-001_P2", "GD01-001"]
    assert {e.card_number for e in deck.entries} == {"GD01-001"}


def test_unknown_alt_art_suffix_of_a_known_card_normalizes() -> None:
    deck = parse_deck("1 GD01-001_p99\n")
    assert deck.main == ("GD01-001",)


@pytest.mark.parametrize(
    ("line", "number"),
    [
        ("4 GD01-035 Zaku II", "GD01-035"),
        ("4 GD01-035 zaku ⅱ", "GD01-035"),
        ("4 ST03-006 Char's Zaku II", "ST03-006"),
        ("4 ST03-006 Char\u2019s  Zaku\u200b Ⅱ", "ST03-006"),
        ("4 GD03-027 Z'Gok E", "GD03-027"),
        ("4 GD03-027 Z\u2019Gok E", "GD03-027"),
        ("\ufeff4 GD01-008 Guntank", "GD01-008"),
    ],
)
def test_names_are_normalized_before_comparison(line: str, number: str) -> None:
    deck = parse_deck(line + "\n")
    assert deck.main == (number,) * 4


def test_normalize_name() -> None:
    assert normalize_name("Zaku Ⅱ") == normalize_name("ZAKU II")
    assert normalize_name("Z\u2019Gok\u00a0E") == "z'gok e"
    assert normalize_name("A\u200bB") == "ab"


def test_syntax_error_names_the_line() -> None:
    with pytest.raises(DeckError) as info:
        parse_deck("4 GD01-008\nfour GD01-001\n")
    assert info.value.problems == [
        "line 2: expected '<count> <card_number> [name]', got 'four GD01-001'"
    ]
    assert [v.code for v in info.value.violations] == ["SYNTAX"]


def test_zero_count_is_a_syntax_error() -> None:
    with pytest.raises(DeckError) as info:
        parse_deck("0 GD01-008\n")
    assert info.value.problems == ["line 1: count must be at least 1 for GD01-008"]


def test_unknown_id_is_reported_with_its_line() -> None:
    with pytest.raises(DeckError) as info:
        parse_deck("4 GD01-008\n4 GD99-999 Nothing\n")
    assert info.value.problems == ["line 2: unknown card id 'GD99-999' (not in the card data)"]
    assert info.value.violations[0].code == "UNKNOWN_ID"


def test_name_mismatch_is_reported() -> None:
    with pytest.raises(DeckError) as info:
        parse_deck("4 GD01-008 Gundam\n")
    assert info.value.problems == ["line 1: GD01-008 is 'Guntank' in the card data, not 'Gundam'"]
    assert info.value.violations[0].code == "NAME_MISMATCH"
    assert info.value.violations[0].cards == ("GD01-008",)


def test_every_parse_problem_is_listed_in_one_error() -> None:
    text = "4 GD01-008 Gundam\nbad line\n4 XX-1\n"
    with pytest.raises(DeckError) as info:
        parse_deck(text, name="broken")
    codes = [v.code for v in info.value.violations]
    assert codes == ["NAME_MISMATCH", "SYNTAX", "UNKNOWN_ID"]
    message = str(info.value)
    assert message.splitlines()[0] == "deck 'broken' has 3 problem(s):"
    assert all(f"  - {p}" in message for p in info.value.problems)


def test_load_deck_uses_file_stem_and_handles_bom(tmp_path: Path) -> None:
    path = tmp_path / "my-deck.txt"
    path.write_bytes("\ufeff# header\n4 GD01-008 Guntank\n".encode())
    deck = load_deck(path)
    assert deck.name == "my-deck"
    assert deck.source == str(path)
    assert deck.main == ("GD01-008",) * 4


def test_to_decklist() -> None:
    deck = parse_deck(SAMPLE)
    assert to_decklist(deck) == DeckList(main=deck.main, resources=deck.resources)


def test_digest_ignores_order_comments_names_and_split_lines() -> None:
    a = parse_deck("4 GD01-008 Guntank\n2 GD01-001\n10 R-001\n", name="a")
    b = parse_deck("# x\n2 GD01-001_p1 Gundam\n2 GD01-008\n2 GD01-008\n10 R-001\n", name="b")
    c = parse_deck("3 GD01-008\n2 GD01-001\n10 R-001\n")
    assert deck_digest(a) == deck_digest(b)
    assert deck_digest(a) != deck_digest(c)
    assert len(deck_digest(a)) == 64


POOL = sorted(
    c.card_number for c in get_card_db().real_cards() if c.card_type.value in ("UNIT", "RESOURCE")
)[:60]


@settings(max_examples=60, deadline=None)
@given(
    counts=st.dictionaries(st.sampled_from(POOL), st.integers(1, 9), min_size=1, max_size=12),
    with_names=st.booleans(),
    comments=st.lists(st.sampled_from(["# c", "", "   ", "#"]), max_size=4),
)
def test_round_trip_preserves_counts(
    counts: dict[str, int], with_names: bool, comments: list[str]
) -> None:
    db = get_card_db()
    lines = [f"{c} {n} {db[n].name}" if with_names else f"{c} {n}" for n, c in counts.items()]
    text = "\n".join(comments + lines + comments)
    deck = parse_deck(text)
    assert Counter(deck.main + deck.resources) == counts
    assert all(db[n].card_type.value == "RESOURCE" for n in deck.resources)
    assert all(db[n].card_type.value != "RESOURCE" for n in deck.main)
