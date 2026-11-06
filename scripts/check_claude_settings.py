"""Check the project's Claude permission rules (docs/dev/WEAKNESSES.md W-120).

The project rule is that only ``/gcg-refresh-data`` uses the network (CLAUDE.md). This check
parses ``.claude/settings.json`` and fails on

- an allowed network download command (``curl``, ``wget``) whose pattern does not pin it to an
  ``https://`` host; the ``WebFetch(domain:...)`` rules are the host-restricted alternative;
- a permission pattern with leading or trailing whitespace inside its parentheses, which may
  never match (``Bash(git -C * )`` does not match ``git -C path cmd``).

It only reads the file: changing the settings is the user's decision (docs/dev/PLAN.md §0.1,
A32 item 3), so until the user approves W-120's change this check reports the current findings
and exits 1. ``--selftest`` checks the rules on fixed examples, including the settings as the
proposed change would leave them.

    python3 scripts/check_claude_settings.py [--settings FILE]
    python3 scripts/check_claude_settings.py --selftest
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
RULE = re.compile(r"^(?P<tool>[A-Za-z]+)\((?P<pattern>.*)\)$", re.S)
NETWORK_TOOLS = ("curl", "wget")
HOST = re.compile(r"https://[A-Za-z0-9.-]+\.[A-Za-z]{2,}/")


def findings(settings: dict[str, Any]) -> list[str]:
    out = []
    perms = settings.get("permissions", {})
    for kind in ("allow", "ask", "deny"):
        for rule in perms.get(kind, []):
            m = RULE.match(rule)
            if not m:
                continue
            pattern = m.group("pattern")
            if pattern != pattern.strip():
                out.append(f"{kind}: {rule!r} has whitespace at the edge of its pattern")
            words = pattern.split()
            if (
                kind == "allow"
                and m.group("tool") == "Bash"
                and words
                and words[0] in NETWORK_TOOLS
                and not HOST.search(pattern)
            ):
                out.append(f"allow: {rule!r} lets {words[0]} reach any host")
    return out


PROPOSED = {
    "permissions": {
        "allow": [
            "Bash(uv *)",
            "Bash(git -C *)",
            "Bash(curl -sS https://raw.githubusercontent.com/yzRobo/gcg-api/*)",
            "WebFetch(domain:www.gundam-gcg.com)",
        ]
    }
}


def selftest() -> int:
    current = {"permissions": {"allow": ["Bash(git -C * )", "Bash(curl -sS *)", "Bash(ls *)"]}}
    problems = []
    got = findings(current)
    if len(got) != 2 or "whitespace" not in got[0] or "any host" not in got[1]:
        problems.append(f"the current rules gave {got}")
    if findings(PROPOSED):
        problems.append(f"the proposed rules gave {findings(PROPOSED)}")
    if not findings({"permissions": {"allow": ["Bash(wget *)"]}}):
        problems.append("an unrestricted wget passed")
    for p in problems:
        print(f"selftest: {p}", file=sys.stderr)
    print(f"check_claude_settings selftest {'failed' if problems else 'passed'}")
    return 1 if problems else 0


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=(__doc__ or "").splitlines()[0])
    ap.add_argument("--settings", type=Path, default=ROOT / ".claude" / "settings.json")
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args(argv)
    if args.selftest:
        return selftest()
    found = findings(json.loads(args.settings.read_text(encoding="utf-8")))
    for f in found:
        print(f"{args.settings}: {f}")
    print(f"claude settings: {len(found)} finding(s)")
    return 1 if found else 0


if __name__ == "__main__":
    raise SystemExit(main())
