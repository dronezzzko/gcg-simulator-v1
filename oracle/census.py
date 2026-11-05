"""Container census and executed-program census over the trace corpus (docs/dev/PLAN.md §8.6).

Replays every game trace of a tier (sets ``full``, ``vanilla``, ``fuzz``, ``coverage``,
``search``, ``baselines``, ``worlds``; ``views`` holds sampled states of those games and
``tests`` is G2's per-test set) in Python from its recorded choices, and measures at every
decision state and at the end:

- container sizes (``containers``): cards, each zone, battles, lasting and delayed effects and
  targets per lasting, pending triggers, batches and batch sizes, frames, vars per frame, list
  length per var, ints per frame, once keys and once-key length, history, effect hits, decision
  options and context, event payload length (frames and triggers), and the derived view's rule
  lists and ability lists;
- fixpoint iterations per ``view._compute`` (``fixpoint``): ``_base_view`` plus ``_copy_base``
  calls inside one compute, minus the first base view;
- per card number (``cards``): whether one of its programs ran and whether it was instantiated in
  a traced state. A real card is vanilla when its registry entry has no abilities (312 numbers;
  PLAN §9.3), else non-vanilla (840).

"Ran" is credited when a frame is pushed whose program belongs to the card of the frame's
``card_uid`` (its abilities' programs and cost programs, its delayed-trigger programs, and the
keyword programs of its printed Repair/Breach/Support), when a trigger whose program belongs to
its provider definition (``TriggerInst.ability_key[0]``) resolves, and when one of the card's
constant abilities is applied in a view (``view._apply_cont`` with rule key ``(aid, host)``).
System programs (deploy, pair, hand limit) never credit a card.

Percentiles are nearest-rank on the sorted samples: ``pq = xs[floor(q * (n - 1))]``.

    uv run python oracle/census.py [--write | --check] [--tier 1]
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import sys
from collections import Counter
from collections.abc import Callable, Iterable
from pathlib import Path
from typing import Any

import traces

from gcg_sim.effects import dsl as d
from gcg_sim.effects.registry import Registry, _delayed_steps, get_registry
from gcg_sim.engine import game as G
from gcg_sim.engine import interp as I
from gcg_sim.engine import view as V
from gcg_sim.engine.game import apply
from gcg_sim.engine.state import Action, GameState, TriggerInst
from gcg_sim.engine.types import Zone
from gcg_sim.runner.digest import state_digest

CENSUS_FORMAT = 1
GAME_SETS = ("full", "vanilla", "fuzz", "coverage", "search", "baselines", "worlds")
OUT = {1: traces.CORPUS / "census.json", 2: traces.CORPUS / "census-tier2.json"}
KEYWORD_PROGRAMS = {d.Kw.REPAIR: "repair_program", d.Kw.BREACH: "breach_program"}

Json = Any


class Census:
    """Samples per container, fixpoint iterations, and the per-card program evidence."""

    def __init__(self, reg: Registry) -> None:
        self.reg = reg
        self.samples: dict[str, list[int]] = {}
        self.fixpoint: list[int] = []
        self.ran: set[str] = set()
        self.instantiated: set[str] = set()
        self.frames_by_kind: Counter[str] = Counter()
        self.uncredited: Counter[str] = Counter()
        self.states = 0
        self.games = 0
        self._compute_depth = 0
        self._views_in_compute = 0
        self.programs_of = self._programs_of_cards()
        self.system = {i for i, p in enumerate(reg.programs) if str(p.label).startswith("system:")}

    def _programs_of_cards(self) -> dict[int, set[int]]:
        reg = self.reg
        out: dict[int, set[int]] = {}
        for entry in reg.cards:
            pids: set[int] = set()
            for aid in (*entry.own, *entry.unit):
                ab = reg.abilities[aid]
                pids.update(p for p in (ab.program_id, ab.cost_program_id) if p >= 0)
                for steps in _delayed_steps(ab.ability):
                    pid = reg._program_index.get(steps)
                    if pid is not None:
                        pids.add(pid)
            kws = reg.printed_keywords(entry.def_id)
            for kw, attr in KEYWORD_PROGRAMS.items():
                if kw in kws:
                    pids.add(getattr(reg, attr))
            if d.Kw.SUPPORT in kws:
                pids.update((reg.support_program, reg.support_cost_program))
            out[entry.def_id] = pids
        return out

    def number(self, def_id: int) -> str:
        db = self.reg.db
        return str(db.by_id(db.base_def_id(def_id)).card_number)

    def add(self, name: str, values: Iterable[int]) -> None:
        self.samples.setdefault(name, []).extend(values)

    def credit(self, def_id: int, pid: int) -> bool:
        if pid in self.programs_of.get(self.reg.db.base_def_id(def_id), set()) or pid in (
            self.programs_of.get(def_id, set())
        ):
            self.ran.add(self.number(def_id))
            return True
        return False

    # -- per state ----------------------------------------------------------------------------
    def state(self, st: GameState) -> None:
        self.states += 1
        add = self.add
        add("cards", [len(st.cards)])
        for z in Zone:
            add(f"zone.{z.name}", [len(st.zones[p][z]) for p in (0, 1)])
        add("battles", [len(st.battles)])
        add("lasting", [len(st.lasting)])
        add("delayed", [len(st.delayed)])
        add("targets_per_lasting", [len(le.targets) for le in (*st.lasting, *st.delayed)])
        add("pending_triggers", [len(st.pending_triggers)])
        add("batches", [len(st.batches)])
        add("batch_size", [len(b) for b in st.batches])
        add("frames", [len(st.frames)])
        add("vars_per_frame", [len(f.vars) for f in st.frames])
        add("var_length", [len(v) for f in st.frames for v in f.vars.values()])
        add("ints_per_frame", [len(f.ints) for f in st.frames])
        add("buf_per_frame", [len(f.buf) for f in st.frames])
        add("once_used", [len(st.once_used)])
        add("once_key_length", [len(k) for k in st.once_used])
        add("history", [len(st.history)])
        add("effect_hits", [len(st.effect_hits)])
        triggers: list[TriggerInst] = [*st.pending_triggers, *(t for b in st.batches for t in b)]
        add("event_payload", [len(f.event) for f in st.frames] + [len(t.event) for t in triggers])
        add("trigger_once_key_length", [len(t.once_key) for t in triggers])
        if st.pending is not None:
            add("decision_options", [len(st.pending.options)])
            add("decision_context", [len(st.pending.context)])
        for c in st.cards:
            self.instantiated.add(self.number(c.def_id))
        dv = V.derived(st)
        add("view_rules_per_card", [len(rs) for rs in dv.rules.values()])
        add("view_abilities_per_card", [len(ab) for ab in dv.abilities.values()])

    # -- instrumentation ----------------------------------------------------------------------
    def install(self) -> Callable[[], None]:
        saved: list[tuple[object, str, object]] = []

        def patch(obj: object, name: str, new: object) -> None:
            saved.append(
                (obj, name, vars(obj)[name] if isinstance(obj, type) else getattr(obj, name))
            )
            setattr(obj, name, new)

        compute = V._compute
        base_view = V._base_view
        copy_base = V._copy_base
        apply_cont = V._apply_cont
        push_frame = I.push_frame
        resolve_trigger = G._resolve_trigger

        def w_compute(st: GameState) -> V.Derived:
            self._compute_depth += 1
            outer = self._views_in_compute
            self._views_in_compute = 0
            try:
                return compute(st)
            finally:
                self.fixpoint.append(max(0, self._views_in_compute - 1))
                self._views_in_compute = outer
                self._compute_depth -= 1

        def w_base_view(st: GameState, granted: Any, prev: Any) -> V.Derived:
            self._views_in_compute += 1
            return base_view(st, granted, prev)

        def w_copy_base(base: V.Derived) -> V.Derived:
            self._views_in_compute += 1
            return copy_base(base)

        def w_apply_cont(
            st: GameState,
            dv: Any,
            view: Any,
            ctx: Any,
            eff: Any,
            targets: Any,
            rule_key: tuple[int, ...],
            source: int,
            granted_out: Any,
            aux: int = -1,
        ) -> None:
            if len(rule_key) == 2 and rule_key[0] >= 0:
                self.ran.add(self.number(self.reg.abilities[rule_key[0]].def_id))
            apply_cont(st, dv, view, ctx, eff, targets, rule_key, source, granted_out, aux)

        def w_push_frame(st: GameState, program_id: int, **kwargs: Any) -> Any:
            f = push_frame(st, program_id, **kwargs)
            self.frames_by_kind[str(kwargs.get("kind"))] += 1
            uid = f.card_uid if f.card_uid >= 0 else f.host
            if program_id not in self.system and not (
                uid >= 0 and self.credit(st.cards[uid].def_id, program_id)
            ):
                self.uncredited[str(kwargs.get("kind"))] += 1
            return f

        def w_resolve_trigger(st: GameState, t: TriggerInst) -> None:
            self.credit(t.ability_key[0], t.program_id)
            resolve_trigger(st, t)

        patch(V, "_compute", w_compute)
        patch(V, "_base_view", w_base_view)
        patch(V, "_copy_base", w_copy_base)
        patch(V, "_apply_cont", w_apply_cont)
        patch(I, "push_frame", w_push_frame)
        patch(G, "_resolve_trigger", w_resolve_trigger)

        def restore() -> None:
            for obj, name, old in reversed(saved):
                setattr(obj, name, old)

        return restore

    # -- report -------------------------------------------------------------------------------
    def report(self, tier: int, inputs_sha: str) -> dict[str, Json]:
        db = self.reg.db
        real = db.real_cards()
        cards = self.reg.cards
        vanilla = sorted(
            c.card_number for c in real if not cards[c.def_id].own and not cards[c.def_id].unit
        )
        non_vanilla = sorted({c.card_number for c in real} - set(vanilla))
        return {
            "format": CENSUS_FORMAT,
            "tier": tier,
            "inputs_sha256": inputs_sha,
            "games": self.games,
            "states": self.states,
            "containers": {k: stats(v) for k, v in sorted(self.samples.items())},
            "fixpoint_iterations": stats(self.fixpoint),
            "frames_by_kind": dict(sorted(self.frames_by_kind.items())),
            "uncredited_frames_by_kind": dict(sorted(self.uncredited.items())),
            "cards": {
                "real": len(real),
                "non_vanilla": len(non_vanilla),
                "vanilla": len(vanilla),
                "ran": len(self.ran & set(non_vanilla)),
                "instantiated": len(self.instantiated & {c.card_number for c in real}),
                "non_vanilla_not_ran": [n for n in non_vanilla if n not in self.ran],
                "vanilla_not_instantiated": [n for n in vanilla if n not in self.instantiated],
            },
        }


def stats(xs: list[int]) -> dict[str, int]:
    if not xs:
        return {"n": 0, "p50": 0, "p99": 0, "p999": 0, "max": 0}
    s = sorted(xs)
    n = len(s)

    def q(p: float) -> int:
        return s[math.floor(p * (n - 1))]

    return {"n": n, "p50": q(0.5), "p99": q(0.99), "p999": q(0.999), "max": s[-1]}


def game_files(tier: int) -> list[Path]:
    """The trace files of the game sets, in path order."""
    base = traces.TIER_DIRS[tier]
    return [
        p
        for p in sorted(base.rglob("*.ndjson"))
        if traces.set_of(p.relative_to(base).as_posix()) in GAME_SETS
    ]


def inputs_sha256(paths: list[Path], base: Path) -> str:
    """SHA-256 over ``path NUL sha256 LF`` of every replayed file, so the census is stale
    exactly when one of its inputs changed."""
    h = hashlib.sha256()
    for p in paths:
        h.update(f"{p.relative_to(base).as_posix()}\0{traces.sha256_file(p)}\n".encode())
    return h.hexdigest()


def run(tier: int) -> dict[str, Json]:
    reg = get_registry()
    files = game_files(tier)
    census = Census(reg)
    restore = census.install()
    try:
        for trace in (t for p in files for t in traces.read_traces(p)):
            st = traces.start_state(trace)
            census.games += 1
            for rec in trace:
                if rec["t"] == "decision":
                    census.state(st)
                    apply(st, Action.from_json(rec["chosen"]))
            census.state(st)
            outcome = trace[-1]
            if outcome["t"] != "outcome" or outcome["final_digest"] != state_digest(st):
                raise AssertionError(f"{trace[0]['id']}: the replay does not reach the outcome")
    finally:
        restore()
    return census.report(tier, inputs_sha256(files, traces.TIER_DIRS[tier]))


def executed_numbers(trace_list: list[list[dict[str, Json]]]) -> set[str]:
    """The card numbers whose programs ran while replaying ``trace_list`` (the
    executed-program part of the census only; no derived views are computed)."""
    census = Census(get_registry())
    restore = census.install()
    try:
        for trace in trace_list:
            st = traces.start_state(trace)
            for rec in trace:
                if rec["t"] == "decision":
                    apply(st, Action.from_json(rec["chosen"]))
    finally:
        restore()
    return set(census.ran)


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    mode = ap.add_mutually_exclusive_group()
    mode.add_argument("--write", action="store_true", help="write the census file")
    mode.add_argument("--check", action="store_true", help="recompute and compare")
    ap.add_argument("--tier", type=int, choices=(1, 2), default=1)
    args = ap.parse_args(argv)
    text = json.dumps(run(args.tier), indent=1, sort_keys=True) + "\n"
    out: Path = OUT[args.tier]
    if args.write:
        out.write_text(text)
        print(f"wrote {out.relative_to(traces.ROOT)}", file=sys.stderr)
        return 0
    if args.check:
        if not out.exists() or out.read_text() != text:
            print(
                f"{out.relative_to(traces.ROOT)} is stale: run census.py --write", file=sys.stderr
            )
            return 1
        print("census check passed", file=sys.stderr)
        return 0
    sys.stdout.write(text)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
