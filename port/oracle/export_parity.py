"""Export the Python side of the runner, CLI and eval parity tests (docs/port/PLAN.md §8.8,
§9.1, §9.5; T-004, T-073, T-076, T-078, T-079, T-082).

Every Python run goes through the installed console script (``.venv/bin/gcg-sim``) or
``python -m gcg_sim.ai.compare`` in a child process from the repository root, under a fixed
``PYTHONHASHSEED`` and without ``COLUMNS``. Report files are recorded as SHA-256 digests of
Python's raw bytes (the Rust tests rebuild them and compare digests after masking the three
implementation fields of ``scripts/normalize_outputs.py``); command transcripts are recorded as
text, with the report directory replaced by ``{OUT}``. Sections:

- ``collect`` -> ``crates/gcg-runner/tests/vectors/parity/python_suites.json``: the pytest node
  ids of ``tests/runner``, ``tests/reports``, ``tests/cli`` and ``tests/deck`` (T-078), which
  the Rust register ``crates/gcg-runner/tests/python_suites.json`` must map one to one.
- ``invalid`` -> ``crates/gcg-runner/tests/vectors/invalid-results/*.json`` and
  ``crates/gcg-runner/tests/vectors/parity/invalid-results.json``: one negative control per
  assertion keyword of ``results.v1.schema.json`` (T-076). Each is a Python ``results.json``
  with one mutation; jsonschema (with the format checker ``validate_results`` uses) reports
  exactly one error for it, and the keyword appears in the schema path of that error or of one
  of its ``oneOf`` context errors. The index records the error's instance pointer, message and
  schema path.
- ``test-random`` -> ``crates/gcg-eval/tests/vectors/parity/test-random.json``: ``gcg-sim
  benchmark`` with ``GCG_SIM_AGENT_FACTORY=tests/runner/random_agent.py:make_random_agent`` in
  the ``tests/reports/conftest.py`` configuration and two more, with the equivalent
  ``gcg-eval reference-benchmark --agent test-random`` command line (T-073).
- ``reference`` -> ``crates/gcg-eval/tests/vectors/parity/reference.json``: one-match ``gcg-sim
  benchmark`` runs of the reference search at each of Python's presets ``fast``, ``standard``
  and ``strong``, with the equivalent ``gcg-eval reference-benchmark --reference-preset``
  command line (T-082).
- ``compare`` -> ``crates/gcg-eval/tests/vectors/parity/compare.json``: the stdout and
  ``--out`` bytes of ``python -m gcg_sim.ai.compare`` for the three configurations of the M9
  gate (T-079).
- ``python-replays`` -> ``crates/gcg-cli/tests/vectors/parity/python-replays.json``: Python's
  ``gcg-sim replay`` and ``replay --log`` transcripts of every committed Python replay
  (``crates/gcg-runner/tests/vectors/records/*/replays/*.json``; T-004, Python to Rust).
- ``rust-replays`` -> ``crates/gcg-cli/tests/vectors/parity/rust-replays.json``: replays that
  the Rust binaries write (``gcg-sim`` with the shipped presets, ``gcg-eval
  reference-benchmark``), their digests, and Python's ``replay`` and ``replay --log``
  transcripts of each (T-004, Rust to Python). This section runs ``target/release/gcg-sim`` and
  ``target/release/gcg-eval``; build them first (``cargo build --release -p gcg-cli -p
  gcg-eval``).

    uv run python port/oracle/export_parity.py [--check] [--section NAME ...]

``--check`` regenerates every selected section under ``PYTHONHASHSEED=2`` (the files are
written under ``PYTHONHASHSEED=1``) and fails if a committed file differs.
"""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
import os
import subprocess
import sys
import tempfile
from collections.abc import Callable, Iterator
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import jsonschema

from gcg_sim.reports import load_schema

