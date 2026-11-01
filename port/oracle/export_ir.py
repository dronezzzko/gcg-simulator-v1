"""Export the built Python effect registry as the versioned JSON IR of the Rust port.

The IR (docs/port/PLAN.md §3.3, schema version 1) is the registry as ``get_registry()`` builds
it, plus the three engine system programs: a hash-consed node arena (index = ``NodeId``, key =
CPython ``repr``), the programs with Python's ids and first-registrant labels, the abilities
with Python's aids, the card entries with their entry lists, and the tables the engine reads.
It is written with ``json.dumps(ir, ensure_ascii=False, separators=(",", ":")) + "\\n"`` to
``crates/gcg-effects/ir/registry.json`` (with ``registry.json.sha256``), which the Rust crate
embeds.

Node encoding: ``{"_": "<Class>", "<field>": <value>, ...}`` with every field in declaration
order. ``int``/``bool``/``str``/``None`` are JSON scalars, an enum is its value string, a tuple
is an array, and a nested node is ``{"$": <NodeId>}``. Strings are resolved against the sorted
``strings`` table (frame-variable names against ``var_names``) by the Rust loader. Children
always have lower ids than their parents (post-order).

The exporter asserts (PLAN §3.3): ``==`` and ``repr`` classes coincide on the node universe;
no system program equals a card program; no unimplemented card, no binding error, every
``DeployToken.token_key`` resolves and every hook name is registered; no IR string holds a
backslash or a control character and every non-ASCII character is printable; every field value
has its annotated type; program content ids are unique; every delayed trigger's steps are a
build-time program.

    uv run python port/oracle/export_ir.py --write    # writes registry.json and its .sha256
    uv run python port/oracle/export_ir.py --check    # exit 1 (first differing path) if stale

Both modes export twice, under ``PYTHONHASHSEED=1`` and ``=2``, and require identical bytes.
``build_fragment`` (bottom of this file) exports the IR fragment of a script override for the
per-test traces (PLAN §3.4 "Test overrides").
"""

from __future__ import annotations

import argparse
import dataclasses
import enum
import hashlib
import json
import os
import platform
import subprocess
import sys
import tempfile
from collections.abc import Callable, Iterable, Sequence
from pathlib import Path
from typing import Any, NoReturn

import gen_dsl
import gen_engine_enums

from gcg_sim.effects import dsl as d
from gcg_sim.effects import program as pr
from gcg_sim.effects.registry import AbilityEntry, CardEntry, Registry, get_registry
from gcg_sim.effects.scripts import binding_module
from gcg_sim.effects.text import text_hash
from gcg_sim.engine import core, interp, view
from gcg_sim.reports.versions import code_digest

IR_VERSION = 1
FRAGMENT_VERSION = 1
ROOT = Path(__file__).resolve().parents[2]
PKG = ROOT / "src" / "gcg_sim"
OUT = ROOT / "crates" / "gcg-effects" / "ir" / "registry.json"
OUT_SHA = OUT.with_name(OUT.name + ".sha256")
DATA_LOCK = ROOT / "data" / "SOURCES.lock.json"
EXPORTER = "port/oracle/export_ir.py"
# Port-owned files whose content shapes the IR besides the Python package (PLAN §3.3 `inputs`).
PORT_INPUTS = (
    "port/oracle/export_ir.py",
    "port/oracle/gen_dsl.py",
    "port/oracle/gen_engine_enums.py",
)
HASH_SEEDS = (1, 2)
SYSTEM_NAMES = ("deploy", "pair", "hand_limit")
KEYWORD_NAMES = ("repair", "breach", "support")
# Frame-variable names that engine code writes or reads literally (PLAN §3.3 `var_names`):
# __todeck (interp.py:808), __play (:1409), it (:84-86), card/pilot/unit (game.py:931,941,
# interp.py:1416), looked (builtins.py:16), t (builtins.py:68).
ENGINE_VAR_NAMES = ("__todeck", "__play", "it", "card", "pilot", "unit", "looked", "t")
HOOK_KIND = {"StepHook": "step", "CondHook": "cond", "FilterHook": "filter", "ValueHook": "value"}
PARAMS_FIELDS = frozenset(
    {("CustomStep", "params"), ("CustomCond", "params"), ("CustomFilter", "params")}
    | {("CustomValue", "params")}
)


class ExportError(Exception):
    """A PLAN §3.3 exporter assertion failed."""


def fail(msg: str) -> NoReturn:
    raise ExportError(msg)


def sha16(text: str) -> str:
    """``runner/digest.py:26-27`` content_id of an object whose repr is ``text``."""
    return hashlib.sha256(text.encode()).hexdigest()[:16]


def sha256_file(p: Path) -> str:
    return hashlib.sha256(p.read_bytes()).hexdigest()


# ---------------------------------------------------------------------------------------------
# The class tables of dsl.rs (shared with gen_dsl.py so that the IR schema and the Rust types
# come from the same introspection).


def has_default(f: dataclasses.Field[object]) -> bool:
    return f.default is not dataclasses.MISSING or f.default_factory is not dataclasses.MISSING


def default_of(f: dataclasses.Field[object]) -> object:
    return f.default_factory() if f.default_factory is not dataclasses.MISSING else f.default


class Catalog:
    def __init__(self) -> None:
        self.gen = gen_dsl.Gen()
        self.classes: list[type] = [*self.gen.dsl_classes, *self.gen.program_classes]
        self.node_classes: dict[str, type] = {c.__name__: c for c in self.gen.node_classes}
        self.enums: dict[str, type[enum.Enum]] = {e.__name__: e for e in self.gen.enums}
        self.aliases: dict[str, frozenset[str]] = {
            name: frozenset(m for m in members if m != "int")
            for name, _, members in self.gen.aliases
        }
        self.hooks: dict[str, list[str]] = {
            enum_name: [py for py, _, _ in hooks] for enum_name, hooks in self.gen.hooks.items()
        }
        self.specs: dict[type, list[tuple[str, gen_dsl.Ty]]] = {
            cls: [(f.name, gen_dsl.parse_annotation(str(f.type))) for f in dataclasses.fields(cls)]
            for cls in self.classes
        }
        self.defaults: dict[type, dict[str, object]] = {
            cls: {f.name: default_of(f) for f in dataclasses.fields(cls) if has_default(f)}
            for cls in self.classes
        }

    def schema(self) -> dict[str, list[list[object]]]:
        out: dict[str, list[list[object]]] = {}
        for cls in self.classes:
            rows: list[list[object]] = []
            for f in dataclasses.fields(cls):
                default = gen_dsl._default_repr(f)
                rows.append([f.name, str(f.type), default, default is not None])
            out[cls.__name__] = rows
        return out

    def enum_table(self) -> dict[str, list[list[str]]]:
        return {name: [[m.name, str(m.value)] for m in e] for name, e in self.enums.items()}

    def in_alias(self, alias: str, x: object) -> bool:
        return type(x).__name__ in self.aliases[alias] and type(x) is self.node_classes.get(
            type(x).__name__
        )


