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

## Phase 1 — Architecture and plan (done)

- Judge panel (workflow wf_48d5ac0e-8aa): three independent drafts (equivalence-, performance-
  and delivery-first), two judges (code spot-checks; brief requirement walk), the delivery draft
  won (9/9/9/9 and 9/8/8/9); synthesis into `docs/port/PLAN.md`, then two adversarial checkers
  found 46 gaps (1 blocker, 16 major), all fixed (review log at the end of PLAN.md).
- Lead review: accepted with the decisions in PLAN.md §0.1 (dependency fetch approved under
  the brief's own requirements; W-120 deferred to the user; §12.11 answer (a); W-122 bytecode
  kept).
- Key decisions: JSON IR exported by `port/oracle/export_ir.py` and embedded; 11 crates with the
  reference search isolated in `gcg-ai-reference` (unreachable from `gcg-sim`); Copy-record
  arenas for `GameState`; Python's derived-view cache policy mirrored exactly; thread pool with
  atomic match claiming; oracle and differential runner (`gcg-diff`) before engine code; engine
  in five layers along Python's import DAG; `gcg-search` forked from the reference with a
  null-config bit-identity test; seed role ranges and a ledger for tuning/evaluation.

**Gate.** `python3 -c "...tests.json keys..."`: 158 entries, valid, 1 passing (T-016 gcg-core).
`uv run python scripts/check_doc_commands.py --list docs/port/PLAN.md`: 0 blocks. Risks: the
engine critical path is sequential (L1→L5); the R1 numbering hazard (effect_key first-use
numbering in once-keys could make Python's own root merges depend on worker history) is probed
in Phase 2.

## Phase 2 — Oracle harness (in progress)

### M2a — contracts (done)

- Workflow wf_8637c6bd-422: three sequential contract agents (foundations/data/effects; engine;
  AI/runner/eval/diff) and three adversarial review rounds (13 → 2 → 1 findings, all fixed; the
  last one, `option_scores` taking no derived view, is also reflected in PLAN §10.3.5).
- 11 crates with the PLAN §2.2 dependency edges; `contracts.rs` per crate (≈5,900 lines of
  types, traits and preset/weight constants); `crates/gcg-effects/src/dsl.rs` (146 node structs,
  14 enums) generated by `port/oracle/gen_dsl.py`; `crates/gcg-engine/src/enums.rs` generated by
  `port/oracle/gen_engine_enums.py`; frozen signatures in `docs/port/contracts.md` (1,469 lines,
  owners and Python `path:line`); `clippy.toml` (float/sort/clock/hash-map bans) and
  `.cargo/config.toml` (target-cpu=native; `dist` profile notes for apple-m2 and M6).
- Dependencies fetched (PLAN §0.1): signal-hook 0.4.6, mimalloc 0.1.52 (optional), zstd 0.14.3
  (optional), plus the cached set.

**Gate.** `cargo fmt --all --check` clean; `cargo clippy --workspace --all-targets
--all-features -- -D warnings` clean; `cargo test --workspace`: 66 passed, 0 failed;
`gen_dsl.py --check`, `gen_engine_enums.py --check`, `export_vectors.py --check` current;
`uv run ruff check .` and `ruff format --check .` clean; `MYPYPATH=src uv run mypy --strict
port/oracle` clean; `git diff --quiet d9d41af -- src/gcg_sim tests` (reference untouched); no
`todo!`/`unimplemented!`/TODO/FIXME/`std::time` in `crates/`; `cargo tree -p gcg-cli` contains
neither `gcg-ai-reference` nor `gcg-diff` (agent-reported, and enforced by
`crates/gcg-cli/tests/no_reference.rs`, which passes).

### M3 — differential oracle and trace corpus (done)

- Wave 1 G part 1 (traces.py, coverage_decks.py, census.py, offset hook, gcg-diff reader,
  MockEngine, negative controls, lints, verify scripts) and G part 2 (pytest_trace.py per-test
  traces, export_tags.py, W-160 normaliser and comparison-script hardening) squash-merged.
- Integration agent: reader accepts the per-test format additions, agent `log` decoded,
  per-test override fragments emitted in B1's fragment format (FragmentBuilder over export_ir's
  encoders, byte-equal to build_fragment where it applies), `apply.pending` = decision before
  the apply. Tier-1 budget raised to 45 MiB (lead decision; per-test set is 22 MB).
- Lead: lint rule 6 narrowed to the three W-122 directories (a bare `__pycache__` is the
  reference's own code_digest skip-name), negative control moved to a `tests/perf` path, and an
  allowed-fixture line added.

**Gate.** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`
clean; `cargo test --workspace --release`: 203 passed, 0 failed; `gcg-diff corpus --tier 1
--parse-only --require-all` and `--mock --roundtrip`: 3804/3804 (baselines 8, coverage 50,
full 4, fuzz 100, search 9, tests 3213, vanilla 40, views 280, worlds 100); `gcg-diff
selftest`: 22 negative controls fired; tier 2 (build/oracle/tier2) parse-only 4154/4154;
`traces.py --check --tier 1` under PYTHONHASHSEED 1 and 2 and `pytest_trace.py --check`
(agent-run after the final regeneration; nothing since touched the corpus or generators);
`census.py --check`, `export_ir.py --check` current; ruff, ruff format, mypy --strict clean;
`git diff --quiet d9d41af -- src/gcg_sim tests`. Tier 1 is 41 MB on disk (42,985,138 bytes).
Open: O14 (`Scenario::prepare_start` so the real engine can answer `before16`); the engine-side
replay, `explain`, `trace-search`, fuzz and traceability land with the engine (wave 2b).
