# AI players (Rust)

The Python reference AI, its tuning history and its measurements are documented in
[`docs/python/AI.md`](python/AI.md); this file describes the Rust players, the two `gcg-sim`
presets and how their strength is measured.

Every number below is copied from a committed evidence file, named next to it. Budgets are
node counts (MCTS iterations), never wall-clock time. Nothing in the players reads a clock or
the thread count.

## Players

| Player | Crate and file | Reachable through | What it is |
| --- | --- | --- | --- |
| `random` | `crates/gcg-ai/src/random.rs` | `gcg-eval` agent spec `random` | Exact port of the Python random player: a uniform option per decision (it draws even with one option). |
| `greedy` | `crates/gcg-ai/src/greedy.rs` | `gcg-eval` agent spec `greedy` | Exact port of the Python greedy baseline: one determinised world, every pruned move played to the next turn with the deterministic policy and scored by the evaluation. Frozen as a baseline. |
| Reference search | `crates/gcg-ai-reference/src/mcts.rs`, presets in `crates/gcg-ai-reference/src/contracts.rs` | `gcg-eval` only: `reference-benchmark --reference-preset fast\|standard\|strong`, agent specs `ref:<preset>` and `ref:baseline-<preset>` | The equivalence-verified port of the Python information-set MCTS (`src/gcg_sim/ai/mcts.py`) with the exact reference budgets and search parameters. It reproduces Python's decisions byte for byte ([`docs/port/EQUIVALENCE.md`](port/EQUIVALENCE.md)). It anchors the equal-work speed comparison and every strength claim below. |
| Strengthened search | `crates/gcg-search/` | `gcg-sim --ai-preset standard\|fast`; `gcg-eval` agent specs `search:<preset>`, `search:null-<preset>`, `search:FILE.json` | The reference loop with the Phase 6 extension points (`crates/gcg-search/src/mech/`), configured per preset. It plays from an information-set observation only (W-135). |

`gcg-sim` links neither the reference search nor `gcg-diff`:
`crates/gcg-cli/tests/no_reference.rs` and `scripts/verify_all_rust.sh` (`cargo tree -p
gcg-cli`) enforce it. Agent names are `search-standard` and `search-fast` for the presets and
`mcts-<preset>` for the reference, so no `gcg-sim` output can be mistaken for a reference run.

`gcg-eval` agent specs (`gcg-eval head-to-head --help`): `random`, `greedy`, `ref:<preset>`,
`ref:baseline-<preset>`, `search:<preset>`, `search:null-<preset>` (every mechanism off) or
`search:FILE.json`, each with optional `:field=value,...` overrides, for example
`search:standard:tactical_solver=false`.

## Presets

`gcg-sim --ai-preset` accepts exactly `standard` (the default) and `fast`; any other value,
`strong` included, is a usage error (exit 2). The wording of each preset is the brief's and is
the `--help` text:

- `standard`: uses the full search so that each player plays at the level of the strongest
  top-tier competitive player; for refining and polishing the final deck.
- `fast`: trades search depth for throughput; for rapidly screening deck variants,
  sub-variants and hypothetical builds.

Both run the strengthened search. A decision with `n` options starts from
`max(min_iterations, min(max_iterations, iterations_per_option * n))` iterations
(`StrengthenedConfig::budget`, `crates/gcg-search/src/presets.rs`; Python
`src/gcg_sim/ai/config.py:55-57`). Target choices that stay a close call may extend up to
`target_extension` times that budget (`target_extension_z` sets the closeness test), as in the
reference.

The table below is the one source of the preset budgets in the documentation. It equals the
constants of `crates/gcg-search/src/presets.rs`, and `crates/gcg-search/tests/ai_md_presets.rs`
fails when either changes without the other (W-129, T-130).

| Preset | Use | Starts from (mapping) | Budget scale | `iterations_per_option` | `min_iterations` | `max_iterations` |
| --- | --- | --- | ---: | ---: | ---: | ---: |
| `standard` | uses the full search so that each player plays at the level of the strongest top-tier competitive player; for refining and polishing the final deck | reference `strong` (128 / 320 / 1280) | 8 | 1024 | 2560 | 10240 |
| `fast` | trades search depth for throughput; for rapidly screening deck variants, sub-variants and hypothetical builds | reference `standard` (64 / 160 / 640) | 1.5 | 96 | 240 | 960 |

