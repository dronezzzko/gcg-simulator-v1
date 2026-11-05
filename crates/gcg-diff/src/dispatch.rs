//! The name dispatcher of the real-engine replay (PLAN §8.2 `call`/`probe` records;
//! docs/dev/contracts.md, gcg-diff section): a recorded call names the Python function the
//! test reached (`<module>.<function>`, `testkit.<function>`, `Scenario.<method>`), and this
//! module runs the Rust function with Python's parameters and encodes the result as
//! `oracle/pytest_trace.py` (`TestTrace.enc`, `enc_result`) and `oracle/traces.py`
//! (`generate_view`) encode Python's. A name without an entry is an error, which the replay
//! counts as a divergence, never as a skip.
//!
//! Engine references: `{"enum": "GameState", "value": d16}` is the current state, and the
//! dispatcher checks that `d16` is its digest prefix; `"scenario"` is the state of the
//! `Scenario` under construction; `{"enum": "Derived", "value": "current"}` is
//! `view::derived` of the current state; `{"$": NodeId}` and `{"enum": "Program", ...}` are
//! nodes and programs of the current registry.

use gcg_core::pyjson::Json;
use gcg_data::DefId;
use gcg_effects::{Duration, FilterId, Kw, NodeId, ProgramId, Registry, RuleKind, Stat};
use gcg_engine::testkit::{self, engine as tk};
use gcg_engine::{
    Action, ActionKind, AddOptions, CheckError, Ctx, CtxVars, Derived, EngineError, EvKey,
    EventPayload, FrameArgs, FrameKind, GameState, LastingArgs, RuleInst, RuleKey, Scenario,
    ScenarioOptions, Step, TestkitError, Uid, Zone, invariants, observe, view,
};
use serde_json::{Value, json};

use crate::engine::{
    DIGEST_DUMP, RealEngine, SigForm, apply_error, diff_error, digest16, json_to_value,
};
use crate::json::sha256_hex;
use crate::replay::{SCENARIO_START, VIEW_SIGNATURE_PROBE, scenario_start_result};
use crate::{CallSpec, EngineRef, EnumValue, ErrorRec, TraceArg};

type Res<T> = Result<T, ErrorRec>;

// ---------------------------------------------------------------------------------------------
// Errors as Python raised them

fn engine_error(e: &EngineError) -> ErrorRec {
    ErrorRec {
        class: e.py_class().to_owned(),
        message: e.to_string(),
    }
}

/// A testkit failure as Python's exception (`testkit.py`: `AssertionError` from `_fail`,
/// `ValueError` from `vanilla_deck`, `KeyError(number)` from `CardDB.__getitem__`,
/// `cards/db.py:299-303`).
#[must_use]
pub fn testkit_error(e: &TestkitError) -> ErrorRec {
    let (class, message) = match e {
        TestkitError::Assertion(m) => ("AssertionError", m.clone()),
        TestkitError::Value(m) => ("ValueError", m.clone()),
        TestkitError::UnknownCard(n) => ("KeyError", gcg_data::py_repr(n)),
        TestkitError::Apply(a) => return apply_error(a),
        TestkitError::Engine(x) => return engine_error(x),
    };
    ErrorRec {
        class: class.to_owned(),
        message,
    }
}

fn check_error(e: &CheckError) -> ErrorRec {
    match e {
        CheckError::Violation(v) => ErrorRec {
            class: "InvariantViolation".to_owned(),
            message: v.to_string(),
        },
        CheckError::Engine(x) => engine_error(x),
    }
}

// ---------------------------------------------------------------------------------------------
// Result encodings (pytest_trace.py `TestTrace.enc`)

fn enum_json(name: &str, value: impl Into<Value>) -> Value {
    json!({"enum": name, "value": value.into()})
}

/// `Action.to_json()` (`engine/state.py:23-24`).
#[must_use]
pub fn action_json(a: &Action) -> Value {
    json!([a.kind.as_str(), a.a, a.b, a.c])
}

fn actions_json(list: &[Action]) -> Value {
    Value::Array(list.iter().map(action_json).collect())
}

/// `{"enum": "GameState", "value": d16}` of a state a call returned.
fn state_ref(st: &GameState) -> Value {
    enum_json("GameState", digest16(st))
}

/// A `RuleInst` (`[{"$": rule}, controller, source, key, aux]`, `pytest_trace.py` `enc`, with
/// a lasting key's effect as its content id, `_rule_key`).
fn rule_json(reg: &Registry, r: &RuleInst) -> Value {
    let key = match r.key {
        RuleKey::Constant { aid, host } => json!([aid.0, host]),
        RuleKey::Lasting { effect, serial } => {
            json!([-1, effect.content_id(reg).as_str(), serial])
        }
    };
    json!([{"$": r.rule.node().0}, r.controller, r.source, key, r.aux])
}

/// `sha16(text)` of `pytest_trace.py`.
fn sha16(text: &str) -> String {
    let mut d = sha256_hex(text.as_bytes());
    d.truncate(16);
    d
}

/// The `line()` form (`json.dumps(sort_keys=True, separators=(",", ":"))`, ASCII).
fn line(v: &Value) -> String {
    crate::json::py_dumps(v)
}

/// `traces.py` `view_signature_json` (the form `view::signature` returns) as
/// `pytest_trace.py` `view_sig`: per-uid maps become `[[uid, value], ...]` sorted by uid,
/// keyword maps `[[keyword, n], ...]` sorted, and rule nodes the sha16 of their CPython repr.
///
/// # Errors
/// A signature of another shape, or a rule node the registry does not hold.
pub fn tests_signature(reg: &Registry, sig: &Value) -> Result<Value, String> {
    let obj = sig
        .as_object()
        .ok_or_else(|| format!("view signature: expected an object, got {sig}"))?;
    let per_uid = |key: &str, f: &dyn Fn(&Value) -> Result<Value, String>| {
        let m = obj
            .get(key)
            .and_then(Value::as_object)
            .ok_or_else(|| format!("view signature: no map {key:?}"))?;
        let mut rows: Vec<(i64, Value)> = Vec::with_capacity(m.len());
        for (u, v) in m {
            let uid: i64 = u
                .parse()
                .map_err(|_| format!("view signature: {key} key {u:?} is not a uid"))?;
            rows.push((uid, f(v)?));
        }
        rows.sort_by_key(|(u, _)| *u);
        Ok::<Value, String>(Value::Array(
            rows.into_iter().map(|(u, v)| json!([u, v])).collect(),
        ))
    };
    let same = |v: &Value| Ok(v.clone());
    let kw = |v: &Value| {
        let m = v
            .as_object()
            .ok_or_else(|| format!("view signature: kw value {v}"))?;
        let mut pairs: Vec<(String, Value)> =
            m.iter().map(|(k, n)| (k.clone(), n.clone())).collect();
        pairs.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| cmp_int(&a.1, &b.1)));
        Ok(Value::Array(
            pairs.into_iter().map(|(k, n)| json!([k, n])).collect(),
        ))
    };
    let rules = |v: &Value| {
        let list = v
            .as_array()
            .ok_or_else(|| format!("view signature: rules value {v}"))?;
        let mut out = Vec::with_capacity(list.len());
        for r in list {
            let parts = r
                .as_array()
                .filter(|p| p.len() == 5)
                .ok_or_else(|| format!("view signature: rule {r}"))?;
            let id = match reg.decode_inline(&parts[0]) {
                Ok(gcg_effects::InlineNode::Arena(id)) => id,
                Ok(gcg_effects::InlineNode::Detached(_)) => {
                    return Err(format!(
                        "view signature: rule {} is no arena node",
                        parts[0]
                    ));
                }
                Err(e) => return Err(format!("view signature: rule {}: {e}", parts[0])),
            };
            let mut row = vec![Value::from(sha16(&reg.repr(id)))];
            row.extend(parts[1..].iter().cloned());
            out.push(Value::Array(row));
        }
        Ok(Value::Array(out))
    };
    Ok(json!({
        "ap": per_uid("ap", &same)?,
        "hp": per_uid("hp", &same)?,
        "kw": per_uid("kw", &kw)?,
        "traits": per_uid("traits", &same)?,
        "rules": per_uid("rules", &rules)?,
        "cost_mod": per_uid("cost_mod", &same)?,
        "level_mod": per_uid("level_mod", &same)?,
        "linked": obj.get("linked").cloned().unwrap_or(Value::Null),
        "abilities": per_uid("abilities", &same)?,
    }))
}

