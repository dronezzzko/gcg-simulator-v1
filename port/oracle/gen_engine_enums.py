"""Generate the engine enums and constants of crates/gcg-engine/src/enums.rs from the Python reference.

Two sources feed the file:

- Introspection of ``src/gcg_sim/engine``: the ``IntEnum``s ``Zone``, ``Phase`` and ``Step``
  (discriminant = Python value), the ``StrEnum``s ``DecisionKind``, ``ActionKind`` and
  ``EndReason`` (declaration order as discriminants, value strings, member names), the zone
  sets, and the module constants of ``types``, ``state``, ``core``, ``game``, ``interp``,
  ``observe``, ``invariants`` and ``testkit``.
- An AST scan of every module under ``src/gcg_sim`` for the string literals that the engine
  writes into the state: ``HistKind`` (the ``kind`` of every ``record`` call), ``EvKey`` (the
  keyword arguments of every ``emit`` call plus ``subject``/``player``/``group``), ``FrameKind``
  (the ``kind`` of every ``push_frame`` call), ``EnginePrompt`` and ``CtxKey`` (the literal
  prompts and context keys of every ``Decision`` and ``_decide`` call). These enums are sorted
  by value, so their derived ``Ord`` is Python's string order (``EvKey`` sorts the event
  extras with it, ``core.py:331``). A call site the scan cannot read is an error unless its
  enclosing function is listed in ``PASS_THROUGH`` (it forwards a value it received).

The output goes through ``rustfmt --edition 2024`` so that ``cargo fmt`` leaves it unchanged.

    uv run python port/oracle/gen_engine_enums.py --write
    uv run python port/oracle/gen_engine_enums.py --check    # exit 1 if enums.rs is stale
"""

from __future__ import annotations

import argparse
import ast
import enum
import hashlib
import inspect
import os
import re
import shutil
import subprocess
import sys
from dataclasses import dataclass, field
from pathlib import Path

from gcg_sim import testkit
from gcg_sim.engine import core, game, interp, invariants, observe, state
from gcg_sim.engine import types as T

ROOT = Path(__file__).resolve().parents[2]
PKG = ROOT / "src" / "gcg_sim"
OUT = ROOT / "crates" / "gcg-engine" / "src" / "enums.rs"

INT_ENUMS: tuple[type[enum.IntEnum], ...] = (T.Zone, T.Phase, T.Step)
STR_ENUMS: tuple[type[enum.StrEnum], ...] = (T.DecisionKind, T.ActionKind, T.EndReason)

# Zone sets: (Rust name, Python module, attribute).
ZONE_SETS = (
    ("PUBLIC_ZONES", T, "PUBLIC_ZONES"),
    ("FIELD_ZONES", T, "FIELD_ZONES"),
    ("ORDERED_ZONES", T, "ORDERED_ZONES"),
    ("TOKEN_ZONES", core, "TOKEN_ZONES"),
    ("HIDDEN_ZONES", observe, "HIDDEN_ZONES"),
)
# Zone tuples whose order is iteration order (kept as is).
ZONE_TUPLES = (("LISTED_ZONES", invariants, "LISTED_ZONES"),)

