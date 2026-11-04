# Final evaluation 2 (shipped presets of M15b)

`standard` = 8x the reference `strong` budget (1024/2560/10240) and `fast` = 1.5x the reference
`standard` budget (96/240/960), both with adaptive_horizon, capped_rollout_continuation,
tactical_solver, exclude_capped_play_draw, engine_error_leaf, kill_aware_targeting and
tree_lethal_select (`crates/gcg-search/src/presets.rs`, `docs/port/eval/M15b/README.md`).
Binaries built at 8c89b5e (`target/release`, plain release). Eval-role masters 90300001-90300006
(`docs/port/eval/seed-ledger.json`), forced play/draw (`--choose-first seat0`), `--workers 6`,
28 panel pairs x 400 games (13 held out) and the AI test decks (400 games). Run 2026-11-03 23:42
to 2026-11-04 21:44 UTC (log order E2, E1, E3 and E4 on the test decks, E4 and E3 on the panel).

```text
gcg-eval head-to-head search:fast ref:standard --decks panel --pairs all --games 11200 --seed 90300002 --role eval --workers 6 --choose-first seat0 --out e2-fast-vs-ref-standard.json
gcg-eval head-to-head search:standard ref:strong --decks panel --pairs all --games 11200 --seed 90300001 --role eval --workers 6 --choose-first seat0 --out e1-standard-vs-ref-strong.json
gcg-eval head-to-head search:standard random --decks tests --games 400 --seed 90300003 --role eval --workers 6 --choose-first seat0 --out e3-tests-standard-vs-random.json
gcg-eval head-to-head search:standard greedy --decks tests --games 400 --seed 90300005 --role eval --workers 6 --choose-first seat0 --out e4-tests-standard-vs-greedy.json
gcg-eval head-to-head search:standard greedy --decks panel --pairs all --games 11200 --seed 90300006 --role eval --workers 6 --choose-first seat0 --out e4-panel-standard-vs-greedy.json
gcg-eval head-to-head search:standard random --decks panel --pairs all --games 11200 --seed 90300004 --role eval --workers 6 --choose-first seat0 --out e3-panel-standard-vs-random.json
gcg-eval head-to-head-replay e3-panel-standard-vs-random.json --losses --out e3-losses
gcg-eval gates --e1 ... --e2 ... --e3-tests ... --e3-panel ... --e4-tests ... --e4-panel ... --losses-documented e3-losses/losses.json --out gates.json
```

(paths relative to this directory; the gates and the loss replays were written with the HEAD
binary, which re-played every loss exactly, see `docs/port/eval/notes.md`).

| Gate | Result | Brief bar | Plan A26 (also held-out pairs alone) |
| --- | --- | --- | --- |
| E1 `standard` vs ref `strong` | 6408/11200 = 0.5721 [0.5630, 0.5813]; held-out 2920/5200 = 0.5615 [0.5480, 0.5750]; no held-out pair UB < 0.5 (weakest pair 0.530) | pooled LB >= 0.55: pass | held-out LB 0.548: fail (by 0.002) |
| E2 `fast` vs ref `standard` | 6254/11200 = 0.5584 [0.5492, 0.5676]; held-out 2880/5200 = 0.5538 [0.5403, 0.5673]; no pair UB < 0.5 (weakest 0.505) | pass | pass |
| E3 `standard` vs random, test decks | 400/400 | pass | pass |
| E3 `standard` vs random, panel | 11186/11200, held-out 5192/5200; all 14 losses saved as verified replays (`e3-losses/`) | pass (losses documented) | pass |
| E4 `standard` vs greedy, test decks | 304/400 = 0.760 [0.7158, 0.7993] | LB > 0.764: fail | fail |
| E4 `standard` vs greedy, panel | 8774/11200 = 0.7834 [0.7757, 0.7909]; held-out 4048/5200 = 0.7785 [0.7670, 0.7895] | pass | pass |

`gates.json` (`gcg-eval gates` with the documented losses): E2 and E3 pass; E1 fails only its
held-out pooled check (0.548 against 0.55) and E4 only its test-deck check (0.7158 against
0.764). Overall: not passed under the plan's strict reading. Under the brief's own bars, every
gate passes except E4 on the AI test decks.

Also here: `fidelity-fast-standard.json` (W-18, master 90300007; verification not met on one
variant) and `audit/` (W-17's blinded audit).
