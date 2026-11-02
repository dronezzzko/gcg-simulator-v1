//! `tests/ai/test_evaluation_roles.py`, ported where it tests the evaluation (4 of 8 tests):
//! the `roles` feature set, its fitted weights and the weights' feature-set check. The other
//! four (`test_compare_selects_the_role_weights` and the three `tuning` tests) exercise
//! `ai.compare.parse_agent` and `ai.tuning`, which are `gcg-eval`'s; the exporter deselects them
//! (`port/oracle/export_ai_tests.py` `DESELECTED`) and they are not ported here.

mod aiport;

use aiport::positions::{both_can_kill, ok};
use aiport::{Recorder, registry};
use gcg_ai::{
    FEATURES, FeatureSet, ROLE_FEATURES, ROLE_WEIGHTS, TUNED_WEIGHTS, Weights, burst_density,
    evaluate, features, role_features, score,
};
use gcg_core::pymath::py_sum;

const M: &str = "test_evaluation_roles";

#[test]
fn every_ported_evaluation_roles_test_is_exported() {
    aiport::assert_ported(
        M,
        &[
            "test_role_features_keep_each_players_threats_apart".to_owned(),
            "test_role_weights_value_the_turn_players_lethal_over_the_others".to_owned(),
            "test_role_score_is_from_the_turn_players_point_of_view".to_owned(),
            "test_weights_check_their_feature_set".to_owned(),
        ],
    );
}

#[test]
fn role_features_keep_each_players_threats_apart() {
    let rec = Recorder::new(M, "test_role_features_keep_each_players_threats_apart");
    let st = both_can_kill(&registry(), true);
    let density = burst_density(&st, 0);
    let role_values = ok(role_features(&st, density));
    let base_values = ok(features(&st, 0, density));
    let roles = |name: &str| {
        role_values[ROLE_FEATURES
            .iter()
            .position(|n| *n == name)
            .expect("a role")]
    };
    let base =
        |name: &str| base_values[FEATURES.iter().position(|n| *n == name).expect("a feature")];
    for name in FEATURES.iter().filter(|n| **n != "can_kill") {
        assert_eq!(roles(name).to_bits(), base(name).to_bits(), "{name}");
    }
    assert_eq!(roles("can_kill_turn_player"), 1.0);
    assert_eq!(roles("can_kill_other"), 1.0);
    assert_eq!(base("can_kill"), 0.0);
    assert_eq!(roles("inv_race_turn_player"), 1.0 / 3.0);
    assert_eq!(roles("inv_race_other"), 1.0);
    rec.finish();
}

#[test]
fn role_weights_value_the_turn_players_lethal_over_the_others() {
    let rec = Recorder::new(
        M,
        "test_role_weights_value_the_turn_players_lethal_over_the_others",
    );
    let st = both_can_kill(&registry(), false);
    let density = burst_density(&st, 0);
    assert!(ok(evaluate(&st, 0, &ROLE_WEIGHTS, density)) > 0.9);
    assert!(ok(evaluate(&st, 0, &TUNED_WEIGHTS, density)) < 0.8);
    rec.finish();
}

#[test]
fn role_score_is_from_the_turn_players_point_of_view() {
    let rec = Recorder::new(M, "test_role_score_is_from_the_turn_players_point_of_view");
    let st = both_can_kill(&registry(), false);
    let density = burst_density(&st, 0);
    let values: Vec<f64> = (0..ROLE_FEATURES.len())
        .map(|i| 0.1 * f64::from(u32::try_from(i + 1).expect("small")))
        .collect();
    let weights = Weights::new(values.clone(), FeatureSet::Roles).expect("17 weights");
    let z = py_sum(
        values
            .iter()
            .zip(ok(role_features(&st, density)))
            .map(|(w, x)| w * x),
    );
    assert_eq!(ok(score(&st, 0, &weights, density)).to_bits(), z.to_bits());
    assert_eq!(
        ok(score(&st, 1, &weights, density)).to_bits(),
        (-z).to_bits()
    );
    // math.isclose(a + b, 1): |a + b - 1| <= 1e-9 * max(|a + b|, 1).
    let total = ok(evaluate(&st, 0, &weights, density)) + ok(evaluate(&st, 1, &weights, density));
    let scale = if total.abs() > 1.0 { total.abs() } else { 1.0 };
    assert!((total - 1.0).abs() <= 1e-9 * scale, "{total}");
    rec.finish();
}

#[test]
fn weights_check_their_feature_set() {
    let rec = Recorder::new(M, "test_weights_check_their_feature_set");
    assert_eq!(TUNED_WEIGHTS.feature_set(), FeatureSet::Base);
    assert_eq!(
        Weights::new(TUNED_WEIGHTS.values().to_vec(), FeatureSet::Base).expect("14 weights"),
        TUNED_WEIGHTS
    );
    assert_eq!(ROLE_WEIGHTS.feature_set(), FeatureSet::Roles);
    let names: Vec<&str> = ROLE_WEIGHTS.as_pairs().map(|(n, _)| n).collect();
    assert_eq!(names, ROLE_FEATURES);
    let err = Weights::new(TUNED_WEIGHTS.values().to_vec(), FeatureSet::Roles).unwrap_err();
    assert!(err.to_string().contains("expected 17 weights"), "{err}");
    // Weights(values, "other") raises "unknown feature set": no FeatureSet has that name.
    assert_eq!(FeatureSet::from_value("other"), None);
    rec.finish();
}
