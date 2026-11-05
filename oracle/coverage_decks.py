"""Covering decks for the ``coverage`` corpus set (docs/dev/PLAN.md §8.3).

Every main-deck pool number (``gcg_sim.tools.fuzz.main_deck_pool``: real Unit, Pilot, Command
and Base cards with a colour and a script, 956 numbers) appears in at least one deck, and every
resource number (``fuzz.resource_pool``, 108) in at least one resource deck. Each deck is one
colour, so it is legal under the engine's construction rules (``engine/game.py:58-100``: 50 main
cards, at most 4 copies of a printing, at most 2 colours, 10 Resource cards).

Construction, deterministic and without randomness:

- colours in value order (``Blue, Green, Purple, Red, White``, as ``tools/fuzz.py:50`` sorts
  them); each colour's numbers in sorted order are cut into runs of 13 targets;
- a deck holds 4 copies of its first 12 targets and 2 of the 13th (50 cards); a shorter last
  run fills up, 4 copies at a time and then the remainder, from the colour's earlier numbers;
- deck ``j`` takes resources ``res[(10 * j + i) % len(res)]`` for ``i`` in ``0..10``;
- pairings put deck ``2k`` against deck ``2k + 1``; an odd last deck meets deck 0.

    uv run python oracle/coverage_decks.py [--json]
"""

from __future__ import annotations

import argparse
import json
from dataclasses import dataclass

from gcg_sim.engine.game import DeckList, deck_construction_problems
from gcg_sim.tools.fuzz import main_deck_pool, resource_pool

TARGETS_PER_DECK = 13
FULL_COPIES = 4
LAST_COPIES = 2
MAIN_SIZE = 50
RESOURCE_SIZE = 10


@dataclass(frozen=True, slots=True)
class CoveringDeck:
    """One covering deck: its colour, the pool numbers it is built to cover, and the list."""

    name: str
    color: str
    targets: tuple[str, ...]
    deck: DeckList


def _main_cards(targets: list[str], earlier: list[str]) -> tuple[str, ...]:
    main: list[str] = []
    for i, n in enumerate(targets):
        copies = LAST_COPIES if i == TARGETS_PER_DECK - 1 else FULL_COPIES
        main += [n] * min(copies, MAIN_SIZE - len(main))
    filler = [n for n in earlier if n not in targets]
    i = 0
    while len(main) < MAIN_SIZE:
        if not filler:
            raise ValueError(f"colour pool too small for a {MAIN_SIZE}-card deck: {targets}")
        n = filler[i % len(filler)]
        have = main.count(n)
        main += [n] * min(FULL_COPIES - have, MAIN_SIZE - len(main))
        i += 1
    return tuple(main)


def covering_decks() -> list[CoveringDeck]:
    """The covering decks in construction order (colour value order, then pool order)."""
    pool = main_deck_pool()
    res = resource_pool()
    out: list[CoveringDeck] = []
    for color in sorted(pool, key=lambda c: c.value):
        numbers = pool[color]
        for start in range(0, len(numbers), TARGETS_PER_DECK):
            targets = numbers[start : start + TARGETS_PER_DECK]
            main = _main_cards(targets, numbers[:start] + numbers[start + TARGETS_PER_DECK :])
            j = len(out)
            resources = tuple(res[(RESOURCE_SIZE * j + i) % len(res)] for i in range(RESOURCE_SIZE))
            deck = DeckList(main, resources)
            problems = deck_construction_problems(deck)
            if problems:
                raise ValueError(f"covering deck {j} is not legal: {problems}")
            name = f"{color.value.lower()}-{start // TARGETS_PER_DECK:02d}"
            out.append(CoveringDeck(name, color.value, tuple(targets), deck))
    return out


def pairings(decks: list[CoveringDeck]) -> list[tuple[int, int]]:
    """Deck index pairs ``(2k, 2k + 1)``; an odd last deck plays deck 0."""
    out = [(i, i + 1) for i in range(0, len(decks) - 1, 2)]
    if len(decks) % 2:
        out.append((len(decks) - 1, 0))
    return out


def check_coverage(decks: list[CoveringDeck]) -> None:
    """Raise unless every pool number and every resource number is in some deck."""
    pool = {n for ns in main_deck_pool().values() for n in ns}
    seen = {n for d in decks for n in d.deck.main}
    res_seen = {n for d in decks for n in d.deck.resources}
    missing = sorted(pool - seen) + sorted(set(resource_pool()) - res_seen)
    if missing:
        raise ValueError(f"covering decks miss {len(missing)} numbers: {missing[:10]}")


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--json", action="store_true", help="print every deck as JSON")
    args = ap.parse_args(argv)
    decks = covering_decks()
    check_coverage(decks)
    if args.json:
        doc = [
            {
                "name": d.name,
                "color": d.color,
                "targets": list(d.targets),
                "main": list(d.deck.main),
                "resources": list(d.deck.resources),
            }
            for d in decks
        ]
        print(json.dumps(doc, indent=1))
    else:
        print(f"{len(decks)} covering decks, {len(pairings(decks))} pairings")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
