# M15 weakness status: the integrated bundle and its leave-one-out rows

Milestone M15 (docs/port/PLAN.md §12.4, §12.8, §12.9 item 4): the strengthened-search bundle of
the shipped presets, its game-level rows against the null configuration, and the leave-one-out
row of every mechanism in it, in the format of §12.10. Written by the M15 experiment engineer on
2026-11-02 in worktree branch `worktree-agent-a978255adc503d8c9`, from `e14c839` (M14).
WEAKNESSES.md is not edited here (the lead records the status lines from this file). The
budgets, the budget curves and the per-game cost are in `docs/port/eval/M15/README.md`.

Every row: `gcg-eval ablation leave-one-out` (the binary built from `e14c839`), 15 tuning pairs
of `docs/port/eval/panel.json`, mirrored blocks of four, forced play/draw (`--choose-first
seat0`, A24) unless the row says otherwise, `--workers 8`. The bundle runs at twice the shipped
`standard` budget (`iterations_per_option=256,min_iterations=640,max_iterations=2560` on
`search:standard`, i.e. reference `strong` settings with `engine_error_leaf`). Masters are in
`docs/port/eval/M15/seed-ledger.json`. "Applies per game" counts every engine apply of the agent
(tree, rollout and auxiliary). The "vs placebo" column (`docs/port/eval/M15/rows_table.py
versus`, the method of `docs/port/eval/P6-F/versus_placebo.py`) compares a row with the placebo
row of its seed family game by game: games with another outcome, length or decision count;
challenger wins only in the row / only in the placebo; the difference per game with a Wald 95%
interval.

## Bundle candidates against the null configuration (step 1)

The opponent is the bundle with all its mechanisms off: `search:standard` at the same budget
(the null configuration plus the `engine_error_leaf` guard, which never fired in these rows).

- `B_core` = `adaptive_horizon` + `capped_rollout_continuation` + `tactical_solver`, the
  mechanisms with a positive or confirmed row (P6-A, P6-B).
- `B_full` = `B_core` + the null-but-non-negative, cheap mechanisms: `ex_variants`,
  `typed_fingerprint`, `prune_worlds=4`, `branch_all_selects`, `rest_prune_worlds`,
  `target_rollouts`, `real_idle_line` (P6-E), `modal_lookahead`, `burst_decline_list`,
  `exhaustive_polarity` (P6-F), `transpositions`, `subtree_reuse`, `macro_multi_pick` (P6-D),
  `leaf_density` (P6-G), `exclude_capped_play_draw` (P6-H). `exact_fight` is left out: it doubles
  the applies per game (243,522 against 118,208 in its P6-F row) for a null effect (+0.005), and
  that cost would halve the largest `standard` scale under the 2x floor.
- `B_mid` = `B_full` without `subtree_reuse` (a diagnostic added here: `B_full` searched 22%
  fewer iterations per search than the others, 656 against 843).

| Row | mechanisms on | wins/N (rate, Wilson 95%) | paired difference (Newcombe 95%) | discordant | iterations / applies per game (challenger vs opponent) | vs `B_core` (same deals) | evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `B_core`, dev 21500100 | 3 | 217/400 (0.542, [0.493, 0.591]) | +0.085 [+0.007, +0.162] | 41/24 | 32039 vs 32385 / 593343 vs 372808 | — | `docs/port/eval/M15/rows/dev-bcore-vs-null2x-400.json` |
| `B_full`, dev 21500100 | 18 | 193/400 (0.482, [0.434, 0.531]) | -0.035 [-0.101, +0.032] | 20/27 | 24467 vs 31826 / 465773 vs 364277 | 385 changed; 43 / 67; -0.0600 [-0.1111, -0.0089] | `docs/port/eval/M15/rows/dev-bfull-vs-null2x-400.json` |
| `B_mid`, dev 21500100 | 17 | 212/400 (0.530, [0.481, 0.578]) | +0.060 [-0.019, +0.138] | 39/27 | 31313 vs 31565 / 593929 vs 359693 | 383 changed; 47 / 52; -0.0125 [-0.0612, +0.0362] | `docs/port/eval/M15/rows/dev-bmid-vs-null2x-400.json` |
| `B_core` confirmation, confirm 30150100 (run once) | 3 | 854/1600 (0.534, [0.509, 0.558]) | +0.068 [+0.030, +0.105] | 148/94 | 32329 vs 32394 / 595018 vs 373624 | — | `docs/port/eval/M15/rows/confirm-bcore-vs-null2x-1600.json` |

Reading:

- `B_core` is the better bundle: +0.085 at dev, confirmed at +0.068 [+0.030, +0.105] on 1,600
  confirm games (148 against 94 discordant deals), at 1.59 times the null configuration's
  applies per game for the same iterations.
