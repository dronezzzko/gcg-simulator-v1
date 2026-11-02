# M15: the strengthened-search bundle and the tuned presets

Experiment record of milestone M15 (docs/port/PLAN.md §12.4, §12.8, §12.9 item 4): the bundle
of the shipped presets, its leave-one-out rows, the budget curves of `standard` and `fast`, the
per-game cost of each scale, and the candidate presets written into
`crates/gcg-search/src/presets.rs`. Written on 2026-11-02 in worktree branch
`worktree-agent-a978255adc503d8c9` from `e14c839` (M14). The final evaluation (§12.6) and the
official Part 2 timing on an idle machine are the lead's and have not been run.

## Method

- **Binaries.** Steps 1-4 ran one release `gcg-eval` built from `e14c839` (copied before
  `presets.rs` changed, so `search:standard` and `search:fast` in those commands are the
  pre-M15 presets: the reference `strong` and `standard` settings with `engine_error_leaf`,
  plus the overrides each command names). Step 5 ran the release build of this commit.
- **Rows.** `gcg-eval ablation`, `budget-curve` and `head-to-head` on the 15 tuning pairs of
  `docs/port/eval/panel.json`, mirrored blocks of four, forced play/draw (`--choose-first
  seat0`, A24) unless a row says otherwise, `--workers 8`. Results do not depend on the worker
  count. No held-out pair, eval master or review master was used.
- **Seeds.** Every master is in `docs/port/eval/M15/seed-ledger.json` (tuning 11500100 and
  11500200; dev 21500100, 21500200, 21500250, 21500400, 21500410 and the timing probe
  21500900; confirm 30150100). The two step-5 rows ran concurrently with other rows, so they
  wrote a scratch ledger whose entries were appended to the M15 ledger afterwards.
- **Tables.** `uv run python docs/port/eval/M15/rows_table.py rows|versus|versus-points|curve FILE...` prints
  every row below from its report (wins/N, Wilson 95%, paired difference with the Newcombe 95%
  interval, discordant deals, iterations and applies per game, the lowest pair and the number
  of pairs whose upper bound is below 0.5); `timing_table.py` prints the cost table.
- **Load.** The machine (Apple M2 Pro, 10 cores) was shared with a long Python equivalence run
  and other work during every run (load averages 9 to 47). Win rates are unaffected; wall and
  user times are upper bounds of an idle machine's. Instructions retired do not depend on load.

## Step 1: bundle candidates at twice the shipped `standard` budget

`B_core` = `adaptive_horizon` + `capped_rollout_continuation` + `tactical_solver`. `B_full` =
`B_core` + `ex_variants`, `typed_fingerprint`, `prune_worlds=4`, `branch_all_selects`,
`rest_prune_worlds`, `target_rollouts`, `real_idle_line`, `modal_lookahead`,
`burst_decline_list`, `exhaustive_polarity`, `transpositions`, `subtree_reuse`,
`macro_multi_pick`, `leaf_density`, `exclude_capped_play_draw` (`exact_fight` excluded: it
doubles the applies per game for a null row, which would halve the largest `standard` scale
under the floor). `B_mid` = `B_full` without `subtree_reuse` (added as a diagnostic). Each row:
the bundle against the bundle with all its mechanisms off (the null configuration with
`engine_error_leaf`) at `iterations_per_option=256,min_iterations=640,max_iterations=2560`.

```bash
gcg-eval ablation leave-one-out \
  --bundle search:standard:iterations_per_option=256,min_iterations=640,max_iterations=2560,<on> \
  --mechanism <every bundle field off> --games 400 --seed 21500100 --role dev \
  --ledger docs/port/eval/M15/seed-ledger.json --workers 8 --choose-first seat0 \
  --out docs/port/eval/M15/rows/dev-<bundle>-vs-null2x-400.json
# confirmation: --games 1600 --seed 30150100 --role confirm, run once
```

