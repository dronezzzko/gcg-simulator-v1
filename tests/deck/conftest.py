from __future__ import annotations

from collections import Counter
from collections.abc import Callable
from pathlib import Path

import pytest

from gcg_sim.deck import Deck, parse_deck

EXAMPLES = Path(__file__).resolve().parents[2] / "examples" / "decks"
BASE_DECK = EXAMPLES / "blue-white-federation.txt"

DeckFactory = Callable[[dict[str, int]], Deck]


def deck_text(counts: dict[str, int]) -> str:
    return "\n".join(f"{c} {n}" for n, c in counts.items() if c > 0) + "\n"


@pytest.fixture
def base_counts() -> dict[str, int]:
    """Card counts (main + resource deck) of a legal Blue/White example deck."""
    deck = parse_deck(BASE_DECK.read_text(encoding="utf-8"))
    return dict(Counter(deck.main + deck.resources))


@pytest.fixture
def make_deck() -> DeckFactory:
    def build(counts: dict[str, int]) -> Deck:
        return parse_deck(deck_text(counts), name="test-deck")

    return build
