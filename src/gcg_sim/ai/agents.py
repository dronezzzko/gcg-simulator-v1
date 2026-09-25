"""The three players (uniform random, one-ply greedy, MCTS) and the ``make_agent`` factory."""

from __future__ import annotations

from gcg_sim.ai.actions import canonical
from gcg_sim.ai.base import Agent, AgentCore, Alternative
from gcg_sim.ai.config import PRESETS, SearchConfig
from gcg_sim.ai.evaluation import DEFAULT_WEIGHTS, Weights, burst_density, evaluate
from gcg_sim.ai.mcts import search
from gcg_sim.ai.playout import play_until_turn
from gcg_sim.ai.setup import first_player_choice, mulligan_choice
from gcg_sim.engine.game import apply
from gcg_sim.engine.observe import determinize
from gcg_sim.engine.state import Action, GameState
from gcg_sim.engine.types import ActionKind, DecisionKind

GREEDY_MULLIGAN_SAMPLES = 48
GREEDY_MAX_DECISIONS = 200


class RandomAgent(AgentCore):
    """Uniformly random legal actions from a seeded generator."""

    def __init__(self, seed: int = 0, *, log: bool = False) -> None:
        super().__init__("random", seed, log)

    def choose(self, st: GameState, player: int) -> Action:
        dec, rng = self._begin(st, player)
        chosen = dec.options[rng.randrange(len(dec.options))]
        share = 1.0 / len(dec.options)
        return self._finish(st, player, chosen, [Alternative(o, share, 0) for o in dec.options])


class GreedyAgent(AgentCore):
    """One-ply look-ahead: each distinct move is applied to one determinization of the
    information set, the rest of the current turn is completed by the deterministic playout
    policy, and the resulting position is scored with the evaluation function. Every move is
    judged at the same point (the next turn's first decision), so ending the turn is compared
    fairly with acting."""

    def __init__(
        self, seed: int = 0, *, log: bool = False, weights: Weights = DEFAULT_WEIGHTS
    ) -> None:
        super().__init__("greedy", seed, log)
        self.weights = weights

    def choose(self, st: GameState, player: int) -> Action:
        dec, rng = self._begin(st, player)
        if len(dec.options) == 1:
            return self._finish(st, player, dec.options[0], [])
        if dec.kind is DecisionKind.REDRAW:
            action, stats = mulligan_choice(st, player, GREEDY_MULLIGAN_SAMPLES, rng)
            return self._finish(st, player, action, stats)
        if dec.kind is DecisionKind.CHOOSE_FIRST:
            return self._finish(st, player, Action(ActionKind.GO_FIRST, player), [])
        det = determinize(st, player, rng.next_u64())
        density = burst_density(st, player)
        scored: list[Alternative] = []
        for _, action in canonical(st, dec):
            s = det.clone()
            apply(s, action, check=False)
            play_until_turn(s, st.turn + 1, None, 0.0, GREEDY_MAX_DECISIONS)
            scored.append(Alternative(action, evaluate(s, player, self.weights, density), 1))
        best = max(range(len(scored)), key=lambda i: scored[i].value)
        return self._finish(st, player, scored[best].action, scored)


class MctsAgent(AgentCore):
    """The strong player: determinized information-set MCTS (see :mod:`gcg_sim.ai.mcts`),
    a simulation-based play/draw choice, and an expected-value mulligan."""

    def __init__(
        self,
        seed: int = 0,
        *,
        config: SearchConfig = PRESETS["standard"],
        log: bool = False,
        weights: Weights = DEFAULT_WEIGHTS,
    ) -> None:
        super().__init__(f"mcts-{config.name}", seed, log)
        self.config = config
        self.weights = weights

    def choose(self, st: GameState, player: int) -> Action:
        dec, rng = self._begin(st, player)
        if len(dec.options) == 1:
            return self._finish(st, player, dec.options[0], [])
        if dec.kind is DecisionKind.REDRAW:
            action, stats = mulligan_choice(st, player, self.config.mulligan_samples, rng)
            return self._finish(st, player, action, stats)
        if dec.kind is DecisionKind.CHOOSE_FIRST:
            action, stats = first_player_choice(st, player, self.config.first_player_games, rng)
            return self._finish(st, player, action, stats)
        result = search(st, player, self.config, self.weights, rng)
        return self._finish(st, player, result.action, result.alternatives)


AGENT_KINDS = ("random", "greedy", "mcts")


def make_agent(kind: str, *, preset: str = "standard", seed: int = 0, log: bool = False) -> Agent:
    """Build an agent: ``kind`` is "random", "greedy", or "mcts"; ``preset`` selects the MCTS
    budget ("standard" or "strong") and is ignored by the baselines."""
    if preset not in PRESETS:
        raise ValueError(f"unknown AI preset {preset!r}; choose from {sorted(PRESETS)}")
    if kind == "random":
        return RandomAgent(seed, log=log)
    if kind == "greedy":
        return GreedyAgent(seed, log=log)
    if kind == "mcts":
        return MctsAgent(seed, config=PRESETS[preset], log=log)
    raise ValueError(f"unknown agent kind {kind!r}; choose from {list(AGENT_KINDS)}")
