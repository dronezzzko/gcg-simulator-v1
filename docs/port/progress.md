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

## Phase 0 — Discovery and baseline (done)

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

## Phase 2 — Oracle harness (done)

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

## Phase 3 — Implementation (done; the open traceability item closed in Phase 4)

### M5.2–M8 — engine layers L2–L5, reference AI, runner and CLI, gcg-eval, real-engine diff

- Wave 2b (workflow wf_9026cf90-4d3): nine worktree agents wrote L2 view, L3 core/battle,
  L4 interpreter, L5 game/observe/invariants/procedures/testkit, D1 policy/evaluation, D2 agent
  core/actions/no-op/setup + gcg-ai-reference + gcg-search v0, E runner orchestration + gcg-sim
  CLI, F gcg-eval, G gcg-diff real engine, against the frozen signatures with local shims
  (never committed). No two branches touched the same file.
- Integration agent (main tree): lib.rs wiring, every accepted interface request (recorded in
  docs/port/contracts.md, change log "Wave 2b integration"), `Observation` in gcg-search (lint
  rule 5), compile and test fixes. Lead: dict-valued probe results (`testkit.keywords`) are
  compared as mappings, as Python's own `==` does (insertion order of the view's keyword dict is
  read nowhere else: `ai/values.py:30` uses len, `ai/actions.py:356` sorts); the interp
  resume-test floor pinned to the 17 effect decisions tier 1 holds.

**Gate (lead-run).** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D
warnings` clean; `cargo test --workspace --release`: 631 passed, 0 failed; slow suite
(`--features gcg-diff/slow,gcg-engine/slow,gcg-eval/slow,gcg-search/slow`): 641 passed, 1
failed (traceability, below). Differential corpus against the real engine: **tier 1 3804/3804**
(17,601 decisions, 2,166 snapshots, 1,396 reference/baseline agent decisions) and **tier 2
4154/4154** (123,726 decisions, 9,078 snapshots, 12,131 reference-search decisions at fast,
standard and strong) — no state, legal-option, RNG, error or agent divergence. `--roundtrip`
3804/3804. `traces.py --check --tier 1` under PYTHONHASHSEED 1 and 2 and with `--nocache`,
`pytest_trace.py --check` and `--selftest` (27/27), `compare_python_vs_rust.sh equivalence
--selftest` (15/15) and `report --selftest` (14/14), `gcg-diff selftest`, `lints`,
`tests-json --check` pass. tests.json: 50 passing, 2 failing (T-010, T-094), rest not started.

**Open.** Traceability: 17 rules and 6 FAQ entries tested by Python suites that produce no
engine trace (card data, deck construction) need native Rust tests tagged `// gcg-tags:`
(gcg-data), then `gcg-diff traceability --write`. Phase 4 matrix, probe, fuzz, paranoid runs,
AI test ports and parity tests follow.

## Phase 4 — Equivalence (done)

### Phase 4 test completion (workflow wf_c6937f85-9a4, five worktree agents, merged)

- P4-A traceability: native Rust tests tagged for the 23 entries whose Python tests leave no
  engine trace (card data, compiled scripts, deck construction); `docs/RULES_TRACEABILITY.md`
  and `docs/port/TRACEABILITY_CARDS.md` regenerated (lead re-ran `--write` after the merge).
- P4-B: 10,000-game fuzz at seed 2026 with invariants, summary byte-equal to Python's; 2,000
  games at a commit-derived seed; paranoid tier 1 and tier 2; paranoid check counters
  (`crates/gcg-engine/src/paranoid.rs`, feature-gated, no game effect).
- P4-C: 52 puzzle cases with decision logs equal to Python's, 128 information-set cases,
  fork-join play/draw choice equals serial.
- P4-D: identical outputs at 1/4/10 workers (reference, v0 search, greedy, random), all 25
  Python replays verify under Rust byte for byte, two-value `--ai-preset`, reference-benchmark
  with test agents, schema negative controls, ports of tests/runner, reports, cli, deck,
  `gcg-eval compare` equal to Python, head-to-head identical across worker counts, seed roles,
  help and usage parity, topology parsing.
- P4-E: 12 lead-example ports, touch/version catalogue, engine property tests, once_used
  semantics, deck order in new_game.

