"""Bindings for GD04-075..GD04-130 whose compiled text is missing or wrong."""

from __future__ import annotations

from gcg_sim.cards.model import CardDef
from gcg_sim.cards.tokens import parse_token_specs
from gcg_sim.effects import dsl as d
from gcg_sim.effects.bindings import card, custom_cond
from gcg_sim.effects.compiler import route_abilities
from gcg_sim.engine import view as V
from gcg_sim.engine.state import GameState
from gcg_sim.engine.types import Duration

UNIT = d.IsKind((d.CardKind.UNIT,))
BURST_ADD_TO_HAND = d.Burst((d.AddToHand(d.ThisCard()),))
BURST_DEPLOY = d.Burst((d.DeployCard(d.ThisCard()),))
DEPLOY_SHIELD_TO_HAND = d.Triggered(d.Trigger(d.Ev.DEPLOYED), (d.ShieldToHand(),))
ENEMY_UNIT_IN_PLAY = d.IsRef(d.All(d.Sel(d.Side.ENEMY, d.Loc.BATTLE, (UNIT,))))
NO_EX_RESOURCES = d.Cmp(
    d.Count(d.Sel(d.Side.FRIENDLY, d.Loc.RESOURCE_AREA, (d.IsKind((d.CardKind.EX_RESOURCE,)),))),
    d.Op.EQ,
    0,
)
PLAYED_WITH_EX = d.CustomCond("wp_gd04_b_played_with_ex")
FRIENDLY_UNIT_COST_PAID = d.Trigger(
    d.Ev.COST_PAID, self_only=False, subject=d.Sel(d.Side.FRIENDLY, d.Loc.BATTLE, (UNIT,))
)


def _friendly_units(*filters: d.Filter) -> d.Sel:
    return d.Sel(d.Side.FRIENDLY, d.Loc.BATTLE, (UNIT, *filters))


def _enemy_units(*filters: d.Filter) -> d.Sel:
    return d.Sel(d.Side.ENEMY, d.Loc.BATTLE, (UNIT, *filters))


def _token_key(c: CardDef) -> str:
    (spec,) = parse_token_specs(c.effect)
    return spec.key


def _reduce_next_damage(amount: int) -> d.RuleGrant:
    return d.RuleGrant(d.RuleMod(d.RuleKind.REDUCE_DAMAGE, amount=amount, once_per_turn=True))


def _script(c: CardDef, abilities: list[d.Ability], notes: str) -> d.CardScript:
    own, unit = route_abilities(c, abilities)
    return d.CardScript(c.card_number, own, unit, source="binding", notes=notes)


@custom_cond("wp_gd04_b_played_with_ex")
def played_with_ex(st: GameState, dv: V.Derived, ctx: V.Ctx, params: dict[str, object]) -> bool:
    """Whether the most recent play of this card from the hand used an EX Resource."""
    uid = ctx.card_uid
    history = st.history
    for i in range(len(history) - 1, -1, -1):
        h = history[i]
        if h.kind == "play" and h.uid == uid:
            nxt = history[i + 1] if i + 1 < len(history) else None
            return nxt is not None and nxt.kind == "played_with_ex" and nxt.uid == uid
    return False


@card("GD04-075")
def gd04_075(c: CardDef) -> d.CardScript:
    un_commands = d.Sel(
        d.Side.FRIENDLY,
        d.Loc.TRASH,
        (d.IsKind((d.CardKind.COMMAND,)), d.HasTrait(("UN", "Superpower Bloc"))),
    )
    reduction = d.CostMod(cost=d.Times(d.Count(un_commands), -1))
    return _script(
        c,
        [d.Constant((reduction,), d.This(), where=d.Where.HAND)],
        "hand cost reduction scaled by a trash count",
    )


@card("GD04-080")
def gd04_080(c: CardDef) -> d.CardScript:
    other_un = _friendly_units(d.HasTrait(("UN", "Superpower Bloc")), d.NotRef(d.This()))
    deploy = d.DeployToken(_token_key(c), 1, rested=True)
    return _script(
        c,
        [d.Triggered(d.Trigger(d.Ev.DESTROYED), (d.If(d.Exists(other_un), (deploy,)),))],
        "token definition not matched by the grammar",
    )


@card("GD04-081")
def gd04_081(c: CardDef) -> d.CardScript:
    is_league = d.RefMatches(d.This(), (d.HasTrait(("League Militaire",)),))
    when_paired = d.Triggered(
        d.Trigger(d.Ev.PAIRED), (d.If(is_league, (d.DeployToken(_token_key(c), 1),)),)
    )
    return _script(c, [BURST_ADD_TO_HAND, when_paired], "token definition not matched")


