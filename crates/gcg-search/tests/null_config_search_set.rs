//! The null-configuration test over every decision of the tier-1 `search` set
//! (`oracle/corpus/v1/tier1/search/`, presets `fast`, `standard`, `strong` and
//! `baseline-standard`), not only those with a stored snapshot (`null_config_traces.rs`): each
//! trace is replayed from its first snapshot by applying the recorded choices (the state's
//! digest prefix must match the trace's before every decision). At every decision whose agent
//! block records a search, the reference and the `gcg-search` copy under `null_config`, started
//! from the recorded per-decision seed, return the recorded action, alternatives, iterations,
//! budget, cap, root keys and generator state; so does the copy with the shipped presets'
//! engine-error leaf on, with no error leaf. At every REDRAW and CHOOSE_FIRST decision the copy's
//! `pregame` returns the recorded action, alternatives and generator state.

mod common;

use std::path::PathBuf;

use gcg_ai::DEFAULT_WEIGHTS;
use gcg_ai_reference::{BASELINE_WEIGHTS, ReferenceConfig, baseline_preset, reference_preset};
use gcg_diff::{Record, read_trace};
use gcg_effects::Registry;
use gcg_engine::canon::{from_canonical, state_digest};
use gcg_engine::{DecisionKind, GameState, apply};

use common::{both_pregames, both_searches, null_config, strengthened_search};

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

type Bits = Vec<(gcg_engine::Action, u64, u32)>;

fn bits(alts: &[gcg_ai::Alternative]) -> Bits {
    alts.iter()
        .map(|a| (a.action, a.value.to_bits(), a.visits))
        .collect()
}

#[test]
fn the_copy_replays_every_decision_of_the_search_set() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../oracle/corpus/v1/tier1/search");
    let reg = Registry::embedded();
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("the search set")
        .map(|e| e.expect("an entry").path())
        .filter(|p| p.extension().is_some_and(|x| x == "ndjson"))
        .collect();
    files.sort();
    let (mut searches, mut pregames) = (0, 0);
    for path in files {
        for trace in read_trace(&path).unwrap_or_else(|e| panic!("{e}")) {
            let name = trace.header.preset.clone().expect("a preset");
            let (reference, weights) = preset(&name);
            let mut st: Option<GameState> = None;
            for rec in &trace.records {
                match rec {
                    Record::Snapshot(s) if st.is_none() => {
                        st = Some(from_canonical(&reg, &s.snapshot).expect("a state"));
                    }
                    Record::Decision(d) => {
                        let id = format!("{} decision {}", trace.header.id, d.index);
                        let s = st
                            .as_mut()
                            .expect("the first snapshot precedes the decisions");
                        assert_eq!(state_digest(s)[..16], d.fingerprint.digest16, "{id}");
                        if let Some(agent) = &d.agent {
                            let recorded: Bits = agent
                                .alternatives
                                .iter()
                                .map(|a| (a.action, a.value.to_bits(), a.visits))
                                .collect();
                            if agent.iterations.is_some() {
                                let ((r, r_rng), (c, c_rng)) =
                                    both_searches(s, d.player, reference, weights, agent.seed);
                                assert_eq!(r.action, d.chosen, "{id}");
                                assert_eq!(bits(&r.alternatives), recorded, "{id}");
                                assert_eq!(r_rng, agent.rng_after, "{id}");
                                assert_eq!(Some(r.stats.iterations), agent.iterations, "{id}");
                                assert_eq!(Some(r.stats.budget), agent.budget, "{id}");
                                assert_eq!(Some(r.stats.cap), agent.cap, "{id}");
                                assert_eq!(Some(&r.stats.roots), agent.roots.as_ref(), "{id}");
                                assert_eq!(c.action, r.action, "{id}");
                                assert_eq!(bits(&c.alternatives), recorded, "{id}");
                                assert_eq!(c.stats, r.stats, "{id}");
                                assert_eq!(c_rng, r_rng, "{id}");
                                let mut leaf = null_config(reference, weights);
                                leaf.mechanisms.engine_error_leaf = true;
                                let (e, effort, e_rng) =
                                    strengthened_search(s, d.player, &leaf, agent.seed);
                                assert_eq!(bits(&e.alternatives), recorded, "{id}");
                                assert_eq!(e.stats, r.stats, "{id}");
                                assert_eq!(e_rng, r_rng, "{id}");
                                assert_eq!(effort.engine_error_leaves, 0, "{id}");
                                searches += 1;
                            } else if matches!(
                                d.kind,
                                DecisionKind::Redraw | DecisionKind::ChooseFirst
                            ) {
                                let null = null_config(reference, weights);
                                let ((r, r_rng), (c, c_rng)) =
                                    both_pregames(s, d.player, &null, agent.seed);
                                assert_eq!(r.0, d.chosen, "{id}");
                                assert_eq!(bits(&r.1), recorded, "{id}");
                                assert_eq!(r_rng, agent.rng_after, "{id}");
                                assert_eq!(c.0, r.0, "{id}");
                                assert_eq!(bits(&c.1), recorded, "{id}");
                                assert_eq!(c_rng, r_rng, "{id}");
                                pregames += 1;
                            }
                        }
                        apply(s, d.chosen, true).unwrap_or_else(|e| panic!("{id}: {e:?}"));
                    }
                    _ => {}
                }
            }
        }
    }
    assert_eq!(searches, 661, "searched decisions");
    assert!(pregames >= 9, "{pregames}");
}
