"""Information-set MCTS with determinizations (single-observer ISMCTS).

One tree is built from the deciding player's point of view. Every iteration picks a root
move by UCB1 plus a progressive prior from the playout policy, then plays it in a
determinization of the player's information set (hidden cards redrawn from the known
decklists, the game RNG replaced). The n-th visit of every root move uses the same n-th
determinization and rollout seed (common random numbers), so root moves are compared on
identical worlds. Below the root the walk chooses among the moves legal in that world
(availability counts keep the bandit fair when a move exists only in some worlds), adds one
node, plays the heuristic policy until ``horizon_turns`` turn boundaries have passed, and
backs up the evaluation. Only the true state's pending options and the player's own
knowledge are read directly.

Settings that are off in the presets: ``target_extension`` lets an effect target decision search
on, one budget at a time, while the move to play is within ``target_extension_z`` paired standard
errors of another; ``observation_keys`` keys children by decision kind and keeps the walk in the
same node after an opponent's action-step pass, which the searcher cannot tell apart from the
engine passing for an opponent without options.
"""

from __future__ import annotations

import math
from dataclasses import dataclass, field, replace
from functools import cache

from gcg_sim.ai import policy
from gcg_sim.ai.actions import Key, canonical, merge_equivalent, prune_dominated
from gcg_sim.ai.base import Alternative, uid_argument
from gcg_sim.ai.config import LEAF_FLOOR, WIN_DECAY, SearchConfig
from gcg_sim.ai.evaluation import (
    Weights,
    burst_density,
    can_kill_to_move,
    logistic,
    score,
    threat_to_move,
)
from gcg_sim.ai.noop import SPENDING, without_effect
from gcg_sim.ai.playout import play_until_turn
from gcg_sim.ai.policy import PolicyKnobs, knobs_for
from gcg_sim.engine.game import apply
from gcg_sim.engine.observe import determinize
from gcg_sim.engine.state import Action, Decision, GameState
from gcg_sim.engine.types import ActionKind, DecisionKind
from gcg_sim.rng import SplitMix64

ChildKey = tuple[int, str, Key]
TIE_TOLERANCE = 1e-9
_TARGET_KINDS = frozenset(
    {DecisionKind.SELECT, DecisionKind.EXCESS, DecisionKind.DISCARD, DecisionKind.ARRANGE}
)


@dataclass(slots=True)
class _Child:
    mover: int
    prior: float
    visits: int = 0
    total: float = 0.0
    avail: int = 0
    node: _Node | None = None

    def mean(self) -> float:
        return self.total / self.visits if self.visits else 0.0


@dataclass(slots=True)
class _Node:
    children: dict[ChildKey, _Child] = field(default_factory=dict)


@dataclass(frozen=True, slots=True)
class _Search:
    player: int
    config: SearchConfig
    weights: Weights
    density: tuple[float, float]
    root_turn: int
    horizon: int


@dataclass(frozen=True, slots=True)
class SearchResult:
    action: Action
    alternatives: list[Alternative]
    iterations: int


def search(
    st: GameState, player: int, config: SearchConfig, weights: Weights, rng: SplitMix64
) -> SearchResult:
    """Choose a move for ``player`` at ``st.pending`` using only their information set."""
    dec = st.pending
    assert dec is not None
    assert dec.player == player
    root_moves = prune_dominated(
        st,
        dec,
        canonical(st, dec),
        strict_attacks=config.strict_attack_prune,
        rest_targets=config.prune_noop_rest_targets,
    )
    if any(a.kind in SPENDING for _, a in root_moves):
        idle = without_effect(
            st,
            player,
            dec,
            root_moves,
            rng.next_u64(),
            knobs_for(config),
            branch_choices=config.noop_branch_choices,
            resource_aware=config.noop_resource_aware,
            weights=weights,
        )
        root_moves = [m for m in root_moves if m[0] not in idle] or root_moves
    if config.merge_equivalent_moves:
        root_moves = merge_equivalent(st, dec, root_moves)
    if len(root_moves) == 1:
        return SearchResult(root_moves[0][1], [], 0)
    ctx = _Search(
        player,
        config,
        weights,
        burst_density(st, player),
        st.turn,
        st.turn + config.horizon_turns,
    )
    budget = config.budget(len(root_moves))
    world_seeds = [rng.next_u64() for _ in range(budget)]
    rollout_seeds = [rng.next_u64() for _ in range(budget)]
    cap = _iteration_cap(config, dec, budget)
    world_seeds += [rng.next_u64() for _ in range(cap - budget)]
    rollout_seeds += [rng.next_u64() for _ in range(cap - budget)]
    root = _Node()
    first_world = determinize(st, player, world_seeds[0])
    _add_children(root, first_world, dec, root_moves, config)
    keys = [_child_key(dec, key, config) for key, _ in root_moves]
    children = [root.children[key] for key in keys]
    slot = {id(child): i for i, child in enumerate(children)}
    per_visit: list[list[float]] = [[] for _ in children]
    limit = budget
    done = 0
    while done < limit:
        child, action, fresh = _select(root, first_world, dec, root_moves, config)
        k = child.visits
        world = determinize(st, player, world_seeds[k])
        apply(world, action, check=False)
        before = child.total
        _iterate(ctx, child, fresh, world, SplitMix64(rollout_seeds[k]))
        if cap > budget:
            per_visit[slot[id(child)]].append(child.total - before)
        done += 1
        if done == limit or _settled(root, keys, limit - done):
            if limit == cap or not _close_call(children, per_visit, config.target_extension_z):
                break
            limit = min(cap, limit + budget)
    return _result(root, keys, root_moves, done)


