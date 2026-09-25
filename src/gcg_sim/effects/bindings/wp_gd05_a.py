"""Bindings for GD05-001..GD05-070 (work package WP-GD05-A).

Cards whose compiled script is missing or deviates from the card text, its rulings, or the
resolutions in ``data/overrides.json`` are bound here as whole cards. Correctly compiled lines
are reused through :func:`compile_parts`.
"""

from __future__ import annotations

from gcg_sim.cards.model import CardDef
from gcg_sim.cards.tokens import parse_token_specs
from gcg_sim.effects import dsl as d
from gcg_sim.effects.bindings import card, custom_cond, custom_step, custom_value
from gcg_sim.effects.compiler import compile_parts
from gcg_sim.effects.compiler.abilities import CompileError
from gcg_sim.effects.registry import UnimplementedCardError
from gcg_sim.engine import view as V
from gcg_sim.engine.state import Frame, GameState
from gcg_sim.engine.types import Duration, Zone

FRIENDLY = d.Side.FRIENDLY
ENEMY = d.Side.ENEMY
UNIT = d.IsKind((d.CardKind.UNIT,))
PILOT = d.IsKind((d.CardKind.PILOT,))
COMMAND = d.IsKind((d.CardKind.COMMAND,))

ONCE_TAG_GD05_006 = -205_006
DEPLOYED_RESTED_TAG = -205_026


def _line(cdef: CardDef, index: int) -> tuple[d.Ability, ...]:
    line, res = compile_parts(cdef)[index]
    if isinstance(res, CompileError):
        raise UnimplementedCardError(f"{cdef.card_number}: {line!r}: {res}")
    return tuple(res)


def _units(side: d.Side, *filters: d.Filter) -> d.Sel:
    return d.Sel(side, d.Loc.BATTLE, (UNIT, *filters))


def _trash(*filters: d.Filter) -> d.Sel:
    return d.Sel(FRIENDLY, d.Loc.TRASH, filters)


def _traits(*names: str) -> d.HasTrait:
    return d.HasTrait(names)


def _pilot_in_play(trait: str) -> d.Exists:
    return d.Exists(d.Sel(FRIENDLY, d.Loc.PAIRED, (PILOT, _traits(trait))))


def _token_key(definition: str) -> str:
    (spec,) = parse_token_specs(definition)
    return spec.key


def _exile_exactly(
    var: str, sel: d.Sel, count: int, then: tuple[d.Step, ...]
) -> tuple[d.Step, ...]:
    """ "You may choose N <cards> from your trash. Exile them from the game. If you do, ..."
    (resolution ruling:GD01-003:Q121: no partial choice of N cards)."""
    return (
        d.If(
            d.Cmp(d.Count(sel), d.Op.GE, count),
            (d.May((d.Choose(var, sel, count), d.Exile(d.Var(var)), d.IfYouDo(then))),),
        ),
    )


def _reduce_enemy_damage(amount: int, *, once_per_turn: bool = False) -> d.RuleGrant:
    return d.RuleGrant(
        d.RuleMod(
            d.RuleKind.REDUCE_DAMAGE,
            amount=amount,
            source_side=ENEMY,
            once_per_turn=once_per_turn,
        )
    )


def _script(c: CardDef, *abilities: d.Ability, notes: str = "") -> d.CardScript:
    return d.CardScript(c.card_number, abilities=tuple(abilities), source="binding", notes=notes)


def _int_param(params: dict[str, object], key: str) -> int:
    v = params[key]
    if not isinstance(v, int):
        raise TypeError(f"custom hook parameter {key!r} must be an int, got {v!r}")
    return v


# ---------------------------------------------------------------------------------------------
# custom hooks


@custom_value("wp_gd05_a_lowest_enemy_unit_level")
def lowest_enemy_unit_level(
    st: GameState, dv: V.Derived, ctx: V.Ctx, params: dict[str, object]
) -> int:
    """Lowest Lv. among enemy Units in the battle area (ruling GD05-002:Q338: any tied Unit)."""
    levels = [V.level_of(st, u) for u in st.zones[1 - ctx.controller][Zone.BATTLE]]
    return min(levels, default=0)


def _once_key(st: GameState, host: int, tag: int) -> tuple[int, int, int]:
    return (tag, host, st.cards[host].zone_seq)


