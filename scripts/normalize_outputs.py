#!/usr/bin/env python3
"""Normalise a gcg-sim report directory for a Python-vs-Rust byte comparison (W-160).

Exactly three implementation fields are masked (docs/port/PLAN.md A6, A7): results.json
`versions.package`, `versions.code_digest` and `ai.factory`, and their renderings in summary.md
(`factory `...`` in the headline, `gcg-sim <package>;` in the Versions section). Each is replaced
by exact text substitution of its value; every other byte of every file is copied unchanged, so
float digits, key order, escaping, indentation and trailing newlines are all compared. results.json
is parsed only to find the values; it is never re-serialised. timing.json is dropped.

    python scripts/normalize_outputs.py SRC_DIR DST_DIR
    python scripts/normalize_outputs.py --selftest [WORK_DIR]

`--selftest` builds Python-shaped fixture report directories and checks the negative controls:
a one-byte change outside the masked values (`1e-5` for `1e-05`, a missing trailing newline, a
changed key order, an escaped non-ASCII character, a changed games.ndjson line) is reported as a
difference, while different masked values and a different timing.json are not. It prints one
line per control and exits 1 if any control fails. With WORK_DIR it keeps the fixtures there.
"""

from __future__ import annotations

import filecmp
import json
import shutil
import sys
import tempfile
from collections.abc import Callable
from dataclasses import dataclass
from pathlib import Path

PLACEHOLDER = '"<implementation>"'
PLACEHOLDER_TEXT = "<implementation>"
MASKED_PATHS = (("versions", "package"), ("versions", "code_digest"), ("ai", "factory"))
DROPPED = frozenset({"timing.json"})


class NormalizeError(Exception):
    """A report file does not have the shape the normaliser masks."""


@dataclass(frozen=True)
class Span:
    start: int
    end: int


class _Scanner:
    """A JSON scanner that records the source span of the values at the masked key paths.

    It accepts exactly RFC 8259 JSON (what `json.loads` accepts, minus NaN/Infinity) and keeps
    the raw text untouched; only the spans are reported."""

    def __init__(self, text: str) -> None:
        self.text = text
        self.pos = 0
        self.spans: dict[tuple[str, ...], list[Span]] = {p: [] for p in MASKED_PATHS}

    def fail(self, what: str) -> NormalizeError:
        return NormalizeError(f"results.json: {what} at offset {self.pos}")

    def ws(self) -> None:
        t, n = self.text, len(self.text)
        while self.pos < n and t[self.pos] in " \t\r\n":
            self.pos += 1

    def value(self, path: tuple[str, ...]) -> None:
        self.ws()
        if self.pos >= len(self.text):
            raise self.fail("unexpected end")
        start = self.pos
        c = self.text[self.pos]
        if c == "{":
            self.obj(path)
        elif c == "[":
            self.arr()
        elif c == '"':
            self.string()
        else:
            self.scalar()
        if path in self.spans:
            self.spans[path].append(Span(start, self.pos))

    def obj(self, path: tuple[str, ...]) -> None:
        self.pos += 1
        self.ws()
        if self.text.startswith("}", self.pos):
            self.pos += 1
            return
        while True:
            self.ws()
            if not self.text.startswith('"', self.pos):
                raise self.fail("expected a key")
            key_start = self.pos
            self.string()
            key = json.loads(self.text[key_start : self.pos])
            self.ws()
            if not self.text.startswith(":", self.pos):
                raise self.fail("expected ':'")
            self.pos += 1
            self.value((*path, key) if len(path) < 2 else ("",))
            self.ws()
            if self.text.startswith(",", self.pos):
                self.pos += 1
                continue
            if self.text.startswith("}", self.pos):
                self.pos += 1
                return
            raise self.fail("expected ',' or '}'")

    def arr(self) -> None:
        self.pos += 1
        self.ws()
        if self.text.startswith("]", self.pos):
            self.pos += 1
            return
        while True:
            self.value(("",))
            self.ws()
            if self.text.startswith(",", self.pos):
                self.pos += 1
                continue
            if self.text.startswith("]", self.pos):
                self.pos += 1
                return
            raise self.fail("expected ',' or ']'")

    def string(self) -> None:
        t, n = self.text, len(self.text)
        self.pos += 1
        while self.pos < n:
            c = t[self.pos]
            if c == "\\":
                self.pos += 2
                continue
            self.pos += 1
            if c == '"':
                return
            if c < " ":
                raise self.fail("control character in string")
        raise self.fail("unterminated string")

    def scalar(self) -> None:
        # Numbers and literals are only skipped; `masked_spans` validates the whole document.
        t, n = self.text, len(self.text)
        start = self.pos
        while self.pos < n and t[self.pos] not in " \t\r\n,]}":
            self.pos += 1
        if self.pos == start:
            raise self.fail("expected a value")


