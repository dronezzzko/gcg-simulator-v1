"""Bindings for GD03-084..GD03-132 whose compiled script is missing or wrong."""

from __future__ import annotations

from typing import cast

from gcg_sim.cards.model import CardDef
from gcg_sim.cards.tokens import parse_token_specs
from gcg_sim.effects import dsl as d
from gcg_sim.effects.bindings import card, custom_filter, custom_value
from gcg_sim.effects.compiler import compile_parts
from gcg_sim.effects.compiler.abilities import CompileError
from gcg_sim.effects.registry import UnimplementedCardError
from gcg_sim.engine import core
from gcg_sim.engine import view as V
from gcg_sim.engine.state import GameState
from gcg_sim.engine.types import Duration, Zone

MODULE = "wp_gd03_b"
COND_HOLDS = f"{MODULE}_cond_holds"
BATTLING_ENEMY_UNIT = f"{MODULE}_battling_enemy_unit"
UNIQUE_NAMES = f"{MODULE}_unique_names"

UNIT = d.IsKind((d.CardKind.UNIT,))
ENEMY_UNITS = d.Sel(d.Side.ENEMY, d.Loc.BATTLE, (UNIT,))


def _hp_at_most(n: int) -> d.StatCmp:
    """ "with N or less HP" reads current HP, i.e. HP minus damage (FAQ Q96)."""
    return d.StatCmp(d.Stat.REMAINING_HP, d.Op.LE, n)


def _enemy_units_hp_at_most(n: int) -> d.Sel:
    return d.Sel(d.Side.ENEMY, d.Loc.BATTLE, (UNIT, _hp_at_most(n)))


def _line(c: CardDef, index: int) -> tuple[d.Ability, ...]:
    """The compiled abilities of one text line that the compiler already handles correctly."""
    _, res = compile_parts(c)[index]
    if isinstance(res, CompileError):
        raise UnimplementedCardError(f"{c.card_number} line {index}: {res}")
    return tuple(res)


def _token_key(c: CardDef, index: int) -> str:
    return parse_token_specs(c.effect)[index].key


def _cond_filter(cond: d.Cond) -> d.CustomFilter:
    return d.CustomFilter(COND_HOLDS, (("cond", cond),))


def _milled_has_trait(*traits: str) -> d.Cond:
    """ "If you placed a (X) card with this effect": one of the cards milled into ``milled``."""
    return d.Exists(
        d.Sel(d.Side.FRIENDLY, d.Loc.TRASH, (d.IsRef(d.Var("milled")), d.HasTrait(traits)))
    )


# ---------------------------------------------------------------------------------------------
# custom hooks


@custom_filter(COND_HOLDS)
def cond_holds(
    st: GameState, dv: V.Derived, ctx: V.Ctx, uid: int, params: dict[str, object]
) -> bool:
    """A card-independent condition used inside a target filter ("... instead" overrides)."""
    return V.cond(st, dv, ctx, cast(d.Cond, params["cond"]))


@custom_filter(BATTLING_ENEMY_UNIT)
def battling_enemy_unit(
    st: GameState, dv: V.Derived, ctx: V.Ctx, uid: int, params: dict[str, object]
) -> bool:
    """The Unit is in the current battle and the card it battles is an enemy Unit."""
    b = st.battle
    if b is None or b.ended:
        return False
    if uid == b.attacker:
        other = b.target
    elif uid == b.target:
        other = b.attacker
    else:
        return False
    if other < 0:
        return False
    oc = st.cards[other]
    return (
        oc.owner != st.cards[uid].owner
        and oc.zone is Zone.BATTLE
        and core.card_type(st, other).is_unit
    )


@custom_value(UNIQUE_NAMES)
def unique_names(st: GameState, dv: V.Derived, ctx: V.Ctx, params: dict[str, object]) -> int:
    """Number of distinct card names among the cards matching ``sel`` (ruling GD03-089:Q240)."""
    sel = params["sel"]
    assert isinstance(sel, d.Sel)
    return len({V.cdef(st, u).name for u in V.select(st, dv, ctx, sel)})


# ---------------------------------------------------------------------------------------------
# Pilots


