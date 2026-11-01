//! The in-house JSON Schema validator for `results.json` (docs/port/PLAN.md §9.5): exactly the
//! draft 2020-12 keywords the results schemas use, with jsonschema 4.26's semantics
//! (`jsonschema.validate(obj, schema, format_checker=FormatChecker())`,
//! `reports/results.py:48-50`).
//!
//! The schema compiles once; an unsupported keyword, `$ref`, `format` or `pattern` is a compile
//! error, so the subset is provably sufficient for every schema that compiles (the tests
//! compile the embedded schema and compare its keyword list with Python's).
//!
//! Semantics reproduced from jsonschema: `integer` accepts floats with no fractional part and
//! never booleans; `const`/`enum` equality treats `1 == 1.0`, never `true == 1`, and compares
//! objects without order; numeric bounds compare ints and floats exactly; `minLength` counts
//! code points; `pattern` is `re.search` (so `$` also matches before a final newline);
//! `format: date` is `^\d{4}-\d{2}-\d{2}$` (ASCII digits) plus `date.fromisoformat`;
//! `unevaluatedProperties` sees the properties evaluated by `properties`,
//! `additionalProperties`, `$ref` and the passing `oneOf` branch.

use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::fmt;
use std::sync::OnceLock;

use gcg_core::pyjson::{DumpOptions, Json, Key, dumps};
use serde_json::Value;

use super::pyregex::Regex;
use crate::{SchemaError, SchemaVersion};

/// The 1.1.0 results schema, Python's file byte for byte (`reports/results.py:26`, `:41-45`).
pub const RESULTS_SCHEMA_V1_1: &str =
    include_str!("../../../../src/gcg_sim/reports/schema/results.v1.schema.json");

/// The keywords the validator implements; anything else in a schema is a compile error.
pub const SUPPORTED_KEYWORDS: [&str; 26] = [
    "$defs",
    "$id",
    "$ref",
    "$schema",
    "additionalProperties",
    "const",
    "description",
    "enum",
    "exclusiveMaximum",
    "exclusiveMinimum",
    "format",
    "items",
    "maxItems",
    "maximum",
    "minItems",
    "minLength",
    "minimum",
    "oneOf",
    "pattern",
    "prefixItems",
    "properties",
    "propertyNames",
    "required",
    "title",
    "type",
    "unevaluatedProperties",
];

impl fmt::Display for SchemaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.pointer.is_empty() {
            f.write_str(&self.message)
        } else {
            write!(f, "{} (at {})", self.message, self.pointer)
        }
    }
}

impl std::error::Error for SchemaError {}

/// `serde_json::Value` as the writer's [`Json`] (integers stay integers, keys keep their
/// order).
#[must_use]
pub fn json_from_value(v: &Value) -> Json {
    match v {
        Value::Null => Json::Null,
        Value::Bool(b) => Json::Bool(*b),
        Value::Number(n) => n.as_u64().map_or_else(
            || {
                n.as_i64().map_or_else(
                    || Json::Float(n.as_f64().unwrap_or(f64::NAN)),
                    |i| Json::Int(i128::from(i)),
                )
            },
            |u| Json::Int(i128::from(u)),
        ),
        Value::String(s) => Json::Str(s.clone()),
        Value::Array(items) => Json::Array(items.iter().map(json_from_value).collect()),
        Value::Object(map) => Json::Object(
            map.iter()
                .map(|(k, v)| (Key::Str(k.clone()), json_from_value(v)))
                .collect(),
        ),
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Type {
    Null,
    Boolean,
    Integer,
    Number,
    String,
    Array,
    Object,
}

impl Type {
    fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "null" => Self::Null,
            "boolean" => Self::Boolean,
            "integer" => Self::Integer,
            "number" => Self::Number,
            "string" => Self::String,
            "array" => Self::Array,
            "object" => Self::Object,
            _ => return None,
        })
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Null => "null",
            Self::Boolean => "boolean",
            Self::Integer => "integer",
            Self::Number => "number",
            Self::String => "string",
            Self::Array => "array",
            Self::Object => "object",
        }
    }

    fn admits(self, v: &Json) -> bool {
        match (self, v) {
            (Self::Null, Json::Null)
            | (Self::Boolean, Json::Bool(_))
            | (Self::Integer | Self::Number, Json::Int(_))
            | (Self::Number, Json::Float(_))
            | (Self::String, Json::Str(_))
            | (Self::Array, Json::Array(_))
            | (Self::Object, Json::Object(_)) => true,
            (Self::Integer, Json::Float(x)) => x.is_finite() && x.fract() == 0.0,
            _ => false,
        }
    }
}

