# P6-I (class T): evaluation tooling, status of its WEAKNESSES items

Package P6-I of docs/port/PLAN.md §12.2: W-19, W-20, W-21, W-22, W-40, W-41, W-42, W-43,
W-123, W-127, W-150, W-152, and the `gcg-eval` tools every later Phase 6 package measures with.
Built on commit 3fbb8aa. Every change is in `crates/gcg-eval/` (sources, tests, schema, fixture)
and this file; no engine, AI, runner or CLI crate was edited. This file does not edit
docs/port/WEAKNESSES.md: the lead copies the status lines in (PLAN §12.10).

All commands below run from the repository root with `export PATH=$HOME/.cargo/bin:$PATH`.
"phase6" is `cargo test --release --offline -p gcg-eval --test phase6` (fast; part of
`cargo test --workspace --release`); "phase6_slow" is `cargo test --release --offline -p gcg-eval
--features slow --test phase6_slow` (feature `slow`, PLAN A12).

## The tools (all deterministic: same seed, same bytes for any `--workers`)

| Command | What it does | Module |
| --- | --- | --- |
| `head-to-head A B` | report version 2: adds per-agent `effort` (decisions, searches, iterations, applies by purpose, worlds, capped rollouts, iterations and applies per game; null when an agent runs no search), `by_deck` (named), a pair-`stratified` interval on pooled blocks, `paired.bootstrap_ci95` (seeded bootstrap over deals) next to the Newcombe interval and McNemar counts, per-game effort in `outcomes`, and the opt-in `--decision-trace FILE` | `head_to_head.rs`, `selfplay.rs`, `stats.rs` |
| `ablation individual\|leave-one-out --mechanism F=V,...` | §12.4 rows: `search:null-<preset>` (the reference configuration as a strengthened config, every mechanism off) plus the mechanism against `ref:<preset>`; or the bundle (`--bundle SPEC` / `--search-config FILE`) against the bundle with the mechanism switched off. Defaults: panel, tuning pairs, 400 games, blocks of four dealt round-robin over the pairs, forced play/draw (A24). `summary` gives wins/N, Wilson, paired difference with Newcombe, discordant counts, iterations and applies per game, and a one-line `text` | `ablation.rs`, `rows.rs` |
| `budget-curve SEARCH OPP --scales ... --fields ...` | §12.8 step 1: the challenger's integer budget fields (default `iterations_per_option,min_iterations,max_iterations`; any other field, such as a root-tree count, by name) at each scale, `max(1, round(v * s))`, all points on the same deals | `ablation.rs` |
| `fidelity A B --variants V,... --against D` | W-18: both agents rank deck variants on the same deals; Spearman, Kendall tau-b, pairwise agreement, same best variant, per-variant paired differences; `--min-spearman X` gates | `fidelity.rs` |
| `regret CAND ORACLE` | W-43: positions sampled from recorded games (`--decisions all\|target\|<kind>`), both agents choose, both moves played out in shared worlds by the policy or `--judge SPEC`; regret per decision with its paired SE, by kind, for target decisions and overall | `regret.rs` |
| `tune --agent SPEC` | W-40: positions from self-play (or `--positions FILE`), Newton/IRLS L2 logistic fit in fixed order (port of `ai/tuning.py` `fit_logistic`/`_solve`, unrounded), k-fold CV by game, lambda by CV, held-out pairs as the check, `--validate N` wins-only row against the starting weights, `--record FILE` | `tune.rs` |
| `calibrate FILE --agent SPEC` | W-41: observed results (schema `crates/gcg-eval/schema/observed-matches.v1.schema.json`) against simulated rates: Brier, log-loss, reliability bins, ECE, bias | `calibrate.rs` |
| `compare-runs DIR_A DIR_B` | W-42: two same-seed benchmark report directories paired game by game and match by match: Newcombe paired and unpaired intervals, McNemar counts and exact p | `compare_runs.rs` |
| `decks --overlap [--check]` | W-20: main-deck copy overlaps, each row deck labelled by its largest overlap with the tuning and AI test decks; `--check` enforces the 10/50 bound on the held-out decks of panel.json | `overlap.rs` |
| `gates --e1 F --e2 F --e3-tests F --e3-panel F --e4-tests F --e4-panel F [--check]` | E1-E4 of §12.6/A26 recomputed from head-to-head `--out` reports: per-pair and pooled Wilson bounds, held-out-only pooling, report validity (agents, deck set, 28/13 pairs, 400 games per pair, `seat0`, eval/review role, agreement with panel.json), documented E3 losses | `gates.rs` |
| `evidence add FILE --command TEXT`, `evidence check` | W-127: the evidence manifest (file, command, commit, dirty flag, SHA-256) and the check that cited commits resolve and cited evidence paths and manifest files match | `evidence.rs` |
| `reference-benchmark`/`preset-benchmark --decision-trace FILE` | W-21: per-decision traces through a tracing factory; the reports stay byte-identical | `bench_cmd.rs`, `trace.rs` |

