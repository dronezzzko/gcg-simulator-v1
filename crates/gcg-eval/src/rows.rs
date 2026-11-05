//! What the Phase 6 measurement tools share (docs/dev/PLAN.md §12.4-§12.6, §12.8): the run
//! options (pairs, games, seed, role, ledger, workers, play/draw protocol, output file), the
//! role checks before any game (A14, §12.5), one *row* (two agents over the selected pairs in
//! mirrored blocks of four, the blocks stratified round-robin over the pairs), its report
//! blocks, and the §12.4 summary of a row: wins/N, the Wilson 95% interval, the paired
//! difference with its Newcombe 95% interval, McNemar's discordant counts, and iterations and
//! applies per game.
//!
//! Every row is deterministic for its seed: the specs depend only on the options, the games
//! run on the work pool with results in spec order, and the report holds no worker count.

use std::io::Write;
use std::path::PathBuf;

use gcg_core::pool::Cancel;
use gcg_core::pyfloat::format_fixed;
use gcg_core::pyjson::{DumpOptions, Json, Key, dumps};
use gcg_data::py_path_str;
use gcg_runner::Workers;

use crate::agent_spec::agent_name;
use crate::bench_cmd::{CommandError, ProgressPrinter};
use crate::head_to_head::PairSource;
use crate::head_to_head::{Played, Selection, check_role, select, stratified_specs};
use crate::ledger::{self, Entry};
use crate::selfplay::{GameResult, SeatEffort, play_all_traced};
use crate::{AgentDesc, ChooseFirst, GameOutcome, MirroredSpec, SeedRole};

/// The options every row tool takes.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct RunOptions {
    pub source: PairSource,
    pub games: u32,
    pub seed: u64,
    pub role: Option<SeedRole>,
    pub ledger: PathBuf,
    pub workers: Workers,
    pub choose_first: ChooseFirst,
    pub out: Option<PathBuf>,
}

impl RunOptions {
    /// The report keys of the options: `decks`, `pairs`, `seed`, `role`, `games`,
    /// `choose_first` (never the worker count).
    pub fn describe(&self, doc: &mut Json) {
        let (decks, pairs) = self.source.report_values();
        doc.set("decks", decks);
        doc.set("pairs", pairs);
        doc.set("seed", self.seed);
        doc.set("role", self.role.map(SeedRole::as_str));
        doc.set("games", self.games);
        doc.set("choose_first", self.choose_first.as_str());
    }

    /// The ledger configuration of a run: the options plus `extra`'s keys.
    #[must_use]
    pub fn ledger_config(&self, extra: Json) -> Json {
        let mut config = extra;
        self.source.ledger_config(&mut config);
        config.set("games", self.games);
        config.set("choose_first", self.choose_first.as_str());
        config
    }

    /// Records a finished role run in the seed ledger (A14).
    ///
    /// # Errors
    /// The ledger cannot be read or written.
    pub fn record(&self, command: &'static str, extra: Json) -> Result<(), CommandError> {
        if let Some(role) = self.role {
            let entry = Entry {
                role,
                master: self.seed,
                command,
                config: self.ledger_config(extra),
            };
            ledger::record(&self.ledger, &entry).map_err(CommandError::Invalid)?;
        }
        Ok(())
    }
}

/// The selected pairs of a run, after the role checks (A14 range, §12.5 held-out refusal).
///
/// # Errors
/// A deck or split problem, or a refused role.
pub fn prepare(opts: &RunOptions) -> Result<Selection, String> {
    let selection = select(&opts.source)?;
    check_role(opts.role, opts.seed, &selection)?;
    Ok(selection)
}

/// The two agents of a row, with the spec texts the report shows.
#[derive(Clone, PartialEq, Debug)]
pub struct RowAgents {
    pub challenger_text: String,
    pub opponent_text: String,
    pub challenger: AgentDesc,
    pub opponent: AgentDesc,
}

/// A played row.
pub struct RowGames {
    pub specs: Vec<MirroredSpec>,
    pub counts: Vec<usize>,
    pub results: Vec<GameResult>,
}

impl RowGames {
    /// The outcomes, in spec order.
    #[must_use]
    pub fn outcomes(&self) -> Vec<GameOutcome> {
        self.results.iter().map(|r| r.outcome).collect()
    }

    /// The per-game seat efforts, in spec order.
    #[must_use]
    pub fn efforts(&self) -> Vec<[SeatEffort; 2]> {
        self.results.iter().map(|r| r.effort).collect()
    }
}