fn cmp_int(a: &Value, b: &Value) -> std::cmp::Ordering {
    a.as_i64().cmp(&b.as_i64())
}

/// The view signature of `dv` in the trace's form.
fn signature_in(form: SigForm, st: &GameState, dv: &Derived) -> Res<Value> {
    let sig = json_to_value(&view::signature(st, dv));
    match form {
        SigForm::Views => Ok(sig),
        SigForm::Tests => tests_signature(st.registry(), &sig).map_err(diff_error),
    }
}

// ---------------------------------------------------------------------------------------------
// Argument decoding

/// The positional and keyword arguments of one call, read in order.
struct Args<'a> {
    spec: &'a CallSpec,
    next: usize,
}

impl<'a> Args<'a> {
    const fn new(spec: &'a CallSpec) -> Self {
        Self { spec, next: 0 }
    }

    fn bad(&self, what: &str) -> ErrorRec {
        diff_error(format!(
            "{}: argument {} is not {what}: {:?}",
            self.spec.func,
            self.next,
            self.spec.args.get(self.next)
        ))
    }

    fn arg(&mut self) -> Res<&'a TraceArg> {
        let a = self.spec.args.get(self.next).ok_or_else(|| {
            diff_error(format!(
                "{}: missing argument {}",
                self.spec.func, self.next
            ))
        })?;
        self.next += 1;
        Ok(a)
    }

    fn opt(&mut self) -> Option<&'a TraceArg> {
        let a = self.spec.args.get(self.next)?;
        self.next += 1;
        Some(a)
    }

    fn rest(&mut self) -> &'a [TraceArg] {
        let r = self.spec.args.get(self.next..).unwrap_or_default();
        self.next = self.spec.args.len();
        r
    }

    fn kw(&self, name: &str) -> Option<&'a TraceArg> {
        self.spec
            .kwargs
            .iter()
            .find_map(|(k, v)| (k == name).then_some(v))
    }

    /// No arguments and keywords besides the ones read.
    fn done(&self, known_kwargs: &[&str]) -> Res<()> {
        if self.next < self.spec.args.len() {
            return Err(diff_error(format!(
                "{}: {} unexpected positional argument(s)",
                self.spec.func,
                self.spec.args.len() - self.next
            )));
        }
        match self
            .spec
            .kwargs
            .iter()
            .find(|(k, _)| !known_kwargs.contains(&k.as_str()))
        {
            Some((k, _)) => Err(diff_error(format!(
                "{}: unexpected keyword {k:?}",
                self.spec.func
            ))),
            None => Ok(()),
        }
    }

    fn int(&mut self) -> Res<i64> {
        match self.arg()? {
            TraceArg::Int(i) => Ok(*i),
            _ => Err(self.bad("an int")),
        }
    }

    fn num<T: TryFrom<i64>>(&mut self) -> Res<T> {
        let i = self.int()?;
        T::try_from(i).map_err(|_| diff_error(format!("{}: {i} is out of range", self.spec.func)))
    }

    fn uid(&mut self) -> Res<Uid> {
        self.num()
    }

    fn player(&mut self) -> Res<u8> {
        self.num()
    }

    fn str_(&mut self) -> Res<&'a str> {
        match self.arg()? {
            TraceArg::Str(s) => Ok(s),
            _ => Err(self.bad("a str")),
        }
    }

    fn enum_(&mut self, name: &str) -> Res<&'a EnumValue> {
        match self.arg()? {
            TraceArg::Enum { name: n, value } if n == name => Ok(value),
            _ => Err(self.bad(name)),
        }
    }

    fn uids(&mut self) -> Res<Vec<Uid>> {
        let TraceArg::List(items) = self.arg()? else {
            return Err(self.bad("a list of uids"));
        };
        items.iter().map(|x| uid_of(x, &self.spec.func)).collect()
    }

    fn engine_ref(&mut self) -> Res<&'a EngineRef> {
        match self.arg()? {
            TraceArg::Ref(r) => Ok(r),
            _ => Err(self.bad("an engine reference")),
        }
    }
}

fn int_of(a: &TraceArg, func: &str) -> Res<i64> {
    match a {
        TraceArg::Int(i) => Ok(*i),
        other => Err(diff_error(format!(
            "{func}: expected an int, got {other:?}"
        ))),
    }
}

fn uid_of(a: &TraceArg, func: &str) -> Res<Uid> {
    let i = int_of(a, func)?;
    Uid::try_from(i).map_err(|_| diff_error(format!("{func}: {i} is not a uid")))
}

fn kw_bool(a: Option<&TraceArg>, default: bool, func: &str) -> Res<bool> {
    match a {
        None => Ok(default),
        Some(TraceArg::Bool(b)) => Ok(*b),
        Some(other) => Err(diff_error(format!(
            "{func}: expected a bool, got {other:?}"
        ))),
    }
}

fn kw_int(a: Option<&TraceArg>, default: i64, func: &str) -> Res<i64> {
    a.map_or(Ok(default), |x| int_of(x, func))
}

fn zone_of_value(v: &EnumValue, func: &str) -> Res<Zone> {
    match v {
        EnumValue::Int(i) => Zone::from_value(*i),
        EnumValue::Str(_) => None,
    }
    .ok_or_else(|| diff_error(format!("{func}: no Zone {v:?}")))
}