type NodeId = usize;

/// One compiled (sub)schema.
#[derive(Default, Debug)]
struct Node {
    /// `true` / `false` boolean schemas.
    always: Option<bool>,
    pointer: String,
    reference: Option<String>,
    resolved: Option<NodeId>,
    types: Option<Vec<Type>>,
    constant: Option<Json>,
    choices: Option<Vec<Json>>,
    required: Vec<String>,
    properties: Vec<(String, NodeId)>,
    additional: Option<NodeId>,
    property_names: Option<NodeId>,
    unevaluated: Option<NodeId>,
    items: Option<NodeId>,
    prefix_items: Vec<NodeId>,
    min_items: Option<u64>,
    max_items: Option<u64>,
    minimum: Option<Json>,
    maximum: Option<Json>,
    exclusive_minimum: Option<Json>,
    exclusive_maximum: Option<Json>,
    min_length: Option<u64>,
    pattern: Option<(String, Regex)>,
    date: bool,
    one_of: Vec<NodeId>,
}

/// A compiled schema.
#[derive(Debug)]
pub struct Schema {
    nodes: Vec<Node>,
}

/// Why a schema did not compile.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct CompileError {
    pub pointer: String,
    pub message: String,
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "schema at '{}': {}", self.pointer, self.message)
    }
}

impl std::error::Error for CompileError {}

fn escape_pointer(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}

fn compact(v: &Json) -> String {
    dumps(
        v,
        DumpOptions {
            indent: None,
            sort_keys: false,
            compact: false,
            ensure_ascii: false,
        },
    )
}

struct Compiler {
    nodes: Vec<Node>,
}

impl Compiler {
    fn err(pointer: &str, message: impl Into<String>) -> CompileError {
        CompileError {
            pointer: pointer.to_owned(),
            message: message.into(),
        }
    }

    fn number(v: &Value, pointer: &str, key: &str) -> Result<Json, CompileError> {
        if v.is_number() {
            Ok(json_from_value(v))
        } else {
            Err(Self::err(pointer, format!("{key} must be a number")))
        }
    }

    fn count(v: &Value, pointer: &str, key: &str) -> Result<u64, CompileError> {
        v.as_u64()
            .ok_or_else(|| Self::err(pointer, format!("{key} must be a non-negative integer")))
    }

