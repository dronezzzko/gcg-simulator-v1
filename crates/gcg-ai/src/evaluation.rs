//! Position evaluation: a logistic model over board features (`ai/evaluation.py`;
//! docs/port/inventory/08-ai-policy-eval.md §6).
//!
//! Every `base` feature is a difference `side(player) - side(opponent)`. The `roles` set is
//! computed from the turn player's point of view and keeps the threat terms of the two players
//! apart; the other player's log-odds are the negation. The weight tables are contract data
//! (`crate::{HAND_WEIGHTS, TUNED_WEIGHTS, ROLE_WEIGHTS}`, bit patterns checked against the live
//! Python objects).
//!
//! Exactness: `score` is CPython's Neumaier-compensated `sum()` of the rounded products in
//! feature order (`gcg_core::pymath::py_sum`), `logistic` calls the platform libm `exp`, and
//! the `_race` divisions and `ceil`s follow the Python form.

use gcg_core::libm;
use gcg_core::pymath::py_sum;
use gcg_data::CardType;
use gcg_effects::{Kw, RuleKind};
use gcg_engine::{Derived, EngineError, GameState, Zone, observe, view};

use crate::targeting::command_harms_enemy_units;
use crate::values::{hp_left, int_f64};
use crate::{Density, FeatureSet, N_FEATURES, ROLE_FEATURES, Weights};

/// `BASE_LIFE_HP` (`ai/evaluation.py:57`): Base HP per Shield equivalent.
pub const BASE_LIFE_HP: f64 = 3.0;
/// `DANGER_LIFE` (`ai/evaluation.py:58`).
pub const DANGER_LIFE: f64 = 3.0;
/// `RACE_CAP` (`ai/evaluation.py:59`).
pub const RACE_CAP: i64 = 6;

/// `burst_density(st, observer)` (`ai/evaluation.py:132-140`): per owner (index 0, 1), the
/// fraction of 【Burst】 cards among the main-deck cards `observer` cannot see
/// (`unknown_pools`), or `0.0` for an empty pool. Callers compute it once, on the true state.
#[must_use]
pub fn burst_density(st: &GameState, observer: u8) -> Density {
    let reg = st.registry();
    let mut out = [0.0; 2];
    for owner in 0..2u8 {
        let (pool, _) = observe::unknown_pools(st, observer, owner);
        let bursts = pool.iter().filter(|&&d| reg.card(d).has_burst).count();
        out[usize::from(owner)] = if pool.is_empty() {
            0.0
        } else {
            int_f64(bursts as i64) / int_f64(pool.len() as i64)
        };
    }
    out
}

/// `_Side` (`ai/evaluation.py:143-156`): one player's board summary.
#[derive(Clone, Copy, PartialEq, Debug)]
struct Side {
    shields: i64,
    has_base: bool,
    life: f64,
    units: i64,
    ap: i64,
    hp: i64,
    linked: i64,
    blockers: i64,
    ready: i64,
    hand: [i64; 4],
    level: i64,
    answers: i64,
}

/// `_HAND_SLOT.get(card_type, 3)` (`ai/evaluation.py:159`).
const fn hand_slot(t: CardType) -> usize {
    match t {
        CardType::Unit => 0,
        CardType::Pilot => 1,
        CardType::Command => 2,
        _ => 3,
    }
}

/// `_side(st, dv, x)` (`ai/evaluation.py:162-202`). The hand is read from `st`, which in the
/// searches is a determinised world.
fn side(st: &GameState, dv: &Derived, x: u8) -> Side {
    let shields = st.zone(x, Zone::Shield).len() as i64;
    let base_hp: i64 = st
        .zone(x, Zone::Base)
        .iter()
        .map(|&u| hp_left(st, dv, u))
        .sum();
    let (mut ap, mut hp, mut linked, mut blockers, mut ready) = (0i64, 0i64, 0i64, 0i64, 0i64);
    for &u in st.zone(x, Zone::Battle) {
        let c = st.card(u);
        ap += i64::from(view::ap_of(st, dv, u));
        hp += hp_left(st, dv, u);
        if view::is_linked(dv, u) {
            linked += 1;
        }
        if !c.rested {
            ready += 1;
            if view::has_kw(dv, u, Kw::Blocker) {
                blockers += 1;
            }
        }
    }
    let level = st.zone(x, Zone::ResourceArea).len() as i64;
    let mut hand = [0i64; 4];
    let mut answers = 0i64;
    for &u in st.zone(x, Zone::Hand) {
        let cd = st.def_of(u);
        hand[hand_slot(cd.card_type)] += 1;
        if cd.card_type == CardType::Command
            && i64::from(cd.level.max(cd.cost)) <= level
            && command_harms_enemy_units(st.registry(), cd.def_id)
        {
            answers += 1;
        }
    }
    Side {
        shields,
        has_base: !st.zone(x, Zone::Base).is_empty(),
        life: int_f64(shields) + int_f64(base_hp) / BASE_LIFE_HP,
        units: st.zone(x, Zone::Battle).len() as i64,
        ap,
        hp,
        linked,
        blockers,
        ready,
        hand,
        level,
        answers,
    }
}