def _reject_constant(name: str) -> object:
    raise NormalizeError(f"results.json: {name} is not JSON")


def masked_spans(text: str) -> dict[tuple[str, ...], Span]:
    """The span of each masked value; each path must occur exactly once at the top levels."""
    try:
        json.loads(text, parse_constant=_reject_constant)
    except json.JSONDecodeError as exc:
        raise NormalizeError(f"results.json: {exc}") from None
    sc = _Scanner(text)
    sc.value(())
    sc.ws()
    if sc.pos != len(text):
        raise sc.fail("trailing data")
    out: dict[tuple[str, ...], Span] = {}
    for path in MASKED_PATHS:
        found = sc.spans[path]
        if len(found) != 1:
            raise NormalizeError(f"results.json: {'.'.join(path)} occurs {len(found)} times")
        out[path] = found[0]
    return out


def masked_values(text: str) -> dict[tuple[str, ...], str]:
    """The decoded string values of the masked fields."""
    out: dict[tuple[str, ...], str] = {}
    for path, span in masked_spans(text).items():
        value = json.loads(text[span.start : span.end])
        if not isinstance(value, str):
            raise NormalizeError(f"results.json: {'.'.join(path)} is not a string")
        out[path] = value
    return out


def normalize_results(text: str) -> str:
    """Replace the three masked values by the placeholder; every other character is kept."""
    spans = sorted(masked_spans(text).values(), key=lambda s: s.start)
    parts: list[str] = []
    pos = 0
    for span in spans:
        parts.append(text[pos : span.start])
        parts.append(PLACEHOLDER)
        pos = span.end
    parts.append(text[pos:])
    return "".join(parts)


def _replace_once(text: str, old: str, new: str, what: str) -> str:
    count = text.count(old)
    if count != 1:
        raise NormalizeError(f"summary.md: {what} {old!r} occurs {count} times")
    return text.replace(old, new)


def normalize_summary(text: str, values: dict[tuple[str, ...], str]) -> str:
    """Mask the summary renderings of `ai.factory` and `versions.package`
    (`reports/summary.py:51`, `:206`) by their exact values from the same run's results.json."""
    factory = values[("ai", "factory")]
    package = values[("versions", "package")]
    text = _replace_once(
        text, f"factory `{factory}`).", f"factory `{PLACEHOLDER_TEXT}`).", "factory rendering"
    )
    return _replace_once(
        text,
        f"\ngcg-sim {package}; results schema ",
        f"\ngcg-sim {PLACEHOLDER_TEXT}; results schema ",
        "package rendering",
    )


def normalize_dir(src: Path, dst: Path) -> None:
    if not (src / "results.json").is_file():
        raise NormalizeError(f"{src}: no results.json")
    if dst.exists():
        shutil.rmtree(dst)
    dst.mkdir(parents=True)
    results_text = (src / "results.json").read_bytes().decode("utf-8")
    values = masked_values(results_text)
    for path in sorted(src.rglob("*")):
        rel = path.relative_to(src).as_posix()
        if path.is_dir() or rel in DROPPED:
            continue
        out = dst / rel
        out.parent.mkdir(parents=True, exist_ok=True)
        if rel == "results.json":
            out.write_bytes(normalize_results(results_text).encode("utf-8"))
        elif rel == "summary.md":
            text = path.read_bytes().decode("utf-8")
            out.write_bytes(normalize_summary(text, values).encode("utf-8"))
        else:
            shutil.copyfile(path, out)


