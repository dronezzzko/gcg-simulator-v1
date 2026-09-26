"""Per-card instrumentation of the real game (never of search copies).

The observer sees every decision of the real game: the action about to be applied and the
state after it. Plays come from the action stream; draws from the hand (state diff) and the
engine's per-turn event history, which also records draws whose cards leave the hand before
the next decision. Benchmark cards count as "seen" once they are played, deployed, paired,
revealed as a Burst, or present in a public in-play zone.
"""

from __future__ import annotations

from collections.abc import Iterable

from gcg_sim.engine import view as V
from gcg_sim.engine.state import Action, Decision, GameState, HistoryEvent
from gcg_sim.engine.types import ActionKind, DecisionKind, Zone
from gcg_sim.runner.config import own_turn
from gcg_sim.runner.records import DutCardEvents, PlayEvent

PLAY_KINDS = frozenset(
    {ActionKind.PLAY_UNIT, ActionKind.PLAY_BASE, ActionKind.PLAY_COMMAND, ActionKind.PAIR}
)
IN_PLAY_ZONES = (Zone.BATTLE, Zone.PAIRED, Zone.BASE, Zone.RESOLVING)
DRAW_EVENTS = frozenset({"draw", "shield_to_hand"})
SEEN_EVENTS = frozenset({"play", "deployed", "paired", "burst_revealed", "command_activated"})


def card_number(st: GameState, uid: int) -> str:
    return V.cdef(st, uid).card_number


class GameObserver:
    """Collects per-card events for the deck under test and benchmark cards seen in play."""

    def __init__(self, st: GameState, dut_seat: int) -> None:
        self.dut = dut_seat
        self.bench = 1 - dut_seat
        self.initial_hand: tuple[int, ...] = ()
        self.redrew = False
        self.opening_hand: tuple[int, ...] = ()
        self.tracking = False
        self.drawn: dict[int, None] = {}
        self.plays: list[PlayEvent] = []
        self.played_uids: set[int] = set()
        self.bench_seen: set[str] = set()
        self._history: list[HistoryEvent] = st.history
        self._history_done = 0

    def before(self, st: GameState, decision: Decision, action: Action) -> None:
        if decision.kind is DecisionKind.REDRAW and decision.player == self.dut:
            self.initial_hand = tuple(st.zones[self.dut][Zone.HAND])
        if action.kind not in PLAY_KINDS:
            return
        number = card_number(st, action.a)
        if decision.player == self.dut:
            turn = own_turn(st.turn, self.dut, st.first_player)
            self.plays.append(PlayEvent(number, st.turn, turn))
            self.played_uids.add(action.a)
        else:
            self._see_bench(st, action.a)

    def after(self, st: GameState, decision: Decision, action: Action) -> None:
        if decision.kind is DecisionKind.REDRAW and decision.player == self.dut:
            self.redrew = action.kind is ActionKind.REDRAW
            self.opening_hand = tuple(st.zones[self.dut][Zone.HAND])
            self.tracking = True
        self._scan_history(st)
        if self.tracking:
            for uid in st.zones[self.dut][Zone.HAND]:
                self.drawn.setdefault(uid, None)
        for zone in IN_PLAY_ZONES:
            for uid in st.zones[self.bench][zone]:
                self._see_bench(st, uid)

    def _see_bench(self, st: GameState, uid: int) -> None:
        cdef = V.cdef(st, uid)
        if st.cards[uid].owner == self.bench and not cdef.is_token:
            self.bench_seen.add(cdef.card_number)

    def _scan_history(self, st: GameState) -> None:
        if st.history is not self._history:
            self._history = st.history
            self._history_done = 0
        for h in st.history[self._history_done :]:
            if h.uid < 0:
                continue
            owner = st.cards[h.uid].owner
            if self.tracking and h.kind in DRAW_EVENTS and owner == self.dut:
                self.drawn.setdefault(h.uid, None)
            elif h.kind in SEEN_EVENTS and owner == self.bench:
                self._see_bench(st, h.uid)
        self._history_done = len(st.history)

    def dut_events(self, st: GameState) -> DutCardEvents:
        def numbers(uids: Iterable[int]) -> tuple[str, ...]:
            return tuple(sorted(card_number(st, u) for u in uids))

        never = [u for u in self.drawn if u not in self.played_uids]
        return DutCardEvents(
            initial_hand=numbers(self.initial_hand),
            redrew=self.redrew,
            opening_hand=numbers(self.opening_hand),
            drawn=numbers(self.drawn),
            played=tuple(self.plays),
            never_played=numbers(never),
            in_hand_at_end=numbers(st.zones[self.dut][Zone.HAND]),
        )
