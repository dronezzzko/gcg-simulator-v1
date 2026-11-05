# Port brief (verbatim from the user, 2026-11-01)

This is the task specification for the Rust port. It ranks as the user's instructions. Facts
in `<repository_context>` are orientation only. The code differs in places (Python 3.15, three
reference presets `fast`/`standard`/`strong`, 1152 card numbers); see `docs/port/PLAN.md`
§Assumptions.

<role>
You are the lead engineer and orchestrator for a performance-critical port and AI upgrade. Plan, delegate to subagents, integrate their work, and verify everything yourself. Never report a result you have not reproduced by running it.
</role>

<objective>
Port gcg-sim (the current repository), a Python Gundam Card Game deck-benchmark simulator, to a pure-Rust implementation that:
1. Is behaviourally equivalent to the Python version in its rules engine and card behaviours, and whose port of the reference AI reproduces Python's decisions exactly for the same seed and search budget.
2. Runs the benchmark workload at least 2x faster than Python on an Apple M2 Pro, as measured by the external comparison in <performance_comparison>. 2x is the hard floor; a well-executed compiled rewrite of an MCTS simulator should exceed 10x at equal work. If you land below 10x, profile and explain why in BENCHMARKS.md.
3. Is optimised for Apple silicon, with the M2 Pro as the default target and first-class support for the M6.
4. Exposes exactly two AI presets, `standard` and `fast`, as defined in <ai_presets>.
5. Resolves every known weakness and deferred proposal recorded in the repository and makes the AI significantly stronger, proven head-to-head against the equivalence-verified port of the reference AI under <strength_evaluation>.
</objective>

<repository_context>
Orientation only. Read the actual code before relying on any of it.
- Python 3.12, uv-managed, package `src/gcg_sim`, CLI `gcg-sim` with commands `validate`, `benchmark`, `data status`, `replay`; exit codes 1 (invalid deck/data/replay), 2 (usage), 130 (interrupt).
- Engine: deterministic state machine (`engine.state/view/core/interp/battle/game/observe/invariants`) implementing Comprehensive Rules Ver. 1.9.0. `GameState.clone()` copies ~120 small objects (~15 µs); cloning, legal-action generation and the derived-view fixpoint are the hot paths under search.
- Card effects: card text -> typed effect DSL (`effects.text/compiler/dsl`), per-set bindings and custom hooks for irregular cards (`effects.bindings`), flattened linear programs with resumable `Frame`s (`effects.registry`, `effects.program`). All 1148 card numbers are implemented; `tests/effects/golden/` freezes each card's compiled behaviour.
- AI: information-set MCTS with determinization and fixed node budgets; the reference ships presets `standard` (default) and `strong` (docs/AI.md), plus greedy and random baselines. Budgets are node counts, never wall-clock. docs/AI.md lists known weaknesses (e.g., decks needing long, precise lines may be underrated); README lists limitations; deferred proposals may also live in docs/ASSUMPTIONS.md, docs/CONFLICTS.md, CLAUDE.md, `.claude/skills`, code comments and git history.
- Runner: process pool; every seed derived from the master seed via `derive_seed` (SHA-256 of the seed path) feeding `SplitMix64`; same seed => byte-identical `results.json` for any `--workers`.
- Reports: `summary.md`, `results.json` (validated against a versioned JSON Schema), `games.ndjson`, `replays/*.json`, `timing.json`.
- Tests: pytest; rule-tagged tests mapped in docs/RULES_TRACEABILITY.md; every non-vanilla card has a `@pytest.mark.card` behaviour test; AI-strength tests (vs random 400/400, vs greedy 289/400, Wilson 95% [0.677, 0.764]); `scripts/verify_all.sh` is the full gate.
- Baseline (docs/AI.md): 100 BO3 matches of the example decks, reference `standard` preset, about 6-11 min on 8 cores.
- Data: packaged gcg-api snapshot (ODbL), `overrides.json`, B&R list, provenance in `data/SOURCES.lock.json`. Keep all licences and notices.
</repository_context>