Every other search setting is the mapped reference preset's (`src/gcg_sim/ai/config.py:93-109`):

| Setting | `standard` (= reference `strong`) | `fast` (= reference `standard`) |
| --- | --- | --- |
| `exploration` | 0.15 | 0.3 |
| `rollout_temperature` | 0.3 | 0.5 |
| `first_player_games` | 48 | 24 |
| `mulligan_samples` | 192 | 96 |
| `prior_weight`, `prior_temperature` | 0.5, 1.5 | 0.5, 1.5 |
| `horizon_turns` | 2 | 2 |
| `value_scale` | 1.0 | 1.0 |
| `max_rollout_decisions` | 250 | 250 |
| `policy_burst_yes_score` | 4.0 | 4.0 |
| `noop_branch_choices`, `noop_resource_aware`, `strict_attack_prune`, `prune_noop_rest_targets`, `merge_equivalent_moves`, `observation_keys`, `policy_rule_polarity_table`, `policy_zero_ap_face` | true | true |
| `target_extension`, `target_extension_z` | 3.0, 2.0 | 3.0, 2.0 |
| weights | `ROLE_WEIGHTS` | `ROLE_WEIGHTS` |

Mechanisms (the same in both presets; every other `MechanismFlags` field is off):

| Mechanism | Item | What it does |
| --- | --- | --- |
| `adaptive_horizon` | W-01 | a rollout that stops at its horizon with an unsettled leaf plays one more turn boundary |
| `capped_rollout_continuation` | W-15 | a rollout that hits its 250-decision cap finishes its step with the deterministic policy |
| `tactical_solver` | W-03 | an exact bounded attack-step solver plays a proved lethal in rollouts (its applies are auxiliary applies, capped at 16 per iteration of the decision) |
| `exclude_capped_play_draw` | W-44 | capped or drawn play/draw simulations leave the paired estimate |
| `engine_error_leaf` | W-143 | a simulated line ending in an engine error scores as a loss instead of failing the real game |
| `kill_aware_targeting` | W-26 | damage targets in rollouts prefer a Unit the damage destroys over one that survives it |
| `tree_lethal_select` | W-25 | at an in-tree MAIN decision, the tactical solver's proved lethal is selected before the policy-ordered moves |

### Mapping to the reference presets

The reference presets keep Python's exact values (`crates/gcg-ai-reference/src/contracts.rs`,
Python `src/gcg_sim/ai/config.py:80-111`) and are reachable only through `gcg-eval`:

| Reference preset | Agent name | `iterations_per_option` | `min_iterations` | `max_iterations` | `exploration` | `rollout_temperature` | `first_player_games` | `mulligan_samples` | Shipped preset that starts from it |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| `fast` | `mcts-fast` | 7 | 16 | 64 | 0.15 | 0.3 | 8 | 96 | none |
| `standard` | `mcts-standard` | 64 | 160 | 640 | 0.3 | 0.5 | 24 | 96 | `fast` (1.5x the budget) |
| `strong` | `mcts-strong` | 128 | 320 | 1280 | 0.15 | 0.3 | 48 | 192 | `standard` (8x the budget) |

The name `fast` therefore means different budgets in the two binaries: Python's
`--ai-preset fast` is the reference `fast` row, `gcg-sim --ai-preset fast` starts from the
reference `standard` row (PLAN D12). `results.json` records the mapping in
`ai.search.mapping` (`"reference strong"` or `"reference standard"`) together with the budget,
the enabled mechanisms and the weights. The baseline presets `ref:baseline-standard` and
`ref:baseline-strong` (the AI as released before the October 2026 changes,
`src/gcg_sim/ai/baseline.py:18-73`) are also in `crates/gcg-ai-reference/src/contracts.rs`.

### How the budgets were chosen

