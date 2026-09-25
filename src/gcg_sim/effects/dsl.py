"""Typed effect DSL.

Every card ability is data built from the node types below: triggers, conditions, costs,
selectors, steps (actions and control flow), continuous effects, and durations. Nodes are frozen,
hashable dataclasses so compiled scripts can be serialized for golden tests and shared across
cloned game states. Card-specific logic that the DSL cannot express goes through the named
``Custom*`` hooks, whose implementations live in binding modules (see ``effects.custom``).

Player and side references are relative to the ability's controller.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from enum import StrEnum

from gcg_sim.engine.types import Duration


class P(StrEnum):
    """Player reference relative to the effect controller."""

    YOU = "you"
    OPP = "opponent"
    ACTIVE = "active"
    STANDBY = "standby"


class Side(StrEnum):
    FRIENDLY = "friendly"
    ENEMY = "enemy"
    ANY = "any"


class Loc(StrEnum):
    BATTLE = "battle"  # Units in the battle area
    PAIRED = "paired"  # Pilots paired in the battle area
    BASE = "base"  # base section
    SHIELDS = "shields"  # shield section (top first)
    SHIELD_AREA = "shield_area"  # base section then shields ("first card in the shield area")
    RESOURCE_AREA = "resource_area"
    HAND = "hand"
    TRASH = "trash"
    DECK = "deck"
    DECK_TOP = "deck_top"  # top ``top_n`` cards of the deck
    RESOURCE_DECK = "resource_deck"
    REMOVAL = "removal"
    FIELD_UNITS_AND_BASES = "units_and_bases"  # "Unit/Base" in play


class Op(StrEnum):
    LE = "<="
    GE = ">="
    EQ = "=="
    LT = "<"
    GT = ">"
    NE = "!="


class Kw(StrEnum):
    """Keyword effects (rule 13-1)."""

    REPAIR = "Repair"
    BREACH = "Breach"
    SUPPORT = "Support"
    BLOCKER = "Blocker"
    FIRST_STRIKE = "First Strike"
    HIGH_MANEUVER = "High-Maneuver"
    SUPPRESSION = "Suppression"


STACKING_KEYWORDS = frozenset({Kw.REPAIR, Kw.BREACH, Kw.SUPPORT})


class Stat(StrEnum):
    LV = "lv"
    COST = "cost"
    AP = "ap"
    HP = "hp"
    DAMAGE = "damage"
    REMAINING_HP = "remaining_hp"


class CardKind(StrEnum):
    UNIT = "unit"
    PILOT = "pilot"
    COMMAND = "command"
    BASE = "base"
    RESOURCE = "resource"
    TOKEN = "token"
    UNIT_TOKEN = "unit_token"
    EX_RESOURCE = "ex_resource"
    EX_BASE = "ex_base"
    LINK_UNIT = "link_unit"


# ---------------------------------------------------------------------------------------------
# Values


@dataclass(frozen=True, slots=True)
class Count:
    """Number of cards matching a selector at evaluation time."""

    sel: Sel


@dataclass(frozen=True, slots=True)
class StatOf:
    ref: Ref
    stat: Stat


@dataclass(frozen=True, slots=True)
class PlayerLevel:
    """Rule 2-9-4: a player's Lv is their number of Resources (including EX Resources)."""

    player: P = P.YOU


@dataclass(frozen=True, slots=True)
class HandSize:
    player: P = P.YOU


@dataclass(frozen=True, slots=True)
class ShieldCount:
    player: P = P.YOU


@dataclass(frozen=True, slots=True)
class DeckSize:
    player: P = P.YOU


@dataclass(frozen=True, slots=True)
class VarSize:
    """Number of cards currently bound to a frame variable."""

    var: str


@dataclass(frozen=True, slots=True)
class EventAmount:
    """Numeric payload of the triggering event (e.g. damage received)."""

    key: str = "amount"


@dataclass(frozen=True, slots=True)
class Sum:
    terms: tuple[Value, ...]


@dataclass(frozen=True, slots=True)
class Times:
    value: Value
    factor: int


@dataclass(frozen=True, slots=True)
class MinOf:
    terms: tuple[Value, ...]


@dataclass(frozen=True, slots=True)
class KwAmount:
    """Current merged amount of a stacking keyword on a card (rules 13-1-1-2, 13-1-2-5, 13-1-3-2)."""

    keyword: Kw
    ref: Ref = field(default_factory=lambda: This())  # noqa: PLW0108 (This is defined below)


