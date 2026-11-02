//! `macro_multi_pick` (S8, W-138): a small multi-card choice searched as one move per subset.
//!
//! The engine asks a "choose up to N cards" effect as a sequence of single SELECT picks in
//! candidate order plus DONE once the minimum is met (`_choose_options`, `engine/interp.py:345-
//! 350`; `_r_choose`, `:452-470`), so the reference tree has one node per partial subset and
//! splits the statistics of one subset over its prefixes (`docs/ARCHITECTURE.md:53-56`). Here a
//! SELECT of cards whose maximum is 2 or 3 is expanded, on clones of the node's state, into its
//! complete subsets (at most [`MAX_SUBSETS`]); each subset is one tree move whose key packs its
//! picks, and applying it applies the engine's single picks in order (and DONE when the subset
//! ends early). The engine interface and the replay encoding are unchanged: the agent plays the
//! first pick of the chosen subset, and every later pick is a decision of its own. A choice
//! with more subsets, or whose picks cannot be packed, keeps the single picks. The expansion's
//! engine applies are counted as auxiliary applies and stop at the decision's cap.

use gcg_ai::{MoveKey, apply_unchecked, canonical, uid_argument};
use gcg_engine::{Action, ActionKind, CtxKey, DecisionKind, DecisionView, EngineError, GameState};
use gcg_engine::{NO_ARG, Prompt};

/// The most cards a covered choice picks.
pub(crate) const MAX_PICKS: i32 = 3;
/// The most subsets a covered choice has.
pub(crate) const MAX_SUBSETS: usize = 20;
/// The variant of a macro key (plus the number of picks); a single SELECT key's variant is
/// `NO_ARG`, so the two never collide.
const MACRO_VARIANT: i32 = i32::MIN;
/// The width of one packed pick: every pick key's argument is a uid or `(1 << 24) + def_id`.
const PICK_BITS: u32 = 25;

/// One subset of a multi-card choice: its tree key and the engine actions that pick it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct Macro {
    pub(crate) key: MoveKey,
    pub(crate) seq: Box<[Action]>,
}

/// The identity of one choice: its decider, prompt and bounds.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct ChoiceId {
    player: u8,
    prompt: Prompt,
    min: i32,
    max: i32,
}

impl ChoiceId {
    fn of(dec: &DecisionView<'_>) -> Self {
        Self {
            player: dec.hdr.player,
            prompt: dec.hdr.prompt,
            min: dec.hdr.ctx(CtxKey::Min),
            max: dec.hdr.ctx(CtxKey::Max),
        }
    }
}

/// Whether `dec` is a multi-card choice the macro covers: a SELECT of cards with a maximum of
/// 2 or 3.
pub(crate) fn covers(dec: &DecisionView<'_>) -> bool {
    dec.hdr.kind == DecisionKind::Select
        && uid_argument(dec)
        && (2..=MAX_PICKS).contains(&dec.hdr.ctx(CtxKey::Max))
}

/// The argument of a single pick key in [`PICK_BITS`] bits.
fn pick_bits(k: MoveKey) -> Option<i64> {
    (k.kind == ActionKind::Select
        && k.b == i64::from(NO_ARG)
        && k.variant == NO_ARG
        && (0..1i64 << PICK_BITS).contains(&k.a))
    .then_some(k.a)
}

/// The key of the subset picked by `picks` (DONE excluded), or `done` for the empty subset.
fn macro_key(picks: &[MoveKey], done: Option<MoveKey>) -> Option<MoveKey> {
    if picks.is_empty() {
        return done;
    }
    let bits: Vec<i64> = picks.iter().map(|&k| pick_bits(k)).collect::<Option<_>>()?;
    let slot = |i: usize| bits.get(i).map_or(0, |&b| b + 1);
    let n = i32::try_from(picks.len()).ok()?;
    Some(MoveKey {
        kind: ActionKind::Select,
        a: bits[0] | (slot(1) << PICK_BITS),
        b: slot(2),
        variant: MACRO_VARIANT + n,
    })
}

/// The search for subsets below one prefix.
struct Expansion<'a> {
    id: ChoiceId,
    out: Vec<Macro>,
    aux: &'a mut u64,
    aux_cap: u64,
    keys: Vec<MoveKey>,
    actions: Vec<Action>,
}

