# P6-D weakness status: tree and budget mechanisms

Package P6-D (docs/port/PLAN.md §12.2, §12.3 S1, S4 parts, S7, S8, S12) for W-04, W-06, W-11,
W-18, W-138 and W-140. Written by the P6-D implementer on 2026-11-02 in worktree branch
`worktree-wf_b2d4a964-3bd-3`, from `8ae4767`. WEAKNESSES.md itself is not edited here (the lead
records the status lines from this file, §12.10).

Code commits: `6e2afd0` (the mechanisms), `d97d3e2` (order-free transposition keys, tests),
`2503285` (`subtree_reuse`), `31140b8` (docs), `b128d08` (the carry's counted apply),
`efac69e` (a transposed walk never re-enters the root), `95cc313` (a `gcg-eval` test pin,
outside the owned files); `88e1a37` and the commit of this file add the evidence. Which code
each measurement ran is given with its row.

## What P6-D owns

- The core loop and the `Mechanisms` composition: `crates/gcg-search/src/mcts.rs`,
  `crates/gcg-search/src/tree.rs`, `crates/gcg-search/src/agent.rs` (the `ReusingAgent` of
  `subtree_reuse`), the P6-D lines of `crates/gcg-search/src/mech/mod.rs` (`mod p6d`, the
  `world` field of `NodeCx`, the `priors`/`backup` dispatch, `available`).
- `crates/gcg-search/src/mech/p6d/`: `ensemble.rs` (S1), `backup.rs` (W-06), `filters.rs`,
  `transpose.rs`, `reuse.rs` (W-11), `macros.rs` (W-138), `priors.rs` (W-140).
- Tests: `crates/gcg-search/tests/scenarios/p6d_*.rs` (one target, `p6d_scenarios`, declared in
  `crates/gcg-search/Cargo.toml`), plus unit tests in each `mech/p6d/*.rs`.
- Evidence: `docs/port/eval/P6-D/` (row, curve, regret and fidelity reports with every game, the
  seed ledgers of these runs, and the weakened decks of W-18).

The null configuration is unchanged: every new path is gated by its flag, and the reference
order of RNG draws, argmaxes and sums is kept on the null path (evidence under "Gate").

## Measurement protocol (all rows)

`gcg-eval ablation individual --mechanism <flags> --reference standard --games 400 --seed
24600001 --role dev --ledger docs/port/eval/P6-D/seed-ledger.json --workers 10 --out
docs/port/eval/P6-D/individual-<flags>.json`: the challenger is `search:null-standard` (the
strengthened search at the reference `standard` budget, every mechanism off) plus the
mechanism, the opponent `ref:standard`; 400 BO1 games in mirrored blocks of four, stratified
round-robin over the 15 tuning pairs of `docs/port/eval/panel.json`, forced play/draw
(`--choose-first seat0`, A24). Every row uses the same master seed, so the rows share their
deals. The paired difference is the W-42 statistic of `gcg-eval` (per deal: the challenger won
the position with seat 0 minus the opponent won it from the same seat and cards; Newcombe 95%),
the discordant counts are McNemar's `b/c`. Iterations and applies are per game, challenger /
opponent.

| Row | Wins/N | Wilson 95% | Paired difference [Newcombe 95%] | Discordant b/c | Iterations per game | Applies per game | Seed (role), games, code | Evidence |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| control: null configuration (`engine_error_leaf=false`) | 201/400 = 0.502 | [0.454, 0.551] | +0.005 [-0.059, +0.069] | 22/21 | 10060.8 / 9965.2 | 123042.3 / 121444.3 | 24600001 (dev), 400, `2503285` | `docs/port/eval/P6-D/individual-engine_error_leaf-false.json` |
| `root_trees=1` | 201/400 = 0.502 | [0.454, 0.551] | +0.005 [-0.063, +0.073] | 25/24 | 9800.2 / 9965.3 | 118321.3 / 120362.2 | 24600001 (dev), 400, `2503285` | `docs/port/eval/P6-D/individual-root_trees-1.json` |
| `root_trees=4` | 190/400 = 0.475 | [0.426, 0.524] | -0.050 [-0.117, +0.017] | 19/29 | 10268.6 / 9888.8 | 123025.3 / 117290.6 | 24600001 (dev), 400, `6e2afd0` | `docs/port/eval/P6-D/individual-root_trees-4.json` |
| `root_trees=8` | 178/400 = 0.445 | [0.397, 0.494] | -0.110 [-0.189, -0.029] | 24/46 | 10358.9 / 9813.6 | 123188.3 / 114461.3 | 24600001 (dev), 400, `2503285` | `docs/port/eval/P6-D/individual-root_trees-8.json` |
| `visited_set_backup=true` | 199/400 = 0.497 | [0.449, 0.546] | -0.005 [-0.072, +0.062] | 23/24 | 10006.6 / 10000.0 | 122330.8 / 121798.0 | 24600001 (dev), 400, `6e2afd0` | `docs/port/eval/P6-D/individual-visited_set_backup-true.json` |
| `in_tree_filters=true` | 197/400 = 0.492 | [0.444, 0.541] | -0.015 [-0.082, +0.052] | 22/25 | 10045.4 / 9973.7 | 122363.3 / 120815.7 | 24600001 (dev), 400, `6e2afd0` | `docs/port/eval/P6-D/individual-in_tree_filters-true.json` |
| `transpositions=true` | 204/400 = 0.510 | [0.461, 0.559] | +0.020 [-0.051, +0.091] | 29/25 | 9747.5 / 9751.2 | 117909.1 / 118798.4 | 24600001 (dev), 400, `d97d3e2` | `docs/port/eval/P6-D/individual-transpositions-true.json` |
| `transpositions=true`, confirm | 795/1600 = 0.497 | [0.472, 0.521] | -0.006 [-0.042, +0.029] | 104/109 | 9798.1 / 9763.8 | 117496.8 / 116833.6 | 30460002 (confirm), 1600, `b128d08`* | `docs/port/eval/P6-D/confirm-transpositions-true.json` |
| `subtree_reuse=true` | 201/400 = 0.502 | [0.454, 0.551] | +0.005 [-0.068, +0.078] | 29/28 | 9291.8 / 9830.5 | 112167.9 / 118419.9 | 24600001 (dev), 400, `2503285` | `docs/port/eval/P6-D/individual-subtree_reuse-true.json` |
| `macro_multi_pick=true` | 202/400 = 0.505 | [0.456, 0.554] | +0.010 [-0.054, +0.074] | 23/21 | 10039.9 / 9976.0 | 124246.4 / 121542.7 | 24600001 (dev), 400, `d97d3e2` | `docs/port/eval/P6-D/individual-macro_multi_pick-true.json` |
| `prior_worlds=4` | 192/400 = 0.480 | [0.431, 0.529] | -0.040 [-0.101, +0.021] | 16/24 | 9947.6 / 9859.0 | 119954.8 / 118215.2 | 24600001 (dev), 400, `2503285` | `docs/port/eval/P6-D/individual-prior_worlds-4.json` |
| `transpositions`, `subtree_reuse`, `macro_multi_pick` together | 205/400 = 0.512 | [0.464, 0.561] | +0.025 [-0.050, +0.099] | 32/27 | 9036.4 / 9698.3 | 110063.2 / 117060.4 | 24600001 (dev), 400, `efac69e` | `docs/port/eval/P6-D/individual-transpositions-true-subtree_reuse-true-macro_multi_pick-true.json` |
| all seven P6-D flags (`root_trees=4`) | 186/400 = 0.465 | [0.417, 0.514] | -0.070 [-0.141, +0.003] | 21/35 | 9965.8 / 9810.5 | 121576.6 / 116573.6 | 24600001 (dev), 400, `2503285` | `docs/port/eval/P6-D/individual-root_trees-4-visited_set_backup-true-in_tree_filters-true-transpositions-true-subtree_reuse-true-macro_multi_pick-true-prior_worlds-4.json` |

"Code" is the commit whose code the binary was built from (* the binary was relinked from the
working tree between `2503285` and `b128d08` while this run started; both are identical for
this flag, as for the null-configuration curve and the regret runs). The measured code path of each row
is the same from `6e2afd0` to `efac69e` except: transposition keys became order-free in
`d97d3e2` (every row with `transpositions` ran at or after it); `b128d08` only adds one counted
auxiliary apply per search that keeps trees (the `subtree_reuse` row ran before it, so its
applies per game omit about one apply per search); `efac69e` stops a transposed walk at the
root, which changes only `transpositions` together with `subtree_reuse`. The all-seven row ran
before `efac69e`; a run of the three-together combination crashed at `2503285` with an
out-of-range world index after 160 games (a transposed walk re-entered the reused root), and the
three-together row reran the same deals at `efac69e` and completed. The all-seven row is
therefore indicative only; the lead's bundle rows supersede it.

Reading: the control (the null configuration against the reference, with different agent seeds)
is the noise floor, +0.005. At equal iterations no P6-D mechanism is distinguishable from it
except `root_trees=8`, which is significantly negative (-0.110 [-0.189, -0.029]); `root_trees=4`
(-0.050) and `prior_worlds=4` (-0.040) have negative point estimates. `transpositions` (+0.020
on dev seeds, the most promising row) did not hold on 1,600 confirm-role games (-0.006 [-0.042,
+0.029]). Measured non-negative (point estimate at least 0): `transpositions` (dev row),
`subtree_reuse`, `macro_multi_pick`, `root_trees=1`, and those three mechanisms together
(+0.025 [-0.050, +0.099], with 7% fewer iterations per game because reused visits settle
decisions earlier). Null within noise with a slightly negative point estimate:
`visited_set_backup` (-0.005), `in_tree_filters` (-0.015). Kept under the rule "non-negative
measured effect" (default-on is the lead's decision): `subtree_reuse` (+0.005) and
`macro_multi_pick` (+0.010). `transpositions` is null: its decisive row (1,600 confirm games)
is -0.006, so it enters the bundle only if the lead's leave-one-out row of the three-together
combination (+0.025) keeps it. Not kept: `visited_set_backup` (null, -0.005; a correctness fix
the lead may still adopt on the W-06 evidence), `in_tree_filters` (-0.015), `prior_worlds`
(-0.040) and `root_trees` (negative at equal iterations, significantly at K = 8, and the
budget curves show it does not move the plateau). Every mechanism stays in the code behind its
flag, off by default.

## W-04 — Budget plateau

- **Status:** resolved (mechanism implemented and measured; the measured effect at equal
  iterations is null to negative, so `root_trees` is not recommended for the presets).
- **Resolution as implemented:** `root_trees{K}` (S1). `crates/gcg-search/src/mcts.rs`
  (`run_trees`, `TreeState`, `draw_seeds`, `trees_close_call`) and
  `crates/gcg-search/src/mech/p6d/ensemble.rs`. The root moves and the decision's budget are
  computed once; one draw of the per-decision generator is the decision seed; tree `t` is
  seeded `derive_seed(decision_seed, "tree", t)` and draws its own world and rollout seeds in
  the reference order (disjoint streams); each tree runs `ceil(B/K)` iterations (cap
  `ceil(cap/K)`) with the reference's per-tree early stop. Each phase is one
  `cx.par.fork_join` over the trees (every job builds the composed mechanisms from the shared
  configuration and owns its state clone, so results never depend on the worker); root
  statistics merge in tree order (visits summed, totals `py_sum`), the move is the first max of
  `(visits, mean)`, and the target-decision close call uses the concatenated per-tree paired
  differences. With `root_trees` the per-iteration drill-down tracer records nothing (the trees
  run as jobs).
- **Evidence:**
  - `p6d_scenarios::p6d_determinism::root_trees_give_the_same_decision_for_any_worker_count`:
    `root_trees=4`, `=8` and the full P6-D bundle on 8 vector decisions, each as tasks of
    pools of 1, 4 and 10 workers and inline: action, every alternative (value bits), stats,
    mechanism counters and generator state identical. Pass.
  - `p6d_determinism::root_trees_split_the_budget_and_draw_one_seed`: K = 1, 2, 4, 8 on 12
    decisions: root moves, budget and cap equal the null configuration's; iterations at most
    `K*ceil(cap/K)`; root visits sum to the iterations; at least K worlds; the generator state
    after the search is the same for every K (one draw after the root moves). Pass.
  - `p6d_determinism::the_bundle_repeats_in_another_process`: the bundle's results digest in a
    child process equals the parent's. Pass.
  - `mech::p6d::ensemble::tests` (4 tests: ceiling split, distinct tree seeds, tree-order
    merge, per-tree pairing of the close call). Pass.
- **Ablation:** rows `root_trees=1` (the ensemble machinery with one tree: a reseeded
  reference), `root_trees=4`, `root_trees=8` in the table above, at equal total iterations.
  Budget curves (W-04: "does root_trees move the budget plateau?"): `gcg-eval budget-curve <challenger> ref:standard --scales 0.5,1,2,4 --games 400
  --seed 24600002 --role dev --ledger docs/port/eval/P6-D/seed-ledger-curves.json --workers
  10` (the budget fields `iterations_per_option`, `min_iterations`, `max_iterations` scaled;
  the opponent fixed at the reference `standard` budget; both curves on the same deals).
  - `search:null-standard` (one tree; code `b128d08`),
    `docs/port/eval/P6-D/budget-curve-null-standard.json`:
    - 0.5x: 201/400 = 0.502 [0.454, 0.551], paired +0.005 [-0.070, +0.080], discordant 30/29, iterations per game 5205.3 / 9926.3, applies per game 63441.2 / 121059.4
    - 1.0x: 201/400 = 0.502 [0.454, 0.551], paired +0.005 [-0.057, +0.067], discordant 21/20, iterations per game 10000.9 / 9884.8, applies per game 121670.7 / 120633.1
    - 2.0x: 204/400 = 0.510 [0.461, 0.559], paired +0.020 [-0.050, +0.090], discordant 28/24, iterations per game 18808.3 / 9970.5, applies per game 230439.1 / 121898.7
    - 4.0x: 218/400 = 0.545 [0.496, 0.593], paired +0.090 [+0.018, +0.161], discordant 37/19, iterations per game 35401.2 / 9861.3, applies per game 425687.4 / 120811.2
  - `search:null-standard:root_trees=4` (code `d97d3e2`),
    `docs/port/eval/P6-D/budget-curve-null-standard-root_trees-4.json`:
    - 0.5x: 174/400 = 0.435 [0.387, 0.484], paired -0.130 [-0.201, -0.057], discordant 16/42, iterations per game 5178.0 / 9923.4, applies per game 61977.3 / 115222.4
    - 1.0x: 178/400 = 0.445 [0.397, 0.494], paired -0.110 [-0.183, -0.035], discordant 19/41, iterations per game 10472.6 / 10102.1, applies per game 128145.3 / 120165.6
    - 2.0x: 200/400 = 0.500 [0.451, 0.549], paired +0.000 [-0.066, +0.066], discordant 23/23, iterations per game 20737.9 / 9928.1, applies per game 252392.5 / 120258.6
    - 4.0x: 204/400 = 0.510 [0.461, 0.559], paired +0.020 [-0.040, +0.080], discordant 21/17, iterations per game 39333.7 / 9845.5, applies per game 480226.4 / 119914.9

Answer: the single tree is flat from 0.5x to 2x (0.502, 0.502, 0.510) and gains only at 4x
  (0.545, paired +0.090 [+0.018, +0.161]), the model-limited plateau W-04 describes. Four root
  trees lie 0.035 to 0.067 below the single tree at every scale (0.435, 0.445, 0.500, 0.510) and
  reach the reference only at 2x: splitting the iterations into shallower trees costs more than
  the disjoint worlds gain, so `root_trees` does not move the plateau; it shifts the curve down.
- **Not done here:** the acceptance run of the verification ("strengthened `standard` beats
  reference `strong` with Wilson lower bound above 0.5 on a held-out seed run once") is the final
  evaluation E1 of the integrated bundle (lead, §12.6, M15).

## W-06 — Observation keys count one child twice

- **Status:** resolved.
- **Resolution as implemented:** `visited_set_backup`. `crates/gcg-search/src/mech/p6d/backup.rs`
  (each child of the path gains one visit and one value per iteration, in first-visit order),
  dispatched by `Composed::backup`; and in `crates/gcg-search/src/mcts.rs` (`Search::select`,
  `counted`) the `avail` count of a child is raised at most once per iteration as well, so the
  UCB availability count stays per iteration. The reference keeps the double count.
- **Evidence:**
  - `mech::p6d::backup::tests::a_child_reached_twice_gains_one_visit_per_iteration`: the
    scripted line (root move, then the opponent's pass child twice: battle action-step pass and
    end-phase action-step pass of one node): the shared child gains 1 visit per iteration with
    the visited set and 2 in the reference (pinned), for 1-3 iterations; totals exact. Pass.
  - `backup::tests::a_path_without_repeats_backs_up_as_the_reference`: bit-identical to the
    reference on a path without repeats. Pass.
  - `backup::tests::a_repeated_opponent_pass_is_backed_up_once_with_the_visited_set`: in the
    searches of all 144 D2 vector decisions (`fast` = reference `standard` budget), the
    reference repeated a child 37 times, every one an opponent's ACTION_STEP pass, each backed
    up twice (pinned); with the visited set every repeated child (55) gained one visit. Pass.
- **Ablation:** row `visited_set_backup=true` in the table above.

## W-11 — No in-tree pruning, transpositions or tree reuse

- **Status:** resolved (three mechanisms implemented and measured).
- **Resolution as implemented:**
  - `in_tree_filters`: `crates/gcg-search/src/mech/p6d/filters.rs`, applied in
    `Search::node_moves` (`mcts.rs`) at every tree node: `prune_dominated` without the trial
    worlds (free 【Burst】 accepted, MAIN attacks that cannot matter dropped) and
    `merge_equivalent`; `without_effect` stays root-only (too costly). No engine applies.
  - `transpositions`: `crates/gcg-search/src/mech/p6d/transpose.rs` and `Tree::transposed`;
    the walk's node at every decision is the node of a SHA-256 `stable_key` of the searcher's
    observation of the world state (turn, phase, step, priority; public cards in uid order with
    identity, rest, damage and pair; hidden zones as size plus the multiset of identities the
    searcher knows; battles; lasting and delayed effects; counts of once-per-turn records,
    frames and pending triggers; the pending decision's header, and its options as a set only
    when the searcher decides). A walk that would re-enter a node of the same iteration stops
    there (cycle guard). Nodes are per tree and per decision.
  - `subtree_reuse`: `crates/gcg-search/src/mech/p6d/reuse.rs`, `ReusingAgent` and
    `make_reusing_agent` in `crates/gcg-search/src/agent.rs`, and the carry in
    `mcts.rs` (`search_with_carry`, `reused`). With the flag, `make_agent` returns a
    `ReusingAgent` that keeps its trees after each search. They are reused at its next decision
    only when that decision is the one the move predicted: same turn, the game's action
    counter one higher (no other decision in between), and the observation key of the new
    information-set state equal to the key of the old one after the move (so a draw, reveal or
    random result breaks the reuse). The new root is the played child's node (or the
    transposed node); its children keep visits, totals, availability and priors, the decision
    runs its own budget of new iterations, and the k-th new visit plays in world k. The carry's
    predicted-transition apply is counted as one auxiliary apply. `SearchAgent` (the
    contract type, `make_search_agent`) refuses the flag rather than ignoring it, because it
    has no field for the carry (see interface notes).
- **Evidence:**
  - `mech::p6d::transpose::tests::keys_ignore_identities_hidden_from_the_searcher`: two worlds
    of one information set have the same key for the searcher and different keys for the
    opponent (who sees its own hand). Pass.
  - `transpose::tests::two_orders_of_the_same_deployments_share_a_key`: two orders of the same
    two Unit deployments from the vector decisions reach one key; one deployment alone does
    not. Pass.
  - `p6d_determinism::every_mechanism_repeats_across_runs_and_thread_counts` (each of the six
    flags and the bundle, two runs and 1 vs 4 workers) and
    `p6d_determinism::the_bundle_repeats_in_another_process`. Pass.
  - `p6d_scenarios::p6d_reuse::consecutive_own_decisions_start_from_the_kept_subtree`: in 4
    fuzz-deck games per configuration at a reduced budget, 90 of 191 searches reused a subtree
    (`subtree_reuse`), 97 of 201 (`+ transpositions`), 97 of 196 (`+ root_trees=2`); without
    the flag none; every game repeats action for action. Pass.
  - `p6d_reuse::the_concrete_search_agent_carries_its_trees`; `mech::p6d::reuse::tests::
    kept_trees_go_only_to_the_predicted_decision`. Pass.
- **Ablation:** rows `in_tree_filters=true`, `transpositions=true`, `subtree_reuse=true` in the
  table above. The `subtree_reuse` row ran at `2503285`, before its predicted-transition apply
  was counted, so its applies per game omit one apply per search that kept trees (decisions
  unchanged). Regret per decision (W-43 tool): `gcg-eval regret <candidate>
  "ref:strong:max_iterations=2560,min_iterations=640,iterations_per_option=256" --recorder
  search:null-standard --games 60 --per-game 2 --seed 24600003 --role dev --ledger
  docs/port/eval/P6-D/seed-ledger-regret.json --workers 10` (code `b128d08`): the same 120
  positions (recorded by `search:null-standard` self-play over the 15 tuning pairs), judged
  against the reference `strong` search at twice its budget with 8 shared worlds. Mean regret
  per decision: null configuration 0.0021 (SE 0.0021), `in_tree_filters` 0.0042 (SE 0.0025), `transpositions` 0.0021 (SE 0.0026). Paired by
  position against the null configuration: `in_tree_filters` +0.0021 [-0.0008, +0.0050],
  `transpositions` +0.0000 [-0.0029, +0.0029] (2 of 120 positions differ in regret for each).
  Neither difference is resolved: 120 positions resolve about ±0.003 per decision, far from
  the ±0.0004 bar of W-04's root rules. Evidence: `docs/port/eval/P6-D/regret-null-standard.json`,
  `docs/port/eval/P6-D/regret-null-standard-in_tree_filters-true.json`,
  `docs/port/eval/P6-D/regret-null-standard-transpositions-true.json`. `subtree_reuse` has no
  regret row: the tool judges one decision of a fresh agent, which has no previous decision to
  reuse.

## W-18 — `fast` rates are not `standard` rates

- **Status:** resolved for P6-D's part (mapping, fidelity tool run, slow-suite port); the final
  `fast` budget is tuned by the lead (§12.8), after which the fidelity run is repeated.
- **Resolution as implemented:** the `fast` budget mapping is the brief's: strengthened `fast`
  starts from reference `standard` (`crates/gcg-search/src/presets.rs`, from M11), `standard`
  from reference `strong`. Fidelity check: `gcg-eval fidelity search:fast search:standard` on
  five variants against `blue-white-federation`: `red-green-zeon`, its weakened variant
  (`tests/data/decks/red-green-zeon-weakened.txt`), `purple-white-tekkadan`, and two new
  weakened variants across colours, `docs/port/eval/P6-D/decks/purple-white-tekkadan-weakened.txt`
  and `docs/port/eval/P6-D/decks/blue-white-federation-weakened.txt` (each swaps 9 cards for
  clearly weaker ones of its colours; both legal, `gcg-sim validate`).
- **Evidence:**
  - `p6d_scenarios::p6d_fast_preset` (the Rust port of `tests/ai/test_fast_preset.py` for the
    strengthened presets): `fast_searches_at_most_half_of_the_standard_budget_at_every_width`
    (n = 1..40), `fast_differs_from_standard_only_in_budget_and_noise` (every differing field,
    mechanisms and weights included, is a budget or noise field),
    `weakened_variants_used_for_fidelity_are_legal`. Pass.
  - Fidelity: `gcg-eval fidelity search:fast search:standard --variants
    red-green-zeon,tests/data/decks/red-green-zeon-weakened.txt,purple-white-tekkadan,docs/port/eval/P6-D/decks/purple-white-tekkadan-weakened.txt,docs/port/eval/P6-D/decks/blue-white-federation-weakened.txt
    --against blue-white-federation --games 200 --seed 30460001 --role confirm --ledger
    docs/port/eval/P6-D/seed-ledger-fidelity.json --workers 10` (code `2503285`; the shipped
    presets as they are now, both with only `engine_error_leaf` on), 200 games per variant and
    agent on shared deals, evidence `docs/port/eval/P6-D/fidelity-fast-standard.json`:
    - `red-green-zeon`: `fast` 112/200 = 0.560 [0.491, 0.627] (rank 1); `standard` 108/200 = 0.540 [0.471, 0.608] (rank 1); paired fast minus standard +0.020 [-0.051, +0.091]
    - `red-green-zeon-weakened`: `fast` 48/200 = 0.240 [0.186, 0.304] (rank 4.5); `standard` 45/200 = 0.225 [0.173, 0.288] (rank 5); paired fast minus standard +0.015 [-0.048, +0.078]
    - `purple-white-tekkadan`: `fast` 95/200 = 0.475 [0.407, 0.544] (rank 2); `standard` 92/200 = 0.460 [0.392, 0.529] (rank 2); paired fast minus standard +0.015 [-0.062, +0.092]
    - `purple-white-tekkadan-weakened`: `fast` 48/200 = 0.240 [0.186, 0.304] (rank 4.5); `standard` 48/200 = 0.240 [0.186, 0.304] (rank 4); paired fast minus standard +0.000 [-0.068, +0.068]
    - `blue-white-federation-weakened`: `fast` 73/200 = 0.365 [0.301, 0.434] (rank 3); `standard` 76/200 = 0.380 [0.316, 0.449] (rank 3); paired fast minus standard -0.015 [-0.084, +0.055]
    - Agreement: Spearman 0.9747, Kendall tau-b 0.9487, pairwise agreement 1.0 (ties excluded), same best variant: true.
    - Against the verification: wherever `standard` resolves a matchup (an interval excluding
      0.5: the three weakened variants), `fast` favours the same deck, and every paired
      difference interval includes 0. Met on these seeds. The run used confirm-role seeds
      (W-18 asks for held-out seeds; the eval and review roles are kept for the gates), and the
      lead repeats it on the tuned `fast` budget with the adopted bundle (§12.8).

## W-138 — Multi-card choices are sequential single picks

- **Status:** resolved.
- **Resolution as implemented:** `macro_multi_pick`. `crates/gcg-search/src/mech/p6d/macros.rs`,
  applied at the root (`prepare`) and at every tree node (`Search::node_moves`): a SELECT of
  cards with a maximum of 2 or 3 is expanded on clones into its complete subsets (at most 20),
  each one tree move whose key packs its picks (variant `i32::MIN + n`, never a single-pick
  key); applying it applies the engine's single picks (and DONE when the subset ends early).
  The agent plays the first pick of the best subset; the root budget is the single picks'.
  Alternatives sharing a first pick are reported as one. Trial picks are auxiliary applies,
  capped at 16 per iteration of the decision's cap; past it, or with more than 20 subsets, the
  choice keeps its single picks. Engine and replay encoding unchanged.
- **Evidence:**
  - `p6d_scenarios::p6d_macros::subset_moves_replay_as_single_picks`: 10 fuzz-deck games of
    two `macro_multi_pick` agents (reduced budget): every recorded action replays with
    `apply(check=true)` from the start to the same winner and state digest; 3 searches used
    subset root moves, 4 multi-card picks were played. Pass.
  - `mech::p6d::macros::tests::subset_keys_are_distinct_and_never_single_pick_keys`. Pass.
- **Ablation:** row `macro_multi_pick=true` in the table above (multi-card choices are rare in
  the panel's decks, as the test's counts show).

## W-140 — In-tree priors come from the first world

- **Status:** resolved.
- **Resolution as implemented:** `prior_worlds{K}`. `crates/gcg-search/src/mech/p6d/priors.rs`,
  dispatched by `Composed::priors`, with the iteration's world index in `NodeCx::world` and the
  refresh condition in `Search::select`: every in-tree node keeps each world's policy scores;
  the first K distinct worlds contribute every move's score, a later world only those of moves
  not seen before; a move's prior is the softmax (`prior_temperature`) of its mean score within
  its decider and decision kind, recomputed whenever a contributing world or a new move arrives;
  sums in world order and softmax in a fixed key order. The root prior is computed once.
- **Evidence:**
  - `mech::p6d::priors::tests::priors_do_not_depend_on_the_order_the_same_worlds_arrive_in`
    (24 vector decisions, 4 worlds in three orders: identical prior bits, summing to 1). Pass.
  - `p6d_scenarios::p6d_priors::root_priors_are_equal_across_eight_determinisations`: on 1,000
    positions (745 tier-1 corpus snapshots with at least two canonical moves, topped up with
    rollout-policy games on fuzz decks), the root priors in 8 determinisations of the decider's
    information set are bit-identical for every decision kind met: CHOOSE_FIRST 231, REDRAW 8,
    MAIN 608, BLOCK 15, ACTION_STEP 36, ORDER_TRIGGER 2, BURST 22, YES_NO 13, SELECT 58,
    EXCESS 3, DISCARD 4; no scorer reads hidden state at the root (the reference result,
    recorded). Pass.
- **Ablation:** row `prior_worlds=4` in the table above.

## Interface notes for the lead

- `subtree_reuse` needs per-agent state. `SearchAgent` (lead-owned `contracts.rs`) has no field
  for it, so the carry lives in a second agent type, `ReusingAgent` in `agent.rs` (lint §9.9
  item 5 still holds: only `agent.rs` implements `Agent`, and both `choose` bodies use the true
  state only in `begin`, `Observation::new` and `finish`). `make_agent` returns it when the flag
  is on; `make_search_agent` returns a `SearchAgent`, which refuses the flag. Proposed contract
  change: add `pub(crate) reuse: crate::mech::p6d::reuse::Carry` to `SearchAgent` and fold
  `ReusingAgent` into it.
- New public items: `gcg_search::{ReusingAgent, make_reusing_agent}`. The hook signatures of
  `Mechanisms` are unchanged; `NodeCx` gained the field `world` (the iteration's world index),
  set by the core.
- `crates/gcg-search/src/presets.rs`: `unavailable_mechanisms_are_refused` used `root_trees` as
  its unavailable example; it now checks that `root_trees` is accepted and uses
  `opponent_buckets` (P6-C) as the refused example.
- `crates/gcg-search/src/mech/tests.rs`: `searched_states`, `config`, `run_with` and `Outcome`
  are `pub(crate)` for the P6-D unit tests; `run_with` passes no carry.
- `crates/gcg-search/Cargo.toml`: the `[[test]] p6d_scenarios` target.
- The seed ledgers of these runs are `docs/port/eval/P6-D/seed-ledger.json`,
  `docs/port/eval/P6-D/seed-ledger-curves.json`, `docs/port/eval/P6-D/seed-ledger-regret.json`
  and `docs/port/eval/P6-D/seed-ledger-fidelity.json` (separate files so that parallel runs
  never raced on one ledger, and so that this worktree never wrote the shared ledger); merge
  them into the shared ledger of A14 (`seed-ledger.json` in the evaluation directory).
- `crates/gcg-eval/src/agent_spec.rs` (outside the owned files, needed for the gate):
  `search_overrides_reach_the_mechanism_flags` used `root_trees` as its unavailable
  mechanism; it now checks that `root_trees` parses and pins `opponent_buckets` (P6-C) as the
  refused one (commit `95cc313`).

## Gate

Run at `88e1a37` (the code of `efac69e` plus evidence and this file), after the last code edit:

| Command | Result |
| --- | --- |
| `cargo fmt --all --check` | pass |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | pass |
| `cargo test --workspace --release` | pass: 1,005 tests in 122 targets, 0 failed, 0 ignored (980 before P6-D) |
| `cargo test --release -p gcg-search --features slow --test null_config_search_set` | pass: the null configuration replays all 661 searched decisions of the tier-1 `search` set (110 s) |
| `target/release/gcg-diff corpus --tier 1 --require-all` | 3804/3804 traces, exit 0 |
| `COMPARE_OUT=<worktree>/build/compare-p6d-head scripts/compare_python_vs_rust.sh equivalence --matches 2 --seeds 1 --formats bo1 --pairs red-green-zeon:blue-white-federation --workers 1 --decision-logs off` | `equivalence: 3/3 identical (fast 1/1; standard 1/1; strong 1/1; decision-log off 3/3;)`, exit 0 |

The P6-D tests within that run: `p6d_scenarios` (12 tests: determinism ×4, fast-preset port ×3,
subset replay, root priors, reuse ×2, process digest child) and the `mech::p6d::*` unit tests
(13). The machine ran at load 100-500 during the measurements (other agents), which affects
wall time only; every run is deterministic for its seed.
