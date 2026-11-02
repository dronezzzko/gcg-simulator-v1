//! Head-to-head statistics: `ai/compare.py`'s `_rate` and `summarize` (`:146-178`, exact), the
//! W-42 paired statistics over deals (Newcombe's interval for paired proportions and the
//! McNemar discordant counts), and the realised play/draw counts of A24.
//!
//! Draws count as non-wins everywhere, as in Python (W-93); every win rate of this crate comes
//! from [`rate_json`]. Wilson intervals are `gcg_runner::reports::stats::wilson` (z = 1.96,
//! `reports/stats.py:12-25`). Python's `int / int` of two counts is the correctly rounded
//! quotient, which `f64` division of the exactly represented counts gives (all counts here are
//! far below 2^53).

use gcg_core::pyfloat::round_f64;
use gcg_core::pyjson::{Json, Key};
use gcg_data::DeckList;
use gcg_runner::reports::stats::{Z95, wilson};

use crate::specs::deals;
use crate::{GameOutcome, MirroredSpec};

/// The digits of `_rate` (`round(x, 4)`, `ai/compare.py:151-152`).
pub const RATE_DIGITS: usize = 4;

#[expect(
    clippy::cast_precision_loss,
    reason = "counts and turn sums stay far below 2^53, where the conversion is exact"
)]
fn quotient(num: i64, den: u64) -> f64 {
    num as f64 / den as f64
}

fn count(n: usize) -> u64 {
    u64::try_from(n).unwrap_or(u64::MAX)
}

/// `wilson(wins, n)` for valid counts.
///
/// # Panics
/// If `wins > n`, which no caller can produce (Python raises `ValueError`).
#[must_use]
pub fn wilson95(wins: u64, n: u64) -> (f64, f64) {
    wilson(wins, n, Z95).unwrap_or_else(|| panic!("wilson({wins}, {n}): more wins than games"))
}

/// `_rate(wins, n)` (`ai/compare.py:146-153`): `{"wins", "n", "rate", "ci95"}` with the rate
/// and the Wilson bounds rounded to 4 places, `rate` null when `n == 0`.
#[must_use]
pub fn rate_json(wins: u64, n: u64) -> Json {
    let (lo, hi) = wilson95(wins, n);
    let mut o = Json::object();
    o.set("wins", wins);
    o.set("n", n);
    o.set(
        "rate",
        (n > 0).then(|| {
            round_f64(
                quotient(i64::try_from(wins).unwrap_or(i64::MAX), n),
                RATE_DIGITS,
            )
        }),
    );
    o.set(
        "ci95",
        vec![round_f64(lo, RATE_DIGITS), round_f64(hi, RATE_DIGITS)],
    );
    o
}

/// `o.winner == o.challenger`: the challenger won (a draw, -1, is a non-win).
#[must_use]
pub fn challenger_won(o: &GameOutcome) -> bool {
    i16::from(o.winner) == i16::from(o.challenger)
}

/// `summarize(specs, outcomes)` (`ai/compare.py:156-178`): the challenger's wins overall
/// (`_rate`'s keys), `draws`, `mean_turns` (2 places), `by_challenger_seat` (`"0"`, `"1"`) and
/// `by_challenger_deck` (`deck{k}` per distinct deck the challenger played, by first
/// appearance; equal decks share a name).
#[must_use]
pub fn summarize(specs: &[MirroredSpec], outcomes: &[GameOutcome]) -> Json {
    let won: Vec<bool> = outcomes.iter().map(challenger_won).collect();
    let mut by_seat = Json::object();
    for seat in [0_u8, 1] {
        let wins = won
            .iter()
            .zip(outcomes)
            .filter(|(w, o)| **w && o.challenger == seat)
            .count();
        let n = outcomes.iter().filter(|o| o.challenger == seat).count();
        by_seat.set(seat.to_string(), rate_json(count(wins), count(n)));
    }
    let mut names: Vec<&DeckList> = Vec::new();
    let mut by_deck: Vec<(String, Vec<bool>)> = Vec::new();
    for (s, w) in specs.iter().zip(&won) {
        let deck = &s.decks[usize::from(s.challenger)];
        let k = names.iter().position(|d| *d == deck).unwrap_or_else(|| {
            names.push(deck);
            by_deck.push((format!("deck{}", names.len() - 1), Vec::new()));
            names.len() - 1
        });
        by_deck[k].1.push(*w);
    }
    let mut decks = Json::object();
    for (name, results) in &by_deck {
        let wins = results.iter().filter(|w| **w).count();
        decks.set(name.as_str(), rate_json(count(wins), count(results.len())));
    }
    let mut out = rate_json(
        count(won.iter().filter(|w| **w).count()),
        count(outcomes.len()),
    );
    out.set(
        "draws",
        count(outcomes.iter().filter(|o| o.winner == -1).count()),
    );
    let turns: i64 = outcomes.iter().map(|o| i64::from(o.turns)).sum();
    out.set(
        "mean_turns",
        round_f64(quotient(turns, count(outcomes.len().max(1))), 2),
    );
    out.set("by_challenger_seat", by_seat);
    out.set("by_challenger_deck", decks);
    out
}

