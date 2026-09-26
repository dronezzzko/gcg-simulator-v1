from __future__ import annotations

import json
from pathlib import Path
from typing import Any

import pytest

from gcg_sim.sources import conflicts as cf

RULES = """# Gundam Card Game Comprehensive Rules

Ver. 1.9.0

Updated Sep 11, 2026

### 1) Game Overview

#### 1-1. Number of Players

**1-1-1.** A game between two players. (See 2. Card Information)

### 2) Card Information

#### 2-1. Card Number

**2-1-1.** Cards with the same card number are the same card. (See 3-1. Card Number)

**2-1-2.** Using an activated effect (see 9-1-7) needs its conditions.

> Ex: An example line that belongs to 2-1-2.

**2-1-3.** Damage is not dealt when the amount of damage dealt would be zero.

### 10) Effect Activation and Resolution

##### 10-1-7. Activated Effects

**10-1-7-1.** An activated effect can be freely activated by the player. (See 10-1-7. Activated Effects)
"""


def make_card(product_id: str, **fields: Any) -> dict[str, Any]:
    number = product_id.split("_p", maxsplit=1)[0]
    effect = fields.pop("effect", "-")
    keywords = cf.derive_keyword_effects(effect)
    markers = cf.derive_timing_markers(effect)
    card: dict[str, Any] = {
        "product_id": product_id,
        "card_number": number,
        "name": "Test Unit",
        "set_code": number.split("-")[0],
        "rarity": "C",
        "card_type": "UNIT",
        "color": "Blue",
        "level": 3,
        "cost": 2,
        "ap": 3,
        "hp": 3,
        "ap_raw": "3",
        "hp_raw": "3",
        "zone": "Space",
        "trait": "(Zeon)",
        "link": "-",
        "traits": ["Zeon"],
        "link_refs": [],
        "keyword_effects": keywords,
        "timing_markers": markers,
        "keywords_text": cf.derive_keywords_text(keywords, markers),
        "where_to_get": "Test Booster [TB01]",
        "effect": effect,
        "detail_url": f"https://example.invalid/{product_id}",
    }
    card.update(fields)
    return card


class Fixture:
    def __init__(self, root: Path) -> None:
        self.root = root
        self.data = root / "gcgapi"
        self.official = root / "official"
        self.rules = root / "rules.md"
        self.curated = root / "curated.json"
        self.overrides = root / "overrides.json"
        self.out_json = root / "out" / "conflicts.json"
        self.out_md = root / "out" / "CONFLICTS.md"
        self.data.mkdir()
        self.rules.write_text(RULES, encoding="utf-8")
        self.write_cards([])

    def write_cards(self, cards: list[dict[str, Any]]) -> None:
        text = "".join(json.dumps(c, ensure_ascii=False) + "\n" for c in cards)
        (self.data / "cards.ndjson").write_text(text, encoding="utf-8")

    def write(self, path: Path, payload: Any) -> None:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(payload, ensure_ascii=False), encoding="utf-8")

    def report(self) -> cf.Report:
        inputs = cf.load_inputs(self.data, self.rules, self.official, self.curated, self.overrides)
        return cf.build_report(inputs)

    def cli(self, *extra: str) -> tuple[int, list[str]]:
        lines: list[str] = []
        code = cf.main(
            [
                "--data-dir",
                str(self.data),
                "--rules",
                str(self.rules),
                "--official-dir",
                str(self.official),
                "--curated",
                str(self.curated),
                "--overrides",
                str(self.overrides),
                "--out-json",
                str(self.out_json),
                "--out-md",
                str(self.out_md),
                *extra,
            ],
            out=lines.append,
        )
        return code, lines


@pytest.fixture
def fx(tmp_path: Path) -> Fixture:
    return Fixture(tmp_path)


def by_id(report: cf.Report) -> dict[str, cf.Conflict]:
    return {c.id: c for c in report.conflicts}


# --- (a) divergent printings ------------------------------------------------------------------


