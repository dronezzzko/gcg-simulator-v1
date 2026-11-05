//! The strengthened no-effect check (`prune_worlds`, `branch_all_selects`, `real_idle_line`;
//! W-12, W-141; docs/dev/PLAN.md §12.3 S6): `ai/noop.py` (reproduced in `gcg_ai::noop`) with
//!
//! - **worlds** (`prune_worlds{K}`): the move is judged in each of the first K search worlds
//!   instead of one world seeded from the search generator, and it is pruned only when it is
//!   without effect in all of them;
//! - **caps** (with `prune_worlds` or `branch_all_selects`): a judgement whose resolution,
//!   continuation, replay or branching stopped at a cap is unsure instead of being compared as
//!   complete;
//! - **every SELECT** (`branch_all_selects`): the branch check branches over every SELECT of
//!   the player in the move's resolution, not only the first, up to [`BRANCH_NODE_CAP`]
//!   branch states;
//! - **real idling** (`real_idle_line`): every policy move of an idle line comes from the
//!   decision's options without any play or activation of the move's card, so the baseline can
//!   never replay the move under test (`noop.py:203-205`, W-141).
//!
//! The worlds are judged in order and the first world in which the move has an effect ends its
//! judgement (the move is kept), so a move with an effect in world 0 costs what the reference's
//! one judgement costs. A move kept after some world judged it without effect or unsure is
//! returned as *penalised*: the root prior lowers it (fail-safe, W-12).

use gcg_ai::{Density, MoveKey, PolicyKnobs, SPENDING, Weights, apply_unchecked, burst_density};
use gcg_ai::{FloatMath, evaluate_with, option_scores, pick};
use gcg_engine::{
    Action, ActionKind, DecisionKind, DecisionView, EngineError, GameState, Uid, Zone,
};

/// `TOP_LEVEL` (`ai/noop.py:36`).
const TOP_LEVEL: [DecisionKind; 2] = [DecisionKind::Main, DecisionKind::ActionStep];
/// `MAX_DECISIONS` (`ai/noop.py:37`).
pub(crate) const MAX_DECISIONS: usize = 400;
/// `PASS` (`ai/noop.py:38`).
const PASS: Action = Action::of(ActionKind::Pass);
/// `_SPENT_ZONES` (`ai/noop.py:39`).
const SPENT_ZONES: [Zone; 2] = [Zone::ResourceArea, Zone::Outside];
/// The most branch states `branch_all_selects` creates for one move in one world; reaching it
/// makes the judgement unsure.
pub(crate) const BRANCH_NODE_CAP: usize = 24;

/// What the strengthened check switches on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Opts {
    /// Cap hits make a judgement unsure (`prune_worlds` or `branch_all_selects`).
    pub(crate) fail_safe: bool,
    pub(crate) branch_all: bool,
    pub(crate) real_idle: bool,
    /// `noop_branch_choices` of the configuration.
    pub(crate) branch_choices: bool,
    /// `noop_resource_aware` of the configuration.
    pub(crate) resource_aware: bool,
    /// [`MAX_DECISIONS`]; smaller only in the unit tests that force a cap hit.
    pub(crate) max_decisions: usize,
    pub(crate) node_cap: usize,
    /// Record the actions of the idle lines ([`judge_traced`]; the unit tests).
    pub(crate) record_idle: bool,
    /// The evaluation's floating-point library in the branch check (`portable_math`, W-137).
    pub(crate) math: FloatMath,
}

/// One move's judgement in one world.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Verdict {
    NoEffect,
    Effect,
    /// A cap was hit (only with `Opts::fail_safe`).
    Unsure,
}

/// The cost and the cap hits of the lines of one judgement.
#[derive(Default)]
struct Lines {
    applies: u64,
    capped: bool,
    /// The actions taken in the idle world, with `Opts::record_idle`.
    idle_actions: Vec<Action>,
}

/// The actions of the move's card that an idle line may not take (`real_idle_line`).
#[derive(Clone, Copy)]
struct Excluded {
    kind: ActionKind,
    card: i32,
}

