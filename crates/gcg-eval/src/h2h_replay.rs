//! `gcg-eval head-to-head-replay REPORT (--index I [--index I ...] | --losses) --out DIR`: games
//! of a `head-to-head --out` report as `gcg-sim replay` documents (PLAN §12.6: every game E3's
//! challenger does not win is documented with its replay).
//!
//! A report keeps one row per game (`[index, pair, challenger, winner, turns, first_player,
//! decisions, ...]`), not its actions. The run's specs are rebuilt from the report's own fields
//! (`challenger`, `opponent`, `decks` and `pairs`, or the `--pair` list that `pairs` holds when
//! `decks` is null, `seed`, `games`, `choose_first`) by [`head_to_head::plan`] without a role,
//! and checked against the report: every `by_pair` row's names, position and master, and every
//! row's index, pair and challenger seat. `--losses` selects the rows whose winner is not the
//! challenger (losses, draws and unfinished games), `--index` the given rows.
//!
//! Each selected game is played again on head-to-head's path ([`play_observed_final`], the
//! agents asked through `choose_traced`) with an observer that records every applied action;
//! its winner, turns, first player and decisions must equal the row. Its document is
//! `gcg_runner::replay::replay_document` of the game as a record: match index = the game's
//! index (the challenger sits in seat `index % 2`, where the deck under test of that match
//! sits), game index 1, the game seed, chooser null (the die roll; the CHOOSE_FIRST answer is
//! among the actions), the engine's default caps, role `dut` for the challenger's seat and
//! `bench` for the other, the report's agent and deck names, and the final state's digest. A
//! game the decision limit stopped has no end reason, so its document's `expected.end_reason`
//! is null, as `check_replay` reports it. Every document must pass `check_replay` before any
//! file is written.
//!
//! `--challenger SPEC` and `--opponent SPEC` play a side with another spec than the report's,
//! for a report written before a preset changed (overrides that restore the configuration it
//! ran); the rows must still match, and the documents keep the report's names.
//!
//! `DIR/game-<index>.json` is written as `gcg-sim` writes replays (`to_json_text`), and
//! `DIR/index.json` (`json.dumps(indent=2)`) lists the report's file name, seed and agents and,
//! per game, its index, pair, seats, winner, end reason, turns, decisions, game seed and
//! replay file. Stdout has one line per game.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use gcg_core::pool::Cancel;
use gcg_core::pyjson::{Json, Key, dumps};
use gcg_data::{CardDb, DeckList, py_path_str};
use gcg_engine::{Action, EndReason, GameState, NewGameOptions, canon};
use gcg_runner::argparse::{Action as Arg, Converter, Exit, Namespace, Parser, PyStr, Value};
use gcg_runner::reports::to_json_text;
use gcg_runner::{
    BenchmarkConfig, DutCardEvents, Format, GameRecord, Outcome, SchemaVersion, Workers, dut_seat,
};
use serde_json::Value as JsonValue;

use crate::bench_cmd::{CommandError, ProgressPrinter};
use crate::build_info::{CODE_DIGEST, PACKAGE_VERSION};
use crate::compare::indented;
use crate::head_to_head::{
    self, HeadToHeadArgs, PairSource, Plan, REPORT_FORMAT, REPORT_VERSION, SelectedPair,
};
use crate::ledger::DEFAULT_LEDGER;
use crate::selfplay::{Observer, play_observed_final, run_indexed};
use crate::specs::pairing_master;
use crate::{ChooseFirst, DeckSet, GameOutcome, MirroredSpec, PairSet};

/// The format name and version of `index.json`.
pub const INDEX_FORMAT: &str = "gcg-eval-head-to-head-replay";
pub const INDEX_VERSION: u32 = 1;
/// The index file's name in the output directory.
pub const INDEX_FILE: &str = "index.json";
/// `game_index` of every document: a head-to-head game is the only game of its match.
pub const GAME_INDEX: u32 = 1;

const DESCRIPTION: &str = "Re-play games of a head-to-head --out report and write each as a \
     gcg-sim replay file, after checking that it reproduces its row (PLAN §12.6).";

/// Which rows to re-play.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Games {
    /// `--index`, sorted and without repeats.
    Indices(Vec<u32>),
    /// `--losses`: every row the challenger did not win.
    Losses,
}

/// The parsed arguments.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ReplayArgs {
    pub report: PathBuf,
    pub games: Games,
    /// `--challenger SPEC` and `--opponent SPEC`.
    pub play: [Option<String>; 2],
    pub out: PathBuf,
}

