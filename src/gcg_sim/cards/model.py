"""Normalized card definitions (rules section 2 and 3)."""

from __future__ import annotations

import re
from dataclasses import dataclass, field
from enum import StrEnum


class CardType(StrEnum):
    UNIT = "UNIT"
    PILOT = "PILOT"
    COMMAND = "COMMAND"
    BASE = "BASE"
    RESOURCE = "RESOURCE"
    UNIT_TOKEN = "UNIT TOKEN"
    EX_BASE = "EX BASE"
    EX_RESOURCE = "EX RESOURCE"

    @property
    def is_token(self) -> bool:
        return self in (CardType.UNIT_TOKEN, CardType.EX_BASE, CardType.EX_RESOURCE)

    @property
    def is_unit(self) -> bool:
        return self in (CardType.UNIT, CardType.UNIT_TOKEN)

    @property
    def is_base(self) -> bool:
        return self in (CardType.BASE, CardType.EX_BASE)

    @property
    def is_resource(self) -> bool:
        return self in (CardType.RESOURCE, CardType.EX_RESOURCE)


MAIN_DECK_TYPES = frozenset({CardType.UNIT, CardType.PILOT, CardType.COMMAND, CardType.BASE})


class Color(StrEnum):
    BLUE = "Blue"
    GREEN = "Green"
    RED = "Red"
    WHITE = "White"
    PURPLE = "Purple"


@dataclass(frozen=True, slots=True)
class LinkCondition:
    """A Unit's link condition (rules 2-12, 3-2-6): any listed name portion or trait satisfies it."""

    name_parts: tuple[str, ...] = ()
    traits: tuple[str, ...] = ()

    def is_empty(self) -> bool:
        return not self.name_parts and not self.traits


_LINK_NAME = re.compile(r"\[([^\]]+)\]")
_LINK_TRAIT = re.compile(r"\(([^)]+)\)")


def parse_link(raw: str | None) -> LinkCondition | None:
    """Parse gcg-api ``link`` strings such as ``[Amuro Ray]``, ``(Zeon) Trait``,
    ``[A] / [B]``, ``(X) Trait / [B]`` and ``(X) / (Y) Trait``.
    """
    if raw is None or raw.strip() in ("", "-"):
        return None
    names = tuple(m.strip() for m in _LINK_NAME.findall(raw))
    without_names = _LINK_NAME.sub("", raw)
    traits = tuple(m.strip() for m in _LINK_TRAIT.findall(without_names))
    if not names and not traits:
        raise ValueError(f"unparseable link condition: {raw!r}")
    return LinkCondition(name_parts=names, traits=traits)


def parse_modifier(raw: str | None) -> int:
    """Pilot AP/HP modifiers are printed as ``+1``; absent values mean +0."""
    if raw is None or raw.strip() in ("", "-"):
        return 0
    return int(raw.strip().replace("+", ""))


_PILOT_EFFECT = re.compile(r"【Pilot】\s*\[([^\]]+)\]")


def parse_command_pilot_name(effect: str) -> str | None:
    m = _PILOT_EFFECT.search(effect)
    return m.group(1).strip() if m else None


@dataclass(frozen=True, slots=True)
class CardDef:
    """One card number's canonical, override-applied gameplay definition."""

    def_id: int
    card_number: str
    name: str
    card_type: CardType
    color: Color | None
    level: int
    cost: int
    ap: int
    hp: int
    zones: tuple[str, ...]
    traits: tuple[str, ...]
    link: LinkCondition | None
    effect: str
    pilot_name: str | None
    set_code: str
    rarity: str
    product_ids: tuple[str, ...]
    canonical_product_id: str
    source_title: str | None = None
    extra_names: tuple[str, ...] = field(default=())

    @property
    def names(self) -> tuple[str, ...]:
        """All card names (rule 2-2-4). A Command with a 【Pilot】 effect also carries its pilot name."""
        out = (self.name, *self.extra_names)
        if self.pilot_name and self.pilot_name not in out:
            out = (*out, self.pilot_name)
        return out

    @property
    def is_token(self) -> bool:
        return self.card_type.is_token

    @property
    def is_unit_card(self) -> bool:
        return self.card_type.is_unit

    @property
    def is_pilot_capable(self) -> bool:
        return self.card_type is CardType.PILOT or (
            self.card_type is CardType.COMMAND and self.pilot_name is not None
        )

    @property
    def is_vanilla(self) -> bool:
        return self.effect.strip() in ("", "-")
