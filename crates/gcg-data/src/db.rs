//! `CardDB` (`cards/db.py`): printings grouped by card number, overrides applied, inline tokens
//! and Unit variants registered, in Python's def_id order.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::OnceLock;

use gcg_core::lookup::LookupMap;
use gcg_core::pyjson::{DumpOptions, Json, dumps};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::contracts::{
    AppliedOverride, CardDb, CardDef, CardType, Color, DataError, DefId, EX_BASE_NUMBER,
    EX_RESOURCE_NUMBER, UNIT_VARIANT_PREFIX,
};
use crate::model::{self, TokenSpec};
use crate::{embedded, py, unicode};

const CARDS_FILE: &str = "cards.ndjson";
const OVERRIDES_FILE: &str = "overrides.json";
/// `real_cards` excludes these prefixes (`cards/db.py:309-313`).
const TOKEN_PREFIX: &str = "TOKEN:";

/// The loader paths that the packaged data never takes (W-164). A data refresh that takes one
/// is reviewed: the tripwire test fails on the packaged data when any flag is set.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct DeadPaths {
    /// An inline token definition with no matching token card (`cards/db.py:220-241`).
    pub(crate) synthetic_token: bool,
    /// Two or more token cards matching one definition (`cards/db.py:218`).
    pub(crate) token_tie: bool,
    /// The nested per-number `field_overrides` form (`cards/db.py:136-137`).
    pub(crate) nested_field_overrides: bool,
    /// The singular `card_number` resolution key (`cards/db.py:90-92`).
    pub(crate) singular_card_number: bool,
}

fn shape(file: &str, detail: impl Into<String>) -> DataError {
    DataError::Shape {
        file: file.into(),
        detail: detail.into(),
    }
}

/// `p[key]` where Python raises `KeyError`/`TypeError`.
fn require<'a>(file: &str, rec: &'a Value, key: &str) -> Result<&'a Value, DataError> {
    py::get(rec, key).ok_or_else(|| shape(file, format!("record without {key:?}: {rec}")))
}

fn require_str<'a>(file: &str, rec: &'a Value, key: &str) -> Result<&'a str, DataError> {
    require(file, rec, key)?
        .as_str()
        .ok_or_else(|| shape(file, format!("{key:?} is not a string: {rec}")))
}

/// `_to_int(v)` (`cards/db.py:112-113`): `int(v) if isinstance(v, int) else 0`.
fn to_int(number: &str, v: Option<&Value>) -> Result<i32, DataError> {
    match v.and_then(py::as_py_int) {
        None => Ok(0),
        Some(i) => i32::try_from(i)
            .map_err(|_| shape(CARDS_FILE, format!("{number}: integer {i} out of range"))),
    }
}

/// A string field that Python reads with `.strip()` (`None` stays `None`).
fn opt_str<'a>(
    number: &str,
    v: Option<&'a Value>,
    key: &str,
) -> Result<Option<&'a str>, DataError> {
    match v {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s)),
        Some(other) => Err(shape(
            CARDS_FILE,
            format!("{number}: {key:?} is not a string: {other}"),
        )),
    }
}

/// `tuple(rec.get("traits") or ())`: a list of strings.
fn traits_of(number: &str, rec: &Value) -> Result<Box<[Box<str>]>, DataError> {
    match py::get_truthy(rec, "traits") {
        None => Ok(Box::new([])),
        Some(Value::Array(items)) => items
            .iter()
            .map(|t| {
                t.as_str().map(Into::into).ok_or_else(|| {
                    shape(CARDS_FILE, format!("{number}: trait {t} is not a string"))
                })
            })
            .collect(),
        Some(other) => Err(shape(
            CARDS_FILE,
            format!("{number}: \"traits\" is not a list: {other}"),
        )),
    }
}

