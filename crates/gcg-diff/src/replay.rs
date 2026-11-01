//! Trace replay against an [`EngineUnderTest`] (PLAN §8.7): every record is checked in order
//! and the replay stops at the first divergence, reported as a [`Divergence`] whose `Display`
//! is the §8.7 report.
//!
//! Game and search traces (`port/oracle/traces.py`): a header with a seed starts the game with
//! `new_game`; a trace without one starts from its first snapshot (determinised worlds,
//! sampled view states). Each decision compares the pending decision, the option list and the
//! fingerprint, then the agent block (through an [`AgentOracle`]), then applies the recorded
//! choice. Snapshots compare the full canonical state, the outcome compares the final state and
//! digest. Per-test records (§8.2) are replayed through the engine's `apply`, `call`, `probe`,
//! `load_state`, `set_registry` and `force_link`.
//!
//! `Divergence.line` counts from the trace's header (line 1); the corpus driver adds the
//! header's line in the file.

#![expect(
    clippy::result_large_err,
    reason = "replay_trace returns Divergence by value (frozen in docs/port/contracts.md, gcg-diff); \
              every replay step shares that error type"
)]

use std::fmt;

use gcg_engine::{Action, NO_ARG};
use serde_json::Value;

use crate::json::{first_difference, short, state_digest};
use crate::options::options_sha16;
use crate::{
    AgentRec, AltRec, ApplyRec, ApplyTraceRec, CallRec, CallSpec, DecisionRec, DiffOptions,
    Divergence, EngineRef, EngineUnderTest, ErrorRec, Fingerprint, Header, IterRec, NewGameSpec,
    OutcomeRec, PendingRec, ProbeRec, Record, ReplayStats, ScenarioRec, SnapshotRec, Trace,
    TraceArg, TraceError, TraceKind, TraceOptions,
};

/// The function name under which the replay asks the engine for `tests/paranoid.py:24-39`'s
/// view signature (a `view` record), in the JSON form of `port/oracle/traces.py`
/// `view_signature_json`.
pub const VIEW_SIGNATURE_PROBE: &str = "paranoid.view_signature";

/// The Scenario method whose dispatcher result is the state before its `advance`
/// (`EngineUnderTest::call`).
pub const SCENARIO_START: &str = "Scenario.start";

/// The `new_game` of a game trace's header (`port/oracle/traces.py` `_start`: Python's default
/// `validate=True`), or `None` for a header without decks and seed.
#[must_use]
pub fn header_spec(h: &Header) -> Option<NewGameSpec> {
    Some(NewGameSpec {
        decks: h.decks.clone()?,
        seed: h.seed?,
        chooser: h.chooser,
        turn_limit: h.turn_limit,
        max_actions: h.max_actions,
        validate: true,
    })
}

/// The call that advances a `calls_reproduce: false` Scenario from its loaded pre-advance
/// snapshot, as `Scenario.start` does (`testkit.py:181`).
#[must_use]
pub fn scenario_advance(before16: &str) -> CallSpec {
    CallSpec {
        func: "game.advance".to_owned(),
        args: vec![TraceArg::Ref(EngineRef::State(before16.to_owned()))],
        kwargs: Vec::new(),
    }
}

/// What `Scenario.start` answers: the digest prefix and `_rules_checked` of the state before
/// its `advance`.
#[must_use]
pub fn scenario_start_result(before16: &str, rules_checked: i64) -> Value {
    serde_json::json!({"digest16": before16, "rules_checked": rules_checked})
}

/// What the Rust agent decided at a decision with an agent block: the action, the block
/// fields, and the level-2/3 records of its search when the trace has them.
#[derive(Clone, PartialEq, Debug)]
pub struct AgentObservation {
    pub action: Action,
    pub block: AgentRec,
    pub iters: Vec<IterRec>,
    pub apply_traces: Vec<ApplyTraceRec>,
}

/// The agent side of a replay. The real one runs the seat's Rust agent on
/// `EngineUnderTest::state` (`Agent::choose_traced`, workstreams D1/D2); the recorded one
/// replays another trace's blocks (the negative controls); [`DecodeOnly`] has none.
pub trait AgentOracle {
    /// The Rust observation for decision `rec` of `header`'s game, or `None` when this replay
    /// does not run agents.
    ///
    /// # Errors
    /// The agent failed, or agents are required but not available.
    fn decide(
        &mut self,
        header: &Header,
        rec: &DecisionRec,
        engine: &dyn EngineUnderTest,
    ) -> Result<Option<AgentObservation>, String>;
}

