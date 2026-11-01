"""Game, search, view and world traces of the Python reference for the Rust differential runner.

docs/port/PLAN.md §8.1-§8.5 and §8.9 (workstream G). Trace format version 1 (§8.2): NDJSON,
one compact object per line with sorted keys and ``ensure_ascii``; floats are ``float.hex()``
strings; ints are JSON numbers except u64 values of 2**53 and above, which are decimal strings;
actions are ``[kind, a, b, c]`` (``engine/state.py:23-24``).

The exporter only observes. It calls public functions, reads state without touching it, and
wraps a few AI functions transparently (pass-through wrappers that record arguments and
results). ``--check`` also replays every trace without any wrapper and requires the same
actions and final digest (the wrapped-vs-plain check, T-029).

Sets (§8.3): ``full``, ``vanilla``, ``fuzz``, ``coverage``, ``search``, ``baselines``,
``views`` and ``worlds``; ``tests`` (per-test traces) is written by
``port/oracle/pytest_trace.py`` (workstream G2) into ``tier1/tests/``, which this tool only
hashes into the manifest. ``components`` vectors are ``export_ai_vectors.py`` (D1/D2).

Every output file is generated in a fresh worker process (one task per file), so the
process-local registry numbering each trace sees depends only on the file's own trace order,
never on scheduling or the worker count.

    uv run python port/oracle/traces.py --write --tier 1          # both hash seeds, then write
    uv run python port/oracle/traces.py --check --tier 1 [--nocache] [--only SET,...]
    uv run python port/oracle/traces.py --write-manifest --tier 1 # rehash (after G2 wrote tests/)
    uv run python port/oracle/traces.py --deep ID --decision K [--apply-level] [--iteration N]
    uv run python port/oracle/traces.py state --trace ID --decision K
    uv run python port/oracle/traces.py game --decks A B --seed S --agents KIND --out FILE
    uv run python port/oracle/traces.py vectors [--check]        # crates/gcg-diff test vectors
"""

from __future__ import annotations

import argparse
import concurrent.futures
import contextlib
import dataclasses
import enum
import hashlib
import importlib
import json
import multiprocessing
import os
import platform
import shutil
import subprocess
import sys
from collections.abc import Iterator, Sequence
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Protocol, cast

import coverage_decks

from gcg_sim.ai import agents as ai_agents
from gcg_sim.ai import mcts, playout, policy
from gcg_sim.ai.base import AgentCore, Alternative
from gcg_sim.ai.baseline import BASELINE_PRESETS, BASELINE_WEIGHTS
from gcg_sim.ai.config import PRESETS
from gcg_sim.ai.evaluation import DEFAULT_WEIGHTS
from gcg_sim.ai.policy import BASELINE_KNOBS
from gcg_sim.deck.parse import load_deck, to_decklist
from gcg_sim.effects.registry import Registry, get_registry
from gcg_sim.engine import view as V
from gcg_sim.engine.game import MAX_ACTIONS_DEFAULT, DeckList, apply, new_game
from gcg_sim.engine.invariants import check as check_invariants
from gcg_sim.engine.invariants import initial_multiset
from gcg_sim.engine.observe import determinize
from gcg_sim.engine.state import Action, GameState
from gcg_sim.engine.types import DecisionKind, EndReason, Zone
from gcg_sim.reports.versions import code_digest
from gcg_sim.rng import SplitMix64, derive_seed
from gcg_sim.runner.config import agent_seed, game_seed
from gcg_sim.runner.digest import (
    LASTING_SOURCE,
    ONCE_TAGS,
    canonical_state,
    content_id,
    state_digest,
)
from gcg_sim.testkit import vanilla_deck
from gcg_sim.tools.fuzz import deck_for, play_one

ROOT = Path(__file__).resolve().parents[2]
CORPUS = ROOT / "port" / "corpus" / "v1"
TIER_DIRS = {1: CORPUS / "tier1", 2: ROOT / "build" / "oracle" / "tier2"}
MANIFESTS = {1: CORPUS / "MANIFEST.json", 2: CORPUS / "MANIFEST-tier2.json"}
IR_PATH = ROOT / "crates" / "gcg-effects" / "ir" / "registry.json"
DATA_LOCK = ROOT / "data" / "SOURCES.lock.json"
EXAMPLES = ROOT / "examples" / "decks"
OFFSET_HOOK = Path(__file__).resolve().parent / "offset_hook"
TESTS_SUBDIR = "tests"

TRACE_VERSION = 1
MANIFEST_FORMAT = 1
SNAPSHOT_EVERY = 32
U53 = 1 << 53
TIER1_MAX_BYTES = 45 * 1024 * 1024
FILE_MAX_BYTES = 8 * 1024 * 1024
HASH_SEEDS = (1, 2)
EFFECT_OFFSET_ENV = "GCG_ORACLE_EFFECT_OFFSET"
SETS = ("full", "vanilla", "fuzz", "coverage", "search", "baselines", "views", "worlds")
EXAMPLE_DECKS = ("blue-white-federation", "purple-white-tekkadan", "red-green-zeon")
SHORT = {"blue-white-federation": "bwf", "purple-white-tekkadan": "pwt", "red-green-zeon": "rgz"}
COLORS = ("Blue", "Green", "Purple", "Red", "White")
POLICY_TEMPERATURE = 0.5
TRACE_MASTER = 20261101
FUZZ_SUMMARY_SEED = 2026
FUZZ_SUMMARY_GAMES = 10_000
FUZZ_SUMMARY_CHUNK = 500
SEARCH_MASTER = 1
BASELINE_GAME_SEED = 4242
BASELINE_AGENT_SEED = 17
ACCEPTANCE_SEARCHES = ((1, 2), (0, 4), (1, 3))  # (seat, agent decision k): Inv 07 §17

Json = Any


# ---------------------------------------------------------------------------------------------
# JSON encoding (§8.2)


def dumps_line(obj: Json) -> str:
    return json.dumps(obj, sort_keys=True, separators=(",", ":"), ensure_ascii=True)


def u64(n: int) -> int | str:
    """A u64 as §8.2 writes it: a JSON number below 2**53, else a decimal string."""
    if not 0 <= n < 1 << 64:
        raise ValueError(f"not a u64: {n}")
    return n if n < U53 else str(n)


def fhex(x: float) -> str:
    return float.hex(x)


def node_json(x: object) -> Json:
    """A DSL value as a self-contained IR node (§3.3 field encoding, nested nodes inline):
    ``{"_": class name, field: value, ...}``; enums are their values, tuples are arrays."""
    if isinstance(x, enum.Enum):
        return x.value
    if x is None or isinstance(x, (bool, int, str)):
        return x
    if isinstance(x, (tuple, list)):
        return [node_json(v) for v in x]
    if dataclasses.is_dataclass(x) and not isinstance(x, type):
        out: dict[str, Json] = {"_": type(x).__name__}
        for f in dataclasses.fields(x):
            out[f.name] = node_json(getattr(x, f.name))
        return out
    raise TypeError(f"cannot encode {type(x).__name__} as an IR node")


def action_json(a: Action) -> list[Json]:
    return a.to_json()


def options_sha16(options: Sequence[Action]) -> str:
    blob = json.dumps([o.to_json() for o in options], separators=(",", ":"), ensure_ascii=True)
    return hashlib.sha256(blob.encode()).hexdigest()[:16]


# ---------------------------------------------------------------------------------------------
# Records


def side_tables(st: GameState, reg: Registry) -> tuple[dict[str, Json], dict[str, Json]]:
    """``{content_id: node}`` of every continuous effect and filter set the canonical state
    names by content id (``runner/digest.py:36-49``), which ``from_canonical`` needs (§4.8)."""
    cont: dict[str, Json] = {}
    fsets: dict[str, Json] = {}
    for le in (*st.lasting, *st.delayed):
        eff = reg.continuous[le.effect_key]
        cont[content_id(eff)] = node_json(eff)
        if le.filters_key >= 0:
            fs = reg.filter_sets[le.filters_key]
            fsets[content_id(fs)] = node_json(fs)
    for key in st.once_used:
        if len(key) >= 3 and key[0] in ONCE_TAGS and key[1] == LASTING_SOURCE:
            eff = reg.continuous[key[2]]
            cont[content_id(eff)] = node_json(eff)
    return cont, fsets


def snapshot_record(st: GameState, i: int, tag: str = "snapshot") -> dict[str, Json]:
    reg = get_registry()
    cont, fsets = side_tables(st, reg)
    return {
        "t": tag,
        "i": i,
        "state": canonical_state(st, reg),
        "continuous": cont,
        "filter_sets": fsets,
        "rules_checked": st._rules_checked,
    }


def fingerprint(st: GameState) -> dict[str, Json]:
    return {
        "rng": u64(st.rng.state),
        "version": st.version,
        "rc": st._rules_checked,
        "action_count": st.action_count,
        "event_group": st.event_group,
        "turn": st.turn,
        "step": int(st.step),
        "digest16": state_digest(st)[:16],
    }


def decision_record(
    st: GameState, i: int, chosen: Action, fp: dict[str, Json], options_mode: str
) -> dict[str, Json]:
    dec = st.pending
    assert dec is not None
    rec: dict[str, Json] = {
        "t": "decision",
        "i": i,
        "player": dec.player,
        "kind": dec.kind.value,
        "prompt": dec.prompt,
        "context": [[k, v] for k, v in dec.context],
        "n": len(dec.options),
        "chosen": action_json(chosen),
        **fp,
    }
    if options_mode == "full":
        rec["options"] = [action_json(o) for o in dec.options]
    else:
        rec["options_sha16"] = options_sha16(dec.options)
    return rec


def outcome_record(st: GameState, actions: int) -> dict[str, Json]:
    return {
        "t": "outcome",
        "winner": st.winner,
        "end_reason": st.end_reason.value if st.end_reason is not None else None,
        "turn": st.turn,
        "actions": actions,
        "final_digest": state_digest(st),
        "state": canonical_state(st),
    }


def deck_json(deck: DeckList) -> dict[str, Json]:
    return {"main": list(deck.main), "res": list(deck.resources)}


# ---------------------------------------------------------------------------------------------
# Specs


