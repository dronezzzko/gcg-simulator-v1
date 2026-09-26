"""Measure how many MCTS-vs-MCTS games a preset plays per core-minute on two fixed decks.

Wall-clock time is read here only to report speed; it never influences a decision or a result.

Run: ``uv run python -m gcg_sim.ai.throughput --preset standard --games 26 --workers 13``
"""

from __future__ import annotations

import argparse
import json
import time
from collections.abc import Sequence
from typing import Any

from gcg_sim.ai.config import PRESETS
from gcg_sim.ai.selfplay import AgentSpec, mirrored_specs, play_all
from gcg_sim.ai.tuning import TUNING_DECKS
from gcg_sim.engine.game import DeckList

DEFAULT_DECKS: tuple[DeckList, DeckList] = (TUNING_DECKS[0], TUNING_DECKS[1])


def measure(
    preset: str,
    games: int,
    workers: int,
    seed: int = 0,
    decks: tuple[DeckList, DeckList] = DEFAULT_DECKS,
) -> dict[str, Any]:
    """Play ``games`` mirrored games of the preset against itself and time them."""
    agent = AgentSpec("mcts", preset)
    specs = mirrored_specs(games, seed, agent, agent, decks)
    workers = max(1, min(workers, games))
    start = time.perf_counter()
    outcomes = play_all(specs, workers)
    wall = time.perf_counter() - start
    core_minutes = wall * workers / 60.0
    return {
        "preset": preset,
        "games": games,
        "workers": workers,
        "wall_seconds": round(wall, 1),
        "games_per_core_minute": round(games / core_minutes, 3),
        "core_seconds_per_game": round(wall * workers / games, 1),
        "mean_turns": round(sum(o.turns for o in outcomes) / games, 1),
        "mean_decisions": round(sum(o.decisions for o in outcomes) / games, 1),
    }


def main(argv: Sequence[str] | None = None) -> None:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--preset", default="standard", choices=sorted(PRESETS))
    ap.add_argument("--games", type=int, default=8)
    ap.add_argument("--workers", type=int, default=1)
    ap.add_argument("--seed", type=int, default=0)
    args = ap.parse_args(argv)
    print(json.dumps(measure(args.preset, args.games, args.workers, args.seed), indent=2))


if __name__ == "__main__":
    main()
