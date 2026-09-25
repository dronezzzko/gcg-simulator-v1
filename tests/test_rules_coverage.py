"""Criterion 4a: every numbered rule relevant to 1v1 maps to a rule-tagged test or an N/A reason."""

from __future__ import annotations

from pathlib import Path

from gcg_sim.rules.index import load_rules_index
from gcg_sim.tools.marks import scan, values

TESTS = Path(__file__).resolve().parent


def test_every_testable_rule_has_a_tagged_test() -> None:
    ri = load_rules_index()
    tagged = values(scan(TESTS, ("rule",)), "rule")
    missing = [rid for rid in ri.testable_ids() if rid not in tagged]
    assert not missing, (
        f"{len(missing)} testable rules lack a @pytest.mark.rule test: {missing[:40]}"
    )


def test_na_rules_have_reasons() -> None:
    ri = load_rules_index()
    assert all(ri.na_reason(rid) for rid in ri.na_ids())


def test_rule_tags_name_real_rules() -> None:
    ri = load_rules_index()
    unknown = sorted(v for v in values(scan(TESTS, ("rule",)), "rule") if ri.get(v) is None)
    assert not unknown, unknown
