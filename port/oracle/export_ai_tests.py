"""Python expectations of the ported AI tests (docs/port/PLAN.md §9.1; T-066, T-067, T-069).

The puzzle tests (``tests/ai/test_puzzles.py``, ``test_puzzles_shipped.py``,
``test_puzzles_targets.py``), the information-set tests (``test_information_set*.py``, the
invariance tests of ``test_root_moves.py`` and ``test_search_options.py``) and the AI-internals
tests (``test_root_moves.py``, ``test_search_options.py``, ``test_search_side_effects.py``,
``test_policy_knobs.py``, ``test_evaluation_roles.py``, ``test_agents.py``) are ported natively
to Rust (``crates/gcg-search/tests/ai_*.rs``). This tool runs the Python tests themselves with
pytest, one module per fresh process, and records every agent decision each test makes, so
that the Rust ports compare more than their assertions: the full decision logs.

Writes ``crates/gcg-ai/tests/vectors/ai_tests/``:

- ``<module>.ndjson`` for each module of ``MODULES``: one line per test node id (pytest order),
  ``{"id", "outcome", "decisions"}``. A decision is one ``Agent.choose`` call of a random,
  greedy or MCTS agent: the digest of the state it decided on, the agent's name, construction
  seed and player, the action, and the agent block of ``traces.agent_block`` (the
  per-decision counter ``k`` and seed, the generator state afterwards, every alternative
  unranked and unrounded with its value as ``float.hex()``, the iterations, budget, cap and root
  keys of a search, and the decision-log entries written, each as
  ``json.dumps(entry, sort_keys=True)``). A direct ``mcts.search`` call (``test_search_options``)
  is recorded with ``"search": true``, the generator state before and after, the action, the
  alternatives and the iterations. A test in ``UNCOMPARED`` keeps only its decision count (its
  monkeypatched searches have no Rust counterpart).
- ``midgame.json``: the AI test decks; the 25 states of ``midgame_states()`` (canonical
  snapshot as ``traces.snapshot_record`` writes it, digest, deciding player), with the digests
  of ``permute_hidden(st, p, s)`` for the three permutation seeds and whether its hidden
  identities changed; the digests of the three extra states of
  ``test_search_options._invariance_states``; and the negative control the Rust port runs
  (``test_information_set.py:79-103``): Rust cannot replace ``determinize`` with a clone, so it
  searches copies of ``st`` and of ``permute_hidden(st, p, 1)`` in which every card is known to
  both players (``known = 0b11``, so every determinization keeps the true identities). Their
  digests and the MCTS decisions on them are recorded here.
- ``fragments/<name>.json``: the IR fragments (``export_ir.build_fragment``) of the
  ``command_script`` overrides of ``test_root_moves.py:312`` and ``:379``, each exported in a
  fresh process (PLAN A16). The ``card_text`` overrides of these modules are the committed
  fixtures of ``crates/gcg-effects/ir/fragments/``.

``test_agents.py``'s Hypothesis test draws random seeds; ``SAMPLE_SEEDS`` replaces them with a
fixed sample, whose game outcomes ``midgame.json`` records (``hypothesis_sample``). The tuning
tests of ``test_evaluation_roles.py`` and its ``parse_agent`` test belong to ``gcg-eval``'s
port (``DESELECTED``).

Every output is generated twice, under ``PYTHONHASHSEED=1`` and ``=2``, and the two runs must
agree byte for byte.

    uv run python port/oracle/export_ai_tests.py [--check] [--jobs N]
"""

from __future__ import annotations

import argparse
import concurrent.futures
import contextlib
import importlib
import json
import os
import subprocess
import sys
import tempfile
from collections.abc import Callable, Iterator
from pathlib import Path
from typing import Any, cast

import export_ir
import pytest
import traces