<ai_presets>
The Rust CLI accepts exactly two `--ai-preset` values and nothing else (no aliases; `strong` or any other value is a usage error, exit 2). Default: `standard`.
- `standard`: uses the full search so that each player plays at the level of the strongest top-tier competitive player; for refining and polishing the final deck.
- `fast`: trades search depth for throughput; for rapidly screening deck variants, sub-variants and hypothetical builds.
Both run the strengthened search produced in Phase 6. Their budgets start from the reference presets (`standard` <- reference `strong`; `fast` <- reference `standard`) and are then tuned: `standard` maximises strength subject to the 2x floor against Python `strong`; `fast` maximises throughput subject to being at least as strong as the reference `standard`-budget search. Budgets are node counts; keep them in one documented table (Rust docs/AI.md) with the mapping to the reference presets.
Reference search: the equivalence-verified port of the reference ISMCTS with the exact reference budgets and search parameters stays in the codebase as an internal baseline, reachable only through the evaluation harness `gcg-eval`, never through `gcg-sim --ai-preset`. It anchors byte-level equivalence with Python, the equal-work speed comparison and every strength claim.
Use the wording above in `--help` and the docs; never claim strength you have not measured.
</ai_presets>

<target_hardware>
- Primary: Apple M2 Pro, 10-core CPU (6 performance + 4 efficiency), 16-core GPU, 32 GB unified memory, macOS arm64. All headline measurements are taken here.
- Must also support: Apple M6 (2 nm; 12-core CPU = 2 super + 4 performance + 6 efficiency; 12-core GPU; up to 170 GB/s unified memory). Three CPU tiers mean heterogeneous per-core throughput: scheduling and task granularity must not leave fast cores idle behind efficiency-core stragglers.
- Operating assumption for all design and measurement: the CPU always runs at maximum performance with no idle or low-power states. Do not add power-, thermal- or idle-state handling of any kind (QoS tuning, warm-up logic, frequency checks, sleep/backoff heuristics), and do not caveat results with power-state conditions.
- If no M6 is available, ship build support, runtime detection and a documented comparison procedure for it. Never report numbers you did not measure.
</target_hardware>

<scope>
In scope: a complete Rust replacement for the runtime — CLI (identical commands, flags, defaults, messages where tested, exit codes and output files, except `--ai-preset`, whose values are defined in <ai_presets>; an additive `--workers auto` is allowed), deck parsing/validation, card database/overrides/B&R loading, effect programs and bindings, rules engine, information sets and determinization, the reference search port, the strengthened search behind the two presets, greedy and random players, multi-threaded runner, reports, replay verification, `data status`, the `gcg-eval` harness, and the resolution of every known weakness, limitation and deferred proposal found in the repository (AI, engine, validator or reporting).
Out of scope: changing rules interpretations, card data or packaged sources; the reference's explicit design decisions (multiplayer, sideboarding, clock) — these are the only admissible exceptions in WEAKNESSES.md; adding presets beyond the two; any GUI; any benchmark suite, timing test or timing-dependent logic inside the Rust codebase.
The Python package stays in the repo untouched as the reference oracle until the port is accepted. Only add clearly separated export/trace tooling for differential testing.
</scope>

<assumptions>
- Work in a local clone on the M2 Pro (clone it if the working directory is not the repo), on branch `rust-port`, with a Cargo workspace at the repo root and crates under `crates/`.
- The text->DSL compiler may be ported to Rust or replaced by a build-time export of a versioned IR generated by the Python reference and checked into the repo. Decide during planning and document the trade-off. Either way the shipped `gcg-sim` binary runs fully offline with no Python at runtime.
- Equivalence applies to the engine (Python game traces replayed through the Rust engine) and to the reference search (same seed and budgets => byte-identical `results.json`, `games.ndjson` and `replays/*.json` via `gcg-eval reference-benchmark`, after the comparison script normalises only implementation version strings and the preset-name field and ignores `timing.json`; `summary.md` identical in content modulo those fields). The strengthened presets are not expected to reproduce Python's decisions.
- Do not ask for clarification unless blocked (e.g., repo unreachable). Record every assumption you make in docs/port/PLAN.md.
</assumptions>

