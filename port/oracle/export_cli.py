"""Export Python 3.15 CLI transcripts and parser vectors for the Rust ``gcg-sim`` parser.

Writes these files (docs/port/PLAN.md §10.4 E, A30, T-072, T-152):

- ``crates/gcg-cli/tests/vectors/cli.json``: argparse transcripts (exit code, stdout, stderr,
  parsed namespace) of the real parser (``gcg_sim.cli.build_parser``) and of the same parser
  with the two Rust differences (``--ai-preset standard|fast`` with the brief's wording,
  ``--workers N|auto``), at several ``COLUMNS`` widths and with a non-TTY stdout; plus vectors
  for ``int(text, 0)``, ``int(text)``, ``repr(str)``, ``difflib.get_close_matches``,
  ``str(pathlib.Path(text))`` and ``textwrap.wrap``.
- ``crates/gcg-runner/src/argparse/unicode_tables.rs``: the ``str.isspace`` and
  decimal-digit tables of this Python's ``unicodedata`` (``repr`` uses gcg-data's
  ``isprintable`` table through ``gcg_data::py_repr``).
- ``crates/gcg-cli/tests/vectors/commands.json`` and the input files under
  ``crates/gcg-cli/tests/fixtures/``: transcripts (exit code, stdout, stderr) of
  ``gcg_sim.cli.main`` running ``validate``, ``data status``, ``replay`` and the ``benchmark``
  checks that fail before any game is played, from the repository root with relative paths.
  Each case says whether it needs the engine (``replay`` of a real replay re-simulates the
  game) and how the Rust transcript must relate: ``same`` (byte-identical once Python's
  ``versions.package`` and ``versions.code_digest`` values are replaced with the Rust build's)
  or ``prefix`` (CPython's ``JSONDecodeError`` text is not reproduced, PLAN A19: stderr must
  agree up to and including the text before it).

Every case also says how the two parsers' transcripts relate (``same``: byte-identical;
``usage``: only the benchmark usage block differs, the error line is identical; ``rust``: an
intended difference of ``--ai-preset`` or ``--workers``). The exporter asserts each claim, so
T-152's "equal except --ai-preset and --workers auto" is checked here and again in Rust. The
real parser's transcript is stored as ``"same"`` when it equals the Rust-shaped one.

Transcripts are produced in-process (``parse_args`` with redirected streams, encoded as the
real process would: stdout with ``surrogateescape``, stderr with ``backslashreplace``). A
subset (``relation: subprocess``, which only exits with help or a usage error, so no command
runs) is also run through the installed ``gcg-sim`` console script and through a child
process of this script, with unset and malformed ``COLUMNS``, and must give the same bytes.

    uv run python port/oracle/export_cli.py --write
    uv run python port/oracle/export_cli.py --check    # exit 1 if a file is stale
"""

from __future__ import annotations

import argparse
import contextlib
import difflib
import io
import json
import os
import re
import subprocess
import sys
import sysconfig
import textwrap
import unicodedata
from collections.abc import Callable, Iterator, Sequence
from pathlib import Path
from typing import Any

from gcg_sim import cli

ROOT = Path(__file__).resolve().parents[2]
VECTORS = ROOT / "crates" / "gcg-cli" / "tests" / "vectors" / "cli.json"
COMMANDS = ROOT / "crates" / "gcg-cli" / "tests" / "vectors" / "commands.json"
FIXTURES = ROOT / "crates" / "gcg-cli" / "tests" / "fixtures"
TABLES = ROOT / "crates" / "gcg-runner" / "src" / "argparse" / "unicode_tables.rs"

# The Rust --ai-preset and --workers help (BRIEF <ai_presets>, PLAN A5, A11, D12, §7.4).
# crates/gcg-cli/src/cli.rs builds the same text from gcg_search::Preset::help().
RUST_PRESET_HELP = (
    "AI strength. standard (default): uses the full search so that each player plays at the "
    "level of the strongest top-tier competitive player; for refining and polishing the final "
    "deck. fast: trades search depth for throughput; for rapidly screening deck variants, "
    "sub-variants and hypothetical builds. Both start from the budgets of the Python presets "
    "strong (standard) and standard (fast); docs/AI.md has the table"
)
RUST_WORKERS_HELP = (
    "worker processes; auto uses one per performance-core CPU (at most one per match)"
)

HELP_WIDTHS = ("1", "20", "40", "60", "79", "80", "81", "100", "120", "200")
ERROR_WIDTHS = ("40", "80", "200")
DEFAULT_COLUMNS = "80"

Arg = str | bytes


def _workers(text: str) -> object:
    if text == "auto":
        return "auto"
    return cli._positive_int(text)


# The Rust converter reports numeric failures exactly as Python's _positive_int.
_workers.__name__ = "_positive_int"


def _subparsers(parser: argparse.ArgumentParser) -> argparse._SubParsersAction[Any]:
    for action in parser._actions:
        if isinstance(action, argparse._SubParsersAction):
            return action
    raise AssertionError("gcg-sim has subcommands")


def python_parser() -> argparse.ArgumentParser:
    return cli.build_parser()


def rust_parser() -> argparse.ArgumentParser:
    """``build_parser()`` with the two Rust differences and nothing else."""
    parser = cli.build_parser()
    bench = _subparsers(parser).choices["benchmark"]
    changed = set()
    for action in bench._actions:
        if action.dest == "ai_preset":
            action.choices = ("standard", "fast")
            action.help = RUST_PRESET_HELP
            changed.add(action.dest)
        elif action.dest == "workers":
            action.type = _workers
            action.help = RUST_WORKERS_HELP
            changed.add(action.dest)
    assert changed == {"ai_preset", "workers"}, changed
    return parser


