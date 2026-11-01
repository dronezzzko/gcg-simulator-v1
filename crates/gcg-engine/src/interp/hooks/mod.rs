//! The 13 custom step hooks (`interp.CUSTOM_STEPS`; Inv 04 §6.2 #1, #2, #5, #6, #7, #9, #13,
//! #14, #20, #21, #26, #28, #29), one Rust module per Python module (PLAN §5.3). Each body is a
//! literal port of the Python function its doc comment names; `_h_custom` sets `did` from the
//! result and touches afterwards (`interp.py:1571-1575`). A hook never raises a decision.
//!
//! The parameters arrive typed (`StepParams`, decoded once at load with `dict(params)`
//! last-key-wins; `crates/gcg-effects/src/hooks.rs`). The `ctx` Python passes is `ctx_of(f)`,
//! which the bodies rebuild where they read it.

mod builtins;
mod wp_eb01_a;
mod wp_gd02_b;
mod wp_gd03_a;
mod wp_gd04_a;
mod wp_gd05_a;
mod wp_gd05_b;

use gcg_effects::{Registry, StepParams};

use crate::{Derived, EngineError, GameState, Uid};

/// `CUSTOM_STEPS[ins.name](st, f, ctx_of(f), dict(ins.params))` (`interp.py:1572-1573`).
pub(super) fn run_step(
    st: &mut GameState,
    reg: &Registry,
    fi: usize,
    params: StepParams,
) -> Result<bool, EngineError> {
    match params {
        StepParams::ReturnLookedBottom => builtins::return_looked_bottom(st, fi),
        StepParams::DeployExBase => builtins::deploy_ex_base(st, fi),
        StepParams::SupportUsed => builtins::support_used(st, fi),
        // W-64: wp_eb01_a_look_own_top and wp_eb01_b_look_top_own have identical bodies
        // (wp_eb01_a.py:61-69, wp_eb01_b.py:240-248); the names stay distinct for `repr`.
        StepParams::EB01aLookOwnTop { player, var }
        | StepParams::EB01bLookTopOwn { player, var } => {
            wp_eb01_a::look_own_top(st, fi, player, var)
        }
        StepParams::GD02bPayDeployCost { var } => wp_gd02_b::pay_deploy_cost(st, fi, var),
        StepParams::GD03aPayCostOf { var } => wp_gd03_a::pay_cost_of(st, reg, fi, var),
        StepParams::GD03aApMinusCount { var, sel } => {
            wp_gd03_a::ap_minus_count(st, reg, fi, var, sel)
        }
        StepParams::GD04aCopyKeywords { var } => wp_gd04_a::copy_keywords(st, reg, fi, var),
        StepParams::GD04aApDownByNamedTrashUnits { name, var } => {
            wp_gd04_a::ap_down_by_named_trash_units(st, reg, fi, name, var)
        }
        StepParams::GD05aUseOnce { tag } => wp_gd05_a::use_once(st, fi, tag),
        StepParams::GD05bDestroyFirstShieldAreaCards { count } => {
            wp_gd05_b::destroy_first_shield_area_cards(st, fi, count)
        }
        StepParams::GD05bResolvingCommandToTrash => wp_gd05_b::resolving_command_to_trash(st, fi),
    }
}

/// `max(0, V.cost_of(st, uid) + dv.cost_mod.get(uid, 0))` (`wp_gd02_b.py:62`,
/// `wp_gd03_a.py:105-106`): the card's cost as modified where it is, without play-cost
/// modifiers.
fn modified_cost(st: &GameState, dv: &Derived, uid: Uid) -> i64 {
    (i64::from(crate::view::cost_of(st, uid)) + i64::from(crate::view::cost_mod_of(dv, uid))).max(0)
}

/// The `stop` of a Python slice `xs[:stop]` over `len` items: a negative stop counts from the
/// end, and both ends are clamped to the list.
fn slice_stop(len: usize, stop: i64) -> usize {
    let n = i64::try_from(len).unwrap_or(i64::MAX);
    let k = if stop < 0 {
        (n + stop).max(0)
    } else {
        stop.min(n)
    };
    usize::try_from(k).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::slice_stop;

    #[test]
    fn slice_stops_follow_python() {
        // [0, 1, 2][:k] for k = -4..4
        let lens: Vec<usize> = (-4..=4).map(|k| slice_stop(3, k)).collect();
        assert_eq!(lens, [0, 0, 1, 2, 0, 1, 2, 3, 3]);
        assert_eq!(slice_stop(0, 2), 0);
    }
}
