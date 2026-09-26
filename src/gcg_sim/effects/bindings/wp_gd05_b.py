"""Card bindings for GD05-071..GD05-130 (work package WP-GD05-B).

Every card here either failed to compile or compiled to a script that disagrees with its text,
its rulings, or a resolution in docs/CONFLICTS.md. Each ``CardScript.notes`` says why.
"""

from __future__ import annotations

from gcg_sim.cards.model import CardDef
from gcg_sim.cards.tokens import parse_token_specs
from gcg_sim.effects import dsl as d
from gcg_sim.effects.bindings import card, custom_cond, custom_filter, custom_step
from gcg_sim.engine import core
from gcg_sim.engine import view as V
from gcg_sim.engine.state import Frame, GameState
from gcg_sim.engine.types import Duration, Zone

FRIENDLY = d.Side.FRIENDLY
ENEMY = d.Side.ENEMY
ANY_SIDE = d.Side.ANY
UNIT = d.IsKind((d.CardKind.UNIT,))
T1 = d.Var("t1")
T2 = d.Var("t2")


def units(side: d.Side, *filters: d.Filter) -> d.Sel:
    return d.Sel(side, d.Loc.BATTLE, (UNIT, *filters))


def trait(*names: str) -> d.HasTrait:
    return d.HasTrait(names)


def current_hp(op: d.Op, n: int) -> d.StatCmp:
    """HP references in card text mean current HP (resolution of ruling GD03-049:Q224)."""
    return d.StatCmp(d.Stat.REMAINING_HP, op, n)


def burst_add_self() -> d.Burst:
    return d.Burst((d.AddToHand(d.ThisCard()),))


def burst_deploy_self() -> d.Burst:
    return d.Burst((d.DeployCard(d.ThisCard()),))


def deploy_add_shield() -> d.Triggered:
    return d.Triggered(d.Trigger(d.Ev.DEPLOYED), (d.ShieldToHand(),))


def script(
    c: CardDef,
    abilities: tuple[d.Ability, ...],
    unit_abilities: tuple[d.Ability, ...] = (),
    notes: str = "",
) -> d.CardScript:
    return d.CardScript(
        c.card_number,
        abilities=abilities,
        unit_abilities=unit_abilities,
        source="binding",
        notes=notes,
    )


# ---------------------------------------------------------------------------------------------
# custom hooks


@custom_step("wp_gd05_b_destroy_first_shield_area_cards")
def destroy_first_shield_area_cards(
    st: GameState, f: Frame, ctx: V.Ctx, params: dict[str, object]
) -> bool:
    """Destroy the first N cards of the opponent's shield area: the Base, then the top
    Shields, simultaneously (rulings GD05-107:Q405-Q407)."""
    count = params["count"]
    assert isinstance(count, int)
    p = 1 - f.controller
    base = list(st.zones[p][Zone.BASE])[:count]
    shields = list(st.zones[p][Zone.SHIELD])[: count - len(base)]
    did = False
    if base and core.destroy(st, base, battle=False, by=f.controller, source=f.host):
        did = True
    if shields:
        core.destroy_shields(st, p, shields, battle=False, source=f.host, by=f.controller)
        did = True
    return did


@custom_step("wp_gd05_b_resolving_command_to_trash")
def resolving_command_to_trash(
    st: GameState, f: Frame, ctx: V.Ctx, params: dict[str, object]
) -> bool:
    """End a played Command's 【Main】 by placing it into the trash (rule 3-4-4) so that
    "After activating this card's 【Main】, you may pair this card from your trash" can follow in
    the same resolution (rulings GD05-112:Q409, GD05-113:Q410, GD05-121:Q414, GD05-122:Q415)."""
    uid = f.card_uid
    if st.cards[uid].zone is not Zone.RESOLVING:
        return False
    core.move(st, uid, Zone.TRASH)
    if f.kind == "command":
        core.emit(st, d.Ev.COMMAND_RESOLVED, uid, player=f.controller)
    return True


@custom_filter("wp_gd05_b_can_pair")
def can_pair_filter(
    st: GameState, dv: V.Derived, ctx: V.Ctx, uid: int, params: dict[str, object]
) -> bool:
    from gcg_sim.engine.interp import can_pair

    return can_pair(st, dv, uid)


@custom_cond("wp_gd05_b_played_with_ex")
def played_with_ex(st: GameState, dv: V.Derived, ctx: V.Ctx, params: dict[str, object]) -> bool:
    """The resolving card's most recent play this turn used an EX Resource."""
    for h in reversed(st.history):
        if h.uid != ctx.card_uid:
            continue
        if h.kind == "played_with_ex":
            return True
        if h.kind == "play":
            return False
    return False


