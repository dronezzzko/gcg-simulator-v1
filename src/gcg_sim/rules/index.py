"""Machine-readable index of the Gundam Card Game comprehensive rules.

The packaged rules markdown is the source of truth. It is parsed into one
``RuleEntry`` per numbered rule (top-level sections, numbered headings and
bold-numbered rules). ``rules_na.json`` lists the rules that are not testable
for the 1v1 engine, each with a reason; every other rule needs a test.

Run ``python -m gcg_sim.rules.index --write`` to regenerate ``rules_index.json``
and ``--check`` to verify that it is current.
"""

from __future__ import annotations

import argparse
import datetime as dt
import functools
import hashlib
import json
import re
import sys
from collections.abc import Mapping, Sequence
from dataclasses import dataclass, field
from pathlib import Path
from types import MappingProxyType
from typing import Final, Literal

RULES_DATA_DIR: Final = Path(__file__).resolve().parent.parent / "data" / "rules"
RULES_MARKDOWN_PATH: Final = RULES_DATA_DIR / "gundam-card-game-comprehensive-rules.md"
RULES_INDEX_PATH: Final = RULES_DATA_DIR / "rules_index.json"
RULES_NA_PATH: Final = RULES_DATA_DIR / "rules_na.json"
SCHEMA_VERSION: Final = 1

RuleKind = Literal["section", "heading", "rule"]
RuleStatus = Literal["na", "testable"]

FEATURE_AREAS: Final[tuple[str, ...]] = (
    "fundamentals",
    "win-loss",
    "card-info",
    "card-types",
    "pairing-link",
    "deck-construction",
    "setup",
    "zones",
    "information",
    "terminology",
    "damage",
    "tokens",
    "turn-structure",
    "battle",
    "action-step",
    "effects-triggers",
    "rules-management",
    "keywords",
    "multiplayer",
)

_FEATURE_AREA_BY_PREFIX: Final[Mapping[str, str]] = MappingProxyType(
    {
        "1": "fundamentals",
        "1-2": "win-loss",
        "1-3-7": "effects-triggers",
        "2": "card-info",
        "2-1-2": "deck-construction",
        "2-4-3": "pairing-link",
        "2-5-5": "pairing-link",
        "2-7-3": "pairing-link",
        "2-8-4": "pairing-link",
        "2-9-3": "tokens",
        "2-10-3": "tokens",
        "2-11-3": "pairing-link",
        "2-12": "pairing-link",
        "3": "card-types",
        "3-2-2": "deck-construction",
        "3-2-6": "pairing-link",
        "3-3": "pairing-link",
        "3-3-2": "deck-construction",
        "3-4-2": "deck-construction",
        "3-4-6": "pairing-link",
        "3-5-2": "deck-construction",
        "3-6-2": "deck-construction",
        "4": "zones",
        "4-1-3": "information",
        "4-1-4": "information",
        "4-1-7": "information",
        "4-2-2": "information",
        "4-3-2": "information",
        "4-4-3": "information",
        "4-5-2": "information",
        "4-5-3": "information",
        "4-5-5": "information",
        "4-6-3-1": "information",
        "4-6-4": "information",
        "4-6-4-2": "zones",
        "4-7-2": "information",
        "4-8-2": "information",
        "4-8-3": "information",
        "4-9-2": "information",
        "5": "terminology",
        "5-5": "damage",
        "5-6": "damage",
        "5-9": "pairing-link",
        "5-16": "effects-triggers",
        "5-17": "tokens",
        "5-18": "damage",
        "5-20": "effects-triggers",
        "5-21": "damage",
        "5-22": "battle",
        "6": "setup",
        "6-1": "deck-construction",
        "6-1-2": "setup",
        "7": "turn-structure",
        "7-3-1-1": "win-loss",
        "7-5-4": "battle",
        "7-6-3": "action-step",
        "8": "battle",
        "8-4": "action-step",
        "9": "action-step",
        "10": "effects-triggers",
        "11": "rules-management",
        "12": "multiplayer",
        "13": "keywords",
    }
)

_MONTHS: Final[Mapping[str, int]] = MappingProxyType(
    {
        name: number
        for number, name in enumerate(
            ("jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"),
            start=1,
        )
    }
)

