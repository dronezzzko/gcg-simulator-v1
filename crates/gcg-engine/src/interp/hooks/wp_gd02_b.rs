//! Step hook of `effects/bindings/wp_gd02_b.py` (#9).

use gcg_effects::{Ev, VarId};

use super::modified_cost;
use crate::core::{self as core_};
use crate::interp::by_of;
use crate::interp::{pay_generic, to_uid, var_items};
use crate::{EngineError, EvKey, GameState, HistKind, NO_ARG, Zone};

/// `wp_gd02_b_pay_deploy_cost` (`wp_gd02_b.py:54-68`): "pay its cost to deploy it" (resolution
/// ambiguous:pay-its-cost-to-deploy): the controller rests Resources equal to the first chosen
/// card still in the trash's cost as modified there, with no Lv. check and no play-cost
/// modifiers; a positive payment with a host counts as a cost paid for this effect (Q286:
/// recorded and announced).
pub(super) fn pay_deploy_cost(
    st: &mut GameState,
    fi: usize,
    var: VarId,
) -> Result<bool, EngineError> {
    let mut first = None;
    for x in var_items(st, fi, var) {
        let u = to_uid(st, x)?;
        if st.cards[usize::from(u)].zone == Zone::Trash {
            first = Some(u);
            break;
        }
    }
    let Some(uid) = first else {
        return Ok(false);
    };
    let dv = crate::view::derived(st)?;
    let cost = modified_cost(st, &dv, uid);
    drop(dv);
    let (controller, host) = (st.frames[fi].controller, st.frames[fi].host);
    if !pay_generic(st, controller, cost, NO_ARG)? {
        return Ok(false);
    }
    if cost > 0 && host >= 0 {
        core_::record(
            st,
            HistKind::CostPaid,
            controller,
            by_of(controller),
            host,
            NO_ARG,
        );
        let amount = crate::interp::to_i32(cost, "cost paid")?;
        core_::emit(
            st,
            Ev::CostPaid,
            host,
            i32::from(controller),
            None,
            &[(EvKey::Amount, amount)],
        )?;
    }
    Ok(true)
}
