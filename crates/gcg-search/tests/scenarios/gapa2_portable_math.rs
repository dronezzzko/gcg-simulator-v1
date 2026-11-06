//! W-137 (GAP-A2, docs/dev/eval/weakness-status/GAP-A2.md): with `portable_math` the
//! strengthened search's results are recorded bits that do not depend on the platform C
//! library. Every float path of the search goes through its provider (the lint
//! `mech::gapa2::every_float_path_of_the_search_goes_through_its_math`), and the portable
//! provider uses IEEE-754 basic operations and the pure-Rust `libm` crate only, so these bits
//! are the same on every IEEE-754 platform: the test is the cross-platform check (W-137 item 2)
//! on any machine that runs it, macOS arm64 (where they were recorded, 2026-11-02) and the
//! Linux x86-64 CI job alike.
//!
//! Recorded: the searches of the first 8 searched decisions of the D2 vectors, and the
//! pre-game choices of their 6 CHOOSE_FIRST and first 4 REDRAW records, each as the stable key
//! of its comparable bits (action, every alternative's value bits and visits, the statistics,
//! the mechanism counters and the generator state), under three configurations with
//! `portable_math` on: the shipped `fast` preset, the shipped `standard` mechanisms at the
//! reference `standard` budget with most other mechanisms on ("wide", every package's float
//! path), and the null configuration.

use std::path::PathBuf;

use gcg_ai::{AgentCx, Alternative, NoTrace, Scratch};
use gcg_core::pool::{Cancel, Pool, TaskCx};
use gcg_core::rng::SplitMix64;
use gcg_core::stable_hash::stable_key;
use gcg_effects::Registry;
use gcg_engine::canon::from_canonical;
use gcg_engine::{GameState, StateSnapshot};
use gcg_search::{MechanismFlags, Observation, Preset, StrengthenedConfig};
use serde_json::Value;

/// The D2 vector records of `kind` (`None`: the searched decisions), as `(state, player,
/// per-decision seed)`.
fn records(kind: Option<&str>, limit: usize) -> Vec<(GameState, u8, u64)> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../gcg-ai/tests/vectors/search/decisions.ndjson");
    let reg = Registry::embedded();
    std::fs::read_to_string(path)
        .expect("the vectors")
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("a record"))
        .filter(|rec| match kind {
            None => rec["search"].as_array().is_some_and(|s| !s.is_empty()),
            Some(k) => rec["kind"].as_str() == Some(k),
        })
        .take(limit)
        .map(|rec| {
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
            (st, player, seed)
        })
        .collect()
}

/// `base` with `portable_math` and the `field=value` overrides of `flags`.
fn portable(base: &StrengthenedConfig, flags: &[(&str, &str)]) -> StrengthenedConfig {
    with_math(base, flags, true)
}

/// `base` with `portable_math` set to `on` and the overrides of `flags`.
fn with_math(base: &StrengthenedConfig, flags: &[(&str, &str)], on: bool) -> StrengthenedConfig {
    let mut cfg = base.clone();
    cfg.mechanisms.portable_math = on;
    for (field, value) in flags {
        assert_eq!(
            cfg.mechanisms.set_text(field, value),
            Some(Ok(())),
            "{field}={value}"
        );
    }
    cfg.check_mechanisms().expect("an available configuration");
    cfg
}

/// The shipped `standard` mechanisms plus one or more flags of every package that has a float
/// path, at the reference `standard` budget.
const WIDE: [(&str, &str); 22] = [
    ("adaptive_horizon", "true"),
    ("capped_rollout_continuation", "true"),
    ("tactical_solver", "true"),
    ("threat_leaf", "true"),
    ("lethal_root", "true"),
    ("opponent_buckets", "4"),
    ("prior_worlds", "4"),
    ("macro_multi_pick", "true"),
    ("transpositions", "true"),
    ("ex_variants", "true"),
    ("typed_fingerprint", "true"),
    ("prune_worlds", "4"),
    ("branch_all_selects", "true"),
    ("rest_prune_worlds", "true"),
    ("target_rollouts", "true"),
    ("real_idle_line", "true"),
    ("modal_lookahead", "true"),
    ("burst_decline_list", "true"),
    ("exhaustive_polarity", "true"),
    ("leaf_density", "true"),
    ("exclude_capped_play_draw", "true"),
    ("engine_error_leaf", "true"),
];

