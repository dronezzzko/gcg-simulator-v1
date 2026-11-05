"""Freshness lines for the Rust gate summary (docs/dev/WEAKNESSES.md W-115, W-153).

Standard library only (``scripts/verify_all_rust.sh`` runs it with the system ``python3``).
Both checks are informational: the script exits 0 whatever it finds and 1 only when a file it
reads is missing or malformed.

- **Research artefacts (W-115).** Each research artefact states the gcg-api data commit it
  profiles. The packaged commit is the ``gcgapi:commit`` entry of ``data/SOURCES.lock.json``. An
  artefact profiling another commit gets a ``stale research artifact`` warning; regenerating it
  is a Python-side task (``scripts/ingest_profile.py``, ``scripts/cluster_effects.py``).
- **News scan (W-153).** The newest ``retrieved_at`` of the news listing sources in
  ``data/sources.d/official.json`` (the URLs whose path ends in ``/news/``): the date of the last
  news scan of ``/gcg-refresh-data``, so an old scan is visible in the gate output.

    python3 scripts/gate_summary.py [--root DIR]
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path
from typing import Any
from urllib.parse import urlparse

ROOT = Path(__file__).resolve().parents[1]
LOCK = "data/SOURCES.lock.json"
OFFICIAL = "data/sources.d/official.json"
SHA = re.compile(r"\b[0-9a-f]{40}\b")
PINNED_ROW = re.compile(r"^\| Pinned data commit \| `([0-9a-f]{40})`", re.M)


def _json(root: Path, rel: str) -> Any:
    return json.loads((root / rel).read_text(encoding="utf-8"))


def packaged_commit(root: Path) -> str:
    """The gcg-api data commit of the packaged snapshot (``gcgapi:commit`` in the lock)."""
    for entry in _json(root, LOCK)["sources"]:
        if entry.get("id") == "gcgapi:commit":
            m = SHA.search(str(entry.get("url", "")))
            if m:
                return m.group(0)
    raise ValueError(f"{LOCK}: no gcgapi:commit entry with a commit url")


def _dig(doc: Any, path: tuple[str, ...]) -> str:
    for key in path:
        doc = doc[key]
    return str(doc)


def artefact_commits(root: Path) -> list[tuple[str, str]]:
    """``(artefact, commit it profiles)`` for every research artefact, in a fixed order."""
    out = []
    text = (root / "docs/research/DATA_INGESTION.md").read_text(encoding="utf-8")
    m = PINNED_ROW.search(text)
    if not m:
        raise ValueError("docs/research/DATA_INGESTION.md: no 'Pinned data commit' row")
    out.append(("docs/research/DATA_INGESTION.md", m.group(1)))
    for rel, path in (
        ("docs/research/data_profile.json", ("snapshot", "pinned_commit")),
        ("docs/research/effect_templates.json", ("source", "gcg_api_commit")),
        ("docs/research/work_packages.json", ("source", "gcg_api_commit")),
    ):
        out.append((rel, _dig(_json(root, rel), path)))
    return out


def news_scan_date(root: Path) -> tuple[str, list[str]]:
    """The newest ``retrieved_at`` of the news listing sources, and their ids."""
    rows = [e for e in _json(root, OFFICIAL) if urlparse(str(e["url"])).path.endswith("/news/")]
    if not rows:
        raise ValueError(f"{OFFICIAL}: no news listing source")
    newest = max(str(e.get("retrieved_at") or "") for e in rows)
    return newest, sorted(str(e["id"]) for e in rows)


def summary(root: Path) -> list[str]:
    packaged = packaged_commit(root)
    lines = []
    for rel, commit in artefact_commits(root):
        if commit == packaged:
            lines.append(f"research artifact {rel}: profiles {commit[:7]}, the packaged snapshot")
        else:
            lines.append(
                f"WARNING: stale research artifact {rel}: profiles {commit[:7]}, the packaged "
                f"snapshot is {packaged[:7]} (regenerate it on the Python side)"
            )
    newest, ids = news_scan_date(root)
    lines.append(
        f"news scan: newest retrieved_at of the news sources is {newest} ({', '.join(ids)})"
    )
    return lines


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=(__doc__ or "").splitlines()[0])
    ap.add_argument("--root", type=Path, default=ROOT, help="repository root")
    args = ap.parse_args(argv)
    try:
        lines = summary(args.root)
    except (OSError, ValueError, KeyError, TypeError) as exc:
        print(f"gate_summary: {exc}", file=sys.stderr)
        return 1
    print("\n".join(lines))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
