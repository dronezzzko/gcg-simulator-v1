"""Report files: schema validity, every required item, consistency, reproducibility."""

from __future__ import annotations

import copy
import dataclasses
import json
from collections import Counter
from collections.abc import Callable
from pathlib import Path
from typing import Any

import jsonschema
import pytest

from gcg_sim.reports import ReportPaths, build_results, load_schema, validate_results, write_reports
from gcg_sim.reports.versions import conflict_resolutions
from gcg_sim.reports.writer import to_json_text
from gcg_sim.runner import BenchmarkRun, GameRecord, MatchRecord
from gcg_sim.runner.records import RunTiming
from gcg_sim.runner.replayfile import check_replay

RecordFactory = Callable[..., GameRecord]


def load(path: Path) -> dict[str, Any]:
    data: dict[str, Any] = json.loads(path.read_text(encoding="utf-8"))
    return data


def ndjson(path: Path) -> list[dict[str, Any]]:
    return [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines()]


def test_results_validate_against_the_versioned_schema(written: ReportPaths) -> None:
    results = load(written.results)
    validate_results(results)
    schema = load_schema()
    assert schema["$id"] == results["schema"] == "urn:gcg-sim:schema:results:1.0.0"
    assert results["schema_version"] == results["versions"]["results_schema"] == "1.0.0"


def test_every_report_file_is_written(written: ReportPaths) -> None:
    for path in (written.results, written.summary, written.games, written.timing):
        assert path.is_file()
        assert path.stat().st_size > 0
    assert written.replays
    assert all(p.is_file() for p in written.replays)
    leftovers = [p for p in written.out_dir.rglob("*") if p.name.endswith(".tmp")]
    assert leftovers == []


def test_versions_seeds_and_ai_settings(written: ReportPaths, run: BenchmarkRun) -> None:
    r = load(written.results)
    v = r["versions"]
    assert v["package"] == "0.1.0"
    assert v["dataset_version"].startswith("25-676f1bf")
    assert v["rules_version"] == "1.9.0"
    assert v["banlist_effective_date"] == "2026-09-25"
    assert r["seeds"]["master"] == 3
    assert r["seeds"]["prng"] == "SplitMix64"
    assert r["ai"] == {
        "factory": "random_agent.py:make_random_agent",
        "preset": "standard",
        "dut_agent": "test-random",
        "bench_agent": "test-random",
        "decision_log": False,
    }
    assert r["config"]["matches"] == 60
    assert r["config"]["format"] == "bo3"
    assert "workers" not in json.dumps(r["config"])


def test_outcomes_match_the_game_records(written: ReportPaths, run: BenchmarkRun) -> None:
    r = load(written.results)
    games = ndjson(written.games)
    assert len(games) == len(run.games)
    wins = sum(g["winner"] == "dut" for g in games)
    g = r["results"]["games"]
    assert (g["n"], g["dut_wins"]) == (len(games), wins)
    assert g["dut_wins"] + g["bench_wins"] + g["draws"] == g["n"]
    assert g["dut_win_rate"]["ci95"][0] <= g["dut_win_rate"]["rate"] <= g["dut_win_rate"]["ci95"][1]
    m = r["results"]["matches"]
    assert m["n"] == 60
    assert m["dut_wins"] == sum(x.winner == "dut" for x in run.matches)
    splits = r["splits"]
    assert splits["on_play"]["n"] + splits["on_draw"]["n"] == len(games)
    assert splits["on_play"]["n"] == sum(x["dut_on_play"] for x in games)
    by_number = splits["by_game_number"]
    assert by_number["1"]["n"] == 60
    assert sum(b["n"] for b in by_number.values()) == len(games)
    reasons = {e["reason"]: e["n"] for e in r["end_reasons"]}
    assert reasons == dict(Counter(x["end_reason"] for x in games))
    length = r["game_length"]
    assert length["turns"]["n"] == len(games)
    assert sum(h["games"] for h in length["histogram"]) == len(games)
    assert r["draws"]["games"] == g["draws"]


