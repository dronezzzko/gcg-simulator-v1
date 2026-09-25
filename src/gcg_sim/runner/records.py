"""Game, match, and run records. Everything except :class:`RunTiming` is a deterministic
function of the configuration and seed, so it can be written to reproducible reports."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any

from gcg_sim.runner.config import BenchmarkConfig

DUT = "dut"
BENCH = "bench"
DRAW = "draw"

ActionJson = tuple[str, int, int, int]


@dataclass(frozen=True, slots=True)
class PlayEvent:
    """A card the deck under test played from its hand (unit, base, command, or pairing)."""

    card_number: str
    turn: int
    own_turn: int

    def to_json(self) -> list[Any]:
        return [self.card_number, self.turn, self.own_turn]

    @staticmethod
    def from_json(d: list[Any]) -> PlayEvent:
        return PlayEvent(str(d[0]), int(d[1]), int(d[2]))


@dataclass(frozen=True, slots=True)
class DutCardEvents:
    """Per-card observations of the deck under test in one game (card numbers, one per copy).

    ``drawn`` counts every copy that entered the hand after the redraw decision, including the
    kept opening hand; ``never_played`` are drawn copies that were never played from the hand.
    """

    initial_hand: tuple[str, ...]
    redrew: bool
    opening_hand: tuple[str, ...]
    drawn: tuple[str, ...]
    played: tuple[PlayEvent, ...]
    never_played: tuple[str, ...]
    in_hand_at_end: tuple[str, ...]

    def to_json(self) -> dict[str, Any]:
        return {
            "initial_hand": list(self.initial_hand),
            "redrew": self.redrew,
            "opening_hand": list(self.opening_hand),
            "drawn": list(self.drawn),
            "played": [p.to_json() for p in self.played],
            "never_played": list(self.never_played),
            "in_hand_at_end": list(self.in_hand_at_end),
        }

    @staticmethod
    def from_json(d: dict[str, Any]) -> DutCardEvents:
        return DutCardEvents(
            initial_hand=tuple(d["initial_hand"]),
            redrew=bool(d["redrew"]),
            opening_hand=tuple(d["opening_hand"]),
            drawn=tuple(d["drawn"]),
            played=tuple(PlayEvent.from_json(p) for p in d["played"]),
            never_played=tuple(d["never_played"]),
            in_hand_at_end=tuple(d["in_hand_at_end"]),
        )


@dataclass(frozen=True, slots=True)
class GameRecord:
    """One played game. ``chooser`` is the argument passed to ``new_game`` (``None`` = seeded
    die roll); ``chooser_seat`` is the seat that actually chose Player One."""

    match_index: int
    game_index: int
    seed: int
    agent_seeds: tuple[int, int]
    agents: tuple[str, str]
    dut_seat: int
    chooser: int | None
    chooser_seat: int
    first_player: int
    dut_on_play: bool
    winner: str
    end_reason: str
    match_point: str | None
    turns: int
    active_at_end: int
    actions: tuple[ActionJson, ...]
    redraws: tuple[bool, bool]
    dut_cards: DutCardEvents
    bench_seen: tuple[str, ...]
    final_state_sha256: str
    turn_limit: int
    max_actions: int
    decision_log: tuple[dict[str, Any], ...] | None = None

    @property
    def bench_seat(self) -> int:
        return 1 - self.dut_seat

    def to_json(self) -> dict[str, Any]:
        return {
            "match_index": self.match_index,
            "game_index": self.game_index,
            "seed": self.seed,
            "agent_seeds": list(self.agent_seeds),
            "agents": list(self.agents),
            "dut_seat": self.dut_seat,
            "chooser": self.chooser,
            "chooser_seat": self.chooser_seat,
            "first_player": self.first_player,
            "dut_on_play": self.dut_on_play,
            "winner": self.winner,
            "end_reason": self.end_reason,
            "match_point": self.match_point,
            "turns": self.turns,
            "active_at_end": self.active_at_end,
            "actions": [list(a) for a in self.actions],
            "redraws": list(self.redraws),
            "dut_cards": self.dut_cards.to_json(),
            "bench_seen": list(self.bench_seen),
            "final_state_sha256": self.final_state_sha256,
            "turn_limit": self.turn_limit,
            "max_actions": self.max_actions,
            "decision_log": None if self.decision_log is None else list(self.decision_log),
        }

    @staticmethod
    def from_json(d: dict[str, Any]) -> GameRecord:
        seeds, agents, redraws = d["agent_seeds"], d["agents"], d["redraws"]
        log = d.get("decision_log")
        return GameRecord(
            match_index=int(d["match_index"]),
            game_index=int(d["game_index"]),
            seed=int(d["seed"]),
            agent_seeds=(int(seeds[0]), int(seeds[1])),
            agents=(str(agents[0]), str(agents[1])),
            dut_seat=int(d["dut_seat"]),
            chooser=None if d["chooser"] is None else int(d["chooser"]),
            chooser_seat=int(d["chooser_seat"]),
            first_player=int(d["first_player"]),
            dut_on_play=bool(d["dut_on_play"]),
            winner=str(d["winner"]),
            end_reason=str(d["end_reason"]),
            match_point=None if d["match_point"] is None else str(d["match_point"]),
            turns=int(d["turns"]),
            active_at_end=int(d["active_at_end"]),
            actions=tuple((str(a[0]), int(a[1]), int(a[2]), int(a[3])) for a in d["actions"]),
            redraws=(bool(redraws[0]), bool(redraws[1])),
            dut_cards=DutCardEvents.from_json(d["dut_cards"]),
            bench_seen=tuple(d["bench_seen"]),
            final_state_sha256=str(d["final_state_sha256"]),
            turn_limit=int(d["turn_limit"]),
            max_actions=int(d["max_actions"]),
            decision_log=None if log is None else tuple(log),
        )


@dataclass(frozen=True, slots=True)
class MatchRecord:
    """A best-of-three (or single-game) match. ``points`` are match-scoring game wins."""

    match_index: int
    dut_seat: int
    games: tuple[GameRecord, ...]
    winner: str
    dut_points: int
    bench_points: int

    def to_json(self) -> dict[str, Any]:
        return {
            "match_index": self.match_index,
            "dut_seat": self.dut_seat,
            "winner": self.winner,
            "dut_points": self.dut_points,
            "bench_points": self.bench_points,
            "games": [g.game_index for g in self.games],
        }


@dataclass(frozen=True, slots=True)
class RunTiming:
    """Wall-clock measurements; written only to ``timing.json``."""

    started_at: str
    finished_at: str
    wall_seconds: float
    match_seconds: tuple[float, ...]
    workers: int


@dataclass(frozen=True, slots=True)
class BenchmarkRun:
    config: BenchmarkConfig
    matches: tuple[MatchRecord, ...]
    timing: RunTiming
    agent_factory: str

    @property
    def games(self) -> tuple[GameRecord, ...]:
        return tuple(g for m in self.matches for g in m.games)
