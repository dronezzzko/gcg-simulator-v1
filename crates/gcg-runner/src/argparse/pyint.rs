//! Python `int(text, base)` for `base` 0 and 10, and `str(int)`, over arbitrary-size values.
//!
//! The CLI's integer arguments go through `int(text, 0)` (`cli.py:61-65`), and their error
//! messages print the parsed value (`must be at least 1, got {value}`, `cli.py:40-58`), so
//! values are not bounded by any machine type: `--matches -99999999999999999999` prints the
//! full number. `COLUMNS` is read with `int(text)` (`shutil.get_terminal_size`). Both follow
//! CPython 3.15.1 (`Objects/longobject.c`, `PyLong_FromUnicodeObject`):
//!
//! 1. `_PyUnicode_TransformDecimalAndSpaceToASCII`: code points below 127 stay, other
//!    Unicode whitespace becomes `' '`, other decimal digits become their ASCII digit, and
//!    any other character makes the literal invalid.
//! 2. `PyLong_FromString`: C-locale whitespace (`' '`, `\t`, `\n`, `\v`, `\f`, `\r`) around
//!    the literal, an optional sign, a `0x`/`0o`/`0b` prefix (base 0) followed by at most one
//!    `_`, single underscores between digits, no leading zero in a non-zero base-0 decimal,
//!    and at most 4300 digits for base 10 (`sys.int_info.default_max_str_digits`).
//!
//! `str(value)` fails above 4300 decimal digits ([`PyInt::py_str`] returns `None`).
//! Checked against `int()` vectors exported by `port/oracle/export_cli.py`.

use std::cmp::Ordering;

use super::pystr::PyStr;
use super::unicode;

/// CPython's default `sys.get_int_max_str_digits()`.
pub const MAX_STR_DIGITS: usize = 4300;

/// An integer of any size: sign and magnitude, little-endian base-2^32 limbs without
/// leading zero limbs (zero has no limbs and is never negative).
#[derive(Clone, PartialEq, Eq, Hash, Debug, Default)]
pub struct PyInt {
    negative: bool,
    mag: Vec<u32>,
}

impl PyInt {
    #[must_use]
    pub fn from_u64(v: u64) -> Self {
        let mut out = Self {
            negative: false,
            mag: vec![(v & 0xFFFF_FFFF) as u32, (v >> 32) as u32],
        };
        out.normalize();
        out
    }

    fn normalize(&mut self) {
        while self.mag.last() == Some(&0) {
            self.mag.pop();
        }
        if self.mag.is_empty() {
            self.negative = false;
        }
    }

    #[must_use]
    pub const fn is_negative(&self) -> bool {
        self.negative
    }

    #[must_use]
    pub fn is_zero(&self) -> bool {
        self.mag.is_empty()
    }

    /// `self.bit_length()`.
    #[must_use]
    pub fn bit_length(&self) -> usize {
        match self.mag.last() {
            None => 0,
            Some(&top) => (self.mag.len() - 1) * 32 + (32 - top.leading_zeros() as usize),
        }
    }

    /// The value, if it is in `0..2**64`.
    #[must_use]
    pub fn to_u64(&self) -> Option<u64> {
        if self.negative || self.mag.len() > 2 {
            return None;
        }
        let lo = u64::from(self.mag.first().copied().unwrap_or(0));
        let hi = u64::from(self.mag.get(1).copied().unwrap_or(0));
        Some(lo | (hi << 32))
    }

    /// Compares with a small integer.
    #[must_use]
    pub fn cmp_i64(&self, other: i64) -> Ordering {
        let other_negative = other < 0;
        match (self.negative, other_negative) {
            (false, true) => return Ordering::Greater,
            (true, false) => return Ordering::Less,
            _ => {}
        }
        let magnitude = Self::from_u64(other.unsigned_abs());
        let by_magnitude = self.cmp_magnitude(&magnitude);
        if self.negative {
            by_magnitude.reverse()
        } else {
            by_magnitude
        }
    }

    fn cmp_magnitude(&self, other: &Self) -> Ordering {
        self.mag.len().cmp(&other.mag.len()).then_with(|| {
            for (a, b) in self.mag.iter().rev().zip(other.mag.iter().rev()) {
                match a.cmp(b) {
                    Ordering::Equal => {}
                    unequal => return unequal,
                }
            }
            Ordering::Equal
        })
    }

