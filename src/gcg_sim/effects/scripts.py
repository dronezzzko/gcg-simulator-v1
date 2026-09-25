"""Resolve a card's script: explicit binding first, then the text compiler."""

from __future__ import annotations

from gcg_sim.cards.model import CardDef
from gcg_sim.effects.bindings import load_bindings
from gcg_sim.effects.compiler import compile_card
from gcg_sim.effects.dsl import CardScript


def script_for(cdef: CardDef) -> CardScript:
    bound = load_bindings().get(cdef.card_number)
    if bound is not None:
        return bound[1](cdef)
    return compile_card(cdef)


def binding_module(card_number: str) -> str | None:
    bound = load_bindings().get(card_number)
    return bound[0] if bound else None