ROOT = Path(__file__).resolve().parents[2]
RUNNER_VECTORS = ROOT / "crates" / "gcg-runner" / "tests" / "vectors"
EVAL_VECTORS = ROOT / "crates" / "gcg-eval" / "tests" / "vectors" / "parity"
CLI_VECTORS = ROOT / "crates" / "gcg-cli" / "tests" / "vectors" / "parity"
RECORDS = RUNNER_VECTORS / "records"
INVALID_DIR = RUNNER_VECTORS / "invalid-results"
INVALID_INDEX = RUNNER_VECTORS / "parity" / "invalid-results.json"
SUITES_OUT = RUNNER_VECTORS / "parity" / "python_suites.json"
SUITES = ("tests/runner", "tests/reports", "tests/cli", "tests/deck")
PY_SIM = Path(sys.executable).parent / "gcg-sim"
RUST_SIM = ROOT / "target" / "release" / "gcg-sim"
RUST_EVAL = ROOT / "target" / "release" / "gcg-eval"
FACTORY_ENV = "GCG_SIM_AGENT_FACTORY"
RANDOM_AGENT = "tests/runner/random_agent.py:make_random_agent"
OUT_TOKEN = "{OUT}"
REPORT_FILES = ("results.json", "summary.md", "games.ndjson")
FED = "examples/decks/blue-white-federation.txt"
ZEON = "examples/decks/red-green-zeon.txt"
TEK = "examples/decks/purple-white-tekkadan.txt"

Json = dict[str, Any]
Files = dict[Path, bytes]


def dumps(data: object) -> bytes:
    return (json.dumps(data, indent=1, sort_keys=True, ensure_ascii=False) + "\n").encode()


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def child_env(hash_seed: str, factory: str | None = None) -> dict[str, str]:
    env = {k: v for k, v in os.environ.items() if k not in ("COLUMNS", FACTORY_ENV)}
    env["PYTHONHASHSEED"] = hash_seed
    if factory is not None:
        env[FACTORY_ENV] = factory
    return env


def run(argv: list[str], env: dict[str, str]) -> subprocess.CompletedProcess[bytes]:
    return subprocess.run(argv, cwd=ROOT, env=env, capture_output=True, check=False)


def text(data: bytes) -> str:
    return data.decode("utf-8")


def masked_text(data: bytes, out: Path) -> str:
    return text(data).replace(str(out), OUT_TOKEN)


# ---------------------------------------------------------------------------------------------
# collect (T-078)


def collect(hash_seed: str) -> Files:
    done = run(
        [sys.executable, "-m", "pytest", "--collect-only", "-q", "-p", "no:cacheprovider", *SUITES],
        child_env(hash_seed),
    )
    if done.returncode != 0:
        raise SystemExit(f"pytest --collect-only failed:\n{text(done.stdout)}{text(done.stderr)}")
    ids = [line for line in text(done.stdout).splitlines() if "::" in line]
    slow = run(
        [
            sys.executable,
            "-m",
            "pytest",
            "--collect-only",
            "-q",
            "-p",
            "no:cacheprovider",
            "-m",
            "slow",
            *SUITES,
        ],
        child_env(hash_seed),
    )
    slow_ids = {line for line in text(slow.stdout).splitlines() if "::" in line}
    tests = [{"id": i, "function": i.split("[", 1)[0], "slow": i in slow_ids} for i in ids]
    return {SUITES_OUT: dumps({"suites": list(SUITES), "tests": tests})}


# ---------------------------------------------------------------------------------------------
# invalid (T-076)

INVALID_BASE = "fast-bo1-s3"
ANNOTATIONS = ("$defs", "$id", "$schema", "description", "title")


@dataclass(frozen=True)
class Mutation:
    keyword: str
    pointer: str
    op: str
    value: object = None


MUTATIONS = (
    Mutation("$ref", "/cards/0/copies", "replace", 0),
    Mutation("additionalProperties", "/ai/extra", "add", 1),
    Mutation("const", "/seeds/prng", "replace", "PCG64"),
    Mutation("enum", "/config/format", "replace", "bo5"),
    Mutation("exclusiveMaximum", "/hypothesis_thresholds/alpha", "replace", 1),
    Mutation("exclusiveMinimum", "/hypothesis_thresholds/alpha", "replace", 0),
    Mutation("format", "/versions/banlist_effective_date", "replace", "2026-13-01"),
    Mutation("items", "/notes/0", "replace", 5),
    Mutation("maxItems", "/results/games/dut_win_rate/ci95/-", "add", 0.5),
    Mutation("maximum", "/benchmark_cards/0/loss_rate_lift", "replace", 1.5),
    Mutation("minItems", "/conflict_resolutions/0/decks", "replace", []),
    Mutation("minLength", "/ai/factory", "replace", ""),
    Mutation("minimum", "/config/matches", "replace", 0),
    Mutation("oneOf", "/results/games/dut_win_rate/ci95", "replace", "0.5"),
    Mutation("pattern", "/versions/code_digest", "replace", "0" * 63),
    Mutation("prefixItems", "/results/games/dut_win_rate/ci95/0", "replace", -0.5),
    Mutation("properties", "/versions/package", "replace", 1),
    Mutation("propertyNames", "/splits/by_game_number/4", "add", "copy:/splits/by_game_number/1"),
    Mutation("required", "/cards", "remove"),
    Mutation("type", "/seeds/master", "replace", "3"),
    Mutation("unevaluatedProperties", "/end_reasons/0/extra", "add", 1),
)