from gcg_sim.ai import mcts
from gcg_sim.ai.agents import GreedyAgent, MctsAgent, RandomAgent, make_agent
from gcg_sim.ai.base import AgentCore
from gcg_sim.ai.selfplay import run_game
from gcg_sim.effects import dsl as d
from gcg_sim.effects.registry import get_registry
from gcg_sim.engine.observe import permute_hidden, unknown_instances
from gcg_sim.engine.state import Action, GameState
from gcg_sim.runner.digest import state_digest

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "crates" / "gcg-ai" / "tests" / "vectors" / "ai_tests"
FRAGMENTS = OUT / "fragments"
IR = ROOT / "crates" / "gcg-effects" / "ir" / "registry.json"
HASH_SEEDS = (1, 2)
MODULES = (
    "test_puzzles",
    "test_puzzles_shipped",
    "test_puzzles_targets",
    "test_information_set",
    "test_information_set_policy",
    "test_information_set_weights",
    "test_root_moves",
    "test_search_options",
    "test_search_side_effects",
    "test_policy_knobs",
    "test_evaluation_roles",
    "test_agents",
)
DESELECTED = {
    "tests/ai/test_agents.py::test_random_and_greedy_finish_games_legally": (
        "Hypothesis draws random seeds; SAMPLE_SEEDS is the fixed sample"
    ),
    "tests/ai/test_evaluation_roles.py::test_compare_selects_the_role_weights": (
        "ai.compare.parse_agent is gcg-eval's"
    ),
    "tests/ai/test_evaluation_roles.py::test_tuning_fits_the_role_feature_set_reproducibly": (
        "ai.tuning is gcg-eval's"
    ),
    "tests/ai/test_evaluation_roles.py::"
    "test_tuning_cli_selects_the_preset_start_weights_and_feature_set": "ai.tuning is gcg-eval's",
    "tests/ai/test_evaluation_roles.py::"
    "test_tuning_without_rounds_reports_the_start_weights_feature_set": "ai.tuning is gcg-eval's",
}
UNCOMPARED = frozenset(
    {
        "tests/ai/test_information_set.py::"
        "test_negative_control_a_search_on_the_true_state_is_not_invariant",
    }
)
# test_agents.py:156-160: Hypothesis integers in [0, 2**32], here a fixed sample of eight.
SAMPLE_SEEDS = (0, 1, 2, 7, 42, 4242, 2**31, 2**32)
REVEALED = 0b11

Json = Any


# ---------------------------------------------------------------------------------------------
# Decision recording


class Recording:
    """The decisions of the running test (``None`` outside a test or while warming caches)."""

    def __init__(self) -> None:
        self.current: list[dict[str, Json]] | None = None
        self.depth = 0


REC = Recording()


def _record_choose(cls: type[AgentCore]) -> Callable[..., Action]:
    original = cast(Callable[..., Action], vars(cls)["choose"])

    def choose(self: AgentCore, st: GameState, player: int) -> Action:
        if REC.current is None or REC.depth:
            return original(self, st, player)
        digest = state_digest(st)
        traces.reset_capture(0, False)
        log_before = len(self._entries)
        REC.depth += 1
        try:
            action = original(self, st, player)
        finally:
            REC.depth -= 1
        block = traces.agent_block(self, log_before)
        block["log"] = [json.dumps(e, sort_keys=True) for e in block["log"]]
        REC.current.append(
            {
                "digest": digest,
                "agent": self.name,
                "agent_seed": traces.u64(self.seed),
                "player": player,
                "action": action.to_json(),
                **block,
            }
        )
        return action

    return choose


def _record_search(original: Callable[..., Any]) -> Callable[..., Any]:
    def search(st: GameState, player: int, *args: Any, **kwargs: Any) -> Any:
        if REC.current is None or REC.depth:
            return original(st, player, *args, **kwargs)
        rng = args[2] if len(args) > 2 else kwargs["rng"]
        digest = state_digest(st)
        before = rng.state
        REC.depth += 1
        try:
            out = original(st, player, *args, **kwargs)
        finally:
            REC.depth -= 1
        REC.current.append(
            {
                "search": True,
                "digest": digest,
                "player": player,
                "rng_before": traces.u64(before),
                "rng_after": traces.u64(rng.state),
                "action": out.action.to_json(),
                "alts": [
                    [a.action.to_json(), traces.fhex(a.value), a.visits] for a in out.alternatives
                ],
                "iters": out.iterations,
            }
        )
        return out

    return search


