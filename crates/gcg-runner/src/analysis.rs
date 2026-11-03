//! `analysis.json`, written only by `gcg-eval benchmark --analysis` (A30; docs/port/PLAN.md
//! §12.2 P6-M): additive statistics next to the reference reports, which stay byte-identical.
//!
//! - W-90: per-card win rates stratified by game length (Mantel-Haenszel risk difference with
//!   Sato's variance) and, for the card hypotheses of `results.json`, a ready-to-run paired
//!   card-swap test (a variant deck, the benchmark command and `gcg-eval compare-runs`).
//! - W-92: match-clustered intervals of the game-level rates (a cluster-robust interval, a
//!   seeded cluster bootstrap over matches, and the design effect).
//! - W-93: the draw count and draw rate next to every wins-only rate (draws are non-wins, the
//!   one definition of [`wins_only`]).
//! - W-94, W-98: every comparison of the four hypothesis families with its unrounded p-value,
//!   the reference decision (Holm per family on rounded p-values), the decision on unrounded
//!   p-values, a run-level Holm adjustment over all families, and a flag where they differ.
//! - W-95: "never played" copies split into discarded for a cost (or by the owner's own effect),
//!   used by an effect from hand, in hand at game end, and other, each with a Wilson interval.
//! - W-96: a broader "observed" definition for benchmark cards (any reveal, discard or trash
//!   entry), lift in both directions, and draw/play statistics of benchmark cards.
//! - W-98: unrounded means and interpolated (Hyndman-Fan type 7) p10, median and p90.
//! - W-101: the interpretations of `docs/port/audit/interpretations.json` whose cards were used
//!   in the run's games, or whose engine code path ran while a game was re-simulated
//!   (`gcg_engine::diag::interpretation_runs`).
//! - W-109: double defeats and how they were scored, and with `--swiss-draws` the match
//!   results with a double-defeat decider scored as Swiss draws.
//! - W-147: the AI's mulligan rule: opening scores and thresholds of kept and redrawn hands and
//!   win rates by the played hand's opening-score quintile, kept against redrawn.
//!
//! The per-game observations come from re-simulating each recorded game through the engine
//! ([`observe_game`]), never from the agents; the cluster bootstrap draws from a `SplitMix64`
//! seeded with `derive_seed(master, "analysis", "bootstrap", <rate>)`. Everything iterates in
//! sorted order, so the file is a pure function of the run.

use std::collections::{BTreeMap, BTreeSet};

use gcg_core::libm;
use gcg_core::pyjson::Json;
use gcg_core::{SeedPart, SplitMix64, derive_seed};
use gcg_data::{CardDef, Deck};
use gcg_effects::Registry;
use gcg_engine::diag::{Interpretation, interpretation_runs};
use gcg_engine::{
    ActionKind, ApplyError, DecisionKind, EndReason, FrameKind, GameState, HistKind,
    NewGameOptions, Phase, Uid, Zone, apply, canon, new_game,
};
use serde_json::Value;

use crate::config::{own_turn, seat_decks};
use crate::reports::aggregate::{Mulligan, Splits, bench_card_stats, card_stats};
use crate::reports::hypotheses::{ALPHA, MIN_GAMES, family_tests};
use crate::reports::results::ResultsData;
use crate::reports::stats::{Rate, Z95, wilson};
use crate::reports::writer::shlex_quote;
use crate::{BenchmarkConfig, BenchmarkRun, GameRecord, MatchRecord, Outcome, ReplayError};

/// `analysis.json` `format`.
pub const ANALYSIS_FORMAT: &str = "gcg-eval-analysis";
/// `analysis.json` `version`.
pub const ANALYSIS_VERSION: u32 = 1;
/// Cluster-bootstrap replicates per rate (W-92).
pub const BOOTSTRAP_REPLICATES: u32 = 2000;
/// The P6-K interpretation audit table (W-56, W-101).
pub const INTERPRETATIONS: &str = include_str!("../../../docs/port/audit/interpretations.json");

/// The opt-in settings of the analysis.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct AnalysisOptions {
    /// `--swiss-draws` (W-109).
    pub swiss_draws: bool,
    pub bootstrap_replicates: u32,
}

impl Default for AnalysisOptions {
    fn default() -> Self {
        Self {
            swiss_draws: false,
            bootstrap_replicates: BOOTSTRAP_REPLICATES,
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Statistics

#[expect(
    clippy::cast_precision_loss,
    reason = "counts stay far below 2^53, where the conversion is exact"
)]
const fn float(x: u64) -> f64 {
    x as f64
}

fn count(n: usize) -> u64 {
    u64::try_from(n).unwrap_or(u64::MAX)
}

/// The one wins-only rate of the analysis (W-93): `wins / n` with draws as non-wins, as
/// `results.json` computes every rate (`reports/stats.py:12-25`, [`Rate`]), plus the draw
/// count and draw rate shown next to it.
#[must_use]
pub fn wins_only(wins: u64, draws: u64, n: u64) -> Json {
    let rate = Rate::new(wins, n);
    let mut o = rate.to_json();
    o.set("draws", draws);
    o.set("draw_rate", (n > 0).then(|| float(draws) / float(n)));
    o
}

fn wilson_json(successes: u64, n: u64) -> Json {
    let mut o = Json::object();
    o.set("count", successes);
    o.set("n", n);
    o.set("rate", (n > 0).then(|| float(successes) / float(n)));
    o.set(
        "ci95",
        wilson(successes, n, Z95).map(|(lo, hi)| vec![lo, hi]),
    );
    o
}

/// The Hyndman-Fan type 7 quantile of sorted values (linear interpolation between order
/// statistics: `h = (n - 1) q`).
#[must_use]
pub fn quantile7(sorted: &[f64], q: f64) -> Option<f64> {
    let n = sorted.len();
    if n == 0 {
        return None;
    }
    let h = float(count(n - 1)) * q;
    let lo = h.floor();
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "0 <= h <= n - 1"
    )]
    let i = lo as usize;
    let frac = h - lo;
    let a = sorted[i];
    Some(if i + 1 < n {
        a + frac * (sorted[i + 1] - a)
    } else {
        a
    })
}

/// A distribution's unrounded mean and interpolated p10, median and p90 (W-98).
#[must_use]
pub fn interpolated(values: &[i64]) -> Json {
    let mut sorted: Vec<f64> = values
        .iter()
        .map(|&v| {
            #[expect(clippy::cast_precision_loss, reason = "small integers")]
            let f = v as f64;
            f
        })
        .collect();
    sorted.sort_by(f64::total_cmp);
    let mut o = Json::object();
    o.set("n", count(sorted.len()));
    o.set(
        "mean",
        (!sorted.is_empty())
            .then(|| gcg_core::pymath::py_sum(sorted.iter().copied()) / float(count(sorted.len()))),
    );
    o.set("p10", quantile7(&sorted, 0.1));
    o.set("median", quantile7(&sorted, 0.5));
    o.set("p90", quantile7(&sorted, 0.9));
    o
}

/// One stratum of a 2x2 comparison: exposed `(events, n)` and unexposed `(events, n)`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Stratum {
    pub exposed_events: u64,
    pub exposed: u64,
    pub unexposed_events: u64,
    pub unexposed: u64,
}

