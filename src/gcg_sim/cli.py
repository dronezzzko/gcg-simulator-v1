"""Command-line interface: ``gcg-sim benchmark | validate | data status | replay``.

Exit codes: 0 success, 1 invalid deck/data/replay or a failed simulation, 2 usage error,
130 interrupted. Test hook: when the environment variable ``GCG_SIM_AGENT_FACTORY`` is set
(``"package.module:attr"`` or ``"/path/to/file.py:attr"``), ``benchmark`` builds agents with
that factory instead of the search AI. It exists for the test suite only.
"""

from __future__ import annotations

import argparse
import json
import os
import sys
from collections import Counter
from collections.abc import Sequence
from pathlib import Path
from typing import Any, TextIO

from gcg_sim.cards.db import get_card_db, load_overrides, read_data_text
from gcg_sim.deck import Deck, DeckError, load_deck, validate_deck
from gcg_sim.deck.official import banlist
from gcg_sim.effects.registry import get_registry
from gcg_sim.reports import write_reports
from gcg_sim.reports.aggregate import game_outcomes, match_outcomes
from gcg_sim.reports.versions import manifest, versions
from gcg_sim.runner import BenchmarkConfig, FactoryRef, GameFailure, run_benchmark
from gcg_sim.runner.agents import AgentFactory, AgentUnavailableError
from gcg_sim.runner.config import AI_PRESETS, FORMATS
from gcg_sim.runner.replayfile import ReplayError, check_replay

AGENT_FACTORY_ENV = "GCG_SIM_AGENT_FACTORY"
EXIT_OK, EXIT_INVALID, EXIT_USAGE, EXIT_INTERRUPTED = 0, 1, 2, 130


class CliError(Exception):
    """An expected failure with a message for the user (exit code 1)."""


def _positive_int(text: str) -> int:
    value = _int(text)
    if value < 1:
        raise argparse.ArgumentTypeError(f"must be at least 1, got {value}")
    return value


def _non_negative_int(text: str) -> int:
    value = _int(text)
    if value < 0:
        raise argparse.ArgumentTypeError(f"must be 0 or more, got {value}")
    return value


def _seed(text: str) -> int:
    value = _int(text)
    if not 0 <= value < 2**64:
        raise argparse.ArgumentTypeError(f"must be in [0, 2**64), got {value}")
    return value


def _int(text: str) -> int:
    try:
        return int(text, 0)
    except ValueError:
        raise argparse.ArgumentTypeError(f"not an integer: {text!r}") from None


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="gcg-sim", description="Gundam Card Game deck benchmarking simulator."
    )
    sub = parser.add_subparsers(dest="command", required=True)
    bench = sub.add_parser("benchmark", help="play a deck under test against a benchmark deck")
    bench.add_argument("deck_under_test", type=Path, help="deck file of the deck under test")
    bench.add_argument("benchmark_deck", type=Path, help="deck file of the benchmark deck")
    bench.add_argument("--matches", type=_positive_int, required=True, help="number of matches")
    bench.add_argument("--seed", type=_seed, default=0, help="master seed (default 0)")
    bench.add_argument("--workers", type=_positive_int, default=1, help="worker processes")
    bench.add_argument("--format", choices=FORMATS, default="bo3", help="match format")
    bench.add_argument("--ai-preset", choices=AI_PRESETS, default="standard", help="AI strength")
    bench.add_argument("--out", type=Path, default=Path("gcg-sim-out"), help="report directory")
    bench.add_argument("--decision-log", action="store_true", help="record AI decision logs")
    bench.add_argument(
        "--replays", type=_non_negative_int, default=3, help="replays saved per result"
    )
    val = sub.add_parser("validate", help="check a deck file against the construction rules")
    val.add_argument("deck", type=Path)
    data = sub.add_parser("data", help="packaged data")
    data_sub = data.add_subparsers(dest="data_command", required=True)
    status = data_sub.add_parser("status", help="data snapshot, rules, B&R, coverage, conflicts")
    status.add_argument("--json", action="store_true", help="print JSON instead of text")
    rep = sub.add_parser("replay", help="re-simulate a saved replay and verify its outcome")
    rep.add_argument("replay_json", type=Path)
    rep.add_argument("--log", action="store_true", help="print the game log")
    return parser


# ---------------------------------------------------------------------------------------------
# decks


def _read_deck(path: Path) -> Deck:
    try:
        return load_deck(path)
    except OSError as exc:
        raise CliError(f"cannot read deck file {path}: {exc.strerror or exc}") from exc
    except DeckError as exc:
        raise CliError(f"{path}: {exc}") from exc


def _deck_colors(deck: Deck) -> str:
    db = get_card_db()
    colors = sorted({c.value for n in deck.main if (c := db[n].color) is not None})
    return "/".join(colors) or "no colors"


