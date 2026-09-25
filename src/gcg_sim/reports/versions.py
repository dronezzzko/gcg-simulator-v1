"""Versions of everything a result depends on, and the conflict resolutions touching a deck."""

from __future__ import annotations

import json
from collections.abc import Iterable, Mapping
from functools import cache
from importlib import metadata
from typing import Any

from gcg_sim.cards.db import get_card_db, load_overrides, read_data_text

PACKAGE = "gcg-sim"
SCHEMA_VERSION = "1.0.0"
SCHEMA_ID = f"urn:gcg-sim:schema:results:{SCHEMA_VERSION}"


def _data_json(*parts: str) -> dict[str, Any]:
    data: dict[str, Any] = json.loads(read_data_text(*parts))
    return data


def package_version() -> str:
    try:
        return metadata.version(PACKAGE)
    except metadata.PackageNotFoundError:
        return "unknown"


@cache
def versions() -> dict[str, Any]:
    manifest = _data_json("gcgapi", "manifest.json")
    rules = _data_json("official", "rules_version.json")
    return {
        "package": package_version(),
        "results_schema": SCHEMA_VERSION,
        "dataset_version": manifest["dataset_version"],
        "data_source_commit": manifest["source_commit"],
        "data_built_at": manifest["built_at"],
        "rules_version": rules["latest_version"],
        "rules_effective_date": rules["effective_date"],
        "banlist_effective_date": _data_json("official", "banlist.json")["effective_date"],
        "deck_rules_effective_date": _data_json("official", "deck_construction.json")[
            "effective_date"
        ],
        "bo3_rules_effective_date": _data_json("official", "bo3_match_rules.json")[
            "effective_date"
        ],
    }


def manifest() -> dict[str, Any]:
    return _data_json("gcgapi", "manifest.json")


def _resolution_cards(cid: str, res: Mapping[str, Any]) -> set[str]:
    numbers = set(res.get("card_numbers") or ())
    if res.get("card_number"):
        numbers.add(str(res["card_number"]))
    numbers.update(cid.split(":")[1:])
    return numbers


def _entry(
    number: str, cid: str, decks: list[str], decision: str, fields: list[str], changes: bool
) -> dict[str, Any]:
    return {
        "card_number": number,
        "name": get_card_db()[number].name,
        "conflict_id": cid,
        "decks": sorted(decks),
        "decision": decision,
        "fields": fields,
        "changes_card_data": changes,
    }


def conflict_resolutions(decks: Mapping[str, Iterable[str]]) -> list[dict[str, Any]]:
    """Resolutions from ``overrides.json`` that touch a card in ``decks`` (label -> card
    numbers): card-data overrides applied at load time (``CardDB.applied_overrides``) and
    rulings/interpretations whose conflict names one of the cards."""
    in_decks: dict[str, list[str]] = {}
    for label, numbers in decks.items():
        for n in set(numbers):
            in_decks.setdefault(n, []).append(label)
    out: dict[tuple[str, str], dict[str, Any]] = {}
    for o in get_card_db().applied_overrides:
        if o.card_number in in_decks:
            key = (o.card_number, o.conflict_id)
            out[key] = _entry(*key, in_decks[o.card_number], o.decision, list(o.fields), True)
    for cid, res in sorted(load_overrides().get("resolutions", {}).items()):
        if not isinstance(res, Mapping):
            continue
        for n in sorted(_resolution_cards(cid, res) & in_decks.keys()):
            decision = str(res.get("decision", ""))
            out.setdefault((n, cid), _entry(n, cid, in_decks[n], decision, [], False))
    return [out[k] for k in sorted(out)]
