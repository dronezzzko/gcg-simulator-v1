"""Bindings for starter decks ST05-ST08 (work package WP-ST05-08).

Lines the text compiler gets right are reused through :func:`compile_parts`; the rest are
written in the DSL here.
"""

from __future__ import annotations

from gcg_sim.cards.model import CardDef
from gcg_sim.effects import dsl as d
from gcg_sim.effects.bindings import card, custom_value
from gcg_sim.effects.compiler import compile_parts, route_abilities
from gcg_sim.effects.registry import UnimplementedCardError
from gcg_sim.engine import view as V
from gcg_sim.engine.state import GameState
from gcg_sim.engine.types import Zone

HIGHEST_ENEMY_UNIT_LV = "wp_st05_08_highest_enemy_unit_lv"

UNIT = d.IsKind((d.CardKind.UNIT,))
PILOT = d.IsKind((d.CardKind.PILOT,))


def _unit_sel(side: d.Side, *filters: d.Filter) -> d.Sel:
    return d.Sel(side, d.Loc.BATTLE, (UNIT, *filters))


def _compiled(c: CardDef, *indexes: int) -> list[d.Ability]:
    parts = compile_parts(c)
    out: list[d.Ability] = []
    for i in indexes:
        line, res = parts[i]
        if isinstance(res, Exception):
            raise UnimplementedCardError(
                f"{c.card_number}: line {line!r} no longer compiles: {res}"
            )
        out.extend(res)
    return out


def _script(c: CardDef, abilities: list[d.Ability]) -> d.CardScript:
    own, unit = route_abilities(c, abilities)
    return d.CardScript(c.card_number, abilities=own, unit_abilities=unit, source="binding")


@custom_value(HIGHEST_ENEMY_UNIT_LV)
def highest_enemy_unit_lv(
    st: GameState, dv: V.Derived, ctx: V.Ctx, params: dict[str, object]
) -> int:
    enemy = 1 - ctx.controller
    return max((V.level_of(st, u) for u in st.zones[enemy][Zone.BATTLE]), default=0)


@card("ST05-010")
def st05_010(c: CardDef) -> d.CardScript:
    """Q168: both Units must be choosable, otherwise the effect does not activate."""
    t1, t2 = d.Var("t1"), d.Var("t2")
    when_paired = d.Triggered(
        d.Trigger(d.Ev.PAIRED),
        (
            d.If(
                d.And((d.Exists(_unit_sel(d.Side.FRIENDLY)), d.Exists(_unit_sel(d.Side.ENEMY)))),
                (
                    d.Choose("t1", _unit_sel(d.Side.FRIENDLY)),
                    d.Choose("t2", _unit_sel(d.Side.ENEMY), distinct_from=("t1",)),
                    d.If(
                        d.And((d.NotC(d.RefEmpty(t1)), d.NotC(d.RefEmpty(t2)))),
                        (d.Damage(d.Union((t1, t2)), 1),),
                    ),
                ),
            ),
        ),
    )
    return _script(c, [*_compiled(c, 0), when_paired])


@card("ST06-015")
def st06_015(c: CardDef) -> d.CardScript:
    """ "It" is the friendly (Clan) Unit that linked."""
    on_link = d.Triggered(
        d.Trigger(
            d.Ev.LINKED,
            self_only=False,
            subject=_unit_sel(d.Side.FRIENDLY, d.HasTrait(("Clan",))),
        ),
        (d.Apply(d.EventCard("subject"), d.KeywordGrant(d.Kw.BREACH, 3)),),
        once_per_turn=True,
    )
    return _script(c, [*_compiled(c, 0, 1), on_link])


@card("ST07-001")
def st07_001(c: CardDef) -> d.CardScript:
    milled_cb = d.Sel(d.Side.FRIENDLY, d.Loc.TRASH, (d.HasTrait(("CB",)), d.IsRef(d.Var("milled"))))
    when_paired = d.Triggered(
        d.Trigger(d.Ev.PAIRED),
        (d.Mill(2), d.If(d.Exists(milled_cb), (d.Draw(1),))),
    )
    return _script(c, [*_compiled(c, 0), when_paired])


@card("ST07-007")
def st07_007(c: CardDef) -> d.CardScript:
    cb_pilot = d.Sel(d.Side.FRIENDLY, d.Loc.PAIRED, (PILOT, d.HasTrait(("CB",))))
    return _script(
        c,
        [
            d.Constant(
                (d.StatMod(ap=2),),
                cond=d.And((d.IsTurn(d.P.YOU), d.Exists(cb_pilot))),
            )
        ],
    )


@card("ST07-009")
def st07_009(c: CardDef) -> d.CardScript:
    """The "instead" branch keeps the "during this turn" duration of the replaced step."""
    cb_in_trash = d.Count(d.Sel(d.Side.FRIENDLY, d.Loc.TRASH, (d.HasTrait(("CB",)),)))
    attack = d.Triggered(
        d.Trigger(d.Ev.ATTACKS),
        (
            d.If(
                d.Cmp(cb_in_trash, d.Op.GE, 7),
                (d.Apply(d.All(_unit_sel(d.Side.FRIENDLY, d.HasTrait(("CB",)))), d.StatMod(ap=1)),),
                (d.Apply(d.This(), d.StatMod(ap=1)),),
            ),
        ),
    )
    return _script(c, [*_compiled(c, 0), attack])


