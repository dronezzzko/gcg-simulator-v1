//! `gcg-diff selftest`: the negative controls of PLAN §8.7. Each control mutates a copy of a
//! trace and requires the runner to report the difference at the exact record:
//!
//! 1. one extra `touch` (`version` + 1 from a decision on);
//! 2. two options swapped;
//! 3. one RNG draw skipped (`rng` one SplitMix64 step back from a decision on);
//! 4. one card's zone changed in a snapshot;
//! 5. a float mean off by one ulp in a search log (an agent block's alternative, and an
//!    iteration record's `total_after`);
//! 6. a missing trailing newline;
//! 7. a once-per-turn key inserted twice (`once_used` set semantics, §4.3).
//!
//! The controls run on a synthetic trace and on real tier-1 traces against [`MockEngine`]s
//! of the unmutated originals, with a recorded agent oracle; with `--real-engine` (from M5)
//! the real tier-1 traces also run against the real engine ([`RealEngine`]) and the Rust
//! agents ([`RustAgents`]). Every unmutated line is also re-serialised and must equal the
//! exporter's bytes, which checks this crate's `json.dumps` writer.

use std::path::Path;

use serde_json::{Value, json};

use crate::agents::RustAgents;
use crate::engine::RealEngine;
use crate::json::{U53, as_u64, py_dumps, state_digest};
use crate::reader::parse_traces;
use crate::replay::{AgentObservation, AgentOracle, replay_with};
use crate::{DecisionRec, DiffOptions, EngineUnderTest, Header, MockEngine, Record, Trace};

/// SplitMix64's increment (`rng.py`): one draw adds it to the state.
const GOLDEN_GAMMA: u64 = 0x9E37_79B9_7F4A_7C15;

/// Replays another trace's agent blocks and search records: the Rust agent of a control run.
struct Recorded {
    by_decision: Vec<(u32, AgentObservation)>,
}

impl Recorded {
    fn new(trace: &Trace) -> Self {
        let mut by_decision: Vec<(u32, AgentObservation)> = Vec::new();
        for rec in &trace.records {
            match rec {
                Record::Decision(d) => {
                    if let Some(block) = &d.agent {
                        by_decision.push((
                            d.index,
                            AgentObservation {
                                action: d.chosen,
                                block: block.clone(),
                                iters: Vec::new(),
                                apply_traces: Vec::new(),
                            },
                        ));
                    }
                }
                Record::Iter(it) => {
                    if let Some((_, obs)) = by_decision
                        .iter_mut()
                        .rev()
                        .find(|(i, _)| *i == it.decision)
                    {
                        obs.iters.push(it.clone());
                    }
                }
                Record::ApplyTrace(a) => {
                    if let Some((_, obs)) =
                        by_decision.iter_mut().rev().find(|(i, _)| *i == a.decision)
                    {
                        obs.apply_traces.push(a.clone());
                    }
                }
                _ => {}
            }
        }
        Self { by_decision }
    }
}

impl AgentOracle for Recorded {
    fn decide(
        &mut self,
        _h: &Header,
        rec: &DecisionRec,
        _e: &dyn EngineUnderTest,
    ) -> Result<Option<AgentObservation>, String> {
        Ok(self
            .by_decision
            .iter()
            .find(|(i, _)| *i == rec.index)
            .map(|(_, obs)| obs.clone()))
    }
}

/// One control: a name, the mutated lines, and where the report must point.
struct Control {
    name: &'static str,
    lines: Vec<Value>,
    /// 1-based line of the mutated record.
    line: usize,
    /// The reported field must start with this.
    field: &'static str,
    newline: bool,
}

fn serialise(lines: &[Value], newline: bool) -> String {
    let mut text: String = lines.iter().map(|l| py_dumps(l) + "\n").collect();
    if !newline {
        text.pop();
    }
    text
}

fn set_u64(v: &mut Value, key: &str, x: u64) {
    v[key] = if x < U53 {
        json!(x)
    } else {
        json!(x.to_string())
    };
}

fn is(v: &Value, t: &str) -> bool {
    v.get("t").and_then(Value::as_str) == Some(t)
}

/// Line index (0-based) of every record of type `t`.
fn indices(lines: &[Value], t: &str) -> Vec<usize> {
    (0..lines.len()).filter(|&i| is(&lines[i], t)).collect()
}

fn next_ulp(hex: &str) -> Result<String, String> {
    let x = crate::floathex::from_hex(hex)?;
    Ok(crate::floathex::to_hex(f64::from_bits(x.to_bits() + 1)))
}

