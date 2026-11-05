"""Per-test differential traces for the Rust engine (docs/dev/PLAN.md §8.1-§8.4, A16, T-157).

A pytest plugin that records, for every rule, card, effects and AI test, what the Rust engine must
reproduce through `gcg-diff`: the `testkit.Scenario` calls and the canonical state (or its
digest) before `advance` in `Scenario.start`, or the `new_game` spec; every `apply` made by test code with the
pending decision and the state digests around it, direct calls into engine functions, the view
signature after each apply, the view and testkit queries in call order, the states a test wrote
directly, and the script overrides. The record format is version 1 of PLAN §8.2
(`crates/gcg-diff/src/contracts.rs` documents the JSON keys).

    uv run python oracle/pytest_trace.py --write      # PYTHONHASHSEED=1 and =2 (W-150)
    uv run python oracle/pytest_trace.py --check
    uv run python oracle/pytest_trace.py --selftest   # T-157 argument encoder round trip
    PYTHONHASHSEED=1 uv run python -m pytest tests/rules tests/cards tests/effects tests/ai \\
        -p oracle.pytest_trace --trace-out DIR
    uv run python -m pytest tests/rules tests/cards tests/effects tests/ai -q \\
        -p oracle.pytest_trace --trace-check oracle/corpus/v1/tier1/tests

`python -m pytest` (or `uv run pytest -o pythonpath=.`) puts the repository root on `sys.path`,
which `-p oracle.pytest_trace` needs. The run is one process without `-n`, so registry
numbering is reproducible (A16). Tests that override a card script (`testkit.card_text`,
`registry.override_script`) are found statically, deselected from that process, and each runs
afterwards in a fresh process, where the plugin checks that every AI memo table is empty when
the override starts, and records the pre-override entries the test reads if one is not. A call
of `override_script` in the shared process is a fatal error, so the scan cannot miss one.

The plugin only observes: the wrappers call the original functions with the original
arguments, view signatures are computed without storing a view, and nothing is written to the
game state. Only calls whose immediate caller is test code (`tests.*`, or `gcg_sim.testkit` for
`apply`) are recorded; engine and AI internals never are. Tests without any engine record are
listed as untraced in the manifest.

Encoding (beyond PLAN §8.2):

- Script overrides travel as IR fragments in B1's format (`oracle/export_ir.py`
  `build_fragment`, `FRAGMENT_VERSION` 1, PLAN §3.4), which `Registry::with_fragment` applies to
  the embedded base registry: `{"t": "override", "kind": "override", "fragment": ..., "memo":
  ...}` after `registry.override_script` (with the AI memo-table check of A16 in `memo`), and
  `{"t": "override", "kind": "restore", "fragment": ...}` after `registry.restore_entry`.
- Every DSL object, filter tuple and program (a `Program`, or a Python program id passed as a
  parameter named `program_id`) a test passes to a recorded call, and every program a test
  module registers with `Registry.program`, is interned into one fragment arena
  per test (`FragmentBuilder`): a node is `{"$": NodeId}` and a program `{"enum": "Program",
  "value": ProgramId}`, numbered as `Registry::with_fragment` numbers them (the base's own ids
  for objects the base IR holds, new ones after the base's otherwise). The arena travels in an
  `{"t": "override", "kind": "additions", "fragment": ...}` record right after the header, and in
  every override and restore fragment of the test, so the ids mean the same everywhere.
  Interned filter tuples also join the fragment's `filter_sets`.
- Engine objects passed by reference are `{"enum": "<Type>", "value": ...}`: `GameState`
  (value: its digest16; it is the replay's current state, or `"scenario"` for the state of a
  Scenario not started yet, whose calls then join the scenario's `calls`), `Derived`
  (`"current"`: the cached view of the current state; a probe result is the sha16 of its view
  signature), `CardDef` and `AbilityEntry` (def id, aid), `Frame` (index in the current
  state's frames, -1 for none), `Registry` (`"current"`), `Program` (above). `Ctx` is
  `[controller, host, card_uid, vars, event]`; dicts are lists of `[key, value]` pairs in
  insertion order; sets are sorted lists; floats are `float.hex()` strings; an `Action` or a
  `Decision` is its `to_json()`.
- Process-local registry numbers never appear: program, continuous-effect and filter-set
  indices become content ids (`runner/digest.py:26-27`), as in `canonical_state`. Embedded
  canonical states keep Python's numbers as `canonical_state` writes them.
- Calls are named `<module>.<function>` after the module the test reached the function through
  (`view.ap_of`, `interp.push_frame`, `game.advance`); testkit queries are `testkit.<function>`
  and Scenario methods `Scenario.<method>`.
- The `scenario` record adds `before16` (the digest of the state before `advance` in
  `Scenario.start`), `after16` (the digest after `advance`, or after `new_game`) and
  `calls_reproduce` (false when the test wrote the scenario's state between recorded calls).
  Like the game traces (A21), it stores the full pre-advance snapshot (`state`, `continuous`,
  `filter_sets`) only where the digest cannot be rebuilt from the calls (`calls_reproduce`
  false), or in every record with `--trace-snapshots all`; `rules_checked` (of the pre-advance
  state) is always present. A `new_game` record carries its spec, `after16` and the new game's
  `rules_checked`; the spec adds `validate`. A `new_game` that raised is a `call` record named
  `game.new_game` with the spec in `new_game`. A call or probe that raised carries `error`; a
  failed Scenario call stays in `calls` with its `error`; a view signature that raised is
  `{"t": "view", "sig": null, "error": ...}`.
"""

from __future__ import annotations

import argparse
import dataclasses
import enum
import functools
import hashlib
import inspect
import json
import os
import platform
import re
import shutil
import subprocess
import sys
import tempfile
import types
import typing
from collections.abc import Callable, Generator, Iterator
from contextlib import contextmanager
from pathlib import Path
from typing import Any

import pytest

ROOT = Path(__file__).resolve().parents[1]
IR_PATH = ROOT / "crates" / "gcg-effects" / "ir" / "registry.json"
DEFAULT_OUT = ROOT / "oracle" / "corpus" / "v1" / "tier1" / "tests"
MANIFEST = "MANIFEST-tests.json"
TRACE_VERSION = 1
MAX_FILE_BYTES = 8 * 1024 * 1024
SUITES = ("tests/rules", "tests/cards", "tests/effects", "tests/ai")
U53 = 1 << 53
TAG_MARKS = ("rule", "card", "faq", "ruling")
# `{"enum": <name>, "value": ...}` names of engine objects passed by reference (`TestTrace.enc`);
# no engine or DSL enum may carry one of them.
ENGINE_REFS = frozenset(
    {"GameState", "Derived", "CardDef", "AbilityEntry", "Frame", "Registry", "Program"}
)

# Engine modules whose functions test code calls directly (PLAN §8.2 `call` and `probe`).
ENGINE_MODULES = {
    "gcg_sim.engine.core": "core",
    "gcg_sim.engine.interp": "interp",
    "gcg_sim.engine.battle": "battle",
    "gcg_sim.engine.observe": "observe",
    "gcg_sim.engine.game": "game",
    "gcg_sim.engine.invariants": "invariants",
    "gcg_sim.engine.view": "view",
}
PROBE_MODULES = frozenset({"view"})
# testkit queries (testkit.py:224-436), recorded as probes.
TESTKIT_PROBES = (
    "options",
    "has_action",
    "zone_of",
    "ap",
    "hp",
    "keywords",
    "uids_in",
    "numbers_in",
    "legal_kinds",
    "names",
    "card_numbers",
)
SCENARIO_METHODS = (
    "__init__",
    "add",
    "add_raw",
    "hand",
    "trash",
    "shields",
    "deck",
    "resources",
    "base",
    "resource_deck",
    "start",
)
# Test modules that instrument the engine themselves (tests/paranoid.py wraps engine functions
# for the paranoid checker); wrapping their names again would record the checker's own calls.
UNINSTRUMENTED = frozenset({"tests.paranoid", "tests.conftest"})
# The AI's process-global memo tables (A16) and the layout of their keys: "program" is a
# process-local program id (written as its content id); every other part is written as is.
MEMO_TABLES: tuple[tuple[str, str, tuple[str, ...]], ...] = (
    ("gcg_sim.ai.targeting", "_cache", ("program", "pc", "rule_table")),  # targeting.py:54
    ("gcg_sim.ai.targeting", "_battle_cache", ("program", "pc")),  # targeting.py:55
    ("gcg_sim.ai.targeting", "_harm_cache", ("def_id",)),  # targeting.py:93
    ("gcg_sim.ai.actions", "_rest_only", ("program", "pc")),  # actions.py:167
)
HASH_SEED = re.compile(r'"hash_seed":\d+')


class TracePluginError(Exception):
    """The plugin met something it cannot record faithfully; no corpus may be written."""


def sha16(text: str) -> str:
    return hashlib.sha256(text.encode()).hexdigest()[:16]


def jint(x: int) -> int | str:
    """PLAN §8.2: ints are JSON numbers, except values of 2^53 and above (decimal strings)."""
    return str(x) if x >= U53 or x <= -U53 else x


def line(obj: object) -> str:
    """One trace line: compact JSON, sorted keys, ASCII (PLAN §8.2)."""
    return json.dumps(obj, sort_keys=True, separators=(",", ":"), ensure_ascii=True)


@functools.cache
def G() -> Any:
    """The gcg_sim modules the plugin reads, imported on first use."""
    from gcg_sim import testkit
    from gcg_sim.cards.model import CardDef
    from gcg_sim.effects import dsl, program, registry
    from gcg_sim.engine import core, game, interp, state, view
    from gcg_sim.runner import digest

    return types.SimpleNamespace(
        CardDef=CardDef,
        dsl=dsl,
        program=program,
        registry=registry,
        core=core,
        game=game,
        interp=interp,
        state=state,
        view=view,
        digest=digest,
        testkit=testkit,
    )


