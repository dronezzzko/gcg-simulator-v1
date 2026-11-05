"""Executed-program census of the fuzz games (docs/port/WEAKNESSES.md W-151, W-117).

Plays the random-agent fuzz games of ``gcg_sim.tools.fuzz`` (the same decks, game seeds, agent
draws and invariant checks as ``play_one``) under the executed-program census of
``port/oracle/census.py``: a card's program "ran" when a frame of one of its programs was
pushed, one of its triggers resolved, or one of its constant abilities applied in a view. Per
card number it records

- ``decks``: the games in which a deck held the card (deck membership, what the fuzz summary's
  ``cards_covered`` counts),
- ``ran``: the games in which one of its programs ran,
- ``created``: the instances that effects or the setup procedure created (cards outside both
  decklists: Unit tokens, EX Bases, EX Resources).

The Rust fuzz plays the same games (``gcg-diff fuzz --games 10000 --seed 2026 --compare``
checks the summary and every per-game row against Python's), so this census is the Rust run's
too; ``gcg-diff coverage --check`` reads the committed census and reports, per pool card,
whether its program ran, every program card that never ran with the tagged test covering it,
and every effect-only number with its creation count.

    uv run python scripts/fuzz_census.py --games 10000 --seed 2026 --workers 8 \
        --out docs/port/eval/P6-N/fuzz-census-2026-10000.json
    uv run python scripts/fuzz_census.py --games 10000 --seed 2026 --workers 8 \
        --check docs/port/eval/P6-N/fuzz-census-2026-10000.json
    uv run python scripts/fuzz_census.py --selftest
"""

from __future__ import annotations

import argparse
import json
import sys
from collections import Counter
from concurrent.futures import ProcessPoolExecutor
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "port" / "oracle"))

import census as census_mod  # noqa: E402  (port/oracle, after the path insert)

from gcg_sim.effects.registry import get_registry  # noqa: E402
from gcg_sim.engine.game import apply, new_game  # noqa: E402
from gcg_sim.engine.invariants import check, initial_multiset  # noqa: E402
from gcg_sim.engine.types import EndReason, Zone  # noqa: E402
from gcg_sim.rng import SplitMix64, derive_seed  # noqa: E402
from gcg_sim.tools import fuzz  # noqa: E402

FORMAT = 1
_CENSUS: census_mod.Census | None = None


def _census() -> census_mod.Census:
    global _CENSUS  # noqa: PLW0603 (one census per worker process)
    if _CENSUS is None:
        _CENSUS = census_mod.Census(get_registry())
        _CENSUS.install()
    return _CENSUS


def play(index: int, seed: int) -> dict[str, Any]:
    """``fuzz.play_one(index, seed)`` with the census: the game's per-card evidence."""
    cen = _census()
    cen.ran.clear()
    d0 = fuzz.deck_for(2 * index, seed)
    d1 = fuzz.deck_for(2 * index + 1, seed)
    game_seed = derive_seed(seed, "fuzz-game", index)
    rng = SplitMix64(derive_seed(seed, "fuzz-agent", index))
    in_decks = set(d0.main) | set(d1.main) | set(d0.resources) | set(d1.resources)
    ok = True
    st = new_game((d0, d1), game_seed)
    init = initial_multiset(st)
    try:
        while st.winner is None:
            opts = st.pending.options if st.pending else ()
            apply(st, opts[rng.randrange(len(opts))])
            check(st, init)
        check(st, init)
        ok = st.end_reason is not EndReason.TURN_LIMIT
    except Exception:  # a failing game counts as a fuzz failure, as in play_one
        ok = False
    created = Counter(n for n in (cen.number(c.def_id) for c in st.cards) if n not in in_decks)
    # Numbers none of whose copies ever left the deck: still in the deck, never moved.
    moved = {cen.number(c.def_id) for c in st.cards if c.zone is not Zone.DECK or c.zone_seq}
    never_left = sorted(in_decks - moved)
    return {
        "index": index,
        "ok": ok,
        "decks": sorted(in_decks),
        "ran": sorted(cen.ran),
        "created": dict(sorted(created.items())),
        "never_left_deck": never_left,
    }


