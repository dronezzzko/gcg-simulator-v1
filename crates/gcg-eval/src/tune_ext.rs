//! `gcg-eval tune-leaf` (docs/port/PLAN.md §12.3 S10, package P6-G; W-34, W-35, W-36, W-37,
//! W-39, W-142): records leaf-point positions from self-play of the strengthened search and fits
//! the strengthened evaluation's models, whose weights `gcg-search` embeds
//! (`crates/gcg-search/src/mech/p6g/weights.txt`).
//!
//! **Recording.** Self-play games of `--agent` in both seats (mirrored blocks of four,
//! stratified over the selected pairs, as `tune`). At every [`p6g::LEAF_POINT`] (the first
//! pending decision of a turn, where the search scores its leaves, W-39) the recorder stores
//! the superset features three times through `gcg-search`'s own code path (W-142): on the true
//! state with the turn player's Burst density (`p6g::true_state_features`, the reference
//! tuning's convention), and in one determinised world from each player's view with that
//! player's density in the world (`p6g::determinized_features`, seed
//! `derive_seed(game_seed, "p6g", "leaf", turn, observer)`), exactly as the leaf scores a
//! world of that searcher. The label is whether the turn player won; drawn and unfinished
//! games are dropped. `--record FILE` writes the positions: a JSON header line
//! (`{"format": "gcg-eval-leaf-positions", ...}`) and then fixed-size little-endian records
//! (`u32` game, `u32` pair, `i32` turn, `u32` turn player | turn player's deck in the pair << 8,
//! `f64` label, then the true, observer-0 and observer-1 vectors).
//!
//! **Fit.** `--positions FILE` (repeatable) fits recorded positions instead of playing; each
//! `--model FLAG[+FLAG...]` names one model by its P6-G flags (`ModelSpec::of`). A
//! determinised model fits both observers' samples of a position, a true-state model the
//! true-state sample. Positions before `--min-turn` (default 3, the first turn a leaf of a
//! two-turn horizon reaches) are skipped. The last `--holdout-pairs` pairs (default 1) are the
//! held-out check. The other games are dealt into `--folds` folds by game (`tune::folds_of`, so
//! no fold splits a game; a pair is named by its decks, so a pair recorded in several files is
//! one pair), or with `--fold-by pair` by pair (every game of a pair in one fold,
//! which judges the transfer to other decks, W-36); each `--lambdas` value is scored by the
//! pooled out-of-fold log-loss
//! of the model's own recording mode, the first smallest wins, and the final weights are fitted
//! on every training position with it. The fit is L2-penalised logistic regression by Newton's
//! method from zero weights (at most `--steps` steps, stopping when every step component is
//! below 1e-9), the Hessian solved by Cholesky, with sums in sample order, the portable `exp`
//! and `ln` (W-137), so the weights are bit-identical for any worker count and platform. A
//! phase model (W-35) has one weight vector per turn tercile of the training positions, fitted
//! jointly as a shared vector plus a per-bin deviation, both penalised. With `--signs` every
//! weight's sign is constrained (the turn player's assets help the turn player, the other
//! player's assets and the danger term hurt it, the intercept is free) by an active set, and a
//! phase model's bins are fitted separately, each with its own active set.
//!
//! The report gives, per model, the cross-validation table, the out-of-fold and held-out
//! log-loss of the model and of `ROLE_WEIGHTS` on determinised and on true-state samples side by
//! side (W-142), and the per-deck out-of-fold difference with its game-clustered standard error
//! (W-36); `--deck-transfer` adds the leave-one-deck-out table (each deck's games scored by
//! the model refitted without them). `--test-positions FILE` (repeatable) also scores every fitted model and
//! `ROLE_WEIGHTS` on independent recorded positions (for example another preset's self-play,
//! W-34), with the game-clustered standard error of each model's difference.
//! `--weights-out FILE` writes the table `gcg-search` embeds.

use std::io::Write;
use std::path::{Path, PathBuf};

use gcg_ai::ROLE_WEIGHTS;
use gcg_core::derive_seed;
use gcg_core::pool::{Cancel, Pool};
use gcg_core::portable;
use gcg_core::pyjson::{Json, dumps};
use gcg_data::py_path_str;
use gcg_engine::GameState;
use gcg_runner::argparse::{Action, Converter, Namespace, Parser, PyInt, PyStr, Value};
use gcg_search::p6g::{self, Model, ModelSpec};
use gcg_search::{MechanismFlags, Observation};

use crate::agent_spec::parse_agent_spec;
use crate::bench_cmd::CommandError;
use crate::head_to_head::{Selection, stratified_specs};
use crate::rows::{RunOptions, emit, prepare, progress_printer};
use crate::selfplay::{GameError, Observer, play_observed, run_indexed, threads};
use crate::tune::{CLIP, folds_of};

/// The subcommand's help line.
pub const HELP: &str = "record leaf-point positions and fit the strengthened evaluation (P6-G)";
/// The positions file format.
pub const POSITIONS_FORMAT: &str = "gcg-eval-leaf-positions";
/// The report format.
pub const REPORT_FORMAT: &str = "gcg-eval-tune-leaf";
pub const FORMAT_VERSION: u32 = 1;
/// The default regularisation grid (the penalty is on the summed log-likelihood).
pub const DEFAULT_LAMBDAS: &str = "1,10,100,1000,10000";
/// The default first turn of a fitted position.
pub const DEFAULT_MIN_TURN: i32 = 3;
/// The phase bins of a phase model (W-35: early, mid and late game).
pub const PHASE_BINS: usize = 3;
/// The Newton steps' stopping threshold (`ai/tuning.py:178-180`).
const STEP_TOL: f64 = 1e-9;

/// The parsed arguments.
#[derive(Clone, PartialEq, Debug)]
pub struct TuneLeafArgs {
    pub agent: String,
    pub positions: Vec<PathBuf>,
    pub record: Option<PathBuf>,
    pub models: Vec<String>,
    pub weights_out: Option<PathBuf>,
    pub test_positions: Vec<PathBuf>,
    pub folds: u32,
    pub lambdas: String,
    pub steps: u32,
    pub holdout_pairs: u32,
    pub min_turn: i32,
    pub fold_by_pair: bool,
    pub deck_transfer: bool,
    pub signs: bool,
    pub opts: RunOptions,
}

fn text_of(s: &PyStr) -> String {
    s.to_string_opt()
        .unwrap_or_else(|| String::from_utf8_lossy(&s.to_surrogateescape_bytes()).into_owned())
}

fn ns_text(ns: &Namespace, dest: &str) -> Option<String> {
    match ns.get(dest) {
        Some(Value::Str(s) | Value::Path(s)) => Some(text_of(s)),
        _ => None,
    }
}

