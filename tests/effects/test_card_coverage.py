"""Criterion 5: every card number has an implemented, tested effect."""

from __future__ import annotations

from pathlib import Path

import pytest

from gcg_sim.effects.registry import get_registry
from gcg_sim.tools.marks import scan, values

TESTS = Path(__file__).resolve().parents[1]


def _is_vanilla(number: str) -> bool:
    reg = get_registry()
    entry = reg.cards[reg.db[number].def_id]
    return entry.script is not None and entry.script.source == "vanilla"


def test_every_card_is_implemented() -> None:
    reg = get_registry()
    missing = [
        f"{reg.db.by_id(c.def_id).card_number}: {c.error}"
        for c in reg.cards
        if c.script is None and not reg.db.by_id(c.def_id).card_number.startswith("TOKEN:")
    ]
    assert not missing, f"{len(missing)} unimplemented cards, e.g. {missing[:10]}"


def test_every_non_vanilla_card_has_a_behaviour_test() -> None:
    reg = get_registry()
    tested = values(scan(TESTS, ("card",)), "card")
    untested = sorted(
        c.card_number
        for c in reg.db.real_cards()
        if not _is_vanilla(c.card_number) and c.card_number not in tested
    )
    assert not untested, (
        f"{len(untested)} cards lack a @pytest.mark.card test, e.g. {untested[:20]}"
    )


def test_card_tags_name_real_cards() -> None:
    reg = get_registry()
    unknown = sorted(v for v in values(scan(TESTS, ("card",)), "card") if reg.db.get(v) is None)
    assert not unknown, f"tests tag unknown card numbers: {unknown}"


@pytest.mark.parametrize(
    "number",
    sorted(c.card_number for c in get_registry().db.real_cards() if _is_vanilla(c.card_number)),
)
def test_vanilla_cards_have_no_abilities(number: str) -> None:
    reg = get_registry()
    cdef = reg.db[number]
    entry = reg.cards[cdef.def_id]
    assert entry.script is not None
    assert entry.script.abilities == () and entry.script.unit_abilities == ()
