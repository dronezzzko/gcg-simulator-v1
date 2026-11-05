"""Export the statistics, seeding and agent-spec vectors of ``gcg-eval`` (docs/dev/PLAN.md
§10.4 F, T-079, T-080).

Writes ``crates/gcg-eval/tests/vectors/stats.json`` from the Python reference:

- ``deck_sets``: the card numbers of the ``tests``, ``tuning`` and ``examples`` decks, which
  the embedded deck files and the example loader must reproduce;
- ``rates``: ``ai/compare.py`` ``_rate(wins, n)`` (Wilson 95% interval, rounded to 4 places) as
  ``json.dumps`` text;
- ``mirrored_specs``: ``ai/selfplay.py`` ``mirrored_specs`` tables (index, game seed, agent seeds,
  challenger seat, which agent and which deck sits in each seat);
- ``comparison_specs``: ``ai/compare.py`` ``comparison_specs`` tables over the ``tests``,
  ``tuning`` and ``examples`` deck sets and over a deck list with a repeated deck, and the
  ``ValueError`` texts of its two checks;
- ``summarize``: ``summarize(specs, outcomes)`` as ``json.dumps(indent=2)`` text, over synthetic
  outcomes (``fake_outcome`` below, which the Rust test reimplements);
- ``compare_cli``: the exact stdout and ``--out`` bytes of ``python -m gcg_sim.ai.compare``
  (``compare.main``) with ``play_all`` replaced by the synthetic outcomes, so every part of the
  report except the games themselves is Python's;
- ``parse_agent``: ``parse_agent(text)`` results (kind, preset, every ``SearchConfig`` field,
  the weights) or the ``ValueError`` text;
- ``int``, ``float``, ``flag``: the override converters ``int``, ``float`` and ``_flag``
  (``compare.py:62-75``) on exotic inputs (Unicode digits and spaces, underscores, specials);
- ``newcombe``: the paired-difference interval of gcg-eval's W-42 statistics (Newcombe 1998,
  method 10, from ``reports/stats.py``'s ``wilson``), computed here in the operation order of
  ``crates/gcg-eval/src/stats.rs``, as bit patterns.

Floats are stored as their IEEE-754 bit patterns (``f64`` objects), so NaN and infinities
round-trip.

With ``--games`` it also writes ``crates/gcg-eval/tests/vectors/games.json``: the stdout and
``--out`` bytes of three ``compare`` runs on real games (T-079). With ``--strength`` it writes
``crates/gcg-eval/tests/vectors/strength.json`` (slow): the outcome lists of
``tests/slow/test_ai_strength.py`` (the ``standard`` preset against random, seed 7001, and
greedy, seed 7002, 400 games each; T-070). Outcomes do not depend on ``--workers``.

    uv run python oracle/export_stats.py [--check] [--games] [--strength [--workers N]]
"""

from __future__ import annotations

import argparse
import contextlib
import dataclasses
import importlib
import io
import json
import math
import os
import struct
import sys
import tempfile
from collections.abc import Sequence
from pathlib import Path
from typing import Any, cast
from unittest import mock

from gcg_sim.ai import compare
from gcg_sim.ai.config import SearchConfig
from gcg_sim.ai.evaluation import Weights
from gcg_sim.ai.selfplay import AgentSpec, GameOutcome, GameSpec, mirrored_specs, play_all
from gcg_sim.ai.tuning import TUNING_DECKS
from gcg_sim.deck import load_deck, to_decklist
from gcg_sim.engine.game import DeckList
from gcg_sim.reports.stats import wilson
from gcg_sim.rng import SplitMix64

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "crates" / "gcg-eval" / "tests" / "vectors" / "stats.json"
GAMES = ROOT / "crates" / "gcg-eval" / "tests" / "vectors" / "games.json"
STRENGTH_OUT = ROOT / "crates" / "gcg-eval" / "tests" / "vectors" / "strength.json"
EXAMPLES = ROOT / "examples" / "decks"
Z95 = 1.96


