#!/usr/bin/env python3
"""Verify the normalized official data and maintain its provenance fragment (stdlib only).

Adopted from the official-sources research generator that first produced
``src/gcg_sim/data/official/*.json``. Those files are now edited by hand when Bandai publishes
something new; this script keeps the edits honest:

    python official_normalize.py check                 # read-only; exit 1 on any failure
    python official_normalize.py fragment [--dry-run] [--retrieved-at TS]

``check`` verifies that every quoted sentence (``{"quote", "local_text"}`` pairs and the
``source_text``-style fields) appears in the cached text extraction, that
``rules_version.json`` agrees with the packaged rules markdown, the repo-root copy and the
cached rules PDF (version, date, sha256, the set of rule ids), and that each
``data/sources.d/official.json`` hash matches its local file and text extraction.

``fragment`` recomputes ``sha256`` (and ``retrieved_at``) for every official.json entry whose
local raw file changed or whose sha256 is empty, and the "Text extraction: PATH (sha256 ...)"
hash in its notes. Afterwards run ``uv run python -m gcg_sim.tools.sources --write``.
"""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import re
import sys
from pathlib import Path
from typing import Any

REPO = Path(__file__).resolve().parents[4]
FRAGMENT = Path("data/sources.d/official.json")
OFFICIAL = Path("src/gcg_sim/data/official")
RAW = Path("data/official_raw")
RULES_MD = Path("src/gcg_sim/data/rules/gundam-card-game-comprehensive-rules.md")
ROOT_RULES_MD = Path("gundam-card-game-comprehensive-rules.md")
RULES_PDF_TXT = RAW / "en_pdf_comprehensiverules_en.txt"
RULES_PDF = RAW / "en_pdf_comprehensiverules_en.pdf"
QUOTE_KEYS = frozenset(
    {
        "source_text",
        "copy_identity_source_text",
        "future_cards_policy",
        "rules_page_text",
        "pdf_header",
    }
)
_TEXT_NOTE = re.compile(r"Text extraction: (\S+) \(sha256 ([0-9a-f]{64})\)")
_PDF_IDS = re.compile(r"(?m)^\s*(\d+(?:-\d+)+)\.\s")
_MD_IDS = re.compile(r"(?m)^(?:\*\*|#+ )(\d+(?:-\d+)+)\.")


def norm(text: str) -> str:
    return re.sub(r"\s+", " ", text).strip()


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def _walk(node: Any, path: str) -> list[tuple[str, dict[str, Any]]]:
    out: list[tuple[str, dict[str, Any]]] = []
    if isinstance(node, dict):
        out.append((path, node))
        for key, value in node.items():
            out += _walk(value, f"{path}.{key}")
    elif isinstance(node, list):
        for i, value in enumerate(node):
            out += _walk(value, f"{path}[{i}]")
    return out


def check_quotes(root: Path) -> tuple[int, list[str]]:
    texts = {p.name: norm(p.read_text("utf-8")) for p in sorted((root / RAW).glob("*.txt"))}
    corpus = list(texts.values())
    failures: list[str] = []
    checked = 0
    for jf in sorted((root / OFFICIAL).glob("*.json")):
        data = json.loads(jf.read_text("utf-8"))
        for path, node in _walk(data, jf.name):
            if isinstance(node.get("quote"), str) and isinstance(node.get("local_text"), str):
                checked += 1
                target = texts.get(Path(node["local_text"]).name)
                if target is None:
                    failures.append(f"{path}: local_text {node['local_text']} does not exist")
                elif norm(node["quote"]) not in target:
                    failures.append(f"{path}: quote not found in {node['local_text']}")
            quotes = [(k, v) for k, v in node.items() if k in QUOTE_KEYS and isinstance(v, str)]
            if path.endswith(".errata") and isinstance(node.get("rule"), str):
                quotes.append(("rule", node["rule"]))
            for key, value in quotes:
                checked += 1
                if not any(norm(value) in t for t in corpus):
                    failures.append(f"{path}.{key}: quote not found in any {RAW}/*.txt")
    return checked, failures


def _md_header(text: str) -> tuple[str | None, str | None]:
    head = text[:600]
    version = re.search(r"Ver\.\s*([0-9][0-9.]*)", head)
    updated = re.search(r"Updated\s+([A-Z][a-z]+\.? \d{1,2}, \d{4})", head)
    date = None
    if updated:
        date = dt.datetime.strptime(updated.group(1).replace(".", ""), "%b %d, %Y").date()
    return (version.group(1) if version else None, date.isoformat() if date else None)