- `B_full` loses to `B_core` on the same deals (-0.060 [-0.111, -0.009]). The cause is
  `subtree_reuse`: with it the kept subtree's visits count toward the budget, so each search
  ran 22% fewer fresh iterations (24,467 against 32,039 per game); without it (`B_mid`) the
  iterations are back (31,313) and the 14 other cheap mechanisms are null against `B_core`
  (-0.013 [-0.061, +0.036]). None of them adds measurable strength on top of `B_core`, so they
  stay available and off, as their own rows already left them (each is null, below the ±0.07
  resolution of a 400-game row).

## Leave-one-out rows of the chosen bundle (step 2)

`gcg-eval ablation leave-one-out --bundle search:standard:iterations_per_option=256,
min_iterations=640,max_iterations=2560,adaptive_horizon=true,capped_rollout_continuation=true,
tactical_solver=true --mechanism <field>=<off>`, 400 games, dev master 21500200 for every row,
so all rows share their deals and the challenger (the bundle) is the same agent in each. The row
`capped_rollout_continuation` is the family's placebo: the cap is never hit (0 capped rollouts
for both agents in every row), so its two agents play the same configuration. A positive paired
difference means the bundle beats the bundle without the mechanism, i.e. the mechanism helps.

| Row (mechanism removed) | wins/N (rate, Wilson 95%) | paired difference (Newcombe 95%) | discordant | iterations / applies per game (challenger vs opponent) | vs placebo: changed games; row-only / placebo-only wins; difference [95%] | evidence |
| --- | --- | --- | --- | --- | --- | --- |
| `capped_rollout_continuation` (placebo) | 207/400 (0.517, [0.469, 0.566]) | +0.035 [-0.032, +0.101] | 27/20 | 32778 vs 32388 / 598554 vs 579017 | — | `docs/port/eval/M15/rows/dev-loo-capped_rollout_continuation-400.json` |
| `adaptive_horizon` | 203/400 (0.507, [0.459, 0.556]) | +0.015 [-0.058, +0.088] | 30/27 | 31734 vs 32752 / 579343 vs 407670 | 388; 46 / 50; -0.0100 [-0.0580, +0.0380] | `docs/port/eval/M15/rows/dev-loo-adaptive_horizon-400.json` |
| `tactical_solver` | 215/400 (0.537, [0.488, 0.586]) | +0.075 [+0.009, +0.140] | 31/16 | 32808 vs 32150 / 580619 vs 476924 | 297; 27 / 19; +0.0200 [-0.0132, +0.0532] | `docs/port/eval/M15/rows/dev-loo-tactical_solver-400.json` |
| `engine_error_leaf` (the guard of the shipped presets) | 207/400 (0.517, [0.469, 0.566]) | +0.035 [-0.032, +0.101] | 27/20 | 32778 vs 32388 / 598554 vs 579017 | 0; 0 / 0; +0.0000 [+0.0000, +0.0000] | `docs/port/eval/M15/rows/dev-loo-engine_error_leaf-400.json` |

W-44's row needs the agents' own play/draw choice (PLAN §12.4), so it ran with `--choose-first
agents` on its own master (dev 21500250), the bundle being `B_core` with
`exclude_capped_play_draw`, with that protocol's placebo (`capped_rollout_continuation` removed):

| Row (mechanism removed) | wins/N (rate, Wilson 95%) | paired difference (Newcombe 95%) | discordant | iterations / applies per game (challenger vs opponent) | vs placebo: changed games; row-only / placebo-only wins; difference [95%] | evidence |
| --- | --- | --- | --- | --- | --- | --- |
| `capped_rollout_continuation` (placebo, agents choose) | 216/400 (0.540, [0.491, 0.588]) | +0.080 [+0.002, +0.156] | 40/24 | 32581 vs 31934 / 611051 vs 586936 | — | `docs/port/eval/M15/rows/dev-loo-agents-capped_rollout_continuation-400.json` |
| `exclude_capped_play_draw` (agents choose) | 216/400 (0.540, [0.491, 0.588]) | +0.080 [+0.002, +0.156] | 40/24 | 32581 vs 31934 / 611051 vs 582693 | 0; 0 / 0; +0.0000 [+0.0000, +0.0000] | `docs/port/eval/M15/rows/dev-loo-agents-exclude_capped_play_draw-400.json` |

The agents-protocol placebo's +0.080 is deal noise between two copies of one configuration with
different decision seeds; it shows how far a 400-game row can move with no change at all.

Reading and decisions (drop a mechanism whose removal clearly helps, paired difference ≤ -0.03):

