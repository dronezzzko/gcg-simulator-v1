//! The canonical JSON form of a state and its digest (`runner/digest.py:36-56`,
//! `engine/state.py:575-611`; docs/dev/PLAN.md §4.7) and the decoding of a canonical snapshot
//! (PLAN §4.8).
//!
//! The canonical form is `GameState.to_json()` with every process-local registry index
//! replaced by a content id (`digest.py:36-49`): the effect (position 0) and filter set
//! (position 9, or `-1`) of every lasting and delayed entry, the program of every frame and
//! trigger, and the effect inside the `(-8 | -7, -1, effect, ...)` once keys, whose list is
//! sorted by each key's `json.dumps` text (`digest.py:48`). Records keep Python's bool
//! encodings: 0/1 in cards, battles and triggers, `true`/`false` in frames and `redraws`.
//!
//! Everything here reads the registry through [`Tables`], the few lookups the canonical form
//! needs (content ids, names, prompt instructions, side-table nodes); `Registry` implements
//! it, which is the integration point with gcg-effects (B1).

use std::borrow::Cow;
use std::fmt::Write as _;
use std::sync::Arc;

use gcg_core::pyjson::{DumpOptions, Json, Key, dumps};
use gcg_data::DefId;
use gcg_effects::{
    ContRef, ContentId, Duration, FilterSetId, InlineNode, Node, ProgramId, Registry, StrId,
    ValueRef, VarId,
};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::{
    AbilityKey, Action, ActionKind, Battle, CONTEXT_CAPACITY, CanonError, CardInstance, CtxKey,
    DECISION_JSON_KEYS, DecisionHdr, DecisionKind, EVENT_CAPACITY, EndReason, EnginePrompt, EvKey,
    EventPayload, FRAME_JSON_KEYS, Frame, FrameKind, GameConst, GameState, HOLD_RULES, HistKind,
    HistoryEvent, IntEntry, IntKey, Lasting, ONCE_KEY_CAPACITY, OnceElem, OnceKey, Phase, Prompt,
    STATE_JSON_KEYS, Span, StateSnapshot, Step, TriggerInst, Uid, VarEntry, Zone, ZoneLists,
};

// ---------------------------------------------------------------------------------------------
// The public API (docs/dev/contracts.md, gcg-engine `canon`)

/// `canonical_state(st)` (`runner/digest.py:36-49`): `st.to_json()` (`engine/state.py:575-611`)
/// with every process-local registry index replaced by a content id from the state's registry.
#[must_use]
pub fn canonical_state(st: &GameState) -> Json {
    encode(&st.parts(), &**st.registry())
}

/// `state_digest(st)` (`runner/digest.py:52-56`): the lowercase SHA-256 hex of
/// `json.dumps(canonical_state(st), sort_keys=True, separators=(",", ":"))` (`ensure_ascii`).
#[must_use]
pub fn state_digest(st: &GameState) -> String {
    digest_of(&canonical_state(st))
}

/// Rebuilds a state from a trace snapshot (PLAN §4.8; `GameState.from_json`,
/// `engine/state.py:613-651`, over the canonical form). Programs, filter sets and
/// build-time continuous effects resolve by content id in `reg`; a continuous effect the
/// registry does not hold is built from its side-table node as the runtime value it is (a
/// frozen `StatMod`, a `KeywordGrant`, a delayed-trigger `AbilityGrant`; PLAN §5.6). Every
/// side-table entry is decoded into typed nodes and must resolve to what its content id names,
/// and every resolved effect and filter set must hash back to its content id. The state has
/// Python's `_rules_checked` from the snapshot (which `from_json` drops), no
/// cached view, and a `GameConst` over `reg` built from the snapshot's decklists and limits.
///
/// # Errors
/// [`CanonError`] naming the JSON path of the first malformed value, or the content id that
/// names nothing in `reg`.
pub fn from_canonical(reg: &Arc<Registry>, snap: &StateSnapshot) -> Result<GameState, CanonError> {
    let mut body = decode(&**reg, snap)?;
    let konst = GameConst::new(
        Arc::clone(reg),
        std::mem::take(&mut body.decklists),
        std::mem::take(&mut body.resource_decklists),
        body.turn_limit,
        body.max_actions,
    )
    .map_err(|e| CanonError::Value {
        path: "$".to_owned(),
        detail: e.to_string(),
    })?;
    Ok(GameState::from_body(
        Arc::new(konst),
        body,
        snap.rules_checked,
    ))
}

impl Prompt {
    /// `Decision.prompt` as Python stores and serialises it (`engine/state.py:42`, `:56`): the
    /// engine literal, the AskMay prompt, or `"choose " + var`.
    #[must_use]
    pub fn text<'r>(&self, reg: &'r Registry) -> Cow<'r, str> {
        prompt_text(*self, reg)
    }
}

/// `json.dumps(x)` with Python's defaults: `", "`, `": "`, `ensure_ascii=True`, insertion
/// order. The once-key sort key (`digest.py:48`).
pub(crate) const PY_DEFAULT: DumpOptions = DumpOptions {
    indent: None,
    sort_keys: false,
    compact: false,
    ensure_ascii: true,
};

/// The registry lookups of the canonical form. Content ids are the 16 lowercase hex
/// characters of `sha256(repr(x))` (`digest.py:26-27`).
pub(crate) trait Tables {
    /// `Registry::str`.
    fn text(&self, id: StrId) -> &str;
    /// `Registry::var_name`: frame variable and loop-key names.
    fn var_name(&self, v: VarId) -> &str;
    /// `Registry::var_id`.
    fn var_id(&self, name: &str) -> Option<VarId>;
    /// The number of card definitions (`len(CardDB)`), which bounds every def_id.
    fn def_count(&self) -> usize;
    /// `content_id(R.programs[p])` (`digest.py:45`, `:47`).
    fn program_cid(&self, p: ProgramId) -> String;
    /// `Registry::program_by_content_id`.
    fn program_by_cid(&self, cid: &str) -> Option<ProgramId>;
    /// `content_id(R.continuous[effect_key])` (`digest.py:32`, `:41`): `ContRef::content_id`.
    fn cont_cid(&self, c: ContRef) -> String;
    /// `Registry::cont_by_content_id`: the effects the registry knows.
    fn cont_by_cid(&self, cid: &str) -> Option<ContRef>;
    /// `content_id(R.filter_sets[k])` (`digest.py:43`).
    fn filter_set_cid(&self, f: FilterSetId) -> String;
    /// `Registry::filter_set_by_content_id`.
    fn filter_set_by_cid(&self, cid: &str) -> Option<FilterSetId>;
    /// A side-table continuous effect (an inline node, PLAN §4.8) as the registry holds it:
    /// the canonical `ContRef` of its arena node, or the runtime value it spells; `None` for a
    /// node that is neither. `Err` for JSON that is not a node of the registry's DSL.
    fn cont_inline(&self, form: &Value) -> Result<Option<ContRef>, String>;
    /// A side-table filter tuple (a list of inline filter nodes) as a filter set of the
    /// registry, or `None` when the registry has no such filter set.
    fn filter_set_inline(&self, form: &Value) -> Result<Option<FilterSetId>, String>;
    /// The prompt of the decision that the instruction at `pc` of `program` asks, when it is an
    /// `AskMay` (`Ask`, or `Engine(YouMay)` for an empty prompt, `interp.py:213-216`) or a
    /// `Choose` (`Choose(var)`, `interp.py:433`, `:461`); `None` for any other instruction.
    fn decision_prompt(&self, program: ProgramId, pc: u16) -> Option<Prompt>;
}

/// The value of a runtime continuous effect that no IR node spells (PLAN §5.6): a `StatMod`
/// with int fields (`_frozen`, `interp.py:1276-1283`, or a hook, `wp_gd03_a.py:111`), a
/// `KeywordGrant` with an int amount (`wp_gd04_a.py:69-106`), an `AbilityGrant` (the
/// delayed-trigger key, `interp.py:1549`); `None` for any other node.
fn runtime_cont(node: &Node) -> Option<ContRef> {
    match node {
        Node::StatMod(m) => match (m.ap, m.hp) {
            (ValueRef::Lit(ap), ValueRef::Lit(hp)) => Some(ContRef::StatMod {
                ap: i32::try_from(ap).ok()?,
                hp: i32::try_from(hp).ok()?,
            }),
            _ => None,
        },
        Node::KeywordGrant(g) => match g.amount {
            ValueRef::Lit(n) => Some(ContRef::KeywordGrant {
                kw: g.keyword,
                amount: i32::try_from(n).ok()?,
            }),
            ValueRef::Node(_) => None,
        },
        Node::AbilityGrant(g) => Some(ContRef::AbilityGrant {
            card: g.card_number,
            index: i32::try_from(g.ability_index).ok()?,
        }),
        _ => None,
    }
}

fn cid_of(s: &str) -> Option<ContentId> {
    ContentId::from_hex(s)
}

/// The registry's lookups for the canonical form (B1's `Registry`, PLAN §4.7-§4.8).
impl Tables for Registry {
    fn text(&self, id: StrId) -> &str {
        self.str(id)
    }

    fn var_name(&self, v: VarId) -> &str {
        Self::var_name(self, v)
    }

    fn var_id(&self, name: &str) -> Option<VarId> {
        Self::var_id(self, name)
    }

    fn def_count(&self) -> usize {
        self.db().len()
    }

    fn program_cid(&self, p: ProgramId) -> String {
        self.program(p).content_id.as_str().to_owned()
    }

    fn program_by_cid(&self, cid: &str) -> Option<ProgramId> {
        cid_of(cid).and_then(|c| self.program_by_content_id(&c))
    }

    fn cont_cid(&self, c: ContRef) -> String {
        c.content_id(self).as_str().to_owned()
    }

    fn cont_by_cid(&self, cid: &str) -> Option<ContRef> {
        cid_of(cid).and_then(|c| self.cont_by_content_id(&c))
    }

    fn filter_set_cid(&self, f: FilterSetId) -> String {
        self.filter_set_content_id(f).as_str().to_owned()
    }

    fn filter_set_by_cid(&self, cid: &str) -> Option<FilterSetId> {
        cid_of(cid).and_then(|c| self.filter_set_by_content_id(&c))
    }

    fn cont_inline(&self, form: &Value) -> Result<Option<ContRef>, String> {
        Ok(match self.decode_inline(form).map_err(|e| e.to_string())? {
            InlineNode::Arena(id) => self.continuous_id(id).map(|c| self.cont_of(c)),
            InlineNode::Detached(node) => runtime_cont(&node),
        })
    }

