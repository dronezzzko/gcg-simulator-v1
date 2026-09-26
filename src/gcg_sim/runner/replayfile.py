"""Self-contained replay files: decklists, seed, chooser, and actions of one game, plus the
expected outcome and a readable log. ``check_replay`` re-simulates and compares."""

from __future__ import annotations

from collections import Counter
from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from typing import Any

from gcg_sim.effects.registry import UnimplementedCardError
from gcg_sim.engine import view as V
from gcg_sim.engine.game import DeckList, IllegalActionError, apply, new_game
from gcg_sim.engine.state import Action, Decision, GameState
from gcg_sim.engine.types import NO_ARG, PLAYER_TARGET, ActionKind, DecisionKind
from gcg_sim.runner.config import BenchmarkConfig, seat_decks
from gcg_sim.runner.digest import state_digest
from gcg_sim.runner.records import BENCH, DUT, GameRecord

REPLAY_FORMAT = "gcg-sim-replay"
REPLAY_VERSION = 1
A = ActionKind
CARD_CHOICES = frozenset({DecisionKind.SELECT, DecisionKind.DISCARD, DecisionKind.EXCESS})


class ReplayError(ValueError):
    """A replay file is malformed or refers to cards the simulator cannot play."""


def _label(st: GameState, uid: int) -> str:
    if not 0 <= uid < len(st.cards):
        return str(uid)
    cdef = V.cdef(st, uid)
    return f"{cdef.card_number} {cdef.name}"


def _describe_choice(st: GameState, dec: Decision, a: Action) -> str:
    if dec.kind is DecisionKind.ARRANGE and a.b != NO_ARG:
        return f"{'bottom' if a.a else 'top'}: {_label(st, a.b)}"
    if dec.kind in CARD_CHOICES and dec.ctx("mode") == NO_ARG and dec.ctx("rest_sub") == NO_ARG:
        return f"select {_label(st, a.a)}"
    return f"select option {a.a}"


def describe_action(st: GameState, dec: Decision, a: Action) -> str:
    """A readable description of ``a`` in state ``st`` (before it is applied)."""
    k = a.kind
    if k in (A.PLAY_UNIT, A.PLAY_BASE, A.PLAY_COMMAND):
        text = f"play {_label(st, a.a)}"
    elif k is A.PAIR:
        text = f"pair {_label(st, a.a)} with {_label(st, a.b)}"
    elif k is A.ACTIVATE:
        text = f"activate {_label(st, a.a)}"
    elif k is A.ATTACK:
        target = "the player" if a.b == PLAYER_TARGET else _label(st, a.b)
        text = f"attack {target} with {_label(st, a.a)}"
    elif k is A.BLOCK:
        text = f"block with {_label(st, a.a)}"
    elif k is A.GO_FIRST:
        text = f"P{a.a} goes first"
    elif k is A.SELECT:
        text = _describe_choice(st, dec, a)
    else:
        text = k.value
    return f"{text} [{a}]"


def _decklist(seat: Mapping[str, Any]) -> DeckList:
    def expand(counts: Mapping[str, int]) -> tuple[str, ...]:
        return tuple(sorted(n for n, c in counts.items() for _ in range(int(c))))

    return DeckList(main=expand(seat["main"]), resources=expand(seat["resources"]))


def replay_with_log(
    decks: tuple[DeckList, DeckList],
    seed: int,
    chooser: int | None,
    actions: Sequence[Sequence[Any]],
    *,
    turn_limit: int,
    max_actions: int,
) -> tuple[GameState, list[str]]:
    try:
        st = new_game(decks, seed, chooser=chooser, turn_limit=turn_limit, max_actions=max_actions)
    except UnimplementedCardError as exc:
        raise ReplayError(f"the replay uses a card this version cannot play: {exc}") from exc
    log: list[str] = []
    for index, raw in enumerate(actions):
        action = Action.from_json(list(raw))
        dec = st.pending
        if dec is None:
            raise ReplayError("the replay has more actions than the game had decisions")
        log.append(f"T{st.turn} P{dec.player} {dec.kind.value}: {describe_action(st, dec, action)}")
        try:
            apply(st, action)
        except IllegalActionError as exc:
            raise ReplayError(f"action {index} {action} is not legal in the replayed game") from exc
    reason = st.end_reason.value if st.end_reason else "not finished"
    log.append(f"T{st.turn} game over: winner P{st.winner} ({reason})")
    return st, log


