# GAP-A1 weakness status: W-05, W-14, W-17, W-25, W-26 (gcg-search AI evidence)

Package GAP-A1 of the residual-gap list (`docs/dev/eval/weakness-gaps.md`, section 2): the
open AI items W-05, W-14, W-17, W-25 and W-26. Written on 2026-11-03 in worktree branch
`worktree-wf_9f38bb2b-c03-1` from `1007ca9` ("Port WEAKNESSES: recorded outcome for all 164
items"). WEAKNESSES.md and `docs/dev/tests.json` are not edited here; the lead records the
status lines from this file (PLAN §12.10).

**Headline for the lead.** Two results need a decision:

1. **`kill_aware_targeting` measures positive inside the shipped bundle (W-26).** Its
   leave-one-out row from `B_core` at twice the pre-M15 `standard` budget was confirmed once on
   1,600 confirm-role games: 830/1600, paired difference **+0.037 [+0.001, +0.074]** (Newcombe
   lower bound above 0), discordant 129/99 (`docs/dev/eval/GAP-A1/rows/confirm-loo-kill_aware_targeting-bcore2x-1600.json`).
   It is not adopted here (the presets are frozen); adoption is the lead's.
2. **`tree_lethal_select` (W-25, new) measures positive as an individual mechanism and fixes
   the shipped `fast`'s over-extension.** Individual row confirmed once on 1,600 confirm-role
   games: 843/1600, **+0.054 [+0.018, +0.089]** (Newcombe lower bound above 0), discordant
   128/85 (`docs/dev/eval/GAP-A1/rows/confirm-individual-tree_lethal_select-1600.json`). Inside
   the shipped bundle it is null but not negative: leave-one-out from the shipped `fast`
   +0.021 [-0.014, +0.056] and from `B_core` at twice the pre-M15 `standard` budget +0.007
   [-0.029, +0.044] (both confirmed once). On the W-25 over-extension scenario the shipped
   `fast` trades its last Blocker in 16 of 16 seeds and with the flag in 0 of 16. Under the
   adoption rule (`docs/ASSUMPTIONS.md:92`: scenario evidence of a real fix, no large harm in a
   400-game guard, the bundle measured at game level) it qualifies for `fast`; not adopted here.

## Method

- **Binaries.** Release builds of this branch. Rows that need no new flag ran a copy of the
  binary built from `1007ca9` before any change (`gcg-eval-base`); rows with a GAP-A1 flag ran
  the build that adds it. Every flag added here is off in `MechanismFlags::NULL` and in both
  shipped presets, so the two builds play the same games for every configuration without the
  new flags.
- **Rows.** `gcg-eval ablation individual|leave-one-out` on the 15 tuning pairs of
  `docs/dev/eval/panel.json`, 400 games (dev) or 1,600 (confirm, run once), mirrored blocks of
  four, forced play/draw (`--choose-first seat0`, A24), 3 or 4 worker threads (results do not
  depend on the count). Tables come from `uv run python docs/dev/eval/M15/rows_table.py rows|versus FILE...`.
  "vs null" compares a row with the null-configuration row on the same master game by game
  (changed games, row-only / null-only challenger wins, difference per game with a Wald 95%
  interval).
- **Individual rows** are `search:null-standard` (the reference `standard` budget, every
  mechanism off) plus the mechanism, against `ref:standard`. **Bundle rows** are
  leave-one-out rows from the shipped bundle (`B_core` with its two guards), at twice the
  pre-M15 `standard` budget (256 / 640 / 2560) as in M15, or at the shipped `fast` budget.
- **Seeds** (A14): dev 22710100, 22710200, 22710300, 22710500, 22710601-22710603, 22710700,
  22710800, 22710810; confirm 30271100, 30271300, 30271800, 30271810, 30271820. Every master is in
  `docs/dev/eval/GAP-A1/seed-ledger.json`. No tuning, eval or review master was used, and no
  held-out pair was played. The scenario tests and `weakness scenarios` use agent seeds 1-16 or
  1-24 as the scenario tests of PLAN §9.8 do.
- **Load.** The machine was shared with the final evaluation and other packages (load averages
  25-35 on 10 cores). Win rates and counts do not depend on load.

## New code

| Path | Content |
| --- | --- |
| `crates/gcg-search/src/flags.rs` | two fields of package `GAP-A1`: `target_multi_rollouts: Option<u32>` (W-14) and `tree_lethal_select: bool` (W-25), off in `MechanismFlags::NULL` |
| `crates/gcg-search/src/presets.rs` | the two fields spelled out as off in `SHIPPED_MECHANISMS` (W-136 requires every field; no preset value changed) |
| `crates/gcg-search/src/mcts.rs` | the core hook `Mechanisms::rollouts`: an iteration plays that many rollouts from the end of its walk; rollout `j > 0` starts from a copy of the walk's end with `derive_seed(R[k], "multi-rollout", j)`; the line's value is the `py_sum` mean. One rollout (the default) runs the reference code unchanged |
| `crates/gcg-search/src/mech/mod.rs` | the trait method `rollouts` (default 1), the dispatch of both flags, `gapa1` in `available` |
| `crates/gcg-search/src/mech/gapa1/mod.rs` | `rollouts`, `budget` (W-14) and `lethal_select` (W-25) |
| `crates/gcg-search/src/mech/gapa1/api.rs` (re-exported as `gcg_search::gapa1`) | the analysis entry points: `proved_lethal`, `lethal_on_state`, `without_effect` (W-17), `damage_judgement`, `is_damage_decision` (W-26), `split_census` (W-05) |
| `crates/gcg-search/src/mech/gapa1/census.rs` | the W-05 observation-key split census |
| `crates/gcg-search/src/mech/gapa1/damage.rs` | the W-26 policy-level Damage judgement |
| `crates/gcg-search/src/mech/gapa1/tests.rs` | unit tests |
| `crates/gcg-search/tests/scenarios/gapa1_*.rs` | W-05 fusion values, W-14 multi-rollout puzzle, W-25 over-extension, W-17 frozen scenarios |
| `crates/gcg-eval/src/weakness_suite.rs` | `gcg-eval weakness` with the modes `scenarios`, `audit`, `damage-regret` and `census` (four small additions in `cli.rs`: the subcommand entry, the `Command` variant, the parse arm and the dispatch arm; one module line in `lib.rs`) |
| `docs/dev/eval/GAP-A1/weakness/scenarios.json` | the 22 frozen WK scenarios (canonical snapshots) |
| `docs/dev/eval/GAP-A1/audit/make_sample.sh` | the deterministic W-17 audit sample, its rule-based count and the blinded transcripts |

Also touched, as the new fields and subcommand force: `crates/gcg-search/tests/common/mod.rs`
(the test copy `NULL_MECHANISMS` names every field, two lines), `crates/gcg-search/src/lib.rs`
(one re-export line), `crates/gcg-search/tests/scenarios/main.rs` (four module lines) and
`docs/dev/eval/P6-N/cli-help.txt` (regenerated with `python3 scripts/cli_help_snapshot.py
--write`: the `weakness` help texts are added, nothing else changes).

## Summary

| Item | Status | What this package added | Key measurement |
| --- | --- | --- | --- |
| W-05 | open (one reason) | `gcg-eval weakness census`; the fusion position's values per world class | every split has an observation key (`transpositions`, measured null); the scenario bar is unattainable under `ROLE_WEIGHTS` (break-even trick share 0.497) |
| W-14 | resolved by measurement (not adopted) | `target_multi_rollouts{R}` with the core hook `Mechanisms::rollouts` | puzzle 0-2/24 alone, 21/24 with the critic; game level null (confirm +0.025 [-0.010, +0.060]) |
| W-17 | open (one step) | `gcg-eval weakness scenarios` and `audit`, 22 frozen WK scenarios, the 120-game audit sample and its transcripts | suite: reference 0.756, shipped `fast` 0.872, shipped `standard` 0.909; the blinded auditor step remains |
| W-25 | open (the lead's decisions) | the over-extension scenario (T-118); `tree_lethal_select` | T-118 met by the shipped `standard` (0/16 over-extensions), missed by the shipped `fast` (16/16), met with `tree_lethal_select` (0/16); `tree_lethal_select` individual confirm +0.054 [+0.018, +0.089], bundle rows null |
| W-26 | resolved by measurement; positive in the bundle | `gcg-eval weakness damage-regret`; the bundle re-measurement | policy regret -0.022 [-0.032, -0.011] per Damage decision; bundle confirm **+0.037 [+0.001, +0.074]** |

## W-05 — Strategy fusion

- **Status:** open, on one precise reason: the scenario's root-decision bar (the strengthened
  search avoids the attack in at least 13 of 16 seeds, with the trick in half of the worlds)
  asks a search that maximises the shipped leaf evaluation for a clear preference it does not
  have: under `ROLE_WEIGHTS` the two moves tie at that share (measured break-even share 0.497,
  below), so no setting of the search makes 13 of 16 a reliable result. It
  closes when the lead accepts this measurement in place of the bar, or with a leaf evaluation
  that prices the bounce (the W-34/W-35 refits measured null or negative and are not adopted).
  The census half of the verification is done: the census exists, and every remaining split has
  an observation key that removes it (`transpositions`, measured null), so no split is left with
  neither a key nor a regret interval.
- **What was done:**
  1. *The census* (`gcg-eval weakness census`, `crates/gcg-eval/src/weakness_suite.rs`, over
     `gcg_search::gapa1::split_census`, `crates/gcg-search/src/mech/gapa1/census.rs`). One search
     per position and configuration runs with a recording tracer; every iteration's walk is
     replayed from its world, and the tree node of every decision in the walk is rebuilt as the
     core finds it (the root move's child, then at every decision with more than one canonical
     move the child of the move taken, keyed `(player, kind if observation_keys, move key)`;
     with `observation_keys` an opponent's action-step pass keeps the node). At each searcher
     node below the root the searcher's observation is keyed as `transpositions` keys it. A
     *split* is one more node for an observation key that already has one: one information set
     of the searcher, separate statistics in the tree. Each split is classified by the first
     difference of its path from the key's first path: `opponent_pass_vs_no_decision` (the
     auto-pass split), `move_order` (the same moves in another order), `extra_node:<mover>:<kind>`
     (one path is the other plus nodes that left the observation unchanged), or
     `<mover>_move:<kind>` (different moves at a decision of that kind).
  2. *The positions* are a reconstruction of the investigation's 183 (only their counts
     survive, `docs/python/AI.md:462-469`): 3 positions sampled per game (`regret`'s sampler) from 60
     reference `standard` self-play games on the three example pairings, dev master 22710500,
     180 positions.
  3. *The scenario*, revisited (`crates/gcg-search/tests/scenarios/gapa1_w05_fusion_values.rs`):
     P6-C's fusion position searched at 4,000 iterations (seeds 1-4) as built (the trick
     hidden), and with the defender's card revealed as the trick or as a filler card, which
     gives each move's value in its world class with the opponent's reply fitted to it.
