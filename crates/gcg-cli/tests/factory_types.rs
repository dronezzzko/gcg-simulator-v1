//! The `gcg-sim` agent factory (workstream E, docs/port/contracts.md gcg-cli section) is an
//! `impl gcg_runner::AgentFactory`, and `make` names `gcg_ai::{Agent, AgentError}`. This test
//! does not compile unless `gcg-cli` depends on `gcg-ai` directly: `gcg-runner` and
//! `gcg-search` import those types privately (PLAN §2.2).

use gcg_ai::{Agent, AgentError};
use gcg_runner::{AgentFactory, AgentSpec, Role};
use gcg_search::Preset;

/// Parses the seat's preset as the real factory does and builds no agent.
struct PresetOnly;

impl AgentFactory for PresetOnly {
    fn make(&self, spec: &AgentSpec) -> Result<Box<dyn Agent>, AgentError> {
        let preset: Preset = spec
            .preset
            .parse()
            .map_err(|e: gcg_search::UnknownPreset| AgentError::Value(e.value))?;
        Err(AgentError::Value(preset.as_str().to_owned()))
    }

    fn describe(&self) -> String {
        "preset-only".to_owned()
    }
}

#[test]
fn the_gcg_sim_factory_signature_is_nameable_from_gcg_cli() {
    let spec = |preset| AgentSpec {
        role: Role::Dut,
        seat: 0,
        preset,
        seed: 7,
        log: false,
    };
    let factory: &dyn AgentFactory = &PresetOnly;
    let refused = |preset| factory.make(&spec(preset)).err();
    assert_eq!(refused("fast"), Some(AgentError::Value("fast".to_owned())));
    assert_eq!(
        refused("strong"),
        Some(AgentError::Value("strong".to_owned()))
    );
    assert_eq!(factory.describe(), "preset-only");
}
