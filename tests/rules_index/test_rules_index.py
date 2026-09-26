import dataclasses
import datetime as dt
import hashlib
import json
from pathlib import Path

import pytest

from gcg_sim.rules.index import (
    FEATURE_AREAS,
    RULES_INDEX_PATH,
    RULES_MARKDOWN_PATH,
    RULES_NA_PATH,
    RulesIndex,
    RulesIndexError,
    feature_area_for,
    index_to_json,
    load_na_reasons,
    load_rules_index,
    main,
    parse_na_reasons,
    parse_rules,
    rule_number,
)

REPO_ROOT = Path(__file__).resolve().parents[2]
MINIMAL_PREAMBLE = "# Rules\n\nVer. 2.0.1\n\nUpdated Sept 3, 2027\n\n## Comprehensive Rules\n\n"


@pytest.fixture(scope="module")
def index() -> RulesIndex:
    return load_rules_index()


@pytest.fixture(scope="module")
def index_json() -> dict[str, object]:
    payload = json.loads(RULES_INDEX_PATH.read_text(encoding="utf-8"))
    assert isinstance(payload, dict)
    return payload


def test_version_and_date(index: RulesIndex) -> None:
    assert index.version == "1.9.0"
    assert index.date == dt.date(2026, 9, 11)
    assert index.date_text == "Updated Sep 11, 2026"


def test_entry_counts(index: RulesIndex) -> None:
    assert len(index) == 579
    kinds = [entry.kind for entry in index.entries]
    assert (kinds.count("section"), kinds.count("heading"), kinds.count("rule")) == (13, 118, 448)
    per_section = {
        s: sum(1 for e in index.entries if e.section == s) for s in map(str, range(1, 14))
    }
    assert per_section == {
        "1": 21,
        "2": 61,
        "3": 52,
        "4": 50,
        "5": 98,
        "6": 26,
        "7": 47,
        "8": 36,
        "9": 11,
        "10": 54,
        "11": 20,
        "12": 32,
        "13": 71,
    }


def test_document_order_is_numeric_order(index: RulesIndex) -> None:
    ids = index.ids()
    assert list(ids) == sorted(ids, key=rule_number)
    line_numbers = [entry.line_no for entry in index.entries]
    assert line_numbers == sorted(set(line_numbers))


@pytest.mark.parametrize(
    ("rule_id", "kind", "title", "text", "parent_id", "depth", "section_title", "line_no"),
    [
        ("1", "section", "Game Overview", "Game Overview", None, 1, "Game Overview", 25),
        (
            "1-2",
            "heading",
            "Winning and Losing the Game",
            "Winning and Losing the Game",
            "1",
            2,
            "Winning and Losing the Game",
            31,
        ),
        (
            "1-2-4",
            "rule",
            None,
            "During a game, any player may concede at any time. That player immediately loses and the game ends.",
            "1-2",
            3,
            "Winning and Losing the Game",
            43,
        ),
        (
            "3-1",
            "rule",
            None,
            "There are five card types: Unit, Pilot, Command, Base, and Resource. Most card elements can be found on all of the card types. (See 2. Card Information) Refer to the rules section of each card type for details about noteworthy card elements.",
            "3",
            2,
            "Card Types",
            193,
        ),
        ("5-17-3-1", "heading", "EX Base", "EX Base", "5-17-3", 4, "EX Base", 527),
        (
            "5-17-3-1-1",
            "rule",
            None,
            "An EX Base is a Base token with 0 AP and 3 HP.",
            "5-17-3-1",
            5,
            "EX Base",
            529,
        ),
        (
            "7-5-2-2-3",
            "rule",
            None,
            "Choose the number of Resources necessary to pay its cost and rest them.",
            "7-5-2-2",
            5,
            "Playing Cards from the Hand",
            699,
        ),
        ("7-2-3", "heading", "Active Step", "Active Step", "7-2", 3, "Active Step", 663),
        (
            "8-5-2-4-2",
            "rule",
            None,
            "If the attacking Unit has <First Strike>, it deals battle damage to the enemy Base before normal battle damage is managed. (See 12-1-5. <First Strike>)",
            "8-5-2-4",
            5,
            "Attack on a Player",
            791,
        ),
        (
            "13-1-3-1",
            "rule",
            None,
            "<Support> is a keyword effect that activates when you rest the Unit. <Support (amount)> indicates the effect “【Activate･Main】Rest this Unit：Choose one other friendly unit. It gets AP+(amount) during this turn.”",
            "13-1-3",
            4,
            "<Support>",
            1075,
        ),
        (
            "13-2-13-2",
            "rule",
            None,
            "If multiple cards have a copy of the same effect with 【Once per Turn】, each card can activate it one time.",
            "13-2-13",
            4,
            "【Once per Turn】",
            1197,
        ),
    ],
)
def test_known_entries(
    index: RulesIndex,
    rule_id: str,
    kind: str,
    title: str | None,
    text: str,
    parent_id: str | None,
    depth: int,
    section_title: str,
    line_no: int,
) -> None:
    entry = index[rule_id]
    assert entry.id == rule_id
    assert entry.kind == kind
    assert entry.title == title
    assert entry.text == text
    assert entry.parent_id == parent_id
    assert entry.depth == depth
    assert entry.section == str(rule_number(rule_id)[0])
    assert entry.section_title == section_title
    assert entry.line_no == line_no
    assert entry.number == rule_number(rule_id)


