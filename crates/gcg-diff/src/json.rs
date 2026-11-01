//! JSON helpers of the trace reader and the replay: strict object decoding, the §8.2 scalar
//! encodings, a writer byte-compatible with Python's `json.dumps(obj, sort_keys=True,
//! separators=(",", ":"))` (the `state_digest` form, `runner/digest.py:52-56`), and a JSON-path
//! diff for divergence reports.

use std::fmt::Write as _;

use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

/// `2**53`: u64 values at or above it are written as decimal strings (PLAN §8.2).
pub const U53: u64 = 1 << 53;

/// An object being decoded: every key must be read exactly once, so an unknown key from a
/// newer exporter is a decode error, never silently dropped.
pub struct Obj<'a> {
    map: &'a Map<String, Value>,
    used: Vec<&'a str>,
    what: &'static str,
}

impl<'a> Obj<'a> {
    /// # Errors
    /// `v` is not an object.
    pub fn new(v: &'a Value, what: &'static str) -> Result<Self, String> {
        match v {
            Value::Object(map) => Ok(Self {
                map,
                used: Vec::new(),
                what,
            }),
            other => Err(format!("{what}: expected an object, got {}", kind(other))),
        }
    }

    /// The value of `key`, if present (a JSON `null` counts as present).
    pub fn opt(&mut self, key: &'static str) -> Option<&'a Value> {
        let (k, v) = self.map.get_key_value(key)?;
        self.used.push(k.as_str());
        Some(v)
    }

    /// # Errors
    /// `key` is missing.
    pub fn req(&mut self, key: &'static str) -> Result<&'a Value, String> {
        let what = self.what;
        self.opt(key)
            .ok_or_else(|| format!("{what}: missing key {key:?}"))
    }

    /// The value of `key` unless it is missing or `null`.
    pub fn nullable(&mut self, key: &'static str) -> Option<&'a Value> {
        self.opt(key).filter(|v| !v.is_null())
    }

    /// # Errors
    /// A key that no accessor read.
    pub fn finish(self) -> Result<(), String> {
        let mut extra: Vec<&str> = self
            .map
            .keys()
            .map(String::as_str)
            .filter(|k| !self.used.contains(k))
            .collect();
        extra.sort();
        if extra.is_empty() {
            Ok(())
        } else {
            Err(format!("{}: unknown key(s) {extra:?}", self.what))
        }
    }

    pub const fn what(&self) -> &'static str {
        self.what
    }
}

/// The JSON type name of `v`, for messages.
#[must_use]
pub const fn kind(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// # Errors
/// Not an integer in `i64`.
pub fn as_i64(v: &Value, what: &str) -> Result<i64, String> {
    v.as_i64()
        .filter(|_| v.is_i64() || v.is_u64())
        .ok_or_else(|| format!("{what}: expected an integer, got {v}"))
}

/// # Errors
/// Not an integer in the target range.
pub fn as_int<T: TryFrom<i64>>(v: &Value, what: &str) -> Result<T, String> {
    let n = as_i64(v, what)?;
    T::try_from(n).map_err(|_| format!("{what}: {n} is out of range"))
}

/// A u64 in the §8.2 encoding: a JSON number below 2**53, a decimal string at or above it.
///
/// # Errors
/// Any other form, including a number written as a string below 2**53 (the exporter's
/// canonical form is checked, so an encoding bug cannot hide).
pub fn as_u64(v: &Value, what: &str) -> Result<u64, String> {
    match v {
        Value::Number(n) => match n.as_u64() {
            Some(x) if x < U53 => Ok(x),
            Some(x) => Err(format!("{what}: {x} >= 2**53 must be a decimal string")),
            None => Err(format!("{what}: expected a u64, got {n}")),
        },
        Value::String(s) => {
            if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) || s.starts_with('0') {
                return Err(format!("{what}: {s:?} is not a canonical decimal u64"));
            }
            let x: u64 = s
                .parse()
                .map_err(|_| format!("{what}: {s:?} does not fit a u64"))?;
            if x < U53 {
                Err(format!("{what}: {x} < 2**53 must be a JSON number"))
            } else {
                Ok(x)
            }
        }
        other => Err(format!("{what}: expected a u64, got {}", kind(other))),
    }
}

/// # Errors
/// Not a JSON string.
pub fn as_str<'a>(v: &'a Value, what: &str) -> Result<&'a str, String> {
    v.as_str()
        .ok_or_else(|| format!("{what}: expected a string, got {}", kind(v)))
}

