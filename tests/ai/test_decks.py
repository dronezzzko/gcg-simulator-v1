"""The AI test decks are legal and use only implemented cards."""

from __future__ import annotations

import json
from collections import Counter
from importlib import resources

import pytest
from tests.ai.decks import DECKS, FEDERATION, SEED

from gcg_sim.cards.model import MAIN_DECK_TYPES, CardType
from gcg_sim.effects.registry import get_registry


def _banlist() -> dict[str, object]:
    text = resources.files("gcg_sim.data.official").joinpath("banlist.json").read_text("utf-8")
    data: dict[str, object] = json.loads(text)
    return data


@pytest.mark.parametrize("main", [FEDERATION, SEED], ids=["federation", "seed"])
def test_deck_is_legal_and_implemented(main: tuple[str, ...]) -> None:
    R = get_registry()
    cards = [R.db[n] for n in main]
    assert len(main) == 50
    assert max(Counter(main).values()) <= 4
    assert all(c.card_type in MAIN_DECK_TYPES for c in cards)
    assert 1 <= len({c.color for c in cards}) <= 2
    assert all(R.cards[c.def_id].script is not None for c in cards)
    ban = _banlist()
    banned = {b["card_number"] for b in ban["banned"]}  # type: ignore[attr-defined]
    assert not banned & set(main)
    for pair in ban["banned_pairs"]:  # type: ignore[attr-defined]
        assert not set(pair["cards"]) <= set(main)
    vanilla = {
        c.card_number
        for c in cards
        if c.card_type is CardType.UNIT
        and (c.level, c.cost, c.ap, c.hp) == (2, 1, 2, 2)
        and c.is_vanilla
    }
    assert len(vanilla) <= 1


def test_resource_decks() -> None:
    R = get_registry()
    for deck in DECKS:
        assert len(deck.resources) == 10
        assert all(R.db[n].card_type is CardType.RESOURCE for n in deck.resources)
