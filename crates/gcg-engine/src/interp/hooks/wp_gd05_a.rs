//! Step hook of `effects/bindings/wp_gd05_a.py` (#26).

use crate::interp::{to_i32, to_uid};
use crate::{EngineError, GameState, OnceElem, OnceKey};

/// `wp_gd05_a_use_once` (`wp_gd05_a.py:116-119`): uses the 【Once per Turn】 that several
/// triggered abilities of one card share, `once_used.add((tag, host, host.zone_seq))`
/// (`_once_key`, `:102-103`), without checking it first (a set add, PLAN §4.3). The tag is
/// `_int_param(params, "tag")`, checked when the IR loads.
pub(super) fn use_once(st: &mut GameState, fi: usize, tag: i64) -> Result<bool, EngineError> {
    let host = st.frames[fi].host;
    let seq = st.cards[usize::from(to_uid(st, host)?)].zone_seq;
    let key = OnceKey::from_elems(&[
        OnceElem::Int(to_i32(tag, "once tag")?),
        OnceElem::Int(host),
        OnceElem::Int(seq),
    ])?;
    st.once_add(key);
    Ok(true)
}
