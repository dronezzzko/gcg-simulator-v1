# P6-A weakness status: horizon and leaf (S2; W-01, W-02, W-15)

Package P6-A (docs/port/PLAN.md §12.2, §12.3 S2): the per-preset `horizon_turns` (a
configuration field the core already reads), and the flags `adaptive_horizon` (W-01),
`generalized_leaf` (W-02) and `capped_rollout_continuation` (W-15). Written by the P6-A
implementer on 2026-11-02 in worktree branch `worktree-wf_94545b98-29a-3`, from `8af9b0e`
("Port M13 + M12.L/M/N"). WEAKNESSES.md is not edited here (the lead records the status lines
from this file, §12.10). `presets.rs` is unchanged: adopting the flags and the horizon in the
shipped presets is the lead's decision; the recommendation is at the end of this file.

## What was built

All three P6-A flags are implemented (`p6a::PROVIDES`). With every P6-A flag off, `Composed`
calls the reference `horizon` and `leaf_value`, so the null configuration still replays the
reference bit for bit (`tests/null_config.rs`, `tests/null_config_traces.rs` and the `slow`
`null_config_search_set`, unchanged and passing).

| File | Content |
| --- | --- |
| `crates/gcg-search/src/mech/p6a/mod.rs` | `PROVIDES`, `active`, `check` (the leaf-constant check of a configuration), `DecisionState` (Cell counters: auxiliary applies of capped-rollout continuations, extended lines, capped lines, lines still unsettled after their continuation), `horizon` and `leaf_value` (the `Mechanisms` overrides), `loss_value` (the engine-error loss under the generalised constants) |
| `mech/p6a/horizon.rs` | `adaptive_horizon`: the unsettled test (`unsettled`) and the extension decision (`decide`), `MAX_EXTENSION` = 1, `RACE_TURNS` = 2 |
| `mech/p6a/leaf.rs` | `generalized_leaf`: `LeafMapping` (floor, decay, the longest line it covers), `LeafMapping::of(cfg)`, the constructor check `LeafMapping::check` |
| `mech/p6a/capped.rs` | `capped_rollout_continuation`: `settled`, `continue_line`, `CONTINUATION_DECISIONS` = 64 |
| `mech/p6a/tests.rs` | 10 unit tests (below) |
| `crates/gcg-search/tests/scenarios/p6a_w01_two_turn_plan.rs` | the W-01 scenario (3 tests) |
| `docs/port/eval/P6-A/` | the ablation reports `rows/*.json`, `versus_placebo.py`, `seed-ledger.json` |

Wiring outside `mech/p6a/` (flagged to the lead):

- `crates/gcg-search/src/mech/mod.rs` (the p6a lines): `pub(crate) mod p6a;`; the `p6a`
  `DecisionState` field of `Composed` (fresh in `new` and `with_state`; it holds only counters,
  so a root tree of `root_trees` needs nothing carried between phases); `Composed::aux_applies`
  adds P6-A's continuation applies; `Composed::root_moves` runs `p6a::check(cfg)` first when a
  P6-A flag is on and fails the decision with `EngineError::Value` (an error outside simulated
  lines, so `engine_error_leaf` never absorbs it); `Composed::horizon` (with
  `adaptive_horizon`), `Composed::leaf_value` (with `generalized_leaf` or
  `capped_rollout_continuation`) and `Composed::error_leaf` (with `generalized_leaf`, the loss
  is `decay * turns` of the generalised constants); `available` reads `p6a::PROVIDES`. The
  `expect(dead_code)` attributes on `HorizonDecision::Extend`, `LeafCx::horizon` and
  `LeafCx::capped` are removed (P6-A reads them); `LeafCx::decisions` keeps its attribute (P6-A
  does not read it).
