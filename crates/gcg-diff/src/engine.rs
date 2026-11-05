//! `RealEngine`: the [`EngineUnderTest`] over `gcg-engine`'s public API (PLAN §8.7, §10.3.7;
//! docs/port/contracts.md, gcg-diff section "The real engine implements them with").
//!
//! - `new_game`: `gcg_engine::new_game_by_number` on the trace's card-number strings
//!   (`engine/game.py:107-158`), on the current registry;
//! - `load` and `load_state`: `canon::from_canonical` (PLAN §4.8), then for a stale view
//!   `testkit::install_view`, then `testkit::set_force_link` with the current list;
//! - `set_registry`: `Registry::with_fragment` on the embedded registry (always the base, never
//!   the previous fragment); `force_link`: `testkit::set_force_link`;
//! - `pending`, `observe`, `canonical`: the state accessors and `canon` (`runner/digest.py`);
//! - `apply`: `gcg_engine::apply` (`engine/game.py:780-818`);
//! - `probe` and `call`: the name dispatcher of [`crate::dispatch`].
//!
//! Errors carry Python's exception class (`type(exc).__name__`) and message, as the trace
//! records them (`port/oracle/pytest_trace.py` `_error`).

use std::cell::RefCell;
use std::sync::Arc;

use gcg_core::pyjson::{DumpOptions, Json, Key, dumps};
use gcg_effects::Registry;
use gcg_engine::canon::{canonical_state, from_canonical};

use gcg_engine::{
    Action, ApplyError, GameState, NewGameError, NewGameOptions, Scenario, StateSnapshot, Step,
    ViewKey,
};
use serde_json::Value;

use crate::dispatch;
use crate::json::sha256_hex;
use crate::{
    CallSpec, DerivedMark, EngineUnderTest, ErrorRec, Fingerprint, NewGameSpec, PendingRec,
    StateRec,
};

/// The class name of errors that are the differential runner's own (no Python counterpart),
/// so they can never equal a recorded Python error.
pub const GCG_DIFF_ERROR: &str = "gcg-diff";

/// `json.dumps(x, sort_keys=True, separators=(",", ":"))` (ASCII): the `state_digest` text.
pub const DIGEST_DUMP: DumpOptions = DumpOptions {
    indent: None,
    sort_keys: true,
    compact: true,
    ensure_ascii: true,
};

/// An error of the differential runner itself (a missing state, a malformed argument).
#[must_use]
pub fn diff_error(message: impl Into<String>) -> ErrorRec {
    ErrorRec {
        class: GCG_DIFF_ERROR.to_owned(),
        message: message.into(),
    }
}

/// A `gcg_core::pyjson::Json` as a `serde_json::Value` (insertion order kept; integer keys
/// become their decimal text, as `json.dumps` writes them).
///
/// # Panics
/// On a non-finite float or an integer outside `i64`/`u64`, neither of which a canonical state
/// or a probe result holds.
#[must_use]
pub fn json_to_value(j: &Json) -> Value {
    match j {
        Json::Null => Value::Null,
        Json::Bool(b) => Value::Bool(*b),
        Json::Int(i) => i64::try_from(*i).map_or_else(
            |_| Value::from(u64::try_from(*i).expect("an integer within u64")),
            Value::from,
        ),
        Json::Float(f) => {
            Value::from(serde_json::Number::from_f64(*f).expect("a finite float in engine JSON"))
        }
        Json::Str(s) => Value::String(s.clone()),
        Json::Array(items) => Value::Array(items.iter().map(json_to_value).collect()),
        Json::Object(items) => Value::Object(
            items
                .iter()
                .map(|(k, v)| {
                    let key = match k {
                        Key::Str(s) => s.clone(),
                        Key::Int(i) => i.to_string(),
                    };
                    (key, json_to_value(v))
                })
                .collect(),
        ),
    }
}

/// The digest-form text of `st`'s canonical state (`runner/digest.py:52-56`).
#[must_use]
pub fn digest_text(st: &GameState) -> String {
    dumps(&canonical_state(st), DIGEST_DUMP)
}

/// The first 16 hex digits of `state_digest(st)`.
#[must_use]
pub fn digest16(st: &GameState) -> String {
    let mut d = sha256_hex(digest_text(st).as_bytes());
    d.truncate(16);
    d
}