/// `CardDB._make_def(def_id, rec, product_ids)` (`cards/db.py:160-197`); `number` is the
/// grouping key, used in error messages.
fn make_def(
    def_id: DefId,
    number: &str,
    rec: &Value,
    product_ids: Box<[Box<str>]>,
) -> Result<CardDef, DataError> {
    let type_value = require(CARDS_FILE, rec, "card_type")?;
    let card_type = type_value
        .as_str()
        .and_then(CardType::from_value)
        .ok_or_else(|| DataError::UnknownCardType {
            card_number: number.into(),
            value: py::str_of(type_value).into(),
        })?;
    let color =
        match py::get_truthy(rec, "color") {
            None => None,
            Some(v) => Some(v.as_str().and_then(Color::from_value).ok_or_else(|| {
                DataError::UnknownColor {
                    card_number: number.into(),
                    value: py::str_of(v).into(),
                }
            })?),
        };
    let effect: Box<str> =
        py::get_truthy(rec, "effect").map_or_else(|| "-".into(), |v| py::str_of(v).into());
    let name: Box<str> = py::str_of(require(CARDS_FILE, rec, "name")?).into();
    let pilot_name: Option<Box<str>> = match card_type {
        CardType::Command => model::parse_command_pilot_name(&effect).map(Into::into),
        CardType::Pilot => Some(name.clone()),
        _ => None,
    };
    let modifiers = card_type == CardType::Pilot
        || (card_type == CardType::Command && pilot_name.as_deref().is_some_and(|p| !p.is_empty()));
    let (ap, hp) = if modifiers {
        let bad = |raw: Option<&str>| DataError::BadModifier {
            card_number: number.into(),
            raw: raw.unwrap_or("").into(),
        };
        let ap_raw = opt_str(number, py::get(rec, "ap_raw"), "ap_raw")?;
        let hp_raw = opt_str(number, py::get(rec, "hp_raw"), "hp_raw")?;
        (
            model::parse_modifier(ap_raw).map_err(|()| bad(ap_raw))?,
            model::parse_modifier(hp_raw).map_err(|()| bad(hp_raw))?,
        )
    } else {
        (
            to_int(number, py::get(rec, "ap"))?,
            to_int(number, py::get(rec, "hp"))?,
        )
    };
    let zone_raw = py::get_truthy(rec, "zone").map_or_else(|| "-".to_owned(), py::str_of);
    let zones: Box<[Box<str>]> = unicode::split_whitespace(&zone_raw)
        .into_iter()
        .filter(|z| *z != "-")
        .map(Into::into)
        .collect();
    let link = if model::has_link(card_type) {
        let raw = opt_str(number, py::get(rec, "link"), "link")?;
        model::parse_link(raw).map_err(|()| DataError::UnparseableLink {
            card_number: number.into(),
            raw: raw.unwrap_or("").into(),
        })?
    } else {
        None
    };
    let source_title =
        opt_str(number, py::get(rec, "source_title"), "source_title")?.map(Into::into);
    let extra_names: Box<[Box<str>]> = Box::new([]);
    let names = model::names_of(&name, &extra_names, pilot_name.as_deref());
    Ok(CardDef {
        def_id,
        card_number: py::str_of(require(CARDS_FILE, rec, "card_number")?).into(),
        name,
        card_type,
        color,
        level: to_int(number, py::get(rec, "level"))?,
        cost: to_int(number, py::get(rec, "cost"))?,
        ap,
        hp,
        zones,
        traits: traits_of(number, rec)?,
        link,
        effect,
        pilot_name,
        set_code: py::get_truthy(rec, "set_code")
            .map_or_else(|| "".into(), |v| py::str_of(v).into()),
        rarity: py::get_truthy(rec, "rarity").map_or_else(|| "".into(), |v| py::str_of(v).into()),
        product_ids,
        canonical_product_id: py::str_of(require(CARDS_FILE, rec, "product_id")?).into(),
        source_title,
        extra_names,
        names,
    })
}

/// The card numbers a resolution names (`cards/db.py:90-94`), with whether the singular
/// `card_number` key supplied them. Non-string entries are kept out: Python appends them to
/// the index, where no string card number ever finds them.
fn resolution_numbers(cid: &str, res: &Value) -> Result<(Vec<String>, bool), DataError> {
    if let Some(v) = py::get_truthy(res, "card_numbers") {
        let Value::Array(items) = v else {
            return Err(shape(
                OVERRIDES_FILE,
                format!("{cid}: \"card_numbers\" is not a list: {v}"),
            ));
        };
        let numbers = items
            .iter()
            .filter_map(|n| n.as_str().map(str::to_owned))
            .collect();
        return Ok((numbers, false));
    }
    if let Some(v) = py::get_truthy(res, "card_number") {
        return Ok((v.as_str().map(str::to_owned).into_iter().collect(), true));
    }
    let inferred = cid
        .split(':')
        .nth(1)
        .map(str::to_owned)
        .into_iter()
        .collect();
    Ok((inferred, false))
}

