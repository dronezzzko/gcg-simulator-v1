"""Position evaluation: a logistic model over board features.

Every feature is a difference ``side(player) - side(opponent)``. The search only evaluates
determinized states, so the composition of the opponent's hand is a sample from the
searching player's information set, never the true hand. The expected number of 【Burst】
cards among each side's Shields is estimated from the known decklists. The model returns the
estimated probability that ``player`` wins. Weights are fitted by :mod:`gcg_sim.ai.tuning`.

Weights carry their feature set. ``base`` (:data:`FEATURES`) is antisymmetric in the two
players. ``roles`` (:data:`ROLE_FEATURES`) is computed from the turn player's point of view and
keeps the threat terms of the turn player and the other player apart; the other player's
log-odds are the negation, so the two estimates still add up to 1.
"""

from __future__ import annotations

import math
from collections.abc import Mapping
from dataclasses import dataclass
from types import MappingProxyType

from gcg_sim.ai.targeting import command_harms_enemy_units
from gcg_sim.cards.model import CardType
from gcg_sim.effects import dsl as d
from gcg_sim.engine import view as V
from gcg_sim.engine.observe import unknown_pools
from gcg_sim.engine.state import GameState
from gcg_sim.engine.types import Zone

FEATURES: tuple[str, ...] = (
    "tempo",
    "life",
    "danger",
    "can_kill",
    "ap",
    "hp",
    "linked",
    "ready",
    "hand_units",
    "hand_pilots",
    "hand_commands",
    "hand_bases",
    "level",
    "bursts",
)
N_FEATURES = len(FEATURES)
ROLE_FEATURES: tuple[str, ...] = (
    *(name for name in FEATURES if name != "can_kill"),
    "can_kill_turn_player",
    "can_kill_other",
    "inv_race_turn_player",
    "inv_race_other",
)
FEATURE_SETS: Mapping[str, tuple[str, ...]] = MappingProxyType(
    {"base": FEATURES, "roles": ROLE_FEATURES}
)
BASE_LIFE_HP = 3.0
DANGER_LIFE = 3.0
RACE_CAP = 6


@dataclass(frozen=True, slots=True)
class Weights:
    """One weight per feature of ``feature_set`` (see :data:`FEATURE_SETS`)."""

    values: tuple[float, ...]
    feature_set: str = "base"

    def __post_init__(self) -> None:
        names = FEATURE_SETS.get(self.feature_set)
        if names is None:
            raise ValueError(
                f"unknown feature set {self.feature_set!r}; choose from {sorted(FEATURE_SETS)}"
            )
        if len(self.values) != len(names):
            raise ValueError(f"expected {len(names)} weights, got {len(self.values)}")

    def as_dict(self) -> dict[str, float]:
        return dict(zip(FEATURE_SETS[self.feature_set], self.values, strict=True))


HAND_WEIGHTS = Weights(
    (0.0, 0.35, -0.30, 1.0, 0.10, 0.10, 0.20, 0.20, 0.30, 0.25, 0.25, 0.25, 0.15, 0.30)
)
# Fitted by `python -m gcg_sim.ai.tuning --games 540 --rounds 2 --seed 2026` (docs/AI.md).
TUNED_WEIGHTS = Weights(
    (
        -0.2017,
        0.4285,
        -0.4398,
        0.5341,
        0.1131,
        0.1574,
        0.2101,
        0.532,
        0.7157,
        0.4885,
        0.3727,
        0.5741,
        0.1643,
        0.5532,
    )
)
DEFAULT_WEIGHTS = TUNED_WEIGHTS
# Fitted by `python -m gcg_sim.ai.tuning --preset standard --start tuned --feature-set roles
# --games 540 --rounds 1 --seed 35001`: standard-preset self-play with TUNED_WEIGHTS.
ROLE_WEIGHTS = Weights(
    (
        -0.7451,
        0.206,
        -0.459,
        0.077,
        0.231,
        0.2152,
        0.3452,
        0.7646,
        0.5453,
        0.5425,
        0.5805,
        0.3053,
        0.8482,
        1.2665,
        -0.418,
        3.2487,
        -0.8534,
    ),
    "roles",
)