- `crates/gcg-search/tests/scenarios/main.rs`: one line, `mod p6a_w01_two_turn_plan;` (the
  shared scenario target; the lead unions the packages' `mod` lines).

## Ablation rows (PLAN §12.4)

Two kinds of row, 400 games each in mirrored blocks of four, stratified round-robin over the 15
tuning pairs of `docs/port/eval/panel.json`, forced play/draw (`--choose-first seat0`, A24):

- **Individual rows** (the plan's row): `gcg-eval ablation individual --reference standard`, the
  challenger `search:null-standard:<mechanism>` (the reference `standard` budget, every other
  mechanism off) against `ref:standard`, dev master 20100100 for every row, so all rows share
  their deals. The placebo row runs `--mechanism engine_error_leaf=false` (a no-op: the null
  configuration, which replays the reference), so it measures the noise of a row whose two agents
  draw different decision seeds.
- **High-budget rows**: `gcg-eval ablation leave-one-out --bundle
  search:standard:iterations_per_option=256,min_iterations=640,max_iterations=2560,<mechanism on>
  --mechanism <mechanism off>`, the shipped `standard` preset (reference `strong` settings,
  `engine_error_leaf` on) at twice its shipped budget with the mechanism against the same without
  it, dev master 20100200.

Commands:

```bash
target/release/gcg-eval ablation individual --mechanism <field=value,...> --reference standard \
  --games 400 --seed 20100100 --role dev --ledger docs/port/eval/P6-A/seed-ledger.json \
  --workers 8 --choose-first seat0 --out docs/port/eval/P6-A/rows/dev-individual-<row>-400.json
target/release/gcg-eval ablation leave-one-out \
  --bundle search:standard:iterations_per_option=256,min_iterations=640,max_iterations=2560,<on> \
  --mechanism <off> --games 400 --seed 20100200 --role dev \
  --ledger docs/port/eval/P6-A/seed-ledger.json --workers 8 --choose-first seat0 \
  --out docs/port/eval/P6-A/rows/dev-highbudget-<row>-400.json
```

Confirmation rows (1,600 games, confirm master 30100100, run once) use the individual command
with `--games 1600 --seed 30100100 --role confirm` and write
`rows/confirm-individual-<row>-1600.json`. The rows ran with 6 or 8 workers (results are
identical for any worker count); the confirmations wrote their ledger entries to a scratch ledger
that was then appended, in run order, to `docs/port/eval/P6-A/seed-ledger.json` (18 entries: dev
masters 20100100 and 20100200, confirm master 30100100).

Individual rows (reference `standard` budget, dev master 20100100):

| Row | mechanism | wins/N (rate, Wilson 95%) | paired difference (Newcombe 95%) | discordant | iterations / applies per game (challenger vs opponent) | vs placebo: changed games; row-only / placebo-only wins; difference [95%] | evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `placebo` | `engine_error_leaf=false` | 208/400 (0.520, [0.471, 0.569]) | +0.040 [-0.033, +0.112] | 32/24 | 9946 vs 9924 / 120263 vs 119879 | — | `docs/port/eval/P6-A/rows/dev-individual-placebo-400.json` |
| `horizon3` | `horizon_turns=3` | 185/400 (0.463, [0.414, 0.511]) | -0.075 [-0.148, -0.000] | 22/37 | 9755 vs 9932 / 174528 vs 116352 | 391; 41 / 64; -0.0575 [-0.1074, -0.0076] | `docs/port/eval/P6-A/rows/dev-individual-horizon3-400.json` |
| `horizon4` | `horizon_turns=4` | 201/400 (0.502, [0.454, 0.551]) | +0.005 [-0.067, +0.077] | 28/27 | 9963 vs 10064 / 228382 vs 119610 | 395; 46 / 53; -0.0175 [-0.0662, +0.0312] | `docs/port/eval/P6-A/rows/dev-individual-horizon4-400.json` |
| `adaptive` | `adaptive_horizon=true` | 218/400 (0.545, [0.496, 0.593]) | +0.090 [+0.018, +0.161] | 37/19 | 10010 vs 9864 / 160122 vs 116450 | 382; 51 / 41; +0.0250 [-0.0219, +0.0719] | `docs/port/eval/P6-A/rows/dev-individual-adaptive-400.json` |
| `generalized` | `generalized_leaf=true` | 207/400 (0.517, [0.469, 0.566]) | +0.035 [-0.036, +0.105] | 30/23 | 9979 vs 9920 / 120640 vs 119940 | 78; 6 / 7; -0.0025 [-0.0202, +0.0152] | `docs/port/eval/P6-A/rows/dev-individual-generalized-400.json` |
| `capped` | `capped_rollout_continuation=true` | 208/400 (0.520, [0.471, 0.569]) | +0.040 [-0.033, +0.112] | 32/24 | 9946 vs 9924 / 120263 vs 119879 | 0; 0 / 0; +0.0000 [+0.0000, +0.0000] | `docs/port/eval/P6-A/rows/dev-individual-capped-400.json` |
| `p6a-h2` | `adaptive_horizon=true,generalized_leaf=true,capped_rollout_continuation=true` | 212/400 (0.530, [0.481, 0.578]) | +0.060 [-0.014, +0.133] | 35/23 | 10058 vs 9839 / 160996 vs 116367 | 391; 48 / 44; +0.0100 [-0.0370, +0.0570] | `docs/port/eval/P6-A/rows/dev-individual-p6a-h2-400.json` |
| `p6a-h3` | `horizon_turns=3,adaptive_horizon=true,generalized_leaf=true,capped_rollout_continuation=true` | 194/400 (0.485, [0.436, 0.534]) | -0.030 [-0.098, +0.039] | 22/28 | 9870 vs 9980 / 212367 vs 117957 | 389; 39 / 53; -0.0350 [-0.0819, +0.0119] | `docs/port/eval/P6-A/rows/dev-individual-p6a-h3-400.json` |

Confirmations (1,600 games, confirm master 30100100, run once; the placebo row is the control of
this seed family, not a mechanism):

| Row | mechanism | wins/N (rate, Wilson 95%) | paired difference (Newcombe 95%) | discordant | iterations / applies per game (challenger vs opponent) | vs placebo: changed games; row-only / placebo-only wins; difference [95%] | evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `placebo` | `engine_error_leaf=false` | 799/1600 (0.499, [0.475, 0.524]) | -0.001 [-0.036, +0.034] | 101/102 | 9806 vs 9866 / 118260 vs 118869 | — | `docs/port/eval/P6-A/rows/confirm-individual-placebo-1600.json` |
| `adaptive` | `adaptive_horizon=true` | 830/1600 (0.519, [0.494, 0.543]) | +0.037 [+0.000, +0.075] | 132/102 | 9954 vs 9951 / 159206 vs 117625 | 1546; 201 / 170; +0.0194 [-0.0042, +0.0430] | `docs/port/eval/P6-A/rows/confirm-individual-adaptive-1600.json` |
| `generalized` | `generalized_leaf=true` | 789/1600 (0.493, [0.469, 0.518]) | -0.014 [-0.048, +0.021] | 95/106 | 9779 vs 9865 / 118095 vs 118781 | 335; 7 / 17; -0.0063 [-0.0122, -0.0003] | `docs/port/eval/P6-A/rows/confirm-individual-generalized-1600.json` |
| `p6a-h2` | `adaptive_horizon=true,generalized_leaf=true,capped_rollout_continuation=true` | 834/1600 (0.521, [0.497, 0.546]) | +0.043 [+0.006, +0.079] | 128/94 | 9914 vs 9930 / 158461 vs 117524 | 1556; 203 / 168; +0.0219 [-0.0017, +0.0454] | `docs/port/eval/P6-A/rows/confirm-individual-p6a-h2-1600.json` |

High-budget rows (`search:standard` at twice its shipped budget with the mechanism, against the
same without it, dev master 20100200; the mechanism column is what the challenger switches on).
The row `capped` is this family's placebo: its mechanism never fires (below), so its two agents
play the same configuration and its result is the noise of a high-budget row whose agents draw
different decision seeds:

| Row | mechanism | wins/N (rate, Wilson 95%) | paired difference (Newcombe 95%) | discordant | iterations / applies per game (challenger vs opponent) | vs placebo: changed games; row-only / placebo-only wins; difference [95%] | evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `capped` | `capped_rollout_continuation=true` | 193/400 (0.482, [0.434, 0.531]) | -0.035 [-0.108, +0.038] | 25/32 | 32722 vs 32810 / 381007 vs 381122 | — | `docs/port/eval/P6-A/rows/dev-highbudget-capped-400.json` |
| `horizon3` | `horizon_turns=3` | 194/400 (0.485, [0.436, 0.534]) | -0.030 [-0.111, +0.051] | 32/38 | 32453 vs 33025 / 570162 vs 375141 | 395; 58 / 57; +0.0025 [-0.0500, +0.0550] | `docs/port/eval/P6-A/rows/dev-highbudget-horizon3-400.json` |
| `horizon4` | `horizon_turns=4` | 201/400 (0.502, [0.454, 0.551]) | +0.005 [-0.067, +0.077] | 28/27 | 32801 vs 33280 / 728960 vs 382473 | 394; 65 / 57; +0.0200 [-0.0341, +0.0741] | `docs/port/eval/P6-A/rows/dev-highbudget-horizon4-400.json` |
| `adaptive` | `adaptive_horizon=true` | 203/400 (0.507, [0.459, 0.556]) | +0.015 [-0.057, +0.087] | 29/26 | 32675 vs 32755 / 506201 vs 373842 | 393; 55 / 45; +0.0250 [-0.0239, +0.0739] | `docs/port/eval/P6-A/rows/dev-highbudget-adaptive-400.json` |
| `generalized` | `generalized_leaf=true` | 195/400 (0.487, [0.439, 0.536]) | -0.025 [-0.095, +0.046] | 24/29 | 32835 vs 32954 / 381563 vs 382522 | 97; 6 / 4; +0.0050 [-0.0105, +0.0205] | `docs/port/eval/P6-A/rows/dev-highbudget-generalized-400.json` |
| `p6a-h3` | `horizon_turns=3,adaptive_horizon=true,generalized_leaf=true,capped_rollout_continuation=true` | 203/400 (0.507, [0.459, 0.556]) | +0.015 [-0.060, +0.089] | 31/28 | 33231 vs 33107 / 700242 vs 384079 | 393; 60 / 50; +0.0250 [-0.0263, +0.0763] | `docs/port/eval/P6-A/rows/dev-highbudget-p6a-h3-400.json` |

Reading the tables:

- The "vs placebo" column (`docs/port/eval/P6-A/versus_placebo.py`) compares a row with its
  family's placebo game by game: game i has the same deal, seats, play/draw and decision seeds
  in every row of a family, so a game differs from the placebo only where the mechanism changed a
  decision. It is a sharper paired comparison than the row's own and isolates the mechanism; it is
  added information, not a replacement for the §12.4 row (the method of P6-F).
- A fixed longer horizon does not help. Three turns lose at the reference `standard` budget
  (-0.075 [-0.148, -0.000]; -0.058 [-0.107, -0.008] against the placebo, the largest effect
  measured here) and are null at the high budget (-0.030 [-0.111, +0.051];
  +0.003 against the placebo) at 1.5x the applies; four turns are null at both budgets (+0.005;
  -0.018 and +0.020 against the placebos) at 1.9x the applies. The Python history agrees: a
  three-turn horizon at twice the budget won 51/96 on the older engine (`docs/AI.md:366-368`).
- `adaptive_horizon` is the one mechanism that measured positive. Against the placebo it gains
  +0.025 (dev), +0.019 (confirm) and +0.025 (high budget); pooled over the 2,000 games of the
  reference-budget rows, +0.021 [-0.001, +0.042] per game (252 row-only against 211
  placebo-only wins). Its own rows are +0.090 [+0.018, +0.161] (dev), +0.037 [+0.000, +0.075]
  (confirm) and +0.015 [-0.057, +0.087] (high budget). It costs about 35% more applies per game
  at the same iterations.
- `generalized_leaf` alone changes few games (78 of 400, 335 of 1,600, 97 of 400) and is null to
  slightly negative: -0.0063 [-0.0122, -0.0003] against the placebo on the confirmation (7
  row-only against 17 placebo-only wins). At `horizon_turns = 2` without an extension it only
  raises `decay` from 0.01 to 0.0167 (a stronger preference for faster wins), which the
  measurement does not support.
- The horizon-2 bundle (`p6a-h2`: `adaptive_horizon` with `generalized_leaf` and
  `capped_rollout_continuation`) confirms at +0.043 [+0.006, +0.079] (+0.022 [-0.002, +0.045]
  against the placebo), the same as `adaptive_horizon` alone within noise.
- The paired 95% intervals of single 400-game rows are about ±0.07 wide, and the two placebo
  rows land at +0.040 (dev) and -0.035 (high budget): no 400-game row can resolve effects of the
  size measured here. The confirmations (±0.035) and the placebo comparisons (±0.02 to ±0.05)
  carry the decisions above.

Every row ran one release `gcg-eval` built from this package's mechanism code (the commit's
`mech/p6a/` minus a later change that only touches lines longer than any search line, below
under W-02). "Applies per game" counts every engine apply of the agent: tree, rollout (the
extended turns of `adaptive_horizon` are rollout applies) and auxiliary (the continuation of
capped rollouts). No row's agents hit the rollout decision cap: `rollouts_capped` is 0 for both
agents in every report above (about 4 million searched iterations per agent in each 400-game row
at the reference `standard` budget, 13 million at the high budget), so
`capped_rollout_continuation` never fires on the panel and its rows play the games of the rows
without it.

## Items

### W-01 — Two-turn rollout horizon (`horizon_turns`, `adaptive_horizon`)

- **Status:** resolved: implemented, scenario-tested and measured. A fixed three-turn (or
  four-turn) horizon measured negative or null at both budgets and is **not** recommended, a
  deviation from the plan's "standard starts at 3" (PLAN §12.2) that the measurement decides
  (brief step 22: keep only changes with a measured non-negative effect). `adaptive_horizon`
  measured non-negative in every row (+0.037 [+0.000, +0.075] on its confirmation, about +0.02
  against the placebos at both budgets); it is recommended.
- **Resolution as implemented:**
  - `horizon_turns` is a configuration field the core reads (`mcts.rs`: the line's horizon is
    `root turn + horizon_turns`); presets and `search:<preset>:horizon_turns=N` set it. Rollout
    decisions are applies, so its cost is explicit in "applies per game". The reference keeps
    `horizon_turns = 2`.
  - `adaptive_horizon` (`mech/p6a/horizon.rs`): when a rollout stops at its horizon with the game
    unfinished and the decision cap not hit, the line plays one more turn boundary (and gets a
    further `max_rollout_decisions` decisions) if its leaf is unsettled, judged from the line's
    own determinised world: a battle, an effect frame or a pending trigger, a pending decision
    other than a main-phase one, or either side's race estimate (`_race`,
    `ai/evaluation.py:244-277`, read as `1 / race` from `role_features`) at most 2 turns. At most
    one extension per line (`MAX_EXTENSION`), so a line advances at most `horizon_turns + 1`
    turns, which the leaf check covers (W-02). The extended turn is played by the configuration's
    rollout policy with the line's own rollout generator; nothing else draws a number. On the 9
    searched decisions of the D2 vectors it extends 245 of 2,147 lines; in the games of the rows
    it adds about 35% applies per game at the same iterations.
  - The refit of the evaluation at the new leaf point (plan item (4), W-34) is P6-G's.