@dataclass(frozen=True, slots=True)
class CustomValue:
    name: str
    params: tuple[tuple[str, object], ...] = ()


type Value = (
    int
    | Count
    | StatOf
    | PlayerLevel
    | HandSize
    | ShieldCount
    | DeckSize
    | VarSize
    | EventAmount
    | Sum
    | Times
    | MinOf
    | KwAmount
    | CustomValue
)

# ---------------------------------------------------------------------------------------------
# Card filters (all conjunctive within a selector)


@dataclass(frozen=True, slots=True)
class IsKind:
    """Card is any of the given kinds ("Unit", "Pilot", "Command", "Base", "token", ...)."""

    kinds: tuple[CardKind, ...]


@dataclass(frozen=True, slots=True)
class HasTrait:
    """Rule 5-19: "(A)/(B)" means either trait."""

    traits: tuple[str, ...]


@dataclass(frozen=True, slots=True)
class NameContains:
    """Rule 2-2-3: 'with "xyz" in its name'."""

    parts: tuple[str, ...]


@dataclass(frozen=True, slots=True)
class NameIs:
    """Rule 2-2-2: exact card name reference."""

    names: tuple[str, ...]


@dataclass(frozen=True, slots=True)
class HasColor:
    colors: tuple[str, ...]


@dataclass(frozen=True, slots=True)
class StatCmp:
    stat: Stat
    op: Op
    value: Value


@dataclass(frozen=True, slots=True)
class IsRested:
    rested: bool = True


@dataclass(frozen=True, slots=True)
class HasKeyword:
    keyword: Kw


@dataclass(frozen=True, slots=True)
class IsLinked:
    linked: bool = True


@dataclass(frozen=True, slots=True)
class IsPaired:
    paired: bool = True


@dataclass(frozen=True, slots=True)
class PairedWith:
    """Unit paired with a Pilot matching the filters (e.g. "paired with an (X-Rounder) Pilot")."""

    filters: tuple[Filter, ...]


@dataclass(frozen=True, slots=True)
class PairedTo:
    """Pilot paired with a Unit matching the filters."""

    filters: tuple[Filter, ...]


@dataclass(frozen=True, slots=True)
class IsToken:
    token: bool = True


@dataclass(frozen=True, slots=True)
class IsDamaged:
    damaged: bool = True


@dataclass(frozen=True, slots=True)
class NotRef:
    """Excludes the referenced card(s) ("other", "another")."""

    ref: Ref


@dataclass(frozen=True, slots=True)
class IsRef:
    ref: Ref


@dataclass(frozen=True, slots=True)
class IsAttacking:
    attacking: bool = True


@dataclass(frozen=True, slots=True)
class IsBattling:
    battling: bool = True


@dataclass(frozen=True, slots=True)
class HasTiming:
    """Card text has an effect with the given timing marker (e.g. "with a 【Destroyed】 effect")."""

    timing: str


@dataclass(frozen=True, slots=True)
class HasBurst:
    burst: bool = True


@dataclass(frozen=True, slots=True)
class DeployedThisTurn:
    value: bool = True


@dataclass(frozen=True, slots=True)
class HasZone:
    """Card's printed zone (Space/Earth) — rule 2-6."""

    zones: tuple[str, ...]


@dataclass(frozen=True, slots=True)
class LevelCmpRef:
    """Compare this card's stat with another referenced card's stat (e.g. "Lv. equal to or lower than this Unit")."""

    stat: Stat
    op: Op
    ref: Ref
    ref_stat: Stat


@dataclass(frozen=True, slots=True)
class AnyOf:
    filters: tuple[Filter, ...]


@dataclass(frozen=True, slots=True)
class AllOf:
    filters: tuple[Filter, ...]


@dataclass(frozen=True, slots=True)
class Not:
    filter: Filter


@dataclass(frozen=True, slots=True)
class CustomFilter:
    name: str
    params: tuple[tuple[str, object], ...] = ()


type Filter = (
    IsKind
    | HasTrait
    | NameContains
    | NameIs
    | HasColor
    | StatCmp
    | IsRested
    | HasKeyword
    | IsLinked
    | IsPaired
    | PairedWith
    | PairedTo
    | IsToken
    | IsDamaged
    | NotRef
    | IsRef
    | IsAttacking
    | IsBattling
    | HasTiming
    | HasBurst
    | DeployedThisTurn
    | HasZone
    | LevelCmpRef
    | AnyOf
    | AllOf
    | Not
    | CustomFilter
)


