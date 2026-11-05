//! `options_sha16` (PLAN §8.2): the first 16 hex digits of
//! `sha256(json.dumps([o.to_json() for o in options], separators=(",", ":"), ensure_ascii=True))`
//! with `Action.to_json() = [kind.value, a, b, c]` (`engine/state.py:23-24`).

use std::fmt::Write as _;

use gcg_engine::Action;

use crate::json::sha256_hex;

/// The Python JSON text of an option list (`[["kind",a,b,c],...]`; kind values are ASCII).
#[must_use]
pub fn options_json(options: &[Action]) -> String {
    let mut out = String::from("[");
    for (i, o) in options.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(out, "[\"{}\",{},{},{}]", o.kind.as_str(), o.a, o.b, o.c);
    }
    out.push(']');
    out
}

/// §8.2 `options_sha16`.
#[must_use]
pub fn options_sha16(options: &[Action]) -> String {
    sha256_hex(options_json(options).as_bytes())[..16].to_owned()
}
