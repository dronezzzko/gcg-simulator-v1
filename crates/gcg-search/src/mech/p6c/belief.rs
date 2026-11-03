//! `belief_worlds{M}` with `belief_last_n{N}` (W-07, PLAN §12.3 S5): belief-weighted
//! determinisation.
//!
//! The reference draws the opponent's hidden cards uniformly from its decklist minus the known
//! cards (`engine/observe.py:56-83`), whatever the opponent did. Here each world is chosen among
//! M candidate worlds, each weighted by the likelihood of the opponent's public decisions under
//! the opponent model, and drawn by systematic resampling.
//!
//! **The decision weighed.** The search sees the information-set state of one decision and no
//! earlier state (agents are called only at their own decisions), so a past decision of the
//! opponent can be weighed only where the current state still shows its options. One can: the
//! end of the opponent's last main phase. During the searcher's turn (from turn 2) and in the
//! opponent's End Phase, the opponent's active Resources are those it kept open when it ended
//! its main phase (they become active only in its own Start Phase), and its hand holds the cards
//! it kept. So the opponent chose `END_MAIN` over playing each card of the candidate hand that
//! those Resources covered. `belief_last_n = N` caps the reconstructed decisions weighed; this
//! is the only one the state shows, so any `N >= 1` weighs it (the default is 1).
//!
//! **The model.** The reference playout policy's MAIN scores (`_main_option`,
//! `ai/policy.py:164-206`, `crates/gcg-ai/src/policy.rs`), evaluated per card from its printed
//! values against the current board: a Unit `4 + 0.5 cost + 0.2 (AP + HP)` (`- 8` with a full
//! Battle Area), a Pilot `4.5 + 0.2 (AP + HP)` when an unpaired Unit can take it, a 【Main】
//! Command `2.5 + 0.3 cost`, a Base `4` over a missing or EX Base (else `-1`), `END_MAIN` 0; a
//! card is castable when the opponent's level and open Resources cover its printed level and
//! cost. The likelihood of `END_MAIN` is its softmax probability at the search's prior
//! temperature (`prior_temperature`, the softmax the search already applies to policy scores):
//! `1 / (1 + Σ exp(score / T))` over the castable cards of the candidate hand.
//!
//! **Candidates and resampling.** Candidate `j` of world `k` is `determinize(st, player, s_j)`
//! (from the source state and with the pools of `opponent_list` and `exact_pools`, `p6c::world`)
//! with `s_0 = W[k]` (the reference's world) and `s_j = derive_seed(W[k], "belief", j)`. The
//! resampling offset is `u_k = frac(u + k (√5 - 1) / 2)`, `u` the first `random()` of
//! `SplitMix64(derive_seed(W[0], "belief-resample"))` (the tree's seed), and world k is the
//! first candidate whose cumulative weight exceeds `u_k` times the total. Only the
//! information-set state, the public Resources and the candidate's own cards enter, so the
//! weights are honest. Every candidate scored counts as one auxiliary apply.

use gcg_core::pymath::py_sum;
use gcg_core::rng::{SeedPart, SplitMix64, derive_seed};
use gcg_data::{CardDef, CardType};
use gcg_effects::Timing;
use gcg_engine::observe::is_hidden_from;
use gcg_engine::{BATTLE_LIMIT, EngineError, GameState, Phase, Zone};

use super::buckets::{command_timing, resources};
use crate::mech::p6o::math::Math;
use crate::mech::{WorldCx, math_of};

/// `(√5 - 1) / 2`: the step of the resampling offsets between worlds.
const STEP: f64 = 0.618_033_988_749_894_9;

/// The opponent's reconstructed decision at one search decision: the end of its last main
/// phase, with the level and open Resources it had and the board it faced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Evidence {
    level: i32,
    open: i32,
    /// The opponent's Battle Area is full (`play_unit`'s `-8`).
    full: bool,
    /// The opponent has a Unit without a Pilot (a Pilot card can be paired).
    unpaired: bool,
    /// The opponent's Base is missing or an EX Base (`my_base_is_weak`).
    weak_base: bool,
}

impl Evidence {
    /// The decision the information-set state `st` of `player` shows, or `None`: outside the
    /// searcher's turn from turn 2 and the opponent's End Phase, with no open Resources, or
    /// with no opponent hand card hidden from `player` (the weights would all be equal).
    pub(crate) fn of(st: &GameState, player: u8) -> Option<Self> {
        let opponent = 1 - player;
        let shown = (st.active() == player && st.turn() >= 2)
            || (st.active() == opponent && st.phase() == Phase::End);
        let (level, open) = resources(st, opponent);
        let hand = st.zone(opponent, Zone::Hand);
        if !shown || open == 0 || !hand.iter().any(|&u| is_hidden_from(st, u, player)) {
            return None;
        }
        let battle = st.zone(opponent, Zone::Battle);
        let base = st.zone(opponent, Zone::Base);
        Some(Self {
            level,
            open,
            full: battle.len() >= BATTLE_LIMIT,
            unpaired: battle.iter().any(|&u| st.card(u).pair < 0),
            weak_base: base
                .first()
                .is_none_or(|&b| st.def_of(b).card_type == CardType::ExBase),
        })
    }