Seed roles (A14) apply to every playing tool: `--role` checks the master's range, the steering
roles (`tuning`, `dev`, `confirm`) refuse held-out pairs once panel.json exists, and every role
run is appended once to the seed ledger (default docs/port/eval/seed-ledger.json). The panel
reader accepts the §12.5 form "pairs with a held-out flag" (`pairs` as `[a, b, flag]` or objects
with `decks`/`a`,`b`/`deck_a`,`deck_b` and `held_out`/`heldout`/`role`/`split`) as well as a list
of held-out decks, and refuses a file that names neither (`decks.rs`, `Panel::from_value`).

## Items

### W-19 — Strength acceptance bars are thin floors
- **Status:** resolved (the gate tool and its test; the gate run itself is the lead's M15 step).
- **Resolution:** `gcg-eval gates` computes E1-E4 (PLAN §12.6, A26, stricter than the W-19
  plan text: E1 lower bound 0.55 pooled over all 28 and over the 13 held-out pairs, E2 point
  estimate 0.5 both ways and no pair upper bound below 0.5, E3 every game or every loss
  documented, E4 lower bound above 0.764 on the test decks, all pairs and held-out pairs) from
  head-to-head reports, after checking each report's agents, deck set, pairs, games, protocol
  and seed role; `--check` exits 1 on any failed or missing gate (`crates/gcg-eval/src/gates.rs`).
  The reference keeps criterion 7a (`tests/strength_reference.rs`, unchanged). The per-puzzle
  bars of the W-19 plan belong to the scenario packages (P6-B and the other class-S packages).
- **Evidence:** phase6 `w19_gates_compute_e1_to_e4_and_fail_below_a_bar`: passes on reports
  above every bar; E1 with 205/400 on the held-out pairs fails the held-out bound while the
  pooled-all bound passes (exit 1, "gates not passed: E1"); one loss to random fails E3 until it
  is listed with an existing replay; a report with the wrong opponent and a `dev` role fails its
  validity checks.
- **Ablation:** no AI effect. The tool reads reports; no agent, engine or search code changed.

### W-20 — Strength evidence rests on few decks; tuning decks overlap the example decks
- **Status:** resolved (tool, bound check and reporting). The bound check on the registered
  panel is one command once the deck-panel package lands panel.json and the evaluation decks:
  `gcg-eval decks --overlap --check`.
- **Resolution:** `decks --overlap` (`overlap.rs`); every head-to-head and row report gives
  `by_deck` by name and a pair-stratified interval on pooled blocks (`head_to_head.rs`); the
  example decks are labelled with their largest tuning and test overlaps and are never within
  the bound.
- **Evidence:** phase6 `w20_overlaps_reproduce_the_recorded_values` reproduces from the files
  27/50 (red-green-zeon vs tuning-zeon), 29/50 (purple-white-tekkadan vs tuning-tekkadan), 2/50
  (blue-white-federation vs tuning-wing), 27/50, 3/50, 3/50 and 2/50 (test decks vs examples), as
  WEAKNESSES.md recorded, and enforces the bound against a split (`violations`); phase6
  `w20_panel_head_to_head_and_the_overlap_bound`: a synthetic 6-deck panel head-to-head reports
  15 pairs, 6 named decks, the 5 held-out pairs pooled alone and the stratified interval, and
  `decks --overlap --check` fails on a held-out deck that copies a tuning deck (50/50) and passes
  with `--bound 50`.
- **Ablation:** no AI effect (reporting and a deck check).

### W-21 — The decision log keeps the top 5 alternatives and does not record search effort
- **Status:** resolved.
- **Resolution:** the decision log stays Python's. `--decision-trace FILE` on `head-to-head`,
  `reference-benchmark` and `preset-benchmark` writes one NDJSON line per decision (every root
  move unranked and unrounded with visits and mean, iterations, budget, cap, root count,
  `extended`, `early_stop`, applies by purpose, worlds, capped rollouts), ordered by game, seat
  and decision counter (`trace.rs`, `bench_cmd.rs`, `cli.rs`). Head-to-head and every row tool
  also sum each agent's effort per game (`selfplay.rs` `SeatEffort`). Prune exceptions are not
  traced: the agents do not expose them (`gcg_ai::SearchStats` has no such field).
- **Evidence:** phase6 `w21_decision_traces_change_no_report_and_count_every_iteration`: the
  head-to-head report is byte-identical with and without the trace; on every searched decision
  the root moves' visits sum to the iterations and their count equals the root count;
  `preset-benchmark` writes byte-identical `games.ndjson`, `results.json`, `summary.md` and
  replays with and without `--decision-trace`.
- **Ablation:** no AI effect. Tracing goes through `Agent::choose_traced`, whose action, draws,
  counter and log equal `choose`'s (phase6 `traced_games_equal_plain_games_for_every_agent_kind`
  plays random, greedy, the reference and the strengthened search both ways, both play/draw
  protocols: identical outcomes).