/// The subcommand parser.
#[must_use]
pub fn parser(prog: &str) -> Parser {
    let mut p = Parser::new(&format!("{prog} head-to-head-replay"), Some(DESCRIPTION));
    p.push(Arg::positional(
        "report",
        Converter::Path,
        Some("a report written by head-to-head --out".to_owned()),
    ));
    p.push(Arg::append(
        "--index",
        "index",
        Converter::NonNegativeInt,
        None,
        "the outcome index of a game to re-play (repeatable)".to_owned(),
    ));
    p.push(Arg::flag(
        "--losses",
        "losses",
        "re-play every game the challenger did not win (losses, draws, unfinished games)"
            .to_owned(),
    ));
    for (option, dest, side) in [
        ("--challenger", "challenger", "challenger"),
        ("--opponent", "opponent", "opponent"),
    ] {
        p.push(Arg::option(
            option,
            dest,
            Converter::Str,
            None,
            Value::None,
            format!(
                "play the {side} with this agent spec instead of the report's (the games must \
                 still reproduce their rows)"
            ),
        ));
    }
    let mut out = Arg::option(
        "--out",
        "out",
        Converter::Path,
        None,
        Value::None,
        "directory for the game-<index>.json replays and index.json".to_owned(),
    );
    out.required = true;
    p.push(out);
    p
}

fn path_of(ns: &Namespace, dest: &str) -> PathBuf {
    match ns.get(dest) {
        Some(Value::Path(s)) => PathBuf::from(s.to_os_string()),
        _ => unreachable!("{dest} is a required Path argument"),
    }
}

fn text_of(ns: &Namespace, dest: &str) -> Option<String> {
    match ns.get(dest) {
        Some(Value::Str(s)) => Some(
            s.to_string_opt()
                .unwrap_or_else(|| String::from_utf8_lossy(&s.to_surrogateescape_bytes()).into()),
        ),
        _ => None,
    }
}

/// The typed arguments of a parsed namespace.
///
/// # Errors
/// A usage error: neither or both of `--index` and `--losses`, or an index above `u32::MAX`.
pub fn args(ns: &Namespace, sub: &Parser, columns: i64) -> Result<ReplayArgs, Exit> {
    let usage = |msg: &str| sub.error(&PyStr::from(msg), columns);
    let given: Vec<&Value> = match ns.get("index") {
        Some(Value::List(items)) => items.iter().collect(),
        _ => Vec::new(),
    };
    let losses = matches!(ns.get("losses"), Some(Value::Bool(true)));
    let games = match (given.is_empty(), losses) {
        (true, false) => return Err(usage("one of the arguments --index --losses is required")),
        (false, true) => {
            return Err(usage(
                "argument --losses: not allowed with argument --index",
            ));
        }
        (true, true) => Games::Losses,
        (false, false) => {
            let mut indices = Vec::with_capacity(given.len());
            for v in given {
                let Value::Int(i) = v else {
                    unreachable!("--index converts to an integer")
                };
                let index = i
                    .to_u64()
                    .and_then(|x| u32::try_from(x).ok())
                    .ok_or_else(|| {
                        usage(&format!(
                            "argument --index: must be at most {}, got {}",
                            u32::MAX,
                            i.to_decimal()
                        ))
                    })?;
                indices.push(index);
            }
            indices.sort();
            indices.dedup();
            Games::Indices(indices)
        }
    };
    Ok(ReplayArgs {
        report: path_of(ns, "report"),
        games,
        play: [text_of(ns, "challenger"), text_of(ns, "opponent")],
        out: path_of(ns, "out"),
    })
}

/// One `by_pair` row: deck names, position and master.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct PairRow {
    pub decks: [String; 2],
    pub position: u64,
    pub master: u64,
}

/// One `outcomes` row: its pair position and the game's outcome.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Row {
    pub pair: u64,
    pub outcome: GameOutcome,
}

/// The fields of a head-to-head report that define its games.
#[derive(Clone, PartialEq, Debug)]
pub struct Report {
    /// The report path's file name.
    pub file_name: String,
    pub challenger: String,
    pub opponent: String,
    /// `agents`: the challenger's and the opponent's names.
    pub names: [String; 2],
    pub source: PairSource,
    pub seed: u64,
    pub games: u32,
    pub choose_first: ChooseFirst,
    pub by_pair: Vec<PairRow>,
    pub rows: Vec<Row>,
}

