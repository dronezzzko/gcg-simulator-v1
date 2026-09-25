"""Bindings for GD01-073..GD01-130 (work package WP-GD01-B).

HP references in card text use current HP, i.e. HP after damage (FAQ Q96, ruling
GD03-049:Q224), so HP filters here use :attr:`Stat.REMAINING_HP`.
"""

from __future__ import annotations

from gcg_sim.cards.model import CardDef
from gcg_sim.cards.tokens import parse_token_specs
from gcg_sim.effects import dsl as d
from gcg_sim.effects.bindings import card, custom_cond
from gcg_sim.effects.compiler import route_abilities
from gcg_sim.engine import view as V
from gcg_sim.engine.state import GameState
from gcg_sim.engine.types import NO_ARG, Duration

UNIT = d.IsKind((d.CardKind.UNIT,))
ACTIVE = d.IsRested(False)


def _enemy_units(*filters: d.Filter) -> d.Sel:
    return d.Sel(d.Side.ENEMY, d.Loc.BATTLE, (UNIT, *filters))


def _friendly_units(*filters: d.Filter) -> d.Sel:
    return d.Sel(d.Side.FRIENDLY, d.Loc.BATTLE, (UNIT, *filters))


def _current_hp(op: d.Op, value: d.Value) -> d.StatCmp:
    return d.StatCmp(d.Stat.REMAINING_HP, op, value)


def _script(c: CardDef, abilities: list[d.Ability]) -> d.CardScript:
    own, unit = route_abilities(c, abilities)
    return d.CardScript(c.card_number, abilities=own, unit_abilities=unit, source="binding")


BURST_ADD_TO_HAND = d.Burst((d.AddToHand(d.ThisCard()),))
BURST_DEPLOY = d.Burst((d.DeployCard(d.ThisCard()),))
BURST_ACTIVATE_MAIN = d.Burst((d.ActivateMain(),))


def _base_deploy(*then: d.Step) -> d.Triggered:
    return d.Triggered(d.Trigger(d.Ev.DEPLOYED), (d.ShieldToHand(), *then))


@card("GD01-073")
def gd01_073(c: CardDef) -> d.CardScript:
    return _script(
        c,
        [
            d.Triggered(
                d.Trigger(d.Ev.ATTACKS),
                (
                    d.Choose("t1", _enemy_units(_current_hp(d.Op.LE, 2))),
                    d.ReturnToHand(d.Var("t1")),
                ),
                gate=d.Gate.LINKED,
            )
        ],
    )


@card("GD01-075")
def gd01_075(c: CardDef) -> d.CardScript:
    return _script(
        c,
        [
            d.Triggered(
                d.Trigger(d.Ev.DEPLOYED),
                (
                    d.Choose("t1", _enemy_units(_current_hp(d.Op.EQ, 1))),
                    d.ReturnToHand(d.Var("t1")),
                ),
            )
        ],
    )


@card("GD01-090")
def gd01_090(c: CardDef) -> d.CardScript:
    return _script(
        c,
        [
            BURST_ADD_TO_HAND,
            d.Constant(
                (d.RuleGrant(d.RuleMod(d.RuleKind.AP_CANT_BE_REDUCED)),),
                gate=d.Gate.LINKED,
            ),
        ],
    )


@card("GD01-091")
def gd01_091(c: CardDef) -> d.CardScript:
    no_damage_from_low_ap = d.RuleMod(
        d.RuleKind.CANT_RECEIVE_DAMAGE,
        damage_kind=d.DamageKind.BATTLE,
        source_filters=(UNIT, d.StatCmp(d.Stat.AP, d.Op.LE, 3)),
        source_side=d.Side.ENEMY,
    )
    return _script(
        c,
        [
            BURST_ADD_TO_HAND,
            d.Constant(
                (d.RuleGrant(no_damage_from_low_ap),),
                cond=d.And(
                    (d.IsTurn(d.P.YOU), d.RefMatches(d.This(), (d.HasKeyword(d.Kw.BREACH),)))
                ),
            ),
        ],
    )


@custom_cond("wp_gd01_b_destroyed_with_damage")
def destroyed_with_damage(
    st: GameState, dv: V.Derived, ctx: V.Ctx, params: dict[str, object]
) -> bool:
    """The destruction came from damage (battle damage or effect damage reaching HP, which
    rules management destroys with no acting player), not from a "destroy" effect (Q149)."""
    return ctx.ev("battle", 0) == 1 or ctx.ev("by") == NO_ARG


@card("GD01-094")
def gd01_094(c: CardDef) -> d.CardScript:
    return _script(
        c,
        [
            BURST_ADD_TO_HAND,
            d.Triggered(
                d.Trigger(
                    d.Ev.DESTROYED,
                    self_only=False,
                    subject=d.Sel(d.Side.ENEMY, d.Loc.BATTLE),
                ),
                (d.Draw(1),),
                cond=d.And(
                    (
                        d.EventFlag("linked"),
                        d.RefMatches(d.This(), (d.IsAttacking(),)),
                        d.CustomCond("wp_gd01_b_destroyed_with_damage"),
                    )
                ),
                once_per_turn=True,
            ),
        ],
    )


