//! `gcg-eval head-to-head` (docs/port/PLAN.md §10.4 F, §12.6, A24, A26): any two agent specs
//! in mirrored blocks of four on named deck pairs, with the challenger's Wilson intervals per
//! pair, pooled over every pair and pooled over the held-out pairs alone, the W-42 paired
//! statistics, and the realised play/draw counts per agent.
//!
//! Pairs come from a deck set (`--decks`, all combinations in `itertools.combinations` order,
//! filtered by `--pairs all|tuning|held-out`) or from `--pair A:B[,C:D...]`. Pair `p` plays
//! `games / pairs` games on `derive_seed(seed, "pairing", p)`, `p` being the pair's position in
//! the set's combination list (or in the `--pair` list), so a pair plays the same games in
//! every selection of its set; with a single pair the master is the seed itself
//! (`ai/compare.py:136-140`). Over all pairs of a set this is exactly `comparison_specs`.
//!
//! `--choose-first seat0` (the default) answers CHOOSE_FIRST with `go_first(0)`: each agent is
//! on the play in exactly two games of every block of four, with each deck once on the play and
//! once on the draw (A24, T-149). `agents` keeps Python's protocol (the W-44 diagnostic).
//!
//! The report (`--out`: `json.dumps(indent=1)`; stdout: without `outcomes`, `indent=2`) has
//! the keys `format`, `version`, `challenger`, `opponent` (the spec texts), `agents` (their
//! names), `decks`, `pairs`, `seed`, `role`, `games`, `choose_first`, `result` (all pairs),
//! `held_out` (a result block over the held-out pairs plus their count `pairs`, or null when
//! none was played; A26), `by_pair` (`decks`, `position`,
//! `held_out`, `master`, `result` per pair), `unfinished` (games the decision limit stopped,
//! W-46) and `outcomes`. A `result` block is `ai/compare.py`'s `summarize` (wins, Wilson
//! interval, draws, mean turns, by seat, by deck) plus `draw_rate`, `paired` (W-42: the deals'
//! 2x2 table, the paired difference and its Newcombe interval) and `play_draw` (per agent, its
//! games and wins on the play and on the draw, A24). Draws are non-wins (W-93).

use std::path::PathBuf;

use gcg_core::pyjson::Json;
use gcg_data::DeckList;
use gcg_runner::Workers;

use crate::agent_spec::{agent_name, parse_agent_spec};
use crate::decks::{NamedDeck, PANEL_FILE, Panel, load_decks, resolve_deck};
use crate::ledger::{self, Entry};
use crate::specs::{combinations, pair_specs, pairing_master};
use crate::stats::{Paired, PlayDraw, summarize};
use crate::{
    AgentDesc, ChooseFirst, DECISION_LIMIT, DeckSet, GameOutcome, MirroredSpec, PairSet, SeedRole,
};

/// The report format name and version.
pub const REPORT_FORMAT: &str = "gcg-eval-head-to-head";
pub const REPORT_VERSION: u32 = 1;

/// Where the pairs come from.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum PairSource {
    Set {
        decks: DeckSet,
        pairs: PairSet,
    },
    /// `--pair A:B[,C:D...]`, every occurrence of the option joined with `,` in order.
    Named(String),
}

/// The parsed arguments.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct HeadToHeadArgs {
    pub challenger: String,
    pub opponent: String,
    pub source: PairSource,
    pub games: u32,
    pub seed: u64,
    pub role: Option<SeedRole>,
    pub ledger: PathBuf,
    pub workers: Workers,
    pub choose_first: ChooseFirst,
    pub out: Option<PathBuf>,
}

/// One selected pair.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SelectedPair {
    /// The pairing index `p` of its master seed.
    pub position: usize,
    pub decks: [NamedDeck; 2],
    pub held_out: bool,
}

/// The pairs of a run and the number of pairings their positions count in.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Selection {
    pub pairs: Vec<SelectedPair>,
    pub pairings_total: usize,
}

fn panel_if_present() -> Result<Option<Panel>, String> {
    let path = std::path::Path::new(PANEL_FILE);
    if path.exists() {
        Panel::load(path).map(Some)
    } else {
        Ok(None)
    }
}

/// The pairs of `source`, with the held-out split of `docs/port/eval/panel.json` when it
/// exists (`--pairs tuning|held-out` require it).
///
/// # Errors
/// As [`select_with`], or a malformed or missing split that is needed.
pub fn select(source: &PairSource) -> Result<Selection, String> {
    let panel = panel_if_present()?;
    if let PairSource::Set { pairs, .. } = source
        && *pairs != PairSet::All
        && panel.is_none()
    {
        return Err(format!(
            "--pairs {} needs the held-out split in {PANEL_FILE}",
            pairs.as_str()
        ));
    }
    select_with(source, panel.as_ref())
}