    fn compile(&mut self, v: &Value, pointer: String) -> Result<NodeId, CompileError> {
        let id = self.nodes.len();
        self.nodes.push(Node {
            pointer: pointer.clone(),
            ..Node::default()
        });
        let obj = match v {
            Value::Bool(b) => {
                self.nodes[id].always = Some(*b);
                return Ok(id);
            }
            Value::Object(obj) => obj,
            _ => {
                return Err(Self::err(
                    &pointer,
                    "a schema must be an object or a boolean",
                ));
            }
        };
        let at = |k: &str| format!("{pointer}/{}", escape_pointer(k));
        let mut node = Node {
            pointer: pointer.clone(),
            ..Node::default()
        };
        for (key, value) in obj {
            match key.as_str() {
                "$schema" | "$id" | "title" | "description" => {}
                "$defs" => {
                    let Value::Object(defs) = value else {
                        return Err(Self::err(&at(key), "$defs must be an object"));
                    };
                    for (name, sub) in defs {
                        self.compile(sub, format!("{}/{}", at(key), escape_pointer(name)))?;
                    }
                }
                "$ref" => {
                    let target = value
                        .as_str()
                        .and_then(|r| r.strip_prefix('#'))
                        .ok_or_else(|| Self::err(&at(key), "only local '#...' references"))?;
                    node.reference = Some(target.to_owned());
                }
                "type" => {
                    let names: Vec<&Value> = match value {
                        Value::Array(items) => items.iter().collect(),
                        other => vec![other],
                    };
                    let mut types = Vec::new();
                    for n in names {
                        let t = n
                            .as_str()
                            .and_then(Type::from_name)
                            .ok_or_else(|| Self::err(&at(key), format!("unknown type {n}")))?;
                        types.push(t);
                    }
                    node.types = Some(types);
                }
                "const" => node.constant = Some(json_from_value(value)),
                "enum" => {
                    let Value::Array(items) = value else {
                        return Err(Self::err(&at(key), "enum must be an array"));
                    };
                    node.choices = Some(items.iter().map(json_from_value).collect());
                }
                "required" => {
                    let names = value
                        .as_array()
                        .and_then(|a| a.iter().map(|n| n.as_str().map(str::to_owned)).collect())
                        .ok_or_else(|| Self::err(&at(key), "required must list strings"))?;
                    node.required = names;
                }
                "properties" => {
                    let Value::Object(props) = value else {
                        return Err(Self::err(&at(key), "properties must be an object"));
                    };
                    for (name, sub) in props {
                        let sub_id =
                            self.compile(sub, format!("{}/{}", at(key), escape_pointer(name)))?;
                        node.properties.push((name.clone(), sub_id));
                    }
                }
                "additionalProperties" => node.additional = Some(self.compile(value, at(key))?),
                "propertyNames" => node.property_names = Some(self.compile(value, at(key))?),
                "unevaluatedProperties" => node.unevaluated = Some(self.compile(value, at(key))?),
                "items" => node.items = Some(self.compile(value, at(key))?),
                "prefixItems" | "oneOf" => {
                    let Value::Array(subs) = value else {
                        return Err(Self::err(&at(key), format!("{key} must be an array")));
                    };
                    let mut ids = Vec::new();
                    for (i, sub) in subs.iter().enumerate() {
                        ids.push(self.compile(sub, format!("{}/{i}", at(key)))?);
                    }
                    if key == "oneOf" {
                        node.one_of = ids;
                    } else {
                        node.prefix_items = ids;
                    }
                }
                "minItems" => node.min_items = Some(Self::count(value, &at(key), key)?),
                "maxItems" => node.max_items = Some(Self::count(value, &at(key), key)?),
                "minLength" => node.min_length = Some(Self::count(value, &at(key), key)?),
                "minimum" => node.minimum = Some(Self::number(value, &at(key), key)?),
                "maximum" => node.maximum = Some(Self::number(value, &at(key), key)?),
                "exclusiveMinimum" => {
                    node.exclusive_minimum = Some(Self::number(value, &at(key), key)?);
                }
                "exclusiveMaximum" => {
                    node.exclusive_maximum = Some(Self::number(value, &at(key), key)?);
                }
                "pattern" => {
                    let text = value
                        .as_str()
                        .ok_or_else(|| Self::err(&at(key), "pattern must be a string"))?;
                    let re = Regex::new(text).map_err(|e| Self::err(&at(key), e.to_string()))?;
                    node.pattern = Some((text.to_owned(), re));
                }
                "format" => {
                    if value.as_str() != Some("date") {
                        return Err(Self::err(&at(key), format!("unsupported format {value}")));
                    }
                    node.date = true;
                }
                other => {
                    return Err(Self::err(
                        &at(other),
                        format!("unsupported keyword {other:?}"),
                    ));
                }
            }
        }
        self.nodes[id] = node;
        Ok(id)
    }
}