/// Agent blocks are decoded, not replayed: right for an engine without a state
/// (`MockEngine`). An engine with a state must replay them, so there it is an error.
#[derive(Clone, Copy, Debug, Default)]
pub struct DecodeOnly;

impl AgentOracle for DecodeOnly {
    fn decide(
        &mut self,
        _header: &Header,
        _rec: &DecisionRec,
        engine: &dyn EngineUnderTest,
    ) -> Result<Option<AgentObservation>, String> {
        if engine.state().is_some() {
            Err(
                "this gcg-diff build has no agent oracle (the agent replay needs the gcg-ai \
                 agents of workstreams D1/D2)"
                    .to_owned(),
            )
        } else {
            Ok(None)
        }
    }
}

/// Replay `t` against `engine`, stopping at the first divergence (PLAN §8.7, §10.3.7). Agent
/// blocks are compared when the engine has a state; with `MockEngine` they are decoded only.
///
/// # Errors
/// The first divergence.
pub fn replay_trace(
    t: &Trace,
    opts: &DiffOptions,
    engine: &mut dyn EngineUnderTest,
) -> Result<ReplayStats, Divergence> {
    replay_with(t, opts, engine, &mut DecodeOnly)
}

/// [`replay_trace`] with an explicit agent oracle.
///
/// # Errors
/// The first divergence.
pub fn replay_with(
    t: &Trace,
    opts: &DiffOptions,
    engine: &mut dyn EngineUnderTest,
    agents: &mut dyn AgentOracle,
) -> Result<ReplayStats, Divergence> {
    let mut r = Replay {
        t,
        engine,
        agents,
        stats: ReplayStats {
            records: 1,
            ..ReplayStats::default()
        },
        applied: 0,
        last_action: None,
        loaded: false,
        current: None,
        expected_iters: Vec::new(),
        expected_applies: Vec::new(),
    };
    if opts.parse_only {
        r.count_only();
        return Ok(r.stats);
    }
    if opts.roundtrip {
        r.roundtrip()?;
        return Ok(r.stats);
    }
    r.start()?;
    for (j, rec) in t.records.iter().enumerate() {
        r.stats.records += 1;
        r.record(j + 2, rec)?;
    }
    Ok(r.stats)
}

struct Replay<'a> {
    t: &'a Trace,
    engine: &'a mut dyn EngineUnderTest,
    agents: &'a mut dyn AgentOracle,
    stats: ReplayStats,
    applied: u32,
    last_action: Option<Action>,
    loaded: bool,
    current: Option<&'a DecisionRec>,
    expected_iters: Vec<IterRec>,
    expected_applies: Vec<ApplyTraceRec>,
}

impl<'a> Replay<'a> {
    fn div(&self, line: usize, field: &str, python: String, rust: String) -> Divergence {
        let d = self.current;
        Divergence {
            trace_id: self.t.header.id.clone(),
            line,
            decision: d.map(|d| d.index),
            turn: d.map(|d| d.fingerprint.turn),
            step: d.map(|d| d.fingerprint.step),
            player: d.map(|d| d.player),
            kind: d.map(|d| d.kind),
            field: field.to_owned(),
            python,
            rust,
            last_action: self.last_action,
            note: None,
        }
    }

    fn div_note(
        &self,
        line: usize,
        field: &str,
        python: String,
        rust: String,
        note: String,
    ) -> Divergence {
        let mut d = self.div(line, field, python, rust);
        d.note = Some(note);
        d
    }

    fn count_only(&mut self) {
        for rec in &self.t.records {
            self.stats.records += 1;
            match rec {
                Record::Decision(_) => self.stats.decisions += 1,
                Record::Snapshot(_) | Record::State(_) => self.stats.snapshots += 1,
                Record::Apply(_) => self.stats.applies += 1,
                Record::Call(_) => self.stats.calls += 1,
                Record::Probe(_) | Record::View(_) => self.stats.probes += 1,
                _ => {}
            }
        }
    }

