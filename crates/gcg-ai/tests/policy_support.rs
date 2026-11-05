//! Shared helpers of the `policy_*` test targets: the vector files written by
//! `oracle/export_policy_vectors.py` (crates/gcg-ai/tests/vectors/policy/), Python's
//! `float.hex()` both ways, actions and u64 values in the trace encoding (PLAN §8.2), and
//! states rebuilt from their canonical snapshots with `canon::from_canonical`.
//!
//! Each test target includes this file as a module and uses a subset of it.
#![allow(
    dead_code,
    reason = "a module of several test targets, each of which uses a subset"
)]

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use gcg_core::rng::SplitMix64;
use gcg_effects::Registry;
use gcg_engine::canon::{from_canonical, state_digest};
use gcg_engine::{Action, ActionKind, GameState, StateSnapshot};
use serde_json::Value;

/// `crates/gcg-ai/tests/vectors/policy/<name>`.
pub fn vector_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/vectors/policy")
        .join(name)
}

/// The records of one NDJSON vector file, in order.
pub fn records(name: &str) -> Vec<Value> {
    fs::read_to_string(vector_path(name))
        .unwrap_or_else(|e| panic!("{name}: {e}"))
        .lines()
        .map(|l| serde_json::from_str(l).expect("a JSON record"))
        .collect()
}

