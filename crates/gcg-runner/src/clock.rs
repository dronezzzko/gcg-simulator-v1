//! The only wall-clock code of the port (docs/port/PLAN.md §9.9, §11.8; brief constraints):
//! the `timing.json` values of [`crate::RunTiming`], written for CLI parity only and never
//! read back, compared or used to decide anything (PLAN A17). `runner/run.py:29-38` measures
//! the same values with `datetime.now(UTC)` and `time.perf_counter()`.

use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// A running duration measurement (`time.perf_counter()` differences, `runner/run.py:36-38`,
/// `:123`, `:131`).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Stopwatch(Instant);

impl Stopwatch {
    #[expect(
        clippy::disallowed_methods,
        reason = "clock.rs is the one wall-clock module; timing.json only (PLAN §9.9)"
    )]
    pub(crate) fn start() -> Self {
        Self(Instant::now())
    }

    /// Seconds since [`Stopwatch::start`].
    pub(crate) fn seconds(self) -> f64 {
        self.0.elapsed().as_secs_f64()
    }
}

/// `_utc_now()` (`runner/run.py:29-30`): `datetime.now(UTC).isoformat(timespec="seconds")`,
/// e.g. `2026-11-01T12:00:00+00:00` (fractions of a second are dropped, as `timespec`
/// truncates them).
#[expect(
    clippy::disallowed_methods,
    reason = "clock.rs is the one wall-clock module; timing.json only (PLAN §9.9)"
)]
pub(crate) fn utc_now() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    iso_utc(secs)
}

/// The ISO 8601 UTC text of `secs` seconds after the Unix epoch, as `isoformat` writes it.
fn iso_utc(secs: u64) -> String {
    let days = i64::try_from(secs / 86_400).unwrap_or(i64::MAX);
    let rem = secs % 86_400;
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}+00:00",
        rem / 3_600,
        rem % 3_600 / 60,
        rem % 60
    )
}

/// The proleptic Gregorian date of a day count since 1970-01-01 (H. Hinnant's
/// `civil_from_days`, exact for every date `datetime` can hold).
const fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "d is in 1..=31 and m in 1..=12"
    )]
    (y, m as u32, d as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_utc_matches_python_isoformat() {
        // datetime.fromtimestamp(s, UTC).isoformat(timespec="seconds") for these s.
        assert_eq!(iso_utc(0), "1970-01-01T00:00:00+00:00");
        assert_eq!(iso_utc(951_782_400), "2000-02-29T00:00:00+00:00");
        assert_eq!(iso_utc(1_793_520_054), "2026-11-01T08:00:54+00:00");
        assert_eq!(iso_utc(4_102_444_799), "2099-12-31T23:59:59+00:00");
        assert_eq!(iso_utc(4_107_542_400), "2100-03-01T00:00:00+00:00");
    }

    #[test]
    fn now_is_well_formed() {
        let now = utc_now();
        assert_eq!(now.len(), "2026-11-01T12:00:00+00:00".len());
        assert!(now.ends_with("+00:00"));
        assert!(Stopwatch::start().seconds() >= 0.0);
    }
}
