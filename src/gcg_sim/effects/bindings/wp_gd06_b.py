"""Bindings for GD06-066..GD06-130 (work package WP-GD06-B).

Cards whose compiled script is missing or deviates from the card text, its rulings, or the
resolutions in ``data/overrides.json`` are bound here as whole cards. Correctly compiled lines
are reused through :func:`compile_parts`. For Pilot cards, ``unit_abilities`` hold the text the
paired Unit gains (rule 3-3-9-2) and ``abilities`` the Pilot's own text.
"""

from __future__ import annotations

from gcg_sim.cards.model import CardDef
from gcg_sim.cards.tokens import parse_token_specs
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
BASE = d.IsKind((d.CardKind.BASE,))
COMMAND = d.IsKind((d.CardKind.COMMAND,))

LINK_NAMES = "wp_gd06_b_link_names"


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


def _milled(*filters: d.Filter) -> d.Exists:
    return d.Exists(d.Sel(FRIENDLY, d.Loc.TRASH, (*filters, d.IsRef(d.Var("milled")))))


def _reduce_enemy_battle_damage(amount: int) -> d.RuleGrant:
    return d.RuleGrant(
        d.RuleMod(
            d.RuleKind.REDUCE_DAMAGE,
            amount=amount,
            damage_kind=d.DamageKind.BATTLE,
            source_side=ENEMY,
        )
    )


def _token_keys(c: CardDef) -> tuple[str, ...]:
    return tuple(s.key for s in parse_token_specs(c.effect))


def _pay_less(c: CardDef, *pair_filters: d.Filter) -> d.PlayModifier:
    """ "When playing this card from your hand and pairing it with <Unit>, it gets cost -1";
    the Lv. is not reduced (rulings GD06-086:Q532, GD06-100:Q537)."""
    return d.PlayModifier(cost=max(0, c.cost - 1), pair_filters=(UNIT, *pair_filters))


def _script(c: CardDef, *abilities: d.Ability, notes: str = "") -> d.CardScript:
    return d.CardScript(c.card_number, abilities=tuple(abilities), source="binding", notes=notes)


def _pilot(
    c: CardDef, own: tuple[d.Ability, ...], unit: tuple[d.Ability, ...], notes: str = ""
) -> d.CardScript:
    return d.CardScript(
        c.card_number, abilities=own, unit_abilities=unit, source="binding", notes=notes
    )


# ---------------------------------------------------------------------------------------------
# custom hooks


@custom_filter(LINK_NAMES)
def link_names(
    st: GameState, dv: V.Derived, ctx: V.Ctx, uid: int, params: dict[str, object]
) -> bool:
    """The Unit's link condition names the Pilot ``name`` ("a Unit with the [Mashymre Cello]
    link condition", GD06-086)."""
    name = params["name"]
    assert isinstance(name, str)
    link = V.cdef(st, uid).link
    return link is not None and name in link.name_parts


# ---------------------------------------------------------------------------------------------
# white Units


@card("GD06-066")
def gd06_066(c: CardDef) -> d.CardScript:
    """Q526: the paired card's 【Action】 is activated without paying its cost; Q527: the card
    stays paired. A paired card without an 【Action】 effect is not activated."""
    paired = d.PairedPilotOf(d.This())
    has_action = d.AnyOf((d.HasTiming("action"), d.HasTiming("main_or_action")))
    return _script(
        c,
        *_line(c, 0),
        d.Triggered(
            d.Trigger(d.Ev.ATTACKS),
            (d.If(d.RefMatches(paired, (has_action,)), (d.ActivateMain(paired),)),),
            gate=d.Gate.LINKED,
        ),
    )


@card("GD06-068")
def gd06_068(c: CardDef) -> d.CardScript:
    """Q529: an already rested Unit may be chosen and still gets AP-2."""
    partner = d.Exists(_units(FRIENDLY, d.NotRef(d.This()), _traits("G Team", "Preventer")))
    return _script(
        c,
        d.Triggered(
            d.Trigger(d.Ev.DEPLOYED),
            (
                d.Choose("t1", _units(ENEMY, _lv(d.Op.LE, 3))),
                d.Rest(d.Var("t1")),
                d.If(partner, (d.Apply(d.Var("t1"), d.StatMod(ap=-2)),)),
            ),
        ),
    )


