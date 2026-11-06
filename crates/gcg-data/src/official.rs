//! Deck-construction rules and the Banned & Restricted list from the packaged official data
//! (`deck/official.py`), plus the rules and BO3 versions that `versions()` reports
//! (`reports/versions.py:58,66-74`).

use std::sync::OnceLock;

use serde_json::Value;

use crate::contracts::{
    AttributePairRule, Banlist, BannedPair, Bo3Rules, CardDef, ConstructionRules, DataError,
    Restricted, RuleException, RulesVersion,
};
use crate::{embedded, model, py};

const CONSTRUCTION_FILE: &str = "official/deck_construction.json";
pub(crate) const BANLIST_FILE: &str = "official/banlist.json";

pub(crate) fn shape(file: &str, detail: impl Into<String>) -> DataError {
    DataError::Shape {
        file: file.into(),
        detail: detail.into(),
    }
}

pub(crate) fn field<'a>(file: &str, d: &'a Value, path: &[&str]) -> Result<&'a Value, DataError> {
    let mut v = d;
    for key in path {
        v = py::get(v, key).ok_or_else(|| shape(file, format!("missing {}", path.join("."))))?;
    }
    Ok(v)
}

/// `int(v)` as a `u32`.
fn uint(file: &str, d: &Value, path: &[&str]) -> Result<u32, DataError> {
    let v = field(file, d, path)?;
    py::int_of(v)
        .and_then(|i| u32::try_from(i).ok())
        .ok_or_else(|| shape(file, format!("{} is not a count: {v}", path.join("."))))
}

/// `str(v)`.
pub(crate) fn text(file: &str, d: &Value, path: &[&str]) -> Result<Box<str>, DataError> {
    Ok(py::str_of(field(file, d, path)?).into())
}

/// A list whose items are read with `f`.
pub(crate) fn list<'a, T>(
    file: &str,
    v: &'a Value,
    what: &str,
    f: impl FnMut(&'a Value) -> Result<T, DataError>,
) -> Result<Vec<T>, DataError> {
    v.as_array()
        .ok_or_else(|| shape(file, format!("{what} is not a list")))?
        .iter()
        .map(f)
        .collect()
}

/// A `frozenset` of strings, stored sorted and deduplicated (membership only).
fn string_set(file: &str, items: Vec<&Value>, what: &str) -> Result<Box<[Box<str>]>, DataError> {
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let s = item
            .as_str()
            .ok_or_else(|| shape(file, format!("{what} holds a non-string: {item}")))?;
        out.push(Box::<str>::from(s));
    }
    out.sort();
    out.dedup();
    Ok(out.into())
}

/// `construction_rules()` from the text of `deck_construction.json` (`deck/official.py:74-86`).
pub(crate) fn parse_construction_rules(text_json: &str) -> Result<ConstructionRules, DataError> {
    let f = CONSTRUCTION_FILE;
    let d: Value = serde_json::from_str(text_json).map_err(|e| DataError::Json {
        file: f.into(),
        line: 0,
        message: e.to_string(),
    })?;
    let types = |path: &[&str]| -> Result<Box<[Box<str>]>, DataError> {
        let items = list(f, field(f, &d, path)?, &path.join("."), Ok)?;
        string_set(f, items, &path.join("."))
    };
    Ok(ConstructionRules {
        main_size: uint(f, &d, &["main_deck", "size"])?,
        resource_size: uint(f, &d, &["resource_deck", "size"])?,
        max_copies: uint(f, &d, &["main_deck", "max_copies_per_card_number"])?,
        main_types: types(&["main_deck", "allowed_card_types"])?,
        resource_types: types(&["resource_deck", "allowed_card_types"])?,
        min_colors: uint(f, &d, &["colors", "min_distinct_colors"])?,
        max_colors: uint(f, &d, &["colors", "max_distinct_colors"])?,
        effective_date: text(f, &d, &["effective_date"])?,
    })
}

