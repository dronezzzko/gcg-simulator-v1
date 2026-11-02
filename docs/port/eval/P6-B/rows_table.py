"""P6-B: the ablation table of docs/port/eval/weakness-status/P6-B.md from the row reports.

Every report under rows/ is a `gcg-eval ablation` report (PLAN §12.4). For each one this prints
its row: wins/N with the Wilson 95% interval, the paired difference with the Newcombe 95%
interval, the discordant counts, and iterations and applies per game (challenger vs opponent).
The individual rows (dev-*) share master 20200100 with the placebo row (the null configuration
against ref:standard), so game i has the same deal, seats and play/draw and the challenger is
always `search-standard` with the same decision seeds; the "vs placebo" columns compare each
individual row with the placebo game by game (games with another outcome or length, McNemar
counts of challenger wins only in the row / only in the placebo, the difference per game with a
Wald 95% interval), as docs/port/eval/P6-F/versus_placebo.py does for P6-F.

Usage: uv run python docs/port/eval/P6-B/rows_table.py [ROWS_DIR]   (default: rows/ next to it)
"""

from __future__ import annotations

import json
import math
import sys
from pathlib import Path

Z = 1.959964


def outcomes(doc: dict) -> list[tuple[bool, tuple[int, int, int]]]:
    out = []
    for row in doc["outcomes"]:
        # [index, position, challenger seat, winner, turns, first player, decisions, ...]
        out.append((row[3] == row[2], (row[3], row[4], row[6])))
    return out


def versus(cur: list, base: list) -> str:
    n = len(cur)
    changed = sum(1 for (_, ka), (_, kb) in zip(cur, base, strict=True) if ka != kb)
    b = sum(1 for (a, _), (p, _) in zip(cur, base, strict=True) if a and not p)
    c = sum(1 for (a, _), (p, _) in zip(cur, base, strict=True) if p and not a)
    d = (b - c) / n
    var = ((b + c) / n - d * d) / n
    half = Z * math.sqrt(max(var, 0.0))
    return f"{changed}; {b} / {c}; {d:+.4f} [{d - half:+.4f}, {d + half:+.4f}]"


def main() -> None:
    rows = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).with_name("rows")
    placebo_path = rows / "dev-placebo-400.json"
    base = outcomes(json.loads(placebo_path.read_text())) if placebo_path.exists() else None
    print(
        "| Row | wins/N (rate, Wilson 95%) | paired difference (Newcombe 95%) | discordant | "
        "iterations / applies per game (challenger vs opponent) | vs placebo: changed games; "
        "row-only / placebo-only wins; difference [95%] | evidence |"
    )
    print("| --- | --- | --- | --- | --- | --- | --- |")
    for path in sorted(rows.glob("*.json")):
        doc = json.loads(path.read_text())
        s = doc["summary"]
        lo, hi = s["ci95"]
        dlo, dhi = s["difference_ci95"]
        it, ap = s["iterations_per_game"], s["applies_per_game"]
        vs = "—"
        if base is not None and path.name.startswith("dev-") and path != placebo_path:
            cur = outcomes(doc)
            if len(cur) == len(base):
                vs = versus(cur, base)
        name = path.stem
        print(
            f"| `{name}` | {s['wins']}/{s['n']} ({s['rate']:.3f}, [{lo:.3f}, {hi:.3f}]) | "
            f"{s['difference']:+.3f} [{dlo:+.3f}, {dhi:+.3f}] | "
            f"{s['discordant'][0]}/{s['discordant'][1]} | "
            f"{it['challenger']:.0f} vs {it['opponent']:.0f} / "
            f"{ap['challenger']:.0f} vs {ap['opponent']:.0f} | {vs} | "
            f"`docs/port/eval/P6-B/rows/{path.name}` |"
        )


if __name__ == "__main__":
    main()