@functools.cache
def X() -> Any:
    """`oracle/export_ir.py` (B1), whose fragment format (`build_fragment`, PLAN §3.4) and
    node encoders the override records use; imported on first use."""
    here = str(Path(__file__).resolve().parent)
    if here not in sys.path:
        sys.path.insert(0, here)
    import export_ir

    return export_ir


@functools.cache
def base_ir() -> dict[str, Any]:
    """The committed IR (`crates/gcg-effects/ir/registry.json`) the fragments extend."""
    ir: dict[str, Any] = json.loads(IR_PATH.read_text(encoding="utf-8"))
    return ir


@functools.cache
def _base_tables() -> tuple[Any, dict[str, int], dict[str, int]]:
    """export_ir's catalog, and the base IR's node and program ids by content id."""
    ir = base_ir()
    nodes = {cid: i for i, cid in enumerate(ir["node_content_ids"])}
    if len(nodes) != len(ir["node_content_ids"]):
        raise TracePluginError("base IR node content ids are not unique")
    programs = {p["content_id"]: i for i, p in enumerate(ir["programs"])}
    return X().Catalog(), nodes, programs


def registry_snapshot(reg: Any) -> Any:
    """The registry tables a fragment reads, as they are now: an override or a restore
    replaces cards and aliases and appends abilities, and the printed-keyword cache fills
    lazily (`registry.py:89-113`, `:209-243`)."""
    return dataclasses.replace(
        reg,
        programs=list(reg.programs),
        abilities=list(reg.abilities),
        cards=list(reg.cards),
        aliases=dict(reg.aliases),
        _printed_kw=dict(reg._printed_kw),
    )


def _members(hint: Any) -> list[type]:
    if isinstance(hint, typing.TypeAliasType):
        return _members(hint.__value__)
    args = typing.get_args(hint)
    if args:
        return [t for a in args for t in _members(a)]
    return [hint] if isinstance(hint, type) else []


@functools.cache
def _union(alias_name: str) -> tuple[type, ...]:
    """The classes of a DSL union alias such as `Continuous` or `Filter` (`effects/dsl.py`)."""
    return tuple(_members(getattr(G().dsl, alias_name)))


def _is_node(v: object) -> bool:
    return (
        dataclasses.is_dataclass(v)
        and not isinstance(v, type)
        and type(v).__module__ in ("gcg_sim.effects.dsl", "gcg_sim.effects.program")
    )


def _fields(obj: object) -> tuple[dataclasses.Field[Any], ...]:
    return dataclasses.fields(obj)  # type: ignore[arg-type]


# ---------------------------------------------------------------------------------------------
# IR fragments (B1's format, PLAN §3.4)


class FragmentBuilder:
    """The IR fragments of one test in B1's format (`export_ir.build_fragment`,
    `FRAGMENT_VERSION` 1, read by `Registry::with_fragment`), built with export_ir's own
    encoders (`Arena`, `ability_json`, `card_json` and the registry tables).

    Every DSL object, program and filter tuple the test passes to a recorded call is interned
    while the test runs, so the `NodeId` and `ProgramId` written into the call are final: what
    the base IR holds keeps the base's id, and anything else is numbered after the base's in
    first-use order and never renumbered. Every fragment of the test is built from this one
    arena when the test ends (from the registry as it was at its record), so an id means the
    same node or program in all of them, and each fragment holds every node interned so far.
    Two things `build_fragment` does not express need this: the test's own objects, and the
    abilities an earlier override of the process left behind (Python's aids keep counting,
    `registry.py:242`), which a fragment lists so that every aid stays Python's. A fragment
    without either is checked against `build_fragment` itself."""

    def __init__(self) -> None:
        cat, node_ids, program_ids = _base_tables()
        self.base = base_ir()
        self.arena = X().Arena(cat, base_cids=node_ids)
        self.prog_by_cid = dict(program_ids)
        self.programs: list[dict[str, Any]] = []
        self.filter_tuples: list[tuple[object, ...]] = []

    def _do(self, fn: Callable[[], Any]) -> Any:
        try:
            return fn()
        except X().ExportError as exc:
            raise TracePluginError(f"IR fragment: {exc}") from exc

    def node(self, obj: object) -> int:
        """The `NodeId` of a DSL or instruction node."""
        got: int = self._do(lambda: self.arena.intern(obj))
        return got

    def program(self, prog: Any) -> int:
        """The IR `ProgramId` of a program, by content id (`runner/digest.py:26-27`)."""
        cid = sha16(repr(prog))
        got = self.prog_by_cid.get(cid)
        if got is None:
            got = len(self.base["programs"]) + len(self.programs)
            self.prog_by_cid[cid] = got
            self.arena.strings.add(prog.label)
            instrs = [self.node(i) for i in prog.instrs]
            self.programs.append({"label": prog.label, "content_id": cid, "instrs": instrs})
        return got

    def filter_set(self, filters: tuple[object, ...]) -> list[int]:
        """The `NodeId`s of a filter tuple, which joins `filter_sets` (PLAN §8.2) unless the
        base or an `ApplyPlayer` node of the arena holds it already."""
        ids = [self.node(f) for f in filters]
        if filters not in self.filter_tuples:
            self.filter_tuples.append(filters)
        return ids

    def fragment(self, reg: Any, def_ids: list[int]) -> dict[str, Any]:
        """The fragment that turns the base IR into `reg` (a `registry_snapshot`), whose cards
        `def_ids` an override replaced: `build_fragment`'s body over this builder's arena, with
        every ability after the base's."""
        frag: dict[str, Any] = self._do(lambda: self._fragment(reg, def_ids))
        return frag

    def _fragment(self, reg: Any, def_ids: list[int]) -> dict[str, Any]:
        x = X()
        base = self.base
        a = self.arena
        plain = not (a.nodes or a.strings or a.var_names or self.programs or self.filter_tuples)
        n_base = len(base["abilities"])

        def program_id(py_id: int) -> int:
            return py_id if py_id < 0 else self.program(reg.programs[py_id])

        own: list[int] = []
        for def_id in def_ids:
            entry = reg.cards[def_id]
            if entry.script is None:
                x.fail(f"{entry.card_number}: override without a script")
            for aid in (*entry.own, *entry.unit):
                if aid < n_base:
                    x.fail(f"{entry.card_number}: an override entry reuses base ability {aid}")
                own.append(aid)
        plain = plain and own == list(range(n_base, len(reg.abilities)))
        abilities = []
        for aid in range(n_base, len(reg.abilities)):
            ab = reg.abilities[aid]
            if ab.aid != aid:
                x.fail(f"ability {aid} has aid {ab.aid}")
            rec = x.ability_json(a, ab)
            rec["program_id"] = program_id(rec["program_id"])
            rec["cost_program_id"] = program_id(rec["cost_program_id"])
            abilities.append(rec)
        cards = [
            x.card_json(reg, a, reg.cards[d], x.effective_printed_keywords(reg, d), program_id)
            for d in def_ids
        ]
        replaced = set(def_ids)
        events = []
        printed_kw = []
        for c in reg.cards:
            evs = sorted(str(e.value) for e in reg.events_by_def.get(c.def_id, ()))
            if evs != base["cards"][c.def_id]["events"]:
                events.append([c.def_id, evs])
            if c.def_id not in replaced:
                kws = x.printed_keywords_json(x.effective_printed_keywords(reg, c.def_id))
                if kws != base["cards"][c.def_id]["printed_keywords"]:
                    printed_kw.append([c.def_id, kws])
        base_aliases = dict(base["aliases"])
        aliases = [
            [def_id, [a.string(n, ("Registry", "aliases")) for n in names]]
            for def_id, names in sorted(reg.aliases.items())
            if list(names) != base_aliases.get(def_id)
        ]
        ids = a.local_ids()
        listed = {f["content_id"] for f in base["filter_sets"]}
        filter_sets = [f for f in x.filter_set_table(a, ids) if f["content_id"] not in listed]
        listed |= {f["content_id"] for f in filter_sets}
        for fs in self.filter_tuples:
            cid = sha16(repr(fs))
            if cid not in listed:
                listed.add(cid)
                filter_sets.append({"filters": [a.intern(f) for f in fs], "content_id": cid})
        strings = sorted(a.strings - set(base["strings"]))
        var_names = sorted(a.var_names - set(base["var_names"]))
        for text in [*strings, *var_names]:
            x.check_text(text, "fragment")
        out = {
            "fragment_version": x.FRAGMENT_VERSION,
            "base_fingerprint": x.registry_fingerprint(base),
            "printable_non_ascii": x.non_ascii([*strings, *var_names]),
            "strings": strings,
            "var_names": var_names,
            "nodes": list(a.nodes),
            "node_content_ids": [sha16(r) for r in a.reprs],
            "programs": list(self.programs),
            "abilities": abilities,
            "cards": cards,
            "continuous": x.continuous_table(a, ids),
            "filter_sets": filter_sets,
            "rule_mods": x.rule_mod_table(a, ids),
            "aliases": aliases,
            "always_events": sorted(str(e.value) for e in reg.always_events),
            "events": events,
            "hp_reduction_possible": reg.hp_reduction_possible,
            "printed_keywords": printed_kw,
        }
        if plain and line(out) != line(x.build_fragment(reg, base, def_ids)):
            raise TracePluginError("the fragment differs from export_ir.build_fragment's")
        return out


def inline(obj: object) -> Any:
    """A DSL node as nested JSON (the snapshot side tables): the §3.3 field encoding with nested
    nodes written in place."""
    if obj is None or isinstance(obj, bool):
        return obj
    if isinstance(obj, enum.Enum):
        return obj.value
    if isinstance(obj, (str, int)):
        return obj
    if isinstance(obj, tuple):
        return [inline(x) for x in obj]
    if _is_node(obj):
        out: dict[str, Any] = {"_": type(obj).__name__}
        for f in _fields(obj):
            out[f.name] = inline(getattr(obj, f.name))
        return out
    raise TracePluginError(f"cannot encode a {type(obj).__name__} inline")