fn str_enum<T>(v: &EnumValue, parse: impl Fn(&str) -> Option<T>, what: &str) -> Res<T> {
    match v {
        EnumValue::Str(s) => parse(s),
        EnumValue::Int(_) => None,
    }
    .ok_or_else(|| diff_error(format!("no {what} {v:?}")))
}

fn node_id(a: &TraceArg, func: &str) -> Res<NodeId> {
    match a {
        TraceArg::Node(n) => Ok(NodeId(*n)),
        other => Err(diff_error(format!(
            "{func}: expected a DSL node, got {other:?}"
        ))),
    }
}

fn filters_of(reg: &Registry, a: &TraceArg, func: &str) -> Res<Vec<FilterId>> {
    let TraceArg::List(items) = a else {
        return Err(diff_error(format!("{func}: expected a filter tuple")));
    };
    items
        .iter()
        .map(|x| {
            let n = node_id(x, func)?;
            reg.filter_id(n)
                .ok_or_else(|| diff_error(format!("{func}: node {} is no Filter", n.0)))
        })
        .collect()
}

fn event_of(a: Option<&TraceArg>, func: &str) -> Res<EventPayload> {
    let mut ev = EventPayload::default();
    let Some(a) = a else { return Ok(ev) };
    let TraceArg::List(items) = a else {
        return Err(diff_error(format!("{func}: expected an event tuple")));
    };
    for item in items {
        let TraceArg::List(kv) = item else {
            return Err(diff_error(format!("{func}: expected a (key, value) pair")));
        };
        let [TraceArg::Str(k), v] = kv.as_slice() else {
            return Err(diff_error(format!("{func}: expected a (key, value) pair")));
        };
        let key = EvKey::from_value(k)
            .ok_or_else(|| diff_error(format!("{func}: no event key {k:?}")))?;
        let value = i32::try_from(int_of(v, func)?)
            .map_err(|_| diff_error(format!("{func}: event value out of range")))?;
        ev.0.try_push((key, value))
            .map_err(|_| diff_error(format!("{func}: event payload over capacity")))?;
    }
    Ok(ev)
}

/// A `Ctx` (`[controller, host, card_uid, vars, event]`); only an absent or empty `vars` dict
/// has a Rust form outside a frame.
fn ctx_of(a: &TraceArg, func: &str) -> Res<Ctx> {
    let TraceArg::List(p) = a else {
        return Err(diff_error(format!("{func}: expected a Ctx")));
    };
    let [controller, host, card_uid, vars, event] = p.as_slice() else {
        return Err(diff_error(format!("{func}: a Ctx has five fields")));
    };
    match vars {
        TraceArg::Null => {}
        TraceArg::List(v) if v.is_empty() => {}
        other => {
            return Err(diff_error(format!(
                "{func}: a Ctx with vars {other:?} has no Rust form"
            )));
        }
    }
    let controller = u8::try_from(int_of(controller, func)?)
        .map_err(|_| diff_error(format!("{func}: Ctx controller")))?;
    let host =
        i32::try_from(int_of(host, func)?).map_err(|_| diff_error(format!("{func}: Ctx host")))?;
    let card_uid = i32::try_from(int_of(card_uid, func)?)
        .map_err(|_| diff_error(format!("{func}: Ctx card_uid")))?;
    Ok(Ctx::new(
        controller,
        host,
        card_uid,
        CtxVars::Empty,
        event_of(Some(event), func)?,
    ))
}

// ---------------------------------------------------------------------------------------------
// The current state

impl RealEngine {
    /// The state a `GameState` argument names: the current one, whose digest prefix must be
    /// the recorded one.
    fn current(&self, r: &EngineRef, func: &str) -> Res<&GameState> {
        match r {
            EngineRef::State(d16) => {
                let st = crate::EngineUnderTest::state(self)
                    .ok_or_else(|| diff_error(format!("{func}: no state is current")))?;
                let got = digest16(st);
                if got == *d16 {
                    Ok(st)
                } else {
                    Err(diff_error(format!(
                        "{func}: the GameState argument {d16} is not the current state {got}"
                    )))
                }
            }
            EngineRef::ScenarioState => self
                .scenario
                .as_ref()
                .map(Scenario::state)
                .ok_or_else(|| diff_error(format!("{func}: no Scenario is being built"))),
            other => Err(diff_error(format!(
                "{func}: expected a GameState, got {other:?}"
            ))),
        }
    }

    fn current_mut(&mut self, r: &EngineRef, func: &str) -> Res<&mut GameState> {
        self.current(r, func)?;
        match r {
            EngineRef::ScenarioState => self
                .scenario
                .as_mut()
                .map(Scenario::state_mut)
                .ok_or_else(|| diff_error(format!("{func}: no Scenario is being built"))),
            _ => self
                .state_mut()
                .ok_or_else(|| diff_error(format!("{func}: no state is current"))),
        }
    }

    fn view_of(&self, r: &EngineRef, func: &str) -> Res<std::sync::Arc<Derived>> {
        match r {
            EngineRef::View => {
                let st = crate::EngineUnderTest::state(self)
                    .ok_or_else(|| diff_error(format!("{func}: no state is current")))?;
                view::derived(st).map_err(|e| engine_error(&e))
            }
            other => Err(diff_error(format!(
                "{func}: expected a Derived, got {other:?}"
            ))),
        }
    }

    fn scenario_mut(&mut self, func: &str) -> Res<&mut Scenario> {
        self.scenario
            .as_mut()
            .ok_or_else(|| diff_error(format!("{func}: no Scenario is being built")))
    }
}

// ---------------------------------------------------------------------------------------------
// The dispatcher's names

/// The `probe` names with a dispatcher entry.
pub const PROBES: [&str; 29] = [
    VIEW_SIGNATURE_PROBE,
    "views.cards",
    "V.trigger_index",
    "testkit.options",
    "testkit.has_action",
    "testkit.zone_of",
    "testkit.ap",
    "testkit.hp",
    "testkit.keywords",
    "testkit.uids_in",
    "testkit.numbers_in",
    "testkit.legal_kinds",
    "testkit.card_numbers",
    "testkit.names",
    "view.has_kw",
    "view.kw_amount",
    "view.is_linked",
    "view.rules_of",
    "view.cdef",
    "view.cost_of",
    "view.level_of",
    "view.derived",
    "view.ap_of",
    "view.hp_of",
    "view.play_cost",
    "view.play_level",
    "view.traits_of",
    "view.stat",
    "view.matches",
];

/// The `call` names, and the calls of a `scenario` record's build, with a dispatcher entry.
pub const CALLS: [&str; 28] = [
    "Scenario.__init__",
    "Scenario.add",
    "Scenario.add_raw",
    "Scenario.hand",
    "Scenario.trash",
    "Scenario.shields",
    "Scenario.deck",
    "Scenario.resources",
    "Scenario.base",
    "Scenario.resource_deck",
    SCENARIO_START,
    "core.clear_listen_cache",
    "core.card_type",
    "game.advance",
    "game.concede",
    "game.legal_actions",
    "interp.draw",
    "interp.deploy_cards",
    "interp.push_frame",
    "interp.add_lasting",
    "invariants.initial_multiset",
    "invariants.check",
    "observe.is_hidden_from",
    "observe.unknown_instances",
    "observe.unknown_pools",
    "observe.determinize",
    "observe.permute_hidden",
    "observe.information_set_key",
];