# ---------------------------------------------------------------------------------------------
# Hash-consed arena


class Arena:
    """Nodes keyed by ``repr`` (PLAN §3.3 hash-consing), encoded in post-order.

    A fragment arena (``base_cids`` set) numbers its nodes after the base IR's and resolves a
    node whose content id is in the base to the base's ``NodeId``."""

    def __init__(self, cat: Catalog, *, base_cids: dict[str, int] | None = None) -> None:
        self.cat = cat
        self.base_cids = base_cids
        self.offset = len(base_cids) if base_cids is not None else 0
        self.ids: dict[str, int] = {}
        self.by_eq: dict[object, int] = {}
        self.objs: list[object] = []
        self.reprs: list[str] = []
        self.nodes: list[dict[str, Any]] = []
        self.strings: set[str] = set()
        self.var_names: set[str] = set()

    def intern(self, x: object) -> int:
        r = repr(x)
        got = self.ids.get(r)
        if got is not None:
            if type(self.obj(got)) is not type(x) or self.obj(got) != x:
                fail(f"repr-equal nodes differ under ==: {r}")
            return got
        if self.base_cids is not None:
            base_id = self.base_cids.get(sha16(r))
            if base_id is not None:
                return base_id
        cls = type(x)
        if self.cat.node_classes.get(cls.__name__) is not cls:
            fail(f"not a DSL or instruction node: {cls!r}")
        enc: dict[str, Any] = {"_": cls.__name__}
        defaults = self.cat.defaults[cls]
        for name, ty in self.cat.specs[cls]:
            v = getattr(x, name)
            enc[name] = self.value(v, ty, (cls.__name__, name))
            if name in defaults and (v == defaults[name]) != (repr(v) == repr(defaults[name])):
                # The Rust golden writer omits a field whose repr equals the default's repr;
                # golden.py:27 compares with ==.
                fail(f"{cls.__name__}.{name}: == and repr disagree on {v!r} vs its default")
        if x in self.by_eq:
            fail(f"==-equal nodes have different reprs: {r}")
        i = self.size()
        self.ids[r] = i
        self.by_eq[x] = i
        self.objs.append(x)
        self.reprs.append(r)
        self.nodes.append(enc)
        return i

    def size(self) -> int:
        return self.offset + len(self.nodes)

    def local_ids(self) -> range:
        return range(self.offset, self.size())

    def obj(self, i: int) -> object:
        return self.objs[i - self.offset]

    def node_repr(self, i: int) -> str:
        return self.reprs[i - self.offset]

    def string(self, s: object, key: tuple[str, str]) -> str:
        if type(s) is not str:
            fail(f"{key[0]}.{key[1]}: expected str, got {s!r}")
        assert isinstance(s, str)
        self.strings.add(s)
        return s

    def value(self, v: object, ty: gen_dsl.Ty, key: tuple[str, str]) -> Any:
        where = f"{key[0]}.{key[1]}"
        if ty.kind == "union":
            if v is None:
                return None
            others = [a for a in ty.args if not (a.kind == "name" and a.name == "None")]
            return self.value(v, others[0], key)
        if ty.kind == "tuple":
            if type(v) is not tuple:
                fail(f"{where}: expected a tuple, got {v!r}")
            assert isinstance(v, tuple)
            if ty.variadic:
                items = [self.value(x, ty.args[0], key) for x in v]
                if key in PARAMS_FIELDS:
                    for k, pv in v:
                        if k == "var":
                            self.var_names.add(str(pv))
                return items
            if len(v) != len(ty.args):
                fail(f"{where}: expected {len(ty.args)} items, got {v!r}")
            return [self.value(x, a, key) for x, a in zip(v, ty.args, strict=True)]
        name = ty.name
        if name == "int":
            if type(v) is not int:
                fail(f"{where}: expected int, got {v!r}")
            return v
        if name == "bool":
            if type(v) is not bool:
                fail(f"{where}: expected bool, got {v!r}")
            return v
        if name == "str":
            if type(v) is not str:
                fail(f"{where}: expected str, got {v!r}")
            assert isinstance(v, str)
            if key in gen_dsl.VAR_FIELDS:
                self.var_names.add(v)
            elif key in gen_dsl.HOOK_FIELDS:
                hook_enum = gen_dsl.HOOK_FIELDS[key]
                if v not in self.cat.hooks[hook_enum]:
                    fail(f"{where}: hook {v!r} is not registered ({HOOK_KIND[hook_enum]})")
            else:
                self.strings.add(v)
            return v
        if name == "object":
            if type(v) is int:
                return v
            if type(v) is str:
                return self.string(v, key)
            if type(v) is d.Sel or self.cat.in_alias("Cond", v):
                return {"$": self.intern(v)}
            fail(f"{where}: a hook parameter must be an int, a str, a Cond or a Sel: {v!r}")
        if name in self.cat.enums:
            if type(v) is not self.cat.enums[name]:
                fail(f"{where}: expected {name}, got {v!r}")
            assert isinstance(v, enum.Enum)
            return str(v.value)
        if name == "Value":
            if type(v) is int:
                return v
            if not self.cat.in_alias("Value", v):
                fail(f"{where}: expected a Value, got {v!r}")
            return {"$": self.intern(v)}
        if name in self.cat.aliases:
            if not self.cat.in_alias(name, v):
                fail(f"{where}: expected a {name}, got {v!r}")
            return {"$": self.intern(v)}
        if name in self.cat.node_classes:
            if type(v) is not self.cat.node_classes[name]:
                fail(f"{where}: expected {name}, got {v!r}")
            return {"$": self.intern(v)}
        fail(f"{where}: unmapped annotation {name!r}")