PLAN §12.8, with the adopted bundle, on the 15 tuning pairs only, 400 games per point, forced
play/draw (`docs/port/eval/M15/README.md`, steps 3-5):

- `standard`: the largest measured scale whose full-search time stays under the 2x floor
  against Python `strong`, and the best point of its budget curve. Curve against `ref:strong`
  (tuning master 11500100): 1x 195/400 (0.487), 2x 205/400 (0.512), 4x 214/400 (0.535), 8x
  227/400 (0.568, Wilson 95% [0.518, 0.615]). 16x was not run: its projected time would not
  fit the floor.
- `fast`: the smallest scale with a pooled point estimate of at least 0.53 against
  `ref:standard` and no pair upper bound below 0.5. Curve (tuning master 11500200): 0.5x
  195/400 (0.487), 0.75x 207/400 (0.517), 1x 210/400 (0.525), 1.5x 228/400 (0.570, [0.521,
  0.618]), 2x 224/400 (0.560).

The 2x floor was then measured on an idle M2 Pro with `gcg-sim` itself
(`docs/port/BENCHMARKS.md`, Part 2): `standard` 133.7 s against Python `strong`'s 356.0 s for 20
BO3 matches at the best worker count (2.66x), `fast` 76.9 s against Python `standard`'s 972.0 s
for 100 matches (12.6x).

Per-game cost of the shipped presets on the Part 2 workload (`docs/port/eval/M15/README.md`,
"Cost of each scale"; instructions retired, which load does not change): `standard` 128.5G
instructions per game against 12.9G for the reference `strong` (9.92x), `fast` 15.5G against
8.4G for the reference `standard` (1.85x, 100 matches at 10 workers).

## Strengthened search design

### Core loop and the `Mechanisms` composition

`crates/gcg-search/src/mcts.rs` is the reference search's loop (`ai/mcts.py:82-339`) with
explicit extension points. At every point a Phase 6 mechanism can change, the loop calls the
`Mechanisms` trait (`crates/gcg-search/src/mech/mod.rs`): root moves, the budget, world
building, priors, selection, rollout choices, the horizon, the leaf value, the engine-error
leaf, the backup and the pre-game choices. Every method's default is the reference behaviour
(`crates/gcg-search/src/mech/reference.rs`).

`Composed` dispatches each method to the work packages' implementations in
`crates/gcg-search/src/mech/<package>/` by the flags of `MechanismFlags`
(`crates/gcg-search/src/flags.rs`), in a fixed order written in the module documentation of
`mech/mod.rs`. With every flag off the composition is the reference, and the null
configuration replays the reference bit for bit: `crates/gcg-search/tests/null_config.rs`
compares action, alternatives, statistics and generator state on every decision of the D2
vectors, and `tests/null_config_search_set.rs` (feature `slow`) does the same on every searched
decision of the tier-1 `search` corpus set. A flag whose mechanism is not implemented is
refused (`StrengthenedConfig::check_mechanisms`), never ignored. `StrengthenedConfig` has no
`Default`; each preset spells out every field (W-136).

### Observation

`SearchAgent::choose` (`crates/gcg-search/src/agent.rs`) receives the true state through the
shared `Agent` trait, builds an `Observation` from it at once
(`crates/gcg-search/src/observation.rs`) and passes only the observation to the search. The
observation holds the observer's information-set state, in which every card identity hidden
from the observer has been redrawn from the observer's own pools, so the hidden identities,
hidden history and game generator of the true state are not in it. Worlds are determinised
from that state. Rustdoc `compile_fail` tests show that no code outside the crate can reach a
`GameState` through an observation, and `gcg-diff lints` rule 5 checks that the agent uses the
true state only for the decision counter, the log and `Observation::new`.

### The adopted mechanisms

- **`adaptive_horizon`** (W-01, `mech/p6a/horizon.rs`). A rollout that reaches its horizon
  (`horizon_turns` = 2 turn boundaries after the root) in an unsettled position plays one more
  turn boundary, once per line. Unsettled means, judged in the line's own determinised world:
  a battle, an effect frame, a pending trigger or a non-main decision is still open, or either
  side's race estimate is at most two turns.