- **Evidence:**
  - Scenario `tests/scenarios/p6a_w01_two_turn_plan.rs` (a testkit position built from packaged
    cards; the module docs give the position and both plans). Player 0 holds a Link Unit and its
    Pilot, playable only in its next turn (T+2); two moves (attack the player with DINN, or end the turn)
    win by force in turn T+2, when the Link Unit, deployed and linked, attacks the turn it is
    deployed; the third (DINN trades with the rested Ez8) lets the defender keep its Blocker. 16
    agent seeds each:
    - premise (`w01_both_plans_win_by_force_and_the_trade_does_not`; after the forced first
      move both seats play the P6-A bundle at the reference `strong` budget): attack 16/16 and
      end 16/16 win by the end of turn T+2, the trade 0/16;
    - reference `standard` budget (`w01_a_longer_horizon_finds_the_two_turn_plan`): the reference
      configuration plays a plan 0/16 (it trades in all 16; recorded, bar ≤ 8 plans), horizon 3
      16/16, `adaptive_horizon` (horizon 2) 16/16, the P6-A bundle (horizon 3 and every P6-A
      flag) 16/16 (bar ≥ 14): pass. At the reference leaf, the start of turn T+2, no line shows
      lethal (the evaluation cannot see a Link Unit in hand attack this turn), so the three
      moves get nearly equal root values and the trade the most visits; in a diagnostic search
      of the position (search seed 1, not a committed test) the reference valued attack, trade
      and end 0.110, 0.113 and 0.103 (66, 70 and 54 visits), horizon 3 0.778, 0.492 and 0.611
      (93, 7 and 10 visits).
    - reference `strong` budget (`w01_the_strong_budget_keeps_the_plan_with_a_longer_horizon`):
      the reference plays a plan 16/16 here (recorded; the position does not reproduce the
      weakness at this budget), the P6-A bundle 14/16 (bar ≥ 14): pass.
  - Unit tests (`mech/p6a/tests.rs`): `the_unsettled_test_reads_the_race_and_pending_resolutions`
    (a two-turn race, a battle in progress and a quiet board),
    `adaptive_horizon_extends_an_unsettled_leaf_once` (an unsettled leaf at its horizon extends
    once; a capped line, a line before its horizon, an already extended line and a settled leaf
    do not), `adaptive_horizon_plays_longer_lines_deterministically` (the counts above; two runs
    give identical results): pass.