impl Excluded {
    fn blocks(&self, a: &Action) -> bool {
        a.kind == self.kind && a.a == self.card
    }
}

/// One judging context: a world, the player and the next turn (`_Lines`, `ai/noop.py:169-194`).
struct Judging<'w> {
    world: &'w GameState,
    player: u8,
    turn: i32,
    knobs: &'w PolicyKnobs,
    opts: Opts,
    lines: Lines,
}

/// `_resolving(st)` (`ai/noop.py:59-64`).
pub(crate) fn resolving(st: &GameState) -> bool {
    st.winner().is_none()
        && st
            .pending()
            .is_some_and(|p| !st.frames().is_empty() || !TOP_LEVEL.contains(&p.hdr.kind))
}

impl Judging<'_> {
    fn apply(&mut self, st: &mut GameState, a: Action) -> Result<(), EngineError> {
        apply_unchecked(st, a)?;
        self.lines.applies += 1;
        Ok(())
    }

    /// An apply in the idle world.
    fn apply_idle(&mut self, st: &mut GameState, a: Action) -> Result<(), EngineError> {
        if self.opts.record_idle {
            self.lines.idle_actions.push(a);
        }
        self.apply(st, a)
    }

    /// `_policy_step(st, knobs)` (`ai/noop.py:55-56`), or the deterministic policy over the
    /// options the exclusion leaves (all options when it would leave none).
    fn policy_step(
        &self,
        st: &GameState,
        excluded: Option<Excluded>,
    ) -> Result<Action, EngineError> {
        let Some(ex) = excluded else {
            return gcg_ai::choose(st, None, 0.0, self.knobs);
        };
        let Some(dec) = st.pending() else {
            return Err(EngineError::Assertion(String::new()));
        };
        let options: Vec<Action> = dec
            .options
            .iter()
            .copied()
            .filter(|a| !ex.blocks(a))
            .collect();
        if options.is_empty() || options.len() == dec.options.len() {
            return gcg_ai::choose(st, None, 0.0, self.knobs);
        }
        if options.len() == 1 {
            return Ok(options[0]);
        }
        let scores = option_scores(st, &dec, &options, self.knobs)?;
        Ok(options[pick(&options, &scores, None, 0.0)])
    }

    /// `_resolve(st, action, knobs)` (`ai/noop.py:67-74`).
    fn resolve(&mut self, st: &mut GameState, action: Action) -> Result<(), EngineError> {
        self.apply(st, action)?;
        let mut n = 0;
        while resolving(st) && n < self.opts.max_decisions {
            let a = self.policy_step(st, None)?;
            self.apply(st, a)?;
            n += 1;
        }
        if resolving(st) {
            self.lines.capped = true;
        }
        Ok(())
    }

    /// `_branches(world, action, player, knobs)` (`ai/noop.py:77-96`): the resolutions after
    /// each option of the player's first SELECT with more than one option.
    fn branches(&mut self, action: Action) -> Result<Vec<GameState>, EngineError> {
        let mut st = self.world.clone();
        self.apply(&mut st, action)?;
        let mut n = 0;
        while resolving(&st) && n < self.opts.max_decisions {
            let dec = st.pending().expect("_resolving implies a pending decision");
            if self.branching(&dec) {
                let options = dec.options.to_vec();
                let mut out = Vec::with_capacity(options.len());
                for option in options {
                    let mut branch = st.clone();
                    self.resolve(&mut branch, option)?;
                    out.push(branch);
                }
                return Ok(out);
            }
            let a = self.policy_step(&st, None)?;
            self.apply(&mut st, a)?;
            n += 1;
        }
        if resolving(&st) {
            self.lines.capped = true;
        }
        Ok(Vec::new())
    }

    /// Whether the check branches at `dec`: a SELECT of the player with more than one option.
    fn branching(&self, dec: &DecisionView<'_>) -> bool {
        dec.hdr.kind == DecisionKind::Select
            && dec.hdr.player == self.player
            && dec.options.len() > 1
    }

    /// `branch_all_selects`: the resolutions after every combination of options of every SELECT
    /// of the player in the move's resolution (depth first, options in order), at most
    /// `node_cap` branch states; none when the resolution has no such SELECT.
    fn all_branches(&mut self, action: Action) -> Result<Vec<GameState>, EngineError> {
        let mut st = self.world.clone();
        self.apply(&mut st, action)?;
        let mut out = Vec::new();
        let mut nodes = 0usize;
        let branched = self.expand(st, &mut nodes, &mut out)?;
        if !branched {
            return Ok(Vec::new());
        }
        Ok(out)
    }

    /// Resolves `st` with the policy, branching at every SELECT of the player; pushes each
    /// finished resolution. Returns whether it branched.
    fn expand(
        &mut self,
        mut st: GameState,
        nodes: &mut usize,
        out: &mut Vec<GameState>,
    ) -> Result<bool, EngineError> {
        let mut n = 0;
        while resolving(&st) && n < self.opts.max_decisions {
            let dec = st.pending().expect("_resolving implies a pending decision");
            if self.branching(&dec) {
                let options = dec.options.to_vec();
                for option in options {
                    if *nodes >= self.opts.node_cap {
                        self.lines.capped = true;
                        return Ok(true);
                    }
                    *nodes += 1;
                    let mut branch = st.clone();
                    self.apply(&mut branch, option)?;
                    self.expand(branch, nodes, out)?;
                }
                return Ok(true);
            }
            let a = self.policy_step(&st, None)?;
            self.apply(&mut st, a)?;
            n += 1;
        }
        if resolving(&st) {
            self.lines.capped = true;
        }
        out.push(st);
        Ok(false)
    }

    /// `_continue(st, turn, knobs)` (`ai/noop.py:99-107`): the policy line to the next turn,
    /// recorded.
    fn continue_line(
        &mut self,
        st: &mut GameState,
        excluded: Option<Excluded>,
        idle: bool,
    ) -> Result<Vec<Action>, EngineError> {
        let mut actions = Vec::new();
        while st.winner().is_none()
            && st.turn() < self.turn
            && st.pending().is_some()
            && actions.len() < self.opts.max_decisions
        {
            let a = self.policy_step(st, excluded)?;
            if idle {
                self.apply_idle(st, a)?;
            } else {
                self.apply(st, a)?;
            }
            actions.push(a);
        }
        if st.winner().is_none() && st.turn() < self.turn && st.pending().is_some() {
            self.lines.capped = true;
        }
        Ok(actions)
    }

    /// `_replay(st, actions, turn, knobs)` (`ai/noop.py:110-125`), with the idle line's policy
    /// moves under the exclusion.
    fn replay(
        &mut self,
        st: &mut GameState,
        actions: &[Action],
        excluded: Option<Excluded>,
    ) -> Result<bool, EngineError> {
        let mut queue = actions;
        let mut n = 0;
        while st.winner().is_none() && st.turn() < self.turn && n < self.opts.max_decisions {
            let Some(dec) = st.pending() else { break };
            n += 1;
            if let Some((&next, rest)) = queue.split_first()
                && dec.options.contains(&next)
            {
                queue = rest;
                self.apply_idle(st, next)?;
            } else if dec.hdr.kind == DecisionKind::ActionStep && dec.options.contains(&PASS) {
                self.apply_idle(st, PASS)?;
            } else if !queue.is_empty() {
                return Ok(false);
            } else {
                let a = self.policy_step(st, excluded)?;
                self.apply_idle(st, a)?;
            }
        }
        if st.winner().is_none() && st.turn() < self.turn && st.pending().is_some() {
            self.lines.capped = true;
        }
        Ok(queue.is_empty())
    }

    /// `continue_and_replay(line)` (`ai/noop.py:179-184`).
    fn continue_and_replay(
        &mut self,
        line: &mut GameState,
        excluded: Option<Excluded>,
    ) -> Result<Option<GameState>, EngineError> {
        let actions = self.continue_line(line, None, false)?;
        let mut idled = self.world.clone();
        Ok(self
            .replay(&mut idled, &actions, excluded)?
            .then_some(idled))
    }

    /// `same(line, idled, spent)` (`ai/noop.py:186-189`).
    fn same(&self, line: &GameState, idled: &GameState, spent: i32) -> bool {
        only_costs_differ(line, idled, spent, self.player)
            && (!self.opts.resource_aware || resources_only_paid(line, idled, self.player))
    }

    /// `_a_branch_beats_idling(lines, action, spent, weights, density)`
    /// (`ai/noop.py:197-210`), with the idle baseline under the exclusion.
    fn a_branch_beats_idling(
        &mut self,
        action: Action,
        spent: i32,
        excluded: Option<Excluded>,
        weights: &Weights,
        density: Density,
    ) -> Result<bool, EngineError> {
        let branches = if self.opts.branch_all {
            self.all_branches(action)?
        } else {
            self.branches(action)?
        };
        for mut line in branches {
            let idled = match self.continue_and_replay(&mut line, excluded)? {
                None => {
                    let mut idled = self.world.clone();
                    self.continue_line(&mut idled, excluded, true)?;
                    idled
                }
                Some(idled) => {
                    if self.same(&line, &idled, spent) {
                        continue;
                    }
                    idled
                }
            };
            let player = self.player;
            let math = self.opts.math;
            if evaluate_with(&line, player, weights, density, math)?
                > evaluate_with(&idled, player, weights, density, math)?
            {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

/// The cards part of `_signature(st, spent)` (`ai/noop.py:43-47`).
fn signature_cards(
    st: &GameState,
    spent: i32,
) -> impl Iterator<Item = (usize, Zone, bool, i32, i32, u8)> + '_ {
    st.cards().iter().enumerate().filter_map(move |(uid, c)| {
        let skip = i32::try_from(uid).is_ok_and(|u| u == spent) || SPENT_ZONES.contains(&c.zone);
        (!skip).then_some((uid, c.zone, c.rested, c.damage, c.pair, c.known))
    })
}

/// `_only_costs_differ(played, idled, spent, player)` (`ai/noop.py:213-227`).
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

/// `_resources(st, player)` (`ai/noop.py:230-232`).
fn resources(st: &GameState, player: u8) -> (Vec<Uid>, usize) {
    let area = st.zone(player, Zone::ResourceArea);
    let mut set = area.to_vec();
    set.sort();
    set.dedup();
    let active = area.iter().filter(|&&u| !st.card(u).rested).count();
    (set, active)
}

/// `_resources_only_paid(played, idled, player)` (`ai/noop.py:235-240`).
fn resources_only_paid(played: &GameState, idled: &GameState, player: u8) -> bool {
    let (mine, mine_active) = resources(played, player);
    let (mine_idle, mine_idle_active) = resources(idled, player);
    let theirs = resources(played, 1 - player);
    let theirs_idle = resources(idled, 1 - player);
    mine.iter().all(|u| mine_idle.binary_search(u).is_ok())
        && mine_active <= mine_idle_active
        && theirs == theirs_idle
}

/// The judgement of one spending move in one world (the loop body of `without_effect`,
/// `ai/noop.py:150-165`); the applies it took are added to `applies`.
#[allow(
    clippy::too_many_arguments,
    reason = "the judgement's inputs, as the reference's without_effect takes them"
)]
pub(crate) fn judge(
    world: &GameState,
    player: u8,
    action: Action,
    knobs: &PolicyKnobs,
    opts: Opts,
    weights: &Weights,
    density: Density,
    applies: &mut u64,
) -> Result<Verdict, EngineError> {
    judge_traced(
        world, player, action, knobs, opts, weights, density, applies,
    )
    .map(|(v, _)| v)
}

