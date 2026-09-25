"""Bindings for ST12, ST13 and ST14 cards whose compiled text is missing or wrong (WP-ST12-14)."""

from __future__ import annotations

from gcg_sim.cards.model import CardDef
from gcg_sim.cards.tokens import parse_token_specs
from gcg_sim.effects import dsl as d
from gcg_sim.effects.bindings import card, custom_cond, custom_step, custom_value
from gcg_sim.engine import core
from gcg_sim.engine import view as V
from gcg_sim.engine.state import Frame, GameState
from gcg_sim.engine.types import Duration, Phase, Step, Zone

FRIENDLY = d.Side.FRIENDLY
ENEMY = d.Side.ENEMY
UNIT = d.IsKind((d.CardKind.UNIT,))

RESOURCE_SET_ACTIVE = "resource_set_active"
PAIRED_UNIT_DESTROYED_ENEMY_BY_BATTLE = "destroys_by_battle_paired"

ADD_THIS_TO_HAND = d.Burst((d.AddToHand(d.ThisCard()),))
DEPLOY_THIS = d.Burst((d.DeployCard(d.ThisCard()),))
SHIELD_TO_HAND_ON_DEPLOY = d.Triggered(d.Trigger(d.Ev.DEPLOYED), (d.ShieldToHand(),))

A_PLAYER_HAS_3_OR_LESS_SHIELDS = d.Or(
    (
        d.Cmp(d.ShieldCount(d.P.YOU), d.Op.LE, 3),
        d.Cmp(d.ShieldCount(d.P.OPP), d.Op.LE, 3),
    )
)


def _units(side: d.Side, *filters: d.Filter) -> d.Sel:
    return d.Sel(side, d.Loc.BATTLE, (UNIT, *filters))


def _lv(op: d.Op, n: d.Value) -> d.StatCmp:
    return d.StatCmp(d.Stat.LV, op, n)


def _looked(*filters: d.Filter) -> d.Sel:
    return d.Sel(FRIENDLY, d.Loc.DECK, (*filters, d.IsRef(d.Var("looked"))))


def _bit_funnel_key(c: CardDef) -> str:
    (spec,) = parse_token_specs(c.effect)
    return spec.key


def _deploy_bit_funnels(key: str, *, up_to_two: bool) -> tuple[d.Step, ...]:
    first = d.DeployToken(key, 1)
    if not up_to_two:
        return (first,)
    return (first, d.May((d.DeployToken(key, 1, var="tokens_x"),)))


def _one_if_at_least(value: d.Value, n: int) -> d.Value:
    return d.Sum((d.MinOf((value, n)), d.Times(d.MinOf((value, n - 1)), -1)))


def _exact_count_choice(sel: d.Sel, n: int, chooser: d.P) -> d.If:
    """Resolution of ruling GD01-003:Q121: choosing N cards from a public location needs N."""
    return d.If(
        d.Cmp(d.Count(sel), d.Op.GE, n),
        (d.Choose("t1", sel, n, chooser=chooser, targeting=False), d.Exile(d.Var("t1"))),
    )


def _script(
    c: CardDef, abilities: tuple[d.Ability, ...], unit: tuple[d.Ability, ...] = ()
) -> d.CardScript:
    return d.CardScript(c.card_number, abilities=abilities, unit_abilities=unit, source="binding")


# ---------------------------------------------------------------------------------------------
# custom hooks


@custom_cond("wp_st12_14_destroyed_by_effect_damage")
def destroyed_by_effect_damage(
    st: GameState, dv: V.Derived, ctx: V.Ctx, params: dict[str, object]
) -> bool:
    """The triggering destruction is rules management (11-3) right after a damage step of the
    resolving effect, i.e. destruction with effect damage (resolution of ruling ST13-012:Q459)."""
    if ctx.ev("battle", 0) or ctx.ev("by") >= 0 or not st.frames:
        return False
    f = st.frames[-1]
    instrs = V.reg().programs[f.program_id].instrs
    return 0 < f.pc <= len(instrs) and isinstance(instrs[f.pc - 1], d.Damage)


