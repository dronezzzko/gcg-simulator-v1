"""Playing a (determinized) state forward with the heuristic policy."""

from __future__ import annotations

from gcg_sim.ai import policy
from gcg_sim.engine.game import apply
from gcg_sim.engine.state import GameState
from gcg_sim.rng import SplitMix64


def play_until_turn(
    st: GameState,
    turn: int,
    rng: SplitMix64 | None,
    temperature: float,
    max_decisions: int,
) -> int:
    """Apply policy moves in place until the game ends, ``st.turn`` reaches ``turn``, or
    ``max_decisions`` moves were made. Returns the number of moves applied."""
    n = 0
    while st.winner is None and st.turn < turn and n < max_decisions and st.pending is not None:
        apply(st, policy.choose(st, rng, temperature), check=False)
        n += 1
    return n


def play_to_end(
    st: GameState, rng: SplitMix64 | None, temperature: float, max_decisions: int
) -> int:
    """Apply policy moves until the game ends (or ``max_decisions``)."""
    n = 0
    while st.winner is None and n < max_decisions and st.pending is not None:
        apply(st, policy.choose(st, rng, temperature), check=False)
        n += 1
    return n
