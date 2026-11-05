"""Throughput of a Rust agent, measured outside the crates (docs/dev/WEAKNESSES.md W-126).

The only timing in the project lives outside the Rust crates; this script is the throughput counterpart of
``scripts/compare_python_vs_rust.sh``. It times ``gcg-eval head-to-head`` (a release build) on
several deck pairings, in whole mirrored blocks of four games, and repeats every run. Per run it
records the wall time, the CPU time of the run from the OS (``getrusage`` of the child: user
plus system), the deterministic effort counts of the report (decisions, searches, iterations,
engine applies), and the machine's load average before the run. Per pairing it reports the
minimum and median over the runs of CPU seconds per game, and iterations, applies and decisions
per wall second and per CPU second.

``--games`` is the number of games per pairing; a value that is not a multiple of 4 is rounded up
to whole blocks, with a message. Unlike the Python tool (``ai/throughput.py``) it never counts
wall time times workers as busy core time. The machine is assumed to run at full performance,
so no power state is read or reported.

``--agree FIRST.json SECOND.json`` is the reproducibility check of W-126: two reports of the same
command (two sessions on an idle machine) must agree within ``--tolerance`` (default 3%) on the
minimum CPU seconds per game and the minimum wall time of every pairing, with equal effort
counts; it exits 1 otherwise.

    python3 scripts/throughput.py search:fast --opponent ref:standard --games 8 --runs 3 \
        [--pairs a:b,c:d] [--workers 1] [--seed 7] [--binary target/release/gcg-eval] [--json FILE]
    python3 scripts/throughput.py --agree first.json second.json [--tolerance 0.03]
    python3 scripts/throughput.py --selftest
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


COMPARED_COMMAND = ("agent", "opponent", "pairs", "games", "runs", "workers", "seed")


def agree(first: dict[str, Any], second: dict[str, Any], tolerance: float) -> list[str]:
    """The reproducibility check of W-126 on two reports of the same command: per pairing, the
    minimum CPU seconds per game and the minimum wall time must agree within ``tolerance``
    (relative to the smaller value), and the deterministic effort counts must be equal. Returns
    one line per pairing and quantity, each starting with ``ok`` or ``FAIL``."""
    lines: list[str] = []
    for key in COMPARED_COMMAND:
        a, b = first["command"].get(key), second["command"].get(key)
        if a != b:
            lines.append(f"FAIL command: {key} {a!r} != {b!r}")
    if sorted(first["pairs"]) != sorted(second["pairs"]):
        lines.append(f"FAIL pairs: {sorted(first['pairs'])} != {sorted(second['pairs'])}")
        return lines
    for pair in sorted(first["pairs"]):
        sa, sb = first["pairs"][pair]["summary"], second["pairs"][pair]["summary"]
        if sa["counts"] != sb["counts"]:
            lines.append(f"FAIL {pair}: effort counts {sa['counts']} != {sb['counts']}")
        for name, field in (("CPU s/game", "cpu_s_per_game"), ("wall s", "wall_s")):
            a, b = float(sa[field]["min"]), float(sb[field]["min"])
            diff = abs(a - b) / min(a, b) if min(a, b) > 0 else float("inf")
            verdict = "ok" if diff <= tolerance else "FAIL"
            lines.append(
                f"{verdict} {pair}: minimum {name} {a:.4f} vs {b:.4f}, "
                f"difference {100 * diff:.2f}% (tolerance {100 * tolerance:.1f}%)"
            )
    return lines


def _synthetic(cpu: float, wall: float, iterations: int = 100) -> dict[str, Any]:
    runs = [
        {
            "pair": "a:b",
            "games": 4,
            "wall_s": wall * k,
            "cpu_s": cpu * 4 * k,
            "load_before": [0.0, 0.0, 0.0],
            "decisions": 10,
            "searches": 5,
            "iterations": iterations,
            "applies": 1000,
        }
        for k in (1.0, 1.1)
    ]
    command = dict.fromkeys(COMPARED_COMMAND, 1) | {"pairs": "a:b"}
    return {"command": command, "pairs": {"a:b": {"summary": summarise(runs), "runs": runs}}}


def selftest() -> int:
    """Block rounding, the per-pairing summary and the agreement check, on synthetic runs."""
    problems: list[str] = []
    if [whole_blocks(g) for g in (1, 3, 4, 5, 400)] != [4, 4, 4, 8, 400]:
        problems.append("whole_blocks does not round up to blocks of four")
    s = _synthetic(cpu=2.0, wall=10.0)["pairs"]["a:b"]["summary"]
    if (s["cpu_s_per_game"]["min"], s["wall_s"]["min"], s["runs"]) != (2.0, 10.0, 2):
        problems.append(f"summary minima: {s}")
    base = _synthetic(cpu=2.0, wall=10.0)
    near = agree(base, _synthetic(cpu=2.05, wall=10.2), 0.03)
    if any(line.startswith("FAIL") for line in near) or len(near) != 2:
        problems.append(f"2.5% and 2% differences should agree: {near}")
    far = agree(base, _synthetic(cpu=2.1, wall=10.0), 0.03)
    if [line.split()[0] for line in far] != ["FAIL", "ok"]:
        problems.append(f"a 5% CPU difference should fail: {far}")
    counts = agree(base, _synthetic(cpu=2.0, wall=10.0, iterations=101), 0.03)
    if not counts[0].startswith("FAIL a:b: effort counts"):
        problems.append(f"different effort counts should fail: {counts}")
    other = _synthetic(cpu=2.0, wall=10.0)
    other["command"]["workers"] = 2
    if not agree(base, other, 0.03)[0].startswith("FAIL command: workers"):
        problems.append("reports of different commands should fail")
    for p in problems:
        print(f"selftest: {p}", file=sys.stderr)
    print("throughput selftest " + ("failed" if problems else "passed"))
    return 1 if problems else 0


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=(__doc__ or "").splitlines()[0])
    ap.add_argument(
        "agent", nargs="?", help="gcg-eval agent spec of the challenger, e.g. search:fast"
    )
    ap.add_argument(
        "--agree",
        nargs=2,
        type=Path,
        metavar=("FIRST.json", "SECOND.json"),
        help="compare two --json reports of the same command (W-126) instead of measuring",
    )
    ap.add_argument("--tolerance", type=float, default=0.03, help="relative, for --agree")
    ap.add_argument("--selftest", action="store_true", help="run the tool's own checks")
    ap.add_argument("--opponent", help="agent spec of the opponent (default: the challenger)")
    ap.add_argument("--pairs", default=",".join(DEFAULT_PAIRS), help="DECK_A:DECK_B,...")
    ap.add_argument("--games", type=int, default=8, help="games per pairing (whole blocks)")
    ap.add_argument("--runs", type=int, default=3, help="repeated runs per pairing")
    ap.add_argument("--workers", type=int, default=1)
    ap.add_argument("--seed", type=int, default=7)
    ap.add_argument("--binary", type=Path, default=ROOT / "target" / "release" / "gcg-eval")
    ap.add_argument("--json", type=Path, help="also write the report here")
    args = ap.parse_args(argv)
    if args.selftest:
        return selftest()
    if args.agree:
        first, second = (json.loads(p.read_text(encoding="utf-8")) for p in args.agree)
        lines = agree(first, second, args.tolerance)
        print("\n".join(lines))
        failed = any(line.startswith("FAIL") for line in lines)
        print("throughput runs " + ("differ" if failed else "agree"))
        return 1 if failed else 0
    if not args.agent:
        ap.error("the agent spec is required (or --agree / --selftest)")
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
