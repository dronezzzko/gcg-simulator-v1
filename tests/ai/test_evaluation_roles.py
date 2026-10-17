"""The ``roles`` feature set of the evaluation, its fitted weights and the tuning options that
fit it."""

from __future__ import annotations

import json
import math
from typing import Any

import pytest

from gcg_sim.ai import tuning
from gcg_sim.ai.compare import parse_agent
from gcg_sim.ai.config import PRESETS, SearchConfig
from gcg_sim.ai.evaluation import (
    FEATURES,
    HAND_WEIGHTS,
    ROLE_FEATURES,
    ROLE_WEIGHTS,
    TUNED_WEIGHTS,
    Weights,
    burst_density,
    evaluate,
    features,
    role_features,
    score,
)
from gcg_sim.ai.selfplay import AgentSpec
from gcg_sim.engine.state import GameState
from gcg_sim.testkit import Scenario

TINY = SearchConfig(
    "tiny",
    max_iterations=6,
    min_iterations=2,
    iterations_per_option=1,
    first_player_games=2,
    mulligan_samples=4,
)


def both_can_kill(*, defender_base: bool) -> GameState:
    """Turn 9, player 0 to move with two ready GMs (AP 2) and one Shield; player 1 has two
    rested Strike Daggers (AP 3) and a rested Demi Trainer (<Blocker>, AP 1), no Shields, and
    an EX Base when ``defender_base``."""
    sc = Scenario(turn=9)
    for p in (0, 1):
        sc.resources(p, 5)
    sc.add(0, "ST01-005")
    sc.add(0, "ST01-005")
    sc.shields(0, "ST01-005")
    sc.add(1, "ST04-005", rested=True)
    sc.add(1, "ST04-005", rested=True)
    sc.add(1, "ST01-008", rested=True)
    if defender_base:
        sc.base(1)
    return sc.start()


def test_role_features_keep_each_players_threats_apart() -> None:
    st = both_can_kill(defender_base=True)
    density = burst_density(st, 0)
    roles = dict(zip(ROLE_FEATURES, role_features(st, density), strict=True))
    base = dict(zip(FEATURES, features(st, 0, density), strict=True))
    assert all(roles[name] == base[name] for name in FEATURES if name != "can_kill")
    assert roles["can_kill_turn_player"] == 1.0 and roles["can_kill_other"] == 1.0
    assert base["can_kill"] == 0.0
    # Player 0: the Demi Trainer stops one GM; the EX Base (3 HP) takes ceil(3 / 2) hits, then
    # one hit on the player: 3 hits at one per turn. Player 1: 3 attackers, 1 Shield + 1 hit.
    assert roles["inv_race_turn_player"] == 1.0 / 3.0
    assert roles["inv_race_other"] == 1.0


def test_role_weights_value_the_turn_players_lethal_over_the_others() -> None:
    """Player 0 wins this turn: the opponent's lethal on board does not matter. The base
    features cancel the two threats, so the tuned weights rate it far lower."""
    st = both_can_kill(defender_base=False)
    density = burst_density(st, 0)
    assert evaluate(st, 0, ROLE_WEIGHTS, density) > 0.9
    assert evaluate(st, 0, TUNED_WEIGHTS, density) < 0.8


def test_role_score_is_from_the_turn_players_point_of_view() -> None:
    st = both_can_kill(defender_base=False)
    density = burst_density(st, 0)
    weights = Weights(tuple(0.1 * (i + 1) for i in range(len(ROLE_FEATURES))), "roles")
    z = sum(w * x for w, x in zip(weights.values, role_features(st, density), strict=True))
    assert score(st, 0, weights, density) == z
    assert score(st, 1, weights, density) == -z
    assert math.isclose(evaluate(st, 0, weights, density) + evaluate(st, 1, weights, density), 1)


def test_weights_check_their_feature_set() -> None:
    assert TUNED_WEIGHTS.feature_set == "base"
    assert Weights(TUNED_WEIGHTS.values) == TUNED_WEIGHTS
    assert ROLE_WEIGHTS.feature_set == "roles"
    assert list(ROLE_WEIGHTS.as_dict()) == list(ROLE_FEATURES)
    with pytest.raises(ValueError, match="expected 17 weights"):
        Weights(TUNED_WEIGHTS.values, "roles")
    with pytest.raises(ValueError, match="unknown feature set"):
        Weights(TUNED_WEIGHTS.values, "other")


def test_compare_selects_the_role_weights() -> None:
    spec = parse_agent("standard:weights=roles")
    assert spec == AgentSpec("mcts", config=PRESETS["standard"], weights=ROLE_WEIGHTS)


def test_tuning_fits_the_role_feature_set_reproducibly() -> None:
    first = tuning.tune(3, 1, 5, 1, TUNED_WEIGHTS, TINY, "roles")
    again = tuning.tune(3, 1, 5, 1, TUNED_WEIGHTS, TINY, "roles")
    assert first == again
    rnd = first["rounds"][0]
    assert rnd["heldout_logloss_before"] is None
    assert list(rnd["weights"]) == list(ROLE_FEATURES)
    assert len(first["weights"]) == len(ROLE_FEATURES)
    base = tuning.tune(3, 1, 5, 1, TUNED_WEIGHTS, TINY)
    assert base["positions"] == first["positions"]
    assert len(base["weights"]) == len(FEATURES)
    json.dumps(first)


def test_tuning_cli_selects_the_preset_start_weights_and_feature_set(
    monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
) -> None:
    calls: list[tuple[Any, ...]] = []

    def fake_tune(*args: Any) -> dict[str, Any]:
        calls.append(args)
        return {"weights": [0.0] * len(ROLE_FEATURES)}

    monkeypatch.setattr(tuning, "tune", fake_tune)
    tuning.main(["--preset", "standard", "--start", "tuned", "--feature-set", "roles"])
    assert calls[-1][4:] == (TUNED_WEIGHTS, PRESETS["standard"], "roles")
    assert json.loads(capsys.readouterr().out)["features"] == list(ROLE_FEATURES)
    tuning.main([])
    assert calls[-1][4:] == (HAND_WEIGHTS, tuning.TUNING_CONFIG, "base")