### W-22 — Greedy baseline: one determinization, baseline knobs, fixed choices, silent cap
- **Status:** resolved.
- **Resolution:** greedy stays the frozen, bit-exact port (Phase 3; the `baselines` corpus set).
  The decision trace counts greedy's cap hits: `trace::greedy_cap_hits` replays greedy's own
  computation from its per-decision seed (one determinisation, the pruned canonical moves, each
  completed to the next turn by the argmax policy) and counts the lines cut by
  `GREEDY_MAX_DECISIONS` and scored mid-turn (`greedy_cap_hits` in each greedy trace line).
- **Evidence:** phase6 `w22_greedy_cap_hits_are_counted` (a forced cap of 0 decisions cuts
  moves at three MAIN decisions of a real game; greedy's cap of 200 cuts no more); phase6
  `w21_...` checks that every multi-option greedy decision carries the count; `gcg-diff corpus
  --tier 1 --require-all` (baselines set included) 3804/3804 (gate below).
- **Ablation:** no AI effect (greedy unchanged; the count is a replay outside the agent).

### W-40 — Weight-tuning methodology limits
- **Status:** resolved.
- **Resolution:** `gcg-eval tune` (`tune.rs`): the target agent's own budget (`--agent`, default
  `search:standard`), the panel's tuning pairs by default (W-20), game-clustered k-fold CV
  (folds dealt by a seeded shuffle of game ids), lambda chosen by pooled out-of-fold log-loss
  (default grid 0.01, 0.1, 1, 10, 100), unrounded final weights fitted on all training positions,
  the last `--holdout-pairs` pairs as the held-out check (start vs fitted log-loss), and
  `--validate N`: a wins-only row with Wilson and paired intervals against the starting weights.
  Positions can be recorded (`--record`) and refitted (`--positions`), so leaf-point positions
  of P6-G fit through the same code.
- **Evidence:** unit `tune::tests::w40_synthetic_weights_are_recovered` (6,000 samples from a
  known 3-weight model recovered within 0.1); `tune::tests::w40_no_fold_splits_a_game`;
  `tune::tests::solve_matches_a_known_system`; `tune::tests::positions_round_trip`; phase6
  `w40_tune_is_deterministic_and_refits_recorded_positions`: identical report bytes (weights
  included) at 1 and 4 workers, the recorded positions refit to the same weights, the CV table,
  the held-out check and a validation row whose wins exclude draws. The 1/4/10-worker identity of
  the underlying games is W-150's.
- **Ablation:** no AI effect (a fitting tool; its weights reach a preset only through P6-G).

### W-41 — Matchup estimates are not calibrated against real tournament results
- **Status:** resolved by its verification (tool, schema, synthetic-fixture e2e), as PLAN §12.10
  decides. Calibration on real results is a follow-up: no tournament data was supplied (A32
  item 5).
- **Resolution:** `gcg-eval calibrate` (`calibrate.rs`) with the embedded schema
  `crates/gcg-eval/schema/observed-matches.v1.schema.json` (validated by gcg-runner's in-house
  JSON Schema validator) and the synthetic fixture
  `crates/gcg-eval/tests/fixtures/observed-matches.synthetic.json` (labelled synthetic).
- **Evidence:** phase6 `w41_calibrate_on_synthetic_fixtures`: observed counts equal to the
  simulated ones give ECE 0 and bias 0 exactly; a +2 wins per matchup shift reports a bias above
  0.1; a negative count, an impossible date and an unknown key are refused before any game;
  unit `calibrate::tests::equal_counts_have_zero_error_and_a_shift_shows_as_bias`,
  `calibrate::tests::the_schema_compiles_and_rejects_malformed_entries`.
