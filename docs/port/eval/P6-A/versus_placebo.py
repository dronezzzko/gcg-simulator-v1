"""P6-A: each individual ablation row against the placebo row, game by game.

The individual rows of P6-A and the placebo row (`engine_error_leaf=false`: the null
configuration against ref:standard) share their master seed (dev 20100100 for the 400-game rows,
confirm 30100100 for the 1,600-game confirmations), so game i of every row of a family has the
same deal, seats and play/draw, and the challenger is always the agent `search-standard` with the
same per-decision seeds: a game differs from the placebo only where the mechanism changed a
decision. The high-budget rows (dev 20100200) have the same structure with the row `capped` as
their placebo: `capped_rollout_continuation` never fires on the panel (0 capped rollouts in every
report), so that row's challenger plays as its opponent, the shipped `standard` bundle at twice
its budget; every other high-budget row's opponent is that same configuration. Per row this
prints the games whose winner, length or decision count changed, the McNemar counts (challenger
wins only in the row / only in the placebo) and the paired difference per game with a Wald 95%
interval (the method of docs/port/eval/P6-F/versus_placebo.py).

Usage: uv run python docs/port/eval/P6-A/versus_placebo.py [ROWS_DIR]   (default: the rows/ next to
it)
"""

from __future__ import annotations

import json
import math
import sys
from pathlib import Path

Z = 1.959964
FAMILIES = (
    ("dev-individual-", "-400.json", "placebo"),
    ("confirm-individual-", "-1600.json", "placebo"),
    ("dev-highbudget-", "-400.json", "capped"),
)


def challenger_wins(path: Path) -> list[tuple[bool, tuple[int, int, int]]]:
    doc = json.loads(path.read_text())
    out = []
    for row in doc["outcomes"]:
        # [index, position, challenger seat, winner, turns, first player, decisions, ...]
        out.append((row[3] == row[2], (row[3], row[4], row[6])))
    return out


def compare(rows: Path, prefix: str, suffix: str, placebo_name: str) -> None:
    placebo = rows / f"{prefix}{placebo_name}{suffix}"
    if not placebo.exists():
        return
    base = challenger_wins(placebo)
    for path in sorted(rows.glob(f"{prefix}*{suffix}")):
        name = path.name[len(prefix) : -len(suffix)]
        if name == placebo_name:
            continue
        cur = challenger_wins(path)
        assert len(cur) == len(base), path
        n = len(cur)
        changed = sum(1 for (_, ka), (_, kb) in zip(cur, base, strict=True) if ka != kb)
        b = sum(1 for (a, _), (p, _) in zip(cur, base, strict=True) if a and not p)
        c = sum(1 for (a, _), (p, _) in zip(cur, base, strict=True) if p and not a)
        d = (b - c) / n
        var = ((b + c) / n - d * d) / n
        half = Z * math.sqrt(max(var, 0.0))
        print(
            f"{prefix}{name} | {n} | {changed} | {b} | {c} | {d:+.4f} | "
            f"[{d - half:+.4f}, {d + half:+.4f}]"
        )


def main() -> None:
    rows = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).with_name("rows")
    print(
        "row | games | changed games | row-only wins | placebo-only wins | difference | "
        "95% interval"
    )
    for prefix, suffix, placebo_name in FAMILIES:
        compare(rows, prefix, suffix, placebo_name)


if __name__ == "__main__":
    main()
