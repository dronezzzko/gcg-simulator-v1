//! Contract types of `gcg-data` (docs/dev/PLAN.md §10.3.1), owned by the lead (§10.1 S).
//!
//! Workstream A implements the functions frozen in docs/dev/contracts.md (gcg-data section)
//! with real bodies; only the lead edits this file (§10.6 change protocol). Fields marked
//! `pub(crate)` are the lead's starting layout of an opaque type: A may change them in its
//! landing diff, because no other crate can observe them.
//!
//! Python sources are cited relative to `src/gcg_sim/`.

use std::io;
use std::path::PathBuf;

use gcg_core::lookup::LookupMap;
use gcg_core::pyjson::Json;

// ---------------------------------------------------------------------------------------------
// Constants (cards/db.py, deck/parse.py, deck/validate.py, reports/versions.py)

/// `EX_BASE_NUMBER` (`cards/db.py:42`).
pub const EX_BASE_NUMBER: &str = "EXB-001";
/// `EX_RESOURCE_NUMBER` (`cards/db.py:43`).
pub const EX_RESOURCE_NUMBER: &str = "EXR-001";
/// `UNIT_VARIANT_PREFIX` (`cards/db.py:46`): synthetic Unit variants are `UNIT:<number>:<ap>/<hp>`.
pub const UNIT_VARIANT_PREFIX: &str = "UNIT:";
/// `SYNTHETIC_PREFIX` (`deck/parse.py:20`): synthetic inline tokens are `TOKEN:<name>:<ap>/<hp>`.
pub const SYNTHETIC_PREFIX: &str = "TOKEN:";
/// `NOT_APPLIED` (`deck/validate.py:17`), used by `_exception_note` (`:29-30`).
pub const NOT_APPLIED: &str =
    "is not applied: the simulator cannot verify an unmodified starter deck";
/// `PACKAGE` (`reports/versions.py:15`): the distribution name.
pub const PACKAGE: &str = "gcg-sim";
/// `SCHEMA_VERSION` (`reports/versions.py:16`): results schema written by the reference.
pub const RESULTS_SCHEMA_VERSION: &str = "1.1.0";
/// `SCHEMA_ID` (`reports/versions.py:17`).
pub const RESULTS_SCHEMA_ID: &str = "urn:gcg-sim:schema:results:1.1.0";
/// `MAIN_DECK_TYPES` (`cards/model.py:37`); a frozenset used for membership only.
pub const MAIN_DECK_TYPES: [CardType; 4] = [
    CardType::Unit,
    CardType::Pilot,
    CardType::Command,
    CardType::Base,
];
/// W-144: the largest card total a [`Deck`] list is ever expanded to. A game addresses cards
/// by a `u16` uid, so no larger deck can be played; `to_decklist` refuses larger totals with
/// [`DeckError::TooLarge`] instead of allocating one entry per copy.
pub const MAX_EXPANDED_CARDS: u128 = u16::MAX as u128;

// ---------------------------------------------------------------------------------------------
// Card model (cards/model.py)

/// Index of a definition in [`CardDb`] order (`cards/db.py:119-158`): the 1152 real card
/// numbers in code-point order, then synthetic `TOKEN:` defs, then Unit variants (1153 today).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct DefId(pub u16);

/// `CardType` (`cards/model.py:10-34`), a `StrEnum`; discriminants are the declaration order.
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum CardType {
    /// `UNIT = "UNIT"`
    Unit = 0,
    /// `PILOT = "PILOT"`
    Pilot = 1,
    /// `COMMAND = "COMMAND"`
    Command = 2,
    /// `BASE = "BASE"`
    Base = 3,
    /// `RESOURCE = "RESOURCE"`
    Resource = 4,
    /// `UNIT_TOKEN = "UNIT TOKEN"`
    UnitToken = 5,
    /// `EX_BASE = "EX BASE"`
    ExBase = 6,
    /// `EX_RESOURCE = "EX RESOURCE"`
    ExResource = 7,
}

