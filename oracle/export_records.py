"""Export Python benchmark records and the exact report bytes they produce, for the Rust report
tests of gcg-runner (docs/dev/PLAN.md §8.1 `export_records.py`, §9.5, §10.4 E).

Writes crates/gcg-runner/tests/vectors/records/:

- ``index.json``: the run ids in order, each with its kind.
- ``<run>/run.json``: the ``BenchmarkRun`` inputs: the config (decks as sorted card counts plus
  their digest), the matches (``MatchRecord.to_json``), the timing values, the factory string,
  the host strings of ``timing.json``, and the report data that gcg-data provides in Rust
  (``versions()``, ``conflict_resolutions``, the card facts of every card in both decks).
- ``<run>/games.ndjson``, ``results.json``, ``summary.md``: Python's bytes. Benchmark runs also
  have ``timing.json`` and ``replays/*.json``, written by ``write_reports``; synthetic runs (the
  hand-built records of tests/reports) have no replays, because their actions are not games.
- ``stats.json``: Wilson intervals, z-tests, Holm, distributions, ``shlex.quote``, the
  stale-replay regex and its Unicode ``\\d`` ranges, the schema patterns, the ``date`` format, and
  schema mutation cases with jsonschema's verdict.

Benchmark runs are real: the random test agent (tests/runner/random_agent.py) and the reference
``fast`` preset. Their wall-clock timing is replaced by fixed values before the reports are
written, so every file, ``timing.json`` included, is a deterministic function of this script.

    uv run python oracle/export_records.py [--check]
"""

from __future__ import annotations

import argparse
import copy
import dataclasses
import json
import os
import platform
import re
import shlex
import struct
import sys
import tempfile
from collections import Counter
from pathlib import Path
from typing import Any

import jsonschema

from gcg_sim.cards.db import get_card_db
from gcg_sim.deck import Deck, load_deck
from gcg_sim.deck.parse import deck_digest, parse_deck
from gcg_sim.reports import build_results, load_schema, validate_results, write_reports
from gcg_sim.reports.stats import distribution, holm, rate, two_proportion_p, wilson
from gcg_sim.reports.summary import render_summary
from gcg_sim.reports.versions import conflict_resolutions, versions
from gcg_sim.reports.writer import REPLAY_FILE, _games_text, reproduce_command, to_json_text
from gcg_sim.rng import SplitMix64
from gcg_sim.runner import (
    BenchmarkConfig,
    BenchmarkRun,
    FactoryRef,
    GameRecord,
    MatchRecord,
    run_benchmark,
)
from gcg_sim.runner.agents import AgentFactory
from gcg_sim.runner.records import DutCardEvents, PlayEvent, RunTiming

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "crates" / "gcg-runner" / "tests" / "vectors" / "records"
EXAMPLES = Path("examples") / "decks"
RANDOM_AGENT = f"{ROOT / 'tests' / 'runner' / 'random_agent.py'}:make_random_agent"
GOOD, DEAD, THREAT = "GD01-004", "GD01-099", "GD01-030"


@dataclasses.dataclass(frozen=True)
class Spec:
    """One real benchmark run."""

    id: str
    dut: str
    bench: str
    matches: int
    seed: int
    fmt: str
    preset: str
    agent: str
    decision_log: bool = False
    replays: int = 3
    turn_limit: int = 200


