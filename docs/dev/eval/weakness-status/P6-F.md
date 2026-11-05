# P6-F weakness status: the strengthened rollout policy (S9)

Package P6-F (docs/dev/PLAN.md §12.2, §12.3 S9): W-23, W-24, W-25, W-26, W-27, W-28, W-29,
W-30, W-31, W-163. Written by the P6-F implementer on 2026-11-02 in worktree branch
`worktree-wf_b2d4a964-3bd-2`, from `8ae4767` (M11). WEAKNESSES.md itself is not edited here (the
lead records the status lines from this file, §12.10). Default-on in the shipped presets is the
lead's later decision; this file only says which mechanisms measured non-negative.

## What was built

All ten P6-F flags of `MechanismFlags` are implemented (`p6f::PROVIDES`). With every P6-F flag
off, `Composed` calls the reference `priors` and `rollout_choice` unchanged, so the null
configuration still replays the reference bit for bit (`tests/null_config.rs`,
`tests/null_config_traces.rs`, unchanged and passing).

| File | Content |
| --- | --- |
| `crates/gcg-search/src/mech/p6f/mod.rs` | `PROVIDES`, `active`, the per-decision state (`DecisionState`: auxiliary applies, the evaluation's Burst density per viewer fixed at first use), `AUX_APPLY_CAP` = 8192 applies per search decision, the Gumbel `pick` with the search's math (with `Math::Platform` it is `gcg_ai::pick` bit for bit), `rollout_choice` (every rollout move) and `priors` (every new tree node) over the strengthened scores |
| `mech/p6f/scorer.rs` | `Scoring::scores`: the reference scores (`gcg_ai::option_scores`) with each flag replacing the scorer of the decisions it covers, then the calibrated offsets; the YES_NO, Burst and end-phase scorers; `LOOKAHEAD_SCALE` 4.0 per unit of evaluation log-odds, `LOOKAHEAD_CAP` 8.0 |
| `mech/p6f/lookahead.rs` | the one-ply look-ahead: the option applied on a clone, the game followed with the reference policy's best option (no random draws) until the effect has resolved (`Until::Effect`, ≤ 64 applies) or through the viewer's next turn (`Until::ThroughTurn`, ≤ 160 applies), scored by the search's evaluation (`score / value_scale`, ±50 for a finished game); every apply counted |
| `mech/p6f/calibrated.rs`, `fitted.rs` | the calibrated policy's six offsets (W-23) and the opponent model (W-25) |
| `mech/p6f/fit.rs` | conditional-logit maximum likelihood by Newton's method (IRLS) in f64: fixed-order sums, portable `exp`/`log`, ridge 1e-3, Cholesky solve |
| `mech/p6f/fit_selfplay.rs` (feature `slow`) | the self-play behind the fit and the regeneration check of `docs/dev/eval/P6-F/policy-fit.json` |
| `mech/p6f/bursts.rs` | the review of a Burst or a "you may" body (cost, self-harm, optional replacement, custom step), the W-163 list, `play_outlasts_turn` (W-28) |
| `mech/p6f/polarity.rs` | the exhaustive polarity (W-30) |
| `mech/p6f/targets.rs` | the SELECT scorer with the exhaustive polarity and kill-aware damage targets (W-26, W-30) |
| `mech/p6f/order.rs` | the ORDER_TRIGGER scorer (W-29) |
| `mech/p6f/fight.rs` | exact fights on clones in the attack and block scorers (W-31) |
| `mech/p6f/tests.rs` | 12 unit tests (below; `fit.rs` and `lookahead.rs` hold 3 more) |
| `crates/gcg-search/tests/scenarios/main.rs`, `p6f_support.rs`, `p6f_w24_modal.rs`, `p6f_w28_end_phase.rs`, `p6f_w29_order.rs` | the scenario test target `scenarios` and P6-F's three scenario tests |
| `docs/dev/eval/P6-F/` | `policy-fit.json` (the fitted weights file), `synthetic_cards.py` and its four IR fragments (`synthetic_*_fragment.json`), the ablation reports `rows/*.json`, `seed-ledger.json` |

Wiring outside `mech/p6f/` (flagged to the lead):

- `crates/gcg-search/src/mech/mod.rs` (the p6f lines): `pub(crate) mod p6f;`, the `p6f`
  `DecisionState` field of `Composed` and `Composed::aux_applies`, the `priors` and
  `rollout_choice` overrides of `Composed` (P6-F only when a P6-F flag is on), `available` reads
  `p6f::PROVIDES`, and the `expect(dead_code)` on `RolloutCx::player` is gone (P6-F reads it).
  If P6-D's `prior_worlds` also overrides `priors`, the lead composes the two.
- `crates/gcg-search/src/mcts.rs` (`search_with_effort`, 4 lines): the look-ahead applies are
  added to `SearchStats::effort.applies_aux`, so ablation rows report them in "applies per game"
  (PLAN §12.3: aux work counted). No other line of the core loop changed.
- `crates/gcg-search/tests/scenarios/main.rs` is the scenario target every package shares: the
  lead unions the `mod` lines of the packages.

## Ablation rows (PLAN §12.4)

Individual rows with `gcg-eval ablation individual --reference standard` (challenger
`search:null-standard:<flag>=true`, opponent `ref:standard`, the reference `standard` budget
that the `fast` preset starts from), 400 games per row in mirrored blocks of four, stratified
round-robin over the 15 tuning pairs of `docs/dev/eval/panel.json`, forced play/draw
(`--choose-first seat0`, A24), dev-role master 20600100 (A14) for every row, so all rows share
their deals. The command of each row:

<!-- doc-check: skip a template with placeholders (flag, ledger) for 400-game rows that take hours; the rows are recorded below -->
```bash
target/release/gcg-eval ablation individual --mechanism <flag>=true --reference standard \
  --games 400 --seed 20600100 --role dev --ledger <ledger> --workers 6 --choose-first seat0 \
  --out docs/dev/eval/P6-F/rows/dev-<flag>-400.json
```

The placebo row runs the same command with `--mechanism engine_error_leaf=false` (a no-op: the
challenger is the null configuration, which replays the reference, so the row measures only the
noise of a row whose two agents draw different decision seeds). The combined rows `p6f_all` and
`p6f_kept` switch on several flags at once (below). Evidence: `docs/dev/eval/P6-F/rows/`, one
report per row named role-row-games, with every game's outcome; its `summary` key holds the
numbers of the table.

| Row | wins/N (rate, Wilson 95%) | paired difference (Newcombe 95%) | discordant | iterations / applies per game (challenger vs opponent) | vs placebo: changed games, row-only / placebo-only wins, difference [95%] | evidence |
| --- | --- | --- | --- | --- | --- | --- |
| `placebo` | 199/400 (0.497, [0.449, 0.546]) | -0.005 [-0.076, +0.066] | 26/27 | 9772 vs 9744 / 117734 vs 117811 | — | `docs/dev/eval/P6-F/rows/dev-placebo-400.json` |
| `calibrated_policy` | 197/400 (0.492, [0.444, 0.541]) | -0.015 [-0.086, +0.056] | 25/28 | 9805 vs 9868 / 118356 vs 118618 | 361; 36 / 38; -0.0050 [-0.0471, +0.0371] | `docs/dev/eval/P6-F/rows/dev-calibrated_policy-400.json` |
| `modal_lookahead` | 202/400 (0.505, [0.456, 0.554]) | +0.010 [-0.063, +0.083] | 29/27 | 9817 vs 9750 / 118289 vs 117910 | 41; 4 / 1; +0.0075 [-0.0034, +0.0184] | `docs/dev/eval/P6-F/rows/dev-modal_lookahead-400.json` |
| `yes_no_lookahead` | 193/400 (0.482, [0.434, 0.531]) | -0.035 [-0.105, +0.036] | 23/30 | 9843 vs 9788 / 120967 vs 117852 | 154; 13 / 19; -0.0150 [-0.0427, +0.0127] | `docs/dev/eval/P6-F/rows/dev-yes_no_lookahead-400.json` |
| `end_phase_actions` | 193/400 (0.482, [0.434, 0.531]) | -0.035 [-0.109, +0.040] | 26/33 | 9681 vs 9924 / 129760 vs 120378 | 269; 26 / 32; -0.0150 [-0.0523, +0.0223] | `docs/dev/eval/P6-F/rows/dev-end_phase_actions-400.json` |
| `burst_decline_list` | 199/400 (0.497, [0.449, 0.546]) | -0.005 [-0.076, +0.066] | 26/27 | 9772 vs 9744 / 117734 vs 117811 | 0; 0 / 0; +0.0000 [+0.0000, +0.0000] | `docs/dev/eval/P6-F/rows/dev-burst_decline_list-400.json` |
| `order_trigger_scorer` | 197/400 (0.492, [0.444, 0.541]) | -0.015 [-0.087, +0.057] | 26/29 | 9739 vs 9742 / 120940 vs 117611 | 33; 1 / 3; -0.0050 [-0.0148, +0.0048] | `docs/dev/eval/P6-F/rows/dev-order_trigger_scorer-400.json` |
| `exhaustive_polarity` | 201/400 (0.502, [0.454, 0.551]) | +0.005 [-0.070, +0.080] | 30/29 | 9763 vs 9676 / 121405 vs 116881 | 112; 14 / 12; +0.0050 [-0.0200, +0.0300] | `docs/dev/eval/P6-F/rows/dev-exhaustive_polarity-400.json` |
| `exact_fight` | 201/400 (0.502, [0.454, 0.551]) | +0.005 [-0.063, +0.073] | 25/24 | 9726 vs 9770 / 243522 vs 118208 | 154; 15 / 13; +0.0050 [-0.0209, +0.0309] | `docs/dev/eval/P6-F/rows/dev-exact_fight-400.json` |
| `opponent_knobs` | 196/400 (0.490, [0.441, 0.539]) | -0.020 [-0.088, +0.049] | 23/27 | 9801 vs 9816 / 117884 vs 118816 | 370; 39 / 42; -0.0075 [-0.0516, +0.0366] | `docs/dev/eval/P6-F/rows/dev-opponent_knobs-400.json` |
| `kill_aware_targeting` | 216/400 (0.540, [0.491, 0.588]) | +0.080 [+0.009, +0.150] | 35/19 | 9865 vs 9883 / 118485 vs 120365 | 331; 47 / 30; +0.0425 [-0.0003, +0.0853] | `docs/dev/eval/P6-F/rows/dev-kill_aware_targeting-400.json` |
| `p6f_all` | 184/400 (0.460, [0.412, 0.509]) | -0.080 [-0.154, -0.005] | 22/38 | 10140 vs 10004 / 264387 vs 121465 | 376; 36 / 51; -0.0375 [-0.0831, +0.0081] | `docs/dev/eval/P6-F/rows/dev-p6f_all-400.json` |
| `p6f_kept` | 204/400 (0.510, [0.461, 0.559]) | +0.020 [-0.049, +0.088] | 27/23 | 9937 vs 9965 / 245751 vs 120744 | 347; 40 / 35; +0.0125 [-0.0299, +0.0549] | `docs/dev/eval/P6-F/rows/dev-p6f_kept-400.json` |

Confirmation (once, 1,600 games, confirm-role master 30600100, the only promising row):
`kill_aware_targeting` 817/1600 = 0.511 [0.486, 0.535]; paired difference +0.021 [-0.015,
+0.057]; discordant 116/99; iterations per game 9998 vs 9886; applies per game 119288 vs 119093
(`docs/dev/eval/P6-F/rows/confirm-kill_aware_targeting-1600.json`).

Combined rows: `p6f_all` switches on all ten flags and measures negative (-0.080, the only row
whose interval excludes 0, and -0.038 against the placebo); `p6f_kept` switches on the five
mechanisms that measured non-negative alone (`modal_lookahead`, `burst_decline_list`,
`exhaustive_polarity`, `exact_fight`, `kill_aware_targeting`) and measures +0.020 [-0.049,
+0.088] (+0.013 against the placebo) at twice the applies per game (from `exact_fight`).

Kept (non-negative, with scenario or unit evidence of a real fix): `modal_lookahead`,
`burst_decline_list`, `exhaustive_polarity`, `exact_fight`. Not kept (negative):
`calibrated_policy`, `yes_no_lookahead`, `end_phase_actions`, `order_trigger_scorer`,
`opponent_knobs`. Re-measured and left out (null after confirmation, W-26):
`kill_aware_targeting`. Every flag stays implemented and available for the lead's bundle and
its leave-one-out rows.

The "vs placebo" column (`docs/dev/eval/P6-F/versus_placebo.py`) compares each row with the
placebo game by game. All rows share the master seed, so game i has the same deal, seats and
play/draw in every row, and the challenger is always the agent `search-standard` with the same
decision seeds: a game differs from the placebo's only where the mechanism changed a decision.
The column gives the games whose winner, length or decision count changed, the McNemar counts
(challenger wins only in the row / only in the placebo), and the difference per game with a
Wald 95% interval. It is a sharper paired comparison than the row's own (which pairs the two
seatings of a deal against agents with different seeds), and it isolates the mechanism; it is
added information, not a replacement for the §12.4 row.

