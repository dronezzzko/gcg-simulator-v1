"""Throughput of a Rust agent, measured outside the crates (docs/port/WEAKNESSES.md W-126).

The only timing in the project lives outside the Rust crates (docs/port/BRIEF.md
<performance_comparison>); this script is the throughput counterpart of
``scripts/compare_python_vs_rust.sh``. It times ``gcg-eval head-to-head`` (a release build) on
several deck pairings, in whole mirrored blocks of four games, and repeats every run. Per run it
records the wall time, the CPU time of the run from the OS (``getrusage`` of the child: user
plus system), the deterministic effort counts of the report (decisions, searches, iterations,
engine applies), and the machine's load average before the run. Per pairing it reports the
minimum and median over the runs of CPU seconds per game, and iterations, applies and decisions
per wall second and per CPU second.

``--games`` is the number of games per pairing; a value that is not a multiple of 4 is rounded up
to whole blocks, with a message. Unlike the Python tool (``ai/throughput.py``) it never counts
wall time times workers as busy core time. The machine is assumed to run at full performance
(docs/port/BRIEF.md <target_hardware>), so no power state is read or reported.

    python3 scripts/throughput.py search:fast --opponent ref:standard --games 8 --runs 3 \
        [--pairs a:b,c:d] [--workers 1] [--seed 7] [--binary target/release/gcg-eval] [--json FILE]
"""

from __future__ import annotations

import argparse
import json
import os
import platform
import resource
import statistics
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_PAIRS = (
    "red-green-zeon:blue-white-federation",
    "blue-white-federation:purple-white-tekkadan",
    "purple-white-tekkadan:red-green-zeon",
)
BLOCK = 4


