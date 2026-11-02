"""Snapshot of every `--help` text of the Rust binaries (docs/port/WEAKNESSES.md W-129).

The command-line interface documentation is generated from the binaries, never typed: this
script runs ``--help`` of ``gcg-sim``, ``gcg-eval`` and ``gcg-diff`` and of every subcommand the
top-level help lists (the ``{a,b,...}`` choice of its usage line, nested ones included), and
writes them in a fixed order into one file. ``--check`` regenerates the snapshot and fails on
any difference, so a changed flag, default or help text cannot go undocumented: whoever changes
the CLI regenerates the snapshot with ``--write`` and the diff shows the change. Standard
library only (the gate runs it with the system ``python3``).

    python3 scripts/cli_help_snapshot.py --write [--bin-dir target/release]
    python3 scripts/cli_help_snapshot.py --check [--bin-dir target/release]
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SNAPSHOT = ROOT / "docs" / "port" / "eval" / "P6-N" / "cli-help.txt"
BINARIES = ("gcg-sim", "gcg-eval", "gcg-diff")
# A subcommand group in an argparse usage line: `{a,b,c} ...` (option choices such as
# `[--decks {examples,tests}]` are not followed by `...`).
CHOICES = re.compile(r"\{([a-z][a-z0-9-]*(?:,[a-z][a-z0-9-]*)*)\} \.\.\.")
MAX_DEPTH = 3


def help_text(binary: Path, args: list[str]) -> str:
    out = subprocess.run(
        [str(binary), *args, "--help"], capture_output=True, text=True, check=False
    )
    if out.returncode != 0:
        raise SystemExit(f"{binary.name} {' '.join(args)} --help exited {out.returncode}")
    return out.stdout


def subcommands(text: str) -> list[str]:
    """The subcommand choices of a help text's usage line (the first ``{a,b}`` group)."""
    usage = text.split("\n\n", 1)[0]
    m = CHOICES.search(usage)
    return m.group(1).split(",") if m else []


def snapshot(bin_dir: Path) -> str:
    parts: list[str] = []
    for name in BINARIES:
        binary = bin_dir / name
        if not binary.is_file():
            raise SystemExit(
                f"{binary} not found: cargo build --release -p gcg-cli -p gcg-eval -p gcg-diff"
            )
        todo: list[list[str]] = [[]]
        while todo:
            args = todo.pop(0)
            text = help_text(binary, args)
            parts.append(f"$ {name} {' '.join([*args, '--help']).strip()}\n{text.rstrip()}\n")
            if len(args) < MAX_DEPTH:
                todo.extend([*args, sub] for sub in subcommands(text))
    return "\n".join(parts)


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=(__doc__ or "").splitlines()[0])
    mode = ap.add_mutually_exclusive_group(required=True)
    mode.add_argument("--check", action="store_true")
    mode.add_argument("--write", action="store_true")
    ap.add_argument("--bin-dir", type=Path, default=ROOT / "target" / "release")
    args = ap.parse_args(argv)
    text = snapshot(args.bin_dir)
    if args.write:
        SNAPSHOT.parent.mkdir(parents=True, exist_ok=True)
        SNAPSHOT.write_text(text, encoding="utf-8")
        print(f"wrote {SNAPSHOT.relative_to(ROOT)} ({text.count(chr(10) + '$ ') + 1} help texts)")
        return 0
    old = SNAPSHOT.read_text(encoding="utf-8") if SNAPSHOT.is_file() else ""
    if old != text:
        print(
            f"{SNAPSHOT.relative_to(ROOT)} differs from the binaries' --help: review the CLI "
            "change and run scripts/cli_help_snapshot.py --write",
            file=sys.stderr,
        )
        return 1
    print(f"{SNAPSHOT.relative_to(ROOT)} matches the binaries")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