- **`capped_rollout_continuation`** (W-15, `mech/p6a/capped.rs`). The reference stops a
  rollout after 250 decisions (`max_rollout_decisions`), possibly mid-battle, and scores it
  there. With the flag, a capped line is continued on a copy with the deterministic policy, for
  at most 64 more decisions, to the next main-phase decision of either player, and scored
  there; a line still unsettled after that scores 0.5. The cap was never reached on the
  evaluation panel (0 capped rollouts in every M15 row), so the mechanism is a guard that costs
  nothing while the cap is not hit.
- **`tactical_solver`** (W-03, `mech/p6b/solver.rs`). An exact, bounded AND-OR search of the
  rest of one turn on a determinised world, played by the engine on clones: the attacker's
  attacks (one per class of interchangeable attackers), Commands, abilities, pairings and plays;
  every defender reply with the engine's eligible Blockers and the defender's determinised
  tricks. It claims a lethal only when every explored defender reply loses. In a rollout, the
  turn player plays a proved lethal line. Its engine applies count as auxiliary applies, capped
  at 16 per iteration of the decision's iteration cap; past the cap the rollouts are the
  reference's.
- **`exclude_capped_play_draw`** (W-44, `mech/p6h/play_draw.rs`). The play/draw choice plays
  the reference's paired simulations on the same seeds. A pair in which either game is capped,
  drawn or failed leaves the paired estimate, so the choice rests on finished games only;
  going second needs a gain of more than two paired standard errors. With every game finished
  the choice is the reference's bit for bit.
- **`engine_error_leaf`** (W-143, `mech/p6o/mod.rs`). A simulated line that ends in an engine
  error (for example the W-143 redirect cycle) scores as a loss of the searching player instead
  of failing the real game. An unimplemented card is never absorbed: it stays a hard failure.
- **`kill_aware_targeting`** (W-26, `mech/p6f/`). The rollout policy's Damage targeting prefers
  an enemy Unit the damage destroys, valued by what the kill removes, over a Unit that survives
  it. Its policy-level regret per Damage decision is 0.0537 against the reference policy's
  0.0754 (difference -0.0217 [-0.0324, -0.0110], `docs/port/eval/weakness-status/GAP-A1.md`).
- **`tree_lethal_select`** (W-25, `mech/gapa1/mod.rs`). At a MAIN decision inside the tree, a
  lethal line that the tactical solver proves for the deciding player is selected before the
  moves ordered by the shared policy prior, so neither side's nodes over-extend into a lethal
  that the solver can see (the W-25 over-extension scenario: 0/16 over-extensions with the
  flag, 16/16 for the reference).

### Effort accounting

Each search counts, per decision, iterations, engine applies by purpose (tree, rollout,
auxiliary), worlds built and capped rollouts (PLAN §6.6). `gcg-sim` reports the totals and
per-game distributions per seat in `results.json` `ai.effort` (schema 1.2.0). These counts are
deterministic and never stop a search.

### Determinism

- Every random choice of the search comes from the per-decision generator
  `SplitMix64(derive_seed(agent_seed, agent_name, "decision", k))` or from a `derive_seed` of
  it; agent seeds derive from the master seed and the match, game and seat
  (`docs/port/PLAN.md` §7.2).
- Nothing reads a clock, the thread id or the worker count. `gcg-diff lints` rules 2 and 3
  reject clock, sleep, timeout and randomness APIs in the crates.
- `scripts/verify_all_rust.sh` runs `gcg-sim benchmark --ai-preset fast` at `--workers 1`, `4`
  and `10` and requires byte-identical reports, and the same for the reference search and
  `gcg-eval head-to-head`.
- The search's floating-point math goes through the platform's C math library, as the
  reference's does. Its decisions are identical across worker counts and runs on one platform;
  identity across platforms is not claimed. `portable_math` (pure-Rust `libm`) is implemented
  but not adopted (W-137).

## Ablation

