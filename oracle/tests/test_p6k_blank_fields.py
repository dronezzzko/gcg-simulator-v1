"""W-81: gcg-api's lossy structure fields (``keyword_effects``, ``timing_markers``,
``link_refs``) shape no card definition and no compiled script.

The Python registry is the IR generator (docs/dev/PLAN.md §3), so this is the Python side of
``crates/gcg-effects/tests/p6k_ir_scans.rs::w81_blanking_keyword_effects_timing_markers_and_
link_refs_changes_nothing``: a card database built from a copy of the snapshot with the three
fields blanked builds a registry whose golden record of every card equals the packaged one.

    uv run pytest oracle/tests -q
"""

from __future__ import annotations

from gcg_sim.cards.db import CardDB, load_overrides, load_raw_printings
from gcg_sim.effects.golden import card_golden
from gcg_sim.effects.registry import build_registry, get_registry
from gcg_sim.effects.scripts import binding_module

LOSSY_FIELDS = ("keyword_effects", "timing_markers", "link_refs")


def test_blanking_the_lossy_fields_changes_no_golden_record() -> None:
    printings = load_raw_printings()
    blanked = 0
    for p in printings:
        for key in LOSSY_FIELDS:
            if key in p:
                p[key] = []
                blanked += 1
    assert blanked > 6000
    copy = build_registry(CardDB(printings, load_overrides()))
    packaged = get_registry()
    assert len(copy.db) == len(packaged.db)
    for cdef in packaged.db:
        module = binding_module(cdef.card_number)
        want = card_golden(cdef, packaged.cards[cdef.def_id], module)
        got = card_golden(copy.db.by_id(cdef.def_id), copy.cards[cdef.def_id], module)
        assert got == want, cdef.card_number
