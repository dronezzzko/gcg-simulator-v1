"""P6-F: each individual ablation row against the placebo row, game by game.

Every individual row of P6-F and the placebo row (the null configuration against ref:standard)
use the same master seed, so game i of every row has the same deal, seats and play/draw; the
challenger is always the agent `search-standard`, so its per-decision seeds are the same too. A
game differs between a row and the placebo only where the mechanism changed a decision. This
script counts, per row, the games whose outcome changed and the McNemar table of the challenger's
wins against the placebo's, with the paired difference and a 95% interval (Wald on the paired
differences, the same z as the rows).

Usage: python3 docs/port/eval/P6-F/versus_placebo.py [ROWS_DIR]   (default: the rows/ next to it)
"""

from __future__ import annotations

import json
import math
import sys
from pathlib import Path

Z = 1.959964


def challenger_wins(path: Path) -> list[tuple[bool, tuple[int, int, int]]]:
    doc = json.loads(path.read_text())
    out = []
    for row in doc["outcomes"]:
        # [index, position, challenger seat, winner, turns, first player, decisions, ...]
        out.append((row[3] == row[2], (row[3], row[4], row[6])))
    return out


def main() -> None:
    rows = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).with_name("rows")
    base = challenger_wins(rows / "dev-placebo-400.json")
    print(
        "row | games with another outcome or length | row-only wins | placebo-only wins | "
        "difference | 95% interval"
    )
    for path in sorted(rows.glob("dev-*-400.json")):
        name = path.name[len("dev-") : -len("-400.json")]
        if name == "placebo":
            continue
        cur = challenger_wins(path)
        assert len(cur) == len(base), path
        n = len(cur)
        changed = sum(1 for (a, ka), (b, kb) in zip(cur, base, strict=True) if ka != kb)
        b = sum(1 for (a, _), (p, _) in zip(cur, base, strict=True) if a and not p)
        c = sum(1 for (a, _), (p, _) in zip(cur, base, strict=True) if p and not a)
        d = (b - c) / n
        var = ((b + c) / n - d * d) / n
        half = Z * math.sqrt(max(var, 0.0))
        print(f"{name} | {changed} | {b} | {c} | {d:+.4f} | [{d - half:+.4f}, {d + half:+.4f}]")


if __name__ == "__main__":
    main()
