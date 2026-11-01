//! Deck files (`deck/parse.py`, `deck/model.py`): the `<count> <card_number> [name]` grammar
//! with Python's behaviour on exotic input (Unicode digits and whitespace, `str.splitlines`),
//! assembly into sorted run-length lists, the digest, and `DeckError`'s texts (Inv 03 §3).
//!
//! W-144: counts are read as `u64`; a count that does not fit is a SYNTAX violation naming
//! the line, and no list is ever expanded per copy before `to_decklist` checks its size.

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;

use gcg_core::pyjson::{DumpOptions, Json, dumps};
use sha2::{Digest, Sha256};

use crate::contracts::{
    CardCount, CardDb, Deck, DeckEntry, DeckError, DeckList, MAX_EXPANDED_CARDS, SYNTHETIC_PREFIX,
    Violation, ViolationCode,
};
use crate::{names, py, unicode};

/// The UTF-8 byte order mark that `encoding="utf-8-sig"` strips once.
const BOM: &[u8] = b"\xEF\xBB\xBF";

fn violation(code: ViolationCode, message: String, cards: &[&str]) -> Violation {
    Violation {
        code,
        message,
        cards: cards.iter().map(|&c| c.into()).collect(),
    }
}

/// `_id_candidates(written)` (`deck/parse.py:23-31`).
fn id_candidates(written: &str) -> Vec<String> {
    let mut out = vec![written.to_owned(), unicode::upper(written)];
    if let Some((base, suffix)) = written.split_once('_') {
        out.push(format!(
            "{}_{}",
            unicode::upper(base),
            unicode::lower(suffix)
        ));
    }
    if let Some(base) = alt_art_base(written) {
        out.push(unicode::upper(base));
    }
    out
}

/// `_ALT_ART.match(written)` (`deck/parse.py:19`): `^(?P<base>.+?)_[pP]\d+$`, the shortest
/// non-empty base whose rest is `_p<digits>`. `written` is a `\S+` group, so it holds no
/// newline for `.` or `$` to treat specially.
fn alt_art_base(written: &str) -> Option<&str> {
    written
        .char_indices()
        .skip(1)
        .filter(|&(_, c)| c == '_')
        .find_map(|(i, _)| {
            let rest = &written[i + 1..];
            let digits = rest.strip_prefix(['p', 'P'])?;
            let all_digits =
                !digits.is_empty() && digits.chars().all(|c| unicode::decimal_value(c).is_some());
            all_digits.then(|| &written[..i])
        })
}

/// `resolve_card_number(written, db)` (`deck/parse.py:34-42`): the canonical card number for
/// a card number or product id (alt-art `_pN` printings normalise to their card number).
#[must_use]
pub fn resolve_card_number<'a>(written: &str, db: &'a CardDb) -> Option<&'a str> {
    id_candidates(written).iter().find_map(|c| {
        db.get(c)
            .filter(|d| !d.card_number.starts_with(SYNTHETIC_PREFIX))
            .map(|d| &*d.card_number)
    })
}

/// The groups of `_ENTRY` (`deck/parse.py:18`):
/// `^(?P<count>\d+)[xX]?\s+(?P<id>\S+)(?:\s+(?P<name>.*\S))?\s*$` on a stripped line.
struct EntryMatch<'a> {
    count: &'a str,
    id: &'a str,
    name: Option<&'a str>,
}

/// `_ENTRY.match(line)`. Each quantifier's backtracking has one viable choice: the digits must
/// run up to the optional `x` and the whitespace, the id up to whitespace, and the name from
/// the end of that whitespace to the last non-space character.
fn match_entry(line: &str) -> Option<EntryMatch<'_>> {
    let count_end = line
        .char_indices()
        .find(|&(_, c)| unicode::decimal_value(c).is_none())
        .map_or(line.len(), |(i, _)| i);
    if count_end == 0 {
        return None;
    }
    let mut i = count_end;
    if line[i..].starts_with(['x', 'X']) {
        i += 1;
    }
    let ws = line[i..].len() - line[i..].trim_start_matches(unicode::is_space).len();
    if ws == 0 {
        return None;
    }
    let id_start = i + ws;
    let id_len = line[id_start..]
        .find(unicode::is_space)
        .unwrap_or(line.len() - id_start);
    if id_len == 0 {
        return None;
    }
    let id_end = id_start + id_len;
    let rest = &line[id_end..];
    let name = rest.trim_start_matches(unicode::is_space);
    let name = name.trim_end_matches(unicode::is_space);
    Some(EntryMatch {
        count: &line[..count_end],
        id: &line[id_start..id_end],
        name: (!name.is_empty()).then_some(name),
    })
}