/// The Mantel-Haenszel risk difference over strata, with Sato's (1989) variance and its 95%
/// interval; `None` when no stratum has both groups.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct MhDifference {
    pub difference: f64,
    pub se: f64,
    pub ci95: (f64, f64),
    pub informative_strata: u64,
}

impl MhDifference {
    #[must_use]
    pub fn to_json(&self) -> Json {
        let mut o = Json::object();
        o.set("difference", self.difference);
        o.set("se", self.se);
        o.set("ci95", vec![self.ci95.0, self.ci95.1]);
        o.set("informative_strata", self.informative_strata);
        o
    }
}

/// `RD_MH = Σ (a n0 - c n1) / N / Σ n1 n0 / N`, `Var = (RD_MH Σ P + Σ Q) / (Σ W)²` with
/// `P = (n1² c - n0² a + n1 n0 (n0 - n1) / 2) / N²` and `Q = (a (n0 - c) + c (n1 - a)) / (2 N)`
/// (Sato, Biometrics 45:1323, 1989); `a`, `c` the exposed and unexposed events.
#[must_use]
pub fn mantel_haenszel(strata: &[Stratum]) -> Option<MhDifference> {
    let (mut num, mut w, mut p, mut q) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for s in strata {
        if s.exposed == 0 || s.unexposed == 0 {
            continue;
        }
        let (a, n1, c, n0) = (
            float(s.exposed_events),
            float(s.exposed),
            float(s.unexposed_events),
            float(s.unexposed),
        );
        let big_n = n1 + n0;
        num.push((a * n0 - c * n1) / big_n);
        w.push(n1 * n0 / big_n);
        p.push((n1 * n1 * c - n0 * n0 * a + n1 * n0 * (n0 - n1) / 2.0) / (big_n * big_n));
        q.push((a * (n0 - c) + c * (n1 - a)) / (2.0 * big_n));
    }
    if w.is_empty() {
        return None;
    }
    let sum = |v: &[f64]| gcg_core::pymath::py_sum(v.iter().copied());
    let sw = sum(&w);
    let rd = sum(&num) / sw;
    let var = (rd * sum(&p) + sum(&q)) / (sw * sw);
    let se = libm::sqrt(if var > 0.0 { var } else { 0.0 });
    Some(MhDifference {
        difference: rd,
        se,
        ci95: (rd - Z95 * se, rd + Z95 * se),
        informative_strata: count(w.len()),
    })
}

/// Holm's step-down adjusted p-values over a family of `m >= p.len()` tests (the missing ones
/// count as `p = 1`): `adj(i) = min(1, max_{j <= i} (m - j + 1) p_(j))` in ascending order,
/// ties in index order.
#[must_use]
pub fn holm_adjusted(p: &[f64], m: usize) -> Vec<f64> {
    let mut order: Vec<usize> = (0..p.len()).collect();
    order.sort_by(|&i, &j| p[i].total_cmp(&p[j]).then(i.cmp(&j)));
    let mut adj = vec![1.0; p.len()];
    let mut running: f64 = 0.0;
    for (rank, &i) in order.iter().enumerate() {
        let factor = float(count(m.saturating_sub(rank)));
        let value = factor * p[i];
        let value = if value > 1.0 { 1.0 } else { value };
        if value > running {
            running = value;
        }
        adj[i] = running;
    }
    adj
}

/// The reference's Holm decisions (`reports/stats.py:63-72`, `_significant`'s padding with
/// `1.0` to the family size `m`) for the first `p.len()` tests.
#[must_use]
pub fn holm_padded(p: &[f64], m: usize) -> Vec<bool> {
    let mut padded = p.to_vec();
    padded.resize(m.max(p.len()), 1.0);
    let mut keep = crate::reports::stats::holm(&padded, ALPHA);
    keep.truncate(p.len());
    keep
}

/// A game-level rate with match clustering (W-92): wins and games per match.
#[derive(Clone, PartialEq, Debug)]
pub struct Clustered {
    pub rate: Option<f64>,
    pub wilson: Option<(f64, f64)>,
    /// `p ± 1.96 sqrt(V_cl)`, `V_cl = C / (C - 1) Σ (y_i - p m_i)² / N²` (the ratio estimator's
    /// cluster-robust variance).
    pub clustered: Option<(f64, f64)>,
    /// Percentile interval of the seeded cluster bootstrap.
    pub bootstrap: Option<(f64, f64)>,
    /// `V_cl / (p (1 - p) / N)`.
    pub design_effect: Option<f64>,
}

/// The clustered statistics of `clusters` (`(wins, games)` per match).
#[must_use]
pub fn clustered_rate(clusters: &[(u64, u64)], replicates: u32, seed: u64) -> Clustered {
    let wins: u64 = clusters.iter().map(|c| c.0).sum();
    let n: u64 = clusters.iter().map(|c| c.1).sum();
    let c = count(clusters.len());
    if n == 0 {
        return Clustered {
            rate: None,
            wilson: None,
            clustered: None,
            bootstrap: None,
            design_effect: None,
        };
    }
    let p = float(wins) / float(n);
    let big_n = float(n);
    let ss = gcg_core::pymath::py_sum(clusters.iter().map(|&(y, m)| {
        let r = float(y) - p * float(m);
        r * r
    }));
    let v_cl = if c > 1 {
        float(c) / float(c - 1) * ss / (big_n * big_n)
    } else {
        0.0
    };
    let se = libm::sqrt(v_cl);
    let v_iid = p * (1.0 - p) / big_n;
    let mut rng = SplitMix64::new(seed);
    let mut reps: Vec<f64> = Vec::with_capacity(usize::try_from(replicates).unwrap_or(0));
    let k = clusters.len();
    for _ in 0..replicates {
        let (mut yw, mut yn) = (0_u64, 0_u64);
        for _ in 0..k {
            let (y, m) = clusters[rng.randrange_usize(k)];
            yw += y;
            yn += m;
        }
        if yn > 0 {
            reps.push(float(yw) / float(yn));
        }
    }
    reps.sort_by(f64::total_cmp);
    Clustered {
        rate: Some(p),
        wilson: wilson(wins, n, Z95),
        clustered: Some((p - Z95 * se, p + Z95 * se)),
        bootstrap: quantile7(&reps, 0.025).zip(quantile7(&reps, 0.975)),
        design_effect: (v_iid > 0.0).then(|| v_cl / v_iid),
    }
}

impl Clustered {
    #[must_use]
    pub fn to_json(&self) -> Json {
        let pair = |x: Option<(f64, f64)>| x.map(|(a, b)| vec![a, b]);
        let mut o = Json::object();
        o.set("rate", self.rate);
        o.set("wilson_ci95", pair(self.wilson));
        o.set("clustered_ci95", pair(self.clustered));
        o.set("bootstrap_ci95", pair(self.bootstrap));
        o.set("design_effect", self.design_effect);
        o
    }
}

// ---------------------------------------------------------------------------------------------
// Per-game observations by re-simulation