def test_per_card_statistics(written: ReportPaths, run: BenchmarkRun) -> None:
    r = load(written.results)
    games = ndjson(written.games)
    deck = Counter(run.config.deck_under_test.main)
    cards = {c["card_number"]: c for c in r["cards"]}
    assert set(cards) == set(deck)
    for number, c in cards.items():
        assert c["copies"] == deck[number]
        drawn_games = [x for x in games if number in x["dut_cards"]["drawn"]]
        assert c["drawn"]["games"] == len(drawn_games)
        assert c["drawn"]["copies"] == sum(x["dut_cards"]["drawn"].count(number) for x in games)
        assert c["win_rate_when_drawn"]["successes"] == sum(
            x["winner"] == "dut" for x in drawn_games
        )
        assert c["win_rate_when_drawn"]["n"] + c["win_rate_when_not_drawn"]["n"] == len(games)
        played = [p for x in games for p in x["dut_cards"]["played"] if p[0] == number]
        assert c["played"]["plays"] == len(played)
        assert sum(t["plays"] for t in c["played"]["by_own_turn"]) == len(played)
        assert c["held_never_played"]["copies"] <= c["drawn"]["copies"]
        initial = [x for x in games if number in x["dut_cards"]["initial_hand"]]
        assert c["mulligan"]["in_initial_hand_games"] == len(initial)
        assert c["mulligan"]["redrawn_games"] == sum(x["dut_cards"]["redrew"] for x in initial)
    assert any(c["played"]["games"] > 0 for c in cards.values())


def test_benchmark_cards_associated_with_losses(written: ReportPaths, run: BenchmarkRun) -> None:
    r = load(written.results)
    bench = r["benchmark_cards"]
    assert {b["card_number"] for b in bench} == set(run.config.benchmark_deck.main)
    eligible = [b for b in bench if b["rank_eligible"]]
    assert eligible
    assert bench[: len(eligible)] == eligible
    lifts = [b["loss_rate_lift"] for b in eligible]
    assert lifts == sorted(lifts, reverse=True)
    for b in bench:
        seen, unseen = b["dut_loss_rate_when_seen"], b["dut_loss_rate_when_not_seen"]
        assert seen["n"] == b["seen_games"]
        assert seen["n"] + unseen["n"] == len(run.games)


def test_hypotheses_are_labelled_with_sample_sizes(written: ReportPaths) -> None:
    r = load(written.results)
    assert r["hypothesis_thresholds"]["min_games"] >= 1
    for h in r["hypotheses"]:
        assert h["label"] == "hypothesis"
        assert h["sample_size"] >= r["hypothesis_thresholds"]["min_games"]
        assert "Hypothesis:" in h["statement"]


def test_conflict_resolutions_cover_deck_cards(written: ReportPaths, run: BenchmarkRun) -> None:
    r = load(written.results)
    dut = set(run.config.deck_under_test.main + run.config.deck_under_test.resources)
    bench = set(run.config.benchmark_deck.main + run.config.benchmark_deck.resources)
    entries = r["conflict_resolutions"]
    assert entries
    for e in entries:
        expected = {"deck_under_test"} if e["card_number"] in dut else set()
        expected |= {"benchmark"} if e["card_number"] in bench else set()
        assert set(e["decks"]) == expected
    r001 = [e for e in entries if e["card_number"] == "R-001"]
    assert r001
    assert r001[0]["decks"] == ["benchmark", "deck_under_test"]
    assert r001[0]["changes_card_data"] is True
    assert any(not e["changes_card_data"] for e in entries)