@custom_value("wp_st12_14_lowest_rested_enemy_lv")
def lowest_rested_enemy_lv(
    st: GameState, dv: V.Derived, ctx: V.Ctx, params: dict[str, object]
) -> int:
    enemy = 1 - ctx.controller
    levels = [V.level_of(st, u) for u in st.zones[enemy][Zone.BATTLE] if st.cards[u].rested]
    return min(levels, default=-1)


@custom_cond("wp_st12_14_start_phase")
def start_phase(st: GameState, dv: V.Derived, ctx: V.Ctx, params: dict[str, object]) -> bool:
    """Rule 7-2; start-step triggers resolve after the engine has moved to the draw step."""
    return st.phase is Phase.START or st.step is Step.DRAW_STEP


@custom_step("wp_st12_14_record_resource_set_active")
def record_resource_set_active(
    st: GameState, f: Frame, ctx: V.Ctx, params: dict[str, object]
) -> bool:
    core.record(st, RESOURCE_SET_ACTIVE, f.controller, f.controller)
    return True


# ---------------------------------------------------------------------------------------------
# ST12


@card("ST12-009")
def st12_009(c: CardDef) -> d.CardScript:
    trash_lv6 = d.Sel(FRIENDLY, d.Loc.TRASH, (UNIT, _lv(d.Op.GE, 6)))
    destroyed = d.Triggered(
        d.Trigger(d.Ev.DESTROYED),
        (
            d.If(
                A_PLAYER_HAS_3_OR_LESS_SHIELDS,
                (
                    d.Choose("t1", trash_lv6),
                    d.AddToHand(d.Var("t1")),
                    d.IfYouDo((d.Discard(),)),
                ),
            ),
        ),
    )
    return _script(c, (destroyed,))


@card("ST12-011")
def st12_011(c: CardDef) -> d.CardScript:
    battling_damaged = _units(ENEMY, d.IsDamaged(), d.IsRef(d.BattlingWith(d.This())))
    deal_2 = d.Activated(
        d.Timing.ACTION,
        (),
        (d.Choose("t1", battling_damaged), d.Damage(d.Var("t1"), 2)),
        once_per_turn=True,
    )
    return _script(c, (d.NameAlias(("Zechs Merquise",)), ADD_THIS_TO_HAND), (deal_2,))


@card("ST12-012")
def st12_012(c: CardDef) -> d.CardScript:
    others = d.Sel(FRIENDLY, d.Loc.DECK, (d.IsRef(d.Var("looked")), d.NotRef(d.Var("kept"))))
    when_linked = d.Triggered(
        d.Trigger(d.Ev.LINKED),
        (
            d.LookTop(2),
            d.Choose("kept", _looked(), targeting=False),
            d.ToTrash(d.All(others)),
            d.If(A_PLAYER_HAS_3_OR_LESS_SHIELDS, (d.AddToHand(d.Var("kept")),)),
        ),
    )
    return _script(c, (d.NameAlias(("Marida Cruz",)), ADD_THIS_TO_HAND), (when_linked,))


@card("ST12-013")
def st12_013(c: CardDef) -> d.CardScript:
    burst = d.Burst((d.Choose("t1", _units(ENEMY)), d.Damage(d.Var("t1"), 1)))
    main = d.Command(
        d.Timing.MAIN,
        (
            d.Choose("mine", _units(FRIENDLY)),
            d.Choose("theirs", _units(ENEMY), chooser=d.P.OPP),
            d.StartBattle(d.Var("mine"), d.Var("theirs")),
        ),
    )
    return _script(c, (burst, main))


