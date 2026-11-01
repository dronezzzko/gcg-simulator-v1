//! The M5.1 gate on the real engine (PLAN §8.7 `--roundtrip`, §13 M5.1): every snapshot of
//! the committed tier-1 corpus (`snapshot` records, `state` records and their view origins,
//! the snapshots of `scenario` records) loads with `canon::from_canonical`, re-serialises to
//! Python's canonical bytes, and has the digest and `_rules_checked` the trace records; and a
//! snapshot changed in one field, or a recorded digest changed, is reported at its record.

use std::path::PathBuf;

use gcg_diff::corpus::{self, CorpusArgs, EngineKind};
use gcg_diff::engine::RealEngine;
use gcg_diff::manifest::Repo;
use gcg_diff::reader::parse_traces;
use gcg_diff::replay::replay_with;
use gcg_diff::{DiffOptions, Divergence, Trace};
use serde_json::Value;

fn repo() -> Repo {
    Repo {
        root: PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../..")),
    }
}

fn roundtrip_args() -> CorpusArgs {
    CorpusArgs {
        tier: 1,
        only: Vec::new(),
        file: None,
        engine: EngineKind::Real,
        roundtrip: true,
        paranoid: false,
        require_all: true,
        jobs: 4,
        report: None,
        junit: None,
        census: false,
    }
}

#[test]
fn every_tier1_snapshot_round_trips_through_the_engine() {
    let report = corpus::run(&repo(), &roundtrip_args());
    assert!(report.errors.is_empty(), "{}", report.errors.join("\n"));
    let failures: Vec<String> = report
        .results
        .iter()
        .filter_map(|r| r.outcome.clone().err())
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    let snapshots: u64 = corpus::set_stats(&report)
        .iter()
        .map(|(_, s)| s.snapshots)
        .sum();
    // 1557 `snapshot` records, 603 `state` records and 6 `scenario` snapshots.
    assert_eq!(snapshots, 2166);
    assert_eq!(report.results.len(), 3804);
}

fn first_trace(rel: &str) -> String {
    let text = std::fs::read_to_string(repo().root.join(rel)).expect("trace file");
    let mut out = String::new();
    for (i, l) in text.lines().enumerate() {
        if i > 0 && l.contains("\"t\":\"header\"") {
            break;
        }
        out.push_str(l);
        out.push('\n');
    }
    out
}

fn roundtrip(text: &str) -> Result<u64, Box<Divergence>> {
    let traces: Vec<Trace> = parse_traces(text).expect("decodes");
    let t = &traces[0];
    let opts = DiffOptions {
        roundtrip: true,
        ..DiffOptions::default()
    };
    let mut engine = RealEngine::for_trace(&t.header);
    replay_with(
        t,
        &opts,
        &mut engine,
        &mut gcg_diff::agents::RustAgents::new(),
    )
    .map(|s| s.snapshots)
    .map_err(Box::new)
}

/// `text` with the JSON of line `k` (0-based) rewritten by `f`.
fn mutate(text: &str, k: usize, f: impl Fn(&mut Value)) -> String {
    text.lines()
        .enumerate()
        .map(|(i, l)| {
            if i == k {
                let mut v: Value = serde_json::from_str(l).expect("JSON");
                f(&mut v);
                v.to_string()
            } else {
                l.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

fn line_of(text: &str, tag: &str) -> usize {
    text.lines()
        .position(|l| l.contains(&format!("\"t\":\"{tag}\"")))
        .expect("a record of that kind")
}

#[test]
fn a_changed_snapshot_or_digest_diverges_at_its_record() {
    let original = first_trace("port/corpus/v1/tier1/full/full.ndjson");
    assert!(roundtrip(&original).expect("the original round-trips") > 1);
    let snap = line_of(&original, "snapshot");

    // A card's zone: the bytes differ from the snapshot's own digest, which the next decision
    // records.
    let zone = mutate(&original, snap, |v| {
        let z = &mut v["state"]["cards"][0][3];
        *z = Value::from(z.as_i64().expect("zone") ^ 1);
    });
    let d = roundtrip(&zone).expect_err("diverges");
    assert_eq!(d.line, snap + 1, "{d}");

    // `_rules_checked`, which the canonical form does not hold.
    let rc = mutate(&original, snap, |v| {
        v["rules_checked"] = Value::from(v["rules_checked"].as_i64().expect("rc") + 7);
    });
    let d = roundtrip(&rc).expect_err("diverges");
    assert_eq!(
        (d.line, d.field.as_str()),
        (snap + 1, "snapshot rules_checked"),
        "{d}"
    );

    // The recorded digest of the decision the snapshot belongs to.
    let dec = line_of(&original, "decision");
    let digest = mutate(&original, dec, |v| {
        v["digest16"] = Value::from("0123456789abcdef");
    });
    let d = roundtrip(&digest).expect_err("diverges");
    assert_eq!(
        (d.line, d.field.as_str()),
        (snap + 1, "snapshot digest16"),
        "{d}"
    );
}

#[test]
fn per_test_state_records_round_trip_with_their_apply_digest() {
    let dir = repo().root.join("port/corpus/v1/tier1/tests/rules");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("tests dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "ndjson"))
        .collect();
    files.sort();
    let text = std::fs::read_to_string(&files[0]).expect("a per-test file");
    let traces = parse_traces(&text).expect("decodes");
    let with_state = traces
        .iter()
        .find(|t| {
            t.records
                .iter()
                .any(|r| matches!(r, gcg_diff::Record::State(_)))
        })
        .expect("a test with a state record");
    let mut engine = RealEngine::for_trace(&with_state.header);
    let stats = replay_with(
        with_state,
        &DiffOptions {
            roundtrip: true,
            ..DiffOptions::default()
        },
        &mut engine,
        &mut gcg_diff::agents::RustAgents::new(),
    )
    .expect("round-trips");
    assert!(stats.snapshots >= 1);
}