@card("ST07-011")
def st07_011(c: CardDef) -> d.CardScript:
    permission = d.RuleMod(
        d.RuleKind.MAY_ATTACK_ACTIVE,
        source_filters=(
            UNIT,
            d.IsRested(False),
            d.LevelCmpRef(d.Stat.LV, d.Op.LE, d.This(), d.Stat.LV),
        ),
    )
    when_paired = d.Triggered(
        d.Trigger(d.Ev.PAIRED),
        (
            d.If(
                d.RefMatches(d.This(), (UNIT, d.HasTrait(("CB",)))),
                (d.Apply(d.This(), d.RuleGrant(permission)),),
            ),
        ),
    )
    return _script(c, [*_compiled(c, 0), when_paired])


@card("ST07-012")
def st07_012(c: CardDef) -> d.CardScript:
    protection = d.RuleMod(
        d.RuleKind.CANT_RECEIVE_DAMAGE,
        damage_kind=d.DamageKind.BATTLE,
        source_filters=(UNIT, d.StatCmp(d.Stat.AP, d.Op.LE, 3)),
        source_side=d.Side.ENEMY,
    )
    cb_link = d.Exists(_unit_sel(d.Side.FRIENDLY, d.HasTrait(("CB",)), d.IsLinked()))
    constant = d.Constant(
        (d.RuleGrant(protection),),
        cond=d.And((d.IsTurn(d.P.YOU), cb_link)),
    )
    return _script(c, [*_compiled(c, 0), constant])


@card("ST07-013")
def st07_013(c: CardDef) -> d.CardScript:
    """Only an attacking enemy Unit has an attack target to change."""
    t1 = d.Var("t1")
    command = d.Command(
        d.Timing.ACTION,
        (
            d.Choose("t1", _unit_sel(d.Side.FRIENDLY, d.IsRested(), d.HasTrait(("CB",)))),
            d.If(
                d.Exists(_unit_sel(d.Side.ENEMY, d.IsAttacking())),
                (d.ChangeAttackTarget(t1),),
            ),
        ),
    )
    return _script(c, [*_compiled(c, 0), command])


@card("ST08-001")
def st08_001(c: CardDef) -> d.CardScript:
    enemy_units = d.Count(_unit_sel(d.Side.ENEMY))
    per_enemy = d.Times(enemy_units, -1)
    reduction = d.Constant(
        (d.CostMod(cost=per_enemy, level=per_enemy),),
        cond=d.NotC(d.Exists(_unit_sel(d.Side.FRIENDLY, d.StatCmp(d.Stat.LV, d.Op.GE, 6)))),
        where=d.Where.HAND,
    )
    highest = d.StatCmp(d.Stat.LV, d.Op.GE, d.CustomValue(HIGHEST_ENEMY_UNIT_LV))
    when_paired = d.Triggered(
        d.Trigger(d.Ev.PAIRED),
        (
            d.Choose("t1", _unit_sel(d.Side.ENEMY, highest)),
            d.Damage(d.Var("t1"), 3),
        ),
    )
    return _script(c, [reduction, when_paired])


@card("ST08-006")
def st08_006(c: CardDef) -> d.CardScript:
    t1 = d.Var("t1")
    ef_unit_in_hand = d.Sel(d.Side.FRIENDLY, d.Loc.HAND, (UNIT, d.HasTrait(("Earth Federation",))))
    attack = d.Triggered(
        d.Trigger(d.Ev.ATTACKS),
        (
            d.If(
                d.AttackTargetIs("player"),
                (
                    d.Choose("t1", ef_unit_in_hand, targeting=False),
                    d.Reveal(t1),
                    d.ToDeck(t1, bottom=True),
                    d.IfYouDo((d.Draw(2),)),
                ),
            ),
        ),
        once_per_turn=True,
        gate=d.Gate.PAIRED,
    )
    return _script(c, [attack])


@card("ST08-011")
def st08_011(c: CardDef) -> d.CardScript:
    """Drawing several cards with one effect triggers this once (rule 10-1-6-3)."""
    on_draw = d.Triggered(
        d.Trigger(d.Ev.DRAWN, self_only=False, subject=d.Sel(d.Side.FRIENDLY, d.Loc.HAND)),
        (
            d.If(
                d.RefMatches(d.This(), (UNIT, d.HasColor(("Blue",)))),
                (d.Apply(d.This(), d.KeywordGrant(d.Kw.HIGH_MANEUVER)),),
            ),
        ),
        cond=d.EventFlag("by_effect"),
    )
    return _script(c, [*_compiled(c, 0), on_draw])


@card("ST08-013")
def st08_013(c: CardDef) -> d.CardScript:
    t1 = d.Var("t1")
    mafty_link = d.Exists(_unit_sel(d.Side.FRIENDLY, d.HasTrait(("Mafty",)), d.IsLinked()))
    command = d.Command(
        d.Timing.MAIN_OR_ACTION,
        (
            d.Choose("t1", _unit_sel(d.Side.ENEMY)),
            d.If(mafty_link, (d.Damage(t1, 2),), (d.Damage(t1, 1),)),
        ),
    )
    return _script(c, [command])
