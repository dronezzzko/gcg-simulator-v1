//! The shipped `gcg-sim` presets of the strengthened search (PLAN §6.4) and the
//! configuration helpers the search reads.
//!
//! The presets start from the reference presets as the brief maps them: `standard` from
//! reference `strong`, `fast` from reference `standard` (`ai/config.py:93-109`), renamed, with
//! `ROLE_WEIGHTS` (`DEFAULT_WEIGHTS`, `ai/evaluation.py:129`). Of the Phase 6 mechanisms only
//! `engine_error_leaf` is on so far; the packages adopt theirs and §12.8 tunes the budgets.

use std::borrow::Cow;
use std::sync::LazyLock;

use gcg_ai::{PolicyKnobs, ROLE_WEIGHTS};

use crate::{MechanismFlags, OpponentList, Preset, StrengthenedConfig};

/// The mechanisms of both shipped presets, every field spelled out: the reference everywhere
/// except that a simulated line ending in an engine error scores as a loss instead of failing
/// the real game (W-143).
const SHIPPED_MECHANISMS: MechanismFlags = MechanismFlags {
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
    engine_error_leaf: true,
};

/// `standard`: reference `strong` (`ai/config.py:98-109`), every field spelled out.
static STANDARD: LazyLock<StrengthenedConfig> = LazyLock::new(|| StrengthenedConfig {
    name: Cow::Borrowed("standard"),
    max_iterations: 1280,
    min_iterations: 320,
    iterations_per_option: 128,
    exploration: 0.15,
    prior_weight: 0.5,
    prior_temperature: 1.5,
    horizon_turns: 2,
    rollout_temperature: 0.3,
    value_scale: 1.0,
    max_rollout_decisions: 250,
    first_player_games: 48,
    mulligan_samples: 192,
    policy_burst_yes_score: 4.0,
    noop_branch_choices: true,
    noop_resource_aware: true,
    strict_attack_prune: true,
    prune_noop_rest_targets: true,
    merge_equivalent_moves: true,
    observation_keys: true,
    target_extension: 3.0,
    target_extension_z: 2.0,
    policy_rule_polarity_table: true,
    policy_zero_ap_face: true,
    weights: ROLE_WEIGHTS.clone(),
    mechanisms: SHIPPED_MECHANISMS,
});

/// `fast`: reference `standard` (`ai/config.py:93-97`), every field spelled out.
static FAST: LazyLock<StrengthenedConfig> = LazyLock::new(|| StrengthenedConfig {
    name: Cow::Borrowed("fast"),
    max_iterations: 640,
    min_iterations: 160,
    iterations_per_option: 64,
    exploration: 0.3,
    prior_weight: 0.5,
    prior_temperature: 1.5,
    horizon_turns: 2,
    rollout_temperature: 0.5,
    value_scale: 1.0,
    max_rollout_decisions: 250,
    first_player_games: 24,
    mulligan_samples: 96,
    policy_burst_yes_score: 4.0,
    noop_branch_choices: true,
    noop_resource_aware: true,
    strict_attack_prune: true,
    prune_noop_rest_targets: true,
    merge_equivalent_moves: true,
    observation_keys: true,
    target_extension: 3.0,
    target_extension_z: 2.0,
    policy_rule_polarity_table: true,
    policy_zero_ap_face: true,
    weights: ROLE_WEIGHTS.clone(),
    mechanisms: SHIPPED_MECHANISMS,
});

/// The configuration of a shipped preset (PLAN §6.4).
#[must_use]
pub fn preset_config(p: Preset) -> &'static StrengthenedConfig {
    match p {
        Preset::Standard => &STANDARD,
        Preset::Fast => &FAST,
    }
}

impl StrengthenedConfig {
    /// `budget(n_options)` (`ai/config.py:55-57`):
    /// `max(min_iterations, min(max_iterations, iterations_per_option * n_options))` in exact
    /// integer arithmetic.
    #[must_use]
    pub fn budget(&self, n_options: usize) -> u32 {
        let n = u64::try_from(n_options).unwrap_or(u64::MAX);
        let wanted = u64::from(self.iterations_per_option).saturating_mul(n);
        let capped = wanted.min(u64::from(self.max_iterations));
        let capped = u32::try_from(capped).unwrap_or(self.max_iterations);
        capped.max(self.min_iterations)
    }

    /// `knobs_for(config)` (`ai/policy.py:63-69`).
    #[must_use]
    pub const fn knobs(&self) -> PolicyKnobs {
        PolicyKnobs {
            burst_yes_score: self.policy_burst_yes_score,
            rule_polarity_table: self.policy_rule_polarity_table,
            zero_ap_face: self.policy_zero_ap_face,
        }
    }

    /// The agent name `search-{name}` (PLAN A9).
    #[must_use]
    pub fn agent_name(&self) -> String {
        format!("{}{}", crate::AGENT_PREFIX, self.name)
    }

    /// Whether this build implements every mechanism the configuration enables; the search
    /// and the agent refuse a configuration that fails it.
    ///
    /// # Errors
    /// `mechanism <name> (<package>) is not available in this build`.
    pub fn check_mechanisms(&self) -> Result<(), String> {
        crate::mech::validate(&self.mechanisms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_start_from_the_mapped_reference_budgets() {
        let standard = preset_config(Preset::Standard);
        let fast = preset_config(Preset::Fast);
        assert_eq!(standard.agent_name(), Preset::Standard.agent_name());
        assert_eq!(fast.agent_name(), Preset::Fast.agent_name());
        let budgets = |c: &StrengthenedConfig| [1, 2, 3, 5, 10, 20, 30].map(|n| c.budget(n));
        assert_eq!(budgets(standard), [320, 320, 384, 640, 1280, 1280, 1280]);
        assert_eq!(budgets(fast), [160, 160, 192, 320, 640, 640, 640]);
        assert_eq!(standard.weights, ROLE_WEIGHTS);
        assert_eq!(fast.knobs().burst_yes_score.to_bits(), 4.0f64.to_bits());
        assert!(fast.knobs().rule_polarity_table && fast.knobs().zero_ap_face);
    }

    #[test]
    fn shipped_presets_enable_only_the_engine_error_leaf() {
        for p in Preset::ALL {
            let cfg = preset_config(p);
            assert_eq!(cfg.mechanisms.enabled(), ["engine_error_leaf"]);
            assert_eq!(cfg.check_mechanisms(), Ok(()));
        }
    }

    #[test]
    fn unavailable_mechanisms_are_refused() {
        let mut cfg = preset_config(Preset::Fast).clone();
        assert_eq!(
            cfg.mechanisms.set_text("portable_math", "true"),
            Some(Ok(()))
        );
        assert_eq!(cfg.check_mechanisms(), Ok(()));
        assert_eq!(cfg.mechanisms.set_text("root_trees", "4"), Some(Ok(())));
        assert_eq!(cfg.check_mechanisms(), Ok(()));
        assert_eq!(
            cfg.mechanisms.set_text("opponent_buckets", "8"),
            Some(Ok(()))
        );
        assert_eq!(
            cfg.check_mechanisms(),
            Err("mechanism opponent_buckets (P6-C) is not available in this build".to_owned())
        );
        assert_eq!(
            cfg.mechanisms.set_text("opponent_buckets", "none"),
            Some(Ok(()))
        );
        assert_eq!(cfg.check_mechanisms(), Ok(()));
        assert_eq!(cfg.mechanisms.set_text("no_such_flag", "true"), None);
        assert!(matches!(
            cfg.mechanisms.set_text("engine_error_leaf", "2"),
            Some(Err(_))
        ));
    }
}