@card("GD03-085")
def gd03_085(c: CardDef) -> d.CardScript:
    """Christina Mackenzie: pairing with a "Gundam NT-1" Unit costs 0; Lv. is unchanged (Q238)."""
    play_free = d.PlayModifier(cost=0, pair_filters=(UNIT, d.NameContains(("Gundam NT-1",))))
    return d.CardScript(c.card_number, abilities=(*_line(c, 0), play_free), source="binding")


@card("GD03-089")
def gd03_089(c: CardDef) -> d.CardScript:
    """Bernard Wiseman: AP + number of uniquely named (Cyclops Team) Pilot/Command cards in trash."""
    cyclops = d.Sel(
        d.Side.FRIENDLY,
        d.Loc.TRASH,
        (d.IsKind((d.CardKind.PILOT, d.CardKind.COMMAND)), d.HasTrait(("Cyclops Team",))),
    )
    boost = d.Constant((d.StatMod(ap=d.CustomValue(UNIQUE_NAMES, (("sel", cyclops),))),))
    return d.CardScript(
        c.card_number, abilities=_line(c, 0), unit_abilities=(boost,), source="binding"
    )


@card("GD03-092")
def gd03_092(c: CardDef) -> d.CardScript:
    """Nyaan: 【When Linked】 mill 1; if it was a (Zeon)/(Clan) card, 1 damage to an enemy Unit."""
    linked = d.Triggered(
        d.Trigger(d.Ev.LINKED),
        (
            d.Mill(1),
            d.If(
                _milled_has_trait("Zeon", "Clan"),
                (d.Choose("t1", ENEMY_UNITS), d.Damage(d.Var("t1"), 1)),
            ),
        ),
    )
    return d.CardScript(
        c.card_number, abilities=_line(c, 0), unit_abilities=(linked,), source="binding"
    )


@card("GD03-093")
def gd03_093(c: CardDef) -> d.CardScript:
    """Carris Nautilus: AP+1 while the opponent has no Base (the EX Base is a Base, 5-17-3-1-1)."""
    no_enemy_base = d.NotC(
        d.Exists(d.Sel(d.Side.ENEMY, d.Loc.BASE, (d.IsKind((d.CardKind.BASE,)),)))
    )
    boost = d.Constant((d.StatMod(ap=1),), cond=no_enemy_base)
    return d.CardScript(
        c.card_number, abilities=_line(c, 0), unit_abilities=(boost,), source="binding"
    )


@card("GD03-094")
def gd03_094(c: CardDef) -> d.CardScript:
    """Zeheart Galette: 【When Paired】 mill 2; if a (Vagan) card was placed, an enemy Unit AP-2."""
    paired = d.Triggered(
        d.Trigger(d.Ev.PAIRED),
        (
            d.Mill(2),
            d.If(
                _milled_has_trait("Vagan"),
                (d.Choose("t1", ENEMY_UNITS), d.Apply(d.Var("t1"), d.StatMod(ap=-2))),
            ),
        ),
    )
    return d.CardScript(
        c.card_number, abilities=_line(c, 0), unit_abilities=(paired,), source="binding"
    )


@card("GD03-097")
def gd03_097(c: CardDef) -> d.CardScript:
    """Wistario Afam: look at the top 2, keep 1 on top, trash the other (fires even if this
    Unit is destroyed in the same battle, Q241)."""
    looked = d.IsRef(d.Var("looked"))
    scry = d.Triggered(
        d.Trigger(d.Ev.DESTROYS_BY_BATTLE, battle_only=True, whose_turn=d.P.YOU),
        (
            d.LookTop(2),
            d.Choose("top", d.Sel(d.Side.FRIENDLY, d.Loc.DECK, (looked,)), targeting=False),
            d.Choose(
                "rest",
                d.Sel(d.Side.FRIENDLY, d.Loc.DECK, (looked, d.NotRef(d.Var("top")))),
                count=2,
                targeting=False,
            ),
            d.ToTrash(d.Var("rest")),
        ),
        once_per_turn=True,
        gate=d.Gate.LINKED,
    )
    return d.CardScript(
        c.card_number, abilities=_line(c, 0), unit_abilities=(scry,), source="binding"
    )