impl CardType {
    /// Declaration order.
    pub const ALL: [Self; 8] = [
        Self::Unit,
        Self::Pilot,
        Self::Command,
        Self::Base,
        Self::Resource,
        Self::UnitToken,
        Self::ExBase,
        Self::ExResource,
    ];

    /// The value string (`card_type.value`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unit => "UNIT",
            Self::Pilot => "PILOT",
            Self::Command => "COMMAND",
            Self::Base => "BASE",
            Self::Resource => "RESOURCE",
            Self::UnitToken => "UNIT TOKEN",
            Self::ExBase => "EX BASE",
            Self::ExResource => "EX RESOURCE",
        }
    }

    /// `CardType(value)`; `None` where Python raises `ValueError`.
    #[must_use]
    pub fn from_value(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.as_str() == value)
    }

    /// `is_token` (`cards/model.py:20-22`).
    #[must_use]
    pub const fn is_token(self) -> bool {
        matches!(self, Self::UnitToken | Self::ExBase | Self::ExResource)
    }

    /// `is_unit` (`cards/model.py:24-26`).
    #[must_use]
    pub const fn is_unit(self) -> bool {
        matches!(self, Self::Unit | Self::UnitToken)
    }

    /// `is_base` (`cards/model.py:28-30`).
    #[must_use]
    pub const fn is_base(self) -> bool {
        matches!(self, Self::Base | Self::ExBase)
    }

    /// `is_resource` (`cards/model.py:32-34`).
    #[must_use]
    pub const fn is_resource(self) -> bool {
        matches!(self, Self::Resource | Self::ExResource)
    }
}

/// `Color` (`cards/model.py:40-45`), a `StrEnum`; discriminants are the declaration order.
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Color {
    /// `BLUE = "Blue"`
    Blue = 0,
    /// `GREEN = "Green"`
    Green = 1,
    /// `RED = "Red"`
    Red = 2,
    /// `WHITE = "White"`
    White = 3,
    /// `PURPLE = "Purple"`
    Purple = 4,
}

impl Color {
    /// Declaration order.
    pub const ALL: [Self; 5] = [
        Self::Blue,
        Self::Green,
        Self::Red,
        Self::White,
        Self::Purple,
    ];

    /// The value string (`color.value`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Blue => "Blue",
            Self::Green => "Green",
            Self::Red => "Red",
            Self::White => "White",
            Self::Purple => "Purple",
        }
    }

    /// `Color(value)`; `None` where Python raises `ValueError`.
    #[must_use]
    pub fn from_value(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.as_str() == value)
    }
}

/// `LinkCondition(name_parts, traits)` (`cards/model.py:48-56`); `parse_link` (`:63-74`).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct LinkCondition {
    pub name_parts: Box<[Box<str>]>,
    pub traits: Box<[Box<str>]>,
}

/// `CardDef` (`cards/model.py:92-141`): the public fields in Python's field order, built by
/// `_make_def` (`cards/db.py:160-197`), `_register_inline_tokens` (`:199-241`) and
/// `_register_unit_variants` (`:243-272`).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct CardDef {
    pub def_id: DefId,
    pub card_number: Box<str>,
    pub name: Box<str>,
    pub card_type: CardType,
    pub color: Option<Color>,
    pub level: i32,
    pub cost: i32,
    pub ap: i32,
    pub hp: i32,
    pub zones: Box<[Box<str>]>,
    pub traits: Box<[Box<str>]>,
    pub link: Option<LinkCondition>,
    pub effect: Box<str>,
    pub pilot_name: Option<Box<str>>,
    pub set_code: Box<str>,
    pub rarity: Box<str>,
    pub product_ids: Box<[Box<str>]>,
    pub canonical_product_id: Box<str>,
    pub source_title: Option<Box<str>>,
    pub extra_names: Box<[Box<str>]>,
    /// The `names` property (`cards/model.py:117-123`), computed once at construction:
    /// `(name, *extra_names)` plus `pilot_name` if truthy and not already present.
    pub(crate) names: Box<[Box<str>]>,
}

