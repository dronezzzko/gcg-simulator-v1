"""Bindings for EB01-047..EB01-090 (work package WP-EB01-B).

Each binding covers the whole card; lines the compiler already gets right are reused through
:func:`compile_parts`. Multiplayer wording is read for 1v1 as recorded in docs/CONFLICTS.md
(``ambiguous:multiplayer-vocabulary``): "each enemy player" is the opponent and "all players
each ..." is both players, the active player first, treated as simultaneous.
"""

from __future__ import annotations

from gcg_sim.cards.model import CardDef
from gcg_sim.effects import dsl as d
from gcg_sim.effects.bindings import card, custom_step
from gcg_sim.effects.compiler import compile_parts, route_abilities
from gcg_sim.effects.compiler.abilities import CompileError
from gcg_sim.effects.registry import UnimplementedCardError
from gcg_sim.engine import view as V
from gcg_sim.engine.state import Frame, GameState
from gcg_sim.engine.types import Duration, Zone

GG = d.HasTrait(("G Generation",))
UNIT = d.IsKind((d.CardKind.UNIT,))
ACTIVE = d.IsRested(False)
RESTED = d.IsRested(True)


def _line(c: CardDef, prefix: str) -> list[d.Ability]:
    """The compiled abilities of the card's text line starting with ``prefix``."""
    for line, res in compile_parts(c):
        if line.startswith(prefix):
            if isinstance(res, CompileError):
                raise UnimplementedCardError(f"{c.card_number}: {line!r}: {res}")
            return res
    raise UnimplementedCardError(f"{c.card_number}: no line starting with {prefix!r}")


def _script(c: CardDef, abilities: list[d.Ability], notes: str) -> d.CardScript:
    own, unit = route_abilities(c, abilities)
    return d.CardScript(
        c.card_number, abilities=own, unit_abilities=unit, source="binding", notes=notes
    )


def _development(n: int, body: tuple[d.Step, ...]) -> tuple[d.Step, ...]:
    """Rule 13-1-8-1: "you may exile n (G Generation) cards in your trash from the game. If
    you do, activate the following effect" (the exile is allowed with no ■ target, Q324)."""
    trash_gg = d.Sel(d.Side.FRIENDLY, d.Loc.TRASH, (GG,))
    return (
        d.If(
            d.Exists(trash_gg, n),
            (
                d.May(
                    (
                        d.Choose("dev", trash_gg, n),
                        d.Exile(d.Var("dev")),
                        d.IfYouDo(body),
                    )
                ),
            ),
        ),
    )


@card("EB01-047")
def eb01_047(c: CardDef) -> d.CardScript:
    grant = d.Apply(d.This(), d.KeywordGrant(d.Kw.HIGH_MANEUVER), Duration.THIS_TURN)
    return _script(
        c,
        [d.Triggered(d.Trigger(d.Ev.PAIRED), _development(1, (grant,)))],
        "Development selector phrase does not compile",
    )


@card("EB01-050")
def eb01_050(c: CardDef) -> d.CardScript:
    steps: tuple[d.Step, ...] = (
        d.Mill(1),
        d.If(
            d.RefMatches(d.Var("milled"), (d.StatCmp(d.Stat.LV, d.Op.GE, 3),)),
            (
                d.Choose("t1", d.Sel(d.Side.ENEMY, d.Loc.BATTLE, (UNIT,))),
                d.Apply(d.Var("t1"), d.StatMod(ap=-2), Duration.THIS_BATTLE),
            ),
        ),
    )
    return _script(
        c,
        [d.Triggered(d.Trigger(d.Ev.ATTACKS), steps)],
        "'place the top card of your deck into your trash' has no template",
    )


@card("EB01-059")
def eb01_059(c: CardDef) -> d.CardScript:
    steps: tuple[d.Step, ...] = (
        d.Choose("r0", d.Sel(d.Side.FRIENDLY, d.Loc.RESOURCE_AREA), chooser=d.P.YOU),
        d.Choose("r1", d.Sel(d.Side.ENEMY, d.Loc.RESOURCE_AREA), chooser=d.P.OPP),
        d.SetActive(d.Union((d.Var("r0"), d.Var("r1")))),
    )
    return _script(
        c,
        [d.Triggered(d.Trigger(d.Ev.ATTACKS), steps, once_per_turn=True, gate=d.Gate.LINKED)],
        "'All players each choose 1 of their Resources' has no template (Q321-Q323)",
    )


@card("EB01-060")
def eb01_060(c: CardDef) -> d.CardScript:
    body: tuple[d.Step, ...] = (
        d.Choose(
            "t1",
            d.Sel(d.Side.ENEMY, d.Loc.BATTLE, (UNIT, d.StatCmp(d.Stat.LV, d.Op.LE, 4))),
        ),
        d.ReturnToHand(d.Var("t1")),
    )
    return _script(
        c,
        [d.Triggered(d.Trigger(d.Ev.PAIRED), _development(3, body))],
        "Development selector phrase does not compile",
    )


