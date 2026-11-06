//! Contract types and data of `gcg-ai-reference` (docs/dev/PLAN.md §6.2, §6.4, §6.5,
//! §10.3.5), owned by the lead (§10.1 S).
//!
//! `ReferenceConfig` is `SearchConfig` (`ai/config.py:13-57`) with Python's field defaults
//! (W-136 keeps defaults in the reference type only). The presets are data: every field of
//! `PRESETS` (`ai/config.py:80-111`), `BASELINE_PRESETS` (`ai/baseline.py:18-73`) and
//! `TUNING_CONFIG` (`ai/tuning.py:50-57`) is spelled out below and checked by the tests against
//! the values printed from the live Python objects and against Python's own
//! construction (`_adopted`, `:60-77`). They exist only in this crate, which `gcg-sim` cannot
//! reach (PLAN §2.2, §6.5).
//!
//! Workstream D2 implements the functions frozen in docs/dev/contracts.md
//! (gcg-ai-reference section); only the lead edits this file (§10.6 change protocol).
//!
//! Python sources are cited relative to `src/gcg_sim/`.

use std::borrow::Cow;
use std::fmt;

use gcg_ai::{AgentCore, PolicyKnobs, TUNED_WEIGHTS, Weights};

// ---------------------------------------------------------------------------------------------
// Constants (ai/config.py:9-10, ai/mcts.py:41, PLAN A9)

/// `WIN_DECAY` (`ai/config.py:9`): a win at turn distance d scores `1.0 - WIN_DECAY * d`.
pub const WIN_DECAY: f64 = 0.01;
/// `LEAF_FLOOR` (`ai/config.py:10`): the lowest evaluated leaf value.
pub const LEAF_FLOOR: f64 = 0.05;
/// `TIE_TOLERANCE` (`ai/mcts.py:41`), used by `_close_call` (`:196`).
pub const TIE_TOLERANCE: f64 = 1e-9;
/// Agent names are `f"mcts-{config.name}"` (`ai/agents.py:81`).
pub const AGENT_PREFIX: &str = "mcts-";
/// `results.json` `ai.factory` of `gcg-eval reference-benchmark` (PLAN A9); Python writes
/// `gcg_sim.ai.make_agent('mcts')` (`runner/agents.py:106-108`), and the normaliser masks the
/// field (A6).
pub const FACTORY_DESCRIPTION: &str = "gcg-ai-reference::make_agent('mcts')";

// ---------------------------------------------------------------------------------------------
// SearchConfig (ai/config.py:13-57)

/// `SearchConfig` (`ai/config.py:13-57`), a frozen dataclass: the 24 fields in Python's
/// declaration order. A decision with `n` distinct root moves gets
/// `clamp(iterations_per_option * n, min_iterations, max_iterations)` iterations
/// ([`ReferenceConfig::budget`]).
///
/// Integer fields hold Python ints that are counts (`u32`) or a turn distance (`i32`). An
/// agent-spec override outside those ranges, which Python accepts and then fails or loops on,
/// is rejected by `gcg-eval`'s parser (F). Overrides (`ai/compare.py:98-112`) build a value
/// with struct update syntax and must call [`ReferenceConfig::check`], as Python's
/// `dataclasses.replace` re-runs `__post_init__`.
#[derive(Clone, PartialEq, Debug)]
pub struct ReferenceConfig {
    /// Seeds the agent's decisions through its name `mcts-{name}` (`ai/agents.py:81`).
    pub name: Cow<'static, str>,
    pub max_iterations: u32,
    pub min_iterations: u32,
    pub iterations_per_option: u32,
    /// UCB `c` (`ai/mcts.py:290`).
    pub exploration: f64,
    /// UCB prior weight `w` (`ai/mcts.py:291`).
    pub prior_weight: f64,
    /// Prior softmax temperature (`ai/mcts.py:314`).
    pub prior_temperature: f64,
    /// `ctx.horizon = st.turn + horizon_turns` (`ai/mcts.py:119`).
    pub horizon_turns: i32,
    /// Rollout softmax temperature (`ai/mcts.py:212`).
    pub rollout_temperature: f64,
    /// Leaf log-odds divisor (`ai/mcts.py:263`).
    pub value_scale: f64,
    /// Rollout move cap (`ai/mcts.py:212`).
    pub max_rollout_decisions: u32,
    /// Play/draw simulations (`ai/agents.py:94`).
    pub first_player_games: u32,
    /// Mulligan samples (`ai/agents.py:90`).
    pub mulligan_samples: u32,
    /// `PolicyKnobs.burst_yes_score` (`ai/policy.py:66`).
    pub policy_burst_yes_score: f64,
    /// `without_effect(branch_choices=)` (`ai/mcts.py:104`).
    pub noop_branch_choices: bool,
    /// `without_effect(resource_aware=)` (`ai/mcts.py:105`).
    pub noop_resource_aware: bool,
    /// `prune_dominated(strict_attacks=)` (`ai/mcts.py:93`).
    pub strict_attack_prune: bool,
    /// `prune_dominated(rest_targets=)` (`ai/mcts.py:94`).
    pub prune_noop_rest_targets: bool,
    /// `merge_equivalent` at the root (`ai/mcts.py:109`).
    pub merge_equivalent_moves: bool,
    /// Child keys by decision kind and the pass handling (`ai/mcts.py:233`, `:268`).
    pub observation_keys: bool,
    /// `_iteration_cap` (`ai/mcts.py:178`).
    pub target_extension: f64,
    /// `_close_call` z (`ai/mcts.py:148`).
    pub target_extension_z: f64,
    /// `PolicyKnobs.rule_polarity_table` (`ai/policy.py:67`).
    pub policy_rule_polarity_table: bool,
    /// `PolicyKnobs.zero_ap_face` (`ai/policy.py:68`).
    pub policy_zero_ap_face: bool,
}

