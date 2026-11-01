//! `MockEngine` (PLAN §8.7, §10.3.7): an [`EngineUnderTest`] that answers from a recorded trace,
//! so the replay logic, the reader and the negative controls are tested before the engine
//! exists (the M3 gate). It follows the recorded game only: an action other than the recorded
//! one is an error, because the mock cannot know the state it would lead to.
//!
//! Layout: game observations are indexed by the number of decisions applied (`k`): the pending
//! decision and fingerprint of decision `k`, and the snapshot (or the outcome state) at `k`.
//! Per-test observations follow the trace's record order: the `apply` records (each one's
//! `pending` is the decision before it), the calls the replay makes (`call` records, and the
//! Scenario calls of `scenario` records, `Scenario.start` answering the recorded state before
//! `advance` and moving to the record's `after16`), the `new_game`s (`scenario` records and
//! failed `game.new_game` calls), and the `probe`/`view` results; plus the digest and
//! `rules_checked` of the last loaded state or the last digest a record named.

use gcg_engine::{GameState, StateSnapshot};
use serde_json::Value;

use crate::json::state_digest;
use crate::replay::{
    SCENARIO_START, VIEW_SIGNATURE_PROBE, action_display, header_spec, scenario_advance,
    scenario_start_result,
};
use crate::{
    ApplyRec, CallSpec, DecisionRec, EngineUnderTest, ErrorRec, Fingerprint, Header, NewGameSpec,
    OptionsMode, OutcomeRec, PendingRec, Record, ScenarioRec, SnapshotRec, StateRec, Trace,
    TraceKind, TraceOptions,
};

/// A call the replay will make, with its recorded outcome and the digest after it (`None`:
/// unchanged).
#[derive(Clone, Debug)]
struct MockCall {
    spec: CallSpec,
    outcome: Result<Value, ErrorRec>,
    after16: Option<String>,
}

/// A `new_game` the replay will make: the spec, its outcome, and the state it leaves.
#[derive(Clone, Debug)]
struct MockNewGame {
    spec: NewGameSpec,
    outcome: Result<(), ErrorRec>,
    after16: String,
    rules_checked: Option<i64>,
    state: Option<Value>,
}

/// The recorded engine of one trace.
#[derive(Clone, Debug)]
pub struct MockEngine {
    header: Header,
    decisions: Vec<DecisionRec>,
    snapshots: Vec<SnapshotRec>,
    outcome: Option<OutcomeRec>,
    applies: Vec<ApplyRec>,
    calls: Vec<MockCall>,
    new_games: Vec<MockNewGame>,
    probes: Vec<(CallSpec, Result<Value, ErrorRec>)>,
    k: usize,
    current: Option<Value>,
    next_apply: usize,
    next_call: usize,
    next_new_game: usize,
    next_probe: usize,
    digest16: String,
    rules_checked: i64,
    /// A snapshot was loaded since the last game decision was applied: a game trace's
    /// fingerprint then reports the loaded state's digest and `rules_checked` (`--roundtrip`
    /// loads every snapshot of a game without applying anything).
    loaded: bool,
}

fn mock_error(message: String) -> ErrorRec {
    ErrorRec {
        class: "MockEngine".to_owned(),
        message,
    }
}

fn outcome_of(result: &Value, error: Option<&ErrorRec>) -> Result<Value, ErrorRec> {
    error.map_or_else(|| Ok(result.clone()), |e| Err(e.clone()))
}

