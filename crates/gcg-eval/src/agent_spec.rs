//! Agent specs (docs/port/PLAN.md §10.4 F): Python's `parse_agent` grammar for `compare`
//! (`ai/compare.py:79-113`, exact, with its `ValueError` texts), the head-to-head grammar that
//! adds `ref:` and `search:` (`ref:<preset>`, `ref:baseline-<preset>`, `search:<preset>`,
//! `search:FILE.json`, each with `:field=value,...` overrides), and the agent a spec builds
//! (`make_agent` as `selfplay.play` calls it, `ai/selfplay.py:78-85`, `ai/agents.py:104-127`).
//!
//! Overrides convert their values with Python's `int`, `float`, `str` and `_flag`
//! ([`crate::pyconv`]). The Rust configurations hold counts as `u32` and `horizon_turns` as
//! `i32` (docs/port/contracts.md, gcg-ai-reference), so a value outside those ranges, which
//! Python accepts and then fails or loops on, is rejected here with a message of this port's
//! own. Every other text is Python's.

use std::borrow::Cow;
use std::path::Path;

use gcg_ai::{Agent, DEFAULT_WEIGHTS, HAND_WEIGHTS, ROLE_WEIGHTS, Weights};
use gcg_ai_reference::{
    BASELINE_PRESETS, BASELINE_WEIGHTS, ConfigError, LEAF_FLOOR, PRESETS, ReferenceConfig,
    WIN_DECAY,
};
use gcg_data::py_repr;
use gcg_search::{Preset, StrengthenedConfig};

use crate::AgentDesc;
use crate::pyconv::{py_flag, py_float, py_int};
use crate::search_config;

/// `BASELINE_PREFIX` (`ai/compare.py:50`).
pub const BASELINE_PREFIX: &str = "baseline-";
/// `BASELINES` (`ai/compare.py:51`).
pub const BASELINES: [&str; 2] = ["random", "greedy"];
/// The head-to-head prefix of a reference-search spec (PLAN §10.4 F).
pub const REF_PREFIX: &str = "ref:";
/// The head-to-head prefix of a strengthened-search spec (PLAN §10.4 F).
pub const SEARCH_PREFIX: &str = "search:";

/// `WEIGHTS` (`ai/compare.py:52-57`), in dict order.
pub static WEIGHTS: [(&str, &Weights); 4] = [
    ("default", DEFAULT_WEIGHTS),
    ("hand", &HAND_WEIGHTS),
    ("baseline", BASELINE_WEIGHTS),
    ("roles", &ROLE_WEIGHTS),
];

/// `WEIGHTS[name]`.
#[must_use]
pub fn weights_named(name: &str) -> Option<&'static Weights> {
    WEIGHTS.iter().find(|(n, _)| *n == name).map(|(_, w)| *w)
}

/// A Python list repr of strings: `['a', 'b']`.
fn list_repr<'a>(items: impl IntoIterator<Item = &'a str>) -> String {
    let parts: Vec<String> = items.into_iter().map(py_repr).collect();
    format!("[{}]", parts.join(", "))
}

/// `sorted(WEIGHTS)`, as the weights error prints it.
fn sorted_weight_names() -> String {
    let mut names: Vec<&str> = WEIGHTS.iter().map(|(n, _)| *n).collect();
    names.sort();
    list_repr(names)
}

fn weights_error() -> String {
    format!(
        "weights must be one of {} for greedy or MCTS",
        sorted_weight_names()
    )
}

/// One `field=value` item of an override list, split as `item.partition("=")`.
fn split_item(item: &str) -> Result<(&str, &str), String> {
    item.split_once('=')
        .ok_or_else(|| format!("expected field=value, got {}", py_repr(item)))
}

fn set_u32(slot: &mut u32, field: &str, value: &str) -> Result<(), String> {
    let v = py_int(value)?;
    *slot = v
        .to_u64()
        .and_then(|x| u32::try_from(x).ok())
        .ok_or_else(|| {
            format!(
                "{field} must be in [0, {}] in this port, got {}",
                u32::MAX,
                v.to_decimal()
            )
        })?;
    Ok(())
}

