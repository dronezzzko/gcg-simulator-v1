//! Helpers shared by the gcg-data fixture tests. The fixtures under `tests/vectors/` are
//! written by `port/oracle/export_data.py` and `port/oracle/export_unicode.py` from the Python
//! reference; `--check` keeps them current.

#![allow(
    dead_code,
    reason = "each test crate uses a different subset of the helpers"
)]

use gcg_core::pyjson::{Json, Key};
use gcg_data::{CardCount, CardDef, Deck, DeckEntry, DefId, ImplementationStatus, Violation};
use serde_json::{Value, json};

/// A fixture file of `tests/vectors/`.
pub fn fixture(name: &str) -> Value {
    let path = format!("{}/tests/vectors/{name}", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{path}: {e} (run port/oracle/export_data.py)"));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{path}: {e}"))
}

pub fn s(v: &Value) -> &str {
    v.as_str()
        .unwrap_or_else(|| panic!("expected a string, got {v}"))
}

pub fn opt_s(v: &Value) -> Option<&str> {
    if v.is_null() { None } else { Some(s(v)) }
}

pub fn arr(v: &Value) -> &[Value] {
    v.as_array()
        .unwrap_or_else(|| panic!("expected a list, got {v}"))
}

pub fn u(v: &Value) -> u64 {
    v.as_u64()
        .unwrap_or_else(|| panic!("expected an unsigned integer, got {v}"))
}

/// `[def_id, card_number, ..., source_title, extra_names, names, is_token, is_unit_card,
/// is_pilot_capable, is_vanilla]`: the exporter's `def_row`.
pub fn def_row(d: &CardDef) -> Value {
    let strs = |xs: &[Box<str>]| -> Value { xs.iter().map(|x| Value::from(&**x)).collect() };
    let link = d.link.as_ref().map_or(Value::Null, |l| {
        json!([strs(&l.name_parts), strs(&l.traits)])
    });
    json!([
        d.def_id.0,
        &*d.card_number,
        &*d.name,
        d.card_type.as_str(),
        d.color.map(|c| c.as_str()),
        d.level,
        d.cost,
        d.ap,
        d.hp,
        strs(&d.zones),
        strs(&d.traits),
        link,
        &*d.effect,
        d.pilot_name.as_deref(),
        &*d.set_code,
        &*d.rarity,
        strs(&d.product_ids),
        &*d.canonical_product_id,
        d.source_title.as_deref(),
        strs(&d.extra_names),
        strs(d.names()),
        d.is_token(),
        d.is_unit_card(),
        d.is_pilot_capable(),
        d.is_vanilla(),
    ])
}

pub const DEF_FIELDS: [&str; 25] = [
    "def_id",
    "card_number",
    "name",
    "card_type",
    "color",
    "level",
    "cost",
    "ap",
    "hp",
    "zones",
    "traits",
    "link",
    "effect",
    "pilot_name",
    "set_code",
    "rarity",
    "product_ids",
    "canonical_product_id",
    "source_title",
    "extra_names",
    "names",
    "is_token",
    "is_unit_card",
    "is_pilot_capable",
    "is_vanilla",
];

/// Compares every definition field by field; returns the mismatches.
pub fn def_mismatches(defs: &[CardDef], expected: &[Value]) -> Vec<String> {
    let mut out = Vec::new();
    if defs.len() != expected.len() {
        out.push(format!(
            "{} defs, Python has {}",
            defs.len(),
            expected.len()
        ));
    }
    for (d, want) in defs.iter().zip(expected) {
        let got = def_row(d);
        for (i, field) in DEF_FIELDS.iter().enumerate() {
            if got[i] != want[i] {
                out.push(format!(
                    "def {} {field}: {} != Python {}",
                    d.def_id.0, got[i], want[i]
                ));
            }
        }
    }
    out
}

/// `[[code, message, cards]]`.
pub fn violation_rows(vs: &[Violation]) -> Value {
    vs.iter()
        .map(|v| {
            let cards: Vec<&str> = v.cards.iter().map(|c| &**c).collect();
            json!([v.code.as_str(), v.message, cards])
        })
        .collect()
}

/// `sorted(Counter(cards).items())` as run-length entries.
pub fn card_counts(v: &Value) -> Box<[CardCount]> {
    arr(v)
        .iter()
        .map(|pair| CardCount {
            card_number: s(&pair[0]).into(),
            count: u128::from(u(&pair[1])),
        })
        .collect()
}

pub fn counts_json(cards: &[CardCount]) -> Value {
    cards
        .iter()
        .map(|c| {
            json!([
                &*c.card_number,
                u64::try_from(c.count).expect("count fits u64")
            ])
        })
        .collect()
}

pub fn entries(v: &Value) -> Box<[DeckEntry]> {
    arr(v)
        .iter()
        .map(|e| DeckEntry {
            count: u(&e[0]),
            card_number: s(&e[1]).into(),
            written: s(&e[2]).into(),
            name: opt_s(&e[3]).map(Into::into),
            line: usize::try_from(u(&e[4])).expect("line fits usize"),
        })
        .collect()
}

pub fn entries_json(entries: &[DeckEntry]) -> Value {
    entries
        .iter()
        .map(|e| {
            json!([
                e.count,
                &*e.card_number,
                &*e.written,
                e.name.as_deref(),
                e.line
            ])
        })
        .collect()
}

/// A deck built field by field from the exporter's outline (Python's `Deck(...)`).
pub fn deck_from(outline: &Value) -> Deck {
    Deck {
        name: s(&outline["name"]).into(),
        main: card_counts(&outline["main"]),
        resources: card_counts(&outline["resources"]),
        entries: entries(&outline["entries"]),
        source: outline
            .get("source")
            .and_then(Value::as_str)
            .map(Into::into),
        digest: s(&outline["digest"]).to_owned(),
    }
}

/// `registry.cards[def_id].script is None` for the listed def_ids, with their errors.
pub struct Unimplemented(pub Vec<(u16, String)>);

impl Unimplemented {
    pub fn from_case(case: &Value) -> Self {
        let rows = case.get("unimplemented").map_or(&[][..], arr);
        Self(
            rows.iter()
                .map(|r| {
                    let id = u16::try_from(u(&r[0])).expect("def_id fits u16");
                    (id, s(&r[1]).to_owned())
                })
                .collect(),
        )
    }
}

impl ImplementationStatus for Unimplemented {
    fn unimplemented_error(&self, def: DefId) -> Option<&str> {
        self.0
            .iter()
            .find(|(id, _)| *id == def.0)
            .map(|(_, e)| e.as_str())
    }
}

/// A `json.loads` value as the reports' [`Json`].
pub fn to_json(v: &Value) -> Json {
    match v {
        Value::Null => Json::Null,
        Value::Bool(b) => Json::Bool(*b),
        Value::Number(n) => n.as_i64().map_or_else(
            || Json::Float(n.as_f64().expect("finite number")),
            |i| Json::Int(i128::from(i)),
        ),
        Value::String(x) => Json::Str(x.clone()),
        Value::Array(items) => Json::Array(items.iter().map(to_json).collect()),
        Value::Object(map) => Json::Object(
            map.iter()
                .map(|(k, x)| (Key::Str(k.clone()), to_json(x)))
                .collect(),
        ),
    }
}