impl MockEngine {
    /// The mock of `trace`. A trace with hashed option lists (tier 2) cannot be mocked
    /// faithfully: `pending()` cannot list the options a hash stands for, so check
    /// [`MockEngine::mockable`] first.
    #[must_use]
    pub fn new(trace: &Trace) -> Self {
        let mut m = Self {
            header: trace.header.clone(),
            decisions: Vec::new(),
            snapshots: Vec::new(),
            outcome: None,
            applies: Vec::new(),
            calls: Vec::new(),
            new_games: Vec::new(),
            probes: Vec::new(),
            k: 0,
            current: None,
            next_apply: 0,
            next_call: 0,
            next_new_game: 0,
            next_probe: 0,
            digest16: String::new(),
            rules_checked: -1,
            loaded: false,
        };
        for rec in &trace.records {
            match rec {
                Record::Decision(d) => m.decisions.push(d.clone()),
                Record::Snapshot(s) => m.snapshots.push(s.clone()),
                Record::Outcome(o) => m.outcome = Some(o.clone()),
                Record::Apply(a) => m.applies.push(a.clone()),
                Record::Call(c) => match &c.new_game {
                    Some(spec) => m.new_games.push(MockNewGame {
                        spec: spec.clone(),
                        outcome: c.error.clone().map_or(Ok(()), Err),
                        after16: c.after16.clone(),
                        rules_checked: None,
                        state: None,
                    }),
                    None => m.calls.push(MockCall {
                        spec: c.call.clone(),
                        outcome: outcome_of(&c.result, c.error.as_ref()),
                        after16: Some(c.after16.clone()),
                    }),
                },
                Record::Scenario(s) => m.scenario(s),
                Record::Probe(p) => m
                    .probes
                    .push((p.call.clone(), outcome_of(&p.result, p.error.as_ref()))),
                Record::View(v) => m.probes.push((
                    CallSpec {
                        func: VIEW_SIGNATURE_PROBE.to_owned(),
                        args: Vec::new(),
                        kwargs: Vec::new(),
                    },
                    outcome_of(&v.sig, v.error.as_ref()),
                )),
                _ => {}
            }
        }
        m
    }

    /// The observations a `scenario` record's replay asks for, in its order.
    fn scenario(&mut self, s: &ScenarioRec) {
        if let Some(spec) = &s.new_game {
            self.new_games.push(MockNewGame {
                spec: spec.clone(),
                outcome: Ok(()),
                after16: s.after16.clone(),
                rules_checked: Some(s.rules_checked),
                state: s.snapshot.as_ref().map(|snap| snap.state.clone()),
            });
            return;
        }
        let before16 = s.before16.clone().unwrap_or_default();
        if !s.calls_reproduce {
            self.calls.push(MockCall {
                spec: scenario_advance(&before16),
                outcome: Ok(Value::Null),
                after16: Some(s.after16.clone()),
            });
            return;
        }
        for c in &s.calls {
            let start = c.call.func == SCENARIO_START;
            self.calls.push(MockCall {
                spec: c.call.clone(),
                outcome: match &c.error {
                    Some(e) => Err(e.clone()),
                    None if start => Ok(scenario_start_result(&before16, s.rules_checked)),
                    None => Ok(Value::Null),
                },
                after16: start.then(|| s.after16.clone()),
            });
        }
    }

    /// Whether `trace` can be replayed against its mock (full option lists).
    #[must_use]
    pub fn mockable(trace: &Trace) -> bool {
        trace.header.options_mode == OptionsMode::Full
    }

    fn is_test(&self) -> bool {
        self.header.kind == TraceKind::Test
    }

    /// The state the recorded game shows after `k` decisions, if the trace stored it.
    fn state_at(&self, k: usize) -> Option<Value> {
        if let Some(s) = self.snapshots.iter().find(|s| s.index as usize == k) {
            return Some(s.snapshot.state.clone());
        }
        match &self.outcome {
            Some(o) if k == self.decisions.len() => Some(o.state.clone()),
            _ => None,
        }
    }

    fn set_loaded(&mut self, snapshot: &StateSnapshot) -> Result<(), ErrorRec> {
        self.digest16 = state_digest(&snapshot.state).map_err(mock_error)?[..16].to_owned();
        self.rules_checked = snapshot.rules_checked;
        self.current = Some(snapshot.state.clone());
        self.loaded = true;
        Ok(())
    }
}

fn pending_of(d: &DecisionRec) -> PendingRec {
    let options = match &d.options {
        TraceOptions::Full(list) => list.clone(),
        TraceOptions::Sha16(_) => Vec::new(),
    };
    PendingRec {
        player: d.player,
        kind: d.kind,
        options,
        prompt: d.prompt.clone(),
        context: d.context.clone(),
    }
}

impl EngineUnderTest for MockEngine {
    fn new_game(&mut self, spec: &NewGameSpec) -> Result<(), ErrorRec> {
        if !self.is_test() {
            if header_spec(&self.header).as_ref() != Some(spec) {
                return Err(mock_error(
                    "new_game differs from the recorded game".to_owned(),
                ));
            }
            self.k = 0;
            self.current = self.state_at(0);
            return Ok(());
        }
        let Some(ng) = self.new_games.get(self.next_new_game) else {
            return Err(mock_error("no recorded new_game left".to_owned()));
        };
        if ng.spec != *spec {
            return Err(mock_error(
                "new_game differs from the recorded one".to_owned(),
            ));
        }
        self.next_new_game += 1;
        self.digest16.clone_from(&ng.after16);
        if let Some(rc) = ng.rules_checked {
            self.rules_checked = rc;
        }
        if ng.state.is_some() {
            self.current.clone_from(&ng.state);
        }
        ng.outcome.clone()
    }