def test_divergent_effect_is_reported_with_field_level_diff(fx: Fixture) -> None:
    fx.write_cards(
        [
            make_card("TB01-001", effect="【Deploy】Draw 1."),
            make_card("TB01-001_p1", effect="【Deploy】Draw 2.", where_to_get="Promo Pack"),
        ]
    )
    conflict = by_id(fx.report())["divergent:TB01-001"]
    assert conflict.kind == "divergent_printing"
    assert conflict.severity == "minor"
    assert conflict.details["canonical_suggestion"] == "TB01-001"
    assert conflict.details["deviating_printings"] == ["TB01-001_p1"]
    fields = {d["field"]: d for d in conflict.details["fields"]}
    assert set(fields) == {"effect"}
    assert fields["effect"]["classification"] == "substantive"
    assert fields["effect"]["variants"] == [
        {"value": "【Deploy】Draw 1.", "product_ids": ["TB01-001"]},
        {"value": "【Deploy】Draw 2.", "product_ids": ["TB01-001_p1"]},
    ]


def test_divergent_reminder_text_and_typography_are_info(fx: Fixture) -> None:
    fx.write_cards(
        [
            make_card(
                "TB01-002",
                name="Z'Gok",
                effect="<Blocker> (Rest this Unit to change the attack target to it.)\nReturn it to its owner's hand.",
            ),
            make_card(
                "TB01-002_p1", name="Z’Gok", effect="<Blocker>\nReturn it to its owner' s hand."
            ),
        ]
    )
    conflict = by_id(fx.report())["divergent:TB01-002"]
    assert conflict.severity == "info"
    classes = {d["field"]: d["classification"] for d in conflict.details["fields"]}
    assert classes == {"name": "cosmetic", "effect": "reminder_only"}
    assert conflict.details["deviating_printings"] == []


@pytest.mark.parametrize(
    ("where", "severity"), [("Store Tournament Pack", "major"), ("Edition Beta", "minor")]
)
def test_divergent_stats_are_major_unless_only_edition_beta(
    fx: Fixture, where: str, severity: str
) -> None:
    fx.write_cards(
        [make_card("TB01-003"), make_card("TB01-003_p1", ap=5, ap_raw="5", where_to_get=where)]
    )
    assert by_id(fx.report())["divergent:TB01-003"].severity == severity


def test_divergent_raw_stat_sign_is_cosmetic(fx: Fixture) -> None:
    fx.write_cards(
        [make_card("TB01-004", ap_raw="+2", ap=2), make_card("TB01-004_p1", ap_raw="2", ap=2)]
    )
    conflict = by_id(fx.report())["divergent:TB01-004"]
    assert conflict.severity == "info"
    assert conflict.details["fields"][0]["classification"] == "cosmetic"


def test_canonical_without_base_printing_prefers_common_non_beta_newest() -> None:
    printings = [
        make_card("TB01-005_p1", effect="A", where_to_get="Edition Beta"),
        make_card("TB01-005_p2", effect="B"),
        make_card("TB01-005_p3", effect="B"),
        make_card("TB01-005_p4", effect="C"),
    ]
    assert cf.suggest_canonical(printings) == "TB01-005_p3"
    assert cf.suggest_canonical([make_card("TB01-005_p1"), make_card("TB01-005")]) == "TB01-005"


def test_identical_printings_are_not_reported(fx: Fixture) -> None:
    fx.write_cards(
        [make_card("TB01-006"), make_card("TB01-006_p1", rarity="C +", where_to_get="Other")]
    )
    assert not [c for c in fx.report().conflicts if c.kind == "divergent_printing"]


# --- (b) errata ---------------------------------------------------------------------------------


def errata(**fields: Any) -> dict[str, Any]:
    entry = {
        "card_number": "TB01-010",
        "name": "Errata Unit",
        "field": "effect",
        "before": "from your trash",
        "after": "from any player's trash",
        "date": "2026-04-10",
        "source_url": "https://example.invalid/errata",
        "status": "applied",
        "printings_affected": 2,
    }
    entry.update(fields)
    return entry


def test_errata_applied_everywhere_is_info(fx: Fixture) -> None:
    text = "Choose 1 Unit card from any player's trash."
    fx.write_cards([make_card("TB01-010", effect=text), make_card("TB01-010_p1", effect=text)])
    fx.write(fx.data / "errata.json", [errata()])
    conflict = by_id(fx.report())["errata:TB01-010"]
    assert conflict.severity == "info"
    assert all(
        row["after_present"] and row["before_absent"] for row in conflict.details["printings"]
    )