impl CardDef {
    /// `CardDef.names` (`cards/model.py:117-123`).
    #[must_use]
    pub fn names(&self) -> &[Box<str>] {
        &self.names
    }

    /// `CardDef.is_token` (`cards/model.py:125-127`).
    #[must_use]
    pub const fn is_token(&self) -> bool {
        self.card_type.is_token()
    }

    /// `CardDef.is_unit_card` (`cards/model.py:129-131`).
    #[must_use]
    pub const fn is_unit_card(&self) -> bool {
        self.card_type.is_unit()
    }

    /// `CardDef.is_pilot_capable` (`cards/model.py:133-137`).
    #[must_use]
    pub const fn is_pilot_capable(&self) -> bool {
        matches!(self.card_type, CardType::Pilot)
            || (matches!(self.card_type, CardType::Command) && self.pilot_name.is_some())
    }
}

/// `AppliedOverride(conflict_id, card_number, decision, fields)` (`cards/db.py:72-77`), in
/// `CardDB.applied_overrides` order (card numbers sorted, then conflict ids sorted).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct AppliedOverride {
    pub conflict_id: Box<str>,
    pub card_number: Box<str>,
    pub decision: Box<str>,
    pub fields: Box<[Box<str>]>,
}

/// `CardDB` (`cards/db.py:116-324`): definitions in def_id order and the lookup tables of
/// `get`, `token_for`, `unit_variant` and `base_def_id`.
#[allow(
    dead_code,
    reason = "built and read by workstream A's loader (docs/dev/contracts.md)"
)]
pub struct CardDb {
    /// `_defs`, index = def_id.
    pub(crate) defs: Box<[CardDef]>,
    /// `_by_number`: the real card numbers only (`TOKEN:`/`UNIT:` keys are absent).
    pub(crate) by_number: LookupMap<Box<str>, DefId>,
    /// `_product_to_number`, resolved to the def of that number.
    pub(crate) product_to_def: LookupMap<Box<str>, DefId>,
    /// `_token_by_key`, sorted by key (code-point order, as the fingerprint lists it).
    pub(crate) token_by_key: Box<[(Box<str>, DefId)]>,
    /// `_unit_variant` by def_id.
    pub(crate) unit_variant: Box<[Option<DefId>]>,
    /// `_variant_base` by def_id.
    pub(crate) variant_base: Box<[Option<DefId>]>,
    pub(crate) applied_overrides: Box<[AppliedOverride]>,
}

/// Why the card database could not be built. W-157: a missing, unreadable or unparsable
/// overrides file is an error naming the file, never an empty default.
#[derive(Debug)]
pub enum DataError {
    /// A data file could not be read (W-157).
    Io { path: PathBuf, error: io::Error },
    /// A data file is not valid JSON; `line` is 1-based for `cards.ndjson`, 0 otherwise.
    Json {
        file: Box<str>,
        line: usize,
        message: String,
    },
    /// A record or table has an unexpected shape (missing key, wrong JSON type).
    Shape { file: Box<str>, detail: String },
    /// `override names unknown canonical printing {preferred!r}` (`cards/db.py:105`).
    UnknownCanonicalPrinting {
        card_number: Box<str>,
        preferred: Box<str>,
    },
    /// `CardType(rec["card_type"])` raised `ValueError` (`cards/db.py:162`).
    UnknownCardType {
        card_number: Box<str>,
        value: Box<str>,
    },
    /// `Color(rec["color"])` raised `ValueError` (`cards/db.py:163`).
    UnknownColor {
        card_number: Box<str>,
        value: Box<str>,
    },
    /// `unparseable link condition: {raw!r}` (`cards/model.py:73`).
    UnparseableLink {
        card_number: Box<str>,
        raw: Box<str>,
    },
    /// `int(raw.strip().replace("+", ""))` raised `ValueError` (`cards/model.py:81`).
    BadModifier {
        card_number: Box<str>,
        raw: Box<str>,
    },
    /// `unknown banlist predicate operator {key!r}` (`deck/official.py:157`). Python raises it
    /// lazily inside `matches`; Rust checks every operator when the banlist loads, so
    /// `official::matches` cannot fail.
    UnknownPredicateOperator { rule_id: Box<str>, key: Box<str> },
}

