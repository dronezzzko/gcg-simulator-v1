//! P6-B's leaf values (docs/port/PLAN.md §12.3 S3): `threat_leaf` (W-03) and the evaluation
//! features of `effective_blockers` (W-32) and `solver_can_kill` (W-38).
//!
//! `threat_leaf`: the leaf of a line is the first decision of the horizon turn, normally the
//! turn player's MAIN decision. When the static attack step of that player is lethal and the
//! solver proves a lethal line on the leaf's world, the leaf is scored as that player's win at
//! the leaf's turn, with the reference's time shaping (`1 - WIN_DECAY * (turn - root_turn)`
//! for the searcher, `WIN_DECAY * (turn - root_turn)` when it is the opponent, the loss value
//! of `ai/mcts.py:259-264`), instead of the evaluation's estimate.
//!
//! The features: the reference leaf (`ai/mcts.py:254-264`) is `LEAF_FLOOR + (1 - 2 LEAF_FLOOR)
//! * logistic(score / value_scale)` with the role features (`ai/evaluation.py:280-300`). With
//! `solver_can_kill` the two `can_kill` features are the exact static attack step
//! (`board::static_lethal`: the turn player's attack step now, the other player's next turn);
//! with `effective_blockers` the reference's count rule of `can_kill` uses the Blockers that
//! can block the attackers (`board::Board::blockers`, engine eligibility) instead of every
//! active <Blocker>, and the two race features are `1 / board::race`. The weights are the
//! configuration's; a weights set other than the role features keeps the reference leaf.
//! [`can_kill`] and [`race_feature`] are the functions P6-G's refit may use as features.

#[cfg(test)]
use gcg_ai::FeatureSet;
use gcg_ai::targeting::command_harms_enemy_units;
use gcg_ai::{ROLE_FEATURES, role_features};
use gcg_core::pymath::py_sum;
use gcg_data::CardType;
use gcg_engine::{DecisionKind, EngineError, GameState, Zone};

use super::DecisionState;
use super::board::{Board, Turn, player_attacks, race, static_lethal};
use super::solver::{Verdict, lethal_line};
use crate::MechanismFlags;
use crate::mech::LeafCx;
#[cfg(test)]
use crate::mech::reference::{self, LEAF_FLOOR, WIN_DECAY};

/// `RACE_CAP` (`ai/evaluation.py:59`).
const RACE_CAP: i64 = gcg_ai::evaluation::RACE_CAP;

/// The position of a role feature (`ROLE_FEATURES`).
fn role_index(name: &str) -> usize {
    ROLE_FEATURES
        .iter()
        .position(|&n| n == name)
        .expect("a role feature")
}

/// The leaf value from the searching player's point of view with P6-B's leaf mechanisms: the
/// package alone (its unit tests and the composition tests); the search composes the pieces
/// ([`proved_lethal`], [`feature_score`], [`swap_role_features`]) in `Composed::leaf_value`.
///
/// # Errors
/// The engine's errors of the evaluation and the static attack step; an unimplemented card's
/// error from the solver.
#[cfg(test)]
pub(crate) fn leaf_value(
    state: &DecisionState,
    flags: &MechanismFlags,
    cx: &mut LeafCx<'_>,
) -> Result<f64, EngineError> {
    let s = cx.s;
    if s.winner().is_some() {
        return reference::leaf_value(cx);
    }
    if flags.threat_leaf
        && let Some(winner) = proved_lethal(state, cx)?
    {
        let elapsed = WIN_DECAY * f64::from(s.turn() - cx.root_turn);
        return Ok(if winner == cx.player {
            1.0 - elapsed
        } else {
            elapsed
        });
    }
    if swaps_features(flags) && cx.weights.feature_set() == FeatureSet::Roles {
        return feature_leaf(flags, cx);
    }
    reference::leaf_value(cx)
}

/// The turn player at the leaf when it has a lethal line the solver proves from its MAIN
/// decision (attacks only), while the decision's solver budget lasts.
///
/// # Errors
/// As [`Board::of`]; an unimplemented card's error from the solver.
pub(crate) fn proved_lethal(
    state: &DecisionState,
    cx: &LeafCx<'_>,
) -> Result<Option<u8>, EngineError> {
    let s = cx.s;
    let Some(dec) = s.pending() else {
        return Ok(None);
    };
    let p = dec.hdr.player;
    if dec.hdr.kind != DecisionKind::Main || p != s.active() || !state.has_budget() {
        return Ok(None);
    }
    let attacks = player_attacks(dec.options);
    if attacks.is_empty() || !Board::of(s, p, Turn::Now)?.may_be_lethal() {
        return Ok(None);
    }
    let out = lethal_line(s, p, super::LINE_LIMITS, &cx.cfg.knobs(), &attacks)?;
    state.spend(out.applies);
    Ok(matches!(out.verdict, Verdict::Lethal(_)).then_some(p))
}

