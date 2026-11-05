//! The engine-level step hooks of `effects/builtins.py`.

use gcg_effects::Ev;

use crate::core::{self as core_};
use crate::interp::by_of;
use crate::interp::{deploy::trash_excess, to_uid, var_items};
use crate::{EngineError, EvKey, GameState, HistKind, NO_ARG, Uid, Zone};

/// `return_looked_bottom` (`effects/builtins.py:13-29`): "Return the remaining cards randomly
/// to the bottom of your deck." The looked cards still in the deck are removed from their
/// owner's deck (first occurrence), shuffled (`n - 1` `randrange` draws) and appended; with
/// more than one card nobody knows the order (rule 4-2-2). Touches; false when none is left.
pub(super) fn return_looked_bottom(st: &mut GameState, fi: usize) -> Result<bool, EngineError> {
    let var = st.engine_vars().looked;
    let mut looked: Vec<Uid> = Vec::new();
    for x in var_items(st, fi, var) {
        let u = to_uid(st, x)?;
        if st.cards[usize::from(u)].zone == Zone::Deck {
            looked.push(u);
        }
    }
    let Some(&first) = looked.first() else {
        return Ok(false);
    };
    let owner = st.cards[usize::from(first)].owner;
    for &u in &looked {
        st.zone_remove(owner, Zone::Deck, u)?;
    }
    st.rng.shuffle(&mut looked);
    st.zone_extend(owner, Zone::Deck, &looked)?;
    if looked.len() > 1 {
        for &u in &looked {
            st.cards[usize::from(u)].known = 0;
        }
    }
    st.touch();
    Ok(true)
}

/// `deploy_ex_base` (`effects/builtins.py:32-43`): rule 5-17-3-1, every current Base of the
/// controller is trashed as excess (rule 11-5), then an EX Base token is created, recorded and
/// announced (DEPLOYED in the current group).
pub(super) fn deploy_ex_base(st: &mut GameState, fi: usize) -> Result<bool, EngineError> {
    let p = st.frames[fi].controller;
    let bases = st.zone(p, Zone::Base).to_vec();
    for b in bases {
        trash_excess(st, b)?;
    }
    let def = st.db().ex_base().def_id;
    let uid = core_::new_card(st, def, p, Zone::Base, false)?;
    core_::record(st, HistKind::Deployed, p, by_of(p), i32::from(uid), NO_ARG);
    core_::emit(
        st,
        Ev::Deployed,
        i32::from(uid),
        i32::from(p),
        None,
        &[(EvKey::By, i32::from(p))],
    )?;
    Ok(true)
}

/// `support_used` (`effects/builtins.py:65-72`): announces the use of <Support> (GD01-046)
/// with the supported Unit; false without one.
pub(super) fn support_used(st: &mut GameState, fi: usize) -> Result<bool, EngineError> {
    let var = st.engine_vars().t;
    let Some(&target) = var_items(st, fi, var).first() else {
        return Ok(false);
    };
    let (host, controller) = (st.frames[fi].host, st.frames[fi].controller);
    core_::emit(
        st,
        Ev::SupportUsed,
        host,
        i32::from(controller),
        None,
        &[(EvKey::Target, target)],
    )?;
    Ok(true)
}
