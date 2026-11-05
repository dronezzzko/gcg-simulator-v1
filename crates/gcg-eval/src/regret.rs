//! `gcg-eval regret CANDIDATE ORACLE` (docs/port/PLAN.md §12.2 P6-I; W-43): decision-level
//! regret of a candidate agent against a much larger search, the committed form of the
//! investigation's oracle replay.
//!
//! 1. **Positions.** `--games` self-play games of `--recorder` (default the candidate) in
//!    both seats, mirrored and stratified over the selected pairs, as the other row tools
//!    play them. In each game `--per-game` positions are sampled uniformly from the decisions
//!    with at least two options that `--decisions` admits (`all`, `target` or one decision
//!    kind; reservoir sampling with `derive_seed(seed, "regret", "sample", game)`; a
//!    CHOOSE_FIRST the harness answers is not a decision), kept in game order.
//! 2. **Choices.** At each position the candidate and the oracle, both built fresh with the
//!    seed `derive_seed(seed, "regret", "agent", position)`, choose on the true state, as in
//!    a game. The same spec on both sides therefore chooses the same action, and a position
//!    where the two agree has regret exactly 0 (no playouts are run).
//! 3. **Playouts.** Otherwise both actions are played out in `--worlds` shared worlds: world
//!    `w` is `determinize(state, player, derive_seed(seed, "regret", "world", position, w))`,
//!    each action is applied to a copy, and the game is finished by the judge with the
//!    rollout seed `derive_seed(seed, "regret", "rollout", position, w)` for both actions
//!    (common random numbers). `--judge policy` (default) plays both seats with the playout
//!    policy at `--temperature` for at most `--max-decisions` moves; `--judge SPEC` plays both
//!    seats with that agent (seeds `derive_seed(seed, "regret", "judge", position, w,
//!    seat)`). The value is the player's result (1, 0, 0.5 for a draw) or, for a capped
//!    playout, `evaluate` with `DEFAULT_WEIGHTS`.
//! 4. **Regret.** Per position, the mean over worlds of `value(oracle) - value(candidate)`
//!    with its paired standard error; per decision kind, for target decisions
//!    (`gcg_search::mcts::is_target_decision`) and over all positions, the mean regret, its
//!    standard error across positions and the normal 95% interval.
//!
//! Every seed derives from the position index, so the report is identical for any
//! `--workers`.

use std::io::Write;
use std::sync::Mutex;

use gcg_ai::{BASELINE_KNOBS, DEFAULT_WEIGHTS, burst_density, evaluate, play_to_end};
use gcg_core::pool::Cancel;
use gcg_core::pyfloat::round_f64;
use gcg_core::pyjson::Json;
use gcg_core::{SplitMix64, derive_seed};
use gcg_engine::{Action, DecisionKind, GameState, apply, observe};

use crate::agent_spec::{make_agent, parse_agent_spec};
use crate::bench_cmd::CommandError;
use crate::head_to_head::stratified_specs;
use crate::rows::{RunOptions, emit, prepare, progress_printer};
use crate::selfplay::{GameError, Observer, play_observed, run_indexed};
use crate::{AgentDesc, ChooseFirst, DECISION_LIMIT};

/// The report format.
pub const REGRET_FORMAT: &str = "gcg-eval-regret";
pub const REGRET_VERSION: u32 = 1;

/// How playouts are finished.
#[derive(Clone, PartialEq, Debug)]
pub enum Judge {
    Policy,
    Agent(String),
}

/// The parsed arguments.
#[derive(Clone, PartialEq, Debug)]
pub struct RegretArgs {
    pub candidate: String,
    pub oracle: String,
    pub recorder: Option<String>,
    pub per_game: u32,
    pub decisions: Decisions,
    pub worlds: u32,
    pub judge: Judge,
    pub temperature: f64,
    pub max_decisions: u32,
    pub opts: RunOptions,
}

/// Which decisions are sampled (`--decisions`): all, the target decisions, or one kind.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Decisions {
    All,
    Target,
    Kind(DecisionKind),
}

impl Decisions {
    /// `all`, `target`, or a decision kind's value (`main`, `select`, ...).
    #[must_use]
    pub fn from_value(v: &str) -> Option<Self> {
        match v {
            "all" => Some(Self::All),
            "target" => Some(Self::Target),
            _ => DecisionKind::ALL
                .into_iter()
                .find(|k| k.as_str() == v)
                .map(Self::Kind),
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Target => "target",
            Self::Kind(k) => k.as_str(),
        }
    }

    fn admits(self, kind: DecisionKind, target: bool) -> bool {
        match self {
            Self::All => true,
            Self::Target => target,
            Self::Kind(k) => k == kind,
        }
    }
}