// ---------------------------------------------------------------------------------------------
// Probes

/// A `probe` or `view` record's query.
///
/// # Errors
/// What the query raised, or a dispatcher miss.
pub fn probe(e: &mut RealEngine, c: &CallSpec) -> Res<Value> {
    let f = c.func.as_str();
    if !PROBES.contains(&f) {
        return Err(diff_error(format!("no dispatcher entry for probe {f}")));
    }
    let mut a = Args::new(c);
    let out = match f {
        VIEW_SIGNATURE_PROBE => {
            a.done(&[])?;
            let st = crate::EngineUnderTest::state(e)
                .ok_or_else(|| diff_error("view signature with no state"))?;
            // `effective_view` (pytest_trace.py) and `generate_view` (traces.py) compute a
            // missing view without storing it in the state: compute it on a clone, which
            // shares a cached view and stores a new one only in itself.
            let probe_st = st.clone();
            let dv = view::derived(&probe_st).map_err(|x| engine_error(&x))?;
            signature_in(e.sig_form, &probe_st, &dv)?
        }
        "views.cards" => {
            a.done(&[])?;
            views_cards(e)?
        }
        "V.trigger_index" => {
            a.done(&[])?;
            let st = crate::EngineUnderTest::state(e)
                .ok_or_else(|| diff_error("trigger_index with no state"))?;
            let dv = view::derived(st).map_err(|x| engine_error(&x))?;
            let idx = view::trigger_index(st, &dv);
            let mut out = serde_json::Map::new();
            for ev in gcg_effects::Ev::ALL {
                let hosts = idx.get(ev);
                if !hosts.is_empty() {
                    let rows = hosts.iter().map(|(u, aid)| json!([u, aid.0])).collect();
                    out.insert(ev.as_str().to_owned(), Value::Array(rows));
                }
            }
            Value::Object(out)
        }
        _ if f.starts_with("testkit.") => testkit_probe(e, f, &mut a)?,
        _ if f.starts_with("view.") => view_probe(e, f, &mut a)?,
        _ => return Err(diff_error(format!("no dispatcher entry for probe {f}"))),
    };
    Ok(out)
}

fn views_cards(e: &RealEngine) -> Res<Value> {
    let st =
        crate::EngineUnderTest::state(e).ok_or_else(|| diff_error("views.cards with no state"))?;
    let dv = view::derived(st).map_err(|x| engine_error(&x))?;
    let reg = st.registry();
    let mut rows = Vec::new();
    for (i, c) in st.cards().iter().enumerate() {
        if matches!(c.zone, Zone::Deck | Zone::ResourceDeck | Zone::Outside) {
            continue;
        }
        let uid = Uid::try_from(i).map_err(|_| diff_error("uid out of range"))?;
        let kws = view::keywords(&dv, uid);
        let mut kw = serde_json::Map::new();
        for k in Kw::ALL {
            if let Some(n) = kws.get(k) {
                kw.insert(k.as_str().to_owned(), Value::from(n));
            }
        }
        let traits: Vec<&str> = view::traits_of(st, &dv, uid).iter(reg).collect();
        let cost = view::play_cost(st, &dv, uid).map_err(|x| engine_error(&x))?;
        rows.push(json!([
            uid,
            view::ap_of(st, &dv, uid),
            view::hp_of(st, &dv, uid),
            Value::Object(kw),
            traits,
            cost,
            view::play_level(st, &dv, uid),
        ]));
    }
    Ok(Value::Array(rows))
}

fn testkit_probe(e: &RealEngine, f: &str, a: &mut Args<'_>) -> Res<Value> {
    let r = a.engine_ref()?;
    let st = e.current(r, f)?;
    let out = match f {
        "testkit.options" => {
            a.done(&[])?;
            actions_json(gcg_engine::legal_actions(st))
        }
        "testkit.has_action" => {
            let kind = str_enum(a.enum_("ActionKind")?, ActionKind::from_value, "ActionKind")?;
            let x = a.opt().map(|v| int_of(v, f)).transpose()?;
            let y = a.opt().map(|v| int_of(v, f)).transpose()?;
            a.done(&[])?;
            let narrow = |v: Option<i64>| {
                v.map(i32::try_from)
                    .transpose()
                    .map_err(|_| diff_error(format!("{f}: argument out of range")))
            };
            Value::from(testkit::has_action(st, kind, narrow(x)?, narrow(y)?))
        }
        "testkit.zone_of" => {
            let uid = a.uid()?;
            a.done(&[])?;
            enum_json("Zone", testkit::zone_of(st, uid).value())
        }
        "testkit.ap" | "testkit.hp" => {
            let uid = a.uid()?;
            a.done(&[])?;
            let v = if f == "testkit.ap" {
                testkit::ap(st, uid)
            } else {
                testkit::hp(st, uid)
            };
            Value::from(v.map_err(|x| testkit_error(&x))?)
        }
        "testkit.keywords" => {
            let uid = a.uid()?;
            a.done(&[])?;
            let kws = testkit::keywords(st, uid).map_err(|x| testkit_error(&x))?;
            Value::Array(kws.iter().map(|(k, n)| json!([k.as_str(), n])).collect())
        }
        "testkit.uids_in" | "testkit.numbers_in" => {
            let p = a.player()?;
            let zone = zone_of_value(a.enum_("Zone")?, f)?;
            a.done(&[])?;
            if f == "testkit.uids_in" {
                json!(testkit::uids_in(st, p, zone))
            } else {
                json!(testkit::numbers_in(st, p, zone))
            }
        }
        "testkit.legal_kinds" => {
            a.done(&[])?;
            // A Python set, written sorted by its members' JSON text (`TestTrace.enc`).
            let mut kinds: Vec<Value> = testkit::legal_kinds(st)
                .into_iter()
                .map(|k| enum_json("ActionKind", k.as_str()))
                .collect();
            kinds.sort_by_key(line);
            kinds.dedup();
            Value::Array(kinds)
        }
        "testkit.card_numbers" | "testkit.names" => {
            let uids = a.uids()?;
            a.done(&[])?;
            if f == "testkit.names" {
                json!(testkit::names(st, &uids))
            } else {
                json!(testkit::card_numbers(st, &uids))
            }
        }
        _ => return Err(diff_error(format!("no dispatcher entry for probe {f}"))),
    };
    Ok(out)
}