def f64(x: float) -> dict[str, int]:
    return {"f64": struct.unpack("<Q", struct.pack("<d", x))[0]}


def test_decks() -> tuple[DeckList, ...]:
    """``tests/ai/decks.py`` ``DECKS`` (read-only import of the reference's test decks)."""
    if str(ROOT) not in sys.path:
        sys.path.insert(0, str(ROOT))
    module = importlib.import_module("tests.ai.decks")
    return cast(tuple[DeckList, ...], module.DECKS)


def example_decks() -> tuple[DeckList, ...]:
    return tuple(to_decklist(load_deck(p)) for p in sorted(EXAMPLES.glob("*.txt")))


# ---------------------------------------------------------------------------------------------
# synthetic games


def fake_outcome(spec: GameSpec) -> GameOutcome:
    """A deterministic stand-in for ``selfplay.play`` (the Rust test computes the same)."""
    r = SplitMix64(spec.game_seed ^ spec.agent_seeds[0])
    winner = r.randrange(3) - 1
    turns = 1 + r.randrange(40)
    first = r.randrange(2)
    decisions = r.randrange(1000)
    return GameOutcome(spec.index, spec.challenger, winner, turns, first, decisions)


def fake_play_all(specs: Sequence[GameSpec], workers: int) -> list[GameOutcome]:
    return [fake_outcome(s) for s in specs]


# ---------------------------------------------------------------------------------------------
# tables


def spec_row(spec: GameSpec, a: AgentSpec, decks: Sequence[DeckList]) -> list[Any]:
    """``[index, game_seed, agent_seed0, agent_seed1, challenger, agents, decks]``: ``agents``
    is ``"co"`` or ``"oc"`` (challenger/opponent by seat), ``decks`` the indices of the seat
    decks in ``decks`` (the first equal deck)."""
    agents = "".join("c" if s is a else "o" for s in spec.agents)
    seat_decks = [decks.index(d) for d in spec.decks]
    return [
        spec.index,
        spec.game_seed,
        spec.agent_seeds[0],
        spec.agent_seeds[1],
        spec.challenger,
        agents,
        seat_decks,
    ]


def mirrored_cases() -> list[dict[str, Any]]:
    a, b = AgentSpec("random"), AgentSpec("greedy")
    decks = test_decks()
    out = []
    for games, master in ((8, 1), (12, 7001), (4, 2**64 - 1), (6, 0), (0, 5)):
        specs = mirrored_specs(games, master, a, b, (decks[0], decks[1]))
        out.append(
            {"games": games, "master": master, "rows": [spec_row(s, a, decks) for s in specs]}
        )
    return out


def deck_sets() -> dict[str, tuple[DeckList, ...]]:
    examples = example_decks()
    return {
        "tests": test_decks(),
        "tuning": TUNING_DECKS,
        "examples": examples,
        "repeated": (examples[2], examples[2], examples[0]),
    }


def deck_set_cases() -> dict[str, list[dict[str, list[str]]]]:
    """The card numbers of every deck of the sets, in ``DeckList`` order."""
    return {
        name: [{"main": list(d.main), "resources": list(d.resources)} for d in decks]
        for name, decks in deck_sets().items()
        if name != "repeated"
    }


def comparison_cases() -> list[dict[str, Any]]:
    a, b = AgentSpec("random"), AgentSpec("greedy")
    out: list[dict[str, Any]] = []
    for name, decks in deck_sets().items():
        multiple = 4 * len(decks) * (len(decks) - 1) // 2
        for games, seed in ((multiple, 5), (2 * multiple, 1), (multiple, 2**63)):
            specs = compare.comparison_specs(games, seed, a, b, decks)
            out.append(
                {
                    "decks": name,
                    "games": games,
                    "seed": seed,
                    "rows": [spec_row(s, a, decks) for s in specs],
                }
            )
    return out


