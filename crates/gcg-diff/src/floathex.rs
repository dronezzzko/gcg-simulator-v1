//! Python's `float.hex()` and `float.fromhex()` for the trace float encoding (PLAN §8.2).
//!
//! `float.hex` (CPython `Objects/floatobject.c` `float_hex`) writes `0x0.0p+0` for zero,
//! `inf`/`nan` for the specials, and otherwise `[-]0x1.<13 hex digits>p<exp>` for normal
//! numbers and `[-]0x0.<13 hex digits>p-1022` for subnormals. The reader accepts exactly the
//! strings the writer produces, so a different spelling in a trace is reported.

/// `float.hex(x)`.
#[must_use]
pub fn to_hex(x: f64) -> String {
    if x.is_nan() {
        return "nan".to_owned();
    }
    if x.is_infinite() {
        return if x > 0.0 { "inf" } else { "-inf" }.to_owned();
    }
    let sign = if x.is_sign_negative() { "-" } else { "" };
    if x == 0.0 {
        return format!("{sign}0x0.0p+0");
    }
    let bits = x.to_bits();
    let exp_bits = ((bits >> 52) & 0x7ff) as i32;
    let mantissa = bits & ((1u64 << 52) - 1);
    let (lead, exp) = if exp_bits == 0 {
        (0, -1022)
    } else {
        (1, exp_bits - 1023)
    };
    let esign = if exp < 0 { '-' } else { '+' };
    format!(
        "{sign}0x{lead}.{mantissa:013x}p{esign}{}",
        exp.unsigned_abs()
    )
}

/// `float.fromhex(s)` restricted to the canonical `float.hex` spellings.
///
/// # Errors
/// A string that `float.hex` would not have produced.
pub fn from_hex(s: &str) -> Result<f64, String> {
    let bad = || format!("{s:?} is not a canonical float.hex() string");
    match s {
        "nan" => return Ok(f64::NAN),
        "inf" => return Ok(f64::INFINITY),
        "-inf" => return Ok(f64::NEG_INFINITY),
        "0x0.0p+0" => return Ok(0.0),
        "-0x0.0p+0" => return Ok(-0.0),
        _ => {}
    }
    let (neg, rest) = match s.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, s),
    };
    let rest = rest.strip_prefix("0x").ok_or_else(bad)?;
    let (lead, rest) = rest.split_at_checked(1).ok_or_else(bad)?;
    let rest = rest.strip_prefix('.').ok_or_else(bad)?;
    let (digits, exp) = rest.split_once('p').ok_or_else(bad)?;
    if digits.len() != 13
        || !digits
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    {
        return Err(bad());
    }
    let mantissa = u64::from_str_radix(digits, 16).map_err(|_| bad())?;
    let (esign, emag) = exp.split_at_checked(1).ok_or_else(bad)?;
    if !matches!(esign, "+" | "-") || emag.is_empty() || (emag.len() > 1 && emag.starts_with('0')) {
        return Err(bad());
    }
    let emag: i32 = emag.parse().map_err(|_| bad())?;
    let exp = if esign == "-" { -emag } else { emag };
    let bits = match lead {
        "1" if (-1022..=1023).contains(&exp) => {
            (u64::try_from(exp + 1023).map_err(|_| bad())? << 52) | mantissa
        }
        "0" if exp == -1022 && mantissa != 0 => mantissa,
        _ => return Err(bad()),
    };
    let x = f64::from_bits(bits);
    Ok(if neg { -x } else { x })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn python_spellings_round_trip() {
        // Values from Python 3.15: float.hex(x) for each x.
        let cases: [(f64, &str); 9] = [
            (1.0, "0x1.0000000000000p+0"),
            (0.5, "0x1.0000000000000p-1"),
            (-2.5, "-0x1.4000000000000p+1"),
            (0.1, "0x1.999999999999ap-4"),
            (f64::MIN_POSITIVE, "0x1.0000000000000p-1022"),
            (5e-324, "0x0.0000000000001p-1022"),
            (f64::MAX, "0x1.fffffffffffffp+1023"),
            (0.0, "0x0.0p+0"),
            (-0.0, "-0x0.0p+0"),
        ];
        for (x, text) in cases {
            assert_eq!(to_hex(x), text);
            assert_eq!(from_hex(text).map(f64::to_bits), Ok(x.to_bits()));
        }
        assert_eq!(
            from_hex("0x1.b00be33f8f4adp-2").map(to_hex).as_deref(),
            Ok("0x1.b00be33f8f4adp-2")
        );
    }

    #[test]
    fn non_canonical_spellings_are_rejected() {
        for text in [
            "0x1.0p+0",
            "1.0",
            "0x1.0000000000000p0",
            "0x2.0000000000000p+0",
            "0x1.0000000000000p+01",
            "0X1.0000000000000p+0",
            "",
        ] {
            assert!(from_hex(text).is_err(), "{text}");
        }
    }
}
