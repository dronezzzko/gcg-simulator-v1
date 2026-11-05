# P6-G evidence: the strengthened evaluation (S10)

Package P6-G (docs/dev/PLAN.md §12.2, §12.3 S10; W-34, W-35, W-36, W-37, W-39, W-142). The
status of every item, with the tables, is in `docs/dev/eval/weakness-status/P6-G.md`.

## Files

| File | Content |
| --- | --- |
| `fit-round1.json` | the fit of the 22 embedded models (`crates/gcg-search/src/mech/p6g/weights.txt`): per model the cross-validation table, out-of-fold and held-out log-loss of the model and of `ROLE_WEIGHTS` on determinised and true-state samples (W-142), the per-deck out-of-fold table (W-36) and the weights |
| `fit-deck-transfer.json` | six of those models with the leave-one-deck-out table (`--deck-transfer`, W-36) |
| `fit-transfer.json` | eight of those models with cross-validation folds dealt by pair (`--fold-by pair`) |
| `fit-standard-selfplay-test-fast.json` | the models fitted on `standard` self-play only (D1a, D1b) and scored on the `fast` self-play positions (D2) they never saw (`--test-positions`, W-34) |
| `fit-fast-selfplay.json` | four models fitted on `fast` self-play only (D2): the per-preset comparison of W-34 |
| `fit-round2-test.json` | round 2 of W-34: the round-1 fit of `determinized_fit` and the bundle, scored on the self-play positions of the round-1 bundle (D3, `--test-positions`) |
| `fit-round2.json` | round 2 of W-34: the refit of the same two models with D3 added |
| `fit-signed.json` | four models fitted with sign-constrained weights (`--signs`); analysis only, not embedded |
| `rows/dev-*-400.json` | individual ablation rows (`search:null-standard` + the mechanism vs `ref:standard`, the reference `standard` budget), dev master 20700100 for every row, so the rows share their deals; `dev-placebo-400.json` is the null configuration (`engine_error_leaf=false`) |
| `rows/hb-*-400.json` | high-budget rows: `search:standard` at twice its shipped budget (`iterations_per_option=256,min_iterations=640,max_iterations=2560`) with the mechanism against the same without it (`gcg-eval ablation leave-one-out`), dev master 20700200 |
| `rows/confirm-*-1600.json` | confirmation rows, confirm master 30700100 |
| `rows/dev-*_signed-400.json` | individual rows of a scratch build whose `weights.txt` replaced `refit_weights`, `determinized_fit`, `determinized_fit+phase_weights` and the bundle by the `fit-signed.json` models (same masters) |
| `seed-ledger.json` | every master this package used (the lead merges it into `docs/dev/eval/seed-ledger.json`) |
| `versus_placebo.py` | the individual rows against the placebo row, game by game |

The position datasets are not committed (about 260 MB of binary records). The commands below
regenerate them; recording and fitting are deterministic for any `--workers` (unit test
`tune_ext::tests::recording_and_fit_do_not_depend_on_the_worker_count`), and the SHA-256 table
below checks a regeneration. The fit command was rerun on the recorded files and reproduced
`crates/gcg-search/src/mech/p6g/weights.txt` and `fit-round1.json` bit for bit (`cmp`; the fit
is serial per model and fold, in sample order, with the portable `exp`/`ln`).

## Regeneration

Build with `cargo build --release -p gcg-eval`; `$D` is any scratch directory, `$L` the ledger
(`docs/dev/eval/P6-G/seed-ledger.json`). The agents of D1a, D1b and D2 (the shipped
`search:standard` and `search:fast`) use no P6-G flag, so their games do not depend on the
weights table; D3's agent uses the round-1 bundle of this table.

