//! The Banned & Restricted model as a list of dated, region-tagged lists (W-72), and the
//! data hook of the unmodified-starter-deck exceptions (W-71).
//!
//! `banlist.json` (read-only packaged data) holds one current list. This module reads it as
//! the newest entry of a history and accepts two optional keys that the packaged file does not
//! carry today, so a data refresh can add them without a code change:
//!
//! - `history`: older lists, each an object of the top-level list's shape (`effective_date`,
//!   `banned`, `restricted`, `banned_pairs`, `attribute_pair_rules`) plus `regions`, a list of
//!   region codes.
//! - `official_product_decklists`: `{product: {"main": [{"card_number", "count"}], "resources":
//!   [...], "source_url", "retrieved_at"}}`, an official product decklist with its provenance.
//!   An exception whose `condition` is `{"deck_equals_official_product_decklist": product}`
//!   exempts a deck from exactly the restriction that lists it when the deck's main deck and
//!   resource deck equal that decklist card for card.
//!
//! Region codes are the official site's region path segment (`https://www.gundam-gcg.com/<region>/`:
//! `en` for the English site of North America, Latin America, Europe and Oceania, `asia-en` for
//! the English (Asia) site). The current list's regions are its `regions` key, else the
//! distinct region segments of its `source_urls`: the packaged list was taken from both sites'
//! current-list page (01_279), whose contents are identical (`banlist.json` `region`).

use std::collections::BTreeMap;
use std::fmt;
use std::sync::OnceLock;

use serde_json::Value;

use super::IsoDate;
use crate::contracts::{Banlist, CardCount, DataError, Deck};
use crate::official::{self, BANLIST_FILE, field, list, shape, text};
use crate::{embedded, py};

/// One Banned & Restricted list with the date it takes effect and the regions it covers.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct BanlistEntry {
    pub effective_date: IsoDate,
    /// Region codes, sorted.
    pub regions: Box<[Box<str>]>,
    pub list: Banlist,
    /// The top-level list of `banlist.json`: the one `validate_deck` applies.
    pub current: bool,
    /// Every exception whose condition names an official product decklist, in file order
    /// (restricted entries, then attribute-pair rules).
    pub product_exceptions: Box<[ProductException]>,
}

/// An exception whose condition is `{"deck_equals_official_product_decklist": product}`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ProductException {
    pub id: Box<str>,
    pub product: Box<str>,
}

/// An official product decklist with provenance (the W-71 hook).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ProductDecklist {
    pub product: Box<str>,
    /// Card numbers with their copies, sorted by card number (as [`Deck::main`]).
    pub main: Box<[CardCount]>,
    pub resources: Box<[CardCount]>,
    pub source_url: Box<str>,
    pub retrieved_at: Box<str>,
}

impl ProductDecklist {
    /// Whether `deck` is this decklist unmodified: the same copies of the same card numbers in
    /// the main deck and in the resource deck.
    #[must_use]
    pub fn is_unmodified(&self, deck: &Deck) -> bool {
        *deck.main == *self.main && *deck.resources == *self.resources
    }
}

/// An exception that applies to a deck: the deck equals the named official decklist.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct AppliedException {
    pub id: Box<str>,
    pub product: Box<str>,
    pub source_url: Box<str>,
}

/// How each product exception stands for one deck.
#[derive(Clone, Default, PartialEq, Eq, Debug)]
pub(crate) struct ExceptionStatus {
    /// Exception ids whose decklist the deck equals: their restriction is not checked.
    pub(crate) exempt: Vec<Box<str>>,
    /// The exceptions whose decklist exists and differs from the deck.
    pub(crate) differs: Vec<ProductException>,
    pub(crate) applied: Vec<AppliedException>,
}

/// Why no list could be selected for `--as-of` and `--region`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum SelectError {
    /// No list in the data covers the region.
    UnknownRegion {
        region: Box<str>,
        known: Box<[Box<str>]>,
    },
    /// No list covering the request had taken effect by the date.
    NoList {
        as_of: IsoDate,
        region: Option<Box<str>>,
        earliest: Vec<(IsoDate, Box<[Box<str>]>)>,
    },
    /// Without `--region`, the regions in the data apply different lists on that date.
    Ambiguous {
        as_of: Option<IsoDate>,
        by_region: Vec<(Box<str>, Option<IsoDate>)>,
    },
}

