//! The trace reader (PLAN §8.2, §10.3.7): one NDJSON line into a [`Record`], a file into its
//! [`Trace`]s. Decoding is strict: every key of every record must be known, every scalar must
//! be in the §8.2 canonical encoding, and a canonical state must keep Python's own guarantees
//! (`once_used` sorted by `json.dumps` text and free of duplicates, `runner/digest.py:48`).
//!
//! Key sets per record follow `oracle/traces.py` (game, search, view and world traces) and
//! the per-test records of §8.2 as `oracle/pytest_trace.py` (workstream G2) writes them:
//! `kwargs` are `[name, arg]` pairs in call order, call names are checked against the recorded
//! modules, and an `override` fragment must have the keys and version of
//! `oracle/export_ir.py` `build_fragment` (its content is `Registry::with_fragment`'s to
//! decode).

use std::path::Path;

use gcg_ai::{IterTrace, MoveKey};
use gcg_data::DefId;
use gcg_effects::{AbilityId, ProgramId};
use gcg_engine::{Action, ActionKind, DecisionKind, EndReason, StateSnapshot, Step};
use serde_json::Value;

use crate::floathex;
use crate::json::{Obj, as_array, as_bool, as_hex, as_i64, as_int, as_str, as_u64, kind, py_dumps};
use crate::{
    AgentRec, AltRec, ApplyRec, ApplyTraceRec, CallRec, CallSpec, DecisionRec, DerivedMark,
    EngineRef, EnumValue, ErrorRec, Fingerprint, Generator, Header, IterRec, MemoCheck, MemoRead,
    NewGameSpec, OptionsMode, OutcomeRec, OverrideKind, OverrideRec, PendingRec, ProbeRec, Record,
    ScenarioCall, ScenarioRec, SnapshotRec, StateRec, TRACE_FORMAT_VERSION, TestOutcome, Trace,
    TraceArg, TraceDeck, TraceError, TraceKind, TraceOptions, ViewRec,
};

/// Decode one NDJSON line (§8.2 encoding: `float.hex` floats, u64 values >= 2**53 as decimal
/// strings, actions as `[kind, a, b, c]`).
///
/// # Errors
/// Invalid JSON, an unknown `"t"`, a missing or unknown key, or a value outside its encoding.
pub fn parse_record(line: &str) -> Result<Record, String> {
    let v: Value = serde_json::from_str(line).map_err(|e| format!("invalid JSON: {e}"))?;
    let t = match v.get("t") {
        Some(Value::String(t)) => t.as_str(),
        Some(other) => return Err(format!("\"t\" must be a string, got {}", kind(other))),
        None => return Err("record has no \"t\"".to_owned()),
    };
    match t {
        "header" => header(&v).map(Record::Header),
        "snapshot" => snapshot_rec(&v, "snapshot").map(Record::Snapshot),
        "decision" => decision(&v).map(Record::Decision),
        "iter" => iter_rec(&v).map(Record::Iter),
        "apply_trace" => apply_trace(&v).map(Record::ApplyTrace),
        "outcome" => outcome(&v).map(Record::Outcome),
        "scenario" => scenario(&v).map(Record::Scenario),
        "apply" => apply_rec(&v).map(Record::Apply),
        "call" => call_rec(&v).map(Record::Call),
        "state" => state_rec(&v).map(Record::State),
        "view" => view_rec(&v).map(Record::View),
        "probe" => probe(&v).map(Record::Probe),
        "override" => override_rec(&v).map(Record::Override),
        "force_link" => single(&v, "force_link", "units").and_then(|u| {
            as_array(u, "force_link.units")?
                .iter()
                .map(|x| as_str(x, "force_link.units").map(str::to_owned))
                .collect::<Result<Vec<_>, _>>()
                .map(Record::ForceLink)
        }),
        "end" => single(&v, "end", "outcome").and_then(|o| match as_str(o, "end.outcome")? {
            "passed" => Ok(Record::End(TestOutcome::Passed)),
            "failed" => Ok(Record::End(TestOutcome::Failed)),
            other => Err(format!("end.outcome: unknown value {other:?}")),
        }),
        other => Err(format!("unknown record type {other:?}")),
    }
}

/// Read a trace file: one or more traces, each starting with a `header` record. Every line,
/// the last included, must end with `\n` (the exporter's form; a missing trailing newline is
/// one of the negative controls of §8.7).
///
/// # Errors
/// An unreadable file, an undecodable line, a record before the first header, or an empty file.
pub fn read_trace(path: &Path) -> Result<Vec<Trace>, TraceError> {
    let err = |line: usize, detail: String| TraceError {
        path: path.to_path_buf(),
        line,
        detail,
    };
    let bytes = std::fs::read(path).map_err(|e| err(0, format!("cannot read: {e}")))?;
    let text = String::from_utf8(bytes).map_err(|e| err(0, format!("not UTF-8: {e}")))?;
    parse_traces(&text).map_err(|(line, detail)| err(line, detail))
}

/// [`read_trace`] on text already in memory; errors carry the 1-based line.
///
/// # Errors
/// As [`read_trace`].
pub fn parse_traces(text: &str) -> Result<Vec<Trace>, (usize, String)> {
    if text.is_empty() {
        return Err((0, "empty trace file".to_owned()));
    }
    let mut traces: Vec<Trace> = Vec::new();
    let mut rest = text;
    let mut n = 0;
    while !rest.is_empty() {
        n += 1;
        let Some(end) = rest.find('\n') else {
            return Err((n, "the last line has no trailing newline".to_owned()));
        };
        let line = &rest[..end];
        rest = &rest[end + 1..];
        if !line.is_ascii() {
            return Err((n, "trace lines are ASCII (ensure_ascii=True)".to_owned()));
        }
        match parse_record(line).map_err(|e| (n, e))? {
            Record::Header(h) => traces.push(Trace {
                header: h,
                records: Vec::new(),
            }),
            rec => match traces.last_mut() {
                Some(t) => t.records.push(rec),
                None => return Err((n, "a record precedes the first header".to_owned())),
            },
        }
    }
    Ok(traces)
}

