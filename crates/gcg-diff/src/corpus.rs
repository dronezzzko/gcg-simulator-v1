//! `gcg-diff corpus` (PLAN §8.7): check a corpus tier against its manifest and the committed IR,
//! decode every trace, and replay it.
//!
//! Engines: the real one (the default: [`RealEngine`] over `gcg-engine`, with the agents of
//! [`crate::agents`] for agent blocks), `--parse-only` (decode and hash checks, the M3 gate)
//! and `--mock` (replay every trace against a [`MockEngine`] built from the trace itself:
//! proves that each trace is internally consistent and that the replay handles every record
//! kind). `--roundtrip` loads every snapshot into the engine and re-serialises it (the M5.1
//! gate).

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use crate::agents::RustAgents;
use crate::engine::RealEngine;
use crate::manifest::{self, Repo};
use crate::mock::MockEngine;
use crate::reader::read_trace;
use crate::replay::{replay_trace, replay_with};
use crate::{DiffOptions, Divergence, ReplayStats, Trace};

/// The engine a corpus run replays against.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EngineKind {
    /// Decode only (`--parse-only`).
    None,
    /// [`MockEngine`] of each trace (`--mock`).
    Mock,
    /// [`RealEngine`] (the default).
    Real,
}

/// `gcg-diff corpus` options.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct CorpusArgs {
    pub tier: u8,
    /// `--only set[:filter]`: a set name and an optional substring of the trace id.
    pub only: Vec<(String, Option<String>)>,
    /// `--file TRACE.ndjson`: one file outside the manifest.
    pub file: Option<PathBuf>,
    pub engine: EngineKind,
    pub roundtrip: bool,
    /// `--paranoid`: the replay runs on a `paranoid` build of the engine (W-49, W-124).
    pub paranoid: bool,
    pub require_all: bool,
    pub jobs: usize,
    pub report: Option<PathBuf>,
    pub junit: Option<PathBuf>,
    /// Record the definitions of every card in a state the real engine reached
    /// ([`TraceResult::seen_defs`]; `gcg-diff coverage`).
    pub census: bool,
}

/// The outcome of one trace.
#[derive(Clone, PartialEq, Debug)]
pub struct TraceResult {
    pub set: String,
    pub id: String,
    pub file: String,
    pub outcome: Result<ReplayStats, String>,
    /// With [`CorpusArgs::census`] and the real engine: the def ids of the cards of every state
    /// the replay observed, sorted.
    pub seen_defs: Vec<u16>,
}

/// What a corpus run found.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct CorpusReport {
    /// Integrity errors (manifest, IR, decoding): always fatal.
    pub errors: Vec<String>,
    pub results: Vec<TraceResult>,
}

impl CorpusReport {
    #[must_use]
    pub fn failed(&self) -> usize {
        self.results.iter().filter(|r| r.outcome.is_err()).count()
    }
}

fn selected(args: &CorpusArgs, set: &str, id: &str) -> bool {
    args.only.is_empty()
        || args
            .only
            .iter()
            .any(|(s, f)| s == set && f.as_ref().is_none_or(|f| id.contains(f.as_str())))
}

