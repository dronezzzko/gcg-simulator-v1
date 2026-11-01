//! The IR loader (docs/port/PLAN.md §3.3-§3.4): JSON text to typed [`Tables`].
//!
//! The IR is produced by `port/oracle/export_ir.py` from the built Python registry
//! (`effects/registry.py:323-340` plus the system programs of `engine/interp.py:1581-1594`).
//! The loader checks the IR version, compares the IR's `schema`, `enums` and `hooks` with the
//! generated `dsl::{CLASSES, ENUMS}` and hook enums, decodes the node arena (each node may
//! reference only earlier nodes, and every handle is checked against its alias kinds), and
//! checks the cross-table invariants the engine relies on. An unknown class or hook fails with
//! the number of the first card whose script contains it (W-69).

use gcg_data::DefId;
use serde_json::{Map, Value};

use crate::codec::{Decoder, Segments, len_u32};
use crate::contracts::{
    AbilityEntry, AbilityId, AbilityNodeId, CardEntry, ContRef, ContentId, ContinuousId, EvSet,
    FilterId, FilterSet, HookParams, IrError, KeywordProgram, KwAmounts, NodeId, Program,
    ProgramId, RuleModId, SUPPORTED_IR_VERSION, ScriptSource, StrId, SystemProgram,
};
use crate::dsl::{
    CLASSES, CondHook, ENUMS, Ev, FilterHook, Kw, Node, NodeKind, StepHook, ValueHook,
};
use crate::hooks::{ParamCx, hook_params};
use crate::view::{View, canonical_cont};

/// Per-card text the golden files need besides the registry entry (`golden.py:41`).
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct CardText {
    pub card_number: Box<str>,
    pub name: Box<str>,
    /// `text_hash(cdef.effect)` (`effects/text.py:94-95`).
    pub text_hash: Box<str>,
}

/// The decoded IR: the `Registry` tables of `crate::contracts` plus [`CardText`].
#[derive(Clone)]
pub(crate) struct Tables {
    pub strings: Box<[Box<str>]>,
    pub var_names: Box<[Box<str>]>,
    pub printable_non_ascii: Box<[char]>,
    pub nodes: Box<[Node]>,
    pub node_content_ids: Box<[ContentId]>,
    pub programs: Box<[Program]>,
    pub keyword_programs: [ProgramId; 3],
    pub system_programs: [ProgramId; 3],
    pub abilities: Box<[AbilityEntry]>,
    pub cards: Box<[CardEntry]>,
    pub continuous: Box<[(ContinuousId, ContRef)]>,
    pub filter_sets: Box<[FilterSet]>,
    pub rule_mod_reprs: Box<[(RuleModId, Box<str>)]>,
    pub aliases: Box<[(DefId, Box<[StrId]>)]>,
    pub always_events: EvSet,
    pub hp_reduction_possible: bool,
    pub token_keys: Box<[(StrId, DefId)]>,
    pub unit_variants: Box<[(DefId, DefId)]>,
    pub hook_params: Box<[(NodeId, HookParams)]>,
    pub card_texts: Box<[CardText]>,
}