/// A sampled position.
pub struct Position {
    pub game: u32,
    pub ordinal: u32,
    pub player: u8,
    pub kind: DecisionKind,
    pub target: bool,
    pub state: GameState,
}

/// Reservoir sampling of the decisions of one game.
struct Sampler {
    keep: usize,
    choose_first: ChooseFirst,
    decisions: Decisions,
    rng: SplitMix64,
    seen: u32,
    slots: Vec<(u32, u8, DecisionKind, bool, GameState)>,
}

impl Observer for Sampler {
    fn before(&mut self, st: &GameState, player: u8) -> Result<(), GameError> {
        let Some(dec) = st.pending() else {
            return Ok(());
        };
        if dec.options.len() < 2
            || (dec.hdr.kind == DecisionKind::ChooseFirst
                && self.choose_first == ChooseFirst::Seat0)
        {
            return Ok(());
        }
        let target = gcg_search::mcts::is_target_decision(&dec);
        if !self.decisions.admits(dec.hdr.kind, target) {
            return Ok(());
        }
        let t = self.seen;
        self.seen += 1;
        let entry = || (t, player, dec.hdr.kind, target, st.clone());
        let tu = usize::try_from(t).unwrap_or(usize::MAX);
        if tu < self.keep {
            self.slots.push(entry());
        } else {
            let j = self.rng.randrange_usize(tu + 1);
            if j < self.keep {
                self.slots[j] = entry();
            }
        }
        Ok(())
    }
}

/// One position's result: whether the agents agreed, and the regret with its standard error
/// over worlds (0 and none when they agreed).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct PositionRegret {
    pub same: bool,
    pub regret: f64,
    pub se: Option<f64>,
}

#[expect(
    clippy::cast_precision_loss,
    reason = "sample counts are far below 2^53"
)]
fn mean_se(xs: &[f64]) -> (f64, Option<f64>) {
    let n = xs.len();
    if n == 0 {
        return (0.0, None);
    }
    let mean = gcg_core::pymath::py_sum(xs.iter().copied()) / n as f64;
    if n < 2 {
        return (mean, None);
    }
    let ss = gcg_core::pymath::py_sum(xs.iter().map(|x| (x - mean) * (x - mean)));
    let var = ss / (n - 1) as f64;
    (mean, Some(gcg_core::libm::sqrt(var / n as f64)))
}

fn value_of(st: &GameState, player: u8) -> Result<f64, GameError> {
    let density = burst_density(st, player);
    evaluate(st, player, DEFAULT_WEIGHTS, density).map_err(|e| GameError::engine(&e))
}

/// Finishes `world` after `action` with the judge and returns `player`'s value.
#[expect(
    clippy::too_many_arguments,
    reason = "the playout's seeds and settings"
)]
fn playout(
    world: &GameState,
    action: Action,
    player: u8,
    judge: Option<&AgentDesc>,
    seeds: (u64, [u64; 2]),
    temperature: f64,
    max_decisions: u32,
    cx: &mut gcg_ai::AgentCx<'_>,
) -> Result<f64, GameError> {
    let mut s = world.clone();
    apply(&mut s, action, true)?;
    match judge {
        None => {
            let mut rng = SplitMix64::new(seeds.0);
            play_to_end(
                &mut s,
                Some(&mut rng),
                temperature,
                max_decisions,
                &BASELINE_KNOBS,
            )
            .map_err(|e| GameError::engine(&e))?;
        }
        Some(desc) => {
            let mut agents = [
                make_agent(desc, seeds.1[0], false),
                make_agent(desc, seeds.1[1], false),
            ];
            let mut n = 0;
            while s.winner().is_none() && n < max_decisions.min(DECISION_LIMIT) {
                let Some(p) = s.pending().map(|d| d.hdr.player) else {
                    break;
                };
                let a = agents[usize::from(p)]
                    .choose(&s, p, cx)
                    .map_err(GameError::agent)?;
                apply(&mut s, a, true)?;
                n += 1;
            }
        }
    }
    value_of(&s, player)
}

/// Samples the positions of the recording games.
///
/// # Errors
/// A spec problem, a failed game, or the interrupt.
pub fn sample_positions(
    recorder: &AgentDesc,
    selection: &crate::head_to_head::Selection,
    opts: &RunOptions,
    (per_game, decisions): (u32, Decisions),
    cancel: &Cancel,
    progress: &mut dyn FnMut(usize),
) -> Result<Vec<Position>, CommandError> {
    let (specs, _) = stratified_specs(selection, opts.games, opts.seed, recorder, recorder)
        .map_err(CommandError::Invalid)?;
    let keep = usize::try_from(per_game).unwrap_or(0);
    let per = run_indexed(
        specs.len(),
        opts.workers,
        cancel,
        progress,
        |i, cx| {
            let mut s = Sampler {
                keep,
                choose_first: opts.choose_first,
                decisions,
                rng: SplitMix64::new(derive_seed!(opts.seed, "regret", "sample", i)),
                seen: 0,
                slots: Vec::new(),
            };
            play_observed(&specs[i], opts.choose_first, cx, &mut s)?;
            let mut slots = s.slots;
            slots.sort_by_key(|x| x.0);
            Ok(slots)
        },
        |i| (specs[i].index, specs[i].game_seed),
    )
    .map_err(CommandError::from)?;
    let mut out = Vec::new();
    for (g, slots) in per.into_iter().enumerate() {
        for (ordinal, player, kind, target, state) in slots {
            out.push(Position {
                game: u32::try_from(g).unwrap_or(u32::MAX),
                ordinal,
                player,
                kind,
                target,
                state,
            });
        }
    }
    Ok(out)
}

