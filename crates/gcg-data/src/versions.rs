//! Versions of everything a result depends on, the conflict resolutions touching a deck
//! (`reports/versions.py`), and the data parts of `gcg-sim data status` (`cli.py:251-291`).

use std::collections::BTreeMap;
use std::sync::OnceLock;

use gcg_core::pyjson::Json;
use serde_json::Value;

use crate::contracts::{CardDb, DataStatusParts, Deck, RESULTS_SCHEMA_VERSION};
use crate::{embedded, official, py};

/// The parsed embedded JSON documents `versions()` and `data status` read.
struct Documents {
    manifest: Value,
    rules: Value,
    banlist: Value,
    construction: Value,
    bo3: Value,
    overrides: Value,
    curated: Value,
}

fn documents() -> &'static Documents {
    static DOCS: OnceLock<Documents> = OnceLock::new();
    DOCS.get_or_init(|| Documents {
        manifest: embedded::json("gcgapi/manifest.json", embedded::MANIFEST_JSON),
        rules: embedded::json("official/rules_version.json", embedded::RULES_VERSION_JSON),
        banlist: embedded::json("official/banlist.json", embedded::BANLIST_JSON),
        construction: embedded::json(
            "official/deck_construction.json",
            embedded::DECK_CONSTRUCTION_JSON,
        ),
        bo3: embedded::json(
            "official/bo3_match_rules.json",
            embedded::BO3_MATCH_RULES_JSON,
        ),
        overrides: embedded::json("overrides.json", embedded::OVERRIDES_JSON),
        curated: embedded::json("curated_conflicts.json", embedded::CURATED_CONFLICTS_JSON),
    })
}

/// `doc[key]` as JSON; Python raises `KeyError` on a missing key, a build defect for the
/// embedded files.
fn item(doc: &Value, name: &str, key: &str) -> Json {
    py::get(doc, key).map_or_else(|| panic!("embedded {name} has no {key:?}"), py::to_json)
}

/// `versions()` (`reports/versions.py:55-75`), with the two values Python computes from its
/// installation (`package_version()`, `code_digest()`) passed in. Keys in insertion order;
/// the data values are the JSON values of the files, unchanged.
#[must_use]
pub fn versions(package: &str, code_digest: &str) -> Json {
    let d = documents();
    let mut v = Json::object();
    v.set("package", package);
    v.set("code_digest", code_digest);
    v.set("results_schema", RESULTS_SCHEMA_VERSION);
    v.set(
        "dataset_version",
        item(&d.manifest, "manifest.json", "dataset_version"),
    );
    v.set(
        "data_source_commit",
        item(&d.manifest, "manifest.json", "source_commit"),
    );
    v.set(
        "data_built_at",
        item(&d.manifest, "manifest.json", "built_at"),
    );
    v.set(
        "rules_version",
        item(&d.rules, "rules_version.json", "latest_version"),
    );
    v.set(
        "rules_effective_date",
        item(&d.rules, "rules_version.json", "effective_date"),
    );
    v.set(
        "banlist_effective_date",
        item(&d.banlist, "banlist.json", "effective_date"),
    );
    v.set(
        "deck_rules_effective_date",
        item(&d.construction, "deck_construction.json", "effective_date"),
    );
    v.set(
        "bo3_rules_effective_date",
        item(&d.bo3, "bo3_match_rules.json", "effective_date"),
    );
    v
}

/// The `resolutions` object of `overrides.json` (`load_overrides().get("resolutions", {})`).
fn resolutions() -> Option<&'static serde_json::Map<String, Value>> {
    py::get(&documents().overrides, "resolutions").and_then(Value::as_object)
}

/// The `conflicts` list of `curated_conflicts.json` (`.get("conflicts", [])`).
fn curated_list() -> &'static [Value] {
    py::get(&documents().curated, "conflicts")
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

/// `str` items of a `json.loads` iterable as `set(...)` would hold them: list items, the
/// characters of a string, the keys of an object.
fn iter_strs(v: &Value) -> Vec<String> {
    match v {
        Value::Array(items) => items
            .iter()
            .filter_map(|x| x.as_str().map(str::to_owned))
            .collect(),
        Value::String(s) => s.chars().map(String::from).collect(),
        Value::Object(map) => map.keys().cloned().collect(),
        _ => Vec::new(),
    }
}

/// `_curated_cards()` (`reports/versions.py:82-90`): card numbers per curated conflict id.
fn curated_cards() -> &'static BTreeMap<String, Vec<String>> {
    static CURATED: OnceLock<BTreeMap<String, Vec<String>>> = OnceLock::new();
    CURATED.get_or_init(|| {
        let mut out = BTreeMap::new();
        for c in curated_list() {
            let Some(id) = py::get_truthy(c, "id") else {
                continue;
            };
            let numbers = py::get_truthy(c, "card_numbers").map_or_else(Vec::new, |v| match v {
                Value::Array(items) => items.iter().map(py::str_of).collect(),
                other => iter_strs(other),
            });
            out.insert(py::str_of(id), numbers);
        }
        out
    })
}

/// `_resolution_cards(cid, res)` (`reports/versions.py:93-99`).
fn resolution_cards(cid: &str, res: &Value) -> Vec<String> {
    let mut numbers: Vec<String> =
        py::get_truthy(res, "card_numbers").map_or_else(Vec::new, iter_strs);
    if let Some(n) = py::get_truthy(res, "card_number") {
        numbers.push(py::str_of(n));
    }
    numbers.extend(cid.split(':').skip(1).map(str::to_owned));
    if let Some(curated) = curated_cards().get(cid) {
        numbers.extend(curated.iter().cloned());
    }
    numbers
}