Row kinds and statistics follow PLAN §12.4 and the "Phase 6 outcomes" section of
[`docs/port/WEAKNESSES.md`](port/WEAKNESSES.md): 400 BO1 games in mirrored blocks of four on the
15 tuning pairs, forced play/draw unless marked "agents", dev-role masters, confirm-role
masters run once. Each cell gives wins/N (rate, Wilson 95%), the paired difference with its
Newcombe 95% interval and the McNemar discordant counts.

- *Individual*: the null configuration at the reference `standard` budget plus the mechanism,
  against `ref:standard`.
- *Leave-one-out*: the adopted bundle at twice the pre-M15 `standard` budget (256 / 640 / 2560)
  against the same bundle without the mechanism (dev 21500200, all rows on the same deals). A
  positive difference means the mechanism helps.

| Mechanism | Individual | Leave-one-out | Evidence |
| --- | --- | --- | --- |
| `adaptive_horizon` | dev 20100100: 218/400 (0.545, [0.496, 0.593]), +0.090 [+0.018, +0.161], 37/19; confirmation (confirm 30100100): 830/1600 (0.519, [0.494, 0.543]), +0.037 [+0.000, +0.075], 132/102 | 203/400 (0.507, [0.459, 0.556]), +0.015 [-0.058, +0.088], 30/27 | `docs/port/eval/weakness-status/P6-A.md`, `docs/port/eval/M15/rows/dev-loo-adaptive_horizon-400.json` |
| `capped_rollout_continuation` | dev 20100100: 208/400 (0.520, [0.471, 0.569]), +0.040 [-0.033, +0.112], 32/24; identical to the placebo game for game | 207/400 (0.517, [0.469, 0.566]), +0.035 [-0.032, +0.101], 27/20 (the family's placebo: the cap never fires) | `docs/port/eval/weakness-status/P6-A.md`, `docs/port/eval/M15/rows/dev-loo-capped_rollout_continuation-400.json` |
| `tactical_solver` | dev 20200100: 212/400 (0.530, [0.481, 0.578]), +0.060 [-0.007, +0.126], 30/18; confirmation at twice the budget (confirm 30200200): 811/1600 (0.507, [0.482, 0.531]), +0.014 [-0.023, +0.050], 116/105 | 215/400 (0.537, [0.488, 0.586]), +0.075 [+0.009, +0.140], 31/16 | `docs/port/eval/P6-B/rows/dev-tactical_solver-400.json`, `docs/port/eval/M15/rows/dev-loo-tactical_solver-400.json` |
| `exclude_capped_play_draw` | agents, dev 20800044: 212/400 (0.530, [0.481, 0.578]), +0.060 [-0.007, +0.126], 30/18; identical to the null control game for game (0 of 400 changed) | agents, dev 21500250: 216/400 (0.540, [0.491, 0.588]), +0.080 [+0.002, +0.156], 40/24; identical to that protocol's placebo (0 of 400 changed) | `docs/port/eval/weakness-status/P6-H.md`, `docs/port/eval/M15/rows/dev-loo-agents-exclude_capped_play_draw-400.json` |
| `engine_error_leaf` | the null configuration with the flag replays all 661 searched decisions of the tier-1 `search` set with 0 error leaves (`crates/gcg-search/tests/null_config_search_set.rs`) | 207/400 (0.517, [0.469, 0.566]), +0.035 [-0.032, +0.101], 27/20; identical to the placebo game for game (0 of 400 changed) | `docs/port/eval/weakness-status/P6-O.md`, `docs/port/eval/M15/rows/dev-loo-engine_error_leaf-400.json` |

| `kill_aware_targeting` (M15b) | P6-F, dev 20600100: 216/400 (0.540, [0.491, 0.588]), +0.080 [+0.009, +0.150], 35/19; confirmation (confirm 30600100): 817/1600 (0.511, [0.486, 0.535]), +0.021 [-0.015, +0.057], 116/99 | `B_core` with it against `B_core` at twice the pre-M15 budget, dev 22710100: 207/400 (0.517), +0.035 [-0.040, +0.109], 33/26; confirmation (confirm 30271100): 830/1600 (0.519, [0.494, 0.543]), +0.037 [+0.001, +0.074], 129/99 | `docs/port/eval/weakness-status/P6-F.md`, `docs/port/eval/weakness-status/GAP-A1.md` |
| `tree_lethal_select` (M15b) | dev 22710300: 217/400 (0.542), +0.085 [+0.016, +0.153], 34/17; confirmation (confirm 30271800): 843/1600 (0.527, [0.502, 0.551]), +0.054 [+0.018, +0.089], 128/85 | from the shipped `fast` (confirm 30271810): +0.021 [-0.014, +0.056]; from `B_core` at twice the budget (confirm 30271820): +0.007 [-0.029, +0.044] | `docs/port/eval/weakness-status/GAP-A1.md` |

The two zero-effect rows (`exclude_capped_play_draw` and `engine_error_leaf`) are guards: they
change a decision only when a simulation is capped, drawn or fails, which did not happen in
those rows. The agents-protocol placebo's +0.080 shows how far a 400-game row moves with no
change at all (`docs/port/eval/weakness-status/M15-bundle.md`).

Bundle rows (`docs/port/eval/weakness-status/M15-bundle.md`), each against the null
configuration at twice the pre-M15 `standard` budget:

| Bundle | Mechanisms | Result |
| --- | --- | --- |
| `B_core` (adopted) | `adaptive_horizon`, `capped_rollout_continuation`, `tactical_solver` | dev 21500100: 217/400 (0.542, [0.493, 0.591]), +0.085 [+0.007, +0.162]; confirmation (confirm 30150100): 854/1600 (0.534, [0.509, 0.558]), +0.068 [+0.030, +0.105], 148/94 |
| `B_mid` | `B_core` plus 14 mechanisms that were null alone | 212/400 (0.530); against `B_core` on the same deals -0.013 [-0.061, +0.036] |
| `B_full` | `B_mid` plus `subtree_reuse` | 193/400 (0.482); against `B_core` -0.060 [-0.111, -0.009] (reused visits count toward the budget, so fewer fresh iterations) |
| M15b (adopted) | the M15 shipped set plus `kill_aware_targeting` and `tree_lethal_select`, at the shipped `standard` budget | against the M15 `standard` on the 15 tuning pairs (confirm 30500001, same deals): 831/1620 (0.513, [0.489, 0.537]); against `ref:strong` (tuning 10500001): 950/1620 (0.586, [0.562, 0.610]); against greedy (dev 20500001): 338/420 (0.805, [0.764, 0.840]), where the M15 `standard` scored 328/420 (0.781) on the same deals (`docs/port/eval/M15b/README.md`) |

Against the reference, the bundle's effect at equal iterations is within noise: on the same
deals against `ref:strong`, the bundle's curve point minus the control curve without the bundle
is -0.018 [-0.066, +0.031] per game at 2x and +0.030 [-0.017, +0.077] at 8x, while the control
alone scored 212/400 (0.530) at 2x and 215/400 (0.537) at 8x (`docs/port/eval/M15/README.md`,
step 3).

**Not adopted.** Every other Phase 6 mechanism is implemented behind its flag, measured with
individual rows (and high-budget or confirmation rows where promising), and left off because
it measured null or negative. Their status is `resolved by measurement (not adopted)` or, where
the plan has remaining work, `open`. The rows are in each item's entry of
[`docs/port/WEAKNESSES.md`](port/WEAKNESSES.md) and in the package records of
`docs/port/eval/weakness-status/`:

| Package | Flags | Items | Record |
| --- | --- | --- | --- |
| P6-A horizon and leaf | `generalized_leaf`; fixed `horizon_turns` 3 or 4 | W-01, W-02 | `docs/port/eval/weakness-status/P6-A.md` |
| P6-B tactical solver | `lethal_root`, `threat_leaf`, `critical_extension`, `effective_blockers`, `solver_can_kill` | W-03, W-16, W-17, W-32, W-38 | `docs/port/eval/weakness-status/P6-B.md` |
| P6-C hidden information | `opponent_buckets`, `belief_worlds`, `belief_last_n`, `exact_pools`, `opponent_view_share`, `opponent_list` | W-05, W-07, W-08, W-134, W-139 | `docs/port/eval/weakness-status/P6-C.md`, `docs/port/eval/weakness-status/GAP-B.md` |
| P6-D tree and budget | `root_trees`, `visited_set_backup`, `in_tree_filters`, `transpositions`, `subtree_reuse`, `macro_multi_pick`, `prior_worlds` | W-04, W-06, W-11, W-18, W-138, W-140 | `docs/port/eval/weakness-status/P6-D.md` |
| P6-E root filters and targets | `ex_variants`, `typed_fingerprint`, `prune_worlds`, `branch_all_selects`, `rest_prune_worlds`, `target_rollouts`, `real_idle_line` | W-09, W-10, W-12, W-13, W-14, W-141 | `docs/port/eval/weakness-status/P6-E.md` |
| P6-F rollout policy | `calibrated_policy`, `modal_lookahead`, `yes_no_lookahead`, `end_phase_actions`, `burst_decline_list`, `order_trigger_scorer`, `exhaustive_polarity`, `exact_fight`, `opponent_knobs`, `kill_aware_targeting` | W-23 to W-31, W-163 | `docs/port/eval/weakness-status/P6-F.md` |
| P6-G evaluation | `refit_weights`, `phase_weights`, `keyword_features`, `ability_count_features`, `hand_slots`, `burst_shield_probability`, `no_tempo_feature`, `determinized_fit`, `leaf_density` | W-34 to W-37, W-39, W-142 | `docs/port/eval/weakness-status/P6-G.md` |
| P6-H pre-game | `paired_mulligan` | W-45 | `docs/port/eval/weakness-status/P6-H.md` |
| P6-O design | `portable_math` | W-137 | `docs/port/eval/weakness-status/P6-O.md` |
| GAP-A1 | `target_multi_rollouts`, `tree_lethal_select` | W-14, W-17 | `docs/port/eval/weakness-status/GAP-A1.md` |

## How strength is evaluated

**Panel.** Eight decks: the three example decks and the five evaluation decks of
`examples/decks/eval/` (`examples/decks/eval/PAIRS.md`, `docs/port/eval/panel.json`). Two decks
are held out, `blue-red-tempo` and `green-white-link`, chosen by their copy overlap with every
tuning and AI test deck. All 28 pairs are played; the 13 pairs containing a held-out deck are
held-out pairs, never used for tuning, screening or budget curves, and `gcg-eval` refuses them
under `--role tuning|dev|confirm`. The split was committed before any tuning run.

**Seeds.** Master seeds have roles with disjoint ranges (PLAN A14,
`crates/gcg-eval/src/contracts.rs`): tuning 10,000,000-19,999,999, dev 20,000,000-29,999,999,
confirm 30,000,000-30,999,999, eval 90,000,000-90,999,999 and review 91,000,000-91,999,999.
`gcg-eval --role R` rejects a master outside its range and records every master in
`docs/port/eval/seed-ledger.json`. Pair `p` plays on `derive_seed(master, "pairing", p)`.

**Games.** `gcg-eval head-to-head` plays BO1 games in mirrored blocks of four: two deals, each
played twice with the agents swapped between seats. With `--choose-first seat0` the harness
answers the play/draw decision itself (seat 0 goes first), so each agent is on the play in
exactly half of every block. 400 games per pair, Wilson 95% intervals per pair, pooled over all
pairs and pooled over the held-out pairs alone; draws count as non-wins.

**Gates** (PLAN §12.6, read strictly as in A26; `gcg-eval gates --check` computes them from the
`head-to-head --out` reports):

| Gate | Comparison | Decks | Bar |
| --- | --- | --- | --- |
| E1 | `search:standard` vs `ref:strong` | panel, 28 pairs, 11,200 games | pooled Wilson lower bound at least 0.55 over all pairs and over the held-out pairs; no held-out pair with an upper bound below 0.5 |
| E2 | `search:fast` vs `ref:standard` | panel, 28 pairs, 11,200 games | pooled point estimate at least 0.5 (all and held-out); no pair with an upper bound below 0.5 |
| E3 | `search:standard` vs `random` | AI test decks (400 games) and panel | 400/400, or every loss documented with its replay |
| E4 | `search:standard` vs `greedy` | AI test decks (400 games) and panel | Wilson lower bound above 0.764 on the test decks, and pooled over all panel pairs and over the held-out pairs |

The 0.764 bar is the upper end of the Wilson interval of the reference's pre-change result
against greedy on the AI test decks (289/400, [0.677, 0.764], the brief); the test-deck rows
compare like for like and are not held-out evidence.

Evaluation 1's E1 report came from this run (the ledger entry of master 90200001 in
`docs/port/eval/seed-ledger.json` records the configuration; the worker count changes no
output), and the gates were computed from its six reports:

