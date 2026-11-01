//! Python semantics on JSON-loaded values and paths that the data layer reproduces: `str()`,
//! `repr()`, truthiness, `int()`, `int == value`, `str(pathlib.Path(p))` and `PurePath.stem`
//! (Inv 03 §10 H1, H4, H9, H12).

use std::fmt::Write as _;

use gcg_core::pyfloat::repr_f64;
use gcg_core::pyjson::{Json, Key};
use serde_json::Value;

use crate::unicode;

/// Python truthiness of a `json.loads` value (`None`, `False`, `0`, `0.0`, `""`, `[]` and
/// `{}` are false).
pub(crate) fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|x| x != 0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

/// `d.get(key)` treating a missing key and a non-object `d` as `None`.
pub(crate) fn get<'a>(d: &'a Value, key: &str) -> Option<&'a Value> {
    d.as_object().and_then(|o| o.get(key))
}

/// `d.get(key)` when it is truthy.
pub(crate) fn get_truthy<'a>(d: &'a Value, key: &str) -> Option<&'a Value> {
    get(d, key).filter(|v| truthy(v))
}

fn number_str(n: &serde_json::Number) -> String {
    if let Some(i) = n.as_i64() {
        i.to_string()
    } else if let Some(u) = n.as_u64() {
        u.to_string()
    } else {
        repr_f64(n.as_f64().unwrap_or(f64::NAN))
    }
}

/// `str(v)` of a `json.loads` value.
pub(crate) fn str_of(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        _ => repr_of(v),
    }
}

/// `repr(v)` of a `json.loads` value (lists and dicts as Python prints them).
pub(crate) fn repr_of(v: &Value) -> String {
    match v {
        Value::Null => "None".to_owned(),
        Value::Bool(true) => "True".to_owned(),
        Value::Bool(false) => "False".to_owned(),
        Value::Number(n) => number_str(n),
        Value::String(s) => unicode::repr_str(s),
        Value::Array(items) => {
            let parts: Vec<String> = items.iter().map(repr_of).collect();
            format!("[{}]", parts.join(", "))
        }
        Value::Object(map) => {
            let parts: Vec<String> = map
                .iter()
                .map(|(k, x)| format!("{}: {}", unicode::repr_str(k), repr_of(x)))
                .collect();
            format!("{{{}}}", parts.join(", "))
        }
    }
}

/// `isinstance(v, int)` (a JSON boolean is an `int` in Python: `True` is 1).
pub(crate) fn as_py_int(v: &Value) -> Option<i128> {
    match v {
        Value::Bool(b) => Some(i128::from(*b)),
        Value::Number(n) => n
            .as_i64()
            .map(i128::from)
            .or_else(|| n.as_u64().map(i128::from)),
        _ => None,
    }
}

/// `int(s)` for a `str` in base 10: surrounding whitespace, one optional sign, decimal digits
/// of any script (Inv 03 §10 H4) with single underscores between them. `None` where Python
/// raises `ValueError` and where the value does not fit an `i128`.
pub(crate) fn int_of_str(s: &str) -> Option<i128> {
    let t = unicode::strip(s);
    let (negative, digits) = match t.chars().next() {
        Some('-') => (true, &t[1..]),
        Some('+') => (false, &t[1..]),
        _ => (false, t),
    };
    let mut value: i128 = 0;
    let mut seen_digit = false;
    let mut last_underscore = false;
    for c in digits.chars() {
        if c == '_' {
            if !seen_digit || last_underscore {
                return None;
            }
            last_underscore = true;
            continue;
        }
        let d = unicode::decimal_value(c)?;
        value = value.checked_mul(10)?.checked_add(i128::from(d))?;
        seen_digit = true;
        last_underscore = false;
    }
    if !seen_digit || last_underscore {
        return None;
    }
    Some(if negative { -value } else { value })
}

/// `int(v)` of a `json.loads` value: integers and booleans as they are, floats truncated,
/// strings parsed by [`int_of_str`]; `None` where Python raises.
pub(crate) fn int_of(v: &Value) -> Option<i128> {
    match v {
        Value::Bool(_) => as_py_int(v),
        Value::Number(n) => as_py_int(v).or_else(|| {
            // int(float) truncates toward zero; the range check keeps the cast exact.
            let x = n.as_f64()?;
            (x.is_finite() && x.abs() < 1e38).then(|| x.trunc() as i128)
        }),
        Value::String(s) => int_of_str(s),
        _ => None,
    }
}

/// `n == v` for a Python `int` `n` and a `json.loads` value `v` (`2 == 2.0`, `1 == True`).
pub(crate) fn int_eq(n: i64, v: &Value) -> bool {
    match v {
        Value::Bool(b) => n == i64::from(*b),
        Value::Number(x) => {
            if let Some(i) = x.as_i64() {
                n == i
            } else if let Some(u) = x.as_u64() {
                u64::try_from(n).is_ok_and(|m| m == u)
            } else {
                // Python compares int and float exactly; callers pass i32 card stats, which
                // f64 represents exactly.
                let m = n as f64;
                x.as_f64().is_some_and(|f| f == m)
            }
        }
        _ => false,
    }
}

