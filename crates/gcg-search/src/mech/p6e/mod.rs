//! P6-E's mechanisms (docs/dev/PLAN.md §12.2, §12.3 S6): root filters and targets.
//!
//! | Flag | Item | Module |
//! | --- | --- | --- |
//! | `ex_variants` | W-09 | `ex.rs`: EX-payment variants of EX-sensitive plays at the root |
//! | `typed_fingerprint` | W-10 | `fingerprint.rs`: uid-independent merge fingerprints |
//! | `prune_worlds{K}` | W-12 | `prune.rs`, `noop.rs`: pruning judged over the first K search worlds, fail-safe caps |
//! | `branch_all_selects` | W-12 | `noop.rs`: the branch check over every SELECT, node-capped |
//! | `rest_prune_worlds` | W-13 | `prune.rs`: the Rest-target exception judged over the K worlds |
//! | `target_rollouts` | W-14 | `critic.rs`: the target critic at effect-target decisions |
//! | `real_idle_line` | W-141 | `noop.rs`: an idle line that cannot replay the move under test |
//!
//! With every flag of the package off the root is the reference's (`reference::root_moves`).
//! The root moves are built in the reference's order (canonical, prune, no-effect check,
//! merge), each step replaced by its strengthened form when its flag is on. What the root found
//! that the search needs later (fail-safe penalties, EX variants, the critic) is kept in
//! [`Shared`] for the root priors and the root selection of the same search.

pub(crate) mod critic;
pub(crate) mod ex;
pub(crate) mod fingerprint;
pub(crate) mod noop;
pub(crate) mod prune;
pub(crate) mod worlds;

#[cfg(test)]
mod tests;

use std::sync::Mutex;

use gcg_ai::{MoveKey, NoopFlags, PruneFlags, SPENDING, canonical, merge_equivalent};
use gcg_ai::{ex_used, policy::EX_PENALTY, without_effect_with};
use gcg_core::pymath::py_sum;
use gcg_engine::observe::determinize;
use gcg_engine::{Action, EngineError, GameState};

use crate::MechanismFlags;
use crate::mcts::is_target_decision;
use crate::mech::reference::py_max;
use crate::mech::{BackupCx, Budget, ChildIdx, Mechanisms, NodeCx, RootCx};
use worlds::{DEFAULT_WORLDS, Worlds};

/// The mechanism flags this package implements.
pub(crate) const PROVIDES: [&str; 7] = [
    "branch_all_selects",
    "ex_variants",
    "prune_worlds",
    "real_idle_line",
    "rest_prune_worlds",
    "target_rollouts",
    "typed_fingerprint",
];

/// The prior factor of a move kept only by the no-effect check's fail-safe (W-12).
pub(crate) const FAIL_SAFE_PRIOR: f64 = 0.25;

/// The root node: the core creates it first (`mcts.rs`, `run`).
const ROOT: u32 = 0;

/// Whether any flag of the package is on.
pub(crate) fn any_on(f: &MechanismFlags) -> bool {
    f.ex_variants
        || f.typed_fingerprint
        || f.prune_worlds.is_some()
        || f.branch_all_selects
        || f.rest_prune_worlds
        || f.target_rollouts
        || f.real_idle_line
}

/// What one decision's root moves left for its root priors and its root selection.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct RootInfo {
    /// Keys kept only by the no-effect fail-safe: their prior is lowered.
    pub(crate) penalised: Vec<MoveKey>,
    /// EX variants: their prior drops the EX penalty.
    pub(crate) ex_free: Vec<MoveKey>,
    /// The target critic of an effect-target decision.
    pub(crate) critic: Option<critic::Critic>,
    /// The values backed up through each root child (by child index) while the critic has a
    /// best move.
    pub(crate) moments: Vec<(u32, critic::Moments)>,
    /// Engine applies of the root judgements (trials, no-effect lines, critic lines).
    pub(crate) applies: u64,
}

/// The root information of the running search, shared by the root moves, the root priors and
/// the root selection (one search per `Composed`).
#[derive(Debug, Default)]
pub(crate) struct Shared(Mutex<RootInfo>);

impl Shared {
    pub(crate) const fn new() -> Self {
        Self(Mutex::new(RootInfo {
            penalised: Vec::new(),
            ex_free: Vec::new(),
            critic: None,
            moments: Vec::new(),
            applies: 0,
        }))
    }

    /// The shared root information `info` (a root tree's copy of its decision's, S1).
    pub(crate) const fn from_info(info: RootInfo) -> Self {
        Self(Mutex::new(info))
    }

    /// The root information, moved out.
    pub(crate) fn into_info(self) -> RootInfo {
        self.0
            .into_inner()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn set(&self, info: RootInfo) {
        *self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = info;
    }

    fn with<T>(&self, f: impl FnOnce(&RootInfo) -> T) -> T {
        f(&self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner))
    }

