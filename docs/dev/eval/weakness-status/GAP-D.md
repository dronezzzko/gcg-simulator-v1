# GAP-D weakness status: W-110, W-118, W-126, W-129, W-154, the M15 re-verifications and the tests.json list

Package GAP-D closes residual items of `docs/dev/eval/weakness-gaps.md`: W-110 (§6), W-118 (§6),
W-126 (§6), W-129 (§6), W-154 (§6), the re-verification of W-150 and W-152 on the M15 presets
(§8), and the `docs/dev/tests.json` bookkeeping list (§10). Written on 2026-11-02/03 in worktree
branch `worktree-wf_9f38bb2b-c03-5`, from commit 1007ca9. WEAKNESSES.md and tests.json are not
edited here; the lead records the status lines from this file.

Commits on the branch:

- 11b1e75: the tools, the gate lines and the doc markers;
- 3f4aafb: the gate's worker caps;
- 54c2bd2: the system-`python3` fix of the normaliser; the official gate run is at this commit;
- the commit that adds this record and `docs/dev/eval/GAP-D/` (records only).

The machine was shared with the final evaluation and other agents throughout (load averages
7-42 on 10 cores). Every game run here used at most 4 workers: `GCG_GATE_WORKERS=4`,
`RUST_TEST_THREADS=4`, `CARGO_BUILD_JOBS=4`. No duration in this file is a measurement.

No AI mechanism was added or changed, the shipped presets are untouched, and no source file of
the engine, the effects, the AI crates, the runner or the CLIs changed. The changed code is in
`crates/gcg-diff` (a gate tool, never linked into `gcg-sim` or `gcg-eval`), `scripts/` and one
formatting-only edit of `oracle/export_cli.py`. No role seeds were used (the fuzz seeds are
2026 and the commit-derived seed, outside every A14 range), so there is no seed-ledger file.

## W-110: strategic paranoid fuzz with the shipped presets

- **Status:** resolved.
- **Done:** no source change. The 1,000 strategic games of PLAN §9.4 ran in the paranoid build
  with the shipped M15 presets: 400 `greedy`, 400 `search:fast` and 200
  `mixed:greedy,search:fast`, at seed 2026, over Python's generated decks, with invariants
  after every action. They are the gate's own lines (`scripts/verify_all_rust.sh`), run on a
  clean clone of the branch at 3f4aafb. That tree has 1007ca9's engine, effects, AI, presets
  and fuzz sources. The gate's only change to these lines is `--workers "$rust_workers"` (4
  here) in place of `--workers auto`. The fuzz outputs do not depend on the worker count.
- **Evidence:** `docs/dev/eval/GAP-D/w110-strategic-fuzz.txt`, the gate's output verbatim.

  | Agent | Build | Games | Actions | Failures | Ends |
  | --- | --- | ---: | ---: | ---: | --- |
  | `greedy` | paranoid | 400 | 31,936 | 0 | 400 battle damage |
  | `search:fast` (shipped M15 `fast`) | paranoid | 400 | 29,781 | 0 | 400 battle damage |
  | `mixed:greedy,search:fast` | paranoid | 200 | 14,650 | 0 | 200 battle damage |
  | random, `--compare` Python's summary | release | 10,000 | 856,034 | 0 | equal to Python (T-055) |
  | random, commit-derived seed 5332448476079853097 | release | 2,000 | 170,998 | 0 | 2,000 battle damage |

  - Every strategic game covers all 1,064 pool cards.
  - Game 221 of the `search:fast` run completes: it reached the W-143 redirect cycle in Phase
    4, and `engine_error_leaf` is now in the shipped presets.
  - `paranoid-games` passed in the same run: 16 fuzz games, 2 greedy games, and every check
    above its floor (cached view 105,589 runs, skipped fixpoint pass 105,974).
  - The official gate run at 54c2bd2 reproduced the three strategic counts exactly: 31,936,
    29,781 and 14,650 actions, 0 failures. It also matched the 10,000-game comparison, and its
    commit seed 1585305945398772890 gave 2,000 games, 170,257 actions, 0 failures.
  - T-056 is satisfied by both parts: the 2,000 commit-seed games and the 1,000 strategic
    paranoid games.
- **Ablation:** no AI effect (row N): gate runs, with no source change.

## W-118: the Rust gate on a clean clone

