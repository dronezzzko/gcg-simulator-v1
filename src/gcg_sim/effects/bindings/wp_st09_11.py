"""Bindings for starter decks ST09 (Minerva Squad), ST10 (G Generation) and ST11 (Marine).

HP filters read current HP (modified HP minus damage), per FAQ Q96 / ruling GD03-049:Q224.
"""

from __future__ import annotations

from gcg_sim.cards.model import CardDef
from gcg_sim.effects import dsl as d
from gcg_sim.effects.bindings import card
from gcg_sim.effects.compiler import route_abilities
from gcg_sim.engine.types import Duration

G_GENERATION = "G Generation"
MARINE = "Marine"
GOOHN_TOKEN = "GOOhN|ZAFT/Marine|1|1|"

UNIT = d.IsKind((d.CardKind.UNIT,))


def _enemy_units(*filters: d.Filter) -> d.Sel:
    return d.Sel(d.Side.ENEMY, d.Loc.BATTLE, (UNIT, *filters))


def _friendly_units(*filters: d.Filter) -> d.Sel:
    return d.Sel(d.Side.FRIENDLY, d.Loc.BATTLE, (UNIT, *filters))


def _current_hp_at_most(n: int) -> d.StatCmp:
    return d.StatCmp(d.Stat.REMAINING_HP, d.Op.LE, n)


def _other_friendly_marine() -> d.Sel:
    return _friendly_units(d.NotRef(d.This()), d.HasTrait((MARINE,)))


def _burst_add_to_hand() -> d.Burst:
    return d.Burst((d.AddToHand(d.ThisCard()),))


def _burst_deploy() -> d.Burst:
    return d.Burst((d.DeployCard(d.ThisCard()),))


def _development(amount: int, body: tuple[d.Step, ...]) -> tuple[d.Step, ...]:
    """Rule 13-1-8-1: "you may exile (amount) (G Generation) cards in your trash from the game.
    If you do, activate the following effect" (Q303/Q304: allowed even if ■ has no target)."""
    pool = d.Sel(d.Side.FRIENDLY, d.Loc.TRASH, (d.HasTrait((G_GENERATION,)),))
    return (
        d.If(
            d.Exists(pool, at_least=amount),
            (
                d.May(
                    (
                        d.Choose("development", pool, amount, targeting=False),
                        d.Exile(d.Var("development")),
                        d.IfYouDo(body),
                    ),
                    prompt=f"Development {amount}: exile {amount} (G Generation) cards?",
                ),
            ),
        ),
    )


def _script(c: CardDef, *abilities: d.Ability) -> d.CardScript:
    own, unit = route_abilities(c, list(abilities))
    return d.CardScript(c.card_number, abilities=own, unit_abilities=unit, source="binding")


@card("ST09-001")
def st09_001(c: CardDef) -> d.CardScript:
    """The DSL has no "return this Unit to its owner's deck" cost; it is the first step, which
    always succeeds because the ability only exists while this Unit is in the battle area."""
    impulse = d.Sel(
        d.Side.FRIENDLY,
        d.Loc.TRASH,
        (UNIT, d.NameContains(("Impulse Gundam",)), d.StatCmp(d.Stat.LV, d.Op.GE, 4)),
    )
    return _script(
        c,
        d.Activated(
            d.Timing.MAIN,
            (d.PayResources(2),),
            (
                d.ToDeck(d.This(), bottom=True),
                d.Choose("t1", impulse),
                d.DeployCard(d.Var("t1")),
            ),
        ),
    )


@card("ST09-010")
def st09_010(c: CardDef) -> d.CardScript:
    top_two = d.Sel(d.Side.FRIENDLY, d.Loc.DECK_TOP, top_n=2)
    remaining = d.Sel(d.Side.FRIENDLY, d.Loc.DECK_TOP, (d.NotRef(d.Var("keep")),), top_n=2)
    return _script(
        c,
        _burst_deploy(),
        d.Triggered(
            d.Trigger(d.Ev.DEPLOYED),
            (
                d.ShieldToHand(1),
                d.If(
                    d.IsTurn(d.P.YOU),
                    (
                        d.LookTop(2),
                        d.Choose("keep", top_two, 1, targeting=False),
                        d.ToTrash(d.All(remaining)),
                    ),
                ),
            ),
        ),
    )


@card("ST10-001")
def st10_001(c: CardDef) -> d.CardScript:
    return _script(
        c,
        d.Triggered(
            d.Trigger(d.Ev.DESTROYS_SHIELD_CARD, battle_only=True),
            (
                d.SetActive(d.This()),
                d.Apply(
                    d.This(),
                    d.RuleGrant(d.RuleMod(d.RuleKind.CANT_ATTACK_PLAYER)),
                    Duration.THIS_TURN,
                ),
            ),
        ),
    )


@card("ST10-002")
def st10_002(c: CardDef) -> d.CardScript:
    return _script(
        c,
        d.Triggered(
            d.Trigger(d.Ev.DEPLOYED),
            _development(
                2,
                (
                    d.Choose("t1", _enemy_units(_current_hp_at_most(4))),
                    d.Rest(d.Var("t1")),
                ),
            ),
        ),
    )


@card("ST10-006")
def st10_006(c: CardDef) -> d.CardScript:
    return _script(
        c,
        d.Triggered(
            d.Trigger(
                d.Ev.DESTROYS_BY_BATTLE,
                battle_only=True,
                whose_turn=d.P.YOU,
                target_filters=(UNIT,),
            ),
            (
                d.Choose("t1", _enemy_units(_current_hp_at_most(3))),
                d.ReturnToHand(d.Var("t1")),
            ),
            gate=d.Gate.PAIRED,
        ),
    )


