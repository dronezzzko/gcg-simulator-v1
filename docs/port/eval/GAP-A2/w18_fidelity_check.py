"""W-18: a `gcg-eval fidelity` report against the entry's verification.

The verification (docs/port/WEAKNESSES.md W-18): wherever strengthened `standard` resolves a
matchup (its Wilson 95% interval excludes 0.5), strengthened `fast` favours the same deck (its
rate is on the same side of 0.5), and every paired difference interval (fast minus standard,
Newcombe 95%) includes 0 or lies within +-0.05. This prints the per-variant table (Markdown),
the rank agreement, and each check; it exits 1 when a check fails.

Usage: uv run python docs/port/eval/GAP-A2/w18_fidelity_check.py REPORT.json
"""

from __future__ import annotations

import json
import sys
from pathlib import Path
from typing import Any

BOUND = 0.05


def result_cell(r: dict[str, Any]) -> str:
    lo, hi = r["ci95"]
    return f"{r['wins']}/{r['n']} ({r['rate']:.3f}, [{lo:.3f}, {hi:.3f}])"


def check(doc: dict[str, Any]) -> tuple[list[str], bool]:
    """The report's table and checks as lines, and whether every check holds."""
    agents = doc["agents"]
    fast, standard = doc["by_agent"][0]["results"], doc["by_agent"][1]["results"]
    lines = [
        f"agents {agents[0]} (fast) and {agents[1]} (standard) against {doc['against']}, "
        f"{doc['games_per_variant']} games per variant and agent, seed {doc['seed']} "
        f"({doc['role']})",
        "",
        f"| Variant | {agents[0]} | {agents[1]} | paired difference (Newcombe 95%) "
        "| standard resolves | fast favours the same deck | difference check |",
        "| --- | --- | --- | --- | --- | --- | --- |",
    ]
    ok = True
    for i, v in enumerate(doc["variants"]):
        f, s = fast[i], standard[i]
        p = doc["by_variant"][i]["paired"]
        lo, hi = p["ci95"]
        resolves = s["ci95"][0] > 0.5 or s["ci95"][1] < 0.5
        same = (f["rate"] - 0.5) * (s["rate"] - 0.5) > 0 if resolves else True
        diff_ok = lo <= 0.0 <= hi or max(abs(lo), abs(hi)) < BOUND
        ok = ok and same and diff_ok
        lines.append(
            f"| `{v}` | {result_cell(f)} | {result_cell(s)} | "
            f"{p['difference']:+.3f} [{lo:+.3f}, {hi:+.3f}] | "
            f"{'yes' if resolves else 'no'} | {'yes' if same else 'NO'} | "
            f"{'pass' if diff_ok else 'FAIL'} |"
        )
    lines += [
        "",
        f"Spearman {doc['spearman']}, Kendall tau-b {doc['kendall_tau_b']}, pairwise "
        f"agreement {doc['pairwise_agreement']}, same best variant {doc['same_best']}.",
        f"Verification: {'met' if ok else 'NOT met'}.",
    ]
    return lines, ok


def main(argv: list[str]) -> int:
    if len(argv) != 2:
        print(__doc__, file=sys.stderr)
        return 2
    doc = json.loads(Path(argv[1]).read_text(encoding="utf-8"))
    lines, ok = check(doc)
    print("\n".join(lines))
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