/// [`judge`] with the actions of the idle lines (recorded with `Opts::record_idle`).
#[allow(
    clippy::too_many_arguments,
    reason = "the judgement's inputs, as the reference's without_effect takes them"
)]
pub(crate) fn judge_traced(
    world: &GameState,
    player: u8,
    action: Action,
    knobs: &PolicyKnobs,
    opts: Opts,
    weights: &Weights,
    density: Density,
    applies: &mut u64,
) -> Result<(Verdict, Vec<Action>), EngineError> {
    let mut j = Judging {
        world,
        player,
        turn: world.turn() + 1,
        knobs,
        opts,
        lines: Lines::default(),
    };
    let excluded = opts.real_idle.then_some(Excluded {
        kind: action.kind,
        card: action.a,
    });
    let verdict = judge_lines(&mut j, action, excluded, weights, density);
    *applies += j.lines.applies;
    let verdict = verdict?;
    let idle = std::mem::take(&mut j.lines.idle_actions);
    if verdict == Verdict::NoEffect && opts.fail_safe && j.lines.capped {
        return Ok((Verdict::Unsure, idle));
    }
    Ok((verdict, idle))
}

fn judge_lines(
    j: &mut Judging<'_>,
    action: Action,
    excluded: Option<Excluded>,
    weights: &Weights,
    density: Density,
) -> Result<Verdict, EngineError> {
    let mut played = j.world.clone();
    j.resolve(&mut played, action)?;
    let spent = if action.kind == ActionKind::PlayCommand {
        action.a
    } else {
        -1
    };
    let idle = j.continue_and_replay(&mut played, excluded)?;
    let Some(idled) = idle else {
        return Ok(Verdict::Effect);
    };
    if !j.same(&played, &idled, spent) {
        return Ok(Verdict::Effect);
    }
    if j.opts.branch_choices
        && j.a_branch_beats_idling(action, spent, excluded, weights, density)?
    {
        return Ok(Verdict::Effect);
    }
    Ok(Verdict::NoEffect)
}

