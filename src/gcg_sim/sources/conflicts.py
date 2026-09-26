"""Detect and report conflicts between the cached sources.

The detectors read only raw snapshot JSON (the gcg-api files), the comprehensive-rules markdown,
optional official-site records, the human-curated conflict list and the reviewable overrides
file. Nothing here imports another ``gcg_sim`` module, so the report can be regenerated even
when the engine is broken.

Regenerate the report (``data/conflicts.json`` and ``docs/CONFLICTS.md``)::

    uv run python -m gcg_sim.sources.conflicts

Verify it is current and that every conflict has a valid resolution::

    uv run python -m gcg_sim.sources.conflicts --check
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
import unicodedata
from collections import Counter, defaultdict
from collections.abc import Callable, Iterable, Mapping, Sequence
from dataclasses import dataclass, field
from datetime import datetime
from pathlib import Path
from typing import Any, Final

JsonDict = dict[str, Any]

SCHEMA_VERSION: Final = 1
SEVERITIES: Final = ("info", "minor", "major")

KIND_TITLES: Final[dict[str, str]] = {
    "divergent_printing": "Divergent printings",
    "errata": "Errata",
    "rules_version": "Rules version",
    "banlist_ambiguity": "Banned/restricted list ambiguities",
    "marker_mismatch": "Keyword and timing markers vs effect text",
    "data_vs_rules": "Card data vs rules",
    "rules_xref": "Rules cross-references",
    "rules_internal": "Rules internal inconsistencies",
    "ruling_vs_text": "Card rulings vs card text",
    "faq_vs_rules": "Rules FAQ vs rules",
    "ambiguous_text": "Ambiguous card text",
}
KIND_ORDER: Final = tuple(KIND_TITLES)
CURATED_KINDS: Final = frozenset(
    {
        "rules_xref",
        "rules_internal",
        "errata",
        "ruling_vs_text",
        "faq_vs_rules",
        "ambiguous_text",
        "data_vs_rules",
        "divergent_printing",
        "marker_mismatch",
    }
)

GAMEPLAY_FIELDS: Final = (
    "name",
    "card_type",
    "color",
    "level",
    "cost",
    "ap",
    "hp",
    "ap_raw",
    "hp_raw",
    "zone",
    "trait",
    "link",
    "traits",
    "link_refs",
    "keyword_effects",
    "timing_markers",
    "keywords_text",
    "effect",
)
STAT_FIELDS: Final = frozenset(
    {
        "name",
        "card_type",
        "color",
        "level",
        "cost",
        "ap",
        "hp",
        "zone",
        "trait",
        "traits",
        "link",
        "link_refs",
    }
)
RAW_STAT_FIELDS: Final = frozenset({"ap_raw", "hp_raw"})
INT_FIELDS: Final = frozenset({"level", "cost", "ap", "hp"})
STR_FIELDS: Final = frozenset(
    {
        "name",
        "card_type",
        "color",
        "ap_raw",
        "hp_raw",
        "zone",
        "trait",
        "link",
        "keywords_text",
        "effect",
    }
)
STR_LIST_FIELDS: Final = frozenset({"traits", "link_refs", "timing_markers"})

TIMING_VOCABULARY: Final = (
    "Activate",
    "Main",
    "Action",
    "Burst",
    "Deploy",
    "Attack",
    "Destroyed",
    "When Paired",
    "During Pair",
    "When Linked",
    "During Link",
    "Once per Turn",
)
KEYWORD_VOCABULARY: Final = (
    "Repair",
    "Breach",
    "Support",
    "Blocker",
    "First Strike",
    "High-Maneuver",
    "Suppression",
)
EDITION_BETA_PREFIX: Final = "Edition Beta"
MARKDOWN_ESCAPES: Final = str.maketrans("", "", "\\")

_PACKAGE_DATA: Final = Path(__file__).resolve().parent.parent / "data"
DEFAULT_DATA_DIR: Final = _PACKAGE_DATA / "gcgapi"
DEFAULT_RULES: Final = _PACKAGE_DATA / "rules" / "gundam-card-game-comprehensive-rules.md"
DEFAULT_OFFICIAL_DIR: Final = _PACKAGE_DATA / "official"
DEFAULT_CURATED: Final = _PACKAGE_DATA / "curated_conflicts.json"
DEFAULT_OVERRIDES: Final = _PACKAGE_DATA / "overrides.json"

_KEYWORD_TAG: Final = re.compile(r"<([A-Za-z][A-Za-z \-]*?)(?:\s+(\d+))?>")
_TIMING_TAG: Final = re.compile(r"【([^【】]+)】")
_TIMING_SPLIT: Final = re.compile(r"[・･]")
_BARE_KEYWORD: Final = re.compile(
    r"(?<![<(A-Za-z])(" + "|".join(re.escape(k) for k in KEYWORD_VOCABULARY) + r")(?![A-Za-z>)])"
)
_ASCII_COST_COLON: Final = re.compile(r"(?<!\d):\s")
_REMINDER: Final = re.compile(r"\(([^()]*)\)")
_LOWERCASE_WORD: Final = re.compile(r"\b[a-z]{2,}\b")
_RULE_HEADING_TOP: Final = re.compile(r"^#{2,6}\s+(\d+)\)\s+(.+?)\s*$")
_RULE_HEADING: Final = re.compile(r"^#{2,6}\s+(\d+(?:-\d+)*)\.\s+(.+?)\s*$")
_RULE_PARAGRAPH: Final = re.compile(r"^\*\*(\d+(?:-\d+)*)\.\*\*\s*(.*)$")
_RULE_VERSION: Final = re.compile(r"^Ver\.\s*(\S+)\s*$")
_RULE_UPDATED: Final = re.compile(r"^Updated\s+(.+?)\s*$")
_XREF_PAREN: Final = re.compile(r"\((?:[Ss]ee\s+)?(\d{1,2}(?:-\d{1,3})*)(?:\.\s*([^()]*?))?\s*\)")
_WORD: Final = re.compile(r"[a-z]+")
_SUFFIX_NUMBER: Final = re.compile(r"_p(\d+)$")


class SourceError(Exception):
    """A required input is missing or unreadable."""


# ---------------------------------------------------------------------------------------------
# Data model


@dataclass
class Conflict:
    """One detected or curated conflict between sources."""

    id: str
    kind: str
    severity: str
    card_numbers: list[str]
    title: str
    description: str
    sources: list[str]
    details: JsonDict = field(default_factory=dict)
    origin: str = "detected"
    engine_behaviour: bool = False
    problems: list[str] = field(default_factory=list)

    def to_json(self) -> JsonDict:
        return {
            "id": self.id,
            "kind": self.kind,
            "severity": self.severity,
            "card_numbers": self.card_numbers,
            "title": self.title,
            "description": self.description,
            "engine_behaviour": self.engine_behaviour,
            "origin": self.origin,
            "sources": self.sources,
            "details": self.details,
            "problems": self.problems,
        }


@dataclass(frozen=True)
class RulesDoc:
    """The comprehensive rules split into numbered paragraphs and section headings."""

    version: str | None
    updated: str | None
    headings: dict[str, str]
    paragraphs: dict[str, str]
    order: tuple[str, ...]

    def text_of(self, rule_id: str) -> str | None:
        if rule_id in self.paragraphs:
            return self.paragraphs[rule_id]
        return self.headings.get(rule_id)


@dataclass(frozen=True)
class Inputs:
    """Every source the report is built from, already parsed."""

    cards: list[JsonDict]
    rulings: list[JsonDict]
    faq: list[JsonDict]
    errata: list[JsonDict]
    manifest: JsonDict
    rules: RulesDoc
    official_rules_version: JsonDict | None
    banlist: JsonDict | None
    curated: JsonDict
    overrides: JsonDict
    fingerprints: dict[str, str]


@dataclass
class Report:
    """The merged conflict list plus resolution bookkeeping."""

    conflicts: list[Conflict]
    policies: JsonDict
    resolutions: dict[str, JsonDict]
    unresolved: list[str]
    invalid: list[str]
    stale: list[str]
    orphaned: list[str]
    inputs: JsonDict

    @property
    def check_failures(self) -> list[str]:
        failures = [f"unresolved conflict: {cid}" for cid in self.unresolved]
        failures += [f"invalid: {msg}" for msg in self.invalid]
        failures += [f"stale curated evidence: {cid}" for cid in self.stale]
        return failures


# ---------------------------------------------------------------------------------------------
# Loading


def _sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def _read_json(path: Path) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise SourceError(f"cannot read {path}: {exc}") from exc


def _read_optional_list(path: Path, fingerprints: dict[str, str], label: str) -> list[JsonDict]:
    if not path.exists():
        return []
    fingerprints[label] = _sha256(path)
    data = _read_json(path)
    if not isinstance(data, list):
        raise SourceError(f"{path} must contain a JSON array")
    return [item for item in data if isinstance(item, dict)]


def _read_optional_object(path: Path, fingerprints: dict[str, str], label: str) -> JsonDict | None:
    if not path.exists():
        return None
    fingerprints[label] = _sha256(path)
    data = _read_json(path)
    if not isinstance(data, dict):
        raise SourceError(f"{path} must contain a JSON object")
    return data


def load_cards(path: Path) -> list[JsonDict]:
    if not path.exists():
        raise SourceError(f"missing card snapshot {path}")
    cards: list[JsonDict] = []
    for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
        if not line.strip():
            continue
        try:
            record = json.loads(line)
        except json.JSONDecodeError as exc:
            raise SourceError(f"{path}:{number}: {exc}") from exc
        if not isinstance(record, dict) or "card_number" not in record:
            raise SourceError(f"{path}:{number}: not a card record")
        cards.append(record)
    return cards


def _parse_rules_date(raw: str) -> str | None:
    for fmt in ("%b %d, %Y", "%B %d, %Y", "%Y-%m-%d"):
        try:
            return datetime.strptime(raw.strip(), fmt).date().isoformat()
        except ValueError:
            continue
    return None


def parse_rules(text: str) -> RulesDoc:
    version: str | None = None
    updated: str | None = None
    headings: dict[str, str] = {}
    paragraphs: dict[str, str] = {}
    order: list[str] = []
    current: str | None = None
    for line in text.splitlines():
        stripped = line.strip()
        if version is None and (match := _RULE_VERSION.match(stripped)):
            version = match.group(1)
            continue
        if updated is None and (match := _RULE_UPDATED.match(stripped)):
            updated = _parse_rules_date(match.group(1))
            continue
        heading = _RULE_HEADING_TOP.match(stripped) or _RULE_HEADING.match(stripped)
        if heading:
            rule_id, title = heading.group(1), heading.group(2).translate(MARKDOWN_ESCAPES)
            headings[rule_id] = title
            if rule_id not in order:
                order.append(rule_id)
            current = None
            continue
        paragraph = _RULE_PARAGRAPH.match(stripped)
        if paragraph:
            current = paragraph.group(1)
            paragraphs[current] = paragraph.group(2)
            if current not in order:
                order.append(current)
            continue
        if current is not None and stripped.startswith(">"):
            paragraphs[current] += " " + stripped.lstrip("> ").strip()
    return RulesDoc(version, updated, headings, paragraphs, tuple(order))


def load_inputs(
    data_dir: Path = DEFAULT_DATA_DIR,
    rules_path: Path = DEFAULT_RULES,
    official_dir: Path = DEFAULT_OFFICIAL_DIR,
    curated_path: Path = DEFAULT_CURATED,
    overrides_path: Path = DEFAULT_OVERRIDES,
) -> Inputs:
    fingerprints: dict[str, str] = {}
    cards_path = data_dir / "cards.ndjson"
    cards = load_cards(cards_path)
    fingerprints["gcgapi/cards.ndjson"] = _sha256(cards_path)
    rulings = _read_optional_list(data_dir / "rulings.json", fingerprints, "gcgapi/rulings.json")
    faq = _read_optional_list(data_dir / "rules-faq.json", fingerprints, "gcgapi/rules-faq.json")
    errata = _read_optional_list(data_dir / "errata.json", fingerprints, "gcgapi/errata.json")
    manifest = (
        _read_optional_object(data_dir / "manifest.json", fingerprints, "gcgapi/manifest.json")
        or {}
    )
    if not rules_path.exists():
        raise SourceError(f"missing rules file {rules_path}")
    fingerprints[f"rules/{rules_path.name}"] = _sha256(rules_path)
    rules = parse_rules(rules_path.read_text(encoding="utf-8"))
    official_rules_version = _read_optional_object(
        official_dir / "rules_version.json", fingerprints, "official/rules_version.json"
    )
    banlist = _read_optional_object(
        official_dir / "banlist.json", fingerprints, "official/banlist.json"
    )
    curated = _read_optional_object(curated_path, fingerprints, "curated_conflicts.json") or {
        "schema_version": SCHEMA_VERSION,
        "conflicts": [],
    }
    overrides = _read_optional_object(overrides_path, fingerprints, "overrides.json") or {
        "schema_version": SCHEMA_VERSION,
        "policies": {},
        "resolutions": {},
    }
    return Inputs(
        cards=cards,
        rulings=rulings,
        faq=faq,
        errata=errata,
        manifest=manifest,
        rules=rules,
        official_rules_version=official_rules_version,
        banlist=banlist,
        curated=curated,
        overrides=overrides,
        fingerprints=dict(sorted(fingerprints.items())),
    )


# ---------------------------------------------------------------------------------------------
# Text helpers


def normalize_text(text: str) -> str:
    """Normalize for quote matching: NFKC, straight quotes, no markdown escapes, single spaces."""
    text = unicodedata.normalize("NFKC", text).translate(MARKDOWN_ESCAPES)
    text = text.replace("’", "'").replace("‘", "'").replace("“", '"').replace("”", '"')
    return " ".join(text.split())


def _cosmetic_key(field_name: str, value: Any) -> str:
    """Key that is equal for two values differing only in case, spacing or quote style."""
    if isinstance(value, str):
        folded = "".join(normalize_text(value).split()).casefold()
        if field_name in RAW_STAT_FIELDS:
            folded = folded.lstrip("+")
        return folded
    return json.dumps(value, sort_keys=True, ensure_ascii=False)


def strip_reminders(text: str) -> str:
    """Drop parenthesized reminder text (rule 2-11-4); trait references like (Zeon) stay."""
    previous = None
    while previous != text:
        previous = text
        text = _REMINDER.sub(
            lambda m: m.group(0) if not _LOWERCASE_WORD.search(m.group(1)) else " ", text
        )
    return text


def _printing_sort_key(product_id: str) -> tuple[str, int]:
    match = _SUFFIX_NUMBER.search(product_id)
    if match is None:
        return (product_id, -1)
    return (product_id[: match.start()], int(match.group(1)))


def _card_sort_key(card_number: str) -> tuple[str, int, str]:
    prefix, _, number = card_number.rpartition("-")
    return (prefix, int(number) if number.isdigit() else 0, card_number)


def _id_sort_key(conflict: Conflict) -> tuple[int, list[tuple[int, int | str]]]:
    parts: list[tuple[int, int | str]] = [
        (0, int(tok)) if tok.isdigit() else (1, tok) for tok in re.split(r"(\d+)", conflict.id)
    ]
    kind_rank = KIND_ORDER.index(conflict.kind) if conflict.kind in KIND_ORDER else len(KIND_ORDER)
    return (kind_rank, parts)


def is_edition_beta(card: Mapping[str, Any]) -> bool:
    return str(card.get("where_to_get") or "").startswith(EDITION_BETA_PREFIX)


def derive_keyword_effects(effect: str) -> list[JsonDict]:
    """Same extraction gcg-api uses for ``keyword_effects``."""
    out: list[JsonDict] = []
    seen: set[tuple[str, int | None]] = set()
    for match in _KEYWORD_TAG.finditer(effect):
        keyword = match.group(1).strip()
        value = int(match.group(2)) if match.group(2) is not None else None
        key = (keyword.lower(), value)
        if key not in seen:
            seen.add(key)
            out.append({"keyword": keyword, "value": value})
    return out


def derive_timing_markers(effect: str) -> list[str]:
    """Same extraction gcg-api uses for ``timing_markers``."""
    vocabulary = {t.lower() for t in TIMING_VOCABULARY}
    out: list[str] = []
    seen: set[str] = set()
    for match in _TIMING_TAG.finditer(effect):
        for token in _TIMING_SPLIT.split(match.group(1)):
            token = token.strip()
            key = token.lower()
            if key in vocabulary and key not in seen:
                seen.add(key)
                out.append(token)
    return out


def derive_keywords_text(
    keyword_effects: Sequence[Mapping[str, Any]], markers: Sequence[str]
) -> str | None:
    joined = " ".join([str(k["keyword"]) for k in keyword_effects] + list(markers)).lower()
    return joined or None


def _group_by_number(cards: Iterable[JsonDict]) -> dict[str, list[JsonDict]]:
    grouped: dict[str, list[JsonDict]] = defaultdict(list)
    for card in cards:
        grouped[str(card["card_number"])].append(card)
    for printings in grouped.values():
        printings.sort(key=lambda c: _printing_sort_key(str(c.get("product_id", ""))))
    return dict(sorted(grouped.items(), key=lambda item: _card_sort_key(item[0])))


def _sorted_numbers(numbers: Iterable[str]) -> list[str]:
    return sorted(set(numbers), key=_card_sort_key)


def _card_source(card: Mapping[str, Any]) -> str:
    url = card.get("detail_url") or ""
    return f"gcg-api card {card.get('product_id')}" + (f" — {url}" if url else "")


# ---------------------------------------------------------------------------------------------
# (a) Divergent printings


def suggest_canonical(printings: Sequence[Mapping[str, Any]]) -> str:
    """Base printing if present; else the most common non-Edition-Beta printing, newest first."""
    for card in printings:
        if card.get("product_id") == card.get("card_number"):
            return str(card["product_id"])
    pool = [c for c in printings if not is_edition_beta(c)] or list(printings)

    def signature(card: Mapping[str, Any]) -> str:
        return json.dumps({f: card.get(f) for f in GAMEPLAY_FIELDS}, sort_keys=True)

    counts = Counter(signature(c) for c in printings)
    best = max(
        pool,
        key=lambda c: (counts[signature(c)], _printing_sort_key(str(c.get("product_id", "")))),
    )
    return str(best["product_id"])


def _classify(field_name: str, values: Sequence[Any]) -> str:
    if len({_cosmetic_key(field_name, v) for v in values}) == 1:
        return "cosmetic"
    if field_name == "effect" and all(isinstance(v, str) for v in values):
        stripped = {_cosmetic_key(field_name, strip_reminders(str(v))) for v in values}
        if len(stripped) == 1:
            return "reminder_only"
    if field_name in {"keyword_effects", "timing_markers", "keywords_text"}:
        return "derived"
    return "substantive"


def _comparable(field_name: str, value: Any) -> str:
    if field_name == "effect" and isinstance(value, str):
        return _cosmetic_key(field_name, strip_reminders(value))
    return _cosmetic_key(field_name, value)


def detect_divergent_printings(cards: Sequence[JsonDict]) -> list[Conflict]:
    conflicts: list[Conflict] = []
    for number, printings in _group_by_number(cards).items():
        if len(printings) < 2:
            continue
        field_diffs: list[JsonDict] = []
        for field_name in GAMEPLAY_FIELDS:
            values = [p.get(field_name) for p in printings]
            rendered = [json.dumps(v, sort_keys=True, ensure_ascii=False) for v in values]
            if len(set(rendered)) == 1:
                continue
            variants: dict[str, JsonDict] = {}
            for printing, value, key in zip(printings, values, rendered, strict=True):
                variant = variants.setdefault(key, {"value": value, "product_ids": []})
                variant["product_ids"].append(printing["product_id"])
            field_diffs.append(
                {
                    "field": field_name,
                    "classification": _classify(field_name, values),
                    "variants": list(variants.values()),
                }
            )
        if not field_diffs:
            continue
        canonical_id = suggest_canonical(printings)
        canonical = next(p for p in printings if p["product_id"] == canonical_id)
        substantive = [
            d["field"] for d in field_diffs if d["classification"] in {"substantive", "derived"}
        ]
        deviating = sorted(
            {
                str(p["product_id"])
                for p in printings
                for f in substantive
                if _comparable(f, p.get(f)) != _comparable(f, canonical.get(f))
            },
            key=_printing_sort_key,
        )
        by_id = {str(p["product_id"]): p for p in printings}
        stat_fields = [f for f in substantive if f in STAT_FIELDS]
        only_beta = bool(deviating) and all(is_edition_beta(by_id[pid]) for pid in deviating)
        if not substantive:
            severity = "info"
        elif stat_fields and not only_beta:
            severity = "major"
        else:
            severity = "minor"
        classes = sorted({d["classification"] for d in field_diffs})
        conflicts.append(
            Conflict(
                id=f"divergent:{number}",
                kind="divergent_printing",
                severity=severity,
                card_numbers=[number],
                title=f"{number} {canonical.get('name')}: printings differ in "
                + ", ".join(d["field"] for d in field_diffs),
                description=(
                    f"{len(printings)} printings of {number} disagree on "
                    f"{', '.join(d['field'] for d in field_diffs)} ({', '.join(classes)}). "
                    + (
                        f"Printings deviating from {canonical_id} in gameplay terms: "
                        f"{', '.join(deviating)}" + (" (all Edition Beta)." if only_beta else ".")
                        if deviating
                        else "The differences are cosmetic or confined to reminder text."
                    )
                ),
                sources=[_card_source(p) for p in printings],
                details={
                    "canonical_suggestion": canonical_id,
                    "deviating_printings": deviating,
                    "printings": [
                        {
                            "product_id": p["product_id"],
                            "set_code": p.get("set_code"),
                            "rarity": p.get("rarity"),
                            "where_to_get": p.get("where_to_get"),
                            "edition_beta": is_edition_beta(p),
                        }
                        for p in printings
                    ],
                    "fields": field_diffs,
                },
            )
        )
    return conflicts


# ---------------------------------------------------------------------------------------------
# (b) Errata


def detect_errata(cards: Sequence[JsonDict], errata: Sequence[JsonDict]) -> list[Conflict]:
    grouped = _group_by_number(cards)
    per_card = Counter(str(e.get("card_number")) for e in errata)
    seen_ids: Counter[str] = Counter()
    conflicts: list[Conflict] = []
    for entry in errata:
        number = str(entry.get("card_number"))
        field_name = str(entry.get("field"))
        before = str(entry.get("before") or "")
        after = str(entry.get("after") or "")
        base_id = f"{number}:{field_name}" if per_card[number] > 1 else number
        seen_ids[base_id] += 1
        suffix = f":{seen_ids[base_id]}" if seen_ids[base_id] > 1 else ""
        printings = grouped.get(number, [])
        checks: list[JsonDict] = []
        for printing in printings:
            value = printing.get(field_name)
            text = value if isinstance(value, str) else json.dumps(value, ensure_ascii=False)
            after_present = bool(after) and after in text
            before_absent = not before or before not in text.replace(after, "")
            derived_ok = True
            if field_name == "trait" and isinstance(value, str):
                derived_ok = printing.get("traits") == re.findall(r"\(([^)]+)\)", value)
            checks.append(
                {
                    "product_id": printing.get("product_id"),
                    "after_present": after_present,
                    "before_absent": before_absent,
                    "derived_fields_consistent": derived_ok,
                }
            )
        applied = bool(printings) and all(
            c["after_present"] and c["before_absent"] and c["derived_fields_consistent"]
            for c in checks
        )
        declared = entry.get("printings_affected")
        count_ok = declared is None or declared == len(printings)
        details: JsonDict = {
            "field": field_name,
            "before": before,
            "after": after,
            "date": entry.get("date"),
            "status": entry.get("status"),
            "note": entry.get("note"),
            "printings_affected_declared": declared,
            "printings_found": len(printings),
            "printings": checks,
        }
        sources = [str(entry["source_url"])] if entry.get("source_url") else []
        sources += [_card_source(p) for p in printings]
        if applied:
            conflicts.append(
                Conflict(
                    id=f"errata:{base_id}{suffix}",
                    kind="errata",
                    severity="info" if count_ok else "minor",
                    card_numbers=[number],
                    title=f"{number} {entry.get('name')}: printed {field_name} superseded by errata",
                    description=(
                        f"Printed {field_name} reads {before!r}; official errata ({entry.get('date')}) "
                        f"changes it to {after!r}. Verified applied in all {len(printings)} "
                        "printing(s) of the snapshot"
                        + (
                            "."
                            if count_ok
                            else f", but the ledger declares {declared} printing(s)."
                        )
                    ),
                    sources=sources,
                    details=details,
                )
            )
        else:
            reason = "no printing found" if not printings else "not applied to every printing"
            conflicts.append(
                Conflict(
                    id=f"errata-unapplied:{base_id}{suffix}",
                    kind="errata",
                    severity="major",
                    card_numbers=[number],
                    title=f"{number}: errata to {field_name} {reason}",
                    description=(
                        f"Errata ({entry.get('date')}) replaces {before!r} with {after!r} in "
                        f"{field_name}, but the snapshot does not reflect it ({reason})."
                    ),
                    sources=sources,
                    details=details,
                )
            )
    return conflicts


# ---------------------------------------------------------------------------------------------
# (c) Rules version


def detect_rules_version(rules: RulesDoc, official: JsonDict | None) -> list[Conflict]:
    local = f"Ver. {rules.version} (updated {rules.updated})"
    if rules.version is None or rules.updated is None:
        return [
            Conflict(
                id="rules-version:unparseable",
                kind="rules_version",
                severity="major",
                card_numbers=[],
                title="Rules file has no parseable version/date line",
                description="Expected 'Ver. X.Y.Z' and 'Updated <Mon DD, YYYY>' near the top.",
                sources=["rules file"],
            )
        ]
    if official is None:
        return [
            Conflict(
                id="rules-version:official-record-pending",
                kind="rules_version",
                severity="info",
                card_numbers=[],
                title="Official rules version check pending",
                description=(
                    f"Local rules file is {local}. The official-version record "
                    "(data/official/rules_version.json) does not exist yet, so the check is "
                    "pending the official-sources fetch."
                ),
                sources=[f"Comprehensive Rules Ver. {rules.version}"],
            )
        ]
    latest_version = official.get("latest_version")
    latest_date_raw = official.get("latest_date")
    latest_date = _parse_rules_date(str(latest_date_raw)) if latest_date_raw else None
    url = str(official.get("url") or "")
    if latest_version is None and latest_date is None:
        return [
            Conflict(
                id="rules-version:official-record-invalid",
                kind="rules_version",
                severity="major",
                card_numbers=[],
                title="Official rules-version record has neither version nor date",
                description=f"Record: {json.dumps(official, sort_keys=True)}",
                sources=[url] if url else [],
            )
        ]
    mismatches = []
    if latest_version is not None and str(latest_version) != rules.version:
        mismatches.append(f"version {latest_version} != {rules.version}")
    if latest_date is not None and latest_date != rules.updated:
        mismatches.append(f"date {latest_date} != {rules.updated}")
    if not mismatches:
        return []
    official_label = f"{latest_version or '?'}@{latest_date or '?'}"
    return [
        Conflict(
            id=f"rules-version:mismatch:{rules.version}@{rules.updated}:{official_label}",
            kind="rules_version",
            severity="major",
            card_numbers=[],
            title="Local rules file differs from the latest official rules",
            description=f"Local {local}; official site: {'; '.join(mismatches)}.",
            sources=[url] if url else [],
            details={
                "official": official,
                "local_version": rules.version,
                "local_updated": rules.updated,
            },
        )
    ]


# ---------------------------------------------------------------------------------------------
# (d) Banned/restricted list ambiguities


def detect_banlist_ambiguities(banlist: JsonDict | None) -> list[Conflict]:
    if not banlist:
        return []
    items = banlist.get("ambiguities")
    if not isinstance(items, list):
        return []
    conflicts: list[Conflict] = []
    for item in items:
        record: JsonDict = item if isinstance(item, dict) else {"description": str(item)}
        description = str(
            record.get("description") or record.get("text") or json.dumps(record, sort_keys=True)
        )
        raw_id = record.get("id")
        stable = (
            str(raw_id)
            if raw_id
            else hashlib.sha256(normalize_text(description).encode()).hexdigest()[:12]
        )
        cards = record.get("card_numbers") or record.get("cards") or []
        numbers = [str(c) for c in cards] if isinstance(cards, list) else [str(cards)]
        severity = str(record.get("severity") or "minor")
        raw_sources = record.get("sources") or record.get("source") or record.get("url") or []
        sources = (
            [str(s) for s in raw_sources] if isinstance(raw_sources, list) else [str(raw_sources)]
        )
        conflicts.append(
            Conflict(
                id=f"banlist:{stable}",
                kind="banlist_ambiguity",
                severity=severity if severity in SEVERITIES else "minor",
                card_numbers=_sorted_numbers(numbers),
                title=str(record.get("title") or description[:80]),
                description=description,
                sources=sources or ["official banned/restricted list"],
                details={"record": record},
            )
        )
    return conflicts


# ---------------------------------------------------------------------------------------------
# (e) Keyword / timing markers vs effect text


def detect_marker_mismatches(cards: Sequence[JsonDict]) -> list[Conflict]:
    conflicts: list[Conflict] = []
    pilot_marker: list[str] = []
    development: list[str] = []
    qualified: list[str] = []
    activate_split: list[str] = []
    mention_only: list[str] = []
    for number, printings in _group_by_number(cards).items():
        mismatched: dict[str, list[JsonDict]] = defaultdict(list)
        bare: dict[str, list[str]] = {}
        colon: dict[str, list[str]] = {}
        for card in printings:
            effect = str(card.get("effect") or "")
            keywords = derive_keyword_effects(effect)
            markers = derive_timing_markers(effect)
            expected = {
                "keyword_effects": keywords,
                "timing_markers": markers,
                "keywords_text": derive_keywords_text(keywords, markers),
            }
            for field_name, value in expected.items():
                if card.get(field_name) != value:
                    mismatched[field_name].append(
                        {
                            "product_id": card.get("product_id"),
                            "stored": card.get(field_name),
                            "derived_from_effect": value,
                        }
                    )
            stripped = _KEYWORD_TAG.sub(" ", effect)
            words = sorted({m.group(1) for m in _BARE_KEYWORD.finditer(stripped)})
            if words:
                bare[str(card["product_id"])] = words
            lines = [
                line
                for line in effect.splitlines()
                if "【Activate" in line and "：" not in line and _ASCII_COST_COLON.search(line)
            ]
            if lines:
                colon[str(card["product_id"])] = lines
        for field_name, rows in sorted(mismatched.items()):
            conflicts.append(
                Conflict(
                    id=f"markers:{number}:{field_name}",
                    kind="marker_mismatch",
                    severity="minor",
                    card_numbers=[number],
                    title=f"{number}: stored {field_name} disagrees with the effect text",
                    description=(
                        f"{field_name} is derived from the effect text by gcg-api, but "
                        f"{len(rows)} printing(s) carry a value that the text does not produce."
                    ),
                    sources=[_card_source(p) for p in printings],
                    details={"printings": rows},
                )
            )
        if bare:
            names = sorted({w for words in bare.values() for w in words})
            conflicts.append(
                Conflict(
                    id=f"markers:{number}:unbracketed-keyword",
                    kind="marker_mismatch",
                    severity="major",
                    card_numbers=[number],
                    title=f"{number}: keyword {', '.join(names)} written without <> brackets",
                    description=(
                        f"The effect text names keyword effect(s) {', '.join(names)} outside the "
                        "<...> form, so keyword_effects omits them and a template compiler "
                        "would miss the keyword."
                    ),
                    sources=[_card_source(p) for p in printings],
                    details={"printings": bare},
                )
            )
        if colon:
            conflicts.append(
                Conflict(
                    id=f"markers:{number}:ascii-colon",
                    kind="marker_mismatch",
                    severity="minor",
                    card_numbers=[number],
                    title=f"{number}: activated-effect cost separated by ASCII ':'",
                    description=(
                        "Activated effects separate cost and effect with the full-width '：' "
                        "(rule 10-1-7-2); this text uses ':' so a parser may treat the whole "
                        "line as a cost-free effect (rule 10-1-7-5)."
                    ),
                    sources=[_card_source(p) for p in printings],
                    details={"printings": colon},
                )
            )
        effect = str(printings[0].get("effect") or "")
        tokens = [
            tok.strip()
            for m in _TIMING_TAG.finditer(effect)
            for tok in _TIMING_SPLIT.split(m.group(1))
        ]
        bracket_bodies = [m.group(1) for m in _TIMING_TAG.finditer(effect)]
        if "Pilot" in tokens:
            pilot_marker.append(number)
        if any(tok.startswith("Development") for tok in tokens):
            development.append(number)
        if any(
            re.match(r"(When Paired|During Pair)[・･]", body) and "Development" not in body
            for body in bracket_bodies
        ):
            qualified.append(number)
        if any(re.match(r"Activate[・･](Main|Action)$", body) for body in bracket_bodies):
            activate_split.append(number)
        own_lines = [line.strip() for line in effect.splitlines()]
        for keyword in printings[0].get("keyword_effects") or []:
            value = keyword.get("value")
            tag = f"<{keyword.get('keyword')}" + (f" {value}>" if value is not None else ">")
            if not any(line.startswith(tag) for line in own_lines):
                mention_only.append(number)
                break

    def aggregate(cid: str, numbers: list[str], title: str, description: str) -> None:
        if numbers:
            conflicts.append(
                Conflict(
                    id=cid,
                    kind="marker_mismatch",
                    severity="info",
                    card_numbers=_sorted_numbers(numbers),
                    title=title,
                    description=description,
                    sources=[
                        "gcg-api src/normalize.js (extractKeywordEffects/extractTimingMarkers)"
                    ],
                    details={"count": len(set(numbers))},
                )
            )

    aggregate(
        "markers-vocab:pilot",
        pilot_marker,
        "【Pilot】 sections are not recorded in timing_markers",
        "Command cards with a 【Pilot】 effect (rule 3-4-6) are indistinguishable from plain "
        "Commands through timing_markers; the Pilot name and AP/HP modifiers live only in the text "
        "and the ap/hp fields.",
    )
    aggregate(
        "markers-vocab:development",
        development,
        "<Development> (rule 13-1-8) is not recorded in keyword_effects or timing_markers",
        "Development is written inside the timing bracket (【Deploy・Development 2】); the extractor "
        "keeps 'Deploy'/'When Linked'/'When Paired' and drops 'Development N'.",
    )
    aggregate(
        "markers-vocab:qualified-timing",
        qualified,
        "Pilot qualifications in 【When Paired･…】/【During Pair･…】 are dropped from timing_markers",
        "timing_markers keeps only 'When Paired'/'During Pair'; the qualification (trait, colour or "
        "Lv. of the Pilot, rules 13-2-9-2 and 13-2-10-2) is only in the effect text.",
    )
    aggregate(
        "markers-vocab:activate-split",
        activate_split,
        "【Activate･Main】/【Activate･Action】 are recorded as separate 'Activate' + 'Main'/'Action'",
        "timing_markers cannot distinguish a 【Main】 Command from an 【Activate･Main】 ability on the "
        "same card; engines must read the bracket from the effect text.",
    )
    aggregate(
        "markers-semantics:keyword-mentions",
        mention_only,
        "keyword_effects lists keywords the card only mentions",
        "keyword_effects is every <Keyword> mentioned anywhere in the text: conditional grants "
        "('While …, this Unit gains <Blocker>'), grants to other Units, target filters ('Unit with "
        "<Blocker>') and reminder text. For these cards at least one listed keyword is not an "
        "unconditional printed keyword of the card itself.",
    )
    return conflicts


# ---------------------------------------------------------------------------------------------
# Card data vs rules (null stats the rules require)


def detect_data_vs_rules(cards: Sequence[JsonDict]) -> list[Conflict]:
    required: dict[str, tuple[str, ...]] = {
        "UNIT": ("level", "cost", "ap", "hp", "color"),
        "PILOT": ("level", "cost", "ap", "hp", "color"),
        "COMMAND": ("level", "cost", "color"),
        "BASE": ("level", "cost", "ap", "hp", "color"),
        "EX BASE": ("ap", "hp"),
    }
    rule_refs = {
        "UNIT": "3-2-5 (Units have AP and HP), 2-4-2, 2-9-2, 2-10-2",
        "PILOT": "3-3-8 (Pilots have AP and HP modifiers), 2-4-2, 2-9-2, 2-10-2",
        "COMMAND": "2-4-2, 2-9-2, 2-10-2",
        "BASE": "3-5-4 (Bases have AP and HP), 2-4-2, 2-9-2, 2-10-2",
        "EX BASE": "5-17-3-1-1 (an EX Base is a Base token with 0 AP and 3 HP)",
    }
    missing: dict[tuple[str, str], list[str]] = defaultdict(list)
    for card in cards:
        card_type = str(card.get("card_type"))
        for field_name in required.get(card_type, ()):
            if card.get(field_name) is None:
                missing[(card_type, field_name)].append(str(card["product_id"]))
        if card_type == "COMMAND" and "【Pilot】" in str(card.get("effect") or ""):
            for field_name in ("ap", "hp"):
                if card.get(field_name) is None:
                    missing[("COMMAND-PILOT", field_name)].append(str(card["product_id"]))
    conflicts: list[Conflict] = []
    for (card_type, field_name), product_ids in sorted(missing.items()):
        numbers = _sorted_numbers(pid.split("_p")[0] for pid in product_ids)
        conflicts.append(
            Conflict(
                id=f"data-null:{card_type.replace(' ', '-')}:{field_name}",
                kind="data_vs_rules",
                severity="info"
                if card_type in {"BASE", "EX BASE"} and field_name == "ap"
                else "minor",
                card_numbers=numbers,
                title=f"{card_type} records with null {field_name}",
                description=(
                    f"{len(product_ids)} {card_type} printing(s) have no {field_name} value "
                    f"although the rules give every such card one (rule {rule_refs.get(card_type, '')})."
                ),
                sources=["gcg-api cards.ndjson"],
                details={"product_ids": sorted(product_ids, key=_printing_sort_key)},
            )
        )
    return conflicts


# ---------------------------------------------------------------------------------------------
# (f) Rules cross-references


def _title_key(title: str) -> str:
    text = normalize_text(title).casefold()
    text = re.sub(r"[【】<>()\[\]]", " ", text)
    return " ".join(text.split())


def _titles_match(reference: str, heading: str) -> bool:
    ref, head = _title_key(reference), _title_key(heading)
    return ref == head or ref.rstrip("s") == head.rstrip("s")


def _suggest_xref(rules: RulesDoc, target: str, title: str | None, context: str) -> str | None:
    if title:
        exact = [
            rid for rid, head in rules.headings.items() if _title_key(title) == _title_key(head)
        ]
        loose = [rid for rid, head in rules.headings.items() if _titles_match(title, head)]
        for matches in (exact, loose):
            if len(matches) == 1:
                return matches[0]
        return None
    suffix = target.split("-", 1)[1] if "-" in target else None
    if suffix is None:
        return None
    candidates = [rid for rid in rules.headings if rid.endswith("-" + suffix) and rid != target]

    def words(text: str) -> set[str]:
        return {w.rstrip("s") for w in _WORD.findall(text.casefold()) if len(w) > 2}

    context_words = words(context)
    scored = sorted((len(context_words & words(rules.headings[rid])), rid) for rid in candidates)
    if scored and scored[-1][0] > 0 and (len(scored) == 1 or scored[-2][0] < scored[-1][0]):
        return scored[-1][1]
    return None


def detect_rules_xrefs(rules: RulesDoc) -> list[Conflict]:
    known = set(rules.headings) | set(rules.paragraphs)
    found: dict[str, list[JsonDict]] = defaultdict(list)
    for citing in rules.order:
        text = rules.paragraphs.get(citing)
        if text is None:
            continue
        for match in _XREF_PAREN.finditer(text):
            target = match.group(1)
            title = (match.group(2) or "").strip() or None
            heading = rules.headings.get(target)
            if target not in known:
                reason = f"rule {target} does not exist"
            elif title is not None and heading is None:
                reason = f"{target} is a numbered paragraph, not the section titled {title!r}"
            elif title is not None and heading is not None and not _titles_match(title, heading):
                reason = f"section {target} is titled {heading!r}, not {title!r}"
            else:
                continue
            suggestion = _suggest_xref(rules, target, title, text[: match.start()][-80:])
            found[citing].append(
                {
                    "citing_rule": citing,
                    "reference": match.group(0).translate(MARKDOWN_ESCAPES),
                    "target": target,
                    "reference_title": title,
                    "actual_title_at_target": heading,
                    "reason": reason,
                    "suggested_target": suggestion,
                    "suggested_title": rules.headings.get(suggestion) if suggestion else None,
                }
            )
    conflicts: list[Conflict] = []
    for citing, refs in found.items():
        for ref in refs:
            cid = (
                f"rules-xref:{citing}" if len(refs) == 1 else f"rules-xref:{citing}:{ref['target']}"
            )
            suggestion = ref["suggested_target"]
            conflicts.append(
                Conflict(
                    id=cid,
                    kind="rules_xref",
                    severity="minor",
                    card_numbers=[],
                    title=f"Rule {citing} cites {ref['reference']}",
                    description=(
                        f"Rule {citing} refers to {ref['reference']}, but {ref['reason']}."
                        + (
                            f" Intended target: {suggestion} ({ref['suggested_title']})."
                            if suggestion
                            else " No unambiguous intended target found."
                        )
                    ),
                    sources=[f"Comprehensive Rules Ver. {rules.version} §{citing}"],
                    details=ref,
                )
            )
    return conflicts


# ---------------------------------------------------------------------------------------------
# Curated conflicts


class _Evidence:
    def __init__(self, inputs: Inputs) -> None:
        self.rules = inputs.rules
        self.cards = _group_by_number(inputs.cards)
        self.rulings = {f"{r.get('card_number')}:{r.get('num')}": r for r in inputs.rulings}
        self.faq = {str(f.get("num")): f for f in inputs.faq}
        self.errata = {str(e.get("card_number")): e for e in inputs.errata}

    def check(self, item: Mapping[str, Any]) -> tuple[str | None, str | None]:
        """Return (source label, problem) for one evidence item."""
        source = str(item.get("source"))
        ref = str(item.get("ref"))
        quote = normalize_text(str(item.get("quote") or ""))
        if not quote:
            return None, f"evidence {source}:{ref} has no quote"
        if source == "rules":
            text = self.rules.text_of(ref)
            label = f"Comprehensive Rules Ver. {self.rules.version} §{ref}"
            if text is None:
                return label, f"rule {ref} not found"
            return label, None if quote in normalize_text(text) else f"quote not in rule {ref}"
        if source == "ruling":
            ruling = self.rulings.get(ref)
            if ruling is None:
                return None, f"ruling {ref} not found"
            label = f"Card ruling {ref} — {ruling.get('source_url', '')}".rstrip(" —")
            haystack = normalize_text(f"{ruling.get('question', '')} {ruling.get('answer', '')}")
            return label, None if quote in haystack else f"quote not in ruling {ref}"
        if source == "faq":
            entry = self.faq.get(ref)
            if entry is None:
                return None, f"FAQ {ref} not found"
            label = f"Rules FAQ {ref} — {entry.get('source_url', '')}".rstrip(" —")
            haystack = normalize_text(f"{entry.get('question', '')} {entry.get('answer', '')}")
            return label, None if quote in haystack else f"quote not in FAQ {ref}"
        if source == "card":
            printings = self.cards.get(ref)
            if not printings:
                return None, f"card {ref} not found"
            field_name = str(item.get("field") or "effect")
            label = _card_source(printings[0]) + f" ({field_name})"
            values = [normalize_text(str(p.get(field_name) or "")) for p in printings]
            ok = any(quote in value for value in values)
            return label, None if ok else f"quote not in {field_name} of {ref}"
        if source == "errata":
            entry = self.errata.get(ref)
            if entry is None:
                return None, f"errata for {ref} not found"
            label = f"Errata {ref} — {entry.get('source_url', '')}".rstrip(" —")
            haystack = normalize_text(json.dumps(entry, ensure_ascii=False))
            return label, None if quote in haystack else f"quote not in errata {ref}"
        return None, f"unknown evidence source {source!r}"


def load_curated(inputs: Inputs) -> tuple[list[Conflict], list[str]]:
    """Curated entries with verified evidence; returns (conflicts, structural errors)."""
    errors: list[str] = []
    entries = inputs.curated.get("conflicts")
    if not isinstance(entries, list):
        return [], ["curated_conflicts.json: 'conflicts' must be a list"]
    evidence = _Evidence(inputs)
    conflicts: list[Conflict] = []
    seen: set[str] = set()
    for index, entry in enumerate(entries):
        where = f"curated_conflicts.json[{index}]"
        if not isinstance(entry, dict):
            errors.append(f"{where}: not an object")
            continue
        cid = entry.get("id")
        if not isinstance(cid, str) or not cid:
            errors.append(f"{where}: missing id")
            continue
        if cid in seen:
            errors.append(f"{where}: duplicate id {cid}")
            continue
        seen.add(cid)
        kind = entry.get("kind")
        severity = entry.get("severity")
        if kind not in CURATED_KINDS:
            errors.append(f"{cid}: unknown kind {kind!r}")
            continue
        if severity not in SEVERITIES:
            errors.append(f"{cid}: unknown severity {severity!r}")
            continue
        items = entry.get("evidence")
        if not isinstance(items, list) or not items:
            errors.append(f"{cid}: needs at least one evidence item")
            continue
        numbers = [str(n) for n in entry.get("card_numbers") or []]
        problems = [f"card {n} not in snapshot" for n in numbers if n not in evidence.cards]
        sources: list[str] = []
        checked: list[JsonDict] = []
        for item in items:
            if not isinstance(item, dict):
                problems.append("evidence item is not an object")
                continue
            label, problem = evidence.check(item)
            if label and label not in sources:
                sources.append(label)
            if problem:
                problems.append(problem)
            checked.append({**item, "verified": problem is None})
        conflicts.append(
            Conflict(
                id=cid,
                kind=str(kind),
                severity=str(severity),
                card_numbers=_sorted_numbers(numbers),
                title=str(entry.get("title") or cid),
                description=str(entry.get("description") or ""),
                sources=sources,
                details={"evidence": checked},
                origin="curated",
                engine_behaviour=bool(entry.get("engine_behaviour", False)),
                problems=problems,
            )
        )
    return conflicts, errors


def review_fingerprint(question: str, answer: str) -> str:
    """Short hash of a ruling/FAQ entry; a changed answer invalidates an earlier review."""
    return hashlib.sha256(normalize_text(f"{question}\n{answer}").encode()).hexdigest()[:12]


def detect_unreviewed(
    inputs: Inputs, curated: Sequence[Conflict]
) -> tuple[list[Conflict], list[str]]:
    """Rulings/FAQ entries neither cited by a curated entry nor recorded as reviewed.

    Returns the conflicts plus warnings for review records that no longer match any entry.
    """
    cited: dict[str, set[str]] = {"ruling": set(), "faq": set()}
    for conflict in curated:
        for item in conflict.details.get("evidence", []):
            if item.get("source") in cited:
                cited[str(item["source"])].add(str(item.get("ref")))
    reviewed_raw = inputs.curated.get("reviewed_without_conflict") or {}
    reviewed: dict[str, dict[str, str]] = {
        "ruling": dict(reviewed_raw.get("rulings") or {}),
        "faq": dict(reviewed_raw.get("faq") or {}),
    }
    entries = {
        "ruling": {f"{r.get('card_number')}:{r.get('num')}": r for r in inputs.rulings},
        "faq": {str(f.get("num")): f for f in inputs.faq},
    }
    conflicts: list[Conflict] = []
    warnings: list[str] = []
    for source, by_ref in entries.items():
        for ref, entry in by_ref.items():
            if ref in cited[source]:
                continue
            fingerprint = review_fingerprint(str(entry.get("question")), str(entry.get("answer")))
            recorded = reviewed[source].get(ref)
            if recorded == fingerprint:
                continue
            state = "changed since it was reviewed" if recorded else "not reviewed yet"
            label = "Card ruling" if source == "ruling" else "Rules FAQ"
            conflicts.append(
                Conflict(
                    id=f"unreviewed:{source}:{ref}",
                    kind="ruling_vs_text" if source == "ruling" else "faq_vs_rules",
                    severity="minor",
                    card_numbers=[str(entry["card_number"])] if source == "ruling" else [],
                    title=f"{label} {ref} is {state}",
                    description=(
                        f"“{entry.get('question')}” — “{entry.get('answer')}” Compare it with the "
                        "card text/rules; add a curated entry if it changes behaviour, otherwise "
                        f"record fingerprint {fingerprint} under reviewed_without_conflict."
                    ),
                    sources=[f"{label} {ref} — {entry.get('source_url', '')}".rstrip(" —")],
                    details={"fingerprint": fingerprint, "recorded_fingerprint": recorded},
                )
            )
        for ref in sorted(set(reviewed[source]) - set(by_ref)):
            warnings.append(f"reviewed:{source}:{ref}")
    return conflicts, warnings


def merge_conflicts(detected: Sequence[Conflict], curated: Sequence[Conflict]) -> list[Conflict]:
    """Curated entries refine detected ones with the same id; the rest are appended."""
    merged: dict[str, Conflict] = {}
    for conflict in detected:
        if conflict.id in merged:
            raise ValueError(f"detector produced duplicate id {conflict.id}")
        merged[conflict.id] = conflict
    for entry in curated:
        base = merged.get(entry.id)
        if base is None:
            merged[entry.id] = entry
            continue
        merged[entry.id] = Conflict(
            id=base.id,
            kind=base.kind,
            severity=entry.severity,
            card_numbers=_sorted_numbers([*base.card_numbers, *entry.card_numbers]),
            title=entry.title,
            description=entry.description,
            sources=list(dict.fromkeys([*base.sources, *entry.sources])),
            details={**base.details, "evidence": entry.details.get("evidence", [])},
            origin="detected+curated",
            engine_behaviour=entry.engine_behaviour,
            problems=entry.problems,
        )
    return sorted(merged.values(), key=_id_sort_key)


# ---------------------------------------------------------------------------------------------
# Resolutions


RESOLUTION_KEYS: Final = frozenset(
    {"policy", "decision", "rationale", "canonical_product_id", "field_overrides", "card_numbers"}
)


def _valid_override_value(field_name: str, value: Any) -> bool:
    if field_name in INT_FIELDS:
        return value is None or (isinstance(value, int) and not isinstance(value, bool))
    if field_name in STR_FIELDS:
        return value is None or isinstance(value, str)
    if field_name in STR_LIST_FIELDS:
        return isinstance(value, list) and all(isinstance(v, str) for v in value)
    if field_name == "keyword_effects":
        return isinstance(value, list) and all(
            isinstance(v, dict)
            and set(v) == {"keyword", "value"}
            and isinstance(v["keyword"], str)
            and (v["value"] is None or isinstance(v["value"], int))
            for v in value
        )
    return False


def validate_resolution(
    resolution: Any,
    conflict: Conflict,
    policies: Mapping[str, Any],
    printings: Mapping[str, list[JsonDict]],
) -> list[str]:
    cid = conflict.id
    if not isinstance(resolution, dict):
        return [f"{cid}: resolution must be an object"]
    problems = [f"{cid}: unknown key {key!r}" for key in sorted(set(resolution) - RESOLUTION_KEYS)]
    policy = resolution.get("policy")
    if policy not in policies:
        problems.append(f"{cid}: unknown policy {policy!r}")
    for key in ("decision", "rationale"):
        if not isinstance(resolution.get(key), str) or not resolution[key].strip():
            problems.append(f"{cid}: {key} must be a non-empty string")
    canonical = resolution.get("canonical_product_id")
    if canonical is not None:
        valid_ids = {
            str(p.get("product_id"))
            for number in conflict.card_numbers
            for p in printings.get(number, [])
        }
        if len(conflict.card_numbers) != 1 or canonical not in valid_ids:
            problems.append(
                f"{cid}: canonical_product_id {canonical!r} is not a printing of {conflict.card_numbers}"
            )
    overrides = resolution.get("field_overrides")
    if overrides is not None:
        if not isinstance(overrides, dict) or not overrides:
            problems.append(f"{cid}: field_overrides must be a non-empty object")
        else:
            for field_name, value in overrides.items():
                if field_name not in GAMEPLAY_FIELDS:
                    problems.append(f"{cid}: field_overrides names unknown field {field_name!r}")
                elif not _valid_override_value(field_name, value):
                    problems.append(f"{cid}: field_overrides[{field_name!r}] has the wrong type")
        listed = resolution.get("card_numbers")
        if (
            not isinstance(listed, list)
            or _sorted_numbers(str(n) for n in listed) != conflict.card_numbers
        ):
            problems.append(
                f"{cid}: card_numbers must list exactly {conflict.card_numbers} when field_overrides is set"
            )
    elif "card_numbers" in resolution:
        problems.append(f"{cid}: card_numbers is only used together with field_overrides")
    return problems


def build_report(inputs: Inputs) -> Report:
    detected = [
        *detect_divergent_printings(inputs.cards),
        *detect_errata(inputs.cards, inputs.errata),
        *detect_rules_version(inputs.rules, inputs.official_rules_version),
        *detect_banlist_ambiguities(inputs.banlist),
        *detect_marker_mismatches(inputs.cards),
        *detect_data_vs_rules(inputs.cards),
        *detect_rules_xrefs(inputs.rules),
    ]
    curated, invalid = load_curated(inputs)
    unreviewed, stale_reviews = detect_unreviewed(inputs, curated)
    conflicts = merge_conflicts([*detected, *unreviewed], curated)
    policies_raw = inputs.overrides.get("policies")
    resolutions_raw = inputs.overrides.get("resolutions")
    policies: JsonDict = policies_raw if isinstance(policies_raw, dict) else {}
    resolutions: dict[str, JsonDict] = resolutions_raw if isinstance(resolutions_raw, dict) else {}
    if inputs.overrides.get("schema_version") != SCHEMA_VERSION:
        invalid.append(f"overrides.json: schema_version must be {SCHEMA_VERSION}")
    for name, policy in sorted(policies.items()):
        if not isinstance(policy, dict) or not policy.get("summary") or not policy.get("rationale"):
            invalid.append(f"overrides.json policy {name!r} needs 'summary' and 'rationale'")
    printings = _group_by_number(inputs.cards)
    unresolved: list[str] = []
    for conflict in conflicts:
        if conflict.id not in resolutions:
            unresolved.append(conflict.id)
            continue
        invalid.extend(validate_resolution(resolutions[conflict.id], conflict, policies, printings))
    ids = {c.id for c in conflicts}
    orphaned = sorted(set(resolutions) - ids) + stale_reviews
    stale = [c.id for c in conflicts if c.problems]
    counts: dict[str, dict[str, int]] = {}
    for conflict in conflicts:
        row = counts.setdefault(conflict.kind, dict.fromkeys(SEVERITIES, 0))
        row[conflict.severity] += 1
    inputs_summary: JsonDict = {
        "dataset_version": inputs.manifest.get("dataset_version"),
        "dataset_built_at": inputs.manifest.get("built_at"),
        "rules_version": inputs.rules.version,
        "rules_updated": inputs.rules.updated,
        "official_rules_version": (
            {
                key: inputs.official_rules_version.get(key)
                for key in ("latest_version", "latest_date", "url")
            }
            if inputs.official_rules_version is not None
            else None
        ),
        "sha256": inputs.fingerprints,
        "counts": {
            "printings": len(inputs.cards),
            "card_numbers": len(printings),
            "rulings": len(inputs.rulings),
            "rules_faq": len(inputs.faq),
            "errata": len(inputs.errata),
        },
        "conflicts_by_kind": {k: counts[k] for k in KIND_ORDER if k in counts},
    }
    return Report(
        conflicts=conflicts,
        policies=dict(sorted(policies.items())),
        resolutions=resolutions,
        unresolved=unresolved,
        invalid=invalid,
        stale=stale,
        orphaned=orphaned,
        inputs=inputs_summary,
    )


# ---------------------------------------------------------------------------------------------
# Rendering


def render_json(report: Report) -> str:
    conflicts = []
    for conflict in report.conflicts:
        entry = conflict.to_json()
        entry["resolution"] = report.resolutions.get(conflict.id)
        conflicts.append(entry)
    payload: JsonDict = {
        "schema_version": SCHEMA_VERSION,
        "generated_by": "python -m gcg_sim.sources.conflicts",
        "inputs": report.inputs,
        "summary": {
            "total": len(report.conflicts),
            "by_severity": {s: sum(c.severity == s for c in report.conflicts) for s in SEVERITIES},
            "unresolved": report.unresolved,
            "invalid": report.invalid,
            "stale_curated": report.stale,
            "orphaned_resolutions": report.orphaned,
        },
        "policies": report.policies,
        "conflicts": conflicts,
    }
    return json.dumps(payload, indent=2, ensure_ascii=False) + "\n"


MARKDOWN_HEADER: Final = """\
# Source conflicts