@custom_cond("wp_gd05_a_once_free")
def once_free(st: GameState, dv: V.Derived, ctx: V.Ctx, params: dict[str, object]) -> bool:
    """A 【Once per Turn】 shared by several triggered abilities of one card."""
    return ctx.host >= 0 and _once_key(st, ctx.host, _int_param(params, "tag")) not in st.once_used


@custom_step("wp_gd05_a_use_once")
def use_once(st: GameState, f: Frame, ctx: V.Ctx, params: dict[str, object]) -> bool:
    st.once_used.add(_once_key(st, f.host, _int_param(params, "tag")))
    return True


@custom_cond("wp_gd05_a_destroyed_by_effect")
def destroyed_by_effect(
    st: GameState, dv: V.Derived, ctx: V.Ctx, params: dict[str, object]
) -> bool:
    """The triggering destruction was performed by a 'destroy' effect, not by battle or effect
    damage (resolution ruling:GD05-054:Q368); with ``by="you"`` only by one of your effects."""
    by = ctx.ev("by")
    if ctx.ev("battle", 0) != 0 or by < 0:
        return False
    return params.get("by") != "you" or by == ctx.controller


def _deploy_group(st: GameState, subject: int) -> list[int]:
    """Cards deployed together with ``subject``: the contiguous run of 'deployed' history
    records (same player and deploying player) that contains the subject's latest record."""
    hist = st.history
    idx = next(
        (
            i
            for i in range(len(hist) - 1, -1, -1)
            if hist[i].kind == "deployed" and hist[i].uid == subject
        ),
        -1,
    )
    if idx < 0:
        return [subject]
    first = hist[idx]

    def same(i: int) -> bool:
        h = hist[i]
        return h.kind == "deployed" and h.player == first.player and h.by == first.by

    lo = idx
    while lo > 0 and same(lo - 1):
        lo -= 1
    hi = idx
    while hi + 1 < len(hist) and same(hi + 1):
        hi += 1
    return [hist[i].uid for i in range(lo, hi + 1)]


@custom_step("wp_gd05_a_deployed_rested")
def deployed_rested(st: GameState, f: Frame, ctx: V.Ctx, params: dict[str, object]) -> bool:
    """GD05-026: enemy Units matching ``sel`` that were just deployed become rested. The Unit is
    placed rested rather than rested by an effect, so no rested event is emitted (ruling
    GD05-026:Q354)."""
    sel = params["sel"]
    if not isinstance(sel, d.Sel):
        raise TypeError("wp_gd05_a_deployed_rested needs a Sel parameter 'sel'")
    subject = f.ev("subject")
    if subject < 0:
        return False
    dv = V.derived(st)
    did = False
    for uid in _deploy_group(st, subject):
        c = st.cards[uid]
        if c.zone is not Zone.BATTLE or c.rested or c.owner == f.controller:
            continue
        key = (DEPLOYED_RESTED_TAG, uid, c.zone_seq)
        if key in st.once_used:
            continue
        if uid != subject and not V.matches(st, dv, ctx, uid, sel.filters):
            continue
        c.rested = True
        st.once_used.add(key)
        did = True
    if did:
        st.touch()
    return did


# ---------------------------------------------------------------------------------------------
# blue


@card("GD05-002")
def gd05_002(c: CardDef) -> d.CardScript:
    grants = (d.AbilityGrant(c.card_number, 2), d.AbilityGrant(c.card_number, 3))
    deploy = d.Triggered(
        d.Trigger(d.Ev.DEPLOYED),
        (
            d.Choose("t1", _units(FRIENDLY), count=2, min_count=1),
            *(d.Apply(d.Var("t1"), g) for g in grants),
        ),
    )
    lowest = d.StatCmp(d.Stat.LV, d.Op.EQ, d.CustomValue("wp_gd05_a_lowest_enemy_unit_level"))
    attack = d.Triggered(
        d.Trigger(d.Ev.ATTACKS),
        (
            d.If(
                d.Cmp(d.HandSize(), d.Op.GE, 2),
                (
                    d.May((d.Discard(2),)),
                    d.IfYouDo(
                        (
                            d.Choose("t2", _units(ENEMY, lowest)),
                            d.ToDeck(d.Var("t2"), bottom=True),
                        )
                    ),
                ),
            ),
        ),
        gate=d.Gate.PAIRED,
    )
    draw_on_unit = d.Triggered(d.Trigger(d.Ev.DESTROYS_BY_BATTLE, battle_only=True), (d.Draw(),))
    draw_on_shield_card = d.Triggered(
        d.Trigger(d.Ev.DESTROYS_SHIELD_CARD, battle_only=True), (d.Draw(),)
    )
    return d.CardScript(
        c.card_number,
        abilities=(deploy, attack),
        unit_abilities=(draw_on_unit, draw_on_shield_card),
        source="binding",
        notes="unit_abilities hold the grant-only triggers the 【Deploy】 gives the chosen Units",
    )