PARSERS: dict[str, Callable[[], argparse.ArgumentParser]] = {
    "python": python_parser,
    "rust": rust_parser,
}


# ---------------------------------------------------------------------------------------------
# text encoding for JSON (lone surrogates cannot travel in JSON strings)


def _decode_arg(arg: Arg) -> str:
    return os.fsdecode(arg) if isinstance(arg, bytes) else arg


def text_json(s: str) -> object:
    """``s`` as a JSON string, or ``{"cps": [...]}`` (its code points) when it holds a lone
    surrogate, which JSON strings cannot carry (an undecodable argv byte decodes to one)."""
    try:
        s.encode("utf-8")
    except UnicodeEncodeError:
        return {"cps": [ord(c) for c in s]}
    return s


def stream_text(text: str, errors: str) -> str:
    data = text.encode("utf-8", errors)
    return data.decode("utf-8")


# ---------------------------------------------------------------------------------------------
# transcripts


def int_str(value: int) -> str:
    """``str(value)`` without the 4300-digit limit (the vectors hold such values)."""
    old = sys.get_int_max_str_digits()
    sys.set_int_max_str_digits(0)
    try:
        return str(value)
    finally:
        sys.set_int_max_str_digits(old)


def namespace_json(ns: argparse.Namespace) -> dict[str, object]:
    out: dict[str, object] = {}
    for key, value in sorted(vars(ns).items()):
        if isinstance(value, bool):
            out[key] = ["bool", "True" if value else "False"]
        elif isinstance(value, int):
            out[key] = ["int", int_str(value)]
        elif isinstance(value, Path):
            out[key] = ["path", text_json(str(value))]
        elif isinstance(value, str):
            out[key] = ["str", text_json(value)]
        elif value is None:
            out[key] = ["none", ""]
        else:
            raise AssertionError(f"unexpected namespace value {key}={value!r}")
    return out


@contextlib.contextmanager
def columns_env(columns: str) -> Iterator[None]:
    saved = {k: os.environ.get(k) for k in ("COLUMNS", "LINES")}
    os.environ["COLUMNS"] = columns
    os.environ["LINES"] = "24"
    try:
        yield
    finally:
        for key, value in saved.items():
            if value is None:
                os.environ.pop(key, None)
            else:
                os.environ[key] = value


def run_inprocess(shape: str, argv: Sequence[Arg], columns: str) -> dict[str, object]:
    out, err = io.StringIO(), io.StringIO()
    namespace: dict[str, object] | None = None
    code: object = 0
    with columns_env(columns), contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
        try:
            namespace = namespace_json(PARSERS[shape]().parse_args([_decode_arg(a) for a in argv]))
        except SystemExit as exc:
            code = exc.code
    assert isinstance(code, int), code
    return {
        "exit": code,
        "stdout": stream_text(out.getvalue(), "surrogateescape"),
        "stderr": stream_text(err.getvalue(), "backslashreplace"),
        "namespace": namespace,
    }


def _child_env(columns: str | None) -> dict[str, str]:
    env = {k: v for k, v in os.environ.items() if k not in {"COLUMNS", "LINES"}}
    for key in ("FORCE_COLOR", "PYTHON_COLORS", "NO_COLOR"):
        env.pop(key, None)
    if columns is not None:
        env["COLUMNS"] = columns
    return env


def _bytes_arg(arg: Arg) -> bytes:
    return arg if isinstance(arg, bytes) else os.fsencode(arg)


def run_subprocess(shape: str, argv: Sequence[Arg], columns: str | None) -> dict[str, object]:
    """The same case in a fresh process with piped (non-TTY) streams: the installed console
    script for the Python parser, a child of this script for the Rust-shaped one."""
    if shape == "python":
        script = Path(sysconfig.get_path("scripts")) / "gcg-sim"
        cmd: list[bytes] = [os.fsencode(script)]
    else:
        cmd = [os.fsencode(sys.executable), os.fsencode(__file__), b"--child-rust"]
    done = subprocess.run(
        [*cmd, *(_bytes_arg(a) for a in argv)],
        env=_child_env(columns),
        capture_output=True,
        check=False,
    )
    return {
        "exit": done.returncode,
        "stdout": done.stdout.decode("utf-8"),
        "stderr": done.stderr.decode("utf-8"),
        "namespace": None,
    }


def child_rust(argv: list[str]) -> int:
    """``cli.main``'s parse step with the Rust-shaped parser (``cli.py:386-391``)."""
    try:
        rust_parser().parse_args(argv)
    except SystemExit as exc:
        return exc.code if isinstance(exc.code, int) else cli.EXIT_USAGE
    return cli.EXIT_OK


# ---------------------------------------------------------------------------------------------
# cases

D1 = "examples/decks/blue-white-federation.txt"
D2 = "examples/decks/red-green-zeon.txt"
BENCH: list[Arg] = ["benchmark", D1, D2]


def _bench(*extra: Arg) -> list[Arg]:
    return [*BENCH, "--matches", "2", *extra]