impl Schema {
    /// Compiles a schema document.
    ///
    /// # Errors
    /// [`CompileError`] for invalid JSON, an unsupported keyword, `format`, `pattern` or an
    /// unresolved `$ref`.
    pub fn compile(text: &str) -> Result<Self, CompileError> {
        let doc: Value =
            serde_json::from_str(text).map_err(|e| Compiler::err("", e.to_string()))?;
        let mut c = Compiler { nodes: Vec::new() };
        c.compile(&doc, String::new())?;
        let mut nodes = c.nodes;
        for i in 0..nodes.len() {
            if let Some(target) = nodes[i].reference.clone() {
                let found = nodes
                    .iter()
                    .position(|n| n.pointer == target)
                    .ok_or_else(|| {
                        Compiler::err(&nodes[i].pointer, format!("unresolved $ref #{target}"))
                    })?;
                nodes[i].resolved = Some(found);
            }
        }
        Ok(Self { nodes })
    }

    /// Validates `instance`.
    ///
    /// # Errors
    /// The first violation found (depth first, in schema order).
    pub fn validate(&self, instance: &Json) -> Result<(), SchemaError> {
        self.check(0, instance, "").map(|_| ())
    }

    fn fail(pointer: &str, message: String) -> Result<BTreeSet<String>, SchemaError> {
        Err(SchemaError {
            pointer: pointer.to_owned(),
            message,
        })
    }

    /// Validates `v` at `ptr` against node `id`; on success returns the object properties the
    /// node evaluated (for `unevaluatedProperties`).
    fn check(&self, id: NodeId, v: &Json, ptr: &str) -> Result<BTreeSet<String>, SchemaError> {
        let node = &self.nodes[id];
        let mut evaluated = BTreeSet::new();
        if let Some(always) = node.always {
            return if always {
                Ok(evaluated)
            } else {
                Self::fail(ptr, format!("False schema does not allow {}", compact(v)))
            };
        }
        if let Some(target) = node.resolved {
            evaluated.extend(self.check(target, v, ptr)?);
        }
        if let Some(types) = &node.types
            && !types.iter().any(|t| t.admits(v))
        {
            let names: Vec<String> = types.iter().map(|t| format!("'{}'", t.name())).collect();
            return Self::fail(
                ptr,
                format!("{} is not of type {}", compact(v), names.join(", ")),
            );
        }
        if let Some(c) = &node.constant
            && !json_equal(v, c)
        {
            return Self::fail(ptr, format!("{} was expected", compact(c)));
        }
        if let Some(choices) = &node.choices
            && !choices.iter().any(|c| json_equal(v, c))
        {
            let list: Vec<String> = choices.iter().map(compact).collect();
            return Self::fail(
                ptr,
                format!("{} is not one of [{}]", compact(v), list.join(", ")),
            );
        }
        Self::check_number(node, v, ptr)?;
        if let Json::Str(s) = v {
            Self::check_string(node, s, v, ptr)?;
        }
        if let Json::Array(items) = v {
            self.check_array(node, items, v, ptr)?;
        }
        if let Json::Object(props) = v {
            evaluated.extend(self.check_object(node, props, ptr)?);
        }
        if !node.one_of.is_empty() {
            let mut passing = Vec::new();
            for &sub in &node.one_of {
                if let Ok(e) = self.check(sub, v, ptr) {
                    passing.push(e);
                }
            }
            match passing.len() {
                1 => evaluated.extend(passing.remove(0)),
                0 => {
                    return Self::fail(
                        ptr,
                        format!("{} is not valid under any of the given schemas", compact(v)),
                    );
                }
                _ => {
                    return Self::fail(
                        ptr,
                        format!("{} is valid under each of the given schemas", compact(v)),
                    );
                }
            }
        }
        if let (Some(unevaluated), Json::Object(props)) = (node.unevaluated, v) {
            for (k, item) in props {
                let name = key_text(k);
                if evaluated.contains(&name) {
                    continue;
                }
                let at = format!("{ptr}/{}", escape_pointer(&name));
                if self.check(unevaluated, item, &at).is_err() {
                    return Self::fail(
                        ptr,
                        format!("Unevaluated properties are not allowed ('{name}' was unexpected)"),
                    );
                }
                evaluated.insert(name);
            }
        }
        Ok(evaluated)
    }