/// Why a drawn copy of the deck under test was never played (W-95).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Fate {
    /// A `discard` by its owner while an effect or cost resolved (the engine records costs and
    /// the owner's own discard effects alike).
    DiscardedForCost,
    /// It left the hand for another zone without a play action (deployed, paired, placed or
    /// returned by an effect).
    UsedByEffect,
    /// It was in hand when the game ended.
    InHandAtEnd,
    /// Discarded to the hand limit (rule 7-6-5-1).
    HandLimit,
    /// Discarded by the opponent's effect.
    OpponentDiscard,
    /// Any other route (for example into the trash without a discard).
    Unknown,
}

impl Fate {
    pub const ALL: [Self; 6] = [
        Self::DiscardedForCost,
        Self::UsedByEffect,
        Self::InHandAtEnd,
        Self::HandLimit,
        Self::OpponentDiscard,
        Self::Unknown,
    ];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DiscardedForCost => "discarded_for_cost",
            Self::UsedByEffect => "used_by_effect_from_hand",
            Self::InHandAtEnd => "in_hand_at_end",
            Self::HandLimit => "other_hand_limit",
            Self::OpponentDiscard => "other_opponent_discard",
            Self::Unknown => "other",
        }
    }
}

/// The deck under test's mulligan decision of one game (W-147).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Opening {
    pub redrew: bool,
    pub first: bool,
    /// `opening_score` of the dealt hand (`ai/setup.py:31-52`).
    pub hand_score: f64,
    /// `opening_score` of the hand played: the dealt hand if kept, the new five if redrawn.
    pub played_score: f64,
    /// The AI's redraw threshold (`KEEP_RATIO * average`) from the decision log, when logged.
    pub threshold: Option<f64>,
}

/// Which safety cap ended a `turn_limit` game (W-46): the turn cap or the decision cap.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum CapKind {
    Turns,
    Decisions,
}

impl CapKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Turns => "turns",
            Self::Decisions => "decisions",
        }
    }
}

/// What re-simulating one recorded game shows besides its record.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct GameFacts {
    /// Every never-played drawn copy of the deck under test: `(card number, fate)`, sorted.
    pub never_played: Vec<(Box<str>, Fate)>,
    /// Benchmark card numbers observed in any way: the reference "seen" events and zones plus
    /// any reveal, discard, trash or removal-area entry (W-96).
    pub bench_observed: BTreeSet<Box<str>>,
    /// Benchmark card numbers in the benchmark's hand at some decision or drawn.
    pub bench_drawn: BTreeSet<Box<str>>,
    /// Benchmark plays `(card number, own turn)`, in game order.
    pub bench_played: Vec<(Box<str>, i32)>,
    /// Card numbers (either deck) that were played, deployed, paired, activated, attacked
    /// with or revealed as a Burst (W-101).
    pub used: BTreeSet<Box<str>>,
    /// The interpretation ids whose engine code path ran in the game (W-101,
    /// `gcg_engine::diag::Interpretation`).
    pub interpretations: BTreeSet<&'static str>,
    pub opening: Option<Opening>,
    pub cap: Option<CapKind>,
    pub decisions: u64,
}

const PLAY_KINDS: [ActionKind; 4] = [
    ActionKind::PlayUnit,
    ActionKind::PlayBase,
    ActionKind::PlayCommand,
    ActionKind::Pair,
];
const IN_PLAY_ZONES: [Zone; 4] = [Zone::Battle, Zone::Paired, Zone::Base, Zone::Resolving];
const PUBLIC_EXTRA_ZONES: [Zone; 2] = [Zone::Trash, Zone::Removal];
const USE_EVENTS: [HistKind; 7] = [
    HistKind::Play,
    HistKind::Deployed,
    HistKind::Paired,
    HistKind::BurstRevealed,
    HistKind::CommandActivated,
    HistKind::Activate,
    HistKind::Attack,
];

fn number(st: &GameState, uid: Uid) -> Box<str> {
    st.def_of(uid).card_number.clone()
}

fn hand_set(st: &GameState, p: u8) -> BTreeSet<Uid> {
    st.zone(p, Zone::Hand).iter().copied().collect()
}

fn score_of(st: &GameState, hand: &[Uid], first: bool, reg: &Registry) -> f64 {
    let cards: Vec<&CardDef> = hand.iter().map(|&u| st.def_of(u)).collect();
    gcg_ai::setup::opening_score(&cards, first, reg)
}

fn threshold_of(record: &GameRecord) -> Option<f64> {
    let log = record.decision_log.as_ref()?;
    let entry = log
        .iter()
        .find(|e| e.decision == DecisionKind::Redraw && e.player == record.dut_seat)?;
    entry
        .alternatives
        .iter()
        .find(|a| a.action.kind == ActionKind::Redraw)
        .map(|a| a.value)
}