impl fmt::Display for SelectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownRegion { region, known } => write!(
                f,
                "no Banned & Restricted list in the data covers region {} (regions in the data: \
                 {})",
                crate::unicode::repr_str(region),
                known.join(", ")
            ),
            Self::NoList {
                as_of,
                region,
                earliest,
            } => {
                write!(
                    f,
                    "no Banned & Restricted list in the data was in effect on {as_of}"
                )?;
                if let Some(r) = region {
                    write!(f, " in region {r}")?;
                }
                let lists: Vec<String> = earliest
                    .iter()
                    .map(|(d, regions)| format!("{d} ({})", regions.join(", ")))
                    .collect();
                write!(
                    f,
                    "; the lists in the data take effect on {}. Older lists are added by a data \
                     refresh (/gcg-refresh-data)",
                    lists.join(", ")
                )
            }
            Self::Ambiguous { as_of, by_region } => {
                let when = as_of.map_or_else(|| "today".to_owned(), |d| format!("on {d}"));
                let picks: Vec<String> = by_region
                    .iter()
                    .map(|(r, d)| match d {
                        Some(d) => format!("{r}: the list effective {d}"),
                        None => format!("{r}: no list"),
                    })
                    .collect();
                write!(
                    f,
                    "the regions apply different Banned & Restricted lists {when} ({}); choose \
                     one with --region",
                    picks.join("; ")
                )
            }
        }
    }
}

impl std::error::Error for SelectError {}

/// Every Banned & Restricted list of the data and the official product decklists.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct BanlistHistory {
    /// History lists in file order, then the current list.
    entries: Box<[BanlistEntry]>,
    decklists: Box<[ProductDecklist]>,
}

fn strings(file: &str, v: &Value, what: &str) -> Result<Vec<Box<str>>, DataError> {
    list(file, v, what, |s| {
        s.as_str()
            .map(Into::into)
            .ok_or_else(|| shape(file, format!("{what} holds a non-string: {s}")))
    })
}

/// The region segment of `https://<host>/<region>/...`.
fn url_region(url: &str) -> Option<&str> {
    let rest = url.split_once("://")?.1;
    let seg = rest.split('/').nth(1)?;
    (!seg.is_empty()).then_some(seg)
}

fn sorted_regions(mut regions: Vec<Box<str>>) -> Box<[Box<str>]> {
    regions.sort();
    regions.dedup();
    regions.into()
}

/// The regions of the top-level list: `regions`, else the region segments of `source_urls`.
fn current_regions(d: &Value) -> Result<Box<[Box<str>]>, DataError> {
    let f = BANLIST_FILE;
    if let Some(v) = py::get(d, "regions") {
        return Ok(sorted_regions(strings(f, v, "regions")?));
    }
    let urls = match py::get(d, "source_urls") {
        Some(v) => strings(f, v, "source_urls")?,
        None => Vec::new(),
    };
    let regions: Vec<Box<str>> = urls
        .iter()
        .filter_map(|u| url_region(u))
        .map(Into::into)
        .collect();
    if regions.is_empty() {
        return Err(shape(
            f,
            "the current list names no region (regions or source_urls)",
        ));
    }
    Ok(sorted_regions(regions))
}

/// The exceptions of one list whose condition names a product decklist.
fn product_exceptions(d: &Value) -> Result<Box<[ProductException]>, DataError> {
    let f = BANLIST_FILE;
    let mut out = Vec::new();
    let mut scan = |holders: &Value, what: &str| -> Result<(), DataError> {
        for holder in list(f, holders, what, Ok)? {
            let Some(exceptions) = py::get(holder, "exceptions") else {
                continue;
            };
            for e in list(f, exceptions, "exceptions", Ok)? {
                let product = py::get(e, "condition")
                    .and_then(|c| py::get(c, "deck_equals_official_product_decklist"));
                if let Some(p) = product {
                    let p = p.as_str().ok_or_else(|| {
                        shape(f, "deck_equals_official_product_decklist is not a string")
                    })?;
                    out.push(ProductException {
                        id: text(f, e, &["id"])?,
                        product: p.into(),
                    });
                }
            }
        }
        Ok(())
    };
    scan(field(f, d, &["restricted"])?, "restricted")?;
    scan(
        field(f, d, &["attribute_pair_rules"])?,
        "attribute_pair_rules",
    )?;
    Ok(out.into())
}

fn entry(d: &Value, regions: Box<[Box<str>]>, current: bool) -> Result<BanlistEntry, DataError> {
    let list = official::parse_banlist_value(d)?;
    let effective_date = IsoDate::parse(&list.effective_date).ok_or_else(|| {
        shape(
            BANLIST_FILE,
            format!("effective_date is not YYYY-MM-DD: {}", list.effective_date),
        )
    })?;
    Ok(BanlistEntry {
        effective_date,
        regions,
        product_exceptions: product_exceptions(d)?,
        list,
        current,
    })
}