def spec_errors() -> list[dict[str, Any]]:
    a, b = AgentSpec("random"), AgentSpec("greedy")
    sets = deck_sets()
    cases = [
        ("tests", 0),
        ("tests", -4),
        ("tests", 6),
        ("tuning", 8),
        ("tuning", 0),
        ("examples", 13),
        ("one", 4),
        ("none", 4),
        ("none", 0),
    ]
    out = []
    for name, games in cases:
        decks = sets["tests"][:1] if name == "one" else () if name == "none" else sets[name]
        try:
            compare.comparison_specs(games, 1, a, b, decks)
        except ValueError as exc:
            out.append({"decks": name, "games": games, "error": str(exc)})
        else:
            raise SystemExit(f"comparison_specs({games}, {name}) did not fail")
    return out


def rate_cases() -> list[list[Any]]:
    pairs = [(0, 0), (0, 1), (1, 1), (0, 4), (2, 4), (4, 4), (3, 7), (289, 400), (292, 400)]
    pairs += [(400, 400), (1, 400), (199, 400), (5603, 11200), (6160, 11200), (17, 40)]
    pairs += [(1, 3), (2, 3), (7, 9), (123456, 999999), (50, 100), (49, 100)]
    pairs += [(1, 32), (3, 32), (5, 32), (1, 16), (3, 16), (7, 64), (1, 8), (3, 8), (31, 32)]
    return [[w, n, json.dumps(compare._rate(w, n))] for w, n in pairs]


def summarize_cases() -> list[dict[str, Any]]:
    a, b = AgentSpec("random"), AgentSpec("greedy")
    out = []
    for name, decks in deck_sets().items():
        multiple = 4 * len(decks) * (len(decks) - 1) // 2
        for games, seed in ((multiple, 3), (5 * multiple, 11)):
            specs = compare.comparison_specs(games, seed, a, b, decks)
            outcomes = fake_play_all(specs, 1)
            out.append(
                {
                    "decks": name,
                    "games": games,
                    "seed": seed,
                    "outcomes": [
                        [o.challenger, o.winner, o.turns, o.first_player, o.decisions]
                        for o in outcomes
                    ],
                    "text": json.dumps(compare.summarize(specs, outcomes), indent=2),
                }
            )
    return out


def compare_cli_cases() -> list[dict[str, Any]]:
    zeon = "examples/decks/red-green-zeon.txt"
    federation = "examples/decks/blue-white-federation.txt"
    argvs: list[list[str]] = [
        ["random", "random", "--games", "4", "--seed", "2"],
        [
            "greedy:weights=hand",
            "baseline-standard:exploration=0.2",
            "--games",
            "24",
            "--seed",
            "5",
            "--decks",
            "tuning",
            "--workers",
            "3",
        ],
        ["standard", "fast:name=fäst", "--games", "12", "--seed", "-3", "--decks", "examples"],
        [
            "strong",
            "random",
            "--games",
            "8",
            "--seed",
            str(2**64 + 1),
            "--decks",
            f"{zeon},{federation}",
        ],
        ["greedy", "random", "--games", "12", "--decks", f"{zeon},{zeon},{federation}"],
        [
            "baseline-strong",
            "standard:horizon_turns=3,weights=roles",
            "--games",
            "8",
            "--seed",
            "0",
        ],
    ]
    out = []
    cwd = os.getcwd()
    os.chdir(ROOT)
    try:
        for argv in argvs:
            with tempfile.TemporaryDirectory() as tmp:
                target = Path(tmp) / "report.json"
                buf = io.StringIO()
                with (
                    mock.patch.object(compare, "play_all", fake_play_all),
                    contextlib.redirect_stdout(buf),
                ):
                    compare.main([*argv, "--out", str(target)])
                out.append(
                    {"argv": argv, "stdout": buf.getvalue(), "out": target.read_text("utf-8")}
                )
    finally:
        os.chdir(cwd)
    return out


# ---------------------------------------------------------------------------------------------
# agent specs and converters


def encode_value(v: object) -> object:
    if isinstance(v, bool | int | str):
        return v
    if isinstance(v, float):
        return f64(v)
    raise SystemExit(f"unexpected SearchConfig value {v!r}")


