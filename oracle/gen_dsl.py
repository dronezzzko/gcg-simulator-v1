"""Generate the Rust effect-DSL types of crates/gcg-effects/src/dsl.rs from the Python reference.

Introspects the frozen dataclasses and StrEnums of ``src/gcg_sim/effects/dsl.py`` (with
``Duration`` from ``engine/types.py``), the instruction classes of ``effects/program.py`` and the
custom-hook tables that the binding modules fill, and emits:

- one ``#[repr(u8)]`` enum per StrEnum: declaration order as discriminants, the value strings,
  the member names (for ``repr``) and the members sorted by value;
- one enum per hook kind (step, cond, filter, value) whose strings are the Python hook names;
- one struct per DSL and instruction dataclass with the fields in declaration order (``repr``
  and the IR depend on it), typed by a fixed mapping of the annotation strings;
- the ``Node`` and ``NodeKind`` enums, the kind sets of the ``type`` aliases, and the schema
  table that the IR loader compares with the IR's ``schema`` and ``enums`` (PLAN §3.4, §5.1);
- the per-class glue of ``crate::codec`` (``Node::decode`` from the IR node encoding of
  ``oracle/export_ir.py``, CPython ``repr`` and the golden JSON of ``effects/golden.py``),
  which applies the hand-written ``Field`` impls to every field in declaration order.

The output goes through ``rustfmt --edition 2024`` so that ``cargo fmt`` leaves it unchanged.

    uv run python oracle/gen_dsl.py --write
    uv run python oracle/gen_dsl.py --check    # exit 1 if dsl.rs is stale
"""

from __future__ import annotations

import argparse
import dataclasses
import enum
import hashlib
import inspect
import os
import re
import shutil
import subprocess
import sys
import typing
from collections.abc import Callable, Iterable, Mapping
from pathlib import Path

import gcg_sim.effects.scripts  # noqa: F401  (registers the builtin hooks)
from gcg_sim.effects import dsl, program
from gcg_sim.effects.bindings import load_bindings
from gcg_sim.engine import interp, view
from gcg_sim.engine import types as engine_types

ROOT = Path(__file__).resolve().parents[1]
PKG = ROOT / "src" / "gcg_sim"
OUT = ROOT / "crates" / "gcg-effects" / "src" / "dsl.rs"
SOURCES = (
    PKG / "effects" / "dsl.py",
    PKG / "effects" / "program.py",
    PKG / "engine" / "types.py",
)

# Counts the plan relies on.
EXPECTED_DSL_CLASSES = 137
EXPECTED_PROGRAM_CLASSES = 10  # 9 instructions + Program
EXPECTED_ENUMS = 14  # 13 DSL enums + Duration

# Fields that hold frame-variable names: they index the IR's `var_names` table (VarId).
VAR_FIELDS = frozenset(
    {
        ("VarSize", "var"),
        ("Var", "name"),
        ("Choose", "var"),
        ("Choose", "distinct_from"),
        ("Discard", "var"),
        ("DeployToken", "var"),
        ("Mill", "var"),
        ("LookTop", "var"),
        ("ForEach", "var"),
        ("BindVar", "var"),
        ("DelayedTrigger", "bind_vars"),
        ("LoopInit", "key"),
        ("LoopNext", "key"),
        ("LoopNext", "var"),
    }
)

# Custom-hook name fields are typed by the hook enum of their kind.
HOOK_FIELDS = {
    ("CustomStep", "name"): "StepHook",
    ("CustomCond", "name"): "CondHook",
    ("CustomFilter", "name"): "FilterHook",
    ("CustomValue", "name"): "ValueHook",
}

