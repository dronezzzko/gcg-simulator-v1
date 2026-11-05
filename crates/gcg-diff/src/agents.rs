//! The agent oracle of the real-engine replay (PLAN §8.7 "Search and baselines traces",
//! drill-down levels 1-3; docs/port/contracts.md, gcg-diff section "Agent replay").
//!
//! [`RustAgents`] builds one Rust agent per seat from the trace header, with `log = true` and
//! the seat's `agent_seeds` entry, and at every decision with an agent block runs
//! `Agent::choose_traced` on the engine's current state (before `apply`), through
//! `Pool::run_inline`. The constructor follows the generator (`port/oracle/traces.py`): the
//! agent names `random` and `greedy` (`DEFAULT_WEIGHTS`), and `mcts-<preset>` with
//! `reference_preset(preset)` and `DEFAULT_WEIGHTS`, or with `baseline_preset(..)` and
//! `BASELINE_WEIGHTS` for a `baseline-*` preset (`tests/ai/test_baseline.py:27-33`).
//!
//! At a decision whose trace holds `iter` or `apply_trace` records, the search runs with a
//! [`Recorder`], whose records the replay compares with the trace's; elsewhere with
//! [`NoTrace`].

use gcg_ai::{
    Agent, AgentCx, Alternative, DEFAULT_WEIGHTS, DecisionTrace, IterTrace, NoTrace, Scratch,
    SearchTracer,
};
use gcg_core::pool::{Cancel, Pool, TaskCx};
use gcg_engine::{Action, GameState};

use crate::engine::{digest16, json_to_value};
use crate::replay::{AgentObservation, AgentOracle};
use crate::{
    AgentRec, AltRec, ApplyTraceRec, DecisionRec, EngineUnderTest, Header, IterRec, Record, Trace,
};

/// The level-2 and level-3 recorder of one decision (`SearchTracer`): every iteration with the
/// digest prefix of its world, and with `apply_level` every engine apply with the digest
/// prefix after it.
#[derive(Clone, Debug, Default)]
pub struct Recorder {
    pub decision: u32,
    pub apply_level: bool,
    /// With `Some(n)`, only iteration `n` and its applies (`traces.py --iteration N`).
    pub only: Option<u32>,
    pub iters: Vec<IterRec>,
    pub applies: Vec<ApplyTraceRec>,
}

impl Recorder {
    #[must_use]
    pub fn new(decision: u32, apply_level: bool) -> Self {
        Self {
            decision,
            apply_level,
            ..Self::default()
        }
    }

    fn wanted(&self, n: u32) -> bool {
        self.only.is_none_or(|k| k == n)
    }
}

impl SearchTracer for Recorder {
    fn iteration(&mut self, world: &GameState, it: &IterTrace) {
        if self.wanted(it.n) {
            self.iters.push(IterRec {
                decision: self.decision,
                iteration: *it,
                world16: digest16(world),
            });
        }
    }

    fn apply(&mut self, n: u32, j: u32, action: Action, after: &GameState) {
        if self.apply_level && self.wanted(n) {
            self.applies.push(ApplyTraceRec {
                decision: self.decision,
                n,
                j,
                action,
                after16: digest16(after),
            });
        }
    }
}

/// The Rust agent of a seat, as the trace generator built the Python one.
///
/// # Errors
/// An agent name or preset without a Rust agent.
pub fn seat_agent(header: &Header, seat: usize) -> Result<Box<dyn Agent>, String> {
    let name = header
        .agents
        .get(seat)
        .ok_or_else(|| format!("the header names no agent for seat {seat}"))?;
    let seed = *header
        .agent_seeds
        .get(seat)
        .ok_or_else(|| format!("the header has no agent seed for seat {seat}"))?;
    match name.as_str() {
        "random" => Ok(gcg_ai::make_random_agent(seed, true)),
        "greedy" => Ok(gcg_ai::make_greedy_agent(
            seed,
            true,
            (*DEFAULT_WEIGHTS).clone(),
        )),
        n if n.starts_with(gcg_ai_reference::AGENT_PREFIX) => {
            let preset = header
                .preset
                .as_deref()
                .ok_or_else(|| format!("agent {n} without a preset in the header"))?;
            let (cfg, weights) = match preset.strip_prefix("baseline-") {
                Some(p) => (
                    gcg_ai_reference::baseline_preset(p),
                    gcg_ai_reference::BASELINE_WEIGHTS,
                ),
                None => (gcg_ai_reference::reference_preset(preset), DEFAULT_WEIGHTS),
            };
            let cfg = cfg.ok_or_else(|| format!("no reference preset {preset:?}"))?;
            if cfg.agent_name() != n {
                return Err(format!(
                    "agent {n} does not match preset {preset:?} ({})",
                    cfg.agent_name()
                ));
            }
            Ok(gcg_ai_reference::make_reference_agent(
                cfg, seed, true, weights,
            ))
        }
        other => Err(format!("no Rust agent replays the Python agent {other:?}")),
    }
}

