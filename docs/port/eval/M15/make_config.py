"""M15: write a `gcg-eval preset-benchmark --search-config` file (every field spelled out).

The base settings are the shipped presets' (crates/gcg-search/src/presets.rs at e14c839):
`standard` = reference `strong` (`ai/config.py:98-109`), `fast` = reference `standard`
(`ai/config.py:93-97`), `ROLE_WEIGHTS` ("roles"), every mechanism off except
`engine_error_leaf`. The budget fields are scaled as `gcg-eval budget-curve` scales them
(`max(1, round(value * scale))`, rounding half away from zero), and `field=value` overrides set
mechanism flags.

Usage: uv run python docs/port/eval/M15/make_config.py PRESET SCALE OUT [field=value ...]
"""

from __future__ import annotations

import json
import math
import sys
from pathlib import Path

COMMON = {
    "prior_weight": 0.5,
    "prior_temperature": 1.5,
    "horizon_turns": 2,
    "value_scale": 1.0,
    "max_rollout_decisions": 250,
    "policy_burst_yes_score": 4.0,
    "noop_branch_choices": True,
    "noop_resource_aware": True,
    "strict_attack_prune": True,
    "prune_noop_rest_targets": True,
    "merge_equivalent_moves": True,
    "observation_keys": True,
    "target_extension": 3.0,
    "target_extension_z": 2.0,
    "policy_rule_polarity_table": True,
    "policy_zero_ap_face": True,
}

PRESETS = {
    "standard": {
        "max_iterations": 1280,
        "min_iterations": 320,
        "iterations_per_option": 128,
        "exploration": 0.15,
        "rollout_temperature": 0.3,
        "first_player_games": 48,
        "mulligan_samples": 192,
    },
    "fast": {
        "max_iterations": 640,
        "min_iterations": 160,
        "iterations_per_option": 64,
        "exploration": 0.3,
        "rollout_temperature": 0.5,
        "first_player_games": 24,
        "mulligan_samples": 96,
    },
}

ORDER = [
    "name",
    "max_iterations",
    "min_iterations",
    "iterations_per_option",
    "exploration",
    "prior_weight",
    "prior_temperature",
    "horizon_turns",
    "rollout_temperature",
    "value_scale",
    "max_rollout_decisions",
    "first_player_games",
    "mulligan_samples",
    "policy_burst_yes_score",
    "noop_branch_choices",
    "noop_resource_aware",
    "strict_attack_prune",
    "prune_noop_rest_targets",
    "merge_equivalent_moves",
    "observation_keys",
    "target_extension",
    "target_extension_z",
    "policy_rule_polarity_table",
    "policy_zero_ap_face",
]

FLAGS_OFF: dict[str, object] = {
    "adaptive_horizon": False,
    "generalized_leaf": False,
    "capped_rollout_continuation": False,
    "tactical_solver": False,
    "lethal_root": False,
    "threat_leaf": False,
    "critical_extension": False,
    "effective_blockers": False,
    "solver_can_kill": False,
    "opponent_buckets": None,
    "belief_worlds": None,
    "belief_last_n": None,
    "exact_pools": False,
    "opponent_view_share": 0.0,
    "opponent_list": "known",
    "root_trees": None,
    "visited_set_backup": False,
    "in_tree_filters": False,
    "transpositions": False,
    "subtree_reuse": False,
    "macro_multi_pick": False,
    "prior_worlds": None,
    "ex_variants": False,
    "typed_fingerprint": False,
    "prune_worlds": None,
    "branch_all_selects": False,
    "rest_prune_worlds": False,
    "target_rollouts": False,
    "real_idle_line": False,
    "calibrated_policy": False,
    "modal_lookahead": False,
    "yes_no_lookahead": False,
    "end_phase_actions": False,
    "burst_decline_list": False,
    "order_trigger_scorer": False,
    "exhaustive_polarity": False,
    "exact_fight": False,
    "opponent_knobs": False,
    "kill_aware_targeting": False,
    "refit_weights": False,
    "phase_weights": False,
    "keyword_features": False,
    "ability_count_features": False,
    "hand_slots": False,
    "burst_shield_probability": False,
    "no_tempo_feature": False,
    "determinized_fit": False,
    "leaf_density": False,
    "exclude_capped_play_draw": False,
    "paired_mulligan": False,
    "portable_math": False,
    "engine_error_leaf": True,
    "target_multi_rollouts": None,
    "tree_lethal_select": False,
}

BUDGET_FIELDS = ("iterations_per_option", "min_iterations", "max_iterations")


def scaled(value: int, scale: float) -> int:
    return max(1, math.floor(value * scale + 0.5))


def parse_flag(text: str) -> object:
    if text in ("true", "false"):
        return text == "true"
    if text == "none":
        return None
    if text in ("known", "archetype"):
        return text
    return float(text) if "." in text else int(text)


def config(preset: str, scale: float, overrides: list[str]) -> dict:
    fields = {"name": preset, **COMMON, **PRESETS[preset]}
    for f in BUDGET_FIELDS:
        fields[f] = scaled(fields[f], scale)
    flags = dict(FLAGS_OFF)
    for item in overrides:
        name, _, text = item.partition("=")
        if name not in flags:
            raise SystemExit(f"unknown mechanism {name!r}")
        flags[name] = parse_flag(text)
    doc = {k: fields[k] for k in ORDER}
    doc["weights"] = "roles"
    doc["mechanisms"] = flags
    return doc


def main() -> None:
    preset, scale, out, *overrides = sys.argv[1:]
    doc = config(preset, float(scale), overrides)
    Path(out).write_text(json.dumps(doc, indent=1) + "\n")


if __name__ == "__main__":
    main()