AFTER_MAIN_PAIR: tuple[d.Step, ...] = (
    d.CustomStep("wp_gd05_b_resolving_command_to_trash"),
    d.If(
        d.Exists(d.Sel(FRIENDLY, d.Loc.TRASH, (d.IsRef(d.ThisCard()),))),
        (
            d.Choose(
                "pair_with",
                units(FRIENDLY, trait("MF"), d.CustomFilter("wp_gd05_b_can_pair")),
                optional=True,
                targeting=False,
            ),
            d.Pair(d.ThisCard(), d.Var("pair_with")),
        ),
    ),
)

AFTER_MAIN_NOTE = (
    "compiler has no template for 'After activating this card's 【Main】, you may pair this card "
    "from your trash'; modelled inline as the last step of the resolution (Q409/Q410/Q414/Q415)"
)


# ---------------------------------------------------------------------------------------------
# Units


@card("GD05-072")
def gd05_072(c: CardDef) -> d.CardScript:
    return script(
        c,
        (
            d.Triggered(
                d.Trigger(d.Ev.LINKED),
                (d.Choose("t1", units(ENEMY, current_hp(d.Op.LE, 4))), d.Rest(T1)),
            ),
        ),
        notes="'with 4 or less HP' compiled to printed HP; resolution GD03-049:Q224 = current HP",
    )


# ---------------------------------------------------------------------------------------------
# Pilots


@card("GD05-083")
def gd05_083(c: CardDef) -> d.CardScript:
    return script(
        c,
        (burst_add_self(),),
        (
            d.Triggered(
                d.Trigger(d.Ev.PAIRED),
                (
                    d.Choose("t1", units(ENEMY, current_hp(d.Op.EQ, 1))),
                    d.ReturnToHand(T1),
                ),
            ),
        ),
        notes="'with 1 HP' compiled to printed HP; resolution GD03-049:Q224 = current HP",
    )


@card("GD05-086")
def gd05_086(c: CardDef) -> d.CardScript:
    attract = d.RuleMod(d.RuleKind.FORCE_ATTACK_TARGET, source_filters=(d.Not(d.IsLinked()),))
    return script(
        c,
        (burst_add_self(),),
        (d.Constant((d.RuleGrant(attract),), gate=d.Gate.LINKED),),
        notes="compile error (attract-attack constant limited to non-Link attackers)",
    )


@card("GD05-088")
def gd05_088(c: CardDef) -> d.CardScript:
    named = d.All(units(FRIENDLY, d.NameContains(("Gundam Lfrith", "Gundnode"))))
    return script(
        c,
        (burst_add_self(),),
        (d.Constant((d.StatMod(ap=1),), scope=d.Union((d.This(), named))),),
        notes="compile error ('This Unit and all your Units with ... get AP+1')",
    )


@card("GD05-089")
def gd05_089(c: CardDef) -> d.CardScript:
    special_move_activated = d.HappenedThisTurn(
        "command_activated", d.P.YOU, filters=(trait("Special Move"),)
    )
    add_to_hand = d.AddToHand(d.ThisCard())
    three_mf_in_trash = d.Cmp(d.Count(d.Sel(FRIENDLY, d.Loc.TRASH, (trait("MF"),))), d.Op.GE, 3)
    burst = d.Burst(
        (
            d.If(
                three_mf_in_trash,
                (
                    d.May((d.DeployCard(d.ThisCard(), as_unit=True),)),
                    d.If(d.NotC(d.DidLast()), (add_to_hand,)),
                ),
                (add_to_hand,),
            ),
        )
    )
    return script(
        c,
        (burst,),
        (
            d.Triggered(
                d.Trigger(d.Ev.ATTACKS),
                (
                    d.If(
                        special_move_activated,
                        (d.Choose("t1", units(ENEMY)), d.Damage(T1, 2)),
                    ),
                ),
                gate=d.Gate.LINKED,
            ),
        ),
        notes="compile error ('deploy it as an (AP3･HP3) Unit instead')",
    )


@card("GD05-094")
def gd05_094(c: CardDef) -> d.CardScript:
    reduce2 = d.RuleMod(
        d.RuleKind.REDUCE_DAMAGE,
        amount=2,
        damage_kind=d.DamageKind.BATTLE,
        source_side=ENEMY,
    )
    return script(
        c,
        (burst_add_self(),),
        (
            d.Triggered(
                d.Trigger(d.Ev.DESTROYED),
                (
                    d.Choose("t1", units(FRIENDLY, trait("Neo Zeon"))),
                    d.Apply(T1, d.RuleGrant(reduce2), Duration.THIS_TURN),
                ),
            ),
        ),
        notes="compile error ('During this turn, when it receives enemy battle damage, reduce')",
    )