def is_target_decision(dec: Decision) -> bool:
    """True when ``dec`` chooses the cards an effect acts on (targets, cards to trash or
    discard, the order of cards put on a deck), not a mode or a card to rest in place of a
    protected one."""
    return dec.kind in _TARGET_KINDS and uid_argument(dec) and dec.ctx("rest_sub") < 0


def _iteration_cap(config: SearchConfig, dec: Decision, budget: int) -> int:
    """Most iterations the root may spend: the budget, or for an effect target decision up to
    ``target_extension`` times it, added one budget at a time while the move to play is a close
    call (``_close_call``)."""
    if not is_target_decision(dec):
        return budget
    return max(budget, round(budget * config.target_extension))


def _close_call(children: list[_Child], per_visit: list[list[float]], z: float) -> bool:
    """True when the move the search would play is not ``z`` paired standard errors better
    than some other visited move. The k-th visit of every root move was played in the k-th
    world with the k-th rollout seed, so the values of the visits two moves share are pairs.
    Two moves that scored the same in every shared world are a tie that more iterations would
    not break, and do not count."""
    leader = max(range(len(children)), key=lambda i: (children[i].visits, children[i].mean()))
    lead = per_visit[leader]
    for i, other in enumerate(per_visit):
        if i == leader or not other:
            continue
        n = min(len(lead), len(other))
        if n < 2:
            return True
        diffs = [lead[j] - other[j] for j in range(n)]
        if max(abs(x) for x in diffs) <= TIE_TOLERANCE:
            continue
        mean = sum(diffs) / n
        var = sum((x - mean) ** 2 for x in diffs) / (n - 1)
        if mean <= z * math.sqrt(var / n):
            return True
    return False


def _iterate(ctx: _Search, first: _Child, fresh: bool, s: GameState, rng: SplitMix64) -> None:
    """Continue one iteration after the root move ``first`` was applied to world ``s``."""
    path = [first]
    if not fresh:
        _descend(ctx, _node_of(first), s, path)
    cfg = ctx.config
    played = play_until_turn(
        s, ctx.horizon, rng, cfg.rollout_temperature, cfg.max_rollout_decisions, knobs_for(cfg)
    )
    if cfg.lethal_extension_turns:
        _extend_rollout(ctx, s, rng, cfg.max_rollout_decisions - played)
    value = _leaf_value(s, ctx)
    for child in path:
        child.visits += 1
        child.total += value if child.mover == ctx.player else 1.0 - value


def _extend_rollout(ctx: _Search, s: GameState, rng: SplitMix64, budget: int) -> None:
    """Play ``lethal_extension_turns`` more turns, within the ``budget`` left of the rollout's
    decisions, when the rollout reached the horizon in a lethal-threat position (by
    ``extension_trigger``), so the leaf value sees whether the attack lands."""
    cfg = ctx.config
    if s.winner is not None or s.turn < ctx.horizon or budget <= 0:
        return
    if not _lethal_threat(ctx, s):
        return
    play_until_turn(
        s,
        ctx.horizon + cfg.lethal_extension_turns,
        rng,
        cfg.rollout_temperature,
        budget,
        _extension_knobs(cfg),
    )


def _lethal_threat(ctx: _Search, s: GameState) -> bool:
    trigger = ctx.config.extension_trigger
    if trigger == "threat":
        return threat_to_move(s)
    if trigger == "opponent" and s.active == ctx.player:
        return False
    return can_kill_to_move(s)


@cache
def _extension_knobs(cfg: SearchConfig) -> PolicyKnobs:
    knobs = knobs_for(cfg)
    return replace(knobs, push=True) if cfg.rollout_push == "extension" else knobs