    fn check_number(node: &Node, v: &Json, ptr: &str) -> Result<(), SchemaError> {
        if !matches!(v, Json::Int(_) | Json::Float(_)) {
            return Ok(());
        }
        let bounds: [(Option<&Json>, Bound); 4] = [
            (node.minimum.as_ref(), Bound::Minimum),
            (node.maximum.as_ref(), Bound::Maximum),
            (node.exclusive_minimum.as_ref(), Bound::ExclusiveMinimum),
            (node.exclusive_maximum.as_ref(), Bound::ExclusiveMaximum),
        ];
        for (bound, kind) in bounds {
            if let Some(b) = bound
                && num_cmp(v, b).is_some_and(|o| kind.fails(o))
            {
                return Err(SchemaError {
                    pointer: ptr.to_owned(),
                    message: format!("{} is {} {}", compact(v), kind.text(), compact(b)),
                });
            }
        }
        Ok(())
    }

    fn check_string(node: &Node, s: &str, v: &Json, ptr: &str) -> Result<(), SchemaError> {
        let fail = |message: String| {
            Err(SchemaError {
                pointer: ptr.to_owned(),
                message,
            })
        };
        if let Some(min) = node.min_length
            && u64::try_from(s.chars().count()).unwrap_or(u64::MAX) < min
        {
            return fail(format!("{} is too short", compact(v)));
        }
        if let Some((text, re)) = &node.pattern
            && !re.is_match(s)
        {
            return fail(format!("{} does not match '{text}'", compact(v)));
        }
        if node.date && !is_date(s) {
            return fail(format!("{} is not a 'date'", compact(v)));
        }
        Ok(())
    }

    fn check_array(
        &self,
        node: &Node,
        items: &[Json],
        v: &Json,
        ptr: &str,
    ) -> Result<(), SchemaError> {
        let len = u64::try_from(items.len()).unwrap_or(u64::MAX);
        if node.min_items.is_some_and(|m| len < m) {
            return Err(SchemaError {
                pointer: ptr.to_owned(),
                message: format!("{} is too short", compact(v)),
            });
        }
        if node.max_items.is_some_and(|m| len > m) {
            return Err(SchemaError {
                pointer: ptr.to_owned(),
                message: format!("{} is too long", compact(v)),
            });
        }
        for (i, item) in items.iter().enumerate() {
            if let Some(sub) = node.prefix_items.get(i).copied().or(node.items) {
                self.check(sub, item, &format!("{ptr}/{i}"))?;
            }
        }
        Ok(())
    }

    fn check_object(
        &self,
        node: &Node,
        props: &[(Key, Json)],
        ptr: &str,
    ) -> Result<BTreeSet<String>, SchemaError> {
        let mut evaluated = BTreeSet::new();
        for name in &node.required {
            if !props.iter().any(|(k, _)| key_text(k) == *name) {
                return Self::fail(ptr, format!("'{name}' is a required property"));
            }
        }
        if let Some(names) = node.property_names {
            for (k, _) in props {
                let name = key_text(k);
                self.check(
                    names,
                    &Json::Str(name.clone()),
                    &format!("{ptr}/{}", escape_pointer(&name)),
                )?;
            }
        }
        let mut extra = Vec::new();
        for (k, item) in props {
            let name = key_text(k);
            let at = format!("{ptr}/{}", escape_pointer(&name));
            if let Some(&(_, sub)) = node.properties.iter().find(|(p, _)| *p == name) {
                self.check(sub, item, &at)?;
                evaluated.insert(name);
            } else if let Some(additional) = node.additional {
                if self.nodes[additional].always == Some(false) {
                    extra.push(name);
                } else {
                    self.check(additional, item, &at)?;
                    evaluated.insert(name);
                }
            }
        }
        if !extra.is_empty() {
            let list: Vec<String> = extra.iter().map(|n| format!("'{n}'")).collect();
            let verb = if extra.len() == 1 { "was" } else { "were" };
            return Self::fail(
                ptr,
                format!(
                    "Additional properties are not allowed ({} {verb} unexpected)",
                    list.join(", ")
                ),
            );
        }
        Ok(evaluated)
    }
}