/// card number → `[(conflict_id, resolution)]`, the resolutions in sorted conflict-id order.
type OverrideIndex<'a> = BTreeMap<String, Vec<(&'a str, &'a Value)>>;

/// `_card_override_index(overrides)` (`cards/db.py:80-97`).
fn override_index<'a>(
    overrides: &'a Value,
    dead: &mut DeadPaths,
) -> Result<OverrideIndex<'a>, DataError> {
    let Some(top) = overrides.as_object() else {
        return Err(shape(OVERRIDES_FILE, "the document is not an object"));
    };
    let mut out: BTreeMap<String, Vec<(&str, &Value)>> = BTreeMap::new();
    let Some(resolutions) = top.get("resolutions") else {
        return Ok(out);
    };
    let Some(resolutions) = resolutions.as_object() else {
        return Err(shape(OVERRIDES_FILE, "\"resolutions\" is not an object"));
    };
    let mut ids: Vec<&String> = resolutions.keys().collect();
    ids.sort();
    for cid in ids {
        let res = &resolutions[cid.as_str()];
        if !res.is_object() {
            continue;
        }
        if py::get_truthy(res, "canonical_product_id").is_none()
            && py::get_truthy(res, "field_overrides").is_none()
        {
            continue;
        }
        let (numbers, singular) = resolution_numbers(cid, res)?;
        dead.singular_card_number |= singular;
        for n in numbers {
            out.entry(n).or_default().push((cid.as_str(), res));
        }
    }
    Ok(out)
}

/// `dict.update` of a JSON object: existing keys keep their position, new keys are appended
/// (serde_json's `preserve_order` map behaves as a Python dict here).
fn update(target: &mut Map<String, Value>, from: &Map<String, Value>) {
    for (k, v) in from {
        target.insert(k.clone(), v.clone());
    }
}

fn to_def_id(n: usize) -> Result<DefId, DataError> {
    u16::try_from(n)
        .map(DefId)
        .map_err(|_| shape(CARDS_FILE, "more than 65,536 card definitions"))
}

impl fmt::Display for DataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let repr = unicode::repr_str;
        match self {
            Self::Io { path, error } => write!(f, "cannot read {}: {error}", path.display()),
            Self::Json {
                file,
                line,
                message,
            } => {
                if *line > 0 {
                    write!(f, "{file} line {line} is not valid JSON: {message}")
                } else {
                    write!(f, "{file} is not valid JSON: {message}")
                }
            }
            Self::Shape { file, detail } => write!(f, "{file}: unexpected data: {detail}"),
            Self::UnknownCanonicalPrinting {
                card_number,
                preferred,
            } => write!(
                f,
                "{card_number}: override names unknown canonical printing {}",
                repr(preferred)
            ),
            Self::UnknownCardType { card_number, value } => {
                write!(f, "{card_number}: {} is not a valid CardType", repr(value))
            }
            Self::UnknownColor { card_number, value } => {
                write!(f, "{card_number}: {} is not a valid Color", repr(value))
            }
            Self::UnparseableLink { card_number, raw } => {
                write!(
                    f,
                    "{card_number}: unparseable link condition: {}",
                    repr(raw)
                )
            }
            Self::BadModifier { card_number, raw } => write!(
                f,
                "{card_number}: invalid literal for int() with base 10: {}",
                repr(&raw.replace('+', ""))
            ),
            Self::UnknownPredicateOperator { rule_id, key } => write!(
                f,
                "{rule_id}: unknown banlist predicate operator {}",
                repr(key)
            ),
        }
    }
}

impl std::error::Error for DataError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { error, .. } => Some(error),
            _ => None,
        }
    }
}

