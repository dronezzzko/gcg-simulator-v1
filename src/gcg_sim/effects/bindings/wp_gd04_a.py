"""Bindings for GD04-001..GD04-074 (work package WP-GD04-A).

Each bound card replaces the compiled script entirely; its ``notes`` say what the text
compiler got wrong or could not parse.
"""

from __future__ import annotations

from gcg_sim.cards.model import CardDef
from gcg_sim.cards.tokens import parse_token_specs
from gcg_sim.effects import dsl as d
from gcg_sim.effects.bindings import card, custom_filter, custom_step
from gcg_sim.engine import core
from gcg_sim.engine import interp as I
from gcg_sim.engine import view as V
from gcg_sim.engine.state import Frame, GameState
from gcg_sim.engine.types import Duration, Zone

F = d.Side.FRIENDLY
E = d.Side.ENEMY
UNIT = d.IsKind((d.CardKind.UNIT,))
COMMAND = d.IsKind((d.CardKind.COMMAND,))
PILOT = d.IsKind((d.CardKind.PILOT,))


def _units(side: d.Side, *filters: d.Filter) -> d.Sel:
    return d.Sel(side, d.Loc.BATTLE, (UNIT, *filters))


def _trait(*traits: str) -> d.HasTrait:
    return d.HasTrait(traits)


def _token_key(c: CardDef, index: int = 0) -> str:
    return parse_token_specs(c.effect)[index].key


def _nonempty(*names: str) -> d.Cond:
    return d.And(tuple(d.NotC(d.RefEmpty(d.Var(n))) for n in names))


def _script(c: CardDef, *abilities: d.Ability, notes: str) -> d.CardScript:
    return d.CardScript(c.card_number, abilities=abilities, source="binding", notes=notes)


# ---------------------------------------------------------------------------------------------
# custom hooks


@custom_filter("wp_gd04_a_pilot_capable")
def _pilot_capable(
    st: GameState, dv: V.Derived, ctx: V.Ctx, uid: int, params: dict[str, object]
) -> bool:
    """Pilot cards and Command cards with a 【Pilot】 section (rule 3-4-6)."""
    return V.cdef(st, uid).is_pilot_capable


def _printed_keywords(st: GameState, uid: int) -> dict[d.Kw, int]:
    return V.reg().printed_keywords(st.cards[uid].def_id)


@custom_filter("wp_gd04_a_has_printed_keyword")
def _has_printed_keyword(
    st: GameState, dv: V.Derived, ctx: V.Ctx, uid: int, params: dict[str, object]
) -> bool:
    return bool(_printed_keywords(st, uid))


@custom_step("wp_gd04_a_copy_keywords")
def _copy_keywords(st: GameState, f: Frame, ctx: V.Ctx, params: dict[str, object]) -> bool:
    """GD04-067: this Unit gains, during this turn, every keyword effect on the chosen card."""
    chosen = f.vars.get(str(params["var"]), ())
    host = st.cards[f.host]
    if not chosen or host.zone is not Zone.BATTLE:
        return False
    target = ((f.host, host.zone_seq),)
    kws = _printed_keywords(st, chosen[0])
    for kw, amount in sorted(kws.items()):
        grant = d.KeywordGrant(kw, amount if kw in d.STACKING_KEYWORDS else 0)
        I.add_lasting(st, grant, f.controller, f.host, target, Duration.THIS_TURN)
    return bool(kws)


@custom_step("wp_gd04_a_ap_down_by_named_trash_units")
def _ap_down_by_named_trash_units(
    st: GameState, f: Frame, ctx: V.Ctx, params: dict[str, object]
) -> bool:
    """GD04-057: AP-X during this turn, X = Unit cards with the name part in your trash, fixed
    when the effect resolves."""
    sel = d.Sel(F, d.Loc.TRASH, (UNIT, d.NameContains((str(params["name"]),))))
    n = len(V.select(st, V.derived(st), ctx, sel))
    live = tuple(
        (u, st.cards[u].zone_seq)
        for u in f.vars.get(str(params["var"]), ())
        if st.cards[u].zone is Zone.BATTLE
    )
    if n <= 0 or not live:
        return False
    dv = V.derived(st)
    before = {u: V.ap_of(st, dv, u) for u, _ in live}
    I.add_lasting(st, d.StatMod(ap=-n), f.controller, f.host, live, Duration.THIS_TURN)
    dv = V.derived(st)
    group = core.next_group(st)
    for u, _ in live:
        if V.ap_of(st, dv, u) < before[u]:
            core.emit(
                st, d.Ev.AP_REDUCED, u, player=st.cards[u].owner, by=f.controller, group=group
            )
    return True