@contextlib.contextmanager
def recording_installed() -> Iterator[None]:
    saved = [(cls, vars(cls)["choose"]) for cls in (RandomAgent, GreedyAgent, MctsAgent)]
    search = mcts.search
    with traces.capture_installed():
        for cls, _ in saved:
            cls.choose = _record_choose(cls)  # type: ignore[method-assign]
        mcts.search = _record_search(search)
        try:
            yield
        finally:
            mcts.search = search
            for cls, choose in saved:
                cls.choose = choose  # type: ignore[method-assign]


class Plugin:
    """Records the decisions of every test of one module run (pytest plugin)."""

    def __init__(self) -> None:
        self.tests: dict[str, dict[str, Json]] = {}

    def pytest_collection_modifyitems(
        self, session: pytest.Session, config: pytest.Config, items: list[pytest.Item]
    ) -> None:
        keep = [i for i in items if i.nodeid.split("[")[0] not in DESELECTED]
        config.hook.pytest_deselected(items=[i for i in items if i not in keep])
        items[:] = keep

    def pytest_collection_finish(self, session: pytest.Session) -> None:
        """``midgame_states()`` is cached and shared by the information-set modules: compute
        it before any test, so that its greedy self-play is never recorded."""
        for fn in _cached_functions(session.items):
            if fn.__name__ == "midgame_states":
                fn()

    @pytest.hookimpl(hookwrapper=True)
    def pytest_runtest_call(self, item: pytest.Item) -> Iterator[None]:
        for fn in _cached_functions([item]):
            if fn.__name__ != "midgame_states":
                fn.cache_clear()
        REC.current = []
        try:
            yield
        finally:
            decisions, REC.current = REC.current, None
            self.tests[item.nodeid] = {"id": item.nodeid, "decisions": decisions}

    def pytest_runtest_logreport(self, report: pytest.TestReport) -> None:
        rec = self.tests.setdefault(report.nodeid, {"id": report.nodeid, "decisions": []})
        if report.failed or report.when == "call":
            rec["outcome"] = report.outcome if report.when == "call" else f"{report.when} failed"


def _cached_functions(items: list[pytest.Item]) -> list[Any]:
    """The ``functools.cache`` functions of the test modules (``tests.ai.*``) the items use;
    cached library functions such as ``get_registry`` are left alone."""
    out: list[Any] = []
    for item in items:
        module = getattr(item, "module", None)
        for value in vars(module).values() if module is not None else ():
            cached = callable(value) and hasattr(value, "cache_clear")
            ours = str(getattr(value, "__module__", "")).startswith("tests.")
            if cached and ours and value not in out:
                out.append(value)
    return out


def module_lines(module: str) -> str:
    """Run one test module under pytest and return its records, one NDJSON line per test."""
    plugin = Plugin()
    args = [
        f"tests/ai/{module}.py",
        "-q",
        "-p",
        "no:cacheprovider",
        "-p",
        "no:xdist",
        "-p",
        "no:randomly",
    ]
    with recording_installed(), contextlib.redirect_stdout(sys.stderr):
        code = pytest.main(args, plugins=[plugin])
    if code not in (pytest.ExitCode.OK,):
        raise SystemExit(f"{module}: pytest exited with {code}")
    lines = []
    for rec in plugin.tests.values():
        if rec.get("outcome") != "passed":
            raise SystemExit(f"{rec['id']}: {rec.get('outcome')}")
        if rec["id"] in UNCOMPARED:
            rec = {"id": rec["id"], "outcome": rec["outcome"], "decisions": len(rec["decisions"])}
        lines.append(json.dumps(rec, sort_keys=True, separators=(",", ":")) + "\n")
    return "".join(lines)


# ---------------------------------------------------------------------------------------------
# Mid-game states, the negative control and the Hypothesis sample


def test_module(name: str) -> Any:
    """``tests.ai.<name>``, imported at run time (the Python tests are not type-checked
    here)."""
    return importlib.import_module(f"tests.ai.{name}")