/// The regret of every position, in order (module docs, steps 2-4).
///
/// # Errors
/// A failed agent or playout, or the interrupt.
pub fn evaluate_positions(
    positions: Vec<Position>,
    candidate: &AgentDesc,
    oracle: &AgentDesc,
    judge: Option<&AgentDesc>,
    args: &RegretArgs,
    cancel: &Cancel,
    progress: &mut dyn FnMut(usize),
) -> Result<Vec<(Position, PositionRegret)>, CommandError> {
    let seed = args.opts.seed;
    let n = positions.len();
    let ident: Vec<(u32, u64)> = positions
        .iter()
        .map(|p| (p.game, u64::from(p.ordinal)))
        .collect();
    let states: Mutex<Vec<Option<Position>>> =
        Mutex::new(positions.into_iter().map(Some).collect());
    let results = run_indexed(
        n,
        args.opts.workers,
        cancel,
        progress,
        |i, cx| {
            let pos = states
                .lock()
                .map_err(|_| GameError::Failed {
                    class: "RuntimeError",
                    message: "position store poisoned".to_owned(),
                })?
                .get_mut(i)
                .and_then(Option::take)
                .ok_or_else(|| GameError::Failed {
                    class: "RuntimeError",
                    message: format!("position {i} taken twice"),
                })?;
            let agent_seed = derive_seed!(seed, "regret", "agent", i);
            let mut c = make_agent(candidate, agent_seed, false);
            let mut o = make_agent(oracle, agent_seed, false);
            let ac = c
                .choose(&pos.state, pos.player, cx)
                .map_err(GameError::agent)?;
            let ao = o
                .choose(&pos.state, pos.player, cx)
                .map_err(GameError::agent)?;
            if ac == ao {
                let r = PositionRegret {
                    same: true,
                    regret: 0.0,
                    se: None,
                };
                return Ok((pos, r));
            }
            let mut diffs = Vec::with_capacity(usize::try_from(args.worlds).unwrap_or(0));
            for w in 0..args.worlds {
                let world = observe::determinize(
                    &pos.state,
                    pos.player,
                    derive_seed!(seed, "regret", "world", i, w),
                )
                .map_err(|e| GameError::engine(&e))?;
                let seeds = (
                    derive_seed!(seed, "regret", "rollout", i, w),
                    [
                        derive_seed!(seed, "regret", "judge", i, w, 0),
                        derive_seed!(seed, "regret", "judge", i, w, 1),
                    ],
                );
                let vo = playout(
                    &world,
                    ao,
                    pos.player,
                    judge,
                    seeds,
                    args.temperature,
                    args.max_decisions,
                    cx,
                )?;
                let vc = playout(
                    &world,
                    ac,
                    pos.player,
                    judge,
                    seeds,
                    args.temperature,
                    args.max_decisions,
                    cx,
                )?;
                diffs.push(vo - vc);
            }
            let (regret, se) = mean_se(&diffs);
            Ok((
                pos,
                PositionRegret {
                    same: false,
                    regret,
                    se,
                },
            ))
        },
        |i| ident[i],
    )
    .map_err(CommandError::from)?;
    Ok(results)
}

fn group_json(label: &str, rows: &[&(Position, PositionRegret)]) -> Json {
    let regrets: Vec<f64> = rows.iter().map(|(_, r)| r.regret).collect();
    let (mean, se) = mean_se(&regrets);
    let mut o = Json::object();
    o.set("group", label);
    o.set("positions", rows.len());
    o.set("differing", rows.iter().filter(|(_, r)| !r.same).count());
    o.set(
        "mean_regret",
        (!rows.is_empty()).then(|| round_f64(mean, 4)),
    );
    o.set("se", se.map(|s| round_f64(s, 4)));
    o.set(
        "ci95",
        se.map_or(Json::Null, |s| {
            let h = gcg_runner::reports::stats::Z95 * s;
            Json::Array(vec![
                round_f64(mean - h, 4).into(),
                round_f64(mean + h, 4).into(),
            ])
        }),
    );
    o
}

