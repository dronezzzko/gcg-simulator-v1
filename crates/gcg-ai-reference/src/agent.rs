//! The reference MCTS agent (`MctsAgent`, `ai/agents.py:69-98`) and the factories
//! `make_agent` (`ai/agents.py:101-127`) and `make_reference_agent`.

use gcg_engine::{Action, DecisionKind, GameState};

use gcg_ai::{
    Agent, AgentCx, AgentError, DEFAULT_WEIGHTS, DecisionTrace, LogEntry, NoTrace, SearchTracer,
    Weights, first_player_choice, make_greedy_agent, make_random_agent, mulligan_choice,
};

use crate::mcts::search;
use crate::{PRESETS, ReferenceAgent, ReferenceConfig, reference_preset};

/// `AGENT_KINDS` (`ai/agents.py:101`).
pub const AGENT_KINDS: [&str; 3] = ["random", "greedy", "mcts"];

impl ReferenceAgent {
    /// `MctsAgent.choose` (`ai/agents.py:85-98`), dispatching: a single option
    /// (no draw), REDRAW (`mulligan_choice`), CHOOSE_FIRST (`first_player_choice`), otherwise
    /// `search`. Both `Agent` methods run this one function, so their choices, draws and log
    /// entries are the same.
    fn decide<T: SearchTracer + ?Sized>(
        &mut self,
        st: &GameState,
        player: u8,
        cx: &mut AgentCx<'_>,
        tracer: &mut T,
    ) -> Result<(Action, DecisionTrace), AgentError> {
        let (dec, mut rng) = self.core.begin(st, player)?;
        let k = self.core.decisions();
        let seed = rng.state;
        let (action, alternatives, stats) = if dec.options.len() == 1 {
            (dec.options[0], Vec::new(), None)
        } else if dec.hdr.kind == DecisionKind::Redraw {
            let (action, stats) =
                mulligan_choice(st, player, self.config.mulligan_samples, &mut rng);
            (action, stats, None)
        } else if dec.hdr.kind == DecisionKind::ChooseFirst {
            let (action, stats) = first_player_choice(
                st,
                player,
                self.config.first_player_games,
                &mut rng,
                &self.config.knobs(),
                cx,
            )?;
            (action, stats, None)
        } else {
            let result = search(
                st,
                player,
                &self.config,
                &self.weights,
                &mut rng,
                cx,
                tracer,
            )?;
            (result.action, result.alternatives, Some(result.stats))
        };
        let action = self.core.finish(st, player, action, &alternatives);
        Ok((
            action,
            DecisionTrace {
                k,
                seed,
                rng_after: rng.state,
                alternatives,
                search: stats,
            },
        ))
    }
}

impl Agent for ReferenceAgent {
    fn name(&self) -> &str {
        self.core.name()
    }

    fn choose(
        &mut self,
        st: &GameState,
        player: u8,
        cx: &mut AgentCx<'_>,
    ) -> Result<Action, AgentError> {
        self.decide(st, player, cx, &mut NoTrace).map(|(a, _)| a)
    }

    fn choose_traced(
        &mut self,
        st: &GameState,
        player: u8,
        cx: &mut AgentCx<'_>,
        tracer: &mut dyn SearchTracer,
    ) -> Result<(Action, Option<DecisionTrace>), AgentError> {
        self.decide(st, player, cx, tracer)
            .map(|(a, trace)| (a, Some(trace)))
    }

    fn decision_log(&self) -> &[LogEntry] {
        self.core.entries()
    }
}

/// `make_agent("mcts", config=cfg, seed=seed, log=log, weights=w)` (`ai/agents.py:124-126`):
/// the reference agent `mcts-{cfg.name}` with its own copies of the configuration and weights.
#[must_use]
pub fn make_reference_agent(
    cfg: &ReferenceConfig,
    seed: u64,
    log: bool,
    w: &Weights,
) -> Box<dyn Agent> {
    Box::new(ReferenceAgent::new(cfg.clone(), seed, log, w.clone()))
}

/// `make_agent(kind, preset=, seed=, log=, config=, weights=)` (`ai/agents.py:104-127`): the
/// preset is checked first for every kind, `weights` defaults to `DEFAULT_WEIGHTS`, and
/// `config` replaces the preset's search settings. This is the factory behind `results.json`'s
/// `gcg-ai-reference::make_agent('mcts')` (PLAN A9).
///
/// # Errors
/// [`AgentError::Value`] with Python's messages for an unknown preset or agent kind.
pub fn make_agent(
    kind: &str,
    preset: &str,
    seed: u64,
    log: bool,
    config: Option<&ReferenceConfig>,
    weights: Option<&Weights>,
) -> Result<Box<dyn Agent>, AgentError> {
    let Some(preset_config) = reference_preset(preset) else {
        let names: Vec<String> = PRESETS.iter().map(|(n, _)| format!("'{n}'")).collect();
        return Err(AgentError::Value(format!(
            "unknown AI preset {}; choose from [{}]",
            gcg_data::py_repr(preset),
            names.join(", ")
        )));
    };
    let chosen_weights = weights.unwrap_or(DEFAULT_WEIGHTS);
    match kind {
        "random" => Ok(make_random_agent(seed, log)),
        "greedy" => Ok(make_greedy_agent(seed, log, chosen_weights.clone())),
        "mcts" => Ok(make_reference_agent(
            config.unwrap_or(preset_config),
            seed,
            log,
            chosen_weights,
        )),
        _ => {
            let names: Vec<String> = AGENT_KINDS.iter().map(|n| format!("'{n}'")).collect();
            Err(AgentError::Value(format!(
                "unknown agent kind {}; choose from [{}]",
                gcg_data::py_repr(kind),
                names.join(", ")
            )))
        }
    }
}