    fn with_mut<T>(&self, f: impl FnOnce(&mut RootInfo) -> T) -> T {
        f(&mut self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner))
    }

    /// The engine applies of the root judgements of the last root moves.
    pub(crate) fn applies(&self) -> u64 {
        self.with(|i| i.applies)
    }

    /// The critic's best root move, when the critic found one.
    fn critic_best(&self) -> Option<MoveKey> {
        self.with(|i| i.critic.as_ref().and_then(|c| c.best.map(|b| c.keys[b])))
    }

    /// A copy of the root information.
    pub(crate) fn info(&self) -> RootInfo {
        self.with(Clone::clone)
    }
}

/// The root moves (`ai/mcts.py:89-110`) with P6-E's strengthened steps.
///
/// # Errors
/// The engine's error from a step.
pub(crate) fn root_moves<M: Mechanisms + ?Sized>(
    mech: &M,
    flags: &MechanismFlags,
    cx: &mut RootCx<'_>,
    shared: &Shared,
) -> Result<Vec<(MoveKey, Action)>, EngineError> {
    let (st, dec, cfg) = (cx.st, cx.dec, cx.cfg);
    let mut info = RootInfo::default();
    let (base, ex_free) = if flags.ex_variants {
        ex::canonical_with_ex(st, dec)
    } else {
        (canonical(st, dec), Vec::new())
    };
    info.ex_free = ex_free;
    let spending = base.iter().any(|(_, a)| SPENDING.contains(&a.kind));
    let strengthened_noop =
        flags.prune_worlds.is_some() || flags.branch_all_selects || flags.real_idle_line;
    // The reference draws its no-effect world's seed before the core's world seeds; the draw
    // is kept (so the search's own seeds stay the reference's) and the K search worlds are the
    // draws after it.
    let skip = usize::from(spending);
    let k = flags.prune_worlds.unwrap_or(DEFAULT_WORLDS).max(1);
    let mut worlds = Worlds::new(mech, st, cx.player, cfg, cx.rng, skip, k);
    let prune_flags = PruneFlags {
        strict_attacks: cfg.strict_attack_prune,
        rest_targets: cfg.prune_noop_rest_targets,
    };
    let (mut moves, trials) = prune::prune(
        st,
        dec,
        base,
        prune_flags,
        prune::Judge {
            attacks: flags.prune_worlds.is_some(),
            rest_targets: flags.rest_prune_worlds,
        },
        &mut worlds,
    )?;
    info.applies += trials;
    if spending {
        let seed = cx.rng.next_u64();
        let idle = if strengthened_noop {
            let single;
            let judged: Vec<&GameState> = if flags.prune_worlds.is_some() {
                worlds.all()?
            } else {
                single = determinize(st, cx.player, seed)?;
                vec![&single]
            };
            let found = noop::without_effect(
                st,
                cx.player,
                dec,
                &moves,
                &judged,
                &cx.knobs,
                noop::Opts {
                    fail_safe: flags.prune_worlds.is_some() || flags.branch_all_selects,
                    branch_all: flags.branch_all_selects,
                    real_idle: flags.real_idle_line,
                    branch_choices: cfg.noop_branch_choices,
                    resource_aware: cfg.noop_resource_aware,
                    max_decisions: noop::MAX_DECISIONS,
                    node_cap: noop::BRANCH_NODE_CAP,
                    record_idle: false,
                    math: crate::mech::math_of(cfg).float(),
                },
                &cfg.weights,
            )?;
            info.applies += found.applies;
            info.penalised = found.penalised;
            found.idle
        } else {
            without_effect_with(
                st,
                cx.player,
                dec,
                &moves,
                seed,
                &cx.knobs,
                NoopFlags {
                    branch_choices: cfg.noop_branch_choices,
                    resource_aware: cfg.noop_resource_aware,
                },
                &cfg.weights,
                crate::mech::math_of(cfg).float(),
            )?
        };
        let kept: Vec<(MoveKey, Action)> = moves
            .iter()
            .copied()
            .filter(|(k, _)| !idle.contains(k))
            .collect();
        if !kept.is_empty() {
            moves = kept;
        }
    }
    if cfg.merge_equivalent_moves {
        moves = if flags.typed_fingerprint {
            fingerprint::merge_equivalent(st, dec, moves)?
        } else {
            merge_equivalent(st, dec, moves)?
        };
    }
    if flags.target_rollouts && moves.len() > 1 && is_target_decision(dec) {
        let math = crate::mech::math_of(cfg);
        let (c, applies) = critic::run(cx, &moves, &mut worlds, math)?;
        info.applies += applies;
        info.critic = Some(c);
    }
    info.penalised.retain(|k| moves.iter().any(|(m, _)| m == k));
    info.ex_free.retain(|k| moves.iter().any(|(m, _)| m == k));
    shared.set(info);
    Ok(moves)
}