# Module constants: (Rust name, Python module, attribute, Rust type, cfg feature or None).
CONSTANTS = (
    ("NO_ARG", T, "NO_ARG", "i32", None),
    ("PLAYER_TARGET", T, "PLAYER_TARGET", "i32", None),
    ("N_ZONES", state, "N_ZONES", "usize", None),
    ("BOTH_KNOW", core, "BOTH_KNOW", "u8", None),
    ("HAND_LIMIT", core, "HAND_LIMIT", "usize", None),
    ("BATTLE_LIMIT", core, "BATTLE_LIMIT", "usize", None),
    ("BASE_LIMIT", core, "BASE_LIMIT", "usize", None),
    ("RESOURCE_LIMIT", core, "RESOURCE_LIMIT", "usize", None),
    ("EX_RESOURCE_LIMIT", core, "EX_RESOURCE_LIMIT", "usize", None),
    ("SHIELD_COUNT", core, "SHIELD_COUNT", "usize", None),
    ("START_HAND", core, "START_HAND", "usize", None),
    ("REDUCE_ONCE_TAG", core, "REDUCE_ONCE_TAG", "i32", None),
    ("PREVENT_ONCE_TAG", core, "PREVENT_ONCE_TAG", "i32", None),
    ("REST_SUB_TAG", core, "REST_SUB_TAG", "i32", None),
    ("SUPPORT_AID", game, "SUPPORT_AID", "i32", None),
    ("ALT_PLAY_BASE", game, "ALT_PLAY_BASE", "i32", None),
    ("MAX_ACTIONS_DEFAULT", game, "MAX_ACTIONS_DEFAULT", "i64", None),
    ("MAIN_DECK_SIZE", game, "MAIN_DECK_SIZE", "usize", None),
    ("RESOURCE_DECK_SIZE", game, "RESOURCE_DECK_SIZE", "usize", None),
    ("MAX_COPIES", game, "MAX_COPIES", "usize", None),
    ("MAX_COLORS", game, "MAX_COLORS", "usize", None),
    ("BURST_PROMPT", interp, "BURST_PROMPT", "&str", None),
    ("HOLD_RULES", interp, "HOLD_RULES", "&str", None),
    ("FILLER_UNIT", testkit, "FILLER_UNIT", "&str", "testkit"),
    ("FILLER_RESOURCE", testkit, "FILLER_RESOURCE", "&str", "testkit"),
)

# Functions that forward a prompt, context or frame kind they received (no literal to read).
PASS_THROUGH = frozenset(
    {
        ("engine/interp.py", "_decide"),  # Decision(player, kind, options, prompt, context)
        ("engine/state.py", "from_json"),  # Decision.from_json
        ("ai/mcts.py", "_add_children"),  # a Decision copy with fewer options (mcts.py:310)
    }
)

# Scanned string enums: (Rust name, Python description, derive Default).
SCANNED = (
    ("HistKind", "the `kind` of `HistoryEvent` (`core.record`, `engine/core.py:142-151`)", False),
    ("EvKey", "an event payload key (`core.emit`, `engine/core.py:316-356`)", True),
    ("FrameKind", "`Frame.kind` (`interp.push_frame`, `engine/interp.py:61-88`)", False),
    ("EnginePrompt", "a literal `Decision.prompt` of the engine", False),
    ("CtxKey", "a key of `Decision.context` (`engine/state.py:43-49`)", True),
)


@dataclass
class Scan:
    values: dict[str, dict[str, list[str]]] = field(
        default_factory=lambda: {name: {} for name, _, _ in SCANNED}
    )
    ask_prompt_sites: list[str] = field(default_factory=list)
    choose_prompt_sites: list[str] = field(default_factory=list)

    def add(self, enum_name: str, value: str, site: str) -> None:
        self.values[enum_name].setdefault(value, []).append(site)


def _call_name(node: ast.Call) -> str | None:
    fn = node.func
    if isinstance(fn, ast.Name):
        return fn.id
    if isinstance(fn, ast.Attribute) and isinstance(fn.value, ast.Name):
        return f"{fn.value.id}.{fn.attr}"
    return None


def _str_const(node: ast.expr | None) -> str | None:
    if isinstance(node, ast.Constant) and isinstance(node.value, str):
        return node.value
    return None


def _arg(call: ast.Call, index: int, name: str) -> ast.expr | None:
    for kw in call.keywords:
        if kw.arg == name:
            return kw.value
    return call.args[index] if index < len(call.args) else None


