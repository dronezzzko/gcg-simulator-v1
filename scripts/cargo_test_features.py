"""Feature lists for the Rust gate (docs/dev/PLAN.md §9.11), from ``cargo metadata``.

Reads ``cargo metadata --format-version 1 --no-deps`` JSON on stdin and prints shell
assignments for ``eval``:

- ``SLOW``: every ``<package>/slow`` feature (the long ``[[test]]`` targets, A12);
- ``SLOW_OFFLINE``: the ``slow`` features whose tests read only committed files;
- ``SLOW_TIER2``: the ``slow`` features of packages whose tests read the regenerated tier-2
  corpus (a test file under the package's ``tests/`` names ``build/oracle/tier2``). They need
  ``oracle/traces.py --write --tier 2`` first, so the gate runs them in ``--full`` after
  that step, and nothing is skipped (W-112, W-118);
- ``DIAG``: every ``<package>/paranoid`` and ``<package>/testkit`` feature.

It exits 1 if any ``[[test]]`` or ``[[bin]]`` target requires a feature outside ``slow``,
``paranoid`` and ``testkit``, because such a target would never be built by the gate.
Standard library only (the gate runs it with the system ``python3``).

    eval "$(cargo metadata --format-version 1 --no-deps --offline | python3 scripts/cargo_test_features.py)"
"""

from __future__ import annotations

import json
import shlex
import sys
from pathlib import Path
from typing import Any

SLOW = ("slow",)
DIAG = ("paranoid", "testkit")
GATED_KINDS = ("test", "bin")
TIER2_MARK = "build/oracle/tier2"


def reads_tier2(manifest_path: str) -> bool:
    """Whether a test source of the package names the regenerated tier-2 corpus."""
    tests = Path(manifest_path).parent / "tests"
    return tests.is_dir() and any(
        TIER2_MARK in p.read_text(encoding="utf-8", errors="replace") for p in tests.rglob("*.rs")
    )


def features(meta: dict[str, Any]) -> tuple[list[str], list[str], list[str], list[str]]:
    """(slow features, of them the tier-2 ones, diagnostic features, errors)."""
    slow: list[str] = []
    tier2: list[str] = []
    diag: list[str] = []
    errors: list[str] = []
    allowed = set(SLOW) | set(DIAG)
    for pkg in sorted(meta["packages"], key=lambda p: p["name"]):
        name = pkg["name"]
        declared = pkg.get("features", {})
        own_slow = [f"{name}/{f}" for f in SLOW if f in declared]
        slow += own_slow
        if own_slow and reads_tier2(pkg["manifest_path"]):
            tier2 += own_slow
        diag += [f"{name}/{f}" for f in DIAG if f in declared]
        for target in pkg.get("targets", []):
            if not set(target.get("kind", [])) & set(GATED_KINDS):
                continue
            for feat in target.get("required-features", []):
                if feat not in allowed:
                    errors.append(
                        f"{name}: target {target['name']!r} requires feature {feat!r}, "
                        f"which the gate never enables (allowed: {sorted(allowed)})"
                    )
    return slow, tier2, diag, errors


def main() -> int:
    meta = json.load(sys.stdin)
    slow, tier2, diag, errors = features(meta)
    if errors:
        for e in errors:
            print(e, file=sys.stderr)
        return 1
    offline = [f for f in slow if f not in tier2]
    print(f"SLOW={shlex.quote(','.join(slow))}")
    print(f"SLOW_OFFLINE={shlex.quote(','.join(offline))}")
    print(f"SLOW_TIER2={shlex.quote(','.join(tier2))}")
    print(f"DIAG={shlex.quote(','.join(diag))}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