- **Evidence:**
  - The census (`docs/dev/eval/GAP-A1/weakness/census-report.json`; 180 positions, dev 22710500):

    ```text
    gcg-eval weakness census --agents search:null-standard,search:null-standard:observation_keys=false,search:fast,search:standard \
      --decks examples --pairs all --games 60 --per-game 3 --seed 22710500 --role dev \
      --ledger docs/dev/eval/GAP-A1/seed-ledger.json --workers 3 --out docs/dev/eval/GAP-A1/weakness/census-report.json
    ```

    | Configuration | iterations | searcher nodes | splits | positions with splits | splits by cause |
    | --- | ---: | ---: | ---: | ---: | --- |
    | reference `standard` (null configuration, `observation_keys` on) | 50,865 | 16,279 | 1,943 | 90 | `move_order` 1,350; `own_move:main` 192; `opponent_move:order_trigger` 161; `own_move:select` 105; `own_move:yes_no` 53; `own_move:action_step` 26; `extra_node:opponent:select` 25; `opponent_move:main` 13; `extra_node:opponent:main` 10; `extra_node:own:select` 3; `own_move:burst` 2; `opponent_move:burst`, `opponent_move:redraw`, `opponent_move:yes_no` 1 each |
    | the same with `observation_keys` off (negative control) | 51,233 | 16,492 | 2,190 | 100 | as above plus **`opponent_pass_vs_no_decision` 237** (and `opponent_move:action_step` 1) |
    | shipped `fast` | 75,300 | 23,532 | 3,316 | 97 | `move_order` 2,220; `own_move:main` 372; `opponent_move:order_trigger` 284; `own_move:select` 213; others 227 |
    | shipped `standard` | 652,540 | 153,417 | 27,638 | 109 | `move_order` 19,098; `opponent_move:order_trigger` 2,913; `own_move:main` 2,066; `own_move:select` 1,063; `own_move:action_step` 874; `own_move:yes_no` 603; others 1,021 |

    The auto-pass split behaves as the investigation measured it (Python: 389 splits in 24 of
    183 positions, then 0): 237 with the observation keys off, 0 with them on. The investigation's
    "12 splits of other causes" cannot be matched one for one, because its classification is
    lost; this census counts every split of the action-path tree, most of them transpositions
    and equivalent moves (`move_order`; different own moves, or the opponent's trigger orders,
    that leave the same observation), which carry no hidden information. Whatever the cause, a
    split is two tree nodes for one observation key of the searcher, and the `transpositions`
    mechanism (P6-D, S7) keys every node by exactly that key (`mcts.rs`, `descend`:
    `node = tree.transposed(transpose::key(s, player))`), so it removes every split of every
    class by construction. Every split therefore has an observation key that removes it, and 0
    splits have neither a key nor a regret interval (the W-05 rule). That key is measured
    (P6-D, W-11): null at game level and in regret, resolved by measurement, not adopted.
  - `weakness_suite::tests::the_census_counts_splits_by_cause` (pass): searcher nodes minus
    observation keys equal the splits, the causes sum to the splits, the census is deterministic,
    and the census iterations are the search's.
  - Scenario values (`w05_the_evaluation_prefers_the_attack_below_a_high_trick_share`, pass,
    T-109 as a record):

    | Position | attack | end the main phase |
    | --- | ---: | ---: |
    | trick hidden (13 of 46 unseen cards, share 0.28) | 0.3163 | 0.2939 |
    | trick revealed | 0.3134 | 0.3349 |
    | filler revealed | 0.2878 | 0.2666 |

    Holding back gains 0.0215 in the trick worlds and loses 0.0212 in the others, so the
    break-even trick share is `0.0212 / (0.0212 + 0.0215) = 0.497`. With the trick in half of the
    worlds the two moves differ by 0.0002 in the search's own evaluation; at the position's own
    share (0.28) the attack is the better move. A search that holds back in 13 of 16 seeds at a
    share of one half would contradict the objective it maximises. P6-C's sweeps (shares 0.28 to
    0.77, other tricks and attackers) found the same root decision; for this trick the expected
    difference in favour of holding back would be at most 0.012 even at a share of 0.77, inside
    the root's noise at the reference `standard` budget.
  - P6-C's records stand: `mech::p6c::tests::w05_bucketed_opponent_nodes_reply_by_hand`
    (bucketed opponent nodes reply by hand) and `p6c_w05_fusion.rs` (16/16 attacks for the
    reference and `opponent_buckets=4` and `=8`).
