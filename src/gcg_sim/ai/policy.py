"""Fast heuristic playout policy.

Scores every legal option of any decision from the current board without look-ahead, the
way an experienced player's first instinct would: develop the biggest Units, pair Pilots that
link, attack when the fight is favourable, block when it saves a Shield or wins the fight,
aim removal at the most valuable enemy Unit, and discard or trash the least valuable card.
Search uses the scores as rollout policy (softmax sampling) and as move priors.
"""

from __future__ import annotations

import math
from collections.abc import Callable, Sequence
from dataclasses import dataclass

from gcg_sim.ai import targeting
from gcg_sim.ai.actions import ex_used
from gcg_sim.ai.values import (
    card_worth,
    exposed,
    fight,
    hp_left,
    life,
    pile_value,
    ready_blockers,
    unit_value,
)
from gcg_sim.cards.model import CardType
from gcg_sim.effects import dsl as d
from gcg_sim.engine import core
from gcg_sim.engine import view as V
from gcg_sim.engine.game import SUPPORT_AID
from gcg_sim.engine.state import Action, Decision, GameState
from gcg_sim.engine.types import PLAYER_TARGET, ActionKind, DecisionKind, Zone
from gcg_sim.rng import SplitMix64

A = ActionKind
K = DecisionKind

EX_PENALTY = 1.5
LETHAL = 60.0
CLEAR_THE_WAY = 8.0
MUST_BLOCK = 100.0
KEEPABLE_TOP = 2.0


def choose(st: GameState, rng: SplitMix64 | None, temperature: float) -> Action:
    """The policy's move: the best-scoring option, or a softmax sample when ``rng`` is given."""
    dec = st.pending
    assert dec is not None
    if len(dec.options) == 1:
        return dec.options[0]
    return pick(dec.options, option_scores(st, dec), rng, temperature)


def pick(
    options: Sequence[Action],
    scores: Sequence[float],
    rng: SplitMix64 | None,
    temperature: float,
) -> Action:
    if rng is None or temperature <= 0.0:
        return options[max(range(len(options)), key=scores.__getitem__)]
    best_i = 0
    best_v = -math.inf
    for i, s in enumerate(scores):
        u = rng.random() or 1e-300
        v = s / temperature - math.log(-math.log(u))
        if v > best_v:
            best_i, best_v = i, v
    return options[best_i]


def option_scores(st: GameState, dec: Decision) -> list[float]:
    scorer = _SCORERS.get(dec.kind)
    if scorer is None:
        return [0.0] * len(dec.options)
    return scorer(st, dec)


# ---------------------------------------------------------------------------------------------
# main phase


@dataclass(frozen=True, slots=True)
class _Board:
    player: int
    units: int
    enemy_blockers: tuple[int, ...]
    enemy_exposed: bool
    enemy_has_base: bool
    my_base_is_weak: bool
    enemy_max_ap: int
    attackers: int

    @property
    def blocked_from_lethal(self) -> bool:
        """The enemy is open to a finishing hit, but has a Blocker for every attacker."""
        return self.enemy_exposed and 0 < self.attackers <= len(self.enemy_blockers)


def _board(st: GameState, dv: V.Derived, dec: Decision) -> _Board:
    p = dec.player
    enemy = 1 - p
    my_base = st.zones[p][Zone.BASE]
    return _Board(
        player=p,
        units=len(st.zones[p][Zone.BATTLE]),
        enemy_blockers=tuple(ready_blockers(st, dv, enemy)),
        enemy_exposed=exposed(st, enemy),
        enemy_has_base=bool(st.zones[enemy][Zone.BASE]),
        my_base_is_weak=not my_base or V.cdef(st, my_base[0]).card_type is CardType.EX_BASE,
        enemy_max_ap=max((V.ap_of(st, dv, u) for u in st.zones[enemy][Zone.BATTLE]), default=0),
        attackers=len({o.a for o in dec.options if o.kind is A.ATTACK}),
    )


def _main(st: GameState, dec: Decision) -> list[float]:
    dv = V.derived(st)
    board = _board(st, dv, dec)
    return [_main_option(st, dv, board, o) for o in dec.options]


