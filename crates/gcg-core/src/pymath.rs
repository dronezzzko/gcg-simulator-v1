//! Python arithmetic semantics that differ from Rust's defaults.

use std::cmp::Ordering;

/// CPython (3.12+) `sum()` of floats: Neumaier compensated summation, compensation added
/// once at the end when finite.
#[must_use]
pub fn py_sum(values: impl IntoIterator<Item = f64>) -> f64 {
    let mut iter = values.into_iter();
    let Some(first) = iter.next() else { return 0.0 };
    // `0 + first` happens in Python's int fast path before the float loop starts.
    let mut f = 0.0 + first;
    let mut c = 0.0f64;
    for x in iter {
        let t = f + x;
        if f.abs() >= x.abs() {
            c += (f - t) + x;
        } else {
            c += (x - t) + f;
        }
        f = t;
    }
    if c != 0.0 && c.is_finite() {
        f += c;
    }
    f
}

/// Python `a // b` for integers (floor division).
///
/// # Panics
/// If `b == 0` (Python raises `ZeroDivisionError`).
#[must_use]
pub const fn floordiv(a: i64, b: i64) -> i64 {
    assert!(b != 0, "integer division by zero");
    let q = a / b;
    if a % b != 0 && ((a < 0) != (b < 0)) {
        q - 1
    } else {
        q
    }
}

/// Python `a % b` for integers (the result has the sign of `b`).
///
/// # Panics
/// If `b == 0`.
#[must_use]
pub const fn pymod(a: i64, b: i64) -> i64 {
    a - b * floordiv(a, b)
}

/// Python `max(items, key=key)`: the FIRST item with the greatest key.
pub fn first_max_by<T, F>(items: impl IntoIterator<Item = T>, mut cmp: F) -> Option<T>
where
    F: FnMut(&T, &T) -> Ordering,
{
    let mut best: Option<T> = None;
    for item in items {
        match &best {
            Some(b) if cmp(&item, b) != Ordering::Greater => {}
            _ => best = Some(item),
        }
    }
    best
}

/// Python `min(items, key=key)`: the FIRST item with the smallest key.
pub fn first_min_by<T, F>(items: impl IntoIterator<Item = T>, mut cmp: F) -> Option<T>
where
    F: FnMut(&T, &T) -> Ordering,
{
    let mut best: Option<T> = None;
    for item in items {
        match &best {
            Some(b) if cmp(&item, b) != Ordering::Less => {}
            _ => best = Some(item),
        }
    }
    best
}

/// Python's `<` on floats as a total order for keys that are never NaN.
///
/// # Panics
/// On NaN (Python comparisons with NaN are all false, which no reference key relies on).
#[must_use]
pub fn cmp_f64(a: f64, b: f64) -> Ordering {
    a.partial_cmp(&b).expect("NaN in an ordered comparison")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floor_division_matches_python() {
        assert_eq!(floordiv(-7, 2), -4);
        assert_eq!(floordiv(7, -2), -4);
        assert_eq!(floordiv(-7, -2), 3);
        assert_eq!(pymod(-7, 2), 1);
        assert_eq!(pymod(7, -2), -1);
    }

    #[test]
    fn neumaier_sum_beats_naive() {
        assert!((py_sum([0.1; 10]) - 1.0).abs() < f64::EPSILON);
        assert_eq!(py_sum([1e100, 1.0, -1e100]), 1.0);
        assert_eq!(py_sum(std::iter::empty()), 0.0);
    }

    #[test]
    fn max_and_min_keep_the_first_tie() {
        let v = [(1, 'a'), (3, 'b'), (3, 'c'), (1, 'd')];
        assert_eq!(first_max_by(v.iter(), |x, y| x.0.cmp(&y.0)).unwrap().1, 'b');
        assert_eq!(first_min_by(v.iter(), |x, y| x.0.cmp(&y.0)).unwrap().1, 'a');
    }
}
