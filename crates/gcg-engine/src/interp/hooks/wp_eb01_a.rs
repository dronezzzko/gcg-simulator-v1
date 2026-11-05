//! Step hooks of `effects/bindings/wp_eb01_a.py` (#6), whose body `wp_eb01_b.py` (#7)
//! repeats verbatim (W-64).

use gcg_effects::{P, VarId};

use crate::interp::{ctx_of, player_of, set_var_uids};
use crate::{EngineError, GameState, Uid, Zone};

/// `wp_eb01_a_look_own_top` (`wp_eb01_a.py:61-69`) and `wp_eb01_b_look_top_own`
/// (`wp_eb01_b.py:240-248`): the given player looks at the top card of their own deck, which
/// only that player learns (no touch); it is bound to `var`; false for an empty deck.
pub(super) fn look_own_top(
    st: &mut GameState,
    fi: usize,
    player: P,
    var: VarId,
) -> Result<bool, EngineError> {
    let player = player_of(st, &ctx_of(st, fi)?, player);
    let top: Vec<Uid> = st
        .zone(player, Zone::Deck)
        .iter()
        .take(1)
        .copied()
        .collect();
    for &u in &top {
        st.cards[usize::from(u)].known |= 1 << player;
    }
    set_var_uids(st, fi, var, &top)?;
    Ok(!top.is_empty())
}
