"""Criterion 5: src/ contains no TODO/FIXME/NotImplementedError or placeholder bodies."""

from __future__ import annotations

import ast
import re
from pathlib import Path

SRC = Path(__file__).resolve().parents[1] / "src"
BANNED = re.compile(r"\b(TODO|FIXME|XXX|HACK)\b|NotImplementedError")


def test_no_banned_markers() -> None:
    hits = []
    for path in sorted(SRC.rglob("*.py")):
        for i, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
            if BANNED.search(line):
                hits.append(f"{path.relative_to(SRC)}:{i}: {line.strip()}")
    assert not hits, "\n".join(hits)


def _placeholder(body: list[ast.stmt]) -> bool:
    stmts = [
        s
        for s in body
        if not (
            isinstance(s, ast.Expr)
            and isinstance(s.value, ast.Constant)
            and isinstance(s.value.value, str)
        )
    ]
    if len(stmts) != 1:
        return False
    s = stmts[0]
    if isinstance(s, ast.Pass):
        return True
    return (
        isinstance(s, ast.Expr) and isinstance(s.value, ast.Constant) and s.value.value is Ellipsis
    )


def test_no_placeholder_function_bodies() -> None:
    hits = []
    for path in sorted(SRC.rglob("*.py")):
        tree = ast.parse(path.read_text(encoding="utf-8"))
        for node in ast.walk(tree):
            if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
                is_protocol_stub = any(
                    isinstance(d, ast.Name) and d.id in ("overload", "abstractmethod")
                    for d in node.decorator_list
                )
                if _placeholder(node.body) and not is_protocol_stub:
                    hits.append(f"{path.relative_to(SRC)}:{node.lineno}: {node.name}")
    assert not hits, "\n".join(hits)