    fn filter_set_inline(&self, form: &Value) -> Result<Option<FilterSetId>, String> {
        let items = form
            .as_array()
            .ok_or_else(|| format!("expected a list of filter nodes, got {form}"))?;
        let mut filters = Vec::with_capacity(items.len());
        for f in items {
            match self.decode_inline(f).map_err(|e| e.to_string())? {
                InlineNode::Arena(id) => match self.filter_id(id) {
                    Some(fid) => filters.push(fid),
                    None => return Ok(None),
                },
                InlineNode::Detached(_) => return Ok(None),
            }
        }
        Ok(self.filters_key(&filters))
    }

    fn decision_prompt(&self, program: ProgramId, pc: u16) -> Option<Prompt> {
        let ins = *self.program(program).instrs.get(usize::from(pc))?;
        match self.node(ins) {
            Node::AskMay(a) if self.str(a.prompt).is_empty() => {
                Some(Prompt::Engine(EnginePrompt::YouMay))
            }
            Node::AskMay(a) => Some(Prompt::Ask(a.prompt)),
            Node::Choose(c) => Some(Prompt::Choose(c.var)),
            _ => None,
        }
    }
}

/// `Decision.prompt` as Python stores it (`engine/state.py:56`).
pub(crate) fn prompt_text<'t>(p: Prompt, t: &'t dyn Tables) -> Cow<'t, str> {
    match p {
        Prompt::Engine(e) => Cow::Borrowed(e.as_str()),
        Prompt::Ask(id) => Cow::Borrowed(t.text(id)),
        Prompt::Choose(v) => Cow::Owned(format!("choose {}", t.var_name(v))),
    }
}

/// The rendered key of `Frame.ints` (`interp.py:207`, `:286`, `:669-675`, `game.py:933`).
fn int_key_name(k: IntKey, t: &dyn Tables) -> Cow<'_, str> {
    match k {
        IntKey::HoldRules => Cow::Borrowed(HOLD_RULES),
        IntKey::Loop(v) => Cow::Borrowed(t.var_name(v)),
        IntKey::RestI => Cow::Borrowed("__rest_i"),
        IntKey::RestAs(i) => Cow::Owned(format!("__rest_as{i}")),
        IntKey::ExUsed => Cow::Borrowed("ex_used"),
    }
}

/// `hashlib.sha256(blob).hexdigest()`.
pub(crate) fn sha256_hex(blob: &[u8]) -> String {
    let digest = Sha256::digest(blob);
    let mut out = String::with_capacity(64);
    for b in digest {
        let _ = write!(out, "{b:02x}");
    }
    out
}

/// `state_digest` of a canonical form (`digest.py:52-56`): the SHA-256 of
/// `json.dumps(canonical, sort_keys=True, separators=(",", ":"))` (`ensure_ascii=True`).
pub(crate) fn digest_of(canonical: &Json) -> String {
    sha256_hex(dumps(canonical, DumpOptions::COMPACT_SORTED_ASCII).as_bytes())
}

// ---------------------------------------------------------------------------------------------
// The serialised fields of a state

/// Everything `GameState.to_json()` serialises, borrowed from a state (`GameState::parts`)
/// or a decoded snapshot (`StateBody::parts`).
#[derive(Clone, Copy)]
pub(crate) struct Parts<'a> {
    pub(crate) seed: u64,
    pub(crate) rng: u64,
    pub(crate) cards: &'a [CardInstance],
    pub(crate) zones: &'a ZoneLists,
    pub(crate) turn: i32,
    pub(crate) active: u8,
    pub(crate) first_player: u8,
    pub(crate) setup_chooser: u8,
    pub(crate) phase: Phase,
    pub(crate) step: Step,
    pub(crate) pending: Option<&'a DecisionHdr>,
    pub(crate) pending_options: &'a [Action],
    pub(crate) battles: &'a [Battle],
    pub(crate) next_battle_id: i32,
    pub(crate) next_lasting_serial: i32,
    pub(crate) priority: u8,
    pub(crate) passes: i32,
    pub(crate) lasting: &'a [Lasting],
    pub(crate) delayed: &'a [Lasting],
    pub(crate) lasting_targets: &'a [(Uid, i32)],
    pub(crate) triggers: &'a [TriggerInst],
    pub(crate) batch_starts: &'a [u32],
    pub(crate) pending_start: u32,
    pub(crate) frames: &'a [Frame],
    pub(crate) frame_vars: &'a [VarEntry],
    pub(crate) frame_ints: &'a [IntEntry],
    pub(crate) frame_items: &'a [i32],
    pub(crate) once_used: &'a [OnceKey],
    pub(crate) history: &'a [HistoryEvent],
    pub(crate) winner: Option<i8>,
    pub(crate) end_reason: Option<EndReason>,
    pub(crate) decklists: [&'a [DefId]; 2],
    pub(crate) resource_decklists: [&'a [DefId]; 2],
    pub(crate) redraws: [bool; 2],
    pub(crate) action_count: i64,
    pub(crate) event_group: i64,
    pub(crate) effect_hits: &'a [(Uid, Uid)],
    pub(crate) turn_limit: i32,
    pub(crate) max_actions: i64,
    pub(crate) version: u64,
}

/// The serialised fields of a decoded snapshot, laid out as in `GameState` (PLAN §4.2), with
/// the decklists and limits that a state keeps in its `GameConst`.
#[derive(Clone, Debug)]
pub(crate) struct StateBody {
    pub(crate) seed: u64,
    pub(crate) rng: u64,
    pub(crate) cards: Vec<CardInstance>,
    pub(crate) zones: ZoneLists,
    pub(crate) turn: i32,
    pub(crate) active: u8,
    pub(crate) first_player: u8,
    pub(crate) setup_chooser: u8,
    pub(crate) phase: Phase,
    pub(crate) step: Step,
    pub(crate) pending: Option<DecisionHdr>,
    pub(crate) pending_options: Vec<Action>,
    pub(crate) battles: Vec<Battle>,
    pub(crate) next_battle_id: i32,
    pub(crate) next_lasting_serial: i32,
    pub(crate) priority: u8,
    pub(crate) passes: i32,
    pub(crate) lasting: Vec<Lasting>,
    pub(crate) delayed: Vec<Lasting>,
    pub(crate) lasting_targets: Vec<(Uid, i32)>,
    pub(crate) triggers: Vec<TriggerInst>,
    pub(crate) batch_starts: Vec<u32>,
    pub(crate) pending_start: u32,
    pub(crate) frames: Vec<Frame>,
    pub(crate) frame_vars: Vec<VarEntry>,
    pub(crate) frame_ints: Vec<IntEntry>,
    pub(crate) frame_items: Vec<i32>,
    pub(crate) once_used: Vec<OnceKey>,
    pub(crate) history: Vec<HistoryEvent>,
    pub(crate) winner: Option<i8>,
    pub(crate) end_reason: Option<EndReason>,
    pub(crate) decklists: [Box<[DefId]>; 2],
    pub(crate) resource_decklists: [Box<[DefId]>; 2],
    pub(crate) redraws: [bool; 2],
    pub(crate) action_count: i64,
    pub(crate) event_group: i64,
    pub(crate) effect_hits: Vec<(Uid, Uid)>,
    pub(crate) turn_limit: i32,
    pub(crate) max_actions: i64,
    pub(crate) version: u64,
}

#[cfg(test)]
impl StateBody {
    /// The fields of this body for [`encode`].
    pub(crate) fn parts(&self) -> Parts<'_> {
        Parts {
            seed: self.seed,
            rng: self.rng,
            cards: &self.cards,
            zones: &self.zones,
            turn: self.turn,
            active: self.active,
            first_player: self.first_player,
            setup_chooser: self.setup_chooser,
            phase: self.phase,
            step: self.step,
            pending: self.pending.as_ref(),
            pending_options: &self.pending_options,
            battles: &self.battles,
            next_battle_id: self.next_battle_id,
            next_lasting_serial: self.next_lasting_serial,
            priority: self.priority,
            passes: self.passes,
            lasting: &self.lasting,
            delayed: &self.delayed,
            lasting_targets: &self.lasting_targets,
            triggers: &self.triggers,
            batch_starts: &self.batch_starts,
            pending_start: self.pending_start,
            frames: &self.frames,
            frame_vars: &self.frame_vars,
            frame_ints: &self.frame_ints,
            frame_items: &self.frame_items,
            once_used: &self.once_used,
            history: &self.history,
            winner: self.winner,
            end_reason: self.end_reason,
            decklists: [&self.decklists[0], &self.decklists[1]],
            resource_decklists: [&self.resource_decklists[0], &self.resource_decklists[1]],
            redraws: self.redraws,
            action_count: self.action_count,
            event_group: self.event_group,
            effect_hits: &self.effect_hits,
            turn_limit: self.turn_limit,
            max_actions: self.max_actions,
            version: self.version,
        }
    }
}

