"""The installed ``gcg-sim`` console script: determinism across workers and processes,
replays in a fresh process, and Ctrl-C handling."""

from __future__ import annotations

import signal
import subprocess
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
EXAMPLES = ROOT / "examples" / "decks"
FEDERATION = EXAMPLES / "blue-white-federation.txt"
TEKKADAN = EXAMPLES / "purple-white-tekkadan.txt"
DETERMINISTIC_FILES = ("results.json", "games.ndjson", "summary.md")


def run_script(
    script: Path, args: list[str], env: dict[str, str], timeout: float = 300
) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [str(script), *args], env=env, capture_output=True, text=True, timeout=timeout, check=False
    )


def benchmark(
    script: Path, env: dict[str, str], out: Path, *extra: str
) -> subprocess.CompletedProcess[str]:
    args = ["benchmark", str(TEKKADAN), str(FEDERATION), "--matches", "12", "--seed", "77"]
    return run_script(script, [*args, "--out", str(out), *extra], env)


def test_help_and_validate(console_script: Path, script_env: dict[str, str]) -> None:
    helped = run_script(console_script, ["--help"], script_env)
    assert helped.returncode == 0
    assert "benchmark" in helped.stdout
    validated = run_script(console_script, ["validate", str(FEDERATION)], script_env)
    assert validated.returncode == 0
    assert validated.stdout.strip().endswith("legal (50 cards, 10 resources; Blue/White)")
    missing = run_script(console_script, ["validate", "no-such-deck.txt"], script_env)
    assert missing.returncode == 1
    assert missing.stderr.startswith("error: cannot read deck file no-such-deck.txt")


def test_results_are_byte_identical_for_one_and_two_workers(
    console_script: Path, script_env: dict[str, str], tmp_path: Path
) -> None:
    one = benchmark(console_script, script_env, tmp_path / "w1", "--workers", "1")
    two = benchmark(console_script, script_env, tmp_path / "w2", "--workers", "2")
    assert one.returncode == 0, one.stderr
    assert two.returncode == 0, two.stderr
    for name in DETERMINISTIC_FILES:
        assert (tmp_path / "w1" / name).read_bytes() == (tmp_path / "w2" / name).read_bytes()
    replays_one = sorted(p.name for p in (tmp_path / "w1" / "replays").iterdir())
    assert replays_one == sorted(p.name for p in (tmp_path / "w2" / "replays").iterdir())
    for name in replays_one:
        a = (tmp_path / "w1" / "replays" / name).read_bytes()
        assert a == (tmp_path / "w2" / "replays" / name).read_bytes()
    assert (tmp_path / "w1" / "timing.json").read_bytes() != (
        tmp_path / "w2" / "timing.json"
    ).read_bytes()


def test_results_do_not_depend_on_the_python_hash_seed(
    console_script: Path, script_env: dict[str, str], tmp_path: Path
) -> None:
    for seed in ("1", "2"):
        env = {**script_env, "PYTHONHASHSEED": seed}
        done = benchmark(console_script, env, tmp_path / seed, "--workers", "2")
        assert done.returncode == 0, done.stderr
    for name in DETERMINISTIC_FILES:
        assert (tmp_path / "1" / name).read_bytes() == (tmp_path / "2" / name).read_bytes()


def test_saved_replays_verify_in_a_fresh_process(
    console_script: Path, script_env: dict[str, str], tmp_path: Path
) -> None:
    done = benchmark(console_script, script_env, tmp_path / "out", "--workers", "3")
    assert done.returncode == 0, done.stderr
    replays = sorted((tmp_path / "out" / "replays").glob("*.json"))
    assert replays
    for path in replays:
        checked = run_script(console_script, ["replay", str(path)], script_env)
        assert checked.returncode == 0, checked.stderr
        assert checked.stdout.splitlines()[-1] == (
            "replay matches the recorded outcome and final state"
        )


def test_ctrl_c_stops_workers_and_writes_nothing(
    console_script: Path, script_env: dict[str, str], tmp_path: Path
) -> None:
    out = tmp_path / "interrupted"
    args = ["benchmark", str(TEKKADAN), str(FEDERATION), "--matches", "100000", "--workers", "2"]
    proc = subprocess.Popen(
        [str(console_script), *args, "--out", str(out)],
        env=script_env,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    assert proc.stderr is not None
    first = proc.stderr.readline()
    assert first.startswith("matches 1/100000"), first
    started = time.monotonic()
    proc.send_signal(signal.SIGINT)
    stdout, stderr = proc.communicate(timeout=60)
    assert time.monotonic() - started < 30
    assert proc.returncode == 130
    assert "interrupted: workers stopped, no reports written" in stderr
    assert stdout == ""
    assert not (out / "results.json").exists()