| Row | wins/N (rate, Wilson 95%) | paired difference (Newcombe 95%) | discordant | iterations / applies per game (challenger vs opponent) | vs `B_core` on the same deals | evidence |
| --- | --- | --- | --- | --- | --- | --- |
| `B_core` (dev 21500100) | 217/400 (0.542, [0.493, 0.591]) | +0.085 [+0.007, +0.162] | 41/24 | 32039 vs 32385 / 593343 vs 372808 | — | `rows/dev-bcore-vs-null2x-400.json` |
| `B_full` (dev 21500100) | 193/400 (0.482, [0.434, 0.531]) | -0.035 [-0.101, +0.032] | 20/27 | 24467 vs 31826 / 465773 vs 364277 | 385 games changed; 43 / 67 wins; -0.060 [-0.111, -0.009] | `rows/dev-bfull-vs-null2x-400.json` |
| `B_mid` (dev 21500100) | 212/400 (0.530, [0.481, 0.578]) | +0.060 [-0.019, +0.138] | 39/27 | 31313 vs 31565 / 593929 vs 359693 | 383 games changed; 47 / 52 wins; -0.013 [-0.061, +0.036] | `rows/dev-bmid-vs-null2x-400.json` |
| `B_core` confirmation (confirm 30150100) | 854/1600 (0.534, [0.509, 0.558]) | **+0.068 [+0.030, +0.105]** | 148/94 | 32329 vs 32394 / 595018 vs 373624 | — | `rows/confirm-bcore-vs-null2x-1600.json` |

`B_core` is chosen. `B_full` is worse because `subtree_reuse` counts the kept subtree's visits
toward the budget (656 fresh iterations per search against 843); without it the other 14
mechanisms are null on top of `B_core`. The timing probe `timing/probe-bcore-60.json` (60
games, dev 21500900) only sized the runs.

## Step 2: leave-one-out rows of `B_core` (dev 21500200)

`gcg-eval ablation leave-one-out --bundle search:standard:iterations_per_option=256,
min_iterations=640,max_iterations=2560,adaptive_horizon=true,capped_rollout_continuation=true,
tactical_solver=true --mechanism <field>=<off> --games 400 --seed 21500200 --role dev ...`;
every row shares its deals and its challenger (the bundle). `capped_rollout_continuation` never
fires (0 capped rollouts in every row), so its row is the family's placebo. A positive paired
difference means the mechanism helps.

| Row (mechanism removed) | wins/N (rate, Wilson 95%) | paired difference (Newcombe 95%) | discordant | iterations / applies per game (challenger vs opponent) | vs placebo, game by game | evidence |
| --- | --- | --- | --- | --- | --- | --- |
| `capped_rollout_continuation` (placebo) | 207/400 (0.517, [0.469, 0.566]) | +0.035 [-0.032, +0.101] | 27/20 | 32778 vs 32388 / 598554 vs 579017 | — | `rows/dev-loo-capped_rollout_continuation-400.json` |
| `adaptive_horizon` | 203/400 (0.507, [0.459, 0.556]) | +0.015 [-0.058, +0.088] | 30/27 | 31734 vs 32752 / 579343 vs 407670 | 388 changed; 46 / 50; -0.010 [-0.058, +0.038] | `rows/dev-loo-adaptive_horizon-400.json` |
| `tactical_solver` | 215/400 (0.537, [0.488, 0.586]) | +0.075 [+0.009, +0.140] | 31/16 | 32808 vs 32150 / 580619 vs 476924 | 297 changed; 27 / 19; +0.020 [-0.013, +0.053] | `rows/dev-loo-tactical_solver-400.json` |

The two guards of the shipped flag set got leave-one-out rows too: `engine_error_leaf` in the
same family (`rows/dev-loo-engine_error_leaf-400.json`: identical to the placebo game for game,
0 of 400 changed) and `exclude_capped_play_draw` with `--choose-first agents` on dev 21500250
against that protocol's own placebo (`rows/dev-loo-agents-*-400.json`: 216/400 both, 0 of 400
games changed). Details: `docs/port/eval/weakness-status/M15-bundle.md`.

No removal helps (no paired difference at or below -0.03), so nothing is dropped and the
confirmed `B_core` row of step 1 is the bundle's measurement (no re-measure needed). The
shipped flag set is `B_core` plus the two zero-cost guards `exclude_capped_play_draw` (W-44,
pre-game only) and `engine_error_leaf` (W-143, the pre-M15 default).

## Step 3: `standard` budget curve against `ref:strong` (tuning 11500100)

