from __future__ import annotations

from collections.abc import Callable
from pathlib import Path

import pytest

from gcg_sim.deck import Deck, load_deck
from gcg_sim.reports import ReportPaths, write_reports
from gcg_sim.runner import BenchmarkConfig, BenchmarkRun, FactoryRef, GameRecord, run_benchmark
from gcg_sim.runner.records import DutCardEvents, PlayEvent

ROOT = Path(__file__).resolve().parents[2]
EXAMPLES = ROOT / "examples" / "decks"
RANDOM_AGENT = f"{ROOT / 'tests' / 'runner' / 'random_agent.py'}:make_random_agent"

RecordFactory = Callable[..., GameRecord]


@pytest.fixture(scope="session")
def federation() -> Deck:
    return load_deck(EXAMPLES / "blue-white-federation.txt")


@pytest.fixture(scope="session")
def zeon() -> Deck:
    return load_deck(EXAMPLES / "red-green-zeon.txt")


@pytest.fixture(scope="session")
def run(federation: Deck, zeon: Deck) -> BenchmarkRun:
    config = BenchmarkConfig(federation, zeon, matches=60, seed=3, replays=2)
    return run_benchmark(config, agent_factory=FactoryRef(RANDOM_AGENT))


@pytest.fixture(scope="session")
def written(run: BenchmarkRun, tmp_path_factory: pytest.TempPathFactory) -> ReportPaths:
    return write_reports(run, tmp_path_factory.mktemp("reports"))


@pytest.fixture
def make_record() -> RecordFactory:
    def build(
        m: int,
        g: int,
        winner: str,
        *,
        point: str | None = None,
        reason: str = "battle_damage",
        dut_on_play: bool = True,
        turns: int = 12,
        drawn: tuple[str, ...] = (),
        played: tuple[str, ...] = (),
        bench_seen: tuple[str, ...] = (),
        redrew: bool = False,
    ) -> GameRecord:
        events = DutCardEvents(
            initial_hand=drawn[:5],
            redrew=redrew,
            opening_hand=drawn[:5],
            drawn=tuple(sorted(drawn)),
            played=tuple(PlayEvent(n, 3, 2) for n in played),
            never_played=tuple(sorted(n for n in drawn if n not in played)),
            in_hand_at_end=(),
        )
        return GameRecord(
            match_index=m,
            game_index=g,
            seed=m * 10 + g,
            agent_seeds=(1, 2),
            agents=("fake", "fake"),
            dut_seat=m % 2,
            chooser=None if g == 0 else 0,
            chooser_seat=0,
            first_player=m % 2 if dut_on_play else 1 - m % 2,
            dut_on_play=dut_on_play,
            winner=winner,
            end_reason=reason,
            match_point=point if point is not None or winner == "draw" else winner,
            turns=turns,
            active_at_end=0,
            actions=(),
            redraws=(redrew, False) if m % 2 == 0 else (False, redrew),
            dut_cards=events,
            bench_seen=bench_seen,
            final_state_sha256="0" * 64,
            turn_limit=200,
            max_actions=20000,
        )

    return build
