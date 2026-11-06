//! Per-decision traces of `gcg-eval` (docs/dev/PLAN.md §6.6, §12.2 P6-I; W-21, W-22): a file
//! of its own, never `games.ndjson`, a replay or the decision log, which stay Python's.
//!
//! One NDJSON line per agent decision with what the agent reported through
//! `Agent::choose_traced` (whose action, draws, counter and log equal `choose`'s): the turn,
//! the player, the decision kind, the number of options, the chosen action
//! (`[kind, a, b, c]`), the agent's decision counter `k` and per-decision seed, every
//! alternative unranked and unrounded (`[action, value, visits]`; for a search, every root
//! move with its visits and mean), and for a search the iterations used, the budget and the
//! iteration cap, the root-move count, `extended` (iterations past the budget, the target
//! extension of `ai/mcts.py:172-178`), `early_stop` (iterations below the cap) and the effort
//! counts (applies by purpose, worlds, capped rollouts). For greedy it adds
//! `greedy_cap_hits`: how many of the moves greedy scored were cut by its 200-decision
//! turn completion and scored mid-turn ([`greedy_cap_hits`], W-22). For a search it adds
//! `rest_prune`: which branch of the reference's Rest-target prune decides the decision
//! (`"pruned"`, or the exception that keeps the moves, `"rest_trigger"` or `"while_rested"`),
//! `null` where the prune does not apply ([`rest_prune_branch`], W-13). It replays the prune's
//! own computation at the decision, like `greedy_cap_hits`, so the agents' decisions do not
//! change. It is the prune's branch at that position: a configuration with
//! `prune_noop_rest_targets` off (the `baseline-*` reference presets) does not run the prune.
//!
//! Lines are ordered by game, then seat, then decision counter, so the file is the same for
//! any worker count. [`TracingFactory`] adds the traces to the benchmark pipeline without
//! changing a byte of its reports.

use std::sync::{Arc, Mutex};

use gcg_ai::{
    Agent, AgentCx, AgentEffort, AgentError, BASELINE_KNOBS, DecisionTrace, GREEDY_MAX_DECISIONS,
    LogEntry, NoTrace, PruneFlags, RestPrune, SearchSettings, SearchTracer, canonical,
    play_until_turn, prune_dominated, rest_prune,
};
use gcg_core::SplitMix64;
use gcg_core::pyjson::{DumpOptions, Json, dumps};
use gcg_engine::{Action, DecisionKind, EngineError, GameState, apply, observe};
use gcg_runner::{AgentFactory, AgentSpec};

/// `[kind, a, b, c]`, as `games.ndjson` writes actions.
#[must_use]
pub fn action_json(a: &Action) -> Json {
    Json::Array(vec![
        a.kind.as_str().into(),
        a.a.into(),
        a.b.into(),
        a.c.into(),
    ])
}

/// How many of the moves greedy scores at `st` its turn completion cuts at `cap` decisions:
/// greedy's own computation (`crate::gcg_ai` greedy, `ai/agents.py:57-66`) replayed from its
/// per-decision `seed` (one `next_u64()` seeds the determinisation; the pruned canonical
/// moves; each completed to the next turn by the argmax policy). A line is cut when the
/// completion made `cap` moves and the game is still on and on the same turn. `None` for the
/// decisions greedy does not score (a single option, REDRAW, CHOOSE_FIRST).
///
/// # Errors
/// An engine error of the replayed computation.
pub fn greedy_cap_hits(
    st: &GameState,
    player: u8,
    seed: u64,
    cap: u32,
) -> Result<Option<u32>, EngineError> {
    let Some(dec) = st.pending() else {
        return Ok(None);
    };
    if dec.options.len() == 1
        || dec.hdr.kind == DecisionKind::Redraw
        || dec.hdr.kind == DecisionKind::ChooseFirst
    {
        return Ok(None);
    }
    let mut rng = SplitMix64::new(seed);
    let det = observe::determinize(st, player, rng.next_u64())?;
    let moves = prune_dominated(st, &dec, canonical(st, &dec), PruneFlags::default())?;
    let mut hits = 0;
    for (_, action) in moves {
        let mut s = det.clone();
        apply(&mut s, action, false).map_err(|e| match e {
            gcg_engine::ApplyError::Engine(e) => e,
            gcg_engine::ApplyError::Illegal(i) => EngineError::Value(i.to_string()),
        })?;
        let n = play_until_turn(&mut s, st.turn() + 1, None, 0.0, cap, &BASELINE_KNOBS)?;
        if n == cap && s.winner().is_none() && s.turn() < st.turn() + 1 && s.pending().is_some() {
            hits += 1;
        }
    }
    Ok(Some(hits))
}