@dataclass(frozen=True, slots=True)
class Spec:
    """How to generate one trace (or one BO3 match of traces, ``bo3``)."""

    set: str
    id: str
    kind: str  # "game" or "search"
    decks: tuple[DeckList, DeckList]
    seed: int | None  # None: the trace starts from a world or view snapshot
    chooser: int | None
    agents: tuple[str, str]  # agent kinds, see make_player
    agent_seeds: tuple[int, int]
    preset: str | None = None
    snapshots: str = "periodic"  # or "all"
    stop: str = "end"  # or "first_main"
    options_mode: str = "full"
    agent_blocks: bool = False
    deep: tuple[tuple[int, int], ...] = ()  # (seat, agent decision k) with level-2 records
    fuzz_check: bool = False  # invariants after every action, as tools/fuzz.py:101-103
    bo3: bool = False
    master: int = 0  # runner master seed and match index of a BO3 match (bo3 only)
    match: int = 0
    turn_limit: int = 200
    max_actions: int = MAX_ACTIONS_DEFAULT
    # worlds and views: the source game and the seed that picks its decision (and determinises)
    source: str | None = None
    sample_seed: int = 0


@dataclass(frozen=True, slots=True)
class FileTask:
    """One output file: its traces in order, generated by one fresh worker process."""

    tier: int
    path: str  # relative to the tier directory
    set: str
    specs: tuple[Spec, ...]
    summary: tuple[int, int, int] | None = None  # fuzz summary chunk (seed, start, count)


def example(name: str) -> DeckList:
    return to_decklist(load_deck(EXAMPLES / f"{name}.txt"))


def test_decks() -> tuple[DeckList, DeckList]:
    """``tests/ai/decks.py`` ``DECKS`` (read-only import of the reference's test decks)."""
    if str(ROOT) not in sys.path:
        sys.path.insert(0, str(ROOT))
    module = importlib.import_module("tests.ai.decks")
    return cast(tuple[DeckList, DeckList], module.DECKS)


def runner_spec(
    set_name: str,
    preset: str,
    master: int,
    dut: str,
    bench: str,
    m: int,
    *,
    bo3: bool,
    deep: tuple[tuple[int, int], ...] = (),
) -> Spec:
    """Match ``m`` of ``gcg-sim benchmark DUT BENCH --seed MASTER --ai-preset P``: seats,
    game seed and agent seeds as ``runner/config.py:64-80``."""
    d, b = example(dut), example(bench)
    decks = (d, b) if m % 2 == 0 else (b, d)
    fmt = "bo3" if bo3 else "bo1"
    tid = f"{set_name}/{preset}/s{master}/{SHORT[dut]}-{SHORT[bench]}/m{m}/{fmt}"
    return Spec(
        set=set_name,
        id=tid,
        kind="search",
        decks=decks,
        seed=game_seed(master, m, 0),
        chooser=None,
        agents=(f"mcts:{preset}", f"mcts:{preset}"),
        agent_seeds=(agent_seed(master, m, 0, 0), agent_seed(master, m, 0, 1)),
        preset=preset,
        agent_blocks=True,
        deep=deep,
        bo3=bo3,
        master=master,
        match=m,
    )


def _ordered_pairs() -> list[tuple[str, str]]:
    return [(a, b) for a in EXAMPLE_DECKS for b in EXAMPLE_DECKS if a != b]


def specs_full() -> list[FileTask]:
    specs: list[Spec] = []
    for j, (c0, c1) in enumerate((("Red", "Blue"), ("Green", "White"))):
        seed = derive_seed(TRACE_MASTER, "full", "vanilla", j)
        specs.append(
            Spec(
                set="full",
                id=f"full/vanilla-{c0.lower()}-{c1.lower()}",
                kind="game",
                decks=(vanilla_deck(c0), vanilla_deck(c1)),
                seed=seed,
                chooser=None,
                agents=("random", "random"),
                agent_seeds=(derive_seed(seed, "agent", 0), derive_seed(seed, "agent", 1)),
                snapshots="all",
            )
        )
    for j, (a, b) in enumerate(
        (("red-green-zeon", "blue-white-federation"), ("purple-white-tekkadan", "red-green-zeon"))
    ):
        seed = derive_seed(TRACE_MASTER, "full", "examples", j)
        specs.append(
            Spec(
                set="full",
                id=f"full/{SHORT[a]}-{SHORT[b]}",
                kind="game",
                decks=(example(a), example(b)),
                seed=seed,
                chooser=None,
                agents=("policy", "policy"),
                agent_seeds=(derive_seed(seed, "policy", 0), derive_seed(seed, "policy", 1)),
                snapshots="all",
            )
        )
    return [FileTask(0, "full/full.ndjson", "full", tuple(specs))]


def specs_vanilla() -> list[FileTask]:
    specs: list[Spec] = []
    for c0 in COLORS:
        for c1 in COLORS:
            if c0 == c1:
                continue
            for g in range(2):
                seed = derive_seed(TRACE_MASTER, "vanilla", c0, c1, g)
                specs.append(
                    Spec(
                        set="vanilla",
                        id=f"vanilla/{c0.lower()}-{c1.lower()}/g{g}",
                        kind="game",
                        decks=(vanilla_deck(c0), vanilla_deck(c1)),
                        seed=seed,
                        chooser=None if g == 0 else g % 2,
                        agents=("random", "random"),
                        agent_seeds=(derive_seed(seed, "agent", 0), derive_seed(seed, "agent", 1)),
                        stop="first_main",
                    )
                )
    return [FileTask(0, "vanilla/vanilla.ndjson", "vanilla", tuple(specs))]


def fuzz_spec(seed: int, index: int, options_mode: str) -> Spec:
    """``tools/fuzz.py:83-119`` game ``index`` of ``seed``."""
    s = derive_seed(seed, "fuzz-agent", index)
    return Spec(
        set="fuzz",
        id=f"fuzz/s{seed}/g{index:04d}",
        kind="game",
        decks=(deck_for(2 * index, seed), deck_for(2 * index + 1, seed)),
        seed=derive_seed(seed, "fuzz-game", index),
        chooser=None,
        agents=("fuzz", "fuzz"),
        agent_seeds=(s, s),
        options_mode=options_mode,
        fuzz_check=True,
    )


def specs_fuzz(tier: int) -> list[FileTask]:
    tasks: list[FileTask] = []
    groups = [(1, range(60)), (5, range(40))] if tier == 1 else [(1, range(500))]
    mode = "full" if tier == 1 else "hash"
    chunk = 20 if tier == 1 else 50
    for seed, indices in groups:
        idx = list(indices)
        for start in range(0, len(idx), chunk):
            part = idx[start : start + chunk]
            specs = tuple(fuzz_spec(seed, i, mode) for i in part)
            path = f"fuzz/s{seed}-g{part[0]:04d}-{part[-1]:04d}.ndjson"
            tasks.append(FileTask(0, path, "fuzz", specs))
    if tier == 1:
        for start in range(0, FUZZ_SUMMARY_GAMES, FUZZ_SUMMARY_CHUNK):
            tasks.append(
                FileTask(
                    0,
                    f"fuzz/summary-part-{start:05d}",
                    "fuzz",
                    (),
                    summary=(FUZZ_SUMMARY_SEED, start, FUZZ_SUMMARY_CHUNK),
                )
            )
    return tasks


def coverage_game_specs(tier: int) -> list[Spec]:
    specs: list[Spec] = []
    for a, b in _ordered_pairs():
        for g in range(2):
            seed = derive_seed(TRACE_MASTER, "coverage", "examples", a, b, g)
            specs.append(
                Spec(
                    set="coverage",
                    id=f"coverage/examples/{SHORT[a]}-{SHORT[b]}/g{g}",
                    kind="game",
                    decks=(example(a), example(b)),
                    seed=seed,
                    chooser=None,
                    agents=("policy", "policy"),
                    agent_seeds=(derive_seed(seed, "policy", 0), derive_seed(seed, "policy", 1)),
                )
            )
    decks = coverage_decks.covering_decks()
    games = 1 if tier == 1 else 3
    for i, j in coverage_decks.pairings(decks):
        for g in range(games):
            seed = derive_seed(TRACE_MASTER, "coverage", decks[i].name, decks[j].name, g)
            specs.append(
                Spec(
                    set="coverage",
                    id=f"coverage/cover/{decks[i].name}-{decks[j].name}/g{g}",
                    kind="game",
                    decks=(decks[i].deck, decks[j].deck),
                    seed=seed,
                    chooser=None,
                    agents=("policy", "policy"),
                    agent_seeds=(derive_seed(seed, "policy", 0), derive_seed(seed, "policy", 1)),
                )
            )
    return specs


def _chunks(set_name: str, specs: list[Spec], size: int, stem: str) -> list[FileTask]:
    return [
        FileTask(0, f"{set_name}/{stem}-{k:03d}.ndjson", set_name, tuple(specs[s : s + size]))
        for k, s in enumerate(range(0, len(specs), size))
    ]


def specs_coverage(tier: int) -> list[FileTask]:
    return _chunks("coverage", coverage_game_specs(tier), 12, "coverage")


