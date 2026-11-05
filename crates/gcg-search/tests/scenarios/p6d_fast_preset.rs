//! W-18 (S12): the Rust port of `tests/ai/test_fast_preset.py` for the strengthened presets.
//! `fast` is the screening preset: it starts from the reference `standard` budgets (the brief's
//! mapping, `docs/port/BRIEF.md` `<ai_presets>`), searches at most half of `standard`'s budget
//! at every decision width, and differs from `standard` only in its budget and noise settings,
//! so a `fast` screen runs the same strengthened search. The weakened variants the fidelity
//! check ranks (`docs/port/eval/P6-D/decks/`, with `tests/data/decks/red-green-zeon-weakened.txt`)
//! are legal decks.

use std::path::PathBuf;

use gcg_data::{ValidateOptions, load_deck, validate};
use gcg_search::{Preset, StrengthenedConfig, preset_config};

/// `fast` searches at most `1 / BUDGET_RATIO` of `standard`'s budget at every width (the
/// starting mapping: reference `standard` against reference `strong`, `ai/config.py:93-109`).
const BUDGET_RATIO: u32 = 2;

/// The settings `fast` may change (`BUDGET_FIELDS`, `test_fast_preset.py:13-22`).
const BUDGET_FIELDS: [&str; 8] = [
    "name",
    "max_iterations",
    "min_iterations",
    "iterations_per_option",
    "exploration",
    "rollout_temperature",
    "first_player_games",
    "mulligan_samples",
];

#[test]
fn fast_searches_at_most_half_of_the_standard_budget_at_every_width() {
    let (fast, standard) = (preset_config(Preset::Fast), preset_config(Preset::Standard));
    for n in 1..=40 {
        assert!(
            standard.budget(n) >= BUDGET_RATIO * fast.budget(n),
            "{n}: {} vs {}",
            standard.budget(n),
            fast.budget(n)
        );
    }
}

/// The fields of `a` and `b` that differ, by name.
fn differing(a: &StrengthenedConfig, b: &StrengthenedConfig) -> Vec<&'static str> {
    let StrengthenedConfig {
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
        weights,
        mechanisms,
    } = a;
    let mut out = Vec::new();
    let mut check = |field: &'static str, same: bool| {
        if !same {
            out.push(field);
        }
    };
    check("name", *name == b.name);
    check("max_iterations", *max_iterations == b.max_iterations);
    check("min_iterations", *min_iterations == b.min_iterations);
    check(
        "iterations_per_option",
        *iterations_per_option == b.iterations_per_option,
    );
    check(
        "exploration",
        exploration.to_bits() == b.exploration.to_bits(),
    );
    check(
        "prior_weight",
        prior_weight.to_bits() == b.prior_weight.to_bits(),
    );
    check(
        "prior_temperature",
        prior_temperature.to_bits() == b.prior_temperature.to_bits(),
    );
    check("horizon_turns", *horizon_turns == b.horizon_turns);
    check(
        "rollout_temperature",
        rollout_temperature.to_bits() == b.rollout_temperature.to_bits(),
    );
    check(
        "value_scale",
        value_scale.to_bits() == b.value_scale.to_bits(),
    );
    check(
        "max_rollout_decisions",
        *max_rollout_decisions == b.max_rollout_decisions,
    );
    check(
        "first_player_games",
        *first_player_games == b.first_player_games,
    );
    check("mulligan_samples", *mulligan_samples == b.mulligan_samples);
    check(
        "policy_burst_yes_score",
        policy_burst_yes_score.to_bits() == b.policy_burst_yes_score.to_bits(),
    );
    check(
        "noop_branch_choices",
        *noop_branch_choices == b.noop_branch_choices,
    );
    check(
        "noop_resource_aware",
        *noop_resource_aware == b.noop_resource_aware,
    );
    check(
        "strict_attack_prune",
        *strict_attack_prune == b.strict_attack_prune,
    );
    check(
        "prune_noop_rest_targets",
        *prune_noop_rest_targets == b.prune_noop_rest_targets,
    );
    check(
        "merge_equivalent_moves",
        *merge_equivalent_moves == b.merge_equivalent_moves,
    );
    check("observation_keys", *observation_keys == b.observation_keys);
    check(
        "target_extension",
        target_extension.to_bits() == b.target_extension.to_bits(),
    );
    check(
        "target_extension_z",
        target_extension_z.to_bits() == b.target_extension_z.to_bits(),
    );
    check(
        "policy_rule_polarity_table",
        *policy_rule_polarity_table == b.policy_rule_polarity_table,
    );
    check(
        "policy_zero_ap_face",
        *policy_zero_ap_face == b.policy_zero_ap_face,
    );
    check("weights", *weights == b.weights);
    check("mechanisms", *mechanisms == b.mechanisms);
    out
}

#[test]
fn fast_differs_from_standard_only_in_budget_and_noise() {
    let diff = differing(preset_config(Preset::Fast), preset_config(Preset::Standard));
    assert!(!diff.is_empty());
    for field in &diff {
        assert!(BUDGET_FIELDS.contains(field), "{field} differs");
    }
}

#[test]
fn weakened_variants_used_for_fidelity_are_legal() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    for file in [
        "tests/data/decks/red-green-zeon-weakened.txt",
        "docs/port/eval/P6-D/decks/blue-white-federation-weakened.txt",
        "docs/port/eval/P6-D/decks/purple-white-tekkadan-weakened.txt",
    ] {
        let deck = load_deck(&root.join(file)).unwrap_or_else(|e| panic!("{file}: {e}"));
        let problems = validate(
            &deck,
            ValidateOptions {
                banlist: true,
                implemented: None,
            },
        );
        assert!(problems.is_empty(), "{file}: {problems:?}");
    }
}
