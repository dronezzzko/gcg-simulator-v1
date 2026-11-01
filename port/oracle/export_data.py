"""Export the Python data layer's outputs for the Rust gcg-data tests (docs/port/PLAN.md §10.4 A).

Writes four fixtures under ``crates/gcg-data/tests/vectors/``:

- ``carddefs.json``: every ``CardDef`` field by field (1153), the four fingerprints of
  Inv 03 §9, the token map, the applied overrides, the Unit variants, ``db.get`` and
  ``resolve_card_number`` lookups, and ``parse_token_specs`` on every effect text plus edge
  cases (T-035);
- ``synthetic.json``: ``CardDB`` built from small synthetic sources, one per loader path that
  the packaged data never takes (W-164) plus fatal load errors (T-035);
- ``decks.json``: deck texts, deck files (raw bytes) and directly constructed decks with
  Python's parse outcome, every validation variant and ``require_legal`` text; every violation
  code occurs (T-036);
- ``status.json``: ``data status`` text and ``--json`` output, ``versions()``,
  ``conflict_resolutions`` for several deck pairs, and a ``code_digest`` vector (T-038, T-039).

Nothing here changes the package: patches of ``validate.get_registry`` and
``official.banlist`` are local to one call and undone in ``finally``.

    uv run python port/oracle/export_data.py [--check]
"""

from __future__ import annotations

import argparse
import dataclasses
import hashlib
import io
import json
import sys
import tempfile
from collections import Counter
from collections.abc import Callable, Iterator
from pathlib import Path
from types import SimpleNamespace
from typing import Any

from gcg_sim import cli
from gcg_sim.cards.db import CardDB, get_card_db, load_overrides, load_raw_printings
from gcg_sim.cards.model import CardDef
from gcg_sim.cards.tokens import parse_token_specs
from gcg_sim.deck import Deck, DeckEntry, DeckError, official, parse_deck, validate_deck
from gcg_sim.deck import validate as validate_module
from gcg_sim.deck.parse import deck_digest, load_deck, resolve_card_number
from gcg_sim.deck.validate import require_legal
from gcg_sim.effects.registry import get_registry
from gcg_sim.reports import versions as versions_module
from gcg_sim.rng import SplitMix64

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "crates" / "gcg-data" / "tests" / "vectors"
EXAMPLES = ROOT / "examples" / "decks"

Json = dict[str, Any]


# --- card definitions ------------------------------------------------------------------------


def dump_record(d: CardDef) -> list[Any]:
    """The Inv 03 §9 fingerprint record."""
    link = None if d.link is None else [list(d.link.name_parts), list(d.link.traits)]
    return [
        d.def_id,
        d.card_number,
        d.name,
        d.card_type.value,
        d.color.value if d.color else None,
        d.level,
        d.cost,
        d.ap,
        d.hp,
        list(d.zones),
        list(d.traits),
        link,
        d.effect,
        d.pilot_name,
        d.set_code,
        d.rarity,
        list(d.product_ids),
        d.canonical_product_id,
        d.source_title,
    ]


def def_row(d: CardDef) -> list[Any]:
    return [
        *dump_record(d),
        list(d.extra_names),
        list(d.names),
        d.is_token,
        d.is_unit_card,
        d.is_pilot_capable,
        d.is_vanilla,
    ]


def fingerprints(db: CardDB) -> list[str]:
    lines = [
        json.dumps(dump_record(d), ensure_ascii=False, separators=(",", ":")) + "\n" for d in db
    ]
    tokens = sorted(db._token_by_key.items())
    return [
        hashlib.sha256("".join(lines).encode()).hexdigest(),
        hashlib.sha256("".join(f"{d.def_id}\t{d.card_number}\n" for d in db).encode()).hexdigest(),
        hashlib.sha256("".join(f"{k}\t{v.card_number}\n" for k, v in tokens).encode()).hexdigest(),
        hashlib.sha256(
            "".join(
                f"{o.conflict_id}\t{o.card_number}\t{o.decision}\t{','.join(o.fields)}\n"
                for o in db.applied_overrides
            ).encode()
        ).hexdigest(),
    ]


def db_outline(db: CardDB) -> Json:
    """Everything a synthetic database is compared on."""
    return {
        "defs": [def_row(d) for d in db],
        "fingerprints": fingerprints(db),
        "tokens": [[k, v.def_id] for k, v in sorted(db._token_by_key.items())],
        "applied_overrides": [
            [o.conflict_id, o.card_number, o.decision, list(o.fields)] for o in db.applied_overrides
        ],
        "unit_variants": [
            [d.def_id, v.def_id] for d in db if (v := db.unit_variant(d.def_id)) is not None
        ],
    }