fn set_i32(slot: &mut i32, field: &str, value: &str) -> Result<(), String> {
    let v = py_int(value)?;
    let fits = v.to_decimal().parse::<i32>().ok();
    *slot = fits.ok_or_else(|| {
        format!(
            "{field} must be in [{}, {}] in this port, got {}",
            i32::MIN,
            i32::MAX,
            v.to_decimal()
        )
    })?;
    Ok(())
}

fn set_f64(slot: &mut f64, value: &str) -> Result<(), String> {
    *slot = py_float(value)?;
    Ok(())
}

fn set_bool(slot: &mut bool, value: &str) -> Result<(), String> {
    *slot = py_flag(value)?;
    Ok(())
}

/// `_FIELD_TYPES` (`ai/compare.py:76`) and the conversion of `:110` for the 24 `SearchConfig`
/// fields, which `ReferenceConfig` and `StrengthenedConfig` share by name and type.
/// `None`: `field` is not a `SearchConfig` field.
macro_rules! search_fields {
    ($name:ident, $ty:ty) => {
        fn $name(cfg: &mut $ty, field: &str, value: &str) -> Option<Result<(), String>> {
            Some(match field {
                "name" => {
                    cfg.name = Cow::Owned(value.to_owned());
                    Ok(())
                }
                "max_iterations" => set_u32(&mut cfg.max_iterations, field, value),
                "min_iterations" => set_u32(&mut cfg.min_iterations, field, value),
                "iterations_per_option" => set_u32(&mut cfg.iterations_per_option, field, value),
                "exploration" => set_f64(&mut cfg.exploration, value),
                "prior_weight" => set_f64(&mut cfg.prior_weight, value),
                "prior_temperature" => set_f64(&mut cfg.prior_temperature, value),
                "horizon_turns" => set_i32(&mut cfg.horizon_turns, field, value),
                "rollout_temperature" => set_f64(&mut cfg.rollout_temperature, value),
                "value_scale" => set_f64(&mut cfg.value_scale, value),
                "max_rollout_decisions" => set_u32(&mut cfg.max_rollout_decisions, field, value),
                "first_player_games" => set_u32(&mut cfg.first_player_games, field, value),
                "mulligan_samples" => set_u32(&mut cfg.mulligan_samples, field, value),
                "policy_burst_yes_score" => set_f64(&mut cfg.policy_burst_yes_score, value),
                "noop_branch_choices" => set_bool(&mut cfg.noop_branch_choices, value),
                "noop_resource_aware" => set_bool(&mut cfg.noop_resource_aware, value),
                "strict_attack_prune" => set_bool(&mut cfg.strict_attack_prune, value),
                "prune_noop_rest_targets" => set_bool(&mut cfg.prune_noop_rest_targets, value),
                "merge_equivalent_moves" => set_bool(&mut cfg.merge_equivalent_moves, value),
                "observation_keys" => set_bool(&mut cfg.observation_keys, value),
                "target_extension" => set_f64(&mut cfg.target_extension, value),
                "target_extension_z" => set_f64(&mut cfg.target_extension_z, value),
                "policy_rule_polarity_table" => {
                    set_bool(&mut cfg.policy_rule_polarity_table, value)
                }
                "policy_zero_ap_face" => set_bool(&mut cfg.policy_zero_ap_face, value),
                _ => return None,
            })
        }
    };
}

search_fields!(set_reference_field, ReferenceConfig);
search_fields!(set_search_field, StrengthenedConfig);

/// The `SearchConfig` field names in declaration order (`ai/config.py:23-46`).
pub const SEARCH_FIELDS: [&str; 24] = [
    "name",
    "max_iterations",
    "min_iterations",
    "iterations_per_option",
    "exploration",
    "prior_weight",
    "prior_temperature",
    "horizon_turns",
    "rollout_temperature",
    "value_scale",
    "max_rollout_decisions",
    "first_player_games",
    "mulligan_samples",
    "policy_burst_yes_score",
    "noop_branch_choices",
    "noop_resource_aware",
    "strict_attack_prune",
    "prune_noop_rest_targets",
    "merge_equivalent_moves",
    "observation_keys",
    "target_extension",
    "target_extension_z",
    "policy_rule_polarity_table",
    "policy_zero_ap_face",
];

