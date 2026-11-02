//! The strengthened agent behind the `gcg-sim` presets (PLAN §6.1, §12.3). It dispatches as the
//! reference agent (`ai/agents.py:85-98`): a single option (no draw), REDRAW and CHOOSE_FIRST
//! (`Mechanisms::pregame`), otherwise the search. The only `Agent` impl for [`SearchAgent`]
//! (lint §9.9 item 5): the true state goes only to `AgentCore::begin`, `Observation::new` and
//! `AgentCore::finish`, and everything else reads the [`Observation`] (W-135).
//!
//! With `subtree_reuse` (S7, W-11) the agent carries the trees of its last search to its next
//! decision (`SearchAgent.reuse`, `mech/p6d/reuse.rs`); the carry holds information-set keys and
//! trees only, so it keeps the same true-state discipline.

use gcg_core::rng::SplitMix64;
use gcg_engine::{Action, DecisionKind, GameState};

use gcg_ai::{
    Agent, AgentCx, AgentError, Alternative, DecisionTrace, LogEntry, NoTrace, SearchEffort,
    SearchStats, SearchTracer,
};

use crate::mcts::{pregame_with_applies, search_with_carry};
use crate::mech::p6d::reuse::Carry;
use crate::mech::validate;
use crate::{MechanismEffort, Observation, SearchAgent, StrengthenedConfig};

impl SearchAgent {
    /// The dispatch of `MctsAgent.choose` (`ai/agents.py:85-98`) for a decision of `kind` with
    /// more than one option, over the strengthened configuration: REDRAW and CHOOSE_FIRST by
    /// [`crate::pregame`], otherwise the search on `obs` (with `subtree_reuse`, from the agent's
    /// carry). A pre-game decision whose mechanisms simulated (P6-H) reports their engine
    /// applies as a search of no iterations (`applies_aux`, PLAN §12.3).
    fn decide<T: SearchTracer + ?Sized>(
        &mut self,
        kind: DecisionKind,
        rng: &mut SplitMix64,
        obs: &Observation<'_>,
        cx: &mut AgentCx<'_>,
        tracer: &mut T,
    ) -> Result<(Action, Vec<Alternative>, Option<SearchStats>), AgentError> {
        if matches!(kind, DecisionKind::Redraw | DecisionKind::ChooseFirst) {
            self.reuse.clear();
            let ((action, stats), applies) = pregame_with_applies(obs, &self.config, rng, cx)?;
            let search = (applies > 0).then(|| SearchStats {
                iterations: 0,
                budget: 0,
                cap: 0,
                roots: Vec::new(),
                effort: SearchEffort {
                    applies_aux: applies,
                    ..SearchEffort::default()
                },
            });
            return Ok((action, stats, search));
        }
        let carry = self
            .config
            .mechanisms
            .subtree_reuse
            .then_some(&mut self.reuse);
        let (result, effort) = search_with_carry(obs, &self.config, rng, cx, tracer, carry)?;
        self.effort.add(effort);
        Ok((result.action, result.alternatives, Some(result.stats)))
    }

    /// The mechanism counters summed over this agent's decisions.
    #[must_use]
    pub const fn mechanism_effort(&self) -> MechanismEffort {
        self.effort
    }
}

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
        validate(&self.config.mechanisms).map_err(AgentError::Value)?;
        let (dec, mut rng) = self.core.begin(st, player)?;
        let (action, alternatives) = if dec.options.len() == 1 {
            self.reuse.clear();
            (dec.options[0], Vec::new())
        } else {
            let obs = Observation::new(st, player)?;
            let (action, alternatives, _) =
                self.decide(dec.hdr.kind, &mut rng, &obs, cx, &mut NoTrace)?;
            (action, alternatives)
        };
        Ok(self.core.finish(st, player, action, &alternatives))
    }

    fn choose_traced(
        &mut self,
        st: &GameState,
        player: u8,
        cx: &mut AgentCx<'_>,
        tracer: &mut dyn SearchTracer,
    ) -> Result<(Action, Option<DecisionTrace>), AgentError> {
        validate(&self.config.mechanisms).map_err(AgentError::Value)?;
        let (dec, mut rng) = self.core.begin(st, player)?;
        let k = self.core.decisions();
        let seed = rng.state;
        let (action, alternatives, search) = if dec.options.len() == 1 {
            self.reuse.clear();
            (dec.options[0], Vec::new(), None)
        } else {
            let obs = Observation::new(st, player)?;
            self.decide(dec.hdr.kind, &mut rng, &obs, cx, tracer)?
        };
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

/// The strengthened agent `search-{cfg.name}` with its own copy of the configuration, as its
/// concrete type (for its [`SearchAgent::mechanism_effort`]).
#[must_use]
pub fn make_search_agent(cfg: &StrengthenedConfig, seed: u64, log: bool) -> SearchAgent {
    SearchAgent {
        core: gcg_ai::AgentCore::new(cfg.agent_name(), seed, log),
        config: cfg.clone(),
        effort: MechanismEffort::default(),
        reuse: Carry::default(),
    }
}

/// The strengthened agent `search-{cfg.name}` with its own copy of the configuration.
#[must_use]
pub fn make_agent(cfg: &StrengthenedConfig, seed: u64, log: bool) -> Box<dyn Agent> {
    Box::new(make_search_agent(cfg, seed, log))
}
