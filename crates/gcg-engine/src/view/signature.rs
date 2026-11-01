//! The view signature (`tests/paranoid.py:24-39`) in the JSON form of
//! `port/oracle/traces.py` `view_signature_json`, which the `views` trace set records and
//! `gcg-diff` compares (its `paranoid.view_signature` probe).

use gcg_core::pyjson::{Json, Key};
use gcg_effects::{Kw, NodeId, Registry};

use crate::{Derived, GameState, RuleKey};

/// `tests/paranoid.py:24-39` `view_signature(dv)` as `view_signature_json(st, dv)`
/// (`port/oracle/traces.py`): `ap`, `hp`, `kw` (keyword values), `traits`, `rules` (each
/// `[rule node, controller, source, key, aux]`, a lasting key `(-1, effect_key, serial)`
/// written `[-1, content id, serial]`), `cost_mod`, `level_mod`, `linked` (sorted) and
/// `abilities` (aid lists). Dict keys are `str(uid)`; Python writes them in insertion order
/// and the trace sorts them, so they are written here in ascending uid order (dict equality
/// ignores the order).
#[must_use]
pub fn signature(st: &GameState, dv: &Derived) -> Json {
    let reg = &**st.registry();
    let n = dv.len();
    let uids = || (0..n).filter_map(|i| crate::Uid::try_from(i).ok());
    let mut ap = Json::object();
    let mut hp = Json::object();
    let mut kw = Json::object();
    let mut traits = Json::object();
    let mut rules = Json::object();
    let mut cost_mod = Json::object();
    let mut level_mod = Json::object();
    let mut abilities = Json::object();
    for u in uids() {
        let key = || Key::Str(u.to_string());
        if let (Some(a), Some(h), Some(k), Some((t, granted))) =
            (dv.ap_get(u), dv.hp_get(u), dv.kw_get(u), dv.traits_get(u))
        {
            ap.set(key(), a);
            hp.set(key(), h);
            let mut kws = Json::object();
            for k_ in Kw::ALL {
                if let Some(amount) = k.get(k_) {
                    kws.set(k_.as_str(), amount);
                }
            }
            kw.set(key(), kws);
            let names: Vec<Json> = dv
                .printed_traits(t)
                .iter()
                .map(|s| Json::from(&**s))
                .chain(granted.iter().map(|&s| Json::from(reg.str(s))))
                .collect();
            traits.set(key(), Json::Array(names));
        }
        let rs = dv.rules_slice(u);
        if !rs.is_empty() {
            let list = rs
                .iter()
                .map(|r| {
                    let key = match r.key {
                        RuleKey::Constant { aid, host } => {
                            Json::Array(vec![Json::from(aid.0), Json::from(host)])
                        }
                        RuleKey::Lasting { effect, serial } => Json::Array(vec![
                            Json::from(-1),
                            Json::from(effect.content_id(reg).as_str()),
                            Json::from(serial),
                        ]),
                    };
                    Json::Array(vec![
                        node_json(reg, r.rule.node()),
                        Json::from(r.controller),
                        Json::from(r.source),
                        key,
                        Json::from(r.aux),
                    ])
                })
                .collect();
            rules.set(key(), Json::Array(list));
        }
        if let Some(c) = dv.cost_mod_get(u) {
            cost_mod.set(key(), c);
        }
        if let Some(l) = dv.level_mod_get(u) {
            level_mod.set(key(), l);
        }
        let ab = dv.abilities_slice(u);
        if !ab.is_empty() {
            abilities.set(
                key(),
                Json::Array(ab.iter().map(|a| Json::from(a.0)).collect()),
            );
        }
    }
    let linked: Vec<Json> = dv.linked_sorted().map(Json::from).collect();
    let mut out = Json::object();
    out.set("ap", ap);
    out.set("hp", hp);
    out.set("kw", kw);
    out.set("traits", traits);
    out.set("rules", rules);
    out.set("cost_mod", cost_mod);
    out.set("level_mod", level_mod);
    out.set("linked", Json::Array(linked));
    out.set("abilities", abilities);
    out
}

/// `node_json(x)` (`port/oracle/traces.py`) of an arena node: `{"_": class, field: value,
/// ...}` with every field in declaration order, nested nodes inline, enums as their values
/// and tuples as arrays. It is read off the node's CPython `repr` (`Registry::repr`, PLAN
/// §5.7), which prints every field of a dataclass in that order; a text the reader does not
/// accept is returned as the string itself, so a mismatch shows in any comparison.
#[must_use]
pub(crate) fn node_json(reg: &Registry, id: NodeId) -> Json {
    let text = reg.repr(id);
    let mut r = ReprReader { s: &text, pos: 0 };
    match r.value() {
        Some(v) if r.pos == text.len() => v,
        _ => Json::Str(text),
    }
}