HELP_CASES: list[tuple[list[Arg], str]] = [
    (["--help"], "same"),
    (["benchmark", "--help"], "rust"),
    (["validate", "--help"], "same"),
    (["data", "--help"], "same"),
    (["data", "status", "--help"], "same"),
    (["replay", "--help"], "same"),
]

# (argv, relation of the two transcripts)
CASES: list[tuple[list[Arg], str]] = [
    # help, wherever it is reached
    (["-h"], "same"),
    (["--he"], "same"),
    (["--h"], "same"),
    (["-h", "benchmark"], "same"),
    (["-hx"], "same"),
    (["-h=x"], "same"),
    (["--help=x"], "same"),
    (["benchmark", "-h"], "rust"),
    (["benchmark", "--hel"], "rust"),
    (["benchmark", "-hfoo"], "rust"),
    (["benchmark", "-h-x"], "usage"),
    (["benchmark", "--help", "--matches", "x"], "rust"),
    (["benchmark", "--matches", "x", "--help"], "usage"),
    (["data", "status", "--json", "-h"], "same"),
    (["replay", "x.json", "--help"], "same"),
    # missing and unknown subcommands
    ([], "same"),
    (["validate"], "same"),
    (["data"], "same"),
    (["replay"], "same"),
    (["frobnicate"], "same"),
    (["benchmrk"], "same"),
    (["Benchmark"], "same"),
    (["valid"], "same"),
    (["data", "stat"], "same"),
    (["data", "statuss"], "same"),
    (["1"], "same"),
    (["--bogus"], "same"),
    (["-x"], "same"),
    (["--m", "1", "benchmark"], "same"),
    (["--", "validate", "a.txt"], "same"),
    # benchmark: required arguments
    (["benchmark"], "usage"),
    (["benchmark", D1], "usage"),
    (BENCH, "usage"),
    (["benchmark", "--matches", "1"], "usage"),
    (["benchmark", D1, "--", D2, "--matches", "1"], "usage"),
    # integer options
    (_bench("--matches", "0"), "usage"),
    ([*BENCH, "--matches", "0"], "usage"),
    ([*BENCH, "--matches", "x"], "usage"),
    ([*BENCH, "--matches", "-1"], "usage"),
    ([*BENCH, "--matches", ""], "usage"),
    ([*BENCH, "--matches", "1.5"], "usage"),
    ([*BENCH, "--matches", "1e3"], "usage"),
    ([*BENCH, "--matches", "010"], "usage"),
    ([*BENCH, "--matches", "00"], "usage"),
    ([*BENCH, "--matches", "1__0"], "usage"),
    ([*BENCH, "--matches", "_1"], "usage"),
    ([*BENCH, "--matches", "0x"], "usage"),
    ([*BENCH, "--matches", "-0"], "usage"),
    ([*BENCH, "--matches", "-0x10"], "usage"),
    ([*BENCH, "--matches", "\x1c1"], "usage"),
    ([*BENCH, "--matches", "1\n"], "same"),
    ([*BENCH, "--matches", "3", "--matches", "x'y"], "usage"),
    ([*BENCH, "--matches", 'a"b'], "usage"),
    ([*BENCH, "--matches", "a'b\"c"], "usage"),
    ([*BENCH, "--matches", "tab\there\\"], "usage"),
    ([*BENCH, "--matches", "bö3"], "usage"),
    ([*BENCH, "--matches", "\U0000200b1"], "usage"),
    ([*BENCH, "--matches", "\U0001f600"], "usage"),
    ([*BENCH, "--matches", "\x7f"], "usage"),
    ([*BENCH, "--matches", "-" + "9" * 4301], "usage"),
    ([*BENCH, "--matches", "-0x" + "f" * 3571], "usage"),
    ([*BENCH, "--matches", "-0x" + "f" * 3572], "usage"),
    ([*BENCH, "--matches"], "usage"),
    ([*BENCH, "--matches", "--seed", "1"], "usage"),
    ([*BENCH, "--matches", "-x"], "usage"),
    ([*BENCH, "--matches", "-h"], "usage"),
    (_bench("--seed", "-1"), "usage"),
    (_bench("--seed", str(2**64)), "usage"),
    (_bench("--seed", "0x1" + "0" * 16), "usage"),
    (_bench("--seed", "0x" + "f" * 3572), "usage"),
    (_bench("--seed", "x"), "usage"),
    (_bench("--replays", "-1"), "usage"),
    (_bench("--replays", "x"), "usage"),
    (_bench("--replays", "-0o1" + "0" * 4767), "usage"),
    (_bench("--workers", "0"), "usage"),
    (_bench("--workers", "x"), "usage"),
    (_bench("--workers", "Auto"), "usage"),
    (_bench("--workers", " auto"), "usage"),
    (_bench("--workers", "-0o7_7"), "usage"),
    (_bench("--workers", "-0x" + "f" * 3572), "usage"),
    (_bench("--workers", "auto"), "rust"),
    # choices
    (_bench("--format", "bo5"), "usage"),
    (_bench("--format", "BO3"), "usage"),
    (_bench("--format", "b"), "usage"),
    (_bench("--format", ""), "usage"),
    (_bench("--format", "bo3 "), "usage"),
    (_bench("--format", "bö3"), "usage"),
    (_bench("--format", "bo3\n"), "usage"),
    (_bench("--format", "o" * 300 + "b3"), "usage"),
    (_bench("--format", "bo1" * 80), "usage"),
    (_bench("--ai-preset", "weak"), "rust"),
    (_bench("--ai-preset", "max"), "rust"),
    (_bench("--ai-preset", "strong"), "rust"),
    (_bench("--ai-preset", "Standard"), "rust"),
    (_bench("--ai-preset", "strng"), "rust"),
    (_bench("--ai-preset", "fst"), "rust"),
    (_bench("--ai-preset", ""), "rust"),
    (_bench("--ai-preset", "reference"), "rust"),
    # flags and explicit arguments
    (_bench("--decision-log=1"), "usage"),
    (_bench("--decision-log="), "usage"),
    (_bench("--decision-lo=yes"), "usage"),
    (["data", "status", "--json=1"], "same"),
    (["data", "status", "--js=", "x"], "same"),
    # abbreviations and ambiguity
    (_bench("--=x"), "usage"),
    (_bench("--="), "usage"),
    (_bench("--mat=x"), "usage"),
    (_bench("--format=bo7"), "usage"),
    (_bench("--for", "bo9"), "usage"),
    (_bench("--ai=strong"), "rust"),
    # unrecognized arguments (reported by the top-level parser)
    ([*_bench(), "extra"], "same"),
    ([*_bench(), "e1", "--e2", "e3"], "same"),
    ([*_bench(), "--seed=3", "--wat"], "same"),
    (["validate", "a.txt", "b.txt"], "same"),
    (["validate", "--x", "a.txt"], "same"),
    (["validate", "a.txt", "-x"], "same"),
    (["validate", "a.txt", "--x=1", "-y z"], "same"),
    (["data", "status", "now"], "same"),
    (["replay", "a.json", "--logs"], "same"),
    (["replay", "a.json", "--l", "--lo", "--log"], "same"),
    (["validate", "-x"], "same"),
    # undecodable argv bytes (surrogateescape)
    (["validate", "a.txt", b"\xffextra"], "same"),
    ([b"bogus\xff"], "same"),
    ([b"--\xff"], "same"),
    (_bench("--format", b"\xff"), "usage"),
    (_bench("--matches", b"\xe2\x28\xa1"), "usage"),
    (_bench("--format", "\udcff".encode("utf-8", "surrogatepass")), "usage"),
    # parsed successfully (no output; the namespace is compared)
    (["validate", D1], "same"),
    (["validate", "--", "--weird"], "same"),
    (["validate", "-1"], "same"),
    (["validate", "-.5"], "same"),
    (["validate", "-٣"], "same"),
    (["validate", "-x y"], "same"),
    (["validate", ""], "same"),
    (["validate", "-"], "same"),
    (["validate", "./a//b/./c/"], "same"),
    (["validate", "//x"], "same"),
    (["validate", "///x"], "same"),
    (["validate", b"\xff.txt"], "same"),
    (["vali" + "date", "a.txt"], "same"),
    (["data", "status"], "same"),
    (["data", "status", "--json"], "same"),
    (["data", "status", "--j"], "same"),
    (["replay", "r.json", "--log"], "same"),
    (["replay", "--lo", "r.json"], "same"),
    (_bench(), "same"),
    (
        [
            "benchmark",
            "--matches",
            "2",
            D1,
            "--seed",
            "0xff",
            D2,
            "--format",
            "bo1",
            "--out",
            "./out/",
            "--decision-log",
            "--replays",
            "0",
            "--workers",
            "4",
            "--ai-preset",
            "fast",
        ],
        "same",
    ),
    (
        _bench("--m", "7", "--s", "1_0", "--w", "3", "--f", "bo1", "--o", "x", "--d", "--r", "5"),
        "same",
    ),
    (_bench("--matches=9", "--seed=-0"), "same"),
    (_bench("--matches", " 12 "), "same"),
    (_bench("--matches", "١٢"), "same"),
    (_bench("--matches", "\U000000a012\U00002003"), "same"),
    (_bench("--matches", "１２", "--seed", "0b1_0"), "same"),
    (_bench("--matches", "0x" + "f" * 3572), "same"),
    (_bench("--seed", "18446744073709551615"), "same"),
    (_bench("--seed", "0O17"), "same"),
    (_bench("--replays", "0_0"), "same"),
    (_bench("--ai-preset", "standard"), "same"),
    (_bench("--workers", "99999999999999999999"), "same"),
    ([*BENCH, "--matches", "4294967295"], "same"),
    ([*BENCH, "--matches", "4294967296"], "same"),
    (_bench("--replays", "99999999999"), "same"),
]