def snapshot(st: Any) -> dict[str, Any]:
    """The canonical state and the side tables `canon::from_canonical` needs (StateSnapshot)."""
    g = G()
    reg = g.registry.get_registry()
    canon = g.digest.canonical_state(st, reg)
    cont: dict[str, Any] = {}
    fsets: dict[str, Any] = {}
    for le in (*st.lasting, *st.delayed):
        c = reg.continuous[le.effect_key]
        cont[g.digest.content_id(c)] = inline(c)
        if le.filters_key >= 0:
            fs = reg.filter_sets[le.filters_key]
            fsets[g.digest.content_id(fs)] = [inline(f) for f in fs]
    return {
        "state": canon,
        "continuous": dict(sorted(cont.items())),
        "filter_sets": dict(sorted(fsets.items())),
        "rules_checked": st._rules_checked,
    }


def digest16_of(canon: dict[str, Any]) -> str:
    """The first 16 hex digits of `state_digest` (`runner/digest.py:52-56`)."""
    blob = json.dumps(canon, sort_keys=True, separators=(",", ":")).encode()
    return hashlib.sha256(blob).hexdigest()[:16]


def _rule_key(key: tuple[int, ...]) -> list[Any]:
    """A view rule key with a lasting effect's process-local number written as its content id
    (`engine/view.py:451`)."""
    if len(key) >= 2 and key[0] == -1:
        reg = G().registry.get_registry()
        return [-1, G().digest.content_id(reg.continuous[key[1]]), *key[2:]]
    return list(key)


def view_sig(dv: Any) -> dict[str, Any]:
    """`tests/paranoid.py:24-39`'s view signature as JSON: per-uid maps sorted by uid, list
    values in their order, rule nodes by content id, keyword maps sorted by keyword."""

    def per_uid(m: dict[int, Any], fn: Callable[[Any], Any]) -> list[Any]:
        return [[u, fn(m[u])] for u in sorted(m)]

    return {
        "ap": per_uid(dv.ap, lambda v: v),
        "hp": per_uid(dv.hp, lambda v: v),
        "kw": per_uid(dv.kw, lambda m: sorted([k.value, n] for k, n in m.items())),
        "traits": per_uid(dv.traits, list),
        "rules": per_uid(
            dv.rules,
            lambda rs: [
                [sha16(repr(r.rule)), r.controller, r.source, _rule_key(r.key), r.aux] for r in rs
            ],
        ),
        "cost_mod": per_uid(dv.cost_mod, lambda v: v),
        "level_mod": per_uid(dv.level_mod, lambda v: v),
        "linked": sorted(dv.linked),
        "abilities": per_uid(dv.abilities, lambda ab: [a.aid for a in ab]),
    }


def effective_view(st: Any) -> Any:
    """The view the next `derived(st)` returns, computed without storing it
    (`engine/view.py:232-239`)."""
    v = G().view
    dv = st._derived
    if dv is not None and dv.key == v._view_key(st):
        return dv
    return v._compute(st)


def _caller_module(depth: int = 2) -> str:
    name = sys._getframe(depth).f_globals.get("__name__", "")
    return name if isinstance(name, str) else ""


def _is_test_module(name: str) -> bool:
    return name.startswith("tests.") and name not in UNINSTRUMENTED


def _error(exc: BaseException) -> dict[str, str]:
    return {"type": type(exc).__name__, "message": str(exc)}


def _tags(item: pytest.Item) -> list[str]:
    tags: list[str] = []
    for m in item.iter_markers():
        if m.name in TAG_MARKS:
            for a in m.args:
                vals = a if isinstance(a, (list, tuple)) else [a]
                tags.extend(f"{m.name}:{v}" for v in vals)
    return tags


# ---------------------------------------------------------------------------------------------
# one test's trace


class TestTrace:
    """The records of one test, buffered until the test ends."""

    __test__ = False  # not a pytest test class

    def __init__(self, plugin: TracePlugin, nodeid: str, tags: list[str]) -> None:
        self.plugin = plugin
        self.nodeid = nodeid
        self.tags = tags
        self.records: list[dict[str, Any]] = []
        self.builder: FragmentBuilder | None = None
        self.cur: Any = None
        self.cur_digest = ""
        self.max_uid = -1
        self.engine_records = 0
        self.scenarios: dict[int, dict[str, Any]] = {}
        self.force_link: list[str] | None = None
        self.memo_reads: list[dict[str, Any]] = []
        # Override and restore records, each with the registry as it was and the replaced
        # def ids; their fragments are built when the test ends (`finish`).
        self.overrides: list[tuple[dict[str, Any], Any, list[int]]] = []
        self.pre_override: Any = None

    @property
    def frags(self) -> FragmentBuilder:
        if self.builder is None:
            self.builder = FragmentBuilder()
        return self.builder

    # -- state tracking ----------------------------------------------------------------------
    def digest(self, st: Any) -> tuple[dict[str, Any], str]:
        snap = snapshot(st)
        self.max_uid = max(self.max_uid, len(st.cards) - 1)
        return snap, digest16_of(snap["state"])

    def ensure_current(self, st: Any) -> str:
        """Make `st` the replay's current state. A `state` record carries any state the replay
        cannot know: another state object, or a direct write since the last record."""
        snap, d16 = self.digest(st)
        self.plugin.seen(st)
        if st is self.cur and d16 == self.cur_digest:
            return d16
        rec: dict[str, Any] = {"t": "state", **snap, "derived": "none"}
        rec["view_from"] = None
        rec["view_key"] = None
        dv = st._derived
        v = G().view
        if dv is not None and dv.key == v._view_key(st):
            fresh = v._compute(st)
            if view_sig(fresh) == view_sig(dv) and self._index_same(st, fresh, dv):
                rec["derived"] = "fresh"
            else:
                origin = self.plugin.view_origin.get(id(dv))
                if origin is None or origin[0] is not dv:
                    raise TracePluginError(
                        f"{self.nodeid}: a stale cached view of unknown origin is installed"
                    )
                rec["derived"] = "stale"
                rec["view_from"] = origin[1]
                rec["view_key"] = json.loads(json.dumps(dv.key))
        self.records.append(rec)
        self.engine_records += 1
        self.cur = st
        self.cur_digest = d16
        return d16

    @staticmethod
    def _index_same(st: Any, fresh: Any, dv: Any) -> bool:
        if dv.by_event is None:
            return True

        def sig(idx: dict[Any, Any]) -> dict[str, list[Any]]:
            return {ev.value: [[u, a.aid] for u, a in hosts] for ev, hosts in idx.items()}

        return sig(G().view.trigger_index(st, fresh)) == sig(dv.by_event)

    def note_view(self, st: Any, canon: dict[str, Any] | None = None) -> None:
        """Remember where the state's cached view was computed, for a later `stale` record."""
        dv = st._derived
        if dv is not None and id(dv) not in self.plugin.view_origin:
            state = canon if canon is not None else snapshot(st)["state"]
            self.plugin.view_origin[id(dv)] = (dv, state)

    def after(self, st: Any) -> str:
        snap, d16 = self.digest(st)
        self.cur = st
        self.cur_digest = d16
        self.note_view(st, snap["state"])
        return d16

    def scenario_of(self, st: Any) -> dict[str, Any] | None:
        for info in self.scenarios.values():
            if info["sc"].st is st:
                return info
        return None

    # -- encoding ----------------------------------------------------------------------------
    def owner_of(self, dv: Any) -> Any:
        if self.cur is not None and self.cur._derived is dv:
            return self.cur
        for st in self.plugin.states_seen():
            if st._derived is dv:
                return st
        return None

    def enc(self, v: Any, param: str = "", scenario_state: Any = None) -> Any:
        g = G()
        if v is None or isinstance(v, bool):
            return v
        if isinstance(v, enum.Enum):
            if type(v).__name__ in ENGINE_REFS:
                raise TracePluginError(
                    f"{self.nodeid}: enum {type(v).__name__} names an engine ref"
                )
            return {"enum": type(v).__name__, "value": v.value}
        if isinstance(v, str):
            return v
        if isinstance(v, int):
            if param == "program_id" and v >= 0:
                reg = g.registry.get_registry()
                return {"enum": "Program", "value": self.frags.program(reg.programs[v])}
            return jint(v)
        if isinstance(v, float):
            return v.hex()
        if isinstance(v, g.state.GameState):
            if scenario_state is not None and v is scenario_state:
                return {"enum": "GameState", "value": "scenario"}
            return {"enum": "GameState", "value": self.ensure_current(v)}
        if isinstance(v, g.view.Derived):
            owner = self.owner_of(v)
            if owner is None:
                raise TracePluginError(f"{self.nodeid}: a view of no traced state is passed")
            self.ensure_current(owner)
            return {"enum": "Derived", "value": "current"}
        if isinstance(v, (g.state.Action, g.state.Decision)):
            return v.to_json()
        if isinstance(v, g.CardDef):
            return {"enum": "CardDef", "value": v.def_id}
        if isinstance(v, g.registry.AbilityEntry):
            return {"enum": "AbilityEntry", "value": v.aid}
        if isinstance(v, g.registry.Registry):
            return {"enum": "Registry", "value": "current"}
        if isinstance(v, g.state.Frame):
            frames = self.cur.frames if self.cur is not None else []
            idx = next((i for i, f in enumerate(frames) if f is v), -1)
            return {"enum": "Frame", "value": idx}
        if isinstance(v, g.view.Ctx):
            return [
                v.controller,
                v.host,
                v.card_uid,
                None if v.vars is None else [[k, list(t)] for k, t in v.vars.items()],
                [list(e) for e in v.event],
            ]
        if isinstance(v, g.view.RuleInst):
            return [{"$": self.frags.node(v.rule)}, v.controller, v.source, _rule_key(v.key), v.aux]
        if isinstance(v, g.program.Program):
            return {"enum": "Program", "value": self.frags.program(v)}
        if _is_node(v):
            return {"$": self.frags.node(v)}
        if isinstance(v, tuple) and v and all(isinstance(x, _union("Filter")) for x in v):
            return [{"$": i} for i in self.frags.filter_set(v)]
        if isinstance(v, (tuple, list)):
            return [self.enc(x, scenario_state=scenario_state) for x in v]
        if isinstance(v, (set, frozenset)):
            return sorted((self.enc(x) for x in v), key=line)
        if isinstance(v, dict):
            return [[self.enc(k), self.enc(x, scenario_state=scenario_state)] for k, x in v.items()]
        raise TracePluginError(f"{self.nodeid}: cannot encode a {type(v).__name__}")

    def call_spec(
        self,
        name: str,
        fn: Callable[..., Any],
        args: tuple[Any, ...],
        kwargs: dict[str, Any],
        *,
        skip: int = 0,
        scenario_state: Any = None,
    ) -> dict[str, Any]:
        """`{"fn", "args", "kwargs"}` of a call; `skip` leading arguments (`self`) are bound
        but not written."""
        names = self.plugin.param_names(fn, args, kwargs)
        enc_args = [
            self.enc(a, names[i] if i < len(names) else "", scenario_state)
            for i, a in enumerate(args)
            if i >= skip
        ]
        spec: dict[str, Any] = {"fn": name, "args": enc_args}
        if kwargs:
            spec["kwargs"] = [[k, self.enc(v, k, scenario_state)] for k, v in kwargs.items()]
        return spec

    # -- records -----------------------------------------------------------------------------
    def check_force_link(self) -> None:
        """A `force_link` record whenever the monkeypatched `view.link_satisfied`
        (`tests/cards/test_wp_gd05_a.py:101-113`) names a different set of Units."""
        fn = G().view.link_satisfied
        if fn is self.plugin.originals["link_satisfied"]:
            return
        units: set[str] | None = None
        for cell in getattr(fn, "__closure__", None) or ():
            val = cell.cell_contents
            if isinstance(val, set) and all(isinstance(x, str) for x in val):
                units = val
        if units is None:
            raise TracePluginError(f"{self.nodeid}: unrecognised patch of view.link_satisfied")
        listed = sorted(units)
        if listed != self.force_link:
            self.force_link = listed
            self.records.append({"t": "force_link", "units": listed})

    def finish(self, outcome: str) -> list[str]:
        header = {
            "t": "header",
            "v": TRACE_VERSION,
            "id": self.nodeid,
            "kind": "test",
            "generator": {**self.plugin.generator, "max_uid": max(self.max_uid, 0)},
            "decks": None,
            "seed": None,
            "chooser": None,
            "turn_limit": 200,
            "max_actions": 20000,
            "agents": [],
            "agent_seeds": [],
            "preset": None,
            "options_mode": "full",
        }
        out = [line(header)]
        if self.builder is not None:
            start = self.pre_override
            if start is None:
                start = registry_snapshot(G().registry.get_registry())
            frag = self.frags.fragment(start, [])
            out.append(line({"t": "override", "kind": "additions", "fragment": frag}))
        for rec, reg, def_ids in self.overrides:
            rec["fragment"] = self.frags.fragment(reg, def_ids)
            if "memo" in rec:
                rec["memo"]["reads"] = self.memo_reads
        out.extend(line(r) for r in self.records)
        out.append(line({"t": "end", "outcome": outcome}))
        return out