> Generated by `uv run python -m gcg_sim.sources.conflicts` from the cached snapshots, the
> comprehensive rules, `src/gcg_sim/data/curated_conflicts.json` and
> `src/gcg_sim/data/overrides.json`. Do not edit by hand: edit the curated list or the
> overrides and regenerate. `uv run python -m gcg_sim.sources.conflicts --check` fails when
> this file or `data/conflicts.json` is stale, when a conflict has no valid resolution, or when
> a curated entry's quoted evidence no longer matches its source.

## How conflicts are resolved

The simulator never edits cached source data. Every conflict below has exactly one resolution
in `overrides.json`, naming a policy, the decision and its rationale. Resolutions may pin a
`canonical_product_id` (the printing whose gameplay fields every printing of that card number
uses) and `field_overrides` (field → value, applied to every printing of the listed
`card_numbers` when the card database loads). Order of authority, highest first:

1. **Official errata** — applied from the gcg-api snapshot, or through an `errata_supersedes_print` resolution's `field_overrides` when gcg-api lags the official notice (the `errata-unapplied:*` entries below); verified each run.
2. **Official card rulings and rules-FAQ answers** — they override literal card text and fill
   gaps in the comprehensive rules; the engine implements the ruled behaviour and a test tagged
   `@pytest.mark.ruling` / `@pytest.mark.faq` pins it.