/// `sys.int_info.default_max_str_digits`: `int()` raises `ValueError` on a longer digit string
/// (leading zeros included), which escapes `parse_deck` (W-144).
const PY_MAX_STR_DIGITS: usize = 4300;

/// `int()` of the count digits as a `u64`; `None` when it does not fit (W-144).
fn count_value(digits: &str) -> Option<u64> {
    let mut value: u64 = 0;
    for c in digits.chars() {
        let d = unicode::decimal_value(c)?;
        value = value.checked_mul(10)?.checked_add(u64::from(d))?;
    }
    Some(value)
}

/// One deck line's outcome.
enum Line {
    Entry(DeckEntry),
    Problem(Violation),
    /// A count Python converts but cannot expand (`[number] * count` in `build_deck`): the
    /// line is otherwise valid, so it becomes a SYNTAX problem only when no other problem
    /// stops Python before `build_deck` (W-144).
    Overflow(Violation),
}

/// W-144's SYNTAX violation for a count that does not fit a `u64`.
fn count_too_large(lineno: usize, written: &str) -> Violation {
    violation(
        ViolationCode::Syntax,
        format!(
            "line {lineno}: count must be at most {} for {written}",
            u64::MAX
        ),
        &[],
    )
}

/// `_parse_line(db, lineno, line)` (`deck/parse.py:45-69`): the first failing step wins. A
/// count longer than `int()` accepts is a SYNTAX problem where Python raises; a count that
/// Python converts but that does not fit a `u64` is checked like any other line first.
fn parse_line(db: &CardDb, lineno: usize, line: &str) -> Line {
    let Some(m) = match_entry(line) else {
        return Line::Problem(violation(
            ViolationCode::Syntax,
            format!(
                "line {lineno}: expected '<count> <card_number> [name]', got {}",
                unicode::repr_str(line)
            ),
            &[],
        ));
    };
    let written = m.id;
    if m.count.chars().count() > PY_MAX_STR_DIGITS {
        return Line::Problem(count_too_large(lineno, written));
    }
    let count = count_value(m.count);
    if count == Some(0) {
        return Line::Problem(violation(
            ViolationCode::Syntax,
            format!("line {lineno}: count must be at least 1 for {written}"),
            &[],
        ));
    }
    let Some(number) = resolve_card_number(written, db) else {
        return Line::Problem(violation(
            ViolationCode::UnknownId,
            format!(
                "line {lineno}: unknown card id {} (not in the card data)",
                unicode::repr_str(written)
            ),
            &[],
        ));
    };
    let cdef = db
        .get(number)
        .expect("a resolved card number is in the card data");
    if let Some(name) = m.name
        && !names::names_match(name, cdef.names())
    {
        return Line::Problem(violation(
            ViolationCode::NameMismatch,
            format!(
                "line {lineno}: {written} is {} in the card data, not {}",
                unicode::repr_str(&cdef.name),
                unicode::repr_str(name)
            ),
            &[number],
        ));
    }
    let Some(count) = count else {
        return Line::Overflow(count_too_large(lineno, written));
    };
    Line::Entry(DeckEntry {
        count,
        card_number: number.into(),
        written: written.into(),
        name: m.name.map(Into::into),
        line: lineno,
    })
}

/// `parse_deck(text, name=..., source=...)` (`deck/parse.py:72-89`) over a given database.
pub(crate) fn parse_deck_with(
    db: &CardDb,
    text: &str,
    name: &str,
    source: Option<&str>,
) -> Result<Deck, DeckError> {
    let mut entries = Vec::new();
    let mut problems = Vec::new();
    let mut overflows = Vec::new();
    for (i, raw) in unicode::splitlines(text).into_iter().enumerate() {
        let stripped = names::strip_invisible(raw);
        let line = unicode::strip(&stripped);
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        match parse_line(db, i + 1, line) {
            Line::Entry(e) => entries.push(e),
            Line::Problem(v) => problems.push(v),
            Line::Overflow(v) => overflows.push(v),
        }
    }
    if problems.is_empty() {
        problems = overflows;
    }
    if !problems.is_empty() {
        return Err(DeckError::Invalid {
            deck_name: Some(name.into()),
            problems: problems.iter().map(|v| v.message.clone()).collect(),
            violations: problems,
        });
    }
    Ok(build_deck(db, entries, name, source))
}