# Cases also run in fresh processes, with the COLUMNS value (None: unset).
SUBPROCESS_CASES: list[tuple[list[Arg], str | None]] = [
    (["--help"], None),
    (["--help"], "abc"),
    (["--help"], "0"),
    (["--help"], "-5"),
    (["--help"], " 60 "),
    (["--help"], "6_0"),
    (["--help"], "٦٠"),
    (["--help"], "\U000000a050"),
    (["--help"], ""),
    (["benchmark", "--help"], None),
    (["benchmark", "--help"], "1"),
    (["benchmark", "--help"], "45"),
    (["data", "status", "--help"], "30"),
    ([], None),
    (["frobnicate"], None),
    (BENCH, None),
    (_bench("--format", "bo5"), None),
    (_bench("--ai-preset", "weak"), None),
    (_bench("--matches", "\x1c1"), None),
    (["validate", "a.txt", b"\xffextra"], None),
    (_bench("--format", b"\xff"), None),
    ([b"bogus\xff"], None),
    (_bench("--matches", "-0x" + "f" * 3572), None),
]


def argv_json(argv: Sequence[Arg]) -> list[object]:
    return [{"hex": a.hex()} if isinstance(a, bytes) else text_json(a) for a in argv]


def _error_line(stderr: object) -> str:
    assert isinstance(stderr, str)
    lines = stderr.splitlines()
    return lines[-1] if lines else ""