def config_json(config: SearchConfig | None) -> object:
    if config is None:
        return None
    return {f.name: encode_value(getattr(config, f.name)) for f in dataclasses.fields(config)}


def weights_json(weights: Weights | None) -> object:
    if weights is None:
        return None
    return {"feature_set": weights.feature_set, "values": [f64(v) for v in weights.values]}


AGENT_TEXTS = [
    "random",
    "greedy",
    "fast",
    "standard",
    "strong",
    "baseline-standard",
    "baseline-strong",
    "baseline-fast",
    "baseline-",
    "baseline",
    "Standard",
    "mcts",
    "minimax",
    "",
    ":",
    "random:",
    "random:,",
    "random:weights=hand",
    "random:exploration=1",
    "greedy:weights=hand",
    "greedy:weights=roles",
    "greedy:weights=default,weights=baseline",
    "greedy:exploration=1",
    "greedy:name=x",
    "standard:nonsense=1",
    "standard:weights=other",
    "standard:weights",
    "standard:exploration",
    "standard:=1",
    "standard:a=b=c",
    "standard:exploration=0.2",
    "standard:exploration= 0.2 ",
    "standard:exploration=1_0.5",
    "standard:exploration=1e-3,,horizon_turns=3",
    "standard:exploration=inf",
    "standard:exploration=-0.0",
    "standard:exploration=0x1p-2",
    "standard:exploration=٠.٥",
    "standard:max_iterations=12",
    "standard:max_iterations= 12 ",
    "standard:max_iterations=+5",
    "standard:max_iterations=1_000",
    "standard:max_iterations=1.5",
    "standard:max_iterations=٣",
    "standard:max_iterations=4294967295",
    "standard:max_iterations=0",
    "standard:horizon_turns=4",
    "standard:horizon_turns=5",
    "standard:horizon_turns=-2",
    "standard:noop_branch_choices=yes",
    "standard:noop_branch_choices=TRUE",
    "standard:strict_attack_prune=No",
    "standard:observation_keys=0",
    "standard:observation_keys=maybe",
    "standard:name=variant",
    "standard:name=fäst",
    "standard:weights=hand,weights=baseline",
    "baseline-standard:exploration=0.2,horizon_turns=3,weights=hand",
    "baseline-strong:weights=default",
    "fast:target_extension=2.5,target_extension_z=1",
    "strong:max_rollout_decisions=3,mulligan_samples=7,first_player_games=0",
    "fast:policy_burst_yes_score=-1.25",
]


def parse_agent_cases() -> list[dict[str, Any]]:
    out: list[dict[str, Any]] = []
    for text in AGENT_TEXTS:
        try:
            spec = compare.parse_agent(text)
        except ValueError as exc:
            out.append({"text": text, "error": str(exc)})
            continue
        out.append(
            {
                "text": text,
                "kind": spec.kind,
                "preset": spec.preset,
                "config": config_json(spec.config),
                "weights": weights_json(spec.weights),
            }
        )
    return out


INT_TEXTS = [
    "12",
    " 12 ",
    "+5",
    "-1",
    "0",
    "-0",
    "007",
    "1_000",
    "1__0",
    "_1",
    "1_",
    "٣",
    "１２",
    "0x10",
    "1.5",
    "",
    " ",
    "  7 ",
    "\x1c3\x1f",
    " 12",
    "4294967296",
    "-4294967297",
    "1" * 25,
    "x" * 230,
    "1 2",
    "١٢٣",
]

FLOAT_TEXTS = [
    "0.2",
    " 0.2 ",
    "1e-3",
    "1E+3",
    "1_0.5",
    "1__0.5",
    "1_.5",
    "1._5",
    "_1.5",
    "1.5_",
    "1_000e1_0",
    "1e_5",
    ".5",
    "5.",
    ".",
    "e5",
    "1e",
    "1e+",
    "inf",
    "-Infinity",
    "+inf",
    "INF",
    "nan",
    "-nan",
    "NaN",
    "infinity",
    "infx",
    "in f",
    "0x1p-2",
    "٠.٥",
    "１.５",
    "1e309",
    "-1e309",
    "-0.0",
    "4.9e-324",
    "2.4703282292062328e-324",
    "0.1",
    "123456789012345678901234567890",
    "",
    " ",
    "1 2",
    " 0.5",
    "\x1c0.5\x1d",
    "+-1",
    "--1",
    "1.5e3.2",
    "0.30000000000000004",
    "9007199254740993",
]