/// `ValueError` of `SearchConfig.__post_init__` (`ai/config.py:48-53`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ConfigError {
    /// `WIN_DECAY * horizon_turns >= LEAF_FLOOR`.
    HorizonTooLong { horizon_turns: i32 },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HorizonTooLong { horizon_turns } => write!(
                f,
                "horizon_turns={horizon_turns} is too long: a win at the horizon would not \
                 outrank every evaluated position (WIN_DECAY * horizon_turns < LEAF_FLOOR)"
            ),
        }
    }
}

impl std::error::Error for ConfigError {}

impl ReferenceConfig {
    /// `SearchConfig(name, max_iterations, min_iterations, iterations_per_option)` with
    /// Python's defaults for the other 20 fields (`ai/config.py:27-46`).
    #[must_use]
    pub const fn new(
        name: &'static str,
        max_iterations: u32,
        min_iterations: u32,
        iterations_per_option: u32,
    ) -> Self {
        Self {
            name: Cow::Borrowed(name),
            max_iterations,
            min_iterations,
            iterations_per_option,
            exploration: 0.3,
            prior_weight: 0.5,
            prior_temperature: 1.5,
            horizon_turns: 2,
            rollout_temperature: 0.5,
            value_scale: 2.0,
            max_rollout_decisions: 250,
            first_player_games: 24,
            mulligan_samples: 96,
            policy_burst_yes_score: 1.0,
            noop_branch_choices: false,
            noop_resource_aware: false,
            strict_attack_prune: false,
            prune_noop_rest_targets: false,
            merge_equivalent_moves: false,
            observation_keys: false,
            target_extension: 1.0,
            target_extension_z: 2.0,
            policy_rule_polarity_table: false,
            policy_zero_ap_face: false,
        }
    }

    /// `__post_init__` (`ai/config.py:48-53`). `WIN_DECAY * horizon_turns` multiplies the
    /// int converted to a float, as Python does; `horizon_turns <= 4` passes.
    ///
    /// # Errors
    /// [`ConfigError::HorizonTooLong`] with Python's message.
    pub const fn check(&self) -> Result<(), ConfigError> {
        if WIN_DECAY * self.horizon_turns as f64 >= LEAF_FLOOR {
            return Err(ConfigError::HorizonTooLong {
                horizon_turns: self.horizon_turns,
            });
        }
        Ok(())
    }

    /// `budget(n_options)` (`ai/config.py:55-57`):
    /// `max(min_iterations, min(max_iterations, iterations_per_option * n_options))` in exact
    /// integer arithmetic (a saturated product is clamped to `max_iterations` exactly as the
    /// unbounded Python product is); `min_iterations` wins when it exceeds `max_iterations`.
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

