# M15b: two mechanisms added after the first final evaluation

The first final evaluation (docs/dev/eval/final-1) was too close to the bars (E1 pooled lower
bound 0.5504 against 0.55; E4 panel 0.7649 against 0.764). Two mechanisms of the gap-closing
wave had positive confirmation rows: `kill_aware_targeting` (in `B_core` against `B_core`,
confirm 30271100: 830/1600, +0.037 [+0.001, +0.074]) and `tree_lethal_select` (alone, confirm
30271800: 843/1600, +0.054 [+0.018, +0.089]). M15b adds both to the shipped mechanisms of both
presets; budgets and every other setting stay M15's. The decision used tuning-pair evidence
only (tuning, dev and confirm seeds); no eval or review seed, no held-out pair and no result of
the first final evaluation's games entered it.

Configurations: `configs/m15b-standard-8x.json`, `configs/m15b-fast-1.5x.json` (M15's
`m15-standard-8x.json` / `m15-fast-1.5x.json` with the two flags on). Binary built at b8483dc.
Rows, 15 tuning pairs of docs/dev/eval/panel.json, forced play/draw (`--choose-first seat0`),
6 workers, seed ledger `seed-ledger.json`:

| Row | Command (abbreviated) | Result |
| --- | --- | --- |
| M15b `standard` vs M15 `standard` | `head-to-head search:configs/m15b-standard-8x.json search:../M15/configs/m15-standard-8x.json --decks panel --pairs tuning --games 1620 --seed 30500001 --role confirm` | 831/1620 = 0.513 [0.4886, 0.5372]; no pair with an upper bound below 0.5 |
| M15b `standard` vs `ref:strong` | `... ref:strong --games 1620 --seed 10500001 --role tuning` | 950/1620 = 0.5864 [0.5623, 0.6102]; no pair below 0.5 |
| M15b `standard` vs greedy | `... greedy --games 420 --seed 20500001 --role dev` | 338/420 = 0.8048 [0.7642, 0.8398] |
| M15 `standard` vs greedy, same deals | `search:../M15/configs/m15-standard-8x.json greedy --games 420 --seed 20500001 --role dev` | 328/420 = 0.7810 [0.7389, 0.8179] |

Every component is non-negative, so both mechanisms were adopted (PLAN §12.4). The cost is
re-measured on an idle machine before the second final evaluation (docs/dev/BENCHMARKS.md).
Row reports with every game: `rows/`.