@dataclass(frozen=True, slots=True)
class Sel:
    """A set of cards: side (relative to controller) × location × filters.

    ``top_n`` applies to :attr:`Loc.DECK_TOP`. Units in the battle area and Bases are
    "in play"; the kinds filter distinguishes "Unit" from "Unit card" by location.
    """

    side: Side
    loc: Loc
    filters: tuple[Filter, ...] = ()
    top_n: Value = 0


# ---------------------------------------------------------------------------------------------
# Card references (resolved to ordered tuples of uids at execution time)


@dataclass(frozen=True, slots=True)
class This:
    """The card generating the effect (rule 10-3-4); for Pilot-granted text, the paired Unit."""


@dataclass(frozen=True, slots=True)
class ThisCard:
    """The physical card that owns the ability (the Pilot card itself for Pilot text)."""


@dataclass(frozen=True, slots=True)
class Var:
    name: str


@dataclass(frozen=True, slots=True)
class EventCard:
    """A card from the trigger payload: subject, attacker, target, blocker, destroyed, source, pilot, unit."""

    key: str = "subject"


@dataclass(frozen=True, slots=True)
class PairedPilotOf:
    ref: Ref


@dataclass(frozen=True, slots=True)
class PairedUnitOf:
    ref: Ref


@dataclass(frozen=True, slots=True)
class BattlingWith:
    """The card battling the referenced card (attack target or attacker)."""

    ref: Ref


@dataclass(frozen=True, slots=True)
class All:
    """Every card matching the selector (non-targeting)."""

    sel: Sel


@dataclass(frozen=True, slots=True)
class Union:
    refs: tuple[Ref, ...]


type Ref = (
    This | ThisCard | Var | EventCard | PairedPilotOf | PairedUnitOf | BattlingWith | All | Union
)

# ---------------------------------------------------------------------------------------------
# Conditions


@dataclass(frozen=True, slots=True)
class Cmp:
    left: Value
    op: Op
    right: Value


@dataclass(frozen=True, slots=True)
class Exists:
    sel: Sel
    at_least: int = 1


@dataclass(frozen=True, slots=True)
class IsTurn:
    player: P = P.YOU


@dataclass(frozen=True, slots=True)
class RefMatches:
    """Every card in ``ref`` (at least one) satisfies the filters."""

    ref: Ref
    filters: tuple[Filter, ...]


@dataclass(frozen=True, slots=True)
class RefEmpty:
    ref: Ref


@dataclass(frozen=True, slots=True)
class AttackTargetIs:
    """During a battle involving ``This``: "unit" or "player" (the shield area / player)."""

    kind: str


@dataclass(frozen=True, slots=True)
class EventFrom:
    """Triggering event's origin location (e.g. deployed from trash)."""

    loc: Loc


@dataclass(frozen=True, slots=True)
class EventFlag:
    """Boolean payload key on the triggering event (e.g. "by_enemy_effect", "battle")."""

    key: str
    value: bool = True


@dataclass(frozen=True, slots=True)
class HappenedThisTurn:
    """Turn history query: an event of ``kind`` involving ``player`` happened this turn
    (e.g. "during a turn where your opponent has discarded due to one of your effects")."""

    kind: str
    player: P = P.YOU
    by: P | None = None
    at_least: int = 1
    filters: tuple[Filter, ...] = ()


@dataclass(frozen=True, slots=True)
class DidLast:
    """Whether the most recent action step succeeded ("If you do", rule 5-20-1)."""


@dataclass(frozen=True, slots=True)
class And:
    conds: tuple[Cond, ...]


@dataclass(frozen=True, slots=True)
class Or:
    conds: tuple[Cond, ...]


@dataclass(frozen=True, slots=True)
class NotC:
    cond: Cond


@dataclass(frozen=True, slots=True)
class CustomCond:
    name: str
    params: tuple[tuple[str, object], ...] = ()


type Cond = (
    Cmp
    | Exists
    | IsTurn
    | RefMatches
    | RefEmpty
    | AttackTargetIs
    | EventFrom
    | EventFlag
    | HappenedThisTurn
    | DidLast
    | And
    | Or
    | NotC
    | CustomCond
)

TRUE: Cond = And(())

# ---------------------------------------------------------------------------------------------
# Rules modifications (restrictions, permissions, replacements) used by continuous effects


