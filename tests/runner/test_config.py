"""Configuration checks, seat assignment, and seed derivation."""

from __future__ import annotations

import dataclasses
import re

import pytest
from hypothesis import given
from hypothesis import strategies as st

from gcg_sim.deck import Deck, to_decklist
from gcg_sim.rng import derive_seed
from gcg_sim.runner import BenchmarkConfig, agent_seed, dut_seat, game_seed
from gcg_sim.runner.config import own_turn, seat_decks

U64 = st.integers(0, 2**64 - 1)


def test_game_and_agent_seeds_follow_the_documented_derivation() -> None:
    assert game_seed(7, 3, 1) == derive_seed(7, "match", 3, "game", 1)
    assert agent_seed(7, 3, 1, 0) == derive_seed(7, "match", 3, "game", 1, "agent", 0)
    assert agent_seed(7, 3, 1, 0) != agent_seed(7, 3, 1, 1)


@given(master=U64, m=st.integers(0, 10_000), g=st.integers(0, 2))
def test_seeds_are_64_bit_and_depend_on_every_index(master: int, m: int, g: int) -> None:
    s = game_seed(master, m, g)
    assert 0 <= s < 2**64
    assert s == game_seed(master, m, g)
    assert s != game_seed(master, m + 1, g)
    assert s != game_seed(master, m, (g + 1) % 3)
    assert s != game_seed((master + 1) % 2**64, m, g)


def test_seats_alternate_between_matches(config: BenchmarkConfig, federation: Deck) -> None:
    assert [dut_seat(m) for m in range(4)] == [0, 1, 0, 1]
    assert seat_decks(config, 0)[0] == to_decklist(federation)
    assert seat_decks(config, 1)[1] == to_decklist(federation)


def test_own_turn_counts_turns_started_by_a_seat() -> None:
    assert [own_turn(t, 0, first_player=0) for t in range(1, 7)] == [1, 1, 2, 2, 3, 3]
    assert [own_turn(t, 1, first_player=0) for t in range(1, 7)] == [0, 1, 1, 2, 2, 3]


def test_format_rules(config: BenchmarkConfig) -> None:
    assert (config.games_per_match, config.wins_needed) == (3, 2)
    bo1 = dataclasses.replace(config, fmt="bo1")
    assert (bo1.games_per_match, bo1.wins_needed) == (1, 1)


@pytest.mark.parametrize(
    ("change", "message"),
    [
        ({"matches": 0}, "matches must be at least 1 (got 0)"),
        ({"workers": 0}, "workers must be at least 1 (got 0)"),
        ({"fmt": "bo5"}, "format must be one of bo3, bo1 (got 'bo5')"),
        ({"ai_preset": "max"}, "AI preset must be one of standard, strong (got 'max')"),
        ({"replays": -1}, "replays must be 0 or more (got -1)"),
        ({"seed": -1}, "seed must be in [0, 2**64) (got -1)"),
        ({"turn_limit": 0}, "turn limit must be at least 1 (got 0)"),
    ],
)
def test_invalid_config_is_rejected_with_a_specific_message(
    config: BenchmarkConfig, change: dict[str, object], message: str
) -> None:
    with pytest.raises(ValueError, match="^" + re.escape(message)):
        dataclasses.replace(config, **change)  # type: ignore[arg-type]
