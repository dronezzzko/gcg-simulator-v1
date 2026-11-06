//! `gcg-eval tune` (docs/dev/PLAN.md §12.2 P6-I, §12.3 S10; W-40): a deterministic,
//! L2-regularised logistic fit of the evaluation weights from recorded positions, with
//! cross-validation by game, the regularisation chosen by cross-validation, a held-out check
//! and an optional validation match.
//!
//! **Positions** (`ai/tuning.py:120-142`, `collect`): self-play games of `--agent` in both
//! seats (mirrored blocks, stratified over the selected pairs; the agent's own weights, so the
//! budget is the target preset's), recording at the first MAIN decision of every turn the
//! active player's features (`features` of the base set or `role_features`, with
//! `burst_density` on the true state); the label is whether that player won, and drawn or
//! unfinished games are dropped. `--positions FILE` fits recorded positions instead (any
//! producer, for example leaf-point positions, W-39) and `--record FILE` writes them: NDJSON,
//! a header `{"format": "gcg-eval-positions", "version": 1, "feature_set", "features"}` then
//! one `{"game", "pair", "y", "x"}` per position.
//!
//! **Fit** (`ai/tuning.py:157-210`): Newton's method on the L2-penalised log-likelihood from
//! zero weights, at most `--steps` steps (default 30), stopping when every step component is
//! below 1e-9; the gradient and Hessian accumulate over the samples in order, dot products are
//! Python's `sum`, and the Newton system is solved by Gaussian elimination with partial
//! pivoting (first maximal pivot), as `_solve`. Unlike Python, the weights are not rounded.
//!
//! **Model selection.** The last `--holdout-pairs` selected pairs (default 1 when at least two
//! are selected) never enter fitting: they are the held-out check. The other games are dealt
//! into `--folds` folds (default 5) by a seeded shuffle of their game ids
//! (`derive_seed(seed, "tune", "folds")`), so no fold splits a game. Each `--lambdas` value
//! (default 0.01, 0.1, 1, 10, 100) is scored by the pooled out-of-fold log-loss; the first
//! smallest wins, and the final weights are fitted on every training position with it. The
//! report gives the held-out log-loss of the agent's starting weights and of the fitted ones.
//!
//! **Validation** (`--validate N`): the agent with the fitted weights against the same agent
//! with its starting weights, N games over the selected pairs in mirrored blocks with the
//! forced play/draw balance (seed `derive_seed(seed, "validate")`), reported as a row
//! (`crate::rows`): wins-only rate with its Wilson interval, the paired Newcombe interval and
//! the discordant counts (Python scored draws as 0.5 and gave no interval).
//!
//! Everything is deterministic for the seed: positions are kept in spec order whatever the
//! worker count, and the fit is serial.

use std::io::Write;
use std::path::PathBuf;

use gcg_ai::{FeatureSet, Weights, burst_density, features, logistic, role_features};
use gcg_core::pool::Cancel;
use gcg_core::pyjson::{DumpOptions, Json, dumps};
use gcg_core::pymath::py_sum;
use gcg_core::{SplitMix64, derive_seed};
use gcg_data::py_path_str;
use gcg_engine::{DecisionKind, GameState};

use crate::AgentDesc;
use crate::agent_spec::parse_agent_spec;
use crate::bench_cmd::CommandError;
use crate::head_to_head::{Selection, stratified_specs};
use crate::rows::{RowAgents, RunOptions, emit, play_row, prepare, progress_printer, row_into};
use crate::selfplay::{GameError, Observer, play_observed, run_indexed};

/// The report format.
pub const TUNE_FORMAT: &str = "gcg-eval-tune";
/// The positions file format.
pub const POSITIONS_FORMAT: &str = "gcg-eval-positions";
pub const FORMAT_VERSION: u32 = 1;
/// `NEWTON_STEPS` (`ai/tuning.py:62`).
pub const NEWTON_STEPS: u32 = 30;
/// The default regularisation grid (it contains Python's `L2 = 1`).
pub const DEFAULT_LAMBDAS: [f64; 5] = [0.01, 0.1, 1.0, 10.0, 100.0];
/// The default fold count.
pub const DEFAULT_FOLDS: u32 = 5;
/// `log_loss`'s probability clip (`ai/tuning.py:150`).
pub const CLIP: f64 = 1e-12;

