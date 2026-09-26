"""Deck-construction rules and the Banned & Restricted list, loaded from the packaged official
data (``src/gcg_sim/data/official/{deck_construction,banlist}.json``)."""

from __future__ import annotations

import json
import re
from collections.abc import Mapping
from dataclasses import dataclass
from functools import cache
from typing import Any

from gcg_sim.cards.db import read_data_text
from gcg_sim.cards.model import CardDef

Predicate = Mapping[str, Any]


@dataclass(frozen=True, slots=True)
class ConstructionRules:
    main_size: int
    resource_size: int
    max_copies: int
    main_types: frozenset[str]
    resource_types: frozenset[str]
    min_colors: int
    max_colors: int
    effective_date: str


@dataclass(frozen=True, slots=True)
class RuleException:
    id: str
    description: str


@dataclass(frozen=True, slots=True)
class Restricted:
    card_number: str
    max_copies: int
    exceptions: tuple[RuleException, ...]


@dataclass(frozen=True, slots=True)
class BannedPair:
    id: str
    cards: tuple[str, str]


@dataclass(frozen=True, slots=True)
class AttributePairRule:
    id: str
    description: str
    member_predicate: Predicate
    enumerated: frozenset[str]
    max_total_copies: int
    exceptions: tuple[RuleException, ...]


@dataclass(frozen=True, slots=True)
class Banlist:
    effective_date: str
    banned: frozenset[str]
    restricted: tuple[Restricted, ...]
    banned_pairs: tuple[BannedPair, ...]
    attribute_rules: tuple[AttributePairRule, ...]


def _official(name: str) -> dict[str, Any]:
    data: dict[str, Any] = json.loads(read_data_text("official", name))
    return data


@cache
def construction_rules() -> ConstructionRules:
    d = _official("deck_construction.json")
    return ConstructionRules(
        main_size=int(d["main_deck"]["size"]),
        resource_size=int(d["resource_deck"]["size"]),
        max_copies=int(d["main_deck"]["max_copies_per_card_number"]),
        main_types=frozenset(d["main_deck"]["allowed_card_types"]),
        resource_types=frozenset(d["resource_deck"]["allowed_card_types"]),
        min_colors=int(d["colors"]["min_distinct_colors"]),
        max_colors=int(d["colors"]["max_distinct_colors"]),
        effective_date=str(d["effective_date"]),
    )


def _exceptions(raw: list[dict[str, Any]]) -> tuple[RuleException, ...]:
    return tuple(RuleException(str(e["id"]), str(e["description"])) for e in raw)


_QUOTED = re.compile(r"\"([^\"]+)\"")


def _rule_description(rule: Mapping[str, Any]) -> str:
    m = _QUOTED.search(str(rule.get("source_text", "")))
    return m.group(1) if m else str(rule["id"])


def _attribute_rule(r: Mapping[str, Any]) -> AttributePairRule:
    return AttributePairRule(
        id=str(r["id"]),
        description=_rule_description(r),
        member_predicate=r["member_predicate"],
        enumerated=frozenset(m["card_number"] for m in r["enumerated_members"]),
        max_total_copies=int(r["max_total_copies_of_matching_cards"]),
        exceptions=_exceptions(r.get("exceptions", [])),
    )


@cache
def banlist() -> Banlist:
    d = _official("banlist.json")
    return Banlist(
        effective_date=str(d["effective_date"]),
        banned=frozenset(b["card_number"] for b in d["banned"]),
        restricted=tuple(
            Restricted(str(r["card_number"]), int(r["max_copies"]), _exceptions(r["exceptions"]))
            for r in d["restricted"]
        ),
        banned_pairs=tuple(
            BannedPair(str(p["id"]), (str(p["cards"][0]), str(p["cards"][1])))
            for p in d["banned_pairs"]
        ),
        attribute_rules=tuple(_attribute_rule(r) for r in d["attribute_pair_rules"]),
    )


def has_effect(card: CardDef) -> bool:
    """Effect text other than empty or "-" counts as an effect (keywords included)."""
    return not card.is_vanilla


def _leaf(key: str, value: Any, card: CardDef, matched: str | None) -> bool:
    match key:
        case "card_type":
            return bool(card.card_type.value == value)
        case "level":
            return bool(card.level == value)
        case "cost":
            return bool(card.cost == value)
        case "ap":
            return bool(card.ap == value)
        case "hp":
            return bool(card.hp == value)
        case "has_effect":
            return bool(has_effect(card) == value)
        case "color":
            return card.color is not None and card.color.value == value
        case "trait":
            return value in card.traits
        case "name_contains":
            return str(value) in card.name
        case "card_number":
            return card.card_number == (matched if value == "$matched" else value)
    raise ValueError(f"unknown banlist predicate operator {key!r}")


def matches(pred: Predicate, card: CardDef, matched: str | None = None) -> bool:
    """Evaluate a banlist.json card predicate (``predicate_language``) against one card."""
    for key, value in pred.items():
        if key == "all_of":
            ok = all(matches(p, card, matched) for p in value)
        elif key == "any_of":
            ok = any(matches(p, card, matched) for p in value)
        elif key == "not":
            ok = not matches(value, card, matched)
        else:
            ok = _leaf(key, value, card, matched)
        if not ok:
            return False
    return True
