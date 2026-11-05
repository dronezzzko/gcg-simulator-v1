//! The reference behaviour of every extension point (`ai/mcts.py:82-339`, `ai/agents.py:85-98`):
//! the defaults of [`super::Mechanisms`]. With `portable_math` off ([`Math::Platform`]) every
//! function is the reference search's, float for float.

use gcg_ai::{
    Alternative, Density, MoveKey, NoopFlags, PruneFlags, SPENDING, burst_density, canonical,
    first_player_choice_with, merge_equivalent, mulligan_choice, option_scores, prune_dominated,
    score, without_effect_with,
};
use gcg_core::pymath::py_sum;
use gcg_engine::observe::determinize_into;
use gcg_engine::{Action, DecisionKind, EngineError, GameState};

use super::{
    BackupCx, Budget, ChildIdx, HorizonDecision, LeafCx, NodeCx, PregameCx, RolloutCx, RootCx,
    WorldCx, math_of,
};
use crate::mcts::is_target_decision;
use crate::tree::Child;
use gcg_ai::AgentError;

/// `WIN_DECAY` (`ai/config.py:9`).
pub(crate) const WIN_DECAY: f64 = 0.01;
/// `LEAF_FLOOR` (`ai/config.py:10`).
pub(crate) const LEAF_FLOOR: f64 = 0.05;

/// CPython `max(values)` of floats: the first value, replaced only by a strictly greater one.
pub(crate) fn py_max(values: &[f64]) -> f64 {
    let mut it = values.iter().copied();
    let mut best = it.next().expect("max() of a non-empty sequence");
    for v in it {
        if v > best {
            best = v;
        }
    }
    best
}

/// `ai/mcts.py:89-110`: `prune_dominated`, then (when a spending move is present, after one
/// draw for its seed) the moves `without_effect` finds idle are dropped unless none would be
/// left, then `merge_equivalent`. The no-effect check's evaluation uses the configuration's
/// math (`without_effect_with`; the reference's `without_effect` at `Math::Platform`).
pub(crate) fn root_moves(cx: &mut RootCx<'_>) -> Result<Vec<(MoveKey, Action)>, EngineError> {
    let (st, dec, cfg) = (cx.st, cx.dec, cx.cfg);
    let mut moves = prune_dominated(
        st,
        dec,
        canonical(st, dec),
        PruneFlags {
            strict_attacks: cfg.strict_attack_prune,
            rest_targets: cfg.prune_noop_rest_targets,
        },
    )?;
    if moves.iter().any(|(_, a)| SPENDING.contains(&a.kind)) {
        let seed = cx.rng.next_u64();
        let idle = without_effect_with(
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
            math_of(cfg).float(),
        )?;
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
        moves = merge_equivalent(st, dec, moves)?;
    }
    Ok(moves)
}

/// `config.budget(n)` (`ai/mcts.py:121`) and `_iteration_cap(config, dec, budget)`
/// (`ai/mcts.py:172-178`): for a target decision `max(budget, round(budget * target_extension))`
/// (`round` ties to even).
pub(crate) fn budget(cx: &RootCx<'_>, n_moves: usize) -> Budget {
    let iterations = cx.cfg.budget(n_moves);
    let cap = if is_target_decision(cx.dec) {
        // An integral float; `as` saturates out-of-range values, which no budget reaches.
        let extended = (f64::from(iterations) * cx.cfg.target_extension).round_ties_even() as u32;
        iterations.max(extended)
    } else {
        iterations
    };
    Budget { iterations, cap }
}

/// `burst_density(st, player)` of the root (`ai/mcts.py:120`).
pub(crate) fn density(cx: &RootCx<'_>) -> Density {
    burst_density(cx.st, cx.player)
}

/// `determinize(st, player, world_seeds[k])` (`ai/mcts.py:126`, `:143`).
pub(crate) fn world(
    cx: &mut WorldCx<'_>,
    k: usize,
    out: &mut GameState,
) -> Result<(), EngineError> {
    determinize_into(cx.st, cx.player, cx.seeds[k], out)
}

/// The reference policy's scores of a node's moves (`policy.option_scores`, in the moves'
/// order), the input of the priors.
///
/// # Errors
/// The scorer's engine error.
pub(crate) fn node_scores(cx: &NodeCx<'_>) -> Result<Vec<f64>, EngineError> {
    let options: Vec<Action> = cx.moves.iter().map(|&(_, a)| a).collect();
    option_scores(cx.s, cx.dec, &options, &cx.knobs)
}

