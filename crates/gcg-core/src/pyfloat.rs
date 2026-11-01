//! Python float formatting: `repr(float)`, `format(x, ".Nf")` and `round(x, n)`.

/// Shortest round-trip decimal digits of a finite, non-zero `x` and the decimal point
/// position, so that `|x| = 0.d1d2...dn * 10^decpt` (David Gay's mode 0, which `repr` uses).
///
/// The digits come from zmij (Schubfach), which breaks exact ties to even like CPython; Rust's
/// own `{:e}` rounds such ties up.
fn shortest_digits(x: f64) -> (String, i32) {
    let mut buf = zmij::Buffer::new();
    let text = buf.format_finite(x.abs());
    let (mantissa, exp) = match text.split_once(['e', 'E']) {
        Some((m, e)) => (m, e.parse::<i32>().expect("integer exponent")),
        None => (text, 0),
    };
    let (int_part, frac_part) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let mut digits: String = format!("{int_part}{frac_part}");
    let mut point = i32::try_from(int_part.len()).expect("short mantissa");
    let leading = digits.len() - digits.trim_start_matches('0').len();
    digits.drain(..leading);
    point -= i32::try_from(leading).expect("short mantissa");
    let kept = digits.trim_end_matches('0').len();
    digits.truncate(kept);
    (digits, point + exp)
}

/// `repr(x)` / `str(x)` for a Python float.
#[must_use]
pub fn repr_f64(x: f64) -> String {
    if x.is_nan() {
        return "nan".to_owned();
    }
    if x.is_infinite() {
        return if x > 0.0 {
            "inf".to_owned()
        } else {
            "-inf".to_owned()
        };
    }
    let mut out = String::new();
    if x.is_sign_negative() {
        out.push('-');
    }
    if x == 0.0 {
        out.push_str("0.0");
        return out;
    }
    let (digits, decpt) = shortest_digits(x);
    let n = i32::try_from(digits.len()).expect("few digits");
    if decpt <= -4 || decpt > 16 {
        out.push_str(&digits[..1]);
        if n > 1 {
            out.push('.');
            out.push_str(&digits[1..]);
        }
        let e = decpt - 1;
        out.push('e');
        out.push(if e < 0 { '-' } else { '+' });
        out.push_str(&format!("{:02}", e.abs()));
    } else if decpt <= 0 {
        out.push_str("0.");
        for _ in 0..(-decpt) {
            out.push('0');
        }
        out.push_str(&digits);
    } else if decpt < n {
        let d = usize::try_from(decpt).expect("positive");
        out.push_str(&digits[..d]);
        out.push('.');
        out.push_str(&digits[d..]);
    } else {
        out.push_str(&digits);
        for _ in 0..(decpt - n) {
            out.push('0');
        }
        out.push_str(".0");
    }
    out
}

/// `format(x, f".{prec}f")`: exact decimal value rounded half-to-even at `prec` digits.
#[must_use]
pub fn format_fixed(x: f64, prec: usize) -> String {
    if x.is_nan() {
        return "nan".to_owned();
    }
    if x.is_infinite() {
        return if x > 0.0 {
            "inf".to_owned()
        } else {
            "-inf".to_owned()
        };
    }
    format!("{x:.prec$}")
}

/// `round(x, ndigits)` for a Python float (`ndigits >= 0`).
#[must_use]
pub fn round_f64(x: f64, ndigits: usize) -> f64 {
    if !x.is_finite() {
        return x;
    }
    format_fixed(x, ndigits)
        .parse()
        .expect("formatted float parses")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repr_matches_python_examples() {
        let cases: &[(f64, &str)] = &[
            (0.0, "0.0"),
            (-0.0, "-0.0"),
            (1.0, "1.0"),
            (0.1, "0.1"),
            (1e-5, "1e-05"),
            (0.0001, "0.0001"),
            (1e16, "1e+16"),
            (1e15, "1000000000000000.0"),
            (1.5e16, "1.5e+16"),
            (123.456, "123.456"),
            (0.5774, "0.5774"),
            (2.0 / 3.0, "0.6666666666666666"),
            (1e100, "1e+100"),
            (f64::MAX, "1.7976931348623157e+308"),
            (5e-324, "5e-324"),
            (785_081_882_490_962.0 + 0.25, "785081882490962.2"),
            (1e-7, "1e-07"),
            (123_456_789.0, "123456789.0"),
        ];
        for &(x, want) in cases {
            assert_eq!(repr_f64(x), want, "repr({x:e})");
        }
    }
}