def test_line_numbers_point_at_the_rule(index: RulesIndex) -> None:
    lines = RULES_MARKDOWN_PATH.read_text(encoding="utf-8").splitlines()
    for entry in index.entries:
        line = lines[entry.line_no - 1]
        assert f"{entry.id}." in line or f"{entry.id})" in line, entry.id


def test_parent_links_and_children(index: RulesIndex) -> None:
    for entry in index.entries:
        if entry.depth == 1:
            assert entry.parent_id is None
            assert entry.kind == "section"
            continue
        assert entry.parent_id is not None
        assert entry.id in index.children(entry.parent_id)
        assert index[entry.parent_id].depth == entry.depth - 1
    for entry in [*index.entries]:
        children = index.children(entry.id)
        assert [rule_number(child)[-1] for child in children] == list(
            range(1, len(children) + 1)
        ), entry.id
    assert index.children("7-2-3") == ("7-2-3-1", "7-2-3-2")
    assert index.children("13") == ("13-1", "13-2")
    assert index.children("1-2-2") == ("1-2-2-1", "1-2-2-2")
    assert index.children("7-5-2-2-3") == ()


def test_examples_attach_to_preceding_rule(index: RulesIndex) -> None:
    with_examples = {entry.id for entry in index.entries if entry.examples}
    assert with_examples == {
        "1-3-2-1",
        "3-2-6-4",
        "5-19-1",
        "13-1-1-2",
        "13-1-2-5",
        "13-1-3-2",
        "13-2-10-2",
        "13-2-12-2",
    }
    assert index["13-1-1-2"].examples == (
        "If a Unit with <Repair 1> gains <Repair 2>, the <Repair> effect on that Unit becomes <Repair 3>.",
    )
    assert (
        index["3-2-6-4"]
        .examples[0]
        .startswith(
            "When you pair the Pilot Garrod Ran & Tiffa Adill with a Gundam X that has the link requirement [Garrod Ran]"
        )
    )


def test_markdown_escapes_are_removed(index: RulesIndex) -> None:
    for entry in index.entries:
        assert "\\" not in entry.text, entry.id
        assert "**" not in entry.text, entry.id
        assert all("\\" not in example for example in entry.examples), entry.id