TOKEN_EDGE_CASES = [
    "[Zaku Ⅱ]((Zeon)･AP1･HP1)",
    "[Zaku Ⅱ]((Zeon)・AP1・HP1)",
    "[A]((X) (Y)･AP2･HP3)",
    "[A]((X)  (Y) (Z)･AP2･HP3･text (with) parens.)",
    "[A]((X)･AP１･HP２)",
    "[A]((X) ･ AP 1 ･ HP 1)",
    "[A]((X)･AP1 ･HP1 ･  Some  text...  )",
    "[A]((X)･AP1･HP1･a (b (c) d) e)",
    "[A]((X)･AP1･HP1･(a)(b))",
    "[A]((X)･AP1･HP1･)",
    "[A](()･AP1･HP1)",
    "[A]((X)･AP･HP1)",
    "[[A]]((X)･AP1･HP1)",
    "[A] ((X)･AP1･HP1)",
    "[A]((X)･AP1･HP1",
    "[ A ](( X )･AP1･HP1) and [B]((Y)･AP2･HP2･Blocker・x)",
    "[A]((X) (Y)･AP1･HP1) [A]((X) (Y)･AP1･HP1)",
    "[A]((X)) (Y)･AP1･HP1)",
    "[A]((X) (Y･AP1･HP1)",
    "[A]((X)･AP1･HP1･t1) [B]((Z)･AP0･HP0)",
    "[A]((X)･AP001･HP010)",
    "[A]((X)･AP1･HP1･a\N{IDEOGRAPHIC SPACE}b\xa0c\N{LINE SEPARATOR}d.)",
]

LOOKUP_EXTRA = [
    "",
    " ",
    " GD01-001 ",
    "\N{IDEOGRAPHIC SPACE}GD01-001\x1f",
    "gd01-001",
    "GD01-001_P1",
    "UNIT:GD05-089:3/3",
    "EXB-001_p7",
    "EXB-001_P7",
    "R-001_p1",
    "T-006",
    "TOKEN:Zaku Ⅱ:1/1",
    "XX-999",
]

RESOLVE_IDS = [
    "GD01-001",
    "gd01-001",
    "Gd01-001_P1",
    "GD01-001_p1",
    "GD01-001_P2",
    "GD01-001_p9",
    "GD01-001_p99",
    "GD01-001_q1",
    "GD01-001__p1",
    "GD01-001_p",
    "GD01-001_p1_p2",
    "exb-001_P7",
    "exb-001",
    "R-001_p1",
    "r-001",
    "rp-001",
    "ſt01-001",
    "ſT01-001_P1",
    "gd01-00ı",
    "GD01-001\N{ZERO WIDTH SPACE}",
    "UNIT:GD05-089:3/3",
    "TOKEN:Zaku Ⅱ:1/1",
    "T-006",
    "t-006_p1",
    "_p1",
    "A_p1_p2",
    "ＧＤ01-001",
]


def carddefs() -> Json:
    db = get_card_db()
    lookups = []
    queries = [d.card_number for d in db] + [p for d in db for p in d.product_ids] + LOOKUP_EXTRA
    for q in queries:
        d = db.get(q)
        lookups.append([q, None if d is None else d.def_id, db.normalize_id(q)])
    rng = SplitMix64(31337)
    real = [d.card_number for d in db.real_cards()]
    resolve = list(RESOLVE_IDS)
    for _ in range(300):
        n = real[rng.randrange(len(real))]
        variant = rng.randrange(4)
        if variant == 0:
            resolve.append(n.lower())
        elif variant == 1:
            resolve.append(f"{n}_p{rng.randrange(12)}")
        elif variant == 2:
            resolve.append(f"{n.lower()}_P{rng.randrange(3)}")
        else:
            resolve.append(n[: rng.randrange(len(n))] + n[rng.randrange(len(n)) :])
    specs = []
    for d in db:
        found = parse_token_specs(d.effect)
        if found:
            specs.append([d.effect, [[s.key, s.ap, s.hp] for s in found]])
    for text in TOKEN_EDGE_CASES:
        specs.append([text, [[s.key, s.ap, s.hp] for s in parse_token_specs(text)]])
    outline = db_outline(db)
    return {
        "len": len(db),
        "real_cards": len(db.real_cards()),
        "ex_base": db.ex_base.def_id,
        "ex_resource": db.ex_resource.def_id,
        **outline,
        "lookups": lookups,
        "resolve": [[w, resolve_card_number(w, db)] for w in resolve],
        "token_specs": specs,
    }


# --- synthetic sources (W-164, W-157, fatal load errors) -------------------------------------


def printing(number: str, product: str | None = None, **fields: Any) -> Json:
    rec: Json = {
        "product_id": product or number,
        "card_number": number,
        "name": fields.pop("name", f"Card {number}"),
        "set_code": number.split("-", maxsplit=1)[0],
        "rarity": "C",
        "card_type": "UNIT",
        "color": "Blue",
        "level": 1,
        "cost": 1,
        "ap": 1,
        "hp": 1,
        "ap_raw": "1",
        "hp_raw": "1",
        "zone": "Space Earth",
        "link": "-",
        "traits": ["Earth Federation"],
        "source_title": "Synthetic",
        "effect": "-",
    }
    rec.update(fields)
    return rec