/// `_exceptions(raw)` (`deck/official.py:89-90`).
fn exceptions(v: &Value) -> Result<Box<[RuleException]>, DataError> {
    let f = BANLIST_FILE;
    Ok(list(f, v, "exceptions", |e| {
        Ok(RuleException {
            id: text(f, e, &["id"])?,
            description: text(f, e, &["description"])?,
        })
    })?
    .into())
}

const LEAVES: [&str; 10] = [
    "card_type",
    "level",
    "cost",
    "ap",
    "hp",
    "has_effect",
    "color",
    "trait",
    "name_contains",
    "card_number",
];

/// Checks a `member_predicate` once at load, so [`matches`] cannot fail: every key is an
/// operator of `predicate_language` (`deck/official.py:135-173`), `all_of`/`any_of` hold lists
/// of predicates and `not` holds one. Python raises the same `ValueError` lazily, only if the
/// evaluation reaches the key.
fn check_predicate(rule_id: &str, pred: &Value) -> Result<(), DataError> {
    let Some(map) = pred.as_object() else {
        return Err(shape(
            BANLIST_FILE,
            format!("{rule_id}: a predicate is not an object"),
        ));
    };
    for (key, value) in map {
        match key.as_str() {
            "all_of" | "any_of" => {
                let items = value.as_array().ok_or_else(|| {
                    shape(BANLIST_FILE, format!("{rule_id}: {key} is not a list"))
                })?;
                for p in items {
                    check_predicate(rule_id, p)?;
                }
            }
            "not" => check_predicate(rule_id, value)?,
            k if LEAVES.contains(&k) => {}
            _ => {
                return Err(DataError::UnknownPredicateOperator {
                    rule_id: rule_id.into(),
                    key: key.as_str().into(),
                });
            }
        }
    }
    Ok(())
}

/// `_attribute_rule(r)` (`deck/official.py:101-109`).
fn attribute_rule(r: &Value) -> Result<AttributePairRule, DataError> {
    let f = BANLIST_FILE;
    let id = text(f, r, &["id"])?;
    let description: Box<str> = {
        let source = py::get(r, "source_text").map_or_else(String::new, py::str_of);
        model::first_quoted(&source).map_or_else(|| id.clone(), Into::into)
    };
    let member_predicate = field(f, r, &["member_predicate"])?.clone();
    check_predicate(&id, &member_predicate)?;
    let members = list(
        f,
        field(f, r, &["enumerated_members"])?,
        "enumerated_members",
        |m| field(f, m, &["card_number"]),
    )?;
    let exceptions = match py::get(r, "exceptions") {
        Some(v) => exceptions(v)?,
        None => Box::new([]),
    };
    Ok(AttributePairRule {
        description,
        member_predicate,
        enumerated: string_set(f, members, "enumerated_members")?,
        max_total_copies: uint(f, r, &["max_total_copies_of_matching_cards"])?,
        exceptions,
        id,
    })
}

/// `banlist()` from the text of `banlist.json` (`deck/official.py:112-127`); lists keep file
/// order.
pub(crate) fn parse_banlist(text_json: &str) -> Result<Banlist, DataError> {
    parse_banlist_value(&banlist_document(text_json)?)
}

/// `json.loads` of `banlist.json`'s text.
pub(crate) fn banlist_document(text_json: &str) -> Result<Value, DataError> {
    serde_json::from_str(text_json).map_err(|e| DataError::Json {
        file: BANLIST_FILE.into(),
        line: 0,
        message: e.to_string(),
    })
}