def test_lookup_api(index: RulesIndex) -> None:
    assert index.get("99-1") is None
    assert "1-2-1" in index
    assert "99-1" not in index
    with pytest.raises(KeyError):
        index["99-1"]
    with pytest.raises(KeyError):
        index.children("99-1")
    with pytest.raises(KeyError):
        index.status("99-1")
    assert index.ids()[:4] == ("1", "1-1", "1-1-1", "1-2")
    assert index.ids()[-1] == "13-2-13-2"
    with pytest.raises(dataclasses.FrozenInstanceError):
        index.version = "0"  # type: ignore[misc]
    with pytest.raises(dataclasses.FrozenInstanceError):
        index["1-1-1"].text = "changed"  # type: ignore[misc]


def test_feature_areas(index: RulesIndex) -> None:
    for entry in index.entries:
        assert entry.feature_area in FEATURE_AREAS, entry.id
        assert entry.feature_area == feature_area_for(entry.id)
    used = {entry.feature_area for entry in index.entries}
    assert used == set(FEATURE_AREAS)
    assert {e.feature_area for e in index.entries if e.section == "12"} == {"multiplayer"}
    expected = {
        "1-2-4": "win-loss",
        "2-1-2": "deck-construction",
        "3-3-4": "pairing-link",
        "4-8-3": "information",
        "5-17-2-5": "tokens",
        "6-1-1": "deck-construction",
        "6-2-2": "setup",
        "7-3-1-1": "win-loss",
        "7-6-3-1": "action-step",
        "8-4-1": "action-step",
        "8-5-3-2-2": "battle",
        "10-1-6-8": "effects-triggers",
        "11-4-2": "rules-management",
        "13-1-5-2": "keywords",
    }
    assert {rule_id: index[rule_id].feature_area for rule_id in expected} == expected


def test_na_file_is_valid_and_partitions_the_index(index: RulesIndex) -> None:
    raw = json.loads(RULES_NA_PATH.read_text(encoding="utf-8"))
    assert raw["schema_version"] == 1
    reasons = load_na_reasons()
    assert reasons == dict(index.na_reasons)
    assert set(reasons) <= set(index.ids())
    assert list(raw["rules"]) == sorted(raw["rules"], key=rule_number)
    assert set(index.na_ids()) | set(index.testable_ids()) == set(index.ids())
    assert not set(index.na_ids()) & set(index.testable_ids())
    assert (len(index.na_ids()), len(index.testable_ids())) == (184, 395)
    for rule_id, reason in reasons.items():
        assert len(reason) >= 20, rule_id
        assert index.status(rule_id) == "na"
        assert index.na_reason(rule_id) == reason


def test_all_multiplayer_rules_are_na(index: RulesIndex) -> None:
    section_12 = [entry.id for entry in index.entries if entry.section == "12"]
    assert len(section_12) == 32
    for rule_id in section_12:
        reason = index.na_reason(rule_id)
        assert reason is not None, rule_id
        assert any(word in reason for word in ("multiplayer", "battle royale", "team battle"))
    assert not [
        e.id for e in index.entries if e.section != "12" and e.feature_area == "multiplayer"
    ]


def test_headings_are_grouping_na(index: RulesIndex) -> None:
    for entry in index.entries:
        reason = index.na_reason(entry.id)
        if entry.kind != "rule" and entry.section != "12":
            assert reason is not None, entry.id
            assert reason.startswith("grouping heading; covered by child"), entry.id
        if reason is not None and reason.startswith("grouping"):
            children = index.children(entry.id)
            assert children, entry.id
            assert children[0] in reason, entry.id


def test_presentational_rules_are_na(index: RulesIndex) -> None:
    for rule_id in ("2-13-1", "2-14-1", "2-15-1", "2-16-1", "5-18-2", "6-2-1-4-1"):
        assert index.status(rule_id) == "na", rule_id