// ---------------------------------------------------------------------------------------------
// Decks (deck/model.py, deck/parse.py)

/// The engine's `DeckList` (`engine/game.py:41-45`) with card numbers resolved to def_ids.
///
/// Keeps the caller's order: `new_game` maps `main` in this order, then shuffles and cuts
/// (`game.py:134-140`), and maps `resources` in order to uids, so the order is RNG-relevant.
/// Only `to_decklist(parse_deck(..))` yields sorted card numbers (`deck/parse.py:104`); fuzz,
/// coverage and testkit decks are unsorted (`tools/fuzz.py:57-70`). No constructor sorts or
/// canonicalises. A unit test: one multiset in two orders gives different shuffles, equal to
/// Python vectors.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct DeckList {
    pub main: Box<[DefId]>,
    pub resources: Box<[DefId]>,
}

/// `DeckEntry(count, card_number, written, name, line)` (`deck/model.py:9-17`). `count` is
/// read as `u64`; a count that does not fit is a SYNTAX violation naming the line (W-144).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct DeckEntry {
    pub count: u64,
    pub card_number: Box<str>,
    pub written: Box<str>,
    pub name: Option<Box<str>>,
    /// 1-based line number in `text.splitlines()` order.
    pub line: usize,
}

/// One card number of a deck with its total number of copies.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct CardCount {
    pub card_number: Box<str>,
    /// Sum over the deck's entries for this number (`u128`: entries are `u64` each).
    pub count: u128,
}

/// `Deck(name, main, resources, entries, source)` (`deck/model.py:20-28`) plus its
/// `deck_digest` (`deck/parse.py:125-129`).
///
/// W-144: Python's `main`/`resources` are sorted tuples with one card number per copy. Here
/// they are the same lists run-length encoded: sorted by card number, one [`CardCount`] per
/// distinct number. Expanding them in order gives Python's tuples exactly, and sizes, copy
/// limits and the digest are computed from the counts without allocating per copy.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Deck {
    pub name: Box<str>,
    pub main: Box<[CardCount]>,
    pub resources: Box<[CardCount]>,
    pub entries: Box<[DeckEntry]>,
    pub source: Option<Box<str>>,
    /// `deck_digest(deck)`: lowercase SHA-256 hex.
    pub digest: String,
}