fn ns_list(ns: &Namespace, dest: &str) -> Vec<String> {
    match ns.get(dest) {
        Some(Value::List(items)) => items
            .iter()
            .filter_map(|v| match v {
                Value::Str(s) | Value::Path(s) => Some(text_of(s)),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn ns_u64(ns: &Namespace, dest: &str) -> u64 {
    match ns.get(dest) {
        Some(Value::Int(v)) => v.to_u64().unwrap_or(u64::MAX),
        _ => 0,
    }
}

/// The options of `tune-leaf` before the shared run options (`cli::push_run_options`).
#[must_use]
pub fn parser() -> Parser {
    let mut p = Parser::new(
        "gcg-eval tune-leaf",
        Some(
            "Record leaf-point positions from self-play of the strengthened search and fit the \
             P6-G evaluation models (W-34 to W-37, W-39, W-142).",
        ),
    );
    let text = |option, dest, default: Option<&str>, help: &str| {
        Action::option(
            option,
            dest,
            Converter::Str,
            None,
            default.map_or(Value::None, |d| Value::Str(PyStr::from(d))),
            help.to_owned(),
        )
    };
    let int = |option, dest, default: u64, help: &str| {
        Action::option(
            option,
            dest,
            Converter::NonNegativeInt,
            None,
            Value::Int(PyInt::from_u64(default)),
            format!("{help} (default {default})"),
        )
    };
    p.push(text(
        "--agent",
        "agent",
        Some("search:standard"),
        "the self-play agent (default search:standard)",
    ));
    p.push(Action::append(
        "--positions",
        "positions",
        Converter::Path,
        None,
        "fit these recorded positions instead of playing; repeatable".to_owned(),
    ));
    p.push(Action::option(
        "--record",
        "record",
        Converter::Path,
        None,
        Value::None,
        "write the recorded positions here".to_owned(),
    ));
    p.push(Action::append(
        "--model",
        "model",
        Converter::Str,
        None,
        "a model to fit, by its P6-G flags joined with + (for example \
         determinized_fit+keyword_features); repeatable"
            .to_owned(),
    ));
    p.push(Action::option(
        "--weights-out",
        "weights_out",
        Converter::Path,
        None,
        Value::None,
        "write the fitted models as the table gcg-search embeds".to_owned(),
    ));
    p.push(Action::append(
        "--test-positions",
        "test_positions",
        Converter::Path,
        None,
        "also score the fitted models and ROLE_WEIGHTS on these recorded positions; repeatable"
            .to_owned(),
    ));
    p.push(int("--folds", "folds", 5, "cross-validation folds"));
    p.push(Action::option(
        "--fold-by",
        "fold_by",
        Converter::Str,
        Some(vec!["game", "pair"]),
        Value::Str(PyStr::from("game")),
        "deal the cross-validation folds by game or by pair (every game of a pair in one fold; \
         judges transfer to other decks, W-36) (default game)"
            .to_owned(),
    ));
    p.push(Action::flag(
        "--signs",
        "signs",
        "constrain each weight's sign: the turn player's assets help the turn player, the other \
         player's assets and the danger term hurt it, the intercept is free"
            .to_owned(),
    ));
    p.push(Action::flag(
        "--deck-transfer",
        "deck_transfer",
        "also refit each model without the games of each deck, at its chosen lambda, and score \
         those games: the per-deck transfer table of W-36"
            .to_owned(),
    ));
    p.push(text(
        "--lambdas",
        "lambdas",
        Some(DEFAULT_LAMBDAS),
        "comma-separated L2 strengths scored by cross-validation (default 1,10,100,1000,10000)",
    ));
    p.push(int("--steps", "steps", 30, "Newton steps"));
    p.push(int(
        "--holdout-pairs",
        "holdout_pairs",
        1,
        "the last N pairs are only the held-out check",
    ));
    p.push(int(
        "--min-turn",
        "min_turn",
        u64::from(DEFAULT_MIN_TURN.unsigned_abs()),
        "the first turn of a fitted position",
    ));
    p
}

/// The arguments of a parsed namespace, with the shared run options.
#[must_use]
pub fn args(ns: &Namespace, opts: RunOptions) -> TuneLeafArgs {
    let u32_of = |dest| u32::try_from(ns_u64(ns, dest)).unwrap_or(u32::MAX);
    TuneLeafArgs {
        agent: ns_text(ns, "agent").unwrap_or_else(|| "search:standard".to_owned()),
        positions: ns_list(ns, "positions")
            .into_iter()
            .map(PathBuf::from)
            .collect(),
        record: ns_text(ns, "record").map(PathBuf::from),
        models: ns_list(ns, "model"),
        weights_out: ns_text(ns, "weights_out").map(PathBuf::from),
        test_positions: ns_list(ns, "test_positions")
            .into_iter()
            .map(PathBuf::from)
            .collect(),
        folds: u32_of("folds"),
        lambdas: ns_text(ns, "lambdas").unwrap_or_else(|| DEFAULT_LAMBDAS.to_owned()),
        steps: u32_of("steps"),
        holdout_pairs: u32_of("holdout_pairs"),
        min_turn: i32::try_from(ns_u64(ns, "min_turn")).unwrap_or(i32::MAX),
        fold_by_pair: ns_text(ns, "fold_by").as_deref() == Some("pair"),
        deck_transfer: matches!(ns.get("deck_transfer"), Some(Value::Bool(true))),
        signs: matches!(ns.get("signs"), Some(Value::Bool(true))),
        opts,
    }
}

/// One recorded position.
#[derive(Clone, PartialEq, Debug)]
pub struct Position {
    pub game: u32,
    pub pair: u32,
    pub turn: i32,
    pub active: u8,
    /// The turn player's deck in its pair (0 or 1).
    pub tp_deck: u8,
    /// 1 when the turn player won.
    pub y: f64,
    /// The true-state vector, then the determinised vectors of observers 0 and 1.
    pub x: [Vec<f64>; 3],
}

/// A positions file: its pairs (deck names) and positions.
#[derive(Clone, PartialEq, Debug)]
pub struct PositionSet {
    pub pairs: Vec<[String; 2]>,
    pub header: Json,
    pub positions: Vec<Position>,
}

/// The recorder of one game (module docs).
struct Recorder {
    seed: u64,
    previous: Option<i32>,
    seen: Vec<(i32, u8, [Vec<f64>; 3])>,
}

impl Observer for Recorder {
    fn before(&mut self, st: &GameState, _player: u8) -> Result<(), GameError> {
        let previous = self.previous.replace(st.turn());
        if st.turn() < 1 || !p6g::LEAF_POINT.holds(previous, st) {
            return Ok(());
        }
        let engine = |e: gcg_engine::EngineError| GameError::engine(&e);
        let truth = p6g::true_state_features(st).map_err(engine)?;
        let mut det: [Vec<f64>; 2] = [Vec::new(), Vec::new()];
        for (o, slot) in (0u8..2).zip(det.iter_mut()) {
            let obs = Observation::new(st, o).map_err(engine)?;
            let seed = derive_seed!(self.seed, "p6g", "leaf", st.turn(), u32::from(o));
            *slot = p6g::determinized_features(&obs, seed).map_err(engine)?;
        }
        let [d0, d1] = det;
        self.seen.push((st.turn(), st.active(), [truth, d0, d1]));
        Ok(())
    }
}

/// Plays the self-play games and returns the decided games' positions, in spec order, and the
/// number of drawn or unfinished games dropped.
///
/// # Errors
/// A spec problem, a failed game, or the interrupt.
pub fn record_positions(
    agent_text: &str,
    selection: &Selection,
    opts: &RunOptions,
    cancel: &Cancel,
    progress: &mut dyn FnMut(usize),
) -> Result<(Vec<Position>, u32), CommandError> {
    let agent = parse_agent_spec(agent_text).map_err(CommandError::Invalid)?;
    let (specs, counts) = stratified_specs(selection, opts.games, opts.seed, &agent, &agent)
        .map_err(CommandError::Invalid)?;
    let mut pair_of = Vec::with_capacity(specs.len());
    for (k, &n) in counts.iter().enumerate() {
        pair_of.extend(std::iter::repeat_n(k, n));
    }
    let per_game = run_indexed(
        specs.len(),
        opts.workers,
        cancel,
        progress,
        |i, cx| {
            let mut rec = Recorder {
                seed: specs[i].game_seed,
                previous: Some(0),
                seen: Vec::new(),
            };
            let r = play_observed(&specs[i], opts.choose_first, cx, &mut rec)?;
            Ok((r.outcome.winner, rec.seen))
        },
        |i| (specs[i].index, specs[i].game_seed),
    )
    .map_err(CommandError::from)?;
    let mut out = Vec::new();
    let mut dropped = 0;
    for (g, (winner, seen)) in per_game.into_iter().enumerate() {
        if winner != 0 && winner != 1 {
            dropped += 1;
            continue;
        }
        let pair = &selection.pairs[pair_of[g]];
        for (turn, active, x) in seen {
            let tp_deck = u8::from(specs[g].decks[usize::from(active)] != pair.decks[0].deck);
            out.push(Position {
                game: u32::try_from(g).unwrap_or(u32::MAX),
                pair: u32::try_from(pair_of[g]).unwrap_or(u32::MAX),
                turn,
                active,
                tp_deck,
                y: if i16::from(active) == i16::from(winner) {
                    1.0
                } else {
                    0.0
                },
                x,
            });
        }
    }
    Ok((out, dropped))
}

fn compact_json(doc: &Json) -> String {
    dumps(
        doc,
        gcg_core::pyjson::DumpOptions {
            indent: None,
            sort_keys: false,
            compact: true,
            ensure_ascii: true,
        },
    )
}

/// The bytes of a positions file.
#[must_use]
pub fn positions_bytes(header: &Json, positions: &[Position]) -> Vec<u8> {
    let mut out = compact_json(header).into_bytes();
    out.push(b'\n');
    for p in positions {
        out.extend_from_slice(&p.game.to_le_bytes());
        out.extend_from_slice(&p.pair.to_le_bytes());
        out.extend_from_slice(&p.turn.to_le_bytes());
        let flags = u32::from(p.active) | (u32::from(p.tp_deck) << 8);
        out.extend_from_slice(&flags.to_le_bytes());
        out.extend_from_slice(&p.y.to_le_bytes());
        for v in &p.x {
            for x in v {
                out.extend_from_slice(&x.to_le_bytes());
            }
        }
    }
    out
}

fn le_u32(b: &[u8]) -> u32 {
    let mut a = [0u8; 4];
    a.copy_from_slice(&b[..4]);
    u32::from_le_bytes(a)
}

fn le_f64(b: &[u8]) -> f64 {
    let mut a = [0u8; 8];
    a.copy_from_slice(&b[..8]);
    f64::from_le_bytes(a)
}

/// Reads a positions file.
///
/// # Errors
/// The file is unreadable, malformed, or recorded with other features.
pub fn read_positions(path: &Path) -> Result<PositionSet, String> {
    use serde_json::Value as J;
    let shown = py_path_str(path);
    let bytes = std::fs::read(path).map_err(|e| format!("cannot read {shown}: {e}"))?;
    let nl = bytes
        .iter()
        .position(|&b| b == b'\n')
        .ok_or_else(|| format!("{shown}: no header"))?;
    let head: J = serde_json::from_slice(&bytes[..nl]).map_err(|e| format!("{shown}: {e}"))?;
    if head.get("format").and_then(J::as_str) != Some(POSITIONS_FORMAT) {
        return Err(format!("{shown}: not a {POSITIONS_FORMAT} file"));
    }
    let names: Vec<&str> = head
        .get("features")
        .and_then(J::as_array)
        .map(|a| a.iter().filter_map(J::as_str).collect())
        .unwrap_or_default();
    if names != p6g::NAMES {
        return Err(format!(
            "{shown}: recorded with other features than this build's P6-G superset"
        ));
    }
    let pairs: Vec<[String; 2]> = head
        .get("pairs")
        .and_then(J::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|p| {
                    let p = p.as_array()?;
                    Some([
                        p.first()?.as_str()?.to_owned(),
                        p.get(1)?.as_str()?.to_owned(),
                    ])
                })
                .collect()
        })
        .unwrap_or_default();
    let n = p6g::N_FEATURES;
    let size = 24 + 3 * 8 * n;
    let body = &bytes[nl + 1..];
    if body.len() % size != 0 {
        return Err(format!("{shown}: truncated positions"));
    }
    let mut positions = Vec::with_capacity(body.len() / size);
    for rec in body.chunks_exact(size) {
        let flags = le_u32(&rec[12..]);
        let vec_at = |k: usize| -> Vec<f64> {
            (0..n)
                .map(|i| le_f64(&rec[24 + 8 * (k * n + i)..]))
                .collect()
        };
        let pair = le_u32(&rec[4..]);
        if usize::try_from(pair).map_or(true, |p| p >= pairs.len()) {
            return Err(format!("{shown}: a position names an unknown pair"));
        }
        positions.push(Position {
            game: le_u32(rec),
            pair,
            turn: i32::from_le_bytes([rec[8], rec[9], rec[10], rec[11]]),
            active: u8::try_from(flags & 0xff).unwrap_or(0),
            tp_deck: u8::try_from((flags >> 8) & 0xff).unwrap_or(0),
            y: le_f64(&rec[16..]),
            x: [vec_at(0), vec_at(1), vec_at(2)],
        });
    }
    let header = to_json(&head);
    Ok(PositionSet {
        pairs,
        header,
        positions,
    })
}

/// A JSON value as a report value, without the (long) feature list of a positions header.
fn to_json(v: &serde_json::Value) -> Json {
    use serde_json::Value as J;
    match v {
        J::Null => Json::Null,
        J::Bool(b) => Json::Bool(*b),
        J::Number(n) => n
            .as_i64()
            .map(Json::from)
            .or_else(|| n.as_u64().map(Json::from))
            .unwrap_or_else(|| Json::Float(n.as_f64().unwrap_or(0.0))),
        J::String(s) => Json::from(s.as_str()),
        J::Array(a) => Json::Array(a.iter().map(to_json).collect()),
        J::Object(m) => {
            let mut o = Json::object();
            for (k, x) in m {
                if k != "features" {
                    o.set(k.as_str(), to_json(x));
                }
            }
            o
        }
    }
}

/// `header` without its feature list.
fn without_features(header: &Json) -> Json {
    match header {
        Json::Object(items) => Json::Object(
            items
                .iter()
                .filter(|(k, _)| *k != gcg_core::pyjson::Key::from("features"))
                .cloned()
                .collect(),
        ),
        other => other.clone(),
    }
}

/// The model a `--model` text names: P6-G flag names joined with `+`.
///
/// # Errors
/// An unknown flag or a text that names no model.
pub fn model_of(text: &str) -> Result<ModelSpec, String> {
    let mut flags = MechanismFlags::NULL;
    for name in text.split('+').map(str::trim) {
        let pkg = MechanismFlags::FIELDS
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, p)| *p);
        if pkg != Some("P6-G") || name == "leaf_density" {
            return Err(format!(
                "--model {text:?}: {name:?} is not a P6-G model flag"
            ));
        }
        flags
            .set_text(name, "true")
            .unwrap_or_else(|| Err(format!("--model: unknown flag {name}")))?;
    }
    ModelSpec::of(&flags).ok_or_else(|| format!("--model {text:?} names no model"))
}

