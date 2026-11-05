//! Deterministic statistics helpers (`reports/stats.py`): Wilson score intervals, rounded rate
//! blocks, ratios, the pooled two-proportion z-test, the Holm-Bonferroni step-down and
//! nearest-rank distributions. Every operation runs on IEEE `f64` in Python's order.
//!
//! Python's `int / int` is correctly rounded. Here both operands are counts of games or copies
//! (at most `u32::MAX` matches of three games), far below `2^53`, so `a as f64 / b as f64` is
//! the same correctly rounded quotient.

use gcg_core::libm;
use gcg_core::pyfloat::round_f64;
use gcg_core::pyjson::Json;

/// `DIGITS` (`reports/stats.py:9`).
pub const DIGITS: usize = 6;

/// The default `z` of `wilson` (`reports/stats.py:12`).
pub const Z95: f64 = 1.96;

/// Python's true division of two counts (`int / int`), exact below `2^53`.
#[must_use]
#[expect(
    clippy::cast_precision_loss,
    reason = "counts stay below 2^53, where the conversion is exact (module docs)"
)]
pub fn div(num: u64, den: u64) -> f64 {
    num as f64 / den as f64
}

#[expect(
    clippy::cast_precision_loss,
    reason = "Python converts the int operand of a float operation the same way (round to nearest)"
)]
const fn float(x: u128) -> f64 {
    x as f64
}

/// `wilson(successes, n, z)` (`reports/stats.py:12-25`): the Wilson score interval, `(0.0, 1.0)`
/// when `n == 0`. `None` is Python's `ValueError` (`successes > n`; negative counts cannot be
/// represented). F's statistics use it too.
#[must_use]
pub fn wilson(successes: u64, n: u64, z: f64) -> Option<(f64, f64)> {
    if successes > n {
        return None;
    }
    if n == 0 {
        return Some((0.0, 1.0));
    }
    let wide = u128::from(n);
    let p = div(successes, n);
    let z2 = z * z;
    let denom = 1.0 + z2 / float(wide);
    let centre = (p + z2 / float(2 * wide)) / denom;
    let half = z * libm::sqrt(p * (1.0 - p) / float(wide) + z2 / float(4 * wide * wide)) / denom;
    let lo = if successes == 0 {
        0.0
    } else {
        py_max_zero(centre - half)
    };
    let hi = if successes == n {
        1.0
    } else {
        py_min_one(centre + half)
    };
    Some((lo, hi))
}

/// `max(0.0, v)`: Python keeps the first argument unless `v > 0.0` (`reports/stats.py:23`).
fn py_max_zero(v: f64) -> f64 {
    if v > 0.0 { v } else { 0.0 }
}

/// `min(1.0, v)`: Python keeps the first argument unless `v < 1.0` (`reports/stats.py:24`).
fn py_min_one(v: f64) -> f64 {
    if v < 1.0 { v } else { 1.0 }
}

/// `rnd(x) = round(x, 6)` (`reports/stats.py:28-29`).
#[must_use]
pub fn rnd(x: f64) -> f64 {
    round_f64(x, DIGITS)
}

/// `ratio(num, den)` (`reports/stats.py:32-33`): `rnd(num / den)`, or `None` when `den == 0`.
#[must_use]
pub fn ratio(num: u64, den: u64) -> Option<f64> {
    (den != 0).then(|| rnd(div(num, den)))
}

/// The block of `rate(successes, n)` (`reports/stats.py:36-41`): `{"successes", "n", "rate",
/// "ci95"}`, with `rate` and `ci95` `None` when `n == 0`. Values are already rounded.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Rate {
    pub successes: u64,
    pub n: u64,
    pub rate: Option<f64>,
    pub ci95: Option<(f64, f64)>,
}