def check_rules(root: Path) -> list[str]:
    failures: list[str] = []
    rv = json.loads((root / OFFICIAL / "rules_version.json").read_text("utf-8"))
    md = root / RULES_MD
    md_text = md.read_text("utf-8")
    version, date = _md_header(md_text)
    if (version, date) != (rv.get("latest_version"), rv.get("latest_date")):
        failures.append(
            f"{RULES_MD} header is Ver. {version} ({date}) but rules_version.json says "
            f"{rv.get('latest_version')} ({rv.get('latest_date')})"
        )
    root_md = root / ROOT_RULES_MD
    if root_md.is_file() and sha256(root_md) != sha256(md):
        failures.append(f"{ROOT_RULES_MD} and {RULES_MD} differ; keep them byte-identical")
    pdf = root / RULES_PDF
    if pdf.is_file() and rv.get("pdf_sha256") != sha256(pdf):
        failures.append(f"rules_version.json pdf_sha256 does not match {RULES_PDF}")
    pdf_txt = root / RULES_PDF_TXT
    if pdf_txt.is_file():
        pdf_ids = set(_PDF_IDS.findall(pdf_txt.read_text("utf-8")))
        md_ids = set(_MD_IDS.findall(md_text))
        if pdf_ids != md_ids:
            failures.append(
                f"rule ids differ: only in the PDF {sorted(pdf_ids - md_ids)[:20]}, only in the "
                f"markdown {sorted(md_ids - pdf_ids)[:20]}"
            )
        if rv.get("rule_ids_in_pdf") not in (None, len(pdf_ids)):
            failures.append(
                f"rules_version.json rule_ids_in_pdf={rv.get('rule_ids_in_pdf')} but the PDF text "
                f"has {len(pdf_ids)}"
            )
        if rv.get("rule_ids_in_repo_rules_file") not in (None, len(md_ids)):
            failures.append(
                "rules_version.json rule_ids_in_repo_rules_file="
                f"{rv.get('rule_ids_in_repo_rules_file')} but the markdown has {len(md_ids)}"
            )
    return failures


def check_fragment(root: Path) -> list[str]:
    failures: list[str] = []
    for e in json.loads((root / FRAGMENT).read_text("utf-8")):
        local = e.get("local_path")
        if local and (root / local).is_file() and sha256(root / local) != e.get("sha256"):
            failures.append(f"{e['id']}: sha256 does not match {local}; run `fragment`")
        for txt, digest in _TEXT_NOTE.findall(str(e.get("notes") or "")):
            if (root / txt).is_file() and sha256(root / txt) != digest:
                failures.append(f"{e['id']}: text extraction hash in notes is stale ({txt})")
    return failures


def cmd_check(args: argparse.Namespace) -> int:
    checked, failures = check_quotes(args.root)
    failures += check_rules(args.root)
    failures += check_fragment(args.root)
    for f in failures:
        print(f"FAIL {f}")
    print(f"{checked} quotes checked; {len(failures)} failure(s)")
    return 1 if failures else 0


def _mtime(path: Path) -> str:
    ts = dt.datetime.fromtimestamp(path.stat().st_mtime, tz=dt.UTC)
    return ts.strftime("%Y-%m-%dT%H:%M:%SZ")


def cmd_fragment(args: argparse.Namespace) -> int:
    path = args.root / FRAGMENT
    entries: list[dict[str, Any]] = json.loads(path.read_text("utf-8"))
    changed: list[str] = []
    for e in entries:
        local = e.get("local_path")
        before = dict(e)
        if local and (args.root / local).is_file():
            digest = sha256(args.root / local)
            if digest != e.get("sha256"):
                e["sha256"] = digest
                e["retrieved_at"] = args.retrieved_at or _mtime(args.root / local)

        def fix(m: re.Match[str]) -> str:
            txt = args.root / m.group(1)
            digest = sha256(txt) if txt.is_file() else m.group(2)
            return f"Text extraction: {m.group(1)} (sha256 {digest})"

        e["notes"] = _TEXT_NOTE.sub(fix, str(e.get("notes") or ""))
        if e != before:
            changed.append(
                f"{e['id']}: " + ", ".join(sorted(k for k in e if e.get(k) != before.get(k)))
            )
    for line in changed:
        print(line)
    if not changed:
        print(f"no changes: {FRAGMENT} matches the files in {RAW}")
        return 0
    if args.dry_run:
        print(f"dry run: {len(changed)} entr(y/ies) would change; nothing was written")
        return 0
    path.write_text(
        json.dumps(entries, indent=2, sort_keys=True, ensure_ascii=False) + "\n", "utf-8"
    )
    print(
        f"updated {len(changed)} entr(y/ies); now run: uv run python -m gcg_sim.tools.sources --write"
    )
    return 0


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=(__doc__ or "").splitlines()[0])
    ap.add_argument("--root", type=Path, default=REPO, help="gcg-sim checkout")
    sub = ap.add_subparsers(dest="command", required=True)
    sub.add_parser("check").set_defaults(func=cmd_check)
    frag = sub.add_parser("fragment")
    frag.add_argument("--dry-run", action="store_true")
    frag.add_argument("--retrieved-at", help="UTC time for changed files (default: file mtime)")
    frag.set_defaults(func=cmd_fragment)
    args = ap.parse_args(argv)
    code: int = args.func(args)
    return code


if __name__ == "__main__":
    sys.exit(main())
