"""W-63, W-67: the port's general templates reproduce their binding cards byte for byte.

``port/oracle/templates/check.py --check`` is the gate; this test pins its result, so a
template or binding change that breaks a reproduction fails here too.

    uv run pytest port/oracle/tests -q
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

import pytest

ORACLE = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ORACLE))

import templates as T  # noqa: E402
from templates import check  # noqa: E402

from gcg_sim.effects import dsl as d  # noqa: E402
from gcg_sim.effects.compiler import grammar  # noqa: E402
from gcg_sim.effects.compiler.abilities import CompileError  # noqa: E402
from gcg_sim.effects.registry import get_registry  # noqa: E402

EXPECTED = {
    "pair-from-trash-after-main": ["GD05-112", "GD05-113", "GD05-121", "GD05-122"],
    "reduce-received-damage-this-turn": ["GD05-094", "GD05-125"],
    "mill-top-of-deck": ["EB01-050", "GD02-127", "GD03-092", "GD03-094"],
    "placed-trait-card-with-this-effect": ["GD03-092", "GD03-094"],
    "placed-card-level-with-this-effect": ["EB01-050"],
    "cant-activate-blocker": ["GD05-127"],
    "exile-this-card-in-trash": [],
    "links-subject-is-it": ["EB01-086", "GD05-127", "ST06-015"],
    "rested-self-is-it": ["GD04-041"],
    "all-enemy-players-each-choose": ["EB01-009", "GD05-049"],
    "all-players-each-choose-resources": ["EB01-059"],
    "they-may-draw": ["EB01-062"],
    "deploy-either-inline-token": ["ST04-012"],
    "if-they-draw-with-this-effect": ["EB01-062"],
    "ex-resource-command-trigger": ["GD04-020"],
    "each-player-in-turn-order-looks-at-top": ["EB01-023"],
    "delayed-during-this-turn-when": ["GD04-002"],
    "reveal-one-return-it-to-the-top": ["EB01-067"],
    "are-deployed-rested": [],
    "selector-suffix-lowest-hp": ["GD03-049"],
}
REVIEWED = ["EB01-074", "EB01-078", "GD03-120", "GD03-129", "GD04-022", "GD05-130"]


def test_templates_reproduce_their_bindings() -> None:
    doc = check.report()
    assert doc["mismatches"] == []
    assert {t["id"]: t["reproduced_bindings"] for t in doc["templates"]} == EXPECTED
    assert doc["reproduced_total"] == 24
    assert doc["compiled_cards_with_a_template_wording"] == []
    assert [r.split(":")[0] for r in doc["reviewed_differences"]] == REVIEWED
    assert sorted(check.REVIEWED_DIFFERENCES) == REVIEWED
    assert T.tables_untouched()
    committed = json.loads((ORACLE.parents[1] / "docs/port/audit/templates.json").read_text())
    assert committed == doc


@pytest.mark.parametrize("number", sorted({n for v in EXPECTED.values() for n in v}))
def test_template_script_equals_the_binding(number: str) -> None:
    reg = get_registry()
    cdef = reg.db[number]
    got, used = T.compile_card(cdef)
    assert used
    bound = reg.cards[cdef.def_id].script
    assert bound is not None
    assert T.behaviour_repr(got) == T.behaviour_repr(bound)


def test_the_compiler_tables_are_restored_after_a_compile_error() -> None:
    reg = get_registry()
    before = {name: list(getattr(grammar, name)) for name in T.TABLES.values()}
    parse_selector = getattr(grammar, T.SELECTOR_PARSER)
    with pytest.raises(CompileError):
        T.compile_card(reg.db["GD05-018"])
    assert T.tables_untouched()
    assert {name: list(getattr(grammar, name)) for name in T.TABLES.values()} == before
    assert getattr(grammar, T.SELECTOR_PARSER) is parse_selector


def test_a_template_that_changes_a_compiled_card_is_caught(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Negative control: a step template that draws one card more changes the compiled cards
    that draw, and the checker's comparison with the committed script sees it."""
    reg = get_registry()
    bogus = T.TableTemplate(
        id="bogus-draw",
        family="negative control",
        entry="W-63",
        pattern=re.compile(r"[Dd]raw \d+"),
        table="steps",
        rx=T._anchored(r"draw (\d+)"),
        build=lambda m, g: [d.Draw(int(m.group(1)) + 1)],
    )
    monkeypatch.setattr(T, "TEMPLATES", (*T.TEMPLATES, bogus))
    changed = []
    for entry in reg.cards:
        script = entry.script
        if script is None or script.source != "compiled":
            continue
        cdef = reg.db.by_id(entry.def_id)
        if not re.search(r"[Dd]raw \d+", cdef.effect):
            continue
        got, used = T.compile_card(cdef)
        if T.behaviour_repr(got) != T.behaviour_repr(script):
            changed.append(cdef.card_number)
            assert "bogus-draw" in used
        if len(changed) == 3:
            break
    assert len(changed) == 3
    assert T.tables_untouched()


def test_a_reviewed_difference_that_no_longer_differs_fails(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Negative control: a listed card whose binding the templates now reproduce is reported."""
    reg = get_registry()
    cdef = reg.db["GD02-127"]
    got, _ = T.compile_card(cdef)
    bound = reg.cards[cdef.def_id].script
    assert bound is not None
    assert T.behaviour_repr(got) == T.behaviour_repr(bound)
    monkeypatch.setattr(
        check, "REVIEWED_DIFFERENCES", {**check.REVIEWED_DIFFERENCES, "GD02-127": "stale"}
    )
    doc = check.report()
    assert "GD02-127: listed as differing, but reproduced" in doc["mismatches"]
    mill = next(t for t in doc["templates"] if t["id"] == "mill-top-of-deck")
    assert "GD02-127" not in mill["reproduced_bindings"]