def check_text(s: str, where: str) -> None:
    """Assertion 4 (PLAN §3.3): no backslash, no control character, printable non-ASCII."""
    for ch in s:
        if ch == "\\" or ord(ch) < 0x20 or ord(ch) == 0x7F:
            fail(f"{where}: {s!r} holds a backslash or a control character")
        if ord(ch) > 0x7F and not ch.isprintable():
            fail(f"{where}: {s!r} holds the non-printable character U+{ord(ch):04X}")


def non_ascii(texts: Iterable[str]) -> list[str]:
    return sorted({ch for s in texts for ch in s if ord(ch) > 0x7F})


# ---------------------------------------------------------------------------------------------
# Continuous-effect identity (PLAN §5.6)


def cont_ref(obj: object, node_id: int) -> dict[str, Any]:
    """The canonical ContRef of a Continuous node: value-shaped effects are value variants."""

    def i32(x: int) -> int:
        if not -(2**31) <= x < 2**31:
            fail(f"{obj!r}: {x} does not fit a ContRef i32")
        return x

    if isinstance(obj, d.StatMod) and type(obj.ap) is int and type(obj.hp) is int:
        return {"StatMod": [i32(obj.ap), i32(obj.hp)]}
    if isinstance(obj, d.KeywordGrant) and type(obj.amount) is int:
        return {"KeywordGrant": [str(obj.keyword.value), i32(obj.amount)]}
    if isinstance(obj, d.AbilityGrant):
        return {"AbilityGrant": [obj.card_number, i32(obj.ability_index)]}
    return {"Node": node_id}


def continuous_table(arena: Arena, ids: Iterable[int]) -> list[dict[str, Any]]:
    """Every Continuous node with its ContRef. Asserts that ContRef equality is ``cont_key``
    interning (``effects/registry.py:124-130``: a dict keyed by the node, so ``hash``/``==``);
    the interning is replayed on a local dict so that the live registry's first-use numbering
    is never touched."""
    out: list[dict[str, Any]] = []
    cont_index: dict[object, int] = {}
    by_ref: dict[str, int] = {}
    by_key: dict[int, str] = {}
    for i in ids:
        obj = arena.obj(i)
        if not arena.cat.in_alias("Continuous", obj):
            continue
        ref = cont_ref(obj, i)
        text = json.dumps(ref, ensure_ascii=False)
        key = cont_index.setdefault(obj, len(cont_index))
        if by_ref.setdefault(text, key) != key or by_key.setdefault(key, text) != text:
            fail(f"ContRef {text} does not intern like cont_key")
        out.append({"node": i, "cont": ref})
    return out


# ---------------------------------------------------------------------------------------------
# Inputs and provenance


def _git(*args: str) -> bytes:
    return subprocess.run(["git", *args], cwd=ROOT, check=True, capture_output=True).stdout


def visible_files(rel_dir: str) -> list[str]:
    """Files under ``rel_dir`` that git sees (tracked or untracked, not ignored), sorted."""
    out = _git("ls-files", "-z", "--cached", "--others", "--exclude-standard", "--", rel_dir)
    names = sorted({n for n in out.decode().split("\0") if n})
    return [n for n in names if (ROOT / n).is_file()]


def tree_id(rel_dir: str, files: Sequence[str]) -> str:
    """The git tree id of ``rel_dir`` built from the working-tree bytes of ``files``; equal to
    ``git rev-parse HEAD:<rel_dir>`` when the directory is clean."""
    root: dict[str, Any] = {}
    prefix = rel_dir.rstrip("/") + "/"
    for name in files:
        parts = name[len(prefix) :].split("/")
        node = root
        for p in parts[:-1]:
            node = node.setdefault(p, {})
        node[parts[-1]] = ROOT / name

    def write(node: dict[str, Any]) -> bytes:
        entries: list[tuple[str, bytes]] = []
        for name, child in node.items():
            if isinstance(child, dict):
                entries.append((name + "/", b"40000 " + name.encode() + b"\0" + write(child)))
            else:
                path = Path(child)
                if path.is_symlink():
                    fail(f"symlink in the inputs: {path}")
                mode = b"100755" if os.access(path, os.X_OK) else b"100644"
                data = path.read_bytes()
                blob = hashlib.sha1(b"blob %d\0" % len(data) + data, usedforsecurity=False)
                entries.append((name, mode + b" " + name.encode() + b"\0" + blob.digest()))
        body = b"".join(e for _, e in sorted(entries))
        return hashlib.sha1(b"tree %d\0" % len(body) + body, usedforsecurity=False).digest()

    return write(root).hex()


def inputs_table() -> tuple[dict[str, str], str]:
    pkg_files = visible_files("src/gcg_sim")
    digest_files: list[str] = []
    for dirpath, dirnames, filenames in os.walk(PKG):
        dirnames[:] = [n for n in dirnames if n != "__pycache__"]
        rel = Path(dirpath).relative_to(ROOT).as_posix()
        digest_files += [f"{rel}/{n}" for n in filenames if not n.endswith(".pyc")]
    if sorted(digest_files) != pkg_files:
        extra = sorted(set(digest_files) ^ set(pkg_files))
        fail(f"code_digest() hashes files git does not track the same way: {extra[:10]}")
    table = {name: sha256_file(ROOT / name) for name in [*pkg_files, *PORT_INPUTS]}
    return dict(sorted(table.items())), tree_id("src/gcg_sim", pkg_files)


def db_fingerprints(reg: Registry) -> dict[str, str]:
    """The four CardDb fingerprints of docs/port/inventory/03-data-deck.md §9."""
    db = reg.db
    lines = []
    for c in db:
        link = None if c.link is None else [list(c.link.name_parts), list(c.link.traits)]
        rec = [
            c.def_id,
            c.card_number,
            c.name,
            c.card_type.value,
            c.color.value if c.color else None,
            c.level,
            c.cost,
            c.ap,
            c.hp,
            list(c.zones),
            list(c.traits),
            link,
            c.effect,
            c.pilot_name,
            c.set_code,
            c.rarity,
            list(c.product_ids),
            c.canonical_product_id,
            c.source_title,
        ]
        lines.append(json.dumps(rec, ensure_ascii=False, separators=(",", ":")) + "\n")

    def h(text: str) -> str:
        return hashlib.sha256(text.encode()).hexdigest()

    return {
        "carddefs": h("".join(lines)),
        "def_ids": h("".join(f"{c.def_id}\t{c.card_number}\n" for c in db)),
        "token_keys": h(
            "".join(f"{k}\t{v.card_number}\n" for k, v in sorted(db._token_by_key.items()))
        ),
        "applied_overrides": h(
            "".join(
                f"{o.conflict_id}\t{o.card_number}\t{o.decision}\t{','.join(o.fields)}\n"
                for o in db.applied_overrides
            )
        ),
    }