/// One sample of a design: its game, bin, label and the model's columns.
struct Design {
    d: usize,
    x: Vec<f64>,
    y: Vec<f64>,
    bin: Vec<u8>,
    game: Vec<u32>,
    pair: Vec<u32>,
    deck: Vec<[u16; 2]>,
}

impl Design {
    fn len(&self) -> usize {
        self.y.len()
    }

    fn row(&self, i: usize) -> &[f64] {
        &self.x[i * self.d..(i + 1) * self.d]
    }
}

/// The recording modes of a sample (true-state, determinised).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mode {
    True,
    Det,
}

impl Mode {
    const fn as_str(self) -> &'static str {
        match self {
            Self::True => "true-state",
            Self::Det => "determinized",
        }
    }
}

/// The samples of `positions` in `mode` with `columns`, binned by `edges`; `deck_ids` maps a
/// position's pair to its two deck ids.
fn design(
    positions: &[&Position],
    mode: Mode,
    columns: &[usize],
    edges: &[i32],
    deck_ids: &[[u16; 2]],
) -> Design {
    let d = columns.len();
    let mut out = Design {
        d,
        x: Vec::new(),
        y: Vec::new(),
        bin: Vec::new(),
        game: Vec::new(),
        pair: Vec::new(),
        deck: Vec::new(),
    };
    let views: &[usize] = match mode {
        Mode::True => &[0],
        Mode::Det => &[1, 2],
    };
    for p in positions {
        let bin = u8::try_from(edges.iter().filter(|&&e| e <= p.turn).count()).unwrap_or(0);
        let pair = usize::try_from(p.pair).unwrap_or(0);
        for &v in views {
            out.x.extend(columns.iter().map(|&c| p.x[v][c]));
            out.y.push(p.y);
            out.bin.push(bin);
            out.game.push(p.game);
            out.pair.push(p.pair);
            out.deck.push(deck_ids[pair]);
        }
    }
    out
}

/// The logistic with the portable `exp` (W-137), in `gcg_ai::logistic`'s stable form.
fn logistic(z: f64) -> f64 {
    if z >= 0.0 {
        return 1.0 / (1.0 + portable::exp(-z));
    }
    let e = portable::exp(z);
    e / (1.0 + e)
}

/// The clipped log-loss of one prediction (`ai/tuning.py:147-152`) with the portable `ln`.
fn loss(z: f64, y: f64) -> f64 {
    let raw = logistic(z);
    let lo = if CLIP >= raw { CLIP } else { raw };
    let p = if 1.0 - CLIP <= lo { 1.0 - CLIP } else { lo };
    -(y * portable::log(p) + (1.0 - y) * portable::log(1.0 - p))
}

fn dot(w: &[f64], x: &[f64]) -> f64 {
    let mut z = 0.0;
    for (a, b) in w.iter().zip(x) {
        z += a * b;
    }
    z
}