@card("ST12-014")
def st12_014(c: CardDef) -> d.CardScript:
    granted = d.Triggered(
        d.Trigger(d.Ev.DESTROYS_BY_BATTLE, target_filters=(UNIT,)),
        (
            d.Choose("t1", _units(ENEMY, d.StatCmp(d.Stat.AP, d.Op.LE, 2))),
            d.Destroy(d.Var("t1")),
        ),
    )
    action = d.Command(
        d.Timing.ACTION,
        (
            d.Choose("t1", _units(FRIENDLY)),
            d.Apply(d.Var("t1"), d.AbilityGrant(c.card_number, 1), Duration.THIS_BATTLE),
        ),
    )
    return _script(c, (action, granted))


@card("ST12-015")
def st12_015(c: CardDef) -> d.CardScript:
    destroy_lv2 = (d.Choose("t1", _units(ENEMY, _lv(d.Op.LE, 2))), d.Destroy(d.Var("t1")))
    damage_both = (
        d.Choose("t1", _units(FRIENDLY)),
        d.Choose("t2", _units(ENEMY, _lv(d.Op.GE, 5))),
        d.Damage(d.Union((d.Var("t1"), d.Var("t2"))), 2),
    )
    action = d.Command(
        d.Timing.ACTION,
        (
            d.ChooseMode(
                (
                    ("destroy_enemy_lv2_or_lower", destroy_lv2),
                    ("damage_friendly_and_enemy_lv5_or_higher", damage_both),
                )
            ),
        ),
    )
    return _script(c, (action,))


@card("ST12-016")
def st12_016(c: CardDef) -> d.CardScript:
    ping = d.Activated(
        d.Timing.MAIN,
        (d.RestSelf(),),
        (
            d.If(
                d.HappenedThisTurn(PAIRED_UNIT_DESTROYED_ENEMY_BY_BATTLE, d.P.YOU),
                (d.Choose("t1", _units(ENEMY, _lv(d.Op.LE, 4))), d.Damage(d.Var("t1"), 1)),
            ),
        ),
    )
    return _script(c, (DEPLOY_THIS, SHIELD_TO_HAND_ON_DEPLOY, ping))


# ---------------------------------------------------------------------------------------------
# ST13


@card("ST13-001")
def st13_001(c: CardDef) -> d.CardScript:
    key = _bit_funnel_key(c)
    on_pair = d.Triggered(
        d.Trigger(d.Ev.PAIRED, self_only=False, subject=_units(FRIENDLY), whose_turn=d.P.YOU),
        _deploy_bit_funnels(key, up_to_two=True),
        once_per_turn=True,
    )
    token_battle = d.Activated(
        d.Timing.MAIN,
        (d.PayResources(1),),
        (
            d.Choose("t1", d.Sel(FRIENDLY, d.Loc.BATTLE, (d.IsKind((d.CardKind.UNIT_TOKEN,)),))),
            d.Choose("t2", _units(ENEMY), distinct_from=("t1",)),
            d.StartBattle(d.Var("t1"), d.Var("t2")),
        ),
        once_per_turn=True,
    )
    return _script(c, (on_pair, token_battle))


@card("ST13-002")
def st13_002(c: CardDef) -> d.CardScript:
    deploy = d.Triggered(
        d.Trigger(d.Ev.DEPLOYED), _deploy_bit_funnels(_bit_funnel_key(c), up_to_two=False)
    )
    return _script(c, (deploy,))


@card("ST13-005")
def st13_005(c: CardDef) -> d.CardScript:
    deploy = d.Triggered(
        d.Trigger(d.Ev.DEPLOYED),
        (
            d.If(
                d.Cmp(d.Count(_units(ENEMY)), d.Op.GE, 4),
                (
                    d.LookTop(5),
                    d.May(
                        (
                            d.Choose("t1", _looked(d.IsKind((d.CardKind.PILOT,))), targeting=False),
                            d.AddToHand(d.Var("t1"), reveal=True),
                        )
                    ),
                    d.CustomStep("return_looked_bottom"),
                ),
            ),
        ),
    )
    return _script(c, (deploy,))


