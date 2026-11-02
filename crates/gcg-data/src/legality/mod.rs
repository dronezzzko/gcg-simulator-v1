//! The extended legality check behind `gcg-eval validate` (docs/port/PLAN.md §12.2 P6-L, A30;
//! W-71 to W-78). It runs the same checks as [`crate::validate`], in the same order and with the
//! same messages, and adds what `validate_deck` cannot do:
//!
//! - a Banned & Restricted list chosen by date and region from a dated, region-tagged model
//!   (W-72, [`BanlistHistory`]), or none at all (W-75, `--no-banlist`);
//! - the unmodified-starter-deck exceptions, applied only when the data carries the official
//!   decklist with provenance (W-71, a data hook the packaged data does not use today);
//! - the rules violations kept apart from the simulator-support ones (W-78);
//! - the optional 7-day new-card check for an event date (W-76, [`releases`]);
//! - hints for EX tokens listed in a deck and for `#` comments after a card (W-77);
//! - the validator's scope statement (W-75, W-76, [`Scope`]).
//!
//! With the default options ([`LegalityOptions::new`]) and the packaged data, `rules` followed
//! by `simulator` is exactly `validate(deck, ValidateOptions { banlist: true, implemented })`.
//! `gcg-sim validate` is unchanged (A30).

mod date;
mod lists;
pub mod releases;
mod scope;

use std::path::Path;

pub use date::IsoDate;
pub use lists::{
    AppliedException, BanlistEntry, BanlistHistory, ProductDecklist, ProductException, SelectError,
};
pub use releases::{EventCheck, NewCardViolation, Product, ReleaseDates};
pub use scope::{FORMAT_APPLIED, Scope, UNSUPPORTED_FORMATS};

use crate::contracts::{
    CardDb, CardType, ConstructionRules, Deck, ImplementationStatus, ValidateOptions, Violation,
    ViolationCode,
};
use crate::validate::{Exceptions, Rules};
use crate::{deck, official};

/// A hint attached to a violation code: what the deck file probably meant (W-77). Hints never
/// change the violations.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Hint {
    /// 1-based deck-file line, when the hint is about one line.
    pub line: Option<usize>,
    pub code: ViolationCode,
    pub text: String,
}

/// Which Banned & Restricted list applies: the newest in effect on `as_of` (any date when
/// `None`) in `region` (see [`BanlistHistory::select`]). The default is the current list.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct ListRequest<'a> {
    pub as_of: Option<IsoDate>,
    pub region: Option<&'a str>,
}

/// The options of one check.
#[derive(Clone, Copy)]
pub struct LegalityOptions<'a> {
    /// `None` applies no Banned & Restricted list (`validate_deck(deck, banlist=False)`).
    pub banlist: Option<ListRequest<'a>>,
    /// `None` skips the simulator-support check (`implemented=False`).
    pub implemented: Option<&'a dyn ImplementationStatus>,
    /// The 7-day new-card check for an event on this date.
    pub event_date: Option<IsoDate>,
}

impl<'a> LegalityOptions<'a> {
    /// `validate_deck(deck)`'s checks: the current list and the simulator-support check.
    #[must_use]
    pub fn new(implemented: &'a dyn ImplementationStatus) -> Self {
        Self {
            banlist: Some(ListRequest::default()),
            implemented: Some(implemented),
            event_date: None,
        }
    }
}

/// The data a check reads.
#[derive(Clone, Copy)]
pub struct LegalitySources<'a> {
    pub db: &'a CardDb,
    pub construction: &'a ConstructionRules,
    pub lists: &'a BanlistHistory,
    pub releases: &'a ReleaseDates,
}

impl<'a> LegalitySources<'a> {
    /// The packaged card database, `deck_construction.json` and `banlist.json`, with the
    /// caller's release dates (`products.json` is not embedded by gcg-data, PLAN §2.4).
    #[must_use]
    pub fn packaged(releases: &'a ReleaseDates) -> Self {
        Self {
            db: CardDb::embedded(),
            construction: official::construction_rules(),
            lists: BanlistHistory::packaged(),
            releases,
        }
    }
}