/// The stable key of a search's or pre-game choice's comparable bits.
fn key_of(text: &str) -> u64 {
    stable_key(text.as_bytes()).0
}

fn alternatives(alts: &[Alternative]) -> Vec<(gcg_engine::Action, u64, u32)> {
    alts.iter()
        .map(|a| (a.action, a.value.to_bits(), a.visits))
        .collect()
}

/// The keys of the searches of `states` with `cfg`, inline.
fn search_keys(states: &[(GameState, u8, u64)], cfg: &StrengthenedConfig) -> Vec<u64> {
    let cancel = Cancel::new();
    states
        .iter()
        .map(|(st, player, seed)| {
            let obs = Observation::new(st, *player).expect("an observation");
            Pool::run_inline(|scratch: &mut Scratch, par: &TaskCx<'_, Scratch>| {
                let mut cx = AgentCx {
                    scratch,
                    par,
                    cancel: &cancel,
                };
                let mut rng = SplitMix64::new(*seed);
                let (r, e) =
                    gcg_search::search_with_effort(&obs, cfg, &mut rng, &mut cx, &mut NoTrace)
                        .unwrap_or_else(|e| panic!("search failed: {e}"));
                key_of(&format!(
                    "{:?}",
                    (
                        r.action,
                        alternatives(&r.alternatives),
                        &r.stats,
                        e,
                        rng.state
                    )
                ))
            })
        })
        .collect()
}

/// The keys of the pre-game choices of `states` with `cfg`, inline.
fn pregame_keys(states: &[(GameState, u8, u64)], cfg: &StrengthenedConfig) -> Vec<u64> {
    let cancel = Cancel::new();
    states
        .iter()
        .map(|(st, player, seed)| {
            let obs = Observation::new(st, *player).expect("an observation");
            Pool::run_inline(|scratch: &mut Scratch, par: &TaskCx<'_, Scratch>| {
                let mut cx = AgentCx {
                    scratch,
                    par,
                    cancel: &cancel,
                };
                let mut rng = SplitMix64::new(*seed);
                let (action, alts) = gcg_search::pregame(&obs, cfg, &mut rng, &mut cx)
                    .unwrap_or_else(|e| panic!("pre-game choice failed: {e}"));
                key_of(&format!("{:?}", (action, alternatives(&alts), rng.state)))
            })
        })
        .collect()
}

/// The keys of every recorded run, in order: per configuration (shipped `fast`, wide, null),
/// the searches, then the CHOOSE_FIRST and REDRAW choices; `portable_math` as `on`.
fn all_keys(on: bool) -> Vec<Vec<u64>> {
    let searched = records(None, 8);
    let first = records(Some("choose_first"), 6);
    let redraw = records(Some("redraw"), 4);
    assert_eq!((searched.len(), first.len(), redraw.len()), (8, 6, 4));
    let fast = with_math(gcg_search::preset_config(Preset::Fast), &[], on);
    let mut base = gcg_search::reference_mapping(Preset::Fast).clone();
    base.mechanisms = MechanismFlags::NULL;
    let wide = with_math(&base, &WIDE, on);
    let mulligan = with_math(&base, &[("paired_mulligan", "true")], on);
    let null = with_math(&base, &[], on);
    vec![
        search_keys(&searched, &fast),
        pregame_keys(&first, &fast),
        search_keys(&searched, &wide),
        pregame_keys(&first, &wide),
        pregame_keys(&redraw, &mulligan),
        search_keys(&searched, &null),
        pregame_keys(&first, &null),
    ]
}