@card("EB01-062")
def eb01_062(c: CardDef) -> d.CardScript:
    steps: tuple[d.Step, ...] = (
        d.May((d.Draw(1, player=d.P.OPP),), player=d.P.OPP),
        d.IfYouDo((d.Draw(1),)),
    )
    return _script(
        c,
        [
            *_line(c, "【Burst】"),
            d.Triggered(d.Trigger(d.Ev.ATTACKS), steps, once_per_turn=True),
        ],
        "'They may draw 1' has no template",
    )


@card("EB01-067")
def eb01_067(c: CardDef) -> d.CardScript:
    looked = d.IsRef(d.Var("looked"))
    steps: tuple[d.Step, ...] = (
        d.LookTop(3),
        d.Choose(
            "pick",
            d.Sel(d.Side.FRIENDLY, d.Loc.DECK, (looked, UNIT, GG)),
            optional=True,
            targeting=False,
        ),
        d.Reveal(d.Var("pick")),
        d.BindVar(
            "looked",
            d.All(d.Sel(d.Side.FRIENDLY, d.Loc.DECK, (looked, d.NotRef(d.Var("pick"))))),
        ),
        d.CustomStep("return_looked_bottom"),
        d.ToDeck(d.Var("pick"), bottom=False),
    )
    return _script(
        c,
        [*_line(c, "【Burst】"), d.Triggered(d.Trigger(d.Ev.PAIRED), steps)],
        "'reveal 1 ... among them and return it to the top of your deck' has no template",
    )


@card("EB01-068")
def eb01_068(c: CardDef) -> d.CardScript:
    paired_card_in_trash = d.All(d.Sel(d.Side.ANY, d.Loc.TRASH, (d.IsRef(d.EventCard("pilot")),)))
    steps: tuple[d.Step, ...] = (d.May((d.ToDeck(paired_card_in_trash, bottom=False),)),)
    return _script(
        c,
        [
            *_line(c, "【Burst】"),
            d.Triggered(d.Trigger(d.Ev.DESTROYED), steps, gate=d.Gate.LINKED),
        ],
        "compiled PairedPilotOf(This) is empty once the Unit is in the trash; use the "
        "destroyed event's pilot (rule 13-2-8-2-1)",
    )


@card("EB01-072")
def eb01_072(c: CardDef) -> d.CardScript:
    mine = d.Sel(d.Side.FRIENDLY, d.Loc.BATTLE, (UNIT, ACTIVE, d.HasKeyword(d.Kw.BLOCKER)))
    theirs = d.Sel(d.Side.ENEMY, d.Loc.BATTLE, (UNIT, d.StatCmp(d.Stat.LV, d.Op.LE, 4)))
    steps: tuple[d.Step, ...] = (
        d.If(
            d.And((d.Exists(mine), d.Exists(theirs))),
            (
                d.Choose("t1", mine),
                d.Choose("t2", theirs),
                d.Rest(d.Union((d.Var("t1"), d.Var("t2")))),
            ),
        ),
    )
    return _script(
        c,
        [*_line(c, "【Burst】"), d.Triggered(d.Trigger(d.Ev.PAIRED), steps)],
        "Q327: resolves only when both Units can be chosen",
    )


@card("EB01-074")
def eb01_074(c: CardDef) -> d.CardScript:
    steps: tuple[d.Step, ...] = (
        d.Choose("t1", d.Sel(d.Side.FRIENDLY, d.Loc.BATTLE, (UNIT, ACTIVE, GG))),
        d.Rest(d.Var("t1")),
        d.IfYouDo(
            (
                d.Choose(
                    "t2",
                    d.Sel(d.Side.ENEMY, d.Loc.BATTLE, (UNIT, ACTIVE)),
                    chooser=d.P.OPP,
                    after_then=True,
                ),
                d.Rest(d.Var("t2")),
            )
        ),
    )
    return _script(
        c,
        [*_line(c, "【Burst】"), d.Command(d.Timing.MAIN_OR_ACTION, steps)],
        "'all enemy players each choose 1 of their active Units' has no template (Q329, Q330)",
    )


@card("EB01-077")
def eb01_077(c: CardDef) -> d.CardScript:
    enemy_attacking = d.Exists(d.Sel(d.Side.ENEMY, d.Loc.BATTLE, (UNIT, d.IsAttacking())))
    steps: tuple[d.Step, ...] = (
        d.Choose("t1", d.Sel(d.Side.FRIENDLY, d.Loc.BATTLE, (UNIT, RESTED, GG))),
        d.If(enemy_attacking, (d.ChangeAttackTarget(d.Var("t1")),)),
    )
    return _script(
        c,
        [*_line(c, "【Burst】"), d.Command(d.Timing.ACTION, steps)],
        "compiled ChangeAttackTarget also redirects a friendly attacker; only a battling "
        "enemy Unit's attack target may change",
    )