/// Re-simulates `record` under `cfg` and collects its [`GameFacts`]; the final state must
/// equal the record's (`final_state_sha256`).
///
/// # Errors
/// What re-simulating raises, or a final-state mismatch (`ReplayError::Malformed`).
#[expect(clippy::too_many_lines, reason = "one pass over the game's decisions")]
pub fn observe_game(record: &GameRecord, cfg: &BenchmarkConfig) -> Result<GameFacts, ReplayError> {
    let decks = seat_decks(cfg, record.match_index).map_err(|e| ReplayError::Malformed {
        py_class: "DeckError",
        detail: e.to_string(),
    })?;
    let opts = NewGameOptions {
        chooser: record.chooser,
        turn_limit: record.turn_limit,
        max_actions: record.max_actions,
        validate: true,
    };
    let reg = Registry::embedded();
    let runs_before = interpretation_runs();
    let mut st = new_game(&reg, [&decks[0], &decks[1]], record.seed, opts)
        .map_err(crate::replay::new_game_replay_error)?;
    let (dut, bench) = (record.dut_seat, record.bench_seat());
    let mut facts = GameFacts::default();
    let mut tracking = false;
    let mut drawn: BTreeSet<Uid> = BTreeSet::new();
    let mut played: BTreeSet<Uid> = BTreeSet::new();
    let mut fate: BTreeMap<Uid, Fate> = BTreeMap::new();
    let mut in_hand: BTreeSet<Uid> = BTreeSet::new();
    let mut epoch = st.history_epoch();
    let mut seen_events = 0_usize;
    for (index, &action) in record.actions.iter().enumerate() {
        let Some(dec) = st.pending() else {
            return Err(ReplayError::ExtraActions);
        };
        let (kind, player) = (dec.hdr.kind, dec.hdr.player);
        let system_frame = st
            .frames()
            .last()
            .is_some_and(|f| f.kind == FrameKind::System);
        let hand_limit = kind == DecisionKind::Discard && system_frame && st.phase() == Phase::End;
        let mut dealt: Vec<Uid> = Vec::new();
        if kind == DecisionKind::Redraw && player == dut {
            dealt = st.zone(dut, Zone::Hand).to_vec();
        }
        if PLAY_KINDS.contains(&action.kind)
            && let Ok(uid) = Uid::try_from(action.a)
            && usize::from(uid) < st.cards().len()
        {
            facts.used.insert(number(&st, uid));
            if player == dut {
                played.insert(uid);
            } else if st.card(uid).owner == bench && !st.def_of(uid).is_token() {
                let turn = own_turn(st.turn(), bench, st.first_player());
                facts.bench_played.push((number(&st, uid), turn));
                facts.bench_observed.insert(number(&st, uid));
            }
        }
        apply(&mut st, action, true).map_err(|e| match e {
            ApplyError::Illegal(_) => ReplayError::Illegal { index, action },
            ApplyError::Engine(e) => ReplayError::Engine(e),
        })?;
        if kind == DecisionKind::Redraw && player == dut {
            let first = st.first_player() == dut;
            let kept: Vec<Uid> = st.zone(dut, Zone::Hand).to_vec();
            facts.opening = Some(Opening {
                redrew: action.kind == ActionKind::Redraw,
                first,
                hand_score: score_of(&st, &dealt, first, &reg),
                played_score: score_of(&st, &kept, first, &reg),
                threshold: threshold_of(record),
            });
            tracking = true;
            in_hand = kept.iter().copied().collect();
        }
        if st.history_epoch() != epoch {
            epoch = st.history_epoch();
            seen_events = 0;
        }
        let mut discarded: BTreeMap<Uid, i8> = BTreeMap::new();
        let mut newly_drawn: BTreeSet<Uid> = BTreeSet::new();
        for h in st.history().get(seen_events..).unwrap_or_default() {
            let Ok(uid) = Uid::try_from(h.uid) else {
                continue;
            };
            if usize::from(uid) >= st.cards().len() {
                continue;
            }
            let owner = st.card(uid).owner;
            let token = st.def_of(uid).is_token();
            if USE_EVENTS.contains(&h.kind) && !token {
                facts.used.insert(number(&st, uid));
            }
            if owner == dut && tracking && matches!(h.kind, HistKind::Draw | HistKind::ShieldToHand)
            {
                newly_drawn.insert(uid);
            }
            if h.kind == HistKind::Discard && owner == dut {
                discarded.insert(uid, h.by);
            }
            if owner == bench && !token {
                if matches!(
                    h.kind,
                    HistKind::Play
                        | HistKind::Deployed
                        | HistKind::Paired
                        | HistKind::BurstRevealed
                        | HistKind::CommandActivated
                        | HistKind::Discard
                ) {
                    facts.bench_observed.insert(number(&st, uid));
                }
                if matches!(h.kind, HistKind::Draw | HistKind::ShieldToHand) {
                    facts.bench_drawn.insert(number(&st, uid));
                }
            }
        }
        seen_events = st.history().len();
        for &uid in st.zone(bench, Zone::Hand) {
            if !st.def_of(uid).is_token() {
                facts.bench_drawn.insert(number(&st, uid));
            }
        }
        for zone in IN_PLAY_ZONES.iter().chain(PUBLIC_EXTRA_ZONES.iter()) {
            for &uid in st.zone(bench, *zone) {
                if !st.def_of(uid).is_token() {
                    facts.bench_observed.insert(number(&st, uid));
                }
            }
        }
        if !tracking {
            continue;
        }
        let now = hand_set(&st, dut);
        drawn.extend(newly_drawn.iter().copied());
        drawn.extend(now.iter().copied());
        let left: Vec<Uid> = in_hand
            .union(&newly_drawn)
            .copied()
            .filter(|u| !now.contains(u))
            .collect();
        for uid in left {
            if played.contains(&uid) || fate.contains_key(&uid) {
                continue;
            }
            let f = match discarded.get(&uid) {
                Some(&by) if hand_limit && i16::from(by) == i16::from(dut) => Fate::HandLimit,
                Some(&by) if i16::from(by) == i16::from(dut) => Fate::DiscardedForCost,
                Some(_) => Fate::OpponentDiscard,
                None if st.card(uid).zone == Zone::Trash => Fate::Unknown,
                None => Fate::UsedByEffect,
            };
            fate.insert(uid, f);
        }
        in_hand = now;
    }
    if canon::state_digest(&st) != record.final_state_sha256 {
        return Err(ReplayError::Malformed {
            py_class: "ValueError",
            detail: format!(
                "match {} game {}: the re-simulated final state differs from the record",
                record.match_index,
                record.game_index + 1
            ),
        });
    }
    let end_hand = hand_set(&st, dut);
    let mut never: Vec<(Box<str>, Fate)> = drawn
        .iter()
        .filter(|u| !played.contains(u))
        .map(|&u| {
            let f = if end_hand.contains(&u) {
                Fate::InHandAtEnd
            } else {
                fate.get(&u).copied().unwrap_or(Fate::Unknown)
            };
            (number(&st, u), f)
        })
        .collect();
    never.sort();
    facts.never_played = never;
    let runs_after = interpretation_runs();
    facts.interpretations = Interpretation::ALL
        .iter()
        .zip(runs_before.iter().zip(&runs_after))
        .filter(|(_, (before, after))| after > before)
        .map(|(i, _)| i.id())
        .collect();
    facts.decisions = count(record.actions.len());
    if record.end_reason == EndReason::TurnLimit {
        facts.cap = Some(if st.action_count() > st.max_actions() {
            CapKind::Decisions
        } else {
            CapKind::Turns
        });
    }
    Ok(facts)
}

/// [`observe_game`] for every game of the run, in run order.
///
/// # Errors
/// The first game that does not re-simulate.
pub fn observe_run(run: &BenchmarkRun) -> Result<Vec<GameFacts>, ReplayError> {
    run.games().map(|g| observe_game(g, &run.config)).collect()
}

// ---------------------------------------------------------------------------------------------
// Sections

/// Game-length strata: one per distinct number of turns.
fn length_strata<'a>(games: &[&'a GameRecord]) -> BTreeMap<i32, Vec<&'a GameRecord>> {
    let mut strata: BTreeMap<i32, Vec<&GameRecord>> = BTreeMap::new();
    for g in games {
        strata.entry(g.turns).or_default().push(g);
    }
    strata
}

fn drew(g: &GameRecord, number: &str) -> bool {
    g.dut_cards.drawn.iter().any(|n| &**n == number)
}

/// W-90: per main-deck card of the deck under test, the raw difference "win rate when drawn
/// minus win rate when not drawn" and the Mantel-Haenszel difference within game-length
/// strata (one stratum per number of turns), which removes the length bias of the raw value.
#[must_use]
pub fn length_stratified(games: &[&GameRecord], deck: &Deck) -> Json {
    let strata = length_strata(games);
    let mut out = Vec::new();
    for cc in &*deck.main {
        let number = &*cc.card_number;
        let mut rows = Vec::new();
        let mut total = Stratum::default();
        for gs in strata.values() {
            let mut s = Stratum::default();
            for g in gs {
                let won = u64::from(g.winner == Outcome::Dut);
                if drew(g, number) {
                    s.exposed += 1;
                    s.exposed_events += won;
                } else {
                    s.unexposed += 1;
                    s.unexposed_events += won;
                }
            }
            total.exposed += s.exposed;
            total.exposed_events += s.exposed_events;
            total.unexposed += s.unexposed;
            total.unexposed_events += s.unexposed_events;
            rows.push(s);
        }
        let raw = (total.exposed > 0 && total.unexposed > 0).then(|| {
            float(total.exposed_events) / float(total.exposed)
                - float(total.unexposed_events) / float(total.unexposed)
        });
        let mut o = Json::object();
        o.set("card_number", number);
        o.set("drawn_games", total.exposed);
        o.set("not_drawn_games", total.unexposed);
        o.set("raw_difference", raw);
        o.set(
            "length_stratified",
            mantel_haenszel(&rows).map(|m| m.to_json()),
        );
        out.push(o);
    }
    let mut o = Json::object();
    o.set(
        "note",
        "win rate when drawn minus when not drawn; length_stratified compares games of equal \
         length only (Mantel-Haenszel risk difference, Sato variance), so cards drawn in long \
         games are not credited with the length effect. Still a correlation: test with the \
         paired swaps of swap_tests.",
    );
    o.set("strata", "turns");
    o.set("cards", Json::Array(out));
    o
}