fn view_probe(e: &RealEngine, f: &str, a: &mut Args<'_>) -> Res<Value> {
    // `view.<fn>(dv, ...)`: functions whose first parameter is the view.
    if matches!(
        f,
        "view.has_kw" | "view.is_linked" | "view.rules_of" | "view.kw_amount"
    ) {
        let dv = e.view_of(a.engine_ref()?, f)?;
        let uid = a.uid()?;
        let out = match f {
            "view.has_kw" | "view.kw_amount" => {
                let kw = str_enum(a.enum_("Kw")?, Kw::from_value, "Kw")?;
                if f == "view.has_kw" {
                    Value::from(view::has_kw(&dv, uid, kw))
                } else {
                    Value::from(view::kw_amount(&dv, uid, kw))
                }
            }
            "view.is_linked" => Value::from(view::is_linked(&dv, uid)),
            _ => {
                let kind = str_enum(a.enum_("RuleKind")?, RuleKind::from_value, "RuleKind")?;
                let st = crate::EngineUnderTest::state(e)
                    .ok_or_else(|| diff_error(format!("{f}: no state")))?;
                Value::Array(
                    view::rules_of(&dv, uid, kind)
                        .map(|r| rule_json(st.registry(), r))
                        .collect(),
                )
            }
        };
        a.done(&[])?;
        return Ok(out);
    }
    let st = e.current(a.engine_ref()?, f)?;
    let out = match f {
        "view.cdef" => {
            let uid = a.uid()?;
            let DefId(d) = st.def_of(uid).def_id;
            enum_json("CardDef", d)
        }
        "view.cost_of" => Value::from(view::cost_of(st, a.uid()?)),
        "view.level_of" => Value::from(view::level_of(st, a.uid()?)),
        "view.derived" => {
            let dv = view::derived(st).map_err(|x| engine_error(&x))?;
            let sig = signature_in(e.sig_form, st, &dv)?;
            enum_json("Derived", sha16(&line(&sig)))
        }
        _ => {
            let dv = e.view_of(a.engine_ref()?, f)?;
            match f {
                "view.ap_of" => Value::from(view::ap_of(st, &dv, a.uid()?)),
                "view.hp_of" => Value::from(view::hp_of(st, &dv, a.uid()?)),
                "view.play_cost" => {
                    Value::from(view::play_cost(st, &dv, a.uid()?).map_err(|x| engine_error(&x))?)
                }
                "view.play_level" => Value::from(view::play_level(st, &dv, a.uid()?)),
                "view.traits_of" => {
                    let traits: Vec<&str> = view::traits_of(st, &dv, a.uid()?)
                        .iter(st.registry())
                        .collect();
                    json!(traits)
                }
                "view.stat" => {
                    let uid = a.uid()?;
                    let s = str_enum(a.enum_("Stat")?, Stat::from_value, "Stat")?;
                    Value::from(view::stat(st, &dv, uid, s))
                }
                "view.matches" => {
                    let ctx = ctx_of(a.arg()?, f)?;
                    let uid = a.uid()?;
                    let filters = filters_of(st.registry(), a.arg()?, f)?;
                    Value::from(
                        view::matches(st, &dv, &ctx, uid, &filters)
                            .map_err(|x| engine_error(&x))?,
                    )
                }
                _ => return Err(diff_error(format!("no dispatcher entry for probe {f}"))),
            }
        }
    };
    a.done(&[])?;
    Ok(out)
}

// ---------------------------------------------------------------------------------------------
// Calls

/// A `call` record's engine call, or a call of a `scenario` record's build.
///
/// # Errors
/// What the call raised, or a dispatcher miss.
pub fn call(e: &mut RealEngine, c: &CallSpec) -> Res<Value> {
    let f = c.func.as_str();
    if !CALLS.contains(&f) {
        return Err(diff_error(format!("no dispatcher entry for call {f}")));
    }
    let mut a = Args::new(c);
    if f.starts_with("Scenario.") {
        return scenario_call(e, f, &mut a);
    }
    if f == "core.clear_listen_cache" {
        // `GameConst.listened` belongs to one game and its own registry, so Python's cleared
        // cache and Rust's per-game set give the same events (docs/dev/contracts.md,
        // gcg-engine testkit section); the record's after16 checks that nothing changed.
        a.done(&[])?;
        return Ok(Value::Null);
    }
    let r = a.engine_ref()?;
    let out = match f {
        "game.advance" => {
            a.done(&[])?;
            tk::advance(e.current_mut(r, f)?).map_err(|x| engine_error(&x))?;
            Value::Null
        }
        "game.concede" => {
            let p = a.player()?;
            a.done(&[])?;
            gcg_engine::concede(e.current_mut(r, f)?, p);
            Value::Null
        }
        "game.legal_actions" => {
            a.done(&[])?;
            actions_json(gcg_engine::legal_actions(e.current(r, f)?))
        }
        "core.card_type" => {
            let uid = a.uid()?;
            a.done(&[])?;
            enum_json("CardType", tk::card_type(e.current(r, f)?, uid).as_str())
        }
        "interp.draw" => {
            let p = a.player()?;
            let n = a.int()?;
            let by_effect = kw_bool(a.kw("by_effect"), false, f)?;
            a.done(&["by_effect"])?;
            Value::from(
                tk::draw(e.current_mut(r, f)?, p, n, by_effect).map_err(|x| engine_error(&x))?,
            )
        }
        "interp.deploy_cards" => {
            let cards = a.uids()?;
            let rested = kw_bool(a.kw("rested"), false, f)?;
            let by = i32::try_from(kw_int(a.kw("by"), -1, f)?)
                .map_err(|_| diff_error(format!("{f}: by out of range")))?;
            let ex_used = i32::try_from(kw_int(a.kw("ex_used"), 0, f)?)
                .map_err(|_| diff_error(format!("{f}: ex_used out of range")))?;
            a.done(&["rested", "by", "ex_used"])?;
            tk::deploy_cards(e.current_mut(r, f)?, &cards, rested, by, ex_used)
                .map_err(|x| engine_error(&x))?;
            Value::Null
        }
        "interp.push_frame" => push_frame(e, r, f, &mut a)?,
        "interp.add_lasting" => add_lasting(e, r, f, &mut a)?,
        "invariants.initial_multiset" => {
            a.done(&[])?;
            let st = e.current(r, f)?;
            let m = invariants::initial_multiset(st);
            let v = multiset_json(st, &m);
            e.multisets.push((v.clone(), m));
            v
        }
        "invariants.check" => {
            let initial = match a.opt() {
                None | Some(TraceArg::Null) => None,
                Some(TraceArg::List(_)) => {
                    let want = &c.args[1];
                    Some(want)
                }
                Some(other) => {
                    return Err(diff_error(format!("{f}: initial is {other:?}")));
                }
            };
            a.done(&[])?;
            let st = e.current(r, f)?;
            let m = match initial {
                None => None,
                Some(arg) => Some(produced_multiset(e, arg, f)?),
            };
            invariants::check(st, m).map_err(|x| check_error(&x))?;
            Value::Null
        }
        "observe.is_hidden_from" => {
            let uid = a.uid()?;
            let observer = a.player()?;
            a.done(&[])?;
            Value::from(observe::is_hidden_from(e.current(r, f)?, uid, observer))
        }
        "observe.unknown_instances" => {
            let observer = a.player()?;
            let owner = a.player()?;
            a.done(&[])?;
            json!(observe::unknown_instances(
                e.current(r, f)?,
                observer,
                owner
            ))
        }
        "observe.unknown_pools" => {
            let observer = a.player()?;
            let owner = a.player()?;
            a.done(&[])?;
            let (main, res) = observe::unknown_pools(e.current(r, f)?, observer, owner);
            let ids = |v: &[DefId]| v.iter().map(|d| d.0).collect::<Vec<_>>();
            json!([ids(&main), ids(&res)])
        }
        "observe.determinize" | "observe.permute_hidden" => {
            let observer = a.player()?;
            let seed: u64 = a.num()?;
            a.done(&[])?;
            let st = e.current(r, f)?;
            let world = if f == "observe.determinize" {
                observe::determinize(st, observer, seed).map_err(|x| engine_error(&x))?
            } else {
                observe::permute_hidden(st, observer, seed)
            };
            state_ref(&world)
        }
        "observe.information_set_key" => {
            let observer = a.player()?;
            a.done(&[])?;
            let st = e.current(r, f)?;
            json_to_value(&observe::information_set_key(st, observer).to_json(st.registry()))
        }
        _ => return Err(diff_error(format!("no dispatcher entry for call {f}"))),
    };
    Ok(out)
}