/// The branch of the reference's Rest-target prune (`gcg_ai::rest_prune`) at `st`'s pending
/// decision over its canonical moves, the moves the search's root prune receives
/// (`ai/mcts.py:89-94`): `"pruned"`, `"rest_trigger"` or `"while_rested"` (the W-13 exception),
/// or `None` where the prune does not apply or would change nothing.
///
/// # Errors
/// An engine error of the replayed prune.
pub fn rest_prune_branch(st: &GameState) -> Result<Option<&'static str>, EngineError> {
    let Some(dec) = st.pending() else {
        return Ok(None);
    };
    Ok(
        rest_prune(st, &dec, &canonical(st, &dec))?.map(|b| match b {
            RestPrune::Pruned => "pruned",
            RestPrune::RestTrigger => "rest_trigger",
            RestPrune::WhileRested => "while_rested",
        }),
    )
}

/// One trace line without its game keys.
///
/// # Errors
/// An engine error of the greedy cap count or of the Rest-target prune's replay.
pub fn decision_json(
    st: &GameState,
    player: u8,
    agent: &str,
    action: Action,
    trace: Option<&DecisionTrace>,
) -> Result<Json, EngineError> {
    let mut o = Json::object();
    o.set("turn", st.turn());
    o.set("player", player);
    let (kind, options) = st
        .pending()
        .map_or(("", 0), |d| (d.hdr.kind.as_str(), d.options.len()));
    o.set("kind", kind);
    o.set("options", options);
    o.set("chosen", action_json(&action));
    o.set("agent", agent);
    match trace {
        None => {
            o.set("k", Json::Null);
            o.set("seed", Json::Null);
            o.set("alternatives", Json::Null);
            o.set("search", Json::Null);
        }
        Some(t) => {
            o.set("k", t.k);
            o.set("seed", t.seed);
            o.set(
                "alternatives",
                Json::Array(
                    t.alternatives
                        .iter()
                        .map(|a| {
                            Json::Array(vec![
                                action_json(&a.action),
                                a.value.into(),
                                a.visits.into(),
                            ])
                        })
                        .collect(),
                ),
            );
            o.set(
                "search",
                t.search.as_ref().map_or(Json::Null, |s| {
                    let mut j = Json::object();
                    j.set("iterations", s.iterations);
                    j.set("budget", s.budget);
                    j.set("cap", s.cap);
                    j.set("roots", s.roots.len());
                    j.set("extended", s.iterations > s.budget);
                    j.set("early_stop", s.iterations < s.cap);
                    j.set("applies_tree", s.effort.applies_tree);
                    j.set("applies_rollout", s.effort.applies_rollout);
                    j.set("applies_aux", s.effort.applies_aux);
                    j.set("worlds", s.effort.worlds);
                    j.set("rollouts_capped", s.effort.rollouts_capped);
                    j
                }),
            );
        }
    }
    let caps = match trace {
        Some(t) if agent == "greedy" => greedy_cap_hits(st, player, t.seed, GREEDY_MAX_DECISIONS)?,
        _ => None,
    };
    o.set("greedy_cap_hits", caps);
    let rest = match trace {
        Some(t) if t.search.is_some() => rest_prune_branch(st)?,
        _ => None,
    };
    o.set("rest_prune", rest);
    Ok(o)
}

/// A trace line keyed for ordering.
#[derive(Clone, PartialEq, Debug)]
pub struct Line {
    pub game: u64,
    pub seat: u8,
    pub k: u64,
    pub json: Json,
}

/// The NDJSON text of trace lines, ordered by game, seat and decision counter.
#[must_use]
pub fn ndjson(mut lines: Vec<Line>) -> String {
    lines.sort_by_key(|l| (l.game, l.seat, l.k));
    let opts = DumpOptions {
        indent: None,
        sort_keys: false,
        compact: true,
        ensure_ascii: true,
    };
    let mut out = String::new();
    for l in lines {
        let mut j = Json::object();
        j.set("game", l.game);
        j.set("seat", l.seat);
        if let Json::Object(items) = l.json {
            for (k, v) in items {
                j.set(k, v);
            }
        }
        out += &dumps(&j, opts);
        out.push('\n');
    }
    out
}

/// Where traced agents put their lines, keyed by agent seed (the benchmark pipeline names its
/// games only through the agent seeds, `runner/config.py:79-80`).
#[derive(Default)]
pub struct Sink {
    lines: Mutex<Vec<(u64, u64, Json)>>,
}

