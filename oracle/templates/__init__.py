"""General compiler templates for wordings the Python compiler cannot compile (W-63, W-67).

docs/dev/PLAN.md §12.11: the templates live outside the Python package and use its DSL and
compiler without editing them. ``compile_card`` compiles a card the way
``compiler.compile_card`` does (``compiler/__init__.py:52-67``), with four kinds of templates:

- a trailing template (``Template``) recognises one general wording at the end of an ability
  line, compiles the rest of the line with the unchanged compiler
  (``compiler.abilities.compile_ability``) and appends the steps the wording means;
- a table template (``TableTemplate``) is one more entry of the compiler's own sentence,
  condition or trigger table (``grammar._CORE``, ``grammar._CONDS``, ``grammar._TRIGGERS``),
  in the form those tables hold: an anchored pattern and a function of the match and the
  compile context. While ``compile_card`` runs, the table templates are put in front of the
  compiler's entries, and the tables are restored when it returns (``_template_tables``), so
  the compiler outside ``compile_card`` is never affected. A table template is exactly what
  the post-acceptance step adds to ``effects/compiler/grammar.py``;
- a line template (``LineTemplate``) matches a whole ability body, rewrites it to a wording the
  compiler compiles, and patches the compiled abilities: it adds what the rewritten wording
  drops (a condition), or replaces the steps of the frame that the compiler builds from the
  line's markers.

``oracle/templates/check.py`` runs ``compile_card`` on every binding card and every
compiled card. A binding card is reproduced when its script equals the binding's byte for byte
(``repr`` of the abilities and the Unit abilities); a compiled card must never change. Retiring
the reproduced bindings and moving the templates into ``effects/compiler/`` is a
post-acceptance step (§12.11, answer (a) of A32 item 4).
"""

from __future__ import annotations

import contextlib
import dataclasses
import re
from collections.abc import Callable, Iterator, Sequence
from dataclasses import dataclass
from typing import Any, cast

from gcg_sim.cards.model import CardDef
from gcg_sim.cards.tokens import parse_token_specs
from gcg_sim.effects import dsl as d
from gcg_sim.effects.compiler import REMINDER_ONLY_TYPES, grammar, route_abilities
from gcg_sim.effects.compiler.abilities import CompileError, compile_ability
from gcg_sim.effects.compiler.nouns import cmp_op, parse_selector, traits_of
from gcg_sim.effects.text import RawAbility, normalize, split_abilities
from gcg_sim.engine.types import Duration

StepsBuilder = Callable[[re.Match[str], tuple[d.Step, ...]], tuple[d.Step, ...]]


@dataclass(frozen=True)
class Template:
    """A trailing sentence of an ability line and the steps it appends."""

    id: str
    family: str
    entry: str
    pattern: re.Pattern[str]
    build: StepsBuilder


@dataclass(frozen=True)
class TableTemplate:
    """One entry of a compiler table: ``table`` is ``steps`` (``grammar._CORE``),
    ``conditions`` (``grammar._CONDS``) or ``triggers`` (``grammar._TRIGGERS``); ``rx`` is what
    the compiler matches (anchored, like the table's own entries) and ``build`` returns what the
    table's functions return. ``pattern`` is the wording as a card's text writes it."""

    id: str
    family: str
    entry: str
    pattern: re.Pattern[str]
    table: str
    rx: re.Pattern[str]
    build: Callable[[re.Match[str], grammar.G], Any]


@dataclass(frozen=True)
class LineTemplate:
    """A whole ability body (``pattern`` matches all of it): ``rewrite`` gives the body the
    compiler compiles, ``patch`` the final abilities from the compiled ones. ``wording`` is the
    wording as a card's text writes it."""

    id: str
    family: str
    entry: str
    pattern: re.Pattern[str]
    wording: re.Pattern[str]
    rewrite: Callable[[re.Match[str]], str]
    patch: Callable[[re.Match[str], list[d.Ability], CardDef], list[d.Ability]]


