//! Per-field-type codec of the DSL nodes: decoding from the IR's node encoding, CPython
//! `repr`, and the golden-file JSON of `effects/golden.py:19-37`.
//!
//! The generated `dsl.rs` holds one glue function per class (`Node::decode`,
//! `Node::repr_into`, `Node::golden`) that calls [`Field`] for every field in declaration
//! order. This module implements [`Field`] for every Rust field type the generator emits
//! (`i64`, `bool`, `StrId`, `VarId`, the enums, the hook names, `ValueRef`, `ParamValue`, the
//! node handles, `Option`, `Box<[T]>` and pairs), so the decoder, `repr` and the golden writer
//! are each written once (docs/port/PLAN.md §5.1, §5.7; docs/port/contracts.md, gcg-effects).

use gcg_core::pyjson::Json;
use serde_json::{Map, Value};

use crate::contracts::{
    AbilityNodeId, CondId, ContinuousId, CostId, FilterId, IrError, NodeId, ParamValue, RefId,
    RuleModId, SelId, StepId, StrId, TriggerId, ValueRef, VarId,
};
use crate::dsl::{CLASSES, Node, NodeKind};
use crate::pyrepr::repr_str;

/// A string table split into sorted segments: the base IR's table, then a fragment's
/// additions (PLAN §3.4). Every string occurs once over all segments.
#[derive(Clone, Copy)]
pub(crate) struct Segments<'a> {
    pub parts: [(&'a [Box<str>], u32); 2],
}

impl<'a> Segments<'a> {
    pub(crate) fn one(table: &'a [Box<str>]) -> Self {
        Self {
            parts: [(table, 0), (&[], len_u32(table.len()))],
        }
    }

    pub(crate) fn two(base: &'a [Box<str>], extra: &'a [Box<str>]) -> Self {
        Self {
            parts: [(base, 0), (extra, len_u32(base.len()))],
        }
    }

    pub(crate) fn find(&self, s: &str) -> Option<u32> {
        self.parts.iter().find_map(|(part, offset)| {
            part.binary_search_by(|x| (**x).cmp(s))
                .ok()
                .map(|i| offset + len_u32(i))
        })
    }
}

pub(crate) fn len_u32(n: usize) -> u32 {
    u32::try_from(n).expect("IR tables hold fewer than 2^32 entries")
}

/// Decoding state: the string and variable tables, and the kinds of the nodes decoded so far
/// (a node may reference only earlier nodes, so the IR is a DAG in post-order).
pub(crate) struct Decoder<'a> {
    pub strings: Segments<'a>,
    pub var_names: Segments<'a>,
    pub kinds: Vec<NodeKind>,
    /// JSON path of the node being decoded, for errors.
    pub at: String,
    pub field: &'static str,
}

impl Decoder<'_> {
    pub(crate) fn malformed(&self, detail: impl Into<String>) -> IrError {
        let path = if self.field.is_empty() {
            self.at.clone()
        } else {
            format!("{}.{}", self.at, self.field)
        };
        IrError::Malformed {
            path,
            detail: detail.into(),
        }
    }

    /// Decodes field `name` of a node object.
    pub(crate) fn field<T: Field>(
        &mut self,
        obj: &Map<String, Value>,
        name: &'static str,
    ) -> Result<T, IrError> {
        self.field = name;
        let v = obj
            .get(name)
            .ok_or_else(|| self.malformed("missing field"))?;
        let out = T::decode(self, v);
        if out.is_ok() {
            self.field = "";
        }
        out
    }

    /// A `{"$": id}` reference to an earlier node whose kind is in `kinds`.
    pub(crate) fn node_ref(&self, v: &Value, kinds: &[NodeKind]) -> Result<NodeId, IrError> {
        let raw = match v {
            Value::Object(m) if m.len() == 1 => m.get("$").and_then(Value::as_u64),
            _ => None,
        }
        .ok_or_else(|| self.malformed(format!("expected a node reference, got {v}")))?;
        let index = usize::try_from(raw)
            .ok()
            .filter(|&i| i < self.kinds.len())
            .ok_or_else(|| self.malformed(format!("node {raw} is not an earlier node")))?;
        let kind = self.kinds[index];
        if !kinds.contains(&kind) {
            return Err(self.malformed(format!(
                "node {raw} is a {}, which this field does not admit",
                kind.py_name()
            )));
        }
        Ok(NodeId(len_u32(index)))
    }

    fn string(&self, v: &Value) -> Result<StrId, IrError> {
        let s = v
            .as_str()
            .ok_or_else(|| self.malformed(format!("expected a string, got {v}")))?;
        self.strings
            .find(s)
            .map(StrId)
            .ok_or_else(|| self.malformed(format!("{s:?} is not in the string table")))
    }

    fn var(&self, v: &Value) -> Result<VarId, IrError> {
        let s = v
            .as_str()
            .ok_or_else(|| self.malformed(format!("expected a variable name, got {v}")))?;
        self.var_names
            .find(s)
            .and_then(|i| u16::try_from(i).ok())
            .map(VarId)
            .ok_or_else(|| self.malformed(format!("{s:?} is not in var_names")))
    }

    /// A class without fields: the object holds only `"_"`.
    pub(crate) fn no_fields(&self, obj: &Map<String, Value>) -> Result<(), IrError> {
        self.exact_fields(obj, 0)
    }

    /// The object holds `"_"` and exactly `n` fields (unknown fields fail the load).
    pub(crate) fn exact_fields(&self, obj: &Map<String, Value>, n: usize) -> Result<(), IrError> {
        if obj.len() == n + 1 {
            Ok(())
        } else {
            Err(self.malformed(format!("expected {n} fields, got {}", obj.len() - 1)))
        }
    }
}