fn str_field(v: &JsonValue, key: &str) -> Result<String, String> {
    v.get(key)
        .and_then(JsonValue::as_str)
        .map(str::to_owned)
        .ok_or_else(|| format!("no string {key:?}"))
}

fn u64_field(v: &JsonValue, key: &str) -> Result<u64, String> {
    v.get(key)
        .and_then(JsonValue::as_u64)
        .ok_or_else(|| format!("no non-negative integer {key:?}"))
}

fn int_at<T: TryFrom<i64>>(row: &[JsonValue], i: usize) -> Option<T> {
    row.get(i)
        .and_then(JsonValue::as_i64)
        .and_then(|x| T::try_from(x).ok())
}

fn row_of(v: &JsonValue, at: usize) -> Result<Row, String> {
    let bad = || {
        format!(
            "outcome {at} is not [index, pair, challenger, winner, turns, first_player, \
             decisions, ...]"
        )
    };
    let row = v.as_array().ok_or_else(bad)?;
    let (
        Some(index),
        Some(pair),
        Some(challenger),
        Some(winner),
        Some(turns),
        Some(first_player),
        Some(decisions),
    ) = (
        int_at::<u32>(row, 0),
        int_at::<u64>(row, 1),
        int_at::<u8>(row, 2),
        int_at::<i8>(row, 3),
        int_at::<i32>(row, 4),
        int_at::<u8>(row, 5),
        int_at::<u32>(row, 6),
    )
    else {
        return Err(bad());
    };
    Ok(Row {
        pair,
        outcome: GameOutcome {
            index,
            challenger,
            winner,
            turns,
            first_player,
            decisions,
        },
    })
}

fn pair_row(v: &JsonValue, at: usize) -> Result<PairRow, String> {
    let bad = |what: &str| format!("by_pair row {at} has no {what}");
    let decks = v
        .get("decks")
        .and_then(JsonValue::as_array)
        .filter(|d| d.len() == 2)
        .ok_or_else(|| bad("decks pair"))?;
    let name = |i: usize| {
        decks[i]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| bad("deck names"))
    };
    Ok(PairRow {
        decks: [name(0)?, name(1)?],
        position: u64_field(v, "position").map_err(|_| bad("position"))?,
        master: u64_field(v, "master").map_err(|_| bad("master"))?,
    })
}

impl Report {
    /// Reads a head-to-head report.
    ///
    /// # Errors
    /// The file is unreadable, not JSON, or not a `gcg-eval-head-to-head` v2 report with
    /// `by_pair` and `outcomes`.
    pub fn load(path: &Path) -> Result<Self, String> {
        let shown = py_path_str(path);
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read the report {shown}: {e}"))?;
        let v: JsonValue =
            serde_json::from_str(&text).map_err(|e| format!("{shown}: not JSON: {e}"))?;
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        Self::from_value(&v, file_name).map_err(|e| format!("{shown}: {e}"))
    }

    /// A report from its parsed JSON.
    ///
    /// # Errors
    /// As [`Report::load`].
    pub fn from_value(v: &JsonValue, file_name: String) -> Result<Self, String> {
        let format_ok = v.get("format").and_then(JsonValue::as_str) == Some(REPORT_FORMAT);
        let version_ok =
            v.get("version").and_then(JsonValue::as_u64) == Some(REPORT_VERSION.into());
        if !format_ok || !version_ok {
            return Err(format!(
                "not a {REPORT_FORMAT} v{REPORT_VERSION} report (head-to-head --out)"
            ));
        }
        let agents = v.get("agents").ok_or("no \"agents\"")?;
        let pairs = str_field(v, "pairs")?;
        let source = match v.get("decks") {
            Some(JsonValue::Null) => PairSource::Named(pairs),
            Some(JsonValue::String(d)) => PairSource::Set {
                decks: DeckSet::from_value(d).ok_or_else(|| format!("unknown deck set {d:?}"))?,
                pairs: PairSet::from_value(&pairs)
                    .ok_or_else(|| format!("unknown pair set {pairs:?}"))?,
            },
            _ => return Err("no \"decks\" (a deck set or null)".to_owned()),
        };
        let choose_first = str_field(v, "choose_first")?;
        let by_pair = v
            .get("by_pair")
            .and_then(JsonValue::as_array)
            .ok_or("no by_pair rows")?
            .iter()
            .enumerate()
            .map(|(k, r)| pair_row(r, k))
            .collect::<Result<_, _>>()?;
        let rows = v
            .get("outcomes")
            .and_then(JsonValue::as_array)
            .ok_or("no outcomes (write the report with --out)")?
            .iter()
            .enumerate()
            .map(|(i, r)| row_of(r, i))
            .collect::<Result<_, _>>()?;
        Ok(Self {
            file_name,
            challenger: str_field(v, "challenger")?,
            opponent: str_field(v, "opponent")?,
            names: [
                str_field(agents, "challenger")?,
                str_field(agents, "opponent")?,
            ],
            source,
            seed: u64_field(v, "seed")?,
            games: u64_field(v, "games")?
                .try_into()
                .map_err(|_| "games is above u32::MAX".to_owned())?,
            choose_first: ChooseFirst::from_value(&choose_first)
                .ok_or_else(|| format!("unknown choose_first {choose_first:?}"))?,
            by_pair,
            rows,
        })
    }

