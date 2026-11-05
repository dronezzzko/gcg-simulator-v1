//! Spending moves that change nothing (`ai/noop.py`).
//!
//! In one determinization of the player's information set the move is played and resolved,
//! the deterministic policy continues to the next turn while its actions are recorded, and the
//! same actions are replayed in the same world without the move. The move is without effect
//! when every recorded action was also legal without it and both lines end in the same
//! position apart from what was spent and the player's own cards rested to pay for it. With
//! `branch_choices`, a flagged move is re-checked once per option of the player's first choice
//! inside its resolution.

use gcg_engine::observe::determinize;
use gcg_engine::{
    Action, ActionKind, DecisionKind, DecisionView, EngineError, GameState, Uid, Zone,
};

use crate::agent::apply_unchecked;
use crate::evaluation::{burst_density, evaluate_with};
use crate::math::FloatMath;
use crate::policy;
use crate::{Density, MoveKey, NoopFlags, PolicyKnobs, Weights};

/// `SPENDING` (`ai/noop.py:35`).
pub const SPENDING: [ActionKind; 2] = [ActionKind::PlayCommand, ActionKind::Activate];
/// `TOP_LEVEL` (`ai/noop.py:36`).
const TOP_LEVEL: [DecisionKind; 2] = [DecisionKind::Main, DecisionKind::ActionStep];
/// `MAX_DECISIONS` (`ai/noop.py:37`).
const MAX_DECISIONS: usize = 400;
/// `PASS = Action(ActionKind.PASS)` (`ai/noop.py:38`).
const PASS: Action = Action::of(ActionKind::Pass);
/// `_SPENT_ZONES` (`ai/noop.py:39`).
const SPENT_ZONES: [Zone; 2] = [Zone::ResourceArea, Zone::Outside];

/// `_policy_step(st, knobs)` (`ai/noop.py:55-56`): the deterministic policy move.
fn policy_step(st: &GameState, knobs: &PolicyKnobs) -> Result<Action, EngineError> {
    policy::choose(st, None, 0.0, knobs)
}

/// `_resolving(st)` (`ai/noop.py:59-64`).
fn resolving(st: &GameState) -> bool {
    st.winner().is_none()
        && st
            .pending()
            .is_some_and(|p| !st.frames().is_empty() || !TOP_LEVEL.contains(&p.hdr.kind))
}

/// `_resolve(st, action, knobs)` (`ai/noop.py:67-74`).
fn resolve(st: &mut GameState, action: Action, knobs: &PolicyKnobs) -> Result<(), EngineError> {
    apply_unchecked(st, action)?;
    let mut n = 0;
    while resolving(st) && n < MAX_DECISIONS {
        let a = policy_step(st, knobs)?;
        apply_unchecked(st, a)?;
        n += 1;
    }
    Ok(())
}

/// `_branches(world, action, player, knobs)` (`ai/noop.py:77-96`).
fn branches(
    world: &GameState,
    action: Action,
    player: u8,
    knobs: &PolicyKnobs,
) -> Result<Vec<GameState>, EngineError> {
    let mut st = world.clone();
    apply_unchecked(&mut st, action)?;
    let mut n = 0;
    while resolving(&st) && n < MAX_DECISIONS {
        let dec = st.pending().expect("_resolving implies a pending decision");
        if dec.hdr.kind == DecisionKind::Select && dec.hdr.player == player && dec.options.len() > 1
        {
            let options = dec.options.to_vec();
            let mut out = Vec::with_capacity(options.len());
            for option in options {
                let mut branch = st.clone();
                resolve(&mut branch, option, knobs)?;
                out.push(branch);
            }
            return Ok(out);
        }
        let a = policy_step(&st, knobs)?;
        apply_unchecked(&mut st, a)?;
        n += 1;
    }
    Ok(Vec::new())
}

/// `_continue(st, turn, knobs)` (`ai/noop.py:99-107`): the policy line to `turn`, recorded.
fn continue_line(
    st: &mut GameState,
    turn: i32,
    knobs: &PolicyKnobs,
) -> Result<Vec<Action>, EngineError> {
    let mut actions = Vec::new();
    while st.winner().is_none()
        && st.turn() < turn
        && st.pending().is_some()
        && actions.len() < MAX_DECISIONS
    {
        let a = policy_step(st, knobs)?;
        apply_unchecked(st, a)?;
        actions.push(a);
    }
    Ok(actions)
}