def _check_relation(
    argv: Sequence[Arg], relation: str, py: dict[str, object], rs: dict[str, object]
) -> None:
    if relation == "same":
        assert py == rs, (argv, py, rs)
    elif relation == "usage":
        assert py != rs, (argv, "expected a different benchmark usage")
        assert py["exit"] == rs["exit"] == 2, (argv, py["exit"], rs["exit"])
        assert py["stdout"] == rs["stdout"] == "", argv
        assert _error_line(py["stderr"]) == _error_line(rs["stderr"]), (argv, py, rs)
        for shape in (py, rs):
            assert str(shape["stderr"]).startswith("usage: gcg-sim benchmark"), argv
    elif relation == "rust":
        assert py != rs, (argv, "expected an intended difference")
    else:
        raise AssertionError(relation)


def transcripts() -> list[dict[str, object]]:
    out: list[dict[str, object]] = []
    help_cases = [(argv, rel, width) for argv, rel in HELP_CASES for width in HELP_WIDTHS]
    # Short command lines are also checked at a narrow and a wide terminal.
    error_cases = [
        (argv, rel, width)
        for argv, rel in CASES
        for width in (ERROR_WIDTHS if len(argv) <= 3 else (DEFAULT_COLUMNS,))
    ]
    for argv, relation, width in help_cases + error_cases:
        py = run_inprocess("python", argv, width)
        rs = run_inprocess("rust", argv, width)
        _check_relation(argv, relation, py, rs)
        out.append(
            {
                "argv": argv_json(argv),
                "columns": width,
                "relation": relation,
                "python": "same" if py == rs else py,
                "rust": rs,
            }
        )
    for argv, columns in SUBPROCESS_CASES:
        entry: dict[str, object] = {
            "argv": argv_json(argv),
            "columns": columns,
            "relation": "subprocess",
        }
        for shape in ("python", "rust"):
            width = _effective_columns(columns)
            expect = run_inprocess(shape, argv, width)
            # Only help and usage errors run in a child: a parsed command line would run the
            # real command (a benchmark) in the console script.
            assert expect["namespace"] is None, (shape, argv, "parses; not run in a child")
            got = run_subprocess(shape, argv, columns)
            assert got == expect, (shape, argv, columns, got, expect)
            entry[shape] = got
        out.append(entry)
    return out


def _effective_columns(columns: str | None) -> str:
    """``shutil.get_terminal_size`` with a non-TTY stdout: ``int(COLUMNS)`` if positive,
    else the fallback 80."""
    if columns is None:
        return DEFAULT_COLUMNS
    try:
        value = int(columns)
    except ValueError:
        return DEFAULT_COLUMNS
    return str(value) if value > 0 else DEFAULT_COLUMNS


# ---------------------------------------------------------------------------------------------
# value vectors

INT_TEXTS: list[str] = [
    "0",
    "1",
    "-1",
    "+1",
    "007",
    "00",
    "0_0",
    "0_1",
    "010",
    "1_000",
    "1__0",
    "_1",
    "1_",
    "0x10",
    "0X1f",
    "0x_1",
    "0x__1",
    "0x",
    "0o17",
    "0O7",
    "0o8",
    "0b101",
    "0B2",
    "0b",
    "1.5",
    "1e3",
    "",
    " ",
    " 7 ",
    "\t7\n",
    "\x0b7\x0c",
    "7\r",
    "\x1c7",
    "7\x1f",
    "\x857",
    "\U000000a012\U00002003",
    "\U000030007",
    "٣",
    "١٢",
    "１２",
    "-١",
    "0x١",
    "١_٢",
    "٠١",
    "٠",
    "𝟏𝟐",
    "²",
    "½",
    "Ⅻ",
    "abc",
    "0xg",
    "--1",
    "+-1",
    "- 1",
    "1 2",
    "9" * 4300,
    "9" * 4301,
    "-" + "9" * 4300,
    "0x" + "f" * 3572,
    "-0x" + "f" * 3571,
    "-0x" + "f" * 3572,
    "18446744073709551615",
    "18446744073709551616",
    "-9223372036854775808",
    "0o" + "7" * 30,
    "0b" + "1" * 70,
    "\x00",
    "1\x00",
    "١" + "٢" * 10,
]

REPR_TEXTS: list[str] = [
    "",
    "a",
    "it's",
    'say "hi"',
    "a'b\"c",
    "tab\there",
    "back\\slash",
    "nl\n",
    "cr\r",
    "\x00\x07\x1b\x7f",
    "bö3",
    "Ⅱ①",
    "\U0000200b",
    "\U000000a0",
    "\U00002028",
    "\U0001f600",
    "\U000e0001",
    "\ud800",
    "\udcff",
    "\U0000ffff",
    "\U0010ffff",
    "\x85",
    "\xa0\xad",
    "日本語",
    "\U00000378",
    "'\"",
    "''",
    '""',
    "\\'",
]