impl crate::GameState {
    /// The serialised fields of this state for [`encode`].
    pub(crate) fn parts(&self) -> Parts<'_> {
        Parts {
            seed: self.seed,
            rng: self.rng.state,
            cards: &self.cards,
            zones: &self.zones,
            turn: self.turn,
            active: self.active,
            first_player: self.first_player,
            setup_chooser: self.setup_chooser,
            phase: self.phase,
            step: self.step,
            pending: self.pending.as_ref(),
            pending_options: &self.pending_options,
            battles: &self.battles,
            next_battle_id: self.next_battle_id,
            next_lasting_serial: self.next_lasting_serial,
            priority: self.priority,
            passes: self.passes,
            lasting: &self.lasting,
            delayed: &self.delayed,
            lasting_targets: &self.lasting_targets,
            triggers: &self.triggers,
            batch_starts: &self.batch_starts,
            pending_start: self.pending_start,
            frames: &self.frames,
            frame_vars: &self.frame_vars,
            frame_ints: &self.frame_ints,
            frame_items: &self.frame_items,
            once_used: &self.once_used,
            history: &self.history,
            winner: self.winner,
            end_reason: self.end_reason,
            decklists: self.decklists(),
            resource_decklists: self.resource_decklists(),
            redraws: self.redraws,
            action_count: self.action_count,
            event_group: self.event_group,
            effect_hits: &self.effect_hits,
            turn_limit: self.konst.turn_limit,
            max_actions: self.konst.max_actions,
            version: self.version,
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Encoding (runner/digest.py:36-49 over engine/state.py:575-611)

fn int(x: impl Into<Json>) -> Json {
    x.into()
}

fn flag(b: bool) -> Json {
    Json::Int(i128::from(b))
}

fn object(pairs: Vec<(&str, Json)>) -> Json {
    Json::Object(
        pairs
            .into_iter()
            .map(|(k, v)| (Key::Str(k.to_owned()), v))
            .collect(),
    )
}

fn defs(ids: &[DefId]) -> Json {
    Json::Array(ids.iter().map(|d| int(d.0)).collect())
}

/// `[[key, value], ...]` (`state.py:302`, `:380`).
fn event_json(e: &EventPayload) -> Json {
    Json::Array(
        e.pairs()
            .iter()
            .map(|&(k, v)| Json::Array(vec![Json::from(k.as_str()), int(v)]))
            .collect(),
    )
}

/// A once key as a list; a `Cont` element is its content id (`digest.py:30-33`).
fn once_json(k: Option<&OnceKey>, t: &dyn Tables) -> Json {
    Json::Array(
        k.map_or(&[][..], OnceKey::elems)
            .iter()
            .map(|e| match *e {
                OnceElem::Int(x) => int(x),
                OnceElem::Cont(c) => Json::Str(t.cont_cid(c)),
            })
            .collect(),
    )
}

/// `CardInstance.to_json()` (`state.py:118-130`).
fn card_json(uid: usize, c: &CardInstance) -> Json {
    Json::Array(vec![
        int(uid),
        int(c.def_id.0),
        int(c.owner),
        int(c.zone.value()),
        flag(c.rested),
        int(c.damage),
        int(c.pair),
        int(c.zone_seq),
        int(c.entered_turn),
        int(c.known),
    ])
}

/// `Battle.to_json()` (`state.py:173-185`).
fn battle_json(b: &Battle) -> Json {
    Json::Array(vec![
        int(b.attacker),
        int(b.attacker_seq),
        int(b.target),
        int(b.target_seq),
        int(b.defender),
        int(b.battle_id),
        flag(b.blocked),
        flag(b.damage_only),
        flag(b.attack_triggers),
        flag(b.ended),
    ])
}

/// `Lasting.to_json()` (`state.py:231-245`) with the content ids of `digest.py:40-43`.
fn lasting_json(l: &Lasting, targets: &[(Uid, i32)], t: &dyn Tables) -> Json {
    Json::Array(vec![
        Json::Str(t.cont_cid(l.effect)),
        int(l.controller),
        int(l.source_uid),
        Json::Array(
            targets[l.targets.range()]
                .iter()
                .map(|&(u, s)| Json::Array(vec![int(u), int(s)]))
                .collect(),
        ),
        Json::from(l.duration.as_str()),
        int(l.created_turn),
        int(l.battle_id),
        int(l.player),
        int(l.uses),
        l.filters
            .map_or(int(crate::NO_ARG), |f| Json::Str(t.filter_set_cid(f))),
        int(l.aux),
        int(l.serial),
    ])
}

/// `TriggerInst.to_json()` (`state.py:294-306`) with the content id of `digest.py:46-47`.
fn trigger_json(tr: &TriggerInst, t: &dyn Tables) -> Json {
    Json::Array(vec![
        Json::Str(t.program_cid(tr.program)),
        int(tr.controller),
        int(tr.host),
        int(tr.host_seq),
        int(tr.card_uid),
        Json::Array(vec![
            int(tr.ability_key.def_id),
            int(tr.ability_key.aid),
            flag(tr.ability_key.unit_text),
        ]),
        event_json(&tr.event),
        flag(tr.burst),
        once_json(tr.once_key.as_ref(), t),
        flag(tr.optional),
    ])
}

/// `Frame.to_json()` (`state.py:372-390`) with the content id of `digest.py:44-45`: `vars`
/// sorted by name and `ints` by rendered key, both as Python's `sorted(d.items())`.
fn frame_json(f: &Frame, p: &Parts<'_>, t: &dyn Tables) -> Json {
    let mut vars: Vec<(&str, &[i32])> = p.frame_vars[f.vars.range()]
        .iter()
        .map(|e| (t.var_name(e.var), &p.frame_items[e.items.range()]))
        .collect();
    vars.sort_by(|a, b| a.0.cmp(b.0));
    let mut ints: Vec<(Cow<'_, str>, i32)> = p.frame_ints[f.ints.range()]
        .iter()
        .map(|e| (int_key_name(e.key, t), e.value))
        .collect();
    ints.sort_by(|a, b| a.0.cmp(&b.0));
    object(vec![
        ("program_id", Json::Str(t.program_cid(f.program))),
        ("controller", int(f.controller)),
        ("host", int(f.host)),
        ("host_seq", int(f.host_seq)),
        ("card_uid", int(f.card_uid)),
        ("kind", Json::from(f.kind.as_str())),
        ("event", event_json(&f.event)),
        ("pc", int(f.pc)),
        (
            "vars",
            Json::Object(
                vars.into_iter()
                    .map(|(k, items)| {
                        (
                            Key::from(k),
                            Json::Array(items.iter().map(|&x| int(x)).collect()),
                        )
                    })
                    .collect(),
            ),
        ),
        ("did", Json::Bool(f.did)),
        ("answer", Json::Bool(f.answer)),
        ("waiting", Json::Bool(f.waiting)),
        (
            "buf",
            Json::Array(
                p.frame_items[f.buf.range()]
                    .iter()
                    .map(|&x| int(x))
                    .collect(),
            ),
        ),
        (
            "ints",
            Json::Object(
                ints.into_iter()
                    .map(|(k, v)| (Key::Str(k.into_owned()), int(v)))
                    .collect(),
            ),
        ),
        ("once_key", once_json(f.once_key.as_ref(), t)),
        ("acted", Json::Bool(f.acted)),
    ])
}

/// `Decision.to_json()` (`state.py:51-58`).
fn decision_json(hdr: &DecisionHdr, options: &[Action], t: &dyn Tables) -> Json {
    object(vec![
        ("player", int(hdr.player)),
        ("kind", Json::from(hdr.kind.as_str())),
        (
            "options",
            Json::Array(options.iter().map(Action::to_json).collect()),
        ),
        ("prompt", Json::Str(prompt_text(hdr.prompt, t).into_owned())),
        (
            "context",
            Json::Array(
                hdr.context
                    .as_slice()
                    .iter()
                    .map(|&(k, v)| Json::Array(vec![Json::from(k.as_str()), int(v)]))
                    .collect(),
            ),
        ),
    ])
}

/// `canonical_state(st)` (`runner/digest.py:36-49`) of the serialised fields `p`: the 34
/// keys of `GameState.to_json()` in Python's order (`state.py:575-611`).
pub(crate) fn encode(p: &Parts<'_>, t: &dyn Tables) -> Json {
    let zones = Json::Array(
        (0..2u8)
            .map(|pl| {
                Json::Array(
                    Zone::ALL
                        .iter()
                        .map(|&z| Json::Array(p.zones.get(pl, z).iter().map(|&u| int(u)).collect()))
                        .collect(),
                )
            })
            .collect(),
    );
    let lasting = |ls: &[Lasting]| {
        Json::Array(
            ls.iter()
                .map(|l| lasting_json(l, p.lasting_targets, t))
                .collect(),
        )
    };
    let triggers =
        |ts: &[TriggerInst]| Json::Array(ts.iter().map(|tr| trigger_json(tr, t)).collect());
    let batches = Json::Array(
        (0..p.batch_starts.len())
            .map(|i| {
                let end = p
                    .batch_starts
                    .get(i + 1)
                    .copied()
                    .unwrap_or(p.pending_start);
                triggers(&p.triggers[p.batch_starts[i] as usize..end as usize])
            })
            .collect(),
    );
    let mut once: Vec<(String, Json)> = p
        .once_used
        .iter()
        .map(|k| {
            let j = once_json(Some(k), t);
            (dumps(&j, PY_DEFAULT), j)
        })
        .collect();
    once.sort_by(|a, b| a.0.cmp(&b.0));
    let mut hits = p.effect_hits.to_vec();
    hits.sort_by_key(|h| h.0);
    object(vec![
        ("seed", int(p.seed)),
        ("rng", int(p.rng)),
        (
            "cards",
            Json::Array(
                p.cards
                    .iter()
                    .enumerate()
                    .map(|(u, c)| card_json(u, c))
                    .collect(),
            ),
        ),
        ("zones", zones),
        ("turn", int(p.turn)),
        ("active", int(p.active)),
        ("first_player", int(p.first_player)),
        ("setup_chooser", int(p.setup_chooser)),
        ("phase", int(p.phase.value())),
        ("step", int(p.step.value())),
        (
            "pending",
            p.pending
                .map_or(Json::Null, |h| decision_json(h, p.pending_options, t)),
        ),
        (
            "battles",
            Json::Array(p.battles.iter().map(battle_json).collect()),
        ),
        ("next_battle_id", int(p.next_battle_id)),
        ("next_lasting_serial", int(p.next_lasting_serial)),
        ("priority", int(p.priority)),
        ("passes", int(p.passes)),
        ("lasting", lasting(p.lasting)),
        ("delayed", lasting(p.delayed)),
        (
            "pending_triggers",
            triggers(&p.triggers[p.pending_start as usize..]),
        ),
        ("batches", batches),
        (
            "frames",
            Json::Array(p.frames.iter().map(|f| frame_json(f, p, t)).collect()),
        ),
        (
            "once_used",
            Json::Array(once.into_iter().map(|(_, j)| j).collect()),
        ),
        (
            "history",
            Json::Array(
                p.history
                    .iter()
                    .map(|h| {
                        Json::Array(vec![
                            Json::from(h.kind.as_str()),
                            int(h.player),
                            int(h.by),
                            int(h.uid),
                            int(h.def_id),
                            int(h.source),
                        ])
                    })
                    .collect(),
            ),
        ),
        ("winner", p.winner.map_or(Json::Null, int)),
        (
            "end_reason",
            p.end_reason.map_or(Json::Null, |r| Json::from(r.as_str())),
        ),
        (
            "decklists",
            Json::Array(vec![defs(p.decklists[0]), defs(p.decklists[1])]),
        ),
        (
            "resource_decklists",
            Json::Array(vec![
                defs(p.resource_decklists[0]),
                defs(p.resource_decklists[1]),
            ]),
        ),
        (
            "redraws",
            Json::Array(vec![Json::Bool(p.redraws[0]), Json::Bool(p.redraws[1])]),
        ),
        ("action_count", int(p.action_count)),
        ("event_group", int(p.event_group)),
        (
            "effect_hits",
            Json::Array(
                hits.into_iter()
                    .map(|(u, s)| Json::Array(vec![int(u), int(s)]))
                    .collect(),
            ),
        ),
        ("turn_limit", int(p.turn_limit)),
        ("max_actions", int(p.max_actions)),
        ("version", int(p.version)),
    ])
}

// ---------------------------------------------------------------------------------------------
// Decoding (engine/state.py:613-651 over the canonical form; PLAN §4.8)

/// A JSON path, rendered only for an error (`$.cards[3][4]`).
#[derive(Clone, Copy)]
enum Loc<'a> {
    Root,
    /// A side table of the snapshot (`continuous`, `filter_sets`).
    Table(&'static str),
    Key(&'a Loc<'a>, &'a str),
    Index(&'a Loc<'a>, usize),
}

impl<'a> Loc<'a> {
    fn key(&'a self, k: &'a str) -> Self {
        Self::Key(self, k)
    }

    fn ix(&'a self, i: usize) -> Self {
        Self::Index(self, i)
    }

    fn render(&self) -> String {
        match self {
            Self::Root => "$".to_owned(),
            Self::Table(name) => (*name).to_owned(),
            Self::Key(parent, k) => format!("{}.{k}", parent.render()),
            Self::Index(parent, i) => format!("{}[{i}]", parent.render()),
        }
    }

    fn shape(&self, detail: impl Into<String>) -> CanonError {
        CanonError::Shape {
            path: self.render(),
            detail: detail.into(),
        }
    }

    fn value(&self, detail: impl Into<String>) -> CanonError {
        CanonError::Value {
            path: self.render(),
            detail: detail.into(),
        }
    }
}

fn arr<'v>(v: &'v Value, at: &Loc<'_>) -> Result<&'v [Value], CanonError> {
    v.as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| at.shape("expected a list"))
}

fn arr_n<'v>(v: &'v Value, n: usize, at: &Loc<'_>) -> Result<&'v [Value], CanonError> {
    let a = arr(v, at)?;
    if a.len() == n {
        Ok(a)
    } else {
        Err(at.shape(format!("expected a list of {n}, got {}", a.len())))
    }
}

fn obj<'v>(v: &'v Value, at: &Loc<'_>) -> Result<&'v Map<String, Value>, CanonError> {
    v.as_object().ok_or_else(|| at.shape("expected an object"))
}

/// The object `v` with exactly the keys `keys` (in any order).
fn record<'v>(
    v: &'v Value,
    keys: &[&str],
    at: &Loc<'_>,
) -> Result<&'v Map<String, Value>, CanonError> {
    let m = obj(v, at)?;
    if let Some(k) = keys.iter().find(|k| !m.contains_key(**k)) {
        return Err(at.shape(format!("missing key {k:?}")));
    }
    if let Some(k) = m.keys().find(|k| !keys.contains(&k.as_str())) {
        return Err(at.shape(format!("unexpected key {k:?}")));
    }
    Ok(m)
}

fn field<'v>(m: &'v Map<String, Value>, k: &str) -> &'v Value {
    // `record` checked that every key is present.
    &m[k]
}