@card("GD01-097")
def gd01_097(c: CardDef) -> d.CardScript:
    opp_has_8 = d.Cmp(d.HandSize(d.P.OPP), d.Op.GE, 8)
    return _script(
        c,
        [
            BURST_ADD_TO_HAND,
            d.Activated(
                d.Timing.MAIN,
                (),
                (
                    d.If(
                        opp_has_8,
                        (
                            d.SetActive(d.This()),
                            d.Apply(d.This(), d.RuleGrant(d.RuleMod(d.RuleKind.CANT_ATTACK))),
                        ),
                    ),
                ),
                cond=opp_has_8,
                once_per_turn=True,
            ),
        ],
    )


@card("GD01-098")
def gd01_098(c: CardDef) -> d.CardScript:
    low_ap_enemy = d.Exists(_enemy_units(d.StatCmp(d.Stat.AP, d.Op.LE, 1)))
    return _script(
        c,
        [
            BURST_ADD_TO_HAND,
            d.Activated(
                d.Timing.ACTION,
                (),
                (d.If(low_ap_enemy, (d.Recover(d.This(), 1),)),),
                cond=d.And((low_ap_enemy, d.RefMatches(d.This(), (d.IsDamaged(),)))),
                once_per_turn=True,
            ),
        ],
    )


@card("GD01-099")
def gd01_099(c: CardDef) -> d.CardScript:
    return _script(
        c,
        [
            d.Burst(
                (
                    d.Choose("t1", _enemy_units(_current_hp(d.Op.LE, 5))),
                    d.Rest(d.Var("t1")),
                )
            ),
            d.Command(
                d.Timing.MAIN_OR_ACTION,
                (
                    d.Choose("t1", _enemy_units(_current_hp(d.Op.LE, 3)), count=2, min_count=1),
                    d.Rest(d.Var("t1")),
                ),
            ),
        ],
    )


@card("GD01-106")
def gd01_106(c: CardDef) -> d.CardScript:
    (zaku,) = parse_token_specs(c.effect)
    return _script(c, [d.Command(d.Timing.MAIN, (d.DeployToken(zaku.key, 2),))])


@card("GD01-112")
def gd01_112(c: CardDef) -> d.CardScript:
    your_active_units = _friendly_units(ACTIVE)
    return _script(
        c,
        [
            d.Command(
                d.Timing.MAIN,
                (
                    d.Choose("t1", your_active_units, count=2),
                    d.Rest(d.Var("t1")),
                    d.IfYouDo(
                        (
                            d.Choose("t2", _enemy_units()),
                            d.Damage(d.Var("t2"), 3),
                        )
                    ),
                ),
                cond=d.Cmp(d.Count(your_active_units), d.Op.GE, 2),
            )
        ],
    )


@card("GD01-114")
def gd01_114(c: CardDef) -> d.CardScript:
    return _script(
        c,
        [
            d.Command(
                d.Timing.ACTION,
                (
                    d.Choose("t1", _friendly_units(), count=2),
                    d.Apply(d.Var("t1"), d.StatMod(ap=1), Duration.THIS_TURN),
                ),
                cond=d.Cmp(d.Count(_friendly_units()), d.Op.GE, 2),
            )
        ],
    )


@card("GD01-117")
def gd01_117(c: CardDef) -> d.CardScript:
    return _script(
        c,
        [
            BURST_ACTIVATE_MAIN,
            d.Command(
                d.Timing.MAIN_OR_ACTION,
                (
                    d.Choose("t1", _enemy_units(_current_hp(d.Op.LE, 5))),
                    d.ReturnToHand(d.Var("t1")),
                ),
            ),
        ],
    )


@card("GD01-122")
def gd01_122(c: CardDef) -> d.CardScript:
    have_link_unit = d.MinOf((d.Count(_friendly_units(d.IsLinked())), 1))
    hp_limit = d.Sum((2, d.Times(have_link_unit, 2)))
    return _script(
        c,
        [
            d.Command(
                d.Timing.MAIN,
                (
                    d.Choose("t1", _enemy_units(_current_hp(d.Op.LE, hp_limit))),
                    d.ReturnToHand(d.Var("t1")),
                ),
            )
        ],
    )


@card("GD01-123")
def gd01_123(c: CardDef) -> d.CardScript:
    return _script(
        c,
        [
            BURST_DEPLOY,
            _base_deploy(
                d.Choose("t1", _enemy_units(_current_hp(d.Op.LE, 3)), after_then=True),
                d.Rest(d.Var("t1")),
            ),
        ],
    )


@card("GD01-129")
def gd01_129(c: CardDef) -> d.CardScript:
    return _script(
        c,
        [
            BURST_DEPLOY,
            _base_deploy(
                d.Choose("t1", _enemy_units(_current_hp(d.Op.LE, 3)), after_then=True),
                d.ReturnToHand(d.Var("t1")),
            ),
        ],
    )


@card("GD01-130")
def gd01_130(c: CardDef) -> d.CardScript:
    academy_in_play = d.Exists(_friendly_units(d.HasTrait(("Academy",))))
    return _script(
        c,
        [
            BURST_DEPLOY,
            _base_deploy(),
            d.Activated(
                d.Timing.MAIN,
                (d.RestSelf(),),
                (
                    d.If(
                        academy_in_play,
                        (
                            d.Choose("t1", _enemy_units()),
                            d.Apply(d.Var("t1"), d.StatMod(ap=-1), Duration.THIS_TURN),
                        ),
                    ),
                ),
                cond=d.Or((d.NotC(academy_in_play), d.Exists(_enemy_units()))),
            ),
        ],
    )