# Rust variant names of the 34 hooks. The table must cover the registered hooks exactly.
HOOK_NAMES = {
    "StepHook": {
        "return_looked_bottom": "ReturnLookedBottom",
        "deploy_ex_base": "DeployExBase",
        "support_used": "SupportUsed",
        "wp_eb01_a_look_own_top": "EB01aLookOwnTop",
        "wp_eb01_b_look_top_own": "EB01bLookTopOwn",
        "wp_gd02_b_pay_deploy_cost": "GD02bPayDeployCost",
        "wp_gd03_a_pay_cost_of": "GD03aPayCostOf",
        "wp_gd03_a_ap_minus_count": "GD03aApMinusCount",
        "wp_gd04_a_copy_keywords": "GD04aCopyKeywords",
        "wp_gd04_a_ap_down_by_named_trash_units": "GD04aApDownByNamedTrashUnits",
        "wp_gd05_a_use_once": "GD05aUseOnce",
        "wp_gd05_b_destroy_first_shield_area_cards": "GD05bDestroyFirstShieldAreaCards",
        "wp_gd05_b_resolving_command_to_trash": "GD05bResolvingCommandToTrash",
    },
    "CondHook": {
        "attack_target_damaged": "AttackTargetDamaged",
        "wp_gd01_b_destroyed_with_damage": "GD01bDestroyedWithDamage",
        "wp_gd04_b_played_with_ex": "GD04bPlayedWithEx",
        "wp_gd05_a_once_free": "GD05aOnceFree",
        "wp_gd05_a_destroyed_by_effect": "GD05aDestroyedByEffect",
        "wp_gd05_b_played_with_ex": "GD05bPlayedWithEx",
        "wp_st12_14_destroyed_by_effect_damage": "ST12DestroyedByEffectDamage",
    },
    "FilterHook": {
        "is_attack_target": "IsAttackTarget",
        "wp_gd03_a_has_destroyed_effect": "GD03aHasDestroyedEffect",
        "wp_gd03_a_lowest_hp": "GD03aLowestHp",
        "wp_gd03_b_cond_holds": "GD03bCondHolds",
        "wp_gd03_b_battling_enemy_unit": "GD03bBattlingEnemyUnit",
        "wp_gd04_a_pilot_capable": "GD04aPilotCapable",
        "wp_gd04_a_has_printed_keyword": "GD04aHasPrintedKeyword",
        "wp_gd04_a_has_partner_in_trash": "GD04aHasPartnerInTrash",
        "wp_gd05_b_can_pair": "GD05bCanPair",
        "wp_gd06_a_name_unlike": "GD06aNameUnlike",
        "wp_gd06_b_link_names": "GD06bLinkNames",
    },
    "ValueHook": {
        "wp_gd03_a_ap_per": "GD03aApPer",
        "wp_gd03_b_unique_names": "GD03bUniqueNames",
        "wp_gd05_a_lowest_enemy_unit_level": "GD05aLowestEnemyUnitLevel",
        "wp_st05_08_highest_enemy_unit_lv": "ST05HighestEnemyUnitLv",
        "wp_st12_14_lowest_rested_enemy_lv": "ST12LowestRestedEnemyLv",
    },
}

# Annotation names that map to typed node handles (crate::contracts) or to Rust scalars.
NAMED_TYPES = {
    "int": "i64",
    "bool": "bool",
    "str": "StrId",
    "object": "ParamValue",
    "Value": "ValueRef",
    "Filter": "FilterId",
    "Ref": "RefId",
    "Sel": "SelId",
    "Cond": "CondId",
    "Step": "StepId",
    "Continuous": "ContinuousId",
    "Trigger": "TriggerId",
    "RuleMod": "RuleModId",
    "Cost": "CostId",
    "Ability": "AbilityNodeId",
    "Instr": "NodeId",
}
COPY_TYPES = frozenset({*NAMED_TYPES.values(), "VarId"})
CONTRACT_TYPES = frozenset(NAMED_TYPES.values()) - {"i64", "bool"} | {"VarId"}

RUST_KEYWORDS = frozenset(
    [
        "as",
        "async",
        "await",
        "break",
        "const",
        "continue",
        "crate",
        "dyn",
        "else",
        "enum",
        "extern",
        "false",
        "fn",
        "for",
        "if",
        "impl",
        "in",
        "let",
        "loop",
        "match",
        "mod",
        "move",
        "mut",
        "pub",
        "ref",
        "return",
        "self",
        "static",
        "struct",
        "super",
        "trait",
        "true",
        "type",
        "unsafe",
        "use",
        "where",
        "while",
        "abstract",
        "become",
        "box",
        "do",
        "final",
        "gen",
        "macro",
        "override",
        "priv",
        "try",
        "typeof",
        "unsized",
        "virtual",
        "yield",
    ]
)
# Names the generated file defines or uses besides the DSL classes; a DSL class must not
# shadow any of them.
RESERVED_NAMES = frozenset(
    {"Node", "NodeKind", "FieldSchema", "ClassSchema", "EnumSchema", "Option", "Box", "Some"}
    | {"None", "Ok", "Err", "Result", "Vec", "String", "Self"}
    | set(HOOK_NAMES)
    | CONTRACT_TYPES
)


@dataclasses.dataclass(frozen=True)
class Ty:
    """A parsed annotation: a name, a tuple (variadic or fixed) or a union."""

    kind: str  # "name" | "tuple" | "union"
    name: str = ""
    args: tuple[Ty, ...] = ()
    variadic: bool = False


_TOKEN = re.compile(r"\s*(\.\.\.|[A-Za-z_][\w.]*|[\[\],|])")


def parse_annotation(text: str) -> Ty:
    tokens: list[str] = []
    pos = 0
    while pos < len(text):
        m = _TOKEN.match(text, pos)
        if m is None:
            raise SystemExit(f"cannot parse annotation {text!r} at {pos}")
        tokens.append(m.group(1))
        pos = m.end()
        while pos < len(text) and text[pos].isspace():
            pos += 1
    ty, rest = _parse_union(tokens)
    if rest:
        raise SystemExit(f"trailing tokens in annotation {text!r}: {rest}")
    return ty


def _parse_union(tokens: list[str]) -> tuple[Ty, list[str]]:
    first, rest = _parse_atom(tokens)
    members = [first]
    while rest and rest[0] == "|":
        nxt, rest = _parse_atom(rest[1:])
        members.append(nxt)
    if len(members) == 1:
        return first, rest
    return Ty("union", args=tuple(members)), rest