/// `max(0.0, v)` with Python's first-argument tie rule.
fn non_negative(v: f64) -> f64 {
    if v > 0.0 { v } else { 0.0 }
}

/// The interval of Newcombe (1998), method 10, for the difference of paired proportions
/// `(a + b) / n - (a + c) / n` of the 2x2 table `a` (both yes), `b` (first only), `c` (second
/// only), `d` (neither): the Wilson intervals `(l1, u1)` and `(l2, u2)` of the two margins and
/// the correlation `phi = (ad - bc) / sqrt((a+b)(c+d)(a+c)(b+d))` (0 when a margin is empty)
/// give `diff - sqrt((p1-l1)^2 - 2 phi (p1-l1)(u2-p2) + (u2-p2)^2)` and
/// `diff + sqrt((u1-p1)^2 - 2 phi (u1-p1)(p2-l2) + (p2-l2)^2)`. `(-1, 1)` for an empty table.
/// The operation order is `export_stats.py`'s `newcombe_paired`, whose vectors pin it.
#[must_use]
pub fn newcombe_paired(a: u64, b: u64, c: u64, d: u64, z: f64) -> (f64, f64) {
    let n = a + b + c + d;
    if n == 0 {
        return (-1.0, 1.0);
    }
    let as_i = |x: u64| i64::try_from(x).unwrap_or(i64::MAX);
    let p1 = quotient(as_i(a + b), n);
    let p2 = quotient(as_i(a + c), n);
    let (l1, u1) = wilson(a + b, n, z).unwrap_or((0.0, 1.0));
    let (l2, u2) = wilson(a + c, n, z).unwrap_or((0.0, 1.0));
    let den = u128::from(a + b) * u128::from(c + d) * u128::from(a + c) * u128::from(b + d);
    let phi = if den == 0 {
        0.0
    } else {
        let num = i128::from(a) * i128::from(d) - i128::from(b) * i128::from(c);
        #[expect(
            clippy::cast_precision_loss,
            reason = "Python converts the int operands of a float operation to the nearest double"
        )]
        let (num, den) = (num as f64, den as f64);
        num / gcg_core::libm::sqrt(den)
    };
    let diff = p1 - p2;
    let dl = p1 - l1;
    let du = u2 - p2;
    let el = u1 - p1;
    let eu = p2 - l2;
    let delta = gcg_core::libm::sqrt(non_negative(dl * dl - 2.0 * phi * dl * du + du * du));
    let eps = gcg_core::libm::sqrt(non_negative(el * el - 2.0 * phi * el * eu + eu * eu));
    (diff - delta, diff + eps)
}

/// The W-42 table over the deals of a head-to-head ([`deals`]): per deal, `x` = the challenger
/// won the deal's first game (challenger in seat 0) and `y` = the opponent won the second (the
/// opponent in seat 0, same cards): both agents played the same position. `a`: both won it,
/// `b`: only the challenger, `c`: only the opponent (`b` and `c` are McNemar's discordant
/// counts), `d`: neither.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct Paired {
    pub a: u64,
    pub b: u64,
    pub c: u64,
    pub d: u64,
}

impl Paired {
    /// The table of the games `specs[i]` with outcomes `outcomes[i]`.
    #[must_use]
    pub fn of(specs: &[MirroredSpec], outcomes: &[GameOutcome]) -> Self {
        let mut t = Self::default();
        for (first, second) in deals(specs) {
            let x = challenger_won(&outcomes[first]);
            let o = &outcomes[second];
            let y = i16::from(o.winner) == 1 - i16::from(o.challenger);
            match (x, y) {
                (true, true) => t.a += 1,
                (true, false) => t.b += 1,
                (false, true) => t.c += 1,
                (false, false) => t.d += 1,
            }
        }
        t
    }

