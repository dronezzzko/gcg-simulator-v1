"""Bindings for GD02-001..GD02-085 (work package WP-GD02-A)."""

from __future__ import annotations

from dataclasses import replace

from gcg_sim.cards.model import CardDef
from gcg_sim.cards.tokens import parse_token_specs
from gcg_sim.effects import dsl as d
from gcg_sim.effects.bindings import card, custom_step
from gcg_sim.effects.compiler import compile_parts, route_abilities
from gcg_sim.effects.compiler.abilities import CompileError
from gcg_sim.engine import view as V
from gcg_sim.engine.state import Frame, GameState
from gcg_sim.engine.types import Duration

PREFIX = "wp_gd02_a_"
START_PHASE_MARK = d.RuleGrant(d.RuleMod(d.RuleKind.CUSTOM, name=PREFIX + "start_phase_lock"))
NO_SET_ACTIVE = d.RuleGrant(d.RuleMod(d.RuleKind.CANT_BE_SET_ACTIVE))


def _line(c: CardDef, index: int) -> list[d.Ability]:
    """The compiled abilities of one text line of ``c`` (the line must compile)."""
    line, res = compile_parts(c)[index]
    if isinstance(res, CompileError):
        raise CompileError(f"{c.card_number} line {index} ({line!r}): {res}")
    return res


def _only(abilities: list[d.Ability]) -> d.Ability:
    (a,) = abilities
    return a


def _triggered(c: CardDef, index: int) -> d.Triggered:
    a = _only(_line(c, index))
    assert isinstance(a, d.Triggered)
    return a


def _script(c: CardDef, abilities: list[d.Ability]) -> d.CardScript:
    own, unit = route_abilities(c, abilities)
    return d.CardScript(c.card_number, abilities=own, unit_abilities=unit, source="binding")


def _units(side: d.Side, *filters: d.Filter) -> d.Sel:
    return d.Sel(side, d.Loc.BATTLE, (d.IsKind((d.CardKind.UNIT,)), *filters))


def _trash_count(*filters: d.Filter) -> d.Count:
    return d.Count(d.Sel(d.Side.FRIENDLY, d.Loc.TRASH, filters))


@card("GD02-002")
def gd02_002(c: CardDef) -> d.CardScript:
    """Q197: while this Unit is already active the effect is not activated, so its
    【Once per Turn】 use is not consumed (rules 1-3-2-1, 13-2-13-1)."""
    trig = _triggered(c, 0)
    return _script(c, [replace(trig, cond=d.RefMatches(d.This(), (d.IsRested(),)))])


@card("GD02-003")
def gd02_003(c: CardDef) -> d.CardScript:
    """【Destroyed】: the Unit and its Pilot are already in the trash, so "the card paired with
    this Unit" is the Pilot recorded on the destruction event, not a live pairing."""
    trig = _triggered(c, 0)
    steps = (
        d.May((d.Discard(filters=(d.IsKind((d.CardKind.UNIT,)),)),)),
        d.IfYouDo((d.ReturnToHand(d.EventCard("pilot")),)),
    )
    return _script(c, [replace(trig, steps=steps)])


@card("GD02-004")
def gd02_004(c: CardDef) -> d.CardScript:
    """Q172: the Unit only stays rested through the opponent's next start phase; effects in
    any other phase can set it active. A marker records the target; the lock itself exists
    from the end of this turn until the opponent's start step."""
    trig = _triggered(c, 0)
    choose = trig.steps[0]
    assert isinstance(choose, d.Choose)
    steps = (
        choose,
        d.Apply(d.Var(choose.var), START_PHASE_MARK, Duration.OPPONENT_NEXT_TURN),
        d.DelayedTrigger(
            d.Trigger(d.Ev.TURN_END, self_only=False, whose_turn=d.P.YOU),
            (d.CustomStep(PREFIX + "arm_start_phase_lock"),),
            Duration.OPPONENT_NEXT_TURN,
        ),
        d.DelayedTrigger(
            d.Trigger(d.Ev.TURN_START, self_only=False, whose_turn=d.P.OPP),
            (d.CustomStep(PREFIX + "release_start_phase_lock"),),
            Duration.OPPONENT_NEXT_TURN,
        ),
    )
    return _script(c, [replace(trig, steps=steps)])


