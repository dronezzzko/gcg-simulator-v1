"""Criterion 6: ≥10,000 random-agent games with zero crashes and invariant violations; the
generated decks jointly include every card."""

from __future__ import annotations

import os

import pytest

from gcg_sim.tools.fuzz import main_deck_pool, resource_pool, run

GAMES = 10_000


@pytest.mark.slow
@pytest.mark.rule("1-2-1", "1-2-2-1", "11-1-2", "4-5-4", "4-6-3", "4-4-2")
def test_ten_thousand_random_games_hold_invariants() -> None:
    workers = max(1, (os.cpu_count() or 2) - 1)
    res = run(GAMES, seed=2026, workers=workers)
    assert res.games == GAMES
    assert not res.failures, res.failures[:3]
    assert set(res.ends) <= {"battle_damage", "deck_out", "both_defeated"}
    pool = {n for ns in main_deck_pool().values() for n in ns} | set(resource_pool())
    assert pool <= res.cards_seen, sorted(pool - res.cards_seen)[:20]


def test_small_fuzz_smoke() -> None:
    res = run(40, seed=5, workers=1)
    assert not res.failures, res.failures[:1]