def token(number: str, name: str, ap: int, hp: int, traits: list[str], **fields: Any) -> Json:
    return printing(
        number,
        name=name,
        card_type="UNIT TOKEN",
        color=None,
        level=None,
        cost=None,
        ap=ap,
        hp=hp,
        traits=traits,
        **fields,
    )


def ndjson(recs: list[Json]) -> str:
    return "".join(json.dumps(r, ensure_ascii=False) + "\n" for r in recs)


def overrides(resolutions: Json) -> str:
    return json.dumps({"schema_version": 1, "policies": {}, "resolutions": resolutions})


def synthetic_cases() -> list[tuple[str, str, str, list[str], str]]:
    """``(id, cards.ndjson, overrides.json, dead paths taken, Rust error variant or "")``."""
    deploy = "【Deploy】Deploy 1 [Ghost]((Phantom)･AP2･HP2･Can't attack) token."
    tie = "【Deploy】Deploy 1 [Twin]((Mirror)･AP1･HP1) token."
    base = [printing("SY01-001"), printing("SY01-002")]
    cases: list[tuple[str, str, str, list[str], str]] = [
        ("baseline", ndjson(base), overrides({}), [], ""),
        (
            "synthetic-token",
            ndjson(
                [
                    base[0],
                    printing("SY01-002", effect=deploy),
                    token("T-900", "Ghost", 2, 2, ["Other"]),
                ]
            ),
            overrides({}),
            ["synthetic_token"],
            "",
        ),
        (
            "token-tie",
            ndjson(
                [
                    printing("SY01-003", effect=tie),
                    token("T-902", "Twin", 1, 1, ["Mirror", "Mirror"]),
                    token("T-901", "Twin", 1, 1, ["Mirror"], effect="Different text."),
                ]
            ),
            overrides({}),
            ["token_tie"],
            "",
        ),
        (
            "nested-field-overrides",
            ndjson(base),
            overrides(
                {
                    "fix:SY01-001": {
                        "policy": "p",
                        "decision": "nested",
                        "card_numbers": ["SY01-001", "SY01-002"],
                        "field_overrides": {"SY01-001": {"ap": 7, "name": "Renamed"}, "hp": 3},
                    }
                }
            ),
            ["nested_field_overrides"],
            "",
        ),
        (
            "singular-card-number",
            ndjson(base),
            overrides(
                {
                    "fix-without-number": {
                        "policy": "only policy",
                        "card_number": "SY01-002",
                        "field_overrides": {"effect": "Patched."},
                    }
                }
            ),
            ["singular_card_number"],
            "",
        ),
        (
            "id-inference-and-canonical",
            ndjson(
                [
                    printing("SY01-001", "SY01-001_p1", name="Alt"),
                    printing("SY01-001", "SY01-001_p2", name="Alt 2"),
                    printing("SY01-004"),
                ]
            ),
            overrides(
                {
                    "divergent:SY01-001:x": {
                        "policy": "p",
                        "decision": "use p2",
                        "canonical_product_id": "SY01-001_p2",
                    },
                    "a:SY01-001": {"decision": None, "field_overrides": {"cost": 5}},
                    "z:SY01-001": {"field_overrides": {"cost": 6}},
                    "skipped:SY01-004": {"decision": "no data change"},
                    "not-a-dict": [1, 2],
                }
            ),
            [],
            "",
        ),
        (
            "unit-variant-and-pilots",
            ndjson(
                [
                    printing(
                        "SY02-001",
                        card_type="PILOT",
                        ap_raw="+2",
                        hp_raw="-",
                        effect="If you deploy it as a (AP03・HP1) Unit, draw 1.",
                    ),
                    printing(
                        "SY02-002",
                        card_type="COMMAND",
                        ap=None,
                        hp=None,
                        ap_raw="+1",
                        hp_raw=None,
                        effect="【Pilot】 [ Kira ]\n【Main】Draw 1.",
                        link="[Nobody]",
                    ),
                    printing(
                        "SY02-003",
                        link="[A] / [B] (Zeon) Trait",
                        zone="-",
                        traits=None,
                        level=True,
                        cost=2.0,
                    ),
                    printing("SY02-004", card_type="UNIT", effect="deploy it as an (AP3･HP3) Unit"),
                ]
            ),
            overrides({}),
            [],
            "",
        ),
        (
            "unknown-canonical-printing",
            ndjson(base),
            overrides(
                {"divergent:SY01-001": {"policy": "p", "canonical_product_id": "SY01-001_p9"}}
            ),
            [],
            "UnknownCanonicalPrinting",
        ),
        (
            "unknown-card-type",
            ndjson([printing("SY01-001", card_type="LEADER")]),
            overrides({}),
            [],
            "UnknownCardType",
        ),
        (
            "unknown-color",
            ndjson([printing("SY01-001", color="Pink")]),
            overrides({}),
            [],
            "UnknownColor",
        ),
        (
            "unparseable-link",
            ndjson([printing("SY01-001", link="Amuro")]),
            overrides({}),
            [],
            "UnparseableLink",
        ),
        (
            "truncated-overrides",
            ndjson(base),
            overrides({})[:-3],
            [],
            "Json",
        ),
    ]
    return cases


