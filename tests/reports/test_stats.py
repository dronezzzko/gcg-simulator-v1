"""Wilson intervals and distribution summaries."""

from __future__ import annotations

import pytest
from hypothesis import given
from hypothesis import strategies as st

from gcg_sim.reports import wilson
from gcg_sim.reports.stats import distribution, intervals_overlap, percentile, rate


def test_wilson_known_values() -> None:
    lo, hi = wilson(5, 10)
    assert lo == pytest.approx(0.2365896, abs=1e-6)
    assert hi == pytest.approx(0.7634104, abs=1e-6)
    lo, hi = wilson(0, 20)
    assert lo == 0.0
    assert hi == pytest.approx(0.1611301, abs=1e-6)
    assert wilson(0, 0) == (0.0, 1.0)


@pytest.mark.parametrize(("k", "n"), [(-1, 5), (6, 5), (1, -1)])
def test_wilson_rejects_impossible_counts(k: int, n: int) -> None:
    with pytest.raises(ValueError, match="successes"):
        wilson(k, n)


@given(n=st.integers(1, 5000), data=st.data())
def test_wilson_bounds_contain_the_estimate(n: int, data: st.DataObject) -> None:
    k = data.draw(st.integers(0, n))
    lo, hi = wilson(k, n)
    assert 0.0 <= lo <= k / n <= hi <= 1.0
    mirror_lo, mirror_hi = wilson(n - k, n)
    assert lo == pytest.approx(1 - mirror_hi, abs=1e-12)
    assert hi == pytest.approx(1 - mirror_lo, abs=1e-12)


@given(k=st.integers(1, 50), scale=st.integers(2, 20))
def test_wilson_narrows_with_more_data(k: int, scale: int) -> None:
    lo1, hi1 = wilson(k, 2 * k)
    lo2, hi2 = wilson(k * scale, 2 * k * scale)
    assert hi2 - lo2 < hi1 - lo1


def test_rate_block() -> None:
    assert rate(0, 0) == {"successes": 0, "n": 0, "rate": None, "ci95": None}
    r = rate(3, 4)
    assert r["rate"] == 0.75
    assert r["ci95"][0] < 0.75 < r["ci95"][1]
    assert intervals_overlap(rate(1, 100), rate(99, 100)) is False
    assert intervals_overlap(rate(50, 100), rate(55, 100)) is True
    assert intervals_overlap(rate(0, 0), rate(1, 2)) is True


def test_distribution_and_percentiles() -> None:
    assert percentile([1, 2, 3, 4], 0.5) == 2
    assert percentile([1, 2, 3, 4], 0.9) == 4
    d = distribution([5, 1, 3])
    assert d == {"n": 3, "mean": 3.0, "median": 3, "min": 1, "max": 5, "p10": 1, "p90": 5}
    assert distribution([])["mean"] is None
