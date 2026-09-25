"""Card database loaded from the packaged gcg-api snapshot plus reviewable overrides."""

from __future__ import annotations

import json
from collections.abc import Iterator, Mapping
from dataclasses import dataclass
from functools import cache
from importlib import resources
from typing import Any

from gcg_sim.cards.model import (
    CardDef,
    CardType,
    Color,
    parse_command_pilot_name,
    parse_link,
    parse_modifier,
)
from gcg_sim.cards.tokens import TokenSpec, parse_token_specs

DATA_PACKAGE = "gcg_sim.data"
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
)

EX_BASE_NUMBER = "EXB-001"
EX_RESOURCE_NUMBER = "EXR-001"


def read_data_text(*parts: str) -> str:
    node = resources.files(DATA_PACKAGE)
    for p in parts:
        node = node.joinpath(p)
    return node.read_text(encoding="utf-8")


def load_raw_printings(text: str | None = None) -> list[dict[str, Any]]:
    raw = text if text is not None else read_data_text("gcgapi", "cards.ndjson")
    return [json.loads(line) for line in raw.splitlines() if line.strip()]


def load_overrides(text: str | None = None) -> dict[str, Any]:
    if text is None:
        try:
            text = read_data_text("overrides.json")
        except FileNotFoundError:
            return {"schema_version": 1, "policies": {}, "resolutions": {}}
    data: dict[str, Any] = json.loads(text)
    return data


@dataclass(frozen=True, slots=True)
class AppliedOverride:
    conflict_id: str
    card_number: str
    decision: str
    fields: tuple[str, ...]


def _card_override_index(
    overrides: Mapping[str, Any],
) -> dict[str, list[tuple[str, dict[str, Any]]]]:
    """Map card_number → [(conflict_id, resolution)] for resolutions that change card data."""
    out: dict[str, list[tuple[str, dict[str, Any]]]] = {}
    for cid, res in sorted(overrides.get("resolutions", {}).items()):
        if not isinstance(res, dict):
            continue
        if not (res.get("canonical_product_id") or res.get("field_overrides")):
            continue
        numbers = res.get("card_numbers") or (
            [res["card_number"]] if res.get("card_number") else []
        )
        if not numbers and ":" in cid:
            numbers = [cid.split(":")[1]]
        for n in numbers:
            out.setdefault(n, []).append((cid, res))
    return out


def _choose_canonical(printings: list[dict[str, Any]], preferred: str | None) -> dict[str, Any]:
    if preferred is not None:
        for p in printings:
            if p["product_id"] == preferred:
                return p
        raise ValueError(f"override names unknown canonical printing {preferred!r}")
    for p in printings:
        if p["product_id"] == p["card_number"]:
            return p
    return sorted(printings, key=lambda p: p["product_id"])[0]


def _to_int(v: Any) -> int:
    return int(v) if isinstance(v, int) else 0