@custom_filter("wp_gd04_a_has_partner_in_trash")
def _has_partner_in_trash(
    st: GameState, dv: V.Derived, ctx: V.Ctx, uid: int, params: dict[str, object]
) -> bool:
    """GD04-071: another card with the partner trait remains in the same trash, so both
    required cards can still be chosen after this one."""
    trait = str(params["trait"])
    owner = st.cards[uid].owner
    return any(u != uid and trait in V.cdef(st, u).traits for u in st.zones[owner][Zone.TRASH])


# ---------------------------------------------------------------------------------------------
# cards


@card("GD04-001")
def gd04_001(c: CardDef) -> d.CardScript:
    pilot = d.PairedPilotOf(d.This())
    return _script(
        c,
        d.Triggered(
            d.Trigger(d.Ev.ATTACKS),
            steps=(
                d.If(
                    d.And(
                        (d.AttackTargetIs("unit"), d.RefMatches(pilot, (d.HasColor(("Blue",)),)))
                    ),
                    then=(d.May((d.ReturnToHand(pilot),), prompt="return the blue Pilot?"),),
                ),
            ),
            gate=d.Gate.LINKED,
        ),
        notes="compile error: selector suffix 'paired with this Unit'",
    )


@card("GD04-002")
def gd04_002(c: CardDef) -> d.CardScript:
    ef = _trait("Earth Federation")
    return _script(
        c,
        d.Constant((d.StatMod(ap=1),), scope=d.All(_units(F, ef)), cond=d.IsTurn()),
        d.Triggered(
            d.Trigger(d.Ev.DEPLOYED),
            steps=(
                d.DelayedTrigger(
                    d.Trigger(
                        d.Ev.DESTROYS_BY_BATTLE,
                        battle_only=True,
                        self_only=False,
                        subject=_units(F, ef),
                        target_filters=(UNIT,),
                    ),
                    steps=(
                        d.Choose("t1", _units(E, d.StatCmp(d.Stat.HP, d.Op.LE, 5))),
                        d.Rest(d.Var("t1")),
                    ),
                    duration=Duration.THIS_TURN,
                ),
            ),
        ),
        notes="compile error: delayed 'during this turn, when one of your Units destroys'",
    )


@card("GD04-007")
def gd04_007(c: CardDef) -> d.CardScript:
    return _script(
        c,
        d.Triggered(
            d.Trigger(d.Ev.ATTACKS),
            steps=(d.DeployToken(_token_key(c), 1),),
            gate=d.Gate.PAIRED,
        ),
        notes="compile error: inline token definition not matched by the deploy-token template",
    )


@card("GD04-011")
def gd04_011(c: CardDef) -> d.CardScript:
    return _script(
        c,
        d.Triggered(
            d.Trigger(d.Ev.DESTROYED),
            steps=(
                d.If(
                    d.Exists(_units(F, d.NotRef(d.This()), _trait("League Militaire"))),
                    then=(d.DeployToken(_token_key(c), 1),),
                ),
            ),
        ),
        notes="compile error: inline token definition not matched by the deploy-token template",
    )