fn integer(v: &Value, at: &Loc<'_>) -> Result<i64, CanonError> {
    match v {
        Value::Number(n) if n.is_i64() || n.is_u64() => n
            .as_i64()
            .ok_or_else(|| at.value(format!("{n} does not fit an i64"))),
        _ => Err(at.shape(format!("expected an integer, got {v}"))),
    }
}

fn int_as<T: TryFrom<i64>>(v: &Value, at: &Loc<'_>) -> Result<T, CanonError> {
    let x = integer(v, at)?;
    T::try_from(x).map_err(|_| at.value(format!("{x} is out of range")))
}

fn u64_of(v: &Value, at: &Loc<'_>) -> Result<u64, CanonError> {
    match v {
        Value::Number(n) => n
            .as_u64()
            .ok_or_else(|| at.value(format!("{n} is not a u64"))),
        _ => Err(at.shape(format!("expected an integer, got {v}"))),
    }
}

fn player(v: &Value, at: &Loc<'_>) -> Result<u8, CanonError> {
    let p: u8 = int_as(v, at)?;
    if p < 2 {
        Ok(p)
    } else {
        Err(at.value(format!("player {p} is not 0 or 1")))
    }
}

/// A 0/1 bool (cards, battles, triggers).
fn flag_of(v: &Value, at: &Loc<'_>) -> Result<bool, CanonError> {
    match integer(v, at)? {
        0 => Ok(false),
        1 => Ok(true),
        x => Err(at.value(format!("{x} is not 0 or 1"))),
    }
}

/// A `true`/`false` bool (frames, `redraws`).
fn bool_of(v: &Value, at: &Loc<'_>) -> Result<bool, CanonError> {
    v.as_bool()
        .ok_or_else(|| at.shape(format!("expected true or false, got {v}")))
}

fn str_of<'v>(v: &'v Value, at: &Loc<'_>) -> Result<&'v str, CanonError> {
    v.as_str()
        .ok_or_else(|| at.shape(format!("expected a string, got {v}")))
}

fn ints_of(v: &Value, at: &Loc<'_>) -> Result<Vec<i32>, CanonError> {
    let a = arr(v, at)?;
    a.iter()
        .enumerate()
        .map(|(i, x)| int_as(x, &at.ix(i)))
        .collect()
}

fn enum_of<T>(
    v: &Value,
    at: &Loc<'_>,
    what: &str,
    parse: impl Fn(&str) -> Option<T>,
) -> Result<T, CanonError> {
    let s = str_of(v, at)?;
    parse(s).ok_or_else(|| at.value(format!("unknown {what} {s:?}")))
}

fn uid_of(v: &Value, n_cards: usize, at: &Loc<'_>) -> Result<Uid, CanonError> {
    let u: Uid = int_as(v, at)?;
    if usize::from(u) < n_cards {
        Ok(u)
    } else {
        Err(at.value(format!("uid {u} is not a card (the state has {n_cards})")))
    }
}

/// `__rest_as{i}` with `i` written as Python's `f"{i}"` (no sign, no leading zero).
fn rest_as_index(s: &str) -> Option<u16> {
    let digits = s.strip_prefix("__rest_as")?;
    let canonical = !digits.is_empty()
        && digits.bytes().all(|b| b.is_ascii_digit())
        && (digits == "0" || !digits.starts_with('0'));
    if canonical { digits.parse().ok() } else { None }
}

/// Resolves the content id `cid` of a `kind` that the state names at `at` (PLAN §4.8):
/// `by_cid` is the registry's entry with that id, and the side-table entry of `table`, when
/// present, is decoded with `inline` (`None`: it names nothing the registry can hold). When
/// both exist they must be equal; at least one must; and the result must hash back to `cid`
/// (`hash`, `runner/digest.py:26-27`).
fn resolve<T: Copy + PartialEq + std::fmt::Debug>(
    kind: &'static str,
    cid: &str,
    at: &Loc<'_>,
    table: (&'static str, Option<&Map<String, Value>>),
    by_cid: Option<T>,
    inline: impl FnOnce(&Value) -> Result<Option<T>, String>,
    hash: impl FnOnce(T) -> String,
) -> Result<T, CanonError> {
    let unknown = || CanonError::UnknownContent {
        kind,
        id: cid.to_owned(),
    };
    let (name, side) = table;
    let root = Loc::Table(name);
    let entry = root.key(cid);
    let decoded = match side.and_then(|m| m.get(cid)) {
        Some(form) => Some(inline(form).map_err(|e| entry.value(e))?),
        None => None,
    };
    let found = match (by_cid, decoded) {
        (Some(r), Some(Some(s))) if r != s => {
            return Err(entry.value(format!(
                "decodes to {s:?}, but the registry's {kind} with this content id is {r:?}"
            )));
        }
        (Some(r), Some(None)) => {
            return Err(entry.value(format!(
                "names no {kind} of the registry, but the registry's {kind} with this content \
                 id is {r:?}"
            )));
        }
        (Some(r), _) => r,
        (None, Some(Some(s))) => s,
        (None, _) => return Err(unknown()),
    };
    let got = hash(found);
    if got != cid {
        return Err(at.value(format!("{kind} {found:?} has content id {got}, not {cid}")));
    }
    Ok(found)
}

/// Decodes canonical snapshots against a registry's [`Tables`].
struct Decoder<'a> {
    t: &'a dyn Tables,
    continuous: Option<&'a Map<String, Value>>,
    filter_sets: Option<&'a Map<String, Value>>,
}

