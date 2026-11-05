"""W-31: the per-keyword worth implied by the W-37 keyword fits (docs/port/eval/P6-G/).

The keyword models of `gcg-eval tune-leaf` (`kw=1`) add, per keyword, the summed amount on the
turn player's battlefield (`tp_kw_*`) and on the other player's (`ot_kw_*`) to the role
features, from the turn player's point of view. A keyword's worth to its owner is then `w_tp`
for the turn player and `-w_ot` for the other player, in log-odds per keyword amount. The
fitted `ap` and `hp` weights are log-odds per point of the battlefield AP and remaining-HP
differences, and the policy's `unit_value` counts one such point as 0.5
(`0.5 * (ap + hp_left)`, `ai/values.py:22-33`, `crates/gcg-ai/src/values.rs`). So
`0.5 * worth / mean(w_ap, w_hp)` is the keyword's worth in the policy's own units, next to the
flat `KEYWORD_BONUS` of 0.3 per keyword.

Usage: uv run python docs/port/eval/GAP-A2/w31_keyword_worth.py [FIT.json]
"""

from __future__ import annotations

import json
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[4]
DEFAULT = ROOT / "docs/port/eval/P6-G/fit-round1.json"
KEYWORDS = (
    "repair",
    "breach",
    "support",
    "blocker",
    "first_strike",
    "high_maneuver",
    "suppression",
)
FLAT_BONUS = 0.3
POINT = 0.5


def keyword_models(doc: dict[str, Any]) -> list[dict[str, Any]]:
    """The fitted models whose only extra block is the keyword one."""
    out = []
    for m in doc["models"]:
        key = dict(part.split("=") for part in m["key"].split(","))
        extra = {k for k, v in key.items() if v == "1"} - {"int", "det"}
        if extra == {"kw"} and key.get("ph") == "0":
            out.append(m)
    return out


def table(m: dict[str, Any]) -> list[str]:
    """The worth table of one model as Markdown rows."""
    w: dict[str, float] = m["weights"][0]
    stat = (w["ap"] + w["hp"]) / 2.0
    unit = stat / POINT
    rows = [
        f"model {m['key']} ({m['mode']}, lambda {m['lambda']}): "
        f"ap {w['ap']:.4f}, hp {w['hp']:.4f} log-odds per point; "
        f"one policy unit = {unit:.4f} log-odds",
        "",
        "| Keyword | owner = turn player: log-odds (policy units) "
        "| owner = other player: log-odds (policy units) | same sign |",
        "| --- | --- | --- | --- |",
    ]
    for kw in KEYWORDS:
        tp = w[f"tp_kw_{kw}"]
        ot = 0.0 - w[f"ot_kw_{kw}"]
        same = "yes" if tp * ot > 0 else ("no data" if tp == 0.0 and ot == 0.0 else "no")
        rows.append(
            f"| {kw} | {tp:+.4f} ({tp / unit:+.2f}) | {ot:+.4f} ({ot / unit:+.2f}) | {same} |"
        )
    rows.append(
        f"| flat bonus (reference policy) | ({FLAT_BONUS:+.2f}) | ({FLAT_BONUS:+.2f}) | yes |"
    )
    return rows


def main(argv: list[str]) -> int:
    path = Path(argv[1]) if len(argv) > 1 else DEFAULT
    doc = json.loads(path.read_text(encoding="utf-8"))
    models = keyword_models(doc)
    if not models:
        print(f"{path}: no keyword-only model", file=sys.stderr)
        return 1
    for m in models:
        print("\n".join(table(m)))
        print()
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