/// The four numeric bounds; NaN never fails one (Python's comparisons are false).
#[derive(Clone, Copy)]
enum Bound {
    Minimum,
    Maximum,
    ExclusiveMinimum,
    ExclusiveMaximum,
}

impl Bound {
    /// jsonschema fails `instance < minimum`, `instance > maximum`, `instance <= exclusiveMinimum`
    /// and `instance >= exclusiveMaximum`.
    fn fails(self, o: Ordering) -> bool {
        match self {
            Self::Minimum => o == Ordering::Less,
            Self::Maximum => o == Ordering::Greater,
            Self::ExclusiveMinimum => o != Ordering::Greater,
            Self::ExclusiveMaximum => o != Ordering::Less,
        }
    }

    const fn text(self) -> &'static str {
        match self {
            Self::Minimum => "less than the minimum of",
            Self::Maximum => "greater than the maximum of",
            Self::ExclusiveMinimum => "less than or equal to the minimum of",
            Self::ExclusiveMaximum => "greater than or equal to the maximum of",
        }
    }
}

fn key_text(k: &Key) -> String {
    match k {
        Key::Str(s) => s.clone(),
        Key::Int(i) => i.to_string(),
    }
}

/// Exact comparison of two JSON numbers (Python compares `int` and `float` exactly).
fn num_cmp(a: &Json, b: &Json) -> Option<Ordering> {
    match (a, b) {
        (Json::Int(x), Json::Int(y)) => Some(x.cmp(y)),
        (Json::Float(x), Json::Float(y)) => x.partial_cmp(y),
        (Json::Int(x), Json::Float(y)) => int_float_cmp(*x, *y),
        (Json::Float(x), Json::Int(y)) => int_float_cmp(*y, *x).map(Ordering::reverse),
        _ => None,
    }
}

/// `i <=> f`, exactly.
fn int_float_cmp(i: i128, f: f64) -> Option<Ordering> {
    const LIMIT: f64 = 170_141_183_460_469_231_731_687_303_715_884_105_728.0; // 2^127
    if f.is_nan() {
        return None;
    }
    if f >= LIMIT {
        return Some(Ordering::Less);
    }
    if f < -LIMIT {
        return Some(Ordering::Greater);
    }
    let t = f.trunc();
    #[expect(
        clippy::cast_possible_truncation,
        reason = "|t| < 2^127 and integral, so the conversion is exact"
    )]
    let ti = t as i128;
    Some(i.cmp(&ti).then_with(|| {
        let frac = f - t;
        0.0f64.partial_cmp(&frac).unwrap_or(Ordering::Equal)
    }))
}

/// jsonschema's `_utils.equal`: strings by value, arrays element-wise, objects by key set
/// (unordered), booleans only with booleans, numbers numerically (`1 == 1.0`).
fn json_equal(a: &Json, b: &Json) -> bool {
    match (a, b) {
        (Json::Null, Json::Null) => true,
        (Json::Bool(x), Json::Bool(y)) => x == y,
        (Json::Str(x), Json::Str(y)) => x == y,
        (Json::Array(x), Json::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| json_equal(p, q))
        }
        (Json::Object(x), Json::Object(y)) => {
            x.len() == y.len()
                && x.iter().all(|(k, v)| {
                    y.iter()
                        .find(|(k2, _)| key_text(k2) == key_text(k))
                        .is_some_and(|(_, v2)| json_equal(v, v2))
                })
        }
        (Json::Int(_) | Json::Float(_), Json::Int(_) | Json::Float(_)) => {
            num_cmp(a, b) == Some(Ordering::Equal)
        }
        _ => false,
    }
}