    /// The decimal text, with a leading `-` for negative values, without CPython's digit
    /// limit.
    #[must_use]
    pub fn to_decimal(&self) -> String {
        if self.mag.is_empty() {
            return "0".to_owned();
        }
        const CHUNK: u64 = 1_000_000_000;
        let mut rest = self.mag.clone();
        let mut chunks = Vec::new();
        while !rest.is_empty() {
            let mut rem = 0_u64;
            for limb in rest.iter_mut().rev() {
                let cur = (rem << 32) | u64::from(*limb);
                *limb = (cur / CHUNK) as u32;
                rem = cur % CHUNK;
            }
            while rest.last() == Some(&0) {
                rest.pop();
            }
            chunks.push(rem);
        }
        let mut out = String::with_capacity(chunks.len() * 9 + 1);
        if self.negative {
            out.push('-');
        }
        let mut iter = chunks.iter().rev();
        if let Some(first) = iter.next() {
            out.push_str(&first.to_string());
        }
        for chunk in iter {
            out.push_str(&format!("{chunk:09}"));
        }
        out
    }

    /// `str(self)`: `None` when the value has more than 4300 decimal digits, where CPython
    /// raises `ValueError` (`Exceeds the limit (4300 digits) for integer string conversion`).
    #[must_use]
    pub fn py_str(&self) -> Option<String> {
        // 2**14400 has more than 4334 digits: no need to convert to know.
        if self.bit_length() > 14_400 {
            return None;
        }
        let text = self.to_decimal();
        let digits = text.len() - usize::from(self.negative);
        (digits <= MAX_STR_DIGITS).then_some(text)
    }

    /// `mag = mag * mul + add` for small factors (base-10 parsing).
    fn mul_add(&mut self, mul: u32, add: u32) {
        let mut carry = u64::from(add);
        for limb in &mut self.mag {
            let cur = u64::from(*limb) * u64::from(mul) + carry;
            *limb = (cur & 0xFFFF_FFFF) as u32;
            carry = cur >> 32;
        }
        if carry > 0 {
            self.mag.push(carry as u32);
        }
    }

    /// The value of `digits` (most significant first) in a power-of-two base, packed bit by
    /// bit from the least significant digit, so a long literal costs linear time.
    fn from_binary_digits(digits: &[u8], bits_per_digit: u32) -> Self {
        let mut out = Self::default();
        let mut acc = 0_u64;
        let mut acc_bits = 0_u32;
        for &d in digits.iter().rev() {
            acc |= u64::from(d) << acc_bits;
            acc_bits += bits_per_digit;
            if acc_bits >= 32 {
                out.mag.push((acc & 0xFFFF_FFFF) as u32);
                acc >>= 32;
                acc_bits -= 32;
            }
        }
        if acc_bits > 0 {
            out.mag.push((acc & 0xFFFF_FFFF) as u32);
        }
        out.normalize();
        out
    }
}

/// `Py_ISSPACE`: the C-locale whitespace that `PyLong_FromString` skips.
const fn is_c_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
}

/// `_PyLong_DigitValue`: 0-9, then letters as 10-35 in either case; 37 for anything else.
const fn digit_value(b: u8) -> u32 {
    match b {
        b'0'..=b'9' => (b - b'0') as u32,
        b'a'..=b'z' => (b - b'a') as u32 + 10,
        b'A'..=b'Z' => (b - b'A') as u32 + 10,
        _ => 37,
    }
}

/// `_PyUnicode_TransformDecimalAndSpaceToASCII`, with `None` where CPython puts its `?`
/// stop marker (any other non-ASCII character makes the literal invalid).
fn to_ascii_digits(text: &PyStr) -> Option<Vec<u8>> {
    text.code_points()
        .iter()
        .map(|&cp| {
            if cp < 127 {
                u8::try_from(cp).ok()
            } else if unicode::is_space(cp) {
                Some(b' ')
            } else {
                unicode::decimal(cp).map(|d| b'0' + d)
            }
        })
        .collect()
}

