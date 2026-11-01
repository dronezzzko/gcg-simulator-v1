//! `gcg-eval compare`: the exact port of `python -m gcg_sim.ai.compare` (`ai/compare.py`):
//! mirrored blocks of four on every pair of the decks, the challenger's wins with Wilson
//! intervals by seat and deck, and the report JSON (`--out`: `json.dumps(report, indent=1)`;
//! stdout: the report without `outcomes`, `indent=2`).
//!
//! `--games`, `--seed` and `--workers` are Python ints (`type=int`). The seed is echoed as
//! given and masked to 64 bits by `derive_seed`, as Python does; this port holds the three in
//! an `i128`, which bounds them (Python's ints are unbounded), and holds `games` in a `u32`.

use std::path::Path;

use gcg_core::pool::Cancel;
use gcg_core::pyjson::{DumpOptions, Json, dumps};
use gcg_runner::Workers;
use gcg_runner::argparse::{PyInt, PyStr};

use crate::agent_spec::parse_agent;
use crate::bench_cmd::CommandError;
use crate::decks::compare_decks;
use crate::selfplay::play_all;
use crate::specs::comparison_specs;
use crate::stats::summarize;
use crate::{ChooseFirst, GameOutcome, MirroredSpec};

/// `json.dumps(obj, indent=n)`: insertion order, `ensure_ascii=True`.
#[must_use]
pub const fn indented(indent: usize) -> DumpOptions {
    DumpOptions {
        indent: Some(indent),
        sort_keys: false,
        compact: false,
        ensure_ascii: true,
    }
}

/// A Python int argument held as `i128`.
///
/// # Errors
/// `{value} is outside the range this port supports`.
pub fn int_arg(v: &PyInt) -> Result<i128, String> {
    v.to_decimal()
        .parse::<i128>()
        .map_err(|_| format!("{} is outside the range this port supports", v.to_decimal()))
}

/// `master & (2**64 - 1)` (`rng.py:52-59`).
#[must_use]
pub const fn mask64(v: i128) -> u64 {
    (v as u128 & 0xFFFF_FFFF_FFFF_FFFF) as u64
}

/// Plays the specs; the real one is [`crate::selfplay::play_all`], tests inject others.
pub type Player<'a> = dyn FnMut(&[MirroredSpec]) -> Result<Vec<GameOutcome>, CommandError> + 'a;

/// `compare(challenger, opponent, games, seed, workers, decks)` (`ai/compare.py:181-196`) with
/// the games played by `play`. Keys: `challenger`, `opponent`, `decks`, `seed`, `games`,
/// `result`, `outcomes` (`[challenger, winner, turns, first_player]` per game).
///
/// # Errors
/// The `ValueError` texts of `parse_agent`, `load_decks` and `comparison_specs`, in that
/// order, and a failed game.
pub fn compare_with(
    challenger: &str,
    opponent: &str,
    games: i128,
    seed: i128,
    decks: &str,
    play: &mut Player<'_>,
) -> Result<Json, CommandError> {
    let invalid = CommandError::Invalid;
    let a = parse_agent(challenger).map_err(invalid)?;
    let b = parse_agent(opponent).map_err(invalid)?;
    let deck_lists = compare_decks(decks).map_err(invalid)?;
    let n = if games <= 0 {
        0
    } else {
        u32::try_from(games).map_err(|_| {
            invalid(format!(
                "games {games} is outside the range this port supports"
            ))
        })?
    };
    let specs = comparison_specs(n, mask64(seed), &a, &b, &deck_lists)
        .map_err(|e| invalid(e.to_string()))?;
    let outcomes = play(&specs)?;
    let mut report = Json::object();
    report.set("challenger", challenger);
    report.set("opponent", opponent);
    report.set("decks", decks);
    report.set("seed", Json::Int(seed));
    report.set("games", Json::Int(games));
    report.set("result", summarize(&specs, &outcomes));
    report.set(
        "outcomes",
        Json::Array(
            outcomes
                .iter()
                .map(|o| {
                    Json::Array(vec![
                        o.challenger.into(),
                        o.winner.into(),
                        o.turns.into(),
                        o.first_player.into(),
                    ])
                })
                .collect(),
        ),
    );
    Ok(report)
}

/// `compare(challenger, opponent, games, seed, workers, decks)` (`ai/compare.py:181-196`):
/// [`compare_with`] on real games, played by `play_all` with Python's play/draw protocol.
///
/// # Errors
/// As [`compare_with`]; a failed game or the interrupt.
pub fn compare(
    challenger: &str,
    opponent: &str,
    games: i128,
    seed: i128,
    workers: Workers,
    decks: &str,
    cancel: &Cancel,
) -> Result<Json, CommandError> {
    let mut play = |specs: &[MirroredSpec]| {
        play_all(specs, workers, ChooseFirst::Agents, cancel, &mut |_| {})
            .map_err(CommandError::from)
    };
    compare_with(challenger, opponent, games, seed, decks, &mut play)
}

/// The two outputs of `main` (`ai/compare.py:212-214`): the `--out` text and stdout.
#[must_use]
pub fn outputs(report: &Json) -> (String, String) {
    let file = dumps(report, indented(1)) + "\n";
    let shown = match report {
        Json::Object(items) => Json::Object(
            items
                .iter()
                .filter(|(k, _)| *k != gcg_core::pyjson::Key::from("outcomes"))
                .cloned()
                .collect(),
        ),
        other => other.clone(),
    };
    (file, dumps(&shown, indented(2)) + "\n")
}

/// The arguments of `main` (`ai/compare.py:199-207`).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct CompareArgs {
    pub challenger: String,
    pub opponent: String,
    pub games: i128,
    pub seed: i128,
    pub workers: i128,
    pub decks: String,
    /// `str(Path(text))`.
    pub out: Option<PyStr>,
}

impl CompareArgs {
    /// The worker count of `play_all` (`workers <= 1` plays serially).
    #[must_use]
    pub fn workers(&self) -> Workers {
        Workers::Count(u32::try_from(self.workers.max(1)).unwrap_or(u32::MAX))
    }
}

/// `main(argv)` after parsing (`ai/compare.py:208-214`): the report, then the `--out` file,
/// then stdout.
///
/// # Errors
/// As [`compare_with`], or the `--out` file cannot be written.
pub fn run(args: &CompareArgs, play: &mut Player<'_>) -> Result<String, CommandError> {
    let report = compare_with(
        &args.challenger,
        &args.opponent,
        args.games,
        args.seed,
        &args.decks,
        play,
    )?;
    let (file, stdout) = outputs(&report);
    if let Some(out) = &args.out {
        let path = out.to_os_string();
        std::fs::write(Path::new(&path), file).map_err(|e| {
            CommandError::Invalid(format!(
                "cannot write {}: {e}",
                out.to_string_opt().unwrap_or_default()
            ))
        })?;
    }
    Ok(stdout)
}