**Gate (lead-run).** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets
--all-features -- -D warnings` clean; `cargo test --workspace --release`: 864 passed, 0 failed;
slow suite (`--features gcg-diff/slow,gcg-engine/slow,gcg-eval/slow,gcg-search/slow`): 880
passed, 0 failed; `gcg-diff traceability --check` passed (Python 395 rules tested, 184 N/A;
Rust 395 tested, 0 without evidence; 840/840 non-vanilla cards, 119/119 FAQ, 360 rulings + 8
N/A with Rust evidence); `gcg-diff coverage --check`, `lints`, `tests-json --check` passed.
Agent-run long items (not re-run by the lead): paranoid tier-1 corpus 3804/3804 and paranoid
tier 2 (T-054, T-091), 10,000-game fuzz comparison (T-055). tests.json: 78 passing, 1 failing.

**Open.** T-056 failing: one `search:fast` strategic fuzz game in 1,000 (game 221, seed
11012090770908643750) reaches the W-143 redirect cycle inside a rollout; the engine reports
`RedirectCycle` exactly where Python raises `RecursionError` (not a divergence). Resolution in
Phase 6: the strengthened search scores a simulated line that ends in an engine error instead
of failing the real game (AI only; W-143, W-110). End-to-end matrix and the numbering-shift
probe still running.

## Phase 5 — Performance (done; gate M10, final timing in Phase 8)

- Phase 5 agent (worktree): profile-driven, output-identical optimisations, one commit each,
  measured by instructions retired (load-independent) on single-thread reference runs:
  sparse derived view in recycled per-thread buffers (14.75G → 6.75G instructions per std
  game), allocation-free hot paths (→ 5.57G), trigger index read off the cached view's hosts
  (→ 5.44G), mimalloc default allocator (→ 4.82G), skip of the confirming fixpoint pass when
  its reads repeat, with a paranoid check that runs the skipped pass (105,974 runs, all equal)
  (→ 4.65G), opt-in PGO via `scripts/build_pgo.sh` (→ 3.84G). Dropped (no gain): recycled
  worlds, link memo, per-definition zone flags, target-cpu variants. GPU not adopted: leaf
  evaluation 0.9% and policy 5.4% of time, not batchable in the sequential reference search.
- Rough (loaded machine, not a measurement of record): std BO1 game 0.29–0.34 s user vs Python
  13.3 s; 20 BO3 matches single-thread 24.9 s vs 1269.5 s; at 10 workers 8.9 s vs 221 s.

**Gate (lead-run, merged tree, `CARGO_TARGET_DIR=target/integrate`).** fmt clean; clippy
`--all-features -D warnings` clean; `cargo test --workspace --release`: 984 passed, 0 failed;
corpus tier 1 3804/3804 and tier 2 4154/4154; lints pass; std one-game
`final_state_sha256` unchanged (69c66131…7e4e). Agent-run: slow suite 884 passed, paranoid
tier 1 3804/3804, paranoid engine tests 214, 10,000-game fuzz equal to the committed summary.
Not done: zero allocation per iteration (about 150 left per iteration, about 2% of samples);
paranoid `search:fast` fuzz stopped under load; Part 1 hyperfine runs (need an idle machine).

### M9 — Phase 4 gate (done)

- End-to-end matrix (`scripts/compare_python_vs_rust.sh equivalence`, defaults): 108 Python
  configurations (3 seeds x 3 example pairings x bo1/bo3 x reference fast/standard/strong x
  decision log off/on, `--matches 10`), each Python baseline identical under PYTHONHASHSEED 1
  and 2 (216 runs); Rust `gcg-eval reference-benchmark` at workers 1/4/10: **324/324
  byte-identical** after masking the three implementation fields (19 h 06 min wall).
- Headline 100 matches (reference standard, bo3, seed 1, rgz vs bwf): **3/3** identical at
  workers 1/4/10, rebuilt from the post-Phase-5 tree.
- Numbering-shift probe (T-033): 0 of 8 configurations depend on Python's effect numbering;
  Python workers 1 vs 10 identical in 8.
- docs/port/EQUIVALENCE.md: method, corpus and matrix results, probe, 7 known differences
  (none changes a compared byte). tests.json: T-002, T-015, T-033, T-062-T-064, T-083-T-090
  passing.

**Gate.** Byte equivalence is exact; no statistical fallback is needed. Risk carried forward:
the matrix binaries predate Phase 5, but the headline, the corpus (tier 1 and 2 at 100% after
every later commit) and every package's three-preset smoke cover the current code.

### M10 — Phase 5 gate: Part 1 and Part 2 measured (done)

- Idle-machine timing, commit 34b3107 (plain release, code digest f9030725…): best Rust worker
  count 6 for both the reference search and the presets (20-match search: 8.5 s at 6 vs 9.6 s
  at 10). Part 1: headline 23.5x (best) / 20.3x (10 workers), single-thread 45.1x, full search
  24.2x. Part 2: fast headline 12.6x / 11.0x, single-thread 24.1x; standard full search 2.66x.
  `scripts/compare_python_vs_rust.sh report`: "all 8 configurations present, current and at
  least 2x". Tables in docs/port/BENCHMARKS.md.
- `--workers auto` now uses the fast tiers only (6 on the M2 Pro, 2+4 on the M6 layout),
  because 6 workers beat 10 on every timed workload (match-granular tail on efficiency cores).
- GPU: not adopted (evaluation 0.9%, policy 5.4% of time; not batchable in the sequential
  reference search). M6: runtime detection, three-tier topology and build notes in
  BENCHMARKS.md (no M6 available; nothing measured on one).

## Phase 6 — AI strength (done; final evaluation in Phase 8)

### M11–M14, M12.J/K/L/M/N — tooling and mechanisms (done)

- M11 (8ae4767): `gcg-eval` evaluation tools (paired head-to-head statistics, ablation rows,
  budget curves, fidelity, regret, tune, calibrate, gates for E1–E4, evidence manifests, seed
  roles with held-out refusal and the ledger); five new legal decks and the pre-registered panel
  (docs/port/eval/panel.json, PAIRS.md: 28 pairs, 13 held-out); `gcg-search` Mechanisms
  extension points, `MechanismFlags::NULL` replaying the reference, the Observation built from
  a determinisation (W-135).
- M13 (8af9b0e) and M14 (e14c839): every strength package (P6-A … P6-H) behind its own flag,
  each with scenario tests (reference failing, mechanism succeeding) and individual ablation
  rows in docs/port/eval/; M12.J/K/L/M/N (ac95836, 7614bec, 8af9b0e): engine hardening, data
  preservation, validator, reporting and tooling items; O9 (9ec31c0): results schema 1.2.0.
- The null configuration replays the reference bit for bit after every wave
  (`crates/gcg-search/tests/null_config.rs`).

### M15 — tuned presets (done, 82b0b34)

- Bundle (adaptive_horizon, capped_rollout_continuation, tactical_solver,
  exclude_capped_play_draw, engine_error_leaf) against null at 2x shipped standard: confirm
  854/1600, +0.068 [+0.030, +0.105]. Budget curves on the tuning pairs set `standard` = 8x the
  reference `strong` budget (1024/2560/10240) and `fast` = 1.5x the reference `standard` budget
  (96/240/960). Evidence: docs/port/eval/M15/.

### WEAKNESSES outcomes and gap closing (done, 1007ca9, b8483dc)

- Every one of the 164 items has a recorded outcome (docs/port/WEAKNESSES.md); the residual
  gaps (docs/port/eval/weakness-gaps.md) were closed or measured in b8483dc (GAP-A1, A2, B, C,
  D status files in docs/port/eval/weakness-status/).

### Final evaluation 1 (archived, aeaaf61) and M15b (done, 8c89b5e)

- Final evaluation 1 (M15 presets, eval masters 90200001–6): E1 0.5596 [0.5504, 0.5688],
  E2 0.5510, E3 400/400 and 11173/11200, E4 0.7675 [0.7237] (test decks) and 0.7728 [0.7649]
  (panel). Too close to the bars, and the 27 panel losses to random were not documented with
  replays, so it was not accepted (docs/port/eval/final-1/README.md).
- Follow-up, on tuning-pair evidence only (tuning, dev and confirm seeds; no eval or review
  seed, no held-out pair, nothing from the final-1 games): M15b adds `kill_aware_targeting`
  (W-26) and `tree_lethal_select` (W-25) to both presets. Against the M15 `standard` 831/1620 =
  0.513 [0.489, 0.537]; against `ref:strong` 950/1620 = 0.586; against greedy 338/420 vs
  328/420 on the same deals (docs/port/eval/M15b/README.md).

**Gate.** `cargo test --workspace --release`: 1320 passed after the pinned M15b expectations
were updated (the W-25 scenario, the portable-math keys, the Rust replay vectors that Python
re-verified); slow gcg-search tests 306 passed; corpus tier 1 3804/3804; lints, traceability,
API snapshot and doc checks pass. Final evaluation 2 is recorded under Phase 8.

## Phase 8 — Documentation, final timing and final evaluation

### Documentation (done, 8c89b5e)

- README rewritten for the Rust implementation; docs/AI.md for the Rust presets and search, the
  Python document moved to docs/python/AI.md (D14a); docs/port/ARCHITECTURE.md; CLAUDE.md and
  both skills for the Rust workflow; `.github/workflows/rust.yml` (arm64 macOS, no timing jobs)
  and `scripts/ci_local.sh`; `crates/gcg-search/tests/ai_md_presets.rs` checks docs/AI.md's
  preset tables against presets.rs.

### Final timing (done)

- Idle machine, commit 8c89b5e (code digest 5240d597…), 6 workers as measured best:
  Part 1 headline 23.5x (best) / 20.3x (10 workers), single-thread 45.3x, full search 24.7x;
  Part 2 `fast` headline 11.8x / 10.4x, single-thread 21.2x; `standard` full search 2.40x
  (148.6 s against 356.0 s; the M15b mechanisms cost 11%). `scripts/compare_python_vs_rust.sh
  report`: "all 8 configurations present, current and at least 2x". Tables in
  docs/port/BENCHMARKS.md.

### Registers and remaining deliverables (2026-11-04)

- `scripts/build_dist.sh` (T-143): apple-m2 dist binaries with LICENSE-DATA and NOTICE.txt;
  `target/dist/gcg-diff corpus --tier 1 --require-all` 3804/3804 (T-092);
  `scripts/verify_binary_offline.sh target/dist/gcg-sim` passed (T-145). `verify_all_rust.sh`
  runs all three.
- WEAKNESSES reconciled with the gap-closing and M15b records (eeecfb5); lead decisions for
  W-05, W-63/W-67 (T-154), W-111 and W-137's adoption in PLAN §0.1 (d362c11); W-101 anchors
  (93cc30d; tier 1 3804/3804, tier 2 4154/4154 after the merge); W-17 blinded audit (c54fc87);
  W-18 fidelity measured on eval master 90300007, verification not met (7b7ab2f).
- `gcg-eval head-to-head-replay` (276f497) writes verified replays of head-to-head games; the
  27 E3 panel losses of final evaluation 1 re-play and verify 27/27.
- tests.json, with the evidence behind each change:
  - passing: T-013 (BENCHMARKS M6 section, topology fixtures); T-034 (`census.py --check`;
    every census maximum within its capacity, e.g. event payload 8 of 10, once key 6 of 6);
    T-059 (the components vectors are the gcg-ai tests `search_components.rs`,
    `search_engine.rs` and `policy_*.rs`, not a corpus set as PLAN's appendix names it);
    T-075 (`crates/gcg-cli/tests/interrupt.rs`); T-092, T-143, T-145 (above); T-096–T-103 (the
    final timing report, all 8 at least 2x; T-011 stays with the reviewer's rerun); T-105
    (BENCHMARKS Phase 5 log); T-106 (`gcg-diff weaknesses --check`); T-107–T-123 (the scenario
    files of `crates/gcg-search/tests/scenarios/`, 306 slow gcg-search tests passed at the
    M15b gate); T-128 (`gcg-eval decks --overlap --check`, P6-decks); T-129 (schema-check in
    the gate); T-130 (`ai_md_presets.rs`); T-132 (M15 budget curves, BENCHMARKS Part 2);
    T-137, T-138, T-140–T-142 (the Phase 8 documents); T-149 (`head_to_head.rs` balance
    tests); T-153 (the gate's byte comparison and `p6m_cli.rs`); T-154 (lead decision:
    `templates/check.py --check`, exit 0); T-155 (`pytest port/oracle/tests/refresh_news`: 9
    passed).
  - failing: T-007 (7 WEAKNESSES items open); T-104 and T-148 (zero allocation after warm-up
    is not reached: about 150 allocations per MCTS iteration, BENCHMARKS "Where the time
    goes"); T-131 (W-18's verification not met).
  - passing after the Phase 7 reviewer's clean-clone runs at 7b7ab2f (`docs/port/REVIEW.md` §1):
    T-135 (`scripts/verify_all.sh`, 3985 Python tests) and T-056 (2,000 commit-seed and 1,000
    strategic paranoid fuzz games, 0 failures, M15b presets).

### Final evaluation 2 (done; `docs/port/eval/final-2/README.md`)

- Shipped M15b presets, binaries of 8c89b5e, eval masters 90300001-90300006 run once, forced
  play/draw, 6 workers (2026-11-03 23:42 to 2026-11-04 21:44 UTC).
- E1 6408/11200 = 0.572 [0.563, 0.581], held-out 0.562 [0.548, 0.575]: the brief's bar holds;
  the plan's held-out pooled bound (A26) misses by 0.002. E2 6254/11200 = 0.558 [0.549, 0.568]:
  pass. E3 400/400 on the test decks and 11186/11200 on the panel, the 14 losses re-played by
  `gcg-eval head-to-head-replay --losses`, matched to their rows and verified by `gcg-sim
  replay` (14/14): pass. E4 panel 8774/11200 = 0.783 [0.776, 0.791], held-out [0.767]: pass;
  E4 test decks 304/400 = 0.760 [0.716]: fail.
- `gcg-eval gates --losses-documented final-2/e3-losses/losses.json`: E2, E3 pass; E1 and E4
  fail one check each (above). Inspection notes: `docs/port/eval/notes.md` (T-144).
- W-04 resolved (lower bound above 0.5 on held-out seeds). W-19 reopened after the review:
  E4 on the AI test decks fails on three independent seed sets (final 1, final 2, review).
  tests.json: T-125, T-126, T-139, T-144 passing; T-124 (held-out pooled bound) and T-127 (test
  decks) failing.

### Phase 7 round 1 (reviewer, `docs/port/REVIEW.md`)

- Clean clone of 7b7ab2f: `verify_all_rust.sh --full` exit 0 (1332 default, 1356 slow, 263
  paranoid, 1333 tier-2 slow tests; corpus tier 1 and 2 at 100%); `verify_all.sh` exit 0 (3985
  Python tests); equivalence on review seeds 129/129; worker identity checks identical.
- E1-E4 on review masters (`docs/port/eval/review/`): E1 0.5708 [0.5616] (held-out [0.5528]),
  E2 0.5679 [0.5588], E3 400/400 and 11181/11200 (19 losses documented with verified replays),
  E4 test decks 305/400 [0.7184] and panel [0.7743] (held-out [0.7623]); `gates --check` exit 1
  on E4. tests.json T-008 failing.
- 18 findings (R-01 blocker: E4 on the test decks); fixes for R-03, R-04, R-06 to R-09, R-11,
  R-12 and R-18 in 8d2a73e; W-154 resolved by the Python gate run (0fc921e).

### Phase 7 round 2 (reviewer, `docs/port/REVIEW.md` §6-§7)

- Timing at HEAD 2d425db on the idle machine: all 8 configurations at least 2x (Part 1 23.59x /
  20.34x / 45.31x / 24.66x; Part 2 `fast` 11.82x / 10.37x / 21.25x, `standard` 2.40x); `report`
  exit 0 (T-011). W-126's throughput pair agrees within 0.33% (resolved).
- `verify_all_rust.sh --full` exit 0 at HEAD (T-001; zstd 145 and `--no-default-features` 189
  tests included); `ci_local.sh` exit 0, 1340 tests (T-156); documented commands 20 run, 19
  skipped, 0 failed (T-134); reading checks of the reference constants and reachability (T-006).
- Partial sign-off: gates, equivalence, worker identity, the two presets and the reference
  reachability, speed, no timing code, the `unsafe` review, cards, the oracle untouched,
  disjoint seeds and pairs, E1-E3 on review seeds. Not signed off: E4 (R-01, reported to the
  user), the open WEAKNESSES items, CI on GitHub (R-17). T-014 failing.
- Lead fixes after round 2: the gates match a documented loss's replay to its game (index and
  the pair's decks; R-11), the held-out guard compares card multisets (R-10), `target/dist`
  holds only the shipped binaries (R-09), `equivalence --help` (R-12), the throughput command
  (R-19), the "not adopted" definition says the weakness remains (R-03).

### Final report (`docs/port/FINAL_REPORT.md`)

- Written from the committed records; T-146 and T-147 (`git status --porcelain
  --untracked-files=all` empty) passing. Not complete against the brief: E4 on the AI test decks,
  four open WEAKNESSES items (W-18, W-19, W-120, W-137), partial sign-off, CI never run.
