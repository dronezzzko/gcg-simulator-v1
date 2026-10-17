"""Preset calibration experiments for the MCTS player (docs/AI.md, "Calibrating the presets").

``match`` plays mirrored blocks of four games (``gcg_sim.ai.selfplay.mirrored_specs``) between
two agents on one or more deck pairings and prints the challenger's wins with a Wilson 95%
interval. ``paired`` compares two BO1 ``gcg-sim benchmark`` runs that used the same decks, seed
and match count but different presets: the difference in the deck under test's game win rate,
with a 95% interval for the paired per-match differences.

An agent is ``random``, ``greedy``, ``mcts:<preset>``, or ``mcts:<preset>:<field>=<value>,...``
(the preset's ``SearchConfig`` with fields replaced, for screening candidates). The config
``name`` seeds the agent's decisions, so give a candidate its own ``name=<label>``.

Pairings: ``tuning`` (the three pairs of ``TUNING_DECKS``), ``test`` (``tests/ai/decks.py``),
``example`` (the three pairs of ``examples/decks``), or one of their members such as
``tuning:0-1``, ``test:federation-seed`` or ``example:blue-white-federation-red-green-zeon``.
Pairing ``p`` uses the master seed ``derive_seed(--seed, "pairing", p)``, so its games do not
depend on which other pairings are listed, and no result depends on ``--workers``.

    uv run python scripts/ai_calibrate.py match --challenger mcts:expert \
        --opponent mcts:strong --pairings test example --games 48 --seed 9001 --workers 12
    uv run python scripts/ai_calibrate.py paired RUN_A RUN_B
"""

from __future__ import annotations

import argparse
import dataclasses
import itertools
import json
import math
import sys
import time
from collections.abc import Sequence
from concurrent.futures import ProcessPoolExecutor
from multiprocessing import get_context
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

from gcg_sim.ai.agents import MctsAgent, make_agent  # noqa: E402
from gcg_sim.ai.base import Agent  # noqa: E402
from gcg_sim.ai.config import PRESETS, SearchConfig  # noqa: E402
from gcg_sim.ai.selfplay import (  # noqa: E402
    MIRROR_BLOCK,
    AgentSpec,
    GameOutcome,
    GameSpec,
    mirrored_specs,
    run_game,
)
from gcg_sim.ai.tuning import TUNING_DECKS  # noqa: E402
from gcg_sim.deck import load_deck, to_decklist  # noqa: E402
from gcg_sim.engine.game import DeckList  # noqa: E402
from gcg_sim.reports.stats import wilson  # noqa: E402
from gcg_sim.rng import derive_seed  # noqa: E402

Z95 = 1.96
TUNING_NAMES = ("zeon", "wing", "tekkadan")


def _pairings() -> dict[str, tuple[DeckList, DeckList]]:
    from tests.ai.decks import DECKS

    out: dict[str, tuple[DeckList, DeckList]] = {}
    for a, b in itertools.combinations(range(len(TUNING_DECKS)), 2):
        out[f"tuning:{a}-{b}"] = (TUNING_DECKS[a], TUNING_DECKS[b])
    out["test:federation-seed"] = DECKS
    examples = sorted((ROOT / "examples" / "decks").glob("*.txt"))
    for pa, pb in itertools.combinations(examples, 2):
        decks = (to_decklist(load_deck(pa)), to_decklist(load_deck(pb)))
        out[f"example:{pa.stem}-{pb.stem}"] = decks
    return out


def _select_pairings(names: Sequence[str]) -> list[str]:
    known = _pairings()
    chosen: list[str] = []
    for name in names:
        matches = [k for k in known if k == name or k.split(":")[0] == name]
        if not matches:
            raise SystemExit(f"unknown pairing {name!r}; choose from {sorted(known)}")
        chosen += [m for m in matches if m not in chosen]
    return chosen