@card("GD04-015")
def gd04_015(c: CardDef) -> d.CardScript:
    mine = _units(F, d.IsRested(False), _trait("League Militaire"))
    theirs = _units(E, d.StatCmp(d.Stat.LV, d.Op.LE, 3))
    return _script(
        c,
        d.Triggered(
            d.Trigger(d.Ev.DEPLOYED),
            steps=(
                d.If(
                    d.And((d.Exists(mine), d.Exists(theirs))),
                    then=(
                        d.Choose("t1", mine),
                        d.Choose("t2", theirs, distinct_from=("t1",)),
                        d.If(
                            _nonempty("t1", "t2"),
                            then=(d.Rest(d.Union((d.Var("t1"), d.Var("t2")))),),
                        ),
                    ),
                ),
            ),
        ),
        notes="compiled script rested one Unit when the other target was missing (ruling Q265)",
    )


@card("GD04-017")
def gd04_017(c: CardDef) -> d.CardScript:
    return _script(
        c,
        d.Triggered(
            d.Trigger(d.Ev.PAIRED, pilot_filters=(_trait("Newtype"),)),
            steps=(d.DeployToken(_token_key(c, 0), 2),),
        ),
        d.Triggered(
            d.Trigger(d.Ev.DESTROYED),
            steps=(d.DeployToken(_token_key(c, 1), 1, rested=True),),
        ),
        notes="compile error: inline token definitions not matched by the deploy-token template",
    )


def _dof_command_with_ex() -> tuple[d.Trigger, d.Cond]:
    trigger = d.Trigger(
        d.Ev.COMMAND_PLAYED,
        self_only=False,
        subject=d.Sel(F, d.Loc.TRASH, (COMMAND, _trait("Dawn of Fold"))),
        whose_turn=d.P.YOU,
    )
    return trigger, d.Cmp(d.EventAmount("ex_used"), d.Op.GE, 1)


@card("GD04-020")
def gd04_020(c: CardDef) -> d.CardScript:
    trigger, cond = _dof_command_with_ex()
    return _script(
        c,
        d.Triggered(trigger, steps=(d.Draw(1),), cond=cond, once_per_turn=True),
        notes="compile error: trigger 'when you play and activate ... using an EX Resource'",
    )


@card("GD04-021")
def gd04_021(c: CardDef) -> d.CardScript:
    trigger, cond = _dof_command_with_ex()
    lfrith = _units(F, d.NameContains(("Gundam Lfrith",)), d.IsPaired(False))
    played = d.Sel(
        F,
        d.Loc.TRASH,
        (d.IsRef(d.EventCard("subject")), d.CustomFilter("wp_gd04_a_pilot_capable")),
    )
    return _script(
        c,
        d.Keyword(d.Kw.BREACH, 3),
        d.Triggered(
            trigger,
            steps=(
                d.BindVar("cmd", d.All(played)),
                d.If(
                    d.And((d.NotC(d.RefEmpty(d.Var("cmd"))), d.Exists(lfrith))),
                    then=(
                        d.May(
                            (
                                d.Choose("unit", lfrith, targeting=False),
                                d.Pair(d.Var("cmd"), d.Var("unit")),
                            ),
                            prompt="pair that card from your trash?",
                        ),
                    ),
                ),
            ),
            cond=cond,
        ),
        notes="compile error: trigger 'when you play and activate ... using an EX Resource'",
    )


@card("GD04-022")
def gd04_022(c: CardDef) -> d.CardScript:
    tokens = d.Sel(F, d.Loc.BATTLE, (d.IsKind((d.CardKind.UNIT_TOKEN,)),))
    small = d.Sel(
        d.Side.ANY,
        d.Loc.BATTLE,
        (UNIT, d.IsToken(False), d.StatCmp(d.Stat.LV, d.Op.LE, 3)),
    )
    return _script(
        c,
        d.Constant((d.KeywordGrant(d.Kw.BREACH, 1),), scope=d.All(tokens)),
        d.Constant(
            (d.RuleGrant(d.RuleMod(d.RuleKind.DEPLOYED_RESTED)),),
            scope=d.All(small),
            gate=d.Gate.LINKED,
        ),
        notes="compile error: static 'are deployed rested'",
    )