/// The 13 violation codes (`deck/model.py:31-37`, `deck/parse.py:45-69`,
/// `deck/validate.py`), in the order `validate_deck` reports them (`:268-289`; SYNTAX,
/// UNKNOWN_ID and NAME_MISMATCH also come from `parse_deck`). Each variant documents its exact
/// message (Python f-string); `{label}` is `f"{number} {cdef.name}"` or the bare number when
/// unknown (`validate.py:20-22`), `{labels}` joins labels with `", "` (`:25-26`), `{date}` is
/// the B&R `effective_date`, and `{x!r}` is CPython `repr`.
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum ViolationCode {
    /// `line {n}: expected '<count> <card_number> [name]', got {line!r}` and
    /// `line {n}: count must be at least 1 for {written}` (`deck/parse.py:48-56`); W-144 adds a
    /// SYNTAX violation naming the line for a count that does not fit `u64`.
    Syntax = 0,
    /// `line {n}: unknown card id {written!r} (not in the card data)` (`deck/parse.py:59-61`);
    /// `unknown card id {n!r} (not in the card data)` (`deck/validate.py:39-44`).
    UnknownId = 1,
    /// `line {n}: {written} is {cdef.name!r} in the card data, not {name!r}`
    /// (`deck/parse.py:63-68`, `deck/validate.py:47-60`).
    NameMismatch = 2,
    /// `the deck has {len(main)} cards; it must have exactly {main_size} (rule 6-1-1)`
    /// (`deck/validate.py:66-74`).
    MainSize = 3,
    /// `the resource deck has {len(resources)} cards; it must have exactly {resource_size}
    /// Resource cards (rule 6-1-1)` (`deck/validate.py:75-83`).
    ResourceSize = 4,
    /// `_type_message` (`deck/validate.py:87-102`): `{num} {name} is a token ({type}); tokens
    /// are prepared outside the game and are never part of a deck or resource deck (rule 6-1-2,
    /// FAQ Q7)`, `{num} {name} is a {type} card; a resource deck is constructed with Resource
    /// cards only (rule 6-1-1-4)`, `{num} {name} is a {type} card; a deck is constructed with
    /// Unit, Pilot, Command, and Base cards only (rule 6-1-1-1)`.
    CardType = 5,
    /// `{count} copies of {label}; at most {max_copies} copies of cards with the same card
    /// number (all printings count together) may be in a deck (rules 6-1-1-3, 2-1-2)`
    /// (`deck/validate.py:119-130`).
    CopyLimit = 6,
    /// `the deck uses {len(copies)} colors ({shown}); a deck must be constructed entirely
    /// using either one or two card colors (rule 6-1-1-2). Cards of the extra color(s):
    /// {labels}` with `shown = ", ".join(f"{color} x{n}")` (`deck/validate.py:133-151`).
    Colors = 7,
    /// `{label} is banned: no copies may be in a deck (Banned & Restricted list effective
    /// {date})` (`deck/validate.py:154-165`).
    Banned = 8,
    /// `{count} copies of {label}; it is restricted to {max} copies (Banned & Restricted list
    /// effective {date}).{exception_note}` (`deck/validate.py:168-184`); `exception_note` is
    /// `"".join(f" Exception {e.id!r} {NOT_APPLIED}.")` (`:29-30`).
    Restricted = 9,
    /// `{labels(p.cards)} are a banned pair and cannot be in the same deck (Banned &
    /// Restricted list effective {date})` (`deck/validate.py:187-199`).
    BannedPair = 10,
    /// `{labels(members)} are each "{description}"; every combination of two such card numbers
    /// is a banned pair, so a deck may contain only one of them, at most {max_total} copies
    /// (Banned & Restricted list effective {date}).{exception_note}{note_text}` with
    /// `note_text = "".join(f" Note: {note}.")` (`deck/validate.py:202-240`).
    AttributePair = 11,
    /// `{label} has no implemented effect in this simulator version, so games cannot use it
    /// ({entry.error})` (`deck/validate.py:248-265`).
    Unimplemented = 12,
}

impl ViolationCode {
    /// Declaration order (= `validate_deck` check order).
    pub const ALL: [Self; 13] = [
        Self::Syntax,
        Self::UnknownId,
        Self::NameMismatch,
        Self::MainSize,
        Self::ResourceSize,
        Self::CardType,
        Self::CopyLimit,
        Self::Colors,
        Self::Banned,
        Self::Restricted,
        Self::BannedPair,
        Self::AttributePair,
        Self::Unimplemented,
    ];

    /// The code string (`Violation.code`), as `[{code}]` prints it (`cli.py:119-136`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Syntax => "SYNTAX",
            Self::UnknownId => "UNKNOWN_ID",
            Self::NameMismatch => "NAME_MISMATCH",
            Self::MainSize => "MAIN_SIZE",
            Self::ResourceSize => "RESOURCE_SIZE",
            Self::CardType => "CARD_TYPE",
            Self::CopyLimit => "COPY_LIMIT",
            Self::Colors => "COLORS",
            Self::Banned => "BANNED",
            Self::Restricted => "RESTRICTED",
            Self::BannedPair => "BANNED_PAIR",
            Self::AttributePair => "ATTRIBUTE_PAIR",
            Self::Unimplemented => "UNIMPLEMENTED",
        }
    }
}