- **Ablation:** no AI effect.

### W-42 — No paired analysis of same-seed runs; compare reports a pooled Wilson interval
- **Status:** resolved.
- **Resolution:** every head-to-head and row block pairs the two games of each deal (same game
  seed, agents swapped): the 2x2 table, McNemar's discordant counts, the paired difference with
  Newcombe's interval (method 10) and a seeded percentile bootstrap over deals (1,000 resamples)
  (`stats.rs`); `gcg-eval compare-runs` pairs two same-seed benchmark runs (`compare_runs.rs`).
  The reports themselves are unchanged.
- **Evidence:** unit `stats::tests::w42_newcombe_published_example` (Newcombe 1998, Table II,
  example (a), 36/12/2/0: 0.0569 to 0.3404; the paper could not be fetched offline, so the
  quoted value is cross-checked by the bit-exact vectors of the independent Python
  implementation in `port/oracle/export_stats.py`, `tests/vectors.rs`
  `newcombe_matches_the_python_reference`); `stats::tests::w42_bootstrap_over_deals`;
  `head_to_head.rs` test `t149_seat0_balances_play_and_draw_in_every_block` (the pairing groups
  exactly the two games of each deal, two deals per block of four); phase6
  `w42_compare_runs_pairs_same_seed_runs`: two identical same-seed runs differ by 0 with no
  discordant game and McNemar p = 1; a variant that turns 10 of 200 losses into wins has a paired
  interval excluding 0 and narrower than the unpaired one (which does not exclude 0); runs with
  different seeds are refused.
- **Ablation:** no AI effect.

### W-43 — Decision-level regret (oracle replay) tooling was never committed
- **Status:** resolved.
- **Resolution:** `gcg-eval regret` (`regret.rs`), module docs give the procedure; the oracle is
  any spec (for example a `search:` spec at 8x budget via overrides).
- **Evidence:** phase6 `w43_regret_of_an_agent_against_itself_is_zero` (12 positions, regret
  exactly 0, no playouts; identical report at 1 and 4 workers);
  `w43_random_targets_show_positive_target_regret`: `random` against `greedy` on 480 target
  decisions has positive mean target regret with a 95% interval above 0, identical report at 1
  and 8 workers (a probe run gave 0.0183, interval [0.0078, 0.0289]).
- **Ablation:** no AI effect.

### W-123 — compare.py usability limits
- **Status:** resolved.
- **Resolution:** the test and tuning sets were embedded already; `build.rs` now embeds
  `examples/decks/*.txt` and `examples/decks/eval/*.txt`, used when the working directory has no
  `examples/decks/` (`decks.rs` `built_in`); reports label decks by file stem (`by_pair`,
  `by_deck`); `ref:fast`, `ref:standard`, `ref:strong` and the baselines are named agents, and
  `search:null-<preset>` adds the reference settings as a strengthened configuration; a `search:`
  override may name any field of the configuration file format, so Phase 6 mechanism fields
  need no parser change (`agent_spec.rs` `set_search_field_generic`). `compare` keeps Python's
  behaviour exactly.
- **Evidence:** unit `agent_spec::tests::w123_every_config_field_round_trips_through_the_parser`
  (all 24 fields through `ref:` and `search:`, table and generic paths agree);
  `agent_spec::tests::null_heads_carry_the_reference_settings`;
  `decks::tests::built_in_examples_are_the_files`; phase6
  `w123_named_deck_sets_work_from_any_directory` (default set, `--decks examples` and a named
  `--pair` from an empty directory, deck names in the report).
- **Ablation:** no AI effect.

### W-127 — Investigation evidence is not reproducible
- **Status:** resolved (port side); the Python docs' dangling shas are listed for the lead below.
- **Resolution:** `gcg-eval evidence add|check` (`evidence.rs`): evidence reports live under
  docs/port/eval/evidence/ with a manifest of command, commit, dirty flag and SHA-256; the check
  resolves every commit cited as "commit SHA" in the port's evaluation Markdown and every
  `"commit"` in its JSON with `git cat-file -e`, checks every backticked docs/port/eval/ path,
  and re-hashes every manifest file. Reports are plain JSON (the zstd encoder stays opt-in, W-99).