def _revealed(st: GameState) -> GameState:
    out = st.clone()
    for c in out.cards:
        c.known = REVEALED
    return out


def _hidden_ids(st: GameState, observer: int) -> list[int]:
    return [st.cards[u].def_id for owner in (0, 1) for u in unknown_instances(st, observer, owner)]


def midgame_json() -> str:
    tis = test_module("test_information_set")
    tso = test_module("test_search_options")
    DECKS = test_module("decks").DECKS

    states = tis.midgame_states()
    out: dict[str, Json] = {"decks": [traces.deck_json(deck) for deck in DECKS]}
    rows = []
    for i, st in enumerate(states):
        assert st.pending is not None
        p = st.pending.player
        perms = []
        for s in tis.PERMUTATION_SEEDS:
            v = permute_hidden(st, p, s)
            perms.append(
                {
                    "seed": s,
                    "digest": state_digest(v),
                    "changed": _hidden_ids(v, p) != _hidden_ids(st, p),
                }
            )
        snap = traces.snapshot_record(st, i)
        del snap["t"]
        rows.append(
            {
                "index": i,
                "player": p,
                "kind": st.pending.kind.value,
                "options": len(st.pending.options),
                "digest": state_digest(st),
                "snapshot": snap,
                "permutations": perms,
            }
        )
    out["states"] = rows
    extra = tso._invariance_states()[len(states) :]
    out["extra_states"] = [state_digest(st) for st in extra]

    control = []
    REC.current = []
    with recording_installed():
        for i, st in enumerate(states):
            assert st.pending is not None
            p = st.pending.player
            seed = tis.AGENT_SEEDS[i % len(tis.AGENT_SEEDS)]
            pair = (_revealed(st), _revealed(permute_hidden(st, p, tis.PERMUTATION_SEEDS[0])))
            results = []
            for variant in pair:
                agent = make_agent("mcts", seed=seed, log=True)
                action = agent.choose(variant, p)
                decision = (action, agent.decision_log()[-1]["alternatives"])
                results.append((decision, state_digest(variant)))
            control.append(
                {
                    "index": i,
                    "digests": [digest for _, digest in results],
                    "differs": results[0][0] != results[1][0],
                }
            )
    out["revealed_decisions"] = REC.current
    REC.current = None
    out["revealed"] = control

    sample = []
    for seed in SAMPLE_SEEDS:
        agents = (make_agent("greedy", seed=seed), make_agent("random", seed=seed + 1))
        st, decisions = run_game(agents, DECKS, seed)
        sample.append(
            {
                "seed": traces.u64(seed),
                "winner": st.winner,
                "turn": st.turn,
                "decisions": decisions,
                "digest": state_digest(st),
            }
        )
    out["hypothesis_sample"] = sample
    return json.dumps(out, indent=1, sort_keys=True) + "\n"


# ---------------------------------------------------------------------------------------------
# Override fragments (test_root_moves.py:312, :379)


def _enemy_unit() -> d.Sel:
    return d.Sel(d.Side.ENEMY, d.Loc.BATTLE)


FRAGMENT_SCRIPTS: dict[str, Callable[[], tuple[d.Step, ...]]] = {
    # test_root_moves.py:312: choose an enemy EX Resource and remove it.
    "remove_enemy_ex_resource": lambda: (
        d.Choose(
            "t1",
            d.Sel(d.Side.ENEMY, d.Loc.RESOURCE_AREA, (d.IsKind((d.CardKind.EX_RESOURCE,)),)),
        ),
        d.Exile(d.Var("t1")),
    ),
    # test_root_moves.py:379-384: "Rest it", without and with "If you do, draw 1".
    "choose_and_rest": lambda: (d.Choose("t1", _enemy_unit()), d.Rest(d.Var("t1"))),
    "choose_rest_if_you_do_draw": lambda: (
        d.Choose("t1", _enemy_unit()),
        d.Rest(d.Var("t1")),
        d.IfYouDo((d.Draw(),)),
    ),
}
FRAGMENT_CARD = "ST01-014"


