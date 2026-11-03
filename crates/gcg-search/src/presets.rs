//! The shipped `gcg-sim` presets of the strengthened search (PLAN §6.4, §12.8) and the
//! configuration helpers the search reads.
//!
//! The presets start from the reference presets as the brief maps them: `standard` from
//! reference `strong`, `fast` from reference `standard` (`ai/config.py:93-109`), renamed, with
//! `ROLE_WEIGHTS` (`DEFAULT_WEIGHTS`, `ai/evaluation.py:129`). M15 (PLAN §12.8) changed only
//! the three budget fields, scaled from the mapped reference preset's, and the mechanisms;
//! every other search setting is the mapped reference preset's:
//!
//! | Preset | Mapping | Budget scale | `iterations_per_option` | `min_iterations` | `max_iterations` |
//! | --- | --- | ---: | ---: | ---: | ---: |
//! | `standard` | reference `strong` (128 / 320 / 1280) | 8 | 1024 | 2560 | 10240 |
//! | `fast` | reference `standard` (64 / 160 / 640) | 1.5 | 96 | 240 | 960 |
//!
//! Both presets run the same mechanisms (`SHIPPED_MECHANISMS`). The measurements behind the
//! choice are in `docs/port/eval/M15/README.md` (bundle and leave-one-out rows, budget curves,
//! per-game cost).

use std::borrow::Cow;
use std::sync::LazyLock;

use gcg_ai::{HAND_WEIGHTS, PolicyKnobs, ROLE_WEIGHTS, SearchSettings, TUNED_WEIGHTS, Weights};
use gcg_core::stable_hash::StableHasher;

use crate::{MechanismFlags, OpponentList, Preset, StrengthenedConfig};

/// The mechanisms of both shipped presets, every field spelled out (M15 bundle,
/// `docs/port/eval/M15/README.md`): the adaptive horizon (W-01) with the capped-rollout
/// continuation (W-15), the tactical solver in rollouts (W-03), capped play/draw simulations
/// excluded from the paired estimate (W-44), and a simulated line ending in an engine error
/// scored as a loss instead of failing the real game (W-143). Everything else is the reference.
const SHIPPED_MECHANISMS: MechanismFlags = MechanismFlags {
    adaptive_horizon: true,
    generalized_leaf: false,
    capped_rollout_continuation: true,
    tactical_solver: true,
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
    exclude_capped_play_draw: true,
    paired_mulligan: false,
    portable_math: false,
    engine_error_leaf: true,
    target_multi_rollouts: None,
    tree_lethal_select: false,
};