<constraints>
- Never meet the speed target by weakening the search: the speed criteria and the strength criteria must hold at the same time. The reference search keeps the exact reference budgets. Equal work, less time for the port; more strength within the time floor for the presets.
- Strength improvements live in the AI only. Rules, card behaviour and data stay unchanged; engine equivalence stays at 100%. Improvements must be deterministic given the seed and must never use wall-clock budgets.
- Preserve the determinism contract: same seed => identical outputs for any thread count or scheduling; no randomness outside the seeded SplitMix64 chain; the reference search keeps identical RNG consumption order and tie-breaking to Python.
- No timing-based benchmarks, timing assertions or wall-clock-dependent behaviour anywhere in the Rust codebase: no criterion or other benchmark suites, no tests that assert durations, no time-based search budgets, timeouts, adaptive throttling, warm-up or idle handling. `timing.json` is written only for CLI parity and is never used for measurement or comparison. The only performance measurement in the project is the external Python-vs-Rust comparison in <performance_comparison>, which lives outside the crates.
- 1148/1148 cards implemented in Rust. An unimplemented card is a hard test failure, as in Python. No silent fallbacks.
- Do not weaken, delete, skip or special-case tests to pass a gate. If a Python test is wrong, report it with evidence instead of working around it.
- Stable Rust toolchain; `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` clean. `unsafe` only where profiling proves a measurable win, each block with a written safety argument and tests (Miri where feasible).
- No runtime network access; keep ODbL and Bandai notices.
- Local, reversible actions are fine. Ask before anything destructive or visible to others (force-push, deleting files you did not create, pushing to the remote).
</constraints>

<execution_plan>
Run the phases in order; each ends with a gate recorded in docs/port/progress.md. Inside a phase, run independent work in parallel through subagents with self-contained briefs (owned files, frozen interfaces, tests to pass, what to report back); they do not share your context. Work directly for sequential or single-file tasks.

Phase 0 — Discovery and baseline (parallel read-only explore subagents)
1. `uv sync --locked`; `uv run pytest -n 8`; record pass counts.
2. Write docs/port/INVENTORY.md: every module, data structure, DSL node and event type, binding module and custom hook, golden-file format, results.json schema, CLI surface, the exact reference preset budgets and every search parameter, and the exact RNG and seed-derivation algorithms (including the SHA-256 path encoding) — precise enough that a Rust implementer never guesses.
3. Write docs/port/WEAKNESSES.md: every known weakness, limitation and deferred proposal found in docs/AI.md, README, docs/ASSUMPTIONS.md, docs/CONFLICTS.md, CLAUDE.md, `.claude/skills`, code comments (TODO/FIXME/deferred/future), tests marked slow or skipped, and git history — each with its source location, a concrete resolution plan, and how its resolution will be verified (scenario test, strength delta, or both). Nothing is dropped silently.
4. Measure the Python baseline on the M2 Pro with the external comparison protocol below; profile it (cProfile/py-spy) to confirm where time goes. Record in docs/port/BENCHMARKS.md.

Phase 1 — Architecture and plan (planning subagent, then your review)
5. Write docs/port/PLAN.md: crate layout; compact, index-based, cache-friendly state layout where `clone` is close to a memcpy; IR-vs-port decision; how bindings and hooks map to Rust; trigger/batch/frame semantics mapping; the preset table and its mapping to the reference; the search architecture that keeps the reference search and the strengthened search side by side behind one player interface; parallelism model (work-stealing pool, per-thread scratch state, per-match seeds independent of scheduling, no idle/backoff heuristics); equivalence and strength-evaluation strategy; milestone order. Create tests.json (id, name, status).

Phase 2 — Oracle harness before engine code
6. Add Python-side trace export (separate module, no behaviour change): per decision, the canonical state snapshot, ordered legal-action list, chosen action and RNG state; plus the final outcome; selectable reference preset. Build a compact, regenerable trace corpus covering the three example decks and generated decks that exercise all 1148 cards (reuse the fuzz/golden tooling).
7. Build the Rust differential runner: replays a trace through the Rust engine and fails on the first divergence with a readable diff of state and legal actions.