# ---------------------------------------------------------------------------------------------
# Registry tables


def card_source(entry: CardEntry) -> str:
    """The effective source, as ``effects/golden.py:45`` reports it."""
    assert entry.script is not None
    src = "binding" if binding_module(entry.card_number) else entry.script.source
    if src not in ("binding", "compiled", "vanilla"):
        fail(f"{entry.card_number}: unknown script source {src!r}")
    return src


def delayed_of(reg: Registry, arena: Arena, entry: CardEntry) -> list[list[int]]:
    """``[n, DelayedTrigger node, program]`` in ``core._walk_delayed`` order over the steps of
    the card's own then unit abilities (``engine/core.py:411-438``, PLAN §3.3)."""
    out: list[list[int]] = []
    n = 0
    for aid in (*entry.own, *entry.unit):
        steps = getattr(reg.abilities[aid].ability, "steps", ())
        for dt in core._walk_delayed(steps):
            pid = reg._program_index.get(dt.steps)
            if pid is None:
                fail(f"{entry.card_number}: delayed trigger steps are not a build-time program")
            out.append([n, arena.intern(dt), pid])
            n += 1
    return out


def printed_keywords_json(kws: dict[d.Kw, int]) -> list[list[object]]:
    for kw, amount in kws.items():
        if not -(2**15) <= amount < 2**15:
            fail(f"printed keyword {kw} amount {amount} does not fit i16")
    return [[str(kw.value), amount] for kw, amount in kws.items()]


def ability_json(arena: Arena, a: AbilityEntry) -> dict[str, Any]:
    return {
        "def_id": a.def_id,
        "card_number": arena.string(a.card_number, ("AbilityEntry", "card_number")),
        "index": a.index,
        "unit_text": a.unit_text,
        "ability": arena.intern(a.ability),
        "program_id": a.program_id,
        "cost_program_id": a.cost_program_id,
        "trigger_event": None if a.trigger_event is None else str(a.trigger_event.value),
    }


def card_json(
    reg: Registry,
    arena: Arena,
    entry: CardEntry,
    printed: dict[d.Kw, int],
    program_map: Callable[[int], int] = lambda p: p,
) -> dict[str, Any]:
    assert entry.script is not None
    cdef = reg.db.by_id(entry.def_id)
    module = binding_module(entry.card_number)
    short = module.rsplit(".", 1)[-1] if module else None
    for t in entry.timings:
        arena.string(t, ("CardEntry", "timings"))
    if short is not None:
        arena.string(short, ("CardEntry", "binding_module"))
    delayed = [[n, node, program_map(p)] for n, node, p in delayed_of(reg, arena, entry)]
    return {
        "def_id": entry.def_id,
        "card_number": arena.string(entry.card_number, ("CardEntry", "card_number")),
        "name": cdef.name,
        "text_hash": text_hash(cdef.effect),
        "source": card_source(entry),
        "binding_module": short,
        "script": arena.intern(entry.script),
        "own": list(entry.own),
        "unit": list(entry.unit),
        "command_aid": entry.command_aid,
        "burst_aid": entry.burst_aid,
        "has_burst": entry.has_burst,
        "timings": sorted(entry.timings),
        "field_entries": [a.aid for a in entry.field_entries],
        "hand_entries": [a.aid for a in entry.hand_entries],
        "trash_entries": [a.aid for a in entry.trash_entries],
        "paired_entries": [a.aid for a in entry.paired_entries],
        "unit_entries": [a.aid for a in entry.unit_entries],
        "gated": entry.gated,
        "events": sorted(str(e.value) for e in reg.events_by_def[entry.def_id]),
        "printed_keywords": printed_keywords_json(printed),
        "delayed": delayed,
        "template": None,
    }


def node_ids_of_class(arena: Arena, ids: Iterable[int], cls: type) -> list[int]:
    return [i for i in ids if type(arena.obj(i)) is cls]


def filter_set_table(arena: Arena, ids: Iterable[int]) -> list[dict[str, Any]]:
    """Every non-empty ``ApplyPlayer.filters`` tuple (the ``filters_key`` call site,
    ``engine/interp.py:1314``), deduplicated by ``==`` in node order."""
    seen: dict[tuple[object, ...], int] = {}
    out: list[dict[str, Any]] = []
    for i in node_ids_of_class(arena, ids, d.ApplyPlayer):
        obj = arena.obj(i)
        assert isinstance(obj, d.ApplyPlayer)
        if obj.filters and obj.filters not in seen:
            seen[obj.filters] = len(out)
            out.append(
                {
                    "filters": [arena.intern(f) for f in obj.filters],
                    "content_id": sha16(repr(obj.filters)),
                }
            )
    return out


def rule_mod_table(arena: Arena, ids: Iterable[int]) -> list[dict[str, Any]]:
    return [
        {"node": i, "repr": arena.node_repr(i)} for i in node_ids_of_class(arena, ids, d.RuleMod)
    ]


def history_kinds() -> list[str]:
    """Every literal ``kind`` that a ``core.record`` call site writes (the HistKind scan of
    ``port/oracle/gen_engine_enums.py``)."""
    return sorted(gen_engine_enums.scan_package().values["HistKind"])


def check_tokens(arena: Arena, reg: Registry) -> None:
    """W-164: every ``DeployToken.token_key`` names a token definition."""
    for obj in arena.objs:
        if isinstance(obj, d.DeployToken) and obj.token_key not in reg.db._token_by_key:
            fail(f"DeployToken token_key {obj.token_key!r} has no token definition")


# ---------------------------------------------------------------------------------------------
# repr vectors (PLAN §3.4 test 5): synthetic instances of every class with non-default fields


VECTOR_STRS = (
    "it's",
    'say "hi"',
    "both ' and \"",
    "Zaku Ⅱ",
    "pay ①?",
    "back\\slash",
    "tab\tnew\nline\rret",
    "nul\x00del\x7f",
    "plain",
    "",
)


