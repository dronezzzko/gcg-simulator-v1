//! Python `repr()` for the value shapes the reference passes to `derive_seed`.

use std::fmt::Write as _;

#[derive(Clone, Copy, Debug)]
pub enum PyRepr<'a> {
    Int(i64),
    UInt(u64),
    Str(&'a str),
    Bool(bool),
    None,
}

/// `repr(s)` for a `str`: single quotes unless the text contains `'` and no `"`.
///
/// # Panics
/// On non-ASCII text: Python's printable test for non-ASCII code points depends on the
/// Unicode database, and no reference seed path contains such text.
#[must_use]
pub fn repr_str(s: &str) -> String {
    assert!(s.is_ascii(), "repr_str supports ASCII text only, got {s:?}");
    let quote = if s.contains('\'') && !s.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut out = String::with_capacity(s.len() + 2);
    out.push(quote);
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c if (' '..='~').contains(&c) => out.push(c),
            c => {
                let _ = write!(out, "\\x{:02x}", u32::from(c));
            }
        }
    }
    out.push(quote);
    out
}

fn repr_one(out: &mut String, v: &PyRepr<'_>) {
    match *v {
        PyRepr::Int(i) => {
            let _ = write!(out, "{i}");
        }
        PyRepr::UInt(u) => {
            let _ = write!(out, "{u}");
        }
        PyRepr::Str(s) => out.push_str(&repr_str(s)),
        PyRepr::Bool(true) => out.push_str("True"),
        PyRepr::Bool(false) => out.push_str("False"),
        PyRepr::None => out.push_str("None"),
    }
}

/// `repr(tuple(items))`, including the trailing comma of a one-element tuple.
#[must_use]
pub fn repr_tuple(items: &[PyRepr<'_>]) -> String {
    let mut out = String::from("(");
    for (i, v) in items.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        repr_one(&mut out, v);
    }
    if items.len() == 1 {
        out.push(',');
    }
    out.push(')');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tuples_match_python() {
        assert_eq!(repr_tuple(&[PyRepr::UInt(5)]), "(5,)");
        assert_eq!(
            repr_tuple(&[PyRepr::UInt(1), PyRepr::Str("match"), PyRepr::Int(-3)]),
            "(1, 'match', -3)"
        );
        assert_eq!(repr_str("it's"), "\"it's\"");
        assert_eq!(repr_str("a'b\"c"), "'a\\'b\"c'");
        assert_eq!(repr_str("x\ty\u{1}"), "'x\\ty\\x01'");
    }
}