/// `Violation(code, message, cards)` (`deck/model.py:31-37`).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Violation {
    pub code: ViolationCode,
    pub message: String,
    pub cards: Box<[Box<str>]>,
}

/// Whether each definition has an implemented effect: `registry.cards[def_id].script is None`
/// (`deck/validate.py:248-265`, `cli.py:236-248`). `gcg-data` cannot depend on the effect
/// registry (PLAN §2.2), so `gcg_effects::Registry` implements this trait and the callers of
/// `validate` pass it in.
pub trait ImplementationStatus {
    /// `Some(entry.error)` when the definition has no script, else `None`.
    fn unimplemented_error(&self, def: DefId) -> Option<&str>;
}

/// The keyword arguments of `validate_deck(deck, *, banlist=True, implemented=True)`
/// (`deck/validate.py:268-289`): `implemented: None` skips check 11 (UNIMPLEMENTED).
#[derive(Clone, Copy)]
pub struct ValidateOptions<'a> {
    pub banlist: bool,
    pub implemented: Option<&'a dyn ImplementationStatus>,
}

/// Every way loading or converting a deck fails. `Display` is Python's text: `Invalid` renders
/// `str(DeckError)` (`deck/model.py:55-58`: `{subject} has {n} problem(s):` and `  - {p}`
/// lines, `subject = f"deck {deck_name!r}"` or `deck`); `Io` and `Decode` render the CLI texts
/// of `cli.py:102-110`.
#[derive(Debug)]
pub enum DeckError {
    /// `cannot read deck file {path}: {exc.strerror or exc}`.
    Io { path: PathBuf, error: io::Error },
    /// `{path}: not UTF-8 text ({reason} at byte {start})`; `start` counts from after the
    /// stripped BOM.
    Decode {
        path: PathBuf,
        reason: &'static str,
        start: usize,
    },
    /// `DeckError(problems, deck_name=..., violations=...)` from `parse_deck` (messages) or
    /// `require_legal` (`[{code}] {message}`, `deck/validate.py:292-300`).
    Invalid {
        deck_name: Option<Box<str>>,
        problems: Vec<String>,
        violations: Vec<Violation>,
    },
    /// W-144: `to_decklist` would expand more than [`MAX_EXPANDED_CARDS`] copies.
    TooLarge {
        deck_name: Box<str>,
        main: u128,
        resources: u128,
    },
}

// ---------------------------------------------------------------------------------------------
// Official data (deck/official.py, reports/versions.py)

/// `ConstructionRules` (`deck/official.py:19-28`), from `official/deck_construction.json`
/// (`:74-86`).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct ConstructionRules {
    pub main_size: u32,
    pub resource_size: u32,
    pub max_copies: u32,
    /// `frozenset` of card type values; membership only.
    pub main_types: Box<[Box<str>]>,
    /// `frozenset` of card type values; membership only.
    pub resource_types: Box<[Box<str>]>,
    pub min_colors: u32,
    pub max_colors: u32,
    pub effective_date: Box<str>,
}

/// `RuleException(id, description)` (`deck/official.py:31-34`).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct RuleException {
    pub id: Box<str>,
    pub description: Box<str>,
}

/// `Restricted(card_number, max_copies, exceptions)` (`deck/official.py:37-41`).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Restricted {
    pub card_number: Box<str>,
    pub max_copies: u32,
    pub exceptions: Box<[RuleException]>,
}

/// `BannedPair(id, cards)` (`deck/official.py:44-47`); only `cards[0]` and `cards[1]` are read
/// (`:123`).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct BannedPair {
    pub id: Box<str>,
    pub cards: [Box<str>; 2],
}

