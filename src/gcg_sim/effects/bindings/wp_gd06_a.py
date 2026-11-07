"""Bindings for GD06-001..GD06-065 (work package WP-GD06-A).

Cards whose compiled script is missing or deviates from the card text, its rulings, or the
resolutions in ``data/overrides.json`` are bound here as whole cards. Correctly compiled lines
are reused through :func:`compile_parts`.
"""

from __future__ import annotations

from gcg_sim.cards.model import CardDef
from gcg_sim.effects import dsl as d
from gcg_sim.effects.bindings import card, custom_filter
from gcg_sim.effects.compiler import compile_parts
from gcg_sim.effects.compiler.abilities import CompileError
from gcg_sim.effects.registry import UnimplementedCardError
from gcg_sim.engine import view as V
from gcg_sim.engine.state import GameState
from gcg_sim.engine.types import Duration

FRIENDLY = d.Side.FRIENDLY
ENEMY = d.Side.ENEMY
UNIT = d.IsKind((d.CardKind.UNIT,))
PILOT = d.IsKind((d.CardKind.PILOT,))
BASE = d.IsKind((d.CardKind.BASE,))
UNIT_TOKEN = d.IsKind((d.CardKind.UNIT_TOKEN,))
RESOURCES = d.IsKind((d.CardKind.RESOURCE,))

ONCE_FREE = "wp_gd05_a_once_free"
USE_ONCE = "wp_gd05_a_use_once"
HIGHEST_ENEMY_UNIT_LV = "wp_st05_08_highest_enemy_unit_lv"
UNIQUE_NAMES = "wp_gd03_b_unique_names"
NAME_UNLIKE = "wp_gd06_a_name_unlike"
RETURN_LOOKED_BOTTOM = "return_looked_bottom"

ONCE_TAG_GD06_002 = -206_002


def _line(cdef: CardDef, index: int) -> tuple[d.Ability, ...]:
    line, res = compile_parts(cdef)[index]
    if isinstance(res, CompileError):
        raise UnimplementedCardError(f"{cdef.card_number}: {line!r}: {res}")
    return tuple(res)


def _units(side: d.Side, *filters: d.Filter) -> d.Sel:
    return d.Sel(side, d.Loc.BATTLE, (UNIT, *filters))


def _traits(*names: str) -> d.HasTrait:
    return d.HasTrait(names)


def _lv(op: d.Op, n: int) -> d.StatCmp:
    return d.StatCmp(d.Stat.LV, op, n)


def _ap(op: d.Op, n: int) -> d.StatCmp:
    return d.StatCmp(d.Stat.AP, op, n)


def _another(*filters: d.Filter) -> d.Exists:
    return d.Exists(_units(FRIENDLY, d.NotRef(d.This()), *filters))


def _looked(*filters: d.Filter) -> d.Sel:
    return d.Sel(FRIENDLY, d.Loc.DECK, (*filters, d.IsRef(d.Var("looked"))))


def _script(c: CardDef, *abilities: d.Ability, notes: str = "") -> d.CardScript:
    return d.CardScript(c.card_number, abilities=tuple(abilities), source="binding", notes=notes)


# ---------------------------------------------------------------------------------------------
# custom hooks


@custom_filter(NAME_UNLIKE)
def name_unlike(
    st: GameState, dv: V.Derived, ctx: V.Ctx, uid: int, params: dict[str, object]
) -> bool:
    """The card's name differs from every card bound to the frame variable ``var`` ("with
    unique card names", GD06-039)."""
    var = params["var"]
    assert isinstance(var, str)
    name = V.cdef(st, uid).name
    return all(V.cdef(st, u).name != name for u in ctx.vars.get(var, ()))


# ---------------------------------------------------------------------------------------------
# blue


@card("GD06-001")
def gd06_001(c: CardDef) -> d.CardScript:
    attacked = d.Trigger(
        d.Ev.ATTACKS,
        self_only=False,
        subject=_units(ENEMY),
        target_filters=(d.IsRef(d.This()),),
    )
    return _script(c, d.Triggered(attacked, (d.Draw(),), once_per_turn=True), *_line(c, 1))


@card("GD06-002")
def gd06_002(c: CardDef) -> d.CardScript:
    """Q489: an enemy Base or Shield destroyed with battle damage counts too; the 【Once per
    Turn】 is shared by both destruction events."""
    tag = (("tag", ONCE_TAG_GD06_002),)
    once = d.CustomCond(ONCE_FREE, tag)
    body = (
        d.If(
            once,
            (
                d.CustomStep(USE_ONCE, tag),
                d.Choose("t1", _units(FRIENDLY, _traits("Orb"))),
                d.Recover(d.Var("t1"), 3),
            ),
        ),
    )
    by_battle = d.Trigger(d.Ev.DESTROYS_BY_BATTLE, battle_only=True, whose_turn=d.P.YOU)
    shield_card = d.Trigger(d.Ev.DESTROYS_SHIELD_CARD, battle_only=True, whose_turn=d.P.YOU)
    return _script(
        c,
        d.Triggered(by_battle, body, cond=once),
        d.Triggered(shield_card, body, cond=once),
        *_line(c, 1),
    )