/// The outcome of one check.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct LegalityReport {
    /// The list applied; `None` with `--no-banlist`.
    pub banlist: Option<BanlistEntry>,
    /// Illegal by the rules: the Comprehensive Rules and the Banned & Restricted list, in
    /// `validate_deck`'s order.
    pub rules: Vec<Violation>,
    /// Not playable by this simulator version (UNIMPLEMENTED); `None` when not checked.
    pub simulator: Option<Vec<Violation>>,
    /// The 7-day new-card check; `None` without an event date.
    pub event: Option<EventCheck>,
    /// Exceptions that applied because the deck equals an official decklist (W-71).
    pub exceptions: Vec<AppliedException>,
    /// Hints for the violations (W-77).
    pub hints: Vec<Hint>,
}

impl LegalityReport {
    /// Every violation of the checks that ran: rules, simulator support, new cards.
    #[must_use]
    pub fn violation_count(&self) -> usize {
        self.rules.len()
            + self.simulator.as_ref().map_or(0, Vec::len)
            + self.event.as_ref().map_or(0, |e| e.violations.len())
    }

    /// Whether the deck passes every check that ran.
    #[must_use]
    pub fn is_legal(&self) -> bool {
        self.violation_count() == 0
    }
}

/// W-77: a CARD_TYPE violation of an EX token names the token rule and the size it counts
/// toward.
fn token_hints(src: &LegalitySources<'_>, deck: &Deck, rules: &[Violation]) -> Vec<Hint> {
    let mut out = Vec::new();
    for v in rules.iter().filter(|v| v.code == ViolationCode::CardType) {
        let Some(d) = v.cards.first().and_then(|n| src.db.get(n)) else {
            continue;
        };
        let (token, side) = match d.card_type {
            CardType::ExResource => (
                "EX Resource",
                format!(
                    "the resource deck's {} cards (RESOURCE_SIZE)",
                    src.construction.resource_size
                ),
            ),
            CardType::ExBase => (
                "EX Base",
                format!(
                    "the main deck's {} cards (MAIN_SIZE)",
                    src.construction.main_size
                ),
            ),
            _ => continue,
        };
        let line = deck
            .entries
            .iter()
            .find(|e| e.card_number == d.card_number)
            .map(|e| e.line);
        let at = line.map_or_else(String::new, |l| format!("line {l}: "));
        out.push(Hint {
            line,
            code: ViolationCode::CardType,
            text: format!(
                "{at}{} is the {token} token: each player prepares one EX Base and one EX \
                 Resource token outside the game (rule 6-1-2), so leave it out of the deck file; \
                 listed, its copies also count toward {side}",
                d.card_number
            ),
        });
    }
    out
}

/// The extended legality check of `deck`.
///
/// # Errors
/// [`SelectError`] when `opts.banlist` asks for a date or region that no list covers.
pub fn check(
    deck: &Deck,
    src: &LegalitySources<'_>,
    opts: &LegalityOptions<'_>,
) -> Result<LegalityReport, SelectError> {
    let entry = match opts.banlist {
        Some(req) => Some(src.lists.select(req.as_of, req.region)?),
        None => None,
    };
    let status = entry
        .map(|e| src.lists.exception_status(e, deck))
        .unwrap_or_default();
    let rules = Rules {
        db: src.db,
        construction: src.construction,
        banlist: entry.map_or(&src.lists.current().list, |e| &e.list),
        exceptions: Exceptions {
            exempt: &status.exempt,
            differs: &status.differs,
        },
    };
    let all = rules.validate(
        deck,
        ValidateOptions {
            banlist: entry.is_some(),
            implemented: opts.implemented,
        },
    );
    let (simulator, rule_violations): (Vec<Violation>, Vec<Violation>) = all
        .into_iter()
        .partition(|v| v.code == ViolationCode::Unimplemented);
    let hints = token_hints(src, deck, &rule_violations);
    Ok(LegalityReport {
        banlist: entry.cloned(),
        rules: rule_violations,
        simulator: opts.implemented.map(|_| simulator),
        event: opts
            .event_date
            .map(|d| releases::event_check(src.db, src.releases, deck, d)),
        exceptions: status.applied,
        hints,
    })
}

/// W-77's hints for a deck text that did not parse (or parsed with a `#` after a card).
#[must_use]
pub fn text_hints(db: &CardDb, text: &str) -> Vec<Hint> {
    deck::comment_hints(db, text)
}

/// [`text_hints`] of a deck file read as `load_deck` reads it (one BOM stripped, strict UTF-8,
/// universal newlines); empty when the file cannot be read or decoded.
#[must_use]
pub fn file_hints(db: &CardDb, path: &Path) -> Vec<Hint> {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| deck::decode_deck_bytes(&bytes).ok())
        .map_or_else(Vec::new, |text| text_hints(db, &text))
}
