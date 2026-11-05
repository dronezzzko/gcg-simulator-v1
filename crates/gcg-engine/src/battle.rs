//! Layer L3 of the engine, part 2 (docs/dev/PLAN.md §5.5, §10.3.4): a function-for-function
//! counterpart of `engine/battle.py` — attack declaration, blocking, the damage step with First
//! Strike, Suppression and <Breach>'s payload, battle destruction, the end of a battle and
//! damage-only battles (rules section 8, keyword effects 13-1).
//!
//! Every `touch()` and `next_group()` is at Python's line: two touches in
//! `declare_attack` (`battle.py:16`, `:28`) and in `damage_only_battle` (`:277`, `:282`). The
//! views Python holds across damage (`damage_step`, `_exchange`, `_destroy_battle`) are held
//! here the same way.

#![allow(
    dead_code,
    reason = "called by layers L4 (interp: StartBattle) and L5 (game, procedures), which land after L3 (docs/dev/contracts.md)"
)]

use gcg_effects::{Ev, Kw, RuleKind};

use crate::core::{self, by_i8, card_at, to_i32, uid_at};
use crate::view;
use crate::{
    Battle, Ctx, CtxVars, DamageOpts, Duration, EndReason, EngineError, EvKey, EventPayload,
    GameState, HistKind, NO_ARG, PLAYER_TARGET, Step, Uid, UidBuf, Zone,
};

/// `st.battle` (`engine/state.py:521-523`) for the procedures that `assert b is not None`
/// (`battle.py:79`, `:199`, `:244`).
fn current_battle(st: &GameState) -> Result<Battle, EngineError> {
    st.battles
        .last()
        .copied()
        .ok_or_else(|| EngineError::Assertion("no battle in progress".to_owned()))
}

fn next_battle_id(st: &mut GameState) -> Result<i32, EngineError> {
    let id = st.next_battle_id;
    st.next_battle_id = id.checked_add(1).ok_or(EngineError::Overflow {
        what: "next_battle_id",
    })?;
    Ok(id)
}

/// `declare_attack(st, attacker, target)` (`battle.py:12-32`): rule 8-2-1 rests the attacker
/// and declares the target (a uid, or [`PLAYER_TARGET`]); then 【Attack】 effects trigger
/// (8-2-2) and the step becomes ATTACK_END_CHECK.
///
/// # Errors
/// Errors of emitting; overflows.
pub(crate) fn declare_attack(
    st: &mut GameState,
    attacker: Uid,
    target: i32,
) -> Result<(), EngineError> {
    st.cards[usize::from(attacker)].rested = true;
    st.touch(); // battle.py:16
    let a = st.cards[usize::from(attacker)];
    let defender = 1 - a.owner;
    let target_seq = if target >= 0 {
        card_at(st, target)?.zone_seq
    } else {
        0
    };
    let battle_id = next_battle_id(st)?;
    st.battles.push(Battle {
        attacker: i32::from(attacker),
        attacker_seq: a.zone_seq,
        target,
        target_seq,
        defender,
        battle_id,
        blocked: false,
        damage_only: false,
        attack_triggers: true,
        ended: false,
    });
    st.touch(); // battle.py:28
    let owner = by_i8(i32::from(a.owner))?;
    core::record(
        st,
        HistKind::Attack,
        a.owner,
        owner,
        i32::from(attacker),
        NO_ARG,
    );
    core::next_group(st);
    core::emit(
        st,
        Ev::Attacks,
        i32::from(attacker),
        i32::from(a.owner),
        None,
        &[
            (EvKey::Target, target),
            (EvKey::Defender, i32::from(defender)),
        ],
    )?;
    st.step = Step::AttackEndCheck;
    Ok(())
}