def whole_blocks(games: int) -> int:
    """``games`` rounded up to a multiple of the mirrored block of four."""
    if games <= 0:
        raise ValueError("--games must be positive")
    return -(-games // BLOCK) * BLOCK


def sysctl(name: str) -> str:
    try:
        out = subprocess.run(
            ["/usr/sbin/sysctl", "-n", name], capture_output=True, text=True, check=False
        )
    except OSError:
        return ""
    return out.stdout.strip() if out.returncode == 0 else ""


def machine() -> dict[str, Any]:
    return {
        "cpu": sysctl("machdep.cpu.brand_string") or platform.processor(),
        "logical_cpus": os.cpu_count(),
        "perf_levels": [
            {
                "name": sysctl(f"hw.perflevel{i}.name"),
                "logical": sysctl(f"hw.perflevel{i}.logicalcpu"),
            }
            for i in range(int(sysctl("hw.nperflevels") or 0))
        ],
        "memory_bytes": sysctl("hw.memsize"),
        "os": platform.platform(),
    }


def run_once(binary: Path, args: argparse.Namespace, pair: str, out: Path) -> dict[str, Any]:
    argv = [
        str(binary),
        "head-to-head",
        args.agent,
        args.opponent,
        "--pair",
        pair,
        "--games",
        str(args.games),
        "--seed",
        str(args.seed),
        "--workers",
        str(args.workers),
        "--out",
        str(out),
    ]
    load = os.getloadavg()
    before = resource.getrusage(resource.RUSAGE_CHILDREN)
    start = time.perf_counter()
    subprocess.run(argv, cwd=ROOT, stdout=subprocess.DEVNULL, check=True)
    wall = time.perf_counter() - start
    after = resource.getrusage(resource.RUSAGE_CHILDREN)
    cpu = (after.ru_utime - before.ru_utime) + (after.ru_stime - before.ru_stime)
    report = json.loads(out.read_text(encoding="utf-8"))
    effort = report["result"].get("effort") or {}
    counts = {"decisions": 0, "searches": 0, "iterations": 0, "applies": 0}
    for side in ("challenger", "opponent"):
        e = effort.get(side) or {}
        counts["decisions"] += int(e.get("decisions", 0))
        counts["searches"] += int(e.get("searches", 0))
        counts["iterations"] += int(e.get("iterations", 0))
        counts["applies"] += sum(
            int(e.get(k, 0)) for k in ("applies_tree", "applies_rollout", "applies_aux")
        )
    return {
        "pair": pair,
        "games": int(report["games"]),
        "wall_s": wall,
        "cpu_s": cpu,
        "load_before": [round(x, 2) for x in load],
        **counts,
    }


def summarise(runs: list[dict[str, Any]]) -> dict[str, Any]:
    def stat(values: list[float]) -> dict[str, float]:
        return {"min": min(values), "median": statistics.median(values), "max": max(values)}

    games = runs[0]["games"]
    out: dict[str, Any] = {
        "runs": len(runs),
        "games": games,
        "counts": {k: runs[0][k] for k in ("decisions", "searches", "iterations", "applies")},
        "cpu_s_per_game": stat([r["cpu_s"] / games for r in runs]),
        "wall_s": stat([r["wall_s"] for r in runs]),
    }
    for k in ("iterations", "applies", "decisions"):
        out[f"{k}_per_wall_s"] = stat([r[k] / r["wall_s"] for r in runs])
        out[f"{k}_per_cpu_s"] = stat([r[k] / r["cpu_s"] for r in runs if r["cpu_s"] > 0] or [0.0])
    if len({tuple(r[k] for k in ("decisions", "iterations", "applies")) for r in runs}) != 1:
        out["warning"] = "the effort counts differ between runs (the search is not deterministic)"
    return out


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=(__doc__ or "").splitlines()[0])
    ap.add_argument("agent", help="gcg-eval agent spec of the challenger, e.g. search:fast")
    ap.add_argument("--opponent", help="agent spec of the opponent (default: the challenger)")
    ap.add_argument("--pairs", default=",".join(DEFAULT_PAIRS), help="DECK_A:DECK_B,...")
    ap.add_argument("--games", type=int, default=8, help="games per pairing (whole blocks)")
    ap.add_argument("--runs", type=int, default=3, help="repeated runs per pairing")
    ap.add_argument("--workers", type=int, default=1)
    ap.add_argument("--seed", type=int, default=7)
    ap.add_argument("--binary", type=Path, default=ROOT / "target" / "release" / "gcg-eval")
    ap.add_argument("--json", type=Path, help="also write the report here")
    args = ap.parse_args(argv)
    args.opponent = args.opponent or args.agent
    try:
        games = whole_blocks(args.games)
    except ValueError as exc:
        ap.error(str(exc))
    if games != args.games:
        print(
            f"--games {args.games} rounded up to {games}: whole blocks of {BLOCK} games", flush=True
        )
        args.games = games
    if args.runs < 1:
        ap.error("--runs must be at least 1")
    if not args.binary.is_file():
        print(f"{args.binary} not found: cargo build --release -p gcg-eval", file=sys.stderr)
        return 2
    report: dict[str, Any] = {
        "machine": machine(),
        "command": vars(args) | {"binary": str(args.binary)},
    }
    report["pairs"] = {}
    with tempfile.TemporaryDirectory(prefix="throughput-") as tmp:
        for pair in args.pairs.split(","):
            runs = [
                run_once(args.binary, args, pair, Path(tmp) / "h2h.json") for _ in range(args.runs)
            ]
            report["pairs"][pair] = {"summary": summarise(runs), "runs": runs}
            s = report["pairs"][pair]["summary"]
            print(
                f"{pair}: {s['games']} games x {s['runs']} runs; CPU s/game min "
                f"{s['cpu_s_per_game']['min']:.3f} median {s['cpu_s_per_game']['median']:.3f}; "
                f"iterations/CPU s min {s['iterations_per_cpu_s']['min']:.0f}; "
                f"applies/CPU s min {s['applies_per_cpu_s']['min']:.0f}"
                + (f"; {s['warning']}" if "warning" in s else "")
            )
    text = json.dumps(report, indent=1, default=str) + "\n"
    if args.json:
        args.json.write_text(text, encoding="utf-8")
    else:
        print(text, end="")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