@card("GD05-003")
def gd05_003(c: CardDef) -> d.CardScript:
    orb = _traits("Orb")
    was_paired_with_orb = d.RefMatches(d.EventCard("pilot"), (orb,))
    return _script(
        c,
        d.Triggered(
            d.Trigger(d.Ev.DESTROYED),
            (d.If(d.Or((_pilot_in_play("Orb"), was_paired_with_orb)), (d.Draw(),)),),
        ),
    )


@card("GD05-004")
def gd05_004(c: CardDef) -> d.CardScript:
    orb_units = d.Times(d.Count(_units(FRIENDLY, _traits("Orb"))), -1)
    no_big_units = d.NotC(d.Exists(_units(FRIENDLY, d.StatCmp(d.Stat.LV, d.Op.GE, 6))))
    discount = d.Constant(
        (d.CostMod(cost=orb_units, level=orb_units),), cond=no_big_units, where=d.Where.HAND
    )
    return _script(c, discount, *_line(c, 1))


@card("GD05-006")
def gd05_006(c: CardDef) -> d.CardScript:
    tag = (("tag", ONCE_TAG_GD05_006),)
    once = d.CustomCond("wp_gd05_a_once_free", tag)
    pluma = _token_key("[Pluma]((Calamity War)･AP2･HP1)")
    body = (d.If(once, (d.CustomStep("wp_gd05_a_use_once", tag), d.DeployToken(pluma, 1))),)
    tokens = d.Sel(
        FRIENDLY, d.Loc.BATTLE, (d.IsKind((d.CardKind.UNIT_TOKEN,)), _traits("Calamity War"))
    )
    return _script(
        c,
        d.Triggered(
            d.Trigger(d.Ev.DESTROYS_BY_BATTLE, battle_only=True, whose_turn=d.P.YOU),
            body,
            cond=once,
        ),
        d.Triggered(
            d.Trigger(d.Ev.DESTROYS_SHIELD_CARD, whose_turn=d.P.YOU, battle_only=True),
            body,
            cond=once,
        ),
        d.Constant((d.KeywordGrant(d.Kw.REPAIR, d.Count(tokens)),), cond=d.Exists(tokens)),
    )


# ---------------------------------------------------------------------------------------------
# green


@card("GD05-017")
def gd05_017(c: CardDef) -> d.CardScript:
    londo_bell = _trash(_traits("Londo Bell"))
    battle = (d.Choose("t2", _units(ENEMY)), d.StartBattle(d.This(), d.Var("t2")))
    return _script(
        c,
        *_line(c, 0),
        d.Triggered(d.Trigger(d.Ev.PAIRED), _exile_exactly("t1", londo_bell, 3, battle)),
    )


@card("GD05-018")
def gd05_018(c: CardDef) -> d.CardScript:
    ex_resource = d.Sel(FRIENDLY, d.Loc.RESOURCE_AREA, (d.IsKind((d.CardKind.EX_RESOURCE,)),))
    shield = d.Triggered(
        d.Trigger(d.Ev.EXILED, self_only=False, subject=ex_resource),
        (
            d.Choose("t1", _units(FRIENDLY), optional=True),
            d.Apply(d.Var("t1"), _reduce_enemy_damage(3), Duration.THIS_TURN),
        ),
    )
    return _script(c, shield, *_line(c, 1))


@card("GD05-021")
def gd05_021(c: CardDef) -> d.CardScript:
    reduce = d.Constant(
        (_reduce_enemy_damage(2, once_per_turn=True),), cond=_pilot_in_play("Earth Federation")
    )
    return _script(c, *_line(c, 0), reduce)


@card("GD05-022")
def gd05_022(c: CardDef) -> d.CardScript:
    guard = d.Activated(
        d.Timing.ACTION,
        (d.ExileCards(_trash(COMMAND), 2),),
        (d.Apply(d.This(), _reduce_enemy_damage(2), Duration.THIS_BATTLE),),
    )
    return _script(c, *_line(c, 0), guard)


