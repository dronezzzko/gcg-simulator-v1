//! The null configuration of D2's gate (PLAN §10.4 D2, X12) and the comparison of one search
//! by the reference and by the `gcg-search` copy.

#![allow(
    dead_code,
    reason = "each test file uses its own subset of the helpers"
)]

use gcg_ai::{AgentCx, NoTrace, Scratch, SearchResult, Weights};
use gcg_ai_reference::ReferenceConfig;
use gcg_core::pool::{Cancel, Pool, TaskCx};
use gcg_core::rng::SplitMix64;
use gcg_engine::GameState;
use gcg_search::StrengthenedConfig;

/// Every field of the reference configuration copied, every Phase 6 mechanism off. The
/// destructuring is exhaustive, so a field added to either type fails to compile here.
pub fn null_config(reference: &ReferenceConfig, weights: &Weights) -> StrengthenedConfig {
    let ReferenceConfig {
        name,
        max_iterations,
        min_iterations,
        iterations_per_option,
        exploration,
        prior_weight,
        prior_temperature,
        horizon_turns,
        rollout_temperature,
        value_scale,
        max_rollout_decisions,
        first_player_games,
        mulligan_samples,
        policy_burst_yes_score,
        noop_branch_choices,
        noop_resource_aware,
        strict_attack_prune,
        prune_noop_rest_targets,
        merge_equivalent_moves,
        observation_keys,
        target_extension,
        target_extension_z,
        policy_rule_polarity_table,
        policy_zero_ap_face,
    } = reference.clone();
    StrengthenedConfig {
        name,
        max_iterations,
        min_iterations,
        iterations_per_option,
        exploration,
        prior_weight,
        prior_temperature,
        horizon_turns,
        rollout_temperature,
        value_scale,
        max_rollout_decisions,
        first_player_games,
        mulligan_samples,
        policy_burst_yes_score,
        noop_branch_choices,
        noop_resource_aware,
        strict_attack_prune,
        prune_noop_rest_targets,
        merge_equivalent_moves,
        observation_keys,
        target_extension,
        target_extension_z,
        policy_rule_polarity_table,
        policy_zero_ap_face,
        weights: weights.clone(),
    }
}

/// One search of each implementation from the same per-decision seed: the results and the
/// generator states afterwards.
pub fn both_searches(
    st: &GameState,
    player: u8,
    reference: &ReferenceConfig,
    weights: &Weights,
    seed: u64,
) -> ((SearchResult, u64), (SearchResult, u64)) {
    let cancel = Cancel::new();
    let null = null_config(reference, weights);
    let run = |strengthened: bool| {
        Pool::run_inline(|scratch: &mut Scratch, par: &TaskCx<'_, Scratch>| {
            let mut cx = AgentCx {
                scratch,
                par,
                cancel: &cancel,
            };
            let mut rng = SplitMix64::new(seed);
            let result = if strengthened {
                gcg_search::search(
                    &gcg_search::Observation::new(st, player),
                    &null,
                    &mut rng,
                    &mut cx,
                    &mut NoTrace,
                )
            } else {
                gcg_ai_reference::search(
                    st,
                    player,
                    reference,
                    weights,
                    &mut rng,
                    &mut cx,
                    &mut NoTrace,
                )
            };
            result.map(|r| (r, rng.state))
        })
        .unwrap_or_else(|e| panic!("search failed: {e}"))
    };
    (run(false), run(true))
}
