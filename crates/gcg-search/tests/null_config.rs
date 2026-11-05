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
use gcg_search::{Preset, preset_config, reference_mapping};
use serde_json::Value;

use common::{both_pregames, both_searches, null_config, strengthened_search};

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
        assert!(null.mechanisms.enabled().is_empty());
        assert_eq!(null.check_mechanisms(), Ok(()));
    }
    let baseline = null_config(&BASELINE_STANDARD, BASELINE_WEIGHTS);
    assert_eq!(baseline.weights, TUNED_WEIGHTS);
    // The shipped presets: standard is reference strong and fast is reference standard,
    // renamed (PLAN §6.4), with the M15 budgets and mechanisms (PLAN §12.8,
    // docs/port/eval/M15/README.md); every other setting is the mapped reference preset's.
    let shipped = |c: &gcg_search::StrengthenedConfig,
                   name: &'static str,
                   (ipo, min, max): (u32, u32, u32)| {
        let mut c = c.clone();
        c.name = name.into();
        c.iterations_per_option = ipo;
        c.min_iterations = min;
        c.max_iterations = max;
        for flag in [
            "adaptive_horizon",
            "capped_rollout_continuation",
            "tactical_solver",
            "exclude_capped_play_draw",
            "engine_error_leaf",
            "kill_aware_targeting",
            "tree_lethal_select",
        ] {
            assert_eq!(c.mechanisms.set_text(flag, "true"), Some(Ok(())), "{flag}");
        }
        c
    };
    assert_eq!(
        shipped(
            &null_config(&STRONG, DEFAULT_WEIGHTS),
            "standard",
            (1024, 2560, 10240)
        ),
        *preset_config(Preset::Standard)
    );
    assert_eq!(
        shipped(
            &null_config(&STANDARD, DEFAULT_WEIGHTS),
            "fast",
            (96, 240, 960)
        ),
        *preset_config(Preset::Fast)
    );
    // The configurations they start from: the mapped reference preset, renamed, with only the
    // engine-error leaf on (W-143), the shipped presets before M15.
    let renamed = |c: &gcg_search::StrengthenedConfig, name: &'static str| {
        let mut c = c.clone();
        c.name = name.into();
        c.mechanisms.engine_error_leaf = true;
        c
    };
    assert_eq!(
        renamed(&null_config(&STRONG, DEFAULT_WEIGHTS), "standard"),
        *reference_mapping(Preset::Standard)
    );
    assert_eq!(
        renamed(&null_config(&STANDARD, DEFAULT_WEIGHTS), "fast"),
        *reference_mapping(Preset::Fast)
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
            // The shipped presets' engine-error leaf changes nothing where no simulated line
            // fails (W-143): the same decision, statistics and draws, and no error leaf.
            let mut leaf = null_config(reference, weights);
            leaf.mechanisms.engine_error_leaf = true;
            let (e, effort, e_rng) = strengthened_search(&st, player, &leaf, seed);
            assert_eq!(e.action, r.action, "{id}");
            assert_eq!(bits(&e.alternatives), bits(&r.alternatives), "{id}");
            assert_eq!(e.stats, r.stats, "{id}");
            assert_eq!(e_rng, r_rng, "{id}");
            assert_eq!(effort.engine_error_leaves, 0, "{id}");
            checked += 1;
        }
    }
    assert!(checked >= 100, "{checked}");
}

/// REDRAW and CHOOSE_FIRST on the observation (`gcg_search::pregame`, null configuration)
/// equal `mulligan_choice` and `first_player_choice` on the true state, which equal Python on
/// these records (`crates/gcg-ai/tests/search_components.rs`, `search_engine.rs`).
#[test]
fn the_copy_replays_the_reference_pregame_choices() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../gcg-ai/tests/vectors/search/decisions.ndjson");
    let reg = Registry::embedded();
    let mut checked = 0;
    for line in fs::read_to_string(path).expect("the vectors").lines() {
        let rec: Value = serde_json::from_str(line).expect("a record");
        let runs: Vec<(Option<u32>, Option<u32>)> = match rec["kind"].as_str() {
            Some("redraw") => rec["mulligan"]
                .as_array()
                .expect("mulligan runs")
                .iter()
                .map(|m| {
                    (
                        u32::try_from(m["samples"].as_u64().expect("samples")).ok(),
                        None,
                    )
                })
                .collect(),
            Some("choose_first") => vec![(
                None,
                u32::try_from(rec["first_player"]["games"].as_u64().expect("games")).ok(),
            )],
            _ => continue,
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
        for (samples, games) in runs {
            let mut cfg = null_config(&FAST, DEFAULT_WEIGHTS);
            if let Some(n) = samples {
                cfg.mulligan_samples = n;
            }
            if let Some(n) = games {
                cfg.first_player_games = n;
            }
            let ((r, r_rng), (c, c_rng)) = both_pregames(&st, player, &cfg, seed);
            let id = format!("{} {samples:?} {games:?}", rec["id"]);
            assert_eq!(c.0, r.0, "{id}");
            assert_eq!(bits(&c.1), bits(&r.1), "{id}");
            assert_eq!(c_rng, r_rng, "{id}");
            checked += 1;
        }
    }
    assert!(checked >= 18, "{checked}");
}

/// The crate's own null flags are the spelled-out list of the gate.
#[test]
fn the_crate_null_flags_equal_the_spelled_out_list() {
    assert_eq!(gcg_search::MechanismFlags::NULL, common::NULL_MECHANISMS);
}