_START_MARKER: Final = "## Comprehensive Rules"
_VERSION_RE: Final = re.compile(r"^Ver\.\s*(?P<version>\d+(?:\.\d+)+)$")
_DATE_RE: Final = re.compile(
    r"^Updated\s+(?P<month>[A-Za-z]+)\.?\s+(?P<day>\d{1,2}),\s*(?P<year>\d{4})$"
)
_SECTION_RE: Final = re.compile(r"^### (?P<id>\d+)\) (?P<title>.+)$")
_HEADING_RE: Final = re.compile(r"^#{4,6} (?P<id>\d+(?:-\d+)+)\. (?P<title>.+)$")
_RULE_RE: Final = re.compile(r"^\*\*(?P<id>\d+(?:-\d+)*)\.\*\*\s*(?P<text>.+)$")
_EXAMPLE_RE: Final = re.compile(r"^> Ex:\s*(?P<text>.+)$")
_MARKDOWN_ESCAPE_RE: Final = re.compile(r"\\([\\`*_{}\[\]()#+\-.!<>|])")


class RulesIndexError(ValueError):
    """The rules markdown or the N/A data file is malformed or inconsistent."""


@dataclass(frozen=True, slots=True)
class RuleEntry:
    """One numbered rule.

    ``title`` is set only for sections and headings; for them ``text`` repeats
    the title. ``section_title`` is the title of the nearest section or heading
    at or above this rule.
    """

    id: str
    kind: RuleKind
    title: str | None
    text: str
    section: str
    section_title: str
    parent_id: str | None
    depth: int
    examples: tuple[str, ...]
    line_no: int
    feature_area: str

    @property
    def number(self) -> tuple[int, ...]:
        return rule_number(self.id)


@dataclass(frozen=True)
class RulesIndex:
    version: str
    date: dt.date
    date_text: str
    source_sha256: str
    entries: tuple[RuleEntry, ...]
    na_reasons: Mapping[str, str] = field(default_factory=lambda: MappingProxyType({}))
    _by_id: Mapping[str, RuleEntry] = field(init=False, repr=False, compare=False)
    _children: Mapping[str, tuple[str, ...]] = field(init=False, repr=False, compare=False)

    def __post_init__(self) -> None:
        by_id: dict[str, RuleEntry] = {}
        children: dict[str, list[str]] = {}
        for entry in self.entries:
            if entry.id in by_id:
                raise RulesIndexError(f"duplicate rule id {entry.id!r} (line {entry.line_no})")
            by_id[entry.id] = entry
            if entry.parent_id is not None:
                if entry.parent_id not in by_id:
                    raise RulesIndexError(
                        f"rule {entry.id!r} appears before or without its parent {entry.parent_id!r}"
                    )
                children.setdefault(entry.parent_id, []).append(entry.id)
        unknown = sorted(set(self.na_reasons) - set(by_id), key=rule_number)
        if unknown:
            raise RulesIndexError(f"N/A entries for unknown rule ids: {', '.join(unknown)}")
        object.__setattr__(self, "na_reasons", MappingProxyType(dict(self.na_reasons)))
        object.__setattr__(self, "_by_id", MappingProxyType(by_id))
        object.__setattr__(
            self,
            "_children",
            MappingProxyType({key: tuple(value) for key, value in children.items()}),
        )

    def __len__(self) -> int:
        return len(self.entries)

    def __contains__(self, rule_id: object) -> bool:
        return rule_id in self._by_id

    def __getitem__(self, rule_id: str) -> RuleEntry:
        return self._by_id[rule_id]

    def get(self, rule_id: str) -> RuleEntry | None:
        return self._by_id.get(rule_id)

    def ids(self) -> tuple[str, ...]:
        return tuple(entry.id for entry in self.entries)

    def children(self, rule_id: str) -> tuple[str, ...]:
        if rule_id not in self._by_id:
            raise KeyError(rule_id)
        return self._children.get(rule_id, ())

    def status(self, rule_id: str) -> RuleStatus:
        if rule_id not in self._by_id:
            raise KeyError(rule_id)
        return "na" if rule_id in self.na_reasons else "testable"

    def na_reason(self, rule_id: str) -> str | None:
        if rule_id not in self._by_id:
            raise KeyError(rule_id)
        return self.na_reasons.get(rule_id)

    def testable_ids(self) -> tuple[str, ...]:
        return tuple(entry.id for entry in self.entries if entry.id not in self.na_reasons)

    def na_ids(self) -> tuple[str, ...]:
        return tuple(entry.id for entry in self.entries if entry.id in self.na_reasons)


def rule_number(rule_id: str) -> tuple[int, ...]:
    return tuple(int(part) for part in rule_id.split("-"))


