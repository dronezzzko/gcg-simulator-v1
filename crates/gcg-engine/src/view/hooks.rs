//! The 23 pure custom hooks (PLAN §5.3): 7 conds, 11 filters and 5 values that
//! the binding modules register in `view.CUSTOM_CONDS`, `CUSTOM_FILTERS` and `CUSTOM_VALUES`
//! (`engine/view.py:112-118`). Python calls them with `dict(params)` (`view.py:781`, `:882`,
//! `:933`); the registry decodes those parameters once at load (last key wins) into
//! [`CondParams`], [`FilterParams`] and [`ValueParams`].

use gcg_core::pymath::floordiv;
use gcg_effects::{
    CondId, CondParams, Ev, FilterParams, HookParams, Node, NodeId, SelId, StrId, ValueParams,
    VarId,
};

use super::eval::{cond as eval_cond, select, stat};
use super::query::{ap_of, level_of};
use super::{ir_bug, uid_of};
use crate::{
    Ctx, Derived, EngineError, EvKey, GameState, HistKind, NO_ARG, OnceElem, OnceKey, Uid, Zone,
};

fn params(st: &GameState, node: NodeId) -> Result<&HookParams, EngineError> {
    st.registry()
        .hook_params(node)
        .ok_or_else(|| ir_bug(&format!("custom hook node {} has no parameters", node.0)))
}

/// `CUSTOM_CONDS[c.name](st, dv, ctx, dict(c.params))` (`engine/view.py:933`).
///
/// # Errors
/// What the hook raises.
pub(crate) fn cond(
    st: &GameState,
    _dv: &Derived,
    ctx: &Ctx,
    node: NodeId,
) -> Result<bool, EngineError> {
    let &HookParams::Cond(p) = params(st, node)? else {
        return Err(ir_bug("a CustomCond with non-cond parameters"));
    };
    match p {
        CondParams::AttackTargetDamaged => attack_target_damaged(st),
        CondParams::GD01bDestroyedWithDamage => Ok(destroyed(st, ctx, Cause::WithDamage)),
        CondParams::GD04bPlayedWithEx => Ok(played_with_ex(st, ctx, ExScope::AfterNewestPlay)),
        CondParams::GD05aOnceFree { tag } => once_free(st, ctx, tag),
        CondParams::GD05aDestroyedByEffect { by_you } => {
            Ok(destroyed(st, ctx, Cause::ByEffect { by_you }))
        }
        CondParams::GD05bPlayedWithEx => Ok(played_with_ex(st, ctx, ExScope::NewestOfTheCard)),
        CondParams::ST12DestroyedByEffectDamage => Ok(destroyed(st, ctx, Cause::ByEffectDamage)),
    }
}

