# P6-N (class N): tooling, docs and process, status of its WEAKNESSES items

Package P6-N of docs/dev/PLAN.md §12.2: W-110, W-111, W-112, W-113, W-114, W-115, W-117, W-118,
W-119, W-120, W-121, W-122, W-126, W-128, W-129, W-130, W-151, W-153, W-154, W-160. Built on
commit 8ae4767. This file does not edit docs/dev/WEAKNESSES.md: the lead copies the status lines
in (PLAN §12.10). Evidence files are under docs/dev/eval/P6-N/.

Every change is in the package's files: `crates/gcg-diff/src/coverage.rs`,
`crates/gcg-diff/tests/p6n_tooling.rs`, `crates/gcg-engine/src/testkit/mod.rs`, `scripts/**`
(not `normalize_outputs.py` or `compare_python_vs_rust.sh`),
`.claude/skills/gcg-refresh-data/scripts/fetch_official.py`, `oracle/tests/refresh_news/**`,
`docs/{SPEC,INTERFACES,PROGRESS,REPORTS}.md`, `docs/research/OFFICIAL_SOURCES.md`, this file
and docs/dev/eval/P6-N/. No engine, AI, runner or CLI source changed; `src/gcg_sim` and
`tests/` are untouched.

All commands run from the repository root with `export PATH=$HOME/.cargo/bin:$PATH`.
"p6n_tooling" is `cargo test --release --offline -p gcg-diff --test p6n_tooling` (fast tier).

**Ablation rows (all items).** P6-N is class N: none of its items adds a search mechanism, so
every row reads "no AI effect" (A13). No item changes code the AI executes: the changed Rust is
`gcg-diff` (a gate tool, never shipped, A31) and the `testkit` module of `gcg-engine`, which only
dev-dependencies and `gcg-diff` enable (`cargo tree -p gcg-cli -e features` has no `testkit`;
`gcg-eval` enables it only under `[dev-dependencies]`), and whose changed helpers
(`pass_all`, `to_next_turn`, `run_random`) no corpus replay calls. The rest is scripts, Python
tests outside the package and documents. The corpus and the reference smoke confirm it (gate
below).

## Items

### W-110 — Robustness fuzzing uses only random agents
- **Status:** resolved except its paranoid-build search runs: 1,000 strategic games pass with 0
  failures (greedy in the paranoid build, `search:fast` and mixed in the release build); the
  paranoid-build `search:fast` (400) and mixed (200) runs did not finish on the shared machine
  (below).
- **Resolution as implemented:** the strategic agents of `gcg-diff fuzz` (`--agent greedy`,
  `search:<preset>`, `ref:<preset>`, `mixed:A,B`; `crates/gcg-diff/src/fuzz.rs`, Phase 4) over
  Python's generated decks and seeds, with invariants after every action, in the `paranoid`
  build (the W-49 self-checks on every derived view, unlistened event and skipped rules check).
  The W-143 redirect cycle that failed one `search:fast` game (game 221, T-056 failing in
  Phase 4) is now scored inside the search by P6-O's `engine_error_leaf` instead of failing the
  real game. The 1,000 games of PLAN §9.4 are lines of `scripts/verify_all_rust.sh`.
- **Evidence:** see "W-110 strategic fuzz runs" below and
  docs/dev/eval/P6-N/w110-strategic-fuzz.txt. Random agent, Python's games: `gcg-diff fuzz
  --games 10000 --seed 2026 --workers auto --compare
  oracle/corpus/v1/tier1/fuzz/summary-2026-10000.json`: 10,000 games, 856,034 actions, 10,000
  battle-damage ends, 0 failures, 1064/1064 pool cards, summary and every per-game row equal to
  Python's, `fuzz passed` (T-055).
- **Ablation:** no AI effect (gate tool runs; no source change).

### W-111 — Coverage gates count tags, not behaviour
- **Status:** resolved for cards; rules, FAQ entries and rulings stay tag-based (reason below).
- **Resolution as implemented:** `scripts/test_census.py` runs `tests/rules` and `tests/cards`
  in one process with the executed-program census of `oracle/census.py` (a card's program
  ran when a frame of it is pushed, one of its triggers resolves, or a constant ability applies
  in a view) and records, per card-tagged test, the tagged cards whose programs ran
  (docs/dev/eval/P6-N/test-execution.json). `gcg-diff coverage --check`
  (`crates/gcg-diff/src/coverage.rs`, `judge_execution`) counts a passing per-test trace as
  execution evidence for a card only when that record shows the card's program ran in the test
  (the Rust replay reproduces every state digest of the test, so the same programs ran). A card
  with a program and no execution evidence (no executing tagged trace, no corpus census game,
  no fuzz census game) fails the check; cards whose only execution evidence is a census game
  are reported as Python-side test gaps; cards with no program at all (keyword-only, such as a
  `<Blocker>` Unit, 115) are reported apart, since no census can see a keyword the engine reads
  directly. Rules: execution of a rule's code path needs per-rule code-path ids (W-56's
  interpretation ids are data, not hooks), so rule, FAQ and ruling coverage keeps the
  traceability of PLAN §9.2 (a passing trace or native test per id).
- **Evidence:** `gcg-diff coverage --check` (passed): of <!-- count:cards.non_vanilla -->840<!-- /count -->
  non-vanilla cards, 724 run by a passing tagged trace, 1 only in a census game (GD04-089, a
  Python-side test gap: its tagged tests never run its program), 115 without a program, 0 with a
  program and no execution evidence; 545 of 2,270 card-tagged Python tests never run one of
  their tagged cards' programs (mostly negative tests that check a condition or cost and stop;
  each listed with `tagged` and `executed` in test-execution.json). Unit test
  `coverage::tests::w111_a_tagged_test_that_never_runs_its_card_is_not_execution_evidence`
  (a tagged passing trace whose record shows no run is no evidence; a failing replay is no
  evidence; a census run makes it a reported gap).
- **Ablation:** no AI effect (gate tool and a Python script outside the package).

### W-112 — The long slow suite is not deselected by default
- **Status:** resolved.
- **Resolution as implemented:** slow tests are gated by their package's `slow` feature, never
  `#[ignore]` (A12), and `scripts/slow-tier.txt` lists the slow tier. The gate runs the fast
  tier, then the slow tier: `--features "$SLOW_OFFLINE"`, and in `--full`, right after the
  tier-2 corpus is written, `--features "$SLOW_TIER2"`. `scripts/cargo_test_features.py` now
  splits the slow features into those whose tests read only committed files and those that read
  `build/oracle/tier2` (`gcg-engine/slow`), found by scanning the test sources. Before this, the
  gate ran `gcg-engine`'s tier-2 test on a clean clone, where the tier-2 files do not exist, so
  the gate could not pass.