@card("GD03-098")
def gd03_098(c: CardDef) -> d.CardScript:
    """Graham Aker: set active by an effect (never by the start phase, Q242) returns an enemy Unit
    with 3 or less current HP."""
    on_set_active = d.Triggered(
        d.Trigger(d.Ev.SET_ACTIVE),
        (d.Choose("t1", _enemy_units_hp_at_most(3)), d.ReturnToHand(d.Var("t1"))),
        gate=d.Gate.LINKED,
    )
    return d.CardScript(
        c.card_number, abilities=_line(c, 0), unit_abilities=(on_set_active,), source="binding"
    )


# ---------------------------------------------------------------------------------------------
# Commands


@card("GD03-101")
def gd03_101(c: CardDef) -> d.CardScript:
    """A Healthy Curiosity: draw 1; then, with 2 or more copies already in the trash (Q244), rest
    an enemy Unit with 4 or less current HP."""
    copies = d.Count(
        d.Sel(d.Side.FRIENDLY, d.Loc.TRASH, (d.NameContains(("A Healthy Curiosity",)),))
    )
    cmd = d.Command(
        d.Timing.MAIN,
        (
            d.Draw(),
            d.If(
                d.Cmp(copies, d.Op.GE, 2),
                (
                    d.Choose("t1", _enemy_units_hp_at_most(4), after_then=True),
                    d.Rest(d.Var("t1")),
                ),
            ),
        ),
    )
    return d.CardScript(c.card_number, abilities=(cmd,), source="binding")


@card("GD03-102")
def gd03_102(c: CardDef) -> d.CardScript:
    """Privileged Position: set active 1 of your (Titans) Link Units battling an enemy Unit."""
    target = d.Sel(
        d.Side.FRIENDLY,
        d.Loc.BATTLE,
        (
            d.IsKind((d.CardKind.LINK_UNIT,)),
            d.HasTrait(("Titans",)),
            d.CustomFilter(BATTLING_ENEMY_UNIT),
        ),
    )
    action = d.Command(d.Timing.ACTION, (d.Choose("t1", target), d.SetActive(d.Var("t1"))))
    return d.CardScript(c.card_number, abilities=(*_line(c, 0), action), source="binding")


@card("GD03-103")
def gd03_103(c: CardDef) -> d.CardScript:
    """Field Directive: unplayable unless 3 or more enemy Units are in play (ruling Q245); the
    【Burst】 HP filter reads current HP."""
    main = d.Command(
        d.Timing.MAIN,
        (
            d.Choose("t1", d.Sel(d.Side.ENEMY, d.Loc.BATTLE, (UNIT, d.IsRested()))),
            d.Damage(d.Var("t1"), 2),
        ),
        cond=d.Cmp(d.Count(ENEMY_UNITS), d.Op.GE, 3),
    )
    burst = d.Burst((d.Choose("t1", _enemy_units_hp_at_most(2)), d.Rest(d.Var("t1"))))
    return d.CardScript(c.card_number, abilities=(burst, main), source="binding")


@card("GD03-104")
def gd03_104(c: CardDef) -> d.CardScript:
    """Reccoa's Shadow: rest 1 enemy Unit with 3 or less HP, or 1 to 2 of them while a friendly
    (Jupitris) Link Unit is in play."""
    jupitris_link = d.Sel(
        d.Side.FRIENDLY,
        d.Loc.BATTLE,
        (d.IsKind((d.CardKind.LINK_UNIT,)), d.HasTrait(("Jupitris",))),
    )
    up_to = d.Sum((1, d.MinOf((d.Count(jupitris_link), 1))))
    cmd = d.Command(
        d.Timing.MAIN_OR_ACTION,
        (
            d.Choose(
                "t1",
                _enemy_units_hp_at_most(3),
                count=up_to,
                min_count=1,
            ),
            d.Rest(d.Var("t1")),
        ),
    )
    return d.CardScript(c.card_number, abilities=(cmd, *_line(c, 1)), source="binding")


@card("GD03-106")
def gd03_106(c: CardDef) -> d.CardScript:
    """M.A.V. Tactics: deploy two rested (Clan) Unit tokens."""
    cmd = d.Command(
        d.Timing.MAIN,
        (
            d.DeployToken(_token_key(c, 0), 1, rested=True),
            d.DeployToken(_token_key(c, 1), 1, rested=True, var="tokens2"),
        ),
    )
    return d.CardScript(c.card_number, abilities=(cmd,), source="binding")


