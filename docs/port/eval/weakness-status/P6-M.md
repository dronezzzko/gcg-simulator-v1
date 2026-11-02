# P6-M status: reporting and runner (class N)

Package P6-M of docs/port/PLAN.md §12.2: W-90 to W-109, W-147, W-148, W-149, W-156 (24 items),
plus the results-schema 1.2.0 groundwork of §12.7. Written by the P6-M implementer on
2026-11-02 in worktree branch `worktree-wf_b2d4a964-3bd-6`, from `8ae4767`. WEAKNESSES.md is
not edited here; the lead records the status lines from this file (§12.10).

Every new option lives in `gcg-eval` (A30). `gcg-sim`'s command line, `gcg-eval
reference-benchmark` and every reference-equivalent output are unchanged.

## What P6-M changed

| File | Items |
| --- | --- |
| `crates/gcg-eval/src/benchmark_opts.rs` (new) | `gcg-eval benchmark` (T-153, W-90 to W-99, W-104, W-105, W-109, W-134, W-147, W-148, W-46), `gcg-eval replay-game` (W-148), `gcg-eval replay-check` (W-156), the `compare-runs` guard (W-91) |
| `crates/gcg-eval/src/cli.rs`, `lib.rs` | dispatch lines only: three subcommands, `compare-runs --allow-mixed` (W-91) |
| `crates/gcg-runner/src/analysis.rs` (new) | `analysis.json` and `diagnostics.json`, with the re-simulation observer (W-46, W-90, W-92 to W-96, W-98, W-101, W-109, W-147) |
| `crates/gcg-runner/src/checkpoint.rs` (new) | `--resume` checkpoints (W-105); the W-107 compile-fail doctest |
| `crates/gcg-runner/src/reports/set.rs` (new) | the report set written as one unit (W-102) plus the opt-in files (W-99, W-148) |
| `crates/gcg-runner/src/reports/gzip.rs` (new) | the in-house gzip writer (W-99) |
| `crates/gcg-runner/src/reports/identity.rs` (new) | AI identity and version comparison (W-91, W-156) |
| `crates/gcg-runner/schema/results.v1_2.schema.json` (new), `reports/schema.rs` | the 1.2.0 schema and its validation (§12.7, W-97) |
| `crates/gcg-runner/src/reports/results.rs`, `summary.rs`, `writer.rs` | 1.2.0 documents (`SearchReport`, `ai.search`, `ai.effort`, the search line), the W-100 reproduce line and deck copies for 1.2.0 runs; 1.1.0 output unchanged |
| `crates/gcg-runner/src/reports/hypotheses.rs` | `family_tests` (the four families before Holm) for W-94/W-98; `hypotheses` unchanged |
| `crates/gcg-runner/src/lib.rs`, `reports/mod.rs` | module lines and re-exports |
| `crates/gcg-runner/tests/p6m_reports.rs` (13 tests), `p6m_analysis.rs` (14), `p6m_interrupt.rs` (1) | evidence |
| `crates/gcg-eval/tests/p6m_cli.rs` (8 + 1 with `--features zstd`) | evidence |

No file under `crates/gcg-{core,engine,ai,ai-reference,search,cli}`, the runner's game loop
(`game.rs`, `matches.rs`, `run.rs`, `instrument.rs`, `config.rs`, `records.rs`) or
`cli_support.rs` changed: `git diff --stat 8ae4767 -- crates/gcg-engine crates/gcg-ai
crates/gcg-ai-reference crates/gcg-search crates/gcg-core crates/gcg-cli
crates/gcg-runner/src/{game,matches,run,instrument,config,records,cli_support}.rs` is empty.

## `gcg-eval benchmark` (A30, T-153)

`gcg-eval benchmark DUT BENCH` takes every `gcg-sim benchmark` option (`--ai-preset
standard|fast`, `--workers N|auto`, ...) and runs the same pipeline: `cli_support::legal_deck`
and `writable_dir` (same messages), `gcg-sim`'s factory (`ShippedFactory`:
`make_agent(preset_config(p))`, `ai.factory` = `Preset::factory_description`), schema 1.1.0
(`SIM_SCHEMA`, as `gcg-cli`'s `commands::SCHEMA`), the same reports and stdout. The reports
are written as one set (W-102). Opt-in options: `--analysis` (`analysis.json`, `variants/*.txt`),
`--swiss-draws` (needs `--analysis`), `--compress`, `--resume`, `--replays-extremes`,
`--diagnostics` (`diagnostics.json`), `--opponent-list known|archetype`. `timing.json` gains the
deterministic `decisions` and `decisions_per_game` (W-104); T-153 excludes `timing.json`.

T-153 evidence: `p6m_cli.rs::t153_benchmark_writes_gcg_sims_bytes_and_ignores_the_factory_variable`
(every file except `timing.json`, stdout and stderr equal the in-process `gcg-sim` pipeline
`cli_support::benchmark_command` with `gcg-sim`'s factory), and the PLAN §9.11 gate line run by
hand with both binaries:

```
target/release/gcg-sim benchmark examples/decks/red-green-zeon.txt examples/decks/blue-white-federation.txt --matches 4 --seed 7 --ai-preset fast --workers 4 --decision-log --out build/p6m/w4
target/release/gcg-eval benchmark (same arguments) --out build/p6m/eval-bench
diff -r -x timing.json build/p6m/w4 build/p6m/eval-bench   -> no difference
stdout (paths normalised) and stderr                        -> identical
```

## Results schema 1.2.0 groundwork (PLAN §12.7)

- `crates/gcg-runner/schema/results.v1_2.schema.json`: the 1.1.0 constraints plus
  `ai.search` (`algorithm` `gcg-search`, `mapping`, `budget` {`iterations_per_option`,
  `min_iterations`, `max_iterations`, `aux_apply_cap`}, sorted `mechanisms`) and `ai.effort`
  (per seat: `iterations`, `applies_tree`, `applies_rollout`, `applies_aux`), `ai.preset` in
  `["fast", "standard"]`, `additionalProperties: false` kept. Embedded as `RESULTS_SCHEMA_V1_2`;
  `validate_results(doc, V1_2)` validates against it (it returned an error before).
- `build_results_with` writes a 1.2.0 document for a 1.2.0 configuration from
  `ResultsData::with_search(SearchReport)`; without the search block the schema rejects it,
  so it is never written. `render_summary` adds the search line; `reproduce_command` writes the
  corrected W-100 line (A23); `write_reports_with` and `write_report_set` write the deck copies
  the line names.
- `gcg-eval reference-benchmark` keeps 1.1.0 byte-identically (reports_parity, the
  equivalence smoke below).
- **Not switched on for `gcg-sim`:** `ai.effort` needs per-seat effort totals from the agents,
  which neither `Agent` nor `AgentFactory` exposes (docs/port/contracts.md O9). Interface change
  request (O9's own proposal): `Agent::effort(&self) -> SearchEffort` (the search agents sum
  their decisions' `SearchResult.effort` and iterations) and `AgentFactory::search_block(&self)
  -> Option<SearchReport>` (mapping, budget, `aux_apply_cap`, `MechanismFlags::enabled()`); the
  runner sums per seat into `GameRecord`/`BenchmarkRun`. Then `gcg-cli`'s `commands::SCHEMA`
  and `gcg-eval`'s `benchmark_opts::SIM_SCHEMA` switch to `V1_2` together (T-153). Evidence of
  the groundwork: `p6m_reports.rs::schema_1_2_validates_a_run_with_its_search_block_and_rejects_without`
  (a 1.2.0 run validates; negative controls: no `ai.search` fails at `/ai`, preset `strong`
  fails at `/ai/preset`, a 1.2.0 document fails 1.1.0).

## Ablation rows (A13), common to every P6-M item

Every P6-M item is class N. **No AI effect: P6-M changes no code the AI executes during a
decision or a game.** The engine, effects, agents, both searches and the runner's game loop
are untouched (the empty `git diff --stat` above). The new code runs before a run (argument
parsing), after it (reports, `analysis.json`, re-simulation for the analysis), or around
`play_match` (`--resume`, which calls the same `play_match` for each missing match). Evidence:
the tier-1 corpus at 3804/3804 (`search` and `baselines` sets included), the reference
equivalence smoke at 3/3, the gcg-search scenario tests in `cargo test --workspace --release`,
T-153 (byte-identical reports with and without `gcg-eval benchmark`), and the resume e2e
(byte-identical reports after an interrupt and a resume). The row text is repeated per item
below as "Ablation: no AI effect (P6-M common row)".

## Items

### W-90 — resolved
- **Resolution:** `analysis.json` `cards_by_length`: per main-deck card of the deck under test,
  the raw "win rate when drawn minus not drawn" and the Mantel-Haenszel risk difference within
  game-length strata (one stratum per number of turns) with Sato's variance and 95% interval
  (`analysis::mantel_haenszel`). `swap_tests`: for every card hypothesis of `results.json`, a
  variant deck (+1 of the card, -1 of the card with the opposite signal; written to
  `variants/swap-<add>-for-<remove>.txt`), the `gcg-sim benchmark` command on the same master
  seed and `gcg-eval compare-runs` on the two directories (paired, W-42). The reference
  statistics are unchanged.
- **Evidence:** `p6m_analysis.rs::w90_a_pure_length_effect_vanishes_within_length_strata`
  (3,000 synthetic games whose wins depend on length only: raw lift > 0.15, stratified
  difference |d| < 0.05 with a 95% interval containing 0), `w90_card_hypotheses_get_a_paired_swap_test`
  (the variant deck parses, passes `validate` with B&R, differs by exactly the swap; the commands
  carry the seed and `compare-runs`). e2e: `p6m_cli.rs::opt_ins_add_files_and_keep_the_reports`
  (every reference file byte-identical with `--analysis`).
- **Ablation:** no AI effect (P6-M common row).

### W-91 — resolved
- **Resolution:** `gcg_runner::reports::identity`: the AI identity (`ai.factory`, `ai.preset`,
  `ai.dut_agent`, `ai.bench_agent`, `ai.search`) and the `versions` fields except
  `results_schema`. `gcg-eval compare-runs` refuses two runs that differ in any of them (the
  message names every difference and `--allow-mixed`); `--allow-mixed` compares anyway and
  prints the differences on stderr. A directory without `results.json` (hand-made records) has
  no recorded identity: a note, then the comparison. The Rust `results.json` already records
  the AI identity (`ai.factory` names `gcg-search::make_agent('<preset>')` or
  `gcg-ai-reference::make_agent('mcts')`; agents `search-*` or `mcts-*`).
- **Evidence:** `p6m_reports.rs::w91_runs_of_different_ais_are_not_compared` (strengthened
  `standard` vs reference `standard` fails naming `ai.factory` and `ai.dut_agent`; another
  `code_digest` fails; identical identities pass; `--allow-mixed` gives the notes),
  `p6m_cli.rs::w91_compare_runs_refuses_runs_of_different_ais` (exit 1 with the message; exit 0
  with `--allow-mixed`; a run against itself passes). The P6-I test
  `phase6.rs::w42_compare_runs_pairs_same_seed_runs` still passes.
- **Ablation:** no AI effect (P6-M common row).

### W-92 — resolved (one part for the lead)
- **Resolution:** `analysis.json` `clustered`: for the game win rate, on the play and on the
  draw, the Wilson interval, a match-clustered interval (ratio estimator, cluster-robust
  variance `C/(C-1) Σ (y_i - p m_i)² / N²`), a seeded cluster bootstrap over matches (2,000
  replicates, `derive_seed(master, "analysis", "bootstrap", <rate>)`) and the design effect.
- **Evidence:** `p6m_analysis.rs::w92_match_clustered_intervals_cover_and_plain_ones_do_not`:
  1,000 replications of 100 BO3-like matches with within-match correlation 0.36 (true rate
  0.5): clustered coverage 0.935, plain Wilson 0.837 (`--nocapture` prints both); the bootstrap
  is reproducible from its seed and within 0.03 of the analytic interval.
- **Not done here:** "the Rust port of the benchmark skill's sample-size guidance uses the
  design effect" is a `.claude/skills/gcg-benchmark` text change (Phase 8, lead-owned).
- **Ablation:** no AI effect (P6-M common row).

### W-93 — resolved
- **Resolution:** every rate in the workspace comes from the one Wilson interval
  (`gcg_runner::reports::stats::wilson`, `Rate`) over wins only. `analysis.json` prints the
  draw count and draw rate beside every outcome rate (`analysis::wins_only`); `summary.md` and
  `results.json` are unchanged (Python's Draws column only). gcg-eval's head-to-head prints
  `draw_rate` (P6-I).
- **Evidence:** `p6m_reports.rs::w93_every_rate_tool_uses_the_one_wins_only_interval` (one
  `fn wilson` in the workspace; analysis, `gcg-eval` stats, compare-runs and gates call it; the
  analysis rate equals `Rate`), `p6m_analysis.rs::w93_draws_are_non_wins_with_their_rate_beside`
  (4 games with 2 draws: rate 0.25, draws 2, draw rate 0.5).
- **Ablation:** no AI effect (P6-M common row).

### W-94 — resolved
- **Resolution:** `analysis.json` `hypotheses`: every comparison of the four families
  (`hypotheses::family_tests`, the same candidates and family sizes as `hypotheses`) with its
  rounded and unrounded p-value, the reference decision (Holm per family on rounded p), the
  run-level Holm-adjusted p-value over all families (unrounded) and the run-level decision.
- **Evidence:** `p6m_analysis.rs::w94_run_level_holm_keeps_the_false_positive_rate_at_alpha`:
  400 simulated null runs of 400 games (no card, seat or mulligan effect): per-family control
  gives at least one spurious hypothesis in 16.75% of runs (3.35 alpha), the run-level
  adjustment in 2.0% (at most alpha + 2 standard errors is asserted).
- **Ablation:** no AI effect (P6-M common row).

### W-95 — resolved (category wording stated)
- **Resolution:** `analysis.json` `never_played`: each never-played drawn copy split by fate,
  from re-simulating the game through the engine (`analysis::observe_game`):
  `discarded_for_cost` (a `discard` by its owner during an effect or cost; the engine records
  the owner's own discard effects and costs alike, so the category covers both),
  `used_by_effect_from_hand` (left the hand for another zone without a play action),
  `in_hand_at_end`, `other_hand_limit` (rule 7-6-5-1), `other_opponent_discard`, `other`; each
  with a Wilson interval over drawn copies. The reference signal is unchanged.
- **Evidence:** `p6m_analysis.rs::w95_never_played_copies_are_split_by_fate` (a copy discarded
  for a cost is reported under `discarded_for_cost`; the reference never-played count is
  unchanged), `the_observer_partitions_the_reference_never_played_set_and_extends_seen` (12
  BO3 matches of real engine games: the fates partition exactly the reference `never_played`
  list of every game; `in_hand_at_end` copies are in the recorded end hand; at least two fate
  kinds occur).
- **Ablation:** no AI effect (P6-M common row).

### W-96 — resolved
- **Resolution:** `analysis.json` `benchmark_cards`: "observed" = the reference "seen" events
  and zones plus any discard, trash or removal-area entry and any play (re-simulation);
  `loss_rate_lift` in both directions with `direction`; the benchmark's own draws and plays
  (games drawn, games played, plays, own-turn distribution, loss rate when drawn).
- **Evidence:** `p6m_analysis.rs::w96_a_benchmark_card_only_discarded_is_observed_not_seen`;
  the real-game test above checks that every reference "seen" card is "observed".
- **Ablation:** no AI effect (P6-M common row).

### W-97 — resolved
- **Resolution:** the 1.1.0 schema keeps Python's file and name (`results.v1.schema.json`);
  the 1.2.0 schema is `results.v1_2.schema.json`. Naming rule (stated in the 1.2.0 schema's
  `description`): the file name carries the major version, and the minor version when two
  minors ship side by side. The Rust report docs (Phase 8) should repeat it.
- **Evidence:** `p6m_reports.rs::w97_schema_files_carry_the_version_they_validate` (`$id`, the
  `schema`/`schema_version`/`versions.results_schema` constants and `SchemaVersion` agree for
  both files).
- **Ablation:** no AI effect (P6-M common row).

### W-98 — resolved
- **Resolution:** `analysis.json` `distributions`: unrounded means and Hyndman-Fan type 7 p10,
  median and p90 for game length (all, by result) and per-card own play turns; `hypotheses`
  adds the unrounded p-values, the Holm decision on them, and `unrounded_differs` where it
  differs from the reference decision. The reference nearest-rank and rounding stay (reports
  parity vectors).
- **Evidence:** `p6m_analysis.rs::w98_unrounded_statistics_beside_the_reference_values` ([1,2,3,4]:
  reference median 2, interpolated 2.5; a Holm family of one at alpha 0.05 with p = 0.0500004:
  the rounded 0.05 is kept, the unrounded one is not); `played_events_feed_the_own_turn_distribution`.
- **Ablation:** no AI effect (P6-M common row).

### W-99 — resolved
- **Resolution:** `gcg-eval benchmark --compress` replaces `games.ndjson` by
  `games.ndjson.zst` (builds with the approved `zstd` encoder, feature `zstd`, level 19) or
  `games.ndjson.gz` (the in-house safe-Rust gzip writer, `reports/gzip.rs`, fixed-Huffman
  DEFLATE with LZ77, deterministic). Defaults and limits are unchanged.
- **Evidence:** `p6m_reports.rs::w99_gzip_output_decompresses_to_the_same_bytes` (`gzip -dc`
  gives back empty, 1-byte, repetitive, multi-block, incompressible and real `games.ndjson`
  inputs exactly), `p6m_cli.rs::opt_ins_add_files_and_keep_the_reports` (the compressed file
  decompresses to the baseline `games.ndjson`), and with `--features zstd`:
  `cargo test -p gcg-eval --release --features zstd --test p6m_cli -- w99_zstd opt_ins` -> 2
  passed (`w99_zstd_output_decompresses_to_the_same_bytes` and the opt-ins test on the zstd
  path). The default gate builds without the feature, so it tests the gzip path.
- **Ablation:** no AI effect (P6-M common row).

### W-100 — resolved for results 1.2.0 (A23); `gcg-sim` prints it once it writes 1.2.0
- **Resolution:** for 1.2.0 runs `reproduce_command` writes the corrected line: a deck without
  a source is written to `decks/deck_under_test/<name>.txt` or `decks/benchmark/<name>.txt` in the report
  directory (so `load_deck` gives it its name back) and the command names that file; a run
  whose `ai.factory` is not `gcg-search::make_agent('<preset>')` (an injected agent) gets
  "not reproducible with the release CLI: the agents came from the injected factory ...". 1.1.0
  runs (`reference-benchmark`, and `gcg-sim` until the O9 switch) keep Python's line byte for
  byte. Every run the release `gcg-sim` can make has file decks and a preset factory (W-108),
  for which the corrected line equals Python's.
- **Evidence:** `p6m_reports.rs::w100_the_corrected_reproduce_line_of_results_1_2` (Python's
  line for 1.1.0; the injected-agent marker; the release factory gives the command; an
  in-memory deck is written and loads back with the same name, card counts and digest; a name
  that cannot be a file gives the marker), `p6m_cli.rs::t153_...` (the printed command, run
  again, writes the same `results.json` bytes).
- **Ablation:** no AI effect (P6-M common row).

### W-101 — partially resolved (card-level trace)
- **Resolution:** `analysis.json` `interpretations_used`: every interpretation of the P6-K table
  `docs/port/audit/interpretations.json` (W-56) one of whose tagged cards was played, deployed,
  paired, activated, attacked with or revealed as a Burst in the run's games, with its games and
  cards; interpretations without card tags (rule level, 14 of 27) are listed as not traceable
  from games. The existing `conflict_resolutions` field is unchanged.
- **Evidence:** `p6m_analysis.rs::w101_a_suppression_card_lists_the_suppression_interpretation`
  (a game using a card of I-18, "<Suppression> with a Base present does nothing", lists I-18;
  rule-level I-01 is reported untraceable).
- **Not done:** the plan's engine-side recording "when an interpretation-tagged code path runs"
  needs tags in the engine and effect interpreter (gcg-engine/gcg-effects, not P6-M files; P6-J
  or P6-K with the lead). The card-level trace over-approximates: a listed interpretation's code
  path may not have run.
- **Ablation:** no AI effect (P6-M common row).

### W-102 — resolved for `gcg-eval benchmark`; `gcg-sim` switch for the lead
- **Resolution:** `reports::write_report_set` builds every file in memory, writes them into
  `.{name}.{pid}.new` beside the report directory and swaps it in (one `rename` for a new
  directory; otherwise `rename(out, .{name}.{pid}.old)`, `rename(staging, out)`, remove the old
  set). A directory holding anything besides report files (or `.`) is updated in place with
  every file staged under a temporary name and renamed only after all are written. Contents
  are `write_reports_with`'s, byte for byte. `gcg-eval benchmark` always uses it.
- **Evidence:** `p6m_reports.rs::w102_an_abort_after_any_file_keeps_the_complete_old_set` (for
  each file of a new BO3 set, an abort after it leaves the old set byte for byte and no staging
  directory, in both modes; the foreign file survives; the completed write gives the new set), `w102_a_symlinked_report_directory_stays_a_link` (a symlinked directory is written through, in place, and stays a link).
- **Not done here:** `gcg-sim benchmark` still calls `write_reports` through
  `cli_support::benchmark_command` (not a P6-M file). Switching it is one call
  (`write_report_set` with `SetOptions::default()`); it changes only the failure behaviour
  (no partial replays) and the failing path named in an I/O error message (the swap shows the
  report path).
- **Ablation:** no AI effect (P6-M common row).

### W-103 — resolved (by construction, Phase 3)
- **Resolution:** `code_digest` is computed at build time over `build-support/digest-manifest.txt`
  (`build.rs` of gcg-cli and gcg-eval, `build-support/code_digest.rs`); the package version is
  `CARGO_PKG_VERSION`.
- **Evidence:** `crates/gcg-cli/tests/code_digest.rs` (an unrelated file leaves the digest
  unchanged, an edited source file changes it; the embedded digest is the files' digest), in
  `cargo test --workspace --release`.
- **Ablation:** no AI effect (P6-M common row).

### W-104 — partially resolved (deterministic counts only)
- **Resolution:** `gcg-eval benchmark`'s `timing.json` adds `decisions` and
  `decisions_per_game` (deterministic, from the records). Python's wall-clock fields and the
  topology stay; `gcg-sim`'s `timing.json` keeps exactly Python's keys plus `topology`
  (reports_parity pins the key list).
- **Evidence:** `p6m_cli.rs::t153_...` (`decisions_per_game` equals the action counts of
  `games.ndjson`; T-153 and the equivalence comparison ignore `timing.json`).
- **Not done:** per-worker CPU time and start-up time. Safe Rust's std has no CPU-time clock
  (`getrusage` needs `unsafe` FFI, denied outside `gcg-core/src/libm.rs`), and the one
  wall-clock module (`crates/gcg-runner/src/clock.rs`, lint rule 2) is not a P6-M file. Lead
  decision: add a per-worker busy-seconds measure to `clock.rs`/`run.rs`, or record the item as
  done with the deterministic counts.
- **Ablation:** no AI effect (P6-M common row).

### W-105 — resolved
- **Resolution:** `gcg-eval benchmark --resume` (`gcg_runner::checkpoint`): `checkpoint.ndjson`
  in the report directory (header with the configuration identity and its digest; one line per
  finished match with its full game records), appended as matches finish; a resume verifies
  the digest (a different configuration is refused, naming the differing fields), drops a cut
  final line, takes the finished matches and plays the missing ones with the same
  `play_match`. The report set write removes the checkpoint. Without `--resume` nothing
  changes; with it a failure still writes no reports (exit 1 or 130) and keeps the checkpoint.
- **Evidence:** `p6m_reports.rs::w105_a_resumed_run_writes_the_uninterrupted_reports` (at 1
  and 3 workers: match 3 fails, a different seed is refused, a cut-off line is ignored and
  dropped, the resumed reports equal an uninterrupted run's byte for byte except
  `timing.json`), `p6m_cli.rs::w105_an_interrupted_resume_run_finishes_with_the_uninterrupted_reports`
  (SIGINT after the first match: exit 130, checkpoint kept, no reports; another seed refused;
  the resume's reports equal the uninterrupted `gcg-sim` pipeline's).
- **Ablation:** no AI effect (P6-M common row).

### W-106 — resolved (by construction, Phase 3)
- **Resolution:** games run on `gcg_core::pool` threads with the shared `Cancel` checked
  between decisions; results are collected in match order.
- **Evidence:** `p6m_interrupt.rs::w106_two_hundred_interrupts_return_no_results` (200 runs,
  each interrupted by a real SIGINT during a match, at 1 and 4 workers: `Err(Interrupted)` every
  time), `crates/gcg-cli/tests/interrupt.rs` and `sigint.rs` (exit 130, no reports), and the
  worker-count identity tests (`run.rs::results_do_not_depend_on_workers` at 1, 4, 10;
  `crates/gcg-cli/tests/benchmark.rs` at 1, 4, 10, auto).
- **Ablation:** no AI effect (P6-M common row).

### W-107 — resolved (by construction, Phase 3)
- **Resolution:** agent factories are `AgentFactory: Sync` trait objects shared by the worker
  threads; no start-method or `__main__` constraint exists.
- **Evidence:** `p6m_reports.rs::w107_a_custom_factory_runs_on_ten_workers` (a test-binary
  factory, 10 workers, 40 agents built), the compile-fail doctest in
  `crates/gcg-runner/src/checkpoint.rs` (a factory holding a `Cell` does not compile), and
  `contracts.rs::factories_and_records_cross_threads`.
- **Ablation:** no AI effect (P6-M common row).

### W-108 — resolved
- **Resolution:** no binary reads an agent-factory variable; test agents are reachable only
  through `gcg-eval reference-benchmark --agent` and the library API, recorded under
  `ai.factory`, and a 1.2.0 run with an injected agent gets the "not reproducible" reproduce line
  (W-100).
- **Evidence:** `p6m_reports.rs::w108_no_crate_reads_an_agent_factory_variable` (every
  environment read in `crates/*/src` is `COLUMNS` or the dev tool's `GCG_REPO_ROOT`),
  `p6m_cli.rs::t153_...` (with `GCG_SIM_AGENT_FACTORY` set, `gcg-eval benchmark` writes the bytes
  of the run without it), `w100_...` (the marker names the injected factory).
- **Ablation:** no AI effect (P6-M common row).

### W-109 — resolved
- **Resolution:** the engine and runner implement both rules exactly (Phase 3:
  `game.rs::match_point_of`); `results.json` already states the double defeats and how they were
  scored (`draws.both_defeated`, `scored_for_match_by_turn_player_rule`, the NOTES line).
  `analysis.json` `double_defeats` repeats them, and `--swiss-draws` adds the Swiss scoring: a
  match whose deciding game was a double defeat is a drawn match (1 point each), only in
  `analysis.json`. The packaged data wording stays (data workflow).
- **Evidence:** `game.rs::tests::match_point_rules` (a double defeat is a game draw; in BO3 the
  turn player loses it, as Python), `p6m_analysis.rs::w109_a_double_defeat_decider_is_a_swiss_draw_in_the_analysis_only`,
  `p6m_cli.rs::opt_ins_...` (`--swiss-draws` adds the section; without `--analysis` it is a
  usage error, exit 2).
- **Ablation:** no AI effect (P6-M common row).

### W-147 — resolved
- **Resolution:** `analysis.json` `mulligan`: per game the deck under test's dealt-hand and
  played-hand opening scores (`gcg_ai::setup::opening_score`, re-simulated), the AI's threshold
  (the redraw alternative's value from the decision log, when logged), kept or redrawn, on the
  play; win rates by quintile of the played hand's score, kept against redrawn, and the
  Mantel-Haenszel difference; labelled as a measure of the AI's mulligan rule, not of the deck.
  The reference hypothesis text is unchanged.
- **Evidence:** `p6m_analysis.rs::w147_the_redraw_gap_is_the_ai_rule_not_the_deck` (4,000
  synthetic games whose wins depend only on the played hand's score: raw redraw gap below -0.1,
  stratified interval contains 0); the real-game test checks `redrew` against the record.
- **Ablation:** no AI effect (P6-M common row).

### W-148 — resolved (both options in `gcg-eval`, A30)
- **Resolution:** `gcg-eval replay-game --games-ndjson FILE --results RESULTS --match M --game G`
  rebuilds the run's decks from `results.json` (their digests must match), re-simulates the
  record, checks winner, end reason, turns and final state digest, and prints the log, the seats,
  the result and the verdict (with the W-156 version diagnosis on a mismatch).
  `gcg-eval benchmark --replays-extremes` also saves the shortest and longest win and loss as
  `replays/{shortest,longest}-{win,loss}-mMMMM-gG.json` (ties by match and game index);
  `results.json` and the representative replays are unchanged.
- **Evidence:** `p6m_cli.rs::w148_replay_game_verifies_every_game_of_a_run` (every game of a
  4-match BO3 run verifies; a tampered final digest fails with "regression or nondeterminism"),
  `p6m_reports.rs::w148_extreme_replays_are_the_minimum_and_maximum_lengths` (minimum and
  maximum lengths with ties by match index; names outside `REPLAY_FILE`; only the 4 extra files
  differ), `p6m_cli.rs::opt_ins_...`.
- **Ablation:** no AI effect (P6-M common row).

### W-149 — resolved (by construction, Phase 3)
- **Resolution:** the pool reports the lowest failing match index and lets lower matches
  finish (`run.rs`, `gcg_core::pool` `lowest_failed`).
- **Evidence:** `p6m_reports.rs::w149_the_lowest_failing_match_is_reported` (failures at
  matches 3 and 5: the match-3 error at 1, 2 and 8 workers over 50 repetitions each; only match
  5 failing: the checkpoint shows matches 0 to 4 finished when the error is reported), and
  `run.rs::the_lowest_failing_match_is_reported`, `a_lower_failure_supersedes_running_higher_matches`.
- **Ablation:** no AI effect (P6-M common row).

### W-156 — resolved (in `gcg-eval replay-check`, A30)
- **Resolution:** `gcg-eval replay-check FILE [--results R] [--log]` runs `gcg-sim replay`'s
  check (`cli_support::replay_command`) and compares the run's `results.json` `versions` (one
  directory above `replays/`, or `--results`) with this build's. A passing check with matching
  versions prints exactly `gcg-sim replay`'s output; a passing check with other versions adds a
  note; a failing check adds "the run's versions differ ... an expected change is possible"
  with every differing field (recorded and current value), "versions match ... a regression or
  nondeterminism", or "the versions are unknown". `gcg-sim replay`, replay files and
  `games.ndjson` are unchanged.
- **Evidence:** `p6m_cli.rs::w156_replay_check_names_version_differences` (matching versions:
  stdout equals `replay_command`'s; a copied-out replay: "versions are unknown"; a run with a
  changed `dataset_version` and a tampered replay: exit 1 naming `dataset_version: recorded
  "synthetic-older", current ...`).
- **Ablation:** no AI effect (P6-M common row).

### W-46 and W-134 (P6-J and P6-C items; P6-M adds the options)
- `--diagnostics` writes `diagnostics.json`: every `turn_limit` game with the cap that fired
  (`turns`, or `decisions` when the re-simulated `action_count` exceeds `max_actions`), from the
  final state. `p6m_analysis.rs::w46_the_cap_kind_names_the_cap_that_fired` (`max_actions =
  10`: both games `turn_limit` with the decision cap; `turn_limit = 2`: the turn cap). The
  engine-recorded `CapKind` and the head-to-head listing are P6-J's.
- `--opponent-list known|archetype` sets `StrengthenedConfig::mechanisms.opponent_list`
  (`ShippedFactory`; `ai.factory` names a non-default value). The `archetype` mechanism is
  P6-C's: this build refuses it at the first decision ("mechanism opponent_list (P6-C) is not
  available in this build", `p6m_cli.rs::w134_opponent_list_reaches_the_search_flag`), never
  ignores it. The prior over candidate lists that `archetype` needs has no option yet (P6-C's
  design).

## Gate (run on this branch after the last edit)

| Command | Result |
| --- | --- |
| `cargo fmt --all --check` | clean |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | clean (the `zstd` feature included) |
| `cargo test --workspace --release --no-fail-fast` | 1020 passed, 0 failed (`p6m_reports` 13, `p6m_analysis` 14, `p6m_interrupt` 1, `p6m_cli` 8, the `checkpoint` compile-fail doctest) |
| `cargo test -p gcg-eval --release --features zstd --test p6m_cli -- w99_zstd opt_ins` | 2 passed (the zstd path) |
| `target/release/gcg-diff corpus --tier 1 --require-all` | 3804/3804 (`search` 9/9, `baselines` 8/8) |
| `target/release/gcg-diff lints`, `tests-json --check`, `weaknesses --check docs/port/WEAKNESSES.md` | passed |
| `COMPARE_OUT=build/p6m/compare scripts/compare_python_vs_rust.sh equivalence --matches 2 --seeds 1 --formats bo1 --pairs red-green-zeon:blue-white-federation --workers 1 --decision-logs off` | `equivalence: 3/3 identical (fast 1/1; standard 1/1; strong 1/1; decision-log off 3/3)` |
| `gcg-sim benchmark ... --out build/p6m/w4` vs `gcg-eval benchmark ... --out build/p6m/eval-bench` (PLAN §9.11 line), `diff -r -x timing.json` | identical; stdout and stderr identical |

## Open items for the lead

1. O9 interface change (schema 1.2.0 for `gcg-sim`): `Agent::effort`, `AgentFactory::search_block`,
   then `commands::SCHEMA` and `benchmark_opts::SIM_SCHEMA` to `V1_2` together.
2. W-102 for `gcg-sim`: `cli_support::benchmark_command` to call `write_report_set`.
3. W-104: CPU and start-up times need `clock.rs`/`run.rs` (not P6-M files) or a decision to
   accept the deterministic counts.
4. W-101: engine-side interpretation tags (P6-J/P6-K).
5. W-92: the benchmark skill's sample-size text with the design effect (Phase 8).
6. `docs/port/tests.json`: register the new test files (`p6m_reports`, `p6m_analysis`,
   `p6m_interrupt`, `p6m_cli`) if the register needs them.
7. The PLAN §9.11 line `gcg-eval benchmark ... --out build/eval-bench; diff -r -x timing.json`
   can join `scripts/verify_all_rust.sh` (it passes, above).