class RuleKind(StrEnum):
    CANT_ATTACK = "cant_attack"
    CANT_ATTACK_PLAYER = "cant_attack_player"
    CANT_ATTACK_UNITS = "cant_attack_units"
    CANT_BLOCK = "cant_block"  # can't activate <Blocker>
    CANT_BE_BLOCKED = "cant_be_blocked"
    MAY_ATTACK_ACTIVE = "may_attack_active"  # may choose active enemy Units matching `filters`
    ATTACK_ON_DEPLOY_TURN = "attack_on_deploy_turn"
    CANT_BE_SET_ACTIVE = "cant_be_set_active"
    CANT_BE_PAIRED = "cant_be_paired"
    CANT_RECEIVE_DAMAGE = "cant_receive_damage"  # params: damage kind, source filters
    REDUCE_DAMAGE = "reduce_damage"  # amount, damage kind, source filters, once per turn
    CANT_BE_DESTROYED = "cant_be_destroyed"  # by effects matching params
    CANT_BE_CHOSEN = "cant_be_chosen"  # by enemy effects
    CANT_BE_RETURNED = "cant_be_returned"  # to hand/deck by enemy effects
    NO_BLOCKER_VS = "no_blocker_vs"  # enemy Blockers can't block this Unit if they match filters
    MUST_ATTACK = "must_attack"
    PLAY_COST_DELTA = "play_cost_delta"  # card in hand: cost +/- amount
    PLAY_LEVEL_DELTA = "play_level_delta"
    ATTACK_TARGET_FIXED = "attack_target_fixed"
    CANT_BE_ATTACKED = "cant_be_attacked"  # enemy Units can't choose this as their attack target
    FORCE_ATTACK_TARGET = (
        "force_attack_target"  # enemy Units must choose this (rested) Unit if possible
    )
    SHIELD_AREA_PROTECTION = (
        "shield_area_protection"  # player-level: shield area can't receive damage
    )
    AP_CANT_BE_REDUCED = "ap_cant_be_reduced"
    REDIRECT_BATTLE_DAMAGE = (
        "redirect_battle_damage"  # battle damage is dealt to the aux card instead
    )
    REST_SUBSTITUTE = "rest_substitute"  # may rest this card instead (name: "unit"|"base"; source_filters: effect host)
    CUSTOM = "custom"


class DamageKind(StrEnum):
    ANY = "any"
    BATTLE = "battle"
    EFFECT = "effect"


@dataclass(frozen=True, slots=True)
class RuleMod:
    kind: RuleKind
    amount: Value = 0
    damage_kind: DamageKind = DamageKind.ANY
    source_filters: tuple[
        Filter, ...
    ] = ()  # filters the other card involved (attacker, target, source)
    source_side: Side = Side.ANY
    once_per_turn: bool = False
    name: str = ""  # CUSTOM rule name


# ---------------------------------------------------------------------------------------------
# Continuous effects (constant abilities and lasting effects created by steps)


@dataclass(frozen=True, slots=True)
class StatMod:
    ap: Value = 0
    hp: Value = 0


@dataclass(frozen=True, slots=True)
class KeywordGrant:
    keyword: Kw
    amount: Value = 0


@dataclass(frozen=True, slots=True)
class TraitGrant:
    traits: tuple[str, ...]


@dataclass(frozen=True, slots=True)
class RuleGrant:
    rule: RuleMod


@dataclass(frozen=True, slots=True)
class AbilityGrant:
    """Gains an ability defined elsewhere, e.g. "gains 【Attack】...". Refers to ``(card_number, ability index)``."""

    card_number: str
    ability_index: int


@dataclass(frozen=True, slots=True)
class CostMod:
    """Cost/Lv modification for cards (usually in hand)."""

    cost: Value = 0
    level: Value = 0


type Continuous = StatMod | KeywordGrant | TraitGrant | RuleGrant | AbilityGrant | CostMod

# ---------------------------------------------------------------------------------------------
# Triggers


