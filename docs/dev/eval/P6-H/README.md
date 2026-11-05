# P6-H evidence (W-44, W-45)

The raw reports behind `docs/dev/eval/weakness-status/P6-H.md`. Every row ran
`target/release/gcg-eval` built (release profile, `--offline`) from the mechanism code of
commit 86e481d (`crates/gcg-search/src/mech/p6h/`; later P6-H commits change tests and
documents only), with `--workers 10` (4 for the two regret runs; outputs are identical for any
worker count). The seeds are dev- and confirm-role masters (PLAN A14) recorded in
`docs/dev/eval/P6-H/seed-ledger.json`, to be merged into the shared ledger (the default
`--ledger`, `seed-ledger.json` next to `docs/dev/eval/panel.json`). The files are not yet in the
evidence manifest (`gcg-eval evidence add` writes the manifest of the evidence directory, outside
this package's files); the commands below are the exact ones.

Each individual row (PLAN §12.4) is paired with a **null control**: the same command with the
mechanism's flag set to `false`, which is the reference configuration (`search:null-standard`)
under the same agent name. Both rows deal the same games and draw the same per-decision seeds,
so a game in which the mechanism changes no decision is replayed exactly; `pair_rows.py`
isolates the games the mechanism changed and gives the mechanism's effect on the row with the
deal noise removed.

| File | Command |
| --- | --- |
| `docs/dev/eval/P6-H/w45-paired-mulligan-individual-dev.json` | `gcg-eval ablation individual --mechanism paired_mulligan=true --reference standard --games 400 --role dev --seed 20800045 --ledger docs/dev/eval/P6-H/seed-ledger.json --workers 10 --out docs/dev/eval/P6-H/w45-paired-mulligan-individual-dev.json` |
| `docs/dev/eval/P6-H/w45-null-control-dev.json` | the same with `--mechanism paired_mulligan=false --out docs/dev/eval/P6-H/w45-null-control-dev.json` |
| `docs/dev/eval/P6-H/w45-paired-vs-control-dev.json` | `python3 docs/port/eval/P6-H/pair_rows.py docs/dev/eval/P6-H/w45-paired-mulligan-individual-dev.json docs/dev/eval/P6-H/w45-null-control-dev.json` |
| `docs/dev/eval/P6-H/w45-paired-mulligan-individual-confirm.json` | `gcg-eval ablation individual --mechanism paired_mulligan=true --reference standard --games 1600 --role confirm --seed 30800045 --ledger docs/dev/eval/P6-H/seed-ledger.json --workers 10 --out docs/dev/eval/P6-H/w45-paired-mulligan-individual-confirm.json` |
| `docs/dev/eval/P6-H/w45-null-control-confirm.json` | the same with `--mechanism paired_mulligan=false --out docs/dev/eval/P6-H/w45-null-control-confirm.json` |
| `docs/dev/eval/P6-H/w45-paired-vs-control-confirm.json` | `python3 docs/port/eval/P6-H/pair_rows.py docs/dev/eval/P6-H/w45-paired-mulligan-individual-confirm.json docs/dev/eval/P6-H/w45-null-control-confirm.json` |
| `docs/dev/eval/P6-H/w45-regret-reference-dev.json` | `gcg-eval regret ref:standard search:null-strong:paired_mulligan=true,mulligan_samples=768 --recorder ref:standard --decisions redraw --per-game 2 --games 60 --worlds 64 --temperature 0.5 --max-decisions 3000 --role dev --seed 20800245 --ledger docs/dev/eval/P6-H/seed-ledger.json --workers 4 --out docs/dev/eval/P6-H/w45-regret-reference-dev.json` |
| `docs/dev/eval/P6-H/w45-regret-paired-mulligan-dev.json` | the same with the candidate `search:null-standard:paired_mulligan=true` and `--out docs/dev/eval/P6-H/w45-regret-paired-mulligan-dev.json` |
| `docs/dev/eval/P6-H/w44-exclude-capped-play-draw-individual-dev.json` | `gcg-eval ablation individual --mechanism exclude_capped_play_draw=true --reference standard --games 400 --role dev --seed 20800044 --choose-first agents --ledger docs/dev/eval/P6-H/seed-ledger.json --workers 10 --out docs/dev/eval/P6-H/w44-exclude-capped-play-draw-individual-dev.json` |
| `docs/dev/eval/P6-H/w44-null-control-dev.json` | the same with `--mechanism exclude_capped_play_draw=false --out docs/dev/eval/P6-H/w44-null-control-dev.json` |
| `docs/dev/eval/P6-H/w44-paired-vs-control-dev.json` | `python3 docs/port/eval/P6-H/pair_rows.py docs/dev/eval/P6-H/w44-exclude-capped-play-draw-individual-dev.json docs/dev/eval/P6-H/w44-null-control-dev.json` |
| `docs/dev/eval/P6-H/w44-e2e-head-to-head-dev.json` | `gcg-eval head-to-head search:null-standard:exclude_capped_play_draw=true ref:standard --decks panel --pairs tuning --games 60 --role dev --seed 20800144 --choose-first agents --ledger docs/dev/eval/P6-H/seed-ledger.json --workers 10 --out docs/dev/eval/P6-H/w44-e2e-head-to-head-dev.json --decision-trace TRACE.ndjson` |
| `docs/dev/eval/P6-H/w44-e2e-trace-summary-dev.json` | `python3 docs/port/eval/P6-H/trace_summary.py docs/dev/eval/P6-H/w44-e2e-head-to-head-dev.json TRACE.ndjson 24` (the 2.9 MB trace itself is not committed; the command above regenerates it byte for byte) |

`pair_rows.py` and `trace_summary.py` were this package's evidence helpers (standard-library
Python, outside the crates, no timing; deleted, at commit 83a06c8).