Phase 3 — Implementation (parallel implementer subagents, one owner per workstream; adjust the split in PLAN.md only if the code argues for it)
8. A: data layer — card DB, overrides, B&R, deck parser/validator with identical messages and exit codes; `data status`.
9. B: effect IR/DSL, program flattening, interpreter with resumable frames, derived view fixpoint, bindings and hooks.
10. C: engine — zones, events, triggers and batches, battle, turn procedure, rules management, invariants, legal actions, canonical JSON, `clone`.
11. D: information sets, determinization, `information_set_key`, the reference ISMCTS port with exact budgets, RNG consumption and tie-breaking; greedy and random players.
12. E: runner (seeds, BO3, thread pool), reports (identical files, schema-validated, Python `json.dumps`-compatible formatting including key order, indentation and float repr), replay, CLI with the two-value `--ai-preset`, interrupt handling, `timing.json` for parity only.
13. F: `gcg-eval` harness — `reference-benchmark` (full benchmark pipeline with the reference search and reference budgets, same report files) and `head-to-head` (any two players, balanced on-play/on-draw, named deck pairs, Wilson intervals), deterministic and seeded.
Each workstream lands with unit tests and clean clippy; integrate each behind the differential runner as soon as it compiles.

Phase 4 — Equivalence
14. Drive the differential corpus to 100% for both reference budgets. Then end-to-end via `gcg-eval reference-benchmark`: Python vs Rust byte-identical normalised outputs for >=3 seeds x >=2 deck pairs from examples/decks x bo1 and bo3 x both reference budgets at `--matches 10`, plus one 100-match run of the headline configuration, each at `--workers 1`, `4` and `10`.
15. Port rule-tagged, card behaviour and AI-strength tests, or document transitive coverage per rule and card through the differential corpus; update docs/RULES_TRACEABILITY.md so it is accurate for the Rust implementation.
16. Robustness: seeded fuzz games with invariant checks (card conservation, legal state); property tests for RNG and seed derivation against vectors exported from Python.

Phase 5 — Performance (profile first; keep only changes with a measured win in the external comparison)
17. Profile with Instruments/xctrace or samply during development to find hot spots in clone, apply, legal-action generation, derived view, determinize and MCTS iteration. Profiling output is not committed and no benchmark code is added to the crates.
18. Apply and measure: release profile (`opt-level=3`, `lto="fat"`, `codegen-units=1`, consider `panic="abort"`); `-C target-cpu=native` for local builds via `.cargo/config.toml` plus an `apple-m2` baseline profile for distributable binaries; allocator (mimalloc vs system); 128-byte alignment of per-thread hot data (`hw.cachelinesize`); zero heap allocation on the search path; NEON only where profiling justifies it; PGO if it measurably helps. Choose the default worker count from detected topology (`hw.perflevelN.*`) assuming every core is available at full speed; pick task granularity to minimise tail latency on heterogeneous cores.
19. M6: runtime feature detection (`sysctlbyname("hw.optional.arm.*")`, `is_aarch64_feature_detected!`), three-tier topology detection, build notes for toolchains lacking an M6 `target-cpu` name (use `native` or the newest known Apple target), and the comparison procedure.
20. GPU: evaluate whether any stage is a measured win via Metal (e.g., batched evaluations); adopt only with evidence from the external comparison, otherwise record why not in BENCHMARKS.md.
21. Record Part 1 of <performance_comparison> (equal-work speedup of the reference search).

Phase 6 — AI strengthening and weakness resolution (parallel subagents per WEAKNESSES.md item where independent; you own integration and evaluation)
22. For each WEAKNESSES.md item: implement the resolution in Rust (search improvements such as better determinization or belief tracking, horizon handling for long precise lines, smarter expansion/selection/evaluation, transposition reuse, parallel search that stays deterministic; engine/validator/reporting fixes for non-AI items), add a deterministic scenario test that reproduces the weakness and shows the reference search failing and the strengthened search succeeding where the weakness is scenario-shaped, and measure its strength effect with `gcg-eval` alone and combined (ablation). Keep only changes with a measured non-negative effect; record each item's outcome and evidence in WEAKNESSES.md.
23. Tune the preset budgets per <ai_presets> using tuning seeds and tuning deck pairs only; then run the full <strength_evaluation> on disjoint evaluation seeds and held-out pairs.
24. If report fields change (new AI settings, decision-log statistics), bump the results.json schema version and keep validation; keep replays verifiable by `replay`.
25. Record Part 2 of <performance_comparison> (shipped presets).