- **Ablation:** P6-C's `opponent_buckets` rows (individual `m` sweep and high budget, all null)
  stand. The observation key of the census, `transpositions`, has its P6-D rows: individual
  +0.020 [-0.051, +0.091] (dev 24600001), confirmation -0.006 [-0.042, +0.029] (confirm
  30460002), regret +0.0000 [-0.0029, +0.0029] per decision, and it was in `B_mid` (null
  against `B_core`). No new row: the census and the value analysis change no search (A13: the
  analysis tools are not on any path the agents execute; the corpus and equivalence smoke of the
  gate show the agents unchanged).
- **tests.json:** T-109 (as a record; its bar is the open part).

## W-14 — Close target choices: several rollouts per world (plan item 2)

- **Status:** resolved by measurement (not adopted). Plan items (1) and (3) were P6-E's
  (`target_rollouts`, resolved by measurement); item (2) is now built and measured: it misses
  the puzzle bar alone, lowers the critic's puzzle count, and is null at game level, so it stays
  off. The puzzle joins `crates/gcg-search/tests/ai_puzzles.rs` only when `target_rollouts` is
  adopted, which neither row supports.
- **Resolution as implemented:** `target_multi_rollouts{R}`. At an effect-target root decision
  (`is_target_decision`, `ai/mcts.py:165-169`) every iteration plays `R` rollouts from the end of
  its walk and backs up their mean (core hook `Mechanisms::rollouts`, `mcts.rs`), and the
  decision's budget and cap are divided by `R` (rounded up, `gapa1::budget`), so the decision
  plays about as many rollouts as the reference plays iterations: several rollouts per world
  instead of more worlds. Rollout `j > 0` uses `derive_seed(R[k], "multi-rollout", j)`, so the
  k-th visit of every root move shares its rollout seeds (common random numbers hold). Every
  other decision is the reference's.
- **Evidence:**
  - Unit tests (`crates/gcg-search/src/mech/gapa1/tests.rs`, pass):
    `one_rollout_per_iteration_is_the_null_configuration` (24 D2 vector decisions bit for bit,
    effort included); `multi_rollouts_change_only_target_decisions_and_play_r_rollouts_per_iteration`
    (the 126 searched D2 vector decisions: the 9 without iterations and the 96 non-target ones
    are bit-identical to the null configuration, and at the 21 target decisions the budget is
    `ceil(B / 4)`, the leaf is valued exactly `4 x iterations` times, and the search is
    deterministic);
    `the_budget_is_divided_by_the_rollouts_rounding_up`.
  - Scenario test `crates/gcg-search/tests/scenarios/gapa1_w14_multi_rollout.rs::multi_rollouts_on_the_repair_ping_are_recorded`
    (T-114, pass), the <Repair> ping puzzle over 24 agent seeds at the reference `standard`
    budget. P6-E's counts are re-checked in the same test (null configuration 10/24,
    `target_rollouts` 24/24):

    | `R` | `target_multi_rollouts` alone | with `target_rollouts` |
    | ---: | ---: | ---: |
    | 2 | 2/24 | 21/24 |
    | 4 | 0/24 | 21/24 |
    | 8 | 0/24 | 21/24 |

    The extra rollouts remove rollout noise within a world, and the rollouts' own preference is
    the wrong target (they favour pinging the Guncannon, as the critic's multi-rollout variant
    did, P6-E), so the search converges on it. Alone it misses the bar (20/24) at every `R`; with
    the critic it costs three seeds.
- **Ablation** (individual rows; dev 22710300 for all four, so the rows share their deals; the
  null row is `engine_error_leaf=false`, the null configuration itself):

  | Row | wins/N (rate, Wilson 95%) | paired difference (Newcombe 95%) | discordant | iterations / applies per game (challenger vs opponent) | vs null | evidence |
  | --- | --- | --- | --- | --- | --- | --- |
  | null configuration | 210/400 (0.525, [0.476, 0.574]) | +0.050 [-0.020, +0.119] | 31/21 | 9799 vs 9934 / 117742 vs 119358 | — | `rows/dev-individual-null-400.json` |
  | `target_multi_rollouts=4` | 207/400 (0.517, [0.469, 0.566]) | +0.035 [-0.040, +0.109] | 33/26 | 8855 vs 9917 / 116654 vs 119391 | 137 changed; 10 / 13; -0.0075 [-0.0310, +0.0160] | `rows/dev-individual-target_multi_rollouts-4-400.json` |
  | `target_multi_rollouts=2` | 205/400 (0.512, [0.464, 0.561]) | +0.025 [-0.044, +0.094] | 28/23 | 9081 vs 9871 / 114910 vs 118309 | 112 changed; 7 / 12; -0.0125 [-0.0338, +0.0088] | `rows/dev-individual-target_multi_rollouts-2-400.json` |
  | both: `target_multi_rollouts=4,target_rollouts=true` | 206/400 (0.515, [0.466, 0.564]) | +0.030 [-0.045, +0.105] | 33/27 | 8841 vs 9934 / 116221 vs 119466 | 141 changed; 10 / 14; -0.0100 [-0.0340, +0.0140] | `rows/dev-individual-target_multi_rollouts-4_target_rollouts-true-400.json` |
  | `target_multi_rollouts=4`, confirm 30271300 (run once) | 820/1600 (0.512, [0.488, 0.537]) | +0.025 [-0.010, +0.060] | 115/95 | 8848 vs 9829 / 115874 vs 117853 | — | `rows/confirm-individual-target_multi_rollouts-4-1600.json` |

  Paths are under `docs/dev/eval/GAP-A1/`. The `R = 4` row reached +0.035 against
  `ref:standard`, so it was confirmed once (the W-14 work item asks it for any row at +0.03): null
  on 1,600 confirm games. Against the null row on the same deals every multi-rollout row is
  slightly worse (-0.0075 to -0.0125). Iterations per game fall by the division (8,855 against
  9,799) while the applies stay level (116,654 against 117,742): the cost is the reference's.
  The critic's row is P6-E's (`docs/dev/eval/P6-E/individual-target_rollouts.json`, -0.005).
  Leave-one-out: not applicable (not adopted).
