"""Card bindings for GD02-086..GD02-130 (work package WP-GD02-B).

Only cards whose compiled script is missing or differs from the card text, its rulings, or the
conflict resolutions are bound; each docstring names what the compiler gets wrong.
"""

from __future__ import annotations

from gcg_sim.cards.model import CardDef
from gcg_sim.effects import dsl as d
from gcg_sim.effects.bindings import card, custom_step
from gcg_sim.effects.compiler import compile_parts
from gcg_sim.effects.registry import UnimplementedCardError
from gcg_sim.engine import core
from gcg_sim.engine import interp as I
from gcg_sim.engine import view as V
from gcg_sim.engine.state import Frame, GameState
from gcg_sim.engine.types import Duration, Zone

F = d.Side.FRIENDLY
E = d.Side.ENEMY
UNIT = d.IsKind((d.CardKind.UNIT,))
PILOT = d.IsKind((d.CardKind.PILOT,))

PAY_DEPLOY_COST = "wp_gd02_b_pay_deploy_cost"


def _line(c: CardDef, index: int) -> tuple[d.Ability, ...]:
    line, res = compile_parts(c)[index]
    if not isinstance(res, list):
        raise UnimplementedCardError(f"{line!r}: {res}")
    return tuple(res)


def _looked(*filters: d.Filter) -> d.Sel:
    return d.Sel(F, d.Loc.DECK, (*filters, d.IsRef(d.Var("looked"))))


def _reveal_one_from_top_three(*filters: d.Filter) -> tuple[d.Step, ...]:
    """Look at the top 3, optionally reveal one matching card into the hand, bottom the rest
    randomly (Q474: the look is mandatory, adding is optional)."""
    return (
        d.LookTop(3),
        d.May(
            (
                d.Choose("t1", _looked(*filters), targeting=False),
                d.AddToHand(d.Var("t1"), reveal=True),
            )
        ),
        d.CustomStep("return_looked_bottom"),
    )


@custom_step(PAY_DEPLOY_COST)
def pay_deploy_cost(st: GameState, f: Frame, ctx: V.Ctx, params: dict[str, object]) -> bool:
    """ "Pay its cost to deploy it" (resolution of ambiguous:pay-its-cost-to-deploy): rest
    Resources equal to the card's cost as modified in the trash, with no Lv. check and no
    play-cost modifiers; the payment counts as a cost paid for this effect (Q286)."""
    cards = [u for u in f.vars.get(str(params["var"]), ()) if st.cards[u].zone is Zone.TRASH]
    if not cards:
        return False
    uid = cards[0]
    cost = max(0, V.cost_of(st, uid) + V.derived(st).cost_mod.get(uid, 0))
    if not I.pay_generic(st, f.controller, cost):
        return False
    if cost > 0 and f.host >= 0:
        core.record(st, "cost_paid", f.controller, f.controller, f.host)
        core.emit(st, d.Ev.COST_PAID, f.host, player=f.controller, amount=cost)
    return True


def _pay_and_deploy(var: str) -> tuple[d.Step, ...]:
    return (
        d.CustomStep(PAY_DEPLOY_COST, (("var", var),)),
        d.IfYouDo((d.DeployCard(d.Var(var)),)),
    )


@card("GD02-088")
def gd02_088(c: CardDef) -> d.CardScript:
    """Flit Asuno: the "Unit card/1 card with "AGE Device" in its card name" alternative does
    not compile."""
    green_ef_unit = d.AllOf((UNIT, d.HasColor(("Green",)), d.HasTrait(("Earth Federation",))))
    age_device = d.NameContains(("AGE Device",))
    linked = d.Triggered(
        d.Trigger(d.Ev.LINKED),
        _reveal_one_from_top_three(d.AnyOf((green_ef_unit, age_device))),
    )
    return d.CardScript(
        c.card_number, abilities=_line(c, 0), unit_abilities=(linked,), source="binding"
    )