/// The controls for one trace (its lines, the header first).
fn controls(lines: &[Value]) -> Result<Vec<Control>, String> {
    let decisions = indices(lines, "decision");
    let snapshots: Vec<usize> = indices(lines, "snapshot");
    if decisions.len() < 4 || snapshots.len() < 2 {
        return Err("a control trace needs at least 4 decisions and 2 snapshots".to_owned());
    }
    let mid = decisions[decisions.len() / 2];
    let mut out = Vec::new();

    let mut l = lines.to_vec();
    for &i in decisions.iter().filter(|&&i| i >= mid) {
        let v = l[i]["version"].as_u64().ok_or("version")? + 1;
        l[i]["version"] = json!(v);
    }
    out.push(Control {
        name: "one extra touch",
        lines: l,
        line: mid + 1,
        field: "version",
        newline: true,
    });

    let swap = *decisions
        .iter()
        .find(|&&i| {
            lines[i]["options"]
                .as_array()
                .is_some_and(|o| o.len() >= 2 && o[0] != o[1])
        })
        .ok_or("no decision with two different options")?;
    let mut l = lines.to_vec();
    if let Some(opts) = l[swap]["options"].as_array_mut() {
        opts.swap(0, 1);
    }
    out.push(Control {
        name: "two options swapped",
        lines: l,
        line: swap + 1,
        field: "options",
        newline: true,
    });

    let mut l = lines.to_vec();
    for &i in decisions.iter().filter(|&&i| i >= mid) {
        let rng = as_u64(&l[i]["rng"], "rng")?.wrapping_sub(GOLDEN_GAMMA);
        set_u64(&mut l[i], "rng", rng);
    }
    out.push(Control {
        name: "one RNG draw skipped",
        lines: l,
        line: mid + 1,
        field: "rng",
        newline: true,
    });

    let snap = snapshots[1];
    let mut l = lines.to_vec();
    let zone = l[snap]["state"]["cards"][0][3]
        .as_u64()
        .ok_or("cards[0][3]")?;
    l[snap]["state"]["cards"][0][3] = json!((zone + 1) % 12);
    out.push(Control {
        name: "a card's zone changed in a snapshot",
        lines: l,
        line: snap + 1,
        field: "state.cards[0][3]",
        newline: true,
    });

    if let Some(&i) = decisions.iter().find(|&&i| {
        lines[i]["agent"]["alts"]
            .as_array()
            .is_some_and(|a| !a.is_empty())
    }) {
        let mut l = lines.to_vec();
        let hex = l[i]["agent"]["alts"][0][1]
            .as_str()
            .ok_or("alts value")?
            .to_owned();
        l[i]["agent"]["alts"][0][1] = json!(next_ulp(&hex)?);
        out.push(Control {
            name: "a float mean off by one ulp (agent block)",
            lines: l,
            line: i + 1,
            field: "agent.alts[0].value",
            newline: true,
        });
    }
    if let Some(&i) = indices(lines, "iter").first() {
        let mut l = lines.to_vec();
        let hex = l[i]["total_after"]
            .as_str()
            .ok_or("total_after")?
            .to_owned();
        l[i]["total_after"] = json!(next_ulp(&hex)?);
        out.push(Control {
            name: "a float mean off by one ulp (iteration record)",
            lines: l,
            line: i + 1,
            field: "iter[",
            newline: true,
        });
    }

    out.push(Control {
        name: "a missing trailing newline",
        lines: lines.to_vec(),
        line: lines.len(),
        field: "",
        newline: false,
    });

    let mut l = lines.to_vec();
    let key = json!([-7, -1, "0123456789abcdef", 0, 0, 0]);
    l[snapshots[0]]["state"]["once_used"] = json!([key.clone(), key]);
    out.push(Control {
        name: "a once-per-turn key inserted twice",
        lines: l,
        line: snapshots[0] + 1,
        field: "",
        newline: true,
    });
    Ok(out)
}

/// The engine side of a control run.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Side {
    /// [`MockEngine`] of the unmutated original, with its recorded agent blocks.
    Mock,
    /// The real engine and the Rust agents.
    Real,
}

#[expect(
    clippy::result_large_err,
    reason = "replay_with returns Divergence by value (frozen in docs/dev/contracts.md, gcg-diff)"
)]
fn replay_side(
    side: Side,
    original: &Trace,
    t: &Trace,
    opts: &DiffOptions,
) -> Result<crate::ReplayStats, crate::Divergence> {
    match side {
        Side::Mock => replay_with(
            t,
            opts,
            &mut MockEngine::new(original),
            &mut Recorded::new(original),
        ),
        Side::Real => replay_with(
            t,
            opts,
            &mut RealEngine::for_trace(&t.header),
            &mut RustAgents::for_trace(t),
        ),
    }
}