fn single<'a>(v: &'a Value, what: &'static str, key: &'static str) -> Result<&'a Value, String> {
    let mut o = Obj::new(v, what)?;
    o.req("t")?;
    let x = o.req(key)?;
    o.finish()?;
    Ok(x)
}

// ---------------------------------------------------------------------------------------------
// Header

fn header(v: &Value) -> Result<Header, String> {
    let mut o = Obj::new(v, "header")?;
    o.req("t")?;
    let version: u32 = as_int(o.req("v")?, "header.v")?;
    if version != TRACE_FORMAT_VERSION {
        return Err(format!(
            "trace format version {version}; this reader reads version {TRACE_FORMAT_VERSION}"
        ));
    }
    let id = as_str(o.req("id")?, "header.id")?.to_owned();
    if id.is_empty() {
        return Err("header.id is empty".to_owned());
    }
    let kind = match as_str(o.req("kind")?, "header.kind")? {
        "game" => TraceKind::Game,
        "test" => TraceKind::Test,
        "search" => TraceKind::Search,
        other => return Err(format!("header.kind: unknown value {other:?}")),
    };
    let generator = generator(o.req("generator")?)?;
    let decks = match o.nullable("decks") {
        None => None,
        Some(d) => {
            let items = as_array(d, "header.decks")?;
            if items.len() != 2 {
                return Err(format!("header.decks: {} decks, expected 2", items.len()));
            }
            Some([trace_deck(&items[0])?, trace_deck(&items[1])?])
        }
    };
    let seed = o
        .nullable("seed")
        .map(|s| as_u64(s, "header.seed"))
        .transpose()?;
    let chooser = o
        .nullable("chooser")
        .map(|c| seat(c, "header.chooser"))
        .transpose()?;
    let turn_limit = as_int(o.req("turn_limit")?, "header.turn_limit")?;
    let max_actions = as_i64(o.req("max_actions")?, "header.max_actions")?;
    let agents = as_array(o.req("agents")?, "header.agents")?
        .iter()
        .map(|a| as_str(a, "header.agents").map(str::to_owned))
        .collect::<Result<Vec<_>, _>>()?;
    let agent_seeds = as_array(o.req("agent_seeds")?, "header.agent_seeds")?
        .iter()
        .map(|s| as_u64(s, "header.agent_seeds"))
        .collect::<Result<Vec<_>, _>>()?;
    if agents.len() != agent_seeds.len() {
        return Err(format!(
            "header: {} agents but {} agent seeds",
            agents.len(),
            agent_seeds.len()
        ));
    }
    let preset = o
        .nullable("preset")
        .map(|p| as_str(p, "header.preset").map(str::to_owned))
        .transpose()?;
    let options_mode = match as_str(o.req("options_mode")?, "header.options_mode")? {
        "full" => OptionsMode::Full,
        "hash" => OptionsMode::Hash,
        other => return Err(format!("header.options_mode: unknown value {other:?}")),
    };
    o.finish()?;
    Ok(Header {
        version,
        id,
        kind,
        generator,
        decks,
        seed,
        chooser,
        turn_limit,
        max_actions,
        agents,
        agent_seeds,
        preset,
        options_mode,
    })
}

fn generator(v: &Value) -> Result<Generator, String> {
    let mut o = Obj::new(v, "header.generator")?;
    let s = |o: &mut Obj<'_>, k: &'static str| -> Result<String, String> {
        as_str(o.req(k)?, "header.generator").map(str::to_owned)
    };
    let g = Generator {
        git: s(&mut o, "git")?,
        python: s(&mut o, "python")?,
        code_digest: s(&mut o, "code_digest")?,
        ir_sha256: s(&mut o, "ir_sha256")?,
        data_lock: s(&mut o, "data_lock")?,
        hash_seed: as_int(o.req("hash_seed")?, "header.generator.hash_seed")?,
        effect_offset: as_int(o.req("effect_offset")?, "header.generator.effect_offset")?,
        max_uid: as_int(o.req("max_uid")?, "header.generator.max_uid")?,
    };
    o.finish()?;
    Ok(g)
}

fn trace_deck(v: &Value) -> Result<TraceDeck, String> {
    let mut o = Obj::new(v, "deck")?;
    let list = |x: &Value, what: &str| -> Result<Vec<String>, String> {
        as_array(x, what)?
            .iter()
            .map(|n| as_str(n, what).map(str::to_owned))
            .collect()
    };
    let main = list(o.req("main")?, "deck.main")?;
    let resources = list(o.req("res")?, "deck.res")?;
    o.finish()?;
    Ok(TraceDeck { main, resources })
}

fn seat(v: &Value, what: &str) -> Result<u8, String> {
    match as_int::<u8>(v, what)? {
        s @ (0 | 1) => Ok(s),
        s => Err(format!("{what}: seat {s} is not 0 or 1")),
    }
}

// ---------------------------------------------------------------------------------------------
// Game records

/// `[kind value, a, b, c]` (`engine/state.py:23-24`).
///
/// # Errors
/// Not a four-element action list with a known kind and `i32` parameters.
pub fn action(v: &Value, what: &str) -> Result<Action, String> {
    let items = as_array(v, what)?;
    if items.len() != 4 {
        return Err(format!(
            "{what}: an action has 4 elements, got {}",
            items.len()
        ));
    }
    let kind = as_str(&items[0], what)?;
    let kind = ActionKind::from_value(kind)
        .ok_or_else(|| format!("{what}: unknown action kind {kind:?}"))?;
    Ok(Action::new(
        kind,
        as_int(&items[1], what)?,
        as_int(&items[2], what)?,
        as_int(&items[3], what)?,
    ))
}

fn actions(v: &Value, what: &str) -> Result<Vec<Action>, String> {
    as_array(v, what)?.iter().map(|a| action(a, what)).collect()
}

fn decision_kind(v: &Value, what: &str) -> Result<DecisionKind, String> {
    let s = as_str(v, what)?;
    DecisionKind::from_value(s).ok_or_else(|| format!("{what}: unknown decision kind {s:?}"))
}