class CardDB:
    """Immutable lookup of :class:`CardDef` by def_id, card number, or product id."""

    def __init__(self, printings: list[dict[str, Any]], overrides: Mapping[str, Any]) -> None:
        by_number: dict[str, list[dict[str, Any]]] = {}
        for p in printings:
            by_number.setdefault(p["card_number"], []).append(p)
        override_index = _card_override_index(overrides)
        self.applied_overrides: list[AppliedOverride] = []
        self._defs: list[CardDef] = []
        self._by_number: dict[str, CardDef] = {}
        self._product_to_number: dict[str, str] = {}
        for number in sorted(by_number):
            ps = sorted(by_number[number], key=lambda p: p["product_id"])
            preferred: str | None = None
            field_overrides: dict[str, Any] = {}
            for cid, res in override_index.get(number, []):
                if res.get("canonical_product_id"):
                    preferred = str(res["canonical_product_id"])
                fo = res.get("field_overrides") or {}
                if isinstance(fo, dict) and number in fo and isinstance(fo[number], dict):
                    fo = fo[number]
                field_overrides.update(fo)
                self.applied_overrides.append(
                    AppliedOverride(
                        conflict_id=cid,
                        card_number=number,
                        decision=str(res.get("decision", res.get("policy", ""))),
                        fields=tuple(sorted(fo)) if isinstance(fo, dict) else (),
                    )
                )
            canon = dict(_choose_canonical(ps, preferred))
            canon.update(field_overrides)
            cdef = self._make_def(len(self._defs), canon, tuple(p["product_id"] for p in ps))
            self._defs.append(cdef)
            self._by_number[number] = cdef
            for p in ps:
                self._product_to_number[p["product_id"]] = number
        self._token_by_key: dict[str, CardDef] = {}
        self._register_inline_tokens()

    @staticmethod
    def _make_def(def_id: int, rec: dict[str, Any], product_ids: tuple[str, ...]) -> CardDef:
        ctype = CardType(rec["card_type"])
        color = Color(rec["color"]) if rec.get("color") else None
        effect = str(rec.get("effect") or "-")
        pilot_name = parse_command_pilot_name(effect) if ctype is CardType.COMMAND else None
        if ctype is CardType.PILOT:
            pilot_name = str(rec["name"])
        if ctype is CardType.PILOT or (ctype is CardType.COMMAND and pilot_name):
            ap = parse_modifier(rec.get("ap_raw"))
            hp = parse_modifier(rec.get("hp_raw"))
        else:
            ap = _to_int(rec.get("ap"))
            hp = _to_int(rec.get("hp"))
        zone_raw = rec.get("zone") or "-"
        zones = tuple(z for z in str(zone_raw).split() if z != "-")
        link = parse_link(rec.get("link")) if ctype.is_unit else None
        return CardDef(
            def_id=def_id,
            card_number=str(rec["card_number"]),
            name=str(rec["name"]),
            card_type=ctype,
            color=color,
            level=_to_int(rec.get("level")),
            cost=_to_int(rec.get("cost")),
            ap=ap,
            hp=hp,
            zones=zones,
            traits=tuple(rec.get("traits") or ()),
            link=link,
            effect=effect,
            pilot_name=pilot_name,
            set_code=str(rec.get("set_code") or ""),
            rarity=str(rec.get("rarity") or ""),
            product_ids=product_ids,
            canonical_product_id=str(rec["product_id"]),
            source_title=rec.get("source_title"),
        )

    def _register_inline_tokens(self) -> None:
        """Map every inline token definition to a T- card when name/stats/traits match,
        otherwise register a synthetic token definition (rule 5-17)."""
        t_cards = [d for d in self._defs if d.card_type is CardType.UNIT_TOKEN]
        specs: dict[str, TokenSpec] = {}
        for d in self._defs:
            for s in parse_token_specs(d.effect):
                specs.setdefault(s.key, s)
        for key in sorted(specs):
            s = specs[key]
            match = [
                t
                for t in t_cards
                if t.name == s.name
                and t.ap == s.ap
                and t.hp == s.hp
                and set(t.traits) == set(s.traits)
            ]
            if match:
                self._token_by_key[key] = sorted(match, key=lambda d: d.card_number)[0]
                continue
            synth = CardDef(
                def_id=len(self._defs),
                card_number=f"TOKEN:{s.name}:{s.ap}/{s.hp}",
                name=s.name,
                card_type=CardType.UNIT_TOKEN,
                color=None,
                level=0,
                cost=0,
                ap=s.ap,
                hp=s.hp,
                zones=(),
                traits=s.traits,
                link=None,
                effect=s.text or "-",
                pilot_name=None,
                set_code="TOKEN",
                rarity="",
                product_ids=(),
                canonical_product_id="",
            )
            self._defs.append(synth)
            self._token_by_key[key] = synth

    def __len__(self) -> int:
        return len(self._defs)

    def __iter__(self) -> Iterator[CardDef]:
        return iter(self._defs)

    def by_id(self, def_id: int) -> CardDef:
        return self._defs[def_id]

    def get(self, card_number_or_product_id: str) -> CardDef | None:
        key = card_number_or_product_id.strip()
        if key in self._by_number:
            return self._by_number[key]
        number = self._product_to_number.get(key)
        return self._by_number[number] if number else None

    def __getitem__(self, card_number_or_product_id: str) -> CardDef:
        d = self.get(card_number_or_product_id)
        if d is None:
            raise KeyError(card_number_or_product_id)
        return d

    def normalize_id(self, card_number_or_product_id: str) -> str | None:
        d = self.get(card_number_or_product_id)
        return d.card_number if d else None

    def real_cards(self) -> list[CardDef]:
        """Every card number present in the snapshot (excludes synthetic inline tokens)."""
        return [d for d in self._defs if not d.card_number.startswith("TOKEN:")]

    def token_for(self, spec: TokenSpec) -> CardDef:
        return self._token_by_key[spec.key]

    @property
    def ex_base(self) -> CardDef:
        return self._by_number[EX_BASE_NUMBER]

    @property
    def ex_resource(self) -> CardDef:
        return self._by_number[EX_RESOURCE_NUMBER]


@cache
def get_card_db() -> CardDB:
    return CardDB(load_raw_printings(), load_overrides())