/// W-92: the game-level rates with match clustering.
#[must_use]
pub fn clustered_rates(run: &BenchmarkRun, replicates: u32) -> Json {
    let master = run.config.seed;
    let seed = |name: &str| {
        derive_seed(
            master,
            &[
                SeedPart::Str("analysis"),
                SeedPart::Str("bootstrap"),
                SeedPart::Str(name),
            ],
        )
    };
    let per_match = |filter: &dyn Fn(&GameRecord) -> bool| -> Vec<(u64, u64)> {
        run.matches
            .iter()
            .map(|m| {
                let gs: Vec<&GameRecord> = m.games.iter().filter(|g| filter(g)).collect();
                (
                    count(gs.iter().filter(|g| g.winner == Outcome::Dut).count()),
                    count(gs.len()),
                )
            })
            .filter(|&(_, n)| n > 0)
            .collect()
    };
    let mut o = Json::object();
    for (name, filter) in [
        (
            "games",
            &(|_: &GameRecord| true) as &dyn Fn(&GameRecord) -> bool,
        ),
        ("on_play", &|g: &GameRecord| g.dut_on_play),
        ("on_draw", &|g: &GameRecord| !g.dut_on_play),
    ] {
        o.set(
            name,
            clustered_rate(&per_match(filter), replicates, seed(name)).to_json(),
        );
    }
    let mut out = Json::object();
    out.set(
        "note",
        "games of one match share decks and the loser chooses who goes first, so game-level \
         Wilson intervals overstate precision; clustered_ci95 and bootstrap_ci95 resample \
         whole matches, and design_effect scales the games needed for a given precision",
    );
    out.set("clusters", count(run.matches.len()));
    out.set("rates", o);
    out
}

/// W-93: the outcome blocks with draw counts and draw rates.
#[must_use]
pub fn outcomes(run: &BenchmarkRun) -> Json {
    let block = |winners: &[Outcome]| {
        let n = count(winners.len());
        let wins = count(winners.iter().filter(|w| **w == Outcome::Dut).count());
        let draws = count(winners.iter().filter(|w| **w == Outcome::Draw).count());
        wins_only(wins, draws, n)
    };
    let games: Vec<Outcome> = run.games().map(|g| g.winner).collect();
    let matches: Vec<Outcome> = run.matches.iter().map(|m| m.winner).collect();
    let mut o = Json::object();
    o.set("games", block(&games));
    o.set("matches", block(&matches));
    o.set(
        "note",
        "wins only: draws are non-wins in every rate (results.json's definition); draw_rate \
         is shown beside each rate",
    );
    o
}

/// W-94 and W-98: the hypothesis families with unrounded and run-level decisions.
#[must_use]
pub fn hypothesis_families(games: &[&GameRecord], run: &BenchmarkRun, data: &ResultsData) -> Json {
    let c = &run.config;
    let cards = card_stats(games, &c.deck_under_test, data);
    let bench = bench_card_stats(games, &c.benchmark_deck, MIN_GAMES, data);
    let split = Splits::of(games, c.format.games_per_match());
    let mull = Mulligan::of(games);
    let families = family_tests(&cards, &bench, &split, &mull);
    let m_total: usize = families.iter().map(|f| f.size).sum();
    let all_p: Vec<f64> = families
        .iter()
        .flat_map(|f| f.tests.iter().map(|t| t.p_unrounded))
        .collect();
    let run_adj = holm_adjusted(&all_p, m_total);
    let run_keep = holm_padded(&all_p, m_total);
    let mut k = 0;
    let mut fams = Vec::new();
    let mut any_reference = false;
    let mut any_run_level = false;
    for f in &families {
        let rounded: Vec<f64> = f.tests.iter().map(|t| t.p_rounded).collect();
        let unrounded: Vec<f64> = f.tests.iter().map(|t| t.p_unrounded).collect();
        let ref_keep = holm_padded(&rounded, f.size);
        let unr_keep = holm_padded(&unrounded, f.size);
        let mut tests = Vec::new();
        for (i, t) in f.tests.iter().enumerate() {
            let reference = ref_keep[i];
            let unrounded_ok = unr_keep[i];
            let run_level = run_keep[k];
            k += 1;
            any_reference |= reference;
            any_run_level |= run_level;
            let mut o = Json::object();
            o.set("id", t.id.as_str());
            o.set("delta", t.delta);
            o.set("p_value", t.p_rounded);
            o.set("p_value_unrounded", t.p_unrounded);
            o.set("reference_significant", reference);
            o.set("unrounded_significant", unrounded_ok);
            o.set("unrounded_differs", reference != unrounded_ok);
            o.set("run_level_adjusted_p", run_adj[k - 1]);
            o.set("run_level_significant", run_level);
            tests.push(o);
        }
        let mut o = Json::object();
        o.set("family", f.family);
        o.set("comparisons", f.size);
        o.set("tests", Json::Array(tests));
        fams.push(o);
    }
    let mut o = Json::object();
    o.set("alpha", ALPHA);
    o.set("run_level_comparisons", m_total);
    o.set("families", Json::Array(fams));
    o.set("any_reference_significant", any_reference);
    o.set("any_run_level_significant", any_run_level);
    o.set(
        "note",
        "results.json controls error per family (Holm on p-values rounded to 6 digits); \
         run_level_adjusted_p is Holm over every comparison of all families on unrounded \
         p-values, which bounds the chance of any spurious hypothesis in the run by alpha",
    );
    o
}

/// W-98: the distributions with unrounded means and interpolated quantiles.
#[must_use]
pub fn distributions(games: &[&GameRecord], deck: &Deck) -> Json {
    let turns: Vec<i64> = games.iter().map(|g| i64::from(g.turns)).collect();
    let mut by = Json::object();
    for (label, outcome) in [
        ("win", Outcome::Dut),
        ("loss", Outcome::Bench),
        ("draw", Outcome::Draw),
    ] {
        let v: Vec<i64> = games
            .iter()
            .filter(|g| g.winner == outcome)
            .map(|g| i64::from(g.turns))
            .collect();
        by.set(label, interpolated(&v));
    }
    let mut cards = Vec::new();
    for cc in &*deck.main {
        let v: Vec<i64> = games
            .iter()
            .flat_map(|g| {
                g.dut_cards
                    .played
                    .iter()
                    .filter(|p| *p.card_number == *cc.card_number)
                    .map(|p| i64::from(p.own_turn))
            })
            .collect();
        let mut o = interpolated(&v);
        o.set("card_number", &*cc.card_number);
        cards.push(o);
    }
    let mut o = Json::object();
    o.set("game_length", interpolated(&turns));
    o.set("game_length_by_result", by);
    o.set("card_own_turn", Json::Array(cards));
    o.set(
        "note",
        "results.json keeps nearest-rank integer quantiles and rounded means; these are the \
         unrounded mean and the linearly interpolated quantiles (Hyndman-Fan type 7)",
    );
    o
}