    /// `--roundtrip` (M5.1): load every snapshot, re-serialise it, compare the canonical
    /// state and, where a decision follows at the same index, its digest prefix.
    fn roundtrip(&mut self) -> Result<(), Divergence> {
        let recs = &self.t.records;
        for (j, rec) in recs.iter().enumerate() {
            self.stats.records += 1;
            let line = j + 2;
            let Record::Snapshot(s) = rec else { continue };
            self.stats.snapshots += 1;
            self.engine
                .load(&s.snapshot)
                .map_err(|e| self.div(line, "load", "ok".to_owned(), err_text(&e)))?;
            let got = self.engine.canonical();
            if let Some((path, p, r)) = first_difference(&s.snapshot.state, &got, "state") {
                return Err(self.div(line, &path, p, r));
            }
            let next = recs[j + 1..].iter().find_map(|x| match x {
                Record::Decision(d) if d.index == s.index => Some(d),
                _ => None,
            });
            if let Some(d) = next {
                let digest =
                    state_digest(&got).map_err(|e| self.div(line, "digest", String::new(), e))?;
                if digest[..16] != d.fingerprint.digest16 {
                    return Err(self.div(
                        line,
                        "digest16",
                        d.fingerprint.digest16.clone(),
                        digest[..16].to_owned(),
                    ));
                }
            }
        }
        Ok(())
    }

    fn start(&mut self) -> Result<(), Divergence> {
        let h = &self.t.header;
        if h.kind != TraceKind::Test && h.seed.is_some() {
            let spec = header_spec(h).ok_or_else(|| {
                self.div(
                    1,
                    "header",
                    "decks with the seed".to_owned(),
                    "no decks".to_owned(),
                )
            })?;
            self.engine
                .new_game(&spec)
                .map_err(|e| self.div(1, "new_game", "ok".to_owned(), err_text(&e)))?;
            self.loaded = true;
        }
        Ok(())
    }

    fn record(&mut self, line: usize, rec: &'a Record) -> Result<(), Divergence> {
        if !matches!(rec, Record::Iter(_) | Record::ApplyTrace(_)) {
            self.finish_search_records(line)?;
        }
        match rec {
            Record::Header(_) => Err(self.div(
                line,
                "record",
                "no header inside a trace".to_owned(),
                "header".to_owned(),
            )),
            Record::Snapshot(s) => self.snapshot(line, s),
            Record::Decision(d) => self.decision(line, d),
            Record::Iter(it) => self.iter(line, it),
            Record::ApplyTrace(a) => self.apply_trace(line, a),
            Record::Outcome(o) => self.outcome(line, o),
            Record::Scenario(s) => self.scenario(line, s),
            Record::Apply(a) => self.test_apply(line, a),
            Record::Call(c) => self.call(line, c),
            Record::State(s) => {
                self.stats.snapshots += 1;
                self.engine
                    .load_state(s)
                    .map_err(|e| self.div(line, "load_state", "ok".to_owned(), err_text(&e)))?;
                self.loaded = true;
                Ok(())
            }
            Record::View(v) => {
                self.stats.probes += 1;
                let spec = CallSpec {
                    func: VIEW_SIGNATURE_PROBE.to_owned(),
                    args: Vec::new(),
                    kwargs: Vec::new(),
                };
                self.compare_probe(line, "view", &spec, v.error.as_ref().ok_or(&v.sig))
            }
            Record::Probe(p) => self.probe(line, p),
            Record::Override(o) => self
                .engine
                .set_registry(&o.fragment)
                .map_err(|e| self.div(line, "override", "ok".to_owned(), err_text(&e))),
            Record::ForceLink(units) => self
                .engine
                .force_link(units)
                .map_err(|e| self.div(line, "force_link", "ok".to_owned(), err_text(&e))),
            Record::End(_) => Ok(()),
        }
    }