/// `int(text, base)` for `base` 0 or 10; `None` where CPython raises `ValueError` (an invalid
/// literal, or more than 4300 digits in base 10).
///
/// # Panics
/// For a base other than 0 or 10 (the CLI uses no other).
#[must_use]
pub fn parse_int(text: &PyStr, base: u32) -> Option<PyInt> {
    assert!(
        base == 0 || base == 10,
        "int() base {base} is not used by the CLI"
    );
    let bytes = to_ascii_digits(text)?;
    let mut i = 0;
    while i < bytes.len() && is_c_space(bytes[i]) {
        i += 1;
    }
    let mut negative = false;
    match bytes.get(i) {
        Some(b'+') => i += 1,
        Some(b'-') => {
            negative = true;
            i += 1;
        }
        _ => {}
    }
    let at = |k: usize| bytes.get(k).copied().unwrap_or(0);
    let mut base = base;
    let mut error_if_nonzero = false;
    if base == 0 {
        base = if at(i) != b'0' {
            10
        } else {
            match at(i + 1) {
                b'x' | b'X' => 16,
                b'o' | b'O' => 8,
                b'b' | b'B' => 2,
                _ => {
                    // "old" C-style octal: invalid unless the value is zero.
                    error_if_nonzero = true;
                    10
                }
            }
        };
        if base != 10 {
            i += 2;
            if at(i) == b'_' {
                i += 1;
            }
        }
    }
    // long_from_string_base: digits and single underscores, then trailing whitespace only.
    let start = i;
    if at(start) == b'_' {
        return None;
    }
    let mut digits: Vec<u8> = Vec::with_capacity(bytes.len() - start);
    let mut prev = 0_u8;
    while i < bytes.len() && (digit_value(bytes[i]) < base || bytes[i] == b'_') {
        if bytes[i] == b'_' {
            if prev == b'_' {
                return None;
            }
        } else {
            digits.push(u8::try_from(digit_value(bytes[i])).ok()?);
        }
        prev = bytes[i];
        i += 1;
    }
    if prev == b'_' || i == start {
        return None;
    }
    while i < bytes.len() && is_c_space(bytes[i]) {
        i += 1;
    }
    if i != bytes.len() {
        return None;
    }
    let mut value = match base {
        16 => PyInt::from_binary_digits(&digits, 4),
        8 => PyInt::from_binary_digits(&digits, 3),
        2 => PyInt::from_binary_digits(&digits, 1),
        _ => {
            if digits.len() > MAX_STR_DIGITS {
                return None;
            }
            let mut v = PyInt::default();
            for chunk in digits.chunks(9) {
                let mut part = 0_u32;
                let mut scale = 1_u32;
                for &d in chunk {
                    part = part * 10 + u32::from(d);
                    scale *= 10;
                }
                v.mul_add(scale, part);
            }
            v.normalize();
            v
        }
    };
    if error_if_nonzero && !value.is_zero() {
        return None;
    }
    value.negative = negative;
    value.normalize();
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn int0(s: &str) -> Option<String> {
        parse_int(&PyStr::from(s), 0).map(|v| v.to_decimal())
    }

    #[test]
    fn literals() {
        assert_eq!(int0("0x_1f").as_deref(), Some("31"));
        assert_eq!(int0("-0").as_deref(), Some("0"));
        assert_eq!(int0("010"), None);
        assert_eq!(int0("0_0").as_deref(), Some("0"));
        assert_eq!(int0("1__0"), None);
        assert_eq!(
            int0("18446744073709551616").as_deref(),
            Some("18446744073709551616")
        );
        assert_eq!(
            parse_int(&PyStr::from("010"), 10)
                .map(|v| v.to_decimal())
                .as_deref(),
            Some("10")
        );
    }

    #[test]
    fn comparisons_and_conversions() {
        let big = parse_int(&PyStr::from("-99999999999999999999"), 0).expect("int");
        assert_eq!(big.cmp_i64(1), Ordering::Less);
        assert_eq!(big.cmp_i64(i64::MIN), Ordering::Less);
        let max = parse_int(&PyStr::from("0xffffffffffffffff"), 0).expect("int");
        assert_eq!(max.to_u64(), Some(u64::MAX));
        assert_eq!(max.cmp_i64(i64::MAX), Ordering::Greater);
        assert_eq!(PyInt::from_u64(5).cmp_i64(5), Ordering::Equal);
        assert_eq!(PyInt::from_u64(0).bit_length(), 0);
        assert_eq!(max.bit_length(), 64);
    }
}
