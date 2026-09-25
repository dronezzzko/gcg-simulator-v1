"""Benchmark configuration, seat assignment, and seed derivation."""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path

from gcg_sim.deck.model import Deck
from gcg_sim.deck.parse import to_decklist
from gcg_sim.engine.game import MAX_ACTIONS_DEFAULT, DeckList
from gcg_sim.rng import derive_seed

FORMATS = ("bo3", "bo1")
AI_PRESETS = ("standard", "strong")
DEFAULT_TURN_LIMIT = 200


@dataclass(frozen=True)
class BenchmarkConfig:
    deck_under_test: Deck
    benchmark_deck: Deck
    matches: int
    seed: int = 0
    workers: int = 1
    fmt: str = "bo3"
    ai_preset: str = "standard"
    out_dir: Path = Path("gcg-sim-out")
    decision_log: bool = False
    replays: int = 3
    turn_limit: int = DEFAULT_TURN_LIMIT
    max_actions: int = MAX_ACTIONS_DEFAULT

    def __post_init__(self) -> None:
        problems = config_problems(self)
        if problems:
            raise ValueError("; ".join(problems))

    @property
    def games_per_match(self) -> int:
        return 3 if self.fmt == "bo3" else 1

    @property
    def wins_needed(self) -> int:
        return 2 if self.fmt == "bo3" else 1


def config_problems(c: BenchmarkConfig) -> list[str]:
    checks = [
        (c.matches >= 1, f"matches must be at least 1 (got {c.matches})"),
        (c.workers >= 1, f"workers must be at least 1 (got {c.workers})"),
        (c.fmt in FORMATS, f"format must be one of {', '.join(FORMATS)} (got {c.fmt!r})"),
        (
            c.ai_preset in AI_PRESETS,
            f"AI preset must be one of {', '.join(AI_PRESETS)} (got {c.ai_preset!r})",
        ),
        (c.replays >= 0, f"replays must be 0 or more (got {c.replays})"),
        (c.turn_limit >= 1, f"turn limit must be at least 1 (got {c.turn_limit})"),
        (c.max_actions >= 1, f"action limit must be at least 1 (got {c.max_actions})"),
        (0 <= c.seed < 2**64, f"seed must be in [0, 2**64) (got {c.seed})"),
    ]
    return [message for ok, message in checks if not ok]


def dut_seat(match_index: int) -> int:
    """The deck under test sits in seat ``match_index % 2``, so seats alternate by match."""
    return match_index % 2


def seat_decks(config: BenchmarkConfig, match_index: int) -> tuple[DeckList, DeckList]:
    dut = to_decklist(config.deck_under_test)
    bench = to_decklist(config.benchmark_deck)
    return (dut, bench) if dut_seat(match_index) == 0 else (bench, dut)


def game_seed(master: int, match_index: int, game_index: int) -> int:
    return derive_seed(master, "match", match_index, "game", game_index)


def agent_seed(master: int, match_index: int, game_index: int, seat: int) -> int:
    return derive_seed(master, "match", match_index, "game", game_index, "agent", seat)


def own_turn(turn: int, seat: int, first_player: int) -> int:
    """How many turns ``seat`` has started by game turn ``turn``."""
    return (turn + 1) // 2 if seat == first_player else turn // 2