```bash
gcg-eval budget-curve search:standard:adaptive_horizon=true,capped_rollout_continuation=true,tactical_solver=true,exclude_capped_play_draw=true \
  ref:strong --scales 1,2,4,8 --games 400 --seed 11500100 --role tuning \
  --ledger docs/port/eval/M15/seed-ledger.json --workers 8 --choose-first seat0 \
  --out docs/port/eval/M15/curves/tuning-standard-bundle-vs-ref-strong.json
```

| scale | budget (ipo/min/max) | wins/N (rate, Wilson 95%) | paired difference (Newcombe 95%) | discordant | iterations / applies per game (challenger vs opponent) | lowest pair | pairs UB < 0.5 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | 128/320/1280 | 195/400 (0.487, [0.439, 0.536]) | -0.025 [-0.091, +0.042] | 21/26 | 17200 vs 16986 / 318334 vs 199259 | 9/24 (UB 0.573) | 0 |
| 2 | 256/640/2560 | 205/400 (0.512, [0.464, 0.561]) | +0.025 [-0.046, +0.095] | 29/24 | 32777 vs 17228 / 601130 vs 201169 | 11/28 (UB 0.576) | 0 |
| 4 | 512/1280/5120 | 214/400 (0.535, [0.486, 0.583]) | +0.070 [-0.003, +0.141] | 35/21 | 63675 vs 16896 / 1148733 vs 196862 | 11/28 (UB 0.576) | 0 |
| 8 | 1024/2560/10240 | 227/400 (**0.568**, [0.518, 0.615]) | +0.135 [+0.059, +0.208] | 45/18 | 122934 vs 16891 / 2197523 vs 197816 | 13/28 (UB 0.642) | 0 |

The curve rises with the budget and is best at 8x (0.568). No 400-game point can show a Wilson
lower bound of 0.55 unless its rate is about 0.60; the E1 bar is a pooled lower bound over
11,200 games (all pairs) and 5,200 (held-out pairs), where a true rate of 0.568 would give
about 0.559 and 0.555.

**Control: the same budgets without the bundle, on the same deals.** `gcg-eval budget-curve
search:standard ref:strong --scales 2,8 --seed 11500100 --role tuning ...` (the pre-M15
`search:standard`, mechanisms off but `engine_error_leaf`; same master, so game i has the same
deal and the same opponent decision seeds in both curves;
`curves/tuning-standard-nobundle-vs-ref-strong.json`):

| scale | wins/N (rate, Wilson 95%) | paired difference (Newcombe 95%) | discordant | iterations / applies per game (challenger vs opponent) | bundle point against this point, game by game |
| --- | --- | --- | --- | --- | --- |
| 2 | 212/400 (0.530, [0.481, 0.578]) | +0.060 [-0.018, +0.137] | 38/26 | 32236 vs 16499 / 373136 vs 194118 | 391 games changed; 46 / 53 wins; -0.018 [-0.066, +0.031] |
| 8 | 215/400 (0.537, [0.488, 0.586]) | +0.075 [+0.006, +0.143] | 33/18 | 120099 vs 16784 / 1354384 vs 197703 | 387 games changed; 53 / 41 wins; +0.030 [-0.017, +0.077] |

Against `ref:strong` the bundle's effect at equal iterations is within noise (-0.018 at 2x,
+0.030 at 8x), much smaller than its self-play gain against the null configuration (+0.068,
step 1). The lead's earlier curve of the pre-M15 `search:standard` (tuning 10100000:
0.5675, 0.5275, 0.5175, 0.5625 at 2x, 4x, 8x, 16x) and this control (0.530 at 2x, 0.537 at 8x)
put the budget-only plateau at 0.52-0.57 depending on the deals. **E1 is at risk**: the best
tuning-pair estimate (0.568 at 8x with the bundle) sits close to the rate the gates need
(about 0.564 for the held-out lower bound), and nothing measured here adds a clear margin.

## Step 4: `fast` budget curve against `ref:standard` (tuning 11500200)

```bash
gcg-eval budget-curve search:fast:adaptive_horizon=true,capped_rollout_continuation=true,tactical_solver=true,exclude_capped_play_draw=true \
  ref:standard --scales 0.5,0.75,1,1.5,2 --games 400 --seed 11500200 --role tuning \
  --ledger docs/port/eval/M15/seed-ledger.json --workers 8 --choose-first seat0 \
  --out docs/port/eval/M15/curves/tuning-fast-bundle-vs-ref-standard.json
```