/// `Agent::choose_traced` on `st` with a context from `Pool::run_inline`.
///
/// # Errors
/// What the agent raised, as `Class: message`.
pub fn run_agent(
    agent: &mut dyn Agent,
    st: &GameState,
    player: u8,
    tracer: &mut dyn SearchTracer,
) -> Result<(Action, Option<DecisionTrace>), String> {
    let cancel = Cancel::new();
    Pool::run_inline(|scratch: &mut Scratch, par: &TaskCx<'_, Scratch>| {
        let mut cx = AgentCx {
            scratch,
            par,
            cancel: &cancel,
        };
        agent.choose_traced(st, player, &mut cx, tracer)
    })
    .map_err(|e| format!("{}: {e:?}", e.py_class()))
}

/// A [`DecisionTrace`] and the seat's new log entries as an [`AgentRec`].
#[must_use]
pub fn agent_rec(trace: DecisionTrace, new_log: &[gcg_ai::LogEntry]) -> AgentRec {
    let alt = |a: &Alternative| AltRec {
        action: a.action,
        value: a.value,
        visits: a.visits,
    };
    let (iterations, budget, cap, roots) = match trace.search {
        Some(s) => (
            Some(s.iterations),
            Some(s.budget),
            Some(s.cap),
            Some(s.roots),
        ),
        None => (None, None, None, None),
    };
    AgentRec {
        k: trace.k,
        seed: trace.seed,
        rng_after: trace.rng_after,
        iterations,
        budget,
        cap,
        roots,
        alternatives: trace.alternatives.iter().map(alt).collect(),
        log: new_log
            .iter()
            .map(|e| json_to_value(&e.to_json()))
            .collect(),
    }
}

/// The Rust agents of one trace.
pub struct RustAgents {
    seats: Option<Vec<Box<dyn Agent>>>,
    /// The decisions whose trace holds level-2 records, and whether also level-3 ones.
    deep: Vec<(u32, bool)>,
}

impl Default for RustAgents {
    fn default() -> Self {
        Self::new()
    }
}

impl RustAgents {
    /// Agents for a trace without drill-down records.
    #[must_use]
    pub fn new() -> Self {
        Self {
            seats: None,
            deep: Vec::new(),
        }
    }

    /// Agents for `t`, recording levels 2 and 3 at the decisions whose records `t` holds.
    #[must_use]
    pub fn for_trace(t: &Trace) -> Self {
        let mut deep: Vec<(u32, bool)> = Vec::new();
        for r in &t.records {
            let (d, apply) = match r {
                Record::Iter(it) => (it.decision, false),
                Record::ApplyTrace(a) => (a.decision, true),
                _ => continue,
            };
            match deep.iter_mut().find(|(k, _)| *k == d) {
                Some(entry) => entry.1 |= apply,
                None => deep.push((d, apply)),
            }
        }
        Self { seats: None, deep }
    }
}

impl AgentOracle for RustAgents {
    fn decide(
        &mut self,
        header: &Header,
        rec: &DecisionRec,
        engine: &dyn EngineUnderTest,
    ) -> Result<Option<AgentObservation>, String> {
        let Some(st) = engine.state() else {
            return Ok(None);
        };
        if self.seats.is_none() {
            let seats = (0..header.agents.len())
                .map(|s| seat_agent(header, s))
                .collect::<Result<Vec<_>, _>>()?;
            self.seats = Some(seats);
        }
        let agent = self
            .seats
            .as_mut()
            .and_then(|s| s.get_mut(usize::from(rec.player)))
            .ok_or_else(|| format!("no agent for player {}", rec.player))?;
        let before = agent.decision_log().len();
        let deep = self.deep.iter().find(|(k, _)| *k == rec.index).copied();
        let (action, trace, iters, applies) = match deep {
            Some((_, apply_level)) => {
                let mut rec_t = Recorder::new(rec.index, apply_level);
                let (a, t) = run_agent(agent.as_mut(), st, rec.player, &mut rec_t)?;
                (a, t, rec_t.iters, rec_t.applies)
            }
            None => {
                let (a, t) = run_agent(agent.as_mut(), st, rec.player, &mut NoTrace)?;
                (a, t, Vec::new(), Vec::new())
            }
        };
        let trace = trace.ok_or_else(|| {
            format!(
                "{} returned no decision trace (choose_traced is not overridden)",
                agent.name()
            )
        })?;
        let log = agent.decision_log().get(before..).unwrap_or_default();
        Ok(Some(AgentObservation {
            action,
            block: agent_rec(trace, log),
            iters,
            apply_traces: applies,
        }))
    }
}
