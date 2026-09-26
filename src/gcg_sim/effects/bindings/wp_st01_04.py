"""Bindings for the starter decks ST01-ST04 (work package WP-ST01-04).

HP filters use the Unit's current HP, modified HP minus damage (rules FAQ Q96). Token
definitions are read from each card's own text so their keys match the card database.
"""

from __future__ import annotations

from gcg_sim.cards.model import CardDef
from gcg_sim.cards.tokens import parse_token_specs
from gcg_sim.effects import dsl as d
from gcg_sim.effects.bindings import card

_UNIT = d.IsKind((d.CardKind.UNIT,))
_BURST_ADD_TO_HAND = d.Burst((d.AddToHand(d.ThisCard()),))
_BURST_DEPLOY = d.Burst((d.DeployCard(d.ThisCard()),))
_BURST_ACTIVATE_MAIN = d.Burst((d.ActivateMain(),))
_DEPLOY_SHIELD_TO_HAND = d.Triggered(d.Trigger(d.Ev.DEPLOYED), (d.ShieldToHand(),))
_CURRENT_HP_NOTE = "HP filter uses current HP (FAQ Q96)."


def _tokens(c: CardDef) -> dict[str, str]:
    return {spec.name: spec.key for spec in parse_token_specs(c.effect)}


def _enemy_unit_with_hp_at_most(hp: int) -> d.Sel:
    return d.Sel(d.Side.ENEMY, d.Loc.BATTLE, (_UNIT, d.StatCmp(d.Stat.REMAINING_HP, d.Op.LE, hp)))


def _choose_enemy_by_hp(hp: int, action: d.Step) -> tuple[d.Step, ...]:
    return (d.Choose("t1", _enemy_unit_with_hp_at_most(hp)), action)


@card("ST01-004")
def st01_004(c: CardDef) -> d.CardScript:
    deploy = d.Triggered(d.Trigger(d.Ev.DEPLOYED), _choose_enemy_by_hp(2, d.Rest(d.Var("t1"))))
    return d.CardScript(c.card_number, (deploy,), source="binding", notes=_CURRENT_HP_NOTE)


@card("ST01-010")
def st01_010(c: CardDef) -> d.CardScript:
    when_paired = d.Triggered(d.Trigger(d.Ev.PAIRED), _choose_enemy_by_hp(5, d.Rest(d.Var("t1"))))
    return d.CardScript(
        c.card_number,
        abilities=(_BURST_ADD_TO_HAND,),
        unit_abilities=(when_paired,),
        source="binding",
        notes=_CURRENT_HP_NOTE,
    )


@card("ST01-015")
def st01_015(c: CardDef) -> d.CardScript:
    tokens = _tokens(c)
    units_in_play = d.Count(d.Sel(d.Side.FRIENDLY, d.Loc.BATTLE, (_UNIT,)))
    deploy_by_unit_count = d.If(
        d.Cmp(units_in_play, d.Op.EQ, 0),
        (d.DeployToken(tokens["Gundam"]),),
        (
            d.If(
                d.Cmp(units_in_play, d.Op.EQ, 1),
                (d.DeployToken(tokens["Guncannon"]),),
                (d.DeployToken(tokens["Guntank"]),),
            ),
        ),
    )
    activate = d.Activated(
        d.Timing.MAIN, (d.PayResources(2),), (deploy_by_unit_count,), once_per_turn=True
    )
    return d.CardScript(
        c.card_number,
        (_BURST_DEPLOY, _DEPLOY_SHIELD_TO_HAND, activate),
        source="binding",
        notes="Token chosen by the number of your Units in play when the effect resolves.",
    )


@card("ST02-014")
def st02_014(c: CardDef) -> d.CardScript:
    command = d.Command(d.Timing.MAIN_OR_ACTION, _choose_enemy_by_hp(5, d.Rest(d.Var("t1"))))
    return d.CardScript(
        c.card_number, (_BURST_ACTIVATE_MAIN, command), source="binding", notes=_CURRENT_HP_NOTE
    )


@card("ST02-015")
def st02_015(c: CardDef) -> d.CardScript:
    looked = d.Sel(d.Side.FRIENDLY, d.Loc.DECK, (d.IsRef(d.Var("looked")),))
    deploy = d.Triggered(
        d.Trigger(d.Ev.DEPLOYED),
        (
            d.ShieldToHand(),
            d.LookTop(2),
            d.Choose("bottom", looked, targeting=False),
            d.ToDeck(d.Var("bottom"), bottom=True),
        ),
    )
    return d.CardScript(
        c.card_number,
        (_BURST_DEPLOY, deploy),
        source="binding",
        notes="The chosen looked card goes to the bottom; the other stays on top.",
    )