/// `battle_broken(st, b)` (`battle.py:35-44`, rules 8-2-4, 8-3-5, 8-4-2): the attacker, or a
/// targeted Unit or Base, left its location; a player target never breaks.
///
/// # Errors
/// Python's `IndexError` for a uid outside the card list.
pub(crate) fn battle_broken(st: &GameState, b: &Battle) -> Result<bool, EngineError> {
    let a = card_at(st, b.attacker)?;
    if a.zone != Zone::Battle || a.zone_seq != b.attacker_seq {
        return Ok(true);
    }
    if b.target >= 0 {
        let t = card_at(st, b.target)?;
        if !matches!(t.zone, Zone::Battle | Zone::Base) || t.zone_seq != b.target_seq {
            return Ok(true);
        }
    }
    Ok(false)
}

/// `eligible_blockers(st, b)` (`battle.py:47-73`, rules 8-3-1..8-3-3, 13-1-4, 13-1-6): the
/// defender's active <Blocker> Units other than the target, in list order, unless the
/// attacker has <High-Maneuver> or can't be blocked. Every "no blocker vs" rule is evaluated
/// for every candidate, as Python's loop does (no early exit).
///
/// # Errors
/// Errors of the view and of filter evaluation.
pub(crate) fn eligible_blockers(st: &GameState, b: &Battle) -> Result<Vec<Uid>, EngineError> {
    let dv = view::derived(st)?;
    let attacker = uid_at(st, b.attacker)?;
    if view::has_kw(&dv, attacker, Kw::HighManeuver)
        || view::rules_of(&dv, attacker, RuleKind::CantBeBlocked)
            .next()
            .is_some()
    {
        return Ok(Vec::new());
    }
    let reg = st.registry();
    let no_vs: Vec<_> = view::rules_of(&dv, attacker, RuleKind::NoBlockerVs)
        .copied()
        .collect();
    let mut out = Vec::new();
    for &uid in st.zones.get(b.defender, Zone::Battle) {
        let c = st.cards[usize::from(uid)];
        if c.rested || i32::from(uid) == b.target {
            continue;
        }
        if !view::has_kw(&dv, uid, Kw::Blocker) {
            continue;
        }
        if view::rules_of(&dv, uid, RuleKind::CantBlock)
            .next()
            .is_some()
        {
            continue;
        }
        let mut blocked_by_rule = false;
        for r in &no_vs {
            let rule = core::rule_mod(reg, r.rule)?;
            if rule.source_filters.is_empty() {
                blocked_by_rule = true;
                continue;
            }
            let ctx = Ctx::new(
                r.controller,
                r.source,
                NO_ARG,
                CtxVars::Empty,
                EventPayload::default(),
            );
            if view::matches(st, &dv, &ctx, uid, &rule.source_filters)? {
                blocked_by_rule = true;
            }
        }
        if blocked_by_rule {
            continue;
        }
        out.push(uid);
    }
    Ok(out)
}

/// `block(st, blocker)` (`battle.py:76-96`): rule 13-1-4-1 rests the Blocker and changes the
/// attack target to it (5-22-2); BLOCKS and BLOCKED share one group.
///
/// # Errors
/// Python's assertion without a battle; errors of emitting.
pub(crate) fn block(st: &mut GameState, blocker: Uid) -> Result<(), EngineError> {
    current_battle(st)?;
    st.cards[usize::from(blocker)].rested = true;
    let c = st.cards[usize::from(blocker)];
    let b = st
        .battles
        .last_mut()
        .ok_or_else(|| EngineError::Assertion("no battle in progress".to_owned()))?;
    b.target = i32::from(blocker);
    b.target_seq = c.zone_seq;
    b.blocked = true;
    let attacker = b.attacker;
    st.touch(); // battle.py:85
    let owner = by_i8(i32::from(c.owner))?;
    core::record(
        st,
        HistKind::Block,
        c.owner,
        owner,
        i32::from(blocker),
        NO_ARG,
    );
    let group = to_i32(core::next_group(st), "event group")?;
    core::emit(
        st,
        Ev::Blocks,
        i32::from(blocker),
        i32::from(c.owner),
        None,
        &[(EvKey::Attacker, attacker), (EvKey::Group, group)],
    )?;
    let attacker_owner = card_at(st, attacker)?.owner;
    core::emit(
        st,
        Ev::Blocked,
        attacker,
        i32::from(attacker_owner),
        None,
        &[(EvKey::Blocker, i32::from(blocker)), (EvKey::Group, group)],
    )
}