| scale | budget (ipo/min/max) | wins/N (rate, Wilson 95%) | paired difference (Newcombe 95%) | discordant | iterations / applies per game (challenger vs opponent) | lowest pair | pairs UB < 0.5 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 0.5 | 32/80/320 | 195/400 (0.487, [0.439, 0.536]) | -0.025 [-0.097, +0.047] | 25/30 | 5190 vs 10312 / 97670 vs 122953 | 10/28 (UB 0.542) | 0 |
| 0.75 | 48/120/480 | 207/400 (0.517, [0.469, 0.566]) | +0.035 [-0.041, +0.110] | 34/27 | 7615 vs 10295 / 144611 vs 122310 | 10/28 (UB 0.542) | 0 |
| 1 | 64/160/640 | 210/400 (0.525, [0.476, 0.574]) | +0.050 [-0.016, +0.115] | 28/18 | 9851 vs 10006 / 185779 vs 119248 | 11/28 (UB 0.576) | 0 |
| 1.5 | 96/240/960 | 228/400 (**0.570**, [0.521, 0.618]) | +0.140 [+0.062, +0.215] | 47/19 | 14822 vs 10088 / 281059 vs 121316 | 14/28 (UB 0.674) | 0 |
| 2 | 128/320/1280 | 224/400 (0.560, [0.511, 0.608]) | +0.120 [+0.047, +0.191] | 41/17 | 18957 vs 10085 / 355132 vs 121290 | 13/28 (UB 0.642) | 0 |

§12.8 step 4 picks the smallest scale with a pooled point estimate of at least 0.53 and no pair
upper bound below 0.5: 0.75 (0.517) and 1 (0.525) miss 0.53, so **`fast` = 1.5x reference
`standard` = 96 / 240 / 960**, 2.3 times the reference `standard`'s applies per game.

## Cost of each scale (Part 2 workload)

`/usr/bin/time -l` of the Part 2 workload (`examples/decks/red-green-zeon.txt
examples/decks/blue-white-federation.txt --format bo3 --seed 1`) through `gcg-eval
preset-benchmark --search-config configs/m15-<preset>-<scale>x.json --ai-preset <preset>` (the
`gcg-sim` pipeline with a candidate; `make_config.py` writes the file) and, for the reference,
`gcg-eval reference-benchmark --reference-preset strong|standard`. Wall times are under the
load in brackets (1-minute load average before the run) and are upper bounds; instructions
retired do not depend on load. The run script is not committed (it only loops over these
commands); every run is a line of `timing/timing.tsv`.

| run | scale | matches / workers | games | wall s (load) | user s | user s per game | instructions per game (G) | instructions vs reference |
| --- | ---: | --- | ---: | --- | ---: | ---: | ---: | ---: |
| ref-strong | 1 | 20 / 10 | 54 | 21.5 (17) | 96 | 1.78 | 12.9 | — |
| standard | 1 | 20 / 10 | 46 | 27.8 (19) | 127 | 2.76 | 19.8 | 1.53x |
| standard | 2 | 20 / 10 | 49 | 50.9 (21) | 225 | 4.59 | 32.9 | 2.54x |
| standard | 4 | 20 / 10 | 51 | 100.4 (24) | 493 | 9.67 | 68.4 | 5.28x |
| standard | 8 | 20 / 10 | 50 | **157.1** (22) | 1151 | 23.01 | 128.5 | 9.92x |
| ref-strong | 1 | 20 / 10 | 54 | 16.5 (14) | 134 | 2.48 | 13.0 | — |
| ref-strong | 1 | 4 / 1 | 10 | 9.9 (16) | 10 | 0.99 | 14.4 | — |
| standard | 1 | 4 / 1 | 11 | 17.5 (14) | 17 | 1.58 | 22.8 | 1.58x |
| standard | 2 | 4 / 1 | 10 | 26.3 (11) | 26 | 2.62 | 37.1 | 2.57x |
| standard | 4 | 4 / 1 | 11 | 81.7 (9) | 70 | 6.39 | 72.6 | 5.03x |
| standard | 8 | 4 / 1 | 11 | 136.3 (47) | 122 | 11.05 | 112.6 | 7.80x |
| ref-standard | 1 | 100 / 10 | 251 | 54.6 (18) | 331 | 1.32 | 8.4 | — |
| fast | 1.5 | 100 / 10 | 245 | **99.9** (22) | 668 | 2.73 | 15.5 | 1.85x |
| ref-standard | 1 | 20 / 1 | 50 | 42.9 (23) | 38 | 0.77 | 8.3 | — |
| fast | 1.5 | 20 / 1 | 47 | **79.8** (20) | 72 | 1.53 | 16.1 | 1.95x |

