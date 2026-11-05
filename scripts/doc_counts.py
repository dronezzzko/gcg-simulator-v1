"""Generated counts for the port's documents (docs/port/WEAKNESSES.md W-128).

A count in a document is written as ``<!-- count:NAME -->VALUE<!-- /count -->``. This tool
computes every named count from the packaged data and the committed records, never from
another document, and checks (or, with ``--write``, rewrites) every marked value, so a count can
not be hand-typed or go stale. Standard library only (the gate runs it with the system
``python3``). The counts:

``cards.numbers``           real card numbers in the packaged gcg-api snapshot
``cards.definitions``       card definitions of the effect IR (the numbers plus synthetic ones)
``cards.non_vanilla``       real cards with an ability (``port/corpus/v1/census.json``)
``cards.vanilla``           real cards without one
``cards.effect_only``       Unit token, EX Base and EX Resource numbers (never in a deck)
``cards.fuzz_pool``         numbers the fuzz decks are built from (the fuzz census pool)
``rules.total``, ``rules.testable``, ``rules.na``   the rules index
``faq.entries``, ``rulings.entries``                the packaged FAQ and rulings
``conflicts.resolutions``   resolutions of ``src/gcg_sim/data/overrides.json``
``corpus.tier1.traces``     traces of the committed tier-1 corpus
``tests_json.entries``      entries of ``docs/port/tests.json``

    python3 scripts/doc_counts.py [--check | --write | --list] [files...]
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_FILES = ("docs/port/**/*.md",)
MARK = re.compile(r"<!-- count:(?P<name>[a-z0-9_.]+) -->(?P<value>[^<]*)<!-- /count -->")
EFFECT_ONLY = ("UNIT TOKEN", "EX BASE", "EX RESOURCE")


def _json(root: Path, rel: str) -> Any:
    return json.loads((root / rel).read_text(encoding="utf-8"))


def counts(root: Path = ROOT) -> dict[str, int]:
    """Every named count, computed from the data and records."""
    types: dict[str, str] = {}
    cards_file = root / "src/gcg_sim/data/gcgapi/cards.ndjson"
    for line in cards_file.read_text(encoding="utf-8").splitlines():
        if line.strip():
            rec = json.loads(line)
            types[str(rec["card_number"])] = str(rec["card_type"])
    census = _json(root, "port/corpus/v1/census.json")["cards"]
    rules = _json(root, "src/gcg_sim/data/rules/rules_index.json")["counts"]
    out = {
        "cards.numbers": len(types),
        "cards.definitions": len(_json(root, "crates/gcg-effects/ir/registry.json")["cards"]),
        "cards.non_vanilla": int(census["non_vanilla"]),
        "cards.vanilla": int(census["vanilla"]),
        "cards.effect_only": sum(1 for t in types.values() if t in EFFECT_ONLY),
        "cards.fuzz_pool": len(
            _json(root, "docs/port/eval/P6-N/fuzz-census-2026-10000.json")["pool"]
        ),
        "rules.total": int(rules["total"]),
        "rules.testable": int(rules["testable"]),
        "rules.na": int(rules["na"]),
        "faq.entries": len(_json(root, "src/gcg_sim/data/gcgapi/rules-faq.json")),
        "rulings.entries": len(_json(root, "src/gcg_sim/data/gcgapi/rulings.json")),
        "conflicts.resolutions": len(_json(root, "src/gcg_sim/data/overrides.json")["resolutions"]),
        "corpus.tier1.traces": int(_json(root, "port/corpus/v1/MANIFEST.json")["totals"]["traces"]),
        "tests_json.entries": len(_json(root, "docs/port/tests.json")),
    }
    return out


def fmt(n: int) -> str:
    """The way the port's prose writes a count: 1,152 (thousands separated), 840."""
    return f"{n:,}"


def doc_files(patterns: list[str], root: Path) -> list[Path]:
    """The files of glob patterns relative to ``root``; an absolute path names one file."""
    files: set[Path] = set()
    for pattern in patterns:
        if Path(pattern).is_absolute():
            files.add(Path(pattern))
        else:
            files.update(p for p in root.glob(pattern) if p.is_file())
    return sorted(files)


def check_text(text: str, values: dict[str, int]) -> tuple[str, list[str]]:
    """The text with every marked count regenerated, and the differences found."""
    problems: list[str] = []

    def sub(m: re.Match[str]) -> str:
        name = m.group("name")
        if name not in values:
            problems.append(f"unknown count {name!r}")
            return m.group(0)
        want = fmt(values[name])
        if m.group("value") != want:
            problems.append(f"{name}: the document says {m.group('value')!r}, the data {want!r}")
        return f"<!-- count:{name} -->{want}<!-- /count -->"

    return MARK.sub(sub, text), problems


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=(__doc__ or "").splitlines()[0])
    ap.add_argument("files", nargs="*", help="glob patterns relative to the repository")
    ap.add_argument("--root", type=Path, default=ROOT)
    mode = ap.add_mutually_exclusive_group()
    mode.add_argument("--check", action="store_true", help="fail on a stale count (default)")
    mode.add_argument("--write", action="store_true", help="rewrite the marked counts")
    mode.add_argument("--list", action="store_true", help="print every count")
    args = ap.parse_args(argv)
    try:
        values = counts(args.root)
    except (OSError, KeyError, ValueError) as exc:
        print(f"doc_counts: {exc}", file=sys.stderr)
        return 1
    if args.list:
        for k, v in values.items():
            print(f"{k} {v}")
        return 0
    failures = 0
    marks = 0
    for path in doc_files(args.files or list(DEFAULT_FILES), args.root):
        text = path.read_text(encoding="utf-8")
        marks += len(MARK.findall(text))
        new, problems = check_text(text, values)
        rel = path.relative_to(args.root) if path.is_relative_to(args.root) else path
        if args.write and new != text:
            path.write_text(new, encoding="utf-8")
            print(f"rewrote {rel}")
        elif problems and not args.write:
            failures += len(problems)
            for p in problems:
                print(f"{rel}: {p}", file=sys.stderr)
    print(f"doc counts: {marks} marked counts, {failures} stale")
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
