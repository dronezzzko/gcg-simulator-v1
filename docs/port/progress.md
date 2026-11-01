# Rust port — progress log

Phase gates for the Rust port of gcg-sim (branch `rust-port`). Each phase records what was
verified (commands and outcomes), what remains, and risks to the acceptance criteria.
Machine-readable test status: `docs/port/tests.json`.

## Environment

| Item | Value |
| --- | --- |
| Machine | Apple M2 Pro, 10 cores (6 performance + 4 efficiency), 32 GB, cache line 128 B |
| OS | macOS 27.2 (26B123) |
| Python reference | Python 3.15 via uv 0.13.6 (`uv sync --locked`) |
| Rust | rustc 1.99.1 (58cf8f9c3 2026-10-13), LLVM 23.1.1, stable, rustup minimal + clippy + rustfmt |
| Tools | hyperfine, samply (Homebrew); xctrace (Xcode) |

## Phase 0 — Discovery and baseline (in progress)

- Branch `rust-port` created from `main` at d9d41af.
- Rust toolchain, hyperfine and samply installed.
- `uv sync --locked` OK. `uv run pytest -n 8 -q`: **3985 passed**, 0 failed, 0 skipped, in
  1542.6 s wall (13611.7 s user). Slow tests are not deselected by default, so this count
  includes `tests/slow/` (AI strength vs random/greedy, robustness). Wall time is not a
  baseline: discovery agents ran concurrently.
- `crates/gcg-core` (SplitMix64, `derive_seed`, Python `repr`/float formatting/`round`,
  CPython 3.12+ Neumaier `sum()`, `json.dumps`-compatible writer, floor division, first-wins
  max/min) verified against vectors exported from Python 3.15 by
  `port/oracle/export_vectors.py`: `cargo test -p gcg-core` 17 passed (10 unit + 7 vector
  tests over 3,021 reprs, 4,263 fixed-format/round cases incl. ties, 301 sums, 4 dumps option
  sets, 11 derive_seed paths, RNG streams).
- Python timing baseline measured with `scripts/compare_python_vs_rust.sh` (hyperfine 2.0.0):
  best worker count 10 (6/8/10 workers: 255.7/239.0/221.0 s for 20 matches); standard m100 w10
  median 972.0 s (251 games), standard m20 w1 1269.5 s, strong m20 w10 356.0 s
  (docs/port/BENCHMARKS.md).
- Profile (inventory/11): engine apply ~80%, view.derived ~33%, legal actions ~21-24%, clone
  3-4%; one standard BO1 game 13.3 s.
- docs/port/INVENTORY.md + 12 sections (11,000+ lines), docs/port/WEAKNESSES.md (164 items from
  389 raw sweep items, two critic rounds), docs/port/BRIEF.md (verbatim specification).
- W-154 handled before the first commit: ruff clean on `port/` and `scripts/`, the two
  inventory shell blocks carry doc-check skip reasons
  (`uv run python scripts/check_doc_commands.py docs/port/**/*.md`: 0 run, 2 skipped, 0 failed).

**Gate.** Verified: Python suite green (3985), gcg-core vector tests green (18), baseline timings
recorded, inventory and weaknesses complete with sources. Remaining risks: byte equivalence of
the reference MCTS (float order, libm `pow`/`erfc`, CPython repr content ids) — mitigated by
gcg-core parity helpers and the IR carrying content ids; the derived-view cache and `version`
counter must be mirrored exactly because both reach `final_state_sha256`.
