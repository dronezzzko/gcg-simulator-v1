"""Run every shell command block in the project documentation (acceptance criterion 10).

Every fenced ```bash block in README.md, CLAUDE.md, docs/**/*.md and the gcg-benchmark skill is
run with ``bash -euo pipefail`` from the repository root. Markers on the line before a block:

- ``<!-- doc-check: skip <reason> -->``: the block is listed with its reason and never run; a
  skip without a reason is an error.
- ``<!-- doc-check: network <reason> -->``: the block needs the network. It is listed and not
  run, except with ``--network`` (the network-enabled job), which runs it like any other block.
- ``<!-- doc-check: rust -->`` before a ```rust block: a Rust example. It is compiled and run as
  a program (wrapped in ``fn main`` unless it has one) in a scratch Cargo project that depends on
  the workspace crates it names (``gcg_core::`` and so on), built ``--offline`` into
  ``target/doc-check``. A ```rust block without the marker is a sketch and is not run.

Each block runs in its own process group (docs/port/WEAKNESSES.md W-114). When the block
exits, the group is killed, so background jobs a block started cannot outlive it; at the
timeout the whole group is killed and the block fails. Output goes to a file, not a pipe, so a
background child holding the output open cannot stall the checker.

    uv run python scripts/check_doc_commands.py [--list] [--network] [--timeout SECONDS] [files...]
    uv run python scripts/check_doc_commands.py --selftest
"""

from __future__ import annotations

import argparse
import contextlib
import os
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import time
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_FILES = (
    "README.md",
    "CLAUDE.md",
    "docs/**/*.md",
    ".claude/skills/gcg-benchmark/**/*.md",
)
BLOCK = re.compile(
    r"(?:<!--\s*doc-check:\s*(?P<marker>skip|network|rust)(?P<reason>[^>]*?)-->\s*\n)?"
    r"^[ \t]*```(?P<lang>bash|sh|shell|rust)[^\n]*\n(?P<body>.*?)^[ \t]*```",
    re.M | re.S,
)
CRATE = re.compile(r"\bgcg_(core|data|effects|engine|ai|search|runner)\b")
KILL_GRACE = 5.0


@dataclass(frozen=True)
class Block:
    path: Path
    line: int
    body: str
    lang: str
    marker: str | None
    reason: str


@dataclass(frozen=True)
class Outcome:
    ok: bool
    detail: list[str]


def blocks(path: Path) -> list[Block]:
    """The shell blocks and the marked Rust examples of ``path``; unmarked Rust sketches are
    left out."""
    text = path.read_text(encoding="utf-8")
    out = []
    for m in BLOCK.finditer(text):
        lang = "rust" if m.group("lang") == "rust" else "shell"
        marker = m.group("marker")
        if lang == "rust" and marker != "rust":
            continue
        if lang == "shell" and marker == "rust":
            marker = None
        out.append(
            Block(
                path=path,
                line=text.count("\n", 0, m.start("body")),
                body=m.group("body"),
                lang=lang,
                marker=marker,
                reason=(m.group("reason") or "").strip(),
            )
        )
    return out


def doc_files(patterns: list[str], root: Path = ROOT) -> list[Path]:
    files: set[Path] = set()
    for pattern in patterns:
        files.update(p for p in root.glob(pattern) if p.is_file())
    return sorted(files)


def _kill_group(pgid: int) -> None:
    with contextlib.suppress(ProcessLookupError):
        os.killpg(pgid, signal.SIGKILL)


def run_group(
    argv: list[str], cwd: Path, timeout: float, env: dict[str, str] | None = None
) -> Outcome:
    """Runs ``argv`` in a new process group; kills the group at the timeout and on exit."""
    with tempfile.TemporaryFile(mode="w+", encoding="utf-8") as log:
        proc = subprocess.Popen(
            argv, cwd=cwd, stdout=log, stderr=subprocess.STDOUT, start_new_session=True, env=env
        )
        try:
            code = proc.wait(timeout=timeout)
            timed_out = False
        except subprocess.TimeoutExpired:
            timed_out = True
            code = None
        _kill_group(proc.pid)
        if timed_out:
            proc.wait(timeout=KILL_GRACE)
        log.seek(0)
        detail = log.read().strip().splitlines()[-15:]
    if timed_out:
        return Outcome(False, [*detail, f"timed out after {timeout:.0f}s; process group killed"])
    return Outcome(code == 0, detail)


def _rust_project(block: Block, root: Path, scratch: Path) -> Path:
    crates = sorted(set(CRATE.findall(block.body)))
    deps = "".join(
        f'gcg-{c} = {{ path = "{(root / "crates" / f"gcg-{c}").as_posix()}" }}\n' for c in crates
    )
    (scratch / "src").mkdir(parents=True)
    (scratch / "Cargo.toml").write_text(
        '[package]\nname = "doc-example"\nversion = "0.0.0"\nedition = "2024"\npublish = false\n\n'
        f"[dependencies]\n{deps}\n[workspace]\n",
        encoding="utf-8",
    )
    lock = root / "Cargo.lock"
    if lock.is_file():
        shutil.copy(lock, scratch / "Cargo.lock")
    body = (
        block.body
        if re.search(r"\bfn\s+main\s*\(", block.body)
        else ("fn main() {\n" + block.body + "}\n")
    )
    (scratch / "src" / "main.rs").write_text(body, encoding="utf-8")
    return scratch