    /// The agent name `f"mcts-{config.name}"` (`ai/agents.py:81`); the baseline presets reuse
    /// `mcts-standard` and `mcts-strong` (`ai/baseline.py:21`, `:47`).
    #[must_use]
    pub fn agent_name(&self) -> String {
        format!("{AGENT_PREFIX}{}", self.name)
    }
}

// ---------------------------------------------------------------------------------------------
// The presets (ai/config.py:80-111, ai/baseline.py:18-74, ai/tuning.py:50-57)

/// `PRESETS["fast"]` (`ai/config.py:82-92`): `_adopted(SearchConfig(name="fast",
/// max_iterations=64, min_iterations=16, iterations_per_option=7, exploration=0.15,
/// rollout_temperature=0.3, first_player_games=8))`.
pub static FAST: ReferenceConfig = ReferenceConfig {
    name: Cow::Borrowed("fast"),
    max_iterations: 64,
    min_iterations: 16,
    iterations_per_option: 7,
    exploration: 0.15,
    prior_weight: 0.5,
    prior_temperature: 1.5,
    horizon_turns: 2,
    rollout_temperature: 0.3,
    value_scale: 1.0,
    max_rollout_decisions: 250,
    first_player_games: 8,
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
};

/// `PRESETS["standard"]` (`ai/config.py:93-97`): `_adopted(SearchConfig(name="standard",
/// max_iterations=640, min_iterations=160, iterations_per_option=64))`.
pub static STANDARD: ReferenceConfig = ReferenceConfig {
    name: Cow::Borrowed("standard"),
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
};

/// `PRESETS["strong"]` (`ai/config.py:98-109`): `_adopted(SearchConfig(name="strong",
/// max_iterations=1280, min_iterations=320, iterations_per_option=128, exploration=0.15,
/// rollout_temperature=0.3, first_player_games=48, mulligan_samples=192))`.
pub static STRONG: ReferenceConfig = ReferenceConfig {
    name: Cow::Borrowed("strong"),
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
};

/// `PRESETS` (`ai/config.py:80-111`) in dict order: `fast`, `standard`, `strong` (also
/// `sorted(PRESETS)`, the order of `make_agent`'s error message, `ai/agents.py:117-118`).
pub static PRESETS: [(&str, &ReferenceConfig); 3] = [
    ("fast", &FAST),
    ("standard", &STANDARD),
    ("strong", &STRONG),
];

/// `BASELINE_PRESETS["standard"]` (`ai/baseline.py:20-45`): the `standard` preset as released
/// at commit bc38ab4, every field spelled out in Python too. Its agent name is
/// `mcts-standard`.
pub static BASELINE_STANDARD: ReferenceConfig = ReferenceConfig {
    name: Cow::Borrowed("standard"),
    max_iterations: 320,
    min_iterations: 80,
    iterations_per_option: 32,
    exploration: 0.3,
    prior_weight: 0.5,
    prior_temperature: 1.5,
    horizon_turns: 2,
    rollout_temperature: 0.5,
    value_scale: 2.0,
    max_rollout_decisions: 250,
    first_player_games: 24,
    mulligan_samples: 96,
    policy_burst_yes_score: 1.0,
    noop_branch_choices: false,
    noop_resource_aware: false,
    strict_attack_prune: false,
    prune_noop_rest_targets: false,
    merge_equivalent_moves: false,
    observation_keys: false,
    target_extension: 1.0,
    target_extension_z: 2.0,
    policy_rule_polarity_table: false,
    policy_zero_ap_face: false,
};

/// `BASELINE_PRESETS["strong"]` (`ai/baseline.py:46-71`); agent name `mcts-strong`.
pub static BASELINE_STRONG: ReferenceConfig = ReferenceConfig {
    name: Cow::Borrowed("strong"),
    max_iterations: 1280,
    min_iterations: 320,
    iterations_per_option: 128,
    exploration: 0.15,
    prior_weight: 0.5,
    prior_temperature: 1.5,
    horizon_turns: 2,
    rollout_temperature: 0.3,
    value_scale: 2.0,
    max_rollout_decisions: 250,
    first_player_games: 48,
    mulligan_samples: 192,
    policy_burst_yes_score: 1.0,
    noop_branch_choices: false,
    noop_resource_aware: false,
    strict_attack_prune: false,
    prune_noop_rest_targets: false,
    merge_equivalent_moves: false,
    observation_keys: false,
    target_extension: 1.0,
    target_extension_z: 2.0,
    policy_rule_polarity_table: false,
    policy_zero_ap_face: false,
};