/// `victims: dict[int, int]` of `_destroy_battle` (`battle.py:99-111`): uid → destroyer uid in
/// insertion order. Building it from Python's dict literal `{x: attacker, y: target}`
/// (`battle.py:193`) with `x == y` keeps the key's first position and the last value
///, which [`Victims::insert`] reproduces.
#[derive(Default)]
struct Victims(Vec<(Uid, Uid)>);

impl Victims {
    fn of(pairs: &[(Uid, Uid)]) -> Self {
        let mut v = Self::default();
        for &(u, src) in pairs {
            v.insert(u, src);
        }
        v
    }

    fn insert(&mut self, u: Uid, src: Uid) {
        if let Some(e) = self.0.iter_mut().find(|e| e.0 == u) {
            e.1 = src;
        } else {
            self.0.push((u, src));
        }
    }

    fn get(&self, u: Uid) -> Option<Uid> {
        self.0.iter().find(|e| e.0 == u).map(|e| e.1)
    }

    /// `sorted(set(victims.values()))`.
    fn sources(&self) -> UidBuf {
        let mut s: UidBuf = self.0.iter().map(|e| e.1).collect();
        s.sort();
        s.dedup();
        s
    }
}

/// `_destroy_battle(st, victims)` (`battle.py:99-147`): the victims whose damage reached their
/// HP are destroyed in battle; then, in one group, each enemy destroyer "destroys a shield
/// area card" (a Base) or "destroys an enemy Unit with battle damage" with its <Breach> amount
/// and paired flag taken before the destruction (Q436: a destroyer that left the battle area
/// triggers from its last-known information; ST12-016:Q452/Q453).
fn destroy_battle(st: &mut GameState, victims: &Victims) -> Result<UidBuf, EngineError> {
    let dv = view::derived(st)?;
    let mut doomed: UidBuf = UidBuf::new();
    for &(u, _) in &victims.0 {
        let c = st.cards[usize::from(u)];
        if matches!(c.zone, Zone::Battle | Zone::Base) && c.damage >= view::hp_of(st, &dv, u) {
            doomed.push(u);
        }
    }
    if doomed.is_empty() {
        return Ok(doomed);
    }
    let sources = victims.sources();
    // breach and paired: read before the destruction (battle.py:109-110).
    let facts: Vec<(Uid, i32, bool)> = sources
        .iter()
        .map(|&src| {
            (
                src,
                view::kw_amount(&dv, src, Kw::Breach),
                st.cards[usize::from(src)].pair >= 0,
            )
        })
        .collect();
    let sources_lki = core::ability_snapshot(st, &sources)?;
    let destroyed = core::destroy(st, &doomed, true, NO_ARG, NO_ARG)?;
    let group = to_i32(core::next_group(st), "event group")?;
    for &u in &destroyed {
        let Some(src) = victims.get(u) else {
            continue;
        };
        let sc = st.cards[usize::from(src)];
        // Q436: a Unit destroyed simultaneously still "destroys an enemy Unit with battle damage"
        let lki = (sc.zone != Zone::Battle).then(|| sources_lki.only(src));
        if sc.owner == st.cards[usize::from(u)].owner {
            continue;
        }
        if st
            .db()
            .by_id(st.cards[usize::from(u)].def_id)
            .card_type
            .is_base()
        {
            core::emit(
                st,
                Ev::DestroysShieldCard,
                i32::from(src),
                i32::from(sc.owner),
                lki.as_ref(),
                &[
                    (EvKey::Target, i32::from(u)),
                    (EvKey::Battle, 1),
                    (EvKey::Group, group),
                ],
            )?;
            continue;
        }
        let owner = sc.owner;
        let by = by_i8(i32::from(owner))?;
        let (breach, paired) = facts
            .iter()
            .find(|f| f.0 == src)
            .map_or((0, false), |f| (f.1, f.2));
        core::record(
            st,
            HistKind::DestroysByBattle,
            owner,
            by,
            i32::from(src),
            NO_ARG,
        );
        if paired {
            // ruling ST12-016:Q452/Q453: paired when the destruction happened
            core::record(
                st,
                HistKind::DestroysByBattlePaired,
                owner,
                by,
                i32::from(src),
                NO_ARG,
            );
        }
        core::emit(
            st,
            Ev::DestroysByBattle,
            i32::from(src),
            i32::from(owner),
            lki.as_ref(),
            &[
                (EvKey::Target, i32::from(u)),
                (EvKey::Battle, 1),
                (EvKey::Breach, breach),
                (EvKey::Group, group),
            ],
        )?;
    }
    Ok(destroyed)
}