/// The weight vector of each bin from the parameters: the shared vector plus the bin's
/// deviation for a phase model, the parameters themselves otherwise.
fn bin_weights(theta: &[f64], d: usize, bins: usize) -> Vec<Vec<f64>> {
    if bins == 1 {
        return vec![theta.to_vec()];
    }
    (0..bins)
        .map(|b| (0..d).map(|i| theta[i] + theta[d * (b + 1) + i]).collect())
        .collect()
}

/// Cholesky solve of the symmetric positive definite `a x = b` (`a` full, row-major `n × n`).
fn cholesky_solve(a: &[f64], b: &[f64], n: usize) -> Result<Vec<f64>, String> {
    let mut l = vec![0.0; n * n];
    for i in 0..n {
        for j in 0..=i {
            let mut s = a[i * n + j];
            for k in 0..j {
                s -= l[i * n + k] * l[j * n + k];
            }
            if i == j {
                if s <= 0.0 || !s.is_finite() {
                    return Err("the Newton system is not positive definite".to_owned());
                }
                l[i * n + i] = s.sqrt();
            } else {
                l[i * n + j] = s / l[j * n + j];
            }
        }
    }
    let mut y = vec![0.0; n];
    for i in 0..n {
        let mut s = b[i];
        for k in 0..i {
            s -= l[i * n + k] * y[k];
        }
        y[i] = s / l[i * n + i];
    }
    let mut x = vec![0.0; n];
    for i in (0..n).rev() {
        let mut s = y[i];
        for k in i + 1..n {
            s -= l[k * n + i] * x[k];
        }
        x[i] = s / l[i * n + i];
    }
    Ok(x)
}

/// The L2-penalised logistic fit of the rows `rows` of `ds` (module docs) by Newton's method
/// from zero, with the parameters marked in `fixed` held at 0 (their gradient and Hessian rows
/// are replaced by the identity's); returns the parameters.
fn newton(
    ds: &Design,
    rows: &[usize],
    bins: usize,
    lambda: f64,
    steps: u32,
    fixed: &[bool],
) -> Result<Vec<f64>, String> {
    let d = ds.d;
    let n = if bins == 1 { d } else { d * (bins + 1) };
    let mut theta = vec![0.0; n];
    for _ in 0..steps {
        let w = bin_weights(&theta, d, bins);
        let mut g_b = vec![vec![0.0; d]; bins];
        let mut h_b = vec![vec![0.0; d * d]; bins];
        for &i in rows {
            let b = usize::from(ds.bin[i]).min(bins - 1);
            let x = ds.row(i);
            let p = logistic(dot(&w[b], x));
            let r = p - ds.y[i];
            let s = p * (1.0 - p);
            let (g, h) = (&mut g_b[b], &mut h_b[b]);
            for (a, &xa) in x.iter().enumerate() {
                if xa == 0.0 {
                    continue;
                }
                g[a] += r * xa;
                let sx = s * xa;
                for c in a..d {
                    h[a * d + c] += sx * x[c];
                }
            }
        }
        let mut grad: Vec<f64> = theta.iter().map(|t| lambda * t).collect();
        let mut hess = vec![0.0; n * n];
        for i in 0..n {
            hess[i * n + i] = lambda;
        }
        for b in 0..bins {
            let blocks: Vec<usize> = if bins == 1 { vec![0] } else { vec![0, b + 1] };
            for &bi in &blocks {
                for a in 0..d {
                    grad[bi * d + a] += g_b[b][a];
                }
                for &bj in &blocks {
                    for a in 0..d {
                        for c in 0..d {
                            let v = if a <= c {
                                h_b[b][a * d + c]
                            } else {
                                h_b[b][c * d + a]
                            };
                            hess[(bi * d + a) * n + bj * d + c] += v;
                        }
                    }
                }
            }
        }
        for (i, &f) in fixed.iter().enumerate() {
            if f {
                grad[i] = 0.0;
                for j in 0..n {
                    hess[i * n + j] = 0.0;
                    hess[j * n + i] = 0.0;
                }
                hess[i * n + i] = 1.0;
            }
        }
        let step = cholesky_solve(&hess, &grad, n)?;
        let mut largest = 0.0_f64;
        for (t, s) in theta.iter_mut().zip(&step) {
            *t -= s;
            if s.abs() > largest {
                largest = s.abs();
            }
        }
        if largest < STEP_TOL {
            break;
        }
    }
    Ok(theta)
}

/// The unconstrained fit of the rows `rows` of `ds`; one weight vector per bin.
fn fit(
    ds: &Design,
    rows: &[usize],
    bins: usize,
    lambda: f64,
    steps: u32,
) -> Result<Vec<Vec<f64>>, String> {
    let theta = newton(ds, rows, bins, lambda, steps, &[])?;
    Ok(bin_weights(&theta, ds.d, bins))
}

/// The gradient of the penalised negative log-likelihood of one weight vector `w` on `rows`.
fn gradient(ds: &Design, rows: &[usize], w: &[f64], lambda: f64) -> Vec<f64> {
    let mut g: Vec<f64> = w.iter().map(|t| lambda * t).collect();
    for &i in rows {
        let x = ds.row(i);
        let r = logistic(dot(w, x)) - ds.y[i];
        for (ga, &xa) in g.iter_mut().zip(x) {
            *ga += r * xa;
        }
    }
    g
}

/// The most active-set rounds of a sign-constrained fit.
const ACTIVE_SET_ROUNDS: usize = 64;

/// The fit with each weight's sign constrained by `signs` (+1 at least 0, -1 at most 0, 0
/// free), by an active set: the weights of the wrong sign are held at 0 and the fit repeated;
/// a held weight whose gradient points into its allowed side is released, the largest first.
/// A phase model's bins are then fitted separately (each bin's own rows), each with its own
/// active set. Deterministic: every choice is in column order.
fn fit_signed(
    ds: &Design,
    rows: &[usize],
    bins: usize,
    lambda: f64,
    steps: u32,
    signs: &[i8],
) -> Result<Vec<Vec<f64>>, String> {
    if signs.is_empty() {
        return fit(ds, rows, bins, lambda, steps);
    }
    let mut out = Vec::with_capacity(bins);
    for b in 0..bins {
        let bin_rows: Vec<usize> = if bins == 1 {
            rows.to_vec()
        } else {
            rows.iter()
                .copied()
                .filter(|&i| usize::from(ds.bin[i]).min(bins - 1) == b)
                .collect()
        };
        let mut fixed = vec![false; ds.d];
        let mut w = vec![0.0; ds.d];
        for _ in 0..ACTIVE_SET_ROUNDS {
            w = newton(ds, &bin_rows, 1, lambda, steps, &fixed)?;
            let mut violated = false;
            for (c, (&wc, &sc)) in w.iter().zip(signs).enumerate() {
                if !fixed[c] && f64::from(sc) * wc < 0.0 {
                    fixed[c] = true;
                    violated = true;
                }
            }
            if violated {
                continue;
            }
            let g = gradient(ds, &bin_rows, &w, lambda);
            let mut release: Option<(usize, f64)> = None;
            for (c, (&gc, &sc)) in g.iter().zip(signs).enumerate() {
                let pull = -f64::from(sc) * gc;
                if fixed[c] && pull > 1e-9 && release.is_none_or(|(_, best)| pull > best) {
                    release = Some((c, pull));
                }
            }
            match release {
                Some((c, _)) => fixed[c] = false,
                None => break,
            }
        }
        for (wc, &f) in w.iter_mut().zip(&fixed) {
            if f {
                *wc = 0.0;
            }
        }
        out.push(w);
    }
    Ok(out)
}

/// The sign of a superset feature's weight under `--signs`: the turn player's assets are good
/// for the turn player (+1), the other player's assets and the turn player's danger are bad
/// (-1), the intercept is free (0).
fn feature_sign(name: &str) -> i8 {
    match name {
        "intercept" => 0,
        "danger" | "can_kill_other" | "inv_race_other" => -1,
        n if n.starts_with("ot_") => -1,
        _ => 1,
    }
}

/// The summed log-loss of `rows` of `ds` under per-bin weights.
fn total_loss(ds: &Design, rows: &[usize], w: &[Vec<f64>]) -> f64 {
    let mut total = 0.0;
    for &i in rows {
        let b = usize::from(ds.bin[i]).min(w.len() - 1);
        total += loss(dot(&w[b], ds.row(i)), ds.y[i]);
    }
    total
}

