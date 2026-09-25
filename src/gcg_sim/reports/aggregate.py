"""Aggregate game records into outcome, split, length, mulligan, and per-card statistics.

All results are from the deck under test's point of view. Win rates count draws as
non-wins (``n`` is every game or match), and draws are reported separately.
"""

from __future__ import annotations

from collections import Counter
from collections.abc import Sequence
from dataclasses import dataclass, field
from typing import Any

from gcg_sim.cards.db import get_card_db
from gcg_sim.deck.model import Deck
from gcg_sim.engine.types import EndReason
from gcg_sim.reports.stats import distribution, rate, ratio, rnd
from gcg_sim.runner.records import BENCH, DRAW, DUT, GameRecord, MatchRecord

WIN, LOSS = "win", "loss"
RESULTS = (WIN, LOSS, DRAW)


def dut_result(g: GameRecord) -> str:
    return {DUT: WIN, BENCH: LOSS}.get(g.winner, DRAW)


def _outcomes(winners: Sequence[str]) -> dict[str, Any]:
    counts = Counter(winners)
    return {
        "n": len(winners),
        "dut_wins": counts[DUT],
        "bench_wins": counts[BENCH],
        "draws": counts[DRAW],
        "dut_win_rate": rate(counts[DUT], len(winners)),
    }


def game_outcomes(games: Sequence[GameRecord]) -> dict[str, Any]:
    return _outcomes([g.winner for g in games])


def match_outcomes(matches: Sequence[MatchRecord]) -> dict[str, Any]:
    return _outcomes([m.winner for m in matches])


def splits(games: Sequence[GameRecord], games_per_match: int) -> dict[str, Any]:
    return {
        "on_play": game_outcomes([g for g in games if g.dut_on_play]),
        "on_draw": game_outcomes([g for g in games if not g.dut_on_play]),
        "by_game_number": {
            str(i + 1): game_outcomes([g for g in games if g.game_index == i])
            for i in range(games_per_match)
        },
    }


def end_reasons(games: Sequence[GameRecord]) -> list[dict[str, Any]]:
    reasons = sorted({g.end_reason for g in games})
    return [
        {"reason": r, **game_outcomes([g for g in games if g.end_reason == r])} for r in reasons
    ]


def draws(games: Sequence[GameRecord]) -> dict[str, Any]:
    drawn = [g for g in games if g.winner == DRAW]
    return {
        "games": len(drawn),
        "both_defeated": sum(g.end_reason == EndReason.BOTH_DEFEATED.value for g in drawn),
        "turn_limit": sum(g.end_reason == EndReason.TURN_LIMIT.value for g in drawn),
        "scored_for_match_by_turn_player_rule": sum(g.match_point is not None for g in drawn),
    }


def game_length(games: Sequence[GameRecord]) -> dict[str, Any]:
    histogram = Counter(g.turns for g in games)
    return {
        "turns": distribution([g.turns for g in games]),
        "by_result": {
            r: distribution([g.turns for g in games if dut_result(g) == r]) for r in RESULTS
        },
        "histogram": [{"turns": t, "games": histogram[t]} for t in sorted(histogram)],
    }


def mulligan(games: Sequence[GameRecord]) -> dict[str, Any]:
    redrew = [g for g in games if g.dut_cards.redrew]
    kept = [g for g in games if not g.dut_cards.redrew]
    bench_redraws = sum(g.redraws[g.bench_seat] for g in games)
    return {
        "dut": {
            "redraw_rate": rate(len(redrew), len(games)),
            "win_rate_after_redraw": rate(sum(g.winner == DUT for g in redrew), len(redrew)),
            "win_rate_after_keep": rate(sum(g.winner == DUT for g in kept), len(kept)),
        },
        "bench": {"redraw_rate": rate(bench_redraws, len(games))},
    }


@dataclass
class _CardTally:
    drawn_games: int = 0
    drawn_wins: int = 0
    drawn_copies: int = 0
    opening_games: int = 0
    played_games: int = 0
    played_wins: int = 0
    drawn_unplayed_games: int = 0
    drawn_unplayed_wins: int = 0
    plays: int = 0
    own_turns: list[int] = field(default_factory=list)
    never_copies: int = 0
    never_games: int = 0
    end_hand_copies: int = 0
    initial_games: int = 0
    redrawn_games: int = 0

    def add(self, g: GameRecord, number: str) -> None:
        c = g.dut_cards
        win = g.winner == DUT
        drawn = c.drawn.count(number)
        turns = [p.own_turn for p in c.played if p.card_number == number]
        self.drawn_copies += drawn
        self.drawn_games += drawn > 0
        self.drawn_wins += drawn > 0 and win
        self.opening_games += number in c.opening_hand
        self.played_games += bool(turns)
        self.played_wins += bool(turns) and win
        self.drawn_unplayed_games += drawn > 0 and not turns
        self.drawn_unplayed_wins += drawn > 0 and not turns and win
        self.plays += len(turns)
        self.own_turns.extend(turns)
        never = c.never_played.count(number)
        self.never_copies += never
        self.never_games += never > 0
        self.end_hand_copies += c.in_hand_at_end.count(number)
        if number in c.initial_hand:
            self.initial_games += 1
            self.redrawn_games += c.redrew