impl Sink {
    fn push(&self, seed: u64, k: u64, json: Json) {
        if let Ok(mut l) = self.lines.lock() {
            l.push((seed, k, json));
        }
    }

    /// The lines, each placed by `place(agent seed) -> (game, seat)`; lines of unknown seeds
    /// are dropped.
    #[must_use]
    pub fn take(&self, place: impl Fn(u64) -> Option<(u64, u8)>) -> Vec<Line> {
        let lines = self
            .lines
            .lock()
            .map(|mut l| std::mem::take(&mut *l))
            .unwrap_or_default();
        lines
            .into_iter()
            .filter_map(|(seed, k, json)| {
                place(seed).map(|(game, seat)| Line {
                    game,
                    seat,
                    k,
                    json,
                })
            })
            .collect()
    }
}

/// An agent that chooses through `choose_traced` and records each decision.
pub struct TracingAgent {
    inner: Box<dyn Agent>,
    seed: u64,
    sink: Arc<Sink>,
    count: u64,
}

impl TracingAgent {
    #[must_use]
    pub fn new(inner: Box<dyn Agent>, seed: u64, sink: Arc<Sink>) -> Self {
        Self {
            inner,
            seed,
            sink,
            count: 0,
        }
    }

    fn record(
        &mut self,
        st: &GameState,
        player: u8,
        action: Action,
        t: Option<&DecisionTrace>,
    ) -> Result<(), AgentError> {
        self.count += 1;
        let k = t.map_or(self.count, |t| t.k);
        let name = self.inner.name().to_owned();
        let json = decision_json(st, player, &name, action, t).map_err(AgentError::Engine)?;
        self.sink.push(self.seed, k, json);
        Ok(())
    }
}

impl Agent for TracingAgent {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn choose(
        &mut self,
        st: &GameState,
        player: u8,
        cx: &mut AgentCx<'_>,
    ) -> Result<Action, AgentError> {
        let (action, trace) = self.inner.choose_traced(st, player, cx, &mut NoTrace)?;
        self.record(st, player, action, trace.as_ref())?;
        Ok(action)
    }

    fn choose_traced(
        &mut self,
        st: &GameState,
        player: u8,
        cx: &mut AgentCx<'_>,
        tracer: &mut dyn SearchTracer,
    ) -> Result<(Action, Option<DecisionTrace>), AgentError> {
        let (action, trace) = self.inner.choose_traced(st, player, cx, tracer)?;
        self.record(st, player, action, trace.as_ref())?;
        Ok((action, trace))
    }

    fn decision_log(&self) -> &[LogEntry] {
        self.inner.decision_log()
    }

    fn effort(&self) -> Option<AgentEffort> {
        self.inner.effort()
    }
}

/// A factory whose agents are [`TracingAgent`]s around `inner`'s.
pub struct TracingFactory<'a> {
    pub inner: &'a dyn AgentFactory,
    pub sink: Arc<Sink>,
}

impl AgentFactory for TracingFactory<'_> {
    fn make(&self, spec: &AgentSpec) -> Result<Box<dyn Agent>, AgentError> {
        let inner = self.inner.make(spec)?;
        Ok(Box::new(TracingAgent::new(
            inner,
            spec.seed,
            Arc::clone(&self.sink),
        )))
    }

    fn describe(&self) -> String {
        self.inner.describe()
    }

    fn search_block(&self) -> Option<SearchSettings> {
        self.inner.search_block()
    }
}

/// The observer that traces every decision of a self-play game ([`crate::selfplay`]); the
/// first error of a greedy cap count is kept and ends the game afterwards.
pub struct TraceObserver {
    pub game: u64,
    pub names: [String; 2],
    pub lines: Vec<Line>,
    pub error: Option<EngineError>,
    count: [u64; 2],
}

impl TraceObserver {
    #[must_use]
    pub fn new(game: u64, names: [String; 2]) -> Self {
        Self {
            game,
            names,
            lines: Vec::new(),
            error: None,
            count: [0, 0],
        }
    }
}

impl crate::selfplay::Observer for TraceObserver {
    fn after(&mut self, st: &GameState, player: u8, action: Action, trace: Option<&DecisionTrace>) {
        let seat = usize::from(player);
        self.count[seat] += 1;
        let k = trace.map_or(self.count[seat], |t| t.k);
        match decision_json(st, player, &self.names[seat], action, trace) {
            Ok(json) => self.lines.push(Line {
                game: self.game,
                seat: player,
                k,
                json,
            }),
            Err(e) => {
                if self.error.is_none() {
                    self.error = Some(e);
                }
            }
        }
    }
}