@card("GD03-108")
def gd03_108(c: CardDef) -> d.CardScript:
    """How Many Miles to the Battlefield?: deploy 1 [Hy-Gogg] Unit token."""
    cmd = d.Command(d.Timing.MAIN, (d.DeployToken(_token_key(c, 0), 1),))
    return d.CardScript(c.card_number, abilities=(cmd, *_line(c, 1)), source="binding")


@card("GD03-109")
def gd03_109(c: CardDef) -> d.CardScript:
    """Improved Technique: 3 damage to an enemy Unit that is Lv.4 or lower, or to any enemy Unit
    with 2 or more "Improved Technique" cards already in the trash (Q247, Q248)."""
    in_trash = d.Count(
        d.Sel(d.Side.FRIENDLY, d.Loc.TRASH, (d.NameContains(("Improved Technique",)),))
    )
    level_ok = d.AnyOf(
        (d.StatCmp(d.Stat.LV, d.Op.LE, 4), _cond_filter(d.Cmp(in_trash, d.Op.GE, 2)))
    )
    cmd = d.Command(
        d.Timing.MAIN_OR_ACTION,
        (
            d.Choose("t1", d.Sel(d.Side.ENEMY, d.Loc.BATTLE, (UNIT, level_ok))),
            d.Damage(d.Var("t1"), 3),
        ),
    )
    return d.CardScript(c.card_number, abilities=(*_line(c, 0), cmd), source="binding")


@card("GD03-110")
def gd03_110(c: CardDef) -> d.CardScript:
    """Eliminate Target: destroy a Pilot paired with an enemy Unit that is Lv.5 or lower (the Lv.
    is the Unit's, Q426)."""
    pilot = d.Sel(
        d.Side.ENEMY,
        d.Loc.PAIRED,
        (
            d.IsKind((d.CardKind.PILOT,)),
            d.PairedTo((UNIT, d.StatCmp(d.Stat.LV, d.Op.LE, 5))),
        ),
    )
    cmd = d.Command(d.Timing.MAIN_OR_ACTION, (d.Choose("t1", pilot), d.Destroy(d.Var("t1"))))
    return d.CardScript(c.card_number, abilities=(cmd,), source="binding")


@card("GD03-113")
def gd03_113(c: CardDef) -> d.CardScript:
    """Human Karma: rest an active friendly Unit; if you do, 3 damage to an enemy Unit whose Lv.
    is at most the rested Unit's (Q250)."""
    cmd = d.Command(
        d.Timing.MAIN_OR_ACTION,
        (
            d.Choose("t1", d.Sel(d.Side.FRIENDLY, d.Loc.BATTLE, (UNIT, d.IsRested(False)))),
            d.Rest(d.Var("t1")),
            d.IfYouDo(
                (
                    d.Choose(
                        "t2",
                        d.Sel(
                            d.Side.ENEMY,
                            d.Loc.BATTLE,
                            (UNIT, d.LevelCmpRef(d.Stat.LV, d.Op.LE, d.Var("t1"), d.Stat.LV)),
                        ),
                    ),
                    d.Damage(d.Var("t2"), 3),
                )
            ),
        ),
    )
    return d.CardScript(c.card_number, abilities=(cmd,), source="binding")


@card("GD03-114")
def gd03_114(c: CardDef) -> d.CardScript:
    """Look of Determination: destroy an active enemy Unit that is Lv.2 or lower, or Lv.4 or lower
    with 10 or more cards already in your trash (Q251)."""
    big_trash = d.Cmp(d.Count(d.Sel(d.Side.FRIENDLY, d.Loc.TRASH)), d.Op.GE, 10)
    level_ok = d.AnyOf(
        (
            d.StatCmp(d.Stat.LV, d.Op.LE, 2),
            d.AllOf((d.StatCmp(d.Stat.LV, d.Op.LE, 4), _cond_filter(big_trash))),
        )
    )
    cmd = d.Command(
        d.Timing.ACTION,
        (
            d.Choose("t1", d.Sel(d.Side.ENEMY, d.Loc.BATTLE, (UNIT, d.IsRested(False), level_ok))),
            d.Destroy(d.Var("t1")),
        ),
    )
    return d.CardScript(c.card_number, abilities=(*_line(c, 0), cmd), source="binding")