class _Scanner(ast.NodeVisitor):
    def __init__(self, rel: str, scan: Scan) -> None:
        self.rel = rel
        self.scan = scan
        self.functions: list[ast.FunctionDef] = []

    def _site(self, node: ast.AST) -> str:
        return f"{self.rel}:{getattr(node, 'lineno', 0)}"

    def _fn(self) -> str:
        return self.functions[-1].name if self.functions else "<module>"

    def _fail(self, node: ast.AST, what: str) -> None:
        raise SystemExit(f"{self._site(node)}: cannot read {what} (function {self._fn()})")

    def _pass_through(self) -> bool:
        return (self.rel, self._fn()) in PASS_THROUGH

    def visit_FunctionDef(self, node: ast.FunctionDef) -> None:
        self.functions.append(node)
        self.generic_visit(node)
        self.functions.pop()

    def visit_Call(self, node: ast.Call) -> None:
        name = _call_name(node)
        if name in {"record", "core.record"} and (self.rel == "engine/core.py") == (
            name == "record"
        ):
            self._record(node)
        elif name in {"emit", "core.emit"} and (self.rel == "engine/core.py") == (name == "emit"):
            self._emit(node)
        elif name in {"push_frame", "I.push_frame"} and (self.rel == "engine/interp.py") == (
            name == "push_frame"
        ):
            self._push_frame(node)
        elif name == "_decide" and self.rel == "engine/interp.py":
            self._decision(node, prompt_index=5, context_index=6)
        elif name == "Decision":
            self._decision(node, prompt_index=3, context_index=4)
        self.generic_visit(node)

    def _record(self, node: ast.Call) -> None:
        kind = _str_const(_arg(node, 1, "kind"))
        if kind is None:
            self._fail(node, "the record kind")
        else:
            self.scan.add("HistKind", kind, self._site(node))

    def _emit(self, node: ast.Call) -> None:
        for kw in node.keywords:
            if kw.arg is None:
                self._fail(node, "a ** argument of emit")
            elif kw.arg not in {"player", "lki"}:
                self.scan.add("EvKey", kw.arg, self._site(node))

    def _push_frame(self, node: ast.Call) -> None:
        kind = _arg(node, 99, "kind")
        lit = _str_const(kind)
        if lit is not None:
            self.scan.add("FrameKind", lit, self._site(node))
            return
        if isinstance(kind, ast.Name) and self.functions:
            found = False
            for sub in ast.walk(self.functions[-1]):
                if isinstance(sub, ast.Assign) and any(
                    isinstance(t, ast.Name) and t.id == kind.id for t in sub.targets
                ):
                    for c in ast.walk(sub.value):
                        s = _str_const(c) if isinstance(c, ast.expr) else None
                        if s is not None:
                            self.scan.add("FrameKind", s, self._site(sub))
                            found = True
            if found:
                return
        self._fail(node, "the frame kind")

    def _decision(self, node: ast.Call, prompt_index: int, context_index: int) -> None:
        prompt = _arg(node, prompt_index, "prompt")
        lit = _str_const(prompt)
        if lit is not None:
            self.scan.add("EnginePrompt", lit, self._site(node))
        elif (
            isinstance(prompt, ast.BoolOp)
            and isinstance(prompt.op, ast.Or)
            and len(prompt.values) == 2
            and ast.unparse(prompt.values[0]) == "ins.prompt"
            and _str_const(prompt.values[1]) is not None
        ):
            # AskMay: `ins.prompt or "you may"` (interp.py:216)
            self.scan.ask_prompt_sites.append(self._site(node))
            self.scan.add("EnginePrompt", str(_str_const(prompt.values[1])), self._site(node))
        elif isinstance(prompt, ast.JoinedStr) and ast.unparse(prompt) == "f'choose {ins.var}'":
            # Choose: f"choose {ins.var}" (interp.py:439,467)
            self.scan.choose_prompt_sites.append(self._site(node))
        elif not self._pass_through():
            self._fail(node, "the decision prompt")
        context = _arg(node, context_index, "context")
        if context is None:
            return
        if isinstance(context, ast.Tuple):
            for item in context.elts:
                key = _str_const(item.elts[0]) if isinstance(item, ast.Tuple) else None
                if key is None:
                    self._fail(node, "a context key")
                else:
                    self.scan.add("CtxKey", key, self._site(node))
        elif not self._pass_through():
            self._fail(node, "the decision context")


def scan_package() -> Scan:
    scan = Scan()
    for path in sorted(PKG.rglob("*.py")):
        rel = path.relative_to(PKG).as_posix()
        _Scanner(rel, scan).visit(ast.parse(path.read_text(encoding="utf-8"), filename=rel))
    for key in ("subject", "player", "group"):  # the fixed head of every payload (core.py:330)
        scan.add("EvKey", key, "engine/core.py:330")
    if not scan.ask_prompt_sites or not scan.choose_prompt_sites:
        raise SystemExit("the AskMay or Choose prompt site was not found")
    return scan


