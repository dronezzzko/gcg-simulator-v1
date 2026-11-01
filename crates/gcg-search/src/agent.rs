//! The strengthened agent behind the `gcg-sim` presets (PLAN §6.1, §12.3). v0 dispatches as the
//! reference agent (`ai/agents.py:85-98`): a single option (no draw), REDRAW, CHOOSE_FIRST,
//! otherwise the search. The only `Agent` impl for [`SearchAgent`] (lint §9.9 item 5).

use gcg_core::rng::SplitMix64;
use gcg_engine::{Action, DecisionKind, DecisionView, GameState};

use gcg_ai::{
    Agent, AgentCx, AgentError, Alternative, DecisionTrace, LogEntry, NoTrace, SearchStats,
    SearchTracer, first_player_choice, mulligan_choice,
};

use crate::mcts::search;
use crate::{Observation, SearchAgent, StrengthenedConfig};

impl SearchAgent {
    /// The dispatch of `MctsAgent.choose` (`ai/agents.py:85-98`) over the strengthened
    /// configuration, between `AgentCore::begin` and `finish`: a single option (no draw),
    /// REDRAW, CHOOSE_FIRST, otherwise the search on `obs`. Both `Agent` methods run it.
    fn decide<T: SearchTracer + ?Sized>(
        &self,
        dec: &DecisionView<'_>,
        rng: &mut SplitMix64,
        obs: &Observation<'_>,
        cx: &mut AgentCx<'_>,
        tracer: &mut T,
    ) -> Result<(Action, Vec<Alternative>, Option<SearchStats>), AgentError> {
        Ok(if dec.options.len() == 1 {
            (dec.options[0], Vec::new(), None)
        } else if dec.hdr.kind == DecisionKind::Redraw {
            let (action, stats) =
                mulligan_choice(obs.state(), obs.player(), self.config.mulligan_samples, rng);
            (action, stats, None)
        } else if dec.hdr.kind == DecisionKind::ChooseFirst {
            let (action, stats) = first_player_choice(
                obs.state(),
                obs.player(),
                self.config.first_player_games,
                rng,
                &self.config.knobs(),
                cx,
            )?;
            (action, stats, None)
        } else {
            let result = search(obs, &self.config, rng, cx, tracer)?;
            (result.action, result.alternatives, Some(result.stats))
        })
    }
}

/// The adapter of PLAN §12.3: the true state goes only to `AgentCore::begin`,
/// `Observation::new` and `AgentCore::finish` (lint §9.9 item 5).
impl Agent for SearchAgent {
    fn name(&self) -> &str {
        self.core.name()
    }

    fn choose(
        &mut self,
        st: &GameState,
        player: u8,
        cx: &mut AgentCx<'_>,
    ) -> Result<Action, AgentError> {
        let (dec, mut rng) = self.core.begin(st, player)?;
        let obs = Observation::new(st, player);
        let (action, alternatives, _) = self.decide(&dec, &mut rng, &obs, cx, &mut NoTrace)?;
        Ok(self.core.finish(st, player, action, &alternatives))
    }

    fn choose_traced(
        &mut self,
        st: &GameState,
        player: u8,
        cx: &mut AgentCx<'_>,
        tracer: &mut dyn SearchTracer,
    ) -> Result<(Action, Option<DecisionTrace>), AgentError> {
        let (dec, mut rng) = self.core.begin(st, player)?;
        let k = self.core.decisions();
        let seed = rng.state;
        let obs = Observation::new(st, player);
        let (action, alternatives, search) = self.decide(&dec, &mut rng, &obs, cx, tracer)?;
        let action = self.core.finish(st, player, action, &alternatives);
        Ok((
            action,
            Some(DecisionTrace {
                k,
                seed,
                rng_after: rng.state,
                alternatives,
                search,
            }),
        ))
    }

    fn decision_log(&self) -> &[LogEntry] {
        self.core.entries()
    }
}

/// The strengthened agent `search-{cfg.name}` with its own copy of the configuration.
#[must_use]
pub fn make_agent(cfg: &StrengthenedConfig, seed: u64, log: bool) -> Box<dyn Agent> {
    Box::new(SearchAgent {
        core: gcg_ai::AgentCore::new(cfg.agent_name(), seed, log),
        config: cfg.clone(),
    })
}