fn context(v: &Value, what: &str) -> Result<Vec<(String, i32)>, String> {
    as_array(v, what)?
        .iter()
        .map(|kv| {
            let pair = as_array(kv, what)?;
            if pair.len() != 2 {
                return Err(format!("{what}: a context entry is [key, value]"));
            }
            Ok((as_str(&pair[0], what)?.to_owned(), as_int(&pair[1], what)?))
        })
        .collect()
}

/// A canonical state: an object whose `once_used` is sorted by each key's `json.dumps` text
/// with default separators and has no duplicates (`runner/digest.py:48`; set semantics, §4.3).
fn canonical(v: &Value, what: &str) -> Result<(), String> {
    let Value::Object(map) = v else {
        return Err(format!(
            "{what}: a canonical state is an object, got {}",
            kind(v)
        ));
    };
    if let Some(once) = map.get("once_used") {
        let keys = as_array(once, &format!("{what}.once_used"))?;
        let texts: Vec<String> = keys.iter().map(default_dumps).collect();
        for (i, pair) in texts.windows(2).enumerate() {
            if pair[0] == pair[1] {
                return Err(format!(
                    "{what}.once_used[{}]: duplicate once-per-turn key {} (once_used is a set)",
                    i + 1,
                    pair[1]
                ));
            }
            if pair[0] > pair[1] {
                return Err(format!(
                    "{what}.once_used[{}]: not sorted by json.dumps text",
                    i + 1
                ));
            }
        }
    }
    Ok(())
}

/// `json.dumps(x)` with default separators (`", "`, `": "`), `ensure_ascii=True`, for the
/// once-key sort order: keys are lists of ints and 16-hex strings.
fn default_dumps(x: &Value) -> String {
    match x {
        Value::Array(items) => {
            let inner: Vec<String> = items.iter().map(default_dumps).collect();
            format!("[{}]", inner.join(", "))
        }
        other => py_dumps(other),
    }
}

fn snapshot_parts(o: &mut Obj<'_>) -> Result<StateSnapshot, String> {
    let what = o.what();
    let state = o.req("state")?.clone();
    canonical(&state, &format!("{what}.state"))?;
    let continuous = o.req("continuous")?.clone();
    let filter_sets = o.req("filter_sets")?.clone();
    for (name, table) in [("continuous", &continuous), ("filter_sets", &filter_sets)] {
        let Value::Object(map) = table else {
            return Err(format!("{what}.{name}: expected an object"));
        };
        for k in map.keys() {
            as_hex(&Value::String(k.clone()), 16, &format!("{what}.{name} key"))?;
        }
    }
    let rules_checked = as_i64(o.req("rules_checked")?, "rules_checked")?;
    Ok(StateSnapshot {
        state,
        continuous,
        filter_sets,
        rules_checked,
    })
}

fn snapshot_rec(v: &Value, what: &'static str) -> Result<SnapshotRec, String> {
    let mut o = Obj::new(v, what)?;
    o.req("t")?;
    let index = as_int(o.req("i")?, "snapshot.i")?;
    let snapshot = snapshot_parts(&mut o)?;
    o.finish()?;
    Ok(SnapshotRec { index, snapshot })
}

fn decision(v: &Value) -> Result<DecisionRec, String> {
    let mut o = Obj::new(v, "decision")?;
    o.req("t")?;
    let index = as_int(o.req("i")?, "decision.i")?;
    let player = seat(o.req("player")?, "decision.player")?;
    let kind = decision_kind(o.req("kind")?, "decision.kind")?;
    let prompt = as_str(o.req("prompt")?, "decision.prompt")?.to_owned();
    let context = context(o.req("context")?, "decision.context")?;
    let n: u32 = as_int(o.req("n")?, "decision.n")?;
    let options = match (o.opt("options"), o.opt("options_sha16")) {
        (Some(list), None) => {
            let list = actions(list, "decision.options")?;
            if list.len() != n as usize {
                return Err(format!("decision: n = {n} but {} options", list.len()));
            }
            TraceOptions::Full(list)
        }
        (None, Some(h)) => TraceOptions::Sha16(as_hex(h, 16, "decision.options_sha16")?.to_owned()),
        _ => return Err("decision: exactly one of options and options_sha16".to_owned()),
    };
    if n == 0 {
        return Err("decision: a pending decision has at least one option".to_owned());
    }
    let chosen = action(o.req("chosen")?, "decision.chosen")?;
    if let TraceOptions::Full(list) = &options
        && !list.contains(&chosen)
    {
        return Err("decision: the chosen action is not among the options".to_owned());
    }
    let fingerprint = Fingerprint {
        version: as_int(o.req("version")?, "decision.version")?,
        rules_checked: as_i64(o.req("rc")?, "decision.rc")?,
        rng: as_u64(o.req("rng")?, "decision.rng")?,
        action_count: as_i64(o.req("action_count")?, "decision.action_count")?,
        event_group: as_i64(o.req("event_group")?, "decision.event_group")?,
        turn: as_int(o.req("turn")?, "decision.turn")?,
        step: {
            let s = as_i64(o.req("step")?, "decision.step")?;
            Step::from_value(s).ok_or_else(|| format!("decision.step: unknown step {s}"))?
        },
        digest16: as_hex(o.req("digest16")?, 16, "decision.digest16")?.to_owned(),
    };
    let agent = o.opt("agent").map(agent_rec).transpose()?;
    o.finish()?;
    Ok(DecisionRec {
        index,
        player,
        kind,
        prompt,
        context,
        options,
        n,
        chosen,
        fingerprint,
        agent,
    })
}

fn move_key(v: &Value, what: &str) -> Result<MoveKey, String> {
    let items = as_array(v, what)?;
    if items.len() != 4 {
        return Err(format!("{what}: a move key has 4 elements"));
    }
    let k = as_str(&items[0], what)?;
    Ok(MoveKey {
        kind: ActionKind::from_value(k).ok_or_else(|| format!("{what}: unknown kind {k:?}"))?,
        a: as_i64(&items[1], what)?,
        b: as_i64(&items[2], what)?,
        variant: as_int(&items[3], what)?,
    })
}