<!-- doc-check: skip a final-evaluation run: 11,200 games on an eval-role master that appends to the committed seed ledger and rewrites committed evidence -->
```bash
target/release/gcg-eval head-to-head search:standard ref:strong --decks panel --pairs all \
  --games 11200 --seed 90200001 --role eval --choose-first seat0 --workers auto \
  --out docs/port/eval/final-1/e1-standard-vs-ref-strong.json
target/release/gcg-eval gates --check \
  --e1 docs/port/eval/final-1/e1-standard-vs-ref-strong.json \
  --e2 docs/port/eval/final-1/e2-fast-vs-ref-standard.json \
  --e3-tests docs/port/eval/final-1/e3-tests-standard-vs-random.json \
  --e3-panel docs/port/eval/final-1/e3-panel-standard-vs-random.json \
  --e4-tests docs/port/eval/final-1/e4-tests-standard-vs-greedy.json \
  --e4-panel docs/port/eval/final-1/e4-panel-standard-vs-greedy.json
```

## Measured strength

<!-- lead: final evaluation 2 table -->

### Evaluation 1

Copied from `docs/port/eval/final-1/README.md` (shipped presets of commit 82b0b34, binaries
built at 34b3107, eval-role masters 90200001-90200006, forced play/draw, 28 panel pairs x 400
games with 13 held out, and the AI test decks at 400 games). It is an earlier measurement than
the table above and is kept as a record:

| Gate | Result | Brief bar | Plan A26 (also held-out pairs alone) |
| --- | --- | --- | --- |
| E1 `standard` vs ref `strong` | 6268/11200 = 0.5596 [0.5504, 0.5688]; held-out 2885/5200 = 0.5548 [0.5413, 0.5683]; no held-out pair UB < 0.5 | pooled LB >= 0.55: pass (by 0.0004) | held-out LB 0.5413: fail |
| E2 `fast` vs ref `standard` | 6171/11200 = 0.5510 [0.5418, 0.5602]; held-out 0.5544; no pair UB < 0.5 | pass | pass |
| E3 `standard` vs random, test decks | 400/400 | pass | pass |
| E3 `standard` vs random, panel | 11173/11200 (27 losses) | losses must be documented with replays | not documented: fail |
| E4 `standard` vs greedy, test decks | 307/400 = 0.7675 [0.7237, 0.8062] | LB > 0.764: fail | fail |
| E4 `standard` vs greedy, panel | 8655/11200 = 0.7728 [0.7649, 0.7804]; held-out 4011/5200 = 0.7713 [0.7597, 0.7826] | pooled LB > 0.764: pass (by 0.0009) | held-out LB 0.7597: fail |

`gcg-eval gates --check` (`docs/port/eval/final-1/gates.json`) reported overall fail under the
plan's strict reading for evaluation 1.

## Known weaknesses

The open AI items are listed with their remaining work in
[`docs/port/WEAKNESSES.md`](port/WEAKNESSES.md) and `docs/port/eval/weakness-gaps.md`, among
them strategy fusion at opponent nodes (W-05), close target choices (W-14), remaining
implausible plays (W-17), the opponent modelled with the searcher's own policy (W-25) and the
evaluation's transfer to unseen decks (W-36). Decks that need long, precise lines may still be
underrated: check a few replays (`gcg-sim replay FILE --log`) before trusting a surprising
matchup result.