/// The pairs of `source` under the held-out split `panel` (none: no pair is held out).
///
/// # Errors
/// A deck that does not load, or a malformed `--pair` list.
pub fn select_with(source: &PairSource, panel: Option<&Panel>) -> Result<Selection, String> {
    let held =
        |a: &NamedDeck, b: &NamedDeck| panel.is_some_and(|p| p.is_held_out(&a.name, &b.name));
    match source {
        PairSource::Named(list) => {
            let mut pairs = Vec::new();
            for (position, item) in list.split(',').enumerate() {
                let (a, b) = item
                    .split_once(':')
                    .ok_or_else(|| format!("--pair expects DECK_A:DECK_B, got {item:?}"))?;
                let decks = [resolve_deck(a)?, resolve_deck(b)?];
                let held_out = held(&decks[0], &decks[1]);
                pairs.push(SelectedPair {
                    position,
                    decks,
                    held_out,
                });
            }
            let pairings_total = pairs.len();
            Ok(Selection {
                pairs,
                pairings_total,
            })
        }
        PairSource::Set { decks, pairs } => {
            let named = load_decks(*decks)?;
            let all = combinations(named.len());
            let pairings_total = all.len();
            let selected = all
                .into_iter()
                .enumerate()
                .filter_map(|(position, (a, b))| {
                    let held_out = held(&named[a], &named[b]);
                    let keep = match pairs {
                        PairSet::All => true,
                        PairSet::Tuning => !held_out,
                        PairSet::HeldOut => held_out,
                    };
                    keep.then(|| SelectedPair {
                        position,
                        decks: [named[a].clone(), named[b].clone()],
                        held_out,
                    })
                })
                .collect();
            Ok(Selection {
                pairs: selected,
                pairings_total,
            })
        }
    }
}

/// The specs of a selection (see the module docs).
///
/// # Errors
/// `comparison_specs`' checks over the selected pairs.
pub fn specs(
    selection: &Selection,
    games: u32,
    seed: u64,
    challenger: &AgentDesc,
    opponent: &AgentDesc,
) -> Result<Vec<MirroredSpec>, String> {
    let pairs: Vec<(usize, [&DeckList; 2])> = selection
        .pairs
        .iter()
        .map(|p| (p.position, [&p.decks[0].deck, &p.decks[1].deck]))
        .collect();
    pair_specs(
        games,
        seed,
        challenger,
        opponent,
        &pairs,
        selection.pairings_total,
    )
    .map_err(|e| e.to_string())
}

/// Games the decision limit stopped (`DECISION_LIMIT` decisions without a winner; W-46 lists
/// them instead of hiding them among the draws).
#[must_use]
pub fn unfinished(outcomes: &[GameOutcome]) -> Vec<u32> {
    outcomes
        .iter()
        .filter(|o| o.decisions >= DECISION_LIMIT && o.winner == -1)
        .map(|o| o.index)
        .collect()
}

/// The share of drawn games, 4 places (W-93: shown next to the wins-only rate in gcg-eval
/// output); `None` without games.
fn draw_rate(outcomes: &[GameOutcome]) -> Option<f64> {
    let draws = outcomes.iter().filter(|o| o.winner == -1).count();
    let n = outcomes.len();
    (n > 0).then(|| {
        let as_u = |x: usize| u64::try_from(x).unwrap_or(u64::MAX);
        gcg_core::pyfloat::round_f64(
            gcg_runner::reports::stats::div(as_u(draws), as_u(n)),
            crate::stats::RATE_DIGITS,
        )
    })
}

/// The result block of a group of games: `summarize`'s keys, then `paired` and `play_draw`.
fn group(specs: &[MirroredSpec], outcomes: &[GameOutcome]) -> Json {
    let mut block = summarize(specs, outcomes);
    block.set("draw_rate", draw_rate(outcomes));
    block.set("paired", Paired::of(specs, outcomes).to_json());
    let mut pd = Json::object();
    pd.set("challenger", PlayDraw::of(outcomes, true).to_json());
    pd.set("opponent", PlayDraw::of(outcomes, false).to_json());
    block.set("play_draw", pd);
    block
}