def _chunk(args: tuple[int, int, int]) -> dict[str, Any]:
    start, stop, seed = args
    decks: Counter[str] = Counter()
    ran: Counter[str] = Counter()
    created: Counter[str] = Counter()
    failures = 0
    for i in range(start, stop):
        g = play(i, seed)
        decks.update(g["decks"])
        ran.update(g["ran"])
        created.update(g["created"])
        failures += not g["ok"]
    return {"decks": decks, "ran": ran, "created": created, "failures": failures}


def run(games: int, seed: int, workers: int, chunk: int = 50) -> dict[str, Any]:
    tasks = [(s, min(s + chunk, games), seed) for s in range(0, games, chunk)]
    if workers <= 1:
        parts = [_chunk(t) for t in tasks]
    else:
        with ProcessPoolExecutor(max_workers=workers) as ex:
            parts = list(ex.map(_chunk, tasks))
    decks: Counter[str] = Counter()
    ran: Counter[str] = Counter()
    created: Counter[str] = Counter()
    for p in parts:
        decks.update(p["decks"])
        ran.update(p["ran"])
        created.update(p["created"])
    reg = get_registry()
    pool = sorted(
        {n for ns in fuzz.main_deck_pool().values() for n in ns} | set(fuzz.resource_pool())
    )
    real = sorted(c.card_number for c in reg.db.real_cards())
    cards = {n: [decks[n], ran[n], created[n]] for n in real if decks[n] or ran[n] or created[n]}
    return {
        "format": FORMAT,
        "generator": "scripts/fuzz_census.py",
        "games": games,
        "seed": seed,
        "failures": sum(p["failures"] for p in parts),
        "pool": pool,
        "columns": ["decks", "ran", "created"],
        "cards": cards,
    }


def dumps(doc: dict[str, Any]) -> str:
    """One card per line, so a diff of the committed file is readable."""
    head = {k: v for k, v in doc.items() if k not in ("pool", "cards")}
    lines = ["{"]
    for k, v in head.items():
        lines.append(f"  {json.dumps(k)}: {json.dumps(v)},")
    lines.append(f'  "pool": {json.dumps(doc["pool"])},')
    lines.append('  "cards": {')
    items = list(doc["cards"].items())
    for i, (n, row) in enumerate(items):
        lines.append(f"    {json.dumps(n)}: {json.dumps(row)}{',' if i + 1 < len(items) else ''}")
    lines.append("  }")
    lines.append("}")
    return "\n".join(lines) + "\n"


def selftest() -> int:
    """W-151's unit check on real games: a card that is in a deck but never leaves it does not
    count as executed, and every card whose program ran was in a deck or was created."""
    problems = []
    seen_never_left = 0
    for index in range(3):
        g = play(index, 2026)
        for n in g["never_left_deck"]:
            if n in g["ran"]:
                problems.append(f"game {index}: {n} never left the deck but counts as executed")
            seen_never_left += 1
        stray = set(g["ran"]) - set(g["decks"]) - set(g["created"])
        if stray:
            problems.append(f"game {index}: ran without being in a deck or created: {stray}")
    if seen_never_left == 0:
        problems.append("no game had a deck card that never left the deck")
    for p in problems:
        print(f"selftest: {p}", file=sys.stderr)
    print(
        f"fuzz_census selftest {'failed' if problems else 'passed'} "
        f"({seen_never_left} deck cards that never left the deck, none executed)"
    )
    return 1 if problems else 0


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=(__doc__ or "").splitlines()[0])
    ap.add_argument("--games", type=int, default=10000)
    ap.add_argument("--seed", type=int, default=2026)
    ap.add_argument("--workers", type=int, default=1)
    out = ap.add_mutually_exclusive_group(required=True)
    out.add_argument("--out", type=Path, help="write the census here")
    out.add_argument("--check", type=Path, help="fail unless the census equals this file")
    out.add_argument("--selftest", action="store_true")
    args = ap.parse_args(argv)
    if args.selftest:
        return selftest()
    text = dumps(run(args.games, args.seed, args.workers))
    if args.out:
        args.out.parent.mkdir(parents=True, exist_ok=True)
        args.out.write_text(text, encoding="utf-8")
        print(f"wrote {args.out}")
        return 0
    if args.check.read_text(encoding="utf-8") != text:
        print(f"{args.check} is stale: regenerate it with --out", file=sys.stderr)
        return 1
    print(f"{args.check} is current")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