    fn snapshot(&mut self, line: usize, s: &SnapshotRec) -> Result<(), Divergence> {
        self.stats.snapshots += 1;
        if s.index != self.applied {
            return Err(self.div(
                line,
                "snapshot index",
                s.index.to_string(),
                self.applied.to_string(),
            ));
        }
        if !self.loaded {
            self.engine
                .load(&s.snapshot)
                .map_err(|e| self.div(line, "load", "ok".to_owned(), err_text(&e)))?;
            self.loaded = true;
            return Ok(());
        }
        let got = self.engine.canonical();
        match first_difference(&s.snapshot.state, &got, "state") {
            None => Ok(()),
            Some((path, p, r)) => {
                let note = self
                    .current
                    .map(|d| format!("gcg-diff explain {} {}", self.t.header.id, d.index + 1));
                let mut d = self.div(line, &path, p, r);
                d.note = note;
                Err(d)
            }
        }
    }

    fn decision(&mut self, line: usize, d: &'a DecisionRec) -> Result<(), Divergence> {
        self.current = Some(d);
        self.stats.decisions += 1;
        if d.index != self.applied {
            return Err(self.div(
                line,
                "decision index",
                d.index.to_string(),
                self.applied.to_string(),
            ));
        }
        let Some(p) = self.engine.pending() else {
            return Err(self.div(
                line,
                "pending",
                format!("{} for player {}", d.kind.as_str(), d.player),
                "none".to_owned(),
            ));
        };
        self.compare_pending(line, d, &p)?;
        let fp = self.engine.observe();
        self.compare_fingerprint(line, d, &fp)?;
        if let Some(block) = &d.agent {
            self.agent(line, d, block)?;
        }
        self.engine.apply(d.chosen, true).map_err(|e| {
            self.div(
                line,
                "apply",
                format!("{} accepted", action_display(&d.chosen)),
                err_text(&e),
            )
        })?;
        self.applied += 1;
        self.last_action = Some(d.chosen);
        Ok(())
    }

    fn compare_pending(
        &self,
        line: usize,
        d: &DecisionRec,
        p: &PendingRec,
    ) -> Result<(), Divergence> {
        if p.player != d.player {
            return Err(self.div(line, "player", d.player.to_string(), p.player.to_string()));
        }
        if p.kind != d.kind {
            return Err(self.div(
                line,
                "kind",
                d.kind.as_str().to_owned(),
                p.kind.as_str().to_owned(),
            ));
        }
        if p.prompt != d.prompt {
            return Err(self.div(
                line,
                "prompt",
                format!("{:?}", d.prompt),
                format!("{:?}", p.prompt),
            ));
        }
        if p.context != d.context {
            return Err(self.div(
                line,
                "context",
                format!("{:?}", d.context),
                format!("{:?}", p.context),
            ));
        }
        match &d.options {
            TraceOptions::Full(list) => {
                if *list != p.options {
                    let i = list
                        .iter()
                        .zip(&p.options)
                        .position(|(a, b)| a != b)
                        .unwrap_or_else(|| list.len().min(p.options.len()));
                    return Err(self.div_note(
                        line,
                        "options",
                        options_display(list),
                        options_display(&p.options),
                        format!(
                            "first difference at option {i} ({} python, {} rust)",
                            list.len(),
                            p.options.len()
                        ),
                    ));
                }
            }
            TraceOptions::Sha16(h) => {
                let got = options_sha16(&p.options);
                if got != *h {
                    return Err(self.div(line, "options_sha16", h.clone(), got));
                }
            }
        }
        Ok(())
    }

    fn compare_fingerprint(
        &self,
        line: usize,
        d: &DecisionRec,
        fp: &Fingerprint,
    ) -> Result<(), Divergence> {
        let want = &d.fingerprint;
        let prev = d
            .index
            .checked_sub(1)
            .map_or_else(|| "the start".to_owned(), |k| format!("decision {k}"));
        let checks: [(&str, bool, String); 8] = [
            (
                "version",
                want.version == fp.version,
                format!(
                    "{:+}: an extra or missing touch() since {prev}",
                    i128::from(fp.version) - i128::from(want.version)
                ),
            ),
            (
                "rc",
                want.rules_checked == fp.rules_checked,
                format!("rules management was skipped or run differently since {prev}"),
            ),
            (
                "rng",
                want.rng == fp.rng,
                format!("an extra or missing RNG draw since {prev}"),
            ),
            (
                "action_count",
                want.action_count == fp.action_count,
                format!("a different number of applied actions since {prev}"),
            ),
            (
                "event_group",
                want.event_group == fp.event_group,
                format!("a different number of event groups since {prev}"),
            ),
            (
                "turn",
                want.turn == fp.turn,
                format!("the turn structure differs since {prev}"),
            ),
            (
                "step",
                want.step == fp.step,
                format!("the step machine differs since {prev}"),
            ),
            (
                "digest16",
                want.digest16 == fp.digest16,
                format!(
                    "the states differ: gcg-diff explain {} {}",
                    self.t.header.id, d.index
                ),
            ),
        ];
        match checks.into_iter().find(|(_, ok, _)| !ok) {
            None => Ok(()),
            Some((field, _, note)) => Err(self.div_note(
                line,
                field,
                fingerprint_display(want),
                fingerprint_display(fp),
                note,
            )),
        }
    }