    /// The playout policy's MAIN score of playing `cd` now, or `None` when it cannot be played.
    fn score(&self, st: &GameState, cd: &CardDef) -> Option<f64> {
        if cd.level > self.level || cd.cost > self.open {
            return None;
        }
        let stats = f64::from(cd.ap) + f64::from(cd.hp);
        match cd.card_type {
            CardType::Unit => {
                let full = if self.full { 8.0 } else { 0.0 };
                Some(4.0 + 0.5 * f64::from(cd.cost) + 0.2 * stats - full)
            }
            CardType::Pilot => self.unpaired.then_some(4.5 + 0.2 * stats),
            CardType::Command => matches!(
                command_timing(st.registry(), cd.def_id),
                Some(Timing::Main | Timing::MainOrAction)
            )
            .then_some(2.5 + 0.3 * f64::from(cd.cost)),
            CardType::Base => Some(if self.weak_base { 4.0 } else { -1.0 }),
            _ => None,
        }
    }

    /// The likelihood that the opponent holding its hand in world `w` ended its main phase:
    /// the softmax probability of `END_MAIN` (score 0) at `temperature`.
    pub(crate) fn likelihood(
        &self,
        w: &GameState,
        player: u8,
        temperature: f64,
        math: Math,
    ) -> f64 {
        let opponent = 1 - player;
        let plays: Vec<f64> = w
            .zone(opponent, Zone::Hand)
            .iter()
            .filter_map(|&u| self.score(w, w.def_of(u)))
            .map(|s| math.exp(s / temperature))
            .collect();
        1.0 / (1.0 + py_sum(plays))
    }
}

/// The seed of candidate `j` of the world with seed `world_seed`.
fn candidate_seed(world_seed: u64, j: u32) -> u64 {
    if j == 0 {
        world_seed
    } else {
        derive_seed(
            world_seed,
            &[SeedPart::Str("belief"), SeedPart::Int(i64::from(j))],
        )
    }
}

/// The resampling offset of world `k` in the tree whose first world seed is `w0`.
pub(crate) fn offset(w0: u64, k: usize) -> f64 {
    let u = SplitMix64::new(derive_seed(w0, &[SeedPart::Str("belief-resample")])).random();
    let k = f64::from(u32::try_from(k).unwrap_or(u32::MAX));
    (u + k * STEP).fract()
}

/// The index of the first weight whose cumulative sum (left to right) exceeds `u` times the
/// total (0 when the total is not positive).
pub(crate) fn resample(weights: &[f64], u: f64) -> usize {
    let total: f64 = weights.iter().sum();
    if !(total > 0.0 && total.is_finite()) {
        return 0;
    }
    let target = u * total;
    let mut sum = 0.0;
    for (j, &w) in weights.iter().enumerate() {
        sum += w;
        if sum > target {
            return j;
        }
    }
    weights.len() - 1
}

/// World `k` into `out`: the candidate of `m` that the resampling draws, by the likelihood of
/// `evidence`. `build(seed, out)` determinises one candidate (the reference's
/// `determinize_into(cx.st, cx.player, seed, out)`, or the source and pools of
/// `opponent_list` and `exact_pools`, `p6c::world`). Returns the candidates scored and whether
/// the world differs from the reference's (candidate 0).
///
/// # Errors
/// `build`'s error.
pub(crate) fn world(
    cx: &WorldCx<'_>,
    k: usize,
    m: u32,
    evidence: &Evidence,
    out: &mut GameState,
    build: &dyn Fn(u64, &mut GameState) -> Result<(), EngineError>,
) -> Result<(u64, bool), EngineError> {
    let seed = cx.seeds[k];
    let math = math_of(cx.cfg);
    let mut weights = Vec::with_capacity(m as usize);
    for j in 0..m {
        build(candidate_seed(seed, j), out)?;
        weights.push(evidence.likelihood(out, cx.player, cx.cfg.prior_temperature, math));
    }
    let chosen = resample(&weights, offset(cx.seeds[0], k));
    let chosen_j = u32::try_from(chosen).expect("candidates fit in u32");
    if chosen_j + 1 != m {
        build(candidate_seed(seed, chosen_j), out)?;
    }
    Ok((u64::from(m), chosen != 0))
}