/// One recorded position: its game, its pair (selection index), the label and the features.
#[derive(Clone, PartialEq, Debug)]
pub struct Sample {
    pub game: u32,
    pub pair: u32,
    pub y: f64,
    pub x: Vec<f64>,
}

/// The parsed arguments.
#[derive(Clone, PartialEq, Debug)]
pub struct TuneArgs {
    pub agent: String,
    pub feature_set: Option<FeatureSet>,
    pub folds: u32,
    pub lambdas: Vec<f64>,
    pub steps: u32,
    pub holdout_pairs: Option<u32>,
    pub validate: u32,
    pub positions: Option<PathBuf>,
    pub record: Option<PathBuf>,
    pub opts: RunOptions,
}

/// `sum(a * b for a, b in zip(w, x))`.
fn dot(w: &[f64], x: &[f64]) -> f64 {
    py_sum(w.iter().zip(x).map(|(a, b)| a * b))
}

/// `log_loss(samples, w)` (`ai/tuning.py:147-152`): the mean clipped log-loss, `0` without
/// samples.
#[must_use]
pub fn log_loss(samples: &[&Sample], w: &[f64]) -> f64 {
    let mut total = 0.0;
    for s in samples {
        let raw = logistic(dot(w, &s.x));
        let lo = if CLIP >= raw { CLIP } else { raw };
        let p = if 1.0 - CLIP <= lo { 1.0 - CLIP } else { lo };
        total -= s.y * gcg_core::libm::ln(p) + (1.0 - s.y) * gcg_core::libm::ln(1.0 - p);
    }
    #[expect(
        clippy::cast_precision_loss,
        reason = "sample counts are far below 2^53"
    )]
    let n = samples.len().max(1) as f64;
    total / n
}

/// `_solve(a, b)` (`ai/tuning.py:202-218`): Gaussian elimination with partial pivoting (the
/// first row of maximal `|m[r][col]|`), then back-substitution with Python's `sum`.
#[must_use]
pub fn solve(a: &[Vec<f64>], b: &[f64]) -> Vec<f64> {
    let n = b.len();
    let mut m: Vec<Vec<f64>> = a
        .iter()
        .zip(b)
        .map(|(row, rhs)| {
            let mut r = row.clone();
            r.push(*rhs);
            r
        })
        .collect();
    for col in 0..n {
        let mut pivot = col;
        for r in col + 1..n {
            if m[r][col].abs() > m[pivot][col].abs() {
                pivot = r;
            }
        }
        m.swap(col, pivot);
        for r in col + 1..n {
            let f = m[r][col] / m[col][col];
            if f != 0.0 {
                let pivot_row = m[col].clone();
                for (v, p) in m[r].iter_mut().zip(&pivot_row).skip(col) {
                    *v -= f * p;
                }
            }
        }
    }
    let mut x = vec![0.0; n];
    for r in (0..n).rev() {
        let s = py_sum((r + 1..n).map(|c| m[r][c] * x[c]));
        x[r] = (m[r][n] - s) / m[r][r];
    }
    x
}

/// `fit_logistic(samples, l2, steps)` (`ai/tuning.py:155-181`) without the final rounding.
#[must_use]
pub fn fit_logistic(samples: &[&Sample], n: usize, l2: f64, steps: u32) -> Vec<f64> {
    let mut w = vec![0.0; n];
    for _ in 0..steps {
        let mut grad: Vec<f64> = w.iter().map(|wi| l2 * wi).collect();
        let mut hess: Vec<Vec<f64>> = (0..n)
            .map(|i| (0..n).map(|j| if i == j { l2 } else { 0.0 }).collect())
            .collect();
        for s in samples {
            let p = logistic(dot(&w, &s.x));
            let r = p - s.y;
            let sw = p * (1.0 - p);
            for i in 0..n {
                let xi = s.x[i];
                if xi == 0.0 {
                    continue;
                }
                grad[i] += r * xi;
                let sxi = sw * xi;
                for (h, xj) in hess[i].iter_mut().zip(&s.x) {
                    *h += sxi * xj;
                }
            }
        }
        let step = solve(&hess, &grad);
        for (wi, si) in w.iter_mut().zip(&step) {
            *wi -= si;
        }
        let largest = step
            .iter()
            .fold(0.0_f64, |m, s| if s.abs() > m { s.abs() } else { m });
        if largest < 1e-9 {
            break;
        }
    }
    w
}

