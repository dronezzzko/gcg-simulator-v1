"""Golden tests: every card's compiled behaviour is frozen with its normalized-text hash."""

from __future__ import annotations

from pathlib import Path

import pytest

from gcg_sim.tools.golden import build, dump

GOLDEN_DIR = Path(__file__).parent / "golden"
DATA = build()


@pytest.mark.parametrize("prefix", sorted(DATA))
def test_golden_matches(prefix: str) -> None:
    path = GOLDEN_DIR / f"{prefix}.json"
    assert path.exists(), (
        f"missing golden file {path}; run: uv run python -m gcg_sim.tools.golden --write"
    )
    expected = path.read_text(encoding="utf-8")
    actual = dump(DATA[prefix])
    if actual != expected:
        import json

        exp = json.loads(expected)
        act = json.loads(actual)
        changed = sorted(k for k in set(exp) | set(act) if exp.get(k) != act.get(k))
        pytest.fail(
            f"compiled behaviour changed for {len(changed)} card(s): {changed[:20]}; review them, "
            "then run: uv run python -m gcg_sim.tools.golden --write"
        )