@card("GD06-003")
def gd06_003(c: CardDef) -> d.CardScript:
    hand_as_ap = d.Constant((d.StatMod(ap=d.HandSize()),))
    return _script(c, hand_as_ap, *_line(c, 1))


@card("GD06-004")
def gd06_004(c: CardDef) -> d.CardScript:
    """When the 【Destroyed】 effect resolves the Unit is already in the trash with its Pilot, so
    "this Unit's paired Pilot" is the Pilot recorded on the destroy event (rule 13-2-8-2-1),
    as for GD01-005."""
    return _script(
        c,
        d.Triggered(
            d.Trigger(d.Ev.DESTROYED),
            (d.If(d.IsTurn(), (d.ReturnToHand(d.EventCard("pilot")), d.Discard())),),
            gate=d.Gate.LINKED,
        ),
    )


# ---------------------------------------------------------------------------------------------
# green


@card("GD06-018")
def gd06_018(c: CardDef) -> d.CardScript:
    """Q493: the redirected damage is still battle damage; Q494: two such redirections that
    point at each other each apply once, so the original Unit receives the damage."""
    unit = d.AllOf((UNIT, _traits("Neo Zeon", "Long-Range Weapon")))
    base = d.AllOf((BASE, _traits("Neo Zeon")))
    shield = d.Sel(FRIENDLY, d.Loc.FIELD_UNITS_AND_BASES, (d.AnyOf((unit, base)),))
    redirect = d.RuleGrant(d.RuleMod(d.RuleKind.REDIRECT_BATTLE_DAMAGE))
    return _script(
        c,
        *_line(c, 0),
        d.Activated(
            d.Timing.ACTION,
            (),
            (
                d.Choose("t1", shield),
                d.Apply(d.This(), redirect, Duration.THIS_BATTLE, aux=d.Var("t1")),
            ),
            once_per_turn=True,
            gate=d.Gate.PAIRED,
        ),
    )


@card("GD06-019")
def gd06_019(c: CardDef) -> d.CardScript:
    """Q495: with several attractors the attacker chooses among them."""
    attract = d.Constant(
        (d.RuleGrant(d.RuleMod(d.RuleKind.FORCE_ATTACK_TARGET)),),
        scope=d.All(d.Sel(FRIENDLY, d.Loc.BATTLE, (UNIT_TOKEN,))),
    )
    return _script(c, attract, *_line(c, 1))


@card("GD06-020")
def gd06_020(c: CardDef) -> d.CardScript:
    """Q496: enemy Units, Bases and Shields count; Q497: destruction by effect damage counts."""
    neo_zeon = _units(FRIENDLY, _traits("Neo Zeon"))
    heal = (d.Recover(d.This(), 1),)
    return _script(
        c,
        d.Triggered(
            d.Trigger(
                d.Ev.DESTROYS_BY_BATTLE, self_only=False, subject=neo_zeon, whose_turn=d.P.YOU
            ),
            heal,
        ),
        d.Triggered(
            d.Trigger(
                d.Ev.DESTROYS_SHIELD_CARD, self_only=False, subject=neo_zeon, whose_turn=d.P.YOU
            ),
            heal,
        ),
    )


@card("GD06-021")
def gd06_021(c: CardDef) -> d.CardScript:
    """Q498: a new token of the same card number, without the chosen token's damage or effects."""
    return _script(
        c,
        d.Triggered(
            d.Trigger(d.Ev.DEPLOYED),
            (
                d.Choose("t1", d.Sel(FRIENDLY, d.Loc.BATTLE, (UNIT_TOKEN,))),
                d.DeployToken("", rested=True, copy_of="t1"),
            ),
        ),
    )


@card("GD06-023")
def gd06_023(c: CardDef) -> d.CardScript:
    target = _units(FRIENDLY, _traits("Neo Zeon"), _lv(d.Op.GE, 3))
    return _script(
        c,
        d.Triggered(
            d.Trigger(d.Ev.DEPLOYED),
            (
                d.Rest(d.This()),
                d.Choose("t1", target, after_then=True),
                d.Apply(d.Var("t1"), d.StatMod(ap=1)),
                d.If(d.RefMatches(d.Var("t1"), (d.IsLinked(),)), (d.SetActive(d.This()),)),
            ),
        ),
    )


