# P6-G weakness status: the strengthened evaluation (S10)

Package P6-G (docs/dev/PLAN.md §12.2, §12.3 S10): W-34, W-35, W-36, W-37, W-39, W-142, with the
flags `refit_weights`, `phase_weights`, `keyword_features`, `ability_count_features`,
`hand_slots`, `burst_shield_probability`, `no_tempo_feature`, `determinized_fit` and
`leaf_density`. Written by the P6-G implementer on 2026-11-02 in worktree branch
`worktree-wf_94545b98-29a-1`, from `8af9b0e` (M13). WEAKNESSES.md is not edited here (the lead
records the status lines from this file, §12.10). `presets.rs` is unchanged: no P6-G flag is on in
the shipped presets; the recommendation is at the end.

## Summary

The fitted evaluation predicts game outcomes much better than `ROLE_WEIGHTS` and does not play
better. On 287,542 determinised leaf-point samples from 10,200 Rust self-play games (tuning
seeds, tuning decks only) every refitted model beats `ROLE_WEIGHTS` out of fold, by 0.022 nats
(the role features refitted without an intercept) to 0.040 nats (the bundle: determinised fit, all four W-37
groups, phase-conditioned); the bundle fitted on the `standard` self-play alone gains 0.047
nats (13 game-clustered SEs) on the `fast` self-play positions it never saw, the embedded bundle
0.043 on the self-play of itself (round 2), and is better on six of the nine decks left out of the fit entirely (on two of
them by less than 0.001 nats) and worse on red-purple-control (by 0.032). At game level no
P6-G mechanism gains. Every 400-game individual row lies between 0.475 and 0.525 (the
placebo, the null configuration itself, scored 0.5375 on the same deals); the three
confirmations are 821/1600 = 0.513 [0.489, 0.538] for the bundle and 802/1600 = 0.501 for
`leaf_density` (both null) and 752/1600 = 0.470 [0.446, 0.494] for `hand_slots` (paired −0.060
[−0.097, −0.022], negative). With twice the shipped `standard` budget the bundle and
`leaf_density` are null (199/400 each) and every single model is below 0.5: 184 to 197 of 400 games.
A better log-loss on real positions is not a better ranking of the search's hypothetical
leaves, and the deeper search follows the evaluation's preferences more faithfully; neither
sign constraints on the weights nor a leaf spread matched to `ROLE_WEIGHTS`' recovered the rows.
The evaluation's predictive accuracy is not what limits this search.

## What was built

All nine P6-G flags of `MechanismFlags` are implemented (`p6g::PROVIDES`). With every P6-G flag
off, `Composed::leaf_value` calls `reference::leaf_value` unchanged, so the null configuration
replays the reference bit for bit (`tests/null_config.rs`, `tests/null_config_traces.rs`,
`tests/null_config_search_set.rs`: unchanged and passing, gate below).