def test_replays_are_representative_and_verify(written: ReportPaths, run: BenchmarkRun) -> None:
    r = load(written.results)
    listed = r["replays"]
    assert {written.out_dir / e["file"] for e in listed} == set(written.replays)
    assert Counter(e["result"] for e in listed)["win"] == 2
    assert Counter(e["result"] for e in listed)["loss"] == 2
    for e in listed:
        doc = load(written.out_dir / e["file"])
        assert doc["label"] == e["result"]
        assert check_replay(doc).ok
    wins = sorted(g.turns for g in run.games if g.winner == "dut")
    median = wins[(len(wins) - 1) // 2]
    worst = max(abs(g.turns - median) for g in run.games if g.winner == "dut")
    chosen = [e for e in listed if e["result"] == "win"]
    assert all(abs(e["turns"] - median) <= worst for e in chosen)


def test_wall_clock_data_only_in_timing(written: ReportPaths, run: BenchmarkRun) -> None:
    timing = load(written.timing)
    assert {"started_at", "finished_at", "wall_seconds", "match_seconds"} <= set(timing)
    for path in (written.results, written.games, written.summary):
        text = path.read_text(encoding="utf-8")
        assert run.timing.started_at not in text
        assert "wall_seconds" not in text
    other_timing = RunTiming("2000-01-01T00:00:00+00:00", "2000-01-01T00:00:01+00:00", 1.0, (), 7)
    rerun = dataclasses.replace(run, timing=other_timing)
    assert to_json_text(build_results(rerun)) == written.results.read_text(encoding="utf-8")


def test_rewriting_removes_stale_replays_but_keeps_other_files(
    run: BenchmarkRun, tmp_path: Path
) -> None:
    (tmp_path / "replays").mkdir()
    stale = tmp_path / "replays" / "win-m9999-g1.json"
    stale.write_text("{}")
    mine = tmp_path / "replays" / "notes.txt"
    mine.write_text("keep me")
    paths = write_reports(run, tmp_path)
    assert not stale.exists()
    assert mine.read_text() == "keep me"
    assert len(paths.replays) == 4


def test_summary_lists_the_key_sections(written: ReportPaths, run: BenchmarkRun) -> None:
    text = written.summary.read_text(encoding="utf-8")
    for heading in (
        "## Result",
        "## Splits",
        "## How games end",
        "## Redraws",
        "## Deck-under-test cards",
        "## Benchmark cards most associated with DUT losses",
        "## Tuning hypotheses (to test, not conclusions)",
        "## Versions",
        "## Conflict resolutions affecting these decks",
        "## Replays",
    ):
        assert heading in text
    assert "Reproduce: `gcg-sim benchmark " in text
    assert str(run.config.deck_under_test.source) in text
    assert "--matches 60 --seed 3 --format bo3 --ai-preset standard --replays 2" in text


@pytest.mark.parametrize(
    "mutate",
    [
        lambda r: r.pop("cards"),
        lambda r: r["results"]["games"]["dut_win_rate"].update(rate=1.5),
        lambda r: r.update(extra=1),
        lambda r: r["hypotheses"].append(
            {
                "id": "x",
                "kind": "card_drawn_win_rate",
                "label": "fact",
                "statement": "s",
                "sample_size": 3,
                "evidence": {},
            }
        ),
        lambda r: r["config"].update(format="bo5"),
    ],
)
def test_schema_rejects_invalid_results(written: ReportPaths, mutate: Any) -> None:
    results = copy.deepcopy(load(written.results))
    mutate(results)
    with pytest.raises(jsonschema.ValidationError):
        validate_results(results)


def test_draws_and_turn_limits_are_counted_and_reported(
    run: BenchmarkRun, make_record: RecordFactory
) -> None:
    both, limit = "both_defeated", "turn_limit"
    matches = (
        MatchRecord(
            0,
            0,
            (
                make_record(0, 0, "draw", point="bench", reason=both),
                make_record(0, 1, "dut"),
                make_record(0, 2, "dut"),
            ),
            "dut",
            2,
            1,
        ),
        MatchRecord(
            1,
            1,
            (
                make_record(1, 0, "draw", reason=limit, turns=200),
                make_record(1, 1, "dut"),
                make_record(1, 2, "bench"),
            ),
            "draw",
            1,
            1,
        ),
    )
    config = dataclasses.replace(run.config, matches=2, replays=1)
    fake = dataclasses.replace(run, config=config, matches=matches)
    results = build_results(fake)
    validate_results(results)
    assert results["draws"] == {
        "games": 2,
        "both_defeated": 1,
        "turn_limit": 1,
        "scored_for_match_by_turn_player_rule": 1,
    }
    reasons = {e["reason"]: e for e in results["end_reasons"]}
    assert reasons[limit]["draws"] == 1
    assert reasons[both]["draws"] == 1
    assert results["results"]["matches"]["draws"] == 1
    assert results["results"]["games"]["draws"] == 2
    assert results["game_length"]["by_result"]["draw"]["max"] == 200
    assert [e["result"] for e in results["replays"]] == ["win", "loss", "draw"]


def test_resolutions_named_by_a_curated_conflict_are_listed_for_every_card_it_names() -> None:
    entries = conflict_resolutions({"deck_under_test": ["GD01-030"]})
    assert ("GD01-030", "ambiguous:breach-reminder") in {
        (e["card_number"], e["conflict_id"]) for e in entries
    }
