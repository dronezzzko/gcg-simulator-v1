//! Step hooks of `effects/bindings/wp_gd03_a.py` (#13, #14). Both re-enter the interpreter
//! with a node built at run time (`interp.execute`), which runs the same handler code as the
//! IR instruction (PLAN §5.3).

use gcg_effects::{Registry, SelId, VarId};

use super::modified_cost;
use crate::interp::{Synth, ctx_of, execute_synth, to_uid, var_items};
use crate::{EngineError, GameState};

/// `wp_gd03_a_pay_cost_of` (`wp_gd03_a.py:92-108`): GD03-051 "Pay its cost to deploy it": the
/// first chosen card's cost as modified where it is (no zone check) is paid with
/// `interp.execute(st, f, d.PayCost(cost))`, so it counts as a cost paid for this Unit's effect
/// (rulings GD04-100:Q286, GD04-129:Q297, GD05-101:Q400). Returns the frame's `did`.
pub(super) fn pay_cost_of(
    st: &mut GameState,
    reg: &Registry,
    fi: usize,
    var: VarId,
) -> Result<bool, EngineError> {
    let Some(&x) = var_items(st, fi, var).first() else {
        return Ok(false);
    };
    let uid = to_uid(st, x)?;
    let dv = crate::view::derived(st)?;
    let cost = modified_cost(st, &dv, uid);
    drop(dv);
    execute_synth(st, reg, fi, Synth::PayCost { amount: cost })?;
    Ok(st.frames[fi].did)
}

/// `wp_gd03_a_ap_minus_count` (`wp_gd03_a.py:111-127`): GD03-071 "for each X, it gets AP-1
/// during this turn", the count fixed when the effect resolves (cf. ruling GD01-059:Q135):
/// `interp.execute(st, f, d.Apply(d.Var(var), d.StatMod(ap=-n), THIS_TURN))`, which creates the
/// effect `StatMod(ap=-n, hp=0)` at run time and announces AP_REDUCED. False when the count is
/// 0 or nothing is chosen; else the frame's `did`.
pub(super) fn ap_minus_count(
    st: &mut GameState,
    reg: &Registry,
    fi: usize,
    var: VarId,
    sel: SelId,
) -> Result<bool, EngineError> {
    let dv = crate::view::derived(st)?;
    let n = crate::view::select(st, &dv, &ctx_of(st, fi)?, sel)?.len();
    drop(dv);
    if n == 0 || var_items(st, fi, var).is_empty() {
        return Ok(false);
    }
    let ap = i32::try_from(n)
        .map(|n| -n)
        .map_err(|_| crate::EngineError::Overflow { what: "StatMod ap" })?;
    execute_synth(st, reg, fi, Synth::ApplyVarStatMod { var, ap })?;
    Ok(st.frames[fi].did)
}
