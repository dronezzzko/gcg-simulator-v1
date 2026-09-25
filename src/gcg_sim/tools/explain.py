"""Explain a card: data, normalized text, compiled abilities (or errors), binding, rulings, conflicts.

uv run python -m gcg_sim.tools.explain GD01-001 [GD01-002 ...] [--json]
"""

from __future__ import annotations

import argparse
import json
from typing import Any

from gcg_sim.cards.db import read_data_text
from gcg_sim.effects.compiler import compile_parts
from gcg_sim.effects.compiler.abilities import CompileError
from gcg_sim.effects.golden import node_to_json
from gcg_sim.effects.registry import get_registry
from gcg_sim.effects.scripts import binding_module
from gcg_sim.effects.text import normalize


def _rulings(number: str) -> list[dict[str, Any]]:
    data: list[dict[str, Any]] = json.loads(read_data_text("gcgapi", "rulings.json"))
    return [r for r in data if r.get("card_number") == number]


def _conflicts(number: str) -> list[dict[str, Any]]:
    try:
        overrides = json.loads(read_data_text("overrides.json"))
    except FileNotFoundError:
        return []
    out = []
    for cid, res in sorted(overrides.get("resolutions", {}).items()):
        if number in cid or number in (res.get("card_numbers") or []):
            out.append({"id": cid, "decision": res.get("decision"), "policy": res.get("policy")})
    return out


def explain(number: str) -> dict[str, Any]:
    reg = get_registry()
    cdef = reg.db[number]
    entry = reg.cards[cdef.def_id]
    parts = []
    for line, res in compile_parts(cdef):
        if isinstance(res, CompileError):
            parts.append({"line": line, "error": str(res)})
        else:
            parts.append({"line": line, "compiled": node_to_json(res)})
    return {
        "card_number": cdef.card_number,
        "name": cdef.name,
        "type": cdef.card_type.value,
        "color": cdef.color.value if cdef.color else None,
        "level": cdef.level,
        "cost": cdef.cost,
        "ap": cdef.ap,
        "hp": cdef.hp,
        "traits": list(cdef.traits),
        "link": node_to_json(cdef.link),
        "pilot_name": cdef.pilot_name,
        "text": normalize(cdef.effect),
        "binding": binding_module(number),
        "script_source": entry.script.source if entry.script else None,
        "error": entry.error,
        "final_script": node_to_json(entry.script) if entry.script else None,
        "compiled_parts": parts,
        "rulings": _rulings(number),
        "resolutions": _conflicts(number),
    }


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("cards", nargs="+")
    ap.add_argument("--json", action="store_true")
    args = ap.parse_args(argv)
    for n in args.cards:
        info = explain(n)
        if args.json:
            print(json.dumps(info, indent=1, ensure_ascii=False))
            continue
        print(
            f"== {info['card_number']} {info['name']} [{info['type']} {info['color']} Lv{info['level']} C{info['cost']} {info['ap']}/{info['hp']}]"
        )
        print(f"traits={info['traits']} link={info['link']} pilot_name={info['pilot_name']}")
        print("text:\n  " + info["text"].replace("\n", "\n  "))
        print(f"binding={info['binding']} source={info['script_source']} error={info['error']}")
        for p in info["compiled_parts"]:
            if "error" in p:
                print(f"  LINE {p['line']!r}\n    ERROR {p['error']}")
            else:
                print(f"  LINE {p['line']!r}\n    {json.dumps(p['compiled'], ensure_ascii=False)}")
        for r in info["rulings"]:
            print(f"  RULING {r.get('num')}: Q: {r.get('question')} A: {r.get('answer')}")
        for c in info["resolutions"]:
            print(f"  RESOLUTION {c['id']}: {c['decision']}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