def parse_agent(text: str) -> tuple[str, SearchConfig | None]:
    """``random`` | ``greedy`` | ``mcts:<preset>[:field=value,...]``."""
    kind, _, rest = text.partition(":")
    if kind in ("random", "greedy") and not rest:
        return kind, None
    if kind != "mcts" or not rest:
        raise ValueError(f"bad agent {text!r}")
    preset, _, overrides = rest.partition(":")
    if preset not in PRESETS:
        raise ValueError(f"unknown preset {preset!r}; choose from {sorted(PRESETS)}")
    config = PRESETS[preset]
    if not overrides:
        return kind, None
    types = {f.name: f.type for f in dataclasses.fields(SearchConfig)}
    changes: dict[str, Any] = {}
    for item in overrides.split(","):
        field, _, value = item.partition("=")
        if field not in types:
            raise ValueError(f"unknown SearchConfig field {field!r}")
        convert = {"int": int, "float": float, "str": str}[str(types[field])]
        changes[field] = convert(value)
    if "name" not in changes:
        raise ValueError(f"a modified preset needs its own name=<label>: {text!r}")
    return kind, dataclasses.replace(config, **changes)


def build_agent(text: str, seed: int) -> Agent:
    kind, config = parse_agent(text)
    if config is not None:
        return MctsAgent(seed, config=config)
    preset = text.split(":")[1] if kind == "mcts" else "standard"
    return make_agent(kind, preset=preset, seed=seed)


def _label(spec: AgentSpec) -> str:
    return spec.kind if spec.kind != "mcts" else f"mcts:{spec.preset}"


def play_one(item: tuple[str, GameSpec]) -> tuple[str, GameOutcome]:
    pairing, spec = item
    a0, a1 = (build_agent(_label(a), s) for a, s in zip(spec.agents, spec.agent_seeds, strict=True))
    st, decisions = run_game((a0, a1), spec.decks, spec.game_seed)
    winner = -1 if st.winner is None else st.winner
    return pairing, GameOutcome(
        spec.index, spec.challenger, winner, st.turn, st.first_player, decisions
    )


def _agent_spec(text: str) -> AgentSpec:
    parse_agent(text)
    kind, _, rest = text.partition(":")
    return AgentSpec(kind, rest or "standard")


def first_deck_seat(index: int) -> int:
    """Seat of the pairing's first deck in game ``index`` of ``mirrored_specs``."""
    swap_decks = (index % MIRROR_BLOCK) // 2
    return 1 if swap_decks else 0


def _summary(outcomes: Sequence[GameOutcome]) -> dict[str, Any]:
    n = len(outcomes)
    wins = sum(o.winner == o.challenger for o in outcomes)
    draws = sum(o.winner == -1 for o in outcomes)
    first_deck_wins = sum(o.winner == first_deck_seat(o.index) for o in outcomes)
    lo, hi = wilson(wins, n)
    return {
        "games": n,
        "challenger_wins": wins,
        "draws": draws,
        "win_rate": round(wins / n, 4) if n else None,
        "wilson95": [round(lo, 4), round(hi, 4)],
        "first_deck_wins": first_deck_wins,
        "mean_turns": round(sum(o.turns for o in outcomes) / n, 2) if n else None,
    }