- **Evidence:** p6n_tooling `w112_the_slow_tier_is_the_listed_one_and_the_gate_runs_both_tiers`
  (the slow-gated targets and test functions of every package equal `scripts/slow-tier.txt`,
  seven entries; every `[[test]]` requires only `slow`, `paranoid` or `testkit`; no ignored
  test; the gate holds the three `cargo test` steps) and
  `w112_slow_gating_is_recognised_on_files_and_functions`. `cargo metadata ... | python3
  scripts/cargo_test_features.py`: `SLOW_OFFLINE=gcg-diff/slow,gcg-eval/slow,gcg-search/slow`,
  `SLOW_TIER2=gcg-engine/slow`.
- **Ablation:** no AI effect (tests and gate script).

### W-113 — Test helpers stop silently at step caps
- **Status:** resolved.
- **Resolution as implemented:** `pass_all`, `to_next_turn` and `run_random` of the Rust
  testkit (`crates/gcg-engine/src/testkit/mod.rs`) fail with an `AssertionError`-style
  `TestkitError::Assertion` naming the helper, the cap and the pending decision when the cap
  runs out before their goal (the action steps and blocks ended, the next MAIN decision, the end
  of the game). `pass_all_allow_cap`, `to_next_turn_allow_cap` (with a cap argument) and
  `run_random_allow_cap` return `CapOutcome::Capped` instead, for a test that wants the cap.
  Signatures of the existing helpers are unchanged (additive API: `CapOutcome`,
  `TO_NEXT_TURN_STEPS` and the three `*_allow_cap` functions).
- **Evidence:** unit tests `testkit::tests::w113_pass_all_fails_at_its_cap_and_the_allow_cap_variant_stops`,
  `w113_to_next_turn_fails_at_its_cap_and_reaches_main_within_it`,
  `w113_run_random_fails_when_the_game_is_not_over_at_its_cap`
  (`cargo test --release --offline -p gcg-engine --lib testkit`: 5 passed). Every ported test
  passes without `allow_cap` (no caller uses it; the helpers' callers in
  `crates/gcg-engine/tests/{lead_examples,once_used,p6k_behaviour}.rs` and
  `crates/gcg-search/tests/{ai_agents.rs,aiport/positions.rs}` pass in the workspace run below).
- **Ablation:** no AI effect (the testkit is not in any shipped binary).

### W-114 — The documented-command checker
- **Status:** resolved for the checker; the refresh-skill documents join its default set once
  their blocks carry markers (lead note 3).
- **Resolution as implemented:** `scripts/check_doc_commands.py` runs every block in its own
  process group, with output to a file, and kills the group when the block exits (background
  jobs cannot outlive it) and at the timeout (the block fails, "process group killed").
  `<!-- doc-check: network <reason> -->` marks a block that needs the network: listed and not
  run, except with `--network` (the network job). `<!-- doc-check: rust -->` before a ```rust
  block makes it a Rust example: compiled and run as a program (wrapped in `fn main`) in a
  scratch Cargo project depending on the workspace crates it names, `--offline`, into
  `target/doc-check`; unmarked ```rust blocks stay sketches (the port documents hold 36).