#[expect(
    clippy::cast_precision_loss,
    reason = "sample counts are far below 2^53"
)]
fn mean(total: f64, n: usize) -> f64 {
    total / (n.max(1) as f64)
}

fn r6(x: f64) -> Json {
    gcg_core::pyfloat::round_f64(x, 6).into()
}

/// The turn terciles of the training positions: the edges of the phase bins.
fn phase_edges(turns: &mut [i32]) -> Vec<i32> {
    turns.sort();
    if turns.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for k in 1..PHASE_BINS {
        let e = turns[turns.len() * k / PHASE_BINS];
        if out.last().is_none_or(|&last| e > last) {
            out.push(e);
        }
    }
    out
}

/// What fitting one model reports, and the fitted model.
struct Fitted {
    report: Json,
    model: Model,
}

/// The inputs of every model's fit.
struct FitInput<'a> {
    train: Vec<&'a Position>,
    held: Vec<&'a Position>,
    deck_names: Vec<String>,
    deck_ids: Vec<[u16; 2]>,
    lambdas: Vec<f64>,
    folds: u32,
    /// Folds by pair (every game of a pair in one fold) instead of by game.
    by_pair: bool,
    /// Also refit without each deck's games and score them (W-36).
    deck_transfer: bool,
    /// Sign-constrained weights (`feature_sign`).
    signs: bool,
    steps: u32,
    seed: u64,
    threads: std::num::NonZeroUsize,
}

/// ROLE_WEIGHTS on the superset's role block (its `tempo` is the constant `intercept`).
fn role_weights_design_loss(ds: &Design, rows: &[usize]) -> f64 {
    let w = vec![ROLE_WEIGHTS.values().to_vec()];
    total_loss(ds, rows, &w)
}

fn fit_model(spec: ModelSpec, input: &FitInput<'_>, cancel: &Cancel) -> Result<Fitted, String> {
    let columns = spec.columns();
    let base: Vec<usize> = (0..ROLE_WEIGHTS.values().len()).collect();
    let mode = if spec.determinized {
        Mode::Det
    } else {
        Mode::True
    };
    let edges = if spec.phase {
        let mut turns: Vec<i32> = input.train.iter().map(|p| p.turn).collect();
        phase_edges(&mut turns)
    } else {
        Vec::new()
    };
    let bins = edges.len() + 1;
    let signs: Vec<i8> = if input.signs {
        spec.names().into_iter().map(feature_sign).collect()
    } else {
        Vec::new()
    };
    let own = design(&input.train, mode, &columns, &edges, &input.deck_ids);
    let det = design(&input.train, Mode::Det, &columns, &edges, &input.deck_ids);
    let tru = design(&input.train, Mode::True, &columns, &edges, &input.deck_ids);
    let det_base = design(&input.train, Mode::Det, &base, &[], &input.deck_ids);
    let tru_base = design(&input.train, Mode::True, &base, &[], &input.deck_ids);
    let unit = |ds: &Design, i: usize| {
        if input.by_pair {
            ds.pair[i]
        } else {
            ds.game[i]
        }
    };
    let units: Vec<u32> = (0..tru.len()).map(|i| unit(&tru, i)).collect();
    let folds = folds_of(&units, input.folds, input.seed)?;
    let fold_of = |g: u32| {
        folds
            .binary_search_by_key(&g, |(x, _)| *x)
            .map_or(0, |i| folds[i].1)
    };
    let split = |ds: &Design, f: u32| -> (Vec<usize>, Vec<usize>) {
        (0..ds.len()).partition(|&i| fold_of(unit(ds, i)) != f)
    };
    let jobs: Vec<(usize, u32)> = (0..input.lambdas.len())
        .flat_map(|l| (0..input.folds).map(move |f| (l, f)))
        .collect();
    let pool = Pool::new(input.threads);
    let per_job = pool
        .map_indexed::<(), _, String>(
            jobs.len(),
            cancel,
            |j, _scratch, _par| {
                let (l, f) = jobs[j];
                let (fit_rows, _) = split(&own, f);
                let w = fit_signed(&own, &fit_rows, bins, input.lambdas[l], input.steps, &signs)?;
                let (_, own_test) = split(&own, f);
                let (_, det_test) = split(&det, f);
                let (_, tru_test) = split(&tru, f);
                // Per-sample out-of-fold losses of the determinised samples (per-deck table).
                let det_losses: Vec<(usize, f64)> = det_test
                    .iter()
                    .map(|&i| {
                        let b = usize::from(det.bin[i]).min(w.len() - 1);
                        (i, loss(dot(&w[b], det.row(i)), det.y[i]))
                    })
                    .collect();
                Ok((
                    total_loss(&own, &own_test, &w),
                    total_loss(&det, &det_test, &w),
                    total_loss(&tru, &tru_test, &w),
                    det_losses,
                ))
            },
            |_| {},
        )
        .map_err(|e| match e {
            gcg_core::pool::PoolError::Cancelled => "interrupted".to_owned(),
            gcg_core::pool::PoolError::Task { error, .. } => error,
        })?;
    let mut cv = Vec::new();
    let mut best = 0;
    let mut scores = Vec::new();
    for (l, &lambda) in input.lambdas.iter().enumerate() {
        let (mut o, mut dt, mut t) = (0.0, 0.0, 0.0);
        for (j, &(jl, _)) in jobs.iter().enumerate() {
            if jl == l {
                o += per_job[j].0;
                dt += per_job[j].1;
                t += per_job[j].2;
            }
        }
        let (o, dt, t) = (mean(o, own.len()), mean(dt, det.len()), mean(t, tru.len()));
        scores.push((o, dt, t));
        if o < scores[best].0 {
            best = l;
        }
        let mut row = Json::object();
        row.set("lambda", lambda);
        row.set("logloss_own", r6(o));
        row.set("logloss_determinized", r6(dt));
        row.set("logloss_true_state", r6(t));
        cv.push(row);
    }
    let lambda = input.lambdas[best];
    let all: Vec<usize> = (0..own.len()).collect();
    let weights = fit_signed(&own, &all, bins, lambda, input.steps, &signs)?;

    // Out-of-fold per-sample losses at the chosen lambda, against ROLE_WEIGHTS on the same
    // determinised samples, per deck (W-36), clustered by game.
    let mut oof = vec![0.0; det.len()];
    for (j, &(jl, _)) in jobs.iter().enumerate() {
        if jl == best {
            for &(i, v) in &per_job[j].3 {
                oof[i] = v;
            }
        }
    }
    let role_w = vec![ROLE_WEIGHTS.values().to_vec()];
    let mut per_deck = Vec::new();
    for (k, name) in input.deck_names.iter().enumerate() {
        let k16 = u16::try_from(k).unwrap_or(u16::MAX);
        let mut by_game: Vec<(u32, f64, usize)> = Vec::new();
        let (mut sum_m, mut sum_r, mut n) = (0.0, 0.0, 0usize);
        for (i, &model_loss) in oof.iter().enumerate() {
            if !det.deck[i].contains(&k16) {
                continue;
            }
            let r = loss(dot(&role_w[0], det_base.row(i)), det_base.y[i]);
            let diff = model_loss - r;
            sum_m += model_loss;
            sum_r += r;
            n += 1;
            match by_game.last_mut() {
                Some(last) if last.0 == det.game[i] => {
                    last.1 += diff;
                    last.2 += 1;
                }
                _ => by_game.push((det.game[i], diff, 1)),
            }
        }
        if n == 0 {
            continue;
        }
        let mean_diff = mean(sum_m - sum_r, n);
        let mut ss = 0.0;
        for &(_, s, c) in &by_game {
            #[expect(clippy::cast_precision_loss, reason = "small counts")]
            let e = s - mean_diff * (c as f64);
            ss += e * e;
        }
        #[expect(clippy::cast_precision_loss, reason = "small counts")]
        let (g, nf) = (by_game.len() as f64, n as f64);
        let se = if g > 1.0 {
            (ss * g / (g - 1.0)).sqrt() / nf
        } else {
            0.0
        };
        let mut o = Json::object();
        o.set("deck", name.as_str());
        o.set("samples", n);
        o.set("games", by_game.len());
        o.set("logloss_model", r6(mean(sum_m, n)));
        o.set("logloss_role_weights", r6(mean(sum_r, n)));
        o.set("difference", r6(mean_diff));
        o.set("se", r6(se));
        o.set("worse_by_more_than_one_se", mean_diff > se);
        per_deck.push(o);
    }

    let held =
        |m: Mode, cols: &[usize], e: &[i32]| design(&input.held, m, cols, e, &input.deck_ids);
    let (hd, ht) = (
        held(Mode::Det, &columns, &edges),
        held(Mode::True, &columns, &edges),
    );
    let (hdb, htb) = (held(Mode::Det, &base, &[]), held(Mode::True, &base, &[]));
    let rows = |ds: &Design| (0..ds.len()).collect::<Vec<usize>>();
    let mut held_doc = Json::object();
    held_doc.set("samples_determinized", hd.len());
    held_doc.set("samples_true_state", ht.len());
    let ll = |ds: &Design, w: &[Vec<f64>]| {
        if ds.len() == 0 {
            Json::Null
        } else {
            r6(mean(total_loss(ds, &rows(ds), w), ds.len()))
        }
    };
    held_doc.set("logloss_determinized", ll(&hd, &weights));
    held_doc.set("logloss_true_state", ll(&ht, &weights));
    held_doc.set("role_weights_determinized", ll(&hdb, &role_w));
    held_doc.set("role_weights_true_state", ll(&htb, &role_w));

    let mut oof_doc = Json::object();
    oof_doc.set("own", r6(scores[best].0));
    oof_doc.set("determinized", r6(scores[best].1));
    oof_doc.set("true_state", r6(scores[best].2));
    oof_doc.set(
        "role_weights_determinized",
        r6(mean(
            role_weights_design_loss(&det_base, &rows(&det_base)),
            det_base.len(),
        )),
    );
    oof_doc.set(
        "role_weights_true_state",
        r6(mean(
            role_weights_design_loss(&tru_base, &rows(&tru_base)),
            tru_base.len(),
        )),
    );

    let mut doc = Json::object();
    doc.set("key", spec.key());
    doc.set("mode", mode.as_str());
    doc.set(
        "features",
        Json::Array(spec.names().into_iter().map(Json::from).collect()),
    );
    doc.set("edges", edges.clone());
    let mut samples = Json::object();
    samples.set("train", own.len());
    samples.set("train_determinized", det.len());
    samples.set("train_true_state", tru.len());
    doc.set("samples", samples);
    doc.set("cv", Json::Array(cv));
    doc.set("lambda", lambda);
    doc.set("out_of_fold", oof_doc);
    doc.set("held_out", held_doc);
    doc.set("per_deck", Json::Array(per_deck));
    if input.deck_transfer {
        doc.set(
            "deck_transfer",
            deck_transfer(&own, &det, &det_base, bins, lambda, &signs, input)?,
        );
    }
    doc.set(
        "weights",
        Json::Array(
            weights
                .iter()
                .map(|w| {
                    let mut o = Json::object();
                    for (name, v) in spec.names().iter().zip(w) {
                        o.set(*name, r6(*v));
                    }
                    o
                })
                .collect(),
        ),
    );
    Ok(Fitted {
        report: doc,
        model: Model {
            spec,
            columns,
            edges,
            weights,
        },
    })
}