/// Plays one row: [`stratified_specs`] of the selection on `seed`, traced.
///
/// # Errors
/// A spec problem, the lowest failing game, or the interrupt.
pub fn play_row(
    agents: &RowAgents,
    selection: &Selection,
    opts: &RunOptions,
    seed: u64,
    cancel: &Cancel,
    progress: &mut dyn FnMut(usize),
) -> Result<RowGames, CommandError> {
    let (specs, counts) = stratified_specs(
        selection,
        opts.games,
        seed,
        &agents.challenger,
        &agents.opponent,
    )
    .map_err(CommandError::Invalid)?;
    let results = play_all_traced(&specs, opts.workers, opts.choose_first, cancel, progress)
        .map_err(CommandError::from)?;
    Ok(RowGames {
        specs,
        counts,
        results,
    })
}

/// A progress printer over `total` games.
pub fn progress_printer<'a>(err: &'a mut dyn Write, tty: bool) -> ProgressPrinter<'a> {
    ProgressPrinter::new(err, tty, "games")
}

fn get<'a>(block: &'a Json, key: &str) -> Option<&'a Json> {
    block.get(key)
}

fn fmt3(v: Option<&Json>) -> String {
    match v {
        Some(Json::Float(x)) => format_fixed(*x, 3),
        Some(Json::Int(i)) => format!("{i}.000"),
        _ => "n/a".to_owned(),
    }
}

fn fmt_pair(v: Option<&Json>) -> String {
    match v {
        Some(Json::Array(xs)) if xs.len() == 2 => {
            format!("[{}, {}]", fmt3(xs.first()), fmt3(xs.get(1)))
        }
        _ => "[n/a]".to_owned(),
    }
}

fn fmt2(v: Option<&Json>) -> String {
    match v {
        Some(Json::Float(x)) => format_fixed(*x, 1),
        Some(Json::Int(i)) => format!("{i}.0"),
        _ => "n/a".to_owned(),
    }
}

/// The §12.4 summary of a result block: `wins`, `n`, `rate`, `ci95`, `difference`,
/// `difference_ci95`, `discordant` (`[challenger_only, opponent_only]`), `iterations_per_game`
/// and `applies_per_game` (`{"challenger", "opponent"}`), and `text`, the same as one line.
#[must_use]
pub fn summary(block: &Json) -> Json {
    let paired = get(block, "paired").cloned().unwrap_or(Json::Null);
    let effort = get(block, "effort").cloned().unwrap_or(Json::Null);
    let per_game = |key: &str| {
        let mut o = Json::object();
        for side in ["challenger", "opponent"] {
            let v = effort
                .get(side)
                .and_then(|e| e.get(key))
                .cloned()
                .unwrap_or(Json::Null);
            o.set(side, v);
        }
        o
    };
    let mut s = Json::object();
    for key in ["wins", "n", "rate", "ci95"] {
        s.set(key, get(block, key).cloned().unwrap_or(Json::Null));
    }
    s.set(
        "difference",
        paired.get("difference").cloned().unwrap_or(Json::Null),
    );
    s.set(
        "difference_ci95",
        paired.get("ci95").cloned().unwrap_or(Json::Null),
    );
    let disc = Json::Array(vec![
        paired.get("challenger_only").cloned().unwrap_or(Json::Null),
        paired.get("opponent_only").cloned().unwrap_or(Json::Null),
    ]);
    s.set("discordant", disc);
    let iterations = per_game("iterations_per_game");
    let applies = per_game("applies_per_game");
    let text = format!(
        "{}/{} = {} {}; paired difference {} {}; discordant {}/{}; iterations per game {} vs {}; applies per game {} vs {}",
        get(block, "wins").map_or_else(|| "?".to_owned(), json_text),
        get(block, "n").map_or_else(|| "?".to_owned(), json_text),
        fmt3(get(block, "rate")),
        fmt_pair(get(block, "ci95")),
        fmt3(paired.get("difference")),
        fmt_pair(paired.get("ci95")),
        paired
            .get("challenger_only")
            .map_or_else(|| "?".to_owned(), json_text),
        paired
            .get("opponent_only")
            .map_or_else(|| "?".to_owned(), json_text),
        fmt2(iterations.get("challenger")),
        fmt2(iterations.get("opponent")),
        fmt2(applies.get("challenger")),
        fmt2(applies.get("opponent")),
    );
    s.set("iterations_per_game", iterations);
    s.set("applies_per_game", applies);
    s.set("text", text);
    s
}