@card("GD06-070")
def gd06_070(c: CardDef) -> d.CardScript:
    paid_three = d.HappenedThisTurn("resource_paid", d.P.YOU, filters=(UNIT,), at_least=3)
    return _script(
        c,
        *_line(c, 0),
        d.Triggered(
            d.Trigger(d.Ev.ATTACKS),
            (
                d.If(
                    paid_three,
                    (
                        d.Choose("t1", _units(ENEMY)),
                        d.Apply(d.Var("t1"), d.StatMod(ap=-5), Duration.THIS_BATTLE),
                    ),
                ),
            ),
        ),
    )


@card("GD06-071")
def gd06_071(c: CardDef) -> d.CardScript:
    """Q530: a (Special Move) Command paired as a Pilot whose 【Main】 an effect activates
    counts."""
    special_move = d.Sel(FRIENDLY, d.Loc.HAND, (COMMAND, _traits("Special Move")))
    return _script(
        c,
        d.Triggered(
            d.Trigger(
                d.Ev.COMMAND_PLAYED, self_only=False, subject=special_move, whose_turn=d.P.YOU
            ),
            (
                d.Choose("t1", _units(ENEMY)),
                d.Apply(d.Var("t1"), d.StatMod(ap=-2)),
                d.IfYouDo((d.Apply(d.This(), d.StatMod(ap=2)),)),
            ),
            once_per_turn=True,
        ),
    )


# ---------------------------------------------------------------------------------------------
# Pilots


@card("GD06-080")
def gd06_080(c: CardDef) -> d.CardScript:
    drew = d.HappenedThisTurn("draw_by_effect", d.P.YOU)
    heal = d.Activated(
        d.Timing.ACTION,
        (),
        (d.Choose("t1", _units(FRIENDLY)), d.Recover(d.Var("t1"), 2)),
        cond=drew,
        once_per_turn=True,
    )
    return _pilot(c, _line(c, 0), (heal,))


@card("GD06-084")
def gd06_084(c: CardDef) -> d.CardScript:
    """Copies of <Breach 1> add up (13-1-2-5); their number is fixed when the effect
    resolves."""
    count = d.Count(_units(FRIENDLY, _traits("Neo Zeon", "Long-Range Weapon")))
    paired = d.Triggered(
        d.Trigger(d.Ev.PAIRED),
        (d.Repeat(count, (d.Apply(d.This(), d.KeywordGrant(d.Kw.BREACH, 1)),)),),
    )
    return _pilot(c, _line(c, 0), (paired,))


@card("GD06-086")
def gd06_086(c: CardDef) -> d.CardScript:
    cheaper = _pay_less(c, d.CustomFilter(LINK_NAMES, (("name", "Mashymre Cello"),)))
    return _pilot(c, (*_line(c, 1), cheaper), ())


@card("GD06-092")
def gd06_092(c: CardDef) -> d.CardScript:
    not_stronger = d.LevelCmpRef(d.Stat.AP, d.Op.LE, d.This(), d.Stat.AP)
    paired = d.Triggered(
        d.Trigger(d.Ev.PAIRED),
        (
            d.If(
                d.RefMatches(d.This(), (_traits("Phantom Pain", "Orb"),)),
                (d.Choose("t1", _units(ENEMY, not_stronger)), d.Damage(d.Var("t1"), 1)),
            ),
        ),
    )
    return _pilot(c, (*_line(c, 0), *_line(c, 1)), (paired,))


@card("GD06-093")
def gd06_093(c: CardDef) -> d.CardScript:
    """Q534: "reduce" lowers the damage received."""
    lower = d.LevelCmpRef(d.Stat.LV, d.Op.LE, d.This(), d.Stat.LV)
    guard = d.RuleGrant(
        d.RuleMod(
            d.RuleKind.REDUCE_DAMAGE,
            amount=2,
            damage_kind=d.DamageKind.BATTLE,
            source_side=ENEMY,
            source_filters=(UNIT, lower),
        )
    )
    attack = d.Triggered(d.Trigger(d.Ev.ATTACKS), (d.Apply(d.This(), guard, Duration.THIS_BATTLE),))
    return _pilot(c, _line(c, 0), (attack,))