# ---------------------------------------------------------------------------------------------
# module proxies


class _ModuleProxy:
    """Stands in for an engine module in a test module's namespace: functions are wrapped,
    everything else (classes, constants) passes through, and attribute writes (monkeypatch)
    go to the real module with any wrapper unwrapped."""

    def __init__(self, plugin: TracePlugin, module: types.ModuleType, short: str) -> None:
        object.__setattr__(self, "_plugin", plugin)
        object.__setattr__(self, "_module", module)
        object.__setattr__(self, "_short", short)

    def __getattr__(self, name: str) -> Any:
        module = object.__getattribute__(self, "_module")
        plugin: TracePlugin = object.__getattribute__(self, "_plugin")
        return plugin.wrap_engine(
            getattr(module, name), object.__getattribute__(self, "_short"), name
        )

    def __setattr__(self, name: str, value: Any) -> None:
        module = object.__getattribute__(self, "_module")
        setattr(module, name, getattr(value, "__gcg_trace_original__", value))

    def __delattr__(self, name: str) -> None:
        delattr(object.__getattribute__(self, "_module"), name)

    def __repr__(self) -> str:
        return f"<trace proxy of {object.__getattribute__(self, '_module').__name__}>"


def _code_names(fn: Any) -> set[str]:
    out: set[str] = set()
    code = getattr(fn, "__code__", None)
    stack = [code] if code is not None else []
    while stack:
        c = stack.pop()
        out.update(c.co_names)
        out.update(c.co_freevars)
        stack.extend(k for k in c.co_consts if isinstance(k, types.CodeType))
    return out


def overrides_script(item: pytest.Item) -> bool:
    """Static scan (A16): the test, or a fixture it uses, reaches `testkit.card_text` or
    `registry.override_script`, directly or through functions of test modules."""
    g = G()
    targets = (g.testkit.card_text, g.registry.override_script)
    fn = getattr(item, "function", None)
    if fn is None:
        return False
    roots: list[Any] = [fn]
    info = getattr(item, "_fixtureinfo", None)
    if info is not None:
        for name in getattr(item, "fixturenames", ()):
            roots.extend(fdef.func for fdef in info.name2fixturedefs.get(name, ()))
    seen: set[int] = set()
    stack = roots
    while stack:
        f = stack.pop()
        f = getattr(f, "__wrapped__", f)
        if id(f) in seen:
            continue
        seen.add(id(f))
        glob = getattr(f, "__globals__", {})
        for name in _code_names(f):
            obj = glob.get(name)
            if obj is None:
                continue
            if any(obj is t for t in targets):
                return True
            if isinstance(obj, types.FunctionType) and _is_test_module(obj.__module__):
                stack.append(obj)
    return False


# ---------------------------------------------------------------------------------------------
# the plugin


