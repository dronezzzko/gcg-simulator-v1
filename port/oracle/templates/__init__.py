"""General compiler templates for wordings the Python compiler cannot compile (W-63, W-67).

docs/port/PLAN.md §12.11: the templates live outside the Python package and import its DSL and
compiler read-only. A template recognises one general wording at the end of an ability line,
compiles the rest of the line with the unchanged compiler (``compiler.abilities.
compile_ability``), and appends the steps the wording means. ``compile_card`` runs the compiler
on every line, uses a template on a line only when the template's wording occurs in it, and
routes the abilities like ``compiler.compile_card`` (``compiler/__init__.py:52-67``).

``port/oracle/templates/check.py`` runs the templates on every binding card and reports, per
template, the bindings whose script it reproduces byte for byte (``repr`` of the abilities and
the Unit abilities). Retiring the reproduced bindings and moving the templates into
``effects/compiler/`` is a post-acceptance step (§12.11, answer (a) of A32 item 4).
"""

from __future__ import annotations

import dataclasses
import re
from collections.abc import Callable, Sequence
from dataclasses import dataclass
from typing import Any, cast

from gcg_sim.cards.model import CardDef
from gcg_sim.effects import dsl as d
from gcg_sim.effects.compiler import REMINDER_ONLY_TYPES, route_abilities
from gcg_sim.effects.compiler.abilities import CompileError, compile_ability
from gcg_sim.effects.text import RawAbility, normalize, split_abilities

StepsBuilder = Callable[[re.Match[str], tuple[d.Step, ...]], tuple[d.Step, ...]]


@dataclass(frozen=True)
class Template:
    """A trailing sentence of an ability line and the steps it appends."""

    id: str
    family: str
    entry: str
    pattern: re.Pattern[str]
    build: StepsBuilder


def _last_chosen(steps: Sequence[d.Step]) -> str:
    for step in reversed(steps):
        if isinstance(step, d.Choose):
            return step.var
    raise CompileError("the wording refers to a chosen card, but the line chooses none")


def _traits(text: str) -> tuple[str, ...]:
    return tuple(t.strip() for t in re.findall(r"\(([^()]+)\)", text))


def _pair_from_trash(m: re.Match[str], prior: tuple[d.Step, ...]) -> tuple[d.Step, ...]:
    """'After activating this card's 【Main】, you may pair this card from your trash with one of
    your (Trait) Units.' The resolving Command goes to the trash first (rule 10-1-8-2), then,
    if it is there, it may be paired with a chosen Unit that can take it (no cost is paid)."""
    unit_filters: tuple[d.Filter, ...] = (
        d.IsKind((d.CardKind.UNIT,)),
        *(d.HasTrait((t,)) for t in _traits(m.group("units"))),
        d.CustomFilter("wp_gd05_b_can_pair"),
    )
    in_trash = d.Exists(d.Sel(d.Side.FRIENDLY, d.Loc.TRASH, (d.IsRef(d.ThisCard()),)))
    choose = d.Choose(
        "pair_with",
        d.Sel(d.Side.FRIENDLY, d.Loc.BATTLE, unit_filters),
        optional=True,
        targeting=False,
    )
    return (
        d.CustomStep("wp_gd05_b_resolving_command_to_trash"),
        d.If(in_trash, (choose, d.Pair(d.ThisCard(), d.Var("pair_with")))),
    )


def _reduce_received_damage(m: re.Match[str], prior: tuple[d.Step, ...]) -> tuple[d.Step, ...]:
    """'During this turn, when it receives enemy [battle ]damage, reduce it by N.' A lasting
    reduction of the damage the chosen Unit receives from enemy sources (rule 10-1-9 does not
    apply: nothing is substituted)."""
    kind = d.DamageKind.BATTLE if m.group("battle") else d.DamageKind.ANY
    rule = d.RuleMod(
        d.RuleKind.REDUCE_DAMAGE,
        amount=int(m.group("amount")),
        damage_kind=kind,
        source_side=d.Side.ENEMY,
    )
    return (d.Apply(d.Var(_last_chosen(prior)), d.RuleGrant(rule)),)


TEMPLATES: tuple[Template, ...] = (
    Template(
        id="pair-from-trash-after-main",
        family="'After activating this card's 【Main】, you may pair this card from your trash'",
        entry="W-67",
        pattern=re.compile(
            r"\s*After activating this card's 【Main】, you may pair this card from your trash "
            r"with one of your (?P<units>(?:\([^()]+\) )*)Units\.$"
        ),
        build=_pair_from_trash,
    ),
    Template(
        id="reduce-received-damage-this-turn",
        family="conditional damage reduction ('when it receives enemy damage, reduce it by N')",
        entry="W-63",
        pattern=re.compile(
            r"\s*During this turn, when it receives enemy (?P<battle>battle )?damage, reduce it "
            r"by (?P<amount>\d+)\.$"
        ),
        build=_reduce_received_damage,
    ),
)


def _with_steps(ab: d.Ability, extra: tuple[d.Step, ...]) -> d.Ability:
    steps: tuple[d.Step, ...] = getattr(ab, "steps", ())
    return cast("d.Ability", dataclasses.replace(cast("Any", ab), steps=(*steps, *extra)))


def compile_line(raw: RawAbility, cdef: CardDef) -> tuple[list[d.Ability], str | None]:
    """The abilities of one line and the template id used, if any (``None``: the compiler's
    own result). Raises ``CompileError`` when neither the compiler nor a template applies."""
    for t in TEMPLATES:
        m = t.pattern.search(raw.body)
        if m is None:
            continue
        head = raw.body[: m.start()].strip()
        prefix = RawAbility(raw.markers, head, raw.line[: len(raw.line) - len(m.group(0))])
        abilities = compile_ability(prefix, cdef)
        if len(abilities) != 1 or not hasattr(abilities[0], "steps"):
            raise CompileError(f"template {t.id}: the line before it is not one ability")
        prior: tuple[d.Step, ...] = getattr(abilities[0], "steps", ())
        return [_with_steps(abilities[0], t.build(m, prior))], t.id
    return compile_ability(raw, cdef), None


def compile_card(cdef: CardDef) -> tuple[d.CardScript, tuple[str, ...]]:
    """``compiler.compile_card`` with the templates: the card's script and the template ids
    it used. Raises ``CompileError`` for a line nothing compiles."""
    if cdef.card_type in REMINDER_ONLY_TYPES:
        return d.CardScript(cdef.card_number, source="vanilla"), ()
    abilities: list[d.Ability] = []
    used: list[str] = []
    for raw in split_abilities(normalize(cdef.effect)):
        got, template = compile_line(raw, cdef)
        abilities.extend(got)
        if template is not None:
            used.append(template)
    own, unit = route_abilities(cdef, abilities)
    return d.CardScript(cdef.card_number, own, unit, source="template"), tuple(used)


def behaviour_repr(script: d.CardScript) -> str:
    """The part of a script that decides behaviour: its abilities and Unit abilities. The
    ``source`` and ``notes`` fields record provenance only."""
    return repr((script.card_number, script.abilities, script.unit_abilities))