/// One side of a product decklist, merged and sorted by card number.
fn decklist_side(v: &Value, what: &str) -> Result<Box<[CardCount]>, DataError> {
    let f = BANLIST_FILE;
    let mut counts: BTreeMap<Box<str>, u128> = BTreeMap::new();
    for item in list(f, v, what, Ok)? {
        let number = text(f, item, &["card_number"])?;
        let count = field(f, item, &["count"])?
            .as_u64()
            .filter(|&c| c > 0)
            .ok_or_else(|| shape(f, format!("{what}: {number} has no positive count")))?;
        *counts.entry(number).or_insert(0) += u128::from(count);
    }
    Ok(counts
        .into_iter()
        .map(|(card_number, count)| CardCount { card_number, count })
        .collect())
}

fn decklists(d: &Value) -> Result<Box<[ProductDecklist]>, DataError> {
    let f = BANLIST_FILE;
    let Some(v) = py::get(d, "official_product_decklists") else {
        return Ok(Box::new([]));
    };
    let map = v
        .as_object()
        .ok_or_else(|| shape(f, "official_product_decklists is not an object"))?;
    let mut out = Vec::with_capacity(map.len());
    for (product, dl) in map {
        let provenance = |key: &str| -> Result<Box<str>, DataError> {
            py::get(dl, key)
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                .map(Into::into)
                .ok_or_else(|| {
                    shape(
                        f,
                        format!("the {product} decklist has no {key} (provenance is required)"),
                    )
                })
        };
        out.push(ProductDecklist {
            product: product.as_str().into(),
            main: decklist_side(field(f, dl, &["main"])?, "main")?,
            resources: decklist_side(field(f, dl, &["resources"])?, "resources")?,
            source_url: provenance("source_url")?,
            retrieved_at: provenance("retrieved_at")?,
        });
    }
    out.sort_by(|a, b| a.product.cmp(&b.product));
    Ok(out.into())
}

