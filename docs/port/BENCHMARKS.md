# Benchmarks: Python reference vs Rust port

Wall-clock comparison of the two implementations. This is the project's only timing
activity, and it lives outside the Rust crates (`scripts/compare_python_vs_rust.sh`). The
Rust codebase contains no benchmark suite, timing test or timing-dependent logic.

## Protocol

- **Workload.** `benchmark examples/decks/red-green-zeon.txt examples/decks/blue-white-federation.txt
  --format bo3 --seed 1 --out <dir>`, with `--ai-preset`, `--matches` and `--workers` set per
  configuration.
- **Runner.** hyperfine 2.0.0, with a fresh `--out` directory per run (`--prepare 'rm -rf …'`)
  and no other load. Long configurations get 3 runs, short ones 5. Each configuration reports
  the median, min and max wall time. The machine runs at full performance; no warm-up, power
  or thermal controls are used.
- **Implementations.**
  - Python: `.venv/bin/gcg-sim` after `uv sync --locked`.
  - Rust, part 1: `target/release/gcg-eval reference-benchmark --reference-preset …`, which
    is the reference search at the reference budgets.
  - Rust, part 2: `target/release/gcg-sim benchmark --ai-preset fast|standard`, which is the
    shipped presets.
- **Pairs.**
  - Screening: Python `standard` against Rust reference `standard` (part 1) and Rust `fast`
    (part 2).
  - Full search: Python `strong` against Rust reference `strong` (part 1) and Rust `standard`
    (part 2).
- **Configurations.**
  - Headline (screening pairs): 100 matches, at each implementation's best worker count and
    at 10 workers.
  - Single-thread (screening pairs): 20 matches, 1 worker.
  - Full search: 20 matches at the best worker count.
- **Commands.**
  - `scripts/compare_python_vs_rust.sh best-workers python|rust-reference|rust-presets`
  - `scripts/compare_python_vs_rust.sh python|part1|part2 --best N`
  - `scripts/compare_python_vs_rust.sh report`
- **Output.** Raw hyperfine JSON and the report directories go to `build/comparison/`, which is
  not committed. The tables below copy them.

## Machine

| Item | Value |
| --- | --- |
| CPU | Apple M2 Pro, 10 cores: 6 performance + 4 efficiency (cache line 128 B) |
| Memory | 32 GB (34,359,738,368 bytes) |
| OS | macOS 27.2 (26B123) |
| Python | 3.15.1 (uv 0.13.6) |
| Rust | rustc 1.99.1 (58cf8f9c3 2026-10-13), LLVM 23.1.1 |
| hyperfine | 2.0.0 |

## Python baseline (measured 2026-11-01, commit d9d41af)

The Python best worker count was found with one run each of `--ai-preset standard --matches 20`.
The median of one run is that run's time.

| Workers | Wall time |
| ---: | ---: |
| 6 | 255.7 s |
| 8 | 239.0 s |
| **10** | **221.0 s** |

The best worker count is 10, so the "best" and "10 workers" headline configurations coincide
for Python.

| Configuration | Command flags | Runs | Median | Min | Max | Games |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| Headline, screening | `--ai-preset standard --matches 100 --workers 10` | 3 | 972.0 s | 971.4 s | 981.5 s | 251 |
| Single-thread, screening | `--ai-preset standard --matches 20 --workers 1` | 3 | 1269.5 s | 1262.3 s | 1292.2 s | 50 |
| Full search | `--ai-preset strong --matches 20 --workers 10` | 3 | 356.0 s | 355.0 s | 356.1 s | 54 |

The 100-match headline played 251 games, the same count docs/AI.md reports for this
workload. The match result was 52 DUT wins out of 100, and 124 of 251 games. A read-only planning
workflow, made of LLM calls and file reads, ran during these measurements. No other local
computation ran.

## Python profile (where the time goes)

Full method, numbers and scripts: [inventory/11-python-profile.md](inventory/11-python-profile.md).
One reference `standard` BO1 game (seed 1, red-green-zeon in seat 0 against
blue-white-federation) takes 13.3 s unprofiled (hyperfine 13.336 ± 0.032 s). It runs 15,116
MCTS iterations, 140,925 `apply` calls, 8,184 determinizations and 23,326 clones.

| Share of game time (cProfile / sampler / wrappers) | |
| --- | --- |
| engine `apply`/`advance` (inclusive) | 80.7 / 80.9 / 79.8% |
| `view.derived` (1.7M calls, 18.6% cache misses) | 33.1 / 34.7 / 32.0% |
| legal-action generation (mostly main-phase options) | about 21–24% |
| playout policy | about 6% |
| `determinize` (including its clone) | 3.9–5.5% |
| `GameState.clone` (17.4 µs on a mid-game state) | 3.2–3.7% |
| evaluation | 1.3–1.7% |
| MCTS bookkeeping (exclusive) | 2.3–2.7% |

The engine dominates, so a compiled engine with a compact state (clone close to a memcpy)
and a cheaper derived view is where the speedup comes from.

## Part 1: equal-work speedup (reference search)

Not measured yet (Phase 5).

## Part 2: shipped presets

Not measured yet (Phase 6).