/// The fold of each game id (ascending ids), by a seeded shuffle dealt round-robin.
///
/// # Errors
/// Fewer games than folds, or fewer than two folds.
pub fn folds_of(games: &[u32], k: u32, seed: u64) -> Result<Vec<(u32, u32)>, String> {
    let mut ids: Vec<u32> = games.to_vec();
    ids.sort();
    ids.dedup();
    let k_us = usize::try_from(k).unwrap_or(usize::MAX);
    if k < 2 || ids.len() < k_us {
        return Err(format!(
            "--folds {k} needs at least two folds and one decided game per fold ({} games)",
            ids.len()
        ));
    }
    let mut order = ids.clone();
    SplitMix64::new(derive_seed!(seed, "tune", "folds")).shuffle(&mut order);
    let mut out: Vec<(u32, u32)> = order
        .iter()
        .enumerate()
        .map(|(i, g)| (*g, u32::try_from(i % k_us).unwrap_or(0)))
        .collect();
    out.sort_by_key(|(g, _)| *g);
    Ok(out)
}

/// The cross-validated log-loss of each lambda and the chosen one (first smallest).
///
/// # Errors
/// As [`folds_of`].
pub fn cross_validate(
    train: &[&Sample],
    n: usize,
    lambdas: &[f64],
    k: u32,
    steps: u32,
    seed: u64,
) -> Result<(Vec<(f64, f64)>, f64), String> {
    let games: Vec<u32> = train.iter().map(|s| s.game).collect();
    let folds = folds_of(&games, k, seed)?;
    let fold = |g: u32| {
        folds
            .binary_search_by_key(&g, |(x, _)| *x)
            .map_or(0, |i| folds[i].1)
    };
    let mut scores = Vec::with_capacity(lambdas.len());
    for &l in lambdas {
        let mut total = 0.0;
        for f in 0..k {
            let fit_on: Vec<&Sample> = train
                .iter()
                .copied()
                .filter(|s| fold(s.game) != f)
                .collect();
            let test: Vec<&Sample> = train
                .iter()
                .copied()
                .filter(|s| fold(s.game) == f)
                .collect();
            let w = fit_logistic(&fit_on, n, l, steps);
            #[expect(
                clippy::cast_precision_loss,
                reason = "sample counts are far below 2^53"
            )]
            let m = test.len() as f64;
            total += log_loss(&test, &w) * m;
        }
        #[expect(
            clippy::cast_precision_loss,
            reason = "sample counts are far below 2^53"
        )]
        let pooled = total / (train.len().max(1) as f64);
        scores.push((l, pooled));
    }
    let mut best = 0;
    for (i, s) in scores.iter().enumerate() {
        if s.1 < scores[best].1 {
            best = i;
        }
    }
    let chosen = scores.get(best).map_or(1.0, |s| s.0);
    Ok((scores, chosen))
}

/// The positions observer of `collect` (`ai/tuning.py:128-136`).
struct Positions {
    set: FeatureSet,
    last_turn: i32,
    seen: Vec<(u8, Vec<f64>)>,
}

impl Observer for Positions {
    fn before(&mut self, st: &GameState, _player: u8) -> Result<(), GameError> {
        let Some(dec) = st.pending() else {
            return Ok(());
        };
        if dec.hdr.kind != DecisionKind::Main || st.turn() == self.last_turn {
            return Ok(());
        }
        self.last_turn = st.turn();
        let active = st.active();
        let density = burst_density(st, active);
        let x = match self.set {
            FeatureSet::Roles => role_features(st, density),
            FeatureSet::Base => features(st, active, density),
        }
        .map_err(|e| GameError::engine(&e))?;
        self.seen.push((active, x));
        Ok(())
    }
}

