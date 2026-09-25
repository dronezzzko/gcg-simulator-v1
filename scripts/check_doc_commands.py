"""Run every shell command block in the project documentation (acceptance criterion 10).

Every fenced ```bash block in README.md, CLAUDE.md, docs/**/*.md and the gcg-benchmark skill is
run with ``bash -euo pipefail`` from the repository root. A block that cannot run offline (it
needs a network clone of a source) must be preceded by ``<!-- doc-check: skip <reason> -->``.
Skipped blocks are listed with their reason; a skip without a reason is an error.

    uv run python scripts/check_doc_commands.py [--list] [--timeout SECONDS] [files...]
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
import time
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_FILES = (
    "README.md",
    "CLAUDE.md",
    "docs/**/*.md",
    ".claude/skills/gcg-benchmark/**/*.md",
)
BLOCK = re.compile(
    r"(?:<!--\s*doc-check:\s*skip(?P<reason>[^>]*?)-->\s*\n)?"
    r"^[ \t]*```(?P<lang>bash|sh|shell)\n(?P<body>.*?)^[ \t]*```",
    re.M | re.S,
)


@dataclass(frozen=True)
class Block:
    path: Path
    line: int
    body: str
    skip: str | None


def blocks(path: Path) -> list[Block]:
    text = path.read_text(encoding="utf-8")
    out = []
    for m in BLOCK.finditer(text):
        reason = m.group("reason")
        out.append(
            Block(
                path=path,
                line=text.count("\n", 0, m.start("body")),
                body=m.group("body"),
                skip=None if reason is None else reason.strip(),
            )
        )
    return out


def doc_files(patterns: list[str]) -> list[Path]:
    files: set[Path] = set()
    for pattern in patterns:
        files.update(p for p in ROOT.glob(pattern) if p.is_file())
    return sorted(files)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=(__doc__ or "").split("\n")[0])
    parser.add_argument("files", nargs="*", help="glob patterns relative to the repository")
    parser.add_argument("--list", action="store_true", help="list the blocks without running")
    parser.add_argument("--timeout", type=float, default=1800.0, help="seconds per block")
    args = parser.parse_args(argv)
    failures: list[str] = []
    ran = skipped = 0
    for path in doc_files(args.files or list(DEFAULT_FILES)):
        for b in blocks(path):
            where = f"{path.relative_to(ROOT)}:{b.line}"
            if b.skip is not None:
                if not b.skip:
                    failures.append(f"{where}: doc-check skip without a reason")
                print(f"SKIP {where}: {b.skip}")
                skipped += 1
                continue
            if args.list:
                print(f"---- {where}\n{b.body}")
                continue
            start = time.monotonic()
            try:
                proc = subprocess.run(
                    ["bash", "-euo", "pipefail", "-c", b.body],
                    cwd=ROOT,
                    capture_output=True,
                    text=True,
                    timeout=args.timeout,
                    check=False,
                )
                ok = proc.returncode == 0
                detail = (proc.stdout + proc.stderr).strip().splitlines()[-15:]
            except subprocess.TimeoutExpired:
                ok, detail = False, [f"timed out after {args.timeout:.0f}s"]
            elapsed = time.monotonic() - start
            ran += 1
            print(f"{'PASS' if ok else 'FAIL'} {where} ({elapsed:.1f}s)")
            if not ok:
                failures.append(where)
                print("\n".join("    " + line for line in detail))
    print(f"{ran} blocks run, {skipped} skipped, {len(failures)} failed")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