def specs_search(tier: int) -> list[FileTask]:
    tasks: list[FileTask] = []
    if tier == 1:
        plan = {
            "fast": [
                ("red-green-zeon", "blue-white-federation", 0),
                ("red-green-zeon", "blue-white-federation", 1),
                ("purple-white-tekkadan", "red-green-zeon", 0),
                ("blue-white-federation", "purple-white-tekkadan", 1),
            ],
            "standard": [
                ("red-green-zeon", "blue-white-federation", 0),
                ("purple-white-tekkadan", "red-green-zeon", 1),
            ],
            "strong": [("red-green-zeon", "blue-white-federation", 0)],
        }
        for preset, games in plan.items():
            specs = tuple(
                runner_spec("search", preset, SEARCH_MASTER, a, b, m, bo3=False)
                for a, b, m in games
            )
            tasks.append(FileTask(0, f"search/{preset}.ndjson", "search", specs))
        decks = test_decks()
        base = Spec(
            set="search",
            id=f"search/baseline-standard/{BASELINE_GAME_SEED}",
            kind="search",
            decks=decks,
            seed=BASELINE_GAME_SEED,
            chooser=None,
            agents=("baseline:standard", "baseline:standard"),
            agent_seeds=(BASELINE_AGENT_SEED, BASELINE_AGENT_SEED + 1),
            preset="baseline-standard",
            agent_blocks=True,
        )
        accept = dataclasses.replace(
            base,
            id=f"search/acceptance/standard/{BASELINE_GAME_SEED}",
            agents=("mcts:standard", "mcts:standard"),
            preset="standard",
            deep=ACCEPTANCE_SEARCHES,
        )
        tasks.append(FileTask(0, "search/baseline.ndjson", "search", (base,)))
        tasks.append(FileTask(0, "search/acceptance.ndjson", "search", (accept,)))
        return tasks
    for preset in ("fast", "standard", "strong"):
        for master in (1, 2, 3):
            for a, b in _ordered_pairs():
                spec = runner_spec("search", preset, master, a, b, 0, bo3=True)
                path = f"search/{preset}-s{master}-{SHORT[a]}-{SHORT[b]}.ndjson"
                tasks.append(FileTask(0, path, "search", (spec,)))
    return tasks


def specs_baselines() -> list[FileTask]:
    specs: list[Spec] = []
    pairs = [
        ("red-green-zeon", "blue-white-federation"),
        ("blue-white-federation", "purple-white-tekkadan"),
        ("purple-white-tekkadan", "red-green-zeon"),
        ("blue-white-federation", "red-green-zeon"),
    ]
    for kind in ("greedy", "random"):
        for j, (a, b) in enumerate(pairs):
            seed = derive_seed(TRACE_MASTER, "baselines", kind, j)
            specs.append(
                Spec(
                    set="baselines",
                    id=f"baselines/{kind}/{SHORT[a]}-{SHORT[b]}",
                    kind="search",
                    decks=(example(a), example(b)),
                    seed=seed,
                    chooser=None,
                    agents=(kind, kind),
                    agent_seeds=(derive_seed(seed, kind, 0), derive_seed(seed, kind, 1)),
                    agent_blocks=True,
                )
            )
    return [FileTask(0, "baselines/baselines.ndjson", "baselines", tuple(specs))]


def _source_specs(tier: int) -> list[Spec]:
    """Games whose states the ``views`` and ``worlds`` sets sample (coverage, then fuzz)."""
    out = coverage_game_specs(tier)
    out += [fuzz_spec(1, i, "full") for i in range(20 if tier == 1 else 100)]
    return out


def specs_derived(set_name: str, tier: int) -> list[FileTask]:
    """``views`` (per-state view checks) and ``worlds`` (determinised worlds): each task
    regenerates a few source games and samples decisions from them."""
    per_game = {("views", 1): 4, ("views", 2): 9, ("worlds", 1): 2, ("worlds", 2): 8}[
        (set_name, tier)
    ]
    sources = _source_specs(tier) if set_name == "views" else coverage_game_specs(tier)
    specs: list[Spec] = []
    for src in sources:
        for j in range(per_game):
            specs.append(
                dataclasses.replace(
                    src,
                    set=set_name,
                    id=f"{set_name}/{src.id}/k{j}",
                    kind="game",
                    seed=None,
                    chooser=None,
                    source=src.id,
                    sample_seed=derive_seed(TRACE_MASTER, set_name, src.id, j),
                    fuzz_check=False,
                    options_mode="full",
                )
            )
    size = 8 * per_game
    return _chunks(set_name, specs, size, set_name)


def build_tasks(tier: int, only: Sequence[str] = SETS) -> list[FileTask]:
    tasks: list[FileTask] = []
    for name in only:
        if name == "full" and tier == 1:
            tasks += specs_full()
        elif name == "vanilla" and tier == 1:
            tasks += specs_vanilla()
        elif name == "fuzz":
            tasks += specs_fuzz(tier)
        elif name == "coverage":
            tasks += specs_coverage(tier)
        elif name == "search":
            tasks += specs_search(tier)
        elif name == "baselines" and tier == 1:
            tasks += specs_baselines()
        elif name in ("views", "worlds"):
            tasks += specs_derived(name, tier)
    return [dataclasses.replace(t, tier=tier) for t in tasks]


def all_specs(tier: int) -> dict[str, Spec]:
    return {s.id: s for t in build_tasks(tier) for s in t.specs}


# ---------------------------------------------------------------------------------------------
# Capture of agent internals (transparent wrappers)


@dataclass
class IterCapture:
    world16: str = ""
    applies_walk: int = 0
    applies_rollout: int = 0
    applies: list[tuple[Action, str]] = field(default_factory=list)
    path_len: int = 1
    leaf: float = 0.0
    terminal: bool = False


@dataclass
class Capture:
    """What one agent decision computed besides its action (PLAN §8.2 ``agent``)."""

    rng: SplitMix64 | None = None
    k: int = 0
    seed: int = 0
    alternatives: list[Alternative] | None = None
    searched: bool = False
    pruned: list[Any] | None = None
    idle: Any = None
    merged: list[Any] | None = None
    iterations: int = 0
    budget: int = 0
    cap: int = 0
    result_keys: list[Any] | None = None
    # level 2/3 drill-down
    deep: bool = False
    apply_level: bool = False
    iteration_filter: int | None = None
    root_children: list[Any] = field(default_factory=list)
    n: int = 0
    it: IterCapture | None = None
    iters: list[dict[str, Json]] = field(default_factory=list)
    apply_traces: list[dict[str, Json]] = field(default_factory=list)
    decision_index: int = 0


CAP = Capture()


def _patch(obj: object, name: str, new: object, saved: list[tuple[object, str, object]]) -> None:
    saved.append((obj, name, getattr(obj, name)))
    setattr(obj, name, new)


@contextlib.contextmanager
def capture_installed() -> Iterator[None]:
    """Install the pass-through wrappers for one traced run and remove them afterwards."""
    saved: list[tuple[object, str, object]] = []
    begin = AgentCore._begin
    finish = AgentCore._finish
    search = vars(ai_agents)["search"]
    prune = vars(mcts)["prune_dominated"]
    idle = vars(mcts)["without_effect"]
    merge = vars(mcts)["merge_equivalent"]
    icap = mcts._iteration_cap
    result = mcts._result
    world = mcts._world
    iterate = mcts._iterate
    descend = mcts._descend
    leaf = mcts._leaf_value
    add_children = mcts._add_children
    walk_apply = vars(mcts)["apply"]
    roll_apply = vars(playout)["apply"]

    def w_begin(self: AgentCore, st: GameState, player: int) -> Any:
        dec, rng = begin(self, st, player)
        CAP.rng = rng
        CAP.k = self._decisions
        CAP.seed = derive_seed(self.seed, self.name, "decision", self._decisions)
        return dec, rng

    def w_finish(
        self: AgentCore, st: GameState, player: int, chosen: Action, alts: list[Alternative]
    ) -> Action:
        CAP.alternatives = list(alts)
        return finish(self, st, player, chosen, alts)

    def w_search(*args: Any, **kwargs: Any) -> Any:
        CAP.searched = True
        out = search(*args, **kwargs)
        CAP.iterations = out.iterations
        return out

    def w_prune(*args: Any, **kwargs: Any) -> Any:
        out = prune(*args, **kwargs)
        CAP.pruned = list(out)
        return out

    def w_idle(*args: Any, **kwargs: Any) -> Any:
        out = idle(*args, **kwargs)
        CAP.idle = out
        return out

    def w_merge(*args: Any, **kwargs: Any) -> Any:
        out = merge(*args, **kwargs)
        CAP.merged = list(out)
        return out

    def w_icap(config: Any, dec: Any, budget: int) -> int:
        out = icap(config, dec, budget)
        CAP.budget, CAP.cap = budget, out
        return out

    def w_result(root: Any, keys: Any, moves: Any, iterations: int) -> Any:
        CAP.result_keys = [key for key, _ in moves]
        return result(root, keys, moves, iterations)

    def w_add_children(node: Any, s: GameState, dec: Any, moves: Any, cfg: Any) -> None:
        add_children(node, s, dec, moves, cfg)
        if CAP.deep and not CAP.root_children:
            CAP.root_children = list(node.children.values())

    def w_world(worlds: Any, st: GameState, player: int, seeds: Any, k: int) -> GameState:
        out = world(worlds, st, player, seeds, k)
        if CAP.deep:
            CAP.it = IterCapture(world16=state_digest(worlds[k])[:16])
        return out

    def w_walk_apply(s: GameState, a: Action, *, check: bool = True) -> None:
        walk_apply(s, a, check=check)
        it = CAP.it
        if it is not None:
            it.applies_walk += 1
            _apply_level(it, s, a)

    def w_roll_apply(s: GameState, a: Action, *, check: bool = True) -> None:
        roll_apply(s, a, check=check)
        it = CAP.it
        if it is not None:
            it.applies_rollout += 1
            _apply_level(it, s, a)

    def w_descend(ctx: Any, node: Any, s: GameState, path: list[Any]) -> None:
        descend(ctx, node, s, path)
        if CAP.it is not None:
            CAP.it.path_len = len(path)

    def w_leaf(s: GameState, ctx: Any) -> float:
        value = leaf(s, ctx)
        if CAP.it is not None:
            CAP.it.leaf = value
            CAP.it.terminal = s.winner is not None
        return value

    def w_iterate(ctx: Any, first: Any, fresh: bool, s: GameState, rng: SplitMix64) -> None:
        it = CAP.it
        if it is None:
            iterate(ctx, first, fresh, s, rng)
            return
        k = first.visits
        before = first.total
        iterate(ctx, first, fresh, s, rng)
        child = next(i for i, c in enumerate(CAP.root_children) if c is first)
        n = CAP.n
        CAP.n += 1
        CAP.it = None
        if CAP.iteration_filter is not None and n != CAP.iteration_filter:
            return
        CAP.iters.append(
            {
                "t": "iter",
                "decision": CAP.decision_index,
                "n": n,
                "child": child,
                "k": k,
                "fresh": fresh,
                "path_len": it.path_len,
                "applies_walk": it.applies_walk,
                "applies_rollout": it.applies_rollout,
                "leaf": fhex(it.leaf),
                "terminal": it.terminal,
                "total_before": fhex(before),
                "total_after": fhex(first.total),
                "world16": it.world16,
            }
        )
        for j, (a, after16) in enumerate(it.applies):
            CAP.apply_traces.append(
                {
                    "t": "apply_trace",
                    "decision": CAP.decision_index,
                    "n": n,
                    "j": j,
                    "action": action_json(a),
                    "after16": after16,
                }
            )

    _patch(AgentCore, "_begin", w_begin, saved)
    _patch(AgentCore, "_finish", w_finish, saved)
    _patch(ai_agents, "search", w_search, saved)
    _patch(mcts, "prune_dominated", w_prune, saved)
    _patch(mcts, "without_effect", w_idle, saved)
    _patch(mcts, "merge_equivalent", w_merge, saved)
    _patch(mcts, "_iteration_cap", w_icap, saved)
    _patch(mcts, "_result", w_result, saved)
    _patch(mcts, "_add_children", w_add_children, saved)
    _patch(mcts, "_world", w_world, saved)
    _patch(mcts, "apply", w_walk_apply, saved)
    _patch(playout, "apply", w_roll_apply, saved)
    _patch(mcts, "_descend", w_descend, saved)
    _patch(mcts, "_leaf_value", w_leaf, saved)
    _patch(mcts, "_iterate", w_iterate, saved)
    try:
        yield
    finally:
        for obj, name, old in reversed(saved):
            setattr(obj, name, old)