/// `_can_kill(attacker, defender, attacker_active)` (`ai/evaluation.py:205-212`): 1 when the
/// attacker's next attack step has enough unblocked attackers to break every Shield and the
/// Base and then hit the player; castable removal in hand answers Blockers.
fn can_kill(attacker: &Side, defender: &Side, attacker_active: bool) -> f64 {
    let attackers = if attacker_active {
        attacker.ready
    } else {
        attacker.units
    };
    let blockers = (defender.blockers - attacker.answers).max(0);
    let hits_needed = defender.shields + i64::from(defender.has_base) + 1;
    if attackers - blockers >= hits_needed {
        1.0
    } else {
        0.0
    }
}

/// `_vector(s, o, active, density)` (`ai/evaluation.py:215-228`): the 14 base terms of one
/// side. `max(0.0, x)` keeps `0.0` unless `x > 0.0`.
fn vector(s: &Side, o: &Side, active: bool, density: f64) -> [f64; N_FEATURES] {
    let danger = DANGER_LIFE - s.life;
    [
        0.0,
        s.life,
        if danger > 0.0 { danger } else { 0.0 },
        can_kill(s, o, active),
        int_f64(s.ap),
        int_f64(s.hp),
        int_f64(s.linked),
        int_f64(s.ready),
        int_f64(s.hand[0]),
        int_f64(s.hand[1]),
        int_f64(s.hand[2]),
        int_f64(s.hand[3]),
        int_f64(s.level),
        density * int_f64(s.shields),
    ]
}

/// `features(st, player, density)` (`ai/evaluation.py:231-241`): the `base` feature vector
/// from `player`'s point of view (antisymmetric in the two players); `tempo` is `+1.0` when
/// `player` is the turn player, else `-1.0`.
///
/// # Errors
/// Any error of the derived view.
pub fn features(st: &GameState, player: u8, density: Density) -> Result<Vec<f64>, EngineError> {
    let dv = view::derived(st)?;
    let me = side(st, &dv, player);
    let opp = side(st, &dv, 1 - player);
    let mine_active = st.active() == player;
    let a = vector(&me, &opp, mine_active, density[usize::from(player)]);
    let b = vector(&opp, &me, !mine_active, density[usize::from(1 - player)]);
    let mut out: Vec<f64> = a.iter().zip(&b).map(|(x, y)| x - y).collect();
    out[0] = if mine_active { 1.0 } else { -1.0 };
    Ok(out)
}

/// `_race(st, dv, attacker, defender)` (`ai/evaluation.py:244-277`): the turns `attacker`
/// needs to win if both boards stayed as they are, capped at [`RACE_CAP`]. Every Unit with
/// AP that may attack the player does so each turn; each of the defender's Blockers that can
/// block (rested ones included) stops one blockable attack, the largest first; the Base
/// absorbs hits at the connecting attacks' mean AP; then every Shield and one more hit.
fn race(st: &GameState, dv: &Derived, attacker: u8, defender: u8) -> i64 {
    let has_rule = |u, kind| view::rules_of(dv, u, kind).next().is_some();
    let blockers = st
        .zone(defender, Zone::Battle)
        .iter()
        .filter(|&&u| view::has_kw(dv, u, Kw::Blocker) && !has_rule(u, RuleKind::CantBlock))
        .count();
    let mut unblockable: Vec<i64> = Vec::new();
    let mut blockable: Vec<i64> = Vec::new();
    for &u in st.zone(attacker, Zone::Battle) {
        let ap = i64::from(view::ap_of(st, dv, u));
        if ap <= 0 || has_rule(u, RuleKind::CantAttack) || has_rule(u, RuleKind::CantAttackPlayer) {
            continue;
        }
        if view::has_kw(dv, u, Kw::HighManeuver) || has_rule(u, RuleKind::CantBeBlocked) {
            unblockable.push(ap);
        } else {
            blockable.push(ap);
        }
    }
    blockable.sort_by(|a, b| b.cmp(a));
    let mut through = unblockable;
    through.extend(blockable.iter().skip(blockers));
    if through.is_empty() {
        return RACE_CAP;
    }
    let base = st.zone(defender, Zone::Base);
    let base_hp: i64 = base.iter().map(|&u| hp_left(st, dv, u)).sum();
    let n = int_f64(through.len() as i64);
    let mean_ap = int_f64(through.iter().sum()) / n;
    let base_hits = if base.is_empty() {
        0
    } else {
        ceil_i64(int_f64(base_hp) / mean_ap)
    };
    let hits = base_hits + st.zone(defender, Zone::Shield).len() as i64 + 1;
    RACE_CAP.min(ceil_i64(int_f64(hits) / n))
}

