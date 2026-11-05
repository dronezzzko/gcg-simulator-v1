"""Run the general templates (``port/oracle/templates``) on every card with an effect (W-63, W-67).

Every binding card and every compiled card is compiled with the templates
(``templates.compile_card``):

- a binding card that compiles with a template is reproduced when its script equals the
  binding's byte for byte (``templates.behaviour_repr``); otherwise it is a mismatch, unless
  ``REVIEWED_DIFFERENCES`` lists it with the reason its binding differs from the general form
  (it then stays a binding, and the entry must still differ);
- a binding card with a template's wording and another line that nothing compiles is listed as
  covered only in part;
- a compiled card must compile to its committed script: a template never changes a compiled
  card;
- the compiler's tables hold no template entry afterwards.

The report lists, per template, the binding cards it reproduces, which is the "binding count
falling" of W-63's verification; ``docs/port/audit/templates.json`` holds the committed report,
which ``gcg-eval data-status --templates`` prints.

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

# Binding cards whose text a template covers in full but whose binding differs from the general
# form, with the reason. They stay bindings; retiring them would change their bytes.
REVIEWED_DIFFERENCES: dict[str, str] = {
    "EB01-074": (
        "all-enemy-players-each-choose: the binding marks the opponent's choice after_then=True "
        "with no 'Then' in the text (EB01-009 and GD05-049, same wording, have False)"
    ),
    "EB01-078": (
        "each-player-in-turn-order-looks-at-top: the binding looks for you (LookTop, top0) then "
        "the opponent (wp_eb01_b_look_top_own, top1); the template's turn order (EB01-023's "
        "binding: active then standby, wp_eb01_a_look_own_top) is the same order for a 【Main】 "
        "Command, played by the active player, in other bytes"
    ),
    "GD03-120": (
        "delayed-during-this-turn-when: the binding's trigger has no target filter and its "
        "selector lists HasTrait before IsRested; the compiler's 'destroys an enemy Unit' "
        "trigger has the Unit filter and its selector orders IsRested first"
    ),
    "GD04-022": (
        "are-deployed-rested: the binding lists IsToken before the Lv. filter; the compiler "
        "keeps the text's order (the Lv. clause comes first)"
    ),
    "GD03-129": (
        "mill-top-of-deck: the binding checks that this Base is active before 'you may rest this "
        "Base'; the compiler's May(Rest) has no check (outside the template's sentence)"
    ),
    "GD05-130": (
        "exile-this-card-in-trash: 'If you do, you may deploy 1 Base card ... from your hand' is "
        "an optional choice in the binding and a May around a choice in the compiler (outside "
        "the template's sentence)"
    ),
}


def _uses_wording(effect: str) -> bool:
    return any(T.wording(t).search(line) for t in T.TEMPLATES for line in effect.split("\n"))


def report() -> dict[str, Any]:
    reg = get_registry()
    reproduced: dict[str, list[str]] = {t.id: [] for t in T.TEMPLATES}
    mismatches: list[str] = []
    partial: list[str] = []
    differences: list[str] = []
    compiler_reproduces: list[str] = []
    bindings = 0
    compiled_with_wording: list[str] = []
    for entry in reg.cards:
        script = entry.script
        if script is None or script.source not in ("binding", "compiled"):
            continue
        cdef = reg.db.by_id(entry.def_id)
        uses_wording = _uses_wording(cdef.effect)
        if script.source == "compiled":
            if uses_wording:
                compiled_with_wording.append(cdef.card_number)
            # A template must never change a card the compiler already compiles.
            got, _ = T.compile_card(cdef)
            if T.behaviour_repr(got) != T.behaviour_repr(script):
                mismatches.append(f"{cdef.card_number}: a template changes a compiled card")
            continue
        bindings += 1
        try:
            got, used = T.compile_card(cdef)
        except CompileError as exc:
            if uses_wording:
                partial.append(f"{cdef.card_number}: {exc}")
            continue
        same = T.behaviour_repr(got) == T.behaviour_repr(script)
        if not used:
            if same:
                compiler_reproduces.append(cdef.card_number)
            continue
        listed = cdef.card_number in REVIEWED_DIFFERENCES
        if same and listed:
            mismatches.append(f"{cdef.card_number}: listed as differing, but reproduced")
        elif same:
            for template in used:
                reproduced[template].append(cdef.card_number)
        elif listed:
            differences.append(f"{cdef.card_number}: {REVIEWED_DIFFERENCES[cdef.card_number]}")
        else:
            mismatches.append(f"{cdef.card_number}: the template script differs from the binding")
    if not T.tables_untouched():
        mismatches.append("the compiler's tables still hold a template entry")
    return {
        "version": 1,
        "generator": "port/oracle/templates/check.py",
        "binding_cards": bindings,
        "templates": [
            {
                "id": t.id,
                "entry": t.entry,
                "family": t.family,
                "kind": T.kind(t),
                "reproduced_bindings": sorted(reproduced[t.id]),
            }
            for t in T.TEMPLATES
        ],
        "reproduced_total": len({n for v in reproduced.values() for n in v}),
        "wording_covered_other_lines_not_compiled": partial,
        "reviewed_differences": differences,
        "compiler_reproduces_bindings": compiler_reproduces,
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
