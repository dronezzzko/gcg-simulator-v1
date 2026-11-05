//! The report helpers against Python vectors (`stats.json`, exported by
//! `oracle/export_records.py`): Wilson intervals, the z-test p-values and Holm, bit for
//! bit; distributions; `shlex.quote`; the stale-replay regex with Python's Unicode `\d`; the
//! schema `pattern`s (`re.search`); the `date` format; the schema keyword list; and the
//! jsonschema verdict on mutated `results.json` documents (one violation per keyword).

use std::fs;
use std::path::Path;

use gcg_core::pyjson::Json;
use gcg_runner::SchemaVersion;
use gcg_runner::reports::pyregex::{DIGIT_RANGES, Regex};
use gcg_runner::reports::schema::{SUPPORTED_KEYWORDS, is_date, results_schema_v1_1};
use gcg_runner::reports::stats::{Distribution, Rate, Z95, holm, two_proportion_p, wilson};
use gcg_runner::reports::writer::{is_replay_file_name, shlex_quote};
use gcg_runner::reports::{json_from_value, to_json_text, validate_results};
use serde_json::Value;

fn vectors() -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/vectors/records/stats.json");
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

fn bits(v: &Value) -> f64 {
    f64::from_bits(u64::from_str_radix(v.as_str().unwrap(), 16).unwrap())
}

fn u64_of(v: &Value) -> u64 {
    v.as_u64().unwrap()
}

#[test]
fn wilson_is_bit_identical() {
    let v = vectors();
    let cases = v["wilson"].as_array().unwrap();
    assert!(cases.len() > 1000);
    for c in cases {
        let (s, n) = (u64_of(&c[0]), u64_of(&c[1]));
        let (lo, hi) = wilson(s, n, Z95).unwrap();
        assert_eq!(
            (lo.to_bits(), hi.to_bits()),
            (bits(&c[2]).to_bits(), bits(&c[3]).to_bits()),
            "wilson({s}, {n})"
        );
    }
    for c in v["wilson_errors"].as_array().unwrap() {
        let (s, n) = (c[0].as_i64().unwrap(), c[1].as_i64().unwrap());
        if let (Ok(s), Ok(n)) = (u64::try_from(s), u64::try_from(n)) {
            assert_eq!(wilson(s, n, Z95), None, "wilson({s}, {n}) raises in Python");
        }
    }
}

#[test]
fn two_proportion_p_is_bit_identical() {
    let v = vectors();
    for c in v["two_proportion_p"].as_array().unwrap() {
        let a = Rate::new(u64_of(&c[0]), u64_of(&c[1]));
        let b = Rate::new(u64_of(&c[2]), u64_of(&c[3]));
        let p = two_proportion_p(&a, &b);
        assert_eq!(p.to_bits(), bits(&c[4]).to_bits(), "{c}");
    }
}

#[test]
fn holm_matches_python() {
    let v = vectors();
    for c in v["holm"].as_array().unwrap() {
        let ps: Vec<f64> = c[0].as_array().unwrap().iter().map(bits).collect();
        let want: Vec<bool> = c[1]
            .as_array()
            .unwrap()
            .iter()
            .map(|b| b.as_bool().unwrap())
            .collect();
        assert_eq!(holm(&ps, 0.05), want, "{ps:?}");
    }
}

#[test]
fn distributions_match_python() {
    let v = vectors();
    for c in v["distribution"].as_array().unwrap() {
        let values: Vec<i64> = c[0]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_i64().unwrap())
            .collect();
        assert_eq!(
            to_json_text(&Distribution::of(&values).to_json()),
            to_json_text(&json_from_value(&c[1])),
            "{values:?}"
        );
    }
}

#[test]
fn shlex_quote_matches_python() {
    let v = vectors();
    for c in v["shlex_quote"].as_array().unwrap() {
        assert_eq!(shlex_quote(c[0].as_str().unwrap()), c[1].as_str().unwrap());
    }
}