/// # Errors
/// Not a JSON bool.
pub fn as_bool(v: &Value, what: &str) -> Result<bool, String> {
    v.as_bool()
        .ok_or_else(|| format!("{what}: expected a bool, got {}", kind(v)))
}

/// # Errors
/// Not a JSON array.
pub fn as_array<'a>(v: &'a Value, what: &str) -> Result<&'a [Value], String> {
    v.as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| format!("{what}: expected an array, got {}", kind(v)))
}

/// # Errors
/// Not `len` lowercase hex digits.
pub fn as_hex<'a>(v: &'a Value, len: usize, what: &str) -> Result<&'a str, String> {
    let s = as_str(v, what)?;
    if s.len() == len && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
        Ok(s)
    } else {
        Err(format!("{what}: {s:?} is not {len} lowercase hex digits"))
    }
}

/// Python's `json.dumps(v, sort_keys=True, separators=(",", ":"), ensure_ascii=True)` for the
/// values a trace holds. Integers are written in full; a float (never part of a canonical
/// state) uses serde_json's shortest form, which matches CPython's `repr` for every finite
/// value it can print without an exponent difference, and is rejected by [`py_dumps_checked`].
#[must_use]
pub fn py_dumps(v: &Value) -> String {
    let mut out = String::new();
    write_value(&mut out, v);
    out
}

/// [`py_dumps`], refusing floats (the canonical state and option lists hold none).
///
/// # Errors
/// The value contains a float.
pub fn py_dumps_checked(v: &Value) -> Result<String, String> {
    if let Some(path) = find_float(v, "$") {
        return Err(format!("float at {path}: canonical JSON holds no floats"));
    }
    Ok(py_dumps(v))
}

fn find_float(v: &Value, path: &str) -> Option<String> {
    match v {
        Value::Number(n) if !(n.is_i64() || n.is_u64()) => Some(path.to_owned()),
        Value::Array(items) => items
            .iter()
            .enumerate()
            .find_map(|(i, x)| find_float(x, &format!("{path}[{i}]"))),
        Value::Object(map) => map
            .iter()
            .find_map(|(k, x)| find_float(x, &format!("{path}.{k}"))),
        _ => None,
    }
}

fn write_value(out: &mut String, v: &Value) {
    match v {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => {
            let _ = write!(out, "{n}");
        }
        Value::String(s) => write_str(out, s),
        Value::Array(items) => {
            out.push('[');
            for (i, x) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_value(out, x);
            }
            out.push(']');
        }
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort_by(|a, b| py_str_cmp(a, b));
            out.push('{');
            for (i, k) in keys.into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_str(out, k);
                out.push(':');
                write_value(out, &map[k.as_str()]);
            }
            out.push('}');
        }
    }
}

/// Python compares `str` by code point; Rust's `str` order is UTF-8 byte order, which is the
/// same order for valid UTF-8.
fn py_str_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    a.cmp(b)
}

/// A JSON string with `ensure_ascii=True` escapes (`json/encoder.py` `py_encode_basestring_ascii`).
fn write_str(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (' '..='~').contains(&c) => out.push(c),
            c => {
                let mut buf = [0u16; 2];
                for unit in c.encode_utf16(&mut buf) {
                    let _ = write!(out, "\\u{unit:04x}");
                }
            }
        }
    }
    out.push('"');
}

/// Lowercase hex SHA-256 of `bytes`.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for b in digest {
        let _ = write!(out, "{b:02x}");
    }
    out
}

/// `state_digest` of a canonical state (`runner/digest.py:52-56`).
///
/// # Errors
/// The state contains a float.
pub fn state_digest(state: &Value) -> Result<String, String> {
    Ok(sha256_hex(py_dumps_checked(state)?.as_bytes()))
}