/// `banlist()`'s body (`deck/official.py:112-127`) over one list object: the top-level
/// document, or one dated list of its optional `history` (W-72, [`crate::legality`]).
pub(crate) fn parse_banlist_value(d: &Value) -> Result<Banlist, DataError> {
    let f = BANLIST_FILE;
    let banned = list(f, field(f, d, &["banned"])?, "banned", |b| {
        field(f, b, &["card_number"])
    })?;
    let restricted = list(f, field(f, d, &["restricted"])?, "restricted", |r| {
        Ok(Restricted {
            card_number: text(f, r, &["card_number"])?,
            max_copies: uint(f, r, &["max_copies"])?,
            exceptions: exceptions(field(f, r, &["exceptions"])?)?,
        })
    })?;
    let banned_pairs = list(f, field(f, d, &["banned_pairs"])?, "banned_pairs", |p| {
        let cards = field(f, p, &["cards"])?
            .as_array()
            .filter(|c| c.len() >= 2)
            .ok_or_else(|| shape(f, "a banned pair does not list two cards"))?;
        Ok(BannedPair {
            id: text(f, p, &["id"])?,
            cards: [py::str_of(&cards[0]).into(), py::str_of(&cards[1]).into()],
        })
    })?;
    let attribute_rules = list(
        f,
        field(f, d, &["attribute_pair_rules"])?,
        "attribute_pair_rules",
        attribute_rule,
    )?;
    Ok(Banlist {
        effective_date: text(f, d, &["effective_date"])?,
        banned: string_set(f, banned, "banned")?,
        restricted: restricted.into(),
        banned_pairs: banned_pairs.into(),
        attribute_rules: attribute_rules.into(),
    })
}

/// `construction_rules()` (`deck/official.py:74-86`, `@cache`).
///
/// # Panics
/// If the embedded file does not load (a build defect; `cargo test -p gcg-data` loads it).
#[must_use]
pub fn construction_rules() -> &'static ConstructionRules {
    static RULES: OnceLock<ConstructionRules> = OnceLock::new();
    RULES.get_or_init(|| {
        parse_construction_rules(embedded::DECK_CONSTRUCTION_JSON)
            .unwrap_or_else(|e| panic!("embedded deck rules do not load: {e}"))
    })
}

/// `banlist()` (`deck/official.py:112-127`, `@cache`); every predicate operator is checked.
///
/// # Panics
/// If the embedded file does not load (a build defect; `cargo test -p gcg-data` loads it).
#[must_use]
pub fn banlist() -> &'static Banlist {
    static BANLIST: OnceLock<Banlist> = OnceLock::new();
    BANLIST.get_or_init(|| {
        parse_banlist(embedded::BANLIST_JSON)
            .unwrap_or_else(|e| panic!("embedded banlist does not load: {e}"))
    })
}

/// `_leaf(key, value, card, matched)` (`deck/official.py:135-157`) with Python's `==` between
/// the card's field and the JSON value.
fn leaf(key: &str, value: &Value, card: &CardDef, matched: Option<&str>) -> bool {
    match key {
        "card_type" => value.as_str() == Some(card.card_type.as_str()),
        "level" => py::int_eq(i64::from(card.level), value),
        "cost" => py::int_eq(i64::from(card.cost), value),
        "ap" => py::int_eq(i64::from(card.ap), value),
        "hp" => py::int_eq(i64::from(card.hp), value),
        "has_effect" => py::int_eq(i64::from(!card.is_vanilla()), value),
        "color" => card
            .color
            .is_some_and(|c| value.as_str() == Some(c.as_str())),
        "trait" => value
            .as_str()
            .is_some_and(|v| card.traits.iter().any(|t| &**t == v)),
        "name_contains" => card.name.contains(py::str_of(value).as_str()),
        "card_number" => {
            let wanted = if value.as_str() == Some("$matched") {
                matched
            } else {
                value.as_str()
            };
            wanted == Some(&*card.card_number)
        }
        _ => panic!(
            "unknown banlist predicate operator {key:?}: the predicate was not loaded by banlist()"
        ),
    }
}

/// `matches(pred, card, matched=None)` (`deck/official.py:160-173`): the keys in JSON order,
/// stopping at the first false one.
///
/// # Panics
/// On an operator outside `predicate_language`. [`banlist`] rejects those when it loads, so a
/// predicate taken from it never panics.
#[must_use]
pub fn matches(pred: &Value, card: &CardDef, matched: Option<&str>) -> bool {
    let Some(map) = pred.as_object() else {
        panic!("a banlist predicate is not an object: {pred}");
    };
    for (key, value) in map {
        let ok = match key.as_str() {
            "all_of" => value
                .as_array()
                .is_some_and(|ps| ps.iter().all(|p| matches(p, card, matched))),
            "any_of" => value
                .as_array()
                .is_some_and(|ps| ps.iter().any(|p| matches(p, card, matched))),
            "not" => !matches(value, card, matched),
            _ => leaf(key, value, card, matched),
        };
        if !ok {
            return false;
        }
    }
    true
}