def _own_lasting(st: GameState, f: Frame, key: int) -> list[int]:
    return [
        i
        for i, le in enumerate(st.lasting)
        if le.effect_key == key
        and le.source_uid == f.host
        and le.controller == f.controller
        and le.player < 0
    ]


@custom_step(PREFIX + "arm_start_phase_lock")
def arm_start_phase_lock(st: GameState, f: Frame, ctx: V.Ctx, params: dict[str, object]) -> bool:
    from gcg_sim.engine.interp import add_lasting

    R = V.reg()
    marks = _own_lasting(st, f, R.cont_key(START_PHASE_MARK))
    for i in marks:
        le = st.lasting[i]
        add_lasting(
            st, NO_SET_ACTIVE, le.controller, le.source_uid, le.targets, Duration.OPPONENT_NEXT_TURN
        )
    return bool(marks)


@custom_step(PREFIX + "release_start_phase_lock")
def release_start_phase_lock(
    st: GameState, f: Frame, ctx: V.Ctx, params: dict[str, object]
) -> bool:
    R = V.reg()
    drop = set(_own_lasting(st, f, R.cont_key(NO_SET_ACTIVE)))
    if not drop:
        return False
    st.lasting = [le for i, le in enumerate(st.lasting) if i not in drop]
    return True


@card("GD02-009")
def gd02_009(c: CardDef) -> d.CardScript:
    """No rested enemy Unit means no target, so the effect does not activate and its
    【Once per Turn】 use is kept (rules 10-3-3-1, 13-2-13-1)."""
    targets = _units(d.Side.ENEMY, d.IsRested())
    return _script(
        c,
        [
            d.Triggered(
                d.Trigger(d.Ev.AP_REDUCED, by_enemy=True),
                (d.Choose("t1", targets), d.Damage(d.Var("t1"), 2)),
                cond=d.Exists(targets),
                once_per_turn=True,
            )
        ],
    )


@card("GD02-011")
def gd02_011(c: CardDef) -> d.CardScript:
    """The Base/Shield this Unit is battling exists only while it attacks the player: the first
    card of the enemy shield area (resolution ambiguous:GD02-011:battling-shield, Q173)."""
    return _script(
        c,
        [
            d.Activated(
                d.Timing.ACTION,
                (d.DestroySelf(),),
                (d.DamageShieldArea(d.P.OPP, 6),),
                cond=d.And(
                    (
                        d.RefMatches(d.This(), (d.IsAttacking(),)),
                        d.AttackTargetIs("player"),
                        d.Exists(d.Sel(d.Side.ENEMY, d.Loc.SHIELD_AREA)),
                    )
                ),
            )
        ],
    )


@card("GD02-021")
def gd02_021(c: CardDef) -> d.CardScript:
    """Q175: the Lv.7 draw after "Then" is still part of the "If you do" branch."""
    trig = _triggered(c, 0)
    may, if_you_do, then = trig.steps
    assert isinstance(if_you_do, d.IfYouDo)
    steps = (may, d.IfYouDo((*if_you_do.steps, then)))
    return _script(c, [replace(trig, steps=steps)])


@card("GD02-022")
def gd02_022(c: CardDef) -> d.CardScript:
    """Without an (AGE System) Unit to choose the effect does not activate, so its
    【Once per Turn】 use is kept (rules 10-3-3-1, 13-2-13-1)."""
    trig = _triggered(c, 0)
    cond = d.Exists(_units(d.Side.FRIENDLY, d.HasTrait(("AGE System",))))
    return _script(c, [replace(trig, cond=cond)])


def _daughtress_token(c: CardDef, event: d.Ev) -> d.CardScript:
    (spec,) = parse_token_specs(c.effect)
    another = d.Exists(_units(d.Side.FRIENDLY, d.NotRef(d.This()), d.HasTrait(("New UNE",))))
    return _script(
        c,
        [
            d.Triggered(
                d.Trigger(event),
                (d.If(another, (d.DeployToken(spec.key, 1, rested=True),)),),
            )
        ],
    )


@card("GD02-043")
def gd02_043(c: CardDef) -> d.CardScript:
    return _daughtress_token(c, d.Ev.DEPLOYED)


@card("GD02-044")
def gd02_044(c: CardDef) -> d.CardScript:
    return _daughtress_token(c, d.Ev.DESTROYED)