/// `_replay(st, actions, turn, knobs)` (`ai/noop.py:110-125`): applies `actions` in order,
/// passing extra action steps; false as soon as a recorded action cannot be taken.
fn replay(
    st: &mut GameState,
    actions: &[Action],
    turn: i32,
    knobs: &PolicyKnobs,
) -> Result<bool, EngineError> {
    let mut queue = actions;
    let mut n = 0;
    while st.winner().is_none() && st.turn() < turn && n < MAX_DECISIONS {
        let Some(dec) = st.pending() else { break };
        n += 1;
        if let Some((&next, rest)) = queue.split_first()
            && dec.options.contains(&next)
        {
            queue = rest;
            apply_unchecked(st, next)?;
        } else if dec.hdr.kind == DecisionKind::ActionStep && dec.options.contains(&PASS) {
            apply_unchecked(st, PASS)?;
        } else if !queue.is_empty() {
            return Ok(false);
        } else {
            let a = policy_step(st, knobs)?;
            apply_unchecked(st, a)?;
        }
    }
    Ok(queue.is_empty())
}

/// `_Lines` (`ai/noop.py:169-194`): compares a line in which the move was played with idling
/// in the same world.
struct Lines<'w> {
    world: &'w GameState,
    player: u8,
    turn: i32,
    knobs: &'w PolicyKnobs,
    resource_aware: bool,
}

impl Lines<'_> {
    /// `continue_and_replay(line)` (`ai/noop.py:179-184`): continues `line` to the next turn;
    /// the world without the move with the same actions replayed, or `None` when they cannot
    /// all be taken.
    fn continue_and_replay(&self, line: &mut GameState) -> Result<Option<GameState>, EngineError> {
        let actions = continue_line(line, self.turn, self.knobs)?;
        let mut idled = self.world.clone();
        Ok(replay(&mut idled, &actions, self.turn, self.knobs)?.then_some(idled))
    }

    /// `same(line, idled, spent)` (`ai/noop.py:186-189`).
    fn same(&self, line: &GameState, idled: &GameState, spent: i32) -> bool {
        only_costs_differ(line, idled, spent, self.player)
            && (!self.resource_aware || resources_only_paid(line, idled, self.player))
    }

    /// `idle_line(line, spent)` (`ai/noop.py:191-194`).
    fn idle_line(
        &self,
        line: &mut GameState,
        spent: i32,
    ) -> Result<Option<GameState>, EngineError> {
        let idled = self.continue_and_replay(line)?;
        Ok(idled.filter(|idled| self.same(line, idled, spent)))
    }
}

