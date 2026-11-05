//! CPython's Unicode text semantics over the tables that `oracle/export_unicode.py`
//! generates from Python's `unicodedata` (docs/dev/PLAN.md A27).
//!
//! Rust std has no NFKC, no full case folding and a different whitespace set
//! (`char::is_whitespace` lacks U+001C-U+001F), so every Python string method the data layer
//! relies on is reimplemented here: `str.isspace` (regex `\s`), `str.isdecimal` with `int()` values
//! (regex `\d`), `str.splitlines`, `str.strip`/`str.split()`, `unicodedata.normalize("NFKC")`,
//! `str.casefold`, `str.upper`, `str.lower` (with the Final_Sigma rule) and `repr(str)`.

#[rustfmt::skip]
mod tables;

use std::fmt::Write as _;

/// `unicodedata.unidata_version` of the tables (Python 3.15.1: 17.0.0).
#[cfg_attr(
    not(test),
    allow(dead_code, reason = "recorded for diagnostics and the vector test")
)]
pub(crate) const UNIDATA_VERSION: &str = tables::UNIDATA_VERSION;

fn in_ranges(table: &[(u32, u32)], c: char) -> bool {
    let c = u32::from(c);
    table
        .binary_search_by(|&(lo, hi)| {
            if hi < c {
                std::cmp::Ordering::Less
            } else if lo > c {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_ok()
}

fn in_triples(table: &[(u32, u32, u8)], c: char) -> Option<(u32, u8)> {
    let c = u32::from(c);
    table
        .binary_search_by(|&(lo, hi, _)| {
            if hi < c {
                std::cmp::Ordering::Less
            } else if lo > c {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .ok()
        .map(|i| (table[i].0, table[i].2))
}

fn mapped(index: &[(u32, u32, u32)], data: &'static str, c: char) -> Option<&'static str> {
    let c = u32::from(c);
    index
        .binary_search_by_key(&c, |&(cp, _, _)| cp)
        .ok()
        .map(|i| &data[index[i].1 as usize..index[i].2 as usize])
}

/// `str.isspace()` for one character: the set of regex `\s`, `str.strip()` and `str.split()`
/// (includes U+001C-U+001F, which `char::is_whitespace` does not).
pub(crate) fn is_space(c: char) -> bool {
    in_ranges(tables::SPACE, c)
}

/// A separator of `str.splitlines()`.
pub(crate) fn is_line_break(c: char) -> bool {
    in_ranges(tables::LINE_BREAK, c)
}

/// `int(c)` for a character with `str.isdecimal()` (regex `\d`, category Nd); `None` otherwise
///.
pub(crate) fn decimal_value(c: char) -> Option<u32> {
    in_triples(tables::DECIMAL, c).map(|(first, value)| u32::from(value) + (u32::from(c) - first))
}

/// `str.isprintable()` for one character (`Py_UNICODE_ISPRINTABLE`, used by `repr`).
pub(crate) fn is_printable(c: char) -> bool {
    !in_ranges(tables::NOT_PRINTABLE, c)
}

fn ccc(c: char) -> u8 {
    in_triples(tables::CCC, c).map_or(0, |(_, class)| class)
}

/// `str.strip()` with no argument: Python whitespace on both ends.
pub(crate) fn strip(s: &str) -> &str {
    s.trim_matches(is_space)
}

/// `str.split()` with no argument: runs of Python whitespace separate the words, and leading
/// or trailing whitespace yields no empty word.
pub(crate) fn split_whitespace(s: &str) -> Vec<&str> {
    s.split(is_space).filter(|w| !w.is_empty()).collect()
}

/// `str.splitlines()` (keepends false): `\r\n` is one separator; a trailing separator does
/// not start an empty last line.
pub(crate) fn splitlines(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut chars = s.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if !is_line_break(c) {
            continue;
        }
        out.push(&s[start..i]);
        start = i + c.len_utf8();
        if c == '\r'
            && let Some(&(j, '\n')) = chars.peek()
        {
            chars.next();
            start = j + 1;
        }
    }
    if start < s.len() {
        out.push(&s[start..]);
    }
    out
}

// --- NFKC (unicodedata.c nfd_nfkd / nfc_nfkc) --------------------------------------------------

const S_BASE: u32 = 0xAC00;
const L_BASE: u32 = 0x1100;
const V_BASE: u32 = 0x1161;
const T_BASE: u32 = 0x11A7;
const L_COUNT: u32 = 19;
const V_COUNT: u32 = 21;
const T_COUNT: u32 = 28;
const N_COUNT: u32 = V_COUNT * T_COUNT;
const S_COUNT: u32 = L_COUNT * N_COUNT;

fn push_cp(out: &mut Vec<char>, cp: u32) {
    if let Some(c) = char::from_u32(cp) {
        out.push(c);
    }
}

/// Full compatibility decomposition, then canonical ordering (`nfd_nfkd` with `k = 1`).
fn nfkd_chars(s: &str) -> Vec<char> {
    let mut out: Vec<char> = Vec::with_capacity(s.len());
    for c in s.chars() {
        let cp = u32::from(c);
        if (S_BASE..S_BASE + S_COUNT).contains(&cp) {
            let index = cp - S_BASE;
            push_cp(&mut out, L_BASE + index / N_COUNT);
            push_cp(&mut out, V_BASE + (index % N_COUNT) / T_COUNT);
            let t = index % T_COUNT;
            if t != 0 {
                push_cp(&mut out, T_BASE + t);
            }
        } else if let Some(m) = mapped(tables::NFKD, tables::NFKD_DATA, c) {
            out.extend(m.chars());
        } else {
            out.push(c);
        }
    }
    let mut i = 0;
    while i < out.len() {
        if ccc(out[i]) == 0 {
            i += 1;
            continue;
        }
        let start = i;
        while i < out.len() && ccc(out[i]) != 0 {
            i += 1;
        }
        out[start..i].sort_by_key(|&c| ccc(c));
    }
    out
}

fn compose_pair(a: char, b: char) -> Option<char> {
    let (a, b) = (u32::from(a), u32::from(b));
    let key = (a, b);
    tables::COMPOSE
        .binary_search_by_key(&key, |&(x, y, _)| (x, y))
        .ok()
        .and_then(|i| char::from_u32(tables::COMPOSE[i].2))
}

fn is_l(c: char) -> bool {
    (L_BASE..L_BASE + L_COUNT).contains(&u32::from(c))
}

fn is_v(c: char) -> bool {
    (V_BASE..V_BASE + V_COUNT).contains(&u32::from(c))
}

fn is_t(c: char) -> bool {
    let c = u32::from(c);
    T_BASE < c && c < T_BASE + T_COUNT
}

/// `unicodedata.normalize("NFKC", s)` (CPython `nfc_nfkc` over `nfd_nfkd`): adjacent modern
/// Hangul jamo L+V(+T) compose algorithmically; every other base composes with the next
/// unblocked character (no intervening character of class 0 or of a class at least its own).
pub(crate) fn nfkc(s: &str) -> String {
    let data = nfkd_chars(s);
    let len = data.len();
    let mut skipped = vec![false; len];
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < len {
        if skipped[i] {
            i += 1;
            continue;
        }
        let c = data[i];
        if is_l(c) && i + 1 < len && is_v(data[i + 1]) {
            let mut code = S_BASE
                + ((u32::from(c) - L_BASE) * V_COUNT + (u32::from(data[i + 1]) - V_BASE)) * T_COUNT;
            i += 2;
            if i < len && is_t(data[i]) {
                code += u32::from(data[i]) - T_BASE;
                i += 1;
            }
            if let Some(ch) = char::from_u32(code) {
                out.push(ch);
            }
            continue;
        }
        let mut base = c;
        let mut comb = 0u8;
        let mut j = i + 1;
        while j < len {
            if skipped[j] {
                j += 1;
                continue;
            }
            let next = data[j];
            let comb1 = ccc(next);
            if comb != 0 {
                if comb1 == 0 {
                    break;
                }
                if comb >= comb1 {
                    j += 1;
                    continue;
                }
            }
            if let Some(composite) = compose_pair(base, next) {
                base = composite;
                skipped[j] = true;
                j += 1;
                continue;
            }
            if comb1 == 0 {
                break;
            }
            comb = comb1;
            j += 1;
        }
        out.push(base);
        i += 1;
    }
    out
}

// --- case mapping ----------------------------------------------------------------------------

/// `str.casefold()` (full case folding, per character; `ß` → `ss`).
pub(crate) fn casefold(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match mapped(tables::CASEFOLD, tables::CASEFOLD_DATA, c) {
            Some(m) => out.push_str(m),
            None => out.push(c),
        }
    }
    out
}

/// `str.upper()` (full case mapping, per character).
pub(crate) fn upper(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match mapped(tables::UPPER, tables::UPPER_DATA, c) {
            Some(m) => out.push_str(m),
            None => out.push(c),
        }
    }
    out
}

const CAPITAL_SIGMA: char = '\u{3A3}';

/// `handle_capital_sigma` (`unicodeobject.c`): U+03A3 is final when a cased letter precedes it
/// (skipping case-ignorable characters) and none follows it.
fn final_sigma(chars: &[char], i: usize) -> bool {
    let before = chars[..i]
        .iter()
        .rev()
        .find(|&&c| !in_ranges(tables::CASE_IGNORABLE, c));
    let mut is_final = before.is_some_and(|&c| in_ranges(tables::CASED, c));
    if is_final && i + 1 < chars.len() {
        let after = chars[i + 1..]
            .iter()
            .find(|&&c| !in_ranges(tables::CASE_IGNORABLE, c));
        is_final = after.is_none_or(|&c| !in_ranges(tables::CASED, c));
    }
    is_final
}

/// `str.lower()` (full case mapping, per character, with the Final_Sigma context of U+03A3).
pub(crate) fn lower(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    for (i, &c) in chars.iter().enumerate() {
        if c == CAPITAL_SIGMA {
            out.push(if final_sigma(&chars, i) {
                '\u{3C2}'
            } else {
                '\u{3C3}'
            });
            continue;
        }
        match mapped(tables::LOWER, tables::LOWER_DATA, c) {
            Some(m) => out.push_str(m),
            None => out.push(c),
        }
    }
    out
}

// --- repr ------------------------------------------------------------------------------------

/// `repr(s)` for a `str` (CPython `unicode_repr`): single quotes unless the
/// text contains `'` and no `"`; `\\`, the quote, `\t`, `\n`, `\r` escaped; other C0 controls
/// and U+007F as `\xhh`; non-ASCII kept when printable, else `\xhh`, `\uhhhh` or `\Uhhhhhhhh`.
pub(crate) fn repr_str(s: &str) -> String {
    let quote = if s.contains('\'') && !s.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut out = String::with_capacity(s.len() + 2);
    out.push(quote);
    for c in s.chars() {
        let cp = u32::from(c);
        match c {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            _ if cp < 0x20 || cp == 0x7F => {
                let _ = write!(out, "\\x{cp:02x}");
            }
            c if cp < 0x7F || is_printable(c) => out.push(c),
            _ if cp <= 0xFF => {
                let _ = write!(out, "\\x{cp:02x}");
            }
            _ if cp <= 0xFFFF => {
                let _ = write!(out, "\\u{cp:04x}");
            }
            _ => {
                let _ = write!(out, "\\U{cp:08x}");
            }
        }
    }
    out.push(quote);
    out
}

/// `repr(s)` of a Python `str`, exactly as CPython 3.15.1 prints it ([`repr_str`] over the
/// generated `isprintable` table): the public form for the crates that print Python messages
/// with an embedded `repr` (`KeyError`, deck errors).
#[must_use]
pub fn py_repr(s: &str) -> String {
    repr_str(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn vectors() -> Value {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/vectors/unicode.json");
        let text = std::fs::read_to_string(path).expect("unicode vectors (export_unicode.py)");
        serde_json::from_str(&text).expect("unicode vectors parse")
    }

    fn s(v: &Value) -> &str {
        v.as_str().expect("string field")
    }

    fn list(v: &Value) -> Vec<&str> {
        v.as_array()
            .expect("list field")
            .iter()
            .map(|x| x.as_str().expect("string item"))
            .collect()
    }

    #[test]
    fn tables_are_from_the_recorded_unicode_version() {
        assert_eq!(s(&vectors()["unidata_version"]), UNIDATA_VERSION);
        assert_eq!(UNIDATA_VERSION, "17.0.0");
    }

    /// T-037: 10,000 strings; NFKC, casefold, `normalize_name`, `strip_invisible`,
    /// whitespace split/strip and `splitlines` equal Python's.
    #[test]
    fn text_vectors_equal_python() {
        let v = vectors();
        let rows = v["text"].as_array().expect("text vectors");
        assert_eq!(rows.len(), 10_000);
        let mut failures = Vec::new();
        for row in rows {
            let input = s(&row[0]);
            let checks = [
                ("nfkc", nfkc(input) == s(&row[1])),
                ("casefold", casefold(input) == s(&row[2])),
                (
                    "normalize_name",
                    crate::names::normalize_name(input) == s(&row[3]),
                ),
                (
                    "strip_invisible",
                    crate::names::strip_invisible(input) == s(&row[4]),
                ),
                ("split", split_whitespace(input) == list(&row[5])),
                ("strip", strip(input) == s(&row[6])),
                ("splitlines", splitlines(input) == list(&row[7])),
            ];
            for (name, ok) in checks {
                if !ok {
                    failures.push(format!("{name} {input:?}"));
                }
            }
        }
        assert!(
            failures.is_empty(),
            "{} mismatches, first: {:?}",
            failures.len(),
            &failures[..failures.len().min(10)]
        );
    }

    #[test]
    fn case_vectors_equal_python() {
        let v = vectors();
        let rows = v["case"].as_array().expect("case vectors");
        assert!(rows.len() >= 3_000);
        for row in rows {
            let input = s(&row[0]);
            assert_eq!(upper(input), s(&row[1]), "upper {input:?}");
            assert_eq!(lower(input), s(&row[2]), "lower {input:?}");
        }
    }

    #[test]
    fn repr_vectors_equal_python() {
        let v = vectors();
        let rows = v["repr"].as_array().expect("repr vectors");
        assert!(rows.len() >= 2_000);
        for row in rows {
            assert_eq!(repr_str(s(&row[0])), s(&row[1]), "repr {:?}", s(&row[0]));
        }
    }

    #[test]
    fn known_values() {
        assert_eq!(nfkc("Zaku \u{2161}"), "Zaku II");
        assert_eq!(nfkc("\u{FF3A}\u{FF21}"), "ZA");
        assert_eq!(nfkc("\u{1100}\u{1161}\u{11A8}"), "\u{AC01}");
        assert_eq!(casefold("Stra\u{DF}e"), "strasse");
        assert_eq!(lower("A\u{3A3}"), "a\u{3C2}");
        assert_eq!(lower("\u{3A3}A"), "\u{3C3}a");
        assert_eq!(decimal_value('\u{FF14}'), Some(4));
        assert_eq!(decimal_value('\u{663}'), Some(3));
        assert_eq!(decimal_value('x'), None);
        assert!(is_space('\u{1F}') && is_space('\u{3000}') && !is_space('\u{200B}'));
        assert_eq!(splitlines("a\r\nb\rc\u{1C}d\n"), ["a", "b", "c", "d"]);
        assert_eq!(splitlines(""), Vec::<&str>::new());
        assert_eq!(splitlines("\n"), [""]);
        assert_eq!(repr_str("Zaku \u{2161}"), "'Zaku \u{2161}'");
        assert_eq!(
            repr_str("\u{A0}x\u{200E}\u{3000}\u{7F}\t"),
            "'\\xa0x\\u200e\\u3000\\x7f\\t'"
        );
        assert_eq!(repr_str("it's"), "\"it's\"");
        assert_eq!(py_repr("Zaku \u{2161}"), repr_str("Zaku \u{2161}"));
        assert_eq!(repr_str("a'b\"c"), "'a\\'b\"c'");
    }
}