/// The records of every `states-NNN.ndjson` file, in file order.
pub fn state_records() -> Vec<Value> {
    let dir = vector_path("");
    let mut names: Vec<String> = fs::read_dir(&dir)
        .expect("the vector directory")
        .map(|e| {
            e.expect("an entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .filter(|n| n.starts_with("states-") && n.ends_with(".ndjson"))
        .collect();
    names.sort();
    assert!(!names.is_empty(), "no states-*.ndjson in {}", dir.display());
    names.iter().flat_map(|n| records(n)).collect()
}

/// Python's `float.fromhex` for the forms `float.hex()` writes (`[-]0x1.<13 hex>p<exp>`,
/// `[-]0x0.0p+0`, subnormals `0x0.<13 hex>p-1022`, `inf`, `-inf`).
pub fn from_hex(text: &str) -> f64 {
    let (neg, body) = text
        .strip_prefix('-')
        .map_or((false, text), |rest| (true, rest));
    let value = if body == "inf" {
        f64::INFINITY
    } else {
        let digits = body
            .strip_prefix("0x")
            .unwrap_or_else(|| panic!("not a float.hex(): {text}"));
        let (mantissa, exponent) = digits
            .split_once('p')
            .unwrap_or_else(|| panic!("not a float.hex(): {text}"));
        let (int_part, frac) = mantissa.split_once('.').unwrap_or((mantissa, ""));
        let mut m: u64 = 0;
        for c in int_part.chars().chain(frac.chars()) {
            m = m * 16 + u64::from(c.to_digit(16).expect("a hex digit"));
        }
        let shift = i32::try_from(frac.len() * 4).expect("a short fraction");
        let e: i32 = exponent.parse().expect("an exponent");
        ldexp(m as f64, e - shift)
    };
    if neg { -value } else { value }
}

/// `m * 2**e` by exact power-of-two steps (`m` holds at most 53 significant bits).
fn ldexp(m: f64, e: i32) -> f64 {
    let mut x = m;
    let mut e = e;
    while e > 1000 {
        x *= f64::from_bits(((1000 + 1023) as u64) << 52);
        e -= 1000;
    }
    while e < -1000 {
        x *= f64::from_bits(((-1000 + 1023) as u64) << 52);
        e += 1000;
    }
    x * f64::from_bits(((e + 1023) as u64) << 52)
}

/// Python's `float.hex(x)`.
pub fn py_hex(x: f64) -> String {
    if x.is_nan() {
        return "nan".into();
    }
    let sign = if x.is_sign_negative() { "-" } else { "" };
    if x.is_infinite() {
        return format!("{sign}inf");
    }
    let bits = x.to_bits();
    let exp = ((bits >> 52) & 0x7ff) as i32;
    let frac = bits & ((1u64 << 52) - 1);
    if exp == 0 && frac == 0 {
        return format!("{sign}0x0.0p+0");
    }
    if exp == 0 {
        return format!("{sign}0x0.{frac:013x}p-1022");
    }
    format!("{sign}0x1.{frac:013x}p{:+}", exp - 1023)
}

/// A float field as Python wrote it.
pub fn f(v: &Value) -> f64 {
    from_hex(v.as_str().unwrap_or_else(|| panic!("not a hex float: {v}")))
}

/// A list of hex floats.
pub fn floats(v: &Value) -> Vec<f64> {
    v.as_array()
        .unwrap_or_else(|| panic!("not a list: {v}"))
        .iter()
        .map(f)
        .collect()
}

/// A u64 in the trace encoding: a JSON number below 2**53, else a decimal string.
pub fn u64_of(v: &Value) -> u64 {
    v.as_u64()
        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        .unwrap_or_else(|| panic!("not a u64: {v}"))
}

/// An action `[kind, a, b, c]`.
pub fn action(v: &Value) -> Action {
    let a = v.as_array().unwrap_or_else(|| panic!("not an action: {v}"));
    let int = |x: &Value| i32::try_from(x.as_i64().expect("an int")).expect("an i32");
    Action::new(
        ActionKind::from_value(a[0].as_str().expect("a kind")).expect("a known kind"),
        int(&a[1]),
        int(&a[2]),
        int(&a[3]),
    )
}

/// `Action.to_json()` as the serde form of the vectors.
pub fn action_value(a: Action) -> Value {
    Value::from(vec![
        Value::from(a.kind.as_str()),
        Value::from(a.a),
        Value::from(a.b),
        Value::from(a.c),
    ])
}

/// The embedded effect registry (the IR every vector was exported against).
pub fn registry() -> Arc<Registry> {
    Registry::embedded()
}

/// The state of a `state`/`terminal` record, rebuilt from its snapshot; its digest is
/// checked against Python's `state_digest`, so every later mismatch is the AI's, not the
/// snapshot's.
pub fn state_of(reg: &Arc<Registry>, rec: &Value) -> GameState {
    let snap = &rec["snapshot"];
    let st = from_canonical(
        reg,
        &StateSnapshot {
            state: snap["state"].clone(),
            continuous: snap["continuous"].clone(),
            filter_sets: snap["filter_sets"].clone(),
            rules_checked: snap["rules_checked"].as_i64().expect("rules_checked"),
        },
    )
    .unwrap_or_else(|e| panic!("{}: {e}", rec["id"]));
    assert_eq!(
        state_digest(&st),
        rec["digest"].as_str().expect("a digest"),
        "{}: snapshot digest",
        rec["id"]
    );
    st
}

/// A generator in the state Python seeded it with.
pub fn rng_of(v: &Value) -> SplitMix64 {
    SplitMix64::new(u64_of(v))
}

/// Bit-for-bit float list equality, with a readable difference.
pub fn same_floats(got: &[f64], want: &[f64]) -> Result<(), String> {
    if got.len() == want.len()
        && got
            .iter()
            .zip(want)
            .all(|(a, b)| a.to_bits() == b.to_bits())
    {
        return Ok(());
    }
    Err(format!(
        "got {:?}, want {:?}",
        got.iter().map(|&x| py_hex(x)).collect::<Vec<_>>(),
        want.iter().map(|&x| py_hex(x)).collect::<Vec<_>>()
    ))
}

/// Collects mismatches and fails once with the first few of them.
#[derive(Default)]
pub struct Mismatches {
    pub checked: usize,
    pub failures: Vec<String>,
}

impl Mismatches {
    pub fn check(&mut self, id: &str, what: &str, r: Result<(), String>) {
        self.checked += 1;
        if let Err(e) = r {
            self.failures.push(format!("{id}: {what}: {e}"));
        }
    }

    pub fn assert_none(&self, total_expected_at_least: usize) {
        assert!(
            self.failures.is_empty(),
            "{} of {} checks differ from Python:\n{}",
            self.failures.len(),
            self.checked,
            self.failures
                .iter()
                .take(12)
                .cloned()
                .collect::<Vec<_>>()
                .join("\n")
        );
        assert!(
            self.checked >= total_expected_at_least,
            "only {} checks ran (expected at least {total_expected_at_least})",
            self.checked
        );
    }
}

/// `Ok` when equal, else a readable difference.
pub fn same<T: PartialEq + std::fmt::Debug>(got: T, want: T) -> Result<(), String> {
    if got == want {
        Ok(())
    } else {
        Err(format!("got {got:?}, want {want:?}"))
    }
}