- **Evidence:** `python3 scripts/check_doc_commands.py --selftest`: `check_doc_commands selftest
  passed` (a plain block, a skip, a network block listed offline and run with `--network`, a
  Rust sketch left out, a Rust example compiled and run, a failing Rust example caught, a block
  starting `sleep 600 &` leaves no process after it exits, and a block waiting on `sleep 600`
  with a 2 s timeout is stopped with its whole group). `--list` over the default documents: the
  three existing skip markers still apply.
- **Ablation:** no AI effect (a documentation tool).

### W-115 — Research artifacts are pinned to an old snapshot and outside the gate
- **Status:** resolved.
- **Resolution as implemented:** `scripts/gate_summary.py` (standard library; run by the gate)
  reads the data commit each research artefact states (the "Pinned data commit" row of
  DATA_INGESTION.md, `snapshot.pinned_commit` of data_profile.json, `source.gcg_api_commit` of
  effect_templates.json and work_packages.json) and the packaged commit (`gcgapi:commit` of
  `data/SOURCES.lock.json`) and prints a non-failing `stale research artifact` warning.
  Regenerating them is Python-side work (lead note 6).
- **Evidence:** `python3 scripts/gate_summary.py`: "WARNING: stale research artifact
  docs/research/DATA_INGESTION.md: profiles f57b7c0, the packaged snapshot is 39c85b5", the same
  for data_profile.json, and effect_templates.json and work_packages.json current. p6n_tooling
  `w115_w153_the_gate_summary_flags_stale_research_and_shows_the_news_scan_date` and
  `w115_w153_negative_controls_follow_the_packaged_commit_and_the_news_dates` (a copy whose lock
  names f57b7c0 makes DATA_INGESTION.md current and effect_templates.json stale; a malformed
  lock fails the script).
- **Ablation:** no AI effect (a report line).

### W-117 — The adversarial review was a sample
- **Status:** resolved.
- **Resolution as implemented:** `gcg-diff coverage --check` prints the four-part differential
  report (`coverage.rs`, `execution_report`, `judge_fuzz`): golden records and tagged tests; deck
  membership in the fuzz games, recomputed by the Rust deck generator for the census's games and
  compared with the census; execution per pool card with a program, each that never ran listed
  with the passing tagged test covering it; and the
  <!-- count:cards.effect_only -->88<!-- /count --> effect-only numbers, each created in fuzz, or
  instantiated in a passing per-test trace, or a vanilla printing with the characteristics of a
  created one (the alternate EX Base and EX Resource printings, which the engine never
  instantiates: it creates `ex_base` and `ex_resource` only), or covered by a tagged test, with
  a golden record. Any card without that evidence fails the check.
- **Evidence:** `gcg-diff coverage --check` (passed): part 1 1152/1152 numbers with a golden record
  and 1152/1152 with Rust evidence; part 2 1064/1064 (10,000 games, seed 2026, 0 failures); part 3
  708 of 724 pool cards with a program ran, the 16 others each with its passing tagged trace
  (for example GD01-003: `test_gd01_003_does_nothing_with_fewer_than_twelve_trash_cards`); part 4
  29 numbers created in fuzz (EXB-001 20,362 times, EXR-001 12,233, T-001 ... T-029), 57
  alternate EX printings covered as printings of EXB-001 or EXR-001, T-022 and T-025
  instantiated in passing per-test traces (`test_gd04_017_newtype_pilot_deploys_two_wire_guided_arms`,
  `test_eb01_022_end_of_turn_trade_for_three_tokens`): 88/88. Unit test
  `coverage::tests::w117_an_uncreated_effect_only_number_needs_a_trace_an_identical_printing_or_a_tag`.
- **Ablation:** no AI effect (gate tool).

### W-118 — verify_all.sh gate limits
- **Status:** resolved for the Rust gate; the Python gate's own limits are listed for the lead
  (lead note 7).
- **Resolution as implemented:** `scripts/verify_all_rust.sh` runs on a clean clone of HEAD:
  fmt; clippy (plain, and with every slow, paranoid and testkit feature); the fast and slow test
  tiers and the paranoid engine tests; `lints`, `selftest` and `selftest --real-engine`; the
  tier-1 corpus parse-only, mock, roundtrip, real engine and paranoid; `coverage --check`,
  `traceability --check` (regeneration with a diff check), `tests-json`, `weaknesses`; the
  10,000-game fuzz against Python's summary, the 2,000-game commit-seed fuzz, `paranoid-games`
  and the 1,000 strategic paranoid games; `gcg-eval decks --overlap --check` and `evidence
  check`; the dependency-closure checks; validation of every deck; `gcg-sim`, `gcg-eval
  reference-benchmark` and `head-to-head` at 1/4/10 workers with normalised diffs; replay of
  every saved replay; schema checks of the written reports, the recorded reports and the
  invalid-report negative controls; the `--ai-preset` usage errors; the `apple-m2` dist build and
  `scripts/verify_binary_offline.sh`; the stdlib checks (doc-checker selftest, its own
  negative controls, the CLI help snapshot, the settings check selftest, the generated counts,
  the freshness summary); the A15 check. `--full` adds ruff, ruff format and mypy, the doc
  checker over every document, the transcripts and fuzz-census selftests, the fuzz and test
  census `--check`s, the Python corpus, census, panel and P6-K checks, `pytest oracle/tests`,
  the comparison-script selftests and the reference equivalence smoke, and the tier-2 corpus
  with its slow test. The steps run in a subshell: macOS `/bin/bash` 3.2 exits 0 from a `set -u`
  error when an EXIT trap is set, a hole found by the new negative controls (fixed the same way in
  `verify_binary_offline.sh`). `scripts/verify_binary_offline.sh` now benchmarks all three example
  pairings at 1, 4 and 10 workers, compares `results.json`, `games.ndjson` and the replays
  byte for byte and replays every saved replay. Artefacts are chosen by path, never by sort order.
