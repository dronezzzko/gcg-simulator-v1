"""Tuning signals are generated only from sufficient samples and are labelled as hypotheses."""

from __future__ import annotations

import dataclasses
from collections.abc import Callable
from typing import Any

from gcg_sim.reports import build_results, validate_results
from gcg_sim.reports.hypotheses import MIN_GAMES
from gcg_sim.runner import BenchmarkRun, GameRecord, MatchRecord

RecordFactory = Callable[..., GameRecord]
GOOD, DEAD, THREAT = "GD01-004", "GD01-099", "GD01-030"


def results_for(run: BenchmarkRun, games: list[GameRecord]) -> dict[str, Any]:
    matches = tuple(
        MatchRecord(
            g.match_index,
            g.dut_seat,
            (g,),
            g.winner,
            int(g.winner == "dut"),
            int(g.winner == "bench"),
        )
        for g in games
    )
    config = dataclasses.replace(run.config, matches=len(games), fmt="bo1")
    results = build_results(dataclasses.replace(run, config=config, matches=matches))
    validate_results(results)
    return results


def by_kind(results: dict[str, Any]) -> dict[str, list[dict[str, Any]]]:
    out: dict[str, list[dict[str, Any]]] = {}
    for h in results["hypotheses"]:
        out.setdefault(h["kind"], []).append(h)
    return out


def games_with(
    make_record: RecordFactory, n: int, wins: int, start: int, **kwargs: Any
) -> list[GameRecord]:
    return [make_record(start + i, 0, "dut" if i < wins else "bench", **kwargs) for i in range(n)]


def test_card_drawn_win_rate_signal(run: BenchmarkRun, make_record: RecordFactory) -> None:
    drawn = games_with(make_record, 30, 21, 0, drawn=(GOOD,), played=(GOOD,))
    not_drawn = games_with(make_record, 30, 9, 30)
    results = results_for(run, drawn + not_drawn)
    (h,) = [h for h in by_kind(results)["card_drawn_win_rate"] if GOOD in h["id"]]
    assert h["label"] == "hypothesis"
    assert h["sample_size"] == 60
    assert h["evidence"]["delta"] == 0.4
    assert h["evidence"]["ci95_separated"] is True
    assert h["statement"].startswith("Games where Guncannon (GD01-004) was drawn were won 70%")
    assert "Hypothesis: more copies" in h["statement"]


def test_dead_card_signal(run: BenchmarkRun, make_record: RecordFactory) -> None:
    games = games_with(make_record, MIN_GAMES, 10, 0, drawn=(DEAD,))
    (h,) = by_kind(results_for(run, games))["held_never_played"]
    assert h["id"] == f"dead-card:{DEAD}"
    assert h["sample_size"] == MIN_GAMES
    assert h["evidence"]["rate_of_drawn_copies"] == 1.0


def test_play_draw_and_redraw_signals(run: BenchmarkRun, make_record: RecordFactory) -> None:
    on_play = games_with(make_record, 25, 20, 0, dut_on_play=True, redrew=False)
    on_draw = games_with(make_record, 25, 5, 25, dut_on_play=False, redrew=True)
    kinds = by_kind(results_for(run, on_play + on_draw))
    (split,) = kinds["play_draw_split"]
    assert split["sample_size"] == 50
    assert "built for going first" in split["statement"]
    (redraw,) = kinds["redraw_outcome"]
    assert redraw["evidence"]["delta"] == -0.6


def test_benchmark_threat_signal(run: BenchmarkRun, make_record: RecordFactory) -> None:
    seen = games_with(make_record, 25, 5, 0, bench_seen=(THREAT,))
    unseen = games_with(make_record, 25, 15, 25)
    results = results_for(run, seen + unseen)
    (h,) = by_kind(results)["benchmark_card_threat"]
    assert h["id"] == f"bench-threat:{THREAT}"
    assert h["evidence"]["delta"] == 0.4
    assert results["benchmark_cards"][0]["card_number"] == THREAT


def test_small_samples_produce_no_hypotheses(run: BenchmarkRun, make_record: RecordFactory) -> None:
    games = games_with(make_record, 8, 8, 0, drawn=(GOOD, DEAD), bench_seen=(THREAT,))
    games += games_with(make_record, 8, 0, 8)
    assert results_for(run, games)["hypotheses"] == []


def test_a_difference_within_sampling_noise_is_not_a_hypothesis(
    run: BenchmarkRun, make_record: RecordFactory
) -> None:
    drawn = games_with(make_record, 30, 17, 0, drawn=(GOOD,), played=(GOOD,))
    not_drawn = games_with(make_record, 30, 14, 30)
    results = results_for(run, drawn + not_drawn)
    assert "card_drawn_win_rate" not in by_kind(results)
    assert results["hypothesis_thresholds"]["multiple_comparisons"] == "holm"


def test_signals_carry_their_test_and_family_size(
    run: BenchmarkRun, make_record: RecordFactory
) -> None:
    drawn = games_with(make_record, 30, 21, 0, drawn=(GOOD,), played=(GOOD,))
    not_drawn = games_with(make_record, 30, 9, 30)
    (h,) = by_kind(results_for(run, drawn + not_drawn))["card_drawn_win_rate"]
    assert h["evidence"]["p_value"] < 0.05
    assert h["evidence"]["comparisons"] == 1