@card("GD06-094")
def gd06_094(c: CardDef) -> d.CardScript:
    """The chosen Unit "gains the following effect during this turn": a delayed trigger bound to
    it, so the Unit is the trigger's "this Unit". Q535: a Unit with 0 AP deals no battle damage,
    so it destroys nothing."""
    gained = d.DelayedTrigger(
        d.Trigger(d.Ev.DEALS_DAMAGE, battle_only=True, target_filters=(UNIT, _lv(d.Op.LE, 4))),
        (d.Destroy(d.EventCard("target")),),
        Duration.THIS_TURN,
        bind_vars=("t1",),
    )
    paired = d.Triggered(d.Trigger(d.Ev.PAIRED), (d.Choose("t1", _units(FRIENDLY)), gained))
    return _pilot(c, _line(c, 0), (paired,))


@card("GD06-096")
def gd06_096(c: CardDef) -> d.CardScript:
    attack = d.Triggered(
        d.Trigger(d.Ev.ATTACKS),
        (
            d.Mill(1),
            d.If(
                _milled(_traits("Zeon", "Alphacide")),
                (d.Apply(d.All(_units(ENEMY)), d.StatMod(ap=-1), Duration.THIS_BATTLE),),
            ),
        ),
        gate=d.Gate.LINKED,
    )
    return _pilot(c, _line(c, 0), (attack,))


@card("GD06-097")
def gd06_097(c: CardDef) -> d.CardScript:
    blocks = d.Triggered(d.Trigger(d.Ev.BLOCKS), (d.SetActive(d.This()),), once_per_turn=True)
    return _pilot(c, _line(c, 0), (blocks,))


@card("GD06-098")
def gd06_098(c: CardDef) -> d.CardScript:
    """Q536: the Lv. compared is this Unit's (the paired Unit's) current Lv."""
    lower = d.LevelCmpRef(d.Stat.LV, d.Op.LE, d.This(), d.Stat.LV)
    shield = d.RuleGrant(
        d.RuleMod(d.RuleKind.CANT_RECEIVE_DAMAGE, source_side=ENEMY, source_filters=(UNIT, lower))
    )
    guard = d.Constant(
        (shield,),
        scope=d.All(d.Sel(FRIENDLY, d.Loc.BASE, (BASE,))),
        cond=d.RefMatches(d.This(), (d.IsRested(),)),
        gate=d.Gate.LINKED,
    )
    return _pilot(c, _line(c, 0), (guard,))


@card("GD06-100")
def gd06_100(c: CardDef) -> d.CardScript:
    cheaper = _pay_less(c, _traits("Dianna Counter"), _lv(d.Op.GE, 5))
    return _pilot(c, (*_line(c, 1), cheaper), ())


# ---------------------------------------------------------------------------------------------
# Commands


@card("GD06-102")
def gd06_102(c: CardDef) -> d.CardScript:
    quiet = d.And((d.IsTurn(d.P.OPP), d.NotC(d.HappenedThisTurn("attack", d.P.OPP))))
    return _script(c, *_line(c, 0), d.Command(d.Timing.ACTION, (d.If(quiet, (d.Draw(2),)),)))


@card("GD06-105")
def gd06_105(c: CardDef) -> d.CardScript:
    calibarn, aerial = _token_keys(c)
    return _script(
        c,
        d.Command(
            d.Timing.MAIN,
            (
                d.ChooseMode(
                    (
                        ("option1", (d.DeployToken(calibarn),)),
                        ("option2", (d.DeployToken(aerial),)),
                    )
                ),
            ),
        ),
    )