@card("GD02-093")
def gd02_093(c: CardDef) -> d.CardScript:
    """Olba Frost: the trigger does not compile. The victim's Pilot is only reported by the
    victim's destruction event, so the trigger listens for an enemy Unit's battle destruction
    while it battles this Unit (Q190: also when both Units are destroyed)."""
    trigger = d.Trigger(
        d.Ev.DESTROYED,
        self_only=False,
        subject=d.Sel(E, d.Loc.BATTLE, (UNIT,)),
        whose_turn=d.P.YOU,
        pilot_filters=(d.HasTrait(("Newtype",)),),
        battle_only=True,
    )
    battled_this_unit = d.And(
        (
            d.RefMatches(d.EventCard("subject"), (d.IsBattling(),)),
            d.RefMatches(d.This(), (d.IsBattling(),)),
        )
    )
    draw = d.Triggered(trigger, (d.Draw(1),), cond=battled_this_unit)
    return d.CardScript(
        c.card_number, abilities=_line(c, 0), unit_abilities=(draw,), source="binding"
    )


@card("GD02-094")
def gd02_094(c: CardDef) -> d.CardScript:
    """Garrod Ran & Tiffa Adill: the compiled look/reveal sits outside "If you do", so declining
    the discard still asks the reveal question (Q175 scope)."""
    paired = d.Triggered(
        d.Trigger(d.Ev.PAIRED),
        (
            d.May((d.Discard(1),)),
            d.IfYouDo(_reveal_one_from_top_three(UNIT, d.HasTrait(("Vulture",)))),
        ),
    )
    return d.CardScript(
        c.card_number, abilities=_line(c, 0), unit_abilities=(paired,), source="binding"
    )


@card("GD02-095")
def gd02_095(c: CardDef) -> d.CardScript:
    """Lafter Frankland: "If this Unit is damaged and Lv.5 or lower" does not compile."""
    attack = d.Triggered(
        d.Trigger(d.Ev.ATTACKS),
        (
            d.If(
                d.RefMatches(d.This(), (d.IsDamaged(), d.StatCmp(d.Stat.LV, d.Op.LE, 5))),
                (d.Apply(d.This(), d.KeywordGrant(d.Kw.HIGH_MANEUVER), Duration.THIS_BATTLE),),
            ),
        ),
    )
    return d.CardScript(
        c.card_number, abilities=_line(c, 0), unit_abilities=(attack,), source="binding"
    )


@card("GD02-096")
def gd02_096(c: CardDef) -> d.CardScript:
    """Desil Galette: the compiled PlayCard plays the card (play-cost modifiers apply and the
    payment is not a cost paid for this Unit's effect) instead of paying and deploying."""
    vagan = d.Sel(F, d.Loc.TRASH, (UNIT, d.HasTrait(("Vagan",)), d.StatCmp(d.Stat.LV, d.Op.LE, 2)))
    linked = d.Triggered(
        d.Trigger(d.Ev.LINKED),
        (d.Choose("t1", vagan, optional=True), *_pay_and_deploy("t1")),
    )
    return d.CardScript(
        c.card_number, abilities=_line(c, 0), unit_abilities=(linked,), source="binding"
    )


@card("GD02-098")
def gd02_098(c: CardDef) -> d.CardScript:
    """Quattro Bajeena: the name alias needs a binding (it stays with the Pilot card, Q191), and
    the compiled 【When Linked】 discards even when "If this is an (AEUG) Unit" fails."""
    linked = d.Triggered(
        d.Trigger(d.Ev.LINKED),
        (
            d.If(
                d.RefMatches(d.This(), (UNIT, d.HasTrait(("AEUG",)))),
                (d.Draw(1), d.IfYouDo((d.Discard(1),))),
            ),
        ),
    )
    return d.CardScript(
        c.card_number,
        abilities=(d.NameAlias(("Char Aznable",)), *_line(c, 1)),
        unit_abilities=(linked,),
        source="binding",
    )


@card("GD02-104")
def gd02_104(c: CardDef) -> d.CardScript:
    """Turning Point of History: "return 1 to the top. Return the remaining cards to the bottom"
    does not compile; the owner orders the bottomed cards (4-1-7)."""
    newtype_pilot = d.Sel(F, d.Loc.PAIRED, (PILOT, d.HasTrait(("Newtype",))))
    main = d.Command(
        d.Timing.MAIN,
        (
            d.LookTop(3),
            d.Choose("top", _looked(), targeting=False),
            d.Choose("rest", _looked(d.NotRef(d.Var("top"))), count=2, targeting=False),
            d.Arrange(d.Var("rest"), "bottom_any_order"),
            d.If(d.Exists(newtype_pilot), (d.Draw(1),)),
        ),
    )
    return d.CardScript(c.card_number, abilities=(main,), source="binding")