def feature_area_for(rule_id: str) -> str:
    parts = rule_id.split("-")
    for length in range(len(parts), 0, -1):
        area = _FEATURE_AREA_BY_PREFIX.get("-".join(parts[:length]))
        if area is not None:
            return area
    raise RulesIndexError(f"no feature area configured for rule {rule_id!r}")


def _unescape(text: str) -> str:
    return _MARKDOWN_ESCAPE_RE.sub(r"\1", text).strip()


def _parse_date(match: re.Match[str]) -> dt.date:
    month = _MONTHS.get(match["month"][:3].lower())
    if month is None:
        raise RulesIndexError(f"unrecognised month in rules date: {match['month']!r}")
    return dt.date(int(match["year"]), month, int(match["day"]))


@dataclass
class _RawEntry:
    id: str
    kind: RuleKind
    title: str | None
    text: str
    line_no: int
    examples: list[str] = field(default_factory=list)


def parse_rules(markdown: str, na_reasons: Mapping[str, str] | None = None) -> RulesIndex:
    lines = markdown.splitlines()
    try:
        start = lines.index(_START_MARKER)
    except ValueError:
        raise RulesIndexError(f"missing {_START_MARKER!r} line") from None

    version: str | None = None
    date: dt.date | None = None
    date_text: str | None = None
    for line in lines[:start]:
        stripped = line.strip()
        if version is None and (version_match := _VERSION_RE.match(stripped)):
            version = version_match["version"]
        elif date is None and (date_match := _DATE_RE.match(stripped)):
            date = _parse_date(date_match)
            date_text = stripped
    if version is None or date is None or date_text is None:
        raise RulesIndexError("rules preamble must contain 'Ver. X.Y.Z' and 'Updated Mon D, YYYY'")

    raw: list[_RawEntry] = []
    for line_no, line in enumerate(lines[start + 1 :], start=start + 2):
        stripped = line.strip()
        if not stripped:
            continue
        if section_match := _SECTION_RE.match(stripped):
            title = _unescape(section_match["title"])
            raw.append(_RawEntry(section_match["id"], "section", title, title, line_no))
        elif heading_match := _HEADING_RE.match(stripped):
            title = _unescape(heading_match["title"])
            raw.append(_RawEntry(heading_match["id"], "heading", title, title, line_no))
        elif rule_match := _RULE_RE.match(stripped):
            raw.append(
                _RawEntry(rule_match["id"], "rule", None, _unescape(rule_match["text"]), line_no)
            )
        elif example_match := _EXAMPLE_RE.match(stripped):
            if not raw:
                raise RulesIndexError(f"line {line_no}: example before any rule")
            raw[-1].examples.append(_unescape(example_match["text"]))
        else:
            raise RulesIndexError(f"line {line_no}: unrecognised rules line: {stripped[:80]!r}")

    titles: dict[str, str] = {}
    entries: list[RuleEntry] = []
    for item in raw:
        number = item.id.split("-")
        parent_id = "-".join(number[:-1]) if len(number) > 1 else None
        if item.title is not None:
            titles[item.id] = item.title
        section_title = next(
            (
                titles["-".join(number[:length])]
                for length in range(len(number), 0, -1)
                if "-".join(number[:length]) in titles
            ),
            None,
        )
        if section_title is None:
            raise RulesIndexError(f"rule {item.id!r} (line {item.line_no}) has no titled section")
        entries.append(
            RuleEntry(
                id=item.id,
                kind=item.kind,
                title=item.title,
                text=item.text,
                section=number[0],
                section_title=section_title,
                parent_id=parent_id,
                depth=len(number),
                examples=tuple(item.examples),
                line_no=item.line_no,
                feature_area=feature_area_for(item.id),
            )
        )

    return RulesIndex(
        version=version,
        date=date,
        date_text=date_text,
        source_sha256=hashlib.sha256(markdown.encode("utf-8")).hexdigest(),
        entries=tuple(entries),
        na_reasons=MappingProxyType(dict(na_reasons or {})),
    )


def parse_na_reasons(payload: object) -> dict[str, str]:
    if not isinstance(payload, dict):
        raise RulesIndexError("rules_na.json must be a JSON object")
    if payload.get("schema_version") != SCHEMA_VERSION:
        raise RulesIndexError(f"rules_na.json schema_version must be {SCHEMA_VERSION}")
    rules = payload.get("rules")
    if not isinstance(rules, dict):
        raise RulesIndexError("rules_na.json 'rules' must be an object")
    reasons: dict[str, str] = {}
    for rule_id, value in rules.items():
        if not isinstance(rule_id, str) or not re.fullmatch(r"\d+(?:-\d+)*", rule_id):
            raise RulesIndexError(f"rules_na.json: malformed rule id {rule_id!r}")
        if not isinstance(value, dict) or set(value) != {"status", "reason"}:
            raise RulesIndexError(f"rules_na.json[{rule_id}] must have exactly status and reason")
        status, reason = value["status"], value["reason"]
        if status != "na":
            raise RulesIndexError(f"rules_na.json[{rule_id}].status must be 'na'")
        if not isinstance(reason, str) or not reason.strip():
            raise RulesIndexError(f"rules_na.json[{rule_id}].reason must be a non-empty string")
        reasons[rule_id] = reason
    return reasons