/// `_attack_base(st, attacker, base)` (`battle.py:150-162`): rule 8-5-2-4, the attacking Unit
/// deals battle damage to the Base; Bases deal none back (resolution rules-internal:base-ap
/// in docs/CONFLICTS.md).
fn attack_base(st: &mut GameState, attacker: Uid, base: Uid) -> Result<(), EngineError> {
    crate::diag::note_interpretation(crate::diag::Interpretation::I01);
    let dv = view::derived(st)?;
    let ap = view::ap_of(st, &dv, attacker);
    let by = i32::from(st.cards[usize::from(attacker)].owner);
    core::damage_card(
        st,
        base,
        i64::from(ap),
        DamageOpts {
            source: i32::from(attacker),
            battle: true,
            by,
        },
    )?;
    destroy_battle(st, &Victims::of(&[(base, attacker)]))?;
    Ok(())
}

/// `_exchange(st, attacker, target, first_strike)` (`battle.py:165-193`). With First Strike
/// the attacker's hit resolves and destroys first, and a destroyed target deals no damage
/// (rule 13-1-5-2); the target's AP is then read from a fresh view. Otherwise both hits are
/// resolved on the pre-damage state before either is applied (rule 8-5-3-2); redirected damage
/// is still battle damage (Q283), and both recipients are destroyed together (8-5-3-2-3).
fn exchange(
    st: &mut GameState,
    attacker: Uid,
    target: Uid,
    first_strike: bool,
) -> Result<(), EngineError> {
    let dv = view::derived(st)?;
    let a_owner = i32::from(st.cards[usize::from(attacker)].owner);
    let t_owner = i32::from(st.cards[usize::from(target)].owner);
    let atk_ap = i64::from(view::ap_of(st, &dv, attacker));
    let tgt_ap = i64::from(view::ap_of(st, &dv, target));
    let by_attacker = DamageOpts {
        source: i32::from(attacker),
        battle: true,
        by: a_owner,
    };
    let by_target = DamageOpts {
        source: i32::from(target),
        battle: true,
        by: t_owner,
    };
    if first_strike {
        let hit = core::resolve_damage(st, target, atk_ap, by_attacker)?;
        core::apply_damage(st, hit.0, hit.1, by_attacker)?;
        destroy_battle(st, &Victims::of(&[(hit.0, attacker)]))?;
        // rule 13-1-5-2: a destroyed target deals no damage
        if matches!(
            st.cards[usize::from(target)].zone,
            Zone::Battle | Zone::Base
        ) && st.cards[usize::from(attacker)].zone == Zone::Battle
        {
            let dv = view::derived(st)?;
            let back_ap = i64::from(view::ap_of(st, &dv, target));
            let back = core::resolve_damage(st, attacker, back_ap, by_target)?;
            core::apply_damage(st, back.0, back.1, by_target)?;
            destroy_battle(st, &Victims::of(&[(back.0, target)]))?;
        }
        return Ok(());
    }
    // rule 8-5-3-2: both Units deal damage simultaneously, so both hits are resolved on the
    // pre-damage state before either is applied; redirected damage (GD04-095) is still battle
    // damage, so its recipient is destroyed in battle (Q283)
    let to_target = core::resolve_damage(st, target, atk_ap, by_attacker)?;
    let to_attacker = core::resolve_damage(st, attacker, tgt_ap, by_target)?;
    core::apply_damage(st, to_target.0, to_target.1, by_attacker)?;
    core::apply_damage(st, to_attacker.0, to_attacker.1, by_target)?;
    destroy_battle(
        st,
        &Victims::of(&[(to_target.0, attacker), (to_attacker.0, target)]),
    )?; // rule 8-5-3-2-3
    Ok(())
}

