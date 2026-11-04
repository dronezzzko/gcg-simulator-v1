"""W-17 step (3): unblind and count the blinded audit (docs/port/WEAKNESSES.md W-17).

Reads the auditors' classifications (`auditors/auditor-N.json`), the packet assignment
(`assignment.json`, written by `prepare_audit.py`), the transcripts' key (`key.json`, written by
`gcg-eval transcripts`) and the rule-based audit (`audit-report.json`), and writes
`audit-result.json`. Only findings with the verdict `clear` count as confirmed implausible plays.

Per AI version (`search-standard`, the shipped strengthened `standard` of M15b, and
`mcts-standard`, the reference `standard`):

- full reads (the stratified sub-sample, 6 games per version): confirmed findings per game
  (mean and standard error), the games with at least one confirmed finding (Wilson 95%), and
  the counts per category;
- rule-based candidates (every flagged turn of the 60 games per version): the candidates and
  how many the auditors confirmed. A candidate in a fully read game counts as confirmed when
  that game's audit has a clear finding of the flagged player in the flagged turn. The games
  with a confirmed missed lethal are reported over all 60 games (Wilson 95%).

    uv run python docs/port/eval/final-2/audit/tally_audit.py
"""

from __future__ import annotations

import json
import math
import sys
from collections import Counter
from pathlib import Path
from typing import Any

HERE = Path(__file__).resolve().parent
VERSIONS = ("search-standard", "mcts-standard")
CATEGORIES = (
    "missed_lethal",
    "passivity",
    "questionable_attack",
    "wasted_effect",
    "bad_target",
    "other",
)


def wilson(k: int, n: int, z: float = 1.96) -> list[float]:
    if n == 0:
        return [0.0, 1.0]
    p = k / n
    d = 1 + z * z / n
    c = (p + z * z / (2 * n)) / d
    h = z * math.sqrt(p * (1 - p) / n + z * z / (4 * n * n)) / d
    return [round(max(0.0, c - h), 4), round(min(1.0, c + h), 4)]


def version_of(path: str) -> str:
    for v in VERSIONS:
        if f"/{v}/" in path:
            return v
    raise ValueError(path)


def main() -> int:
    key: dict[str, list[str]] = json.loads((HERE / "key.json").read_text())
    assignment: dict[str, list[dict[str, Any]]] = json.loads((HERE / "assignment.json").read_text())
    report = json.loads((HERE / "audit-report.json").read_text())
    version = {gid: version_of(paths[0]) for gid, paths in key.items()}
    by_path = {paths[0]: gid for gid, paths in key.items()}

    findings: dict[str, list[dict[str, Any]]] = {}
    for auditor, items in sorted(assignment.items()):
        doc = json.loads((HERE / "auditors" / f"{auditor}.json").read_text())
        got = {it["file"]: it for it in doc["items"]}
        for item in items:
            if item["file"] not in got:
                raise SystemExit(f"{auditor} did not report {item['file']}")
            findings[item["file"]] = [
                f for f in got[item["file"]]["findings"] if f["verdict"] == "clear"
            ]

    full = {
        it["game"]: it["file"]
        for items in assignment.values()
        for it in items
        if it["kind"] == "full"
    }
    excerpts = {
        (it["game"], it["turn"], it["player"]): it["file"]
        for items in assignment.values()
        for it in items
        if it["kind"] == "excerpt"
    }

    result: dict[str, Any] = {"format": "w17-audit-result", "version": 1, "versions": {}}
    for v in VERSIONS:
        games = sorted(g for g in full if version[g] == v)
        per_game = [len(findings[full[g]]) for g in games]
        cats = Counter(f["category"] for g in games for f in findings[full[g]])
        n = len(per_game)
        mean = sum(per_game) / n if n else 0.0
        var = sum((x - mean) ** 2 for x in per_game) / (n - 1) if n > 1 else 0.0
        with_one = sum(1 for x in per_game if x > 0)

        candidates = 0
        confirmed = 0
        lethal_games: set[str] = set()
        sample_games = sum(1 for g in version.values() if g == v)
        for run in report["runs"]:
            if version_of(run["run"] + "/") != v:
                continue
            for g in run["flagged_games"]:
                gid = by_path[f"{run['run']}/replays/{g['file']}"]
                flags = sorted(
                    {
                        (f[0], f[1], kind)
                        for kind in ("missed_lethal", "wasted_effect")
                        for f in g[kind]
                    }
                    | {(f[0], f[1], "self_harm_target") for f in g["self_harm_target"]}
                )
                for player, turn, kind in flags:
                    candidates += 1
                    if gid in full:
                        hits = [
                            f
                            for f in findings[full[gid]]
                            if f["turn"] == turn and f["player"] == player
                        ]
                    else:
                        hits = findings[excerpts[(gid, turn, player)]]
                    if hits:
                        confirmed += 1
                    if kind == "missed_lethal" and any(
                        f["category"] == "missed_lethal" for f in hits
                    ):
                        lethal_games.add(gid)
        for g in games:
            if any(f["category"] == "missed_lethal" for f in findings[full[g]]):
                lethal_games.add(g)

        result["versions"][v] = {
            "full_reads": {
                "games": n,
                "confirmed_per_game": per_game,
                "mean": round(mean, 4),
                "se": round(math.sqrt(var / n), 4) if n else 0.0,
                "games_with_a_confirmed_finding": {
                    "wins": with_one,
                    "n": n,
                    "rate": round(with_one / n, 4) if n else 0.0,
                    "ci95": wilson(with_one, n),
                },
                "by_category": {c: cats.get(c, 0) for c in CATEGORIES},
            },
            "rule_candidates": {
                "candidates": candidates,
                "confirmed": confirmed,
            },
            "games_with_a_confirmed_missed_lethal": {
                "wins": len(lethal_games),
                "n": sample_games,
                "rate": round(len(lethal_games) / sample_games, 4),
                "ci95": wilson(len(lethal_games), sample_games),
                "note": "rule-flagged turns of all games plus the full reads; a missed lethal outside a flagged turn of an unread game is not counted",
            },
        }
    (HERE / "audit-result.json").write_text(json.dumps(result, indent=1) + "\n")
    print(json.dumps(result["versions"], indent=1))
    return 0


if __name__ == "__main__":
    sys.exit(main())