/// `st.pending.to_json()` (`engine/state.py:51-58`) as a [`PendingRec`].
#[must_use]
pub fn pending_of(st: &GameState) -> Option<PendingRec> {
    let dec = st.pending()?;
    Some(PendingRec {
        player: dec.hdr.player,
        kind: dec.hdr.kind,
        options: dec.options.to_vec(),
        prompt: dec.hdr.prompt.text(st.registry()).into_owned(),
        context: dec
            .hdr
            .context
            .as_slice()
            .iter()
            .map(|(k, v)| (k.as_str().to_owned(), *v))
            .collect(),
    })
}

/// The fingerprint of `st` (PLAN A21, §8.7 step 2).
#[must_use]
pub fn fingerprint_of(st: &GameState) -> Fingerprint {
    Fingerprint {
        version: st.version(),
        rules_checked: st.rules_checked(),
        rng: st.rng_state(),
        action_count: st.action_count(),
        event_group: st.event_group(),
        turn: st.turn(),
        step: st.step(),
        digest16: digest16(st),
    }
}

/// What Python's `apply` raised, as the trace records it.
#[must_use]
pub fn apply_error(e: &ApplyError) -> ErrorRec {
    match e {
        ApplyError::Illegal(i) => ErrorRec {
            class: "IllegalActionError".to_owned(),
            message: i.to_string(),
        },
        ApplyError::Engine(x) => ErrorRec {
            class: x.py_class().to_owned(),
            message: x.to_string(),
        },
    }
}

/// What Python's `new_game` raised, as the trace records it.
#[must_use]
pub fn new_game_error(e: &NewGameError) -> ErrorRec {
    let class = match e {
        NewGameError::Construction(_) => "DeckConstructionError",
        NewGameError::Unimplemented(_) => "UnimplementedCardError",
        NewGameError::Engine(x) => x.py_class(),
    };
    ErrorRec {
        class: class.to_owned(),
        message: e.to_string(),
    }
}

/// `_view_key` as `json.loads(json.dumps(dv.key))` writes it (`engine/view.py:223-229`):
/// `[active, turn, len(battles), [battle_id, target, ended] or [], len(history)]`.
///
/// # Errors
/// Any other shape.
pub fn view_key(v: &Value) -> Result<ViewKey, String> {
    let bad = || format!("view_key: expected [active, turn, battles, battle, history], got {v}");
    let items = v.as_array().filter(|a| a.len() == 5).ok_or_else(bad)?;
    let int = |x: &Value| x.as_i64().ok_or_else(bad);
    let battle = match items[3].as_array().ok_or_else(bad)?.as_slice() {
        [] => None,
        [id, target, ended] => Some((
            i32::try_from(int(id)?).map_err(|_| bad())?,
            i32::try_from(int(target)?).map_err(|_| bad())?,
            ended.as_bool().ok_or_else(bad)?,
        )),
        _ => return Err(bad()),
    };
    Ok(ViewKey {
        active: u8::try_from(int(&items[0])?).map_err(|_| bad())?,
        turn: i32::try_from(int(&items[1])?).map_err(|_| bad())?,
        n_battles: u32::try_from(int(&items[2])?).map_err(|_| bad())?,
        battle,
        n_history: u32::try_from(int(&items[4])?).map_err(|_| bad())?,
    })
}

/// The JSON form of the view signature a trace records (`view` records, the
/// `paranoid.view_signature` probe, and the sha16 of a `view.derived` result).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum SigForm {
    /// `port/oracle/traces.py` `view_signature_json` (game, search and `views` traces): the
    /// form `gcg_engine::view::signature` returns.
    Views,
    /// `port/oracle/pytest_trace.py` `view_sig` (per-test traces).
    Tests,
}

/// The real engine of one replay.
pub struct RealEngine {
    base: Arc<Registry>,
    reg: Arc<Registry>,
    st: Option<GameState>,
    /// The Unit numbers of the last `force_link` record (sorted, as the record lists them).
    force_link: Vec<String>,
    /// The `Scenario` under construction (between `Scenario.__init__` and `Scenario.start`).
    pub(crate) scenario: Option<Scenario>,
    pub(crate) sig_form: SigForm,
    /// The multisets `invariants.initial_multiset` returned in this replay, by their JSON, for
    /// a later `invariants.check(st, initial)` call.
    pub(crate) multisets: Vec<(Value, gcg_engine::CardMultiset)>,
    /// With [`RealEngine::record_cards`]: per def id, whether a card of it was in a state the
    /// replay observed (`observe`, `canonical`).
    seen: RefCell<Option<Vec<bool>>>,
}