/// The weights an agent plays with.
#[must_use]
pub fn agent_weights(desc: &AgentDesc) -> Option<&Weights> {
    match desc {
        AgentDesc::Random => None,
        AgentDesc::Greedy { weights } | AgentDesc::Reference { weights, .. } => Some(weights),
        AgentDesc::Search { config } => Some(&config.weights),
    }
}

/// `desc` playing with `w`.
#[must_use]
pub fn with_weights(desc: &AgentDesc, w: Weights) -> AgentDesc {
    match desc {
        AgentDesc::Random => AgentDesc::Random,
        AgentDesc::Greedy { .. } => AgentDesc::Greedy { weights: w },
        AgentDesc::Reference { config, .. } => AgentDesc::Reference {
            config: config.clone(),
            weights: w,
        },
        AgentDesc::Search { config } => {
            let mut c = config.clone();
            c.weights = w;
            AgentDesc::Search { config: c }
        }
    }
}

/// Plays the self-play games and returns every decided game's positions, in spec order.
///
/// # Errors
/// A spec problem, a failed game, or the interrupt.
pub fn record_positions(
    agent: &AgentDesc,
    set: FeatureSet,
    selection: &Selection,
    opts: &RunOptions,
    cancel: &Cancel,
    progress: &mut dyn FnMut(usize),
) -> Result<(Vec<Sample>, u32), CommandError> {
    let (specs, counts) = stratified_specs(selection, opts.games, opts.seed, agent, agent)
        .map_err(CommandError::Invalid)?;
    let mut pair_of = Vec::with_capacity(specs.len());
    for (k, &n) in counts.iter().enumerate() {
        pair_of.extend(std::iter::repeat_n(u32::try_from(k).unwrap_or(0), n));
    }
    let per_game = run_indexed(
        specs.len(),
        opts.workers,
        cancel,
        progress,
        |i, cx| {
            let mut obs = Positions {
                set,
                last_turn: 0,
                seen: Vec::new(),
            };
            let r = play_observed(&specs[i], opts.choose_first, cx, &mut obs)?;
            Ok((r.outcome.winner, obs.seen))
        },
        |i| (specs[i].index, specs[i].game_seed),
    )
    .map_err(CommandError::from)?;
    let mut samples = Vec::new();
    let mut dropped = 0;
    for (g, (winner, seen)) in per_game.into_iter().enumerate() {
        if winner != 0 && winner != 1 {
            dropped += 1;
            continue;
        }
        for (player, x) in seen {
            samples.push(Sample {
                game: u32::try_from(g).unwrap_or(u32::MAX),
                pair: pair_of[g],
                y: if i16::from(player) == i16::from(winner) {
                    1.0
                } else {
                    0.0
                },
                x,
            });
        }
    }
    Ok((samples, dropped))
}

fn compact() -> DumpOptions {
    DumpOptions {
        indent: None,
        sort_keys: false,
        compact: true,
        ensure_ascii: true,
    }
}

/// The positions file text (module docs).
#[must_use]
pub fn positions_text(set: FeatureSet, samples: &[Sample]) -> String {
    let mut head = Json::object();
    head.set("format", POSITIONS_FORMAT);
    head.set("version", FORMAT_VERSION);
    head.set("feature_set", set.as_str());
    head.set(
        "features",
        Json::Array(set.names().iter().map(|n| (*n).into()).collect()),
    );
    let mut out = dumps(&head, compact()) + "\n";
    for s in samples {
        let mut o = Json::object();
        o.set("game", s.game);
        o.set("pair", s.pair);
        o.set("y", s.y);
        o.set("x", s.x.clone());
        out += &dumps(&o, compact());
        out.push('\n');
    }
    out
}