def run_block(block: Block, root: Path, timeout: float) -> Outcome:
    if block.lang == "shell":
        return run_group(["bash", "-euo", "pipefail", "-c", block.body], root, timeout)
    with tempfile.TemporaryDirectory(prefix="doc-check-rust-") as tmp:
        project = _rust_project(block, root, Path(tmp) / "example")
        env = dict(os.environ, CARGO_TARGET_DIR=str(root / "target" / "doc-check"))
        argv = ["cargo", "run", "--offline", "--quiet", "--manifest-path", "Cargo.toml"]
        return run_group(argv, project, timeout, env)


def check(
    files: list[Path], root: Path, *, timeout: float, network: bool, list_only: bool
) -> tuple[int, int, list[str]]:
    """Runs (or lists) every block: ``(ran, skipped, failures)``."""
    failures: list[str] = []
    ran = skipped = 0
    for path in files:
        for b in blocks(path):
            where = f"{path.relative_to(root)}:{b.line}"
            if b.marker in ("skip", "network") and not b.reason:
                failures.append(f"{where}: doc-check {b.marker} without a reason")
            if b.marker == "skip" or (b.marker == "network" and not network):
                print(f"{b.marker.upper()} {where}: {b.reason}")
                skipped += 1
                continue
            if list_only:
                print(f"---- {where} ({b.lang})\n{b.body}")
                continue
            start = time.monotonic()
            res = run_block(b, root, timeout)
            elapsed = time.monotonic() - start
            ran += 1
            print(f"{'PASS' if res.ok else 'FAIL'} {where} ({b.lang}, {elapsed:.1f}s)")
            if not res.ok:
                failures.append(where)
                print("\n".join("    " + line for line in res.detail))
    return ran, skipped, failures


SELFTEST_DOC = """# doc-check self-test

```bash
echo plain
```

<!-- doc-check: skip a reason -->
```bash
exit 1
```

<!-- doc-check: network needs the network -->
```bash
echo "network block ran"
```

```rust
this is a sketch and never compiles
```

<!-- doc-check: rust -->
```rust
let v: Vec<u32> = (1..=4).collect();
assert_eq!(v.iter().sum::<u32>(), 10);
println!("rust example ok");
```
"""


def _alive(pid: int) -> bool:
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    return True


def _wait_dead(pid: int) -> bool:
    for _ in range(50):
        if not _alive(pid):
            return True
        time.sleep(0.1)
    return False


def selftest() -> int:
    """The W-114 checks: markers, Rust examples, and process groups killed at exit and timeout."""
    problems: list[str] = []
    with tempfile.TemporaryDirectory(prefix="doc-check-selftest-") as tmp:
        root = Path(tmp)
        doc = root / "doc.md"
        doc.write_text(SELFTEST_DOC, encoding="utf-8")
        found = [(b.lang, b.marker) for b in blocks(doc)]
        if found != [("shell", None), ("shell", "skip"), ("shell", "network"), ("rust", "rust")]:
            problems.append(f"blocks found: {found}")
        ran, skipped, fails = check([doc], root, timeout=600, network=False, list_only=False)
        if (ran, skipped, fails) != (2, 2, []):
            problems.append(f"offline run: ran {ran}, skipped {skipped}, failures {fails}")
        ran, skipped, fails = check([doc], root, timeout=600, network=True, list_only=False)
        if (ran, skipped, fails) != (3, 1, []):
            problems.append(f"--network run: ran {ran}, skipped {skipped}, failures {fails}")
        bad = root / "bad.md"
        bad.write_text("<!-- doc-check: rust -->\n```rust\nassert!(1 + 1 == 3);\n```\n", "utf-8")
        if check([bad], root, timeout=600, network=False, list_only=False)[2] != ["bad.md:2"]:
            problems.append("a failing Rust example passed")
        pidfile = root / "bg.pid"
        bg = Block(doc, 0, f"sleep 600 & echo $! > {pidfile}\necho started\n", "shell", None, "")
        res = run_block(bg, root, 30)
        pid = int(pidfile.read_text().strip())
        if not res.ok or not _wait_dead(pid):
            problems.append(f"a background job outlived its block (pid {pid}, {res})")
        hang = Block(doc, 0, f"sleep 600 & echo $! > {pidfile}\nwait\n", "shell", None, "")
        start = time.monotonic()
        res = run_block(hang, root, 2)
        pid = int(pidfile.read_text().strip())
        if res.ok or "process group killed" not in res.detail[-1] or not _wait_dead(pid):
            problems.append(f"the timeout did not kill the group (pid {pid}, {res})")
        if time.monotonic() - start > 2 + KILL_GRACE:
            problems.append("the timed-out block was not stopped at its timeout")
    for p in problems:
        print(f"selftest: {p}", file=sys.stderr)
    print("check_doc_commands selftest " + ("failed" if problems else "passed"))
    return 1 if problems else 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=(__doc__ or "").split("\n")[0])
    parser.add_argument("files", nargs="*", help="glob patterns relative to the repository")
    parser.add_argument("--list", action="store_true", help="list the blocks without running")
    parser.add_argument("--network", action="store_true", help="also run network blocks")
    parser.add_argument("--timeout", type=float, default=1800.0, help="seconds per block")
    parser.add_argument("--selftest", action="store_true", help="run the checker's own checks")
    args = parser.parse_args(argv)
    if args.selftest:
        return selftest()
    files = doc_files(args.files or list(DEFAULT_FILES))
    ran, skipped, failures = check(
        files, ROOT, timeout=args.timeout, network=args.network, list_only=args.list
    )
    for f in failures:
        if " without a reason" in f:
            print(f)
    print(f"{ran} blocks run, {skipped} skipped, {len(failures)} failed")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
