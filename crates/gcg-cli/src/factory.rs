//! The `gcg-sim` agent factory (docs/dev/contracts.md, gcg-cli section): every seat of every
//! game gets a fresh strengthened-search agent of the run's preset (PLAN §6.4, A9). It stands
//! where Python's `default_agent_factory` (`runner/agents.py:56-63`) builds the reference
//! search; the reference is not reachable from `gcg-sim` (PLAN §2.2, §6.5).

use gcg_ai::{Agent, AgentError, SearchSettings};
use gcg_runner::{AgentFactory, AgentSpec};
use gcg_search::{Preset, make_agent, preset_config, search_settings};

/// Builds `search-<preset>` agents through [`gcg_search::make_agent`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SearchFactory {
    preset: Preset,
}

impl SearchFactory {
    #[must_use]
    pub const fn new(preset: Preset) -> Self {
        Self { preset }
    }
}

impl AgentFactory for SearchFactory {
    /// `make_agent(preset_config(spec.preset), spec.seed, spec.log)`; role and seat are
    /// ignored, as Python's default factory ignores them. A preset outside
    /// `standard`/`fast` fails as Python's `make_agent` does for an unknown preset
    /// (`ai/agents.py:117-118`).
    fn make(&self, spec: &AgentSpec) -> Result<Box<dyn Agent>, AgentError> {
        let preset = Preset::from_value(spec.preset).ok_or_else(|| {
            let choices: Vec<String> = Preset::ALL
                .iter()
                .map(|p| gcg_data::py_repr(p.as_str()))
                .collect();
            AgentError::Value(format!(
                "unknown AI preset {}; choose from [{}]",
                gcg_data::py_repr(spec.preset),
                choices.join(", ")
            ))
        })?;
        Ok(make_agent(preset_config(preset), spec.seed, spec.log))
    }

    /// `results.json` `ai.factory` (PLAN A9): `gcg-search::make_agent('<preset>')`.
    fn describe(&self) -> String {
        self.preset.factory_description().to_owned()
    }

    /// Results 1.2.0 `ai.search` (PLAN §12.7, O9): the preset's settings.
    fn search_block(&self) -> Option<SearchSettings> {
        Some(search_settings(self.preset, preset_config(self.preset)))
    }
}

#[cfg(test)]
mod tests {
    use gcg_runner::Role;

    use super::*;

    #[test]
    fn unknown_presets_fail_like_python() {
        let spec = AgentSpec {
            role: Role::Bench,
            seat: 1,
            preset: "strong",
            seed: 3,
            log: false,
        };
        let f = SearchFactory::new(Preset::Fast);
        assert_eq!(
            f.make(&spec).err(),
            Some(AgentError::Value(
                "unknown AI preset 'strong'; choose from ['standard', 'fast']".to_owned()
            ))
        );
        assert_eq!(f.describe(), "gcg-search::make_agent('fast')");
        assert_eq!(
            SearchFactory::new(Preset::Standard).describe(),
            "gcg-search::make_agent('standard')"
        );
    }

    #[test]
    fn the_search_block_is_the_presets() {
        for p in Preset::ALL {
            let block = SearchFactory::new(p)
                .search_block()
                .expect("a search block");
            assert_eq!(block, search_settings(p, preset_config(p)));
            assert_eq!((block.preset, block.mapping), (p.as_str(), p.mapping()));
        }
    }
}
