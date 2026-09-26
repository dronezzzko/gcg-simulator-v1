"""The final-state digest does not depend on how a process numbered its registry entries."""

from __future__ import annotations

from types import SimpleNamespace
from typing import Any

from gcg_sim.effects.registry import get_registry
from gcg_sim.engine.core import PREVENT_ONCE_TAG
from gcg_sim.engine.game import apply, new_game
from gcg_sim.engine.state import GameState
from gcg_sim.rng import SplitMix64
from gcg_sim.runner import BenchmarkConfig
from gcg_sim.runner.config import seat_decks
from gcg_sim.runner.digest import canonical_state, state_digest


def state_with_lasting_effects(config: BenchmarkConfig) -> GameState:
    for seed in range(200):
        st = new_game(seat_decks(config, 0), seed)
        rng = SplitMix64(seed)
        while st.pending is not None:
            if st.lasting and st.frames:
                return st
            options = st.pending.options
            apply(st, options[rng.randrange(len(options))])
    raise AssertionError("no random game produced a lasting effect during a resolving frame")


def renumbered(st: GameState) -> tuple[GameState, Any]:
    """The same game state under a registry whose tables are stored in reverse order."""
    reg = get_registry()
    fake = SimpleNamespace(
        continuous=list(reversed(reg.continuous)),
        filter_sets=list(reversed(reg.filter_sets)),
        programs=list(reversed(reg.programs)),
    )

    def flip(i: int, table: list[Any]) -> int:
        return len(table) - 1 - i

    s = st.clone()
    for le in (*s.lasting, *s.delayed):
        le.effect_key = flip(le.effect_key, reg.continuous)
        if le.filters_key >= 0:
            le.filters_key = flip(le.filters_key, reg.filter_sets)
    for f in s.frames:
        f.program_id = flip(f.program_id, reg.programs)
    for t in (*s.pending_triggers, *(t for b in s.batches for t in b)):
        t.program_id = flip(t.program_id, reg.programs)
    s.once_used = {
        (k[0], k[1], flip(k[2], reg.continuous), *k[3:]) if k[:2] == (PREVENT_ONCE_TAG, -1) else k
        for k in s.once_used
    }
    return s, fake


def test_digest_ignores_registry_numbering(config: BenchmarkConfig) -> None:
    st = state_with_lasting_effects(config)
    le = st.lasting[0]
    st.once_used.add((PREVENT_ONCE_TAG, -1, le.effect_key, le.source_uid, 0))
    other, fake = renumbered(st)
    assert other.to_json() != st.to_json()
    assert canonical_state(other, fake) == canonical_state(st)
    assert state_digest(other, fake) == state_digest(st)


def test_digest_changes_with_the_game_state(config: BenchmarkConfig) -> None:
    st = new_game(seat_decks(config, 0), 1)
    before = state_digest(st)
    assert st.pending is not None
    apply(st, st.pending.options[0])
    assert state_digest(st) != before
