"""The fast preset: a screening budget that keeps every setting of the standard search."""

from __future__ import annotations

import dataclasses
from pathlib import Path

from gcg_sim.ai.config import PRESETS
from gcg_sim.deck import load_deck, validate_deck

ROOT = Path(__file__).resolve().parents[2]
WEAKENED = ROOT / "tests" / "data" / "decks" / "red-green-zeon-weakened.txt"
BUDGET_RATIO = 4.5
BUDGET_FIELDS = {
    "name",
    "max_iterations",
    "min_iterations",
    "iterations_per_option",
    "exploration",
    "rollout_temperature",
    "first_player_games",
    "mulligan_samples",
}


def test_fast_searches_a_fraction_of_the_standard_budget_at_every_width() -> None:
    fast, standard = PRESETS["fast"], PRESETS["standard"]
    assert all(standard.budget(n) >= BUDGET_RATIO * fast.budget(n) for n in range(1, 41))


def test_fast_differs_from_standard_only_in_budget_and_noise() -> None:
    fast, standard = PRESETS["fast"], PRESETS["standard"]
    differing = {
        f.name
        for f in dataclasses.fields(fast)
        if getattr(fast, f.name) != getattr(standard, f.name)
    }
    assert differing <= BUDGET_FIELDS


def test_weakened_variant_used_for_fidelity_is_legal() -> None:
    assert validate_deck(load_deck(WEAKENED)) == []