class TracePlugin:
    def __init__(self, config: pytest.Config, mode: str, target: Path) -> None:
        self.config = config
        self.mode = mode  # "out", "check" or "isolated"
        self.target = target
        self.trace: TestTrace | None = None
        self.depth = 0
        self.module_traces: dict[str, list[tuple[str, list[str]]]] = {}
        self.untraced: list[str] = []
        self.failed: list[str] = []
        self.isolated: list[pytest.Item] = []
        self.isolated_info: list[dict[str, Any]] = []
        self.collected: list[str] = []
        self.order: dict[str, int] = {}
        self.view_origin: dict[int, tuple[Any, Any]] = {}
        self.originals: dict[str, Any] = {}
        self.originals_wrapped: dict[str, Any] = {}
        self.restore: list[Callable[[], None]] = []
        self.wrappers: dict[tuple[int, str], Any] = {}
        self.reports: dict[str, list[pytest.TestReport]] = {}
        self.errors: list[str] = []
        self.generator: dict[str, Any] = {}
        self.pending_start: Any = None
        self._seen_states: list[Any] = []
        self._signatures: dict[int, list[str] | None] = {}
        self.all_snapshots = config.getoption("trace_snapshots") == "all"

    # -- generator block (Header.generator) --------------------------------------------------
    @staticmethod
    def make_generator() -> dict[str, Any]:
        from gcg_sim.reports.versions import code_digest

        res = subprocess.run(
            ["git", "log", "-1", "--format=%H", "--", "src/gcg_sim", "tests"],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=False,
        )
        ir = ROOT / "crates" / "gcg-effects" / "ir" / "registry.json"
        seed = os.environ.get("PYTHONHASHSEED", "")
        offset = os.environ.get("GCG_ORACLE_EFFECT_OFFSET", "")
        return {
            "git": res.stdout.strip(),
            "python": platform.python_version(),
            "code_digest": code_digest(),
            "ir_sha256": hashlib.sha256(ir.read_bytes()).hexdigest() if ir.is_file() else "",
            "data_lock": hashlib.sha256(
                (ROOT / "data" / "SOURCES.lock.json").read_bytes()
            ).hexdigest(),
            "hash_seed": int(seed) if seed.isdigit() else 0,
            "effect_offset": int(offset) if offset.isdigit() else 0,
        }

    # -- helpers -----------------------------------------------------------------------------
    @contextmanager
    def quiet(self) -> Iterator[None]:
        """The plugin's own work and the wrapped call itself: nothing inside is recorded."""
        self.depth += 1
        try:
            yield
        finally:
            self.depth -= 1

    def recording(self, caller: str, *, testkit: bool = False) -> TestTrace | None:
        if self.trace is None or self.depth:
            return None
        if _is_test_module(caller) or (testkit and caller == "gcg_sim.testkit"):
            return self.trace
        return None

    def states_seen(self) -> list[Any]:
        return self._seen_states

    def seen(self, st: Any) -> None:
        if not any(s is st for s in self._seen_states):
            self._seen_states.append(st)

    def param_names(self, fn: Callable[..., Any], args: Any, kwargs: Any) -> list[str]:
        key = id(fn)
        if key not in self._signatures:
            try:
                self._signatures[key] = list(inspect.signature(fn).parameters)
            except TypeError, ValueError:
                self._signatures[key] = None
        names = self._signatures[key]
        return names[: len(args)] if names is not None else []

    # -- wrappers ----------------------------------------------------------------------------
    def wrap_engine(self, value: Any, short: str, name: str) -> Any:
        if not isinstance(value, types.FunctionType) or not value.__module__.startswith("gcg_sim"):
            return value
        if short == "game" and name in ("apply", "new_game"):
            return self.originals_wrapped[name]
        key = (id(value), f"{short}.{name}")
        got = self.wrappers.get(key)
        if got is None:
            kind = "probe" if short in PROBE_MODULES else "call"
            got = self._make_wrapper(value, f"{short}.{name}", kind)
            self.wrappers[key] = got
        return got

    def _make_wrapper(self, fn: Callable[..., Any], name: str, kind: str) -> Any:
        plugin = self

        @functools.wraps(fn)
        def wrapper(*args: Any, **kwargs: Any) -> Any:
            tr = plugin.recording(_caller_module())
            if tr is None:
                return fn(*args, **kwargs)
            gs = G().state.GameState
            st = next((a for a in args if isinstance(a, gs)), None)
            info = tr.scenario_of(st) if st is not None else None
            with plugin.quiet():
                tr.check_force_link()
                spec = tr.call_spec(name, fn, args, kwargs, scenario_state=st if info else None)
            if info is not None:
                # A call on a Scenario's state before `start` belongs to the scenario's build.
                try:
                    with plugin.quiet():
                        result = fn(*args, **kwargs)
                except Exception as exc:
                    spec["error"] = _error(exc)
                    info["calls"].append(spec)
                    raise
                with plugin.quiet():
                    info["calls"].append(spec)
                    info["digest"] = tr.digest(st)[1]
                return result
            try:
                with plugin.quiet():
                    result = fn(*args, **kwargs)
            except Exception as exc:
                with plugin.quiet():
                    rec = {"t": kind, **spec, "result": None, "error": _error(exc)}
                    if kind == "call":
                        rec["after16"] = tr.after(st) if st is not None else tr.cur_digest
                    tr.records.append(rec)
                    tr.engine_records += 1
                raise
            with plugin.quiet():
                rec = {"t": kind, **spec, "result": plugin.enc_result(tr, result, st)}
                if kind == "call":
                    rec["after16"] = tr.after(st) if st is not None else tr.cur_digest
                    rec["error"] = None
                tr.records.append(rec)
                tr.engine_records += 1
            return result

        wrapper.__gcg_trace_original__ = fn  # type: ignore[attr-defined]
        return wrapper

    def enc_result(self, tr: TestTrace, result: Any, st: Any) -> Any:
        g = G()
        if isinstance(result, g.state.GameState):
            self.seen(result)
            return {"enum": "GameState", "value": tr.digest(result)[1]}
        if isinstance(result, g.view.Derived):
            if st is not None:
                tr.note_view(st)
            return {"enum": "Derived", "value": sha16(line(view_sig(result)))}
        return tr.enc(result)

    def _apply_wrapper(self, fn: Callable[..., Any]) -> Any:
        plugin = self

        @functools.wraps(fn)
        def apply(st: Any, action: Any, *, check: bool = True) -> None:
            tr = plugin.recording(_caller_module(), testkit=True)
            if tr is None:
                fn(st, action, check=check)
                return
            with plugin.quiet():
                tr.check_force_link()
                before = tr.ensure_current(st)
                pending = st.pending.to_json() if st.pending is not None else None
            err: dict[str, str] | None = None
            try:
                with plugin.quiet():
                    fn(st, action, check=check)
            except Exception as exc:
                err = _error(exc)
                raise
            finally:
                with plugin.quiet():
                    after = tr.after(st)
                    tr.records.append(
                        {
                            "t": "apply",
                            "action": action.to_json(),
                            "check": check,
                            "before16": before,
                            "after16": after,
                            "pending": pending,
                            "error": err,
                        }
                    )
                    try:
                        tr.records.append({"t": "view", "sig": view_sig(effective_view(st))})
                    except Exception as exc:
                        tr.records.append({"t": "view", "sig": None, "error": _error(exc)})
                    tr.engine_records += 1

        apply.__gcg_trace_original__ = fn  # type: ignore[attr-defined]
        return apply

    def _new_game_wrapper(self, fn: Callable[..., Any]) -> Any:
        plugin = self

        @functools.wraps(fn)
        def new_game(decks: Any, seed: int, **kwargs: Any) -> Any:
            tr = plugin.recording(_caller_module())
            if tr is None:
                return fn(decks, seed, **kwargs)
            with plugin.quiet():
                tr.check_force_link()
                spec = {
                    "decks": [{"main": list(dk.main), "res": list(dk.resources)} for dk in decks],
                    "seed": jint(seed),
                    "chooser": kwargs.get("chooser"),
                    "turn_limit": kwargs.get("turn_limit", 200),
                    "max_actions": kwargs.get("max_actions", 20000),
                    "validate": kwargs.get("validate", True),
                }
            try:
                with plugin.quiet():
                    st = fn(decks, seed, **kwargs)
            except Exception as exc:
                with plugin.quiet():
                    tr.records.append(
                        {
                            "t": "call",
                            "fn": "game.new_game",
                            "args": [],
                            "new_game": spec,
                            "result": None,
                            "after16": tr.cur_digest,
                            "error": _error(exc),
                        }
                    )
                    tr.engine_records += 1
                raise
            with plugin.quiet():
                plugin.seen(st)
                snap, d16 = tr.digest(st)
                state = snap if plugin.all_snapshots else {"rules_checked": snap["rules_checked"]}
                tr.records.append(
                    {
                        "t": "scenario",
                        "test": tr.nodeid,
                        "tags": tr.tags,
                        "calls": [],
                        "new_game": spec,
                        **state,
                        "after16": d16,
                        "calls_reproduce": True,
                    }
                )
                tr.engine_records += 1
                tr.cur = st
                tr.cur_digest = d16
                tr.note_view(st, snap["state"])
            return st

        new_game.__gcg_trace_original__ = fn  # type: ignore[attr-defined]
        return new_game

    # -- Scenario ----------------------------------------------------------------------------
    def _scenario_method(self, method: str, fn: Callable[..., Any]) -> Any:
        plugin = self

        @functools.wraps(fn)
        def wrapper(sc: Any, *args: Any, **kwargs: Any) -> Any:
            tr = plugin.recording(_caller_module())
            if tr is None:
                return fn(sc, *args, **kwargs)
            if method == "__init__":
                with plugin.quiet():
                    spec = tr.call_spec("Scenario.__init__", fn, (sc, *args), kwargs, skip=1)
                    fn(sc, *args, **kwargs)
                    tr.scenarios[id(sc)] = {
                        "sc": sc,
                        "calls": [spec],
                        "reproduce": True,
                        "digest": tr.digest(sc.st)[1],
                    }
                    plugin.seen(sc.st)
                return None
            info = tr.scenarios.get(id(sc))
            if info is None or info["sc"] is not sc:
                # Built outside this test: its state reaches the trace as a `state` record.
                return fn(sc, *args, **kwargs)
            with plugin.quiet():
                tr.check_force_link()
                if tr.digest(sc.st)[1] != info["digest"]:
                    info["reproduce"] = False
                spec = tr.call_spec(f"Scenario.{method}", fn, (sc, *args), kwargs, skip=1)
            if method == "start":
                plugin.pending_start = sc
            try:
                with plugin.quiet():
                    result = fn(sc, *args, **kwargs)
            except Exception as exc:
                spec["error"] = _error(exc)
                info["calls"].append(spec)
                raise
            finally:
                plugin.pending_start = None
            with plugin.quiet():
                info["calls"].append(spec)
                if method != "start":
                    info["digest"] = tr.digest(sc.st)[1]
                    return result
                st = result
                pre = info.pop("pre_advance")
                snap, d16 = tr.digest(st)
                if plugin.all_snapshots or not info["reproduce"]:
                    state = pre
                else:
                    state = {"rules_checked": pre["rules_checked"]}
                tr.records.append(
                    {
                        "t": "scenario",
                        "test": tr.nodeid,
                        "tags": tr.tags,
                        "calls": info["calls"],
                        "new_game": None,
                        **state,
                        "before16": digest16_of(pre["state"]),
                        "after16": d16,
                        "calls_reproduce": info["reproduce"],
                    }
                )
                tr.engine_records += 1
                tr.cur = st
                tr.cur_digest = d16
                tr.note_view(st, snap["state"])
                del tr.scenarios[id(sc)]
            return result

        wrapper.__gcg_trace_original__ = fn  # type: ignore[attr-defined]
        return wrapper

    def _testkit_advance(self, fn: Callable[..., Any]) -> Any:
        """`testkit.advance` (only `Scenario.start` calls it, `testkit.py:181`): the canonical
        state before it is the scenario snapshot."""
        plugin = self

        @functools.wraps(fn)
        def advance(st: Any) -> None:
            sc, tr = plugin.pending_start, plugin.trace
            if sc is not None and tr is not None and st is sc.st:
                info = tr.scenarios.get(id(sc))
                if info is not None:
                    info["pre_advance"] = snapshot(st)
            fn(st)

        advance.__gcg_trace_original__ = fn  # type: ignore[attr-defined]
        return advance

    # -- registry ----------------------------------------------------------------------------
    def _program_wrapper(self, fn: Callable[..., Any]) -> Any:
        """`Registry.program` (`registry.py:115-122`): a program a test module registers joins
        the arena (a delayed program the engine registers lazily is part of the base IR)."""
        plugin = self

        @functools.wraps(fn)
        def program(reg: Any, steps: Any, label: str) -> int:
            before = len(reg.programs)
            pid: int = fn(reg, steps, label)
            tr = plugin.trace
            if tr is not None and len(reg.programs) > before and _is_test_module(_caller_module()):
                with plugin.quiet():
                    tr.frags.program(reg.programs[pid])
            return pid

        program.__gcg_trace_original__ = fn  # type: ignore[attr-defined]
        return program

    def _override_wrapper(self, fn: Callable[..., Any]) -> Any:
        plugin = self

        @functools.wraps(fn)
        def override_script(reg: Any, cdef: Any, script: Any) -> Any:
            tr = plugin.trace
            if plugin.mode != "isolated":
                name = tr.nodeid if tr is not None else "(outside a test)"
                plugin.errors.append(
                    f"{name} calls override_script in the shared process, but the static scan "
                    "did not isolate it (A16)"
                )
                raise TracePluginError(plugin.errors[-1])
            if tr is None:
                return fn(reg, cdef, script)
            with plugin.quiet():
                memo = plugin.memo_check(tr)
                if tr.pre_override is None:
                    tr.pre_override = registry_snapshot(reg)
            previous = fn(reg, cdef, script)
            with plugin.quiet():
                rec = {"t": "override", "kind": "override", "fragment": None, "memo": memo}
                tr.records.append(rec)
                tr.overrides.append((rec, registry_snapshot(reg), [cdef.def_id]))
                tr.engine_records += 1
            return previous

        override_script.__gcg_trace_original__ = fn  # type: ignore[attr-defined]
        return override_script

    def _restore_wrapper(self, fn: Callable[..., Any]) -> Any:
        plugin = self

        @functools.wraps(fn)
        def restore_entry(reg: Any, entry: Any) -> None:
            fn(reg, entry)
            tr = plugin.trace
            if tr is not None and plugin.mode == "isolated":
                with plugin.quiet():
                    rec = {"t": "override", "kind": "restore", "fragment": None}
                    tr.records.append(rec)
                    tr.overrides.append((rec, registry_snapshot(reg), []))

        restore_entry.__gcg_trace_original__ = fn  # type: ignore[attr-defined]
        return restore_entry

    # -- AI memo tables (A16) ----------------------------------------------------------------
    @staticmethod
    def memo_check(tr: TestTrace) -> dict[str, Any]:
        """Whether every AI memo table is empty as the override starts; a table that is not is
        replaced by a read log of its pre-override entries (`MEMO_TABLES`)."""
        sizes: dict[str, int] = {}
        for mod_name, attr, layout in MEMO_TABLES:
            mod = sys.modules.get(mod_name)
            name = f"{mod_name}.{attr}"
            if mod is None:
                sizes[name] = 0
                continue
            table = getattr(mod, attr)
            sizes[name] = len(table)
            if table:
                setattr(mod, attr, _ReadLog(table, tr, name, layout))
        return {"empty": not any(sizes.values()), "sizes": sizes, "reads": []}

    # -- pytest hooks ------------------------------------------------------------------------
    @pytest.hookimpl(tryfirst=True)
    def pytest_collection_modifyitems(
        self, session: pytest.Session, config: pytest.Config, items: list[pytest.Item]
    ) -> None:
        self.collected = [it.nodeid for it in items]
        self.order = {n: i for i, n in enumerate(self.collected)}
        if self.mode == "isolated":
            return
        keep: list[pytest.Item] = []
        for it in items:
            (self.isolated if overrides_script(it) else keep).append(it)
        if self.isolated:
            config.hook.pytest_deselected(items=self.isolated)
            items[:] = keep

    def pytest_collection_finish(self, session: pytest.Session) -> None:
        with self.quiet():
            self.generator = self.make_generator()
            self.install()

    @pytest.hookimpl(hookwrapper=True)
    def pytest_runtest_protocol(
        self, item: pytest.Item, nextitem: pytest.Item | None
    ) -> Generator[None, Any]:
        self.trace = TestTrace(self, item.nodeid, _tags(item))
        self._seen_states = []
        self.view_origin = {}
        self.reports[item.nodeid] = []
        yield
        tr, self.trace = self.trace, None
        reports = self.reports.pop(item.nodeid, [])
        ok = bool(reports) and all(r.passed for r in reports)
        if not ok:
            self.failed.append(item.nodeid)
        module = item.nodeid.split("::", 1)[0]
        if tr.engine_records:
            outcome = "passed" if ok else "failed"
            try:
                with self.quiet():
                    lines = tr.finish(outcome)
            except TracePluginError as exc:
                self.errors.append(f"{item.nodeid}: {exc}")
                lines = []
            self.module_traces.setdefault(module, []).append((item.nodeid, lines))
        else:
            self.untraced.append(item.nodeid)
        self._seen_states = []
        self.view_origin = {}

    def pytest_runtest_logreport(self, report: pytest.TestReport) -> None:
        got = self.reports.get(report.nodeid)
        if got is not None and (report.when == "call" or not report.passed):
            got.append(report)

    @pytest.hookimpl(trylast=True)
    def pytest_sessionfinish(self, session: pytest.Session, exitstatus: int) -> None:
        self.uninstall()
        if self.mode == "isolated":
            self.write_isolated()
            return
        for item in self.isolated:
            self.run_isolated(item)
        if self.errors:
            for e in self.errors:
                print(f"\npytest_trace: {e}", file=sys.stderr)
            session.exitstatus = pytest.ExitCode.INTERNAL_ERROR
            return
        if self.failed:
            print(f"\npytest_trace: {len(self.failed)} traced test(s) failed", file=sys.stderr)
            for n in self.failed[:20]:
                print(f"  {n}", file=sys.stderr)
            session.exitstatus = pytest.ExitCode.TESTS_FAILED
            return
        meta = self.manifest_meta(session)
        if self.mode == "out":
            write_corpus(self.target, self.files(), meta)
            print(f"\npytest_trace: wrote {_rel(self.target)}", file=sys.stderr)
            return
        with tempfile.TemporaryDirectory(prefix="pytest-trace-check-") as tmp:
            fresh = Path(tmp) / "tests"
            write_corpus(fresh, self.files(), meta)
            problems = compare_corpus(self.target, fresh)
        if problems:
            for p in problems[:50]:
                print(f"pytest_trace --trace-check: {p}", file=sys.stderr)
            session.exitstatus = pytest.ExitCode.TESTS_FAILED
        else:
            print(f"\npytest_trace: {_rel(self.target)} is current", file=sys.stderr)

    # -- output ------------------------------------------------------------------------------
    def files(self) -> list[tuple[str, list[str], int]]:
        """Trace lines per output file: one file per test module, traces in collection order,
        split at MAX_FILE_BYTES; with the number of traces in each file."""
        out: list[tuple[str, list[str], int]] = []
        by_module = {
            m: sorted(traces, key=lambda t: self.order.get(t[0], 0))
            for m, traces in self.module_traces.items()
        }
        for module in sorted(by_module, key=lambda m: self.order.get(by_module[m][0][0], 0)):
            stem = module.removeprefix("tests/").removesuffix(".py")
            part, size, n = 1, 0, 0
            lines: list[str] = []
            for _, tlines in by_module[module]:
                tsize = sum(len(x) + 1 for x in tlines)
                if lines and size + tsize > MAX_FILE_BYTES:
                    out.append((_part_name(stem, part), lines, n))
                    part, size, n, lines = part + 1, 0, 0, []
                lines.extend(tlines)
                size += tsize
                n += 1
            if lines:
                out.append((_part_name(stem, part), lines, n))
        return out

    def manifest_meta(self, session: pytest.Session) -> dict[str, Any]:
        traced = sum(len(v) for v in self.module_traces.values())
        return {
            "generator": self.generator,
            "command": "uv run python oracle/pytest_trace.py --write",
            "suites": [str(a) for a in session.config.args],
            "tests": {
                "collected": len(self.collected),
                "traced": traced,
                "untraced": sorted(self.untraced, key=lambda n: self.order.get(n, 0)),
                "isolated": self.isolated_info,
            },
        }

    # -- override tests in fresh processes (A16) ---------------------------------------------
    def run_isolated(self, item: pytest.Item) -> None:
        with tempfile.TemporaryDirectory(prefix="pytest-trace-iso-") as tmp:
            result = Path(tmp) / "trace.json"
            cmd = [
                sys.executable,
                "-m",
                "pytest",
                item.nodeid,
                "-q",
                "-p",
                "no:cacheprovider",
                "-p",
                "oracle.pytest_trace",
                "--trace-isolated-out",
                str(result),
                "--trace-snapshots",
                "all" if self.all_snapshots else "needed",
            ]
            res = subprocess.run(
                cmd, cwd=self.config.rootpath, capture_output=True, text=True, check=False
            )
            if not result.is_file():
                self.errors.append(
                    f"{item.nodeid}: the isolated run wrote no trace (exit {res.returncode})\n"
                    f"{res.stdout[-3000:]}{res.stderr[-3000:]}"
                )
                return
            doc = json.loads(result.read_text())
        self.errors.extend(doc["errors"])
        if not doc["passed"]:
            self.failed.append(item.nodeid)
        memos = [
            r["memo"]
            for r in map(json.loads, doc["lines"])
            if r["t"] == "override" and r["kind"] == "override"
        ]
        info = {
            "test": item.nodeid,
            "fresh_process": True,
            "overrides": len(memos),
            "memo_empty_at_override": [m["empty"] for m in memos],
            "memo_reads": sum(len(m["reads"]) for m in memos),
        }
        if not memos:
            self.errors.append(f"{item.nodeid}: isolated, but it overrode no card script")
        self.isolated_info.append(info)
        print(
            f"pytest_trace: {item.nodeid}: fresh process, {len(memos)} override(s), AI memo "
            f"tables empty at each start: {info['memo_empty_at_override']}, pre-override "
            f"entries read: {info['memo_reads']}",
            file=sys.stderr,
        )
        module = item.nodeid.split("::", 1)[0]
        if doc["lines"]:
            self.module_traces.setdefault(module, []).append((item.nodeid, doc["lines"]))
        else:
            self.untraced.append(item.nodeid)

    def write_isolated(self) -> None:
        lines = [x for traces in self.module_traces.values() for _, tl in traces for x in tl]
        doc = {
            "lines": lines,
            "passed": not self.failed and len(self.collected) == 1,
            "errors": self.errors,
        }
        self.target.write_text(json.dumps(doc))

    # -- installation ------------------------------------------------------------------------
    def _set(self, obj: Any, name: str, value: Any) -> None:
        old = getattr(obj, name)
        setattr(obj, name, value)
        self.restore.append(lambda: setattr(obj, name, old))

    def _rebind(self, original: Any, wrapped: Any, accept: Callable[[str], bool]) -> None:
        for mod in list(sys.modules.values()):
            mname = getattr(mod, "__name__", None) if mod is not None else None
            if not isinstance(mname, str) or not accept(mname):
                continue
            for attr, val in list(vars(mod).items()):
                if val is original:
                    self._set(mod, attr, wrapped)

    def install(self) -> None:
        g = G()
        tk = g.testkit
        self.originals = {
            "apply": g.game.apply,
            "new_game": g.game.new_game,
            "link_satisfied": g.view.link_satisfied,
            "override_script": g.registry.override_script,
            "restore_entry": g.registry.restore_entry,
        }
        self.originals_wrapped = {
            "apply": self._apply_wrapper(g.game.apply),
            "new_game": self._new_game_wrapper(g.game.new_game),
            "override_script": self._override_wrapper(g.registry.override_script),
            "restore_entry": self._restore_wrapper(g.registry.restore_entry),
        }
        for name in ("override_script", "restore_entry"):
            self._rebind(self.originals[name], self.originals_wrapped[name], lambda m: True)

        def shared(m: str) -> bool:
            return m in ("gcg_sim.engine.game", "gcg_sim.testkit") or _is_test_module(m)

        for name in ("apply", "new_game"):
            self._rebind(self.originals[name], self.originals_wrapped[name], shared)
        self._set(tk, "advance", self._testkit_advance(g.game.advance))
        for method in SCENARIO_METHODS:
            original = getattr(tk.Scenario, method)
            self._set(tk.Scenario, method, self._scenario_method(method, original))
        self._set(
            g.registry.Registry, "program", self._program_wrapper(g.registry.Registry.program)
        )
        engine_by_module = {
            sys.modules[m]: s for m, s in ENGINE_MODULES.items() if m in sys.modules
        }
        probes = {id(getattr(tk, n)): n for n in TESTKIT_PROBES}
        for mname, mod in list(sys.modules.items()):
            if mod is None or not _is_test_module(mname):
                continue
            for attr, val in list(vars(mod).items()):
                if isinstance(val, types.ModuleType) and val in engine_by_module:
                    self._set(mod, attr, _ModuleProxy(self, val, engine_by_module[val]))
                elif isinstance(val, types.FunctionType):
                    if id(val) in probes and getattr(tk, probes[id(val)]) is val:
                        self._set(
                            mod,
                            attr,
                            self._make_wrapper(val, f"testkit.{probes[id(val)]}", "probe"),
                        )
                        continue
                    fname: str = val.__name__
                    short = ENGINE_MODULES.get(val.__module__)
                    home = sys.modules.get(val.__module__)
                    if short is not None and getattr(home, fname, None) is val:
                        self._set(mod, attr, self.wrap_engine(val, short, fname))

    def uninstall(self) -> None:
        while self.restore:
            self.restore.pop()()