- **Evidence:** `scripts/verify_all_rust_selftest.sh`: `verify_all_rust selftest passed` (a
  failing step, a failing pipeline and an unset variable each fail a copy of the gate with exit
  1 before the next step; the control `true` passes; no gate step is guarded by `|| true`);
  `scripts/verify_binary_offline.sh target/release/gcg-sim`: `offline binary check passed:
  gcg-sim (3 pairings, workers 1/4/10)`. The clean-clone run is under "Gate" below.
- **Ablation:** no AI effect (gate scripts).

### W-119 — blind_transcripts.py uses a private helper and path-dependent ids, untested
- **Status:** resolved for the script; the Rust `gcg-eval transcripts` subcommand is not
  written (lead note 2).
- **Resolution as implemented:** `scripts/blind_transcripts.py` builds decklists itself from the
  replay's counts (no private `_decklist`), checks each replay with the public
  `check_replay` first, and names each transcript by a hash of the game alone (seed, chooser,
  caps, both decklists, actions), so the same game has one id wherever its replay lives and
  agents, agent seeds and logs never reach the id; the key lists every file of a game.
  `--selftest` is its test.
- **Evidence:** `uv run python scripts/blind_transcripts.py --selftest`: `blind_transcripts selftest
  passed` (a real `fast` BO1 replay with a decision log, copied into two directories under
  another name: one id with both paths; the transcript holds moves and none of "visits",
  "iterations", "mean", "alternatives", "mcts", "search", "greedy", "agent"; the id ignores the
  agents and follows the actions).
- **Ablation:** no AI effect (an audit script).

### W-120 — Project Claude permissions are broader than the network rule
- **Status:** open, pending the user. PLAN §0.1: "A32 (3): W-120 | Deferred to the user, asked
  once near the end of the port (it changes this project's Claude permission settings, which
  only the user may decide). Nothing else depends on it | Global instructions: permission
  changes belong to the user".
- **Done in scope:** the verification check, `scripts/check_claude_settings.py` (reads only).
  On the current `.claude/settings.json` it reports the two findings and exits 1: `Bash(git -C *
  )` has whitespace at the edge of its pattern, and `Bash(curl -sS *)` lets curl reach any host.
  `--selftest` (in the gate) passes, including the settings as the proposed change leaves them
  (a host-pinned curl rule and `Bash(git -C *)`). The settings file is unchanged.
- **Ablation:** no AI effect.

### W-121 — Refresh tooling: only the offline core is tested; official fetches need a TLS workaround
- **Status:** resolved.
- **Resolution as implemented:** the refresh workflow stays in Python. Rust side: p6n_tooling
  `w121_the_card_loader_accepts_recorded_records_and_names_a_renamed_field` loads one recorded
  `cards.ndjson` record of each of the 8 card types (the packaged file is byte-identical to the
  pinned gcg-api file) through the public `CardDb::from_sources`, and renaming `card_number`,
  `card_type` or `product_id` fails with an error naming `cards.ndjson` and the field. Python
  side: recorded-response tests of the fetch script's news discovery (W-153). TLS: the fetch
  keeps verifying against certifi's bundle (`--with certifi`), as documented in the script.
  `cards.ndjson` is the only fetched format the Rust runtime parses from text; `manifest.json`
  and the official JSON files are embedded and have no public from-text loader (lead note 12).
- **Evidence:** the test above (pass).
- **Ablation:** no AI effect.