# --- rendering ------------------------------------------------------------------------------


def _ident(value: str) -> str:
    parts = [p for p in re.split(r"[^0-9A-Za-z]+", value) if p]
    if not parts:
        raise SystemExit(f"no identifier for {value!r}")
    ident = "".join(p[:1].upper() + p[1:].lower() for p in parts)
    return ident if not ident[0].isdigit() else f"V{ident}"


def _camel(name: str) -> str:
    return "".join(part[:1] + part[1:].lower() for part in name.split("_"))


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


def _src_line(obj: object) -> str:
    path = Path(inspect.getsourcefile(obj) or "")  # type: ignore[arg-type]
    _, line = inspect.getsourcelines(obj)  # type: ignore[arg-type]
    return f"{path.relative_to(PKG).as_posix()}:{line}"


def _const_line(mod: object, name: str) -> str:
    path = Path(inspect.getsourcefile(mod) or "")  # type: ignore[arg-type]
    pattern = re.compile(rf"^{re.escape(name)}\s*[:=]")
    for i, text in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if pattern.match(text):
            return f"{path.relative_to(PKG).as_posix()}:{i}"
    raise SystemExit(f"{name} not found in {path}")


def _rust_literal(value: object, ty: str) -> str:
    if ty == "&str":
        if not isinstance(value, str):
            raise SystemExit(f"{value!r} is not a str")
        return _rs_str(value)
    if not isinstance(value, int) or isinstance(value, bool):
        raise SystemExit(f"{value!r} is not an int")
    if ty in {"usize", "u8"} and value < 0:
        raise SystemExit(f"{value!r} does not fit {ty}")
    return str(value)


def render_int_enum(e: type[enum.IntEnum]) -> list[str]:
    members = list(e)
    variants = [(m, _camel(m.name)) for m in members]
    out = [
        f"/// Python `{e.__name__}` (`{_src_line(e)}`), an `IntEnum`: each discriminant is the",
        "/// member's Python value (serialised with `int(...)`), and the derived `Ord` is value",
        "/// order, which equals declaration order.",
        "#[repr(u8)]",
        "#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]",
        f"pub enum {e.__name__} {{",
    ]
    for m, v in variants:
        out += [f"    /// `{m.name} = {int(m)}`", f"    {v} = {int(m)},"]
    out += ["}", "", f"impl {e.__name__} {{"]
    out.append(f'    pub const PY_CLASS: &\'static str = "{e.__name__}";')
    out.append("    /// Declaration order (Python's iteration order).")
    out.append(
        f"    pub const ALL: [Self; {len(members)}] = ["
        + ", ".join(f"Self::{v}" for _, v in variants)
        + "];"
    )
    out += [
        "",
        "    /// The Python value, as `int(member)` writes it.",
        "    #[must_use]",
        "    pub const fn value(self) -> u8 {",
        "        self as u8",
        "    }",
        "",
        "    /// The member's Python name (`member.name`).",
        "    #[must_use]",
        "    pub const fn py_name(self) -> &'static str {",
        "        match self {",
        *(f"            Self::{v} => {_rs_str(m.name)}," for m, v in variants),
        "        }",
        "    }",
        "",
        "    /// The member whose Python value is `value` (`Zone(value)` and the like).",
        "    #[must_use]",
        "    pub const fn from_value(value: i64) -> Option<Self> {",
        "        match value {",
        *(f"            {int(m)} => Some(Self::{v})," for m, v in variants),
        "            _ => None,",
        "        }",
        "    }",
    ]
    if e is T.Zone:
        out += [
            "",
            "    /// The index of this zone's list in `zones[player]` (`state.py:479`, `:515`).",
            "    #[must_use]",
            "    pub const fn index(self) -> usize {",
            "        self as usize",
            "    }",
            "",
            "    /// `Zone.is_public` (`engine/types.py:24-26`): membership in [`PUBLIC_ZONES`].",
            "    #[must_use]",
            "    pub const fn is_public(self) -> bool {",
            "        matches!(self, "
            + " | ".join(f"Self::{_camel(z.name)}" for z in sorted(T.PUBLIC_ZONES))
            + ")",
            "    }",
            "",
            "    /// `Zone.is_field` (`engine/types.py:28-30`): membership in [`FIELD_ZONES`].",
            "    #[must_use]",
            "    pub const fn is_field(self) -> bool {",
            "        matches!(self, "
            + " | ".join(f"Self::{_camel(z.name)}" for z in sorted(T.FIELD_ZONES))
            + ")",
            "    }",
        ]
    out += ["}", ""]
    return out


