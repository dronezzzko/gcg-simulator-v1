"""Bindings for WP-GD03-A (GD03-001..GD03-083).

Cards whose compiled script is missing or deviates from the card text, its rulings, or the
resolutions in docs/CONFLICTS.md. HP references use current HP (rules FAQ Q96).
"""

from __future__ import annotations

from gcg_sim.cards.model import CardDef
from gcg_sim.cards.tokens import parse_token_specs
from gcg_sim.effects import dsl as d
from gcg_sim.effects.bindings import card, custom_filter, custom_step, custom_value
from gcg_sim.engine import view as V
from gcg_sim.engine.state import Frame, GameState
from gcg_sim.engine.types import Duration, Zone

F = d.Side.FRIENDLY
E = d.Side.ENEMY
UNIT = d.IsKind((d.CardKind.UNIT,))
PILOT = d.IsKind((d.CardKind.PILOT,))
ENEMY_UNITS = d.Sel(E, d.Loc.BATTLE, (UNIT,))
FRIENDLY_UNITS = d.Sel(F, d.Loc.BATTLE, (UNIT,))
T1 = d.Var("t1")
T2 = d.Var("t2")


def _trait(*traits: str) -> d.HasTrait:
    return d.HasTrait(traits)


def _script(c: CardDef, *abilities: d.Ability) -> d.CardScript:
    return d.CardScript(c.card_number, abilities=abilities, source="binding")


def _token_key(c: CardDef) -> str:
    (spec,) = parse_token_specs(c.effect)
    return spec.key


def _current_hp(op: d.Op, value: int) -> d.StatCmp:
    return d.StatCmp(d.Stat.REMAINING_HP, op, value)


def _other_friendly(*traits: str) -> d.Sel:
    return d.Sel(F, d.Loc.BATTLE, (UNIT, d.NotRef(d.This()), _trait(*traits)))


# ---------------------------------------------------------------------------------------------
# custom hooks


@custom_value("wp_gd03_a_ap_per")
def ap_per(st: GameState, dv: V.Derived, ctx: V.Ctx, params: dict[str, object]) -> int:
    """GD03-033: 1 for each ``per`` AP the effect's Unit has (Q219: 9 AP → 2)."""
    per = params["per"]
    assert isinstance(per, int)
    if ctx.host < 0:
        return 0
    return V.ap_of(st, dv, ctx.host) // per


@custom_filter("wp_gd03_a_has_destroyed_effect")
def has_destroyed_effect(
    st: GameState, dv: V.Derived, ctx: V.Ctx, uid: int, params: dict[str, object]
) -> bool:
    """GD03-037 / Q222: the card currently has a 【Destroyed】 effect (gated effects whose
    【During Pair】/【During Link】 condition is unmet are not possessed; Pilot-granted ones are)."""
    for entry in dv.abilities.get(uid, ()):
        ab = entry.ability
        if (
            isinstance(ab, d.Triggered)
            and ab.trigger.event is d.Ev.DESTROYED
            and ab.trigger.self_only
        ):
            return True
    return False


@custom_filter("wp_gd03_a_lowest_hp")
def lowest_hp(
    st: GameState, dv: V.Derived, ctx: V.Ctx, uid: int, params: dict[str, object]
) -> bool:
    """GD03-049: the Unit has the lowest current HP among its controller's Units (Q224, Q225)."""
    owner = st.cards[uid].owner
    units = st.zones[owner][Zone.BATTLE]
    if uid not in units:
        return False
    lowest = min(V.stat(st, dv, u, d.Stat.REMAINING_HP) for u in units)
    return V.stat(st, dv, uid, d.Stat.REMAINING_HP) == lowest