impl Tables {
    pub(crate) fn view(&self) -> View<'_> {
        View {
            strings: &self.strings,
            var_names: &self.var_names,
            nodes: &self.nodes,
            node_content_ids: &self.node_content_ids,
            programs: &self.programs,
            keyword_programs: &self.keyword_programs,
            system_programs: &self.system_programs,
            abilities: &self.abilities,
            cards: &self.cards,
            continuous: &self.continuous,
            filter_sets: &self.filter_sets,
            rule_mod_reprs: &self.rule_mod_reprs,
            aliases: &self.aliases,
            hook_params: &self.hook_params,
        }
    }

    /// Decodes and checks an IR text (PLAN §3.3-§3.4).
    pub(crate) fn decode(ir_json: &str) -> Result<Self, IrError> {
        let ir: Value = serde_json::from_str(ir_json).map_err(|e| IrError::Json {
            message: e.to_string(),
        })?;
        let version = key(&ir, "ir_version", "$")?;
        let found = version.as_i64().unwrap_or(-1);
        if u32::try_from(found).ok() != Some(SUPPORTED_IR_VERSION) {
            return Err(IrError::Version { found });
        }
        check_schema(&ir)?;
        check_hooks(&ir)?;
        let ArenaTables {
            printable,
            strings,
            var_names,
            nodes,
            kinds,
        } = decode_arena(&ir)?;
        let mut b = Builder {
            strings: Segments::one(&strings),
            var_names: Segments::one(&var_names),
            all_strings: &strings,
            nodes: &nodes,
            kinds: &kinds,
            n_programs: 0,
            n_abilities: 0,
        };
        let node_content_ids = b.content_ids(
            key(&ir, "node_content_ids", "$")?,
            "$.node_content_ids",
            nodes.len(),
        )?;
        let programs = b.programs(
            arr(key(&ir, "programs", "$")?, "$.programs")?,
            "$.programs",
            0,
        )?;
        b.n_programs = programs.len();
        let keyword_programs = b.program_table(
            &ir,
            "keyword_programs",
            &KeywordProgram::ALL.map(KeywordProgram::as_str),
        )?;
        let system_programs = b.program_table(
            &ir,
            "system_programs",
            &SystemProgram::ALL.map(SystemProgram::as_str),
        )?;
        let abilities = b.abilities(
            arr(key(&ir, "abilities", "$")?, "$.abilities")?,
            "$.abilities",
            0,
        )?;
        b.n_abilities = abilities.len();
        let (cards, card_texts) = b.cards(arr(key(&ir, "cards", "$")?, "$.cards")?, "$.cards")?;
        for (i, c) in cards.iter().enumerate() {
            if usize::from(c.def_id.0) != i {
                return Err(malformed(
                    &format!("$.cards[{i}].def_id"),
                    "def_ids must be 0..n in order",
                ));
            }
        }
        let continuous = b.continuous(key(&ir, "continuous", "$")?, "$.continuous", 0)?;
        let filter_sets = b.filter_sets(key(&ir, "filter_sets", "$")?, "$.filter_sets")?;
        let rule_mod_reprs = b.rule_mods(key(&ir, "rule_mods", "$")?, "$.rule_mods", 0)?;
        let aliases = b.aliases(key(&ir, "aliases", "$")?, cards.len())?;
        let always_events = ev_set(key(&ir, "always_events", "$")?, "$.always_events")?;
        let hp_reduction_possible = flag(
            key(&ir, "hp_reduction_possible", "$")?,
            "$.hp_reduction_possible",
        )?;
        let token_keys = b.token_keys(key(&ir, "token_keys", "$")?, cards.len())?;
        let unit_variants = def_pairs(
            key(&ir, "unit_variants", "$")?,
            "$.unit_variants",
            cards.len(),
        )?;
        let hook_params = b.hook_params(0, "$.nodes")?;
        check_ability_defs(&abilities, cards.len())?;
        Ok(Self {
            strings,
            var_names,
            printable_non_ascii: printable,
            nodes,
            node_content_ids,
            programs,
            keyword_programs,
            system_programs,
            abilities,
            cards,
            continuous,
            filter_sets,
            rule_mod_reprs,
            aliases,
            always_events,
            hp_reduction_possible,
            token_keys,
            unit_variants,
            hook_params,
            card_texts,
        })
    }

    /// Applies an IR fragment of a script override (PLAN §3.4 "Test overrides"; the format
    /// of `export_ir.build_fragment`): every value is applied verbatim, nothing is
    /// recomputed. New strings, variables, nodes, programs and abilities are numbered after
    /// the existing ones; aids must be Python's.
    pub(crate) fn with_fragment(&self, frag: &Value) -> Result<Self, IrError> {
        let version = key(frag, "fragment_version", "$")?.as_i64();
        if version != Some(FRAGMENT_VERSION) {
            return Err(malformed(
                "$.fragment_version",
                format!("expected {FRAGMENT_VERSION}, got {version:?}"),
            ));
        }
        let base = text(key(frag, "base_fingerprint", "$")?, "$.base_fingerprint")?;
        let found = self.view().fingerprint();
        if base != found {
            return Err(IrError::DataMismatch {
                which: "base_fingerprint",
                expected: base.to_owned(),
                found,
            });
        }
        let mut printable = self.printable_non_ascii.to_vec();
        for ch in decode_printable(
            key(frag, "printable_non_ascii", "$")?,
            "$.printable_non_ascii",
        )?
        .iter()
        {
            if !printable.contains(ch) {
                printable.push(*ch);
            }
        }
        let extra_strings = decode_table(frag, "strings", &printable)?;
        let extra_vars = decode_table(frag, "var_names", &printable)?;
        for (table, extra, what) in [
            (&self.strings, &extra_strings, "strings"),
            (&self.var_names, &extra_vars, "var_names"),
        ] {
            if let Some(dup) = extra.iter().find(|s| table.binary_search(s).is_ok()) {
                return Err(malformed(
                    &format!("$.{what}"),
                    format!("{dup:?} is already in the base table"),
                ));
            }
        }
        let strings: Box<[Box<str>]> = self
            .strings
            .iter()
            .chain(extra_strings.iter())
            .cloned()
            .collect();
        let var_names: Box<[Box<str>]> = self
            .var_names
            .iter()
            .chain(extra_vars.iter())
            .cloned()
            .collect();
        if var_names.len() > usize::from(u16::MAX) {
            return Err(malformed("$.var_names", "more than 65535 names"));
        }
        let str_segs = Segments::two(&self.strings, &extra_strings);
        let var_segs = Segments::two(&self.var_names, &extra_vars);
        let mut arena = Arena::new(str_segs, var_segs);
        arena.nodes = self.nodes.to_vec();
        arena.kinds = self.nodes.iter().map(Node::kind).collect();
        let n_base_nodes = self.nodes.len();
        arena.decode_nodes(frag, arr(key(frag, "nodes", "$")?, "$.nodes")?, "$.nodes")?;
        let Arena { nodes, kinds, .. } = arena;
        let nodes = nodes.into_boxed_slice();
        let mut b = Builder {
            strings: str_segs,
            var_names: var_segs,
            all_strings: &strings,
            nodes: &nodes,
            kinds: &kinds,
            n_programs: self.programs.len(),
            n_abilities: self.abilities.len(),
        };
        let mut node_content_ids = self.node_content_ids.to_vec();
        node_content_ids.extend(
            b.content_ids(
                key(frag, "node_content_ids", "$")?,
                "$.node_content_ids",
                nodes.len() - n_base_nodes,
            )?
            .iter(),
        );
        let mut programs = self.programs.to_vec();
        programs.extend(
            b.programs(
                arr(key(frag, "programs", "$")?, "$.programs")?,
                "$.programs",
                self.programs.len(),
            )?
            .into_vec(),
        );
        b.n_programs = programs.len();
        let mut abilities = self.abilities.to_vec();
        abilities.extend(
            b.abilities(
                arr(key(frag, "abilities", "$")?, "$.abilities")?,
                "$.abilities",
                self.abilities.len(),
            )?
            .into_vec(),
        );
        b.n_abilities = abilities.len();
        check_ability_defs(&abilities[self.abilities.len()..], self.cards.len())?;
        let mut cards = self.cards.to_vec();
        let mut card_texts = self.card_texts.to_vec();
        let (new_cards, new_texts) =
            b.cards(arr(key(frag, "cards", "$")?, "$.cards")?, "$.cards")?;
        for (i, (c, t)) in new_cards
            .into_vec()
            .into_iter()
            .zip(new_texts.into_vec())
            .enumerate()
        {
            let at = usize::from(c.def_id.0);
            if at >= cards.len() {
                return Err(malformed(
                    &format!("$.cards[{i}].def_id"),
                    "not a definition of the base",
                ));
            }
            cards[at] = c;
            if at < card_texts.len() {
                card_texts[at] = t;
            }
        }
        for (i, item) in arr(key(frag, "events", "$")?, "$.events")?
            .iter()
            .enumerate()
        {
            let at = format!("$.events[{i}]");
            let (def, evs) = pair(item, &at)?;
            let def = def_id(def, &at, cards.len())?;
            cards[usize::from(def.0)].events = ev_set(evs, &at)?;
        }
        for (i, item) in arr(key(frag, "printed_keywords", "$")?, "$.printed_keywords")?
            .iter()
            .enumerate()
        {
            let at = format!("$.printed_keywords[{i}]");
            let (def, kws) = pair(item, &at)?;
            let def = def_id(def, &at, cards.len())?;
            cards[usize::from(def.0)].printed_keywords = kw_amounts(kws, &at)?;
        }
        let mut aliases = self.aliases.to_vec();
        for (def, names) in b
            .aliases(key(frag, "aliases", "$")?, cards.len())?
            .into_vec()
        {
            match aliases.binary_search_by_key(&def, |(d, _)| *d) {
                Ok(i) => aliases[i].1 = names,
                Err(i) => aliases.insert(i, (def, names)),
            }
        }
        let mut continuous = self.continuous.to_vec();
        continuous.extend(
            b.continuous(key(frag, "continuous", "$")?, "$.continuous", n_base_nodes)?
                .into_vec(),
        );
        let mut filter_sets = self.filter_sets.to_vec();
        filter_sets.extend(
            b.filter_sets(key(frag, "filter_sets", "$")?, "$.filter_sets")?
                .into_vec(),
        );
        if filter_sets.len() > usize::from(u16::MAX) {
            return Err(malformed("$.filter_sets", "more than 65535 filter sets"));
        }
        let mut rule_mod_reprs = self.rule_mod_reprs.to_vec();
        rule_mod_reprs.extend(
            b.rule_mods(key(frag, "rule_mods", "$")?, "$.rule_mods", n_base_nodes)?
                .into_vec(),
        );
        let mut hook_params = self.hook_params.to_vec();
        hook_params.extend(b.hook_params(n_base_nodes, "$.nodes")?.into_vec());
        let always_events = ev_set(key(frag, "always_events", "$")?, "$.always_events")?;
        let hp_reduction_possible = flag(
            key(frag, "hp_reduction_possible", "$")?,
            "$.hp_reduction_possible",
        )?;
        Ok(Self {
            strings,
            var_names,
            printable_non_ascii: printable.into_boxed_slice(),
            nodes,
            node_content_ids: node_content_ids.into_boxed_slice(),
            programs: programs.into_boxed_slice(),
            keyword_programs: self.keyword_programs,
            system_programs: self.system_programs,
            abilities: abilities.into_boxed_slice(),
            cards: cards.into_boxed_slice(),
            continuous: continuous.into_boxed_slice(),
            filter_sets: filter_sets.into_boxed_slice(),
            rule_mod_reprs: rule_mod_reprs.into_boxed_slice(),
            aliases: aliases.into_boxed_slice(),
            always_events,
            hp_reduction_possible,
            token_keys: self.token_keys.clone(),
            unit_variants: self.unit_variants.clone(),
            hook_params: hook_params.into_boxed_slice(),
            card_texts: card_texts.into_boxed_slice(),
        })
    }
}