/// `__post_init__` (`ai/config.py:48-53`) on a strengthened configuration: the same horizon
/// rule, so a `search:` override cannot build a configuration the reference would refuse.
fn check_search(cfg: &StrengthenedConfig) -> Result<(), String> {
    if WIN_DECAY * f64::from(cfg.horizon_turns) >= LEAF_FLOOR {
        return Err(ConfigError::HorizonTooLong {
            horizon_turns: cfg.horizon_turns,
        }
        .to_string());
    }
    cfg.check_mechanisms()
}

/// A strengthened-search field the 24-field table does not know (the Phase 6 mechanism
/// flags and constants, PLAN §12.3), set through the configuration file format
/// ([`search_config::to_json`] and [`search_config::from_value`]): the text converts by the
/// type of the field's current value (a boolean with `_flag`, an integer with `int`, a float
/// with `float`, a string as given), and the file format's checks apply. `Ok(None)`: the
/// configuration has no such field.
///
/// # Errors
/// The conversion's or the file format's message.
pub fn set_search_field_generic(
    cfg: &StrengthenedConfig,
    field: &str,
    value: &str,
) -> Result<Option<StrengthenedConfig>, String> {
    use serde_json::Value;
    if field == search_config::WEIGHTS_KEY {
        return Ok(None);
    }
    let text = gcg_core::pyjson::dumps(
        &search_config::to_json(cfg),
        gcg_core::pyjson::DumpOptions::PRETTY_SORTED_UTF8,
    );
    let mut doc: Value = serde_json::from_str(&text)
        .map_err(|e| format!("cannot re-read the search configuration: {e}"))?;
    let Some(slot) = doc.get_mut(field) else {
        return Ok(None);
    };
    let new = match &*slot {
        Value::Bool(_) => Value::Bool(py_flag(value)?),
        Value::Number(n) if n.is_f64() => {
            let x = py_float(value)?;
            serde_json::Number::from_f64(x)
                .map(Value::Number)
                .ok_or_else(|| format!("{field} must be a finite number, got {value}"))?
        }
        Value::Number(_) => {
            let v = py_int(value)?;
            let d = v.to_decimal();
            d.parse::<i64>()
                .map(Value::from)
                .or_else(|_| d.parse::<u64>().map(Value::from))
                .map_err(|_| format!("{field} must be a 64-bit integer in this port, got {d}"))?
        }
        Value::String(_) => Value::String(value.to_owned()),
        _ => return Err(format!("{field} cannot be set from the command line")),
    };
    *slot = new;
    search_config::from_value(&doc).map(Some)
}

/// The strengthened search with a reference configuration's 24 settings and `weights`, every
/// Phase 6 mechanism off: the null configuration, which replays the reference search
/// (`crates/gcg-search/tests/null_config.rs`; PLAN §12.3, §12.4 individual rows). Only the
/// agent name differs (`search-<name>`, PLAN A9), so its per-decision seeds differ from the
/// reference agent's.
#[must_use]
pub fn null_config(reference: &ReferenceConfig, weights: &Weights) -> StrengthenedConfig {
    StrengthenedConfig {
        name: reference.name.clone(),
        max_iterations: reference.max_iterations,
        min_iterations: reference.min_iterations,
        iterations_per_option: reference.iterations_per_option,
        exploration: reference.exploration,
        prior_weight: reference.prior_weight,
        prior_temperature: reference.prior_temperature,
        horizon_turns: reference.horizon_turns,
        rollout_temperature: reference.rollout_temperature,
        value_scale: reference.value_scale,
        max_rollout_decisions: reference.max_rollout_decisions,
        first_player_games: reference.first_player_games,
        mulligan_samples: reference.mulligan_samples,
        policy_burst_yes_score: reference.policy_burst_yes_score,
        noop_branch_choices: reference.noop_branch_choices,
        noop_resource_aware: reference.noop_resource_aware,
        strict_attack_prune: reference.strict_attack_prune,
        prune_noop_rest_targets: reference.prune_noop_rest_targets,
        merge_equivalent_moves: reference.merge_equivalent_moves,
        observation_keys: reference.observation_keys,
        target_extension: reference.target_extension,
        target_extension_z: reference.target_extension_z,
        policy_rule_polarity_table: reference.policy_rule_polarity_table,
        policy_zero_ap_face: reference.policy_zero_ap_face,
        weights: weights.clone(),
        mechanisms: gcg_search::MechanismFlags::NULL,
    }
}

