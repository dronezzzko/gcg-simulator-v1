# Deck files

A deck file is plain UTF-8 text with one entry per line:

```text
# Blue/White Earth Federation midrange      <- "#" starts a comment line
4 GD01-008 Guntank                          <- <count> <card_number> [name]
2 GD01-001_p1 Gundam                        <- alt-art product ids are allowed
3 ST01-005                                  <- the name is optional
10 R-001 Resource                           <- Resource cards form the resource deck
```

- **Count**: a positive integer; `4x` is accepted as well as `4`. Several lines for the same card
  number add up.
- **Card id**: a card number (`GD01-008`) or a product id. Alternate-art printings (`GD01-001_p1`)
  normalize to their card number, because all printings of a card number are the same card. Ids
  are matched case-insensitively.
- **Name** (optional): if present it must be the card's name in the card data. Before comparing,
  both names are NFKC-normalized, zero-width characters are removed, curly quotes are
  straightened, whitespace is collapsed, and case is ignored. So `Zaku II` matches `Zaku Ⅱ`, and
  `Char's Zaku II` matches `Char’s Zaku Ⅱ`.
- **Comments and blank lines**: a line whose first non-blank character is `#` is a comment.
  There are no end-of-line comments, because card names may contain `#`.
- **Resource deck**: RESOURCE and EX RESOURCE cards go to the 10-card resource deck. Everything
  else goes to the 50-card main deck. EX Resource is a token, so a deck file that lists it fails
  `CARD_TYPE` (and counts toward `RESOURCE_SIZE`).

Three legal example decks live in `examples/decks/`:

| File | Archetype |
| --- | --- |
| `blue-white-federation.txt` | Blue/White Earth Federation midrange (White Base Team) |
| `red-green-zeon.txt` | Red/Green Zeon aggro (Zeon + Neo Zeon) |
| `purple-white-tekkadan.txt` | Purple/White Iron-Blooded Orphans (Tekkadan + Gjallarhorn) |

## Validating a deck

```bash
uv run gcg-sim validate examples/decks/blue-white-federation.txt
```

The command prints `legal (...)` and exits 0. Otherwise it prints every problem to stderr and
exits 1. `gcg-sim benchmark` runs the same checks on both decks before it plays any game.

Parse errors are reported first, for every affected line:

| Code | Meaning |
| --- | --- |
| `SYNTAX` | The line is not `<count> <card_number> [name]`, or the count is 0. |
| `UNKNOWN_ID` | The id is not in the packaged card data. |
| `NAME_MISMATCH` | The written name does not match the card's name. |

Legality checks run once the whole file parses. They use
`src/gcg_sim/data/official/deck_construction.json` and `banlist.json`:

| Code | Rule | Check |
| --- | --- | --- |
| `MAIN_SIZE` | CR 6-1-1 | The main deck has exactly 50 cards. |
| `RESOURCE_SIZE` | CR 6-1-1 | The resource deck has exactly 10 cards. |
| `CARD_TYPE` | CR 6-1-1-1, 6-1-1-4, 6-1-2 | The main deck holds only Unit, Pilot, Command and Base cards; the resource deck holds only Resource cards; tokens (Unit tokens, EX Base, EX Resource) are in neither. |
| `COPY_LIMIT` | CR 6-1-1-3, 2-1-2 | At most 4 copies per card number, counting all printings together. Different card numbers with the same name count separately (FAQ Q4). |
| `COLORS` | CR 6-1-1-2 | The main deck uses one or two colors. |
| `BANNED` | B&R list | No copies of a banned card. |
| `RESTRICTED` | B&R list | No more than the allowed copies of a restricted card. |
| `BANNED_PAIR` | B&R list | Both cards of a banned pair cannot be in the same deck. |
| `ATTRIBUTE_PAIR` | B&R list | Any two card numbers that are "a Unit card that is Lv.2 with cost 1, 2 AP, and 2 HP, and without effects" are a banned pair, so a deck may contain only one of them. A card belongs to the rule if it matches that description in the card data or is on the official list. If the two disagree for a card, the message says so. |
| `UNIMPLEMENTED` | simulator | Every card has an implemented effect in this simulator version. |

Two official exceptions are **not applied**: the unmodified ST02 starter deck (3x ST02-016
Corsica Base) and the unmodified ST05 starter deck (ST05-004 with ST05-009). The simulator
cannot verify that a deck is an unmodified starter deck, so the violation message names the
exception and says it was not applied. Sideboards, the Premier-event 7-day rule, and physical
card rules are not modelled either (see `docs/research/OFFICIAL_SOURCES.md`).

## Python API

```python
from gcg_sim.deck import load_deck, validate_deck, require_legal, to_decklist, deck_digest

deck = load_deck("examples/decks/red-green-zeon.txt")   # raises DeckError on parse problems
problems = validate_deck(deck)                          # [] means legal
require_legal(deck)                                     # raises DeckError listing every violation
decklist = to_decklist(deck)                            # engine DeckList(main, resources)
digest = deck_digest(deck)                              # sha256 of the card counts
```

`validate_deck(deck, banlist=False)` checks only the Comprehensive Rules.
`validate_deck(deck, implemented=False)` skips the simulator-support check.