- **Status:** resolved. `scripts/verify_all_rust.sh --full` passed in one run on a clean clone
  of the branch at 54c2bd2: it printed "verify_all_rust passed" and exited 0 ("The official
  clean-clone run" below). That is the verification's "passes on the clean branch" and T-001.
  The "fails when any step fails" half was already resolved by the self-test. The Python gate's
  own limits stay unchanged by design, as P6-N listed: bash 3.2 `set -u` with the EXIT trap,
  and the wheel picked by `ls | tail`.
- **Done** (`scripts/verify_all_rust.sh`; its self-test and `p6n_tooling.rs::w112_*` and
  `w122_*` still pass):
  - Lines the reports proposed that the script lacked:
    - the A30/T-153 identity (P6-M open item 7), until now commented out "from M12.M":
      `gcg-eval benchmark` with no opt-in option against `gcg-sim benchmark` at 4 workers,
      `diff -r -x timing.json`;
    - `target/release/gcg-diff api-snapshot --check` (W-129);
    - `python3 scripts/throughput.py --selftest` (W-126).
  - Lines the reports proposed that the script already had, checked one by one:
    - fuzz on the real engine (10,000 with `--compare`, 2,000 at the commit seed), and the
      paranoid build (tier-1 corpus `--paranoid`, `paranoid-games`, the three W-110 runs, the
      `gcg-engine` paranoid and testkit tests);
    - the three `schema-check` lines;
    - `oracle/eval_decks.py --check`, and P6-K's `p6k_vectors.py`, `p6k_audit.py`,
      `templates/check.py` and `pytest oracle/tests` (`--full`);
    - `gcg-eval evidence check`, `traceability --check`, `coverage --check`;
    - the slow tier: `$SLOW_OFFLINE`, and `$SLOW_TIER2` after the tier-2 regeneration.
  - `--full` runs the documented commands (`check_doc_commands.py`) in their own clone of HEAD
    with `--timeout 21600`. A regeneration they document cannot then change the tree under
    test, and the CLAUDE.md:8 block (3,851 s here) is not cut at the 1,800-s default. The
    tracked files they rewrite there must be exactly `docs/RULES_TRACEABILITY.md`, or the gate
    fails (lead note 1).
  - `GCG_GATE_WORKERS=N` caps the corpus jobs, the fuzz workers and the Python trace and census
    workers on a shared machine. It must be a positive integer, or the gate exits 2. Unset, the
    defaults are unchanged on this machine: corpus jobs = online CPUs = 10, fuzz `auto`, Python
    8. No output depends on it. The 1/4/10-worker identity runs keep their counts.
  - `scripts/normalize_outputs.py`: the gate runs it with the system `python3`, which is 3.9.6
    here. It used `dataclass(slots=True)`, which needs 3.10, and that stopped the first
    clean-clone run (below). Its self-test passes under 3.9.6 (13/13), and every stdlib helper
    the gate runs compiles under 3.9.6.
- **Evidence:**
  - `scripts/verify_all_rust_selftest.sh`: "verify_all_rust selftest passed". The controls are
    a failing step, a failing pipeline and an unset variable; no step is guarded by `|| true`.
    `GCG_GATE_WORKERS=x scripts/verify_all_rust.sh` exits 2.
  - First clean-clone run, `scripts/verify_all_rust.sh --full` at 3f4aafb with
    `GCG_GATE_WORKERS=4 RUST_TEST_THREADS=4 CARGO_BUILD_JOBS=4`. It passed every step from the
    clone through `gcg-eval evidence check` and the dependency-tree checks, then stopped at the
    first normaliser call: `TypeError: dataclass() got an unexpected keyword argument 'slots'`.
    - The log of its first 75 minutes was lost: another agent of this session overwrote the
      shared scratchpad log (lead note 2). Under `set -e` the run reached the paranoid engine
      tests only after fmt, both clippy runs, the builds, the default tier and the slow tier
      had exited 0.
    - The kept part shows:
      - the paranoid `gcg-engine` tests (131 unit tests plus the integration and doc tests);
      - `lints passed`;
      - `selftest`: 22 negative controls; `selftest --real-engine`: 36;
      - tier-1 corpus parse-only, mock, mock roundtrip, real engine, real-engine roundtrip and
        paranoid: 3804/3804 each, `search` 9/9 and `baselines` 8/8, 1,396 agent decisions
        equal;
      - `coverage check passed`, `traceability passed` (both documents current),
        `tests.json check passed`, `weaknesses check passed`, `api-snapshot check passed`;
      - the fuzz and paranoid runs of W-110;
      - `decks --overlap --check` and `evidence check` passing.
  - Pre-check of the rest, at 54c2bd2 (the fix). This is the gate's prologue (a clean clone of
    the branch), the feature lists and the release build, then every step from the CLI runs to
    the end, `--full` included. It printed "verify_all_rust passed" and exited 0 (13,638 s).
    It covered:
    - identical normalised outputs at 1/4/10 workers for `gcg-sim`, `reference-benchmark` and
      head-to-head;
    - the T-153 identity;
    - replays verify, and the three `schema-check` lines pass;
    - `--ai-preset strong|STANDARD|Fast|bogus` exit 2;
    - `offline binary check passed: gcg-sim (3 pairings, workers 1/4/10)` (T-145);
    - the stdlib checks, and A15 unchanged;
    - `--full`:
      - `ruff`, `ruff format` and `mypy --strict oracle` clean;
      - the documented commands: 19 run, 11 skipped, 0 failed (W-154);
      - `blind_transcripts` and `fuzz_census` self-tests; the fuzz census and the test census
        current;
      - tier 1 regenerated and equal under PYTHONHASHSEED 1, 2 and `--nocache`;
      - vectors, census, `eval_decks`, `p6k_vectors`, `p6k_audit` and `templates/check.py`
        current;
      - `pytest oracle/tests`: 25 passed (T-155);
      - comparison self-tests 15/15 and 14/14; normaliser 13/13;
      - equivalence smoke: 3/3 identical (fast, standard, strong; Python baselines identical
        under hash seeds 1 and 2);
      - tier 2 regenerated, corpus tier 2 at 4154/4154 (12,131 reference-search decisions);
      - the `$SLOW_TIER2` tests, including `tier2_views_match_python`.
  - The single official run of the whole gate: see "The official clean-clone run" below.