def render_str_enum(e: type[enum.StrEnum]) -> list[str]:
    members = list(e)
    variants = [(m, _camel(m.name)) for m in members]
    by_value = sorted(members, key=lambda m: m.value)
    out = [
        f"/// Python `{e.__name__}` (`{_src_line(e)}`), a `StrEnum`: discriminants are the",
        "/// declaration order, which is also the derived `Ord`; sort by value only through",
        "/// `SORTED_BY_VALUE`. Serialised as the value string.",
        "#[repr(u8)]",
        "#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]",
        f"pub enum {e.__name__} {{",
    ]
    for i, (m, v) in enumerate(variants):
        out += [f"    /// `{m.name} = {m.value!r}`", f"    {v} = {i},"]
    out += ["}", "", f"impl {e.__name__} {{"]
    out.append(f'    pub const PY_CLASS: &\'static str = "{e.__name__}";')
    out.append("    /// Declaration order.")
    out.append(
        f"    pub const ALL: [Self; {len(members)}] = ["
        + ", ".join(f"Self::{v}" for _, v in variants)
        + "];"
    )
    out.append("    /// Sorted by value string, as Python's `sorted()` orders a `StrEnum`.")
    out.append(
        f"    pub const SORTED_BY_VALUE: [Self; {len(members)}] = ["
        + ", ".join(f"Self::{_camel(m.name)}" for m in by_value)
        + "];"
    )
    out += [
        "",
        "    /// The member's value string.",
        "    #[must_use]",
        "    pub const fn as_str(self) -> &'static str {",
        "        match self {",
        *(f"            Self::{v} => {_rs_str(m.value)}," for m, v in variants),
        "        }",
        "    }",
        "",
        "    /// The member's Python name (`member.name`).",
        "    #[must_use]",
        "    pub const fn py_name(self) -> &'static str {",
        "        match self {",
        *(f"            Self::{v} => {_rs_str(m.name)}," for m, v in variants),
        "        }",
        "    }",
        "",
        "    /// The member whose value string is `value`.",
        "    #[must_use]",
        "    pub fn from_value(value: &str) -> Option<Self> {",
        "        match value {",
        *(f"            {_rs_str(m.value)} => Some(Self::{v})," for m, v in variants),
        "            _ => None,",
        "        }",
        "    }",
        "}",
        "",
    ]
    return out


def render_scanned(name: str, what: str, default: bool, values: dict[str, list[str]]) -> list[str]:
    ordered = sorted(values)
    idents = [_ident(v) for v in ordered]
    if len(set(idents)) != len(idents):
        raise SystemExit(f"{name}: identifier clash in {idents}")
    derives = "Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug"
    if default:
        derives += ", Default"
    out = [
        f"/// Every literal string the Python package writes as {what}, found by an AST scan",
        "/// of `src/gcg_sim`. Discriminants follow the sorted value strings, so the derived",
        "/// `Ord` is Python's string order. Serialised as the value string.",
        "#[repr(u8)]",
        f"#[derive({derives})]",
        f"pub enum {name} {{",
    ]
    for i, (value, ident) in enumerate(zip(ordered, idents, strict=True)):
        sites = values[value]
        shown = ", ".join(f"`{s}`" for s in sites[:4])
        more = f" and {len(sites) - 4} more" if len(sites) > 4 else ""
        out.append(f"    /// {_rs_str(value)}: {shown}{more}")
        if default and i == 0:
            out.append("    #[default]")
        out.append(f"    {ident} = {i},")
    out += ["}", "", f"impl {name} {{"]
    out.append("    /// Sorted by value string (= declaration order).")
    out.append(
        f"    pub const ALL: [Self; {len(ordered)}] = ["
        + ", ".join(f"Self::{i}" for i in idents)
        + "];"
    )
    out += [
        "",
        "    /// The value string.",
        "    #[must_use]",
        "    pub const fn as_str(self) -> &'static str {",
        "        match self {",
        *(f"            Self::{i} => {_rs_str(v)}," for v, i in zip(ordered, idents, strict=True)),
        "        }",
        "    }",
        "",
        "    /// The member whose value string is `value`.",
        "    #[must_use]",
        "    pub fn from_value(value: &str) -> Option<Self> {",
        "        match value {",
        *(
            f"            {_rs_str(v)} => Some(Self::{i}),"
            for v, i in zip(ordered, idents, strict=True)
        ),
        "            _ => None,",
        "        }",
        "    }",
        "}",
        "",
    ]
    return out


