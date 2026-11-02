//! Release dates of the packaged product list (`gcgapi/products.json`) and the optional 7-day
//! new-card check of `--event-date` (W-76).
//!
//! TRM 2.4.1 (`data/official_raw/en_pdf_floor_rule_en.txt`; the Asia floor rules say the same,
//! `asia_pdf_floor-rule-forasia.txt:229-238`): "A card with a new card number may not be used in
//! official tournaments or championship-related tournaments ... unless it has been sold or
//! distributed for at least 7 days since its initial release in the corresponding territory.
//! ... Cards with new card numbers initially released on the 1st of each month can be used
//! starting on the 8th of that month." So a card number first released on day R is usable from
//! R + 7 days.
//!
//! A card number's initial release is the earliest dated product whose `set_code` is the set
//! code the card number starts with (`ST11-016` → `ST11`): the set that introduced the number.
//! The canonical printing's `set_code` is not used, because reprints move it (R-001's
//! canonical printing is the SC01 Deck Build Box of 2026-07-24). Printings of a number
//! distributed before its set (Edition Beta, promotions) carry no date in the data, so the check
//! can only be stricter than the official rule, never more lenient. Card numbers whose set has
//! no dated product (Resources, tokens, promotion sets: `R`, `RP`, `T`, `EXB`, `EXBP`, `EXR`,
//! `EXRP`) are skipped and listed.
//!
//! `products.json` is not a file the Python runtime reads, so gcg-data does not embed it
//! (docs/port/PLAN.md §2.4): the caller passes its text to [`ReleaseDates::from_json`]
//! (`gcg-eval validate` embeds it).

use serde_json::Value;

use super::IsoDate;
use crate::contracts::{CardDb, DataError, Deck};
use crate::py;

const PRODUCTS_FILE: &str = "gcgapi/products.json";

/// TRM 2.4.1: a new card number is usable this many days after its initial release.
pub const NEW_CARD_DAYS: i64 = 7;

/// One product of `products.json`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Product {
    pub product_id: Box<str>,
    pub name: Box<str>,
    pub set_code: Option<Box<str>>,
    /// `None` when the data has no date or one that is not `YYYY-MM-DD`.
    pub release_date: Option<IsoDate>,
}

/// The products of the data, in file order.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ReleaseDates {
    products: Box<[Product]>,
}

/// A card number the event date comes too early for.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct NewCardViolation {
    pub card_number: Box<str>,
    pub released: IsoDate,
    pub product: Box<str>,
    pub usable_from: IsoDate,
    pub message: String,
}

/// The outcome of the 7-day new-card check for one event date.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct EventCheck {
    pub event_date: IsoDate,
    pub violations: Vec<NewCardViolation>,
    /// Card numbers of the deck whose release date the data does not give (not checked).
    pub unknown: Vec<Box<str>>,
}

fn opt_str(v: &Value, key: &str) -> Option<Box<str>> {
    py::get(v, key).and_then(Value::as_str).map(Into::into)
}

impl ReleaseDates {
    /// The products of a `products.json` text.
    ///
    /// # Errors
    /// [`DataError::Json`] for invalid JSON, [`DataError::Shape`] when it is not a list of
    /// products with a `product_id`.
    pub fn from_json(text_json: &str) -> Result<Self, DataError> {
        let f = PRODUCTS_FILE;
        let d: Value = serde_json::from_str(text_json).map_err(|e| DataError::Json {
            file: f.into(),
            line: 0,
            message: e.to_string(),
        })?;
        let items = d.as_array().ok_or_else(|| DataError::Shape {
            file: f.into(),
            detail: "not a list of products".into(),
        })?;
        let mut products = Vec::with_capacity(items.len());
        for p in items {
            let product_id = opt_str(p, "product_id").ok_or_else(|| DataError::Shape {
                file: f.into(),
                detail: format!("a product has no product_id: {p}"),
            })?;
            products.push(Product {
                name: opt_str(p, "name").unwrap_or_else(|| product_id.clone()),
                set_code: opt_str(p, "set_code"),
                release_date: opt_str(p, "release_date").and_then(|s| IsoDate::parse(&s)),
                product_id,
            });
        }
        Ok(Self {
            products: products.into(),
        })
    }

    /// Every product, in file order.
    #[must_use]
    pub fn products(&self) -> &[Product] {
        &self.products
    }

    /// The earliest dated product of `set_code` (the first in file order on a tie).
    #[must_use]
    pub fn first_release(&self, set_code: &str) -> Option<(&Product, IsoDate)> {
        let mut best: Option<(&Product, IsoDate)> = None;
        for p in self.products.iter() {
            let Some(date) = p.release_date else {
                continue;
            };
            if p.set_code.as_deref() == Some(set_code) && best.is_none_or(|(_, b)| date < b) {
                best = Some((p, date));
            }
        }
        best
    }
}

/// The set code a card number starts with: the text before its first `-`.
#[must_use]
pub fn number_set(card_number: &str) -> &str {
    card_number
        .split_once('-')
        .map_or(card_number, |(set, _)| set)
}

/// The 7-day new-card check of every distinct card number of `deck` (sorted), against the
/// event held on `event_date`.
pub(crate) fn event_check(
    db: &CardDb,
    releases: &ReleaseDates,
    deck: &Deck,
    event_date: IsoDate,
) -> EventCheck {
    let mut numbers: Vec<&str> = deck
        .main
        .iter()
        .chain(deck.resources.iter())
        .map(|c| &*c.card_number)
        .collect();
    numbers.sort();
    numbers.dedup();
    let mut out = EventCheck {
        event_date,
        violations: Vec::new(),
        unknown: Vec::new(),
    };
    for n in numbers {
        let Some(d) = db.get(n) else {
            continue;
        };
        let Some((product, released)) = releases.first_release(number_set(n)) else {
            out.unknown.push(n.into());
            continue;
        };
        let Some(usable_from) = released.add_days(NEW_CARD_DAYS) else {
            out.unknown.push(n.into());
            continue;
        };
        if event_date >= usable_from {
            continue;
        }
        let rule = format!(
            "a card with a new card number may be used at official tournaments only once it has \
             been sold for at least {NEW_CARD_DAYS} days, from {usable_from} (TRM 2.4.1)"
        );
        let message = if event_date < released {
            format!(
                "{n} {} is not released until {released} ({}), after the event on {event_date}; \
                 {rule}",
                d.name, product.name
            )
        } else {
            format!(
                "{n} {} was first released on {released} ({}), {} day(s) before the event on \
                 {event_date}; {rule}",
                d.name,
                product.name,
                released.days_until(event_date)
            )
        };
        out.violations.push(NewCardViolation {
            card_number: n.into(),
            released,
            product: product.product_id.clone(),
            usable_from,
            message,
        });
    }
    out
}