@card("GD06-024")
def gd06_024(c: CardDef) -> d.CardScript:
    blocker = _units(ENEMY, d.HasKeyword(d.Kw.BLOCKER), _ap(d.Op.LE, 3))
    return _script(
        c,
        d.Triggered(d.Trigger(d.Ev.DESTROYED), (d.Choose("t1", blocker), d.Destroy(d.Var("t1")))),
    )


# ---------------------------------------------------------------------------------------------
# red


@card("GD06-033")
def gd06_033(c: CardDef) -> d.CardScript:
    """Q503: battle damage too; Q504: an area effect of an (AEUG) card deals +1 to each Unit."""
    boost = d.RuleMod(d.RuleKind.INCREASE_DAMAGE, amount=1, source_filters=(_traits("AEUG"),))
    return _script(c, d.Constant((d.RuleGrant(boost),)), *_line(c, 1))


@card("GD06-037")
def gd06_037(c: CardDef) -> d.CardScript:
    """Q509: the <First Strike> ends when the battled Unit is no longer damaged. The enemy
    Units' controller's own effects can't set them active (a rule that names "enemy effects"
    from the viewpoint of this card); the start-phase untap is not an effect."""
    battling_damaged = d.RefMatches(d.BattlingWith(d.This()), (UNIT, d.IsDamaged()))
    first_strike = d.Constant(
        (d.KeywordGrant(d.Kw.FIRST_STRIKE),), cond=d.And((d.IsTurn(), battling_damaged))
    )
    no_untap = d.RuleMod(d.RuleKind.CANT_BE_SET_ACTIVE, source_side=FRIENDLY)
    locked = d.Constant((d.RuleGrant(no_untap),), scope=d.All(_units(ENEMY)), gate=d.Gate.PAIRED)
    return _script(c, first_strike, locked)


@card("GD06-038")
def gd06_038(c: CardDef) -> d.CardScript:
    """Q510: any one of several Units tied for the highest Lv."""
    highest = d.StatCmp(d.Stat.LV, d.Op.GE, d.CustomValue(HIGHEST_ENEMY_UNIT_LV))
    return _script(
        c,
        *_line(c, 0),
        d.Triggered(
            d.Trigger(d.Ev.PAIRED),
            (d.Choose("t1", _units(ENEMY, highest)), d.Damage(d.Var("t1"), 3)),
        ),
    )


@card("GD06-039")
def gd06_039(c: CardDef) -> d.CardScript:
    """The cost destroys two Lv.3 (Gundam Team) Units with different names together with this
    Unit; Q511: the Lv.8 or higher Unit is deployed without paying its cost; Q512: its
    【Deploy】 activates."""
    core = (UNIT, _traits("Gundam Team"), _lv(d.Op.EQ, 3))
    core_sel = d.Sel(FRIENDLY, d.Loc.BATTLE, core)
    two_names = d.Cmp(d.CustomValue(UNIQUE_NAMES, (("sel", core_sel),)), d.Op.GE, 2)
    second = d.Sel(
        FRIENDLY,
        d.Loc.BATTLE,
        (*core, d.NotRef(d.Var("c1")), d.CustomFilter(NAME_UNLIKE, (("var", "c1"),))),
    )
    big = d.Sel(FRIENDLY, d.Loc.HAND, (UNIT, _traits("Gundam Team"), _lv(d.Op.GE, 8)))
    return _script(
        c,
        d.Activated(
            d.Timing.MAIN,
            (),
            (
                d.Choose("c1", core_sel, targeting=False),
                d.Choose("c2", second, targeting=False),
                d.Destroy(d.Union((d.Var("c1"), d.Var("c2"), d.This()))),
                d.Choose("t1", big, targeting=False),
                d.DeployCard(d.Var("t1")),
            ),
            cond=two_names,
        ),
        notes="the destruction cost is the first steps, guarded by the two-names condition",
    )


@card("GD06-042")
def gd06_042(c: CardDef) -> d.CardScript:
    """Q513: the Pilot is paired without paying its cost; Q514: looking is mandatory."""
    bio_cpu = _looked(PILOT, _traits("Biological CPU"))
    return _script(
        c,
        d.Triggered(
            d.Trigger(d.Ev.DEPLOYED),
            (
                d.LookTop(4),
                d.May(
                    (
                        d.Choose("t1", bio_cpu, targeting=False),
                        d.Pair(d.Var("t1"), d.This()),
                    )
                ),
                d.CustomStep(RETURN_LOOKED_BOTTOM),
            ),
        ),
    )


# ---------------------------------------------------------------------------------------------
# purple