/// The keys recorded on the golden platform (macOS arm64, 2026-11-02; the searched decisions
/// re-recorded 2026-11-06 over the D2 vectors of the 2026-11-06 data refresh).
const RECORDED: [&[u64]; 7] = [
    &[
        0x6e8f4a7ee1c427e4,
        0x6b2c5f06fd3a5072,
        0xbfece474d56657af,
        0xc852bbffbf18943e,
        0xeaab85662ac87e6f,
        0xe159860131ec7503,
        0x88dbd6976e562a5e,
        0xc0ef7391c0f162e2,
    ],
    &[
        0x4c6efc11d8aa48a0,
        0xdaccecae2743e7c1,
        0x973dc17791f96f50,
        0x7bb5fa8da005f8ae,
        0x021e68bdec486353,
        0x92cd16b9a6f1c345,
    ],
    &[
        0x6749faeef8ac3c2a,
        0xe53dc71e954c8667,
        0x41459de6013f6ac1,
        0x769db68634e2fdd4,
        0xf571a14d6067007b,
        0x7e4c7d45238ebcda,
        0x9964628d6ac1093e,
        0x68bee3c44b6f76ef,
    ],
    &[
        0x15b7f5dbd5c13621,
        0xdaccecae2743e7c1,
        0x804bc05941b1ca74,
        0x0727bc8fd9d2edac,
        0x58fc070667ca984d,
        0x12d273aa3e9b201e,
    ],
    &[
        0x37e8dc21c0ec246a,
        0xca648abce06ff0c8,
        0xf3153be4f48dcae1,
        0x04af38f4fd348c00,
    ],
    &[
        0xbfc5edc4779663f0,
        0xc3eedc73c1ce3f61,
        0xfbf12dbb3719ae90,
        0x33ad532299d5738e,
        0xc7589d9545eafe5d,
        0x5deae254990997df,
        0x3fe06c0e62eeffba,
        0xa9fd8572853eb7c4,
    ],
    &[
        0xe0575cf4215c0d58,
        0xdaccecae2743e7c1,
        0x5f0ca5b0e5434ede,
        0x0727bc8fd9d2edac,
        0x58fc070667ca984d,
        0xda0e812ec01f379e,
    ],
];

#[test]
fn portable_math_searches_give_the_recorded_bits_on_every_platform() {
    let keys = all_keys(true);
    let printed: Vec<String> = keys
        .iter()
        .map(|k| {
            let items: Vec<String> = k.iter().map(|x| format!("0x{x:016x}")).collect();
            format!("&[{}],", items.join(", "))
        })
        .collect();
    let recorded: Vec<Vec<u64>> = RECORDED.iter().map(|r| r.to_vec()).collect();
    assert_eq!(keys, recorded, "computed keys:\n{}", printed.join("\n"));
}

/// The same searches give the same keys run after run (no hidden state between searches).
#[test]
fn portable_math_searches_repeat_exactly() {
    let searched = records(None, 4);
    let fast = portable(gcg_search::preset_config(Preset::Fast), &[]);
    assert_eq!(search_keys(&searched, &fast), search_keys(&searched, &fast));
}

/// The recorded bits are the portable library's: on the golden platform the same runs with the
/// platform library differ in some of them (Apple libm and the `libm` crate differ in the last
/// bit for about 8% of `exp`/`ln` results), so a platform call left on a portable path would
/// change the keys.
#[test]
fn the_recorded_bits_depend_on_the_library() {
    if !cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        return;
    }
    let portable_keys = all_keys(true);
    let platform_keys = all_keys(false);
    let differ: usize = portable_keys
        .iter()
        .zip(&platform_keys)
        .map(|(a, b)| a.iter().zip(b).filter(|(x, y)| x != y).count())
        .sum();
    let total: usize = portable_keys.iter().map(Vec::len).sum();
    eprintln!("W-137: {differ} of {total} recorded keys differ with the platform library");
    assert!(
        differ > 0,
        "the platform library gave the portable bits everywhere"
    );
}