Reading the table: the paired 95% intervals of single rows are about ±0.07 wide, and the placebo
row (the null configuration against the reference) lands at -0.005. No 400-game individual row
can resolve an effect of the size these mechanisms can have; the point estimates decide only the
non-negative/negative classification the brief asks for. Iterations per game differ only through
the games themselves (same budgets per decision); "applies per game" includes the auxiliary
applies of the look-aheads and exact fights (`SearchEffort::applies_aux`, capped at 8192 per
decision): `exact_fight` doubles them, `end_phase_actions` adds 8%, the others add at most 4%.

Leave-one-out rows need the full strengthened bundle, which the lead assembles (§12.9 step 4);
they are not run here.

## Items

### W-23 — Hand-written playout policy (`calibrated_policy`)

- **Status:** resolved (implemented, fitted, measured); measured negative at 400 games, so not
  kept (left out of the presets unless the bundle's leave-one-out row says otherwise).
- **Resolution as implemented:** `calibrated.rs`, `fitted.rs`, `fit.rs`, `fit_selfplay.rs`.
  The reference scores stay; six fitted offsets are added in score units: PLAY_COMMAND base
  `main_command` +0.894, ACTIVATE base `main_activate` -1.803, <Support> `main_support` 0.000
  (no Support activation occurred in the fit's games; the ridge keeps it at 0), action-step plays
  in a battle `battle_play` -0.556, end-phase plays `end_phase_play` +0.345, per EX Resource
  spent `ex_resource` -1.804 (on top of the reference's `EX_PENALTY` 1.5). The fit is a
  conditional logit over each decision's options, `logit = reference score / 0.5 + offsets · x`,
  by Newton's method (IRLS) in f64 with fixed-order sums, portable `exp`/`log`, ridge 1e-3 and a
  Cholesky solve, on 7,102 MAIN and ACTION_STEP decisions (8,842 searched decisions) of 120
  self-play games of `search:null-standard` on the 15 tuning pairs, tuning-role master 10600001;
  6 Newton steps. Because the offsets are indicator features of a maximum-likelihood fit, they
  are moment conditions: the model's expected share of each kind of move equals the search's
  own share on the fit's decisions (W-23's frequency matching, by construction instead of by a
  coordinate search). The weights file is `docs/dev/eval/P6-F/policy-fit.json` (values, exact
  bits, standard errors, shares); `fitted.rs` holds the same bits.
- **Evidence:**
  - Frequency gap (W-23's verification, target ≤ 5 points for Commands and activations), out
    of sample on 1,655 decisions of 30 dev-role games (master 20600001): Commands search 13.2%,
    reference policy 11.2%, calibrated 12.8% (gap 0.4 points); activations search 2.3%,
    reference 19.1%, calibrated 6.2% (gap 3.9 points). In sample the gaps are 0.0 by
    construction. Battle tricks (search 47.4%, reference 60.3%, calibrated 35.3%) and end-phase
    plays (19.4%, 9.5%, 16.2%) are recorded in the file too.
  - `mech::p6f::tests::fitted_constants_are_the_recorded_fit` (the constants are the report's
    bits, the masters lie in the tuning and dev ranges, the fit converged, and both gaps ≤ 5
    points): pass.
  - `mech::p6f::fit_selfplay::fitted_policy_reproduces` (feature `slow`): replays both self-play
    runs and the fit and compares the report byte for byte: pass (599 s, in a second build
    profile, so the fit does not depend on the build).
  - `mech::p6f::fit::tests::irls_recovers_a_known_model_deterministically`,
    `cholesky_solves_a_small_system`: pass.
- **Ablation:** individual row (dev master 20600100, 400 games): 197/400 = 0.492 [0.444, 0.541]; paired difference -0.015 [-0.086, 0.056]; discordant 25/28; iterations per game 9805.5 vs 9868.3; applies per game 118355.6 vs 118618.0; `docs/dev/eval/P6-F/rows/dev-calibrated_policy-400.json`. Leave-one-out: lead.

### W-24 — Modal choices scored flat (`modal_lookahead`)

- **Status:** resolved; measured non-negative at 400 games, so kept.
- **Resolution as implemented:** `scorer.rs` (`Scoring::by_lookahead` for SELECT decisions with
  the `Mode` context), `lookahead.rs`. Each mode is resolved on a clone of the rollout world
  (nested choices by the reference policy's best option, no random draws) until the effect has
  resolved, and scored `4 · (value − best value)` (floored at −8) in evaluation log-odds; used
  for rollout moves and for the priors of new tree nodes. Every apply counts as auxiliary work.
  The planned per-state cache is not built: rollout states never repeat within a decision, so
  it would not save applies.
- **Evidence:**
  - Unit test `modal_lookahead_picks_the_useful_mode` (Wings of Light, GD05-102, cast while the
    enemy Gundam attacks: one mode returns the attacker to hand, the other recovers 3 HP on an
    undamaged Unit), 20,000 rollout draws at temperature 0.5: reference 9,977 / 10,023 (pinned,
    about 50/50), `modal_lookahead` 20,000 / 0 (bar: ≥ 90%): pass.
  - Scenario `tests/scenarios/p6f_w24_modal.rs` (synthetic cards, `synthetic_modal_fragment.json`:
    a Unit whose end-of-turn modal trigger draws 1 or 3 against a Unit that draws 2 on deploy;
    the mode is chosen inside the search's simulated lines), 16 agent seeds at the reference
    `fast` budget: reference deploys the modal Unit 0/16 (pinned), `modal_lookahead` 16/16
    (bar ≥ 13): pass.
- **Ablation:** individual row (dev master 20600100, 400 games): 202/400 = 0.505 [0.456, 0.554]; paired difference 0.010 [-0.063, 0.083]; discordant 29/27; iterations per game 9817.1 vs 9750.0; applies per game 118288.8 vs 117909.7; `docs/dev/eval/P6-F/rows/dev-modal_lookahead-400.json`.

### W-25 — The opponent modelled with the searcher's own policy (`opponent_knobs`)

- **Status:** partly resolved: implemented, unit-tested and measured (negative at 400 games, so
  not kept); the over-extension scenario was **not** reproduced (below).
- **Resolution as implemented:** `mod.rs` (`rollout_choice`), `calibrated.rs`. The opponent's
  rollout moves use its own model: the reference scores plus the fitted offsets of W-23 (both
  seats of the fit are the search, so its choices are the opponent's too) at a quarter of the
  rollout temperature ("competent opponent", W-25 plan (1)); the searcher's own moves and every
  prior are unchanged. A purely predictive fit of the opponent's temperature (coefficient of the
  reference score fitted with the offsets on all 8,842 decisions: 0.312, temperature 3.21) is in
  the fit report (`opponent_predictive`) and not used: it would model the opponent as playing
  nearly at random. The per-deck profile (plan (2)) is not built; P6-C can read the same model
  for belief weighting (plan (3)).
- **Evidence:**
  - Unit test `opponent_knobs_model_the_opponent_by_its_own_policy`: player 1 choosing among
    three close attack targets, 20,000 rollout draws at temperature 0.5: shared policy picks the
    best target 11,279 times, the opponent model 18,866; the searcher's own moves are identical
    with and without the flag: pass.
  - Scenario: not delivered. Two "over-extension" positions were built and measured at the
    reference `standard` budget: (a) attacking with the best Unit, which the opponent can
    destroy while it is rested; (b) the same with the opponent's attack decision pushed past the
    tree by three deployments first. The reference and `opponent_knobs` both attacked in 16/16
    seeds in both, and the root values showed why: in (a) the opponent's reply lies inside the
    search tree (opponent nodes are searched, not rolled out), and in both the evaluation rates
    the attack as good even when the reply comes (one Shield against a 3/3 Unit). The fitted
    offsets also make the modelled opponent play battle tricks less often (the search plays them
    in 40% of its action steps, the reference policy in 65%), so the "punishing reply the shared
    policy plays only rarely" of the plan has no natural instance for this model. Needs the lead
    to decide whether a scenario with a stronger punishment is worth more construction.
- **Ablation:** individual row (dev master 20600100, 400 games): 196/400 = 0.490 [0.441, 0.539]; paired difference -0.020 [-0.088, 0.049]; discordant 23/27; iterations per game 9801.2 vs 9815.6; applies per game 117883.7 vs 118815.6; `docs/dev/eval/P6-F/rows/dev-opponent_knobs-400.json`.

### W-26 — Kill-aware targeting (`kill_aware_targeting`, re-measured)

- **Status:** resolved by re-measurement. The 400-game row was promising (+0.080 [+0.009, +0.150]), so it was confirmed once with 1,600 games on confirm-role seeds: +0.021 [-0.015, +0.057], null. With the regret null too (below), W-26's rule ("adopt only under the adoption rule, otherwise record the null row and leave them out again") leaves it out of the presets; the flag stays available.
- **Resolution as implemented:** `targets.rs`, `polarity.rs` (`choice_damage`). For a harmful
  choice whose step on the chosen variable is a `Damage` with a literal amount, an enemy Unit the
  damage destroys scores its worth plus 1.0, and one it does not destroy scores the share of its
  remaining HP the damage removes times half its worth (the measured-and-removed Python setting,
  re-implemented as a strengthened-policy option). Damaged-payoff targeting is not re-built.
- **Evidence:** decision-level regret per target decision with the W-43 tool, search level, on
  the same 157 target positions (80 games recorded by `search:null-standard`, 2 per game, 8
  shared worlds, oracle `ref:strong`, dev master 20600300):
  `gcg-eval regret search:null-standard ref:strong --recorder search:null-standard --decisions
  target --games 80 --per-game 2 --worlds 8 --seed 20600300 --role dev`: mean regret -0.0048
  (se 0.0044), 25 positions differing from the oracle; with `:kill_aware_targeting=true`:
  -0.0016 (se 0.0049), 25 differing. The difference (+0.0032, kill-aware slightly worse) is
  within noise, as in Python (`docs/python/AI.md:457-461`). Policy-level regret is not separately
  measured (the tool judges agents). Evidence files `rows/regret-*.json`.
- **Ablation:** individual row (dev master 20600100, 400 games): 216/400 = 0.540 [0.491, 0.588]; paired difference 0.080 [0.009, 0.150]; discordant 35/19; iterations per game 9864.9 vs 9882.7; applies per game 118485.0 vs 120364.9; `docs/dev/eval/P6-F/rows/dev-kill_aware_targeting-400.json`. Its confirmation: `gcg-eval ablation individual --mechanism kill_aware_targeting=true --reference standard --games 1600 --seed 30600100 --role confirm --choose-first seat0`: 817/1600 = 0.511 [0.486, 0.535]; paired difference +0.021 [-0.015, +0.057]; discordant 116/99; iterations per game 9998 vs 9886; applies per game 119288 vs 119093; `docs/dev/eval/P6-F/rows/confirm-kill_aware_targeting-1600.json`.

### W-27 — YES_NO answers (`yes_no_lookahead`)

- **Status:** resolved. Measured negative at 400 games (row -0.035, -0.015 against the placebo), so not kept.
- **Resolution as implemented:** `scorer.rs` (`yes_no`), `bursts.rs` (`waiting_may_review`). The
  "you may" is answered by look-ahead: when its body has no cost, no step that harms its
  controller's cards, no nested "you may" and no custom step, a non-losing YES gets the Burst
  score (4.0 in the presets); otherwise YES scores `4 · (value(YES) − value(NO))`, clamped to
  ±8. Pre-game choices are P6-H's.
- **Evidence:** unit test `yes_no_lookahead_declines_an_idle_cost_and_accepts_a_free_benefit`,
  20,000 draws at temperature 0.5: the idle optional cost (Garrod Ran & Tiffa Adill, GD02-094,
  "You may discard 1. If you do, look at the top 3 …" with no Vulture Unit on top): reference YES
  17,655 / 20,000 = 88.3% (pinned), `yes_no_lookahead` YES 42 / 20,000 (declined 99.8%; bar ≥
  90%); the free beneficial "you may" (Big-Rang, EB01-030, with a Lv.3 G Generation Unit on
  top): reference YES 88.3%, `yes_no_lookahead` YES 19,994 / 20,000 = 99.97% (bar ≥ 99.9%): pass.
- **Ablation:** individual row (dev master 20600100, 400 games): 193/400 = 0.482 [0.434, 0.531]; paired difference -0.035 [-0.105, 0.036]; discordant 23/30; iterations per game 9843.2 vs 9788.2; applies per game 120967.1 vs 117852.1; `docs/dev/eval/P6-F/rows/dev-yes_no_lookahead-400.json`.

### W-28 — End-phase action-step plays (`end_phase_actions`)

- **Status:** resolved by the scenario and unit test; measured negative at 400 games, so not
  kept.
- **Resolution as implemented:** `scorer.rs` (`end_phase`), `bursts.rs` (`play_outlasts_turn`),
  `lookahead.rs` (`Until::ThroughTurn`). In the end phase's action step, a Command or ability
  whose program rests, destroys, damages, returns, exiles or trashes a card, or applies an effect
  beyond this turn and battle, is scored against passing: both lines are played on with the
  reference policy's best options through the deciding player's next turn, and the play gets the
  pass score plus `4 · (value(play) − value(pass))`. Stopping at the next main phase (the first
  version) was wrong: the evaluation counts a removal Command still in hand as an answer to a
  Blocker (`_can_kill`'s `answers`) whatever its timing, so passing always looked as good; the
  unit test below pins the correct sign.
- **Evidence:**
  - Unit test `end_phase_lookahead_plays_the_rest_in_the_opponents_end_phase` (Unheralded Attack,
    an 【Action】 rest, in the opponent's end phase against its only Blocker): reference scores the
    play below passing; `end_phase_actions` scores it ≥ 4 points above passing (the look-ahead
    plays the next turn out and wins): pass.
  - Scenario `tests/scenarios/p6f_w28_end_phase.rs` (a Base with a synthetic "【Activate･Action】
    Rest this Base：Choose 1 enemy Unit with 3 or less HP. Rest it. Then, draw 1.",
    `synthetic_end_phase_fragment.json`; using it in the own end phase draws now but wastes the
    rest; kept for the opponent's end phase it wins the next turn), 16 agent seeds at the
    reference `fast` budget, both seats playing the configuration: reference wins 8/16 (pinned;
    bar ≤ 8), `end_phase_actions` 16/16 (bar ≥ 14): pass. The packaged "hold removal" puzzle
    itself (`ai_puzzles.rs`) does not reproduce the weakness: the reference solves it 16/16 at the
    `fast` budget (a Main/Action removal can still be cast in the next main phase), so the
    scenario uses an 【Action】-timed rest effect that is not a Command.
- **Ablation:** individual row (dev master 20600100, 400 games): 193/400 = 0.482 [0.434, 0.531]; paired difference -0.035 [-0.109, 0.040]; discordant 26/33; iterations per game 9680.8 vs 9924.4; applies per game 129759.8 vs 120377.6; `docs/dev/eval/P6-F/rows/dev-end_phase_actions-400.json`.

### W-29 — ORDER_TRIGGER scorer (`order_trigger_scorer`)

- **Status:** resolved. Measured negative at 400 games (row -0.015; 33 games changed against the placebo, -0.005), so not kept.
- **Resolution as implemented:** `order.rs`. With at most 6 orders, each is resolved on a clone
  (the rest of the batch by the reference policy) and scored by the evaluation; with more, a
  heuristic: +1 for the decider's own trigger that harms none of its cards, +2 per draw, -2 per
  own discard, +1 per damage step, -1 per recovery (damage before Repair).
- **Evidence:**
  - Unit test `order_heuristic_draws_before_discarding_and_damages_before_repair` (registry
    programs): pass.
  - Scenario `tests/scenarios/p6f_w29_order.rs` (synthetic cards, `synthetic_order_fragment.json`:
    a Unit with two simultaneous end-of-turn triggers, "draw 1" and "if you have 1 or fewer cards
    in your hand, draw 3", whose order leaves 5 or 2 cards in hand, against a Unit that draws 2 on
    deploy; the order is decided inside the search's simulated lines), 16 agent seeds at the
    reference `fast` budget: reference deploys the trigger Unit 7/16 (pinned), the
    `order_trigger_scorer` 16/16 (bar ≥ 15): pass. A first version with the order decision at the
    search's root does not reproduce the weakness (reference 16/16): the root search tells the
    orders apart by itself, so the weakness lies in the rollouts and priors.
- **Ablation:** individual row (dev master 20600100, 400 games): 197/400 = 0.492 [0.444, 0.541]; paired difference -0.015 [-0.087, 0.057]; discordant 26/29; iterations per game 9739.2 vs 9742.1; applies per game 120939.6 vs 117610.9; `docs/dev/eval/P6-F/rows/dev-order_trigger_scorer-400.json`.

### W-30 — Target polarity (`exhaustive_polarity`)

- **Status:** resolved. Measured non-negative at 400 games (row +0.005, +0.005 against the placebo), so kept.
- **Resolution as implemented:** `polarity.rs`, `targets.rs`. An exhaustive `match` over every
  `RuleKind` (`rule_kind_polarity`: a new kind fails to compile until signed); PLAY_COST_DELTA,
  PLAY_LEVEL_DELTA and `CostMod` signed by their amount's sign; CUSTOM rules signed per custom id
  by the table `CUSTOM_RULES` (the packaged registry grants none, so it is empty, and the test
  below fails on an unsigned one); `StatMod` totals signed from each part's sign, computed parts
  signed from their value node; all steps on the chosen variable combined (UNKNOWN when they
  disagree); a choice still UNKNOWN is scored by look-ahead.
- **Evidence:** unit tests `every_granted_rule_and_custom_id_is_signed` (every `RuleGrant` of
  the registry is signed, no CUSTOM id is missing, every kind is signed or amount-signed) and
  `exhaustive_polarity_signs_the_listed_sites_and_computed_effects` (the sites of
  `tests/ai/test_policy_knobs.py:96-105`: CANT_RECEIVE_DAMAGE and CANT_BE_ATTACKED on friendly
  Units help, STAYS_RESTED_IN_START_PHASE and CANT_BLOCK on enemy Units harm, MAY_ATTACK_ACTIVE
  helps; the stat change with a computed amount, which the reference leaves UNKNOWN, helps): pass.
- **Ablation:** individual row (dev master 20600100, 400 games): 201/400 = 0.502 [0.454, 0.551]; paired difference 0.005 [-0.070, 0.080]; discordant 30/29; iterations per game 9762.7 vs 9675.7; applies per game 121405.4 vs 116881.1; `docs/dev/eval/P6-F/rows/dev-exhaustive_polarity-400.json`.

### W-31 — Card and fight values (`exact_fight`)

- **Status:** resolved for fights; measured non-negative at 400 games, so kept (it doubles the
  applies per game). The keyword-worth half of the plan depends on P6-G's per-keyword features
  (W-37) and is not done here.
- **Resolution as implemented:** `fight.rs`. The attack and block scorers of the reference policy
  are ported line by line with every `fight()` replaced by the engine's resolution of the battle
  on a clone: the attack (or the block) and the rest of the battle (the forced blocker if the
  engine offers it, otherwise no block; passes in action steps; the reference policy's best
  option elsewhere), the result being which of the two Units left the battle area. Damage
  prevention and reduction, First Strike, Breach, redirects and battle triggers are therefore
  exact; a Blocker the engine does not let block adds no risk. Memoised per `(attack, blocker)`
  within one scoring.
- **Evidence:** unit test `exact_fights_equal_the_engine_battles`: 98 attacker/defender pairs
  (First Strike Units GD04-034 and GD05-055, damage-reducing GD04-068 and GD05-055, damage-immune
  EB01-014, eight Units with 【Attack】 triggers, four plain Units): the exact fight equals the
  engine's battle on every pair; the reference `fight()` disagrees on 2 (pinned): pass.
- **Ablation:** individual row (dev master 20600100, 400 games): 201/400 = 0.502 [0.454, 0.551]; paired difference 0.005 [-0.063, 0.073]; discordant 25/24; iterations per game 9726.2 vs 9770.0; applies per game 243522.1 vs 118207.8; `docs/dev/eval/P6-F/rows/dev-exact_fight-400.json`.

### W-163 — Burst acceptance assumption (`burst_decline_list`)

- **Status:** resolved. Measured null at 400 games (every game identical to the placebo), so kept: it changes nothing on the packaged decks and guards future cards.
- **Resolution as implemented:** `bursts.rs`, `scorer.rs` (`burst`). The tripwire lists every
  packaged Burst whose program has a cost, a step that discards, mills, damages, destroys, rests,
  returns, exiles or trashes its controller's own cards or damages its controller (adding cards to
  the hand and deploying the card itself are not harm), a nested "you may" (an optional
  replacement), or a custom step. The strengthened policy answers a listed Burst by look-ahead
  (accepted with the Burst score unless the look-ahead shows a loss, declined as far as it
  loses) and keeps the high score for every other Burst.
- **Evidence:**
  - The reviewed list on the packaged pool is exactly `GD05-089: optional` (Master Asia: "you may
    deploy it as an (AP3･HP3) Unit instead", an optional replacement whose two answers are both
    benefits): `burst_tripwire_holds_on_the_packaged_pool`: pass. No harmful Burst is listed, as
    the plan expected.
  - Negative control: the synthetic Burst "Discard 2 cards. Draw 1." on EB01-007 in a temporary
    data copy (`synthetic_burst_fragment.json`, from `synthetic_cards.py`, built with
    `export_ir.build_fragment`) fails the tripwire and is listed as `self_harm`:
    `burst_tripwire_fails_on_a_synthetic_harmful_burst`: pass.
  - Rollout distributions, 20,000 draws at temperature 0.5
    (`burst_decline_list_declines_a_harmful_burst_and_accepts_packaged_ones`): the synthetic
    Burst, reference YES 19,994 (99.97%), `burst_decline_list` YES 42 (declined 99.8%; bar ≥ 90%);
    packaged Bursts ST02-014 (off the list) and GD05-089 (on it), `burst_decline_list` YES
    19,994 / 20,000 = 99.97% each (bar ≥ 99.9%): pass.
- **Ablation:** individual row (dev master 20600100, 400 games): 199/400 = 0.497 [0.449, 0.546]; paired difference -0.005 [-0.076, 0.066]; discordant 26/27; iterations per game 9772.0 vs 9744.2; applies per game 117734.4 vs 117811.0; `docs/dev/eval/P6-F/rows/dev-burst_decline_list-400.json`. No Burst of the panel's decks is on the list, so the
  row plays the reference's Burst answers: all 400 games are identical to the placebo's.

## Gate

Run after the last code edit (logs not committed):

| Command | Result |
| --- | --- |
| `cargo fmt --all --check` | clean |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | clean |
| `cargo test --workspace --release` | 998 passed, 0 failed, 0 ignored (119 test result lines), including `tests/null_config.rs` (4), `tests/null_config_traces.rs`, the 15 P6-F unit tests and the 3 scenario tests |
| `cargo test -p gcg-search --release --features slow --lib fitted_policy_reproduces` | pass: the self-play and the fit reproduce `policy-fit.json` byte for byte (599 s) |
| `target/release/gcg-diff corpus --tier 1 --require-all` | 3804/3804 traces (`search` 9/9, `tests` 3213/3213, `fuzz` 100/100, `worlds` 100/100, `views` 280/280, `vanilla` 40/40) |
| `COMPARE_OUT=build/p6f/compare scripts/compare_python_vs_rust.sh equivalence --matches 2 --seeds 1 --formats bo1 --pairs red-green-zeon:blue-white-federation --workers 1 --decision-logs off` | 3/3 IDENTICAL (`fast`, `standard`, `strong`) |
| `target/release/gcg-eval evidence check` | pass (this file's cited paths exist) |
| `uv run ruff check docs/dev/eval/P6-F/` and `uv run ruff format --check docs/dev/eval/P6-F/` | clean |

The ablation rows ran on two builds of the same branch: the rows `calibrated_policy`,
`opponent_knobs`, `exact_fight`, `modal_lookahead`, `exhaustive_polarity` and
`burst_decline_list` on the build before the end-phase look-ahead was changed to follow the next
turn (`Until::ThroughTurn`), the others on the final build. The change touches only
`end_phase_actions` (the `Until::Effect` look-ahead is identical in both), so those rows measure
the final code.

## Not done

- W-25's over-extension scenario (above): two constructions measured, neither reproduces the
  weakness; reported with the reasons.
- Leave-one-out rows: they need the lead's full bundle (§12.9 step 4).
- W-31's keyword worth from fitted per-keyword features: depends on P6-G (W-37).
- W-25's per-deck opponent profile (plan item (2)) and the belief weighting by the opponent
  model (item (3), P6-C's).
- W-26's policy-level regret: the W-43 tool judges agents, so only the search-level regret was
  measured.
- W-24's per-state look-ahead cache: not built (rollout states do not repeat within a decision).