@dataclass(frozen=True)
class SuffixTemplate:
    """A selector suffix (``grammar.parse_selector``'s ``_POST`` table, ``nouns.py:84-140``):
    the phrase before ``suffix`` is parsed by the compiler, and ``filters`` are appended to its
    selector. ``pattern`` is the wording as a card's text writes it."""

    id: str
    family: str
    entry: str
    pattern: re.Pattern[str]
    suffix: re.Pattern[str]
    filters: Callable[[re.Match[str]], tuple[d.Filter, ...]]


AnyTemplate = Template | TableTemplate | LineTemplate | SuffixTemplate


def wording(t: AnyTemplate) -> re.Pattern[str]:
    """The regex that finds a template's wording in a line of a card's text."""
    return t.wording if isinstance(t, LineTemplate) else t.pattern


def kind(t: AnyTemplate) -> str:
    if isinstance(t, LineTemplate):
        return "line"
    if isinstance(t, TableTemplate):
        return f"table:{t.table}"
    if isinstance(t, SuffixTemplate):
        return "selector-suffix"
    return "trailing"


TABLES = {"steps": "_CORE", "conditions": "_CONDS", "triggers": "_TRIGGERS"}
# The name under which ``grammar`` calls the selector parser (imported from ``nouns``).
SELECTOR_PARSER = "parse_selector"


def _anchored(pattern: str) -> re.Pattern[str]:
    """A table pattern as ``grammar.core`` compiles one (``grammar.py:992-998``)."""
    return re.compile("^(?:" + pattern + ")$", re.I)


# ---------------------------------------------------------------------------------------------
# trailing templates


def _last_chosen(steps: Sequence[d.Step]) -> str:
    for step in reversed(steps):
        if isinstance(step, d.Choose):
            return step.var
    raise CompileError("the wording refers to a chosen card, but the line chooses none")


