"""Bindings for GD01-001..GD01-072 (work package WP-GD01-A) whose compiled script is missing
or wrong. Every ability of a bound card is written out here so the binding does not depend on
how the text compiler handles the card's other lines."""

from __future__ import annotations

from gcg_sim.cards.model import CardDef
from gcg_sim.cards.tokens import parse_token_specs
from gcg_sim.effects import dsl as d
from gcg_sim.effects.bindings import card
from gcg_sim.engine.types import Duration

_UNIT = d.IsKind((d.CardKind.UNIT,))
_ENEMY_UNITS = d.Sel(d.Side.ENEMY, d.Loc.BATTLE, (_UNIT,))


def _token_key(c: CardDef) -> str:
    (spec,) = parse_token_specs(c.effect)
    return spec.key


def _enemy_units_with_current_hp(op: d.Op, amount: int) -> d.Sel:
    """Rules FAQ Q96: "with N or less HP" means HP after subtracting damage."""
    return d.Sel(d.Side.ENEMY, d.Loc.BATTLE, (_UNIT, d.StatCmp(d.Stat.REMAINING_HP, op, amount)))


def _rest_enemy_when_paired(max_hp: int) -> d.Triggered:
    return d.Triggered(
        d.Trigger(d.Ev.PAIRED),
        (d.Choose("t1", _enemy_units_with_current_hp(d.Op.LE, max_hp)), d.Rest(d.Var("t1"))),
    )


@card("GD01-002")
def gd01_002(c: CardDef) -> d.CardScript:
    """Unicorn Gundam (Destroy Mode): the "When playing this card from your hand" line is an
    alternative way to play the card (Q120: ignore Lv. and pay 0), not a trigger."""
    lv5_unicorn_mode_link_unit = d.Sel(
        d.Side.FRIENDLY,
        d.Loc.BATTLE,
        (
            d.IsKind((d.CardKind.LINK_UNIT,)),
            d.NameContains(("Unicorn Mode",)),
            d.StatCmp(d.Stat.LV, d.Op.EQ, 5),
        ),
    )
    play_for_free = d.PlayModifier(
        costs=(d.DestroyCards(lv5_unicorn_mode_link_unit, 1),), level=0, cost=0
    )
    rest_on_attack = d.Triggered(
        d.Trigger(d.Ev.ATTACKS), (d.Choose("t1", _ENEMY_UNITS), d.Rest(d.Var("t1")))
    )
    return d.CardScript(c.card_number, abilities=(play_for_free, rest_on_attack), source="binding")


@card("GD01-003")
def gd01_003(c: CardDef) -> d.CardScript:
    """Unicorn Gundam 02 Banshee (Destroy Mode): "It gains <First Strike>" refers to this Unit
    (the compiler bound it to the returned trash cards), both follow "If you do", and the effect
    does nothing unless 12 cards can be chosen (Q121)."""
    trash = d.Sel(d.Side.FRIENDLY, d.Loc.TRASH)
    steps: tuple[d.Step, ...] = (
        d.If(
            d.Cmp(d.Count(trash), d.Op.GE, 12),
            (
                d.Choose("t1", trash, 12),
                d.ToDeck(d.Var("t1"), shuffle=True),
                d.IfYouDo(
                    (d.SetActive(d.This()), d.Apply(d.This(), d.KeywordGrant(d.Kw.FIRST_STRIKE)))
                ),
            ),
        ),
    )
    attack = d.Triggered(d.Trigger(d.Ev.ATTACKS), steps, gate=d.Gate.LINKED)
    return d.CardScript(c.card_number, abilities=(attack,), source="binding")


@card("GD01-004")
def gd01_004(c: CardDef) -> d.CardScript:
    """Guncannon: the compiled HP filter read printed HP instead of current HP (FAQ Q96)."""
    return d.CardScript(
        c.card_number,
        abilities=(d.Keyword(d.Kw.REPAIR, 1), _rest_enemy_when_paired(2)),
        source="binding",
    )


@card("GD01-005")
def gd01_005(c: CardDef) -> d.CardScript:
    """Unicorn Gundam (Unicorn Mode): when the 【Destroyed】 effect resolves the Unit is already
    in the trash with its Pilot, so "this Unit's paired Pilot" is the Pilot recorded on the
    destroy event (rule 13-2-8-2-1), not the current pairing."""
    destroyed = d.Triggered(
        d.Trigger(d.Ev.DESTROYED),
        (d.ReturnToHand(d.EventCard("pilot")), d.Discard()),
        gate=d.Gate.LINKED,
    )
    return d.CardScript(c.card_number, abilities=(destroyed,), source="binding")


