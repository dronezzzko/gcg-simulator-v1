# Phase 7 review (independent reviewer, round 1)

Brief step 26 and PLAN §13 M16. The reviewer wrote none of the port's code. This file records
what was checked, how, and every finding. The lead fixes; the reviewer re-verifies.

**Status: not signed off.** Round 1 is complete (2026-11-05). The strength protocol on fresh
review seeds fails E4 (R-01, a blocker). Round 2, the timing rerun on an idle machine, has not
started. Sign-off needs every finding below resolved and round 2 done.

## 1. Scope and method

| Item | Value |
| --- | --- |
| Commit reviewed | `7b7ab2f` (code and gates). The branch moved to `a0e065a` during the review (`c54fc87` W-17 audit records, `a0e065a` registers). `git diff --stat 7b7ab2f..a0e065a -- crates scripts src tests port Cargo.toml Cargo.lock .cargo` is empty, so the code results hold for `a0e065a`; the docs and registers were read and checked at `a0e065a` |
| Clean clone | `/var/folders/0b/bb2yzbwn76l2y5lcblwyy6y80000gn/T/tmp.Glcf65zxkn/repo` (`git clone --branch rust-port`), binaries built there with `cargo build --release --locked --offline -p gcg-cli -p gcg-eval -p gcg-diff`; a second clone of `a0e065a` in `.../tmp.Glcf65zxkn/repo-head` for the register checks |
| Machine | Apple M2 Pro (6P + 4E), macOS 27.2, rustc 1.99.1, Python 3.15.1, uv 0.13.7. Shared with the lead's final evaluation 2 (6 workers) until it ended at 2026-11-04 21:45 UTC; every reviewer job ran under `nice -n 10`; load averages 20-55. No wall time below is a measurement |
| Seeds | Review role [91000000, 91999999]; none of the nine used appears anywhere in the repository before this review (`grep -rl` over `docs port scripts crates examples`) |

### Commands and outcomes

