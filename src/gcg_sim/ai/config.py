"""Search budgets for the MCTS player. Budgets are iteration counts, never wall-clock time."""

from __future__ import annotations

from collections.abc import Mapping
from dataclasses import dataclass, replace
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
    policy_burst_yes_score: float = 1.0
    noop_branch_choices: bool = False
    noop_resource_aware: bool = False
    strict_attack_prune: bool = False
    prune_noop_rest_targets: bool = False
    merge_equivalent_moves: bool = False
    observation_keys: bool = False
    target_extension: float = 1.0
    target_extension_z: float = 2.0
    policy_rule_polarity_table: bool = False
    policy_zero_ap_face: bool = False

    def budget(self, n_options: int) -> int:
        wanted = self.iterations_per_option * n_options
        return max(self.min_iterations, min(self.max_iterations, wanted))


def _adopted(config: SearchConfig) -> SearchConfig:
    """``config`` with the settings adopted after the 2026-10 measurements (docs/AI.md):
    root pruning and no-effect fixes, policy fixes, close target extension, observation keys
    and the calibrated leaf value."""
    return replace(
        config,
        value_scale=1.0,
        policy_burst_yes_score=4.0,
        noop_branch_choices=True,
        noop_resource_aware=True,
        strict_attack_prune=True,
        prune_noop_rest_targets=True,
        merge_equivalent_moves=True,
        observation_keys=True,
        target_extension=3.0,
        policy_rule_polarity_table=True,
        policy_zero_ap_face=True,
    )


PRESETS: Mapping[str, SearchConfig] = MappingProxyType(
    {
        "fast": _adopted(
            SearchConfig(
                name="fast",
                max_iterations=64,
                min_iterations=16,
                iterations_per_option=7,
                exploration=0.15,
                rollout_temperature=0.3,
                first_player_games=8,
            )
        ),
        "standard": _adopted(
            SearchConfig(
                name="standard", max_iterations=640, min_iterations=160, iterations_per_option=64
            )
        ),
        "strong": _adopted(
            SearchConfig(
                name="strong",
                max_iterations=1280,
                min_iterations=320,
                iterations_per_option=128,
                exploration=0.15,
                rollout_temperature=0.3,
                first_player_games=48,
                mulligan_samples=192,
            )
        ),
    }
)
"""From cheapest to strongest: ``fast`` screens deck variants, ``standard`` is the default,
``strong`` confirms. Measured costs and strength are in docs/AI.md."""