class _ReadLog(dict[Any, Any]):
    """An AI memo table that logs the reads of the entries present when an override started."""

    def __init__(
        self, table: dict[Any, Any], tr: TestTrace, name: str, layout: tuple[str, ...]
    ) -> None:
        super().__init__(table)
        self._before = set(table)
        self._tr = tr
        self._name = name
        self._layout = layout
        self._logged: set[Any] = set()

    def get(self, key: Any, default: Any = None) -> Any:
        if key in self._before and key not in self._logged:
            self._logged.add(key)
            self._tr.memo_reads.append(
                {"table": self._name, "key": self._key(key), "value": dict.__getitem__(self, key)}
            )
        return super().get(key, default)

    def _key(self, key: Any) -> list[Any]:
        parts = key if isinstance(key, tuple) else (key,)
        reg = G().registry.get_registry()
        return [
            G().digest.content_id(reg.programs[v]) if role == "program" else v
            for role, v in zip(self._layout, parts, strict=True)
        ]


def _part_name(stem: str, part: int) -> str:
    return f"{stem}.ndjson" if part == 1 else f"{stem}.part{part}.ndjson"


def _rel(p: Path) -> str:
    try:
        return p.resolve().relative_to(ROOT).as_posix()
    except ValueError:
        return str(p)