SPECS = (
    Spec(
        "random-bo3-s3",
        "blue-white-federation",
        "red-green-zeon",
        60,
        3,
        "bo3",
        "standard",
        "random",
        replays=2,
    ),
    Spec(
        "random-bo1-s11",
        "red-green-zeon",
        "purple-white-tekkadan",
        40,
        11,
        "bo1",
        "strong",
        "random",
    ),
    Spec(
        "random-bo3-tekkadan",
        "purple-white-tekkadan",
        "blue-white-federation",
        12,
        2**64 - 1,
        "bo3",
        "fast",
        "random",
        replays=1,
    ),
    Spec(
        "random-bo1-log",
        "blue-white-federation",
        "purple-white-tekkadan",
        1,
        5,
        "bo1",
        "fast",
        "random",
        decision_log=True,
        replays=1,
    ),
    Spec(
        "random-bo3-turn-limit",
        "red-green-zeon",
        "blue-white-federation",
        6,
        21,
        "bo3",
        "standard",
        "random",
        turn_limit=9,
    ),
    Spec(
        "random-bo1-no-replays",
        "purple-white-tekkadan",
        "red-green-zeon",
        5,
        8,
        "bo1",
        "standard",
        "random",
        replays=0,
    ),
    Spec(
        "random-bo3-quoted-path",
        "zeon's deck",
        "blue-white-federation",
        3,
        42,
        "bo3",
        "strong",
        "random",
    ),
    Spec("fast-bo1-s3", "red-green-zeon", "blue-white-federation", 1, 3, "bo1", "fast", "mcts"),
    Spec(
        "fast-bo3-log",
        "blue-white-federation",
        "red-green-zeon",
        1,
        7,
        "bo3",
        "fast",
        "mcts",
        decision_log=True,
        replays=1,
    ),
    Spec(
        "fast-bo3-s9",
        "purple-white-tekkadan",
        "red-green-zeon",
        2,
        9,
        "bo3",
        "fast",
        "mcts",
        replays=1,
    ),
)


def bits(x: float) -> str:
    return struct.pack(">d", x).hex()


def dumps(data: Any) -> str:
    return json.dumps(data, indent=1, sort_keys=True, ensure_ascii=False) + "\n"


def load_spec_deck(name: str) -> Deck:
    """An example deck, or ``zeon's deck``: red-green-zeon's text under a name and a source
    with a space and a quote, which the reproduce line must shell-quote. No file is read from
    that source; ``load_deck`` would give the same ``Deck`` for a file at that path."""
    if name == "zeon's deck":
        text = (ROOT / EXAMPLES / "red-green-zeon.txt").read_text(encoding="utf-8-sig")
        source = "crates/gcg-runner/tests/vectors/records/decks/zeon's deck.txt"
        return parse_deck(text, name=name, source=source)
    return load_deck(EXAMPLES / f"{name}.txt")


def counts(cards: tuple[str, ...]) -> list[list[Any]]:
    return [[n, c] for n, c in sorted(Counter(cards).items())]


def deck_json(deck: Deck) -> dict[str, Any]:
    return {
        "name": deck.name,
        "source": deck.source,
        "main": counts(deck.main),
        "resources": counts(deck.resources),
        "digest": deck_digest(deck),
    }


def card_facts(decks: tuple[Deck, Deck]) -> dict[str, Any]:
    db = get_card_db()
    numbers = sorted({n for d in decks for n in d.main + d.resources})
    out: dict[str, Any] = {}
    for n in numbers:
        c = db[n]
        out[n] = {
            "name": c.name,
            "card_type": c.card_type.value,
            "level": c.level,
            "cost": c.cost,
            "color": None if c.color is None else c.color.value,
        }
    return out


def fixed_timing(run: BenchmarkRun, k: int) -> RunTiming:
    """Deterministic stand-ins for the wall-clock values, with rounding ties and a zero wall
    time (``games_per_second`` null) among them."""
    seconds = tuple(
        0.0005 + 0.001 * i + 0.25 * k + (i % 3) * 1.2345 for i in range(len(run.matches))
    )
    wall = 0.0 if k == 5 else 2.675 + 1.0005 * k
    return RunTiming(
        started_at=f"2026-11-01T07:{k:02d}:50+00:00",
        finished_at=f"2026-11-01T08:{k:02d}:54+00:00",
        wall_seconds=wall,
        match_seconds=seconds,
        workers=1 + k % 4,
    )


