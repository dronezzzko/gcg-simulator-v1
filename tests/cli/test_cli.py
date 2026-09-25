"""Every CLI command through main([...]), including invalid input and exit codes."""

from __future__ import annotations

import json
from pathlib import Path

import pytest

from gcg_sim.cli import main
from gcg_sim.reports import validate_results
from gcg_sim.runner import agents as agents_module

ROOT = Path(__file__).resolve().parents[2]
EXAMPLES = ROOT / "examples" / "decks"
FEDERATION = EXAMPLES / "blue-white-federation.txt"
ZEON = EXAMPLES / "red-green-zeon.txt"


def short_deck(tmp_path: Path) -> Path:
    lines = FEDERATION.read_text(encoding="utf-8").replace("3 ST01-005 GM", "2 ST01-005 GM")
    path = tmp_path / "short.txt"
    path.write_text(lines, encoding="utf-8")
    return path


# --- validate -----------------------------------------------------------------------------


def test_validate_legal_deck(capsys: pytest.CaptureFixture[str]) -> None:
    assert main(["validate", str(FEDERATION)]) == 0
    out = capsys.readouterr().out
    assert out.strip() == f"{FEDERATION}: legal (50 cards, 10 resources; Blue/White)"


def test_validate_illegal_deck(tmp_path: Path, capsys: pytest.CaptureFixture[str]) -> None:
    deck = short_deck(tmp_path)
    assert main(["validate", str(deck)]) == 1
    err = capsys.readouterr().err
    assert err.startswith(f"error: {deck}: 1 violation(s)")
    assert "[MAIN_SIZE] the deck has 49 cards; it must have exactly 50 (rule 6-1-1)" in err


def test_validate_reports_parse_errors(tmp_path: Path, capsys: pytest.CaptureFixture[str]) -> None:
    deck = tmp_path / "typo.txt"
    deck.write_text("4 GD01-008 Guntank\n4 GD01-9999\n", encoding="utf-8")
    assert main(["validate", str(deck)]) == 1
    err = capsys.readouterr().err
    assert "line 2: unknown card id 'GD01-9999' (not in the card data)" in err


def test_validate_missing_file(tmp_path: Path, capsys: pytest.CaptureFixture[str]) -> None:
    missing = tmp_path / "nope.txt"
    assert main(["validate", str(missing)]) == 1
    assert capsys.readouterr().err == (
        f"error: cannot read deck file {missing}: No such file or directory\n"
    )


# --- usage errors -------------------------------------------------------------------------


@pytest.mark.parametrize(
    "argv",
    [
        [],
        ["validate"],
        ["data"],
        ["frobnicate"],
        ["benchmark", str(FEDERATION), str(ZEON)],
        ["benchmark", str(FEDERATION), str(ZEON), "--matches", "0"],
        ["benchmark", str(FEDERATION), str(ZEON), "--matches", "x"],
        ["benchmark", str(FEDERATION), str(ZEON), "--matches", "2", "--format", "bo5"],
        ["benchmark", str(FEDERATION), str(ZEON), "--matches", "2", "--ai-preset", "max"],
        ["benchmark", str(FEDERATION), str(ZEON), "--matches", "2", "--workers", "0"],
        ["benchmark", str(FEDERATION), str(ZEON), "--matches", "2", "--seed", "-1"],
        ["benchmark", str(FEDERATION), str(ZEON), "--matches", "2", "--replays", "-1"],
    ],
)
def test_usage_errors_exit_2(argv: list[str], capsys: pytest.CaptureFixture[str]) -> None:
    assert main(argv) == 2
    assert "usage: gcg-sim" in capsys.readouterr().err


def test_help_exits_0(capsys: pytest.CaptureFixture[str]) -> None:
    assert main(["--help"]) == 0
    out = capsys.readouterr().out
    for command in ("benchmark", "validate", "data", "replay"):
        assert command in out


# --- data status --------------------------------------------------------------------------


def test_data_status_text(capsys: pytest.CaptureFixture[str]) -> None:
    assert main(["data", "status"]) == 0
    out = capsys.readouterr().out
    assert "card data: gcg-api dataset 25-676f1bf3b752718dab59923a855819aad85ec361" in out
    assert "built 2026-09-21T12:11:55.812Z" in out
    assert "1148 card numbers" in out
    assert "rules: Comprehensive Rules Ver. 1.9.0 (effective 2026-09-11)" in out
    assert "banned & restricted list: effective 2026-09-25 (1 banned, 1 restricted" in out
    assert "implementation coverage: " in out
    assert "conflicts: 291 resolutions in overrides.json" in out


def test_data_status_json(capsys: pytest.CaptureFixture[str]) -> None:
    assert main(["data", "status", "--json"]) == 0
    status = json.loads(capsys.readouterr().out)
    assert status["snapshot"]["dataset_version"].startswith("25-676f1bf")
    assert status["versions"]["rules_version"] == "1.9.0"
    assert status["banlist"]["banned"] == ["GD01-020"]
    coverage = status["coverage"]
    assert 0 < coverage["implemented"] <= coverage["card_numbers"] == 1148
    assert status["conflicts"]["resolutions"] == 291