/// The log-loss of every fitted model and of `ROLE_WEIGHTS` on every position of `set` from
/// `--min-turn` on (determinised and true-state samples), with a game-clustered standard error
/// of each model's difference to `ROLE_WEIGHTS` on the determinised samples.
fn test_report(path: &Path, set: &PositionSet, fitted: &[Model], min_turn: i32) -> Json {
    let positions: Vec<&Position> = set
        .positions
        .iter()
        .filter(|p| p.turn >= min_turn)
        .collect();
    let (_, deck_ids) = deck_index(&set.pairs);
    let base: Vec<usize> = (0..ROLE_WEIGHTS.values().len()).collect();
    let role_w = vec![ROLE_WEIGHTS.values().to_vec()];
    let det_base = design(&positions, Mode::Det, &base, &[], &deck_ids);
    let tru_base = design(&positions, Mode::True, &base, &[], &deck_ids);
    let all = |ds: &Design| (0..ds.len()).collect::<Vec<usize>>();
    let role_det: Vec<f64> = (0..det_base.len())
        .map(|i| loss(dot(&role_w[0], det_base.row(i)), det_base.y[i]))
        .collect();
    let mut doc = Json::object();
    // The file name only: a report never names the scratch directory of its datasets.
    doc.set(
        "positions_file",
        path.file_name()
            .map_or_else(|| py_path_str(path), |n| n.to_string_lossy().into_owned()),
    );
    doc.set("source", without_features(&set.header));
    doc.set("samples_determinized", det_base.len());
    doc.set("samples_true_state", tru_base.len());
    doc.set(
        "role_weights_determinized",
        r6(mean(
            total_loss(&det_base, &all(&det_base), &role_w),
            det_base.len(),
        )),
    );
    doc.set(
        "role_weights_true_state",
        r6(mean(
            total_loss(&tru_base, &all(&tru_base), &role_w),
            tru_base.len(),
        )),
    );
    let mut rows = Vec::new();
    for m in fitted {
        let det = design(&positions, Mode::Det, &m.columns, &m.edges, &deck_ids);
        let tru = design(&positions, Mode::True, &m.columns, &m.edges, &deck_ids);
        let mut by_game: Vec<(u32, f64, usize)> = Vec::new();
        let mut total = 0.0;
        for (i, &r) in role_det.iter().enumerate() {
            let b = usize::from(det.bin[i]).min(m.weights.len() - 1);
            let v = loss(dot(&m.weights[b], det.row(i)), det.y[i]);
            total += v;
            match by_game.last_mut() {
                Some(last) if last.0 == det.game[i] => {
                    last.1 += v - r;
                    last.2 += 1;
                }
                _ => by_game.push((det.game[i], v - r, 1)),
            }
        }
        let n = det.len();
        let role_total: f64 = {
            let mut s = 0.0;
            for r in &role_det {
                s += r;
            }
            s
        };
        let diff = mean(total - role_total, n);
        let mut ss = 0.0;
        for &(_, s, c) in &by_game {
            #[expect(clippy::cast_precision_loss, reason = "small counts")]
            let e = s - diff * (c as f64);
            ss += e * e;
        }
        #[expect(clippy::cast_precision_loss, reason = "small counts")]
        let (g, nf) = (by_game.len() as f64, n.max(1) as f64);
        let se = if g > 1.0 {
            (ss * g / (g - 1.0)).sqrt() / nf
        } else {
            0.0
        };
        let mut o = Json::object();
        o.set("key", m.spec.key());
        o.set("logloss_determinized", r6(mean(total, n)));
        o.set(
            "logloss_true_state",
            r6(mean(total_loss(&tru, &all(&tru), &m.weights), tru.len())),
        );
        o.set("difference_to_role_weights_determinized", r6(diff));
        o.set("se", r6(se));
        rows.push(o);
    }
    doc.set("models", Json::Array(rows));
    doc
}