/// `math.ceil(x)` of a finite float of small magnitude.
#[allow(
    clippy::cast_possible_truncation,
    reason = "the quotients of board counts are small and finite, so the ceiling is exact"
)]
fn ceil_i64(x: f64) -> i64 {
    x.ceil() as i64
}

/// `role_features(st, density)` (`ai/evaluation.py:280-300`): [`ROLE_FEATURES`] from the turn
/// player's point of view: `tempo` `1.0`, the `base` differences without `tempo` and
/// `can_kill`, then `can_kill` of the turn player and of the other player, and `1 / _race` of
/// each.
///
/// # Errors
/// Any error of the derived view.
pub fn role_features(st: &GameState, density: Density) -> Result<Vec<f64>, EngineError> {
    let dv = view::derived(st)?;
    let turn_player = st.active();
    let other = 1 - turn_player;
    let me = side(st, &dv, turn_player);
    let opp = side(st, &dv, other);
    let a = vector(&me, &opp, true, density[usize::from(turn_player)]);
    let b = vector(&opp, &me, false, density[usize::from(other)]);
    let diff: Vec<f64> = a.iter().zip(&b).map(|(x, y)| x - y).collect();
    let mut out = Vec::with_capacity(ROLE_FEATURES.len());
    out.push(1.0);
    out.extend_from_slice(&diff[1..3]);
    out.extend_from_slice(&diff[4..]);
    out.push(a[3]);
    out.push(b[3]);
    out.push(1.0 / int_f64(race(st, &dv, turn_player, other)));
    out.push(1.0 / int_f64(race(st, &dv, other, turn_player)));
    Ok(out)
}

/// `logistic(z)` (`ai/evaluation.py:303-307`), the numerically stable form with the platform
/// libm `exp`; `logistic(-0.0)` takes the `z >= 0` branch.
#[must_use]
pub fn logistic(z: f64) -> f64 {
    if z >= 0.0 {
        return 1.0 / (1.0 + libm::exp(-z));
    }
    let e = libm::exp(z);
    e / (1.0 + e)
}

/// `score(st, player, weights, density)` (`ai/evaluation.py:310-317`): the model's log-odds
/// that `player` wins an unfinished game, a Neumaier `sum()` of `w * x` in feature order. With
/// the `roles` set the turn player's log-odds are negated for the other player.
///
/// # Errors
/// Any error of the derived view.
pub fn score(
    st: &GameState,
    player: u8,
    w: &Weights,
    density: Density,
) -> Result<f64, EngineError> {
    if w.feature_set() == FeatureSet::Roles {
        let r = role_features(st, density)?;
        let z = dot(w.values(), &r);
        return Ok(if player == st.active() { z } else { -z });
    }
    let f = features(st, player, density)?;
    Ok(dot(w.values(), &f))
}

/// `sum(w * x for w, x in zip(values, xs, strict=True))`.
fn dot(values: &[f64], xs: &[f64]) -> f64 {
    debug_assert_eq!(values.len(), xs.len());
    py_sum(values.iter().zip(xs).map(|(w, x)| w * x))
}

/// `evaluate(st, player, weights, density)` (`ai/evaluation.py:320-326`): the estimated
/// probability that `player` wins; exactly 1.0, 0.0 or 0.5 (a draw, winner -1) for a finished
/// game.
///
/// # Errors
/// Any error of the derived view.
pub fn evaluate(
    st: &GameState,
    player: u8,
    w: &Weights,
    density: Density,
) -> Result<f64, EngineError> {
    if let Some(winner) = st.winner() {
        if i32::from(winner) == i32::from(player) {
            return Ok(1.0);
        }
        return Ok(if i32::from(winner) == 1 - i32::from(player) {
            0.0
        } else {
            0.5
        });
    }
    Ok(logistic(score(st, player, w, density)?))
}
