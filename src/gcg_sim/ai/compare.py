"""Head-to-head comparison of two AI versions in mirrored blocks of four games.

Every game's seeds derive from ``--seed`` and the game index, so a comparison is repeatable bit
for bit for any ``--workers``. Draws count as non-wins for the challenger, as in the benchmark
reports, and the win rate carries a Wilson 95% interval.

An agent is ``random``, ``greedy``, a preset (``standard``, ``strong``, ...), or a baseline
preset (``baseline-standard``: the AI as released before the 2026-10 changes, see
:mod:`gcg_sim.ai.baseline`), optionally followed by ``:field=value,...`` to replace
``SearchConfig`` fields, or ``weights=<name>`` (``default``, ``hand``, ``baseline``, ``roles``)
to replace the evaluation weights. The configuration's ``name`` seeds the agent's decisions, so
variants of one preset keep the same per-decision seeds.

Decks: ``tests`` (``tests/ai/decks.py``, run from the repository root), ``tuning`` (the three
tuning decks), ``examples`` (``examples/decks/*.txt``), a comma-separated list of deck files,
or ``module:attr`` naming a sequence of ``DeckList``. With more than two decks, every pair is
played: pairing ``p`` gets ``games / pairs`` games on the master seed
``derive_seed(--seed, "pairing", p)``; a single pairing uses ``--seed`` itself.

Run: ``uv run python -m gcg_sim.ai.compare standard baseline-standard --games 400 --seed 1``
"""

from __future__ import annotations

import argparse
import dataclasses
import importlib
import itertools
import json
from collections.abc import Callable, Sequence
from pathlib import Path
from typing import Any

from gcg_sim.ai.baseline import BASELINE_PRESETS, BASELINE_WEIGHTS
from gcg_sim.ai.config import PRESETS, SearchConfig
from gcg_sim.ai.evaluation import DEFAULT_WEIGHTS, HAND_WEIGHTS, ROLE_WEIGHTS, Weights
from gcg_sim.ai.selfplay import (
    MIRROR_BLOCK,
    AgentSpec,
    GameOutcome,
    GameSpec,
    mirrored_specs,
    play_all,
)
from gcg_sim.deck import load_deck, to_decklist
from gcg_sim.engine.game import DeckList
from gcg_sim.reports.stats import wilson
from gcg_sim.rng import derive_seed

BASELINE_PREFIX = "baseline-"
BASELINES = ("random", "greedy")
WEIGHTS: dict[str, Weights] = {
    "default": DEFAULT_WEIGHTS,
    "hand": HAND_WEIGHTS,
    "baseline": BASELINE_WEIGHTS,
    "roles": ROLE_WEIGHTS,
}
DECK_SETS = {"tests": "tests.ai.decks:DECKS", "tuning": "gcg_sim.ai.tuning:TUNING_DECKS"}
EXAMPLES = Path("examples") / "decks"


def _flag(text: str) -> bool:
    if text.lower() in ("1", "true", "yes"):
        return True
    if text.lower() in ("0", "false", "no"):
        return False
    raise ValueError(f"not a boolean: {text!r}")


_CONVERTERS: dict[str, Callable[[str], Any]] = {
    "int": int,
    "float": float,
    "str": str,
    "bool": _flag,
}
_FIELD_TYPES = {f.name: str(f.type) for f in dataclasses.fields(SearchConfig)}


def parse_agent(text: str) -> AgentSpec:
    """``random`` | ``greedy`` | ``<preset>`` | ``baseline-<preset>``, each optionally followed
    by ``:field=value,...`` (``SearchConfig`` fields, or ``weights=<name>``)."""
    head, _, overrides = text.partition(":")
    config: SearchConfig | None = None
    weights: Weights | None = None
    if head in BASELINES:
        kind = head
    elif head in PRESETS:
        kind, config = "mcts", PRESETS[head]
    elif head.removeprefix(BASELINE_PREFIX) in BASELINE_PRESETS and head.startswith(
        BASELINE_PREFIX
    ):
        kind = "mcts"
        config = BASELINE_PRESETS[head.removeprefix(BASELINE_PREFIX)]
        weights = BASELINE_WEIGHTS
    else:
        choices = [*BASELINES, *PRESETS, *(BASELINE_PREFIX + p for p in BASELINE_PRESETS)]
        raise ValueError(f"unknown agent {head!r}; choose from {choices}")
    changes: dict[str, Any] = {}
    for item in filter(None, overrides.split(",")):
        field, sep, value = item.partition("=")
        if not sep:
            raise ValueError(f"expected field=value, got {item!r}")
        if field == "weights":
            if value not in WEIGHTS or kind == "random":
                raise ValueError(f"weights must be one of {sorted(WEIGHTS)} for greedy or MCTS")
            weights = WEIGHTS[value]
        elif config is None or field not in _FIELD_TYPES:
            raise ValueError(f"{head!r} has no search setting {field!r}")
        else:
            changes[field] = _CONVERTERS[_FIELD_TYPES[field]](value)
    if config is not None and changes:
        config = dataclasses.replace(config, **changes)
    return AgentSpec(kind, config=config, weights=weights)