### W-122 — Untracked bytecode of an unmerged later branch
- **Status:** resolved by the lint (PLAN §0.1 A32 (6): the bytecode is kept; deleting it is the
  user's option).
- **Evidence:** `target/release/gcg-diff lints`: `lints passed` (rule 6 over `crates/**`, the
  manifests and the Rust gate scripts; its fixture test `crates/gcg-diff/tests/lints.rs`); p6n_tooling
  `w122_the_p6n_gate_scripts_never_name_the_untracked_bytecode` applies rule 6 to every script
  this package added.
- **Ablation:** no AI effect.

### W-126 — Throughput measurement
- **Status:** resolved as a tool; its 3 % reproducibility check is not verified (reason below).
- **Resolution as implemented:** `scripts/throughput.py` (outside the crates) times
  `gcg-eval head-to-head` on the three example pairings (or `--pairs`), in whole blocks of four
  (`--games` is rounded up with a message), repeated `--runs` times. Per run: wall time, CPU time
  of the child from `getrusage`, the report's deterministic effort counts and the load average
  before the run; per pairing the minimum, median and maximum of CPU s per game and of
  iterations, applies and decisions per wall second and per CPU second, and a warning if the
  counts differ between runs; the machine's CPU, core tiers and memory. It reads no power
  state: the brief's operating assumption forbids power-state handling and caveats
  (docs/port/BRIEF.md <target_hardware>), which overrides that part of the W-126 plan.
- **Evidence:** `python3 scripts/throughput.py search:fast --opponent ref:fast --games 3 --runs 2
  --pairs red-green-zeon:blue-white-federation --workers 2`: "--games 3 rounded up to 4: whole
  blocks of 4 games", identical effort counts in both runs. Not verified: "two runs on an idle
  machine agree within 3 % on the minimum"; this machine was not idle (load average 130-330
  from parallel agents during this package), so the check needs a quiet machine.
- **Ablation:** no AI effect.

### W-128 — Stale counts across status.json and the research docs
- **Status:** resolved for the port's documents; the stale Python-doc counts are listed for the
  lead (lead note 6).
- **Resolution as implemented:** `scripts/doc_counts.py` computes 14 named counts from the
  packaged data and committed records (card numbers <!-- count:cards.numbers -->1,152<!-- /count -->,
  definitions <!-- count:cards.definitions -->1,153<!-- /count -->, fuzz pool
  <!-- count:cards.fuzz_pool -->1,064<!-- /count -->, testable rules
  <!-- count:rules.testable -->395<!-- /count -->, tier-1 traces
  <!-- count:corpus.tier1.traces -->3,804<!-- /count -->, and the others of its docstring) and
  checks every `<!-- count:NAME -->VALUE<!-- /count -->` in `docs/dev/**/*.md`; `--write`
  regenerates them. The counts in this file are such markers. The gate runs `--check`.
- **Evidence:** p6n_tooling `w128_marked_counts_are_regenerated_from_the_data` (a correct marker
  passes; "1,148" for `cards.numbers` fails naming it; an unknown count fails; the port's
  documents pass).
- **Ablation:** no AI effect.

### W-129 — Hand-maintained docs drift from the code
- **Status:** resolved for the CLI interface and the listed `docs/*.md` corrections; the public
  API signature snapshot is not done (reason below).