/// The prefix of a null-configuration head: `search:null-<preset>` or
/// `search:null-baseline-<preset>`.
pub const NULL_PREFIX: &str = "null-";

/// What the head of a spec resolved to, before the overrides.
enum Base {
    Random,
    Greedy,
    Reference(ReferenceConfig),
    Search(StrengthenedConfig),
}

/// The override loop of `parse_agent` (`ai/compare.py:98-113`) for any base: `weights=` first,
/// then a configuration field (none for random and greedy), in order; `dataclasses.replace`
/// (with its `__post_init__`) only when there were changes.
fn apply_overrides(
    head: &str,
    base: Base,
    mut weights: Option<Weights>,
    overrides: &str,
) -> Result<AgentDesc, String> {
    let mut base = base;
    let mut changed = false;
    for item in overrides.split(',').filter(|s| !s.is_empty()) {
        let (field, value) = split_item(item)?;
        if field == "weights" {
            let named = weights_named(value);
            match (named, &base) {
                (Some(w), Base::Greedy | Base::Reference(_) | Base::Search(_)) => {
                    weights = Some(w.clone());
                }
                _ => return Err(weights_error()),
            }
            continue;
        }
        let no_setting = || format!("{} has no search setting {}", py_repr(head), py_repr(field));
        let applied = match &mut base {
            Base::Random | Base::Greedy => None,
            Base::Reference(cfg) => set_reference_field(cfg, field, value),
            Base::Search(cfg) => set_search_field(cfg, field, value)
                .or_else(|| cfg.mechanisms.set_text(field, value))
                .or_else(|| {
                    set_search_field_generic(cfg, field, value)
                        .transpose()
                        .map(|r| r.map(|new| *cfg = new))
                }),
        };
        applied.ok_or_else(no_setting)??;
        changed = true;
    }
    Ok(match base {
        Base::Random => AgentDesc::Random,
        Base::Greedy => AgentDesc::Greedy {
            weights: weights.unwrap_or_else(|| DEFAULT_WEIGHTS.clone()),
        },
        Base::Reference(config) => {
            if changed {
                config.check().map_err(|e| e.to_string())?;
            }
            AgentDesc::Reference {
                config,
                weights: weights.unwrap_or_else(|| DEFAULT_WEIGHTS.clone()),
            }
        }
        Base::Search(mut config) => {
            if changed {
                check_search(&config)?;
            }
            if let Some(w) = weights {
                config.weights = w;
            }
            AgentDesc::Search { config }
        }
    })
}

fn reference_preset(head: &str) -> Option<&'static ReferenceConfig> {
    PRESETS.iter().find(|(n, _)| *n == head).map(|(_, c)| *c)
}

fn baseline_preset(head: &str) -> Option<&'static ReferenceConfig> {
    let rest = head.strip_prefix(BASELINE_PREFIX)?;
    BASELINE_PRESETS
        .iter()
        .find(|(n, _)| *n == rest)
        .map(|(_, c)| *c)
}

/// The reference heads of `parse_agent`: `PRESETS`, then `baseline-<BASELINE_PRESETS>` with
/// `BASELINE_WEIGHTS` (`ai/compare.py:87-94`).
fn reference_head(head: &str) -> Option<(Base, Option<Weights>)> {
    if let Some(cfg) = reference_preset(head) {
        return Some((Base::Reference(cfg.clone()), None));
    }
    baseline_preset(head).map(|cfg| (Base::Reference(cfg.clone()), Some(BASELINE_WEIGHTS.clone())))
}

/// The reference spec names, in Python's `choices` order (`ai/compare.py:96`).
fn reference_choices() -> Vec<String> {
    let mut out: Vec<String> = PRESETS.iter().map(|(n, _)| (*n).to_owned()).collect();
    out.extend(
        BASELINE_PRESETS
            .iter()
            .map(|(n, _)| format!("{BASELINE_PREFIX}{n}")),
    );
    out
}