Applies per game of the candidate (the challenger of the budget curves, panel average):
`standard` 318,334 (1x), 601,130 (2x), 1,148,733 (4x), 2,197,523 (8x) against 197,000-201,000
for `ref:strong`; `fast` 281,059 at 1.5x against 121,316 for `ref:standard`.

Prediction against the floors (Python medians from `docs/port/BENCHMARKS.md`):

- **`standard`, full search** (`--matches 20` at the best worker count; floor 356.0 / 2.2 =
  162 s): 8x ran in 157.1 s at 10 workers under a load average of 22, already below the floor.
  On the same workload it executes 9.9 times the instructions of the reference `strong`, so with
  the lead's idle Part 1 Pair B time T of the reference `strong` it should take about 9.9 T: it
  holds the floor while T ≤ 16.4 s (the reference took 16.5-21.5 s here under load averages of
  14-17). 4x (5.3 T, 100.4 s under load) is the fallback if the idle measurement of 8x misses
  the floor. 16x (about 20 T, roughly 300 s under this load) would not fit.
- **`fast`, screening** (floors 972.0 / 2 = 486 s for 100 matches at 10 workers, 1269.5 / 2 =
  635 s for 20 matches at 1 worker): 99.9 s and 79.8 s under load, 1.85-1.95 times the reference
  `standard`'s instructions: far inside both floors. The best-worker-count headline was not
  measured separately (Python's best is 10 workers, the run above).

## Step 5: the candidate presets

Written into `crates/gcg-search/src/presets.rs` as a candidate (every field spelled out; the
table for the Rust `docs/AI.md` is `preset-table.md`):

| Preset | Mapping | Scale | `iterations_per_option` / `min_iterations` / `max_iterations` | Mechanisms |
| --- | --- | ---: | --- | --- |
| `standard` | reference `strong` | 8 | 1024 / 2560 / 10240 | `adaptive_horizon`, `capped_rollout_continuation`, `tactical_solver`, `exclude_capped_play_draw`, `engine_error_leaf` |
| `fast` | reference `standard` | 1.5 | 96 / 240 / 960 | the same |

Every other setting is the mapped reference preset's. Reasons:

- `standard` = 8x: the largest measured scale whose full-search time fits the floor (157.1 s
  under load, about 9.9 T idle) and the best point of its curve (0.568); the curve did not
  plateau before 8x, and 16x would not fit the floor.
- `fast` = 1.5x: §12.8 step 4's rule (the smallest scale with a pooled estimate of at least 0.53
  and no pair upper bound below 0.5); far inside both screening floors.
