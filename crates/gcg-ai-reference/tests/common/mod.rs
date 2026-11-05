//! Readers of the search vectors of `crates/gcg-ai/tests/vectors/search/`
//! (`oracle/export_ai_vectors.py`) for the reference-search tests.

#![allow(
    dead_code,
    reason = "each test file uses its own subset of the readers"
)]

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use gcg_ai::{Alternative, Weights};
use gcg_ai_reference::{
    BASELINE_STANDARD, BASELINE_WEIGHTS, FAST, ReferenceConfig, STANDARD, STRONG,
};
use gcg_effects::Registry;
use gcg_engine::canon::from_canonical;
use gcg_engine::{Action, ActionKind, GameState, StateSnapshot};
use serde_json::Value;

pub fn vectors() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../gcg-ai/tests/vectors/search")
}

pub fn json(name: &str) -> Value {
    let text = fs::read_to_string(vectors().join(name)).expect("a vector file");
    serde_json::from_str(&text).expect("JSON")
}

pub fn records() -> Vec<Value> {
    fs::read_to_string(vectors().join("decisions.ndjson"))
        .expect("the vectors")
        .lines()
        .map(|l| serde_json::from_str(l).expect("a JSON record"))
        .collect()
}

pub fn state(reg: &Arc<Registry>, rec: &Value) -> GameState {
    let snap = &rec["snapshot"];
    let snapshot = StateSnapshot {
        state: snap["state"].clone(),
        continuous: snap["continuous"].clone(),
        filter_sets: snap["filter_sets"].clone(),
        rules_checked: snap["rules_checked"].as_i64().expect("rules_checked"),
    };
    from_canonical(reg, &snapshot).unwrap_or_else(|e| panic!("{}: {e}", rec["id"]))
}

pub fn u64_of(v: &Value) -> u64 {
    v.as_u64()
        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        .unwrap_or_else(|| panic!("not a u64: {v}"))
}

pub fn int(v: &Value) -> i64 {
    v.as_i64().unwrap_or_else(|| panic!("not an int: {v}"))
}

pub fn action(v: &Value) -> Action {
    let kind = ActionKind::from_value(v[0].as_str().expect("a kind")).expect("an action kind");
    let arg = |i: usize| i32::try_from(int(&v[i])).expect("an i32 argument");
    Action::new(kind, arg(1), arg(2), arg(3))
}

/// `float.fromhex` for the forms `float.hex()` writes (`0x1.8p-2`, `-0x0.0p+0`, subnormals).
pub fn fhex(s: &str) -> f64 {
    let (neg, body) = s.strip_prefix('-').map_or((false, s), |r| (true, r));
    let body = body.strip_prefix("0x").expect("a hex float");
    let (mantissa, exp) = body.split_once('p').expect("an exponent");
    let exp: i32 = exp.parse().expect("an int exponent");
    let (int_part, frac) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let mut m = u64::from_str_radix(int_part, 16).expect("hex digits");
    for c in frac.chars() {
        m = m * 16 + u64::from(c.to_digit(16).expect("a hex digit"));
    }
    let shift = exp - 4 * i32::try_from(frac.len()).expect("short");
    let pow2 = if shift >= -1022 {
        f64::from_bits(u64::try_from(shift + 1023).expect("in range") << 52)
    } else {
        f64::from_bits(1u64 << u32::try_from(shift + 1074).expect("subnormal range"))
    };
    #[expect(clippy::cast_precision_loss, reason = "a 53-bit mantissa is exact")]
    let v = m as f64 * pow2;
    if neg { -v } else { v }
}

/// `[[action, value hex, visits], ...]` as comparable bit triples.
pub fn alternatives(v: &Value) -> Vec<(Action, u64, u32)> {
    v.as_array()
        .expect("alternatives")
        .iter()
        .map(|a| {
            (
                action(&a[0]),
                fhex(a[1].as_str().expect("a hex float")).to_bits(),
                u32::try_from(int(&a[2])).expect("visits"),
            )
        })
        .collect()
}

pub fn bits(alts: &[Alternative]) -> Vec<(Action, u64, u32)> {
    alts.iter()
        .map(|a| (a.action, a.value.to_bits(), a.visits))
        .collect()
}

/// The configuration and weights a vector's `config` label names (the exporter's
/// `PRESETS[...]` with `DEFAULT_WEIGHTS`, or the baseline with `BASELINE_WEIGHTS`).
pub fn config(label: &str) -> (&'static ReferenceConfig, &'static Weights) {
    match label {
        "fast" => (&FAST, gcg_ai::DEFAULT_WEIGHTS),
        "standard" => (&STANDARD, gcg_ai::DEFAULT_WEIGHTS),
        "strong" => (&STRONG, gcg_ai::DEFAULT_WEIGHTS),
        "baseline-standard" => (&BASELINE_STANDARD, BASELINE_WEIGHTS),
        other => panic!("unknown configuration label {other}"),
    }
}
