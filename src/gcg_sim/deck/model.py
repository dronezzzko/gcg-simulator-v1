"""Deck data types: parsed entries, the deck itself, violations, and the error that lists them."""

from __future__ import annotations

from collections.abc import Iterable
from dataclasses import dataclass


@dataclass(frozen=True, slots=True)
class DeckEntry:
    """One line of a deck file."""

    count: int
    card_number: str
    written: str
    name: str | None
    line: int


@dataclass(frozen=True, slots=True)
class Deck:
    """A deck (``main``) and a resource deck (``resources``), one card number per copy, sorted."""

    name: str
    main: tuple[str, ...]
    resources: tuple[str, ...]
    entries: tuple[DeckEntry, ...]
    source: str | None


@dataclass(frozen=True, slots=True)
class Violation:
    """A deck-construction problem with a specific, human-readable message."""

    code: str
    message: str
    cards: tuple[str, ...]


class DeckError(ValueError):
    """A deck that cannot be parsed or is not legal. ``problems`` holds one message per issue."""

    def __init__(
        self,
        problems: Iterable[str],
        *,
        deck_name: str | None = None,
        violations: Iterable[Violation] = (),
    ) -> None:
        self.problems = list(problems)
        self.deck_name = deck_name
        self.violations = tuple(violations)
        super().__init__(self._render())

    def _render(self) -> str:
        subject = f"deck {self.deck_name!r}" if self.deck_name else "deck"
        header = f"{subject} has {len(self.problems)} problem(s):"
        return "\n".join([header, *(f"  - {p}" for p in self.problems)])