- **Ablation:** see the tables above: individual rows `horizon3`, `horizon4`, `adaptive`,
  `p6a-h3`, `p6a-h2` and their confirmations; high-budget rows `horizon3`, `horizon4`,
  `adaptive`, `p6a-h3` (evidence files `docs/port/eval/P6-A/rows/`).

### W-02 — Leaf-value constants cap the horizon at four turns (`generalized_leaf`)

- **Status:** resolved: implemented and unit-tested; measured null to slightly negative (it only
  rescales the terminal values' turn shading: -0.006 [-0.012, -0.000] against the placebo on its
  confirmation), so it is **not** recommended for the presets at `horizon_turns = 2`; it stays
  available and is required whenever the longest line passes four turns (`horizon_turns = 4`
  with `adaptive_horizon`, or `horizon_turns >= 5`), where the reference constants are refused.
- **Resolution as implemented:** `mech/p6a/leaf.rs`. With the flag, a finished line scores
  `1 - decay * turns` (win) and `decay * turns` (loss), an unfinished one
  `floor + (1 - 2 floor) * σ(score / value_scale)`, with `floor = LEAF_FLOOR` (0.05) and
  `decay = floor / (max_turns + 1)`, `max_turns = max(horizon_turns, 1) + max_extension` (1 with
  `adaptive_horizon`, else 0): the most turns any search line can advance (the root move is
  applied before the horizon test, and one apply crosses at most one turn boundary). The turns
  counted are capped at `max_turns`, so a finished line longer than any search line (a P6-H
  pre-game simulation played to the end) still outranks every estimate; search lines never reach
  the cap. The engine-error loss (`engine_error_leaf`) uses the same constants. The constructor
  check (`LeafMapping::check`, run by `p6a::check` before the root moves of every decision whose
  configuration has a P6-A flag on) requires the worst win to exceed the best estimate and the
  best loss to fall below the worst estimate at the longest line, so the reference constants
  with an extension past four turns (`horizon_turns = 4` with `adaptive_horizon`) are refused
  with "a line of 5 turns is too long for the reference leaf constants ... enable
  generalized_leaf". Without the flag the reference constants and expressions are used, bit for
  bit. The reference keeps `LEAF_FLOOR`, `WIN_DECAY` and its check
  (`gcg_ai_reference::ReferenceConfig::check`) unchanged.