/// `parse_deck(text, name=..., source=...)` (`deck/parse.py:72-89`): every SYNTAX,
/// UNKNOWN_ID and NAME_MISMATCH problem in one [`DeckError::Invalid`], in line order.
/// Legality is checked separately by [`crate::validate`].
///
/// # Errors
/// [`DeckError::Invalid`] listing the problems.
pub fn parse_deck(text: &str, name: &str, source: Option<&str>) -> Result<Deck, DeckError> {
    parse_deck_with(CardDb::embedded(), text, name, source)
}

/// Run-length list of `(number, copies)` from per-entry counts, sorted by card number.
fn counts(map: BTreeMap<Box<str>, u128>) -> Box<[CardCount]> {
    map.into_iter()
        .map(|(card_number, count)| CardCount { card_number, count })
        .collect()
}

/// `build_deck(entries, name=..., source=...)` (`deck/parse.py:92-108`): RESOURCE and
/// EX RESOURCE cards go to the resource deck, everything else to the main deck; both lists
/// sorted by card number.
pub(crate) fn build_deck(
    db: &CardDb,
    entries: Vec<DeckEntry>,
    name: &str,
    source: Option<&str>,
) -> Deck {
    let mut main: BTreeMap<Box<str>, u128> = BTreeMap::new();
    let mut resources: BTreeMap<Box<str>, u128> = BTreeMap::new();
    for e in &entries {
        let to_resources = db
            .get(&e.card_number)
            .is_some_and(|d| d.card_type.is_resource());
        let target = if to_resources {
            &mut resources
        } else {
            &mut main
        };
        *target.entry(e.card_number.clone()).or_insert(0) += u128::from(e.count);
    }
    let mut deck = Deck {
        name: name.into(),
        main: counts(main),
        resources: counts(resources),
        entries: entries.into(),
        source: source.map(Into::into),
        digest: String::new(),
    };
    deck.digest = deck_digest(&deck);
    deck
}

/// `deck_digest(deck)` (`deck/parse.py:121-129`): SHA-256 hex of
/// `json.dumps({"main": counts, "resources": counts}, sort_keys=True, separators=(",", ":"))`.
#[must_use]
pub fn deck_digest(deck: &Deck) -> String {
    let side = |cards: &[CardCount]| {
        Json::Object(
            cards
                .iter()
                .map(|c| {
                    let n = i128::try_from(c.count).unwrap_or(i128::MAX);
                    (c.card_number.to_string().into(), Json::Int(n))
                })
                .collect(),
        )
    };
    let mut canonical = Json::object();
    canonical.set("main", side(&deck.main));
    canonical.set("resources", side(&deck.resources));
    let blob = dumps(&canonical, DumpOptions::COMPACT_SORTED_ASCII);
    py::hex(&Sha256::digest(blob.as_bytes()))
}

/// `Path(path).read_text(encoding="utf-8-sig")` (`deck/parse.py:113`, Inv 03 §3.1): one
/// leading BOM stripped, strict UTF-8 with CPython's error reason and the offset counted after
/// the BOM, then universal newlines (`\r\n` and `\r` become `\n`).
pub(crate) fn decode_deck_bytes(bytes: &[u8]) -> Result<String, (&'static str, usize)> {
    // The utf-8-sig incremental decoder waits for a third byte while the input is a proper
    // prefix of the BOM; with the whole file read at once, such a file decodes to "".
    if bytes.len() < BOM.len() && BOM.starts_with(bytes) {
        return Ok(String::new());
    }
    let body = bytes.strip_prefix(BOM).unwrap_or(bytes);
    let text = std::str::from_utf8(body).map_err(|e| {
        let start = e.valid_up_to();
        let reason = match e.error_len() {
            None => "unexpected end of data",
            Some(_) if matches!(body[start], 0x80..=0xC1 | 0xF5..=0xFF) => "invalid start byte",
            Some(_) => "invalid continuation byte",
        };
        (reason, start)
    })?;
    Ok(text.replace("\r\n", "\n").replace('\r', "\n"))
}