impl Rate {
    /// `rate(successes, n)` (`reports/stats.py:36-41`).
    ///
    /// # Panics
    /// If `successes > n` (Python's `wilson` raises `ValueError`); every caller passes a count
    /// of a subset of the `n` games.
    #[must_use]
    pub fn new(successes: u64, n: u64) -> Self {
        if n == 0 {
            return Self {
                successes,
                n: 0,
                rate: None,
                ci95: None,
            };
        }
        let (lo, hi) = wilson(successes, n, Z95).expect("successes count a subset of the n games");
        Self {
            successes,
            n,
            rate: Some(rnd(div(successes, n))),
            ci95: Some((rnd(lo), rnd(hi))),
        }
    }

    /// The dict, keys in Python's insertion order.
    #[must_use]
    pub fn to_json(&self) -> Json {
        let mut o = Json::object();
        o.set("successes", self.successes);
        o.set("n", self.n);
        o.set("rate", self.rate);
        o.set(
            "ci95",
            self.ci95.map_or(Json::Null, |(lo, hi)| {
                Json::Array(vec![lo.into(), hi.into()])
            }),
        );
        o
    }
}

/// `intervals_overlap(a, b)` (`reports/stats.py:44-47`) on the rounded intervals; `true` when
/// either is `None`.
#[must_use]
pub fn intervals_overlap(a: &Rate, b: &Rate) -> bool {
    match (a.ci95, b.ci95) {
        (Some((alo, ahi)), Some((blo, bhi))) => alo <= bhi && blo <= ahi,
        _ => true,
    }
}

/// `two_proportion_p(a, b)` (`reports/stats.py:50-60`): the two-sided p-value of the pooled
/// two-proportion z-test on the raw counts. `math.erfc` is the platform libm's
/// (`gcg_core::libm::erfc`, PLAN A2).
#[must_use]
pub fn two_proportion_p(a: &Rate, b: &Rate) -> f64 {
    let (n1, n2) = (a.n, b.n);
    if n1 == 0 || n2 == 0 {
        return 1.0;
    }
    let pooled = div(a.successes + b.successes, n1 + n2);
    let se = libm::sqrt(pooled * (1.0 - pooled) * (div(1, n1) + div(1, n2)));
    if se == 0.0 {
        return 1.0;
    }
    let z = (div(a.successes, n1) - div(b.successes, n2)).abs() / se;
    libm::erfc(z / libm::sqrt(2.0))
}

/// `holm(p_values, alpha)` (`reports/stats.py:63-72`): Holm-Bonferroni step-down. Indices are
/// visited in `(p, i)` order; the walk stops at the first `p > alpha / (m - rank)`, so equality
/// passes.
///
/// # Panics
/// On a NaN p-value (Python's sort would order it arbitrarily; none is ever produced).
#[must_use]
pub fn holm(p_values: &[f64], alpha: f64) -> Vec<bool> {
    let mut order: Vec<usize> = (0..p_values.len()).collect();
    order.sort_by(|&i, &j| {
        p_values[i]
            .partial_cmp(&p_values[j])
            .expect("p-values are never NaN")
            .then(i.cmp(&j))
    });
    let mut passed = vec![false; p_values.len()];
    for (rank, &i) in order.iter().enumerate() {
        let left = u64::try_from(p_values.len() - rank).expect("a list length fits in u64");
        if p_values[i] > alpha / float(u128::from(left)) {
            break;
        }
        passed[i] = true;
    }
    passed
}

/// `percentile(sorted_values, q)` (`reports/stats.py:75-78`): nearest rank,
/// `max(1, ceil(q * len))` with the product in `f64`.
///
/// # Panics
/// If `sorted_values` is empty (Python raises `IndexError`; `distribution` never calls it so).
#[must_use]
pub fn percentile(sorted_values: &[i64], q: f64) -> i64 {
    let len = u64::try_from(sorted_values.len()).expect("a list length fits in u64");
    let product = (q * float(u128::from(len))).ceil();
    let rank = if product > 1.0 {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "an integral value in [1, len]: q <= 1 in every caller"
        )]
        let r = product as usize;
        r
    } else {
        1
    };
    sorted_values[rank - 1]
}