impl CardDb {
    /// `get_card_db()` (`cards/db.py:327-329`): the packaged snapshot, loaded once.
    ///
    /// # Panics
    /// If the embedded data does not load, which is a build defect (`cargo test -p gcg-data`
    /// loads it).
    #[must_use]
    pub fn embedded() -> &'static Self {
        static DB: OnceLock<CardDb> = OnceLock::new();
        DB.get_or_init(|| {
            Self::from_sources(embedded::CARDS_NDJSON, embedded::OVERRIDES_JSON)
                .unwrap_or_else(|e| panic!("embedded card data does not load: {e}"))
        })
    }

    /// `CardDB(load_raw_printings(text), load_overrides(text))` (`cards/db.py:57-69`,
    /// `:119-158`). W-157: an unparsable overrides document is an error naming the file, never
    /// an empty default.
    ///
    /// # Errors
    /// Invalid JSON, records of an unexpected shape, and the fatal load errors of Python
    /// (unknown canonical printing, card type or color, unparseable link or modifier).
    pub fn from_sources(cards_ndjson: &str, overrides: &str) -> Result<Self, DataError> {
        Self::from_sources_traced(cards_ndjson, overrides).map(|(db, _)| db)
    }

    /// [`CardDb::from_sources`] plus the dead loader paths the sources took (W-164 tripwire).
    pub(crate) fn from_sources_traced(
        cards_ndjson: &str,
        overrides: &str,
    ) -> Result<(Self, DeadPaths), DataError> {
        let mut printings = Vec::new();
        for (i, line) in unicode::splitlines(cards_ndjson).into_iter().enumerate() {
            if unicode::strip(line).is_empty() {
                continue;
            }
            let rec: Value = serde_json::from_str(line).map_err(|e| DataError::Json {
                file: CARDS_FILE.into(),
                line: i + 1,
                message: e.to_string(),
            })?;
            printings.push(rec);
        }
        let overrides: Value = serde_json::from_str(overrides).map_err(|e| DataError::Json {
            file: OVERRIDES_FILE.into(),
            line: 0,
            message: e.to_string(),
        })?;
        Self::build(&printings, &overrides)
    }

    /// `CardDB.__init__` (`cards/db.py:119-158`).
    fn build(printings: &[Value], overrides: &Value) -> Result<(Self, DeadPaths), DataError> {
        let mut dead = DeadPaths::default();
        let mut by_number_groups: BTreeMap<String, Vec<&Value>> = BTreeMap::new();
        for p in printings {
            let number = require_str(CARDS_FILE, p, "card_number")?;
            by_number_groups
                .entry(number.to_owned())
                .or_default()
                .push(p);
        }
        let index = override_index(overrides, &mut dead)?;
        let mut defs: Vec<CardDef> = Vec::new();
        let mut applied: Vec<AppliedOverride> = Vec::new();
        let mut by_number: LookupMap<Box<str>, DefId> = LookupMap::new();
        let mut product_to_def: LookupMap<Box<str>, DefId> = LookupMap::new();
        for (number, group) in &by_number_groups {
            let mut ps: Vec<(&str, &Value)> = Vec::with_capacity(group.len());
            for p in group {
                ps.push((require_str(CARDS_FILE, p, "product_id")?, *p));
            }
            ps.sort_by(|a, b| a.0.cmp(b.0));
            let mut preferred: Option<String> = None;
            let mut field_overrides: Map<String, Value> = Map::new();
            for &(cid, res) in index.get(number).map_or(&[][..], Vec::as_slice) {
                if let Some(v) = py::get_truthy(res, "canonical_product_id") {
                    preferred = Some(py::str_of(v));
                }
                let empty = Map::new();
                let mut fo: &Map<String, Value> = match py::get_truthy(res, "field_overrides") {
                    None => &empty,
                    Some(Value::Object(m)) => m,
                    Some(other) => {
                        return Err(shape(
                            OVERRIDES_FILE,
                            format!("{cid}: \"field_overrides\" is not an object: {other}"),
                        ));
                    }
                };
                if let Some(Value::Object(nested)) = fo.get(number.as_str()) {
                    fo = nested;
                    dead.nested_field_overrides = true;
                }
                update(&mut field_overrides, fo);
                let decision = match py::get(res, "decision").or_else(|| py::get(res, "policy")) {
                    Some(v) => py::str_of(v),
                    None => String::new(),
                };
                let mut fields: Vec<&String> = fo.keys().collect();
                fields.sort();
                applied.push(AppliedOverride {
                    conflict_id: cid.into(),
                    card_number: number.as_str().into(),
                    decision: decision.into(),
                    fields: fields.into_iter().map(|k| k.as_str().into()).collect(),
                });
            }
            let canonical = match &preferred {
                Some(want) => {
                    ps.iter()
                        .find(|(pid, _)| *pid == want.as_str())
                        .ok_or_else(|| DataError::UnknownCanonicalPrinting {
                            card_number: number.as_str().into(),
                            preferred: want.as_str().into(),
                        })?
                        .1
                }
                None => {
                    ps.iter()
                        .find(|(pid, p)| {
                            py::get(p, "card_number").and_then(Value::as_str) == Some(pid)
                        })
                        .unwrap_or(&ps[0])
                        .1
                }
            };
            let mut canon = canonical
                .as_object()
                .cloned()
                .ok_or_else(|| shape(CARDS_FILE, "a printing is not an object"))?;
            update(&mut canon, &field_overrides);
            let def_id = to_def_id(defs.len())?;
            let product_ids: Box<[Box<str>]> = ps.iter().map(|(pid, _)| (*pid).into()).collect();
            defs.push(make_def(
                def_id,
                number,
                &Value::Object(canon),
                product_ids,
            )?);
            by_number.insert(number.as_str().into(), def_id);
            // `get` ignores a product id that maps to the empty card number (`if number`,
            // `cards/db.py:297`). The empty number sorts first, so any later number that
            // shares the product id overwrites it, exactly as in Python's map.
            if !number.is_empty() {
                for (pid, _) in &ps {
                    product_to_def.insert((*pid).into(), def_id);
                }
            }
        }
        let token_by_key = register_inline_tokens(&mut defs, &mut dead)?;
        let (unit_variant, variant_base) = register_unit_variants(&mut defs)?;
        let db = Self {
            defs: defs.into(),
            by_number,
            product_to_def,
            token_by_key: token_by_key.into(),
            unit_variant: unit_variant.into(),
            variant_base: variant_base.into(),
            applied_overrides: applied.into(),
        };
        Ok((db, dead))
    }

    /// `len(db)` (`cards/db.py:283-284`).
    #[must_use]
    pub fn len(&self) -> usize {
        self.defs.len()
    }

    /// `not db` (`len(db) == 0`). A database loaded from the packaged snapshot always holds
    /// definitions.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }

    /// `iter(db)` (`cards/db.py:286-287`): every definition in def_id order.
    #[must_use]
    pub fn defs(&self) -> &[CardDef] {
        &self.defs
    }

    /// `db.by_id(def_id)` (`cards/db.py:289-290`).
    ///
    /// # Panics
    /// If `id` is not a def_id of this database (Python raises `IndexError`).
    #[must_use]
    pub fn by_id(&self, id: DefId) -> &CardDef {
        &self.defs[usize::from(id.0)]
    }

    /// `db.get(s)` (`cards/db.py:292-297`): `s.strip()` (Python whitespace) as a card number,
    /// else as a product id; case-sensitive. Synthetic `TOKEN:`/`UNIT:` numbers are not keys.
    #[must_use]
    pub fn get(&self, number_or_product_id: &str) -> Option<&CardDef> {
        let key = unicode::strip(number_or_product_id);
        if let Some(&id) = self.by_number.get(key) {
            return Some(self.by_id(id));
        }
        self.product_to_def.get(key).map(|&id| self.by_id(id))
    }

    /// `db.normalize_id(s)` (`cards/db.py:305-307`).
    #[must_use]
    pub fn normalize_id(&self, s: &str) -> Option<&str> {
        self.get(s).map(|d| &*d.card_number)
    }

    /// `db.real_cards()` (`cards/db.py:309-313`): definitions whose number does not start with
    /// `TOKEN:` or `UNIT:`, in def_id order.
    pub fn real_cards(&self) -> impl Iterator<Item = &CardDef> {
        self.defs.iter().filter(|d| {
            !d.card_number.starts_with(TOKEN_PREFIX)
                && !d.card_number.starts_with(UNIT_VARIANT_PREFIX)
        })
    }

    /// `db.unit_variant(def_id)` (`cards/db.py:274-277`).
    #[must_use]
    pub fn unit_variant(&self, id: DefId) -> Option<&CardDef> {
        self.unit_variant
            .get(usize::from(id.0))
            .copied()
            .flatten()
            .map(|v| self.by_id(v))
    }

    /// `db.base_def_id(def_id)` (`cards/db.py:279-281`): identity except for Unit variants.
    #[must_use]
    pub fn base_def_id(&self, id: DefId) -> DefId {
        self.variant_base
            .get(usize::from(id.0))
            .copied()
            .flatten()
            .unwrap_or(id)
    }

    /// `db.token_for(spec)` = `_token_by_key[spec.key]` (`cards/db.py:315-316`).
    #[must_use]
    pub fn token_for_key(&self, key: &str) -> Option<&CardDef> {
        self.token_by_key
            .binary_search_by(|(k, _)| (**k).cmp(key))
            .ok()
            .map(|i| self.by_id(self.token_by_key[i].1))
    }

    /// `db.ex_base` (`cards/db.py:318-320`).
    ///
    /// # Panics
    /// If the snapshot has no `EXB-001` (Python raises `KeyError`).
    #[must_use]
    pub fn ex_base(&self) -> &CardDef {
        let id = self.by_number.get(EX_BASE_NUMBER).copied();
        self.by_id(id.unwrap_or_else(|| panic!("card data has no {EX_BASE_NUMBER}")))
    }

    /// `db.ex_resource` (`cards/db.py:322-324`).
    ///
    /// # Panics
    /// If the snapshot has no `EXR-001` (Python raises `KeyError`).
    #[must_use]
    pub fn ex_resource(&self) -> &CardDef {
        let id = self.by_number.get(EX_RESOURCE_NUMBER).copied();
        self.by_id(id.unwrap_or_else(|| panic!("card data has no {EX_RESOURCE_NUMBER}")))
    }

    /// `db.applied_overrides` (`cards/db.py:124-146`): card numbers in sorted order, then
    /// conflict ids in sorted order.
    #[must_use]
    pub fn applied_overrides(&self) -> &[AppliedOverride] {
        &self.applied_overrides
    }

    /// The four recorded fingerprints: the full CardDef dump, def_id → card number, token
    /// key → card number, and the applied overrides (lowercase SHA-256 hex each).
    #[must_use]
    pub fn fingerprints(&self) -> [String; 4] {
        let compact = DumpOptions {
            indent: None,
            sort_keys: false,
            compact: true,
            ensure_ascii: false,
        };
        let mut h = Sha256::new();
        for d in self.defs.iter() {
            h.update(dumps(&dump_record(d), compact).as_bytes());
            h.update(b"\n");
        }
        let carddefs = py::hex(&h.finalize());
        let mut h = Sha256::new();
        for d in self.defs.iter() {
            h.update(format!("{}\t{}\n", d.def_id.0, d.card_number).as_bytes());
        }
        let def_ids = py::hex(&h.finalize());
        let mut h = Sha256::new();
        for (k, id) in self.token_by_key.iter() {
            h.update(format!("{k}\t{}\n", self.by_id(*id).card_number).as_bytes());
        }
        let token_keys = py::hex(&h.finalize());
        let mut h = Sha256::new();
        for o in self.applied_overrides.iter() {
            h.update(
                format!(
                    "{}\t{}\t{}\t{}\n",
                    o.conflict_id,
                    o.card_number,
                    o.decision,
                    o.fields.join(",")
                )
                .as_bytes(),
            );
        }
        let applied = py::hex(&h.finalize());
        [carddefs, def_ids, token_keys, applied]
    }
}