def synthetic() -> Json:
    out = []
    for cid, cards_text, overrides_text, dead, rust_error in synthetic_cases():
        case: Json = {
            "id": cid,
            "cards_ndjson": cards_text,
            "overrides": overrides_text,
            "dead_paths": dead,
        }
        try:
            db = CardDB(load_raw_printings(cards_text), load_overrides(overrides_text))
        except (ValueError, KeyError) as exc:
            case["error"] = {"python": f"{type(exc).__name__}: {exc}", "rust": rust_error}
        else:
            assert not rust_error, cid
            case["db"] = db_outline(db)
        out.append(case)
    return {"cases": out}


# --- decks -----------------------------------------------------------------------------------


def violation_rows(vs: Any) -> list[list[Any]]:
    return [[v.code, v.message, list(v.cards)] for v in vs]


class Patched:
    """``validate.get_registry`` returning entries whose script is ``None`` for some def_ids
    (as ``tests/deck/test_validate.py:409-426`` does), restored on exit."""

    def __init__(self, errors: dict[int, str]) -> None:
        self.errors = errors
        self.saved: object = None

    def __enter__(self) -> None:
        cards = list(get_registry().cards)
        for def_id, error in self.errors.items():
            cards[def_id] = dataclasses.replace(cards[def_id], script=None, error=error)
        self.saved = vars(validate_module)["get_registry"]
        vars(validate_module)["get_registry"] = lambda: SimpleNamespace(cards=cards)

    def __exit__(self, *exc: object) -> None:
        vars(validate_module)["get_registry"] = self.saved


def deck_outline(deck: Deck) -> Json:
    return {
        "name": deck.name,
        "source": deck.source,
        "main": sorted(Counter(deck.main).items()),
        "resources": sorted(Counter(deck.resources).items()),
        "entries": [[e.count, e.card_number, e.written, e.name, e.line] for e in deck.entries],
        "digest": deck_digest(deck),
        "colors": cli._deck_colors(deck),
    }


def validations(deck: Deck, errors: dict[int, str]) -> Json:
    with Patched(errors):
        out: Json = {
            "default": violation_rows(validate_deck(deck)),
            "no_banlist": violation_rows(validate_deck(deck, banlist=False)),
            "no_implemented": violation_rows(validate_deck(deck, implemented=False)),
        }
        try:
            require_legal(deck)
        except DeckError as exc:
            out["require_legal"] = str(exc)
        else:
            out["require_legal"] = None
    return out


def error_outline(exc: DeckError) -> Json:
    return {
        "str": str(exc),
        "deck_name": exc.deck_name,
        "problems": exc.problems,
        "violations": violation_rows(exc.violations),
    }


def text_case(cid: str, text: str, name: str, errors: dict[int, str] | None = None) -> Json:
    case: Json = {"id": cid, "kind": "text", "text": text, "name": name}
    errors = errors or {}
    if errors:
        case["unimplemented"] = [[k, v] for k, v in sorted(errors.items())]
    try:
        deck = parse_deck(text, name=name, source=f"{name}.txt")
    except DeckError as exc:
        case["error"] = error_outline(exc)
        return case
    case["deck"] = deck_outline(deck)
    case["validate"] = validations(deck, errors)
    return case


def counts_text(counts: dict[str, int]) -> str:
    return "".join(f"{c} {n}\n" for n, c in counts.items() if c > 0)


def swap(counts: dict[str, int], remove: dict[str, int], add: dict[str, int]) -> dict[str, int]:
    out = dict(counts)
    for n, c in remove.items():
        out[n] -= c
    for n, c in add.items():
        out[n] = out.get(n, 0) + c
    return {n: c for n, c in out.items() if c > 0}


PARSE_ERRORS = (
    "# parse errors (Inv 03 §3.4)\n4 GD01-001\n0 GD01-002\nfoo\n4 XX-999\n"
    "1 GD01-003 Wrong Name\n1 T-006 Zaku\n1 GD01-001 Gun'dam\n4x\n4X GD01-008\n"
    "4xx GD01-008\n1 UNIT:GD05-089:3/3\n1 TOKEN:Zaku\n00 GD01-008\n"
)

TWELVE = (
    "5 GD01-001\n5 GD01-020\n3 ST02-016\n1 ST01-010\n1 ST05-010\n4 GD01-035\n1 GD01-060\n"
    "2 T-001\n1 EXB-001\n1 GD01-018\n3 R-001\n1 EXR-001\n"
)