def test_errata_missing_from_a_printing_is_major(fx: Fixture) -> None:
    fx.write_cards(
        [
            make_card("TB01-010", effect="Choose 1 Unit card from any player's trash."),
            make_card("TB01-010_p1", effect="Choose 1 Unit card from your trash."),
        ]
    )
    fx.write(fx.data / "errata.json", [errata()])
    found = by_id(fx.report())
    assert "errata:TB01-010" not in found
    assert found["errata-unapplied:TB01-010"].severity == "major"


def test_errata_before_inside_after_is_not_a_false_positive(fx: Fixture) -> None:
    fx.write_cards(
        [
            make_card(
                "TB01-011", trait="(Cyclops Team) (Zeon Army)", traits=["Cyclops Team", "Zeon Army"]
            )
        ]
    )
    entry = errata(
        card_number="TB01-011",
        field="trait",
        before="(Zeon",
        after="(Zeon Army)",
        printings_affected=1,
    )
    fx.write(fx.data / "errata.json", [entry])
    assert by_id(fx.report())["errata:TB01-011"].severity == "info"


def test_errata_trait_requires_consistent_traits_array(fx: Fixture) -> None:
    fx.write_cards([make_card("TB01-012", trait="(Cyclops Team)", traits=["Zeon"])])
    entry = errata(
        card_number="TB01-012",
        field="trait",
        before="(Zeon)",
        after="(Cyclops Team)",
        printings_affected=1,
    )
    fx.write(fx.data / "errata.json", [entry])
    assert "errata-unapplied:TB01-012" in by_id(fx.report())


def test_errata_printing_count_mismatch_is_minor(fx: Fixture) -> None:
    fx.write_cards([make_card("TB01-010", effect="from any player's trash")])
    fx.write(fx.data / "errata.json", [errata(printings_affected=3)])
    assert by_id(fx.report())["errata:TB01-010"].severity == "minor"


# --- (c) rules version --------------------------------------------------------------------------


def test_rules_version_pending_without_official_record(fx: Fixture) -> None:
    conflict = by_id(fx.report())["rules-version:official-record-pending"]
    assert conflict.severity == "info"
    assert "1.9.0" in conflict.description


def test_rules_version_matching_record_emits_nothing(fx: Fixture) -> None:
    fx.write(
        fx.official / "rules_version.json",
        {"latest_version": "1.9.0", "latest_date": "2026-09-11", "url": "u"},
    )
    assert not [c for c in fx.report().conflicts if c.kind == "rules_version"]


def test_rules_version_mismatch_is_major(fx: Fixture) -> None:
    fx.write(
        fx.official / "rules_version.json",
        {"latest_version": "1.10.0", "latest_date": "October 2, 2026", "url": "u"},
    )
    [conflict] = [c for c in fx.report().conflicts if c.kind == "rules_version"]
    assert conflict.id == "rules-version:mismatch:1.9.0@2026-09-11:1.10.0@2026-10-02"
    assert conflict.severity == "major"


def test_rules_version_unparseable_rules_file(fx: Fixture) -> None:
    fx.rules.write_text("# Rules without a version\n", encoding="utf-8")
    assert "rules-version:unparseable" in by_id(fx.report())


# --- (d) banned/restricted ambiguities ---------------------------------------------------------


def test_banlist_absent_or_without_ambiguities_is_tolerated(fx: Fixture) -> None:
    assert not [c for c in fx.report().conflicts if c.kind == "banlist_ambiguity"]
    fx.write(fx.official / "banlist.json", {"banned": []})
    assert not [c for c in fx.report().conflicts if c.kind == "banlist_ambiguity"]


def test_banlist_ambiguities_become_conflicts(fx: Fixture) -> None:
    fx.write(
        fx.official / "banlist.json",
        {
            "ambiguities": [
                {"id": "scope", "description": "Does the list apply?", "cards": ["TB01-001"]},
                "Plain text item",
            ]
        },
    )
    found = {c.id: c for c in fx.report().conflicts if c.kind == "banlist_ambiguity"}
    assert found["banlist:scope"].card_numbers == ["TB01-001"]
    assert len(found) == 2
    hashed = next(cid for cid in found if cid != "banlist:scope")
    assert found[hashed].description == "Plain text item"