@custom_step("wp_gd03_a_pay_cost_of")
def pay_cost_of(st: GameState, f: Frame, ctx: V.Ctx, params: dict[str, object]) -> bool:
    """GD03-051 "Pay its cost to deploy it": rest Resources equal to the chosen card's cost as
    modified where it is (resolution ambiguous:pay-its-cost-to-deploy); the payment counts as a
    cost paid for this Unit's effect (rulings GD04-100:Q286, GD04-129:Q297, GD05-101:Q400)."""
    from gcg_sim.engine import interp

    var = params["var"]
    assert isinstance(var, str)
    cards = f.vars.get(var, ())
    if not cards:
        return False
    uid = cards[0]
    dv = V.derived(st)
    cost = max(0, V.cost_of(st, uid) + dv.cost_mod.get(uid, 0))
    interp.execute(st, f, d.PayCost(cost))
    return f.did


@custom_step("wp_gd03_a_ap_minus_count")
def ap_minus_count(st: GameState, f: Frame, ctx: V.Ctx, params: dict[str, object]) -> bool:
    """GD03-071: "For each X, it gets AP-1 during this turn" with the count fixed when the
    effect resolves (a resolved effect is not recomputed later, cf. ruling GD01-059:Q135)."""
    from gcg_sim.engine import interp

    var = params["var"]
    sel = params["sel"]
    assert isinstance(var, str)
    assert isinstance(sel, d.Sel)
    n = len(V.select(st, V.derived(st), ctx, sel))
    if n <= 0 or not f.vars.get(var):
        return False
    interp.execute(st, f, d.Apply(d.Var(var), d.StatMod(ap=-n), Duration.THIS_TURN))
    return f.did


# ---------------------------------------------------------------------------------------------
# card bindings


@card("GD03-001")
def gd03_001(c: CardDef) -> d.CardScript:
    """Q209: the draw checks only whether this effect's damage destroyed the chosen Unit."""
    destroyed_now = d.And(
        (
            d.HappenedThisTurn("destroyed", d.P.OPP, filters=(d.IsRef(T1),)),
            d.RefEmpty(d.All(d.Sel(E, d.Loc.BATTLE, (d.IsRef(T1),)))),
        )
    )
    return _script(
        c,
        d.Keyword(d.Kw.REPAIR, 2),
        d.Triggered(
            d.Trigger(d.Ev.PAIRED),
            (
                d.Choose("t1", d.Sel(E, d.Loc.BATTLE, (UNIT, d.IsRested()))),
                d.Damage(T1, 1),
                d.If(destroyed_now, (d.Draw(1),)),
            ),
        ),
    )


@card("GD03-002")
def gd03_002(c: CardDef) -> d.CardScript:
    """'that Unit' is the attacking Unit (the compiled reference was never bound)."""
    return _script(
        c,
        d.Keyword(d.Kw.REPAIR, 3),
        d.Triggered(
            d.Trigger(
                d.Ev.ATTACKS,
                self_only=False,
                subject=d.Sel(F, d.Loc.BATTLE, (UNIT, d.HasKeyword(d.Kw.REPAIR))),
                include_self=False,
            ),
            (
                d.Choose(
                    "t1",
                    d.Sel(
                        E,
                        d.Loc.BATTLE,
                        (
                            UNIT,
                            d.LevelCmpRef(d.Stat.LV, d.Op.LE, d.EventCard("subject"), d.Stat.LV),
                        ),
                    ),
                ),
                d.Rest(T1),
            ),
            gate=d.Gate.PAIRED,
        ),
    )


@card("GD03-004")
def gd03_004(c: CardDef) -> d.CardScript:
    return _script(
        c,
        d.Triggered(
            d.Trigger(d.Ev.ATTACKS),
            (
                d.If(
                    d.Cmp(d.Count(_other_friendly("Titans")), d.Op.GE, 2),
                    (
                        d.Choose("t1", d.Sel(E, d.Loc.BATTLE, (UNIT, _current_hp(d.Op.LE, 5)))),
                        d.Rest(T1),
                    ),
                ),
            ),
        ),
    )


