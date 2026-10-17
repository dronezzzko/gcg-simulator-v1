"""The AI as released before the October 2026 quality work, kept for paired comparisons.

``make_agent("mcts", config=BASELINE_PRESETS["standard"], weights=BASELINE_WEIGHTS)`` plays
exactly as the ``standard`` preset did at commit bc38ab4: ``tests/ai/test_baseline.py`` replays
a game recorded on that commit, decision logs included. Every search setting added since then
defaults to the old behaviour, and each configuration spells out all of its fields so that a
later change of a default cannot move it.
"""

from __future__ import annotations

from collections.abc import Mapping
from types import MappingProxyType

from gcg_sim.ai.config import SearchConfig
from gcg_sim.ai.evaluation import TUNED_WEIGHTS, Weights

BASELINE_PRESETS: Mapping[str, SearchConfig] = MappingProxyType(
    {
        "standard": SearchConfig(
            name="standard",
            max_iterations=320,
            min_iterations=80,
            iterations_per_option=32,
            exploration=0.3,
            prior_weight=0.5,
            prior_temperature=1.5,
            horizon_turns=2,
            rollout_temperature=0.5,
            value_scale=2.0,
            max_rollout_decisions=250,
            first_player_games=24,
            mulligan_samples=96,
            policy_burst_yes_score=1.0,
            observation_keys=False,
        ),
        "strong": SearchConfig(
            name="strong",
            max_iterations=1280,
            min_iterations=320,
            iterations_per_option=128,
            exploration=0.15,
            prior_weight=0.5,
            prior_temperature=1.5,
            horizon_turns=2,
            rollout_temperature=0.3,
            value_scale=2.0,
            max_rollout_decisions=250,
            first_player_games=48,
            mulligan_samples=192,
            policy_burst_yes_score=1.0,
            observation_keys=False,
        ),
    }
)
BASELINE_WEIGHTS: Weights = TUNED_WEIGHTS
