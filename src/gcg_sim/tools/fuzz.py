"""Random-agent robustness fuzzing (acceptance criterion 6).

Generates decks that together include every implemented main-deck card, plays games with
uniformly random legal actions, and checks state invariants after every action.

    uv run python -m gcg_sim.tools.fuzz --games 10000 --workers 8
"""

from __future__ import annotations

import argparse
import json
import sys
import traceback
from collections import Counter
from concurrent.futures import ProcessPoolExecutor
from dataclasses import dataclass, field
from typing import Any

from gcg_sim.cards.model import MAIN_DECK_TYPES, CardType, Color
from gcg_sim.effects.registry import get_registry
from gcg_sim.engine.game import DeckList, apply, new_game
from gcg_sim.engine.invariants import InvariantViolation, check, initial_multiset
from gcg_sim.engine.types import EndReason
from gcg_sim.rng import SplitMix64, derive_seed


def resource_pool() -> list[str]:
    reg = get_registry()
    return sorted(c.card_number for c in reg.db.real_cards() if c.card_type is CardType.RESOURCE)


def main_deck_pool() -> dict[Color, list[str]]:
    reg = get_registry()
    pool: dict[Color, list[str]] = {c: [] for c in Color}
    for cdef in reg.db.real_cards():
        if (
            cdef.card_type in MAIN_DECK_TYPES
            and cdef.color is not None
            and reg.cards[cdef.def_id].script is not None
        ):
            pool[cdef.color].append(cdef.card_number)
    return {c: sorted(ns) for c, ns in pool.items()}


def deck_for(index: int, seed: int) -> DeckList:
    """Deck ``index`` covers a rotating slice of one or two colours' card pools so that
    consecutive decks jointly include every implemented card."""
    pool = main_deck_pool()
    colors = sorted(pool, key=lambda c: c.value)
    rng = SplitMix64(derive_seed(seed, "fuzz-deck", index))
    c1 = colors[index % len(colors)]
    c2 = colors[(index // len(colors) + index) % len(colors)]
    cards = pool[c1] + (pool[c2] if c2 is not c1 else [])
    start = (index // len(colors)) * 13 % max(1, len(cards))
    rotated = cards[start:] + cards[:start]
    main: list[str] = []
    for n in rotated:
        if len(main) >= 50:
            break
        main += [n] * min(1 + rng.randrange(4), 50 - len(main))
    i = 0
    while len(main) < 50:
        n = rotated[i % len(rotated)]
        if main.count(n) < 4:
            main.append(n)
        i += 1
    res = resource_pool()
    r0 = (index * 10) % len(res)
    resources = tuple((res[r0:] + res[:r0])[:10])
    return DeckList(tuple(main), resources)


@dataclass
class FuzzResult:
    games: int = 0
    actions: int = 0
    ends: Counter[str] = field(default_factory=Counter)
    failures: list[dict[str, Any]] = field(default_factory=list)
    cards_seen: set[str] = field(default_factory=set)


def play_one(index: int, seed: int, check_every: int = 1) -> dict[str, Any]:
    d0 = deck_for(2 * index, seed)
    d1 = deck_for(2 * index + 1, seed)
    game_seed = derive_seed(seed, "fuzz-game", index)
    rng = SplitMix64(derive_seed(seed, "fuzz-agent", index))
    cards = set(d0.main) | set(d1.main) | set(d0.resources) | set(d1.resources)
    out: dict[str, Any] = {"index": index, "seed": game_seed, "cards": sorted(cards)}
    actions: list[list[int | str]] = []
    try:
        st = new_game((d0, d1), game_seed)
        init = initial_multiset(st)
        n = 0
        while st.winner is None:
            opts = st.pending.options if st.pending else ()
            a = opts[rng.randrange(len(opts))]
            actions.append(a.to_json())
            apply(st, a)
            n += 1
            if n % check_every == 0:
                check(st, init)
        check(st, init)
        out.update(
            ok=True, actions=n, end=st.end_reason.value if st.end_reason else "none", turns=st.turn
        )
        if st.end_reason is EndReason.TURN_LIMIT:
            out.update(
                ok=False, error="game ended by the engine safety cap (not a rules-defined end)"
            )
    except (InvariantViolation, Exception) as exc:
        out.update(
            ok=False,
            error=f"{type(exc).__name__}: {exc}",
            trace=traceback.format_exc(limit=8),
            actions_so_far=len(actions),
            last_actions=actions[-5:],
        )
    return out


def run(games: int, seed: int, workers: int) -> FuzzResult:
    res = FuzzResult()
    indices = list(range(games))
    if workers <= 1:
        outs = [play_one(i, seed) for i in indices]
    else:
        with ProcessPoolExecutor(max_workers=workers) as ex:
            outs = list(ex.map(play_one, indices, [seed] * games, chunksize=16))
    for o in outs:
        res.games += 1
        res.cards_seen.update(o["cards"])
        if o.get("ok"):
            res.actions += int(o["actions"])
            res.ends[str(o["end"])] += 1
        else:
            res.failures.append(o)
    return res


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--games", type=int, default=1000)
    ap.add_argument("--seed", type=int, default=1)
    ap.add_argument("--workers", type=int, default=1)
    ap.add_argument("--show", type=int, default=5)
    args = ap.parse_args(argv)
    res = run(args.games, args.seed, args.workers)
    pool = {n for ns in main_deck_pool().values() for n in ns} | set(resource_pool())
    summary = {
        "games": res.games,
        "actions": res.actions,
        "ends": dict(res.ends),
        "failures": len(res.failures),
        "cards_covered": len(res.cards_seen & pool),
        "cards_in_pool": len(pool),
        "cards_missing": sorted(pool - res.cards_seen)[:20],
    }
    print(json.dumps(summary, indent=1))
    for f in res.failures[: args.show]:
        print(
            json.dumps({k: f.get(k) for k in ("index", "seed", "error", "last_actions")}),
            file=sys.stderr,
        )
        print(f.get("trace", ""), file=sys.stderr)
    return 1 if res.failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