/// The only fragment format this build reads (`export_ir.FRAGMENT_VERSION`).
pub(crate) const FRAGMENT_VERSION: i64 = 1;

/// Every ability belongs to a definition of the registry.
fn check_ability_defs(abilities: &[AbilityEntry], n_defs: usize) -> Result<(), IrError> {
    match abilities.iter().find(|a| usize::from(a.def_id.0) >= n_defs) {
        Some(a) => Err(malformed(
            &format!("$.abilities[{}].def_id", a.aid.0),
            format!("{} is not a def_id", a.def_id.0),
        )),
        None => Ok(()),
    }
}

// ---------------------------------------------------------------------------------------------
// JSON helpers

pub(crate) fn malformed(path: &str, detail: impl Into<String>) -> IrError {
    IrError::Malformed {
        path: path.to_owned(),
        detail: detail.into(),
    }
}

fn key<'a>(v: &'a Value, k: &str, at: &str) -> Result<&'a Value, IrError> {
    v.get(k)
        .ok_or_else(|| malformed(&format!("{at}.{k}"), "missing"))
}

fn arr<'a>(v: &'a Value, at: &str) -> Result<&'a [Value], IrError> {
    v.as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| malformed(at, "expected an array"))
}

fn obj<'a>(v: &'a Value, at: &str) -> Result<&'a Map<String, Value>, IrError> {
    v.as_object()
        .ok_or_else(|| malformed(at, "expected an object"))
}

fn text<'a>(v: &'a Value, at: &str) -> Result<&'a str, IrError> {
    v.as_str().ok_or_else(|| malformed(at, "expected a string"))
}

fn int(v: &Value, at: &str) -> Result<i64, IrError> {
    v.as_i64().ok_or_else(|| malformed(at, "expected an int"))
}

fn flag(v: &Value, at: &str) -> Result<bool, IrError> {
    v.as_bool().ok_or_else(|| malformed(at, "expected a bool"))
}

fn pair<'a>(v: &'a Value, at: &str) -> Result<(&'a Value, &'a Value), IrError> {
    match arr(v, at)? {
        [a, b] => Ok((a, b)),
        _ => Err(malformed(at, "expected a pair")),
    }
}

fn def_id(v: &Value, at: &str, n_defs: usize) -> Result<DefId, IrError> {
    let i = int(v, at)?;
    usize::try_from(i)
        .ok()
        .filter(|&i| i < n_defs)
        .and_then(|i| u16::try_from(i).ok())
        .map(DefId)
        .ok_or_else(|| malformed(at, format!("{i} is not a def_id")))
}

fn ev_set(v: &Value, at: &str) -> Result<EvSet, IrError> {
    let mut set = EvSet::EMPTY;
    for (i, e) in arr(v, at)?.iter().enumerate() {
        let s = text(e, &format!("{at}[{i}]"))?;
        let ev = Ev::from_value(s)
            .ok_or_else(|| malformed(&format!("{at}[{i}]"), format!("{s:?} is not an Ev value")))?;
        set = set.with(ev);
    }
    Ok(set)
}

