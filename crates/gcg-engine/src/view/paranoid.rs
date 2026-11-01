//! Feature `paranoid` (W-49, W-124): the view half of `tests/paranoid.py` `CacheChecker`.
//! Every cache hit of `derived` is recomputed and compared (`:59-73`), and every reference
//! resolved without the view is resolved again with it (`:125-132`). A mismatch panics with
//! the check's name: these are gate diagnostics, never part of a shipped build
//! (docs/port/contracts.md, gcg-engine behaviour).

use gcg_effects::RefId;

use super::derived::{build_index, compute, derived};
use super::eval::resolve;
use super::signature::signature;
use crate::{Ctx, Derived, EngineError, GameState};

/// `CacheChecker.derived` (`tests/paranoid.py:59-73`) on a cache hit: a fresh `_compute` has
/// the cached view's signature, and a filled trigger index equals the fresh view's.
///
/// # Errors
/// What the fresh compute raises.
///
/// # Panics
/// `paranoid: stale view` or `paranoid: stale trigger index`.
pub(crate) fn cached_view(st: &GameState, dv: &Derived) -> Result<(), EngineError> {
    let fresh = compute(st)?;
    assert!(
        signature(st, &fresh) == signature(st, dv),
        "paranoid: stale view, turn {}, step {:?}",
        st.turn,
        st.step
    );
    if let Some(idx) = dv.by_event.get() {
        assert!(
            build_index(st, &fresh) == *idx,
            "paranoid: stale trigger index, turn {}, step {:?}",
            st.turn,
            st.step
        );
    }
    Ok(())
}

/// `CacheChecker.resolve_ref` (`tests/paranoid.py:125-132`) for a reference that selects no
/// cards: resolving it against `derived(st)` gives the same uids.
///
/// # Errors
/// What `derived` or `resolve` raises.
///
/// # Panics
/// `paranoid: view-free reference`.
#[allow(
    dead_code,
    reason = "called by resolve_ref, whose callers land in layers L3-L5"
)]
pub(crate) fn view_free_ref(
    st: &GameState,
    ctx: &Ctx,
    r: RefId,
    got: &[crate::Uid],
) -> Result<(), EngineError> {
    let dv = derived(st)?;
    assert!(
        resolve(st, &dv, ctx, r)? == got,
        "paranoid: view-free reference {} resolved differently, turn {}",
        st.registry().repr(r.node()),
        st.turn
    );
    Ok(())
}
