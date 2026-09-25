"""Search budgets for the MCTS player. Budgets are iteration counts, never wall-clock time."""

from __future__ import annotations

from collections.abc import Mapping
from dataclasses import dataclass
from types import MappingProxyType


@dataclass(frozen=True, slots=True)
class SearchConfig:
    """Deterministic knobs of the determinized MCTS player (documented in docs/AI.md).

    A decision with ``n`` distinct candidate moves gets
    ``clamp(iterations_per_option * n, min_iterations, max_iterations)`` iterations; each
    iteration plays a root move in a determinization, descends the tree, and plays the
    rollout policy until ``horizon_turns`` turn boundaries have passed.
    """

    name: str
    max_iterations: int
    min_iterations: int
    iterations_per_option: int
    exploration: float = 0.3
    prior_weight: float = 0.5
    prior_temperature: float = 1.5
    horizon_turns: int = 2
    rollout_temperature: float = 0.5
    value_scale: float = 2.0
    max_rollout_decisions: int = 250
    first_player_games: int = 24
    mulligan_samples: int = 96

    def budget(self, n_options: int) -> int:
        wanted = self.iterations_per_option * n_options
        return max(self.min_iterations, min(self.max_iterations, wanted))


PRESETS: Mapping[str, SearchConfig] = MappingProxyType(
    {
        "standard": SearchConfig(
            name="standard", max_iterations=320, min_iterations=80, iterations_per_option=32
        ),
        "strong": SearchConfig(
            name="strong",
            max_iterations=1280,
            min_iterations=320,
            iterations_per_option=128,
            exploration=0.15,
            rollout_temperature=0.3,
            first_player_games=48,
            mulligan_samples=192,
        ),
    }
)