# --- (e) keyword / timing markers --------------------------------------------------------------


def test_stored_markers_that_disagree_with_text_are_flagged(fx: Fixture) -> None:
    card = make_card("TB01-020", effect="<Blocker> (Rest this Unit.)\n【Deploy】Draw 1.")
    card["keyword_effects"] = [{"keyword": "Repair", "value": 1}]
    card["timing_markers"] = ["Deploy", "Attack"]
    card["keywords_text"] = "repair deploy attack"
    fx.write_cards([card])
    found = by_id(fx.report())
    assert found["markers:TB01-020:keyword_effects"].details["printings"][0][
        "derived_from_effect"
    ] == [{"keyword": "Blocker", "value": None}]
    assert "markers:TB01-020:timing_markers" in found
    assert "markers:TB01-020:keywords_text" in found


def test_keyword_outside_angle_brackets_and_ascii_colon(fx: Fixture) -> None:
    fx.write_cards(
        [
            make_card("TB01-021", effect="[Suppression] (Damage to Shields is dealt to 2 cards.)"),
            make_card("TB01-022", effect="【Activate･Main】Exile 3 cards from your trash: Draw 1."),
            make_card("TB01-023", effect="(Support) Trait units with <Support 1> are fine."),
        ]
    )
    found = by_id(fx.report())
    assert found["markers:TB01-021:unbracketed-keyword"].severity == "major"
    assert found["markers:TB01-022:ascii-colon"].severity == "minor"
    assert not [cid for cid in found if cid.startswith("markers:TB01-023")]


def test_marker_vocabulary_aggregates(fx: Fixture) -> None:
    fx.write_cards(
        [
            make_card(
                "TB01-030",
                card_type="COMMAND",
                effect="【Main】Draw 1.\n【Pilot】[Amuro Ray]",
                ap=1,
                hp=1,
            ),
            make_card("TB01-031", effect="【Deploy・Development 2】You may exile 2 cards."),
            make_card("TB01-032", effect="【When Paired･(Zeon) Pilot】Draw 1."),
            make_card("TB01-033", effect="【Activate･Main】Rest this Unit：Draw 1."),
            make_card("TB01-034", effect="While you have 2 Units, this Unit gains <Blocker>."),
        ]
    )
    found = by_id(fx.report())
    assert found["markers-vocab:pilot"].card_numbers == ["TB01-030"]
    assert found["markers-vocab:development"].card_numbers == ["TB01-031"]
    assert found["markers-vocab:qualified-timing"].card_numbers == ["TB01-032"]
    assert found["markers-vocab:activate-split"].card_numbers == ["TB01-033"]
    assert found["markers-semantics:keyword-mentions"].card_numbers == ["TB01-034"]


def test_null_stats_required_by_rules(fx: Fixture) -> None:
    fx.write_cards(
        [
            make_card("TB01-040", ap=None, ap_raw=None),
            make_card("TB01-041", card_type="BASE", ap=None, ap_raw=None),
            make_card(
                "TB01-042",
                card_type="COMMAND",
                ap=None,
                hp=None,
                effect="【Main】Draw 1.\n【Pilot】[X]",
            ),
        ]
    )
    found = by_id(fx.report())
    assert found["data-null:UNIT:ap"].severity == "minor"
    assert found["data-null:BASE:ap"].severity == "info"
    assert found["data-null:COMMAND-PILOT:ap"].card_numbers == ["TB01-042"]


# --- (f) rules cross-references and parsing ------------------------------------------------------


def test_parse_rules() -> None:
    rules = cf.parse_rules(RULES)
    assert (rules.version, rules.updated) == ("1.9.0", "2026-09-11")
    assert rules.headings["2"] == "Card Information"
    assert rules.headings["10-1-7"] == "Activated Effects"
    assert rules.paragraphs["2-1-2"].endswith("An example line that belongs to 2-1-2.")