def _legal_deck(path: Path) -> Deck:
    deck = _read_deck(path)
    violations = validate_deck(deck)
    if violations:
        lines = [f"{path}: {len(violations)} violation(s)"]
        lines += [f"  [{v.code}] {v.message}" for v in violations]
        raise CliError("\n".join(lines))
    return deck


def cmd_validate(args: argparse.Namespace, out: TextIO) -> int:
    deck = _legal_deck(args.deck)
    print(
        f"{args.deck}: legal ({len(deck.main)} cards, {len(deck.resources)} resources; "
        f"{_deck_colors(deck)})",
        file=out,
    )
    return EXIT_OK


# ---------------------------------------------------------------------------------------------
# benchmark


class ProgressPrinter:
    """Writes match progress to stderr: in place on a terminal, else about every 10%."""

    def __init__(self, stream: TextIO) -> None:
        self.stream = stream
        self.tty = stream.isatty()
        self.last_decile = -1

    def __call__(self, done: int, total: int) -> None:
        if self.tty:
            end = "\n" if done == total else ""
            print(f"\rmatches {done}/{total}", end=end, file=self.stream, flush=True)
            return
        decile = 10 * done // total
        if decile != self.last_decile:
            self.last_decile = decile
            print(f"matches {done}/{total}", file=self.stream, flush=True)


def _agent_factory() -> AgentFactory | None:
    ref = os.environ.get(AGENT_FACTORY_ENV)
    return FactoryRef(ref) if ref else None


def _rate_line(label: str, block: dict[str, Any]) -> str:
    r = block["dut_win_rate"]
    lo, hi = r["ci95"]
    return (
        f"{label}: DUT {block['dut_wins']}-{block['bench_wins']} (draws {block['draws']}), win "
        f"rate {100 * r['rate']:.1f}% [95% CI {100 * lo:.1f}%-{100 * hi:.1f}%] over {block['n']}"
    )


def cmd_benchmark(args: argparse.Namespace, out: TextIO, err: TextIO) -> int:
    dut = _legal_deck(args.deck_under_test)
    bench = _legal_deck(args.benchmark_deck)
    config = BenchmarkConfig(
        deck_under_test=dut,
        benchmark_deck=bench,
        matches=args.matches,
        seed=args.seed,
        workers=args.workers,
        fmt=args.format,
        ai_preset=args.ai_preset,
        out_dir=args.out,
        decision_log=args.decision_log,
        replays=args.replays,
    )
    try:
        run = run_benchmark(config, ProgressPrinter(err), agent_factory=_agent_factory())
    except AgentUnavailableError as exc:
        raise CliError(f"{exc}; the benchmark needs the search AI") from exc
    except GameFailure as exc:
        raise CliError(f"simulation failed: {exc}") from exc
    paths = write_reports(run, config.out_dir)
    print(f"{dut.name} (deck under test) vs {bench.name} (benchmark)", file=out)
    print(_rate_line("matches", match_outcomes(run.matches)), file=out)
    print(_rate_line("games", game_outcomes(run.games)), file=out)
    print("reports:", file=out)
    for label, path in (
        ("results", paths.results),
        ("summary", paths.summary),
        ("games", paths.games),
        ("timing", paths.timing),
    ):
        print(f"  {label:8} {path}", file=out)
    print(f"  replays  {len(paths.replays)} file(s) in {paths.out_dir / 'replays'}", file=out)
    return EXIT_OK


# ---------------------------------------------------------------------------------------------
# data status


def _coverage() -> dict[str, Any]:
    reg = get_registry()
    real = reg.db.real_cards()
    implemented = [c for c in real if reg.cards[c.def_id].script is not None]
    by_type = Counter(c.card_type.value for c in real)
    impl_by_type = Counter(c.card_type.value for c in implemented)
    return {
        "card_numbers": len(real),
        "implemented": len(implemented),
        "by_type": {
            t: {"cards": by_type[t], "implemented": impl_by_type[t]} for t in sorted(by_type)
        },
    }


def _conflicts() -> dict[str, Any]:
    resolutions = load_overrides().get("resolutions", {})
    applied = get_card_db().applied_overrides
    curated = json.loads(read_data_text("curated_conflicts.json")).get("conflicts", [])
    return {
        "resolutions": len(resolutions),
        "card_data_overrides": len(applied),
        "card_numbers_overridden": len({o.card_number for o in applied}),
        "curated_conflicts": len(curated),
    }


def data_status() -> dict[str, Any]:
    m = manifest()
    bl = banlist()
    return {
        "versions": versions(),
        "snapshot": {
            k: m.get(k)
            for k in (
                "dataset_version",
                "source_commit",
                "built_at",
                "card_count",
                "set_count",
                "ruling_count",
                "rules_faq_count",
                "errata_count",
                "product_count",
            )
        },
        "banlist": {
            "effective_date": bl.effective_date,
            "banned": sorted(bl.banned),
            "restricted": {r.card_number: r.max_copies for r in bl.restricted},
            "banned_pairs": [list(p.cards) for p in bl.banned_pairs],
            "attribute_pair_rules": [r.id for r in bl.attribute_rules],
        },
        "coverage": _coverage(),
        "conflicts": _conflicts(),
    }


