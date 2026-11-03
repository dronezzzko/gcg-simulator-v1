# GAP-A2 weakness status: W-13, W-18, W-31, W-36, W-45, W-137, W-142

This file records the residual items of `docs/port/eval/weakness-gaps.md` (sections 2 and 9)
in the format of docs/port/PLAN.md §12.10.

- **Written by:** the GAP-A2 implementer, on 2026-11-03.
- **Branch:** `worktree-wf_9f38bb2b-c03-2`, from `1007ca9` ("Port WEAKNESSES: recorded outcome
  for all 164 items; residual-gap list").
- **Commits:** `6bdd500` holds the code and the tests. The commit that adds this file holds
  the evidence.
- **Not edited:** WEAKNESSES.md and `docs/port/tests.json`. The lead records the status lines
  from this file.
- **Presets:** `crates/gcg-search/src/presets.rs` is unchanged. No mechanism of this package
  is on in the shipped presets.

| Item | Final status |
| --- | --- |
| W-13 | resolved |
| W-18 | resolved |
| W-31 | resolved by measurement (not adopted) |
| W-36 | resolved by measurement (not adopted) |
| W-45 | resolved by measurement (not adopted) |
| W-137 | open: item 2 (the Linux CI run, M17) and item 3 (adoption, the lead's preset decision); item 1 resolved, item 3's rows measured null |
| W-142 | resolved by measurement (not adopted) |

**Positive mechanisms:** no mechanism measured positive. The one row with a positive lower
Newcombe bound, `portable_math`'s `fast` leave-one-out row at +0.070 [+0.004, +0.135] (dev),
is identical to its placebo row in every winner. That number is the protocol's noise on that
master, not the flag (W-137 below).

How the runs were made:

- Every run used 4 worker threads or fewer. The machine was shared with the final evaluation,
  at load averages 25 to 40.
- Every run used mirrored blocks of four and the forced play/draw balance (`--choose-first
  seat0`, A24). Results do not depend on the worker count.
- Masters follow A14: dev and confirm only, never eval or review, each new to every ledger
  under `docs/port/`. All are in `docs/port/eval/GAP-A2/seed-ledger.json`.
- The ablation rows and the W-13 run used the release `gcg-eval` with the runtime code of
  `6bdd500` (edits before that commit were tests and doc comments only). The W-18 fidelity run
  used a release build of `1007ca9`; the shipped presets play the same games in both.
- Commands in this file are `text` blocks (the doc checker runs `bash` blocks). `TRACE` stands
  for a scratch file.

## W-13 — The "while rested" Rest-target exception (e2e)

- **Final status:** resolved. The open part was the e2e of the verification: an "effect
  coverage" deck set, a per-exception firing counter in the decision trace, and 100 `gcg-eval`
  games that report a non-zero firing count. All three are done. The scenario and the 400-game
  no-harm guard were already recorded (`rest_prune_worlds`).
- **What was done:**
  - `crates/gcg-ai/src/actions.rs`: `rest_prune(st, dec, moves) -> Option<RestPrune>`, which
    branch of the reference's no-op Rest-target prune (`prune_dominated`, `ai/actions.py:250-258`)
    decides a SELECT: `Pruned`, `RestTrigger` (`_resting_can_trigger` keeps the moves) or
    `WhileRested` (the W-13 exception, `_resting_changes_more_than_the_unit`, keeps them);
    `None` where the prune does not apply or would change nothing. It is analysis only: no
    agent calls it, and it computes exactly what the prune computes (the same private helpers,
    in the prune's order).
  - `crates/gcg-eval/src/trace.rs`: every searched decision of a `--decision-trace` file gets
    `"rest_prune"` (`"pruned"`, `"rest_trigger"`, `"while_rested"` or `null`), replayed at the
    decision from the canonical moves the search's root prune receives, as `greedy_cap_hits`
    replays greedy (W-22). This is the counter. A `gcg_search::MechanismEffort` field was not
    needed, so the lead-owned `contracts.rs` is unchanged.
  - The effect-coverage deck set, `docs/port/eval/GAP-A2/decks/`. Both decks pass
    `gcg-sim validate` (B&R included):
    - `coverage-white-blue.txt` holds every packaged card that reads its own rested state and
      fits White/Blue. These are Freedom Gundam GD03-070, Psycho Haro EB01-042, Dom Gross Beil
      EB01-055, Core Fighter GD04-013, Acguy ST11-002 and Narrative Gundam A-Packs EB01-003. It
      also holds the White/Blue effects that choose an enemy Unit and rest it.
    - `coverage-red-green.txt` holds the Red/Green cards whose rested-state text is a trigger:
      GuAIZ (Commander Type) GD03-038, Gundam Throne Drei GD04-041 and Gundam Plutone EB01-028.
    - The census of the 1,152 card numbers (`effect` text matching "while/if this Unit/Base/card
      is rested") found 9 cards. EB01-055 needs two or more enemy players, so it cannot fire in
      a two-player game. It is in the deck for coverage only.
- **Evidence** (tests pass):
  - `crates/gcg-eval/tests/gapa2_w13.rs::w13_the_coverage_decks_fire_the_while_rested_exception`:
    100 `gcg-eval head-to-head` games of two tiny null-configuration searches on the White/Blue
    coverage mirror. Counts: `while_rested` 110, `pruned` 331, `null` 9,375. The head-to-head
    report is byte-identical with and without `--decision-trace`, so the counter changes no
    game.
  - `w13_the_counted_branch_is_the_prune_s_own`: 40 policy games on the coverage decks (the
    mirror and the Red/Green pairing). At every SELECT with more than one option,
    `prune_dominated` with `rest_targets` removes moves exactly when the counter says `Pruned`.
    Counts: `pruned` 97, `rest_trigger` 32, `while_rested` 27, `null` 183.
  - The recorded coverage-deck run with the shipped `fast` preset: 100 games of `search:fast`
    against `ref:standard` on the White/Blue mirror (dev 22721300):

    ```text
    gcg-eval head-to-head search:fast ref:standard \
      --pair docs/port/eval/GAP-A2/decks/coverage-white-blue.txt:docs/port/eval/GAP-A2/decks/coverage-white-blue.txt \
      --games 100 --seed 22721300 --role dev --workers 1 \
      --out docs/port/eval/GAP-A2/w13-coverage-fast-vs-ref-standard-100.json --decision-trace TRACE
    uv run python docs/port/eval/GAP-A2/w13_trace_summary.py TRACE
    ```

    The "while rested" exception fired at 123 of 8,979 searched decisions, in 65 of the 100
    games: 59 at `search:fast`'s decisions and 64 at `ref:standard`'s. The prune applied at 279
    decisions. Summary: `docs/port/eval/GAP-A2/w13-coverage-trace-summary.json` (the 5.3 MB
    trace is not committed; the command regenerates it). The row itself, for the record:
    65/100 (0.650, [0.552, 0.736]), +0.300 [+0.144, +0.434], discordant 17/2, iterations per
    game 12595 vs 8269, applies per game 249542 vs 100287
    (`docs/port/eval/GAP-A2/w13-coverage-fast-vs-ref-standard-100.json`).
  - Already recorded: scenario test W-13 (T-113,
    `crates/gcg-search/tests/scenarios/p6e_w13_freedom.rs`: reference 16/16, null configuration
    16/16, `rest_prune_worlds` 16/16), the negative control
    `mech::p6e::tests::without_the_exception_the_freedom_lethal_is_missed` (0/16), and the
    400-game no-harm guard (`rest_prune_worlds` identical to the null row in all 400 games).
- **Ablation:** no AI effect (A13) for the counter. It replays the prune after the agent has
  chosen, and the agents never call `rest_prune`. Evidence: the head-to-head report is
  byte-identical with and without the trace (the e2e test above); the tier-1 corpus is
  3804/3804, with the `search` set 9/9 and the `baselines` set 8/8; the seed-1 equivalence
  smoke is 3/3 (gate below). The mechanism rows are unchanged: `rest_prune_worlds` individual
  (dev 20005000), 197/400 [0.444, 0.541], -0.015 [-0.080, 0.050], discordant 21/24, 9831.4 vs
  9820.6 / 118560.6 vs 117536.8, 0 games changed (`docs/port/eval/P6-E/individual-rest_prune_worlds.json`).
- **tests.json:** T-113 (the scenario, pass); the e2e tests above have no register id of
  their own.

## W-18 — `fast` rates are not `standard` rates (fidelity on the shipped presets)

- **Final status:** resolved. The fidelity run on the shipped M15 presets meets the
  verification.
  - The presets: `fast` at 1.5x the reference `standard`, 96/240/960; `standard` at 8x the
    reference `strong`, 1024/2560/10240. Both carry `adaptive_horizon`,
    `capped_rollout_continuation`, `tactical_solver`, `exclude_capped_play_draw` and
    `engine_error_leaf`.
  - Seed reading: the verification asks for "held-out seeds". This run uses a confirm-role
    master that no ledger under `docs/port/` had used, run once. That follows this task's seed
    rule (tuning, dev and confirm only); eval and review masters stay with the final
    evaluation and the reviewer.
  - The gap list's command used `--role eval`. If the lead wants the eval-role run as well,
    it is the same command with an eval master after the final evaluation.
- **What was done:** the run below, with `docs/port/eval/GAP-A2/w18_fidelity_check.py`, which
  checks a fidelity report against the verification. It also reproduces P6-D's pre-M15 table
  from `docs/port/eval/P6-D/fidelity-fast-standard.json`, "Verification: met".
- **Command** (the release `gcg-eval` built from `1007ca9`, the shipped presets of M15;
  `--workers 3`):

  ```text
  gcg-eval fidelity search:fast search:standard \
    --variants red-green-zeon,tests/data/decks/red-green-zeon-weakened.txt,purple-white-tekkadan,docs/port/eval/P6-D/decks/purple-white-tekkadan-weakened.txt,docs/port/eval/P6-D/decks/blue-white-federation-weakened.txt \
    --against blue-white-federation --games 200 --seed 30721801 --role confirm \
    --ledger docs/port/eval/GAP-A2/seed-ledger.json --workers 3 \
    --out docs/port/eval/GAP-A2/fidelity-fast-standard-m15.json
  uv run python docs/port/eval/GAP-A2/w18_fidelity_check.py docs/port/eval/GAP-A2/fidelity-fast-standard-m15.json
  ```

- **Evidence** (`docs/port/eval/GAP-A2/fidelity-fast-standard-m15.json`, every game included;
  200 games per variant and agent on shared deals, each agent in both seats):

  | Variant | `fast` wins (Wilson 95%) | `standard` wins (Wilson 95%) | paired difference, fast minus standard (Newcombe 95%) | `standard` resolves | `fast` favours the same deck |
  | --- | --- | --- | --- | --- | --- |
  | `red-green-zeon` | 92/200 (0.460, [0.392, 0.529]) | 98/200 (0.490, [0.422, 0.559]) | -0.030 [-0.100, +0.040] | no | — |
  | `red-green-zeon-weakened` | 48/200 (0.240, [0.186, 0.304]) | 38/200 (0.190, [0.142, 0.250]) | +0.050 [-0.007, +0.107] | yes | yes |
  | `purple-white-tekkadan` | 80/200 (0.400, [0.335, 0.469]) | 81/200 (0.405, [0.339, 0.474]) | -0.005 [-0.080, +0.070] | yes | yes |
  | `purple-white-tekkadan-weakened` | 34/200 (0.170, [0.124, 0.228]) | 46/200 (0.230, [0.177, 0.293]) | -0.060 [-0.123, +0.003] | yes | yes |
  | `blue-white-federation-weakened` | 72/200 (0.360, [0.297, 0.429]) | 71/200 (0.355, [0.292, 0.423]) | +0.005 [-0.059, +0.069] | yes | yes |

  - Rank agreement: Spearman 0.9, Kendall tau-b 0.8, pairwise agreement 0.9, the same best
    variant (`red-green-zeon`).
  - Against the verification:
    - wherever `standard` resolves a matchup (4 of 5 variants), `fast` favours the same deck;
    - every paired difference interval includes 0. Met.
  - The one discordant pair of variants is the two weakened Zeon and Tekkadan decks at the
    bottom (fast 0.240 against 0.170, standard 0.190 against 0.230). Their paired differences
    are the largest, +0.050 and -0.060, and both intervals include 0.
  - On the pre-M15 presets (confirm 30460001, P6-D) the same protocol gave Spearman 0.9747,
    Kendall 0.9487 and pairwise agreement 1.0.
- **Ablation:** no AI effect (the run measures the shipped presets; no code path changed). The
  `fast` and `standard` rows against the reference are M15's and E2's (unchanged).
- **tests.json:** T-131 (the fidelity run on the shipped presets: met, on confirm-role seeds as
  read above). The slow-suite port `crates/gcg-search/tests/scenarios/p6d_fast_preset.rs`
  passes.

## W-31 — Heuristic card and fight values are rough (the keyword half)

- **Final status:** resolved by measurement (not adopted). The flat `KEYWORD_BONUS` of 0.3 per
  keyword stays in the policy. The fight half was already measured: `exact_fight` is null and
  not adopted.
- **Decision and evidence:** the resolution plan asks for "keyword worth from the per-keyword
  evaluation features fitted in W-37". The only fitted per-keyword worths are the weights of
  W-37's `keyword_features` models (P6-G). They are no usable worth:
  - `docs/port/eval/GAP-A2/w31_keyword_worth.py` reads them from
    `docs/port/eval/P6-G/fit-round1.json`. It gives each keyword's worth to its owner in each
    seat: `w_tp` for the turn player and `-w_ot` for the other player.
  - The script converts the worth to the policy's own units. `unit_value` counts one point of
    AP or remaining HP as 0.5 (`0.5 * (ap + hp_left)`, `ai/values.py:22-33`). The fit prices
    one point at the mean of its `ap` and `hp` weights, 0.1374 log-odds. So one policy unit is
    0.2748 log-odds, and the flat bonus is 0.3 of these units.
  - The true-state model (`kw=1`, lambda 3), in policy units:

    | Keyword | owner = turn player | owner = other player |
    | --- | --- | --- |
    | Repair | -0.40 | -0.36 |
    | Breach | +0.06 | -0.11 |
    | Support | -0.27 | -0.41 |
    | Blocker | +0.03 | -0.28 |
    | First Strike | 0.00 | 0.00 (weight exactly 0: the fit saw no variation) |
    | High-Maneuver | +0.34 | -1.19 |
    | Suppression | +3.42 | +2.10 |
    | (reference flat bonus) | +0.30 | +0.30 |

    The determinised model (`kw=1,det=1`, lambda 10) gives the same signs, within 0.12 units.
  - Repair and Support are worth less than nothing to their owner in both seats.
  - Breach, Blocker and High-Maneuver change sign between the seats.
  - Suppression is worth 2.1 to 3.4 units. A 3 AP / 3 HP Level 3 Unit has a `unit_value` of
    3.75, so the fit prices the keyword at almost a whole Unit.
  - So the coefficients are associations of a linear fit with whatever comes with a keyword
    in the tuning games, not values of the keywords.
  - The only game-level use of them measured negative: `keyword_features` individual (dev
    20700100) 194/400 (0.485, [0.436, 0.534]), -0.030 [-0.100, +0.040], discordant 23/29.
    High budget (dev 20700200) 184/400 (0.460), -0.080 [-0.159, +0.000], discordant 26/42
    (`docs/port/eval/P6-G/rows/`).
  - Putting these numbers into the policy's card worth would carry the same associations into
    every rollout and prior score, with no game-level row in their favour.
- **What was done:** the script above. No mechanism is added. The policy (`gcg_ai::values`)
  and every preset are unchanged.
- **Ablation:** the W-37 `keyword_features` rows above (individual and high budget) and the
  `exact_fight` rows of the fight half: individual (dev 20600100) 201/400 (0.502, [0.454,
  0.551]), +0.005 [-0.063, +0.073], discordant 25/24, 9726 vs 9770 / 243522 vs 118208
  (`docs/port/eval/P6-F/rows/dev-exact_fight-400.json`). Leave-one-out: not applicable (not in
  the shipped bundle).
- **tests.json:** none (the unit test `exact_fights_equal_the_engine_battles` is P6-F's).

## W-36 and W-142 — the held-out panel decks in the log-loss tables

- **Final status:** W-36 and W-142 are resolved by measurement (not adopted). The held-out
  per-deck log-loss table is not produced: PLAN does not allow it under the seed roles this
  package may use, and its gate decides nothing while no refit ships.
- **Reading of PLAN (recorded as asked):**
  - §12.5 makes the 13 pairs with `blue-red-tempo` or `green-white-link` held-out pairs,
    "never used in tuning, screening or budget curves".
  - `gcg-eval` refuses held-out pairs under `--role tuning|dev|confirm`. The refusal was
    checked here: `gcg-eval tune-leaf --decks panel --pairs held-out --games 8 --seed 30721802
    --role confirm` exits 1 with "--role confirm may not play the held-out pair
    blue-white-federation:blue-red-tempo (PLAN §12.5)". It plays nothing and records no master.
  - A14 leaves held-out play to the `eval` masters of the final evaluation (§12.6 "Eval-role
    masters only") and the `review` masters of the Phase 7 reviewer. This task forbids both.
  - Scoring a fitted model on held-out positions is evaluation, not tuning. So PLAN would
    allow it, but only after the final evaluation and under a fresh eval master, with
    positions recorded on the held-out pairs (gap list: "after the final evaluation").
  - No positions are recorded on held-out pairs today: every `tune-leaf` source is a tuning
    pair (`docs/port/eval/P6-G/fit-round1.json` `sources`). The fits' "held-out pair",
    purple-blue-swarm : red-purple-control, is a pair of tuning decks.
  - The table is the verification of an adoption gate. W-36: "no example or held-out panel
    deck worse than `ROLE_WEIGHTS` by more than one standard error" for the refit. W-142:
    "the strengthened presets use whichever fit is better out of fold".
  - Using held-out positions to choose a fit would make them a screening step (§12.5). The
    held-out bounds of E1-E4 would then no longer be independent of the choice.
  - Every fit it would gate measured negative and is off in both presets, which use
    `ROLE_WEIGHTS`:
    - `refit_weights` (W-34/W-36): individual -0.035, high budget -0.060.
    - `determinized_fit` (W-142): individual -0.030, high budget -0.055.
    - `leaf_density` (W-142) measured null and does not change the weights.
- **Decision:** the held-out per-deck log-loss gate is moot while no refit and no
  `determinized_fit` ships. If a refit is ever proposed for adoption, the lead records leaf
  positions on the 13 held-out pairs after the final evaluation, under a fresh eval-range
  master in the ledger. The command is `gcg-eval tune-leaf --decks panel --pairs held-out
  --role eval --record FILE ...`. The lead then scores the embedded models with
  `--test-positions FILE` before deciding.
- **Evidence (unchanged, P6-G):**
  - The per-deck out-of-fold and leave-one-deck-out tables of the example and tuning decks
    (`docs/port/eval/P6-G/fit-deck-transfer.json`, `fit-transfer.json`). The gate holds for
    the three example decks and fails for red-purple-control under every refit.
  - The determinised and true-state log-loss side by side (`fit-round1.json`).
  - `mech::p6g::tests::w142_recorded_features_are_the_leaf_features` (pass).
- **Ablation:** as recorded in W-34 (`refit_weights`) and W-142 (`determinized_fit`,
  `leaf_density`; `docs/port/eval/P6-G/rows/`). No new row: nothing here changes a search.
- **tests.json:** none.

## W-45 — Mulligan heuristic (the recorded reason for not adopting `paired_mulligan`)

- **Final status:** resolved by measurement (not adopted). The mechanism, its scenario
  evidence and its rows are P6-H's (`docs/port/eval/weakness-status/P6-H.md`). This entry
  adds the missing reason (gap list §9).
- **Reason:** `paired_mulligan` is not adopted because its individual row on dev seeds is
  negative: 188/400 = 0.470 [0.422, 0.519], paired difference -0.060 [-0.137, +0.018],
  discordant 26/38. The adoption rule (§12.4, `docs/ASSUMPTIONS.md:92`) keeps only mechanisms
  "with a measured non-negative effect", with "the bundle measured at game level". So the
  candidate was not carried into the M15 bundles (`B_core`, `B_full`, `B_mid`), and the
  shipped presets keep the reference mulligan rule. Also noted:
  - The confirm row is positive: 836/1600 = 0.522 [0.498, 0.547], +0.045 [+0.009, +0.081],
    128/92.
  - The game-by-game effect against the null control is positive and not significant in
    both runs: dev +0.0175 [-0.0049, +0.0399], confirm +0.0088 [-0.0009, +0.0184].
  - The mechanism costs about 6,100 engine applies per REDRAW decision at the reference
    `standard` budget and 12,200 at `strong`. These are not counted in applies per game.
  - Revisiting it would take a `B_core` + `paired_mulligan` against `B_core` row, which the
    lead may run.
- **Ablation:** P6-H's rows, unchanged:

  | Row | wins/N | Wilson 95% | paired difference (Newcombe 95%) | discordant | iterations / applies per game (challenger vs opponent) | file |
  | --- | --- | --- | --- | --- | --- | --- |
  | individual, dev 20800045 | 188/400 = 0.470 | [0.422, 0.519] | -0.060 [-0.137, +0.018] | 26/38 | 10,006.2 vs 9,915.5 / 120,761.1 vs 119,326.7 | `docs/port/eval/P6-H/w45-paired-mulligan-individual-dev.json` |
  | null control, dev 20800045 | 181/400 = 0.453 | [0.404, 0.501] | -0.095 [-0.168, -0.021] | 20/39 | 9,928.3 vs 9,994.4 / 120,055.6 vs 119,578.3 | `w45-null-control-dev.json` |
  | individual, confirm 30800045 | 836/1600 = 0.522 | [0.498, 0.547] | +0.045 [+0.009, +0.081] | 128/92 | 9,884.0 vs 9,907.1 / 119,036.2 vs 119,757.9 | `w45-paired-mulligan-individual-confirm.json` |
  | null control, confirm 30800045 | 822/1600 = 0.514 | [0.489, 0.538] | +0.028 [-0.007, +0.062] | 111/89 | 9,863.8 vs 9,937.1 / 119,127.2 vs 119,856.9 | `w45-null-control-confirm.json` |

  Leave-one-out: not applicable (not in the shipped bundle).
- **tests.json:** T-122 (scenario test W-45, P6-H, pass).

## W-137 — The determinism contract (strengthened-search math)

- **Final status:** open. Item 1 is resolved here. Item 3's rows are measured here. Two parts
  stay open, each for a reason outside this package:
  - Item 2, the cross-platform run on macOS arm64 and Linux x86-64, needs a Linux runner: the
    CI push that M17 asks the user to approve (A29, T-133). The check itself is now one test,
    below; it passes on macOS arm64.
  - Item 3, adopting `portable_math` in the presets, is a preset change. The presets are frozen
    while the final evaluation runs, so adoption is the lead's decision. The rows are below.
- **Item 1, what was done:** `portable_math` now reaches every float path of the strengthened
  search, including the shared `gcg-ai` code it calls. The reference paths are unchanged.
  - `crates/gcg-ai/src/math.rs` (new): `FloatMath::{Platform, Portable}`, the provider of
    `exp`, `ln`, squares and `sqrt` for the shared AI functions. `Platform` is
    `gcg_core::libm`. `Portable` is `gcg_core::portable` (the pure-Rust `libm` crate) and
    `x * x`. `sqrt` is IEEE-754 in both.
  - Parallel `_with` variants take the provider:
    - `logistic_with`, `evaluate_with` (`evaluation.rs`);
    - `pick_with`, `choose_with` (`policy.rs`);
    - `play_to_end_with` (`playout.rs`);
    - `first_player_choice_with` (`setup.rs`), for the simulations' Gumbel draws, the squares
      and the `sqrt`;
    - `without_effect_with` (`noop.rs`), for the branch check's `evaluate` comparison.
  - The frozen entry points (`logistic`, `evaluate`, `pick`, `choose`, `play_to_end`,
    `first_player_choice`, `without_effect`) are now one-line calls of their variants at
    `FloatMath::Platform`. Their signatures are unchanged, so `gcg-ai-reference` and every
    other caller compile and run as before.
  - `gcg-search`:
    - `p6o::math::Math::float()` maps the search's math to the provider.
    - `reference::root_moves` calls `without_effect_with` and `reference::pregame` calls
      `first_player_choice_with`, both at `math_of(cfg).float()`.
    - P6-E's root (`p6e/mod.rs`) calls `without_effect_with`, and its no-effect check
      (`p6e/noop.rs`) calls `evaluate_with` through a new `Opts::math`.
    - With `portable_math` off every call is the `Platform` variant, which is the frozen
      function.
  - `mulligan_choice` and the argmax policy (`choose(.., None, 0.0, ..)`, used by greedy, the
    no-effect lines, the solver and the look-aheads) call no library function, so they need
    no variant.
- **Item 1, evidence** (tests pass):
  - Reference identity:
    - the tier-1 corpus is 3804/3804, with the `search` set 9/9 and the `baselines` set 8/8;
    - `crates/gcg-search/tests/null_config.rs`, `null_config_traces.rs` and, with `--features
      slow`, `null_config_search_set.rs` are unchanged and pass;
    - the seed-1 equivalence smoke is 3/3 (gate below).
  - `crates/gcg-ai/src/math.rs` tests:
    - `platform_is_gcg_core_libm_and_portable_is_the_libm_crate`: bit for bit, 17 inputs.
    - `the_providers_agree_within_one_ulp`.
    - `every_platform_libm_call_of_the_crate_is_in_this_module`, the lint for `gcg-ai`: no
      `libm::` call outside `math.rs`, and every transcendental method call elsewhere is on a
      `FloatMath` named `math`. Its scanner has the negative control
      `the_lint_names_the_receiver`.
  - `crates/gcg-search/src/mech/gapa2/mod.rs` tests:
    - `the_search_math_and_the_gcg_ai_provider_are_one_library`: `Math` and `FloatMath` give
      the same bits for `exp`, `ln`, squares, `sqrt` and the logistic, in both libraries, and
      `logistic_with(Platform)` is `gcg_ai::logistic` bit for bit.
    - `the_search_policy_move_is_gcg_ai_choose_with_its_library`: 160 policy moves on 40
      vector positions at temperatures 0, 0.3, 0.5 and 1.5. The search's portable move is
      `gcg_ai::choose_with(Portable)` draw for draw, and its platform move is `gcg_ai::choose`.
    - `the_reference_play_draw_choice_takes_the_configuration_math`: on the 6 CHOOSE_FIRST
      records, `reference::pregame` equals `first_player_choice` without `portable_math` and
      `first_player_choice_with(Portable)` with it (action, alternatives, generator state).
    - `every_float_path_of_the_search_goes_through_its_math`, the lint for `gcg-search`:
      outside `p6o/math.rs`, no non-test source calls `libm::` or a transcendental method on
      anything but the search's `math`. None imports or calls the platform-only `gcg-ai`
      entry points (`evaluate`, `logistic`, `without_effect`, `first_player_choice`,
      `play_to_end`, `play_until_turn`), and every `gcg-ai` `choose`/`pick` call passes `None`
      as the generator. Negative controls: `the_float_path_lint_finds_platform_calls`, and a
      manual run with `without_effect` imported again into `reference.rs`, which the lint
      failed with "mech/reference.rs: imports gcg_ai::without_effect, which fixes the platform
      library".
  - `crates/gcg-search/tests/scenarios/gapa2_portable_math.rs`, the item 2 check:
    - `portable_math_searches_give_the_recorded_bits_on_every_platform` records 46 stable keys
      (SHA-256) of portable searches and pre-game choices. Each key covers the action, every
      alternative's value bits and visits, the statistics, the mechanism counters and the
      generator state.
    - The runs: the 8 first searched vector decisions, the 6 CHOOSE_FIRST and 4 REDRAW
      records, under the shipped `fast`, a "wide" configuration (22 flags: every package with
      a float path, at the reference `standard` budget) and the null configuration, each with
      `portable_math`.
    - The keys are equal on every IEEE-754 platform by construction, so on the Linux CI job
      this test is the cross-platform check. It passes on macOS arm64, where the keys were
      recorded.
    - `the_recorded_bits_depend_on_the_library` (golden platform only): with the platform
      library, 1 of the 46 keys differs, so the test sees the library.
    - `portable_math_searches_repeat_exactly`.
  - Unchanged and passing: `mech::p6o::math::tests::*`,
    `mech::tests::portable_math_runs_the_same_search_with_portable_functions`, and
    `crates/gcg-core/tests/vectors.rs::libm_parity_matches`.
- **Item 3, ablation rows** (`portable_math` on against off). The flag is expected to be null:
  the libraries differ in the last bit of about 8% of `exp`/`ln` results (P6-O's 8,000-input
  comparison), which rarely moves a decision.

  Every row: `gcg-eval ablation`, 15 tuning pairs, 400 games, `--workers 1`, the release
  `gcg-eval` with the runtime code of `6bdd500`. The versus column compares the row with its control game by game (the
  same master, so the same deals and agent seeds; `docs/port/eval/M15/rows_table.py versus`).
  "Effort" games differ only in iterations or applies; a "moves" game has another decision
  count or length.

  | Row | wins/N (rate, Wilson 95%) | paired difference (Newcombe 95%) | discordant | iterations / applies per game (challenger vs opponent) | against its control, game by game | evidence |
  | --- | --- | --- | --- | --- | --- | --- |
  | individual (dev 22721400): `search:null-standard:portable_math=true` vs `ref:standard` | 202/400 (0.505, [0.456, 0.554]) | +0.010 [-0.054, +0.074] | 23/21 | 9925 vs 9943 / 119782 vs 120899 | null control (`portable_math=false`, same master): 202/400, +0.010 [-0.054, +0.074], 23/21. Effort 3 games, moves 0, winner 0 | `docs/port/eval/GAP-A2/rows/dev-individual-portable_math-400.json`, `dev-null-control-400.json` |
  | leave-one-out, shipped `fast` (dev 22721500): `search:fast:portable_math=true` vs `search:fast` | 214/400 (0.535, [0.486, 0.583]) | +0.070 [+0.004, +0.135] | 30/16 | 14819 vs 14900 / 278932 vs 280563 | placebo (`search:fast` against itself without `capped_rollout_continuation`, 0 capped rollouts): 214/400, +0.070 [+0.004, +0.135], 30/16. Effort 10 games, moves 1 (game 200: 105 against 107 decisions), winner 0; difference +0.0000 [+0.0000, +0.0000] | `dev-loo-fast-portable_math-400.json`, `dev-loo-fast-placebo-400.json` |
  | leave-one-out, high budget (dev 22721600): the shipped `standard` mechanisms at 2x reference `strong` (256/640/2560, M15's bundle budget), with against without the flag | 184/400 (0.460, [0.412, 0.509]) | -0.080 [-0.154, -0.005] | 22/38 | 32021 vs 32498 / 590975 vs 601004 | placebo (the bundle against itself without `capped_rollout_continuation`, 0 capped rollouts): 184/400, -0.080 [-0.154, -0.005], 22/38. Effort 14 games, moves 1 (game 202: 78 against 80 decisions), winner 0; difference +0.0000 [+0.0000, +0.0000] | `dev-hb-loo-standard2x-portable_math-400.json`, `dev-hb-loo-standard2x-placebo-400.json` |

  The leave-one-out rows' paired differences are the protocol's placebo noise, not the flag.
  - Agent seeds depend on the game index and the seat (`crates/gcg-eval/src/specs.rs`,
    `mirrored_specs`), so two identical agents still disagree in 46 (`fast`) and 60 (high
    budget) of 200 deals.
  - The placebos, with no mechanism difference at all, reach +0.070 [+0.004, +0.135] and
    -0.080 [-0.154, -0.005]: the `portable_math` rows are those numbers.
  - Game by game against its placebo, `portable_math` changed no winner in either family.
  - So no row is promising. No confirm row was run, because a 1,600-game confirm row would
    measure the placebo of its own master, not the flag.

  `portable_math` measures null: no winner changes in 1,200 games against its controls, at
  equal cost (the challenger's applies per game are within 0.1% of its control's in each
  family). By §12.4 it is adoptable (non-negative). Adopted, it would make the shipped presets
  platform-independent, once item 2's Linux run confirms the recorded bits. Adopting it is a
  change to `presets.rs`, so it is the lead's decision. It would change a few shipped-preset games, and with them
  `crates/gcg-cli/tests/vectors/parity/rust-replays.json` (`export_parity.py --section
  rust-replays`) and `tests/null_config.rs`'s pin of the shipped flag set. The rows above are
  the evidence for it.
- **tests.json:** none is satisfied in full. T-133 (the CI job) waits on the user's approval
  of the push; `gapa2_portable_math.rs` is the test it should run.

## Files

| Path | Change |
| --- | --- |
| `crates/gcg-ai/src/math.rs` | new: `FloatMath`, its tests and the `gcg-ai` lint (W-137) |
| `crates/gcg-ai/src/{evaluation,policy,playout,setup,noop}.rs` | the `_with` variants; the frozen functions call them at `Platform` (W-137) |
| `crates/gcg-ai/src/actions.rs` | `RestPrune`, `rest_prune` (W-13; analysis only) |
| `crates/gcg-ai/src/lib.rs` | exports |
| `crates/gcg-search/src/mech/p6o/math.rs` | `Math::float` |
| `crates/gcg-search/src/mech/reference.rs` | `without_effect_with`, `first_player_choice_with` at the configuration's math |
| `crates/gcg-search/src/mech/p6e/{mod,noop,tests}.rs` | the same for P6-E's root and no-effect check (`Opts::math`) |
| `crates/gcg-search/src/mech/mod.rs` | `#[cfg(test)] mod gapa2;` |
| `crates/gcg-search/src/mech/gapa2/mod.rs` | new: the W-137 tests and the `gcg-search` lint |
| `crates/gcg-search/tests/scenarios/gapa2_portable_math.rs`, `main.rs` | new: the recorded portable bits (W-137 item 2) |
| `crates/gcg-eval/src/trace.rs` | the `rest_prune` field of the decision trace (W-13) |
| `crates/gcg-eval/tests/gapa2_w13.rs` | new: the W-13 e2e tests |
| `docs/port/eval/GAP-A2/` | coverage decks, scripts, reports, rows, seed ledger |
| `docs/port/eval/weakness-status/GAP-A2.md` | this file |

Interface notes for the lead:

1. Some edits fall outside the files this package owned. They are additive or one-line call
   changes:
   - `crates/gcg-search/src/mech/{mod,reference}.rs`, `p6e/*`, `p6o/math.rs`;
   - `crates/gcg-eval/src/trace.rs`;
   - `crates/gcg-ai/src/actions.rs` (`rest_prune`, which is not portable-math work).

   `contracts.rs` and `docs/port/contracts.md` are unchanged: the frozen signatures are
   untouched and the new functions are additive. The lead may list the `_with` variants and
   `rest_prune` in `contracts.md`.
2. The decision trace gained one key, `rest_prune`. The trace is an opt-in `gcg-eval` output
   (A30). `gcg-sim`, `reference-benchmark` and every report are unchanged (W-21's test and
   this package's e2e compare them byte for byte).
3. The `portable_math` flag documentation in `crates/gcg-search/src/flags.rs` still lists
   only the search's own functions. With this package the flag also covers the shared
   `gcg-ai` calls, which the lead may add to that doc comment.

## Gate

Run after the last code change (commit `6bdd500`; later edits are this file and evidence).
Logs are not committed.

| Command | Result |
| --- | --- |
| `cargo fmt --all --check` | clean |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | clean |
| `cargo test --workspace --release --no-fail-fast` (3 test threads) | 1,249 passed, 0 failed, 0 ignored (136 result lines): M15's 1,235 plus the 14 tests of this package |
| `cargo test --release -p gcg-search --features slow --no-fail-fast` | 280 passed, 0 failed (19 result lines), including `null_config_search_set::the_copy_replays_every_decision_of_the_search_set` |
| `target/release/gcg-diff corpus --tier 1 --require-all` | 3804/3804 traces (`search` 9/9, `baselines` 8/8) |
| `scripts/compare_python_vs_rust.sh equivalence --matches 2 --seeds 1 --formats bo1 --pairs red-green-zeon:blue-white-federation --workers 1 --decision-logs off` | 3/3 identical (`fast`, `standard`, `strong`) |
| `gcg-eval evidence check` | pass (262 cited paths, 13 cited commits) |
| `uv run python scripts/check_doc_commands.py --list docs/port/eval/weakness-status/GAP-A2.md` | 0 blocks to run |
| `uv run ruff check .`, `uv run mypy` | clean (mypy: 101 files) |
| `uv run ruff format --check .` | 1 file would be reformatted: `port/oracle/export_cli.py`. It is unchanged since `d750b6c`, so the failure predates this package and is outside its files. The three GAP-A2 scripts pass `ruff check`, `ruff format --check` and `mypy --strict` |

## Not done here

- W-137 item 2 needs the Linux x86-64 CI run of
  `crates/gcg-search/tests/scenarios/gapa2_portable_math.rs` and the tier-1 scenario tests.
  That waits on the user's approval of the CI push (M17, T-133).
- W-137 item 3, adopting `portable_math` in the presets, is the lead's decision. The presets
  are frozen while the final evaluation runs.
- W-137 item 3 was not measured on the shipped `standard` at its own 8x budget. The
  high-budget row ran at M15's bundle budget (2x reference `strong`) with the shipped
  mechanisms. The 8x row is `gcg-eval ablation leave-one-out --bundle
  search:standard:portable_math=true --mechanism portable_math=false` with a placebo on the
  same master; it costs about 2.5 hours at 4 workers.
- W-36 and W-142: the held-out per-deck log-loss table is not produced (decision above).
- W-18: if the lead wants an eval-role repeat, it runs after the final evaluation.
- `tests.json`: T-113 and T-131 can be marked passing (lead-owned file).