/// Run the corpus check.
#[must_use]
pub fn run(repo: &Repo, args: &CorpusArgs) -> CorpusReport {
    let mut report = CorpusReport::default();
    let ir = match repo.ir_sha256() {
        Ok(h) => h,
        Err(e) => {
            report.errors.push(e);
            return report;
        }
    };
    let mut files: Vec<(String, String, PathBuf)> = Vec::new();
    if let Some(f) = &args.file {
        files.push(("file".to_owned(), f.display().to_string(), f.clone()));
    } else {
        let m = match manifest::load(repo, args.tier) {
            Ok(m) => m,
            Err(e) => {
                report.errors.push(e);
                return report;
            }
        };
        let check = manifest::check(&m);
        report.errors.extend(check.errors);
        for e in &m.entries {
            if e.records.is_some()
                && (args.only.is_empty() || args.only.iter().any(|(s, _)| *s == e.set))
            {
                files.push((e.set.clone(), e.path.clone(), m.root.join(&e.path)));
            }
        }
    }
    let jobs = args.jobs.max(1);
    let mut outputs: Vec<Option<Result<Vec<TraceResult>, String>>> = vec![None; files.len()];
    std::thread::scope(|scope| {
        let chunks: Vec<Vec<usize>> = (0..jobs)
            .map(|j| (j..files.len()).step_by(jobs).collect())
            .collect();
        let handles: Vec<_> = chunks
            .into_iter()
            .map(|idx| {
                let files = &files;
                let ir = ir.as_str();
                scope.spawn(move || {
                    idx.into_iter()
                        .map(|i| {
                            let (set, rel, path) = &files[i];
                            (i, check_file(set, rel, path, ir, args))
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        for h in handles {
            match h.join() {
                Ok(done) => {
                    for (i, out) in done {
                        outputs[i] = Some(out);
                    }
                }
                Err(_) => report.errors.push("a corpus worker panicked".to_owned()),
            }
        }
    });
    for out in outputs.into_iter().flatten() {
        match out {
            Ok(results) => report.results.extend(results),
            Err(e) => report.errors.push(e),
        }
    }
    report
}

fn check_file(
    set: &str,
    rel: &str,
    path: &Path,
    ir: &str,
    args: &CorpusArgs,
) -> Result<Vec<TraceResult>, String> {
    let traces = read_trace(path).map_err(|e| e.to_string())?;
    let mut line = 1;
    let mut out = Vec::new();
    for t in &traces {
        let start = line;
        line += 1 + t.records.len();
        if t.header.generator.ir_sha256 != ir {
            return Err(format!(
                "{rel}:{start}: trace {} was generated against IR {} but the committed IR is {ir}: \
                 regenerate the corpus (uv run python oracle/traces.py --write --tier {})",
                t.header.id, t.header.generator.ir_sha256, args.tier
            ));
        }
        let set_of = if set == "file" {
            t.header.id.split('/').next().unwrap_or("file")
        } else {
            set
        };
        if !selected(args, set_of, &t.header.id) {
            continue;
        }
        let (outcome, seen_defs) = replay_one(t, args);
        let outcome = outcome.map_err(|mut d| {
            d.line += start - 1;
            format!("{rel}: {d}")
        });
        out.push(TraceResult {
            set: set_of.to_owned(),
            id: t.header.id.clone(),
            file: rel.to_owned(),
            outcome,
            seen_defs,
        });
    }
    Ok(out)
}

fn replay_one(t: &Trace, args: &CorpusArgs) -> (Result<ReplayStats, Divergence>, Vec<u16>) {
    let opts = DiffOptions {
        parse_only: args.engine == EngineKind::None,
        roundtrip: args.roundtrip,
        paranoid: args.paranoid,
    };
    if args.engine == EngineKind::Real {
        let mut engine = RealEngine::for_trace(&t.header);
        if args.census {
            engine.record_cards();
        }
        let out = replay_with(t, &opts, &mut engine, &mut RustAgents::for_trace(t));
        return (out, engine.seen_defs());
    }
    if args.engine == EngineKind::Mock && !MockEngine::mockable(t) {
        return (
            Err(mock_divergence(
                t,
                "MockEngine needs full option lists (options_mode \"full\")".to_owned(),
            )),
            Vec::new(),
        );
    }
    let mut mock = MockEngine::new(t);
    (replay_trace(t, &opts, &mut mock), Vec::new())
}

fn mock_divergence(t: &Trace, e: String) -> Divergence {
    Divergence {
        trace_id: t.header.id.clone(),
        line: 1,
        decision: None,
        turn: None,
        step: None,
        player: None,
        kind: None,
        field: "engine".to_owned(),
        python: "a mockable trace".to_owned(),
        rust: e,
        last_action: None,
        note: None,
    }
}

/// Per-set counts in a stable order: `(set, traces, passed)`.
#[must_use]
pub fn per_set(report: &CorpusReport) -> Vec<(String, usize, usize)> {
    let mut sets: Vec<String> = report.results.iter().map(|r| r.set.clone()).collect();
    sets.sort();
    sets.dedup();
    sets.into_iter()
        .map(|s| {
            let of: Vec<&TraceResult> = report.results.iter().filter(|r| r.set == s).collect();
            let passed = of.iter().filter(|r| r.outcome.is_ok()).count();
            (s, of.len(), passed)
        })
        .collect()
}

/// What the passing traces of each set covered, summed, in [`per_set`]'s order.
#[must_use]
pub fn set_stats(report: &CorpusReport) -> Vec<(String, ReplayStats)> {
    per_set(report)
        .into_iter()
        .map(|(s, _, _)| {
            let mut sum = ReplayStats::default();
            for st in report
                .results
                .iter()
                .filter(|r| r.set == s)
                .filter_map(|r| r.outcome.as_ref().ok())
            {
                sum.records += st.records;
                sum.decisions += st.decisions;
                sum.snapshots += st.snapshots;
                sum.applies += st.applies;
                sum.calls += st.calls;
                sum.probes += st.probes;
                sum.agent_decisions += st.agent_decisions;
            }
            (s, sum)
        })
        .collect()
}

/// The `--report` JSON document.
#[must_use]
pub fn report_json(report: &CorpusReport) -> serde_json::Value {
    let sets: serde_json::Map<String, serde_json::Value> = per_set(report)
        .into_iter()
        .map(|(s, n, ok)| {
            (
                s,
                serde_json::json!({"traces": n, "passed": ok, "failed": n - ok}),
            )
        })
        .collect();
    let failures: Vec<serde_json::Value> = report
        .results
        .iter()
        .filter_map(|r| {
            r.outcome
                .as_ref()
                .err()
                .map(|e| serde_json::json!({"id": r.id, "file": r.file, "report": e}))
        })
        .collect();
    serde_json::json!({"errors": report.errors, "sets": sets, "failures": failures})
}

fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c if c == '\n' || c == '\t' || (c >= ' ' && c != '\u{7f}') => out.push(c),
            _ => out.push('?'),
        }
    }
    out
}

/// The `--junit` document: one test case per trace, named by trace id (for a per-test trace,
/// the pytest node id), class name the set.
#[must_use]
pub fn junit_xml(report: &CorpusReport) -> String {
    let failed = report.failed();
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n");
    let _ = writeln!(
        out,
        "<testsuites><testsuite name=\"gcg-diff corpus\" tests=\"{}\" failures=\"{failed}\" errors=\"{}\">",
        report.results.len(),
        report.errors.len()
    );
    for r in &report.results {
        let _ = write!(
            out,
            "<testcase classname=\"{}\" name=\"{}\"",
            xml_escape(&r.set),
            xml_escape(&r.id)
        );
        match &r.outcome {
            Ok(_) => out.push_str("/>\n"),
            Err(e) => {
                let _ = writeln!(
                    out,
                    "><failure message=\"diverged\">{}</failure></testcase>",
                    xml_escape(e)
                );
            }
        }
    }
    out.push_str("</testsuite></testsuites>\n");
    out
}