/// `parse_agent(text)` (`ai/compare.py:79-113`): `random` | `greedy` | `<preset>` |
/// `baseline-<preset>`, each optionally followed by `:field=value,...`. A preset is the
/// reference search (`make_agent("mcts", config=...)`); greedy and the reference play with
/// `DEFAULT_WEIGHTS` unless the spec names weights (`ai/agents.py:119`).
///
/// # Errors
/// Python's `ValueError` text.
pub fn parse_agent(text: &str) -> Result<AgentDesc, String> {
    let (head, overrides) = text.split_once(':').unwrap_or((text, ""));
    let (base, weights) = if head == "random" {
        (Base::Random, None)
    } else if head == "greedy" {
        (Base::Greedy, None)
    } else if let Some(found) = reference_head(head) {
        found
    } else {
        let mut choices: Vec<String> = BASELINES.iter().map(|s| (*s).to_owned()).collect();
        choices.extend(reference_choices());
        return Err(format!(
            "unknown agent {}; choose from {}",
            py_repr(head),
            list_repr(choices.iter().map(String::as_str))
        ));
    };
    apply_overrides(head, base, weights, overrides)
}

/// The head-to-head grammar (PLAN §10.4 F): `ref:<preset>`, `ref:baseline-<preset>`,
/// `search:<preset>` (`standard`, `fast`: `gcg_search::preset_config`),
/// `search:null-<preset>` and `search:null-baseline-<preset>` (the [`null_config`] of a
/// reference preset with the weights its `ref:` spec plays with, PLAN §12.4), `search:FILE.json`
/// ([`search_config::load`]; the path cannot contain `:`), each with `:field=value,...`
/// overrides (a `search:` override may name any field of the configuration file format,
/// [`set_search_field_generic`]); anything else is [`parse_agent`]'s grammar (`random`,
/// `greedy`, and the bare reference presets).
///
/// # Errors
/// The parse error; for `search:FILE.json` also the file's.
pub fn parse_agent_spec(text: &str) -> Result<AgentDesc, String> {
    if let Some(rest) = text.strip_prefix(REF_PREFIX) {
        let (head, overrides) = rest.split_once(':').unwrap_or((rest, ""));
        let Some((base, weights)) = reference_head(head) else {
            return Err(format!(
                "unknown reference preset {}; choose from {}",
                py_repr(head),
                list_repr(reference_choices().iter().map(String::as_str))
            ));
        };
        return apply_overrides(head, base, weights, overrides);
    }
    if let Some(rest) = text.strip_prefix(SEARCH_PREFIX) {
        let (head, overrides) = rest.split_once(':').unwrap_or((rest, ""));
        let null = head
            .strip_prefix(NULL_PREFIX)
            .and_then(reference_head)
            .and_then(|(base, w)| match base {
                Base::Reference(cfg) => {
                    Some(null_config(&cfg, w.as_ref().unwrap_or(DEFAULT_WEIGHTS)))
                }
                _ => None,
            });
        let config = if let Some(p) = Preset::from_value(head) {
            gcg_search::preset_config(p).clone()
        } else if let Some(cfg) = null {
            cfg
        } else if head.ends_with(".json") {
            search_config::load(Path::new(head))?
        } else {
            return Err(format!(
                "unknown search preset {}; choose from {}, null-<reference preset> or a \
                 FILE.json",
                py_repr(head),
                list_repr(Preset::ALL.iter().map(|p| p.as_str()))
            ));
        };
        return apply_overrides(head, Base::Search(config), None, overrides);
    }
    parse_agent(text)
}

/// A strengthened-search configuration with `:field=value,...` overrides applied, as a
/// `search:` spec applies them (`head` names the spec in error messages).
///
/// # Errors
/// The first bad override.
pub fn search_with(
    config: StrengthenedConfig,
    head: &str,
    overrides: &str,
) -> Result<StrengthenedConfig, String> {
    match apply_overrides(head, Base::Search(config), None, overrides)? {
        AgentDesc::Search { config } => Ok(config),
        _ => unreachable!("a search base stays a search agent"),
    }
}

