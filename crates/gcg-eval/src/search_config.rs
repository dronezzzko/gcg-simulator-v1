//! Strengthened-search configuration files: `gcg-eval preset-benchmark --search-config FILE`
//! (PLAN §12.8) and the `search:FILE.json` agent spec (PLAN §10.4 F).
//!
//! A file is one JSON object with every field of `gcg_search::StrengthenedConfig` and nothing
//! else, so a candidate can never leave a field at a silent default (W-136): the 24
//! `SearchConfig` fields under their Python names, plus `weights`, either a name of
//! `ai/compare.py`'s `WEIGHTS` (`default`, `hand`, `baseline`, `roles`) or
//! `{"feature_set": "base"|"roles", "values": [...]}`. Counts are integers in `0..2^32`,
//! `horizon_turns` an `i32`, the float fields any JSON number, the flags booleans.
//! [`to_json`] writes a configuration in this format (fields in declaration order, weights as
//! an object).

use std::borrow::Cow;
use std::path::Path;

use gcg_ai::{FeatureSet, Weights};
use gcg_core::pyjson::Json;
use gcg_data::py_path_str;
use gcg_search::StrengthenedConfig;
use serde_json::{Map, Value};

use crate::agent_spec::{SEARCH_FIELDS, weights_named};

/// The key of the weights entry.
pub const WEIGHTS_KEY: &str = "weights";

fn field<'a>(obj: &'a Map<String, Value>, name: &str) -> Result<&'a Value, String> {
    obj.get(name)
        .ok_or_else(|| format!("missing field {name:?}"))
}

fn count(obj: &Map<String, Value>, name: &str) -> Result<u32, String> {
    field(obj, name)?
        .as_u64()
        .and_then(|v| u32::try_from(v).ok())
        .ok_or_else(|| format!("{name:?} must be an integer in [0, {}]", u32::MAX))
}

fn int32(obj: &Map<String, Value>, name: &str) -> Result<i32, String> {
    field(obj, name)?
        .as_i64()
        .and_then(|v| i32::try_from(v).ok())
        .ok_or_else(|| {
            format!(
                "{name:?} must be an integer in [{}, {}]",
                i32::MIN,
                i32::MAX
            )
        })
}

fn float(obj: &Map<String, Value>, name: &str) -> Result<f64, String> {
    field(obj, name)?
        .as_f64()
        .ok_or_else(|| format!("{name:?} must be a number"))
}

fn flag(obj: &Map<String, Value>, name: &str) -> Result<bool, String> {
    field(obj, name)?
        .as_bool()
        .ok_or_else(|| format!("{name:?} must be true or false"))
}

fn weights(v: &Value) -> Result<Weights, String> {
    if let Some(name) = v.as_str() {
        return weights_named(name)
            .cloned()
            .ok_or_else(|| format!("unknown weights {name:?}"));
    }
    let obj = v
        .as_object()
        .ok_or_else(|| "\"weights\" must be a name or an object".to_owned())?;
    if let Some(extra) = obj.keys().find(|k| *k != "feature_set" && *k != "values") {
        return Err(format!("unknown weights field {extra:?}"));
    }
    let set_name = field(obj, "feature_set")?
        .as_str()
        .ok_or_else(|| "\"feature_set\" must be a string".to_owned())?;
    let set = FeatureSet::from_value(set_name)
        .ok_or_else(|| format!("unknown feature set {set_name:?}"))?;
    let values = field(obj, "values")?
        .as_array()
        .ok_or_else(|| "\"values\" must be a list of numbers".to_owned())?
        .iter()
        .map(|x| {
            x.as_f64()
                .ok_or_else(|| "\"values\" must be a list of numbers".to_owned())
        })
        .collect::<Result<Vec<f64>, String>>()?;
    Weights::new(values, set).map_err(|e| e.to_string())
}

/// A configuration from a parsed JSON value.
///
/// # Errors
/// The first missing, unknown or mistyped field.
pub fn from_value(v: &Value) -> Result<StrengthenedConfig, String> {
    let obj = v
        .as_object()
        .ok_or_else(|| "a search configuration is a JSON object".to_owned())?;
    if let Some(extra) = obj
        .keys()
        .find(|k| *k != WEIGHTS_KEY && !SEARCH_FIELDS.contains(&k.as_str()))
    {
        return Err(format!("unknown field {extra:?}"));
    }
    let name = field(obj, "name")?
        .as_str()
        .ok_or_else(|| "\"name\" must be a string".to_owned())?;
    Ok(StrengthenedConfig {
        name: Cow::Owned(name.to_owned()),
        max_iterations: count(obj, "max_iterations")?,
        min_iterations: count(obj, "min_iterations")?,
        iterations_per_option: count(obj, "iterations_per_option")?,
        exploration: float(obj, "exploration")?,
        prior_weight: float(obj, "prior_weight")?,
        prior_temperature: float(obj, "prior_temperature")?,
        horizon_turns: int32(obj, "horizon_turns")?,
        rollout_temperature: float(obj, "rollout_temperature")?,
        value_scale: float(obj, "value_scale")?,
        max_rollout_decisions: count(obj, "max_rollout_decisions")?,
        first_player_games: count(obj, "first_player_games")?,
        mulligan_samples: count(obj, "mulligan_samples")?,
        policy_burst_yes_score: float(obj, "policy_burst_yes_score")?,
        noop_branch_choices: flag(obj, "noop_branch_choices")?,
        noop_resource_aware: flag(obj, "noop_resource_aware")?,
        strict_attack_prune: flag(obj, "strict_attack_prune")?,
        prune_noop_rest_targets: flag(obj, "prune_noop_rest_targets")?,
        merge_equivalent_moves: flag(obj, "merge_equivalent_moves")?,
        observation_keys: flag(obj, "observation_keys")?,
        target_extension: float(obj, "target_extension")?,
        target_extension_z: float(obj, "target_extension_z")?,
        policy_rule_polarity_table: flag(obj, "policy_rule_polarity_table")?,
        policy_zero_ap_face: flag(obj, "policy_zero_ap_face")?,
        weights: weights(field(obj, WEIGHTS_KEY)?)?,
    })
}

