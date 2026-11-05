//! Dominated-move pruning judged over K worlds (`prune_worlds`, `rest_prune_worlds`; W-12,
//! W-13; docs/dev/PLAN.md §12.3 S6).
//!
//! The reference trial-applies two prunes in one fixed determinisation (seed 0,
//! `_TRIAL_WORLD_SEED`, `ai/actions.py:136`): the idle 0-AP attack prune
//! (`_drop_idle_attacks`, `:139-150`) and the "while rested" exception of the Rest-target prune
//! (`_resting_changes_more_than_the_unit`, `:214-232`). Here the reference prune runs first
//! (`gcg_ai::prune_dominated`), and every move it removed by a trial is judged again in each of
//! the first K search worlds: it stays removed only when it is a no-op in all of them, so a move
//! whose effect appears only in some worlds is kept (fail-safe).

use gcg_ai::{MoveKey, PruneFlags, prune_dominated};
use gcg_engine::procedures::{action_step_options, declare_attack, set_rested_for_trial};
use gcg_engine::view::{derived, same_values};
use gcg_engine::{
    Action, ActionKind, DecisionKind, DecisionView, EngineError, GameState, Uid, Zone,
};

use super::worlds::Worlds;
use crate::mech::Mechanisms;

/// `uid` as an index of `st.cards` (`gcg_ai::actions::card_index`).
fn card_index(st: &GameState, uid: i32) -> Option<Uid> {
    let u = usize::try_from(uid).ok()?;
    if u < st.cards().len() {
        Uid::try_from(u).ok()
    } else {
        None
    }
}

/// `_attack_matters(world, player, attacker, target)` (`ai/actions.py:123-133`) in one world:
/// the declared attack queues a trigger of the player's or opens an action step with options.
fn attack_matters(world: &GameState, player: u8, a: Action) -> Result<bool, EngineError> {
    let mut trial = world.clone();
    let queued = trial.pending_triggers().len();
    let attacker = card_index(&trial, a.a)
        .ok_or_else(|| EngineError::Index(format!("list index out of range: {}", a.a)))?;
    declare_attack(&mut trial, attacker, a.b)?;
    let new = trial.pending_triggers().get(queued..).unwrap_or(&[]);
    if new.iter().any(|t| t.controller == player) {
        return Ok(true);
    }
    Ok(!action_step_options(&trial, player)?.is_empty())
}

/// `_resting_changes_more_than_the_unit(st, dec)` (`ai/actions.py:214-232`) in one world:
/// resting some active enemy Battle-Area candidate changes the derived view.
fn resting_changes_more_than_the_unit(
    world: &GameState,
    dec: &DecisionView<'_>,
) -> Result<bool, EngineError> {
    let before = derived(world)?;
    for o in dec.options {
        if o.kind != ActionKind::Select {
            continue;
        }
        let Some(uid) = card_index(world, o.a) else {
            continue;
        };
        let c = world.card(uid);
        if c.owner == dec.hdr.player || c.zone != Zone::Battle || c.rested {
            continue;
        }
        let mut rested = world.clone();
        set_rested_for_trial(&mut rested, uid, true);
        if !same_values(&*derived(&rested)?, &before) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// What the K-world prune switches on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Judge {
    /// `prune_worlds`: idle 0-AP attacks.
    pub(crate) attacks: bool,
    /// `rest_prune_worlds`: no-op Rest targets.
    pub(crate) rest_targets: bool,
}

/// `prune_dominated` (`ai/actions.py:242-277`) with the trial prunes judged again over the
/// worlds: an attack the reference drops is restored when it matters in some world, and the
/// Rest-target prune is undone when resting a candidate changes the view in some world.
/// Returns the moves and the number of trial applies (declared attacks, rested copies).
///
/// # Errors
/// The engine's error from the reference prune, a world or a trial.
pub(crate) fn prune<M: Mechanisms + ?Sized>(
    st: &GameState,
    dec: &DecisionView<'_>,
    moves: Vec<(MoveKey, Action)>,
    flags: PruneFlags,
    judge: Judge,
    worlds: &mut Worlds<'_, M>,
) -> Result<(Vec<(MoveKey, Action)>, u64), EngineError> {
    let kind = dec.hdr.kind;
    let pruned = prune_dominated(st, dec, moves.clone(), flags)?;
    if pruned.len() == moves.len() {
        return Ok((pruned, 0));
    }
    let mut trials = 0u64;
    if judge.rest_targets && kind == DecisionKind::Select && flags.rest_targets {
        for k in 0..worlds.len() {
            trials += 1;
            if resting_changes_more_than_the_unit(worlds.get(k)?, dec)? {
                return Ok((moves, trials));
            }
        }
        return Ok((pruned, trials));
    }
    if judge.attacks && kind == DecisionKind::Main && flags.strict_attacks {
        let player = dec.hdr.player;
        let mut kept = Vec::with_capacity(moves.len());
        for &(key, a) in &moves {
            let dropped = a.kind == ActionKind::Attack && !pruned.iter().any(|(k, _)| *k == key);
            let mut keep = !dropped;
            if dropped {
                for k in 0..worlds.len() {
                    trials += 1;
                    if attack_matters(worlds.get(k)?, player, a)? {
                        keep = true;
                        break;
                    }
                }
            }
            if keep {
                kept.push((key, a));
            }
        }
        return Ok((kept, trials));
    }
    Ok((pruned, trials))
}
