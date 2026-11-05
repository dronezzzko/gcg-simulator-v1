//! The converters of the agent-spec overrides (`ai/compare.py:62-75`): Python's `int(text)`,
//! `float(text)` and `_flag(text)`, with CPython 3.15.1's results and `ValueError` texts
//! (vectors: `oracle/export_stats.py`, `int`, `float`, `flag`).
//!
//! `int` and `float` first map every non-ASCII character to ASCII as
//! `_PyUnicode_TransformDecimalAndSpaceToASCII` does (`Objects/unicodeobject.c`): Unicode
//! whitespace becomes a space, a decimal digit (category Nd) its ASCII digit, anything else
//! makes the literal invalid. ASCII characters are kept, so U+001C-U+001F, which `str.isspace`
//! accepts, are not stripped (C `isspace` does not know them).

use gcg_data::py_repr;
use gcg_runner::argparse::{PyInt, PyStr, pyint};
use gcg_runner::reports::pyregex::DIGIT_RANGES;

/// The repr width of CPython's `int()` error, `%.200R` (`Objects/longobject.c`).
const INT_REPR_LIMIT: usize = 200;

/// `int(text)` (base 10).
///
/// # Errors
/// `invalid literal for int() with base 10: {text!r}`, the repr cut to 200 characters.
pub fn py_int(text: &str) -> Result<PyInt, String> {
    pyint::parse_int(&PyStr::from(text), 10).ok_or_else(|| {
        let repr: String = py_repr(text).chars().take(INT_REPR_LIMIT).collect();
        format!("invalid literal for int() with base 10: {repr}")
    })
}

/// `float(text)` (`PyFloat_FromString`, `Objects/floatobject.c`).
///
/// # Errors
/// `could not convert string to float: {text!r}`.
pub fn py_float(text: &str) -> Result<f64, String> {
    parse_float(text).ok_or_else(|| format!("could not convert string to float: {}", py_repr(text)))
}

/// `_flag(text)` (`ai/compare.py:62-67`). `text.lower()` is compared with ASCII words only,
/// and no non-ASCII character lowers to a letter of them (the Kelvin sign lowers to `k`), so
/// ASCII lowercasing gives the same answer.
///
/// # Errors
/// `not a boolean: {text!r}`.
pub fn py_flag(text: &str) -> Result<bool, String> {
    let lower = text.to_ascii_lowercase();
    match lower.as_str() {
        "1" | "true" | "yes" => Ok(true),
        "0" | "false" | "no" => Ok(false),
        _ => Err(format!("not a boolean: {}", py_repr(text))),
    }
}

/// `Py_UNICODE_TODECIMAL` for a decimal digit: every Nd run is a complete `0..=9` sequence,
/// so the value is the offset from the start of its range modulo 10.
fn decimal_value(c: char) -> Option<u8> {
    let cp = u32::from(c);
    DIGIT_RANGES
        .iter()
        .find(|&&(lo, hi)| lo <= cp && cp <= hi)
        .and_then(|&(lo, _)| u8::try_from((cp - lo) % 10).ok())
}

/// `_PyUnicode_TransformDecimalAndSpaceToASCII`; `None` where CPython writes its `?` marker.
fn to_ascii(text: &str) -> Option<Vec<u8>> {
    text.chars()
        .map(|c| {
            if c.is_ascii() {
                u8::try_from(u32::from(c)).ok()
            } else if c.is_whitespace() {
                Some(b' ')
            } else {
                decimal_value(c).map(|d| b'0' + d)
            }
        })
        .collect()
}

/// `Py_ISSPACE`: C-locale whitespace.
const fn is_c_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
}

/// `_Py_string_to_number_with_underscores` (`Objects/abstract.c`): an underscore must follow
/// a digit and precede one; the underscores are removed.
fn without_underscores(s: &[u8]) -> Option<Vec<u8>> {
    if !s.contains(&b'_') {
        return Some(s.to_vec());
    }
    let mut out = Vec::with_capacity(s.len());
    let mut prev = 0_u8;
    for &b in s {
        if b == b'_' {
            if !prev.is_ascii_digit() {
                return None;
            }
        } else {
            if prev == b'_' && !b.is_ascii_digit() {
                return None;
            }
            out.push(b);
        }
        prev = b;
    }
    (prev != b'_').then_some(out)
}

/// The length of the decimal literal `_Py_dg_strtod` reads at the start of `s` (sign,
/// mantissa with at least one digit, and an exponent only when it has digits), or 0.
fn decimal_prefix(s: &[u8]) -> usize {
    let mut i = 0;
    if matches!(s.first(), Some(b'+' | b'-')) {
        i += 1;
    }
    let int_start = i;
    while s.get(i).is_some_and(u8::is_ascii_digit) {
        i += 1;
    }
    let mut digits = i - int_start;
    if s.get(i) == Some(&b'.') {
        let frac_start = i + 1;
        let mut j = frac_start;
        while s.get(j).is_some_and(u8::is_ascii_digit) {
            j += 1;
        }
        digits += j - frac_start;
        if digits > 0 {
            i = j;
        }
    }
    if digits == 0 {
        return 0;
    }
    if matches!(s.get(i), Some(b'e' | b'E')) {
        let mut j = i + 1;
        if matches!(s.get(j), Some(b'+' | b'-')) {
            j += 1;
        }
        let exp_start = j;
        while s.get(j).is_some_and(u8::is_ascii_digit) {
            j += 1;
        }
        if j > exp_start {
            i = j;
        }
    }
    i
}

/// `_Py_parse_inf_or_nan` (`Python/pystrtod.c`): `[sign](inf|infinity|nan)`, case-insensitive,
/// as the whole of `s`.
fn inf_or_nan(s: &[u8]) -> Option<f64> {
    let (negative, rest) = match s.first() {
        Some(b'-') => (true, &s[1..]),
        Some(b'+') => (false, &s[1..]),
        _ => (false, s),
    };
    let word = rest.to_ascii_lowercase();
    let magnitude = match word.as_slice() {
        b"inf" | b"infinity" => f64::INFINITY,
        b"nan" => f64::NAN,
        _ => return None,
    };
    Some(if negative { -magnitude } else { magnitude })
}

fn parse_float(text: &str) -> Option<f64> {
    let ascii = without_underscores(&to_ascii(text)?)?;
    let start = ascii.iter().position(|&b| !is_c_space(b))?;
    let end = ascii.iter().rposition(|&b| !is_c_space(b))? + 1;
    let s = &ascii[start..end];
    if decimal_prefix(s) == s.len() {
        let literal = std::str::from_utf8(s).ok()?;
        return literal.parse::<f64>().ok();
    }
    inf_or_nan(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spot_checks() {
        assert_eq!(py_float("1_0.5"), Ok(10.5));
        assert_eq!(py_float(" \u{0660}.\u{0665}\u{2003}"), Ok(0.5));
        assert!(py_float("-nan").is_ok_and(|x| x.is_nan() && x.is_sign_negative()));
        assert_eq!(
            py_float("1e"),
            Err("could not convert string to float: '1e'".to_owned())
        );
        assert_eq!(py_int(" 12 ").map(|v| v.to_u64()), Ok(Some(12)));
        assert_eq!(py_flag("Yes"), Ok(true));
        assert_eq!(
            py_flag("\u{212a}"),
            Err("not a boolean: '\u{212a}'".to_owned())
        );
        assert_eq!(decimal_value('\u{1D7FF}'), Some(9));
    }
}