@card("GD04-024")
def gd04_024(c: CardDef) -> d.CardScript:
    wanted = d.Sel(
        F,
        d.Loc.DECK,
        (
            d.IsKind((d.CardKind.UNIT, d.CardKind.COMMAND)),
            _trait("Academy"),
            d.IsRef(d.Var("looked")),
        ),
    )
    return _script(
        c,
        d.Triggered(
            d.Trigger(d.Ev.DEPLOYED),
            steps=(
                d.LookTop(3),
                d.May(
                    (d.Choose("t1", wanted, targeting=False), d.AddToHand(d.Var("t1"), reveal=True))
                ),
                d.CustomStep("return_looked_bottom"),
            ),
        ),
        notes="compile error: selector 'Unit card/Command card'",
    )


@card("GD04-026")
def gd04_026(c: CardDef) -> d.CardScript:
    return _script(
        c,
        d.Triggered(
            d.Trigger(d.Ev.DEPLOYED),
            steps=(
                d.LookTop(1),
                d.May((d.ToTrash(d.Var("looked")),), prompt="place it into your trash?"),
            ),
        ),
        notes="compile error: 'return it to the top of your deck or place it into your trash'",
    )


@card("GD04-029")
def gd04_029(c: CardDef) -> d.CardScript:
    cb_pilot = d.Sel(F, d.Loc.PAIRED, (PILOT, _trait("CB")))
    return _script(
        c,
        d.Constant(
            (
                d.RuleGrant(
                    d.RuleMod(
                        d.RuleKind.REDUCE_DAMAGE,
                        amount=1,
                        source_side=E,
                        once_per_turn=True,
                    )
                ),
            ),
            cond=d.Exists(cb_pilot),
        ),
        notes="compile error: conditional damage reduction",
    )


@card("GD04-036")
def gd04_036(c: CardDef) -> d.CardScript:
    others = _units(F, d.NotRef(d.This()), d.IsRested(False), _trait("CB"))
    victims = d.All(_units(E, d.StatCmp(d.Stat.LV, d.Op.LE, 6)))
    return _script(
        c,
        d.Triggered(
            d.Trigger(d.Ev.DEPLOYED),
            steps=(
                d.Choose("t1", others, count=2, min_count=1, optional=True),
                d.Rest(d.Var("t1")),
                d.IfYouDo((d.Damage(victims, d.VarSize("t1")),)),
            ),
        ),
        notes="compile error: damage equal to the number of Units rested with this effect",
    )


@card("GD04-039")
def gd04_039(c: CardDef) -> d.CardScript:
    neo_zeon_trash = d.Count(d.Sel(F, d.Loc.TRASH, (_trait("Neo Zeon"),)))
    t1 = d.Var("t1")
    return _script(
        c,
        d.Constant(
            (d.CostMod(cost=-4),),
            cond=d.Cmp(neo_zeon_trash, d.Op.GE, 8),
            where=d.Where.HAND,
        ),
        d.Triggered(
            d.Trigger(d.Ev.DEPLOYED),
            steps=(
                d.Choose("t1", _units(E)),
                d.If(
                    d.RefMatches(t1, (d.HasKeyword(d.Kw.REPAIR),)),
                    then=(d.Damage(t1, 3),),
                    otherwise=(d.Damage(t1, 1),),
                ),
            ),
        ),
        notes="compile error: 'If it has <Repair>, deal 3 damage instead'",
    )


@card("GD04-041")
def gd04_041(c: CardDef) -> d.CardScript:
    return _script(
        c,
        d.Triggered(d.Trigger(d.Ev.RESTED), steps=(d.SetActive(d.This()),), once_per_turn=True),
        notes="compile error: pronoun 'it' without antecedent",
    )


@card("GD04-042")
def gd04_042(c: CardDef) -> d.CardScript:
    source = _units(F, d.PairedWith((_trait("Cyber-Newtype"),)))
    return _script(
        c,
        d.Triggered(
            d.Trigger(d.Ev.DESTROYS_SHIELD_CARD, self_only=False, subject=source),
            steps=(
                d.Choose("t1", _units(E, d.StatCmp(d.Stat.AP, d.Op.LE, 5))),
                d.Damage(d.Var("t1"), 2),
            ),
            once_per_turn=True,
            gate=d.Gate.LINKED,
        ),
        notes="compile error: trigger 'when damage from one of your Units ... destroys'",
    )


