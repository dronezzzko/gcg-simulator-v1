"""Information-set MCTS with determinizations (single-observer ISMCTS).

One tree is built from the deciding player's point of view. Every iteration picks a root
move by UCB1 plus a progressive prior from the playout policy, then plays it in a
determinization of the player's information set (hidden cards redrawn from the open
decklists, the game RNG replaced). The n-th visit of every root move uses the same n-th
determinization and rollout seed (common random numbers), so root moves are compared on
identical worlds. Below the root the walk chooses among the moves legal in that world
(availability counts keep the bandit fair when a move exists only in some worlds), adds one
node, plays the heuristic policy until ``horizon_turns`` turn boundaries have passed, and
backs up the evaluation. Only the true state's pending options and the player's own
knowledge are read directly.
"""

from __future__ import annotations

import math
from dataclasses import dataclass, field

from gcg_sim.ai import policy
from gcg_sim.ai.actions import Key, canonical, prune_dominated
from gcg_sim.ai.base import Alternative
from gcg_sim.ai.config import SearchConfig
from gcg_sim.ai.evaluation import Weights, burst_density, logistic, score
from gcg_sim.ai.playout import play_until_turn
from gcg_sim.engine.game import apply
from gcg_sim.engine.observe import determinize
from gcg_sim.engine.state import Action, Decision, GameState
from gcg_sim.rng import SplitMix64

ChildKey = tuple[int, Key]
WIN_DECAY = 0.01
LEAF_FLOOR = 0.05


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
    root_moves = prune_dominated(st, dec, canonical(st, dec))
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
    root = _Node()
    first_world = determinize(st, player, world_seeds[0])
    _add_children(root, first_world, dec, root_moves, config)
    done = 0
    while done < budget:
        child, action, fresh = _select(root, first_world, dec, root_moves, config)
        k = child.visits
        world = determinize(st, player, world_seeds[k])
        apply(world, action, check=False)
        _iterate(ctx, child, fresh, world, SplitMix64(rollout_seeds[k]))
        done += 1
        if _settled(root, player, root_moves, budget - done):
            break
    return _result(root, player, root_moves, done)


def _iterate(ctx: _Search, first: _Child, fresh: bool, s: GameState, rng: SplitMix64) -> None:
    """Continue one iteration after the root move ``first`` was applied to world ``s``."""
    path = [first]
    if not fresh:
        _descend(ctx, _node_of(first), s, path)
    cfg = ctx.config
    play_until_turn(s, ctx.horizon, rng, cfg.rollout_temperature, cfg.max_rollout_decisions)
    value = _leaf_value(s, ctx)
    for child in path:
        child.visits += 1
        child.total += value if child.mover == ctx.player else 1.0 - value


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
        node = _node_of(child)


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


def _select(
    node: _Node,
    s: GameState,
    dec: Decision,
    moves: list[tuple[Key, Action]],
    cfg: SearchConfig,
) -> tuple[_Child, Action, bool]:
    """Pick a child by UCB1 + progressive prior; unvisited moves go first, best prior first."""
    mover = dec.player
    children = node.children
    if any((mover, key) not in children for key, _ in moves):
        _add_children(node, s, dec, moves, cfg)
    avail = [(children[(mover, key)], action) for key, action in moves]
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
        s, Decision(dec.player, dec.kind, tuple(a for _, a in moves), dec.prompt, dec.context)
    )
    top = max(scores)
    weights = [math.exp((x - top) / cfg.prior_temperature) for x in scores]
    total = sum(weights)
    for (key, _), wgt in zip(moves, weights, strict=True):
        node.children.setdefault((dec.player, key), _Child(dec.player, wgt / total))


def _settled(root: _Node, player: int, moves: list[tuple[Key, Action]], remaining: int) -> bool:
    """True when the most-visited move can no longer be overtaken in the remaining budget."""
    visits = sorted(
        (ch.visits for key, _ in moves if (ch := root.children.get((player, key))) is not None),
        reverse=True,
    )
    if len(visits) < len(moves):
        return False
    return visits[0] - visits[1] > remaining


def _result(
    root: _Node, player: int, moves: list[tuple[Key, Action]], iterations: int
) -> SearchResult:
    stats: list[Alternative] = []
    for key, action in moves:
        ch = root.children.get((player, key))
        stats.append(Alternative(action, ch.mean() if ch else 0.0, ch.visits if ch else 0))
    best = max(range(len(stats)), key=lambda i: (stats[i].visits, stats[i].value))
    return SearchResult(stats[best].action, stats, iterations)