    fn agent(&mut self, line: usize, d: &DecisionRec, block: &AgentRec) -> Result<(), Divergence> {
        let obs = self
            .agents
            .decide(&self.t.header, d, &*self.engine)
            .map_err(|e| self.div(line, "agent", "an agent decision".to_owned(), e))?;
        let Some(obs) = obs else { return Ok(()) };
        self.stats.agent_decisions += 1;
        if obs.action != d.chosen {
            return Err(self.div(
                line,
                "agent.action",
                action_display(&d.chosen),
                action_display(&obs.action),
            ));
        }
        let (want, got) = (block, &obs.block);
        let scalar: [(&str, u64, u64); 3] = [
            ("agent.k", want.k, got.k),
            ("agent.seed", want.seed, got.seed),
            ("agent.rng_after", want.rng_after, got.rng_after),
        ];
        if let Some((field, w, g)) = scalar.into_iter().find(|(_, w, g)| w != g) {
            return Err(self.div(line, field, w.to_string(), g.to_string()));
        }
        if want.alternatives.len() != got.alternatives.len() {
            return Err(self.div(
                line,
                "agent.alts (length)",
                want.alternatives.len().to_string(),
                got.alternatives.len().to_string(),
            ));
        }
        for (i, (w, g)) in want.alternatives.iter().zip(&got.alternatives).enumerate() {
            if let Some((field, p, r)) = alt_difference(w, g) {
                return Err(self.div(line, &format!("agent.alts[{i}].{field}"), p, r));
            }
        }
        let search: [(&str, String, String); 4] = [
            (
                "agent.iters",
                format!("{:?}", want.iterations),
                format!("{:?}", got.iterations),
            ),
            (
                "agent.budget",
                format!("{:?}", want.budget),
                format!("{:?}", got.budget),
            ),
            (
                "agent.cap",
                format!("{:?}", want.cap),
                format!("{:?}", got.cap),
            ),
            (
                "agent.roots",
                format!("{:?}", want.roots),
                format!("{:?}", got.roots),
            ),
        ];
        if let Some((field, p, r)) = search.into_iter().find(|(_, p, r)| p != r) {
            return Err(self.div(line, field, p, r));
        }
        let (want_log, got_log) = (Value::from(want.log.clone()), Value::from(got.log.clone()));
        if let Some((path, p, r)) = first_difference(&want_log, &got_log, "agent.log") {
            return Err(self.div(line, &path, p, r));
        }
        self.expected_iters = obs.iters;
        self.expected_applies = obs.apply_traces;
        Ok(())
    }

    fn iter(&mut self, line: usize, it: &IterRec) -> Result<(), Divergence> {
        if self.expected_iters.is_empty() && self.expected_applies.is_empty() {
            return Ok(());
        }
        if self.expected_iters.is_empty() {
            return Err(self.div(
                line,
                "iter",
                format!("iteration {}", it.iteration.n),
                "no further iteration".to_owned(),
            ));
        }
        let got = self.expected_iters.remove(0);
        if got != *it {
            let p = format!("{it:?}");
            let r = format!("{got:?}");
            return Err(self.div(line, &format!("iter[{}]", it.iteration.n), p, r));
        }
        Ok(())
    }

