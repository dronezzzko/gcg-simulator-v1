//! `gcg-eval weakness MODE` (docs/dev/eval/weakness-gaps.md, GAP-A1: W-05, W-17, W-26): the
//! deterministic weakness measurements. No mode changes a search or a game; each reads states
//! and plays the existing agents unchanged.
//!
//! - `scenarios` (W-17 step 2): the frozen scenarios of `--scenarios` (default
//!   `docs/dev/eval/GAP-A1/weakness/scenarios.json`, written by
//!   `crates/gcg-search/tests/scenarios/gapa1_weakness_scenarios.rs`), one or more per WK
//!   category, scored for each agent of `--agents` over the agent seeds `1..=--seeds`. A
//!   `choice` scenario is solved when the agent's decision is one of `want`; a `turn` scenario
//!   when, after the agent plays its seat until the turn ends (the other seat `ref:standard`
//!   seeded `seed + 100`), its `winner` (a seat, or `"none"`), `zones` and `active` cards hold.
//!   The report gives each scenario's solves and every category's pooled rate with its Wilson
//!   95% interval.
//! - `audit` (W-17 steps 3-4, the deterministic part): every game of the replays under each run
//!   directory of `--runs` (`gcg-sim benchmark` or `gcg-eval reference-benchmark` with
//!   `--replays`, the audit protocol of `scripts/blind_transcripts.py`) is replayed, and at every
//!   decision of a player the rules below flag a play: a *missed lethal* (at a MAIN decision of
//!   the turn player, the tactical solver of `lethal_root` proves a lethal first move in each of
//!   `--worlds` worlds of the decider's information set, `gcg_search::gapa1::proved_lethal`, and
//!   the player does not win that turn; *real* when the solver also proves it on the true
//!   state), a *wasted effect* (a spending play that the reference's no-effect check,
//!   `gcg_ai::without_effect`, finds without effect on the decider's information set), and a
//!   *self-harm target* (a harmful SELECT on one's own card while an enemy card was an option).
//!   Per run: the flags per game, and the games with at least one flag with the Wilson 95%
//!   interval. The blinded human or agent audit of the same games' transcripts is not part of
//!   this mode.
//! - `damage-regret` (W-26): every Damage decision of `--games` self-play games of
//!   `--recorder`, judged with `gcg_search::gapa1::damage_judgement` under the policy
//!   configuration `--policy` (the reference policy against `kill_aware_targeting`, oracle
//!   values over `--worlds` shared worlds): mean regret per Damage decision of both policies,
//!   at temperature 0 and in expectation under the rollout's sampling, and the paired
//!   difference with its normal 95% interval.
//! - `census` (W-05): `--per-game` positions per game of `--games` self-play games of
//!   `--recorder` (`regret`'s sampler), and the observation-key census of one search per
//!   configuration of `--agents` at each (`gcg_search::gapa1::split_census`).
//!
//! Every seed derives from `--seed` and the item's index, so the reports are identical for any
//! `--workers`.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use gcg_ai::targeting::{self, Polarity};
use gcg_core::derive_seed;
use gcg_core::pool::Cancel;
use gcg_core::pyfloat::round_f64;
use gcg_core::pyjson::Json;
use gcg_core::pymath::py_sum;
use gcg_effects::Registry;
use gcg_engine::canon::from_canonical;
use gcg_engine::{
    Action, ActionKind, CtxKey, DecisionKind, GameState, NewGameOptions, StateSnapshot, Uid, Zone,
    apply, new_game_by_number,
};
use gcg_runner::argparse::{
    Action as Arg, ActionKind as ArgKind, Converter, Namespace, Parser, PyInt, PyStr,
    Value as ArgValue,
};
use gcg_search::StrengthenedConfig;
use serde_json::Value;

use crate::AgentDesc;
use crate::agent_spec::{make_agent, parse_agent_spec};
use crate::bench_cmd::{CommandError, ProgressPrinter};
use crate::head_to_head::stratified_specs;
use crate::regret::{Decisions, sample_positions};
use crate::rows::{RunOptions, emit, prepare, progress_printer};
use crate::selfplay::{GameError, Observer, play_observed, run_indexed};
use crate::stats::rate_json;

/// The report format.
pub const WEAKNESS_FORMAT: &str = "gcg-eval-weakness";
pub const WEAKNESS_VERSION: u32 = 1;
/// The default scenario file (relative to the repository root, like the ledger).
pub const DEFAULT_SCENARIOS: &str = "docs/dev/eval/GAP-A1/weakness/scenarios.json";
/// The scenario file format.
pub const SCENARIOS_FORMAT: &str = "gcg-eval-weakness-scenarios";
/// The defender of a `turn` scenario and its seed offset.
pub const DEFENDER: &str = "ref:standard";
pub const DEFENDER_SEED_OFFSET: u64 = 100;
/// The most decisions one `turn` scenario may play before it counts as unsolved.
pub const TURN_DECISIONS: u32 = 400;

/// The mode of a run.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Scenarios,
    Audit,
    DamageRegret,
    Census,
}

impl Mode {
    pub const ALL: [Self; 4] = [
        Self::Scenarios,
        Self::Audit,
        Self::DamageRegret,
        Self::Census,
    ];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Scenarios => "scenarios",
            Self::Audit => "audit",
            Self::DamageRegret => "damage-regret",
            Self::Census => "census",
        }
    }
}

/// The parsed arguments.
#[derive(Clone, PartialEq, Debug)]
pub struct WeaknessArgs {
    pub mode: Mode,
    /// `scenarios`: the agents scored; `census`: the search configurations (strengthened
    /// specs).
    pub agents: Vec<String>,
    pub scenarios: PathBuf,
    pub seeds: u32,
    /// `audit`: the run directories.
    pub runs: Vec<PathBuf>,
    /// `damage-regret`, `census`: the agent whose self-play games give the positions.
    pub recorder: String,
    /// `damage-regret`: the strengthened configuration whose policy settings judge (its
    /// knobs, rollout temperature and weights).
    pub policy: String,
    pub worlds: u32,
    pub per_game: u32,
    pub opts: RunOptions,
}

fn failed(message: String) -> GameError {
    GameError::Failed {
        class: "RuntimeError",
        message,
    }
}

fn engine(e: &gcg_engine::EngineError) -> GameError {
    GameError::engine(e)
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
    let mean = py_sum(xs.iter().copied()) / n as f64;
    if n < 2 {
        return (mean, None);
    }
    let ss = py_sum(xs.iter().map(|x| (x - mean) * (x - mean)));
    let var = ss / (n - 1) as f64;
    (mean, Some(gcg_core::libm::sqrt(var / n as f64)))
}