# ---------------------------------------------------------------------------------------------
# corpus files, manifest and check


def mask_hash_seed(text: str) -> str:
    """The run's PYTHONHASHSEED is the only field allowed to differ between runs (W-150)."""
    return HASH_SEED.sub('"hash_seed":0', text)


def clear_corpus(target: Path) -> None:
    """Remove the trace files and the manifest; other files (the tags) stay."""
    if target.exists():
        for old in sorted(target.rglob("*.ndjson")):
            old.unlink()
        if (target / MANIFEST).exists():
            (target / MANIFEST).unlink()


def write_corpus(
    target: Path, files: list[tuple[str, list[str], int]], meta: dict[str, Any]
) -> None:
    clear_corpus(target)
    target.mkdir(parents=True, exist_ok=True)
    listed: list[dict[str, Any]] = []
    for rel, lines, n_traces in files:
        path = target / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        data = ("\n".join(lines) + "\n").encode("ascii")
        path.write_bytes(data)
        listed.append(
            {
                "path": rel,
                "sha256": hashlib.sha256(data).hexdigest(),
                "bytes": len(data),
                "records": len(lines),
                "traces": n_traces,
            }
        )
    manifest = {
        "version": TRACE_VERSION,
        "set": "tests",
        "tier": 1,
        **meta,
        "files": listed,
        "totals": {
            "files": len(listed),
            "bytes": sum(f["bytes"] for f in listed),
            "records": sum(f["records"] for f in listed),
            "traces": sum(f["traces"] for f in listed),
        },
    }
    (target / MANIFEST).write_text(json.dumps(manifest, indent=1, sort_keys=True) + "\n")


def verify_manifest(target: Path) -> list[str]:
    """The files match their manifest entries (sha256, bytes, records) and none is unlisted."""
    mpath = target / MANIFEST
    if not mpath.is_file():
        return [f"{_rel(mpath)} is missing"]
    problems: list[str] = []
    listed = {f["path"]: f for f in json.loads(mpath.read_text())["files"]}
    present = {p.relative_to(target).as_posix() for p in target.rglob("*.ndjson")}
    problems.extend(f"{x}: not listed in {MANIFEST}" for x in sorted(present - set(listed)))
    for rel, f in sorted(listed.items()):
        path = target / rel
        if not path.is_file():
            problems.append(f"{rel}: listed but missing")
            continue
        data = path.read_bytes()
        if hashlib.sha256(data).hexdigest() != f["sha256"] or len(data) != f["bytes"]:
            problems.append(f"{rel}: sha256 or size differs from {MANIFEST}")
        if data.count(b"\n") != f["records"]:
            problems.append(f"{rel}: record count differs from {MANIFEST}")
    return problems


def compare_corpus(committed: Path, fresh: Path) -> list[str]:
    """Regenerated vs committed: the same tests, files and bytes, except the run's hash seed."""
    problems = verify_manifest(committed)
    if problems and not (committed / MANIFEST).is_file():
        return problems
    old = json.loads((committed / MANIFEST).read_text())
    new = json.loads((fresh / MANIFEST).read_text())
    if old.get("tests") != new.get("tests"):
        problems.append(f"{MANIFEST}: the collected/traced/untraced/isolated tests differ")
    a = {f["path"] for f in old.get("files", [])}
    b = {f["path"] for f in new["files"]}
    problems.extend(
        f"{rel}: in only one of the committed and the regenerated corpus" for rel in sorted(a ^ b)
    )
    for rel in sorted(a & b):
        x = mask_hash_seed((committed / rel).read_text()).splitlines()
        y = mask_hash_seed((fresh / rel).read_text()).splitlines()
        if x != y:
            n = next(
                (i for i, (p, q) in enumerate(zip(x, y, strict=False)) if p != q),
                min(len(x), len(y)),
            )
            problems.append(f"{rel}: first difference at line {n + 1}")
    return problems


# ---------------------------------------------------------------------------------------------
# pytest entry points


def pytest_addoption(parser: pytest.Parser) -> None:
    group = parser.getgroup("gcg-trace", "per-test differential traces (oracle)")
    group.addoption("--trace-out", default=None, help="write the per-test trace corpus to DIR")
    group.addoption(
        "--trace-check", default=None, help="regenerate and compare with the corpus in DIR"
    )
    group.addoption(
        "--trace-snapshots",
        choices=("needed", "all"),
        default="needed",
        help="scenario snapshots only where the calls do not rebuild the state (default; the "
        "digests check every state exactly), or in every scenario record (for a readable diff)",
    )
    group.addoption("--trace-isolated-out", default=None, help=argparse.SUPPRESS)


def pytest_configure(config: pytest.Config) -> None:
    chosen = [
        (mode, path)
        for mode, path in (
            ("out", config.getoption("trace_out")),
            ("check", config.getoption("trace_check")),
            ("isolated", config.getoption("trace_isolated_out")),
        )
        if path
    ]
    if not chosen:
        return
    if len(chosen) > 1:
        raise pytest.UsageError("use one of --trace-out and --trace-check")
    if getattr(config.option, "numprocesses", None):
        raise pytest.UsageError("per-test traces run in one process: do not pass -n (A16)")
    mode, path = chosen[0]
    if mode == "out" and not os.environ.get("PYTHONHASHSEED", "").isdigit():
        raise pytest.UsageError("--trace-out needs PYTHONHASHSEED (1 for the committed corpus)")
    config.pluginmanager.register(TracePlugin(config, mode, Path(path).resolve()), "gcg-trace")


# ---------------------------------------------------------------------------------------------
# T-157: argument encoder round trip


def _ir_row(frag: dict[str, Any], table: str, idx: int) -> Any:
    """Row `idx` of an IR table (`nodes`, `node_content_ids`, `programs`) of the base IR with
    `frag` applied: the base's rows first, the fragment's after them."""
    rows: list[Any] = base_ir()[table]
    return rows[idx] if idx < len(rows) else frag[table][idx - len(rows)]


def decode_node(frag: dict[str, Any], idx: int, memo: dict[int, Any]) -> Any:
    """Rebuild a Python DSL object from `NodeId` `idx` of the base IR with `frag` applied (the
    inverse of export_ir's `Arena.intern`)."""
    if idx in memo:
        return memo[idx]
    g = G()
    raw = _ir_row(frag, "nodes", idx)
    cls = getattr(g.dsl, raw["_"], None) or getattr(g.program, raw["_"])
    hints = typing.get_type_hints(cls)
    obj = cls(
        **{
            f.name: _decode_field(frag, raw[f.name], hints[f.name], memo)
            for f in dataclasses.fields(cls)
        }
    )
    memo[idx] = obj
    return obj