@card("GD05-097")
def gd05_097(c: CardDef) -> d.CardScript:
    discarded = d.Var("discarded97")
    return script(
        c,
        (burst_add_self(),),
        (
            d.Triggered(
                d.Trigger(d.Ev.PAIRED),
                (
                    d.Draw(1),
                    d.Discard(1, var="discarded97"),
                    d.If(
                        d.RefMatches(
                            discarded, (d.IsKind((d.CardKind.COMMAND,)), trait("Special Move"))
                        ),
                        (d.May((d.ActivateMain(discarded),)),),
                    ),
                ),
            ),
        ),
        notes="compile error ('If you discard a (Special Move) Command ... activate its 【Main】')",
    )


@card("GD05-101")
def gd05_101(c: CardDef) -> d.CardScript:
    return script(
        c,
        (burst_add_self(),),
        (
            d.Triggered(
                d.Trigger(d.Ev.COST_PAID, self_only=False, subject=units(FRIENDLY)),
                (d.May((d.Recover(d.This(), 2),)),),
                cond=d.RefMatches(d.This(), (trait("Militia"),)),
                once_per_turn=True,
            ),
        ),
        notes="compile error ('When you pay ① or more for one of your Unit's effects')",
    )


# ---------------------------------------------------------------------------------------------
# Commands


@card("GD05-102")
def gd05_102(c: CardDef) -> d.CardScript:
    modes = d.ChooseMode(
        (
            (
                "return",
                (d.Choose("t1", units(ENEMY, current_hp(d.Op.LE, 5))), d.ReturnToHand(T1)),
            ),
            ("recover", (d.Choose("t2", units(ANY_SIDE)), d.Recover(T2, 3))),
        )
    )
    return script(
        c,
        (d.Command(d.Timing.ACTION, (modes,), cond=d.Exists(units(ANY_SIDE))),),
        notes=(
            "compiled only the first ■ mode and ran the second unconditionally; HP means "
            "current HP (Q224); playable only if some mode has a target (Q401/Q402)"
        ),
    )


@card("GD05-104")
def gd05_104(c: CardDef) -> d.CardScript:
    granted = d.Triggered(
        d.Trigger(d.Ev.DESTROYED),
        (d.Choose("t1", units(FRIENDLY, trait("League Militaire"))), d.SetActive(T1)),
        gate=d.Gate.LINKED,
    )
    return script(
        c,
        (
            d.Command(
                d.Timing.ACTION,
                (
                    d.Choose("t1", units(FRIENDLY, trait("Shrike Team"))),
                    d.Apply(T1, d.AbilityGrant(c.card_number, 1), Duration.THIS_TURN),
                ),
            ),
            granted,
        ),
        notes="compile error ('It gains the following effect during this turn: ■...')",
    )


@card("GD05-107")
def gd05_107(c: CardDef) -> d.CardScript:
    return script(
        c,
        (
            d.Burst((d.PlaceExResource(),)),
            d.Command(
                d.Timing.MAIN,
                (d.CustomStep("wp_gd05_b_destroy_first_shield_area_cards", (("count", 2),)),),
            ),
        ),
        notes="compile error ('Destroy the first 2 cards in that player's shield area')",
    )


@card("GD05-108")
def gd05_108(c: CardDef) -> d.CardScript:
    return script(
        c,
        (
            d.Command(
                d.Timing.ACTION,
                (
                    d.Choose("t1", units(FRIENDLY, d.IsRested(), trait("Academy"))),
                    d.If(
                        d.Exists(units(ENEMY, d.IsAttacking())),
                        (d.ChangeAttackTarget(T1),),
                    ),
                ),
            ),
        ),
        notes="compiled script retargeted any battle, including your own Unit's attack",
    )


@card("GD05-112")
def gd05_112(c: CardDef) -> d.CardScript:
    no_breach = d.Not(d.HasKeyword(d.Kw.BREACH))
    return script(
        c,
        (
            d.Command(
                d.Timing.MAIN,
                (
                    d.Choose("t1", units(FRIENDLY, trait("MF"), no_breach)),
                    d.Apply(T1, d.KeywordGrant(d.Kw.BREACH, 3), Duration.THIS_TURN),
                    *AFTER_MAIN_PAIR,
                ),
            ),
        ),
        notes=AFTER_MAIN_NOTE,
    )