@card("ST02-016")
def st02_016(c: CardDef) -> d.CardScript:
    tokens = _tokens(c)
    corsica_in_trash = d.Exists(
        d.Sel(d.Side.FRIENDLY, d.Loc.TRASH, (d.NameContains(("Corsica Base",)),))
    )
    tokens_on_your_turn = d.If(
        d.IsTurn(d.P.YOU),
        (
            d.If(
                corsica_in_trash,
                (d.DeployToken(tokens["Leo"], 2),),
                (d.DeployToken(tokens["Tallgeese"], 1),),
            ),
        ),
    )
    deploy = d.Triggered(d.Trigger(d.Ev.DEPLOYED), (d.ShieldToHand(), tokens_on_your_turn))
    return d.CardScript(
        c.card_number,
        (_BURST_DEPLOY, deploy),
        source="binding",
        notes="'instead' replaces the Tallgeese token with two Leo tokens.",
    )


@card("ST03-009")
def st03_009(c: CardDef) -> d.CardScript:
    token = _tokens(c)["Zaku Ⅱ"]
    deploy = d.Triggered(d.Trigger(d.Ev.DEPLOYED), (d.DeployToken(token, 1, rested=True),))
    return d.CardScript(c.card_number, (deploy,), source="binding", notes="Unit token deploy.")


@card("ST03-011")
def st03_011(c: CardDef) -> d.CardScript:
    attack = d.Triggered(
        d.Trigger(d.Ev.ATTACKS),
        (
            d.Apply(d.This(), d.StatMod(ap=1)),
            d.If(
                d.RefMatches(d.This(), (d.IsLinked(),)),
                (d.Apply(d.This(), d.KeywordGrant(d.Kw.HIGH_MANEUVER)),),
            ),
        ),
    )
    return d.CardScript(
        c.card_number,
        abilities=(_BURST_ADD_TO_HAND,),
        unit_abilities=(attack,),
        source="binding",
        notes="AP+1 and, while resolving as a Link Unit, <High-Maneuver>; both this turn.",
    )


@card("ST03-016")
def st03_016(c: CardDef) -> d.CardScript:
    token = _tokens(c)["Char's Zaku Ⅱ"]
    deploy = d.Triggered(
        d.Trigger(d.Ev.DEPLOYED),
        (
            d.ShieldToHand(),
            d.If(d.IsTurn(d.P.YOU), (d.DeployToken(token, 1, rested=True),)),
        ),
    )
    return d.CardScript(
        c.card_number, (_BURST_DEPLOY, deploy), source="binding", notes="Unit token deploy."
    )


@card("ST04-001")
def st04_001(c: CardDef) -> d.CardScript:
    when_paired = d.Triggered(
        d.Trigger(d.Ev.PAIRED, pilot_filters=(d.StatCmp(d.Stat.LV, d.Op.GE, 4),)),
        _choose_enemy_by_hp(4, d.ReturnToHand(d.Var("t1"))),
    )
    return d.CardScript(
        c.card_number,
        (d.Keyword(d.Kw.BLOCKER), when_paired),
        source="binding",
        notes=_CURRENT_HP_NOTE,
    )


@card("ST04-012")
def st04_012(c: CardDef) -> d.CardScript:
    tokens = _tokens(c)
    no_earth_alliance_tokens = d.NotC(
        d.Exists(
            d.Sel(
                d.Side.FRIENDLY,
                d.Loc.BATTLE,
                (d.IsKind((d.CardKind.UNIT_TOKEN,)), d.HasTrait(("Earth Alliance",))),
            )
        )
    )
    burst = d.Burst(
        (d.If(no_earth_alliance_tokens, (d.DeployToken(tokens["Aile Strike Gundam"]),)),)
    )
    main = d.Command(
        d.Timing.MAIN,
        (
            d.If(
                no_earth_alliance_tokens,
                (
                    d.ChooseMode(
                        (
                            (
                                "Sword Strike Gundam",
                                (d.DeployToken(tokens["Sword Strike Gundam"]),),
                            ),
                            (
                                "Launcher Strike Gundam",
                                (d.DeployToken(tokens["Launcher Strike Gundam"]),),
                            ),
                        )
                    ),
                ),
            ),
        ),
    )
    return d.CardScript(c.card_number, (burst, main), source="binding", notes="Unit token deploys.")


@card("ST04-013")
def st04_013(c: CardDef) -> d.CardScript:
    command = d.Command(
        d.Timing.MAIN_OR_ACTION, _choose_enemy_by_hp(3, d.ReturnToHand(d.Var("t1")))
    )
    return d.CardScript(c.card_number, (command,), source="binding", notes=_CURRENT_HP_NOTE)
