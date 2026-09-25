"""Canonical JSON rendering of compiled card scripts for golden tests.

Golden files freeze each card's compiled behaviour together with a hash of its normalized
text, so a data refresh that changes wording (or a compiler change that changes output)
fails loudly until the card is reviewed.
"""

from __future__ import annotations

import dataclasses
import enum
from typing import Any

from gcg_sim.cards.model import CardDef
from gcg_sim.effects.registry import CardEntry
from gcg_sim.effects.text import text_hash


def node_to_json(x: Any) -> Any:
    if dataclasses.is_dataclass(x) and not isinstance(x, type):
        out: dict[str, Any] = {"_": type(x).__name__}
        for f in dataclasses.fields(x):
            v = getattr(x, f.name)
            default: Any = f.default
            if f.default_factory is not dataclasses.MISSING:
                default = f.default_factory()
            if default is not dataclasses.MISSING and v == default:
                continue
            out[f.name] = node_to_json(v)
        return out
    if isinstance(x, enum.Enum):
        return x.value
    if isinstance(x, (tuple, list)):
        return [node_to_json(v) for v in x]
    if isinstance(x, dict):
        return {str(k): node_to_json(v) for k, v in sorted(x.items())}
    return x


def card_golden(cdef: CardDef, entry: CardEntry, binding_module: str | None) -> dict[str, Any]:
    rec: dict[str, Any] = {"name": cdef.name, "text_hash": text_hash(cdef.effect)}
    if entry.script is None:
        rec["error"] = entry.error
        return rec
    rec["source"] = "binding" if binding_module else entry.script.source
    if binding_module:
        rec["binding"] = binding_module.rsplit(".", 1)[-1]
    rec["abilities"] = node_to_json(entry.script.abilities)
    if entry.script.unit_abilities:
        rec["unit_abilities"] = node_to_json(entry.script.unit_abilities)
    return rec


def prefix_of(card_number: str) -> str:
    return card_number.split("-", 1)[0]
