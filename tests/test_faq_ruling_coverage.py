"""Criterion 4b: every rules-FAQ entry and card ruling is a tagged test or N/A with a reason."""

from __future__ import annotations

import json
from pathlib import Path

from gcg_sim.cards.db import read_data_text
from gcg_sim.tools.marks import scan, values

TESTS = Path(__file__).resolve().parent
META = TESTS / "meta"


def _na(path: Path) -> dict[str, str]:
    out: dict[str, str] = {}
    files = sorted(path.glob("*.json")) if path.is_dir() else ([path] if path.exists() else [])
    for f in files:
        out.update(json.loads(f.read_text(encoding="utf-8")))
    return out


def test_every_faq_entry_is_tested_or_na() -> None:
    faq = json.loads(read_data_text("gcgapi", "rules-faq.json"))
    tagged = values(scan(TESTS, ("faq",)), "faq")
    na = _na(META / "faq_na.json")
    assert all(reason.strip() for reason in na.values())
    missing = [e["num"] for e in faq if e["num"] not in tagged and e["num"] not in na]
    assert not missing, f"{len(missing)} FAQ entries lack a test or N/A reason: {missing[:40]}"


def test_every_card_ruling_is_tested_or_na() -> None:
    rulings = json.loads(read_data_text("gcgapi", "rulings.json"))
    tagged = values(scan(TESTS, ("ruling",)), "ruling")
    na = _na(META / "rulings_na")
    assert all(reason.strip() for reason in na.values())
    ids = [f"{r['card_number']}:{r['num']}" for r in rulings]
    missing = [i for i in ids if i not in tagged and i not in na]
    assert not missing, f"{len(missing)} rulings lack a test or N/A reason: {missing[:40]}"