- **Resolution as implemented:** `scripts/cli_help_snapshot.py` writes the `--help` of
  `gcg-sim`, `gcg-eval` and `gcg-diff` and of every subcommand their usage lists (25 texts) into
  docs/dev/eval/P6-N/cli-help.txt; the gate's `--check` fails on any change until the snapshot is
  regenerated. `gcg-sim`'s help also equals Python's (T-152). Corrections: docs/SPEC.md (Python
  ≥3.15, as `pyproject.toml` requires); docs/INTERFACES.md (`validate_deck(..., implemented: bool
  = True)`, `data status [--json]`, `replay REPLAY_JSON [--log]`); docs/PROGRESS.md (the `fast`
  values were provisional at that step and fixed later, docs/python/AI.md "The `fast` preset").
  The mirrored-block statement is checked by `crates/gcg-eval/tests/vectors.rs::mirrored_specs_match_selfplay`
  (the Rust generator equals Python's `mirrored_specs` on fixed masters). Not done: a snapshot of
  the public Rust API signatures (it needs rustdoc JSON, a nightly feature this stable toolchain
  lacks; docs/dev/contracts.md stays the frozen signature document), and the Python-side wording
  of `docs/python/AI.md:326-327` and the `mirrored_specs` docstring (lead note 6).
- **Evidence:** `python3 scripts/cli_help_snapshot.py --check`: matches the binaries.
- **Ablation:** no AI effect.

### W-130 — The shuffle-cut and chooser-procedure wording
- **Status:** resolved (docs); `src/gcg_sim/data/official/floor_rules.json` stays unchanged
  (packaged data) and is listed for the lead.
- **Resolution as implemented:** docs/research/OFFICIAL_SOURCES.md now says what the engine does:
  each main deck is shuffled and then cut once at a seeded random point (`engine/game.py:137-140`),
  which consumes an RNG draw and is reproduced draw for draw. docs/REPORTS.md says a seeded fair
  draw (`randrange(2)`, `engine/game.py:153`) picks the chooser, which is what floor_rules.json's
  "fair coin flip" and the old "die roll" both describe.
- **Evidence:** `crates/gcg-engine/tests/l5_setup.rs::new_game_shuffles_cuts_and_numbers_cards_as_python`
  (Rust `new_game` gives Python's digest, RNG state, card order and chooser for every exported
  seed) passes in the workspace run.
- **Ablation:** no AI effect (documents).

### W-151 — Fuzz coverage counts cards in decks, not cards executed
- **Status:** resolved.
- **Resolution as implemented:** `scripts/fuzz_census.py` plays the seed-2026 fuzz games of
  `gcg_sim.tools.fuzz` (same decks, seeds, agent draws and invariant checks) under the census and
  records per card number the games whose decks held it, the games in which one of its programs
  ran, and the instances that effects or the setup created
  (docs/dev/eval/P6-N/fuzz-census-2026-10000.json). The Rust fuzz plays the same games (T-055
  compares the summary and every per-game row), so this is the Rust run's census; the Rust deck
  generator's membership must equal the census's. `gcg-diff coverage --check` reports it (W-117
  parts 2-4). The commit-seeded job is `gcg-diff fuzz --games 2000 --seed-from-commit SHA`
  (prints the seed), in the gate.
- **Evidence:** `uv run python scripts/fuzz_census.py --selftest`: passed (in three real games, 37
  deck cards none of whose copies left the deck, none counted as executed; every executed card
  was in a deck or created); the census: 10,000 games, 0 failures, 1064 pool numbers, 787 numbers
  whose programs ran. Unit test
  `coverage::tests::w151_a_card_in_a_deck_whose_program_never_ran_is_reported_as_not_executed`.
- **Ablation:** no AI effect.

### W-153 — News and errata discovery in the refresh skill
- **Status:** resolved (the script and its recorded-response tests; the SKILL.md wording is the
  lead's, lead note 4).
- **Resolution as implemented:** `.claude/skills/gcg-refresh-data/scripts/fetch_official.py`:
  the cutoff is `--since` or the newest `retrieved_at` of the news listing sources; `fetch` pages
  through the US and Asia all-news listings (`NEWS_LISTINGS`) until a page holds a dated entry
  before the cutoff or the listing's last page (pinned "pickup" entries do not stop it, being out
  of date order), and an exhausted `--news-max-pages` (default 30) is an error (exit 1). Every
  uncached entry on or after the cutoff is fetched into `news/`, flagged or not; `news` flags an
  entry when it is RULES-tagged, its title matches the old pattern, or its text names a card
  number or says errata, prints the reasons, and exits 1 when a listing or entry was not read.
  Rust gate side: `scripts/gate_summary.py` prints the newest `retrieved_at` of the news sources.
- **Evidence:** `uv run pytest oracle/tests/refresh_news -q`: 9 passed (T-155): the recorded
  US page 1 parses with its pager and pinned entries; page 3 still ends after the cutoff, so the
  stop rule reads page 4; a cap of 3 is an error; a pinned July entry does not stop the Asia
  paging, its last page does; flags by text ("names GD03-015" on an unusual title, "says errata");
  an end-to-end fetch with a recording getter reads every listing to the cutoff and fetches all
  20 uncached entries; a capped fetch fails `fetch` and `news`; an old manifest without a scan is
  reported incomplete. p6n_tooling `w115_w153_*`: the gate line's date equals the newest
  `retrieved_at` of the news listing sources (2026-10-15T20:03:21Z), and follows a changed date.
- **Ablation:** no AI effect.

### W-154 — Untracked port artifacts would break the project's Python gates
- **Status:** resolved before the first port commit (docs/port/progress.md, Phase 0); one new
  regression found and listed (lead note 5).
- **Evidence:** `uv run ruff check .`: All checks passed; `uv run ruff format --check .`: 233 files
  already formatted; `MYPYPATH=src uv run mypy --strict oracle`: no issues in 33 files;
  `uv run mypy`: no issues in 101 files. `check_doc_commands.py --list`: the two inventory blocks
  are skipped with reasons. New: `docs/dev/eval/weakness-status/P6-decks.md` (line 106) holds a
  bash-fenced block with `--out ...` and no marker, so `check_doc_commands.py` (criterion 10)
  fails on it.
- **Ablation:** no AI effect.

### W-160 — The Python-vs-Rust equivalence harness
- **Status:** resolved (harness of Phase 3; evidence recorded here).
- **Evidence:** `scripts/compare_python_vs_rust.sh equivalence --selftest`: 15/15 passed (raw-byte
  comparison with only the three masked fields; `1e-5` for `1e-05`, a missing newline and the
  other negative controls fail; stale-baseline stamps; every cited doc exists; the default matrix
  has `fast` and decision-log runs); `report --selftest`: 14/14 passed; the reference
  equivalence smoke under "Gate" below.
- **Ablation:** no AI effect.

## W-110 strategic fuzz runs

Seed 2026, Python's generated decks, invariants after every action; outputs in
docs/dev/eval/P6-N/w110-strategic-fuzz.txt. "paranoid" is the build `cargo build --release
--offline -p gcg-diff --features paranoid --target-dir target/paranoid` with `--paranoid`;
"release" is `target/release/gcg-diff`.

| Run | Build | Games | Actions | Ends | Failures |
| --- | --- | --- | --- | --- | --- |
| `--agent greedy` | paranoid | 400 | 31,936 | 400 battle damage | 0 (`fuzz passed`) |
| `--agent search:fast` | release | 400 | 27,490 | 400 battle damage | 0 (`fuzz passed`; game 221, the W-143 cycle, completes) |
| `--agent mixed:greedy,search:fast` | release | 200 | 14,558 | 200 battle damage | 0 (`fuzz passed`) |
| `--agent search:fast` | paranoid | 400 | not completed | | |
| `--agent mixed:greedy,search:fast` | paranoid | 200 | not completed | | |

Every run's decks cover the 1064 pool cards. The two release runs give exactly P6-O's numbers
(27,490 and 14,558 actions), so they are deterministic across builds of the same sources.
**Not done:** the paranoid-build runs of the two search agents. In the paranoid build every
cached derived view the search reads is recomputed and compared, which multiplies the cost of a
search decision; on this shared machine (load average 100-330 from parallel agents) the
`search:fast` run had used 254 CPU minutes in 3 h 25 min without finishing, and it stops with
this package's session. They are the gate's lines (PLAN §9.4, T-056 second part) and need a
quieter machine: `target/paranoid/release/gcg-diff fuzz --games 400 --seed 2026 --agent
search:fast --paranoid --workers auto` and the `mixed:greedy,search:fast` one with `--games 200`.

## Interface notes for the lead

1. `docs/dev/tests.json`: T-155 is passing (W-153 above). T-056: the 2,000-game commit-seed run
   and the greedy paranoid run pass; it stays failing until the paranoid `search:fast` and mixed
   runs complete (W-110 runs above). The new tests (p6n_tooling, the coverage, testkit and script
   selftests) need ids.
2. W-119: the Rust `gcg-eval transcripts` subcommand of the W-119 plan is not written (gcg-eval
   is outside this package); the Python script is fixed and tested.
3. W-114: to add the refresh skill's documents to the doc checker's default set, mark their
   blocks (`.claude/skills/gcg-refresh-data/SKILL.md` and `references/card-checklist.md`: the
   fetch blocks `doc-check: network`, the procedural blocks with `<...>` placeholders or
   `$SCRATCH` `doc-check: skip <reason>`) and add `.claude/skills/gcg-refresh-data/**/*.md` to
   `DEFAULT_FILES`.
4. W-153: `.claude/skills/gcg-refresh-data/SKILL.md` step 2 still says `fetch` reads "news listing
   pages 2–3" and that RELEVANT entries are RULES-tagged or match the title words. Replacement:
   "`fetch` also pages through the US and Asia all-news listings back to the news cutoff (the
   newest `retrieved_at` of the news listing sources, or `--since`), fails if
   `--news-max-pages` runs out first, and fetches every uncached entry since the cutoff into
   `news/`. `compare` lists the listing pages as `not cached (new page)`. `news` flags an entry
   RELEVANT when it is RULES-tagged, its title mentions errata, banned, restricted, rule,
   correction, revision, FAQ or comprehensive, or its text names a card number or says errata;
   it prints the reasons and exits 1 when a listing or entry was not read."