def _no_battle_damage_from(max_ap: int) -> d.Apply:
    rule = d.RuleMod(
        d.RuleKind.CANT_RECEIVE_DAMAGE,
        damage_kind=d.DamageKind.BATTLE,
        source_filters=(UNIT, d.StatCmp(d.Stat.AP, d.Op.LE, max_ap)),
        source_side=d.Side.ENEMY,
    )
    return d.Apply(d.Var("t1"), d.RuleGrant(rule), Duration.THIS_BATTLE)


@card("GD03-115")
def gd03_115(c: CardDef) -> d.CardScript:
    """Distant Reunion: battle-damage immunity during this battle in both branches."""
    target = d.Sel(
        d.Side.FRIENDLY, d.Loc.BATTLE, (UNIT, d.PairedWith((d.HasTrait(("X-Rounder",)),)))
    )
    cmd = d.Command(
        d.Timing.ACTION,
        (
            d.Choose("t1", target),
            d.If(
                d.Cmp(d.PlayerLevel(), d.Op.GE, 7),
                (_no_battle_damage_from(5),),
                (_no_battle_damage_from(2),),
            ),
        ),
    )
    return d.CardScript(c.card_number, abilities=(cmd, *_line(c, 1)), source="binding")


@card("GD03-117")
def gd03_117(c: CardDef) -> d.CardScript:
    """Orga's Order: a token sized by the number of enemy Units; unplayable with none (Q245 policy)."""
    n = d.Count(ENEMY_UNITS)
    cmd = d.Command(
        d.Timing.MAIN,
        (
            d.If(
                d.And((d.Cmp(n, d.Op.GE, 1), d.Cmp(n, d.Op.LE, 4))),
                (d.DeployToken(_token_key(c, 0), 1),),
            ),
            d.If(d.Cmp(n, d.Op.GE, 5), (d.DeployToken(_token_key(c, 1), 1, var="tokens2"),)),
        ),
        cond=d.Cmp(n, d.Op.GE, 1),
    )
    return d.CardScript(c.card_number, abilities=(cmd,), source="binding")


@card("GD03-120")
def gd03_120(c: CardDef) -> d.CardScript:
    """Immortal Colasour: for the rest of the turn, each battle kill by a friendly
    (Superpower Bloc)/(UN) Unit sets a rested one active; that Unit can't attack this turn."""
    sb_un = (UNIT, d.HasTrait(("Superpower Bloc", "UN")))
    delayed = d.DelayedTrigger(
        d.Trigger(
            d.Ev.DESTROYS_BY_BATTLE,
            battle_only=True,
            self_only=False,
            subject=d.Sel(d.Side.FRIENDLY, d.Loc.BATTLE, sb_un),
        ),
        (
            d.Choose("t1", d.Sel(d.Side.FRIENDLY, d.Loc.BATTLE, (*sb_un, d.IsRested()))),
            d.SetActive(d.Var("t1")),
            d.Apply(d.Var("t1"), d.RuleGrant(d.RuleMod(d.RuleKind.CANT_ATTACK))),
        ),
    )
    cmd = d.Command(d.Timing.MAIN, (delayed,))
    return d.CardScript(c.card_number, abilities=(cmd, *_line(c, 1)), source="binding")


@card("GD03-121")
def gd03_121(c: CardDef) -> d.CardScript:
    """Unheralded Attack: rest a friendly Base and an enemy Unit with 3 or less current HP; both
    targets are required (Q253)."""
    cmd = d.Command(
        d.Timing.ACTION,
        (
            d.Choose("t1", d.Sel(d.Side.FRIENDLY, d.Loc.BASE, (d.IsKind((d.CardKind.BASE,)),))),
            d.Choose("t2", _enemy_units_hp_at_most(3)),
            d.Rest(d.Union((d.Var("t1"), d.Var("t2")))),
        ),
    )
    return d.CardScript(c.card_number, abilities=(cmd, *_line(c, 1)), source="binding")


# ---------------------------------------------------------------------------------------------
# Bases