- **Ablation:** no AI effect (row N). The changes are gate scripts and one stdlib helper. No
  AI-executed path changes, and the `search` and `baselines` corpus sets are at 100% in every
  run above.

## W-126: throughput reproducibility

- **Status:** open, with one step left. The 3% criterion holds for the timing harness of
  record on the medians of the lead's two idle-machine sessions. Two things are missing. The
  minima of the second session are not in the repository. And no idle machine was available
  for the pair of `scripts/throughput.py` runs that the gap entry names, because the final
  evaluation shares the machine.
- **Done:**
  - `scripts/throughput.py --agree FIRST.json SECOND.json [--tolerance 0.03]` checks two `--json`
    reports of the same command. Per pairing, the minimum CPU seconds per game and the minimum
    wall time must agree within the tolerance, relative to the smaller value. The effort counts
    must be equal, and so must the command (agent, opponent, pairs, games, runs, workers, seed).
    It exits 1 otherwise.
  - `scripts/throughput.py --selftest` covers block rounding (1, 3, 4, 5, 400 → 4, 4, 4, 8,
    400), the summary minima, agreement at 2.5% and 2%, failure at 5%, failure on different
    effort counts, and failure on different commands. The gate runs it (stdlib section of
    `scripts/verify_all_rust.sh`).
