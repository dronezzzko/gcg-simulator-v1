//! The one-ply greedy player (`GreedyAgent`, `ai/agents.py:35-66`;
//! docs/port/inventory/08-ai-policy-eval.md §7.2).
//!
//! Each distinct move is applied to one determinisation of the information set, the rest of
//! the current turn is completed by the deterministic playout policy for both players, and
//! the resulting position is scored with the evaluation function. Every move is judged at the
//! same point (the next turn's first decision), so ending the turn is compared fairly with
//! acting. Greedy is a frozen baseline (PLAN §6.3).

use gcg_engine::{Action, ActionKind, ApplyError, DecisionKind, GameState, NO_ARG, apply, observe};

use crate::actions::{canonical, prune_dominated};
use crate::evaluation::{burst_density, evaluate};
use crate::playout::play_until_turn;
use crate::setup::mulligan_choice;
use crate::{
    Agent, AgentCx, AgentError, Alternative, BASELINE_KNOBS, DecisionTrace, GREEDY_MAX_DECISIONS,
    GREEDY_MULLIGAN_SAMPLES, GreedyAgent, LogEntry, PruneFlags, SearchTracer, Weights,
};

impl GreedyAgent {
    /// `GreedyAgent.choose` (`ai/agents.py:48-66`):
    /// - a single option is returned with no draw and no alternatives;
    /// - REDRAW is the mulligan of `mulligan_choice` with 48 samples;
    /// - CHOOSE_FIRST always goes first, with no draw;
    /// - otherwise one `determinize` seeded with one `next_u64()`, `burst_density` on the
    ///   true state, and for each pruned canonical move (default flags) a clone of that one
    ///   world with the move applied, played to the next turn by the argmax policy with
    ///   `BASELINE_KNOBS` (at most 200 moves) and scored by `evaluate`; the first maximal
    ///   value wins.
    fn decide(
        &mut self,
        st: &GameState,
        player: u8,
    ) -> Result<(Action, DecisionTrace), AgentError> {
        let (dec, mut rng) = self.core.begin(st, player)?;
        let seed = rng.state;
        let (chosen, alternatives) = if dec.options.len() == 1 {
            (dec.options[0], Vec::new())
        } else if dec.hdr.kind == DecisionKind::Redraw {
            mulligan_choice(st, player, GREEDY_MULLIGAN_SAMPLES, &mut rng)
        } else if dec.hdr.kind == DecisionKind::ChooseFirst {
            (
                Action::new(ActionKind::GoFirst, i32::from(player), NO_ARG, NO_ARG),
                Vec::new(),
            )
        } else {
            let det =
                observe::determinize(st, player, rng.next_u64()).map_err(AgentError::Engine)?;
            let density = burst_density(st, player);
            let moves = prune_dominated(st, &dec, canonical(st, &dec), PruneFlags::default())
                .map_err(AgentError::Engine)?;
            let mut scored = Vec::with_capacity(moves.len());
            for (_, action) in moves {
                let mut s = det.clone();
                apply(&mut s, action, false).map_err(agent_error)?;
                play_until_turn(
                    &mut s,
                    st.turn() + 1,
                    None,
                    0.0,
                    GREEDY_MAX_DECISIONS,
                    &BASELINE_KNOBS,
                )
                .map_err(AgentError::Engine)?;
                let value =
                    evaluate(&s, player, &self.weights, density).map_err(AgentError::Engine)?;
                scored.push(Alternative {
                    action,
                    value,
                    visits: 1,
                });
            }
            let Some(best) = first_max_value(&scored) else {
                return Err(AgentError::Value("max() iterable argument is empty".into()));
            };
            (scored[best].action, scored)
        };
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

/// `max(range(len(scored)), key=lambda i: scored[i].value)` (`ai/agents.py:65`): the first
/// index of the maximal value.
fn first_max_value(scored: &[Alternative]) -> Option<usize> {
    let mut best: Option<usize> = None;
    for (i, alt) in scored.iter().enumerate() {
        if best.is_none_or(|b| alt.value > scored[b].value) {
            best = Some(i);
        }
    }
    best
}

fn agent_error(e: ApplyError) -> AgentError {
    match e {
        ApplyError::Engine(e) => AgentError::Engine(e),
        ApplyError::Illegal(i) => AgentError::Illegal(i),
    }
}

impl Agent for GreedyAgent {
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

/// `make_agent("greedy", seed=seed, log=log, weights=weights)` (`ai/agents.py:122-123`).
#[must_use]
pub fn make_greedy_agent(seed: u64, log: bool, weights: Weights) -> Box<dyn Agent> {
    Box::new(GreedyAgent::new(seed, log, weights))
}