@card("GD05-113")
def gd05_113(c: CardDef) -> d.CardScript:
    return script(
        c,
        (
            d.Command(
                d.Timing.MAIN,
                (
                    d.Choose("t1", units(FRIENDLY, trait("MF"), d.StatCmp(d.Stat.AP, d.Op.LE, 4))),
                    d.Apply(T1, d.StatMod(ap=2), Duration.THIS_TURN),
                    *AFTER_MAIN_PAIR,
                ),
            ),
        ),
        notes=AFTER_MAIN_NOTE,
    )


@card("GD05-118")
def gd05_118(c: CardDef) -> d.CardScript:
    return script(
        c,
        (
            d.Command(
                d.Timing.MAIN,
                (
                    d.Choose("t1", units(ENEMY)),
                    d.Apply(T1, d.StatMod(ap=-2), Duration.THIS_TURN),
                    d.If(d.CustomCond("wp_gd05_b_played_with_ex"), (d.Rest(T1),)),
                ),
            ),
        ),
        notes=(
            "compiled condition read an event payload a Command frame never has and rested the "
            "wrong card"
        ),
    )


@card("GD05-119")
def gd05_119(c: CardDef) -> d.CardScript:
    my_lv5_battlers = d.All(units(FRIENDLY, d.StatCmp(d.Stat.LV, d.Op.GE, 5), d.IsBattling()))
    return script(
        c,
        (
            d.Command(
                d.Timing.ACTION,
                (
                    d.Choose("t1", units(ENEMY, d.IsRef(d.BattlingWith(my_lv5_battlers)))),
                    d.Apply(T1, d.StatMod(ap=-3), Duration.THIS_BATTLE),
                ),
            ),
        ),
        notes="compile error ('enemy Unit that is battling one of your Units that is Lv.5 or higher')",
    )


@card("GD05-120")
def gd05_120(c: CardDef) -> d.CardScript:
    return script(
        c,
        (
            burst_add_self(),
            d.Command(
                d.Timing.MAIN_OR_ACTION,
                (
                    d.Choose("t1", units(ENEMY, current_hp(d.Op.LE, 4))),
                    d.Rest(T1),
                    d.Choose(
                        "t2",
                        units(FRIENDLY, d.NameContains(("Shining Gundam",))),
                        optional=True,
                        after_then=True,
                    ),
                    d.Apply(T2, d.KeywordGrant(d.Kw.FIRST_STRIKE), Duration.THIS_TURN),
                ),
            ),
        ),
        notes="compile error ('It gets <First Strike> during this turn')",
    )


@card("GD05-121")
def gd05_121(c: CardDef) -> d.CardScript:
    return script(
        c,
        (
            d.Command(
                d.Timing.MAIN,
                (
                    d.Choose("t1", units(ENEMY)),
                    d.Apply(T1, d.StatMod(ap=-2), Duration.THIS_TURN),
                    *AFTER_MAIN_PAIR,
                ),
            ),
        ),
        notes=AFTER_MAIN_NOTE,
    )


@card("GD05-122")
def gd05_122(c: CardDef) -> d.CardScript:
    return script(
        c,
        (
            d.Command(
                d.Timing.MAIN,
                (
                    d.Choose("t1", units(ENEMY, d.StatCmp(d.Stat.LV, d.Op.LE, 4))),
                    d.Rest(T1),
                    *AFTER_MAIN_PAIR,
                ),
            ),
        ),
        notes=AFTER_MAIN_NOTE,
    )


# ---------------------------------------------------------------------------------------------
# Bases


@card("GD05-123")
def gd05_123(c: CardDef) -> d.CardScript:
    small_damage_immunity = d.RuleMod(
        d.RuleKind.CANT_RECEIVE_DAMAGE,
        amount=2,
        damage_kind=d.DamageKind.EFFECT,
        source_side=ENEMY,
    )
    return script(
        c,
        (
            burst_deploy_self(),
            deploy_add_shield(),
            d.Constant(
                (d.RuleGrant(small_damage_immunity),),
                scope=d.All(units(FRIENDLY, trait("Orb"))),
                cond=d.IsTurn(d.P.OPP),
            ),
        ),
        notes="compile error ('can't receive 2 or less enemy effect damage')",
    )