| File | Content |
| --- | --- |
| `crates/gcg-search/src/mech/p6g/mod.rs` | `PROVIDES`, `active`, `validate` (a model flag combination without a fitted model is refused like an unimplemented flag), `leaf_score` and `leaf_value` (the reference's terminal values, `LEAF_FLOOR` and logistic; the logistic in the search's math, portable with `portable_math`), `LEAF_POINT` (W-39) and `api`, the code path the tuning tool shares with the leaf (W-142) |
| `mech/p6g/features.rs` | the 51-column superset vector of a position from the turn player's point of view: the reference's 17 role features (`gcg_ai::role_features`, the constant first term named `intercept`), the hand slots (12), the keyword amounts (14), the ability counts (4) and the Burst probabilities (4) |
| `mech/p6g/model.rs` | `ModelSpec` (which flags name which model, which columns it reads), `Model` (phase bins and per-bin weights; `z = w · x` summed in column order in f64), the table format, its parser and writer, the embedded table |
| `mech/p6g/weights.txt` | the 22 fitted models (IEEE-754 bits), written by `gcg-eval tune-leaf --weights-out` (`docs/dev/eval/P6-G/README.md`) |
| `mech/p6g/leaf_vectors.txt` | the recorded leaf values of the W-35/W-137 test |
| `mech/p6g/tests.rs` | 7 unit tests (below) |
| `crates/gcg-eval/src/tune_ext.rs` | `gcg-eval tune-leaf`: the leaf-point recorder, the positions file, the fitter (L2 logistic regression by Newton's method with a Cholesky solve, CV by game or by pair, lambda by CV, the held-out pair check, the per-deck and leave-one-deck-out tables, phase models, `--signs`, `--test-positions`), the report and the weights table; 5 unit tests |
| `docs/dev/eval/P6-G/` | fit reports, ablation rows, `seed-ledger.json`, `README.md` (regeneration commands), `versus_placebo.py` (deleted, at commit 83a06c8) |

Wiring outside the package's directories (flagged to the lead, interface notes of the report):

- `crates/gcg-search/src/mech/mod.rs` (the p6g lines): `pub(crate) mod p6g;`, the
  `Composed::leaf_value` override (P6-G when a P6-G flag is on, otherwise the reference),
  `p6g::PROVIDES` in `available`, and `p6g::validate(flags)` at the end of `validate`.
- `crates/gcg-search/src/lib.rs` (one line): `pub use mech::p6g::api as p6g;`, the public path
  through which `gcg-eval tune-leaf` calls the leaf's own feature and density functions (W-142
  asks for one shared code path; `mech` is private). `api` takes the true state only in
  `true_state_features` (the harness's reference-convention recording) and an `Observation`
  otherwise.
- `crates/gcg-eval/src/lib.rs` (`pub mod tune_ext;`) and `crates/gcg-eval/src/cli.rs` (the
  `tune-leaf` subcommand entry with the shared run options, its `Command` variant, its parse arm
  and its dispatch arm).

Composition notes for the lead's bundle: P6-H's pre-game simulations score through
`Mechanisms::leaf_value`, so they use the P6-G model when its flags are on. P6-F's look-aheads
call `gcg_ai::score` with the configuration's weights and keep `ROLE_WEIGHTS` (their code);
`p6g::leaf_score(flags, cx)` is the crate-private function they would call to use the model.
P6-A's horizon changes keep the leaf at `LEAF_POINT` (a turn start) as long as the horizon is in
turns; a P6-A leaf constant (`generalized_leaf`) would compose with `p6g::leaf_score` the way
`p6g::leaf_value` composes with `LEAF_FLOOR`.

## Data and fit

- **Recording point (W-39).** `p6g::LEAF_POINT`: the first pending decision of a turn, after its
  start, draw and resource phases: where `play_until_turn` stops a line at the horizon turn. The
  recorder records there (turn 1 on, one position per turn); the search's leaves are there
  (unit test). The reference's tuning recorded at the first MAIN decision instead.
- **Recordings** (tuning-role masters; ledger `docs/dev/eval/P6-G/seed-ledger.json`; tuning
  decks only): D1a `search:standard` (the shipped preset, reference `strong` budgets) on the
  panel's 15 tuning pairs, 6,000 games, master 10700100, 104,334 positions; D1b
  `search:standard` on the Python tuning decks (`ai/tuning.py:89-93`, `--decks tuning`), 1,200
  games, master 10700200, 18,380 positions; D2 `search:fast` on the 15 tuning pairs, 3,000
  games, master 10700300, 51,763 positions. No game was drawn or unfinished. Per position: the
  true-state vector (turn player's density) and one determinised vector per observer, label =
  the turn player won.
- **Fit** (`fit-round1.json`): positions from turn 3 (the first turn a two-turn leaf reaches),
  the panel pair purple-blue-swarm : red-purple-control held out (10,306 positions, in D1a and
  D2), 143,771 training positions (287,542 determinised samples), 5 folds by game (fold seed
  10700100), lambda from 0.03 to 1000 by CV, Newton from zero (at most 30 steps, tolerance
  1e-9), Cholesky solve, sums in sample order, portable `exp`/`ln`: bit-identical for any
  worker count (unit test) and platform. A phase model's bins are the training positions' turn
  terciles: turns 3-6, 7-12, 13 and later.

## Log-loss evidence

### Log-loss of every embedded model (`fit-round1.json`)

Out of fold: pooled over the 5 folds by game at the chosen lambda; held out: the pair
purple-blue-swarm : red-purple-control (never fitted); "det" are the determinised samples (the
leaf's distribution), "true" the true-state samples. The last column lists the decks whose
out-of-fold log-loss is worse than `ROLE_WEIGHTS`' on the same samples by more than one
game-clustered standard error.

| Model (flags) | mode | features | lambda | OOF det | OOF true | held-out det | held-out true | decks worse than ROLE_WEIGHTS by > 1 SE |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| `ROLE_WEIGHTS` (reference) | — | 17 | — | 0.5578 | 0.5500 | 0.5857 | 0.5792 | — |
| `refit_weights` | true-state | 17 | 1 | 0.5338 | 0.5282 | 0.5768 | 0.5713 | none |
| `determinized_fit` | determinized | 17 | 3 | 0.5318 | 0.5302 | 0.5740 | 0.5716 | tuning-zeon |
| `no_tempo_feature` | true-state | 16 | 3 | 0.5356 | 0.5302 | 0.5784 | 0.5731 | tuning-zeon |
| `keyword_features` | true-state | 31 | 3 | 0.5306 | 0.5253 | 0.5693 | 0.5641 | none |
| `ability_count_features` | true-state | 21 | 1 | 0.5338 | 0.5282 | 0.5771 | 0.5716 | none |
| `hand_slots` | true-state | 25 | 1 | 0.5327 | 0.5269 | 0.5781 | 0.5728 | none |
| `burst_shield_probability` | true-state | 21 | 0.1 | 0.5327 | 0.5267 | 0.5748 | 0.5694 | none |
| `phase_weights` | true-state | 17 | 0.03 | 0.5297 | 0.5235 | 0.5720 | 0.5660 | none |
| `determinized_fit+keyword_features` | determinized | 31 | 10 | 0.5287 | 0.5272 | 0.5670 | 0.5648 | none |
| `determinized_fit+ability_count_features` | determinized | 21 | 3 | 0.5318 | 0.5301 | 0.5743 | 0.5719 | tuning-zeon |
| `determinized_fit+hand_slots` | determinized | 25 | 3 | 0.5306 | 0.5290 | 0.5756 | 0.5738 | tuning-zeon |
| `determinized_fit+burst_shield_probability` | determinized | 21 | 0.3 | 0.5306 | 0.5287 | 0.5719 | 0.5694 | tuning-zeon |
| `determinized_fit+phase_weights` | determinized | 17 | 0.03 | 0.5275 | 0.5256 | 0.5686 | 0.5660 | tuning-zeon |
| `determinized_fit+keyword_features+ability_count_features+hand_slots+burst_shield_probability` | determinized | 47 | 1 | 0.5264 | 0.5246 | 0.5671 | 0.5654 | none |
| `keyword_features+ability_count_features+hand_slots+burst_shield_probability` | true-state | 47 | 0.3 | 0.5285 | 0.5226 | 0.5694 | 0.5645 | none |
| `determinized_fit+keyword_features+ability_count_features+hand_slots+burst_shield_probability+phase_weights` | determinized | 47 | 0.1 | 0.5176 | 0.5149 | 0.5680 | 0.5652 | none |
| `keyword_features+ability_count_features+hand_slots+burst_shield_probability+phase_weights` | true-state | 47 | 0.03 | 0.5205 | 0.5123 | 0.5763 | 0.5662 | none |
| `determinized_fit+ability_count_features+hand_slots+burst_shield_probability+phase_weights` | determinized | 33 | 0.03 | 0.5207 | 0.5178 | 0.5720 | 0.5688 | none |
| `determinized_fit+keyword_features+hand_slots+burst_shield_probability+phase_weights` | determinized | 43 | 0.1 | 0.5179 | 0.5151 | 0.5676 | 0.5649 | none |
| `determinized_fit+keyword_features+ability_count_features+burst_shield_probability+phase_weights` | determinized | 39 | 0.1 | 0.5190 | 0.5165 | 0.5678 | 0.5644 | none |
| `determinized_fit+keyword_features+ability_count_features+hand_slots+phase_weights` | determinized | 43 | 10 | 0.5216 | 0.5193 | 0.5666 | 0.5651 | none |
| `determinized_fit+keyword_features+ability_count_features+hand_slots+burst_shield_probability+phase_weights+no_tempo_feature` | determinized | 46 | 0.1 | 0.5178 | 0.5151 | 0.5678 | 0.5654 | none |

### Transfer to decks left out of the fit (`fit-deck-transfer.json`, W-36)

Each deck's games scored by the model refitted without them (at the model's lambda), against
`ROLE_WEIGHTS` on the same determinised samples: log-loss (difference ± game-clustered SE);
"worse" marks a difference above one SE.

| Deck (held out) | ROLE_WEIGHTS | `refit_weights` | `determinized_fit` | `determinized_fit+keyword_features` | `determinized_fit+phase_weights` | `determinized_fit+keyword_features+ability_count_features+hand_slots+burst_shield_probability` | `determinized_fit+keyword_features+ability_count_features+hand_slots+burst_shield_probability+phase_weights` |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| tuning-zeon | 0.5324 | 0.5373 (+0.0049 ± 0.0043) (worse) | 0.5408 (+0.0084 ± 0.0047) (worse) | 0.5379 (+0.0054 ± 0.0044) (worse) | 0.5403 (+0.0078 ± 0.0049) (worse) | 0.5392 (+0.0067 ± 0.0047) (worse) | 0.5368 (+0.0044 ± 0.0047) |
| tuning-wing | 0.5272 | 0.5274 (+0.0002 ± 0.0044) | 0.5282 (+0.0010 ± 0.0046) | 0.5270 (-0.0002 ± 0.0042) | 0.5247 (-0.0025 ± 0.0046) | 0.5284 (+0.0012 ± 0.0043) | 0.5309 (+0.0037 ± 0.0046) |
| tuning-tekkadan | 0.5506 | 0.5513 (+0.0006 ± 0.0054) | 0.5530 (+0.0023 ± 0.0057) | 0.5490 (-0.0016 ± 0.0050) | 0.5531 (+0.0025 ± 0.0060) | 0.5513 (+0.0006 ± 0.0054) | 0.5505 (-0.0001 ± 0.0056) |
| blue-white-federation | 0.5842 | 0.5522 (-0.0320 ± 0.0023) | 0.5536 (-0.0306 ± 0.0023) | 0.5644 (-0.0199 ± 0.0021) | 0.5545 (-0.0297 ± 0.0025) | 0.5770 (-0.0072 ± 0.0023) | 0.5834 (-0.0009 ± 0.0028) |
| purple-white-tekkadan | 0.5359 | 0.5267 (-0.0092 ± 0.0024) | 0.5239 (-0.0120 ± 0.0025) | 0.5293 (-0.0066 ± 0.0027) | 0.5214 (-0.0145 ± 0.0026) | 0.5305 (-0.0053 ± 0.0029) | 0.5223 (-0.0136 ± 0.0030) |
| red-green-zeon | 0.5460 | 0.5305 (-0.0154 ± 0.0029) | 0.5284 (-0.0175 ± 0.0029) | 0.5266 (-0.0194 ± 0.0031) | 0.5231 (-0.0229 ± 0.0030) | 0.5271 (-0.0189 ± 0.0031) | 0.5192 (-0.0268 ± 0.0034) |
| blue-green-ramp | 0.5501 | 0.5389 (-0.0112 ± 0.0025) | 0.5327 (-0.0174 ± 0.0026) | 0.5311 (-0.0190 ± 0.0028) | 0.5302 (-0.0199 ± 0.0029) | 0.5322 (-0.0179 ± 0.0029) | 0.5212 (-0.0289 ± 0.0033) |
| purple-blue-swarm | 0.5846 | 0.5686 (-0.0160 ± 0.0021) | 0.5600 (-0.0247 ± 0.0024) | 0.5586 (-0.0260 ± 0.0025) | 0.5570 (-0.0276 ± 0.0022) | 0.5594 (-0.0253 ± 0.0025) | 0.5398 (-0.0449 ± 0.0029) |
| red-purple-control | 0.5664 | 0.6188 (+0.0524 ± 0.0044) (worse) | 0.6034 (+0.0370 ± 0.0041) (worse) | 0.5999 (+0.0335 ± 0.0041) (worse) | 0.6003 (+0.0340 ± 0.0041) (worse) | 0.6158 (+0.0494 ± 0.0045) (worse) | 0.5988 (+0.0324 ± 0.0040) (worse) |

With the cross-validation folds dealt by pair instead (`fit-transfer.json`, 6 folds, every game
of a pair in one fold), the out-of-fold gains over `ROLE_WEIGHTS` keep their order:

| Model (flags) | mode | features | lambda | OOF det | OOF true | held-out det | held-out true | decks worse than ROLE_WEIGHTS by > 1 SE |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| `ROLE_WEIGHTS` (reference) | — | 17 | — | 0.5578 | 0.5500 | 0.5857 | 0.5792 | — |
| `refit_weights` | true-state | 17 | 1 | 0.5370 | 0.5316 | 0.5768 | 0.5713 | tuning-zeon |
| `determinized_fit` | determinized | 17 | 10 | 0.5344 | 0.5329 | 0.5739 | 0.5716 | tuning-zeon |
| `determinized_fit+keyword_features` | determinized | 31 | 10 | 0.5320 | 0.5306 | 0.5670 | 0.5648 | tuning-zeon |
| `determinized_fit+phase_weights` | determinized | 17 | 0.1 | 0.5307 | 0.5290 | 0.5685 | 0.5659 | tuning-zeon |
| `determinized_fit+burst_shield_probability` | determinized | 21 | 10 | 0.5337 | 0.5320 | 0.5717 | 0.5693 | tuning-zeon |
| `determinized_fit+keyword_features+ability_count_features+hand_slots+burst_shield_probability` | determinized | 47 | 10 | 0.5311 | 0.5294 | 0.5669 | 0.5652 | tuning-zeon |
| `determinized_fit+keyword_features+ability_count_features+hand_slots+burst_shield_probability+phase_weights` | determinized | 47 | 0.1 | 0.5216 | 0.5190 | 0.5680 | 0.5652 | tuning-zeon |
| `determinized_fit+burst_shield_probability+phase_weights` | determinized | 21 | 0.1 | 0.5254 | 0.5228 | 0.5708 | 0.5670 | tuning-zeon |

### Positions of another preset's self-play (`fit-standard-selfplay-test-fast.json`, W-34)

The models fitted on the `standard` self-play only (D1a, D1b; lambda grid 0.3 to 3000), scored
on every `fast` self-play position (D2) from turn 3: log-loss on determinised and true-state
samples, the difference to `ROLE_WEIGHTS` on the determinised samples and its game-clustered SE.

```text
Test file d2-fast-panel.bin: ROLE_WEIGHTS det 0.5663 true 0.5601
  refit_weights: det 0.5413 true 0.5358 diff -0.0250 (SE 0.0028)
  determinized_fit: det 0.5373 true 0.5358 diff -0.0290 (SE 0.0028)
  no_tempo_feature: det 0.5434 true 0.5379 diff -0.0229 (SE 0.0029)
  keyword_features: det 0.5368 true 0.5316 diff -0.0295 (SE 0.0030)
  ability_count_features: det 0.5412 true 0.5357 diff -0.0250 (SE 0.0028)
  hand_slots: det 0.5403 true 0.5347 diff -0.0260 (SE 0.0028)
  burst_shield_probability: det 0.5391 true 0.5333 diff -0.0272 (SE 0.0027)
  phase_weights: det 0.5358 true 0.5293 diff -0.0305 (SE 0.0029)
  determinized_fit+keyword_features: det 0.5329 true 0.5315 diff -0.0334 (SE 0.0032)
  determinized_fit+ability_count_features: det 0.5373 true 0.5357 diff -0.0290 (SE 0.0029)
  determinized_fit+hand_slots: det 0.5363 true 0.5349 diff -0.0300 (SE 0.0029)
  determinized_fit+burst_shield_probability: det 0.5353 true 0.5335 diff -0.0310 (SE 0.0028)
  determinized_fit+phase_weights: det 0.5316 true 0.5295 diff -0.0347 (SE 0.0030)
  determinized_fit+keyword_features+ability_count_features+hand_slots+burst_shield_probability: det 0.5299 true 0.5283 diff -0.0364 (SE 0.0032)
  determinized_fit+keyword_features+ability_count_features+hand_slots+burst_shield_probability+phase_weights: det 0.5190 true 0.5165 diff -0.0473 (SE 0.0036)
  keyword_features+ability_count_features+hand_slots+burst_shield_probability: det 0.5336 true 0.5280 diff -0.0327 (SE 0.0031)
```

### Sign-constrained weights (`fit-signed.json`, analysis only)

`--signs`: the turn player's assets at least 0, the other player's assets and the danger term at
most 0, the intercept free (active set; a phase model's bins fitted separately). Not embedded;
measured at game level with a scratch build that replaced these four models.

| Model (flags) | mode | features | lambda | OOF det | OOF true | held-out det | held-out true | decks worse than ROLE_WEIGHTS by > 1 SE |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| `ROLE_WEIGHTS` (reference) | — | 17 | — | 0.5578 | 0.5500 | 0.5857 | 0.5792 | — |
| `refit_weights` | true-state | 17 | 1 | 0.5348 | 0.5292 | 0.5753 | 0.5698 | none |
| `determinized_fit` | determinized | 17 | 3 | 0.5328 | 0.5312 | 0.5725 | 0.5701 | tuning-zeon |
| `determinized_fit+phase_weights` | determinized | 17 | 0.03 | 0.5284 | 0.5265 | 0.5666 | 0.5640 | tuning-zeon |
| `determinized_fit+keyword_features+ability_count_features+hand_slots+burst_shield_probability+phase_weights` | determinized | 47 | 3 | 0.5249 | 0.5231 | 0.5637 | 0.5617 | none |

## Tests

| Test | What it shows | Item |
| --- | --- | --- |
| `mech::p6g::tests::w142_recorded_features_are_the_leaf_features` | on 100 searched positions (`crates/gcg-ai/tests/vectors/search/decisions.ndjson`) and both observers, `p6g::determinized_features(obs, seed)` (the recorder) equals `p6g::leaf_features` of the world the search builds with that seed (`determinize_into` of the information-set state), bit for bit (200 vectors) | W-142 |
| `mech::p6g::tests::w39_search_leaves_are_at_the_recording_point` | six traced searches (over 1,000 leaves): every leaf is a finished game, a capped rollout (their count equals `rollouts_capped`) or a state where `LEAF_POINT.holds`, the constant the recorder uses | W-39 |
| `mech::p6g::tests::w39_no_tempo_feature` | no strengthened feature is named `tempo`; the constant is `intercept`; `no_tempo_feature` drops it | W-39 |
| `mech::p6g::tests::w37_every_keyword_has_a_feature` | every `Kw` variant has its own feature pair (exhaustive `match`), named after it | W-37 |
| `mech::p6g::tests::w37_every_hand_card_type_has_a_slot` | Unit, Pilot, Command and Base have slots 0-3; Resource and the tokens are an error, never the Base slot | W-37 |
| `mech::p6g::tests::w35_leaf_values_match_the_recorded_vectors` | the leaf values of all 22 embedded models on 20 positions (portable math, leaf density) equal `leaf_vectors.txt` bit for bit; only IEEE-754 basic operations and the `libm` crate's `exp` are involved, so the bits are the same on every IEEE-754 platform (run here on macOS arm64 only; the Linux x86-64 run belongs to the CI matrix of W-137) | W-35, W-137 |
| `mech::p6g::tests::model_keys_and_tables_round_trip` | model keys, columns, phase bins and the table format | — |
| `tune_ext::tests::recording_and_fit_do_not_depend_on_the_worker_count` | 12 recorded games: one position per turn from turn 1, the same positions and a bit-identical phase-model fit and report with 1 and 3 workers | W-39, W-40 |
| `tune_ext::tests::synthetic_weights_are_recovered` | a two-bin phase fit recovers known per-bin weights within 0.12 on 20,000 synthetic samples, and repeats bit for bit | W-35 |
| `tune_ext::tests::{cholesky_solves_a_known_system, models_are_named_by_their_flags, positions_round_trip}` | the solver, `--model` parsing and phase edges, the positions file | — |

None of P6-G's items is scenario-shaped (PLAN §12.1 lists the 17 scenario-shaped items), so the
package has no `tests/scenarios/p6g_*.rs`.

## Measurement protocol

- **Individual rows** (PLAN §12.4, the plan's row): `gcg-eval ablation individual --mechanism
  <flags> --reference standard --games 400 --seed 20700100 --role dev --workers 10
  --choose-first seat0`: the challenger is `search:null-standard` (the strengthened search at the
  reference `standard` budget, every mechanism off) plus the mechanism, the opponent
  `ref:standard`; 400 BO1 games in mirrored blocks of four, stratified round-robin over the 15
  tuning pairs of `docs/dev/eval/panel.json`, forced play/draw (A24). Every row uses the same
  master, so the rows share their deals, and the placebo row (`engine_error_leaf=false`, the null
  configuration, which replays the reference) measures the noise of a row on those deals.
- **High-budget rows** (the task's row (b)): `gcg-eval ablation leave-one-out --bundle
  search:standard:iterations_per_option=256,min_iterations=640,max_iterations=2560,<flags>
  --mechanism <flags off> --games 400 --seed 20700200 --role dev --workers 10 --choose-first
  seat0`: the shipped `standard` (`engine_error_leaf` on) at twice its budget with the mechanism
  against the same without it.
- **Confirmation** (promising rows only: Newcombe lower bound above 0 or paired difference at
  least +0.03): 1,600 games, confirm master 30700100, run once (no high-budget row was promising).
- Each row reports wins/N with the Wilson 95% interval, the paired difference with its Newcombe
  95% interval (W-42), McNemar's discordant counts, and iterations and applies per game; the
  evaluation adds no engine applies (`applies_aux` 0), so applies per game differ only through
  the games themselves.

## Ablation rows

### Individual rows (reference `standard` budget, dev master 20700100)

| Row | wins/N (rate, Wilson 95%) | paired difference (Newcombe 95%) | discordant | iterations / applies per game (challenger vs opponent) | vs placebo: row-only / placebo-only wins, difference [95%] | evidence |
| --- | --- | --- | --- | --- | --- | --- |
| `ability_count_features` | 191/400 (0.477, [0.429, 0.526]) | -0.045 [-0.116, +0.027] | 23/32 | 9599 vs 9770 / 117280 vs 117838 | 41 / 65, -0.0600 [-0.1101, -0.0099] | `docs/dev/eval/P6-G/rows/dev-ability_count_features-400.json` |
| the bundle with `value_scale=0.78` | 208/400 (0.520, [0.471, 0.569]) | +0.040 [-0.032, +0.111] | 31/23 | 9723 vs 9980 / 116811 vs 120492 | 49 / 56, -0.0175 [-0.0677, +0.0327] | `docs/dev/eval/P6-G/rows/dev-bundle_vs078-400.json` |
| `burst_shield_probability` | 193/400 (0.482, [0.434, 0.531]) | -0.035 [-0.116, +0.047] | 32/39 | 9449 vs 9786 / 114163 vs 117872 | 47 / 69, -0.0550 [-0.1075, -0.0025] | `docs/dev/eval/P6-G/rows/dev-burst_shield_probability-400.json` |
| `determinized_fit` with `value_scale=0.78` (spread matched to `ROLE_WEIGHTS`) | 190/400 (0.475, [0.426, 0.524]) | -0.050 [-0.114, +0.015] | 17/27 | 9489 vs 9867 / 115779 vs 119574 | 42 / 67, -0.0625 [-0.1133, -0.0117] | `docs/dev/eval/P6-G/rows/dev-det_vs078-400.json` |
| `determinized_fit` | 194/400 (0.485, [0.436, 0.534]) | -0.030 [-0.106, +0.047] | 28/34 | 9583 vs 9709 / 116655 vs 116307 | 41 / 62, -0.0525 [-0.1020, -0.0030] | `docs/dev/eval/P6-G/rows/dev-determinized_fit-400.json` |
| `determinized_fit_signed` | 193/400 (0.482, [0.434, 0.531]) | -0.035 [-0.108, +0.038] | 25/32 | 9692 vs 9838 / 118242 vs 119575 | 41 / 63, -0.0550 [-0.1047, -0.0053] | `docs/dev/eval/P6-G/rows/dev-determinized_fit_signed-400.json` |
| `hand_slots` | 207/400 (0.517, [0.469, 0.566]) | +0.035 [-0.041, +0.110] | 34/27 | 9821 vs 9999 / 119604 vs 121352 | 54 / 62, -0.0200 [-0.0727, +0.0327] | `docs/dev/eval/P6-G/rows/dev-hand_slots-400.json` |
| `keyword_features` | 194/400 (0.485, [0.436, 0.534]) | -0.030 [-0.100, +0.040] | 23/29 | 9492 vs 9685 / 115311 vs 117471 | 42 / 63, -0.0525 [-0.1024, -0.0026] | `docs/dev/eval/P6-G/rows/dev-keyword_features-400.json` |
| `leaf_density` | 207/400 (0.517, [0.469, 0.566]) | +0.035 [-0.037, +0.107] | 31/24 | 9806 vs 9854 / 119302 vs 119835 | 41 / 49, -0.0200 [-0.0664, +0.0264] | `docs/dev/eval/P6-G/rows/dev-leaf_density-400.json` |
| `no_tempo_feature` | 196/400 (0.490, [0.441, 0.539]) | -0.020 [-0.097, +0.058] | 30/34 | 9676 vs 9832 / 116858 vs 117928 | 37 / 56, -0.0475 [-0.0945, -0.0005] | `docs/dev/eval/P6-G/rows/dev-no_tempo_feature-400.json` |
| P6-G bundle (`determinized_fit`, the four W-37 groups, `phase_weights`) | 210/400 (0.525, [0.476, 0.574]) | +0.050 [-0.028, +0.127] | 37/27 | 9821 vs 9826 / 117551 vs 118436 | 55 / 60, -0.0125 [-0.0650, +0.0400] | `docs/dev/eval/P6-G/rows/dev-p6g_bundle-400.json` |
| `p6g_bundle_signed` | 193/400 (0.482, [0.434, 0.531]) | -0.035 [-0.102, +0.033] | 21/28 | 9502 vs 9871 / 116019 vs 119661 | 42 / 64, -0.0550 [-0.1052, -0.0048] | `docs/dev/eval/P6-G/rows/dev-p6g_bundle_signed-400.json` |
| `phase_weights` | 200/400 (0.500, [0.451, 0.549]) | +0.000 [-0.073, +0.073] | 28/28 | 9519 vs 9802 / 115476 vs 117949 | 54 / 69, -0.0375 [-0.0917, +0.0167] | `docs/dev/eval/P6-G/rows/dev-phase_weights-400.json` |
| placebo (null configuration) | 215/400 (0.537, [0.488, 0.586]) | +0.075 [+0.000, +0.148] | 37/22 | 9759 vs 9761 / 118729 vs 119581 | — | `docs/dev/eval/P6-G/rows/dev-placebo-400.json` |
| `refit_weights` | 193/400 (0.482, [0.434, 0.531]) | -0.035 [-0.104, +0.035] | 22/29 | 9424 vs 9864 / 114184 vs 118881 | 42 / 64, -0.0550 [-0.1052, -0.0048] | `docs/dev/eval/P6-G/rows/dev-refit_weights-400.json` |
| `refit_weights_signed` | 198/400 (0.495, [0.446, 0.544]) | -0.010 [-0.085, +0.065] | 29/31 | 9553 vs 9771 / 115397 vs 117411 | 40 / 57, -0.0425 [-0.0906, +0.0056] | `docs/dev/eval/P6-G/rows/dev-refit_weights_signed-400.json` |

The placebo (the null configuration, which replays the reference) scored 215/400 on these deals,
+1.5 standard errors from its true rate of 0.5; with almost every game changed by any mechanism
(359 to 388 of 400 games differ from the placebo's), the "vs placebo" column measures that luck as
much as the mechanism, so the rows are read against 0.5. The `_signed` rows ran a scratch build
whose table replaced `refit_weights`, `determinized_fit`, `determinized_fit+phase_weights` and the
bundle by the sign-constrained fits; the `vs078` rows add `value_scale=0.78` (the leaf spread of
the refit matched to `ROLE_WEIGHTS`'; standard deviation of the turn player's log-odds 2.11
against 2.72 on 17,706 positions of D1a).

### High-budget rows (`search:standard` at twice its budget with the mechanism vs without, dev master 20700200)

| Row | wins/N (rate, Wilson 95%) | paired difference (Newcombe 95%) | discordant | iterations / applies per game (challenger vs opponent) | vs placebo: row-only / placebo-only wins, difference [95%] | evidence |
| --- | --- | --- | --- | --- | --- | --- |
| `ability_count_features` | 187/400 (0.468, [0.419, 0.516]) | -0.065 [-0.129, +0.000] | 16/29 | 30928 vs 31935 / 357280 vs 365338 | — | `docs/dev/eval/P6-G/rows/hb-ability_count_features-400.json` |
| `burst_shield_probability` | 194/400 (0.485, [0.436, 0.534]) | -0.030 [-0.101, +0.042] | 24/30 | 31238 vs 32214 / 361097 vs 369503 | — | `docs/dev/eval/P6-G/rows/hb-burst_shield_probability-400.json` |
| `determinized_fit` | 189/400 (0.472, [0.424, 0.521]) | -0.055 [-0.125, +0.016] | 21/32 | 31105 vs 32030 / 359938 vs 365589 | — | `docs/dev/eval/P6-G/rows/hb-determinized_fit-400.json` |
| `hand_slots` | 185/400 (0.463, [0.414, 0.511]) | -0.075 [-0.145, -0.003] | 20/35 | 30941 vs 32169 / 357656 vs 368028 | — | `docs/dev/eval/P6-G/rows/hb-hand_slots-400.json` |
| `keyword_features` | 184/400 (0.460, [0.412, 0.509]) | -0.080 [-0.159, +0.000] | 26/42 | 30756 vs 32297 / 354033 vs 367746 | — | `docs/dev/eval/P6-G/rows/hb-keyword_features-400.json` |
| `leaf_density` | 199/400 (0.497, [0.449, 0.546]) | -0.005 [-0.074, +0.064] | 25/26 | 31701 vs 31675 / 366898 vs 363690 | — | `docs/dev/eval/P6-G/rows/hb-leaf_density-400.json` |
| `no_tempo_feature` | 197/400 (0.492, [0.444, 0.541]) | -0.015 [-0.081, +0.052] | 22/25 | 30520 vs 31875 / 353029 vs 363165 | — | `docs/dev/eval/P6-G/rows/hb-no_tempo_feature-400.json` |
| P6-G bundle (`determinized_fit`, the four W-37 groups, `phase_weights`) | 199/400 (0.497, [0.449, 0.546]) | -0.005 [-0.078, +0.068] | 28/29 | 31345 vs 31873 / 359030 vs 367985 | — | `docs/dev/eval/P6-G/rows/hb-p6g_bundle-400.json` |
| `phase_weights` | 188/400 (0.470, [0.422, 0.519]) | -0.060 [-0.128, +0.009] | 19/31 | 30994 vs 32181 / 358286 vs 369310 | — | `docs/dev/eval/P6-G/rows/hb-phase_weights-400.json` |
| `refit_weights` | 188/400 (0.470, [0.422, 0.519]) | -0.060 [-0.135, +0.017] | 25/37 | 31388 vs 32113 / 363551 vs 368678 | — | `docs/dev/eval/P6-G/rows/hb-refit_weights-400.json` |

### Confirmations (1,600 games, confirm master 30700100; the promising individual rows)

| Row | wins/N (rate, Wilson 95%) | paired difference (Newcombe 95%) | discordant | iterations / applies per game (challenger vs opponent) | vs placebo: row-only / placebo-only wins, difference [95%] | evidence |
| --- | --- | --- | --- | --- | --- | --- |
| `hand_slots` | 752/1600 (0.470, [0.446, 0.494]) | -0.060 [-0.097, -0.022] | 95/143 | 9625 vs 9858 / 116832 vs 118321 | — | `docs/dev/eval/P6-G/rows/confirm-hand_slots-1600.json` |
| `leaf_density` | 802/1600 (0.501, [0.477, 0.526]) | +0.003 [-0.033, +0.038] | 105/103 | 9830 vs 9779 / 118649 vs 117841 | — | `docs/dev/eval/P6-G/rows/confirm-leaf_density-1600.json` |
| P6-G bundle (`determinized_fit`, the four W-37 groups, `phase_weights`) | 821/1600 (0.513, [0.489, 0.538]) | +0.026 [-0.011, +0.063] | 125/104 | 9725 vs 9850 / 118012 vs 119270 | — | `docs/dev/eval/P6-G/rows/confirm-p6g_bundle-1600.json` |

Iterations per game are lower for the fitted models than for their opponents in every row but
the bundle's individual one, by up to 5% (for example 30,756 against 32,297 with
`keyword_features` at the high budget), while `leaf_density` and the placebo match theirs: with the same per-decision budgets, the fitted models' searches stop
early (the settled check of `ai/mcts.py:320-328`) or their games are shorter more often.

The individual rows with a paired difference of at least +0.03 were the bundle (+0.050),
`hand_slots` (+0.035) and `leaf_density` (+0.035); each was confirmed once. The value-scale
variant of the bundle (+0.040) is not a mechanism configuration and was not confirmed. No
high-budget row was promising.

Leave-one-out rows need the full strengthened bundle, which the lead assembles (§12.9 step 4);
they are not run here. Every leave-one-out model of the P6-G bundle is embedded (the bundle
without each of its six flags, and with `no_tempo_feature`). `refit_weights` is implied by every
other model flag, so its leave-one-out row inside a bundle with another model flag is the bundle
itself.

## Items

### W-34 — Evaluation fitted on pre-change search games; refit and per-preset fits deferred (`refit_weights`)

- **Status:** resolved (implemented, fitted, measured); the refit is measured negative at game
  level, so it is not kept (off in the presets).
- **Resolution as implemented:** the Rust tuning tool `gcg-eval tune-leaf` records Rust self-play
  of the strengthened search (`search:standard`, the shipped preset) at the strengthened leaf
  point and fits by the deterministic, worker-count-independent Newton method of W-40 with CV by
  game, lambda by CV and a held-out pair. `refit_weights` scores the leaves with the 17 role
  features refitted on true-state recordings (the reference tuning's convention at the new
  leaf point). The reference keeps `ROLE_WEIGHTS` and `TUNED_WEIGHTS` (`gcg_ai`, unchanged).
  Per preset: one fit serves both presets, because a fit on `fast` self-play alone gains only 0.0014
  to 0.0035 nats on `fast` positions over the `standard` fit (below); the model has no preset
  dimension (`ModelSpec`), recorded here as the decision.
- **Evidence (log-loss, `fit-round1.json`):** `ROLE_WEIGHTS` out of fold 0.5578 (determinised
  samples) / 0.5500 (true-state); `refit_weights` 0.5338 / 0.5282 (lambda 1); held-out pair
  0.5857 → 0.5768. On the `fast` self-play positions the fit never saw
  (`fit-standard-selfplay-test-fast.json`): `ROLE_WEIGHTS` 0.5663, `refit_weights` 0.5413
  (difference −0.0250, game-clustered SE 0.0028). Per preset (`fit-fast-selfplay.json`, a fit
  on D2 alone): out of fold on the `fast` positions, `determinized_fit` 0.5338 and the bundle
  0.5176, against 0.5373 and 0.5190 for the `standard` fit scored on every D2 position out of
  sample (the `fast` fit's out-of-fold set leaves out its held-out pair). Rounds (W-34's iteration, at most three): round 2 recorded 2,100 games of
  `search:standard` with the round-1 bundle (D3, master 10700400, 36,469 positions). The
  round-1 models score 0.5360 (`determinized_fit`) and 0.5189 (the bundle) on them out of
  sample, against 0.5619 for `ROLE_WEIGHTS` (`fit-round2-test.json`); refitting with D3
  added leaves the bundle at 0.5181 out of fold (round 1: 0.5176) and its held-out gain over
  `ROLE_WEIGHTS` at 0.0155 nats (round 1: 0.0177; `fit-round2.json`): the held-out log-loss
  stopped improving after round 1, so the iteration stopped there and the embedded models
  are round 1's.
- **Ablation:** individual 193/400 (dev, −0.035 [−0.104, +0.035]); high budget 188/400
  (−0.060 [−0.135, +0.017]); the sign-constrained refit 198/400 (individual). Rows in the
  tables above; evidence `rows/dev-refit_weights-400.json`, `rows/hb-refit_weights-400.json`,
  `rows/dev-refit_weights_signed-400.json`.

### W-35 — The evaluation is linear (`phase_weights`)

- **Status:** resolved (implemented, fitted, measured); measured negative at the high budget,
  not kept.
- **Resolution as implemented:** a phase-conditioned linear model: one weight vector per turn
  tercile of the training positions (turns 3-6, 7-12, 13 and later), fitted jointly as a shared
  vector plus a per-bin deviation (both penalised), so the bins share the role features. Sums in
  column order in f64; the logistic uses the search's math, the portable `exp` with
  `portable_math` (W-137). Deviation from the plan text: the bins are by turn only, not by turn
  and level (the turn player's level is close to a function of the turn at a turn start). The
  exact threat solver that replaces the approximate lethal terms is P6-B's (W-38).
- **Evidence:** out of fold on determinised samples, `phase_weights` 0.5297 vs the single linear refit
  0.5338 (both true-state fits);
  `determinized_fit+phase_weights` 0.5275 vs `determinized_fit` 0.5318; on the unseen `fast`
  positions 0.5316 vs 0.5373. Unit tests: the recorded leaf vectors (bit for bit, portable math)
  and the synthetic two-bin recovery.
- **Ablation:** individual 200/400 (0.000 [−0.073, +0.073]); high budget 188/400 (−0.060
  [−0.128, +0.009]).

### W-36 — The evaluation was trained on three tuning decks and transfers weakly

- **Status:** resolved (the tool, the tables and the measurement); the per-deck gate holds for
  the example decks and fails for one tuning deck (red-purple-control); the held-out panel decks
  were not used (below).
- **Resolution as implemented:** the fit trains on the tuning half of the deck panel (the 15
  tuning pairs: six decks, the three example decks among them) plus the Python tuning decks,
  reports the per-deck out-of-fold table with game-clustered standard errors, and with
  `--deck-transfer` refits without each deck's games and scores them (leave-one-deck-out), the
  per-deck regression gate.
- **Evidence (`fit-deck-transfer.json`, leave-one-deck-out, determinised samples):** every
  example deck is better than `ROLE_WEIGHTS` under every refit (blue-white-federation −0.032 to
  −0.001, purple-white-tekkadan −0.015 to −0.005, red-green-zeon −0.027 to −0.015 nats);
  blue-green-ramp and purple-blue-swarm are better by 0.011 to 0.045; tuning-wing and
  tuning-tekkadan are within one SE; tuning-zeon (in `ROLE_WEIGHTS`'s own training set) is worse
  by 0.004 to 0.008 (one to two SEs); red-purple-control, the panel's control deck, is worse by
  0.032 to 0.052 nats (eight to twelve SEs) under every refit: a deck whose games the other decks do
  not predict. The held-out panel decks (blue-red-tempo, green-white-link) were not scored: the
  protocol keeps them out of every tuning and screening run (`--role tuning|dev|confirm` refuses
  them), and the final evaluation (§12.6) is the lead's.
- **Ablation:** as W-34 (the refit is fitted on the panel's tuning decks).

### W-37 — The evaluation ignores most keywords, card text and effects

- **Status:** resolved (implemented, fitted, measured); none of the four groups is kept.
- **Resolution as implemented (`features.rs`; each group one flag, fitted before use):**
  - `keyword_features`: per `Kw` variant and per player, the summed amounts on the battlefield (a
    keyword without an amount counts 1); `kw_feature` maps every variant by an exhaustive
    `match`, so a new keyword does not compile until it has a feature.
  - `ability_count_features`: per player, the Units with at least one activated and with at
    least one triggered ability among the view's active abilities (IR node kinds `Activated`,
    `Triggered`).
  - `hand_slots`: per player, the hand counts of Unit, Pilot, Command and Base cards by an
    exhaustive `match` on `CardType` (a Resource or a token in hand is an engine error, never the
    reference's catch-all Base slot), and the castable Units and other cards (level and cost at
    most the player's level; the other player one Resource later); the group replaces the role
    block's four hand differences.
  - `burst_shield_probability`: per player, the probability that the next Shield is a Burst card
    (the Burst density of that owner's unseen cards; 0 without Shields) and that at least one
    Shield is, `1 - (1 - d)^shields`.
- **Evidence:** unit tests `w37_every_keyword_has_a_feature`, `w37_every_hand_card_type_has_a_slot`.
  Log-loss out of fold (true-state fits, against `refit_weights` 0.5338): keywords 0.5306,
  abilities 0.5338, hand slots 0.5327, Burst 0.5327; all four with the determinised fit 0.5264,
  and with phases 0.5176 (the best model; held-out pair 0.5680 against 0.5857 for
  `ROLE_WEIGHTS`).
- **Ablation:** individual: keywords 194/400 (−0.030), abilities 191/400 (−0.045), hand slots
  207/400 (+0.035; confirmation 752/1600 = 0.470 [0.446, 0.494], paired −0.060
  [−0.097, −0.022], negative), Burst 193/400 (−0.035); high budget: keywords
  184/400 (−0.080 [−0.159, +0.000]), abilities 187/400 (−0.065 [−0.129, +0.000]), hand slots 185/400 (−0.075 [−0.145, −0.003]), Burst 194/400 (−0.030).

### W-39 — The negative tempo weight is an artifact of when positions are recorded (`no_tempo_feature`)

- **Status:** resolved (unit tests; `no_tempo_feature` implemented and measured, not kept).
- **Resolution as implemented:** no strengthened feature is a tempo (turn) indicator: the role
  block's constant is named `intercept`, and `no_tempo_feature` fits without it. The recorder and
  the search share one constant, `p6g::LEAF_POINT` (the first pending decision of a turn), and
  every report names it (`leaf_point`). The reference keeps `TUNED_WEIGHTS` for
  `baseline-standard` bit for bit (`gcg_ai`, unchanged).
- **Evidence:** unit tests `w39_no_tempo_feature` and `w39_search_leaves_are_at_the_recording_point`
  (asserted from the same constant); the fitted intercept at the leaf point is −0.47 (the 17
  refitted role features) against −0.75 in `ROLE_WEIGHTS`; without it, out of fold 0.5356
  against 0.5338.
- **Ablation:** individual 196/400 (−0.020 [−0.097, +0.058]); high budget 197/400 (−0.015 [−0.081, +0.052]).

### W-142 — True-state fit, determinised leaves; Burst density computed once (`determinized_fit`, `leaf_density`)

- **Status:** resolved (unit test, both recording modes' log-loss side by side, both flags
  measured); neither is enabled: `determinized_fit` measured negative,
  `leaf_density` null (it stays available).
- **Resolution as implemented:** the recorder stores, at every position, the true-state vector
  and one determinised vector per observer through `p6g::determinized_features`: the world
  `determinize(information set, observer, seed)` scored by `p6g::leaf_features`, the function the
  leaf calls with `leaf_density` (one code path). `determinized_fit` fits the determinised
  samples. `leaf_density` recomputes the Burst density at every leaf from the searcher's known
  cards in the leaf world (`burst_density(leaf, searcher)`) instead of the root's. The reference
  keeps both behaviours.
- **Evidence:** unit test `w142_recorded_features_are_the_leaf_features` (100 positions, both
  observers, bit for bit). Log-loss on determinised samples (the leaf's distribution): the
  determinised fit 0.5318 against the true-state fit 0.5338 out of fold, 0.5740 against 0.5768
  held out, 0.5373 against 0.5413 on the unseen `fast` positions; on true-state samples the
  true-state fit is better (0.5282 against 0.5302), so the two distributions do differ, and the
  determinised fit is the better one for the leaf.
- **Ablation:** `determinized_fit` individual 194/400 (−0.030), high budget 189/400 (−0.055
  [−0.125, +0.016]), sign-constrained 193/400; `leaf_density` individual 207/400 (+0.035
  [−0.037, +0.107]; confirmation 802/1600 = 0.501 [0.477, 0.526], paired +0.003
  [−0.033, +0.038], null), high budget 199/400 (−0.005 [−0.074, +0.064]).

## Recommendation for the shipped presets

| Flag | Individual (400) | High budget (400) | Confirmation (1,600) | Measured effect | Kept |
| --- | --- | --- | --- | --- | --- |
| `refit_weights` | 193 (−0.035) | 188 (−0.060) | — | negative | no |
| `determinized_fit` | 194 (−0.030) | 189 (−0.055) | — | negative | no |
| `no_tempo_feature` | 196 (−0.020) | 197 (−0.015) | — | negative | no |
| `keyword_features` | 194 (−0.030) | 184 (−0.080) | — | negative | no |
| `ability_count_features` | 191 (−0.045) | 187 (−0.065) | — | negative | no |
| `hand_slots` | 207 (+0.035) | 185 (−0.075) | 752 (−0.060 [−0.097, −0.022]) | negative | no |
| `burst_shield_probability` | 193 (−0.035) | 194 (−0.030) | — | negative | no |
| `phase_weights` | 200 (0.000) | 188 (−0.060) | — | negative | no |
| `leaf_density` | 207 (+0.035) | 199 (−0.005) | 802 (+0.003 [−0.033, +0.038]) | null | available, off |
| bundle (`determinized_fit` + the four groups + `phase_weights`) | 210 (+0.050) | 199 (−0.005) | 821 (+0.026 [−0.011, +0.063]) | null | available, off |

Wins of the challenger, with the paired difference in parentheses.

Enable no P6-G flag in `standard` or in `fast` (`presets.rs` unchanged). None of the nine flags
has a measured gain: the individual rows are all within the noise of a 400-game row (the
placebo, the null configuration itself, scored 215/400 on the same deals); two of the three
confirmations are null (the bundle 821/1600 = 0.513 [0.489, 0.538], `leaf_density` 802/1600 =
0.501) and the third is negative (`hand_slots` 752/1600, paired −0.060 [−0.097, −0.022]); and
with twice the shipped `standard` budget the bundle and `leaf_density` are null (199/400) while
every single-model refit is below 0.5 (184 to 197 of 400 games), the most consistent signal of
the package, pointing the wrong way. `leaf_density` and the bundle measured non-negative and may
stay available to the lead's bundle; the single models measured negative. The flags stay
implemented, fitted and selectable for the lead's bundle and its leave-one-out rows (every
leave-one-out model of the bundle is embedded), and a refit after the wave-2 search changes
(P6-A horizon, P6-B solver, P6-C beliefs) is one `tune-leaf` command
(`docs/dev/eval/P6-G/README.md`).

What the measurements say about the evaluation as a lever: its predictive accuracy is not what
limits the search at these budgets. A leaf that is 0.02 to 0.04 nats better on the leaf
distribution (4% to 7% of `ROLE_WEIGHTS`' log-loss) moved no row, and the plain refits lose more
with the deeper search. Two mechanisms fit that pattern. The weights of an observational fit
are confounded where the search compares sibling moves (refitted against `ROLE_WEIGHTS`:
`linked` −0.21 against +0.22, `bursts` 0.34 to 0.38 against 0.85, `level` 0.02 to 0.03 against
0.31), and a
deeper search follows the evaluation's preferences more faithfully; constraining the signs
(`--signs`, which holds `linked` at 0) did not recover the rows (198/400, 193/400 and 193/400),
so the confounding is not only in the signs. And the search's constants (exploration, prior
weight and temperature, rollout temperature) were tuned with `ROLE_WEIGHTS`; a leaf spread
matched to `ROLE_WEIGHTS`' (`value_scale=0.78`) did not help either (190/400 and 208/400). The
next step for a stronger evaluation is a decision-aware fit (comparison training against a
deeper search's choices, or labels from playouts of the searched leaves; both would reuse the
recorder's shared feature code), not more features or more self-play outcomes.

## Gate

Run on the final tree of this package (commands from the task; results as observed):

| Command | Result |
| --- | --- |
| `cargo fmt --all --check` | clean |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | clean (no warning) |
| `cargo test --workspace --release` | 128 targets, 1,149 tests passed, 0 failed, 0 ignored (W-127's evidence check and the W-35 vectors among them) |
| `cargo test --release -p gcg-search --features slow` | 19 targets, 221 tests passed, 0 failed (run after the last code edit) |
| `target/release/gcg-diff corpus --tier 1 --require-all` | 3,804/3,804 traces passed (100%: 17,601 decisions, 1,396 agent decisions) |
| `scripts/compare_python_vs_rust.sh equivalence --matches 2 --seeds 1 --formats bo1 --pairs red-green-zeon:blue-white-federation --workers 1 --decision-logs off` | 3/3 identical (fast, standard, strong) |
| refit of the embedded models (README command) against `weights.txt` | weights.txt bit-identical (cmp), fit-round1.json identical |

The null configuration's tests (`tests/null_config.rs`, `tests/null_config_traces.rs`, and with
`slow` `tests/null_config_search_set.rs`) are unchanged and pass: with every P6-G flag off the
leaf is `reference::leaf_value`.