/// A `json.loads` value as the reports' [`Json`] (integers stay integers, key order kept).
pub(crate) fn to_json(v: &Value) -> Json {
    match v {
        Value::Null => Json::Null,
        Value::Bool(b) => Json::Bool(*b),
        Value::Number(n) => match as_py_int(v) {
            Some(i) => Json::Int(i),
            None => Json::Float(n.as_f64().unwrap_or(f64::NAN)),
        },
        Value::String(s) => Json::Str(s.clone()),
        Value::Array(items) => Json::Array(items.iter().map(to_json).collect()),
        Value::Object(map) => Json::Object(
            map.iter()
                .map(|(k, x)| (Key::Str(k.clone()), to_json(x)))
                .collect(),
        ),
    }
}

/// `str(pathlib.PurePosixPath(p))` (Inv 03 §10 H12): `.` components and repeated or trailing
/// slashes dropped, exactly two leading slashes kept, `"."` for an empty result.
pub(crate) fn path_str(p: &str) -> String {
    let (root, rel) = if !p.starts_with('/') {
        ("", p)
    } else if p.starts_with("//") && !p.starts_with("///") {
        ("//", &p[2..])
    } else {
        ("/", p.trim_start_matches('/'))
    };
    let parts: Vec<&str> = rel
        .split('/')
        .filter(|x| !x.is_empty() && *x != ".")
        .collect();
    let out = format!("{root}{}", parts.join("/"));
    if out.is_empty() { ".".to_owned() } else { out }
}

/// `PurePath.name` of an already normalised path string.
fn path_name(normalised: &str) -> &str {
    if normalised == "." || normalised.trim_start_matches('/').is_empty() {
        return "";
    }
    normalised.rsplit('/').next().unwrap_or("")
}

/// `PurePath(p).stem` (Python 3.14+): the name without its last suffix, where a stem must keep
/// at least one non-dot character (`x.` → `x`, `.hidden` → `.hidden`, `..` → `..`).
pub(crate) fn path_stem(p: &str) -> String {
    let normalised = path_str(p);
    let name = path_name(&normalised);
    if let Some(i) = name.rfind('.') {
        let stem = &name[..i];
        if !stem.trim_start_matches('.').is_empty() {
            return stem.to_owned();
        }
    }
    name.to_owned()
}

/// Lowercase hex of a digest.
pub(crate) fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(out, "{b:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_match_python_315() {
        // Values printed by Python 3.15.1 (str(Path(p)), Path(p).stem).
        let cases = [
            ("x.", "x.", "x"),
            ("x..txt", "x..txt", "x."),
            (".hidden", ".hidden", ".hidden"),
            (".x.txt", ".x.txt", ".x"),
            ("..", "..", ".."),
            ("a..", "a..", "a."),
            ("//a//b/", "//a/b", "b"),
            ("./a//b.txt", "a/b.txt", "b"),
            ("", ".", ""),
            (".", ".", ""),
            ("/", "/", ""),
            ("///x", "/x", "x"),
            ("a/./b", "a/b", "b"),
        ];
        for (p, s, stem) in cases {
            assert_eq!(path_str(p), s, "str(Path({p:?}))");
            assert_eq!(path_stem(p), stem, "Path({p:?}).stem");
        }
    }

    #[test]
    fn python_int_and_str() {
        assert_eq!(int_of_str(" +1_0 "), Some(10));
        assert_eq!(int_of_str("\u{FF14}"), Some(4));
        assert_eq!(int_of_str("1__0"), None);
        assert_eq!(int_of_str("_1"), None);
        assert_eq!(int_of_str(""), None);
        assert_eq!(int_of_str("-007"), Some(-7));
        assert_eq!(int_of(&serde_json::json!(2.9)), Some(2));
        assert_eq!(int_of(&serde_json::json!(true)), Some(1));
        assert!(int_eq(2, &serde_json::json!(2.0)));
        assert!(int_eq(1, &serde_json::json!(true)));
        assert!(!int_eq(1, &serde_json::json!("1")));
        assert_eq!(str_of(&serde_json::json!(null)), "None");
        assert_eq!(str_of(&serde_json::json!(["a", 1, true])), "['a', 1, True]");
        assert_eq!(str_of(&serde_json::json!({"k": "it's"})), "{'k': \"it's\"}");
        assert_eq!(str_of(&serde_json::json!(1.5)), "1.5");
        assert!(!truthy(&serde_json::json!(0.0)) && truthy(&serde_json::json!("x")));
    }
}