@card("GD03-006")
def gd03_006(c: CardDef) -> d.CardScript:
    return _script(
        c,
        d.Triggered(
            d.Trigger(d.Ev.DEPLOYED),
            (
                d.Choose(
                    "t1",
                    d.Sel(E, d.Loc.BATTLE, (UNIT, _current_hp(d.Op.LE, 3))),
                    count=2,
                    min_count=1,
                ),
                d.Rest(T1),
            ),
        ),
    )


@card("GD03-007")
def gd03_007(c: CardDef) -> d.CardScript:
    return _script(
        c,
        d.Triggered(
            d.Trigger(d.Ev.DESTROYED),
            (
                d.Choose("t1", d.Sel(E, d.Loc.BATTLE, (UNIT, _current_hp(d.Op.LE, 3)))),
                d.Rest(T1),
            ),
        ),
    )


def _exile_n_then(trash: d.Sel, n: int, then: tuple[d.Step, ...]) -> d.Step:
    """ "You may choose N cards from your trash. Exile them. If you do, ..." — only possible
    with N valid cards (resolution ruling:GD01-003:Q121)."""
    return d.If(
        d.Exists(trash, at_least=n),
        (d.May((d.Choose("t1", trash, count=n), d.Exile(T1), d.IfYouDo(then))),),
    )


@card("GD03-009")
def gd03_009(c: CardDef) -> d.CardScript:
    return _script(
        c,
        d.Triggered(
            d.Trigger(d.Ev.DEPLOYED),
            (
                _exile_n_then(
                    d.Sel(F, d.Loc.TRASH, (_trait("Titans"),)),
                    2,
                    (
                        d.Choose(
                            "t2", d.Sel(E, d.Loc.BATTLE, (UNIT, d.StatCmp(d.Stat.LV, d.Op.LE, 4)))
                        ),
                        d.Rest(T2),
                    ),
                ),
            ),
        ),
    )


@card("GD03-020")
def gd03_020(c: CardDef) -> d.CardScript:
    return _script(
        c,
        d.Triggered(
            d.Trigger(d.Ev.PAIRED),
            (
                d.If(
                    d.Cmp(d.Count(d.Sel(F, d.Loc.TRASH, (_trait("Cyclops Team"),))), d.Op.GE, 4),
                    (d.DeployToken(_token_key(c), 2, rested=True),),
                ),
            ),
        ),
        d.Constant(
            (
                d.RuleGrant(
                    d.RuleMod(
                        d.RuleKind.CANT_RECEIVE_DAMAGE,
                        damage_kind=d.DamageKind.BATTLE,
                        source_side=E,
                    )
                ),
            ),
            cond=d.Exists(d.Sel(F, d.Loc.BATTLE, (UNIT, d.NameContains(("Ad Balloon",))))),
        ),
    )


@card("GD03-024")
def gd03_024(c: CardDef) -> d.CardScript:
    return _script(
        c,
        d.Triggered(
            d.Trigger(d.Ev.LINKED),
            (
                d.If(
                    d.Exists(_other_friendly("Cyclops Team")),
                    (d.DeployToken(_token_key(c), 1, rested=True),),
                ),
            ),
        ),
    )


@card("GD03-025")
def gd03_025(c: CardDef) -> d.CardScript:
    return _script(
        c,
        d.Constant(
            (d.RuleGrant(d.RuleMod(d.RuleKind.FORCE_ATTACK_TARGET)),),
            scope=d.All(d.Sel(F, d.Loc.BATTLE, (UNIT, _trait("Maganac Corps")))),
        ),
    )