class VectorFactory:
    """Builds one instance per class whose every field differs from its default."""

    def __init__(self, cat: Catalog) -> None:
        self.cat = cat
        self.salt = 0
        sel = d.Sel(d.Side.ANY, d.Loc.HAND, (d.HasTrait(("Zeon", "it's")),), 2)
        self.simple: dict[str, list[object]] = {
            "Value": [d.Count(d.Sel(d.Side.ENEMY, d.Loc.TRASH)), d.HandSize(d.P.OPP)],
            "Filter": [d.IsKind((d.CardKind.PILOT,)), d.IsRested(False)],
            "Ref": [d.ThisCard(), d.Var("Zaku Ⅱ")],
            "Sel": [sel, d.Sel(d.Side.FRIENDLY, d.Loc.BATTLE)],
            "Cond": [d.IsTurn(d.P.OPP), d.DidLast()],
            "Step": [d.Draw(2), d.Reveal(d.This())],
            "Continuous": [d.StatMod(ap=1, hp=-1), d.TraitGrant(("UN",))],
            "Trigger": [d.Trigger(d.Ev.ATTACKS, self_only=False), d.Trigger(d.Ev.EXILED)],
            "RuleMod": [d.RuleMod(d.RuleKind.CANT_BLOCK), d.RuleMod(d.RuleKind.CUSTOM, 2)],
            "Cost": [d.RestSelf(), d.PayResources(2)],
            "Ability": [d.Keyword(d.Kw.BLOCKER), d.NameAlias(("Char Aznable",))],
            "Instr": [pr.Jump(3), d.Draw(1, d.P.OPP)],
        }
        self.parameterless = {
            "StepHook": "return_looked_bottom",
            "CondHook": "attack_target_damaged",
            "FilterHook": "is_attack_target",
            "ValueHook": "wp_gd05_a_lowest_enemy_unit_level",
        }

    def bump(self) -> int:
        self.salt += 1
        return self.salt

    def pick(self, options: Sequence[object], default: object) -> object:
        start = self.bump()
        for k in range(len(options)):
            v = options[(start + k) % len(options)]
            if default is dataclasses.MISSING or v != default:
                return v
        fail(f"no non-default option among {options!r}")

    def make(self, ty: gen_dsl.Ty, key: tuple[str, str], default: object) -> object:
        if ty.kind == "union":
            others = [a for a in ty.args if not (a.kind == "name" and a.name == "None")]
            return self.make(others[0], key, dataclasses.MISSING)
        if ty.kind == "tuple":
            if key in PARAMS_FIELDS:
                return (
                    ("var", "it's"),
                    ("count", -2),
                    ("cond", d.IsTurn(d.P.OPP)),
                    ("sel", self.simple["Sel"][0]),
                    ("var", "t1"),
                )
            if ty.variadic:
                n = 1 + self.bump() % 2
                return tuple(self.make(ty.args[0], key, dataclasses.MISSING) for _ in range(n))
            return tuple(self.make(a, key, dataclasses.MISSING) for a in ty.args)
        name = ty.name
        if name == "int":
            return self.pick((-3, 7, 0, 205006, -1), default)
        if name == "bool":
            return (not default) if isinstance(default, bool) else True
        if name == "str":
            if key in gen_dsl.HOOK_FIELDS:
                return self.parameterless[gen_dsl.HOOK_FIELDS[key]]
            return self.pick(VECTOR_STRS, default)
        if name in self.cat.enums:
            return self.pick(list(self.cat.enums[name]), default)
        if name == "Value":
            return self.pick((4, *self.simple["Value"], -2), default)
        if name in self.simple:
            return self.pick(self.simple[name], default)
        if name in self.cat.aliases:
            return self.pick(self.simple[name], default)
        fail(f"no vector value for {key}: {name}")

    def instance(self, cls: type) -> object:
        kwargs: dict[str, object] = {}
        for f in dataclasses.fields(cls):
            default: object = f.default
            if f.default_factory is not dataclasses.MISSING:
                default = f.default_factory()
            ty = gen_dsl.parse_annotation(str(f.type))
            v = self.make(ty, (cls.__name__, f.name), default)
            if default is not dataclasses.MISSING and v == default:
                fail(f"vector for {cls.__name__}.{f.name} equals its default")
            kwargs[f.name] = v
        return cls(**kwargs)


def repr_vectors(cat: Catalog) -> dict[str, Any]:
    arena = Arena(cat)
    fac = VectorFactory(cat)
    cases = []
    for cls in cat.gen.node_classes:
        for _ in range(2):  # two instances per class: one- and two-element tuples both occur
            inst = fac.instance(cls)
            cases.append({"node": arena.intern(inst), "repr": repr(inst)})
    defaults = [d.This(), d.Draw(), d.Sel(d.Side.ANY, d.Loc.DECK), d.Keyword(d.Kw.REPAIR)]
    cases += [{"node": arena.intern(x), "repr": repr(x)} for x in defaults]
    programs = []
    for label, instrs in (
        ("it's Ⅱ", (d.Draw(), pr.Jump(0))),
        ('say "x"', ()),
        ("system:deploy", (d.DeployCard(d.Var("card")),)),
    ):
        p = pr.Program(instrs, label)
        arena.strings.add(label)
        programs.append(
            {"label": label, "instrs": [arena.intern(i) for i in instrs], "repr": repr(p)}
        )
    conts: list[dict[str, Any]] = []
    for value in (
        d.StatMod(ap=-3, hp=0),
        d.StatMod(ap=0, hp=2),
        d.KeywordGrant(d.Kw.BREACH, 2),
        d.KeywordGrant(d.Kw.FIRST_STRIKE, 0),
        d.AbilityGrant("GD01-001", 1),
        d.AbilityGrant("Char's Zaku Ⅱ", 0),
    ):
        ref = cont_ref(value, -1)
        if isinstance(value, d.AbilityGrant):
            arena.strings.add(value.card_number)
        conts.append({"cont": ref, "repr": repr(value)})
    filter_tuples = []
    for tup in (
        (),
        (d.IsKind((d.CardKind.UNIT,)),),
        (d.HasTrait(("Zaku Ⅱ",)), d.NameIs(("it's", 'a"b'))),
    ):
        filter_tuples.append({"filters": [arena.intern(f) for f in tup], "repr": repr(tup)})
    strings = sorted(arena.strings)
    var_names = sorted(arena.var_names)
    return {
        "printable_non_ascii": non_ascii([*strings, *var_names]),
        "strings": strings,
        "var_names": var_names,
        "nodes": arena.nodes,
        "cases": cases,
        "programs": programs,
        "conts": conts,
        "filter_tuples": filter_tuples,
    }