def burst_density(st: GameState, observer: int) -> tuple[float, float]:
    """Per owner: the fraction of 【Burst】 cards among main-deck cards ``observer`` cannot see."""
    R = V.reg()
    out: list[float] = []
    for owner in (0, 1):
        pool, _ = unknown_pools(st, observer, owner)
        bursts = sum(1 for def_id in pool if R.cards[def_id].has_burst)
        out.append(bursts / len(pool) if pool else 0.0)
    return out[0], out[1]


@dataclass(slots=True)
class _Side:
    shields: int
    has_base: bool
    life: float
    units: int
    ap: int
    hp: int
    linked: int
    blockers: int
    ready: int
    hand: tuple[int, int, int, int]
    level: int
    answers: int


_HAND_SLOT = {CardType.UNIT: 0, CardType.PILOT: 1, CardType.COMMAND: 2, CardType.BASE: 3}


def _side(st: GameState, dv: V.Derived, x: int) -> _Side:
    zones = st.zones[x]
    shields = len(zones[Zone.SHIELD])
    base_hp = sum(max(0, V.hp_of(st, dv, u) - st.cards[u].damage) for u in zones[Zone.BASE])
    ap = hp = linked = blockers = ready = 0
    for u in zones[Zone.BATTLE]:
        c = st.cards[u]
        ap += V.ap_of(st, dv, u)
        hp += max(0, V.hp_of(st, dv, u) - c.damage)
        if u in dv.linked:
            linked += 1
        if not c.rested:
            ready += 1
            if d.Kw.BLOCKER in dv.kw.get(u, ()):
                blockers += 1
    level = len(zones[Zone.RESOURCE_AREA])
    hand = [0, 0, 0, 0]
    answers = 0
    for u in zones[Zone.HAND]:
        cd = V.cdef(st, u)
        hand[_HAND_SLOT.get(cd.card_type, 3)] += 1
        if (
            cd.card_type is CardType.COMMAND
            and max(cd.level, cd.cost) <= level
            and command_harms_enemy_units(cd.def_id)
        ):
            answers += 1
    return _Side(
        shields=shields,
        has_base=bool(zones[Zone.BASE]),
        life=shields + base_hp / BASE_LIFE_HP,
        units=len(zones[Zone.BATTLE]),
        ap=ap,
        hp=hp,
        linked=linked,
        blockers=blockers,
        ready=ready,
        hand=(hand[0], hand[1], hand[2], hand[3]),
        level=level,
        answers=answers,
    )


def _can_kill(attacker: _Side, defender: _Side, attacker_active: bool) -> float:
    """1 when the attacker's next attack step has enough unblocked attackers to break every
    Shield and the Base and then deal battle damage to the player; castable removal in hand
    counts as answers to Blockers."""
    attackers = attacker.ready if attacker_active else attacker.units
    blockers = max(0, defender.blockers - attacker.answers)
    hits_needed = defender.shields + (1 if defender.has_base else 0) + 1
    return 1.0 if attackers - blockers >= hits_needed else 0.0


def _vector(s: _Side, o: _Side, active: bool, density: float) -> tuple[float, ...]:
    return (
        0.0,
        s.life,
        max(0.0, DANGER_LIFE - s.life),
        _can_kill(s, o, active),
        float(s.ap),
        float(s.hp),
        float(s.linked),
        float(s.ready),
        *(float(n) for n in s.hand),
        float(s.level),
        density * s.shields,
    )


def features(st: GameState, player: int, density: tuple[float, float]) -> list[float]:
    """Feature vector from ``player``'s point of view (antisymmetric in the two players)."""
    dv = V.derived(st)
    me = _side(st, dv, player)
    opp = _side(st, dv, 1 - player)
    mine_active = st.active == player
    a = _vector(me, opp, mine_active, density[player])
    b = _vector(opp, me, not mine_active, density[1 - player])
    out = [x - y for x, y in zip(a, b, strict=True)]
    out[0] = 1.0 if mine_active else -1.0
    return out