@card("GD03-033")
def gd03_033(c: CardDef) -> d.CardScript:
    return _script(
        c,
        d.Constant(
            (d.StatMod(ap=2),),
            scope=d.All(d.Sel(F, d.Loc.BATTLE, (UNIT, _trait("ZAFT")))),
            cond=d.IsTurn(),
            gate=d.Gate.PAIRED,
            gate_filters=(_trait("ZAFT"),),
        ),
        d.Triggered(
            d.Trigger(d.Ev.ATTACKS),
            (
                d.Choose("t1", ENEMY_UNITS),
                d.Damage(T1, d.CustomValue("wp_gd03_a_ap_per", (("per", 4),))),
            ),
        ),
    )


@card("GD03-037")
def gd03_037(c: CardDef) -> d.CardScript:
    battling_destroyed = d.Sel(
        E,
        d.Loc.BATTLE,
        (
            UNIT,
            d.IsRef(d.BattlingWith(d.This())),
            d.CustomFilter("wp_gd03_a_has_destroyed_effect"),
        ),
    )
    return _script(
        c,
        d.Constant(
            (d.KeywordGrant(d.Kw.FIRST_STRIKE),),
            cond=d.And((d.IsTurn(), d.Exists(battling_destroyed))),
            gate=d.Gate.LINKED,
        ),
    )


@card("GD03-044")
def gd03_044(c: CardDef) -> d.CardScript:
    return _script(
        c,
        d.Triggered(d.Trigger(d.Ev.DEPLOYED), (d.DeployToken(_token_key(c), 1, rested=True),)),
    )


@card("GD03-048")
def gd03_048(c: CardDef) -> d.CardScript:
    return _script(
        c,
        d.Burst(
            (
                d.If(
                    d.Cmp(d.ShieldCount(d.P.OPP), d.Op.LE, 3),
                    (d.DeployToken(_token_key(c), 1, rested=True),),
                ),
            )
        ),
    )


@card("GD03-049")
def gd03_049(c: CardDef) -> d.CardScript:
    return _script(
        c,
        d.Keyword(d.Kw.SUPPRESSION),
        d.Triggered(
            d.Trigger(d.Ev.DESTROYS_SHIELD_CARD, battle_only=True),
            (
                d.If(
                    d.Cmp(d.Count(d.Sel(F, d.Loc.TRASH, (_trait("CB"),))), d.Op.GE, 10),
                    (
                        d.Choose(
                            "t1",
                            d.Sel(E, d.Loc.BATTLE, (UNIT, d.CustomFilter("wp_gd03_a_lowest_hp"))),
                        ),
                        d.Destroy(T1),
                    ),
                ),
            ),
        ),
    )


@card("GD03-050")
def gd03_050(c: CardDef) -> d.CardScript:
    """Q121 resolution: activatable only with 3 valid cards in the trash."""
    trash = d.Sel(F, d.Loc.TRASH, (UNIT, _trait("Tekkadan", "Teiwaz")))
    return _script(
        c,
        d.Activated(
            d.Timing.MAIN,
            (),
            (
                d.Choose("t1", trash, count=3),
                d.Exile(T1),
                d.IfYouDo((d.Choose("t2", ENEMY_UNITS), d.Damage(T2, 2))),
            ),
            cond=d.Exists(trash, at_least=3),
        ),
    )


@card("GD03-051")
def gd03_051(c: CardDef) -> d.CardScript:
    return _script(
        c,
        d.Triggered(
            d.Trigger(d.Ev.LINKED),
            (
                d.Choose(
                    "t1",
                    d.Sel(F, d.Loc.TRASH, (UNIT, d.StatCmp(d.Stat.LV, d.Op.LE, 4))),
                    optional=True,
                ),
                d.CustomStep("wp_gd03_a_pay_cost_of", (("var", "t1"),)),
                d.IfYouDo((d.DeployCard(T1),)),
            ),
        ),
    )


@card("GD03-052")
def gd03_052(c: CardDef) -> d.CardScript:
    return _script(
        c,
        d.Keyword(d.Kw.SUPPORT, 2),
        d.Triggered(
            d.Trigger(
                d.Ev.DEALS_DAMAGE,
                battle_only=True,
                target_filters=(UNIT, d.StatCmp(d.Stat.LV, d.Op.LE, 5)),
            ),
            (
                d.If(
                    d.Exists(d.Sel(F, d.Loc.PAIRED, (PILOT, _trait("CB")))),
                    (d.Destroy(d.EventCard("target")),),
                ),
            ),
        ),
    )


