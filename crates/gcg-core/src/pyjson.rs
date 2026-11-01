//! A JSON value and a writer byte-compatible with Python's `json.dumps`.

use std::cmp::Ordering;
use std::fmt::Write as _;

use crate::pyfloat::repr_f64;

/// A dict key: Python sorts the original keys, so integer keys sort numerically.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Key {
    Str(String),
    Int(i128),
}

impl From<&str> for Key {
    fn from(s: &str) -> Self {
        Self::Str(s.to_owned())
    }
}

impl From<String> for Key {
    fn from(s: String) -> Self {
        Self::Str(s)
    }
}

impl Key {
    fn cmp_py(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Self::Str(a), Self::Str(b)) => a.cmp(b),
            (Self::Int(a), Self::Int(b)) => a.cmp(b),
            _ => panic!("sort_keys over mixed str/int keys raises TypeError in Python"),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Int(i128),
    Float(f64),
    Str(String),
    Array(Vec<Json>),
    /// Insertion-ordered, like a Python dict.
    Object(Vec<(Key, Json)>),
}

impl Json {
    #[must_use]
    pub fn object() -> Self {
        Self::Object(Vec::new())
    }

    /// Insert or replace `key` (Python dict assignment keeps the original position).
    ///
    /// # Panics
    /// If `self` is not an object.
    pub fn set(&mut self, key: impl Into<Key>, value: impl Into<Self>) {
        let Self::Object(items) = self else {
            panic!("set on a non-object")
        };
        let key = key.into();
        let value = value.into();
        if let Some(slot) = items.iter_mut().find(|(k, _)| *k == key) {
            slot.1 = value;
        } else {
            items.push((key, value));
        }
    }

    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Self> {
        match self {
            Self::Object(items) => items.iter().find_map(|(k, v)| match k {
                Key::Str(s) if s == key => Some(v),
                _ => None,
            }),
            _ => None,
        }
    }
}

macro_rules! from_int {
    ($($t:ty),*) => {$(
        impl From<$t> for Json {
            fn from(v: $t) -> Self { Self::Int(i128::from(v)) }
        }
    )*};
}
from_int!(i8, i16, i32, i64, u8, u16, u32, u64);

impl From<usize> for Json {
    fn from(v: usize) -> Self {
        Self::Int(i128::try_from(v).expect("usize fits in i128"))
    }
}

impl From<bool> for Json {
    fn from(v: bool) -> Self {
        Self::Bool(v)
    }
}

impl From<f64> for Json {
    fn from(v: f64) -> Self {
        Self::Float(v)
    }
}

impl From<&str> for Json {
    fn from(v: &str) -> Self {
        Self::Str(v.to_owned())
    }
}

impl From<String> for Json {
    fn from(v: String) -> Self {
        Self::Str(v)
    }
}

impl<T: Into<Self>> From<Option<T>> for Json {
    fn from(v: Option<T>) -> Self {
        v.map_or(Self::Null, Into::into)
    }
}

impl<T: Into<Self>> From<Vec<T>> for Json {
    fn from(v: Vec<T>) -> Self {
        Self::Array(v.into_iter().map(Into::into).collect())
    }
}

/// `json.dumps` options that the reference uses.
#[derive(Clone, Copy, Debug)]
pub struct DumpOptions {
    pub indent: Option<usize>,
    pub sort_keys: bool,
    /// `separators=(",", ":")`; otherwise Python's defaults for the indent mode.
    pub compact: bool,
    pub ensure_ascii: bool,
}

impl DumpOptions {
    /// `json.dumps(obj, indent=2, sort_keys=True, ensure_ascii=False)` (results.json).
    pub const PRETTY_SORTED_UTF8: Self = Self {
        indent: Some(2),
        sort_keys: true,
        compact: false,
        ensure_ascii: false,
    };
    /// `json.dumps(obj, sort_keys=True, separators=(",", ":"))` (games.ndjson, digests).
    pub const COMPACT_SORTED_ASCII: Self = Self {
        indent: None,
        sort_keys: true,
        compact: true,
        ensure_ascii: true,
    };
}