impl Default for RealEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl RealEngine {
    /// An engine on the embedded registry with no state, for game traces.
    #[must_use]
    pub fn new() -> Self {
        let base = Registry::embedded();
        Self {
            reg: Arc::clone(&base),
            base,
            st: None,
            force_link: Vec::new(),
            scenario: None,
            sig_form: SigForm::Views,
            multisets: Vec::new(),
            seen: RefCell::new(None),
        }
    }

    /// Record the definitions of the cards of every state the replay observes from now on.
    pub fn record_cards(&mut self) {
        *self.seen.borrow_mut() = Some(vec![false; self.base.db().len()]);
    }

    /// The def ids recorded since [`RealEngine::record_cards`], sorted.
    #[must_use]
    pub fn seen_defs(&self) -> Vec<u16> {
        self.seen.borrow().as_ref().map_or_else(Vec::new, |v| {
            v.iter()
                .enumerate()
                .filter(|(_, s)| **s)
                .filter_map(|(i, _)| u16::try_from(i).ok())
                .collect()
        })
    }

    fn note_cards(&self, st: &GameState) {
        if let Some(v) = self.seen.borrow_mut().as_mut() {
            for c in st.cards() {
                if let Some(slot) = v.get_mut(usize::from(c.def_id.0)) {
                    *slot = true;
                }
            }
        }
    }

    /// An engine for the trace with this header: per-test traces record the `pytest_trace.py`
    /// signature form.
    #[must_use]
    pub fn for_trace(header: &crate::Header) -> Self {
        let mut e = Self::new();
        if header.kind == crate::TraceKind::Test {
            e.sig_form = SigForm::Tests;
        }
        e
    }

    /// The registry new states are built on: the embedded one, or it with the trace's current
    /// override fragment.
    #[must_use]
    pub fn registry(&self) -> &Arc<Registry> {
        &self.reg
    }

    /// The current state, mutably (the dispatcher's direct engine calls).
    pub(crate) fn state_mut(&mut self) -> Option<&mut GameState> {
        self.st.as_mut()
    }

    /// Make `st` the current state, with the current force-link list applied.
    pub(crate) fn install(&mut self, mut st: GameState) {
        if !self.force_link.is_empty() {
            let units: Vec<&str> = self.force_link.iter().map(String::as_str).collect();
            gcg_engine::testkit::set_force_link(&mut st, &units);
        }
        self.st = Some(st);
    }

    /// The units of the current force-link list.
    pub(crate) fn force_link_units(&self) -> &[String] {
        &self.force_link
    }

    fn snapshot_state(&self, snap: &StateSnapshot) -> Result<GameState, ErrorRec> {
        from_canonical(&self.reg, snap).map_err(|e| ErrorRec {
            class: "CanonError".to_owned(),
            message: e.to_string(),
        })
    }
}

impl EngineUnderTest for RealEngine {
    fn new_game(&mut self, spec: &NewGameSpec) -> Result<(), ErrorRec> {
        let decks: Vec<(Vec<&str>, Vec<&str>)> = spec
            .decks
            .iter()
            .map(|d| {
                (
                    d.main.iter().map(String::as_str).collect(),
                    d.resources.iter().map(String::as_str).collect(),
                )
            })
            .collect();
        let pair = [
            (decks[0].0.as_slice(), decks[0].1.as_slice()),
            (decks[1].0.as_slice(), decks[1].1.as_slice()),
        ];
        let opts = NewGameOptions {
            chooser: spec.chooser,
            turn_limit: spec.turn_limit,
            max_actions: spec.max_actions,
            validate: spec.validate,
        };
        let st = gcg_engine::new_game_by_number(&self.reg, pair, spec.seed, opts)
            .map_err(|e| new_game_error(&e))?;
        self.install(st);
        Ok(())
    }

    fn load(&mut self, snapshot: &StateSnapshot) -> Result<(), ErrorRec> {
        let st = self.snapshot_state(snapshot)?;
        self.install(st);
        Ok(())
    }