@card("GD04-085")
def gd04_085(c: CardDef) -> d.CardScript:
    academy_command = d.Sel(
        d.Side.FRIENDLY, d.Loc.TRASH, (d.IsKind((d.CardKind.COMMAND,)), d.HasTrait(("Academy",)))
    )
    trigger = d.Trigger(d.Ev.COMMAND_PLAYED, self_only=False, subject=academy_command)
    ability = d.Triggered(
        trigger,
        (d.If(NO_EX_RESOURCES, (d.PlaceExResource(rested=True),)),),
        cond=d.And((d.EventFlag("ex_used"), NO_EX_RESOURCES)),
        once_per_turn=True,
        gate=d.Gate.LINKED,
    )
    return _script(c, [BURST_ADD_TO_HAND, ability], "EX-Resource command-play trigger")


@card("GD04-086")
def gd04_086(c: CardDef) -> d.CardScript:
    destroyed = d.Triggered(
        d.Trigger(d.Ev.DESTROYED),
        (d.If(NO_EX_RESOURCES, (d.PlaceExResource(),)),),
        gate=d.Gate.LINKED,
    )
    return _script(c, [BURST_ADD_TO_HAND, destroyed], "conditional EX Resource placement")


@card("GD04-087")
def gd04_087(c: CardDef) -> d.CardScript:
    redirect = d.RuleGrant(d.RuleMod(d.RuleKind.REDIRECT_BATTLE_DAMAGE))
    attack = d.Triggered(
        d.Trigger(d.Ev.ATTACKS),
        (
            d.Choose("t1", _friendly_units(d.HasTrait(("Academy",))), optional=True),
            d.Apply(d.This(), redirect, Duration.THIS_BATTLE, aux=d.Var("t1")),
        ),
        gate=d.Gate.LINKED,
    )
    return _script(c, [BURST_ADD_TO_HAND, attack], "battle-damage redirect to the chosen Unit")


@card("GD04-088")
def gd04_088(c: CardDef) -> d.CardScript:
    no_battle_damage = d.RuleGrant(
        d.RuleMod(d.RuleKind.CANT_RECEIVE_DAMAGE, damage_kind=d.DamageKind.BATTLE)
    )
    blocked = d.Triggered(
        d.Trigger(d.Ev.BLOCKED),
        (d.Apply(d.This(), no_battle_damage, Duration.THIS_BATTLE),),
        cond=d.RefMatches(d.EventCard("blocker"), (UNIT, d.StatCmp(d.Stat.LV, d.Op.LE, 4))),
    )
    return _script(c, [BURST_ADD_TO_HAND, blocked], "blocked-by trigger with blocker filter")


@card("GD04-095")
def gd04_095(c: CardDef) -> d.CardScript:
    redirect = d.RuleGrant(d.RuleMod(d.RuleKind.REDIRECT_BATTLE_DAMAGE))
    linked = d.Triggered(
        d.Trigger(d.Ev.LINKED),
        (
            d.Choose("t1", _friendly_units(d.HasTrait(("Minerva Squad",)))),
            d.Apply(d.Var("t1"), redirect, Duration.THIS_TURN, aux=d.This()),
        ),
    )
    return _script(c, [BURST_ADD_TO_HAND, linked], "battle-damage redirect to this Unit")


@card("GD04-096")
def gd04_096(c: CardDef) -> d.CardScript:
    deals = d.Trigger(
        d.Ev.DEALS_DAMAGE,
        battle_only=True,
        target_filters=(ENEMY_UNIT_IN_PLAY, d.StatCmp(d.Stat.LV, d.Op.LE, 5)),
    )
    ability = d.Triggered(deals, (d.Destroy(d.EventCard("target")),), gate=d.Gate.LINKED)
    return _script(c, [BURST_ADD_TO_HAND, ability], "deals-battle-damage trigger")


@card("GD04-100")
def gd04_100(c: CardDef) -> d.CardScript:
    ability = d.Triggered(
        FRIENDLY_UNIT_COST_PAID,
        (d.May((d.Repeat(d.EventAmount("amount"), (d.Apply(d.This(), d.StatMod(ap=1)),)),)),),
        cond=d.Cmp(d.EventAmount("amount"), d.Op.GE, 1),
        once_per_turn=True,
    )
    return _script(
        c, [BURST_ADD_TO_HAND, ability], "cost-paid trigger; AP bonus fixed at resolution"
    )


@card("GD04-106")
def gd04_106(c: CardDef) -> d.CardScript:
    academy = _friendly_units(d.HasTrait(("Academy",)))
    may_attack_active = d.RuleGrant(
        d.RuleMod(
            d.RuleKind.MAY_ATTACK_ACTIVE,
            source_filters=(UNIT, d.IsRested(False), d.StatCmp(d.Stat.AP, d.Op.LE, 5)),
        )
    )
    main = d.Command(
        d.Timing.MAIN,
        (
            d.Choose("t1", academy),
            d.If(
                PLAYED_WITH_EX,
                (d.Choose("t2", academy, 1, min_count=0, distinct_from=("t1",)),),
            ),
            d.Apply(d.Union((d.Var("t1"), d.Var("t2"))), may_attack_active),
        ),
    )
    return _script(c, [main], "EX-Resource variant chose Units but granted nothing")