def render_zone_sets() -> list[str]:
    out: list[str] = []
    for rust, mod, attr in ZONE_SETS:
        zones = sorted(getattr(mod, attr))
        out += [
            f"/// `{attr}` (`{_const_line(mod, attr)}`), a frozenset used for membership only;",
            "/// listed in value order.",
            f"pub const {rust}: [Zone; {len(zones)}] = ["
            + ", ".join(f"Zone::{_camel(z.name)}" for z in zones)
            + "];",
            "",
        ]
    for rust, mod, attr in ZONE_TUPLES:
        zones = list(getattr(mod, attr))
        out += [
            f"/// `{attr}` (`{_const_line(mod, attr)}`), in Python's iteration order.",
            f"pub const {rust}: [Zone; {len(zones)}] = ["
            + ", ".join(f"Zone::{_camel(z.name)}" for z in zones)
            + "];",
            "",
        ]
    return out


def render_constants() -> list[str]:
    out: list[str] = []
    for rust, mod, attr, ty, cfg in CONSTANTS:
        value = getattr(mod, attr)
        if cfg is not None:
            out.append(f'#[cfg(feature = "{cfg}")]')
        out.append(f"/// `{attr} = {value!r}` (`{_const_line(mod, attr)}`).")
        out.append(f"pub const {rust}: {ty} = {_rust_literal(value, ty)};")
        out.append("")
    return out


def render_json_keys() -> list[str]:
    """The dict key orders of the `to_json` methods, read from Python objects."""
    tables = (
        (
            "STATE_JSON_KEYS",
            "GameState.to_json",
            state.GameState,
            list(state.GameState(0).to_json()),
        ),
        (
            "FRAME_JSON_KEYS",
            "Frame.to_json",
            state.Frame,
            list(state.Frame(0, 0, 0, 0, 0, "system").to_json()),
        ),
        (
            "DECISION_JSON_KEYS",
            "Decision.to_json",
            state.Decision,
            list(state.Decision(0, T.DecisionKind.MAIN, ()).to_json()),
        ),
    )
    out: list[str] = []
    for rust, what, cls, keys in tables:
        method = cls.to_json
        out += [
            f"/// The keys of `{what}()` in insertion order (`{_src_line(method)}`), read from a",
            "/// Python object; `canon::canonical_state` writes exactly this order.",
            f"pub const {rust}: [&str; {len(keys)}] = ["
            + ", ".join(_rs_str(k) for k in keys)
            + "];",
            "",
        ]
    return out


def render_tests(scan: Scan) -> list[str]:
    int_checks = []
    for e in INT_ENUMS:
        int_checks += [
            f"        for z in {e.__name__}::ALL {{",
            f"            assert_eq!({e.__name__}::from_value(i64::from(z.value())), Some(z));",
            "        }",
            f"        assert_eq!({e.__name__}::ALL.len(), {len(e)});",
        ]
    str_checks = []
    for se in STR_ENUMS:
        str_checks += [
            f"        for k in {se.__name__}::ALL {{",
            f"            assert_eq!({se.__name__}::from_value(k.as_str()), Some(k));",
            "        }",
            f"        assert!({se.__name__}::SORTED_BY_VALUE.windows(2).all(|w| w[0].as_str() < w[1].as_str()));",
        ]
    for name, _, _ in SCANNED:
        str_checks += [
            f"        for k in {name}::ALL {{",
            f"            assert_eq!({name}::from_value(k.as_str()), Some(k));",
            "        }",
            f"        assert!({name}::ALL.windows(2).all(|w| w[0] < w[1] && w[0].as_str() < w[1].as_str()));",
            f"        assert_eq!({name}::ALL.len(), {len(scan.values[name])});",
        ]
    return [
        "#[cfg(test)]",
        "mod tests {",
        "    use super::*;",
        "",
        "    #[test]",
        "    fn int_enums_round_trip_their_python_values() {",
        *int_checks,
        "        assert_eq!(Zone::ALL.len(), N_ZONES);",
        "    }",
        "",
        "    #[test]",
        "    fn str_enums_round_trip_their_value_strings() {",
        *str_checks,
        "    }",
        "}",
        "",
    ]