@pytest.mark.parametrize(
    "rule_id",
    [
        "1-1-1",
        "1-2-1",
        "1-2-2-1",
        "1-2-2-2",
        "1-2-3",
        "1-2-4",
        "1-2-5",
        "1-3-4",
        "2-1-2",
        "2-4-2-1",
        "2-11-4",
        "4-1-3",
        "4-1-4",
        "4-1-5",
        "4-1-7",
        "4-2-2",
        "4-3-2",
        "4-6-4-1",
        "4-8-2",
        "4-8-3",
        "5-13-1",
        "5-17-1",
        "5-17-2-1",
        "5-17-2-5",
        "5-17-2-5-1",
        "5-17-3-2-3",
        "5-17-4",
        "5-17-4-2",
        "5-20-1",
        "5-20-2",
        "6-1-1",
        "6-1-1-1",
        "6-1-1-2",
        "6-1-1-2-1",
        "6-1-1-3",
        "6-1-1-4",
        "6-1-1-5",
        "6-1-2",
        "6-2-1-1",
        "6-2-1-2",
        "6-2-1-3",
        "6-2-1-4",
        "6-2-1-5",
        "6-2-1-6",
        "6-2-1-6-1",
        "6-2-1-7",
        "6-2-2",
        "6-2-3",
        "6-2-4",
        "6-2-5",
        "7-5-2-2-1",
        "7-6-5-1",
        "8-5-2-4-2",
        "10-1-6-8",
        "11-4-2-2",
        "13-1-5-4",
    ],
)
def test_engine_observable_rules_stay_testable(index: RulesIndex, rule_id: str) -> None:
    assert index.status(rule_id) == "testable"


def test_rules_index_json_matches_parse(index: RulesIndex, index_json: dict[str, object]) -> None:
    assert index_json["schema_version"] == 1
    source = index_json["source"]
    assert isinstance(source, dict)
    markdown_bytes = RULES_MARKDOWN_PATH.read_bytes()
    assert source == {
        "date": "2026-09-11",
        "date_text": "Updated Sep 11, 2026",
        "file": "gundam-card-game-comprehensive-rules.md",
        "sha256": hashlib.sha256(markdown_bytes).hexdigest(),
        "version": "1.9.0",
    }
    rules = index_json["rules"]
    assert isinstance(rules, dict)
    assert list(rules) == list(index.ids())
    for rule_id, payload in rules.items():
        entry = index[rule_id]
        assert payload["status"] == index.status(rule_id)
        assert payload["na_reason"] == index.na_reason(rule_id)
        assert payload["feature_area"] == entry.feature_area
        assert payload["parent_id"] == entry.parent_id
        assert payload["children"] == list(index.children(rule_id))
        assert payload["text"] == entry.text
        assert payload["line_no"] == entry.line_no
        assert list(payload) == sorted(payload)
    counts = index_json["counts"]
    assert isinstance(counts, dict)
    assert (counts["total"], counts["na"], counts["testable"]) == (579, 184, 395)
    assert index_json["feature_areas"] == list(FEATURE_AREAS)


def test_check_passes_for_committed_index() -> None:
    assert main(["--check"]) == 0


def test_check_detects_stale_index_and_write_fixes_it(tmp_path: Path) -> None:
    target = tmp_path / "rules_index.json"
    assert main(["--check", "--index", str(target)]) == 1
    target.write_text("{}\n", encoding="utf-8")
    assert main(["--check", "--index", str(target)]) == 1
    assert main(["--write", "--index", str(target)]) == 0
    assert target.read_bytes() == RULES_INDEX_PATH.read_bytes()
    assert main(["--check", "--index", str(target)]) == 0


def test_check_detects_na_change(tmp_path: Path) -> None:
    payload = json.loads(RULES_NA_PATH.read_text(encoding="utf-8"))
    del payload["rules"]["2-16-1"]
    na_path = tmp_path / "rules_na.json"
    na_path.write_text(json.dumps(payload), encoding="utf-8")
    assert main(["--check", "--na", str(na_path)]) == 1