@card("GD04-049")
def gd04_049(c: CardDef) -> d.CardScript:
    vulture = d.Sel(F, d.Loc.TRASH, (_trait("Vulture"),))
    victims = d.Sel(
        E,
        d.Loc.FIELD_UNITS_AND_BASES,
        (
            d.IsKind((d.CardKind.UNIT, d.CardKind.BASE)),
            d.StatCmp(d.Stat.LV, d.Op.LE, 8),
        ),
    )
    return _script(
        c,
        d.Keyword(d.Kw.SUPPRESSION),
        d.Triggered(
            d.Trigger(d.Ev.ATTACKS),
            steps=(
                d.If(
                    d.And((d.AttackTargetIs("player"), d.Cmp(d.Count(vulture), d.Op.GE, 7))),
                    then=(
                        d.May(
                            (d.Choose("t1", vulture, count=7), d.Exile(d.Var("t1"))),
                            prompt="exile 7 (Vulture) cards?",
                        ),
                        d.IfYouDo((d.Choose("t2", victims), d.Destroy(d.Var("t2")))),
                    ),
                ),
            ),
            gate=d.Gate.PAIRED,
        ),
        notes="compiled script let fewer than 7 cards be exiled (optional choice had no minimum)",
    )


@card("GD04-052")
def gd04_052(c: CardDef) -> d.CardScript:
    return _script(
        c,
        d.Triggered(
            d.Trigger(d.Ev.ATTACKS),
            steps=(
                d.Choose("t1", _units(E), optional=True),
                d.If(
                    _nonempty("t1"),
                    then=(d.Damage(d.Union((d.Var("t1"), d.This())), 2),),
                ),
            ),
            gate=d.Gate.PAIRED,
        ),
        notes="compiled script damaged this Unit even when no enemy Unit was chosen",
    )


@card("GD04-054")
def gd04_054(c: CardDef) -> d.CardScript:
    that_unit = d.Sel(E, d.Loc.BATTLE, (d.IsRef(d.EventCard("target")),))
    return _script(
        c,
        d.Triggered(
            d.Trigger(d.Ev.DEALS_DAMAGE, battle_only=True, target_filters=(UNIT,)),
            steps=(d.Destroy(d.All(that_unit)),),
        ),
        notes="compile error: 'destroy that enemy Unit'",
    )


@card("GD04-057")
def gd04_057(c: CardDef) -> d.CardScript:
    return _script(
        c,
        d.Triggered(
            d.Trigger(d.Ev.DEPLOYED),
            steps=(
                d.Choose("t1", _units(E, d.StatCmp(d.Stat.LV, d.Op.LE, 6))),
                d.CustomStep(
                    "wp_gd04_a_ap_down_by_named_trash_units",
                    (("name", "Gundam Virtue"), ("var", "t1")),
                ),
            ),
        ),
        notes="compiled AP reduction recounted the trash continuously instead of fixing it",
    )


@card("GD04-058")
def gd04_058(c: CardDef) -> d.CardScript:
    return _script(
        c,
        d.Triggered(
            d.Trigger(d.Ev.DESTROYED),
            steps=(d.If(d.IsTurn(), then=(d.ReturnToHand(d.EventCard("pilot")),)),),
            gate=d.Gate.PAIRED,
            gate_filters=(_trait("Vulture"),),
        ),
        notes="compiled script referred to the Pilot through the destroyed Unit, which has none",
    )


@card("GD04-063")
def gd04_063(c: CardDef) -> d.CardScript:
    weak = d.AnyOf((d.StatCmp(d.Stat.LV, d.Op.LE, 1), d.StatCmp(d.Stat.AP, d.Op.LE, 1)))
    return _script(
        c,
        d.Triggered(
            d.Trigger(d.Ev.DEPLOYED),
            steps=(d.Choose("t1", _units(E, weak)), d.Destroy(d.Var("t1"))),
        ),
        notes="compile error: selector suffix 'or has 1 or less AP'",
    )