/// `damage_step(st)` (`battle.py:196-238`, rule 8-5). First Strike and the attacker's AP come
/// from the view at the start. An attack on the player hits the Base (8-5-2-4), else the top
/// one Shield, or two with <Suppression> (13-1-7), each receiving the full AP (Q35: only AP 1
/// or more deals damage, 5-5-5), else defeats the defender with AP above 0 (8-5-2-2, 1-2-2-1);
/// a shield-area protection stops all of it. An attack on a Unit or Base exchanges damage.
///
/// # Errors
/// Python's assertion without a battle; [`EngineError::RedirectCycle`] (W-143); errors of the
/// view, damage, destruction and emitting.
pub(crate) fn damage_step(st: &mut GameState) -> Result<(), EngineError> {
    let b = current_battle(st)?;
    let dv = view::derived(st)?;
    let attacker = uid_at(st, b.attacker)?;
    let a_owner = st.cards[usize::from(attacker)].owner;
    let first_strike = view::has_kw(&dv, attacker, Kw::FirstStrike);
    if b.target == PLAYER_TARGET {
        let defender = b.defender;
        let base = st.zones.get(defender, Zone::Base).first().copied();
        let has_shields = !st.zones.get(defender, Zone::Shield).is_empty();
        if (base.is_some() || has_shields)
            && core::shield_area_protected(st, defender, b.attacker, true, i32::from(a_owner))?
        {
            return Ok(());
        }
        let d = DamageOpts {
            source: b.attacker,
            battle: true,
            by: i32::from(a_owner),
        };
        if let Some(base) = base {
            if view::has_kw(&dv, attacker, Kw::Suppression) {
                crate::diag::note_interpretation(crate::diag::Interpretation::I18);
            }
            attack_base(st, attacker, base)?; // rules 8-5-2-4, 8-5-2-4-2
        } else if has_shields {
            let ap = view::ap_of(st, &dv, attacker);
            if ap >= 1 {
                // Q35: each Shield has 1 HP; 0 damage is not dealt (5-5-5)
                let n = if view::has_kw(&dv, attacker, Kw::Suppression) {
                    2
                } else {
                    1
                }; // rule 13-1-7
                let top: UidBuf = st
                    .zones
                    .get(defender, Zone::Shield)
                    .iter()
                    .take(n)
                    .copied()
                    .collect();
                let shields = core::shields_receiving_damage(st, &top, i64::from(ap), d)?;
                core::destroy_shields(st, defender, &shields, d)?;
            }
        } else {
            let ap = view::ap_of(st, &dv, attacker);
            if ap > 0 {
                // rules 8-5-2-2, 1-2-2-1
                let by = by_i8(i32::from(a_owner))?;
                core::record(st, HistKind::PlayerDamage, defender, by, b.attacker, NO_ARG);
                core::set_winner(st, &[defender], EndReason::BattleDamage);
            } else {
                crate::diag::note_interpretation(crate::diag::Interpretation::I16);
            }
        }
        return Ok(());
    }
    let target = uid_at(st, b.target)?;
    if st.cards[usize::from(target)].zone == Zone::Base {
        return attack_base(st, attacker, target);
    }
    exchange(st, attacker, target, first_strike)
}

/// `end_battle(st)` (`battle.py:241-258`): rule 8-6-1, "during this battle" lasting effects
/// and delayed triggers of this battle end (order kept); one touch, a new group, BATTLE_END.
///
/// # Errors
/// Python's assertion without a battle; errors of emitting.
pub(crate) fn end_battle(st: &mut GameState) -> Result<(), EngineError> {
    current_battle(st)?;
    let b = st
        .battles
        .last_mut()
        .ok_or_else(|| EngineError::Assertion("no battle in progress".to_owned()))?;
    b.ended = true;
    let b = *b;
    let ends = |le: &crate::Lasting, _: &[(Uid, i32)]| {
        !(le.duration == Duration::ThisBattle && le.battle_id == b.battle_id)
    };
    let mut arena = st.lasting_arena();
    arena.retain_lasting(ends);
    arena.retain_delayed(ends);
    st.touch(); // battle.py:256
    core::next_group(st);
    let owner = card_at(st, b.attacker)?.owner;
    core::emit(
        st,
        Ev::BattleEnd,
        b.attacker,
        i32::from(owner),
        None,
        &[(EvKey::Target, b.target)],
    )
}

