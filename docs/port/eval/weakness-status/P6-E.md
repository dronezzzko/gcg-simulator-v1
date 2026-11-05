# P6-E weakness status: root filters and targets (S6)

Package P6-E (docs/port/PLAN.md §12.2, §12.3 S6): W-09, W-10, W-12, W-13, W-14, W-141, with
the mechanism flags `ex_variants`, `typed_fingerprint`, `prune_worlds{K}`, `branch_all_selects`,
`rest_prune_worlds`, `target_rollouts` and `real_idle_line`. Written by the P6-E implementer on
2026-11-02 in worktree branch `worktree-wf_b2d4a964-3bd-1`, from `8ae4767`. WEAKNESSES.md is
not edited here (the lead records the status lines from this file, §12.10). Whether a flag is on
in the shipped presets is the lead's decision; `presets.rs` is unchanged (every P6-E flag off).

## Shared implementation

- `crates/gcg-search/src/mech/p6e/mod.rs`: `PROVIDES` (the seven flags) and the root pipeline
  `root_moves`, in the reference's order (canonical, prune, no-effect check, merge, then the
  target critic), each step replaced by its strengthened form when its flag is on. With every
  P6-E flag off `Composed` calls `reference::root_moves` unchanged. `Shared` (a `Mutex` over
  `RootInfo`, so `Composed` stays `Sync`) carries what the root found to the root priors
  (`priors`: the fail-safe factor `FAIL_SAFE_PRIOR = 0.25`, the EX penalty dropped for EX
  variants, the critic's best raised to the top prior), the root selection (`select`, the
  critic's tie-break), the backup (`record_backup`, the root values the tie-break tests) and the
  budget (`budget`: no close-call extension once the critic has decided). `RootInfo::applies`
  counts every engine apply of the root judgements.
- `crates/gcg-search/src/mech/p6e/worlds.rs`: the first K search worlds (common random
  numbers, no extra determinisation). The core draws `W[0:B]` right after the root moves, so
  the next K draws of a copy of the per-decision generator, after the reference's one
  no-effect draw (kept, so the search's own seeds stay the reference's), are exactly `W[0:K]`.
  Each world is built through the composed `Mechanisms::world`, so a P6-C world mechanism
  shapes it as it shapes the search's. K is `prune_worlds`, else 4 (`DEFAULT_WORLDS`).
- `crates/gcg-search/src/mech/mod.rs`, wiring only: `mod p6e`, the `p6e: p6e::Shared` field of
  `Composed` (built in `Composed::new`), the dispatch of `root_moves`, `budget`, `priors`,
  `select` and `backup`, and `p6e::PROVIDES` in `available`.
- Scenario tests: `crates/gcg-search/tests/scenarios/main.rs` (the scenarios test target: Cargo
  only discovers `tests/<dir>/main.rs`, so the §9.8 directory needs it; other packages add their
  `mod` lines), `p6e_common.rs`, `p6e_w09_ex_payment.rs`, `p6e_w12_hidden_burst.rs`,
  `p6e_w13_freedom.rs`, `p6e_w14_target_critic.rs`. Unit tests:
  `crates/gcg-search/src/mech/p6e/tests.rs` (8 tests).
- The reference is unchanged: `tests/null_config.rs` (4 tests), `tests/null_config_traces.rs`
  and the composed-null tests of `src/mech/tests.rs` pass. `the_ported_no_effect_check_with_its_options_off_is_the_reference`:
  the port of `ai/noop.py` in `p6e/noop.rs`, with its options off, gives `gcg_ai::without_effect`'s
  keys on every searched decision of the D2 vectors, with `branch_choices` off and on.
  `every_flag_runs_on_the_vector_decisions`: every flag alone and all together search every
  third searched decision of the D2 vectors (42) without error; with no P6-E flag the search
  equals the null configuration.

## Measurement protocol (all rows)

Individual rows: `gcg-eval ablation individual --mechanism <flag> --reference standard --games
400 --seed 20005000 --role dev --ledger docs/port/eval/P6-E/seed-ledger.json --workers 8 --out
docs/port/eval/P6-E/individual-<flag>.json`: the challenger `search:null-standard:<flag>` (the
reference `standard` budget, the `fast` preset's starting point, with only the mechanism on)
against `ref:standard`; mirrored blocks of four; forced play/draw (`--choose-first seat0`, the
default); stratified over the 15 tuning pairs of `docs/port/eval/panel.json`; one dev-role
master for every row, so all rows share their deals. The machine ran at a load average of
60-165 during the rows (other agents); nothing depends on time.

Every challenger carries the name `null-standard`, so its per-decision seeds are the null
configuration's. The null row (`--mechanism engine_error_leaf=0`, the null configuration
itself) on the same master is the paired baseline: a game of a row differs from the same game
of the null row only when the mechanism changed a decision. The last two columns compare each
row with the null row game by game (games changed, and win flips: + won only with the
mechanism, - lost only with it).

Paired rows against the null configuration (`vs-null-*`): `gcg-eval ablation leave-one-out
--bundle search:null-standard:<flags> --mechanism <flags off>` with the same master: the null
configuration with the mechanism against the null configuration without it. Their baseline is
not 0.5: the row of a mechanism that never acts on this panel (`vs-null-ex_variants`, the
challenger plays exactly as the opponent) gives 188/400, paired difference -0.060, discordant
19/31: two agents that play alike still do not replay each other's games when their seats
swap (the harness's seeding of the two agents; the cause was not examined further), so they
split deals. Each paired row is therefore read against that no-op row on the same master,
game by game (last two columns).

Binaries: `target/release/gcg-eval` of this branch, copied before the runs. The first copy
ran every row except the four v2 rows; it holds the first target critic (v1, files
`*-v1.json`), and the later edits before the critic change were behaviour-neutral (a 60-game
`ablation individual` of all seven flags, `--seed 1`, gave identical outcomes with the copy and
with a fresh build of the code just before the critic change). No mechanism other than the
critic changed after it, so those rows stand for the final code. The rows of the final critic
(v2, W-14) and the later paired rows (`vs-null-p6e_without_critic`, `vs-null-ex_variants`) ran
on a copy of the final build.

| Row | wins/N | Wilson 95% | paired diff [Newcombe 95%] | discordant | iterations/game | applies/game | games changed vs null | flips vs null |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| null (`engine_error_leaf=0`) | 197/400 | [0.444, 0.541] | -0.015 [-0.080, 0.050] | 21/24 | 9831.4 vs 9820.6 | 118560.6 vs 117536.8 | 0 | +0 -0 |
| `ex_variants=1` | 197/400 | [0.444, 0.541] | -0.015 [-0.080, 0.050] | 21/24 | 9831.4 vs 9820.6 | 118560.6 vs 117536.8 | 0 | +0 -0 |
| `typed_fingerprint=1` | 198/400 | [0.446, 0.544] | -0.010 [-0.076, 0.056] | 22/24 | 9835.1 vs 9822.0 | 118598.7 vs 117578.5 | 4 | +1 -0 |
| `prune_worlds=4` | 196/400 | [0.441, 0.539] | -0.020 [-0.084, 0.044] | 20/24 | 9865.6 vs 9834.0 | 118722.0 vs 117395.7 | 81 | +1 -2 |
| `branch_all_selects=1` | 197/400 | [0.444, 0.541] | -0.015 [-0.080, 0.050] | 21/24 | 9834.7 vs 9827.0 | 118574.8 vs 117556.7 | 9 | +0 -0 |
| `rest_prune_worlds=1` | 197/400 | [0.444, 0.541] | -0.015 [-0.080, 0.050] | 21/24 | 9831.4 vs 9820.6 | 118560.6 vs 117536.8 | 0 | +0 -0 |
| `target_rollouts=1` (v2, final) | 199/400 | [0.449, 0.546] | -0.005 [-0.072, 0.062] | 23/24 | 9831.6 vs 9832.3 | 118602.9 vs 117773.6 | 46 | +2 -0 |
| `target_rollouts=1` (v1) | 195/400 | [0.439, 0.536] | -0.025 [-0.091, 0.042] | 21/26 | 9543.7 vs 9777.5 | 115118.1 vs 117046.1 | 276 | +6 -8 |
| `real_idle_line=1` | 197/400 | [0.444, 0.541] | -0.015 [-0.080, 0.050] | 21/24 | 9823.9 vs 9817.4 | 118459.2 vs 117403.8 | 13 | +0 -0 |
| all seven (critic v2) | 199/400 | [0.449, 0.546] | -0.005 [-0.072, 0.062] | 23/24 | 9867.5 vs 9848.2 | 118733.6 vs 117648.4 | 129 | +4 -2 |
| all seven (critic v1) | 196/400 | [0.441, 0.539] | -0.020 [-0.086, 0.046] | 21/25 | 9588.7 vs 9792.5 | 115479.6 vs 116989.1 | 306 | +8 -9 |

| Paired row against the null configuration | wins/N | Wilson 95% | paired diff [Newcombe 95%] | discordant | iterations/game | applies/game | games changed vs no-op | flips vs no-op |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `vs-null-ex_variants` (no-op baseline) | 188/400 | [0.422, 0.519] | -0.060 [-0.128, 0.009] | 19/31 | 9980.8 vs 9919.5 | 119998.7 vs 119222.1 | 0 | +0 -0 |
| `vs-null-target_rollouts` (v2, final) | 188/400 | [0.422, 0.519] | -0.060 [-0.126, 0.007] | 18/30 | 9958.6 vs 9896.5 | 119755.5 vs 119008.7 | 30 | +1 -1 |
| `vs-null-target_rollouts-v1` | 180/400 | [0.402, 0.499] | -0.100 [-0.169, -0.029] | 17/37 | 9666.3 vs 9891.8 | 116350.1 vs 118760.0 | 270 | +3 -11 |
| `vs-null-p6e_all` (all seven, critic v2) | 188/400 | [0.422, 0.519] | -0.060 [-0.126, 0.007] | 18/30 | 10040.5 vs 9915.6 | 120786.3 vs 119267.5 | 128 | +3 -3 |
| `vs-null-p6e_all-v1` | 180/400 | [0.402, 0.499] | -0.100 [-0.169, -0.029] | 17/37 | 9742.1 vs 9907.4 | 117338.9 vs 118947.4 | 300 | +5 -13 |
| `vs-null-p6e_without_critic` (six flags) | 188/400 | [0.422, 0.519] | -0.060 [-0.128, 0.009] | 19/31 | 10063.7 vs 9938.7 | 121042.8 vs 119481.0 | 105 | +2 -2 |

Root applies. The core does not count the root mechanisms' applies: `RootCx` carries no
effort counter and `run` builds `SearchEffort` after `root_moves`, so `applies/game` above
holds tree and rollout applies only (interface note at the end). `RootInfo::applies` counts
them; on the 42 decisions of `every_flag_runs_on_the_vector_decisions` (reference `standard`
budget, 11,739 iterations, about 280 per search) the root judgements took: `prune_worlds=4`
909 applies (21.6 per search; the strengthened check at K = 1, the 409 of
`branch_all_selects`/`real_idle_line`, is the cost the reference also pays without counting
it, so the four worlds add about 12 per search), `branch_all_selects` 409, `real_idle_line` 409,
`target_rollouts` 231, `rest_prune_worlds` 4, `ex_variants` and `typed_fingerprint` none, all
seven 1,144. At about 12 applies per iteration that is under 1% of a search's applies.

Outcome. No row is promising, so none was confirmed on confirm-role seeds (the brief: "confirm
a promising row"). Every individual row is null: none differs from the null row by more than two
win flips at 400 games. The rows of `ex_variants` and `rest_prune_worlds` are the null row game
for game: no position of the 400 tuning-panel games triggers them (no EX-sensitive card in the
tuning decks, no "while rested" view change), so their game-level effect cannot be measured on
this panel; their evidence is the scenarios. Against the no-op paired row, v2 of the critic, the
six flags without it and all seven are null too (+1 -1, +2 -2, +3 -3). The first form of the
target critic (v1) was negative: +3 -11 against the no-op paired row and +6 -8 against the
null individual row, so it was narrowed (v2, W-14), which is null in both (+1 -1; +2 -0). No
final flag measured a negative effect (the largest net loss is `prune_worlds`' one flip, +1 -2),
and none a positive one; under the brief's rule (keep only a measured non-negative effect) they
are measured null, and which flags go into the shipped presets is the lead's bundle decision
(leave-one-out rows, M15).

## W-09 — Move canonicalization never pays with an EX Resource on purpose

- **Status:** resolved (scenario and unit evidence; the ablation row is null because no tuning
  deck holds an EX-sensitive card, as the entry's "bundle only; few decks hold these cards"
  expected).
- **Resolution as implemented:** `crates/gcg-search/src/mech/p6e/ex.rs`. The EX-sensitivity
  table is read from the effect IR (`sense`: the CPython repr of each ability of a card): a card
  whose own effect reads its payment (`EventAmount(key='ex_used')`, `EventFlag(key='ex_used')`,
  the `wp_gd04_b_played_with_ex`/`wp_gd05_b_played_with_ex` hooks), a card whose triggered
  ability on Commands played reads their EX payment, and a card whose trigger fires when its
  controller's EX Resource is exiled. `canonical_with_ex` keeps one root representative per
  distinct EX count for a paid action of a self-sensitive card, for a Command while a Command
  listener is in play (Battle Area, Base or paired), and for any paid action while an exile
  listener is in play; every other play keeps the reference's fewest-EX representative (the EX
  count enters the key's `variant` as `EX_VARIANT_STEP * ex`). The root prior of an EX variant
  drops `EX_PENALTY` (`p6e::priors`). The reference is unchanged.
- **Deviation:** the plan's "the policy drops the EX penalty when the reward condition can be
  met" is implemented for the root priors only. The rollout policy (`Mechanisms::rollout_choice`)
  is P6-F's extension point and keeps the penalty; in-tree moves use the core's `canonical`.
- **Evidence:**
  - Scenario `tests/scenarios/p6e_w09_ex_payment.rs::ex_sensitive_plays_keep_their_ex_payment`
    (T-111): Gundam Lfrith Ur GD04-020 rested on the field, Witches from Earth GD04-108 (a
    (Dawn of Fold) Command) in hand, four Resources and one EX Resource; paying with EX draws a
    card. Reference `standard` (ROLE_WEIGHTS, agent seeds 1-16): **0/16** pay with EX (pinned;
    the fewest-EX payment is kept and then pruned by the no-effect check). Null configuration:
    0/16. `ex_variants`: **16/16** (bar 14/16).
  - Unit `mech::p6e::tests::the_ex_sensitivity_table_matches_a_walk_of_the_ir`: an independent
    walk of `crates/gcg-effects/ir/registry.json` (every node reachable from each ability) gives
    the same sets as the table over all 1,153 definitions: own GD04-106, GD04-108, GD05-118;
    Command listeners GD04-020, GD04-021, GD04-085; exile listener GD05-018.
- **Ablation:** `ex_variants=1` row above (`docs/port/eval/P6-E/individual-ex_variants.json`):
  197/400, identical to the null row in all 400 games. No leave-one-out row (the bundle is the
  lead's, M15).

## W-10 — Equivalent-move merge fingerprint is conservative and depends on instance numbers

- **Status:** resolved.
- **Resolution as implemented:** `crates/gcg-search/src/mech/p6e/fingerprint.rs`, the
  strengthened `merge_equivalent` (a port of `gcg_ai::fingerprint`, `ai/actions.py:280-393`)
  with typed card references: a card is in flight only when an event payload names it under a
  card key (`attacker`, `blocker`, `pilot`, `source`, `subject`, `target`), not when any event
  value equals its uid; its once-per-turn references are the once keys whose *card positions*
  (by layout: `(aid | tag, host, zone_seq)` position 1, `(-8 | -7, aid, host, uid, zone_seq)`
  positions 2-3, `(-8 | -7, -1, effect, serial, uid, zone_seq)` position 4, `(-9, uid, turn)`
  position 1) hold its uid, and only those positions are rewritten to "self". A key of another
  layout falls back to the raw comparison. History, lasting effects, battles, frame buffers and
  chosen variables already hold typed uids. The engine is unchanged: the typing lives in the
  layout table (the engine's `OnceKey`/`EventPayload` documentation), not in a new `CardRef`
  type in the engine records, which would change an engine contract outside this package.
- **Evidence:**
  - Unit `mech::p6e::tests::typed_fingerprints_ignore_numbers_that_merely_equal_a_uid`: two GM
    copies that can attack the player merge in both forms; after a once record
    `(0, gundam, a)` whose zone-sequence position equals copy A's uid, the reference keeps the
    two attacks apart (pinned) and the typed fingerprint merges them; a record that names A as
    its host keeps them apart in the typed form too.
  - Property `mech::p6e::tests::moves_the_typed_merge_groups_lead_to_equivalent_successors`:
    over every decision of 1,000 random games on the AI test decks (up to 300 decisions each),
    every move the typed merge groups with an earlier one leads to a successor whose uid-free
    summary (turn, active player, winner, pending decision kind/player/option count, every card
    as `(owner, def, zone, rested, damage, pilot def)` sorted, lasting effects with target defs)
    equals the earlier move's: 4,913 merged pairs, 17 of them merged only by the typed form.
- **Deviation:** the property compares uid-free summaries instead of state digests after
  swapping the two uids (a uid swap of the canonical state would need a uid-typed map of every
  canonical field). A multi-card selection is finished (DONE) before the comparison: the engine
  offers later candidates only after an earlier one (combinations, not permutations), so the
  reference too merges a first pick of the earlier copy with a first pick of the later one
  (game 11 of the property found it; the reference's merge keeps the earlier one, whose
  continuations include every outcome of the later one up to the copies' symmetry).
- **Ablation:** `typed_fingerprint=1` (`individual-typed_fingerprint.json`): 198/400 [0.446,
  0.544], 4 games changed against the null row, +1 -0.

## W-12 — Root pruning is judged in one fixed determinization by the deterministic policy, with silent caps

- **Status:** resolved.
- **Resolution as implemented:**
  - `prune_worlds{K}` (`p6e/noop.rs`, `p6e/prune.rs`, `p6e/worlds.rs`): the no-effect check
    judges each spending move in each of the first K search worlds in order; a move is pruned
    only when it is without effect in all of them, the first world with an effect ends its
    judgement (so a move with an effect in world 0 costs the reference's one judgement), and a
    move kept after some world judged it without effect, or unsure, is *penalised*
    (`FAIL_SAFE_PRIOR` on its root prior). The idle 0-AP attack prune keeps every attack the
    reference's seed-0 trial dropped when it matters in one of the K worlds. With
    `prune_worlds` (or `branch_all_selects`), a judgement whose resolution, continuation,
    replay or branching stopped at a cap is unsure (`Verdict::Unsure`) instead of being
    compared as complete.
  - `branch_all_selects` (`p6e/noop.rs`, `Judging::all_branches`): the branch check branches
    over every SELECT of the player in the move's resolution (depth first, every combination),
    up to `BRANCH_NODE_CAP = 24` branch states counted in the applies; reaching the cap makes
    the judgement unsure.
  - The reference keeps its single worlds, first-SELECT branching and 400 caps (`gcg-ai`
    unchanged). The plan's "reusing the common-random-number worlds, so no extra
    determinization" holds: world k is the search's world k (`worlds.rs`).
- **Evidence:**
  - Scenario `tests/scenarios/p6e_w12_hidden_burst.rs::the_no_effect_check_keeps_a_move_that_matters_in_some_worlds`
    (T-112), the entry's "hidden-dependent effect": the searcher's only Unit GD04-031 (3/2) can
    attack the player; Witches from Earth GD04-108 ("reduce the next damage it receives by 2"
    during this turn) matters only when the destroyed Shield holds Battle of Aces GD01-111
    (【Burst】2 damage to an enemy Unit); 20 of the defender's 28 unseen cards are Battle of
    Aces. The builder's check: the reference no-effect check prunes Witches in **52/100**
    world seeds (pinned; the decision differs between worlds). Over agent seeds 1-16 the
    reference `standard` keeps it in **8/16** (pinned), the null configuration 8/16,
    `prune_worlds=4` in **14/16** (bar 14/16).
  - Unit `mech::p6e::tests::a_forced_cap_hit_keeps_the_move`: Siege Ploy ST02-014 when the only
    enemy Unit is rested is without effect at the full cap (reference and port); with a cap of
    no policy decision the reference's comparison of the cut lines says "without effect"
    (pinned: compared as complete), the fail-safe says unsure, and the strengthened check keeps
    the move as penalised.
- **Ablation:** `prune_worlds=4` (`individual-prune_worlds.json`): 196/400 [0.441, 0.539], 81
  games changed, +1 -2 against the null row; `branch_all_selects=1`
  (`individual-branch_all_selects.json`): 197/400, 9 games changed, +0 -0. Leave-one-out: the
  lead's bundle (M15).

## W-13 — The "while rested" Rest-target exception was added after the measurements and never fired at game level

- **Status:** resolved for the search and the scenario; the e2e part is not done (below).
- **Resolution as implemented:** the reference reproduces the fixed check bit-exactly (`gcg-ai`,
  unchanged). `rest_prune_worlds` (`p6e/prune.rs`): when the reference prunes rested Rest
  targets (its exception found no view change in its seed-0 trial world), the exception
  (`_resting_changes_more_than_the_unit`, ported per world) is judged again in each of the K
  search worlds, and the prune is undone when resting a candidate changes the derived view in
  any of them.
- **Evidence:**
  - Scenario `tests/scenarios/p6e_w13_freedom.rs::both_searches_rest_the_rested_unit_and_keep_the_lethal`
    (T-113): Amuro Ray pairs with the Gundam (【When Paired】rest 1 enemy Unit with 5 or less
    HP); the targets are the active Freedom Gundam GD03-070 ("While this Unit is rested,
    friendly Shields can't receive battle damage from enemy Units.") and a rested Zaku II; the
    defender has one Shield, the GM and the Gundam can attack. Resting the Zaku keeps the
    lethal. Agent seeds 1-16: reference `standard` **16/16** (pinned), null configuration
    16/16, `rest_prune_worlds` **16/16**.
  - Negative control `mech::p6e::tests::without_the_exception_the_freedom_lethal_is_missed`:
    the same position with the prune applied without the exception leaves only Freedom, and
    the search (seeds 1-16) rests Freedom every time: **0/16**, so the scenario discriminates;
    the reference prune keeps both targets.
- **Not done:** the entry's e2e (an "effect coverage" deck set with GD03-070 and every card
  that reads its own rested state, and a firing count of each prune exception in the
  instrumented trace over 100 games) needs a `gcg-eval` deck set and trace counter, outside
  this package's files. The 400-game no-harm guard is the `rest_prune_worlds` row (on the
  tuning panel): identical to the null row in all 400 games.
- **Ablation:** `rest_prune_worlds=1` (`individual-rest_prune_worlds.json`): 197/400, 0 games
  changed.

## W-14 — Close target choices stay noisy: the <Repair> ping puzzle is solved about a third of the time

- **Status:** resolved (24/24 against the reference's 7/24 on the scenario; the final form, v2,
  is null at game level, the first form v1 was negative and was replaced); part (2) and the
  shipped-puzzle-suite step are not done (below).
- **Resolution as implemented:** `target_rollouts` (`p6e/critic.rs`, `p6e/mod.rs`). At an
  effect-target decision with two or more root moves the critic judges each root move by what it
  leaves: in each of the K search worlds the move is played, the effect is resolved with the
  deterministic policy, both players then idle to the search horizon (ending the main phase,
  passing, not blocking, declining, choosing nothing wherever the decision allows; the
  deterministic policy elsewhere), so end-of-turn healing (<Repair>), the end of "during this
  turn" effects and untapping apply; the line is scored by the evaluation's log-odds (`score`),
  where the calibrated leaf value's squeeze near `LEAF_FLOOR` does not hide the difference. A
  root move is the critic's *best* when its paired horizon difference with every other root move
  exceeds `CRITIC_Z = 2` paired standard errors over the K worlds and the evaluation right after
  the resolution does not already favour it (mean immediate difference at most zero: the critic
  speaks only for what the turn clock adds, v2). The best seeds the root prior
  (raised to the top prior) and breaks the search's ties: from the middle of the budget on, a
  root selection of another move goes to the best unless the search's own root values favour
  that move by more than `SEARCH_Z = 2` standard errors; such a decision does not extend its
  budget for a close call. A DONE option is a root move, so "no target" is scored like a target
  (part 3). When the targets leave the same position, or differ at once, the critic is silent
  and the search is the reference's.
- **The first form (v1) and why it changed:** v1 had no immediate-evaluation condition, so its
  idle lines also decided every target choice whose difference shows at once (removal targets,
  rests, damage). Its paired row against the null configuration lost 11 and won 3 of the games
  it changed relative to the no-op paired row (`vs-null-target_rollouts-v1.json`: 180/400
  [0.402, 0.499], against the no-op row's 188/400), and its individual row +6 -8 against the
  null row. v2 (final) restricts the critic to clock-only
  differences; it still solves the puzzle 24/24 (the immediate difference there is exactly 0),
  changes no search of the D2 vector decisions (`every_flag_runs_on_the_vector_decisions`:
  0/42), and measures null (below).
- **Tried and not kept:** a critic of 4 stochastic policy rollouts per world (composed rollout
  policy, common seeds) scored by the leaf value, then by log-odds: 9/24 and 10/24 on the
  puzzle (its lines favour pinging the Guncannon by about 0.05 log-odds; the idle critic's
  difference is the HP weight, 0.231, in every world).
- **Evidence:** scenario `tests/scenarios/p6e_w14_target_critic.rs::the_target_critic_solves_the_repair_ping`
  (T-114), the Rust port of the <Repair> ping puzzle (`tests/ai/test_puzzles_targets.py:68-86`,
  `aiport::positions::damage_the_unit_that_cannot_repair`), agent seeds 1-24: shipped reference
  `standard` (ROLE_WEIGHTS) **7/24** (pinned; Python's recorded 7 of 24), null configuration
  10/24, `target_rollouts` **24/24** (bar 20/24).
- **Not done:** (2) "several rollouts per world inside the search's iterations at target
  decisions" needs a core hook (a root move's iteration running several rollouts from its
  world), which the `Mechanisms` trait does not have; the critic's own multi-rollout variant was
  measured on the puzzle and not kept (above), so no multi-rollout or "both" ablation row
  exists. Adding the puzzle to the shipped-preset puzzle suite (`tests/ai_puzzles.rs`, not a
  P6-E file) follows the lead's adoption of `target_rollouts` in the presets.
- **Ablation:** v2 (final): individual `target_rollouts=1` (`individual-target_rollouts.json`)
  199/400 [0.449, 0.546], paired difference -0.005 [-0.072, 0.062], discordant 23/24,
  iterations per game 9831.6 vs 9832.3, applies per game 118602.9 vs 117773.6, 46 games
  changed against the null row, +2 -0; paired against the null configuration
  (`vs-null-target_rollouts.json`) 188/400 [0.422, 0.519], -0.060 [-0.126, 0.007], discordant
  18/30, which is the no-op paired row's result (30 games changed, +1 -1). v1: individual
  195/400 [0.439, 0.536], 276 games changed, +6 -8, iterations per game 9543.7 (v1 skipped many
  close-call extensions); paired 180/400, +3 -11 against the no-op row (above). No
  leave-one-out row of the full bundle (the lead's, M15).

## W-141 — The no-effect branch check compares a branch with a policy line that may replay the move

- **Status:** resolved.
- **Resolution as implemented:** `real_idle_line` (`p6e/noop.rs`, `Excluded`, `policy_step`):
  every policy move of an idle line (the replay's policy continuation and the fallback idle
  line of the branch check) comes from the decision's options without any play or activation of
  the move's card ("the move" covers every variant: same kind, same card); the deterministic
  policy is applied over the remaining options (`option_scores`/`pick`). The baseline is real
  idling. The reference keeps its policy line.
- **Evidence:** unit `mech::p6e::tests::the_real_idle_line_never_replays_the_move_under_test`:
  game 53 of greedy self-play on the AI test decks (agent seeds 53 and 1053), decision 24, an
  action step of player 0 (found by comparing the null configuration with `real_idle_line` on
  every spending decision of 60 greedy games; the other three differences were Commands the
  reference kept only because its idle line re-cast them). With the no-effect world of
  `search-standard`'s first decision, the reference's check prunes the Command (pinned) and its
  idle line casts that Command; the strengthened check keeps it (`Verdict::Effect`) and its idle
  line takes no action of the Command's card.
- **Ablation:** `real_idle_line=1` (`individual-real_idle_line.json`): 197/400, 13 games
  changed, +0 -0. Leave-one-out with W-12: the lead's bundle (M15).

## Gate (after the last edit)

Run after the last code edit (the critic's v2 change), in this worktree:

| Command | Result |
| --- | --- |
| `cargo fmt --all --check` | exit 0 |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | exit 0 |
| `cargo test --workspace --release` | 992 passed, 0 failed (980 before this package; +8 P6-E unit tests, +4 scenario tests) |
| `cargo test --release -p gcg-search` | every target passes: lib 28 (8 of them P6-E), `null_config` 4, `null_config_traces` 1, `scenarios` 4 (W-09, W-12, W-13, W-14), the AI ports, doc tests 10 |
| `target/release/gcg-diff corpus --tier 1 --require-all` | 3804/3804 traces (baselines 8, coverage 50, full 4, fuzz 100, search 9, tests 3213, vanilla 40, views 280, worlds 100) |
| `COMPARE_OUT=build/compare-p6e-2 scripts/compare_python_vs_rust.sh equivalence --matches 2 --seeds 1 --formats bo1 --pairs red-green-zeon:blue-white-federation --workers 1 --decision-logs off` | `equivalence: 3/3 identical (fast 1/1; standard 1/1; strong 1/1; decision-log off 3/3)` |

The seeds of every role run are in `docs/port/eval/P6-E/seed-ledger.json` (dev role, master
20005000), a package ledger so that parallel packages do not write one file; the lead merges it
into `docs/port/eval/seed-ledger.json`.

## Notes for the lead (interface and integration)

1. **`Composed` state.** `crates/gcg-search/src/mech/mod.rs` gained, besides the module and
   dispatch lines, the field `p6e: p6e::Shared` (built in `Composed::new`): the root moves of a
   search must hand their findings to the root priors, selection, backup and budget of the
   same search, and no context carries them. `Shared` is a `Mutex`, so `Composed` stays `Sync`
   (P6-D's root ensemble may share it across trees; each tree's `root_moves` call then resets
   it, and the information is a function of the root, so it is the same for every tree).
2. **Root applies are not counted by the core.** `RootCx` has no effort counter and `run`
   creates `SearchEffort` after `root_moves`, so `applies_aux` stays 0 for the root mechanisms
   (and the ablation rows' applies per game omit them). Proposed wiring (lead or P6-D): a
   trait method `fn root_applies(&self) -> u64 { 0 }` that `Composed` answers from
   `p6e::RootInfo::applies`, added by `run` to `effort.applies_aux` after `root_moves` (or an
   `aux: &mut u64` field of `RootCx`).
3. **`flags.rs` doc comments name other items** than WEAKNESSES/PLAN for three P6-E flags:
   `branch_all_selects` says W-13 (it is W-12's), `rest_prune_worlds` says W-14 (W-13's),
   `target_rollouts` says W-141 (W-14's); `real_idle_line` (W-141) names none. `flags.rs` is not
   a P6-E file.
4. **The scenarios target.** `crates/gcg-search/tests/scenarios/main.rs` is new and shared:
   Cargo only builds `tests/<dir>/main.rs` (or `[[test]]` entries in `Cargo.toml`), so the PLAN
   §9.8 directory needs it. It includes `../aiport/mod.rs` and `../common/mod.rs` by path; other
   packages add their `mod` lines (a union merge).
5. **Composition with other packages.** The K worlds are the core's `W[0:K]` under the
   reference's seed layout (`worlds.rs`); a P6-D root ensemble that seeds trees by
   `derive_seed(decision_seed, "tree", t)` needs `Worlds::new` to take the tree's seeds. The
   critic's lines use the deterministic policy `gcg_ai::choose` (not the composed rollout
   policy), and its score is `gcg_ai::score` with `cfg.weights`, so a P6-G refit reaches it.
   The EX penalty is dropped in the root priors only; P6-F's rollout policy would need the
   same rule for W-09's "the policy drops the EX penalty".
6. **Leave-one-out rows are not centred.** A leave-one-out row whose mechanism never acts
   (`vs-null-ex_variants.json`, dev master 20005000) reports 188/400, paired difference -0.060
   [-0.128, 0.009], discordant 19/31, not 200/400 and 0/0. A leave-one-out row should be read
   against such a no-op row on the same master (or the harness should seed both agents of a
   leave-one-out row so that identical agents replay mirrored games). This concerns the M15
   leave-one-out protocol (P6-I's `gcg-eval ablation`).
7. **Left to the lead:** leave-one-out rows of the adopted bundle (M15); W-14's puzzle in the
   shipped-preset suite (`tests/ai_puzzles.rs`) once `target_rollouts` is adopted; W-13's
   coverage deck set and prune-exception firing count (`gcg-eval` tooling, W-21).
