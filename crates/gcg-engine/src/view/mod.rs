//! Layer L2 (workstream B2): the derived view of `engine/view.py` and the 21 cond, filter and
//! value hooks of the binding modules (docs/dev/PLAN.md §4.6, §5.3; docs/dev/contracts.md,
//! gcg-engine "L2 (B2) — view/").
//!
//! - [`derived`] fills the state's cache exactly as `view.py:232-239` fills `st._derived`;
//!   `touch()` (L1) is the only invalidation.
//! - The fixpoint (`view.py:409-463`), the base view, gates and `_apply_cont` live in
//!   `derived.rs`; expression evaluation (`view.py:577-933`) in `eval.rs`; the
//!   characteristic queries (`view.py:490-570`) in `query.rs`; the hooks in `hooks.rs`; the
//!   view signature of the `views` trace set in `signature.rs`; the `paranoid` cache checks
//!   (W-49) in `paranoid.rs`.
//!
//! Python sources are cited relative to `src/gcg_sim/`.

mod derived;
mod eval;
mod hooks;
#[cfg(feature = "paranoid")]
mod paranoid;
mod pool;
mod query;
mod signature;
#[cfg(test)]
mod tests;

use gcg_effects::{Ev, Registry, StrId};

use crate::{
    Ctx, CtxVars, Derived, EngineError, EvKey, EventPayload, GameState, NO_ARG, Names, Traits,
    TriggerIndex, Uid, ViewKey,
};

pub use derived::{derived, same_values, trigger_index};
pub use eval::{matches, stat};
pub use query::{
    abilities_of, ap_of, cost_of, has_kw, hp_of, is_linked, keywords, kw_amount, level_of,
    link_satisfied, link_satisfied_unpatched, names_of, play_cost, play_level, rules_all, rules_of,
    traits_of, traits_present,
};
pub use signature::signature;

pub(crate) use pool::recycle;

#[allow(
    unused_imports,
    reason = "the L2 boundary names that layers L3-L5 call (docs/dev/contracts.md)"
)]
pub(crate) use eval::{cond, player_of, resolve, resolve_ref, select, value};
#[allow(
    unused_imports,
    reason = "the L2 boundary names that layers L3-L5 call (docs/dev/contracts.md)"
)]
pub(crate) use query::{cost_mod_of, player_cost_mod, zone_loc_code};

/// The `_view_key` of `Derived::empty()`: Python's `()` (`view.py:92`). The empty view is
/// never cached, so this key is never compared with a state's key.
const UNREAD_KEY: ViewKey = ViewKey {
    active: 0,
    turn: 0,
    n_battles: 0,
    battle: None,
    n_history: 0,
};

impl Ctx {
    /// `Ctx(controller, host=-1, card_uid=-1, vars=None, event=())` (`engine/view.py:32-44`):
    /// `card_uid` falls back to `host` when it is -1 (`:42`).
    #[must_use]
    pub fn new(
        controller: u8,
        host: i32,
        card_uid: i32,
        vars: CtxVars,
        event: EventPayload,
    ) -> Ctx {
        Ctx {
            controller,
            host,
            card_uid: if card_uid != NO_ARG { card_uid } else { host },
            vars,
            event,
        }
    }

    /// `Ctx.ev(key, default=-1)` (`engine/view.py:46-50`): the first matching payload pair.
    #[must_use]
    pub fn ev(&self, key: EvKey, default: i32) -> i32 {
        self.event.get(key).unwrap_or(default)
    }

    /// `ctx.ev(key, default)` for an IR key string (`EventAmount.key`, `EventFlag.key`,
    /// `EventCard.key`): a string outside the keys `emit` writes never matches, as Python's
    /// `ev` returns the default (docs/dev/contracts.md, gcg-engine differences).
    pub(crate) fn ev_str(&self, reg: &Registry, key: StrId, default: i32) -> i32 {
        EvKey::from_value(reg.str(key)).map_or(default, |k| self.ev(k, default))
    }