CLOSE_MATCHES: list[tuple[str, list[str]]] = [
    ("bo5", ["bo3", "bo1"]),
    ("BO3", ["bo3", "bo1"]),
    ("b", ["bo3", "bo1"]),
    ("o", ["bo3", "bo1"]),
    ("bo", ["bo3", "bo1"]),
    ("", ["bo3", "bo1"]),
    ("bo3 ", ["bo3", "bo1"]),
    ("strong", ["standard", "fast"]),
    ("strong", ["fast", "standard", "strong"]),
    ("weak", ["standard", "fast"]),
    ("fst", ["standard", "fast"]),
    ("stand", ["standard", "fast"]),
    ("benchmrk", ["benchmark", "validate", "data", "replay"]),
    ("dat", ["benchmark", "validate", "data", "replay"]),
    ("repaly", ["benchmark", "validate", "data", "replay"]),
    ("ab", ["ba", "ab"]),
    ("ab", ["ab", "ba"]),
    ("xy", ["yx", "xz"]),
    ("o" * 300 + "b3", ["bo3", "bo1"]),
    ("bo1" * 80, ["bo3", "bo1"]),
    ("bo3" * 70, ["bo3" * 69, "bo3"]),
    ("a" * 250 + "b" * 3, ["a" * 200 + "b", "b" * 3]),
    ("statuss", ["status"]),
    ("bö3", ["bo3", "bo1"]),
]

PATH_TEXTS: list[str] = [
    "",
    ".",
    "./",
    "a",
    "a/",
    "a//b",
    "./a//b/./c/",
    "/",
    "//",
    "///",
    "//x",
    "///x",
    "/a/../b",
    "..",
    "a/./.",
    "./gcg-sim-out/",
    "a b",
    "-",
    "~",
    "x\\y",
]

WRAP_TEXTS: list[str] = [
    "show this help message and exit",
    "Gundam Card Game deck benchmarking simulator.",
    "play a deck under test against a benchmark deck",
    "check a deck file against the construction rules",
    "data snapshot, rules, B&R, coverage, conflicts",
    "re-simulate a saved replay and verify its outcome",
    RUST_PRESET_HELP,
    RUST_WORKERS_HELP,
    "Hello there -- you goof-ball, use the -b option!",
    "Look, goof-ball -- use the -b option!",
    "a-b-c-d e--f g---h i-j-",
    "--matches --seed x--y -leading trailing- a--b e.g.--foo top-tier",
    "well-known long-word-with-hyphens and-some-more-hyphenated-words",
    "abcdefghijklmnopqrstuvwxyz0123456789 supercalifragilisticexpialidocious",
    "x" * 30 + "-" + "y" * 30,
    "-" * 25 + "abc",
    "_a-b 1a-b a1-b ab-1c ab-c1 a_-b ab-_c ab--cd",
    'it\'s a "quote"--done, ok? yes! & more.',
]
WRAP_WIDTHS = (1, 2, 3, 5, 8, 11, 12, 15, 20, 25, 30, 33, 40, 50, 54, 56, 60, 70, 80, 100)


def int_vectors() -> list[dict[str, object]]:
    out = []
    for text in INT_TEXTS:
        entry: dict[str, object] = {"text": text}
        for key, base in (("base0", 0), ("base10", 10)):
            try:
                value = int(text, base)
            except ValueError:
                entry[key] = None
                continue
            try:
                str(value)
                printable = True
            except ValueError:
                printable = False
            entry[key] = {"value": int_str(value), "str_ok": printable}
        out.append(entry)
    return out


def tables_rs() -> str:
    space: list[tuple[int, int]] = []
    decimal: list[tuple[int, int, int]] = []
    digit_re = re.compile(r"\d")
    for cp in range(0x110000):
        ch = chr(cp)
        if ch.isspace():
            if space and space[-1][1] == cp - 1:
                space[-1] = (space[-1][0], cp)
            else:
                space.append((cp, cp))
        is_digit = bool(digit_re.match(ch))
        assert is_digit == ch.isdecimal(), hex(cp)
        if ch.isdecimal():
            value = int(ch)
            last = decimal[-1] if decimal else None
            if last and last[1] == cp - 1 and last[2] + (cp - last[0]) == value:
                decimal[-1] = (last[0], cp, last[2])
            else:
                decimal.append((cp, cp, value))

    def ranges(name: str, doc: str, items: list[tuple[int, int]]) -> list[str]:
        lines = [f"/// {doc}", "#[rustfmt::skip]", f"pub const {name}: &[(u32, u32)] = &["]
        row: list[str] = []
        for lo, hi in items:
            row.append(f"(0x{lo:x}, 0x{hi:x}),")
            if len(row) == 6:
                lines.append("    " + " ".join(row))
                row = []
        if row:
            lines.append("    " + " ".join(row))
        lines.append("];")
        return lines

    lines = [
        "//! Unicode tables of CPython "
        f"{sys.version.split()[0]} (unicodedata {unicodedata.unidata_version}) for the argparse port:",
        "//! `str.isspace` and the decimal digits (`int()`, the `\\d` of argparse's negative-number",
        "//! test); `repr` uses gcg-data's `isprintable` table (`gcg_data::py_repr`). Generated by",
        "//! `port/oracle/export_cli.py`; do not edit by hand (`--write` regenerates it, `--check`",
        "//! fails when it is stale).",
        "",
        f'/// The `unicodedata.unidata_version` the tables come from.\npub const UNIDATA_VERSION: &str = "{unicodedata.unidata_version}";',
        "",
        *ranges("SPACE", "Inclusive code point ranges where `str.isspace()` is true.", space),
        "",
        "/// `(first, last, value of first)`: inclusive ranges of decimal digits (`str.isdecimal()`,",
        "/// the same set as the `re` class `\\d`) whose values rise by one per code point.",
        "#[rustfmt::skip]",
        "pub const DECIMAL: &[(u32, u32, u8)] = &[",
    ]
    row: list[str] = []
    for lo, hi, value in decimal:
        row.append(f"(0x{lo:x}, 0x{hi:x}, {value}),")
        if len(row) == 5:
            lines.append("    " + " ".join(row))
            row = []
    if row:
        lines.append("    " + " ".join(row))
    lines.append("];")
    return "\n".join(lines) + "\n"