/// `{mean, se, ci95}` of a sample, rounded to 6 places.
fn mean_block(xs: &[f64]) -> Json {
    let (mean, se) = mean_se(xs);
    let mut o = Json::object();
    o.set("n", xs.len());
    o.set("mean", (!xs.is_empty()).then(|| round_f64(mean, 6)));
    o.set("se", se.map(|s| round_f64(s, 6)));
    o.set(
        "ci95",
        se.map_or(Json::Null, |s| {
            let h = gcg_runner::reports::stats::Z95 * s;
            Json::Array(vec![
                round_f64(mean - h, 6).into(),
                round_f64(mean + h, 6).into(),
            ])
        }),
    );
    o
}

fn search_config(spec: &str) -> Result<StrengthenedConfig, CommandError> {
    match parse_agent_spec(spec).map_err(CommandError::Invalid)? {
        AgentDesc::Search { config } => Ok(config),
        _ => Err(CommandError::Invalid(format!(
            "{spec}: a strengthened search spec (search:...) is needed here"
        ))),
    }
}

// ---------------------------------------------------------------------------------------------
// scenarios

/// A scenario's check.
#[derive(Clone, PartialEq, Debug)]
enum Check {
    Choice(Vec<Action>),
    Turn {
        /// `Some(Some(p))`: seat `p` wins; `Some(None)`: no winner; `None`: either.
        winner: Option<Option<u8>>,
        zones: Vec<(Uid, Zone)>,
        active: Vec<Uid>,
    },
}

/// One frozen scenario.
#[derive(Clone)]
struct Scenario {
    id: String,
    category: String,
    player: u8,
    check: Check,
    state: GameState,
}

fn bad(path: &Path, detail: impl std::fmt::Display) -> CommandError {
    CommandError::Invalid(format!("{}: {detail}", path.display()))
}

fn action_of(v: &Value) -> Option<Action> {
    let a = v.as_array()?;
    let kind = ActionKind::from_value(a.first()?.as_str()?)?;
    let arg = |i: usize| a.get(i)?.as_i64().and_then(|x| i32::try_from(x).ok());
    Some(Action::new(kind, arg(1)?, arg(2)?, arg(3)?))
}

fn uid_of(v: &Value) -> Option<Uid> {
    v.as_u64().and_then(|x| Uid::try_from(x).ok())
}

fn check_of(v: &Value) -> Option<Check> {
    match v.get("kind")?.as_str()? {
        "choice" => Some(Check::Choice(
            v.get("want")?
                .as_array()?
                .iter()
                .map(action_of)
                .collect::<Option<Vec<_>>>()?,
        )),
        "turn" => {
            let winner = match v.get("winner") {
                None | Some(Value::Null) => None,
                Some(Value::String(s)) if s == "none" => Some(None),
                Some(w) => Some(Some(u8::try_from(w.as_u64()?).ok()?)),
            };
            let zones = v
                .get("zones")?
                .as_array()?
                .iter()
                .map(|z| {
                    let z = z.as_array()?;
                    let name = z.get(1)?.as_str()?;
                    let zone = Zone::ALL.into_iter().find(|x| x.py_name() == name)?;
                    Some((uid_of(z.first()?)?, zone))
                })
                .collect::<Option<Vec<_>>>()?;
            let active = v
                .get("active")?
                .as_array()?
                .iter()
                .map(uid_of)
                .collect::<Option<Vec<_>>>()?;
            Some(Check::Turn {
                winner,
                zones,
                active,
            })
        }
        _ => None,
    }
}

fn load_scenarios(path: &Path) -> Result<Vec<Scenario>, CommandError> {
    let text = std::fs::read_to_string(path).map_err(|e| bad(path, e))?;
    let doc: Value = serde_json::from_str(&text).map_err(|e| bad(path, e))?;
    if doc.get("format").and_then(Value::as_str) != Some(SCENARIOS_FORMAT) {
        return Err(bad(path, format!("not a {SCENARIOS_FORMAT} file")));
    }
    let reg = Registry::embedded();
    let items = doc
        .get("scenarios")
        .and_then(Value::as_array)
        .ok_or_else(|| bad(path, "no scenarios"))?;
    let mut out = Vec::with_capacity(items.len());
    for s in items {
        let field = |k: &str| {
            s.get(k)
                .ok_or_else(|| bad(path, format!("a scenario has no {k}")))
        };
        let id = field("id")?.as_str().unwrap_or_default().to_owned();
        let snap = field("snapshot")?;
        let state = from_canonical(
            &reg,
            &StateSnapshot {
                state: snap["state"].clone(),
                continuous: snap["continuous"].clone(),
                filter_sets: snap["filter_sets"].clone(),
                rules_checked: snap["rules_checked"].as_i64().unwrap_or(0),
            },
        )
        .map_err(|e| bad(path, format!("{id}: {e:?}")))?;
        let check =
            check_of(field("check")?).ok_or_else(|| bad(path, format!("{id}: a bad check")))?;
        let player = field("player")?
            .as_u64()
            .and_then(|p| u8::try_from(p).ok())
            .filter(|&p| p < 2)
            .ok_or_else(|| bad(path, format!("{id}: a bad player")))?;
        if state.pending().map(|d| d.hdr.player) != Some(player) {
            return Err(bad(path, format!("{id}: the player is not to decide")));
        }
        out.push(Scenario {
            id,
            category: field("category")?.as_str().unwrap_or_default().to_owned(),
            player,
            check,
            state,
        });
    }
    Ok(out)
}

/// Whether `agent` (seed `seed`) solves `sc`.
fn solves(
    sc: &Scenario,
    agent: &AgentDesc,
    defender: &AgentDesc,
    seed: u64,
    cx: &mut gcg_ai::AgentCx<'_>,
) -> Result<bool, GameError> {
    let mut me = make_agent(agent, seed, false);
    match &sc.check {
        Check::Choice(want) => {
            let a = me
                .choose(&sc.state, sc.player, cx)
                .map_err(GameError::agent)?;
            Ok(want.contains(&a))
        }
        Check::Turn {
            winner,
            zones,
            active,
        } => {
            let mut other = make_agent(defender, seed + DEFENDER_SEED_OFFSET, false);
            let mut s = sc.state.clone();
            let turn = s.turn();
            let mut n = 0;
            while s.winner().is_none() && s.turn() == turn {
                let Some(p) = s.pending().map(|d| d.hdr.player) else {
                    break;
                };
                if n >= TURN_DECISIONS {
                    return Ok(false);
                }
                let a = if p == sc.player {
                    me.choose(&s, p, cx)
                } else {
                    other.choose(&s, p, cx)
                }
                .map_err(GameError::agent)?;
                apply(&mut s, a, true)?;
                n += 1;
            }
            let won = s.winner().and_then(|w| u8::try_from(w).ok());
            let winner_ok = match winner {
                None => true,
                Some(None) => s.winner().is_none(),
                Some(Some(p)) => won == Some(*p),
            };
            Ok(winner_ok
                && zones.iter().all(|&(u, z)| s.card(u).zone == z)
                && active.iter().all(|&u| !s.card(u).rested))
        }
    }
}

