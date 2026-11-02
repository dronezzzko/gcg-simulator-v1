//! Calendar dates of the official data (`YYYY-MM-DD`, proleptic Gregorian), compared and
//! shifted by whole days. Nothing here reads a clock: every date comes from the data or from a
//! command-line argument.

use std::fmt;

/// A date `YYYY-MM-DD` with year 1..=9999, as the official data writes `effective_date`,
/// `since` and `release_date`. Ordering is chronological.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct IsoDate {
    year: u16,
    month: u8,
    day: u8,
}

const fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

const fn days_in_month(year: i64, month: u8) -> u8 {
    match month {
        2 if is_leap(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

impl IsoDate {
    /// Parses exactly `YYYY-MM-DD` (ASCII digits, a real calendar day); `None` otherwise.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let b = text.as_bytes();
        if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
            return None;
        }
        let num = |s: &[u8]| -> Option<u16> {
            s.iter().try_fold(0_u16, |acc, &c| {
                c.is_ascii_digit().then(|| acc * 10 + u16::from(c - b'0'))
            })
        };
        let year = num(&b[0..4])?;
        let month = u8::try_from(num(&b[5..7])?).ok()?;
        let day = u8::try_from(num(&b[8..10])?).ok()?;
        let valid = year >= 1
            && (1..=12).contains(&month)
            && day >= 1
            && day <= days_in_month(i64::from(year), month);
        valid.then_some(Self { year, month, day })
    }

    /// Days since 1970-01-01 (Howard Hinnant's `days_from_civil`).
    const fn day_number(self) -> i64 {
        let m = self.month as i64;
        let y = self.year as i64 - if m <= 2 { 1 } else { 0 };
        let era = y.div_euclid(400);
        let yoe = y - era * 400;
        let mp = (m + 9) % 12;
        let doy = (153 * mp + 2) / 5 + self.day as i64 - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146_097 + doe - 719_468
    }

    /// The date `days` after 1970-01-01 (`civil_from_days`), if its year is in 1..=9999.
    fn from_day_number(days: i64) -> Option<Self> {
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = doy - (153 * mp + 2) / 5 + 1;
        let month = if mp < 10 { mp + 3 } else { mp - 9 };
        let year = yoe + era * 400 + i64::from(month <= 2);
        Some(Self {
            year: u16::try_from(year)
                .ok()
                .filter(|y| (1..=9999).contains(y))?,
            month: u8::try_from(month).ok()?,
            day: u8::try_from(day).ok()?,
        })
    }

    /// This date moved by `days` (negative moves back); `None` outside years 1..=9999.
    #[must_use]
    pub fn add_days(self, days: i64) -> Option<Self> {
        Self::from_day_number(self.day_number().checked_add(days)?)
    }

    /// Whole days from `self` to `later` (negative if `later` is earlier).
    #[must_use]
    pub const fn days_until(self, later: Self) -> i64 {
        later.day_number() - self.day_number()
    }
}

impl fmt::Display for IsoDate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_only_real_days() {
        assert_eq!(
            IsoDate::parse("2026-09-25").map(|d| d.to_string()),
            Some("2026-09-25".to_owned())
        );
        for bad in [
            "2026-9-25",
            "2026-09-31",
            "2025-02-29",
            "0000-01-01",
            "2026-13-01",
            "2026-00-10",
            "２026-09-25",
            "2026-09-25 ",
            "",
        ] {
            assert_eq!(IsoDate::parse(bad), None, "{bad:?}");
        }
        assert!(IsoDate::parse("2024-02-29").is_some());
    }

    #[test]
    fn day_arithmetic_crosses_months_years_and_leap_days() {
        let d = |s: &str| IsoDate::parse(s).expect("date");
        assert_eq!(d("1970-01-01").day_number(), 0);
        assert_eq!(d("2026-09-25").add_days(7), Some(d("2026-10-02")));
        assert_eq!(d("2026-12-29").add_days(7), Some(d("2027-01-05")));
        assert_eq!(d("2024-02-25").add_days(7), Some(d("2024-03-03")));
        assert_eq!(d("2026-07-01").add_days(7), Some(d("2026-07-08")));
        assert_eq!(d("2026-07-08").add_days(-7), Some(d("2026-07-01")));
        assert_eq!(d("2026-09-25").days_until(d("2026-09-28")), 3);
        assert_eq!(d("9999-12-31").add_days(1), None);
        assert_eq!(d("0001-01-01").add_days(-1), None);
        let mut day = d("1999-12-25");
        for _ in 0..20_000 {
            let next = day.add_days(1).expect("in range");
            assert!(next > day);
            assert_eq!(IsoDate::parse(&next.to_string()), Some(next));
            assert_eq!(day.days_until(next), 1);
            day = next;
        }
    }
}