    fn apply_trace(&mut self, line: usize, a: &ApplyTraceRec) -> Result<(), Divergence> {
        if self.expected_applies.is_empty() {
            return Ok(());
        }
        let got = self.expected_applies.remove(0);
        if got != *a {
            return Err(self.div(
                line,
                &format!("apply_trace[{}][{}]", a.n, a.j),
                format!("{a:?}"),
                format!("{got:?}"),
            ));
        }
        Ok(())
    }

    /// Iteration records the Rust search produced beyond the ones the trace holds.
    fn finish_search_records(&mut self, line: usize) -> Result<(), Divergence> {
        if let Some(extra) = self.expected_iters.first() {
            return Err(self.div(
                line,
                "iter",
                "no further iteration".to_owned(),
                format!("iteration {}", extra.iteration.n),
            ));
        }
        if let Some(extra) = self.expected_applies.first() {
            return Err(self.div(
                line,
                "apply_trace",
                "no further apply".to_owned(),
                format!("{extra:?}"),
            ));
        }
        Ok(())
    }

    fn outcome(&mut self, line: usize, o: &OutcomeRec) -> Result<(), Divergence> {
        self.current = None;
        let got = self.engine.canonical();
        if let Some((path, p, r)) = first_difference(&o.state, &got, "state") {
            return Err(self.div(line, &path, p, r));
        }
        let digest = state_digest(&got)
            .map_err(|e| self.div(line, "final_digest", o.final_digest.clone(), e))?;
        if digest != o.final_digest {
            return Err(self.div(line, "final_digest", o.final_digest.clone(), digest));
        }
        let state_field = |k: &str| o.state.get(k).cloned().unwrap_or(Value::Null);
        let want_winner = o.winner.map_or(Value::Null, |w| Value::from(i64::from(w)));
        let want_end = o
            .end_reason
            .map_or(Value::Null, |e| Value::from(e.as_str()));
        let checks: [(&str, Value, Value); 3] = [
            ("winner", want_winner, state_field("winner")),
            ("end_reason", want_end, state_field("end_reason")),
            ("turn", Value::from(o.turn), state_field("turn")),
        ];
        if let Some((field, w, g)) = checks.into_iter().find(|(_, w, g)| w != g) {
            return Err(self.div(line, &format!("outcome.{field}"), short(&w), short(&g)));
        }
        if o.actions != u64::from(self.applied) {
            return Err(self.div(
                line,
                "outcome.actions",
                o.actions.to_string(),
                self.applied.to_string(),
            ));
        }
        Ok(())
    }

    /// A `scenario` record: `new_game` from its spec, or the Scenario calls in order (each
    /// raising what Python's raised, `Scenario.start` answering the state before `advance`),
    /// or, when the calls do not reproduce the state, the recorded pre-advance snapshot and
    /// `advance`; then the digest after.
    fn scenario(&mut self, line: usize, s: &ScenarioRec) -> Result<(), Divergence> {
        if let Some(ng) = &s.new_game {
            self.engine
                .new_game(ng)
                .map_err(|e| self.div(line, "scenario.new_game", "ok".to_owned(), err_text(&e)))?;
            self.loaded = true;
            let rc = self.engine.observe().rules_checked;
            if rc != s.rules_checked {
                return Err(self.div(
                    line,
                    "scenario.rules_checked",
                    s.rules_checked.to_string(),
                    rc.to_string(),
                ));
            }
            if let Some(snap) = &s.snapshot {
                self.stats.snapshots += 1;
                let got = self.engine.canonical();
                if let Some((path, p, r)) = first_difference(&snap.state, &got, "state") {
                    return Err(self.div(line, &path, p, r));
                }
            }
            return self.scenario_after(line, s);
        }
        let before16 = s.before16.as_deref().unwrap_or_default();
        if s.calls_reproduce {
            for c in &s.calls {
                self.stats.calls += 1;
                let field = format!("scenario {}", c.call.func);
                let got = self.engine.call(&c.call);
                compare_error(c.error.as_ref(), got.as_ref().err())
                    .map_err(|(p, r)| self.div(line, &field, p, r))?;
                if c.call.func == SCENARIO_START
                    && let Ok(v) = &got
                {
                    let want = scenario_start_result(before16, s.rules_checked);
                    if let Some((path, p, r)) = first_difference(&want, v, "before advance") {
                        return Err(self.div_note(
                            line,
                            &format!("{field} {path}"),
                            p,
                            r,
                            "the Rust testkit's Scenario state differs from Python's".to_owned(),
                        ));
                    }
                }
            }
        } else {
            let Some(snap) = &s.snapshot else {
                return Err(self.div(
                    line,
                    "scenario",
                    "a pre-advance snapshot".to_owned(),
                    "none".to_owned(),
                ));
            };
            self.stats.snapshots += 1;
            self.engine
                .load(snap)
                .map_err(|e| self.div(line, "scenario.load", "ok".to_owned(), err_text(&e)))?;
            let got = self.engine.observe();
            if got.digest16 != before16 {
                return Err(self.div(line, "scenario.before16", before16.to_owned(), got.digest16));
            }
            if got.rules_checked != s.rules_checked {
                return Err(self.div(
                    line,
                    "scenario.rules_checked",
                    s.rules_checked.to_string(),
                    got.rules_checked.to_string(),
                ));
            }
            self.stats.calls += 1;
            self.engine.call(&scenario_advance(before16)).map_err(|e| {
                self.div(line, "scenario game.advance", "ok".to_owned(), err_text(&e))
            })?;
        }
        self.loaded = true;
        self.scenario_after(line, s)
    }