EXOTIC = (
    "\N{ZERO WIDTH NO-BREAK SPACE}# exotic whitespace, digits and ids\n"
    "４ GD01-008 Guntank\n"
    "٣ ST01-005\x1fGM\n"
    "2x\N{IDEOGRAPHIC SPACE}GD01-004\xa0Guncannon\n"
    "1X gd01-001_P1  Gundam\n"
    "1 GD01-001\N{ZERO WIDTH SPACE}\n"
    "\N{ZERO WIDTH SPACE}1 st01-001\x0c1 ST01-002\x1c1 ST01-004\N{LINE SEPARATOR} 1 ST01-010 Amuro\xadRay\r\n"
    "1 ST03-006 Char’s  Zaku\N{ZERO WIDTH SPACE} Ⅱ\r"
    "1 GD03-027 Z´Gok E\n"
    "\u0085   # indented comment\n"
    "1 ſt01-012\n"
    "007 R-001\n"
    "3 r-001_p1 Resource\n"
)

EXOTIC_OK = (
    "\N{ZERO WIDTH NO-BREAK SPACE}# every line parses\n"
    "４ GD01-008 Guntank\n"
    "٣ ST01-005\x1fGM\n"
    "2x\N{IDEOGRAPHIC SPACE}GD01-004\xa0Guncannon\n"
    "1X gd01-001_P1  Gundam\n"
    "1 GD01-001\N{ZERO WIDTH SPACE}\n"
    "\N{ZERO WIDTH SPACE}1 st01-001\x0c1 ST01-002\x1c1 ST01-004\N{LINE SEPARATOR} 1 ST01-010 Amuro\xa0Ray\r\n"
    "1 ST03-006 Char’s  Zaku\N{ZERO WIDTH SPACE} Ⅱ\r"
    "1 GD03-027 Z’Gok E\n"
    "\u0085   # indented comment\n"
    "1 ſt01-012\n"
    "007 R-001\n"
    "3 r-001_p1 Resource\n"
    "4 GD01-008 GUNTANK # not a comment\n"
)


def legal_counts() -> dict[str, int]:
    deck = parse_deck((EXAMPLES / "blue-white-federation.txt").read_text(encoding="utf-8"))
    return dict(Counter(deck.main + deck.resources))


def crafted_cases() -> Iterator[Json]:
    base = legal_counts()
    yield text_case("parse-errors", PARSE_ERRORS, "parse_errs")
    yield text_case("twelve-violations", TWELVE, "legal_errs")
    yield text_case("exotic-input", EXOTIC, "exotic")
    yield text_case("exotic-ok", EXOTIC_OK, "exotic-ok")
    yield text_case("empty", "", "")
    yield text_case("comments-only", "# a\n\n   \n#\n", "it's")
    yield text_case("quote-names", '1 GD01-008 a"b\n', 'q"uote')
    # W-144: counts Python converts but could never expand, next to a line Python rejects (so
    # Python stops before build_deck), and the longest count int() accepts.
    yield text_case("huge-count-with-problem", f"{'9' * 30} GD01-001\n1 XX-1\n", "huge")
    yield text_case("huge-count-mismatch", f"{'9' * 25} GD01-008 Gundam\n", "huge")
    yield text_case("max-digits-with-problem", f"{'9' * 4300} GD01-001\n0 GD01-008\n", "max")
    yield text_case("zero-padded-count", f"{'0' * 4299}1 GD01-001\n", "padded")
    yield text_case("base", counts_text(base), "base")
    swaps: list[tuple[str, dict[str, int], dict[str, int]]] = [
        ("main-49", {"ST01-005": 1}, {}),
        ("main-51", {}, {"ST01-005": 1}),
        ("resources-9", {"R-001": 1}, {}),
        ("resources-11", {}, {"R-001": 1}),
        ("token-unit", {"ST01-005": 2}, {"T-001": 2}),
        ("token-ex", {"ST01-005": 1}, {"EXB-001": 1}),
        ("ex-resource", {"R-001": 1}, {"EXR-001": 1}),
        ("copies-5", {"ST01-005": 1}, {"GD01-008": 1}),
        ("copies-printings", {"GD01-008": 4}, {"GD01-001": 4}),
        ("three-colors", {"ST01-005": 3}, {"GD01-059": 1, "GD01-060": 2}),
        ("banned", {"ST01-005": 1}, {"GD01-020": 1}),
        ("restricted", {"ST01-005": 3}, {"ST02-016": 3}),
        ("banned-pair-1", {"ST01-005": 1}, {"ST05-010": 1}),
        ("banned-pair-2", {"ST01-005": 1}, {"GD05-015": 1}),
        ("attribute-pair", {"ST01-005": 3}, {"GD01-035": 2, "GD05-014": 1}),
        ("attribute-one-five", {"ST01-005": 3}, {"GD01-085": 5}),
        ("attribute-one-four", {"ST01-005": 3}, {"GD01-085": 3}),
        ("huge-count", {}, {"GD01-008": 996}),
    ]
    for cid, remove, add in swaps:
        yield text_case(cid, counts_text(swap(base, remove, add)), cid)
    yield text_case(
        "colorless",
        "4 T-001\n4 T-002\n2 EXB-001\n10 R-001\n",
        "colorless",
    )
    yield text_case(
        "unimplemented",
        counts_text(base),
        "unimplemented",
        {get_card_db()["GD01-008"].def_id: "not compiled", get_card_db()["R-001"].def_id: "x"},
    )
    for path in sorted(EXAMPLES.glob("*.txt")):
        yield text_case(f"example:{path.stem}", path.read_text(encoding="utf-8"), path.stem)