def test_rules_xrefs_detected_with_suggestions(fx: Fixture) -> None:
    found = {c.id: c for c in fx.report().conflicts if c.kind == "rules_xref"}
    assert set(found) == {"rules-xref:2-1-1", "rules-xref:2-1-2"}
    assert found["rules-xref:2-1-1"].details["suggested_target"] == "2-1"
    assert found["rules-xref:2-1-2"].details["suggested_target"] == "10-1-7"


# --- curated entries ---------------------------------------------------------------------------


def curated_entry(**fields: Any) -> dict[str, Any]:
    entry: dict[str, Any] = {
        "id": "rules-internal:zero-damage",
        "kind": "rules_internal",
        "severity": "info",
        "card_numbers": [],
        "title": "Zero damage",
        "description": "Test entry.",
        "evidence": [
            {
                "source": "rules",
                "ref": "2-1-3",
                "quote": "Damage is not dealt when the amount of damage dealt would be zero.",
            }
        ],
    }
    entry.update(fields)
    return entry


def test_curated_evidence_is_verified(fx: Fixture) -> None:
    fx.write_cards([make_card("TB01-050", effect="【Deploy】Draw 1.")])
    fx.write(
        fx.data / "rulings.json",
        [
            {
                "card_number": "TB01-050",
                "num": "Q1",
                "question": "Draw?",
                "answer": "Yes, you do.",
                "source_url": "u",
            }
        ],
    )
    fx.write(
        fx.data / "rules-faq.json",
        [
            {
                "num": "Q9",
                "question": "Who goes first?",
                "answer": "The winner decides.",
                "source_url": "f",
            }
        ],
    )
    evidence = [
        {"source": "rules", "ref": "2-1-3", "quote": "Damage is not dealt"},
        {"source": "card", "ref": "TB01-050", "quote": "【Deploy】Draw 1."},
        {"source": "ruling", "ref": "TB01-050:Q1", "quote": "Yes, you do."},
        {"source": "faq", "ref": "Q9", "quote": "The winner decides."},
    ]
    fx.write(
        fx.curated,
        {
            "schema_version": 1,
            "conflicts": [curated_entry(card_numbers=["TB01-050"], evidence=evidence)],
        },
    )
    report = fx.report()
    conflict = by_id(report)["rules-internal:zero-damage"]
    assert conflict.origin == "curated"
    assert conflict.problems == []
    assert report.stale == []
    assert all(item["verified"] for item in conflict.details["evidence"])
    assert len(conflict.sources) == 4


@pytest.mark.parametrize(
    "evidence",
    [
        {"source": "rules", "ref": "2-1-3", "quote": "Damage is always dealt."},
        {"source": "rules", "ref": "99-1", "quote": "anything"},
        {"source": "card", "ref": "TB01-999", "quote": "Draw 1."},
        {"source": "ruling", "ref": "TB01-050:Q7", "quote": "Yes"},
        {"source": "faq", "ref": "Q404", "quote": "Yes"},
        {"source": "unknown", "ref": "x", "quote": "y"},
    ],
)
def test_curated_entry_with_unmatched_evidence_is_stale(
    fx: Fixture, evidence: dict[str, str]
) -> None:
    fx.write(fx.curated, {"schema_version": 1, "conflicts": [curated_entry(evidence=[evidence])]})
    report = fx.report()
    assert report.stale == ["rules-internal:zero-damage"]
    assert "stale curated evidence: rules-internal:zero-damage" in report.check_failures


def test_curated_entry_refines_detected_conflict_with_same_id(fx: Fixture) -> None:
    entry = curated_entry(
        id="rules-xref:2-1-2",
        kind="rules_xref",
        severity="minor",
        title="Means 10-1-7",
        evidence=[{"source": "rules", "ref": "2-1-2", "quote": "(see 9-1-7)"}],
    )
    fx.write(fx.curated, {"schema_version": 1, "conflicts": [entry]})
    conflict = by_id(fx.report())["rules-xref:2-1-2"]
    assert conflict.origin == "detected+curated"
    assert conflict.title == "Means 10-1-7"
    assert conflict.details["suggested_target"] == "10-1-7"
    assert conflict.details["evidence"][0]["verified"]


