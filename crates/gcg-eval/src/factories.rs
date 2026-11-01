//! The agent factories of the benchmark commands (`runner/agents.py:56-63`, `:106-114`).
//!
//! - [`ReferenceFactory`]: `gcg-eval reference-benchmark`, Python's `default_agent_factory`:
//!   `make_agent("mcts", preset=spec.preset, seed=spec.seed, log=spec.log)`, the reference
//!   preset with `DEFAULT_WEIGHTS`; `ai.factory` is `gcg_ai_reference::FACTORY_DESCRIPTION`
//!   (PLAN A9).
//! - [`SearchConfigFactory`]: `gcg-eval preset-benchmark --search-config FILE` (PLAN §12.8),
//!   the strengthened search with the file's configuration for both seats.
//! - The test agents of `--agent test-random|test-illegal` are `gcg_runner::test_agents`.

use gcg_ai::{Agent, AgentError, DEFAULT_WEIGHTS};
use gcg_ai_reference::{FACTORY_DESCRIPTION, PRESETS, make_reference_agent};
use gcg_runner::{AgentFactory, AgentSpec};
use gcg_search::StrengthenedConfig;

/// The reference search at the spec's preset (Python's `default_agent_factory`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct ReferenceFactory;

/// `f"unknown AI preset {preset!r}; choose from {sorted(PRESETS)}"` (`ai/agents.py:117-118`).
#[must_use]
pub fn unknown_preset(preset: &str) -> String {
    let mut names: Vec<&str> = PRESETS.iter().map(|(n, _)| *n).collect();
    names.sort();
    let shown: Vec<String> = names.iter().map(|n| gcg_data::py_repr(n)).collect();
    format!(
        "unknown AI preset {}; choose from [{}]",
        gcg_data::py_repr(preset),
        shown.join(", ")
    )
}

impl AgentFactory for ReferenceFactory {
    fn make(&self, spec: &AgentSpec) -> Result<Box<dyn Agent>, AgentError> {
        let cfg = gcg_ai_reference::reference_preset(spec.preset)
            .ok_or_else(|| AgentError::Value(unknown_preset(spec.preset)))?;
        Ok(make_reference_agent(
            cfg,
            spec.seed,
            spec.log,
            DEFAULT_WEIGHTS,
        ))
    }

    fn describe(&self) -> String {
        FACTORY_DESCRIPTION.to_owned()
    }
}

/// The strengthened search with one configuration for every seat.
#[derive(Clone, PartialEq, Debug)]
pub struct SearchConfigFactory {
    pub config: StrengthenedConfig,
}

impl AgentFactory for SearchConfigFactory {
    fn make(&self, spec: &AgentSpec) -> Result<Box<dyn Agent>, AgentError> {
        Ok(gcg_search::make_agent(&self.config, spec.seed, spec.log))
    }

    /// `gcg-search::make_agent('<config name>')`, the shape of
    /// `gcg_search::Preset::factory_description`.
    fn describe(&self) -> String {
        format!(
            "gcg-search::make_agent({})",
            gcg_data::py_repr(&self.config.name)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptions_and_errors() {
        assert_eq!(
            ReferenceFactory.describe(),
            "gcg-ai-reference::make_agent('mcts')"
        );
        assert_eq!(
            unknown_preset("x"),
            "unknown AI preset 'x'; choose from ['fast', 'standard', 'strong']"
        );
    }
}
