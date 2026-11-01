//! The null-configuration test (PLAN §10.4 D2 gate, X12): `gcg-search` with
//! `null_config(reference_preset(p))` makes the reference's decisions, alternatives and RNG
//! consumption, compared at the `search` level with the reference's per-decision generator
//! (agent names differ by design, PLAN A9). This file checks the configurations and runs both
//! searches on every searched decision of `crates/gcg-ai/tests/vectors/search/decisions.ndjson`
//! for `fast`, `standard`, `strong` and `baseline-standard`; `null_config_traces.rs` does the
//! same on the snapshots of the `search` corpus traces.

mod common;

use std::fs;
use std::path::PathBuf;

use gcg_ai::{Alternative, DEFAULT_WEIGHTS, ROLE_WEIGHTS, TUNED_WEIGHTS};
use gcg_ai_reference::{BASELINE_STANDARD, BASELINE_WEIGHTS, FAST, PRESETS, STANDARD, STRONG};
use gcg_effects::Registry;
use gcg_engine::StateSnapshot;
use gcg_engine::canon::from_canonical;
use gcg_search::{Preset, preset_config};
use serde_json::Value;

use common::{both_searches, null_config};

fn bits(alts: &[Alternative]) -> Vec<(gcg_engine::Action, u64, u32)> {
    alts.iter()
        .map(|a| (a.action, a.value.to_bits(), a.visits))
        .collect()
}

#[test]
fn null_configurations_copy_every_reference_field() {
    for (_, reference) in PRESETS {
        let null = null_config(reference, DEFAULT_WEIGHTS);
        assert_eq!(null.name, reference.name);
        assert_eq!(null.weights, ROLE_WEIGHTS);
        for n in [1, 2, 3, 5, 10, 30] {
            assert_eq!(null.budget(n), reference.budget(n));
        }
        assert_eq!(null.knobs(), reference.knobs());
    }
    let baseline = null_config(&BASELINE_STANDARD, BASELINE_WEIGHTS);
    assert_eq!(baseline.weights, TUNED_WEIGHTS);
    // v0 presets: standard is reference strong and fast is reference standard, renamed
    // (PLAN §6.4).
    let renamed = |c: &gcg_search::StrengthenedConfig, name: &'static str| {
        let mut c = c.clone();
        c.name = name.into();
        c
    };
    assert_eq!(
        renamed(&null_config(&STRONG, DEFAULT_WEIGHTS), "standard"),
        *preset_config(Preset::Standard)
    );
    assert_eq!(
        renamed(&null_config(&STANDARD, DEFAULT_WEIGHTS), "fast"),
        *preset_config(Preset::Fast)
    );
    assert_ne!(
        null_config(&FAST, DEFAULT_WEIGHTS).agent_name(),
        FAST.agent_name(),
        "search-* never collides with mcts-* (PLAN A9)"
    );
}

#[test]
fn the_copy_replays_the_reference_on_every_vector_decision() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../gcg-ai/tests/vectors/search/decisions.ndjson");
    let reg = Registry::embedded();
    let mut checked = 0;
    for line in fs::read_to_string(path).expect("the vectors").lines() {
        let rec: Value = serde_json::from_str(line).expect("a record");
        let Some(searches) = rec["search"].as_array() else {
            continue;
        };
        let snap = &rec["snapshot"];
        let st = from_canonical(
            &reg,
            &StateSnapshot {
                state: snap["state"].clone(),
                continuous: snap["continuous"].clone(),
                filter_sets: snap["filter_sets"].clone(),
                rules_checked: snap["rules_checked"].as_i64().expect("rules_checked"),
            },
        )
        .expect("a state");
        let player = u8::try_from(rec["player"].as_i64().expect("a seat")).expect("a seat");
        let seed = rec["seed"]
            .as_u64()
            .or_else(|| rec["seed"].as_str().and_then(|s| s.parse().ok()))
            .expect("a seed");
        for s in searches {
            let (reference, weights) = match s["config"].as_str() {
                Some("fast") => (&FAST, DEFAULT_WEIGHTS),
                Some("standard") => (&STANDARD, DEFAULT_WEIGHTS),
                Some("strong") => (&STRONG, DEFAULT_WEIGHTS),
                Some("baseline-standard") => (&BASELINE_STANDARD, BASELINE_WEIGHTS),
                other => panic!("unknown configuration {other:?}"),
            };
            let ((r, r_rng), (c, c_rng)) = both_searches(&st, player, reference, weights, seed);
            let id = format!("{} {}", rec["id"], s["config"]);
            assert_eq!(c.action, r.action, "{id}");
            assert_eq!(bits(&c.alternatives), bits(&r.alternatives), "{id}");
            assert_eq!(c.stats, r.stats, "{id}");
            assert_eq!(c_rng, r_rng, "{id}");
            checked += 1;
        }
    }
    assert!(checked >= 100, "{checked}");
}