@card("GD06-107")
def gd06_107(c: CardDef) -> d.CardScript:
    fleet = d.Exists(_units(FRIENDLY, _traits("Ghingnham Fleet")))
    blocker = _units(ENEMY, d.HasKeyword(d.Kw.BLOCKER), _ap(d.Op.LE, 3))
    return _script(
        c,
        d.Command(
            d.Timing.MAIN,
            (d.If(fleet, (d.Choose("t1", blocker), d.Destroy(d.Var("t1")))),),
        ),
        *_line(c, 1),
    )


@card("GD06-109")
def gd06_109(c: CardDef) -> d.CardScript:
    arms, legs = _token_keys(c)
    return _script(
        c,
        d.Command(
            d.Timing.MAIN,
            (
                d.ChooseMode(
                    (
                        ("option1", (d.DeployToken(arms, rested=True),)),
                        ("option2", (d.DeployToken(legs, rested=True),)),
                    )
                ),
            ),
        ),
    )


@card("GD06-112")
def gd06_112(c: CardDef) -> d.CardScript:
    """With one battle at a time, an enemy Unit battles a friendly (AEUG) Unit exactly when
    both are battling (as for GD02-118)."""
    aeug_battling = d.Exists(_units(FRIENDLY, _traits("AEUG"), d.IsBattling()))
    return _script(
        c,
        d.Command(
            d.Timing.ACTION,
            (d.Choose("t1", _units(ENEMY, d.IsBattling())), d.Damage(d.Var("t1"), 2)),
            cond=aeug_battling,
        ),
        *_line(c, 1),
    )


@card("GD06-114")
def gd06_114(c: CardDef) -> d.CardScript:
    """Q540, Q541: a mode is available only when its target can be chosen."""
    return _script(
        c,
        d.Command(
            d.Timing.ACTION,
            (
                d.ChooseMode(
                    (
                        (
                            "option1",
                            (
                                d.Choose("t1", _units(FRIENDLY, _traits("Alphacide"))),
                                d.Apply(
                                    d.Var("t1"),
                                    _reduce_enemy_battle_damage(3),
                                    Duration.THIS_BATTLE,
                                ),
                            ),
                        ),
                        (
                            "option2",
                            (
                                d.Choose("t2", _units(ENEMY, _lv(d.Op.LE, 2))),
                                d.Destroy(d.Var("t2")),
                            ),
                        ),
                    )
                ),
            ),
        ),
    )


@card("GD06-115")
def gd06_115(c: CardDef) -> d.CardScript:
    """Q542: the destroyed Unit's owner deploys the Nu Gundam token even without a token card
    of their own. It is deployed only when a Unit was destroyed."""
    sazabi, nu = _token_keys(c)
    return _script(
        c,
        d.Command(
            d.Timing.MAIN,
            (
                d.Choose("t1", _units(ENEMY, _lv(d.Op.LE, 4))),
                d.Destroy(d.Var("t1")),
                d.If(
                    d.DidLast(),
                    (d.DeployToken(sazabi), d.DeployToken(nu, player=d.P.OPP)),
                    otherwise=(d.DeployToken(sazabi),),
                ),
            ),
        ),
    )


@card("GD06-117")
def gd06_117(c: CardDef) -> d.CardScript:
    """Only an attacking enemy Unit has an attack target to change (as for EB01-077, GD05-108
    and ST07-013); Q545: the chosen Unit may already be battling."""
    rested_delaz = _units(FRIENDLY, d.IsRested(), _traits("Delaz Fleet"))
    enemy_attacking = d.Exists(_units(ENEMY, d.IsAttacking()))
    return _script(
        c,
        d.Command(
            d.Timing.ACTION,
            (
                d.Choose("t1", rested_delaz),
                d.If(enemy_attacking, (d.ChangeAttackTarget(d.Var("t1")),)),
            ),
        ),
        *_line(c, 1),
        notes="compiled ChangeAttackTarget also retargeted a friendly attacker",
    )


@card("GD06-118")
def gd06_118(c: CardDef) -> d.CardScript:
    white_blockers = d.Count(_units(FRIENDLY, d.HasColor(("White",)), d.HasKeyword(d.Kw.BLOCKER)))
    return _script(
        c,
        d.Command(
            d.Timing.MAIN_OR_ACTION,
            (
                d.Choose("t1", _units(ENEMY), count=white_blockers, min_count=0),
                d.Apply(d.Var("t1"), d.StatMod(ap=-2)),
            ),
        ),
    )