/// Reads a positions file.
///
/// # Errors
/// The file is unreadable or malformed.
pub fn read_positions(path: &std::path::Path) -> Result<(FeatureSet, Vec<Sample>), String> {
    use serde_json::Value;
    let shown = py_path_str(path);
    let text = std::fs::read_to_string(path).map_err(|e| format!("cannot read {shown}: {e}"))?;
    let mut lines = text.lines().filter(|l| !l.trim().is_empty());
    let head: Value = lines
        .next()
        .ok_or_else(|| format!("{shown}: empty"))
        .and_then(|l| serde_json::from_str(l).map_err(|e| format!("{shown}: not JSON: {e}")))?;
    if head.get("format").and_then(Value::as_str) != Some(POSITIONS_FORMAT) {
        return Err(format!("{shown}: not a {POSITIONS_FORMAT} file"));
    }
    let set = head
        .get("feature_set")
        .and_then(Value::as_str)
        .and_then(FeatureSet::from_value)
        .ok_or_else(|| format!("{shown}: unknown feature_set"))?;
    let n = set.names().len();
    let mut out = Vec::new();
    for (i, l) in lines.enumerate() {
        let bad =
            || format!("{shown}: position {i} needs game, pair, y in {{0, 1}} and {n} features");
        let v: Value = serde_json::from_str(l).map_err(|_| bad())?;
        let game = v
            .get("game")
            .and_then(Value::as_u64)
            .and_then(|g| u32::try_from(g).ok());
        let pair = v
            .get("pair")
            .and_then(Value::as_u64)
            .and_then(|g| u32::try_from(g).ok());
        let y = v
            .get("y")
            .and_then(Value::as_f64)
            .filter(|y| *y == 0.0 || *y == 1.0);
        let x: Option<Vec<f64>> = v
            .get("x")
            .and_then(Value::as_array)
            .and_then(|xs| xs.iter().map(Value::as_f64).collect());
        match (game, pair, y, x) {
            (Some(game), Some(pair), Some(y), Some(x)) if x.len() == n => {
                out.push(Sample { game, pair, y, x });
            }
            _ => return Err(bad()),
        }
    }
    Ok((set, out))
}

fn weights_json(set: FeatureSet, w: &[f64]) -> Json {
    let mut o = Json::object();
    o.set("feature_set", set.as_str());
    o.set("values", w.to_vec());
    o
}

fn r6(x: f64) -> Json {
    gcg_core::pyfloat::round_f64(x, 6).into()
}