def _sha256(p: Path) -> str:
    return hashlib.sha256(p.read_bytes()).hexdigest()


def render(scan: Scan) -> str:
    sources = sorted(
        {
            PKG / "engine" / "types.py",
            PKG / "engine" / "state.py",
            PKG / "engine" / "core.py",
            PKG / "engine" / "game.py",
            PKG / "engine" / "interp.py",
            PKG / "engine" / "observe.py",
            PKG / "engine" / "invariants.py",
            PKG / "testkit.py",
            *(
                PKG / site.split(":")[0]
                for sites in scan.values.values()
                for site_list in sites.values()
                for site in site_list
            ),
        }
    )
    head = [
        "//! Engine enums and constants, generated by `port/oracle/gen_engine_enums.py` from the",
        "//! Python reference; do not edit by hand (`uv run python port/oracle/gen_engine_enums.py",
        "//! --write` regenerates it, `--check` fails when it is stale). Lead-owned contract code",
        "//! (docs/port/PLAN.md §4.2, §10.2).",
        "//!",
        "//! Contents: the `IntEnum`s `Zone`, `Phase`, `Step` (discriminant = Python value), the",
        "//! `StrEnum`s `DecisionKind`, `ActionKind`, `EndReason` (declaration order, value",
        "//! strings, member names), the zone sets, the module constants, and the string enums",
        "//! found by scanning the package's call sites: `HistKind` (`record`), `EvKey` (`emit`),",
        "//! `FrameKind` (`push_frame`), `EnginePrompt` and `CtxKey` (`Decision`, `_decide`), and",
        "//! the key orders of `GameState`, `Frame` and `Decision` `to_json`. The",
        '//! AskMay prompt `ins.prompt or "you may"` and the Choose prompt `f"choose {ins.var}"`',
        "//! are not literals; `contracts::Prompt` carries them.",
        "//!",
        f"//! AskMay prompt sites: {', '.join(f'`{s}`' for s in scan.ask_prompt_sites)}.",
        f"//! Choose prompt sites: {', '.join(f'`{s}`' for s in scan.choose_prompt_sites)}.",
        "//!",
        "//! Sources (SHA-256):",
        *(f"//! - `{p.relative_to(ROOT).as_posix()}` `{_sha256(p)}`" for p in sources),
        "",
    ]
    body: list[str] = []
    for ie in INT_ENUMS:
        body += render_int_enum(ie)
    for se in STR_ENUMS:
        body += render_str_enum(se)
    for name, what, default in SCANNED:
        body += render_scanned(name, what, default, scan.values[name])
    body += render_zone_sets()
    body += render_constants()
    body += render_json_keys()
    body += render_tests(scan)
    return "\n".join([*head, *body]) + "\n"


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
    ap = argparse.ArgumentParser(description=(__doc__ or "").splitlines()[0])
    mode = ap.add_mutually_exclusive_group(required=True)
    mode.add_argument("--write", action="store_true", help=f"write {OUT.relative_to(ROOT)}")
    mode.add_argument("--check", action="store_true", help="exit 1 if the file is stale")
    args = ap.parse_args()
    text = _rustfmt(render(scan_package()))
    rel = OUT.relative_to(ROOT)
    if args.check:
        if not OUT.exists() or OUT.read_text(encoding="utf-8") != text:
            print(f"{rel} is stale; run port/oracle/gen_engine_enums.py --write", file=sys.stderr)
            return 1
        print(f"{rel} is current")
        return 0
    OUT.write_text(text, encoding="utf-8")
    print(f"wrote {rel}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
