//! W-26: the policy-level regret per Damage decision (the "policy-level" half of W-26's
//! verification; the search level is `gcg-eval regret`, docs/port/eval/P6-F/rows/regret-*.json).
//!
//! A *Damage decision* is the decision `kill_aware_targeting` changes: a SELECT outside a mode
//! choice and outside a Rest substitution whose target polarity is harmful
//! (`targeting::choice_polarity`, with the configuration's rule-polarity table) and whose chosen
//! cards take a literal amount of damage (`p6f::polarity::choice_damage`), with at least two
//! options.
//!
//! At such a decision both rollout policies score the options: the reference policy
//! (`gcg_ai::option_scores`, `ai/policy.py:104-108`) and the policy with `kill_aware_targeting`
//! (P6-F's scorer with that flag alone). The *oracle value* of an option is its mean playout
//! value over `worlds` shared worlds determinised from the decider's information set: the
//! option is applied, and the reference policy finishes the game for both seats at the
//! configuration's rollout temperature with one rollout seed per world for every option
//! (common random numbers), at most `MAX_DECISIONS` moves, valued 1, 0 or 0.5, or by
//! `evaluate` with the configuration's weights when capped (the playout and the evaluation at
//! the configuration's math, `play_to_end_with` and `evaluate_with`: the reference's own with
//! `portable_math` off). A policy's *regret* is the best option's oracle value minus the value
//! of its pick: its first-maximum pick at temperature 0, and its expectation under the
//! rollout's sampling (the softmax of the scores at the rollout temperature, which Gumbel-max
//! sampling realises, `ai/policy.py:86-101`).

use gcg_ai::targeting::{self, Polarity};
use gcg_ai::{burst_density, evaluate_with, option_scores, play_to_end_with};
use gcg_core::pymath::py_sum;
use gcg_core::rng::{SeedPart, SplitMix64, derive_seed};
use gcg_engine::observe::determinize;
use gcg_engine::{Action, CtxKey, DecisionKind, EngineError, GameState, apply};

use crate::mech::p6f::calibrated::Model;
use crate::mech::p6f::scorer::Scoring;
use crate::mech::p6f::{DecisionState, polarity};
use crate::mech::p6o::math::Math;
use crate::{MechanismFlags, Observation, StrengthenedConfig};

/// The most moves of one oracle playout before the evaluation scores it.
pub(crate) const MAX_DECISIONS: u32 = 2000;

/// The damage amount of the Damage decision pending in `st`, or `None` for another decision.
pub(crate) fn damage_amount(st: &GameState, cfg: &StrengthenedConfig) -> Option<i64> {
    let dec = st.pending()?;
    if dec.hdr.kind != DecisionKind::Select
        || dec.hdr.ctx(CtxKey::Mode) == 1
        || dec.hdr.ctx(CtxKey::RestSub) >= 0
        || dec.options.len() < 2
    {
        return None;
    }
    if targeting::choice_polarity(st, cfg.policy_rule_polarity_table) != Polarity::Harms {
        return None;
    }
    polarity::choice_damage(st)
}

/// The scores of the pending decision's options: the reference policy's, or with `kill_aware`
/// the policy of `kill_aware_targeting` alone.
pub(crate) fn policy_scores(
    st: &GameState,
    cfg: &StrengthenedConfig,
    kill_aware: bool,
) -> Result<Vec<f64>, EngineError> {
    let dec = st
        .pending()
        .ok_or_else(|| EngineError::Assertion(String::new()))?;
    if !kill_aware {
        return option_scores(st, &dec, dec.options, &cfg.knobs());
    }
    let mut flags = MechanismFlags::NULL;
    flags.kill_aware_targeting = true;
    let state = DecisionState::new();
    Scoring {
        flags: &flags,
        state: &state,
        cfg,
        knobs: cfg.knobs(),
        model: Model::None,
    }
    .scores(st, &dec, dec.options)
}

/// The first maximum of `scores` (the policy at temperature 0, `gcg_ai::pick`).
pub(crate) fn first_max(scores: &[f64]) -> usize {
    let mut best = 0;
    for (i, &s) in scores.iter().enumerate().skip(1) {
        if s > scores[best] {
            best = i;
        }
    }
    best
}

/// The softmax of `scores` at `temperature` (the pick probabilities of Gumbel-max sampling).
pub(crate) fn softmax(scores: &[f64], temperature: f64, math: Math) -> Vec<f64> {
    let top = scores[first_max(scores)];
    let w: Vec<f64> = scores
        .iter()
        .map(|&s| math.exp((s - top) / temperature))
        .collect();
    let total = py_sum(w.iter().copied());
    w.into_iter().map(|x| x / total).collect()
}