- **Evidence:** phase6 `w127_evidence_check_resolves_commits_and_paths` (a temporary git
  repository: a resolvable commit and an existing path pass; the squashed-away 5a6f226 and a
  missing evidence path fail; an edited evidence file no longer matches its manifest entry);
  phase6 `w127_the_port_evaluation_docs_cite_only_reachable_commits` (`evidence check` on this
  repository's docs/port/eval passes); unit `evidence::tests::commits_and_paths_are_found_in_markdown`.
- **For the lead (Python docs, outside the port's files):** the shas W-127 lists as lost in the
  squash into d9d41af and cited by `docs/PROGRESS.md`: 5a6f226, fc29d5f, daf73c5, c27e523,
  ed2cedc, 4be24da, 87d8a72, 65d6e1d, 9684f29, 8eb9e8a, a339080, 3351b78, 39c5643, d79ea78,
  a32d212, 4d8bd95. Rust docs should cite evidence only through docs/port/eval/ (run
  `gcg-eval evidence check`, optionally with `--path docs/AI.md` once the Rust AI document exists).
- **Ablation:** no AI effect.

### W-150 — Determinism acceptance tests run only with the random test agent
- **Status:** resolved.
- **Resolution:** phase6_slow `w150_every_preset_is_identical_at_1_4_and_10_workers`: the
  reference `fast` and `standard` presets (`reference-benchmark`) and the shipped strengthened
  `fast` and `standard` configurations (`preset-benchmark`), on two pairings, BO1, 2 matches,
  decision logs on, plus `head-to-head search:fast ref:standard` on the same two pairings: every
  output except `timing.json` byte-identical at 1, 4 and 10 workers. The Python side (corpus and
  baselines under PYTHONHASHSEED 1 and 2) is recorded in docs/port/progress.md and
  docs/port/EQUIVALENCE.md.
- **Evidence:** the test above (118 s, ok); negative control phase6_slow
  `w150_negative_control_hash_order_breaks_the_comparison`: an agent breaking ties by iterating
  a randomly seeded std `HashMap` gives different action lists on identical runs, while the
  random agent gives identical lists at 1 and 4 workers.
- **Ablation:** no AI effect (tests only).

### W-152 — Seat and first-player symmetry was checked once and has no regression test
- **Status:** resolved.
- **Resolution:** phase6_slow `w152_seats_and_first_player_are_symmetric`: mirror-deck games
  (the same example deck in both seats, the agents choosing who goes first), 4,000 with the
  random agent and 400 with `search:fast`: the seat-0 win rate's Wilson 95% interval contains
  0.5 and the first player's win rate does not differ by seat (two-proportion z-test,
  |z| < 2.576).
- **Evidence:** the test above (ok, `--nocapture`): random, seat 0 won 1982/4000, Wilson
  [0.4800, 0.5110]; the first player won 1108/1990 from seat 0 and 1136/2010 from seat 1,
  z = -0.535. `search:fast`, seat 0 won 196/400, [0.4413, 0.5389]; first player 118/206 and
  116/194, z = -0.510. Negative control phase6_slow
  `w152_negative_control_two_extra_shields_fail_the_seat_check`: the same 4,000 random games
  with two deck cards of seat 0 moved to its Shield area at the first decision (testkit
  `move_card`, a dev-dependency only): seat 0 won 2393/4000, [0.5830, 0.6133], so the check
  fails as it must (ok).
- **Ablation:** no AI effect (tests only).

## Gate (worktree, 2026-11-02)

| Command | Result |
| --- | --- |
| `cargo fmt --all --check` | clean |
| `cargo clippy --offline --workspace --all-targets --all-features -- -D warnings` | clean |
| `cargo test --workspace --release --offline` | 905 passed, 0 failed (864 before this package: phase6 adds 17 tests, the gcg-eval unit tests grow from 18 to 42) |
| `target/release/gcg-diff corpus --tier 1 --require-all` | 3804/3804 |
| `target/release/gcg-diff lints` | passed |
| phase6_slow (`--features slow`) | 4 passed (W-150 determinism 118 s; W-152 and its negative control) |
| `cargo test --release --offline -p gcg-eval --features slow --test e2e` | 8 passed (head-to-head identical at 1/4/10 workers for every player, compare equals Python, seat0 balance) |

The reference equivalence smoke of §12.9 is `tests/parity.rs`
(`reference_presets_write_pythons_names_and_reports`, one configuration per reference preset),
part of the workspace run above. `git status` shows changes under `crates/gcg-eval/` and this
file only; `src/gcg_sim` and `tests/` are untouched.