/// `damage_only_battle(st, attacker, target)` (`battle.py:261-282`; rules 5-22-3, 5-22-4,
/// 13-1-5-4): a battle that performs only the damage step, from the `StartBattle` instruction
/// (`interp.py:1495-1506`): no declaration, block, action step, ATTACKS event or step change.
/// It touches once after the battle is pushed ("battling" continuous effects apply, 5-22-4,
/// Q447, Q455) and once after it is popped.
///
/// # Errors
/// Errors of the damage step and of ending the battle; overflows.
pub(crate) fn damage_only_battle(
    st: &mut GameState,
    attacker: Uid,
    target: Uid,
) -> Result<(), EngineError> {
    let a = st.cards[usize::from(attacker)];
    let t = st.cards[usize::from(target)];
    let battle_id = next_battle_id(st)?;
    st.battles.push(Battle {
        attacker: i32::from(attacker),
        attacker_seq: a.zone_seq,
        target: i32::from(target),
        target_seq: t.zone_seq,
        defender: t.owner,
        battle_id,
        blocked: false,
        damage_only: true,
        attack_triggers: false,
        ended: false,
    });
    st.touch(); // battle.py:277
    damage_step(st)?;
    if st.winner.is_none() {
        end_battle(st)?;
    }
    st.battles.pop();
    st.touch(); // battle.py:282
    Ok(())
}

#[cfg(test)]
mod tests {
    //! `battle.*` calls of the layer-3 primitive vectors (`core::vectors`), plus the dict
    //! semantics of `_destroy_battle`'s victims.

    use super::*;
    use crate::core::vectors;

    #[test]
    fn attack_and_block_vectors_match_python() {
        assert!(vectors::replay("battle.declare_attack") > 0);
        assert!(vectors::replay("battle.battle_broken") > 0);
        assert!(vectors::replay("battle.eligible_blockers") > 0);
        assert!(vectors::replay("battle.block") > 0);
    }

    #[test]
    fn damage_step_vectors_match_python() {
        assert!(vectors::replay("battle.damage_step") > 0);
    }

    #[test]
    fn end_and_damage_only_battle_vectors_match_python() {
        assert!(vectors::replay("battle.end_battle") > 0);
        assert!(vectors::replay("battle.damage_only_battle") > 0);
    }

    /// `{to_target[0]: attacker, to_attacker[0]: target}` with equal keys keeps the key's
    /// first position and the last value (`battle.py:193`).
    #[test]
    fn victims_follow_dict_literal_semantics() {
        let v = Victims::of(&[(5, 1), (5, 2)]);
        assert_eq!(v.0, [(5, 2)]);
        let v = Victims::of(&[(5, 1), (3, 2), (5, 4)]);
        assert_eq!(v.0, [(5, 4), (3, 2)]);
        assert_eq!(v.get(3), Some(2));
        assert_eq!(v.get(9), None);
        assert_eq!(Victims::of(&[(5, 4), (3, 2), (7, 4)]).sources(), [2, 4]);
    }

    #[test]
    fn procedures_without_a_battle_fail_like_pythons_assert() {
        let (_, mut st) = crate::canon::fixtures::states()
            .into_iter()
            .find(|(_, st)| st.battles.is_empty() && st.winner.is_none())
            .expect("a fixture state without a battle");
        let before = crate::canon::state_digest(&st);
        for r in [damage_step(&mut st), end_battle(&mut st), block(&mut st, 0)] {
            assert!(matches!(r, Err(EngineError::Assertion(_))), "{r:?}");
        }
        assert_eq!(crate::canon::state_digest(&st), before);
    }
}