def load_decks(ref: str) -> tuple[DeckList, ...]:
    """``tests``, ``tuning``, ``examples``, ``a.txt,b.txt,...`` or ``module:attr``."""
    if ref == "examples":
        return tuple(to_decklist(load_deck(p)) for p in sorted(EXAMPLES.glob("*.txt")))
    ref = DECK_SETS.get(ref, ref)
    if ":" in ref and not ref.endswith(".txt"):
        module, _, attr = ref.partition(":")
        return tuple(getattr(importlib.import_module(module), attr))
    return tuple(to_decklist(load_deck(p)) for p in ref.split(","))


def comparison_specs(
    games: int, seed: int, challenger: AgentSpec, opponent: AgentSpec, decks: Sequence[DeckList]
) -> list[GameSpec]:
    """Mirrored blocks of four on every pair of ``decks`` (see the module docstring)."""
    pairs = list(itertools.combinations(decks, 2))
    if not pairs:
        raise ValueError("need at least two decks")
    if games <= 0 or games % (MIRROR_BLOCK * len(pairs)):
        raise ValueError(f"games must be a positive multiple of {MIRROR_BLOCK * len(pairs)}")
    if len(pairs) == 1:
        return mirrored_specs(games, seed, challenger, opponent, pairs[0])
    out: list[GameSpec] = []
    for p, pair in enumerate(pairs):
        master = derive_seed(seed, "pairing", p)
        for s in mirrored_specs(games // len(pairs), master, challenger, opponent, pair):
            out.append(dataclasses.replace(s, index=len(out)))
    return out


def _rate(wins: int, n: int) -> dict[str, Any]:
    lo, hi = wilson(wins, n)
    return {
        "wins": wins,
        "n": n,
        "rate": round(wins / n, 4) if n else None,
        "ci95": [round(lo, 4), round(hi, 4)],
    }


def summarize(specs: Sequence[GameSpec], outcomes: Sequence[GameOutcome]) -> dict[str, Any]:
    """The challenger's wins (draws are non-wins) overall, by seat and by the deck it played."""
    won = [o.winner == o.challenger for o in outcomes]
    by_seat = {
        str(seat): _rate(
            sum(w for w, o in zip(won, outcomes, strict=True) if o.challenger == seat),
            sum(1 for o in outcomes if o.challenger == seat),
        )
        for seat in (0, 1)
    }
    deck_names: dict[DeckList, str] = {}
    by_deck: dict[str, list[bool]] = {}
    for s, w in zip(specs, won, strict=True):
        deck = s.decks[s.challenger]
        name = deck_names.setdefault(deck, f"deck{len(deck_names)}")
        by_deck.setdefault(name, []).append(w)
    return {
        **_rate(sum(won), len(outcomes)),
        "draws": sum(1 for o in outcomes if o.winner == -1),
        "mean_turns": round(sum(o.turns for o in outcomes) / max(1, len(outcomes)), 2),
        "by_challenger_seat": by_seat,
        "by_challenger_deck": {k: _rate(sum(v), len(v)) for k, v in by_deck.items()},
    }


def compare(
    challenger: str, opponent: str, games: int, seed: int, workers: int, decks: str
) -> dict[str, Any]:
    specs = comparison_specs(
        games, seed, parse_agent(challenger), parse_agent(opponent), load_decks(decks)
    )
    outcomes = play_all(specs, workers)
    return {
        "challenger": challenger,
        "opponent": opponent,
        "decks": decks,
        "seed": seed,
        "games": games,
        "result": summarize(specs, outcomes),
        "outcomes": [[o.challenger, o.winner, o.turns, o.first_player] for o in outcomes],
    }


def main(argv: Sequence[str] | None = None) -> None:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("challenger")
    ap.add_argument("opponent")
    ap.add_argument("--games", type=int, default=400, help="a multiple of 4 per deck pairing")
    ap.add_argument("--seed", type=int, default=1)
    ap.add_argument("--workers", type=int, default=1)
    ap.add_argument("--decks", default="tests")
    ap.add_argument("--out", type=Path, help="also write the report, with every game, here")
    args = ap.parse_args(argv)
    report = compare(
        args.challenger, args.opponent, args.games, args.seed, args.workers, args.decks
    )
    if args.out is not None:
        args.out.write_text(json.dumps(report, indent=1) + "\n", encoding="utf-8")
    print(json.dumps({k: v for k, v in report.items() if k != "outcomes"}, indent=2))


if __name__ == "__main__":
    main()
