"""``results.json``: the reproducible benchmark result, validated by the packaged JSON Schema.

The content depends only on the configuration (decks, seed, match count, format, AI preset)
and the code and data versions: never on wall-clock time, the output path, or ``workers``.
"""

from __future__ import annotations

import json
from collections import Counter
from collections.abc import Sequence
from functools import cache
from importlib import resources
from typing import Any

import jsonschema

from gcg_sim.cards.db import get_card_db
from gcg_sim.deck.model import Deck
from gcg_sim.deck.parse import deck_digest
from gcg_sim.reports import aggregate as agg
from gcg_sim.reports.hypotheses import MIN_GAMES, hypotheses
from gcg_sim.reports.versions import SCHEMA_ID, SCHEMA_VERSION, conflict_resolutions, versions
from gcg_sim.runner.records import DRAW, BenchmarkRun, GameRecord

SCHEMA_FILE = "results.v1.schema.json"
DRAW_REPLAYS = 1
NOTES = (
    "Win rates count draws as non-wins; n is every game (or match). Intervals are 95% Wilson "
    "score intervals.",
    "Both seats are played by the same AI (see 'ai'); results measure the decks under that AI, "
    "not under human play.",
    "Per-card and benchmark-card signals are correlations within these games; the "
    "'hypotheses' list suggests what to test next and is not a conclusion.",
    "A simultaneous defeat (both_defeated) is a game draw; in BO3 match scoring the turn player "
    "at the end loses it (TRM 5.2). turn_limit games are engine safety-cap draws and are "
    "always counted and reported.",
)


@cache
def load_schema() -> dict[str, Any]:
    text = resources.files("gcg_sim.reports").joinpath("schema", SCHEMA_FILE).read_text("utf-8")
    schema: dict[str, Any] = json.loads(text)
    return schema


def validate_results(obj: dict[str, Any]) -> None:
    """Raise ``jsonschema.ValidationError`` unless ``obj`` matches the results schema."""
    jsonschema.validate(obj, load_schema(), format_checker=jsonschema.FormatChecker())


def _deck_block(deck: Deck) -> dict[str, Any]:
    db = get_card_db()

    def listing(cards: tuple[str, ...]) -> list[dict[str, Any]]:
        counts = Counter(cards)
        return [{"card_number": n, "name": db[n].name, "count": counts[n]} for n in sorted(counts)]

    colors = sorted({c.value for n in deck.main if (c := db[n].color) is not None})
    return {
        "name": deck.name,
        "digest": deck_digest(deck),
        "main_count": len(deck.main),
        "resource_count": len(deck.resources),
        "colors": colors,
        "main": listing(deck.main),
        "resources": listing(deck.resources),
    }


def _pick(games: Sequence[GameRecord], k: int) -> list[GameRecord]:
    """``k`` games closest to the median length (ties by match/game order)."""
    if not games or k <= 0:
        return []
    turns = sorted(g.turns for g in games)
    median = turns[(len(turns) - 1) // 2]
    ranked = sorted(games, key=lambda g: (abs(g.turns - median), g.match_index, g.game_index))
    return sorted(ranked[:k], key=lambda g: (g.match_index, g.game_index))


def replay_name(label: str, g: GameRecord) -> str:
    return f"replays/{label}-m{g.match_index:04d}-g{g.game_index + 1}.json"


def select_replays(run: BenchmarkRun) -> list[tuple[str, GameRecord]]:
    """Representative games to save: wins and losses near their median length, plus a draw."""
    k = run.config.replays
    by_result = {r: [g for g in run.games if agg.dut_result(g) == r] for r in agg.RESULTS}
    counts = {agg.WIN: k, agg.LOSS: k, DRAW: min(k, DRAW_REPLAYS)}
    return [(r, g) for r in agg.RESULTS for g in _pick(by_result[r], counts[r])]


def _replay_entries(run: BenchmarkRun) -> list[dict[str, Any]]:
    return [
        {
            "file": replay_name(label, g),
            "result": label,
            "match_index": g.match_index,
            "game_number": g.game_index + 1,
            "turns": g.turns,
            "end_reason": g.end_reason,
            "seed": g.seed,
        }
        for label, g in select_replays(run)
    ]


def _config_block(run: BenchmarkRun) -> dict[str, Any]:
    c = run.config
    return {
        "matches": c.matches,
        "format": c.fmt,
        "max_games_per_match": c.games_per_match,
        "wins_needed": c.wins_needed,
        "turn_limit": c.turn_limit,
        "max_actions": c.max_actions,
        "replays_per_result": c.replays,
    }


def _ai_block(run: BenchmarkRun) -> dict[str, Any]:
    first = run.games[0]
    return {
        "factory": run.agent_factory,
        "preset": run.config.ai_preset,
        "dut_agent": first.agents[first.dut_seat],
        "bench_agent": first.agents[first.bench_seat],
        "decision_log": run.config.decision_log,
    }


def _seeds_block(run: BenchmarkRun) -> dict[str, Any]:
    return {
        "master": run.config.seed,
        "prng": "SplitMix64",
        "game_seed": "derive_seed(master, 'match', m, 'game', g)",
        "agent_seed": "derive_seed(master, 'match', m, 'game', g, 'agent', seat)",
        "dut_seat": "m % 2",
        "game_one_chooser": "seeded die roll inside new_game (rule 6-2-1-4)",
        "later_game_chooser": "loser of the previous game (BO3 Match Rules, TRM 4.6)",
    }


def build_results(run: BenchmarkRun) -> dict[str, Any]:
    """The content of ``results.json`` (byte-stable for a given configuration and seed)."""
    games = run.games
    config = run.config
    cards = agg.card_stats(games, config.deck_under_test)
    bench_cards = agg.bench_card_stats(games, config.benchmark_deck, MIN_GAMES)
    split = agg.splits(games, config.games_per_match)
    mull = agg.mulligan(games)
    return {
        "schema": SCHEMA_ID,
        "schema_version": SCHEMA_VERSION,
        "versions": versions(),
        "config": _config_block(run),
        "seeds": _seeds_block(run),
        "ai": _ai_block(run),
        "decks": {
            "deck_under_test": _deck_block(config.deck_under_test),
            "benchmark": _deck_block(config.benchmark_deck),
        },
        "results": {
            "matches": agg.match_outcomes(run.matches),
            "games": agg.game_outcomes(games),
        },
        "splits": split,
        "game_length": agg.game_length(games),
        "end_reasons": agg.end_reasons(games),
        "draws": agg.draws(games),
        "mulligan": mull,
        "cards": cards,
        "benchmark_cards": bench_cards,
        "hypotheses": hypotheses(cards, bench_cards, split, mull),
        "hypothesis_thresholds": {"min_games": MIN_GAMES},
        "conflict_resolutions": conflict_resolutions(
            {
                "deck_under_test": config.deck_under_test.main + config.deck_under_test.resources,
                "benchmark": config.benchmark_deck.main + config.benchmark_deck.resources,
            }
        ),
        "replays": _replay_entries(run),
        "notes": list(NOTES),
    }