def load_na_reasons(path: Path = RULES_NA_PATH) -> dict[str, str]:
    return parse_na_reasons(json.loads(path.read_text(encoding="utf-8")))


@functools.cache
def load_rules_index(
    markdown_path: Path = RULES_MARKDOWN_PATH, na_path: Path = RULES_NA_PATH
) -> RulesIndex:
    return parse_rules(markdown_path.read_text(encoding="utf-8"), load_na_reasons(na_path))


def _status_counts(index: RulesIndex, ids: Sequence[str]) -> dict[str, int]:
    na = sum(1 for rule_id in ids if rule_id in index.na_reasons)
    return {"na": na, "testable": len(ids) - na, "total": len(ids)}


def _entry_payload(index: RulesIndex, entry: RuleEntry) -> dict[str, object]:
    payload: dict[str, object] = {
        "children": list(index.children(entry.id)),
        "depth": entry.depth,
        "examples": list(entry.examples),
        "feature_area": entry.feature_area,
        "id": entry.id,
        "kind": entry.kind,
        "line_no": entry.line_no,
        "na_reason": index.na_reasons.get(entry.id),
        "parent_id": entry.parent_id,
        "section": entry.section,
        "section_title": entry.section_title,
        "status": index.status(entry.id),
        "text": entry.text,
        "title": entry.title,
    }
    return dict(sorted(payload.items()))


def index_to_json(index: RulesIndex, source_name: str = RULES_MARKDOWN_PATH.name) -> str:
    sections = sorted({entry.section for entry in index.entries}, key=int)
    payload = {
        "schema_version": SCHEMA_VERSION,
        "source": {
            "date": index.date.isoformat(),
            "date_text": index.date_text,
            "file": source_name,
            "sha256": index.source_sha256,
            "version": index.version,
        },
        "counts": {
            **_status_counts(index, index.ids()),
            "by_kind": {
                kind: _status_counts(index, [e.id for e in index.entries if e.kind == kind])
                for kind in ("section", "heading", "rule")
            },
            "by_section": {
                section: _status_counts(
                    index, [e.id for e in index.entries if e.section == section]
                )
                for section in sections
            },
            "by_feature_area": {
                area: _status_counts(index, [e.id for e in index.entries if e.feature_area == area])
                for area in FEATURE_AREAS
            },
        },
        "feature_areas": list(FEATURE_AREAS),
        "rules": {entry.id: _entry_payload(index, entry) for entry in index.entries},
    }
    return json.dumps(payload, indent=2, ensure_ascii=False) + "\n"


def main(argv: Sequence[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        prog="python -m gcg_sim.rules.index",
        description="Regenerate or verify rules_index.json from the packaged rules markdown.",
    )
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--write", action="store_true", help="regenerate rules_index.json")
    mode.add_argument("--check", action="store_true", help="fail if rules_index.json is stale")
    parser.add_argument("--markdown", type=Path, default=RULES_MARKDOWN_PATH)
    parser.add_argument("--na", type=Path, default=RULES_NA_PATH)
    parser.add_argument("--index", type=Path, default=RULES_INDEX_PATH)
    args = parser.parse_args(argv)

    markdown_path: Path = args.markdown
    index_path: Path = args.index
    index = parse_rules(markdown_path.read_text(encoding="utf-8"), load_na_reasons(args.na))
    rendered = index_to_json(index, markdown_path.name)
    if args.write:
        index_path.write_text(rendered, encoding="utf-8")
        print(
            f"wrote {index_path} ({len(index)} rules, {len(index.na_ids())} N/A, "
            f"{len(index.testable_ids())} testable)"
        )
        return 0
    current = index_path.read_text(encoding="utf-8") if index_path.exists() else None
    if current != rendered:
        print(
            f"{index_path} is stale; run `uv run python -m gcg_sim.rules.index --write`",
            file=sys.stderr,
        )
        return 1
    print(f"{index_path} is current ({len(index)} rules)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