fn kw_amounts(v: &Value, at: &str) -> Result<KwAmounts, IrError> {
    let mut out = KwAmounts::default();
    for (i, item) in arr(v, at)?.iter().enumerate() {
        let at = format!("{at}[{i}]");
        let (kw, n) = pair(item, &at)?;
        let kw = text(kw, &at)?;
        let kw = Kw::from_value(kw)
            .ok_or_else(|| malformed(&at, format!("{kw:?} is not a Kw value")))?;
        let n =
            i16::try_from(int(n, &at)?).map_err(|_| malformed(&at, "amount does not fit i16"))?;
        out.0[kw as usize] = Some(n);
    }
    Ok(out)
}

fn def_pairs(v: &Value, at: &str, n_defs: usize) -> Result<Box<[(DefId, DefId)]>, IrError> {
    arr(v, at)?
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let at = format!("{at}[{i}]");
            let (a, b) = pair(item, &at)?;
            Ok((def_id(a, &at, n_defs)?, def_id(b, &at, n_defs)?))
        })
        .collect()
}

fn decode_printable(v: &Value, at: &str) -> Result<Box<[char]>, IrError> {
    arr(v, at)?
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let s = text(c, &format!("{at}[{i}]"))?;
            let mut chars = s.chars();
            match (chars.next(), chars.next()) {
                (Some(ch), None) if !ch.is_ascii() => Ok(ch),
                _ => Err(malformed(
                    &format!("{at}[{i}]"),
                    "expected one non-ASCII character",
                )),
            }
        })
        .collect()
}

/// A string table: strictly ascending (code-point order, which is byte order of UTF-8), and
/// every non-ASCII character in `printable` (PLAN §5.7).
fn decode_table(ir: &Value, name: &str, printable: &[char]) -> Result<Box<[Box<str>]>, IrError> {
    let at = format!("$.{name}");
    let items = arr(key(ir, name, "$")?, &at)?;
    let mut out: Vec<Box<str>> = Vec::with_capacity(items.len());
    for (i, v) in items.iter().enumerate() {
        let s = text(v, &format!("{at}[{i}]"))?;
        if let Some(prev) = out.last()
            && **prev >= *s
        {
            return Err(malformed(
                &format!("{at}[{i}]"),
                "the table is not strictly sorted",
            ));
        }
        if let Some(ch) = s.chars().find(|c| !c.is_ascii() && !printable.contains(c)) {
            return Err(malformed(
                &format!("{at}[{i}]"),
                format!(
                    "U+{:04X} is not a printable_non_ascii character",
                    u32::from(ch)
                ),
            ));
        }
        out.push(s.into());
    }
    u32::try_from(out.len()).map_err(|_| malformed(&at, "too many strings"))?;
    Ok(out.into_boxed_slice())
}

/// The IR's `schema` and `enums` equal the generated `dsl::CLASSES` and `dsl::ENUMS`: class
/// and enum names, order, field names, annotations and defaults (PLAN §3.4).
fn check_schema(ir: &Value) -> Result<(), IrError> {
    let schema = obj(key(ir, "schema", "$")?, "$.schema")?;
    let names: Vec<&String> = schema.keys().collect();
    let want: Vec<&str> = CLASSES.iter().map(|c| c.name).collect();
    if names.len() != want.len() || names.iter().zip(&want).any(|(a, b)| a.as_str() != *b) {
        let first = names
            .iter()
            .zip(&want)
            .position(|(a, b)| a.as_str() != *b)
            .unwrap_or(names.len().min(want.len()));
        return Err(IrError::Schema {
            detail: format!(
                "class #{first}: IR {:?}, this build {:?} ({} vs {} classes)",
                names.get(first),
                want.get(first),
                names.len(),
                want.len()
            ),
        });
    }
    for class in CLASSES {
        let rows = arr(&schema[class.name], &format!("$.schema.{}", class.name))?;
        if rows.len() != class.fields.len() {
            return Err(IrError::Schema {
                detail: format!(
                    "{}: IR has {} fields, this build {}",
                    class.name,
                    rows.len(),
                    class.fields.len()
                ),
            });
        }
        for (row, field) in rows.iter().zip(class.fields) {
            let row = arr(row, "$.schema")?;
            let got = match row {
                [n, a, d, h] => (n.as_str(), a.as_str(), d.as_str(), h.as_bool()),
                _ => (None, None, None, None),
            };
            let want = (
                Some(field.name),
                Some(field.annotation),
                field.default_repr,
                Some(field.default_repr.is_some()),
            );
            if got != want {
                return Err(IrError::Schema {
                    detail: format!(
                        "{}.{}: IR {got:?}, this build {want:?}",
                        class.name, field.name
                    ),
                });
            }
        }
    }
    let enums = obj(key(ir, "enums", "$")?, "$.enums")?;
    if enums.len() != ENUMS.len() {
        return Err(IrError::Schema {
            detail: format!("IR has {} enums, this build {}", enums.len(), ENUMS.len()),
        });
    }
    for ((name, members), e) in enums.iter().zip(ENUMS) {
        let members = arr(members, "$.enums")?;
        let same = name == e.name
            && members.len() == e.members.len()
            && members.iter().zip(e.members).all(|(m, (mn, mv))| {
                matches!(m.as_array().map(Vec::as_slice), Some([a, b]) if a.as_str() == Some(mn) && b.as_str() == Some(mv))
            });
        if !same {
            return Err(IrError::Schema {
                detail: format!(
                    "enum {name}: IR members differ from this build's {}",
                    e.name
                ),
            });
        }
    }
    Ok(())
}