@card("GD04-066")
def gd04_066(c: CardDef) -> d.CardScript:
    return _script(
        c,
        d.Keyword(d.Kw.SUPPRESSION),
        d.Triggered(
            d.Trigger(
                d.Ev.COMMAND_PLAYED, self_only=False, subject=d.Sel(F, d.Loc.TRASH, (COMMAND,))
            ),
            steps=(d.Choose("t1", _units(E)), d.Apply(d.Var("t1"), d.StatMod(ap=-2))),
        ),
        notes="compile error: trigger 'when you activate a Command's 【Main】/【Action】 effect'",
    )


@card("GD04-067")
def gd04_067(c: CardDef) -> d.CardScript:
    chosen = d.Sel(d.Side.ANY, d.Loc.TRASH, (UNIT, d.CustomFilter("wp_gd04_a_has_printed_keyword")))
    return _script(
        c,
        d.Activated(
            d.Timing.MAIN,
            costs=(d.PayResources(1),),
            steps=(
                d.Choose("t1", chosen),
                d.If(
                    _nonempty("t1"),
                    then=(
                        d.Apply(d.This(), d.StatMod(ap=1)),
                        d.CustomStep("wp_gd04_a_copy_keywords", (("var", "t1"),)),
                    ),
                ),
            ),
            once_per_turn=True,
        ),
        notes="compile error: 'gets AP+1 and all <keywords> on that Unit card'",
    )


@card("GD04-069")
def gd04_069(c: CardDef) -> d.CardScript:
    paid_for = (UNIT, _trait("Militia", "Dianna Counter"), d.NotRef(d.This()))
    return _script(
        c,
        d.Keyword(d.Kw.BLOCKER),
        d.Triggered(
            d.Trigger(d.Ev.TURN_END, self_only=False),
            steps=(
                d.Choose("t1", _units(F, _trait("Militia"))),
                d.SetActive(d.Var("t1")),
            ),
            cond=d.HappenedThisTurn("cost_paid", d.P.YOU, filters=paid_for),
            gate=d.Gate.LINKED,
        ),
        notes="compile error: trigger 'at the end of a turn where you have paid ①'",
    )


@card("GD04-071")
def gd04_071(c: CardDef) -> d.CardScript:
    sb = d.Sel(
        F,
        d.Loc.TRASH,
        (
            _trait("Superpower Bloc"),
            d.CustomFilter("wp_gd04_a_has_partner_in_trash", (("trait", "UN"),)),
        ),
    )
    un = d.Sel(F, d.Loc.TRASH, (_trait("UN"),))
    return _script(
        c,
        d.Burst(
            (
                d.If(
                    d.Exists(_units(E, _trait("CB"))),
                    then=(d.AddToHand(d.ThisCard()),),
                ),
            )
        ),
        d.Activated(
            d.Timing.MAIN,
            costs=(),
            steps=(
                d.Choose("t1", sb),
                d.Choose("t2", un, distinct_from=("t1",)),
                d.If(
                    _nonempty("t1", "t2"),
                    then=(
                        d.Exile(d.Union((d.Var("t1"), d.Var("t2")))),
                        d.IfYouDo(
                            (
                                d.SetActive(d.This()),
                                d.Apply(d.This(), d.RuleGrant(d.RuleMod(d.RuleKind.CANT_ATTACK))),
                            )
                        ),
                    ),
                ),
            ),
        ),
        notes="compiled script chose from the battle area and restricted the exiled cards",
    )


@card("GD04-074")
def gd04_074(c: CardDef) -> d.CardScript:
    return _script(
        c,
        d.Triggered(
            d.Trigger(d.Ev.ATTACKS),
            steps=(
                d.May((d.PayCost(1),), prompt="pay ①?"),
                d.IfYouDo((d.Draw(1), d.Discard(1))),
            ),
        ),
        notes="compiled script discarded without paying (ruling GD02-021:Q175 policy)",
    )
