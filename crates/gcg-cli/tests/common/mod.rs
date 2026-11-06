//! Reading `tests/vectors/cli.json` (written by `oracle/export_cli.py`).

#![allow(
    dead_code,
    reason = "each test binary that includes this module uses a different subset"
)]

use std::ffi::OsString;

use gcg_runner::argparse::PyStr;
use serde_json::Value as Json;

/// The vectors: Python 3.15.1 CLI transcripts and parser-helper vectors.
pub fn vectors() -> Json {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/vectors/cli.json");
    let text = std::fs::read_to_string(path).expect("tests/vectors/cli.json is readable");
    serde_json::from_str(&text).expect("tests/vectors/cli.json is JSON")
}

/// A `text_json` value: a string, or `{"cps": [...]}` for text with lone surrogates.
pub fn py_text(v: &Json) -> PyStr {
    match v {
        Json::String(s) => PyStr::from(s.as_str()),
        Json::Object(map) => PyStr::from_code_points(
            map["cps"]
                .as_array()
                .expect("cps list")
                .iter()
                .map(|c| u32::try_from(c.as_u64().expect("code point")).expect("u32"))
                .collect(),
        ),
        other => panic!("not a text value: {other}"),
    }
}

/// [`py_text`] in the exporter's encoding.
pub fn text_json(s: &PyStr) -> Json {
    match s.to_string_opt() {
        Some(text) => Json::String(text),
        None => serde_json::json!({ "cps": s.code_points() }),
    }
}

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex"))
        .collect()
}

/// An `argv` entry: a string, `{"hex": ...}` for raw bytes, or `{"cps": ...}`.
pub fn os_arg(v: &Json) -> OsString {
    use std::os::unix::ffi::OsStringExt as _;
    match v {
        Json::Object(map) if map.contains_key("hex") => {
            OsString::from_vec(unhex(map["hex"].as_str().expect("hex string")))
        }
        other => py_text(other).to_os_string(),
    }
}

pub fn str_of(v: &Json) -> &str {
    v.as_str().expect("string")
}