impl Expansion<'_> {
    /// Emits the subset of the current prefix (ending with `done` when given); `false` when it
    /// cannot be keyed or the choice has too many subsets.
    fn emit(&mut self, done: Option<(MoveKey, Action)>) -> bool {
        let Some(key) = macro_key(&self.keys, done.map(|(k, _)| k)) else {
            return false;
        };
        if self.out.iter().any(|m| m.key == key) {
            return true;
        }
        let mut seq = self.actions.clone();
        seq.extend(done.map(|(_, a)| a));
        self.out.push(Macro {
            key,
            seq: seq.into_boxed_slice(),
        });
        self.out.len() <= MAX_SUBSETS
    }

    /// Every subset that starts with the current prefix and continues with one of `moves`
    /// (the canonical moves of the choice's next pick in state `s`).
    fn walk(&mut self, s: &GameState, moves: &[(MoveKey, Action)]) -> Result<bool, EngineError> {
        for &(k, a) in moves {
            if a.kind == ActionKind::Done {
                if !self.emit(Some((k, a))) {
                    return Ok(false);
                }
                continue;
            }
            if a.kind != ActionKind::Select
                || self.keys.len() >= MAX_PICKS as usize
                || *self.aux >= self.aux_cap
            {
                return Ok(false);
            }
            let mut next = s.clone();
            apply_unchecked(&mut next, a)?;
            *self.aux += 1;
            self.keys.push(k);
            self.actions.push(a);
            let more = next
                .pending()
                .filter(|d| d.hdr.kind == DecisionKind::Select && ChoiceId::of(d) == self.id);
            let ok = match more {
                Some(d) => {
                    let follow = canonical(&next, &d);
                    self.walk(&next, &follow)?
                }
                None => self.emit(None),
            };
            self.keys.pop();
            self.actions.pop();
            if !ok {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

/// The subsets of the covered choice `dec` in state `s` whose first pick is one of `first`
/// (the node's moves, in order), or `None` when the choice keeps its single picks (too many
/// subsets, an unpackable pick, or the auxiliary cap reached). `aux` counts the expansion's
/// engine applies, which stop at `aux_cap`.
///
/// # Errors
/// The engine's error from a trial pick.
pub(crate) fn expand(
    s: &GameState,
    dec: &DecisionView<'_>,
    first: &[(MoveKey, Action)],
    aux: &mut u64,
    aux_cap: u64,
) -> Result<Option<Vec<Macro>>, EngineError> {
    let mut e = Expansion {
        id: ChoiceId::of(dec),
        out: Vec::new(),
        aux,
        aux_cap,
        keys: Vec::new(),
        actions: Vec::new(),
    };
    Ok(e.walk(s, first)?.then_some(e.out))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pick(a: i64) -> MoveKey {
        MoveKey {
            kind: ActionKind::Select,
            a,
            b: i64::from(NO_ARG),
            variant: NO_ARG,
        }
    }

    #[test]
    fn subset_keys_are_distinct_and_never_single_pick_keys() {
        let done = MoveKey {
            kind: ActionKind::Done,
            a: -1,
            b: -1,
            variant: -1,
        };
        let def = (1i64 << 24) + 1151;
        let subsets: [&[MoveKey]; 7] = [
            &[pick(3)],
            &[pick(3), pick(4)],
            &[pick(4)],
            &[pick(3), pick(4), pick(5)],
            &[pick(def), pick(def)],
            &[pick(def)],
            &[pick(0)],
        ];
        let keys: Vec<MoveKey> = subsets
            .iter()
            .map(|p| macro_key(p, None).expect("packable"))
            .collect();
        for (i, k) in keys.iter().enumerate() {
            assert!(keys[i + 1..].iter().all(|o| o != k), "{k:?}");
            assert_ne!(*k, pick(k.a));
            assert!(k.variant < NO_ARG);
        }
        assert_eq!(macro_key(&[], Some(done)), Some(done));
        assert_eq!(macro_key(&[pick(1 << PICK_BITS)], None), None);
        let mode = MoveKey {
            kind: ActionKind::Select,
            a: 2,
            b: 7,
            variant: NO_ARG,
        };
        assert_eq!(macro_key(&[mode], None), None);
    }
}
