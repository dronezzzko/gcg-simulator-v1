"""Compile normalized card text into the typed effect DSL.

The compiler recognises ability markers (rule 13-2 keywords), keyword effects (13-1), and a
library of sentence templates. Any fragment it cannot recognise raises :class:`CompileError`;
such cards need an explicit binding (``effects.bindings``).
"""

from __future__ import annotations

from gcg_sim.cards.model import CardDef, CardType
from gcg_sim.effects import dsl as d
from gcg_sim.effects.compiler.abilities import CompileError, compile_ability
from gcg_sim.effects.registry import UnimplementedCardError
from gcg_sim.effects.text import normalize, split_abilities

REMINDER_ONLY_TYPES = frozenset({CardType.RESOURCE, CardType.EX_RESOURCE, CardType.EX_BASE})


def compile_parts(cdef: CardDef) -> list[tuple[str, list[d.Ability] | CompileError]]:
    """Compile each ability line independently; failures are returned, not raised."""
    out: list[tuple[str, list[d.Ability] | CompileError]] = []
    if cdef.card_type in REMINDER_ONLY_TYPES:
        return out
    for raw in split_abilities(normalize(cdef.effect)):
        try:
            out.append((raw.line, compile_ability(raw, cdef)))
        except CompileError as exc:
            out.append((raw.line, exc))
    return out


def route_abilities(
    cdef: CardDef, abilities: list[d.Ability]
) -> tuple[tuple[d.Ability, ...], tuple[d.Ability, ...]]:
    """Split a Pilot card's abilities into its own text and text gained by the paired Unit
    (rules 2-11-3, 3-3-9): 【Burst】 and location-specific effects stay with the Pilot card."""
    if cdef.card_type is not CardType.PILOT:
        return tuple(abilities), ()
    own: list[d.Ability] = []
    unit: list[d.Ability] = []
    for a in abilities:
        where = getattr(a, "where", d.Where.FIELD)
        if isinstance(a, d.Burst) or where in (d.Where.HAND, d.Where.TRASH):
            own.append(a)
        else:
            unit.append(a)
    return tuple(own), tuple(unit)


def compile_card(cdef: CardDef) -> d.CardScript:
    parts = compile_parts(cdef)
    if not parts:
        return d.CardScript(cdef.card_number, source="vanilla")
    abilities: list[d.Ability] = []
    errors: list[str] = []
    for line, res in parts:
        if isinstance(res, CompileError):
            errors.append(f"{line!r}: {res}")
        else:
            abilities.extend(res)
    if errors:
        raise UnimplementedCardError("; ".join(errors))
    own, unit = route_abilities(cdef, abilities)
    return d.CardScript(cdef.card_number, abilities=own, unit_abilities=unit, source="compiled")