    fn load_state(&mut self, rec: &StateRec) -> Result<(), ErrorRec> {
        let st = self.snapshot_state(&rec.snapshot)?;
        if rec.derived == DerivedMark::Stale {
            let (Some(from), Some(key)) = (&rec.view_from, &rec.view_key) else {
                return Err(diff_error(
                    "a stale state record without view_from and view_key",
                ));
            };
            let key = view_key(key).map_err(diff_error)?;
            let origin = self.snapshot_state(&StateSnapshot {
                state: from.clone(),
                ..rec.snapshot.clone()
            })?;
            gcg_engine::testkit::install_view(&st, &origin, key).map_err(|e| ErrorRec {
                class: e.py_class().to_owned(),
                message: e.to_string(),
            })?;
        }
        self.install(st);
        Ok(())
    }

    fn set_registry(&mut self, fragment: &Value) -> Result<(), ErrorRec> {
        let reg = self.base.with_fragment(fragment).map_err(|e| ErrorRec {
            class: "IrError".to_owned(),
            message: e.to_string(),
        })?;
        self.reg = Arc::new(reg);
        Ok(())
    }

    /// The list applies to the current state, the `Scenario` being built and every later
    /// state. A test may patch `view.link_satisfied` before it builds any state
    /// (`tests/cards/test_wp_gd05_a.py:101-113`), so with no state the list is only kept.
    fn force_link(&mut self, units: &[String]) -> Result<(), ErrorRec> {
        units.clone_into(&mut self.force_link);
        let list: Vec<&str> = units.iter().map(String::as_str).collect();
        if let Some(sc) = &mut self.scenario {
            sc.force_link(&list);
        }
        if let Some(st) = &mut self.st {
            gcg_engine::testkit::set_force_link(st, &list);
        }
        Ok(())
    }

    fn pending(&self) -> Option<PendingRec> {
        self.st.as_ref().and_then(pending_of)
    }

    fn state(&self) -> Option<&GameState> {
        self.st.as_ref()
    }

    fn observe(&self) -> Fingerprint {
        if let Some(st) = &self.st {
            self.note_cards(st);
        }
        self.st.as_ref().map_or_else(
            || Fingerprint {
                version: 0,
                rules_checked: -1,
                rng: 0,
                action_count: 0,
                event_group: 0,
                turn: 0,
                step: Step::ALL[0],
                digest16: String::new(),
            },
            fingerprint_of,
        )
    }

    fn apply(&mut self, action: Action, check: bool) -> Result<(), ErrorRec> {
        let st = self
            .st
            .as_mut()
            .ok_or_else(|| diff_error("apply with no state loaded"))?;
        gcg_engine::apply(st, action, check).map_err(|e| apply_error(&e))
    }

    fn canonical(&self) -> Value {
        self.st
            .as_ref()
            .map_or(Value::Null, |st| json_to_value(&canonical_state(st)))
    }

    fn probe(&mut self, call: &CallSpec) -> Result<Value, ErrorRec> {
        dispatch::probe(self, call)
    }

    fn call(&mut self, call: &CallSpec) -> Result<Value, ErrorRec> {
        dispatch::call(self, call)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn view_keys_decode_pythons_tuple() {
        assert_eq!(
            view_key(&json!([1, 4, 2, [7, -2, false], 31])),
            Ok(ViewKey {
                active: 1,
                turn: 4,
                n_battles: 2,
                battle: Some((7, -2, false)),
                n_history: 31
            })
        );
        assert_eq!(
            view_key(&json!([0, 3, 0, [], 0])).map(|k| k.battle),
            Ok(None)
        );
        assert!(view_key(&json!([0, 3, 0, [1], 0])).is_err());
        assert!(view_key(&json!([0, 3, 0, []])).is_err());
    }

    #[test]
    fn python_json_converts_in_insertion_order() {
        let mut o = Json::object();
        o.set("b", 1_i64);
        o.set("a", vec![Json::Null, Json::Bool(true), Json::from("x")]);
        let v = json_to_value(&o);
        assert_eq!(v, json!({"b": 1, "a": [null, true, "x"]}));
        let keys: Vec<&String> = v
            .as_object()
            .map(|m| m.keys().collect())
            .unwrap_or_default();
        assert_eq!(keys, ["b", "a"]);
    }
}
