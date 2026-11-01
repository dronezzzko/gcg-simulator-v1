"""Feature lists for the Rust gate (docs/port/PLAN.md §9.11), from ``cargo metadata``.

Reads ``cargo metadata --format-version 1 --no-deps`` JSON on stdin and prints two shell
assignments for ``eval``:

- ``SLOW``: every ``<package>/slow`` feature (the long ``[[test]]`` targets, A12);
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
from typing import Any

SLOW = ("slow",)
DIAG = ("paranoid", "testkit")
GATED_KINDS = ("test", "bin")


def features(meta: dict[str, Any]) -> tuple[list[str], list[str], list[str]]:
    """(slow features, diagnostic features, errors) of the workspace packages."""
    slow: list[str] = []
    diag: list[str] = []
    errors: list[str] = []
    allowed = set(SLOW) | set(DIAG)
    for pkg in sorted(meta["packages"], key=lambda p: p["name"]):
        name = pkg["name"]
        declared = pkg.get("features", {})
        slow += [f"{name}/{f}" for f in SLOW if f in declared]
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
    return slow, diag, errors


def main() -> int:
    meta = json.load(sys.stdin)
    slow, diag, errors = features(meta)
    if errors:
        for e in errors:
            print(e, file=sys.stderr)
        return 1
    print(f"SLOW={shlex.quote(','.join(slow))}")
    print(f"DIAG={shlex.quote(','.join(diag))}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