@card("GD05-026")
def gd05_026(c: CardDef) -> d.CardScript:
    namesakes = d.Count(_units(FRIENDLY, d.NameContains(("Gundam Lfrith", "Gundnode"))))
    low_enemy = _units(ENEMY, d.StatCmp(d.Stat.LV, d.Op.LE, d.Sum((namesakes, 1))))
    return _script(
        c,
        d.Triggered(
            d.Trigger(d.Ev.DEPLOYED, self_only=False, subject=low_enemy),
            (d.CustomStep("wp_gd05_a_deployed_rested", (("sel", low_enemy),)),),
        ),
        notes="static 'deployed rested' approximated by a trigger that rests without an event",
    )


# ---------------------------------------------------------------------------------------------
# red


@card("GD05-033")
def gd05_033(c: CardDef) -> d.CardScript:
    special_moves = _trash(COMMAND, _traits("Special Move"))
    hit = (d.DamageShieldArea(d.P.OPP, 5),)
    return _script(
        c, d.Triggered(d.Trigger(d.Ev.ATTACKS), _exile_exactly("t1", special_moves, 2, hit))
    )


@card("GD05-034")
def gd05_034(c: CardDef) -> d.CardScript:
    phantom_pain = d.Sel(
        FRIENDLY,
        d.Loc.HAND,
        (UNIT, _traits("Phantom Pain"), d.StatCmp(d.Stat.LV, d.Op.LE, 4)),
    )
    return _script(
        c,
        d.Triggered(
            d.Trigger(d.Ev.DESTROYS_SHIELD_CARD, battle_only=True),
            (
                d.May((d.Discard(1, player=d.P.OPP),), player=d.P.OPP),
                d.If(
                    d.NotC(d.DidLast()),
                    (
                        d.Choose("t1", phantom_pain, optional=True, targeting=False),
                        d.DeployCard(d.Var("t1")),
                    ),
                ),
            ),
            once_per_turn=True,
            gate=d.Gate.PAIRED,
        ),
    )


@card("GD05-036")
def gd05_036(c: CardDef) -> d.CardScript:
    mf = _units(FRIENDLY, d.NotRef(d.This()), d.IsRested(False), _traits("MF"))
    low = d.LevelCmpRef(d.Stat.LV, d.Op.LE, d.Var("t1"), d.Stat.LV)
    return _script(
        c,
        d.Triggered(
            d.Trigger(d.Ev.PAIRED),
            (
                d.Choose("t1", mf, optional=True),
                d.Rest(d.Var("t1")),
                d.IfYouDo((d.Damage(d.All(_units(ENEMY, low)), 2),)),
            ),
        ),
    )


@card("GD05-041")
def gd05_041(c: CardDef) -> d.CardScript:
    discarded = d.HappenedThisTurn("discard", player=d.P.OPP, by=d.P.YOU)
    return _script(c, d.Constant((d.CostMod(cost=-2),), cond=discarded, where=d.Where.HAND))


@card("GD05-046")
def gd05_046(c: CardDef) -> d.CardScript:
    return _script(
        c,
        d.Triggered(
            d.Trigger(d.Ev.PAIRED, pilot_filters=(_traits("Phantom Pain"),)),
            (d.If(d.Cmp(d.HandSize(d.P.OPP), d.Op.GE, 4), (d.Discard(1, player=d.P.OPP),)),),
        ),
    )


# ---------------------------------------------------------------------------------------------
# purple


@card("GD05-049")
def gd05_049(c: CardDef) -> d.CardScript:
    non_battling = _units(ENEMY, d.IsBattling(False))
    return _script(
        c,
        *_line(c, 0),
        d.Triggered(
            d.Trigger(d.Ev.ATTACKS),
            (
                d.Choose("t1", _units(FRIENDLY), optional=True),
                d.Destroy(d.Var("t1")),
                d.IfYouDo((d.Choose("t2", non_battling, chooser=d.P.OPP), d.Destroy(d.Var("t2")))),
            ),
        ),
    )


@card("GD05-050")
def gd05_050(c: CardDef) -> d.CardScript:
    small_unpaired = (UNIT, d.StatCmp(d.Stat.LV, d.Op.LE, 4), d.IsPaired(False))
    return _script(
        c,
        d.Triggered(
            d.Trigger(d.Ev.DEALS_DAMAGE, battle_only=True, target_filters=small_unpaired),
            (d.Destroy(d.EventCard("target")),),
        ),
        d.Triggered(d.Trigger(d.Ev.DESTROYED), (d.Mill(2),)),
    )