<!-- doc-check: skip regeneration recipe: needs $D and $L set, plays 10,200 self-play games over hours and rewrites the embedded weights.txt -->
```bash
# D1a: standard self-play on the panel's 15 tuning pairs (6,000 games, 104,334 positions)
target/release/gcg-eval tune-leaf --agent search:standard --games 6000 --seed 10700100 \
  --role tuning --ledger $L --workers 10 --record $D/d1a-standard-panel.bin
# D1b: standard self-play on the Python tuning decks (ai/tuning.py:89-93; 1,200 games, 18,380 positions)
target/release/gcg-eval tune-leaf --agent search:standard --decks tuning --pairs all \
  --games 1200 --seed 10700200 --role tuning --ledger $L --workers 10 \
  --record $D/d1b-standard-pytuning.bin
# D2: fast self-play on the panel's tuning pairs (3,000 games, 51,763 positions)
target/release/gcg-eval tune-leaf --agent search:fast --games 3000 --seed 10700300 \
  --role tuning --ledger $L --workers 10 --record $D/d2-fast-panel.bin

# The embedded models (writes weights.txt and fit-round1.json)
ALL=keyword_features+ability_count_features+hand_slots+burst_shield_probability
target/release/gcg-eval tune-leaf --positions $D/d1b-standard-pytuning.bin \
  --positions $D/d2-fast-panel.bin --positions $D/d1a-standard-panel.bin \
  --model refit_weights --model determinized_fit --model no_tempo_feature \
  --model keyword_features --model ability_count_features --model hand_slots \
  --model burst_shield_probability --model phase_weights \
  --model determinized_fit+keyword_features --model determinized_fit+ability_count_features \
  --model determinized_fit+hand_slots --model determinized_fit+burst_shield_probability \
  --model determinized_fit+phase_weights --model determinized_fit+$ALL --model $ALL \
  --model determinized_fit+$ALL+phase_weights --model $ALL+phase_weights \
  --model determinized_fit+ability_count_features+hand_slots+burst_shield_probability+phase_weights \
  --model determinized_fit+keyword_features+hand_slots+burst_shield_probability+phase_weights \
  --model determinized_fit+keyword_features+ability_count_features+burst_shield_probability+phase_weights \
  --model determinized_fit+keyword_features+ability_count_features+hand_slots+phase_weights \
  --model determinized_fit+$ALL+phase_weights+no_tempo_feature \
  --folds 5 --lambdas 0.03,0.1,0.3,1,3,10,30,100,300,1000 --seed 10700100 --workers 10 \
  --weights-out crates/gcg-search/src/mech/p6g/weights.txt \
  --out docs/dev/eval/P6-G/fit-round1.json
```

The SHA-256 of the recorded files (`shasum -a 256`), for checking a regeneration:

| File | SHA-256 |
| --- | --- |
| `d1a-standard-panel.bin` | `0e30daf3f4666ecc02857d3dff868fb18747400a740c9b1d4845c9d645b0377e` |
| `d1b-standard-pytuning.bin` | `eac37049a52c056210ced369e08fef486a2f397975250fc166a321606bc125c1` |
| `d2-fast-panel.bin` | `a1eecf7fc276d164ddfe2eac4d1df7d5b12ca18e94aba38e84bd9c9842d3a946` |
| `d3-bundle-panel.bin` | `e159f97089f5656fe2ecc3a900140dd1f044cf089ab2077ece2e89ab3ce2f2ed` |

After a new `weights.txt`, run `cargo test --release -p gcg-search --lib p6g`: the test
`w35_leaf_values_match_the_recorded_vectors` fails and names the regenerated
`leaf_vectors.txt` in the temporary directory; review and copy it next to `weights.txt`.

The analysis fits add, to the same positions, `--deck-transfer` (`fit-deck-transfer.json`),
`--fold-by pair --folds 6 --lambdas 0.1,1,10,100,1000,10000` (`fit-transfer.json`) or `--signs`
(`fit-signed.json`) with the models named in each report; `fit-standard-selfplay-test-fast.json`
fits on D1b and D1a with `--test-positions $D/d2-fast-panel.bin --lambdas
0.3,1,3,10,30,100,300,1000,3000`, and `fit-fast-selfplay.json` fits on D2 alone with `--seed
10700300`.

<!-- doc-check: skip regeneration recipe: needs $D and $L set and plays 2,100 self-play games over hours -->
```bash
# D3, round 2 of W-34 (needs the round-1 weights.txt): self-play of the round-1 bundle
target/release/gcg-eval tune-leaf --agent search:standard:determinized_fit=true,keyword_features=true,ability_count_features=true,hand_slots=true,burst_shield_probability=true,phase_weights=true \
  --games 2100 --seed 10700400 --role tuning --ledger $L --workers 10 --record $D/d3-bundle-panel.bin
```

`fit-round2-test.json` fits `determinized_fit` and the bundle (`--model determinized_fit --model
determinized_fit+$ALL+phase_weights`, the same options as the embedded fit) on D1b, D2, D1a with
`--test-positions $D/d3-bundle-panel.bin`; `fit-round2.json` adds `--positions
$D/d3-bundle-panel.bin` instead.

The rows: `docs/dev/eval/weakness-status/P6-G.md` ("Measurement protocol") gives the
commands; every report holds its exact challenger and opponent specs.