def test_curated_structural_errors_are_invalid(fx: Fixture) -> None:
    entries = [
        curated_entry(),
        curated_entry(),
        curated_entry(id="x:kind", kind="nonsense"),
        curated_entry(id="x:severity", severity="critical"),
        curated_entry(id="x:evidence", evidence=[]),
    ]
    fx.write(fx.curated, {"schema_version": 1, "conflicts": entries})
    invalid = fx.report().invalid
    assert any("duplicate id" in msg for msg in invalid)
    assert any("unknown kind" in msg for msg in invalid)
    assert any("unknown severity" in msg for msg in invalid)
    assert any("at least one evidence" in msg for msg in invalid)


# --- resolutions ----------------------------------------------------------------------------------

POLICIES = {"p": {"summary": "s", "rationale": "r"}}


def test_resolution_validation(fx: Fixture) -> None:
    fx.write_cards(
        [
            make_card("TB01-060"),
            make_card("TB01-060_p1", ap=9, ap_raw="9"),
            make_card("TB01-061", ap=None),
        ]
    )
    good = {"policy": "p", "decision": "d", "rationale": "r"}
    fx.write(
        fx.overrides,
        {
            "schema_version": 1,
            "policies": POLICIES,
            "resolutions": {
                "divergent:TB01-060": {**good, "canonical_product_id": "TB01-061"},
                "data-null:UNIT:ap": {
                    **good,
                    "field_overrides": {"ap": "zero", "bogus": 1},
                    "card_numbers": ["TB01-999"],
                },
                "rules-xref:2-1-1": {**good, "policy": "missing", "extra": True},
                "rules-xref:2-1-2": {"policy": "p", "decision": "", "rationale": "r"},
                "rules-version:official-record-pending": {**good, "card_numbers": ["TB01-060"]},
                "gone:away": good,
            },
        },
    )
    report = fx.report()
    joined = "\n".join(report.invalid)
    assert "canonical_product_id 'TB01-061' is not a printing" in joined
    assert "field_overrides['ap'] has the wrong type" in joined
    assert "unknown field 'bogus'" in joined
    assert "card_numbers must list exactly ['TB01-061']" in joined
    assert "unknown policy 'missing'" in joined
    assert "unknown key 'extra'" in joined
    assert "decision must be a non-empty string" in joined
    assert "card_numbers is only used together with field_overrides" in joined
    assert report.orphaned == ["gone:away"]
    assert report.unresolved == []


def test_valid_field_overrides_pass(fx: Fixture) -> None:
    fx.write_cards([make_card("TB01-061", ap=None)])
    resolution = {
        "policy": "p",
        "decision": "d",
        "rationale": "r",
        "card_numbers": ["TB01-061"],
        "field_overrides": {
            "ap": 0,
            "effect": "x",
            "traits": ["A"],
            "keyword_effects": [{"keyword": "Blocker", "value": None}],
        },
    }
    resolutions = {
        "data-null:UNIT:ap": resolution,
        "rules-xref:2-1-1": {"policy": "p", "decision": "d", "rationale": "r"},
        "rules-xref:2-1-2": {"policy": "p", "decision": "d", "rationale": "r"},
        "rules-version:official-record-pending": {"policy": "p", "decision": "d", "rationale": "r"},
    }
    fx.write(fx.overrides, {"schema_version": 1, "policies": POLICIES, "resolutions": resolutions})
    report = fx.report()
    assert report.invalid == []
    assert report.check_failures == []


# --- CLI, determinism -------------------------------------------------------------------------------


def resolve_everything(fx: Fixture) -> None:
    ids = [c.id for c in fx.report().conflicts]
    resolutions = {cid: {"policy": "p", "decision": "d", "rationale": "r"} for cid in ids}
    fx.write(fx.overrides, {"schema_version": 1, "policies": POLICIES, "resolutions": resolutions})