@card("GD01-010", "GD01-012")
def gd01_010_012(c: CardDef) -> d.CardScript:
    """Banshee (Unicorn Mode) / Zechs' Leo: current-HP filter (FAQ Q96)."""
    return d.CardScript(c.card_number, abilities=(_rest_enemy_when_paired(3),), source="binding")


@card("GD01-026")
def gd01_026(c: CardDef) -> d.CardScript:
    """Char's Zaku Ⅱ: token deployment is not matched by the compiler's token template."""
    destroyed = d.Triggered(
        d.Trigger(d.Ev.DESTROYED),
        (d.DeployToken(_token_key(c), 1, rested=True),),
        gate=d.Gate.PAIRED,
    )
    return d.CardScript(c.card_number, abilities=(destroyed,), source="binding")


@card("GD01-046")
def gd01_046(c: CardDef) -> d.CardScript:
    """Buster Gundam: the <Support> usage trigger has no compiler template."""
    set_active_after_support = d.Triggered(
        d.Trigger(d.Ev.SUPPORT_USED, target_filters=(_UNIT, d.HasTrait(("ZAFT",)))),
        (d.SetActive(d.This()),),
        once_per_turn=True,
        gate=d.Gate.PAIRED,
        gate_filters=(d.HasTrait(("Coordinator",)),),
    )
    return d.CardScript(
        c.card_number,
        abilities=(d.Keyword(d.Kw.SUPPORT, 3), set_active_after_support),
        source="binding",
    )


@card("GD01-063")
def gd01_063(c: CardDef) -> d.CardScript:
    """ZnO: "while this Unit is battling an enemy Unit that is Lv.2 or lower" has no compiler
    template; the battling Unit is re-read continuously, so a block by a higher-Lv. Unit ends
    the grant (Q136)."""
    battling_low_level_enemy = d.Exists(
        d.Sel(
            d.Side.ENEMY,
            d.Loc.BATTLE,
            (_UNIT, d.StatCmp(d.Stat.LV, d.Op.LE, 2), d.IsRef(d.BattlingWith(d.This()))),
        )
    )
    first_strike = d.Constant(
        (d.KeywordGrant(d.Kw.FIRST_STRIKE),),
        cond=d.And((d.IsTurn(d.P.YOU), battling_low_level_enemy)),
    )
    return d.CardScript(c.card_number, abilities=(first_strike,), source="binding")


@card("GD01-066")
def gd01_066(c: CardDef) -> d.CardScript:
    """Justice Gundam: token deployment is not matched by the compiler's token template."""
    deploy_fatum = d.Triggered(d.Trigger(d.Ev.DEPLOYED), (d.DeployToken(_token_key(c), 1),))
    friendly_tsa_tokens = d.Sel(
        d.Side.FRIENDLY,
        d.Loc.BATTLE,
        (d.IsKind((d.CardKind.UNIT_TOKEN,)), d.HasTrait(("Triple Ship Alliance",))),
    )
    token_may_attack = d.Triggered(
        d.Trigger(d.Ev.ATTACKS),
        (
            d.Choose("t1", friendly_tsa_tokens),
            d.Apply(
                d.Var("t1"),
                d.RuleGrant(d.RuleMod(d.RuleKind.ATTACK_ON_DEPLOY_TURN)),
                Duration.THIS_TURN,
            ),
        ),
        gate=d.Gate.PAIRED,
    )
    return d.CardScript(c.card_number, abilities=(deploy_fatum, token_may_attack), source="binding")


@card("GD01-068")
def gd01_068(c: CardDef) -> d.CardScript:
    """Perfect Strike Gundam: "with 1 HP" is current HP (FAQ Q96)."""
    bounce = d.Triggered(
        d.Trigger(d.Ev.DEPLOYED),
        (
            d.Choose("t1", _enemy_units_with_current_hp(d.Op.EQ, 1)),
            d.ReturnToHand(d.Var("t1")),
        ),
    )
    return d.CardScript(
        c.card_number, abilities=(d.Keyword(d.Kw.BLOCKER), bounce), source="binding"
    )