FLAG_TEXTS = [
    "1",
    "true",
    "yes",
    "TRUE",
    "Yes",
    "yEs",
    "0",
    "false",
    "no",
    "NO",
    "maybe",
    "",
    " true",
]
FLAG_TEXTS += ["True ", "K", "y", "2", "t"]


def converter_cases() -> dict[str, list[dict[str, Any]]]:
    ints: list[dict[str, Any]] = []
    for text in INT_TEXTS:
        try:
            ints.append({"text": text, "value": str(int(text))})
        except ValueError as exc:
            ints.append({"text": text, "error": str(exc)})
    floats: list[dict[str, Any]] = []
    for text in FLOAT_TEXTS:
        try:
            floats.append({"text": text, "value": f64(float(text))})
        except ValueError as exc:
            floats.append({"text": text, "error": str(exc)})
    flags: list[dict[str, Any]] = []
    for text in FLAG_TEXTS:
        try:
            flags.append({"text": text, "value": compare._flag(text)})
        except ValueError as exc:
            flags.append({"text": text, "error": str(exc)})
    return {"int": ints, "float": floats, "flag": flags}


# ---------------------------------------------------------------------------------------------
# W-42 paired statistics


def newcombe_paired(a: int, b: int, c: int, d: int, z: float = Z95) -> tuple[float, float]:
    """Newcombe (1998) method 10 for the difference of paired proportions ``(a + b) / n -
    (a + c) / n``: the Wilson intervals of both margins (``reports/stats.py:12-25``) combined
    with the correlation ``phi`` (0 when a margin is empty). Operation order as stats.rs."""
    n = a + b + c + d
    if n == 0:
        return (-1.0, 1.0)
    p1 = (a + b) / n
    p2 = (a + c) / n
    l1, u1 = wilson(a + b, n, z)
    l2, u2 = wilson(a + c, n, z)
    den = (a + b) * (c + d) * (a + c) * (b + d)
    phi = 0.0 if den == 0 else (a * d - b * c) / math.sqrt(den)
    diff = p1 - p2
    dl = p1 - l1
    du = u2 - p2
    el = u1 - p1
    eu = p2 - l2
    delta = math.sqrt(max(0.0, dl * dl - 2.0 * phi * dl * du + du * du))
    eps = math.sqrt(max(0.0, el * el - 2.0 * phi * el * eu + eu * eu))
    return (diff - delta, diff + eps)


NEWCOMBE_TABLES = [
    (36, 12, 2, 0),
    (263, 15, 3, 0),
    (2, 1, 1, 0),
    (1, 1, 0, 0),
    (5, 5, 0, 0),
    (39, 7, 0, 4),
    (0, 0, 0, 0),
    (0, 0, 0, 5),
    (5, 0, 0, 0),
    (0, 5, 0, 0),
    (0, 0, 5, 0),
    (10, 0, 0, 10),
    (0, 10, 10, 0),
    (50, 30, 20, 100),
    (100, 45, 55, 0),
    (1234, 567, 89, 10),
    (1, 0, 0, 0),
    (0, 1, 0, 0),
    (3, 2, 1, 4),
]


def newcombe_cases() -> list[dict[str, Any]]:
    out = []
    for a, b, c, d in NEWCOMBE_TABLES:
        lo, hi = newcombe_paired(a, b, c, d)
        out.append({"table": [a, b, c, d], "lo": f64(lo), "hi": f64(hi)})
    return out


# ---------------------------------------------------------------------------------------------
# real games (slow; --games): compare runs and the AI-strength outcome lists


