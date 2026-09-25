"""Card bindings for EB01-001..EB01-046 (work package WP-EB01-A).

Multiplayer wording follows the 1v1 reading in docs/ASSUMPTIONS.md: "each enemy player",
"another player" and "the player who destroyed this Unit" are the opponent, and
"2 or more enemy players" is never true. <Development N> (rule 13-1-8) is an optional exile of
N (G Generation) cards from your trash that may be paid even when the ■ effect has no target
(ruling EB01-010:Q315).
"""

from __future__ import annotations

from gcg_sim.cards.model import CardDef
from gcg_sim.effects import dsl as d
from gcg_sim.effects.bindings import card, custom_cond, custom_step
from gcg_sim.engine import view as V
from gcg_sim.engine.state import Frame, GameState
from gcg_sim.engine.types import Duration, Step, Zone

UNIT = d.IsKind((d.CardKind.UNIT,))
G_GENERATION = d.HasTrait(("G Generation",))
NEVER: d.Cond = d.Or(())
G_GENERATION_TRASH = d.Sel(d.Side.FRIENDLY, d.Loc.TRASH, (G_GENERATION,))
START_PHASE_STEPS = frozenset({Step.ACTIVE_STEP, Step.START_STEP, Step.DRAW_STEP})
EB01_001_FREEZE_INDEX = 1


def _units(side: d.Side, *filters: d.Filter) -> d.Sel:
    return d.Sel(side, d.Loc.BATTLE, (UNIT, *filters))


def _lv(op: d.Op, n: int) -> d.StatCmp:
    return d.StatCmp(d.Stat.LV, op, n)


def _script(c: CardDef, *abilities: d.Ability, notes: str = "") -> d.CardScript:
    return d.CardScript(c.card_number, abilities=abilities, source="binding", notes=notes)


def _development(n: int, effect: tuple[d.Step, ...]) -> tuple[d.Step, ...]:
    """<Development N> (rule 13-1-8-1): you may exile N (G Generation) cards from your trash;
    if you do, perform the ■ effect."""
    return (
        d.If(
            d.Exists(G_GENERATION_TRASH, n),
            (
                d.May(
                    (
                        d.Choose("dev", G_GENERATION_TRASH, n, targeting=False),
                        d.Exile(d.Var("dev")),
                        d.IfYouDo(effect),
                    ),
                    prompt=f"Development {n}: exile {n} (G Generation) cards from your trash?",
                ),
            ),
        ),
    )


# ---------------------------------------------------------------------------------------------
# custom hooks


@custom_cond("wp_eb01_a_start_phase_in_battle_area")
def start_phase_in_battle_area(
    st: GameState, dv: V.Derived, ctx: V.Ctx, params: dict[str, object]
) -> bool:
    """The host is in the battle area during a start phase; start-step triggers resolve while
    the engine is on its draw step, before the draw is performed."""
    return st.step in START_PHASE_STEPS and ctx.host >= 0 and st.cards[ctx.host].zone is Zone.BATTLE


@custom_step("wp_eb01_a_look_own_top")
def look_own_top(st: GameState, f: Frame, ctx: V.Ctx, params: dict[str, object]) -> bool:
    """A player looks at the top card of their own deck; only that player learns it."""
    player = V.player_of(st, ctx, d.P(str(params["player"])))
    top = tuple(st.zones[player][Zone.DECK][:1])
    for u in top:
        st.cards[u].known |= 1 << player
    f.vars[str(params["var"])] = top
    return bool(top)


# ---------------------------------------------------------------------------------------------
# cards


@card("EB01-001")
def eb01_001(c: CardDef) -> d.CardScript:
    activated = d.Activated(
        d.Timing.MAIN,
        (d.ExileCards(d.Sel(d.Side.FRIENDLY, d.Loc.TRASH, (d.IsKind((d.CardKind.COMMAND,)),)), 2),),
        (
            d.Choose("t1", _units(d.Side.ENEMY, d.IsDamaged(), _lv(d.Op.LE, 7))),
            d.Rest(d.Var("t1")),
            d.Apply(
                d.Var("t1"),
                d.AbilityGrant(c.card_number, EB01_001_FREEZE_INDEX),
                Duration.OPPONENT_NEXT_TURN,
            ),
        ),
        once_per_turn=True,
    )
    start_phase_freeze = d.Constant(
        (d.RuleGrant(d.RuleMod(d.RuleKind.CANT_BE_SET_ACTIVE)),),
        cond=d.CustomCond("wp_eb01_a_start_phase_in_battle_area"),
        where=d.Where.TRASH,
    )
    return _script(
        c,
        activated,
        start_phase_freeze,
        notes="The granted freeze applies only during the start phase of the opponent's next "
        "turn; its own copy never applies (it would need this card in the battle area while "
        "in the trash).",
    )