| # | Command (from the clean clone) | Outcome |
| --- | --- | --- |
| C1 | `GCG_GATE_WORKERS=4 scripts/verify_all_rust.sh --full` | **exit 0**, "verify_all_rust passed" (2026-11-04 08:12-19:51 UTC, its own clone of `7b7ab2f`). fmt and both clippy runs clean. Tests: default tier 1332 passed in 139 test binaries; slow offline tier (`gcg-diff/slow,gcg-eval/slow,gcg-search/slow`) 1356 in 143; `gcg-engine --features paranoid,testkit` 263 in 23; tier-2 slow tier 1333 in 139; 0 failed and 0 ignored anywhere (444 `test result` lines, 4284 passes in all). `gcg-diff lints` passed; `selftest` 22 and `--real-engine` 36 negative controls fired. Tier-1 corpus 3804/3804 in each of parse-only, mock, mock roundtrip, real engine (17,601 decisions, 2,166 snapshots, 1,396 agent decisions), real roundtrip, paranoid, and the `apple-m2` dist build. `coverage --check` (840/840 non-vanilla by a passing tagged trace; 0 cards with a program and no execution evidence), `traceability --check`, `tests-json --check`, `weaknesses --check`, `api-snapshot --check` passed. Fuzz: 10,000 games seed 2026 equal to the committed summary, 2,000 at the commit seed, 400 greedy, 400 `search:fast` and 200 mixed paranoid games, 0 failures each; `paranoid-games` passed (every check above its run floor). `decks --overlap --check`, `evidence check`, the `cargo tree` reachability checks, the 1/4/10-worker identity runs (`gcg-sim --ai-preset fast`, reference `fast`, `head-to-head`), replay verification, `schema-check` (2 runs, 19 vectors valid, 21 invalid vectors rejected), the four bad `--ai-preset` values (exit 2), the dist build, `verify_binary_offline.sh` ("3 pairings, workers 1/4/10"), the tool self-tests, `cli_help_snapshot`, `doc_counts` (10 counts, 0 stale) and the reference-untouched check passed. `--full`: ruff, format, `mypy --strict port/oracle` (33 files) clean; documented commands "20 blocks run, 19 skipped, 0 failed" (2 network blocks); census, fuzz-census, test-census, traces `--check` under both hash seeds and `--nocache`, vectors, `eval_decks`, `p6k`, templates checks current; `pytest port/oracle/tests` 46 passed; equivalence and report self-tests 15/15 and 14/14; the seed-1 equivalence smoke 3/3; tier 2 regenerated under both hash seeds and replayed 4154/4154 (123,726 decisions, 12,131 search decisions). Warnings printed, not failures: two stale Python-side research artefacts (`docs/research/DATA_INGESTION.md`, `data_profile.json`; W-115/W-128 follow-up) |
| C2 | `scripts/verify_all.sh` (Python gate, W-154 / T-135) | **exit 0**, "verify_all passed" (2026-11-04 08:28-09:45 UTC, its own clone of `7b7ab2f`): `uv sync --locked`; ruff "All checks passed!"; ruff format "248 files already formatted"; mypy "no issues found in 101 source files"; `pytest -m "" -n auto` **3985 passed** in 3864.7 s (the Phase 0 count; no failure, skip or xfail); golden and sources checks silent; conflicts "402 conflicts ... unresolved 0, invalid 0, stale 0, orphaned 0"; rules index current (579 rules); `uv build`; offline wheel check passed |
| C3 | `scripts/compare_python_vs_rust.sh equivalence` on fresh seeds (reduced matrix, §5) | **129/129 identical**, every Python baseline identical under PYTHONHASHSEED 1 and 2 (43/43); all three invocations exit 0 (08:24-15:30 UTC) |
| C4 | E1-E4 with `--role review` (§4) | all six reports done (2026-11-04 08:12 to 2026-11-05 07:16 UTC); `gates --check` exit 1: E1 and E2 pass, E4 fails, E3 needs its 19 loss replays (§4) |
| C5 | `cargo build --release -p gcg-diff --features paranoid --target-dir target/paranoid`, then `target/paranoid/release/gcg-diff corpus --tier 2 --require-all --paranoid --jobs 3` on the lead's tier-2 corpus (`build/oracle/tier2`, copied into the clone) | **exit 0**: 4154/4154 traces (coverage 468, fuzz 500, search 144, views 2034, worlds 1008), 123,726 decisions, 9,078 snapshots, 12,131 agent decisions; this backs EQUIVALENCE.md's "both also pass with the paranoid feature" for tier 2, which the gate does not run |
| C6 | `cargo tree -p gcg-cli -e normal --offline \| grep -c gcg-ai-reference` | `0` |
| C7 | `gcg-sim benchmark ... --ai-preset P` for P in `strong STANDARD Fast bogus "" "standard " st ref:strong reference baseline-standard search:standard null-standard standard:max_iterations=10` | exit 2 for all 13, argparse `invalid choice` message; no output directory created |
| C8 | `gcg-sim benchmark ... --matches 1 --format bo1` without `--ai-preset` | `results.json` `ai.preset` `standard`, `ai.factory` `gcg-search::make_agent('standard')`, `ai.search.mapping` `reference strong`, schema 1.2.0 |
| C9 | `grep -rnE 'todo!\|unimplemented!\|#\[ignore\]\|std::time\|Instant\|SystemTime\|sleep\|criterion\|\[\[bench\]\]' crates/` | only `crates/gcg-runner/src/clock.rs` (the `timing.json` clock), lint sources, lint fixtures and one comment in `gcg-core/src/pool.rs` |
| C10 | `grep -rn unsafe crates/` | one block: `crates/gcg-core/src/libm.rs:11-19` (`unsafe extern "C"` `erfc`); the rest are lint code, lint fixtures and the API-snapshot tokenizer |
| C11 | Worker identity, reviewer's own runs: `gcg-sim benchmark examples/decks/eval/blue-red-tempo.txt examples/decks/purple-white-tekkadan.txt --matches 3 --format bo1 --seed 4242 --ai-preset standard --decision-log --workers 1/4/10`; `gcg-eval head-to-head search:fast greedy --decks examples --games 12 --seed 4243`; `search:standard ref:strong --pair green-white-link:red-green-zeon --games 4 --seed 4244`; `random ref:baseline-standard --decks tests --games 8 --seed 4245`; `reference-benchmark red-purple-control blue-green-ramp --matches 3 --format bo3 --seed 4246 --reference-preset strong`, each at `--workers 1`, `4` and `10` | all identical: `diff -r -x timing.json` of the raw report directories is empty (no masking needed), `cmp` of the head-to-head reports succeeds; all 7 saved replays verify with `gcg-sim replay` |
| C12 | `gcg-diff weaknesses --check docs/port/WEAKNESSES.md --require-resolved` | 8 problems at `7b7ab2f`; 7 at `a0e065a` (W-04, W-18, W-19, W-120, W-126, W-137, W-154) |
| C13 | At `a0e065a`: `gcg-diff tests-json --check`, `weaknesses --check`, `traceability --check`, `coverage --check`, `lints`; `python3 scripts/doc_counts.py --check`; `gcg-eval evidence check` | all pass |
| C14 | `gcg-sim data status` | 1152/1152 card numbers implemented (every type 100%) |
| C15 | Python vs Rust `gcg-sim` help texts (`COLUMNS=100`, every command) and 17 error paths (exit code, stdout, stderr) | identical except the `--ai-preset`/`--workers` lines of `benchmark` and the CPython `JSONDecodeError` text (EQUIVALENCE D1); `data status --json` differs only in `code_digest` and `results_schema` |
| C16 | `gcg-eval head-to-head random random` under `--role tuning\|dev\|confirm` with held-out pairs (panel all, panel held-out, eval all, `--pair` by path and by name), and with masters outside the role's range | refused, exit 1, with the PLAN §12.5 / A14 messages; but see R-10 |
| C17 | `GCG_GATE_WORKERS=2 scripts/ci_local.sh` (T-156) in a clean clone of `a0e065a` | **exit 0**, "ci_local passed": golden-platform libm vectors pass, fmt and both clippy runs clean, 1340 tests passed (0 failed, 0 ignored), tier-1 corpus 3804/3804, lints, traceability and tests.json checks passed |