def _descend(ctx: _Search, node: _Node, s: GameState, path: list[_Child]) -> None:
    """Walk the tree in world ``s`` until a new node is added or the horizon is reached."""
    while s.winner is None and s.turn < ctx.horizon and s.pending is not None:
        dec = s.pending
        moves = canonical(s, dec)
        if len(moves) == 1:
            apply(s, moves[0][1], check=False)
            continue
        child, action, fresh = _select(node, s, dec, moves, ctx.config)
        apply(s, action, check=False)
        path.append(child)
        if fresh:
            return
        if ctx.config.observation_keys and _unobserved_pass(dec, action, ctx.player):
            continue
        node = _node_of(child)


def _unobserved_pass(dec: Decision, action: Action, player: int) -> bool:
    """An opponent's pass in an action step looks to ``player`` the same as the engine passing
    for an opponent without options, which happens without a decision."""
    return (
        dec.player != player
        and dec.kind is DecisionKind.ACTION_STEP
        and action.kind is ActionKind.PASS
    )


def _node_of(child: _Child) -> _Node:
    if child.node is None:
        child.node = _Node()
    return child.node


def _leaf_value(s: GameState, ctx: _Search) -> float:
    """Value for the searching player: finished games score 1/0 shaded by how many turns
    they took (win sooner, lose later); unfinished ones map the evaluation, tempered by
    ``value_scale``, strictly inside that range so a certain result outranks any estimate."""
    if s.winner is not None:
        elapsed = WIN_DECAY * (s.turn - ctx.root_turn)
        if s.winner == ctx.player:
            return 1.0 - elapsed
        return elapsed if s.winner == 1 - ctx.player else 0.5
    z = score(s, ctx.player, ctx.weights, ctx.density) / ctx.config.value_scale
    return LEAF_FLOOR + (1.0 - 2.0 * LEAF_FLOOR) * logistic(z)


def _child_key(dec: Decision, key: Key, cfg: SearchConfig) -> ChildKey:
    return (dec.player, dec.kind.value if cfg.observation_keys else "", key)


def _select(
    node: _Node,
    s: GameState,
    dec: Decision,
    moves: list[tuple[Key, Action]],
    cfg: SearchConfig,
) -> tuple[_Child, Action, bool]:
    """Pick a child by UCB1 + progressive prior; unvisited moves go first, best prior first."""
    children = node.children
    keys = [_child_key(dec, key, cfg) for key, _ in moves]
    if any(key not in children for key in keys):
        _add_children(node, s, dec, moves, cfg)
    avail = [(children[key], action) for key, (_, action) in zip(keys, moves, strict=True)]
    for child, _ in avail:
        child.avail += 1
    unvisited = [(c, a) for c, a in avail if c.visits == 0]
    if unvisited:
        child, action = max(unvisited, key=lambda ca: ca[0].prior)
        return child, action, True
    c = cfg.exploration
    w = cfg.prior_weight

    def ucb(ca: tuple[_Child, Action]) -> float:
        ch = ca[0]
        return (
            ch.mean()
            + c * math.sqrt(math.log(ch.avail) / ch.visits)
            + w * ch.prior / (ch.visits + 1)
        )

    child, action = max(avail, key=ucb)
    return child, action, False


def _add_children(
    node: _Node, s: GameState, dec: Decision, moves: list[tuple[Key, Action]], cfg: SearchConfig
) -> None:
    scores = policy.option_scores(
        s,
        Decision(dec.player, dec.kind, tuple(a for _, a in moves), dec.prompt, dec.context),
        knobs_for(cfg),
    )
    top = max(scores)
    weights = [math.exp((x - top) / cfg.prior_temperature) for x in scores]
    total = sum(weights)
    for (key, _), wgt in zip(moves, weights, strict=True):
        node.children.setdefault(_child_key(dec, key, cfg), _Child(dec.player, wgt / total))


def _settled(root: _Node, keys: list[ChildKey], remaining: int) -> bool:
    """True when the most-visited move can no longer be overtaken in the remaining budget."""
    visits = sorted(
        (ch.visits for key in keys if (ch := root.children.get(key)) is not None),
        reverse=True,
    )
    if len(visits) < len(keys):
        return False
    return visits[0] - visits[1] > remaining


def _result(
    root: _Node, keys: list[ChildKey], moves: list[tuple[Key, Action]], iterations: int
) -> SearchResult:
    stats: list[Alternative] = []
    for key, (_, action) in zip(keys, moves, strict=True):
        ch = root.children.get(key)
        stats.append(Alternative(action, ch.mean() if ch else 0.0, ch.visits if ch else 0))
    best = max(range(len(stats)), key=lambda i: (stats[i].visits, stats[i].value))
    return SearchResult(stats[best].action, stats, iterations)