    #[must_use]
    pub const fn deals(&self) -> u64 {
        self.a + self.b + self.c + self.d
    }

    /// The paired difference `P(x) - P(y)`, `None` without deals.
    #[must_use]
    pub fn difference(&self) -> Option<f64> {
        let n = self.deals();
        (n > 0).then(|| {
            let as_i = |x: u64| i64::try_from(x).unwrap_or(i64::MAX);
            quotient(as_i(self.a + self.b), n) - quotient(as_i(self.a + self.c), n)
        })
    }

    /// `{"deals", "both", "challenger_only", "opponent_only", "neither", "difference", "ci95"}`
    /// with the difference and the Newcombe bounds rounded to 4 places.
    #[must_use]
    pub fn to_json(&self) -> Json {
        let (lo, hi) = newcombe_paired(self.a, self.b, self.c, self.d, Z95);
        let mut o = Json::object();
        o.set("deals", self.deals());
        o.set("both", self.a);
        o.set("challenger_only", self.b);
        o.set("opponent_only", self.c);
        o.set("neither", self.d);
        o.set(
            "difference",
            self.difference().map(|x| round_f64(x, RATE_DIGITS)),
        );
        o.set(
            "ci95",
            vec![round_f64(lo, RATE_DIGITS), round_f64(hi, RATE_DIGITS)],
        );
        o
    }

    /// The seeded percentile bootstrap over deals of the paired difference (W-42): `reps`
    /// resamples of the `n` deals with replacement (each draw `randrange(n)` of a
    /// `SplitMix64(seed)`: below `b` a challenger-only deal, +1; below `b + c` an
    /// opponent-only deal, -1; otherwise 0), each resample's mean difference, sorted; the
    /// interval is the order statistics `floor(0.025 reps)` and `ceil(0.975 reps) - 1`.
    /// `None` without deals or resamples.
    #[must_use]
    pub fn bootstrap_ci(&self, seed: u64, reps: u32) -> Option<(f64, f64)> {
        let n = self.deals();
        if n == 0 || reps == 0 {
            return None;
        }
        let mut rng = gcg_core::SplitMix64::new(seed);
        let mut means = Vec::with_capacity(usize::try_from(reps).unwrap_or(0));
        for _ in 0..reps {
            let mut sum: i64 = 0;
            for _ in 0..n {
                let k = rng.randrange(n);
                if k < self.b {
                    sum += 1;
                } else if k < self.b + self.c {
                    sum -= 1;
                }
            }
            means.push(quotient(sum, n));
        }
        means.sort_by(|x, y| gcg_core::pymath::cmp_f64(*x, *y));
        let r = usize::try_from(reps).unwrap_or(usize::MAX);
        let lo = r * 25 / 1000;
        let hi = (r * 975).div_ceil(1000) - 1;
        Some((means[lo], means[hi]))
    }

    /// [`Paired::to_json`] plus `bootstrap_ci95`, the [`Paired::bootstrap_ci`] of
    /// [`BOOTSTRAP_REPS`] resamples on [`bootstrap_seed`] (the resamples depend only on the
    /// counts, so one fixed seed keeps every report deterministic), 4 places.
    #[must_use]
    pub fn to_json_with_bootstrap(&self) -> Json {
        let mut o = self.to_json();
        o.set(
            "bootstrap_ci95",
            self.bootstrap_ci(bootstrap_seed(), BOOTSTRAP_REPS)
                .map_or(Json::Null, |(lo, hi)| {
                    Json::Array(vec![
                        round_f64(lo, RATE_DIGITS).into(),
                        round_f64(hi, RATE_DIGITS).into(),
                    ])
                }),
        );
        o
    }
}

/// Resamples of the deal bootstrap.
pub const BOOTSTRAP_REPS: u32 = 1000;

/// The seed of the deal bootstrap: `derive_seed(0, "gcg-eval", "bootstrap")`.
#[must_use]
pub fn bootstrap_seed() -> u64 {
    gcg_core::derive_seed!(0, "gcg-eval", "bootstrap")
}

/// The realised play/draw split of one agent (A24): the games it played on the play (its seat
/// went first) and on the draw, and its wins in each.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct PlayDraw {
    pub on_play: u64,
    pub on_play_wins: u64,
    pub on_draw: u64,
    pub on_draw_wins: u64,
}

