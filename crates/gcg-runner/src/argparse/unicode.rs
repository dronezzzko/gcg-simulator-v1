//! Lookups in the CPython 3.15.1 Unicode tables of [`super::unicode_tables`].

use std::cmp::Ordering;

use super::unicode_tables::{DECIMAL, PRINTABLE, SPACE};

fn in_ranges(ranges: &[(u32, u32)], cp: u32) -> bool {
    ranges
        .binary_search_by(|&(lo, hi)| {
            if hi < cp {
                Ordering::Less
            } else if lo > cp {
                Ordering::Greater
            } else {
                Ordering::Equal
            }
        })
        .is_ok()
}

/// `chr(cp).isprintable()` (`Py_UNICODE_ISPRINTABLE`, used by `repr`).
#[must_use]
pub fn is_printable(cp: u32) -> bool {
    in_ranges(PRINTABLE, cp)
}

/// `chr(cp).isspace()` (`Py_UNICODE_ISSPACE`).
#[must_use]
pub fn is_space(cp: u32) -> bool {
    in_ranges(SPACE, cp)
}

/// `Py_UNICODE_TODECIMAL(cp)` when it is a decimal digit (`str.isdecimal()`, the `re`
/// class `\d`).
#[must_use]
pub fn decimal(cp: u32) -> Option<u8> {
    let i = DECIMAL
        .binary_search_by(|&(lo, hi, _)| {
            if hi < cp {
                Ordering::Less
            } else if lo > cp {
                Ordering::Greater
            } else {
                Ordering::Equal
            }
        })
        .ok()?;
    let (lo, _, first) = DECIMAL[i];
    u8::try_from(cp - lo).ok().map(|offset| first + offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spot_checks() {
        assert!(is_printable(u32::from('a')));
        assert!(!is_printable(0x7f));
        assert!(!is_printable(0xa0));
        assert!(is_printable(u32::from('ö')));
        assert!(!is_printable(0xdcff));
        assert!(is_space(u32::from(' ')));
        assert!(is_space(0x1c));
        assert!(is_space(0x3000));
        assert!(!is_space(0x200b));
        assert_eq!(decimal(u32::from('7')), Some(7));
        assert_eq!(decimal(0x663), Some(3));
        assert_eq!(decimal(0xff12), Some(2));
        assert_eq!(decimal(0x1d7cf), Some(1));
        assert_eq!(decimal(u32::from('a')), None);
        assert_eq!(decimal(0xb2), None);
    }
}
