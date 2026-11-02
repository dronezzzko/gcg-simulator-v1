# WEAKNESSES.md: items not fully resolved

This file lists every item of `docs/port/WEAKNESSES.md` whose status is `open`, as of 2026-11-02 at HEAD `d750b6c`. For each item it gives:

- what is missing;
- why it is missing;
- the work that would close it;
- whether it is an AI item that needs ablation rows (PLAN §12.4);
- an effort estimate.

The statuses come from the package records in `docs/port/eval/weakness-status/` and were checked against the repository. Each entry's "Status" line in WEAKNESSES.md gives the same remaining work in short. This file also lists:

- resolved items whose slow evidence ran on the presets from before M15;
- resolved items that carry a follow-up or a hand-off;
- bookkeeping for `docs/port/tests.json`.

The final strength evaluation (E1-E4) was still running when this file was written.

Effort: **S** is under a day, or one command plus its record. **M** is one to three days, or several hours of compute. **L** is more than three days.

AI row: "yes" means the work changes a strengthened-search mechanism or adds one. Such work needs an individual row, and a leave-one-out row from the shipped bundle if the mechanism is adopted (§12.4, §12.10). "no" means a non-AI item. It needs the A13 "no AI effect" evidence: the `search` and `baselines` corpus sets at 100%, plus the seed-1 equivalence smoke, wherever it touches code the AI executes.

## Counts

`target/integrate/release/gcg-diff weaknesses --check docs/port/WEAKNESSES.md` passes. With `--require-resolved` it reports 33 problems, one per open item:

| Status | Items | Count |
| --- | --- | ---: |
| resolved | — | 106 |
| resolved by measurement (not adopted) | W-02, W-06, W-07, W-09, W-10, W-11, W-12, W-16, W-23, W-24, W-27, W-28, W-29, W-30, W-34, W-35, W-37, W-38, W-45, W-138, W-140, W-141 | 22 |
| admissible exception | W-131, W-132, W-133 | 3 |
| open: final evaluation pending | W-04, W-19 | 2 |
| open: blocked on an engine interface | W-08, W-134 | 2 |
| open: the user's decision | W-120 | 1 |
| open: remaining work | W-05, W-13, W-14, W-17, W-18, W-25, W-26, W-31, W-36, W-63, W-64, W-67, W-75, W-76, W-88, W-101, W-104, W-110, W-111, W-118, W-119, W-126, W-129, W-137, W-139, W-142, W-145, W-154 | 28 |

The 33 open items fall into four kinds:

- 18 are non-AI items.
- 13 are AI items (class S): W-04, W-05, W-08, W-13, W-14, W-17, W-18, W-25, W-26, W-31, W-36, W-139, W-142.
- W-134 is an AI item and a design decision.
- W-137 has a non-AI part and an AI part (adopting `portable_math`).

## 1. Lead: the final evaluation (docs/port/eval/final/)

### W-04: budget plateau (acceptance)
- **Missing:** the acceptance run. Strengthened `standard` must beat reference `strong` with a Wilson lower bound above 0.5 on a held-out seed run once.
- **Why:** it is gate E1 (§12.6), which is still running.
- **Work:** when E1 finishes, copy its pooled row into the entry: wins/N, Wilson, paired Newcombe, discordant counts, iterations and applies per game, held-out pooled bound. Then change the status to resolved. The mechanism part is done: `root_trees` is resolved by measurement and not adopted.
- **AI row:** yes, E1 is the row. **Effort:** S once E1 is done.

### W-19: strength acceptance bars
- **Missing:** the gate run of E1-E4 on eval-role seeds (T-124 to T-127). The tool `gcg-eval gates` and its test are done.
- **Why:** the final evaluation is still running.
- **Work:** run `gcg-eval gates --e1 F --e2 F --e3-tests F --e3-panel F --e4-tests F --e4-panel F --check` on the E1-E4 reports under docs/port/eval/final/. Record the per-gate results in W-19, and close it if `--check` exits 0. If a gate fails, W-19 stays open with that result.
- **AI row:** no (the gates read reports). **Effort:** S.

## 2. gcg-search: mechanisms and AI evidence

### W-05: strategy fusion
- **Missing:**
  - The scenario bar: strengthened avoids the attack in at least 13 of 16 seeds. Measured: 0/16 for `opponent_buckets=4` and `=8`, the same as the reference.
  - The census of the 12 remaining observation-key splits. Each split needs an observation key, or a W-43 regret interval within ±0.0004 per decision. By the entry's own rule, a split with neither stays open.