- `tactical_solver`: +0.075 [+0.009, +0.140] (31/16), +0.020 [-0.013, +0.053] against the
  placebo; the solver costs 109,470 auxiliary applies per game (19% of the bundle's applies).
  Kept.
- `adaptive_horizon`: +0.015 [-0.058, +0.088], -0.010 [-0.058, +0.038] against the placebo:
  null inside the bundle at this budget, as its own high-budget row was (+0.015, P6-A), and
  never negative beyond noise; its confirmed rows are at the reference `standard` budget
  (+0.037 [+0.000, +0.075]) and in `B_core` (above). It costs about 38% of the bundle's applies
  (rollout applies 303,776 against 197,892 per game, and the solver's applies fall from 112,517
  to 43,905 without it). Kept: removal does not help (+0.015 > -0.03).
- `capped_rollout_continuation`: the placebo, +0.035 [-0.032, +0.101]: the noise of a row whose
  agents are the same configuration with different decision seeds. Kept as a zero-cost guard
  (it fires only when a rollout hits its 250-decision cap, W-15).
- No mechanism's removal helps, so nothing is dropped and the confirmed `B_core` row above is
  the bundle's measurement; no re-measure was needed.

## Items

### W-01 — `adaptive_horizon` in the bundle

- **Status:** adopted in both shipped presets (`crates/gcg-search/src/presets.rs`).
- **Ablation:** individual rows and confirmation in `docs/port/eval/weakness-status/P6-A.md`;
  bundle rows above (`B_core` +0.068 [+0.030, +0.105] confirmed); leave-one-out +0.015
  [-0.058, +0.088], 203/400 (0.507, [0.459, 0.556]), discordant 30/27, iterations / applies per
  game 31734 vs 32752 / 579343 vs 407670,
  `docs/port/eval/M15/rows/dev-loo-adaptive_horizon-400.json`.

### W-03 — `tactical_solver` in the bundle

- **Status:** adopted in both shipped presets.
- **Ablation:** individual and high-budget rows in `docs/port/eval/weakness-status/P6-B.md`;
  leave-one-out +0.075 [+0.009, +0.140], 215/400 (0.537, [0.488, 0.586]), discordant 31/16,
  iterations / applies per game 32808 vs 32150 / 580619 vs 476924,
  `docs/port/eval/M15/rows/dev-loo-tactical_solver-400.json`.

### W-15 — `capped_rollout_continuation` in the bundle

- **Status:** adopted in both shipped presets as a guard (never fires on the panel).
- **Ablation:** leave-one-out (the placebo) +0.035 [-0.032, +0.101], 207/400 (0.517, [0.469,
  0.566]), discordant 27/20, iterations / applies per game 32778 vs 32388 / 598554 vs 579017,
  `docs/port/eval/M15/rows/dev-loo-capped_rollout_continuation-400.json`.

### W-44 — `exclude_capped_play_draw` in the shipped presets

- **Status:** adopted in both shipped presets as a zero-cost pre-game guard (not in `B_core`'s
  rows: under forced play/draw no agent sees CHOOSE_FIRST, so it cannot act there).
- **Ablation:** leave-one-out with `--choose-first agents` (dev 21500250): 216/400 (0.540,
  [0.491, 0.588]), paired +0.080 [+0.002, +0.156], discordant 40/24, iterations / applies per
  game 32581 vs 31934 / 611051 vs 582693; identical to the protocol's placebo game for game (0 of
  400 games changed: no play/draw choice changed), so its measured effect is exactly zero, as in
  its P6-H individual row; the opponent's applies differ by 0.7% (how the capped pre-game
  simulations are scored). `docs/port/eval/M15/rows/dev-loo-agents-exclude_capped_play_draw-400.json`.

### W-143 — `engine_error_leaf` in the shipped presets

- **Status:** kept on in both shipped presets (the pre-M15 default, P6-O).
- **Ablation:** leave-one-out (dev 21500200): identical to the placebo game for game (0 of 400
  games changed: no simulated line hit an engine error), 207/400 (0.517, [0.469, 0.566]),
  paired +0.035 [-0.032, +0.101], discordant 27/20, iterations / applies per game 32778 vs 32388
  / 598554 vs 579017. `docs/port/eval/M15/rows/dev-loo-engine_error_leaf-400.json`. Its
  regression test now runs at the configuration the failing fuzz game was found with
  (`reference_mapping(Fast)`, `crates/gcg-search/tests/engine_error_leaf.rs`); the shipped
  `fast` preset finishes that game without reaching the cycle.

### The cheap null mechanisms of `B_full`

`ex_variants`, `typed_fingerprint`, `prune_worlds=4`, `branch_all_selects`,
`rest_prune_worlds`, `target_rollouts`, `real_idle_line`, `modal_lookahead`,
`burst_decline_list`, `exhaustive_polarity`, `transpositions`, `subtree_reuse`,
`macro_multi_pick`, `leaf_density`.

- **Status:** measured in the bundle and left off the presets: `B_full` (with `subtree_reuse`)
  -0.060 [-0.111, -0.009] against `B_core` on the same deals, `B_mid` (without it) -0.013
  [-0.061, +0.036]. Their individual rows are in the P6-D, P6-E, P6-F and P6-G status files;
  they stay available as flags.
- `exact_fight` was not measured in a bundle (its cost, above).

## Budgets

The bundle's budgets (`standard` 8x reference `strong`, `fast` 1.5x reference `standard`), the
budget curves, the control without the bundle and the per-game cost are in
`docs/port/eval/M15/README.md`.