/// The record of one definition in the definitions dump (`[def_id, card_number, ...,
/// source_title]`).
pub(crate) fn dump_record(d: &CardDef) -> Json {
    let strs = |xs: &[Box<str>]| Json::Array(xs.iter().map(|x| Json::from(&**x)).collect());
    let link = d.link.as_ref().map_or(Json::Null, |l| {
        Json::Array(vec![strs(&l.name_parts), strs(&l.traits)])
    });
    Json::Array(vec![
        Json::from(d.def_id.0),
        Json::from(&*d.card_number),
        Json::from(&*d.name),
        Json::from(d.card_type.as_str()),
        d.color.map_or(Json::Null, |c| Json::from(c.as_str())),
        Json::from(d.level),
        Json::from(d.cost),
        Json::from(d.ap),
        Json::from(d.hp),
        strs(&d.zones),
        strs(&d.traits),
        link,
        Json::from(&*d.effect),
        d.pilot_name.as_deref().map_or(Json::Null, Json::from),
        Json::from(&*d.set_code),
        Json::from(&*d.rarity),
        strs(&d.product_ids),
        Json::from(&*d.canonical_product_id),
        d.source_title.as_deref().map_or(Json::Null, Json::from),
    ])
}

/// Trait multisets compared as sets (`set(t.traits) == set(s.traits)`, `cards/db.py:215`).
fn trait_set(traits: &[Box<str>]) -> Vec<&str> {
    let mut v: Vec<&str> = traits.iter().map(|t| &**t).collect();
    v.sort();
    v.dedup();
    v
}