def pointer_parts(pointer: str) -> list[str]:
    return [p.replace("~1", "/").replace("~0", "~") for p in pointer.split("/")[1:]]


def resolve(doc: Any, pointer: str) -> Any:
    node = doc
    for part in pointer_parts(pointer):
        node = node[int(part)] if isinstance(node, list) else node[part]
    return node


def mutate(doc: Json, m: Mutation) -> Json:
    out = copy.deepcopy(doc)
    *parents, last = pointer_parts(m.pointer)
    parent: Any = out
    for part in parents:
        parent = parent[int(part)] if isinstance(parent, list) else parent[part]
    value = m.value
    if isinstance(value, str) and value.startswith("copy:"):
        value = copy.deepcopy(resolve(doc, value.removeprefix("copy:")))
    if m.op == "remove":
        if isinstance(parent, list):
            del parent[int(last)]
        else:
            del parent[last]
    elif isinstance(parent, list):
        if last == "-":
            parent.append(value)
        else:
            parent[int(last)] = value
    else:
        if m.op == "add" and last in parent:
            raise SystemExit(f"{m.pointer} exists; an add must create it")
        if m.op == "replace" and last not in parent:
            raise SystemExit(f"{m.pointer} does not exist; a replace needs it")
        parent[last] = value
    return out


def json_pointer(path: Any) -> str:
    return "".join("/" + str(p).replace("~", "~0").replace("/", "~1") for p in path)


def leaves(error: jsonschema.ValidationError) -> Iterator[jsonschema.ValidationError]:
    yield error
    for sub in error.context or ():
        yield from leaves(sub)


def through_refs(schema: Json, path: list[object]) -> list[object]:
    """``path`` with a ``$ref`` step inserted wherever jsonschema followed a local reference
    (its schema paths leave the ``$ref`` hop out)."""
    node: Any = schema
    out: list[object] = []
    for step in path:
        while isinstance(node, dict) and step not in node and "$ref" in node:
            out.append("$ref")
            node = resolve(schema, str(node["$ref"]).removeprefix("#"))
        node = node[step]
        out.append(step)
    return out


def schema_keywords(node: object) -> set[str]:
    found: set[str] = set()
    if isinstance(node, dict):
        for key, value in node.items():
            found.add(key)
            if key in ("properties", "$defs"):
                for sub in value.values():
                    found |= schema_keywords(sub)
            else:
                found |= schema_keywords(value)
    elif isinstance(node, list):
        for sub in node:
            found |= schema_keywords(sub)
    return found


def invalid(_hash_seed: str) -> Files:
    schema = load_schema()
    validator = jsonschema.Draft202012Validator(schema, format_checker=jsonschema.FormatChecker())
    base_text = (RECORDS / INVALID_BASE / "results.json").read_text(encoding="utf-8")
    base = json.loads(base_text)
    if list(validator.iter_errors(base)):
        raise SystemExit(f"the base document {INVALID_BASE} does not validate")
    keywords = sorted(schema_keywords(schema))
    assertions = [k for k in keywords if k not in ANNOTATIONS]
    if sorted(m.keyword for m in MUTATIONS) != assertions:
        raise SystemExit(
            f"negative controls {sorted(m.keyword for m in MUTATIONS)} "
            f"do not cover the assertion keywords {assertions}"
        )
    files: Files = {}
    index: list[Json] = []
    for n, m in enumerate(MUTATIONS, 1):
        doc = mutate(base, m)
        errors = list(validator.iter_errors(doc))
        if len(errors) != 1:
            raise SystemExit(f"{m.keyword}: {len(errors)} errors, expected exactly one")
        (error,) = errors
        paths = [through_refs(schema, list(e.absolute_schema_path)) for e in leaves(error)]
        if not any(m.keyword in p for p in paths):
            raise SystemExit(f"{m.keyword}: no error path names it: {paths}")
        name = f"{n:02d}-{m.keyword.lstrip('$')}.json"
        files[INVALID_DIR / name] = (json.dumps(doc, ensure_ascii=False) + "\n").encode()
        index.append(
            {
                "file": name,
                "keyword": m.keyword,
                "mutation": [m.op, m.pointer, m.value],
                "pointer": json_pointer(error.absolute_path),
                "message": error.message,
                "validator": error.validator,
                "schema_path": json_pointer(through_refs(schema, list(error.absolute_schema_path))),
            }
        )
    files[INVALID_INDEX] = dumps(
        {
            "base": INVALID_BASE,
            "schema_keywords": keywords,
            "annotations": list(ANNOTATIONS),
            "controls": index,
        }
    )
    return files


