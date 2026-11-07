# P6-B weakness status: the tactical solver (S3)

Package P6-B (docs/dev/PLAN.md §12.2, §12.3 S3): W-03, W-16, W-17, W-32, W-38. Written by the
P6-B implementer on 2026-11-02 in worktree branch `worktree-wf_94545b98-29a-2`, from `8af9b0e`
(M13). WEAKNESSES.md itself is not edited here (the lead records the status lines from this
file, §12.10). Default-on in the shipped presets is the lead's decision; the recommendation is
at the end.

## What was built

All six P6-B flags of `MechanismFlags` are implemented (`p6b::PROVIDES`). With every P6-B flag
off, `Composed` calls exactly what it called before (the P6-E or reference root moves and
budget, the P6-F or reference rollout policy, the reference leaf), so the null configuration
still replays the reference bit for bit (`tests/null_config.rs`, `tests/null_config_traces.rs`,
`tests/null_config_search_set.rs` with `slow`: unchanged and passing).

| File | Content |
| --- | --- |
| `crates/gcg-search/src/mech/p6b/mod.rs` | `PROVIDES`, `any_on`, `changes_leaf`, the per-decision state (`DecisionState`: solver applies of rollouts and leaves, their cap, root applies; `for_tree` gives each S1 root tree `1/K` of the cap), `rollout_lethal` (`tactical_solver`), `critical` and `budget` (`critical_extension`), the constants `SOLVER_APPLIES_PER_ITERATION` = 16, `CRITICAL_BUDGETS` = 2, `LINE_LIMITS` (400 applies, attacks only) |
| `mech/p6b/solver.rs` | the engine-backed AND-OR solver of the rest of one turn on a determinised world: `lethal_line` (a lethal line starting with one of given first moves, iterative deepening over setup plays), `forced_win` (the value of a state for the turn player), `first_moves`, `Limits`, `Verdict` |
| `mech/p6b/board.rs` | the static attack step: `Board::of` (attackers from the ATTACK options or a later turn, engine-correct Blockers per attacker through `procedures::eligible_blockers` on a hypothetical battle, Base HP, Shields, <Suppression>, <Breach>, 【Attack】 abilities), `Board::lethal` (the exact solution of the effect-free attack step by memoised AND-OR over attacker and Blocker masks), `Board::may_be_lethal` (the solver's gate), `effective_blockers`, `static_lethal`, `race` (the strengthened race of W-38) |
| `mech/p6b/root.rs` | `lethal_root`: the searcher's lethal proved in each of the first K = 4 search worlds (`p6e::worlds::Worlds`, built through the composed `Mechanisms::world`) is the only root move left; during the opponent's turn the moves after which the opponent has a forced lethal in every one of the K worlds are dropped unless every move is. `LETHAL_LIMITS` (3,000 applies per world, one setup play, own action-step plays branched), `DEFENCE_LIMITS` (1,500 per move and world), `ROOT_APPLY_CAP` 24,000 |
| `mech/p6b/leaf.rs` | `threat_leaf` (a proved lethal of the leaf's turn player gives the time-shaped terminal value), the feature leaf of `effective_blockers` and `solver_can_kill`, and the feature functions `can_kill` and `race_feature` for P6-G |
| `mech/p6b/tests.rs` | 12 unit tests (below) |
| `mech/p6b/audit.rs` (feature `slow`) | W-17 step (4): the regenerated audit games and their missed lethals |
| `mech/p6b/can_kill_census.rs` (feature `slow`) | W-38: the `can_kill` census on 10,000 positions |
| `crates/gcg-search/tests/scenarios/p6b_support.rs`, `p6b_w03_last_blocker.rs`, `p6b_w16_fast_lethal.rs`, `p6b_w32_cant_block.rs` | the scenario tests |
| `docs/dev/eval/P6-B/` | `rows/*.json` (the ablation reports, every game included), `rows_table.py` (the table below, with the placebo comparison; deleted, at commit 83a06c8), `seed-ledger.json` |

Wiring outside `mech/p6b/` (flagged to the lead):

- `crates/gcg-search/src/mech/mod.rs` (the p6b lines): `pub(crate) mod p6b;`; the `p6b` field of
  `Composed` and of `ComposedState` (moved in `with_state`/`into_state`, and `tree_state` gives a
  root tree `p6b.for_tree(K)`); `aux_applies` adds P6-B's rollout and leaf solver applies and
  `root_applies` its root applies; `root_moves` runs P6-E's (or the reference's) root moves, then
  `p6b::root::root_moves` when `lethal_root` is on; `budget` runs P6-E's (or the reference's)
  budget, then `p6b::budget` when any P6-B flag is on (it sets the solver cap; with
  `critical_extension` it may raise the cap); `rollout_choice` asks `p6b::rollout_lethal` first
  when `tactical_solver` is on, then P6-F's (or the reference's) policy; a `leaf_value` override
  calls `p6b::leaf::leaf_value` when a P6-B leaf flag is on (`threat_leaf`, `effective_blockers`,
  `solver_can_kill`), otherwise the reference leaf; `available` reads `p6b::PROVIDES`. When P6-A
  or P6-G also override the leaf, the lead composes them: `threat_leaf` replaces the value only
  for a proved lethal and falls back to whichever leaf is otherwise in force.
