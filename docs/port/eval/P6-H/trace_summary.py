"""Summarise the CHOOSE_FIRST decisions of a head-to-head run (W-44's e2e, P6-H evidence).

    python3 docs/port/eval/P6-H/trace_summary.py REPORT.json TRACE.ndjson GAMES

REPORT.json is a `gcg-eval head-to-head --out` report run with `--choose-first agents`, and
TRACE.ndjson its `--decision-trace`. GAMES is the agents' `first_player_games`. For each deck
pair and agent the script prints, as JSON, how often the agent chose, how often it went first,
and the play/draw simulations its choices counted (the alternatives' `visits`) and excluded
(`GAMES - visits`, W-44's unfinished games); the report's realised play/draw counts per agent are
copied next to them.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path


def main(report_path: str, trace_path: str, games: int) -> None:
    report = json.loads(Path(report_path).read_text(encoding="utf-8"))
    pair_of = {row[0]: row[1] for row in report["outcomes"]}
    names = {p["position"]: " vs ".join(p["decks"]) for p in report["by_pair"]}
    summary: dict[str, dict[str, dict[str, int]]] = {}
    for line in Path(trace_path).read_text(encoding="utf-8").splitlines():
        rec = json.loads(line)
        if rec["kind"] != "choose_first":
            continue
        pair = names[pair_of[rec["game"]]]
        agent = summary.setdefault(pair, {}).setdefault(
            rec["agent"], {"choices": 0, "went_first": 0, "counted": 0, "excluded": 0}
        )
        agent["choices"] += 1
        agent["went_first"] += int(rec["chosen"][1] == rec["player"])
        visits = rec["alternatives"][0][2]
        agent["counted"] += visits
        agent["excluded"] += games - visits
    on_play = {
        names[p["position"]]: {
            side: p["result"]["play_draw"][side]["on_play"]["n"]
            for side in ("challenger", "opponent")
        }
        for p in report["by_pair"]
    }
    totals: dict[str, dict[str, int]] = {}
    for agents in summary.values():
        for name, counts in agents.items():
            t = totals.setdefault(name, dict.fromkeys(counts, 0))
            for k, v in counts.items():
                t[k] += v
    out = {
        "report": report_path,
        "challenger": report["challenger"],
        "opponent": report["opponent"],
        "agents": report["agents"],
        "seed": report["seed"],
        "games": report["games"],
        "choose_first": report["choose_first"],
        "first_player_games": games,
        "by_pair": {
            pair: {"choices": summary.get(pair, {}), "on_play_games": on_play[pair]}
            for pair in names.values()
        },
        "totals": totals,
    }
    print(json.dumps(out, indent=1))


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2], int(sys.argv[3]))
