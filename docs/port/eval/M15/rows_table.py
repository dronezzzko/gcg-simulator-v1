"""M15: the tables of docs/port/eval/M15/README.md from the gcg-eval reports.

Subcommands:

- ``rows [FILE...]``: one PLAN §12.10 row per ``gcg-eval ablation`` or ``head-to-head`` report:
  wins/N with the Wilson 95% interval, the paired difference with the Newcombe 95% interval,
  the McNemar discordant counts, iterations and applies per game (challenger vs opponent), the
  lowest pair (wins/n and its Wilson upper bound) and the number of pairs whose upper bound is
  below 0.5.
- ``versus PLACEBO FILE...``: each report against a report of the same seed family game by game
  (``docs/port/eval/P6-F/versus_placebo.py``'s method): games whose outcome, length or decision
  count differ; challenger wins only in the row / only in the placebo; the difference per game
  with a Wald 95% interval.
- ``curve FILE``: one row per point of a ``gcg-eval budget-curve`` report, with the lowest pair.
- ``versus-points BASE FILE``: each point of a budget curve against the point of the same scale
  of a base curve on the same master, game by game (the ``versus`` columns).

Usage: uv run python docs/port/eval/M15/rows_table.py rows docs/port/eval/M15/rows/*.json
"""

from __future__ import annotations

import json
import math
import sys
from pathlib import Path

Z = 1.959964


def lowest_pair(by_pair: list[dict]) -> tuple[str, int]:
    worst = min(by_pair, key=lambda p: (p["result"]["ci95"][1], p["result"]["rate"]))
    r = worst["result"]
    below = sum(1 for p in by_pair if p["result"]["ci95"][1] < 0.5)
    name = ":".join(worst["decks"])
    return f"{r['wins']}/{r['n']} (UB {r['ci95'][1]:.3f}, {name})", below


def summary_of(doc: dict) -> dict:
    """The row summary; a head-to-head report has none, so it is built from `result`."""
    if "summary" in doc:
        return doc["summary"]
    r = doc["result"]
    e = r["effort"]
    return {
        "wins": r["wins"],
        "n": r["n"],
        "rate": r["rate"],
        "ci95": r["ci95"],
        "difference": r["paired"]["difference"],
        "difference_ci95": r["paired"]["ci95"],
        "discordant": [r["paired"]["challenger_only"], r["paired"]["opponent_only"]],
        "iterations_per_game": {k: e[k]["iterations_per_game"] for k in e},
        "applies_per_game": {k: e[k]["applies_per_game"] for k in e},
    }


def summary_cells(s: dict) -> str:
    lo, hi = s["ci95"]
    dlo, dhi = s["difference_ci95"]
    it, ap = s["iterations_per_game"], s["applies_per_game"]
    return (
        f"{s['wins']}/{s['n']} ({s['rate']:.3f}, [{lo:.3f}, {hi:.3f}]) | "
        f"{s['difference']:+.3f} [{dlo:+.3f}, {dhi:+.3f}] | "
        f"{s['discordant'][0]}/{s['discordant'][1]} | "
        f"{it['challenger']:.0f} vs {it['opponent']:.0f} / "
        f"{ap['challenger']:.0f} vs {ap['opponent']:.0f}"
    )


def rows(paths: list[Path]) -> None:
    print(
        "| Row | wins/N (rate, Wilson 95%) | paired difference (Newcombe 95%) | discordant | "
        "iterations / applies per game (challenger vs opponent) | lowest pair | pairs UB < 0.5 "
        "| seed (role) | evidence |"
    )
    print("| --- | --- | --- | --- | --- | --- | --- | --- | --- |")
    for path in paths:
        doc = json.loads(path.read_text())
        low, below = lowest_pair(doc["by_pair"])
        print(
            f"| `{path.stem}` | {summary_cells(summary_of(doc))} | {low} | {below} | "
            f"{doc['seed']} ({doc['role']}) | `{path}` |"
        )


