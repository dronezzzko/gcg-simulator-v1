"""The throughput tool reports a speed without affecting results."""

from __future__ import annotations

import json

import pytest

from gcg_sim.ai.throughput import main, measure


@pytest.mark.slow
def test_measure_reports_speed_for_a_preset() -> None:
    report = measure("standard", games=2, workers=2, seed=3)
    assert report["games"] == 2
    assert report["games_per_core_minute"] > 0
    assert report["mean_turns"] > 0


@pytest.mark.slow
def test_cli_prints_json(capsys: pytest.CaptureFixture[str]) -> None:
    main(["--preset", "standard", "--games", "1"])
    assert json.loads(capsys.readouterr().out)["preset"] == "standard"