def _main_option(st: GameState, dv: V.Derived, b: _Board, o: Action) -> float:
    k = o.kind
    if k is A.END_MAIN:
        return 0.0
    if k is A.ATTACK:
        return _attack(st, dv, b, o.a, o.b)
    cost = EX_PENALTY * ex_used(o)
    if k is A.PLAY_UNIT:
        return _play_unit(st, b, o.a) - cost
    if k is A.PAIR:
        return _pair(st, dv, o.a, o.b) - cost
    if k is A.PLAY_BASE:
        return (4.0 if b.my_base_is_weak else -1.0) - cost
    if k is A.PLAY_COMMAND:
        s = 2.5 + 0.3 * V.cdef(st, o.a).cost - cost
        if b.blocked_from_lethal and targeting.command_harms_enemy_units(st.cards[o.a].def_id):
            s += CLEAR_THE_WAY
        return s
    if k is A.ACTIVATE:
        return (0.5 if o.b == SUPPORT_AID else 1.5) - cost
    return 0.0


def _play_unit(st: GameState, b: _Board, uid: int) -> float:
    cd = V.cdef(st, uid)
    s = 4.0 + 0.5 * cd.cost + 0.2 * (cd.ap + cd.hp)
    if b.units >= core.BATTLE_LIMIT:
        s -= 8.0
    return s


def _pair(st: GameState, dv: V.Derived, pilot: int, unit: int) -> float:
    pcd = V.cdef(st, pilot)
    ucd = V.cdef(st, unit)
    uc = st.cards[unit]
    s = 4.5 + 0.2 * (pcd.ap + pcd.hp) + 0.1 * unit_value(st, dv, unit)
    if V.link_satisfied(ucd, pcd):
        s += 3.0
        if uc.entered_turn == st.turn and not uc.rested:
            s += 1.0
    if uc.rested:
        s -= 1.5
    return s


def _unblockable(dv: V.Derived, attacker: int) -> bool:
    return d.Kw.HIGH_MANEUVER in dv.kw.get(attacker, ()) or bool(
        V.rules_of(dv, attacker, d.RuleKind.CANT_BE_BLOCKED)
    )


def _block_risk(st: GameState, dv: V.Derived, b: _Board, attacker: int, target: int) -> float:
    if _unblockable(dv, attacker):
        return 0.0
    worst = 0.0
    a_val = unit_value(st, dv, attacker)
    for bl in b.enemy_blockers:
        if bl == target:
            continue
        f = fight(st, dv, attacker, bl)
        if f.dies:
            gain = 0.5 * unit_value(st, dv, bl) if f.kills else 0.0
            worst = max(worst, 1.0 + 0.5 * a_val - gain)
    return worst


def _attack(st: GameState, dv: V.Derived, b: _Board, attacker: int, target: int) -> float:
    risk = _block_risk(st, dv, b, attacker, target)
    if target == PLAYER_TARGET:
        if b.enemy_exposed:
            if _unblockable(dv, attacker) or not b.enemy_blockers:
                return LETHAL
            return 2.0 if b.blocked_from_lethal else LETHAL / 2 - risk
        s = 2.5 - (0.3 if b.enemy_has_base else 0.0)
        if b.enemy_max_ap >= hp_left(st, dv, attacker):
            s -= 0.4
        return s - risk
    f = fight(st, dv, attacker, target)
    s = 3.0 + 0.5 * unit_value(st, dv, target) if f.kills else 0.3
    if f.dies:
        s -= 1.0 + 0.5 * unit_value(st, dv, attacker)
    breach = V.kw_amount(dv, attacker, d.Kw.BREACH)
    if f.kills and breach and not exposed(st, 1 - b.player):
        s += 0.5 * breach
    return s - risk


# ---------------------------------------------------------------------------------------------
# battle decisions


def _block(st: GameState, dec: Decision) -> list[float]:
    battle = st.battle
    assert battle is not None
    dv = V.derived(st)
    p = dec.player
    attacker = battle.attacker
    target = battle.target
    at_player = target == PLAYER_TARGET or st.cards[target].zone is Zone.BASE
    if at_player:
        if target == PLAYER_TARGET and exposed(st, p):
            saved = MUST_BLOCK
        else:
            saved = 1.2 + (2.0 if life(st, dv, p) <= 2 else 0.0)
    else:
        tf = fight(st, dv, attacker, target)
        saved = 1.0 + 0.5 * unit_value(st, dv, target) if tf.kills else 0.0
    scores = []
    for o in dec.options:
        if o.kind is not A.BLOCK:
            scores.append(0.0)
            continue
        f = fight(st, dv, attacker, o.a)
        s = saved
        if f.kills:
            s -= 1.0 + 0.5 * unit_value(st, dv, o.a)
        if f.dies:
            s += 1.0 + 0.5 * unit_value(st, dv, attacker)
        scores.append(s)
    return scores


