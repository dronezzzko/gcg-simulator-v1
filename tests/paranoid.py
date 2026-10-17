"""Paranoid derived-view checking: every cached view that ``view.derived`` hands out is compared
with a fresh computation. Used by ``tests/rules/test_engine_caches.py`` and, for the whole
suite, by setting ``GCG_SIM_PARANOID_VIEW=1`` (see ``tests/conftest.py``)."""

from __future__ import annotations

from collections.abc import Callable

import pytest

from gcg_sim.effects import dsl as d
from gcg_sim.engine import view as V
from gcg_sim.engine.state import GameState


def view_signature(dv: V.Derived) -> tuple[object, ...]:
    """Every value of a view, rule instances and ability lists in their order."""
    return (
        dv.ap,
        dv.hp,
        dv.kw,
        dv.traits,
        {
            u: [(r.rule, r.controller, r.source, r.key, r.aux) for r in rs]
            for u, rs in dv.rules.items()
        },
        dv.cost_mod,
        dv.level_mod,
        sorted(dv.linked),
        {u: [a.aid for a in ab] for u, ab in dv.abilities.items()},
    )


def index_signature(idx: dict[d.Ev, list[tuple[int, V.AbilityEntry]]]) -> dict[d.Ev, list[object]]:
    return {ev: [(u, a.aid) for u, a in hosts] for ev, hosts in idx.items()}


class ViewChecker:
    """Wraps ``view.derived``; records each cached view (or its trigger index) that differs
    from a fresh computation of the same state."""

    def __init__(self) -> None:
        self.hits = 0
        self.stale: list[str] = []
        self._original: Callable[[GameState], V.Derived] = V.derived

    def install(self, monkeypatch: pytest.MonkeyPatch) -> None:
        monkeypatch.setattr(V, "derived", self.derived)

    def derived(self, st: GameState) -> V.Derived:
        before = st._derived
        dv = self._original(st)
        if dv is not before:
            return dv
        self.hits += 1
        fresh = V._compute(st)
        if view_signature(fresh) != view_signature(dv):
            self.stale.append(f"view, turn {st.turn}, step {st.step.name}")
        if dv.by_event is not None and index_signature(
            V.trigger_index(st, fresh)
        ) != index_signature(dv.by_event):
            self.stale.append(f"trigger index, turn {st.turn}, step {st.step.name}")
        return dv