- **Evidence (the lead's measurement, recorded here against the criterion):**
  `scripts/compare_python_vs_rust.sh part1`/`part2` (hyperfine) on the idle machine at commit
  34b3107, two sessions (docs/dev/BENCHMARKS.md has one session's median, minimum and maximum):

  | Configuration | Session 1 median (min, max) | Session 2 median | Difference of the medians |
  | --- | ---: | ---: | ---: |
  | `rs-ref-standard-m100-w10` | 47.9 s (47.8, 47.9) | 47.9 s | 0.00% |
  | `rs-fast-m100-w10` | 88.0 s (87.9, 88.3) | 88.0 s | 0.00% |
  | `rs-ref-standard-m20-w1` | 28.2 s (28.1, 28.2) | 28.3 s | 0.35% |
  | `rs-fast-m20-w1` | 52.7 s (52.6, 52.7) | 52.7 s | 0.00% |

  Every difference is under 3%. The criterion names the minimum, and only session 1's minima
  are recorded: within 0.3% of its medians.
- **To close:** on an idle machine, run
  `python3 scripts/throughput.py search:fast --opponent ref:fast --games 400 --runs 2 --workers 6 --json A.json`
  twice (A.json, then B.json), then `python3 scripts/throughput.py --agree A.json B.json`. Or
  the lead records that the two harness sessions above meet the criterion.
- **Tool check on this machine (loaded, so not evidence for the criterion):** two sessions of
  `python3 scripts/throughput.py search:fast --opponent ref:fast --games 4 --runs 2 --workers 1 --pairs red-green-zeon:blue-white-federation,blue-white-federation:purple-white-tekkadan --json F`,
  with load average 7.0 before them. `--agree` reported:
  - `red-green-zeon:blue-white-federation`: minimum CPU s/game 0.5008 against 0.4957 (1.04%),
    minimum wall 2.0065 s against 1.9850 s (1.08%);
  - `blue-white-federation:purple-white-tekkadan`: 0.7959 against 0.7929 (0.37%), 3.1861 s
    against 3.1752 s (0.34%);
  - "throughput runs agree", exit 0, with equal effort counts.

  The reports are `docs/dev/eval/GAP-D/throughput-A.json` and
  `docs/dev/eval/GAP-D/throughput-B.json`.
- **Ablation:** no AI effect (row N). The changes are in a script outside the crates, and no
  AI-executed path changes.

## W-129: checked interface docs

- **Status:** open: the public API snapshot is done. D14a, the Rust `docs/AI.md` and its test
  (T-130) are the lead's Phase 8 step.
- **Done:** `gcg-diff api-snapshot (--check | --write)` (`crates/gcg-diff/src/apisnapshot.rs`) is
  a stable-toolchain listing of the public API, read from the sources. Rustdoc's JSON output
  needs nightly, so it cannot be used.
  - It covers the eight library crates: the workspace members that another member's
    `[dependencies]` names. These are `gcg-core`, `gcg-data`, `gcg-effects`, `gcg-engine`,
    `gcg-ai`, `gcg-ai-reference`, `gcg-search` and `gcg-runner`. The binaries' crates are
    interfaces through their command lines, which `scripts/cli_help_snapshot.py` snapshots.
  - It lists every exactly-`pub` item outside `#[cfg(test)]` in every module the crate root
    reaches. Private modules count too, because the roots re-export most of their items with
    `pub use`. So the listing over-approximates the API and never misses a `pub` item.
  - Per item:
    - functions and methods: the signature;
    - structs: the `pub` fields, with `..` for private ones;
    - enums: every variant;
    - traits: every member;
    - constants and statics: name and type;
    - inherent impls: their `pub` items;
    - trait impls: the header;
    - `macro_rules!`: name and body digest, plus the item-level invocations of the crate's own
      macros, such as the `mechanism_flags!` field list.
  - It keeps the `derive`, `cfg`, `repr` and `non_exhaustive` attributes. Whitespace is
    normalised, so rustfmt's line breaking does not change the listing.
  - The listing is `docs/dev/api-snapshot.txt`: 4,780 lines, 125 module sections. The comment
    and literal lexer is shared with `gcg-diff lints` (`lints::strip_comments`; the lint
    behaviour is unchanged, and `tests/lints.rs` passes).
- **Evidence:**
  - `crates/gcg-diff/tests/api_snapshot.rs`:
    - `the_committed_api_snapshot_matches_the_sources`;
    - `the_listing_covers_exactly_the_library_crates`;
    - `a_changed_public_signature_fails_the_check_at_its_line`: a negative control on a copy
      of the sources. `Pool::threads` returning `usize` fails the check, with 1 line only
      committed and 1 only in the sources. A private signature change plus a body edit leaves
      the listing unchanged.
  - Nine unit tests in `apisnapshot::tests` (pub(crate) and test-only exclusion, line-break
    and where-clause invariance, fields, variants, trait members, impl filtering, macro
    digests, inline modules, an unrecognised item is an error).
  - The gate runs `target/release/gcg-diff api-snapshot --check` after `weaknesses --check`.
- **D14a (the lead's Phase 8 step, not done here):**
  - `git mv docs/AI.md docs/python/AI.md`. `docs/python/` does not exist yet.
  - Rewrite the `docs/AI.md:N` citations under `docs/dev/` in the same commit: 102 citations
    in 6 files, 94 of them in WEAKNESSES.md (counted at 3f4aafb).
  - Write the Rust `docs/AI.md` with the preset table of
    `docs/dev/eval/M15/preset-table.md`, and add T-130's test.
  - Reword `docs/python/AI.md:326-327` to "two deals of two games".

  It edits WEAKNESSES.md (a lead file) and `docs/AI.md`, both outside GAP-D's files.
- **Merge note:** every branch that changes a `pub` signature in the eight crates must run
  `target/release/gcg-diff api-snapshot --write`. After merging parallel branches, regenerate
  the listing once and review the diff. A branch that adds a `MechanismFlags` field changes the
  `mechanism_flags!` lines.
- **Ablation:** no AI effect (row N). The change is a gate tool in `gcg-diff` and a generated
  text file. No AI-executed path changes, and the corpus `search` and `baselines` sets are at
  100% (W-118 above).
- **tests.json:** no id names the API snapshot; T-130 stays the lead's.

## W-154: the Python gates with the port committed

- **Status:** open: only T-135 remains, the clean-clone run of the Python gate
  `scripts/verify_all.sh` with the port committed. The gap list makes it the lead's M16 step.
  The rest of the verification passes: `ruff check`, `ruff format --check` and the documented
  commands of the whole repository, on a clean clone of the branch.
- **Done:**
  - A `<!-- doc-check: skip <reason> -->` marker on each of the eight fenced blocks of the nine
    listed. The ninth is P6-N's inline mention, handled below. The fences stay `bash`, and no
    number in those records changed. New marker lines:
    - `docs/dev/eval/M15/README.md` 44, 94 and 135;
    - `docs/dev/eval/P6-G/README.md` 39 and 93;
    - `docs/dev/eval/weakness-status/P6-A.md` 64;
    - `docs/dev/eval/weakness-status/P6-F.md` 55;
    - `docs/dev/eval/weakness-status/P6-decks.md` 106.

    The P6-N line (now 369-371) is reworded to "bash-fenced block", so it no longer starts with
    a fence. The checker reads a reason only up to the first `>`, so the reasons name
    placeholders without angle brackets.
  - `oracle/export_cli.py`: `uv run ruff format` wrapped one long string (formatting only;
    `export_cli.py --check` still exits 0). It was the one file that failed
    `ruff format --check .` at 1007ca9. The file is outside GAP-D's listed files; the change is
    the formatter's only.
  - The gate runs the documented commands in their own clone of HEAD (see W-118).
- **Evidence:**
  - `uv run python scripts/check_doc_commands.py --list`: no unmarked bash block under
    `docs/dev/`. 11 blocks are skipped with reasons: the eight above, the two inventory blocks
    and `docs/research/DATA_INGESTION.md:8`.
  - `uv run python scripts/check_doc_commands.py 'docs/dev/**/*.md'`: 0 blocks run,
    10 skipped, 0 failed.
  - The full run on the repository: `uv run python scripts/check_doc_commands.py --timeout 21600`
    in a clean clone of 54c2bd2, inside the `--full` gate pre-check (W-118). Result: "19
    blocks run, 11 skipped, 0 failed".
    - The three blocks with the full Python suite took 3,851 s (CLAUDE.md:8), 2,397 s
      (CLAUDE.md:24) and 2,397 s (README.md:141). The first exceeds the checker's default
      1,800-s block timeout, so the gate passes `--timeout 21600`.
    - The only tracked file the documented commands rewrote was `docs/RULES_TRACEABILITY.md`
      (lead note 1).
    - The official gate run at 54c2bd2 repeated the result: 19 run, 11 skipped, 0 failed; the
      three blocks took 2,413 s, 2,408 s and 2,399 s.
  - `uv run ruff check .`: All checks passed. `uv run ruff format --check .`: 245 files already
    formatted. `uv run mypy`: no issues in 101 source files.
    `MYPYPATH=src uv run mypy --strict oracle`: no issues in 33 source files.
- **Not done:** T-135, the clean-clone run of `scripts/verify_all.sh` (the Python gate,
  `pytest -m "" -n auto`) with the port committed. The gap list makes it an M16 step for the
  lead.
- **Ablation:** no AI effect (row N): documentation markers and one formatter change in an
  oracle script.

## §8: W-150 and W-152 on the M15 presets

- **W-150** (`crates/gcg-eval/tests/phase6_slow.rs::w150_every_preset_is_identical_at_1_4_and_10_workers`)
  and **W-152** (`w152_seats_and_first_player_are_symmetric`), with their negative controls.
  They run in the gate's slow tier (`cargo test --workspace --release --locked --offline
  --features "$SLOW_OFFLINE"`). `preset_config(p)` is the shipped M15 configuration, which
  `crates/gcg-search/src/presets.rs` has carried since 82b0b34.
- **Status:** re-verified on the M15 presets. In the official clean-clone run at 54c2bd2,
  `phase6_slow` passed: "test result: ok. 4 passed; 0 failed" (242.96 s). The four are
  `w150_every_preset_is_identical_at_1_4_and_10_workers`, `w152_seats_and_first_player_are_symmetric`
  and both negative controls. The counts come from
  `cargo test --release --locked --offline -p gcg-eval --features slow --test phase6_slow w152 -- --nocapture`
  in the worktree at 54c2bd2 (2 passed, 204.82 s):
  - random: seat 0 won 1982/4000 [0.4800, 0.5110]. The first player won 1108/1990 from seat 0
    and 1136/2010 from seat 1 (z = -0.535). This equals the recorded pre-M15 row, because the
    random agent does not use the presets.
  - `search:fast` (shipped M15): seat 0 won 198/400 [0.4463, 0.5438]. The first player won
    120/206 from seat 0 and 116/194 from seat 1 (z = -0.313). The pre-M15 `fast` gave 196/400.
  - The negative control with two extra Shields: 2393/4000 [0.5830, 0.6133], z = 12.541. It
    fails the seat check, as it must.

  W-150 has no counts: it asserts byte identity at 1, 4 and 10 workers for the reference `fast`
  and `standard`, the shipped `fast` and `standard`, and `head-to-head search:fast
  ref:standard`, on two pairings.
- **Ablation:** no AI effect (row I): tests only, no source change.

## §10: `docs/dev/tests.json` bookkeeping (for the lead; tests.json is not edited here)

The ids below are recorded as `not-started` or `failing`, but their tests pass in the official
clean-clone run at 54c2bd2. The scenario tests run in the default tier:
`cargo test --workspace --release`, target `scenarios` (`crates/gcg-search/tests/scenarios/main.rs`)
and target `p6h_pregame`. Every line below reads "... ok" in the gate log.

| Id | Item | Passing test(s) |
| --- | --- | --- |
| T-107 | W-01 | `p6a_w01_two_turn_plan::w01_a_longer_horizon_finds_the_two_turn_plan`, `::w01_the_strong_budget_keeps_the_plan_with_a_longer_horizon`, `::w01_both_plans_win_by_force_and_the_trade_does_not` |
| T-108 | W-03 | `p6b_w03_last_blocker::the_solver_in_rollouts_keeps_the_last_blocker_home` |
| T-109 | W-05 | `p6c_w05_fusion::w05_fusion_decisions_are_recorded` (the item stays open: its bar is not met) |
| T-110 | W-07 | `p6c_w07_open_resources::w07_belief_worlds_hold_back_from_the_inferred_trick` |
| T-111 | W-09 | `p6e_w09_ex_payment::ex_sensitive_plays_keep_their_ex_payment` |
| T-112 | W-12 | `p6e_w12_hidden_burst::the_no_effect_check_keeps_a_move_that_matters_in_some_worlds` |
| T-113 | W-13 | `p6e_w13_freedom::both_searches_rest_the_rested_unit_and_keep_the_lethal` (W-13 stays open for its e2e part) |
| T-114 | W-14 | `p6e_w14_target_critic::the_target_critic_solves_the_repair_ping` |
| T-115 | W-16 | `p6b_w16_fast_lethal::the_fast_budget_misses_a_four_attack_lethal_that_lethal_root_plays` |
| T-117 | W-24 | `p6f_w24_modal::w24_modes_are_scored_in_simulated_lines` |
| T-119 | W-28 | `p6f_w28_end_phase::w28_end_phase_plays_keep_the_rest_effect_for_the_opponents_turn` |
| T-120 | W-29 | `p6f_w29_order::w29_trigger_order_is_scored_in_simulated_lines` |
| T-121 | W-32 | `p6b_w32_cant_block::a_lethal_past_a_blocker_that_cannot_block` |
| T-122 | W-45 | `p6h_pregame`: `w45_the_reference_redraws_a_ramp_hand_that_wins_more_kept`, `w45_the_reference_keeps_a_hand_that_wins_more_redrawn` |
| T-123 | W-139 | `p6c_w139_held_trick::w139_held_trick_decisions_are_recorded` (the item stays open: its bar is not met) |
| T-153 | A30 | `crates/gcg-eval/tests/p6m_cli.rs::t153_benchmark_writes_gcg_sims_bytes_and_ignores_the_factory_variable`, plus the gate's identity line (`diff -r -x timing.json build/w4 build/eval-bench`), which now runs |
| T-155 | W-153 | `uv run pytest oracle/tests -q` in `--full` (25 passed, `refresh_news` included) |
| T-056 | W-110 | the commit-seed fuzz and the 1,000 strategic paranoid games (W-110 above) |
| T-145 | offline binary | `scripts/verify_binary_offline.sh target/dist/gcg-sim` in the gate: "offline binary check passed: gcg-sim (3 pairings, workers 1/4/10)" |
| T-001 | the gate | the official run below |

For the scenario ids, "test passes" means the file's assertions hold. For W-05 and W-139 those
pin the recorded solve counts and do not assert the bar, so the item stays open in
WEAKNESSES.md either way.

Ids that should stay as they are:

- T-116 (W-17): no scenario file.
- T-118 (W-25): the scenario was not reproduced.
- T-106 and T-007: `weaknesses --check --require-resolved` still reports the open items.
- T-130: D14a and the Rust `docs/AI.md` are the lead's Phase 8 step.
- T-135: `scripts/verify_all.sh` on a clean clone is the lead's M16 step.
- T-131: the fidelity run of W-18 is not GAP-D's.

P6-C's status file cites the wrong register ids, as §10 of the gap list says: T-105, T-107 and
T-139 should be T-109, T-110 and T-123. Not edited here; WEAKNESSES.md already uses the
register's ids.

## Notes for the lead

1. **`docs/RULES_TRACEABILITY.md` has two generators.** CLAUDE.md:24-27 documents the Python
   `uv run python -m gcg_sim.tools.traceability --junit build/junit.xml`. It rewrites the file
   in the Python format, but the file is now generated by `gcg-diff traceability --write`
   (with the Rust column), and `traceability --check` reads it. `--full` therefore runs the
   documented commands in their own clone. It allows exactly this one rewritten file and prints
   a note. CLAUDE.md is a project file outside the port's files, so this is for the lead (or
   the post-acceptance CLAUDE.md update).
2. **The session scratchpad is shared between agents.** `scratchpad/gate.sh` and
   `scratchpad/gate.log` were overwritten by another workflow agent (GAP-B, worktree
   `wf_9f38bb2b-c03-3`) during the first run. That cost the log of that run's first 75 minutes.
   Bash had the script open, so it kept running GAP-D's own text. GAP-D then moved everything
   to a private subdirectory. Packages should use unique subdirectories.
3. **The API snapshot after merges.** Every branch that changes a `pub` item of the eight
   library crates must run `target/release/gcg-diff api-snapshot --write`. After merging
   parallel branches, regenerate it once and review the diff. A new `MechanismFlags` field
   changes the `mechanism_flags!` lines.
4. **`oracle/export_cli.py`** is outside GAP-D's listed files. It was changed by
   `ruff format` only, because W-154's verification needs `ruff format --check .` to pass, and
   it failed at 1007ca9 on this file alone.
5. **Worker counts.** Some steps run more than 4 workers by design: the gate's 1/4/10-worker
   identity runs, `scripts/verify_binary_offline.sh`, `phase6_slow.rs`'s `mirror_run`
   (`Workers::Count(10)`) and the documented Python commands. The Python commands fix
   `pytest -n 8` and `--workers 8`, and the slow Python tests open nested pools. GAP-D ran
   them at a lowered priority (nice 10-20).
   - In the pre-check, the efficiency-core background QoS starved them: about 1.3 cores for 8
     workers. So the official run used nice 10 instead.
   - From 00:02 to about 01:30 local on 2026-11-03, the `mixed` step of a standalone W-110 run
     of GAP-D (4 workers) overlapped the first gate run (4 workers). Stopping the run's
     search:fast step left its script running into the next step, and GAP-D noticed late. It
     was stopped; its results are not used.
6. **The system `python3` is 3.9.6** (`/usr/bin/python3`, CommandLineTools). Every stdlib
   helper the gate runs with `python3` must stay 3.9-compatible. `scripts/normalize_outputs.py`
   was the one that was not.
7. **W-126** needs only the idle-machine pair of `throughput.py` runs, or the lead's decision
   that the two harness sessions meet the criterion. The session-2 minima of
   `scripts/compare_python_vs_rust.sh` are not in the repository.

## The official clean-clone run

The command was `GCG_GATE_WORKERS=4 RUST_TEST_THREADS=4 CARGO_BUILD_JOBS=4 scripts/verify_all_rust.sh --full`,
started 2026-11-03T08:46:26Z from the worktree. The script cloned the branch at 54c2bd2 into a
temporary directory and ran every step there. It ended with "verify_all_rust passed", exit 0,
at 16:52:59Z: 29,193 s wall, with the machine shared, so not a measurement.

The documented-commands step ran at nice 10 (lead note 5). The system `python3` was 3.9.6.

| Step | Result |
| --- | --- |
| `cargo fmt --check`; `cargo clippy` without features, and with `$SLOW,$DIAG` (`-D warnings`) | clean |
| `cargo test --workspace --release` (default tier) | 1,247 passed, 0 failed (133 test binaries), including `api_snapshot.rs` 3/3, the scenario tests and `p6m_cli.rs::t153_*` |
| `cargo test ... --features "$SLOW_OFFLINE"` (slow tier) | 1,271 passed, 0 failed (137 binaries), including `phase6_slow` 4/4 (W-150, W-152) |
| `cargo test -p gcg-engine --features paranoid,testkit` | 247 passed, 0 failed |
| `gcg-diff lints`; `selftest`; `selftest --real-engine` | passed; 22 and 36 negative controls fired |
| tier-1 corpus: parse-only, mock, mock roundtrip, real engine, real-engine roundtrip, paranoid | 3804/3804 each (`search` 9/9, `baselines` 8/8; 1,396 agent decisions; per-test traces 3213/3213, census games 311/311) |
| `coverage --check`, `traceability --check`, `tests-json --check`, `weaknesses --check`, `api-snapshot --check` | all pass; both traceability documents current |
| `fuzz --games 10000 --seed 2026 --compare` | 856,034 actions, 0 failures, equal to Python (T-055) |
| `fuzz --games 2000 --seed-from-commit` 54c2bd2 | seed 1585305945398772890, 170,257 actions, 0 failures |
| `paranoid-games` | passed |
| paranoid fuzz `greedy` 400 / `search:fast` 400 / `mixed` 200 | 31,936 / 29,781 / 14,650 actions, 0 failures each: the same counts as the first run (W-110) |
| `gcg-eval decks --overlap --check`, `evidence check`; the `gcg-cli` dependency-tree checks | pass |
| CLI runs: `gcg-sim`, `reference-benchmark` and head-to-head at 1/4/10 workers | identical (normalised; head-to-head byte for byte) |
| T-153: `gcg-eval benchmark` against `gcg-sim benchmark`, `diff -r -x timing.json` | identical |
| replays; three `schema-check` lines; `--ai-preset strong STANDARD Fast bogus` | verify; pass; exit 2 each |
| dist build, `scripts/verify_binary_offline.sh target/dist/gcg-sim` | "offline binary check passed: gcg-sim (3 pairings, workers 1/4/10)" (T-145) |
| stdlib checks: doc-checker self-test, CLI help snapshot, throughput self-test, gate self-test, settings self-test, `doc_counts --check`, `gate_summary` | pass (two stale-research warnings, as before) |
| A15: `src/gcg_sim`, `tests` | unchanged |
| `--full`: `uv sync`, `ruff check`, `ruff format --check`, `mypy --strict oracle` | clean |
| `--full`: documented commands in their own clone | 19 run, 11 skipped, 0 failed. The three full-suite blocks took 2,413 s, 2,408 s and 2,399 s. Only `docs/RULES_TRACEABILITY.md` was rewritten (note printed) |
| `--full`: `blind_transcripts`, `fuzz_census` self-tests; fuzz census 10,000 games; test census (2,892 passed) | pass; both records current |
| `--full`: `traces.py --check --tier 1` under PYTHONHASHSEED 1, 2 and `--nocache`; `vectors`, `census`, `eval_decks`, `p6k_vectors`, `p6k_audit`, `templates/check.py` | all current |
| `--full`: `pytest oracle/tests` | 25 passed (T-155) |
| `--full`: comparison self-tests; equivalence smoke (seed 1, bo1, rgz:bwf, workers 1, decision logs off) | 15/15 and 14/14; "equivalence: 3/3 identical (fast 1/1; standard 1/1; strong 1/1)"; Python baselines identical under hash seeds 1 and 2 |
| `--full`: `traces.py --write --tier 2`, `corpus --tier 2 --require-all` | 4154/4154 (123,726 decisions, 12,131 reference-search decisions) |
| `--full`: `cargo test ... --features "$SLOW_TIER2"` | 1,248 passed, 0 failed |
| the tree after the gate | clean |

The record files (this file and `docs/dev/eval/GAP-D/`) were committed after this run, in a
commit that adds only them. On that commit, the gate's checks that read these files were rerun:

- `gcg-eval evidence check`: pass, 224 files, including this one and the two throughput reports.
- `python3 scripts/doc_counts.py --check`: 7 marked counts, 0 stale.
- `uv run python scripts/check_doc_commands.py --list`: 0 run, 11 skipped, 0 failed. This file
  holds no shell block.
- `gcg-diff weaknesses --check docs/dev/WEAKNESSES.md`, `tests-json --check`,
  `api-snapshot --check` and `lints`: pass.
- `uv run ruff check .` and `uv run ruff format --check .`: clean.