/// Whether P6-B changes the leaf evaluation's features (`effective_blockers`,
/// `solver_can_kill`).
pub(crate) const fn swaps_features(flags: &MechanismFlags) -> bool {
    flags.solver_can_kill || flags.effective_blockers
}

/// The role-feature leaf with P6-B's `can_kill` and race features.
#[cfg(test)]
fn feature_leaf(flags: &MechanismFlags, cx: &LeafCx<'_>) -> Result<f64, EngineError> {
    let z = feature_score(flags, cx, cx.density)? / cx.cfg.value_scale;
    Ok(LEAF_FLOOR + (1.0 - 2.0 * LEAF_FLOOR) * cx.math.logistic(z))
}

/// The configuration's role-feature score of the leaf from the searching player's point of
/// view, with P6-B's `can_kill` and race features and the Burst density `density`.
///
/// # Errors
/// The derived view's error; as [`Board::of`].
pub(crate) fn feature_score(
    flags: &MechanismFlags,
    cx: &LeafCx<'_>,
    density: gcg_ai::Density,
) -> Result<f64, EngineError> {
    let s = cx.s;
    let mut r = role_features(s, density)?;
    swap_role_features(flags, s, &mut r)?;
    let z = py_sum(cx.weights.values().iter().zip(&r).map(|(w, x)| w * x));
    Ok(if cx.player == s.active() { z } else { -z })
}

/// Replaces the `can_kill` features of the role features `r` of `s` (`ROLE_FEATURES` order,
/// turn player first; P6-G's superset features start with that block) by P6-B's, and with
/// `effective_blockers` the race features too; with neither flag on `r` is unchanged.
///
/// # Errors
/// As [`Board::of`].
pub(crate) fn swap_role_features(
    flags: &MechanismFlags,
    s: &GameState,
    r: &mut [f64],
) -> Result<(), EngineError> {
    if !swaps_features(flags) {
        return Ok(());
    }
    let turn_player = s.active();
    let other = 1 - turn_player;
    let (ck_tp, ck_other) = (
        role_index("can_kill_turn_player"),
        role_index("can_kill_other"),
    );
    r[ck_tp] = can_kill(s, turn_player, flags.solver_can_kill)?;
    r[ck_other] = can_kill(s, other, flags.solver_can_kill)?;
    if flags.effective_blockers {
        r[role_index("inv_race_turn_player")] = race_feature(s, turn_player)?;
        r[role_index("inv_race_other")] = race_feature(s, other)?;
    }
    Ok(())
}

/// The `can_kill` feature of `attacker` (1.0 or 0.0) for P6-G and the leaf: with `exact` the
/// static attack step (`board::static_lethal`; the attack step now for the turn player, the
/// next turn for the other player), otherwise the reference's count rule (`_can_kill`,
/// `ai/evaluation.py:205-212`: attackers minus the Blockers the castable removal in hand does
/// not answer, against Shields, Base and one more hit) with the engine-correct attackers and
/// Blockers of `board::Board` (W-32).
///
/// # Errors
/// As [`Board::of`].
pub(crate) fn can_kill(st: &GameState, attacker: u8, exact: bool) -> Result<f64, EngineError> {
    let turn = if st.active() == attacker {
        Turn::Now
    } else {
        Turn::Next
    };
    if exact {
        return Ok(if static_lethal(st, attacker, turn)? {
            1.0
        } else {
            0.0
        });
    }
    let board = Board::of(st, attacker, turn)?;
    let level = i64::try_from(st.zone(attacker, Zone::ResourceArea).len()).unwrap_or(i64::MAX);
    let answers = st
        .zone(attacker, Zone::Hand)
        .iter()
        .filter(|&&u| {
            let cd = st.def_of(u);
            cd.card_type == CardType::Command
                && i64::from(cd.level.max(cd.cost)) <= level
                && command_harms_enemy_units(st.registry(), cd.def_id)
        })
        .count();
    let attackers = board.attackers.len();
    let blockers = board.blockers.len().saturating_sub(answers);
    let needed = usize::try_from(board.hits_needed()).unwrap_or(usize::MAX);
    Ok(if attackers.saturating_sub(blockers) >= needed {
        1.0
    } else {
        0.0
    })
}

/// `1 / race` of `attacker` with the strengthened race (`board::race`, W-38).
///
/// # Errors
/// As [`Board::of`].
pub(crate) fn race_feature(st: &GameState, attacker: u8) -> Result<f64, EngineError> {
    let turns = race(st, attacker, RACE_CAP)?.max(1);
    Ok(1.0 / gcg_ai::values::int_f64(turns))
}