/// `Counter((owner, base_def_id(def_id)) for non-token cards)` (`invariants.py:112-116`) as
/// `TestTrace.enc` writes a dict: `[[[owner, def_id], count], ...]` in the Counter's insertion
/// order, which is the order of first appearance over `st.cards`.
fn multiset_json(st: &GameState, m: &gcg_engine::CardMultiset) -> Value {
    let db = st.db();
    let mut order: Vec<(u8, DefId)> = Vec::new();
    for c in st.cards() {
        if db.by_id(c.def_id).is_token() {
            continue;
        }
        let key = (c.owner, db.base_def_id(c.def_id));
        if !order.contains(&key) {
            order.push(key);
        }
    }
    let entries = m.entries();
    let mut rows: Vec<Value> = Vec::with_capacity(entries.len());
    for key in &order {
        if let Some((_, n)) = entries.iter().find(|(k, _)| k == key) {
            rows.push(json!([[key.0, key.1.0], n]));
        }
    }
    for ((owner, def), n) in entries {
        if !order.contains(&(*owner, *def)) {
            rows.push(json!([[owner, def.0], n]));
        }
    }
    Value::Array(rows)
}

/// The multiset an earlier `invariants.initial_multiset` call of this replay returned with the
/// same entries (Counter equality ignores order).
fn produced_multiset<'e>(
    e: &'e RealEngine,
    arg: &TraceArg,
    f: &str,
) -> Res<&'e gcg_engine::CardMultiset> {
    let want = sorted_pairs(&trace_arg_json(arg));
    e.multisets
        .iter()
        .find(|(v, _)| sorted_pairs(v) == want)
        .map(|(_, m)| m)
        .ok_or_else(|| {
            diff_error(format!(
                "{f}: the initial multiset was not returned by an earlier initial_multiset call"
            ))
        })
}

fn sorted_pairs(v: &Value) -> Vec<String> {
    let mut rows: Vec<String> = v
        .as_array()
        .map(|a| a.iter().map(line).collect())
        .unwrap_or_default();
    rows.sort();
    rows
}

/// A decoded argument back as its JSON (ints and lists only, as multisets hold).
fn trace_arg_json(a: &TraceArg) -> Value {
    match a {
        TraceArg::Int(i) => Value::from(*i),
        TraceArg::List(items) => Value::Array(items.iter().map(trace_arg_json).collect()),
        TraceArg::Str(s) => Value::from(s.as_str()),
        TraceArg::Bool(b) => Value::from(*b),
        _ => Value::Null,
    }
}

/// `interp.push_frame(st, program_id, *, controller, host, card_uid=-1, kind, event=(),
/// vars=None)` (`engine/interp.py:61-71`); the result is the frame (`{"enum": "Frame",
/// "value": index}`).
fn push_frame(e: &mut RealEngine, r: &EngineRef, f: &str, a: &mut Args<'_>) -> Res<Value> {
    let program = match a.engine_ref()? {
        EngineRef::Program(p) => *p,
        other => {
            return Err(diff_error(format!(
                "{f}: expected a Program, got {other:?}"
            )));
        }
    };
    let controller = u8::try_from(
        a.kw("controller")
            .map(|v| int_of(v, f))
            .transpose()?
            .ok_or_else(|| diff_error(format!("{f}: controller is required")))?,
    )
    .map_err(|_| diff_error(format!("{f}: controller out of range")))?;
    let host = i32::try_from(
        a.kw("host")
            .map(|v| int_of(v, f))
            .transpose()?
            .ok_or_else(|| diff_error(format!("{f}: host is required")))?,
    )
    .map_err(|_| diff_error(format!("{f}: host out of range")))?;
    let card_uid = i32::try_from(kw_int(a.kw("card_uid"), -1, f)?)
        .map_err(|_| diff_error(format!("{f}: card_uid out of range")))?;
    let kind = match a.kw("kind") {
        Some(TraceArg::Str(k)) => FrameKind::from_value(k)
            .ok_or_else(|| diff_error(format!("{f}: no frame kind {k:?}")))?,
        _ => return Err(diff_error(format!("{f}: kind is required"))),
    };
    let event = event_of(a.kw("event"), f)?;
    let reg = std::sync::Arc::clone(e.registry());
    let mut owned: Vec<(gcg_effects::VarId, Vec<i32>)> = Vec::new();
    match a.kw("vars") {
        None | Some(TraceArg::Null) => {}
        Some(TraceArg::List(pairs)) => {
            for p in pairs {
                let TraceArg::List(kv) = p else {
                    return Err(diff_error(format!("{f}: vars entry {p:?}")));
                };
                let [TraceArg::Str(name), TraceArg::List(items)] = kv.as_slice() else {
                    return Err(diff_error(format!("{f}: vars entry {p:?}")));
                };
                let var = reg
                    .var_id(name)
                    .ok_or_else(|| diff_error(format!("{f}: no frame variable {name:?}")))?;
                let vals = items
                    .iter()
                    .map(|x| {
                        i32::try_from(int_of(x, f)?)
                            .map_err(|_| diff_error(format!("{f}: var value out of range")))
                    })
                    .collect::<Res<Vec<i32>>>()?;
                owned.push((var, vals));
            }
        }
        Some(other) => return Err(diff_error(format!("{f}: vars {other:?}"))),
    }
    a.done(&["controller", "host", "card_uid", "kind", "event", "vars"])?;
    let vars: Vec<(gcg_effects::VarId, &[i32])> =
        owned.iter().map(|(v, x)| (*v, x.as_slice())).collect();
    let args = FrameArgs {
        controller,
        host,
        card_uid,
        kind,
        event,
        vars: &vars,
    };
    let st = e.current_mut(r, f)?;
    let index = tk::push_frame(st, ProgramId(program.0), args).map_err(|x| engine_error(&x))?;
    Ok(enum_json("Frame", index))
}