/// jsonschema's `is_date` (`_format.py`): `^\d{4}-\d{2}-\d{2}$` (`re.ASCII`) as a full match,
/// then `datetime.date.fromisoformat` (year 1-9999, month 1-12, the month's day count).
#[must_use]
pub fn is_date(s: &str) -> bool {
    let b = s.as_bytes();
    let digits = |r: std::ops::Range<usize>| b[r].iter().all(u8::is_ascii_digit);
    if b.len() != 10
        || b[4] != b'-'
        || b[7] != b'-'
        || !digits(0..4)
        || !digits(5..7)
        || !digits(8..10)
    {
        return false;
    }
    let num = |r: std::ops::Range<usize>| {
        b[r].iter()
            .fold(0u32, |acc, d| acc * 10 + u32::from(d - b'0'))
    };
    let (year, month, day) = (num(0..4), num(5..7), num(8..10));
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    year >= 1 && (1..=days).contains(&day)
}

/// The compiled 1.1.0 schema (compiled once per process).
///
/// # Panics
/// If the embedded schema does not compile (a build defect the tests catch).
#[must_use]
pub fn results_schema_v1_1() -> &'static Schema {
    static SCHEMA: OnceLock<Schema> = OnceLock::new();
    SCHEMA.get_or_init(|| {
        Schema::compile(RESULTS_SCHEMA_V1_1).expect("the embedded 1.1.0 schema compiles")
    })
}

/// `validate_results(obj)` (`reports/results.py:48-50`) for the schema version `v`. Results
/// 1.2.0 (PLAN §12.7) has no schema file before Phase 6 (docs/port/contracts.md O9), so a
/// 1.2.0 document is always rejected and never written.
///
/// # Errors
/// The first violation, or the missing 1.2.0 schema.
pub fn validate_results(doc: &Json, v: SchemaVersion) -> Result<(), SchemaError> {
    match v {
        SchemaVersion::V1_1 => results_schema_v1_1().validate(doc),
        SchemaVersion::V1_2 => Err(SchemaError {
            pointer: String::new(),
            message: format!(
                "results schema {} is not embedded in this build (PLAN §12.7)",
                v.as_str()
            ),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_embedded_schema_compiles() {
        let s = results_schema_v1_1();
        assert!(s.nodes.len() > 100);
    }

    #[test]
    fn unsupported_keywords_fail_to_compile() {
        for text in [
            r#"{"anyOf": []}"#,
            r#"{"format": "email"}"#,
            r#"{"pattern": "\\w+"}"#,
            r##"{"$ref": "#/$defs/missing"}"##,
            r#"{"$ref": "other.json"}"#,
        ] {
            assert!(Schema::compile(text).is_err(), "{text}");
        }
    }

    #[test]
    fn dates() {
        assert!(is_date("2024-02-29"));
        assert!(!is_date("2026-02-29"));
        assert!(!is_date("0000-01-01"));
        assert!(!is_date("2026-09-25\n"));
    }

    #[test]
    fn numbers_compare_exactly() {
        assert_eq!(int_float_cmp(1, 1.0), Some(Ordering::Equal));
        assert_eq!(int_float_cmp(1, 1.5), Some(Ordering::Less));
        assert_eq!(int_float_cmp(-1, -1.5), Some(Ordering::Greater));
        assert_eq!(
            int_float_cmp(i128::from(u64::MAX), 1.8e19),
            Some(Ordering::Greater)
        );
        let two53 = 9_007_199_254_740_992_i128;
        assert_eq!(
            int_float_cmp(two53 + 1, 9_007_199_254_740_992.0),
            Some(Ordering::Greater)
        );
        assert!(json_equal(&Json::Int(3), &Json::Float(3.0)));
        assert!(!json_equal(&Json::Bool(true), &Json::Int(1)));
    }
}
