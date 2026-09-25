"""Writing the report files. Each file is written to a temporary name and atomically renamed,
and ``results.json`` is written last, so an interrupted write never leaves a corrupt file."""

from __future__ import annotations

import json
import os
import platform
import re
import shlex
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from gcg_sim.reports.results import build_results, replay_name, select_replays, validate_results
from gcg_sim.reports.summary import render_summary
from gcg_sim.runner.records import BenchmarkRun
from gcg_sim.runner.replayfile import replay_document

REPLAY_FILE = re.compile(r"^(win|loss|draw)-m\d{4,}-g[1-3]\.json$")


@dataclass(frozen=True, slots=True)
class ReportPaths:
    out_dir: Path
    results: Path
    summary: Path
    games: Path
    replays: tuple[Path, ...]
    timing: Path


def to_json_text(obj: Any) -> str:
    return json.dumps(obj, indent=2, sort_keys=True, ensure_ascii=False) + "\n"


def write_atomic(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_name(f".{path.name}.{os.getpid()}.tmp")
    tmp.write_bytes(text.encode("utf-8"))
    os.replace(tmp, path)


def reproduce_command(run: BenchmarkRun) -> str:
    c = run.config
    decks = [d.source or d.name for d in (c.deck_under_test, c.benchmark_deck)]
    args = ["gcg-sim", "benchmark", *decks, "--matches", str(c.matches), "--seed", str(c.seed)]
    args += ["--format", c.fmt, "--ai-preset", c.ai_preset, "--replays", str(c.replays)]
    if c.decision_log:
        args.append("--decision-log")
    return shlex.join(args)


def _games_text(run: BenchmarkRun) -> str:
    return "".join(
        json.dumps(g.to_json(), sort_keys=True, separators=(",", ":")) + "\n" for g in run.games
    )


def _timing(run: BenchmarkRun) -> dict[str, Any]:
    t = run.timing
    seconds = t.match_seconds
    return {
        "note": "wall-clock measurements; the only non-reproducible report file",
        "started_at": t.started_at,
        "finished_at": t.finished_at,
        "wall_seconds": round(t.wall_seconds, 3),
        "workers": t.workers,
        "matches": len(run.matches),
        "games": len(run.games),
        "match_seconds": [round(s, 3) for s in seconds],
        "mean_match_seconds": round(sum(seconds) / len(seconds), 3) if seconds else None,
        "games_per_second": round(len(run.games) / t.wall_seconds, 3) if t.wall_seconds else None,
        "python": platform.python_version(),
        "platform": platform.platform(),
    }


def _write_replays(run: BenchmarkRun, out_dir: Path) -> tuple[Path, ...]:
    written = []
    for label, record in select_replays(run):
        path = out_dir / replay_name(label, record)
        write_atomic(path, to_json_text(replay_document(record, run.config, label)))
        written.append(path)
    replay_dir = out_dir / "replays"
    if replay_dir.is_dir():
        keep = {p.name for p in written}
        for stale in replay_dir.iterdir():
            if REPLAY_FILE.match(stale.name) and stale.name not in keep:
                stale.unlink()
    return tuple(written)


def write_reports(run: BenchmarkRun, out_dir: Path) -> ReportPaths:
    """Write results.json, summary.md, games.ndjson, replays/*.json and timing.json."""
    results = build_results(run)
    validate_results(results)
    paths = ReportPaths(
        out_dir=out_dir,
        results=out_dir / "results.json",
        summary=out_dir / "summary.md",
        games=out_dir / "games.ndjson",
        replays=_write_replays(run, out_dir),
        timing=out_dir / "timing.json",
    )
    write_atomic(paths.games, _games_text(run))
    write_atomic(paths.summary, render_summary(results, reproduce_command(run)))
    write_atomic(paths.timing, to_json_text(_timing(run)))
    write_atomic(paths.results, to_json_text(results))
    return paths