@card("GD02-047")
def gd02_047(c: CardDef) -> d.CardScript:
    return _script(
        c,
        [
            d.Activated(
                d.Timing.MAIN,
                (d.RestSelf(),),
                (
                    d.Destroy(d.This()),
                    d.Choose("t1", _units(d.Side.ENEMY, d.StatCmp(d.Stat.LV, d.Op.LE, 5))),
                    d.Damage(d.Var("t1"), 1),
                ),
            )
        ],
    )


@card("GD02-053")
def gd02_053(c: CardDef) -> d.CardScript:
    others = _units(d.Side.FRIENDLY, d.NotRef(d.This()), d.HasTrait(("Vulture",)))
    return _script(
        c,
        [
            *_line(c, 0),
            d.Constant(
                (d.StatMod(ap=2),),
                scope=d.All(others),
                cond=d.And((d.IsTurn(d.P.YOU), d.Cmp(_trash_count(), d.Op.GE, 7))),
                gate=d.Gate.LINKED,
            ),
        ],
    )


@card("GD02-058")
def gd02_058(c: CardDef) -> d.CardScript:
    """A leading "If you do" also governs the part after "Then" (resolution of Q175)."""
    trig = _triggered(c, 0)
    choose, damage, if_you_do, then = trig.steps
    assert isinstance(if_you_do, d.IfYouDo)
    steps = (choose, damage, d.IfYouDo((*if_you_do.steps, then)))
    return _script(c, [replace(trig, steps=steps)])


@card("GD02-064")
def gd02_064(c: CardDef) -> d.CardScript:
    immune = d.RuleMod(
        d.RuleKind.CANT_RECEIVE_DAMAGE,
        damage_kind=d.DamageKind.EFFECT,
        source_filters=(d.IsKind((d.CardKind.COMMAND,)),),
        source_side=d.Side.ENEMY,
    )
    return _script(
        c,
        [
            d.Constant(
                (d.RuleGrant(immune),),
                cond=d.And((d.IsTurn(d.P.YOU), d.Cmp(_trash_count(), d.Op.GE, 7))),
            )
        ],
    )


@card("GD02-069")
def gd02_069(c: CardDef) -> d.CardScript:
    """ "It" in the last sentence is this Unit (the compiled line applied it to the Base); the
    restriction accompanies setting this Unit active (Q182: it covers Bases and Shields)."""
    act = _only(_line(c, 0))
    assert isinstance(act, d.Activated)
    choose, rest = act.steps[:2]
    steps = (
        choose,
        rest,
        d.IfYouDo(
            (
                d.SetActive(d.This()),
                d.Apply(d.This(), d.RuleGrant(d.RuleMod(d.RuleKind.CANT_ATTACK_PLAYER))),
            )
        ),
    )
    return _script(c, [replace(act, steps=steps)])


@card("GD02-070")
def gd02_070(c: CardDef) -> d.CardScript:
    """ "If you do, discard 2" belongs inside the trash condition (no draw, no discard)."""
    enough = d.Cmp(_trash_count(d.HasTrait(("Gjallarhorn",))), d.Op.GE, 4)
    return _script(
        c,
        [
            d.Triggered(
                d.Trigger(d.Ev.DEPLOYED),
                (d.If(enough, (d.Draw(2), d.IfYouDo((d.Discard(2),)))),),
            )
        ],
    )


@card("GD02-073")
def gd02_073(c: CardDef) -> d.CardScript:
    """Q185: only the enemy Unit currently battling this Unit gains <First Strike>."""
    return _script(
        c,
        [
            d.Constant(
                (d.KeywordGrant(d.Kw.FIRST_STRIKE),),
                scope=d.BattlingWith(d.This()),
                cond=d.IsTurn(d.P.OPP),
            )
        ],
    )


@card("GD02-085")
def gd02_085(c: CardDef) -> d.CardScript:
    """With 5 or more cards in hand nothing is performed, so the effect is not activated and
    its 【Once per Turn】 use is kept (resolution of GD02-002 Q197)."""
    burst = _line(c, 0)
    trig = _triggered(c, 1)
    small_hand = d.Cmp(d.HandSize(), d.Op.LE, 4)
    body = (d.If(small_hand, (d.Draw(),)),)
    return _script(c, [*burst, replace(trig, steps=body, cond=small_hand)])
