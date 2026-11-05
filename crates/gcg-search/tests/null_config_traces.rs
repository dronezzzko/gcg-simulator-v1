//! The null-configuration test on the `search` corpus traces (`port/corpus/v1/tier1/search/`,
//! read with `gcg_diff::read_trace`; docs/port/contracts.md, gcg-search section): at every
//! decision that has a snapshot and whose agent block records a search, the reference search
//! and the `gcg-search` copy under `null_config` of the trace's preset, started from the
//! recorded per-decision seed, return the same action, alternatives, statistics and generator
//! state, and both equal the recorded block.

mod common;

use std::path::PathBuf;

use gcg_ai::DEFAULT_WEIGHTS;
use gcg_ai_reference::{BASELINE_WEIGHTS, ReferenceConfig, baseline_preset, reference_preset};
use gcg_diff::{Record, read_trace};
use gcg_effects::Registry;
use gcg_engine::canon::from_canonical;

use common::both_searches;

fn preset(name: &str) -> (&'static ReferenceConfig, &'static gcg_ai::Weights) {
    match name.strip_prefix("baseline-") {
        Some(b) => (
            baseline_preset(b).expect("a baseline preset"),
            BASELINE_WEIGHTS,
        ),
        None => (
            reference_preset(name).expect("a reference preset"),
            DEFAULT_WEIGHTS,
        ),
    }
}

#[test]
fn the_copy_replays_the_reference_on_the_search_traces() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../port/corpus/v1/tier1/search");
    let reg = Registry::embedded();
    let mut checked = 0;
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("the search set")
        .map(|e| e.expect("an entry").path())
        .filter(|p| p.extension().is_some_and(|x| x == "ndjson"))
        .collect();
    files.sort();
    assert!(!files.is_empty());
    for path in files {
        for trace in read_trace(&path).unwrap_or_else(|e| panic!("{e}")) {
            let name = trace
                .header
                .preset
                .clone()
                .expect("a search trace has a preset");
            let (reference, weights) = preset(&name);
            let mut snapshot = None;
            for rec in &trace.records {
                match rec {
                    Record::Snapshot(s) => snapshot = Some((s.index, s.snapshot.clone())),
                    Record::Decision(d) => {
                        let Some((i, snap)) = &snapshot else { continue };
                        let Some(agent) = &d.agent else { continue };
                        if *i != d.index || agent.iterations.is_none() {
                            continue;
                        }
                        let st = from_canonical(&reg, snap).expect("a state");
                        let ((r, r_rng), (c, c_rng)) =
                            both_searches(&st, d.player, reference, weights, agent.seed);
                        let id = format!("{} decision {}", trace.header.id, d.index);
                        assert_eq!(r.action, d.chosen, "{id}");
                        assert_eq!(r_rng, agent.rng_after, "{id}");
                        assert_eq!(Some(r.stats.iterations), agent.iterations, "{id}");
                        assert_eq!(Some(r.stats.budget), agent.budget, "{id}");
                        assert_eq!(Some(r.stats.cap), agent.cap, "{id}");
                        assert_eq!(Some(&r.stats.roots), agent.roots.as_ref(), "{id}");
                        let rec_alts: Vec<_> = agent
                            .alternatives
                            .iter()
                            .map(|a| (a.action, a.value.to_bits(), a.visits))
                            .collect();
                        let alts: Vec<_> = r
                            .alternatives
                            .iter()
                            .map(|a| (a.action, a.value.to_bits(), a.visits))
                            .collect();
                        assert_eq!(alts, rec_alts, "{id}");
                        assert_eq!(c.action, r.action, "{id}");
                        let c_alts: Vec<_> = c
                            .alternatives
                            .iter()
                            .map(|a| (a.action, a.value.to_bits(), a.visits))
                            .collect();
                        assert_eq!(c_alts, alts, "{id}");
                        assert_eq!(c.stats, r.stats, "{id}");
                        assert_eq!(c_rng, r_rng, "{id}");
                        checked += 1;
                    }
                    _ => {}
                }
            }
        }
    }
    assert!(checked > 0, "no searched decision has a snapshot");
}
