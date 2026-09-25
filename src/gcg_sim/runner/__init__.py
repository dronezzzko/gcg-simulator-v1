"""Benchmark runner: seeds, games, BO3 matches, worker processes, replays."""

from gcg_sim.runner.agents import AgentFactory, AgentSpec, FactoryRef, GameAgent
from gcg_sim.runner.config import BenchmarkConfig, agent_seed, dut_seat, game_seed
from gcg_sim.runner.digest import state_digest
from gcg_sim.runner.game import GameFailure, play_game, replay, replay_game
from gcg_sim.runner.match import play_match
from gcg_sim.runner.records import BenchmarkRun, DutCardEvents, GameRecord, MatchRecord
from gcg_sim.runner.run import run_benchmark

__all__ = [
    "AgentFactory",
    "AgentSpec",
    "BenchmarkConfig",
    "BenchmarkRun",
    "DutCardEvents",
    "FactoryRef",
    "GameAgent",
    "GameFailure",
    "GameRecord",
    "MatchRecord",
    "agent_seed",
    "dut_seat",
    "game_seed",
    "play_game",
    "play_match",
    "replay",
    "replay_game",
    "run_benchmark",
    "state_digest",
]