def run_json(run: BenchmarkRun, kind: str, run_id: str) -> dict[str, Any]:
    c = run.config
    t = run.timing
    return {
        "id": run_id,
        "kind": kind,
        "config": {
            "deck_under_test": deck_json(c.deck_under_test),
            "benchmark_deck": deck_json(c.benchmark_deck),
            "matches": c.matches,
            "seed": c.seed,
            "workers": c.workers,
            "fmt": c.fmt,
            "ai_preset": c.ai_preset,
            "decision_log": c.decision_log,
            "replays": c.replays,
            "turn_limit": c.turn_limit,
            "max_actions": c.max_actions,
        },
        "matches": [m.to_json() for m in run.matches],
        "timing": {
            "started_at": t.started_at,
            "finished_at": t.finished_at,
            "wall_seconds": t.wall_seconds,
            "match_seconds": list(t.match_seconds),
            "workers": t.workers,
        },
        "agent_factory": run.agent_factory,
        "host": {"python": platform.python_version(), "platform": platform.platform()},
        "report_data": {
            "versions": versions(),
            "conflict_resolutions": conflict_resolutions(
                {
                    "deck_under_test": c.deck_under_test.main + c.deck_under_test.resources,
                    "benchmark": c.benchmark_deck.main + c.benchmark_deck.resources,
                }
            ),
            "cards": card_facts((c.deck_under_test, c.benchmark_deck)),
        },
    }


def read_tree(root: Path, prefix: str) -> dict[str, bytes]:
    return {
        f"{prefix}/{p.relative_to(root).as_posix()}": p.read_bytes()
        for p in sorted(root.rglob("*"))
        if p.is_file()
    }


def benchmark_runs(files: dict[str, bytes], index: list[dict[str, str]]) -> BenchmarkRun:
    base: BenchmarkRun | None = None
    with tempfile.TemporaryDirectory() as tmp:
        for k, spec in enumerate(SPECS):
            dut, bench = load_spec_deck(spec.dut), load_spec_deck(spec.bench)
            config = BenchmarkConfig(
                dut,
                bench,
                matches=spec.matches,
                seed=spec.seed,
                fmt=spec.fmt,
                ai_preset=spec.preset,
                decision_log=spec.decision_log,
                replays=spec.replays,
                turn_limit=spec.turn_limit,
            )
            factory: AgentFactory | None = None
            if spec.agent == "random":
                factory = FactoryRef(RANDOM_AGENT)
            run = run_benchmark(config, agent_factory=factory)
            run = dataclasses.replace(run, timing=fixed_timing(run, k))
            out = Path(tmp) / spec.id
            write_reports(run, out)
            files.update(read_tree(out, spec.id))
            files[f"{spec.id}/run.json"] = dumps(run_json(run, "benchmark", spec.id)).encode()
            index.append({"id": spec.id, "kind": "benchmark"})
            if spec.id == "random-bo3-s3":
                base = run
            print(f"{spec.id}: {len(run.games)} games", file=sys.stderr)
    assert base is not None
    return base


def make_record(
    m: int,
    g: int,
    winner: str,
    *,
    point: str | None = None,
    reason: str = "battle_damage",
    dut_on_play: bool = True,
    turns: int = 12,
    drawn: tuple[str, ...] = (),
    played: tuple[str, ...] = (),
    bench_seen: tuple[str, ...] = (),
    redrew: bool = False,
) -> GameRecord:
    """``make_record`` of tests/reports/conftest.py:42-91."""
    events = DutCardEvents(
        initial_hand=drawn[:5],
        redrew=redrew,
        opening_hand=drawn[:5],
        drawn=tuple(sorted(drawn)),
        played=tuple(PlayEvent(n, 3, 2) for n in played),
        never_played=tuple(sorted(n for n in drawn if n not in played)),
        in_hand_at_end=(),
    )
    return GameRecord(
        match_index=m,
        game_index=g,
        seed=m * 10 + g,
        agent_seeds=(1, 2),
        agents=("fake", "fake"),
        dut_seat=m % 2,
        chooser=None if g == 0 else 0,
        chooser_seat=0,
        first_player=m % 2 if dut_on_play else 1 - m % 2,
        dut_on_play=dut_on_play,
        winner=winner,
        end_reason=reason,
        match_point=point if point is not None or winner == "draw" else winner,
        turns=turns,
        active_at_end=0,
        actions=(),
        redraws=(redrew, False) if m % 2 == 0 else (False, redrew),
        dut_cards=events,
        bench_seen=bench_seen,
        final_state_sha256="0" * 64,
        turn_limit=200,
        max_actions=20000,
    )


