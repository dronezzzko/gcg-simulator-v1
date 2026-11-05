//! Every field of every reference configuration equals the dump of the live Python objects
//! (`presets.json`, `port/oracle/export_ai_vectors.py`; PLAN §6.4: "a test compares every field
//! of every preset with export_ai_vectors.py's dump"), and the factory's errors and agent names
//! are Python's (`ai/agents.py:104-127`).

mod common;

use gcg_ai_reference::{
    BASELINE_PRESETS, PRESETS, ReferenceConfig, TUNING_CONFIG, make_agent, make_reference_agent,
};
use serde_json::Value;

use common::{fhex, int, json};

fn assert_config(label: &str, c: &ReferenceConfig, want: &Value, options: &[usize]) {
    let f = |name: &str| fhex(want[name].as_str().expect("a hex float")).to_bits();
    let n = |name: &str| int(&want[name]);
    let b = |name: &str| want[name].as_bool().expect("a bool");
    assert_eq!(c.name, want["name"].as_str().expect("name"), "{label}");
    let ints = [
        i64::from(c.max_iterations),
        i64::from(c.min_iterations),
        i64::from(c.iterations_per_option),
        i64::from(c.horizon_turns),
        i64::from(c.max_rollout_decisions),
        i64::from(c.first_player_games),
        i64::from(c.mulligan_samples),
    ];
    let want_ints = [
        n("max_iterations"),
        n("min_iterations"),
        n("iterations_per_option"),
        n("horizon_turns"),
        n("max_rollout_decisions"),
        n("first_player_games"),
        n("mulligan_samples"),
    ];
    assert_eq!(ints, want_ints, "{label}");
    let floats = [
        c.exploration,
        c.prior_weight,
        c.prior_temperature,
        c.rollout_temperature,
        c.value_scale,
        c.policy_burst_yes_score,
        c.target_extension,
        c.target_extension_z,
    ]
    .map(f64::to_bits);
    let want_floats = [
        f("exploration"),
        f("prior_weight"),
        f("prior_temperature"),
        f("rollout_temperature"),
        f("value_scale"),
        f("policy_burst_yes_score"),
        f("target_extension"),
        f("target_extension_z"),
    ];
    assert_eq!(floats, want_floats, "{label}");
    let flags = [
        c.noop_branch_choices,
        c.noop_resource_aware,
        c.strict_attack_prune,
        c.prune_noop_rest_targets,
        c.merge_equivalent_moves,
        c.observation_keys,
        c.policy_rule_polarity_table,
        c.policy_zero_ap_face,
    ];
    let want_flags = [
        b("noop_branch_choices"),
        b("noop_resource_aware"),
        b("strict_attack_prune"),
        b("prune_noop_rest_targets"),
        b("merge_equivalent_moves"),
        b("observation_keys"),
        b("policy_rule_polarity_table"),
        b("policy_zero_ap_face"),
    ];
    assert_eq!(flags, want_flags, "{label}");
    // 24 fields: name, 7 ints, 8 floats, 8 flags (plus the exporter's `budgets`).
    assert_eq!(
        want.as_object().expect("an object").len(),
        25,
        "{label}: a field was added"
    );
    let budgets: Vec<i64> = options.iter().map(|&o| i64::from(c.budget(o))).collect();
    let want_budgets: Vec<i64> = want["budgets"]
        .as_array()
        .expect("budgets")
        .iter()
        .map(int)
        .collect();
    assert_eq!(budgets, want_budgets, "{label}");
}

#[test]
fn every_reference_configuration_equals_the_live_python_object() {
    let dump = json("presets.json");
    let options: Vec<usize> = dump["budget_options"]
        .as_array()
        .expect("options")
        .iter()
        .map(|v| usize::try_from(int(v)).expect("a count"))
        .collect();
    let presets = dump["presets"].as_object().expect("presets");
    let names: Vec<&str> = presets.keys().map(String::as_str).collect();
    assert_eq!(names, PRESETS.map(|(n, _)| n));
    for (name, c) in PRESETS {
        assert_config(name, c, &presets[name], &options);
        assert_eq!(
            c.agent_name(),
            dump["agent_names"][name].as_str().expect("an agent name")
        );
    }
    let baselines = dump["baseline_presets"].as_object().expect("baselines");
    assert_eq!(baselines.len(), BASELINE_PRESETS.len());
    for (name, c) in BASELINE_PRESETS {
        assert_config(name, c, &baselines[name], &options);
    }
    assert_config("tuning", &TUNING_CONFIG, &dump["tuning"], &options);
}

#[test]
fn the_factory_rejects_unknown_presets_and_kinds_with_pythons_messages() {
    let err = make_agent("mcts", "strongest", 0, false, None, None)
        .err()
        .expect("an unknown preset");
    assert_eq!(err.py_class(), "ValueError");
    assert_eq!(
        err.to_string(),
        "unknown AI preset 'strongest'; choose from ['fast', 'standard', 'strong']"
    );
    // The preset is checked first, for every kind (ai/agents.py:117-118).
    let err = make_agent("random", "x", 0, false, None, None)
        .err()
        .expect("an unknown preset");
    assert!(err.to_string().starts_with("unknown AI preset 'x'"));
    let err = make_agent("minimax", "standard", 0, false, None, None)
        .err()
        .expect("an unknown kind");
    assert_eq!(
        err.to_string(),
        "unknown agent kind 'minimax'; choose from ['random', 'greedy', 'mcts']"
    );
    let agent = make_agent("mcts", "fast", 3, false, None, None).expect("an agent");
    assert_eq!(agent.name(), "mcts-fast");
    assert!(agent.decision_log().is_empty());
    let baseline = make_reference_agent(
        &gcg_ai_reference::BASELINE_STANDARD,
        17,
        true,
        gcg_ai_reference::BASELINE_WEIGHTS,
    );
    assert_eq!(baseline.name(), "mcts-standard");
}