#[test]
fn the_replay_file_regex_matches_python() {
    let v = vectors();
    let ranges: Vec<(u32, u32)> = v["digit_ranges"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            let lo = u32::try_from(u64_of(&r[0])).unwrap();
            let hi = u32::try_from(u64_of(&r[1])).unwrap();
            (lo, hi)
        })
        .collect();
    assert_eq!(
        ranges, DIGIT_RANGES,
        "Python's \\d is the table in pyregex.rs"
    );
    for c in v["replay_file"].as_array().unwrap() {
        let name = c[0].as_str().unwrap();
        assert_eq!(
            is_replay_file_name(name),
            c[1].as_bool().unwrap(),
            "{name:?}"
        );
    }
}

#[test]
fn schema_patterns_and_dates_match_python() {
    let v = vectors();
    let patterns = v["patterns"].as_array().unwrap();
    assert_eq!(patterns.len(), 2, "every pattern of the schema");
    for p in patterns {
        let re = Regex::new(p["pattern"].as_str().unwrap()).unwrap();
        for c in p["cases"].as_array().unwrap() {
            let s = c[0].as_str().unwrap();
            assert_eq!(
                re.is_match(s),
                c[1].as_bool().unwrap(),
                "{} on {s:?}",
                p["pattern"]
            );
        }
    }
    for c in v["dates"].as_array().unwrap() {
        let s = c[0].as_str().unwrap();
        assert_eq!(is_date(s), c[1].as_bool().unwrap(), "date {s:?}");
    }
}

#[test]
fn the_validator_supports_exactly_the_schema_keywords() {
    let v = vectors();
    let python: Vec<&str> = v["schema_keywords"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k.as_str().unwrap())
        .collect();
    assert_eq!(
        python, SUPPORTED_KEYWORDS,
        "keywords of results.v1.schema.json"
    );
    let _ = results_schema_v1_1();
}

fn pointer_parts(pointer: &str) -> Vec<String> {
    pointer
        .split('/')
        .skip(1)
        .map(|p| p.replace("~1", "/").replace("~0", "~"))
        .collect()
}

/// `apply_op` of export_records.py: `replace`, `add` (an object key or `-` to append) and
/// `remove`.
fn apply_op(doc: &mut Value, op: &Value) {
    let kind = op[0].as_str().unwrap();
    let parts = pointer_parts(op[1].as_str().unwrap());
    let (last, parents) = parts.split_last().unwrap();
    let mut parent = doc;
    for p in parents {
        parent = match parent {
            Value::Array(items) => &mut items[p.parse::<usize>().unwrap()],
            Value::Object(map) => map.get_mut(p).unwrap(),
            _ => panic!("bad pointer {op}"),
        };
    }
    match (kind, parent) {
        ("remove", Value::Array(items)) => {
            items.remove(last.parse::<usize>().unwrap());
        }
        ("remove", Value::Object(map)) => {
            map.shift_remove(last);
        }
        (_, Value::Array(items)) if last == "-" => items.push(op[2].clone()),
        (_, Value::Array(items)) => items[last.parse::<usize>().unwrap()] = op[2].clone(),
        (_, Value::Object(map)) => {
            map.insert(last.clone(), op[2].clone());
        }
        _ => panic!("bad op {op}"),
    }
}

#[test]
fn schema_verdicts_match_jsonschema() {
    let v = vectors();
    let cases = v["schema_cases"].as_array().unwrap();
    assert!(cases.len() > 60);
    let (mut valid, mut invalid) = (0, 0);
    for c in cases {
        let base = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/vectors/records")
            .join(c["base"].as_str().unwrap())
            .join("results.json");
        let mut doc: Value = serde_json::from_str(&fs::read_to_string(base).unwrap()).unwrap();
        for op in c["ops"].as_array().unwrap() {
            apply_op(&mut doc, op);
        }
        let ours = validate_results(&json_from_value(&doc), SchemaVersion::V1_1);
        let want = c["valid"].as_bool().unwrap();
        assert_eq!(ours.is_ok(), want, "ops {} gave {ours:?}", c["ops"]);
        if want {
            valid += 1;
        } else {
            invalid += 1;
        }
    }
    assert!(
        valid >= 10 && invalid >= 50,
        "{valid} valid, {invalid} invalid"
    );
}

#[test]
fn a_1_2_document_has_no_schema_yet() {
    assert!(validate_results(&Json::object(), SchemaVersion::V1_2).is_err());
}