def _hint_types(hint: Any) -> list[Any]:
    if isinstance(hint, typing.TypeAliasType):
        return _hint_types(hint.__value__)
    if typing.get_origin(hint) in (types.UnionType, typing.Union):
        return [t for a in typing.get_args(hint) for t in _hint_types(a)]
    return [hint]


def _tuple_item(hint: Any) -> Any:
    for h in _hint_types(hint):
        if typing.get_origin(h) is tuple:
            args = typing.get_args(h)
            if len(args) == 2 and args[1] is Ellipsis:
                return args[0]
    return Any


def _decode_field(frag: dict[str, Any], v: Any, hint: Any, memo: dict[int, Any]) -> Any:
    if isinstance(v, dict) and "$" in v:
        return decode_node(frag, v["$"], memo)
    if isinstance(v, list):
        inner = _tuple_item(hint)
        return tuple(_decode_field(frag, x, inner, memo) for x in v)
    if isinstance(v, str):
        # The IR writes an enum member as `str(value)` (export_ir `Arena.value`).
        raws: tuple[Any, ...] = (v, int(v)) if v.lstrip("-").isdigit() else (v,)
        for cand in _hint_types(hint):
            if isinstance(cand, type) and issubclass(cand, enum.Enum):
                for raw in raws:
                    try:
                        return cand(raw)
                    except ValueError:
                        continue
    return v


def selftest() -> int:
    """T-157: one synthetic call per argument kind is encoded, written as trace lines, read back
    and decoded. DSL objects round-trip through the fragment (B1's format) with their content
    ids, continuous effects through `continuous`, filter tuples through `filter_sets`, and
    programs through `programs`; a fragment without them is `export_ir.build_fragment`'s."""
    from gcg_sim.effects import dsl as d
    from gcg_sim.engine.state import Action
    from gcg_sim.engine.types import ActionKind, Duration, Zone
    from gcg_sim.testkit import Scenario

    plugin = TracePlugin.__new__(TracePlugin)
    plugin.view_origin = {}
    plugin._seen_states = []
    plugin.generator = {}
    plugin.originals = {}
    tr = TestTrace(plugin, "selftest::encoder", [])
    st = Scenario().start()
    grant = d.KeywordGrant(d.Kw.FIRST_STRIKE)
    reduce1 = d.RuleGrant(d.RuleMod(d.RuleKind.REDUCE_DAMAGE, amount=1))
    filters = (d.NameIs(("Amuro",)), d.IsKind((d.CardKind.UNIT,)))
    reg = G().registry.get_registry()
    prog_id = reg.program((d.Rest(d.This()), d.Draw()), "selftest:program")
    exact: list[tuple[str, Any, Any]] = [
        ("None", None, None),
        ("bool", True, True),
        ("int", -3, -3),
        ("int >= 2^53", 1 << 60, str(1 << 60)),
        ("str", "Zaku Ⅱ", "Zaku Ⅱ"),
        ("float", 0.1, (0.1).hex()),
        ("tuple of ints", ((0, 1), (2, 3)), [[0, 1], [2, 3]]),
        ("IntEnum", Zone.HAND, {"enum": "Zone", "value": int(Zone.HAND)}),
        ("StrEnum", Duration.THIS_TURN, {"enum": "Duration", "value": Duration.THIS_TURN.value}),
        ("Action", Action(ActionKind.ATTACK, 0, -2), ["attack", 0, -2, -1]),
        ("dict", {"t": (1, 2)}, [["t", [1, 2]]]),
        ("set", {3, 1, 2}, [1, 2, 3]),
    ]
    structural: list[tuple[str, Any, str]] = [
        ("GameState", st, ""),
        ("KeywordGrant (continuous)", grant, ""),
        ("RuleGrant (continuous)", reduce1, ""),
        ("filter tuple (filter set)", filters, ""),
        ("program_id", prog_id, "program_id"),
    ]
    failed = 0
    calls = []
    for label, value, want in exact:
        got = tr.enc(value)
        calls.append({"t": "call", "fn": f"selftest.{label}", "args": [got], "result": None})
        back = json.loads(line(got))
        ok = got == want and back == want
        failed += not ok
        print(f"{'ok  ' if ok else 'FAIL'} {label}: {line(got)}")
    for label, value, param in structural:
        calls.append(
            {"t": "call", "fn": f"selftest.{label}", "args": [tr.enc(value, param)], "result": None}
        )
    plain = FragmentBuilder().fragment(registry_snapshot(reg), [])
    built = tr.frags.fragment(registry_snapshot(reg), [])
    additions = {"t": "override", "kind": "additions", "fragment": built}
    text = "\n".join([line(additions), *map(line, calls)])
    lines = [json.loads(x) for x in text.splitlines()]
    frag = lines[0]["fragment"]
    rec = {c["fn"].split(".", 1)[1]: c["args"][0] for c in lines[1:]}
    memo: dict[int, Any] = {}
    base = base_ir()
    checks: list[tuple[str, bool]] = [
        (
            "a fragment without test objects is export_ir.build_fragment's",
            line(plain) == line(X().build_fragment(registry_snapshot(reg), base, [])),
        ),
        ("the fragment has build_fragment's keys", sorted(frag) == sorted(plain)),
    ]
    for label, obj in (("KeywordGrant (continuous)", grant), ("RuleGrant (continuous)", reduce1)):
        idx = rec[label]["$"]
        dec = decode_node(frag, idx, memo)
        cid = sha16(repr(obj))
        conts = [*base["continuous"], *frag["continuous"]]
        checks.append((f"{label} decodes to an equal node", dec == obj and repr(dec) == repr(obj)))
        checks.append(
            (f"{label} content id round-trips", _ir_row(frag, "node_content_ids", idx) == cid)
        )
        checks.append((f"{label} is in continuous", any(c["node"] == idx for c in conts)))
    ids = [x["$"] for x in rec["filter tuple (filter set)"]]
    fs = tuple(decode_node(frag, i, memo) for i in ids)
    checks.append(("filter tuple decodes to equal filters", fs == filters))
    sets = [*base["filter_sets"], *frag["filter_sets"]]
    want_set = {"content_id": sha16(repr(filters)), "filters": ids}
    checks.append(("filter tuple is in filter_sets", want_set in sets))
    real = reg.programs[prog_id]
    ref = rec["program_id"]
    entry = _ir_row(frag, "programs", ref["value"]) if ref.get("enum") == "Program" else {}
    checks.append(
        (
            "program decodes to the registered program",
            tuple(decode_node(frag, i, memo) for i in entry.get("instrs", ())) == real.instrs
            and entry.get("label") == real.label,
        )
    )
    checks.append(
        (
            "program content id equals runner/digest.py content_id",
            entry.get("content_id") == G().digest.content_id(real),
        )
    )
    checks.append(
        (
            "GameState is its digest16",
            rec["GameState"] == {"enum": "GameState", "value": G().digest.state_digest(st)[:16]},
        )
    )
    checks.append(("GameState emitted one state record", [r["t"] for r in tr.records] == ["state"]))
    checks.append(
        (
            "the state record carries the snapshot side tables",
            {"state", "continuous", "filter_sets", "rules_checked"} <= set(tr.records[0]),
        )
    )
    for label, ok in checks:
        failed += not ok
        print(f"{'ok  ' if ok else 'FAIL'} {label}")
    total = len(exact) + len(checks)
    print(f"pytest_trace selftest: {total - failed}/{total} passed")
    return failed


# ---------------------------------------------------------------------------------------------
# command line: two-hash-seed generation and check (W-150)


def _run_pytest(seed: int, option: str, path: Path) -> subprocess.Popen[bytes]:
    env = {**os.environ, "PYTHONHASHSEED": str(seed)}
    cmd = [
        sys.executable,
        "-m",
        "pytest",
        *SUITES,
        "-q",
        "-p",
        "no:cacheprovider",
        "-p",
        "oracle.pytest_trace",
        option,
        str(path),
    ]
    return subprocess.Popen(cmd, cwd=ROOT, env=env)


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(description="Per-test differential traces (PLAN §8.2).")
    group = ap.add_mutually_exclusive_group(required=True)
    group.add_argument("--write", action="store_true", help="regenerate the committed corpus")
    group.add_argument("--check", action="store_true", help="check the committed corpus")
    group.add_argument("--selftest", action="store_true", help="T-157 encoder round trip")
    ap.add_argument("--out", type=Path, default=DEFAULT_OUT)
    args = ap.parse_args(argv)
    if args.selftest:
        return 1 if selftest() else 0
    if args.check:
        codes = [p.wait() for p in [_run_pytest(s, "--trace-check", args.out) for s in (1, 2)]]
        print(f"pytest_trace --check: PYTHONHASHSEED=1 exit {codes[0]}, =2 exit {codes[1]}")
        return 0 if codes == [0, 0] else 1
    with tempfile.TemporaryDirectory(prefix="pytest-trace-write-") as tmp:
        dirs = [Path(tmp) / f"h{seed}" for seed in (1, 2)]
        procs = [_run_pytest(s, "--trace-out", d) for s, d in zip((1, 2), dirs, strict=True)]
        codes = [p.wait() for p in procs]
        if codes != [0, 0]:
            print(f"pytest_trace --write: runs exited {codes}; nothing written", file=sys.stderr)
            return 1
        problems = compare_corpus(dirs[0], dirs[1])
        if problems:
            for p in problems[:50]:
                print(f"pytest_trace --write: hash seeds 1 and 2 differ: {p}", file=sys.stderr)
            return 1
        clear_corpus(args.out)
        shutil.copytree(dirs[0], args.out, dirs_exist_ok=True)
    print(f"pytest_trace --write: wrote {_rel(args.out)} (hash seeds 1 and 2 identical)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
