"""Export the Python-side vectors of the P6-K data and effects tripwires (docs/dev/PLAN.md
§12.2 P6-K, docs/dev/eval/weakness-status/P6-K.md).

Each vector is computed from the frozen Python package (read-only imports; nothing under
``src/gcg_sim`` is edited) and read by a Rust test that checks the Rust side against it:

- ``p6k_parentheticals.json`` (W-60, W-155): every top-level parenthetical that
  ``effects.text.normalize`` strips from a card's effective text, recorded by wrapping
  ``text._is_reminder`` in this process (``text.py:17-28``, called from ``strip_reminders``,
  ``:31-62``), keyed by its whitespace-normalised text, with the card numbers that carry it.
  ``crates/gcg-data/tests/p6k_data_tripwires.rs`` reimplements the extraction and must find the same.
- ``p6k_vocabulary.json`` (W-65): every keyword and timing vocabulary the Python package keeps
  (``dsl.Kw``, ``dsl.STACKING_KEYWORDS``, ``dsl.Timing``, the compiler regexes
  ``abilities._KW``, ``nouns.KW_RE`` and ``grammar._KWS``, and ``sources.conflicts``'
  ``KEYWORD_VOCABULARY`` and ``TIMING_VOCABULARY``). ``crates/gcg-effects/tests/
  p6k_ir_scans.rs`` checks that the Rust single source (``gcg_effects::Kw``, ``Timing``)
  derives each of them.
- ``p6k_hp_fragments.json`` (W-68): for each card whose binding only fixes the printed-HP
  reading, the script the Python compiler gives it (``compiler.compile_card``) as an IR
  fragment (``export_ir.build_fragment`` over the committed IR, one fresh process per card),
  with both scripts' ``repr``. ``crates/gcg-engine/tests/p6k_behaviour.rs`` runs the bound
  and the compiled script on the same HP/damage states.

    uv run python oracle/p6k_vectors.py --write
    uv run python oracle/p6k_vectors.py --check    # exit 1 if a committed vector is stale
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
OUT_DIR = ROOT / "oracle" / "tests" / "vectors"
IR_PATH = ROOT / "crates" / "gcg-effects" / "ir" / "registry.json"
# Bindings whose note says only that the compiled HP filter read printed HP (W-68):
# wp_gd01_a.py:78-85, wp_st01_04.py:19 (_CURRENT_HP_NOTE), wp_gd05_b.py:164, :186.
HP_FIX_CARDS = (
    "GD01-004",
    "ST01-004",
    "ST01-010",
    "ST02-014",
    "ST04-001",
    "ST04-013",
    "GD05-072",
    "GD05-083",
)
VECTORS = ("p6k_parentheticals.json", "p6k_vocabulary.json", "p6k_hp_fragments.json")


def dump(obj: object) -> str:
    return json.dumps(obj, indent=1, sort_keys=True, ensure_ascii=True) + "\n"


def parentheticals() -> dict[str, Any]:
    from gcg_sim.cards.db import get_card_db
    from gcg_sim.effects import text

    original = text._is_reminder
    recorded: list[str] = []

    def recording(inner: str) -> bool:
        result = original(inner)
        if result:
            recorded.append(inner)
        return result

    by_text: dict[str, list[str]] = {}
    text._is_reminder = recording
    try:
        for cdef in get_card_db():
            recorded.clear()
            text.normalize(cdef.effect)
            for inner in recorded:
                key = " ".join(inner.split())
                numbers = by_text.setdefault(key, [])
                if cdef.card_number not in numbers:
                    numbers.append(cdef.card_number)
    finally:
        text._is_reminder = original
    return {
        "version": 1,
        "generator": "oracle/p6k_vectors.py",
        "source": "gcg_sim.effects.text.normalize over every CardDef of get_card_db()",
        "stripped": {k: sorted(v) for k, v in sorted(by_text.items())},
    }


def _alternatives(pattern: str) -> list[str]:
    m = re.search(r"<\(([^)]*)\)", pattern)
    if m is None:
        raise SystemExit(f"p6k_vectors: no keyword alternation in {pattern!r}")
    return m.group(1).split("|")


def vocabulary() -> dict[str, Any]:
    from gcg_sim.effects import dsl as d
    from gcg_sim.effects.compiler import abilities, grammar, nouns
    from gcg_sim.sources import conflicts

    return {
        "version": 1,
        "generator": "oracle/p6k_vectors.py",
        "keywords": {
            "dsl.Kw": [k.value for k in d.Kw],
            "dsl.STACKING_KEYWORDS": sorted(k.value for k in d.STACKING_KEYWORDS),
            "compiler.abilities._KW": _alternatives(abilities._KW.pattern),
            "compiler.nouns.KW_RE": _alternatives(nouns.KW_RE),
            "compiler.grammar._KWS": _alternatives(grammar._KWS.pattern),
            "sources.conflicts.KEYWORD_VOCABULARY": list(conflicts.KEYWORD_VOCABULARY),
        },
        "timings": {
            "dsl.Timing": [t.value for t in d.Timing],
            "sources.conflicts.TIMING_VOCABULARY": list(conflicts.TIMING_VOCABULARY),
        },
    }


def hp_fragment(number: str) -> dict[str, Any]:
    """One card's compiled script as an IR fragment, in this (fresh) process."""
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    import export_ir

    from gcg_sim.effects.compiler import compile_card
    from gcg_sim.effects.registry import get_registry, override_script, restore_entry

    reg = get_registry()
    cdef = reg.db[number]
    bound = reg.cards[cdef.def_id].script
    compiled = compile_card(cdef)
    base = json.loads(IR_PATH.read_text(encoding="utf-8"))
    previous = override_script(reg, cdef, compiled)
    try:
        frag = export_ir.build_fragment(reg, base, [cdef.def_id])
    finally:
        restore_entry(reg, previous)
    return {
        "card_number": number,
        "bound": repr(bound),
        "compiled": repr(compiled),
        "fragment": frag,
    }


def hp_fragments() -> dict[str, Any]:
    cards = []
    for number in HP_FIX_CARDS:
        out = subprocess.run(
            [sys.executable, str(Path(__file__).resolve()), "--hp-fragment", number],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=True,
        )
        cards.append(json.loads(out.stdout))
    return {
        "version": 1,
        "generator": "oracle/p6k_vectors.py",
        "source": "gcg_sim.effects.compiler.compile_card + export_ir.build_fragment",
        "cards": cards,
    }


def build(name: str) -> str:
    if name == "p6k_parentheticals.json":
        return dump(parentheticals())
    if name == "p6k_vocabulary.json":
        return dump(vocabulary())
    return dump(hp_fragments())


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(description="Export the P6-K tripwire vectors.")
    mode = ap.add_mutually_exclusive_group(required=True)
    mode.add_argument("--write", action="store_true")
    mode.add_argument("--check", action="store_true")
    mode.add_argument("--hp-fragment", metavar="CARD", help=argparse.SUPPRESS)
    args = ap.parse_args(argv)
    if args.hp_fragment:
        sys.stdout.write(json.dumps(hp_fragment(args.hp_fragment), ensure_ascii=True))
        return 0
    stale = []
    for name in VECTORS:
        text = build(name)
        path = OUT_DIR / name
        if args.write:
            OUT_DIR.mkdir(parents=True, exist_ok=True)
            path.write_text(text, encoding="utf-8")
        elif not path.is_file() or path.read_text(encoding="utf-8") != text:
            stale.append(name)
    for name in stale:
        print(f"p6k_vectors: {OUT_DIR / name} is stale; run with --write", file=sys.stderr)
    return 1 if stale else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
