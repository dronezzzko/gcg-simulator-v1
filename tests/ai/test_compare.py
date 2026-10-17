"""Paired comparisons of AI versions: agent specs, deck sets, mirrored specs, determinism."""

from __future__ import annotations

import dataclasses
import json
from pathlib import Path

import pytest
from tests.ai.decks import DECKS

from gcg_sim.ai import make_agent
from gcg_sim.ai.agents import GreedyAgent, MctsAgent
from gcg_sim.ai.baseline import BASELINE_PRESETS, BASELINE_WEIGHTS
from gcg_sim.ai.compare import comparison_specs, load_decks, main, parse_agent, summarize
from gcg_sim.ai.config import PRESETS, SearchConfig
from gcg_sim.ai.evaluation import HAND_WEIGHTS
from gcg_sim.ai.selfplay import AgentSpec, mirrored_specs, play_all
from gcg_sim.ai.tuning import TUNING_DECKS

TINY = SearchConfig(
    "tiny",
    max_iterations=4,
    min_iterations=2,
    iterations_per_option=1,
    first_player_games=2,
    mulligan_samples=4,
)


def test_make_agent_accepts_a_config_and_weights() -> None:
    agent = make_agent("mcts", seed=3, config=TINY, weights=HAND_WEIGHTS)
    assert isinstance(agent, MctsAgent)
    assert agent.config == TINY and agent.weights == HAND_WEIGHTS
    assert agent.name == "mcts-tiny"
    greedy = make_agent("greedy", weights=HAND_WEIGHTS)
    assert isinstance(greedy, GreedyAgent) and greedy.weights == HAND_WEIGHTS


def test_parse_agent_presets_baselines_and_overrides() -> None:
    assert parse_agent("random") == AgentSpec("random")
    assert parse_agent("standard") == AgentSpec("mcts", config=PRESETS["standard"])
    assert parse_agent("fast") == AgentSpec("mcts", config=PRESETS["fast"])
    assert parse_agent("baseline-standard") == AgentSpec(
        "mcts", config=BASELINE_PRESETS["standard"], weights=BASELINE_WEIGHTS
    )
    spec = parse_agent("baseline-standard:exploration=0.2,horizon_turns=3,weights=hand")
    assert spec.config == dataclasses.replace(
        BASELINE_PRESETS["standard"], exploration=0.2, horizon_turns=3
    )
    assert spec.weights == HAND_WEIGHTS
    assert parse_agent("greedy:weights=hand") == AgentSpec("greedy", weights=HAND_WEIGHTS)


@pytest.mark.parametrize(
    ("text", "message"),
    [
        ("minimax", "unknown agent"),
        ("standard:nonsense=1", "no search setting"),
        ("greedy:exploration=1", "no search setting"),
        ("random:weights=hand", "weights must be one of"),
        ("standard:weights=other", "weights must be one of"),
        ("standard:exploration", "expected field=value"),
    ],
)
def test_parse_agent_rejects_bad_specs(text: str, message: str) -> None:
    with pytest.raises(ValueError, match=message):
        parse_agent(text)


def test_deck_sets() -> None:
    assert load_decks("tests") == DECKS
    assert load_decks("tuning") == TUNING_DECKS
    assert len(load_decks("examples")) == 3


def test_specs_cover_every_pairing_in_mirrored_blocks() -> None:
    a, b = AgentSpec("random"), AgentSpec("greedy")
    assert comparison_specs(8, 5, a, b, DECKS) == mirrored_specs(8, 5, a, b, DECKS)
    specs = comparison_specs(24, 5, a, b, TUNING_DECKS)
    assert [s.index for s in specs] == list(range(24))
    assert len({frozenset(s.decks) for s in specs}) == 3
    with pytest.raises(ValueError, match="multiple of 12"):
        comparison_specs(8, 5, a, b, TUNING_DECKS)


def test_tiny_comparison_is_the_same_for_any_worker_count() -> None:
    tiny = AgentSpec("mcts", config=TINY)
    specs = comparison_specs(4, 11, tiny, AgentSpec("random"), DECKS)
    serial = play_all(specs, 1)
    assert play_all(specs, 2) == serial
    result = summarize(specs, serial)
    assert result["n"] == 4
    assert result["by_challenger_seat"]["0"]["n"] == 2
    assert sum(r["n"] for r in result["by_challenger_deck"].values()) == 4


def test_cli_writes_every_game(capsys: pytest.CaptureFixture[str], tmp_path: Path) -> None:
    out = tmp_path / "ab.json"
    main(["random", "random", "--games", "4", "--seed", "2", "--out", str(out)])
    printed = json.loads(capsys.readouterr().out)
    saved = json.loads(out.read_text(encoding="utf-8"))
    assert printed["result"] == saved["result"]
    assert len(saved["outcomes"]) == 4