fn run_scenarios(
    args: &WeaknessArgs,
    cancel: &Cancel,
    err: &mut dyn Write,
    tty: bool,
) -> Result<Json, CommandError> {
    let scenarios = load_scenarios(&args.scenarios)?;
    let agents: Vec<AgentDesc> = args
        .agents
        .iter()
        .map(|s| parse_agent_spec(s).map_err(CommandError::Invalid))
        .collect::<Result<_, _>>()?;
    let defender = parse_agent_spec(DEFENDER).map_err(CommandError::Invalid)?;
    let seeds = u64::from(args.seeds);
    let jobs: Vec<(usize, usize, u64)> = (0..scenarios.len())
        .flat_map(|s| (0..agents.len()).flat_map(move |a| (1..=seeds).map(move |k| (s, a, k))))
        .collect();
    let total = u64::try_from(jobs.len()).unwrap_or(0);
    let mut printer = ProgressPrinter::new(err, tty, "scenario seeds");
    let mut progress = |d: usize| printer.update(u64::try_from(d).unwrap_or(0), total);
    let store = Mutex::new(scenarios.clone());
    let results = run_indexed(
        jobs.len(),
        args.opts.workers,
        cancel,
        &mut progress,
        |i, cx| {
            let (s, a, k) = jobs[i];
            let sc = store
                .lock()
                .map_err(|_| failed("scenario store poisoned".into()))?[s]
                .clone();
            solves(&sc, &agents[a], &defender, k, cx)
        },
        |i| (u32::try_from(i).unwrap_or(u32::MAX), jobs[i].2),
    )
    .map_err(CommandError::from)?;
    let solved = |s: usize, a: usize| -> u64 {
        let n = jobs
            .iter()
            .zip(&results)
            .filter(|((js, ja, _), ok)| *js == s && *ja == a && **ok)
            .count();
        u64::try_from(n).unwrap_or(u64::MAX)
    };
    let count = |n: usize| u64::try_from(n).unwrap_or(u64::MAX);
    let mut doc = Json::object();
    doc.set("format", WEAKNESS_FORMAT);
    doc.set("version", WEAKNESS_VERSION);
    doc.set("mode", Mode::Scenarios.as_str());
    doc.set("scenarios_file", args.scenarios.display().to_string());
    doc.set(
        "agents",
        Json::Array(args.agents.iter().map(|s| s.as_str().into()).collect()),
    );
    doc.set("seeds", args.seeds);
    doc.set("defender", DEFENDER);
    let mut rows = Vec::new();
    for (s, sc) in scenarios.iter().enumerate() {
        let mut o = Json::object();
        o.set("id", sc.id.as_str());
        o.set("category", sc.category.as_str());
        let mut by = Json::object();
        for (a, name) in args.agents.iter().enumerate() {
            by.set(name.as_str(), solved(s, a));
        }
        o.set("solved", by);
        rows.push(o);
    }
    doc.set("scenarios", Json::Array(rows));
    let mut categories: Vec<&str> = scenarios.iter().map(|s| s.category.as_str()).collect();
    categories.sort();
    categories.dedup();
    let mut cats = Vec::new();
    for c in &categories {
        let mut o = Json::object();
        o.set("category", *c);
        let members: Vec<usize> = (0..scenarios.len())
            .filter(|&s| scenarios[s].category == *c)
            .collect();
        o.set("scenarios", members.len());
        let mut by = Json::object();
        for (a, name) in args.agents.iter().enumerate() {
            let wins: u64 = members.iter().map(|&s| solved(s, a)).sum();
            by.set(name.as_str(), rate_json(wins, seeds * count(members.len())));
        }
        o.set("rate", by);
        cats.push(o);
    }
    doc.set("categories", Json::Array(cats));
    let mut all = Json::object();
    for (a, name) in args.agents.iter().enumerate() {
        let wins: u64 = (0..scenarios.len()).map(|s| solved(s, a)).sum();
        all.set(
            name.as_str(),
            rate_json(wins, seeds * count(scenarios.len())),
        );
    }
    doc.set("all", all);
    Ok(doc)
}

// ---------------------------------------------------------------------------------------------
// audit

/// The flags of one game.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct GameFlags {
    file: String,
    decisions: u32,
    missed_lethal: Vec<(u8, i32, bool)>,
    wasted_effect: Vec<(u8, i32, String)>,
    self_harm_target: Vec<(u8, i32)>,
}

impl GameFlags {
    fn count(&self) -> usize {
        self.missed_lethal.len() + self.wasted_effect.len() + self.self_harm_target.len()
    }
}

fn replay_files(dir: &Path) -> Result<Vec<PathBuf>, CommandError> {
    let replays = dir.join("replays");
    let mut out = Vec::new();
    let entries = std::fs::read_dir(&replays).map_err(|e| bad(&replays, e))?;
    for e in entries {
        let p = e.map_err(|e| bad(&replays, e))?.path();
        if p.extension().is_some_and(|x| x == "json") {
            out.push(p);
        }
    }
    out.sort();
    Ok(out)
}

/// The state at the start of a replay document (`check_replay`'s setup) and its actions.
fn replay_start(path: &Path) -> Result<(GameState, Vec<Action>), CommandError> {
    let text = std::fs::read_to_string(path).map_err(|e| bad(path, e))?;
    let doc: Value = serde_json::from_str(&text).map_err(|e| bad(path, e))?;
    let mut seats: Vec<&Value> = doc
        .get("seats")
        .and_then(Value::as_array)
        .ok_or_else(|| bad(path, "no seats"))?
        .iter()
        .collect();
    seats.sort_by_key(|s| s.get("seat").and_then(Value::as_i64).unwrap_or(0));
    let mut decks: Vec<Vec<String>> = Vec::new();
    for s in &seats {
        for side in ["main", "resources"] {
            let counts = s
                .get(side)
                .and_then(Value::as_object)
                .ok_or_else(|| bad(path, "a seat without counts"))?;
            let mut cards: Vec<String> = Vec::new();
            for (number, c) in counts {
                let n = c.as_u64().unwrap_or(0);
                cards.extend(std::iter::repeat_n(
                    number.clone(),
                    usize::try_from(n).unwrap_or(0),
                ));
            }
            cards.sort();
            decks.push(cards);
        }
    }
    if decks.len() != 4 {
        return Err(bad(path, "two seats expected"));
    }
    let refs: Vec<Vec<&str>> = decks
        .iter()
        .map(|d| d.iter().map(String::as_str).collect())
        .collect();
    let int = |k: &str| doc.get(k).and_then(Value::as_i64);
    let opts = NewGameOptions {
        chooser: int("chooser").and_then(|c| u8::try_from(c).ok()),
        turn_limit: int("turn_limit")
            .and_then(|x| i32::try_from(x).ok())
            .ok_or_else(|| bad(path, "no turn_limit"))?,
        max_actions: int("max_actions").ok_or_else(|| bad(path, "no max_actions"))?,
        validate: true,
    };
    let seed = doc
        .get("seed")
        .and_then(Value::as_u64)
        .ok_or_else(|| bad(path, "no seed"))?;
    let st = new_game_by_number(
        &Registry::embedded(),
        [(&refs[0], &refs[1]), (&refs[2], &refs[3])],
        seed,
        opts,
    )
    .map_err(|e| bad(path, format!("{e:?}")))?;
    let actions = doc
        .get("actions")
        .and_then(Value::as_array)
        .ok_or_else(|| bad(path, "no actions"))?
        .iter()
        .map(action_of)
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| bad(path, "a bad action"))?;
    Ok((st, actions))
}