Phase 7 — Independent verification (a reviewer subagent that wrote none of the code)
26. From a clean clone: build, run every gate, re-run the equivalence, comparison and strength protocols on fresh seeds, confirm by reading both sources that the reference search's budgets and parameters equal the reference exactly and that `gcg-sim` accepts only `fast` and `standard`, check that tuning and evaluation seeds and deck pairs are disjoint, confirm every WEAKNESSES.md item is resolved with evidence or is one of the admissible exceptions, grep for shortcuts (`todo!`, `unimplemented!`, ignored tests, skipped cards), confirm the crates contain no benchmark suites, timing assertions or wall-clock-dependent logic, review every `unsafe` block, and verify identical outputs across `--workers 1/4/10`. Record findings in docs/port/REVIEW.md; fix and re-verify until clean.

Phase 8 — Documentation and hand-off
27. README (build/run on M2 Pro and M6; the two presets and when to use each), Rust docs/AI.md (preset table, mapping, search design, ablation and measured strength), docs/port/WEAKNESSES.md (final status), docs/port/ARCHITECTURE.md, docs/port/EQUIVALENCE.md (methodology and every known divergence with root cause and impact), docs/port/BENCHMARKS.md, updated CLAUDE.md and `.claude/skills/gcg-benchmark` for the two-stage workflow (screen variants with `fast`, polish the final deck with `standard`), and a GitHub Actions workflow on an arm64 macOS runner running fmt, clippy, tests and the differential corpus (no timing jobs).
</execution_plan>

<strength_evaluation>
Run with `gcg-eval head-to-head` (deterministic, seeded, BO1 games balanced on-play/on-draw, 95% Wilson intervals per pair and pooled).
Deck set: the three example decks plus >=4 additional legal decks of distinct archetypes built from the packaged pool and committed under examples/decks/eval/; hold out >=2 deck pairs never used during tuning; evaluation seeds disjoint from tuning seeds; >=400 games per pair.
Required results:
- `standard` vs reference search at reference `strong` budget: pooled 95% lower bound >= 0.55, and no held-out pair whose 95% upper bound is below 0.5.
- `fast` vs reference search at reference `standard` budget: pooled point estimate >= 0.5, and no pair whose 95% upper bound is below 0.5.
- `standard` vs random: 400/400, or every loss documented with its replay; `standard` vs greedy: 95% lower bound above the reference's 0.764.
- Every scenario test from Phase 6 passes; every WEAKNESSES.md item has an ablation row (individual and combined effect).
Report the full table in docs/AI.md; replays of surprising results are saved and inspected.
</strength_evaluation>

<performance_comparison>
An external wall-clock comparison of the two binaries, run and documented outside the Rust crates; it is the only timing-based activity in the project. Python baselines are measured once and reused for both parts.
Workload: `benchmark examples/decks/red-green-zeon.txt examples/decks/blue-white-federation.txt --format bo3 --seed 1 --out <dir>`.
Part 1 — implementation speedup at equal work (Rust via `gcg-eval reference-benchmark` with the reference search and budgets):
- Pair A (screening budget): Python `--ai-preset standard` vs Rust reference `standard` budget.
- Pair B (full search): Python `--ai-preset strong` vs Rust reference `strong` budget.
Part 2 — shipped presets (Rust via `gcg-sim`): `--ai-preset fast` vs Python `standard`; `--ai-preset standard` vs Python `strong`.
Configurations for each pair in each part:
- Headline (screening pairs): `--matches 100`, at each implementation's best worker count and at an identical 10 workers.
- Single-thread (screening pairs): `--matches 20 --workers 1`.
- Full-search pairs: `--matches 20` at the best worker count.
Run with hyperfine (>=3 runs for long configurations, >=5 for short), both implementations under identical conditions with no other load. Under the maximum-power assumption, no warm-up, power or thermal controls are needed. Report median/min/max, speedup ratios, CPU model, core counts, memory, macOS, Python, uv, rustc and LLVM versions, and the exact commands. The 2x floor must hold in every configuration of both parts.
</performance_comparison>