/// The reader on a `repr` text, as `serde_json` (tests).
#[cfg(test)]
pub(super) fn read_repr(text: &str) -> Option<serde_json::Value> {
    use gcg_core::pyjson::{DumpOptions, dumps};
    let mut r = ReprReader { s: text, pos: 0 };
    let v = r.value().filter(|_| r.pos == text.len())?;
    serde_json::from_str(&dumps(&v, DumpOptions::COMPACT_SORTED_ASCII)).ok()
}

/// A reader of the `repr` grammar of the DSL universe (Inv 04 §10.1; PLAN §5.7):
/// `Name(f=v, ...)`, `<Enum.MEMBER: 'value'>`, `()`/`(x,)`/`(x, y)`, quoted strings with
/// CPython's escapes, ints, `True`, `False` and `None`.
struct ReprReader<'a> {
    s: &'a str,
    pos: usize,
}

impl ReprReader<'_> {
    fn rest(&self) -> &str {
        &self.s[self.pos..]
    }

    fn eat(&mut self, lit: &str) -> bool {
        if self.rest().starts_with(lit) {
            self.pos += lit.len();
            true
        } else {
            false
        }
    }

    fn value(&mut self) -> Option<Json> {
        let c = self.rest().chars().next()?;
        match c {
            '(' => self.tuple(),
            '<' => self.enum_value(),
            '\'' | '"' => self.string().map(Json::Str),
            '-' | '0'..='9' => self.int(),
            c if c.is_ascii_alphabetic() || c == '_' => self.name(),
            _ => None,
        }
    }

    fn tuple(&mut self) -> Option<Json> {
        self.pos += 1;
        let mut items = Vec::new();
        if self.eat(")") {
            return Some(Json::Array(items));
        }
        loop {
            items.push(self.value()?);
            if self.eat(")") {
                return Some(Json::Array(items));
            }
            if self.eat(",)") {
                return (items.len() == 1).then_some(Json::Array(items));
            }
            if !self.eat(", ") {
                return None;
            }
        }
    }

    /// `<Cls.MEMBER: 'value'>`: the value string.
    fn enum_value(&mut self) -> Option<Json> {
        let sep = self.rest().find(": ")?;
        self.pos += sep + 2;
        let v = self.string()?;
        self.eat(">").then_some(Json::Str(v))
    }

    fn int(&mut self) -> Option<Json> {
        let rest = self.rest();
        let len = rest
            .char_indices()
            .find(|&(i, c)| !(c.is_ascii_digit() || (i == 0 && c == '-')))
            .map_or(rest.len(), |(i, _)| i);
        let n: i128 = rest[..len].parse().ok()?;
        self.pos += len;
        Some(Json::Int(n))
    }

    fn name(&mut self) -> Option<Json> {
        let rest = self.rest();
        let len = rest
            .char_indices()
            .find(|&(_, c)| !(c.is_ascii_alphanumeric() || c == '_'))
            .map_or(rest.len(), |(i, _)| i);
        let name = rest[..len].to_owned();
        self.pos += len;
        if !self.eat("(") {
            return match name.as_str() {
                "None" => Some(Json::Null),
                "True" => Some(Json::Bool(true)),
                "False" => Some(Json::Bool(false)),
                _ => None,
            };
        }
        let mut obj = Json::object();
        obj.set("_", name);
        if self.eat(")") {
            return Some(obj);
        }
        loop {
            let rest = self.rest();
            let eq = rest.find('=')?;
            let field = rest[..eq].to_owned();
            if field.is_empty() || !field.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                return None;
            }
            self.pos += eq + 1;
            let v = self.value()?;
            obj.set(field, v);
            if self.eat(")") {
                return Some(obj);
            }
            if !self.eat(", ") {
                return None;
            }
        }
    }

    /// A CPython string literal as `repr` writes it: `\\`, `\t`, `\n`, `\r`, the quote and
    /// `\xNN` escapes; every other character is itself.
    fn string(&mut self) -> Option<String> {
        let quote = self.rest().chars().next()?;
        self.pos += quote.len_utf8();
        let mut out = String::new();
        let mut chars = self.rest().char_indices();
        while let Some((i, c)) = chars.next() {
            match c {
                '\\' => {
                    let (_, e) = chars.next()?;
                    match e {
                        '\\' => out.push('\\'),
                        't' => out.push('\t'),
                        'n' => out.push('\n'),
                        'r' => out.push('\r'),
                        '\'' => out.push('\''),
                        '"' => out.push('"'),
                        'x' => {
                            let (_, h) = chars.next()?;
                            let (_, l) = chars.next()?;
                            let code = u32::from_str_radix(&format!("{h}{l}"), 16).ok()?;
                            out.push(char::from_u32(code)?);
                        }
                        _ => return None,
                    }
                }
                c if c == quote => {
                    self.pos += i + c.len_utf8();
                    return Some(out);
                }
                c => out.push(c),
            }
        }
        None
    }
}