/// Whether `a` at `st` is a harmful SELECT on the decider's own card while an enemy card was an
/// option.
fn self_harm(st: &GameState, player: u8, a: Action) -> bool {
    let Some(dec) = st.pending() else {
        return false;
    };
    if dec.hdr.kind != DecisionKind::Select
        || dec.hdr.ctx(CtxKey::Mode) == 1
        || a.kind != ActionKind::Select
        || targeting::choice_polarity(st, true) != Polarity::Harms
    {
        return false;
    }
    let owner = |x: i32| {
        usize::try_from(x)
            .ok()
            .filter(|&u| u < st.cards().len())
            .map(|u| st.cards()[u].owner)
    };
    owner(a.a) == Some(player)
        && dec
            .options
            .iter()
            .any(|o| o.kind == ActionKind::Select && owner(o.a) == Some(1 - player))
}

fn audit_game(
    path: &Path,
    seed: u64,
    worlds: u32,
    cfg: &StrengthenedConfig,
) -> Result<GameFlags, GameError> {
    let (mut st, actions) = replay_start(path).map_err(|e| match e {
        CommandError::Invalid(m) => failed(m),
        CommandError::Interrupted => GameError::Interrupted,
    })?;
    let mut flags = GameFlags {
        file: path
            .file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_default(),
        ..GameFlags::default()
    };
    let mut lethal_turns: Vec<(u8, i32, bool)> = Vec::new();
    for (n, &a) in actions.iter().enumerate() {
        let Some(dec) = st.pending() else {
            return Err(failed(format!("{}: an action after the end", flags.file)));
        };
        let p = dec.hdr.player;
        if dec.options.len() > 1 {
            flags.decisions += 1;
            let ni = i64::try_from(n).unwrap_or(i64::MAX);
            if dec.hdr.kind == DecisionKind::Main
                && p == st.active()
                && !lethal_turns
                    .iter()
                    .any(|&(q, t, _)| q == p && t == st.turn())
            {
                let seeds: Vec<u64> = (0..worlds)
                    .map(|k| derive_seed!(seed, "weakness", "audit", ni, i64::from(k)))
                    .collect();
                if gcg_search::gapa1::proved_lethal(&st, p, cfg, &seeds)
                    .map_err(|e| engine(&e))?
                    .is_some()
                {
                    let real = gcg_search::gapa1::lethal_on_state(&st, p, cfg)
                        .map_err(|e| engine(&e))?
                        .is_some();
                    lethal_turns.push((p, st.turn(), real));
                }
            }
            if gcg_search::gapa1::without_effect(
                &st,
                p,
                a,
                cfg,
                derive_seed!(seed, "weakness", "noop", ni),
            )
            .map_err(|e| engine(&e))?
            {
                flags
                    .wasted_effect
                    .push((p, st.turn(), a.kind.as_str().to_owned()));
            }
            if self_harm(&st, p, a) {
                flags.self_harm_target.push((p, st.turn()));
            }
        }
        apply(&mut st, a, true)?;
    }
    let winner = st.winner().and_then(|w| u8::try_from(w).ok());
    let last = st.turn();
    for (p, t, real) in lethal_turns {
        if !(winner == Some(p) && last == t) {
            flags.missed_lethal.push((p, t, real));
        }
    }
    Ok(flags)
}

fn run_audit(
    args: &WeaknessArgs,
    cancel: &Cancel,
    err: &mut dyn Write,
    tty: bool,
) -> Result<Json, CommandError> {
    if args.runs.is_empty() {
        return Err(CommandError::Invalid("audit needs --runs".to_owned()));
    }
    if let Some(role) = args.opts.role {
        crate::ledger::check_master(role, args.opts.seed).map_err(CommandError::Invalid)?;
    }
    let cfg = search_config(&args.policy)?;
    let mut files: Vec<(usize, PathBuf)> = Vec::new();
    for (r, dir) in args.runs.iter().enumerate() {
        for f in replay_files(dir)? {
            files.push((r, f));
        }
    }
    let total = u64::try_from(files.len()).unwrap_or(0);
    let mut printer = ProgressPrinter::new(err, tty, "games");
    let mut progress = |d: usize| printer.update(u64::try_from(d).unwrap_or(0), total);
    let seed = args.opts.seed;
    let results = run_indexed(
        files.len(),
        args.opts.workers,
        cancel,
        &mut progress,
        |i, _cx| {
            let gi = i64::try_from(i).unwrap_or(i64::MAX);
            audit_game(
                &files[i].1,
                derive_seed!(seed, "weakness", "game", gi),
                args.worlds,
                &cfg,
            )
        },
        |i| (u32::try_from(i).unwrap_or(u32::MAX), seed),
    )
    .map_err(CommandError::from)?;
    let mut doc = Json::object();
    doc.set("format", WEAKNESS_FORMAT);
    doc.set("version", WEAKNESS_VERSION);
    doc.set("mode", Mode::Audit.as_str());
    doc.set("seed", seed);
    doc.set("worlds", args.worlds);
    doc.set("policy", args.policy.as_str());
    let mut runs = Vec::new();
    for (r, dir) in args.runs.iter().enumerate() {
        let games: Vec<&GameFlags> = files
            .iter()
            .zip(&results)
            .filter(|((fr, _), _)| *fr == r)
            .map(|(_, g)| g)
            .collect();
        let n = u64::try_from(games.len()).unwrap_or(u64::MAX);
        let flagged =
            u64::try_from(games.iter().filter(|g| g.count() > 0).count()).unwrap_or(u64::MAX);
        let per_game: Vec<f64> = games
            .iter()
            .map(|g| f64::from(u32::try_from(g.count()).unwrap_or(u32::MAX)))
            .collect();
        let sum = |f: &dyn Fn(&GameFlags) -> usize| -> usize { games.iter().map(|g| f(g)).sum() };
        let mut o = Json::object();
        o.set("run", dir.display().to_string());
        o.set("games", n);
        o.set(
            "decisions",
            sum(&|g| usize::try_from(g.decisions).unwrap_or(usize::MAX)),
        );
        o.set("missed_lethal", sum(&|g| g.missed_lethal.len()));
        o.set(
            "missed_lethal_real",
            sum(&|g| g.missed_lethal.iter().filter(|m| m.2).count()),
        );
        o.set("wasted_effect", sum(&|g| g.wasted_effect.len()));
        o.set("self_harm_target", sum(&|g| g.self_harm_target.len()));
        o.set("flags_per_game", mean_block(&per_game));
        o.set("games_with_a_flag", rate_json(flagged, n));
        let mut list = Vec::new();
        for g in &games {
            if g.count() == 0 {
                continue;
            }
            let mut x = Json::object();
            x.set("file", g.file.as_str());
            x.set(
                "missed_lethal",
                Json::Array(
                    g.missed_lethal
                        .iter()
                        .map(|&(p, t, real)| Json::Array(vec![p.into(), t.into(), real.into()]))
                        .collect(),
                ),
            );
            x.set(
                "wasted_effect",
                Json::Array(
                    g.wasted_effect
                        .iter()
                        .map(|(p, t, k)| {
                            Json::Array(vec![(*p).into(), (*t).into(), k.as_str().into()])
                        })
                        .collect(),
                ),
            );
            x.set(
                "self_harm_target",
                Json::Array(
                    g.self_harm_target
                        .iter()
                        .map(|&(p, t)| Json::Array(vec![p.into(), t.into()]))
                        .collect(),
                ),
            );
            list.push(x);
        }
        o.set("flagged_games", Json::Array(list));
        runs.push(o);
    }
    doc.set("runs", Json::Array(runs));
    Ok(doc)
}

