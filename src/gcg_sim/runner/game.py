"""Playing and replaying single games."""

from __future__ import annotations

from collections.abc import Iterable, Sequence
from typing import Any

from gcg_sim.engine.game import DeckList, apply, new_game
from gcg_sim.engine.state import Action, GameState
from gcg_sim.engine.types import EndReason
from gcg_sim.runner.agents import (
    AgentFactory,
    AgentSpec,
    AgentUnavailableError,
    GameAgent,
    default_agent_factory,
)
from gcg_sim.runner.config import (
    BenchmarkConfig,
    agent_seed,
    dut_seat,
    game_seed,
    seat_decks,
)
from gcg_sim.runner.digest import state_digest
from gcg_sim.runner.instrument import GameObserver
from gcg_sim.runner.records import BENCH, DRAW, DUT, ActionJson, GameRecord


class GameFailure(RuntimeError):
    """A game could not be completed (engine error, illegal agent action, unimplemented card)."""


def _role(seat: int, dut: int) -> str:
    return DUT if seat == dut else BENCH


def result_label(winner_seat: int, dut: int) -> str:
    return DRAW if winner_seat < 0 else _role(winner_seat, dut)


def match_point(st: GameState, dut: int, fmt: str) -> str | None:
    """Who scores this game for the match. A simultaneous defeat has no game winner; in BO3 the
    turn player at the end loses it (TRM 5.2). A turn-limit draw scores for nobody."""
    if st.winner is None:
        raise GameFailure("the game has not ended")
    if st.winner >= 0:
        return _role(st.winner, dut)
    if fmt == "bo3" and st.end_reason is EndReason.BOTH_DEFEATED:
        return _role(1 - st.active, dut)
    return None


def match_loser_seat(record: GameRecord) -> int | None:
    """The seat that chooses Player One next game (the match-scoring loser), if any."""
    if record.match_point is None:
        return None
    winner_seat = record.dut_seat if record.match_point == DUT else record.bench_seat
    return 1 - winner_seat


def _make_agents(
    config: BenchmarkConfig, match_index: int, game_index: int, factory: AgentFactory
) -> tuple[GameAgent, GameAgent]:
    dut = dut_seat(match_index)
    specs = [
        AgentSpec(
            role=_role(seat, dut),
            seat=seat,
            preset=config.ai_preset,
            seed=agent_seed(config.seed, match_index, game_index, seat),
            log=config.decision_log,
        )
        for seat in (0, 1)
    ]
    return factory(specs[0]), factory(specs[1])


def _drive(
    st: GameState,
    agents: tuple[GameAgent, GameAgent],
    observer: GameObserver,
    log: list[dict[str, Any]] | None,
) -> list[ActionJson]:
    actions: list[ActionJson] = []
    logged = [0, 0]
    while st.pending is not None:
        decision = st.pending
        version = st.version
        agent = agents[decision.player]
        action = agent.choose(st, decision.player)
        if st.version != version or st.pending is not decision:
            raise GameFailure(f"agent {agent.name!r} mutated the game state")
        if log is not None:
            entries = agent.decision_log()
            log.extend(entries[logged[decision.player] :])
            logged[decision.player] = len(entries)
        observer.before(st, decision, action)
        apply(st, action)
        observer.after(st, decision, action)
        k, a, b, c = action.to_json()
        actions.append((str(k), int(a), int(b), int(c)))
    return actions


def play_game(
    config: BenchmarkConfig,
    match_index: int,
    game_index: int,
    chooser: int | None,
    *,
    agent_factory: AgentFactory | None = None,
) -> GameRecord:
    """Play one game of match ``match_index`` (game ``game_index``, 0-based) to completion."""
    factory = agent_factory or default_agent_factory
    dut = dut_seat(match_index)
    seed = game_seed(config.seed, match_index, game_index)
    try:
        st = new_game(
            seat_decks(config, match_index),
            seed,
            chooser=chooser,
            turn_limit=config.turn_limit,
            max_actions=config.max_actions,
        )
        agents = _make_agents(config, match_index, game_index, factory)
        observer = GameObserver(st, dut)
        log: list[dict[str, Any]] | None = [] if config.decision_log else None
        actions = _drive(st, agents, observer, log)
    except (GameFailure, AgentUnavailableError):
        raise
    except Exception as exc:
        raise GameFailure(
            f"match {match_index} game {game_index + 1} (seed {seed}) failed: "
            f"{type(exc).__name__}: {exc}"
        ) from exc
    if st.winner is None or st.end_reason is None:
        raise GameFailure(f"match {match_index} game {game_index + 1} stopped without a result")
    return GameRecord(
        match_index=match_index,
        game_index=game_index,
        seed=seed,
        agent_seeds=(
            agent_seed(config.seed, match_index, game_index, 0),
            agent_seed(config.seed, match_index, game_index, 1),
        ),
        agents=(agents[0].name, agents[1].name),
        dut_seat=dut,
        chooser=chooser,
        chooser_seat=st.setup_chooser,
        first_player=st.first_player,
        dut_on_play=st.first_player == dut,
        winner=result_label(st.winner, dut),
        end_reason=st.end_reason.value,
        match_point=match_point(st, dut, config.fmt),
        turns=st.turn,
        active_at_end=st.active,
        actions=tuple(actions),
        redraws=(st.redraws[0], st.redraws[1]),
        dut_cards=observer.dut_events(st),
        bench_seen=tuple(sorted(observer.bench_seen)),
        final_state_sha256=state_digest(st),
        turn_limit=config.turn_limit,
        max_actions=config.max_actions,
        decision_log=None if log is None else tuple(log),
    )


def replay_game(
    decks: tuple[DeckList, DeckList],
    seed: int,
    chooser: int | None,
    actions: Iterable[Sequence[object]],
    *,
    turn_limit: int,
    max_actions: int,
) -> GameState:
    """Re-create a game from its seed and re-apply the recorded actions (``[kind, a, b, c]``)."""
    st = new_game(decks, seed, chooser=chooser, turn_limit=turn_limit, max_actions=max_actions)
    for a in actions:
        apply(st, Action.from_json(list(a)))
    return st


def replay(record: GameRecord, config: BenchmarkConfig) -> GameState:
    """Replay ``record`` under ``config``; the result has the recorded final state."""
    return replay_game(
        seat_decks(config, record.match_index),
        record.seed,
        record.chooser,
        record.actions,
        turn_limit=record.turn_limit,
        max_actions=record.max_actions,
    )