3. **Card text** over the comprehensive rules (rule 1-3-1), using the canonical printing's
   wording: when printings differ, the base printing on the official card list (which carries
   Bandai's current wording) wins; Edition Beta and promotional printings with older wording
   or stats are superseded. Reminder text in parentheses never changes behaviour (rule 2-11-4).
4. **Comprehensive rules**, reading obvious typos and wrong cross-references as intended.
5. **Most defensible reading** for genuinely ambiguous text, explained in the resolution.

Edition Beta cards' tournament legality is governed by the official tournament rules, not by
this report; here they only matter as divergent printings of a card number.

Re-running after a data refresh: new conflicts appear without a resolution and `--check`
fails until a reviewer adds one; a card ruling or rules-FAQ entry that is new, or whose answer
changed, appears as `unreviewed:*` until it is cited by a curated entry or recorded (with its
fingerprint) under `reviewed_without_conflict` in the curated file; curated entries whose
quotes disappear from the sources are flagged stale and must be re-reviewed; resolutions and
review records that no longer match anything are listed as orphaned so they can be deleted.
"""


def _md_escape(text: str) -> str:
    return text.replace("|", "\\|").replace("\n", " ")


def _md_value(value: Any) -> str:
    rendered = json.dumps(value, ensure_ascii=False)
    return "`" + rendered.replace("`", "'") + "`"


def _render_details(conflict: Conflict) -> list[str]:
    lines: list[str] = []
    details = conflict.details
    if conflict.kind == "divergent_printing":
        lines.append(f"- Suggested canonical printing: `{details['canonical_suggestion']}`")
        lines.append("- Field differences:")
        for diff in details["fields"]:
            lines.append(f"  - `{diff['field']}` ({diff['classification']}):")
            for variant in diff["variants"]:
                ids = ", ".join(f"`{pid}`" for pid in variant["product_ids"])
                lines.append(f"    - {ids}: {_md_value(variant['value'])}")
    elif conflict.kind == "errata" and "field" in details:
        lines.append(
            f"- Field `{details['field']}`: {_md_value(details['before'])} → {_md_value(details['after'])}"
        )
        for row in details["printings"]:
            status = "ok" if row["after_present"] and row["before_absent"] else "NOT APPLIED"
            lines.append(f"  - `{row['product_id']}`: {status}")
    elif conflict.kind == "rules_xref" and "suggested_target" in details:
        lines.append(
            f"- Reference {_md_value(details['reference'])}; suggested target "
            f"`{details['suggested_target']}` ({details['suggested_title']})"
        )
    elif conflict.kind == "marker_mismatch" and "printings" in details:
        printings = details["printings"]
        if isinstance(printings, dict):
            for pid, found in sorted(printings.items()):
                lines.append(f"  - `{pid}`: {_md_value(found)}")
        else:
            for row in printings:
                lines.append(
                    f"  - `{row['product_id']}`: stored {_md_value(row['stored'])}, "
                    f"derived {_md_value(row['derived_from_effect'])}"
                )
    evidence = details.get("evidence")
    if evidence:
        lines.append("- Evidence:")
        for item in evidence:
            mark = "" if item.get("verified") else " **(NOT FOUND — stale)**"
            field_note = f" [{item['field']}]" if item.get("field") else ""
            lines.append(
                f"  - {item['source']} {item['ref']}{field_note}: “{_md_escape(str(item['quote']))}”{mark}"
            )
    return lines


def _render_resolution(resolution: JsonDict | None) -> list[str]:
    if resolution is None:
        return ["- **Resolution: MISSING**"]
    lines = [
        f"- **Resolution** (`{resolution.get('policy')}`): {_md_escape(str(resolution.get('decision', '')))}",
        f"  - Rationale: {_md_escape(str(resolution.get('rationale', '')))}",
    ]
    if resolution.get("canonical_product_id"):
        lines.append(f"  - Canonical printing: `{resolution['canonical_product_id']}`")
    if resolution.get("field_overrides"):
        for field_name, value in resolution["field_overrides"].items():
            lines.append(f"  - Override `{field_name}` → {_md_value(value)}")
    return lines


def render_markdown(report: Report) -> str:
    lines = [MARKDOWN_HEADER]
    lines.append("## Policies\n")
    lines.append("| Policy | Summary | Rationale |")
    lines.append("| --- | --- | --- |")
    for name, policy in report.policies.items():
        summary = _md_escape(str(policy.get("summary", ""))) if isinstance(policy, dict) else ""
        rationale = _md_escape(str(policy.get("rationale", ""))) if isinstance(policy, dict) else ""
        lines.append(f"| `{name}` | {summary} | {rationale} |")
    inputs = report.inputs
    lines.append("\n## Inputs\n")
    lines.append(
        f"- gcg-api dataset `{inputs['dataset_version']}` (built {inputs['dataset_built_at']})"
    )
    lines.append(
        f"- Comprehensive Rules Ver. {inputs['rules_version']} (updated {inputs['rules_updated']})"
    )
    official = inputs.get("official_rules_version")
    if official:
        lines.append(
            f"- Official latest rules: Ver. {official.get('latest_version')} "
            f"(updated {official.get('latest_date')}) — {official.get('url')}"
        )
    else:
        lines.append("- Official latest rules: record not fetched yet")
    counts = inputs["counts"]
    lines.append(
        f"- {counts['printings']} printings / {counts['card_numbers']} card numbers, "
        f"{counts['rulings']} card rulings, {counts['rules_faq']} rules-FAQ entries, "
        f"{counts['errata']} errata"
    )
    lines.append("- SHA-256 of every input:")
    for label, digest in inputs["sha256"].items():
        lines.append(f"  - `{label}`: `{digest}`")
    lines.append("\n## Summary\n")
    lines.append("| Kind | info | minor | major |")
    lines.append("| --- | ---: | ---: | ---: |")
    for kind, row in inputs["conflicts_by_kind"].items():
        lines.append(
            f"| {KIND_TITLES.get(kind, kind)} | {row['info']} | {row['minor']} | {row['major']} |"
        )
    lines.append(
        f"\nTotal: {len(report.conflicts)} conflicts; unresolved: {len(report.unresolved)}; "
        f"invalid resolutions: {len(report.invalid)}; stale curated entries: {len(report.stale)}; "
        f"orphaned resolutions: {len(report.orphaned)}."
    )
    for label, items in (
        ("Unresolved", report.unresolved),
        ("Invalid", report.invalid),
        ("Stale curated entries", report.stale),
        ("Orphaned resolutions", report.orphaned),
    ):
        if items:
            lines.append(f"\n**{label}:**\n")
            lines.extend(f"- {item}" for item in items)
    by_kind: dict[str, list[Conflict]] = defaultdict(list)
    for conflict in report.conflicts:
        by_kind[conflict.kind].append(conflict)
    for kind in [k for k in KIND_ORDER if k in by_kind] + sorted(set(by_kind) - set(KIND_ORDER)):
        group = by_kind[kind]
        lines.append(f"\n## {KIND_TITLES.get(kind, kind)} ({len(group)})\n")
        for conflict in group:
            lines.append(f"### `{conflict.id}` — {_md_escape(conflict.title)}\n")
            cards = ", ".join(conflict.card_numbers) if conflict.card_numbers else "—"
            flags = [f"severity **{conflict.severity}**", f"origin {conflict.origin}"]
            if conflict.engine_behaviour:
                flags.append("changes engine behaviour")
            lines.append(f"- {'; '.join(flags)}")
            lines.append(f"- Cards: {cards}")
            lines.append(f"- {_md_escape(conflict.description)}")
            lines.extend(_render_details(conflict))
            if conflict.problems:
                lines.append("- **Problems:** " + "; ".join(conflict.problems))
            lines.append("- Sources: " + "; ".join(_md_escape(s) for s in conflict.sources))
            lines.extend(_render_resolution(report.resolutions.get(conflict.id)))
            lines.append("")
    return "\n".join(lines).rstrip() + "\n"


# ---------------------------------------------------------------------------------------------
# CLI


def _project_root() -> Path:
    for parent in Path(__file__).resolve().parents:
        if (parent / "pyproject.toml").exists():
            return parent
    return Path.cwd()


def _write_or_compare(path: Path, content: str, check: bool, failures: list[str]) -> None:
    if check:
        current = path.read_text(encoding="utf-8") if path.exists() else None
        if current != content:
            failures.append(f"{path} is stale; regenerate with python -m gcg_sim.sources.conflicts")
        return
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8")


def main(argv: Sequence[str] | None = None, out: Callable[[str], None] = print) -> int:
    root = _project_root()
    parser = argparse.ArgumentParser(
        prog="python -m gcg_sim.sources.conflicts", description=(__doc__ or "").split("\n")[0]
    )
    parser.add_argument(
        "--data-dir", type=Path, default=DEFAULT_DATA_DIR, help="gcg-api snapshot directory"
    )
    parser.add_argument(
        "--rules", type=Path, default=DEFAULT_RULES, help="comprehensive rules markdown"
    )
    parser.add_argument(
        "--official-dir", type=Path, default=DEFAULT_OFFICIAL_DIR, help="official-site records"
    )
    parser.add_argument(
        "--curated", type=Path, default=DEFAULT_CURATED, help="curated conflicts JSON"
    )
    parser.add_argument(
        "--overrides", type=Path, default=DEFAULT_OVERRIDES, help="resolutions JSON"
    )
    parser.add_argument("--out-json", type=Path, default=root / "data" / "conflicts.json")
    parser.add_argument("--out-md", type=Path, default=root / "docs" / "CONFLICTS.md")
    parser.add_argument(
        "--check", action="store_true", help="fail if outputs are stale or anything is unresolved"
    )
    args = parser.parse_args(argv)
    try:
        inputs = load_inputs(
            args.data_dir, args.rules, args.official_dir, args.curated, args.overrides
        )
    except SourceError as exc:
        out(f"error: {exc}")
        return 2
    report = build_report(inputs)
    failures: list[str] = []
    _write_or_compare(args.out_json, render_json(report), args.check, failures)
    _write_or_compare(args.out_md, render_markdown(report), args.check, failures)
    severities = Counter(c.severity for c in report.conflicts)
    out(
        f"{len(report.conflicts)} conflicts ({severities['major']} major, {severities['minor']} minor, "
        f"{severities['info']} info); unresolved {len(report.unresolved)}, invalid {len(report.invalid)}, "
        f"stale {len(report.stale)}, orphaned {len(report.orphaned)}"
    )
    for orphan in report.orphaned:
        out(f"warning: orphaned resolution {orphan}")
    if not args.check:
        return 0
    failures += report.check_failures
    for failure in failures:
        out(f"check failed: {failure}")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