// ---------------------------------------------------------------------------------------------
// damage-regret

/// Keeps every Damage decision of a game.
struct DamageObserver<'c> {
    cfg: &'c StrengthenedConfig,
    kept: Vec<(u8, GameState)>,
}

impl Observer for DamageObserver<'_> {
    fn before(&mut self, st: &GameState, player: u8) -> Result<(), GameError> {
        if st.pending().is_some_and(|d| d.hdr.player == player)
            && gcg_search::gapa1::is_damage_decision(st, self.cfg)
        {
            self.kept.push((player, st.clone()));
        }
        Ok(())
    }
}

fn run_damage_regret(
    args: &WeaknessArgs,
    cancel: &Cancel,
    err: &mut dyn Write,
    tty: bool,
) -> Result<Json, CommandError> {
    let cfg = search_config(&args.policy)?;
    let recorder = parse_agent_spec(&args.recorder).map_err(CommandError::Invalid)?;
    let selection = prepare(&args.opts).map_err(CommandError::Invalid)?;
    let (specs, _) = stratified_specs(
        &selection,
        args.opts.games,
        args.opts.seed,
        &recorder,
        &recorder,
    )
    .map_err(CommandError::Invalid)?;
    let positions = {
        let total = u64::from(args.opts.games);
        let mut printer = progress_printer(err, tty);
        let mut progress = |d: usize| printer.update(u64::try_from(d).unwrap_or(0), total);
        run_indexed(
            specs.len(),
            args.opts.workers,
            cancel,
            &mut progress,
            |i, cx| {
                let mut o = DamageObserver {
                    cfg: &cfg,
                    kept: Vec::new(),
                };
                play_observed(&specs[i], args.opts.choose_first, cx, &mut o)?;
                Ok(o.kept)
            },
            |i| (specs[i].index, specs[i].game_seed),
        )
        .map_err(CommandError::from)?
    };
    let mut flat: Vec<(u32, u32, u8)> = Vec::new();
    let mut states: Vec<Option<GameState>> = Vec::new();
    for (g, kept) in positions.into_iter().enumerate() {
        for (k, (p, s)) in kept.into_iter().enumerate() {
            flat.push((
                u32::try_from(g).unwrap_or(u32::MAX),
                u32::try_from(k).unwrap_or(u32::MAX),
                p,
            ));
            states.push(Some(s));
        }
    }
    let seed = args.opts.seed;
    let store: Mutex<Vec<Option<GameState>>> = Mutex::new(states);
    let judged = {
        let total = u64::try_from(flat.len()).unwrap_or(0);
        let mut printer = ProgressPrinter::new(err, tty, "positions");
        let mut progress = |d: usize| printer.update(u64::try_from(d).unwrap_or(0), total);
        run_indexed(
            flat.len(),
            args.opts.workers,
            cancel,
            &mut progress,
            |i, _cx| {
                let st = store
                    .lock()
                    .map_err(|_| failed("position store poisoned".into()))?
                    .get_mut(i)
                    .and_then(Option::take)
                    .ok_or_else(|| failed(format!("position {i} taken twice")))?;
                let ii = i64::try_from(i).unwrap_or(i64::MAX);
                gcg_search::gapa1::damage_judgement(
                    &st,
                    flat[i].2,
                    &cfg,
                    args.worlds,
                    derive_seed!(seed, "weakness", "damage", ii),
                )
                .map_err(|e| engine(&e))?
                .ok_or_else(|| failed(format!("position {i} is no Damage decision")))
            },
            |i| (flat[i].0, u64::from(flat[i].1)),
        )
        .map_err(CommandError::from)?
    };
    let shared: Vec<f64> = judged.iter().map(|j| j.regret(j.shared_pick)).collect();
    let kill: Vec<f64> = judged.iter().map(|j| j.regret(j.kill_pick)).collect();
    let shared_e: Vec<f64> = judged
        .iter()
        .map(|j| j.expected_regret(&j.shared_probs))
        .collect();
    let kill_e: Vec<f64> = judged
        .iter()
        .map(|j| j.expected_regret(&j.kill_probs))
        .collect();
    let diff: Vec<f64> = kill.iter().zip(&shared).map(|(k, s)| k - s).collect();
    let diff_e: Vec<f64> = kill_e.iter().zip(&shared_e).map(|(k, s)| k - s).collect();
    let mut doc = Json::object();
    doc.set("format", WEAKNESS_FORMAT);
    doc.set("version", WEAKNESS_VERSION);
    doc.set("mode", Mode::DamageRegret.as_str());
    doc.set("recorder", args.recorder.as_str());
    doc.set("policy", args.policy.as_str());
    doc.set("worlds", args.worlds);
    doc.set(
        "max_decisions",
        u64::from(gcg_search::gapa1::ORACLE_MAX_DECISIONS),
    );
    args.opts.describe(&mut doc);
    doc.set("damage_decisions", judged.len());
    doc.set(
        "differing_picks",
        judged
            .iter()
            .filter(|j| j.shared_pick != j.kill_pick)
            .count(),
    );
    doc.set("regret_reference_policy", mean_block(&shared));
    doc.set("regret_kill_aware", mean_block(&kill));
    doc.set("regret_kill_aware_minus_reference", mean_block(&diff));
    doc.set("expected_regret_reference_policy", mean_block(&shared_e));
    doc.set("expected_regret_kill_aware", mean_block(&kill_e));
    doc.set(
        "expected_regret_kill_aware_minus_reference",
        mean_block(&diff_e),
    );
    doc.set(
        "positions",
        Json::Array(
            flat.iter()
                .zip(&judged)
                .map(|((g, k, p), j)| {
                    Json::Array(vec![
                        (*g).into(),
                        (*k).into(),
                        (*p).into(),
                        j.amount.into(),
                        j.options.len().into(),
                        j.shared_pick.into(),
                        j.kill_pick.into(),
                        round_f64(j.regret(j.shared_pick), 6).into(),
                        round_f64(j.regret(j.kill_pick), 6).into(),
                    ])
                })
                .collect(),
        ),
    );
    Ok(doc)
}