impl Decoder<'_> {
    fn program(&self, v: &Value, at: &Loc<'_>) -> Result<ProgramId, CanonError> {
        let cid = str_of(v, at)?;
        self.t
            .program_by_cid(cid)
            .ok_or_else(|| CanonError::UnknownContent {
                kind: "program",
                id: cid.to_owned(),
            })
    }

    /// A continuous effect by content id (PLAN §4.8): the registry's effect with that id or
    /// the one its side-table node spells (both, and equal, when both exist), hashing back to
    /// `cid`.
    fn cont(&self, cid: &str, at: &Loc<'_>) -> Result<ContRef, CanonError> {
        resolve(
            "continuous",
            cid,
            at,
            ("continuous", self.continuous),
            self.t.cont_by_cid(cid),
            |form| self.t.cont_inline(form),
            |c| self.t.cont_cid(c),
        )
    }

    /// A filter set by content id: the registry's filter set with that id or the one its
    /// side-table filter nodes form (both, and equal, when both exist), hashing back to `cid`.
    fn filter_set(&self, cid: &str, at: &Loc<'_>) -> Result<FilterSetId, CanonError> {
        resolve(
            "filter_set",
            cid,
            at,
            ("filter_sets", self.filter_sets),
            self.t.filter_set_by_cid(cid),
            |form| self.t.filter_set_inline(form),
            |f| self.t.filter_set_cid(f),
        )
    }

    fn event(&self, v: &Value, at: &Loc<'_>) -> Result<EventPayload, CanonError> {
        let mut e = EventPayload::default();
        for (i, pair) in arr(v, at)?.iter().enumerate() {
            let at = at.ix(i);
            let kv = arr_n(pair, 2, &at)?;
            let key = enum_of(&kv[0], &at.ix(0), "event key", EvKey::from_value)?;
            let value = int_as(&kv[1], &at.ix(1))?;
            e.0.try_push((key, value))
                .map_err(|_| at.value(format!("more than {EVENT_CAPACITY} event pairs")))?;
        }
        Ok(e)
    }

    fn once_key(&self, v: &Value, at: &Loc<'_>) -> Result<Option<OnceKey>, CanonError> {
        let a = arr(v, at)?;
        if a.is_empty() {
            return Ok(None);
        }
        let mut k = OnceKey::default();
        for (i, x) in a.iter().enumerate() {
            let at = at.ix(i);
            let e = match x {
                Value::String(cid) => OnceElem::Cont(self.cont(cid, &at)?),
                _ => OnceElem::Int(int_as(x, &at)?),
            };
            k.0.try_push(e).map_err(|_| {
                at.value(format!("more than {ONCE_KEY_CAPACITY} once-key elements"))
            })?;
        }
        Ok(Some(k))
    }

    fn card(
        &self,
        v: &Value,
        uid: usize,
        n_defs: usize,
        at: &Loc<'_>,
    ) -> Result<CardInstance, CanonError> {
        let a = arr_n(v, 10, at)?;
        let got: i64 = integer(&a[0], &at.ix(0))?;
        if usize::try_from(got).ok() != Some(uid) {
            return Err(at.ix(0).value(format!("card {got} at index {uid}")));
        }
        let def: u16 = int_as(&a[1], &at.ix(1))?;
        if usize::from(def) >= n_defs {
            return Err(at
                .ix(1)
                .value(format!("def_id {def} beyond the {n_defs} definitions")));
        }
        Ok(CardInstance {
            def_id: DefId(def),
            owner: player(&a[2], &at.ix(2))?,
            zone: Zone::from_value(integer(&a[3], &at.ix(3))?)
                .ok_or_else(|| at.ix(3).value("unknown zone"))?,
            rested: flag_of(&a[4], &at.ix(4))?,
            damage: int_as(&a[5], &at.ix(5))?,
            pair: int_as(&a[6], &at.ix(6))?,
            zone_seq: int_as(&a[7], &at.ix(7))?,
            entered_turn: int_as(&a[8], &at.ix(8))?,
            known: int_as(&a[9], &at.ix(9))?,
        })
    }

    fn battle(&self, v: &Value, at: &Loc<'_>) -> Result<Battle, CanonError> {
        let a = arr_n(v, 10, at)?;
        Ok(Battle {
            attacker: int_as(&a[0], &at.ix(0))?,
            attacker_seq: int_as(&a[1], &at.ix(1))?,
            target: int_as(&a[2], &at.ix(2))?,
            target_seq: int_as(&a[3], &at.ix(3))?,
            defender: player(&a[4], &at.ix(4))?,
            battle_id: int_as(&a[5], &at.ix(5))?,
            blocked: flag_of(&a[6], &at.ix(6))?,
            damage_only: flag_of(&a[7], &at.ix(7))?,
            attack_triggers: flag_of(&a[8], &at.ix(8))?,
            ended: flag_of(&a[9], &at.ix(9))?,
        })
    }

    fn lasting(
        &self,
        v: &Value,
        n_cards: usize,
        targets: &mut Vec<(Uid, i32)>,
        at: &Loc<'_>,
    ) -> Result<Lasting, CanonError> {
        let a = arr_n(v, 12, at)?;
        let effect = self.cont(str_of(&a[0], &at.ix(0))?, &at.ix(0))?;
        let start = targets.len();
        let tl = at.ix(3);
        for (i, pair) in arr(&a[3], &tl)?.iter().enumerate() {
            let at = tl.ix(i);
            let us = arr_n(pair, 2, &at)?;
            targets.push((
                uid_of(&us[0], n_cards, &at.ix(0))?,
                int_as(&us[1], &at.ix(1))?,
            ));
        }
        let span = span_from(start, targets.len(), &tl)?;
        let filters = match &a[9] {
            Value::String(cid) => Some(self.filter_set(cid, &at.ix(9))?),
            other => match integer(other, &at.ix(9))? {
                -1 => None,
                x => {
                    return Err(at
                        .ix(9)
                        .value(format!("filters {x} is neither a content id nor -1")));
                }
            },
        };
        Ok(Lasting {
            effect,
            controller: player(&a[1], &at.ix(1))?,
            source_uid: int_as(&a[2], &at.ix(2))?,
            targets: span,
            duration: enum_of(&a[4], &at.ix(4), "duration", Duration::from_value)?,
            created_turn: int_as(&a[5], &at.ix(5))?,
            battle_id: int_as(&a[6], &at.ix(6))?,
            player: int_as(&a[7], &at.ix(7))?,
            uses: int_as(&a[8], &at.ix(8))?,
            filters,
            aux: int_as(&a[10], &at.ix(10))?,
            serial: int_as(&a[11], &at.ix(11))?,
        })
    }

    fn trigger(&self, v: &Value, at: &Loc<'_>) -> Result<TriggerInst, CanonError> {
        let a = arr_n(v, 10, at)?;
        let ak = arr_n(&a[5], 3, &at.ix(5))?;
        let akl = at.ix(5);
        Ok(TriggerInst {
            program: self.program(&a[0], &at.ix(0))?,
            controller: player(&a[1], &at.ix(1))?,
            host: int_as(&a[2], &at.ix(2))?,
            host_seq: int_as(&a[3], &at.ix(3))?,
            card_uid: int_as(&a[4], &at.ix(4))?,
            ability_key: AbilityKey {
                def_id: int_as(&ak[0], &akl.ix(0))?,
                aid: int_as(&ak[1], &akl.ix(1))?,
                unit_text: flag_of(&ak[2], &akl.ix(2))?,
            },
            event: self.event(&a[6], &at.ix(6))?,
            burst: flag_of(&a[7], &at.ix(7))?,
            once_key: self.once_key(&a[8], &at.ix(8))?,
            optional: flag_of(&a[9], &at.ix(9))?,
        })
    }

    fn int_key(&self, s: &str, at: &Loc<'_>) -> Result<IntKey, CanonError> {
        match s {
            HOLD_RULES => Ok(IntKey::HoldRules),
            "__rest_i" => Ok(IntKey::RestI),
            "ex_used" => Ok(IntKey::ExUsed),
            _ => {
                if let Some(i) = rest_as_index(s) {
                    Ok(IntKey::RestAs(i))
                } else if s.starts_with("__loop")
                    && let Some(v) = self.t.var_id(s)
                {
                    Ok(IntKey::Loop(v))
                } else {
                    Err(at.value(format!("unknown frame int key {s:?}")))
                }
            }
        }
    }

    fn frame(&self, v: &Value, body: &mut StateBody, at: &Loc<'_>) -> Result<Frame, CanonError> {
        let m = record(v, &FRAME_JSON_KEYS, at)?;
        let fl = |k: &'static str| (field(m, k), k);
        let vstart = body.frame_vars.len();
        let (vars, vk) = fl("vars");
        let vl = at.key(vk);
        for (name, items) in obj(vars, &vl)? {
            let il = vl.key(name);
            let var = self
                .t
                .var_id(name)
                .ok_or_else(|| il.value(format!("unknown frame variable {name:?}")))?;
            let data = ints_of(items, &il)?;
            let span = push_span(&mut body.frame_items, &data, &il)?;
            body.frame_vars.push(VarEntry { var, items: span });
        }
        let vars = span_from(vstart, body.frame_vars.len(), &vl)?;
        let (buf, bk) = fl("buf");
        let buf = push_span(
            &mut body.frame_items,
            &ints_of(buf, &at.key(bk))?,
            &at.key(bk),
        )?;
        let (ints, ik) = fl("ints");
        let il = at.key(ik);
        let istart = body.frame_ints.len();
        for (name, value) in obj(ints, &il)? {
            let kl = il.key(name);
            body.frame_ints.push(IntEntry {
                key: self.int_key(name, &kl)?,
                value: int_as(value, &kl)?,
            });
        }
        let ints = span_from(istart, body.frame_ints.len(), &il)?;
        let at_k = |k: &'static str| at.key(k);
        Ok(Frame {
            program: self.program(field(m, "program_id"), &at_k("program_id"))?,
            controller: player(field(m, "controller"), &at_k("controller"))?,
            host: int_as(field(m, "host"), &at_k("host"))?,
            host_seq: int_as(field(m, "host_seq"), &at_k("host_seq"))?,
            card_uid: int_as(field(m, "card_uid"), &at_k("card_uid"))?,
            kind: enum_of(
                field(m, "kind"),
                &at_k("kind"),
                "frame kind",
                FrameKind::from_value,
            )?,
            event: self.event(field(m, "event"), &at_k("event"))?,
            pc: int_as(field(m, "pc"), &at_k("pc"))?,
            vars,
            did: bool_of(field(m, "did"), &at_k("did"))?,
            answer: bool_of(field(m, "answer"), &at_k("answer"))?,
            waiting: bool_of(field(m, "waiting"), &at_k("waiting"))?,
            buf,
            ints,
            once_key: self.once_key(field(m, "once_key"), &at_k("once_key"))?,
            acted: bool_of(field(m, "acted"), &at_k("acted"))?,
        })
    }

    /// The prompt of a decoded decision: the waiting frame's `AskMay`/`Choose` when its text
    /// is the snapshot's, else an engine literal (contracts.md: `from_canonical` recovers `Ask`
    /// and `Choose` from the waiting frame's instruction).
    fn prompt(&self, text: &str, frames: &[Frame], at: &Loc<'_>) -> Result<Prompt, CanonError> {
        if let Some(f) = frames.iter().rev().find(|f| f.waiting)
            && let Some(p) = self.t.decision_prompt(f.program, f.pc)
            && prompt_text(p, self.t) == text
        {
            return Ok(p);
        }
        EnginePrompt::from_value(text)
            .map(Prompt::Engine)
            .ok_or_else(|| {
                at.value(format!(
                    "prompt {text:?} is neither the waiting frame's nor an engine literal"
                ))
            })
    }

    fn decision(&self, v: &Value, body: &mut StateBody, at: &Loc<'_>) -> Result<(), CanonError> {
        let m = record(v, &DECISION_JSON_KEYS, at)?;
        let ol = at.key("options");
        for (i, o) in arr(field(m, "options"), &ol)?.iter().enumerate() {
            let at = ol.ix(i);
            let a = arr_n(o, 4, &at)?;
            body.pending_options.push(Action {
                kind: enum_of(&a[0], &at.ix(0), "action kind", ActionKind::from_value)?,
                a: int_as(&a[1], &at.ix(1))?,
                b: int_as(&a[2], &at.ix(2))?,
                c: int_as(&a[3], &at.ix(3))?,
            });
        }
        let pl = at.key("prompt");
        let prompt = self.prompt(str_of(field(m, "prompt"), &pl)?, &body.frames, &pl)?;
        let mut hdr = DecisionHdr {
            player: player(field(m, "player"), &at.key("player"))?,
            kind: enum_of(
                field(m, "kind"),
                &at.key("kind"),
                "decision kind",
                DecisionKind::from_value,
            )?,
            prompt,
            context: gcg_core::small::InlineVec::new(),
        };
        let cl = at.key("context");
        for (i, pair) in arr(field(m, "context"), &cl)?.iter().enumerate() {
            let at = cl.ix(i);
            let kv = arr_n(pair, 2, &at)?;
            let key = enum_of(&kv[0], &at.ix(0), "context key", CtxKey::from_value)?;
            hdr.context
                .try_push((key, int_as(&kv[1], &at.ix(1))?))
                .map_err(|_| at.value(format!("more than {CONTEXT_CAPACITY} context pairs")))?;
        }
        body.pending = Some(hdr);
        Ok(())
    }

    fn decode(&self, state: &Value) -> Result<StateBody, CanonError> {
        let root = Loc::Root;
        let m = record(state, &STATE_JSON_KEYS, &root)?;
        let k = |name: &'static str| (field(m, name), root.key(name));
        let n_defs = self.t.def_count();

        let (v, at) = k("cards");
        let cards = arr(v, &at)?
            .iter()
            .enumerate()
            .map(|(u, c)| self.card(c, u, n_defs, &at.ix(u)))
            .collect::<Result<Vec<_>, _>>()?;
        if cards.len() > usize::from(Uid::MAX) + 1 {
            return Err(at.value("more cards than u16 uids"));
        }
        let n = cards.len();

        let (v, at) = k("zones");
        let mut zones = ZoneLists::new();
        for (p, pz) in arr_n(v, 2, &at)?.iter().enumerate() {
            let pl = at.ix(p);
            for (zi, list) in arr_n(pz, Zone::ALL.len(), &pl)?.iter().enumerate() {
                let zl = pl.ix(zi);
                let uids = arr(list, &zl)?
                    .iter()
                    .enumerate()
                    .map(|(i, u)| uid_of(u, n, &zl.ix(i)))
                    .collect::<Result<Vec<_>, _>>()?;
                let pu8 = u8::from(p == 1);
                zones
                    .set(pu8, Zone::ALL[zi], &uids)
                    .map_err(|e| zl.value(e.to_string()))?;
            }
        }

        let scalar_i32 = |name: &'static str| -> Result<i32, CanonError> {
            let (v, at) = k(name);
            int_as(v, &at)
        };
        let scalar_i64 = |name: &'static str| -> Result<i64, CanonError> {
            let (v, at) = k(name);
            integer(v, &at)
        };
        let scalar_player = |name: &'static str| -> Result<u8, CanonError> {
            let (v, at) = k(name);
            player(v, &at)
        };

        let mut body = StateBody {
            seed: {
                let (v, at) = k("seed");
                u64_of(v, &at)?
            },
            rng: {
                let (v, at) = k("rng");
                u64_of(v, &at)?
            },
            cards,
            zones,
            turn: scalar_i32("turn")?,
            active: scalar_player("active")?,
            first_player: scalar_player("first_player")?,
            setup_chooser: scalar_player("setup_chooser")?,
            phase: {
                let (v, at) = k("phase");
                Phase::from_value(integer(v, &at)?).ok_or_else(|| at.value("unknown phase"))?
            },
            step: {
                let (v, at) = k("step");
                Step::from_value(integer(v, &at)?).ok_or_else(|| at.value("unknown step"))?
            },
            pending: None,
            pending_options: Vec::new(),
            battles: {
                let (v, at) = k("battles");
                arr(v, &at)?
                    .iter()
                    .enumerate()
                    .map(|(i, b)| self.battle(b, &at.ix(i)))
                    .collect::<Result<_, _>>()?
            },
            next_battle_id: scalar_i32("next_battle_id")?,
            next_lasting_serial: scalar_i32("next_lasting_serial")?,
            priority: scalar_player("priority")?,
            passes: scalar_i32("passes")?,
            lasting: Vec::new(),
            delayed: Vec::new(),
            lasting_targets: Vec::new(),
            triggers: Vec::new(),
            batch_starts: Vec::new(),
            pending_start: 0,
            frames: Vec::new(),
            frame_vars: Vec::new(),
            frame_ints: Vec::new(),
            frame_items: Vec::new(),
            once_used: Vec::new(),
            history: Vec::new(),
            winner: {
                let (v, at) = k("winner");
                if v.is_null() {
                    None
                } else {
                    Some(int_as(v, &at)?)
                }
            },
            end_reason: {
                let (v, at) = k("end_reason");
                if v.is_null() {
                    None
                } else {
                    Some(enum_of(v, &at, "end reason", EndReason::from_value)?)
                }
            },
            decklists: [Box::new([]), Box::new([])],
            resource_decklists: [Box::new([]), Box::new([])],
            redraws: {
                let (v, at) = k("redraws");
                let a = arr_n(v, 2, &at)?;
                [bool_of(&a[0], &at.ix(0))?, bool_of(&a[1], &at.ix(1))?]
            },
            action_count: scalar_i64("action_count")?,
            event_group: scalar_i64("event_group")?,
            effect_hits: Vec::new(),
            turn_limit: scalar_i32("turn_limit")?,
            max_actions: scalar_i64("max_actions")?,
            version: {
                let (v, at) = k("version");
                u64_of(v, &at)?
            },
        };

        for (name, into_delayed) in [("lasting", false), ("delayed", true)] {
            let (v, at) = k(name);
            for (i, l) in arr(v, &at)?.iter().enumerate() {
                let le = self.lasting(l, n, &mut body.lasting_targets, &at.ix(i))?;
                if into_delayed {
                    body.delayed.push(le);
                } else {
                    body.lasting.push(le);
                }
            }
        }

        let (v, at) = k("batches");
        for (i, batch) in arr(v, &at)?.iter().enumerate() {
            let bl = at.ix(i);
            body.batch_starts.push(
                u32::try_from(body.triggers.len()).map_err(|_| bl.value("too many triggers"))?,
            );
            for (j, tr) in arr(batch, &bl)?.iter().enumerate() {
                body.triggers.push(self.trigger(tr, &bl.ix(j))?);
            }
        }
        body.pending_start =
            u32::try_from(body.triggers.len()).map_err(|_| at.value("too many triggers"))?;
        let (v, at) = k("pending_triggers");
        for (i, tr) in arr(v, &at)?.iter().enumerate() {
            body.triggers.push(self.trigger(tr, &at.ix(i))?);
        }

        let (v, at) = k("frames");
        for (i, f) in arr(v, &at)?.iter().enumerate() {
            let frame = self.frame(f, &mut body, &at.ix(i))?;
            body.frames.push(frame);
        }

        let (v, at) = k("pending");
        if !v.is_null() {
            self.decision(v, &mut body, &at)?;
        }

        let (v, at) = k("once_used");
        for (i, key) in arr(v, &at)?.iter().enumerate() {
            let kl = at.ix(i);
            let Some(key) = self.once_key(key, &kl)? else {
                return Err(kl.value("an empty once key"));
            };
            if body.once_used.contains(&key) {
                return Err(kl.value("a once key listed twice (a Python set)"));
            }
            body.once_used.push(key);
        }

        let (v, at) = k("history");
        for (i, h) in arr(v, &at)?.iter().enumerate() {
            let hl = at.ix(i);
            let a = arr_n(h, 6, &hl)?;
            body.history.push(HistoryEvent {
                kind: enum_of(&a[0], &hl.ix(0), "history kind", HistKind::from_value)?,
                player: player(&a[1], &hl.ix(1))?,
                by: int_as(&a[2], &hl.ix(2))?,
                uid: int_as(&a[3], &hl.ix(3))?,
                def_id: int_as(&a[4], &hl.ix(4))?,
                source: int_as(&a[5], &hl.ix(5))?,
            });
        }

        let decks = |name: &'static str| -> Result<[Box<[DefId]>; 2], CanonError> {
            let (v, at) = k(name);
            let a = arr_n(v, 2, &at)?;
            let one = |p: usize| -> Result<Box<[DefId]>, CanonError> {
                let pl = at.ix(p);
                arr(&a[p], &pl)?
                    .iter()
                    .enumerate()
                    .map(|(i, d)| {
                        let dl = pl.ix(i);
                        let def: u16 = int_as(d, &dl)?;
                        if usize::from(def) < n_defs {
                            Ok(DefId(def))
                        } else {
                            Err(dl.value(format!("def_id {def} beyond the {n_defs} definitions")))
                        }
                    })
                    .collect()
            };
            Ok([one(0)?, one(1)?])
        };
        body.decklists = decks("decklists")?;
        body.resource_decklists = decks("resource_decklists")?;

        let (v, at) = k("effect_hits");
        for (i, pair) in arr(v, &at)?.iter().enumerate() {
            let hl = at.ix(i);
            let us = arr_n(pair, 2, &hl)?;
            let uid = uid_of(&us[0], n, &hl.ix(0))?;
            if body.effect_hits.iter().any(|h| h.0 == uid) {
                return Err(hl.value(format!("uid {uid} listed twice (a Python dict)")));
            }
            body.effect_hits.push((uid, uid_of(&us[1], n, &hl.ix(1))?));
        }
        Ok(body)
    }
}