def random_line(rng: SplitMix64, pool: list[CardDef]) -> str:
    db = get_card_db()
    d = pool[rng.randrange(len(pool))]
    n = d.card_number
    roll = rng.randrange(20)
    count = (1 + rng.randrange(4)) if roll < 16 else rng.randrange(1001)
    count_text = str(count)
    if rng.randrange(10) == 0:
        count_text = "".join(chr(0xFF10 + int(c)) for c in count_text)
    if rng.randrange(8) == 0:
        count_text += "x"
    written = n
    form = rng.randrange(8)
    if form == 0:
        written = n.lower()
    elif form == 1 and len(d.product_ids) > 1:
        written = d.product_ids[rng.randrange(len(d.product_ids))]
    elif form == 2:
        written = f"{n}_P{rng.randrange(3)}"
    elif form == 3 and rng.randrange(4) == 0:
        written = n.replace("-", "")
    sep = [" ", "  ", "\t", "\N{IDEOGRAPHIC SPACE}", "\x1f", "\xa0"][rng.randrange(6)]
    name = ""
    which = rng.randrange(6)
    if which == 0:
        name = d.name
    elif which == 1:
        name = d.name.upper()
    elif which == 2 and d.pilot_name:
        name = d.pilot_name
    elif which == 3:
        other = db.by_id(rng.randrange(len(db)))
        name = other.name
    line = f"{count_text}{sep}{written}"
    if name:
        line += f"{sep}{name}"
    if rng.randrange(15) == 0:
        line = "bad " + line
    return line


def random_cases(n: int) -> Iterator[Json]:
    db = get_card_db()
    pool = list(db.real_cards())
    legal_pool = [d for d in pool if not d.is_token]
    rng = SplitMix64(20261101)
    for i in range(n):
        lines = []
        for _ in range(1 + rng.randrange(14)):
            if rng.randrange(6) == 0:
                lines.append(["# comment", "", "   ", "\t#x"][rng.randrange(4)])
            lines.append(random_line(rng, pool if rng.randrange(5) == 0 else legal_pool))
        newline = ["\n", "\r\n", "\n"][rng.randrange(3)]
        yield text_case(f"random-{i:03d}", newline.join(lines) + newline, f"random-{i:03d}")


def near_legal_cases(n: int) -> Iterator[Json]:
    """Decks built from the example decks with random swaps: mostly legal or one violation."""
    db = get_card_db()
    bases = []
    for path in sorted(EXAMPLES.glob("*.txt")):
        deck = parse_deck(path.read_text(encoding="utf-8"))
        bases.append(dict(Counter(deck.main + deck.resources)))
    pool = [d.card_number for d in db.real_cards() if not d.is_token]
    rng = SplitMix64(424242)
    for i in range(n):
        counts = dict(bases[rng.randrange(len(bases))])
        for _ in range(rng.randrange(4)):
            main = [k for k in counts if not db[k].card_type.is_resource]
            out = main[rng.randrange(len(main))]
            take = 1 + rng.randrange(2)
            counts = swap(counts, {out: min(take, counts[out])}, {})
            add = pool[rng.randrange(len(pool))]
            counts = swap(counts, {}, {add: take})
        yield text_case(f"near-{i:03d}", counts_text(counts), f"near-{i:03d}")


def constructed(cid: str, main: list[str], resources: list[str], entries: list[DeckEntry]) -> Json:
    deck = Deck(
        name=cid,
        main=tuple(main),
        resources=tuple(resources),
        entries=tuple(entries),
        source=None,
    )
    return {
        "id": cid,
        "kind": "constructed",
        "deck": {
            "name": cid,
            "main": sorted(Counter(main).items()),
            "resources": sorted(Counter(resources).items()),
            "entries": [[e.count, e.card_number, e.written, e.name, e.line] for e in entries],
            "digest": deck_digest(deck),
        },
        "validate": validations(deck, {}),
    }