def build_vectors() -> dict[str, object]:
    return {
        "python": sys.version.split()[0],
        "unidata_version": unicodedata.unidata_version,
        "rust_preset_help": RUST_PRESET_HELP,
        "rust_workers_help": RUST_WORKERS_HELP,
        "int": int_vectors(),
        "repr": [{"text": text_json(t), "repr": repr(t)} for t in REPR_TEXTS],
        "close_matches": [
            {
                "word": w,
                "possibilities": p,
                "result": difflib.get_close_matches(w, p, 1),
            }
            for w, p in CLOSE_MATCHES
        ],
        "path": [{"text": t, "str": str(Path(t))} for t in PATH_TEXTS],
        "wrap": [
            {"text": t, "width": w, "lines": textwrap.wrap(t, w)}
            for t in WRAP_TEXTS
            for w in WRAP_WIDTHS
        ],
        "transcripts": transcripts(),
    }


def render_vectors() -> str:
    return json.dumps(build_vectors(), indent=1, ensure_ascii=False) + "\n"


# ---------------------------------------------------------------------------------------------
# command transcripts (cli.py:102-370)

FX = "crates/gcg-cli/tests/fixtures"
RECORDS = "crates/gcg-runner/tests/vectors/records"
SAVED_REPLAY = f"{RECORDS}/random-bo1-s11/replays/win-m0009-g1.json"
D3 = "examples/decks/purple-white-tekkadan.txt"


def _replay_variant(change: Callable[[dict[str, Any]], None]) -> bytes:
    doc = json.loads((ROOT / SAVED_REPLAY).read_text(encoding="utf-8"))
    change(doc)
    return json.dumps(doc).encode("utf-8")


def _drop(key: str) -> Callable[[dict[str, Any]], None]:
    def change(doc: dict[str, Any]) -> None:
        del doc[key]

    return change


def _more_turns(doc: dict[str, Any]) -> None:
    doc["expected"]["turns"] += 5


def _illegal_action(doc: dict[str, Any]) -> None:
    doc["actions"].insert(1, ["attack", 999, 999, -1])


def _extra_action(doc: dict[str, Any]) -> None:
    doc["actions"].append(["pass", -1, -1, -1])


def _bad_kind(doc: dict[str, Any]) -> None:
    doc["actions"].append(["no-such-kind", 0, 0, 0])


def _unknown_card(doc: dict[str, Any]) -> None:
    doc["seats"][0]["main"] = {"GD01-9999": 50}


def fixture_files() -> dict[str, bytes]:
    """The input files of the command transcripts (``tests/cli/test_cli.py:21-25``,
    ``:53-62``, ``:241-271`` build the same ones in a temporary directory). The files that
    are not UTF-8 end in ``.bin``: ``gcg-diff lints`` reads every ``.txt`` and ``.json``
    under ``crates/`` as UTF-8 text."""
    fed = (ROOT / D1).read_text(encoding="utf-8")
    return {
        "short.txt": fed.replace("3 ST01-005 GM", "2 ST01-005 GM").encode("utf-8"),
        "typo.txt": b"4 GD01-008 Guntank\n4 GD01-9999\n",
        "utf16-deck.bin": fed.encode("utf-16"),
        "bom.txt": b"\xef\xbb\xbf" + fed.encode("utf-8"),
        "crlf.txt": fed.replace("\n", "\r\n").encode("utf-8"),
        "truncated-deck.bin": fed.encode("utf-8") + b"\xe2\x82",
        "afile": b"",
        "latin1-replay.bin": '{"caf\xe9": 1}'.encode("latin-1"),
        "bad.json": b"{not json",
        "list.json": b"[1, 2]",
        "other.json": b'{"format": "other"}',
        "bom.json": b"\xef\xbb\xbf{}",
        "no-seed.json": _replay_variant(_drop("seed")),
        "no-seats.json": _replay_variant(_drop("seats")),
        "tampered.json": _replay_variant(_more_turns),
        "illegal-action.json": _replay_variant(_illegal_action),
        "extra-action.json": _replay_variant(_extra_action),
        "bad-kind.json": _replay_variant(_bad_kind),
        "unknown-card.json": _replay_variant(_unknown_card),
    }