/// The regret document of evaluated positions.
#[must_use]
pub fn document(args: &RegretArgs, rows: &[(Position, PositionRegret)]) -> Json {
    let mut doc = Json::object();
    doc.set("format", REGRET_FORMAT);
    doc.set("version", REGRET_VERSION);
    doc.set("candidate", args.candidate.as_str());
    doc.set("oracle", args.oracle.as_str());
    doc.set(
        "recorder",
        args.recorder.as_deref().unwrap_or(&args.candidate),
    );
    doc.set(
        "judge",
        match &args.judge {
            Judge::Policy => "policy",
            Judge::Agent(s) => s.as_str(),
        },
    );
    doc.set("temperature", args.temperature);
    doc.set("max_decisions", args.max_decisions);
    doc.set("worlds", args.worlds);
    doc.set("per_game", args.per_game);
    doc.set("decisions", args.decisions.as_str());
    args.opts.describe(&mut doc);
    let all: Vec<&(Position, PositionRegret)> = rows.iter().collect();
    doc.set("all", group_json("all", &all));
    let target: Vec<&(Position, PositionRegret)> = rows.iter().filter(|(p, _)| p.target).collect();
    doc.set("target", group_json("target", &target));
    let mut kinds = Vec::new();
    for k in DecisionKind::ALL {
        let of: Vec<&(Position, PositionRegret)> =
            rows.iter().filter(|(p, _)| p.kind == k).collect();
        if !of.is_empty() {
            kinds.push(group_json(k.as_str(), &of));
        }
    }
    doc.set("by_kind", Json::Array(kinds));
    doc.set(
        "outcomes",
        Json::Array(
            rows.iter()
                .map(|(p, r)| {
                    Json::Array(vec![
                        p.game.into(),
                        p.ordinal.into(),
                        p.kind.as_str().into(),
                        p.target.into(),
                        p.player.into(),
                        r.same.into(),
                        round_f64(r.regret, 6).into(),
                        r.se.map_or(Json::Null, |s| round_f64(s, 6).into()),
                    ])
                })
                .collect(),
        ),
    );
    doc
}

/// Runs `regret`.
///
/// # Errors
/// A setup problem, a failed game or playout, the interrupt, or the output.
pub fn run(
    args: &RegretArgs,
    cancel: &Cancel,
    tty: bool,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<(), CommandError> {
    let invalid = CommandError::Invalid;
    let candidate = parse_agent_spec(&args.candidate).map_err(invalid)?;
    let oracle = parse_agent_spec(&args.oracle).map_err(CommandError::Invalid)?;
    let recorder = match &args.recorder {
        Some(r) => parse_agent_spec(r).map_err(CommandError::Invalid)?,
        None => candidate.clone(),
    };
    let judge = match &args.judge {
        Judge::Policy => None,
        Judge::Agent(s) => Some(parse_agent_spec(s).map_err(CommandError::Invalid)?),
    };
    if args.worlds == 0 || args.per_game == 0 {
        return Err(CommandError::Invalid(
            "--worlds and --per-game must be positive".to_owned(),
        ));
    }
    let selection = prepare(&args.opts).map_err(CommandError::Invalid)?;
    let positions = {
        let total = u64::from(args.opts.games);
        let mut printer = progress_printer(err, tty);
        let mut progress = |d: usize| printer.update(u64::try_from(d).unwrap_or(0), total);
        sample_positions(
            &recorder,
            &selection,
            &args.opts,
            (args.per_game, args.decisions),
            cancel,
            &mut progress,
        )?
    };
    let rows = {
        let total = u64::try_from(positions.len()).unwrap_or(0);
        let mut printer = crate::bench_cmd::ProgressPrinter::new(err, tty, "positions");
        let mut progress = |d: usize| printer.update(u64::try_from(d).unwrap_or(0), total);
        evaluate_positions(
            positions,
            &candidate,
            &oracle,
            judge.as_ref(),
            args,
            cancel,
            &mut progress,
        )?
    };
    let doc = document(args, &rows);
    emit(&doc, args.opts.out.as_ref(), out)?;
    let mut extra = Json::object();
    extra.set("candidate", args.candidate.as_str());
    extra.set("oracle", args.oracle.as_str());
    args.opts.record("regret", extra)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mean_and_paired_standard_error() {
        assert_eq!(mean_se(&[]), (0.0, None));
        assert_eq!(mean_se(&[0.5]), (0.5, None));
        let (m, se) = mean_se(&[1.0, 0.0, 1.0, 0.0]);
        assert_eq!(m, 0.5);
        let want = (1.0_f64 / 3.0 / 4.0).sqrt();
        assert!(se.is_some_and(|s| (s - want).abs() < 1e-12));
    }
}