- **Evidence:** unit tests `generalized_leaf_constants_order_terminal_results_at_every_horizon`
  (W-02's property test: horizons 1..8 and extensions 0..2, every line length up to
  `max_turns + 40`, the decay formula bit for bit, the shading strictly monotone up to
  `max_turns`), `the_reference_constants_keep_their_check` (4 turns pass, 5 fail; the reference
  configuration with `horizon_turns = 5` is still rejected with the Python message),
  `the_leaf_check_covers_the_adaptive_extension`,
  `a_search_refuses_a_horizon_its_leaf_constants_cannot_order` (the decision fails with a
  `ValueError` before any iteration; with `generalized_leaf` it searches),
  `leaf_values_use_the_configured_constants` (without the flag the value equals
  `reference::leaf_value` bit for bit on 24 vector states; with it a win and a loss after two
  turns and the engine-error loss use `decay = 0.05 / 4` at `horizon_turns = 3`): pass.
- **Ablation:** individual row `generalized` and its confirmation, high-budget row `generalized`
  (tables above). The change of the clamp at `max_turns` came after the rows' build; search lines
  never exceed `max_turns`, so it changes no search decision.

### W-15 — Rollout cap of 250 decisions: capped rollouts are evaluated mid-turn (`capped_rollout_continuation`)