def _apply_level(it: IterCapture, s: GameState, a: Action) -> None:
    if CAP.apply_level and (CAP.iteration_filter is None or CAP.n == CAP.iteration_filter):
        it.applies.append((a, state_digest(s)[:16]))


def reset_capture(decision_index: int, deep: bool) -> None:
    keep = (CAP.apply_level, CAP.iteration_filter)
    CAP.__init__()  # type: ignore[misc]
    CAP.apply_level, CAP.iteration_filter = keep
    CAP.deep = deep
    CAP.decision_index = decision_index


def agent_block(agent: AgentCore, log_before: int) -> dict[str, Json]:
    """The ``agent`` block of a decision record from the capture of the last ``choose``."""
    assert CAP.rng is not None
    assert CAP.alternatives is not None
    block: dict[str, Json] = {
        "k": CAP.k,
        "seed": u64(CAP.seed),
        "rng_after": u64(CAP.rng.state),
        "alts": [[action_json(a.action), fhex(a.value), a.visits] for a in CAP.alternatives],
        "log": agent.decision_log()[log_before:],
    }
    if CAP.searched:
        roots = CAP.pruned
        assert roots is not None
        if CAP.idle is not None:
            roots = [m for m in roots if m[0] not in CAP.idle] or roots
        if CAP.merged is not None:
            roots = CAP.merged
        keys = [list(key) for key, _ in roots]
        if CAP.result_keys is not None and [list(k) for k in CAP.result_keys] != keys:
            raise AssertionError("root-move reconstruction disagrees with mcts._result")
        block.update(iters=CAP.iterations, budget=CAP.budget, cap=CAP.cap, roots=keys)
    return block


# ---------------------------------------------------------------------------------------------
# Players


class Player(Protocol):
    """One seat's chooser. ``choose`` returns the action and, for agents with blocks, the
    agent block; ``plain`` makes the same choice without any capture (the plain run)."""

    name: str

    def choose(self, st: GameState, i: int, deep: bool) -> tuple[Action, dict[str, Json] | None]:
        """Choose with capture (traced run)."""
        ...

    def plain(self, st: GameState) -> Action:
        """Choose without capture (plain run)."""
        ...


class SharedRandom:
    """``tools/fuzz.py:87,96-97``: one generator for both seats, ``opts[randrange(n)]``."""

    def __init__(self, rng: SplitMix64) -> None:
        self.name = "fuzz"
        self.rng = rng

    def choose(self, st: GameState, i: int, deep: bool) -> tuple[Action, dict[str, Json] | None]:
        return self.plain(st), None

    def plain(self, st: GameState) -> Action:
        opts = st.pending.options if st.pending else ()
        return opts[self.rng.randrange(len(opts))]


class PolicyPlayer:
    """``policy.choose(st, rng, 0.5, BASELINE_KNOBS)`` (§8.3 coverage), the rollout policy."""

    def __init__(self, rng: SplitMix64) -> None:
        self.name = "policy"
        self.rng = rng

    def choose(self, st: GameState, i: int, deep: bool) -> tuple[Action, dict[str, Json] | None]:
        return self.plain(st), None

    def plain(self, st: GameState) -> Action:
        return policy.choose(st, self.rng, POLICY_TEMPERATURE, BASELINE_KNOBS)


class CorePlayer:
    """A ``gcg_sim.ai`` agent (random, greedy, MCTS); with ``blocks`` its decisions carry the
    agent block (drill-down level 1)."""

    def __init__(self, agent: AgentCore, blocks: bool) -> None:
        self.agent = agent
        self.name = agent.name
        self.blocks = blocks

    def choose(self, st: GameState, i: int, deep: bool) -> tuple[Action, dict[str, Json] | None]:
        if not self.blocks:
            return self.plain(st), None
        reset_capture(i, deep)
        log_before = len(self.agent.decision_log())
        action = self.plain(st)
        return action, agent_block(self.agent, log_before)

    def plain(self, st: GameState) -> Action:
        dec = st.pending
        assert dec is not None
        return cast(Action, cast(Any, self.agent).choose(st, dec.player))


def make_core_agent(kind: str, seed: int) -> AgentCore:
    if kind == "random":
        return cast(AgentCore, ai_agents.RandomAgent(seed, log=True))
    if kind == "greedy":
        return cast(AgentCore, ai_agents.GreedyAgent(seed, log=True, weights=DEFAULT_WEIGHTS))
    if kind.startswith("mcts:"):
        preset = kind.split(":", 1)[1]
        return cast(
            AgentCore,
            ai_agents.MctsAgent(seed, config=PRESETS[preset], log=True, weights=DEFAULT_WEIGHTS),
        )
    if kind.startswith("baseline:"):
        preset = kind.split(":", 1)[1]
        return cast(
            AgentCore,
            ai_agents.MctsAgent(
                seed, config=BASELINE_PRESETS[preset], log=True, weights=BASELINE_WEIGHTS
            ),
        )
    raise ValueError(f"unknown agent kind {kind!r}")


def make_players(spec: Spec, game_index: int = 0) -> tuple[Player, Player]:
    k0, k1 = spec.agents
    s0, s1 = spec.agent_seeds
    if k0 == "fuzz":
        shared = SharedRandom(SplitMix64(s0))
        return shared, shared
    if k0 == "policy":
        return PolicyPlayer(SplitMix64(s0)), PolicyPlayer(SplitMix64(s1))
    return (
        CorePlayer(make_core_agent(k0, s0), spec.agent_blocks),
        CorePlayer(make_core_agent(k1, s1), spec.agent_blocks),
    )


# ---------------------------------------------------------------------------------------------
# Generation


@dataclass(frozen=True, slots=True)
class Gen:
    """Generator block fields that are the same for every trace of a run (§8.2)."""

    git: str
    python: str
    code_digest: str
    ir_sha256: str
    data_lock: str
    hash_seed: int
    effect_offset: int

    def block(self, max_uid: int) -> dict[str, Json]:
        return {**dataclasses.asdict(self), "max_uid": max_uid}


def header_record(
    spec: Spec,
    gen: Gen,
    max_uid: int,
    seed: int | None,
    chooser: int | None,
    agent_names: tuple[str, ...],
    agent_seeds: tuple[int, ...],
    tid: str,
) -> dict[str, Json]:
    return {
        "t": "header",
        "v": TRACE_VERSION,
        "id": tid,
        "kind": spec.kind,
        "generator": gen.block(max_uid),
        "decks": [deck_json(spec.decks[0]), deck_json(spec.decks[1])],
        "seed": None if seed is None else u64(seed),
        "chooser": chooser,
        "turn_limit": spec.turn_limit,
        "max_actions": spec.max_actions,
        "agents": list(agent_names),
        "agent_seeds": [u64(s) for s in agent_seeds],
        "preset": spec.preset,
        "options_mode": spec.options_mode,
    }


@dataclass
class Played:
    """One generated trace: its records (header last-filled) and what the plain run compares."""

    lines: list[str]
    actions: list[list[Json]]
    final_digest: str
    winner: int | None
    end_reason: EndReason | None
    loser_seat: int | None
    turn: int = 0


def _play(
    spec: Spec,
    gen: Gen | None,
    st: GameState,
    players: tuple[Player, Player],
    tid: str,
    seed: int | None,
    chooser: int | None,
    agent_seeds: tuple[int, int],
) -> Played:
    """Drive ``st`` to the end (or the first MAIN decision) and record the trace. With
    ``gen=None`` it is the plain run: no records, no wrappers, the same choices."""
    records: list[dict[str, Json]] = []
    traced = gen is not None
    init = initial_multiset(st) if spec.fuzz_check else None
    if traced:
        records.append(snapshot_record(st, 0))
    actions: list[list[Json]] = []
    i = 0
    last_snapshot = 0
    agent_k = [0, 0]
    while st.winner is None and st.pending is not None:
        dec = st.pending
        if spec.stop == "first_main" and dec.kind is DecisionKind.MAIN:
            break
        p = dec.player
        agent_k[p] += 1
        deep = (p, agent_k[p]) in spec.deep
        block: dict[str, Json] | None = None
        if traced:
            fp = fingerprint(st)
            action, block = players[p].choose(st, i, deep)
        else:
            action = players[p].plain(st)
        if st.pending is not dec:
            raise AssertionError(f"{tid}: the agent replaced the pending decision")
        if traced:
            if fingerprint(st) != fp:
                raise AssertionError(f"{tid}: the agent changed the state at decision {i}")
            rec = decision_record(st, i, action, fp, spec.options_mode)
            if block is not None:
                rec["agent"] = block
            records.append(rec)
            if deep:
                records.extend(CAP.iters)
                records.extend(CAP.apply_traces)
        actions.append(action_json(action))
        apply(st, action)
        if init is not None:
            check_invariants(st, init)
        i += 1
        if traced and (spec.snapshots == "all" or i % SNAPSHOT_EVERY == 0):
            records.append(snapshot_record(st, i))
            last_snapshot = i
    if init is not None:
        check_invariants(st, init)
    if traced and last_snapshot != i:
        records.append(snapshot_record(st, i))
    final = state_digest(st)
    loser: int | None = None
    if st.winner is not None and st.winner >= 0:
        loser = 1 - st.winner
    elif st.winner == -1 and st.end_reason is EndReason.BOTH_DEFEATED:
        loser = st.active
    lines: list[str] = []
    if gen is not None:
        records.append(outcome_record(st, i))
        names = (players[0].name, players[1].name)
        head = header_record(spec, gen, len(st.cards) - 1, seed, chooser, names, agent_seeds, tid)
        lines = [dumps_line(head)] + [dumps_line(r) for r in records]
    return Played(lines, actions, final, st.winner, st.end_reason, loser, st.turn)