fn push_span(items: &mut Vec<i32>, data: &[i32], at: &Loc<'_>) -> Result<Span, CanonError> {
    let start = items.len();
    items.extend_from_slice(data);
    span_from(start, items.len(), at)
}

/// The arena region `start..end`, as the arena helpers lay it out (an empty one is
/// `Span::EMPTY`).
fn span_from(start: usize, end: usize, at: &Loc<'_>) -> Result<Span, CanonError> {
    crate::state::region(start, end - start, "snapshot arena")
        .map_err(|_| at.value("arena beyond u32 offsets"))
}

/// Decodes `snap.state` (a canonical form) with the side tables of the snapshot
/// (PLAN §4.8): content ids resolve through `t` and the inline nodes of `snap.continuous` and
/// `snap.filter_sets` (see [`resolve`]).
///
/// # Errors
/// [`CanonError`] naming the JSON path of the first malformed or unknown value.
pub(crate) fn decode(t: &dyn Tables, snap: &StateSnapshot) -> Result<StateBody, CanonError> {
    let d = Decoder {
        t,
        continuous: side_table(&snap.continuous, "continuous")?,
        filter_sets: side_table(&snap.filter_sets, "filter_sets")?,
    };
    d.decode(&snap.state)
}

/// A snapshot side table: an object keyed by content id, or `null` for none.
fn side_table<'v>(
    v: &'v Value,
    name: &'static str,
) -> Result<Option<&'v Map<String, Value>>, CanonError> {
    match v {
        Value::Null => Ok(None),
        Value::Object(m) => Ok(Some(m)),
        _ => Err(CanonError::Shape {
            path: name.to_owned(),
            detail: "expected an object of content ids".to_owned(),
        }),
    }
}

#[cfg(test)]
pub(crate) mod fixtures {
    //! The canonical states that `oracle/export_state_fixtures.py` exports with Python's
    //! digests (`tests/fixtures/states.ndjson`), and a registry that holds all of them.

    use std::sync::{Arc, OnceLock};

    use gcg_data::CardDb;
    use gcg_effects::{InlineNode, Registry};
    use serde_json::{Value, json};

    use crate::{GameState, StateSnapshot};

    const FIXTURES: &str = include_str!("../tests/fixtures/states.ndjson");
    const IR: &str = include_str!("../../gcg-effects/ir/registry.json");