// ---------------------------------------------------------------------------------------------
// census

fn run_census(
    args: &WeaknessArgs,
    cancel: &Cancel,
    err: &mut dyn Write,
    tty: bool,
) -> Result<Json, CommandError> {
    let configs: Vec<StrengthenedConfig> = args
        .agents
        .iter()
        .map(|s| search_config(s))
        .collect::<Result<_, _>>()?;
    let recorder = parse_agent_spec(&args.recorder).map_err(CommandError::Invalid)?;
    let selection = prepare(&args.opts).map_err(CommandError::Invalid)?;
    let positions = {
        let total = u64::from(args.opts.games);
        let mut printer = progress_printer(err, tty);
        let mut progress = |d: usize| printer.update(u64::try_from(d).unwrap_or(0), total);
        sample_positions(
            &recorder,
            &selection,
            &args.opts,
            (args.per_game, Decisions::All),
            cancel,
            &mut progress,
        )?
    };
    let seed = args.opts.seed;
    let jobs: Vec<(usize, usize)> = (0..positions.len())
        .flat_map(|p| (0..configs.len()).map(move |c| (p, c)))
        .collect();
    let meta: Vec<(u32, u32, DecisionKind, u8)> = positions
        .iter()
        .map(|p| (p.game, p.ordinal, p.kind, p.player))
        .collect();
    let store: Mutex<Vec<GameState>> = Mutex::new(positions.into_iter().map(|p| p.state).collect());
    let results = {
        let total = u64::try_from(jobs.len()).unwrap_or(0);
        let mut printer = ProgressPrinter::new(err, tty, "searches");
        let mut progress = |d: usize| printer.update(u64::try_from(d).unwrap_or(0), total);
        run_indexed(
            jobs.len(),
            args.opts.workers,
            cancel,
            &mut progress,
            |i, cx| {
                let (p, c) = jobs[i];
                let state = store
                    .lock()
                    .map_err(|_| failed("position store poisoned".into()))?[p]
                    .clone();
                let pi = i64::try_from(p).unwrap_or(i64::MAX);
                gcg_search::gapa1::split_census(
                    &state,
                    meta[p].3,
                    &configs[c],
                    derive_seed!(seed, "weakness", "census", pi),
                    cx,
                )
                .map(|(census, _)| census)
                .map_err(GameError::agent)
            },
            |i| (u32::try_from(jobs[i].0).unwrap_or(u32::MAX), seed),
        )
        .map_err(CommandError::from)?
    };
    let mut doc = Json::object();
    doc.set("format", WEAKNESS_FORMAT);
    doc.set("version", WEAKNESS_VERSION);
    doc.set("mode", Mode::Census.as_str());
    doc.set("recorder", args.recorder.as_str());
    doc.set("per_game", args.per_game);
    args.opts.describe(&mut doc);
    doc.set("positions", meta.len());
    let mut out = Vec::new();
    for (c, name) in args.agents.iter().enumerate() {
        let of: Vec<&gcg_search::gapa1::SplitCensus> = jobs
            .iter()
            .zip(&results)
            .filter(|((_, jc), _)| *jc == c)
            .map(|(_, r)| r)
            .collect();
        let mut causes: Vec<(String, u32)> = Vec::new();
        for r in &of {
            for (cause, n) in &r.causes {
                match causes.iter_mut().find(|(x, _)| x == cause) {
                    Some((_, m)) => *m += n,
                    None => causes.push((cause.clone(), *n)),
                }
            }
        }
        causes.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        let mut o = Json::object();
        o.set("config", name.as_str());
        o.set(
            "iterations",
            of.iter().map(|r| u64::from(r.iterations)).sum::<u64>(),
        );
        o.set(
            "searcher_nodes",
            of.iter().map(|r| u64::from(r.searcher_nodes)).sum::<u64>(),
        );
        o.set(
            "splits",
            of.iter().map(|r| u64::from(r.splits)).sum::<u64>(),
        );
        o.set(
            "positions_with_splits",
            of.iter().filter(|r| r.splits > 0).count(),
        );
        let mut cj = Json::object();
        for (cause, n) in &causes {
            cj.set(cause.as_str(), *n);
        }
        o.set("causes", cj);
        o.set(
            "per_position",
            Json::Array(
                of.iter()
                    .zip(&meta)
                    .map(|(r, &(game, ordinal, kind, _))| {
                        Json::Array(vec![
                            game.into(),
                            ordinal.into(),
                            kind.as_str().into(),
                            r.iterations.into(),
                            r.searcher_nodes.into(),
                            r.splits.into(),
                        ])
                    })
                    .collect(),
            ),
        );
        out.push(o);
    }
    doc.set("configs", Json::Array(out));
    Ok(doc)
}

/// The help line of `weakness`.
pub const HELP: &str =
    "deterministic weakness measurements: scenarios, audit, damage-regret, census";
/// The default agents of `scenarios`.
pub const DEFAULT_AGENTS: &str = "ref:standard,search:fast,search:standard";
/// The default configurations of `census`: the reference `standard` budget with every mechanism
/// off, with and without `observation_keys` (the negative control).
pub const DEFAULT_CENSUS: &str = "search:null-standard,search:null-standard:observation_keys=false";

fn text_of(s: &PyStr) -> String {
    s.to_string_opt()
        .unwrap_or_else(|| String::from_utf8_lossy(&s.to_surrogateescape_bytes()).into_owned())
}

fn ns_text(ns: &Namespace, dest: &str) -> Option<String> {
    match ns.get(dest) {
        Some(ArgValue::Str(s) | ArgValue::Path(s)) => Some(text_of(s)),
        _ => None,
    }
}

fn ns_u32(ns: &Namespace, dest: &str) -> u32 {
    match ns.get(dest) {
        Some(ArgValue::Int(v)) => v
            .to_u64()
            .and_then(|x| u32::try_from(x).ok())
            .unwrap_or(u32::MAX),
        _ => 0,
    }
}

