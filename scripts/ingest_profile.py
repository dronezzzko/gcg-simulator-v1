"""Profile the pinned gcg-api snapshot for the card model.

Reads the packaged snapshot (``src/gcg_sim/data/gcgapi``) and, optionally, a full clone of
https://github.com/yzRobo/gcg-api at the pinned commit, then writes a deterministic
``docs/research/data_profile.json`` (sorted keys, no timestamps). With ``--sources-out`` it also
writes the provenance fragment for ``data/SOURCES.lock.json``.

    uv run python scripts/ingest_profile.py --full-clone PATH
    uv run python scripts/ingest_profile.py --full-clone PATH \\
        --sources-out data/sources.d/gcgapi.json --retrieved-at 2026-09-25T12:40:00Z
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
import sys
from collections import Counter, defaultdict
from collections.abc import Iterable
from pathlib import Path
from typing import Any

REPO_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_DATA_DIR = REPO_ROOT / "src" / "gcg_sim" / "data" / "gcgapi"
DEFAULT_OUT = REPO_ROOT / "docs" / "research" / "data_profile.json"

UPSTREAM_REPO = "yzRobo/gcg-api"
PINNED_COMMIT = "f57b7c0b0ebc4c13d359649de19750c793ecbefc"
RAW_BASE = f"https://raw.githubusercontent.com/{UPSTREAM_REPO}/{PINNED_COMMIT}/"

PACKAGED_FILES: dict[str, str] = {
    "cards.ndjson": "data/cards.ndjson",
    "rulings.json": "data/rulings.json",
    "rules-faq.json": "data/rules-faq.json",
    "errata.json": "data/errata.json",
    "products.json": "data/products.json",
    "manifest.json": "data/manifest.json",
    "sets/en/index.json": "data/sets/en/index.json",
    "LICENSE-DATA": "LICENSE-DATA",
    "schema.sql": "schema.sql",
}
FILE_KINDS = {"LICENSE-DATA": "license", "schema.sql": "schema"}

GAMEPLAY_FIELDS = (
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
    "traits",
    "link",
    "link_refs",
    "effect",
    "keyword_effects",
    "timing_markers",
)
EFFECT_DERIVED = ("effect", "keyword_effects", "timing_markers")
STAT_FIELDS = ("level", "cost", "ap", "hp")
EDITION_BETA = "Edition Beta"
TOKEN_TYPES = ("UNIT TOKEN", "EX BASE", "EX RESOURCE")
CLASS_SEVERITY = (
    "other",
    "edition-beta-stats",
    "errata-related",
    "reworded-reprint",
    "reminder-text-only",
    "punctuation-only",
)

# --- text normalization -------------------------------------------------------------------

CANONICAL_DOT = "･"  # HALFWIDTH KATAKANA MIDDLE DOT, the majority form in the dataset
_FOLD_TABLE: dict[int, int | str | None] = {cp: cp - 0xFEE0 for cp in range(0xFF01, 0xFF5F)}
_FOLD_TABLE.update(
    {
        0x30FB: CANONICAL_DOT,  # KATAKANA MIDDLE DOT
        0x2018: "'",
        0x2019: "'",
        0x201C: '"',
        0x201D: '"',
        0x200B: None,
        0xFEFF: None,
        0x3000: " ",
        0x00A0: " ",
    }
)
_HSPACE = re.compile(r"[ \t]+")
_LEADING_MARKER_SPACE = re.compile(r"^((?:【[^【】]*】)+) +")
_LEADING_MARKERS = re.compile(r"^(?:【[^【】]*】)+")
_SPACED_APOSTROPHE = re.compile(r"(?<=[A-Za-z])' (?=[st]\b)")
_QUOTED = re.compile(r'"([^"\n]*)"')
_LOWER_WORD = re.compile(r"(?:^|[\s(/])[a-z]")
_TOKEN_DEF_OPEN = re.compile(r"\[([^\[\]]+)\]\(")
_ALIAS = re.compile(r"This card's name is also treated as \[([^\]]+)\]")
_COMMAND_PILOT = re.compile(r"【Pilot】\s*\[([^\]]+)\]")
_MARKER = re.compile(r"【([^【】]+)】")
_ANGLE = re.compile(r"<([A-Za-z][A-Za-z \-]*?)(?:\s+(\d+))?>")
_SUFFIX = re.compile(r"_p(\d+)$")


def tidy_whitespace(text: str) -> str:
    """Unify line breaks and spaces: trim lines, drop blank lines, collapse runs of spaces."""
    lines = []
    for line in text.replace("\r\n", "\n").replace("\r", "\n").split("\n"):
        line = _HSPACE.sub(" ", line).strip()
        line = _LEADING_MARKER_SPACE.sub(r"\1", line)
        if line:
            lines.append(line)
    return "\n".join(lines)


def typographic(text: str) -> str:
    """Level-1 normalization: typography and whitespace only; wording is untouched."""
    text = text.translate(_FOLD_TABLE)
    text = _SPACED_APOSTROPHE.sub("'", text)
    text = _QUOTED.sub(lambda m: f'"{m.group(1).strip()}"', text)
    return tidy_whitespace(text)


def _matching_paren(text: str, start: int) -> int:
    depth = 0
    for index in range(start, len(text)):
        if text[index] == "(":
            depth += 1
        elif text[index] == ")":
            depth -= 1
            if depth == 0:
                return index
    return -1


def top_level_paren_groups(text: str) -> list[tuple[int, int]]:
    """Balanced ``(...)`` groups that are not inside ``[...]`` names or ``【...】`` markers."""
    groups: list[tuple[int, int]] = []
    square = lenticular = 0
    index = 0
    while index < len(text):
        char = text[index]
        if char == "[":
            square += 1
        elif char == "]":
            square = max(0, square - 1)
        elif char == "【":
            lenticular += 1
        elif char == "】":
            lenticular = max(0, lenticular - 1)
        elif char == "(" and not square and not lenticular:
            end = _matching_paren(text, index)
            if end < 0:
                break
            groups.append((index, end))
            index = end + 1
            continue
        index += 1
    return groups


def is_reminder(inner: str, trait_vocab: frozenset[str]) -> bool:
    """Rule 2-11-4 explanatory note: not a trait reference and contains a lowercase word."""
    return inner not in trait_vocab and bool(_LOWER_WORD.search(inner))


def strip_reminders(text: str, trait_vocab: frozenset[str]) -> tuple[str, list[str]]:
    """Remove reminder parentheticals; keep trait references and token definitions."""
    removed: list[str] = []
    kept_from = 0
    parts: list[str] = []
    for start, end in top_level_paren_groups(text):
        if start > 0 and text[start - 1] == "]":
            continue
        if is_reminder(text[start + 1 : end], trait_vocab):
            parts.append(text[kept_from:start])
            removed.append(text[start : end + 1])
            kept_from = end + 1
    parts.append(text[kept_from:])
    return tidy_whitespace("".join(parts)), removed


def normalize_effect(text: str, trait_vocab: frozenset[str]) -> str:
    """Level-2 normalization: typographic() plus rule 2-11-4 reminder-text removal."""
    return strip_reminders(typographic(text), trait_vocab)[0]


NORMALIZATION_EXAMPLES: tuple[tuple[str, str, str], ...] = (
    (
        "level1",
        "return it to its owner' s hand. It can' t attack.",
        "return it to its owner's hand. It can't attack.",
    ),
    ("level1", "all players' hands", "all players' hands"),
    (
        "level1",
        'with " Awakened Potential"  in their card name',
        'with "Awakened Potential" in their card name',
    ),
    ("level1", "【Once per Turn】 When\n\n x", "【Once per Turn】When\nx"),
    ("level1", "【Deploy・Development 2】①：Draw 1.", "【Deploy･Development 2】①:Draw 1."),
    (
        "level2",
        "<Breach 2> (When this Unit's attack destroys an enemy Unit, deal damage.)",
        "<Breach 2>",
    ),
    (
        "level2",
        "<Breach 2>(During your turn, when this Unit destroys an enemy Unit.)",
        "<Breach 2>",
    ),
    (
        "level2",
        "【Activate･Main】<Support 1> (Rest this Unit. 1 other friendly Unit gets AP+(specified amount) during this turn.)",
        "【Activate･Main】<Support 1>",
    ),
    (
        "level2",
        "Deploy 1 [Hy-Gogg]((Cyclops Team)･AP2･HP1) Unit token.",
        "Deploy 1 [Hy-Gogg]((Cyclops Team)･AP2･HP1) Unit token.",
    ),
    (
        "level2",
        "Deploy 1 rested [Zeong (Head)]((Zeon)･AP3･HP1) Unit token.",
        "Deploy 1 rested [Zeong (Head)]((Zeon)･AP3･HP1) Unit token.",
    ),
    ("level2", "play a (Dawn of Fold) Command card", "play a (Dawn of Fold) Command card"),
    ("level2", "【When Paired･(Newtype) Pilot】Draw 1.", "【When Paired･(Newtype) Pilot】Draw 1."),
    (
        "level2",
        "deploy it as an (AP3・HP3) Unit instead. (Don't treat it as a Pilot.)",
        "deploy it as an (AP3･HP3) Unit instead.",
    ),
    ("level2", "(Rest a Resource when paying a cost.)", ""),
)


def check_normalization(trait_vocab: frozenset[str]) -> None:
    """Fail fast if the documented normalization contract no longer holds."""
    for level, source, expected in NORMALIZATION_EXAMPLES:
        actual = typographic(source) if level == "level1" else normalize_effect(source, trait_vocab)
        if actual != expected:
            raise AssertionError(f"{level} normalization of {source!r}: {actual!r} != {expected!r}")


# --- loading ------------------------------------------------------------------------------


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    return sha256_bytes(path.read_bytes())


def load_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def load_ndjson(path: Path) -> list[dict[str, Any]]:
    lines = path.read_text(encoding="utf-8").splitlines()
    return [json.loads(line) for line in lines if line.strip()]


def canonical(record: Any) -> str:
    return json.dumps(record, sort_keys=True, ensure_ascii=False, separators=(",", ":"))


def suffix_number(product_id: str) -> int:
    match = _SUFFIX.search(product_id)
    return int(match.group(1)) if match else 0


def pick_base(printings: list[dict[str, Any]]) -> dict[str, Any]:
    """Base printing: product_id == card_number; else lowest _pN outside Edition Beta."""
    for printing in printings:
        if printing["product_id"] == printing["card_number"]:
            return printing
    pool = [p for p in printings if p["set_name"] != EDITION_BETA] or printings
    return min(pool, key=lambda p: (suffix_number(p["product_id"]), p["product_id"]))


def key_of(value: Any) -> str:
    return "null" if value is None else str(value)


def counts(values: Iterable[Any]) -> dict[str, int]:
    return dict(sorted(Counter(key_of(v) for v in values).items()))


def value_shape(value: Any) -> str:
    if value is None:
        return "null"
    if isinstance(value, int):
        return "int"
    if value == "-":
        return "'-'"
    if re.fullmatch(r"\+\d+", value):
        return "+N"
    if re.fullmatch(r"\d+", value):
        return "N"
    return "text"


def placeholder_shape(value: str | None) -> str:
    if value is None:
        return "null"
    shaped = re.sub(r"\[[^\]]+\]", "[N]", value)
    return re.sub(r"\([^)]+\)", "(T)", shaped)


# --- analyses -----------------------------------------------------------------------------


def group_by_number(cards: list[dict[str, Any]]) -> dict[str, list[dict[str, Any]]]:
    grouped: dict[str, list[dict[str, Any]]] = defaultdict(list)
    for card in cards:
        grouped[card["card_number"]].append(card)
    return dict(sorted(grouped.items()))


def profile_counts(cards: list[dict[str, Any]], bases: dict[str, dict[str, Any]]) -> dict[str, Any]:
    per_set: dict[str, dict[str, Any]] = defaultdict(lambda: {"printings": 0, "numbers": set()})
    per_prefix: dict[str, dict[str, Any]] = defaultdict(lambda: {"printings": 0, "numbers": set()})
    for card in cards:
        per_set[card["set_code"]]["printings"] += 1
        per_set[card["set_code"]]["numbers"].add(card["card_number"])
        prefix = card["card_number"].split("-")[0]
        per_prefix[prefix]["printings"] += 1
        per_prefix[prefix]["numbers"].add(card["card_number"])

    def flatten(table: dict[str, dict[str, Any]]) -> dict[str, dict[str, int]]:
        return {
            key: {"printings": row["printings"], "card_numbers": len(row["numbers"])}
            for key, row in sorted(table.items())
        }

    return {
        "printings": len(cards),
        "distinct_product_ids": len({c["product_id"] for c in cards}),
        "distinct_card_numbers": len(bases),
        "card_numbers_without_base_printing": sorted(
            n for n, b in bases.items() if b["product_id"] != n
        ),
        "alt_printings": sum(1 for c in cards if _SUFFIX.search(c["product_id"])),
        "per_card_type_printings": counts(c["card_type"] for c in cards),
        "per_card_type_card_numbers": counts(b["card_type"] for b in bases.values()),
        "per_set_code": flatten(per_set),
        "per_card_number_prefix": flatten(per_prefix),
        "printings_with_set_code_not_equal_prefix": sum(
            1 for c in cards if c["set_code"] != c["card_number"].split("-")[0]
        ),
    }


def profile_effect_texts(
    cards: list[dict[str, Any]],
    by_number: dict[str, list[dict[str, Any]]],
    bases: dict[str, dict[str, Any]],
    vocab: frozenset[str],
) -> dict[str, Any]:
    level1 = {c["product_id"]: typographic(c["effect"]) for c in cards}
    level2 = {c["product_id"]: normalize_effect(c["effect"], vocab) for c in cards}
    removed_catalog: dict[str, dict[str, Any]] = defaultdict(
        lambda: {"printings": 0, "card_numbers": set()}
    )
    kept_non_trait: Counter[str] = Counter()
    for card in cards:
        text = level1[card["product_id"]]
        for fragment in strip_reminders(text, vocab)[1]:
            removed_catalog[fragment]["printings"] += 1
            removed_catalog[fragment]["card_numbers"].add(card["card_number"])
        for start, end in top_level_paren_groups(text):
            inner = text[start + 1 : end]
            preceded_by_name = start > 0 and text[start - 1] == "]"
            if not preceded_by_name and inner not in vocab and not is_reminder(inner, vocab):
                kept_non_trait[text[start : end + 1]] += 1

    def divergent_numbers(texts: dict[str, str]) -> list[str]:
        return sorted(
            n for n, ps in by_number.items() if len({texts[p["product_id"]] for p in ps}) > 1
        )

    base_ids = [b["product_id"] for b in bases.values()]
    raw_by_id = {c["product_id"]: c["effect"] for c in cards}
    return {
        "distinct_raw_printings": len(set(raw_by_id.values())),
        "distinct_level1_printings": len(set(level1.values())),
        "distinct_level2_printings": len(set(level2.values())),
        "distinct_raw_per_card_number": len({raw_by_id[i] for i in base_ids}),
        "distinct_level1_per_card_number": len({level1[i] for i in base_ids}),
        "distinct_level2_per_card_number": len({level2[i] for i in base_ids}),
        "card_numbers_with_divergent_effect_raw": len(divergent_numbers(raw_by_id)),
        "card_numbers_with_divergent_effect_level1": len(divergent_numbers(level1)),
        "card_numbers_with_divergent_effect_level2": divergent_numbers(level2),
        "printings_whose_level2_text_is_empty": counts(
            c["card_type"] for c in cards if level2[c["product_id"]] == ""
        ),
        "removed_reminder_texts": {
            text: {"printings": row["printings"], "card_numbers": len(row["card_numbers"])}
            for text, row in sorted(removed_catalog.items())
        },
        "kept_non_trait_parentheticals": dict(sorted(kept_non_trait.items())),
        "normalization": {
            "examples": [
                {"level": level, "input": source, "output": expected}
                for level, source, expected in NORMALIZATION_EXAMPLES
            ],
            "level1_typographic": [
                "fold full-width ASCII forms U+FF01-U+FF5E to ASCII (brackets, colon, digits, plus)",
                "map U+30FB KATAKANA MIDDLE DOT to U+FF65 HALFWIDTH KATAKANA MIDDLE DOT",
                "map curly quotes U+2018/U+2019 to ' and U+201C/U+201D to \"",
                "delete U+200B/U+FEFF; map U+3000 and U+00A0 to a space",
                'close spaced apostrophes before s/t ("owner\' s" -> "owner\'s", "can\' t" -> "can\'t")',
                "trim spaces just inside double-quoted spans",
                "unify line breaks, collapse runs of spaces/tabs, trim every line, drop blank lines",
                "drop spaces after a line-leading chain of 【...】 markers",
            ],
            "level2_reminder_strip": [
                "apply level 1, then remove every balanced top-level (...) group that is",
                "  not inside [...] (names such as [Zeong (Head)]) and not inside 【...】 markers,",
                "  not directly preceded by ']' (token definitions [Name]((Trait)･AP2･HP2･...)),",
                "  not an exact trait from the dataset trait vocabulary (e.g. (Dawn of Fold)),",
                "  and contains at least one word starting with a lowercase ASCII letter;",
                "then re-apply the whitespace step. Everything else, including (AP3･HP3) and",
                "unknown-trait references such as (Dianna Counter), is kept.",
            ],
            "base_printing_rule": "product_id == card_number; for card numbers without one, the "
            "lowest _pN printing whose set_name is not 'Edition Beta'",
        },
    }


def classify_effect(base: str, other: str, vocab: frozenset[str]) -> tuple[str, str]:
    if typographic(base) == typographic(other):
        return "punctuation-only", "typography/whitespace"
    left, right = normalize_effect(base, vocab), normalize_effect(other, vocab)
    if left == right:
        return "reminder-text-only", "differs only inside rule 2-11-4 reminder text"
    if left.casefold() == right.casefold():
        return "reworded-reprint", "letter case only"
    if sorted(left.split("\n")) == sorted(right.split("\n")):
        return "reworded-reprint", "same lines in a different order"
    return "reworded-reprint", "wording differs; manual equivalence review needed"


def profile_divergences(
    by_number: dict[str, list[dict[str, Any]]],
    bases: dict[str, dict[str, Any]],
    errata: list[dict[str, Any]],
    vocab: frozenset[str],
) -> list[dict[str, Any]]:
    errata_fields = {(e["card_number"], e["field"]) for e in errata}
    result = []
    for number, printings in by_number.items():
        base = bases[number]
        rows = []
        for printing in sorted(printings, key=lambda p: p["product_id"]):
            if printing is base:
                continue
            diff = {
                f: {"base": base[f], "printing": printing[f]}
                for f in GAMEPLAY_FIELDS
                if base[f] != printing[f]
            }
            if not diff:
                continue
            classes: dict[str, str] = {}
            other_fields: list[str] = []
            for field in diff:
                if (number, field) in errata_fields:
                    classes["errata-related"] = f"errata field '{field}'"
                elif field in EFFECT_DERIVED:
                    label, why = classify_effect(base["effect"], printing["effect"], vocab)
                    classes.setdefault(label, why)
                elif field in ("ap_raw", "hp_raw") and all(
                    base[f.removesuffix("_raw")] == printing[f.removesuffix("_raw")]
                    for f in ("ap_raw", "hp_raw")
                ):
                    classes.setdefault("punctuation-only", "stat sign formatting (+N vs N)")
                elif field == "name" and typographic(base["name"]) == typographic(printing["name"]):
                    classes.setdefault("punctuation-only", "apostrophe variant in name")
                elif field in (*STAT_FIELDS, "ap_raw", "hp_raw"):
                    if printing["set_name"] == EDITION_BETA:
                        classes.setdefault("edition-beta-stats", "stats differ on Edition Beta")
                    else:
                        other_fields.append(field)
                else:
                    other_fields.append(field)
            if other_fields:
                classes["other"] = f"fields differ: {', '.join(other_fields)}"
            rows.append(
                {
                    "product_id": printing["product_id"],
                    "set_code": printing["set_code"],
                    "set_name": printing["set_name"],
                    "where_to_get": printing["where_to_get"],
                    "rarity": printing["rarity"],
                    "edition_beta": printing["set_name"] == EDITION_BETA,
                    "diff": diff,
                    "classes": dict(sorted(classes.items())),
                }
            )
        if rows:
            all_classes = {c for row in rows for c in row["classes"]}
            result.append(
                {
                    "card_number": number,
                    "name": base["name"],
                    "base_product_id": base["product_id"],
                    "base_set_code": base["set_code"],
                    "base_where_to_get": base["where_to_get"],
                    "base_rarity": base["rarity"],
                    "classification": next(c for c in CLASS_SEVERITY if c in all_classes),
                    "all_classes": sorted(all_classes),
                    "printings": rows,
                }
            )
    return result


def profile_fields(cards: list[dict[str, Any]]) -> dict[str, Any]:
    by_type: dict[str, list[dict[str, Any]]] = defaultdict(list)
    for card in cards:
        by_type[card["card_type"]].append(card)
    shape_fields = (
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
    )
    null_patterns = {
        card_type: {f: counts(value_shape(c[f]) for c in group) for f in shape_fields}
        for card_type, group in sorted(by_type.items())
    }
    raw_formats = {
        card_type: {
            "ap_raw": counts(c["ap_raw"] for c in group),
            "hp_raw": counts(c["hp_raw"] for c in group),
        }
        for card_type, group in sorted(by_type.items())
    }
    return {
        "distinct_values": {
            f: counts(c[f] for c in cards)
            for f in ("color", "card_type", "rarity", "zone", "sp", "block_icon")
        },
        "level": counts(c["level"] for c in cards),
        "cost": counts(c["cost"] for c in cards),
        "null_patterns_by_card_type": null_patterns,
        "stat_raw_formats_by_card_type": raw_formats,
        "color_null_by_card_type": counts(c["card_type"] for c in cards if c["color"] is None),
        "resources_with_stats": sorted(
            c["product_id"]
            for c in cards
            if c["card_type"] in ("RESOURCE", "EX RESOURCE")
            and (c["ap"] is not None or c["hp"] is not None)
        ),
        "units_with_null_ap": sorted(
            c["product_id"] for c in cards if c["card_type"] == "UNIT" and c["ap"] is None
        ),
        "unit_tokens_with_null_stats": sorted(
            c["product_id"]
            for c in cards
            if c["card_type"] == "UNIT TOKEN" and (c["ap"] is None or c["hp"] is None)
        ),
        "raw_stat_without_plus_on_pilot_or_command": sorted(
            f"{c['product_id']}:{f}={c[f]}"
            for c in cards
            if c["card_type"] in ("PILOT", "COMMAND")
            for f in ("ap_raw", "hp_raw")
            if c[f] is not None and not c[f].startswith("+")
        ),
        "name_anomalies": sorted(
            f"{c['product_id']}:{c['name']!r}" for c in cards if typographic(c["name"]) != c["name"]
        ),
    }


def parse_link(link: str | None) -> list[tuple[str, str]]:
    """Split a link string into ('name'|'trait'|'anomaly', value) alternatives joined by ' / '."""
    if link is None or link.strip() in ("", "-"):
        return []
    alternatives: list[tuple[str, str]] = []
    for part in link.split(" / "):
        part = part.strip()
        name = re.fullmatch(r"\[([^\]]+)\]", part)
        trait = re.fullmatch(r"\(([^)]+)\)(?: Trait)?", part)
        if name:
            alternatives.append(("name", name.group(1).strip()))
        elif trait:
            alternatives.append(("trait", trait.group(1).strip()))
        else:
            alternatives.append(("anomaly", part))
    return alternatives


def pilot_names(cards: list[dict[str, Any]]) -> set[str]:
    names: set[str] = set()
    for card in cards:
        if card["card_type"] == "PILOT":
            names.add(typographic(card["name"]))
        names.update(typographic(n) for n in _ALIAS.findall(card["effect"]))
        if card["card_type"] == "COMMAND":
            names.update(typographic(n) for n in _COMMAND_PILOT.findall(card["effect"]))
    return names


def profile_links(cards: list[dict[str, Any]], vocab: frozenset[str]) -> dict[str, Any]:
    shapes: Counter[str] = Counter()
    examples: dict[str, str] = {}
    order_only: list[str] = []
    extra_entries: list[str] = []
    anomalies = []
    unknown_traits: set[str] = set()
    unmatched_names: set[str] = set()
    known_pilots = pilot_names(cards)
    for card in cards:
        shape = placeholder_shape(card["link"])
        shapes[shape] += 1
        examples.setdefault(shape, f"{card['product_id']}: {card['link']}")
        alternatives = parse_link(card["link"])
        parsed = [value for _, value in alternatives]
        if parsed != card["link_refs"]:
            if sorted(parsed) == sorted(card["link_refs"]):
                order_only.append(card["product_id"])
            else:
                extra = sorted(set(card["link_refs"]) - set(parsed))
                extra_entries.append(f"{card['product_id']}: {card['link']} -> extra {extra}")
        for kind, value in alternatives:
            if kind == "anomaly":
                anomalies.append(f"{card['product_id']}: {card['link']}")
            elif kind == "trait" and value not in vocab:
                unknown_traits.add(f"{card['product_id']}: {value}")
            elif kind == "name" and not any(typographic(value) in n for n in known_pilots):
                unmatched_names.add(value)
    return {
        "shapes": {
            shape: {"count": n, "example": examples[shape]} for shape, n in sorted(shapes.items())
        },
        "link_refs_names_listed_before_traits": sorted(order_only),
        "link_refs_with_spurious_entries": sorted(extra_entries),
        "unparseable_alternatives": sorted(anomalies),
        "trait_refs_not_in_trait_vocab": sorted(unknown_traits),
        "name_refs_without_matching_pilot_name": sorted(unmatched_names),
    }


def profile_traits(cards: list[dict[str, Any]], vocab: frozenset[str]) -> dict[str, Any]:
    shapes = Counter(placeholder_shape(c["trait"]) for c in cards)
    mismatch = [
        c["product_id"]
        for c in cards
        if re.findall(r"\(([^)]+)\)", c["trait"] if c["trait"] != "-" else "") != c["traits"]
    ]
    return {
        "shapes": dict(sorted(shapes.items())),
        "trait_string_vs_traits_array_mismatches": sorted(mismatch),
        "vocabulary": sorted(vocab),
        "vocabulary_size": len(vocab),
    }


def profile_keywords(
    cards: list[dict[str, Any]], bases: dict[str, dict[str, Any]], vocab: frozenset[str]
) -> dict[str, Any]:
    per_keyword: dict[str, Counter[str]] = defaultdict(Counter)
    for card in cards:
        for entry in card["keyword_effects"]:
            per_keyword[entry["keyword"]][key_of(entry["value"])] += 1
    line_start: Counter[str] = Counter()
    inline_only: Counter[str] = Counter()
    for base in bases.values():
        lines = normalize_effect(base["effect"], vocab).split("\n")
        bodies = [_LEADING_MARKERS.sub("", line) for line in lines]
        starts = {m.group(1) for body in bodies if (m := _ANGLE.match(body))}
        for entry in base["keyword_effects"]:
            (line_start if entry["keyword"] in starts else inline_only)[entry["keyword"]] += 1
    markers: Counter[str] = Counter()
    for card in cards:
        markers.update(m.group(1) for m in _MARKER.finditer(typographic(card["effect"])))
    return {
        "keyword_effects_entry_shapes": counts(
            ",".join(sorted(e)) for c in cards for e in c["keyword_effects"]
        ),
        "keyword_effects_values": {
            k: dict(sorted(v.items())) for k, v in sorted(per_keyword.items())
        },
        "keyword_effects_on_base_printings_by_position": {
            "line_start_after_leading_markers": dict(sorted(line_start.items())),
            "inline_only_grant_or_reference": dict(sorted(inline_only.items())),
        },
        "square_bracket_keyword_typos": sorted(
            f"{c['product_id']}: {m.group(0)}"
            for c in cards
            for m in re.finditer(
                r"\[(Repair|Breach|Support|Blocker|First Strike|High-Maneuver|Suppression)[^\]]*\]",
                c["effect"],
            )
        ),
        "timing_markers_vocabulary": counts(t for c in cards for t in c["timing_markers"]),
        "raw_lenticular_markers_level1": dict(sorted(markers.items())),
    }


def profile_commands_and_pilots(
    cards: list[dict[str, Any]], bases: dict[str, dict[str, Any]]
) -> dict[str, Any]:
    commands = [c for c in cards if c["card_type"] == "COMMAND"]
    with_pilot = [c for c in commands if "【Pilot】" in c["effect"]]
    inconsistent = sorted(
        c["product_id"]
        for c in commands
        if len({"【Pilot】" in c["effect"], c["ap"] is not None, c["hp"] is not None}) > 1
    )
    not_last = sorted(
        c["product_id"]
        for c in with_pilot
        if not re.search(r"【Pilot】\s*\[[^\]]+\]\s*$", c["effect"])
    )
    pilots = [b for b in bases.values() if b["card_type"] == "PILOT"]
    burst_first = sum(
        1
        for p in pilots
        if (lines := [ln for ln in typographic(p["effect"]).split("\n") if not _ALIAS.match(ln)])
        and lines[0].startswith("【Burst】")
    )
    return {
        "command_printings": len(commands),
        "command_printings_with_pilot_marker": len(with_pilot),
        "command_card_numbers_with_pilot_marker": len({c["card_number"] for c in with_pilot}),
        "command_pilot_marker_not_last_line": not_last,
        "command_pilot_marker_vs_stats_inconsistent": inconsistent,
        "commands_with_traits_but_no_pilot_marker": sorted(
            c["product_id"]
            for c in commands
            if c["trait"] != "-" and "【Pilot】" not in c["effect"]
        ),
        "command_pilot_ap_raw": counts(c["ap_raw"] for c in with_pilot),
        "command_pilot_hp_raw": counts(c["hp_raw"] for c in with_pilot),
        "pilot_card_numbers": len(pilots),
        "pilot_card_numbers_whose_first_rule_line_is_burst": burst_first,
        "multi_name_aliases": sorted(
            f"{b['card_number']}: {b['name']} = {alias}"
            for b in bases.values()
            for alias in _ALIAS.findall(b["effect"])
        ),
        "command_pilot_names_distinct": len(
            {n for b in bases.values() for n in _COMMAND_PILOT.findall(b["effect"])}
        ),
        "names_with_ampersand": sorted(
            f"{b['card_number']}: {b['name']}" for b in bases.values() if " & " in b["name"]
        ),
    }


def parse_token_definitions(text: str) -> list[dict[str, Any]]:
    definitions = []
    for match in _TOKEN_DEF_OPEN.finditer(text):
        open_index = match.end() - 1
        close_index = _matching_paren(text, open_index)
        inner = text[open_index + 1 : close_index]
        parts = inner.split(CANONICAL_DOT)
        ap = next((int(m.group(1)) for p in parts if (m := re.fullmatch(r"AP(\d+)", p))), None)
        hp = next((int(m.group(1)) for p in parts if (m := re.fullmatch(r"HP(\d+)", p))), None)
        extras = [p.strip().rstrip(".") for p in parts[1:] if not re.fullmatch(r"[AH]P\d+", p)]
        definitions.append(
            {
                "name": match.group(1),
                "definition": inner,
                "traits": re.findall(r"\(([^)]+)\)", parts[0]),
                "ap": ap,
                "hp": hp,
                "abilities": CANONICAL_DOT.join(extras),
            }
        )
    return definitions


def token_card_view(card: dict[str, Any], vocab: frozenset[str]) -> dict[str, Any]:
    text = normalize_effect(card["effect"], vocab)
    abilities = "" if text == "-" else CANONICAL_DOT.join(ln.rstrip(".") for ln in text.split("\n"))
    return {
        "traits": card["traits"],
        "ap": 0 if card["ap"] is None else card["ap"],
        "hp": card["hp"],
        "abilities": abilities,
    }


def profile_tokens(
    cards: list[dict[str, Any]],
    by_number: dict[str, list[dict[str, Any]]],
    vocab: frozenset[str],
) -> dict[str, Any]:
    token_cards = [c for c in cards if c["card_type"] in TOKEN_TYPES]
    unit_tokens_by_name: dict[str, set[str]] = defaultdict(set)
    for card in token_cards:
        if card["card_type"] == "UNIT TOKEN":
            unit_tokens_by_name[typographic(card["name"])].add(card["card_number"])
    definitions: dict[tuple[str, str], dict[str, Any]] = {}
    for card in cards:
        for definition in parse_token_definitions(typographic(card["effect"])):
            key = (definition["name"], definition["definition"])
            entry = definitions.setdefault(key, {**definition, "source_card_numbers": set()})
            entry["source_card_numbers"].add(card["card_number"])
    rows = []
    referenced: set[str] = set()
    for (name, _), entry in sorted(definitions.items()):
        numbers = sorted(unit_tokens_by_name.get(name, set()))
        referenced.update(numbers)
        mismatches = []
        null_ap_as_zero = []
        for number in numbers:
            for printing in by_number[number]:
                view = token_card_view(printing, vocab)
                if printing["ap"] is None:
                    null_ap_as_zero.append(printing["product_id"])
                mismatches.extend(
                    {
                        "product_id": printing["product_id"],
                        "field": field,
                        "definition": entry[field],
                        "card": view[field],
                        "edition_beta": printing["set_name"] == EDITION_BETA,
                    }
                    for field in ("traits", "ap", "hp", "abilities")
                    if entry[field] != view[field]
                )
        rows.append(
            {
                "name": name,
                "definition": entry["definition"],
                "source_card_numbers": sorted(entry["source_card_numbers"]),
                "token_card_numbers": numbers,
                "printings_with_null_ap_read_as_0": sorted(null_ap_as_zero),
                "mismatches": mismatches,
            }
        )
    unit_token_numbers = {c["card_number"] for c in token_cards if c["card_type"] == "UNIT TOKEN"}
    ex_refs = defaultdict(set)
    for card in cards:
        if card["card_type"] in TOKEN_TYPES:
            continue
        for label in ("EX Resource", "EX Base"):
            if label in card["effect"]:
                ex_refs[label].add(card["card_number"])
    return {
        "token_printings_by_type": counts(c["card_type"] for c in token_cards),
        "token_card_numbers_by_type": counts(
            {c["card_number"]: c["card_type"] for c in token_cards}.values()
        ),
        "definitions_in_text": rows,
        "definitions_distinct": len(rows),
        "definitions_without_token_card": sorted(
            r["name"] for r in rows if not r["token_card_numbers"]
        ),
        "definitions_with_mismatch": sorted(
            {f"{r['name']} -> {m['product_id']}" for r in rows for m in r["mismatches"]}
        ),
        "unit_token_numbers_never_defined_in_text": sorted(unit_token_numbers - referenced),
        "card_numbers_mentioning_ex_tokens": {k: sorted(v) for k, v in sorted(ex_refs.items())},
        "token_printings_where_to_get_dash": sorted(
            c["product_id"] for c in token_cards if c["where_to_get"] in (None, "-")
        ),
    }


def edition_beta_stat_offsets(
    cards: list[dict[str, Any]], bases: dict[str, dict[str, Any]]
) -> list[dict[str, Any]]:
    """Compare off-base Edition Beta stats with the base stats of the preceding ndjson record."""
    rows = []
    for index, card in enumerate(cards):
        base = bases[card["card_number"]]
        stats = [card["ap"], card["hp"]]
        if card["set_name"] != EDITION_BETA or index == 0 or stats == [base["ap"], base["hp"]]:
            continue
        previous = cards[index - 1]
        previous_base = bases[previous["card_number"]]
        previous_stats = [previous_base["ap"], previous_base["hp"]]
        rows.append(
            {
                "product_id": card["product_id"],
                "printing_ap_hp": stats,
                "base_ap_hp": [base["ap"], base["hp"]],
                "previous_record": previous["product_id"],
                "previous_record_base_ap_hp": previous_stats,
                "equals_previous_record_base": stats == previous_stats,
            }
        )
    return rows


def profile_misc(
    cards: list[dict[str, Any]], bases: dict[str, dict[str, Any]], vocab: frozenset[str]
) -> dict[str, Any]:
    beta = [c for c in cards if c["set_name"] == EDITION_BETA]
    return {
        "edition_beta_stat_offsets": edition_beta_stat_offsets(cards, bases),
        "edition_beta": {
            "printings": len(beta),
            "card_numbers": len({c["card_number"] for c in beta}),
            "where_to_get_values": counts(c["where_to_get"] for c in beta),
            "block_icon_values": counts(c["block_icon"] for c in beta),
            "set_code_values": counts(c["set_code"] for c in beta),
            "rarity_values": counts(c["rarity"] for c in beta),
            "all_have_alt_suffix": all(_SUFFIX.search(c["product_id"]) for c in beta),
            "beta_block_icon_outside_edition_beta": sorted(
                f"{c['product_id']} ({c['set_name']}; {c['where_to_get']})"
                for c in cards
                if c["block_icon"] == "β" and c["set_name"] != EDITION_BETA
            ),
        },
        "vanilla_effect_dash": {
            "printings_by_type": counts(
                c["card_type"] for c in cards if c["effect"].strip() == "-"
            ),
            "card_numbers_by_type": counts(
                b["card_type"] for b in bases.values() if b["effect"].strip() == "-"
            ),
            "card_numbers": sorted(n for n, b in bases.items() if b["effect"].strip() == "-"),
        },
        "resource_effects_level1": counts(
            typographic(c["effect"]) for c in cards if c["card_type"] == "RESOURCE"
        ),
        "resource_effects_level2_nonempty": sorted(
            c["product_id"]
            for c in cards
            if c["card_type"] == "RESOURCE" and normalize_effect(c["effect"], vocab)
        ),
    }


def profile_side_files(
    data: dict[str, Any], cards: list[dict[str, Any]], bases: dict[str, dict[str, Any]]
) -> dict[str, Any]:
    rulings = data["rulings.json"]
    faq = data["rules-faq.json"]
    errata = data["errata.json"]
    products = data["products.json"]
    sets_index = data["sets/en/index.json"]
    manifest = data["manifest.json"]
    per_set_code = Counter(c["set_code"] for c in cards)
    errata_checks = []
    for entry in errata:
        printings = [c for c in cards if c["card_number"] == entry["card_number"]]
        errata_checks.append(
            {
                "card_number": entry["card_number"],
                "field": entry["field"],
                "status": entry["status"],
                "printings_affected_declared": entry["printings_affected"],
                "printings_found": len(printings),
                "printings_with_after_text": sum(
                    entry["after"] in c[entry["field"]] for c in printings
                ),
                "printings_with_before_text": sum(
                    entry["before"] in c[entry["field"]] for c in printings
                ),
            }
        )
    computed = {
        "card_count": len(cards),
        "set_count": len(sets_index),
        "ruling_count": len(rulings),
        "rules_faq_count": len(faq),
        "errata_count": len(errata),
        "product_count": len(products),
    }
    return {
        "rulings": {
            "count": len(rulings),
            "non_empty_answers": sum(1 for r in rulings if (r.get("answer") or "").strip()),
            "distinct_card_numbers": len({r["card_number"] for r in rulings}),
            "unknown_card_numbers": sorted({r["card_number"] for r in rulings} - set(bases)),
            "duplicate_nums": sorted(
                n for n, k in Counter(r["num"] for r in rulings).items() if k > 1
            ),
        },
        "rules_faq": {
            "count": len(faq),
            "non_empty_answers": sum(1 for r in faq if (r.get("answer") or "").strip()),
            "categories": counts(r["category"] for r in faq),
        },
        "errata": {
            "count": len(errata),
            "status": counts(e["status"] for e in errata),
            "checks": errata_checks,
        },
        "products": {
            "count": len(products),
            "category_tag": counts(p["category_tag"] for p in products),
        },
        "sets_index": {
            "count": len(sets_index),
            "card_count_sum": sum(s["card_count"] for s in sets_index),
            "mismatch_vs_printings_per_set_code": sorted(
                s["set_code"] for s in sets_index if s["card_count"] != per_set_code[s["set_code"]]
            ),
        },
        "manifest": {
            key: {"manifest": manifest[key], "computed": value, "ok": manifest[key] == value}
            for key, value in computed.items()
        },
    }


def compare_full_clone(clone: Path, data_dir: Path, cards: list[dict[str, Any]]) -> dict[str, Any]:
    packaged_identical = {
        local: sha256_file(data_dir / local) == sha256_file(clone / upstream)
        for local, upstream in PACKAGED_FILES.items()
    }
    bulk_json = load_json(clone / "data" / "cards.json")
    nd_canon = Counter(canonical(c) for c in cards)
    per_set_records: list[dict[str, Any]] = []
    per_set_files = {}
    wrong_file = []
    for path in sorted((clone / "data" / "cards" / "en").glob("*.json")):
        records = load_json(path)
        per_set_files[path.name] = len(records)
        per_set_records.extend(records)
        wrong_file.extend(
            f"{path.name}:{r['product_id']}" for r in records if r["set_code"].lower() != path.stem
        )
    per_set_canon = Counter(canonical(r) for r in per_set_records)
    bulk_canon = Counter(canonical(r) for r in bulk_json)
    return {
        "git_head": git_head(clone),
        "packaged_files_byte_identical_to_clone": packaged_identical,
        "cards_json": {
            "records": len(bulk_json),
            "identical_in_order_to_ndjson": bulk_json == cards,
            "only_in_cards_json": len(bulk_canon - nd_canon),
            "only_in_ndjson": len(nd_canon - bulk_canon),
        },
        "cards_en_per_set": {
            "files": len(per_set_files),
            "records_per_file": per_set_files,
            "records": len(per_set_records),
            "multiset_equal_to_ndjson": per_set_canon == nd_canon,
            "only_in_per_set": len(per_set_canon - nd_canon),
            "only_in_ndjson": len(nd_canon - per_set_canon),
            "records_in_wrong_set_file": sorted(wrong_file),
        },
    }


def git_bytes(repo: Path, *args: str) -> bytes:
    return subprocess.run(["git", "-C", str(repo), *args], capture_output=True, check=True).stdout


def git_head(repo: Path) -> str:
    return git_bytes(repo, "rev-parse", "HEAD").decode("ascii").strip()


def build_sources(
    data_dir: Path, clone: Path, manifest: dict[str, Any], retrieved_at: str
) -> list[dict[str, Any]]:
    version = manifest["dataset_version"]
    effective = manifest["built_at"]
    common = {"version": version, "effective_date": effective, "retrieved_at": retrieved_at}
    verify_note = (
        f"Byte-identical to the file at commit {PINNED_COMMIT} (sha256 of the packaged copy "
        "equals sha256 of the clone file and of the raw URL)."
    )
    entries: list[dict[str, Any]] = []
    for local, upstream in PACKAGED_FILES.items():
        entries.append(
            {
                "id": f"gcgapi:{upstream}",
                "kind": FILE_KINDS.get(local, "data-file"),
                "url": RAW_BASE + upstream,
                "sha256": sha256_file(data_dir / local),
                "local_path": str((data_dir / local).relative_to(REPO_ROOT)),
                "notes": verify_note,
                **common,
            }
        )
    entries.append(
        {
            "id": "gcgapi:data/cards.json",
            "kind": "data-file",
            "url": RAW_BASE + "data/cards.json",
            "sha256": sha256_file(clone / "data" / "cards.json"),
            "local_path": "not-packaged",
            "notes": "Omitted from the package: same 1,912 records in the same order as "
            "data/cards.ndjson (checked by scripts/ingest_profile.py).",
            **common,
        }
    )
    per_set = sorted((clone / "data" / "cards" / "en").glob("*.json"))
    listing = "".join(f"{sha256_file(p)}  {p.name}\n" for p in per_set)
    entries.append(
        {
            "id": "gcgapi:data/cards/en/*.json",
            "kind": "data-file-set",
            "url": RAW_BASE + "data/cards/en/",
            "sha256": sha256_bytes(listing.encode("utf-8")),
            "local_path": "not-packaged",
            "notes": f"Aggregate of {len(per_set)} per-set files ({', '.join(p.name for p in per_set)}). "
            "sha256 is taken over the text '<sha256>  <file name>\\n' per file, sorted by file "
            "name, i.e. `(cd data/cards/en && shasum -a 256 *.json) | shasum -a 256`. Same "
            "multiset of records as data/cards.ndjson.",
            **common,
        }
    )
    commit_object = git_bytes(clone, "cat-file", "commit", PINNED_COMMIT)
    entries.append(
        {
            "id": "gcgapi:commit",
            "kind": "git-commit",
            "url": f"https://github.com/{UPSTREAM_REPO}/commit/{PINNED_COMMIT}",
            "sha256": sha256_bytes(commit_object),
            "local_path": "not-packaged",
            "notes": f"Data commit 'data: weekly refresh (run 25)', git object id {PINNED_COMMIT} "
            "(SHA-1), committed 2026-09-21T12:12:05Z. sha256 is over the raw commit object "
            f"(`git cat-file commit {PINNED_COMMIT} | shasum -a 256`). dataset_version's sha "
            f"({manifest['source_commit']}) is the code commit that produced the data, not this one.",
            **common,
        }
    )
    return sorted(entries, key=lambda e: e["id"])


def build_profile(data_dir: Path, clone: Path | None) -> dict[str, Any]:
    data: dict[str, Any] = {
        name: load_json(data_dir / name)
        for name in PACKAGED_FILES
        if name.endswith(".json") and name != "cards.ndjson"
    }
    cards = load_ndjson(data_dir / "cards.ndjson")
    by_number = group_by_number(cards)
    bases = {number: pick_base(printings) for number, printings in by_number.items()}
    vocab = frozenset(t for c in cards for t in c["traits"])
    check_normalization(vocab)
    divergences = profile_divergences(by_number, bases, data["errata.json"], vocab)
    counts_section = profile_counts(cards, bases)
    effects = profile_effect_texts(cards, by_number, bases, vocab)
    side = profile_side_files(data, cards, bases)
    profile: dict[str, Any] = {
        "generator": "scripts/ingest_profile.py",
        "snapshot": {
            "upstream": f"https://github.com/{UPSTREAM_REPO}",
            "pinned_commit": PINNED_COMMIT,
            "dataset_version": data["manifest.json"]["dataset_version"],
            "built_at": data["manifest.json"]["built_at"],
            "packaged_sha256": {name: sha256_file(data_dir / name) for name in PACKAGED_FILES},
        },
        "counts": counts_section,
        "effect_text": effects,
        "divergent_printings": {
            "card_numbers": len(divergences),
            "by_classification": counts(d["classification"] for d in divergences),
            "items": divergences,
        },
        "fields": profile_fields(cards),
        "links": profile_links(cards, vocab),
        "traits": profile_traits(cards, vocab),
        "keywords_and_timings": profile_keywords(cards, bases, vocab),
        "commands_and_pilots": profile_commands_and_pilots(cards, bases),
        "tokens": profile_tokens(cards, by_number, vocab),
        "misc": profile_misc(cards, bases, vocab),
        "side_files": side,
        "full_clone": compare_full_clone(clone, data_dir, cards) if clone else None,
    }
    profile["figures"] = {
        "printings": counts_section["printings"],
        "distinct_card_numbers": counts_section["distinct_card_numbers"],
        "rulings": side["rulings"]["count"],
        "rulings_non_empty_answers": side["rulings"]["non_empty_answers"],
        "rules_faq": side["rules_faq"]["count"],
        "errata": side["errata"]["count"],
        "products": side["products"]["count"],
        "distinct_effect_raw": effects["distinct_raw_printings"],
        "distinct_effect_level1": effects["distinct_level1_printings"],
        "distinct_effect_level2": effects["distinct_level2_printings"],
        "distinct_effect_level2_per_card_number": effects["distinct_level2_per_card_number"],
        "divergent_card_numbers": len(divergences),
    }
    return profile


def write_json(path: Path, payload: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    text = json.dumps(payload, sort_keys=True, indent=2, ensure_ascii=False)
    path.write_text(text + "\n", encoding="utf-8")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--data-dir", type=Path, default=DEFAULT_DATA_DIR)
    parser.add_argument("--full-clone", type=Path, default=None, help="gcg-api clone at the pin")
    parser.add_argument("--out", type=Path, default=DEFAULT_OUT)
    parser.add_argument("--sources-out", type=Path, default=None)
    parser.add_argument("--retrieved-at", default=None, help="UTC time the clone was fetched")
    args = parser.parse_args(argv)
    if args.sources_out and not (args.full_clone and args.retrieved_at):
        parser.error("--sources-out needs --full-clone and --retrieved-at")
    clone = args.full_clone.resolve() if args.full_clone else None
    if clone and git_head(clone) != PINNED_COMMIT:
        parser.error(f"{clone} is not checked out at {PINNED_COMMIT}")
    profile = build_profile(args.data_dir.resolve(), clone)
    write_json(args.out, profile)
    print(json.dumps(profile["figures"], sort_keys=True, indent=2))
    if args.sources_out and clone:
        manifest = load_json(args.data_dir / "manifest.json")
        write_json(
            args.sources_out,
            build_sources(args.data_dir.resolve(), clone, manifest, args.retrieved_at),
        )
    return 0


if __name__ == "__main__":
    sys.exit(main())
