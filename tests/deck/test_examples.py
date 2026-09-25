"""The example decks in examples/decks are legal, documented, and playable."""

from __future__ import annotations

from itertools import permutations
from pathlib import Path

import pytest

from gcg_sim.deck import load_deck, to_decklist, validate_deck
from gcg_sim.engine.game import apply, new_game
from gcg_sim.rng import SplitMix64

EXAMPLES = Path(__file__).resolve().parents[2] / "examples" / "decks"
DECKS = sorted(EXAMPLES.glob("*.txt"))


def test_there_are_at_least_three_example_decks() -> None:
    assert len(DECKS) >= 3


@pytest.mark.rule("6-1-1", "6-1-1-1", "6-1-1-2", "6-1-1-3", "6-1-1-4", "6-1-1-5")
@pytest.mark.parametrize("path", DECKS, ids=lambda p: p.stem)
def test_example_deck_is_legal_and_implemented(path: Path) -> None:
    deck = load_deck(path)
    assert validate_deck(deck) == []
    assert len(deck.main) == 50
    assert len(deck.resources) == 10


@pytest.mark.parametrize("path", DECKS, ids=lambda p: p.stem)
def test_example_deck_names_its_archetype_first(path: Path) -> None:
    first = path.read_text(encoding="utf-8").splitlines()[0]
    assert first.startswith("# ")
    assert len(first) > 10


@pytest.mark.parametrize(("a", "b"), list(permutations(DECKS, 2)), ids=lambda p: p.stem)
def test_example_matchups_play_to_completion(a: Path, b: Path) -> None:
    decks = (to_decklist(load_deck(a)), to_decklist(load_deck(b)))
    for seed in range(3):
        st = new_game(decks, seed)
        rng = SplitMix64(seed + 100)
        while st.pending is not None:
            options = st.pending.options
            apply(st, options[rng.randrange(len(options))])
        assert st.winner is not None
        assert st.end_reason is not None