/// The first difference between two JSON values, as `(path, python, rust)`, in document order
/// (object keys in sorted order, so the report is stable).
#[must_use]
pub fn first_difference(
    python: &Value,
    rust: &Value,
    path: &str,
) -> Option<(String, String, String)> {
    match (python, rust) {
        (Value::Array(a), Value::Array(b)) => {
            for (i, (x, y)) in a.iter().zip(b).enumerate() {
                if let Some(d) = first_difference(x, y, &format!("{path}[{i}]")) {
                    return Some(d);
                }
            }
            if a.len() == b.len() {
                None
            } else {
                Some((
                    format!("{path} (length)"),
                    a.len().to_string(),
                    b.len().to_string(),
                ))
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            let mut keys: Vec<&String> = a.keys().chain(b.keys()).collect();
            keys.sort();
            keys.dedup();
            for k in keys {
                let sub = format!("{path}.{k}");
                match (a.get(k.as_str()), b.get(k.as_str())) {
                    (Some(x), Some(y)) => {
                        if let Some(d) = first_difference(x, y, &sub) {
                            return Some(d);
                        }
                    }
                    (Some(x), None) => return Some((sub, short(x), "(absent)".to_owned())),
                    (None, Some(y)) => return Some((sub, "(absent)".to_owned(), short(y))),
                    (None, None) => {}
                }
            }
            None
        }
        (x, y) if x == y => None,
        (x, y) => Some((path.to_owned(), short(x), short(y))),
    }
}

/// `v` as compact JSON, cut to 120 characters for reports.
#[must_use]
pub fn short(v: &Value) -> String {
    let text = py_dumps(v);
    if text.chars().count() <= 120 {
        text
    } else {
        let cut: String = text.chars().take(117).collect();
        format!("{cut}...")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn u64_encoding_is_canonical() {
        assert_eq!(as_u64(&json!(5), "x"), Ok(5));
        assert_eq!(as_u64(&json!("9007199254740992"), "x"), Ok(U53));
        assert!(as_u64(&json!("5"), "x").is_err());
        assert!(as_u64(&json!(9_007_199_254_740_992_u64), "x").is_err());
        assert!(as_u64(&json!("0123"), "x").is_err());
        assert!(as_u64(&json!(-1), "x").is_err());
        assert_eq!(as_u64(&json!("18446744073709551615"), "x"), Ok(u64::MAX));
        assert!(as_u64(&json!("18446744073709551616"), "x").is_err());
    }

    #[test]
    fn dumps_matches_python_sort_keys_compact_ascii() {
        let v = json!({"b": [1, -2, "\u{2161}"], "a": {"z": null, "y": true}, "\u{e9}": "\"\\\n"});
        assert_eq!(
            py_dumps(&v),
            r#"{"a":{"y":true,"z":null},"b":[1,-2,"\u2161"],"\u00e9":"\"\\\n"}"#
        );
        let astral = json!(["\u{1F600}"]);
        assert_eq!(py_dumps(&astral), r#"["\ud83d\ude00"]"#);
        assert!(py_dumps_checked(&json!({"x": [1.5]})).is_err());
    }

    #[test]
    fn strict_objects_reject_unknown_keys() {
        let v = json!({"a": 1, "b": 2});
        let mut o = Obj::new(&v, "rec").unwrap();
        assert!(o.req("a").is_ok());
        assert_eq!(o.finish(), Err("rec: unknown key(s) [\"b\"]".to_owned()));
    }

    #[test]
    fn first_difference_names_the_path() {
        let a = json!({"cards": [[0, 1, 2], [3, 4, 5]], "turn": 3});
        let b = json!({"cards": [[0, 1, 2], [3, 9, 5]], "turn": 3});
        assert_eq!(
            first_difference(&a, &b, "state"),
            Some((
                "state.cards[1][1]".to_owned(),
                "4".to_owned(),
                "9".to_owned()
            ))
        );
        assert_eq!(first_difference(&a, &a, "state"), None);
    }
}
