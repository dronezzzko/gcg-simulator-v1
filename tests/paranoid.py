"""Paranoid checks of the engine's caches and shortcuts: every cached derived view that
``view.derived`` hands out is compared with a fresh computation, and every event that ``emit``
skips as unlistened is shown to have no trigger candidate. Used by
``tests/rules/test_engine_caches.py`` and, for the whole suite, by setting
``GCG_SIM_PARANOID_VIEW=1`` (see ``tests/conftest.py``)."""

from __future__ import annotations

from collections import Counter
from collections.abc import Callable

import pytest

from gcg_sim.effects import dsl as d
from gcg_sim.engine import core
from gcg_sim.engine import view as V
from gcg_sim.engine.state import GameState
from gcg_sim.engine.types import NO_ARG


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


class CacheChecker:
    """Wraps ``view.derived`` and ``core.emit``; ``checks`` counts what was verified and
    ``stale`` records every cached view, trigger index or skipped event that was wrong."""

    def __init__(self) -> None:
        self.checks: Counter[str] = Counter()
        self.stale: list[str] = []
        self._derived: Callable[[GameState], V.Derived] = V.derived
        self._emit = core.emit

    def install(self, monkeypatch: pytest.MonkeyPatch) -> None:
        monkeypatch.setattr(V, "derived", self.derived)
        monkeypatch.setattr(core, "emit", self.emit)

    def derived(self, st: GameState) -> V.Derived:
        before = st._derived
        dv = self._derived(st)
        if dv is not before:
            return dv
        self.checks["cached view"] += 1
        fresh = V._compute(st)
        if view_signature(fresh) != view_signature(dv):
            self.stale.append(f"view, turn {st.turn}, step {st.step.name}")
        if dv.by_event is not None and index_signature(
            V.trigger_index(st, fresh)
        ) != index_signature(dv.by_event):
            self.stale.append(f"trigger index, turn {st.turn}, step {st.step.name}")
        return dv

    def emit(
        self,
        st: GameState,
        ev: d.Ev,
        subject: int = NO_ARG,
        *,
        player: int = NO_ARG,
        lki: dict[int, core.Snapshot] | None = None,
        **extra: int,
    ) -> None:
        if st.winner is None and ev not in core.listened_events(st) and not st.delayed and not lki:
            self.checks["unlistened event"] += 1
            if V.trigger_index(st, V.derived(st)).get(ev):
                self.stale.append(f"unlistened {ev.value} has candidates, turn {st.turn}")
        self._emit(st, ev, subject, player=player, lki=lki, **extra)
