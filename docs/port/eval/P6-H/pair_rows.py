"""Pair two `gcg-eval ablation` reports of the same seed game by game (P6-H evidence).

    python3 docs/port/eval/P6-H/pair_rows.py MECHANISM_ROW.json CONTROL_ROW.json

Both rows must play the same specs (same seed, pairs, games and play/draw protocol) with
challengers of the same agent name, so their per-decision seeds agree. The control row is the
mechanism's flag set to false (the null configuration), so a game whose decisions never differ is
replayed exactly and both rows report the same outcome row. The script prints, as JSON:

- the games whose outcome rows differ (the games where the mechanism changed a decision);
- among them, the challenger's wins with the mechanism and in the control, and McNemar's
  discordant counts (won only with the mechanism / won only in the control);
- the exact two-sided sign-test p-value of those discordant counts;
- the mechanism's effect on the whole row: (wins with - wins without) / games, with the Wald 95%
  interval of a difference of paired proportions, `d +- 1.96 * sqrt(b + c - (b - c)**2 / N) / N`
  (b, c the discordant counts, N the games).

Outcome rows are `[index, pair, challenger seat, winner, turns, first player, decisions,
challenger iterations, challenger applies, opponent iterations, opponent applies]`
(`crates/gcg-eval/src/head_to_head.rs`, `Played::blocks`).
"""

from __future__ import annotations

import json
import math
import sys
from pathlib import Path


def sign_test(b: int, c: int) -> float:
    n = b + c
    if n == 0:
        return 1.0
    k = min(b, c)
    tail = sum(math.comb(n, i) for i in range(k + 1)) / 2**n
    return min(1.0, 2 * tail)


def main(mechanism_path: str, control_path: str) -> None:
    mech = json.loads(Path(mechanism_path).read_text(encoding="utf-8"))
    ctl = json.loads(Path(control_path).read_text(encoding="utf-8"))
    for key in ("seed", "games", "pairs", "decks", "choose_first", "reference"):
        assert mech[key] == ctl[key], f"{key} differs: {mech[key]!r} vs {ctl[key]!r}"
    assert mech["agents"] == ctl["agents"], "the challengers must share their agent name"
    rows_m = {r[0]: r for r in mech["outcomes"]}
    rows_c = {r[0]: r for r in ctl["outcomes"]}
    assert rows_m.keys() == rows_c.keys()
    changed = [i for i in sorted(rows_m) if rows_m[i] != rows_c[i]]

    def won(row: list) -> bool:
        return row[3] == row[2]

    wins_m = sum(won(rows_m[i]) for i in changed)
    wins_c = sum(won(rows_c[i]) for i in changed)
    only_m = sum(won(rows_m[i]) and not won(rows_c[i]) for i in changed)
    only_c = sum(won(rows_c[i]) and not won(rows_m[i]) for i in changed)
    games = len(rows_m)
    d = (only_m - only_c) / games
    half = 1.96 * math.sqrt(max(0.0, only_m + only_c - (only_m - only_c) ** 2 / games)) / games
    lo, hi = d - half, d + half
    out = {
        "mechanism_row": mechanism_path,
        "control_row": control_path,
        "seed": mech["seed"],
        "games": games,
        "choose_first": mech["choose_first"],
        "mechanism_summary": mech["summary"]["text"],
        "control_summary": ctl["summary"]["text"],
        "changed_games": len(changed),
        "changed_game_indices": changed,
        "challenger_wins_in_changed_games": {"mechanism": wins_m, "control": wins_c},
        "discordant": {"mechanism_only": only_m, "control_only": only_c},
        "sign_test_p": round(sign_test(only_m, only_c), 4),
        "effect_on_row": round((wins_m - wins_c) / games, 4),
        "effect_ci95_wald": [round(lo, 4), round(hi, 4)],
    }
    print(json.dumps(out, indent=1))


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