# ---------------------------------------------------------------------------------------------
# Benchmark runs (T-073, T-082)


@dataclass(frozen=True)
class BenchRun:
    id: str
    python: tuple[str, ...]
    rust: tuple[str, ...]
    factory: str | None
    note: str


def report_manifest(out: Path) -> Json:
    results = json.loads((out / "results.json").read_text(encoding="utf-8"))
    files = {name: sha256((out / name).read_bytes()) for name in REPORT_FILES}
    replays = {p.name: sha256(p.read_bytes()) for p in sorted((out / "replays").glob("*.json"))}
    return {
        "files": files,
        "replays": replays,
        "masked": {
            "package": results["versions"]["package"],
            "code_digest": results["versions"]["code_digest"],
            "factory": results["ai"]["factory"],
        },
        "ai": results["ai"],
        "schema": results["schema"],
        "schema_version": results["schema_version"],
        "results_schema": results["versions"]["results_schema"],
    }


def bench_case(spec: BenchRun, hash_seed: str) -> Json:
    with tempfile.TemporaryDirectory(prefix="gcg-parity-") as tmp:
        out = Path(tmp) / "out"
        done = run(
            [str(PY_SIM), *spec.python, "--out", str(out)], child_env(hash_seed, spec.factory)
        )
        if done.returncode != 0:
            raise SystemExit(f"{spec.id}: Python exited {done.returncode}: {text(done.stderr)}")
        if not (out / "timing.json").exists():
            raise SystemExit(f"{spec.id}: no timing.json")
        return {
            "id": spec.id,
            "note": spec.note,
            "python_argv": ["gcg-sim", *spec.python, "--out", OUT_TOKEN],
            "factory_env": spec.factory,
            "rust_argv": ["gcg-eval", *spec.rust, "--out", OUT_TOKEN],
            "exit": done.returncode,
            "stdout": masked_text(done.stdout, out),
            "stderr": masked_text(done.stderr, out),
            **report_manifest(out),
        }


def bench_args(dut: str, bench: str, matches: int, seed: int, *rest: str) -> tuple[str, ...]:
    return (dut, bench, "--matches", str(matches), "--seed", str(seed), *rest)


TEST_RANDOM = (
    BenchRun(
        "conftest",
        ("benchmark", *bench_args(FED, ZEON, 60, 3, "--replays", "2")),
        (
            "reference-benchmark",
            *bench_args(FED, ZEON, 60, 3, "--replays", "2"),
            "--agent",
            "test-random",
        ),
        RANDOM_AGENT,
        "tests/reports/conftest.py run(): 60 BO3 matches, seed 3, 2 replays per result",
    ),
    BenchRun(
        "console-workers",
        ("benchmark", *bench_args(TEK, FED, 12, 77, "--workers", "2")),
        (
            "reference-benchmark",
            *bench_args(TEK, FED, 12, 77, "--workers", "4"),
            "--agent",
            "test-random",
        ),
        RANDOM_AGENT,
        "tests/cli/test_console_script.py benchmark(): Python at 2 workers, Rust at 4",
    ),
    BenchRun(
        "strong-bo1-log",
        (
            "benchmark",
            *bench_args(ZEON, TEK, 3, 2**64 - 1, "--format", "bo1", "--ai-preset", "strong"),
            "--decision-log",
            "--replays",
            "1",
        ),
        (
            "reference-benchmark",
            *bench_args(ZEON, TEK, 3, 2**64 - 1, "--format", "bo1", "--reference-preset", "strong"),
            "--decision-log",
            "--replays",
            "1",
            "--agent",
            "test-random",
        ),
        RANDOM_AGENT,
        "BO1 with decision logs, the largest seed and Python's preset name strong",
    ),
)