    fn scenario_after(&self, line: usize, s: &ScenarioRec) -> Result<(), Divergence> {
        let after = self.engine.observe().digest16;
        if after == s.after16 {
            Ok(())
        } else {
            Err(self.div(line, "scenario.after16", s.after16.clone(), after))
        }
    }

    fn test_apply(&mut self, line: usize, a: &ApplyRec) -> Result<(), Divergence> {
        self.stats.applies += 1;
        let before = self.engine.observe().digest16;
        if before != a.before16 {
            return Err(self.div(line, "apply.before16", a.before16.clone(), before));
        }
        let pending = self.engine.pending();
        if pending != a.pending {
            return Err(self.div(
                line,
                "apply.pending",
                format!("{:?}", a.pending),
                format!("{pending:?}"),
            ));
        }
        let result = self.engine.apply(a.action, a.check);
        compare_error(a.error.as_ref(), result.err().as_ref())
            .map_err(|(p, r)| self.div(line, "apply.error", p, r))?;
        let after = self.engine.observe().digest16;
        if after != a.after16 {
            return Err(self.div(line, "apply.after16", a.after16.clone(), after));
        }
        self.last_action = Some(a.action);
        Ok(())
    }

    fn call(&mut self, line: usize, c: &CallRec) -> Result<(), Divergence> {
        self.stats.calls += 1;
        let result = match &c.new_game {
            Some(spec) => self.engine.new_game(spec).map(|()| Value::Null),
            None => self.engine.call(&c.call),
        };
        let field = format!("call {}", c.call.func);
        match (&c.error, result) {
            (None, Ok(v)) => {
                if let Some((path, p, r)) = first_difference(&c.result, &v, "result") {
                    return Err(self.div(line, &format!("{field} {path}"), p, r));
                }
            }
            (want, got) => {
                compare_error(want.as_ref(), got.err().as_ref())
                    .map_err(|(p, r)| self.div(line, &field, p, r))?;
            }
        }
        let after = self.engine.observe().digest16;
        if after != c.after16 {
            return Err(self.div(line, &format!("{field} after16"), c.after16.clone(), after));
        }
        Ok(())
    }

    fn probe(&mut self, line: usize, p: &ProbeRec) -> Result<(), Divergence> {
        self.stats.probes += 1;
        self.compare_probe(line, "probe", &p.call, p.error.as_ref().ok_or(&p.result))
    }

    /// A probe against its recorded outcome: `Err(result)` is the value Python returned,
    /// `Ok(error)` what it raised (the shape of `Option::ok_or` on the record's `error`).
    fn compare_probe(
        &mut self,
        line: usize,
        what: &str,
        spec: &CallSpec,
        want: Result<&ErrorRec, &Value>,
    ) -> Result<(), Divergence> {
        let field = format!("{what} {}", spec.func);
        let got = self.engine.probe(spec);
        match (want, got) {
            (Err(want), Ok(got)) => match first_difference(want, &got, "result") {
                None => Ok(()),
                Some((path, p, r)) => Err(self.div(line, &format!("{field} {path}"), p, r)),
            },
            (want, got) => compare_error(want.ok(), got.as_ref().err())
                .map_err(|(p, r)| self.div(line, &field, p, r)),
        }
    }
}