/// `load_deck(path)` (`deck/parse.py:111-114`): the deck name is the path's stem, the source
/// `str(Path(path))`.
///
/// # Errors
/// [`DeckError::Io`] (`OSError`), [`DeckError::Decode`] (`UnicodeDecodeError`) and the
/// [`DeckError::Invalid`] of [`parse_deck`]; the paths in them are normalised as `pathlib`
/// prints them.
pub fn load_deck(path: &Path) -> Result<Deck, DeckError> {
    let raw = path.to_string_lossy();
    let shown = py::path_str(&raw);
    let bytes = std::fs::read(path).map_err(|error| DeckError::Io {
        path: shown.clone().into(),
        error,
    })?;
    let text = decode_deck_bytes(&bytes).map_err(|(reason, start)| DeckError::Decode {
        path: shown.clone().into(),
        reason,
        start,
    })?;
    parse_deck(&text, &py::path_stem(&raw), Some(&shown))
}

/// `to_decklist(deck)` (`deck/parse.py:117-118`) with the card numbers resolved to def_ids:
/// the run-length lists expanded in order (Python's sorted tuples), the main deck in the
/// order `new_game` maps it (`engine/game.py:134-141`).
///
/// # Errors
/// [`DeckError::TooLarge`] when the deck would expand to more than [`MAX_EXPANDED_CARDS`]
/// cards (W-144); [`DeckError::Invalid`] naming a card number that is not in the card data
/// (Python raises `KeyError` in `new_game`), which a parsed deck never holds.
pub fn to_decklist(deck: &Deck) -> Result<DeckList, DeckError> {
    let main_total: u128 = deck.main.iter().map(|c| c.count).sum();
    let res_total: u128 = deck.resources.iter().map(|c| c.count).sum();
    if main_total.saturating_add(res_total) > MAX_EXPANDED_CARDS {
        return Err(DeckError::TooLarge {
            deck_name: deck.name.clone(),
            main: main_total,
            resources: res_total,
        });
    }
    let db = CardDb::embedded();
    let expand = |cards: &[CardCount]| -> Result<Box<[crate::DefId]>, DeckError> {
        let mut out = Vec::new();
        for c in cards {
            let Some(d) = db.get(&c.card_number) else {
                return Err(DeckError::Invalid {
                    deck_name: Some(deck.name.clone()),
                    problems: vec![format!(
                        "unknown card id {} (not in the card data)",
                        unicode::repr_str(&c.card_number)
                    )],
                    violations: Vec::new(),
                });
            };
            for _ in 0..c.count {
                out.push(d.def_id);
            }
        }
        Ok(out.into())
    };
    Ok(DeckList {
        main: expand(&deck.main)?,
        resources: expand(&deck.resources)?,
    })
}

/// The deck file's display text for an `OSError` (`exc.strerror or exc`): the C library's
/// message, without the ` (os error N)` that `io::Error` appends.
fn strerror(error: &std::io::Error) -> String {
    let text = error.to_string();
    match error.raw_os_error() {
        Some(code) => text
            .strip_suffix(&format!(" (os error {code})"))
            .map_or_else(|| text.clone(), str::to_owned),
        None => text,
    }
}

impl fmt::Display for DeckError {
    /// `Invalid` is `str(DeckError)` (`deck/model.py:55-58`); `Io` and `Decode` are the CLI
    /// texts of `cli.py:102-110`, path included; `TooLarge` is W-144's message.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, error } => write!(
                f,
                "cannot read deck file {}: {}",
                path.display(),
                strerror(error)
            ),
            Self::Decode {
                path,
                reason,
                start,
            } => write!(
                f,
                "{}: not UTF-8 text ({reason} at byte {start})",
                path.display()
            ),
            Self::Invalid {
                deck_name,
                problems,
                ..
            } => {
                match deck_name.as_deref().filter(|n| !n.is_empty()) {
                    Some(n) => write!(f, "deck {}", unicode::repr_str(n))?,
                    None => f.write_str("deck")?,
                }
                write!(f, " has {} problem(s):", problems.len())?;
                for p in problems {
                    write!(f, "\n  - {p}")?;
                }
                Ok(())
            }
            Self::TooLarge {
                deck_name,
                main,
                resources,
            } => write!(
                f,
                "deck {} has {main} cards and {resources} resources; a game can hold at most \
                 {MAX_EXPANDED_CARDS} cards",
                unicode::repr_str(deck_name)
            ),
        }
    }
}