@card("GD03-054")
def gd03_054(c: CardDef) -> d.CardScript:
    return _script(
        c,
        d.Keyword(d.Kw.HIGH_MANEUVER),
        d.Triggered(
            d.Trigger(d.Ev.PAIRED, pilot_filters=(_trait("X-Rounder"),)),
            (
                _exile_n_then(
                    d.Sel(F, d.Loc.TRASH, (_trait("Vagan"),)),
                    4,
                    (
                        d.Choose(
                            "t2", d.Sel(E, d.Loc.BATTLE, (UNIT, d.StatCmp(d.Stat.LV, d.Op.LE, 4)))
                        ),
                        d.Destroy(T2),
                    ),
                ),
            ),
        ),
    )


@card("GD03-056")
def gd03_056(c: CardDef) -> d.CardScript:
    """Q230: only when both Units can be chosen."""
    both = d.And((d.NotC(d.RefEmpty(T1)), d.NotC(d.RefEmpty(T2))))
    return _script(
        c,
        d.Triggered(
            d.Trigger(d.Ev.DEPLOYED),
            (
                d.If(
                    d.Exists(ENEMY_UNITS),
                    (
                        d.Choose("t1", FRIENDLY_UNITS),
                        d.Choose("t2", ENEMY_UNITS, distinct_from=("t1",)),
                        d.If(both, (d.Damage(d.Union((T1, T2)), 1),)),
                    ),
                ),
            ),
        ),
    )


@card("GD03-060")
def gd03_060(c: CardDef) -> d.CardScript:
    return _script(
        c,
        d.Triggered(
            d.Trigger(d.Ev.DAMAGED, whose_turn=d.P.YOU, battle_only=False),
            (d.DeployToken(_token_key(c), 1, rested=True),),
            once_per_turn=True,
        ),
    )


@card("GD03-061")
def gd03_061(c: CardDef) -> d.CardScript:
    return _script(
        c,
        d.Constant(
            (d.KeywordGrant(d.Kw.REPAIR, 3),),
            cond=d.RefMatches(d.This(), (_current_hp(d.Op.EQ, 1),)),
        ),
    )


@card("GD03-064")
def gd03_064(c: CardDef) -> d.CardScript:
    return _script(
        c,
        d.Triggered(
            d.Trigger(d.Ev.DEPLOYED),
            (
                d.Choose("t1", d.Sel(F, d.Loc.TRASH, (_trait("X-Rounder"),)), optional=True),
                d.AddToHand(T1),
                d.IfYouDo((d.Discard(1),)),
            ),
        ),
    )


@card("GD03-069")
def gd03_069(c: CardDef) -> d.CardScript:
    paired_this_turn = d.HappenedThisTurn(
        "paired", d.P.YOU, filters=(d.PairedTo((d.IsRef(d.This()),)),)
    )
    return _script(
        c,
        d.Keyword(d.Kw.HIGH_MANEUVER),
        d.Triggered(
            d.Trigger(d.Ev.TURN_END, self_only=False),
            (d.SetActive(d.This()),),
            cond=paired_this_turn,
            gate=d.Gate.LINKED,
        ),
    )


@card("GD03-071")
def gd03_071(c: CardDef) -> d.CardScript:
    aeug_units = d.Sel(F, d.Loc.TRASH, (UNIT, _trait("AEUG")))
    return _script(
        c,
        d.Triggered(
            d.Trigger(d.Ev.DEPLOYED),
            (
                d.Choose("t1", ENEMY_UNITS),
                d.CustomStep("wp_gd03_a_ap_minus_count", (("var", "t1"), ("sel", aeug_units))),
            ),
        ),
    )