/// `interp.add_lasting(st, effect, controller, source, targets, duration, *, player=-1,
/// uses=0, filters=(), aux=-1)` (`engine/interp.py:1286-1298`).
fn add_lasting(e: &mut RealEngine, r: &EngineRef, f: &str, a: &mut Args<'_>) -> Res<Value> {
    let reg = std::sync::Arc::clone(e.registry());
    let node = node_id(a.arg()?, f)?;
    let effect = reg
        .continuous_id(node)
        .ok_or_else(|| diff_error(format!("{f}: node {} is no Continuous", node.0)))?;
    let controller = a.player()?;
    let source: i32 = a.num()?;
    let TraceArg::List(ts) = a.arg()? else {
        return Err(diff_error(format!("{f}: targets must be a tuple")));
    };
    let mut targets: Vec<(Uid, i32)> = Vec::with_capacity(ts.len());
    for t in ts {
        let TraceArg::List(pair) = t else {
            return Err(diff_error(format!("{f}: target {t:?}")));
        };
        let [u, seq] = pair.as_slice() else {
            return Err(diff_error(format!("{f}: target {t:?}")));
        };
        let seq = i32::try_from(int_of(seq, f)?)
            .map_err(|_| diff_error(format!("{f}: target seq out of range")))?;
        targets.push((uid_of(u, f)?, seq));
    }
    let duration = str_enum(a.enum_("Duration")?, Duration::from_value, "Duration")?;
    let narrow = |v: i64, what: &str| {
        i32::try_from(v).map_err(|_| diff_error(format!("{f}: {what} out of range")))
    };
    let player = i8::try_from(kw_int(a.kw("player"), -1, f)?)
        .map_err(|_| diff_error(format!("{f}: player out of range")))?;
    let uses = narrow(kw_int(a.kw("uses"), 0, f)?, "uses")?;
    let aux = narrow(kw_int(a.kw("aux"), -1, f)?, "aux")?;
    let filters = match a.kw("filters") {
        None => Vec::new(),
        Some(x) => filters_of(&reg, x, f)?,
    };
    a.done(&["player", "uses", "filters", "aux"])?;
    let opts = LastingArgs {
        player,
        uses,
        filters: &filters,
        aux,
    };
    let st = e.current_mut(r, f)?;
    tk::add_lasting(st, effect, controller, source, &targets, duration, opts)
        .map_err(|x| engine_error(&x))?;
    Ok(Value::Null)
}

// ---------------------------------------------------------------------------------------------
// Scenario (testkit.py:38-182)

fn numbers<'a>(rest: &'a [TraceArg], f: &str) -> Res<Vec<&'a str>> {
    rest.iter()
        .map(|x| match x {
            TraceArg::Str(s) => Ok(s.as_str()),
            other => Err(diff_error(format!(
                "{f}: expected a card number, got {other:?}"
            ))),
        })
        .collect()
}

fn scenario_call(e: &mut RealEngine, f: &str, a: &mut Args<'_>) -> Res<Value> {
    if f == "Scenario.__init__" {
        let int = |k: &str, d: i64| kw_int(a.kw(k), d, f);
        let defaults = ScenarioOptions::default();
        let opts = ScenarioOptions {
            active: u8::try_from(int("active", i64::from(defaults.active))?)
                .map_err(|_| diff_error(format!("{f}: active out of range")))?,
            turn: i32::try_from(int("turn", i64::from(defaults.turn))?)
                .map_err(|_| diff_error(format!("{f}: turn out of range")))?,
            seed: u64::try_from(int("seed", 0)?)
                .map_err(|_| diff_error(format!("{f}: seed out of range")))?,
            deck_size: usize::try_from(int(
                "deck_size",
                i64::try_from(defaults.deck_size).unwrap_or(20),
            )?)
            .map_err(|_| diff_error(format!("{f}: deck_size out of range")))?,
        };
        a.done(&["active", "turn", "seed", "deck_size"])?;
        let mut sc = Scenario::new(e.registry(), opts);
        let units = e.force_link_units().to_vec();
        if !units.is_empty() {
            let list: Vec<&str> = units.iter().map(String::as_str).collect();
            sc.force_link(&list);
        }
        e.scenario = Some(sc);
        return Ok(Value::Null);
    }
    if f == SCENARIO_START {
        let step = match a.opt() {
            None => Step::Main,
            Some(TraceArg::Enum {
                name,
                value: EnumValue::Int(v),
            }) if name == "Step" => {
                Step::from_value(*v).ok_or_else(|| diff_error(format!("{f}: no Step {v}")))?
            }
            Some(other) => return Err(diff_error(format!("{f}: step {other:?}"))),
        };
        a.done(&[])?;
        let mut sc = e
            .scenario
            .take()
            .ok_or_else(|| diff_error(format!("{f}: no Scenario is being built")))?;
        sc.prepare_start(step).map_err(|x| testkit_error(&x))?;
        let before = scenario_start_result(&digest16(sc.state()), sc.state().rules_checked());
        let advanced = tk::advance(sc.state_mut());
        e.install(sc.into_state());
        advanced.map_err(|x| engine_error(&x))?;
        return Ok(before);
    }
    let p = a.player()?;
    let tk_err = |x: TestkitError| testkit_error(&x);
    let out = match f {
        "Scenario.add" => {
            let number = a.str_()?;
            let zone = match a.opt() {
                None => Zone::Battle,
                Some(TraceArg::Enum { name, value }) if name == "Zone" => zone_of_value(value, f)?,
                Some(other) => return Err(diff_error(format!("{f}: zone {other:?}"))),
            };
            let pilot = match a.kw("pilot") {
                None | Some(TraceArg::Null) => None,
                Some(TraceArg::Str(s)) => Some(s.as_str()),
                Some(other) => return Err(diff_error(format!("{f}: pilot {other:?}"))),
            };
            let opts = AddOptions {
                rested: kw_bool(a.kw("rested"), false, f)?,
                damage: i32::try_from(kw_int(a.kw("damage"), 0, f)?)
                    .map_err(|_| diff_error(format!("{f}: damage out of range")))?,
                pilot,
                deployed_this_turn: kw_bool(a.kw("deployed_this_turn"), false, f)?,
                known: kw_bool(a.kw("known"), true, f)?,
            };
            a.done(&["rested", "damage", "pilot", "deployed_this_turn", "known"])?;
            Value::from(
                e.scenario_mut(f)?
                    .add(p, number, zone, opts)
                    .map_err(tk_err)?,
            )
        }
        "Scenario.add_raw" => {
            let number = a.str_()?;
            let zone = zone_of_value(a.enum_("Zone")?, f)?;
            a.done(&[])?;
            Value::from(
                e.scenario_mut(f)?
                    .add_raw(p, number, zone)
                    .map_err(tk_err)?,
            )
        }
        "Scenario.hand" | "Scenario.trash" | "Scenario.shields" | "Scenario.deck" => {
            let list = numbers(a.rest(), f)?;
            a.done(&[])?;
            let sc = e.scenario_mut(f)?;
            match f {
                "Scenario.hand" => json!(sc.hand(p, &list).map_err(tk_err)?),
                "Scenario.trash" => json!(sc.trash(p, &list).map_err(tk_err)?),
                "Scenario.shields" => json!(sc.shields(p, &list).map_err(tk_err)?),
                _ => {
                    sc.deck(p, &list);
                    Value::Null
                }
            }
        }
        "Scenario.resources" => {
            let n: usize = a.num()?;
            let count = |k: &str| {
                usize::try_from(kw_int(a.kw(k), 0, f)?)
                    .map_err(|_| diff_error(format!("{f}: {k} out of range")))
            };
            let (rested, ex) = (count("rested")?, count("ex")?);
            a.done(&["rested", "ex"])?;
            json!(
                e.scenario_mut(f)?
                    .resources(p, n, rested, ex)
                    .map_err(tk_err)?
            )
        }
        "Scenario.base" => {
            let number = match a.opt() {
                None | Some(TraceArg::Null) => None,
                Some(TraceArg::Str(s)) => Some(s.as_str()),
                Some(other) => return Err(diff_error(format!("{f}: number {other:?}"))),
            };
            let damage = i32::try_from(kw_int(a.kw("damage"), 0, f)?)
                .map_err(|_| diff_error(format!("{f}: damage out of range")))?;
            a.done(&["damage"])?;
            Value::from(e.scenario_mut(f)?.base(p, number, damage).map_err(tk_err)?)
        }
        "Scenario.resource_deck" => {
            let n: usize = a.num()?;
            a.done(&[])?;
            e.scenario_mut(f)?.resource_deck(p, n).map_err(tk_err)?;
            Value::Null
        }
        _ => return Err(diff_error(format!("no dispatcher entry for {f}"))),
    };
    Ok(out)
}

