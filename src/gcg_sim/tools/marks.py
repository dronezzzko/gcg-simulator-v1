"""Static scan of test files for ``pytest.mark.<name>(...)`` tags (rule, card, faq, ruling).

Coverage gates and the rules traceability report are computed from the tags in the source,
so they do not depend on test execution order or on which subset of tests is collected.
"""

from __future__ import annotations

import ast
from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True, slots=True)
class Tag:
    mark: str
    value: str
    path: str
    test: str
    line: int


def _mark_name(node: ast.expr) -> str | None:
    # pytest.mark.<name>
    if isinstance(node, ast.Attribute) and isinstance(node.value, ast.Attribute):
        inner = node.value
        if (
            inner.attr == "mark"
            and isinstance(inner.value, ast.Name)
            and inner.value.id == "pytest"
        ):
            return node.attr
    return None


def _values(call: ast.Call) -> list[str]:
    out: list[str] = []
    for arg in call.args:
        if isinstance(arg, ast.Constant) and isinstance(arg.value, str):
            out.append(arg.value)
        elif isinstance(arg, (ast.List, ast.Tuple)):
            out.extend(
                e.value
                for e in arg.elts
                if isinstance(e, ast.Constant) and isinstance(e.value, str)
            )
    return out


class _Visitor(ast.NodeVisitor):
    def __init__(self, path: str, marks: frozenset[str]) -> None:
        self.path = path
        self.marks = marks
        self.tags: list[Tag] = []
        self._scope: list[str] = []

    def _record(self, call: ast.Call, test: str) -> None:
        name = _mark_name(call.func)
        if name in self.marks:
            for v in _values(call):
                self.tags.append(Tag(name, v, self.path, test, call.lineno))

    def _visit_def(self, node: ast.FunctionDef | ast.AsyncFunctionDef | ast.ClassDef) -> None:
        test = "::".join([*self._scope, node.name])
        for dec in node.decorator_list:
            if isinstance(dec, ast.Call):
                self._record(dec, test)
        self._scope.append(node.name)
        for child in ast.walk(node):
            if child is node:
                continue
            if (
                isinstance(child, ast.Call)
                and _mark_name(child.func) in self.marks
                and child not in node.decorator_list
            ):
                if any(child is dec for dec in node.decorator_list):
                    continue
                self._record(child, test)
        self._scope.pop()

    def visit_FunctionDef(self, node: ast.FunctionDef) -> None:
        self._visit_def(node)

    def visit_AsyncFunctionDef(self, node: ast.AsyncFunctionDef) -> None:
        self._visit_def(node)

    def visit_ClassDef(self, node: ast.ClassDef) -> None:
        self._visit_def(node)

    def visit_Assign(self, node: ast.Assign) -> None:
        # module-level ``pytestmark = pytest.mark.card("X")`` applies to every test in the file
        for child in ast.walk(node):
            if isinstance(child, ast.Call):
                self._record(child, "<module>")


def scan(root: Path, marks: tuple[str, ...] = ("rule", "card", "faq", "ruling")) -> list[Tag]:
    tags: list[Tag] = []
    wanted = frozenset(marks)
    for path in sorted(root.rglob("test_*.py")):
        tree = ast.parse(path.read_text(encoding="utf-8"), filename=str(path))
        v = _Visitor(
            str(path.relative_to(root.parent) if root.parent in path.parents else path), wanted
        )
        for node in tree.body:
            v.visit(node)
        tags.extend(v.tags)
    seen: set[tuple[str, str, str, str]] = set()
    out = []
    for t in tags:
        key = (t.mark, t.value, t.path, t.test)
        if key not in seen:
            seen.add(key)
            out.append(t)
    return out


def values(tags: list[Tag], mark: str) -> set[str]:
    return {t.value for t in tags if t.mark == mark}