def _race(st: GameState, dv: V.Derived, attacker: int, defender: int) -> int:
    """Turns ``attacker`` needs to win if both boards stayed as they are: every Unit with AP
    that may attack the player does so each turn, each of the defender's Blockers stops one
    blockable attack (the largest), the Base absorbs hits at the connecting attacks' mean AP,
    then every Shield and one more hit; capped at :data:`RACE_CAP`."""
    blockers = sum(
        1
        for u in st.zones[defender][Zone.BATTLE]
        if d.Kw.BLOCKER in dv.kw.get(u, ()) and not V.rules_of(dv, u, d.RuleKind.CANT_BLOCK)
    )
    unblockable: list[int] = []
    blockable: list[int] = []
    for u in st.zones[attacker][Zone.BATTLE]:
        ap = V.ap_of(st, dv, u)
        if (
            ap <= 0
            or V.rules_of(dv, u, d.RuleKind.CANT_ATTACK)
            or V.rules_of(dv, u, d.RuleKind.CANT_ATTACK_PLAYER)
        ):
            continue
        if d.Kw.HIGH_MANEUVER in dv.kw.get(u, ()) or V.rules_of(dv, u, d.RuleKind.CANT_BE_BLOCKED):
            unblockable.append(ap)
        else:
            blockable.append(ap)
    blockable.sort(reverse=True)
    through = unblockable + blockable[blockers:]
    if not through:
        return RACE_CAP
    zones = st.zones[defender]
    base_hp = sum(max(0, V.hp_of(st, dv, u) - st.cards[u].damage) for u in zones[Zone.BASE])
    mean_ap = sum(through) / len(through)
    base_hits = math.ceil(base_hp / mean_ap) if zones[Zone.BASE] else 0
    hits = base_hits + len(zones[Zone.SHIELD]) + 1
    return min(RACE_CAP, math.ceil(hits / len(through)))


def role_features(st: GameState, density: tuple[float, float]) -> list[float]:
    """:data:`ROLE_FEATURES` from the turn player's point of view: the ``base`` differences
    without ``can_kill``, then ``can_kill`` of the turn player and of the other player, and
    1 / :func:`_race` of each."""
    dv = V.derived(st)
    turn_player = st.active
    other = 1 - turn_player
    me = _side(st, dv, turn_player)
    opp = _side(st, dv, other)
    a = _vector(me, opp, True, density[turn_player])
    b = _vector(opp, me, False, density[other])
    diff = [x - y for x, y in zip(a, b, strict=True)]
    return [
        1.0,
        *diff[1:3],
        *diff[4:],
        a[3],
        b[3],
        1.0 / _race(st, dv, turn_player, other),
        1.0 / _race(st, dv, other, turn_player),
    ]


def logistic(z: float) -> float:
    if z >= 0:
        return 1.0 / (1.0 + math.exp(-z))
    e = math.exp(z)
    return e / (1.0 + e)


def score(st: GameState, player: int, weights: Weights, density: tuple[float, float]) -> float:
    """The model's log-odds that ``player`` wins an unfinished game."""
    if weights.feature_set == "roles":
        r = role_features(st, density)
        z = sum(w * x for w, x in zip(weights.values, r, strict=True))
        return z if player == st.active else -z
    f = features(st, player, density)
    return sum(w * x for w, x in zip(weights.values, f, strict=True))


def evaluate(st: GameState, player: int, weights: Weights, density: tuple[float, float]) -> float:
    """Estimated probability that ``player`` wins (exact 1/0/0.5 for finished games)."""
    if st.winner is not None:
        if st.winner == player:
            return 1.0
        return 0.0 if st.winner == 1 - player else 0.5
    return logistic(score(st, player, weights, density))