fn compare_error(want: Option<&ErrorRec>, got: Option<&ErrorRec>) -> Result<(), (String, String)> {
    match (want, got) {
        (None, None) => Ok(()),
        (Some(w), Some(g)) if w == g => Ok(()),
        (w, g) => Err((
            w.map_or_else(|| "no error".to_owned(), err_text),
            g.map_or_else(|| "no error".to_owned(), err_text),
        )),
    }
}

fn alt_difference(w: &AltRec, g: &AltRec) -> Option<(&'static str, String, String)> {
    if w.action != g.action {
        return Some((
            "action",
            action_display(&w.action),
            action_display(&g.action),
        ));
    }
    if w.value.to_bits() != g.value.to_bits() {
        return Some((
            "value",
            crate::floathex::to_hex(w.value),
            crate::floathex::to_hex(g.value),
        ));
    }
    if w.visits != g.visits {
        return Some(("visits", w.visits.to_string(), g.visits.to_string()));
    }
    None
}

fn err_text(e: &ErrorRec) -> String {
    format!("{}: {}", e.class, e.message)
}

/// `Action.__str__` (`engine/state.py:30-32`): `kind(a,b,c)` without the `-1` parameters.
#[must_use]
pub fn action_display(a: &Action) -> String {
    let args: Vec<String> = [a.a, a.b, a.c]
        .into_iter()
        .filter(|&x| x != NO_ARG)
        .map(|x| x.to_string())
        .collect();
    format!("{}({})", a.kind.as_str(), args.join(","))
}

fn options_display(list: &[Action]) -> String {
    let items: Vec<String> = list.iter().map(action_display).collect();
    format!("[{}]", items.join(", "))
}

fn fingerprint_display(f: &Fingerprint) -> String {
    format!(
        "version={} rc={} rng={} action_count={} event_group={} turn={} step={} digest16={}",
        f.version,
        f.rules_checked,
        f.rng,
        f.action_count,
        f.event_group,
        f.turn,
        f.step.py_name(),
        f.digest16
    )
}

impl fmt::Display for Divergence {
    /// The divergence report of PLAN §8.7.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "DIVERGED {}", self.trace_id)?;
        match (self.decision, self.turn, self.step, self.player, self.kind) {
            (Some(k), Some(turn), Some(step), Some(p), Some(kind)) => write!(
                f,
                " at decision {k} (turn {turn}, step {}, player {p}, kind {}), line {}",
                step.py_name(),
                kind.as_str(),
                self.line
            )?,
            _ => write!(f, " at line {}", self.line)?,
        }
        writeln!(f)?;
        writeln!(f, "  {:<13} {}", "first field", self.field)?;
        writeln!(f, "  {:<13} python {}", "values", self.python)?;
        writeln!(f, "  {:<13} rust   {}", "", self.rust)?;
        if let Some(note) = &self.note {
            writeln!(f, "  {:<13} {note}", "note")?;
        }
        if let Some(a) = &self.last_action {
            writeln!(f, "  {:<13} {}", "last action", action_display(a))?;
        }
        if let Some(k) = self.decision {
            writeln!(
                f,
                "  {:<13} gcg-diff explain {} {k}",
                "state diff", self.trace_id
            )?;
        }
        Ok(())
    }
}

impl fmt::Display for TraceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line == 0 {
            write!(f, "{}: {}", self.path.display(), self.detail)
        } else {
            write!(f, "{}:{}: {}", self.path.display(), self.line, self.detail)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gcg_engine::ActionKind;

    #[test]
    fn actions_print_like_python() {
        assert_eq!(
            action_display(&Action::new(ActionKind::PlayUnit, 2, -1, 1)),
            "play_unit(2,1)"
        );
        assert_eq!(
            action_display(&Action::of(ActionKind::EndMain)),
            "end_main()"
        );
        assert_eq!(
            action_display(&Action::new(ActionKind::Attack, 0, -2, -1)),
            "attack(0,-2)"
        );
    }
}
