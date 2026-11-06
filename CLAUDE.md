# gcg-sim

A deterministic Gundam Card Game (Comprehensive Rules Ver. 1.10.0) engine, AI and deck benchmark
CLI, implemented in Rust (`crates/`, binaries `gcg-sim` and `gcg-eval`). The Python package
(`src/gcg_sim`) is the reference oracle and the effect-IR generator. Start with `README.md`.
Architecture: `docs/dev/ARCHITECTURE.md` (Rust) and `docs/ARCHITECTURE.md` (the oracle). AI
and presets: `docs/AI.md`. Design and registers: `docs/dev/PLAN.md`,
`docs/dev/WEAKNESSES.md`, `docs/dev/tests.json`. `docs/SPEC.md`, `docs/INTERFACES.md`,
`docs/PROGRESS.md` and `docs/status.json` are the oracle's build records, not current
documentation. The Rust build records (brief, inventory, final report, review, progress log) were
deleted, and so were the one-off evaluation scripts under `docs/port/eval/` (all but
`P6-F/synthetic_cards.py`); citations of them in the registers and eval records (`BRIEF.md:131`,
`Inv 06b §5`, `REVIEW.md §4`, `docs/port/eval/M15/rows_table.py`) refer to commit 83a06c8.

## Rust commands

Rust 1.99 stable (`export PATH=$HOME/.cargo/bin:$PATH` if `cargo` is not found). Builds are
`--locked --offline` once `cargo fetch --locked` has filled the crate cache.

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
cargo build --release --locked --offline -p gcg-cli -p gcg-eval
cargo build --release --locked --offline -p gcg-diff
target/release/gcg-diff lints
target/release/gcg-diff traceability --check
target/release/gcg-diff tests-json --check
target/release/gcg-diff weaknesses --check docs/dev/WEAKNESSES.md
target/release/gcg-diff coverage --check
```

<!-- doc-check: skip the full test suite, the differential corpus and the two gates take long; scripts/verify_all_rust.sh and scripts/ci_local.sh run them -->
```bash
cargo test --workspace --release --locked --offline
target/release/gcg-diff corpus --tier 1 --require-all
scripts/ci_local.sh
scripts/verify_all_rust.sh
```

- Long tests are `required-features = ["slow"]` targets, never `#[ignore]`. The offline slow
  tier is `cargo test --workspace --release --locked --offline --features
  gcg-diff/slow,gcg-eval/slow,gcg-search/slow`; `gcg-engine/slow` needs the regenerated tier-2
  corpus. `scripts/cargo_test_features.py` derives these lists from `cargo metadata`, and
  `scripts/verify_all_rust.sh` uses them.
- `scripts/verify_all_rust.sh` is the full Rust gate on a clean clone of HEAD (`--full` adds
  the Python-side checks and the tier-2 corpus). `scripts/ci_local.sh` runs the steps of
  `.github/workflows/rust.yml` in the working tree; `GCG_GATE_WORKERS=N` caps its corpus jobs.
- `gcg-diff`: `corpus` replays the Python traces through the Rust engine and stops at the first
  divergence; `explain <trace> <decision>` and `trace-search` drill down; `fuzz` plays seeded
  games with invariants; `selftest` runs the negative controls; `schema-check` validates
  `results.json` files; `api-snapshot --check` pins the public API. `gcg-diff --help` lists all.
- `gcg-eval` is the only route to the reference search (`reference-benchmark
  --reference-preset fast|standard|strong`, agent specs `ref:<preset>`); `head-to-head`,
  `ablation`, `budget-curve`, `gates` and the other tools measure strength. Runs that count as
  evidence take `--role tuning|dev|confirm|eval|review` (the seed must lie in the role's range
  and is recorded in `docs/dev/eval/seed-ledger.json`); never use a held-out pair or an eval or
  review seed for tuning.
- After changing rule, card, FAQ or ruling tags in the Python tests, regenerate the tag export
  and the per-test traces, then the traceability documents with the Rust generator. Do not use
  `python -m gcg_sim.tools.traceability` for `docs/RULES_TRACEABILITY.md`: it drops the Rust
  column.

  <!-- doc-check: skip regenerates the per-test traces (the rule and card suites in one process) and rewrites committed corpus and traceability files -->
  ```bash
  uv run python oracle/export_tags.py
  uv run python oracle/pytest_trace.py --write
  uv run python oracle/traces.py --write --tier 1
  target/release/gcg-diff traceability --write
  ```

## The Python oracle

The Python package stays unchanged as the oracle: never edit `src/gcg_sim/` or `tests/`
(every gate checks `git diff --quiet d9d41af -- src/gcg_sim tests`). Its effect toolchain
(`effects/`, `cards/`, `data/`) is also the generator of the effect IR that the Rust binaries
embed. Tooling outside the package lives in `oracle/` (the oracle exporters) and
`scripts/` (the gates and helpers).

```bash
uv sync --locked
uv run ruff check . && uv run ruff format --check . && uv run mypy
uv run python -m gcg_sim.tools.explain GD01-001
uv run python -m gcg_sim.tools.golden --check
uv run python -m gcg_sim.sources.conflicts --check
uv run python -m gcg_sim.tools.sources --check
uv run python oracle/export_ir.py --check
```

<!-- doc-check: skip the full Python suite and a 500-game fuzz run; scripts/verify_all.sh runs both -->
```bash
uv run pytest -n 8
uv run python -m gcg_sim.tools.fuzz --games 500 --workers 8
```