# ---------------------------------------------------------------------------------------------
# The IR


def build_ir() -> dict[str, Any]:
    cat = Catalog()
    reg = get_registry()
    db = reg.db
    for e in reg.cards:
        if e.script is None:
            fail(f"{e.card_number}: unimplemented or binding error: {e.error}")  # W-69
    build_programs = len(reg.programs)
    for name in SYSTEM_NAMES:
        if interp._SYSTEM[name] in reg._program_index:
            fail(f"system program {name} equals a card program")
    system = {name: interp.system_program(name) for name in SYSTEM_NAMES}
    if list(system.values()) != list(range(build_programs, build_programs + 3)):
        fail(f"system programs registered out of order: {system}")

    arena = Arena(cat)
    for e in reg.cards:
        arena.intern(e.script)
    program_instrs = [[arena.intern(i) for i in p.instrs] for p in reg.programs]
    check_tokens(arena, reg)

    programs = []
    seen_cids: set[str] = set()
    for p, instrs in zip(reg.programs, program_instrs, strict=True):
        cid = sha16(repr(p))
        if cid in seen_cids:
            fail(f"program content id {cid} is not unique ({p.label})")
        seen_cids.add(cid)
        arena.strings.add(p.label)
        programs.append({"label": p.label, "content_id": cid, "instrs": instrs})
    for i, a in enumerate(reg.abilities):
        if a.aid != i:
            fail(f"ability {i} has aid {a.aid}")
    abilities = [ability_json(arena, a) for a in reg.abilities]
    cards = [card_json(reg, arena, e, reg.printed_keywords(e.def_id)) for e in reg.cards]
    for i, c in enumerate(cards):
        if c["def_id"] != i:
            fail(f"card {i} has def_id {c['def_id']}")
    token_keys = [
        [arena.string(k, ("CardDB", "token_key")), v.def_id]
        for k, v in sorted(db._token_by_key.items())
    ]
    aliases = [[def_id, list(names)] for def_id, names in sorted(reg.aliases.items())]
    arena.var_names.update(ENGINE_VAR_NAMES)

    all_ids = range(arena.size())
    node_cids = [sha16(r) for r in arena.reprs]
    if len(set(node_cids)) != len(node_cids):
        fail("node content ids are not unique")
    strings = sorted(arena.strings)
    var_names = sorted(arena.var_names)
    for s in strings:
        check_text(s, "strings")
    for s in var_names:
        check_text(s, "var_names")
    printable = non_ascii([*strings, *var_names])

    counts = {
        "defs": len(reg.cards),
        "real_cards": len(db.real_cards()),
        "abilities": len(reg.abilities),
        "build_programs": build_programs,
        "system_programs": len(system),
        "build_instrs": sum(len(p.instrs) for p in reg.programs[:build_programs]),
        "instrs": sum(len(p.instrs) for p in reg.programs),
        "nodes": arena.size(),
        "strings": len(strings),
        "var_names": len(var_names),
        "binding": sum(1 for c in cards if c["source"] == "binding"),
        "compiled": sum(1 for c in cards if c["source"] == "compiled"),
        "vanilla": sum(1 for c in cards if c["source"] == "vanilla"),
        "nonempty_events": sum(1 for c in cards if c["events"]),
    }
    inputs, git_tree = inputs_table()
    return {
        "ir_version": IR_VERSION,
        "generator": {
            "exporter": EXPORTER,
            "python": platform.python_version(),
            "gcg_sim_code_digest": code_digest(),
            "git": git_tree,
            "hash_seeds_checked": list(HASH_SEEDS),
        },
        "inputs": inputs,
        "data_lock_sha256": sha256_file(DATA_LOCK),
        "db_fingerprints": db_fingerprints(reg),
        "schema": cat.schema(),
        "enums": cat.enum_table(),
        "printable_non_ascii": printable,
        "strings": strings,
        "var_names": var_names,
        "nodes": arena.nodes,
        "node_content_ids": node_cids,
        "programs": programs,
        "system_programs": system,
        "keyword_programs": dict(
            zip(
                KEYWORD_NAMES,
                (reg.repair_program, reg.breach_program, reg.support_program),
                strict=True,
            )
        ),
        "abilities": abilities,
        "cards": cards,
        "continuous": continuous_table(arena, all_ids),
        "filter_sets": filter_set_table(arena, all_ids),
        "rule_mods": rule_mod_table(arena, all_ids),
        "aliases": aliases,
        "always_events": sorted(str(e.value) for e in reg.always_events),
        "hp_reduction_possible": reg.hp_reduction_possible,
        "token_keys": token_keys,
        "unit_variants": sorted([k, v] for k, v in db._unit_variant.items()),
        "hooks": {
            "step": list(interp.CUSTOM_STEPS),
            "cond": list(view.CUSTOM_CONDS),
            "filter": list(view.CUSTOM_FILTERS),
            "value": list(view.CUSTOM_VALUES),
        },
        "history_kinds": history_kinds(),
        "repr_vectors": repr_vectors(cat),
        "counts": counts,
    }


def render(obj: object) -> str:
    return json.dumps(obj, ensure_ascii=False, separators=(",", ":")) + "\n"


# ---------------------------------------------------------------------------------------------
# IR fragments of script overrides (PLAN §3.4 "Test overrides")


def registry_fingerprint(ir: dict[str, Any]) -> str:
    """The content fingerprint of a registry that a fragment names as its base (Rust
    ``View::fingerprint``): SHA-256 over every node content id, every program content id, and
    the ability and card counts."""
    text = "".join(f"{c}\n" for c in ir["node_content_ids"])
    text += "programs\n" + "".join(f"{p['content_id']}\n" for p in ir["programs"])
    text += f"abilities {len(ir['abilities'])}\ncards {len(ir['cards'])}\n"
    return hashlib.sha256(text.encode()).hexdigest()


def effective_printed_keywords(reg: Registry, def_id: int) -> dict[d.Kw, int]:
    """What ``reg.printed_keywords(def_id)`` returns now (the cache entry, which
    ``override_script`` never invalidates, or a fresh value), without filling the cache."""
    had = def_id in reg._printed_kw
    out = reg.printed_keywords(def_id)
    if not had:
        del reg._printed_kw[def_id]
    return out


