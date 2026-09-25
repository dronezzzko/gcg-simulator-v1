"""The weight-tuning procedure: a small, reproducible re-run plus the regression it relies on."""

from __future__ import annotations

import json
import math
from collections import Counter

import pytest

from gcg_sim.ai.config import SearchConfig
from gcg_sim.ai.evaluation import (
    DEFAULT_WEIGHTS,
    FEATURES,
    HAND_WEIGHTS,
    N_FEATURES,
    TUNED_WEIGHTS,
    logistic,
)
from gcg_sim.ai.tuning import (
    TUNING_DECKS,
    _solve,
    fit_logistic,
    log_loss,
    main,
    tune,
    validate,
)
from gcg_sim.cards.model import MAIN_DECK_TYPES
from gcg_sim.effects.registry import get_registry
from gcg_sim.rng import SplitMix64

TINY = SearchConfig(
    "tiny",
    max_iterations=6,
    min_iterations=2,
    iterations_per_option=1,
    first_player_games=2,
    mulligan_samples=4,
)


def _synthetic(true_w: list[float], n: int, seed: int) -> list[tuple[tuple[float, ...], float]]:
    rng = SplitMix64(seed)
    out = []
    for _ in range(n):
        x = (1.0, *(rng.random() * 4.0 - 2.0 for _ in range(N_FEATURES - 1)))
        p = logistic(sum(a * b for a, b in zip(true_w, x, strict=True)))
        out.append((x, 1.0 if rng.random() < p else 0.0))
    return out


def test_fit_logistic_recovers_known_weights() -> None:
    true_w = [0.3 * ((-1) ** i) * (1 + i % 3) / 2 for i in range(N_FEATURES)]
    samples = _synthetic(true_w, 1500, seed=4)
    fitted = fit_logistic(samples, l2=0.1)
    assert all(abs(a - b) < 0.3 for a, b in zip(fitted.values, true_w, strict=True))
    assert log_loss(samples, fitted.values) < log_loss(samples, [0.0] * N_FEATURES)


def test_solve_linear_system() -> None:
    a = [[4.0, 1.0, 0.0], [1.0, 3.0, 1.0], [0.0, 1.0, 2.0]]
    x = _solve(a, [1.0, 2.0, 3.0])
    for row, rhs in zip(a, [1.0, 2.0, 3.0], strict=True):
        assert math.isclose(sum(c * v for c, v in zip(row, x, strict=True)), rhs, abs_tol=1e-9)


def test_weights_cover_every_feature() -> None:
    assert len(FEATURES) == N_FEATURES == len(HAND_WEIGHTS.values) == len(TUNED_WEIGHTS.values)
    assert DEFAULT_WEIGHTS == TUNED_WEIGHTS
    with pytest.raises(ValueError, match="expected"):
        type(HAND_WEIGHTS)((1.0,))


def test_tuning_decks_are_legal_and_implemented() -> None:
    R = get_registry()
    for deck in TUNING_DECKS:
        cards = [R.db[n] for n in deck.main]
        assert len(deck.main) == 50 and len(deck.resources) == 10
        assert max(Counter(deck.main).values()) <= 4
        assert all(c.card_type in MAIN_DECK_TYPES for c in cards)
        assert 1 <= len({c.color for c in cards}) <= 2
        assert all(R.cards[c.def_id].script is not None for c in cards)


def test_small_tuning_run_is_reproducible() -> None:
    first = tune(games=3, rounds=1, seed=5, workers=1, config=TINY)
    again = tune(games=3, rounds=1, seed=5, workers=1, config=TINY)
    assert first == again
    rnd = first["rounds"][0]
    assert rnd["positions"] > 0
    assert set(rnd["weights"]) == set(FEATURES)
    assert all(math.isfinite(w) for w in first["weights"])
    json.dumps(first)


def test_validation_scores_are_bounded() -> None:
    report = validate(4, 3, 1, TUNED_WEIGHTS, HAND_WEIGHTS, TINY)
    assert report["games"] == 4
    assert 0.0 <= report["tuned_rate"] <= 1.0


def test_cli_prints_a_json_report(capsys: pytest.CaptureFixture[str]) -> None:
    main(["--games", "2", "--rounds", "1", "--seed", "9"])
    report = json.loads(capsys.readouterr().out)
    assert report["features"] == list(FEATURES)
    assert len(report["weights"]) == N_FEATURES