/// What the strengthened check found for the spending moves.
#[derive(Default, Debug)]
pub(crate) struct Found {
    /// Keys of the moves without effect in every world, in move order.
    pub(crate) idle: Vec<MoveKey>,
    /// Keys kept only by the fail-safe (worlds disagree, or a judgement was unsure).
    pub(crate) penalised: Vec<MoveKey>,
    pub(crate) applies: u64,
}

/// The strengthened `without_effect` (`ai/noop.py:128-166`) over `worlds`, judged in order.
///
/// # Errors
/// The engine's error from a world, an `apply` or the policy.
#[allow(
    clippy::too_many_arguments,
    reason = "the inputs of the reference's without_effect plus the worlds"
)]
pub(crate) fn without_effect(
    st: &GameState,
    player: u8,
    dec: &DecisionView<'_>,
    moves: &[(MoveKey, Action)],
    worlds: &[&GameState],
    knobs: &PolicyKnobs,
    opts: Opts,
    weights: &Weights,
) -> Result<Found, EngineError> {
    let mut found = Found::default();
    let spending: Vec<(MoveKey, Action)> = moves
        .iter()
        .copied()
        .filter(|(_, a)| SPENDING.contains(&a.kind))
        .collect();
    if !TOP_LEVEL.contains(&dec.hdr.kind) || spending.is_empty() {
        return Ok(found);
    }
    let density = burst_density(st, player);
    for (key, action) in spending {
        let (mut none, mut some, mut unsure) = (0usize, 0usize, 0usize);
        for &world in worlds {
            match judge(
                world,
                player,
                action,
                knobs,
                opts,
                weights,
                density,
                &mut found.applies,
            )? {
                Verdict::NoEffect => none += 1,
                Verdict::Effect => some += 1,
                Verdict::Unsure => unsure += 1,
            }
            if some > 0 {
                break;
            }
        }
        if none == worlds.len() {
            if !found.idle.contains(&key) {
                found.idle.push(key);
            }
        } else if (none > 0 || unsure > 0) && !found.penalised.contains(&key) {
            found.penalised.push(key);
        }
    }
    Ok(found)
}