def _parse_atom(tokens: list[str]) -> tuple[Ty, list[str]]:
    name, rest = tokens[0], tokens[1:]
    if name.startswith("d."):
        name = name[2:]
    if name != "tuple":
        return Ty("name", name=name), rest
    if not rest or rest[0] != "[":
        raise SystemExit(f"bare tuple annotation: {tokens}")
    rest = rest[1:]
    args: list[Ty] = []
    variadic = False
    while True:
        arg, rest = _parse_union(rest)
        args.append(arg)
        if rest[0] == ",":
            rest = rest[1:]
            if rest[0] == "...":
                variadic = True
                rest = rest[1:]
        if rest[0] == "]":
            return Ty("tuple", args=tuple(args), variadic=variadic), rest[1:]


class Gen:
    def __init__(self) -> None:
        load_bindings()
        self.enums = self._enums()
        self.enum_names = {e.__name__ for e in self.enums}
        self.dsl_classes = _module_dataclasses(dsl)
        self.program_classes = _module_dataclasses(program)
        self.instr_classes = [c for c in self.program_classes if c.__name__ != "Program"]
        self.node_classes = [*self.dsl_classes, *self.instr_classes]
        self.aliases = self._aliases()
        self.hooks = self._hooks()
        self.used_contract_types: set[str] = set()
        self._check()

    # --- collection -----------------------------------------------------------------------

    @staticmethod
    def _enums() -> list[type[enum.Enum]]:
        found = [
            obj
            for obj in vars(dsl).values()
            if isinstance(obj, type)
            and issubclass(obj, enum.Enum)
            and obj.__module__ == dsl.__name__
        ]
        if vars(dsl).get("Duration") is not engine_types.Duration:
            raise SystemExit("dsl.Duration is not engine.types.Duration")
        found.append(engine_types.Duration)
        for e in found:
            if not issubclass(e, enum.StrEnum):
                raise SystemExit(f"{e.__name__} is not a StrEnum")
        return found

    @staticmethod
    def _aliases() -> list[tuple[str, str, list[str]]]:
        out = []
        for mod in (dsl, program):
            for name, obj in vars(mod).items():
                if isinstance(obj, typing.TypeAliasType):
                    members = _alias_members(obj)
                    out.append((name, _source_line_of_alias(mod, name), members))
        return out

    @staticmethod
    def _hooks() -> dict[str, list[tuple[str, str, Callable[..., object]]]]:
        tables: dict[str, Mapping[str, Callable[..., object]]] = {
            "StepHook": interp.CUSTOM_STEPS,
            "CondHook": view.CUSTOM_CONDS,
            "FilterHook": view.CUSTOM_FILTERS,
            "ValueHook": view.CUSTOM_VALUES,
        }
        out: dict[str, list[tuple[str, str, Callable[..., object]]]] = {}
        for enum_name, table in tables.items():
            names = HOOK_NAMES[enum_name]
            if set(table) != set(names):
                raise SystemExit(
                    f"{enum_name}: registered hooks {sorted(table)} != table {sorted(names)}"
                )
            out[enum_name] = [(py, names[py], fn) for py, fn in table.items()]
        return out

    def _check(self) -> None:
        if len(self.dsl_classes) != EXPECTED_DSL_CLASSES:
            raise SystemExit(
                f"{len(self.dsl_classes)} DSL classes, expected {EXPECTED_DSL_CLASSES}"
            )
        if len(self.program_classes) != EXPECTED_PROGRAM_CLASSES:
            raise SystemExit(f"{len(self.program_classes)} program classes")
        if len(self.enums) != EXPECTED_ENUMS:
            raise SystemExit(f"{len(self.enums)} enums, expected {EXPECTED_ENUMS}")
        names = [c.__name__ for c in [*self.dsl_classes, *self.program_classes]]
        clash = (set(names) & (RESERVED_NAMES | self.enum_names)) - {"Program"}
        if clash or len(set(names)) != len(names):
            raise SystemExit(f"class name clash: {sorted(clash)}")
        for cls in self.node_classes:
            for f in dataclasses.fields(cls):
                if (cls.__name__, f.name) in VAR_FIELDS and f.type not in (
                    "str",
                    "tuple[str, ...]",
                ):
                    raise SystemExit(f"var field {cls.__name__}.{f.name} is {f.type!r}")
                if (cls.__name__, f.name) in HOOK_FIELDS and f.type != "str":
                    raise SystemExit(f"hook field {cls.__name__}.{f.name} is {f.type!r}")
        for cls_name, field_name in [*VAR_FIELDS, *HOOK_FIELDS]:
            cls = getattr(dsl, cls_name, None) or getattr(program, cls_name)
            if field_name not in {f.name for f in dataclasses.fields(cls)}:
                raise SystemExit(f"override names a missing field {cls_name}.{field_name}")

    # --- type mapping ---------------------------------------------------------------------

    def rust_type(self, cls: str, field: str, ty: Ty) -> str:
        if ty.kind == "union":
            others = [a for a in ty.args if not (a.kind == "name" and a.name == "None")]
            if len(others) != 1 or len(ty.args) != 2:
                raise SystemExit(f"{cls}.{field}: only `X | None` unions are supported")
            return f"Option<{self.rust_type(cls, field, others[0])}>"
        if ty.kind == "tuple":
            inner = [self.rust_type(cls, field, a) for a in ty.args]
            if ty.variadic:
                if len(inner) != 1:
                    raise SystemExit(f"{cls}.{field}: variadic tuple with {len(inner)} args")
                return f"Box<[{inner[0]}]>"
            return "(" + ", ".join(inner) + ")"
        if ty.name == "str" and (cls, field) in VAR_FIELDS:
            self.used_contract_types.add("VarId")
            return "VarId"
        if ty.name == "str" and (cls, field) in HOOK_FIELDS:
            return HOOK_FIELDS[(cls, field)]
        if ty.name in self.enum_names:
            return ty.name
        if ty.name not in NAMED_TYPES:
            raise SystemExit(f"{cls}.{field}: unmapped annotation name {ty.name!r}")
        rust = NAMED_TYPES[ty.name]
        if rust in CONTRACT_TYPES:
            self.used_contract_types.add(rust)
        return rust

    def is_copy(self, rust: str) -> bool:
        if rust.startswith("Box<"):
            return False
        if rust.startswith("Option<"):
            return self.is_copy(rust[len("Option<") : -1])
        if rust.startswith("("):
            return all(self.is_copy(p.strip()) for p in _split_top(rust[1:-1]))
        return rust in COPY_TYPES or rust in self.enum_names or rust in HOOK_NAMES

    # --- rendering ------------------------------------------------------------------------

    def render(self) -> str:
        body: list[str] = []
        body += self.render_enums()
        body += self.render_hooks()
        body += self.render_classes()
        body += self.render_node()
        body += self.render_schema()
        body += self.render_codec()
        body += self.render_tests()
        uses = sorted(self.used_contract_types)
        head = [
            "//! Effect DSL types, generated by `oracle/gen_dsl.py` from the Python reference;",
            "//! do not edit by hand (`uv run python oracle/gen_dsl.py --write` regenerates it,",
            "//! `--check` fails when it is stale).",
            "//!",
            "//! Contents: the 13 DSL `StrEnum`s and `Duration` (declaration order as discriminants,",
            "//! Python value strings, member names for `repr`), the 34 custom-hook names, one struct",
            "//! per DSL and instruction dataclass with Python's field order, the `Node` and",
            "//! `NodeKind` enums, the kind sets of the `type` aliases, and the schema table that",
            "//! the IR loader compares with the IR (docs/dev/PLAN.md §3.4, §5.1), and the per-class",
            "//! glue of the codec (`Node::decode`, `Node::repr_into`, `Node::golden`), which",
            "//! applies `crate::codec::Field` to every field in declaration order. Field types:",
            "//! `int` is `i64`, `str` is `StrId` (`VarId` for frame-variable names), `Value` is",
            "//! `ValueRef`, every other node type is its typed `NodeId` handle, `X | None` is",
            "//! `Option<X>` and `tuple[X, ...]` is `Box<[X]>`.",
            "//!",
            "//! Sources (SHA-256):",
            *(f"//! - `{p.relative_to(ROOT).as_posix()}` `{_sha256(p)}`" for p in SOURCES),
            "",
            f"use crate::contracts::{{{', '.join(uses)}}};",
            "",
        ]
        return "\n".join([*head, *body]) + "\n"

    def render_enums(self) -> list[str]:
        out: list[str] = []
        for e in self.enums:
            members = list(e)
            src = _source_of(e)
            variants = [(m, _camel(m.name)) for m in members]
            out.append(f"/// Python `{e.__name__}` (`{src}`), a `StrEnum`: discriminants are the")
            out.append(
                "/// declaration order, which is also the derived `Ord`; sort by value only through"
            )
            out.append("/// `SORTED_BY_VALUE`.")
            out.append("#[repr(u8)]")
            out.append("#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]")
            out.append(f"pub enum {e.__name__} {{")
            for i, (m, v) in enumerate(variants):
                out.append(f"    /// `{m.name} = {m.value!r}`")
                out.append(f"    {v} = {i},")
            out.append("}")
            out.append("")
            n = len(members)
            by_value = sorted(members, key=lambda m: m.value)
            out.append(f"impl {e.__name__} {{")
            out.append(f'    pub const PY_CLASS: &\'static str = "{e.__name__}";')
            out.append("    /// Declaration order.")
            out.append(
                f"    pub const ALL: [Self; {n}] = ["
                + ", ".join(f"Self::{_camel(m.name)}" for m in members)
                + "];"
            )
            out.append("    /// Sorted by value string, as Python's `sorted()` orders a `StrEnum`.")
            out.append(
                f"    pub const SORTED_BY_VALUE: [Self; {n}] = ["
                + ", ".join(f"Self::{_camel(m.name)}" for m in by_value)
                + "];"
            )
            out.append("")
            out.append("    /// The member's value string.")
            out.append("    #[must_use]")
            out.append("    pub const fn as_str(self) -> &'static str {")
            out.append("        match self {")
            out += [f"            Self::{v} => {_rs_str(m.value)}," for m, v in variants]
            out.append("        }")
            out.append("    }")
            out.append("")
            out.append(
                "    /// The member's Python name, as `repr` prints it (`<Kw.REPAIR: 'Repair'>`)."
            )
            out.append("    #[must_use]")
            out.append("    pub const fn py_name(self) -> &'static str {")
            out.append("        match self {")
            out += [f"            Self::{v} => {_rs_str(m.name)}," for m, v in variants]
            out.append("        }")
            out.append("    }")
            out.append("")
            out.append("    /// The member whose value string is `value`.")
            out.append("    #[must_use]")
            out.append("    pub fn from_value(value: &str) -> Option<Self> {")
            out.append("        match value {")
            out += [f"            {_rs_str(m.value)} => Some(Self::{v})," for m, v in variants]
            out.append("            _ => None,")
            out.append("        }")
            out.append("    }")
            out.append("}")
            out.append("")
        return out

    def render_hooks(self) -> list[str]:
        out: list[str] = []
        kinds = {
            "StepHook": ("CustomStep", "interp.CUSTOM_STEPS", "engine/interp.py:36"),
            "CondHook": ("CustomCond", "view.CUSTOM_CONDS", "engine/view.py:118"),
            "FilterHook": ("CustomFilter", "view.CUSTOM_FILTERS", "engine/view.py:116"),
            "ValueHook": ("CustomValue", "view.CUSTOM_VALUES", "engine/view.py:117"),
        }
        for enum_name, hooks in self.hooks.items():
            node, table, line = kinds[enum_name]
            n = len(hooks)
            out.append(f"/// The `{node}.name` hooks registered in `{table}` (`{line}`), in")
            out.append(
                "/// registration order; the strings are the Python hook names. A name outside"
            )
            out.append("/// this enum fails the IR load with the card number (W-69).")
            out.append("#[repr(u8)]")
            out.append("#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]")
            out.append(f"pub enum {enum_name} {{")
            for i, (py, rust, fn) in enumerate(hooks):
                out.append(f"    /// `{py}` (`{_fn_source(fn)}`)")
                out.append(f"    {rust} = {i},")
            out.append("}")
            out.append("")
            out.append(f"impl {enum_name} {{")
            out.append("    /// Registration order.")
            out.append(
                f"    pub const ALL: [Self; {n}] = ["
                + ", ".join(f"Self::{r}" for _, r, _ in hooks)
                + "];"
            )
            out.append("")
            out.append("    /// The Python hook name.")
            out.append("    #[must_use]")
            out.append("    pub const fn as_str(self) -> &'static str {")
            out.append("        match self {")
            out += [f"            Self::{r} => {_rs_str(py)}," for py, r, _ in hooks]
            out.append("        }")
            out.append("    }")
            out.append("")
            out.append("    /// The hook registered under `name`.")
            out.append("    #[must_use]")
            out.append("    pub fn from_name(name: &str) -> Option<Self> {")
            out.append("        match name {")
            out += [f"            {_rs_str(py)} => Some(Self::{r})," for py, r, _ in hooks]
            out.append("            _ => None,")
            out.append("        }")
            out.append("    }")
            out.append("}")
            out.append("")
        return out

    def render_classes(self) -> list[str]:
        out: list[str] = []
        for cls in self.node_classes:
            fields = dataclasses.fields(cls)
            typed = [
                (f, self.rust_type(cls.__name__, f.name, parse_annotation(str(f.type))))
                for f in fields
            ]
            copy = all(self.is_copy(t) for _, t in typed)
            derives = (
                "Clone, Copy, PartialEq, Eq, Hash, Debug"
                if copy
                else "Clone, PartialEq, Eq, Hash, Debug"
            )
            out.append(f"/// Python `{cls.__name__}` (`{_source_of(cls)}`).")
            out.append(f"#[derive({derives})]")
            if not typed:
                out.append(f"pub struct {cls.__name__};")
                out.append("")
                continue
            out.append(f"pub struct {cls.__name__} {{")
            for f, rust in typed:
                default = _default_repr(f)
                suffix = f" = {default}" if default is not None else ""
                doc = f"`{f.name}: {f.type}{suffix}`"
                if "`" in str(f.type) + (default or ""):
                    raise SystemExit(f"backtick in {cls.__name__}.{f.name}")
                out.append(f"    /// {doc}")
                out.append(f"    pub {_field_ident(f.name)}: {rust},")
            out.append("}")
            out.append("")
        return out

    def render_node(self) -> list[str]:
        out: list[str] = []
        names = [c.__name__ for c in self.node_classes]
        out.append(
            "/// One hash-consed IR node: every DSL class and every program instruction class"
        )
        out.append("/// (`Program` itself is `crate::contracts::Program`). Index = `NodeId`.")
        out.append("#[derive(Clone, PartialEq, Eq, Hash, Debug)]")
        out.append("pub enum Node {")
        out += [f"    {n}({n})," for n in names]
        out.append("}")
        out.append("")
        out.append(
            "/// The class of a [`Node`]; the discriminant is the class's index in [`CLASSES`]."
        )
        out.append("#[repr(u8)]")
        out.append("#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]")
        out.append("pub enum NodeKind {")
        out += [f"    {n} = {i}," for i, n in enumerate(names)]
        out.append("}")
        out.append("")
        out.append("impl Node {")
        out.append("    #[must_use]")
        out.append("    pub const fn kind(&self) -> NodeKind {")
        out.append("        match self {")
        out += [f"            Self::{n}(_) => NodeKind::{n}," for n in names]
        out.append("        }")
        out.append("    }")
        out.append("}")
        out.append("")
        out.append("impl NodeKind {")
        out.append(f"    pub const ALL: [Self; {len(names)}] = [")
        out += [f"        Self::{n}," for n in names]
        out.append("    ];")
        out.append("")
        for alias, src, members in self.aliases:
            const = _upper_snake(alias)
            literal = " (plus the literal `int`, `ValueRef::Lit`)" if "int" in members else ""
            out.append(f"    /// Members of `type {alias}` (`{src}`){literal}.")
            kinds = [m for m in members if m != "int"]
            out.append(f"    pub const {const}: &'static [Self] = &[")
            out += [f"        Self::{m}," for m in kinds]
            out.append("    ];")
            out.append("")
        out.append("    /// The Python class name.")
        out.append("    #[must_use]")
        out.append("    pub const fn py_name(self) -> &'static str {")
        out.append("        CLASSES[self as usize].name")
        out.append("    }")
        out.append("")
        out.append("    #[must_use]")
        out.append("    pub const fn schema(self) -> &'static ClassSchema {")
        out.append("        &CLASSES[self as usize]")
        out.append("    }")
        out.append("}")
        out.append("")
        return out

    def render_schema(self) -> list[str]:
        out = [
            "/// One dataclass field as `dataclasses.fields()` reports it.",
            "#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]",
            "pub struct FieldSchema {",
            "    pub name: &'static str,",
            "    /// The annotation string (`from __future__ import annotations`).",
            "    pub annotation: &'static str,",
            "    /// `repr()` of the default (`default`, or `default_factory()`); `None` when the",
            "    /// field has no default. Python's `dataclass` `repr` prints a field equal to its",
            "    /// default exactly like this text, and golden files omit such fields.",
            "    pub default_repr: Option<&'static str>,",
            "}",
            "",
            "/// One dataclass: its name, source line and fields in declaration order.",
            "#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]",
            "pub struct ClassSchema {",
            "    pub name: &'static str,",
            "    pub source: &'static str,",
            "    pub fields: &'static [FieldSchema],",
            "}",
            "",
            "/// One `StrEnum`: `(member name, value)` in declaration order.",
            "#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]",
            "pub struct EnumSchema {",
            "    pub name: &'static str,",
            "    pub source: &'static str,",
            "    pub members: &'static [(&'static str, &'static str)],",
            "}",
            "",
            "/// Every DSL class (declaration order), then every `effects/program.py` class",
            "/// (declaration order, `Program` last). `CLASSES[k as usize]` describes `NodeKind` `k`.",
            "pub const CLASSES: &[ClassSchema] = &[",
        ]
        for cls in [*self.dsl_classes, *self.program_classes]:
            out.append("    ClassSchema {")
            out.append(f"        name: {_rs_str(cls.__name__)},")
            out.append(f"        source: {_rs_str(_source_of(cls))},")
            fields = dataclasses.fields(cls)
            if not fields:
                out.append("        fields: &[],")
            else:
                out.append("        fields: &[")
                for f in fields:
                    d = _default_repr(f)
                    dr = "None" if d is None else f"Some({_rs_str(d)})"
                    out.append(
                        f"            FieldSchema {{ name: {_rs_str(f.name)}, "
                        f"annotation: {_rs_str(str(f.type))}, default_repr: {dr} }},"
                    )
                out.append("        ],")
            out.append("    },")
        out.append("];")
        out.append("")
        out.append("/// Every enum of the DSL (declaration order), then `Duration`.")
        out.append("pub const ENUMS: &[EnumSchema] = &[")
        for e in self.enums:
            out.append("    EnumSchema {")
            out.append(f"        name: {_rs_str(e.__name__)},")
            out.append(f"        source: {_rs_str(_source_of(e))},")
            out.append("        members: &[")
            out += [f"            ({_rs_str(m.name)}, {_rs_str(m.value)})," for m in e]
            out.append("        ],")
            out.append("    },")
        out.append("];")
        return out

    def render_codec(self) -> list[str]:
        """The per-class glue of ``crate::codec``: decode from the IR node object, CPython
        ``repr`` (``Name(f1=..., f2=...)``, ``This()``) and the golden JSON."""
        hook_kinds = ", ".join(
            f'{name} => "{name.removesuffix("Hook").lower()}"' for name in self.hooks
        )
        out = [
            "",
            "crate::codec::enum_field!(" + ", ".join(e.__name__ for e in self.enums) + ");",
            f"crate::codec::hook_field!({hook_kinds});",
            "",
            "impl Node {",
            "    /// Decodes an IR node object of class `kind` (every field present, declaration",
            "    /// order; docs/dev/PLAN.md §3.3).",
            "    pub(crate) fn decode(",
            "        kind: NodeKind,",
            "        obj: &serde_json::Map<String, serde_json::Value>,",
            "        d: &mut crate::codec::Decoder<'_>,",
            "    ) -> Result<Self, crate::contracts::IrError> {",
            "        Ok(match kind {",
        ]
        for cls in self.node_classes:
            name = cls.__name__
            fields = dataclasses.fields(cls)
            out.append(f"            NodeKind::{name} => {{")
            if not fields:
                out.append("                d.no_fields(obj)?;")
                out.append(f"                Self::{name}({name})")
                out.append("            }")
                continue
            out.append(f"                d.exact_fields(obj, {len(fields)})?;")
            out.append(f"                Self::{name}({name} {{")
            for f in fields:
                ident = _field_ident(f.name)
                out.append(f"                    {ident}: d.field(obj, {_rs_str(f.name)})?,")
            out.append("                })")
            out.append("            }")
        out += [
            "        })",
            "    }",
            "",
            "    /// CPython `repr` (dataclass `repr`: all fields in declaration order).",
            "    pub(crate) fn repr_into(&self, cx: &crate::codec::ReprCx<'_>, out: &mut String) {",
            "        use crate::codec::Field as _;",
            "        match self {",
        ]
        for cls in self.node_classes:
            name = cls.__name__
            fields = dataclasses.fields(cls)
            if not fields:
                out.append(f'            Self::{name}(_) => out.push_str("{name}()"),')
                continue
            out.append(f"            Self::{name}(x) => {{")
            for i, f in enumerate(fields):
                head = f"{name}(" if i == 0 else ", "
                out.append(f"                out.push_str({_rs_str(head + f.name + '=')});")
                out.append(f"                x.{_field_ident(f.name)}.repr(cx, out);")
            out.append("                out.push(')');")
            out.append("            }")
        out += [
            "        }",
            "    }",
            "",
            "    /// `golden.node_to_json` (`effects/golden.py:19-37`): fields equal to their",
            "    /// default are omitted.",
            "    pub(crate) fn golden(&self, cx: &crate::codec::ReprCx<'_>) -> gcg_core::pyjson::Json {",
            "        let mut g = crate::codec::Golden::new(self.kind(), cx);",
            "        match self {",
        ]
        for cls in self.node_classes:
            name = cls.__name__
            fields = dataclasses.fields(cls)
            if not fields:
                out.append(f"            Self::{name}(_) => {{}}")
                continue
            if len(fields) == 1:
                ident = _field_ident(fields[0].name)
                out.append(f"            Self::{name}(x) => g.field(0, &x.{ident}),")
                continue
            out.append(f"            Self::{name}(x) => {{")
            for i, f in enumerate(fields):
                out.append(f"                g.field({i}, &x.{_field_ident(f.name)});")
            out.append("            }")
        out += [
            "        }",
            "        g.finish()",
            "    }",
            "}",
        ]
        return out

    def render_tests(self) -> list[str]:
        n_classes = len(self.dsl_classes) + len(self.program_classes)
        out = [
            "",
            "#[cfg(test)]",
            "mod tests {",
            "    use super::*;",
            "",
            "    #[test]",
            "    fn counts_match_the_python_reference() {",
            f"        assert_eq!(CLASSES.len(), {n_classes});",
            f"        assert_eq!(NodeKind::ALL.len(), {len(self.node_classes)});",
            f"        assert_eq!(ENUMS.len(), {len(self.enums)});",
            *(
                f"        assert_eq!({name}::ALL.len(), {len(hooks)});"
                for name, hooks in self.hooks.items()
            ),
            "    }",
            "",
            "    #[test]",
            "    fn node_kinds_index_the_schema() {",
            "        for (i, k) in NodeKind::ALL.into_iter().enumerate() {",
            "            assert_eq!(k as usize, i);",
            "            assert_eq!(k.schema().name, CLASSES[i].name);",
            "        }",
            '        assert_eq!(CLASSES[CLASSES.len() - 1].name, "Program");',
            "    }",
            "",
            "    #[test]",
            "    fn enums_round_trip_and_match_the_schema() {",
        ]
        for i, e in enumerate(self.enums):
            name = e.__name__
            out += [
                "        {",
                f"            let schema = &ENUMS[{i}];",
                f"            assert_eq!(schema.name, {name}::PY_CLASS);",
                f"            assert_eq!(schema.members.len(), {name}::ALL.len());",
                f"            for (j, x) in {name}::ALL.into_iter().enumerate() {{",
                "                assert_eq!(x as usize, j);",
                "                assert_eq!(schema.members[j], (x.py_name(), x.as_str()));",
                f"                assert_eq!({name}::from_value(x.as_str()), Some(x));",
                "            }",
                f"            let sorted = {name}::SORTED_BY_VALUE.map({name}::as_str);",
                "            assert!(sorted.is_sorted());",
                "        }",
            ]
        out.append("    }")
        out.append("")
        out.append("    #[test]")
        out.append("    fn hook_names_round_trip() {")
        for name in self.hooks:
            out += [
                f"        for (i, h) in {name}::ALL.into_iter().enumerate() {{",
                "            assert_eq!(h as usize, i);",
                f"            assert_eq!({name}::from_name(h.as_str()), Some(h));",
                "        }",
            ]
        out.append("    }")
        out.append("}")
        return out


