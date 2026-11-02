# P6-C weakness status: hidden information (S4, S5)

Package P6-C (docs/port/PLAN.md §12.2, §12.3 S4 and S5): W-05, W-07, W-08, W-134, W-139.
Written by the P6-C implementer on 2026-11-02 in worktree branch
`worktree-wf_94545b98-29a-4`, from `8af9b0e` (M13). WEAKNESSES.md itself is not edited here
(the lead records the status lines from this file, §12.10). Default-on in the shipped presets is
the lead's decision; this file says which mechanisms measured non-negative and recommends.

Code commits: `09b0f7b` (the mechanisms), `fa6e381` (scenario and unit evidence; the
opponent slices also record their child key, a diagnostic field that changes no decision), and
`ba9dc76` (this file, the last evidence, one module doc comment). Every ablation row ran a
`gcg-eval` built from `09b0f7b`, except the two `belief_worlds=16` rows, which ran one built from
`fa6e381` (the same decisions).

## Summary

| Item | Flag | Status | Game-level effect | Scenario |
| --- | --- | --- | --- | --- |
| W-05 | `opponent_buckets{m}` | resolved (implemented, measured null, not recommended) | null at both budgets; `m` sweep {4, 8, 16, 32}: `m = 4` | node-level fix shown; the root-decision bar (strengthened holds back in ≥ 13/16) **not met** |
| W-07 | `belief_worlds{M}`, `belief_last_n{N}` | resolved (implemented, measured null, not recommended; one reconstructed decision, see the deviations) | reference budget +0.015 / 0.000 / +0.020 (M = 4 / 8 / 16); 2x shipped `standard` +0.035 (M = 4, dev) not confirmed (-0.013 on 1,600 confirm games), +0.020 (M = 16, dev) | **met**: reference attacks 16/16, `belief_worlds=64` holds back 12/16 |
| W-08 | `exact_pools` | **open: blocked** on an engine interface (proposed below) | — | — |
| W-134 | `opponent_list = archetype` | **open: blocked** on the same engine interface | — | — |
| W-139 | `opponent_view_share` | resolved in part (implemented for root decisions whose options name no card hidden from the opponent; measured null, not recommended) | 0.000 at both budgets | world-level fix shown; the root-decision bar (strengthened keeps the trick in ≥ 12/16) **not met** |

## What was built