/// Run every control on one trace given as lines; returns one report line per control.
///
/// # Errors
/// A control that did not fire at its record, or an original that does not replay cleanly.
pub fn run_controls(label: &str, original_text: &str) -> Result<Vec<String>, String> {
    run_controls_on(Side::Mock, label, original_text)
}

/// [`run_controls`] against `side`.
///
/// # Errors
/// A control that did not fire at its record, or an original that does not replay cleanly.
pub fn run_controls_on(
    side: Side,
    label: &str,
    original_text: &str,
) -> Result<Vec<String>, String> {
    let lines: Vec<Value> = original_text
        .lines()
        .map(|l| serde_json::from_str(l).map_err(|e| format!("{label}: {e}")))
        .collect::<Result<_, _>>()?;
    for (i, (text, v)) in original_text.lines().zip(&lines).enumerate() {
        if py_dumps(v) != text {
            return Err(format!(
                "{label}:{}: re-serialising the record changes its bytes",
                i + 1
            ));
        }
    }
    let originals = parse_traces(original_text).map_err(|(n, e)| format!("{label}:{n}: {e}"))?;
    let [original] = originals.as_slice() else {
        return Err(format!("{label}: a control trace is one trace"));
    };
    let opts = DiffOptions::default();
    replay_side(side, original, original, &opts)
        .map_err(|d| format!("{label}: the unmutated trace diverges:\n{d}"))?;
    let mut report = Vec::new();
    for c in controls(&lines)? {
        let text = serialise(&c.lines, c.newline);
        let fired = match parse_traces(&text) {
            Err((n, e)) => {
                if n == c.line && c.field.is_empty() {
                    Ok(format!("rejected at line {n}: {e}"))
                } else {
                    Err(format!(
                        "rejected at line {n} ({e}), expected line {} field {:?}",
                        c.line, c.field
                    ))
                }
            }
            Ok(mutated) => {
                let [m] = mutated.as_slice() else {
                    return Err(format!("{label}: {}: the mutation split the trace", c.name));
                };
                match replay_side(side, original, m, &opts) {
                    Ok(_) => Err("no divergence reported".to_owned()),
                    Err(d)
                        if d.line == c.line
                            && !c.field.is_empty()
                            && d.field.starts_with(c.field) =>
                    {
                        Ok(format!("diverged at line {} field {}", d.line, d.field))
                    }
                    Err(d) => Err(format!(
                        "reported line {} field {}, expected line {} field {:?}\n{d}",
                        d.line, d.field, c.line, c.field
                    )),
                }
            }
        };
        match fired {
            Ok(what) => report.push(format!(
                "ok    {label}{}: {}: {what}",
                if side == Side::Real {
                    " (real engine)"
                } else {
                    ""
                },
                c.name
            )),
            Err(e) => return Err(format!("FAIL  {label}: {}: {e}", c.name)),
        }
    }
    Ok(report)
}

