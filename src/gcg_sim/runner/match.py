"""Match sequencing: best-of-three (official BO3 Match Rules) or single games."""

from __future__ import annotations

from gcg_sim.runner.agents import AgentFactory
from gcg_sim.runner.config import BenchmarkConfig, dut_seat
from gcg_sim.runner.game import match_loser_seat, play_game
from gcg_sim.runner.records import BENCH, DRAW, DUT, GameRecord, MatchRecord


def _winner(dut_points: int, bench_points: int) -> str:
    if dut_points > bench_points:
        return DUT
    if bench_points > dut_points:
        return BENCH
    return DRAW


def play_match(
    config: BenchmarkConfig, match_index: int, *, agent_factory: AgentFactory | None = None
) -> MatchRecord:
    """Play games until one side has ``wins_needed`` match points or the game cap is reached.

    Game 1's Player One is chosen after a seeded die roll; each later game is chosen by the
    previous game's (match-scoring) loser. After a game without a match point (turn limit, or
    a BO1-style draw) the next chooser is again decided by the seeded die roll.
    """
    games: list[GameRecord] = []
    points = {DUT: 0, BENCH: 0}
    chooser: int | None = None
    for game_index in range(config.games_per_match):
        record = play_game(config, match_index, game_index, chooser, agent_factory=agent_factory)
        games.append(record)
        if record.match_point is not None:
            points[record.match_point] += 1
        if max(points.values()) >= config.wins_needed:
            break
        chooser = match_loser_seat(record)
    return MatchRecord(
        match_index=match_index,
        dut_seat=dut_seat(match_index),
        games=tuple(games),
        winner=_winner(points[DUT], points[BENCH]),
        dut_points=points[DUT],
        bench_points=points[BENCH],
    )