/// Whether the root priors differ from the reference's.
pub(crate) fn changes_priors(cx: &NodeCx<'_>, shared: &Shared) -> bool {
    cx.node == ROOT
        && shared.with(|i| {
            !i.penalised.is_empty()
                || !i.ex_free.is_empty()
                || i.critic.as_ref().is_some_and(|c| c.best.is_some())
        })
}

/// The root priors (`_add_children`, `ai/mcts.py:305-317`) with the EX penalty dropped for EX
/// variants, the fail-safe factor on penalised moves, and the critic's best move raised to the
/// highest prior (it is visited first). `scores` are the policy scores of the root moves (the
/// reference's `policy.option_scores`, or P6-F's strengthened scores).
pub(crate) fn priors(cx: &mut NodeCx<'_>, shared: &Shared, mut scores: Vec<f64>) {
    let (penalised, ex_free) = shared.with(|i| (i.penalised.clone(), i.ex_free.clone()));
    for (score, &(key, action)) in scores.iter_mut().zip(cx.moves) {
        if ex_free.contains(&key) {
            *score += EX_PENALTY * f64::from(ex_used(action));
        }
    }
    let top = py_max(&scores);
    let weights: Vec<f64> = scores
        .iter()
        .zip(cx.moves)
        .map(|(&x, (key, _))| {
            let w = cx.math.exp((x - top) / cx.cfg.prior_temperature);
            if penalised.contains(key) {
                w * FAIL_SAFE_PRIOR
            } else {
                w
            }
        })
        .collect();
    let mut weights = weights;
    if let Some(best) = shared.critic_best()
        && let Some(i) = cx.moves.iter().position(|(k, _)| *k == best)
    {
        let high = py_max(&weights);
        if weights[i] < high {
            weights[i] = high;
        }
    }
    let total = py_sum(weights.iter().copied());
    for (&ck, &wgt) in cx.keys.iter().zip(&weights) {
        cx.tree
            .setdefault(cx.node, ck, cx.dec.hdr.player, wgt / total);
    }
}

/// Whether the root selection differs from the reference's.
pub(crate) fn changes_select(cx: &NodeCx<'_>, shared: &Shared) -> bool {
    cx.node == ROOT && shared.critic_best().is_some()
}

/// The root selection with the critic's tie-break: the reference's selection (`_select`,
/// `ai/mcts.py:271-302`) during the first half of the budget; afterwards a selection of a move
/// other than the critic's best goes to the critic's best unless the search's own root values
/// favour the selected move by more than `SEARCH_Z` standard errors.
pub(crate) fn select(cx: &mut NodeCx<'_>, shared: &Shared) -> ChildIdx {
    let chosen = crate::mech::reference::select(cx);
    let Some(best) = shared.critic_best() else {
        return chosen;
    };
    let Some(b) = cx.moves.iter().position(|(k, _)| *k == best) else {
        return chosen;
    };
    let iterations = cx.tree.children[cx.children[0] as usize].avail;
    if chosen.0 == b || iterations <= cx.cfg.budget(cx.moves.len()) / 2 {
        return chosen;
    }
    let (chosen_child, best_child) = (cx.children[chosen.0], cx.children[b]);
    let beats = shared.with(|i| {
        let of = |c: u32| {
            i.moments
                .iter()
                .find(|(x, _)| *x == c)
                .map_or_else(critic::Moments::default, |&(_, m)| m)
        };
        of(chosen_child).beats(of(best_child), critic::SEARCH_Z, cx.math)
    });
    if beats { chosen } else { ChildIdx(b) }
}

/// Whether the root records its backed-up values (the critic has a best move).
pub(crate) fn records_backups(shared: &Shared) -> bool {
    shared.critic_best().is_some()
}

/// Records the value backed up through the root child of `cx.path` (from the searching
/// player's point of view, the root mover's).
pub(crate) fn record_backup(cx: &BackupCx<'_>, shared: &Shared) {
    let Some(&first) = cx.path.first() else {
        return;
    };
    let value = cx.value;
    shared.with_mut(|i| match i.moments.iter_mut().find(|(c, _)| *c == first) {
        Some((_, m)) => m.add(value),
        None => {
            let mut m = critic::Moments::default();
            m.add(value);
            i.moments.push((first, m));
        }
    });
}

/// The budget (`ai/mcts.py:121-124`): the reference's, without the close-call extension when
/// the critic has a best move.
pub(crate) fn budget(cx: &RootCx<'_>, n_moves: usize, shared: &Shared) -> Budget {
    let b = crate::mech::reference::budget(cx, n_moves);
    if shared.critic_best().is_some() {
        Budget {
            iterations: b.iterations,
            cap: b.iterations,
        }
    } else {
        b
    }
}