/// A synthetic search trace with snapshots, an agent block and iteration records.
#[must_use]
pub fn synthetic_trace() -> String {
    let deck = json!({"main": ["GD01-060", "GD01-060"], "res": ["R-001"]});
    let generator = json!({"code_digest": "c", "data_lock": "d", "effect_offset": 0, "git": "g", "hash_seed": 1, "ir_sha256": "absent", "max_uid": 2, "python": "3.15.1"});
    let header = json!({"t": "header", "v": 1, "id": "selftest/synthetic", "kind": "search", "generator": generator,
        "decks": [deck.clone(), deck], "seed": 7, "chooser": null, "turn_limit": 200, "max_actions": 20000,
        "agents": ["mcts-fast", "mcts-fast"], "agent_seeds": [1, 2], "preset": "fast", "options_mode": "full"});
    let state = |turn: u64, zone: u64, winner: Value| {
        json!({"cards": [[0, 208, 0, zone, 0, 0, -1, 0, 1, 3], [1, 208, 1, 3, 0, 0, -1, 0, 2, 3], [2, 913, 0, 4, 0, 0, -1, 0, 1, 0]],
               "once_used": [[-8, -1, "fedcba9876543210", 0, 1, 0]], "turn": turn, "winner": winner,
               "end_reason": if winner.is_null() { Value::Null } else { json!("battle_damage") }, "rng": "12345678901234567890"})
    };
    let snap = |i: u64, s: Value| json!({"t": "snapshot", "i": i, "state": s, "continuous": {}, "filter_sets": {}, "rules_checked": 5});
    let mut rng = 0x0123_4567_89AB_CDEF_u64;
    let mut lines = vec![header, snap(0, state(1, 3, Value::Null))];
    let options = [
        json!(["end_main", -1, -1, -1]),
        json!(["play_unit", 2, -1, 0]),
        json!(["attack", 0, -2, -1]),
    ];
    for i in 0..4_u64 {
        rng = rng.wrapping_add(GOLDEN_GAMMA);
        let mut d = json!({"t": "decision", "i": i, "player": i % 2, "kind": "main", "prompt": "main phase", "context": [],
            "options": options, "n": 3, "chosen": options[usize::from(i == 1)], "version": 40 + 3 * i, "rc": 39 + 3 * i,
            "action_count": i, "event_group": i, "turn": 1 + i / 2, "step": 40, "digest16": format!("{:016x}", i + 1)});
        set_u64(&mut d, "rng", rng);
        if i == 1 {
            d["agent"] = json!({"k": 1, "seed": 99, "rng_after": 100, "iters": 2, "budget": 16, "cap": 16,
                "roots": [["end_main", -1, -1, -1], ["play_unit", 16_777_424, -1, 0]],
                "alts": [[["end_main", -1, -1, -1], "0x1.b00be33f8f4adp-2", 1], [["play_unit", 2, -1, 0], "0x1.b577bf6413a4fp-2", 1]],
                "log": []});
        }
        lines.push(d);
        if i == 1 {
            for n in 0..2_u64 {
                lines.push(json!({"t": "iter", "decision": 1, "n": n, "child": n, "k": 0, "fresh": true, "path_len": 1,
                    "applies_walk": 1, "applies_rollout": 7, "leaf": "0x1.25a572c70d886p-1", "terminal": false,
                    "total_before": "0x0.0p+0", "total_after": "0x1.25a572c70d886p-1", "world16": "2f7d724b093357e9"}));
            }
        }
        if i == 1 {
            lines.push(snap(2, state(1, 2, Value::Null)));
        }
    }
    let end = state(3, 2, json!(0));
    lines.push(snap(4, end.clone()));
    let digest = state_digest(&end).unwrap_or_default();
    lines.push(json!({"t": "outcome", "winner": 0, "end_reason": "battle_damage", "turn": 3, "actions": 4, "final_digest": digest, "state": end}));
    serialise(&lines, true)
}

/// The tier-1 traces the real-trace controls use: one search trace with level-2 records and
/// one fuzz trace.
pub const REAL_TRACES: [(&str, &str); 2] = [
    (
        "search/acceptance.ndjson",
        "search/acceptance/standard/4242",
    ),
    ("fuzz/s1-g0000-0019.ndjson", "fuzz/s1/g0000"),
];

/// The text of trace `id` in `file` (a multi-trace file), with its own lines only.
///
/// # Errors
/// An unreadable file or a missing trace.
pub fn extract_trace(file: &Path, id: &str) -> Result<String, String> {
    let text = std::fs::read_to_string(file).map_err(|e| format!("{}: {e}", file.display()))?;
    let mut out = String::new();
    let mut inside = false;
    for line in text.lines() {
        if line.contains("\"t\":\"header\"") {
            if inside {
                break;
            }
            inside = line.contains(&format!("\"id\":{}", py_dumps(&json!(id))));
        }
        if inside {
            out.push_str(line);
            out.push('\n');
        }
    }
    if out.is_empty() {
        Err(format!("{}: no trace {id}", file.display()))
    } else {
        Ok(out)
    }
}

/// All controls: the synthetic trace, then the real tier-1 traces under `tier1`.
///
/// # Errors
/// The first control that did not fire, or a missing corpus trace.
pub fn run(tier1: &Path) -> Result<Vec<String>, String> {
    let mut report = run_controls("synthetic", &synthetic_trace())?;
    for (file, id) in REAL_TRACES {
        let text = extract_trace(&tier1.join(file), id)?;
        report.extend(run_controls(id, &text)?);
    }
    Ok(report)
}

/// [`run`], then the real tier-1 traces' controls against the real engine and the Rust
/// agents (`gcg-diff selftest --real-engine`, the gate item from M5).
///
/// # Errors
/// The first control that did not fire, or a missing corpus trace.
pub fn run_real(tier1: &Path) -> Result<Vec<String>, String> {
    let mut report = run(tier1)?;
    for (file, id) in REAL_TRACES {
        let text = extract_trace(&tier1.join(file), id)?;
        report.extend(run_controls_on(Side::Real, id, &text)?);
    }
    Ok(report)
}