class Ev(StrEnum):
    """Event kinds that triggered effects listen for."""

    DEPLOYED = "deployed"  # Unit/Base deployed (subject)
    ATTACKS = "attacks"  # attack declared (subject = attacker, target)
    BLOCKS = "blocks"  # <Blocker> activated (subject = blocker, attacker)
    BLOCKED = "blocked"  # the attacking Unit was blocked (subject = attacker, blocker)
    DESTROYED = "destroyed"  # subject destroyed (battle or effect)
    PAIRED = "paired"  # pilot paired (subject = unit, pilot)
    LINKED = "linked"  # pilot meeting link condition paired (subject = unit, pilot)
    DAMAGED = "damaged"  # subject received damage (amount, battle flag, source)
    DESTROYS_BY_BATTLE = "destroys_by_battle"  # subject destroyed an enemy Unit with damage
    DESTROYS_SHIELD_CARD = "destroys_shield_card"  # subject destroyed an enemy shield area card
    DEALS_DAMAGE = "deals_damage"  # subject dealt damage to target (amount, battle flag)
    COST_PAID = "cost_paid"  # resources paid for an effect of subject (amount)
    SUPPORT_USED = "support_used"  # subject used <Support> on target
    AP_REDUCED = "ap_reduced"  # subject's AP was reduced by an effect (by)
    SHIELD_DESTROYED = "shield_destroyed"  # a Shield was destroyed (subject shield, owner)
    TURN_START = "turn_start"
    TURN_END = "turn_end"
    EX_RESOURCE_PLACED = "ex_resource_placed"
    RESOURCE_PLACED = "resource_placed"
    DISCARDED = "discarded"
    RETURNED_TO_HAND = "returned_to_hand"
    COMMAND_PLAYED = "command_played"
    COMMAND_RESOLVED = "command_resolved"  # after a Command's effect ended (card now in trash)
    RESTED = "rested"
    SET_ACTIVE = "set_active"
    RECOVERED = "recovered"
    DRAWN = "drawn"
    ADDED_TO_HAND = "added_to_hand"
    SHIELD_TO_HAND = "shield_to_hand"
    BATTLE_END = "battle_end"
    LEFT_BATTLE_AREA = "left_battle_area"
    BURST_ACTIVATED = "burst_activated"
    EXILED = "exiled"


@dataclass(frozen=True, slots=True)
class Trigger:
    """``event`` whose subject is ``This`` (``self_only``) or matches ``subject``.

    ``whose_turn`` restricts to your/opponent's turn ("During your turn, when...").
    ``pilot_filters`` qualifies 【When Paired･(X) Pilot】.
    """

    event: Ev
    self_only: bool = True
    subject: Sel | None = None
    whose_turn: P | None = None
    pilot_filters: tuple[Filter, ...] = ()
    target_filters: tuple[Filter, ...] = ()
    battle_only: bool | None = None
    by_enemy: bool | None = None
    include_self: bool = True


# ---------------------------------------------------------------------------------------------
# Costs for activated abilities (rule 10-1-7)


@dataclass(frozen=True, slots=True)
class RestSelf:
    """ "Rest this Unit/Base" as a cost."""


@dataclass(frozen=True, slots=True)
class PayResources:
    """The ① symbol (rule 10-1-7-3): rest that many active Resources."""

    amount: int


@dataclass(frozen=True, slots=True)
class RestCards:
    sel: Sel
    count: int


@dataclass(frozen=True, slots=True)
class DestroyCards:
    sel: Sel
    count: int


@dataclass(frozen=True, slots=True)
class DiscardCards:
    sel: Sel
    count: int


@dataclass(frozen=True, slots=True)
class ExileCards:
    sel: Sel
    count: int


@dataclass(frozen=True, slots=True)
class ReturnSelf:
    """ "Return this card to your hand" as a cost."""


@dataclass(frozen=True, slots=True)
class DestroySelf:
    pass


@dataclass(frozen=True, slots=True)
class TrashSelf:
    pass


type Cost = (
    RestSelf
    | PayResources
    | RestCards
    | DestroyCards
    | DiscardCards
    | ExileCards
    | ReturnSelf
    | DestroySelf
    | TrashSelf
)

# ---------------------------------------------------------------------------------------------
# Steps (imperative instructions inside effect bodies)


@dataclass(frozen=True, slots=True)
class Choose:
    """Choose cards into ``var`` (rule 10-3-3: targets chosen when the instruction appears).

    ``count`` cards are chosen; ``min_count`` defaults to ``count`` ("choose 1") and may be
    lower for "up to"/"1 to 2". ``optional`` models "you may choose". ``chooser`` is the
    choosing player. When fewer legal cards exist than required, as many as possible are
    chosen (rule 1-3-2); with none, the variable is empty.
    """

    var: str
    sel: Sel
    count: Value = 1
    min_count: Value | None = None
    optional: bool = False
    chooser: P = P.YOU
    distinct_from: tuple[str, ...] = ()
    targeting: bool = True
    random: bool = False
    after_then: bool = False  # rule 10-1-8-1-2: choices after "Then" are not required to play


@dataclass(frozen=True, slots=True)
class ChooseMode:
    """Choose one of several labelled modes."""

    options: tuple[tuple[str, tuple[Step, ...]], ...]
    chooser: P = P.YOU