def _start(spec: Spec, seed: int, chooser: int | None) -> GameState:
    return new_game(
        spec.decks, seed, chooser=chooser, turn_limit=spec.turn_limit, max_actions=spec.max_actions
    )


def generate_game(spec: Spec, gen: Gen | None) -> list[Played]:
    """The trace(s) of a game spec: one game, or a BO3 match (``runner/match.py:19-46``)."""
    assert spec.seed is not None
    if not spec.bo3:
        st = _start(spec, spec.seed, spec.chooser)
        with capture_installed() if gen is not None else contextlib.nullcontext():
            return [
                _play(
                    spec,
                    gen,
                    st,
                    make_players(spec),
                    spec.id,
                    spec.seed,
                    spec.chooser,
                    spec.agent_seeds,
                )
            ]
    # BO3: game g of match m; the next chooser is the previous game's match-scoring loser.
    out: list[Played] = []
    master, m = spec.master, spec.match
    chooser: int | None = None
    wins = [0, 0]
    for g in range(3):
        seed = game_seed(master, m, g)
        seeds = (agent_seed(master, m, g, 0), agent_seed(master, m, g, 1))
        gspec = dataclasses.replace(spec, agent_seeds=seeds)
        st = _start(spec, seed, chooser)
        tid = f"{spec.id}/g{g}"
        with capture_installed() if gen is not None else contextlib.nullcontext():
            played = _play(gspec, gen, st, make_players(gspec), tid, seed, chooser, seeds)
        out.append(played)
        if played.loser_seat is not None:
            wins[1 - played.loser_seat] += 1
        if max(wins) >= 2:
            break
        chooser = played.loser_seat
    return out


def _source_states(src: Spec) -> list[GameState]:
    """Every decision state of the source game (clones), replayed with its own players."""
    assert src.seed is not None
    st = _start(src, src.seed, src.chooser)
    players = make_players(src)
    states: list[GameState] = []
    init = initial_multiset(st) if src.fuzz_check else None
    while st.winner is None and st.pending is not None:
        states.append(st.clone())
        apply(st, players[st.pending.player].plain(st))
        if init is not None:
            check_invariants(st, init)
    return states


def _pick(n: int, sample_seed: int) -> int:
    """The sampled decision index in ``1..n-1`` (``0`` for a one-decision game)."""
    return 1 + SplitMix64(sample_seed).randrange(n - 1) if n > 1 else 0


def generate_world(spec: Spec, gen: Gen | None, src_states: list[GameState]) -> Played:
    """A determinised world (``engine/observe.py:56-83``) of a sampled source decision,
    played to the end by the rollout policy at T = 0.5 with one seeded generator."""
    k = _pick(len(src_states), spec.sample_seed)
    base = src_states[k]
    assert base.pending is not None
    observer = base.pending.player
    world = determinize(base, observer, spec.sample_seed)
    rollout_seed = derive_seed(spec.sample_seed, "world-rollout")
    rollout = PolicyPlayer(SplitMix64(rollout_seed))
    tid = f"{spec.id}/d{k}/p{observer}"
    return _play(
        spec, gen, world, (rollout, rollout), tid, None, None, (rollout_seed, rollout_seed)
    )


def view_signature_json(st: GameState, dv: V.Derived) -> dict[str, Json]:
    """``tests/paranoid.py:24-39``'s view signature as JSON: dict keys are strings (sorted when
    written), enums are values, rule instances are ``[rule node, controller, source, key,
    aux]`` with a lasting rule key ``(-1, effect_key, serial)`` (``engine/view.py:451``) written
    as ``[-1, content id, serial]``."""
    reg = get_registry()

    def key_json(key: tuple[int, ...]) -> list[Json]:
        if len(key) == 3 and key[0] == LASTING_SOURCE:
            return [key[0], content_id(reg.continuous[key[1]]), key[2]]
        return list(key)

    return {
        "ap": {str(u): v for u, v in dv.ap.items()},
        "hp": {str(u): v for u, v in dv.hp.items()},
        "kw": {str(u): {k.value: n for k, n in kws.items()} for u, kws in dv.kw.items()},
        "traits": {str(u): list(t) for u, t in dv.traits.items()},
        "rules": {
            str(u): [
                [node_json(r.rule), r.controller, r.source, key_json(r.key), r.aux] for r in rs
            ]
            for u, rs in dv.rules.items()
        },
        "cost_mod": {str(u): v for u, v in dv.cost_mod.items()},
        "level_mod": {str(u): v for u, v in dv.level_mod.items()},
        "linked": sorted(dv.linked),
        "abilities": {str(u): [a.aid for a in ab] for u, ab in dv.abilities.items()},
    }


VIEW_SKIP_ZONES = (Zone.DECK, Zone.RESOURCE_DECK, Zone.OUTSIDE)


def generate_view(spec: Spec, gen: Gen, src_states: list[GameState]) -> Played:
    """One sampled state: the snapshot, its view signature, the per-card queries
    (``views.cards``: uid, ``ap_of``, ``hp_of``, ``keywords``, ``traits_of``, ``play_cost``,
    ``play_level`` for every card outside the decks) and the trigger index."""
    k = _pick(len(src_states), spec.sample_seed)
    st = src_states[k].clone()
    st._derived = None
    snap = snapshot_record(st, 0)
    dv = V.derived(st)
    sig = view_signature_json(st, dv)
    cards = [
        [
            uid,
            V.ap_of(st, dv, uid),
            V.hp_of(st, dv, uid),
            {kw.value: n for kw, n in V.keywords(dv, uid).items()},
            list(V.traits_of(st, dv, uid)),
            V.play_cost(st, dv, uid),
            V.play_level(st, dv, uid),
        ]
        for uid, c in enumerate(st.cards)
        if c.zone not in VIEW_SKIP_ZONES
    ]
    index = {
        ev.value: [[uid, a.aid] for uid, a in hosts]
        for ev, hosts in V.trigger_index(st, V.derived(st)).items()
    }
    tid = f"{spec.id}/d{k}"
    head = header_record(spec, gen, len(st.cards) - 1, None, None, (), (), tid)
    recs = [
        snap,
        {"t": "view", "sig": sig},
        {"t": "probe", "fn": "views.cards", "args": [], "result": cards},
        {"t": "probe", "fn": "V.trigger_index", "args": [], "result": index},
    ]
    lines = [dumps_line(head)] + [dumps_line(r) for r in recs]
    return Played(lines, [], state_digest(st), None, None, None)


# ---------------------------------------------------------------------------------------------
# Worker tasks


@dataclass(frozen=True, slots=True)
class TaskResult:
    path: str
    lines: list[str]
    plain_mismatches: list[str]
    summary_games: list[dict[str, Json]]


def install_nocache() -> None:
    """Replace ``view.derived`` with always-compute in every ``gcg_sim`` module namespace
    (Inv 11 §7, ``nocache.py``)."""
    original = V.derived

    def derived_nocache(st: GameState) -> V.Derived:
        dv = V._compute(st)
        dv.key = V._view_key(st)
        st._derived = dv
        return dv

    for name, mod in list(sys.modules.items()):
        if not name.startswith("gcg_sim") or mod is None:
            continue
        for attr, val in list(vars(mod).items()):
            if val is original:
                setattr(mod, attr, derived_nocache)


def run_task(task: FileTask, gen_fields: dict[str, Json], nocache: bool, plain: bool) -> TaskResult:
    """Generate one file in this (fresh) process."""
    if nocache:
        install_nocache()
    gen = Gen(
        **gen_fields,
        hash_seed=int(os.environ.get("PYTHONHASHSEED", "0") or 0),
        effect_offset=int(os.environ.get(EFFECT_OFFSET_ENV, "0") or 0),
    )
    get_registry()
    if task.summary is not None:
        seed, start, count = task.summary
        games = [_summary_row(play_one(i, seed)) for i in range(start, start + count)]
        return TaskResult(task.path, [], [], games)
    lines: list[str] = []
    mismatches: list[str] = []
    sources: dict[str, list[GameState]] = {}
    src_specs = (
        {s.id: s for s in _source_specs(task.tier)} if task.set in ("views", "worlds") else {}
    )
    for spec in task.specs:
        if spec.source is not None:
            states = sources.get(spec.source)
            if states is None:
                sources.clear()
                states = sources[spec.source] = _source_states(src_specs[spec.source])
            if spec.set == "views":
                lines += generate_view(spec, gen, states).lines
                continue
            traced = generate_world(spec, gen, states)
            lines += traced.lines
            if plain:
                mismatches += _compare_plain(
                    spec.id, [traced], [generate_world(spec, None, states)]
                )
            continue
        played = generate_game(spec, gen)
        for p in played:
            lines += p.lines
        if plain:
            mismatches += _compare_plain(spec.id, played, generate_game(spec, None))
            if spec.set == "fuzz":
                mismatches += _compare_fuzz(spec, played[0])
    return TaskResult(task.path, lines, mismatches, [])


