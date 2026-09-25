"""Deck-file parser: ``<count> <card_number> [name]`` lines, ``#`` comment lines, blank lines."""

from __future__ import annotations

import hashlib
import json
import os
import re
from collections import Counter
from collections.abc import Iterator
from pathlib import Path

from gcg_sim.cards.db import CardDB, get_card_db
from gcg_sim.deck.model import Deck, DeckEntry, DeckError, Violation
from gcg_sim.deck.names import names_match, strip_invisible
from gcg_sim.engine.game import DeckList

_ENTRY = re.compile(r"^(?P<count>\d+)[xX]?\s+(?P<id>\S+)(?:\s+(?P<name>.*\S))?\s*$")
_ALT_ART = re.compile(r"^(?P<base>.+?)_[pP]\d+$")
SYNTHETIC_PREFIX = "TOKEN:"


def _id_candidates(written: str) -> Iterator[str]:
    yield written
    yield written.upper()
    base, sep, suffix = written.partition("_")
    if sep:
        yield f"{base.upper()}_{suffix.lower()}"
    alt = _ALT_ART.match(written)
    if alt:
        yield alt["base"].upper()


def resolve_card_number(written: str, db: CardDB | None = None) -> str | None:
    """Canonical card number for a card number or product id (alt-art ``_pN`` printings
    normalize to their card number); ``None`` when the id is not in the card data."""
    cards = db or get_card_db()
    for candidate in _id_candidates(written):
        cdef = cards.get(candidate)
        if cdef is not None and not cdef.card_number.startswith(SYNTHETIC_PREFIX):
            return cdef.card_number
    return None


def _parse_line(db: CardDB, lineno: int, line: str) -> DeckEntry | Violation:
    m = _ENTRY.match(line)
    if m is None:
        return Violation(
            "SYNTAX",
            f"line {lineno}: expected '<count> <card_number> [name]', got {line!r}",
            (),
        )
    count = int(m["count"])
    written = m["id"]
    if count < 1:
        return Violation("SYNTAX", f"line {lineno}: count must be at least 1 for {written}", ())
    number = resolve_card_number(written, db)
    if number is None:
        return Violation(
            "UNKNOWN_ID", f"line {lineno}: unknown card id {written!r} (not in the card data)", ()
        )
    name = m["name"]
    if name is not None and not names_match(name, db[number].names):
        return Violation(
            "NAME_MISMATCH",
            f"line {lineno}: {written} is {db[number].name!r} in the card data, not {name!r}",
            (number,),
        )
    return DeckEntry(count=count, card_number=number, written=written, name=name, line=lineno)


def parse_deck(text: str, *, name: str = "deck", source: str | None = None) -> Deck:
    """Parse deck-file text. Raises :class:`DeckError` listing every SYNTAX, UNKNOWN_ID and
    NAME_MISMATCH problem. Legality is checked separately by ``validate_deck``."""
    db = get_card_db()
    entries: list[DeckEntry] = []
    problems: list[Violation] = []
    for lineno, raw in enumerate(text.splitlines(), 1):
        line = strip_invisible(raw).strip()
        if not line or line.startswith("#"):
            continue
        parsed = _parse_line(db, lineno, line)
        if isinstance(parsed, Violation):
            problems.append(parsed)
        else:
            entries.append(parsed)
    if problems:
        raise DeckError([p.message for p in problems], deck_name=name, violations=problems)
    return build_deck(entries, name=name, source=source)


def build_deck(entries: list[DeckEntry], *, name: str, source: str | None = None) -> Deck:
    """Assemble a deck from entries: RESOURCE and EX RESOURCE cards go to the resource deck,
    everything else to the main deck (legality is checked by ``validate_deck``)."""
    db = get_card_db()
    main: list[str] = []
    resources: list[str] = []
    for e in entries:
        cdef = db.get(e.card_number)
        target = resources if cdef is not None and cdef.card_type.is_resource else main
        target.extend([e.card_number] * e.count)
    return Deck(
        name=name,
        main=tuple(sorted(main)),
        resources=tuple(sorted(resources)),
        entries=tuple(entries),
        source=source,
    )


def load_deck(path: str | os.PathLike[str]) -> Deck:
    p = Path(path)
    text = p.read_text(encoding="utf-8-sig")
    return parse_deck(text, name=p.stem, source=str(p))


def to_decklist(deck: Deck) -> DeckList:
    return DeckList(main=deck.main, resources=deck.resources)


def deck_counts(cards: tuple[str, ...]) -> dict[str, int]:
    return dict(sorted(Counter(cards).items()))


def deck_digest(deck: Deck) -> str:
    """sha256 of the canonical content (card counts only; name, order and comments ignored)."""
    canonical = {"main": deck_counts(deck.main), "resources": deck_counts(deck.resources)}
    blob = json.dumps(canonical, sort_keys=True, separators=(",", ":")).encode()
    return hashlib.sha256(blob).hexdigest()
