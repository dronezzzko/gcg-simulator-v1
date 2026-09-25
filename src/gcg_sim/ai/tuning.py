"""Reproducible self-play tuning of the evaluation weights.

Method (see docs/AI.md): MCTS agents with a small budget play each other with the current
weights on the tuning decks (which differ from the AI test decks). At the first decision of
every turn the active player's feature vector is recorded; the label is whether that player
went on to win (draws are dropped). An L2-regularised logistic regression fitted by Newton's
method on the even-indexed games (pooled over rounds) gives the next round's weights, and the
round's odd-indexed games measure held-out log-loss. The final weights are fitted on every
position of every round; a mirrored head-to-head match then compares them with the hand-set
weights. Every seed derives from ``--seed``, so a run is repeatable bit for bit for any
worker count.

Run: ``uv run python -m gcg_sim.ai.tuning --games 540 --rounds 2 --validate 216 --workers 13``
"""

from __future__ import annotations

import argparse
import json
import math
from collections.abc import Sequence
from concurrent.futures import ProcessPoolExecutor
from dataclasses import dataclass
from multiprocessing import get_context
from typing import Any

from gcg_sim.ai.agents import MctsAgent
from gcg_sim.ai.config import PRESETS, SearchConfig
from gcg_sim.ai.evaluation import (
    FEATURES,
    HAND_WEIGHTS,
    N_FEATURES,
    Weights,
    burst_density,
    features,
    logistic,
)
from gcg_sim.ai.selfplay import MIRROR_BLOCK, run_game
from gcg_sim.engine.game import DeckList
from gcg_sim.engine.state import GameState
from gcg_sim.engine.types import DecisionKind
from gcg_sim.rng import derive_seed

TUNING_CONFIG = SearchConfig(
    name="tuning",
    max_iterations=48,
    min_iterations=16,
    iterations_per_option=6,
    first_player_games=8,
    mulligan_samples=32,
)
L2 = 1.0
NEWTON_STEPS = 30

RESOURCES = ("R-001",) * 10
ZEON_COUNTS = {
    "ST03-006": 3, "ST03-007": 3, "ST03-008": 4, "GD01-031": 3, "GD01-030": 3, "GD01-036": 3,
    "GD01-037": 2, "GD01-032": 2, "GD01-027": 2, "GD01-092": 3, "GD02-089": 2, "ST03-003": 3,
    "ST03-004": 2, "GD01-059": 2, "GD01-105": 2, "ST03-013": 3, "GD01-115": 2, "ST03-015": 2,
    "GD01-126": 2, "ST03-012": 2,
}  # fmt: skip
WING_COUNTS = {
    "ST02-001": 2, "ST02-002": 3, "ST02-003": 2, "ST02-004": 3, "ST02-005": 4, "GD01-033": 3,
    "GD01-040": 3, "GD01-041": 2, "GD01-043": 2, "GD02-028": 3, "ST02-010": 4, "ST02-006": 2,
    "ST02-008": 3, "ST02-009": 3, "ST02-011": 2, "ST02-012": 2, "ST02-014": 3, "GD01-126": 2,
    "GD01-099": 2,
}  # fmt: skip
TEKKADAN_COUNTS = {
    "ST05-001": 2, "ST05-002": 3, "ST05-003": 3, "ST05-004": 4, "ST05-005": 3, "ST05-006": 3,
    "GD03-066": 2, "GD03-056": 2, "GD03-068": 2, "GD05-058": 3, "GD05-065": 3, "ST05-010": 4,
    "ST05-011": 3, "ST05-013": 2, "ST05-014": 3, "GD05-117": 2, "ST05-015": 2, "GD03-067": 2,
    "GD05-096": 2,
}  # fmt: skip


def _deck(counts: dict[str, int]) -> DeckList:
    return DeckList(tuple(n for n, c in sorted(counts.items()) for _ in range(c)), RESOURCES)


TUNING_DECKS: tuple[DeckList, ...] = (
    _deck(ZEON_COUNTS),
    _deck(WING_COUNTS),
    _deck(TEKKADAN_COUNTS),
)
MATCHUPS: tuple[tuple[int, int], ...] = tuple(
    (a, b) for a in range(len(TUNING_DECKS)) for b in range(len(TUNING_DECKS))
)

Sample = tuple[tuple[float, ...], float]


@dataclass(frozen=True, slots=True)
class TuningGame:
    index: int
    seed: int
    agent_seeds: tuple[int, int]
    decks: tuple[DeckList, DeckList]
    weights: tuple[Weights, Weights]
    config: SearchConfig = TUNING_CONFIG