    pub(crate) fn snapshots() -> &'static [Value] {
        static SNAPSHOTS: OnceLock<Vec<Value>> = OnceLock::new();
        SNAPSHOTS.get_or_init(|| {
            FIXTURES
                .lines()
                .map(|l| serde_json::from_str(l).unwrap())
                .collect()
        })
    }

    /// The embedded registry with the filter sets that the fixtures name and the IR lacks
    /// (Python registers filter tuples at run time, `effects/registry.py:132-138`; the IR has
    /// none yet), added to a copy of the IR under Python's content ids.
    pub(crate) fn registry() -> &'static Arc<Registry> {
        static REGISTRY: OnceLock<Arc<Registry>> = OnceLock::new();
        REGISTRY.get_or_init(|| {
            let base = Registry::embedded();
            let mut ir: Value = serde_json::from_str(IR).unwrap();
            let mut sets = ir["filter_sets"].as_array().unwrap().clone();
            for s in snapshots() {
                for (cid, filters) in s["filter_sets"].as_object().unwrap() {
                    if sets.iter().any(|x| x["content_id"] == *cid) {
                        continue;
                    }
                    let ids: Vec<Value> = filters
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|f| match base.decode_inline(f) {
                            Ok(InlineNode::Arena(id)) => Value::from(id.0),
                            other => panic!("filter {f} is not an IR node: {other:?}"),
                        })
                        .collect();
                    sets.push(json!({"filters": ids, "content_id": cid}));
                }
            }
            ir["filter_sets"] = Value::Array(sets);
            Arc::new(Registry::from_ir(&ir.to_string(), CardDb::embedded()).unwrap())
        })
    }

    pub(crate) fn by_id(id: &str) -> &'static Value {
        snapshots().iter().find(|r| r["id"] == id).unwrap()
    }

    pub(crate) fn snapshot(rec: &Value) -> StateSnapshot {
        StateSnapshot {
            state: rec["state"].clone(),
            continuous: rec["continuous"].clone(),
            filter_sets: rec["filter_sets"].clone(),
            rules_checked: rec["rules_checked"].as_i64().unwrap(),
        }
    }

    /// Every fixture snapshot rebuilt with `canon::from_canonical`, with its record.
    pub(crate) fn states() -> Vec<(&'static Value, GameState)> {
        snapshots()
            .iter()
            .map(|r| {
                let st = super::from_canonical(registry(), &snapshot(r))
                    .unwrap_or_else(|e| panic!("{}: {e}", r["id"]));
                (r, st)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    //! M5.1 without the corpus: Python-exported canonical states
    //! (`oracle/export_state_fixtures.py`) decode and re-encode byte for byte, with
    //! Python's digests, against the embedded effect registry (the public round trip is
    //! `tests/canonical_fixtures.rs`).

    use gcg_core::prop;
    use serde_json::json;

    use super::fixtures::{by_id, snapshot, snapshots};
    use super::*;
    use crate::NO_ARG;
    use crate::state::FrameArena;

    fn registry() -> &'static Registry {
        super::fixtures::registry()
    }

    /// Python's JSON value as a `gcg_core` value (insertion order kept; no floats occur).
    fn to_json(v: &Value) -> Json {
        match v {
            Value::Null => Json::Null,
            Value::Bool(b) => Json::Bool(*b),
            Value::Number(n) => Json::Int(
                n.as_i64()
                    .map_or_else(|| i128::from(n.as_u64().expect("an integer")), i128::from),
            ),
            Value::String(s) => Json::Str(s.clone()),
            Value::Array(a) => Json::Array(a.iter().map(to_json).collect()),
            Value::Object(m) => Json::Object(
                m.iter()
                    .map(|(k, v)| (Key::Str(k.clone()), to_json(v)))
                    .collect(),
            ),
        }
    }

    fn first_difference(a: &str, b: &str) -> String {
        let at = a
            .bytes()
            .zip(b.bytes())
            .position(|(x, y)| x != y)
            .unwrap_or(a.len().min(b.len()));
        let lo = at.saturating_sub(80);
        format!(
            "first difference at byte {at}:\n  python: …{}…\n  rust:   …{}…",
            &a[lo..(at + 80).min(a.len())],
            &b[lo..(at + 80).min(b.len())]
        )
    }

    fn keys(j: &Json) -> Vec<&str> {
        match j {
            Json::Object(items) => items
                .iter()
                .map(|(k, _)| match k {
                    Key::Str(s) => s.as_str(),
                    Key::Int(_) => panic!("int key"),
                })
                .collect(),
            _ => panic!("not an object"),
        }
    }

    /// T-045 on the fixture snapshots: decode, re-encode, compare Python's bytes and digest.
    #[test]
    fn python_snapshots_round_trip_byte_for_byte() {
        let t = registry();
        assert_eq!(snapshots().len(), 69);
        for rec in snapshots() {
            let id = rec["id"].as_str().unwrap();
            let python = to_json(&rec["state"]);
            let python_text = dumps(&python, PY_DEFAULT);
            // The reader is faithful: the bytes and the digest are Python's own.
            assert_eq!(
                sha256_hex(python_text.as_bytes()),
                rec["dump_sha256"],
                "{id}"
            );
            assert_eq!(digest_of(&python), rec["digest"], "{id}");
            let body = decode(t, &snapshot(rec)).unwrap_or_else(|e| panic!("{id}: {e}"));
            let rust = encode(&body.parts(), t);
            let rust_text = dumps(&rust, PY_DEFAULT);
            assert!(
                rust_text == python_text,
                "{id}: {}",
                first_difference(&python_text, &rust_text)
            );
            assert_eq!(digest_of(&rust), rec["digest"], "{id}");
        }
    }

    #[test]
    fn records_keep_pythons_key_orders_and_bool_encodings() {
        let t = registry();
        let body = decode(t, &snapshot(by_id("synthetic/rare-shapes"))).unwrap();
        let j = encode(&body.parts(), t);
        assert_eq!(keys(&j), STATE_JSON_KEYS);
        let frames = j.get("frames").unwrap();
        let Json::Array(frames) = frames else {
            panic!()
        };
        assert_eq!(keys(&frames[0]), FRAME_JSON_KEYS);
        assert_eq!(keys(j.get("pending").unwrap()), DECISION_JSON_KEYS);
        // __rest_as10 < __rest_as2 < __rest_i: sorted as strings (state.py:387).
        assert_eq!(
            keys(frames[0].get("ints").unwrap()),
            ["__loop0", "__rest_as10", "__rest_as2", "__rest_i"]
        );
        assert_eq!(
            keys(frames[0].get("vars").unwrap())[..2],
            ["__todeck", "pick"]
        );
        assert_eq!(frames[0].get("did"), Some(&Json::Bool(true)));
        // Battles, triggers and cards write their bools as 0/1.
        let text = dumps(&j, PY_DEFAULT);
        assert!(
            text.contains("\"battles\": [[7, 1, 9, 2, 1, 40, 0, 1, 0, 1]]"),
            "{text}"
        );
        // once_used is sorted by json.dumps text: "-7" < "-8" < "-9" < "10" < "2".
        let once = dumps(j.get("once_used").unwrap(), PY_DEFAULT);
        assert!(once.starts_with("[[-7, 0, 5, 4, 1], [-8, -1, \""), "{once}");
        assert!(
            once.ends_with("[-9, 3, 4], [10, 2, 1], [2, 1, 1]]"),
            "{once}"
        );
        assert_eq!(body.effect_hits, [(2, 7), (5, 9)]);
        assert!(body.lasting.iter().any(|l| l.filters.is_some()));
    }

    #[test]
    fn prompts_are_recovered_from_the_waiting_frame() {
        let t = registry();
        let mut seen = Vec::new();
        for rec in snapshots() {
            let body = decode(t, &snapshot(rec)).unwrap();
            if let Some(hdr) = body.pending {
                let text = rec["state"]["pending"]["prompt"].as_str().unwrap();
                assert_eq!(prompt_text(hdr.prompt, t), text);
                assert_eq!(hdr.prompt.text(t), text);
                let kind = match hdr.prompt {
                    Prompt::Engine(EnginePrompt::YouMay) => "you may".to_owned(),
                    Prompt::Engine(_) => "engine".to_owned(),
                    Prompt::Ask(_) => format!("ask {text}"),
                    Prompt::Choose(_) => "choose".to_owned(),
                };
                if !seen.contains(&kind) {
                    seen.push(kind);
                }
            }
        }
        seen.sort();
        assert_eq!(
            seen,
            [
                "ask activate Burst?",
                "ask pay ①?",
                "choose",
                "engine",
                "you may"
            ]
        );
        // ensure_ascii in the digest blob (digest.py:55).
        let rec = snapshots()
            .iter()
            .find(|r| r["state"]["pending"]["prompt"] == "pay ①?")
            .unwrap();
        let blob = dumps(&to_json(&rec["state"]), DumpOptions::COMPACT_SORTED_ASCII);
        assert!(blob.contains("\"prompt\":\"pay \\u2460?\""));
    }

    #[test]
    fn only_ask_may_and_choose_instructions_ask_prompts() {
        let t = registry();
        // Program 0 is `keyword:Repair`, whose instructions ask nothing.
        let repair = t.keyword_program(gcg_effects::KeywordProgram::Repair);
        assert_eq!(t.decision_prompt(repair, 0), None);
        assert_eq!(t.decision_prompt(repair, u16::MAX), None);
    }

    fn mutated(rec: &Value, edit: impl FnOnce(&mut Value)) -> StateSnapshot {
        let mut s = snapshot(rec);
        edit(&mut s.state);
        s
    }

    /// A change to a snapshot's `state`, and the error it must produce.
    type Edit = Box<dyn Fn(&mut Value)>;
    type Check = Box<dyn Fn(&CanonError) -> bool>;

    fn decode_err(t: &dyn Tables, s: &StateSnapshot) -> CanonError {
        decode(t, s).expect_err("the snapshot must be rejected")
    }

    fn shape(path: String) -> Check {
        Box::new(move |e| matches!(e, CanonError::Shape { path: p, .. } if *p == path))
    }

    fn value(path: String) -> Check {
        Box::new(move |e| matches!(e, CanonError::Value { path: p, .. } if *p == path))
    }

    #[test]
    fn malformed_snapshots_name_the_path() {
        let t = registry();
        let rec = by_id("synthetic/rare-shapes");
        let n_once = rec["state"]["once_used"].as_array().unwrap().len();
        let p = |s: &str| s.to_owned();
        let cases: Vec<(Edit, Check)> = vec![
            (
                Box::new(|s| {
                    s.as_object_mut().unwrap().shift_remove("seed");
                }),
                shape(p("$")),
            ),
            (
                Box::new(|s| {
                    s["extra"] = Value::Null;
                }),
                shape(p("$")),
            ),
            (
                Box::new(|s| s["cards"][3][0] = 4.into()),
                value(p("$.cards[3][0]")),
            ),
            (
                Box::new(|s| s["cards"][3][4] = true.into()),
                shape(p("$.cards[3][4]")),
            ),
            (
                Box::new(|s| s["cards"][3][4] = 2.into()),
                value(p("$.cards[3][4]")),
            ),
            (
                Box::new(|s| s["cards"][3][1] = 5000.into()),
                value(p("$.cards[3][1]")),
            ),
            (
                Box::new(|s| s["zones"][0][7][0] = 9999.into()),
                value(p("$.zones[0][7][0]")),
            ),
            (
                Box::new(|s| s["frames"][0]["did"] = 1.into()),
                shape(p("$.frames[0].did")),
            ),
            (
                Box::new(|s| {
                    s["frames"][0]["ints"]["__rest_as01"] = 3.into();
                }),
                value(p("$.frames[0].ints.__rest_as01")),
            ),
            (
                Box::new(|s| {
                    s["frames"][0]["ints"]["__loop3"] = 3.into();
                }),
                value(p("$.frames[0].ints.__loop3")),
            ),
            (
                Box::new(|s| {
                    s["frames"][0]["vars"]["no_such_var"] = Value::Array(vec![]);
                }),
                value(p("$.frames[0].vars.no_such_var")),
            ),
            (
                Box::new(|s| s["pending"]["prompt"] = "say what?".into()),
                value(p("$.pending.prompt")),
            ),
            (
                Box::new(|s| s["pending"]["kind"] = "think".into()),
                value(p("$.pending.kind")),
            ),
            (
                Box::new(|s| s["lasting"][0][4] = "forever".into()),
                value(p("$.lasting[0][4]")),
            ),
            (
                Box::new(|s| s["lasting"][0][9] = (-2).into()),
                value(p("$.lasting[0][9]")),
            ),
            (
                Box::new(|s| {
                    let dup = s["once_used"][0].clone();
                    s["once_used"].as_array_mut().unwrap().push(dup);
                }),
                value(format!("$.once_used[{n_once}]")),
            ),
            (Box::new(|s| s["seed"] = (-1).into()), value(p("$.seed"))),
            (Box::new(|s| s["step"] = 7.into()), value(p("$.step"))),
            (
                Box::new(|s| s["redraws"] = Value::Array(vec![true.into()])),
                shape(p("$.redraws")),
            ),
        ];
        for (i, (edit, check)) in cases.iter().enumerate() {
            let e = decode_err(t, &mutated(rec, edit));
            assert!(check(&e), "case {i}: {e}");
        }
        let unknown_program = mutated(rec, |s| {
            s["frames"][0]["program_id"] = "0000000000000000".into();
        });
        assert_eq!(
            decode_err(t, &unknown_program),
            CanonError::UnknownContent {
                kind: "program",
                id: "0000000000000000".into()
            }
        );
        let unknown_filters = mutated(rec, |s| s["lasting"][0][9] = "ffffffffffffffff".into());
        assert_eq!(
            decode_err(t, &unknown_filters),
            CanonError::UnknownContent {
                kind: "filter_set",
                id: "ffffffffffffffff".into()
            }
        );
    }

    /// The side tables (PLAN §4.8): a runtime value needs its entry, every entry must decode
    /// to what its content id names, and the registry wins only when both agree.
    #[test]
    fn side_table_entries_must_resolve_to_their_content_ids() {
        let t = registry();
        let rec = by_id("synthetic/rare-shapes");
        let cid = rec["state"]["lasting"][0][0].as_str().unwrap();
        let entry_path = format!("continuous.{cid}");

        // A frozen StatMod is no IR node: without its entry it is unknown.
        let mut no_side_table = snapshot(rec);
        no_side_table.continuous = Value::Object(Map::new());
        assert_eq!(
            decode_err(t, &no_side_table),
            CanonError::UnknownContent {
                kind: "continuous",
                id: cid.into()
            }
        );
        // An entry that spells another value does not hash back to its key.
        let mut wrong_value = snapshot(rec);
        wrong_value.continuous[cid]["ap"] = 7.into();
        assert!(value(p_lasting())(&decode_err(t, &wrong_value)));
        // An entry that is not a DSL node.
        let mut not_a_node = snapshot(rec);
        not_a_node.continuous[cid] = json!({"_": "Nonsense"});
        assert!(value(entry_path.clone())(&decode_err(t, &not_a_node)));
        let mut not_continuous = snapshot(rec);
        not_continuous.continuous[cid] = json!({"_": "IsDamaged", "damaged": true});
        assert_eq!(
            decode_err(t, &not_continuous),
            CanonError::UnknownContent {
                kind: "continuous",
                id: cid.into()
            }
        );

        // A build-time effect resolves through the registry with or without its entry, and an
        // entry that names another registry effect is refused.
        let rule = by_id("fuzz/g003/d0019");
        let rule_cid = rule["state"]["lasting"][0][0].as_str().unwrap();
        let other = by_id("fuzz/g010/d0067");
        let other_form = other["continuous"]
            .as_object()
            .unwrap()
            .values()
            .next()
            .unwrap();
        let mut bare = snapshot(rule);
        bare.continuous = Value::Object(Map::new());
        assert!(decode(t, &bare).is_ok());
        let mut swapped = snapshot(rule);
        swapped.continuous[rule_cid] = other_form.clone();
        assert!(value(format!("continuous.{rule_cid}"))(&decode_err(
            t, &swapped
        )));

        // Filter sets: the same rules, over lists of inline filter nodes.
        let fcid = rec["state"]["lasting"][0][9].as_str().unwrap();
        let mut not_ir = snapshot(rec);
        not_ir.filter_sets[fcid] = json!([{"_": "HasBurst", "burst": true}]);
        assert!(value(format!("filter_sets.{fcid}"))(&decode_err(
            t, &not_ir
        )));
        let mut not_a_list = snapshot(rec);
        not_a_list.filter_sets[fcid] = json!({"_": "IsDamaged", "damaged": true});
        assert!(value(format!("filter_sets.{fcid}"))(&decode_err(
            t,
            &not_a_list
        )));
        let mut one_filter = snapshot(rec);
        one_filter.filter_sets[fcid] = json!([{"_": "IsDamaged", "damaged": true}]);
        assert!(value(format!("filter_sets.{fcid}"))(&decode_err(
            t,
            &one_filter
        )));
        // The embedded registry has no filter sets at all.
        assert_eq!(
            decode_err(&*Registry::embedded(), &snapshot(rec)),
            CanonError::UnknownContent {
                kind: "filter_set",
                id: fcid.into()
            }
        );
    }

    fn p_lasting() -> String {
        "$.lasting[0][0]".to_owned()
    }

    /// T-150: adding a once key that is present leaves `canonical_state` unchanged.
    #[test]
    fn re_adding_a_once_key_changes_nothing() {
        let t = registry();
        let mut checked = 0;
        for rec in snapshots() {
            let mut body = decode(t, &snapshot(rec)).unwrap();
            let Some(&key) = body.once_used.first() else {
                continue;
            };
            checked += 1;
            let before = dumps(&encode(&body.parts(), t), PY_DEFAULT);
            crate::state::set_add(&mut body.once_used, key);
            assert_eq!(dumps(&encode(&body.parts(), t), PY_DEFAULT), before);
            // A blind push would serialise the duplicate (the control).
            body.once_used.push(key);
            assert_ne!(dumps(&encode(&body.parts(), t), PY_DEFAULT), before);
        }
        assert!(checked >= 5, "only {checked} snapshots hold once keys");
    }

    /// T-058 (round-trip part): edited fixture states re-encode identically after a decode,
    /// for shapes and orders that random play does not reach.
    #[test]
    fn edited_states_round_trip() {
        let t = registry();
        let bodies: Vec<StateBody> = snapshots()
            .iter()
            .map(|r| decode(t, &snapshot(r)).unwrap())
            .collect();
        let mut conts: Vec<ContRef> = Vec::new();
        let mut vars: Vec<VarId> = Vec::new();
        for b in &bodies {
            for l in b.lasting.iter().chain(&b.delayed) {
                if !conts.contains(&l.effect) {
                    conts.push(l.effect);
                }
            }
            for e in &b.frame_vars {
                if !vars.contains(&e.var) {
                    vars.push(e.var);
                }
            }
        }
        assert!(conts.len() >= 10 && vars.len() >= 5, "{conts:?} {vars:?}");
        let side: Map<String, Value> = snapshots()
            .iter()
            .flat_map(|r| r["continuous"].as_object().unwrap().clone())
            .collect();
        prop::check("canonical-round-trip", 200, |rng| {
            let mut body = bodies[rng.randrange_usize(bodies.len())].clone();
            let n = body.cards.len();
            for _ in 0..8 {
                let u = rng.randrange_usize(n);
                let c = &mut body.cards[u];
                c.damage = i32::try_from(rng.randrange(5)).unwrap();
                c.rested = rng.randrange(2) == 1;
                c.known = u8::try_from(rng.randrange(4)).unwrap();
                c.pair = if rng.randrange(3) == 0 {
                    i32::try_from(rng.randrange_usize(n)).unwrap()
                } else {
                    NO_ARG
                };
            }
            for _ in 0..rng.randrange(4) {
                let mut elems = vec![OnceElem::Int(-8), OnceElem::Int(-1)];
                elems.push(OnceElem::Cont(conts[rng.randrange_usize(conts.len())]));
                for _ in 0..3 {
                    elems.push(OnceElem::Int(
                        i32::try_from(rng.randrange(200)).unwrap() - 20,
                    ));
                }
                let k = OnceKey::from_elems(&elems[..if rng.randrange(2) == 0 { 6 } else { 3 }])
                    .unwrap();
                crate::state::set_add(&mut body.once_used, k);
            }
            for _ in 0..rng.randrange(3) {
                let u = Uid::try_from(rng.randrange_usize(n)).unwrap();
                if !body.effect_hits.iter().any(|h| h.0 == u) {
                    body.effect_hits
                        .push((u, Uid::try_from(rng.randrange_usize(n)).unwrap()));
                }
            }
            if !body.frames.is_empty() {
                let i = rng.randrange_usize(body.frames.len());
                let mut a = FrameArena {
                    frames: &mut body.frames,
                    vars: &mut body.frame_vars,
                    ints: &mut body.frame_ints,
                    items: &mut body.frame_items,
                };
                let var = vars[rng.randrange_usize(vars.len())];
                let key = match rng.randrange(4) {
                    0 => IntKey::RestAs(u16::try_from(rng.randrange(30)).unwrap()),
                    1 => IntKey::RestI,
                    2 => IntKey::ExUsed,
                    _ => IntKey::HoldRules,
                };
                a.int_set(i, key, i32::try_from(rng.randrange(9)).unwrap() - 4)
                    .map_err(|e| e.to_string())?;
                a.var_set(i, var, &[3, 1, 2]).map_err(|e| e.to_string())?;
                a.buf_push(i, 7).map_err(|e| e.to_string())?;
            }
            body.zones.get_mut(0, Zone::Deck).reverse();
            let first = encode(&body.parts(), t);
            let snap = StateSnapshot {
                state: serde_json::from_str(&dumps(&first, PY_DEFAULT))
                    .map_err(|e| e.to_string())?,
                continuous: side.clone().into(),
                filter_sets: Value::Null,
                rules_checked: -1,
            };
            let again = decode(t, &snap).map_err(|e| e.to_string())?;
            let second = encode(&again.parts(), t);
            let (a, b) = (dumps(&first, PY_DEFAULT), dumps(&second, PY_DEFAULT));
            if a == b {
                Ok(())
            } else {
                Err(first_difference(&a, &b))
            }
        });
    }
}