- **tests.json:** T-114 (with P6-E's `p6e_w14_target_critic.rs`).

## W-26 — Kill-aware and damage-payoff targeting

- **Status:** resolved by measurement, and **positive inside the bundle**: the re-measurement
  under the strengthened search that the plan asks for is confirmed above zero (Newcombe lower
  bound +0.001). Not adopted here: the shipped presets are frozen, so adoption, and a
  leave-one-out row from the adopted bundle at the shipped budgets, are the lead's. The
  policy-level regret is measured with the new `gcg-eval weakness damage-regret`. Damage-payoff
  targeting is not rebuilt, by a recorded decision (below).
- **What was done:**
  1. *Re-measurement under the strengthened search.* `kill_aware_targeting` (P6-F) was added to
     `B_core` with its guards at twice the pre-M15 `standard` budget, and its leave-one-out row
     was run on dev seeds and, being at +0.035, confirmed once:

     ```text
     gcg-eval ablation leave-one-out --bundle search:standard:iterations_per_option=256,min_iterations=640,max_iterations=2560,adaptive_horizon=true,capped_rollout_continuation=true,tactical_solver=true,exclude_capped_play_draw=true,engine_error_leaf=true,kill_aware_targeting=true \
       --mechanism kill_aware_targeting=false --games 400 --seed 22710100 --role dev --choose-first seat0 \
       --ledger docs/dev/eval/GAP-A1/seed-ledger.json --workers 4 --out docs/dev/eval/GAP-A1/rows/dev-loo-kill_aware_targeting-bcore2x-400.json
     # confirmation: --games 1600 --seed 30271100 --role confirm, run once
     ```

  2. *Policy-level regret per Damage decision* (`gcg-eval weakness damage-regret`,
     `crates/gcg-eval/src/weakness_suite.rs` over `gcg_search::gapa1::damage_judgement`,
     `crates/gcg-search/src/mech/gapa1/damage.rs`). A Damage decision is the decision
     `kill_aware_targeting` changes: a SELECT outside a mode choice and a Rest substitution, of
     harmful polarity, whose chosen cards take a literal amount of damage, with two or more
     options. At each one in the recording games both rollout policies score the options (the
     reference policy, and P6-F's scorer with `kill_aware_targeting` alone); each option's oracle
     value is the mean over shared worlds of the decider's information set of a playout to the
     end by the reference policy at the configuration's rollout temperature, with one rollout
     seed per world for every option. A policy's regret is the best option's value minus its
     pick's: at temperature 0, and in expectation under the rollout's sampling (the softmax at
     the rollout temperature).
  3. *Damage-payoff targeting: not rebuilt (decision).* The Python setting's code was squashed
     with `ws/ai-policy` and its exact rule is lost; "damage payoff" can only mean preferring a
     non-lethal damage target that one's own cards then cash in. On the panel that payoff
     exists in one tuning deck: of the 8 panel decks only `red-purple-control` holds a card that
     reads a damaged *enemy* Unit (3 copies of GD01-111 Battle of Aces, "Choose 1 damaged enemy
     Unit. Deal 3 damage to it."); `purple-white-tekkadan`'s "damaged" cards (ST05-001, ST05-002,
     ST05-015, GD02-054, GD02-114) pay off on damaged *friendly* Units, which harmful damage
     choices never target; the other six decks hold none (scan of
     `src/gcg_sim/data/gcgapi/cards.ndjson` texts for "damaged", 2026-11-03). A rule that can fire
     only for one deck in 5 of the 15 tuning pairs, behind the rarer half of the Python pair,
     would give a row indistinguishable from its placebo at 400 games, so it is not built.
- **Evidence:**
  - Policy-level regret (`docs/dev/eval/GAP-A1/weakness/damage-regret-report.json`): every
    Damage decision of 120 `ref:standard` self-play games on the tuning pairs (dev 22710200),
    oracle values over 8 shared worlds, the shipped `standard`'s policy settings (knobs, rollout
    temperature 0.3, `ROLE_WEIGHTS`):

    ```text
    gcg-eval weakness damage-regret --recorder ref:standard --policy search:standard --worlds 8 --games 120 \
      --seed 22710200 --role dev --ledger docs/dev/eval/GAP-A1/seed-ledger.json --workers 3 \
      --out docs/dev/eval/GAP-A1/weakness/damage-regret-report.json
    ```

    | Regret per Damage decision (484 decisions, 184 with different picks) | reference policy | `kill_aware_targeting` | difference (kill-aware minus reference, normal 95%) |
    | --- | ---: | ---: | --- |
    | pick at temperature 0 | 0.0754 (se 0.0059) | 0.0537 (se 0.0051) | **-0.0217 [-0.0324, -0.0110]** |
    | expected under the rollout's sampling | 0.0752 (se 0.0056) | 0.0542 (se 0.0046) | **-0.0210 [-0.0307, -0.0113]** |

    The policy's own Damage picks are clearly better with the flag, which reproduces the Python
    investigation's "regret per Damage decision -0.020" (`docs/python/AI.md:457-461`).
  - Unit test `weakness_suite::tests::a_damage_decision_is_judged_by_both_policies` (pass): the
    <Repair> ping position is a Damage decision, the block decision is not; the judgement is
    deterministic, every value lies in [0, 1], both regrets are non-negative, both pick
    distributions sum to 1.
  - The search-level regret is P6-F's (`docs/dev/eval/P6-F/rows/regret-*.json`): -0.0048 (se
    0.0044) against -0.0016 (se 0.0049), within noise.
