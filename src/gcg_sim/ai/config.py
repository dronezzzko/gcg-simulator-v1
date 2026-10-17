"""Search budgets for the MCTS player. Budgets are iteration counts, never wall-clock time."""

from __future__ import annotations

from collections.abc import Mapping
from dataclasses import dataclass
from types import MappingProxyType

WIN_DECAY = 0.01
LEAF_FLOOR = 0.05
EXTENSION_TRIGGERS = ("can_kill", "threat", "opponent")
PUSH_MODES = ("off", "extension", "all")


@dataclass(frozen=True, slots=True)
class SearchConfig:
    """Deterministic knobs of the determinized MCTS player (documented in docs/AI.md).

    A decision with ``n`` distinct candidate moves gets
    ``clamp(iterations_per_option * n, min_iterations, max_iterations)`` iterations; each
    iteration plays a root move in a determinization, descends the tree, and plays the
    rollout policy until ``horizon_turns`` turn boundaries have passed. With
    ``lethal_extension_turns`` > 0, a rollout whose horizon position is a lethal threat by
    ``extension_trigger`` plays that many more turns. ``rollout_push`` and ``rollout_guard``
    switch on the policy's lethal push and Blocker guard (``policy.PolicyKnobs``).
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
    lethal_extension_turns: int = 0
    extension_trigger: str = "can_kill"
    rollout_push: str = "off"
    rollout_guard: bool = False

    def __post_init__(self) -> None:
        if self.extension_trigger not in EXTENSION_TRIGGERS:
            raise ValueError(f"extension_trigger must be one of {EXTENSION_TRIGGERS}")
        if self.rollout_push not in PUSH_MODES:
            raise ValueError(f"rollout_push must be one of {PUSH_MODES}")
        if WIN_DECAY * (self.horizon_turns + self.lethal_extension_turns) >= LEAF_FLOOR:
            raise ValueError(
                "horizon_turns + lethal_extension_turns is too long: a win at the end of it "
                "would not outrank every evaluated position"
            )

    def budget(self, n_options: int) -> int:
        wanted = self.iterations_per_option * n_options
        return max(self.min_iterations, min(self.max_iterations, wanted))


PRESETS: Mapping[str, SearchConfig] = MappingProxyType(
    {
        "fast": SearchConfig(
            name="fast",
            max_iterations=80,
            min_iterations=20,
            iterations_per_option=8,
            exploration=0.15,
            rollout_temperature=0.3,
            first_player_games=8,
            mulligan_samples=32,
        ),
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
"""From cheapest to strongest: ``fast`` screens deck variants, ``standard`` is the default,
``strong`` confirms. Measured costs and strength are in docs/AI.md."""