- `crates/gcg-search/tests/scenarios/main.rs`: the `mod p6b_*` lines (the lead unions the
  packages' lines).
- For P6-G: `p6b::leaf::can_kill(st, attacker, exact)` (exact: the static attack step; otherwise
  the reference's count rule with engine-correct attackers and Blockers), `p6b::leaf::race_feature`
  (`1 / board::race`), `p6b::board::static_lethal`, `p6b::board::effective_blockers` and
  `p6b::board::race` are `pub(crate)`, so a refit can use them as features.

### The solver

`solver.rs` searches the rest of one player's turn on a determinised world, played by the engine
on clones (`gcg_ai::apply_unchecked`). The turn player (the attacker) wins a line when the
defender is defeated before the turn ends. At the attacker's MAIN decisions the search tries
every attack on the player (one per class of interchangeable attackers, `merge_equivalent` over
`canonical`) and, while the line has setup plays left, every Command, ability, pairing and Unit
play (W-17's deploy, pair and removal orders); ending the main phase never wins. The defender's
BLOCK decisions are answered by every option (the engine's own eligible Blockers, so CANT_BLOCK,
NO_BLOCKER_VS, <High-Maneuver>, CANT_BE_BLOCKED and rested Blockers are the engine's, W-32) and
its ACTION_STEP decisions by every play and the pass (its determinised tricks); the attacker's
own SELECT and YES_NO decisions (its Commands' and 【Attack】 abilities' targets) are branched,
its action steps are branched or passed by `Limits::own_tricks`. Every other decision (the
defender's targets and Bursts, trigger order) takes the reference policy's best option without a
random draw, so the solver is deterministic. Shields, Bursts, the Base and its HP, AP and First
Strike come from playing the line. A line that reaches the apply bound, or whose apply raises an
engine error other than an unimplemented card, is unknown; the solver claims a lethal only when
every defender reply it explored loses. The bound is in engine applies, every one counted.

The static attack step (`board.rs`) gates the solver: in rollouts and at leaves the solver runs
only when `Board::may_be_lethal` holds (the exact static game, or, when the attacker's side has
an 【Attack】 ability, the hits it can land with no Blocker at all); at the root the gate is the
static bound with no Blocker plus one hit when a setup play is available, and outside the MAIN
decision the hits of the rest of the turn with no Blocker.

## Ablation rows (PLAN §12.4)

All rows ran with one `gcg-eval` build of this package's code (its worktree head `941c71b`,
integrated at M14 and squashed into commit 83a06c8; later edits touched test code only), 400 games per row (1,600 for the confirmations) in mirrored blocks of
four, stratified round-robin over the 15 tuning pairs of `docs/dev/eval/panel.json`, forced
play/draw (`--choose-first seat0`, A24), `--workers 10`.
The machine was shared with other packages' runs (load average 40 to 100 on 10 cores), which
changes wall times only.

**(a) Individual rows** at the reference `standard` budget (the budget the `fast` preset starts
from): `gcg-eval ablation individual --mechanism <flag>=true --reference standard --games 400
--seed 20200100 --role dev --ledger docs/dev/eval/P6-B/seed-ledger.json --workers 10
--choose-first seat0` (challenger `search:null-standard:<flag>=true`, opponent
`ref:standard`). The placebo row runs `--mechanism engine_error_leaf=false` (the null
configuration, which replays the reference: it measures the noise of a row whose agents draw
different decision seeds). All individual rows share master 20200100, so game i has the same
deal, seats and play/draw in every row and the challenger always has the same decision seeds;
the "vs placebo" column (`docs/port/eval/P6-B/rows_table.py`) compares each row with the placebo
game by game, which isolates the mechanism. This master favours the challenger: the placebo
itself scores 211/400 (+0.055), so the individual rows are read against the placebo, not
against 0.5. `p6b_solver` is `tactical_solver` + `threat_leaf` + `lethal_root`; `p6b_all` is all
six flags.

| Row | wins/N (rate, Wilson 95%) | paired difference (Newcombe 95%) | discordant | iterations / applies per game (challenger vs opponent) | vs placebo: changed games; row-only / placebo-only wins; difference [95%] | evidence |
| --- | --- | --- | --- | --- | --- | --- |
| `placebo` | 211/400 (0.527, [0.478, 0.576]) | +0.055 [-0.012, +0.121] | 29/18 | 9837 vs 9653 / 117942 vs 116077 | — | `docs/dev/eval/P6-B/rows/dev-placebo-400.json` |
| `tactical_solver` | 212/400 (0.530, [0.481, 0.578]) | +0.060 [-0.007, +0.126] | 30/18 | 9934 vs 9740 / 131226 vs 116345 | 215; 11 / 10; +0.0025 [-0.0200, +0.0250] | `docs/dev/eval/P6-B/rows/dev-tactical_solver-400.json` |
| `lethal_root` | 210/400 (0.525, [0.476, 0.574]) | +0.050 [-0.017, +0.117] | 29/19 | 9162 vs 9660 / 113513 vs 116178 | 81; 1 / 2; -0.0025 [-0.0110, +0.0060] | `docs/dev/eval/P6-B/rows/dev-lethal_root-400.json` |
| `threat_leaf` | 205/400 (0.512, [0.464, 0.561]) | +0.025 [-0.044, +0.094] | 28/23 | 9659 vs 9649 / 129095 vs 116530 | 229; 12 / 18; -0.0150 [-0.0418, +0.0118] | `docs/dev/eval/P6-B/rows/dev-threat_leaf-400.json` |
| `critical_extension` | 210/400 (0.525, [0.476, 0.574]) | +0.050 [-0.020, +0.119] | 31/21 | 12565 vs 9611 / 150118 vs 115744 | 169; 6 / 7; -0.0025 [-0.0202, +0.0152] | `docs/dev/eval/P6-B/rows/dev-critical_extension-400.json` |
| `effective_blockers` | 207/400 (0.517, [0.469, 0.566]) | +0.035 [-0.032, +0.101] | 27/20 | 9795 vs 9700 / 117279 vs 115982 | 210; 20 / 24; -0.0100 [-0.0425, +0.0225] | `docs/dev/eval/P6-B/rows/dev-effective_blockers-400.json` |
| `solver_can_kill` | 203/400 (0.507, [0.459, 0.556]) | +0.015 [-0.053, +0.083] | 26/23 | 9683 vs 9718 / 116037 vs 116349 | 254; 22 / 30; -0.0200 [-0.0553, +0.0153] | `docs/dev/eval/P6-B/rows/dev-solver_can_kill-400.json` |
| `p6b_solver` | 214/400 (0.535, [0.486, 0.583]) | +0.070 [-0.000, +0.139] | 33/19 | 8992 vs 9736 / 133381 vs 117216 | 311; 17 / 14; +0.0075 [-0.0198, +0.0348] | `docs/dev/eval/P6-B/rows/dev-p6b_solver-400.json` |
| `p6b_all` | 197/400 (0.492, [0.444, 0.541]) | -0.015 [-0.087, +0.057] | 26/29 | 10978 vs 9685 / 169408 vs 115465 | 362; 24 / 38; -0.0350 [-0.0734, +0.0034] | `docs/dev/eval/P6-B/rows/dev-p6b_all-400.json` |

**(b) High-budget rows**: the shipped `search:standard` at twice its budget
(`iterations_per_option=256,min_iterations=640,max_iterations=2560`, four times the reference
`standard` budget, with `engine_error_leaf`) with the mechanism against the same without it:
`gcg-eval ablation leave-one-out --bundle
search:standard:iterations_per_option=256,min_iterations=640,max_iterations=2560,<flag>=true
--mechanism <flag>=false --games 400 --seed 20200200 --role dev --ledger
docs/dev/eval/P6-B/seed-ledger.json --workers 10 --choose-first seat0`. Both agents are the
same search apart from the flag and play the same deals, so the row's own paired difference is
the mechanism's effect.

| Row | wins/N (rate, Wilson 95%) | paired difference (Newcombe 95%) | discordant | iterations / applies per game (with vs without) | evidence |
| --- | --- | --- | --- | --- | --- |
| `tactical_solver` | 213/400 (0.532, [0.483, 0.581]) | +0.065 [-0.007, +0.136] | 34/21 | 32178 vs 32431 / 404687 vs 375681 | `docs/dev/eval/P6-B/rows/hb-tactical_solver-400.json` |
| `lethal_root` | 206/400 (0.515, [0.466, 0.564]) | +0.030 [-0.040, +0.100] | 29/23 | 29816 vs 32180 / 357828 vs 373833 | `docs/dev/eval/P6-B/rows/hb-lethal_root-400.json` |
| `threat_leaf` | 197/400 (0.492, [0.444, 0.541]) | -0.015 [-0.092, +0.062] | 30/33 | 32062 vs 32731 / 421183 vs 381468 | `docs/dev/eval/P6-B/rows/hb-threat_leaf-400.json` |
| `critical_extension` | 206/400 (0.515, [0.466, 0.564]) | +0.030 [-0.042, +0.101] | 30/24 | 40527 vs 32284 / 463948 vs 375353 | `docs/dev/eval/P6-B/rows/hb-critical_extension-400.json` |
| `effective_blockers` | 202/400 (0.505, [0.456, 0.554]) | +0.010 [-0.068, +0.087] | 33/31 | 32129 vs 32499 / 372538 vs 379164 | `docs/dev/eval/P6-B/rows/hb-effective_blockers-400.json` |
| `solver_can_kill` | 196/400 (0.490, [0.441, 0.539]) | -0.020 [-0.096, +0.057] | 29/33 | 32063 vs 32600 / 370883 vs 379262 | `docs/dev/eval/P6-B/rows/hb-solver_can_kill-400.json` |
| `p6b_solver` (the three) | 207/400 (0.517, [0.469, 0.566]) | +0.035 [-0.037, +0.107] | 31/24 | 29450 vs 32569 / 428270 vs 378846 | `docs/dev/eval/P6-B/rows/hb-p6b_solver-400.json` |

**(c) Confirmation** (once, 1,600 games, confirm-role master 30200200, the high-budget row's
command with `--games 1600 --seed 30200200 --role confirm`) of every row that looked promising:
a high-budget paired difference of at least +0.03 (`tactical_solver`, `lethal_root`,
`critical_extension`). No individual row was promising: the largest difference against the
placebo is +0.0075 (`p6b_solver`), and every individual row's Newcombe lower bound is below 0.

| Row | wins/N (rate, Wilson 95%) | paired difference (Newcombe 95%) | discordant | iterations / applies per game (with vs without) | evidence |
| --- | --- | --- | --- | --- | --- |
| `tactical_solver` | 811/1600 (0.507, [0.482, 0.531]) | +0.014 [-0.023, +0.050] | 116/105 | 32202 vs 32208 / 412290 vs 375393 | `docs/dev/eval/P6-B/rows/confirm-hb-tactical_solver-1600.json` |
| `lethal_root` | 791/1600 (0.494, [0.470, 0.519]) | -0.011 [-0.049, +0.026] | 114/123 | 30110 vs 32244 / 363616 vs 375795 | `docs/dev/eval/P6-B/rows/confirm-hb-lethal_root-1600.json` |
| `critical_extension` | 795/1600 (0.497, [0.472, 0.521]) | -0.006 [-0.044, +0.032] | 119/124 | 41036 vs 32213 / 475116 vs 375627 | `docs/dev/eval/P6-B/rows/confirm-hb-critical_extension-1600.json` |

Reading the tables:

- No row's interval excludes 0. The individual rows move at most ±0.02 against the placebo, and
  the high-budget rows' 400-game point estimates shrink toward 0 on confirmation
  (`tactical_solver` +0.065 to +0.014, `lethal_root` +0.030 to -0.011, `critical_extension`
  +0.030 to -0.006), as regression to the mean predicts for rows picked by their point
  estimate. The solver changes the decisions of a minority of positions (lethal races in the
  late game): `lethal_root` changed 81 of 400 games, `tactical_solver` 215.
- Pooling each high-budget row with its confirmation (2,000 games on two masters):
  `tactical_solver` 1024/2000 = 0.512, `lethal_root` 997/2000 = 0.4985, `critical_extension`
  1001/2000 = 0.5005.
- Cost per game (against the placebo's challenger, or the high-budget row's opponent):
  `tactical_solver` adds 8-11% applies (its rollout solves, capped at 16 applies per iteration
  of the cap) at unchanged iterations; `threat_leaf` adds 9-10% applies; `lethal_root` saves 7%
  of the iterations (a proved lethal is played without a search) and 3-4% of the applies;
  `critical_extension` adds 26-28% iterations; `effective_blockers` and `solver_can_kill` cost
  nothing measurable.
- The leaf features (`effective_blockers`, `solver_can_kill`) swap features under weights that
  were fitted for the reference's features (`ROLE_WEIGHTS`), so their rows measure the swap, not
  the features; their worth is for P6-G's refit (W-34), which can call `leaf::can_kill` and
  `leaf::race_feature`.

Kept (measured non-negative): `tactical_solver` (individual +0.0025 against the placebo,
high-budget +0.065, confirmed +0.014; with the W-03 scenario fix). Not kept:
`critical_extension` (null: -0.0025 against the placebo, +0.030 at the high budget, -0.006 on
confirmation, for 26-28% more iterations), `threat_leaf` (negative in both rows), `solver_can_kill` (negative in both rows),
`effective_blockers` (-0.010 against the placebo at the reference budget, +0.010 at the high
budget; a feature swap that P6-G's refit should judge), `lethal_root` (null: -0.0025 against
the placebo, +0.030 at the high budget, -0.011 on confirmation; it carries the W-16, W-17 and
W-32 scenario fixes and saves iterations, so it stays available for the lead's bundle and its
leave-one-out row). Every flag stays implemented and available.

Leave-one-out rows need the full strengthened bundle, which the lead assembles (§12.9 step 4);
they are not run here.

## Tests

Unit tests (`mech::p6b::tests`, all passing):

| Test | What it shows |
| --- | --- |
| `the_package_provides_its_six_flags` | `PROVIDES` is the six P6-B fields of `MechanismFlags` |
| `the_static_attack_step_counts_hits_blockers_bases_and_keywords` | exposed player, active and rested Blockers, two Shields with 2/3/4 attackers and a Blocker, <High-Maneuver>, <Suppression> (two Shields per hit), a 3-HP Base against 2 and 3 GMs |
| `engine_blockers_exclude_cant_block_where_the_reference_counts_it` | W-32: after Girty Lue's trigger (GD05-127, CANT_BLOCK this turn) the reference's `values.ready_blockers` still lists Demi Trainer and its `can_kill_turn_player` is 0 (pinned); `effective_blockers` is empty, the static step, the count rule with engine Blockers and the solver all see the lethal |
| `the_strengthened_race_skips_a_blocker_that_stays_rested_this_turn` | W-38: two GMs, one enemy Shield and a rested Demi Trainer: the reference race counts the rested Blocker (2 turns, `inv_race_turn_player` 0.5, pinned), the strengthened race does not (1 turn); with the Blocker active both say 2 |
| `the_solver_finds_lethal_and_proves_its_absence` | lethal and no-lethal boards; a bound too small is `Unknown`, never a claim |
| `the_solver_plays_a_removal_before_the_attack` | `test_lethal_rests_the_blocker_before_attacking`'s position: attacks alone are blocked, the setup search plays Siege Ploy first |
| `the_defenders_action_step_trick_holds_and_is_explored` | the defender's Battle of Aces in the action step destroys the damaged attacker: lethal without the trick, none with it |
| `forced_win_reads_the_defenders_blocks` | the opponent's forced lethal against one Blocker: 1 attacker holds, 1 against a rested Blocker wins, 2 win |
| `a_leaf_with_a_proved_lethal_takes_the_time_shaped_terminal_value` | `threat_leaf`: `1 - 0.01·2` for the searcher's lethal two turns after the root, `0.01·2` for the opponent's; no lethal keeps the reference leaf bit for bit |
| `the_feature_leaf_changes_only_the_can_kill_and_race_features` | the `solver_can_kill` and `effective_blockers` leaves raise the estimate of the open lethal; other P6-B flags keep the reference leaf bit for bit |
| `critical_decisions_get_two_extra_budgets_and_root_trees_share_the_solver_cap` | `critical_extension`: cap 480 for a 160-iteration critical decision, 160 otherwise; the solver cap; `for_tree(4)` |
| `a_spent_solver_budget_leaves_rollouts_and_leaves_to_the_reference` | the solver cap is a count; once spent the leaf is the reference's bit for bit |

Slow tests (feature `slow`, `cargo test --release -p gcg-search --features slow`):
`mech::p6b::audit::every_real_missed_lethal_of_the_regenerated_audit_games_is_found` (W-17,
76 s) and
`mech::p6b::can_kill_census::the_static_can_kill_agrees_with_an_exhaustive_search_more_often_than_the_reference`
(W-38, 8 s), below.

Scenario tests (`cargo test --release -p gcg-search --test scenarios p6b`), all passing:
`p6b_w03_last_blocker`, `p6b_w16_fast_lethal`, `p6b_w32_cant_block` (counts in the items).

## Items

### W-03 — Lethal-threat defence: keep the last Blocker (`tactical_solver`, `threat_leaf`, `lethal_root`)

- **Status:** resolved (scenario reproduced and fixed; game-level rows below).
- **Resolution as implemented:** no rollout-wide threat settings (the plan's first rule). The
  threat-aware leaf of the plan is `threat_leaf`: when the leaf is the turn player's MAIN
  decision and the solver proves a lethal line on the leaf's world (attacks only; every Blocker,
  every defender action-step play explored; 400 applies), the leaf takes that player's terminal
  value at the leaf's turn with the reference's time shaping (the loss value when it is the
  searcher's opponent). With the reference horizon (two turn boundaries) the leaf after a root
  decision of the searcher's turn is the searcher's next turn, so the opponent's reply turn is
  played by the rollouts; `tactical_solver` makes that turn exact where it matters: a rollout's
  turn player whose static attack step may be lethal plays the solver's proved lethal line
  (the opponent's determinised hand and Shields of the rollout world included) instead of the
  hand-written policy's move. `lethal_root`'s defence half drops, during the opponent's turn,
  the root moves after which the opponent wins the turn in every one of the first K worlds.
  Solver work is counted in applies (`applies_aux`, capped at 16 per iteration of the
  decision's cap).
- **Evidence:** the audited position (bwf vs pwt, match 1, decision 93) could not be
  regenerated: the Python audit's command and seed mapping are lost (W-17), and the squash
  commit `d9d41af` does not contain the testkit puzzle that rebuilt it. The scenario
  `tests/scenarios/p6b_w03_last_blocker.rs` rebuilds its shape with the testkit: the searcher
  has two Shields, two rested Pisces and its last Blocker, Messala, which can break the
  opponent's last Shield; the opponent's three active GMs are lethal on its turn exactly when
  Messala is rested. 8 agent seeds at the reference `standard` budget, against the reference
  `standard` defender: Messala kept by the reference 0/8 (it trades the Blocker 8/8, pinned), by
  the null configuration 0/8, by `tactical_solver` 8/8 (bar ≥ 7/8): pass. Root values at agent
  seed 1 (`choose_traced`): the null configuration, which searches as the reference does, rates
  the attack 0.885 (81 visits) against ending the turn 0.829; with `tactical_solver` the attack
  is 0.010 (a certain loss) and ending the turn 0.834. Unit tests
  `forced_win_reads_the_defenders_blocks` and
  `a_leaf_with_a_proved_lethal_takes_the_time_shaped_terminal_value`.
- **Ablation** (tables above): `tactical_solver` individual 212/400 = 0.530 [0.481, 0.578],
  paired +0.060 [-0.007, +0.126], discordant 30/18, against the placebo +0.0025 [-0.0200,
  +0.0250]; high budget 213/400 = 0.532 [0.483, 0.581], +0.065 [-0.007, +0.136], 34/21;
  confirmed 811/1600 = 0.507 [0.482, 0.531], +0.014 [-0.023, +0.050], 116/105; 8-11% more
  applies per game. `threat_leaf` individual 205/400, against the placebo -0.0150 [-0.0418,
  +0.0118]; high budget 197/400 = 0.492, -0.015 [-0.092, +0.062]. `lethal_root`: see W-17.
  The three together (`p6b_solver`): individual +0.0075 against the placebo, high budget 207/400
  = 0.517, +0.035 [-0.037, +0.107]. Adoption needed more than the Python measurement's null
  449/448: `tactical_solver` is non-negative in all three rows (kept), `threat_leaf` negative in
  both (not kept).

### W-16 — Budget fixed by option count (`critical_extension`; lethal at a fast-sized budget by `lethal_root`)

- **Status:** resolved (scenario reproduced and fixed by `lethal_root`; `critical_extension`
  implemented and measured).
- **Resolution as implemented:** `critical_extension`: a MAIN, BLOCK or ACTION_STEP root decision
  is critical when the static attack step is lethal for either side (the searcher's attack step
  now when it is the turn player, else its next turn; the opponent's rest of turn when it is the
  turn player, else its next turn against the searcher's Blockers as they are now). A critical
  decision's cap becomes three budgets, and the core's close-call rule (`ai/mcts.py:131-150`,
  `target_extension_z` paired standard errors) extends it one budget at a time while the top two
  moves are within z. Deterministic and counted in iterations. The extension alone does not
  solve the scenario below; the proved lethal of `lethal_root` does, at the same budget.
- **Evidence:** scenario `tests/scenarios/p6b_w16_fast_lethal.rs`: four GMs against two Shields,
  one Demi Trainer <Blocker> and two rested Pisces a GM destroys for free; all four attacks must
  go to the player. 16 agent seeds, the turn played out against the reference defender of the
  same preset: reference `fast` 0/16 (pinned; bar ≤ 8), reference `standard` 5/16,
  `fast` + `critical_extension` 0/16, `fast` + `lethal_root` 16/16 (bar ≥ 14): pass. Unit test
  `critical_decisions_get_two_extra_budgets_and_root_trees_share_the_solver_cap`.
- **Ablation** (tables above): individual 210/400 = 0.525 [0.476, 0.574], paired +0.050
  [-0.020, +0.119], discordant 31/21, against the placebo -0.0025 [-0.0202, +0.0152], at 12,565
  iterations per game against the placebo's 9,837 (+28%); high budget 206/400 = 0.515 [0.466,
  0.564], +0.030 [-0.042, +0.101], 30/24, at 40,527 iterations against 32,284 (+26%);
  confirmed 795/1600 = 0.497 [0.472, 0.521], -0.006 [-0.044, +0.032], 119/124, at 41,036
  iterations against 32,213 (+27%). Null for a quarter more iterations: not kept by default
  (available). W-16's critical-position extension is therefore resolved by the measurement and
  this decision; the lethal puzzle at the fast budget is solved by `lethal_root`.

### W-17 — Implausible plays and missed lethals (`lethal_root`; the audit's kinds)

- **Status:** partly resolved. Step (1), the exact this-turn lethal solver at the root, is
  implemented (`lethal_root`), with scenario evidence; step (4), the regenerated audit games, is
  done (no real missed lethal recovered); steps (2) (the `gcg-eval weakness` suite) and (3) (a
  larger blinded audit with human or agent auditors) are not P6-B files and are **not done**
  here (handed to the lead).
- **Resolution as implemented:** `root.rs`: during the searcher's turn, when the static board
  leaves lethal possible, the solver looks for a lethal line in world 0 that starts with a root
  move (an attack on the player, or a Command, ability, pairing or Unit play followed by
  attacks, with the attacker's own targets branched: the deploy, pair and removal orders), and
  checks the line's first move in worlds 1..K (K = 4, the search's own first worlds). A first
  move lethal in every world is the only root move left, so it is played at once (the "plays a
  found lethal" option of the plan; no dominant prior is needed). Each later decision of the
  line solves again.
- **Evidence:**
  - WK1 lethal detection: W-16's four-attack puzzle (reference `standard` 5/16, `fast` 0/16,
    `lethal_root` 16/16) and W-32's reproducer (reference `standard` 0/16, `lethal_root` 16/16);
    unit test `the_solver_plays_a_removal_before_the_attack` (the deploy/removal order). WK2
    lethal avoidance: W-03's scenario (reference keeps the Blocker 0/8, `tactical_solver` 8/8).
  - Step (4) (`mech::p6b::audit`, feature `slow`): 15 regenerated BO1 games of the reference
    `standard` on both sides, five per example pairing, masters 90031 (rgz vs bwf), 90032 (bwf vs
    pwt), 90033 (pwt vs rgz); game `g` of master `m` is `new_game(decks, derive_seed(m, "game",
    g))` with agents `derive_seed(m, "agent", g, p)` (a reconstruction: the lost audit's command
    and seed mapping are unknown). At each of the turn player's MAIN decisions the solver ran
    from the decider's information (K = 4 determinisations of the information-set state). 31
    decisions had a lethal proved in every world; the reference's line won that turn in all but
    one (90032:0, turn 15, player 1). That one is not lethal on the true state (the solver on the
    true state finds none: the decider's worlds did not hold the opponent's real cards), and both
    the reference and `lethal_root` win it 2/16 from the frozen state. So the regenerated games
    contain **no real missed lethal**: the number recovered is 0 against the audit's two, and the
    lethal bar rests on the W-16 and W-32 puzzles, as the entry foresees.
- **Ablation** (tables above): `lethal_root` individual 210/400 = 0.525 [0.476, 0.574], paired
  +0.050 [-0.017, +0.117], discordant 29/19, against the placebo -0.0025 [-0.0110, +0.0060] (81
  games changed, 1 / 2 wins); high budget 206/400 = 0.515 [0.466, 0.564], +0.030 [-0.040,
  +0.100], 29/23; confirmed 791/1600 = 0.494 [0.470, 0.519], -0.011 [-0.049, +0.026], 114/123.
  It saves 7% of the iterations per game. Null at game level: not kept by default (available).
  The audit's implausible-play count per game (step 3) is not reported: no audit was run.

### W-32 — AI Blocker counts ignore CANT_BLOCK and NO_BLOCKER_VS (`effective_blockers`)

- **Status:** resolved.
- **Resolution as implemented:** `board::effective_blockers(st, attacker, defender)` calls the
  engine's `procedures::eligible_blockers` on a hypothetical attack of `attacker` on the player
  (rules 8-3-1..8-3-3, 13-1-4, 13-1-6: CANT_BLOCK, the attacker's NO_BLOCKER_VS rules,
  <High-Maneuver>, CANT_BE_BLOCKED, rested Units). The solver plays the engine's own block
  options and the static attack step reads its Blockers from that helper; with
  `effective_blockers` the leaf evaluation's `can_kill` count rule and the race use it too. The
  reference keeps `values.ready_blockers` and `_race` as written (pinned by the unit tests). The
  rollout policy's block-risk scoring is P6-F's (`gcg_ai::policy`); it can call the same helper.
  No card of the packaged pool grants NO_BLOCKER_VS (the rule kind appears only in the IR's
  `repr` vectors), so it reaches the strengthened search only through the engine's own
  `eligible_blockers` (`crates/gcg-engine/src/battle.rs:127-180`), which the helper calls.
- **Evidence:** unit test `engine_blockers_exclude_cant_block_where_the_reference_counts_it`
  (CANT_BLOCK from Girty Lue GD05-127; reference counts pinned). Scenario
  `tests/scenarios/p6b_w32_cant_block.rs`, 16 agent seeds at the reference `standard` budget:
  (a) one enemy Shield, Gaia and a GM must both hit: reference `standard` 16/16, reference `fast`
  16/16, `lethal_root` + `effective_blockers` 16/16 (the reference already solves it: the
  entry's no-regression check); (b) three enemy Shields and two rested Pisces, Gaia and three
  GMs must all hit: reference `standard` 0/16 (pinned; the reproducer bar ≤ 8), reference `fast`
  0/16, strengthened 16/16 (bar 16/16): pass.
- **Ablation** (tables above): `effective_blockers` individual 207/400 = 0.517 [0.469, 0.566],
  paired +0.035 [-0.032, +0.101], discordant 27/20, against the placebo -0.0100 [-0.0425,
  +0.0225]; high budget 202/400 = 0.505 [0.456, 0.554], +0.010 [-0.068, +0.087], 33/31. The row
  measures the leaf features under `ROLE_WEIGHTS` (fitted to the reference's counts); mixed and
  null, so not kept by default; the helper is what the solver and P6-G use.

### W-38 — can_kill and race features are crude board heuristics (`solver_can_kill`, `effective_blockers`)

- **Status:** resolved (features implemented, unit-tested and measured; the refit that would
  give them their own weights is P6-G's, W-34).
- **Resolution as implemented:** `solver_can_kill`: the leaf's `can_kill_turn_player` is the
  exact static attack step of the turn player now and `can_kill_other` the other player's next
  turn (`board::static_lethal`: engine-correct attackers from the ATTACK options or the Units
  that can attack, their AP, <Suppression>, <Breach>, the Base's HP and Shields, Blockers by
  engine eligibility per attacker, solved exactly). `effective_blockers`: the reference's count
  rule with those attackers and Blockers, and the race of `board::race`: in the attacker's first
  turn only the Units that can attack then, against the Blockers active now; from its second
  turn every Unit with AP and every <Blocker> without CANT_BLOCK, rested now or not; the Base at
  the mean connecting AP, then Shields and one more hit. Both keep `ROLE_WEIGHTS`; a fitted race
  transform needs P6-G's refit, which can call `leaf::can_kill` and `leaf::race_feature`.
- **Evidence:**
  - `can_kill` census (`mech::p6b::can_kill_census`, feature `slow`, dev master 20200300): 10,000
    turn-player MAIN positions from 231 random-agent and greedy games on the example decks,
    compared with an exhaustive search of the attack lines on the true state (the solver, attacks
    only, every Blocker and defender action-step reply, 20,000 applies; none reached the bound).
    1,403 positions are lethal. The static attack step disagrees on 61 (0.61%: 59 false lethals
    from Bursts, tricks and effects the static game does not play, 2 missed through 【Attack】
    effects); the reference's `can_kill` on 694 (6.94%).
  - Unit test `the_strengthened_race_skips_a_blocker_that_stays_rested_this_turn` (the plan's
    second unit test; reference pinned).
- **Ablation** (tables above): `solver_can_kill` individual 203/400 = 0.507 [0.459, 0.556],
  paired +0.015 [-0.053, +0.083], discordant 26/23, against the placebo -0.0200 [-0.0553,
  +0.0153]; high budget 196/400 = 0.490 [0.441, 0.539], -0.020 [-0.096, +0.057], 29/33: negative
  under `ROLE_WEIGHTS`, not kept. The exact feature disagrees with the true attack step 11
  times less often than the reference's (census above), but its weights must come from a refit
  (P6-G, W-34); `effective_blockers` as in W-32.

## Recommendation for the shipped presets

- **`standard`** (strength subject to the 2x time floor): enable `tactical_solver`. It is the
  only P6-B mechanism that measured non-negative in every row (individual +0.0025 against the
  placebo; at twice the shipped budget +0.065 at 400 games and +0.014 [-0.023, +0.050] at 1,600,
  1024/2000 pooled), it fixes the W-03 scenario (the last Blocker kept 8/8 where the reference
  trades it 8/8), and it costs 8-11% more applies at the same iterations, which the lead's
  budget tuning (§12.8) has to charge to the time floor. Its game-level gain is small and not
  significant; it is not a source of the several points E1 needs.
- **`fast`** (throughput subject to being at least as strong as the reference `standard`
  budget): enable none by default. At the reference `standard` budget every P6-B row is within
  ±0.02 of the placebo; `tactical_solver` (+0.0025) costs 11% more applies for no measured gain.
  `lethal_root` is the one candidate on throughput grounds (7% fewer iterations and 3-4% fewer
  applies per game, the W-16 lethal at the fast budget 0/16 → 16/16, W-32's reproducer 0/16 →
  16/16) but measured -0.0025 against the placebo and -0.011 on the high-budget confirmation, so
  by the non-negative rule it stays off unless the lead's bundle leave-one-out row says
  otherwise.
- **Not recommended:** `threat_leaf`, `critical_extension`, `effective_blockers`,
  `solver_can_kill` in either preset (null or negative, `critical_extension` at +27%
  iterations). The leaf features should be judged by P6-G after a refit that uses
  `leaf::can_kill` (W-38's census: 0.61% disagreement with the true attack step against the
  reference's 6.94%) and `leaf::race_feature`.

## Seeds

Package-local ledger `docs/dev/eval/P6-B/seed-ledger.json` (the lead merges ledgers): dev
20200100 (individual rows), dev 20200200 (high-budget rows), confirm 30200200 (confirmations),
written by `gcg-eval --ledger`; dev 20200300 (the W-38 census, a unit test, added by hand).
The W-17 audit reconstruction uses the Python audit's masters 90031-90033, which lie in no role
range (they are the lost audit's seeds, reused only to regenerate its games for analysis).