@card("GD05-051")
def gd05_051(c: CardDef) -> d.CardScript:
    damage_as_ap = d.Constant((d.StatMod(ap=d.StatOf(d.This(), d.Stat.DAMAGE)),))
    return _script(c, damage_as_ap, *_line(c, 1))


@card("GD05-052")
def gd05_052(c: CardDef) -> d.CardScript:
    milled_neo_zeon = _trash(UNIT, _traits("Neo Zeon"), d.IsRef(d.Var("milled")))
    return _script(
        c,
        d.Triggered(
            d.Trigger(d.Ev.DEPLOYED),
            (
                d.Choose("t1", _units(FRIENDLY, d.NotRef(d.This())), optional=True),
                d.Destroy(d.Var("t1")),
                d.IfYouDo(
                    (
                        d.Mill(3, var="milled"),
                        d.Choose("t2", milled_neo_zeon, targeting=False),
                        d.AddToHand(d.Var("t2")),
                    )
                ),
            ),
        ),
    )


@card("GD05-053")
def gd05_053(c: CardDef) -> d.CardScript:
    by_own_neo_zeon_effect = d.And(
        (
            d.CustomCond("wp_gd05_a_destroyed_by_effect", (("by", "you"),)),
            d.RefMatches(d.EventCard("source"), (_traits("Neo Zeon"),)),
        )
    )
    itself_in_trash = d.All(_trash(d.IsRef(d.This())))
    return _script(
        c,
        d.Triggered(
            d.Trigger(d.Ev.DESTROYED),
            (d.If(by_own_neo_zeon_effect, (d.AddToHand(itself_in_trash),)),),
        ),
    )


@card("GD05-054")
def gd05_054(c: CardDef) -> d.CardScript:
    return _script(
        c,
        *_line(c, 0),
        d.Triggered(
            d.Trigger(d.Ev.DESTROYED, self_only=False, subject=_units(FRIENDLY)),
            (d.Draw(),),
            cond=d.CustomCond("wp_gd05_a_destroyed_by_effect"),
            once_per_turn=True,
        ),
    )


@card("GD05-057")
def gd05_057(c: CardDef) -> d.CardScript:
    return _script(
        c,
        d.Activated(
            d.Timing.MAIN,
            (),
            (
                d.Choose("t1", _units(FRIENDLY, d.NotRef(d.This()))),
                d.Destroy(d.Var("t1")),
                d.IfYouDo(
                    (
                        d.SetActive(d.This()),
                        d.Apply(d.This(), d.RuleGrant(d.RuleMod(d.RuleKind.CANT_ATTACK_PLAYER))),
                    )
                ),
            ),
            once_per_turn=True,
        ),
    )


@card("GD05-059")
def gd05_059(c: CardDef) -> d.CardScript:
    gjallarhorn = _units(FRIENDLY, d.IsRested(False), _traits("Gjallarhorn"))
    return _script(
        c,
        d.Triggered(
            d.Trigger(d.Ev.ATTACKS),
            (
                d.Choose("t1", gjallarhorn),
                d.Rest(d.Var("t1")),
                d.IfYouDo((d.Draw(), d.Apply(d.This(), d.KeywordGrant(d.Kw.HIGH_MANEUVER)))),
            ),
        ),
    )


# ---------------------------------------------------------------------------------------------
# white


@card("GD05-066")
def gd05_066(c: CardDef) -> d.CardScript:
    mf_units = _trash(UNIT, _traits("MF"))
    fetch = (
        d.Choose("t2", _trash(COMMAND, _traits("Special Move"))),
        d.AddToHand(d.Var("t2")),
    )
    return _script(
        c,
        d.Triggered(d.Trigger(d.Ev.DEPLOYED), _exile_exactly("t1", mf_units, 2, fetch)),
        *_line(c, 1),
    )


@card("GD05-068")
def gd05_068(c: CardDef) -> d.CardScript:
    special_move = d.Sel(FRIENDLY, d.Loc.HAND, (COMMAND, _traits("Special Move")))
    return _script(
        c,
        d.Triggered(
            d.Trigger(d.Ev.COMMAND_PLAYED, self_only=False, subject=special_move),
            (d.Apply(d.This(), d.KeywordGrant(d.Kw.SUPPRESSION)),),
        ),
        *_line(c, 1),
    )