@card("GD03-124")
def gd03_124(c: CardDef) -> d.CardScript:
    """Ribo Colony: when you pair a Lv.3 or lower Pilot with one of your Units, rest an enemy
    Unit with 3 or less HP (once per turn)."""
    on_pair = d.Triggered(
        d.Trigger(
            d.Ev.PAIRED,
            self_only=False,
            subject=d.Sel(d.Side.FRIENDLY, d.Loc.BATTLE, (UNIT,)),
            pilot_filters=(d.StatCmp(d.Stat.LV, d.Op.LE, 3),),
            by_enemy=False,
        ),
        (
            d.Choose("t1", _enemy_units_hp_at_most(3)),
            d.Rest(d.Var("t1")),
        ),
        once_per_turn=True,
    )
    return d.CardScript(
        c.card_number, abilities=(*_line(c, 0), *_line(c, 1), on_pair), source="binding"
    )


@card("GD03-125")
def gd03_125(c: CardDef) -> d.CardScript:
    """Peacemillion: the destroying Lv.6+ (Operation Meteor)/(G Team) Unit may recover 2 HP; a
    Unit destroyed in the same battle cannot recover (Q254)."""
    killer = d.Sel(
        d.Side.FRIENDLY,
        d.Loc.BATTLE,
        (
            UNIT,
            d.HasTrait(("Operation Meteor", "G Team")),
            d.StatCmp(d.Stat.LV, d.Op.GE, 6),
        ),
    )
    on_kill = d.Triggered(
        d.Trigger(
            d.Ev.DESTROYS_BY_BATTLE,
            battle_only=True,
            self_only=False,
            subject=killer,
            whose_turn=d.P.YOU,
        ),
        (d.May((d.Recover(d.EventCard("subject"), 2),)),),
        once_per_turn=True,
    )
    return d.CardScript(
        c.card_number, abilities=(*_line(c, 0), *_line(c, 1), on_kill), source="binding"
    )


@card("GD03-128")
def gd03_128(c: CardDef) -> d.CardScript:
    """Doritea: during the opponent's turn, when their effect rests one of your Units, 1 damage
    to an enemy Unit (once per turn)."""
    on_rest = d.Triggered(
        d.Trigger(
            d.Ev.RESTED,
            self_only=False,
            subject=d.Sel(d.Side.FRIENDLY, d.Loc.BATTLE, (UNIT,)),
            whose_turn=d.P.OPP,
            by_enemy=True,
        ),
        (d.Choose("t1", ENEMY_UNITS), d.Damage(d.Var("t1"), 1)),
        once_per_turn=True,
    )
    return d.CardScript(
        c.card_number, abilities=(*_line(c, 0), *_line(c, 1), on_rest), source="binding"
    )


@card("GD03-129")
def gd03_129(c: CardDef) -> d.CardScript:
    """Hotarubi: during your turn, when a friendly (Tekkadan)/(Teiwaz) Unit receives effect
    damage, you may rest this active Base to mill 1."""
    on_damage = d.Triggered(
        d.Trigger(
            d.Ev.DAMAGED,
            self_only=False,
            subject=d.Sel(
                d.Side.FRIENDLY, d.Loc.BATTLE, (UNIT, d.HasTrait(("Tekkadan", "Teiwaz")))
            ),
            whose_turn=d.P.YOU,
            battle_only=False,
        ),
        (
            d.If(
                d.RefMatches(d.This(), (d.IsRested(False),)),
                (d.May((d.Rest(d.This()),)), d.IfYouDo((d.Mill(1),))),
            ),
        ),
    )
    return d.CardScript(
        c.card_number, abilities=(*_line(c, 0), *_line(c, 1), on_damage), source="binding"
    )


@card("GD03-132")
def gd03_132(c: CardDef) -> d.CardScript:
    """Radish: 【Destroyed】 with an (AEUG) Link Unit in play, rest an enemy Unit with 4 or less
    current HP."""
    aeug_link = d.Sel(
        d.Side.FRIENDLY, d.Loc.BATTLE, (d.IsKind((d.CardKind.LINK_UNIT,)), d.HasTrait(("AEUG",)))
    )
    destroyed = d.Triggered(
        d.Trigger(d.Ev.DESTROYED),
        (
            d.If(
                d.Exists(aeug_link),
                (d.Choose("t1", _enemy_units_hp_at_most(4)), d.Rest(d.Var("t1"))),
            ),
        ),
    )
    return d.CardScript(
        c.card_number, abilities=(*_line(c, 0), *_line(c, 1), destroyed), source="binding"
    )