    /// The rows `games` selects, in index order.
    ///
    /// # Errors
    /// An index without a row.
    pub fn select(&self, games: &Games) -> Result<Vec<usize>, String> {
        match games {
            Games::Losses => Ok(self
                .rows
                .iter()
                .enumerate()
                .filter(|(_, r)| i16::from(r.outcome.winner) != i16::from(r.outcome.challenger))
                .map(|(i, _)| i)
                .collect()),
            Games::Indices(indices) => indices
                .iter()
                .map(|&i| {
                    usize::try_from(i)
                        .ok()
                        .filter(|&i| i < self.rows.len())
                        .ok_or_else(|| {
                            format!(
                                "--index {i}: the report has {} games (indices 0 to {})",
                                self.rows.len(),
                                self.rows.len().saturating_sub(1)
                            )
                        })
                })
                .collect(),
        }
    }
}

/// The run's plan rebuilt from the report, played with `play` in place of the report's specs
/// where given, and the selection pair of every game.
///
/// # Errors
/// What [`head_to_head::plan`] refuses, or a report that disagrees with the rebuilt run.
pub fn rebuild(report: &Report, play: &[Option<String>; 2]) -> Result<(Plan, Vec<usize>), String> {
    let spec_text = |i: usize, given: &str| play[i].clone().unwrap_or_else(|| given.to_owned());
    let args = HeadToHeadArgs {
        challenger: spec_text(0, &report.challenger),
        opponent: spec_text(1, &report.opponent),
        source: report.source.clone(),
        games: report.games,
        seed: report.seed,
        role: None,
        ledger: PathBuf::from(DEFAULT_LEDGER),
        workers: Workers::Count(1),
        choose_first: report.choose_first,
        out: None,
        decision_trace: None,
    };
    let plan = head_to_head::plan(&args)?;
    let sel = &plan.selection;
    if report.by_pair.len() != sel.pairs.len() {
        return Err(format!(
            "the report has {} by_pair rows; its configuration selects {} pairs",
            report.by_pair.len(),
            sel.pairs.len()
        ));
    }
    for (k, (row, pair)) in report.by_pair.iter().zip(&sel.pairs).enumerate() {
        let names = [pair.decks[0].name.as_str(), pair.decks[1].name.as_str()];
        let position = u64::try_from(pair.position).unwrap_or(u64::MAX);
        let master = pairing_master(report.seed, pair.position, sel.pairings_total);
        if row.decks != names || row.position != position || row.master != master {
            return Err(format!(
                "by_pair row {k} is {:?} at position {} with master {}; the configuration \
                 gives {names:?} at position {position} with master {master}",
                row.decks, row.position, row.master
            ));
        }
    }
    if report.rows.len() != plan.specs.len() {
        return Err(format!(
            "the report has {} outcomes; its configuration plays {} games",
            report.rows.len(),
            plan.specs.len()
        ));
    }
    let pair_of: Vec<usize> = plan
        .counts
        .iter()
        .enumerate()
        .flat_map(|(k, &n)| std::iter::repeat_n(k, n))
        .collect();
    for (i, (row, spec)) in report.rows.iter().zip(&plan.specs).enumerate() {
        let position = u64::try_from(sel.pairs[pair_of[i]].position).unwrap_or(u64::MAX);
        let o = &row.outcome;
        if usize::try_from(o.index).ok() != Some(i)
            || row.pair != position
            || o.challenger != spec.challenger
        {
            return Err(format!(
                "outcome {i} is game {} of pair {} with the challenger in seat {}; the \
                 configuration gives game {i} of pair {position} with the challenger in seat {}",
                o.index, row.pair, o.challenger, spec.challenger
            ));
        }
    }
    Ok((plan, pair_of))
}