/// `BASELINE_PRESETS` (`ai/baseline.py:18-73`) in dict order: `standard`, `strong`. The
/// `gcg-eval` agent specs are `ref:baseline-standard` and `ref:baseline-strong` (PLAN D2;
/// `ai/compare.py:89-94` spells them `baseline-<preset>`).
pub static BASELINE_PRESETS: [(&str, &ReferenceConfig); 2] = [
    ("standard", &BASELINE_STANDARD),
    ("strong", &BASELINE_STRONG),
];

/// `BASELINE_WEIGHTS = TUNED_WEIGHTS` (`ai/baseline.py:74`): the weights every baseline preset
/// plays with (`ai/compare.py:89-94`, `tests/ai/test_baseline.py:27-32`).
pub static BASELINE_WEIGHTS: &Weights = &TUNED_WEIGHTS;

/// `TUNING_CONFIG = SearchConfig(name="tuning", max_iterations=48, min_iterations=16,
/// iterations_per_option=6, first_player_games=8, mulligan_samples=32)` (`ai/tuning.py:50-57`);
/// agent name `mcts-tuning`.
pub static TUNING_CONFIG: ReferenceConfig = ReferenceConfig {
    name: Cow::Borrowed("tuning"),
    max_iterations: 48,
    min_iterations: 16,
    iterations_per_option: 6,
    exploration: 0.3,
    prior_weight: 0.5,
    prior_temperature: 1.5,
    horizon_turns: 2,
    rollout_temperature: 0.5,
    value_scale: 2.0,
    max_rollout_decisions: 250,
    first_player_games: 8,
    mulligan_samples: 32,
    policy_burst_yes_score: 1.0,
    noop_branch_choices: false,
    noop_resource_aware: false,
    strict_attack_prune: false,
    prune_noop_rest_targets: false,
    merge_equivalent_moves: false,
    observation_keys: false,
    target_extension: 1.0,
    target_extension_z: 2.0,
    policy_rule_polarity_table: false,
    policy_zero_ap_face: false,
};

// Every shipped configuration passes `__post_init__`.
const _: () = {
    assert!(FAST.check().is_ok());
    assert!(STANDARD.check().is_ok());
    assert!(STRONG.check().is_ok());
    assert!(BASELINE_STANDARD.check().is_ok());
    assert!(BASELINE_STRONG.check().is_ok());
    assert!(TUNING_CONFIG.check().is_ok());
};

/// `PRESETS[name]` (`ai/config.py:80-111`).
#[must_use]
pub fn reference_preset(name: &str) -> Option<&'static ReferenceConfig> {
    PRESETS.iter().find(|(n, _)| *n == name).map(|(_, c)| *c)
}

/// `BASELINE_PRESETS[name]` (`ai/baseline.py:18-73`); `name` without the `baseline-` prefix.
#[must_use]
pub fn baseline_preset(name: &str) -> Option<&'static ReferenceConfig> {
    BASELINE_PRESETS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, c)| *c)
}

// ---------------------------------------------------------------------------------------------
// The agent (ai/agents.py:69-98)

/// `MctsAgent(seed=0, *, config=PRESETS["standard"], log=False, weights=DEFAULT_WEIGHTS)`
/// (`ai/agents.py:69-98`), named `mcts-{config.name}`. Its `Agent` impl (D2) dispatches as
/// `:85-98`: one option, REDRAW (`mulligan_choice`), CHOOSE_FIRST (`first_player_choice`),
/// otherwise `search`.
#[derive(Clone, Debug)]
#[allow(dead_code, reason = "read by workstream D2's Agent impl")]
pub struct ReferenceAgent {
    pub(crate) core: AgentCore,
    pub(crate) config: ReferenceConfig,
    pub(crate) weights: Weights,
}

