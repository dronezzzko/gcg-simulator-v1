"""Export the traceability tags of every Python test (docs/port/PLAN.md §8.1, §9.2).

Writes port/corpus/v1/tier1/tests/tags.json from `gcg_sim.tools.marks.scan(Path("tests"))`, the
static scan that the Python coverage gates and `gcg_sim.tools.traceability` use
(`tools/marks.py:98-116`), so the Rust traceability generator (`gcg-diff traceability`) joins
exactly the tags the Python report counts with the per-test trace outcomes. The scan's rules
are kept as they are: literal arguments only, tags attributed to the outermost definition, and
`<module>` for module-level assignments (Inv 10 §3.1, hazards 1-3).

    uv run python port/oracle/export_tags.py [--check]

Layout: `tags` lists every tag in scan order as `{mark, value, path, test, line}`, one per
line. A tag's test id is `<path>::<test>`; the Python report keys it `<basename of path>::<last
"::" component of test>` (`tools/traceability.py:44-45`), and a per-test trace id is the pytest
node id, which is `<path>::<test>` plus any parametrisation suffix `[...]`. `counts` gives the
number of tags and of distinct values per mark; `inputs` the SHA-256 of every scanned file.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path
from typing import Any

from gcg_sim.tools.marks import scan

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "port" / "corpus" / "v1" / "tier1" / "tests" / "tags.json"
MARKS = ("rule", "card", "faq", "ruling")


def compact(obj: object) -> str:
    return json.dumps(obj, sort_keys=True, separators=(",", ":"), ensure_ascii=True)


def build(tests_root: Path) -> dict[str, Any]:
    tags = scan(tests_root, MARKS)
    files = sorted({t.path for t in tags})
    return {
        "version": 1,
        "generator": "port/oracle/export_tags.py",
        "source": "gcg_sim.tools.marks.scan(Path('tests'), ('rule', 'card', 'faq', 'ruling'))",
        "inputs": {
            p: hashlib.sha256((tests_root.parent / p).read_bytes()).hexdigest() for p in files
        },
        "counts": {
            m: {
                "tags": sum(1 for t in tags if t.mark == m),
                "values": len({t.value for t in tags if t.mark == m}),
            }
            for m in MARKS
        },
        "tags": [
            {"mark": t.mark, "value": t.value, "path": t.path, "test": t.test, "line": t.line}
            for t in tags
        ],
    }


def render(doc: dict[str, Any]) -> str:
    """Compact JSON with sorted keys, the tag list last and one tag per line."""
    head = {k: v for k, v in doc.items() if k != "tags"}
    body = ",\n".join(compact(t) for t in doc["tags"])
    return compact(head)[:-1] + ',"tags":[\n' + body + "\n]}\n"


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(description="Export the Python traceability tags.")
    ap.add_argument("--check", action="store_true", help="fail if the committed file is stale")
    ap.add_argument("--out", type=Path, default=OUT)
    args = ap.parse_args(argv)
    doc = build(ROOT / "tests")
    text = render(doc)
    if json.loads(text) != doc:
        print("export_tags: the rendered file does not parse back to the scan", file=sys.stderr)
        return 1
    if args.check:
        current = args.out.read_text() if args.out.is_file() else ""
        if current != text:
            print(f"export_tags: {args.out} is stale; run without --check", file=sys.stderr)
            return 1
        print(f"export_tags: {args.out.relative_to(ROOT)} is current")
        return 0
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(text)
    print(f"export_tags: wrote {args.out.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