/// The IR's `hooks` equal the generated hook enums, in registration order.
fn check_hooks(ir: &Value) -> Result<(), IrError> {
    let hooks = key(ir, "hooks", "$")?;
    let tables: [(&str, Vec<&str>); 4] = [
        ("step", StepHook::ALL.iter().map(|h| h.as_str()).collect()),
        ("cond", CondHook::ALL.iter().map(|h| h.as_str()).collect()),
        (
            "filter",
            FilterHook::ALL.iter().map(|h| h.as_str()).collect(),
        ),
        ("value", ValueHook::ALL.iter().map(|h| h.as_str()).collect()),
    ];
    for (kind, want) in tables {
        let got: Vec<Option<&str>> = arr(key(hooks, kind, "$.hooks")?, "$.hooks")?
            .iter()
            .map(Value::as_str)
            .collect();
        if got.len() != want.len() || got.iter().zip(&want).any(|(a, b)| *a != Some(*b)) {
            return Err(IrError::Schema {
                detail: format!("{kind} hooks: IR {got:?}, this build {want:?}"),
            });
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// The node arena

/// The self-contained part of an IR document: its character, string and variable tables and
/// its node arena (the IR itself, or its `repr_vectors`).
pub(crate) struct ArenaTables {
    pub printable: Box<[char]>,
    pub strings: Box<[Box<str>]>,
    pub var_names: Box<[Box<str>]>,
    pub nodes: Box<[Node]>,
    pub kinds: Vec<NodeKind>,
}

/// Decodes `printable_non_ascii`, `strings`, `var_names` and `nodes` of `doc`.
pub(crate) fn decode_arena(doc: &Value) -> Result<ArenaTables, IrError> {
    let printable = decode_printable(
        key(doc, "printable_non_ascii", "$")?,
        "$.printable_non_ascii",
    )?;
    let strings = decode_table(doc, "strings", &printable)?;
    let var_names = decode_table(doc, "var_names", &printable)?;
    if var_names.len() > usize::from(u16::MAX) {
        return Err(malformed("$.var_names", "more than 65535 names"));
    }
    let mut arena = Arena::new(Segments::one(&strings), Segments::one(&var_names));
    arena.decode_nodes(doc, arr(key(doc, "nodes", "$")?, "$.nodes")?, "$.nodes")?;
    let Arena { nodes, kinds, .. } = arena;
    Ok(ArenaTables {
        printable,
        strings,
        var_names,
        nodes: nodes.into_boxed_slice(),
        kinds,
    })
}

struct Arena<'a> {
    strings: Segments<'a>,
    var_names: Segments<'a>,
    nodes: Vec<Node>,
    kinds: Vec<NodeKind>,
}

impl<'a> Arena<'a> {
    fn new(strings: Segments<'a>, var_names: Segments<'a>) -> Self {
        Self {
            strings,
            var_names,
            nodes: Vec::new(),
            kinds: Vec::new(),
        }
    }

    /// Decodes `items` after the nodes already present. `ir` is the document that holds them
    /// (its `cards` name the owner of a node in errors).
    fn decode_nodes(&mut self, ir: &Value, items: &[Value], at: &str) -> Result<(), IrError> {
        let mut by_name: Vec<(&'static str, NodeKind)> =
            NodeKind::ALL.iter().map(|k| (k.py_name(), *k)).collect();
        by_name.sort_by_key(|(n, _)| *n);
        let first = self.nodes.len();
        self.nodes.reserve(items.len());
        let mut d = Decoder {
            strings: self.strings,
            var_names: self.var_names,
            kinds: std::mem::take(&mut self.kinds),
            at: String::new(),
            field: "",
        };
        for (i, item) in items.iter().enumerate() {
            d.at = format!("{at}[{i}]");
            let o = obj(item, &d.at)?;
            let class = o
                .get("_")
                .and_then(Value::as_str)
                .ok_or_else(|| malformed(&d.at, "missing class name"))?;
            let Ok(k) = by_name.binary_search_by_key(&class, |(n, _)| *n) else {
                return Err(IrError::UnknownClass {
                    class: class.to_owned(),
                    card_number: owner_card(ir, first + i, first),
                });
            };
            let kind = by_name[k].1;
            let node = Node::decode(kind, o, &mut d).map_err(|e| match e {
                IrError::UnknownHook { kind, name, .. } => IrError::UnknownHook {
                    kind,
                    name,
                    card_number: owner_card(ir, first + i, first),
                },
                other => other,
            })?;
            d.kinds.push(kind);
            self.nodes.push(node);
        }
        len_u32(self.nodes.len());
        self.kinds = d.kinds;
        Ok(())
    }
}

/// The number of the first card (def_id order) whose script contains node `target`, from the
/// raw IR document whose `nodes` start at id `offset` (used for W-69 errors only).
fn owner_card(ir: &Value, target: usize, offset: usize) -> Option<String> {
    let nodes = ir.get("nodes")?.as_array()?;
    for card in ir.get("cards")?.as_array()? {
        let Some(root) = card
            .get("script")
            .and_then(Value::as_u64)
            .and_then(|r| usize::try_from(r).ok())
        else {
            continue;
        };
        let mut seen = vec![false; nodes.len()];
        let mut stack = vec![root];
        while let Some(n) = stack.pop() {
            if n == target {
                return card
                    .get("card_number")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
            }
            let Some(local) = n.checked_sub(offset).filter(|&l| l < nodes.len()) else {
                continue;
            };
            if !std::mem::replace(&mut seen[local], true) {
                collect_refs(&nodes[local], &mut stack);
            }
        }
    }
    None
}

fn collect_refs(v: &Value, out: &mut Vec<usize>) {
    match v {
        Value::Object(m) => {
            if m.len() == 1
                && let Some(id) = m
                    .get("$")
                    .and_then(Value::as_u64)
                    .and_then(|r| usize::try_from(r).ok())
            {
                out.push(id);
                return;
            }
            for x in m.values() {
                collect_refs(x, out);
            }
        }
        Value::Array(items) => items.iter().for_each(|x| collect_refs(x, out)),
        _ => {}
    }
}

// ---------------------------------------------------------------------------------------------
// The other tables

/// Card entries and their golden texts, by def_id.
type Cards = (Box<[CardEntry]>, Box<[CardText]>);
/// `repr` of every `RuleMod` node, by node.
type RuleModReprs = Box<[(RuleModId, Box<str>)]>;
/// Name aliases by def_id.
type Aliases = Box<[(DefId, Box<[StrId]>)]>;

struct Builder<'a> {
    strings: Segments<'a>,
    var_names: Segments<'a>,
    /// The whole string table (base and fragment), indexed by `StrId`.
    all_strings: &'a [Box<str>],
    nodes: &'a [Node],
    kinds: &'a [NodeKind],
    n_programs: usize,
    n_abilities: usize,
}

impl Builder<'_> {
    fn string(&self, v: &Value, at: &str) -> Result<StrId, IrError> {
        let s = text(v, at)?;
        self.strings
            .find(s)
            .map(StrId)
            .ok_or_else(|| malformed(at, format!("{s:?} is not in the string table")))
    }

    fn node_of(&self, v: &Value, at: &str, kinds: &[NodeKind]) -> Result<NodeId, IrError> {
        let i = int(v, at)?;
        let ok = usize::try_from(i).ok().filter(|&i| i < self.nodes.len());
        let Some(i) = ok else {
            return Err(malformed(at, format!("{i} is not a node")));
        };
        if !kinds.contains(&self.kinds[i]) {
            return Err(malformed(
                at,
                format!("node {i} is a {}", self.kinds[i].py_name()),
            ));
        }
        Ok(NodeId(len_u32(i)))
    }

    fn program_ref(&self, v: &Value, at: &str) -> Result<Option<ProgramId>, IrError> {
        let i = int(v, at)?;
        if i == -1 {
            return Ok(None);
        }
        usize::try_from(i)
            .ok()
            .filter(|&i| i < self.n_programs)
            .and_then(|i| u16::try_from(i).ok())
            .map(|i| Some(ProgramId(i)))
            .ok_or_else(|| malformed(at, format!("{i} is not a program id")))
    }

    fn aid(&self, v: &Value, at: &str) -> Result<Option<AbilityId>, IrError> {
        let i = int(v, at)?;
        if i == -1 {
            return Ok(None);
        }
        usize::try_from(i)
            .ok()
            .filter(|&i| i < self.n_abilities)
            .and_then(|i| u16::try_from(i).ok())
            .map(|i| Some(AbilityId(i)))
            .ok_or_else(|| malformed(at, format!("{i} is not an ability id")))
    }

    fn aids(&self, v: &Value, at: &str) -> Result<Box<[AbilityId]>, IrError> {
        arr(v, at)?
            .iter()
            .enumerate()
            .map(|(i, x)| {
                let at = format!("{at}[{i}]");
                self.aid(x, &at)?
                    .ok_or_else(|| malformed(&at, "expected an ability id"))
            })
            .collect()
    }

    fn content_ids(&self, v: &Value, at: &str, n: usize) -> Result<Box<[ContentId]>, IrError> {
        let items = arr(v, at)?;
        if items.len() != n {
            return Err(malformed(
                at,
                format!("{} content ids for {n} nodes", items.len()),
            ));
        }
        items
            .iter()
            .enumerate()
            .map(|(i, x)| content_id(x, &format!("{at}[{i}]")))
            .collect()
    }

    fn programs(&self, items: &[Value], at: &str, first: usize) -> Result<Box<[Program]>, IrError> {
        let out = items
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let at = format!("{at}[{i}]");
                let instrs = arr(key(p, "instrs", &at)?, &format!("{at}.instrs"))?
                    .iter()
                    .enumerate()
                    .map(|(j, x)| self.node_of(x, &format!("{at}.instrs[{j}]"), NodeKind::INSTR))
                    .collect::<Result<Box<[NodeId]>, IrError>>()?;
                Ok(Program {
                    label: self.string(key(p, "label", &at)?, &format!("{at}.label"))?,
                    content_id: content_id(
                        key(p, "content_id", &at)?,
                        &format!("{at}.content_id"),
                    )?,
                    instrs,
                })
            })
            .collect::<Result<Box<[Program]>, IrError>>()?;
        if first + out.len() > usize::from(u16::MAX) {
            return Err(malformed(at, "more than 65535 programs"));
        }
        Ok(out)
    }

    fn program_table<const N: usize>(
        &self,
        ir: &Value,
        name: &str,
        names: &[&str; N],
    ) -> Result<[ProgramId; N], IrError> {
        let at = format!("$.{name}");
        let table = obj(key(ir, name, "$")?, &at)?;
        if table.len() != N {
            return Err(malformed(&at, format!("expected {N} entries")));
        }
        let mut out = [ProgramId(0); N];
        for (slot, n) in out.iter_mut().zip(names) {
            let at = format!("{at}.{n}");
            *slot = self
                .program_ref(table.get(*n).ok_or_else(|| malformed(&at, "missing"))?, &at)?
                .ok_or_else(|| malformed(&at, "expected a program id"))?;
        }
        Ok(out)
    }

    fn abilities(
        &self,
        items: &[Value],
        at: &str,
        first: usize,
    ) -> Result<Box<[AbilityEntry]>, IrError> {
        if first + items.len() > usize::from(u16::MAX) {
            return Err(malformed(at, "more than 65535 abilities"));
        }
        items
            .iter()
            .enumerate()
            .map(|(i, a)| {
                let at = format!("{at}[{i}]");
                let f = |k: &str| key(a, k, &at);
                let index = u16::try_from(int(f("index")?, &at)?)
                    .map_err(|_| malformed(&format!("{at}.index"), "does not fit u16"))?;
                let trigger_event = match f("trigger_event")? {
                    Value::Null => None,
                    v => {
                        let s = text(v, &format!("{at}.trigger_event"))?;
                        Some(Ev::from_value(s).ok_or_else(|| {
                            malformed(
                                &format!("{at}.trigger_event"),
                                format!("{s:?} is not an Ev value"),
                            )
                        })?)
                    }
                };
                Ok(AbilityEntry {
                    aid: AbilityId(u16::try_from(first + i).unwrap_or(u16::MAX)),
                    card_number: self.string(f("card_number")?, &format!("{at}.card_number"))?,
                    def_id: def_id(f("def_id")?, &format!("{at}.def_id"), usize::from(u16::MAX))?,
                    index,
                    unit_text: flag(f("unit_text")?, &format!("{at}.unit_text"))?,
                    ability: AbilityNodeId(self.node_of(
                        f("ability")?,
                        &format!("{at}.ability"),
                        NodeKind::ABILITY,
                    )?),
                    program: self.program_ref(f("program_id")?, &format!("{at}.program_id"))?,
                    cost_program: self
                        .program_ref(f("cost_program_id")?, &format!("{at}.cost_program_id"))?,
                    trigger_event,
                })
            })
            .collect()
    }

    fn cards(&self, items: &[Value], at: &str) -> Result<Cards, IrError> {
        let mut cards = Vec::with_capacity(items.len());
        let mut texts = Vec::with_capacity(items.len());
        for (i, c) in items.iter().enumerate() {
            let at = format!("{at}[{i}]");
            let f = |k: &str| key(c, k, &at);
            let p = |k: &str| format!("{at}.{k}");
            let source = text(f("source")?, &p("source"))?;
            let source = ScriptSource::ALL
                .into_iter()
                .find(|s| s.as_str() == source)
                .ok_or_else(|| {
                    malformed(&p("source"), format!("{source:?} is not a script source"))
                })?;
            let binding_module = match f("binding_module")? {
                Value::Null => None,
                v => Some(self.string(v, &p("binding_module"))?),
            };
            let script = match f("script")? {
                Value::Null => {
                    return Err(malformed(
                        &p("script"),
                        "no script: the card is unimplemented (W-69)",
                    ));
                }
                v => self.node_of(v, &p("script"), &[NodeKind::CardScript])?,
            };
            let timings = arr(f("timings")?, &p("timings"))?
                .iter()
                .map(|t| self.string(t, &p("timings")))
                .collect::<Result<Box<[StrId]>, IrError>>()?;
            let delayed = arr(f("delayed")?, &p("delayed"))?
                .iter()
                .enumerate()
                .map(|(j, d)| {
                    let at = format!("{at}.delayed[{j}]");
                    match arr(d, &at)? {
                        [n, node, prog] => Ok((
                            u16::try_from(int(n, &at)?)
                                .map_err(|_| malformed(&at, "n does not fit u16"))?,
                            self.node_of(node, &at, &[NodeKind::DelayedTrigger])?,
                            self.program_ref(prog, &at)?
                                .ok_or_else(|| malformed(&at, "expected a program id"))?,
                        )),
                        _ => Err(malformed(&at, "expected [n, node, program]")),
                    }
                })
                .collect::<Result<Box<[(u16, NodeId, ProgramId)]>, IrError>>()?;
            let template = match f("template")? {
                Value::Null => None,
                v => Some(self.string(v, &p("template"))?),
            };
            let entry = CardEntry {
                def_id: def_id(f("def_id")?, &p("def_id"), usize::from(u16::MAX))?,
                source,
                binding_module,
                script: Some(script),
                own: self.aids(f("own")?, &p("own"))?,
                unit: self.aids(f("unit")?, &p("unit"))?,
                command_aid: self.aid(f("command_aid")?, &p("command_aid"))?,
                burst_aid: self.aid(f("burst_aid")?, &p("burst_aid"))?,
                has_burst: flag(f("has_burst")?, &p("has_burst"))?,
                timings,
                field_entries: self.aids(f("field_entries")?, &p("field_entries"))?,
                hand_entries: self.aids(f("hand_entries")?, &p("hand_entries"))?,
                trash_entries: self.aids(f("trash_entries")?, &p("trash_entries"))?,
                paired_entries: self.aids(f("paired_entries")?, &p("paired_entries"))?,
                unit_entries: self.aids(f("unit_entries")?, &p("unit_entries"))?,
                gated: flag(f("gated")?, &p("gated"))?,
                events: ev_set(f("events")?, &p("events"))?,
                printed_keywords: kw_amounts(f("printed_keywords")?, &p("printed_keywords"))?,
                delayed,
                template,
            };
            let card_number = text(f("card_number")?, &p("card_number"))?;
            if let Node::CardScript(s) = &self.nodes[script.0 as usize]
                && &*self.all_strings[s.card_number.0 as usize] != card_number
            {
                return Err(malformed(
                    &p("script"),
                    format!(
                        "the script is for {}",
                        self.all_strings[s.card_number.0 as usize]
                    ),
                ));
            }
            texts.push(CardText {
                card_number: card_number.into(),
                name: text(f("name")?, &p("name"))?.into(),
                text_hash: text(f("text_hash")?, &p("text_hash"))?.into(),
            });
            cards.push(entry);
        }
        Ok((cards.into_boxed_slice(), texts.into_boxed_slice()))
    }

    /// Every `Continuous` node from `first` on, in node order, with its ContRef; the IR's
    /// ContRef must equal [`canonical_cont`].
    fn continuous(
        &self,
        v: &Value,
        at: &str,
        first: usize,
    ) -> Result<Box<[(ContinuousId, ContRef)]>, IrError> {
        let items = arr(v, at)?;
        let want: Vec<usize> = (first..self.nodes.len())
            .filter(|&i| NodeKind::CONTINUOUS.contains(&self.kinds[i]))
            .collect();
        if items.len() != want.len() {
            return Err(malformed(
                at,
                format!(
                    "{} entries for {} Continuous nodes",
                    items.len(),
                    want.len()
                ),
            ));
        }
        items
            .iter()
            .zip(want)
            .enumerate()
            .map(|(i, (item, n))| {
                let at = format!("{at}[{i}]");
                let node = self.node_of(
                    key(item, "node", &at)?,
                    &format!("{at}.node"),
                    NodeKind::CONTINUOUS,
                )?;
                if node.0 as usize != n {
                    return Err(malformed(
                        &format!("{at}.node"),
                        format!("expected node {n}"),
                    ));
                }
                let cont = self.cont_ref(key(item, "cont", &at)?, &format!("{at}.cont"))?;
                let canonical = canonical_cont(node, &self.nodes[n]);
                if canonical != Some(cont) {
                    return Err(malformed(
                        &at,
                        format!("IR ContRef {cont:?}, canonical {canonical:?}"),
                    ));
                }
                Ok((ContinuousId(node), cont))
            })
            .collect()
    }

    fn cont_ref(&self, v: &Value, at: &str) -> Result<ContRef, IrError> {
        let o = obj(v, at)?;
        let (tag, body) = match o.iter().next() {
            Some(x) if o.len() == 1 => x,
            _ => return Err(malformed(at, "expected one ContRef variant")),
        };
        let i32_of =
            |x: &Value| i32::try_from(int(x, at)?).map_err(|_| malformed(at, "does not fit i32"));
        Ok(match tag.as_str() {
            "StatMod" => {
                let (ap, hp) = pair(body, at)?;
                ContRef::StatMod {
                    ap: i32_of(ap)?,
                    hp: i32_of(hp)?,
                }
            }
            "KeywordGrant" => {
                let (kw, n) = pair(body, at)?;
                let kw = text(kw, at)?;
                ContRef::KeywordGrant {
                    kw: Kw::from_value(kw)
                        .ok_or_else(|| malformed(at, format!("{kw:?} is not a Kw value")))?,
                    amount: i32_of(n)?,
                }
            }
            "AbilityGrant" => {
                let (card, index) = pair(body, at)?;
                ContRef::AbilityGrant {
                    card: self.string(card, at)?,
                    index: i32_of(index)?,
                }
            }
            "Node" => ContRef::Node(self.node_of(body, at, NodeKind::CONTINUOUS)?),
            other => return Err(malformed(at, format!("unknown ContRef variant {other:?}"))),
        })
    }

    fn filter_sets(&self, v: &Value, at: &str) -> Result<Box<[FilterSet]>, IrError> {
        arr(v, at)?
            .iter()
            .enumerate()
            .map(|(i, item)| {
                let at = format!("{at}[{i}]");
                let filters = arr(key(item, "filters", &at)?, &at)?
                    .iter()
                    .map(|f| self.node_of(f, &at, NodeKind::FILTER).map(FilterId))
                    .collect::<Result<Box<[FilterId]>, IrError>>()?;
                Ok(FilterSet {
                    filters,
                    content_id: content_id(
                        key(item, "content_id", &at)?,
                        &format!("{at}.content_id"),
                    )?,
                })
            })
            .collect()
    }

    /// Every `RuleMod` node from `first` on, in node order, with its `repr`.
    fn rule_mods(&self, v: &Value, at: &str, first: usize) -> Result<RuleModReprs, IrError> {
        let items = arr(v, at)?;
        let want: Vec<usize> = (first..self.nodes.len())
            .filter(|&i| self.kinds[i] == NodeKind::RuleMod)
            .collect();
        if items.len() != want.len() {
            return Err(malformed(
                at,
                format!("{} entries for {} RuleMod nodes", items.len(), want.len()),
            ));
        }
        items
            .iter()
            .zip(want)
            .enumerate()
            .map(|(i, (item, n))| {
                let at = format!("{at}[{i}]");
                let node = self.node_of(
                    key(item, "node", &at)?,
                    &format!("{at}.node"),
                    &[NodeKind::RuleMod],
                )?;
                if node.0 as usize != n {
                    return Err(malformed(
                        &format!("{at}.node"),
                        format!("expected node {n}"),
                    ));
                }
                Ok((
                    RuleModId(node),
                    text(key(item, "repr", &at)?, &format!("{at}.repr"))?.into(),
                ))
            })
            .collect()
    }

    fn aliases(&self, v: &Value, n_defs: usize) -> Result<Aliases, IrError> {
        let mut out: Vec<(DefId, Box<[StrId]>)> = Vec::new();
        for (i, item) in arr(v, "$.aliases")?.iter().enumerate() {
            let at = format!("$.aliases[{i}]");
            let (def, names) = pair(item, &at)?;
            let def = def_id(def, &at, n_defs)?;
            if out.last().is_some_and(|(d, _)| *d >= def) {
                return Err(malformed(&at, "aliases must be sorted by def_id"));
            }
            let names = arr(names, &at)?
                .iter()
                .map(|n| self.string(n, &at))
                .collect::<Result<Box<[StrId]>, IrError>>()?;
            out.push((def, names));
        }
        Ok(out.into_boxed_slice())
    }

    fn token_keys(&self, v: &Value, n_defs: usize) -> Result<Box<[(StrId, DefId)]>, IrError> {
        let mut out: Vec<(StrId, DefId)> = Vec::new();
        for (i, item) in arr(v, "$.token_keys")?.iter().enumerate() {
            let at = format!("$.token_keys[{i}]");
            let (k, def) = pair(item, &at)?;
            let k = self.string(k, &at)?;
            if out.last().is_some_and(|(p, _)| *p >= k) {
                return Err(malformed(&at, "token keys must be sorted"));
            }
            out.push((k, def_id(def, &at, n_defs)?));
        }
        Ok(out.into_boxed_slice())
    }

    /// Typed parameters of every `Custom*` node from `first` on (PLAN §5.3).
    fn hook_params(&self, first: usize, at: &str) -> Result<Box<[(NodeId, HookParams)]>, IrError> {
        let mut out = Vec::new();
        for i in first..self.nodes.len() {
            let cx = ParamCx {
                strings: self.all_strings,
                var_names: self.var_names,
                kinds: self.kinds,
                at: format!("{at}[{}]", i - first),
            };
            if let Some(p) = hook_params(&self.nodes[i], &cx)? {
                out.push((NodeId(len_u32(i)), p));
            }
        }
        Ok(out.into_boxed_slice())
    }
}

fn content_id(v: &Value, at: &str) -> Result<ContentId, IrError> {
    let s = text(v, at)?;
    ContentId::from_hex(s)
        .ok_or_else(|| malformed(at, format!("{s:?} is not 16 lowercase hex digits")))
}