/// The JSON text of a value in the `state_digest` form (for the CLI's explain output).
#[must_use]
pub fn digest_form(v: &Json) -> String {
    gcg_core::pyjson::dumps(v, DIGEST_DUMP)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader::trace_arg;

    fn spec(func: &str, args: Value, kwargs: Value) -> CallSpec {
        let args = args
            .as_array()
            .unwrap()
            .iter()
            .map(|v| trace_arg(v).unwrap())
            .collect();
        let kwargs = kwargs
            .as_array()
            .unwrap()
            .iter()
            .map(|kv| {
                (
                    kv[0].as_str().unwrap().to_owned(),
                    trace_arg(&kv[1]).unwrap(),
                )
            })
            .collect();
        CallSpec {
            func: func.to_owned(),
            args,
            kwargs,
        }
    }

    #[test]
    fn unknown_names_are_dispatcher_misses() {
        let mut e = RealEngine::new();
        let c = spec("core.no_such_function", json!([]), json!([]));
        let err = call(&mut e, &c).unwrap_err();
        assert_eq!(err.class, crate::engine::GCG_DIFF_ERROR);
        assert!(err.message.contains("no dispatcher entry"), "{err:?}");
        let p = spec("testkit.no_such_probe", json!([]), json!([]));
        assert!(probe(&mut e, &p).is_err());
        let v = spec("V.nothing", json!([]), json!([]));
        assert!(
            probe(&mut e, &v)
                .unwrap_err()
                .message
                .contains("no dispatcher entry")
        );
    }

    #[test]
    fn a_state_argument_needs_a_current_state() {
        let mut e = RealEngine::new();
        let c = spec(
            "observe.is_hidden_from",
            json!([{"enum": "GameState", "value": "0123456789abcdef"}, 3, 1]),
            json!([]),
        );
        let err = call(&mut e, &c).unwrap_err();
        assert!(err.message.contains("no state is current"), "{err:?}");
        // clear_listen_cache needs nothing and changes nothing.
        let c = spec("core.clear_listen_cache", json!([]), json!([]));
        assert_eq!(call(&mut e, &c), Ok(Value::Null));
        let c = spec("core.clear_listen_cache", json!([1]), json!([]));
        assert!(call(&mut e, &c).is_err());
    }

    #[test]
    fn contexts_events_and_errors_decode() {
        let ctx = ctx_of(
            &trace_arg(&json!([0, 5, -1, [], [["amount", 2]]])).unwrap(),
            "f",
        )
        .unwrap();
        assert_eq!((ctx.controller, ctx.host, ctx.card_uid), (0, 5, 5));
        assert_eq!(ctx.event.0.as_slice(), &[(EvKey::Amount, 2)]);
        assert!(
            ctx_of(
                &trace_arg(&json!([0, 5, -1, [["x", [1]]], []])).unwrap(),
                "f"
            )
            .is_err()
        );
        let e = testkit_error(&TestkitError::UnknownCard("GD99-999".into()));
        assert_eq!(
            (e.class.as_str(), e.message.as_str()),
            ("KeyError", "'GD99-999'")
        );
        let e = testkit_error(&TestkitError::Assertion("no legal action".to_owned()));
        assert_eq!(e.class, "AssertionError");
    }

    #[test]
    fn signatures_convert_to_the_per_test_form() {
        let reg = Registry::embedded();
        let sig = json!({
            "ap": {"10": 3, "2": 1},
            "hp": {"2": 4},
            "kw": {"2": {"Repair": 1, "Blocker": 0}},
            "traits": {"2": ["Zeon"]},
            "rules": {},
            "cost_mod": {},
            "level_mod": {"7": -1},
            "linked": [2, 4],
            "abilities": {"2": [5, 5]},
        });
        let got = tests_signature(&reg, &sig).unwrap();
        assert_eq!(
            got,
            json!({
                "ap": [[2, 1], [10, 3]],
                "hp": [[2, 4]],
                "kw": [[2, [["Blocker", 0], ["Repair", 1]]]],
                "traits": [[2, ["Zeon"]]],
                "rules": [],
                "cost_mod": [],
                "level_mod": [[7, -1]],
                "linked": [2, 4],
                "abilities": [[2, [5, 5]]],
            })
        );
        assert!(tests_signature(&reg, &json!({"ap": []})).is_err());
    }

    #[test]
    fn view_hashes_use_the_line_form() {
        assert_eq!(line(&json!({"b": 1, "a": [true]})), r#"{"a":[true],"b":1}"#);
        assert_eq!(sha16("abc"), "ba7816bf8f01cfea");
        assert_eq!(
            action_json(&Action::new(ActionKind::Attack, 0, -2, -1)),
            json!(["attack", 0, -2, -1])
        );
    }
}
