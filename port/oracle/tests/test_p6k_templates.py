"""W-63, W-67: the port's general templates reproduce their binding cards byte for byte.

``port/oracle/templates/check.py --check`` is the gate; this test pins its result, so a
template or binding change that breaks a reproduction fails here too.

    uv run pytest port/oracle/tests -q
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

import pytest

ORACLE = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ORACLE))

import templates as T  # noqa: E402
from templates import check  # noqa: E402

from gcg_sim.effects.registry import get_registry  # noqa: E402

EXPECTED = {
    "pair-from-trash-after-main": ["GD05-112", "GD05-113", "GD05-121", "GD05-122"],
    "reduce-received-damage-this-turn": ["GD05-094", "GD05-125"],
}


def test_templates_reproduce_their_bindings() -> None:
    doc = check.report()
    assert doc["mismatches"] == []
    assert {t["id"]: t["reproduced_bindings"] for t in doc["templates"]} == EXPECTED
    assert doc["reproduced_total"] == 6
    assert doc["compiled_cards_with_a_template_wording"] == []
    committed = json.loads((ORACLE.parents[1] / "docs/port/audit/templates.json").read_text())
    assert committed == doc


@pytest.mark.parametrize("number", sorted(n for v in EXPECTED.values() for n in v))
def test_template_script_equals_the_binding(number: str) -> None:
    reg = get_registry()
    cdef = reg.db[number]
    got, used = T.compile_card(cdef)
    assert used
    bound = reg.cards[cdef.def_id].script
    assert bound is not None
    assert T.behaviour_repr(got) == T.behaviour_repr(bound)