def build_fragment(
    reg: Registry, base_ir: dict[str, Any], def_ids: Sequence[int]
) -> dict[str, Any]:
    """The IR fragment that turns the committed base IR into ``reg`` while
    ``registry.override_script`` has replaced the entries of ``def_ids`` (PLAN §3.4).

    It holds the replaced card entries with Python's aids; every node, program and ability they
    reference that the base lacks (programs are numbered after the base's, by content id,
    because Python numbers programs created after the build in first-use order); and, verbatim,
    the registry-global values that the override changed: the ``aliases`` of every
    definition whose aliases differ from the base (``_build_entry`` appends a NameAlias's
    names to the old ones, ``registry.py:240-241``), and what ``_index_events`` recomputed:
    ``always_events``,
    ``hp_reduction_possible``, the ``events`` of every definition that differ from the base, and
    the ``printed_keywords`` of every definition whose current value (Python's never-invalidated
    cache entry, or a fresh value, ``registry.py:89-113``) differs from the base."""
    cat = Catalog()
    base_cids = {cid: i for i, cid in enumerate(base_ir["node_content_ids"])}
    if len(base_cids) != len(base_ir["node_content_ids"]):
        fail("base IR node content ids are not unique")
    frag = Arena(cat, base_cids=base_cids)
    base_programs: list[dict[str, Any]] = base_ir["programs"]
    prog_by_cid = {p["content_id"]: i for i, p in enumerate(base_programs)}
    new_programs: list[dict[str, Any]] = []
    py_to_ir: dict[int, int] = {}

    def program_id(py_id: int) -> int:
        if py_id < 0:
            return py_id
        if py_id not in py_to_ir:
            p = reg.programs[py_id]
            cid = sha16(repr(p))
            got = prog_by_cid.get(cid)
            if got is None:
                got = len(base_programs) + len(new_programs)
                prog_by_cid[cid] = got
                frag.strings.add(p.label)
                new_programs.append(
                    {
                        "label": p.label,
                        "content_id": cid,
                        "instrs": [frag.intern(i) for i in p.instrs],
                    }
                )
            py_to_ir[py_id] = got
        return py_to_ir[py_id]

    n_base = len(base_ir["abilities"])
    new_abilities: list[dict[str, Any]] = []
    cards = []
    for def_id in def_ids:
        entry = reg.cards[def_id]
        if entry.script is None:
            fail(f"{entry.card_number}: override without a script")
        for aid in (*entry.own, *entry.unit):
            if aid < n_base:
                fail(f"{entry.card_number}: an override entry reuses base ability {aid}")
            if aid != n_base + len(new_abilities):
                fail(f"{entry.card_number}: override ability {aid} is not the next aid")
            rec = ability_json(frag, reg.abilities[aid])
            rec["program_id"] = program_id(rec["program_id"])
            rec["cost_program_id"] = program_id(rec["cost_program_id"])
            new_abilities.append(rec)
        printed = effective_printed_keywords(reg, def_id)
        cards.append(card_json(reg, frag, entry, printed, program_id))
    base_cards: list[dict[str, Any]] = base_ir["cards"]
    replaced = set(def_ids)
    events = []
    printed_kw = []
    for c in reg.cards:
        evs = sorted(str(e.value) for e in reg.events_by_def.get(c.def_id, ()))
        if evs != base_cards[c.def_id]["events"]:
            events.append([c.def_id, evs])
        if c.def_id not in replaced:
            kws = printed_keywords_json(effective_printed_keywords(reg, c.def_id))
            if kws != base_cards[c.def_id]["printed_keywords"]:
                printed_kw.append([c.def_id, kws])
    base_aliases: dict[int, list[str]] = dict(base_ir["aliases"])
    aliases = [
        [def_id, [frag.string(n, ("Registry", "aliases")) for n in names]]
        for def_id, names in sorted(reg.aliases.items())
        if list(names) != base_aliases.get(def_id)
    ]
    ids = frag.local_ids()
    base_filter_cids = {f["content_id"] for f in base_ir["filter_sets"]}
    strings = sorted(frag.strings - set(base_ir["strings"]))
    var_names = sorted(frag.var_names - set(base_ir["var_names"]))
    for s in [*strings, *var_names]:
        check_text(s, "fragment")
    return {
        "fragment_version": FRAGMENT_VERSION,
        "base_fingerprint": registry_fingerprint(base_ir),
        "printable_non_ascii": non_ascii([*strings, *var_names]),
        "strings": strings,
        "var_names": var_names,
        "nodes": frag.nodes,
        "node_content_ids": [sha16(r) for r in frag.reprs],
        "programs": new_programs,
        "abilities": new_abilities,
        "cards": cards,
        "continuous": continuous_table(frag, ids),
        "filter_sets": [
            f for f in filter_set_table(frag, ids) if f["content_id"] not in base_filter_cids
        ],
        "rule_mods": rule_mod_table(frag, ids),
        "aliases": aliases,
        "always_events": sorted(str(e.value) for e in reg.always_events),
        "events": events,
        "hp_reduction_possible": reg.hp_reduction_possible,
        "printed_keywords": printed_kw,
    }


# Script overrides of the Python tests that the fragment fixtures replay (PLAN §3.4; the
# per-test traces export the same fragments, Inv 10 §9.2).
FRAGMENT_CASES = (
    # tests/ai/test_policy_knobs.py:167
    ("ST04-005", "【Attack】Choose 1 enemy Unit. It can't block during this battle."),
    # tests/ai/test_root_moves.py:188
    ("ST05-003", "<Repair 1>"),
    # tests/rules/test_damage.py:791 (DINN; makes hp_reduction_possible true, W-51)
    ("GD01-064", "【Deploy】Choose 1 enemy Unit. It gets HP-1 during this turn."),
    # Synthetic (no Python test overrides a name alias): GOOhN, the HOST of
    # tests/rules/test_turn.py:60, gains a NameAlias, so the fragment changes `aliases`.
    ("GD01-062", "This card's name is also treated as [Char Aznable]."),
)
FRAGMENT_DIR = OUT.parent / "fragments"


def fragment_path(number: str) -> Path:
    return FRAGMENT_DIR / f"{number}.json"