/// The head-to-head report (keys in the module docs); `outcomes` holds
/// `[index, pair, challenger, winner, turns, first_player, decisions]` per game.
#[must_use]
pub fn report(
    args: &HeadToHeadArgs,
    names: [&str; 2],
    selection: &Selection,
    specs: &[MirroredSpec],
    outcomes: &[GameOutcome],
) -> Json {
    let per_pair = specs.len() / selection.pairs.len().max(1);
    let mut doc = Json::object();
    doc.set("format", REPORT_FORMAT);
    doc.set("version", REPORT_VERSION);
    doc.set("challenger", args.challenger.as_str());
    doc.set("opponent", args.opponent.as_str());
    let mut agents = Json::object();
    agents.set("challenger", names[0]);
    agents.set("opponent", names[1]);
    doc.set("agents", agents);
    match &args.source {
        PairSource::Set { decks, pairs } => {
            doc.set("decks", decks.as_str());
            doc.set("pairs", pairs.as_str());
        }
        PairSource::Named(list) => {
            doc.set("decks", Json::Null);
            doc.set("pairs", list.as_str());
        }
    }
    doc.set("seed", args.seed);
    doc.set("role", args.role.map(SeedRole::as_str));
    doc.set("games", args.games);
    doc.set("choose_first", args.choose_first.as_str());
    doc.set("result", group(specs, outcomes));

    let held: Vec<usize> = (0..specs.len())
        .filter(|i| selection.pairs[i / per_pair.max(1)].held_out)
        .collect();
    let held_block = if held.is_empty() {
        Json::Null
    } else {
        let s: Vec<MirroredSpec> = held.iter().map(|&i| specs[i].clone()).collect();
        let o: Vec<GameOutcome> = held.iter().map(|&i| outcomes[i]).collect();
        let mut b = group(&s, &o);
        b.set(
            "pairs",
            selection.pairs.iter().filter(|p| p.held_out).count(),
        );
        b
    };
    doc.set("held_out", held_block);

    let mut rows = Vec::new();
    for (k, pair) in selection.pairs.iter().enumerate() {
        let range = k * per_pair..(k + 1) * per_pair;
        let mut row = Json::object();
        row.set(
            "decks",
            vec![pair.decks[0].name.as_str(), pair.decks[1].name.as_str()],
        );
        row.set("position", pair.position);
        row.set("held_out", pair.held_out);
        row.set(
            "master",
            pairing_master(args.seed, pair.position, selection.pairings_total),
        );
        row.set("result", group(&specs[range.clone()], &outcomes[range]));
        rows.push(row);
    }
    doc.set("by_pair", Json::Array(rows));
    doc.set(
        "unfinished",
        Json::Array(unfinished(outcomes).into_iter().map(Json::from).collect()),
    );
    doc.set(
        "outcomes",
        Json::Array(
            outcomes
                .iter()
                .enumerate()
                .map(|(i, o)| {
                    Json::Array(vec![
                        o.index.into(),
                        selection.pairs[i / per_pair.max(1)].position.into(),
                        o.challenger.into(),
                        o.winner.into(),
                        o.turns.into(),
                        o.first_player.into(),
                        o.decisions.into(),
                    ])
                })
                .collect(),
        ),
    );
    doc
}

/// Everything a run needs before it plays: the agents, the pairs, the specs, and the role
/// checks (A14, §12.5).
pub struct Plan {
    pub challenger: AgentDesc,
    pub opponent: AgentDesc,
    pub selection: Selection,
    pub specs: Vec<MirroredSpec>,
}

/// Parses the agents, selects the pairs, builds the specs and checks the role.
///
/// # Errors
/// The first problem, in that order.
pub fn plan(args: &HeadToHeadArgs) -> Result<Plan, String> {
    let challenger = parse_agent_spec(&args.challenger)?;
    let opponent = parse_agent_spec(&args.opponent)?;
    let selection = select(&args.source)?;
    if let Some(role) = args.role {
        ledger::check_master(role, args.seed)?;
        ledger::check_pairs(
            role,
            selection
                .pairs
                .iter()
                .filter(|p| p.held_out)
                .map(|p| (p.decks[0].name.as_str(), p.decks[1].name.as_str())),
        )?;
    }
    let specs = specs(&selection, args.games, args.seed, &challenger, &opponent)?;
    Ok(Plan {
        challenger,
        opponent,
        selection,
        specs,
    })
}

/// The ledger entry of a finished role run.
#[must_use]
pub fn ledger_entry(args: &HeadToHeadArgs, role: SeedRole) -> Entry {
    let mut config = Json::object();
    config.set("challenger", args.challenger.as_str());
    config.set("opponent", args.opponent.as_str());
    match &args.source {
        PairSource::Set { decks, pairs } => {
            config.set("decks", decks.as_str());
            config.set("pairs", pairs.as_str());
        }
        PairSource::Named(list) => config.set("pair", list.as_str()),
    }
    config.set("games", args.games);
    config.set("choose_first", args.choose_first.as_str());
    Entry {
        role,
        master: args.seed,
        command: "head-to-head",
        config,
    }
}

/// The agents' names, for the report.
#[must_use]
pub fn names(plan: &Plan) -> [String; 2] {
    [agent_name(&plan.challenger), agent_name(&plan.opponent)]
}
