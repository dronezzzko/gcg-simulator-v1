"""Pre-game choices: who goes first (rule 6-2-1-4) and whether to redraw (rule 6-2-1-6)."""

from __future__ import annotations

import math
from collections import Counter
from collections.abc import Sequence

from gcg_sim.ai.base import Alternative
from gcg_sim.ai.playout import play_to_end
from gcg_sim.cards.model import CardDef, CardType
from gcg_sim.engine import view as V
from gcg_sim.engine.game import apply
from gcg_sim.engine.observe import determinize
from gcg_sim.engine.state import Action, GameState
from gcg_sim.engine.types import ActionKind, Zone
from gcg_sim.engine.view import link_satisfied
from gcg_sim.rng import SplitMix64

A = ActionKind
OPENING_TURNS = 4
KEEP_RATIO = 0.9
START_HAND = 5
FIRST_PLAYER_TEMPERATURE = 0.5
MAX_GAME_DECISIONS = 3000
SIGNIFICANCE = 2.0


def opening_score(hand: Sequence[CardDef], first: bool) -> float:
    """How much board a hand develops over the first turns if played out greedily.

    Each turn the player may spend (turn number) resources, plus Player Two's single EX
    Resource; a Unit or Pilot put down on turn ``t`` counts ``(OPENING_TURNS + 1 - t)`` times
    its stats, so early plays weigh most.
    """
    cards = sorted(hand, key=lambda c: (-c.cost, c.card_number))
    unpaired: list[CardDef] = []
    ex = 0 if first else 1
    score = 0.0
    for turn in range(1, OPENING_TURNS + 1):
        weight = OPENING_TURNS + 1 - turn
        budget = turn + ex
        spent = 0
        for c in [c for c in cards if c.card_type is CardType.UNIT]:
            if c.level <= turn + ex and c.cost <= budget - spent:
                spent += c.cost
                cards.remove(c)
                unpaired.append(c)
                score += weight * 0.5 * (c.ap + c.hp)
        for c in [c for c in cards if c.card_type is CardType.PILOT]:
            if unpaired and c.level <= turn + ex and c.cost <= budget - spent:
                unit = next((u for u in unpaired if link_satisfied(u, c)), unpaired[0])
                unpaired.remove(unit)
                spent += c.cost
                cards.remove(c)
                score += weight * (0.5 * (c.ap + c.hp) + (1.0 if link_satisfied(unit, c) else 0.0))
        if spent > turn:
            ex = 0
    return score


def mulligan_choice(
    st: GameState, player: int, samples: int, rng: SplitMix64
) -> tuple[Action, list[Alternative]]:
    """Redraw when the hand develops clearly less board than a fresh five from the rest of
    the deck would on average (the returned hand goes to the bottom, rule 6-2-1-6-1).
    Alternatives carry the opening scores: the hand's, and the replacement average."""
    R = V.reg()
    hand_ids = [st.cards[u].def_id for u in st.zones[player][Zone.HAND]]
    first = st.first_player == player
    current = opening_score([R.db.by_id(i) for i in hand_ids], first)
    rest = Counter(st.decklists[player])
    rest.subtract(hand_ids)
    pool = sorted(rest.elements())
    total = 0.0
    for _ in range(samples):
        rng.shuffle(pool)
        total += opening_score([R.db.by_id(i) for i in pool[:START_HAND]], first)
    average = total / samples if samples else 0.0
    keep, redraw = Action(A.KEEP), Action(A.REDRAW)
    stats = [Alternative(keep, current, 1), Alternative(redraw, KEEP_RATIO * average, samples)]
    return (keep if current >= KEEP_RATIO * average else redraw), stats


def first_player_choice(
    st: GameState, player: int, games: int, rng: SplitMix64
) -> tuple[Action, list[Alternative]]:
    """Play policy-vs-policy games from both choices on the same determinizations (common
    random numbers) and go second only if that is significantly better; otherwise go first.
    Alternatives carry each choice's score over the simulated games."""
    first = Action(A.GO_FIRST, player)
    second = Action(A.GO_FIRST, 1 - player)
    seeds = [rng.next_u64() for _ in range(games)]
    as_first = [_result(st, player, first, s) for s in seeds]
    as_second = [_result(st, player, second, s) for s in seeds]
    stats = [
        Alternative(first, sum(as_first) / max(1, games), games),
        Alternative(second, sum(as_second) / max(1, games), games),
    ]
    diffs = [a - b for a, b in zip(as_first, as_second, strict=True)]
    if len(diffs) < 2:
        return first, stats
    mean = sum(diffs) / len(diffs)
    var = sum((x - mean) ** 2 for x in diffs) / (len(diffs) - 1)
    se = math.sqrt(var / len(diffs))
    return (second if mean < 0 and -mean > SIGNIFICANCE * se else first), stats


def _result(st: GameState, player: int, choice: Action, seed: int) -> float:
    det = determinize(st, player, seed)
    apply(det, choice)
    play_to_end(det, SplitMix64(seed), FIRST_PLAYER_TEMPERATURE, MAX_GAME_DECISIONS)
    if det.winner == player:
        return 1.0
    return 0.5 if det.winner == -1 or det.winner is None else 0.0
