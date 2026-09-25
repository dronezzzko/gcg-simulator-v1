"""Tuning signals, stated as hypotheses with their sample sizes (never as conclusions).

Every signal is a correlation between a card event and the game result in AI-vs-AI games;
it suggests what to test next, not what caused a result.
"""

from __future__ import annotations

from collections.abc import Callable
from typing import Any

from gcg_sim.reports.stats import intervals_overlap, rnd

MIN_GAMES = 20
MIN_REDRAWS = 10
MIN_DELTA = 0.05
MIN_SPLIT_DELTA = 0.10
DEAD_CARD_RATE = 0.5
PER_DIRECTION = 3


def _pct(x: float | None) -> str:
    return "n/a" if x is None else f"{100 * x:.0f}%"


def _hypothesis(
    hid: str, kind: str, statement: str, sample: int, evidence: dict[str, Any]
) -> dict[str, Any]:
    return {
        "id": hid,
        "kind": kind,
        "label": "hypothesis",
        "statement": statement,
        "sample_size": sample,
        "evidence": evidence,
    }


def _compare(a: dict[str, Any], b: dict[str, Any]) -> dict[str, Any]:
    return {
        "a": a,
        "b": b,
        "delta": rnd(a["rate"] - b["rate"]),
        "ci95_separated": not intervals_overlap(a, b),
    }


def _card_delta(card: dict[str, Any]) -> dict[str, Any] | None:
    drawn, not_drawn = card["win_rate_when_drawn"], card["win_rate_when_not_drawn"]
    if drawn["n"] < MIN_GAMES or not_drawn["n"] < MIN_GAMES:
        return None
    ev = _compare(drawn, not_drawn)
    if abs(ev["delta"]) < MIN_DELTA:
        return None
    label = f"{card['name']} ({card['card_number']})"
    if ev["delta"] > 0:
        advice = "more copies or more ways to find it may raise the win rate"
    else:
        advice = "it may underperform in this matchup; test cutting a copy"
    statement = (
        f"Games where {label} was drawn were won {_pct(drawn['rate'])} of the time vs "
        f"{_pct(not_drawn['rate'])} when it was not (n={drawn['n']}/{not_drawn['n']}). "
        f"Hypothesis: {advice}. Correlation only: longer games also draw more cards."
    )
    return _hypothesis(
        f"card-drawn-delta:{card['card_number']}",
        "card_drawn_win_rate",
        statement,
        drawn["n"] + not_drawn["n"],
        ev,
    )


def _dead_card(card: dict[str, Any]) -> dict[str, Any] | None:
    held = card["held_never_played"]
    drawn = card["drawn"]["copies"]
    if drawn < MIN_GAMES or held["rate_of_drawn_copies"] is None:
        return None
    if held["rate_of_drawn_copies"] < DEAD_CARD_RATE:
        return None
    statement = (
        f"{_pct(held['rate_of_drawn_copies'])} of drawn copies of {card['name']} "
        f"({card['card_number']}) were never played (n={drawn} copies; "
        f"{held['in_hand_at_end_copies']} still in hand at game end). Hypothesis: it is often "
        "uncastable or low priority in this matchup; consider fewer copies or a lower curve."
    )
    return _hypothesis(
        f"dead-card:{card['card_number']}",
        "held_never_played",
        statement,
        drawn,
        {"drawn_copies": drawn, **held},
    )


def _top_by_delta(found: list[dict[str, Any]]) -> list[dict[str, Any]]:
    pos = sorted((h for h in found if h["evidence"]["delta"] > 0), key=_by_delta)
    neg = sorted((h for h in found if h["evidence"]["delta"] < 0), key=_by_delta)
    return pos[:PER_DIRECTION] + neg[:PER_DIRECTION]


def _by_delta(h: dict[str, Any]) -> tuple[float, str]:
    return (-abs(h["evidence"]["delta"]), str(h["id"]))


def _play_draw(split: dict[str, Any]) -> dict[str, Any] | None:
    play, draw = split["on_play"]["dut_win_rate"], split["on_draw"]["dut_win_rate"]
    if play["n"] < MIN_GAMES or draw["n"] < MIN_GAMES:
        return None
    ev = _compare(play, draw)
    if abs(ev["delta"]) < MIN_SPLIT_DELTA:
        return None
    better = "going first" if ev["delta"] > 0 else "going second"
    statement = (
        f"The deck under test wins {_pct(play['rate'])} on the play vs {_pct(draw['rate'])} on "
        f"the draw (n={play['n']}/{draw['n']}). Hypothesis: it is built for {better}; its "
        "curve and early interaction are the first things to test."
    )
    return _hypothesis("play-draw", "play_draw_split", statement, play["n"] + draw["n"], ev)


def _redraw(mull: dict[str, Any]) -> dict[str, Any] | None:
    after_redraw, after_keep = mull["win_rate_after_redraw"], mull["win_rate_after_keep"]
    if after_redraw["n"] < MIN_REDRAWS or after_keep["n"] < MIN_REDRAWS:
        return None
    ev = _compare(after_redraw, after_keep)
    if abs(ev["delta"]) < MIN_SPLIT_DELTA:
        return None
    statement = (
        f"After a redraw the deck under test won {_pct(after_redraw['rate'])} vs "
        f"{_pct(after_keep['rate'])} after keeping (n={after_redraw['n']}/{after_keep['n']}). "
        "Hypothesis: opening-hand consistency matters here; the early curve may need smoothing."
    )
    sample = after_redraw["n"] + after_keep["n"]
    return _hypothesis("redraw", "redraw_outcome", statement, sample, ev)


def _bench_threat(card: dict[str, Any]) -> dict[str, Any] | None:
    lift = card["loss_rate_lift"]
    if not card["rank_eligible"] or lift is None or lift < MIN_SPLIT_DELTA:
        return None
    seen, unseen = card["dut_loss_rate_when_seen"], card["dut_loss_rate_when_not_seen"]
    statement = (
        f"The deck under test lost {_pct(seen['rate'])} of games where the benchmark's "
        f"{card['name']} ({card['card_number']}) was seen in play vs {_pct(unseen['rate'])} "
        f"otherwise (n={seen['n']}/{unseen['n']}). Hypothesis: answers to it (removal, "
        "blockers, or racing it) may be worth testing."
    )
    return _hypothesis(
        f"bench-threat:{card['card_number']}",
        "benchmark_card_threat",
        statement,
        seen["n"] + unseen["n"],
        _compare(seen, unseen),
    )


def _collect(
    items: list[dict[str, Any]], make: Callable[[dict[str, Any]], dict[str, Any] | None]
) -> list[dict[str, Any]]:
    return [h for h in (make(i) for i in items) if h is not None]


def hypotheses(
    cards: list[dict[str, Any]],
    bench_cards: list[dict[str, Any]],
    split: dict[str, Any],
    mull: dict[str, Any],
) -> list[dict[str, Any]]:
    out = _top_by_delta(_collect(cards, _card_delta))
    out += _collect(cards, _dead_card)
    out += [h for h in (_play_draw(split), _redraw(mull["dut"])) if h is not None]
    out += _collect(bench_cards, _bench_threat)[:PER_DIRECTION]
    return out