def _pair_from_trash(m: re.Match[str], prior: tuple[d.Step, ...]) -> tuple[d.Step, ...]:
    """'After activating this card's 【Main】, you may pair this card from your trash with one of
    your (Trait) Units.' The resolving Command goes to the trash first (rule 10-1-8-2), then,
    if it is there, it may be paired with a chosen Unit that can take it (no cost is paid)."""
    unit_filters: tuple[d.Filter, ...] = (
        d.IsKind((d.CardKind.UNIT,)),
        *(d.HasTrait((t,)) for t in traits_of(m.group("units"))),
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


# ---------------------------------------------------------------------------------------------
# table templates: sentences


def _mill(m: re.Match[str], g: grammar.G) -> list[d.Step]:
    """'Place the top N cards of your deck into your trash.' The compiler's own ``_mill``
    (``grammar.py:1324-1330``), which the generic "place (.+?) into ... trash" entry earlier in
    the table shadows: the milled cards become 'it' (``Var('milled')``)."""
    return grammar._mill(m, g)


def _cant_activate_blocker(m: re.Match[str], g: grammar.G) -> list[d.Step]:
    """'It can't activate <Blocker> during this turn.' <Blocker> is activated to block (rules
    8-3-1, 13-1-4-1), so the referenced Unit gets the CANT_BLOCK rule for the turn."""
    steps, ref = grammar.ref_phrase(m.group(1), g)
    rule = d.RuleGrant(d.RuleMod(d.RuleKind.CANT_BLOCK))
    return [*steps, d.Apply(ref, rule, Duration.THIS_TURN)]


def _exile_this_from_trash(m: re.Match[str], g: grammar.G) -> list[d.Step]:
    """'Exile this card in your trash from the game.' The card is exiled only if it is in the
    trash when the step resolves (a cost-like choice of exactly this card there)."""
    sel = d.Sel(d.Side.FRIENDLY, d.Loc.TRASH, (d.IsRef(d.ThisCard()),))
    return [
        d.Choose("self", sel, 1, targeting=False, after_then=g.after_then),
        d.Exile(d.Var("self")),
    ]


def _opponent_each_chooses(m: re.Match[str], g: grammar.G) -> list[d.Step]:
    """'All enemy players each choose N of their <cards>.' (``_multiplayer`` rewrites it to 'your
    opponent choose ...', which ``_opp_choose`` misses.) The opponent chooses their own cards,
    a targeting choice as the family's bindings model it (``_opp_choose`` gives a non-targeting
    one)."""
    phrase = re.sub(r"^(\d+|one) of their (?:own )?", r"\1 enemy ", m.group(1))
    spec = parse_selector(phrase)
    v = g.var()
    g.it = d.Var(v)
    return [
        d.Choose(
            v,
            spec.sel,
            spec.count,
            spec.min_count,
            targeting=True,
            chooser=d.P.OPP,
            after_then=g.after_then,
        )
    ]


def _all_players_choose_resources(m: re.Match[str], g: grammar.G) -> list[d.Step]:
    """'All players each choose N of their Resources.' Each player chooses from their own
    resource area, any Resource, EX Resources included (rulings Q322, Q323), you first: the
    active player's order of ruling Q321 when the effect resolves on your turn, as on
    EB01-059's 【Attack】. 'them' is both choices."""
    n = int(m.group(1))
    own = d.Sel(d.Side.FRIENDLY, d.Loc.RESOURCE_AREA, ())
    other = d.Sel(d.Side.ENEMY, d.Loc.RESOURCE_AREA, ())
    g.it = d.Union((d.Var("r0"), d.Var("r1")))
    return [
        d.Choose("r0", own, n, after_then=g.after_then),
        d.Choose("r1", other, n, chooser=d.P.OPP, after_then=g.after_then),
    ]


def _they_may_draw(m: re.Match[str], g: grammar.G) -> list[d.Step]:
    """'Choose 1 enemy player. They may draw N.' The chosen player (the opponent in a two-player
    game) decides."""
    return [d.May((d.Draw(int(m.group(1)), player=d.P.OPP),), player=d.P.OPP)]


def _if_they_drew(m: re.Match[str], g: grammar.G) -> list[d.Step]:
    """'If they draw with this effect, <effect>.' The preceding 'They may draw' was taken."""
    inner: list[d.Step] = []
    grammar.compile_sentence(grammar._cap(m.group(1)), g, inner)
    return [d.IfYouDo(tuple(inner))]


def _token_key(tdef: str) -> str:
    """The token key of one inline token definition (``grammar._deploy_token``'s ``key``)."""
    specs = parse_token_specs(tdef)
    if len(specs) != 1:
        raise CompileError("token definition")
    return specs[0].key


def _deploy_either_token(m: re.Match[str], g: grammar.G) -> list[d.Step]:
    """'Deploy 1 [A](...) or 1 [B](...) Unit token.' Two inline token definitions, one of which
    is deployed: a choice of mode named by the tokens' names."""
    first, second = m.group(1), m.group(2)
    g.it = d.Var("tokens")
    return [
        d.ChooseMode(
            (
                (_token_name(first), (d.DeployToken(_token_key(first), 1),)),
                (_token_name(second), (d.DeployToken(_token_key(second), 1),)),
            )
        )
    ]


def _token_name(tdef: str) -> str:
    """The name of an inline token definition: the text in its brackets."""
    return tdef[1 : tdef.index("]")]


# ---------------------------------------------------------------------------------------------
# table templates: conditions


def _placed_trait_card(m: re.Match[str], g: grammar.G) -> d.Cond:
    """'If you placed a (Trait) card with this effect': a milled card with the trait is in the
    trash."""
    filters = (d.IsRef(d.Var("milled")), d.HasTrait(traits_of(m.group(1))))
    return d.Exists(d.Sel(d.Side.FRIENDLY, d.Loc.TRASH, filters))


def _placed_card_level(m: re.Match[str], g: grammar.G) -> d.Cond:
    """'If you placed a card that is Lv.N or higher with this effect': the milled card's Lv."""
    stat = d.StatCmp(d.Stat.LV, cmp_op(m.group(2)), int(m.group(1)))
    return d.RefMatches(d.Var("milled"), (stat,))


# ---------------------------------------------------------------------------------------------
# table templates: triggers (the compiler's own trigger, plus the antecedent of 'it')


def _compiler_trigger(text: str, g: grammar.G) -> d.Trigger:
    """The trigger the compiler's own table gives ``text`` (template entries skipped)."""
    for rx, fn in grammar._TRIGGERS:
        if getattr(fn, "template_id", None) is not None:
            continue
        mm = rx.match(text)
        if mm:
            return fn(mm, g)
    raise CompileError(f"unknown trigger {text!r}")


def _links_subject_is_it(m: re.Match[str], g: grammar.G) -> d.Trigger:
    """'When a friendly (Trait) Unit links, it ...': 'it' is the Unit that linked."""
    trig = _compiler_trigger(m.string, g)
    g.it = d.EventCard("subject")
    return trig


def _rested_self_is_it(m: re.Match[str], g: grammar.G) -> d.Trigger:
    """'When this Unit is rested by an effect, set it as active': 'it' is this Unit."""
    trig = _compiler_trigger(m.string, g)
    g.it = d.This()
    return trig


# ---------------------------------------------------------------------------------------------
# line templates


def _with_ex_used(m: re.Match[str], abilities: list[d.Ability], cdef: CardDef) -> list[d.Ability]:
    """'When you play and activate a (Trait) Command card using an EX Resource': the Command
    trigger, on condition that the play used an EX Resource (the event's ``ex_used`` amount).
    The subject is a Command card with the qualifier: ``_tr_command`` keeps only the
    qualifier's filters when there is one (``grammar.py:884-889``), so the kind is put back
    first."""
    out: list[d.Ability] = []
    command = d.IsKind((d.CardKind.COMMAND,))
    for ab in abilities:
        if not isinstance(ab, d.Triggered) or ab.trigger.event is not d.Ev.COMMAND_PLAYED:
            raise CompileError("the rewritten line is not a Command trigger")
        if ab.cond != d.TRUE or ab.trigger.subject is None:
            raise CompileError("the rewritten line already has a condition")
        sel = ab.trigger.subject
        filters = (command, *(f for f in sel.filters if f != command))
        trig = dataclasses.replace(ab.trigger, subject=dataclasses.replace(sel, filters=filters))
        used = d.Cmp(d.EventAmount("ex_used"), d.Op.GE, 1)
        out.append(dataclasses.replace(ab, trigger=trig, cond=used))
    return out


LINE_TEMPLATE_BODY = "Draw 1."


def _frame_steps(abilities: list[d.Ability]) -> d.Ability:
    """The one ability the compiler built for ``LINE_TEMPLATE_BODY``, checked."""
    if len(abilities) != 1 or getattr(abilities[0], "steps", None) != (d.Draw(1),):
        raise CompileError("the line's markers do not give one ability with steps")
    return abilities[0]


def _each_player_looks_at_top(
    m: re.Match[str], abilities: list[d.Ability], cdef: CardDef
) -> list[d.Ability]:
    """'All players each look at the top card of their deck. If it is <card>, they may reveal it
    and add it to their hand. They return any remaining card to the top or bottom of their
    deck.' Each player, in turn order starting with the active player, looks, may take the card
    and puts back the rest; the whole is treated as performed simultaneously (rulings Q319,
    Q331)."""
    frame = _frame_steps(abilities)
    filters = parse_selector("1 " + re.sub(r"^an? ", "", m.group(1))).sel.filters
    steps: list[d.Step] = []
    for p in (d.P.ACTIVE, d.P.STANDBY):
        var = f"looked_{p.value}"
        steps += [
            d.CustomStep("wp_eb01_a_look_own_top", (("player", p.value), ("var", var))),
            d.If(
                d.RefMatches(d.Var(var), filters),
                (d.May((d.AddToHand(d.Var(var), reveal=True),), player=p),),
            ),
            d.Arrange(d.Var(var), "top_or_bottom", player=p),
        ]
    return [dataclasses.replace(cast("Any", frame), steps=(d.Simultaneous(tuple(steps)),))]


def _delayed_this_turn(
    m: re.Match[str], abilities: list[d.Ability], cdef: CardDef
) -> list[d.Ability]:
    """'During this turn, when <a friendly Unit> destroys ..., <effect>.' A delayed trigger for
    the rest of the turn (rule 10-1-6-1: a 'when (some event occurs)' effect is a triggered
    effect): the trigger is the compiler's, the effect (every
    sentence after the comma) is compiled with the trigger's event and numbered from the line's
    start, and nothing of the line runs now."""
    frame = _frame_steps(abilities)
    g = grammar.G(cdef)
    trig = grammar.parse_trigger(f"when {m.group(1)}", g)
    inner = grammar.G(cdef, n=0, it=None, event=trig.event)
    steps = grammar.compile_steps(grammar._cap(m.group(2)), inner)
    delayed = d.DelayedTrigger(trig, steps, Duration.THIS_TURN)
    return [dataclasses.replace(cast("Any", frame), steps=(delayed,))]


def _reveal_return_to_top(
    m: re.Match[str], abilities: list[d.Ability], cdef: CardDef
) -> list[d.Ability]:
    """'Look at the top N cards of your deck. You may reveal 1 <card> among them and return it
    to the top of your deck. Return the remaining cards randomly to the bottom of your deck.'
    The rest goes to the bottom first, so the revealed card ends on top."""
    frame = _frame_steps(abilities)
    looked, pick = d.Var("looked"), d.Var("pick")
    filters = parse_selector("1 " + m.group(2)).sel.filters
    steps: tuple[d.Step, ...] = (
        d.LookTop(int(m.group(1))),
        d.Choose(
            "pick",
            d.Sel(d.Side.FRIENDLY, d.Loc.DECK, (d.IsRef(looked), *filters)),
            1,
            optional=True,
            targeting=False,
        ),
        d.Reveal(pick),
        d.BindVar(
            "looked",
            d.All(d.Sel(d.Side.FRIENDLY, d.Loc.DECK, (d.IsRef(looked), d.NotRef(pick)))),
        ),
        d.CustomStep("return_looked_bottom"),
        d.ToDeck(pick, bottom=False),
    )
    return [dataclasses.replace(cast("Any", frame), steps=steps)]


def _deployed_rested(
    m: re.Match[str], abilities: list[d.Ability], cdef: CardDef
) -> list[d.Ability]:
    """'All <Units> are deployed rested.' A constant DEPLOYED_RESTED rule on the cards the
    subject names, whoever deploys them. The compiler builds the constant (subject, side,
    condition, gate) for '<subject> gain <Blocker>'; its effect is replaced."""
    rule = d.RuleGrant(d.RuleMod(d.RuleKind.DEPLOYED_RESTED))
    out: list[d.Ability] = []
    for ab in abilities:
        if not isinstance(ab, d.Constant) or ab.effects != (d.KeywordGrant(d.Kw.BLOCKER),):
            raise CompileError("the rewritten line is not one keyword constant")
        out.append(dataclasses.replace(ab, effects=(rule,)))
    return out


# An inline token definition (``grammar._TOKDEF``, ``grammar.py:1185``) without its group.
_TOKEN_DEF = r"\[[^\[\]]+\]\((?:\([^()]*\)\s*)+[^()]*\)"

TEMPLATES: tuple[AnyTemplate, ...] = (
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
    TableTemplate(
        id="mill-top-of-deck",
        family="'place the top card of your deck into your trash'",
        entry="W-67",
        pattern=re.compile(r"place the top (?:\d+ cards|card) of your deck into your trash", re.I),
        table="steps",
        rx=_anchored(
            r"place the top (\d+) cards of your deck into your trash"
            r"|place the top card of your deck into your trash"
        ),
        build=_mill,
    ),
    TableTemplate(
        id="placed-trait-card-with-this-effect",
        family="'If you placed a (Trait) card with this effect' (after a mill)",
        entry="W-67",
        pattern=re.compile(r"If you placed an? (?:\([^()]+\)/?)+ card with this effect"),
        table="conditions",
        rx=_anchored(r"you placed an? ((?:\([^()]+\)/?)+) card with this effect"),
        build=_placed_trait_card,
    ),
    TableTemplate(
        id="placed-card-level-with-this-effect",
        family="'If you placed a card that is Lv.N or higher with this effect' (after a mill)",
        entry="W-67",
        pattern=re.compile(r"If you placed a card that is Lv\.\d+ or (?:higher|lower) with this"),
        table="conditions",
        rx=_anchored(r"you placed a card that is Lv\.(\d+) (or higher|or lower) with this effect"),
        build=_placed_card_level,
    ),
    TableTemplate(
        id="cant-activate-blocker",
        family="'It can't activate <Blocker>'",
        entry="W-63",
        pattern=re.compile(r"can't activate <Blocker> during this turn"),
        table="steps",
        rx=_anchored(r"(it|that Unit|the chosen Unit) can't activate <Blocker> during this turn"),
        build=_cant_activate_blocker,
    ),
    TableTemplate(
        id="exile-this-card-in-trash",
        family="'exile this card in your trash'",
        entry="W-63",
        pattern=re.compile(r"exile this card in your trash from the game", re.I),
        table="steps",
        rx=_anchored(r"exile this card in your trash from the game"),
        build=_exile_this_from_trash,
    ),
    TableTemplate(
        id="links-subject-is-it",
        family="'it' without antecedent: the Unit that links",
        entry="W-63",
        pattern=re.compile(r"When (?:a|one of your|another) friendly [^,]+? links, it "),
        table="triggers",
        rx=_anchored(r"when (?:a|one of your|another) friendly (.+?) links"),
        build=_links_subject_is_it,
    ),
    TableTemplate(
        id="rested-self-is-it",
        family="'it' without antecedent: this Unit, rested by an effect",
        entry="W-63",
        pattern=re.compile(r"When this Unit is rested by an effect, set it as active"),
        table="triggers",
        rx=_anchored(r"when this Unit is rested by an effect"),
        build=_rested_self_is_it,
    ),
    TableTemplate(
        id="all-enemy-players-each-choose",
        family="'all enemy players each choose'",
        entry="W-67",
        pattern=re.compile(r"[Aa]ll enemy players each choose \d+ of their"),
        table="steps",
        rx=_anchored(r"your opponent choose (\d+ of their .+)"),
        build=_opponent_each_chooses,
    ),
    TableTemplate(
        id="all-players-each-choose-resources",
        family="'All players each choose 1 of their Resources'",
        entry="W-67",
        pattern=re.compile(r"All players each choose \d+ of their Resources?"),
        table="steps",
        rx=_anchored(r"All players each choose (\d+) of their Resources?"),
        build=_all_players_choose_resources,
    ),
    TableTemplate(
        id="they-may-draw",
        family="'They may draw 1'",
        entry="W-67",
        pattern=re.compile(r"They may draw \d+\."),
        table="steps",
        rx=_anchored(r"They may draw (\d+)"),
        build=_they_may_draw,
    ),
    TableTemplate(
        id="deploy-either-inline-token",
        family="inline token definitions ('deploy 1 [A](...) or 1 [B](...) Unit token')",
        entry="W-63",
        pattern=re.compile(r"[Dd]eploy 1 \[[^\[\]]+\]\([^)]*\)[^\[]* or 1 \["),
        table="steps",
        rx=_anchored(r"deploy 1 (" + _TOKEN_DEF + r") or 1 (" + _TOKEN_DEF + r") Unit token"),
        build=_deploy_either_token,
    ),
    TableTemplate(
        id="if-they-draw-with-this-effect",
        family="'If they draw with this effect' (after 'They may draw')",
        entry="W-67",
        pattern=re.compile(r"If they draw with this effect, "),
        table="steps",
        rx=_anchored(r"If they draw with this effect, (.+)"),
        build=_if_they_drew,
    ),
    LineTemplate(
        id="ex-resource-command-trigger",
        family="the EX-Resource trigger ('when you play and activate ... using an EX Resource')",
        entry="W-63",
        pattern=re.compile(
            r"^(During your turn, )?[Ww]hen you play and activate (an? )?(.+?)Command card "
            r"using an EX Resource, (.+)$",
            re.S,
        ),
        wording=re.compile(r"[Ww]hen you play and activate (?:an? )?.+?Command card using an EX"),
        rewrite=lambda m: (
            f"{m.group(1) or ''}{'when' if m.group(1) else 'When'} you play "
            f"{m.group(2) or ''}{m.group(3)}Command card, {m.group(4)}"
        ),
        patch=_with_ex_used,
    ),
    LineTemplate(
        id="each-player-in-turn-order-looks-at-top",
        family="'All players each look at the top card' (each player in turn order)",
        entry="W-67",
        pattern=re.compile(
            r"^All players each look at the top card of their deck\. If it is (.+?), they may "
            r"reveal it and add it to their hand\. They return any remaining card to the top or "
            r"bottom of their deck\.$",
            re.S,
        ),
        wording=re.compile(r"All players each look at the top card of their deck"),
        rewrite=lambda m: LINE_TEMPLATE_BODY,
        patch=_each_player_looks_at_top,
    ),
    LineTemplate(
        id="delayed-during-this-turn-when",
        family="delayed 'during this turn, when …'",
        entry="W-63",
        pattern=re.compile(
            r"^During this turn, (?:when|if) ((?!it |they |this Unit |that Unit )(?:one of your|a "
            r"friendly|another friendly) .+? destroys an enemy (?:.*?)Unit with battle damage), "
            r"(.+)$",
            re.S,
        ),
        wording=re.compile(
            r"During this turn, (?:when|if) (?:one of your|a friendly|another friendly) .+? "
            r"destroys an enemy"
        ),
        rewrite=lambda m: LINE_TEMPLATE_BODY,
        patch=_delayed_this_turn,
    ),
    LineTemplate(
        id="reveal-one-return-it-to-the-top",
        family="'reveal 1 … return it to the top'",
        entry="W-67",
        pattern=re.compile(
            r"^Look at the top (\d+) cards of your deck\. You may reveal 1 (.+?) among them and "
            r"return it to the top of your deck\. Return the remaining cards randomly to the bottom "
            r"of your deck\.$"
        ),
        wording=re.compile(r"reveal 1 .+? among them and return it to the top of your deck"),
        rewrite=lambda m: LINE_TEMPLATE_BODY,
        patch=_reveal_return_to_top,
    ),
    LineTemplate(
        id="are-deployed-rested",
        family="'are deployed rested'",
        entry="W-63",
        pattern=re.compile(r"^(All .+?) are deployed rested\.$"),
        wording=re.compile(r"are deployed rested"),
        rewrite=lambda m: f"{m.group(1)} gain <Blocker>.",
        patch=_deployed_rested,
    ),
    SuffixTemplate(
        id="selector-suffix-lowest-hp",
        family="selector suffixes ('with the lowest HP')",
        entry="W-63",
        pattern=re.compile(r"Unit with the lowest HP"),
        suffix=re.compile(r" with the lowest HP$"),
        filters=lambda m: (d.CustomFilter("wp_gd03_a_lowest_hp"),),
    ),
)


# ---------------------------------------------------------------------------------------------
# compiling a card


def _entry(t: TableTemplate, used: list[str]) -> tuple[re.Pattern[str], Any]:
    def fn(m: re.Match[str], g: grammar.G) -> Any:
        out = t.build(m, g)
        used.append(t.id)
        return out

    fn.template_id = t.id  # type: ignore[attr-defined]
    return t.rx, fn


def _selector_with_suffixes(used: list[str]) -> Callable[..., Any]:
    """``grammar.parse_selector`` with the selector-suffix templates."""
    original = getattr(grammar, SELECTOR_PARSER)
    suffixes = [t for t in TEMPLATES if isinstance(t, SuffixTemplate)]

    def parse(phrase: str, **kwargs: Any) -> Any:
        text = phrase.strip().rstrip(".").strip()
        for t in suffixes:
            m = t.suffix.search(text)
            if m is None:
                continue
            spec = original(text[: m.start()], **kwargs)
            sel = dataclasses.replace(spec.sel, filters=(*spec.sel.filters, *t.filters(m)))
            used.append(t.id)
            return dataclasses.replace(spec, sel=sel)
        return original(phrase, **kwargs)

    parse.template_id = "selector-suffixes"  # type: ignore[attr-defined]
    return parse


@contextlib.contextmanager
def _template_tables(used: list[str]) -> Iterator[None]:
    """The compiler's tables with the table templates in front, and its selector parser with
    the suffix templates, restored on exit."""
    saved = {name: list(getattr(grammar, name)) for name in TABLES.values()}
    parse_selector_saved = getattr(grammar, SELECTOR_PARSER)
    table_templates = [t for t in TEMPLATES if isinstance(t, TableTemplate)]
    try:
        for table, name in TABLES.items():
            front = [_entry(t, used) for t in table_templates if t.table == table]
            getattr(grammar, name)[:0] = front
        setattr(grammar, SELECTOR_PARSER, _selector_with_suffixes(used))
        yield
    finally:
        for name, items in saved.items():
            getattr(grammar, name)[:] = items
        setattr(grammar, SELECTOR_PARSER, parse_selector_saved)


def tables_untouched() -> bool:
    """Whether the compiler's tables hold no template entry (checked after every run)."""
    parser = getattr(grammar, SELECTOR_PARSER)
    return getattr(parser, "template_id", None) is None and not any(
        getattr(fn, "template_id", None) is not None
        for name in TABLES.values()
        for _, fn in getattr(grammar, name)
    )


def _with_steps(ab: d.Ability, extra: tuple[d.Step, ...]) -> d.Ability:
    steps: tuple[d.Step, ...] = getattr(ab, "steps", ())
    return cast("d.Ability", dataclasses.replace(cast("Any", ab), steps=(*steps, *extra)))


def compile_line(raw: RawAbility, cdef: CardDef) -> tuple[list[d.Ability], str | None]:
    """The abilities of one line and the trailing or line template used, if any (``None``:
    the compiler's own result, with the table templates). Raises ``CompileError`` when neither
    the compiler nor a template applies."""
    for t in TEMPLATES:
        if isinstance(t, LineTemplate):
            m = t.pattern.match(raw.body)
            if m is None:
                continue
            body = t.rewrite(m)
            compiled = compile_ability(RawAbility(raw.markers, body, raw.line), cdef)
            return t.patch(m, compiled, cdef), t.id
        if not isinstance(t, Template):
            continue
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
    it used, in first-use order. Raises ``CompileError`` for a line nothing compiles."""
    if cdef.card_type in REMINDER_ONLY_TYPES:
        return d.CardScript(cdef.card_number, source="vanilla"), ()
    abilities: list[d.Ability] = []
    used: list[str] = []
    with _template_tables(used):
        for raw in split_abilities(normalize(cdef.effect)):
            got, template = compile_line(raw, cdef)
            abilities.extend(got)
            if template is not None:
                used.append(template)
    own, unit = route_abilities(cdef, abilities)
    return d.CardScript(cdef.card_number, own, unit, source="template"), tuple(dict.fromkeys(used))


def behaviour_repr(script: d.CardScript) -> str:
    """The part of a script that decides behaviour: its abilities and Unit abilities. The
    ``source`` and ``notes`` fields record provenance only."""
    return repr((script.card_number, script.abilities, script.unit_abilities))
