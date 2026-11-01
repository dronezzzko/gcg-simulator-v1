//! Methods of the plain records of `engine/state.py` (`Action`, `Decision`, the event and
//! once-key tuples, `Frame.vars`) whose types are frozen in `contracts.rs`
//! (docs/port/contracts.md, gcg-engine public API; PLAN §4.2).

use std::fmt;
use std::ops::Range;

use gcg_core::pyjson::Json;
use gcg_effects::VarId;

use crate::{
    Action, CONTEXT_CAPACITY, CtxKey, DecisionHdr, EVENT_CAPACITY, EngineError, EvKey,
    EventPayload, FpOnceElem, FpOnceKey, FrameVars, NO_ARG, ONCE_KEY_CAPACITY, OnceElem, OnceKey,
    Span,
};

impl Span {
    /// `items[start..start + len]` as a `usize` range.
    #[must_use]
    pub(crate) const fn range(self) -> Range<usize> {
        self.start as usize..(self.start + self.len) as usize
    }

    /// One past the last element.
    #[must_use]
    pub(crate) const fn end(self) -> usize {
        (self.start + self.len) as usize
    }
}

/// `Action.__str__` (`engine/state.py:30-32`): the parameters equal to `-1` are dropped by
/// value, so `Action(X, -1, 3)` prints `x(3)` and `Action(ATTACK, 5, -2)` prints `attack(5,-2)`.
impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}(", self.kind.as_str())?;
        let mut first = true;
        for x in [self.a, self.b, self.c] {
            if x != NO_ARG {
                if !first {
                    f.write_str(",")?;
                }
                write!(f, "{x}")?;
                first = false;
            }
        }
        f.write_str(")")
    }
}

impl Action {
    /// `[kind.value, a, b, c]` (`engine/state.py:23-24`).
    #[must_use]
    pub fn to_json(&self) -> Json {
        Json::Array(vec![
            Json::from(self.kind.as_str()),
            Json::from(self.a),
            Json::from(self.b),
            Json::from(self.c),
        ])
    }
}

impl EventPayload {
    /// The value of the first pair with `key` (`_payload`, `core.py:187-191`; `Ctx.ev`,
    /// `view.py:46-50`; `Frame.ev`, `state.py:366-370`).
    #[must_use]
    pub fn get(&self, key: EvKey) -> Option<i32> {
        self.0
            .as_slice()
            .iter()
            .find_map(|&(k, v)| (k == key).then_some(v))
    }

    /// The pairs in tuple order.
    #[must_use]
    pub(crate) fn pairs(&self) -> &[(EvKey, i32)] {
        self.0.as_slice()
    }

    /// Appends `(key, value)` to the tuple.
    ///
    /// # Errors
    /// [`EngineError::Capacity`] beyond [`EVENT_CAPACITY`] pairs (never a silent drop).
    #[allow(
        dead_code,
        reason = "called by layers L3-L5 (docs/port/contracts.md); they land after L1"
    )]
    pub(crate) fn push(&mut self, key: EvKey, value: i32) -> Result<(), EngineError> {
        self.0
            .try_push((key, value))
            .map_err(|_| EngineError::Capacity {
                what: "event payload",
                capacity: EVENT_CAPACITY,
            })
    }
}

impl DecisionHdr {
    /// `Decision.ctx(key, default=-1)`: the first matching pair (`engine/state.py:45-49`).
    #[must_use]
    pub fn ctx(&self, key: CtxKey) -> i32 {
        self.context
            .as_slice()
            .iter()
            .find_map(|&(k, v)| (k == key).then_some(v))
            .unwrap_or(NO_ARG)
    }

    /// Appends `(key, value)` to `context`.
    ///
    /// # Errors
    /// [`EngineError::Capacity`] beyond [`CONTEXT_CAPACITY`] pairs.
    #[allow(
        dead_code,
        reason = "called by layers L3-L5 (docs/port/contracts.md); they land after L1"
    )]
    pub(crate) fn push_context(&mut self, key: CtxKey, value: i32) -> Result<(), EngineError> {
        self.context
            .try_push((key, value))
            .map_err(|_| EngineError::Capacity {
                what: "decision context",
                capacity: CONTEXT_CAPACITY,
            })
    }
}

impl<'a> FrameVars<'a> {
    /// `f.vars.get(var)`.
    #[must_use]
    pub fn get(&self, var: VarId) -> Option<&'a [i32]> {
        let items = self.items;
        self.entries
            .iter()
            .find(|e| e.var == var)
            .map(|e| &items[e.items.range()])
    }

    /// `f.vars.items()`. The order is unspecified: its one Python reader is an `any()`
    /// (`ai/actions.py:292`); the canonical form sorts by name (`state.py:382`).
    pub fn iter(&self) -> impl Iterator<Item = (VarId, &'a [i32])> + 'a {
        let items = self.items;
        self.entries
            .iter()
            .map(move |e| (e.var, &items[e.items.range()]))
    }
}

