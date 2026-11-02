//! The null configuration of D2's gate (PLAN §10.4 D2, X12) and the comparison of one search
//! by the reference and by the `gcg-search` copy.

#![allow(
    dead_code,
    reason = "each test file uses its own subset of the helpers"
)]

use gcg_ai::{AgentCx, AgentError, Alternative, NoTrace, Scratch, SearchResult, Weights};
use gcg_ai_reference::ReferenceConfig;
use gcg_core::pool::{Cancel, Pool, TaskCx};
use gcg_core::rng::SplitMix64;
use gcg_engine::{Action, DecisionKind, GameState};
use gcg_search::{MechanismEffort, MechanismFlags, OpponentList, StrengthenedConfig};

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
        mechanisms: NULL_MECHANISMS,
    }
}

/// Every Phase 6 mechanism off, every field spelled out (no `Default`, W-136): a field added to
/// `MechanismFlags` fails to compile here. Equal to `MechanismFlags::NULL` (checked below).
pub const NULL_MECHANISMS: MechanismFlags = MechanismFlags {
    adaptive_horizon: false,
    generalized_leaf: false,
    capped_rollout_continuation: false,
    tactical_solver: false,
    lethal_root: false,
    threat_leaf: false,
    critical_extension: false,
    effective_blockers: false,
    solver_can_kill: false,
    opponent_buckets: None,
    belief_worlds: None,
    belief_last_n: None,
    exact_pools: false,
    opponent_view_share: 0.0,
    opponent_list: OpponentList::Known,
    root_trees: None,
    visited_set_backup: false,
    in_tree_filters: false,
    transpositions: false,
    subtree_reuse: false,
    macro_multi_pick: false,
    prior_worlds: None,
    ex_variants: false,
    typed_fingerprint: false,
    prune_worlds: None,
    branch_all_selects: false,
    rest_prune_worlds: false,
    target_rollouts: false,
    real_idle_line: false,
    calibrated_policy: false,
    modal_lookahead: false,
    yes_no_lookahead: false,
    end_phase_actions: false,
    burst_decline_list: false,
    order_trigger_scorer: false,
    exhaustive_polarity: false,
    exact_fight: false,
    opponent_knobs: false,
    kill_aware_targeting: false,
    refit_weights: false,
    phase_weights: false,
    keyword_features: false,
    ability_count_features: false,
    hand_slots: false,
    burst_shield_probability: false,
    no_tempo_feature: false,
    determinized_fit: false,
    leaf_density: false,
    exclude_capped_play_draw: false,
    paired_mulligan: false,
    portable_math: false,
    engine_error_leaf: false,
};

/// A REDRAW or CHOOSE_FIRST choice and its alternatives.
pub type Choice = (Action, Vec<Alternative>);

/// One REDRAW or CHOOSE_FIRST choice from the same per-decision seed: the reference functions
/// (`mulligan_choice`, `first_player_choice`) on the true state, and `gcg_search::pregame` on
/// the observation; the choices and the generator states afterwards.
pub fn both_pregames(
    st: &GameState,
    player: u8,
    cfg: &StrengthenedConfig,
    seed: u64,
) -> ((Choice, u64), (Choice, u64)) {
    let cancel = Cancel::new();
    let redraw = st.pending().expect("a decision").hdr.kind == DecisionKind::Redraw;
    let run = |strengthened: bool| {
        Pool::run_inline(|scratch: &mut Scratch, par: &TaskCx<'_, Scratch>| {
            let mut cx = AgentCx {
                scratch,
                par,
                cancel: &cancel,
            };
            let mut rng = SplitMix64::new(seed);
            let result = if strengthened {
                gcg_search::Observation::new(st, player)
                    .map_err(AgentError::from)
                    .and_then(|obs| gcg_search::pregame(&obs, cfg, &mut rng, &mut cx))
            } else if redraw {
                Ok(gcg_ai::mulligan_choice(
                    st,
                    player,
                    cfg.mulligan_samples,
                    &mut rng,
                ))
            } else {
                gcg_ai::first_player_choice(
                    st,
                    player,
                    cfg.first_player_games,
                    &mut rng,
                    &cfg.knobs(),
                    &mut cx,
                )
            };
            result.map(|r| (r, rng.state))
        })
        .unwrap_or_else(|e| panic!("pregame failed: {e}"))
    };
    (run(false), run(true))
}

/// One search of each implementation from the same per-decision seed: the results and the
/// generator states afterwards.
/// One strengthened search with `cfg` on the observation of `player` at `st`: the result, the
/// mechanism counters and the generator state afterwards.
pub fn strengthened_search(
    st: &GameState,
    player: u8,
    cfg: &StrengthenedConfig,
    seed: u64,
) -> (SearchResult, MechanismEffort, u64) {
    let cancel = Cancel::new();
    Pool::run_inline(|scratch: &mut Scratch, par: &TaskCx<'_, Scratch>| {
        let mut cx = AgentCx {
            scratch,
            par,
            cancel: &cancel,
        };
        let mut rng = SplitMix64::new(seed);
        let obs = gcg_search::Observation::new(st, player)?;
        let (r, e) = gcg_search::search_with_effort(&obs, cfg, &mut rng, &mut cx, &mut NoTrace)?;
        Ok::<_, AgentError>((r, e, rng.state))
    })
    .unwrap_or_else(|e| panic!("search failed: {e}"))
}

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
                gcg_search::Observation::new(st, player)
                    .map_err(gcg_ai::AgentError::from)
                    .and_then(|obs| {
                        gcg_search::search(&obs, &null, &mut rng, &mut cx, &mut NoTrace)
                    })
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
