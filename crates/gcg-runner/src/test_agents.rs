//! The two injected test agents of the Python suite (`tests/runner/random_agent.py`), reachable
//! only through `gcg-eval reference-benchmark --agent test-random|test-illegal` and tests
//! (PLAN §6.1): a seeded uniform-random player and a player that always answers an illegal
//! action.

use gcg_ai::{Agent, AgentCx, AgentError, LogAlternative, LogEntry};
use gcg_engine::{Action, GameState};

use crate::{
    AgentFactory, AgentSpec, ILLEGAL_ACTION, TestAgentKind, TestIllegalAgent, TestRandomAgent,
};

impl Agent for TestRandomAgent {
    fn name(&self) -> &str {
        TestAgentKind::Random.as_str()
    }

    /// `SeededRandomAgent.choose` (`tests/runner/random_agent.py:24-41`): one draw per decision
    /// (`options[randrange(len(options))]`, also for a single option), and with logging a dict
    /// without labels whose alternatives are every option with value `0.0` and 0 visits.
    fn choose(
        &mut self,
        st: &GameState,
        player: u8,
        _cx: &mut AgentCx<'_>,
    ) -> Result<Action, AgentError> {
        // `assert st.pending is not None` and `assert st.pending.player == player`.
        let Some(dec) = st.pending() else {
            return Err(AgentError::Assertion(String::new()));
        };
        if dec.hdr.player != player {
            return Err(AgentError::Assertion(String::new()));
        }
        let options = dec.options;
        let action = options[self.rng.randrange_usize(options.len())];
        if self.log_enabled {
            self.log.push(LogEntry {
                turn: st.turn(),
                player,
                decision: dec.hdr.kind,
                chosen: action,
                label: None,
                alternatives: options
                    .iter()
                    .map(|&o| LogAlternative {
                        action: o,
                        label: None,
                        value: 0.0,
                        visits: 0,
                    })
                    .collect(),
            });
        }
        Ok(action)
    }

    /// `decision_log()` (`tests/runner/random_agent.py:43-44`).
    fn decision_log(&self) -> &[LogEntry] {
        &self.log
    }
}

impl Agent for TestIllegalAgent {
    fn name(&self) -> &str {
        TestAgentKind::Illegal.as_str()
    }

    /// `IllegalActionAgent.choose` (`tests/runner/random_agent.py:56-57`).
    fn choose(
        &mut self,
        _st: &GameState,
        _player: u8,
        _cx: &mut AgentCx<'_>,
    ) -> Result<Action, AgentError> {
        Ok(ILLEGAL_ACTION)
    }

    /// `decision_log()` (`tests/runner/random_agent.py:59-60`): always empty.
    fn decision_log(&self) -> &[LogEntry] {
        &[]
    }
}

/// The factory of one test agent kind.
#[derive(Clone, Copy, Debug)]
struct TestFactory(TestAgentKind);

impl AgentFactory for TestFactory {
    /// `make_random_agent(spec)` = `SeededRandomAgent(spec.seed, log=spec.log)`,
    /// `make_illegal_agent(spec)` = `IllegalActionAgent()` (`random_agent.py:47-48`, `:63-64`).
    fn make(&self, spec: &AgentSpec) -> Result<Box<dyn Agent>, AgentError> {
        Ok(match self.0 {
            TestAgentKind::Random => Box::new(TestRandomAgent::new(spec.seed, spec.log)),
            TestAgentKind::Illegal => Box::new(TestIllegalAgent),
        })
    }

    fn describe(&self) -> String {
        self.0.factory_description().to_owned()
    }
}

/// `FactoryRef("…/random_agent.py:make_random_agent")` and `…:make_illegal_agent`
/// (`tests/runner/conftest.py:11`, `tests/runner/test_pool.py:15`).
#[must_use]
pub fn factory(kind: TestAgentKind) -> Box<dyn AgentFactory> {
    Box::new(TestFactory(kind))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Role;

    #[test]
    fn factories_build_the_named_agents() {
        let spec = AgentSpec {
            role: Role::Dut,
            seat: 1,
            preset: "standard",
            seed: 9,
            log: true,
        };
        let random = factory(TestAgentKind::Random);
        assert_eq!(random.describe(), "random_agent.py:make_random_agent");
        let agent = random.make(&spec).expect("random agent");
        assert_eq!(agent.name(), "test-random");
        assert!(agent.decision_log().is_empty());
        let illegal = factory(TestAgentKind::Illegal);
        assert_eq!(illegal.describe(), "random_agent.py:make_illegal_agent");
        assert_eq!(
            illegal.make(&spec).expect("illegal agent").name(),
            "test-illegal"
        );
    }
}