def same_tree(a: Path, b: Path) -> bool:
    """Byte equality of two normalised trees (same relative file set, same bytes)."""

    def files(root: Path) -> list[str]:
        return sorted(p.relative_to(root).as_posix() for p in root.rglob("*") if p.is_file())

    fa, fb = files(a), files(b)
    if fa != fb:
        return False
    return all(filecmp.cmp(a / f, b / f, shallow=False) for f in fa)


# ---------------------------------------------------------------------------------------------
# self-test (negative controls)


def _fixture_results(package: str, digest: str, factory: str) -> dict[str, object]:
    return {
        "schema_version": "1.1.0",
        "versions": {"package": package, "code_digest": digest, "results_schema": "1.1.0"},
        "ai": {"preset": "standard", "factory": factory, "dut_agent": "mcts-standard"},
        "decks": {"deck_under_test": {"name": "Zeon ü"}, "benchmark": {"name": "Federation"}},
        "results": {"win_rate": 0.55, "tiny": 1e-05, "ratio": 0.1, "big": 1e16, "n": 7},
    }


def _fixture_summary(package: str, factory: str) -> str:
    return (
        "# Zeon ü vs Federation\n\n"
        f"AI preset `standard` (agents `mcts-standard` / `mcts-standard`, factory `{factory}`).\n\n"
        "## Versions\n\n"
        f"gcg-sim {package}; results schema 1.1.0; card data x (built y).\n"
    )


def _python_text(obj: object) -> str:
    # reports/writer.py:33-34
    return json.dumps(obj, indent=2, sort_keys=True, ensure_ascii=False) + "\n"


def _write_fixture(root: Path, package: str, digest: str, factory: str) -> None:
    root.mkdir(parents=True)
    (root / "results.json").write_bytes(
        _python_text(_fixture_results(package, digest, factory)).encode("utf-8")
    )
    (root / "summary.md").write_bytes(_fixture_summary(package, factory).encode("utf-8"))
    (root / "games.ndjson").write_bytes(b'{"g":1,"v":1e-05}\n{"g":2,"v":0.5}\n')
    (root / "replays").mkdir()
    (root / "replays" / "win-m0000-g1.json").write_bytes(b'{\n  "format": "gcg-sim-replay"\n}\n')
    (root / "timing.json").write_bytes(b'{"wall_seconds": 1.0}\n')


def _edit(path: Path, fn: Callable[[str], str]) -> None:
    text = path.read_bytes().decode("utf-8")
    new = fn(text)
    if new == text:
        raise NormalizeError(f"self-test edit did not change {path.name}")
    path.write_bytes(new.encode("utf-8"))


def _reorder_keys(text: str) -> str:
    # Swap the first two keys of "results" ("big" and "n" are sorted neighbours in Python's text).
    a = '    "big": 1e+16,\n'
    b = '    "n": 7,\n'
    if a not in text or b not in text:
        raise NormalizeError("self-test fixture lost its key layout")
    return text.replace(a, "\0").replace(b, a).replace("\0", b)