/// The `latest_version` and `effective_date` of `rules_version.json` as strings
/// (`reports/versions.py:58,66-67`).
///
/// # Panics
/// If the embedded file lacks either string (a build defect).
#[must_use]
pub fn rules_version() -> &'static RulesVersion {
    static RULES: OnceLock<RulesVersion> = OnceLock::new();
    RULES.get_or_init(|| {
        let d = embedded::json("rules_version.json", embedded::RULES_VERSION_JSON);
        let get = |k: &str| -> Box<str> {
            py::get(&d, k)
                .and_then(Value::as_str)
                .unwrap_or_else(|| panic!("rules_version.json has no string {k:?}"))
                .into()
        };
        RulesVersion {
            latest_version: get("latest_version"),
            effective_date: get("effective_date"),
        }
    })
}

/// The `effective_date` of `bo3_match_rules.json` (`reports/versions.py:72-74`).
///
/// # Panics
/// If the embedded file lacks it (a build defect).
#[must_use]
pub fn bo3_rules() -> &'static Bo3Rules {
    static BO3: OnceLock<Bo3Rules> = OnceLock::new();
    BO3.get_or_init(|| {
        let d = embedded::json("bo3_match_rules.json", embedded::BO3_MATCH_RULES_JSON);
        Bo3Rules {
            effective_date: py::get(&d, "effective_date")
                .and_then(Value::as_str)
                .unwrap_or_else(|| panic!("bo3_match_rules.json has no string effective_date"))
                .into(),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packaged_official_data() {
        let r = construction_rules();
        assert_eq!((r.main_size, r.resource_size, r.max_copies), (50, 10, 4));
        assert_eq!((r.min_colors, r.max_colors), (1, 2));
        assert_eq!(&*r.effective_date, "2026-09-11");
        assert_eq!(
            r.main_types.iter().map(|t| &**t).collect::<Vec<_>>(),
            ["BASE", "COMMAND", "PILOT", "UNIT"]
        );
        let b = banlist();
        assert_eq!(&*b.effective_date, "2026-09-25");
        assert_eq!(&*b.banned, [Box::from("GD01-020")]);
        assert_eq!(&*b.restricted[0].card_number, "ST02-016");
        assert_eq!(b.restricted[0].max_copies, 2);
        assert_eq!(
            b.banned_pairs
                .iter()
                .map(|p| [&*p.cards[0], &*p.cards[1]])
                .collect::<Vec<_>>(),
            [["ST01-010", "ST05-010"], ["GD01-008", "GD05-015"]]
        );
        let rule = &b.attribute_rules[0];
        assert_eq!(&*rule.id, "vanilla-lv2-cost1-ap2-hp2");
        assert_eq!(
            &*rule.description,
            "a Unit card that is Lv.2 with cost 1, 2 AP, and 2 HP, and without effects"
        );
        assert_eq!(rule.enumerated.len(), 22);
        assert_eq!(rule.max_total_copies, 4);
        assert_eq!(&*rules_version().latest_version, "1.9.0");
        assert_eq!(&*bo3_rules().effective_date, "2026-07-03");
    }

    #[test]
    fn unknown_operators_are_rejected_at_load() {
        let text = embedded::BANLIST_JSON.replace("\"has_effect\"", "\"has_effects\"");
        match parse_banlist(&text) {
            Err(DataError::UnknownPredicateOperator { rule_id, key }) => {
                assert_eq!(&*rule_id, "vanilla-lv2-cost1-ap2-hp2");
                assert_eq!(&*key, "has_effects");
            }
            other => panic!("expected an unknown operator, got {:?}", other.err()),
        }
    }
}
