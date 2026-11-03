"""W-13: count the Rest-target prune branches of a `gcg-eval --decision-trace` file.

Each searched decision of the trace carries `rest_prune`: `"pruned"`, `"rest_trigger"`,
`"while_rested"` or `null` (the prune does not apply). This prints, as sorted JSON, the counts
per agent and branch, the number of games in which each branch occurred, and the trace size.

Usage: uv run python docs/port/eval/GAP-A2/w13_trace_summary.py TRACE.ndjson [--games N]
"""

from __future__ import annotations

import argparse
import json
from collections import Counter, defaultdict
from pathlib import Path


def summary(path: Path, games: int | None) -> dict[str, object]:
    """The branch counts of the trace at `path`."""
    by_agent: dict[str, Counter[str]] = defaultdict(Counter)
    games_with: dict[str, set[int]] = defaultdict(set)
    lines = searched = 0
    seen_games: set[int] = set()
    with path.open(encoding="utf-8") as f:
        for line in f:
            rec = json.loads(line)
            lines += 1
            seen_games.add(rec["game"])
            if rec["search"] is None:
                continue
            searched += 1
            branch = rec["rest_prune"] or "none"
            by_agent[rec["agent"]][branch] += 1
            if branch != "none":
                games_with[branch].add(rec["game"])
    total: Counter[str] = Counter()
    for c in by_agent.values():
        total.update(c)
    return {
        "trace_lines": lines,
        "searched_decisions": searched,
        "games": games if games is not None else len(seen_games),
        "by_agent": {a: dict(sorted(c.items())) for a, c in sorted(by_agent.items())},
        "total": dict(sorted(total.items())),
        "games_with": {b: len(g) for b, g in sorted(games_with.items())},
    }


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("trace", type=Path)
    ap.add_argument("--games", type=int, default=None)
    args = ap.parse_args()
    print(json.dumps(summary(args.trace, args.games), indent=1, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