5. W-154: give `docs/dev/eval/weakness-status/P6-decks.md` (line 106) a `text` fence or a
   `<!-- doc-check: skip <reason> -->` marker; until then `check_doc_commands.py` and the
   `--full` gate fail on it.
6. Python-side documents and data (outside this package): `docs/python/AI.md:326-327` and the
   `mirrored_specs` docstring (`ai/selfplay.py:95-97`, §12.11 post-acceptance) should say that a
   block of four holds two deals of two games; `src/gcg_sim/data/official/floor_rules.json`
   "Seeded fair coin flip" is accurate (`randrange(2)`) and stays; research artefacts
   DATA_INGESTION.md and data_profile.json profile f57b7c0, not the packaged 39c85b5
   (`scripts/ingest_profile.py` with a clone at 39c85b5); the stale counts of W-128's entry
   (status.json 1148 cards and 395 conflicts, RULES_INDEX 399/180, EFFECT_TEMPLATES prose 1,148 /
   1,912 / 308 / 189, DATA_INGESTION run 25 and 1,912 printings, Edition Beta 83/70 against 85).
7. W-118, Python gate: `scripts/verify_all.sh` and `scripts/verify_wheel_offline.sh` have the same
   bash 3.2 `set -u` / EXIT-trap hole, and `verify_all.sh` picks the wheel with `ls ... | tail -n
   1`; the W-118 resolution is the Rust gate, so they are unchanged.