CONTROLS: tuple[tuple[str, str, bool, Callable[[Path], object]], ...] = (
    ("identical outputs, other implementation values", "masked", True, lambda d: None),
    (
        "timing.json differs",
        "ignored",
        True,
        lambda d: (d / "timing.json").write_bytes(b'{"wall_seconds": 2.5}\n'),
    ),
    (
        "float 1e-5 instead of Python's 1e-05",
        "negative",
        False,
        lambda d: _edit(d / "results.json", lambda t: t.replace("1e-05", "1e-5")),
    ),
    (
        "results.json without its trailing newline",
        "negative",
        False,
        lambda d: _edit(d / "results.json", lambda t: t.rstrip("\n")),
    ),
    ("changed key order", "negative", False, lambda d: _edit(d / "results.json", _reorder_keys)),
    (
        "non-ASCII written as an escape",
        "negative",
        False,
        lambda d: _edit(d / "results.json", lambda t: t.replace("ü", "\\u00fc")),
    ),
    (
        "float 0.1 written as 0.10000000000000001",
        "negative",
        False,
        lambda d: _edit(d / "results.json", lambda t: t.replace("0.1,", "0.10000000000000001,")),
    ),
    (
        "games.ndjson without its trailing newline",
        "negative",
        False,
        lambda d: _edit(d / "games.ndjson", lambda t: t.rstrip("\n")),
    ),
    (
        "replay file with one byte changed",
        "negative",
        False,
        lambda d: _edit(d / "replays" / "win-m0000-g1.json", lambda t: t.replace("  ", "   ", 1)),
    ),
    (
        "summary.md with one byte changed outside the masked values",
        "negative",
        False,
        lambda d: _edit(d / "summary.md", lambda t: t.replace("Federation", "Federatiom", 1)),
    ),
    (
        "summary.md without its trailing newline",
        "negative",
        False,
        lambda d: _edit(d / "summary.md", lambda t: t.rstrip("\n")),
    ),
    (
        "missing replay file",
        "negative",
        False,
        lambda d: (d / "replays" / "win-m0000-g1.json").unlink(),
    ),
)


def selftest(work: Path) -> int:
    """Run every control; return the number of failed controls."""
    py = work / "python"
    _write_fixture(py, "0.1.0", "py-digest", "gcg_sim.ai:make_agent")
    normalize_dir(py, work / "python.norm")
    failed = 0
    for i, (label, kind, expect_same, mutate) in enumerate(CONTROLS):
        rs = work / f"rust-{i:02d}"
        _write_fixture(rs, "0.9.9-rust", "rust-digest-123", "gcg-ai-reference::make_agent('mcts')")
        mutate(rs)
        try:
            normalize_dir(rs, work / f"rust-{i:02d}.norm")
            same = same_tree(work / "python.norm", work / f"rust-{i:02d}.norm")
        except NormalizeError as exc:
            same = False
            label = f"{label} ({exc})"
        ok = same == expect_same
        failed += not ok
        verdict = "IDENTICAL" if same else "DIFFERENT"
        print(f"{'ok  ' if ok else 'FAIL'} {kind:8} {verdict:9} {label}")
    # The masking must be exact: a value that also occurs elsewhere is masked only in place.
    text = _python_text(_fixture_results("1.1.0", "1.1.0", "1.1.0"))
    norm = normalize_results(text)
    exact = text.count("1.1.0") == 5 and norm.count("1.1.0") == 2 and norm.count(PLACEHOLDER) == 3
    print(f"{'ok  ' if exact else 'FAIL'} masked   exact     masked values that repeat elsewhere")
    failed += not exact
    print(f"normalize_outputs selftest: {len(CONTROLS) + 1 - failed}/{len(CONTROLS) + 1} passed")
    return failed


def main(argv: list[str]) -> int:
    if len(argv) in (2, 3) and argv[1] == "--selftest":
        if len(argv) == 3:
            work = Path(argv[2])
            if work.exists():
                shutil.rmtree(work)
            work.mkdir(parents=True)
            return 1 if selftest(work) else 0
        with tempfile.TemporaryDirectory(prefix="normalize-selftest-") as tmp:
            return 1 if selftest(Path(tmp)) else 0
    if len(argv) != 3:
        print(__doc__, file=sys.stderr)
        return 2
    try:
        normalize_dir(Path(argv[1]), Path(argv[2]))
    except (NormalizeError, UnicodeDecodeError, OSError) as exc:
        print(f"normalize_outputs: {exc}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