- **Why:**
  - The search's evaluation prefers the attack even against the bucketed reply, so a node-level fix does not change the root decision.
  - The census tool was never built: it is a `gcg-eval` tool outside P6-C's files (P6-C interface request 5).
- **Work:**
  - Port the 183-position census as a `gcg-eval` subcommand with a test.
  - Give each split an observation key in `crates/gcg-search/src/mech/p6c/`, or a `gcg-eval regret` row with and without it on its census positions.
  - Revisit the scenario, or record why the root-decision bar cannot be met while the leaf evaluation is `ROLE_WEIGHTS`.
- **AI row:** yes (any key that changes the search needs rows). **Effort:** L.

### W-13: the "while rested" Rest-target exception (e2e)
- **Missing:** the e2e part of the verification:
  - an "effect coverage" deck set with GD03-070 and every card that reads its own rested state;
  - a per-exception firing counter in the decision trace;
  - 100 `gcg-eval` games that report a non-zero firing count.
- **Why:** the agents do not expose prune exceptions, because `gcg_ai::SearchStats` has no field for them. So `--decision-trace` (W-21) cannot count them, and the deck set is a `gcg-eval` file outside P6-E's files.
- **Work:**
  - Add a counter: either a `MechanismEffort` field in `crates/gcg-search/src/contracts.rs`, filled by `p6e::prune` and by the reference prune path, or a trace-side replay as `trace::greedy_cap_hits` does for greedy.
  - Write it into `crates/gcg-eval/src/trace.rs`.
  - Add a deck set under `examples/decks/` or embedded in `gcg-eval`.
  - Add a `crates/gcg-eval/tests/` e2e test of 100 games asserting a non-zero count.
  - Keep the 400-game no-harm guard: the `rest_prune_worlds` row already exists.
- **AI row:** no for a pure counter (A13: identical decision traces with and without it, plus the corpus `search` set). **Effort:** M.