/// W-95: never-played copies by fate.
#[must_use]
pub fn never_played(games: &[&GameRecord], facts: &[GameFacts], deck: &Deck) -> Json {
    let mut out = Vec::new();
    for cc in &*deck.main {
        let number = &*cc.card_number;
        let drawn: u64 = games
            .iter()
            .map(|g| count(g.dut_cards.drawn.iter().filter(|n| &***n == number).count()))
            .sum();
        let reference: u64 = games
            .iter()
            .map(|g| {
                count(
                    g.dut_cards
                        .never_played
                        .iter()
                        .filter(|n| &***n == number)
                        .count(),
                )
            })
            .sum();
        let mut by = Json::object();
        let mut total = 0;
        for fate in Fate::ALL {
            let k: u64 = facts
                .iter()
                .map(|f| {
                    count(
                        f.never_played
                            .iter()
                            .filter(|(n, x)| &**n == number && *x == fate)
                            .count(),
                    )
                })
                .sum();
            total += k;
            by.set(fate.as_str(), wilson_json(k, drawn));
        }
        let mut o = Json::object();
        o.set("card_number", number);
        o.set("drawn_copies", drawn);
        o.set("never_played_copies", reference);
        o.set("classified_copies", total);
        o.set("by_fate", by);
        out.push(o);
    }
    let mut o = Json::object();
    o.set(
        "note",
        "results.json counts every drawn copy without a PLAY_UNIT, PLAY_BASE, PLAY_COMMAND or \
         PAIR action as never played; here each is split by what happened to it (rates over \
         drawn copies, Wilson 95% intervals). discarded_for_cost covers costs and the owner's \
         own discard effects, which the engine records alike",
    );
    o.set("cards", Json::Array(out));
    o
}

/// W-96: benchmark cards with the broader "observed" definition and their draw and play
/// statistics.
#[must_use]
pub fn benchmark_observed(games: &[&GameRecord], facts: &[GameFacts], deck: &Deck) -> Json {
    let mut out = Vec::new();
    for cc in &*deck.main {
        let number = &*cc.card_number;
        let mut obs = (0_u64, 0_u64, 0_u64, 0_u64); // losses observed, observed, losses not, not
        let (mut seen_ref, mut drawn_games, mut played_games, mut plays) = (0, 0, 0, 0);
        let mut drawn_losses = 0;
        let mut turns: Vec<i64> = Vec::new();
        for (g, f) in games.iter().zip(facts) {
            let lost = u64::from(g.winner == Outcome::Bench);
            if f.bench_observed.contains(number) {
                obs.0 += lost;
                obs.1 += 1;
            } else {
                obs.2 += lost;
                obs.3 += 1;
            }
            seen_ref += u64::from(g.bench_seen.iter().any(|n| &**n == number));
            if f.bench_drawn.contains(number) {
                drawn_games += 1;
                drawn_losses += lost;
            }
            let k = f
                .bench_played
                .iter()
                .filter(|(n, _)| &**n == number)
                .count();
            if k > 0 {
                played_games += 1;
            }
            plays += count(k);
            turns.extend(
                f.bench_played
                    .iter()
                    .filter(|(n, _)| &**n == number)
                    .map(|(_, t)| i64::from(*t)),
            );
        }
        let lift = (obs.1 > 0 && obs.3 > 0)
            .then(|| float(obs.0) / float(obs.1) - float(obs.2) / float(obs.3));
        let mut o = Json::object();
        o.set("card_number", number);
        o.set("reference_seen_games", seen_ref);
        o.set("observed_games", obs.1);
        o.set("dut_loss_rate_when_observed", wilson_json(obs.0, obs.1));
        o.set("dut_loss_rate_when_not_observed", wilson_json(obs.2, obs.3));
        o.set("loss_rate_lift", lift);
        o.set(
            "direction",
            match lift {
                Some(l) if l > 0.0 => "positive",
                Some(l) if l < 0.0 => "negative",
                Some(_) => "none",
                None => "n/a",
            },
        );
        o.set("drawn_games", drawn_games);
        o.set(
            "dut_loss_rate_when_drawn",
            wilson_json(drawn_losses, drawn_games),
        );
        o.set("played_games", played_games);
        o.set("plays", plays);
        o.set("play_own_turn", interpolated(&turns));
        out.push(o);
    }
    let mut o = Json::object();
    o.set(
        "note",
        "observed adds any reveal, discard, trash or removal-area entry to results.json's \
         'seen' (played, deployed, paired, Burst-revealed or in a public in-play zone); lift \
         is reported in both directions; drawn and played are the benchmark's own draws and \
         plays",
    );
    o.set("cards", Json::Array(out));
    o
}

