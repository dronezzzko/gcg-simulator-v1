"""Ability-level compilation: markers, keywords, triggers, activations, commands, constants."""

from __future__ import annotations

import re

from gcg_sim.cards.model import CardDef
from gcg_sim.effects import dsl as d
from gcg_sim.effects.text import RawAbility


class CompileError(Exception):
    pass


_KW = re.compile(
    r"<(Repair|Breach|Support|Blocker|First Strike|High-Maneuver|Suppression)(?:\s+(\d+))?>"
)


def parse_keyword_line(body: str) -> list[d.Keyword] | None:
    """A line consisting only of keyword effects, e.g. ``<Blocker>`` or ``<Repair 2>``."""
    s = body.strip().rstrip(".")
    if not s or _KW.sub("", s).strip():
        return None
    out = []
    for m in _KW.finditer(s):
        out.append(d.Keyword(d.Kw(m.group(1)), int(m.group(2) or 0)))
    return out


def compile_ability(raw: RawAbility, cdef: CardDef) -> list[d.Ability]:
    from gcg_sim.effects.compiler.grammar import compile_marked

    markers = list(raw.markers)
    if markers and markers[0] == "Pilot":
        return []  # 【Pilot】[Name]: pilot name is card data (rule 3-4-6-1)
    if not markers:
        kws = parse_keyword_line(raw.body)
        if kws is not None:
            return list(kws)
    return compile_marked(markers, raw.body, cdef)