impl PlayDraw {
    /// The challenger's split (`challenger = true`) or the opponent's.
    #[must_use]
    pub fn of(outcomes: &[GameOutcome], challenger: bool) -> Self {
        let mut t = Self::default();
        for o in outcomes {
            let seat = if challenger {
                o.challenger
            } else {
                1 - o.challenger
            };
            let won = i16::from(o.winner) == i16::from(seat);
            if o.first_player == seat {
                t.on_play += 1;
                t.on_play_wins += u64::from(won);
            } else {
                t.on_draw += 1;
                t.on_draw_wins += u64::from(won);
            }
        }
        t
    }

    #[must_use]
    pub fn to_json(&self) -> Json {
        let mut o = Json::object();
        o.set("on_play", rate_json(self.on_play_wins, self.on_play));
        o.set("on_draw", rate_json(self.on_draw_wins, self.on_draw));
        o
    }
}

/// A JSON object with integer-like string keys in the given order.
#[must_use]
pub fn keyed(items: Vec<(String, Json)>) -> Json {
    Json::Object(items.into_iter().map(|(k, v)| (Key::Str(k), v)).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newcombe_properties() {
        for (a, b, c, d) in [(36, 12, 2, 0), (5, 1, 3, 9), (0, 4, 0, 0), (10, 0, 0, 10)] {
            let (lo, hi) = newcombe_paired(a, b, c, d, Z95);
            let n = a + b + c + d;
            let diff = (b as f64 - c as f64) / n as f64;
            assert!(lo <= diff + 1e-12 && diff <= hi + 1e-12, "{a} {b} {c} {d}");
            assert!((-1.0..=1.0).contains(&lo) && (-1.0..=1.0).contains(&hi));
            let (mlo, mhi) = newcombe_paired(a, c, b, d, Z95);
            assert!(
                (mlo + hi).abs() < 1e-12 && (mhi + lo).abs() < 1e-12,
                "mirror"
            );
        }
        assert_eq!(newcombe_paired(0, 0, 0, 0, Z95), (-1.0, 1.0));
    }

    /// W-42: Newcombe (1998), Table II, example (a): 36, 12, 2, 0 (n = 50), method 10,
    /// 0.0569 to 0.3404 to four places. The paper could not be fetched offline: the value is
    /// the one quoted from it, and the full-precision vectors of `export_stats.py` (an
    /// independent Python implementation) pin the bits (`tests/vectors.rs`).
    #[test]
    fn w42_newcombe_published_example() {
        let (lo, hi) = newcombe_paired(36, 12, 2, 0, Z95);
        assert_eq!((round_f64(lo, 4), round_f64(hi, 4)), (0.0569, 0.3404));
    }

    /// W-42: the bootstrap interval is seeded (same seed, same bounds), contains the paired
    /// difference, is degenerate without discordant deals, and narrows with more deals.
    #[test]
    fn w42_bootstrap_over_deals() {
        let t = Paired {
            a: 40,
            b: 30,
            c: 10,
            d: 20,
        };
        let ci = t.bootstrap_ci(7, 1000).unwrap_or((9.0, 9.0));
        assert_eq!(t.bootstrap_ci(7, 1000), Some(ci));
        let diff = t.difference().unwrap_or(0.0);
        assert!(ci.0 < diff && diff < ci.1, "{ci:?} around {diff}");
        assert!(ci.0 > 0.0, "20 more challenger-only deals of 100: {ci:?}");
        let none = Paired {
            a: 5,
            b: 0,
            c: 0,
            d: 5,
        };
        assert_eq!(none.bootstrap_ci(7, 1000), Some((0.0, 0.0)));
        let big = Paired {
            a: 400,
            b: 300,
            c: 100,
            d: 200,
        };
        let wide = big.bootstrap_ci(7, 1000).unwrap_or((0.0, 0.0));
        assert!(wide.1 - wide.0 < ci.1 - ci.0);
        assert_eq!(Paired::default().bootstrap_ci(7, 1000), None);
    }

    #[test]
    fn identical_agents_on_identical_results_give_no_difference() {
        let t = Paired {
            a: 7,
            b: 0,
            c: 0,
            d: 5,
        };
        assert_eq!(t.difference(), Some(0.0));
        let (lo, hi) = newcombe_paired(t.a, t.b, t.c, t.d, Z95);
        assert!(lo < 0.0 && hi > 0.0);
    }
}