/// `distribution(values)` (`reports/stats.py:81-101`).
#[derive(Clone, PartialEq, Debug)]
pub struct Distribution {
    pub n: u64,
    pub mean: Option<f64>,
    pub median: Option<i64>,
    pub min: Option<i64>,
    pub max: Option<i64>,
    pub p10: Option<i64>,
    pub p90: Option<i64>,
}

impl Distribution {
    /// `distribution(values)` (`reports/stats.py:81-101`): all `None` when empty, else the
    /// rounded mean, the lower median and the nearest-rank p10/p90.
    #[must_use]
    pub fn of(values: &[i64]) -> Self {
        if values.is_empty() {
            return Self {
                n: 0,
                mean: None,
                median: None,
                min: None,
                max: None,
                p10: None,
                p90: None,
            };
        }
        let mut s = values.to_vec();
        s.sort();
        let n = u64::try_from(s.len()).expect("a list length fits in u64");
        let total: i128 = s.iter().map(|&v| i128::from(v)).sum();
        #[expect(
            clippy::cast_precision_loss,
            reason = "Python's int / int: exact below 2^53 (sums of turn counts)"
        )]
        let mean = total as f64 / float(u128::from(n));
        Self {
            n,
            mean: Some(rnd(mean)),
            median: Some(percentile(&s, 0.5)),
            min: s.first().copied(),
            max: s.last().copied(),
            p10: Some(percentile(&s, 0.1)),
            p90: Some(percentile(&s, 0.9)),
        }
    }

    /// The dict, keys in Python's insertion order.
    #[must_use]
    pub fn to_json(&self) -> Json {
        let mut o = Json::object();
        o.set("n", self.n);
        o.set("mean", self.mean);
        o.set("median", self.median);
        o.set("min", self.min);
        o.set("max", self.max);
        o.set("p10", self.p10);
        o.set("p90", self.p90);
        o
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_values_of_tests_reports_test_stats() {
        let (lo, hi) = wilson(5, 10, Z95).unwrap();
        assert!((lo - 0.236_589_6).abs() < 1e-6 && (hi - 0.763_410_4).abs() < 1e-6);
        let (lo, hi) = wilson(0, 20, Z95).unwrap();
        assert_eq!(lo, 0.0);
        assert!((hi - 0.161_130_1).abs() < 1e-6);
        assert_eq!(wilson(0, 0, Z95), Some((0.0, 1.0)));
        assert_eq!(wilson(6, 5, Z95), None);
        assert_eq!(
            Rate::new(0, 0),
            Rate {
                successes: 0,
                n: 0,
                rate: None,
                ci95: None
            }
        );
        assert_eq!(Rate::new(3, 4).rate, Some(0.75));
        assert!(!intervals_overlap(&Rate::new(1, 100), &Rate::new(99, 100)));
        assert!(intervals_overlap(&Rate::new(50, 100), &Rate::new(55, 100)));
        assert!(intervals_overlap(&Rate::new(0, 0), &Rate::new(1, 2)));
        assert_eq!(percentile(&[1, 2, 3, 4], 0.5), 2);
        assert_eq!(percentile(&[1, 2, 3, 4], 0.9), 4);
        let d = Distribution::of(&[5, 1, 3]);
        assert_eq!(
            (d.n, d.mean, d.median, d.min, d.max, d.p10, d.p90),
            (3, Some(3.0), Some(3), Some(1), Some(5), Some(1), Some(5))
        );
        assert_eq!(Distribution::of(&[]).mean, None);
        assert_eq!(
            two_proportion_p(&Rate::new(15, 30), &Rate::new(15, 30)),
            1.0
        );
        let p = two_proportion_p(&Rate::new(21, 30), &Rate::new(9, 30));
        assert!(0.001 < p && p < 0.003);
        assert_eq!(holm(&[0.01, 0.04, 0.03], 0.05), [true, false, false]);
        assert_eq!(holm(&[0.001, 0.01, 0.02], 0.05), [true, true, true]);
    }
}
