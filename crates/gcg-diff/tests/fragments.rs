//! The per-test traces' IR fragments against their consumer (PLAN §3.4, §8.2): every
//! `override` record of the committed `tests` set applies to the committed IR with
//! `gcg_effects::Registry::with_fragment`, the registry numbers the fragment's nodes and
//! programs as `oracle/pytest_trace.py` did (Python's content ids at the ids the trace
//! uses), and every `{"$": NodeId}` and `Program` reference of the trace names a node or
//! program of the registry in force at its record.

use std::fs;
use std::path::{Path, PathBuf};

use gcg_data::CardDb;
use gcg_diff::{CallSpec, EngineRef, Record, TraceArg, read_trace};
use gcg_effects::{ContentId, NodeId, ProgramId, Registry};
use serde_json::Value;

fn root() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

fn trace_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)
        .expect("corpus directory")
        .map(|e| e.expect("entry").path())
        .collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            trace_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "ndjson") {
            out.push(p);
        }
    }
}

fn len(v: &Value, key: &str) -> u32 {
    u32::try_from(v[key].as_array().expect(key).len()).expect("fits")
}

fn cid(v: &Value) -> ContentId {
    ContentId::from_hex(v.as_str().expect("content id")).expect("16 hex digits")
}

/// The node and program counts of the registry in force: the base's, plus the fragment's.
#[derive(Clone, Copy)]
struct Bounds {
    nodes: u32,
    programs: u32,
}

fn check_arg(arg: &TraceArg, b: Bounds, at: &str) -> (u32, u32) {
    match arg {
        TraceArg::Node(n) => {
            assert!(*n < b.nodes, "{at}: node {n} of {}", b.nodes);
            (1, 0)
        }
        TraceArg::Ref(EngineRef::Program(p)) => {
            assert!(
                u32::from(p.0) < b.programs,
                "{at}: program {} of {}",
                p.0,
                b.programs
            );
            (0, 1)
        }
        TraceArg::List(items) => items.iter().fold((0, 0), |acc, x| {
            let (n, p) = check_arg(x, b, at);
            (acc.0 + n, acc.1 + p)
        }),
        _ => (0, 0),
    }
}

fn check_spec(spec: &CallSpec, b: Bounds, at: &str) -> (u32, u32) {
    let mut all = spec.args.clone();
    all.extend(spec.kwargs.iter().map(|(_, v)| v.clone()));
    check_arg(&TraceArg::List(all), b, &format!("{at} {}", spec.func))
}

fn check_result(v: &Value, b: Bounds, at: &str) -> u32 {
    match v {
        Value::Object(m) if m.len() == 1 && m.contains_key("$") => {
            let n = m["$"].as_u64().expect("node id");
            assert!(
                n < u64::from(b.nodes),
                "{at}: result node {n} of {}",
                b.nodes
            );
            1
        }
        Value::Array(items) => items.iter().map(|x| check_result(x, b, at)).sum(),
        _ => 0,
    }
}

#[test]
fn per_test_fragments_apply_and_their_references_resolve() {
    let ir_text = fs::read_to_string(root().join("crates/gcg-effects/ir/registry.json"))
        .expect("the committed IR");
    let base = Registry::from_ir(&ir_text, CardDb::embedded()).expect("the IR loads");
    let ir: Value = serde_json::from_str(&ir_text).expect("JSON");
    let base_bounds = Bounds {
        nodes: len(&ir, "node_content_ids"),
        programs: len(&ir, "programs"),
    };
    let mut files = Vec::new();
    trace_files(&root().join("oracle/corpus/v1/tier1/tests"), &mut files);
    let (mut fragments, mut nodes, mut programs) = (0, 0, 0);
    for path in &files {
        for t in read_trace(path).unwrap_or_else(|e| panic!("{e}")) {
            let id = &t.header.id;
            let mut b = base_bounds;
            for rec in &t.records {
                match rec {
                    Record::Override(o) => {
                        let f = &o.fragment;
                        let reg = base
                            .with_fragment(f)
                            .unwrap_or_else(|e| panic!("{id}: {:?} fragment: {e}", o.kind));
                        fragments += 1;
                        for (k, c) in f["node_content_ids"]
                            .as_array()
                            .expect("ids")
                            .iter()
                            .enumerate()
                        {
                            let n = NodeId(base_bounds.nodes + u32::try_from(k).expect("fits"));
                            assert_eq!(reg.node_content_id(n), cid(c), "{id}: node {}", n.0);
                        }
                        for (k, p) in f["programs"]
                            .as_array()
                            .expect("programs")
                            .iter()
                            .enumerate()
                        {
                            let want = u32::try_from(k).expect("fits") + base_bounds.programs;
                            assert_eq!(
                                reg.program_by_content_id(&cid(&p["content_id"])),
                                Some(ProgramId(u16::try_from(want).expect("fits"))),
                                "{id}: program {want}"
                            );
                        }
                        for fs in f["filter_sets"].as_array().expect("filter sets") {
                            assert!(
                                reg.filter_set_by_content_id(&cid(&fs["content_id"]))
                                    .is_some(),
                                "{id}: filter set {}",
                                fs["content_id"]
                            );
                        }
                        b = Bounds {
                            nodes: base_bounds.nodes + len(f, "nodes"),
                            programs: base_bounds.programs + len(f, "programs"),
                        };
                    }
                    Record::Call(c) => {
                        let (n, p) = check_spec(&c.call, b, id);
                        nodes += n + check_result(&c.result, b, id);
                        programs += p;
                    }
                    Record::Probe(c) => {
                        let (n, p) = check_spec(&c.call, b, id);
                        nodes += n + check_result(&c.result, b, id);
                        programs += p;
                    }
                    Record::Scenario(s) => {
                        for c in &s.calls {
                            let (n, p) = check_spec(&c.call, b, id);
                            nodes += n;
                            programs += p;
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    assert!(fragments >= 40, "{fragments} fragments");
    assert!(
        nodes > 0 && programs > 0,
        "{nodes} node and {programs} program references"
    );
}