/// `AttributePairRule` (`deck/official.py:50-57`, built by `_attribute_rule` `:101-109`).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct AttributePairRule {
    pub id: Box<str>,
    /// First `"..."` substring of `source_text`, else the rule id (`:93-98`).
    pub description: Box<str>,
    /// The `member_predicate` object; `matches` walks its keys in JSON order with
    /// short-circuit (`:160-173`), so the map keeps insertion order.
    pub member_predicate: serde_json::Value,
    /// `frozenset` of card numbers, stored sorted; membership only.
    pub enumerated: Box<[Box<str>]>,
    pub max_total_copies: u32,
    pub exceptions: Box<[RuleException]>,
}

/// `Banlist` (`deck/official.py:60-66`, `banlist()` `:112-127`); lists keep file order.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Banlist {
    pub effective_date: Box<str>,
    /// `frozenset` of card numbers, stored sorted; membership only.
    pub banned: Box<[Box<str>]>,
    pub restricted: Box<[Restricted]>,
    pub banned_pairs: Box<[BannedPair]>,
    pub attribute_rules: Box<[AttributePairRule]>,
}

/// The two fields read from `official/rules_version.json` (`reports/versions.py:58,66-67`).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct RulesVersion {
    pub latest_version: Box<str>,
    pub effective_date: Box<str>,
}

/// The one field read from `official/bo3_match_rules.json` (`reports/versions.py:72-74`).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Bo3Rules {
    pub effective_date: Box<str>,
}

/// `data_status()` (`cli.py:263-291`) without `coverage`, which needs the effect registry and
/// is assembled by the CLI. Each part is the exact JSON value Python builds (key order as
/// built; `--json` output sorts keys), so the CLI renders both the `--json` and the text form
/// from it (`cli.py:294-331`).
#[derive(Clone, PartialEq, Debug)]
pub struct DataStatusParts {
    /// `versions()` (`reports/versions.py:55-75`).
    pub versions: Json,
    /// `{k: manifest.get(k)}` for the nine keys of `cli.py:268-281` (missing keys are `null`).
    pub snapshot: Json,
    /// `cli.py:282-288`.
    pub banlist: Json,
    /// `_conflicts()` (`cli.py:251-260`).
    pub conflicts: Json,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enum_values_are_pythons() {
        let types: Vec<&str> = CardType::ALL.into_iter().map(CardType::as_str).collect();
        assert_eq!(
            types,
            [
                "UNIT",
                "PILOT",
                "COMMAND",
                "BASE",
                "RESOURCE",
                "UNIT TOKEN",
                "EX BASE",
                "EX RESOURCE"
            ]
        );
        for (i, t) in CardType::ALL.into_iter().enumerate() {
            assert_eq!(t as usize, i);
            assert_eq!(CardType::from_value(t.as_str()), Some(t));
        }
        assert_eq!(CardType::from_value("unit"), None);
        for (i, c) in Color::ALL.into_iter().enumerate() {
            assert_eq!(c as usize, i);
            assert_eq!(Color::from_value(c.as_str()), Some(c));
        }
        let tokens: Vec<CardType> = CardType::ALL.into_iter().filter(|t| t.is_token()).collect();
        assert_eq!(
            tokens,
            [CardType::UnitToken, CardType::ExBase, CardType::ExResource]
        );
    }

    #[test]
    fn thirteen_violation_codes_in_check_order() {
        let codes: Vec<&str> = ViolationCode::ALL
            .into_iter()
            .map(ViolationCode::as_str)
            .collect();
        assert_eq!(
            codes,
            [
                "SYNTAX",
                "UNKNOWN_ID",
                "NAME_MISMATCH",
                "MAIN_SIZE",
                "RESOURCE_SIZE",
                "CARD_TYPE",
                "COPY_LIMIT",
                "COLORS",
                "BANNED",
                "RESTRICTED",
                "BANNED_PAIR",
                "ATTRIBUTE_PAIR",
                "UNIMPLEMENTED"
            ]
        );
        for (i, c) in ViolationCode::ALL.into_iter().enumerate() {
            assert_eq!(c as usize, i);
        }
    }
}