# --- benchmark ----------------------------------------------------------------------------


@pytest.mark.usefixtures("random_agents")
def test_benchmark_writes_reports_and_prints_a_summary(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    out_dir = tmp_path / "out"
    argv = ["benchmark", str(FEDERATION), str(ZEON), "--matches", "6", "--out", str(out_dir)]
    assert main([*argv, "--seed", "9", "--replays", "1", "--decision-log"]) == 0
    captured = capsys.readouterr()
    lines = captured.out.splitlines()
    assert lines[0] == "blue-white-federation (deck under test) vs red-green-zeon (benchmark)"
    assert lines[1].startswith("matches: DUT ")
    assert lines[2].startswith("games: DUT ")
    assert f"  results  {out_dir / 'results.json'}" in lines
    assert "matches 6/6" in captured.err
    results = json.loads((out_dir / "results.json").read_text(encoding="utf-8"))
    validate_results(results)
    assert results["config"]["matches"] == 6
    assert results["ai"]["decision_log"] is True
    replay = json.loads((out_dir / results["replays"][0]["file"]).read_text(encoding="utf-8"))
    assert replay["decision_log"]


@pytest.mark.usefixtures("random_agents")
def test_benchmark_rejects_an_illegal_deck(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    out_dir = tmp_path / "out"
    argv = ["benchmark", str(short_deck(tmp_path)), str(ZEON), "--matches", "2"]
    assert main([*argv, "--out", str(out_dir)]) == 1
    assert "[MAIN_SIZE]" in capsys.readouterr().err
    assert not out_dir.exists()


def test_benchmark_without_the_ai_package_fails_clearly(
    tmp_path: Path, capsys: pytest.CaptureFixture[str], monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.delenv("GCG_SIM_AGENT_FACTORY", raising=False)
    monkeypatch.setattr(agents_module, "AI_MODULE", "gcg_sim._no_such_ai_module")
    out_dir = tmp_path / "out"
    argv = ["benchmark", str(FEDERATION), str(ZEON), "--matches", "1", "--out", str(out_dir)]
    assert main(argv) == 1
    assert capsys.readouterr().err == (
        "error: the AI package 'gcg_sim._no_such_ai_module' is not available; the benchmark "
        "needs the search AI\n"
    )
    assert not out_dir.exists()


# --- replay -------------------------------------------------------------------------------


@pytest.fixture
def replay_file(tmp_path: Path, random_agents: None) -> Path:
    out_dir = tmp_path / "bench"
    argv = ["benchmark", str(ZEON), str(FEDERATION), "--matches", "4", "--out", str(out_dir)]
    assert main(argv) == 0
    return sorted((out_dir / "replays").glob("*.json"))[0]


def test_replay_verifies_a_saved_game(
    replay_file: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    capsys.readouterr()
    assert main(["replay", str(replay_file)]) == 0
    out = capsys.readouterr().out.splitlines()
    assert out[0].startswith("seat 0: ")
    assert out[-1] == "replay matches the recorded outcome and final state"


def test_replay_log(replay_file: Path, capsys: pytest.CaptureFixture[str]) -> None:
    capsys.readouterr()
    assert main(["replay", "--log", str(replay_file)]) == 0
    out = capsys.readouterr().out.splitlines()
    assert out[0].startswith("T0 P")
    assert "choose_first" in out[0]
    assert any(" game over: winner P" in line for line in out)


def test_replay_detects_a_changed_outcome(
    replay_file: Path, tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    doc = json.loads(replay_file.read_text(encoding="utf-8"))
    doc["expected"]["turns"] += 5
    tampered = tmp_path / "tampered.json"
    tampered.write_text(json.dumps(doc), encoding="utf-8")
    capsys.readouterr()
    assert main(["replay", str(tampered)]) == 1
    err = capsys.readouterr().err
    assert err.startswith("error: replay does NOT match the recorded outcome:")
    assert "turns: expected" in err


@pytest.mark.parametrize(
    ("content", "message"),
    [
        ("{not json", "is not valid JSON"),
        ("[1, 2]", "is not a replay document"),
        ('{"format": "other"}', "not a gcg-sim-replay v1 document"),
    ],
)
def test_replay_rejects_invalid_files(
    tmp_path: Path, capsys: pytest.CaptureFixture[str], content: str, message: str
) -> None:
    path = tmp_path / "bad.json"
    path.write_text(content, encoding="utf-8")
    assert main(["replay", str(path)]) == 1
    assert message in capsys.readouterr().err


def test_replay_missing_file(tmp_path: Path, capsys: pytest.CaptureFixture[str]) -> None:
    assert main(["replay", str(tmp_path / "none.json")]) == 1
    assert "cannot read replay file" in capsys.readouterr().err
