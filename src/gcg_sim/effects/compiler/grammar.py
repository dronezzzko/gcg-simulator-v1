"""Sentence grammar (stub during engine bring-up)."""

from __future__ import annotations

from gcg_sim.cards.model import CardDef
from gcg_sim.effects import dsl as d
from gcg_sim.effects.compiler.abilities import CompileError


def compile_marked(markers: list[str], body: str, cdef: CardDef) -> list[d.Ability]:
    if markers == ["Burst"] and body == "Add this card to your hand.":
        return [d.Burst((d.AddToHand(d.ThisCard()),))]
    if markers == ["Burst"] and body == "Deploy this card.":
        return [d.Burst((d.DeployCard(d.ThisCard()),))]
    if markers == ["Deploy"] and body == "Add 1 of your Shields to your hand.":
        return [d.Triggered(d.Trigger(d.Ev.DEPLOYED), (d.ShieldToHand(1),))]
    raise CompileError("no template")