@card("GD05-124")
def gd05_124(c: CardDef) -> d.CardScript:
    substitute = d.RuleMod(
        d.RuleKind.REST_SUBSTITUTE,
        source_filters=(UNIT, trait("League Militaire")),
        name="unit",
    )
    return script(
        c,
        (
            burst_deploy_self(),
            deploy_add_shield(),
            d.Constant((d.RuleGrant(substitute),), cond=d.IsTurn(d.P.YOU)),
        ),
        notes="compile error ('when you would rest a Unit ..., you may rest this Base instead')",
    )


@card("GD05-125")
def gd05_125(c: CardDef) -> d.CardScript:
    reduce1 = d.RuleMod(d.RuleKind.REDUCE_DAMAGE, amount=1, source_side=ENEMY)
    return script(
        c,
        (
            burst_deploy_self(),
            deploy_add_shield(),
            d.Activated(
                d.Timing.MAIN,
                (d.RestSelf(),),
                (
                    d.Choose("t1", units(FRIENDLY, trait("Londo Bell"))),
                    d.Apply(T1, d.RuleGrant(reduce1), Duration.THIS_TURN),
                ),
            ),
        ),
        notes="compile error ('During this turn, when it receives enemy damage, reduce it by 1')",
    )


@card("GD05-126")
def gd05_126(c: CardDef) -> d.CardScript:
    (gundnode,) = parse_token_specs(c.effect)
    aerial = units(FRIENDLY, d.NameContains(("Gundam Aerial",)), d.StatCmp(d.Stat.LV, d.Op.GE, 5))
    return script(
        c,
        (
            burst_deploy_self(),
            deploy_add_shield(),
            d.Activated(
                d.Timing.MAIN,
                (d.PayResources(2),),
                (d.If(d.Exists(aerial), (d.DeployToken(gundnode.key, 1),)),),
                once_per_turn=True,
            ),
        ),
        notes="compile error (token definition with a keyword clause)",
    )


@card("GD05-127")
def gd05_127(c: CardDef) -> d.CardScript:
    return script(
        c,
        (
            burst_deploy_self(),
            deploy_add_shield(),
            d.Triggered(
                d.Trigger(
                    d.Ev.LINKED,
                    self_only=False,
                    subject=units(FRIENDLY, trait("Phantom Pain")),
                ),
                (
                    d.Choose("t1", units(ENEMY)),
                    d.Apply(T1, d.RuleGrant(d.RuleMod(d.RuleKind.CANT_BLOCK)), Duration.THIS_TURN),
                ),
                once_per_turn=True,
            ),
        ),
        notes="compile error ('It can't activate <Blocker> during this turn')",
    )


@card("GD05-129")
def gd05_129(c: CardDef) -> d.CardScript:
    destroyed_by_neo_zeon_effect = d.HappenedThisTurn(
        "destroyed", d.P.YOU, by=d.P.YOU, filters=(UNIT,), source_filters=(trait("Neo Zeon"),)
    )
    neo_zeon_lv3 = d.Sel(
        FRIENDLY,
        d.Loc.HAND,
        (UNIT, trait("Neo Zeon"), d.StatCmp(d.Stat.LV, d.Op.LE, 3)),
    )
    return script(
        c,
        (
            burst_deploy_self(),
            deploy_add_shield(),
            d.Activated(
                d.Timing.MAIN,
                (d.RestSelf(),),
                (
                    d.If(
                        destroyed_by_neo_zeon_effect,
                        (d.Choose("t1", neo_zeon_lv3, targeting=False), d.DeployCard(T1)),
                    ),
                ),
            ),
        ),
        notes="compile error ('destroyed by one of your (Neo Zeon) card's effects')",
    )


@card("GD05-130")
def gd05_130(c: CardDef) -> d.CardScript:
    this_in_trash = d.Sel(FRIENDLY, d.Loc.TRASH, (d.IsRef(d.ThisCard()),))
    office = d.Sel(
        FRIENDLY,
        d.Loc.HAND,
        (d.IsKind((d.CardKind.BASE,)), d.NameContains(("Presidential Office",))),
    )
    return script(
        c,
        (
            burst_deploy_self(),
            deploy_add_shield(),
            d.Triggered(
                d.Trigger(d.Ev.DESTROYED),
                (
                    d.May(
                        (
                            d.Choose("self", this_in_trash, targeting=False),
                            d.Exile(d.Var("self")),
                        )
                    ),
                    d.IfYouDo(
                        (
                            d.Choose("t1", office, optional=True, targeting=False),
                            d.DeployCard(T1),
                        )
                    ),
                ),
            ),
        ),
        notes="compile error ('exile this card in your trash from the game')",
    )