/// One Damage decision judged: the options, their oracle values and both policies' picks.
#[derive(Clone, Debug, PartialEq)]
pub struct DamageJudgement {
    /// The damage amount.
    pub amount: i64,
    /// The options, in order.
    pub options: Vec<Action>,
    /// The mean oracle value of each option for the decider.
    pub values: Vec<f64>,
    /// The reference policy's pick at temperature 0 and its pick probabilities.
    pub shared_pick: usize,
    pub shared_probs: Vec<f64>,
    /// The `kill_aware_targeting` policy's pick at temperature 0 and its pick probabilities.
    pub kill_pick: usize,
    pub kill_probs: Vec<f64>,
    /// Per world, the value of the reference pick minus the value of the kill-aware pick.
    pub pick_diffs: Vec<f64>,
}

impl DamageJudgement {
    fn best(&self) -> f64 {
        self.values[first_max(&self.values)]
    }

    /// The regret of a pick.
    #[must_use]
    pub fn regret(&self, pick: usize) -> f64 {
        self.best() - self.values[pick]
    }

    /// The expected regret of a pick distribution.
    #[must_use]
    pub fn expected_regret(&self, probs: &[f64]) -> f64 {
        let best = self.best();
        py_sum(probs.iter().zip(&self.values).map(|(p, v)| p * (best - v)))
    }
}

/// The oracle value of `action` in `world` for `player`.
fn oracle_value(
    world: &GameState,
    action: Action,
    player: u8,
    cfg: &StrengthenedConfig,
    rollout_seed: u64,
) -> Result<f64, EngineError> {
    let math = crate::mech::math_of(cfg).float();
    let mut s = world.clone();
    apply(&mut s, action, false).map_err(|e| EngineError::Value(e.to_string()))?;
    let mut rng = SplitMix64::new(rollout_seed);
    play_to_end_with(
        &mut s,
        Some(&mut rng),
        cfg.rollout_temperature,
        MAX_DECISIONS,
        &cfg.knobs(),
        math,
    )?;
    evaluate_with(&s, player, &cfg.weights, burst_density(&s, player), math)
}

/// The judgement of the Damage decision pending in `st` for `player` (`None` for another
/// decision): `worlds` worlds `determinize(info, player, derive_seed(seed, "world", w))` and
/// rollout seeds `derive_seed(seed, "rollout", w)`.
///
/// # Errors
/// The engine's errors of a world, a policy or a playout.
pub(crate) fn judge(
    st: &GameState,
    player: u8,
    cfg: &StrengthenedConfig,
    worlds: u32,
    seed: u64,
) -> Result<Option<DamageJudgement>, EngineError> {
    let obs = Observation::new(st, player)?;
    let info = obs.info();
    let Some(amount) = damage_amount(info, cfg) else {
        return Ok(None);
    };
    let dec = info
        .pending()
        .ok_or_else(|| EngineError::Assertion(String::new()))?;
    let options: Vec<Action> = dec.options.to_vec();
    let math = crate::mech::math_of(cfg);
    let shared = policy_scores(info, cfg, false)?;
    let kill = policy_scores(info, cfg, true)?;
    let (shared_pick, kill_pick) = (first_max(&shared), first_max(&kill));
    let mut sums = vec![Vec::with_capacity(worlds as usize); options.len()];
    for w in 0..worlds {
        let wi = i64::from(w);
        let world = determinize(
            info,
            player,
            derive_seed(seed, &[SeedPart::Str("world"), SeedPart::Int(wi)]),
        )?;
        let rollout = derive_seed(seed, &[SeedPart::Str("rollout"), SeedPart::Int(wi)]);
        for (i, &o) in options.iter().enumerate() {
            sums[i].push(oracle_value(&world, o, player, cfg, rollout)?);
        }
    }
    let n = f64::from(worlds.max(1));
    let values = sums.iter().map(|v| py_sum(v.iter().copied()) / n).collect();
    let pick_diffs = sums[shared_pick]
        .iter()
        .zip(&sums[kill_pick])
        .map(|(a, b)| a - b)
        .collect();
    Ok(Some(DamageJudgement {
        amount,
        shared_probs: softmax(&shared, cfg.rollout_temperature, math),
        kill_probs: softmax(&kill, cfg.rollout_temperature, math),
        options,
        values,
        shared_pick,
        kill_pick,
        pick_diffs,
    }))
}