impl OnceKey {
    /// A key of plain ints, as the engine builds them (`core.py:236-237`, `interp.py:648`).
    ///
    /// # Errors
    /// [`EngineError::Capacity`] beyond [`ONCE_KEY_CAPACITY`] elements.
    #[allow(
        dead_code,
        reason = "called by layers L3-L5 (docs/port/contracts.md); they land after L1"
    )]
    pub(crate) fn from_elems(elems: &[OnceElem]) -> Result<Self, EngineError> {
        let mut k = Self::default();
        for &e in elems {
            k.0.try_push(e).map_err(|_| EngineError::Capacity {
                what: "once key",
                capacity: ONCE_KEY_CAPACITY,
            })?;
        }
        Ok(k)
    }

    /// The elements in tuple order.
    #[must_use]
    pub(crate) fn elems(&self) -> &[OnceElem] {
        self.0.as_slice()
    }

    /// `x in key` (`ai/actions.py:309`): an `Int` element equal to `x`. A `Cont` element never
    /// equals a uid (PLAN §4.2, §8.9).
    #[must_use]
    pub fn contains_int(&self, x: i32) -> bool {
        self.0.as_slice().contains(&OnceElem::Int(x))
    }

    /// `tuple(_SELF if x == uid else x for x in key)` (`ai/actions.py:309`).
    #[must_use]
    pub fn with_self(&self, uid: i32) -> FpOnceKey {
        let mut out = FpOnceKey::default();
        for &e in self.0.as_slice() {
            let fp = match e {
                OnceElem::Int(x) if x == uid => FpOnceElem::SelfRef,
                OnceElem::Int(x) => FpOnceElem::Int(x),
                OnceElem::Cont(c) => FpOnceElem::Cont(c),
            };
            // Same capacity as the source key, so the push cannot fail.
            if out.0.try_push(fp).is_err() {
                unreachable!("FpOnceKey has the capacity of OnceKey");
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ActionKind, PLAYER_TARGET, VarEntry};
    use gcg_core::pyjson::{DumpOptions, dumps};
    use gcg_effects::ContRef;

    #[test]
    fn action_str_drops_minus_one_by_value() {
        assert_eq!(Action::of(ActionKind::EndMain).to_string(), "end_main()");
        assert_eq!(
            Action::new(ActionKind::Select, -1, 3, -1).to_string(),
            "select(3)"
        );
        assert_eq!(
            Action::new(ActionKind::Attack, 5, PLAYER_TARGET, -1).to_string(),
            "attack(5,-2)"
        );
        assert_eq!(
            Action::new(ActionKind::PlayUnit, 4, -1, 2).to_string(),
            "play_unit(4,2)"
        );
        let j = Action::new(ActionKind::GoFirst, 1, -1, -1).to_json();
        let opts = DumpOptions {
            indent: None,
            sort_keys: false,
            compact: false,
            ensure_ascii: true,
        };
        assert_eq!(dumps(&j, opts), "[\"go_first\", 1, -1, -1]");
    }

    #[test]
    fn payload_and_context_return_the_first_match() {
        let mut ev = EventPayload::default();
        ev.push(EvKey::Subject, 4).unwrap();
        ev.push(EvKey::Player, 0).unwrap();
        ev.push(EvKey::Subject, 9).unwrap();
        assert_eq!(ev.get(EvKey::Subject), Some(4));
        assert_eq!(ev.get(EvKey::Group), None);
        for i in 3..EVENT_CAPACITY {
            ev.push(EvKey::Amount, i32::try_from(i).unwrap()).unwrap();
        }
        assert_eq!(
            ev.push(EvKey::By, 1),
            Err(EngineError::Capacity {
                what: "event payload",
                capacity: EVENT_CAPACITY
            })
        );
        let mut hdr = DecisionHdr {
            player: 0,
            kind: crate::DecisionKind::Select,
            prompt: crate::Prompt::Engine(crate::EnginePrompt::Discard),
            context: gcg_core::small::InlineVec::new(),
        };
        assert_eq!(hdr.ctx(CtxKey::Min), NO_ARG);
        hdr.push_context(CtxKey::Min, 1).unwrap();
        hdr.push_context(CtxKey::Max, 2).unwrap();
        hdr.push_context(CtxKey::Min, 5).unwrap();
        assert_eq!((hdr.ctx(CtxKey::Min), hdr.ctx(CtxKey::Max)), (1, 2));
    }

    #[test]
    fn frame_vars_read_their_items() {
        let entries = [
            VarEntry {
                var: VarId(3),
                items: Span { start: 2, len: 2 },
            },
            VarEntry {
                var: VarId(1),
                items: Span { start: 0, len: 0 },
            },
        ];
        let items = [7, 8, 9, 10];
        let fv = FrameVars {
            entries: &entries,
            items: &items,
        };
        assert_eq!(fv.get(VarId(3)), Some(&[9, 10][..]));
        assert_eq!(fv.get(VarId(1)), Some(&[][..]));
        assert_eq!(fv.get(VarId(2)), None);
        let all: Vec<_> = fv.iter().collect();
        assert_eq!(all, [(VarId(3), &[9, 10][..]), (VarId(1), &[][..])]);
    }

    #[test]
    fn once_keys_match_uids_and_rewrite_them() {
        let k = OnceKey::from_elems(&[
            OnceElem::Int(-8),
            OnceElem::Int(-1),
            OnceElem::Cont(ContRef::StatMod { ap: 5, hp: 0 }),
            OnceElem::Int(5),
            OnceElem::Int(7),
            OnceElem::Int(5),
        ])
        .unwrap();
        assert!(k.contains_int(5));
        assert!(k.contains_int(-1));
        assert!(!k.contains_int(0));
        let fp = k.with_self(5);
        assert_eq!(
            fp.0.as_slice(),
            &[
                FpOnceElem::Int(-8),
                FpOnceElem::Int(-1),
                FpOnceElem::Cont(ContRef::StatMod { ap: 5, hp: 0 }),
                FpOnceElem::SelfRef,
                FpOnceElem::Int(7),
                FpOnceElem::SelfRef,
            ]
        );
        assert!(OnceKey::from_elems(&[OnceElem::Int(0); 7]).is_err());
    }
}