def tuning_games(
    games: int, master: int, weights: Weights, config: SearchConfig = TUNING_CONFIG
) -> list[TuningGame]:
    out = []
    for i in range(games):
        a, b = MATCHUPS[i % len(MATCHUPS)]
        out.append(
            TuningGame(
                index=i,
                seed=derive_seed(master, "tuning-game", i),
                agent_seeds=(
                    derive_seed(master, "agent", i, 0),
                    derive_seed(master, "agent", i, 1),
                ),
                decks=(TUNING_DECKS[a], TUNING_DECKS[b]),
                weights=(weights, weights),
                config=config,
            )
        )
    return out


def collect(game: TuningGame) -> list[Sample]:
    """Play one self-play game; return (active player's features, 1.0 if they won)."""
    agents = (
        MctsAgent(game.agent_seeds[0], config=game.config, weights=game.weights[0]),
        MctsAgent(game.agent_seeds[1], config=game.config, weights=game.weights[1]),
    )
    seen: list[tuple[int, tuple[float, ...]]] = []
    last_turn = [0]

    def observe(st: GameState) -> None:
        dec = st.pending
        if dec is None or dec.kind is not DecisionKind.MAIN or st.turn == last_turn[0]:
            return
        last_turn[0] = st.turn
        density = burst_density(st, st.active)
        seen.append((st.active, tuple(features(st, st.active, density))))

    st, _ = run_game(agents, game.decks, game.seed, observe)
    if st.winner not in (0, 1):
        return []
    return [(x, 1.0 if player == st.winner else 0.0) for player, x in seen]


def play_games(games: Sequence[TuningGame], workers: int) -> list[list[Sample]]:
    if workers <= 1:
        return [collect(g) for g in games]
    with ProcessPoolExecutor(max_workers=workers, mp_context=get_context("spawn")) as pool:
        return list(pool.map(collect, games, chunksize=1))


# ---------------------------------------------------------------------------------------------
# logistic regression


def log_loss(samples: Sequence[Sample], w: Sequence[float]) -> float:
    total = 0.0
    for x, y in samples:
        p = min(1.0 - 1e-12, max(1e-12, logistic(sum(a * b for a, b in zip(w, x, strict=True)))))
        total -= y * math.log(p) + (1.0 - y) * math.log(1.0 - p)
    return total / max(1, len(samples))


def fit_logistic(samples: Sequence[Sample], l2: float = L2, steps: int = NEWTON_STEPS) -> Weights:
    """L2-regularised maximum likelihood by Newton's method (deterministic, pure Python)."""
    n = N_FEATURES
    w = [0.0] * n
    for _ in range(steps):
        grad = [l2 * wi for wi in w]
        hess = [[l2 if i == j else 0.0 for j in range(n)] for i in range(n)]
        for x, y in samples:
            p = logistic(sum(a * b for a, b in zip(w, x, strict=True)))
            r = p - y
            s = p * (1.0 - p)
            for i in range(n):
                xi = x[i]
                if xi == 0.0:
                    continue
                grad[i] += r * xi
                row = hess[i]
                sxi = s * xi
                for j in range(n):
                    row[j] += sxi * x[j]
        step = _solve(hess, grad)
        w = [wi - si for wi, si in zip(w, step, strict=True)]
        if max(abs(si) for si in step) < 1e-9:
            break
    return Weights(tuple(round(wi, 4) for wi in w))


def _solve(a: list[list[float]], b: list[float]) -> list[float]:
    """Gaussian elimination with partial pivoting (the Hessian is positive definite)."""
    n = len(b)
    m = [[*row, rhs] for row, rhs in zip(a, b, strict=True)]
    for col in range(n):
        pivot = max(range(col, n), key=lambda r: abs(m[r][col]))
        m[col], m[pivot] = m[pivot], m[col]
        for r in range(col + 1, n):
            f = m[r][col] / m[col][col]
            if f:
                for c in range(col, n + 1):
                    m[r][c] -= f * m[col][c]
    x = [0.0] * n
    for r in range(n - 1, -1, -1):
        x[r] = (m[r][n] - sum(m[r][c] * x[c] for c in range(r + 1, n))) / m[r][r]
    return x


# ---------------------------------------------------------------------------------------------
# validation match


@dataclass(frozen=True, slots=True)
class DuelGame:
    index: int
    seed: int
    agent_seeds: tuple[int, int]
    decks: tuple[DeckList, DeckList]
    weights: tuple[Weights, Weights]
    challenger: int
    config: SearchConfig = TUNING_CONFIG