# (argv, needs the engine, relation)
COMMAND_CASES: list[tuple[list[str], bool, str]] = [
    (["validate", D1], False, "same"),
    (["validate", D2], False, "same"),
    (["validate", D3], False, "same"),
    (["validate", "./" + D1], False, "same"),
    (["validate", "examples//decks/../decks/red-green-zeon.txt"], False, "same"),
    (["validate", f"{FX}/short.txt"], False, "same"),
    (["validate", f"{FX}/typo.txt"], False, "same"),
    (["validate", f"{FX}/utf16-deck.bin"], False, "same"),
    (["validate", f"{FX}/bom.txt"], False, "same"),
    (["validate", f"{FX}/crlf.txt"], False, "same"),
    (["validate", f"{FX}/truncated-deck.bin"], False, "same"),
    (["validate", "no-such-deck.txt"], False, "same"),
    (["validate", FX], False, "same"),
    (["data", "status"], False, "same"),
    (["data", "status", "--json"], False, "same"),
    (["replay", f"{FX}/latin1-replay.bin"], False, "same"),
    (["replay", f"{FX}/bad.json"], False, "prefix"),
    (["replay", f"{FX}/list.json"], False, "same"),
    (["replay", f"{FX}/other.json"], False, "same"),
    (["replay", f"{FX}/bom.json"], False, "same"),
    (["replay", "none.json"], False, "same"),
    (["replay", FX], False, "same"),
    (["replay", f"{FX}/no-seed.json"], False, "same"),
    (["replay", f"{FX}/no-seats.json"], False, "same"),
    (["benchmark", f"{FX}/short.txt", D2, "--matches", "2", "--out", f"{FX}/never"], False, "same"),
    (["benchmark", D2, f"{FX}/typo.txt", "--matches", "2", "--out", f"{FX}/never"], False, "same"),
    (["benchmark", D2, D1, "--matches", "2", "--out", f"{FX}/afile"], False, "same"),
    (["benchmark", D2, D1, "--matches", "2", "--out", f"{FX}/afile/sub"], False, "same"),
    (["benchmark", "no-such.txt", D1, "--matches", "2"], False, "same"),
    (["replay", SAVED_REPLAY], True, "same"),
    (["replay", "--log", SAVED_REPLAY], True, "same"),
    (["replay", f"{RECORDS}/random-bo3-turn-limit/replays/draw-m0000-g1.json"], True, "same"),
    (["replay", f"{RECORDS}/fast-bo3-log/replays/win-m0000-g2.json"], True, "same"),
    (["replay", f"{FX}/tampered.json"], True, "same"),
    (["replay", f"{FX}/illegal-action.json"], True, "same"),
    (["replay", f"{FX}/extra-action.json"], True, "same"),
    (["replay", f"{FX}/bad-kind.json"], True, "same"),
    (["replay", f"{FX}/unknown-card.json"], True, "same"),
]


def run_command(argv: list[str]) -> dict[str, object]:
    """``cli.main(argv)`` from the repository root with captured streams."""
    out, err = io.StringIO(), io.StringIO()
    cwd = Path.cwd()
    os.chdir(ROOT)
    try:
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            code = cli.main(argv)
    finally:
        os.chdir(cwd)
    return {
        "exit": code,
        "stdout": stream_text(out.getvalue(), "surrogateescape"),
        "stderr": stream_text(err.getvalue(), "backslashreplace"),
    }


def build_commands() -> dict[str, object]:
    from gcg_sim.reports.versions import versions

    v = versions()
    cases = []
    for argv, engine, relation in COMMAND_CASES:
        got = run_command(argv)
        assert not (ROOT / FX / "never").exists(), argv
        cases.append({"argv": argv, "engine": engine, "relation": relation, **got})
    return {
        "python": sys.version.split()[0],
        "package": v["package"],
        "code_digest": v["code_digest"],
        "cases": cases,
    }


def render_commands() -> str:
    """The transcripts, run over the fixture files on disk (``main`` checks them first)."""
    return json.dumps(build_commands(), indent=1, ensure_ascii=False) + "\n"


def main() -> int:
    if len(sys.argv) > 1 and sys.argv[1] == "--child-rust":
        return child_rust(sys.argv[2:])
    ap = argparse.ArgumentParser(description=(__doc__ or "").splitlines()[0])
    mode = ap.add_mutually_exclusive_group(required=True)
    mode.add_argument("--write", action="store_true", help="write the vectors and tables")
    mode.add_argument("--check", action="store_true", help="exit 1 if a file is stale")
    args = ap.parse_args()
    for key in ("FORCE_COLOR", "PYTHON_COLORS"):
        os.environ.pop(key, None)
    fixtures = fixture_files()
    if args.write:
        FIXTURES.mkdir(parents=True, exist_ok=True)
        for name, data in fixtures.items():
            (FIXTURES / name).write_bytes(data)
    stale_fixtures = [
        FIXTURES / name
        for name, data in fixtures.items()
        if not (FIXTURES / name).exists() or (FIXTURES / name).read_bytes() != data
    ]
    if stale_fixtures:
        for path in stale_fixtures:
            print(
                f"{path.relative_to(ROOT)} is stale; run port/oracle/export_cli.py --write",
                file=sys.stderr,
            )
        return 1
    outputs = {
        VECTORS: render_vectors(),
        COMMANDS: render_commands(),
        TABLES: tables_rs(),
    }
    if args.write:
        for path, text in outputs.items():
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text, encoding="utf-8")
        return 0
    stale = [
        p for p, text in outputs.items() if not p.exists() or p.read_text(encoding="utf-8") != text
    ]
    for path in stale:
        print(
            f"{path.relative_to(ROOT)} is stale; run port/oracle/export_cli.py --write",
            file=sys.stderr,
        )
    return 1 if stale else 0


if __name__ == "__main__":
    sys.exit(main())