@card("ST13-009")
def st13_009(c: CardDef) -> d.CardScript:
    enemy_trash_units = d.Sel(ENEMY, d.Loc.TRASH, (UNIT,))
    attack = d.Triggered(
        d.Trigger(d.Ev.ATTACKS), (_exact_count_choice(enemy_trash_units, 2, d.P.OPP),)
    )
    return _script(c, (attack,))


@card("ST13-012")
def st13_012(c: CardDef) -> d.CardScript:
    draw = d.Triggered(
        d.Trigger(d.Ev.DESTROYED, self_only=False, subject=_units(ENEMY), battle_only=False),
        (d.Draw(),),
        cond=d.And(
            (
                d.RefMatches(d.This(), (d.IsAttacking(),)),
                d.CustomCond("wp_st12_14_destroyed_by_effect_damage"),
            )
        ),
        once_per_turn=True,
    )
    return _script(c, (ADD_THIS_TO_HAND,), (draw,))


@card("ST13-013")
def st13_013(c: CardDef) -> d.CardScript:
    main = d.Command(d.Timing.MAIN, _deploy_bit_funnels(_bit_funnel_key(c), up_to_two=True))
    return _script(c, (main,))


@card("ST13-014")
def st13_014(c: CardDef) -> d.CardScript:
    main = d.Command(
        d.Timing.MAIN,
        (
            d.Choose("t1", _units(FRIENDLY)),
            d.Destroy(d.Var("t1")),
            d.IfYouDo(
                (
                    d.LookTop(4),
                    d.May(
                        (
                            d.Choose("t2", _looked(UNIT, _lv(d.Op.LE, 4)), targeting=False),
                            d.DeployCard(d.Var("t2")),
                        )
                    ),
                    d.CustomStep("return_looked_bottom"),
                )
            ),
        ),
    )
    return _script(c, (main,))


@card("ST13-015")
def st13_015(c: CardDef) -> d.CardScript:
    trash_units = d.Sel(FRIENDLY, d.Loc.TRASH, (UNIT,))
    command = d.Command(
        d.Timing.MAIN_OR_ACTION,
        (
            d.Choose("t1", trash_units, 3),
            d.Exile(d.Var("t1")),
            d.IfYouDo((d.Choose("t2", _units(ENEMY)), d.Damage(d.Var("t2"), 3))),
        ),
        cond=d.Cmp(d.Count(trash_units), d.Op.GE, 3),
    )
    return _script(c, (ADD_THIS_TO_HAND, command))


@card("ST13-016")
def st13_016(c: CardDef) -> d.CardScript:
    deploy = d.Triggered(
        d.Trigger(d.Ev.DEPLOYED),
        (d.ShieldToHand(), *_deploy_bit_funnels(_bit_funnel_key(c), up_to_two=False)),
    )
    return _script(c, (DEPLOY_THIS, deploy))


# ---------------------------------------------------------------------------------------------
# ST14


@card("ST14-001")
def st14_001(c: CardDef) -> d.CardScript:
    lowest_rested = _units(
        ENEMY,
        d.IsRested(),
        _lv(d.Op.LE, d.CustomValue("wp_st12_14_lowest_rested_enemy_lv")),
    )
    freeze = d.Constant(
        (d.RuleGrant(d.RuleMod(d.RuleKind.CANT_BE_SET_ACTIVE)),),
        scope=d.All(lowest_rested),
        cond=d.And((d.IsTurn(d.P.OPP), d.CustomCond("wp_st12_14_start_phase"))),
    )
    return _script(c, (d.Keyword(d.Kw.SUPPRESSION), freeze))


@card("ST14-003")
def st14_003(c: CardDef) -> d.CardScript:
    enemy_trash_units = d.Sel(ENEMY, d.Loc.TRASH, (UNIT,))
    deploy = d.Triggered(
        d.Trigger(d.Ev.DEPLOYED), (_exact_count_choice(enemy_trash_units, 2, d.P.OPP),)
    )
    return _script(c, (deploy,))