/// The read-only tables `repr` and the golden writer need.
#[derive(Clone, Copy)]
pub(crate) struct ReprCx<'a> {
    pub nodes: &'a [Node],
    pub strings: &'a [Box<str>],
    pub var_names: &'a [Box<str>],
}

impl ReprCx<'_> {
    pub(crate) fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id.0 as usize]
    }

    pub(crate) fn str(&self, id: StrId) -> &str {
        &self.strings[id.0 as usize]
    }

    pub(crate) fn repr_of<T: Field>(&self, v: &T) -> String {
        let mut out = String::new();
        v.repr(self, &mut out);
        out
    }
}

/// One field type of the generated node structs.
pub(crate) trait Field: Sized {
    fn decode(d: &mut Decoder<'_>, v: &Value) -> Result<Self, IrError>;
    /// CPython `repr` of the value.
    fn repr(&self, cx: &ReprCx<'_>, out: &mut String);
    /// `golden.node_to_json` of the value (`effects/golden.py:19-37`).
    fn golden(&self, cx: &ReprCx<'_>) -> Json;
}

impl Field for i64 {
    fn decode(d: &mut Decoder<'_>, v: &Value) -> Result<Self, IrError> {
        v.as_i64()
            .ok_or_else(|| d.malformed(format!("expected an int, got {v}")))
    }

    fn repr(&self, _: &ReprCx<'_>, out: &mut String) {
        out.push_str(&self.to_string());
    }

    fn golden(&self, _: &ReprCx<'_>) -> Json {
        Json::from(*self)
    }
}

impl Field for bool {
    fn decode(d: &mut Decoder<'_>, v: &Value) -> Result<Self, IrError> {
        v.as_bool()
            .ok_or_else(|| d.malformed(format!("expected a bool, got {v}")))
    }

    fn repr(&self, _: &ReprCx<'_>, out: &mut String) {
        out.push_str(if *self { "True" } else { "False" });
    }

    fn golden(&self, _: &ReprCx<'_>) -> Json {
        Json::Bool(*self)
    }
}

impl Field for StrId {
    fn decode(d: &mut Decoder<'_>, v: &Value) -> Result<Self, IrError> {
        d.string(v)
    }

    fn repr(&self, cx: &ReprCx<'_>, out: &mut String) {
        out.push_str(&repr_str(cx.str(*self)));
    }

    fn golden(&self, cx: &ReprCx<'_>) -> Json {
        Json::from(cx.str(*self))
    }
}

impl Field for VarId {
    fn decode(d: &mut Decoder<'_>, v: &Value) -> Result<Self, IrError> {
        d.var(v)
    }

    fn repr(&self, cx: &ReprCx<'_>, out: &mut String) {
        out.push_str(&repr_str(&cx.var_names[usize::from(self.0)]));
    }

    fn golden(&self, cx: &ReprCx<'_>) -> Json {
        Json::from(&*cx.var_names[usize::from(self.0)])
    }
}

impl Field for ValueRef {
    fn decode(d: &mut Decoder<'_>, v: &Value) -> Result<Self, IrError> {
        if v.is_number() {
            return i64::decode(d, v).map(ValueRef::Lit);
        }
        d.node_ref(v, NodeKind::VALUE).map(ValueRef::Node)
    }

    fn repr(&self, cx: &ReprCx<'_>, out: &mut String) {
        match self {
            Self::Lit(n) => n.repr(cx, out),
            Self::Node(id) => id.repr(cx, out),
        }
    }

    fn golden(&self, cx: &ReprCx<'_>) -> Json {
        match self {
            Self::Lit(n) => Json::from(*n),
            Self::Node(id) => id.golden(cx),
        }
    }
}

/// A hook parameter value: an int, a string, or a `Cond`/`Sel` node (Inv 04 §10.1).
impl Field for ParamValue {
    fn decode(d: &mut Decoder<'_>, v: &Value) -> Result<Self, IrError> {
        match v {
            Value::Number(_) => i64::decode(d, v).map(ParamValue::Int),
            Value::String(_) => d.string(v).map(ParamValue::Str),
            _ => {
                let mut kinds = NodeKind::COND.to_vec();
                kinds.push(NodeKind::Sel);
                d.node_ref(v, &kinds).map(ParamValue::Node)
            }
        }
    }

    fn repr(&self, cx: &ReprCx<'_>, out: &mut String) {
        match self {
            Self::Int(n) => n.repr(cx, out),
            Self::Str(s) => s.repr(cx, out),
            Self::Node(id) => id.repr(cx, out),
        }
    }

    fn golden(&self, cx: &ReprCx<'_>) -> Json {
        match self {
            Self::Int(n) => Json::from(*n),
            Self::Str(s) => s.golden(cx),
            Self::Node(id) => id.golden(cx),
        }
    }
}

/// An `Instr` (`effects/program.py:76-87`) or any node: the arena handle itself.
impl Field for NodeId {
    fn decode(d: &mut Decoder<'_>, v: &Value) -> Result<Self, IrError> {
        d.node_ref(v, NodeKind::INSTR)
    }

    fn repr(&self, cx: &ReprCx<'_>, out: &mut String) {
        cx.node(*self).repr_into(cx, out);
    }

    fn golden(&self, cx: &ReprCx<'_>) -> Json {
        cx.node(*self).golden(cx)
    }
}

/// The node handles of `crate::contracts`, each checked against the kinds of its alias.
macro_rules! handle_field {
    ($($handle:ident => $kinds:expr),* $(,)?) => {$(
        impl Field for $handle {
            fn decode(d: &mut Decoder<'_>, v: &Value) -> Result<Self, IrError> {
                d.node_ref(v, $kinds).map($handle)
            }

            fn repr(&self, cx: &ReprCx<'_>, out: &mut String) {
                self.0.repr(cx, out);
            }

            fn golden(&self, cx: &ReprCx<'_>) -> Json {
                self.0.golden(cx)
            }
        }
    )*};
}

handle_field!(
    FilterId => NodeKind::FILTER,
    RefId => NodeKind::REF,
    SelId => &[NodeKind::Sel],
    CondId => NodeKind::COND,
    StepId => NodeKind::STEP,
    ContinuousId => NodeKind::CONTINUOUS,
    TriggerId => &[NodeKind::Trigger],
    RuleModId => &[NodeKind::RuleMod],
    CostId => NodeKind::COST,
    AbilityNodeId => NodeKind::ABILITY,
);

impl<T: Field> Field for Option<T> {
    fn decode(d: &mut Decoder<'_>, v: &Value) -> Result<Self, IrError> {
        if v.is_null() {
            Ok(None)
        } else {
            T::decode(d, v).map(Some)
        }
    }

    fn repr(&self, cx: &ReprCx<'_>, out: &mut String) {
        match self {
            None => out.push_str("None"),
            Some(x) => x.repr(cx, out),
        }
    }

    fn golden(&self, cx: &ReprCx<'_>) -> Json {
        self.as_ref().map_or(Json::Null, |x| x.golden(cx))
    }
}

impl<T: Field> Field for Box<[T]> {
    fn decode(d: &mut Decoder<'_>, v: &Value) -> Result<Self, IrError> {
        let items = v
            .as_array()
            .ok_or_else(|| d.malformed(format!("expected an array, got {v}")))?;
        items.iter().map(|x| T::decode(d, x)).collect()
    }

    /// `()`, `(x,)`, `(x, y)`.
    fn repr(&self, cx: &ReprCx<'_>, out: &mut String) {
        out.push('(');
        for (i, x) in self.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            x.repr(cx, out);
        }
        if self.len() == 1 {
            out.push(',');
        }
        out.push(')');
    }

    fn golden(&self, cx: &ReprCx<'_>) -> Json {
        Json::Array(self.iter().map(|x| x.golden(cx)).collect())
    }
}

impl<A: Field, B: Field> Field for (A, B) {
    fn decode(d: &mut Decoder<'_>, v: &Value) -> Result<Self, IrError> {
        match v.as_array().map(Vec::as_slice) {
            Some([a, b]) => Ok((A::decode(d, a)?, B::decode(d, b)?)),
            _ => Err(d.malformed(format!("expected a pair, got {v}"))),
        }
    }

    fn repr(&self, cx: &ReprCx<'_>, out: &mut String) {
        out.push('(');
        self.0.repr(cx, out);
        out.push_str(", ");
        self.1.repr(cx, out);
        out.push(')');
    }

    fn golden(&self, cx: &ReprCx<'_>) -> Json {
        Json::Array(vec![self.0.golden(cx), self.1.golden(cx)])
    }
}

/// `Field` for the generated `StrEnum`s: decoded from the value string, `repr`
/// `<Cls.MEMBER: 'value'>`, golden the value string (`golden.py:31-32`).
macro_rules! enum_field {
    ($($e:ident),* $(,)?) => {$(
        impl $crate::codec::Field for $e {
            fn decode(
                d: &mut $crate::codec::Decoder<'_>,
                v: &serde_json::Value,
            ) -> Result<Self, $crate::contracts::IrError> {
                v.as_str()
                    .and_then($e::from_value)
                    .ok_or_else(|| d.malformed(format!("not a {} value: {v}", $e::PY_CLASS)))
            }

            fn repr(&self, _: &$crate::codec::ReprCx<'_>, out: &mut String) {
                out.push('<');
                out.push_str($e::PY_CLASS);
                out.push('.');
                out.push_str(self.py_name());
                out.push_str(": ");
                out.push_str(&$crate::pyrepr::repr_str(self.as_str()));
                out.push('>');
            }

            fn golden(&self, _: &$crate::codec::ReprCx<'_>) -> gcg_core::pyjson::Json {
                gcg_core::pyjson::Json::from(self.as_str())
            }
        }
    )*};
}
pub(crate) use enum_field;

/// `Field` for the generated hook-name enums: the `Custom*.name` field is a `str` in Python,
/// so `repr` and golden print the hook name; an unregistered name is
/// [`IrError::UnknownHook`] (W-69).
macro_rules! hook_field {
    ($($h:ident => $kind:literal),* $(,)?) => {$(
        impl $crate::codec::Field for $h {
            fn decode(
                _: &mut $crate::codec::Decoder<'_>,
                v: &serde_json::Value,
            ) -> Result<Self, $crate::contracts::IrError> {
                let name = v.as_str().unwrap_or_default();
                $h::from_name(name).ok_or_else(|| $crate::contracts::IrError::UnknownHook {
                    kind: $kind,
                    name: v.as_str().map_or_else(|| v.to_string(), str::to_owned),
                    card_number: None,
                })
            }

            fn repr(&self, _: &$crate::codec::ReprCx<'_>, out: &mut String) {
                out.push_str(&$crate::pyrepr::repr_str(self.as_str()));
            }

            fn golden(&self, _: &$crate::codec::ReprCx<'_>) -> gcg_core::pyjson::Json {
                gcg_core::pyjson::Json::from(self.as_str())
            }
        }
    )*};
}
pub(crate) use hook_field;

/// Builds the golden object of one node: `{"_": Class, field: value, ...}` without the fields
/// equal to their default (`golden.py:20-30`). Equality with the default is compared on the
/// CPython `repr`, which the exporter proves equal to Python `==` on the node universe.
pub(crate) struct Golden<'a, 'b> {
    kind: NodeKind,
    cx: &'b ReprCx<'a>,
    out: Json,
}

impl<'a, 'b> Golden<'a, 'b> {
    pub(crate) fn new(kind: NodeKind, cx: &'b ReprCx<'a>) -> Self {
        let mut out = Json::object();
        out.set("_", kind.py_name());
        Self { kind, cx, out }
    }

    /// Field `index` of the class (declaration order).
    pub(crate) fn field<T: Field>(&mut self, index: usize, v: &T) {
        let schema = &CLASSES[self.kind as usize].fields[index];
        if let Some(default) = schema.default_repr
            && self.cx.repr_of(v) == default
        {
            return;
        }
        self.out.set(schema.name, v.golden(self.cx));
    }

    pub(crate) fn finish(self) -> Json {
        self.out
    }
}