@dataclass(frozen=True, slots=True)
class Draw:
    """Draw ``count``. ``each_player`` draws for both players (active first) in one simultaneous
    step, so rules management sees both results together (rule 11-2-1)."""

    count: Value = 1
    player: P = P.YOU
    each_player: bool = False


@dataclass(frozen=True, slots=True)
class Discard:
    count: Value = 1
    player: P = P.YOU
    chooser: P | None = None
    random: bool = False
    filters: tuple[Filter, ...] = ()
    var: str = "discarded"


@dataclass(frozen=True, slots=True)
class Damage:
    """Effect damage (rule 5-5-4) from the effect's host to each card in ``ref``."""

    ref: Ref
    amount: Value


@dataclass(frozen=True, slots=True)
class DamageShieldArea:
    """Deal damage to the first card(s) of a player's shield area (Base first, else top Shield)."""

    player: P
    amount: Value
    cards: int = 1


@dataclass(frozen=True, slots=True)
class DamagePlayer:
    player: P
    amount: Value


@dataclass(frozen=True, slots=True)
class Destroy:
    ref: Ref


@dataclass(frozen=True, slots=True)
class Rest:
    ref: Ref


@dataclass(frozen=True, slots=True)
class SetActive:
    ref: Ref


@dataclass(frozen=True, slots=True)
class ReturnToHand:
    ref: Ref


@dataclass(frozen=True, slots=True)
class ToDeck:
    ref: Ref
    bottom: bool = True
    shuffle: bool = False


@dataclass(frozen=True, slots=True)
class Exile:
    """Remove from the game (rule 5-12; "exile ... from the game")."""

    ref: Ref


@dataclass(frozen=True, slots=True)
class ToTrash:
    """Place into the trash; for a Unit/Base on the field this is a destroy (rule 5-10-1)."""

    ref: Ref


@dataclass(frozen=True, slots=True)
class AddToHand:
    ref: Ref
    reveal: bool = False


@dataclass(frozen=True, slots=True)
class DeployCard:
    """Deploy Unit/Base cards from any location (rule 5-8). ``pay_cost`` false means free."""

    ref: Ref
    rested: bool = False
    pay_cost: bool = False
    ignore_level: bool = True


@dataclass(frozen=True, slots=True)
class DeployToken:
    """Deploy ``count`` Unit tokens (rule 5-17) identified by their inline definition key."""

    token_key: str
    count: Value = 1
    rested: bool = False
    player: P = P.YOU
    var: str = "tokens"


@dataclass(frozen=True, slots=True)
class PlaceExResource:
    player: P = P.YOU
    rested: bool = False


@dataclass(frozen=True, slots=True)
class PlaceResource:
    """Place the top Resource of the resource deck into the resource area."""

    player: P = P.YOU
    rested: bool = False


@dataclass(frozen=True, slots=True)
class SetResourcesActive:
    count: Value = 1
    player: P = P.YOU


@dataclass(frozen=True, slots=True)
class RestResources:
    count: Value = 1
    player: P = P.OPP


@dataclass(frozen=True, slots=True)
class ShieldToHand:
    """Add ``count`` of a player's Shields (top first, rule 4-6-4-1) to their hand."""

    count: Value = 1
    player: P = P.YOU


@dataclass(frozen=True, slots=True)
class AddToShields:
    """Place cards (default: top of deck) face down onto the shield section."""

    ref: Ref | None = None
    count: Value = 1
    player: P = P.YOU
    top: bool = True


@dataclass(frozen=True, slots=True)
class Mill:
    """Place the top ``count`` cards of a deck into the trash (bound to ``var``)."""

    count: Value
    player: P = P.YOU
    var: str = "milled"


@dataclass(frozen=True, slots=True)
class LookTop:
    """Look at the top ``count`` cards (bound to ``var``, known only to the looking player)."""

    count: Value
    player: P = P.YOU
    var: str = "looked"
    reveal: bool = False


@dataclass(frozen=True, slots=True)
class Arrange:
    """Return cards in ``ref`` to the deck: mode "top_or_bottom", "top_any_order",
    "bottom_any_order", or "top_or_bottom_each"."""

    ref: Ref
    mode: str
    player: P = P.YOU


@dataclass(frozen=True, slots=True)
class Reveal:
    ref: Ref


@dataclass(frozen=True, slots=True)
class Shuffle:
    player: P = P.YOU


@dataclass(frozen=True, slots=True)
class Recover:
    ref: Ref
    amount: Value