- **Ablation** (paths under `docs/dev/eval/GAP-A1/`):

  | Row | wins/N (rate, Wilson 95%) | paired difference (Newcombe 95%) | discordant | iterations / applies per game (bundle vs bundle without) | lowest pair | evidence |
  | --- | --- | --- | --- | --- | --- | --- |
  | leave-one-out from `B_core` 2x, dev 22710100 | 207/400 (0.517, [0.469, 0.566]) | +0.035 [-0.040, +0.109] | 33/26 | 32382 vs 32061 / 581291 vs 596442 | 11/28 (UB 0.576) | `rows/dev-loo-kill_aware_targeting-bcore2x-400.json` |
  | the same, confirm 30271100 (run once) | 830/1600 (0.519, [0.494, 0.543]) | **+0.037 [+0.001, +0.074]** | 129/99 | 32860 vs 32490 / 601667 vs 606589 | 48/104 (UB 0.557) | `rows/confirm-loo-kill_aware_targeting-bcore2x-1600.json` |

  The earlier rows, at the null configuration (P6-F): individual +0.080 [+0.009, +0.150] (dev
  20600100), confirmed null +0.021 [-0.015, +0.057] (confirm 30600100). Inside the bundle the
  dev and confirm rows agree (+0.035 and +0.037; 2,000 games together, 1037/2000). The
  mechanism costs nothing measurable (applies per game 601,667 against 606,589). No pair has an
  upper bound below 0.5.
