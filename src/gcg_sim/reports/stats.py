"""Small, deterministic statistics helpers (Wilson score intervals, rates, distributions)."""

from __future__ import annotations

import math
from collections.abc import Sequence
from typing import Any

DIGITS = 6


def wilson(successes: int, n: int, z: float = 1.96) -> tuple[float, float]:
    """Wilson score interval for a binomial proportion; ``(0.0, 1.0)`` when ``n == 0``."""
    if n < 0 or not 0 <= successes <= max(n, 0):
        raise ValueError(f"need 0 <= successes <= n, got successes={successes}, n={n}")
    if n == 0:
        return (0.0, 1.0)
    p = successes / n
    z2 = z * z
    denom = 1 + z2 / n
    centre = (p + z2 / (2 * n)) / denom
    half = z * math.sqrt(p * (1 - p) / n + z2 / (4 * n * n)) / denom
    lo = 0.0 if successes == 0 else max(0.0, centre - half)
    hi = 1.0 if successes == n else min(1.0, centre + half)
    return (lo, hi)


def rnd(x: float) -> float:
    return round(x, DIGITS)


def ratio(num: float, den: float) -> float | None:
    return rnd(num / den) if den else None


def rate(successes: int, n: int) -> dict[str, Any]:
    """``{"successes", "n", "rate", "ci95"}``; rate and interval are ``None`` when ``n == 0``."""
    if n == 0:
        return {"successes": successes, "n": 0, "rate": None, "ci95": None}
    lo, hi = wilson(successes, n)
    return {"successes": successes, "n": n, "rate": rnd(successes / n), "ci95": [rnd(lo), rnd(hi)]}


def intervals_overlap(a: dict[str, Any], b: dict[str, Any]) -> bool:
    if a["ci95"] is None or b["ci95"] is None:
        return True
    return bool(a["ci95"][0] <= b["ci95"][1] and b["ci95"][0] <= a["ci95"][1])


def percentile(sorted_values: Sequence[int], q: float) -> int:
    """Nearest-rank percentile of already sorted values."""
    rank = max(1, math.ceil(q * len(sorted_values)))
    return sorted_values[rank - 1]


def distribution(values: Sequence[int]) -> dict[str, Any]:
    if not values:
        return {
            "n": 0,
            "mean": None,
            "median": None,
            "min": None,
            "max": None,
            "p10": None,
            "p90": None,
        }
    s = sorted(values)
    return {
        "n": len(s),
        "mean": rnd(sum(s) / len(s)),
        "median": percentile(s, 0.5),
        "min": s[0],
        "max": s[-1],
        "p10": percentile(s, 0.1),
        "p90": percentile(s, 0.9),
    }