def test_rendering_is_deterministic(index: RulesIndex) -> None:
    rendered = index_to_json(index)
    assert rendered == index_to_json(load_rules_index())
    assert rendered == RULES_INDEX_PATH.read_text(encoding="utf-8")


def test_packaged_rules_match_repository_copy() -> None:
    assert (REPO_ROOT / "gundam-card-game-comprehensive-rules.md").read_bytes() == (
        RULES_MARKDOWN_PATH.read_bytes()
    )


def test_parse_minimal_document() -> None:
    markdown = (
        MINIMAL_PREAMBLE
        + "### 1) Alpha\n\n#### 1-1. Beta \\<Gamma\\>\n\n**1-1-1.** Do \\[x\\].\n\n"
        + "> Ex: First.\n\n> Ex: Second.\n\n**1-1-2.** Other.\n"
    )
    parsed = parse_rules(markdown, {"1": "grouping heading; covered by child 1-1"})
    assert parsed.version == "2.0.1"
    assert parsed.date == dt.date(2027, 9, 3)
    assert parsed.ids() == ("1", "1-1", "1-1-1", "1-1-2")
    assert parsed["1-1"].title == "Beta <Gamma>"
    assert parsed["1-1-1"].text == "Do [x]."
    assert parsed["1-1-1"].examples == ("First.", "Second.")
    assert parsed["1-1-1"].section_title == "Beta <Gamma>"
    assert parsed["1-1-1"].line_no == 13
    assert parsed.status("1") == "na"
    assert parsed.testable_ids() == ("1-1", "1-1-1", "1-1-2")


@pytest.mark.parametrize(
    ("markdown", "message"),
    [
        ("# Rules\n\nVer. 1.0.0\n\nUpdated Jan 1, 2026\n", "missing"),
        ("Updated Jan 1, 2026\n\n## Comprehensive Rules\n", "preamble"),
        ("Ver. 1.0.0\n\n## Comprehensive Rules\n", "preamble"),
        ("Ver. 1.0.0\nUpdated Foo 1, 2026\n## Comprehensive Rules\n", "month"),
        (MINIMAL_PREAMBLE + "### 1) A\n\nstray paragraph\n", "unrecognised rules line"),
        (MINIMAL_PREAMBLE + "> Ex: orphan\n", "example before any rule"),
        (MINIMAL_PREAMBLE + "### 1) A\n\n**1-1.** x\n\n**1-1.** y\n", "duplicate"),
        (MINIMAL_PREAMBLE + "### 1) A\n\n**1-2-1.** x\n", "parent"),
        (MINIMAL_PREAMBLE + "### 99) A\n", "feature area"),
    ],
)
def test_parse_errors(markdown: str, message: str) -> None:
    with pytest.raises(RulesIndexError, match=message):
        parse_rules(markdown)


def test_na_for_unknown_rule_is_rejected() -> None:
    with pytest.raises(RulesIndexError, match="unknown rule ids: 1-9"):
        parse_rules(MINIMAL_PREAMBLE + "### 1) A\n", {"1-9": "reason text long enough"})


@pytest.mark.parametrize(
    ("payload", "message"),
    [
        ([], "JSON object"),
        ({"schema_version": 2, "rules": {}}, "schema_version"),
        ({"schema_version": 1, "rules": []}, "'rules' must be an object"),
        ({"schema_version": 1, "rules": {"1-x": {"status": "na", "reason": "r"}}}, "malformed"),
        ({"schema_version": 1, "rules": {"1": {"status": "na"}}}, "exactly status and reason"),
        ({"schema_version": 1, "rules": {"1": {"status": "ok", "reason": "r"}}}, "status"),
        ({"schema_version": 1, "rules": {"1": {"status": "na", "reason": " "}}}, "non-empty"),
    ],
)
def test_na_payload_validation(payload: object, message: str) -> None:
    with pytest.raises(RulesIndexError, match=message):
        parse_na_reasons(payload)