fn json_text(v: &Json) -> String {
    dumps(
        v,
        DumpOptions {
            indent: None,
            sort_keys: false,
            compact: true,
            ensure_ascii: false,
        },
    )
}

/// The row's report keys: `challenger`, `opponent` (spec texts), `agents` (names), `summary`,
/// `result`, `held_out`, `by_pair`, `by_deck`, `unfinished`, `outcomes`.
pub fn row_into(
    doc: &mut Json,
    agents: &RowAgents,
    selection: &Selection,
    seed: u64,
    games: &RowGames,
) {
    let outcomes = games.outcomes();
    let efforts = games.efforts();
    let played = Played {
        selection,
        specs: &games.specs,
        counts: &games.counts,
        outcomes: &outcomes,
        efforts: Some(&efforts),
    };
    let b = played.blocks(seed);
    doc.set("challenger", agents.challenger_text.as_str());
    doc.set("opponent", agents.opponent_text.as_str());
    let mut names = Json::object();
    names.set("challenger", agent_name(&agents.challenger));
    names.set("opponent", agent_name(&agents.opponent));
    doc.set("agents", names);
    doc.set("summary", summary(&b.result));
    doc.set("result", b.result);
    doc.set("held_out", b.held_out);
    doc.set("by_pair", b.by_pair);
    doc.set("by_deck", b.by_deck);
    doc.set("unfinished", b.unfinished);
    doc.set("outcomes", b.outcomes);
}

/// `v` without any `outcomes` key, at any depth (what stdout shows).
#[must_use]
pub fn without_outcomes(v: &Json) -> Json {
    match v {
        Json::Object(items) => Json::Object(
            items
                .iter()
                .filter(|(k, _)| *k != Key::from("outcomes"))
                .map(|(k, x)| (k.clone(), without_outcomes(x)))
                .collect(),
        ),
        Json::Array(xs) => Json::Array(xs.iter().map(without_outcomes).collect()),
        other => other.clone(),
    }
}

/// Writes a report: the whole document to `out` (`json.dumps(indent=1)`, as head-to-head's
/// `--out`), and to stdout without the per-game rows (`indent=2`).
///
/// # Errors
/// The file cannot be written.
pub fn emit(
    doc: &Json,
    out_file: Option<&PathBuf>,
    out: &mut dyn Write,
) -> Result<(), CommandError> {
    let indented = |n: usize| DumpOptions {
        indent: Some(n),
        sort_keys: false,
        compact: false,
        ensure_ascii: true,
    };
    if let Some(path) = out_file {
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir).map_err(|e| {
                CommandError::Invalid(format!("cannot create {}: {e}", py_path_str(dir)))
            })?;
        }
        std::fs::write(path, dumps(doc, indented(1)) + "\n").map_err(|e| {
            CommandError::Invalid(format!("cannot write {}: {e}", py_path_str(path)))
        })?;
    }
    let _ = out.write_all((dumps(&without_outcomes(doc), indented(2)) + "\n").as_bytes());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_summary_reads_the_result_block() {
        let mut block = crate::stats::rate_json(3, 4);
        let mut paired = Json::object();
        paired.set("challenger_only", 2_u64);
        paired.set("opponent_only", 1_u64);
        paired.set("difference", 0.25);
        paired.set("ci95", vec![-0.1, 0.5]);
        block.set("paired", paired);
        let mut effort = Json::object();
        let mut c = Json::object();
        c.set("iterations_per_game", 10.5);
        c.set("applies_per_game", 99.25);
        effort.set("challenger", c);
        let mut o = Json::object();
        o.set("iterations_per_game", Json::Null);
        o.set("applies_per_game", Json::Null);
        effort.set("opponent", o);
        block.set("effort", effort);
        let s = summary(&block);
        assert_eq!(
            s.get("text").cloned(),
            Some(Json::from(
                "3/4 = 0.750 [0.301, 0.954]; paired difference 0.250 [-0.100, 0.500]; \
                 discordant 2/1; iterations per game 10.5 vs n/a; applies per game 99.2 vs n/a"
            ))
        );
        assert_eq!(
            s.get("discordant"),
            Some(&Json::Array(vec![2_u64.into(), 1_u64.into()]))
        );
    }
}