REFERENCE = tuple(
    BenchRun(
        f"reference-{preset}",
        (
            "benchmark",
            *bench_args(ZEON, FED, 1, 1, "--format", "bo1", "--ai-preset", preset),
            "--replays",
            "1",
        ),
        (
            "reference-benchmark",
            *bench_args(ZEON, FED, 1, 1, "--format", "bo1", "--reference-preset", preset),
            "--replays",
            "1",
        ),
        None,
        f"the reference search at Python's preset {preset}: one BO1 game",
    )
    for preset in ("fast", "standard", "strong")
)


def bench_section(path: Path, specs: tuple[BenchRun, ...]) -> Callable[[str], Files]:
    def build(hash_seed: str) -> Files:
        return {path: dumps({"cases": [bench_case(s, hash_seed) for s in specs]})}

    return build


# ---------------------------------------------------------------------------------------------
# compare (T-079)

COMPARE = (
    ("fast", "greedy", "--games", "48", "--seed", "11", "--decks", "tests"),
    ("standard", "random", "--games", "24", "--seed", "12", "--decks", "examples"),
    ("baseline-standard", "fast", "--games", "24", "--seed", "13", "--decks", "tuning"),
)


def compare_section(hash_seed: str) -> Files:
    cases = []
    for argv in COMPARE:
        with tempfile.TemporaryDirectory(prefix="gcg-parity-") as tmp:
            out = Path(tmp) / "report.json"
            done = run(
                [
                    sys.executable,
                    "-m",
                    "gcg_sim.ai.compare",
                    *argv,
                    "--workers",
                    "4",
                    "--out",
                    str(out),
                ],
                child_env(hash_seed),
            )
            if done.returncode != 0:
                raise SystemExit(f"compare {argv}: exit {done.returncode}: {text(done.stderr)}")
            cases.append(
                {
                    "argv": [*argv, "--workers", "4"],
                    "stdout": text(done.stdout),
                    "out": out.read_text(encoding="utf-8"),
                }
            )
    return {EVAL_VECTORS / "compare.json": dumps({"cases": cases})}


# ---------------------------------------------------------------------------------------------
# Replays (T-004)


def replay_transcripts(path: Path, hash_seed: str) -> Json:
    env = child_env(hash_seed)
    plain = run([str(PY_SIM), "replay", str(path)], env)
    logged = run([str(PY_SIM), "replay", "--log", str(path)], env)
    return {
        "exit": plain.returncode,
        "stdout": text(plain.stdout),
        "stderr": text(plain.stderr),
        "log_exit": logged.returncode,
        "log_sha256": sha256(logged.stdout),
        "log_lines": len(text(logged.stdout).splitlines()),
        "log_stderr": text(logged.stderr),
    }


def python_replays(hash_seed: str) -> Files:
    cases = []
    for path in sorted(RECORDS.glob("*/replays/*.json")):
        rel = path.relative_to(ROOT).as_posix()
        case: Json = {"file": rel, "sha256": sha256(path.read_bytes())}
        case |= replay_transcripts(path.relative_to(ROOT), hash_seed)
        if case["exit"] != 0 or case["log_exit"] != 0:
            raise SystemExit(f"Python does not verify its own replay {rel}: {case['stderr']}")
        cases.append(case)
    return {CLI_VECTORS / "python-replays.json": dumps({"cases": cases})}


@dataclass(frozen=True)
class RustRun:
    id: str
    binary: str
    argv: tuple[str, ...]
    note: str


RUST_RUNS = (
    RustRun(
        "sim-fast-bo1",
        "gcg-sim",
        ("benchmark", *bench_args(TEK, FED, 3, 77, "--format", "bo1", "--ai-preset", "fast")),
        "gcg-sim, the shipped fast preset, BO1",
    ),
    RustRun(
        "sim-fast-bo3-log",
        "gcg-sim",
        (
            "benchmark",
            *bench_args(FED, ZEON, 2, 9, "--ai-preset", "fast", "--workers", "2"),
            "--decision-log",
            "--replays",
            "1",
        ),
        "gcg-sim, the shipped fast preset, BO3 with decision logs",
    ),
    RustRun(
        "sim-standard-bo1",
        "gcg-sim",
        ("benchmark", *bench_args(ZEON, TEK, 1, 5, "--format", "bo1"), "--replays", "1"),
        "gcg-sim, the default standard preset, BO1",
    ),
    RustRun(
        "eval-reference-fast",
        "gcg-eval",
        (
            "reference-benchmark",
            *bench_args(ZEON, FED, 2, 4, "--format", "bo1", "--reference-preset", "fast"),
            "--replays",
            "1",
        ),
        "gcg-eval reference-benchmark, the reference search at Python's fast budget",
    ),
)