def constructed_cases() -> Iterator[Json]:
    base = parse_deck(counts_text(legal_counts()), name="base")
    main, res = list(base.main), list(base.resources)
    yield constructed("unknown-main", [*main[:-1], "GD99-999"], res, list(base.entries))
    yield constructed(
        "unknown-both", [*main[:-2], "ZZ-1", "AA-1"], [*res[:-1], "ZZ-1"], list(base.entries)
    )
    yield constructed(
        "name-mismatch",
        main,
        res,
        [
            DeckEntry(count=4, card_number="GD01-008", written="GD01-008", name="Gundam", line=3),
            DeckEntry(
                count=1, card_number="ST01-010", written="st01-010", name="Amuro Ray", line=4
            ),
            DeckEntry(count=1, card_number="XX-9", written="XX-9", name="Nobody", line=5),
            DeckEntry(count=1, card_number="T-006", written="T-006", name="Zaku", line=6),
        ],
    )
    yield constructed(
        "wrong-decks", [*main[:-2], "R-001", "EXR-001"], [*res[:-2], "GD01-008", "T-001"], []
    )
    yield constructed("empty", [], [], [])


FILE_CASES: list[tuple[str, bytes]] = [
    ("my-deck.txt", "\N{ZERO WIDTH NO-BREAK SPACE}# header\n4 GD01-008 Guntank\n".encode()),
    ("crlf.txt", b"4 GD01-008\r\n2 GD01-001\r10 R-001\r\n"),
    ("bad-start.txt", b"\xef\xbb\xbf4 GD01-008\xff\n"),
    ("bad-cont.txt", b"4 GD01-008 Gun\xe2\x82tank\n"),
    ("truncated.txt", b"4 GD01-008 Gun\xe2\x82"),
    ("surrogate.txt", b"\xed\xa0\x80"),
    ("overlong.txt", b"ab\xc0\xaf"),
    ("bom-prefix.txt", b"\xef\xbb"),
    ("empty.txt", b""),
    ("b.c.txt", b"1 GD01-008\n"),
    (".hidden", b"1 GD01-008\n"),
    ("x.", b"1 GD01-008\n"),
    ("it's.txt", b"foo\n"),
    ("invalid.txt", b"0 GD01-008\n1 XX-1\n"),
]


def file_cases() -> Iterator[Json]:
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp).resolve()
        for name, data in FILE_CASES:
            (root / name).write_bytes(data)
        (root / "a-directory").mkdir()
        cases: list[tuple[str, str | None]] = [(n, d.hex()) for n, d in FILE_CASES]
        cases += [("missing.txt", None), ("a-directory", None)]
        for name, hex_bytes in cases:
            case: Json = {
                "id": f"file:{name}",
                "kind": "file",
                "filename": name,
                "bytes_hex": hex_bytes,
            }
            try:
                deck = load_deck(root / name)
            except OSError as exc:
                case["os_error"] = str(exc.strerror or exc)
            except UnicodeDecodeError as exc:
                case["decode_error"] = [exc.reason, exc.start]
            except DeckError as exc:
                case["error"] = error_outline(exc)
            else:
                outline = deck_outline(deck)
                outline["source"] = str(outline["source"]).replace(str(root), "<dir>")
                case["deck"] = outline
                case["validate"] = validations(deck, {})
            yield case


def decks() -> Json:
    cases = [
        *crafted_cases(),
        *random_cases(300),
        *near_legal_cases(120),
        *constructed_cases(),
        *file_cases(),
    ]
    seen: Counter[str] = Counter()
    for c in cases:
        for key in ("default", "no_banlist", "no_implemented"):
            for row in c.get("validate", {}).get(key, []):
                seen[row[0]] += 1
        for row in c.get("error", {}).get("violations", []):
            seen[row[0]] += 1
    missing = {
        "SYNTAX",
        "UNKNOWN_ID",
        "NAME_MISMATCH",
        "MAIN_SIZE",
        "RESOURCE_SIZE",
        "CARD_TYPE",
        "COPY_LIMIT",
        "COLORS",
        "BANNED",
        "RESTRICTED",
        "BANNED_PAIR",
        "ATTRIBUTE_PAIR",
        "UNIMPLEMENTED",
    } - set(seen)
    assert not missing, f"violation codes without a case: {sorted(missing)}"
    return {"cases": cases, "attribute_notes": attribute_notes()}


def attribute_notes() -> list[Json]:
    """``tests/deck/test_validate.py:345-386``: the membership notes, reachable only with a
    banlist whose enumeration differs from the predicate."""
    real = official.banlist()
    (rule,) = real.attribute_rules
    base = legal_counts()
    out = []
    changes: list[tuple[str, frozenset[str], dict[str, int]]] = [
        ("listed-minus", rule.enumerated - {"GD01-035"}, {"GD01-035": 1, "GD01-060": 1}),
        ("listed-plus", rule.enumerated | {"GD01-008"}, {"GD01-085": 1}),
    ]
    for cid, enumerated, add in changes:
        patched = dataclasses.replace(
            real, attribute_rules=(dataclasses.replace(rule, enumerated=enumerated),)
        )
        saved = official.banlist
        vars(official)["banlist"] = lambda patched=patched: patched
        try:
            text = counts_text(swap(base, {"ST01-005": sum(add.values())}, add))
            deck = parse_deck(text, name=cid)
            rows = violation_rows(validate_deck(deck))
        finally:
            vars(official)["banlist"] = saved
        out.append({"id": cid, "enumerated": sorted(enumerated), "text": text, "validate": rows})
    return out