def cmd_match(args: argparse.Namespace) -> dict[str, Any]:
    if args.games <= 0 or args.games % 4:
        raise SystemExit("--games must be a positive multiple of 4 (mirrored blocks)")
    challenger, opponent = _agent_spec(args.challenger), _agent_spec(args.opponent)
    decks = _pairings()
    items: list[tuple[str, GameSpec]] = []
    for name in _select_pairings(args.pairings):
        seed = derive_seed(args.seed, "pairing", name)
        specs = mirrored_specs(args.games, seed, challenger, opponent, decks[name])
        items += [(name, s) for s in specs]
    workers = max(1, min(args.workers, len(items)))
    start = time.perf_counter()
    if workers == 1:
        results = [play_one(i) for i in items]
    else:
        with ProcessPoolExecutor(max_workers=workers, mp_context=get_context("spawn")) as pool:
            results = list(pool.map(play_one, items, chunksize=1))
    wall = time.perf_counter() - start
    by_pairing: dict[str, list[GameOutcome]] = {}
    for name, outcome in results:
        by_pairing.setdefault(name, []).append(outcome)
    report = {
        "command": "match",
        "challenger": args.challenger,
        "opponent": args.opponent,
        "seed": args.seed,
        "games_per_pairing": args.games,
        "total": _summary([o for _, o in results]),
        "pairings": {k: _summary(v) for k, v in by_pairing.items()},
        "timing": {
            "workers": workers,
            "wall_seconds": round(wall, 1),
            "core_seconds_per_game": round(wall * workers / len(results), 2),
        },
    }
    if args.out:
        out = Path(args.out)
        out.mkdir(parents=True, exist_ok=True)
        with (out / "games.ndjson").open("w", encoding="utf-8") as fh:
            for name, o in results:
                fh.write(json.dumps({"pairing": name, **dataclasses.asdict(o)}) + "\n")
        (out / "summary.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    return report


def _bo1_dut_wins(run: Path) -> tuple[dict[str, Any], dict[int, int]]:
    results = json.loads((run / "results.json").read_text(encoding="utf-8"))
    if results["config"]["format"] != "bo1":
        raise SystemExit(f"{run}: paired comparison needs a BO1 run")
    wins: dict[int, int] = {}
    with (run / "games.ndjson").open(encoding="utf-8") as fh:
        for line in fh:
            game = json.loads(line)
            wins[game["match_index"]] = int(game["winner"] == "dut")
    return results, wins


def cmd_paired(args: argparse.Namespace) -> dict[str, Any]:
    """Δ = A − B in the deck under test's BO1 game win rate, paired by match index."""
    ra, wa = _bo1_dut_wins(Path(args.run_a))
    rb, wb = _bo1_dut_wins(Path(args.run_b))
    for key in ("seeds", "decks"):
        if ra[key] != rb[key]:
            raise SystemExit(f"runs differ in {key!r}: not a paired comparison")
    if wa.keys() != wb.keys():
        raise SystemExit("runs played different matches")
    diffs = [wa[m] - wb[m] for m in sorted(wa)]
    n = len(diffs)
    mean = sum(diffs) / n
    var = sum((d - mean) ** 2 for d in diffs) / (n - 1) if n > 1 else 0.0
    half = Z95 * math.sqrt(var / n)
    return {
        "command": "paired",
        "run_a": str(args.run_a),
        "preset_a": ra["ai"]["preset"],
        "run_b": str(args.run_b),
        "preset_b": rb["ai"]["preset"],
        "games": n,
        "dut_wins_a": sum(wa.values()),
        "dut_wins_b": sum(wb.values()),
        "rate_a": round(sum(wa.values()) / n, 4),
        "rate_b": round(sum(wb.values()) / n, 4),
        "delta": round(mean, 4),
        "delta_ci95": [round(mean - half, 4), round(mean + half, 4)],
        "discordant": {"a_only": diffs.count(1), "b_only": diffs.count(-1)},
    }


def main(argv: Sequence[str] | None = None) -> None:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = ap.add_subparsers(dest="command", required=True)
    m = sub.add_parser("match", help="mirrored head-to-head games")
    m.add_argument("--challenger", required=True)
    m.add_argument("--opponent", required=True)
    m.add_argument("--pairings", nargs="+", default=["tuning"])
    m.add_argument(
        "--games", type=int, default=8, help="games per pairing (positive multiple of 4)"
    )
    m.add_argument("--seed", type=int, default=0)
    m.add_argument("--workers", type=int, default=1)
    m.add_argument("--out", help="directory for games.ndjson and summary.json")
    p = sub.add_parser("paired", help="paired Δ of two BO1 benchmark runs")
    p.add_argument("run_a")
    p.add_argument("run_b")
    args = ap.parse_args(argv)
    report = cmd_match(args) if args.command == "match" else cmd_paired(args)
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