def _module_dataclasses(mod: object) -> list[type]:
    return [
        obj
        for obj in vars(mod).values()
        if isinstance(obj, type)
        and dataclasses.is_dataclass(obj)
        and obj.__module__ == getattr(mod, "__name__", None)
    ]


def _alias_members(alias: typing.TypeAliasType) -> list[str]:
    out: list[str] = []
    for arg in typing.get_args(alias.__value__) or (alias.__value__,):
        if isinstance(arg, typing.TypeAliasType):
            out += _alias_members(arg)
        else:
            out.append(arg.__name__)
    return out


def _source_line_of_alias(mod: object, name: str) -> str:
    path = Path(inspect.getsourcefile(mod))  # type: ignore[arg-type]
    for i, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if line.startswith(f"type {name} ="):
            return f"{path.relative_to(PKG).as_posix()}:{i}"
    raise SystemExit(f"type alias {name} not found in {path}")


def _source_of(obj: type) -> str:
    path = Path(inspect.getsourcefile(obj))  # type: ignore[arg-type]
    return f"{path.relative_to(PKG).as_posix()}:{inspect.getsourcelines(obj)[1]}"


def _fn_source(fn: Callable[..., object]) -> str:
    path = Path(inspect.getsourcefile(fn))  # type: ignore[arg-type]
    return f"{path.relative_to(PKG).as_posix()}:{inspect.getsourcelines(fn)[1]}"


