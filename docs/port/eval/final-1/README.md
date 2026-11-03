# Final evaluation 1 (shipped presets of commit 82b0b34)

`standard` = 8x the reference `strong` budget with adaptive_horizon, capped_rollout_continuation,
tactical_solver, exclude_capped_play_draw and engine_error_leaf; `fast` = 1.5x the reference
`standard` budget with the same mechanisms. Binaries built at 34b3107. Eval-role masters
90200001-90200006 (docs/port/eval/seed-ledger.json), forced play/draw (`--choose-first seat0`),
28 panel pairs x 400 games (13 held-out) and the AI test decks (400 games).

| Gate | Result | Brief bar | Plan A26 (also held-out pairs alone) |
| --- | --- | --- | --- |
| E1 `standard` vs ref `strong` | 6268/11200 = 0.5596 [0.5504, 0.5688]; held-out 2885/5200 = 0.5548 [0.5413, 0.5683]; no held-out pair UB < 0.5 | pooled LB >= 0.55: pass (by 0.0004) | held-out LB 0.5413: fail |
| E2 `fast` vs ref `standard` | 6171/11200 = 0.5510 [0.5418, 0.5602]; held-out 0.5544; no pair UB < 0.5 | pass | pass |
| E3 `standard` vs random, test decks | 400/400 | pass | pass |
| E3 `standard` vs random, panel | 11173/11200 (27 losses) | losses must be documented with replays | not documented: fail |
| E4 `standard` vs greedy, test decks | 307/400 = 0.7675 [0.7237, 0.8062] | LB > 0.764: fail | fail |
| E4 `standard` vs greedy, panel | 8655/11200 = 0.7728 [0.7649, 0.7804]; held-out 4011/5200 = 0.7713 [0.7597, 0.7826] | pooled LB > 0.764: pass (by 0.0009) | held-out LB 0.7597: fail |

`gcg-eval gates --check` (gates.json) reports overall fail under the plan's strict reading.
The results are too close to the bars to accept as they are; the follow-up (a strength
round on tuning-pair evidence only, then a second evaluation on fresh eval seeds) is recorded in
docs/port/progress.md.
