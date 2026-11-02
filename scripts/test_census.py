"""Which card programs each card-tagged Python test executes (docs/port/WEAKNESSES.md W-111).

Runs the Python rule and card tests (``tests/rules``, ``tests/cards``) in one process with the
executed-program census of ``port/oracle/census.py`` installed (a card's program "ran" when a
frame of one of its programs was pushed, one of its triggers resolved, or one of its constant
abilities applied in a view). For every test with ``@pytest.mark.card`` tags it records the
tagged numbers and those whose program ran during the test (setup, call and teardown).

The Rust replay of a test's per-test trace reproduces every state digest the Python test
recorded, so the programs that ran are the same; ``gcg-diff coverage --check`` therefore counts
a tagged trace as execution evidence for a card only when this record shows the card's program
ran in that test, and lists every tagged test that never executes its card.

A test that overrides a card's script (``testkit.card_text``) adds programs to the registry;
the census then recomputes which programs belong to which card before it gives up on a frame.
``cards_without_programs`` lists the non-vanilla real cards with no program at all (their
abilities are keywords such as <Blocker> that the engine reads directly), which no census can
see run. ``effect_only_instantiated`` maps each Unit token, EX Base and EX Resource number that
a test instantiates (by an effect, the setup or the scenario) to the first such test (W-117).

    uv run python scripts/test_census.py --out docs/port/eval/P6-N/test-execution.json
    uv run python scripts/test_census.py --check docs/port/eval/P6-N/test-execution.json
"""

from __future__ import annotations

import argparse
import json
import sys
from collections.abc import Iterator
from pathlib import Path
from typing import Any

import pytest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "port" / "oracle"))

import census as census_mod  # noqa: E402  (port/oracle, after the path insert)

from gcg_sim.cards.model import CardType  # noqa: E402
from gcg_sim.effects.registry import get_registry  # noqa: E402
from gcg_sim.engine import core  # noqa: E402

FORMAT = 1
TEST_DIRS = ("tests/rules", "tests/cards")
EFFECT_ONLY = frozenset({CardType.UNIT_TOKEN, CardType.EX_BASE, CardType.EX_RESOURCE})


class RefreshingCensus(census_mod.Census):
    """The census, refreshing its program-to-card map when the registry gained programs."""

    def __init__(self) -> None:
        super().__init__(get_registry())
        self._n_programs = len(self.reg.programs)

    def credit(self, def_id: int, pid: int) -> bool:
        if super().credit(def_id, pid):
            return True
        if len(self.reg.programs) != self._n_programs:
            self._n_programs = len(self.reg.programs)
            self.programs_of = self._programs_of_cards()
            return super().credit(def_id, pid)
        return False


class Plugin:
    """Records, per card-tagged test, the tagged numbers whose programs ran, and the first test
    that instantiates each effect-only number (Unit token, EX Base, EX Resource)."""

    def __init__(self) -> None:
        self.census = RefreshingCensus()
        self.restore_census = self.census.install()
        self.rows: dict[str, dict[str, Any]] = {}
        self.failed: list[str] = []
        self.created: dict[str, str] = {}
        self.current = ""
        new_card = core.new_card

        def w_new_card(st: Any, def_id: int, *args: Any, **kwargs: Any) -> int:
            cd = self.census.reg.db.by_id(def_id)
            if cd.card_type in EFFECT_ONLY:
                self.created.setdefault(self.census.number(def_id), self.current)
            uid: int = new_card(st, def_id, *args, **kwargs)
            return uid

        core.new_card = w_new_card
        self._new_card = new_card

    def restore(self) -> None:
        core.new_card = self._new_card
        self.restore_census()

    @pytest.hookimpl(hookwrapper=True)
    def pytest_runtest_protocol(self, item: pytest.Item, nextitem: object) -> Iterator[None]:
        self.census.ran.clear()
        self.current = item.nodeid
        yield
        tags = sorted({str(n) for m in item.iter_markers("card") for n in m.args})
        if tags:
            ran = self.census.ran
            self.rows[item.nodeid] = {
                "tagged": tags,
                "executed": [n for n in tags if n in ran],
            }

    def pytest_runtest_logreport(self, report: pytest.TestReport) -> None:
        if report.failed:
            self.failed.append(report.nodeid)


def run(pytest_args: list[str]) -> dict[str, Any]:
    plugin = Plugin()
    try:
        code = pytest.main(
            [*TEST_DIRS, "-q", "-p", "no:cacheprovider", "-p", "no:randomly", *pytest_args],
            plugins=[plugin],
        )
    finally:
        plugin.restore()
    if code != 0 or plugin.failed:
        raise SystemExit(f"pytest exited {code}; failed: {plugin.failed[:10]}")
    rows = dict(sorted(plugin.rows.items()))
    never = {k: v for k, v in rows.items() if len(v["executed"]) < len(v["tagged"])}
    reg = get_registry()
    fresh = census_mod.Census(reg)
    programless = sorted(
        c.card_number
        for c in reg.db.real_cards()
        if (reg.cards[c.def_id].own or reg.cards[c.def_id].unit)
        and not fresh.programs_of.get(c.def_id)
    )
    return {
        "format": FORMAT,
        "generator": "scripts/test_census.py",
        "test_dirs": list(TEST_DIRS),
        "tagged_tests": len(rows),
        "tests_missing_a_tagged_card": len(never),
        "cards_without_programs": programless,
        "effect_only_instantiated": dict(sorted(plugin.created.items())),
        "tests": rows,
    }


def dumps(doc: dict[str, Any]) -> str:
    """One test per line, so a diff of the committed file is readable."""
    lines = ["{"]
    for k, v in doc.items():
        if k != "tests":
            lines.append(f"  {json.dumps(k)}: {json.dumps(v)},")
    lines.append('  "tests": {')
    items = list(doc["tests"].items())
    for i, (k, v) in enumerate(items):
        lines.append(f"    {json.dumps(k)}: {json.dumps(v)}{',' if i + 1 < len(items) else ''}")
    lines.append("  }")
    lines.append("}")
    return "\n".join(lines) + "\n"


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=(__doc__ or "").splitlines()[0])
    out = ap.add_mutually_exclusive_group(required=True)
    out.add_argument("--out", type=Path, help="write the record here")
    out.add_argument("--check", type=Path, help="fail unless the record equals this file")
    args, rest = ap.parse_known_args(argv)
    text = dumps(run(rest))
    if args.out:
        args.out.parent.mkdir(parents=True, exist_ok=True)
        args.out.write_text(text, encoding="utf-8")
        print(f"wrote {args.out}")
        return 0
    if args.check.read_text(encoding="utf-8") != text:
        print(f"{args.check} is stale: regenerate it with --out", file=sys.stderr)
        return 1
    print(f"{args.check} is current")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