def _default_repr(f: dataclasses.Field[object]) -> str | None:
    if f.default is not dataclasses.MISSING:
        return repr(f.default)
    if f.default_factory is not dataclasses.MISSING:
        return repr(f.default_factory())
    return None


def _camel(name: str) -> str:
    return "".join(part[:1] + part[1:].lower() for part in name.split("_"))


def _upper_snake(name: str) -> str:
    return re.sub(r"(?<!^)(?=[A-Z])", "_", name).upper()


def _field_ident(name: str) -> str:
    if name in {"self", "super", "crate", "Self"}:
        raise SystemExit(f"field name {name!r} cannot be a Rust identifier")
    return f"r#{name}" if name in RUST_KEYWORDS else name


def _rs_str(s: str) -> str:
    out = []
    for ch in s:
        if ch in '"\\':
            out.append("\\" + ch)
        elif ch.isprintable():
            out.append(ch)
        else:
            out.append(f"\\u{{{ord(ch):x}}}")
    return '"' + "".join(out) + '"'


def _split_top(s: str) -> Iterable[str]:
    depth, start = 0, 0
    for i, ch in enumerate(s):
        if ch in "<([":
            depth += 1
        elif ch in ">)]":
            depth -= 1
        elif ch == "," and depth == 0:
            yield s[start:i]
            start = i + 1
    yield s[start:]