def _compare_fuzz(spec: Spec, played: Played) -> list[str]:
    """A fuzz trace against ``tools/fuzz.play_one`` itself (actions, end, turns)."""
    _, seed, index = spec.id.split("/")
    o = play_one(int(index[1:]), int(seed[1:]))
    want = (o.get("actions"), o.get("end"), o.get("turns"))
    end = played.end_reason.value if played.end_reason is not None else "none"
    got = (len(played.actions), end, played.turn)
    if not o.get("ok") and o.get("end") is None:
        return [f"{spec.id}: tools/fuzz.play_one failed: {o.get('error')}"]
    return [] if want == got else [f"{spec.id}: play_one gives {want}, the trace {got}"]


def _compare_plain(tid: str, traced: list[Played], plain: list[Played]) -> list[str]:
    out: list[str] = []
    if len(traced) != len(plain):
        return [f"{tid}: {len(traced)} traced games, {len(plain)} plain games"]
    for g, (a, b) in enumerate(zip(traced, plain, strict=True)):
        if a.actions != b.actions:
            n = next(
                (j for j, (x, y) in enumerate(zip(a.actions, b.actions, strict=False)) if x != y),
                min(len(a.actions), len(b.actions)),
            )
            out.append(f"{tid} game {g}: actions differ from the plain run at action {n}")
        elif a.final_digest != b.final_digest:
            out.append(f"{tid} game {g}: final digest differs from the plain run")
    return out


def _summary_row(o: dict[str, Json]) -> dict[str, Json]:
    keys = ("index", "seed", "ok", "actions", "end", "turns", "error")
    return {k: o[k] for k in keys if k in o}


def fuzz_summary_text(games: list[dict[str, Json]]) -> str:
    """``python -m gcg_sim.tools.fuzz --games N --seed S`` stdout (``tools/fuzz.py:149-159``),
    rebuilt from the per-game rows exactly as ``run`` aggregates them (``:130-137``)."""
    from collections import Counter

    from gcg_sim.tools.fuzz import main_deck_pool, resource_pool

    actions = 0
    ends: Counter[str] = Counter()
    failures = 0
    seen: set[str] = set()
    for row in games:
        d0 = deck_for(2 * row["index"], FUZZ_SUMMARY_SEED)
        d1 = deck_for(2 * row["index"] + 1, FUZZ_SUMMARY_SEED)
        seen.update(set(d0.main) | set(d1.main) | set(d0.resources) | set(d1.resources))
        if row.get("ok"):
            actions += int(row["actions"])
            ends[str(row["end"])] += 1
        else:
            failures += 1
    pool = {n for ns in main_deck_pool().values() for n in ns} | set(resource_pool())
    summary = {
        "games": len(games),
        "actions": actions,
        "ends": dict(ends),
        "failures": failures,
        "cards_covered": len(seen & pool),
        "cards_in_pool": len(pool),
        "cards_missing": sorted(pool - seen)[:20],
    }
    return json.dumps(summary, indent=1) + "\n"


# ---------------------------------------------------------------------------------------------
# Runs, manifests and checks