@card("ST14-006")
def st14_006(c: CardDef) -> d.CardScript:
    deploy = d.Triggered(
        d.Trigger(d.Ev.DEPLOYED),
        (
            d.LookTop(3),
            d.May(
                (
                    d.Choose("t1", _looked(), targeting=False),
                    d.AddToHand(d.Var("t1"), reveal=True),
                )
            ),
            d.CustomStep("return_looked_bottom"),
        ),
    )
    no_battle_damage = d.Constant(
        (
            d.RuleGrant(
                d.RuleMod(
                    d.RuleKind.CANT_RECEIVE_DAMAGE,
                    damage_kind=d.DamageKind.BATTLE,
                    source_filters=(UNIT, d.LevelCmpRef(d.Stat.AP, d.Op.LE, d.This(), d.Stat.AP)),
                    source_side=ENEMY,
                    once_per_turn=True,
                )
            ),
        ),
        gate=d.Gate.PAIRED,
    )
    return _script(c, (deploy, no_battle_damage))


@card("ST14-011")
def st14_011(c: CardDef) -> d.CardScript:
    rested_enemies = _units(ENEMY, d.IsRested())
    reductions = tuple(
        d.If(
            d.Cmp(d.VarSize("rested"), d.Op.EQ, n),
            (d.Apply(d.Var("t1"), d.StatMod(ap=-n), Duration.THIS_BATTLE),),
        )
        for n in range(1, core.BATTLE_LIMIT + 1)
    )
    attack = d.Triggered(
        d.Trigger(d.Ev.ATTACKS),
        (
            d.Choose("t1", _units(ENEMY)),
            d.BindVar("rested", d.All(rested_enemies)),
            *reductions,
        ),
    )
    return _script(c, (ADD_THIS_TO_HAND,), (attack,))


@card("ST14-013")
def st14_013(c: CardDef) -> d.CardScript:
    ap_minus_3 = (d.Choose("t1", _units(ENEMY)), d.Apply(d.Var("t1"), d.StatMod(ap=-3)))
    rest_low_hp = (
        d.Choose("t1", _units(ENEMY, d.StatCmp(d.Stat.HP, d.Op.LE, 3)), 2, min_count=1),
        d.Rest(d.Var("t1")),
    )
    command = d.Command(
        d.Timing.MAIN_OR_ACTION,
        (
            d.ChooseMode(
                (
                    ("rest_1_to_2_enemy_units_with_3_or_less_hp", rest_low_hp),
                    ("enemy_unit_ap_minus_3", ap_minus_3),
                )
            ),
        ),
    )
    return _script(c, (d.Burst(ap_minus_3), command))


@card("ST14-014")
def st14_014(c: CardDef) -> d.CardScript:
    commands_in_trash = d.Count(d.Sel(FRIENDLY, d.Loc.TRASH, (d.IsKind((d.CardKind.COMMAND,)),)))
    max_lv = d.Sum((5, d.Times(_one_if_at_least(commands_in_trash, 4), 100)))
    command = d.Command(
        d.Timing.MAIN_OR_ACTION,
        (
            d.Choose("t1", _units(ENEMY, _lv(d.Op.LE, max_lv))),
            d.Apply(d.Var("t1"), d.StatMod(ap=-3)),
        ),
    )
    return _script(c, (command,))


@card("ST14-015")
def st14_015(c: CardDef) -> d.CardScript:
    main = d.Command(
        d.Timing.MAIN,
        (
            d.PlaceResource(rested=True),
            d.If(
                d.NotC(d.HappenedThisTurn(RESOURCE_SET_ACTIVE, d.P.YOU, by=d.P.YOU)),
                (
                    d.SetResourcesActive(1),
                    d.IfYouDo((d.CustomStep("wp_st12_14_record_resource_set_active"),)),
                ),
            ),
        ),
    )
    return _script(c, (d.Burst((d.PlaceExResource(),)), main))
