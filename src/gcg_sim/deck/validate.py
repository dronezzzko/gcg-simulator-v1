"""Deck legality (rules 6-1-1, 2-1-2, 3-x-2 and the Banned & Restricted list) plus the
simulator's own requirement that every card has an implemented effect."""

from __future__ import annotations

from collections import Counter
from collections.abc import Iterable

from gcg_sim.cards.db import CardDB, get_card_db
from gcg_sim.cards.model import CardDef
from gcg_sim.deck import official
from gcg_sim.deck.model import Deck, DeckError, Violation
from gcg_sim.deck.names import names_match
from gcg_sim.deck.official import AttributePairRule, RuleException, construction_rules, matches
from gcg_sim.effects.registry import get_registry

NOT_APPLIED = "is not applied: the simulator cannot verify an unmodified starter deck"


def _label(db: CardDB, number: str) -> str:
    cdef = db.get(number)
    return f"{number} {cdef.name}" if cdef else number


def _labels(db: CardDB, numbers: Iterable[str]) -> str:
    return ", ".join(_label(db, n) for n in numbers)


def _exception_note(exceptions: tuple[RuleException, ...]) -> str:
    return "".join(f" Exception {e.id!r} {NOT_APPLIED}." for e in exceptions)


def _known(db: CardDB, deck: Deck) -> tuple[list[CardDef], list[CardDef]]:
    main = [c for c in (db.get(n) for n in deck.main) if c is not None]
    res = [c for c in (db.get(n) for n in deck.resources) if c is not None]
    return main, res


def check_unknown(db: CardDB, deck: Deck) -> list[Violation]:
    unknown = sorted({n for n in (*deck.main, *deck.resources) if db.get(n) is None})
    return [
        Violation("UNKNOWN_ID", f"unknown card id {n!r} (not in the card data)", (n,))
        for n in unknown
    ]


def check_names(db: CardDB, deck: Deck) -> list[Violation]:
    out = []
    for e in deck.entries:
        cdef = db.get(e.card_number)
        if e.name is None or cdef is None or names_match(e.name, cdef.names):
            continue
        out.append(
            Violation(
                "NAME_MISMATCH",
                f"line {e.line}: {e.written} is {cdef.name!r} in the card data, not {e.name!r}",
                (e.card_number,),
            )
        )
    return out


def check_sizes(deck: Deck) -> list[Violation]:
    rules = construction_rules()
    out = []
    if len(deck.main) != rules.main_size:
        out.append(
            Violation(
                "MAIN_SIZE",
                f"the deck has {len(deck.main)} cards; it must have exactly {rules.main_size} "
                "(rule 6-1-1)",
                (),
            )
        )
    if len(deck.resources) != rules.resource_size:
        out.append(
            Violation(
                "RESOURCE_SIZE",
                f"the resource deck has {len(deck.resources)} cards; it must have exactly "
                f"{rules.resource_size} Resource cards (rule 6-1-1)",
                (),
            )
        )
    return out


def _type_message(cdef: CardDef, in_resource_deck: bool) -> str:
    kind = cdef.card_type.value
    if cdef.is_token:
        return (
            f"{cdef.card_number} {cdef.name} is a token ({kind}); tokens are prepared outside "
            "the game and are never part of a deck or resource deck (rule 6-1-2, FAQ Q7)"
        )
    if in_resource_deck:
        return (
            f"{cdef.card_number} {cdef.name} is a {kind} card; a resource deck is constructed "
            "with Resource cards only (rule 6-1-1-4)"
        )
    return (
        f"{cdef.card_number} {cdef.name} is a {kind} card; a deck is constructed with Unit, "
        "Pilot, Command, and Base cards only (rule 6-1-1-1)"
    )


def check_card_types(db: CardDB, deck: Deck) -> list[Violation]:
    rules = construction_rules()
    main, res = _known(db, deck)
    bad_main = {c.card_number: c for c in main if c.card_type.value not in rules.main_types}
    bad_res = {c.card_number: c for c in res if c.card_type.value not in rules.resource_types}
    out = [
        Violation("CARD_TYPE", _type_message(c, False), (n,)) for n, c in sorted(bad_main.items())
    ]
    out += [
        Violation("CARD_TYPE", _type_message(c, True), (n,)) for n, c in sorted(bad_res.items())
    ]
    return out


def check_copies(db: CardDB, deck: Deck) -> list[Violation]:
    limit = construction_rules().max_copies
    return [
        Violation(
            "COPY_LIMIT",
            f"{count} copies of {_label(db, n)}; at most {limit} copies of cards with the same "
            "card number (all printings count together) may be in a deck (rules 6-1-1-3, 2-1-2)",
            (n,),
        )
        for n, count in sorted(Counter(deck.main).items())
        if count > limit
    ]


def check_colors(db: CardDB, deck: Deck) -> list[Violation]:
    rules = construction_rules()
    main, _ = _known(db, deck)
    copies = Counter(c.color.value for c in main if c.color is not None)
    if not main or rules.min_colors <= len(copies) <= rules.max_colors:
        return []
    ranked = sorted(copies.items(), key=lambda kv: (-kv[1], kv[0]))
    extra = {color for color, _ in ranked[rules.max_colors :]}
    cards = sorted({c.card_number for c in main if c.color is not None and c.color.value in extra})
    shown = ", ".join(f"{color} x{n}" for color, n in ranked)
    return [
        Violation(
            "COLORS",
            f"the deck uses {len(copies)} colors ({shown}); a deck must be constructed entirely "
            f"using either one or two card colors (rule 6-1-1-2). Cards of the extra color(s): "
            f"{_labels(db, cards)}",
            tuple(cards),
        )
    ]