@dataclass(frozen=True, slots=True)
class Apply:
    """Create a lasting effect on the cards in ``ref`` for ``duration``.

    ``aux`` binds a related card to the effect (e.g. the destination of redirected damage).
    """

    ref: Ref
    effect: Continuous
    duration: Duration = Duration.THIS_TURN
    aux: Ref | None = None


@dataclass(frozen=True, slots=True)
class ApplyPlayer:
    """Create a lasting player-level rule (e.g. cost reduction for the next card played)."""

    player: P
    effect: Continuous
    duration: Duration = Duration.THIS_TURN
    filters: tuple[Filter, ...] = ()
    uses: int = 0


@dataclass(frozen=True, slots=True)
class Pair:
    """Pair a Pilot card (``pilot``) with a Unit (``unit``) (rule 5-9)."""

    pilot: Ref
    unit: Ref


@dataclass(frozen=True, slots=True)
class PlayCard:
    """Play cards as if from hand, optionally ignoring Lv/cost (rule 5-7-1-1)."""

    ref: Ref
    free: bool = False
    ignore_level: bool = False
    cost_delta: Value = 0


@dataclass(frozen=True, slots=True)
class ActivateMain:
    """Activate a Command card's 【Main】 effect (e.g. "【Burst】Activate this card's 【Main】")."""

    ref: Ref = field(default_factory=ThisCard)


@dataclass(frozen=True, slots=True)
class StartBattle:
    """Begin a battle between attacker and target, performing only the damage step
    (rules 5-22-3, 5-22-4, 13-1-5-4)."""

    attacker: Ref
    target: Ref
    damage_only: bool = True


@dataclass(frozen=True, slots=True)
class ChangeAttackTarget:
    target: Ref


@dataclass(frozen=True, slots=True)
class If:
    cond: Cond
    then: tuple[Step, ...]
    otherwise: tuple[Step, ...] = ()


@dataclass(frozen=True, slots=True)
class May:
    """ "You may ..." (rule 10-1-3): the controller decides whether to perform ``steps``."""

    steps: tuple[Step, ...]
    prompt: str = ""
    player: P = P.YOU


@dataclass(frozen=True, slots=True)
class IfYouDo:
    """Rule 5-20-1: resolve ``steps`` only if the preceding portion resolved."""

    steps: tuple[Step, ...]


@dataclass(frozen=True, slots=True)
class ForEach:
    """Run ``steps`` once per card in ``ref`` with the card bound to ``var``."""

    ref: Ref
    var: str
    steps: tuple[Step, ...]


@dataclass(frozen=True, slots=True)
class Repeat:
    times: Value
    steps: tuple[Step, ...]


@dataclass(frozen=True, slots=True)
class BindVar:
    """Bind ``var`` to the cards of ``ref`` evaluated now."""

    var: str
    ref: Ref


@dataclass(frozen=True, slots=True)
class DelayedTrigger:
    """Create a delayed trigger that stays armed for ``duration`` (e.g. "during this turn, when
    ...", "at the end of this turn, ...")."""

    trigger: Trigger
    steps: tuple[Step, ...]
    duration: Duration = Duration.THIS_TURN
    bind_vars: tuple[str, ...] = ()


@dataclass(frozen=True, slots=True)
class PayCost:
    """Rest ``amount`` active Resources as part of an effect ("You may pay ①"); did = paid."""

    amount: int
    player: P = P.YOU


@dataclass(frozen=True, slots=True)
class CustomStep:
    name: str
    params: tuple[tuple[str, object], ...] = ()


type Step = (
    Choose
    | ChooseMode
    | Draw
    | Discard
    | Damage
    | DamageShieldArea
    | DamagePlayer
    | Destroy
    | Rest
    | SetActive
    | ReturnToHand
    | ToDeck
    | Exile
    | ToTrash
    | AddToHand
    | DeployCard
    | DeployToken
    | PlaceExResource
    | PlaceResource
    | SetResourcesActive
    | RestResources
    | ShieldToHand
    | AddToShields
    | Mill
    | LookTop
    | Arrange
    | Reveal
    | Shuffle
    | Recover
    | Apply
    | ApplyPlayer
    | Pair
    | PlayCard
    | ActivateMain
    | StartBattle
    | ChangeAttackTarget
    | If
    | May
    | IfYouDo
    | ForEach
    | Repeat
    | BindVar
    | DelayedTrigger
    | PayCost
    | CustomStep
)

# ---------------------------------------------------------------------------------------------
# Abilities