fn list(text: &str) -> Vec<String> {
    text.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

/// The options of `weakness` before the shared run options (`cli::push_run_options`).
#[must_use]
pub fn parser() -> Parser {
    let mut p = Parser::new(
        "gcg-eval weakness",
        Some(
            "Deterministic weakness measurements (W-05, W-17, W-26): the frozen WK scenarios, \
             the rule-based audit of recorded games, the policy-level Damage regret and the \
             observation-key split census.",
        ),
    );
    p.push(Arg {
        option_strings: Vec::new(),
        dest: Some("mode"),
        kind: ArgKind::Store {
            convert: Converter::Str,
            choices: Some(Mode::ALL.iter().map(|m| m.as_str()).collect()),
        },
        required: true,
        help: Some("scenarios, audit, damage-regret or census".to_owned()),
        default: Some(ArgValue::None),
    });
    let text = |option, dest, default: Option<&str>, help: &str| {
        Arg::option(
            option,
            dest,
            Converter::Str,
            None,
            default.map_or(ArgValue::None, |d| ArgValue::Str(PyStr::from(d))),
            help.to_owned(),
        )
    };
    let int = |option, dest, default: u64, help: &str| {
        Arg::option(
            option,
            dest,
            Converter::PositiveInt,
            None,
            ArgValue::Int(PyInt::from_u64(default)),
            format!("{help} (default {default})"),
        )
    };
    p.push(text(
        "--agents",
        "agents",
        None,
        "comma-separated agent specs: scenarios scores them (default \
         ref:standard,search:fast,search:standard); census runs these search configurations \
         (default search:null-standard and the same with observation_keys=false)",
    ));
    p.push(text(
        "--scenarios",
        "scenarios",
        Some(DEFAULT_SCENARIOS),
        "scenarios: the frozen scenario file",
    ));
    p.push(int(
        "--seeds",
        "seeds",
        16,
        "scenarios: agent seeds per scenario and agent",
    ));
    p.push(text(
        "--runs",
        "runs",
        None,
        "audit: comma-separated run directories with a replays/ directory",
    ));
    p.push(text(
        "--recorder",
        "recorder",
        Some("ref:standard"),
        "damage-regret, census: the agent whose self-play games give the positions (default \
         ref:standard)",
    ));
    p.push(text(
        "--policy",
        "policy",
        Some("search:standard"),
        "audit, damage-regret: the strengthened configuration whose solver, policy settings \
         and weights judge (default search:standard)",
    ));
    p.push(int(
        "--worlds",
        "worlds",
        8,
        "audit: worlds of a proved lethal; damage-regret: shared oracle worlds",
    ));
    p.push(int(
        "--per-game",
        "per_game",
        3,
        "census: positions sampled per game",
    ));
    p
}

/// The typed `weakness` arguments.
#[must_use]
pub fn args(ns: &Namespace, opts: RunOptions) -> WeaknessArgs {
    let mode = ns_text(ns, "mode")
        .and_then(|m| Mode::ALL.into_iter().find(|x| x.as_str() == m))
        .unwrap_or(Mode::Scenarios);
    let default_agents = match mode {
        Mode::Census => DEFAULT_CENSUS,
        _ => DEFAULT_AGENTS,
    };
    WeaknessArgs {
        mode,
        agents: list(&ns_text(ns, "agents").unwrap_or_else(|| default_agents.to_owned())),
        scenarios: PathBuf::from(
            ns_text(ns, "scenarios").unwrap_or_else(|| DEFAULT_SCENARIOS.to_owned()),
        ),
        seeds: ns_u32(ns, "seeds"),
        runs: ns_text(ns, "runs")
            .map(|r| list(&r).into_iter().map(PathBuf::from).collect())
            .unwrap_or_default(),
        recorder: ns_text(ns, "recorder").unwrap_or_else(|| "ref:standard".to_owned()),
        policy: ns_text(ns, "policy").unwrap_or_else(|| "search:standard".to_owned()),
        worlds: ns_u32(ns, "worlds"),
        per_game: ns_u32(ns, "per_game"),
        opts,
    }
}

/// Runs `weakness`.
///
/// # Errors
/// A setup problem, a failed game or search, the interrupt, or the output.
pub fn run(
    args: &WeaknessArgs,
    cancel: &Cancel,
    tty: bool,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<(), CommandError> {
    if args.seeds == 0 || args.worlds == 0 || args.per_game == 0 {
        return Err(CommandError::Invalid(
            "--seeds, --worlds and --per-game must be positive".to_owned(),
        ));
    }
    let doc = match args.mode {
        Mode::Scenarios => run_scenarios(args, cancel, err, tty)?,
        Mode::Audit => run_audit(args, cancel, err, tty)?,
        Mode::DamageRegret => run_damage_regret(args, cancel, err, tty)?,
        Mode::Census => run_census(args, cancel, err, tty)?,
    };
    emit(&doc, args.opts.out.as_ref(), out)?;
    if args.mode != Mode::Scenarios {
        let mut extra = Json::object();
        extra.set("mode", args.mode.as_str());
        extra.set("recorder", args.recorder.as_str());
        args.opts.record("weakness", extra)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks_parse_from_the_scenario_format() {
        let v: Value = serde_json::from_str(
            r#"{"kind": "turn", "winner": "none", "zones": [[3, "TRASH"]], "active": [1]}"#,
        )
        .expect("json");
        assert_eq!(
            check_of(&v),
            Some(Check::Turn {
                winner: Some(None),
                zones: vec![(3, Zone::Trash)],
                active: vec![1],
            })
        );
        let v: Value =
            serde_json::from_str(r#"{"kind": "choice", "want": [["block", 1, -1, -1]]}"#)
                .expect("json");
        assert_eq!(
            check_of(&v),
            Some(Check::Choice(vec![Action::new(
                ActionKind::Block,
                1,
                -1,
                -1
            )]))
        );
        let v: Value =
            serde_json::from_str(r#"{"kind": "turn", "winner": 0, "zones": [], "active": []}"#)
                .expect("json");
        assert!(matches!(
            check_of(&v),
            Some(Check::Turn {
                winner: Some(Some(0)),
                ..
            })
        ));
        let v: Value = serde_json::from_str(r#"{"kind": "other"}"#).expect("json");
        assert_eq!(check_of(&v), None);
    }

    #[test]
    fn means_and_standard_errors() {
        assert_eq!(mean_se(&[]), (0.0, None));
        let (m, se) = mean_se(&[1.0, 0.0, 1.0, 0.0]);
        assert_eq!(m, 0.5);
        assert!(se.is_some_and(|s| (s - (1.0_f64 / 12.0).sqrt()).abs() < 1e-12));
    }

    #[test]
    fn mode_names_round_trip() {
        for m in Mode::ALL {
            assert_eq!(
                Mode::ALL.iter().find(|x| x.as_str() == m.as_str()),
                Some(&m)
            );
        }
    }

    fn committed() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(DEFAULT_SCENARIOS)
    }

    fn with_cx<T>(f: impl FnOnce(&mut gcg_ai::AgentCx<'_>) -> T) -> T {
        let cancel = Cancel::new();
        gcg_core::pool::Pool::run_inline(
            |scratch: &mut gcg_ai::Scratch, par: &gcg_core::pool::TaskCx<'_, gcg_ai::Scratch>| {
                let mut cx = gcg_ai::AgentCx {
                    scratch,
                    par,
                    cancel: &cancel,
                };
                f(&mut cx)
            },
        )
    }

    fn scenario<'s>(all: &'s [Scenario], id: &str) -> &'s Scenario {
        all.iter().find(|s| s.id == id).expect("a frozen scenario")
    }

    #[test]
    fn the_committed_scenarios_cover_every_category_and_score_with_controls() {
        let all = load_scenarios(&committed()).expect("the committed scenarios");
        assert_eq!(all.len(), 22);
        let mut cats: Vec<&str> = all.iter().map(|s| s.category.as_str()).collect();
        cats.sort();
        cats.dedup();
        assert_eq!(
            cats,
            ["WK1", "WK2", "WK3", "WK4", "WK5", "WK6", "WK7", "WK8"]
        );
        for s in &all {
            if let Check::Choice(want) = &s.check {
                let options = s.state.pending().expect("a decision").options.to_vec();
                assert!(want.iter().all(|w| options.contains(w)), "{}", s.id);
            }
        }
        let reference = parse_agent_spec("ref:standard").expect("spec");
        let defender = parse_agent_spec(DEFENDER).expect("spec");
        let lethal = scenario(&all, "wk1_lethal_attack_the_exposed_player");
        assert!(with_cx(|cx| solves(lethal, &reference, &defender, 1, cx)).expect("solve"));
        let mut wrong_winner = lethal.clone();
        wrong_winner.check = Check::Turn {
            winner: Some(Some(1)),
            zones: Vec::new(),
            active: Vec::new(),
        };
        assert!(!with_cx(|cx| solves(&wrong_winner, &reference, &defender, 1, cx)).expect("solve"));
        let block = scenario(&all, "wk2_blocks_to_survive");
        assert!(with_cx(|cx| solves(block, &reference, &defender, 1, cx)).expect("solve"));
        let mut nothing = block.clone();
        nothing.check = Check::Choice(Vec::new());
        assert!(!with_cx(|cx| solves(&nothing, &reference, &defender, 1, cx)).expect("solve"));
    }

    #[test]
    fn a_damage_decision_is_judged_by_both_policies() {
        let all = load_scenarios(&committed()).expect("the committed scenarios");
        let ping = scenario(&all, "wk7_damage_the_unit_that_cannot_repair");
        let cfg = search_config("search:standard").expect("config");
        assert!(gcg_search::gapa1::is_damage_decision(&ping.state, &cfg));
        let block = scenario(&all, "wk2_blocks_to_survive");
        assert!(!gcg_search::gapa1::is_damage_decision(&block.state, &cfg));
        let j = gcg_search::gapa1::damage_judgement(&ping.state, ping.player, &cfg, 2, 7)
            .expect("judge")
            .expect("a Damage decision");
        let again = gcg_search::gapa1::damage_judgement(&ping.state, ping.player, &cfg, 2, 7)
            .expect("judge")
            .expect("a Damage decision");
        assert_eq!(j, again, "deterministic");
        assert_eq!(j.values.len(), j.options.len());
        assert!(j.values.iter().all(|v| (0.0..=1.0).contains(v)));
        assert!(j.regret(j.shared_pick) >= 0.0 && j.regret(j.kill_pick) >= 0.0);
        for probs in [&j.shared_probs, &j.kill_probs] {
            assert!((py_sum(probs.iter().copied()) - 1.0).abs() < 1e-12);
        }
        assert_eq!(j.pick_diffs.len(), 2);
    }

    #[test]
    fn the_census_counts_splits_by_cause() {
        let all = load_scenarios(&committed()).expect("the committed scenarios");
        let st = &scenario(&all, "wk7_rest_the_active_unit").state;
        for spec in [
            "search:null-standard",
            "search:null-standard:observation_keys=false",
        ] {
            let cfg = search_config(spec).expect("config");
            let player = st.pending().expect("a decision").hdr.player;
            let (c, result) =
                with_cx(|cx| gcg_search::gapa1::split_census(st, player, &cfg, 11, cx))
                    .expect("census");
            assert_eq!(c.iterations, result.stats.iterations);
            assert!(c.observation_keys <= c.searcher_nodes, "{spec}");
            assert_eq!(c.searcher_nodes - c.observation_keys, c.splits, "{spec}");
            assert_eq!(
                c.causes.iter().map(|(_, n)| n).sum::<u32>(),
                c.splits,
                "{spec}"
            );
            let again = with_cx(|cx| gcg_search::gapa1::split_census(st, player, &cfg, 11, cx))
                .expect("census");
            assert_eq!(c, again.0, "deterministic");
        }
    }

    #[test]
    fn the_audit_replays_a_recorded_game_and_its_flags_are_deterministic() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let deck = |name: &str| {
            let d = gcg_data::load_deck(&root.join(format!("examples/decks/{name}.txt")))
                .expect("an example deck");
            let side = |cards: &[gcg_data::CardCount]| {
                let mut m = serde_json::Map::new();
                for c in cards {
                    m.insert(
                        c.card_number.to_string(),
                        Value::from(u64::try_from(c.count).expect("a count")),
                    );
                }
                Value::Object(m)
            };
            (side(&d.main), side(&d.resources))
        };
        let seats = [deck("red-green-zeon"), deck("blue-white-federation")];
        let doc_of = |actions: &[Action]| {
            serde_json::json!({
                "seed": 5,
                "chooser": 0,
                "turn_limit": 60,
                "max_actions": 20000,
                "seats": [
                    {"seat": 0, "main": seats[0].0, "resources": seats[0].1},
                    {"seat": 1, "main": seats[1].0, "resources": seats[1].1},
                ],
                "actions": actions
                    .iter()
                    .map(|a| serde_json::json!([a.kind.as_str(), a.a, a.b, a.c]))
                    .collect::<Vec<_>>(),
            })
        };
        let dir = std::env::temp_dir().join(format!("gcg-gapa1-audit-{}", std::process::id()));
        let path = dir.join("replays").join("game.json");
        std::fs::create_dir_all(path.parent().expect("dir")).expect("mkdir");
        std::fs::write(&path, doc_of(&[]).to_string()).expect("write");
        let (mut st, none) = replay_start(&path).expect("the start");
        assert!(none.is_empty());
        let mut agents = [
            gcg_ai::make_random_agent(1, false),
            gcg_ai::make_random_agent(2, false),
        ];
        let mut actions = Vec::new();
        while st.winner().is_none() && actions.len() < 3000 {
            let p = st.pending().expect("a decision").hdr.player;
            let a = with_cx(|cx| agents[usize::from(p)].choose(&st, p, cx)).expect("a move");
            apply(&mut st, a, true).expect("legal");
            actions.push(a);
        }
        std::fs::write(&path, doc_of(&actions).to_string()).expect("write");
        let cfg = search_config("search:fast").expect("config");
        let flags = audit_game(&path, 3, 2, &cfg).expect("audit");
        let again = audit_game(&path, 3, 2, &cfg).expect("audit");
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(flags, again, "deterministic");
        assert!(flags.decisions > 0);
        assert!(
            flags.count() > 0,
            "random play makes flagged plays: {flags:?}"
        );
        assert_eq!(replay_files(&root.join("no-such-run")).ok(), None);
    }
}
