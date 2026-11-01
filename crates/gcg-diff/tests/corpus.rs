//! The M3 gate on the committed tier-1 corpus (PLAN §13 M3): the manifest matches every file,
//! every record decodes, every trace replays against its `MockEngine`, and every negative
//! control of §8.7 fires at its record (synthetic and real traces).

use std::path::PathBuf;

use gcg_diff::corpus::{self, CorpusArgs, EngineKind};
use gcg_diff::manifest::{self, Repo};

fn repo() -> Repo {
    Repo {
        root: PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../..")),
    }
}

fn args(engine: EngineKind) -> CorpusArgs {
    CorpusArgs {
        tier: 1,
        only: Vec::new(),
        file: None,
        engine,
        roundtrip: false,
        require_all: true,
        jobs: 4,
        report: None,
        junit: None,
    }
}

#[test]
fn tier1_manifest_matches_the_files() {
    let m = manifest::load(&repo(), 1).expect("manifest");
    let report = manifest::check(&m);
    assert!(report.errors.is_empty(), "{}", report.errors.join("\n"));
    assert!(report.files > 30);
}

#[test]
fn tier1_decodes_and_replays_against_mock_engines() {
    for engine in [EngineKind::None, EngineKind::Mock] {
        let report = corpus::run(&repo(), &args(engine));
        assert!(report.errors.is_empty(), "{}", report.errors.join("\n"));
        let failures: Vec<String> = report
            .results
            .iter()
            .filter_map(|r| r.outcome.clone().err())
            .collect();
        assert!(failures.is_empty(), "{}", failures.join("\n"));
        let sets: Vec<String> = corpus::per_set(&report)
            .into_iter()
            .map(|(s, _, _)| s)
            .collect();
        for set in [
            "baselines",
            "coverage",
            "full",
            "fuzz",
            "search",
            "tests",
            "vanilla",
            "views",
            "worlds",
        ] {
            assert!(
                sets.iter().any(|s| s == set),
                "set {set} missing from {sets:?}"
            );
        }
    }
}

#[test]
fn negative_controls_fire_at_their_records() {
    let lines =
        gcg_diff::selftest::run(&repo().root.join("port/corpus/v1/tier1")).expect("selftest");
    assert!(lines.len() >= 20, "{lines:?}");
}

fn to_py(v: &serde_json::Value) -> gcg_core::pyjson::Json {
    use gcg_core::pyjson::{Json, Key};
    match v {
        serde_json::Value::Null => Json::Null,
        serde_json::Value::Bool(b) => Json::Bool(*b),
        serde_json::Value::Number(n) => match (n.as_i64(), n.as_u64(), n.as_f64()) {
            (Some(i), _, _) => Json::Int(i128::from(i)),
            (None, Some(u), _) => Json::Int(i128::from(u)),
            (_, _, f) => Json::Float(f.expect("number")),
        },
        serde_json::Value::String(s) => Json::Str(s.clone()),
        serde_json::Value::Array(items) => Json::Array(items.iter().map(to_py).collect()),
        serde_json::Value::Object(map) => Json::Object(
            map.iter()
                .map(|(k, x)| (Key::Str(k.clone()), to_py(x)))
                .collect(),
        ),
    }
}

/// The baseline trace's actions and decision-log entries reproduce `tests/ai/test_baseline.py`'s
/// recorded digest (`sha256(json.dumps({"actions", "logs", "winner", "turn"}, sort_keys=True))`),
/// which pins the agent blocks' `log` capture.
#[test]
fn baseline_trace_reproduces_the_test_baseline_digest() {
    use serde_json::{Value, json};
    let path = repo()
        .root
        .join("port/corpus/v1/tier1/search/baseline.ndjson");
    let text = std::fs::read_to_string(path).expect("baseline trace");
    let (mut actions, mut logs) = (Vec::new(), [Vec::new(), Vec::new()]);
    let mut end = (Value::Null, Value::Null);
    for line in text.lines() {
        let r: Value = serde_json::from_str(line).expect("record");
        match r["t"].as_str() {
            Some("decision") => {
                actions.push(r["chosen"].clone());
                let seat = usize::try_from(r["player"].as_u64().expect("player")).expect("seat");
                logs[seat].extend(r["agent"]["log"].as_array().expect("log").iter().cloned());
            }
            Some("outcome") => end = (r["winner"].clone(), r["turn"].clone()),
            _ => {}
        }
    }
    let payload = json!({"actions": actions, "logs": logs, "winner": end.0, "turn": end.1});
    let opts = gcg_core::pyjson::DumpOptions {
        indent: None,
        sort_keys: true,
        compact: false,
        ensure_ascii: true,
    };
    let dumped = gcg_core::pyjson::dumps(&to_py(&payload), opts);
    assert_eq!(
        gcg_diff::json::sha256_hex(dumped.as_bytes()),
        "5cff789d2a8bb26cfef4db98b1ef1ef8f37c3bf63210114781d357bd215f3b96"
    );
    assert_eq!((end.0, end.1, actions.len()), (json!(1), json!(20), 115));
}
