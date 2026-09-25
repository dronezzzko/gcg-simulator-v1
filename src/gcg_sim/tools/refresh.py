"""Data-refresh helpers used by the ``gcg-refresh-data`` skill.

Everything here reads local files; only the skill's fetch scripts touch the network.

    uv run python -m gcg_sim.tools.refresh diff --new-data DIR [--old-data DIR] [--json OUT]
    uv run python -m gcg_sim.tools.refresh apply --new-data DIR [--dry-run] [--retrieved-at TS]
    uv run python -m gcg_sim.tools.refresh coverage [--json OUT] [--fail-on-gaps]
    uv run python -m gcg_sim.tools.refresh banlist [--banlist PATH] [--data DIR]

``diff`` compares two gcg-api data directories (a git clone of yzRobo/gcg-api, or a packaged
copy such as ``src/gcg_sim/data/gcgapi``). ``apply`` installs the packaged file set from a clone
into ``src/gcg_sim/data/gcgapi`` and rewrites ``data/sources.d/gcgapi.json``. ``coverage`` lists
the cards that need implementation work. ``banlist`` validates the banned/restricted list
against the card data.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
import unicodedata
from collections import Counter
from collections.abc import Iterable, Mapping, Sequence
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from gcg_sim.cards.db import GAMEPLAY_FIELDS, CardDB, load_overrides, load_raw_printings
from gcg_sim.cards.model import CardDef
from gcg_sim.effects.text import normalize, text_hash
from gcg_sim.tools.marks import scan, values

JsonDict = dict[str, Any]

PACKAGE_DATA = Path(__file__).resolve().parent.parent / "data"
PACKAGED_GCGAPI = PACKAGE_DATA / "gcgapi"
DEFAULT_BANLIST = PACKAGE_DATA / "official" / "banlist.json"
OFFICIAL_ERRATA = PACKAGE_DATA / "official" / "edition_language.json"

UPSTREAM_REPO = "yzRobo/gcg-api"
DEST_REL = Path("src/gcg_sim/data/gcgapi")
FRAGMENT_REL = Path("data/sources.d/gcgapi.json")

# Packaged file (relative to src/gcg_sim/data/gcgapi) -> path inside a gcg-api clone.
PACKAGED_FILES: dict[str, str] = {
    "LICENSE-DATA": "LICENSE-DATA",
    "cards.ndjson": "data/cards.ndjson",
    "errata.json": "data/errata.json",
    "manifest.json": "data/manifest.json",
    "products.json": "data/products.json",
    "rules-faq.json": "data/rules-faq.json",
    "rulings.json": "data/rulings.json",
    "schema.sql": "schema.sql",
    "sets/en/index.json": "data/sets/en/index.json",
}
FILE_KINDS = {"LICENSE-DATA": "license", "schema.sql": "schema"}
MARKER_FIELDS = ("keyword_effects", "timing_markers", "keywords_text")
CARD_FIELDS = (*GAMEPLAY_FIELDS, *MARKER_FIELDS)
MANIFEST_COUNTS = (
    "card_count",
    "set_count",
    "ruling_count",
    "rules_faq_count",
    "errata_count",
    "product_count",
)
FOLLOW_UP = (
    "uv run python -m gcg_sim.tools.sources --write",
    "uv run python -m gcg_sim.sources.conflicts",
    "uv run python -m gcg_sim.sources.conflicts --check",
    "uv run python -m gcg_sim.tools.refresh banlist",
    "uv run python -m gcg_sim.tools.refresh coverage",
    "uv run python -m gcg_sim.tools.golden --check",
    'uv run pytest -q -m "not slow"',
)


class RefreshError(Exception):
    """An input is missing or inconsistent; the message says what to do."""


# ---------------------------------------------------------------------------------------------
# Loading


def sha256_file(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def _read_json(path: Path) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise RefreshError(f"cannot read {path}: {exc}") from exc


def _dump_json(data: Any) -> str:
    return json.dumps(data, sort_keys=True, indent=2, ensure_ascii=False) + "\n"


def locate(root: Path) -> tuple[str, dict[str, Path]]:
    """Map every packaged file name to its path under ``root`` (clone or packaged layout)."""
    if (root / "data" / "manifest.json").is_file():
        return "clone", {local: root / upstream for local, upstream in PACKAGED_FILES.items()}
    if (root / "manifest.json").is_file():
        return "package", {local: root / local for local in PACKAGED_FILES}
    raise RefreshError(
        f"{root}: neither data/manifest.json (gcg-api clone) nor manifest.json (packaged copy) exists"
    )


@dataclass(frozen=True)
class Snapshot:
    """One gcg-api data directory, parsed."""

    root: Path
    layout: str
    paths: dict[str, Path]
    manifest: JsonDict
    printings: list[JsonDict]
    rulings: list[JsonDict]
    faq: list[JsonDict]
    errata: list[JsonDict]
    products: list[JsonDict]
    sets: list[JsonDict]

    def sha256(self, local: str) -> str | None:
        path = self.paths[local]
        return sha256_file(path) if path.is_file() else None

    def by_number(self) -> dict[str, list[JsonDict]]:
        return group_by_number(self.printings)


def _optional_list(path: Path) -> list[JsonDict]:
    if not path.is_file():
        return []
    data = _read_json(path)
    if not isinstance(data, list):
        raise RefreshError(f"{path} must contain a JSON array")
    return [item for item in data if isinstance(item, dict)]


def load_snapshot(root: Path) -> Snapshot:
    layout, paths = locate(root)
    manifest = _read_json(paths["manifest.json"])
    if not isinstance(manifest, dict):
        raise RefreshError(f"{paths['manifest.json']} must contain a JSON object")
    cards_path = paths["cards.ndjson"]
    if not cards_path.is_file():
        raise RefreshError(f"missing {cards_path}")
    try:
        printings = load_raw_printings(cards_path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        raise RefreshError(f"{cards_path}: {exc}") from exc
    return Snapshot(
        root=root,
        layout=layout,
        paths=paths,
        manifest=manifest,
        printings=printings,
        rulings=_optional_list(paths["rulings.json"]),
        faq=_optional_list(paths["rules-faq.json"]),
        errata=_optional_list(paths["errata.json"]),
        products=_optional_list(paths["products.json"]),
        sets=_optional_list(paths["sets/en/index.json"]),
    )


def group_by_number(printings: Iterable[JsonDict]) -> dict[str, list[JsonDict]]:
    out: dict[str, list[JsonDict]] = {}
    for p in printings:
        out.setdefault(str(p["card_number"]), []).append(p)
    for ps in out.values():
        ps.sort(key=lambda p: str(p["product_id"]))
    return out


def base_printing(printings: Sequence[JsonDict]) -> JsonDict:
    """The printing whose product_id equals its card number, else the first by product_id."""
    for p in printings:
        if p["product_id"] == p["card_number"]:
            return p
    return sorted(printings, key=lambda p: str(p["product_id"]))[0]


# ---------------------------------------------------------------------------------------------
# diff


def _field_changes(
    old: Mapping[str, Any], new: Mapping[str, Any], fields: Iterable[str]
) -> list[JsonDict]:
    return [
        {"field": f, "old": old.get(f), "new": new.get(f)}
        for f in fields
        if old.get(f) != new.get(f)
    ]


def classify_effect(old: str | None, new: str | None) -> JsonDict:
    """ "wording" when the normalized text (and so the golden text_hash) changes, else
    "typography" (typographic, whitespace or reminder-text-only change, rule 2-11-4)."""
    old_text = old or "-"
    new_text = new or "-"
    old_norm = normalize(old_text)
    new_norm = normalize(new_text)
    out: JsonDict = {
        "kind": "typography" if old_norm == new_norm else "wording",
        "old_text_hash": text_hash(old_text),
        "new_text_hash": text_hash(new_text),
    }
    if old_norm != new_norm:
        out["old_normalized"] = old_norm
        out["new_normalized"] = new_norm
    return out


def _card_summary(number: str, printings: Sequence[JsonDict]) -> JsonDict:
    base = base_printing(printings)
    return {
        "card_number": number,
        "name": base.get("name"),
        "card_type": base.get("card_type"),
        "color": base.get("color"),
        "set_codes": sorted({str(p.get("set_code")) for p in printings}),
        "product_ids": [str(p["product_id"]) for p in printings],
        "effect": base.get("effect"),
    }


def _changes_with_effect(old: Mapping[str, Any], new: Mapping[str, Any]) -> list[JsonDict]:
    changes = _field_changes(old, new, CARD_FIELDS)
    for change in changes:
        if change["field"] == "effect":
            change["effect_change"] = classify_effect(change["old"], change["new"])
    return changes


def diff_cards(old: Snapshot, new: Snapshot) -> JsonDict:
    old_by = old.by_number()
    new_by = new.by_number()
    added = [_card_summary(n, new_by[n]) for n in sorted(set(new_by) - set(old_by))]
    removed = [_card_summary(n, old_by[n]) for n in sorted(set(old_by) - set(new_by))]
    changed: list[JsonDict] = []
    for number in sorted(set(old_by) & set(new_by)):
        ob = base_printing(old_by[number])
        nb = base_printing(new_by[number])
        changes = _changes_with_effect(ob, nb)
        if not changes and ob["product_id"] == nb["product_id"]:
            continue
        changed.append(
            {
                "card_number": number,
                "name": nb.get("name"),
                "old_base_printing": ob["product_id"],
                "new_base_printing": nb["product_id"],
                "changes": changes,
            }
        )
    return {"added": added, "removed": removed, "changed": changed}


def diff_printings(old: Snapshot, new: Snapshot) -> JsonDict:
    old_p = {str(p["product_id"]): p for p in old.printings}
    new_p = {str(p["product_id"]): p for p in new.printings}
    old_numbers = {str(p["card_number"]) for p in old.printings}

    def row(p: Mapping[str, Any]) -> JsonDict:
        return {
            "product_id": p["product_id"],
            "card_number": p["card_number"],
            "set_code": p.get("set_code"),
            "set_name": p.get("set_name"),
        }

    added = [
        {**row(new_p[k]), "new_card_number": new_p[k]["card_number"] not in old_numbers}
        for k in sorted(set(new_p) - set(old_p))
    ]
    removed = [row(old_p[k]) for k in sorted(set(old_p) - set(new_p))]
    changed: list[JsonDict] = []
    cosmetic: Counter[str] = Counter()
    for k in sorted(set(old_p) & set(new_p)):
        o, n = old_p[k], new_p[k]
        changes = _changes_with_effect(o, n)
        other = sorted(f for f in set(o) | set(n) if f not in CARD_FIELDS and o.get(f) != n.get(f))
        cosmetic.update(other)
        if changes:
            changed.append({**row(n), "changes": changes, "other_fields": other})
    return {
        "added": added,
        "removed": removed,
        "changed": changed,
        "cosmetic_field_changes": dict(sorted(cosmetic.items())),
    }


def _keyed_diff(
    old: Iterable[JsonDict], new: Iterable[JsonDict], key: str | tuple[str, ...]
) -> JsonDict:
    def k(item: Mapping[str, Any]) -> str:
        if isinstance(key, str):
            return str(item.get(key))
        return ":".join(str(item.get(part)) for part in key)

    old_i = {k(i): i for i in old}
    new_i = {k(i): i for i in new}
    changed = []
    for ref in sorted(set(old_i) & set(new_i)):
        fields = sorted(set(old_i[ref]) | set(new_i[ref]))
        changes = _field_changes(old_i[ref], new_i[ref], fields)
        if changes:
            changed.append({"key": ref, "changes": changes})
    return {
        "added": [{"key": ref, **new_i[ref]} for ref in sorted(set(new_i) - set(old_i))],
        "removed": [{"key": ref, **old_i[ref]} for ref in sorted(set(old_i) - set(new_i))],
        "changed": changed,
    }


def _field_text(value: Any) -> str:
    return value if isinstance(value, str) else json.dumps(value, ensure_ascii=False)


def official_errata_status(new: Snapshot, official_path: Path = OFFICIAL_ERRATA) -> list[JsonDict]:
    """Official errata notices (edition_language.json) checked against the new snapshot."""
    if not official_path.is_file():
        return []
    data = _read_json(official_path)
    notices = (data.get("errata") or {}).get("official_notices") or []
    overrides = load_overrides().get("resolutions") or {}
    by_number = new.by_number()
    out = []
    for notice in notices:
        number = str(notice.get("card_number"))
        field_name = str(notice.get("field"))
        after = str(notice.get("after") or "")
        printings = by_number.get(number, [])
        covered = sorted(
            cid
            for cid, res in overrides.items()
            if isinstance(res, dict)
            and number in (res.get("card_numbers") or [])
            and field_name in (res.get("field_overrides") or {})
        )
        out.append(
            {
                "card_number": number,
                "field": field_name,
                "date": notice.get("date"),
                "source_url": notice.get("source_url"),
                "in_gcgapi_errata_json": any(
                    str(e.get("card_number")) == number and str(e.get("field")) == field_name
                    for e in new.errata
                ),
                "card_data_reflects_after": bool(printings)
                and all(after in _field_text(p.get(field_name)) for p in printings),
                "override_resolutions": covered,
            }
        )
    return out


def diff_snapshots(old: Snapshot, new: Snapshot) -> JsonDict:
    files = {}
    for local in PACKAGED_FILES:
        o, n = old.sha256(local), new.sha256(local)
        if o is None and n is None:
            status = "missing"
        elif o is None:
            status = "added"
        elif n is None:
            status = "removed"
        else:
            status = "unchanged" if o == n else "changed"
        files[local] = {"old_sha256": o, "new_sha256": n, "status": status}
    manifest: JsonDict = {}
    for key in ("schema_version", "dataset_version", "source_commit", "built_at"):
        o_val, n_val = old.manifest.get(key), new.manifest.get(key)
        manifest[key] = {"old": o_val, "new": n_val, "changed": o_val != n_val}
    manifest["counts"] = {
        key: {"old": old.manifest.get(key), "new": new.manifest.get(key)}
        for key in MANIFEST_COUNTS
        if old.manifest.get(key) != new.manifest.get(key)
    }
    cards = diff_cards(old, new)
    printings = diff_printings(old, new)
    report: JsonDict = {
        "old": {"path": _display(old.root), "layout": old.layout},
        "new": {"path": _display(new.root), "layout": new.layout},
        "files": files,
        "manifest": manifest,
        "cards": cards,
        "printings": printings,
        "rulings": _keyed_diff(old.rulings, new.rulings, ("card_number", "num")),
        "rules_faq": _keyed_diff(old.faq, new.faq, "num"),
        "errata": _keyed_diff(old.errata, new.errata, ("card_number", "field", "date")),
        "products": _keyed_diff(old.products, new.products, "product_id"),
        "sets": _keyed_diff(old.sets, new.sets, "set_code"),
        "official_errata": official_errata_status(new),
    }
    for local, info in files.items():
        if info["status"] == "changed":
            info["note"] = _file_note(local, report)
    report["summary"] = _diff_summary(report)
    return report


def _display(path: Path) -> str:
    try:
        return path.resolve().relative_to(Path.cwd().resolve()).as_posix()
    except ValueError:
        return str(path)


FILE_SECTIONS: dict[str, tuple[str, ...]] = {
    "cards.ndjson": ("cards", "printings"),
    "rulings.json": ("rulings",),
    "rules-faq.json": ("rules_faq",),
    "errata.json": ("errata",),
    "products.json": ("products",),
    "sets/en/index.json": ("sets",),
}


def _file_note(local: str, report: Mapping[str, Any]) -> str:
    if local == "manifest.json":
        return "see the manifest fields"
    sections = FILE_SECTIONS.get(local)
    if sections is None:
        return "not parsed: review the upstream change by hand"
    if any(report[s][k] for s in sections for k in ("added", "removed", "changed")):
        return "record changes"
    if local == "cards.ndjson" and report["printings"]["cosmetic_field_changes"]:
        return "cosmetic printing fields only"
    return "record order or formatting only"


def _diff_summary(report: Mapping[str, Any]) -> JsonDict:
    cards = report["cards"]
    effect_kinds: Counter[str] = Counter()
    stat_changes = 0
    for c in cards["changed"]:
        for ch in c["changes"]:
            if ch["field"] == "effect":
                effect_kinds[ch["effect_change"]["kind"]] += 1
            else:
                stat_changes += 1
    counts = {
        section: {kind: len(report[section][kind]) for kind in ("added", "removed", "changed")}
        for section in ("cards", "printings", "rulings", "rules_faq", "errata", "products", "sets")
    }
    return {
        "has_changes": any(f["status"] != "unchanged" for f in report["files"].values()),
        "counts": counts,
        "effect_wording_changes": effect_kinds["wording"],
        "effect_typography_changes": effect_kinds["typography"],
        "other_card_field_changes": stat_changes,
        "schema_version_changed": report["manifest"]["schema_version"]["changed"],
        "dataset_version_changed": report["manifest"]["dataset_version"]["changed"],
        "official_errata_missing_from_gcgapi": sorted(
            f"{e['card_number']}:{e['field']}"
            for e in report["official_errata"]
            if not e["in_gcgapi_errata_json"]
        ),
    }


def _short(text: Any, limit: int = 120) -> str:
    s = str(text).replace("\n", " / ")
    return s if len(s) <= limit else s[: limit - 1] + "…"


def render_diff(report: Mapping[str, Any]) -> list[str]:
    s = report["summary"]
    m = report["manifest"]
    lines = [
        f"gcg-api diff: {report['old']['path']} -> {report['new']['path']}",
        f"dataset_version {m['dataset_version']['old']} -> {m['dataset_version']['new']}"
        f"; schema_version {m['schema_version']['old']} -> {m['schema_version']['new']}"
        + (
            "  ** SCHEMA CHANGED: review the loader before applying **"
            if s["schema_version_changed"]
            else ""
        ),
    ]
    changed_files = sorted(
        f"{k} ({v['note']})" if v.get("note") else f"{k} ({v['status']})"
        for k, v in report["files"].items()
        if v["status"] != "unchanged"
    )
    lines.append(
        "files: all unchanged"
        if not changed_files
        else f"files changed: {', '.join(changed_files)}"
    )
    for key, val in m["counts"].items():
        lines.append(f"manifest {key}: {val['old']} -> {val['new']}")
    for section, c in s["counts"].items():
        lines.append(
            f"{section}: +{c['added']} new, -{c['removed']} removed, ~{c['changed']} changed"
        )
    lines.append(
        f"card effect changes: {s['effect_wording_changes']} wording, "
        f"{s['effect_typography_changes']} typography-only; other card field changes: "
        f"{s['other_card_field_changes']}"
    )
    cosmetic = report["printings"]["cosmetic_field_changes"]
    if cosmetic:
        lines.append(
            "cosmetic printing fields changed: "
            + ", ".join(f"{k} x{v}" for k, v in cosmetic.items())
        )
    for e in report["official_errata"]:
        if not e["in_gcgapi_errata_json"]:
            lines.append(
                f"official errata missing from gcg-api errata.json: {e['card_number']} {e['field']} "
                f"({e['date']}); card data reflects it: {e['card_data_reflects_after']}; "
                f"overrides: {', '.join(e['override_resolutions']) or 'NONE'}"
            )
    for c in report["cards"]["added"]:
        lines.append(
            f"  NEW {c['card_number']} {c['name']} [{c['card_type']}] {'/'.join(c['set_codes'])}"
        )
    for c in report["cards"]["removed"]:
        lines.append(f"  REMOVED {c['card_number']} {c['name']}")
    for c in report["cards"]["changed"]:
        parts = []
        for ch in c["changes"]:
            if ch["field"] == "effect":
                parts.append(f"effect ({ch['effect_change']['kind']})")
            else:
                parts.append(
                    f"{ch['field']} {_short(ch['old'], 40)!s} -> {_short(ch['new'], 40)!s}"
                )
        if c["old_base_printing"] != c["new_base_printing"]:
            parts.append(f"base printing {c['old_base_printing']} -> {c['new_base_printing']}")
        lines.append(f"  CHANGED {c['card_number']} {c['name']}: {'; '.join(parts)}")
    for p in report["printings"]["added"]:
        if not p["new_card_number"]:
            lines.append(f"  NEW PRINTING {p['product_id']} ({p['set_code']})")
    for p in report["printings"]["removed"]:
        lines.append(f"  REMOVED PRINTING {p['product_id']}")
    for section, label in (
        ("rulings", "RULING"),
        ("rules_faq", "FAQ"),
        ("errata", "ERRATA"),
        ("products", "PRODUCT"),
        ("sets", "SET"),
    ):
        for kind in ("added", "removed", "changed"):
            for item in report[section][kind]:
                fields = (
                    " (" + ", ".join(ch["field"] for ch in item["changes"]) + ")"
                    if kind == "changed"
                    else ""
                )
                lines.append(f"  {label} {kind.upper()} {item['key']}{fields}")
    return lines


# ---------------------------------------------------------------------------------------------
# apply


def default_root() -> Path:
    for parent in Path(__file__).resolve().parents:
        if (parent / "pyproject.toml").is_file() and (parent / "src" / "gcg_sim").is_dir():
            return parent
    return Path.cwd()


def _git(repo: Path, *args: str) -> bytes:
    env = {**os.environ, "TZ": "UTC"}
    try:
        return subprocess.run(
            ["git", "-C", str(repo), *args], capture_output=True, check=True, env=env
        ).stdout
    except (OSError, subprocess.CalledProcessError) as exc:
        raise RefreshError(f"git {' '.join(args)} failed in {repo}: {exc}") from exc


def clone_commit(snapshot: Snapshot) -> str | None:
    """HEAD of a gcg-api clone root; None for a packaged copy (never the simulator's repo)."""
    if snapshot.layout != "clone" or not (snapshot.root / ".git").exists():
        return None
    return _git(snapshot.root, "rev-parse", "HEAD").decode("ascii").strip()


def pinned_commit(fragment: Sequence[Mapping[str, Any]]) -> str | None:
    for e in fragment:
        if e.get("id") == "gcgapi:commit":
            return str(e.get("url", "")).rsplit("/", 1)[-1] or None
    return None


def _canonical_json(record: Any) -> str:
    return json.dumps(record, sort_keys=True, ensure_ascii=False)


def build_fragment(
    new: Snapshot, commit: str, retrieved_at: str, existing: Sequence[JsonDict]
) -> tuple[list[JsonDict], list[str]]:
    """Provenance entries for a clone at ``commit``; unknown existing ids are kept as they are."""
    warnings: list[str] = []
    raw_base = f"https://raw.githubusercontent.com/{UPSTREAM_REPO}/{commit}/"
    common = {
        "version": new.manifest.get("dataset_version"),
        "effective_date": new.manifest.get("built_at"),
        "retrieved_at": retrieved_at,
    }
    entries: list[JsonDict] = []
    for local, upstream in PACKAGED_FILES.items():
        entries.append(
            {
                "id": f"gcgapi:{upstream}",
                "kind": FILE_KINDS.get(local, "data-file"),
                "url": raw_base + upstream,
                "sha256": sha256_file(new.paths[local]),
                "local_path": (DEST_REL / local).as_posix(),
                "notes": f"Byte-identical to the file at commit {commit} (installed from the clone "
                "by gcg_sim.tools.refresh apply; sha256 of the packaged copy equals sha256 of the "
                "clone file).",
                **common,
            }
        )
    bulk = new.root / "data" / "cards.json"
    if bulk.is_file():
        same = _read_json(bulk) == new.printings
        if not same:
            warnings.append("data/cards.json differs from data/cards.ndjson")
        entries.append(
            {
                "id": "gcgapi:data/cards.json",
                "kind": "data-file",
                "url": raw_base + "data/cards.json",
                "sha256": sha256_file(bulk),
                "local_path": "not-packaged",
                "notes": (
                    f"Omitted from the package: same {len(new.printings):,} records in the same "
                    "order as data/cards.ndjson (checked by gcg_sim.tools.refresh apply)."
                    if same
                    else "Omitted from the package; DIFFERS from data/cards.ndjson (reported by "
                    "gcg_sim.tools.refresh apply)."
                ),
                **common,
            }
        )
    else:
        warnings.append("data/cards.json is missing from the clone; its lock entry is dropped")
    per_set = sorted((new.root / "data" / "cards" / "en").glob("*.json"))
    if per_set:
        listing = "".join(f"{sha256_file(p)}  {p.name}\n" for p in per_set)
        records: Counter[str] = Counter()
        for p in per_set:
            records.update(_canonical_json(r) for r in _read_json(p))
        same_multiset = records == Counter(_canonical_json(r) for r in new.printings)
        if not same_multiset:
            warnings.append("data/cards/en/*.json records differ from data/cards.ndjson")
        entries.append(
            {
                "id": "gcgapi:data/cards/en/*.json",
                "kind": "data-file-set",
                "url": raw_base + "data/cards/en/",
                "sha256": hashlib.sha256(listing.encode("utf-8")).hexdigest(),
                "local_path": "not-packaged",
                "notes": f"Aggregate of {len(per_set)} per-set files "
                f"({', '.join(p.name for p in per_set)}). sha256 is taken over the text "
                "'<sha256>  <file name>\\n' per file, sorted by file name, i.e. "
                "`(cd data/cards/en && shasum -a 256 *.json) | shasum -a 256`. "
                + (
                    "Same multiset of records as data/cards.ndjson."
                    if same_multiset
                    else "Records DIFFER from data/cards.ndjson."
                ),
                **common,
            }
        )
    else:
        warnings.append("data/cards/en/*.json is missing from the clone; its lock entry is dropped")
    commit_object = _git(new.root, "cat-file", "commit", commit)
    subject = _git(new.root, "log", "-1", "--format=%s", commit).decode("utf-8").strip()
    committed = (
        _git(
            new.root, "log", "-1", "--date=format-local:%Y-%m-%dT%H:%M:%SZ", "--format=%cd", commit
        )
        .decode("ascii")
        .strip()
    )
    entries.append(
        {
            "id": "gcgapi:commit",
            "kind": "git-commit",
            "url": f"https://github.com/{UPSTREAM_REPO}/commit/{commit}",
            "sha256": hashlib.sha256(commit_object).hexdigest(),
            "local_path": "not-packaged",
            "notes": f"Data commit '{subject}', git object id {commit} (SHA-1), committed "
            f"{committed}. sha256 is over the raw commit object (`git cat-file commit {commit} | "
            "shasum -a 256`). dataset_version's sha "
            f"({new.manifest.get('source_commit')}) is the code commit that produced the data, "
            "not this one.",
            **common,
        }
    )
    produced = {e["id"] for e in entries}
    entries += [dict(e) for e in existing if e.get("id") not in produced]
    return sorted(entries, key=lambda e: str(e["id"])), warnings


@dataclass
class ApplyPlan:
    copies: list[tuple[str, Path, Path, str]] = field(default_factory=list)
    fragment_path: Path = Path()
    fragment_old: list[JsonDict] = field(default_factory=list)
    fragment_new: list[JsonDict] = field(default_factory=list)
    old_commit: str | None = None
    new_commit: str | None = None
    warnings: list[str] = field(default_factory=list)

    @property
    def file_changes(self) -> list[tuple[str, Path, Path, str]]:
        return [c for c in self.copies if c[3] != "unchanged"]

    @property
    def fragment_changed(self) -> bool:
        return self.fragment_old != self.fragment_new

    @property
    def has_changes(self) -> bool:
        return bool(self.file_changes) or self.fragment_changed


def plan_apply(
    new_dir: Path,
    root: Path,
    retrieved_at: str | None = None,
    allow_schema_change: bool = False,
) -> ApplyPlan:
    new = load_snapshot(new_dir)
    missing = [str(p) for p in new.paths.values() if not p.is_file()]
    if missing:
        raise RefreshError(f"{new_dir}: packaged files missing: {missing}")
    fragment_path = root / FRAGMENT_REL
    if not fragment_path.parent.is_dir():
        raise RefreshError(f"{root} is not a gcg-sim checkout ({FRAGMENT_REL.parent} missing)")
    dest = root / DEST_REL
    plan = ApplyPlan(fragment_path=fragment_path)
    for local in PACKAGED_FILES:
        src, dst = new.paths[local], dest / local
        if not dst.is_file():
            status = "add"
        elif sha256_file(src) == sha256_file(dst):
            status = "unchanged"
        else:
            status = "update"
        plan.copies.append((local, src, dst, status))
    current_manifest = dest / "manifest.json"
    if current_manifest.is_file():
        old_schema = _read_json(current_manifest).get("schema_version")
        new_schema = new.manifest.get("schema_version")
        if old_schema != new_schema and not allow_schema_change:
            raise RefreshError(
                f"manifest schema_version changes {old_schema} -> {new_schema}; review the "
                "upstream schema and the loaders first, then re-run with --allow-schema-change"
            )
    existing: list[JsonDict] = _read_json(fragment_path) if fragment_path.is_file() else []
    plan.fragment_old = existing
    plan.old_commit = pinned_commit(existing)
    plan.new_commit = clone_commit(new)
    same_pin = plan.new_commit is None or plan.new_commit == plan.old_commit
    if not plan.file_changes and same_pin:
        plan.fragment_new = existing
        return plan
    if plan.new_commit is None:
        raise RefreshError(
            f"{new_dir} differs from the packaged copy but is not a gcg-api git clone root, so "
            "its commit cannot be recorded; fetch it with the skill's scripts/fetch_gcgapi.sh"
        )
    stamp = retrieved_at or str(new.manifest.get("built_at") or "")
    if not stamp:
        raise RefreshError("pass --retrieved-at (the manifest has no built_at)")
    plan.fragment_new, plan.warnings = build_fragment(new, plan.new_commit, stamp, existing)
    return plan


def execute_apply(plan: ApplyPlan) -> None:
    for _, src, dst, status in plan.copies:
        if status != "unchanged":
            dst.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(src, dst)
    if plan.fragment_changed:
        plan.fragment_path.write_text(_dump_json(plan.fragment_new), encoding="utf-8")


def render_plan(plan: ApplyPlan, dry_run: bool) -> list[str]:
    lines = [f"pinned commit: {plan.old_commit} -> {plan.new_commit or plan.old_commit}"]
    for local, _, dst, status in plan.copies:
        if status != "unchanged":
            lines.append(f"  {status}: {dst}  ({local})")
    unchanged = sum(1 for c in plan.copies if c[3] == "unchanged")
    lines.append(f"files: {len(plan.file_changes)} to install, {unchanged} unchanged")
    old_ids = {e.get("id"): e for e in plan.fragment_old}
    for e in plan.fragment_new:
        prev = old_ids.get(e["id"])
        if prev is None:
            lines.append(f"  lock entry added: {e['id']}")
        elif prev != e:
            keys = sorted(k for k in set(prev) | set(e) if prev.get(k) != e.get(k))
            lines.append(f"  lock entry {e['id']}: {', '.join(keys)}")
    for gone in sorted(str(i) for i in set(old_ids) - {e["id"] for e in plan.fragment_new}):
        lines.append(f"  lock entry removed: {gone}")
    lines += [f"warning: {w}" for w in plan.warnings]
    if not plan.has_changes:
        lines.append("no changes: the packaged gcg-api snapshot and its lock entries are current")
        return lines
    verb = "would update" if dry_run else "updated"
    lines.append(
        f"{verb} {plan.fragment_path}" if plan.fragment_changed else "lock fragment unchanged"
    )
    lines.append("dry run: nothing was written" if dry_run else "next, run:")
    if not dry_run:
        lines += [f"  {cmd}" for cmd in FOLLOW_UP]
    return lines


# ---------------------------------------------------------------------------------------------
# coverage


def _prefix(card_number: str) -> str:
    return card_number.split("-", 1)[0]


def load_golden(golden_dir: Path) -> dict[str, JsonDict]:
    out: dict[str, JsonDict] = {}
    for path in sorted(golden_dir.glob("*.json")):
        data = _read_json(path)
        if isinstance(data, dict):
            out.update({str(k): v for k, v in data.items() if isinstance(v, dict)})
    return out


def coverage_report(tests_dir: Path, golden_dir: Path) -> JsonDict:
    from gcg_sim.effects.registry import get_registry

    reg = get_registry()
    real = reg.db.real_cards()
    golden = load_golden(golden_dir)
    tagged = values(scan(tests_dir, ("card",)), "card") if tests_dir.is_dir() else set()
    unimplemented: list[JsonDict] = []
    text_changed: list[JsonDict] = []
    not_in_golden: list[str] = []
    untested: list[str] = []
    for cdef in real:
        entry = reg.cards[cdef.def_id]
        number = cdef.card_number
        if entry.script is None:
            unimplemented.append({"card_number": number, "name": cdef.name, "error": entry.error})
        current = text_hash(cdef.effect)
        rec = golden.get(number)
        if rec is None:
            not_in_golden.append(number)
        elif rec.get("text_hash") != current:
            text_changed.append(
                {
                    "card_number": number,
                    "name": cdef.name,
                    "golden_text_hash": rec.get("text_hash"),
                    "current_text_hash": current,
                }
            )
        vanilla = entry.script is not None and entry.script.source == "vanilla"
        if not vanilla and number not in tagged:
            untested.append(number)
    numbers = {c.card_number for c in real}
    by_set: dict[str, JsonDict] = {}
    groups = (
        ("unimplemented", [u["card_number"] for u in unimplemented]),
        ("text_changed", [t["card_number"] for t in text_changed]),
        ("not_in_golden", not_in_golden),
        ("untested", untested),
    )
    for label, items in groups:
        for n in items:
            row = by_set.setdefault(
                _prefix(n),
                {
                    "unimplemented": 0,
                    "text_changed": 0,
                    "not_in_golden": 0,
                    "untested": 0,
                    "cards": [],
                },
            )
            row[label] += 1
            if n not in row["cards"]:
                row["cards"].append(n)
    for row in by_set.values():
        row["cards"].sort()
    needs_work = sorted({n for _, items in groups for n in items})
    return {
        "counts": {
            "card_numbers": len(real),
            "unimplemented": len(unimplemented),
            "text_changed": len(text_changed),
            "not_in_golden": len(not_in_golden),
            "untested": len(untested),
            "needs_work": len(needs_work),
        },
        "unimplemented": unimplemented,
        "text_changed": text_changed,
        "not_in_golden": not_in_golden,
        "golden_only": sorted(set(golden) - numbers),
        "untested": untested,
        "unknown_card_tags": sorted(t for t in tagged if t not in numbers),
        "by_set": dict(sorted(by_set.items())),
        "needs_work": needs_work,
    }


def render_coverage(report: Mapping[str, Any]) -> list[str]:
    c = report["counts"]
    lines = [
        f"{c['card_numbers']} card numbers: {c['unimplemented']} fail to compile and have no "
        f"binding, {c['text_changed']} golden text_hash changed, {c['not_in_golden']} not in the "
        f"golden files, {c['untested']} without a @pytest.mark.card test",
        "set    unimpl  text  new  untested",
    ]
    for prefix, row in report["by_set"].items():
        lines.append(
            f"{prefix:<6} {row['unimplemented']:>6} {row['text_changed']:>5} "
            f"{row['not_in_golden']:>4} {row['untested']:>9}"
        )
    for u in report["unimplemented"]:
        lines.append(f"  UNIMPLEMENTED {u['card_number']}: {_short(u['error'], 160)}")
    for t in report["text_changed"]:
        lines.append(
            f"  TEXT CHANGED {t['card_number']} {t['name']}: golden {t['golden_text_hash']} "
            f"!= current {t['current_text_hash']}"
        )
    if report["not_in_golden"]:
        lines.append("  NOT IN GOLDEN: " + " ".join(report["not_in_golden"]))
    if report["golden_only"]:
        lines.append("  GOLDEN ONLY (card removed from data): " + " ".join(report["golden_only"]))
    if report["unknown_card_tags"]:
        lines.append("  TESTS TAG UNKNOWN CARDS: " + " ".join(report["unknown_card_tags"]))
    return lines


# ---------------------------------------------------------------------------------------------
# banlist


def matches(pred: Mapping[str, Any], card: Mapping[str, Any], bound: str | None = None) -> bool:
    """Evaluate a banlist.json predicate (see its ``predicate_language``) on a card view."""
    if len(pred) != 1:
        raise RefreshError(f"a predicate needs exactly one operator: {dict(pred)}")
    ((op, val),) = pred.items()
    if op == "all_of":
        return all(matches(p, card, bound) for p in val)
    if op == "any_of":
        return any(matches(p, card, bound) for p in val)
    if op == "not":
        return not matches(val, card, bound)
    if op == "card_number":
        return bool(card["card_number"] == (bound if val == "$matched" else val))
    if op == "trait":
        return val in card["traits"]
    if op == "name_contains":
        return str(val) in str(card["name"])
    if op in ("card_type", "color", "level", "cost", "ap", "hp", "has_effect"):
        return bool(card[op] == val)
    raise RefreshError(f"unknown predicate operator {op!r}")


def _has_effect(effect: Any) -> bool:
    return str(effect or "").strip() not in ("", "-")


def card_view(cdef: CardDef) -> JsonDict:
    """Canonical card data after overrides, as banlist predicates see it."""
    return {
        "card_number": cdef.card_number,
        "name": cdef.name,
        "card_type": cdef.card_type.value,
        "color": cdef.color.value if cdef.color else None,
        "level": cdef.level,
        "cost": cdef.cost,
        "ap": cdef.ap,
        "hp": cdef.hp,
        "traits": list(cdef.traits),
        "has_effect": _has_effect(cdef.effect),
    }


def printing_view(rec: Mapping[str, Any]) -> JsonDict:
    return {
        "card_number": rec.get("card_number"),
        "name": rec.get("name"),
        "card_type": rec.get("card_type"),
        "color": rec.get("color"),
        "level": rec.get("level"),
        "cost": rec.get("cost"),
        "ap": rec.get("ap"),
        "hp": rec.get("hp"),
        "traits": list(rec.get("traits") or ()),
        "has_effect": _has_effect(rec.get("effect")),
    }


def _name_key(name: str) -> str:
    return unicodedata.normalize("NFKC", name).replace("\u200b", "").replace("’", "'")


@dataclass
class BanlistReport:
    problems: list[str] = field(default_factory=list)
    warnings: list[str] = field(default_factory=list)
    info: list[str] = field(default_factory=list)


def check_banlist(banlist: Mapping[str, Any], snapshot: Snapshot) -> BanlistReport:
    report = BanlistReport()
    db = CardDB(snapshot.printings, load_overrides())
    by_number = snapshot.by_number()

    def check_card(number: Any, official_name: Any, where: str) -> None:
        cdef = db.get(str(number))
        if cdef is None or cdef.card_number != number:
            report.problems.append(f"{where}: card number {number} is not in the card data")
            return
        if official_name is None:
            return
        if cdef.name == official_name:
            return
        if _name_key(cdef.name) == _name_key(str(official_name)):
            report.warnings.append(
                f"{where}: {number} name {official_name!r} matches data {cdef.name!r} only after "
                "typographic normalization"
            )
        else:
            report.problems.append(
                f"{where}: {number} is named {official_name!r} in banlist.json but {cdef.name!r} "
                "in the card data"
            )

    checked: set[str] = set()
    for b in banlist.get("banned") or []:
        check_card(b.get("card_number"), b.get("name"), "banned")
        checked.add(str(b.get("card_number")))
    for r in banlist.get("restricted") or []:
        check_card(r.get("card_number"), r.get("name"), "restricted")
        checked.add(str(r.get("card_number")))
        max_copies = r.get("max_copies")
        if not isinstance(max_copies, int) or not 1 <= max_copies <= 3:
            report.problems.append(
                f"restricted {r.get('card_number')}: max_copies must be 1..3, got {max_copies!r}"
            )
    for pair in banlist.get("banned_pairs") or []:
        cards = list(pair.get("cards") or [])
        names = list(pair.get("names") or [None] * len(cards))
        if len(cards) != 2 or len(set(cards)) != 2 or len(names) != len(cards):
            report.problems.append(
                f"banned pair {pair.get('id')}: needs two distinct cards and names"
            )
        for number, name in zip(cards, names, strict=False):
            check_card(number, name, f"banned pair {pair.get('id')}")
            checked.add(str(number))
    for rule in banlist.get("attribute_pair_rules") or []:
        rid = rule.get("id")
        members = rule.get("enumerated_members") or []
        for m in members:
            check_card(m.get("card_number"), m.get("name"), f"attribute rule {rid}")
            checked.add(str(m.get("card_number")))
        pred = rule.get("member_predicate")
        if not isinstance(pred, dict):
            report.problems.append(f"attribute rule {rid}: member_predicate is missing")
            continue
        computed = sorted(c.card_number for c in db.real_cards() if matches(pred, card_view(c)))
        enumerated = sorted(str(m.get("card_number")) for m in members)
        equal = computed == enumerated
        if not equal:
            only_pred = sorted(set(computed) - set(enumerated))
            only_list = sorted(set(enumerated) - set(computed))
            report.problems.append(
                f"attribute rule {rid}: the predicate matches {len(computed)} card numbers but the "
                f"enumerated list has {len(enumerated)}; only the predicate matches {only_pred} "
                "(new matching cards: confirm on the official B&R page and add them to "
                f"enumerated_members); only listed {only_list}"
            )
        if rule.get("predicate_equals_enumeration") is not equal:
            report.problems.append(
                f"attribute rule {rid}: predicate_equals_enumeration is "
                f"{rule.get('predicate_equals_enumeration')!r} but the check says {equal}"
            )
        recorded = rule.get("predicate_matches_in_card_data")
        if recorded is not None and sorted(recorded) != computed:
            report.problems.append(
                f"attribute rule {rid}: predicate_matches_in_card_data is stale; the card data "
                f"matches {computed}"
            )
        disagreeing = sorted(
            str(p["product_id"])
            for recs in by_number.values()
            for p in recs
            if matches(pred, printing_view(p)) != matches(pred, printing_view(base_printing(recs)))
        )
        recorded_dis = rule.get("printings_disagreeing_with_canonical_printing")
        if recorded_dis is not None and sorted(recorded_dis) != disagreeing:
            report.problems.append(
                f"attribute rule {rid}: printings_disagreeing_with_canonical_printing is stale; "
                f"the card data gives {disagreeing}"
            )
        report.info.append(
            f"attribute rule {rid}: predicate matches {len(computed)} card numbers"
            + (", identical to the enumerated list" if equal else "")
        )
    overlap = {str(b.get("card_number")) for b in banlist.get("banned") or []} & {
        str(r.get("card_number")) for r in banlist.get("restricted") or []
    }
    if overlap:
        report.problems.append(f"cards both banned and restricted: {sorted(overlap)}")
    verification = banlist.get("verification") or {}
    recorded_version = verification.get("dataset_version")
    current_version = snapshot.manifest.get("dataset_version")
    if recorded_version and recorded_version != current_version:
        report.warnings.append(
            f"banlist.json verification block records dataset {recorded_version}; this check "
            f"re-verified it against {current_version} (refresh the block when banlist.json is "
            "next regenerated)"
        )
    report.info.insert(0, f"{len(checked)} card numbers checked against dataset {current_version}")
    return report


# ---------------------------------------------------------------------------------------------
# CLI


def _write_json(path: Path | None, data: Any) -> None:
    if path is not None:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(_dump_json(data), encoding="utf-8")


def _cmd_diff(args: argparse.Namespace) -> int:
    report = diff_snapshots(load_snapshot(args.old_data), load_snapshot(args.new_data))
    _write_json(args.json, report)
    print("\n".join(render_diff(report)))
    return 0


def _cmd_apply(args: argparse.Namespace) -> int:
    plan = plan_apply(args.new_data, args.root, args.retrieved_at, args.allow_schema_change)
    if not args.dry_run:
        execute_apply(plan)
    print("\n".join(render_plan(plan, args.dry_run)))
    return 0


def _cmd_coverage(args: argparse.Namespace) -> int:
    report = coverage_report(args.tests, args.golden_dir)
    _write_json(args.json, report)
    print("\n".join(render_coverage(report)))
    return 1 if args.fail_on_gaps and report["needs_work"] else 0


def _cmd_banlist(args: argparse.Namespace) -> int:
    data = _read_json(args.banlist)
    if not isinstance(data, dict):
        raise RefreshError(f"{args.banlist} must contain a JSON object")
    report = check_banlist(data, load_snapshot(args.data))
    _write_json(
        args.json,
        {"problems": report.problems, "warnings": report.warnings, "info": report.info},
    )
    for line in report.info:
        print(line)
    for line in report.warnings:
        print(f"warning: {line}")
    for line in report.problems:
        print(f"problem: {line}")
    print("banlist OK" if not report.problems else f"banlist: {len(report.problems)} problem(s)")
    return 1 if report.problems else 0


def build_parser() -> argparse.ArgumentParser:
    root = default_root()
    ap = argparse.ArgumentParser(
        prog="python -m gcg_sim.tools.refresh",
        description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    sub = ap.add_subparsers(dest="command", required=True)

    diff = sub.add_parser("diff", help="compare two gcg-api data directories")
    diff.add_argument("--new-data", type=Path, required=True)
    diff.add_argument("--old-data", type=Path, default=PACKAGED_GCGAPI)
    diff.add_argument("--json", type=Path, help="write the full report as JSON")
    diff.set_defaults(func=_cmd_diff)

    apply = sub.add_parser("apply", help="install a gcg-api clone into the package and the lock")
    apply.add_argument("--new-data", type=Path, required=True, help="gcg-api clone root")
    apply.add_argument("--root", type=Path, default=root, help="gcg-sim checkout to update")
    apply.add_argument("--retrieved-at", help="UTC fetch time (default: the manifest's built_at)")
    apply.add_argument("--dry-run", action="store_true", help="print the plan; change nothing")
    apply.add_argument("--allow-schema-change", action="store_true")
    apply.set_defaults(func=_cmd_apply)

    cov = sub.add_parser("coverage", help="cards that need implementation or review work")
    cov.add_argument("--tests", type=Path, default=root / "tests")
    cov.add_argument("--golden-dir", type=Path, default=root / "tests" / "effects" / "golden")
    cov.add_argument("--json", type=Path)
    cov.add_argument("--fail-on-gaps", action="store_true", help="exit 1 if any card needs work")
    cov.set_defaults(func=_cmd_coverage)

    ban = sub.add_parser("banlist", help="validate banlist.json against the card data")
    ban.add_argument("--banlist", type=Path, default=DEFAULT_BANLIST)
    ban.add_argument("--data", type=Path, default=PACKAGED_GCGAPI, help="gcg-api data directory")
    ban.add_argument("--json", type=Path)
    ban.set_defaults(func=_cmd_banlist)
    return ap


def main(argv: Sequence[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    try:
        code: int = args.func(args)
    except RefreshError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2
    return code


if __name__ == "__main__":
    raise SystemExit(main())