/// The leave-one-deck-out table (W-36): per deck, the model refitted at `lambda` on the
/// training samples of games without that deck, scored on the determinised samples of games
/// with it, against `ROLE_WEIGHTS` on the same samples, with a game-clustered standard error.
fn deck_transfer(
    own: &Design,
    det: &Design,
    det_base: &Design,
    bins: usize,
    lambda: f64,
    signs: &[i8],
    input: &FitInput<'_>,
) -> Result<Json, String> {
    let role_w = [ROLE_WEIGHTS.values().to_vec()];
    let pool = Pool::new(input.threads);
    let decks: Vec<u16> = (0..input.deck_names.len())
        .map(|k| u16::try_from(k).unwrap_or(u16::MAX))
        .collect();
    let rows = pool
        .map_indexed::<(), _, String>(
            decks.len(),
            &Cancel::new(),
            |k, _scratch, _par| {
                let d = decks[k];
                let train: Vec<usize> = (0..own.len())
                    .filter(|&i| !own.deck[i].contains(&d))
                    .collect();
                let w = fit_signed(own, &train, bins, lambda, input.steps, signs)?;
                let mut by_game: Vec<(u32, f64, usize)> = Vec::new();
                let (mut sum_m, mut sum_r, mut n) = (0.0, 0.0, 0usize);
                for i in (0..det.len()).filter(|&i| det.deck[i].contains(&d)) {
                    let b = usize::from(det.bin[i]).min(w.len() - 1);
                    let m = loss(dot(&w[b], det.row(i)), det.y[i]);
                    let r = loss(dot(&role_w[0], det_base.row(i)), det_base.y[i]);
                    sum_m += m;
                    sum_r += r;
                    n += 1;
                    match by_game.last_mut() {
                        Some(last) if last.0 == det.game[i] => {
                            last.1 += m - r;
                            last.2 += 1;
                        }
                        _ => by_game.push((det.game[i], m - r, 1)),
                    }
                }
                let diff = mean(sum_m - sum_r, n);
                let mut ss = 0.0;
                for &(_, s, c) in &by_game {
                    #[expect(clippy::cast_precision_loss, reason = "small counts")]
                    let e = s - diff * (c as f64);
                    ss += e * e;
                }
                #[expect(clippy::cast_precision_loss, reason = "small counts")]
                let (g, nf) = (by_game.len() as f64, n.max(1) as f64);
                let se = if g > 1.0 {
                    (ss * g / (g - 1.0)).sqrt() / nf
                } else {
                    0.0
                };
                let mut o = Json::object();
                o.set("deck", input.deck_names[k].as_str());
                o.set("samples", n);
                o.set("games", by_game.len());
                o.set("training_samples", train.len());
                o.set("logloss_model", r6(mean(sum_m, n)));
                o.set("logloss_role_weights", r6(mean(sum_r, n)));
                o.set("difference", r6(diff));
                o.set("se", r6(se));
                o.set("worse_by_more_than_one_se", diff > se);
                Ok(o)
            },
            |_| {},
        )
        .map_err(|e| match e {
            gcg_core::pool::PoolError::Cancelled => "interrupted".to_owned(),
            gcg_core::pool::PoolError::Task { error, .. } => error,
        })?;
    Ok(Json::Array(rows))
}

/// The deck names of the pairs and each pair's two deck ids.
fn deck_index(pairs: &[[String; 2]]) -> (Vec<String>, Vec<[u16; 2]>) {
    let mut names: Vec<String> = Vec::new();
    let mut ids = Vec::with_capacity(pairs.len());
    for p in pairs {
        let mut id = [0u16; 2];
        for (slot, name) in id.iter_mut().zip(p) {
            let k = names.iter().position(|n| n == name).unwrap_or_else(|| {
                names.push(name.clone());
                names.len() - 1
            });
            *slot = u16::try_from(k).unwrap_or(u16::MAX);
        }
        ids.push(id);
    }
    (names, ids)
}