- **tests.json:** no scenario id (W-26's verification is the ablation and regret rows).

## W-17 — Implausible plays and missed lethals

- **Status:** open, on one remaining step: the blinded audit itself (step 3's human or agent
  auditors classifying the transcripts of the new sample, and the confirmed-implausible count
  per game with its Wilson interval from their classification). Everything the plan asks of the
  code is done: step (1) `lethal_root` (P6-B, resolved by measurement); step (2) the
  `gcg-eval weakness` suite of frozen WK scenarios, scored for the reference and the shipped
  presets (below); step (3) the deterministic sample of 20 BO1 games per example pairing for
  each AI version with every replay committed, its rule-based count (missed lethals by the
  tactical solver, wasted effects, self-harm targets) and the transcript generator; step (4)
  P6-B's recovery (0 real missed lethals). The auditor step is a follow-up for the lead: it needs
  auditors who did not generate the games, so it cannot be done by this package.
- **What was done:**
  1. *Frozen scenarios* (step 2): `crates/gcg-search/tests/scenarios/gapa1_weakness_scenarios.rs`
     builds 22 positions with the Rust testkit from the existing puzzles and scenario tests, one
     or more per WK category, including the six the lost benchmark named (`hold_up`,
     `ex_variant`, `rest_the_active_unit`, `damage_the_unit_that_cannot_repair`,
     `burst_aware_attack_order`, `pair_the_linking_pilot`), and freezes them as canonical
     snapshots in `docs/dev/eval/GAP-A1/weakness/scenarios.json` (the test checks that each
     reloads to the same digest and the same reference decision, and that the committed file is
     current; `GAPA1_WRITE_SCENARIOS=1` rewrites it). A `choice` scenario is solved when the
     agent's decision is one of `want`; a `turn` scenario when, after the agent plays its seat
     until the turn ends (the other seat `ref:standard`, seed + 100), its winner, zones and active
     cards hold.
  2. *`gcg-eval weakness scenarios`* scores them over agent seeds 1-16:

     ```text
     gcg-eval weakness scenarios --agents ref:standard,search:fast,search:standard,search:fast:tree_lethal_select=true \
       --seeds 16 --workers 3 --out docs/dev/eval/GAP-A1/weakness/scenarios-report.json
     ```

  3. *The audit sample* (step 3): `docs/dev/eval/GAP-A1/audit/make_sample.sh runs` plays, for
     each example pairing in the 2026-10 audit's order, 20 BO1 matches with every replay saved,
     with the shipped strengthened `standard` (`gcg-sim benchmark --ai-preset standard`) and with
     the reference `standard` (`gcg-eval reference-benchmark --reference-preset standard`), each
     in both seats, masters 22710601-22710603 (the audit protocol of
     `scripts/blind_transcripts.py`). The 120 games are in `docs/dev/eval/GAP-A1/audit/runs/`
     (results, summaries and replays). `make_sample.sh transcripts OUT KEY` writes the blinded
     transcripts with the existing, tested `scripts/blind_transcripts.py` (the Rust port is W-119)
     for the auditors.
  4. *`gcg-eval weakness audit`* (`make_sample.sh count`) replays every game and flags, at each
     decision with two or more options: a *missed lethal* (at a MAIN decision of the turn player,
     `gcg_search::gapa1::proved_lethal` proves a lethal first move in each of 4 worlds of the
     decider's information set and the player does not win that turn; *real* when
     `lethal_on_state` proves it on the true state too), a *wasted effect* (a spending play that
     the reference's no-effect check finds without effect on the decider's information set), and
     a *self-harm target* (a harmful SELECT on one's own card while an enemy card was an option).
     These are candidates for the auditors, not their verdict.
- **Evidence:**
  - Frozen scenarios (`docs/dev/eval/GAP-A1/weakness/scenarios-report.json`), solves of 16:

    | Scenario | WK | `ref:standard` | shipped `fast` | shipped `standard` | `fast` + `tree_lethal_select` |
    | --- | --- | ---: | ---: | ---: | ---: |
    | `wk1_lethal_attack_the_exposed_player` | 1 | 16 | 16 | 16 | 16 |
    | `wk1_lethal_goes_face_instead_of_taking_a_free_kill` | 1 | 16 | 16 | 16 | 16 |
    | `wk1_lethal_rests_the_blocker_before_attacking` | 1 | 16 | 16 | 16 | 16 |
    | `wk1_pair_the_linking_pilot` | 1 | 16 | 16 | 16 | 16 |
    | `wk1_burst_aware_attack_order` | 1 | 16 | 16 | 16 | 16 |
    | `wk1_four_attacks_through_one_blocker` | 1 | 5 | 16 | 16 | 16 |
    | `wk1_lethal_past_a_blocker_that_cannot_block` | 1 | 0 | 16 | 16 | 16 |
    | `wk2_blocks_to_survive` | 2 | 16 | 16 | 16 | 16 |
    | `wk2_uses_an_action_step_trick_to_survive` | 2 | 16 | 16 | 16 | 16 |
    | `wk2_keeps_the_last_blocker` | 2 | 0 | 0 | 16 | 16 |
    | `wk3_takes_the_favourable_trade` | 3 | 16 | 16 | 16 | 16 |
    | `wk3_does_not_attack_into_a_bigger_blocker` | 3 | 16 | 16 | 16 | 16 |
    | `wk3_blocks_when_the_blocker_wins_the_fight` | 3 | 16 | 16 | 16 | 16 |
    | `wk4_hold_up` | 4 | 16 | 16 | 16 | 16 |
    | `wk5_ex_variant` | 5 | 0 | 0 | 0 | 0 |
    | `wk6_mulligans_a_hand_of_uncastable_cards` | 6 | 16 | 16 | 16 | 16 |
    | `wk6_keeps_a_hand_with_early_plays` | 6 | 16 | 16 | 16 | 16 |
    | `wk7_rest_the_active_unit` | 7 | 16 | 16 | 16 | 16 |
    | `wk7_damage_the_unit_that_cannot_repair` | 7 | 5 | 3 | 16 | 3 |
    | `wk7_removal_targets_the_most_valuable_unit_it_can_destroy` | 7 | 16 | 16 | 0 | 16 |
    | `wk7_excess_trashes_the_weakest_unit` | 7 | 16 | 16 | 16 | 16 |
    | `wk8_two_turn_plan` | 8 | 0 | 16 | 16 | 16 |

    | Category (scenarios) | `ref:standard` | shipped `fast` | shipped `standard` | `fast` + `tree_lethal_select` |
    | --- | --- | --- | --- | --- |
    | WK1 (7) | 85/112 [0.672, 0.829] | 112/112 [0.967, 1.000] | 112/112 [0.967, 1.000] | 112/112 [0.967, 1.000] |
    | WK2 (3) | 32/48 [0.525, 0.783] | 32/48 [0.525, 0.783] | 48/48 [0.926, 1.000] | 48/48 [0.926, 1.000] |
    | WK3 (3) | 48/48 [0.926, 1.000] | 48/48 [0.926, 1.000] | 48/48 [0.926, 1.000] | 48/48 [0.926, 1.000] |
    | WK4 (1) | 16/16 [0.806, 1.000] | 16/16 [0.806, 1.000] | 16/16 [0.806, 1.000] | 16/16 [0.806, 1.000] |
    | WK5 (1) | 0/16 [0.000, 0.194] | 0/16 [0.000, 0.194] | 0/16 [0.000, 0.194] | 0/16 [0.000, 0.194] |
    | WK6 (2) | 32/32 [0.893, 1.000] | 32/32 [0.893, 1.000] | 32/32 [0.893, 1.000] | 32/32 [0.893, 1.000] |
    | WK7 (4) | 53/64 [0.718, 0.901] | 51/64 [0.683, 0.877] | 48/64 [0.632, 0.840] | 51/64 [0.683, 0.877] |
    | WK8 (1) | 0/16 [0.000, 0.194] | 16/16 [0.806, 1.000] | 16/16 [0.806, 1.000] | 16/16 [0.806, 1.000] |
    | all (22) | 266/352 (0.756) [0.708, 0.798] | 307/352 (0.872) [0.833, 0.903] | 320/352 (0.909) [0.875, 0.935] | 323/352 (0.918) [0.884, 0.942] |

    Reading: the shipped presets solve every WK1 lethal (the reference misses the W-16 and W-32
    lethals) and the WK8 two-turn plan (the adaptive horizon). WK2's last Blocker is the W-25
    finding: the shipped `fast` trades it 16/16, the shipped `standard` and `fast` with
    `tree_lethal_select` keep it 16/16. WK5's EX payment needs `ex_variants` (P6-E, not adopted).
    Two WK7 rows need a caveat: the <Repair> ping is solved by the shipped `standard` (16/16) but
    not by `fast` (3/16, the critic `target_rollouts` is not adopted), and the removal-target
    puzzle (`test_removal_targets_the_most_valuable_unit_it_can_destroy`) is "missed" 16/16 by
    the shipped `standard` because the position, as Python built it, gives the searcher no
    Shields: every target loses the game next turn, the root values are 0.01083, 0.01088 and
    0.01082 at about 3,080 visits each, and the 8x budget's choice among equal lost lines is a
    tie that the reference's smaller budget breaks by its prior. That row measures the
    scenario's construction, not a strength regression.
  - `gapa1_weakness_scenarios::the_frozen_weakness_scenarios_are_current` (T-116, pass) and
    `the_audit_solver_proves_the_wk1_lethals_and_no_others` (pass: the solver proves the three
    pure-attack WK1 lethals from the decider's information and on the state, finds no lethal at
    any MAIN position outside WK1 (negative control), and never for the player not to decide).
  - `crates/gcg-eval/src/weakness_suite.rs` unit tests (pass):
    `the_committed_scenarios_cover_every_category_and_score_with_controls` (22 scenarios, WK1-WK8,
    every `want` legal; a solved scenario fails with the wrong winner or an empty `want`:
    negative controls), `the_audit_replays_a_recorded_game_and_its_flags_are_deterministic` (a
    random-play game written as a replay is replayed, flagged, and flagged identically twice;
    random play makes flags: positive control), `checks_parse_from_the_scenario_format`.
  - The audit count (`docs/dev/eval/GAP-A1/weakness/audit-report.json`):

    | Version, pairing | games | decisions (2+ options) | missed lethals (real) | wasted effects | self-harm targets | games with a flag (Wilson 95%) |
    | --- | ---: | ---: | --- | ---: | ---: | --- |
    | `mcts-standard`, blue-white-federation-vs-purple-white-tekkadan | 20 | 1617 | 2 (2) | 2 | 0 | 3/20 [0.052, 0.360] |
    | `mcts-standard`, purple-white-tekkadan-vs-red-green-zeon | 20 | 1331 | 0 (0) | 0 | 0 | 0/20 [0.000, 0.161] |
    | `mcts-standard`, red-green-zeon-vs-blue-white-federation | 20 | 1586 | 4 (2) | 3 | 0 | 6/20 [0.145, 0.519] |
    | `search-standard`, blue-white-federation-vs-purple-white-tekkadan | 20 | 1698 | 1 (1) | 0 | 0 | 1/20 [0.009, 0.236] |
    | `search-standard`, purple-white-tekkadan-vs-red-green-zeon | 20 | 1408 | 0 (0) | 0 | 0 | 0/20 [0.000, 0.161] |
    | `search-standard`, red-green-zeon-vs-blue-white-federation | 20 | 1506 | 5 (4) | 2 | 0 | 6/20 [0.145, 0.519] |
    | **`mcts-standard`, all three** | 60 | 4534 | 6 (4) | 5 | 0 | 9/60 [0.081, 0.261] |
    | **`search-standard`, all three** | 60 | 4612 | 6 (5) | 2 | 0 | 7/60 [0.058, 0.222] |

    Reading: these are candidates for the auditors, not their verdict, and the counts are small
    (60 games per version). The shipped `standard` misses a lethal that the solver proves on the
    true state 5 times in 60 games (the reference 4 times): the shipped presets run the solver in
    rollouts (`tactical_solver`) but not at the root (`lethal_root`, P6-B, null at game level and
    not adopted), so a missed lethal at the root remains possible. Wasted effects are rare (5 and
    2 Command plays the reference's no-effect check calls idle), and no self-harm target occurred.
    The flagged games and turns are listed in the report (`runs[].flagged_games`).
  - The transcripts: `make_sample.sh transcripts OUT KEY` wrote 120 blinded transcripts (10 MB)
    and the key, and a second run wrote identical bytes (`diff -r`, `cmp`: identical). They are
    not committed: the replays they are made from are, and the command regenerates them byte for
    byte (game ids are hashes of the game's content).
  - P6-B's step (4) record stands: 15 regenerated games, 0 real missed lethals.
- **Ablation:** no mechanism (A13): the suite, the sample and the count only play the existing
  agents and read states; the agents' code paths are untouched (gate: corpus, null-configuration
  tests). The lethal solver's rows are P6-B's (`lethal_root`, resolved by measurement).
- **tests.json:** T-116.

## W-25 — The opponent modelled with the searcher's own policy

- **Status:** open, narrowed to two decisions of the lead. The verification is met for the
  shipped `standard`: the over-extension scenario (T-118) now reproduces the weakness (the
  reference over-extends in 16 of 16 seeds) and the shipped `standard` over-extends in 0 of 16
  (bar: at most 3), and the opponent settings have their rows. Open: (a) the shipped `fast`
  misses the bar (16 of 16) unless `tree_lethal_select` is adopted (0 of 16 with it; its rows
  below); (b) plan items (2), the per-deck opponent profile, and (3), belief weighting by the
  opponent model, are not built, by the decisions recorded below, which the lead accepts or
  not.
- **What was done:**
  1. *The over-extension scenario (T-118) now reproduces.* P6-F's two constructions put the
     punishing reply inside the tree or let the evaluation rate the attack as good. The
     position that has both properties is P6-B's last-Blocker position
     (`p6b_w03_last_blocker::last_blocker`): the searcher's <Blocker> Messala can attack and
     destroy the opponent's last Shield, which over-extends, because the opponent's three
     active GMs then need only the searcher's two Shields and one more hit. The punishing reply,
     all three GMs at the player, is the line the shared policy plays only rarely (it prefers
     destroying a rested Pisces for free to attacking a player who still has Shields), so
     rollouts that model the opponent with the shared policy see the trade as safe. A searcher
     over-extends when it has lost or Messala is rested at the end of its turn.
  2. *Opponent settings measured on it:* the shared policy (reference and null configuration),
     P6-F's fitted opponent policy (`opponent_knobs`, plan item 1), and the tactical solver in
     rollouts (`tactical_solver`, P6-B, adopted): with it a rollout's mover, the opponent
     included, plays a lethal line the solver proves on the rollout world, a competent opponent
     where it matters most.
  3. *A finding on the shipped `fast`, and a new mechanism for it.* The shipped `fast`
     over-extends in 16 of 16 seeds. The cause (traced with a recording tracer, seed 1): with
     `adaptive_horizon`, ending the main phase is valued lower (its lines are unsettled and play
     one more turn: 0.736 against 0.835 without), so the search returns to the attack after its
     first line lost to the solver's lethal in the rollout (0.010). The second visit expands
     the opponent's turn into the tree, where the opponent's replies are tried in prior order
     (the shared policy's preference for the free Pisces kills) and selected by UCB, not by the
     solver; each of those lines is a win for the searcher, and at a `fast`-sized budget the
     attack's mean ends above ending the phase (81 visits at 0.788 against 53 at 0.736). The
     shipped `standard` has the budget for the opponent's nodes to find the lethal. So below the
     root the opponent is still modelled by the shared policy's priors. `tree_lethal_select`
     (GAP-A1, `mech/gapa1/mod.rs::lethal_select`) closes that gap: at an in-tree MAIN decision
     of the turn player (either side, below the root, which `lethal_root` owns) the node selects
     the first move of a lethal line that P6-B's `rollout_lethal` proves on the iteration's
     world, with P6-B's per-decision solver cap and applies counted as auxiliary applies.
  4. *Plan items (2) and (3): not built (decisions recorded).* (2) A per-deck opponent profile
     would be the W-23 calibration fitted per benchmark deck; it specialises the fitted opponent
     policy (`opponent_knobs`), which measured negative (-0.020, P6-F) and over-extends 16/16
     here like the shared policy, while the competent-opponent effect the scenario needs comes
     from the solver, which needs no fit. (3) Belief weighting by this model: P6-C's
     `belief_worlds` weighs the opponent's END_MAIN decision with the reference policy; neither
     `opponent_knobs` nor a profile is adopted, and the solver plays only proved lethal lines,
     which the weighted decision never involves, so there is no adopted opponent model to weigh
     by.
- **Evidence:**
  - Scenario test `crates/gcg-search/tests/scenarios/gapa1_w25_over_extension.rs::the_reference_over_extends_and_a_competent_opponent_model_does_not`
    (T-118, pass), 16 agent seeds, the defender `ref:standard` (seed + 100). Over-extensions:

    | Searcher | over-extends |
    | --- | ---: |
    | reference `standard` (pinned) | 16/16 |
    | null configuration (reference `standard` budget) | 16/16 |
    | `opponent_knobs` | 16/16 |
    | `tactical_solver` | 0/16 |
    | `tactical_solver` + `adaptive_horizon` | 15/16 |
    | shipped `fast` | 16/16 |
    | shipped `fast` without `adaptive_horizon` | 0/16 |
    | shipped `standard` | 0/16 |
    | `tree_lethal_select` alone | 0/16 |
    | `tactical_solver` + `adaptive_horizon` + `tree_lethal_select` | 0/16 |
    | shipped `fast` + `tree_lethal_select` | 0/16 |

    The bar (reference at least 8, strengthened at most 3) is met by the shipped `standard`, by
    `tactical_solver` and by every configuration with `tree_lethal_select`; the shipped `fast`
    misses it (16/16).
  - Unit test `mech::gapa1::tests::tree_lethal_select_is_deterministic_and_spends_counted_solver_applies`
    (pass): on the 126 searched D2 vector decisions the flag is deterministic, 8 searches ran
    the solver at an in-tree MAIN decision and 7 changed, each of them one that ran the solver.
  - With the flag off nothing changes: `tests/null_config.rs` (the null configuration replays
    the reference bit for bit) and the presets' pins pass.
  - P6-F's unit test `opponent_knobs_model_the_opponent_by_its_own_policy` (W-25 plan item 1).
- **Ablation** (paths under `docs/dev/eval/GAP-A1/`):

  | Row | wins/N (rate, Wilson 95%) | paired difference (Newcombe 95%) | discordant | iterations / applies per game (challenger vs opponent) | lowest pair | evidence |
  | --- | --- | --- | --- | --- | --- | --- |
  | `tree_lethal_select` individual, dev 22710300 | 217/400 (0.542, [0.493, 0.591]) | +0.085 [+0.016, +0.153] | 34/17 | 10002 vs 9930 / 126284 vs 119213 | 11/24 (UB 0.649) | `rows/dev-individual-tree_lethal_select-400.json` |
  | the same, confirm 30271800 (run once) | 843/1600 (0.527, [0.502, 0.551]) | **+0.054 [+0.018, +0.089]** | 128/85 | 10222 vs 9887 / 128675 vs 118656 | 50/108 (UB 0.557) | `rows/confirm-individual-tree_lethal_select-1600.json` |
  | leave-one-out from the shipped `fast` (96 / 240 / 960), dev 22710800 | 202/400 (0.505, [0.456, 0.554]) | +0.010 [-0.063, +0.083] | 29/27 | 14612 vs 14321 / 271722 vs 263337 | 12/28 (UB 0.609) | `rows/dev-loo-tree_lethal_select-fast-400.json` |
  | the same, confirm 30271810 (run once) | 817/1600 (0.511, [0.486, 0.535]) | +0.021 [-0.014, +0.056] | 112/95 | 14565 vs 14125 / 271686 vs 261804 | 48/108 (UB 0.538) | `rows/confirm-loo-tree_lethal_select-fast-1600.json` |
  | leave-one-out from `B_core` at twice the pre-M15 `standard` budget, dev 22710810 | 208/400 (0.520, [0.471, 0.569]) | +0.040 [-0.029, +0.108] | 29/21 | 33153 vs 31619 / 614297 vs 582338 | 13/28 (UB 0.642) | `rows/dev-loo-tree_lethal_select-standard2x-400.json` |
  | the same, confirm 30271820 (run once) | 806/1600 (0.504, [0.479, 0.528]) | +0.007 [-0.029, +0.044] | 114/108 | 33544 vs 32824 / 612374 vs 603871 | 47/108 (UB 0.529) | `rows/confirm-loo-tree_lethal_select-standard2x-1600.json` |

  The individual row against the null row of the same deals (dev 22710300): 200 games changed,
  10 won only with the flag and 3 only without, +0.0175 [-0.0001, +0.0351] per game. The flag
  costs about 8% more applies per game at the reference `standard` budget (the solver's applies
  at in-tree MAIN decisions, counted as auxiliary applies) and 3-5% inside the bundles. The
  confirm-row rule (+0.03 at dev) was applied to every row; the `fast` leave-one-out row
  (+0.010) did not need one and was confirmed anyway, because it is the row an adoption into
  the shipped `fast` would rest on.

  The other opponent settings: `opponent_knobs` individual 196/400, -0.020 [-0.088, +0.049]
  (dev 20600100, `docs/dev/eval/P6-F/rows/dev-opponent_knobs-400.json`); `tactical_solver`
  individual +0.060 [-0.007, +0.126] (dev 20200100), high budget +0.065, confirmation +0.014
  [-0.023, +0.050] (confirm 30200200), and its leave-one-out row from `B_core` +0.075 [+0.009,
  +0.140] (dev 21500200, `docs/dev/eval/M15/rows/dev-loo-tactical_solver-400.json`).
- **tests.json:** T-118.


## Gate

Run after the last code change (release builds of this branch; logs not committed):

| Command | Result |
| --- | --- |
| `cargo fmt --all --check` | clean |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | clean |
| `cargo test --workspace --release --no-fail-fast -- --test-threads=1` | 1,251 passed, 0 failed, 0 ignored (135 test result lines) |
| `cargo test --release -p gcg-search --features slow -- --test-threads=1` | 281 passed, 0 failed (19 test result lines) |
| `target/release/gcg-diff corpus --tier 1 --require-all --jobs 1` | 3804/3804 traces; `search` 9/9 (771 agent decisions), `baselines` 8/8 (625) |
| `scripts/compare_python_vs_rust.sh equivalence --matches 2 --seeds 1 --formats bo1 --pairs red-green-zeon:blue-white-federation --workers 1 --decision-logs off` | 3/3 identical (reference `fast`, `standard`, `strong`); Python baselines identical under hash seeds 1 and 2 |
| `target/release/gcg-diff lints` | lints passed |
| `python3 scripts/cli_help_snapshot.py --check --bin-dir target/release` | matches the binaries (after `--write`: the `weakness` help texts are the only change) |
| `uv run python scripts/check_doc_commands.py --list docs/dev/eval/weakness-status/GAP-A1.md` | 0 blocks (every command is in a `text` fence) |

No Python file was added or changed (`make_sample.sh` is a shell script), so ruff, format and
mypy are unaffected. The tests that pin the reference and the presets pass unchanged:
`tests/null_config.rs` (the null configuration replays the reference bit for bit; the shipped
presets are the M15 values with every other field, the two new ones included, off),
`null_config_traces.rs`, `null_config_search_set.rs` (slow) and the presets' unit tests.

A13 for the analysis tools (`gcg-eval weakness`, `gcg_search::gapa1::api`): no AI effect. They
run only when called by `gcg-eval weakness`, change no search state (the census records a
search through `SearchTracer`, which the search never reads), and no agent, preset, runner or
engine path calls them; the corpus `search` and `baselines` sets and the equivalence smoke
above are unchanged. `gcg-sim`'s and `reference-benchmark`'s outputs do not change: no runner,
report or engine file is touched, the two new flags are off in the shipped presets, and the
equivalence smoke is byte-identical.

## Hand-offs and follow-ups

1. **Adoption decisions (the lead's; the presets are frozen here).** `kill_aware_targeting`
   (W-26: bundle confirm +0.037 [+0.001, +0.074]) and `tree_lethal_select` (W-25: individual confirm +0.054 [+0.018, +0.089], bundle leave-one-out confirms +0.021 (`fast`) and +0.007 (`standard` 2x), and the T-118 fix for the shipped `fast`).
   An adopted flag needs its leave-one-out row from the adopted bundle at the shipped budgets
   (§12.9 step 4), the pin in `crates/gcg-search/tests/null_config.rs` and the preset table.
2. **The M15 configuration files need the two new fields.** `search_config::load` requires every
   mechanism field (W-136), so with this branch `docs/dev/eval/M15/configs/*.json` fail with
   `missing mechanism field "target_multi_rollouts"` (checked:
   `gcg-eval head-to-head search:m15-fast-1.5x.json ...`). Add `"target_multi_rollouts": null`
   and `"tree_lethal_select": false` to their `mechanisms` objects (and to the field list of
   `docs/dev/eval/M15/make_config.py`) before using them as `--search-config` or `part2
   --candidate` files; both values are off, so the files configure the same searches. Not edited
   here: they are M15's records.
3. **W-17's blinded audit.** `bash docs/dev/eval/GAP-A1/audit/make_sample.sh transcripts OUT KEY`
   writes the 120 transcripts; auditors who did not see the key classify the implausible plays
   (passivity, questionable attacks, wasted effects, bad targets, missed lethals; the flags of
   `docs/dev/eval/GAP-A1/weakness/audit-report.json` are candidates), and the confirmed count per
   game with its Wilson interval closes the item. W-119's Rust transcripts tool can replace the
   Python script for the same files.
4. **W-05.** The lead's decision whether the measured break-even share (0.497) replaces the
   scenario bar.
5. **W-25.** The lead's acceptance of the recorded decisions on plan items (2) and (3).
6. **`docs/dev/tests.json` (lead file):** T-109 (record), T-114, T-116 and T-118 pass with the
   tests named in the items.
7. **The WK7 removal-target scenario** is lost for the searcher in every line (no Shields, as
   Python built the puzzle); a variant with Shields would make it a real target choice for the
   8x budget. Not changed here (the frozen scenarios reproduce the existing puzzles).