# --- data status, versions, conflict resolutions, code digest --------------------------------


def status_output(as_json: bool) -> str:
    buf = io.StringIO()
    cli.cmd_data_status(argparse.Namespace(json=as_json), buf)
    return buf.getvalue()


CONFLICT_DECKS = {
    **{p.stem: p.read_text(encoding="utf-8") for p in sorted(EXAMPLES.glob("*.txt"))},
    "overridden": (
        "4 EXB-001\n4 ST01-015\n4 GD02-053\n4 ST12-001\n4 GD03-015\n4 GD01-001\n10 R-001\n"
    ),
}


def conflict_vectors() -> list[Json]:
    """``conflict_resolutions`` for every ordered pair of the example decks and a deck of
    overridden cards, passed as ``reports/results.py:181-186`` does."""
    decks = {name: parse_deck(text, name=name) for name, text in CONFLICT_DECKS.items()}
    out = []
    for a, deck_a in decks.items():
        for b, deck_b in decks.items():
            result = versions_module.conflict_resolutions(
                {
                    "deck_under_test": deck_a.main + deck_a.resources,
                    "benchmark": deck_b.main + deck_b.resources,
                }
            )
            out.append(
                {
                    "deck_under_test": [a, CONFLICT_DECKS[a]],
                    "benchmark": [b, CONFLICT_DECKS[b]],
                    "json": json.dumps(result, ensure_ascii=False),
                }
            )
    return out


CODE_DIGEST_TREE: list[tuple[str, bytes]] = [
    ("a.py", b"x = 1\n"),
    ("a/b.py", b"y = 2\n"),
    ("a/__pycache__/b.cpython-315.pyc", b"\0\0"),
    ("a.pyc", b"\0"),
    (".DS_Store", b"\x00\x01"),
    ("data/z.json", '{"k": "Ⅱ"}\n'.encode()),
    ("data/z", b""),
    ("é.txt", b"accent\n"),
]


def code_digest_vector() -> Json:
    """``reports/versions.py:32-52`` over a small tree, for the binaries' build-time digest
    (PLAN §2.4, W-103): every file except ``__pycache__`` entries and ``*.pyc``, sorted by the
    whole relative path, hashed as ``path``, ``b"\\0"``, bytes."""
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        for rel, data in CODE_DIGEST_TREE:
            p = root / rel
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_bytes(data)
        files = sorted(versions_module._package_files(root))
        h = hashlib.sha256()
        for path, file in files:
            h.update(path.encode())
            h.update(b"\0")
            h.update(file.read_bytes())
    return {
        "files": [[rel, data.hex()] for rel, data in CODE_DIGEST_TREE],
        "hashed": [path for path, _ in files],
        "sha256": h.hexdigest(),
    }


def status() -> Json:
    package = versions_module.package_version()
    digest = versions_module.code_digest()
    return {
        "package": package,
        "code_digest": digest,
        "versions_json": json.dumps(versions_module.versions(), ensure_ascii=False),
        "coverage": cli._coverage(),
        "text": status_output(False),
        "json": status_output(True),
        "conflict_resolutions": conflict_vectors(),
        "code_digest_vector": code_digest_vector(),
    }


# --- output ----------------------------------------------------------------------------------


def render(data: Json) -> str:
    """Top-level keys sorted; a list value is written one item per line, so a stale fixture
    shows a readable diff."""
    lines = ["{"]
    keys = sorted(data)
    for i, key in enumerate(keys):
        sep = "," if i < len(keys) - 1 else ""
        value = data[key]
        if isinstance(value, list) and value:
            lines.append(f"{json.dumps(key)}: [")
            rows = [json.dumps(item, ensure_ascii=False, sort_keys=True) for item in value]
            lines += [r + ("," if j < len(rows) - 1 else "") for j, r in enumerate(rows)]
            lines.append("]" + sep)
        else:
            lines.append(f"{json.dumps(key)}: {json.dumps(value, ensure_ascii=False)}{sep}")
    lines.append("}")
    return "\n".join(lines) + "\n"


BUILDERS: dict[str, Callable[[], Json]] = {
    "carddefs.json": carddefs,
    "synthetic.json": synthetic,
    "decks.json": decks,
    "status.json": status,
}


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true", help="fail if a committed fixture is stale")
    args = ap.parse_args()
    stale = []
    for name, build in BUILDERS.items():
        path = OUT / name
        text = render(build())
        current = path.exists() and path.read_text(encoding="utf-8") == text
        if args.check:
            if not current:
                stale.append(path)
            continue
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")
        print(f"wrote {path.relative_to(ROOT)}")
    if args.check:
        for path in stale:
            print(f"{path.relative_to(ROOT)} is stale; run without --check", file=sys.stderr)
        if not stale:
            print("gcg-data fixtures are current")
        return 1 if stale else 0
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
