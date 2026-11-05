//! The uniformly random player (`RandomAgent`, `ai/agents.py:22-32`;
//! docs/port/inventory/08-ai-policy-eval.md §7.1).

use gcg_engine::{Action, GameState};

use crate::{
    Agent, AgentCx, AgentError, Alternative, DecisionTrace, LogEntry, RandomAgent, SearchTracer,
};

impl RandomAgent {
    /// `RandomAgent.choose` (`ai/agents.py:28-32`): `options[rng.randrange(len(options))]` from
    /// the per-decision generator. There is no single-option shortcut, so one option still
    /// draws. The alternatives are every option with value `1.0 / n` and 0 visits.
    fn decide(
        &mut self,
        st: &GameState,
        player: u8,
    ) -> Result<(Action, DecisionTrace), AgentError> {
        let (dec, mut rng) = self.core.begin(st, player)?;
        let seed = rng.state;
        let n = dec.options.len();
        if n == 0 {
            return Err(AgentError::Value("randrange() requires n > 0".into()));
        }
        let chosen = dec.options[rng.randrange_usize(n)];
        let share = 1.0 / crate::values::int_f64(n as i64);
        let alternatives: Vec<Alternative> = dec
            .options
            .iter()
            .map(|&action| Alternative {
                action,
                value: share,
                visits: 0,
            })
            .collect();
        let chosen = self.core.finish(st, player, chosen, &alternatives);
        Ok((
            chosen,
            DecisionTrace {
                k: self.core.decisions(),
                seed,
                rng_after: rng.state,
                alternatives,
                search: None,
            },
        ))
    }
}

impl Agent for RandomAgent {
    fn name(&self) -> &str {
        self.core.name()
    }

    fn choose(
        &mut self,
        st: &GameState,
        player: u8,
        _cx: &mut AgentCx<'_>,
    ) -> Result<Action, AgentError> {
        self.decide(st, player).map(|(action, _)| action)
    }

    fn choose_traced(
        &mut self,
        st: &GameState,
        player: u8,
        _cx: &mut AgentCx<'_>,
        _tracer: &mut dyn SearchTracer,
    ) -> Result<(Action, Option<DecisionTrace>), AgentError> {
        self.decide(st, player)
            .map(|(action, trace)| (action, Some(trace)))
    }

    fn decision_log(&self) -> &[LogEntry] {
        self.core.entries()
    }
}

/// `make_agent("random", seed=seed, log=log)` (`ai/agents.py:120-121`).
#[must_use]
pub fn make_random_agent(seed: u64, log: bool) -> Box<dyn Agent> {
    Box::new(RandomAgent::new(seed, log))
}