- **Status:** resolved: implemented, unit-tested and measured. The cap is never hit on the panel
  at either budget (0 capped rollouts in every row), so the mechanism changes no game; it costs
  nothing when the cap is not hit and is kept as a guard for decks whose rollouts loop.
- **Resolution as implemented:** `mech/p6a/capped.rs`. A rollout that stopped at its decision cap
  is continued on a copy with the deterministic policy (the reference policy's best option, no
  random draws) until a main-phase decision with nothing resolving or the end of the game, for at
  most 64 further decisions and never past the line's horizon turn, and scored there by the leaf
  mapping; a line still unsettled scores 0.5. The continuation's applies are auxiliary applies
  (`SearchEffort::applies_aux`), every capped line is counted by the mechanism, and the core's
  `rollouts_capped` still counts every cap hit. The cap and the continuation are documented here
  for the Rust docs of the presets (PLAN §12.3 S2; the Rust `docs/AI.md` is the lead's).
- **Evidence:** unit tests `capped_rollouts_continue_to_a_main_phase_decision` (with
  `max_rollout_decisions = 2` on 6 vector states the trace records cap hits, the mechanism counts
  exactly `rollouts_capped` lines, spends auxiliary applies, and settles at least one; on 24
  vector states two policy moves then the continuation end at a main-phase decision or a
  finished game whenever they settle, and otherwise stop after exactly 64 decisions) and `an_unsettled_capped_line_scores_one_half` (a capped line in a
  battle past its horizon scores 0.5 and is counted; within its horizon it finishes the battle
  and is evaluated): pass. The e2e capped-rollout rate (W-15's verification): 0 capped rollouts
  per decision for `search:null-standard` and `ref:standard` (individual rows, 400 games, about
  15,000 searched decisions per agent) and for `search:standard` at twice its budget (high-budget
  rows), so the fallback needs no ablation decision; the rows were run anyway.
- **Ablation:** individual row `capped` (identical, game for game, to the placebo row) and
  high-budget row `capped` (tables above).

## Recommendation for the shipped presets

| Setting | `standard` | `fast` | Evidence |
| --- | --- | --- | --- |
| `horizon_turns` | 2 (keep the reference's) | 2 | three turns: -0.075 (reference `standard` budget), -0.030 (high budget); four turns: null at 1.9x the applies |
| `adaptive_horizon` | on | on, if its ~37% applies fit `fast`'s throughput goal | +0.037 [+0.000, +0.075] confirmed at the reference `standard` budget; +0.015 (+0.025 against the placebo) at twice the shipped `standard` budget; never negative |
| `generalized_leaf` | off | off | null to slightly negative alone; needed only when the longest line passes four turns |
| `capped_rollout_continuation` | on (a guard; it never fires on the panel and costs nothing then) | on | 0 capped rollouts in every row; rows identical to the placebo |

Context for the lead: the P6-A mechanisms move the win rate by about two points at most. They do
not by themselves close the gap to E1 (pooled lower bound 0.55 against `ref:strong`); the
`adaptive_horizon` applies (about +35% per game) also reduce the budget headroom under the 2x
floor, which the lead's §12.8 tuning trades against iterations. The budget curve
(`docs/port/eval/lead/budget-curve-standard-vs-ref-strong-4c1024d.json`) gains little past 2x,
so spending the applies on the extension rather than on more iterations is reasonable, but it is
not measured head to head here. The refit of the evaluation (P6-G, W-34) at the final leaf point
should be done with the adopted horizon (2, plus the adaptive extension).

## Interface notes (for the lead)

- **The leaf check at construction.** `mech::validate` takes only the flags, so the W-02 check
  runs at the start of every decision (`Composed::root_moves`, an `EngineError::Value` that fails
  the game loudly). To refuse a bad configuration when it is parsed, `StrengthenedConfig::
  check_mechanisms` (`presets.rs`, lead-owned) can also call `crate::mech::p6a::check(self)`
  (one line).
- **`gcg-eval`'s horizon rule.** `agent_spec::check_search` applies the reference rule
  `WIN_DECAY * horizon_turns < LEAF_FLOOR` to every `search:` spec, so `horizon_turns >= 5` is
  refused even with `generalized_leaf`, which covers it. W-02's plan gives the strengthened search
  its own check; `check_search` could skip the reference rule when `generalized_leaf` is on and
  rely on `check_mechanisms` with the call above. Not needed for the recommended presets.
- **Counters.** P6-A counts extended lines, capped lines and lines still unsettled after their
  continuation (`p6a::DecisionState`), but `MechanismEffort` (`contracts.rs`, lead-owned) has no
  fields for them, so only the unit tests read them; reports show the cost through applies per
  game and `rollouts_capped`. Three `u64` fields would surface them.
- **Composition with other packages.** P6-B (`threat_leaf`) and P6-G (evaluation) will also
  override `leaf_value`: `p6a::leaf_value` keeps the leaf mapping and the capped continuation
  separate from the evaluation (`value_of`), so the lead can route their estimate through
  `LeafMapping::estimate`. P6-H's pre-game simulations score their lines through
  `Composed::leaf_value`: with `capped_rollout_continuation` on, a simulation that hit P6-H's
  decision cap is continued to a main-phase decision (or scored 0.5) before P6-H sees its value;
  with `generalized_leaf` on, a finished simulation longer than `max_turns` is shaded as one of
  `max_turns` turns (the clamp of W-02).
- **Leave-one-out rows** need the lead's bundle (§12.9 step 4); not run here.

## Gate

Run after the last code edit (logs not committed):

| Command | Result |
| --- | --- |
| `cargo fmt --all --check` | clean |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | clean |
| `cargo test --workspace --release` | 1,150 passed, 0 failed, 0 ignored (131 test result lines), including `tests/null_config.rs` (4), `tests/null_config_traces.rs`, the 10 P6-A unit tests and the 3 W-01 scenario tests |
| `cargo test --release -p gcg-search --features slow` | 19 targets, all pass (78 unit tests with the `slow` ones, including `fitted_policy_reproduces`; `null_config_search_set`) |
| `target/release/gcg-diff corpus --tier 1 --require-all` | 3804/3804 traces (`baselines` 8/8, `coverage` 50/50, `full` 4/4, `fuzz` 100/100, `search` 9/9, `tests` 3213/3213, `vanilla` 40/40, `views` 280/280, `worlds` 100/100) |
| `COMPARE_OUT=build/p6a/compare scripts/compare_python_vs_rust.sh equivalence --matches 2 --seeds 1 --formats bo1 --pairs red-green-zeon:blue-white-federation --workers 1 --decision-logs off` | 3/3 IDENTICAL (`fast`, `standard`, `strong`) |
| `uv run ruff check docs/port/eval/P6-A/` and `uv run ruff format --check docs/port/eval/P6-A/` | clean |

## Not done

- Leave-one-out rows from the full bundle (lead, §12.9 step 4).
- The refit of the evaluation at the new leaf point (W-01 plan item (4)): P6-G's (W-34).
- The construction-time leaf check and `gcg-eval`'s horizon rule for `generalized_leaf`
  (interface notes): lead-owned files.
- The Rust `docs/AI.md` text for the cap and the continuation (W-15's documentation): lead-owned;
  the facts are in W-15 above.
- No variant of the extension rule (race threshold, a deterministic extended turn, two
  boundaries) was tuned: at 400 games per row their differences would be far below the noise.