/// The interpretation ids with their tagged cards.
fn interpretation_cards() -> Vec<(String, String, Vec<String>)> {
    let doc: Value = serde_json::from_str(INTERPRETATIONS).unwrap_or(Value::Null);
    doc.get("interpretations")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|it| {
                    let id = it.get("id")?.as_str()?.to_owned();
                    let topic = it.get("topic")?.as_str().unwrap_or_default().to_owned();
                    let cards = it
                        .pointer("/tags/card")
                        .and_then(Value::as_array)
                        .map(|c| {
                            c.iter()
                                .filter_map(|x| x.as_str().map(str::to_owned))
                                .collect()
                        })
                        .unwrap_or_default();
                    Some((id, topic, cards))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// W-101: the interpretations whose cards were used in the run's games, or whose engine code
/// path ran in them. `games` counts the games with either; `card_games` the card-level trace
/// and `engine_games` the engine record (`null` for an interpretation without an engine code
/// path).
#[must_use]
pub fn interpretations_used(facts: &[GameFacts]) -> Json {
    let mut used = Vec::new();
    let mut untraceable = Vec::new();
    let anchored: Vec<&'static str> = Interpretation::ALL.iter().map(|i| i.id()).collect();
    for (id, topic, cards) in interpretation_cards() {
        let engine = anchored.contains(&id.as_str());
        if cards.is_empty() && !engine {
            untraceable.push(Json::Str(id));
            continue;
        }
        let by_card = |f: &GameFacts| cards.iter().any(|c| f.used.contains(c.as_str()));
        let by_engine = |f: &GameFacts| f.interpretations.contains(id.as_str());
        let games = count(facts.iter().filter(|f| by_card(f) || by_engine(f)).count());
        if games == 0 {
            continue;
        }
        let card_games = count(facts.iter().filter(|f| by_card(f)).count());
        let engine_games = if engine {
            Json::from(count(facts.iter().filter(|f| by_engine(f)).count()))
        } else {
            Json::Null
        };
        let by: Vec<Json> = cards
            .iter()
            .filter(|c| facts.iter().any(|f| f.used.contains(c.as_str())))
            .map(|c| Json::Str(c.clone()))
            .collect();
        let mut o = Json::object();
        o.set("id", id.as_str());
        o.set("topic", topic.as_str());
        o.set("games", games);
        o.set("cards", Json::Array(by));
        o.set("card_games", card_games);
        o.set("engine_games", engine_games);
        used.push(o);
    }
    let mut o = Json::object();
    o.set("interpretations", Json::Array(used));
    o.set("not_traceable_from_games", Json::Array(untraceable));
    o.set(
        "engine_recorded",
        Json::Array(
            anchored
                .iter()
                .map(|&id| Json::Str(id.to_owned()))
                .collect(),
        ),
    );
    o.set(
        "note",
        "an interpretation of docs/port/audit/interpretations.json is listed when one of its \
         cards was played, deployed, paired, activated, attacked with or revealed as a Burst in \
         a game of this run (card_games: a card-level trace, whose code path may still not have \
         run), or when the engine recorded its code path while the game was re-simulated \
         (engine_games; the interpretations of engine_recorded). Interpretations with neither \
         cards nor an engine code path cannot be traced from the games",
    );
    o
}

/// W-109: double defeats and, with `swiss`, the matches decided by one scored as Swiss draws.
#[must_use]
pub fn double_defeats(run: &BenchmarkRun, swiss: bool) -> Json {
    let games: Vec<&GameRecord> = run.games().collect();
    let both: Vec<&&GameRecord> = games
        .iter()
        .filter(|g| g.end_reason == EndReason::BothDefeated)
        .collect();
    let scored = count(both.iter().filter(|g| g.match_point.is_some()).count());
    let mut o = Json::object();
    o.set("games", count(both.len()));
    o.set("game_result", "draw (a non-win for both)");
    o.set("bo3_scored_for_the_non_turn_player", scored);
    o.set(
        "rule",
        "a 1v1 double defeat is a game draw; in BO3 match scoring the turn player at the end \
         loses it (TRM 5.2), as results.json scores it",
    );
    if swiss {
        let decided_by = |m: &MatchRecord| {
            m.games
                .last()
                .is_some_and(|g| g.end_reason == EndReason::BothDefeated && g.match_point.is_some())
        };
        let mut wins = 0_u64;
        let mut losses = 0_u64;
        let mut draws = 0_u64;
        let mut affected = Vec::new();
        for m in &run.matches {
            if decided_by(m) {
                draws += 1;
                affected.push(Json::Int(i128::from(m.match_index)));
                continue;
            }
            match m.winner {
                Outcome::Dut => wins += 1,
                Outcome::Bench => losses += 1,
                Outcome::Draw => draws += 1,
            }
        }
        let mut s = Json::object();
        s.set(
            "rule",
            "Swiss scoring: a match whose deciding game was a double defeat is a drawn match \
             (1 point each); win 3, draw 1, loss 0",
        );
        s.set("dut_wins", wins);
        s.set("bench_wins", losses);
        s.set("draws", draws);
        s.set("dut_points", 3 * wins + draws);
        s.set("bench_points", 3 * losses + draws);
        s.set(
            "dut_win_rate",
            wins_only(wins, draws, count(run.matches.len())),
        );
        s.set("matches_rescored", Json::Array(affected));
        o.set("swiss_draws", s);
    }
    o
}

/// W-147: the mulligan rule's selection, stratified by the played hand's opening score.
#[must_use]
pub fn mulligan_confound(games: &[&GameRecord], facts: &[GameFacts]) -> Json {
    let rows: Vec<(Opening, bool)> = games
        .iter()
        .zip(facts)
        .filter_map(|(g, f)| f.opening.map(|o| (o, g.winner == Outcome::Dut)))
        .collect();
    let mut scores: Vec<f64> = rows.iter().map(|(o, _)| o.played_score).collect();
    scores.sort_by(f64::total_cmp);
    let cuts: Vec<f64> = [0.2, 0.4, 0.6, 0.8]
        .iter()
        .filter_map(|&q| quantile7(&scores, q))
        .collect();
    let stratum_of = |s: f64| cuts.iter().filter(|&&c| s > c).count();
    let mut strata = vec![Stratum::default(); cuts.len() + 1];
    for (o, won) in &rows {
        let s = &mut strata[stratum_of(o.played_score)];
        let w = u64::from(*won);
        if o.redrew {
            s.exposed += 1;
            s.exposed_events += w;
        } else {
            s.unexposed += 1;
            s.unexposed_events += w;
        }
    }
    let total = strata.iter().fold(Stratum::default(), |mut t, s| {
        t.exposed += s.exposed;
        t.exposed_events += s.exposed_events;
        t.unexposed += s.unexposed;
        t.unexposed_events += s.unexposed_events;
        t
    });
    let raw = (total.exposed > 0 && total.unexposed > 0).then(|| {
        float(total.exposed_events) / float(total.exposed)
            - float(total.unexposed_events) / float(total.unexposed)
    });
    let quintiles: Vec<Json> = strata
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let mut o = Json::object();
            o.set("quintile", count(i) + 1);
            o.set("redrawn", wilson_json(s.exposed_events, s.exposed));
            o.set("kept", wilson_json(s.unexposed_events, s.unexposed));
            o
        })
        .collect();
    let hands: Vec<Json> = games
        .iter()
        .zip(facts)
        .filter_map(|(g, f)| {
            let o = f.opening?;
            let mut h = Json::object();
            h.set("match_index", g.match_index);
            h.set("game_number", u64::from(g.game_index) + 1);
            h.set("redrew", o.redrew);
            h.set("on_play", o.first);
            h.set("hand_score", o.hand_score);
            h.set("threshold", o.threshold);
            h.set("played_hand_score", o.played_score);
            h.set("dut_won", g.winner == Outcome::Dut);
            Some(h)
        })
        .collect();
    let mut o = Json::object();
    o.set(
        "label",
        "a measure of the AI's mulligan rule, not of the deck: the AI redraws only hands whose \
         opening score falls below its threshold (KEEP_RATIO times the average of fresh \
         five-card samples), so kept and redrawn games are selected by the AI",
    );
    o.set("games", count(rows.len()));
    o.set("raw_redraw_minus_keep", raw);
    o.set("quintile_cuts", cuts);
    o.set("quintiles", Json::Array(quintiles));
    o.set(
        "stratified_redraw_minus_keep",
        mantel_haenszel(&strata).map(|m| m.to_json()),
    );
    o.set("hands", Json::Array(hands));
    o
}

/// The shell text of a command.
fn command(args: &[String]) -> String {
    args.iter()
        .map(|a| shlex_quote(a))
        .collect::<Vec<_>>()
        .join(" ")
}