def games_with(n: int, wins: int, start: int, **kwargs: Any) -> list[GameRecord]:
    """``games_with`` of tests/reports/test_hypotheses.py:42-45."""
    return [make_record(start + i, 0, "dut" if i < wins else "bench", **kwargs) for i in range(n)]


def one_game_matches(games: list[GameRecord]) -> tuple[MatchRecord, ...]:
    """``results_for`` of tests/reports/test_hypotheses.py:17-32 (one BO1 match per game)."""
    return tuple(
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


def many_cards(base: BenchmarkRun) -> list[GameRecord]:
    """Several card signals in both directions with ties in |delta|, so the Holm family, the
    per-direction cut and the (-|delta|, id) order all matter."""
    dut_cards = sorted(set(base.config.deck_under_test.main))
    games: list[GameRecord] = []
    gen = SplitMix64(77)
    for i in range(160):
        win_p = 0.5
        drawn = []
        for j, number in enumerate(dut_cards[:9]):
            if (i + j) % 2 == 0:
                drawn.append(number)
                win_p += (0.09, -0.08, 0.07, -0.09, 0.05, 0.08, -0.07, 0.09, -0.05)[j]
        win = gen.random() < win_p
        played = tuple(drawn[::2])
        games.append(
            make_record(
                i,
                0,
                "dut" if win else ("draw" if i % 17 == 0 else "bench"),
                drawn=tuple(drawn),
                played=played,
                dut_on_play=i % 3 != 0,
                redrew=i % 4 == 0,
                turns=8 + i % 13,
                bench_seen=(THREAT,) if i % 2 == 0 else (),
            )
        )
    return games


def synthetic_runs(
    base: BenchmarkRun, files: dict[str, bytes], index: list[dict[str, str]]
) -> None:
    """The hand-built runs of tests/reports (results.json, summary.md and games.ndjson only)."""
    cases: list[tuple[str, list[GameRecord], dict[str, Any]]] = [
        (
            "synthetic-card-delta",
            games_with(30, 21, 0, drawn=(GOOD,), played=(GOOD,)) + games_with(30, 9, 30),
            {},
        ),
        ("synthetic-dead-card", games_with(20, 10, 0, drawn=(DEAD,)), {}),
        (
            "synthetic-play-draw-redraw",
            games_with(25, 20, 0, dut_on_play=True, redrew=False)
            + games_with(25, 5, 25, dut_on_play=False, redrew=True),
            {},
        ),
        (
            "synthetic-bench-threat",
            games_with(25, 5, 0, bench_seen=(THREAT,)) + games_with(25, 15, 25),
            {},
        ),
        (
            "synthetic-small",
            games_with(8, 8, 0, drawn=(GOOD, DEAD), bench_seen=(THREAT,)) + games_with(8, 0, 8),
            {},
        ),
        (
            "synthetic-noise",
            games_with(30, 17, 0, drawn=(GOOD,), played=(GOOD,)) + games_with(30, 14, 30),
            {},
        ),
        ("synthetic-many-cards", many_cards(base), {}),
        (
            "synthetic-in-memory-deck",
            games_with(21, 12, 0, drawn=(GOOD,), played=(GOOD,)),
            {"in_memory": True, "decision_log": True, "seed": 2**64 - 1},
        ),
    ]
    for run_id, games, opts in cases:
        dut = base.config.deck_under_test
        if opts.get("in_memory"):
            dut = dataclasses.replace(dut, name="in memory deck", source=None)
        config = dataclasses.replace(
            base.config,
            deck_under_test=dut,
            matches=len(games),
            fmt="bo1",
            decision_log=bool(opts.get("decision_log", False)),
            seed=int(opts.get("seed", base.config.seed)),
        )
        run = dataclasses.replace(base, config=config, matches=one_game_matches(games))
        emit_synthetic(run, run_id, files, index)
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
    config = dataclasses.replace(base.config, matches=2, replays=1)
    emit_synthetic(
        dataclasses.replace(base, config=config, matches=matches), "synthetic-draws", files, index
    )


def emit_synthetic(
    run: BenchmarkRun, run_id: str, files: dict[str, bytes], index: list[dict[str, str]]
) -> None:
    results = build_results(run)
    validate_results(results)
    files[f"{run_id}/results.json"] = to_json_text(results).encode()
    files[f"{run_id}/summary.md"] = render_summary(results, reproduce_command(run)).encode()
    files[f"{run_id}/games.ndjson"] = _games_text(run).encode()
    files[f"{run_id}/run.json"] = dumps(run_json(run, "synthetic", run_id)).encode()
    index.append({"id": run_id, "kind": "synthetic"})


def digit_ranges() -> list[list[int]]:
    """Code points that Python's ``\\d`` (str pattern, no ASCII flag) matches, as ranges."""
    pat = re.compile(r"\d")
    out: list[list[int]] = []
    for cp in range(sys.maxunicode + 1):
        if pat.fullmatch(chr(cp)):
            if out and out[-1][1] == cp - 1:
                out[-1][1] = cp
            else:
                out.append([cp, cp])
    return out


def pointer_parts(pointer: str) -> list[str]:
    return [p.replace("~1", "/").replace("~0", "~") for p in pointer.split("/")[1:]]


def apply_op(doc: Any, op: list[Any]) -> None:
    """Apply one ``[op, pointer, value?]`` mutation (``replace``, ``add``, ``remove``)."""
    kind, pointer = op[0], op[1]
    parts = pointer_parts(pointer)
    parent = doc
    for p in parts[:-1]:
        parent = parent[int(p)] if isinstance(parent, list) else parent[p]
    last = parts[-1]
    if kind == "remove":
        if isinstance(parent, list):
            del parent[int(last)]
        else:
            del parent[last]
    elif isinstance(parent, list):
        if last == "-":
            parent.append(copy.deepcopy(op[2]))
        else:
            parent[int(last)] = copy.deepcopy(op[2])
    else:
        parent[last] = copy.deepcopy(op[2])


HYPOTHESIS = {
    "id": "x",
    "kind": "card_drawn_win_rate",
    "label": "hypothesis",
    "statement": "s",
    "sample_size": 3,
    "evidence": {},
}
HEX64 = "0123456789abcdef" * 4

SCHEMA_OPS: list[list[list[Any]]] = [
    [],
    [["remove", "/cards"]],
    [["replace", "/results/games/dut_win_rate/rate", 1.5]],
    [["add", "/extra", 1]],
    [["add", "/hypotheses/-", {**HYPOTHESIS, "label": "fact"}]],
    [["add", "/hypotheses/-", HYPOTHESIS]],
    [["add", "/hypotheses/-", {**HYPOTHESIS, "sample_size": 0}]],
    [["add", "/hypotheses/-", {**HYPOTHESIS, "evidence": 5}]],
    [["add", "/hypotheses/-", {**HYPOTHESIS, "statement": ""}]],
    [["add", "/hypotheses/-", {**HYPOTHESIS, "kind": "other"}]],
    [["replace", "/config/format", "bo5"]],
    [["replace", "/schema", "urn:x"]],
    [["replace", "/schema_version", "1.2.0"]],
    [["replace", "/versions/code_digest", "ABC"]],
    [["replace", "/versions/code_digest", HEX64.upper()]],
    [["replace", "/versions/code_digest", HEX64 + "\n"]],
    [["replace", "/versions/code_digest", HEX64 + "\n\n"]],
    [["replace", "/versions/code_digest", HEX64 + "0"]],
    [["replace", "/versions/dataset_version", ""]],
    [["replace", "/versions/rules_effective_date", "2026-02-30"]],
    [["replace", "/versions/rules_effective_date", "2024-02-29"]],
    [["replace", "/versions/rules_effective_date", "2026-9-25"]],
    [["replace", "/versions/rules_effective_date", "2026-09-25\n"]],
    [["replace", "/versions/rules_effective_date", 20260925]],
    [["replace", "/versions/package", 1]],
    [["replace", "/config/matches", 0]],
    [["replace", "/config/matches", 1.0]],
    [["replace", "/config/matches", 1.5]],
    [["replace", "/config/matches", True]],
    [["replace", "/config/max_games_per_match", 2]],
    [["replace", "/config/max_games_per_match", 3.0]],
    [["replace", "/hypothesis_thresholds/alpha", 0]],
    [["replace", "/hypothesis_thresholds/alpha", 1]],
    [["replace", "/hypothesis_thresholds/alpha", 0.5]],
    [["replace", "/hypothesis_thresholds/multiple_comparisons", "bonferroni"]],
    [["add", "/splits/by_game_number/4", {"n": 0}]],
    [["remove", "/splits/by_game_number/1"]],
    [["add", "/end_reasons/0/extra", 1]],
    [["replace", "/end_reasons/0/reason", "other"]],
    [["remove", "/end_reasons/0/reason"]],
    [["add", "/results/games/extra", 1]],
    [["replace", "/results/games/dut_win_rate/ci95", [0.1]]],
    [["replace", "/results/games/dut_win_rate/ci95", [0.1, 0.2, 0.3]]],
    [["replace", "/results/games/dut_win_rate/ci95", [0.1, 2]]],
    [["replace", "/results/games/dut_win_rate/ci95", "x"]],
    [["replace", "/results/games/dut_win_rate/ci95", None]],
    [["replace", "/results/games/dut_win_rate/rate", None]],
    [["replace", "/results/games/dut_win_rate/rate", -0.0]],
    [["replace", "/results/games/dut_win_rate/rate", "0.5"]],
    [["replace", "/notes/0", 5]],
    [["replace", "/notes", "x"]],
    [["replace", "/decks/deck_under_test/colors", ["Black"]]],
    [["replace", "/decks/deck_under_test/digest", HEX64]],
    [["replace", "/decks/deck_under_test/main/0/count", 0]],
    [["replace", "/game_length/turns/mean", None]],
    [["replace", "/game_length/turns/median", 1.5]],
    [["replace", "/game_length/turns/median", 2.0]],
    [["replace", "/game_length/histogram/0/games", 0]],
    [["replace", "/game_length/histogram", {}]],
    [["replace", "/cards/0/drawn/copies_per_game", 5.0]],
    [["replace", "/cards/0/drawn/copies_per_game", -1]],
    [["replace", "/cards/0/level", 2.5]],
    [["replace", "/benchmark_cards/0/loss_rate_lift", -1.5]],
    [["replace", "/benchmark_cards/0/rank_eligible", 1]],
    [["replace", "/conflict_resolutions/0/decks", []]],
    [["replace", "/conflict_resolutions/0/decks", ["sideboard"]]],
    [["replace", "/ai/preset", "fast"]],
    [["replace", "/ai/preset", "turbo"]],
    [["replace", "/ai/factory", ""]],
    [["replace", "/ai/decision_log", 0]],
    [["replace", "/seeds/master", 2**64 - 1]],
    [["replace", "/seeds/master", -1]],
    [["replace", "/seeds/prng", "PCG"]],
    [["replace", "/versions", "x"]],
    [["replace", "/replays/0/file", "replays/win-m0-g1.json"]],
    [["replace", "/replays/0/file", "replays/win-m00000-g3.json\n"]],
    [["replace", "/replays/0/file", "replays/tie-m0000-g1.json"]],
    [["replace", "/replays/0/game_number", 4]],
    [["replace", "/draws/games", -1]],
    [["replace", "/mulligan/bench", {}]],
    [["replace", "/mulligan/dut/redraw_rate/n", 1.0]],
    [["remove", "/mulligan/dut/redraw_rate/ci95"]],
]


def schema_cases(files: dict[str, bytes]) -> list[dict[str, Any]]:
    base_id = "random-bo3-s3"
    base = json.loads(files[f"{base_id}/results.json"])
    cases = []
    for ops in SCHEMA_OPS:
        doc = copy.deepcopy(base)
        for op in ops:
            apply_op(doc, op)
        try:
            validate_results(doc)
            valid = True
        except jsonschema.ValidationError:
            valid = False
        cases.append({"base": base_id, "ops": ops, "valid": valid})
    return cases


def schema_keywords(node: Any, out: set[str]) -> None:
    if isinstance(node, dict):
        for k, v in node.items():
            out.add(k)
            if k in {"properties", "$defs"}:
                for sub in v.values():
                    schema_keywords(sub, out)
            elif k not in {"enum", "const", "required"}:
                schema_keywords(v, out)
    elif isinstance(node, list):
        for item in node:
            schema_keywords(item, out)


def stats_vectors(files: dict[str, bytes]) -> dict[str, Any]:
    gen = SplitMix64(20261101)
    wilson_cases = [[s, n, *map(bits, wilson(s, n))] for n in range(41) for s in range(n + 1)]
    for _ in range(400):
        n = 1 + gen.randrange(1_000_000)
        s = gen.randrange(n + 1)
        wilson_cases.append([s, n, *map(bits, wilson(s, n))])
    z_cases = []
    for i in range(3000):
        n1, n2 = gen.randrange(301), gen.randrange(301)
        if i % 10 == 0:
            n1 = 0
        s1 = gen.randrange(n1 + 1) if n1 else 0
        s2 = gen.randrange(n2 + 1) if n2 else 0
        if i % 13 == 0 and n1 and n2:
            s1, s2 = 0, 0
        z_cases.append([s1, n1, s2, n2, bits(two_proportion_p(rate(s1, n1), rate(s2, n2)))])
    holm_cases = []
    for i in range(400):
        m = gen.randrange(12)
        ps = [round(gen.random() * (0.2 if i % 2 else 0.02), 6) for _ in range(m)]
        if m > 2 and i % 3 == 0:
            ps[1] = ps[0]
        if m and i % 5 == 0:
            ps[-1] = 0.05 / m
        ps += [1.0] * (i % 4)
        holm_cases.append([[bits(p) for p in ps], holm(ps, 0.05)])
    dist_cases = []
    for i in range(300):
        values = [gen.randrange(1 + (i % 50) * 7) for _ in range(i % 23)]
        dist_cases.append([values, distribution(values)])
    quote = [
        "",
        "abc",
        "a b",
        "it's",
        "Zaku Ⅱ",
        'a"b',
        "%+,-./:=@_09AZaz",
        "\n",
        "~",
        "examples/decks/red-green-zeon.txt",
        "$HOME",
        "a*b",
        "ü",
        "crates/gcg-runner/tests/vectors/records/decks/zeon's deck.txt",
        "''",
    ]
    names = [
        "win-m0000-g1.json",
        "loss-m0012-g2.json",
        "draw-m12345-g3.json",
        "win-m0000-g1.json\n",
        "win-m0000-g1.json\n\n",
        "loss-m000-g1.json",
        "win-m0000-g4.json",
        "win-m0000-g0.json",
        "win-m٠١٢٣-g1.json",
        "win-m\U0001d7ce\U0001d7cf\U0001d7d0\U0001d7d1-g1.json",
        "win-m0000-g١.json",
        "win-m0000-g1.jsonx",
        "xwin-m0000-g1.json",
        "win-m0000-g1xjson",
        "notes.txt",
        "WIN-m0000-g1.json",
        "win-m00a0-g1.json",
        "draw-m0000-g1.json.tmp",
        ".win-m0000-g1.json.123.tmp",
    ]
    schema = load_schema()
    keywords: set[str] = set()
    schema_keywords(schema, keywords)
    patterns = sorted(
        {m.group(1) for m in re.finditer(r'"pattern": "((?:[^"\\]|\\.)*)"', json.dumps(schema))}
    )
    pattern_cases = []
    probes = [
        HEX64,
        HEX64 + "\n",
        HEX64 + "\n\n",
        HEX64.upper(),
        HEX64[:-1],
        HEX64 + "0",
        "x" + HEX64,
        "",
        "replays/win-m0000-g1.json",
        "replays/loss-m12345-g3.json",
        "replays/draw-m0000-g2.json\n",
        "replays/win-m000-g1.json",
        "replays/win-m0000-g4.json",
        "replays/tie-m0000-g1.json",
        "replays/win-m0000-g1.jsonx",
        "areplays/win-m0000-g1.json",
        "replays/win-m0000-g1Xjson",
        "replays/win-m٠٠٠٠-g1.json",
    ]
    for p in patterns:
        text = json.loads(f'"{p}"')
        pattern_cases.append(
            {"pattern": text, "cases": [[s, re.search(text, s) is not None] for s in probes]}
        )
    checker = jsonschema.FormatChecker()
    dates = [
        "2026-09-25",
        "2026-02-29",
        "2024-02-29",
        "1900-02-29",
        "2000-02-29",
        "0000-01-01",
        "0001-01-01",
        "9999-12-31",
        "2026-13-01",
        "2026-00-10",
        "2026-04-31",
        "2026-04-30",
        "2026-1-01",
        "2026-09-25\n",
        " 2026-09-25",
        "２０２６-09-25",
        "2026/09/25",
        "20260925",
        "",
    ]
    return {
        "python": sys.version.split()[0],
        "wilson": wilson_cases,
        "wilson_errors": [[-1, 5], [6, 5], [1, -1]],
        "two_proportion_p": z_cases,
        "holm": holm_cases,
        "distribution": dist_cases,
        "shlex_quote": [[s, shlex.quote(s)] for s in quote],
        "replay_file": [[s, REPLAY_FILE.match(s) is not None] for s in names],
        "digit_ranges": digit_ranges(),
        "schema_keywords": sorted(keywords),
        "patterns": pattern_cases,
        "dates": [[s, checker.conforms(s, "date")] for s in dates],
        "schema_cases": schema_cases(files),
    }


def build() -> dict[str, bytes]:
    files: dict[str, bytes] = {}
    index: list[dict[str, str]] = []
    base = benchmark_runs(files, index)
    synthetic_runs(base, files, index)
    stats = stats_vectors(files)
    stats.pop("python")
    files["stats.json"] = dumps(stats).encode()
    files["index.json"] = dumps({"runs": index}).encode()
    return files


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true", help="fail if the committed files are stale")
    args = ap.parse_args()
    os.chdir(ROOT)
    files = build()
    existing = (
        {p.relative_to(OUT).as_posix(): p for p in sorted(OUT.rglob("*")) if p.is_file()}
        if OUT.exists()
        else {}
    )
    if args.check:
        stale = sorted(
            name
            for name in set(files) | set(existing)
            if name not in files
            or name not in existing
            or existing[name].read_bytes() != files[name]
        )
        if stale:
            print(
                f"{OUT} is stale ({len(stale)} files, first {stale[0]}); run without --check",
                file=sys.stderr,
            )
            return 1
        print(f"{OUT} is current ({len(files)} files)")
        return 0
    for name, path in existing.items():
        if name not in files:
            path.unlink()
    for name, data in files.items():
        path = OUT / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
    print(f"wrote {len(files)} files to {OUT}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