/// `_entry(number, cid, decks, decision, fields, changes)` (`reports/versions.py:102-113`).
fn entry(
    number: &str,
    cid: &str,
    decks: &[&str],
    decision: &str,
    fields: Vec<Json>,
    changes: bool,
) -> Json {
    let name = CardDb::embedded()
        .get(number)
        .map(|d| d.name.to_string())
        .unwrap_or_else(|| panic!("conflict_resolutions: {number:?} is not in the card data"));
    let mut sorted_decks: Vec<&str> = decks.to_vec();
    sorted_decks.sort();
    let mut e = Json::object();
    e.set("card_number", number);
    e.set("name", name);
    e.set("conflict_id", cid);
    e.set(
        "decks",
        Json::Array(sorted_decks.into_iter().map(Json::from).collect()),
    );
    e.set("decision", decision);
    e.set("fields", Json::Array(fields));
    e.set("changes_card_data", changes);
    e
}

/// `conflict_resolutions(decks)` (`reports/versions.py:116-135`) for `decks` given as `(label,
/// deck)` pairs in dict order; a deck contributes `main + resources`, as `reports/results.py:
/// 181-186` passes it. Entries are sorted by `(card_number, conflict_id)`; the card-data
/// overrides applied at load time win over the resolutions that only name a card.
///
/// # Panics
/// If a deck holds a card number that is not in the card data (Python raises `KeyError`); a
/// parsed deck never does.
#[must_use]
pub fn conflict_resolutions(decks: &[(&str, &Deck)]) -> Json {
    // A Python dict: a repeated label keeps its first position and takes the last value.
    let mut labelled: Vec<(&str, &Deck)> = Vec::new();
    for &(label, deck) in decks {
        match labelled.iter_mut().find(|(l, _)| *l == label) {
            Some(slot) => slot.1 = deck,
            None => labelled.push((label, deck)),
        }
    }
    let mut in_decks: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (label, deck) in &labelled {
        let mut numbers: Vec<&str> = deck
            .main
            .iter()
            .chain(deck.resources.iter())
            .map(|c| &*c.card_number)
            .collect();
        numbers.sort();
        numbers.dedup();
        for n in numbers {
            in_decks.entry(n).or_default().push(label);
        }
    }
    let mut out: BTreeMap<(String, String), Json> = BTreeMap::new();
    for o in CardDb::embedded().applied_overrides() {
        if let Some(labels) = in_decks.get(&*o.card_number) {
            let fields = o.fields.iter().map(|f| Json::from(&**f)).collect();
            out.insert(
                (o.card_number.to_string(), o.conflict_id.to_string()),
                entry(
                    &o.card_number,
                    &o.conflict_id,
                    labels,
                    &o.decision,
                    fields,
                    true,
                ),
            );
        }
    }
    if let Some(resolutions) = resolutions() {
        let mut ids: Vec<&String> = resolutions.keys().collect();
        ids.sort();
        for cid in ids {
            let res = &resolutions[cid.as_str()];
            if !res.is_object() {
                continue;
            }
            let mut cards: Vec<String> = resolution_cards(cid, res)
                .into_iter()
                .filter(|n| in_decks.contains_key(n.as_str()))
                .collect();
            cards.sort();
            cards.dedup();
            let decision = py::get(res, "decision").map_or_else(String::new, py::str_of);
            for n in cards {
                let labels = &in_decks[n.as_str()];
                out.entry((n.clone(), cid.clone()))
                    .or_insert_with(|| entry(&n, cid, labels, &decision, Vec::new(), false));
            }
        }
    }
    Json::Array(out.into_values().collect())
}

/// `data_status()` (`cli.py:263-291`) without `coverage`, which reads the effect registry: the
/// `versions`, `snapshot`, `banlist` and `conflicts` parts, each as Python builds it.
#[must_use]
pub fn data_status_parts(package: &str, code_digest: &str) -> DataStatusParts {
    let d = documents();
    let mut snapshot = Json::object();
    for key in [
        "dataset_version",
        "source_commit",
        "built_at",
        "card_count",
        "set_count",
        "ruling_count",
        "rules_faq_count",
        "errata_count",
        "product_count",
    ] {
        snapshot.set(
            key,
            py::get(&d.manifest, key).map_or(Json::Null, py::to_json),
        );
    }
    let bl = official::banlist();
    let mut banlist = Json::object();
    banlist.set("effective_date", &*bl.effective_date);
    banlist.set(
        "banned",
        Json::Array(bl.banned.iter().map(|n| Json::from(&**n)).collect()),
    );
    let mut restricted = Json::object();
    for r in bl.restricted.iter() {
        restricted.set(&*r.card_number, r.max_copies);
    }
    banlist.set("restricted", restricted);
    banlist.set(
        "banned_pairs",
        Json::Array(
            bl.banned_pairs
                .iter()
                .map(|p| Json::Array(p.cards.iter().map(|c| Json::from(&**c)).collect()))
                .collect(),
        ),
    );
    banlist.set(
        "attribute_pair_rules",
        Json::Array(
            bl.attribute_rules
                .iter()
                .map(|r| Json::from(&*r.id))
                .collect(),
        ),
    );
    let applied = CardDb::embedded().applied_overrides();
    let mut overridden: Vec<&str> = applied.iter().map(|o| &*o.card_number).collect();
    overridden.sort();
    overridden.dedup();
    let mut conflicts = Json::object();
    conflicts.set("resolutions", resolutions().map_or(0, serde_json::Map::len));
    conflicts.set("card_data_overrides", applied.len());
    conflicts.set("card_numbers_overridden", overridden.len());
    conflicts.set("curated_conflicts", curated_list().len());
    DataStatusParts {
        versions: versions(package, code_digest),
        snapshot,
        banlist,
        conflicts,
    }
}