@card("GD03-072")
def gd03_072(c: CardDef) -> d.CardScript:
    """Q233: the 'If' governs the 'Then, discard 1.' part too."""
    return _script(
        c,
        d.Keyword(d.Kw.BLOCKER),
        d.Triggered(
            d.Trigger(d.Ev.DEPLOYED),
            (
                d.If(
                    d.Exists(_other_friendly("Triple Ship Alliance")),
                    (d.Draw(1), d.Discard(1)),
                ),
            ),
        ),
    )


@card("GD03-073")
def gd03_073(c: CardDef) -> d.CardScript:
    return _script(
        c,
        d.Keyword(d.Kw.BLOCKER),
        d.Activated(
            d.Timing.ACTION,
            (),
            (
                d.Choose("t1", d.Sel(E, d.Loc.BATTLE, (UNIT, d.IsRef(d.BattlingWith(d.This()))))),
                d.Apply(T1, d.StatMod(ap=-3), Duration.THIS_BATTLE),
            ),
            cond=d.Cmp(d.Count(d.Sel(F, d.Loc.TRASH, (_trait("Gjallarhorn"),))), d.Op.GE, 6),
            once_per_turn=True,
            gate=d.Gate.LINKED,
        ),
    )


@card("GD03-076")
def gd03_076(c: CardDef) -> d.CardScript:
    """Q235: a Unit destroyed by that battle damage is no longer there to return."""
    return _script(
        c,
        d.Triggered(
            d.Trigger(
                d.Ev.DEALS_DAMAGE,
                self_only=False,
                subject=d.Sel(F, d.Loc.BATTLE, (UNIT, _trait("Triple Ship Alliance"))),
                whose_turn=d.P.YOU,
                battle_only=True,
                target_filters=(UNIT,),
            ),
            (
                d.BindVar(
                    "t1", d.All(d.Sel(E, d.Loc.BATTLE, (UNIT, d.IsRef(d.EventCard("target")))))
                ),
                d.If(d.NotC(d.RefEmpty(T1)), (d.May((d.ReturnToHand(T1),)),)),
            ),
            once_per_turn=True,
        ),
    )


@card("GD03-077")
def gd03_077(c: CardDef) -> d.CardScript:
    return _script(
        c,
        d.Triggered(
            d.Trigger(d.Ev.LINKED),
            (
                d.Choose(
                    "t1",
                    d.Sel(E, d.Loc.BATTLE, (UNIT, _current_hp(d.Op.LE, 3))),
                    count=3,
                    min_count=1,
                ),
                d.ReturnToHand(T1),
            ),
        ),
    )


@card("GD03-078")
def gd03_078(c: CardDef) -> d.CardScript:
    """The paired card follows this Unit to the trash (rule 3-3-6); return it from there."""
    return _script(
        c,
        d.Triggered(
            d.Trigger(d.Ev.DESTROYED),
            (d.ReturnToHand(d.EventCard("pilot")),),
            gate=d.Gate.LINKED,
        ),
    )


@card("GD03-079")
def gd03_079(c: CardDef) -> d.CardScript:
    return _script(
        c,
        d.Constant(
            (
                d.RuleGrant(
                    d.RuleMod(d.RuleKind.REST_SUBSTITUTE, name="base", source_filters=(UNIT,))
                ),
            ),
        ),
    )


@card("GD03-081")
def gd03_081(c: CardDef) -> d.CardScript:
    """Q236: this Unit's own deployment satisfies the condition."""
    deployed = d.HappenedThisTurn(
        "deployed", d.P.YOU, filters=(UNIT, _trait("Superpower Bloc", "UN"))
    )
    return _script(
        c,
        d.Constant(
            (d.RuleGrant(d.RuleMod(d.RuleKind.CANT_ATTACK)),),
            cond=d.NotC(deployed),
        ),
    )