def _status_text(s: dict[str, Any]) -> list[str]:
    v, snap, bl, cov, con = (
        s[k] for k in ("versions", "snapshot", "banlist", "coverage", "conflicts")
    )
    pct = 100 * cov["implemented"] / cov["card_numbers"] if cov["card_numbers"] else 0.0
    lines = [
        f"gcg-sim {v['package']} (results schema {v['results_schema']})",
        f"card data: gcg-api dataset {snap['dataset_version']}",
        f"  source commit {snap['source_commit']}, built {snap['built_at']}",
        f"  {snap['card_count']} printings, {cov['card_numbers']} card numbers, "
        f"{snap['set_count']} sets, {snap['ruling_count']} rulings, "
        f"{snap['rules_faq_count']} rules FAQ entries, {snap['errata_count']} errata",
        f"rules: Comprehensive Rules Ver. {v['rules_version']} (effective "
        f"{v['rules_effective_date']})",
        f"banned & restricted list: effective {bl['effective_date']} ({len(bl['banned'])} banned, "
        f"{len(bl['restricted'])} restricted, {len(bl['banned_pairs'])} banned pairs, "
        f"{len(bl['attribute_pair_rules'])} attribute pair rule(s))",
        f"BO3 match rules: effective {v['bo3_rules_effective_date']}",
        f"implementation coverage: {cov['implemented']}/{cov['card_numbers']} card numbers "
        f"({pct:.1f}%)",
    ]
    lines += [f"  {t}: {c['implemented']}/{c['cards']}" for t, c in cov["by_type"].items()]
    lines.append(
        f"conflicts: {con['resolutions']} resolutions in overrides.json "
        f"({con['card_data_overrides']} card-data overrides on "
        f"{con['card_numbers_overridden']} card numbers); {con['curated_conflicts']} curated "
        "conflicts"
    )
    return lines


def cmd_data_status(args: argparse.Namespace, out: TextIO) -> int:
    status = data_status()
    if args.json:
        print(json.dumps(status, indent=2, sort_keys=True, ensure_ascii=False), file=out)
    else:
        print("\n".join(_status_text(status)), file=out)
    return EXIT_OK


# ---------------------------------------------------------------------------------------------
# replay


def cmd_replay(args: argparse.Namespace, out: TextIO) -> int:
    try:
        doc = json.loads(args.replay_json.read_text(encoding="utf-8"))
    except OSError as exc:
        raise CliError(
            f"cannot read replay file {args.replay_json}: {exc.strerror or exc}"
        ) from exc
    except json.JSONDecodeError as exc:
        raise CliError(f"{args.replay_json} is not valid JSON: {exc}") from exc
    if not isinstance(doc, dict):
        raise CliError(f"{args.replay_json} is not a replay document")
    try:
        check = check_replay(doc)
    except (ReplayError, ValueError, LookupError) as exc:
        raise CliError(f"cannot replay {args.replay_json}: {exc}") from exc
    if args.log:
        print("\n".join(check.log), file=out)
    for seat in sorted(doc["seats"], key=lambda s: int(s["seat"])):
        print(f"seat {seat['seat']}: {seat['role']} {seat['deck']} ({seat['agent']})", file=out)
    print(
        f"result: winner seat {check.winner_seat} ({check.end_reason}) after {check.turns} turns",
        file=out,
    )
    if not check.ok:
        raise CliError(
            "replay does NOT match the recorded outcome:\n  " + "\n  ".join(check.mismatches)
        )
    print("replay matches the recorded outcome and final state", file=out)
    return EXIT_OK


# ---------------------------------------------------------------------------------------------


def _dispatch(args: argparse.Namespace, out: TextIO, err: TextIO) -> int:
    if args.command == "benchmark":
        return cmd_benchmark(args, out, err)
    if args.command == "validate":
        return cmd_validate(args, out)
    if args.command == "replay":
        return cmd_replay(args, out)
    return cmd_data_status(args, out)


def main(argv: Sequence[str] | None = None) -> int:
    out, err = sys.stdout, sys.stderr
    try:
        args = build_parser().parse_args(argv)
    except SystemExit as exc:
        return exc.code if isinstance(exc.code, int) else EXIT_USAGE
    try:
        return _dispatch(args, out, err)
    except CliError as exc:
        print(f"error: {exc}", file=err)
        return EXIT_INVALID
    except KeyboardInterrupt:
        print("interrupted: workers stopped, no reports written", file=err)
        return EXIT_INTERRUPTED