/// Runs `tune-leaf`.
///
/// # Errors
/// A setup problem, a failed game, the interrupt, or the output.
pub fn run(
    args: &TuneLeafArgs,
    cancel: &Cancel,
    tty: bool,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<(), CommandError> {
    let invalid = CommandError::Invalid;
    let specs: Vec<ModelSpec> = args
        .models
        .iter()
        .map(|m| model_of(m))
        .collect::<Result<_, _>>()
        .map_err(invalid)?;
    let lambdas: Vec<f64> = args
        .lambdas
        .split(',')
        .map(|t| {
            t.trim()
                .parse::<f64>()
                .ok()
                .filter(|l| l.is_finite() && *l > 0.0)
        })
        .collect::<Option<_>>()
        .ok_or_else(|| CommandError::Invalid("--lambdas must be positive numbers".into()))?;
    if args.steps == 0 {
        return Err(CommandError::Invalid("--steps must be positive".into()));
    }
    let mut sets: Vec<PositionSet> = Vec::new();
    let mut doc = Json::object();
    doc.set("format", REPORT_FORMAT);
    doc.set("version", FORMAT_VERSION);
    doc.set("leaf_point", p6g::LEAF_POINT.as_str());
    if args.positions.is_empty() {
        let selection = prepare(&args.opts).map_err(invalid)?;
        let total = u64::from(args.opts.games);
        let mut printer = progress_printer(err, tty);
        let mut progress = |d: usize| printer.update(u64::try_from(d).unwrap_or(0), total);
        let (positions, dropped) =
            record_positions(&args.agent, &selection, &args.opts, cancel, &mut progress)?;
        let pairs: Vec<[String; 2]> = selection
            .pairs
            .iter()
            .map(|p| [p.decks[0].name.clone(), p.decks[1].name.clone()])
            .collect();
        let mut header = Json::object();
        header.set("format", POSITIONS_FORMAT);
        header.set("version", FORMAT_VERSION);
        header.set("leaf_point", p6g::LEAF_POINT.as_str());
        header.set("agent", args.agent.as_str());
        args.opts.describe(&mut header);
        header.set(
            "pairs",
            Json::Array(
                pairs
                    .iter()
                    .map(|p| Json::Array(vec![p[0].as_str().into(), p[1].as_str().into()]))
                    .collect(),
            ),
        );
        header.set("positions", positions.len());
        header.set("drawn_games_dropped", dropped);
        header.set(
            "features",
            Json::Array(p6g::NAMES.iter().map(|n| (*n).into()).collect()),
        );
        if let Some(path) = &args.record {
            std::fs::write(path, positions_bytes(&header, &positions)).map_err(|e| {
                CommandError::Invalid(format!("cannot write {}: {e}", py_path_str(path)))
            })?;
        }
        let mut extra = Json::object();
        extra.set("agent", args.agent.as_str());
        args.opts.record("tune-leaf", extra)?;
        sets.push(PositionSet {
            pairs,
            header,
            positions,
        });
    } else {
        for p in &args.positions {
            sets.push(read_positions(p).map_err(invalid)?);
        }
    }

    // Merge: game ids offset per file, so games stay distinct; a pair is identified by its
    // deck names, so one pair recorded in two files is one pair (held out or folded together).
    let mut pairs: Vec<[String; 2]> = Vec::new();
    let mut merged: Vec<Position> = Vec::new();
    let mut sources = Vec::new();
    for (k, set) in sets.into_iter().enumerate() {
        let game_base = u32::try_from(k).unwrap_or(0).saturating_mul(10_000_000);
        sources.push(without_features(&set.header));
        let ids: Vec<u32> = set
            .pairs
            .iter()
            .map(|names| {
                let i = pairs.iter().position(|p| p == names).unwrap_or_else(|| {
                    pairs.push(names.clone());
                    pairs.len() - 1
                });
                u32::try_from(i).unwrap_or(u32::MAX)
            })
            .collect();
        merged.extend(set.positions.into_iter().map(|mut p| {
            p.pair = ids[usize::try_from(p.pair).unwrap_or(0)];
            p.game += game_base;
            p
        }));
    }
    doc.set("sources", Json::Array(sources));
    let n_pairs = u32::try_from(pairs.len()).unwrap_or(u32::MAX);
    if args.holdout_pairs >= n_pairs && !specs.is_empty() {
        return Err(CommandError::Invalid(format!(
            "--holdout-pairs {} leaves no pair to fit on ({n_pairs} pairs)",
            args.holdout_pairs
        )));
    }
    let first_held = n_pairs.saturating_sub(args.holdout_pairs);
    let kept: Vec<&Position> = merged.iter().filter(|p| p.turn >= args.min_turn).collect();
    let (deck_names, deck_ids) = deck_index(&pairs);
    let input = FitInput {
        train: kept
            .iter()
            .copied()
            .filter(|p| p.pair < first_held)
            .collect(),
        held: kept
            .iter()
            .copied()
            .filter(|p| p.pair >= first_held)
            .collect(),
        deck_names,
        deck_ids,
        lambdas: lambdas.clone(),
        folds: args.folds,
        by_pair: args.fold_by_pair,
        deck_transfer: args.deck_transfer,
        signs: args.signs,
        steps: args.steps,
        seed: args.opts.seed,
        threads: threads(args.opts.workers, usize::MAX),
    };
    let mut fit_doc = Json::object();
    fit_doc.set("min_turn", args.min_turn);
    fit_doc.set("folds", args.folds);
    fit_doc.set("lambdas", lambdas);
    fit_doc.set("steps", args.steps);
    fit_doc.set("fold_seed", args.opts.seed);
    fit_doc.set("fold_by", if args.fold_by_pair { "pair" } else { "game" });
    fit_doc.set("signs", args.signs);
    fit_doc.set("positions", merged.len());
    fit_doc.set("positions_fitted", input.train.len());
    fit_doc.set("positions_held_out", input.held.len());
    fit_doc.set(
        "held_out_pairs",
        Json::Array(
            pairs
                .iter()
                .skip(usize::try_from(first_held).unwrap_or(usize::MAX))
                .map(|p| Json::Array(vec![p[0].as_str().into(), p[1].as_str().into()]))
                .collect(),
        ),
    );
    doc.set("fit", fit_doc);
    let mut models = Vec::new();
    let mut fitted = Vec::new();
    for spec in &specs {
        let f = fit_model(*spec, &input, cancel).map_err(invalid)?;
        models.push(f.report);
        fitted.push(f.model);
    }
    doc.set("models", Json::Array(models));
    let mut tests = Vec::new();
    for path in &args.test_positions {
        let set = read_positions(path).map_err(invalid)?;
        tests.push(test_report(path, &set, &fitted, args.min_turn));
    }
    doc.set("tests", Json::Array(tests));
    if let Some(path) = &args.weights_out {
        let header = vec![
            "P6-G fitted evaluation models (crates/gcg-search/src/mech/p6g/model.rs).".to_owned(),
            "Written by gcg-eval tune-leaf; regenerate with the command in".to_owned(),
            "docs/port/eval/P6-G/README.md, never by hand.".to_owned(),
        ];
        std::fs::write(path, p6g::table_text(&header, &fitted)).map_err(|e| {
            CommandError::Invalid(format!("cannot write {}: {e}", py_path_str(path)))
        })?;
    }
    emit(&doc, args.opts.out.as_ref(), out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gcg_core::SplitMix64;

    /// A synthetic dataset from a known logistic model is recovered (W-40's check for this
    /// fitter), and the phase fit recovers per-bin weights.
    #[test]
    fn synthetic_weights_are_recovered() {
        let truth = [[0.8, -1.2, 0.4], [0.2, 0.6, -0.9]];
        let mut rng = SplitMix64::new(7);
        let mut ds = Design {
            d: 3,
            x: Vec::new(),
            y: Vec::new(),
            bin: Vec::new(),
            game: Vec::new(),
            pair: Vec::new(),
            deck: Vec::new(),
        };
        for i in 0..20_000u32 {
            let b = usize::from(rng.random() < 0.5);
            let x = [1.0, 4.0 * rng.random() - 2.0, 4.0 * rng.random() - 2.0];
            let p = logistic(dot(&truth[b], &x));
            ds.x.extend_from_slice(&x);
            ds.y.push(if rng.random() < p { 1.0 } else { 0.0 });
            ds.bin.push(u8::try_from(b).unwrap_or(0));
            ds.game.push(i / 10);
            ds.deck.push([0, 0]);
        }
        let rows: Vec<usize> = (0..ds.len()).collect();
        let w = fit(&ds, &rows, 2, 0.01, 30).unwrap_or_default();
        for (b, wb) in w.iter().enumerate() {
            for (got, want) in wb.iter().zip(truth[b]) {
                assert!((got - want).abs() < 0.12, "{w:?} vs {truth:?}");
            }
        }
        let again = fit(&ds, &rows, 2, 0.01, 30).unwrap_or_default();
        assert_eq!(w, again, "the fit is deterministic");
    }

    #[test]
    fn cholesky_solves_a_known_system() {
        let x = cholesky_solve(&[4.0, 2.0, 2.0, 3.0], &[2.0, 1.0], 2).unwrap_or_default();
        assert!((x[0] - 0.5).abs() < 1e-12 && x[1].abs() < 1e-12, "{x:?}");
        assert!(cholesky_solve(&[0.0, 0.0, 0.0, 0.0], &[1.0, 1.0], 2).is_err());
    }

    #[test]
    fn models_are_named_by_their_flags() {
        let spec = model_of("determinized_fit+keyword_features").unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(spec.key(), "kw=1,ab=0,hs=0,bp=0,det=1,int=1,ph=0");
        assert!(model_of("leaf_density").is_err());
        assert!(model_of("root_trees").is_err());
        assert_eq!(phase_edges(&mut [3, 3, 4, 5, 6, 7, 8, 9, 10]), vec![5, 8]);
    }

    #[test]
    fn positions_round_trip() {
        let n = p6g::N_FEATURES;
        let pos = Position {
            game: 3,
            pair: 0,
            turn: 5,
            active: 1,
            tp_deck: 1,
            y: 1.0,
            x: [
                vec![0.5; n],
                vec![0.25; n],
                (0..n)
                    .map(|i| f64::from(u32::try_from(i).unwrap_or(0)))
                    .collect(),
            ],
        };
        let mut header = Json::object();
        header.set("format", POSITIONS_FORMAT);
        header.set(
            "features",
            Json::Array(p6g::NAMES.iter().map(|x| (*x).into()).collect()),
        );
        header.set(
            "pairs",
            Json::Array(vec![Json::Array(vec!["a".into(), "b".into()])]),
        );
        let dir = std::env::temp_dir().join(format!("gcg-eval-leaf-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("p.bin");
        std::fs::write(&path, positions_bytes(&header, std::slice::from_ref(&pos)))
            .unwrap_or_default();
        let set = read_positions(&path).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(set.positions, vec![pos]);
        assert_eq!(set.pairs, vec![["a".to_owned(), "b".to_owned()]]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// W-39, W-40: the recorder records once per turn, at the leaf point, from turn 1; the
    /// positions and the fitted models are bit-identical for 1 and 3 workers.
    #[test]
    fn recording_and_fit_do_not_depend_on_the_worker_count() {
        use crate::head_to_head::{PairSource, select_with};
        use crate::{ChooseFirst, DeckSet, PairSet};
        let selection = select_with(
            &PairSource::Set {
                decks: DeckSet::Tuning,
                pairs: PairSet::All,
            },
            None,
        )
        .unwrap_or_else(|e| panic!("{e}"));
        let opts = |w| RunOptions {
            source: PairSource::Named(String::new()),
            games: 12,
            seed: 7,
            role: None,
            ledger: PathBuf::from("unused-ledger.json"),
            workers: gcg_runner::Workers::Count(w),
            choose_first: ChooseFirst::Seat0,
            out: None,
        };
        let cancel = Cancel::new();
        let record = |w| {
            record_positions("greedy", &selection, &opts(w), &cancel, &mut |_| {})
                .unwrap_or_else(|_| panic!("recording"))
        };
        let (one, dropped) = record(1);
        assert_eq!(record(3), (one.clone(), dropped));
        assert!(one.len() > 12 * 5, "{} positions", one.len());
        for g in 0..12 {
            let turns: Vec<i32> = one.iter().filter(|p| p.game == g).map(|p| p.turn).collect();
            assert!(turns.windows(2).all(|t| t[1] == t[0] + 1), "{turns:?}");
            assert!(turns.first().is_none_or(|&t| t == 1), "{turns:?}");
        }
        let pairs: Vec<[String; 2]> = selection
            .pairs
            .iter()
            .map(|p| [p.decks[0].name.clone(), p.decks[1].name.clone()])
            .collect();
        let (deck_names, deck_ids) = deck_index(&pairs);
        let fit_with = |threads: usize| {
            let input = FitInput {
                train: one.iter().filter(|p| p.pair < 2 && p.turn >= 3).collect(),
                held: one.iter().filter(|p| p.pair == 2 && p.turn >= 3).collect(),
                deck_names: deck_names.clone(),
                deck_ids: deck_ids.clone(),
                lambdas: vec![10.0, 100.0],
                folds: 2,
                by_pair: false,
                deck_transfer: true,
                signs: false,
                steps: 30,
                seed: 7,
                threads: std::num::NonZeroUsize::new(threads)
                    .unwrap_or(std::num::NonZeroUsize::MIN),
            };
            let spec = model_of("determinized_fit+phase_weights").unwrap_or_else(|e| panic!("{e}"));
            fit_model(spec, &input, &cancel).unwrap_or_else(|e| panic!("{e}"))
        };
        let (a, b) = (fit_with(1), fit_with(3));
        assert_eq!(a.model, b.model);
        assert_eq!(dumps_compact(&a.report), dumps_compact(&b.report));
    }

    fn dumps_compact(doc: &Json) -> String {
        compact_json(doc)
    }
}
