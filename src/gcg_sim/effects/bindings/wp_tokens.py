"""WP-TOKENS: Unit token cards (T-001..T-029) whose text the compiler cannot parse.

Tokens created by inline definitions such as ``[Ad Balloon]((Civilian)･AP0･HP1･...)`` resolve to
these T- cards (``CardDB.token_for``), so the restrictions below are the ones deployed tokens have.
"""

from __future__ import annotations

from gcg_sim.cards.model import CardDef
from gcg_sim.effects import dsl as d
from gcg_sim.effects.bindings import card


def _restrictions(*kinds: d.RuleKind) -> d.Constant:
    return d.Constant(effects=tuple(d.RuleGrant(d.RuleMod(k)) for k in kinds))


@card("T-014")
def t_014(c: CardDef) -> d.CardScript:
    """Ad Balloon: "This Unit can't be set as active or paired with a Pilot." """
    return d.CardScript(
        c.card_number,
        abilities=(_restrictions(d.RuleKind.CANT_BE_SET_ACTIVE, d.RuleKind.CANT_BE_PAIRED),),
        source="binding",
        notes="compound restriction split into two rule grants (rules 7-2-3-1, 5-9, 10-1-5-6)",
    )


@card("T-029")
def t_029(c: CardDef) -> d.CardScript:
    """Bit / Funnel: "This Unit can't be paired with a Pilot or attack." """
    return d.CardScript(
        c.card_number,
        abilities=(_restrictions(d.RuleKind.CANT_BE_PAIRED, d.RuleKind.CANT_ATTACK),),
        source="binding",
        notes="compound restriction split into two rule grants (rules 5-9, 8-2-1, 10-1-5-6)",
    )