@card("ST10-007")
def st10_007(c: CardDef) -> d.CardScript:
    command = d.Sel(
        d.Side.FRIENDLY,
        d.Loc.TRASH,
        (d.IsKind((d.CardKind.COMMAND,)), d.StatCmp(d.Stat.LV, d.Op.LE, 4)),
    )
    return _script(
        c,
        d.Triggered(
            d.Trigger(d.Ev.LINKED),
            _development(2, (d.Choose("t1", command), d.AddToHand(d.Var("t1")))),
        ),
    )


@card("ST10-008")
def st10_008(c: CardDef) -> d.CardScript:
    enemy_players = 1
    return _script(
        c,
        d.Triggered(
            d.Trigger(d.Ev.DEPLOYED),
            _development(
                2,
                (
                    d.Draw(enemy_players),
                    d.If(d.DidLast(), (d.Discard(enemy_players),)),
                ),
            ),
        ),
    )


@card("ST10-014")
def st10_014(c: CardDef) -> d.CardScript:
    g_gen_unit_card = d.Sel(d.Side.FRIENDLY, d.Loc.HAND, (UNIT, d.HasTrait((G_GENERATION,))))
    return _script(
        c,
        d.PlayModifier(costs=(d.DiscardCards(g_gen_unit_card, 1),), level=2, cost=2),
        d.Command(d.Timing.MAIN, (d.Draw(2),)),
    )


@card("ST11-001")
def st11_001(c: CardDef) -> d.CardScript:
    return _script(
        c,
        d.Constant(
            (d.RuleGrant(d.RuleMod(d.RuleKind.CANT_BE_ATTACKED)),),
            cond=d.Exists(_other_friendly_marine(), at_least=2),
            gate=d.Gate.PAIRED,
        ),
        d.Triggered(
            d.Trigger(d.Ev.DEPLOYED),
            (
                d.If(
                    d.Exists(_other_friendly_marine()),
                    (d.Choose("t1", _enemy_units(d.StatCmp(d.Stat.LV, d.Op.LE, 2))),),
                ),
                d.ToDeck(d.Var("t1"), bottom=True),
            ),
        ),
    )


@card("ST11-002")
def st11_002(c: CardDef) -> d.CardScript:
    protected = d.All(_friendly_units(d.HasTrait((MARINE,)), _current_hp_at_most(2)))
    no_enemy_effect_damage = d.RuleMod(
        d.RuleKind.CANT_RECEIVE_DAMAGE,
        damage_kind=d.DamageKind.EFFECT,
        source_side=d.Side.ENEMY,
    )
    return _script(
        c,
        d.Constant(
            (d.RuleGrant(no_enemy_effect_damage),),
            scope=protected,
            cond=d.And((d.IsTurn(d.P.OPP), d.RefMatches(d.This(), (d.IsRested(True),)))),
        ),
    )


@card("ST11-004")
def st11_004(c: CardDef) -> d.CardScript:
    return _script(
        c,
        d.Triggered(d.Trigger(d.Ev.DEPLOYED), (d.DeployToken(GOOHN_TOKEN, 1, rested=True),)),
    )


@card("ST11-006")
def st11_006(c: CardDef) -> d.CardScript:
    reduce_enemy_effects = d.RuleMod(
        d.RuleKind.SHIELD_AREA_REDUCTION,
        amount=5,
        damage_kind=d.DamageKind.EFFECT,
        source_side=d.Side.ENEMY,
    )
    marine_card = d.Sel(d.Side.FRIENDLY, d.Loc.TRASH, (UNIT, d.HasTrait((MARINE,))))
    return _script(
        c,
        d.Triggered(
            d.Trigger(d.Ev.TURN_START, self_only=False, whose_turn=d.P.OPP),
            (d.ApplyPlayer(d.P.YOU, d.RuleGrant(reduce_enemy_effects), Duration.THIS_TURN),),
            cond=d.Exists(_other_friendly_marine()),
        ),
        d.Triggered(
            d.Trigger(d.Ev.DEPLOYED),
            (d.Choose("t1", marine_card), d.AddToHand(d.Var("t1"))),
        ),
    )


@card("ST11-009")
def st11_009(c: CardDef) -> d.CardScript:
    return _script(
        c,
        d.Triggered(d.Trigger(d.Ev.DESTROYED), (d.If(d.IsTurn(d.P.OPP), (d.Draw(1),)),)),
    )


@card("ST11-012")
def st11_012(c: CardDef) -> d.CardScript:
    reduce_battle_damage = d.RuleMod(
        d.RuleKind.REDUCE_DAMAGE,
        amount=2,
        damage_kind=d.DamageKind.BATTLE,
        source_filters=(UNIT,),
        source_side=d.Side.ENEMY,
    )
    return _script(
        c,
        _burst_add_to_hand(),
        d.Triggered(
            d.Trigger(d.Ev.PAIRED),
            (
                d.Choose("t1", _friendly_units(d.HasTrait((MARINE,)))),
                d.Apply(d.Var("t1"), d.RuleGrant(reduce_battle_damage), Duration.THIS_TURN),
            ),
        ),
    )


@card("ST11-013")
def st11_013(c: CardDef) -> d.CardScript:
    target = _enemy_units(d.IsRested(True), _current_hp_at_most(3))
    bounce = (d.Choose("t1", target), d.ReturnToHand(d.Var("t1")))
    return _script(
        c,
        d.Burst(bounce),
        d.Command(d.Timing.MAIN_OR_ACTION, (*bounce, d.IfYouDo((d.Draw(1),)))),
    )