COMPARE_REAL: list[list[str]] = [
    ["random", "greedy", "--games", "8", "--seed", "2"],
    ["greedy:weights=hand", "random", "--games", "12", "--seed", "5", "--decks", "tuning"],
    ["fast", "random", "--games", "4", "--seed", "3", "--workers", "4"],
]
STRENGTH = (("random", 7001), ("greedy", 7002))
STRENGTH_GAMES = 400


def compare_real_cases() -> list[dict[str, Any]]:
    """``python -m gcg_sim.ai.compare`` with real games: stdout and ``--out`` bytes (T-079)."""
    out = []
    cwd = os.getcwd()
    os.chdir(ROOT)
    try:
        for argv in COMPARE_REAL:
            with tempfile.TemporaryDirectory() as tmp:
                target = Path(tmp) / "report.json"
                buf = io.StringIO()
                with contextlib.redirect_stdout(buf):
                    compare.main([*argv, "--out", str(target)])
                out.append(
                    {"argv": argv, "stdout": buf.getvalue(), "out": target.read_text("utf-8")}
                )
    finally:
        os.chdir(cwd)
    return out


def strength_cases(workers: int) -> list[dict[str, Any]]:
    """``tests/slow/test_ai_strength.py``: the MCTS ``standard`` preset against random (seed
    7001) and greedy (seed 7002), 400 mirrored games each on ``tests/ai/decks.py`` (T-070)."""
    mcts = AgentSpec("mcts", "standard")
    decks = test_decks()
    out = []
    for kind, seed in STRENGTH:
        specs = mirrored_specs(STRENGTH_GAMES, seed, mcts, AgentSpec(kind), (decks[0], decks[1]))
        outcomes = play_all(specs, workers)
        out.append(
            {
                "opponent": kind,
                "seed": seed,
                "outcomes": [
                    [o.challenger, o.winner, o.turns, o.first_player, o.decisions] for o in outcomes
                ],
            }
        )
    return out


def build_games() -> bytes:
    data = {"compare": compare_real_cases()}
    return (json.dumps(data, indent=1, ensure_ascii=True) + "\n").encode("ascii")


def build_strength(workers: int) -> bytes:
    data = {"strength": strength_cases(workers)}
    return (json.dumps(data, indent=1, ensure_ascii=True) + "\n").encode("ascii")


# ---------------------------------------------------------------------------------------------


def build() -> bytes:
    data = {
        "deck_sets": deck_set_cases(),
        "rates": rate_cases(),
        "mirrored_specs": mirrored_cases(),
        "comparison_specs": comparison_cases(),
        "spec_errors": spec_errors(),
        "summarize": summarize_cases(),
        "compare_cli": compare_cli_cases(),
        "parse_agent": parse_agent_cases(),
        **converter_cases(),
        "newcombe": newcombe_cases(),
    }
    return (json.dumps(data, indent=1, ensure_ascii=True) + "\n").encode("ascii")


def emit(path: Path, data: bytes, check: bool) -> int:
    if check:
        if not path.exists() or path.read_bytes() != data:
            print(f"{path} is stale; run without --check", file=sys.stderr)
            return 1
        print(f"{path} is current")
        return 0
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)
    print(f"wrote {path}")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true", help="fail if a committed file is stale")
    ap.add_argument(
        "--games", action="store_true", help=f"also play the compare games of {GAMES.name}"
    )
    ap.add_argument(
        "--strength",
        action="store_true",
        help=f"also play the games of {STRENGTH_OUT.name} (slow: 800 MCTS standard games)",
    )
    ap.add_argument("--workers", type=int, default=8, help="processes for --strength")
    args = ap.parse_args()
    status = emit(OUT, build(), args.check)
    if args.games:
        status = max(status, emit(GAMES, build_games(), args.check))
    if args.strength:
        status = max(status, emit(STRENGTH_OUT, build_strength(args.workers), args.check))
    return status


if __name__ == "__main__":
    sys.exit(main())