    /// `ctx.vars.get(var, ())` (`engine/view.py:43`, `:790`, `:870`): a frame's live vars
    /// (`interp.ctx_of` shares `f.vars`, `interp.py:57-58`) or a fresh empty dict.
    ///
    /// # Errors
    /// [`EngineError::Internal`] when the context names a frame the state no longer has
    /// (Python's context would still hold the popped frame's dict; no engine path reads it).
    pub(crate) fn var<'s>(
        &self,
        st: &'s GameState,
        var: gcg_effects::VarId,
    ) -> Result<&'s [i32], EngineError> {
        match self.vars {
            CtxVars::Empty => Ok(&[]),
            CtxVars::Frame(i) if usize::from(i) < st.frames.len() => {
                Ok(st.var_get(usize::from(i), var).unwrap_or(&[]))
            }
            CtxVars::Frame(i) => Err(ir_bug(&format!(
                "context vars of frame {i}, but the state has {} frames",
                st.frames.len()
            ))),
        }
    }
}

impl Derived {
    /// `_UNREAD = Derived()` (`engine/view.py:845`): the view `resolve_ref` passes for a
    /// reference that selects no cards. Every accessor reads it as all keys absent.
    #[must_use]
    pub fn empty() -> Derived {
        Derived::unallocated()
    }

    /// The `_view_key` this view was computed for (`engine/view.py:237`).
    #[must_use]
    pub fn key(&self) -> ViewKey {
        self.key
    }

    /// W-48: false when the fixpoint reached `FIXPOINT_PASSES` without converging
    /// (`engine/view.py:421-461`); the view is the last pass, as in Python.
    #[must_use]
    pub fn converged(&self) -> bool {
        self.converged
    }
}

impl TriggerIndex {
    /// `idx.get(ev, ())` (`engine/core.py:339`, `ai/actions.py:113`, `:210`): the Triggered
    /// abilities of `ev` in host order (`engine/view.py:314-333`).
    #[must_use]
    pub fn get(&self, ev: Ev) -> &[(Uid, gcg_effects::AbilityId)] {
        let i = ev as usize;
        let (a, b) = (self.offs[i] as usize, self.offs[i + 1] as usize);
        &self.items[a..b]
    }
}

impl<'a> Traits<'a> {
    /// The traits tuple (`engine/view.py:560-562`): the printed traits, then the granted ones
    /// (`view.py:383-384`).
    pub fn iter(&self, reg: &'a Registry) -> impl Iterator<Item = &'a str> + 'a {
        let printed = self.printed;
        let granted = self.granted;
        printed
            .iter()
            .map(|s| &**s)
            .chain(granted.iter().map(move |&id| reg.str(id)))
    }
}

impl<'a> Names<'a> {
    /// `names_of(cd)` (`engine/view.py:471-474`): `cd.names`, then the registry's `NameAlias`
    /// aliases (`registry.py:240-241`).
    pub fn iter(&self, reg: &'a Registry) -> impl Iterator<Item = &'a str> + 'a {
        let names = self.names;
        let aliases = self.aliases;
        names
            .iter()
            .map(|s| &**s)
            .chain(aliases.iter().map(move |&id| reg.str(id)))
    }
}

/// A uid read from an `int` slot (frame vars, event payloads, battles, history), checked
/// against the state: Python indexes `st.cards[u]` with it, and a uid outside the list is an
/// engine bug (`IndexError`), never a wrap-around.
pub(crate) fn uid_of(st: &GameState, x: i32) -> Result<Uid, EngineError> {
    match Uid::try_from(x) {
        Ok(u) if usize::from(u) < st.cards.len() => Ok(u),
        _ => Err(EngineError::Index(format!(
            "list index out of range (uid {x} of {} cards)",
            st.cards.len()
        ))),
    }
}

/// An internal invariant of the IR or the registry failed (a node of the wrong kind where
/// the generated types allow several, or a missing hook parameter set).
pub(crate) fn ir_bug(what: &str) -> EngineError {
    EngineError::Internal(format!("view: {what}"))
}