def test_cli_generate_and_check_round_trip(fx: Fixture) -> None:
    fx.write_cards([make_card("TB01-070", effect="A"), make_card("TB01-070_p1", effect="B")])
    code, lines = fx.cli("--check")
    assert code == 1
    assert any("stale" in line for line in lines)
    assert any("unresolved conflict: divergent:TB01-070" in line for line in lines)

    assert fx.cli()[0] == 0
    code, lines = fx.cli("--check")
    assert code == 1
    assert not any("is stale" in line for line in lines)

    resolve_everything(fx)
    assert fx.cli()[0] == 0
    code, lines = fx.cli("--check")
    assert code == 0
    assert "unresolved 0, invalid 0, stale 0, orphaned 0" in lines[0]

    fx.out_md.write_text(fx.out_md.read_text(encoding="utf-8") + "edited\n", encoding="utf-8")
    code, lines = fx.cli("--check")
    assert code == 1
    assert any("CONFLICTS.md is stale" in line for line in lines)


def test_cli_reports_missing_inputs(fx: Fixture) -> None:
    (fx.data / "cards.ndjson").unlink()
    code, lines = fx.cli()
    assert code == 2
    assert "missing card snapshot" in lines[0]


def test_outputs_are_deterministic_and_complete(fx: Fixture) -> None:
    fx.write_cards([make_card("TB01-080", effect="A"), make_card("TB01-080_p1", effect="B")])
    resolve_everything(fx)
    first = fx.report()
    second = fx.report()
    assert cf.render_json(first) == cf.render_json(second)
    assert cf.render_markdown(first) == cf.render_markdown(second)
    payload = json.loads(cf.render_json(first))
    assert payload["summary"]["unresolved"] == []
    assert all(entry["resolution"] for entry in payload["conflicts"])
    assert "sha256" in payload["inputs"]
    markdown = cf.render_markdown(first)
    assert markdown.startswith("# Source conflicts")
    assert "## Divergent printings (1)" in markdown
    assert "### `divergent:TB01-080`" in markdown


def test_strip_reminders_keeps_traits_and_token_specs() -> None:
    text = "Deploy 1 [Zaku]((Zeon)･AP1･HP1) token. <Blocker> (Rest this Unit to change the attack target to it.)"
    stripped = cf.strip_reminders(text)
    assert "((Zeon)･AP1･HP1)" in stripped
    assert "change the attack target" not in stripped


# --- packaged data ---------------------------------------------------------------------------------


def test_packaged_data_has_every_conflict_resolved() -> None:
    report = cf.build_report(cf.load_inputs())
    assert report.unresolved == []
    assert report.invalid == []
    assert report.stale == []
    assert report.orphaned == []
    kinds = {c.kind for c in report.conflicts}
    assert {
        "divergent_printing",
        "errata",
        "rules_xref",
        "ruling_vs_text",
        "faq_vs_rules",
        "ambiguous_text",
    } <= kinds


def test_packaged_outputs_are_current() -> None:
    lines: list[str] = []
    assert cf.main(["--check"], out=lines.append) == 0, lines


def test_unreviewed_rulings_and_faq_entries_need_review(fx: Fixture) -> None:
    ruling = {
        "card_number": "TB01-090",
        "num": "Q1",
        "question": "Draw?",
        "answer": "Yes.",
        "source_url": "u",
    }
    entry = {"num": "Q9", "question": "Who goes first?", "answer": "The winner.", "source_url": "f"}
    fx.write_cards([make_card("TB01-090")])
    fx.write(fx.data / "rulings.json", [ruling])
    fx.write(fx.data / "rules-faq.json", [entry])
    found = by_id(fx.report())
    assert found["unreviewed:ruling:TB01-090:Q1"].card_numbers == ["TB01-090"]
    assert found["unreviewed:faq:Q9"].kind == "faq_vs_rules"

    reviewed = {
        "rulings": {
            "TB01-090:Q1": cf.review_fingerprint("Draw?", "Yes."),
            "TB01-999:Q2": "000000000000",
        },
        "faq": {"Q9": cf.review_fingerprint("Who goes first?", "The winner.")},
    }
    fx.write(
        fx.curated, {"schema_version": 1, "conflicts": [], "reviewed_without_conflict": reviewed}
    )
    report = fx.report()
    assert not [c for c in report.conflicts if c.id.startswith("unreviewed:")]
    assert report.orphaned == ["reviewed:ruling:TB01-999:Q2"]

    fx.write(fx.data / "rulings.json", [{**ruling, "answer": "No."}])
    changed = by_id(fx.report())["unreviewed:ruling:TB01-090:Q1"]
    assert "changed since it was reviewed" in changed.title