/// W-90: for each card hypothesis of `results.json`, a variant deck that adds or removes one
/// copy (swapped against the card with the opposite signal) and the commands of a paired test.
#[must_use]
pub fn swap_tests(results: &Json, run: &BenchmarkRun) -> (Json, Vec<(String, String)>) {
    let c = &run.config;
    let dut = &c.deck_under_test;
    let deltas: BTreeMap<String, f64> = match results.get("hypotheses") {
        Some(Json::Array(hs)) => hs
            .iter()
            .filter_map(|h| {
                let id = match h.get("id")? {
                    Json::Str(s) => s.strip_prefix("card-drawn-delta:")?.to_owned(),
                    _ => return None,
                };
                let delta = match h.get("evidence")?.get("delta")? {
                    Json::Float(d) => *d,
                    Json::Int(i) => {
                        #[expect(clippy::cast_precision_loss, reason = "a rounded delta")]
                        let d = *i as f64;
                        d
                    }
                    _ => return None,
                };
                Some((id, delta))
            })
            .collect(),
        _ => BTreeMap::new(),
    };
    let copies: BTreeMap<&str, u128> = dut
        .main
        .iter()
        .map(|cc| (&*cc.card_number, cc.count))
        .collect();
    let out_dir = c.out_dir.to_string_lossy().into_owned();
    let mut tests = Vec::new();
    let mut files = Vec::new();
    for (card, delta) in &deltas {
        let (add, remove) = if *delta > 0.0 {
            let partner = deltas
                .iter()
                .filter(|(n, d)| **d < 0.0 && *n != card && copies.get(n.as_str()) > Some(&0))
                .map(|(n, _)| n.clone())
                .next();
            (Some(card.clone()), partner)
        } else {
            let partner = deltas
                .iter()
                .filter(|(n, d)| **d > 0.0 && *n != card && copies.get(n.as_str()) < Some(&4))
                .map(|(n, _)| n.clone())
                .next();
            (partner, Some(card.clone()))
        };
        let mut o = Json::object();
        o.set("hypothesis", format!("card-drawn-delta:{card}"));
        let (Some(add), Some(remove)) = (add, remove) else {
            o.set(
                "skipped",
                "no card with the opposite signal to swap against; edit the deck by hand",
            );
            tests.push(o);
            continue;
        };
        if copies.get(add.as_str()).copied().unwrap_or(0) >= 4 {
            o.set("skipped", "the card to add already has 4 copies");
            tests.push(o);
            continue;
        }
        let mut text = format!(
            "# {} with +1 {add} -1 {remove} (analysis.json swap test)\n",
            dut.name
        );
        for cc in dut.main.iter().chain(dut.resources.iter()) {
            let mut k = cc.count;
            if *cc.card_number == *add {
                k += 1;
            }
            if *cc.card_number == *remove {
                k -= 1;
            }
            if k > 0 {
                text.push_str(&format!("{k} {}\n", cc.card_number));
            }
        }
        if !copies.contains_key(add.as_str()) {
            text.push_str(&format!("1 {add}\n"));
        }
        let rel = format!("variants/swap-{add}-for-{remove}.txt");
        let variant_path = format!("{}/{rel}", out_dir.trim_end_matches('/'));
        let variant_out = format!("{}-swap-{add}-for-{remove}", out_dir.trim_end_matches('/'));
        let bench_deck = c
            .benchmark_deck
            .source
            .as_deref()
            .unwrap_or(&c.benchmark_deck.name)
            .to_owned();
        let mut bench_cmd = vec![
            "gcg-sim".to_owned(),
            "benchmark".to_owned(),
            variant_path.clone(),
            bench_deck,
            "--matches".to_owned(),
            c.matches.to_string(),
            "--seed".to_owned(),
            c.seed.to_string(),
            "--format".to_owned(),
            c.format.as_str().to_owned(),
            "--ai-preset".to_owned(),
            c.ai_preset.to_owned(),
            "--out".to_owned(),
            variant_out.clone(),
        ];
        if c.decision_log {
            bench_cmd.push("--decision-log".to_owned());
        }
        let compare = vec![
            "gcg-eval".to_owned(),
            "compare-runs".to_owned(),
            out_dir.clone(),
            variant_out,
        ];
        o.set("add", add.as_str());
        o.set("remove", remove.as_str());
        o.set("variant_deck", rel.as_str());
        o.set("commands", vec![command(&bench_cmd), command(&compare)]);
        o.set(
            "note",
            "same master seed, so compare-runs pairs the games of both runs deal by deal (W-42)",
        );
        tests.push(o);
        files.push((rel, text));
    }
    (Json::Array(tests), files)
}

/// W-46: the safety-cap kind of every `turn_limit` game (`diagnostics.json`).
#[must_use]
pub fn diagnostics_json(run: &BenchmarkRun, facts: &[GameFacts]) -> Json {
    let mut capped = Vec::new();
    let (mut turns, mut decisions) = (0_u64, 0_u64);
    for (g, f) in run.games().zip(facts) {
        let Some(cap) = f.cap else {
            continue;
        };
        match cap {
            CapKind::Turns => turns += 1,
            CapKind::Decisions => decisions += 1,
        }
        let mut o = Json::object();
        o.set("match_index", g.match_index);
        o.set("game_number", u64::from(g.game_index) + 1);
        o.set("seed", g.seed);
        o.set("turns", g.turns);
        o.set("decisions", f.decisions);
        o.set("cap", cap.as_str());
        capped.push(o);
    }
    let mut o = Json::object();
    o.set("format", "gcg-eval-diagnostics");
    o.set("version", 1);
    o.set("turn_limit", run.config.turn_limit);
    o.set("max_actions", run.config.max_actions);
    o.set("capped_games", count(capped.len()));
    o.set("turn_cap", turns);
    o.set("decision_cap", decisions);
    o.set("games", Json::Array(capped));
    o.set(
        "note",
        "every engine safety cap ends a game as turn_limit in results.json and replays; cap \
         names which one fired (turns: the turn cap, decisions: more than max_actions \
         decisions), read from the re-simulated final state",
    );
    o
}

/// The whole `analysis.json` and the variant decks of the swap tests.
#[must_use]
pub fn analysis_json(
    run: &BenchmarkRun,
    data: &ResultsData,
    results: &Json,
    facts: &[GameFacts],
    opts: AnalysisOptions,
) -> (Json, Vec<(String, String)>) {
    let games: Vec<&GameRecord> = run.games().collect();
    let c = &run.config;
    let (swaps, files) = swap_tests(results, run);
    let mut o = Json::object();
    o.set("format", ANALYSIS_FORMAT);
    o.set("version", ANALYSIS_VERSION);
    o.set(
        "note",
        "additive statistics of gcg-eval benchmark --analysis (docs/port/PLAN.md section 12.2 \
         P6-M); results.json, summary.md and the other reports are unchanged",
    );
    o.set("outcomes", outcomes(run));
    o.set("clustered", clustered_rates(run, opts.bootstrap_replicates));
    o.set(
        "cards_by_length",
        length_stratified(&games, &c.deck_under_test),
    );
    o.set("swap_tests", swaps);
    o.set("hypotheses", hypothesis_families(&games, run, data));
    o.set("distributions", distributions(&games, &c.deck_under_test));
    o.set(
        "never_played",
        never_played(&games, facts, &c.deck_under_test),
    );
    o.set(
        "benchmark_cards",
        benchmark_observed(&games, facts, &c.benchmark_deck),
    );
    o.set("interpretations_used", interpretations_used(facts));
    o.set("double_defeats", double_defeats(run, opts.swiss_draws));
    o.set("mulligan", mulligan_confound(&games, facts));
    let decisions: u64 = facts.iter().map(|f| f.decisions).sum();
    o.set("decisions", decisions);
    (o, files)
}