/// Reads a configuration file.
///
/// # Errors
/// `cannot read search configuration {path}: ...` or `{path}: {problem}`.
pub fn load(path: &Path) -> Result<StrengthenedConfig, String> {
    let shown = py_path_str(path);
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read search configuration {shown}: {e}"))?;
    let value: Value =
        serde_json::from_str(&text).map_err(|e| format!("{shown}: not JSON: {e}"))?;
    from_value(&value).map_err(|e| format!("{shown}: {e}"))
}

/// The file format of a configuration (fields in declaration order).
#[must_use]
pub fn to_json(cfg: &StrengthenedConfig) -> Json {
    let mut o = Json::object();
    o.set("name", &*cfg.name);
    o.set("max_iterations", cfg.max_iterations);
    o.set("min_iterations", cfg.min_iterations);
    o.set("iterations_per_option", cfg.iterations_per_option);
    o.set("exploration", cfg.exploration);
    o.set("prior_weight", cfg.prior_weight);
    o.set("prior_temperature", cfg.prior_temperature);
    o.set("horizon_turns", cfg.horizon_turns);
    o.set("rollout_temperature", cfg.rollout_temperature);
    o.set("value_scale", cfg.value_scale);
    o.set("max_rollout_decisions", cfg.max_rollout_decisions);
    o.set("first_player_games", cfg.first_player_games);
    o.set("mulligan_samples", cfg.mulligan_samples);
    o.set("policy_burst_yes_score", cfg.policy_burst_yes_score);
    o.set("noop_branch_choices", cfg.noop_branch_choices);
    o.set("noop_resource_aware", cfg.noop_resource_aware);
    o.set("strict_attack_prune", cfg.strict_attack_prune);
    o.set("prune_noop_rest_targets", cfg.prune_noop_rest_targets);
    o.set("merge_equivalent_moves", cfg.merge_equivalent_moves);
    o.set("observation_keys", cfg.observation_keys);
    o.set("target_extension", cfg.target_extension);
    o.set("target_extension_z", cfg.target_extension_z);
    o.set("policy_rule_polarity_table", cfg.policy_rule_polarity_table);
    o.set("policy_zero_ap_face", cfg.policy_zero_ap_face);
    let mut w = Json::object();
    w.set("feature_set", cfg.weights.feature_set().as_str());
    w.set("values", cfg.weights.values().to_vec());
    o.set(WEIGHTS_KEY, w);
    o
}

#[cfg(test)]
mod tests {
    use super::*;
    use gcg_core::pyjson::{DumpOptions, dumps};

    fn sample() -> StrengthenedConfig {
        StrengthenedConfig {
            name: Cow::Borrowed("candidate"),
            max_iterations: 640,
            min_iterations: 160,
            iterations_per_option: 64,
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
            weights: gcg_ai::ROLE_WEIGHTS.clone(),
        }
    }

    #[test]
    fn round_trip_and_strictness() {
        let cfg = sample();
        let text = dumps(&to_json(&cfg), DumpOptions::PRETTY_SORTED_UTF8);
        let value: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        assert_eq!(from_value(&value), Ok(cfg.clone()));

        let mut named = value.clone();
        named["weights"] = Value::from("hand");
        assert_eq!(
            from_value(&named).map(|c| c.weights),
            Ok(gcg_ai::HAND_WEIGHTS.clone())
        );

        let mut missing = value.clone();
        if let Some(o) = missing.as_object_mut() {
            o.remove("observation_keys");
        }
        assert_eq!(
            from_value(&missing),
            Err("missing field \"observation_keys\"".to_owned())
        );

        let mut extra = value.clone();
        extra["exploraton"] = Value::from(1.0);
        assert_eq!(
            from_value(&extra),
            Err("unknown field \"exploraton\"".to_owned())
        );

        let mut negative = value;
        negative["max_iterations"] = Value::from(-1);
        assert_eq!(
            from_value(&negative),
            Err("\"max_iterations\" must be an integer in [0, 4294967295]".to_owned())
        );
    }
}