def _sha256(p: Path) -> str:
    return hashlib.sha256(p.read_bytes()).hexdigest()


def _rustfmt(text: str) -> str:
    exe = shutil.which("rustfmt") or str(Path.home() / ".cargo" / "bin" / "rustfmt")
    if not os.access(exe, os.X_OK):
        raise SystemExit("rustfmt not found (install the rustfmt component)")
    res = subprocess.run(
        [exe, "--edition", "2024", "--emit", "stdout"],
        input=text,
        capture_output=True,
        text=True,
        check=False,
        cwd=ROOT,
    )
    if res.returncode != 0:
        raise SystemExit(f"rustfmt failed:\n{res.stderr}")
    return res.stdout


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    mode = ap.add_mutually_exclusive_group(required=True)
    mode.add_argument("--write", action="store_true", help=f"write {OUT.relative_to(ROOT)}")
    mode.add_argument("--check", action="store_true", help="exit 1 if the file is stale")
    args = ap.parse_args()
    text = _rustfmt(Gen().render())
    rel = OUT.relative_to(ROOT)
    if args.check:
        if not OUT.exists() or OUT.read_text(encoding="utf-8") != text:
            print(f"{rel} is stale; run oracle/gen_dsl.py --write", file=sys.stderr)
            return 1
        print(f"{rel} is current")
        return 0
    OUT.write_text(text, encoding="utf-8")
    print(f"wrote {rel}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