def rust_replays(hash_seed: str) -> Files:
    for binary in (RUST_SIM, RUST_EVAL):
        if not binary.is_file():
            raise SystemExit(f"{binary} is missing; run cargo build --release first")
    runs = []
    with tempfile.TemporaryDirectory(prefix="gcg-parity-rust-") as tmp:
        for spec in RUST_RUNS:
            out = Path(tmp) / spec.id
            exe = RUST_SIM if spec.binary == "gcg-sim" else RUST_EVAL
            done = run([str(exe), *spec.argv, "--out", str(out)], child_env(hash_seed))
            if done.returncode != 0:
                raise SystemExit(f"{spec.id}: Rust exited {done.returncode}: {text(done.stderr)}")
            replays: dict[str, Json] = {}
            for path in sorted((out / "replays").glob("*.json")):
                case: Json = {"sha256": sha256(path.read_bytes())}
                case |= replay_transcripts(path, hash_seed)
                if case["exit"] != 0 or case["log_exit"] != 0:
                    raise SystemExit(f"Python rejects the Rust replay {spec.id}/{path.name}")
                replays[path.name] = case
            if not replays:
                raise SystemExit(f"{spec.id} wrote no replays")
            runs.append(
                {
                    "id": spec.id,
                    "note": spec.note,
                    "argv": [spec.binary, *spec.argv, "--out", OUT_TOKEN],
                    "replays": replays,
                }
            )
    return {CLI_VECTORS / "rust-replays.json": dumps({"runs": runs})}


# ---------------------------------------------------------------------------------------------

SECTIONS: dict[str, tuple[Callable[[str], Files], tuple[Path, ...]]] = {
    "collect": (collect, (SUITES_OUT,)),
    "invalid": (invalid, (INVALID_DIR, INVALID_INDEX)),
    "test-random": (
        bench_section(EVAL_VECTORS / "test-random.json", TEST_RANDOM),
        (EVAL_VECTORS / "test-random.json",),
    ),
    "reference": (
        bench_section(EVAL_VECTORS / "reference.json", REFERENCE),
        (EVAL_VECTORS / "reference.json",),
    ),
    "compare": (compare_section, (EVAL_VECTORS / "compare.json",)),
    "python-replays": (python_replays, (CLI_VECTORS / "python-replays.json",)),
    "rust-replays": (rust_replays, (CLI_VECTORS / "rust-replays.json",)),
}


def existing(owned: tuple[Path, ...]) -> dict[Path, bytes]:
    found: dict[Path, bytes] = {}
    for path in owned:
        if path.is_dir():
            found |= {p: p.read_bytes() for p in sorted(path.rglob("*")) if p.is_file()}
        elif path.is_file():
            found[path] = path.read_bytes()
    return found


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true", help="fail if the committed files are stale")
    ap.add_argument(
        "--section",
        action="append",
        choices=list(SECTIONS),
        help="only these sections (repeatable; default all)",
    )
    args = ap.parse_args()
    os.chdir(ROOT)
    hash_seed = "2" if args.check else "1"
    status = 0
    for name in args.section or list(SECTIONS):
        build, owned = SECTIONS[name]
        files = build(hash_seed)
        current = existing(owned)
        if args.check:
            stale = sorted(
                str(p.relative_to(ROOT))
                for p in set(files) | set(current)
                if files.get(p) != current.get(p)
            )
            if stale:
                print(f"{name}: stale ({len(stale)} files, first {stale[0]})", file=sys.stderr)
                status = 1
            else:
                print(f"{name}: current ({len(files)} files)")
            continue
        for path in current:
            if path not in files:
                path.unlink()
        for path, data in files.items():
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
        print(f"{name}: wrote {len(files)} files")
    return status


if __name__ == "__main__":
    sys.exit(main())