### W-14: close target choices (multi-rollout)
- **Missing:** plan item (2), several rollouts per world inside the search's iterations at target decisions. So the multi-rollout row and the "both" row are missing. If `target_rollouts` is ever adopted, the puzzle must also join `crates/gcg-search/tests/ai_puzzles.rs`.
- **Why:** the `Mechanisms` trait has no hook that lets a root move's iteration run several rollouts from its world. The critic-side multi-rollout variant was measured on the puzzle (9/24, 10/24) and dropped.
- **Work:**
  - Add a core hook in `crates/gcg-search/src/mcts.rs` and `mech/mod.rs` (P6-D's core), for example a per-iteration rollout count chosen by the mechanism.
  - Implement multi-rollout in `mech/p6e/`.
  - Run `gcg-eval ablation individual --mechanism target_rollouts=true,<multi-rollout flag> --reference standard --games 400 --role dev ...`, alone and with the critic. Confirm any row at +0.03 or more on confirm seeds.
  - Alternatively, record a decision that the multi-rollout path is dropped, citing the puzzle measurement.
- **AI row:** yes. **Effort:** M.

### W-17: implausible plays and missed lethals
- **Missing:**
  - Step (2): the `gcg-eval weakness` suite of frozen scenarios per WK category, scored for the reference and the strengthened presets. It covers `hold_up`, `ex_variant`, `rest_the_active_unit`, `damage_the_unit_that_cannot_repair`, `burst_aware_attack_order` and `pair_the_linking_pilot`.
  - Step (3): a blinded audit of 20 BO1 games per example pairing, with transcripts and committed raw outputs, and the implausible-play count per game with a Wilson interval.
- **Why:** neither is a P6-B file, and step (3) needs the Rust transcripts tool (W-119).
- **Work:**
  - Collect the existing puzzles of the WK categories, from `crates/gcg-search/tests/ai_puzzles.rs` and the scenario tests, into a `gcg-eval weakness` subcommand with a report and a test.
  - After W-119, generate the audit games with `gcg-eval`, write transcripts, and run the audit with human or agent auditors.
  - Commit the outputs under `docs/port/eval/`.
  - Step (4) is done: 0 real missed lethals recovered.
- **AI row:** no new mechanism (the suite and audit measure the shipped presets). **Effort:** L.

### W-18: fidelity on the shipped presets
- **Missing:** `gcg-eval fidelity` on the M15 presets with held-out seeds (T-131). The recorded run used the pre-M15 presets and confirm-role seeds.
- **Why:** the budgets were tuned after the P6-D run, and M15 did not repeat it (`docs/port/eval/M15/README.md` lists no fidelity run).
- **Work:** run this command from a release build of HEAD:

  ```text
  gcg-eval fidelity search:fast search:standard --variants red-green-zeon,tests/data/decks/red-green-zeon-weakened.txt,purple-white-tekkadan,docs/port/eval/P6-D/decks/purple-white-tekkadan-weakened.txt,docs/port/eval/P6-D/decks/blue-white-federation-weakened.txt --against blue-white-federation --games 200 --seed <eval master> --role eval --ledger docs/port/eval/seed-ledger.json --workers auto --out docs/port/eval/final/fidelity-fast-standard.json
  ```

  Record Spearman, Kendall and the per-variant paired differences in W-18, and mark T-131.
- **AI row:** no (it measures the shipped presets). **Effort:** M (compute: `standard` at 8x).

### W-25: opponent modelled with the searcher's policy
- **Missing:**
  - The over-extension scenario (T-118), which was not reproduced.
  - Plan item (2), the per-deck opponent profile.
  - Plan item (3), belief weighting by this model.
- **Why:**
  - In both constructed positions the opponent's reply lies inside the tree, or the evaluation rates the attack as good. The fitted offsets also make the modelled opponent play tricks less often.
  - Item (3) depends on P6-C, which uses the reference policy's MAIN scores.
- **Work:**
  - Build a stronger-punishment scenario in `crates/gcg-search/tests/scenarios/`, or record the lead's decision that the scenario is not worth more construction.
  - Build the profile, if wanted: the W-23 calibration run on reference self-play of the benchmark deck, stored per deck digest.
  - Measure each opponent setting and a leave-one-out row if any is adopted.
- **AI row:** yes. **Effort:** M.

### W-26: kill-aware targeting
- **Missing:**
  - Policy-level regret per Damage decision.
  - A re-measurement under the strengthened search, as the plan requires. The rows ran at the null configuration, and the flag was in neither `B_full` nor `B_mid`.
  - Damage-payoff targeting, which is not rebuilt.
- **Why:** `gcg-eval regret` judges agents, not rollout policies, and M15 left the flag out of its bundles.
- **Work:**
  - Extend `crates/gcg-eval/src/regret.rs` with a policy judge, or record that the search-level regret answers the question.
  - Run `gcg-eval ablation leave-one-out --bundle search:standard:...,adaptive_horizon=true,capped_rollout_continuation=true,tactical_solver=true,kill_aware_targeting=true --mechanism kill_aware_targeting=false --games 400 --role dev ...`, and confirm a promising row once.
  - Decide on damage-payoff targeting.
- **AI row:** yes. **Effort:** M.

### W-31: keyword worth
- **Missing:** the plan's second half: keyword worth from the fitted per-keyword features of W-37, instead of a flat bonus.
- **Why:** W-37's `keyword_features` measured negative (individual -0.030, high budget -0.080) and is not adopted, so no fitted keyword worth exists.
- **Work:**
  - Either derive per-keyword worth for the policy (`crates/gcg-search/src/mech/p6f/`) from a fit, and measure it as a flag.
  - Or record in W-31 that the flat bonus stays because W-37 measured negative. That decision closes the item by measurement.
- **AI row:** yes if a mechanism is built. **Effort:** S as a decision, M as a mechanism.

### W-36 and W-142: held-out panel decks in the log-loss tables
- **Missing:** the log-loss of the refits on the held-out panel decks, `blue-red-tempo` and `green-white-link`. Both verifications name it: W-36's "no example or held-out panel deck worse than `ROLE_WEIGHTS`", and W-142's "on the held-out panel decks".
- **Why:** the tuning, dev and confirm roles refuse held-out pairs (PLAN §12.5), so `gcg-eval tune-leaf` never recorded positions there. The "held-out pair" of the fits, purple-blue-swarm : red-purple-control, is a pair of tuning decks.
- **Work:**
  - After the final evaluation, record leaf-point positions on the 13 held-out pairs with `gcg-eval tune-leaf`, under a fresh eval-range master recorded in the ledger.
  - Score the embedded models on them with `--test-positions`.
  - Add the per-deck table to `docs/port/eval/P6-G/` and both entries.
  - Alternatively, the lead records that the gate is moot while no refit or `determinized_fit` ships: the shipped presets use `ROLE_WEIGHTS`.
- **AI row:** no (analysis of unadopted models). **Effort:** S as a decision, M as a measurement.

### W-137: determinism contract (strengthened-search math)
- **Missing:**
  1. With `portable_math` on, shared `gcg-ai` code still calls the platform libm inside the strengthened search: `crates/gcg-ai/src/noop.rs:196` through `evaluation.rs:271-277`, `setup.rs:283`, and `policy.rs:114`.
  2. The cross-platform check on macOS arm64 and Linux x86-64.
  3. Adopting `portable_math` in the presets, with its rows.
- **Why:**
  - `gcg-ai` is frozen (shared with the reference). Making those paths portable needs a math parameter in `gcg-ai`, or P6-E/P6-H replacements of the paths.
  - A Linux runner needs the CI push that M17 asks the user for (A29, T-133).
- **Work:**
  - Add a `Math` parameter to the `gcg-ai` functions the strengthened search calls, or route them through the mechanisms. The null configuration and the corpus must stay at 100%.
  - Run the tier-1 scenario tests on the Linux CI job.
  - Run `gcg-eval ablation leave-one-out --bundle <shipped standard> --mechanism portable_math=false` (and the `fast` equivalent), adopt `portable_math` if non-negative, and update `crates/gcg-search/src/presets.rs` and `tests/null_config.rs`.
- **AI row:** yes, for item 3. **Effort:** M (items 1 and 3), plus M17 for item 2.

### W-139: opponent nodes and the searcher's private cards
- **Missing:**
  - Opponent-view worlds at root decisions whose options name a hidden card. Today the share applies at 11 of 32 vector decisions.
  - The scenario bar: strengthened keeps the trick in at least 12 of 16. Measured 0/16 at shares 0.25 and 0.5, the same as the reference.
- **Why:** a redrawn world gives the searcher's hand cards other identities, so a root move that plays a hand card would play another card. Fixing it needs the engine identity API (P6-C request 1, as W-08) or a core hook after the root move is applied (request 6). The leaf evaluation also values the alternative above keeping the trick.
- **Work:** after the W-08 engine change, or a `Mechanisms` hook in `crates/gcg-search/src/mcts.rs` after the root move with `&mut GameState`:
  - extend `mech/p6c/view.rs` to every decision;
  - re-run the scenario `p6c_w139_held_trick.rs`;
  - measure individual and high-budget rows.
- **AI row:** yes. **Effort:** M, after the engine change.

## 3. gcg-engine: interface changes

### W-08 and W-134: exact pools and archetype decklists
- **Missing:**
  - `exact_pools` (W-08): conservation-exact worlds, the reference's pad/trim counter, the testkit scenario and the 10,000-game fuzz invariant.
  - `opponent_list = archetype` (W-134): draw the opponent's unseen cards from a prior of candidate lists, the information-set test, and the one-list-prior e2e.
- **Why:** both are blocked on the engine. `determinize` assigns identities per zone side and pads or trims with `_fit`, and it is the only public way to change an identity: `GameState` cards are private to gcg-engine. Archetype mode also needs a state with other decklists, because `determinize`, `burst_density` and the evaluation read the decklists in `GameConst`.
- **Work** (P6-C interface requests 1-2):
  - Add `pub fn assign_hidden(s: &mut GameState, observer: u8, ids: &[(Uid, DefId)]) -> Result<(), EngineError>` and a public `with_decklists(st, main, resource) -> GameState` in `crates/gcg-engine/src/observe.rs`.
  - Add the pad/trim counter inside `observe::fit`.
  - Implement `exact_pools` and `archetype` in `crates/gcg-search/src/mech/p6c/`.
  - Add a prior: the packaged deck lists, or a file named by a `gcg-eval benchmark` option (A30).
  - Tests: the W-08 testkit scenario (a card moved into the other player's deck), the fuzz invariant over 10,000 random games, the W-134 information-set test, and the e2e where a one-list prior equal to the true list reproduces `known` mode exactly.
  - The engine change is additive and unused by the reference: the corpus must stay at 100%.
- **AI row:** yes, for both mechanisms. A one-off probe found 0 of 3,528 pools that `_fit` pads or trims, so W-08 is expected to have no game-level effect. **Effort:** M (engine) + M (mechanisms and rows).

### W-64: consolidated custom hooks
- **Missing:** a behaviour-identical, parametrised implementation for `gd04b_played_with_ex`/`gd05b_played_with_ex` (a scope parameter), and one parametrised predicate for the three destroyed-cause conditions.
- **Why:** they are `gcg-engine` source files, outside P6-K's scope.
- **Work:**
  - Edit the hook bodies in `crates/gcg-engine/src/` (the interpreter and hooks layer).
  - The per-test traces of GD04-106, GD04-108 and the GD05 cards pin the behaviour.
  - `gcg-diff corpus --tier 1 --require-all` and `--tier 2` must stay at 100%. Run the differential fuzz (`gcg-diff fuzz --games 10000 --seed 2026 --compare ...`).
  - The "custom steps cannot request choices" part stays as in Python, a stated decision.
- **AI row:** no (A13: the engine path the AI runs, so the corpus `search` and `baselines` sets at 100% are the row). **Effort:** S-M.

### W-101: interpretation-tagged code paths
- **Missing:** the engine-side record of when an interpretation-tagged code path runs. The card-level trace in `analysis.json` over-approximates, and it cannot see the 14 rule-level interpretations.
- **Why:** it needs tags in `gcg-engine` and `gcg-effects`, which P6-M does not own.
- **Work:**
  - Add a per-interpretation counter keyed by I-NN at each anchored code path. It must be read-only and absent from every compared output, like the `paranoid` side tables.
  - Feed it into `crates/gcg-runner/src/analysis.rs` `interpretations_used`.
  - The test: `p6m_analysis.rs::w101_*` with a rule-level interpretation that fires.
- **AI row:** no (A13: a counter that the AI does not read; corpus at 100%). **Effort:** M.

### W-111: execution coverage of rules, FAQ entries and rulings
- **Missing:** execution evidence for rules, FAQ entries and rulings. They stay tag-based, while the verification's bar is 100% execution evidence for every one.
- **Why:** execution of a rule's code path needs per-rule code-path ids, and W-56's interpretation ids are data, not hooks.
- **Work:**
  - Either add rule-id probes to the engine's rule procedures (paired with the W-101 counters) and extend `crates/gcg-diff/src/coverage.rs`.
  - Or record the lead's decision that tag-based coverage stands for rules, FAQ entries and rulings, with the reason.
- **AI row:** no. **Effort:** L for the probes, S for a decision.

## 4. gcg-eval and gcg-data: opt-in tools (A30)

### W-88: `gcg-eval data-status --sources`
- **Missing:** the CLI option of the e2e verification. `crates/gcg-eval` has no `data-status` subcommand.
- **Why:** it needs a `gcg-data` and `gcg-eval` source change outside P6-K's files.
- **Work:**
  - Move `provenance_listing` from `crates/gcg-data/tests/p6k_data_tripwires.rs` into `gcg-data`, embedding `data/sources.d/*.json`.
  - Add `gcg-eval data-status [--sources]` (dispatch in `crates/gcg-eval/src/cli.rs`).
  - Add an e2e test: the listing covers every source and flags the Asia PDF as text-only.
  - Regenerate `docs/port/eval/P6-N/cli-help.txt` with `python3 scripts/cli_help_snapshot.py --write`.
- **AI row:** no. **Effort:** S.

### W-145: `gcg-eval data-status --freshness`
- **Missing:**
  - `gcg-eval data-status --freshness [--as-of DATE]`.
  - A public `parse_deck`/`validate` over a given `CardDb`, so that a deck using new cards validates without a rebuild (P6-K interface notes 2-3).
- **Why:** these are source changes in `gcg-data` and `gcg-eval` outside P6-K's files.
- **Work:**
  - Move `freshness` from `p6k_data_tripwires.rs` into `gcg-data`, with `products.json` and the manifest embedded.
  - Add the option to the `data-status` subcommand of W-88.
  - Make `parse_deck_with` and `Validator::validate` public over a `&CardDb`.
  - Add the e2e test: `--as-of 2026-11-01` names GD06 (released 2026-10-30, 0 card numbers) and not GD07, and a deck of the synthetic GD06 cards validates.
  - Running `/gcg-refresh-data` for GD06 is a separate lead task.
- **AI row:** no. **Effort:** S-M.

### W-63 and W-67: general templates (§12.11 in-scope part)
- **Missing:**
  - Templates for the remaining compile-error families (W-63): selector suffixes, delayed "during this turn, when …", inline token definitions, the EX-Resource trigger, "are deployed rested", "it" without antecedent, "exile this card in your trash", and "It can't activate <Blocker>".
  - The "each player in turn order" template and the six single-player wordings (W-67).
  - The IR `cards[].template` field, written by `port/oracle/export_ir.py`.
  - `gcg-eval data-status --templates`.
  - T-154: `export_ir.py` asserts that every template reproduces its binding cards byte for byte.
- **Why:** templating takes much work (6 bindings are reproduced so far). The IR field changes the IR bytes and so the corpus `ir_sha256`, which P6-K did not touch.
- **Work:**
  - Add templates in `port/oracle/templates/`, checked by `uv run python port/oracle/templates/check.py --check`.
  - Set `cards[].template` in `export_ir.py`; the IR slot and the loader `CardEntry::template` exist. Then run `uv run python port/oracle/export_ir.py --write`, and regenerate the corpus manifests if `ir_sha256` changes.
  - Report the counts per family in `data-status --templates`.
  - Golden, corpus and equivalence runs must show no behaviour change.
  - Post-acceptance (answer (a)): move the templates into `src/gcg_sim/effects/compiler/` and retire the reproduced bindings.
- **AI row:** no. **Effort:** L (templates), M (IR field and the option).

### W-75 and W-76: the `scope` block of `analysis.json`
- **Missing:** `analysis.json` does not write the `scope` block. W-75's verification needs "`analysis.json` names the format and the list date". W-76's needs "`analysis.json`'s scope block lists the unmodelled tournament procedures".
- **Why:** P6-L provided `gcg_eval::validate_cmd::packaged_scope().to_json(Some(list))` and handed the wiring to P6-M, which did not do it. `crates/gcg-runner/src/analysis.rs` has no `scope` key.
- **Work:**
  - Write the block in `gcg-eval benchmark --analysis` (`crates/gcg-eval/src/benchmark_opts.rs`, or pass it into `gcg_runner::analysis`). It holds `format`, `banlist{applied, effective_date, regions, format}`, `unsupported_formats`, `unchecked_tournament_rules`, `unmodelled_tournament_procedures` and `edition_beta`.
  - Extend `crates/gcg-eval/tests/p6m_cli.rs::opt_ins_add_files_and_keep_the_reports` to assert the block, and that the outputs without `--analysis` are unchanged (T-153).
- **AI row:** no. **Effort:** S.

### W-119: `gcg-eval transcripts`
- **Missing:** the Rust `gcg-eval transcripts` subcommand, with its test. The Python `scripts/blind_transcripts.py` is fixed and tested.
- **Why:** `gcg-eval` was outside P6-N's files.
- **Work:**
  - Port the script to `crates/gcg-eval/src/` over the public replay API, with ids from the game content only.
  - Add a test: the same replay in two directories gets the same id, and transcripts hold no search statistics.
  - Regenerate the CLI help snapshot.
  - W-17's step (3) uses it.
- **AI row:** no. **Effort:** M.

## 5. gcg-runner

### W-104: CPU and start-up time in `timing.json`
- **Missing:** per-worker CPU time and start-up time.
- **Why:** safe std has no CPU-time clock, `getrusage` needs `unsafe` FFI (denied outside `crates/gcg-core/src/libm.rs`), and the one wall-clock module is `crates/gcg-runner/src/clock.rs`.
- **Work:**
  - Either the lead records the item as done with the deterministic counts (`decisions`, `decisions_per_game`).
  - Or add a per-worker busy-seconds measure from `clock.rs` wall time in `run.rs`, in `gcg-eval benchmark`'s `timing.json` only, with a unit test that the fields are present. Python's keys stay in `gcg-sim`'s `timing.json`.
- **AI row:** no. **Effort:** S.

## 6. Lead: gates, docs and long runs

### W-110: strategic paranoid fuzz with the shipped presets
- **Missing:** the 1,000 strategic paranoid games of PLAN §9.4 with 0 failures (T-056, failing in `docs/port/tests.json`):
  - the paranoid-build `search:fast` (400) and `mixed:greedy,search:fast` (200) runs, which did not finish;
  - a rerun with the shipped M15 `fast`, since every recorded `search:fast` run used the pre-M15 preset.

  Phase 4 found the cause of T-056's failure: the W-143 redirect cycle in a `search:fast` rollout (game 221). `engine_error_leaf` is now in both shipped presets. The release-build runs had 0 failures.
- **Why:** the paranoid build recomputes every cached view the search reads, so on the shared machine the run used 254 CPU minutes in 3 h 25 min without finishing.
- **Work:** on a quiet machine, run:

  ```text
  cargo build --release --offline -p gcg-diff --features paranoid --target-dir target/paranoid
  target/paranoid/release/gcg-diff fuzz --games 400 --seed 2026 --agent greedy --paranoid --workers auto
  target/paranoid/release/gcg-diff fuzz --games 400 --seed 2026 --agent search:fast --paranoid --workers auto
  target/paranoid/release/gcg-diff fuzz --games 200 --seed 2026 --agent mixed:greedy,search:fast --paranoid --workers auto
  ```

  All must print `fuzz passed` with 0 failures. Record them in `docs/port/eval/P6-N/w110-strategic-fuzz.txt` and W-110, and mark T-056 passing.
- **AI row:** no. **Effort:** M (compute).

### W-118: the Rust gate on a clean clone
- **Missing:** a recorded passing run of `scripts/verify_all_rust.sh` (and `--full`) on a clean clone (T-001).
- **Why:** the P6-N run stopped at `traceability --check`. The traceability documents were regenerated at commit 8af9b0e, but no full run is recorded since.
- **Work:** run `scripts/verify_all_rust.sh` and `scripts/verify_all_rust.sh --full` on the committed HEAD. They take hours: fuzz, the slow suite, tier 2. Record the result in `docs/port/progress.md` and W-118.
- **AI row:** no. **Effort:** M (compute).

### W-126: throughput reproducibility
- **Missing:** "two runs on an idle machine agree within 3% on the minimum".
- **Why:** every run so far was on a loaded machine (load average 130-330).
- **Work:** on an idle machine, run `python3 scripts/throughput.py search:fast --opponent ref:fast --games 400 --runs 2 --workers auto` on the three example pairings. Record both minima and their difference in W-126. The power-state part of the plan is overridden by the brief.
- **AI row:** no. **Effort:** S.

### W-129: checked interface docs
- **Missing:**
  - A snapshot test of the public Rust API signatures.
  - D14a: `git mv docs/AI.md docs/python/AI.md` and rewrite the `docs/AI.md:N` citations under `docs/port/`. `docs/python/` does not exist yet.
  - The Rust `docs/AI.md` with the preset table of `docs/port/eval/M15/preset-table.md`, and its test (T-130).
  - The wording of `docs/python/AI.md:326-327`.
- **Why:**
  - Rustdoc JSON needs a nightly feature that the stable toolchain lacks.
  - D14a is scheduled for the lead and was not done.
- **Work:**
  - Either a stable-toolchain signature extractor, for example a `gcg-diff` lint over the `pub` items of `contracts.rs` and `lib.rs` compared with `docs/port/contracts.md`, or a recorded decision that `docs/port/contracts.md` stays the frozen signature document.
  - D14a as PLAN §1.1 describes it, then the Rust `docs/AI.md` and T-130.
  - Post-acceptance: the `mirrored_specs` docstring (`ai/selfplay.py:95-97`).
- **AI row:** no. **Effort:** M.

### W-154: the Python gates with the port committed
- **Missing:**
  - Nine shell-fenced blocks in the Phase 6 evaluation records have no doc-check marker. `scripts/check_doc_commands.py` runs every bash block under `docs/**/*.md`, so it, and with it the `--full` gate, fails on them. The blocks are:
    - `docs/port/eval/M15/README.md` lines 44, 93 and 133;
    - `docs/port/eval/P6-G/README.md` lines 39 and 92;
    - `docs/port/eval/weakness-status/P6-A.md` line 64;
    - `docs/port/eval/weakness-status/P6-F.md` line 55;
    - `docs/port/eval/weakness-status/P6-N.md` line 370, an indented inline mention that the block pattern matches;
    - `docs/port/eval/weakness-status/P6-decks.md` line 106, the one P6-N found.

    Most hold placeholders such as `<on>` or `<row>`, or runs that take hours.
  - The clean-clone run of `scripts/verify_all.sh` with the port committed (T-135, M16).
- **Why:** the records were written after the Phase 0 fix (W-154 was resolved then), and P6-N saw only the first. T-135 is an M16 step.
- **Work:**
  - Give each block a `text` fence or a `<!-- doc-check: skip <reason> -->` marker, and reword the P6-N line so it no longer starts with a fence.
  - Run `uv run python scripts/check_doc_commands.py --list`, then run the checker itself.
  - At M16, run `scripts/verify_all.sh` on a clean clone and record it.
- **AI row:** no. **Effort:** S.

## 7. The user's decision

### W-120: project Claude permissions
- **Missing:** the user's answer on the proposed `.claude/settings.json` change: a host-pinned curl rule, and `Bash(git -C *)` without the edge whitespace.
- **Why:** PLAN §0.1, A32 (3): deferred to the user, because it changes this project's Claude permission settings.
- **Work:**
  - Ask once, near the end of the port.
  - If approved: apply the change, then run `python3 scripts/check_claude_settings.py`, which should exit 0, and `--selftest`.
  - If declined: the entry stays open with the answer quoted, and the final report says so (§12.10).
- **AI row:** no. **Effort:** S.

## 8. Resolved items to re-verify after the M15 preset change

The presets changed at M15 (commit 82b0b34). These resolved items' slow evidence ran on the earlier presets, and no later run is recorded (the M15 and M10 gates ran `cargo test --workspace --release` without `--features slow`):

- **W-150:** the 1/4/10-worker identity of the shipped presets (`phase6_slow::w150_every_preset_is_identical_at_1_4_and_10_workers`).
- **W-152:** seat symmetry with `search:fast` (`phase6_slow::w152_seats_and_first_player_are_symmetric`).

Work: run `cargo test --release --offline -p gcg-eval --features slow --test phase6_slow` (W-118's gate runs it). AI row: no. Effort: S (compute).

## 9. Resolved items with follow-ups or hand-offs

None of these follow-ups blocks the item's resolution. They are listed so that none is lost.

| Item | Follow-up | Owner |
| --- | --- | --- |
| W-06 | `visited_set_backup` is a correctness fix that measured null (-0.005). The P6-D file leaves its adoption to the lead | lead (preset decision; leave-one-out row if adopted) |
| W-15 | A sentence on the 250-decision cap and the continuation in the Rust `docs/AI.md` (T-139) | lead, Phase 8 |
| W-41 | Calibration against real tournament results once the user supplies data (A32 item 5) | user, then lead |
| W-45 | `paired_mulligan` measured non-negative on dev and confirm, but M15 tested it in no bundle, and its non-adoption has no recorded reason. Record the decision, or run `B_core` with it against `B_core` | lead |
| W-46 | The head-to-head `unfinished` list is tested on recorded outcomes. The verification's e2e with a tiny engine decision cap is not separately recorded | lead or P6-I (S) |
| W-57 | The `tuning --rounds 0` bug row has no Rust test. P6-K says `gcg-eval tune` should carry it | P6-I (S) |
| W-63, W-67, W-68, W-129, W-155 | §12.11 post-acceptance package steps (answer (a)) | after acceptance, `/gcg-refresh-data` binding workflow |
| W-71, W-72 | The official ST02/ST05 decklists and the historical B&R lists through `/gcg-refresh-data` | lead |
| W-76 | Sealed support needs its rules page in the data | lead |
| W-89, W-97 | Repeat the `dataset_version` meaning and the schema naming rule in the Rust report docs | lead, Phase 8 |
| W-92 | The benchmark skill's sample-size guidance with the design effect | lead, Phase 8 (T-142) |
| W-114 | Mark the refresh skill's blocks and add `.claude/skills/gcg-refresh-data/**/*.md` to the doc checker's default set | lead |
| W-115, W-128 | Regenerate the research artefacts and the stale Python-doc counts (Python side) | lead |
| W-127 | The 16 dangling shas in `docs/PROGRESS.md` | lead (Python docs) |
| W-130 | `src/gcg_sim/data/official/floor_rules.json` wording, listed for the user (packaged data) | user |
| W-143, W-162 | Rules questions to the Python conflict workflow: redirect substitution once per damage event (W-143), and the 395 moved-host battle-damage triggers (W-162) | lead |
| W-153 | The SKILL.md step 2 wording (replacement text in P6-N lead note 4) | lead, M17 (T-142) |
| W-155 | Run the parenthetical gate inside the refresh workflow | lead, M17 |

## 10. `docs/port/tests.json` bookkeeping (lead file)

The register lags behind the package evidence. `gcg-diff tests-json --check` checks only its format and id list, and it passes.

- **Recorded as `not-started`, but their tests pass according to the package files:**
  - scenario tests T-107, T-108, T-109, T-110, T-111, T-112, T-113, T-114, T-115, T-117, T-119, T-120, T-121, T-122 and T-123;
  - T-153 (`gcg-eval benchmark` byte identity, P6-M);
  - T-155 (refresh news tests, P6-N).
- **Should stay open:**
  - T-116 (W-17) has no scenario file; the WK puzzles stand in for it, but step (2) is missing.
  - T-118 (W-25): the scenario was not reproduced.
  - T-106 and T-007 need `gcg-diff weaknesses --check --require-resolved` to pass: 33 problems today.
- **P6-C's status file cites the wrong register ids.** It cites T-105 for W-05's unit test, T-107 for W-07's scenario and T-139 for W-139's unit test. The register's ids are T-109 (W-05), T-110 (W-07) and T-123 (W-139). WEAKNESSES.md uses the register's ids.