@card("EB01-002")
def eb01_002(c: CardDef) -> d.CardScript:
    deploy = d.Triggered(
        d.Trigger(d.Ev.DEPLOYED),
        (
            d.If(
                d.Exists(_units(d.Side.FRIENDLY, d.NotRef(d.This()), G_GENERATION)),
                (d.Choose("t1", _units(d.Side.ENEMY)), d.Rest(d.Var("t1"))),
            ),
        ),
    )
    attack = d.Triggered(
        d.Trigger(d.Ev.ATTACKS),
        (
            d.If(
                d.Cmp(
                    d.Count(_units(d.Side.ANY, d.NotRef(d.This()), d.IsRested())),
                    d.Op.GE,
                    3,
                ),
                (d.SetActive(d.This()),),
            ),
        ),
        once_per_turn=True,
        gate=d.Gate.LINKED,
    )
    return _script(c, deploy, attack)


@card("EB01-003")
def eb01_003(c: CardDef) -> d.CardScript:
    all_units = d.All(_units(d.Side.ANY))
    rested_by_this = d.Count(
        d.Sel(d.Side.ANY, d.Loc.BATTLE, (d.IsRef(d.Var("was_active")), d.IsRested()))
    )
    end_of_turn = d.Triggered(
        d.Trigger(d.Ev.TURN_END, self_only=False, whose_turn=d.P.YOU),
        (
            d.If(
                d.RefMatches(d.This(), (d.IsRested(),)),
                (
                    d.BindVar("was_active", d.All(_units(d.Side.ANY, d.IsRested(False)))),
                    d.Rest(all_units),
                    d.If(d.Cmp(rested_by_this, d.Op.GE, 3), (d.Draw(1),)),
                ),
            ),
        ),
    )
    return _script(c, d.Keyword(d.Kw.REPAIR, 2), end_of_turn)


@card("EB01-005")
def eb01_005(c: CardDef) -> d.CardScript:
    deploy = d.Triggered(
        d.Trigger(d.Ev.DEPLOYED),
        (
            d.Choose("t1", _units(d.Side.ENEMY, d.IsRested())),
            d.SetActive(d.Var("t1")),
            d.Draw(1),
        ),
    )
    return _script(c, deploy)


@card("EB01-008")
def eb01_008(c: CardDef) -> d.CardScript:
    effect = (d.Choose("t1", _units(d.Side.FRIENDLY)), d.Recover(d.Var("t1"), 2))
    return _script(c, d.Triggered(d.Trigger(d.Ev.DEPLOYED), _development(1, effect)))


@card("EB01-009")
def eb01_009(c: CardDef) -> d.CardScript:
    deploy = d.Triggered(
        d.Trigger(d.Ev.DEPLOYED),
        (
            d.Choose("t1", _units(d.Side.ENEMY, d.IsRested(False)), chooser=d.P.OPP),
            d.Rest(d.Var("t1")),
        ),
    )
    return _script(c, deploy)


@card("EB01-010")
def eb01_010(c: CardDef) -> d.CardScript:
    effect = (d.Choose("t1", _units(d.Side.ENEMY, d.IsRested())), d.Damage(d.Var("t1"), 2))
    return _script(c, d.Triggered(d.Trigger(d.Ev.DEPLOYED), _development(3, effect)))


@card("EB01-017")
def eb01_017(c: CardDef) -> d.CardScript:
    destroyed = d.Triggered(
        d.Trigger(d.Ev.DESTROYED, battle_only=True),
        (d.Draw(1, each_player=True),),
    )
    return _script(c, destroyed)


@card("EB01-022")
def eb01_022(c: CardDef) -> d.CardScript:
    token = "Gundam Exia|G Generation|2|2|"
    end_of_turn = d.Triggered(
        d.Trigger(d.Ev.TURN_END, self_only=False, whose_turn=d.P.YOU),
        (d.May((d.Destroy(d.This()), d.IfYouDo((d.DeployToken(token, 3),)))),),
        gate=d.Gate.PAIRED,
        gate_filters=(G_GENERATION,),
    )
    return _script(c, d.Keyword(d.Kw.BREACH, 5), end_of_turn)


def _le_cygne_player(player: d.P, var: str) -> tuple[d.Step, ...]:
    card_ref = d.Var(var)
    return (
        d.CustomStep("wp_eb01_a_look_own_top", (("player", player.value), ("var", var))),
        d.If(
            d.RefMatches(card_ref, (_lv(d.Op.GE, 5),)),
            (d.May((d.AddToHand(card_ref, reveal=True),), player=player),),
        ),
        d.Arrange(card_ref, "top_or_bottom", player=player),
    )