impl ReferenceAgent {
    /// `MctsAgent(seed, config=config, log=log, weights=weights)` (`ai/agents.py:73-83`).
    #[must_use]
    pub fn new(config: ReferenceConfig, seed: u64, log: bool, weights: Weights) -> Self {
        Self {
            core: AgentCore::new(config.agent_name(), seed, log),
            config,
            weights,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `_adopted(config)` (`ai/config.py:60-77`), as Python builds the presets.
    fn adopted(config: ReferenceConfig) -> ReferenceConfig {
        ReferenceConfig {
            value_scale: 1.0,
            policy_burst_yes_score: 4.0,
            noop_branch_choices: true,
            noop_resource_aware: true,
            strict_attack_prune: true,
            prune_noop_rest_targets: true,
            merge_equivalent_moves: true,
            observation_keys: true,
            target_extension: 3.0,
            policy_rule_polarity_table: true,
            policy_zero_ap_face: true,
            ..config
        }
    }

    #[test]
    fn presets_follow_pythons_construction() {
        assert_eq!(
            FAST,
            adopted(ReferenceConfig {
                exploration: 0.15,
                rollout_temperature: 0.3,
                first_player_games: 8,
                ..ReferenceConfig::new("fast", 64, 16, 7)
            })
        );
        assert_eq!(
            STANDARD,
            adopted(ReferenceConfig::new("standard", 640, 160, 64))
        );
        assert_eq!(
            STRONG,
            adopted(ReferenceConfig {
                exploration: 0.15,
                rollout_temperature: 0.3,
                first_player_games: 48,
                mulligan_samples: 192,
                ..ReferenceConfig::new("strong", 1280, 320, 128)
            })
        );
        assert_eq!(
            TUNING_CONFIG,
            ReferenceConfig {
                first_player_games: 8,
                mulligan_samples: 32,
                ..ReferenceConfig::new("tuning", 48, 16, 6)
            }
        );
    }

    /// One row per configuration: the integer fields, the float fields as bit patterns and
    /// the boolean fields, in Python's field order, as printed from the live objects
    /// (`dataclasses.fields`, floats via `float.hex()`, Python 3.15.1).
    #[test]
    fn presets_equal_the_live_python_objects() {
        type Row = (&'static str, [u32; 6], i32, [f64; 9], [bool; 9]);
        fn row(c: &ReferenceConfig) -> (String, [u32; 6], i32, [u64; 9], [bool; 9]) {
            (
                c.name.to_string(),
                [
                    c.max_iterations,
                    c.min_iterations,
                    c.iterations_per_option,
                    c.max_rollout_decisions,
                    c.first_player_games,
                    c.mulligan_samples,
                ],
                c.horizon_turns,
                [
                    c.exploration,
                    c.prior_weight,
                    c.prior_temperature,
                    c.rollout_temperature,
                    c.value_scale,
                    c.policy_burst_yes_score,
                    c.target_extension,
                    c.target_extension_z,
                    0.0,
                ]
                .map(f64::to_bits),
                [
                    c.noop_branch_choices,
                    c.noop_resource_aware,
                    c.strict_attack_prune,
                    c.prune_noop_rest_targets,
                    c.merge_equivalent_moves,
                    c.observation_keys,
                    c.policy_rule_polarity_table,
                    c.policy_zero_ap_face,
                    false,
                ],
            )
        }
        // exploration, prior_weight, prior_temperature, rollout_temperature, value_scale,
        // policy_burst_yes_score, target_extension, target_extension_z (then padding).
        let adopted_floats = |exploration: f64, rollout: f64| {
            [exploration, 0.5, 1.5, rollout, 1.0, 4.0, 3.0, 2.0, 0.0]
        };
        let baseline_floats = |exploration: f64, rollout: f64| {
            [exploration, 0.5, 1.5, rollout, 2.0, 1.0, 1.0, 2.0, 0.0]
        };
        let on = [true, true, true, true, true, true, true, true, false];
        let off = [false; 9];
        let expected: [(Row, &ReferenceConfig); 6] = [
            (
                (
                    "fast",
                    [64, 16, 7, 250, 8, 96],
                    2,
                    adopted_floats(0.15, 0.3),
                    on,
                ),
                &FAST,
            ),
            (
                (
                    "standard",
                    [640, 160, 64, 250, 24, 96],
                    2,
                    adopted_floats(0.3, 0.5),
                    on,
                ),
                &STANDARD,
            ),
            (
                (
                    "strong",
                    [1280, 320, 128, 250, 48, 192],
                    2,
                    adopted_floats(0.15, 0.3),
                    on,
                ),
                &STRONG,
            ),
            (
                (
                    "standard",
                    [320, 80, 32, 250, 24, 96],
                    2,
                    baseline_floats(0.3, 0.5),
                    off,
                ),
                &BASELINE_STANDARD,
            ),
            (
                (
                    "strong",
                    [1280, 320, 128, 250, 48, 192],
                    2,
                    baseline_floats(0.15, 0.3),
                    off,
                ),
                &BASELINE_STRONG,
            ),
            (
                (
                    "tuning",
                    [48, 16, 6, 250, 8, 32],
                    2,
                    baseline_floats(0.3, 0.5),
                    off,
                ),
                &TUNING_CONFIG,
            ),
        ];
        for ((name, ints, horizon, floats, flags), config) in expected {
            assert_eq!(
                row(config),
                (
                    name.to_owned(),
                    ints,
                    horizon,
                    floats.map(f64::to_bits),
                    flags
                ),
                "{name}"
            );
        }
        // The decimal literals are the bits Python prints: 0.15 = 0x1.3333333333333p-3,
        // 0.3 = 0x1.3333333333333p-2.
        assert_eq!(FAST.exploration.to_bits(), 0x3FC3_3333_3333_3333);
        assert_eq!(STANDARD.exploration.to_bits(), 0x3FD3_3333_3333_3333);
        assert_eq!(WIN_DECAY.to_bits(), 0x3F84_7AE1_47AE_147B);
        assert_eq!(LEAF_FLOOR.to_bits(), 0x3FA9_9999_9999_999A);
        assert_eq!(TIE_TOLERANCE.to_bits(), 0x3E11_2E0B_E826_D695);
    }

    #[test]
    fn budgets_lookups_and_names_follow_python() {
        // Budgets for n = 1, 2, 3, 5, 10, 20, 30 root moves.
        let budgets = |c: &ReferenceConfig| [1, 2, 3, 5, 10, 20, 30].map(|n| c.budget(n));
        assert_eq!(budgets(&FAST), [16, 16, 21, 35, 64, 64, 64]);
        assert_eq!(budgets(&STANDARD), [160, 160, 192, 320, 640, 640, 640]);
        assert_eq!(budgets(&STRONG), [320, 320, 384, 640, 1280, 1280, 1280]);
        assert_eq!(
            budgets(&BASELINE_STANDARD),
            [80, 80, 96, 160, 320, 320, 320]
        );
        assert_eq!(budgets(&BASELINE_STRONG), budgets(&STRONG));
        assert_eq!(FAST.budget(usize::MAX), 64);
        let inverted = ReferenceConfig::new("x", 10, 50, 3);
        assert_eq!(inverted.budget(100), 50);

        assert_eq!(reference_preset("strong"), Some(&STRONG));
        assert_eq!(reference_preset("baseline-standard"), None);
        assert_eq!(baseline_preset("standard"), Some(&BASELINE_STANDARD));
        assert_eq!(baseline_preset("fast"), None);
        assert!(std::ptr::eq(BASELINE_WEIGHTS, &TUNED_WEIGHTS));

        assert_eq!(STANDARD.agent_name(), "mcts-standard");
        assert_eq!(BASELINE_STANDARD.agent_name(), "mcts-standard");
        assert_eq!(TUNING_CONFIG.agent_name(), "mcts-tuning");
        let agent = ReferenceAgent::new(FAST.clone(), 17, false, TUNED_WEIGHTS.clone());
        assert_eq!(agent.core.name(), "mcts-fast");

        let k = STANDARD.knobs();
        assert_eq!(
            (k.burst_yes_score, k.rule_polarity_table, k.zero_ap_face),
            (4.0, true, true)
        );
        assert_eq!(BASELINE_STANDARD.knobs(), gcg_ai::BASELINE_KNOBS);
    }

    #[test]
    fn horizon_check_matches_post_init() {
        for (h, ok) in [(-3, true), (0, true), (4, true), (5, false), (40, false)] {
            let c = ReferenceConfig {
                horizon_turns: h,
                ..ReferenceConfig::new("x", 1, 1, 1)
            };
            assert_eq!(c.check().is_ok(), ok, "{h}");
        }
        let err = ReferenceConfig {
            horizon_turns: 5,
            ..STANDARD.clone()
        }
        .check()
        .unwrap_err();
        assert_eq!(
            err.to_string(),
            "horizon_turns=5 is too long: a win at the horizon would not outrank every \
             evaluated position (WIN_DECAY * horizon_turns < LEAF_FLOOR)"
        );
    }
}
