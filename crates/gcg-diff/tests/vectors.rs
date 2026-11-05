//! The §8.2 encodings against vectors exported from Python 3.15 by
//! `uv run python port/oracle/traces.py vectors` (`tests/vectors/trace_vectors.json`):
//! `options_sha16`, `float.hex`, the u64 encoding, and `state_digest` of real canonical states.

use gcg_diff::floathex::{from_hex, to_hex};
use gcg_diff::json::{as_u64, state_digest};
use gcg_diff::options_sha16;
use gcg_diff::reader::action;
use serde_json::Value;

fn vectors() -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/vectors/trace_vectors.json"
    );
    serde_json::from_str(&std::fs::read_to_string(path).expect("vectors")).expect("JSON")
}

fn list<'a>(v: &'a Value, key: &str) -> &'a [Value] {
    v[key].as_array().map(Vec::as_slice).expect(key)
}

#[test]
fn options_sha16_matches_python() {
    let v = vectors();
    let cases = list(&v, "options_sha16");
    assert!(cases.len() > 10);
    for case in cases {
        let options: Vec<_> = list(case, "options")
            .iter()
            .map(|a| action(a, "option").expect("action"))
            .collect();
        assert_eq!(
            options_sha16(&options),
            case["sha16"].as_str().expect("sha16")
        );
    }
}

#[test]
fn float_hex_matches_python() {
    let v = vectors();
    for case in list(&v, "float_hex") {
        let bits = u64::from_str_radix(case["bits"].as_str().expect("bits"), 16).expect("hex bits");
        let text = case["hex"].as_str().expect("hex");
        assert_eq!(to_hex(f64::from_bits(bits)), text);
        assert_eq!(from_hex(text).map(f64::to_bits), Ok(bits), "{text}");
    }
}

#[test]
fn u64_encoding_matches_python() {
    let v = vectors();
    for case in list(&v, "u64") {
        let want: u64 = case["value"].as_str().expect("value").parse().expect("u64");
        assert_eq!(as_u64(&case["json"], "u64"), Ok(want));
    }
}

#[test]
fn state_digest_matches_python() {
    let v = vectors();
    let cases = list(&v, "state_digest");
    assert!(!cases.is_empty());
    for case in cases {
        assert_eq!(
            state_digest(&case["state"]).as_deref(),
            Ok(case["digest"].as_str().expect("digest"))
        );
    }
}