fn float(v: &Value, what: &str) -> Result<f64, String> {
    floathex::from_hex(as_str(v, what)?).map_err(|e| format!("{what}: {e}"))
}

fn agent_rec(v: &Value) -> Result<AgentRec, String> {
    let mut o = Obj::new(v, "decision.agent")?;
    let k = as_int(o.req("k")?, "agent.k")?;
    let seed = as_u64(o.req("seed")?, "agent.seed")?;
    let rng_after = as_u64(o.req("rng_after")?, "agent.rng_after")?;
    let alternatives = as_array(o.req("alts")?, "agent.alts")?
        .iter()
        .map(|alt| {
            let parts = as_array(alt, "agent.alts")?;
            if parts.len() != 3 {
                return Err("agent.alts: an alternative is [action, value, visits]".to_owned());
            }
            Ok(AltRec {
                action: action(&parts[0], "agent.alts")?,
                value: float(&parts[1], "agent.alts")?,
                visits: as_int(&parts[2], "agent.alts")?,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let log = as_array(o.req("log")?, "agent.log")?
        .iter()
        .map(|entry| {
            if entry.is_object() {
                Ok(entry.clone())
            } else {
                Err("agent.log: every entry is an object".to_owned())
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    let search = [
        o.opt("iters"),
        o.opt("budget"),
        o.opt("cap"),
        o.opt("roots"),
    ];
    let (iterations, budget, cap, roots) = match search {
        [None, None, None, None] => (None, None, None, None),
        [Some(i), Some(b), Some(c), Some(r)] => (
            Some(as_int(i, "agent.iters")?),
            Some(as_int(b, "agent.budget")?),
            Some(as_int(c, "agent.cap")?),
            Some(
                as_array(r, "agent.roots")?
                    .iter()
                    .map(|m| move_key(m, "agent.roots"))
                    .collect::<Result<Vec<_>, _>>()?,
            ),
        ),
        _ => {
            return Err(
                "agent: iters, budget, cap and roots are all present or all absent".to_owned(),
            );
        }
    };
    o.finish()?;
    Ok(AgentRec {
        k,
        seed,
        rng_after,
        iterations,
        budget,
        cap,
        roots,
        alternatives,
        log,
    })
}

fn iter_rec(v: &Value) -> Result<IterRec, String> {
    let mut o = Obj::new(v, "iter")?;
    o.req("t")?;
    let decision = as_int(o.req("decision")?, "iter.decision")?;
    let iteration = IterTrace {
        n: as_int(o.req("n")?, "iter.n")?,
        child: as_int(o.req("child")?, "iter.child")?,
        visit: as_int(o.req("k")?, "iter.k")?,
        fresh: as_bool(o.req("fresh")?, "iter.fresh")?,
        path_len: as_int(o.req("path_len")?, "iter.path_len")?,
        applies_walk: as_int(o.req("applies_walk")?, "iter.applies_walk")?,
        applies_rollout: as_int(o.req("applies_rollout")?, "iter.applies_rollout")?,
        leaf: float(o.req("leaf")?, "iter.leaf")?,
        terminal: as_bool(o.req("terminal")?, "iter.terminal")?,
        total_before: float(o.req("total_before")?, "iter.total_before")?,
        total_after: float(o.req("total_after")?, "iter.total_after")?,
    };
    let world16 = as_hex(o.req("world16")?, 16, "iter.world16")?.to_owned();
    o.finish()?;
    Ok(IterRec {
        decision,
        iteration,
        world16,
    })
}

fn apply_trace(v: &Value) -> Result<ApplyTraceRec, String> {
    let mut o = Obj::new(v, "apply_trace")?;
    o.req("t")?;
    let r = ApplyTraceRec {
        decision: as_int(o.req("decision")?, "apply_trace.decision")?,
        n: as_int(o.req("n")?, "apply_trace.n")?,
        j: as_int(o.req("j")?, "apply_trace.j")?,
        action: action(o.req("action")?, "apply_trace.action")?,
        after16: as_hex(o.req("after16")?, 16, "apply_trace.after16")?.to_owned(),
    };
    o.finish()?;
    Ok(r)
}

fn outcome(v: &Value) -> Result<OutcomeRec, String> {
    let mut o = Obj::new(v, "outcome")?;
    o.req("t")?;
    let winner = o
        .nullable("winner")
        .map(|w| match as_int::<i8>(w, "outcome.winner")? {
            x @ -1..=1 => Ok(x),
            x => Err(format!("outcome.winner: {x} is not -1, 0 or 1")),
        })
        .transpose()?;
    let end_reason = o
        .nullable("end_reason")
        .map(|e| {
            let s = as_str(e, "outcome.end_reason")?;
            EndReason::from_value(s).ok_or_else(|| format!("outcome.end_reason: unknown {s:?}"))
        })
        .transpose()?;
    let turn = as_int(o.req("turn")?, "outcome.turn")?;
    let actions = as_int(o.req("actions")?, "outcome.actions")?;
    let final_digest = as_hex(o.req("final_digest")?, 64, "outcome.final_digest")?.to_owned();
    let state = o.req("state")?.clone();
    canonical(&state, "outcome.state")?;
    o.finish()?;
    Ok(OutcomeRec {
        winner,
        end_reason,
        turn,
        actions,
        final_digest,
        state,
    })
}

// ---------------------------------------------------------------------------------------------
// Per-test records

/// The fragment format this reader decodes (`oracle/export_ir.py` `FRAGMENT_VERSION`).
pub const FRAGMENT_VERSION: u32 = 1;

/// Engine modules a recorded call is named after (`pytest_trace.py` `ENGINE_MODULES`; `view`
/// names probes, the others calls).
const CALL_MODULES: [&str; 6] = ["core", "interp", "battle", "observe", "game", "invariants"];
const PROBE_MODULES: [&str; 2] = ["view", "testkit"];
/// The two probes of a `views` trace (`oracle/traces.py` `generate_view`): the per-card
/// view values and the trigger index.
const VIEW_SET_PROBES: [&str; 2] = ["views.cards", "V.trigger_index"];
/// The wrapped `testkit.Scenario` methods (`pytest_trace.py` `SCENARIO_METHODS`).
const SCENARIO_METHODS: [&str; 11] = [
    "__init__",
    "add",
    "add_raw",
    "hand",
    "trash",
    "shields",
    "deck",
    "resources",
    "base",
    "resource_deck",
    "start",
];

/// Where a [`CallSpec`] was recorded, which fixes the names it may carry.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Site {
    /// A `call` record: `<call module>.<fn>`.
    Call,
    /// A `probe` record: `view.<fn>` or `testkit.<fn>`, or a `views` trace's probe.
    Probe,
    /// A `scenario` record's build: any of these, or `Scenario.<method>`.
    Scenario,
}

fn check_name(func: &str, site: Site) -> Result<(), String> {
    let bad = || format!("{site:?} name {func:?}: not <module>.<function> of a recorded module");
    let (module, name) = func.split_once('.').ok_or_else(bad)?;
    let ident = name
        .bytes()
        .next()
        .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_');
    let known = match site {
        Site::Call => CALL_MODULES.contains(&module),
        Site::Probe => PROBE_MODULES.contains(&module) || VIEW_SET_PROBES.contains(&func),
        Site::Scenario => {
            (module == "Scenario" && SCENARIO_METHODS.contains(&name))
                || CALL_MODULES.contains(&module)
                || PROBE_MODULES.contains(&module)
        }
    };
    if ident && known { Ok(()) } else { Err(bad()) }
}

/// A §8.2 argument (`pytest_trace.py` `TestTrace.enc`): JSON scalars and lists, `{"$": NodeId}`,
/// `{"enum": name, "value": v}` (an [`EngineRef`] for the engine object type names), and a
/// `Decision` as `to_json()`.
///
/// # Errors
/// A float, an object of another shape, or an engine reference with a malformed value.
pub fn trace_arg(v: &Value) -> Result<TraceArg, String> {
    Ok(match v {
        Value::Null => TraceArg::Null,
        Value::Bool(b) => TraceArg::Bool(*b),
        Value::Number(_) => TraceArg::Int(as_i64(v, "argument")?),
        Value::String(s) => TraceArg::Str(s.clone()),
        Value::Array(items) => {
            TraceArg::List(items.iter().map(trace_arg).collect::<Result<_, _>>()?)
        }
        Value::Object(map) => {
            if map.len() == 1
                && let Some(id) = map.get("$")
            {
                return Ok(TraceArg::Node(as_int(id, "argument {\"$\"}")?));
            }
            if map.contains_key("enum") {
                return enum_arg(v);
            }
            if map.contains_key("kind") && map.contains_key("player") {
                return pending_rec(v).map(TraceArg::Decision);
            }
            return Err(format!(
                "argument: an object is {{\"$\": id}}, {{\"enum\", \"value\"}} or a Decision, got \
                 keys {:?}",
                map.keys().collect::<Vec<_>>()
            ));
        }
    })
}

fn enum_arg(v: &Value) -> Result<TraceArg, String> {
    let mut o = Obj::new(v, "enum argument")?;
    let name = as_str(o.req("enum")?, "argument.enum")?.to_owned();
    let value = o.req("value")?;
    let what = "argument.value";
    let current = |kind: &str| -> Result<(), String> {
        match as_str(value, what)? {
            "current" => Ok(()),
            other => Err(format!(
                "{what}: a {kind} reference is \"current\", got {other:?}"
            )),
        }
    };
    let arg = match name.as_str() {
        "GameState" => match as_str(value, what)? {
            "scenario" => TraceArg::Ref(EngineRef::ScenarioState),
            _ => TraceArg::Ref(EngineRef::State(as_hex(value, 16, what)?.to_owned())),
        },
        "Derived" => {
            current("Derived")?;
            TraceArg::Ref(EngineRef::View)
        }
        "Registry" => {
            current("Registry")?;
            TraceArg::Ref(EngineRef::Registry)
        }
        "CardDef" => TraceArg::Ref(EngineRef::CardDef(DefId(as_int(value, what)?))),
        "AbilityEntry" => TraceArg::Ref(EngineRef::Ability(AbilityId(as_int(value, what)?))),
        "Program" => TraceArg::Ref(EngineRef::Program(ProgramId(as_int(value, what)?))),
        "Frame" => match as_int::<i32>(value, what)? {
            i @ -1.. => TraceArg::Ref(EngineRef::Frame(i)),
            i => return Err(format!("{what}: frame index {i} is below -1")),
        },
        _ => TraceArg::Enum {
            name,
            value: match value {
                Value::String(s) => EnumValue::Str(s.clone()),
                n @ Value::Number(_) => EnumValue::Int(as_i64(n, what)?),
                other => {
                    return Err(format!("{what}: expected int or str, got {}", kind(other)));
                }
            },
        },
    };
    o.finish()?;
    Ok(arg)
}

fn call_spec(o: &mut Obj<'_>, site: Site) -> Result<CallSpec, String> {
    let what = o.what();
    let func = as_str(o.req("fn")?, what)?.to_owned();
    check_name(&func, site)?;
    let args = as_array(o.req("args")?, what)?
        .iter()
        .map(trace_arg)
        .collect::<Result<Vec<_>, _>>()?;
    let kwargs = match o.opt("kwargs") {
        None => Vec::new(),
        Some(Value::Array(pairs)) => pairs
            .iter()
            .map(|p| {
                let kv = as_array(p, "kwargs")?;
                if kv.len() != 2 {
                    return Err("kwargs: an entry is [name, argument]".to_owned());
                }
                Ok((as_str(&kv[0], "kwargs")?.to_owned(), trace_arg(&kv[1])?))
            })
            .collect::<Result<Vec<_>, _>>()?,
        Some(other) => {
            return Err(format!(
                "kwargs: expected a list of [name, argument] pairs, got {}",
                kind(other)
            ));
        }
    };
    Ok(CallSpec { func, args, kwargs })
}

fn scenario_call(v: &Value) -> Result<ScenarioCall, String> {
    let mut o = Obj::new(v, "scenario call")?;
    let call = call_spec(&mut o, Site::Scenario)?;
    let error = match o.opt("error") {
        None => None,
        Some(e) => Some(error_rec(e, "scenario call error")?),
    };
    o.finish()?;
    Ok(ScenarioCall { call, error })
}

fn new_game_spec(v: &Value) -> Result<NewGameSpec, String> {
    let mut o = Obj::new(v, "new_game")?;
    let decks = as_array(o.req("decks")?, "new_game.decks")?;
    if decks.len() != 2 {
        return Err("new_game.decks: expected 2 decks".to_owned());
    }
    let chooser = o.req("chooser")?;
    let spec = NewGameSpec {
        decks: [trace_deck(&decks[0])?, trace_deck(&decks[1])?],
        seed: as_u64(o.req("seed")?, "new_game.seed")?,
        chooser: match chooser {
            Value::Null => None,
            c => Some(seat(c, "new_game.chooser")?),
        },
        turn_limit: as_int(o.req("turn_limit")?, "new_game.turn_limit")?,
        max_actions: as_i64(o.req("max_actions")?, "new_game.max_actions")?,
        validate: as_bool(o.req("validate")?, "new_game.validate")?,
    };
    o.finish()?;
    Ok(spec)
}

fn hex16(o: &mut Obj<'_>, key: &'static str, what: &'static str) -> Result<String, String> {
    as_hex(o.req(key)?, 16, what).map(str::to_owned)
}

fn scenario(v: &Value) -> Result<ScenarioRec, String> {
    let mut o = Obj::new(v, "scenario")?;
    o.req("t")?;
    let test = as_str(o.req("test")?, "scenario.test")?.to_owned();
    let tags = as_array(o.req("tags")?, "scenario.tags")?
        .iter()
        .map(|t| as_str(t, "scenario.tags").map(str::to_owned))
        .collect::<Result<Vec<_>, _>>()?;
    let calls = as_array(o.req("calls")?, "scenario.calls")?
        .iter()
        .map(scenario_call)
        .collect::<Result<Vec<_>, _>>()?;
    let new_game = match o.req("new_game")? {
        Value::Null => None,
        ng => Some(new_game_spec(ng)?),
    };
    let before16 = o
        .opt("before16")
        .map(|b| as_hex(b, 16, "scenario.before16").map(str::to_owned))
        .transpose()?;
    let after16 = hex16(&mut o, "after16", "scenario.after16")?;
    let calls_reproduce = as_bool(o.req("calls_reproduce")?, "scenario.calls_reproduce")?;
    let (snapshot, rules_checked) = if v.get("state").is_some() {
        let s = snapshot_parts(&mut o)?;
        let rc = s.rules_checked;
        (Some(s), rc)
    } else {
        (
            None,
            as_i64(o.req("rules_checked")?, "scenario.rules_checked")?,
        )
    };
    o.finish()?;
    let started = |c: Option<&ScenarioCall>, name: &str| {
        c.is_some_and(|c| c.call.func == name && c.error.is_none())
    };
    if new_game.is_some() {
        if !calls.is_empty() || before16.is_some() || !calls_reproduce {
            return Err(
                "scenario: a new_game record has no calls, no before16 and calls_reproduce true"
                    .to_owned(),
            );
        }
    } else if before16.is_none()
        || !started(calls.first(), "Scenario.__init__")
        || !started(calls.last(), "Scenario.start")
    {
        return Err(
            "scenario: a Scenario record has before16 and calls from Scenario.__init__ to a \
             Scenario.start that returned"
                .to_owned(),
        );
    } else if !calls_reproduce && snapshot.is_none() {
        return Err("scenario: calls_reproduce false needs the state before advance".to_owned());
    }
    Ok(ScenarioRec {
        test,
        tags,
        calls,
        new_game,
        before16,
        after16,
        calls_reproduce,
        rules_checked,
        snapshot,
    })
}

fn error_rec(v: &Value, what: &'static str) -> Result<ErrorRec, String> {
    let mut o = Obj::new(v, what)?;
    let e = ErrorRec {
        class: as_str(o.req("type")?, what)?.to_owned(),
        message: as_str(o.req("message")?, what)?.to_owned(),
    };
    o.finish()?;
    Ok(e)
}

/// A required `"error"` key: `null` or `{"type", "message"}`.
fn nullable_error(o: &mut Obj<'_>, what: &'static str) -> Result<Option<ErrorRec>, String> {
    match o.req("error")? {
        Value::Null => Ok(None),
        e => error_rec(e, what).map(Some),
    }
}

/// An optional `"error"` key, present only when the call raised.
fn optional_error(o: &mut Obj<'_>, what: &'static str) -> Result<Option<ErrorRec>, String> {
    o.opt("error").map(|e| error_rec(e, what)).transpose()
}

/// `Decision.to_json()` (`engine/state.py:51-58`).
///
/// # Errors
/// A missing or malformed field.
pub fn pending_rec(v: &Value) -> Result<PendingRec, String> {
    let mut o = Obj::new(v, "pending")?;
    let p = PendingRec {
        player: seat(o.req("player")?, "pending.player")?,
        kind: decision_kind(o.req("kind")?, "pending.kind")?,
        options: actions(o.req("options")?, "pending.options")?,
        prompt: as_str(o.req("prompt")?, "pending.prompt")?.to_owned(),
        context: context(o.req("context")?, "pending.context")?,
    };
    o.finish()?;
    Ok(p)
}

fn apply_rec(v: &Value) -> Result<ApplyRec, String> {
    let mut o = Obj::new(v, "apply")?;
    o.req("t")?;
    let r = ApplyRec {
        action: action(o.req("action")?, "apply.action")?,
        check: as_bool(o.req("check")?, "apply.check")?,
        before16: hex16(&mut o, "before16", "apply.before16")?,
        after16: hex16(&mut o, "after16", "apply.after16")?,
        pending: match o.req("pending")? {
            Value::Null => None,
            p => Some(pending_rec(p)?),
        },
        error: nullable_error(&mut o, "apply.error")?,
    };
    o.finish()?;
    Ok(r)
}

fn call_rec(v: &Value) -> Result<CallRec, String> {
    let mut o = Obj::new(v, "call")?;
    o.req("t")?;
    let call = call_spec(&mut o, Site::Call)?;
    let result = o.req("result")?.clone();
    let after16 = match o.req("after16")? {
        Value::String(s) if s.is_empty() => String::new(),
        a => as_hex(a, 16, "call.after16")?.to_owned(),
    };
    let error = nullable_error(&mut o, "call.error")?;
    let new_game = o.opt("new_game").map(new_game_spec).transpose()?;
    o.finish()?;
    if error.is_some() && !result.is_null() {
        return Err("call: a call that raised has a null result".to_owned());
    }
    if new_game.is_some()
        && (call.func != "game.new_game"
            || !call.args.is_empty()
            || !call.kwargs.is_empty()
            || error.is_none())
    {
        return Err(
            "call: new_game is the spec of a game.new_game without arguments that raised"
                .to_owned(),
        );
    }
    Ok(CallRec {
        call,
        result,
        after16,
        error,
        new_game,
    })
}

fn state_rec(v: &Value) -> Result<StateRec, String> {
    let mut o = Obj::new(v, "state")?;
    o.req("t")?;
    let snapshot = snapshot_parts(&mut o)?;
    let derived = match as_str(o.req("derived")?, "state.derived")? {
        "none" => DerivedMark::None,
        "fresh" => DerivedMark::Fresh,
        "stale" => DerivedMark::Stale,
        other => return Err(format!("state.derived: unknown value {other:?}")),
    };
    let view_from = o.nullable("view_from").cloned();
    let view_key = o.nullable("view_key").cloned();
    if (derived == DerivedMark::Stale) != (view_from.is_some() && view_key.is_some()) {
        return Err(
            "state: view_from and view_key are present exactly when derived is stale".to_owned(),
        );
    }
    if let Some(from) = &view_from {
        canonical(from, "state.view_from")?;
    }
    o.finish()?;
    Ok(StateRec {
        snapshot,
        derived,
        view_from,
        view_key,
    })
}

fn probe(v: &Value) -> Result<ProbeRec, String> {
    let mut o = Obj::new(v, "probe")?;
    o.req("t")?;
    let call = call_spec(&mut o, Site::Probe)?;
    let result = o.req("result")?.clone();
    let error = optional_error(&mut o, "probe.error")?;
    o.finish()?;
    if error.is_some() && !result.is_null() {
        return Err("probe: a query that raised has a null result".to_owned());
    }
    Ok(ProbeRec {
        call,
        result,
        error,
    })
}

fn view_rec(v: &Value) -> Result<ViewRec, String> {
    let mut o = Obj::new(v, "view")?;
    o.req("t")?;
    let sig = o.req("sig")?.clone();
    let error = optional_error(&mut o, "view.error")?;
    o.finish()?;
    if sig.is_object() == error.is_some() || !(sig.is_object() || sig.is_null()) {
        return Err("view: sig is a signature object, or null with an error".to_owned());
    }
    Ok(ViewRec { sig, error })
}

fn override_rec(v: &Value) -> Result<OverrideRec, String> {
    let mut o = Obj::new(v, "override")?;
    o.req("t")?;
    let kind = match as_str(o.req("kind")?, "override.kind")? {
        "additions" => OverrideKind::Additions,
        "override" => OverrideKind::Override,
        "restore" => OverrideKind::Restore,
        other => return Err(format!("override.kind: unknown value {other:?}")),
    };
    let fragment = o.req("fragment")?;
    fragment_rec(fragment)?;
    let memo = o.opt("memo").map(memo_check).transpose()?;
    o.finish()?;
    if memo.is_some() != (kind == OverrideKind::Override) {
        return Err("override: memo is present exactly on kind \"override\"".to_owned());
    }
    Ok(OverrideRec {
        kind,
        fragment: fragment.clone(),
        memo,
    })
}

fn strings(v: &Value, what: &str) -> Result<(), String> {
    as_array(v, what)?
        .iter()
        .try_for_each(|s| as_str(s, what).map(|_| ()))
}

/// An IR fragment in the format of `oracle/export_ir.py` `build_fragment`: every key,
/// the version, the base fingerprint, and the arena's shape. `Registry::with_fragment` decodes
/// the rest when a replay applies it.
fn fragment_rec(v: &Value) -> Result<(), String> {
    let mut o = Obj::new(v, "override.fragment")?;
    let version: u32 = as_int(o.req("fragment_version")?, "fragment.fragment_version")?;
    if version != FRAGMENT_VERSION {
        return Err(format!(
            "fragment version {version}; this reader reads version {FRAGMENT_VERSION}"
        ));
    }
    as_hex(o.req("base_fingerprint")?, 64, "fragment.base_fingerprint")?;
    for key in [
        "printable_non_ascii",
        "strings",
        "var_names",
        "always_events",
    ] {
        strings(o.req(key)?, "fragment string table")?;
    }
    let nodes = as_array(o.req("nodes")?, "fragment.nodes")?;
    for n in nodes {
        as_str(n.get("_").unwrap_or(&Value::Null), "fragment.nodes[]._")?;
    }
    let cids = as_array(o.req("node_content_ids")?, "fragment.node_content_ids")?;
    if cids.len() != nodes.len() {
        return Err(format!(
            "fragment: {} nodes but {} node content ids",
            nodes.len(),
            cids.len()
        ));
    }
    for c in cids {
        as_hex(c, 16, "fragment.node_content_ids")?;
    }
    for p in as_array(o.req("programs")?, "fragment.programs")? {
        let mut po = Obj::new(p, "fragment.programs[]")?;
        as_str(po.req("label")?, "fragment.programs[].label")?;
        as_hex(po.req("content_id")?, 16, "fragment.programs[].content_id")?;
        for i in as_array(po.req("instrs")?, "fragment.programs[].instrs")? {
            as_int::<u32>(i, "fragment.programs[].instrs")?;
        }
        po.finish()?;
    }
    for key in [
        "abilities",
        "cards",
        "continuous",
        "filter_sets",
        "rule_mods",
        "aliases",
        "events",
        "printed_keywords",
    ] {
        as_array(o.req(key)?, "fragment table")?;
    }
    as_bool(
        o.req("hp_reduction_possible")?,
        "fragment.hp_reduction_possible",
    )?;
    o.finish()
}

fn memo_check(v: &Value) -> Result<MemoCheck, String> {
    let mut o = Obj::new(v, "override.memo")?;
    let empty = as_bool(o.req("empty")?, "memo.empty")?;
    let Value::Object(sizes) = o.req("sizes")? else {
        return Err("memo.sizes: expected an object".to_owned());
    };
    let sizes = sizes
        .iter()
        .map(|(k, n)| Ok((k.clone(), as_u64(n, "memo.sizes")?)))
        .collect::<Result<Vec<_>, String>>()?;
    let reads = as_array(o.req("reads")?, "memo.reads")?
        .iter()
        .map(|r| {
            let mut ro = Obj::new(r, "memo.reads[]")?;
            let read = MemoRead {
                table: as_str(ro.req("table")?, "memo.reads[].table")?.to_owned(),
                key: as_array(ro.req("key")?, "memo.reads[].key")?.to_vec(),
                value: ro.req("value")?.clone(),
            };
            ro.finish()?;
            Ok(read)
        })
        .collect::<Result<Vec<_>, String>>()?;
    o.finish()?;
    if empty != sizes.iter().all(|(_, n)| *n == 0) {
        return Err("memo: empty exactly when every table size is 0".to_owned());
    }
    Ok(MemoCheck {
        empty,
        sizes,
        reads,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decision_records_decode_every_fingerprint_field() {
        let line = r#"{"action_count":0,"chosen":["go_first",0,-1,-1],"context":[["min",1]],"digest16":"e193900bbf04a8f2","event_group":0,"i":0,"kind":"choose_first","n":2,"options":[["go_first",1,-1,-1],["go_first",0,-1,-1]],"player":1,"prompt":"choose Player One","rc":-1,"rng":"7912973102683061168","step":0,"t":"decision","turn":0,"version":120}"#;
        let Record::Decision(d) = parse_record(line).unwrap() else {
            panic!("not a decision")
        };
        assert_eq!(d.fingerprint.rng, 7_912_973_102_683_061_168);
        assert_eq!(d.fingerprint.version, 120);
        assert_eq!(d.context, vec![("min".to_owned(), 1)]);
        assert_eq!(d.chosen, Action::new(ActionKind::GoFirst, 0, -1, -1));
        assert!(parse_record(&line.replace("\"turn\":0", "\"turn\":0,\"extra\":1")).is_err());
        assert!(parse_record(&line.replace("\"n\":2", "\"n\":3")).is_err());
    }

    #[test]
    fn agent_blocks_keep_their_decision_log_entries() {
        let line = r#"{"action_count":0,"agent":{"alts":[[["go_first",0,-1,-1],"0x1.4000000000000p-1",24]],"k":1,"log":[{"chosen":"go_first(0)","value":0.625}],"rng_after":5,"seed":7},"chosen":["go_first",0,-1,-1],"context":[],"digest16":"e193900bbf04a8f2","event_group":0,"i":0,"kind":"choose_first","n":1,"options":[["go_first",0,-1,-1]],"player":1,"prompt":"choose Player One","rc":-1,"rng":"7912973102683061168","step":0,"t":"decision","turn":0,"version":120}"#;
        let Record::Decision(d) = parse_record(line).unwrap() else {
            panic!("not a decision")
        };
        let agent = d.agent.expect("agent block");
        assert_eq!(agent.log.len(), 1);
        assert_eq!(agent.log[0]["value"], 0.625);
        assert!(
            parse_record(&line.replace(r#""log":[{"chosen":"go_first(0)","value":0.625}],"#, ""))
                .is_err()
        );
        assert!(
            parse_record(&line.replace(
                r#""log":[{"chosen":"go_first(0)","value":0.625}]"#,
                r#""log":[1]"#
            ))
            .is_err()
        );
    }

    #[test]
    fn once_used_must_be_a_sorted_set() {
        let snap = |once: &str| {
            format!(
                r#"{{"continuous":{{}},"filter_sets":{{}},"i":0,"rules_checked":-1,"state":{{"once_used":{once}}},"t":"snapshot"}}"#
            )
        };
        assert!(parse_record(&snap("[[-8,-1,\"0123456789abcdef\",0,1,0],[3,4,5]]")).is_ok());
        let dup = parse_record(&snap("[[3,4,5],[3,4,5]]")).unwrap_err();
        assert!(dup.contains("duplicate once-per-turn key"), "{dup}");
        assert!(parse_record(&snap("[[4],[3]]")).is_err());
    }

    #[test]
    fn missing_trailing_newline_is_reported_at_the_last_line() {
        let head = r#"{"agent_seeds":[],"agents":[],"chooser":null,"decks":null,"generator":{"code_digest":"c","data_lock":"d","effect_offset":0,"git":"g","hash_seed":1,"ir_sha256":"absent","max_uid":0,"python":"3.15.1"},"id":"x","kind":"game","max_actions":20000,"options_mode":"full","preset":null,"seed":null,"t":"header","turn_limit":200,"v":1}"#;
        assert!(parse_traces(&format!("{head}\n")).is_ok());
        assert_eq!(
            parse_traces(head).map(|_| ()),
            Err((1, "the last line has no trailing newline".to_owned()))
        );
    }
}
