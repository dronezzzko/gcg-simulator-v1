//! Every function a committed tier-1 trace calls or probes has an entry in the real engine's
//! name dispatcher (PLAN §8.2: "a name without a dispatcher entry is a counted failure"), so
//! the per-test replay can only fail on behaviour, never on a missing name.

use std::path::PathBuf;

use gcg_diff::Record;
use gcg_diff::dispatch::{CALLS, PROBES};
use gcg_diff::manifest::{self, Repo};
use gcg_diff::read_trace;
use gcg_diff::replay::{SCENARIO_START, VIEW_SIGNATURE_PROBE, scenario_advance};

fn repo() -> Repo {
    Repo {
        root: PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../..")),
    }
}

#[test]
fn every_recorded_call_and_probe_name_has_a_dispatcher_entry() {
    let m = manifest::load(&repo(), 1).expect("manifest");
    let mut calls: Vec<String> = Vec::new();
    let mut probes: Vec<String> = vec![VIEW_SIGNATURE_PROBE.to_owned()];
    for e in &m.entries {
        if e.records.is_none() {
            continue;
        }
        for t in read_trace(&m.root.join(&e.path)).expect("decodes") {
            for r in &t.records {
                match r {
                    Record::Call(c) if c.new_game.is_none() => calls.push(c.call.func.clone()),
                    Record::Probe(p) => probes.push(p.call.func.clone()),
                    Record::Scenario(s) => {
                        calls.extend(s.calls.iter().map(|c| c.call.func.clone()));
                        if !s.calls_reproduce {
                            calls.push(scenario_advance("").func);
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    calls.sort();
    calls.dedup();
    probes.sort();
    probes.dedup();
    let missing: Vec<&String> = calls
        .iter()
        .filter(|f| !CALLS.contains(&f.as_str()))
        .chain(probes.iter().filter(|f| !PROBES.contains(&f.as_str())))
        .collect();
    assert!(missing.is_empty(), "no dispatcher entry: {missing:?}");
    assert!(calls.iter().any(|f| f == SCENARIO_START));
    assert!(
        calls.len() >= 25 && probes.len() >= 25,
        "{calls:?} {probes:?}"
    );
}
