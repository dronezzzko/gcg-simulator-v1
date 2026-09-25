"""Write or check golden files of compiled card behaviour.

uv run python -m gcg_sim.tools.golden --check
uv run python -m gcg_sim.tools.golden --write [--prefix GD01]
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any

from gcg_sim.effects.golden import card_golden, prefix_of
from gcg_sim.effects.registry import get_registry
from gcg_sim.effects.scripts import binding_module

DEFAULT_DIR = Path("tests/effects/golden")


def build(prefix: str | None = None) -> dict[str, dict[str, Any]]:
    reg = get_registry()
    out: dict[str, dict[str, Any]] = {}
    for cdef in reg.db.real_cards():
        p = prefix_of(cdef.card_number)
        if prefix is not None and p != prefix:
            continue
        out.setdefault(p, {})[cdef.card_number] = card_golden(
            cdef, reg.cards[cdef.def_id], binding_module(cdef.card_number)
        )
    return out


def dump(data: dict[str, Any]) -> str:
    return json.dumps(data, indent=1, sort_keys=True, ensure_ascii=False) + "\n"


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--write", action="store_true")
    ap.add_argument("--check", action="store_true")
    ap.add_argument("--prefix")
    ap.add_argument("--dir", type=Path, default=DEFAULT_DIR)
    args = ap.parse_args(argv)
    data = build(args.prefix)
    bad = 0
    for p, cards in sorted(data.items()):
        path = args.dir / f"{p}.json"
        text = dump(cards)
        if args.write:
            args.dir.mkdir(parents=True, exist_ok=True)
            path.write_text(text, encoding="utf-8")
        else:
            current = path.read_text(encoding="utf-8") if path.exists() else ""
            if current != text:
                bad += 1
                print(f"golden out of date: {path}", file=sys.stderr)
    if args.write:
        print(f"wrote {len(data)} golden files to {args.dir}")
    return 1 if bad else 0


if __name__ == "__main__":
    raise SystemExit(main())