def check_banned(db: CardDB, deck: Deck) -> list[Violation]:
    bl = official.banlist()
    present = sorted(set(deck.main) & bl.banned)
    return [
        Violation(
            "BANNED",
            f"{_label(db, n)} is banned: no copies may be in a deck "
            f"(Banned & Restricted list effective {bl.effective_date})",
            (n,),
        )
        for n in present
    ]


def check_restricted(db: CardDB, deck: Deck) -> list[Violation]:
    bl = official.banlist()
    counts = Counter(deck.main)
    out = []
    for r in bl.restricted:
        if counts[r.card_number] <= r.max_copies:
            continue
        out.append(
            Violation(
                "RESTRICTED",
                f"{counts[r.card_number]} copies of {_label(db, r.card_number)}; it is restricted "
                f"to {r.max_copies} copies (Banned & Restricted list effective "
                f"{bl.effective_date}).{_exception_note(r.exceptions)}",
                (r.card_number,),
            )
        )
    return out


def check_banned_pairs(db: CardDB, deck: Deck) -> list[Violation]:
    bl = official.banlist()
    present = set(deck.main)
    return [
        Violation(
            "BANNED_PAIR",
            f"{_labels(db, p.cards)} are a banned pair and cannot be in the same deck "
            f"(Banned & Restricted list effective {bl.effective_date})",
            p.cards,
        )
        for p in bl.banned_pairs
        if set(p.cards) <= present
    ]


def _membership_note(rule: AttributePairRule, number: str, by_predicate: bool) -> str:
    listed = number in rule.enumerated
    if by_predicate and not listed:
        return f"{number} matches the description in the card data but is not on the official list"
    if listed and not by_predicate:
        return f"{number} is on the official list but does not match the description in the data"
    return ""


def attribute_members(db: CardDB, rule: AttributePairRule, numbers: Iterable[str]) -> list[str]:
    """Card numbers that belong to the rule by predicate over card data or by the official list."""
    out = []
    for n in sorted(set(numbers)):
        cdef = db.get(n)
        if n in rule.enumerated or (cdef is not None and matches(rule.member_predicate, cdef)):
            out.append(n)
    return out


def _attribute_violation(db: CardDB, deck: Deck, rule: AttributePairRule) -> Violation | None:
    members = attribute_members(db, rule, deck.main)
    total = sum(1 for n in deck.main if n in members)
    if len(members) < 2 and total <= rule.max_total_copies:
        return None
    notes = [
        note
        for n in members
        if (cdef := db.get(n)) is not None
        and (note := _membership_note(rule, n, matches(rule.member_predicate, cdef)))
    ]
    note_text = "".join(f" Note: {n}." for n in notes)
    return Violation(
        "ATTRIBUTE_PAIR",
        f'{_labels(db, members)} are each "{rule.description}"; every combination of two such '
        f"card numbers is a banned pair, so a deck may contain only one of them, at most "
        f"{rule.max_total_copies} copies (Banned & Restricted list effective "
        f"{official.banlist().effective_date}).{_exception_note(rule.exceptions)}{note_text}",
        tuple(members),
    )


def check_attribute_pairs(db: CardDB, deck: Deck) -> list[Violation]:
    found = (_attribute_violation(db, deck, r) for r in official.banlist().attribute_rules)
    return [v for v in found if v is not None]


def check_implemented(db: CardDB, deck: Deck) -> list[Violation]:
    reg = get_registry()
    out = []
    for n in sorted(set(deck.main) | set(deck.resources)):
        cdef = db.get(n)
        if cdef is None:
            continue
        entry = reg.cards[cdef.def_id]
        if entry.script is None:
            out.append(
                Violation(
                    "UNIMPLEMENTED",
                    f"{_label(db, n)} has no implemented effect in this simulator version, so "
                    f"games cannot use it ({entry.error})",
                    (n,),
                )
            )
    return out


def validate_deck(deck: Deck, *, banlist: bool = True, implemented: bool = True) -> list[Violation]:
    """Every legality problem of ``deck``; ``[]`` means legal. ``banlist=False`` checks the
    Comprehensive Rules only; ``implemented=False`` skips the simulator-support check."""
    db = get_card_db()
    out = [
        *check_unknown(db, deck),
        *check_names(db, deck),
        *check_sizes(deck),
        *check_card_types(db, deck),
        *check_copies(db, deck),
        *check_colors(db, deck),
    ]
    if banlist:
        out += [
            *check_banned(db, deck),
            *check_restricted(db, deck),
            *check_banned_pairs(db, deck),
            *check_attribute_pairs(db, deck),
        ]
    if implemented:
        out += check_implemented(db, deck)
    return out


def require_legal(deck: Deck) -> None:
    """Raise :class:`DeckError` listing every violation when ``deck`` is not legal."""
    violations = validate_deck(deck)
    if violations:
        raise DeckError(
            [f"[{v.code}] {v.message}" for v in violations],
            deck_name=deck.name,
            violations=violations,
        )
