//! Contract types of `gcg-search` (docs/port/PLAN.md §6.4, §6.5, §10.3.5, §12.3), owned by the
//! lead (§10.1 S).
//!
//! Phase 3 ships the strengthened search as a copy of the reference (D2, "v0"); Phase 6 adds
//! one [`StrengthenedConfig`] field per mechanism (§12.3), so every ablation row is a flag
//! flip. This crate never depends on `gcg-ai-reference` (PLAN §2.2): the two configuration
//! types have no conversion in shipped code (§6.5 item 1), and the null-config conversion of
//! D2's gate lives in this crate's tests, which reach the reference through a dev-dependency.
//!
//! `StrengthenedConfig` has no `Default`, so a preset or an override cannot leave a field
//! unset (W-136):
//!
//! ```compile_fail,E0277
//! let _cfg = gcg_search::StrengthenedConfig { ..Default::default() };
//! ```
//!
//! Workstream D2 implements the functions frozen in docs/port/contracts.md (gcg-search
//! section); only the lead edits this file (§10.6 change protocol).

use std::borrow::Cow;
use std::str::FromStr;

use gcg_ai::{AgentCore, Weights};

/// Agent names are `search-<preset>` (PLAN A9): seed-path material that never collides with
/// the reference's `mcts-*`.
pub const AGENT_PREFIX: &str = "search-";

/// A `gcg-sim --ai-preset` value (BRIEF `<ai_presets>`, PLAN §6.4, §6.5 item 3): exactly
/// `standard` (the default) and `fast`, nothing else, no aliases.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Preset {
    Standard,
    Fast,
}

impl Preset {
    /// The argparse choices, in their order: `(choose from 'standard', 'fast')`.
    pub const ALL: [Self; 2] = [Self::Standard, Self::Fast];
    /// `--ai-preset` default.
    pub const DEFAULT: Self = Self::Standard;

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::Fast => "fast",
        }
    }

    /// Exactly `standard` or `fast` (case-sensitive, as argparse `choices`).
    #[must_use]
    pub fn from_value(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.as_str() == value)
    }

    /// The agent name `search-<preset>` (PLAN A9).
    #[must_use]
    pub const fn agent_name(self) -> &'static str {
        match self {
            Self::Standard => "search-standard",
            Self::Fast => "search-fast",
        }
    }

    /// `results.json` `ai.factory` of `gcg-sim` (PLAN A9).
    #[must_use]
    pub const fn factory_description(self) -> &'static str {
        match self {
            Self::Standard => "gcg-search::make_agent('standard')",
            Self::Fast => "gcg-search::make_agent('fast')",
        }
    }

    /// `results.json` 1.2.0 `ai.search.mapping` (PLAN §12.7): the reference preset the
    /// budgets start from (BRIEF `<ai_presets>`).
    #[must_use]
    pub const fn mapping(self) -> &'static str {
        match self {
            Self::Standard => "reference strong",
            Self::Fast => "reference standard",
        }
    }

    /// The `--ai-preset` help wording of each value (BRIEF `<ai_presets>`, PLAN A11, §6.4).
    #[must_use]
    pub const fn help(self) -> &'static str {
        match self {
            Self::Standard => {
                "uses the full search so that each player plays at the level of the strongest \
                 top-tier competitive player; for refining and polishing the final deck"
            }
            Self::Fast => {
                "trades search depth for throughput; for rapidly screening deck variants, \
                 sub-variants and hypothetical builds"
            }
        }
    }
}

/// A value outside [`Preset::ALL`]; the argparse-style usage error is the CLI's (E).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct UnknownPreset {
    pub value: String,
}

impl FromStr for Preset {
    type Err = UnknownPreset;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_value(s).ok_or_else(|| UnknownPreset {
            value: s.to_owned(),
        })
    }
}

/// The strengthened search's configuration (PLAN §6.5, §10.3.5, §12.3). v0 has exactly the 24
/// `SearchConfig` fields of `gcg_ai_reference::ReferenceConfig` (`ai/config.py:23-46`, same
/// names, types and meaning) plus the evaluation weights, so that a preset is self-contained
/// (S10 refits weights per preset). Phase 6 appends one field per mechanism; all of them off
/// gives the reference behaviour (the null configuration of D2's gate). No `Default`: every
/// preset spells out every field (W-136).
#[derive(Clone, PartialEq, Debug)]
pub struct StrengthenedConfig {
    /// Seeds the agent's decisions through its name `search-{name}` (PLAN A9).
    pub name: Cow<'static, str>,
    pub max_iterations: u32,
    pub min_iterations: u32,
    pub iterations_per_option: u32,
    pub exploration: f64,
    pub prior_weight: f64,
    pub prior_temperature: f64,
    pub horizon_turns: i32,
    pub rollout_temperature: f64,
    pub value_scale: f64,
    pub max_rollout_decisions: u32,
    pub first_player_games: u32,
    pub mulligan_samples: u32,
    pub policy_burst_yes_score: f64,
    pub noop_branch_choices: bool,
    pub noop_resource_aware: bool,
    pub strict_attack_prune: bool,
    pub prune_noop_rest_targets: bool,
    pub merge_equivalent_moves: bool,
    pub observation_keys: bool,
    pub target_extension: f64,
    pub target_extension_z: f64,
    pub policy_rule_polarity_table: bool,
    pub policy_zero_ap_face: bool,
    /// The evaluation weights the search scores leaves with; the reference pairs them
    /// separately (`ai/agents.py:79`).
    pub weights: Weights,
}

/// The strengthened agent (PLAN §6.1, §12.3). Only `agent.rs` (D2) implements `Agent` for it
/// (lint §9.9 item 5); from Phase 6 its `choose` passes the search an `Observation` only.
#[derive(Clone, Debug)]
#[allow(dead_code, reason = "read by workstream D2's agent.rs")]
pub struct SearchAgent {
    pub(crate) core: AgentCore,
    pub(crate) config: StrengthenedConfig,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_are_exactly_standard_and_fast() {
        assert_eq!(Preset::ALL.map(Preset::as_str), ["standard", "fast"]);
        assert_eq!("fast".parse::<Preset>(), Ok(Preset::Fast));
        for bad in ["strong", "Standard", "fast ", "", "reference"] {
            assert_eq!(
                bad.parse::<Preset>(),
                Err(UnknownPreset {
                    value: bad.to_owned()
                })
            );
        }
        assert_eq!(Preset::DEFAULT, Preset::Standard);
        // The brief's wording (docs/port/BRIEF.md:36-37) without the final period, one line.
        let standard = "uses the full search so that each player plays at the level of the strongest top-tier competitive player; for refining and polishing the final deck";
        let fast = "trades search depth for throughput; for rapidly screening deck variants, sub-variants and hypothetical builds";
        assert_eq!(Preset::ALL.map(Preset::help), [standard, fast]);
        assert!(Preset::ALL.iter().all(|p| !p.help().contains("  ")));
        for p in Preset::ALL {
            assert_eq!(p.agent_name(), format!("{AGENT_PREFIX}{}", p.as_str()));
            assert!(!p.agent_name().contains("mcts-"));
            assert_eq!(
                p.factory_description(),
                format!("gcg-search::make_agent('{}')", p.as_str())
            );
        }
    }
}