@custom_step("wp_eb01_b_look_top_own")
def look_top_own(st: GameState, f: Frame, ctx: V.Ctx, params: dict[str, object]) -> bool:
    """The given player looks at the top card of their own deck (known to them only)."""
    player = V.player_of(st, ctx, d.P(str(params["player"])))
    cards = tuple(st.zones[player][Zone.DECK][:1])
    for u in cards:
        st.cards[u].known |= 1 << player
    f.vars[str(params["var"])] = cards
    return bool(cards)


def _premium_look(player: d.P, var: str, look: d.Step) -> tuple[d.Step, ...]:
    top = d.Var(var)
    return (
        look,
        d.If(
            d.RefMatches(top, (UNIT,)),
            (d.May((d.AddToHand(top, reveal=True),), player=player),),
        ),
        d.Arrange(top, "top_or_bottom", player=player),
    )


@card("EB01-078")
def eb01_078(c: CardDef) -> d.CardScript:
    opp_look = d.CustomStep("wp_eb01_b_look_top_own", (("player", "opponent"), ("var", "top1")))
    steps: tuple[d.Step, ...] = (
        *_premium_look(d.P.YOU, "top0", d.LookTop(1, var="top0")),
        *_premium_look(d.P.OPP, "top1", opp_look),
    )
    return _script(
        c,
        [d.Command(d.Timing.MAIN, steps)],
        "'All players each look at the top card of their deck' has no template (Q331)",
    )


@card("EB01-082")
def eb01_082(c: CardDef) -> d.CardScript:
    steps: tuple[d.Step, ...] = (
        d.Choose(
            "t1",
            d.Sel(d.Side.ENEMY, d.Loc.BATTLE, (UNIT, d.StatCmp(d.Stat.LV, d.Op.LE, 3))),
        ),
        d.ReturnToHand(d.Var("t1")),
    )
    return _script(
        c,
        [*_line(c, "【Burst】"), d.Command(d.Timing.ACTION, steps)],
        "'belonging to each enemy player' compiled as either side; it means the opponent",
    )


@card("EB01-085")
def eb01_085(c: CardDef) -> d.CardScript:
    mine = d.Sel(d.Side.FRIENDLY, d.Loc.BATTLE, (UNIT, ACTIVE, d.HasColor(("Blue",)), GG))
    theirs = d.Sel(d.Side.ENEMY, d.Loc.BATTLE, (UNIT,))
    steps: tuple[d.Step, ...] = (
        d.ShieldToHand(1),
        d.If(
            d.And((d.Exists(mine), d.Exists(theirs))),
            (
                d.May(
                    (
                        d.Choose("t1", mine, after_then=True),
                        d.Choose("t2", theirs, after_then=True),
                        d.Rest(d.Union((d.Var("t1"), d.Var("t2")))),
                    )
                ),
            ),
        ),
    )
    return _script(
        c,
        [*_line(c, "【Burst】"), d.Triggered(d.Trigger(d.Ev.DEPLOYED), steps)],
        "compiled script dropped '... and 1 enemy Unit'",
    )


@card("EB01-086")
def eb01_086(c: CardDef) -> d.CardScript:
    links = d.Trigger(
        d.Ev.LINKED,
        self_only=False,
        subject=d.Sel(d.Side.FRIENDLY, d.Loc.BATTLE, (UNIT, GG)),
    )
    grant = d.Apply(d.EventCard("subject"), d.KeywordGrant(d.Kw.REPAIR, 2), Duration.THIS_TURN)
    return _script(
        c,
        [
            *_line(c, "【Burst】"),
            *_line(c, "【Deploy】"),
            d.Triggered(links, (grant,), once_per_turn=True),
        ],
        "'When a friendly (G Generation) Unit links, it gains ...' has no antecedent for 'it'",
    )


@card("EB01-090")
def eb01_090(c: CardDef) -> d.CardScript:
    steps: tuple[d.Step, ...] = (
        d.ShieldToHand(1),
        d.If(
            d.IsTurn(d.P.YOU),
            (
                d.Choose(
                    "t1",
                    d.Sel(d.Side.ENEMY, d.Loc.BATTLE, (UNIT, d.StatCmp(d.Stat.HP, d.Op.LE, 2))),
                    after_then=True,
                ),
                d.ReturnToHand(d.Var("t1")),
            ),
        ),
    )
    return _script(
        c,
        [*_line(c, "【Burst】"), d.Triggered(d.Trigger(d.Ev.DEPLOYED), steps)],
        "'belonging to each enemy player' compiled as either side; it means the opponent",
    )