impl std::error::Error for DeckError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { error, .. } => Some(error),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_grammar() {
        let m = match_entry("4x GD01-001 Gundam  (x)").unwrap();
        assert_eq!(
            (m.count, m.id, m.name),
            ("4", "GD01-001", Some("Gundam  (x)"))
        );
        let m = match_entry("\u{FF14} GD01-001\u{1F}Gundam").unwrap();
        assert_eq!(
            (m.count, m.id, m.name),
            ("\u{FF14}", "GD01-001", Some("Gundam"))
        );
        assert!(match_entry("4X").is_none());
        assert!(match_entry("4xx GD01").is_none());
        assert!(match_entry("four GD01").is_none());
        let m = match_entry("4 x GD01").unwrap();
        assert_eq!((m.id, m.name), ("x", Some("GD01")));
    }

    #[test]
    fn alt_art_and_candidates() {
        assert_eq!(alt_art_base("A_p1_p2"), Some("A_p1"));
        assert_eq!(alt_art_base("GD01-001_P9"), Some("GD01-001"));
        assert_eq!(alt_art_base("_p1"), None);
        assert_eq!(alt_art_base("GD01-001_q1"), None);
        let db = CardDb::embedded();
        assert_eq!(resolve_card_number("gd01-001", db), Some("GD01-001"));
        assert_eq!(resolve_card_number("GD01-001_P1", db), Some("GD01-001"));
        assert_eq!(resolve_card_number("GD01-001_p9", db), Some("GD01-001"));
        assert_eq!(resolve_card_number("GD01-001_q1", db), None);
        assert_eq!(resolve_card_number("exb-001_P7", db), Some("EXB-001"));
        assert_eq!(resolve_card_number("R-001_p1", db), Some("R-001"));
        assert_eq!(resolve_card_number("UNIT:GD05-089:3/3", db), None);
        // str.upper() maps the long s to ASCII S.
        assert_eq!(resolve_card_number("\u{17F}t01-001", db), Some("ST01-001"));
    }

    #[test]
    fn huge_counts_are_syntax_violations() {
        let digits = "9".repeat(5_000);
        let err = parse_deck(&format!("{digits} GD01-001\n"), "big", None).unwrap_err();
        let DeckError::Invalid { violations, .. } = &err else {
            panic!("expected Invalid, got {err}")
        };
        assert_eq!(violations[0].code, ViolationCode::Syntax);
        assert_eq!(
            violations[0].message,
            "line 1: count must be at most 18446744073709551615 for GD01-001"
        );
        // A 5,000-digit count is a SYNTAX problem even next to another problem (Python's
        // int() raises there); a 30-digit one only when nothing else stops the parse.
        let err = parse_deck(&format!("{digits} GD01-001\n1 XX-1\n"), "big", None).unwrap_err();
        let DeckError::Invalid { problems, .. } = &err else {
            panic!("expected Invalid, got {err}")
        };
        assert_eq!(problems.len(), 2);
        let err = parse_deck(&format!("{} GD01-001\n", "9".repeat(30)), "big", None).unwrap_err();
        let DeckError::Invalid { violations, .. } = &err else {
            panic!("expected Invalid, got {err}")
        };
        assert_eq!(violations[0].code, ViolationCode::Syntax);
        let err = parse_deck(
            &format!("{} GD01-001\n1 XX-1\n", "9".repeat(30)),
            "big",
            None,
        )
        .unwrap_err();
        let DeckError::Invalid { violations, .. } = &err else {
            panic!("expected Invalid, got {err}")
        };
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].code, ViolationCode::UnknownId);
        let deck = parse_deck("1000000000000 GD01-001\n", "big", None).unwrap();
        assert_eq!(deck.main[0].count, 1_000_000_000_000);
        assert!(matches!(
            to_decklist(&deck),
            Err(DeckError::TooLarge { .. })
        ));
    }

    #[test]
    fn decoding() {
        assert_eq!(
            decode_deck_bytes(b"\xEF\xBB\xBFa\r\nb\rc"),
            Ok("a\nb\nc".to_owned())
        );
        assert_eq!(decode_deck_bytes(b"\xEF\xBB"), Ok(String::new()));
        assert_eq!(
            decode_deck_bytes(b"\xEF\xBB\xBFabcdefghijk\xFF"),
            Err(("invalid start byte", 11))
        );
        assert_eq!(
            decode_deck_bytes(b"a\xE2\x82"),
            Err(("unexpected end of data", 1))
        );
        assert_eq!(
            decode_deck_bytes(b"a\xE2\x82A"),
            Err(("invalid continuation byte", 1))
        );
        assert_eq!(
            decode_deck_bytes(b"\xED\xA0\x80"),
            Err(("invalid continuation byte", 0))
        );
    }
}