/// Runs `tune`.
///
/// # Errors
/// A setup problem, a failed game, the interrupt, or the output.
#[expect(
    clippy::too_many_lines,
    reason = "one linear pipeline: record, select, fit, check, validate"
)]
pub fn run(
    args: &TuneArgs,
    cancel: &Cancel,
    tty: bool,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<(), CommandError> {
    let invalid = CommandError::Invalid;
    let agent = parse_agent_spec(&args.agent).map_err(invalid)?;
    let start = agent_weights(&agent)
        .cloned()
        .ok_or_else(|| CommandError::Invalid("--agent random has no evaluation weights".into()))?;
    let set = args.feature_set.unwrap_or_else(|| start.feature_set());
    if args.lambdas.is_empty() || args.lambdas.iter().any(|l| !(l.is_finite() && *l > 0.0)) {
        return Err(CommandError::Invalid(
            "--lambdas must be positive numbers".into(),
        ));
    }
    let selection = prepare(&args.opts).map_err(CommandError::Invalid)?;
    let (set, samples, dropped) = if let Some(p) = &args.positions {
        let (fs, s) = read_positions(p).map_err(CommandError::Invalid)?;
        (fs, s, 0)
    } else {
        let total = u64::from(args.opts.games);
        let mut printer = progress_printer(err, tty);
        let mut progress = |d: usize| printer.update(u64::try_from(d).unwrap_or(0), total);
        let (s, d) = record_positions(&agent, set, &selection, &args.opts, cancel, &mut progress)?;
        (set, s, d)
    };
    if let Some(path) = &args.record {
        std::fs::write(path, positions_text(set, &samples)).map_err(|e| {
            CommandError::Invalid(format!("cannot write {}: {e}", py_path_str(path)))
        })?;
    }
    let n_pairs = u32::try_from(selection.pairs.len()).unwrap_or(0);
    let pairs_in_data = samples
        .iter()
        .map(|s| s.pair + 1)
        .max()
        .unwrap_or(0)
        .max(n_pairs);
    let hold = args.holdout_pairs.unwrap_or(u32::from(pairs_in_data >= 2));
    if hold >= pairs_in_data && hold > 0 {
        return Err(CommandError::Invalid(format!(
            "--holdout-pairs {hold} leaves no pair to fit on ({pairs_in_data} pairs)"
        )));
    }
    let first_held = pairs_in_data - hold;
    let train: Vec<&Sample> = samples.iter().filter(|s| s.pair < first_held).collect();
    let held: Vec<&Sample> = samples.iter().filter(|s| s.pair >= first_held).collect();
    let n = set.names().len();
    let (cv, lambda) = cross_validate(
        &train,
        n,
        &args.lambdas,
        args.folds,
        args.steps,
        args.opts.seed,
    )
    .map_err(CommandError::Invalid)?;
    let fitted = fit_logistic(&train, n, lambda, args.steps);
    let start_values = (start.feature_set() == set).then(|| start.values().to_vec());

    let mut doc = Json::object();
    doc.set("format", TUNE_FORMAT);
    doc.set("version", FORMAT_VERSION);
    doc.set("agent", args.agent.as_str());
    doc.set("feature_set", set.as_str());
    doc.set(
        "features",
        Json::Array(set.names().iter().map(|f| (*f).into()).collect()),
    );
    args.opts.describe(&mut doc);
    doc.set(
        "source",
        args.positions
            .as_ref()
            .map_or(Json::from("self-play"), |p| Json::from(py_path_str(p))),
    );
    let mut pos = Json::object();
    pos.set("total", samples.len());
    pos.set("train", train.len());
    pos.set("held_out", held.len());
    pos.set("drawn_games_dropped", dropped);
    doc.set("positions", pos);
    doc.set("folds", args.folds);
    doc.set(
        "cv",
        Json::Array(
            cv.iter()
                .map(|(l, s)| {
                    let mut o = Json::object();
                    o.set("lambda", *l);
                    o.set("logloss", r6(*s));
                    o
                })
                .collect(),
        ),
    );
    doc.set("lambda", lambda);
    doc.set("weights", weights_json(set, &fitted));
    doc.set(
        "start_weights",
        start_values
            .as_ref()
            .map_or(Json::Null, |w| weights_json(set, w)),
    );
    doc.set("train_logloss", r6(log_loss(&train, &fitted)));
    let mut check = Json::object();
    check.set(
        "pairs",
        Json::Array(
            selection
                .pairs
                .iter()
                .skip(usize::try_from(first_held).unwrap_or(usize::MAX))
                .map(|p| {
                    Json::Array(vec![
                        p.decks[0].name.as_str().into(),
                        p.decks[1].name.as_str().into(),
                    ])
                })
                .collect(),
        ),
    );
    check.set("positions", held.len());
    check.set(
        "logloss_start",
        start_values
            .as_ref()
            .filter(|_| !held.is_empty())
            .map_or(Json::Null, |w| r6(log_loss(&held, w))),
    );
    check.set(
        "logloss_fitted",
        if held.is_empty() {
            Json::Null
        } else {
            r6(log_loss(&held, &fitted))
        },
    );
    doc.set("held_out_check", check);

    let validation = if args.validate > 0 {
        let fitted_w =
            Weights::new(fitted.clone(), set).map_err(|e| CommandError::Invalid(e.to_string()))?;
        let agents = RowAgents {
            challenger_text: format!("{} with the fitted weights", args.agent),
            opponent_text: args.agent.clone(),
            challenger: with_weights(&agent, fitted_w),
            opponent: agent.clone(),
        };
        let vopts = RunOptions {
            games: args.validate,
            ..args.opts.clone()
        };
        let seed = derive_seed!(args.opts.seed, "validate");
        let total = u64::from(args.validate);
        let mut printer = progress_printer(err, tty);
        let mut progress = |d: usize| printer.update(u64::try_from(d).unwrap_or(0), total);
        let games = play_row(&agents, &selection, &vopts, seed, cancel, &mut progress)?;
        let mut v = Json::object();
        v.set("games", args.validate);
        v.set("seed", seed);
        row_into(&mut v, &agents, &selection, seed, &games);
        v
    } else {
        Json::Null
    };
    doc.set("validation", validation);
    emit(&doc, args.opts.out.as_ref(), out)?;
    let mut extra = Json::object();
    extra.set("agent", args.agent.as_str());
    extra.set("validate", args.validate);
    args.opts.record("tune", extra)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// W-40: a synthetic dataset drawn from a known logistic model is recovered.
    #[test]
    fn w40_synthetic_weights_are_recovered() {
        let truth = [0.8, -1.2, 0.4];
        let mut rng = SplitMix64::new(7);
        let mut samples = Vec::new();
        for i in 0..6000_u32 {
            let x: Vec<f64> = (0..3)
                .map(|j| {
                    if j == 0 {
                        1.0
                    } else {
                        4.0 * rng.random() - 2.0
                    }
                })
                .collect();
            let p = logistic(dot(&truth, &x));
            let y = if rng.random() < p { 1.0 } else { 0.0 };
            samples.push(Sample {
                game: i / 20,
                pair: 0,
                y,
                x,
            });
        }
        let refs: Vec<&Sample> = samples.iter().collect();
        let w = fit_logistic(&refs, 3, 0.01, NEWTON_STEPS);
        for (got, want) in w.iter().zip(truth) {
            assert!((got - want).abs() < 0.1, "{w:?} vs {truth:?}");
        }
        let (cv, lambda) = cross_validate(&refs, 3, &DEFAULT_LAMBDAS, 5, NEWTON_STEPS, 3)
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(cv.len(), 5);
        assert!(
            lambda <= 10.0,
            "a weak penalty fits a large clean sample: {lambda}"
        );
        assert!(log_loss(&refs, &w) < log_loss(&refs, &[0.0, 0.0, 0.0]));
    }

    /// W-40: folds are dealt by game, so no fold splits a game, and the deal is seeded.
    #[test]
    fn w40_no_fold_splits_a_game() {
        let games: Vec<u32> = (0..40).flat_map(|g| [g, g, g]).collect();
        let folds = folds_of(&games, 5, 11).unwrap_or_default();
        assert_eq!(folds.len(), 40);
        for f in 0..5 {
            assert_eq!(folds.iter().filter(|(_, x)| *x == f).count(), 8);
        }
        assert_eq!(folds_of(&games, 5, 11), Ok(folds.clone()));
        assert_ne!(folds_of(&games, 5, 12), Ok(folds));
        assert!(folds_of(&[1, 2], 5, 1).is_err());
        assert!(folds_of(&games, 1, 1).is_err());
    }

    #[test]
    fn solve_matches_a_known_system() {
        let a = vec![vec![2.0, 1.0], vec![1.0, 3.0]];
        let x = solve(&a, &[3.0, 5.0]);
        assert!((x[0] - 0.8).abs() < 1e-12 && (x[1] - 1.4).abs() < 1e-12);
        let pivot = solve(&[vec![0.0, 1.0], vec![1.0, 0.0]], &[2.0, 3.0]);
        assert_eq!(pivot, [3.0, 2.0]);
    }

    #[test]
    fn positions_round_trip() {
        let samples = vec![
            Sample {
                game: 0,
                pair: 1,
                y: 1.0,
                x: (0..17).map(f64::from).collect(),
            },
            Sample {
                game: 3,
                pair: 0,
                y: 0.0,
                x: vec![0.1; 17],
            },
        ];
        let dir = std::env::temp_dir().join(format!("gcg-eval-positions-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("p.ndjson");
        std::fs::write(&path, positions_text(FeatureSet::Roles, &samples)).unwrap_or_default();
        assert_eq!(read_positions(&path), Ok((FeatureSet::Roles, samples)));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