def fragment_json(name: str) -> str:
    """``command_script(FRAGMENT_CARD, *steps)`` (``test_root_moves.py:363-373``) as an IR
    fragment over the committed base IR, in this (fresh) process."""
    command_script = test_module("test_root_moves").command_script

    base = json.loads(IR.read_text(encoding="utf-8"))
    reg = get_registry()
    with command_script(FRAGMENT_CARD, *FRAGMENT_SCRIPTS[name]()):
        frag = export_ir.build_fragment(reg, base, [reg.db[FRAGMENT_CARD].def_id])
    body = {"card_number": FRAGMENT_CARD, "name": name, "fragment": frag}
    return json.dumps(body, indent=1, sort_keys=True, ensure_ascii=True) + "\n"


# ---------------------------------------------------------------------------------------------
# Driver


def emit(what: str) -> str:
    if what == "midgame":
        return midgame_json()
    if what.startswith("fragment:"):
        return fragment_json(what.split(":", 1)[1])
    return module_lines(what)


def targets() -> dict[str, str]:
    """Output path (relative to OUT) of every emit target."""
    out = {m: f"{m}.ndjson" for m in MODULES}
    out["midgame"] = "midgame.json"
    for name in FRAGMENT_SCRIPTS:
        out[f"fragment:{name}"] = f"fragments/{name}.json"
    return out


def run_under(seed: int, what: str) -> str:
    env = {**os.environ, "PYTHONHASHSEED": str(seed)}
    with tempfile.TemporaryDirectory(prefix="export_ai_tests-") as tmp:
        path = Path(tmp) / "out"
        res = subprocess.run(
            [sys.executable, str(Path(__file__).resolve()), "--emit", what, str(path)],
            cwd=ROOT,
            env=env,
            capture_output=True,
            text=True,
            encoding="utf-8",
            check=False,
        )
        if res.returncode != 0:
            raise SystemExit(f"{what} under PYTHONHASHSEED={seed} failed:\n{res.stderr[-4000:]}")
        return path.read_text(encoding="utf-8")


def build(jobs: int) -> dict[str, str]:
    names = targets()
    work = [(seed, what) for what in names for seed in HASH_SEEDS]
    with concurrent.futures.ThreadPoolExecutor(max_workers=jobs) as pool:
        texts = dict(zip(work, pool.map(lambda w: run_under(*w), work), strict=True))
    files = {}
    for what, rel in names.items():
        first, second = (texts[(seed, what)] for seed in HASH_SEEDS)
        if first != second:
            raise SystemExit(f"{what}: the export depends on PYTHONHASHSEED")
        files[rel] = first
    return files


def main() -> int:
    ap = argparse.ArgumentParser(description=(__doc__ or "").splitlines()[0])
    ap.add_argument("--check", action="store_true", help="fail if the committed files are stale")
    ap.add_argument("--jobs", type=int, default=8, help="parallel exporter processes")
    ap.add_argument("--emit", nargs=2, metavar=("WHAT", "OUT"), help=argparse.SUPPRESS)
    args = ap.parse_args()
    if args.emit:
        sys.path.insert(0, str(ROOT))
        text = emit(args.emit[0])
        Path(args.emit[1]).write_text(text, encoding="utf-8")
        return 0
    files = build(args.jobs)
    expected = {OUT / rel for rel in files}
    present = {p for p in OUT.rglob("*") if p.is_file()} if OUT.exists() else set()
    if args.check:
        stale = sorted(
            rel
            for rel, text in files.items()
            if not (OUT / rel).exists() or (OUT / rel).read_text(encoding="utf-8") != text
        )
        stray = sorted(str(p.relative_to(OUT)) for p in present - expected)
        if stale or stray:
            print(f"{OUT}: stale {stale}, unexpected {stray}; run without --check", file=sys.stderr)
            return 1
        print(f"{OUT.relative_to(ROOT)} is current (PYTHONHASHSEED 1 and 2)")
        return 0
    for p in present - expected:
        p.unlink()
    for rel, text in files.items():
        path = OUT / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")
    print(f"wrote {len(files)} files to {OUT.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
