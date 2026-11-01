#!/usr/bin/env python3
"""Normalise a gcg-sim report directory for a Python-vs-Rust byte comparison.

Only implementation-specific fields change (docs/port/EQUIVALENCE.md):
results.json `versions.package`, `versions.code_digest`, `ai.factory`, and their renderings in
summary.md. timing.json is dropped. games.ndjson and replays/*.json are copied unchanged.

    python3 scripts/normalize_outputs.py SRC_DIR DST_DIR
"""

from __future__ import annotations

import json
import re
import shutil
import sys
from pathlib import Path

PLACEHOLDER = "<implementation>"


def normalize_results(text: str) -> str:
    data = json.loads(text)
    data["versions"]["package"] = PLACEHOLDER
    data["versions"]["code_digest"] = PLACEHOLDER
    data["ai"]["factory"] = PLACEHOLDER
    return json.dumps(data, indent=2, sort_keys=True, ensure_ascii=False) + "\n"


def normalize_summary(text: str) -> str:
    text = re.sub(r"factory `[^`]*`", f"factory `{PLACEHOLDER}`", text)
    return re.sub(r"^gcg-sim [^;\n]*;", f"gcg-sim {PLACEHOLDER};", text, flags=re.MULTILINE)


def main(argv: list[str]) -> int:
    if len(argv) != 3:
        print(__doc__, file=sys.stderr)
        return 2
    src, dst = Path(argv[1]), Path(argv[2])
    if dst.exists():
        shutil.rmtree(dst)
    dst.mkdir(parents=True)
    for path in sorted(src.rglob("*")):
        rel = path.relative_to(src)
        if path.is_dir() or rel.as_posix() == "timing.json":
            continue
        out = dst / rel
        out.parent.mkdir(parents=True, exist_ok=True)
        if rel.as_posix() == "results.json":
            out.write_text(normalize_results(path.read_text(encoding="utf-8")), encoding="utf-8")
        elif rel.as_posix() == "summary.md":
            out.write_text(normalize_summary(path.read_text(encoding="utf-8")), encoding="utf-8")
        else:
            shutil.copyfile(path, out)
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