def _card_entry(number: str, copies: int, t: _CardTally, n: int, wins: int) -> dict[str, Any]:
    cdef = get_card_db()[number]
    turns = Counter(t.own_turns)
    return {
        "card_number": number,
        "name": cdef.name,
        "card_type": cdef.card_type.value,
        "level": cdef.level,
        "cost": cdef.cost,
        "copies": copies,
        "drawn": {
            "games": t.drawn_games,
            "game_rate": ratio(t.drawn_games, n),
            "copies": t.drawn_copies,
            "copies_per_game": ratio(t.drawn_copies, n),
        },
        "opening_hand": {"games": t.opening_games, "game_rate": ratio(t.opening_games, n)},
        "played": {
            "games": t.played_games,
            "plays": t.plays,
            "rate_when_drawn": ratio(t.played_games, t.drawn_games),
            "own_turn": distribution(t.own_turns),
            "by_own_turn": [{"own_turn": k, "plays": turns[k]} for k in sorted(turns)],
        },
        "win_rate_when_drawn": rate(t.drawn_wins, t.drawn_games),
        "win_rate_when_not_drawn": rate(wins - t.drawn_wins, n - t.drawn_games),
        "win_rate_when_played": rate(t.played_wins, t.played_games),
        "win_rate_when_drawn_not_played": rate(t.drawn_unplayed_wins, t.drawn_unplayed_games),
        "held_never_played": {
            "copies": t.never_copies,
            "rate_of_drawn_copies": ratio(t.never_copies, t.drawn_copies),
            "games": t.never_games,
            "in_hand_at_end_copies": t.end_hand_copies,
        },
        "mulligan": {
            "in_initial_hand_games": t.initial_games,
            "redrawn_games": t.redrawn_games,
            "rate": ratio(t.redrawn_games, t.initial_games),
        },
    }


def card_stats(games: Sequence[GameRecord], deck: Deck) -> list[dict[str, Any]]:
    """Per-card statistics for the deck under test (main deck), sorted by card number."""
    copies = Counter(deck.main)
    wins = sum(g.winner == DUT for g in games)
    out = []
    for number in sorted(copies):
        tally = _CardTally()
        for g in games:
            tally.add(g, number)
        out.append(_card_entry(number, copies[number], tally, len(games), wins))
    return out


def _bench_entry(number: str, copies: int, games: Sequence[GameRecord]) -> dict[str, Any]:
    cdef = get_card_db()[number]
    seen = [g for g in games if number in g.bench_seen]
    unseen = [g for g in games if number not in g.bench_seen]
    when_seen = rate(sum(g.winner == BENCH for g in seen), len(seen))
    when_unseen = rate(sum(g.winner == BENCH for g in unseen), len(unseen))
    lift = None
    if when_seen["rate"] is not None and when_unseen["rate"] is not None:
        lift = rnd(when_seen["rate"] - when_unseen["rate"])
    return {
        "card_number": number,
        "name": cdef.name,
        "card_type": cdef.card_type.value,
        "copies": copies,
        "seen_games": len(seen),
        "seen_rate": ratio(len(seen), len(games)),
        "dut_loss_rate_when_seen": when_seen,
        "dut_loss_rate_when_not_seen": when_unseen,
        "loss_rate_lift": lift,
    }


def bench_card_stats(
    games: Sequence[GameRecord], deck: Deck, min_games: int
) -> list[dict[str, Any]]:
    """Benchmark cards ranked by how much more often the deck under test loses when the card
    was seen in play. Ranking needs ``min_games`` games both with and without the card."""
    copies = Counter(deck.main)
    entries = [_bench_entry(n, copies[n], games) for n in sorted(copies)]
    for e in entries:
        unseen = e["dut_loss_rate_when_not_seen"]["n"]
        e["rank_eligible"] = e["seen_games"] >= min_games and unseen >= min_games
    return sorted(
        entries,
        key=lambda e: (
            not e["rank_eligible"],
            -(e["loss_rate_lift"] if e["loss_rate_lift"] is not None else -2.0),
            -e["seen_games"],
            e["card_number"],
        ),
    )