<deliverables>
1. Rust workspace (crates for data, effects, engine, ai, runner/reports, cli, eval) producing `gcg-sim` (two presets, strengthened search) and `gcg-eval` (reference search and head-to-head evaluation), containing no benchmark code, timing tests or timing-dependent logic.
2. Tests: unit, differential (trace corpus), end-to-end equivalence, fuzz/invariants, property, AI-strength and weakness scenario tests; tests.json; `scripts/verify_all_rust.sh` mirroring the Python gate.
3. `scripts/compare_python_vs_rust.sh` implementing the external comparison and output normalisation (outside the crates); docs/port/BENCHMARKS.md with results; examples/decks/eval/ with the evaluation decks.
4. Documentation from Phase 8, `.cargo/config.toml` with documented profiles, CI workflow.
5. A clean git history on `rust-port` with one meaningful commit per milestone.
</deliverables>

<acceptance_criteria>
All must hold and be demonstrated by commands and outputs in the final report:
- `scripts/verify_all_rust.sh` passes from a clean clone: fmt, clippy, all tests, differential corpus 100%, results.json schema validation.
- Byte-identical normalised results.json, games.ndjson and replays between Python and the Rust reference search for every Phase 4 configuration; identical outputs across worker counts for every player; `replay` verifies every saved replay.
- Exactly two presets, `standard` (default) and `fast`, accepted by `gcg-sim`; any other value is a usage error (exit 2); the reference search is reachable only via `gcg-eval` and carries the exact reference budgets and parameters (reviewer-verified).
- Every WEAKNESSES.md item resolved with evidence (scenario test and/or ablation row), the only exceptions being the reference's explicit design decisions named in <scope>.
- All <strength_evaluation> thresholds met on held-out pairs and fresh seeds, reproduced by the reviewer.
- 1148/1148 cards implemented and enforced by a coverage test; RULES_TRACEABILITY.md accurate.
- Measured speedup >=2x in every configuration of both parts of the external comparison on the M2 Pro, with rules and data unchanged.
- The Rust workspace contains no timing-based benchmarks, timing assertions or wall-clock-dependent behaviour (reviewer-verified).
- M6 build and detection support present and documented; no unmeasured performance or strength claims anywhere.
- Reviewer sign-off in docs/port/REVIEW.md with every finding resolved.
If byte equivalence for the reference search is impossible for a specific, root-caused reason (e.g., Python-specific floating-point ordering), document it in EQUIVALENCE.md, fall back to statistical equivalence (win rates with overlapping 95% Wilson intervals over >=400 games per configuration), and flag it prominently in the final report. Never accept unexplained divergence.
</acceptance_criteria>

<working_conventions>
- This task spans many context windows. Keep docs/port/progress.md, tests.json, WEAKNESSES.md and git current; commit after every milestone. On resume: read progress.md, tests.json, WEAKNESSES.md and `git log`, run the quick gate, then continue. Never restart from scratch and never stop early with uncommitted work.
- Read the relevant Python module before porting it; never port from memory or assumption.
- Make independent tool calls in parallel.
- Prefer simple, general solutions: no hard-coding for the trace corpus or evaluation decks, no test-specific branches, no speculative abstractions.
- Remove temporary scripts and files you created for iteration.
- At each phase gate, tell the user in a few sentences what was verified, what remains and any risk to the acceptance criteria.
</working_conventions>

<final_report>
End with: a results table (Python vs Rust for every comparison configuration in both parts, with speedups), equivalence status (exact or statistical, with every divergence), the preset table with budgets and measured strength, the WEAKNESSES.md status table with evidence per item, card coverage, each gate command with its outcome, confirmation that the crates contain no timing-based code, known limitations, and recommended next steps. State uncertainty plainly; no padding.
</final_report>