/// The observer that records every applied action.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct ActionLog {
    pub actions: Vec<Action>,
}

impl Observer for ActionLog {
    fn applied(&mut self, action: Action) {
        self.actions.push(action);
    }
}

/// A game played again: its outcome, actions and the final state's facts.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Replayed {
    pub outcome: GameOutcome,
    pub actions: Vec<Action>,
    pub winner_seat: Option<i8>,
    pub end_reason: Option<EndReason>,
    pub chooser_seat: u8,
    pub active_at_end: u8,
    pub redraws: [bool; 2],
    pub final_state_sha256: String,
}

impl Replayed {
    /// The facts of a finished (or stopped) game.
    #[must_use]
    pub fn new(outcome: GameOutcome, actions: Vec<Action>, st: &GameState) -> Self {
        Self {
            outcome,
            actions,
            winner_seat: st.winner(),
            end_reason: st.end_reason(),
            chooser_seat: st.setup_chooser(),
            active_at_end: st.active(),
            redraws: st.redraws(),
            final_state_sha256: canon::state_digest(st),
        }
    }

    /// The fields that differ from the report's row, as `field X (report Y)`.
    #[must_use]
    pub fn differences(&self, row: &GameOutcome) -> Vec<String> {
        let o = &self.outcome;
        let mut out = Vec::new();
        let mut check = |field: &str, got: i64, want: i64| {
            if got != want {
                out.push(format!("{field} {got} (report {want})"));
            }
        };
        check("winner", o.winner.into(), row.winner.into());
        check("turns", o.turns.into(), row.turns.into());
        check(
            "first_player",
            o.first_player.into(),
            row.first_player.into(),
        );
        check("decisions", o.decisions.into(), row.decisions.into());
        out
    }
}

/// Plays the games `chosen` (spec indices) again, recording their actions, in order.
///
/// # Errors
/// A failed game, or the interrupt.
pub fn replay_games(
    specs: &[MirroredSpec],
    chosen: &[usize],
    choose_first: ChooseFirst,
    cancel: &Cancel,
    progress: &mut dyn FnMut(usize),
) -> Result<Vec<Replayed>, CommandError> {
    run_indexed(
        chosen.len(),
        Workers::Count(1),
        cancel,
        progress,
        |i, cx| {
            let mut log = ActionLog::default();
            let (result, st) = play_observed_final(&specs[chosen[i]], choose_first, cx, &mut log)?;
            Ok(Replayed::new(result.outcome, log.actions, &st))
        },
        |i| (specs[chosen[i]].index, specs[chosen[i]].game_seed),
    )
    .map_err(CommandError::from)
}

/// A deck of `list` named `name`, rebuilt from its card numbers; it must expand back to `list`.
///
/// # Errors
/// A list that does not round-trip (a deck order `gcg-sim replay` cannot rebuild).
pub fn deck_of(name: &str, list: &DeckList) -> Result<gcg_data::Deck, String> {
    let db = CardDb::embedded();
    let mut counts: BTreeMap<&str, u64> = BTreeMap::new();
    for id in list.main.iter().chain(list.resources.iter()) {
        *counts.entry(&db.by_id(*id).card_number).or_insert(0) += 1;
    }
    let text: String = counts.iter().map(|(n, c)| format!("{c} {n}\n")).collect();
    let deck = gcg_data::parse_deck(&text, name, None)
        .map_err(|e| format!("deck {name} does not rebuild from its cards: {e}"))?;
    let back = gcg_data::to_decklist(&deck).map_err(|e| format!("deck {name}: {e}"))?;
    if back != *list {
        return Err(format!(
            "deck {name} does not rebuild to the list the game played (a replay sorts each \
             side by card number)"
        ));
    }
    Ok(deck)
}

/// One selected game with everything its document and index entry need.
pub struct Game<'a> {
    pub report: &'a Report,
    pub spec: &'a MirroredSpec,
    pub pair: &'a SelectedPair,
    pub played: &'a Replayed,
}