/// `spec` with `overrides` appended (`:` after a bare head, `,` after existing overrides).
#[must_use]
pub fn with_overrides(spec: &str, overrides: &str) -> String {
    if overrides.is_empty() {
        return spec.to_owned();
    }
    let rest = spec
        .strip_prefix(REF_PREFIX)
        .or_else(|| spec.strip_prefix(SEARCH_PREFIX))
        .unwrap_or(spec);
    if rest.contains(':') {
        format!("{spec},{overrides}")
    } else {
        format!("{spec}:{overrides}")
    }
}

/// The agent's name (`Agent::name`): seed-path material (`ai/base.py:65`).
#[must_use]
pub fn agent_name(desc: &AgentDesc) -> String {
    match desc {
        AgentDesc::Random => "random".to_owned(),
        AgentDesc::Greedy { .. } => "greedy".to_owned(),
        AgentDesc::Reference { config, .. } => config.agent_name(),
        AgentDesc::Search { config } => format!("{}{}", gcg_search::AGENT_PREFIX, config.name),
    }
}

/// `make_agent(kind, seed=seed, log=log, config=..., weights=...)` (`ai/agents.py:104-127`)
/// for a parsed spec; the strengthened search through `gcg_search::make_agent`.
#[must_use]
pub fn make_agent(desc: &AgentDesc, seed: u64, log: bool) -> Box<dyn Agent> {
    match desc {
        AgentDesc::Random => gcg_ai::make_random_agent(seed, log),
        AgentDesc::Greedy { weights } => gcg_ai::make_greedy_agent(seed, log, weights.clone()),
        AgentDesc::Reference { config, weights } => {
            gcg_ai_reference::make_reference_agent(config, seed, log, weights)
        }
        AgentDesc::Search { config } => gcg_search::make_agent(config, seed, log),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weight_names_and_choices_are_pythons() {
        assert_eq!(
            sorted_weight_names(),
            "['baseline', 'default', 'hand', 'roles']"
        );
        assert!(std::ptr::eq(
            weights_named("default").unwrap_or(&HAND_WEIGHTS),
            DEFAULT_WEIGHTS
        ));
        assert_eq!(weights_named("baseline"), Some(&gcg_ai::TUNED_WEIGHTS));
        assert_eq!(
            list_repr(reference_choices().iter().map(String::as_str)),
            "['fast', 'standard', 'strong', 'baseline-standard', 'baseline-strong']"
        );
    }

    #[test]
    fn head_to_head_grammar() {
        let AgentDesc::Reference { config, weights } =
            parse_agent_spec("ref:baseline-strong:exploration=0.5").unwrap_or(AgentDesc::Random)
        else {
            panic!("a reference spec")
        };
        assert_eq!(config.name, "strong");
        assert_eq!(config.exploration.to_bits(), 0.5_f64.to_bits());
        assert_eq!(&weights, BASELINE_WEIGHTS);
        assert_eq!(
            parse_agent_spec("ref:random").err().as_deref(),
            Some(
                "unknown reference preset 'random'; choose from ['fast', 'standard', 'strong', \
                 'baseline-standard', 'baseline-strong']"
            )
        );
        assert_eq!(
            parse_agent_spec("search:strong").err().as_deref(),
            Some(
                "unknown search preset 'strong'; choose from ['standard', 'fast'], \
                 null-<reference preset> or a FILE.json"
            )
        );
        assert_eq!(parse_agent_spec("random"), Ok(AgentDesc::Random));
        assert!(matches!(
            parse_agent_spec("standard"),
            Ok(AgentDesc::Reference { .. })
        ));
        assert_eq!(
            parse_agent("standard:max_iterations=-1").err().as_deref(),
            Some("max_iterations must be in [0, 4294967295] in this port, got -1")
        );
        assert_eq!(
            parse_agent("standard:horizon_turns=2147483648")
                .err()
                .as_deref(),
            Some("horizon_turns must be in [-2147483648, 2147483647] in this port, got 2147483648")
        );
    }

    /// PLAN §12.4: `search:null-<preset>` is the reference preset's configuration with its
    /// `ref:` weights, every mechanism off; a baseline keeps `BASELINE_WEIGHTS`.
    #[test]
    fn null_heads_carry_the_reference_settings() {
        for (name, cfg) in PRESETS {
            let Ok(AgentDesc::Search { config }) = parse_agent_spec(&format!("search:null-{name}"))
            else {
                panic!("search:null-{name}")
            };
            assert_eq!(config, null_config(cfg, DEFAULT_WEIGHTS), "{name}");
            assert_eq!(
                agent_name(&AgentDesc::Search { config }),
                format!("search-{name}")
            );
        }
        let Ok(AgentDesc::Search { config }) =
            parse_agent_spec("search:null-baseline-strong:max_iterations=9")
        else {
            panic!("baseline null")
        };
        assert_eq!(&config.weights, BASELINE_WEIGHTS);
        assert_eq!(config.max_iterations, 9);
        assert!(parse_agent_spec("search:null-random").is_err());
    }

    /// W-123: every `SearchConfig` field round-trips through the override parser of both the
    /// `ref:` and the `search:` grammar, and nothing else changes; the generic path (the
    /// configuration file format) sets the same values as the field table.
    #[test]
    fn w123_every_config_field_round_trips_through_the_parser() {
        let base = parse_agent_spec("ref:standard").unwrap_or(AgentDesc::Random);
        let AgentDesc::Reference { config: rbase, .. } = base else {
            panic!("ref:standard")
        };
        let sbase = null_config(&rbase, DEFAULT_WEIGHTS);
        let sample = |field: &str| -> &'static str {
            match field {
                "name" => "variant",
                "exploration"
                | "prior_weight"
                | "prior_temperature"
                | "rollout_temperature"
                | "value_scale"
                | "policy_burst_yes_score"
                | "target_extension"
                | "target_extension_z" => "0.375",
                "horizon_turns" => "1",
                f if f.starts_with("noop_")
                    || f.starts_with("policy_")
                    || f.ends_with("_prune")
                    || f.ends_with("_targets")
                    || f.ends_with("_moves")
                    || f.ends_with("_keys") =>
                {
                    "false"
                }
                _ => "7",
            }
        };
        for field in SEARCH_FIELDS {
            let text = format!("{field}={}", sample(field));
            let Ok(AgentDesc::Reference { config: r, .. }) =
                parse_agent_spec(&format!("ref:standard:{text}"))
            else {
                panic!("ref:standard:{text}")
            };
            let Ok(AgentDesc::Search { config: s }) =
                parse_agent_spec(&format!("search:null-standard:{text}"))
            else {
                panic!("search:null-standard:{text}")
            };
            assert_ne!(r, rbase, "{text} changes the reference configuration");
            assert_eq!(s, null_config(&r, DEFAULT_WEIGHTS), "{text}");
            let generic = set_search_field_generic(&sbase, field, sample(field));
            assert_eq!(generic, Ok(Some(s.clone())), "generic {text}");
            let mut table = sbase.clone();
            assert_eq!(
                set_search_field(&mut table, field, sample(field)),
                Some(Ok(()))
            );
            assert_eq!(table, s, "table {text}");
        }
        assert_eq!(set_search_field_generic(&sbase, "nope", "1"), Ok(None));
        assert_eq!(
            set_search_field_generic(&sbase, "weights", "hand"),
            Ok(None)
        );
        assert!(set_search_field_generic(&sbase, "max_iterations", "-1").is_err());
    }

    #[test]
    fn search_overrides_reach_the_mechanism_flags() {
        let Ok(AgentDesc::Search { config }) =
            parse_agent_spec("search:fast:engine_error_leaf=false,portable_math=true")
        else {
            panic!("a search spec")
        };
        assert!(!config.mechanisms.engine_error_leaf && config.mechanisms.portable_math);
        assert!(parse_agent_spec("search:fast:root_trees=4").is_ok());
        assert_eq!(
            parse_agent_spec("search:fast:opponent_buckets=8")
                .err()
                .as_deref(),
            Some("mechanism opponent_buckets (P6-C) is not available in this build")
        );
        assert!(parse_agent_spec("search:fast:engine_error_leaf=2").is_err());
        assert_eq!(
            parse_agent_spec("search:fast:no_such_flag=1")
                .err()
                .as_deref(),
            Some("'fast' has no search setting 'no_such_flag'")
        );
    }
}
