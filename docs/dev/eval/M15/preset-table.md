# Preset table for the Rust `docs/AI.md` (M15 candidate)

The table below is the M15 candidate of the two shipped presets, for the lead to copy into the
Rust `docs/AI.md` (W-129, T-130). It equals the constants of
`crates/gcg-search/src/presets.rs` (pinned by `presets::tests` and
`crates/gcg-search/tests/null_config.rs`). The measurements behind it are in
`docs/dev/eval/M15/README.md`; the final evaluation (PLAN §12.6) and the final Part 2 timing
have not been run on it.

## Presets

`gcg-sim --ai-preset` accepts exactly `standard` (the default) and `fast`. Both run the
strengthened search (`gcg-search`). Budgets are node counts (iterations), never wall-clock: a
decision with `n` options starts from `max(min_iterations, min(max_iterations,
iterations_per_option * n))` iterations (`ai/config.py:55-57`), with the reference's close-call
extension of target choices (`target_extension`, `target_extension_z`).

| Preset | Use | Starts from (mapping) | Budget scale | `iterations_per_option` | `min_iterations` | `max_iterations` |
| --- | --- | --- | ---: | ---: | ---: | ---: |
| `standard` | uses the full search so that each player plays at the level of the strongest top-tier competitive player; for refining and polishing the final deck | reference `strong` (128 / 320 / 1280) | 8 | 1024 | 2560 | 10240 |
| `fast` | trades search depth for throughput; for rapidly screening deck variants, sub-variants and hypothetical builds | reference `standard` (64 / 160 / 640) | 1.5 | 96 | 240 | 960 |

Every other search setting is the mapped reference preset's (`ai/config.py:93-109`):

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

## Measured on the tuning pairs (M15, not the final evaluation)

Budget curves on the 15 tuning pairs, 400 games per point, tuning masters 11500100 and
11500200, forced play/draw:

| Comparison | Point | Result |
| --- | --- | --- |
| `standard` bundle vs reference `strong` | 8x (shipped), curve point | 227/400 = 0.568 [0.518, 0.615] |
| `fast` bundle vs reference `standard` | 1.5x (shipped), curve point | 228/400 = 0.570 [0.521, 0.618] |
| `gcg-sim`'s `standard` vs reference `strong` | head-to-head, dev 21500400 | 246/420 = 0.586 [0.538, 0.632] |
| `gcg-sim`'s `fast` vs reference `standard` | head-to-head, dev 21500410 | 223/420 = 0.531 [0.483, 0.578] |

The final evaluation numbers (E1-E4 on eval masters and the held-out pairs) belong in this
section once measured.