| File | Content |
| --- | --- |
| `crates/gcg-search/src/mech/p6c/mod.rs` | `PROVIDES` (`belief_last_n`, `belief_worlds`, `opponent_buckets`, `opponent_view_share`), `check` (refuses `belief_last_n` without `belief_worlds` or `= 0`, and `belief_worlds < 2`), the per-search state `Search` (the searcher, the opponent-view test of the root, the root's children at the tree start, the running iteration's world kind and its opponent walk, per opponent child the statistics slices), `world`, `select` (opponent nodes), `after_select` (the root: the iteration's world index `k = visits - base`), `backup` |
| `mech/p6c/buckets.rs` | the bucket of the opponent's hand (bit 1: an 【Action】 or 【Main】/【Action】 Command its level and open Resources cover; bit 0: a <Blocker> Unit card it can deploy) and `command_timing` |
| `mech/p6c/belief.rs` | the reconstructed decision (`Evidence`), its likelihood under the playout policy's MAIN scores, the candidate seeds, the resampling offsets and the belief world |
| `mech/p6c/view.rs` | the opponent-view share, the root test and the redraw from the opponent's view |
| `mech/p6c/tests.rs` | 11 unit tests (below; `view.rs` holds one more) |
| `crates/gcg-search/tests/scenarios/p6c_common.rs`, `p6c_w05_fusion.rs`, `p6c_w07_open_resources.rs`, `p6c_w139_held_trick.rs` | the scenario tests (target `scenarios`) |
| `docs/port/eval/P6-C/` | the ablation reports with every game, `seed-ledger.json` |

Wiring outside the owned files (flagged to the lead):

- `crates/gcg-search/src/mech/mod.rs` (the p6c lines): `pub(crate) mod p6c;`; the `p6c` field
  of `Composed` and `ComposedState` (`Shared::new`, `from_search`, `into_search`,
  `tree_copy` for a root tree of S1, which copies the decision's findings only);
  `Composed::aux_applies` adds P6-C's candidate worlds; the `world` override (P6-C when
  `belief_worlds` or `opponent_view_share` is on); `priors` (all packages' priors moved
  unchanged into `Composed::composed_priors`, then `p6c::note_priors`); `select` (P6-C first at
  opponent nodes, then P6-E or the reference, then `p6c::after_select`); `backup` (an
  opponent-view iteration is backed up by P6-C, every other by P6-D or the reference, then
  P6-C's slices and P6-E's record); `root_moves` (`p6c::note_root` before the root moves);
  `available` reads `p6c::PROVIDES`; `validate` ends with `p6c::check`; the
  `expect(dead_code)` on `WorldCx::cfg` is gone (P6-C reads it).
- `crates/gcg-search/tests/scenarios/main.rs`: the four `p6c_*` modules.
- `crates/gcg-search/src/presets.rs` (test `unavailable_mechanisms_are_refused`) and
  `crates/gcg-eval/src/agent_spec.rs` (test `search_overrides_reach_the_mechanism_flags`): their
  example of a refused flag was `opponent_buckets=8`, which this build provides; both now use
  `opponent_list=archetype` (still refused) and `agent_spec` checks that
  `opponent_buckets=8` parses. The assertions are as strict as before. `crates/gcg-eval/tests/p6m_cli.rs`
  (`w134_opponent_list_reaches_the_search_flag`, archetype refused) stays valid unchanged.

The null configuration is unchanged: with every P6-C flag off no P6-C function runs (`Composed`
calls the reference), and the null-configuration tests pass (gate below).

## Measurement protocol

- **Individual rows** (PLAN §12.4): `gcg-eval ablation individual --mechanism <flags>
  --reference standard --games 400 --seed 20300001 --role dev --ledger
  docs/port/eval/P6-C/seed-ledger.json --workers 10 --out docs/port/eval/P6-C/individual-<flags>.json`:
  `search:null-standard` (the reference `standard` budget, every mechanism off) plus the
  mechanism against `ref:standard`, 400 BO1 games in mirrored blocks of four over the 15 tuning
  pairs, forced play/draw (A24). All rows share their deals.
- **High-budget rows** (the lead's (b)): `gcg-eval ablation leave-one-out --bundle
  search:standard:iterations_per_option=256,min_iterations=640,max_iterations=2560,<flag>
  --mechanism <flag off> --games 400 --seed 20300002 --role dev`: `search:standard` (shipped
  mechanisms, i.e. `engine_error_leaf`) at 2x its shipped budget with the mechanism, against the
  same without it.
- **Confirm rows** (the lead's (c)): rows with a paired difference of at least +0.03 rerun once
  with 1,600 games on `--seed 30300001 --role confirm`.
- Paired difference: the W-42 statistic (per deal), Newcombe 95%; discordant counts McNemar's
  b/c; iterations and applies per game challenger / opponent (applies include P6-C's candidate
  worlds, one per candidate scored).

| Row | Wins/N | Wilson 95% | Paired difference [Newcombe 95%] | Discordant b/c | Iterations per game | Applies per game | Seed (role), games | Evidence |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 2x `standard`: `belief_worlds=4` vs off | 790/1600 = 0.494 | [0.469, 0.518] | -0.013 [-0.044, +0.019] | 80/90 | 32029.9 / 32268.9 | 399901.5 / 373145.0 | 30300001 (confirm), 1600 | `docs/port/eval/P6-C/confirm-high-budget-belief_worlds-4.json` |
| 2x `standard`: `belief_worlds=16` vs off | 204/400 = 0.510 | [0.461, 0.559] | +0.020 [-0.053, +0.092] | 30/26 | 32963.9 / 32955.6 | 506853.0 / 388092.5 | 20300002 (dev), 400 | `docs/port/eval/P6-C/high-budget-belief_worlds-16.json` |
| 2x `standard`: `belief_worlds=4` vs off | 207/400 = 0.517 | [0.469, 0.566] | +0.035 [-0.040, +0.109] | 33/26 | 32919.0 / 32804.7 | 415918.2 / 388094.2 | 20300002 (dev), 400 | `docs/port/eval/P6-C/high-budget-belief_worlds-4.json` |
| 2x `standard`: `opponent_buckets=4` vs off | 196/400 = 0.490 | [0.441, 0.539] | -0.020 [-0.094, +0.054] | 27/31 | 33066.0 / 33107.2 | 391066.4 / 390722.6 | 20300002 (dev), 400 | `docs/port/eval/P6-C/high-budget-opponent_buckets-4.json` |
| 2x `standard`: `opponent_view_share=0.25` vs off | 200/400 = 0.500 | [0.451, 0.549] | +0.000 [-0.071, +0.071] | 27/27 | 32392.4 / 32598.3 | 382912.1 / 383899.0 | 20300002 (dev), 400 | `docs/port/eval/P6-C/high-budget-opponent_view_share-0_25.json` |
| individual: `belief_worlds=16` | 204/400 = 0.510 | [0.461, 0.559] | +0.020 [-0.053, +0.092] | 30/26 | 9996.8 / 9892.1 | 144634.0 / 119733.6 | 20300001 (dev), 400 | `docs/port/eval/P6-C/individual-belief_worlds-16.json` |
| individual: `belief_worlds=4` | 203/400 = 0.507 | [0.459, 0.556] | +0.015 [-0.057, +0.087] | 29/26 | 10027.5 / 9801.8 | 126960.5 / 118237.0 | 20300001 (dev), 400 | `docs/port/eval/P6-C/individual-belief_worlds-4.json` |
| individual: `belief_worlds=8` | 200/400 = 0.500 | [0.451, 0.549] | +0.000 [-0.070, +0.070] | 26/26 | 9872.2 / 9843.7 | 130205.2 / 118792.8 | 20300001 (dev), 400 | `docs/port/eval/P6-C/individual-belief_worlds-8.json` |
| individual: `engine_error_leaf=false` | 199/400 = 0.497 | [0.449, 0.546] | -0.005 [-0.077, +0.067] | 27/28 | 9841.5 / 9841.3 | 118683.1 / 117791.5 | 20300001 (dev), 400 | `docs/port/eval/P6-C/individual-engine_error_leaf-false.json` |
| individual: `opponent_buckets=16` | 198/400 = 0.495 | [0.446, 0.544] | -0.010 [-0.083, +0.063] | 27/29 | 9767.0 / 9830.0 | 118108.6 / 117960.5 | 20300001 (dev), 400 | `docs/port/eval/P6-C/individual-opponent_buckets-16.json` |
| individual: `opponent_buckets=32` | 196/400 = 0.490 | [0.441, 0.539] | -0.020 [-0.091, +0.051] | 25/29 | 9790.0 / 9819.6 | 118067.3 / 117340.0 | 20300001 (dev), 400 | `docs/port/eval/P6-C/individual-opponent_buckets-32.json` |
| individual: `opponent_buckets=4` | 203/400 = 0.507 | [0.459, 0.556] | +0.015 [-0.056, +0.086] | 28/25 | 9937.2 / 9887.2 | 119556.8 / 118690.0 | 20300001 (dev), 400 | `docs/port/eval/P6-C/individual-opponent_buckets-4.json` |
| individual: `opponent_buckets=8,belief_worlds=4,opponent_view_share=0.25` | 193/400 = 0.482 | [0.434, 0.531] | -0.035 [-0.104, +0.035] | 22/29 | 9959.7 / 10036.5 | 125742.6 / 119994.1 | 20300001 (dev), 400 | `docs/port/eval/P6-C/individual-opponent_buckets-8-belief_worlds-4-opponent_view_share-0_25.json` |
| individual: `opponent_buckets=8` | 203/400 = 0.507 | [0.459, 0.556] | +0.015 [-0.056, +0.086] | 28/25 | 9914.9 / 9888.2 | 120492.1 / 119423.3 | 20300001 (dev), 400 | `docs/port/eval/P6-C/individual-opponent_buckets-8.json` |
| individual: `opponent_view_share=0.25` | 200/400 = 0.500 | [0.451, 0.549] | +0.000 [-0.073, +0.073] | 28/28 | 9952.2 / 9911.3 | 120460.5 / 118543.1 | 20300001 (dev), 400 | `docs/port/eval/P6-C/individual-opponent_view_share-0_25.json` |

## W-05 — Strategy fusion: opponent nodes pool statistics across hidden worlds

- **Status:** resolved: the mechanism is implemented and measured, and the unit test shows the
  fusion removed at the opponent's node; measured null at game level at both budgets, so it is
  not recommended for the presets. The entry's root-decision bar (reference attacks in at least
  8 of 16 seeds, strengthened avoids it in at least 13) is **not met**: in the fusion position
  the reference attacks in 16/16 and `opponent_buckets=4` and `=8` also attack in 16/16.
- **Resolution as implemented:** `opponent_buckets{m}` (`mech/p6c/buckets.rs`, `p6c::select`,
  `p6c::backup`). Every opponent child keeps one slice of `(visits, total, avail)` per bucket of
  the opponent's determinised hand in the iteration's world: bit 1 when the hand holds an
  【Action】 or 【Main】/【Action】 Command whose printed level and cost the opponent's level and
  active Resources cover, bit 0 when it holds a <Blocker> Unit card it can deploy (conditions
  and targets are not checked, so the abstraction is cheap and reads the world's hand and the
  public Resources only). At an opponent node the bucket of the world is computed; once that
  bucket has `m` visits at the node, the selection is the reference's UCB with the progressive
  prior (`ai/mcts.py:290-302`) over the bucket's slice (a child the slice has not seen is
  valued at its pooled mean and explored as one visit; first maximum wins); below `m` it is the
  reference's selection over the pooled statistics, so no reply starves. Each selection joins
  the iteration's walk with its bucket, and the backup adds the line's value, from the
  opponent's point of view, to that slice (once per iteration under `visited_set_backup`). The
  pooled statistics are backed up as in the reference.
- **`m` sweep** (individual rows, dev 20300001): `m = 4` +0.015, `8` +0.015, `16` -0.010,
  `32` -0.020; the largest paired difference wins and ties go to the smaller value: **`m = 4`**.
- **Evidence:**
  - `mech::p6c::tests::w05_bucketed_opponent_nodes_reply_by_hand` (T-105): the fusion position
    (Gundam GD01-013 against a GD02-079 Blocker; 13 of the defender's 46 unseen cards are
    GD02-118, which bounces an attacker battling a <Blocker>), two seeds, 2,000 iterations.
    Visits to Block among the block decision's visits, bucketed (`m = 4`): no trick 50/724 and
    60/725, trick 256/276 and 253/275; pooled (the reference's selection, slices recording
    only): no trick 267/724 and 129/725, trick 110/276 and 36/275. Bucketed nodes block in the
    trick worlds and let the attack through otherwise; pooled nodes give one reply for both.
    Pass.
  - `p6c_w05_fusion::w05_fusion_decisions_are_recorded`: attacks of 16 seeds at the reference
    `standard` budget: reference 16, `opponent_buckets=4` 16, `=8` 16 (pinned). The attack stays
    the better move in the search's own evaluation even against the bucketed reply (at 4,000
    iterations: attack 0.313 against 0.299 for ending the main phase; with the trick known to
    the searcher, 0.292 against 0.310): the bounce costs little in the leaf evaluation. Sweeps of
    other tricks (GD01-120's AP+3, trick shares 28% to 77%) and attackers (GD02-078) gave the
    same root decision. Pass (as a record).
  - `mech::p6c::tests::every_flag_runs_on_the_vector_decisions_and_reads_the_information_set_only`:
    on 32 searched decisions of the D2 vectors, the search with `opponent_buckets=8` from two
    other true states of each information set (`determinize` and `permute_hidden` of the true
    state) gives the identical result and package state (the permutation invariance of
    `tests/ai/test_information_set.py` for the mechanism); it changes 20 of the 32 decisions'
    statistics. Pass.
- **Ablation:** the individual rows `opponent_buckets=4/8/16/32` and the high-budget row
  `opponent_buckets=4` in the table above (-0.020 [-0.094, +0.054] at 2x the shipped `standard`
  budget). Not confirmed (no row reached +0.03). Leave-one-out from the integrated bundle is the
  lead's (M15).
- **Not done:** the census of the 12 remaining observation-key splits that W-05's verification
  names (a `gcg-eval` tool, outside the owned files) and their observation keys or regret
  intervals.

## W-07 — Determinization samples hidden cards uniformly

- **Status:** resolved: implemented and measured; the scenario bar is met (reference attacks
  into the inferred trick in 16/16 seeds, `belief_worlds=64` holds back in 12/16). Game level:
  null. At the reference `standard` budget `belief_worlds=4` measured +0.015 and `=8` 0.000; at
  2x the shipped `standard` budget `=4` measured +0.035 [-0.040, +0.109] on dev seeds, which did
  not hold on 1,600 confirm-role games (-0.013 [-0.044, +0.019]). Not recommended for the
  presets; it stays in the code behind its flag.
- **Resolution as implemented:** `belief_worlds{M}` with `belief_last_n{N}` (`mech/p6c/belief.rs`,
  `p6c::world`). World k is chosen among M candidates `determinize(st, player, s_j)` with
  `s_0 = W[k]` (the reference's world) and `s_j = derive_seed(W[k], "belief", j)`, each weighted
  by the likelihood of the opponent's reconstructed public decision under the opponent model,
  and drawn by systematic resampling: offset `u_k = frac(u + k (√5 - 1) / 2)` with `u` the first
  `random()` of `SplitMix64(derive_seed(W[0], "belief-resample"))` (the tree's seed), world k
  being the first candidate whose cumulative weight exceeds `u_k` times the total. Every
  candidate scored is counted as one auxiliary apply. Only the information-set state, the
  public Resources and each candidate's own cards enter, so the weights are honest.
- **Deviations from the plan text (recorded for the lead):**
  - *The decision weighed.* The plan weighs the opponent's last N public decisions replayed on
    the candidate world. The search sees the information-set state of its own decision only
    (agents are called at their own decisions; the state keeps no earlier state and its history
    holds effect events, not decisions), so a past decision can be weighed only where the
    current state still shows its options. One decision qualifies: the end of the opponent's
    last main phase, during the searcher's turn from turn 2 and in the opponent's End Phase. The
    opponent's active Resources are then those it kept open (they become active only in its own
    Start Phase), and its hand holds the cards it kept, so it chose `END_MAIN` over playing each
    castable card of the candidate hand. `belief_last_n = N` caps the decisions weighed; this is
    the only one, so every `N >= 1` weighs it (default 1; `0` is refused). Weighing more
    decisions needs the agent to observe the opponent's decisions (interface request 4).
  - *The model.* P6-F's calibrated policy was dropped, so the model is the reference playout
    policy's MAIN scores (`_main_option`, `ai/policy.py:164-206`) evaluated per card from its
    printed values (a Unit `4 + 0.5 cost + 0.2 (AP + HP)`, `-8` with a full Battle Area; a Pilot
    `4.5 + 0.2 (AP + HP)` when an unpaired Unit can take it; a 【Main】 Command
    `2.5 + 0.3 cost`; a Base 4 over a missing or EX Base, else -1; `END_MAIN` 0), a card counting
    as castable when the opponent's level and open Resources cover its printed level and cost.
    The likelihood is the softmax probability of `END_MAIN` at the search's `prior_temperature`
    (1.5; the rollout temperature 0.3/0.5 would make every castable hand all but impossible).
- **Evidence:**
  - `p6c_w07_open_resources::w07_belief_worlds_hold_back_from_the_inferred_trick` (T-107): the
    searcher's Gundam can attack; the opponent passed its main phase with two of four Resources
    open and two hidden cards, a ST01-008 Blocker on board; its 26 unseen cards are 4 GD01-120
    (【Action】 AP+3 to a friendly Blocker, which then destroys Gundam in the block), 18 castable
    GD01-060 and 4 Lv.5 GD03-016. Attacks of 16 seeds at the reference `standard` budget:
    reference **16**; `belief_worlds` 8: 16, 16: 12, 32: 8, 64: **4** (holds back in 12). Pass.
  - `mech::p6c::tests::w07_belief_worlds_infer_the_trick_from_the_open_resources`: of the first
    400 worlds of that position, the trick is in the opponent's hand in 121 reference worlds
    (the pools' 0.29), 178 (M = 4), 224 (M = 8) and 243 (M = 16); a castable GD01-060 in 368,
    281, 201, 154. Pass.
  - `mech::p6c::tests::the_end_of_the_main_phase_weighs_castable_hands_down`: a castable hand
    weighs < 0.05, a hand of a 【Action】-only trick and an uncastable card > 0.99; no open
    Resource gives no evidence. Pass.
  - `mech::p6c::tests::belief_worlds_are_candidates_of_the_world_seed` (every world is one of its
    M candidates, candidate 0 the reference's, repeatable), `resampling_draws_by_cumulative_weight`
    (resampling and offsets), `settings_that_do_nothing_are_refused`. Pass.
  - Honesty and permutation invariance (W-07's unit tests): `every_flag_runs_on_the_vector_decisions_and_reads_the_information_set_only`
    with `belief_worlds=4` and `=8` (and the three-flag bundle): identical results and state from
    other true states of the same information set (a changed hidden card and permuted hidden
    cards); evidence applies at 9 of 32 vector decisions. Pass.
- **Ablation:** individual rows `belief_worlds=4` (+0.015) and `=8` (0.000), `=16` +0.020 [-0.053, +0.092];
  high-budget rows `belief_worlds=4` (+0.035 [-0.040, +0.109], dev) and `=16` +0.020 [-0.053, +0.092] (dev; below the +0.03 confirm threshold);
  confirm row `belief_worlds=4` at 2x the shipped `standard` budget: 790/1600 = 0.494 [0.469, 0.518], paired -0.013 [-0.044, +0.019], discordant 80/90 (confirm
  30300001, run once).

## W-08 — Determinization pads or trims pools

- **Status:** **open (blocked)**: `exact_pools` is not provided and stays refused by `validate`
  (`mech::p6c::tests::settings_that_do_nothing_are_refused`).
- **Why:** an exact world assigns each owner's hidden identities, by conservation, to that
  owner's hidden cards wherever they lie (owner and controller apart). `determinize` assigns per
  zone side and pads or trims with `_fit`, and it is the only public way to change a card's
  identity: `GameState`'s cards are private to gcg-engine, `procedures` writes only the rested
  flag, and the canonical round trip (`canonical_state` → `from_canonical`) would need
  `serde_json` in gcg-search's dependencies and the side table of runtime continuous effects
  that only the engine's `testkit` builds. Rejection over `determinize` seeds cannot help: in a
  mismatched state every world it builds pads or trims. Both routes need files outside P6-C
  (interface request 1).
- **What the lead's change unlocks:** with `observe::assign_hidden` the mechanism is a pool
  computation and a shuffle in `mech/p6c/` (every hidden instance of owner o, by uid order, gets
  a shuffled copy of o's decklist minus o's visible cards; a count mismatch is an
  `EngineError`), and the reference's pad/trim counter belongs inside `_fit` (gcg-engine).
- **How often it matters:** a one-off probe (a temporary unit test, not committed) counted the
  `(observer, owner)` pools whose size differs from the hidden slots, which is exactly when
  `_fit` pads or trims, over the 882 states of the D2 search vectors and the tier-1 `search`,
  `worlds`, `fuzz` and `baselines` snapshots: 0 of 3,528. On those states the exact world equals
  the reference's, so `exact_pools` is expected to have no game-level effect; it is a
  correctness guard for the states that move cards between players' decks.

## W-134 — Both players know both exact decklists

- **Status:** **open (blocked)**: `opponent_list = archetype` is not provided and stays refused;
  `gcg-eval benchmark --opponent-list archetype` fails with the refusal
  (`crates/gcg-eval/tests/p6m_cli.rs::w134_opponent_list_reaches_the_search_flag`, unchanged).
- **Why:** the archetype mode must draw the opponent's unseen cards from a declared prior of
  candidate lists, never from the true list. `determinize` draws from the decklists in the
  state's `GameConst`, which the search cannot replace, and `burst_density` and the evaluation
  read them too; resampling `determinize` worlds keeps the true list's support, so replacing the
  true list by another list consistent with the prior would change the worlds (the entry's own
  test). It needs interface request 1 (identities) and a state with other decklists.
- **Plan once unblocked:** the prior is a weighted list of candidate decklists (a built-in prior
  of the packaged deck lists, or a file named by a `gcg-eval` option, A30), reweighted by the
  multivariate hypergeometric likelihood of the opponent's visible cards; world k draws a list
  from the posterior with `derive_seed(W[k], "archetype")`, patches it to contain the visible
  cards, and assigns the hidden identities from it; the leaf's Burst density is recomputed from
  the drawn list. A one-list prior equal to the true list then reproduces `known` exactly.

## W-139 — Opponent nodes are tuned against the searcher's true private cards

- **Status:** resolved in part: `opponent_view_share` is implemented and measured null at both
  budgets (not recommended). It applies at root decisions none of whose options names a card
  hidden from the opponent (block decisions, attacks, activations, end of phase); elsewhere
  every world is the usual one (counted: the opponent-view test held at 11 of 32 vector
  decisions). The entry's root-decision bar (reference keeps the trick in at most 8 of 16,
  strengthened in at least 12) is **not met**: reference 0/16, share 0.25 0/16, share 0.5 0/16.
- **Resolution as implemented:** `opponent_view_share = σ` (`mech/p6c/view.rs`, `p6c::world`,
  `p6c::after_select`, `p6c::backup`). World k is an opponent-view world when
  `floor((k + 1) σ) > floor(k σ)` and `k >= 1` (with σ = 1/4 worlds 3, 7, 11, ...; world 0,
  which sets the root priors, never is): after the usual world, every card hidden from the
  opponent (the searcher's hand, deck, Shields, resource deck and the opponent's own deck and
  Shields) is redrawn by `determinize(world, opponent, derive_seed(W[k], "opponent-view"))`.
  The root selection gives the iteration's world `k = visits - base` (base: the root child's
  visits when the tree started, for `subtree_reuse`). An iteration in an opponent-view world
  trains the opponent's nodes only: opponent children gain the value in their pooled
  statistics and in their opponent-view slice; the root child gains its visit (it indexes the
  worlds) at its current mean, deeper searcher children are left unchanged. An opponent node
  selects by its opponent-view slice once that has 8 visits, in every iteration, so the
  searcher's usual lines face an opponent that cannot see its hand; with `opponent_buckets` the
  opponent-view slice takes precedence.
- **Why only those decisions:** root moves name cards by uid, and a redrawn world gives the
  searcher's hidden hand cards other identities, so a root move that plays a hand card would
  play another card. Keeping the named cards while redrawing the rest needs the engine's
  identity API (request 1), or a core hook that redraws after the root move is applied
  (request 6); either extends the mechanism to every decision.
- **Evidence:**
  - `mech::p6c::tests::w139_opponent_view_worlds_hide_the_held_trick` (T-139): the held-trick
    position; of 400 worlds, the 300 usual ones equal the reference's and all hold the trick,
    the 100 opponent-view ones hold it in 2 (the opponent's pools: 1 trick among 46 cards), with
    the opponent's information set equal to the usual world's and the searcher's board
    unchanged. Pass.
  - `mech::p6c::tests::an_opponent_view_iteration_trains_only_the_opponent` (the backup: root
    child one visit at its mean, the opponent child the value, the deeper searcher child
    unchanged, the slices) and `opponent_view_worlds_need_root_options_the_opponent_can_identify`
    (the root test; the redraw keeps the opponent's information set and the searcher's board and
    changes its hand in more than 16 of 32 seeds), `view::tests::view_worlds_follow_the_share`. Pass.
  - `p6c_w139_held_trick::w139_held_trick_decisions_are_recorded`: keeping the Resource open,
    16 seeds: reference 0, share 0.25 0, share 0.5 0. The search's evaluation values the
    activation (1 damage) above keeping the trick with or without the opponent model: the leaf
    weighs the trick card in hand (`hand_commands` 0.54) heavily against a saved Unit: in a
    variant with a token-deploying Base (ST01-015), the traced line in which the trick saves the
    5/4 Unit and destroys the attacker evaluates 0.255, the line that deploys the token and
    trades the Units 0.319.
    Pass (as a record).

## Interface change requests

1. **gcg-engine `observe` (owner: lead / engine).** `pub fn assign_hidden(s: &mut GameState,
   observer: u8, ids: &[(Uid, DefId)]) -> Result<(), EngineError>`: sets the identity of each
   listed instance, which must lie in a hidden zone and be unknown to `observer` (else
   `EngineError::Value`), rewrites the history to the new identities (`own_history`) and
   touches the state; plus `pub fn with_decklists(st: &GameState, main: [Box<[DefId]>; 2],
   resource: [Box<[DefId]>; 2]) -> GameState` (the testkit's `with_decklists` made public).
   Additive, unused by the reference. Unlocks `exact_pools` (W-08), `opponent_list = archetype`
   (W-134) and opponent-view worlds at every decision (W-139).
2. **gcg-engine `_fit` (W-08 diagnostic).** The reference's pad/trim counter belongs inside
   `observe::fit`.
3. **`contracts.rs` `MechanismEffort` (lead).** Counters for P6-C's diagnostics, which the search
   keeps per decision in `p6c::Search` (`reweighted`, `view_worlds`): worlds whose belief draw
   differs from the reference's and opponent-view worlds built.
4. **The `Agent` trait (lead).** Weighing more than one of the opponent's public decisions
   (W-07's N) needs the agent to see the states of the opponent's decisions between its own; an
   `observe(&mut self, st: &GameState, action: Action)` hook the runner calls after every public
   action would give the agent a public-history record to replay.
5. **gcg-eval (P6-I/lead).** W-05's census of the remaining observation-key splits.
6. **`mcts.rs` (P6-D/lead), alternative to 1 for W-139.** A `Mechanisms` hook after the root move
   is applied to the iteration's world, with `&mut GameState`, so an opponent-view iteration can
   redraw the searcher's hidden cards once the played card is public.

## Recommendation for the shipped presets

**Enable no P6-C flag in `standard` or `fast`.** Every P6-C mechanism measured null at game
level: `opponent_buckets` +0.015 (`m = 4`, reference budget) and -0.020 (2x shipped `standard`);
`belief_worlds` +0.015 / 0.000 (M = 4 / 8, reference budget), +0.035 at 2x on dev seeds that
did not hold on 1,600 confirm games (-0.013 [-0.044, +0.019]), and `=16` +0.020 at both budgets;
`opponent_view_share` 0.000 at both budgets; the three together -0.035 at the reference budget.
None reaches a lower Newcombe bound above 0, so none moves the E1 target (pooled lower bound
0.55 against reference `strong`). The mechanisms do what the entries ask at the node and world
level (the unit tests above), but the decisions they change are worth little in the reference
evaluation, which is what the search maximises: the W-05 and W-139 positions show the leaf
evaluation preferring the root move the reference already picks even under the corrected
opponent model. The lead's integrated-bundle leave-one-out rows (M15) are the place to recheck
them once P6-A/P6-B/P6-G change the leaf; `belief_worlds` is the one with a positive dev
signal at the higher budget and a working scenario, so it is the first to retry there
(its cost, counted as one auxiliary apply per candidate scored: +7% applies per game at
M = 4 and +10% at M = 8 against the control row).

## Gate

Run in the worktree on the code of `fa6e381` (the later commit changes only this file, the
evidence files and one module doc comment of `mech/mod.rs`, after which `cargo fmt --all
--check` and the workspace `cargo clippy` were rerun clean):

| Command | Result |
| --- | --- |
| `cargo fmt --all --check` | clean |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | clean |
| `cargo test --workspace --release` | 128 test suites, 1,152 tests passed, 0 failed (the null-configuration tests `null_config`, `null_config_traces` and every P6-C unit and scenario test among them) |
| `cargo test --release -p gcg-search --features slow` | 19 test suites, 229 tests passed, 0 failed (`null_config_search_set` included) |
| `target/release/gcg-diff corpus --tier 1 --require-all` | 3,804/3,804 traces passed (17,601 decisions, 2,166 snapshots, 1,396 agent decisions) |
| `scripts/compare_python_vs_rust.sh equivalence --matches 2 --seeds 1 --formats bo1 --pairs red-green-zeon:blue-white-federation --workers 1 --decision-logs off` | 3/3 identical (fast, standard, strong) |