- `explain` shows a card's text, compiled script, rulings and applied resolutions.
- `scripts/verify_all.sh` runs the full Python gate on a clean clone.
- **Regenerate the oracle artefacts** after a change to the Python package's effects, cards or
  data (a data refresh), a Python version bump or a trace-format change, never by hand. Each
  script has `--check`; regenerate only what `--check` reports stale, review the diff, and
  commit each regeneration on its own:
  - effect IR: `uv run python oracle/export_ir.py --write` (the Rust test
    `crates/gcg-effects/src/tests/ir_current.rs` fails until it is current);
  - generated Rust: `oracle/gen_dsl.py --write`, `oracle/gen_engine_enums.py --write`;
  - test vectors: `oracle/export_<name>.py` without `--check` (`export_cli.py`,
    `p6k_vectors.py` and `p6k_audit.py` take `--write`), `oracle/traces.py vectors`;
  - trace corpus: `oracle/pytest_trace.py --write`, then `oracle/traces.py --write
    --tier 1` (both hash seeds; it rewrites `oracle/corpus/v1/MANIFEST.json`), then
    `oracle/census.py --write`; check with `target/release/gcg-diff corpus --tier 1
    --require-all` and `target/release/gcg-diff coverage --check`.

## Rules of the codebase

- **Determinism.** All randomness goes through `SplitMix64` and `derive_seed`
  (`gcg_core::rng`; `gcg_sim.rng` in Python). No wall-clock, sleep, timeout, thread id, worker
  count or `hash()` of strings in game or AI logic, no `HashMap`/`HashSet` (use
  `gcg_core::lookup::LookupMap`), stable sorts only; `clippy.toml` and `gcg-diff lints` enforce
  this. Same seed means byte-identical outputs for any `--workers`. Budgets are node counts.
- **No timing code in the crates.** No benchmark suites, timing tests or timing-dependent
  behaviour; `timing.json` exists for CLI parity only. The one timing activity is
  `scripts/compare_python_vs_rust.sh`, outside the crates (`docs/dev/BENCHMARKS.md`).
- **The reference search is frozen.** `gcg-ai-reference` reproduces Python's decisions byte for
  byte and is reachable only from `gcg-eval`; `gcg-cli` must never depend on it. Strength work
  goes into `gcg-search` behind a `MechanismFlags` field, with its null configuration still
  replaying the reference, and is adopted into the presets only with a measured non-negative
  ablation row. Rules, card behaviour and data never change for strength.
- **`gcg-sim`'s command line stays Python's**: only `--ai-preset standard|fast` and `--workers
  auto` differ. New options go into `gcg-eval` subcommands.
- **Cached sources are read-only.** Never hand-edit `src/gcg_sim/data/gcgapi/*`,
  `data/official_raw/*` or the Comprehensive Rules markdown
  (`src/gcg_sim/data/rules/gundam-card-game-comprehensive-rules.md` and its repo-root copy);
  only `/gcg-refresh-data` updates them (whole files, and the markdown from the official PDF).
  Derived files are curated by hand: `src/gcg_sim/data/official/*.json` (with verbatim quotes)
  and `src/gcg_sim/data/rules/rules_na.json` (N/A reasons; then regenerate `rules_index.json`
  with `python -m gcg_sim.rules.index --write`). Re-hash with
  `python -m gcg_sim.tools.sources --write` after changing any of them. Resolve source
  disagreements in `src/gcg_sim/data/overrides.json` (plus `curated_conflicts.json` with
  verified quotes), then regenerate `docs/CONFLICTS.md` with `python -m gcg_sim.sources.conflicts`.
  These are package files, so they change only in a data refresh, followed by the oracle
  regeneration above.
- **Card effects.** The Python text compiler (`effects/compiler/`) handles common wording.
  Per-card bindings in `effects/bindings/*.py` (`@card("GD01-001")`, auto-discovered) handle the
  rest, and a binding wins over the compiler. Prefer a general compiler template or a DSL/engine
  feature over a card-specific hook. Rust never compiles card text: it runs the exported IR, and
  every hook body has a Rust implementation in `crates/gcg-engine/src/interp/hooks/` or
  `view/hooks.rs`.
- **Tests carry traceability tags.** Python: `@pytest.mark.rule("8-5-2")`,
  `@pytest.mark.card(...)`, `@pytest.mark.faq("Q12")` and `@pytest.mark.ruling("GD01-001:Q100")`.
  Rust tests without a Python trace carry a `// gcg-tags:` comment. Every card, every testable
  rule, and every FAQ entry and ruling needs a tagged passing test or a written N/A reason
  (`tests/meta/`, `src/gcg_sim/data/rules/rules_na.json`).
- **No gaps.** No TODO, FIXME, `NotImplementedError`, `todo!`, `unimplemented!`, `#[ignore]` or
  placeholder bodies (`tests/test_no_placeholders.py`, `gcg-diff lints`). Never skip, weaken or
  xfail a test to get green. `unsafe` stays in `crates/gcg-core/src/libm.rs` only.
- **Documented commands run.** Every `bash` block in `README.md`, `CLAUDE.md`, `docs/**/*.md`
  and the gcg-benchmark skill runs in `uv run python scripts/check_doc_commands.py`, unless it
  carries a `<!-- doc-check: skip <reason> -->` marker. Counts in `docs/dev/` use
  `<!-- count:NAME -->` markers (`scripts/doc_counts.py --check`).
- Record interpretation decisions in `docs/ASSUMPTIONS.md`, and in `docs/CONFLICTS.md` when
  sources disagree. Record design decisions in `docs/dev/PLAN.md` and every weakness's outcome
  in `docs/dev/WEAKNESSES.md`.

## Skills

- `/gcg-benchmark`: benchmark and tune a deck with `gcg-sim` (screen with `fast`, polish with
  `standard`), and interpret its reports.
- `/gcg-refresh-data`: refresh the card data, rules and B&R snapshot; implement new cards;
  regenerate the effect IR, the vectors and the trace corpus; run the Python and Rust gates. It
  is the only workflow that uses the network.