@card("GD06-049")
def gd06_049(c: CardDef) -> d.CardScript:
    """Q515: looking is mandatory."""
    zeon_units = d.Count(d.Sel(FRIENDLY, d.Loc.TRASH, (UNIT, _traits("Zeon"))))
    enemy_base = d.All(d.Sel(ENEMY, d.Loc.BASE, (BASE,)))
    delaz = _looked(UNIT, _traits("Delaz Fleet"))
    return _script(
        c,
        d.Triggered(
            d.Trigger(d.Ev.PAIRED),
            (d.If(d.Cmp(zeon_units, d.Op.GE, 10), (d.Destroy(enemy_base),)),),
        ),
        d.Triggered(
            d.Trigger(d.Ev.DESTROYED),
            (
                d.LookTop(3),
                d.May(
                    (
                        d.Choose("t1", delaz, targeting=False),
                        d.AddToHand(d.Var("t1"), reveal=True),
                    )
                ),
                d.ToTrash(d.All(_looked())),
            ),
        ),
    )


@card("GD06-050")
def gd06_050(c: CardDef) -> d.CardScript:
    zaft_link = d.Exists(_units(FRIENDLY, _traits("ZAFT"), d.IsLinked()))
    in_trash = d.Constant((d.CostMod(cost=-3, level=-3),), cond=zaft_link, where=d.Where.TRASH)
    return _script(c, in_trash, *_line(c, 1))


@card("GD06-051")
def gd06_051(c: CardDef) -> d.CardScript:
    """Q518: an active Resource may be chosen; Q519: an EX Resource may be chosen."""
    granted = d.Triggered(
        d.Trigger(d.Ev.TURN_END, self_only=False),
        (
            d.Choose("t1", d.Sel(FRIENDLY, d.Loc.RESOURCE_AREA, (RESOURCES,)), count=2),
            d.SetActive(d.Var("t1")),
        ),
    )
    deploy = d.Triggered(
        d.Trigger(d.Ev.DEPLOYED),
        (
            d.If(
                _another(_traits("Alphacide")),
                (d.Apply(d.This(), d.AbilityGrant(c.card_number, 2), Duration.THIS_TURN),),
            ),
        ),
    )
    return d.CardScript(
        c.card_number,
        abilities=(*_line(c, 0), deploy),
        unit_abilities=(granted,),
        source="binding",
        notes="unit_abilities hold the end-of-turn effect the 【Deploy】 grants this Unit",
    )


@card("GD06-052")
def gd06_052(c: CardDef) -> d.CardScript:
    milled_zeon = d.Exists(
        d.Sel(FRIENDLY, d.Loc.TRASH, (_traits("Zeon"), d.IsRef(d.Var("milled"))))
    )
    return _script(
        c,
        *_line(c, 0),
        d.Triggered(
            d.Trigger(d.Ev.DEPLOYED),
            (
                d.Mill(1),
                d.If(
                    milled_zeon,
                    (d.Choose("t1", _units(ENEMY, _ap(d.Op.LE, 4))), d.Rest(d.Var("t1"))),
                ),
            ),
        ),
    )


@card("GD06-054")
def gd06_054(c: CardDef) -> d.CardScript:
    """Q522: the effect activates only when both a friendly and an enemy Unit can be chosen."""
    both = d.And((d.Exists(_units(FRIENDLY)), d.Exists(_units(ENEMY))))
    return _script(
        c,
        *_line(c, 0),
        d.Triggered(
            d.Trigger(d.Ev.DEPLOYED),
            (
                d.If(
                    both,
                    (
                        d.May(
                            (
                                d.Choose("t1", _units(FRIENDLY)),
                                d.Choose("t2", _units(ENEMY)),
                                d.Damage(d.Union((d.Var("t1"), d.Var("t2"))), 1),
                            )
                        ),
                    ),
                ),
            ),
        ),
        notes="compiled script dropped the enemy Unit",
    )


# ---------------------------------------------------------------------------------------------
# white


@card("GD06-065")
def gd06_065(c: CardDef) -> d.CardScript:
    """Q525: the Units set active stay unable to attack even if they lose <Blocker>."""
    blockers = d.All(
        _units(FRIENDLY, _traits("Dianna Counter", "Militia"), d.HasKeyword(d.Kw.BLOCKER))
    )
    return _script(
        c,
        *_line(c, 0),
        d.Activated(
            d.Timing.MAIN,
            (d.PayResources(4),),
            (
                d.BindVar("t1", blockers),
                d.SetActive(d.Var("t1")),
                d.Apply(d.Var("t1"), d.RuleGrant(d.RuleMod(d.RuleKind.CANT_ATTACK))),
            ),
            once_per_turn=True,
        ),
    )