impl Game<'_> {
    /// The deck name of each seat.
    #[must_use]
    pub fn seat_decks(&self) -> [String; 2] {
        let [a, b] = &self.pair.decks;
        let name = |seat: usize| {
            if self.spec.decks[seat] == a.deck {
                a.name.clone()
            } else {
                b.name.clone()
            }
        };
        [name(0), name(1)]
    }

    /// The agent name of each seat.
    #[must_use]
    pub fn seat_agents(&self) -> [&str; 2] {
        let c = usize::from(self.spec.challenger);
        let mut out = [self.report.names[1].as_str(); 2];
        out[c] = &self.report.names[0];
        out
    }

    /// The game's label in its document.
    #[must_use]
    pub fn label(&self) -> String {
        format!("head-to-head game {}", self.spec.index)
    }

    /// The game's replay file name.
    #[must_use]
    pub fn file_name(&self) -> String {
        format!("game-{}.json", self.spec.index)
    }

    fn winner(&self) -> Outcome {
        match self.played.winner_seat {
            Some(w) if i16::from(w) == i16::from(self.spec.challenger) => Outcome::Dut,
            Some(w) if w >= 0 => Outcome::Bench,
            _ => Outcome::Draw,
        }
    }

    /// The replay document (module docs).
    ///
    /// # Errors
    /// A deck that does not rebuild, a challenger outside seat `index % 2`, or a replay error.
    pub fn document(&self, seat_decks: &[String; 2]) -> Result<Json, String> {
        let spec = self.spec;
        let index = spec.index;
        let c = usize::from(spec.challenger);
        if dut_seat(index) != spec.challenger {
            return Err(format!(
                "game {index}: the challenger sits in seat {c}, not in seat {} where a replay of \
                 match {index} puts its deck under test",
                dut_seat(index)
            ));
        }
        let opts = NewGameOptions::default();
        let cfg = BenchmarkConfig {
            deck_under_test: deck_of(&seat_decks[c], &spec.decks[c])?,
            benchmark_deck: deck_of(&seat_decks[1 - c], &spec.decks[1 - c])?,
            matches: self.report.games,
            seed: self.report.seed,
            workers: Workers::Count(1),
            format: Format::Bo1,
            ai_preset: "",
            out_dir: PathBuf::new(),
            decision_log: false,
            replays: 0,
            turn_limit: opts.turn_limit,
            max_actions: opts.max_actions,
            schema: SchemaVersion::V1_1,
            package: PACKAGE_VERSION,
            code_digest: CODE_DIGEST,
        };
        let played = self.played;
        let first_player = played.outcome.first_player;
        let record = GameRecord {
            match_index: index,
            game_index: GAME_INDEX,
            seed: spec.game_seed,
            agent_seeds: spec.agent_seeds,
            agents: self.seat_agents().map(Into::into),
            dut_seat: spec.challenger,
            chooser: opts.chooser,
            chooser_seat: played.chooser_seat,
            first_player,
            dut_on_play: first_player == spec.challenger,
            winner: self.winner(),
            // A record always holds an end reason; the stopped game's null is set below.
            end_reason: played.end_reason.unwrap_or(EndReason::TurnLimit),
            match_point: None,
            turns: played.outcome.turns,
            active_at_end: played.active_at_end,
            actions: played.actions.clone(),
            redraws: played.redraws,
            dut_cards: DutCardEvents {
                initial_hand: Vec::new(),
                redrew: false,
                opening_hand: Vec::new(),
                drawn: Vec::new(),
                played: Vec::new(),
                never_played: Vec::new(),
                in_hand_at_end: Vec::new(),
            },
            bench_seen: Vec::new(),
            final_state_sha256: played.final_state_sha256.clone(),
            turn_limit: opts.turn_limit,
            max_actions: opts.max_actions,
            decision_log: None,
            effort: [None, None],
        };
        let mut doc = gcg_runner::replay::replay_document(&record, &cfg, &self.label())
            .map_err(|e| format!("game {index}: cannot build its replay: {e}"))?;
        if played.end_reason.is_none()
            && let Json::Object(items) = &mut doc
            && let Some((_, expected)) = items.iter_mut().find(|(k, _)| *k == Key::from("expected"))
        {
            expected.set("end_reason", Json::Null);
        }
        Ok(doc)
    }

    /// The game's `index.json` entry.
    #[must_use]
    pub fn entry(&self, seat_decks: &[String; 2]) -> Json {
        let o = &self.played.outcome;
        let mut e = Json::object();
        e.set("index", o.index);
        e.set(
            "pair",
            vec![
                self.pair.decks[0].name.as_str(),
                self.pair.decks[1].name.as_str(),
            ],
        );
        e.set("position", self.pair.position);
        e.set(
            "seat_decks",
            vec![seat_decks[0].as_str(), seat_decks[1].as_str()],
        );
        e.set("challenger_seat", o.challenger);
        e.set("first_player", o.first_player);
        e.set("winner", o.winner);
        e.set("end_reason", self.played.end_reason.map(EndReason::as_str));
        e.set("turns", o.turns);
        e.set("decisions", o.decisions);
        e.set("game_seed", self.spec.game_seed);
        e.set("replay", self.file_name());
        e
    }

    /// The game's stdout line.
    #[must_use]
    pub fn line(&self, seat_decks: &[String; 2]) -> String {
        let o = &self.played.outcome;
        format!(
            "game {}: seat 0 {}, seat 1 {}; challenger seat {}; winner {} ({}) after {} turns",
            o.index,
            seat_decks[0],
            seat_decks[1],
            o.challenger,
            if o.winner < 0 {
                "none".to_owned()
            } else {
                format!("seat {}", o.winner)
            },
            self.played
                .end_reason
                .map_or("not finished", EndReason::as_str),
            o.turns
        )
    }
}