def replay_document(record: GameRecord, config: BenchmarkConfig, label: str) -> dict[str, Any]:
    decks = seat_decks(config, record.match_index)
    st, log = replay_with_log(
        decks,
        record.seed,
        record.chooser,
        record.actions,
        turn_limit=record.turn_limit,
        max_actions=record.max_actions,
    )
    names = {
        record.dut_seat: config.deck_under_test.name,
        record.bench_seat: config.benchmark_deck.name,
    }
    return {
        "format": REPLAY_FORMAT,
        "version": REPLAY_VERSION,
        "label": label,
        "match_index": record.match_index,
        "game_index": record.game_index,
        "seed": record.seed,
        "chooser": record.chooser,
        "turn_limit": record.turn_limit,
        "max_actions": record.max_actions,
        "seats": [
            {
                "seat": seat,
                "role": DUT if seat == record.dut_seat else BENCH,
                "deck": names[seat],
                "agent": record.agents[seat],
                "agent_seed": record.agent_seeds[seat],
                "main": dict(sorted(Counter(decks[seat].main).items())),
                "resources": dict(sorted(Counter(decks[seat].resources).items())),
            }
            for seat in (0, 1)
        ],
        "actions": [list(a) for a in record.actions],
        "expected": {
            "winner": record.winner,
            "winner_seat": st.winner,
            "end_reason": record.end_reason,
            "turns": record.turns,
            "final_state_sha256": record.final_state_sha256,
        },
        "log": log,
        "decision_log": None if record.decision_log is None else list(record.decision_log),
    }


@dataclass(frozen=True, slots=True)
class ReplayCheck:
    ok: bool
    mismatches: tuple[str, ...]
    winner_seat: int | None
    end_reason: str | None
    turns: int
    final_state_sha256: str
    log: tuple[str, ...]


def check_replay(doc: Mapping[str, Any]) -> ReplayCheck:
    """Re-simulate a replay document and compare with its expected outcome."""
    if doc.get("format") != REPLAY_FORMAT or doc.get("version") != REPLAY_VERSION:
        raise ReplayError(f"not a {REPLAY_FORMAT} v{REPLAY_VERSION} document")
    try:
        seats = sorted(doc["seats"], key=lambda s: int(s["seat"]))
        decks = (_decklist(seats[0]), _decklist(seats[1]))
        st, log = replay_with_log(
            decks,
            int(doc["seed"]),
            None if doc["chooser"] is None else int(doc["chooser"]),
            doc["actions"],
            turn_limit=int(doc["turn_limit"]),
            max_actions=int(doc["max_actions"]),
        )
        expected = doc["expected"]
    except ReplayError:
        raise
    except (KeyError, TypeError, IndexError, ValueError) as exc:
        raise ReplayError(f"malformed replay document: {type(exc).__name__}: {exc}") from exc
    end_reason = st.end_reason.value if st.end_reason else None
    digest = state_digest(st)
    actual: dict[str, object] = {
        "winner_seat": st.winner,
        "end_reason": end_reason,
        "turns": st.turn,
        "final_state_sha256": digest,
    }
    mismatches = tuple(
        f"{k}: expected {expected.get(k)!r}, replay gave {v!r}"
        for k, v in actual.items()
        if expected.get(k) != v
    )
    return ReplayCheck(
        ok=not mismatches,
        mismatches=mismatches,
        winner_seat=st.winner,
        end_reason=end_reason,
        turns=st.turn,
        final_state_sha256=digest,
        log=tuple(log),
    )