/// `CardDB._register_inline_tokens` (`cards/db.py:199-241`): returns `_token_by_key` sorted by
/// key.
fn register_inline_tokens(
    defs: &mut Vec<CardDef>,
    dead: &mut DeadPaths,
) -> Result<Vec<(Box<str>, DefId)>, DataError> {
    let t_cards: Vec<DefId> = defs
        .iter()
        .filter(|d| matches!(d.card_type, CardType::UnitToken | CardType::BaseToken))
        .map(|d| d.def_id)
        .collect();
    let mut specs: BTreeMap<String, TokenSpec> = BTreeMap::new();
    for d in defs.iter() {
        let parsed = model::parse_token_specs(&d.effect).map_err(|()| {
            shape(
                CARDS_FILE,
                format!("{}: token AP/HP out of range", d.card_number),
            )
        })?;
        for s in parsed {
            specs.entry(s.key()).or_insert(s);
        }
    }
    let mut out = Vec::with_capacity(specs.len());
    for (key, s) in specs {
        let wanted = trait_set(&s.traits);
        let mut best: Option<&CardDef> = None;
        let mut matches = 0usize;
        for &id in &t_cards {
            let t = &defs[usize::from(id.0)];
            if *t.name == *s.name && t.ap == s.ap && t.hp == s.hp && trait_set(&t.traits) == wanted
            {
                matches += 1;
                if best.is_none_or(|b| t.card_number < b.card_number) {
                    best = Some(t);
                }
            }
        }
        if let Some(b) = best {
            dead.token_tie |= matches > 1;
            out.push((key.into(), b.def_id));
            continue;
        }
        dead.synthetic_token = true;
        let def_id = to_def_id(defs.len())?;
        let name: Box<str> = s.name.as_str().into();
        let names = model::names_of(&name, &[], None);
        defs.push(CardDef {
            def_id,
            card_number: format!("{TOKEN_PREFIX}{}:{}/{}", s.name, s.ap, s.hp).into(),
            name,
            card_type: CardType::UnitToken,
            color: None,
            level: 0,
            cost: 0,
            ap: s.ap,
            hp: s.hp,
            zones: Box::new([]),
            traits: s.traits.into(),
            link: None,
            effect: if s.text.is_empty() {
                "-".into()
            } else {
                s.text.into()
            },
            pilot_name: None,
            set_code: "TOKEN".into(),
            rarity: "".into(),
            product_ids: Box::new([]),
            canonical_product_id: "".into(),
            source_title: None,
            extra_names: Box::new([]),
            names,
        });
        out.push((key.into(), def_id));
    }
    Ok(out)
}