def outcomes(doc: dict) -> list[tuple[bool, tuple[int, int, int]]]:
    # [index, position, challenger seat, winner, turns, first player, decisions, ...]
    return [(row[3] == row[2], (row[3], row[4], row[6])) for row in doc["outcomes"]]


def versus(placebo: Path, paths: list[Path]) -> None:
    base = outcomes(json.loads(placebo.read_text()))
    print("| Row | changed games | row-only / placebo-only wins | difference [Wald 95%] |")
    print("| --- | --- | --- | --- |")
    for path in paths:
        cur = outcomes(json.loads(path.read_text()))
        if len(cur) != len(base):
            print(f"| `{path.stem}` | — (different game count) | — | — |")
            continue
        n = len(cur)
        pairs = list(zip(cur, base, strict=True))
        changed = sum(1 for (_, ka), (_, kb) in pairs if ka != kb)
        b = sum(1 for (a, _), (p, _) in pairs if a and not p)
        c = sum(1 for (a, _), (p, _) in pairs if p and not a)
        d = (b - c) / n
        half = Z * math.sqrt(max(((b + c) / n - d * d) / n, 0.0))
        print(
            f"| `{path.stem}` | {changed} | {b} / {c} | {d:+.4f} [{d - half:+.4f}, {d + half:+.4f}] |"
        )


def versus_points(base_path: Path, path: Path) -> None:
    """Each point of a budget curve against the same scale's point of a base curve."""
    base = {p["scale"]: outcomes(p) for p in json.loads(base_path.read_text())["points"]}
    print(f"`{path.stem}` against `{base_path.stem}`, game by game:")
    print("| scale | changed games | row-only / base-only wins | difference [Wald 95%] |")
    print("| --- | --- | --- | --- |")
    for p in json.loads(path.read_text())["points"]:
        cur, ref = outcomes(p), base.get(p["scale"])
        if ref is None or len(ref) != len(cur):
            continue
        n = len(cur)
        pairs = list(zip(cur, ref, strict=True))
        changed = sum(1 for (_, ka), (_, kb) in pairs if ka != kb)
        b = sum(1 for (a, _), (q, _) in pairs if a and not q)
        c = sum(1 for (a, _), (q, _) in pairs if q and not a)
        d = (b - c) / n
        half = Z * math.sqrt(max(((b + c) / n - d * d) / n, 0.0))
        print(
            f"| {p['scale']:g} | {changed} | {b} / {c} | "
            f"{d:+.4f} [{d - half:+.4f}, {d + half:+.4f}] |"
        )


def curve(path: Path) -> None:
    doc = json.loads(path.read_text())
    print(f"{doc['challenger']} vs {doc['opponent']}, seed {doc['seed']} ({doc['role']})")
    print(
        "| scale | budget (ipo/min/max) | wins/N (rate, Wilson 95%) | paired difference "
        "(Newcombe 95%) | discordant | iterations / applies per game (challenger vs opponent) "
        "| lowest pair | pairs UB < 0.5 |"
    )
    print("| --- | --- | --- | --- | --- | --- | --- | --- |")
    for p in doc["points"]:
        b = p["budget"]
        low, below = lowest_pair(p["by_pair"])
        print(
            f"| {p['scale']:g} | {b['iterations_per_option']}/{b['min_iterations']}/"
            f"{b['max_iterations']} | {summary_cells(p['summary'])} | {low} | {below} |"
        )


def main() -> None:
    cmd, args = sys.argv[1], [Path(a) for a in sys.argv[2:]]
    if cmd == "rows":
        rows(args)
    elif cmd == "versus":
        versus(args[0], args[1:])
    elif cmd == "curve":
        for a in args:
            curve(a)
    elif cmd == "versus-points":
        versus_points(args[0], args[1])
    else:
        raise SystemExit(f"unknown subcommand {cmd!r}")


if __name__ == "__main__":
    main()