/// `CUSTOM_FILTERS[f.name](st, dv, ctx, uid, dict(f.params))` (`engine/view.py:780-781`).
///
/// # Errors
/// What the hook raises.
pub(crate) fn filter(
    st: &GameState,
    dv: &Derived,
    ctx: &Ctx,
    uid: Uid,
    node: NodeId,
) -> Result<bool, EngineError> {
    let &HookParams::Filter(p) = params(st, node)? else {
        return Err(ir_bug("a CustomFilter with non-filter parameters"));
    };
    match p {
        FilterParams::IsAttackTarget => Ok(is_attack_target(st, uid)),
        FilterParams::GD03aHasDestroyedEffect => Ok(has_destroyed_effect(st, dv, uid)),
        FilterParams::GD03aLowestHp => Ok(lowest_hp(st, dv, uid)),
        FilterParams::GD03bCondHolds { cond } => cond_holds(st, dv, ctx, cond),
        FilterParams::GD03bBattlingEnemyUnit => battling_enemy_unit(st, uid),
        FilterParams::GD04aPilotCapable => Ok(st.def_of(uid).is_pilot_capable()),
        FilterParams::GD04aHasPrintedKeyword => Ok(has_printed_keyword(st, uid)),
        FilterParams::GD04aHasPartnerInTrash { r#trait } => {
            Ok(has_partner_in_trash(st, uid, r#trait))
        }
        FilterParams::GD05bCanPair => Ok(crate::interp::can_pair(st, dv, uid)),
        FilterParams::GD06aNameUnlike { var } => name_unlike(st, ctx, uid, var),
        FilterParams::GD06bLinkNames { name } => Ok(link_names(st, uid, name)),
    }
}

/// `CUSTOM_VALUES[v.name](st, dv, ctx, dict(v.params))` (`engine/view.py:882`).
///
/// # Errors
/// What the hook raises.
pub(crate) fn value(
    st: &GameState,
    dv: &Derived,
    ctx: &Ctx,
    node: NodeId,
) -> Result<i64, EngineError> {
    let &HookParams::Value(p) = params(st, node)? else {
        return Err(ir_bug("a CustomValue with non-value parameters"));
    };
    match p {
        ValueParams::GD03aApPer { per } => ap_per(st, dv, ctx, per),
        ValueParams::GD03bUniqueNames { sel } => unique_names(st, dv, ctx, sel),
        ValueParams::GD05aLowestEnemyUnitLevel => {
            Ok(enemy_unit_level(st, ctx, false, Pick::Min).unwrap_or(0))
        }
        ValueParams::ST05HighestEnemyUnitLv => {
            Ok(enemy_unit_level(st, ctx, false, Pick::Max).unwrap_or(0))
        }
        ValueParams::ST12LowestRestedEnemyLv => {
            Ok(enemy_unit_level(st, ctx, true, Pick::Min).unwrap_or(-1))
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Conds

/// #4 `attack_target_damaged` (`effects/builtins.py:54-59`): a live battle whose target is a
/// card with damage.
fn attack_target_damaged(st: &GameState) -> Result<bool, EngineError> {
    match st.battle() {
        Some(b) if !b.ended && b.target >= 0 => Ok(st.card(uid_of(st, b.target)?).damage > 0),
        _ => Ok(false),
    }
}

/// How a destroyed card was destroyed (W-64: the three destroyed-cause conditions are one
/// predicate, [`destroyed`], with each card's exact test as this parameter; interpretation I-10
/// of `docs/dev/audit/interpretations.json`, `docs/ASSUMPTIONS.md:68`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Cause {
    /// #8 `wp_gd01_b_destroyed_with_damage` (`effects/bindings/wp_gd01_b.py:117-123`):
    /// `ctx.ev("battle", 0) == 1 or ctx.ev("by") == NO_ARG` (battle damage, or damage with no
    /// acting player: effect damage destroys "with damage").
    WithDamage,
    /// #27 `wp_gd05_a_destroyed_by_effect` (`effects/bindings/wp_gd05_a.py:122-131`): no battle
    /// and an acting player (a "destroy" effect), with `by="you"` by the controller.
    ByEffect { by_you: bool },
    /// #33 `wp_st12_14_destroyed_by_effect_damage` (`effects/bindings/wp_st12_14.py:83-93`): no
    /// battle, no acting player, and the instruction before the top frame's `pc` is a `Damage`
    /// (the flattened program layout, PLAN §5.2).
    ByEffectDamage,
}

/// W-64: the destroyed-cause predicate of the destruction event in `ctx` (see [`Cause`]).
fn destroyed(st: &GameState, ctx: &Ctx, cause: Cause) -> bool {
    let battle = ctx.ev(EvKey::Battle, 0);
    let by = ctx.ev(EvKey::By, NO_ARG);
    let held = match cause {
        Cause::WithDamage => battle == 1 || by == NO_ARG,
        Cause::ByEffect { by_you } => {
            battle == 0 && by >= 0 && (!by_you || by == i32::from(ctx.controller))
        }
        Cause::ByEffectDamage => battle == 0 && by < 0 && previous_instr_is_damage(st),
    };
    crate::diag::note_interpretation(crate::diag::Interpretation::I10);
    held
}

/// The instruction before the top frame's `pc` is a `Damage` (#33's frame test).
fn previous_instr_is_damage(st: &GameState) -> bool {
    let Some(f) = st.frames().last() else {
        return false;
    };
    let reg = &**st.registry();
    let instrs = &reg.program(f.program).instrs;
    let pc = usize::from(f.pc);
    0 < pc && pc <= instrs.len() && matches!(reg.node(instrs[pc - 1]), Node::Damage(_))
}

/// Which "play" record a `played_with_ex` hook reads (W-64: #23 and #31 are one scan,
/// [`played_with_ex`], with each card's exact semantics as this parameter).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ExScope {
    /// #23 `wp_gd04_b_played_with_ex` (`effects/bindings/wp_gd04_b.py:52-60`): the newest
    /// "play" record of `ctx.card_uid` is followed directly by its "played_with_ex" record.
    AfterNewestPlay,
    /// #31 `wp_gd05_b_played_with_ex` (`effects/bindings/wp_gd05_b.py:115-125`): newest first,
    /// the first record of `ctx.card_uid` that is "played_with_ex" (True) or "play" (False).
    NewestOfTheCard,
}

/// W-64: whether the card of `ctx` was played with an EX Resource, newest record first; the
/// records of other cards are skipped, and the card's newest "play" record decides unless
/// `scope` lets a newer "played_with_ex" record of the card decide first.
fn played_with_ex(st: &GameState, ctx: &Ctx, scope: ExScope) -> bool {
    let uid = ctx.card_uid;
    let history = st.history();
    for i in (0..history.len()).rev() {
        let h = &history[i];
        if h.uid != uid {
            continue;
        }
        match (h.kind, scope) {
            (HistKind::Play, ExScope::AfterNewestPlay) => {
                return history
                    .get(i + 1)
                    .is_some_and(|n| n.kind == HistKind::PlayedWithEx && n.uid == uid);
            }
            (HistKind::Play, ExScope::NewestOfTheCard) => return false,
            (HistKind::PlayedWithEx, ExScope::NewestOfTheCard) => return true,
            _ => {}
        }
    }
    false
}

/// #25 `wp_gd05_a_once_free` (`effects/bindings/wp_gd05_a.py:106-113`): `ctx.host >= 0 and
/// (tag, host, zone_seq) not in st.once_used`.
fn once_free(st: &GameState, ctx: &Ctx, tag: i64) -> Result<bool, EngineError> {
    if ctx.host < 0 {
        return Ok(false);
    }
    let host = uid_of(st, ctx.host)?;
    let tag = i32::try_from(tag).map_err(|_| EngineError::Overflow {
        what: "once-per-turn tag",
    })?;
    let key = OnceKey::from_elems(&[
        OnceElem::Int(tag),
        OnceElem::Int(ctx.host),
        OnceElem::Int(st.card(host).zone_seq),
    ])?;
    Ok(!st.once_contains(&key))
}

// ---------------------------------------------------------------------------------------------
// Filters

/// #3 `is_attack_target` (`effects/builtins.py:46-51`).
fn is_attack_target(st: &GameState, uid: Uid) -> bool {
    st.battle()
        .is_some_and(|b| !b.ended && b.target == i32::from(uid))
}

/// #11 `wp_gd03_a_has_destroyed_effect` (`effects/bindings/wp_gd03_a.py:62-76`): one of the
/// card's current abilities (`dv.abilities`, so unmet gates are excluded) is a self-only
/// 【Destroyed】 trigger.
fn has_destroyed_effect(st: &GameState, dv: &Derived, uid: Uid) -> bool {
    let reg = &**st.registry();
    dv.abilities_slice(uid).iter().any(|&aid| {
        let Node::Triggered(t) = reg.node(reg.ability(aid).ability.node()) else {
            return false;
        };
        matches!(reg.node(t.trigger.node()), Node::Trigger(tr) if tr.event == Ev::Destroyed && tr.self_only)
    })
}

/// #12 `wp_gd03_a_lowest_hp` (`effects/bindings/wp_gd03_a.py:79-89`): the card is in its
/// owner's battle area with the lowest REMAINING_HP there.
fn lowest_hp(st: &GameState, dv: &Derived, uid: Uid) -> bool {
    let units = st.zone(st.card(uid).owner, Zone::Battle);
    if !units.contains(&uid) {
        return false;
    }
    let lowest = units
        .iter()
        .map(|&u| stat(st, dv, u, gcg_effects::Stat::RemainingHp))
        .min();
    lowest == Some(stat(st, dv, uid, gcg_effects::Stat::RemainingHp))
}

/// #15 `wp_gd03_b_cond_holds` (`effects/bindings/wp_gd03_b.py:64-69`): a card-independent
/// condition (`V.cond` with its default `did=True`).
fn cond_holds(st: &GameState, dv: &Derived, ctx: &Ctx, c: CondId) -> Result<bool, EngineError> {
    eval_cond(st, dv, ctx, c, true)
}

/// #16 `wp_gd03_b_battling_enemy_unit` (`effects/bindings/wp_gd03_b.py:72-93`): the card is
/// the attacker or the target of a live battle whose other card is an enemy Unit in BATTLE.
fn battling_enemy_unit(st: &GameState, uid: Uid) -> Result<bool, EngineError> {
    let Some(&b) = st.battle() else {
        return Ok(false);
    };
    if b.ended {
        return Ok(false);
    }
    let u = i32::from(uid);
    let other = if u == b.attacker {
        b.target
    } else if u == b.target {
        b.attacker
    } else {
        return Ok(false);
    };
    if other < 0 {
        return Ok(false);
    }
    let o = uid_of(st, other)?;
    let oc = st.card(o);
    Ok(oc.owner != st.card(uid).owner
        && oc.zone == Zone::Battle
        && crate::core::card_type(st, o).is_unit())
}

/// #19 `wp_gd04_a_has_printed_keyword` (`effects/bindings/wp_gd04_a.py:62-66`): the printed
/// keywords dict is non-empty (a 0-amount keyword counts).
fn has_printed_keyword(st: &GameState, uid: Uid) -> bool {
    st.registry()
        .card(st.card(uid).def_id)
        .printed_keywords
        .0
        .iter()
        .any(Option::is_some)
}

/// #22 `wp_gd04_a_has_partner_in_trash` (`effects/bindings/wp_gd04_a.py:112-119`): another
/// card in the owner's trash has the trait among its printed traits.
fn has_partner_in_trash(st: &GameState, uid: Uid, r#trait: StrId) -> bool {
    let t = st.registry().str(r#trait);
    st.zone(st.card(uid).owner, Zone::Trash)
        .iter()
        .any(|&u| u != uid && st.def_of(u).traits.iter().any(|x| **x == *t))
}

/// #35 `wp_gd06_a_name_unlike` (`effects/bindings/wp_gd06_a.py:77-85`): the card's name differs
/// from the name of every card bound to the frame variable `var` ("with unique card names").
fn name_unlike(st: &GameState, ctx: &Ctx, uid: Uid, var: VarId) -> Result<bool, EngineError> {
    let name = &*st.def_of(uid).name;
    for &x in ctx.var(st, var)? {
        if *st.def_of(uid_of(st, x)?).name == *name {
            return Ok(false);
        }
    }
    Ok(true)
}

/// #36 `wp_gd06_b_link_names` (`effects/bindings/wp_gd06_b.py:95-103`): the Unit's link condition
/// names the Pilot `name` ("a Unit with the [Mashymre Cello] link condition").
fn link_names(st: &GameState, uid: Uid, name: StrId) -> bool {
    let n = st.registry().str(name);
    st.def_of(uid)
        .link
        .as_ref()
        .is_some_and(|l| l.name_parts.iter().any(|p| **p == *n))
}

// ---------------------------------------------------------------------------------------------
// Values

/// #10 `wp_gd03_a_ap_per` (`effects/bindings/wp_gd03_a.py:52-59`): `ap_of(host) // per`
/// (floor division; the AP is unclamped inside the fixpoint), 0 without a host.
fn ap_per(st: &GameState, dv: &Derived, ctx: &Ctx, per: i64) -> Result<i64, EngineError> {
    if ctx.host < 0 {
        return Ok(0);
    }
    if per == 0 {
        // Python raises ZeroDivisionError("division by zero"); the IR's only `per` is 4.
        return Err(EngineError::Internal(
            "ZeroDivisionError: division by zero (wp_gd03_a_ap_per)".to_owned(),
        ));
    }
    let host = uid_of(st, ctx.host)?;
    Ok(floordiv(i64::from(ap_of(st, dv, host)), per))
}

/// #17 `wp_gd03_b_unique_names` (`effects/bindings/wp_gd03_b.py:96-101`): the number of
/// distinct `CardDef.name` strings among the selected cards.
fn unique_names(st: &GameState, dv: &Derived, ctx: &Ctx, sel: SelId) -> Result<i64, EngineError> {
    let mut names: Vec<&str> = Vec::new();
    for u in select(st, dv, ctx, sel)? {
        let n = &*st.def_of(u).name;
        if !names.contains(&n) {
            names.push(n);
        }
    }
    i64::try_from(names.len()).map_err(|_| EngineError::Overflow {
        what: "unique names",
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Pick {
    Min,
    Max,
}

/// The lowest or highest `level_of` among the enemy (`1 - ctx.controller`) Units in BATTLE,
/// optionally only the rested ones; `None` for none (the hook's `default`):
/// #24 `wp_gd05_a_lowest_enemy_unit_level` (`wp_gd05_a.py:97-103`, default 0),
/// #32 `wp_st05_08_highest_enemy_unit_lv` (`wp_st05_08.py:46-51`, default 0),
/// #34 `wp_st12_14_lowest_rested_enemy_lv` (`wp_st12_14.py:96-102`, default -1).
fn enemy_unit_level(st: &GameState, ctx: &Ctx, rested_only: bool, pick: Pick) -> Option<i64> {
    let levels = st
        .zone(1 - ctx.controller, Zone::Battle)
        .iter()
        .filter(|&&u| !rested_only || st.card(u).rested)
        .map(|&u| i64::from(level_of(st, u)));
    match pick {
        Pick::Min => levels.min(),
        Pick::Max => levels.max(),
    }
}

#[cfg(test)]
mod tests {
    //! The hooks on a corpus state (`view/tests.rs` `corpus_state`), changed the way the
    //! engine changes it; the expected values follow the cited Python bodies.

    use gcg_data::DefId;
    use gcg_effects::{AbilityId, Node, ProgramId};

    use super::*;
    use crate::view::derived::blank;
    use crate::view::tests::corpus_state;
    use crate::view::{UNREAD_KEY, derived};
    use crate::{
        Battle, CtxVars, EventPayload, Frame, FrameKind, HistoryEvent, PLAYER_TARGET, Span,
    };

    fn ctx(controller: u8, host: i32, event: EventPayload) -> Ctx {
        Ctx::new(controller, host, NO_ARG, CtxVars::Empty, event)
    }

    fn event(pairs: &[(EvKey, i32)]) -> EventPayload {
        let mut ev = EventPayload::default();
        for &(k, v) in pairs {
            ev.push(k, v).unwrap();
        }
        ev
    }

    fn battle(attacker: i32, target: i32) -> Battle {
        Battle {
            attacker,
            attacker_seq: 0,
            target,
            target_seq: 0,
            defender: 1,
            battle_id: 3,
            blocked: false,
            damage_only: false,
            attack_triggers: true,
            ended: false,
        }
    }

    fn record(kind: HistKind, uid: i32) -> HistoryEvent {
        HistoryEvent {
            kind,
            player: 0,
            by: 0,
            uid,
            def_id: 0,
            source: NO_ARG,
        }
    }

    /// `views-000` trace 0: p0 BATTLE [0, 14, 17, 13, 18], p1 BATTLE [64, 67].
    fn state() -> GameState {
        corpus_state("views-000.ndjson", 0)
    }

    #[test]
    fn battle_hooks_read_the_live_battle() {
        let mut st = state();
        assert!(!is_attack_target(&st, 64));
        assert!(!attack_target_damaged(&st).unwrap());
        assert!(!battling_enemy_unit(&st, 0).unwrap());
        st.battles.push(battle(0, 64));
        assert!(is_attack_target(&st, 64) && !is_attack_target(&st, 0));
        assert!(!attack_target_damaged(&st).unwrap());
        st.cards[64].damage = 1;
        assert!(attack_target_damaged(&st).unwrap());
        assert!(battling_enemy_unit(&st, 0).unwrap());
        assert!(battling_enemy_unit(&st, 64).unwrap());
        assert!(!battling_enemy_unit(&st, 14).unwrap());
        st.battles.last_mut().unwrap().ended = true;
        assert!(!is_attack_target(&st, 64));
        assert!(!attack_target_damaged(&st).unwrap());
        assert!(!battling_enemy_unit(&st, 0).unwrap());
        st.battles.push(battle(0, PLAYER_TARGET));
        assert!(!attack_target_damaged(&st).unwrap());
        assert!(
            !battling_enemy_unit(&st, 0).unwrap(),
            "the player is no Unit"
        );
        st.battles.push(battle(0, 14));
        assert!(!battling_enemy_unit(&st, 0).unwrap(), "a friendly target");
    }

    /// The pre-W-64 bodies of the three destroyed-cause hooks and the two `played_with_ex`
    /// hooks, kept verbatim as the oracles of the consolidated ones.
    mod before_w64 {
        use super::super::previous_instr_is_damage;
        use crate::{Ctx, EvKey, GameState, HistKind, NO_ARG};

        pub(super) fn destroyed_with_damage(ctx: &Ctx) -> bool {
            ctx.ev(EvKey::Battle, 0) == 1 || ctx.ev(EvKey::By, NO_ARG) == NO_ARG
        }

        pub(super) fn destroyed_by_effect(ctx: &Ctx, by_you: bool) -> bool {
            let by = ctx.ev(EvKey::By, NO_ARG);
            if ctx.ev(EvKey::Battle, 0) != 0 || by < 0 {
                return false;
            }
            !by_you || by == i32::from(ctx.controller)
        }

        pub(super) fn destroyed_by_effect_damage(st: &GameState, ctx: &Ctx) -> bool {
            if ctx.ev(EvKey::Battle, 0) != 0 || ctx.ev(EvKey::By, NO_ARG) >= 0 {
                return false;
            }
            previous_instr_is_damage(st)
        }

        pub(super) fn gd04b_played_with_ex(st: &GameState, ctx: &Ctx) -> bool {
            let uid = ctx.card_uid;
            let history = st.history();
            for i in (0..history.len()).rev() {
                let h = &history[i];
                if h.kind == HistKind::Play && h.uid == uid {
                    return history
                        .get(i + 1)
                        .is_some_and(|n| n.kind == HistKind::PlayedWithEx && n.uid == uid);
                }
            }
            false
        }

        pub(super) fn gd05b_played_with_ex(st: &GameState, ctx: &Ctx) -> bool {
            for h in st.history().iter().rev() {
                if h.uid != ctx.card_uid {
                    continue;
                }
                if h.kind == HistKind::PlayedWithEx {
                    return true;
                }
                if h.kind == HistKind::Play {
                    return false;
                }
            }
            false
        }
    }

    /// #33 through the consolidated predicate, checked against the pre-W-64 body on every
    /// call (the frame states of `st12_damage_hook_reads_the_instruction_before_pc`).
    fn destroyed_by_effect_damage(st: &GameState, c: &Ctx) -> bool {
        let held = destroyed(st, c, Cause::ByEffectDamage);
        assert_eq!(held, before_w64::destroyed_by_effect_damage(st, c));
        held
    }

    fn gd04b_played_with_ex(st: &GameState, c: &Ctx) -> bool {
        played_with_ex(st, c, ExScope::AfterNewestPlay)
    }

    fn gd05b_played_with_ex(st: &GameState, c: &Ctx) -> bool {
        played_with_ex(st, c, ExScope::NewestOfTheCard)
    }

    #[test]
    fn destruction_hooks_read_the_event() {
        let st = state();
        let with_damage = |c: &Ctx| destroyed(&st, c, Cause::WithDamage);
        let by_effect = |c: &Ctx, by_you| destroyed(&st, c, Cause::ByEffect { by_you });
        let none = ctx(0, 0, EventPayload::default());
        assert!(with_damage(&none), "ev('by') defaults to -1");
        let battle = ctx(0, 0, event(&[(EvKey::Battle, 1), (EvKey::By, 1)]));
        assert!(with_damage(&battle));
        let effect = ctx(0, 0, event(&[(EvKey::Battle, 0), (EvKey::By, 1)]));
        assert!(!with_damage(&effect));
        assert!(by_effect(&effect, false));
        assert!(!by_effect(&effect, true), "by the opponent");
        let mine = ctx(1, 0, event(&[(EvKey::Battle, 0), (EvKey::By, 1)]));
        assert!(by_effect(&mine, true));
        assert!(!by_effect(&battle, false));
        assert!(!by_effect(&none, false), "no acting player");
    }

    /// W-64: the consolidated predicate equals each pre-W-64 body on every combination of the
    /// event's `battle` and `by` values, the controller, and a top frame whose previous
    /// instruction is or is not a `Damage`.
    #[test]
    fn w64_the_destroyed_cause_predicate_is_the_three_hooks() {
        let mut checked = 0;
        for frames in [false, true] {
            let mut st = state();
            if !frames {
                st.frames.clear();
            }
            for battle in [None, Some(0), Some(1), Some(2)] {
                for by in [None, Some(-1), Some(0), Some(1)] {
                    for controller in 0..2u8 {
                        let mut ev = Vec::new();
                        if let Some(b) = battle {
                            ev.push((EvKey::Battle, b));
                        }
                        if let Some(b) = by {
                            ev.push((EvKey::By, b));
                        }
                        let c = ctx(controller, 0, event(&ev));
                        assert_eq!(
                            destroyed(&st, &c, Cause::WithDamage),
                            before_w64::destroyed_with_damage(&c)
                        );
                        for by_you in [false, true] {
                            assert_eq!(
                                destroyed(&st, &c, Cause::ByEffect { by_you }),
                                before_w64::destroyed_by_effect(&c, by_you)
                            );
                        }
                        assert_eq!(
                            destroyed(&st, &c, Cause::ByEffectDamage),
                            before_w64::destroyed_by_effect_damage(&st, &c)
                        );
                        checked += 1;
                    }
                }
            }
        }
        assert_eq!(checked, 64);
    }

    /// W-64: the consolidated scan equals both pre-W-64 bodies on every history of up to five
    /// records over two uids and three record kinds.
    #[test]
    fn w64_the_played_with_ex_scan_is_the_two_hooks() {
        let kinds = [HistKind::Play, HistKind::PlayedWithEx, HistKind::Draw];
        let mut st = state();
        let c = ctx(0, 5, EventPayload::default());
        let mut histories = 0;
        for len in 0..=5u32 {
            for code in 0..6u32.pow(len) {
                st.history.clear();
                let mut x = code;
                for _ in 0..len {
                    let kind = kinds[(x % 3) as usize];
                    let uid = if (x / 3) % 2 == 0 { 5 } else { 7 };
                    st.history.push(record(kind, uid));
                    x /= 6;
                }
                assert_eq!(
                    gd04b_played_with_ex(&st, &c),
                    before_w64::gd04b_played_with_ex(&st, &c)
                );
                assert_eq!(
                    gd05b_played_with_ex(&st, &c),
                    before_w64::gd05b_played_with_ex(&st, &c)
                );
                histories += 1;
            }
        }
        assert_eq!(histories, 9331);
    }

    #[test]
    fn played_with_ex_hooks_scan_the_history() {
        let mut st = state();
        st.history.clear();
        let c = ctx(0, 5, EventPayload::default());
        assert!(!gd04b_played_with_ex(&st, &c) && !gd05b_played_with_ex(&st, &c));
        st.history.push(record(HistKind::Play, 5));
        st.history.push(record(HistKind::PlayedWithEx, 5));
        assert!(gd04b_played_with_ex(&st, &c) && gd05b_played_with_ex(&st, &c));
        st.history.push(record(HistKind::Play, 5));
        assert!(!gd04b_played_with_ex(&st, &c) && !gd05b_played_with_ex(&st, &c));
        st.history.push(record(HistKind::Draw, 7));
        assert!(!gd04b_played_with_ex(&st, &c) && !gd05b_played_with_ex(&st, &c));
        // GD04 needs the EX record right after the play; GD05 skips other uids' records.
        st.history.clear();
        st.history.push(record(HistKind::Play, 5));
        st.history.push(record(HistKind::Draw, 7));
        st.history.push(record(HistKind::PlayedWithEx, 5));
        assert!(!gd04b_played_with_ex(&st, &c));
        assert!(gd05b_played_with_ex(&st, &c));
    }

    #[test]
    fn once_free_reads_the_once_keys_of_host_and_zone_seq() {
        let mut st = state();
        let tag = -205_006;
        let c = ctx(0, 0, EventPayload::default());
        assert!(once_free(&st, &c, tag).unwrap());
        assert!(!once_free(&st, &ctx(0, NO_ARG, EventPayload::default()), tag).unwrap());
        let seq = st.cards[0].zone_seq;
        let key = |s| {
            OnceKey::from_elems(&[OnceElem::Int(-205_006), OnceElem::Int(0), OnceElem::Int(s)])
                .unwrap()
        };
        st.once_add(key(seq + 1));
        assert!(once_free(&st, &c, tag).unwrap(), "another zone_seq");
        st.once_add(key(seq));
        assert!(!once_free(&st, &c, tag).unwrap());
        assert!(once_free(&st, &c, tag + 1).unwrap(), "another tag");
    }

    #[test]
    fn st12_damage_hook_reads_the_instruction_before_pc() {
        let mut st = state();
        let reg = std::sync::Arc::clone(st.registry());
        let no_ev = ctx(0, 0, EventPayload::default());
        assert!(!destroyed_by_effect_damage(&st, &no_ev), "no frames");
        let (program, at) = (0..reg.db().len())
            .flat_map(|d| reg.card(DefId(u16::try_from(d).unwrap())).own.to_vec())
            .find_map(|aid: AbilityId| {
                let p: ProgramId = reg.ability(aid).program?;
                let i = reg
                    .program(p)
                    .instrs
                    .iter()
                    .position(|&n| matches!(reg.node(n), Node::Damage(_)))?;
                Some((p, i))
            })
            .expect("a program with a Damage step");
        let frame = |pc: usize| Frame {
            program,
            controller: 0,
            host: 0,
            host_seq: 0,
            card_uid: 0,
            kind: FrameKind::Trigger,
            event: EventPayload::default(),
            pc: u16::try_from(pc).unwrap(),
            vars: Span::EMPTY,
            did: true,
            answer: false,
            waiting: false,
            buf: Span::EMPTY,
            ints: Span::EMPTY,
            once_key: None,
            acted: false,
        };
        st.frame_push(frame(at + 1), &[]).unwrap();
        assert!(destroyed_by_effect_damage(&st, &no_ev));
        let by = ctx(0, 0, event(&[(EvKey::By, 0)]));
        assert!(!destroyed_by_effect_damage(&st, &by), "an acting player");
        let in_battle = ctx(0, 0, event(&[(EvKey::Battle, 1)]));
        assert!(!destroyed_by_effect_damage(&st, &in_battle), "a battle");
        st.frame_pop();
        st.frame_push(frame(0), &[]).unwrap();
        assert!(!destroyed_by_effect_damage(&st, &no_ev), "pc 0");
        st.frame_pop();
        let len = reg.program(program).instrs.len();
        st.frame_push(frame(len + 1), &[]).unwrap();
        assert!(
            !destroyed_by_effect_damage(&st, &no_ev),
            "pc past the program"
        );
    }

    #[test]
    fn enemy_level_hooks_use_their_defaults() {
        let mut st = state();
        let c = ctx(1, NO_ARG, EventPayload::default());
        let levels: Vec<i64> = [0u16, 14, 17, 13, 18]
            .iter()
            .map(|&u| i64::from(level_of(&st, u)))
            .collect();
        let (lo, hi) = (*levels.iter().min().unwrap(), *levels.iter().max().unwrap());
        assert_eq!(enemy_unit_level(&st, &c, false, Pick::Min), Some(lo));
        assert_eq!(enemy_unit_level(&st, &c, false, Pick::Max), Some(hi));
        assert_eq!(
            enemy_unit_level(&st, &c, true, Pick::Min),
            None,
            "none rested"
        );
        st.cards[17].rested = true;
        assert_eq!(
            enemy_unit_level(&st, &c, true, Pick::Min),
            Some(i64::from(level_of(&st, 17)))
        );
        let p0 = ctx(0, NO_ARG, EventPayload::default());
        st.zones.set(1, Zone::Battle, &[]).unwrap();
        assert_eq!(enemy_unit_level(&st, &p0, false, Pick::Min), None);
    }

    #[test]
    fn ap_per_floors_the_unclamped_ap() {
        let st = state();
        let reg = std::sync::Arc::clone(st.registry());
        let mut dv = blank(st.cards.len(), Some(reg.db()), UNREAD_KEY);
        dv.set_stats(0, -5, 1, st.cards[0].def_id).unwrap();
        let c = ctx(0, 0, EventPayload::default());
        assert_eq!(ap_per(&st, &dv, &c, 4).unwrap(), -2, "-5 // 4");
        dv.set_stats(0, 9, 1, st.cards[0].def_id).unwrap();
        assert_eq!(ap_per(&st, &dv, &c, 4).unwrap(), 2, "Q219: 9 AP gives 2");
        let no_host = ctx(0, NO_ARG, EventPayload::default());
        assert_eq!(ap_per(&st, &dv, &no_host, 4).unwrap(), 0);
        assert!(ap_per(&st, &dv, &c, 0).is_err());
    }

    #[test]
    fn trash_and_printed_keyword_filters_read_printed_data() {
        let st = state();
        let reg = std::sync::Arc::clone(st.registry());
        let trash = st.zone(0, Zone::Trash).to_vec();
        let mut checked = 0;
        for &u in &trash {
            for t in &st.def_of(u).traits {
                let Some(id) = reg.str_id(t) else {
                    continue;
                };
                let others = trash
                    .iter()
                    .any(|&v| v != u && st.def_of(v).traits.contains(t));
                assert_eq!(has_partner_in_trash(&st, u, id), others);
                checked += 1;
            }
            let printed = reg.card(st.card(u).def_id).printed_keywords;
            assert_eq!(
                has_printed_keyword(&st, u),
                printed.0.iter().any(Option::is_some)
            );
        }
        assert!(checked > 0);
    }

    #[test]
    fn lowest_hp_compares_remaining_hp_in_the_owners_battle_area() {
        let mut st = state();
        let dv = derived(&st).unwrap();
        let hp = |st: &GameState, u| stat(st, &dv, u, gcg_effects::Stat::RemainingHp);
        let p0 = [0u16, 14, 17, 13, 18];
        let low = p0.iter().map(|&u| hp(&st, u)).min().unwrap();
        for &u in &p0 {
            assert_eq!(lowest_hp(&st, &dv, u), hp(&st, u) == low);
        }
        assert!(!lowest_hp(&st, &dv, 1), "not in BATTLE");
        st.cards[18].damage = 50;
        assert!(lowest_hp(&st, &dv, 18));
    }
}