/// `_add_children(node, s, dec, moves, cfg)` (`ai/mcts.py:305-317`): priors over the node's
/// moves, scored in `s`; an existing child keeps its first prior.
pub(crate) fn priors(cx: &mut NodeCx<'_>) -> Result<(), EngineError> {
    let scores = node_scores(cx)?;
    let top = py_max(&scores);
    let weights: Vec<f64> = scores
        .iter()
        .map(|&x| cx.math.exp((x - top) / cx.cfg.prior_temperature))
        .collect();
    let total = py_sum(weights.iter().copied());
    for (&ck, &wgt) in cx.keys.iter().zip(&weights) {
        cx.tree
            .setdefault(cx.node, ck, cx.dec.hdr.player, wgt / total);
    }
    Ok(())
}

/// `_select` (`ai/mcts.py:271-302`) after the priors and the `avail` counts: the unvisited move
/// with the best prior first (the first strictly greater), otherwise the first maximum of UCB1
/// plus the progressive prior.
pub(crate) fn select(cx: &mut NodeCx<'_>) -> ChildIdx {
    let children = &cx.tree.children;
    let mut best_unvisited: Option<(usize, f64)> = None;
    for (i, &c) in cx.children.iter().enumerate() {
        let ch = &children[c as usize];
        if ch.visits != 0 {
            continue;
        }
        if best_unvisited.is_none_or(|(_, p)| ch.prior > p) {
            best_unvisited = Some((i, ch.prior));
        }
    }
    if let Some((i, _)) = best_unvisited {
        return ChildIdx(i);
    }
    let (c, w, math) = (cx.cfg.exploration, cx.cfg.prior_weight, cx.math);
    let ucb = |ch: &Child| {
        let visits = f64::from(ch.visits);
        ch.mean()
            + c * math.sqrt(math.ln(f64::from(ch.avail)) / visits)
            + w * ch.prior / (visits + 1.0)
    };
    let mut best: Option<(usize, f64)> = None;
    for (i, &ci) in cx.children.iter().enumerate() {
        let v = ucb(&children[ci as usize]);
        if best.is_none_or(|(_, bv)| v > bv) {
            best = Some((i, v));
        }
    }
    ChildIdx(best.expect("a decision with options").0)
}

/// `policy.choose(s, rng, cfg.rollout_temperature, knobs_for(cfg))` (`ai/playout.py:24`).
pub(crate) fn rollout_choice(cx: &mut RolloutCx<'_>) -> Result<Action, EngineError> {
    cx.math
        .choose(cx.s, cx.rng, cx.cfg.rollout_temperature, &cx.knobs)
}

/// The reference stops at the horizon or the decision cap.
pub(crate) const fn horizon(_cx: &LeafCx<'_>) -> HorizonDecision {
    HorizonDecision::Leaf
}

/// The value of a loss of the searching player at the leaf's turn (`ai/mcts.py:259-264`):
/// `WIN_DECAY * (turn - root_turn)`.
pub(crate) fn loss_value(cx: &LeafCx<'_>) -> f64 {
    WIN_DECAY * f64::from(cx.s.turn() - cx.root_turn)
}

/// `_leaf_value(s, ctx)` (`ai/mcts.py:254-264`).
pub(crate) fn leaf_value(cx: &mut LeafCx<'_>) -> Result<f64, EngineError> {
    let s = cx.s;
    if let Some(w) = s.winner() {
        let elapsed = WIN_DECAY * f64::from(s.turn() - cx.root_turn);
        if i32::from(w) == i32::from(cx.player) {
            return Ok(1.0 - elapsed);
        }
        return Ok(if i32::from(w) == 1 - i32::from(cx.player) {
            elapsed
        } else {
            0.5
        });
    }
    let z = score(s, cx.player, cx.weights, cx.density)? / cx.cfg.value_scale;
    Ok(LEAF_FLOOR + (1.0 - 2.0 * LEAF_FLOOR) * cx.math.logistic(z))
}

/// `ai/mcts.py:214-217`: every child of the path, duplicates included, gains a visit and the
/// value from its mover's point of view.
pub(crate) fn backup(cx: &mut BackupCx<'_>) {
    for &c in cx.path {
        let child = &mut cx.tree.children[c as usize];
        child.visits += 1;
        child.total += if child.mover == cx.player {
            cx.value
        } else {
            1.0 - cx.value
        };
    }
}

/// `ai/agents.py:90-95`: REDRAW by `mulligan_choice`, CHOOSE_FIRST by `first_player_choice`
/// with the configuration's math (`first_player_choice_with`; the reference's at
/// `Math::Platform`). `mulligan_choice` makes no libm call.
pub(crate) fn pregame(
    cx: &mut PregameCx<'_, '_>,
) -> Result<(Action, Vec<Alternative>), AgentError> {
    if cx.dec.hdr.kind == DecisionKind::Redraw {
        return Ok(mulligan_choice(
            cx.st,
            cx.player,
            cx.cfg.mulligan_samples,
            cx.rng,
        ));
    }
    first_player_choice_with(
        cx.st,
        cx.player,
        cx.cfg.first_player_games,
        cx.rng,
        &cx.cfg.knobs(),
        cx.agent,
        math_of(cx.cfg).float(),
    )
}