def sha256_file(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def python_git() -> str:
    """The last commit that touched the Python reference (stable across port commits)."""
    out = subprocess.run(
        ["git", "-C", str(ROOT), "log", "-1", "--format=%H", "--", "src/gcg_sim"],
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()
    dirty = subprocess.run(
        ["git", "-C", str(ROOT), "status", "--porcelain", "--", "src/gcg_sim"],
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()
    return out + ("+dirty" if dirty else "")


def gen_fields() -> dict[str, Json]:
    return {
        "git": python_git(),
        "python": platform.python_version(),
        "code_digest": code_digest(),
        "ir_sha256": sha256_file(IR_PATH) if IR_PATH.exists() else "absent",
        "data_lock": sha256_file(DATA_LOCK),
    }


def run_all(
    tasks: list[FileTask],
    *,
    hash_seed: int,
    effect_offset: int,
    nocache: bool,
    plain: bool,
    workers: int,
) -> dict[str, TaskResult]:
    """Run every task in a fresh spawned process under ``PYTHONHASHSEED=hash_seed``."""
    env_saved = {k: os.environ.get(k) for k in ("PYTHONHASHSEED", EFFECT_OFFSET_ENV, "PYTHONPATH")}
    os.environ["PYTHONHASHSEED"] = str(hash_seed)
    if effect_offset:
        os.environ[EFFECT_OFFSET_ENV] = str(effect_offset)
        os.environ["PYTHONPATH"] = os.pathsep.join(
            [str(OFFSET_HOOK), *filter(None, [env_saved["PYTHONPATH"]])]
        )
    else:
        os.environ.pop(EFFECT_OFFSET_ENV, None)
    fields = gen_fields()
    results: dict[str, TaskResult] = {}
    order = sorted(tasks, key=lambda t: -_cost(t))
    try:
        ctx = multiprocessing.get_context("spawn")
        with concurrent.futures.ProcessPoolExecutor(
            max_workers=workers, mp_context=ctx, max_tasks_per_child=1
        ) as ex:
            futs = {ex.submit(run_task, t, fields, nocache, plain): t for t in order}
            for fut in concurrent.futures.as_completed(futs):
                res = fut.result()
                results[res.path] = res
                print(f"  {res.path}", file=sys.stderr, flush=True)
    finally:
        for k, v in env_saved.items():
            if v is None:
                os.environ.pop(k, None)
            else:
                os.environ[k] = v
    return results


def _cost(t: FileTask) -> int:
    if t.set == "search":
        weight = {"fast": 1, "standard": 4, "strong": 10, "baseline-standard": 2}
        return sum(weight.get(s.preset or "", 1) * (3 if s.bo3 else 1) for s in t.specs) * 100
    return len(t.specs) + (5 if t.summary else 0)


def assemble(results: dict[str, TaskResult], tasks: list[FileTask]) -> dict[str, bytes]:
    """Output files (path -> bytes) from the task results; fuzz summary parts are merged."""
    files: dict[str, bytes] = {}
    summary: list[dict[str, Json]] = []
    for t in tasks:
        res = results[t.path]
        if t.summary is not None:
            summary += res.summary_games
            continue
        files[t.path] = "".join(line + "\n" for line in res.lines).encode()
    if summary:
        summary.sort(key=lambda r: int(r["index"]))
        n = len(summary)
        stem = f"fuzz/summary-{FUZZ_SUMMARY_SEED}-{n}"
        files[f"{stem}.json"] = fuzz_summary_text(summary).encode()
        files[f"{stem}.games.jsonl"] = "".join(dumps_line(r) + "\n" for r in summary).encode()
    return files


def normalized(data: bytes, hash_seed: int | None, effect_offset: int | None) -> bytes:
    """``data`` with every header's ``hash_seed``/``effect_offset`` set to the given values
    (run parameters, not outputs), so runs under different seeds compare byte for byte."""
    if hash_seed is None and effect_offset is None:
        return data
    out: list[str] = []
    for line in data.decode().splitlines():
        if '"t":"header"' in line:
            rec = json.loads(line)
            if rec.get("t") == "header":
                if hash_seed is not None:
                    rec["generator"]["hash_seed"] = hash_seed
                if effect_offset is not None:
                    rec["generator"]["effect_offset"] = effect_offset
                line = dumps_line(rec)
        out.append(line)
    return "".join(line + "\n" for line in out).encode()


def count_records(data: bytes) -> tuple[int, int]:
    """(records, traces) of an NDJSON trace file."""
    lines = data.splitlines()
    return len(lines), sum(1 for line in lines if b'"t":"header"' in line)


def first_generator(data: bytes) -> dict[str, Json] | None:
    for line in data.splitlines():
        if b'"t":"header"' in line:
            gen = dict(json.loads(line)["generator"])
            gen.pop("max_uid", None)
            return gen
    return None


def set_of(path: str) -> str:
    return path.split("/", 1)[0]


def manifest_entry(path: str, data: bytes) -> dict[str, Json]:
    entry: dict[str, Json] = {
        "path": path,
        "set": set_of(path),
        "sha256": hashlib.sha256(data).hexdigest(),
        "bytes": len(data),
    }
    if path.endswith(".ndjson"):
        records, traces = count_records(data)
        entry.update(records=records, traces=traces, generator=first_generator(data))
    return entry


def tier_files(tier: int) -> dict[str, bytes]:
    base = TIER_DIRS[tier]
    if not base.exists():
        return {}
    return {
        p.relative_to(base).as_posix(): p.read_bytes()
        for p in sorted(base.rglob("*"))
        if p.is_file() and not p.name.startswith(".")
    }


def write_manifest(tier: int, files: dict[str, bytes]) -> None:
    entries = [manifest_entry(p, files[p]) for p in sorted(files)]
    doc = {
        "format": MANIFEST_FORMAT,
        "tier": tier,
        "root": TIER_DIRS[tier].relative_to(ROOT).as_posix(),
        "files": entries,
        "totals": {
            "files": len(entries),
            "bytes": sum(e["bytes"] for e in entries),
            "records": sum(e.get("records", 0) for e in entries),
            "traces": sum(e.get("traces", 0) for e in entries),
        },
    }
    MANIFESTS[tier].write_text(json.dumps(doc, indent=1, sort_keys=True) + "\n")


def load_manifest(tier: int) -> dict[str, dict[str, Json]]:
    path = MANIFESTS[tier]
    if not path.exists():
        return {}
    doc = json.loads(path.read_text())
    return {e["path"]: e for e in doc["files"]}


def first_diff(a: bytes, b: bytes) -> str:
    la, lb = a.splitlines(), b.splitlines()
    for n, (x, y) in enumerate(zip(la, lb, strict=False), start=1):
        if x != y:
            j = next((i for i, (p, q) in enumerate(zip(x, y, strict=False)) if p != q), 0)
            lo = max(0, j - 60)
            return (
                f"line {n} col {j}: committed …{x[lo : j + 60]!r}… regenerated …{y[lo : j + 60]!r}…"
            )
    return f"{len(la)} committed lines, {len(lb)} regenerated lines"


def check_limits(tier: int, files: dict[str, bytes]) -> list[str]:
    errors: list[str] = []
    if tier != 1:
        return errors
    total = sum(len(v) for v in files.values())
    if total > TIER1_MAX_BYTES:
        errors.append(f"tier 1 is {total} bytes, over the {TIER1_MAX_BYTES} limit (A22)")
    for p, v in files.items():
        if len(v) > FILE_MAX_BYTES:
            errors.append(f"{p} is {len(v)} bytes, over the {FILE_MAX_BYTES} per-file limit")
    return errors


COVERAGE_ROUNDS = 3
COVERAGE_EXTRA_GAMES = 3


def coverage_extension(tier: int, rnd: int, executed: set[str]) -> list[FileTask]:
    """Tier-2 coverage seeds added until every non-vanilla card has executed a program
    (§8.3): round ``rnd`` adds 3 more games to every covering pairing with a non-vanilla
    target whose program has not run in any coverage game so far."""
    reg = get_registry()

    def non_vanilla(number: str) -> bool:
        e = reg.cards[reg.db[number].def_id]
        return bool(e.own or e.unit)

    decks = coverage_decks.covering_decks()
    specs: list[Spec] = []
    for i, j in coverage_decks.pairings(decks):
        targets = (*decks[i].targets, *decks[j].targets)
        if not any(non_vanilla(n) and n not in executed for n in targets):
            continue
        first = 3 + COVERAGE_EXTRA_GAMES * rnd
        for g in range(first, first + COVERAGE_EXTRA_GAMES):
            seed = derive_seed(TRACE_MASTER, "coverage", decks[i].name, decks[j].name, g)
            specs.append(
                Spec(
                    set="coverage",
                    id=f"coverage/cover/{decks[i].name}-{decks[j].name}/g{g}",
                    kind="game",
                    decks=(decks[i].deck, decks[j].deck),
                    seed=seed,
                    chooser=None,
                    agents=("policy", "policy"),
                    agent_seeds=(derive_seed(seed, "policy", 0), derive_seed(seed, "policy", 1)),
                )
            )
    tasks = _chunks("coverage", specs, 12, f"extend-r{rnd}")
    return [dataclasses.replace(t, tier=tier) for t in tasks]


def executed_in(files: dict[str, bytes], set_name: str) -> set[str]:
    """Card numbers whose programs ran in the ``set_name`` traces of ``files``
    (``census.py``'s executed-program census)."""
    import census

    traces = [
        trace
        for p, data in sorted(files.items())
        if set_of(p) == set_name and p.endswith(".ndjson")
        for trace in parse_trace_lines(data.decode())
    ]
    return census.executed_numbers(traces)


def parse_trace_lines(text: str) -> list[list[dict[str, Json]]]:
    traces: list[list[dict[str, Json]]] = []
    for line in text.splitlines():
        rec = json.loads(line)
        if rec["t"] == "header":
            traces.append([])
        traces[-1].append(rec)
    return traces


def generate(
    tier: int,
    only: tuple[str, ...],
    *,
    hash_seed: int,
    effect_offset: int,
    nocache: bool,
    plain: bool,
    workers: int,
) -> tuple[dict[str, bytes], list[str]]:
    """Every file of the selected sets, and the wrapped-vs-plain mismatches."""
    tasks = build_tasks(tier, only)

    def go(ts: list[FileTask]) -> tuple[dict[str, bytes], list[str]]:
        results = run_all(
            ts,
            hash_seed=hash_seed,
            effect_offset=effect_offset,
            nocache=nocache,
            plain=plain,
            workers=workers,
        )
        return assemble(results, ts), [m for r in results.values() for m in r.plain_mismatches]

    files, mismatches = go(tasks)
    if tier == 2 and "coverage" in only:
        for rnd in range(COVERAGE_ROUNDS):
            ext = coverage_extension(tier, rnd, executed_in(files, "coverage"))
            if not ext:
                break
            more, mism = go(ext)
            files.update(more)
            mismatches += mism
    return files, mismatches


def cmd_write(args: argparse.Namespace) -> int:
    tier = args.tier
    only = tuple(args.only)
    seeds = args.hash_seeds
    runs: list[dict[str, bytes]] = []
    for seed in seeds:
        print(f"generating tier {tier} under PYTHONHASHSEED={seed}", file=sys.stderr)
        files, mism = generate(
            tier,
            only,
            hash_seed=seed,
            effect_offset=args.effect_offset,
            nocache=False,
            plain=args.plain if args.plain is not None else tier == 1,
            workers=args.workers,
        )
        if mism:
            print("wrapped and plain runs differ:\n  " + "\n  ".join(mism), file=sys.stderr)
            return 1
        runs.append(files)
    ref = runs[0]
    for seed, other in zip(seeds[1:], runs[1:], strict=True):
        for p in sorted(ref):
            if normalized(other[p], seeds[0], None) != ref[p]:
                print(
                    f"{p}: PYTHONHASHSEED={seed} differs: {first_diff(ref[p], other[p])}",
                    file=sys.stderr,
                )
                return 1
    errors = check_limits(tier, {**tier_files(tier), **ref}) if only == SETS else []
    if errors:
        print("\n".join(errors), file=sys.stderr)
        return 1
    base = TIER_DIRS[tier]
    for name in only:
        stale = base / name
        if stale.exists() and name != TESTS_SUBDIR:
            shutil.rmtree(stale)
    for p, data in ref.items():
        out = base / p
        out.parent.mkdir(parents=True, exist_ok=True)
        out.write_bytes(data)
    files = tier_files(tier)
    errors = check_limits(tier, files)
    if tier == 2 and not args.update_manifest:
        manifest = load_manifest(tier)
        for p in sorted(set(manifest) | set(files)):
            if p not in files or p not in manifest or manifest[p] != manifest_entry(p, files[p]):
                errors.append(f"{p}: the regenerated tier 2 file does not match its manifest entry")
        for e in errors:
            print(e, file=sys.stderr)
        return 1 if errors else 0
    write_manifest(tier, files)
    print(
        f"wrote {len(ref)} files to {base.relative_to(ROOT)}; "
        f"tier {tier} now {sum(len(v) for v in files.values())} bytes",
        file=sys.stderr,
    )
    if errors:
        print("\n".join(errors), file=sys.stderr)
        return 1
    return 0


def cmd_check(args: argparse.Namespace) -> int:
    tier = args.tier
    only = tuple(args.only)
    manifest = load_manifest(tier)
    if not manifest:
        print(f"no manifest at {MANIFESTS[tier].relative_to(ROOT)}", file=sys.stderr)
        return 1
    errors: list[str] = []
    on_disk = tier_files(tier)
    if tier == 1:
        for p in sorted(set(manifest) | set(on_disk)):
            if p not in on_disk:
                errors.append(f"{p}: listed in the manifest but missing")
            elif p not in manifest:
                errors.append(f"{p}: not listed in the manifest")
            elif manifest[p] != manifest_entry(p, on_disk[p]):
                errors.append(f"{p}: does not match its manifest entry")
    env_seed = os.environ.get("PYTHONHASHSEED")
    seeds = [int(env_seed)] if env_seed in ("1", "2") else list(args.hash_seeds)
    for seed in seeds:
        label = f"PYTHONHASHSEED={seed}" + (" --nocache" if args.nocache else "")
        if args.effect_offset:
            label += f" offset {args.effect_offset}"
        print(f"regenerating tier {tier} ({label})", file=sys.stderr)
        regenerated, mism = generate(
            tier,
            only,
            hash_seed=seed,
            effect_offset=args.effect_offset,
            nocache=args.nocache,
            plain=(args.plain if args.plain is not None else tier == 1) and not args.nocache,
            workers=args.workers,
        )
        errors += mism
        expected = {p: e for p, e in manifest.items() if set_of(p) in only}
        for p in sorted(set(expected) | set(regenerated)):
            if p not in regenerated:
                errors.append(f"{p}: in the manifest but not regenerated")
                continue
            if p not in expected:
                errors.append(f"{p}: regenerated but not in the manifest")
                continue
            gen = expected[p].get("generator") or {}
            data = normalized(
                regenerated[p],
                gen.get("hash_seed"),
                gen.get("effect_offset") if args.effect_offset else None,
            )
            if hashlib.sha256(data).hexdigest() != expected[p]["sha256"]:
                where = first_diff(on_disk[p], data) if p in on_disk else "no committed copy"
                errors.append(f"{p} ({label}): differs from the manifest: {where}")
    for e in errors:
        print(e, file=sys.stderr)
    if errors:
        return 1
    print(f"tier {tier} check passed ({', '.join(only)})", file=sys.stderr)
    return 0


def cmd_write_manifest(args: argparse.Namespace) -> int:
    files = tier_files(args.tier)
    write_manifest(args.tier, files)
    errors = check_limits(args.tier, files)
    for e in errors:
        print(e, file=sys.stderr)
    return 1 if errors else 0


# ---------------------------------------------------------------------------------------------
# On-demand modes


def read_traces(path: Path) -> list[list[dict[str, Json]]]:
    traces: list[list[dict[str, Json]]] = []
    for line in path.read_text().splitlines():
        rec = json.loads(line)
        if rec["t"] == "header":
            traces.append([])
        traces[-1].append(rec)
    return traces


def find_trace(tid: str) -> list[dict[str, Json]]:
    for tier in (1, 2):
        base = TIER_DIRS[tier]
        if not base.exists():
            continue
        for path in sorted(base.rglob("*.ndjson")):
            if tid.encode() not in path.read_bytes():
                continue
            for trace in read_traces(path):
                if trace[0]["id"] == tid:
                    return trace
    raise SystemExit(f"no trace {tid!r} in tier 1 or tier 2")


def _decks_of(head: dict[str, Json]) -> tuple[DeckList, DeckList]:
    d = head["decks"]
    return (
        DeckList(tuple(d[0]["main"]), tuple(d[0]["res"])),
        DeckList(tuple(d[1]["main"]), tuple(d[1]["res"])),
    )


def _as_int(v: Json) -> int:
    return int(v)


def start_state(trace: list[dict[str, Json]]) -> GameState:
    """The state a game trace starts from: ``new_game`` from the header, or for a world trace
    the determinised world, rebuilt from its source game and sample seed."""
    head = trace[0]
    tid = str(head["id"])
    if head["seed"] is not None:
        return new_game(
            _decks_of(head),
            _as_int(head["seed"]),
            chooser=head["chooser"],
            turn_limit=head["turn_limit"],
            max_actions=head["max_actions"],
        )
    if not tid.startswith("worlds/"):
        raise SystemExit(f"{tid}: this trace starts from its snapshot and has no decisions")
    spec_id = tid.rsplit("/", 2)[0]
    specs = {**all_specs(2), **all_specs(1)}
    spec = specs.get(spec_id)
    src = specs.get(spec.source or "") if spec is not None else None
    if spec is None or src is None:
        raise SystemExit(f"{tid}: no world spec {spec_id!r}")
    states = _source_states(src)
    base = states[_pick(len(states), spec.sample_seed)]
    assert base.pending is not None
    return determinize(base, base.pending.player, spec.sample_seed)


def state_at(tid: str, decision: int) -> GameState:
    """The state before decision ``decision`` of trace ``tid``, from its recorded choices."""
    trace = find_trace(tid)
    st = start_state(trace)
    for rec in trace:
        if rec["t"] != "decision":
            continue
        if rec["i"] >= decision:
            break
        apply(st, Action.from_json(rec["chosen"]))
    return st


def cmd_state(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(prog="traces.py state")
    ap.add_argument("--trace", required=True)
    ap.add_argument("--decision", type=int, required=True)
    args = ap.parse_args(argv)
    st = state_at(args.trace, args.decision)
    print(dumps_line(snapshot_record(st, args.decision)))
    return 0


def cmd_deep(args: argparse.Namespace) -> int:
    """Drill-down levels 2-3 (§8.7): re-run the search of one recorded decision with iteration
    (and with ``--apply-level`` per-apply) records."""
    trace = find_trace(args.deep)
    head = trace[0]
    rec = next((r for r in trace if r["t"] == "decision" and r["i"] == args.decision), None)
    if rec is None or "agent" not in rec or "iters" not in rec["agent"]:
        raise SystemExit(f"{args.deep}: decision {args.decision} has no recorded search")
    st = state_at(args.deep, args.decision)
    preset = head["preset"]
    if preset.startswith("baseline-"):
        config, weights = BASELINE_PRESETS[preset.split("-", 1)[1]], BASELINE_WEIGHTS
    else:
        config, weights = PRESETS[preset], DEFAULT_WEIGHTS
    seed = _as_int(rec["agent"]["seed"])
    CAP.apply_level = args.apply_level
    CAP.iteration_filter = args.iteration
    with capture_installed():
        reset_capture(args.decision, True)
        mcts.search(st, rec["player"], config, weights, SplitMix64(seed))
        out = [*CAP.iters, *CAP.apply_traces]
    text = "".join(dumps_line(r) + "\n" for r in out)
    if args.out:
        Path(args.out).write_text(text)
    else:
        sys.stdout.write(text)
    return 0


def cmd_game(argv: list[str]) -> int:
    """Ad hoc mode (brief step 6): one trace of any deck pair at any reference preset."""
    ap = argparse.ArgumentParser(prog="traces.py game")
    ap.add_argument("--decks", nargs=2, required=True, metavar=("A", "B"))
    ap.add_argument("--seed", type=int, required=True)
    ap.add_argument("--chooser", type=int, choices=(0, 1))
    ap.add_argument(
        "--agents",
        required=True,
        choices=("random", "policy", "greedy", "ref:fast", "ref:standard", "ref:strong"),
    )
    ap.add_argument("--snapshots", choices=("all",))
    ap.add_argument("--out", required=True)
    args = ap.parse_args(argv)
    decks = (to_decklist(load_deck(args.decks[0])), to_decklist(load_deck(args.decks[1])))
    kind = args.agents
    preset = None
    if kind.startswith("ref:"):
        preset = kind.split(":", 1)[1]
        kind = f"mcts:{preset}"
    blocks = kind not in ("policy",)
    seeds = (derive_seed(args.seed, "agent", 0), derive_seed(args.seed, "agent", 1))
    spec = Spec(
        set="adhoc",
        id=f"adhoc/{Path(args.decks[0]).stem}-{Path(args.decks[1]).stem}/s{args.seed}",
        kind="search" if blocks else "game",
        decks=decks,
        seed=args.seed,
        chooser=args.chooser,
        agents=(kind, kind),
        agent_seeds=seeds,
        preset=preset,
        snapshots=args.snapshots or "periodic",
        agent_blocks=blocks,
    )
    fields = gen_fields()
    gen = Gen(
        **fields,
        hash_seed=int(os.environ.get("PYTHONHASHSEED", "0") or 0),
        effect_offset=int(os.environ.get(EFFECT_OFFSET_ENV, "0") or 0),
    )
    played = generate_game(spec, gen)
    Path(args.out).write_text("".join(line + "\n" for p in played for line in p.lines))
    return 0


def parse_sets(text: str) -> list[str]:
    names = [s for s in text.split(",") if s]
    bad = [s for s in names if s not in SETS]
    if bad:
        raise argparse.ArgumentTypeError(f"unknown set(s) {bad}; choose from {list(SETS)}")
    return names


def parse_seeds(text: str) -> list[int]:
    return [int(s) for s in text.split(",") if s]


VECTORS = ROOT / "crates" / "gcg-diff" / "tests" / "vectors" / "trace_vectors.json"


def build_vectors() -> dict[str, Json]:
    """Vectors pinning the §8.2 encodings for ``crates/gcg-diff``: ``options_sha16`` of real
    option lists, ``float.hex`` spellings, the u64 encoding, and ``state_digest`` of real
    canonical states (the compact sorted ``json.dumps`` the digest hashes)."""
    import math

    spec = fuzz_spec(1, 0, "full")
    assert spec.seed is not None
    st = _start(spec, spec.seed, None)
    rng = SplitMix64(spec.agent_seeds[0])
    options: list[Json] = []
    states: list[Json] = []
    n = 0
    while st.winner is None and st.pending is not None:
        opts = st.pending.options
        if n % 7 == 0:
            options.append({"options": [o.to_json() for o in opts], "sha16": options_sha16(opts)})
        if n % 25 == 0:
            states.append({"state": canonical_state(st), "digest": state_digest(st)})
        apply(st, opts[rng.randrange(len(opts))])
        n += 1
    edge = [Action.from_json(a) for a in (["end_main", -1, -1, -1], ["play_unit", 1024, -1, 205])]
    options.append({"options": [o.to_json() for o in edge], "sha16": options_sha16(edge)})
    options.append({"options": [], "sha16": options_sha16(())})
    gen = SplitMix64(20261102)
    floats = [0.0, -0.0, 1.0, 0.1, -2.5, 5e-324, 2.2250738585072014e-308, 1.7976931348623157e308]
    floats += [math.ldexp(gen.random(), e) for e in range(-1074, 1024, 61)]
    return {
        "options_sha16": options,
        "float_hex": [{"bits": struct_bits(x), "hex": float.hex(x)} for x in floats],
        "u64": [{"value": str(v), "json": u64(v)} for v in (0, 1, U53 - 1, U53, (1 << 64) - 1)],
        "state_digest": states,
    }


def struct_bits(x: float) -> str:
    import struct

    return struct.pack(">d", x).hex()


def cmd_vectors(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(prog="traces.py vectors")
    ap.add_argument("--check", action="store_true")
    args = ap.parse_args(argv)
    text = json.dumps(build_vectors(), indent=1, sort_keys=True) + "\n"
    if args.check:
        if not VECTORS.exists() or VECTORS.read_text() != text:
            print(f"{VECTORS.relative_to(ROOT)} is stale: run traces.py vectors", file=sys.stderr)
            return 1
        return 0
    VECTORS.parent.mkdir(parents=True, exist_ok=True)
    VECTORS.write_text(text)
    return 0


def main(argv: list[str] | None = None) -> int:
    argv = list(sys.argv[1:] if argv is None else argv)
    if argv and argv[0] == "vectors":
        return cmd_vectors(argv[1:])
    if argv and argv[0] == "state":
        return cmd_state(argv[1:])
    if argv and argv[0] == "game":
        return cmd_game(argv[1:])
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    mode = ap.add_mutually_exclusive_group(required=True)
    mode.add_argument("--write", action="store_true", help="regenerate and write the corpus")
    mode.add_argument("--check", action="store_true", help="regenerate and compare")
    mode.add_argument("--write-manifest", action="store_true", help="rehash the files on disk")
    mode.add_argument("--deep", metavar="ID", help="re-run one search with level-2 records")
    ap.add_argument("--tier", type=int, choices=(1, 2), default=1)
    ap.add_argument("--only", type=parse_sets, default=list(SETS), help="comma-separated sets")
    ap.add_argument("--nocache", action="store_true", help="view.derived always recomputes")
    ap.add_argument("--hash-seeds", type=parse_seeds, default=list(HASH_SEEDS))
    ap.add_argument("--effect-offset", type=int, default=0, help="numbering-shift probe (§8.9)")
    ap.add_argument("--workers", type=int, default=max(1, (os.cpu_count() or 2) - 2))
    ap.add_argument("--decision", type=int, help="with --deep: the decision index")
    ap.add_argument("--apply-level", action="store_true", help="with --deep: level-3 records")
    ap.add_argument("--iteration", type=int, help="with --deep: one iteration only")
    ap.add_argument("--out", help="with --deep: output file (default stdout)")
    ap.add_argument(
        "--plain",
        action=argparse.BooleanOptionalAction,
        default=None,
        help="also replay every trace unwrapped and compare (default: tier 1 only)",
    )
    ap.add_argument(
        "--update-manifest",
        action="store_true",
        help="with --write --tier 2: rewrite MANIFEST-tier2.json (tier 1 always rewrites)",
    )
    args = ap.parse_args(argv)
    if args.deep:
        if args.decision is None:
            ap.error("--deep needs --decision")
        return cmd_deep(args)
    if args.write_manifest:
        return cmd_write_manifest(args)
    if args.write:
        return cmd_write(args)
    return cmd_check(args)


if __name__ == "__main__":
    raise SystemExit(main())