def duel_games(
    games: int,
    master: int,
    challenger: Weights,
    opponent: Weights,
    config: SearchConfig = TUNING_CONFIG,
) -> list[DuelGame]:
    """Mirrored blocks: each deal is played with the challenger in either seat."""
    out = []
    for i in range(games):
        block, variant = divmod(i, MIRROR_BLOCK)
        swap, seat = divmod(variant, 2)
        a, b = MATCHUPS[(block + swap) % len(MATCHUPS)]
        weights = (challenger, opponent) if seat == 0 else (opponent, challenger)
        out.append(
            DuelGame(
                index=i,
                seed=derive_seed(master, "duel", block, swap),
                agent_seeds=(
                    derive_seed(master, "duel-agent", i, 0),
                    derive_seed(master, "duel-agent", i, 1),
                ),
                decks=(TUNING_DECKS[a], TUNING_DECKS[b]),
                weights=weights,
                challenger=seat,
                config=config,
            )
        )
    return out


def duel(game: DuelGame) -> float:
    """Challenger's score in one game (1 win, 0.5 draw, 0 loss)."""
    agents = (
        MctsAgent(game.agent_seeds[0], config=game.config, weights=game.weights[0]),
        MctsAgent(game.agent_seeds[1], config=game.config, weights=game.weights[1]),
    )
    st, _ = run_game(agents, game.decks, game.seed)
    if st.winner == game.challenger:
        return 1.0
    return 0.5 if st.winner not in (0, 1) else 0.0


def play_duels(games: Sequence[DuelGame], workers: int) -> list[float]:
    if workers <= 1:
        return [duel(g) for g in games]
    with ProcessPoolExecutor(max_workers=workers, mp_context=get_context("spawn")) as pool:
        return list(pool.map(duel, games, chunksize=1))


# ---------------------------------------------------------------------------------------------
# driver


def tune(
    games: int,
    rounds: int,
    seed: int,
    workers: int,
    start: Weights = HAND_WEIGHTS,
    config: SearchConfig = TUNING_CONFIG,
) -> dict[str, Any]:
    """Run the tuning rounds; returns a JSON-serialisable report including the final weights.

    Round ``r`` plays ``games`` self-play games with the current weights, fits on the
    even-indexed games of rounds ``0..r`` pooled, and reports log-loss on the odd-indexed
    games of round ``r`` before and after. The final weights are fitted on every position.
    """
    weights = start
    report: dict[str, Any] = {"seed": seed, "games": games, "rounds": []}
    pooled_train: list[Sample] = []
    pooled_all: list[Sample] = []
    for r in range(rounds):
        specs = tuning_games(games, derive_seed(seed, "round", r), weights, config)
        per_game = play_games(specs, workers)
        pooled_train += [s for i, g in enumerate(per_game) if i % 2 == 0 for s in g]
        test = [s for i, g in enumerate(per_game) if i % 2 == 1 for s in g]
        pooled_all += [s for g in per_game for s in g]
        fitted = fit_logistic(pooled_train)
        report["rounds"].append(
            {
                "round": r,
                "positions": sum(len(g) for g in per_game),
                "heldout_logloss_before": round(log_loss(test, weights.values), 4),
                "heldout_logloss_after": round(log_loss(test, fitted.values), 4),
                "weights": fitted.as_dict(),
            }
        )
        weights = fitted
    final = fit_logistic(pooled_all) if pooled_all else weights
    report["positions"] = len(pooled_all)
    report["weights"] = list(final.values)
    return report


def validate(
    games: int,
    seed: int,
    workers: int,
    tuned: Weights,
    baseline: Weights,
    config: SearchConfig = TUNING_CONFIG,
) -> dict[str, Any]:
    """Mirrored head-to-head of ``tuned`` against ``baseline`` weights."""
    specs = duel_games(games, derive_seed(seed, "validate"), tuned, baseline, config)
    scores = play_duels(specs, workers)
    return {
        "games": games,
        "preset": config.name,
        "tuned_score": sum(scores),
        "tuned_rate": round(sum(scores) / max(1, games), 4),
    }


def main(argv: Sequence[str] | None = None) -> None:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--games", type=int, default=240)
    ap.add_argument("--rounds", type=int, default=2)
    ap.add_argument("--seed", type=int, default=2026)
    ap.add_argument("--workers", type=int, default=1)
    ap.add_argument("--validate", type=int, default=0, help="head-to-head games (multiple of 4)")
    ap.add_argument("--validate-preset", default="tuning", choices=["tuning", *sorted(PRESETS)])
    args = ap.parse_args(argv)
    report = tune(args.games, args.rounds, args.seed, args.workers)
    if args.validate:
        tuned = Weights(tuple(report["weights"]))
        preset = PRESETS.get(args.validate_preset, TUNING_CONFIG)
        report["validation"] = validate(
            args.validate, args.seed, args.workers, tuned, HAND_WEIGHTS, preset
        )
    report["features"] = list(FEATURES)
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