def fragment_fixture(index: int, base_ir: dict[str, Any]) -> dict[str, Any]:
    """One override of ``FRAGMENT_CASES`` applied with ``testkit.card_text`` in this (fresh)
    process: the fragment and the golden record Python gives the overridden card."""
    from gcg_sim import testkit
    from gcg_sim.effects.golden import card_golden

    number, text = FRAGMENT_CASES[index]
    reg = get_registry()
    cdef = reg.db[number]
    with testkit.card_text(number, text):
        frag = build_fragment(reg, base_ir, [cdef.def_id])
        golden = card_golden(cdef, reg.cards[cdef.def_id], binding_module(number))
    return {"card_number": number, "text": text, "fragment": frag, "golden": golden}


def first_difference(a: Any, b: Any, path: str = "$") -> str | None:
    if type(a) is not type(b):
        return f"{path}: {type(a).__name__} vs {type(b).__name__}"
    if isinstance(a, dict):
        ka, kb = list(a), list(b)
        if ka != kb:
            for i, (x, y) in enumerate(zip(ka, kb, strict=False)):
                if x != y:
                    return f"{path}: key #{i} {x!r} vs {y!r}"
            return f"{path}: {len(ka)} vs {len(kb)} keys"
        for k in ka:
            got = first_difference(a[k], b[k], f"{path}.{k}")
            if got:
                return got
        return None
    if isinstance(a, list):
        for i, (x, y) in enumerate(zip(a, b, strict=False)):
            got = first_difference(x, y, f"{path}[{i}]")
            if got:
                return got
        if len(a) != len(b):
            return f"{path}: length {len(a)} vs {len(b)}"
        return None
    return None if a == b else f"{path}: {a!r} vs {b!r}"


def export_under(seed: int, *extra: str) -> str:
    env = {**os.environ, "PYTHONHASHSEED": str(seed)}
    res = subprocess.run(
        [sys.executable, str(Path(__file__).resolve()), "--emit", *extra],
        cwd=ROOT,
        env=env,
        capture_output=True,
        text=True,
        encoding="utf-8",
        check=False,
    )
    if res.returncode != 0:
        raise SystemExit(f"export under PYTHONHASHSEED={seed} failed:\n{res.stderr}")
    return res.stdout


def sha_line(text: str) -> str:
    return f"{hashlib.sha256(text.encode()).hexdigest()}  {OUT.name}\n"


def export_all() -> tuple[str, list[str]] | None:
    """The IR and the fragment fixtures, each exported under both hash seeds (each fixture in
    a fresh process, PLAN A16, against the IR of the same run in a temporary file)."""
    texts = [export_under(seed) for seed in HASH_SEEDS]
    if texts[0] != texts[1]:
        where = first_difference(json.loads(texts[0]), json.loads(texts[1]))
        print(f"the export depends on PYTHONHASHSEED: {where}", file=sys.stderr)
        return None
    with tempfile.TemporaryDirectory(prefix="export_ir-") as tmp:
        base = Path(tmp) / OUT.name
        base.write_text(texts[0], encoding="utf-8")
        fixtures = []
        for i in range(len(FRAGMENT_CASES)):
            runs = [export_under(seed, "--fragment", str(i), str(base)) for seed in HASH_SEEDS]
            if runs[0] != runs[1]:
                where = first_difference(json.loads(runs[0]), json.loads(runs[1]))
                print(f"fragment {i} depends on PYTHONHASHSEED: {where}", file=sys.stderr)
                return None
            fixtures.append(runs[0])
    return texts[0], fixtures


def main() -> int:
    ap = argparse.ArgumentParser(description=(__doc__ or "").splitlines()[0])
    mode = ap.add_mutually_exclusive_group(required=True)
    mode.add_argument("--write", action="store_true", help="write the IR and its fixtures")
    mode.add_argument("--check", action="store_true", help="exit 1 if the committed IR is stale")
    mode.add_argument("--emit", action="store_true", help="print one export (this hash seed)")
    ap.add_argument(
        "--fragment",
        nargs=2,
        metavar=("INDEX", "BASE_IR"),
        help="with --emit: print fragment fixture INDEX against the IR file BASE_IR",
    )
    args = ap.parse_args()
    if args.emit:
        try:
            if args.fragment:
                base = json.loads(Path(args.fragment[1]).read_text(encoding="utf-8"))
                sys.stdout.write(render(fragment_fixture(int(args.fragment[0]), base)))
            else:
                sys.stdout.write(render(build_ir()))
        except ExportError as exc:
            print(f"export_ir: {exc}", file=sys.stderr)
            return 1
        return 0
    got = export_all()
    if got is None:
        return 1
    text, fixtures = got
    rel = OUT.relative_to(ROOT)
    paths = [fragment_path(number) for number, _ in FRAGMENT_CASES]
    if args.write:
        OUT.parent.mkdir(parents=True, exist_ok=True)
        OUT.write_text(text, encoding="utf-8")
        OUT_SHA.write_text(sha_line(text), encoding="utf-8")
        FRAGMENT_DIR.mkdir(exist_ok=True)
        for path, fixture in zip(paths, fixtures, strict=True):
            path.write_text(fixture, encoding="utf-8")
        print(
            f"wrote {rel} ({len(text.encode())} bytes), {OUT_SHA.name} and {len(paths)} fragments"
        )
        return 0
    current = OUT.read_text(encoding="utf-8") if OUT.exists() else ""
    if current != text:
        where = first_difference(json.loads(current), json.loads(text)) if current else "missing"
        print(f"{rel} is stale ({where}); run {EXPORTER} --write", file=sys.stderr)
        return 1
    sha = OUT_SHA.read_text(encoding="utf-8") if OUT_SHA.exists() else ""
    if sha != sha_line(text):
        print(f"{OUT_SHA.relative_to(ROOT)} is stale; run {EXPORTER} --write", file=sys.stderr)
        return 1
    stray = sorted(set(FRAGMENT_DIR.glob("*.json")) - set(paths))
    if stray:
        print(f"unexpected fragment fixtures: {stray}", file=sys.stderr)
        return 1
    for path, fixture in zip(paths, fixtures, strict=True):
        if not path.exists() or path.read_text(encoding="utf-8") != fixture:
            print(f"{path.relative_to(ROOT)} is stale; run {EXPORTER} --write", file=sys.stderr)
            return 1
    print(f"{rel} and {len(paths)} fragments are current (PYTHONHASHSEED 1 and 2)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