impl BanlistHistory {
    /// The model of the packaged `banlist.json`: today one current list, no history and no
    /// decklist. Its current list is exactly [`official::banlist`].
    ///
    /// # Panics
    /// If the embedded file does not load (a build defect; `cargo test -p gcg-data` loads it).
    #[must_use]
    pub fn packaged() -> &'static Self {
        static HISTORY: OnceLock<BanlistHistory> = OnceLock::new();
        HISTORY.get_or_init(|| {
            Self::from_json(embedded::BANLIST_JSON)
                .unwrap_or_else(|e| panic!("embedded banlist does not load: {e}"))
        })
    }

    /// The model of a `banlist.json` text.
    ///
    /// # Errors
    /// The [`DataError`] of `banlist()`, or a `Shape` error for a malformed `history`,
    /// `official_product_decklists`, region or date, two lists of one region with the same
    /// date, or a history list that takes effect after the current list in one of its regions.
    pub fn from_json(text_json: &str) -> Result<Self, DataError> {
        let f = BANLIST_FILE;
        let d = official::banlist_document(text_json)?;
        let mut entries = Vec::new();
        if let Some(history) = py::get(&d, "history") {
            for h in list(f, history, "history", Ok)? {
                let regions = sorted_regions(strings(f, field(f, h, &["regions"])?, "regions")?);
                if regions.is_empty() {
                    return Err(shape(f, "a history list covers no region"));
                }
                entries.push(entry(h, regions, false)?);
            }
        }
        let current = entry(&d, current_regions(&d)?, true)?;
        for (i, a) in entries.iter().enumerate() {
            for b in entries.iter().skip(i + 1).chain(std::iter::once(&current)) {
                let shared = a.regions.iter().find(|r| b.regions.contains(r));
                if let Some(r) = shared
                    && a.effective_date == b.effective_date
                {
                    return Err(shape(
                        f,
                        format!(
                            "two lists of region {r} take effect on {}",
                            a.effective_date
                        ),
                    ));
                }
            }
            let later = a.effective_date > current.effective_date;
            if let Some(r) = a.regions.iter().find(|r| current.regions.contains(r))
                && later
            {
                return Err(shape(
                    f,
                    format!(
                        "a history list of region {r} takes effect on {}, after the current list \
                         ({})",
                        a.effective_date, current.effective_date
                    ),
                ));
            }
        }
        entries.push(current);
        Ok(Self {
            entries: entries.into(),
            decklists: decklists(&d)?,
        })
    }

    /// Every list: the history in file order, then the current list.
    #[must_use]
    pub fn entries(&self) -> &[BanlistEntry] {
        &self.entries
    }

    /// The current list (the top level of `banlist.json`).
    #[must_use]
    pub fn current(&self) -> &BanlistEntry {
        self.entries
            .last()
            .unwrap_or_else(|| unreachable!("from_json always adds the current list"))
    }

    /// Every region code of the data, sorted.
    #[must_use]
    pub fn regions(&self) -> Vec<&str> {
        let mut out: Vec<&str> = self
            .entries
            .iter()
            .flat_map(|e| e.regions.iter().map(|r| &**r))
            .collect();
        out.sort();
        out.dedup();
        out
    }

    /// The official product decklists, sorted by product.
    #[must_use]
    pub fn decklists(&self) -> &[ProductDecklist] {
        &self.decklists
    }

    /// The decklist of `product`, if the data has one.
    #[must_use]
    pub fn decklist(&self, product: &str) -> Option<&ProductDecklist> {
        self.decklists.iter().find(|d| &*d.product == product)
    }

    /// The newest list of `region` in effect on `as_of` (any date when `None`).
    fn newest(&self, as_of: Option<IsoDate>, region: &str) -> Option<&BanlistEntry> {
        let mut best: Option<&BanlistEntry> = None;
        for e in self.entries.iter() {
            let covers = e.regions.iter().any(|r| &**r == region);
            let in_effect = as_of.is_none_or(|d| e.effective_date <= d);
            if covers && in_effect && best.is_none_or(|b| e.effective_date > b.effective_date) {
                best = Some(e);
            }
        }
        best
    }

    /// The list that applies on `as_of` in `region`. With neither, the current list (what
    /// `validate_deck` applies). With a date and no region, every region of the data must
    /// apply the same list on that date.
    ///
    /// # Errors
    /// [`SelectError`]: an unknown region, no list in effect yet, or regions that disagree.
    pub fn select(
        &self,
        as_of: Option<IsoDate>,
        region: Option<&str>,
    ) -> Result<&BanlistEntry, SelectError> {
        let known = self.regions();
        let regions: Vec<&str> = match region {
            None if as_of.is_none() => return Ok(self.current()),
            None => known.clone(),
            Some(r) if known.contains(&r) => vec![r],
            Some(r) => {
                return Err(SelectError::UnknownRegion {
                    region: r.into(),
                    known: known.iter().map(|&k| k.into()).collect(),
                });
            }
        };
        let picks: Vec<(&str, Option<&BanlistEntry>)> = regions
            .iter()
            .map(|&r| (r, self.newest(as_of, r)))
            .collect();
        let first = picks[0].1;
        let same = picks
            .iter()
            .all(|(_, p)| p.map(std::ptr::from_ref) == first.map(std::ptr::from_ref));
        match (first, same, as_of) {
            (Some(e), true, _) => Ok(e),
            (None, true, Some(as_of)) => Err(SelectError::NoList {
                as_of,
                region: region.map(Into::into),
                earliest: self.earliest(region),
            }),
            _ => Err(SelectError::Ambiguous {
                as_of,
                by_region: picks
                    .iter()
                    .map(|(r, p)| ((*r).into(), p.map(|e| e.effective_date)))
                    .collect(),
            }),
        }
    }

    /// Each list's date and regions, oldest first, restricted to `region` when given.
    fn earliest(&self, region: Option<&str>) -> Vec<(IsoDate, Box<[Box<str>]>)> {
        let mut out: Vec<(IsoDate, Box<[Box<str>]>)> = self
            .entries
            .iter()
            .filter(|e| region.is_none_or(|r| e.regions.iter().any(|x| &**x == r)))
            .map(|e| (e.effective_date, e.regions.clone()))
            .collect();
        out.sort_by_key(|e| e.0);
        out
    }

    /// How `entry`'s product exceptions stand for `deck`.
    pub(crate) fn exception_status(&self, entry: &BanlistEntry, deck: &Deck) -> ExceptionStatus {
        let mut status = ExceptionStatus::default();
        for pe in entry.product_exceptions.iter() {
            let Some(dl) = self.decklist(&pe.product) else {
                continue;
            };
            if dl.is_unmodified(deck) {
                status.exempt.push(pe.id.clone());
                status.applied.push(AppliedException {
                    id: pe.id.clone(),
                    product: pe.product.clone(),
                    source_url: dl.source_url.clone(),
                });
            } else {
                status.differs.push(pe.clone());
            }
        }
        status
    }
}