@card("GD06-121")
def gd06_121(c: CardDef) -> d.CardScript:
    def weaken(n: int) -> tuple[d.Step, ...]:
        return (d.Apply(d.Var("t1"), d.StatMod(ap=-n), Duration.THIS_BATTLE),)

    high_maneuver = d.RefMatches(d.Var("t1"), (d.HasKeyword(d.Kw.HIGH_MANEUVER),))
    return _script(
        c,
        d.Command(
            d.Timing.ACTION,
            (d.Choose("t1", _units(ENEMY)), d.If(high_maneuver, weaken(6), otherwise=weaken(2))),
        ),
        *_line(c, 1),
    )


# ---------------------------------------------------------------------------------------------
# Bases


@card("GD06-123")
def gd06_123(c: CardDef) -> d.CardScript:
    low_pilot = d.PairedWith((_lv(d.Op.LE, 3),))
    ready = d.And((d.IsTurn(), d.Exists(_units(FRIENDLY, _traits("Neo Zeon"), low_pilot))))
    blocker = _units(ENEMY, d.HasKeyword(d.Kw.BLOCKER), _ap(d.Op.LE, 3))
    return _script(
        c,
        *_line(c, 0),
        d.Triggered(
            d.Trigger(d.Ev.DEPLOYED),
            (
                d.ShieldToHand(),
                d.If(
                    ready,
                    (d.Choose("t1", blocker, after_then=True), d.Destroy(d.Var("t1"))),
                ),
            ),
        ),
    )


@card("GD06-125")
def gd06_125(c: CardDef) -> d.CardScript:
    """Q547: <Breach> damage is effect damage, so it does not trigger this."""
    rested_aeug = d.Exists(_units(FRIENDLY, _traits("AEUG"), d.IsRested()))
    return _script(
        c,
        *_line(c, 0),
        *_line(c, 1),
        d.Triggered(
            d.Trigger(d.Ev.DAMAGED, battle_only=True),
            (d.Choose("t1", _units(ENEMY)), d.Damage(d.Var("t1"), 1)),
            cond=rested_aeug,
            once_per_turn=True,
        ),
    )


@card("GD06-127")
def gd06_127(c: CardDef) -> d.CardScript:
    return _script(
        c,
        *_line(c, 0),
        d.Triggered(
            d.Trigger(d.Ev.DEPLOYED),
            (
                d.ShieldToHand(),
                d.If(
                    d.IsTurn(),
                    (
                        d.Mill(2),
                        d.If(
                            _milled(_traits("Alphacide")),
                            (
                                d.Choose("t1", _units(ENEMY, _ap(d.Op.LE, 2))),
                                d.Destroy(d.Var("t1")),
                            ),
                        ),
                    ),
                ),
            ),
        ),
    )


@card("GD06-128")
def gd06_128(c: CardDef) -> d.CardScript:
    """Q548: "reduce" lowers the damage received."""
    return _script(
        c,
        *_line(c, 0),
        *_line(c, 1),
        d.Activated(
            d.Timing.MAIN,
            (d.RestSelf(),),
            (
                d.Choose("t1", _units(FRIENDLY, _traits("Zeon"))),
                d.Apply(d.Var("t1"), _reduce_enemy_battle_damage(1), Duration.THIS_TURN),
            ),
        ),
    )


@card("GD06-129")
def gd06_129(c: CardDef) -> d.CardScript:
    """The Axis Base token (T-037) replaces nothing: this Base has left the base section."""
    (axis,) = _token_keys(c)
    shields = d.Exists(d.Sel(FRIENDLY, d.Loc.SHIELDS))
    return _script(
        c,
        *_line(c, 0),
        *_line(c, 1),
        d.Triggered(d.Trigger(d.Ev.DESTROYED), (d.If(shields, (d.DeployToken(axis),)),)),
    )