/// `check_replay` of a written document's text.
///
/// # Errors
/// The text does not parse, the replay fails, or it does not reproduce its expected outcome.
pub fn check_text(text: &str) -> Result<(), String> {
    let v: JsonValue = serde_json::from_str(text).map_err(|e| format!("not JSON: {e}"))?;
    let check = gcg_runner::replay::check_replay(&v).map_err(|e| e.to_string())?;
    if check.ok {
        Ok(())
    } else {
        Err(format!(
            "the replay does not match its expected outcome: {}",
            check.mismatches.join("; ")
        ))
    }
}

fn write(path: &Path, text: &str) -> Result<(), CommandError> {
    gcg_runner::reports::write_atomic(path, text)
        .map_err(|e| CommandError::Invalid(format!("cannot write {}: {e}", py_path_str(path))))
}

/// `gcg-eval head-to-head-replay` (module docs).
///
/// # Errors
/// An unreadable or inconsistent report, a game that fails or does not reproduce its row, a
/// document that does not replay, an unwritable output, or the interrupt.
pub fn run(
    args: &ReplayArgs,
    cancel: &Cancel,
    tty: bool,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<(), CommandError> {
    let invalid = CommandError::Invalid;
    let report = Report::load(&args.report).map_err(invalid)?;
    let shown = py_path_str(&args.report);
    let (plan, pair_of) =
        rebuild(&report, &args.play).map_err(|e| invalid(format!("{shown}: {e}")))?;
    let chosen = report.select(&args.games).map_err(invalid)?;
    let total = u64::try_from(chosen.len()).unwrap_or(u64::MAX);
    let played = {
        let mut printer = ProgressPrinter::new(err, tty, "games");
        let mut progress = |done: usize| printer.update(u64::try_from(done).unwrap_or(0), total);
        replay_games(
            &plan.specs,
            &chosen,
            report.choose_first,
            cancel,
            &mut progress,
        )?
    };
    let mismatches: Vec<String> = chosen
        .iter()
        .zip(&played)
        .filter_map(|(&i, p)| {
            let d = p.differences(&report.rows[i].outcome);
            (!d.is_empty()).then(|| format!("game {i}: {}", d.join(", ")))
        })
        .collect();
    if !mismatches.is_empty() {
        return Err(invalid(format!(
            "{} of {} games do not reproduce their rows of {shown}:\n  {}",
            mismatches.len(),
            chosen.len(),
            mismatches.join("\n  ")
        )));
    }
    let mut files = Vec::with_capacity(chosen.len());
    let mut entries = Vec::with_capacity(chosen.len());
    let mut lines = Vec::with_capacity(chosen.len());
    for (&i, p) in chosen.iter().zip(&played) {
        let game = Game {
            report: &report,
            spec: &plan.specs[i],
            pair: &plan.selection.pairs[pair_of[i]],
            played: p,
        };
        let seats = game.seat_decks();
        let text = to_json_text(&game.document(&seats).map_err(invalid)?);
        check_text(&text).map_err(|e| invalid(format!("game {i}: {e}")))?;
        files.push((args.out.join(game.file_name()), text));
        entries.push(game.entry(&seats));
        lines.push(game.line(&seats));
    }
    let mut index = Json::object();
    index.set("format", INDEX_FORMAT);
    index.set("version", INDEX_VERSION);
    index.set("report", report.file_name.as_str());
    index.set("seed", report.seed);
    index.set("challenger", report.challenger.as_str());
    index.set("opponent", report.opponent.as_str());
    let mut agents = Json::object();
    agents.set("challenger", report.names[0].as_str());
    agents.set("opponent", report.names[1].as_str());
    index.set("agents", agents);
    let mut specs = Json::object();
    specs.set(
        "challenger",
        args.play[0].as_deref().unwrap_or(&report.challenger),
    );
    specs.set(
        "opponent",
        args.play[1].as_deref().unwrap_or(&report.opponent),
    );
    index.set("played", specs);
    index.set(
        "selection",
        match &args.games {
            Games::Losses => "losses",
            Games::Indices(_) => "index",
        },
    );
    index.set("games", Json::Array(entries));
    std::fs::create_dir_all(&args.out)
        .map_err(|e| invalid(format!("cannot create {}: {e}", py_path_str(&args.out))))?;
    for (path, text) in &files {
        write(path, text)?;
    }
    write(
        &args.out.join(INDEX_FILE),
        &(dumps(&index, indented(2)) + "\n"),
    )?;
    for line in lines {
        let _ = writeln!(out, "{line}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use gcg_effects::Registry;
    use gcg_engine::{apply, legal_actions, new_game};

    use super::*;
    use crate::decks::{NamedDeck, embedded};

    fn test_decks() -> Vec<NamedDeck> {
        embedded(DeckSet::Tests).unwrap_or_default()
    }

    #[test]
    fn embedded_decks_rebuild_from_their_card_numbers() {
        for d in test_decks() {
            let deck = deck_of(&d.name, &d.deck).expect("rebuilds");
            assert_eq!(&*deck.name, d.name.as_str());
            assert_eq!(gcg_data::to_decklist(&deck).ok().as_ref(), Some(&d.deck));
        }
    }

    /// A game the decision limit stopped has no winner and no end reason: its document says
    /// null for both and still replays.
    #[test]
    fn a_stopped_game_documents_a_null_end_reason() {
        let decks = test_decks();
        let lists = [&decks[0].deck, &decks[1].deck];
        let mut st =
            new_game(&Registry::embedded(), lists, 7, NewGameOptions::default()).expect("new game");
        let mut actions = Vec::new();
        for _ in 0..12 {
            let action = legal_actions(&st)[0];
            apply(&mut st, action, true).expect("legal");
            actions.push(action);
        }
        assert!(st.winner().is_none() && st.pending().is_some());
        let outcome = GameOutcome {
            index: 0,
            challenger: 0,
            winner: -1,
            turns: st.turn(),
            first_player: st.first_player(),
            decisions: 12,
        };
        let played = Replayed::new(outcome, actions, &st);
        let report = Report {
            file_name: "r.json".to_owned(),
            challenger: "random".to_owned(),
            opponent: "greedy".to_owned(),
            names: ["random".to_owned(), "greedy".to_owned()],
            source: PairSource::Set {
                decks: DeckSet::Tests,
                pairs: PairSet::All,
            },
            seed: 7,
            games: 4,
            choose_first: ChooseFirst::Seat0,
            by_pair: Vec::new(),
            rows: Vec::new(),
        };
        let spec = MirroredSpec {
            index: 0,
            game_seed: 7,
            agent_seeds: [1, 2],
            agents: [crate::AgentDesc::Random, crate::AgentDesc::Random],
            decks: [decks[0].deck.clone(), decks[1].deck.clone()],
            challenger: 0,
        };
        let pair = SelectedPair {
            position: 0,
            decks: [decks[0].clone(), decks[1].clone()],
            held_out: false,
        };
        let game = Game {
            report: &report,
            spec: &spec,
            pair: &pair,
            played: &played,
        };
        let seats = game.seat_decks();
        assert_eq!(seats, [decks[0].name.clone(), decks[1].name.clone()]);
        let doc = game.document(&seats).expect("document");
        let expected = doc.get("expected").expect("expected");
        assert_eq!(expected.get("end_reason"), Some(&Json::Null));
        assert_eq!(expected.get("winner_seat"), Some(&Json::Null));
        assert_eq!(expected.get("winner"), Some(&Json::from("draw")));
        assert_eq!(check_text(&to_json_text(&doc)), Ok(()));
        assert_eq!(
            game.line(&seats),
            format!(
                "game 0: seat 0 {}, seat 1 {}; challenger seat 0; winner none (not finished) \
                 after {} turns",
                decks[0].name,
                decks[1].name,
                st.turn()
            )
        );
    }
}