    fn load(&mut self, snapshot: &StateSnapshot) -> Result<(), ErrorRec> {
        self.set_loaded(snapshot)
    }

    fn load_state(&mut self, rec: &StateRec) -> Result<(), ErrorRec> {
        self.set_loaded(&rec.snapshot)
    }

    fn set_registry(&mut self, fragment: &Value) -> Result<(), ErrorRec> {
        if fragment.is_object() {
            Ok(())
        } else {
            Err(mock_error("an IR fragment is an object".to_owned()))
        }
    }

    fn force_link(&mut self, _units: &[String]) -> Result<(), ErrorRec> {
        Ok(())
    }

    fn pending(&self) -> Option<PendingRec> {
        if self.is_test() {
            return self
                .applies
                .get(self.next_apply)
                .and_then(|a| a.pending.clone());
        }
        self.decisions.get(self.k).map(pending_of)
    }

    fn state(&self) -> Option<&GameState> {
        None
    }

    fn observe(&self) -> Fingerprint {
        if !self.is_test()
            && let Some(d) = self.decisions.get(self.k)
        {
            let mut fp = d.fingerprint.clone();
            if self.loaded {
                fp.digest16.clone_from(&self.digest16);
                fp.rules_checked = self.rules_checked;
            }
            return fp;
        }
        let step = self
            .decisions
            .last()
            .map_or(gcg_engine::Step::SetupChooseFirst, |d| d.fingerprint.step);
        Fingerprint {
            version: 0,
            rules_checked: self.rules_checked,
            rng: 0,
            action_count: 0,
            event_group: 0,
            turn: 0,
            step,
            digest16: self.digest16.clone(),
        }
    }

    fn apply(&mut self, action: gcg_engine::Action, check: bool) -> Result<(), ErrorRec> {
        if self.is_test() {
            let Some(rec) = self.applies.get(self.next_apply) else {
                return Err(mock_error("no recorded apply left".to_owned()));
            };
            if rec.action != action || rec.check != check {
                return Err(mock_error(format!(
                    "apply {} differs from the recorded {}",
                    action_display(&action),
                    action_display(&rec.action)
                )));
            }
            self.next_apply += 1;
            self.digest16.clone_from(&rec.after16);
            return rec.error.clone().map_or(Ok(()), Err);
        }
        let Some(d) = self.decisions.get(self.k) else {
            return Err(ErrorRec {
                class: "IllegalActionError".to_owned(),
                message: "no decision pending".to_owned(),
            });
        };
        if let TraceOptions::Full(list) = &d.options
            && check
            && !list.contains(&action)
        {
            return Err(ErrorRec {
                class: "IllegalActionError".to_owned(),
                message: format!(
                    "illegal action {} for {}",
                    action_display(&action),
                    d.kind.as_str()
                ),
            });
        }
        if action != d.chosen {
            return Err(mock_error(format!(
                "the mock follows the recorded game: {} instead of {}",
                action_display(&action),
                action_display(&d.chosen)
            )));
        }
        self.k += 1;
        self.loaded = false;
        self.current = self.state_at(self.k);
        Ok(())
    }

    fn canonical(&self) -> Value {
        self.current.clone().unwrap_or(Value::Null)
    }

    fn probe(&mut self, call: &CallSpec) -> Result<Value, ErrorRec> {
        let Some((spec, result)) = self.probes.get(self.next_probe) else {
            return Err(mock_error(format!(
                "no recorded probe left for {}",
                call.func
            )));
        };
        if spec != call {
            return Err(mock_error(format!(
                "probe {} differs from the recorded {}",
                call.func, spec.func
            )));
        }
        self.next_probe += 1;
        result.clone()
    }

    fn call(&mut self, call: &CallSpec) -> Result<Value, ErrorRec> {
        let Some(rec) = self.calls.get(self.next_call) else {
            return Err(mock_error(format!(
                "no recorded call left for {}",
                call.func
            )));
        };
        if rec.spec != *call {
            return Err(mock_error(format!(
                "call {} differs from the recorded {}",
                call.func, rec.spec.func
            )));
        }
        self.next_call += 1;
        if let Some(after) = &rec.after16 {
            self.digest16.clone_from(after);
        }
        rec.outcome.clone()
    }
}
