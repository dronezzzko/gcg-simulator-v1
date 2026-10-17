"""Paranoid checks of the engine's caches and shortcuts: every cached derived view that
``view.derived`` hands out is compared with a fresh computation, every event that ``emit``
skips as unlistened is shown to have no trigger candidate, every rules-management check
skipped because no damage reaches printed HP, or because the state has not changed since such a
check, is shown to change nothing, and every reference resolved without the view resolves the
same with it. Used by
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
from gcg_sim.engine.types import NO_ARG, Phase, Zone


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
    """Wraps ``view.derived``, ``core.emit``, ``core._damage_scan``,
    ``core.rules_management`` and ``view.resolve_ref``; ``checks`` counts what was verified and
    ``stale`` records every cached view, trigger index, skipped event, skipped rules check or
    view-free reference that was wrong."""

    def __init__(self) -> None:
        self.checks: Counter[str] = Counter()
        self.stale: list[str] = []
        self._derived: Callable[[GameState], V.Derived] = V.derived
        self._emit = core.emit
        self._damage_scan = core._damage_scan
        self._rules_management = core.rules_management
        self._resolve_ref = V.resolve_ref

    def install(self, monkeypatch: pytest.MonkeyPatch) -> None:
        monkeypatch.setattr(V, "derived", self.derived)
        monkeypatch.setattr(core, "emit", self.emit)
        monkeypatch.setattr(core, "_damage_scan", self.damage_scan)
        monkeypatch.setattr(core, "rules_management", self.rules_management)
        monkeypatch.setattr(V, "resolve_ref", self.resolve_ref)

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

    def damage_scan(self, st: GameState) -> tuple[bool, bool]:
        damaged, lethal = self._damage_scan(st)
        if damaged and not lethal:
            self.checks["no printed lethal damage"] += 1
            dv = V.derived(st)
            for p in (0, 1):
                for z in (Zone.BATTLE, Zone.BASE):
                    for uid in st.zones[p][z]:
                        if st.cards[uid].damage >= V.hp_of(st, dv, uid):
                            self.stale.append(f"{uid} destroyed by rules, turn {st.turn}")
        return damaged, lethal

    def rules_management(self, st: GameState) -> None:
        if st.winner is None and st.phase is not Phase.SETUP and st._rules_checked == st.version:
            self.checks["unchanged state"] += 1
            full = st.clone()
            full._rules_checked = -1
            self._rules_management(full)
            if (full.winner, full.version, full.effect_hits, len(full.pending_triggers)) != (
                st.winner,
                st.version,
                st.effect_hits,
                len(st.pending_triggers),
            ):
                self.stale.append(f"rules management skipped a change, turn {st.turn}")
        self._rules_management(st)

    def resolve_ref(self, st: GameState, ctx: V.Ctx, ref: d.Ref) -> tuple[int, ...]:
        got = self._resolve_ref(st, ctx, ref)
        if not V.selects(ref):
            self.checks["view-free reference"] += 1
            if V.resolve(st, V.derived(st), ctx, ref) != got:
                self.stale.append(f"reference {ref} resolved without the view, turn {st.turn}")
        return got