def _action_step(st: GameState, dec: Decision) -> list[float]:
    in_battle = bool(st.battles)
    scores = []
    for o in dec.options:
        if o.kind is A.PASS:
            scores.append(0.0)
        elif o.kind is A.PLAY_COMMAND:
            scores.append((0.3 if in_battle else -1.0) - EX_PENALTY * ex_used(o))
        else:
            scores.append((0.2 if in_battle else -0.5) - EX_PENALTY * ex_used(o))
    return scores


# ---------------------------------------------------------------------------------------------
# choices inside effects


def _yes(st: GameState, dec: Decision) -> list[float]:
    return [1.0 if o.kind is A.YES else 0.0 for o in dec.options]


def _choose_first(st: GameState, dec: Decision) -> list[float]:
    return [1.0 if o.a == dec.player else 0.0 for o in dec.options]


def _keep(st: GameState, dec: Decision) -> list[float]:
    return [1.0 if o.kind is A.KEEP else 0.0 for o in dec.options]


def _excess(st: GameState, dec: Decision) -> list[float]:
    dv = V.derived(st)
    return [-pile_value(st, dv, o.a) for o in dec.options]


def _discard(st: GameState, dec: Decision) -> list[float]:
    out = []
    for o in dec.options:
        worth = card_worth(V.cdef(st, o.a))
        out.append(-worth if st.cards[o.a].owner == dec.player else worth)
    return out


def _arrange(st: GameState, dec: Decision) -> list[float]:
    if dec.ctx("order") == 1:
        return [_deck_worth(st, o.a, dec.player) for o in dec.options]
    card = dec.ctx("card")
    if card < 0:
        return [0.1 if o.a == 0 else 0.0 for o in dec.options]
    keep_on_top = _deck_worth(st, card, dec.player) >= KEEPABLE_TOP
    return [1.0 if (o.a == 0) == keep_on_top else 0.0 for o in dec.options]


def _deck_worth(st: GameState, uid: int, chooser: int) -> float:
    worth = card_worth(V.cdef(st, uid))
    return worth if st.cards[uid].owner == chooser else -worth


def _select(st: GameState, dec: Decision) -> list[float]:
    if dec.ctx("mode") == 1:
        return [0.0] * len(dec.options)
    dv = V.derived(st)
    protected = dec.ctx("rest_sub")
    if protected >= 0:
        keep = pile_value(st, dv, protected)
        return [
            0.0 if o.kind is A.DONE else keep - pile_value(st, dv, o.a) - 0.5 for o in dec.options
        ]
    polarity = targeting.choice_polarity(st)
    scores = []
    for o in dec.options:
        if o.kind is not A.SELECT or not 0 <= o.a < len(st.cards):
            scores.append(_done_score(st, dec, polarity))
            continue
        worth = pile_value(st, dv, o.a)
        mine = st.cards[o.a].owner == dec.player
        wants = polarity == targeting.HELPS if mine else polarity != targeting.HELPS
        if mine and polarity == targeting.UNKNOWN:
            wants = True
        scores.append(worth if wants else -worth)
    return scores


def _done_score(st: GameState, dec: Decision, polarity: int) -> float:
    cands = [o.a for o in dec.options if o.kind is A.SELECT and 0 <= o.a < len(st.cards)]
    mine = bool(cands) and st.cards[cands[0]].owner == dec.player
    unwanted = (mine and polarity == targeting.HARMS) or (not mine and polarity == targeting.HELPS)
    return 1.0 if unwanted else -0.25


_SCORERS: dict[DecisionKind, Callable[[GameState, Decision], list[float]]] = {
    K.MAIN: _main,
    K.BLOCK: _block,
    K.ACTION_STEP: _action_step,
    K.BURST: _yes,
    K.YES_NO: _yes,
    K.CHOOSE_FIRST: _choose_first,
    K.REDRAW: _keep,
    K.EXCESS: _excess,
    K.DISCARD: _discard,
    K.ARRANGE: _arrange,
    K.SELECT: _select,
}
