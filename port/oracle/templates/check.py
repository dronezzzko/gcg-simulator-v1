"""Run the general templates (``port/oracle/templates``) on every binding card (W-63, W-67).

For each binding card whose text uses a template's wording and whose other lines the compiler
compiles, the templates' script must equal the binding's byte for byte
(``templates.behaviour_repr``); a card with another line that nothing compiles is listed as
covered only in part. The report lists, per template,
the binding cards it reproduces, which is the "binding count falling" of W-63's verification;
``docs/port/audit/templates.json`` holds the committed report.

    uv run python port/oracle/templates/check.py --write
    uv run python port/oracle/templates/check.py --check    # exit 1 on a stale report or a mismatch
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import templates as T
from gcg_sim.effects.compiler.abilities import CompileError
from gcg_sim.effects.registry import get_registry

ROOT = Path(__file__).resolve().parents[3]
REPORT = ROOT / "docs" / "port" / "audit" / "templates.json"


def report() -> dict[str, Any]:
    reg = get_registry()
    reproduced: dict[str, list[str]] = {t.id: [] for t in T.TEMPLATES}
    mismatches: list[str] = []
    partial: list[str] = []
    bindings = 0
    compiled_with_wording: list[str] = []
    for entry in reg.cards:
        script = entry.script
        if script is None or script.source not in ("binding", "compiled"):
            continue
        cdef = reg.db.by_id(entry.def_id)
        uses_wording = any(
            t.pattern.search(line) for t in T.TEMPLATES for line in cdef.effect.split("\n")
        )
        if script.source == "compiled":
            if uses_wording:
                # A template must never change a card the compiler already compiles.
                compiled_with_wording.append(cdef.card_number)
                got, _ = T.compile_card(cdef)
                if T.behaviour_repr(got) != T.behaviour_repr(script):
                    mismatches.append(f"{cdef.card_number}: a template changes a compiled card")
            continue
        bindings += 1
        if not uses_wording:
            continue
        try:
            got, used = T.compile_card(cdef)
        except CompileError as exc:
            partial.append(f"{cdef.card_number}: {exc}")
            continue
        if T.behaviour_repr(got) != T.behaviour_repr(script):
            mismatches.append(f"{cdef.card_number}: the template script differs from the binding")
            continue
        for template in used:
            reproduced[template].append(cdef.card_number)
    return {
        "version": 1,
        "generator": "port/oracle/templates/check.py",
        "binding_cards": bindings,
        "templates": [
            {
                "id": t.id,
                "entry": t.entry,
                "family": t.family,
                "reproduced_bindings": sorted(reproduced[t.id]),
            }
            for t in T.TEMPLATES
        ],
        "reproduced_total": len({n for v in reproduced.values() for n in v}),
        "wording_covered_other_lines_not_compiled": partial,
        "compiled_cards_with_a_template_wording": compiled_with_wording,
        "mismatches": mismatches,
    }


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(description="Check the port's general compiler templates.")
    mode = ap.add_mutually_exclusive_group(required=True)
    mode.add_argument("--write", action="store_true")
    mode.add_argument("--check", action="store_true")
    args = ap.parse_args(argv)
    doc = report()
    text = json.dumps(doc, indent=1, ensure_ascii=False) + "\n"
    if doc["mismatches"]:
        for m in doc["mismatches"]:
            print(f"templates/check: {m}", file=sys.stderr)
        return 1
    if args.write:
        REPORT.write_text(text, encoding="utf-8")
        return 0
    if not REPORT.is_file() or REPORT.read_text(encoding="utf-8") != text:
        print(f"templates/check: {REPORT} is stale; run with --write", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