@card("GD04-107")
def gd04_107(c: CardDef) -> d.CardScript:
    must_attack = d.RuleGrant(d.RuleMod(d.RuleKind.FORCE_ATTACK_TARGET, name="must"))
    action = d.Command(
        d.Timing.ACTION,
        (
            d.Choose("t1", _friendly_units(d.IsRested())),
            d.Apply(d.Var("t1"), must_attack),
        ),
    )
    return _script(c, [BURST_ADD_TO_HAND, action], "compiled rule targeted the event card")


@card("GD04-108")
def gd04_108(c: CardDef) -> d.CardScript:
    command = d.Command(
        d.Timing.MAIN_OR_ACTION,
        (
            d.Choose("t1", _friendly_units(d.HasTrait(("Academy",)))),
            d.If(
                PLAYED_WITH_EX,
                (d.Apply(d.Var("t1"), _reduce_next_damage(4)),),
                (d.Apply(d.Var("t1"), _reduce_next_damage(2)),),
            ),
        ),
    )
    return _script(c, [command], "EX-Resource override of the reduction amount")


@card("GD04-115")
def gd04_115(c: CardDef) -> d.CardScript:
    burst = d.Burst(
        (d.Choose("t1", _enemy_units()), d.Damage(d.Var("t1"), 1)),
    )
    main = d.Command(
        d.Timing.MAIN,
        (
            d.Choose("t1", _friendly_units()),
            d.Apply(d.Var("t1"), d.AbilityGrant(c.card_number, 2)),
        ),
    )
    granted = d.Triggered(
        d.Trigger(
            d.Ev.DEALS_DAMAGE,
            battle_only=True,
            target_filters=(ENEMY_UNIT_IN_PLAY, d.StatCmp(d.Stat.LV, d.Op.LE, 5)),
        ),
        (d.Destroy(d.EventCard("target")),),
    )
    return _script(c, [burst, main, granted], "turn-long granted deals-battle-damage trigger")


@card("GD04-116")
def gd04_116(c: CardDef) -> d.CardScript:
    milled_minerva = d.Count(
        d.Sel(
            d.Side.FRIENDLY,
            d.Loc.TRASH,
            (d.IsRef(d.Var("milled")), d.HasTrait(("Minerva Squad",))),
        )
    )
    main = d.Command(
        d.Timing.MAIN,
        (
            d.Mill(2),
            d.IfYouDo(
                (
                    d.Choose("t1", _enemy_units(d.StatCmp(d.Stat.AP, d.Op.LE, 4)), after_then=True),
                    d.Damage(d.Var("t1"), milled_minerva),
                )
            ),
        ),
    )
    return _script(c, [main], "mill then damage scaled by milled (Minerva Squad) cards")


@card("GD04-121")
def gd04_121(c: CardDef) -> d.CardScript:
    league_in_play = d.Exists(_friendly_units(d.HasTrait(("League Militaire",))))
    deploy = d.Triggered(
        d.Trigger(d.Ev.DEPLOYED),
        (
            d.ShieldToHand(),
            d.If(
                d.And((d.IsTurn(d.P.YOU), league_in_play)),
                (d.DeployToken(_token_key(c), 1),),
            ),
        ),
    )
    return _script(c, [BURST_DEPLOY, deploy], "token definition not matched")


@card("GD04-126")
def gd04_126(c: CardDef) -> d.CardScript:
    received = d.Triggered(
        d.Trigger(d.Ev.DAMAGED, battle_only=True, by_enemy=True),
        (d.Damage(d.EventCard("source"), 1),),
        cond=d.RefMatches(d.EventCard("source"), (UNIT, d.StatCmp(d.Stat.AP, d.Op.LE, 3))),
    )
    return _script(
        c, [BURST_DEPLOY, DEPLOY_SHIELD_TO_HAND, received], "receives-battle-damage trigger"
    )


@card("GD04-129")
def gd04_129(c: CardDef) -> d.CardScript:
    deploy = d.Triggered(d.Trigger(d.Ev.DEPLOYED), (d.ShieldToHand(), d.Damage(d.This(), 3)))
    trigger = d.Trigger(
        d.Ev.COST_PAID,
        self_only=False,
        subject=d.Sel(d.Side.FRIENDLY, d.Loc.BATTLE, (UNIT,)),
        whose_turn=d.P.YOU,
    )
    recover = d.Triggered(trigger, (d.Recover(d.This(), 2),), once_per_turn=True)
    return _script(c, [BURST_DEPLOY, deploy, recover], "cost-paid trigger")