/// `_a_branch_beats_idling(lines, action, spent, weights, density)` (`ai/noop.py:197-210`).
fn a_branch_beats_idling(
    lines: &Lines<'_>,
    action: Action,
    spent: i32,
    weights: &Weights,
    density: Density,
    math: FloatMath,
) -> Result<bool, EngineError> {
    let player = lines.player;
    for mut line in branches(lines.world, action, player, lines.knobs)? {
        let idled = match lines.continue_and_replay(&mut line)? {
            None => {
                let mut idled = lines.world.clone();
                continue_line(&mut idled, lines.turn, lines.knobs)?;
                idled
            }
            Some(idled) => {
                if lines.same(&line, &idled, spent) {
                    continue;
                }
                idled
            }
        };
        if evaluate_with(&line, player, weights, density, math)?
            > evaluate_with(&idled, player, weights, density, math)?
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// The cards part of `_signature(st, spent)` (`ai/noop.py:43-47`): uid order, without the
/// spent card and the spent zones.
fn signature_cards(
    st: &GameState,
    spent: i32,
) -> impl Iterator<Item = (usize, Zone, bool, i32, i32, u8)> + '_ {
    st.cards().iter().enumerate().filter_map(move |(uid, c)| {
        let skip = i32::try_from(uid).is_ok_and(|u| u == spent) || SPENT_ZONES.contains(&c.zone);
        (!skip).then_some((uid, c.zone, c.rested, c.damage, c.pair, c.known))
    })
}

/// `_only_costs_differ(played, idled, spent, player)` (`ai/noop.py:213-227`) over the
/// signatures of `ai/noop.py:42-52`: `(winner, turn, active)`, `(*lasting, *delayed)` as
/// `(effect, targets, duration, player)` in list order, and both decks must be equal; the
/// cards may differ only in the player's own cards rested in the played line and active in the
/// idle one.
fn only_costs_differ(played: &GameState, idled: &GameState, spent: i32, player: u8) -> bool {
    if played.winner() != idled.winner()
        || played.turn() != idled.turn()
        || played.active() != idled.active()
    {
        return false;
    }
    let lasting = |st: &GameState| {
        st.lasting()
            .iter()
            .chain(st.delayed())
            .map(|le| {
                (
                    le.effect,
                    st.lasting_targets(le).to_vec(),
                    le.duration,
                    le.player,
                )
            })
            .collect::<Vec<_>>()
    };
    if lasting(played) != lasting(idled) {
        return false;
    }
    if (0..2).any(|p| played.zone(p, Zone::Deck) != idled.zone(p, Zone::Deck)) {
        return false;
    }
    if signature_cards(played, spent).count() != signature_cards(idled, spent).count() {
        return false;
    }
    for (x, y) in signature_cards(played, spent).zip(signature_cards(idled, spent)) {
        if x == y {
            continue;
        }
        let (uid, zone, rested, damage, pair, known) = x;
        if uid != y.0 || zone != y.1 || (damage, pair, known) != (y.3, y.4, y.5) {
            return false;
        }
        let owner = played.cards()[uid].owner;
        if owner != player || !rested || y.2 {
            return false;
        }
    }
    true
}

/// `_resources(st, player)` (`ai/noop.py:230-232`): the resource-area set and its active count.
fn resources(st: &GameState, player: u8) -> (Vec<Uid>, usize) {
    let area = st.zone(player, Zone::ResourceArea);
    let mut set = area.to_vec();
    set.sort();
    set.dedup();
    let active = area.iter().filter(|&&u| !st.card(u).rested).count();
    (set, active)
}

/// `_resources_only_paid(played, idled, player)` (`ai/noop.py:235-240`): a subset test on the
/// player's own areas, equality on the opponent's.
fn resources_only_paid(played: &GameState, idled: &GameState, player: u8) -> bool {
    let (mine, mine_active) = resources(played, player);
    let (mine_idle, mine_idle_active) = resources(idled, player);
    let theirs = resources(played, 1 - player);
    let theirs_idle = resources(idled, 1 - player);
    mine.iter().all(|u| mine_idle.binary_search(u).is_ok())
        && mine_active <= mine_idle_active
        && theirs == theirs_idle
}

/// `without_effect(st, player, dec, moves, seed, knobs, branch_choices=, resource_aware=,
/// weights=)` (`ai/noop.py:128-166`): the keys of the spending moves that change nothing but
/// what they spend. A set in Python, used for membership only; the keys come out in move order.
///
/// # Errors
/// The engine's error from the determinisation, an `apply` or the policy.
#[allow(
    clippy::too_many_arguments,
    reason = "the frozen signature (docs/port/contracts.md, gcg-ai)"
)]
pub fn without_effect(
    st: &GameState,
    player: u8,
    dec: &DecisionView<'_>,
    moves: &[(MoveKey, Action)],
    seed: u64,
    knobs: &PolicyKnobs,
    f: NoopFlags,
    w: &Weights,
) -> Result<Vec<MoveKey>, EngineError> {
    without_effect_with(
        st,
        player,
        dec,
        moves,
        seed,
        knobs,
        f,
        w,
        FloatMath::Platform,
    )
}

/// [`without_effect`] with the evaluation's logistic at `math` in the branch check
/// (`_a_branch_beats_idling`, W-137): at [`FloatMath::Platform`] it is [`without_effect`], bit
/// for bit.
///
/// # Errors
/// As [`without_effect`].
#[allow(
    clippy::too_many_arguments,
    reason = "the frozen without_effect inputs plus the floating-point library"
)]
pub fn without_effect_with(
    st: &GameState,
    player: u8,
    dec: &DecisionView<'_>,
    moves: &[(MoveKey, Action)],
    seed: u64,
    knobs: &PolicyKnobs,
    f: NoopFlags,
    w: &Weights,
    math: FloatMath,
) -> Result<Vec<MoveKey>, EngineError> {
    let spending: Vec<(MoveKey, Action)> = moves
        .iter()
        .copied()
        .filter(|(_, a)| SPENDING.contains(&a.kind))
        .collect();
    if !TOP_LEVEL.contains(&dec.hdr.kind) || spending.is_empty() {
        return Ok(Vec::new());
    }
    let world = determinize(st, player, seed)?;
    let lines = Lines {
        world: &world,
        player,
        turn: world.turn() + 1,
        knobs,
        resource_aware: f.resource_aware,
    };
    let mut out: Vec<MoveKey> = Vec::new();
    let mut density: Option<Density> = None;
    for (key, action) in spending {
        let mut played = world.clone();
        resolve(&mut played, action, knobs)?;
        let spent = if action.kind == ActionKind::PlayCommand {
            action.a
        } else {
            -1
        };
        if lines.idle_line(&mut played, spent)?.is_none() {
            continue;
        }
        if f.branch_choices {
            let d = *density.get_or_insert_with(|| burst_density(st, player));
            if a_branch_beats_idling(&lines, action, spent, w, d, math)? {
                continue;
            }
        }
        if !out.contains(&key) {
            out.push(key);
        }
    }
    Ok(out)
}