## 2. Findings

Severity: **blocker** fails an acceptance criterion outright; **major** leaves an acceptance
criterion unproven or a deliverable incomplete; **minor** is an inaccuracy or gap that does not
change an acceptance result.

| ID | Severity | Area | Finding (evidence) | Required fix |
| --- | --- | --- | --- | --- |
| R-01 | **blocker** | strength (T-008, T-127) | E4 (`standard` against greedy, bar: Wilson lower bound above 0.764) fails on the review's fresh seeds, as in both lead evaluations. AI test decks: review 305/400 = 0.7625 [0.7184, 0.8016] (master 91364052, `docs/port/eval/review/e4-tests-standard-vs-greedy.json`); final 1 307/400 [0.7237, ...]; final 2 304/400 [0.7158, 0.7993]. At 400 games the bar needs at least 323/400 (0.8075); three runs on different seeds put the shipped `standard` at about 0.76 on these decks. Panel: review 8759/11200, LB 0.7743 (passes), but held-out 4024/5200, LB 0.7623 (fails A26's held-out reading; final 2's held-out LB was 0.767). `gcg-eval gates --check docs/port/eval/review` exits 1 ("gates not passed: E3, E4"). The other strength gates pass on the review seeds: E1 0.5708 [0.5616], held-out [0.5528]; E2 0.5679, held-out 0.5665 (§4) | Either strengthen `standard` against greedy-like play on tuning evidence only and re-run E1-E4 on fresh eval and review seeds, or report E4 as a failed acceptance criterion to the user with these three measurements |
| R-02 | major | weaknesses (T-007) | 7 WEAKNESSES items are open at `a0e065a` (C12): W-04 and W-19 (final evaluation 2 pending; the review's E1, LB 0.5616 on fresh seeds, meets W-04's bar, while W-19 stays open because E4 fails, R-01), W-18 (fidelity verification not met: `fast` vs `standard` on `blue-white-federation-weakened` -0.075 [-0.146, -0.003], `docs/port/eval/final-2/fidelity-fast-standard.json`), W-126 (idle-machine throughput pair not run), W-137 (Linux run needs the push), W-154 (Python gate on a clean clone: C2 passed at `7b7ab2f`, so the lead can close it with that evidence), W-120 (the user's decision). The acceptance criterion allows only multiplayer, sideboarding and clock as exceptions | Close each or record it as a failed criterion in the final report; W-120 and W-137's push need the user |
| R-03 | major | weaknesses, docs | 30 items are `resolved by measurement (not adopted)`: the mechanism was built and measured null or negative, and the weakness remains in the shipped presets. `docs/AI.md:372-377` ("Known weaknesses") calls W-05, W-14, W-17, W-25 and W-36 open AI items, while the register calls them resolved (W-05, W-14, W-36 "resolved (not adopted)", W-17 and W-25 "resolved"). The brief's objective 5 says the port "resolves every known weakness"; its step 22 keeps only non-negative changes. Both readings cannot be presented at once | Make the two documents agree, and state in WEAKNESSES.md and the final report that a "resolved by measurement" item is a weakness that persists in the shipped AI with its measured attempts recorded; the user decides whether that meets the criterion |
| R-04 | minor (was major; evidence now exists) | equivalence (T-002, Phase 4 matrix) | The 324/324 matrix of `docs/port/EQUIVALENCE.md` ran on binaries built from `6daf17f`, before Phase 5; the 100-match headline on `e14c839`. Since then the engine and the reference path changed (Phase 5 optimisations, M13-M15b, the W-101 counters of `93cc30d` in `core.rs` and `interp/*`). The recorded outputs in `build/comparison/equivalence/` are all dated 2026-11-02. The current code is covered by the corpus and the seed-1 smoke only | Reviewer-verified at `7b7ab2f`: the fresh-seed run (§5) is 129/129 identical over 43 configurations. Remaining: EQUIVALENCE.md must say that the 324/324 matrix predates Phase 5 and cite this run (and any full re-run) as the evidence for the final code; then this finding is resolved |
| R-05 | major | docs (T-139), README | The measured strength of the shipped presets is missing: `README.md:279` and `docs/AI.md:349` hold placeholder comments (`<!-- lead: final evaluation numbers -->`, `<!-- lead: final evaluation 2 table -->`), and `docs/AI.md` shows only evaluation 1, which measured the M15 presets, not the shipped M15b ones | Fill both from final evaluation 2 and the review's E1-E4 (§4, `docs/port/eval/review/gates.json`), including the E4 failure |
| R-06 | minor | docs | `README.md:168-169` and `.claude/skills/gcg-benchmark/SKILL.md:53-55` give 76.9 s (`fast`, 100 matches) and 133.7 s (`standard`, 20 matches) as the measured cost, citing BENCHMARKS Part 2. Those are the M10 numbers of the M15 presets (commit 34b3107); Part 2 of the shipped presets (8c89b5e) is 82.1 s and 148.6 s | Update both to the Part 2 numbers of the binary that ships (after round 2) |
| R-07 | minor | evidence | W-11 cites `p6d_reuse::the_flag_without_the_carry_is_refused` and says "All of the above pass" (`docs/port/WEAKNESSES.md:546`, also `docs/port/eval/weakness-status/P6-D.md:214`). No such test exists; `git log -S the_flag_without_the_carry` finds it only in those two documents, never in `crates/` | Remove the citation or add the test |
| R-08 | minor | docs | `docs/AI.md:249` is a blank line inside the ablation table, so the `kill_aware_targeting` and `tree_lethal_select` rows (`:250-251`) render as plain text, not as table rows | Remove the blank line |
| R-09 | minor | build, shipping | Cargo feature unification: `gcg-diff` depends on `gcg-engine` with `features = ["testkit"]`, so every build that names `-p gcg-diff` with `gcg-cli`/`gcg-eval` (CLAUDE.md, `verify_all_rust.sh`, `ci_local.sh`, `rust.yml`, and `scripts/build_dist.sh:33` for the distributable binaries) compiles `gcg-sim` and `gcg-eval` with the test-only `testkit`. `compare_python_vs_rust.sh` builds `-p gcg-cli -p gcg-eval` only (observed: it rebuilt `gcg-engine` and every crate above it in the clone), so the timed binaries are not the ones the gate tests. `build_dist.sh` also ships `gcg-diff`, which links `gcg-ai-reference`, into `target/dist/`, while PLAN A31 and `docs/port/ARCHITECTURE.md:27` say `gcg-diff` is never shipped. The gate's feature check (`cargo tree -p gcg-cli -e features \| grep paranoid`) cannot see unification | Build the shipped binaries in a cargo invocation without `gcg-diff`; keep `gcg-diff` out of `target/dist/` or correct A31; check the dist binaries' feature set |
| R-10 | minor | evaluation hygiene (A14) | The held-out guard compares deck names only: a byte-identical copy of `examples/decks/eval/blue-red-tempo.txt` under another file name played under `--role dev --seed 20000003` (exit 0, recorded in the scratch ledger). `panel.json` holds each deck's digest. No misuse was found: no tracked deck file other than the archetype prior copies equals a held-out list, and no ledger entry of a tuning, dev or confirm role names a held-out deck (§3 c) | Compare the deck lists (or digests) against the held-out decks |
| R-11 | minor | E3 documentation (T-144) | `gcg-eval gates` counts a loss as documented when its replay path is a file (`crates/gcg-eval/src/gates.rs:333-337`); it does not check that the replay verifies or belongs to that game. The brief asks for surprising results to be saved and inspected; T-144 (inspection notes, `docs/port/eval/notes.md`) is not started, and the 27 final-1 loss replays that progress.md says verify 27/27 are not committed. For the review's 19 E3 losses the reviewer verified every replay and recorded an inspection (§4) | Verify the replay (or the `head-to-head-replay` index) in the gate; commit `docs/port/eval/review/e3-panel-losses/` and the loss replays of the final evaluation, with an inspection note |
| R-12 | minor | plan and tooling accuracy | PLAN §13 M16 (`docs/port/PLAN.md:3534`) and `:1276` cite `crates/gcg-ai-reference/src/config.rs`, which does not exist (the constants are in `crates/gcg-ai-reference/src/contracts.rs:206-400`), and `:1276` says the reference constants exist only there, but `crates/gcg-search/src/presets.rs` repeats the mapped budgets `(128, 320, 1280)` and `(64, 160, 640)` in `mapped_reference`. `scripts/compare_python_vs_rust.sh equivalence --help` exits 2 (`unknown equivalence option --help`); the usage is only in the header comment. BENCHMARKS' machine table gives uv 0.13.6; the final timing session's `env.txt` records uv 0.13.7 | Correct the paths and the "only" claim; add `--help`; record both uv versions |
| R-13 | minor | performance (brief step 18; T-104, T-148) | Zero heap allocation on the search path is not reached: about 150 allocations per MCTS iteration (`docs/port/BENCHMARKS.md`, "Where the time goes"); T-104 and T-148 are `failing` | List it as a known limitation in the final report |
| R-14 | minor | `unsafe` | The single `unsafe` block (`crates/gcg-core/src/libm.rs:11-19`, the `erfc` FFI) has a `// SAFETY:` argument, the workspace denies `unsafe_code` everywhere else (`Cargo.toml:48`, all 11 crates use `[lints] workspace = true`), and the libm vectors test it (`crates/gcg-core/tests/vectors.rs:202-204`). It exists for byte equivalence, not for a profiled win as the brief's constraint states, and no Miri run is recorded (an FFI call cannot run under Miri) | Record the deviation (PLAN A2) and why Miri does not apply in the final report |
| R-15 | minor | W-17 evidence | W-17 is resolved on a reduced audit: 12 of the 120 games were fully read (the plan read all 120), the comparison is with reference `standard` rather than `strong` (the preset `standard` starts from), and all 6 fully read games of the shipped `standard` still contain at least one confirmed implausible play (`docs/port/eval/final-2/audit/README.md`) | State the deviation in the final report; W-17 is better described as measured than as fixed |
| R-16 | minor | timing provenance | The Part 1/Part 2 timing and final evaluation 2 used binaries of `8c89b5e`. HEAD adds per-thread counter increments in engine hot paths (`93cc30d`: `crates/gcg-engine/src/core.rs`, `interp/lasting.rs`, `interp/choose.rs`, `interp/mod.rs`, `interp/deploy.rs`, `interp/play.rs`) and the `head-to-head-replay` tool | Round 2 times the binaries of the commit that is signed off |
| R-17 | minor | CI (T-133) | `.github/workflows/rust.yml` (arm64 `macos-15`, no timing job) has never run: the push needs the user's approval. `scripts/ci_local.sh` (T-156), the same steps run locally, passes at `a0e065a` (C17) | Ask for the push, or record T-133 as not met; T-156 can be set passing from C17 |
| R-18 | minor | gate coverage | Tests behind a cargo feature outside `slow`, `paranoid` and `testkit` are never built by any gate: `#[cfg(feature = "zstd")]` (3 sites, e.g. W-99's `w99_zstd_output_decompresses_to_the_same_bytes`, `crates/gcg-eval/tests/p6m_cli.rs:574-576`) and the `--no-default-features` (system allocator) build. `scripts/cargo_test_features.py` only refuses `[[test]]`/`[[bin]]` targets with other required features; it does not see in-file `cfg(feature)` gates. The reviewer ran it by hand: `cargo test -p gcg-eval --release --locked --offline --features zstd --target-dir target/zstd --test p6m_cli` passed 9/9, `w99_zstd_output_decompresses_to_the_same_bytes` included | Add a `--features zstd` test run to the gate (and a `--no-default-features` build), or extend the helper to find in-file feature gates |

## 3. Reading checks

**(a) Reference budgets and parameters.** `src/gcg_sim/ai/config.py:80-111`, `ai/baseline.py:18-73`
and `ai/tuning.py:50-57` were read next to `crates/gcg-ai-reference/src/contracts.rs:40-430`.
Every field of `fast`, `standard`, `strong`, `BASELINE_PRESETS["standard"]`, `["strong"]` and
`TUNING_CONFIG` is equal, including `_adopted`'s eleven overrides and the defaults of
`ReferenceConfig::new` (exploration 0.3, prior weight 0.5, prior temperature 1.5, horizon 2,
rollout temperature 0.5, value scale 2.0, 250 rollout decisions, 24 first-player games, 96
mulligan samples, burst score 1.0, target extension 1.0 / z 2.0, all booleans false).
`WIN_DECAY` 0.01, `LEAF_FLOOR` 0.05, `TIE_TOLERANCE` 1e-9, `budget()` and the post-init
horizon check match. The other module constants match too: `TUNED_WEIGHTS`, `ROLE_WEIGHTS`
(`crates/gcg-ai/src/contracts.rs:459-478` against `ai/evaluation.py:82-129`), the policy
constants (`EX_PENALTY`, `LETHAL`, `CLEAR_THE_WAY`, `MUST_BLOCK`, `KEEPABLE_TOP`,
`IDLE_ATTACK`), the evaluation constants, `GREEDY_MULLIGAN_SAMPLES` 48,
`GREEDY_MAX_DECISIONS` 200, noop `MAX_DECISIONS` 400, setup `OPENING_TURNS` 4 / `KEEP_RATIO`
0.9, `LOG_ALTERNATIVES` 5, `DECISION_LIMIT` 20000, `MIRROR_BLOCK` 4. The test
`presets_equal_the_live_python_objects` pins the presets against the live Python objects
(`float.hex`). Result: equal. Only the cited file path is wrong (R-12).

**(b) `gcg-sim` presets and reachability.** C6, C7, C8 and C15. `crates/gcg-cli/Cargo.toml`
depends on `gcg-core`, `gcg-data`, `gcg-effects`, `gcg-ai`, `gcg-runner`, `gcg-search` only;
`crates/gcg-cli/src/factory.rs` builds agents only through
`gcg_search::make_agent(preset_config(preset))`; `Preset` has two variants
(`crates/gcg-search/src/contracts.rs:80-89`, default `Standard`). No environment variable
reaches the AI (the one `std::env::var_os` in `gcg-cli` reads `COLUMNS`). The strengthened
search's null configuration, which replays the reference, is linked into `gcg-sim` but has no
command-line path. `search:standard` and `search:fast` in `gcg-eval` are
`gcg_search::preset_config(p).clone()` (`crates/gcg-eval/src/agent_spec.rs:458`), so the
strength runs measure the shipped configuration. The shipped presets use
`opponent_list: Known` (`crates/gcg-search/src/presets.rs:50`), the reference's information:
the archetype prior, which embeds all eight panel decks including the two held-out ones
(`crates/gcg-search/decks/prior-*.txt`), is not enabled. Result: pass.

**(c) Seed and pair disjointness.** All 20 ledgers under `docs/port/eval/` (384 entries) were
aggregated: every master lies in its role's range; no master is used under two roles; no
`tuning`, `dev` or `confirm` entry plays `--decks panel|eval` with `--pairs` other than
`tuning`, and none names a held-out deck in any configuration field. `grep -rlE
'blue-red-tempo|green-white-link' docs/port/eval` finds, besides `panel.json` and three
status documents that describe the split, only final-evaluation files (final 1's four panel
reports and `gates.json`, final 2's E1 and E2); none of the other 440 JSON files names a
held-out deck. Eval masters used: 90200001-90200006 (final 1) and 90300001-90300007 (final 2
and its fidelity run); review masters: the six of §4, recorded in
`docs/port/eval/review/seed-ledger.json`. Re-run after the review runs over all 21 ledgers:
still 0 problems. The held-out
decks were played before tuning in the deck sanity runs of
`docs/port/eval/weakness-status/P6-decks.md:99-125` (`ref:fast` and `greedy` self-play, seed 7,
no role), where the ramp deck's list was revised against the seven other decks; that shaped a
deck, not the AI. Result: pass, with R-10.

**(d) `unsafe`.** C10 and R-14. Result: pass (with the recorded deviation).

**(e) WEAKNESSES.md.** At `a0e065a`: 124 resolved, 30 resolved by measurement (not adopted),
3 admissible exceptions (W-131 multiplayer, W-132 sideboarding, W-133 clock), 7 open (R-02).
Spot checks:

- Citations: a script resolved the 526 file paths cited in the Status, Evidence and Ablation
  fields of all 164 entries. Every one resolves (a few relative ones by hand, e.g.
  `auditors/auditor-1.json` under `docs/port/eval/final-2/audit/` and `ai/selfplay.py` under
  `src/gcg_sim/`), except names of output files (`analysis.json`, `diagnostics.json`). Of the
  505 snake_case identifiers cited, every test name exists in the code except
  `the_flag_without_the_carry_is_refused` (R-07); the others not found as functions are
  `MechanismFlags` field names or JSON keys.
- Numbers: 39 ablation rows across 28 items (W-01-W-08, W-11, W-14-W-18, W-23, W-25-W-27,
  W-31, W-32, W-35, W-44, W-134, W-138-W-140, W-143, W-163) were compared with their cited JSON
  files: wins, N, paired difference and discordant counts all equal, and every Wilson interval
  recomputes from wins/N.
- Tests named by 19 resolved items (W-01, W-03, W-15, W-21, W-25, W-26, W-32, W-44, W-46,
  W-57, W-99, W-103, W-110, W-112, W-129, W-143, W-150, W-152, W-160): of the 43 test
  functions they cite, 42 appear as `... ok` in the C1 log; the exception is W-99's
  `w99_zstd_output_decompresses_to_the_same_bytes`, which is `#[cfg(feature = "zstd")]`
  (`crates/gcg-eval/tests/p6m_cli.rs:574-576`) and is built by no gate (R-18). W-103, W-110
  and W-160 cite runs and files rather than test names; their files were opened (W-110's gate
  log; W-160's normaliser self-tests ran in C1, 13/13).
- Other evidence opened: W-57 (`docs/port/audit/regressions.json`; its `tuning --rounds 0` row
  is n/a because `gcg-eval tune` has no rounds option), W-110
  (`docs/port/eval/GAP-D/w110-strategic-fuzz.txt`, a verbatim gate log at 3f4aafb), W-17
  (`docs/port/eval/final-2/audit/`, five auditor files present), the M15b and final-1 records.
- Test expectation changes: the pinned values changed by `8c89b5e` (W-25 scenario: shipped
  `fast` over-extensions 16 → 0 with `tree_lethal_select`; portable-math keys; replay vectors)
  follow a behaviour change of the presets, and the assertions removed in `b8483dc` and
  `93cc30d` were replaced by stronger ones when a refused feature became available. No test
  was weakened to pass a gate in the diffs read.

**(f) Shortcuts.** C9: no `todo!`, `unimplemented!`, `#[ignore]`, benchmark target or
`criterion`; `unreachable!` appears only after argparse choice validation. No silent test skips
(no early returns on missing files or environment variables in `crates/*/tests`). No card is
skipped (C14; `gcg-diff coverage --check` passes). No card number or panel deck name appears in
non-test code of `gcg-search` or `gcg-ai` except the archetype prior list.

**(g) No timing in the crates.** The only clock is `crates/gcg-runner/src/clock.rs`
(`Instant`/`SystemTime` for `timing.json` and the `--resume` checkpoint's per-match seconds,
`crates/gcg-runner/src/checkpoint.rs:378-380`), read by nothing else. No `Duration`, timeout,
`recv_timeout`, `wait_timeout`, `park_timeout`, `getrusage` or `clock_gettime`; `spin_loop` and
`yield_now` appear only in the pool's join and in the SIGINT test helper; no QoS, thermal or
warm-up code. `gcg-diff lints` (rules 2 and 3) enforces it. Result: pass.

**(h) Worker identity.** C11 (the reviewer's own runs: the `standard` preset, `fast` against
greedy, `standard` against reference `strong`, random against `ref:baseline-standard`, and the
reference `strong` benchmark, all identical) plus the gate's runs (C1) and the 1/4/10-worker
Rust comparisons of §5. Result: pass.

**(i) Quantitative claims.** BENCHMARKS Part 1 and Part 2: every median, min and max was
recomputed from the hyperfine JSON files in the main tree's `build/comparison/`
(`py-*.json`, `rs-*.json`), and every Rust timing's `.digest` equals `env.txt`'s code digest
(`5240d597...`, commit `8c89b5e`), except the two superseded 10-worker full-search files that
are not in the tables. The README speed paragraph equals BENCHMARKS. EQUIVALENCE's 324/324:
`build/comparison/equivalence/` holds 109 configurations and 327 `diff-w*.txt` files, all empty
(R-04 for their age). docs/AI.md's budget-curve and ablation numbers match their JSON files
(see (e)). Stale or missing: R-05, R-06.

**(j) Other observations (no fix asked of the lead).**

- The brief's mandated wording for `standard`, "plays at the level of the strongest top-tier
  competitive player", is used in `--help` and the docs as the brief requires. No measurement in
  the repository supports it as a claim: strength is measured only against the reference
  search, random and greedy. The brief also says "never claim strength you have not measured";
  the user may want to settle which instruction wins.
- The C1 coverage report names one Python-side test gap: GD04-089's tagged Python test never
  runs its program; the card is executed only in the census games or fuzz. The Python tests stay
  untouched, so this is for the user.
- C1 printed two stale-artefact warnings for Python-side research files
  (`docs/research/DATA_INGESTION.md`, `docs/research/data_profile.json`), already listed as
  W-115/W-128 follow-ups.

## 4. Strength review (T-008)

E1-E4 exactly as the final evaluation runs them, with `--role review`, `--choose-first seat0`,
`--ledger docs/port/eval/review/seed-ledger.json` and `--out docs/port/eval/review/<name>.json`
(main tree), using the clean clone's binaries of `7b7ab2f`:

| Report | Command | Master | Status |
| --- | --- | --- | --- |
| E2 | `head-to-head search:fast ref:standard --decks panel --pairs all --games 11200` | 91582093 | **done** (08:12-11:16 UTC, 4 workers): 6361/11200 = 0.5679 [0.5588, 0.5771]; held-out 2946/5200 = 0.5665 [0.5530, 0.5800]; no pair with an upper bound below 0.5 (lowest: red-green-zeon:green-white-link, held out, 209/400 [0.4736, 0.5710]); 0 draws, 0 unfinished; paired over 5600 deals +0.1359 (challenger-only 1227, opponent-only 466). Passes the brief's bar and A26 |
| E1 | `head-to-head search:standard ref:strong --decks panel --pairs all --games 11200` | 91437261 | **done** (11:16-23:19 UTC, 4 workers): 6393/11200 = 0.5708 [0.5616, 0.5799]; held-out 2945/5200 = 0.5663 [0.5528, 0.5798]; no held-out pair (and no pair at all) with an upper bound below 0.5 (lowest: blue-green-ramp:red-purple-control, tuning pair, 206/400); 0 draws, 0 unfinished. **Passes** the brief's bar and A26's held-out reading |
| E3 tests | `head-to-head search:standard random --decks tests --games 400` | 91716648 | **done**: 400/400. **Passes** |
| E4 tests | `head-to-head search:standard greedy --decks tests --games 400` | 91364052 | **done**: 305/400 = 0.7625 [0.7184, 0.8016]. **Fails**: the bar is a lower bound above 0.764 |
| E4 panel | `head-to-head search:standard greedy --decks panel --pairs all --games 11200` | 91950783 | **done**: 8759/11200 = 0.7821 [0.7743, 0.7896] (passes); held-out 4024/5200 = 0.7738 [0.7623, 0.7850] (**fails** A26's held-out reading: 0.7623 is not above 0.764) |
| E3 panel | `head-to-head search:standard random --decks panel --pairs all --games 11200` | 91829417 | **done**: 11181/11200 = 0.9983; 19 losses in 11 pairs, 11 of them in held-out pairs (5189/5200); 0 draws, 0 unfinished. **Passes** with every loss documented and its replay verified (below) |
| gates | `gcg-eval gates --e1 ... --e4-panel ... --losses-documented docs/port/eval/review/e3-panel-losses/losses-documented.json --out docs/port/eval/review/gates.json`, then `--check` | - | **done**. First run (07:16 UTC, without the loss file): `--check` exit 1, "gates not passed: E3, E4". Final run with the 19 documented losses: E1 pass, E2 pass, E3 pass, **E4 fail** (test decks LB 0.7184; held-out LB 0.7623); `--check` exit 1, "gates not passed: E4". `gates.json` holds the final run |

The reviewer recomputes each result from the report's per-game `outcomes` rows (winner against
challenger seat), not from its summary: for E2 the rows give the same 6361/11200 and
2946/5200, 400 games in each of the 28 pairs, and the challenger on the play in exactly 5600
games. The same holds for every other report (all six: the rows reproduce wins and N, 400 games
per pair, 0 draws, 0 unfinished).

**E3 losses.** `gcg-eval head-to-head-replay docs/port/eval/review/e3-panel-standard-vs-random.json
--losses --out docs/port/eval/review/e3-panel-losses` re-played all 19 losses and each one
reproduced its row (winner, turns, first player, decisions). All 19 replay files verify with
`gcg-sim replay` (19/19, "replay matches the recorded outcome and final state").
`docs/port/eval/review/e3-panel-losses/losses-documented.json` lists them (repository-relative
paths). With it, `gcg-eval gates ... --losses-documented
docs/port/eval/review/e3-panel-losses/losses-documented.json --out
docs/port/eval/review/gates.json`, run from the main tree, gives E1 pass, E2 pass, E3 pass, E4
fail; `--check` exits 1 with "gates not passed: E4".

Inspection: every loss ends by battle damage, after 16 to 31 turns; `standard` was on the play in
7 of the 19. In 13 of them random played `red-purple-control`, the strongest deck of the panel
in the self-play sanity runs (0.67-0.72, `docs/port/eval/weakness-status/P6-decks.md:113-123`).
`standard` played the held-out `blue-red-tempo` in 7 and `green-white-link` in 4. The shortest
loss, game 6006 (`standard` with `green-white-link` against random `red-green-zeon`, 16 turns),
was read in full with `gcg-sim replay --log`. `standard` passes turns 1, 3 and 7 without a play
while the random side curves out with Zakus and two Close Combats, and its Burst triggers come
too late. Nothing in the log contradicts a rule. The log does not show hands, so it cannot tell a
dead draw from a passive mistake. The losses read as variance against an aggressive random
curve, not as an engine defect.

Result of the review's strength protocol (T-008): **not met**. E1, E2 and E3 meet their bars on
fresh review seeds (E1 also under A26's held-out reading); E4 fails on the AI test decks and on
the held-out pairs (R-01).

## 5. Equivalence on fresh seeds

The planned matrix (3 seeds x 3 pairs x bo1/bo3 x fast/standard/strong x decision log off/on at
`--matches 10`) is 216 Python runs; the same matrix took 19 h on this machine before. On a
machine shared with the lead's evaluation it would far exceed the ~6 h budget, so the reviewer
runs, with `COMPARE_OUT` under the review directory:

1. `--matches 10 --seeds "91208815 91671342 91093576" --formats "bo1 bo3" --presets "fast
   standard strong" --workers "1 4 10" --pairs "red-green-zeon:blue-white-federation
   purple-white-tekkadan:red-green-zeon" --decision-logs off` (36 configurations, 72 Python
   runs, 108 Rust comparisons);
2. `--matches 10 --seeds 91671342 --formats "bo1 bo3" --presets "fast standard strong" --workers
   "1 4 10" --pairs blue-white-federation:purple-white-tekkadan --decision-logs on` (the third
   pairing and the decision log: 6 configurations, 18 comparisons);
3. the 100-match headline on a fresh seed: `--matches 100 --seeds 91208815 --formats bo3
   --presets standard --pairs red-green-zeon:blue-white-federation --workers "1 4 10"
   --decision-logs off` (3 comparisons).

Results (Rust binaries of `7b7ab2f`, built by the script's `need_rust`; Python at `d9d41af`
from the clone's `uv sync --locked`, Python 3.15.1):

| Run | Configurations | Python baselines (hash seeds 1 = 2) | Rust comparisons identical |
| --- | ---: | ---: | ---: |
| 1. main matrix, decision log off | 36 | 36/36 | 108/108 (fast 36, standard 36, strong 36) |
| 2. third pairing, decision log on | 6 | 6/6 | 18/18 (fast 6, standard 6, strong 6) |
| 3. headline, 100 matches | 1 | 1/1 | 3/3 |
| **total** | **43** | **43/43** | **129/129** |

Every one of the 129 `diff-w{1,4,10}.txt` files is empty. In the raw (unmasked) reports the
only differences are the three masked fields: `ai.factory` (`gcg_sim.ai.make_agent('mcts')`
against `gcg-ai-reference::make_agent('mcts')`), `versions.code_digest`, and their rendering in
`summary.md`; `timing.json` is excluded. The compared trees hold `results.json`,
`games.ndjson`, `summary.md` and every saved replay. So the reference search at HEAD is byte
for byte Python's on three fresh seeds, all three example pairings, both formats, all three
reference presets, both decision-log settings and 1, 4 and 10 workers. The 500 saved replays
of the Python `python-h1` and Rust `rust-w1` runs all verify with the Rust `gcg-sim replay`
(500 ok, 0 failed). Not covered in this
round: decision log on for the first two pairings, and seeds x pairs beyond the 43
configurations (the original matrix had 108).

## 6. Not yet verified

- Round 2: `scripts/compare_python_vs_rust.sh env`, `best-workers rust-reference`,
  `best-workers rust-presets`, `part1`, `part2` and `report` on an idle machine, against the
  recorded Python baselines (T-011), with the binaries of the commit to be signed off (R-16).
  Not run in round 1: the machine was loaded.
- The full 108-configuration equivalence matrix on the final code (round 1 ran 43, §5).
- The CI workflow on GitHub (R-17; needs the user's approval of the push).

## 7. Sign-off

Not signed off. Open: R-01 (blocker), R-02, R-03 and R-05 (major), the minor findings R-04 and R-06 to R-18, and round 2.