class Timing(StrEnum):
    MAIN = "main"
    ACTION = "action"
    MAIN_OR_ACTION = "main_or_action"


class Where(StrEnum):
    """Where an ability functions (rule 2-11-2, 10-1-2)."""

    FIELD = "field"  # battle area / shield area (bases) — default for Units and Bases
    HAND = "hand"
    TRASH = "trash"
    ANY = "any"


class Gate(StrEnum):
    """【During Pair】/【During Link】 wrappers (rules 13-2-10, 13-2-12)."""

    NONE = "none"
    PAIRED = "paired"
    LINKED = "linked"


@dataclass(frozen=True, slots=True)
class Keyword:
    keyword: Kw
    amount: int = 0
    gate: Gate = Gate.NONE
    gate_filters: tuple[Filter, ...] = ()


@dataclass(frozen=True, slots=True)
class Constant:
    """Constant effect (rule 10-1-5) applying ``effects`` to ``scope`` while ``cond`` holds."""

    effects: tuple[Continuous, ...]
    scope: Ref = field(default_factory=This)
    cond: Cond = TRUE
    where: Where = Where.FIELD
    gate: Gate = Gate.NONE
    gate_filters: tuple[Filter, ...] = ()


@dataclass(frozen=True, slots=True)
class Triggered:
    """Triggered effect (rule 10-1-6)."""

    trigger: Trigger
    steps: tuple[Step, ...]
    cond: Cond = TRUE
    once_per_turn: bool = False
    where: Where = Where.FIELD
    gate: Gate = Gate.NONE
    gate_filters: tuple[Filter, ...] = ()


@dataclass(frozen=True, slots=True)
class Activated:
    """【Activate･Main】 / 【Activate･Action】 effect (rules 10-1-7, 13-2-1, 13-2-2)."""

    timing: Timing
    costs: tuple[Cost, ...]
    steps: tuple[Step, ...]
    cond: Cond = TRUE
    once_per_turn: bool = False
    where: Where = Where.FIELD
    gate: Gate = Gate.NONE
    gate_filters: tuple[Filter, ...] = ()
    support: int = 0  # <Support n> (rule 13-1-3) — amount contributed to the merged Support ability


@dataclass(frozen=True, slots=True)
class Command:
    """Command effect (rule 10-1-8): 【Main】, 【Action】, or both."""

    timing: Timing
    steps: tuple[Step, ...]
    cond: Cond = TRUE


@dataclass(frozen=True, slots=True)
class Burst:
    """【Burst】 effect (rule 13-2-5)."""

    steps: tuple[Step, ...]


@dataclass(frozen=True, slots=True)
class Replacement:
    """Substitution effect (rule 10-1-9): when ``event`` would happen to ``scope``, do ``steps`` instead."""

    event: Ev
    steps: tuple[Step, ...]
    scope: Ref = field(default_factory=This)
    cond: Cond = TRUE
    once_per_turn: bool = False
    optional: bool = True
    gate: Gate = Gate.NONE


@dataclass(frozen=True, slots=True)
class NameAlias:
    """ "This card's name is also treated as [X]" (rule 2-2-4)."""

    names: tuple[str, ...]


@dataclass(frozen=True, slots=True)
class PlayModifier:
    """Alternative way to play this card from the hand ("When playing this card from your
    hand, ... play this card as if it has N Lv. and cost").

    ``costs`` are additional costs paid when playing this way; ``level``/``cost`` replace the
    printed values when not ``None``; ``pair_filters`` restrict the Unit a Pilot is paired with.
    """

    cond: Cond = TRUE
    costs: tuple[Cost, ...] = ()
    level: int | None = None
    cost: int | None = None
    pair_filters: tuple[Filter, ...] = ()


type Ability = (
    Keyword
    | Constant
    | Triggered
    | Activated
    | Command
    | Burst
    | Replacement
    | NameAlias
    | PlayModifier
)


@dataclass(frozen=True, slots=True)
class CardScript:
    """Compiled behaviour of one card number.

    ``abilities`` are the card's own abilities. For Pilot cards, ``unit_abilities`` are the
    effects the paired Unit gains (rule 3-3-9-2); ``abilities`` holds the Pilot's own text
    (typically 【Burst】, rule 3-3-9-1).
    """

    card_number: str
    abilities: tuple[Ability, ...] = ()
    unit_abilities: tuple[Ability, ...] = ()
    source: str = "compiled"  # "compiled" | "binding" | "mixed" | "vanilla"
    notes: str = ""
