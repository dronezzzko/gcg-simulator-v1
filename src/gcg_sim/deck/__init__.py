"""Deck files, deck legality, and conversion to the engine's :class:`DeckList`."""

from gcg_sim.deck.model import Deck, DeckEntry, DeckError, Violation
from gcg_sim.deck.parse import deck_digest, load_deck, parse_deck, to_decklist
from gcg_sim.deck.validate import require_legal, validate_deck

__all__ = [
    "Deck",
    "DeckEntry",
    "DeckError",
    "Violation",
    "deck_digest",
    "load_deck",
    "parse_deck",
    "require_legal",
    "to_decklist",
    "validate_deck",
]