fn write_str(out: &mut String, s: &str, ensure_ascii: bool) {
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if u32::from(c) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c if ensure_ascii && !(' '..='~').contains(&c) => {
                let mut buf = [0u16; 2];
                for unit in c.encode_utf16(&mut buf) {
                    let _ = write!(out, "\\u{unit:04x}");
                }
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

fn write_float(out: &mut String, x: f64) {
    if x.is_nan() {
        out.push_str("NaN");
    } else if x.is_infinite() {
        out.push_str(if x > 0.0 { "Infinity" } else { "-Infinity" });
    } else {
        out.push_str(&repr_f64(x));
    }
}

fn newline(out: &mut String, opts: DumpOptions, level: usize) {
    if let Some(n) = opts.indent {
        out.push('\n');
        for _ in 0..n * level {
            out.push(' ');
        }
    }
}

fn write_value(out: &mut String, v: &Json, opts: DumpOptions, level: usize) {
    let item_sep = if opts.compact || opts.indent.is_some() {
        ","
    } else {
        ", "
    };
    let key_sep = if opts.compact { ":" } else { ": " };
    match v {
        Json::Null => out.push_str("null"),
        Json::Bool(true) => out.push_str("true"),
        Json::Bool(false) => out.push_str("false"),
        Json::Int(i) => {
            let _ = write!(out, "{i}");
        }
        Json::Float(x) => write_float(out, *x),
        Json::Str(s) => write_str(out, s, opts.ensure_ascii),
        Json::Array(items) => {
            if items.is_empty() {
                out.push_str("[]");
                return;
            }
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(item_sep);
                }
                newline(out, opts, level + 1);
                write_value(out, item, opts, level + 1);
            }
            newline(out, opts, level);
            out.push(']');
        }
        Json::Object(items) => {
            if items.is_empty() {
                out.push_str("{}");
                return;
            }
            let mut order: Vec<&(Key, Json)> = items.iter().collect();
            if opts.sort_keys {
                order.sort_by(|a, b| a.0.cmp_py(&b.0));
            }
            out.push('{');
            for (i, (k, item)) in order.into_iter().enumerate() {
                if i > 0 {
                    out.push_str(item_sep);
                }
                newline(out, opts, level + 1);
                match k {
                    Key::Str(s) => write_str(out, s, opts.ensure_ascii),
                    Key::Int(n) => {
                        let _ = write!(out, "\"{n}\"");
                    }
                }
                out.push_str(key_sep);
                write_value(out, item, opts, level + 1);
            }
            newline(out, opts, level);
            out.push('}');
        }
    }
}

/// `json.dumps(value, ...)` with the given options.
#[must_use]
pub fn dumps(value: &Json, opts: DumpOptions) -> String {
    let mut out = String::new();
    write_value(&mut out, value, opts, 0);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pretty_and_compact_match_python() {
        let mut o = Json::object();
        o.set("b", vec![1i64, 2]);
        o.set("a", "é\"x");
        o.set("c", Json::object());
        o.set("d", 0.5);
        assert_eq!(
            dumps(&o, DumpOptions::PRETTY_SORTED_UTF8),
            "{\n  \"a\": \"é\\\"x\",\n  \"b\": [\n    1,\n    2\n  ],\n  \"c\": {},\n  \"d\": 0.5\n}"
        );
        assert_eq!(
            dumps(&o, DumpOptions::COMPACT_SORTED_ASCII),
            "{\"a\":\"\\u00e9\\\"x\",\"b\":[1,2],\"c\":{},\"d\":0.5}"
        );
    }

    #[test]
    fn int_keys_sort_numerically() {
        let o = Json::Object(vec![(Key::Int(10), Json::Null), (Key::Int(9), Json::Null)]);
        assert_eq!(
            dumps(&o, DumpOptions::COMPACT_SORTED_ASCII),
            "{\"9\":null,\"10\":null}"
        );
    }
}