type VariantMaps = (Vec<Option<DefId>>, Vec<Option<DefId>>);

/// `CardDB._register_unit_variants` (`cards/db.py:243-272`): `(_unit_variant, _variant_base)`
/// indexed by def_id.
fn register_unit_variants(defs: &mut Vec<CardDef>) -> Result<VariantMaps, DataError> {
    let snapshot = defs.len();
    let mut pairs = Vec::new();
    for i in 0..snapshot {
        let base = &defs[i];
        let Some((ap_text, hp_text)) = model::as_unit(&base.effect) else {
            continue;
        };
        if base.card_type.is_unit() {
            continue;
        }
        let bad = || {
            shape(
                CARDS_FILE,
                format!("{}: Unit variant AP/HP out of range", base.card_number),
            )
        };
        let ap = model::digits_value(ap_text).ok_or_else(bad)?;
        let hp = model::digits_value(hp_text).ok_or_else(bad)?;
        let def_id = to_def_id(defs.len())?;
        let variant = CardDef {
            def_id,
            card_number: format!(
                "{UNIT_VARIANT_PREFIX}{}:{ap_text}/{hp_text}",
                base.card_number
            )
            .into(),
            name: base.name.clone(),
            card_type: CardType::Unit,
            color: base.color,
            level: base.level,
            cost: base.cost,
            ap,
            hp,
            zones: Box::new([]),
            traits: base.traits.clone(),
            link: None,
            effect: "-".into(),
            pilot_name: None,
            set_code: base.set_code.clone(),
            rarity: "".into(),
            product_ids: Box::new([]),
            canonical_product_id: "".into(),
            source_title: None,
            extra_names: Box::new([]),
            names: model::names_of(&base.name, &[], None),
        };
        pairs.push((base.def_id, def_id));
        defs.push(variant);
    }
    let mut unit_variant = vec![None; defs.len()];
    let mut variant_base = vec![None; defs.len()];
    for (base, variant) in pairs {
        unit_variant[usize::from(base.0)] = Some(variant);
        variant_base[usize::from(variant.0)] = Some(base);
    }
    Ok((unit_variant, variant_base))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synthetic_cases() -> Vec<Value> {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/vectors/synthetic.json");
        let text = std::fs::read_to_string(path).expect("synthetic.json (export_data.py)");
        let fx: Value = serde_json::from_str(&text).expect("synthetic.json parses");
        fx["cases"].as_array().expect("cases").clone()
    }

    fn flags(d: DeadPaths) -> Vec<&'static str> {
        let all = [
            ("synthetic_token", d.synthetic_token),
            ("token_tie", d.token_tie),
            ("nested_field_overrides", d.nested_field_overrides),
            ("singular_card_number", d.singular_card_number),
        ];
        all.into_iter()
            .filter(|(_, on)| *on)
            .map(|(n, _)| n)
            .collect()
    }

    /// W-164 tripwire: the packaged data takes none of the dead loader paths (a data refresh
    /// that takes one fails here and is reviewed), and each synthetic source takes exactly the
    /// path it was written for.
    #[test]
    fn dead_path_tripwire() {
        let (_, dead) =
            CardDb::from_sources_traced(embedded::CARDS_NDJSON, embedded::OVERRIDES_JSON)
                .expect("packaged data loads");
        assert_eq!(
            flags(dead),
            Vec::<&str>::new(),
            "packaged data took a dead loader path"
        );
        for case in synthetic_cases() {
            if case.get("error").is_some() {
                continue;
            }
            let id = case["id"].as_str().expect("id");
            let (_, dead) = CardDb::from_sources_traced(
                case["cards_ndjson"].as_str().expect("cards"),
                case["overrides"].as_str().expect("overrides"),
            )
            .expect("synthetic case loads");
            let want: Vec<&str> = case["dead_paths"]
                .as_array()
                .expect("dead_paths")
                .iter()
                .map(|v| v.as_str().expect("flag"))
                .collect();
            assert_eq!(flags(dead), want, "{id}");
        }
    }

    /// The 50 resolutions without a card-number field infer it from the conflict id
    /// (`cards/db.py:93-94`), a live path; the packaged data resolves their card data.
    #[test]
    fn id_inference_is_live() {
        let overrides: Value =
            serde_json::from_str(embedded::OVERRIDES_JSON).expect("overrides parse");
        let mut dead = DeadPaths::default();
        let index = override_index(&overrides, &mut dead).expect("index");
        let inferred = index
            .values()
            .flatten()
            .filter(|(_, res)| {
                py::get_truthy(res, "card_numbers").is_none()
                    && py::get_truthy(res, "card_number").is_none()
            })
            .count();
        assert_eq!(inferred, 66);
    }
}