/// `standard`: reference `strong` (`ai/config.py:98-109`) at 8 times its budget, every field
/// spelled out.
static STANDARD: LazyLock<StrengthenedConfig> = LazyLock::new(|| StrengthenedConfig {
    name: Cow::Borrowed("standard"),
    max_iterations: 10240,
    min_iterations: 2560,
    iterations_per_option: 1024,
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

/// `fast`: reference `standard` (`ai/config.py:93-97`) at 1.5 times its budget, every field
/// spelled out.
static FAST: LazyLock<StrengthenedConfig> = LazyLock::new(|| StrengthenedConfig {
    name: Cow::Borrowed("fast"),
    max_iterations: 960,
    min_iterations: 240,
    iterations_per_option: 96,
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

/// `p`'s preset at its mapped reference preset's budget with only `engine_error_leaf` on: the
/// reference settings, renamed (the shipped preset before M15, PLAN §12.8's starting point).
fn mapped_reference(p: Preset) -> StrengthenedConfig {
    let (iterations_per_option, min_iterations, max_iterations) = match p {
        Preset::Standard => (128, 320, 1280),
        Preset::Fast => (64, 160, 640),
    };
    let mut mechanisms = MechanismFlags::NULL;
    mechanisms.engine_error_leaf = true;
    StrengthenedConfig {
        iterations_per_option,
        min_iterations,
        max_iterations,
        mechanisms,
        ..preset_config(p).clone()
    }
}

static STANDARD_REFERENCE: LazyLock<StrengthenedConfig> =
    LazyLock::new(|| mapped_reference(Preset::Standard));
static FAST_REFERENCE: LazyLock<StrengthenedConfig> =
    LazyLock::new(|| mapped_reference(Preset::Fast));

/// The configuration a shipped preset starts from (PLAN §12.8): its mapped reference preset
/// (`standard` from reference `strong`, `fast` from reference `standard`) with every reference
/// setting and budget, renamed, `ROLE_WEIGHTS`, and only `engine_error_leaf` on. Tests that
/// measure a mechanism at the reference budgets start from it; the shipped presets are
/// [`preset_config`].
#[must_use]
pub fn reference_mapping(p: Preset) -> &'static StrengthenedConfig {
    match p {
        Preset::Standard => &STANDARD_REFERENCE,
        Preset::Fast => &FAST_REFERENCE,
    }
}

/// The named weight tables `weights_id` recognises, in the order it checks them.
static NAMED_WEIGHTS: [(&str, &Weights); 3] = [
    ("ROLE_WEIGHTS", &ROLE_WEIGHTS),
    ("TUNED_WEIGHTS", &TUNED_WEIGHTS),
    ("HAND_WEIGHTS", &HAND_WEIGHTS),
];

/// The identity of evaluation weights in results 1.2.0 `ai.search.weights` (PLAN §12.7): the
/// name of the shipped table whose feature set and bit patterns they equal, otherwise
/// `<feature set>:<16 hex digits>`, the stable key of the feature set and the little-endian
/// bit patterns (`gcg_core::stable_hash`), so any other weights get their own identity.
#[must_use]
pub fn weights_id(w: &Weights) -> String {
    let bits = |x: &Weights| -> Vec<u64> { x.values().iter().map(|v| v.to_bits()).collect() };
    if let Some((name, _)) = NAMED_WEIGHTS
        .iter()
        .find(|(_, t)| t.feature_set() == w.feature_set() && bits(t) == bits(w))
    {
        return (*name).to_owned();
    }
    let mut h = StableHasher::new();
    h.update(w.feature_set().as_str().as_bytes());
    h.update(&[0]);
    for b in bits(w) {
        h.update(&b.to_le_bytes());
    }
    format!("{}:{:016x}", w.feature_set().as_str(), h.finish().0)
}

/// The results 1.2.0 `ai.search` settings (PLAN §12.7, O9) of the strengthened search with
/// configuration `cfg` for the shipped preset `preset` (`cfg` is `preset_config(preset)`,
/// possibly with overridden mechanisms): the preset, its reference mapping, the iteration
/// budget, the auxiliary apply cap ([`crate::mcts::AUX_APPLIES_PER_ITERATION`]), the enabled
/// mechanisms (sorted) and the weights' identity ([`weights_id`]). The agent factories of
/// `gcg-sim` and `gcg-eval benchmark` return it from `AgentFactory::search_block`.
#[must_use]
pub fn search_settings(preset: Preset, cfg: &StrengthenedConfig) -> SearchSettings {
    SearchSettings {
        preset: preset.as_str(),
        mapping: preset.mapping(),
        iterations_per_option: cfg.iterations_per_option,
        min_iterations: cfg.min_iterations,
        max_iterations: cfg.max_iterations,
        aux_apply_cap: crate::mcts::AUX_APPLIES_PER_ITERATION,
        mechanisms: cfg.mechanisms.enabled(),
        weights: weights_id(&cfg.weights),
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
    fn presets_scale_the_mapped_reference_budgets() {
        let standard = preset_config(Preset::Standard);
        let fast = preset_config(Preset::Fast);
        assert_eq!(standard.agent_name(), Preset::Standard.agent_name());
        assert_eq!(fast.agent_name(), Preset::Fast.agent_name());
        let budgets = |c: &StrengthenedConfig| [1, 2, 3, 5, 10, 20, 30].map(|n| c.budget(n));
        // Reference `strong` (320, 320, 384, 640, 1280, 1280, 1280) times 8.
        assert_eq!(
            budgets(standard),
            [2560, 2560, 3072, 5120, 10240, 10240, 10240]
        );
        // Reference `standard` (160, 160, 192, 320, 640, 640, 640) times 1.5.
        assert_eq!(budgets(fast), [240, 240, 288, 480, 960, 960, 960]);
        let (standard_ref, fast_ref) = (
            reference_mapping(Preset::Standard),
            reference_mapping(Preset::Fast),
        );
        assert_eq!(
            budgets(standard_ref),
            [320, 320, 384, 640, 1280, 1280, 1280]
        );
        assert_eq!(budgets(fast_ref), [160, 160, 192, 320, 640, 640, 640]);
        // A preset and its starting point differ only in the budget and the mechanisms.
        for p in Preset::ALL {
            let start = reference_mapping(p);
            assert_eq!(start.mechanisms.enabled(), ["engine_error_leaf"]);
            let shipped = preset_config(p);
            let rebudgeted = StrengthenedConfig {
                iterations_per_option: start.iterations_per_option,
                min_iterations: start.min_iterations,
                max_iterations: start.max_iterations,
                mechanisms: start.mechanisms,
                ..shipped.clone()
            };
            assert_eq!(&rebudgeted, start);
        }
        assert_eq!(standard.weights, ROLE_WEIGHTS);
        assert_eq!(fast.knobs().burst_yes_score.to_bits(), 4.0f64.to_bits());
        assert!(fast.knobs().rule_polarity_table && fast.knobs().zero_ap_face);
    }

    #[test]
    fn search_settings_describe_each_preset() {
        for (p, mapping) in [
            (Preset::Standard, "reference strong"),
            (Preset::Fast, "reference standard"),
        ] {
            let cfg = preset_config(p);
            let s = search_settings(p, cfg);
            assert_eq!(s.preset, p.as_str());
            assert_eq!(s.mapping, mapping);
            assert_eq!(
                (s.iterations_per_option, s.min_iterations, s.max_iterations),
                (
                    cfg.iterations_per_option,
                    cfg.min_iterations,
                    cfg.max_iterations
                )
            );
            assert_eq!(s.aux_apply_cap, crate::mcts::AUX_APPLIES_PER_ITERATION);
            assert_eq!(s.mechanisms, cfg.mechanisms.enabled());
            assert_eq!(s.weights, "ROLE_WEIGHTS");
        }
        // An overridden mechanism shows up in the settings, sorted by name.
        let mut cfg = preset_config(Preset::Fast).clone();
        assert_eq!(cfg.mechanisms.set_text("root_trees", "4"), Some(Ok(())));
        assert_eq!(
            search_settings(Preset::Fast, &cfg).mechanisms,
            [
                "adaptive_horizon",
                "capped_rollout_continuation",
                "engine_error_leaf",
                "exclude_capped_play_draw",
                "root_trees",
                "tactical_solver"
            ]
        );
    }

    #[test]
    fn weights_have_a_name_or_a_digest() {
        assert_eq!(weights_id(&ROLE_WEIGHTS), "ROLE_WEIGHTS");
        assert_eq!(weights_id(&TUNED_WEIGHTS), "TUNED_WEIGHTS");
        assert_eq!(weights_id(&HAND_WEIGHTS), "HAND_WEIGHTS");
        let with_first = |x: f64| {
            let mut v = ROLE_WEIGHTS.values().to_vec();
            v[0] = x;
            weights_id(&Weights::new(v, ROLE_WEIGHTS.feature_set()).expect("17 weights"))
        };
        assert_eq!(with_first(ROLE_WEIGHTS.values()[0]), "ROLE_WEIGHTS");
        let (negative, positive) = (with_first(-0.0), with_first(0.0));
        assert!(
            negative.starts_with("roles:") && negative.len() == "roles:".len() + 16,
            "{negative}"
        );
        // Bit patterns, not values: -0.0 and 0.0 are different weights.
        assert_ne!(negative, positive);
        assert_eq!(negative, with_first(-0.0));
        // Weights rebuilt from a table's values are that table.
        let base = Weights::new(TUNED_WEIGHTS.values().to_vec(), TUNED_WEIGHTS.feature_set())
            .expect("14 weights");
        assert_eq!(weights_id(&base), "TUNED_WEIGHTS");
    }

    #[test]
    fn shipped_presets_enable_the_m15_bundle() {
        for p in Preset::ALL {
            let cfg = preset_config(p);
            assert_eq!(
                cfg.mechanisms.enabled(),
                [
                    "adaptive_horizon",
                    "capped_rollout_continuation",
                    "engine_error_leaf",
                    "exclude_capped_play_draw",
                    "tactical_solver"
                ]
            );
            assert_eq!(cfg.mechanisms, SHIPPED_MECHANISMS);
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
            cfg.mechanisms.set_text("opponent_list", "archetype"),
            Some(Ok(()))
        );
        // W-134 (GAP-B): the archetype mode is provided.
        assert_eq!(cfg.check_mechanisms(), Ok(()));
        assert_eq!(
            cfg.mechanisms.set_text("opponent_list", "known"),
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