@card("GD02-110")
def gd02_110(c: CardDef) -> d.CardScript:
    """Awakened Power: the compiled PlayCard plays the card (play-cost modifiers apply) instead
    of paying its cost and deploying it by effect (Q193: its 【Deploy】 triggers)."""
    lv5 = d.Sel(F, d.Loc.TRASH, (UNIT, d.StatCmp(d.Stat.LV, d.Op.LE, 5)))
    main = d.Command(d.Timing.MAIN, (d.Choose("t1", lv5), *_pay_and_deploy("t1")))
    return d.CardScript(c.card_number, abilities=(main,), source="binding")


@card("GD02-111")
def gd02_111(c: CardDef) -> d.CardScript:
    """Decisive Last Resort: the compiled Command is playable with fewer than 6 valid cards and
    then exiles fewer; choosing 6 cards needs 6 (Q121 resolution, 10-1-8-1-2)."""
    purple_units = d.Sel(F, d.Loc.TRASH, (UNIT, d.HasColor(("Purple",))))
    main = d.Command(
        d.Timing.MAIN,
        (
            d.Choose("t1", purple_units, count=6),
            d.Exile(d.Var("t1")),
            d.IfYouDo(
                (
                    d.Choose("t2", d.Sel(E, d.Loc.BATTLE, (UNIT,))),
                    d.Destroy(d.Var("t2")),
                )
            ),
        ),
        cond=d.Cmp(d.Count(purple_units), d.Op.GE, 6),
    )
    return d.CardScript(c.card_number, abilities=(*_line(c, 0), main), source="binding")


@card("GD02-118")
def gd02_118(c: CardDef) -> d.CardScript:
    """Heart Set on Revenge: "battling a friendly Unit with <Blocker>" does not compile. With a
    single battle at a time, the enemy Unit battles a friendly <Blocker> Unit exactly when both
    are battling; HP means current HP (FAQ Q96)."""
    friendly_blocker_battling = d.Exists(
        d.Sel(F, d.Loc.BATTLE, (UNIT, d.IsBattling(), d.HasKeyword(d.Kw.BLOCKER)))
    )
    target = d.Sel(
        E, d.Loc.BATTLE, (UNIT, d.IsBattling(), d.StatCmp(d.Stat.REMAINING_HP, d.Op.LE, 4))
    )
    action = d.Command(
        d.Timing.ACTION,
        (d.Choose("t1", target), d.ReturnToHand(d.Var("t1"))),
        cond=friendly_blocker_battling,
    )
    return d.CardScript(c.card_number, abilities=(action,), source="binding")


@card("GD02-124")
def gd02_124(c: CardDef) -> d.CardScript:
    """Diva: the "during your turn, while you are Lv.7 or higher" constant does not compile."""
    green_ef = d.All(
        d.Sel(F, d.Loc.BATTLE, (UNIT, d.HasColor(("Green",)), d.HasTrait(("Earth Federation",))))
    )
    aura = d.Constant(
        (d.StatMod(ap=1),),
        scope=green_ef,
        cond=d.And((d.IsTurn(d.P.YOU), d.Cmp(d.PlayerLevel(d.P.YOU), d.Op.GE, 7))),
    )
    return d.CardScript(
        c.card_number, abilities=(*_line(c, 0), *_line(c, 1), aura), source="binding"
    )


@card("GD02-125")
def gd02_125(c: CardDef) -> d.CardScript:
    """Gwadan: the compiled "If you do, draw 1" sits outside "if it is your turn", so a 【Burst】
    deploy on the opponent's turn draws."""
    deploy = d.Triggered(
        d.Trigger(d.Ev.DEPLOYED),
        (
            d.ShieldToHand(1),
            d.If(
                d.IsTurn(d.P.YOU),
                (
                    d.May((d.Discard(1, filters=(d.HasColor(("Red",)),)),)),
                    d.IfYouDo((d.Draw(1),)),
                ),
            ),
        ),
    )
    return d.CardScript(c.card_number, abilities=(*_line(c, 0), deploy), source="binding")


@card("GD02-127")
def gd02_127(c: CardDef) -> d.CardScript:
    """Freeden: "Place the top 2 cards of your deck into your trash." does not compile."""
    destroyed = d.Triggered(d.Trigger(d.Ev.DESTROYED), (d.Mill(2),))
    return d.CardScript(
        c.card_number, abilities=(*_line(c, 0), *_line(c, 1), destroyed), source="binding"
    )