@card("EB01-023")
def eb01_023(c: CardDef) -> d.CardScript:
    attack = d.Triggered(
        d.Trigger(d.Ev.ATTACKS),
        (
            d.Simultaneous(
                (
                    *_le_cygne_player(d.P.ACTIVE, "looked_active"),
                    *_le_cygne_player(d.P.STANDBY, "looked_standby"),
                )
            ),
        ),
    )
    return _script(c, attack, notes="Q319: players act in turn order from the active player.")


@card("EB01-025")
def eb01_025(c: CardDef) -> d.CardScript:
    effect = (d.PlaceExResource(d.P.ACTIVE), d.PlaceExResource(d.P.STANDBY))
    battle_immunity = d.Constant(
        (
            d.RuleGrant(
                d.RuleMod(
                    d.RuleKind.CANT_RECEIVE_DAMAGE,
                    damage_kind=d.DamageKind.BATTLE,
                    source_filters=(UNIT, _lv(d.Op.LE, 5)),
                    source_side=d.Side.ENEMY,
                )
            ),
        ),
        cond=d.Exists(
            d.Sel(d.Side.ENEMY, d.Loc.RESOURCE_AREA, (d.IsKind((d.CardKind.EX_RESOURCE,)),))
        ),
        gate=d.Gate.PAIRED,
    )
    return _script(
        c, d.Triggered(d.Trigger(d.Ev.DEPLOYED), _development(2, effect)), battle_immunity
    )


@card("EB01-027")
def eb01_027(c: CardDef) -> d.CardScript:
    effect = (
        d.Choose("t1", _units(d.Side.FRIENDLY, G_GENERATION)),
        d.Apply(d.Var("t1"), d.KeywordGrant(d.Kw.BREACH, 1), Duration.THIS_TURN),
    )
    return _script(c, d.Triggered(d.Trigger(d.Ev.DEPLOYED), _development(2, effect)))


@card("EB01-028")
def eb01_028(c: CardDef) -> d.CardScript:
    another_unit_attacks_enemy_unit = d.Trigger(
        d.Ev.ATTACKS,
        self_only=False,
        subject=_units(d.Side.FRIENDLY),
        target_filters=(UNIT,),
        include_self=False,
    )
    return _script(
        c,
        d.Triggered(
            another_unit_attacks_enemy_unit,
            (
                d.Apply(
                    d.EventCard("subject"), d.KeywordGrant(d.Kw.BREACH, 2), Duration.THIS_BATTLE
                ),
            ),
            cond=d.RefMatches(d.This(), (d.IsRested(),)),
            once_per_turn=True,
        ),
    )


@card("EB01-037")
def eb01_037(c: CardDef) -> d.CardScript:
    battling_enemy_blocker = d.RefMatches(
        d.BattlingWith(d.This()), (UNIT, d.HasKeyword(d.Kw.BLOCKER))
    )
    no_battle_damage = d.Constant(
        (d.RuleGrant(d.RuleMod(d.RuleKind.CANT_RECEIVE_DAMAGE, damage_kind=d.DamageKind.BATTLE)),),
        cond=d.And((d.IsTurn(d.P.YOU), battling_enemy_blocker)),
    )
    return _script(c, no_battle_damage)


@card("EB01-039")
def eb01_039(c: CardDef) -> d.CardScript:
    three_enemy_units = d.Cmp(d.Count(_units(d.Side.ENEMY)), d.Op.GE, 3)
    return _script(c, d.PlayModifier(cond=three_enemy_units, level=3, cost=3))


@card("EB01-041")
def eb01_041(c: CardDef) -> d.CardScript:
    deploy = d.Triggered(
        d.Trigger(d.Ev.DEPLOYED),
        (
            d.Choose("t1", _units(d.Side.ENEMY, d.StatCmp(d.Stat.HP, d.Op.LE, 4))),
            d.ReturnToHand(d.Var("t1")),
        ),
    )
    return _script(c, d.Keyword(d.Kw.HIGH_MANEUVER), deploy)


@card("EB01-044")
def eb01_044(c: CardDef) -> d.CardScript:
    deploy = d.Triggered(
        d.Trigger(d.Ev.DEPLOYED),
        (
            d.If(
                NEVER,
                (d.Choose("t1", _units(d.Side.ENEMY)), d.ReturnToHand(d.Var("t1"))),
            ),
        ),
    )
    return _script(
        c,
        d.Keyword(d.Kw.BLOCKER),
        deploy,
        notes="'If there are 2 or more enemy players' is never true in 1v1.",
    )