- Both presets keep the same mechanisms, so a `fast` screen runs the same search as `standard`
  (`tests/scenarios/p6d_fast_preset.rs`: `fast` differs only in budget and noise settings and
  searches at most half of `standard`'s budget at every width).
- The rebuilt `gcg-sim` presets play the same games as the timed configuration files
  (`gcg-sim benchmark --format bo1 --seed 5 --matches 2` against `preset-benchmark
  --search-config configs/m15-standard-8x.json` and `configs/m15-fast-1.5x.json`: identical
  `games.ndjson`).

Sanity head-to-heads of the rebuilt presets (`gcg-eval head-to-head search:<preset>
ref:<reference> --decks panel --pairs tuning --games 420 --role dev ...`; 420 games, the nearest
count with an equal multiple of 4 per pair, 28 per pair):

| Row | wins/N (rate, Wilson 95%) | paired difference (Newcombe 95%) | discordant | iterations / applies per game (challenger vs opponent) | lowest pair | pairs UB < 0.5 | evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `search:fast` vs `ref:standard` (dev 21500410) | 223/420 (0.531, [0.483, 0.578]) | +0.062 [-0.012, +0.134] | 38/25 | 14656 vs 9909 / 270697 vs 117941 | 11/28 (UB 0.576) | 0 | `rows/dev-sanity-fast-vs-ref-standard-420.json` |
| `search:standard` vs `ref:strong` (dev 21500400) | 246/420 (0.586, [0.538, 0.632]) | +0.171 [+0.097, +0.243] | 52/16 | 118573 vs 16604 / 2070098 vs 191480 | 12/28 (UB 0.609) | 0 | `rows/dev-sanity-standard-vs-ref-strong-420.json` |

Both candidates pass their bars on these tuning-pair rows as far as 400 games can tell: `fast`
0.531 (E2 needs a pooled estimate of at least 0.5 and no pair upper bound below 0.5); `standard`
0.586 (E1's pooled lower bound of 0.55 is out of reach of 420 games; the 8x curve point and this
row together are 473/820 = 0.577 on disjoint deals).

Gate (run after the last code change; logs not committed):

| Command | Result |
| --- | --- |
| `cargo fmt --all --check` | clean |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | clean |
| `cargo test --workspace --release --no-fail-fast` | 1,235 passed, 0 failed, 0 ignored (135 test result lines) |
| `target/release/gcg-diff corpus --tier 1 --require-all` | 3804/3804 traces |
| `uv run python port/oracle/export_parity.py --check --section rust-replays` | current (the vector was regenerated for the new presets, below) |

Code changes beyond the preset constants, all forced by tests that used the shipped presets as
"the reference budget with only `engine_error_leaf`":

- `presets.rs`: `reference_mapping(p)`, the configuration a preset starts from (the mapped
  reference preset renamed, reference budgets, only `engine_error_leaf`: the pre-M15 shipped
  preset), exported from `lib.rs`. The mechanism unit tests (`mech/tests.rs`,
  `compose_tests.rs`, `p6b`, `p6c`, `p6d/priors.rs`, `p6e`, `p6h` tests) and
  `tests/scenarios/p6d_main.rs` now start from it instead of `preset_config`, so each package's
  test keeps the configuration it was written against (no assertion changed).
- `tests/null_config.rs`: the shipped presets are pinned as the mapped reference presets with the
  M15 budgets and mechanisms, and `reference_mapping` as the old assertion (renamed reference,
  `engine_error_leaf` only).
- `tests/engine_error_leaf.rs` (W-143): the shipped `fast` no longer reaches the redirect cycle
  in fuzz game 221, so the cycle assertions run at `reference_mapping(Fast)` (the configuration
  the game failed with); the shipped `fast` still has to finish the game with the same actions
  as the fuzz runner.
- `crates/gcg-cli/tests/vectors/parity/rust-replays.json`: regenerated with
  `uv run python port/oracle/export_parity.py --section rust-replays` (Python verified every
  replay of the new presets; `--check` reproduces it).

## Not done here (the lead's)

- The official Part 2 timing on an idle machine (`scripts/compare_python_vs_rust.sh part2`), and
  Part 1 Pair B, whose time T decides the 8x projection above. If `standard` at 8x misses
  162 s there, the fallback is 4x (`configs/m15-standard-4x.json`).
- The final evaluation (E1-E4, §12.6) on eval masters and held-out pairs, and a larger
  tuning-pair confirmation of `standard` against `ref:strong` before it if wanted: at 8x the
  measured rates (0.568 and 0.586 on 400 and 420 games) are close to the 0.564 the held-out
  lower bound needs.
- Copying `preset-table.md` into the Rust `docs/AI.md` (T-130) and the WEAKNESSES.md status
  lines from `../weakness-status/M15-bundle.md`.

## Files

| Path | Content |
| --- | --- |
| `seed-ledger.json` | every master of M15 |
| `rows/*.json` | ablation and head-to-head reports, every game included |
| `curves/*.json` | budget-curve reports |
| `configs/*.json` | `preset-benchmark --search-config` files: `shipped-*-1x.json` (the pre-M15 presets; their games equal `gcg-sim`'s at `e14c839`), `m15-<preset>-<scale>x.json` (the timed candidates; `m15-standard-8x.json` and `m15-fast-1.5x.json` play the games of the shipped presets of this commit) |
| `timing/timing.tsv`, `timing/probe-bcore-60.json` | the cost runs and the sizing probe |
| `make_config.py`, `rows_table.py`, `timing_table.py` | the configuration writer and the table scripts |
| `preset-table.md` | the preset table for the Rust `docs/AI.md` |
| `../weakness-status/M15-bundle.md` | the §12.10 rows per mechanism |