8. `coverage --check` now reads docs/dev/eval/P6-N/test-execution.json and
   fuzz-census-2026-10000.json; regenerate them with `scripts/test_census.py --out` and
   `scripts/fuzz_census.py --out` (the `--full` gate checks both) if the tests or the fuzz change.
   The CLI snapshot docs/dev/eval/P6-N/cli-help.txt needs `scripts/cli_help_snapshot.py --write`
   after every merge that changes a `--help` (P6-M's `gcg-eval benchmark` will).
9. `crates/gcg-diff/src/lints.rs` `GATE_SCRIPTS` could list the new gate scripts; p6n_tooling
   applies rule 6 to them meanwhile.
10. `gcg-data`: public from-text loaders for `manifest.json` and the official JSON files would let
    W-121's fixture test cover those formats too (the P6-K notes 1-2 move the same code).
11. W-120's settings change waits for the user's answer.
12. `target/release/gcg-diff traceability --check` fails at the base commit 8ae4767, before any
    P6-N change: P6-K (7614bec) added native tests tagged `// gcg-tags:` in
    `crates/gcg-engine/tests/p6k_behaviour.rs` (for example `w66_two_target_commands_need_both_targets`
    on rules 10-1-8-1-1, 10-2-2 and 10-3-3-1) after docs/RULES_TRACEABILITY.md and
    docs/dev/TRACEABILITY_CARDS.md were last regenerated (3fbb8aa). `gcg-diff traceability
    --write` regenerates both (lead files; P6-N adds no tag, so its regeneration only adds the
    P6-K rows); until then `scripts/verify_all_rust.sh` stops at that step, and the slow test
    `crates/gcg-diff/tests/corpus_replay.rs::coverage_and_traceability_hold` fails on its
    traceability half.

## Gate

Run in this worktree after the last source edit (2026-11-02, M2 Pro, shared with other agents:
load average 120-330 throughout, so no duration here is a measurement).

| Command | Result |
| --- | --- |
| `cargo fmt --all --check` | clean |
| `cargo clippy --offline --workspace --all-targets --all-features -- -D warnings` | clean |
| `cargo test --workspace --release --offline --no-fail-fast` | 993 passed, 0 failed (980 at the base: + 3 testkit, 3 coverage, 7 p6n_tooling) |
| `cargo test --release --offline -p gcg-engine --features paranoid,testkit --target-dir target/paranoid` | 224 passed, 0 failed |
| `target/release/gcg-diff corpus --tier 1 --require-all` | 3804/3804 traces (baselines 8/8, coverage 50/50, full 4/4, fuzz 100/100, search 9/9, tests 3213/3213, vanilla 40/40, views 280/280, worlds 100/100) |
| `target/release/gcg-diff lints` | lints passed |
| `target/release/gcg-diff selftest --real-engine` | 36 negative controls fired |
| `target/release/gcg-diff coverage --check` | coverage check passed (with the W-111 and W-117 parts above) |
| `target/release/gcg-diff traceability --check` | fails, as at the base commit (lead note 12) |
| `target/release/gcg-diff tests-json --check`, `weaknesses --check docs/dev/WEAKNESSES.md` | passed |
| `target/release/gcg-diff fuzz --games 10000 --seed 2026 --workers auto --compare ...` | 0 failures, equal to Python (T-055) |
| `target/release/gcg-diff fuzz --games 2000 --seed-from-commit SHA --workers auto`, SHA this package's first commit on its branch | seed 11348187240972289486; 2,000 games, 172,794 actions, 0 failures (T-056, first part) |
| `target/release/gcg-eval decks --overlap --check`, `evidence check` | pass |
| `scripts/compare_python_vs_rust.sh equivalence --matches 2 --seeds 1 --formats bo1 --pairs red-green-zeon:blue-white-federation --workers 1 --decision-logs off` (COMPARE_OUT under the worktree's build/) | 3/3 identical (fast, standard, strong); Python baselines identical under hash seeds 1 and 2 |
| `scripts/compare_python_vs_rust.sh equivalence --selftest`, `report --selftest` | 15/15, 14/14 |
| `scripts/verify_all_rust_selftest.sh`, `scripts/verify_binary_offline.sh target/release/gcg-sim` | passed |
| `python3 scripts/check_doc_commands.py --selftest`, `cli_help_snapshot.py --check`, `check_claude_settings.py --selftest`, `doc_counts.py --check`, `gate_summary.py` | pass (gate_summary prints the two stale-research warnings) |
| `uv run pytest oracle/tests -q` | 25 passed |
| `uv run python scripts/test_census.py --check docs/dev/eval/P6-N/test-execution.json`, `fuzz_census.py --games 10000 --seed 2026 --workers 8 --check docs/dev/eval/P6-N/fuzz-census-2026-10000.json` | both records regenerate byte for byte |
| `uv run python scripts/fuzz_census.py --selftest`, `blind_transcripts.py --selftest` | passed |
| `uv run ruff check .`, `uv run ruff format --check .`, `MYPYPATH=src uv run mypy --strict oracle`, `uv run mypy` | clean |
| `git diff --quiet d9d41af -- src/gcg_sim tests` | unchanged |

`scripts/verify_all_rust.sh` was not run end to end on a clean clone: it stops at `traceability
--check` until the lead regenerates the two traceability documents (lead note 12), and its
fuzz, strength and slow-suite steps take hours on this shared machine. Every step it holds was
run on its own above or under W-110, except the slow-feature `cargo test`, the paranoid-build
corpus, `paranoid-games`, the paranoid-build `search:fast` and mixed fuzz runs and the dist
build; none of them runs code this package changed except the slow `corpus_replay.rs`, whose
coverage half is `gcg-diff coverage --check` above.
