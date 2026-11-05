//! `gcg-eval head-to-head` (docs/port/PLAN.md §10.4 F, §12.6, A24, A26): any two agent specs
//! in mirrored blocks of four on named deck pairs, with the challenger's Wilson intervals per
//! pair, pooled over every pair and pooled over the held-out pairs alone, the W-42 paired
//! statistics, the realised play/draw counts per agent, and the agents' search effort.
//!
//! Pairs come from a deck set (`--decks`, all combinations in `itertools.combinations` order,
//! filtered by `--pairs all|tuning|held-out`) or from `--pair A:B[,C:D...]`. Pair `p` plays
//! `games / pairs` games on `derive_seed(seed, "pairing", p)`, `p` being the pair's position in
//! the set's combination list (or in the `--pair` list), so a pair plays the same games in
//! every selection of its set; with a single pair the master is the seed itself
//! (`ai/compare.py:136-140`). Over all pairs of a set this is exactly `comparison_specs`.
//! The Phase 6 row tools (`ablation`, `budget-curve`) allocate blocks to pairs round-robin
//! instead ([`stratified_specs`]), so 400 games stratify over 15 pairs.
//!
//! `--choose-first seat0` (the default) answers CHOOSE_FIRST with `go_first(0)`: each agent is
//! on the play in exactly two games of every block of four, with each deck once on the play and
//! once on the draw (A24, T-149). `agents` keeps Python's protocol (the W-44 diagnostic).
//!
//! The report (`--out`: `json.dumps(indent=1)`; stdout: without `outcomes`, `indent=2`) has
//! the keys `format`, `version`, `challenger`, `opponent` (the spec texts), `agents` (their
//! names), `decks`, `pairs`, `seed`, `role`, `games`, `choose_first`, `result` (all pairs),
//! `held_out` (a result block over the held-out pairs plus their count `pairs`, or null when
//! none was played; A26), `by_pair` (`decks`, `position`, `held_out`, `master`, `result` per
//! pair), `by_deck` (per deck name, the challenger's games with that deck, W-20, W-123),
//! `unfinished` (games the decision limit stopped, W-46) and `outcomes`. A `result` block is
//! `ai/compare.py`'s `summarize` (wins, Wilson interval, draws, mean turns, by seat, by deck)
//! plus `draw_rate`, `paired` (W-42: the deals' 2x2 table, the paired difference with its
//! Newcombe interval and a seeded bootstrap interval over deals, McNemar's discordant
//! counts), `play_draw` (per agent, its games and wins on the play and on the draw, A24) and `effort` (per agent: decisions, searches, iterations
//! and engine applies, and their means per game, null when the agent reports no search;
//! PLAN §12.4). Pooled blocks over several pairs add `stratified`: the pair-stratified rate
//! and its normal interval `sum w_k p_k +- 1.96 sqrt(sum w_k^2 p_k (1 - p_k) / n_k)` with
//! `w_k = n_k / n` (W-20). Draws are non-wins (W-93).

use std::path::PathBuf;

use gcg_core::pyfloat::round_f64;
use gcg_core::pyjson::Json;
use gcg_data::DeckList;
use gcg_runner::Workers;

use crate::agent_spec::{agent_name, parse_agent_spec};
use crate::decks::{NamedDeck, PANEL_FILE, Panel, load_decks, resolve_deck};
use crate::ledger::{self, Entry};
use crate::selfplay::{GameResult, SeatEffort};
use crate::specs::{combinations, mirrored_specs, pair_specs, pairing_master};
use crate::stats::{Paired, PlayDraw, RATE_DIGITS, challenger_won, rate_json, summarize};
use crate::{
    AgentDesc, ChooseFirst, DECISION_LIMIT, DeckSet, GameOutcome, MIRROR_BLOCK, MirroredSpec,
    PairSet, SeedRole, SpecError,
};

/// The report format name and version (2: `by_deck`, `effort`, `stratified`, per-game effort
/// in `outcomes`).
pub const REPORT_FORMAT: &str = "gcg-eval-head-to-head";
pub const REPORT_VERSION: u32 = 2;

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

impl PairSource {
    /// The report's `decks` and `pairs` values.
    #[must_use]
    pub fn report_values(&self) -> (Json, Json) {
        match self {
            Self::Set { decks, pairs } => (decks.as_str().into(), pairs.as_str().into()),
            Self::Named(list) => (Json::Null, list.as_str().into()),
        }
    }

    /// The ledger configuration entries of the source.
    pub fn ledger_config(&self, config: &mut Json) {
        match self {
            Self::Set { decks, pairs } => {
                config.set("decks", decks.as_str());
                config.set("pairs", pairs.as_str());
            }
            Self::Named(list) => config.set("pair", list.as_str()),
        }
    }
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
    /// `--decision-trace FILE` (W-21): every decision's trace (`crate::trace`).
    pub decision_trace: Option<PathBuf>,
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

/// The held-out split of `docs/port/eval/panel.json` in the working directory, if it exists.
///
/// # Errors
/// A malformed file.
pub fn panel_if_present() -> Result<Option<Panel>, String> {
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

/// A deck list's cards as a multiset (sorted copies of `main` and `resources`).
fn sorted_list(deck: &DeckList) -> DeckList {
    let mut main = deck.main.to_vec();
    let mut resources = deck.resources.to_vec();
    main.sort();
    resources.sort();
    DeckList {
        main: main.into_boxed_slice(),
        resources: resources.into_boxed_slice(),
    }
}

/// The card multisets of the held-out panel decks, so that a held-out deck is held out under
/// any name or path (a renamed copy included; the split names decks, PLAN §12.5, A14).
fn held_out_lists(panel: Option<&Panel>) -> Result<Vec<DeckList>, String> {
    let Some(panel) = panel else {
        return Ok(Vec::new());
    };
    let names = panel.held_out_decks();
    Ok(load_decks(DeckSet::Panel)?
        .iter()
        .filter(|d| names.contains(&d.name))
        .map(|d| sorted_list(&d.deck))
        .collect())
}

/// The pairs of `source` under the held-out split `panel` (none: no pair is held out; a pair
/// is held out when the split names one of its decks or when one of its decks has the cards
/// of a held-out panel deck).
///
/// # Errors
/// A deck that does not load, or a malformed `--pair` list.
pub fn select_with(source: &PairSource, panel: Option<&Panel>) -> Result<Selection, String> {
    let held_lists = held_out_lists(panel)?;
    let held = |a: &NamedDeck, b: &NamedDeck| {
        panel.is_some_and(|p| p.is_held_out(&a.name, &b.name))
            || held_lists
                .iter()
                .any(|l| *l == sorted_list(&a.deck) || *l == sorted_list(&b.deck))
    };
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

/// The specs of a selection with an equal share of the games per pair (see the module docs).
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

/// The games per pair of [`stratified_specs`]: `games / 4` blocks dealt round-robin over the
/// pairs in selection order, so pair `k` of `P` plays `ceil((blocks - k) / P)` blocks.
///
/// # Errors
/// `games` is not a positive multiple of 4, or gives some pair no block.
pub fn stratified_counts(games: u32, pairs: usize) -> Result<Vec<usize>, String> {
    if pairs == 0 {
        return Err(SpecError::TooFewDecks.to_string());
    }
    if games == 0 || !games.is_multiple_of(MIRROR_BLOCK) {
        return Err(SpecError::GamesNotMultiple {
            multiple: MIRROR_BLOCK,
        }
        .to_string());
    }
    let blocks = usize::try_from(games / MIRROR_BLOCK).unwrap_or(usize::MAX);
    if blocks < pairs {
        return Err(format!(
            "games must be at least {} (one block of {MIRROR_BLOCK} per pair)",
            pairs.saturating_mul(4)
        ));
    }
    Ok((0..pairs)
        .map(|k| (blocks - k).div_ceil(pairs) * 4)
        .collect())
}

/// Mirrored specs with [`stratified_counts`] games per pair: pair `k` plays its games as
/// [`mirrored_specs`] on its pairing master, as in [`specs`], re-indexed in order. When every
/// pair gets the same share this equals [`specs`].
///
/// # Errors
/// As [`stratified_counts`].
pub fn stratified_specs(
    selection: &Selection,
    games: u32,
    seed: u64,
    challenger: &AgentDesc,
    opponent: &AgentDesc,
) -> Result<(Vec<MirroredSpec>, Vec<usize>), String> {
    let counts = stratified_counts(games, selection.pairs.len())?;
    let mut out = Vec::with_capacity(usize::try_from(games).unwrap_or(0));
    for (pair, &n) in selection.pairs.iter().zip(&counts) {
        let master = pairing_master(seed, pair.position, selection.pairings_total);
        let decks = [&pair.decks[0].deck, &pair.decks[1].deck];
        let n = u32::try_from(n).unwrap_or(u32::MAX);
        for mut s in mirrored_specs(n, master, challenger, opponent, decks) {
            s.index = u32::try_from(out.len()).unwrap_or(u32::MAX);
            out.push(s);
        }
    }
    Ok((out, counts))
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

fn as_u64(x: usize) -> u64 {
    u64::try_from(x).unwrap_or(u64::MAX)
}

#[expect(
    clippy::cast_precision_loss,
    reason = "counts stay far below 2^53, where the conversion is exact"
)]
fn ratio(num: u64, den: u64) -> f64 {
    num as f64 / den as f64
}

/// The share of drawn games, 4 places (W-93: shown next to the wins-only rate in gcg-eval
/// output); `None` without games.
fn draw_rate(outcomes: &[GameOutcome]) -> Option<f64> {
    let draws = outcomes.iter().filter(|o| o.winner == -1).count();
    let n = outcomes.len();
    (n > 0).then(|| round_f64(ratio(as_u64(draws), as_u64(n)), RATE_DIGITS))
}

/// One agent's effort over `games` games: the totals and, when it ran any search, the means
/// of iterations and applies per game (2 places; null otherwise: the agent reports none).
#[must_use]
pub fn effort_json(e: &SeatEffort, games: u64) -> Json {
    let mut o = Json::object();
    o.set("games", games);
    o.set("decisions", e.decisions);
    o.set("searches", e.searches);
    o.set("iterations", e.iterations);
    o.set("applies_tree", e.applies_tree);
    o.set("applies_rollout", e.applies_rollout);
    o.set("applies_aux", e.applies_aux);
    o.set("worlds", e.worlds);
    o.set("rollouts_capped", e.rollouts_capped);
    let reported = e.searches > 0 && games > 0;
    o.set(
        "iterations_per_game",
        reported.then(|| round_f64(ratio(e.iterations, games), 2)),
    );
    o.set(
        "applies_per_game",
        reported.then(|| round_f64(ratio(e.applies(), games), 2)),
    );
    o
}

/// The challenger's and the opponent's summed effort over a group of games.
#[must_use]
pub fn agent_efforts(outcomes: &[GameOutcome], efforts: &[[SeatEffort; 2]]) -> [SeatEffort; 2] {
    let mut out = [SeatEffort::default(); 2];
    for (o, e) in outcomes.iter().zip(efforts) {
        let c = usize::from(o.challenger);
        out[0] = out[0].plus(&e[c]);
        out[1] = out[1].plus(&e[1 - c]);
    }
    out
}

/// The result block of a group of games: `summarize`'s keys, then `draw_rate`, `paired`,
/// `play_draw` and `effort` (null without effort counts).
#[must_use]
pub fn group(
    specs: &[MirroredSpec],
    outcomes: &[GameOutcome],
    efforts: Option<&[[SeatEffort; 2]]>,
) -> Json {
    let mut block = summarize(specs, outcomes);
    block.set("draw_rate", draw_rate(outcomes));
    block.set(
        "paired",
        Paired::of(specs, outcomes).to_json_with_bootstrap(),
    );
    let mut pd = Json::object();
    pd.set("challenger", PlayDraw::of(outcomes, true).to_json());
    pd.set("opponent", PlayDraw::of(outcomes, false).to_json());
    block.set("play_draw", pd);
    let effort = efforts.map_or(Json::Null, |e| {
        let [c, o] = agent_efforts(outcomes, e);
        let n = as_u64(outcomes.len());
        let mut j = Json::object();
        j.set("challenger", effort_json(&c, n));
        j.set("opponent", effort_json(&o, n));
        j
    });
    block.set("effort", effort);
    block
}

/// The pair-stratified rate and its 95% normal interval over `(wins, n)` per pair (module
/// docs), bounds clipped to `[0, 1]`, 4 places; null without games.
#[must_use]
pub fn stratified_json(per_pair: &[(u64, u64)]) -> Json {
    let n: u64 = per_pair.iter().map(|(_, k)| k).sum();
    if n == 0 {
        return Json::Null;
    }
    let mut rate = 0.0;
    let mut var = 0.0;
    for &(w, k) in per_pair.iter().filter(|(_, k)| *k > 0) {
        let wk = ratio(k, n);
        let p = ratio(w, k);
        rate += wk * p;
        var += wk * wk * p * (1.0 - p) / ratio(k, 1);
    }
    let half = gcg_runner::reports::stats::Z95 * gcg_core::libm::sqrt(var);
    let lo = if rate - half > 0.0 { rate - half } else { 0.0 };
    let hi = if rate + half < 1.0 { rate + half } else { 1.0 };
    let mut o = Json::object();
    o.set("pairs", as_u64(per_pair.len()));
    o.set("rate", round_f64(rate, RATE_DIGITS));
    o.set(
        "ci95",
        vec![round_f64(lo, RATE_DIGITS), round_f64(hi, RATE_DIGITS)],
    );
    o
}

/// The played games of a selection, with each game's pair.
pub struct Played<'a> {
    pub selection: &'a Selection,
    pub specs: &'a [MirroredSpec],
    /// Games per selected pair, in order (they sum to `specs.len()`).
    pub counts: &'a [usize],
    pub outcomes: &'a [GameOutcome],
    pub efforts: Option<&'a [[SeatEffort; 2]]>,
}

/// The report blocks every row tool shares: `result`, `held_out`, `by_pair`, `by_deck`,
/// `unfinished` and the `outcomes` rows.
pub struct Blocks {
    pub result: Json,
    pub held_out: Json,
    pub by_pair: Json,
    pub by_deck: Json,
    pub unfinished: Json,
    pub outcomes: Json,
}

impl Played<'_> {
    fn ranges(&self) -> Vec<std::ops::Range<usize>> {
        let mut start = 0;
        self.counts
            .iter()
            .map(|&n| {
                let r = start..start + n;
                start += n;
                r
            })
            .collect()
    }

    fn sub(&self, idx: &[usize]) -> (Vec<MirroredSpec>, Vec<GameOutcome>, Vec<[SeatEffort; 2]>) {
        let s = idx.iter().map(|&i| self.specs[i].clone()).collect();
        let o = idx.iter().map(|&i| self.outcomes[i]).collect();
        let e = self
            .efforts
            .map(|e| idx.iter().map(|&i| e[i]).collect())
            .unwrap_or_default();
        (s, o, e)
    }

    fn pooled(&self, pairs: &[usize]) -> Json {
        let ranges = self.ranges();
        let idx: Vec<usize> = pairs.iter().flat_map(|&k| ranges[k].clone()).collect();
        let (s, o, e) = self.sub(&idx);
        let mut block = group(&s, &o, self.efforts.map(|_| e.as_slice()));
        if pairs.len() > 1 {
            let per: Vec<(u64, u64)> = pairs
                .iter()
                .map(|&k| {
                    let games = &self.outcomes[ranges[k].clone()];
                    (
                        as_u64(games.iter().filter(|g| challenger_won(g)).count()),
                        as_u64(games.len()),
                    )
                })
                .collect();
            block.set("stratified", stratified_json(&per));
        }
        block
    }

    /// The name of the deck the challenger played in game `i` of pair `k`.
    fn challenger_deck(&self, k: usize, i: usize) -> &str {
        let pair = &self.selection.pairs[k];
        let s = &self.specs[i];
        if s.decks[usize::from(s.challenger)] == pair.decks[0].deck {
            &pair.decks[0].name
        } else {
            &pair.decks[1].name
        }
    }

    /// The shared report blocks.
    #[must_use]
    pub fn blocks(&self, seed: u64) -> Blocks {
        let ranges = self.ranges();
        let all: Vec<usize> = (0..self.counts.len()).collect();
        let result = self.pooled(&all);
        let held: Vec<usize> = all
            .iter()
            .copied()
            .filter(|&k| self.selection.pairs[k].held_out)
            .collect();
        let held_out = if held.is_empty() {
            Json::Null
        } else {
            let mut b = self.pooled(&held);
            b.set("pairs", as_u64(held.len()));
            b
        };
        let mut rows = Vec::new();
        for (k, pair) in self.selection.pairs.iter().enumerate() {
            let r = ranges[k].clone();
            let mut row = Json::object();
            row.set(
                "decks",
                vec![pair.decks[0].name.as_str(), pair.decks[1].name.as_str()],
            );
            row.set("position", pair.position);
            row.set("held_out", pair.held_out);
            row.set(
                "master",
                pairing_master(seed, pair.position, self.selection.pairings_total),
            );
            let e = self.efforts.map(|e| &e[r.clone()]);
            row.set(
                "result",
                group(&self.specs[r.clone()], &self.outcomes[r], e),
            );
            rows.push(row);
        }
        let mut deck_names: Vec<String> = Vec::new();
        let mut deck_games: Vec<(u64, u64, usize)> = Vec::new();
        for (k, r) in ranges.iter().enumerate() {
            for i in r.clone() {
                let name = self.challenger_deck(k, i);
                let d = deck_names
                    .iter()
                    .position(|n| n == name)
                    .unwrap_or_else(|| {
                        deck_names.push(name.to_owned());
                        deck_games.push((0, 0, 0));
                        deck_names.len() - 1
                    });
                deck_games[d].0 += u64::from(challenger_won(&self.outcomes[i]));
                deck_games[d].1 += 1;
            }
            let pair = &self.selection.pairs[k];
            for (d, name) in deck_names.iter().enumerate() {
                if pair.decks[0].name == *name || pair.decks[1].name == *name {
                    deck_games[d].2 += 1;
                }
            }
        }
        let by_deck = Json::Array(
            deck_names
                .iter()
                .zip(&deck_games)
                .map(|(name, &(w, n, pairs))| {
                    let mut row = Json::object();
                    row.set("deck", name.as_str());
                    row.set("pairs", as_u64(pairs));
                    row.set("result", rate_json(w, n));
                    row
                })
                .collect(),
        );
        let unfinished = Json::Array(
            unfinished(self.outcomes)
                .into_iter()
                .map(Json::from)
                .collect(),
        );
        let mut games = Vec::with_capacity(self.outcomes.len());
        for (k, r) in ranges.iter().enumerate() {
            let position = self.selection.pairs[k].position;
            for i in r.clone() {
                let o = &self.outcomes[i];
                let mut row: Vec<Json> = vec![
                    o.index.into(),
                    position.into(),
                    o.challenger.into(),
                    o.winner.into(),
                    o.turns.into(),
                    o.first_player.into(),
                    o.decisions.into(),
                ];
                if let Some(e) = self.efforts {
                    let c = usize::from(o.challenger);
                    row.extend([
                        e[i][c].iterations.into(),
                        e[i][c].applies().into(),
                        e[i][1 - c].iterations.into(),
                        e[i][1 - c].applies().into(),
                    ]);
                }
                games.push(Json::Array(row));
            }
        }
        Blocks {
            result,
            held_out,
            by_pair: Json::Array(rows),
            by_deck,
            unfinished,
            outcomes: Json::Array(games),
        }
    }
}

/// Equal games per pair, as [`specs`] deals them.
#[must_use]
pub fn equal_counts(selection: &Selection, games: usize) -> Vec<usize> {
    let n = selection.pairs.len().max(1);
    vec![games / n; selection.pairs.len()]
}

/// The head-to-head report (keys in the module docs) of games without effort counts;
/// `outcomes` holds `[index, pair, challenger, winner, turns, first_player, decisions]` per
/// game.
#[must_use]
pub fn report(
    args: &HeadToHeadArgs,
    names: [&str; 2],
    selection: &Selection,
    specs: &[MirroredSpec],
    outcomes: &[GameOutcome],
) -> Json {
    let counts = equal_counts(selection, specs.len());
    report_played(
        args,
        names,
        &Played {
            selection,
            specs,
            counts: &counts,
            outcomes,
            efforts: None,
        },
    )
}

/// The head-to-head report of played games; with effort counts each `outcomes` row adds the
/// challenger's iterations and applies, then the opponent's.
#[must_use]
pub fn report_played(args: &HeadToHeadArgs, names: [&str; 2], played: &Played<'_>) -> Json {
    let mut doc = Json::object();
    doc.set("format", REPORT_FORMAT);
    doc.set("version", REPORT_VERSION);
    doc.set("challenger", args.challenger.as_str());
    doc.set("opponent", args.opponent.as_str());
    let mut agents = Json::object();
    agents.set("challenger", names[0]);
    agents.set("opponent", names[1]);
    doc.set("agents", agents);
    let (decks, pairs) = args.source.report_values();
    doc.set("decks", decks);
    doc.set("pairs", pairs);
    doc.set("seed", args.seed);
    doc.set("role", args.role.map(SeedRole::as_str));
    doc.set("games", args.games);
    doc.set("choose_first", args.choose_first.as_str());
    let b = played.blocks(args.seed);
    doc.set("result", b.result);
    doc.set("held_out", b.held_out);
    doc.set("by_pair", b.by_pair);
    doc.set("by_deck", b.by_deck);
    doc.set("unfinished", b.unfinished);
    doc.set("outcomes", b.outcomes);
    doc
}

/// The head-to-head report of [`GameResult`]s.
#[must_use]
pub fn report_results(
    args: &HeadToHeadArgs,
    names: [&str; 2],
    selection: &Selection,
    specs: &[MirroredSpec],
    counts: &[usize],
    results: &[GameResult],
) -> Json {
    let outcomes: Vec<GameOutcome> = results.iter().map(|r| r.outcome).collect();
    let efforts: Vec<[SeatEffort; 2]> = results.iter().map(|r| r.effort).collect();
    report_played(
        args,
        names,
        &Played {
            selection,
            specs,
            counts,
            outcomes: &outcomes,
            efforts: Some(&efforts),
        },
    )
}

/// Everything a run needs before it plays: the agents, the pairs, the specs and the games per
/// pair.
pub struct Plan {
    pub challenger: AgentDesc,
    pub opponent: AgentDesc,
    pub selection: Selection,
    pub specs: Vec<MirroredSpec>,
    pub counts: Vec<usize>,
}

/// The role checks of a run (A14, §12.5): the master lies in the role's range, and the roles
/// that steer the search play no held-out pair.
///
/// # Errors
/// The first problem.
pub fn check_role(role: Option<SeedRole>, seed: u64, selection: &Selection) -> Result<(), String> {
    if let Some(role) = role {
        ledger::check_master(role, seed)?;
        ledger::check_pairs(
            role,
            selection
                .pairs
                .iter()
                .filter(|p| p.held_out)
                .map(|p| (p.decks[0].name.as_str(), p.decks[1].name.as_str())),
        )?;
    }
    Ok(())
}

/// Parses the agents, selects the pairs, checks the role and builds the specs.
///
/// # Errors
/// The first problem, in that order.
pub fn plan(args: &HeadToHeadArgs) -> Result<Plan, String> {
    let challenger = parse_agent_spec(&args.challenger)?;
    let opponent = parse_agent_spec(&args.opponent)?;
    let selection = select(&args.source)?;
    check_role(args.role, args.seed, &selection)?;
    let specs = specs(&selection, args.games, args.seed, &challenger, &opponent)?;
    let counts = equal_counts(&selection, specs.len());
    Ok(Plan {
        challenger,
        opponent,
        selection,
        specs,
        counts,
    })
}

/// The ledger entry of a finished role run.
#[must_use]
pub fn ledger_entry(args: &HeadToHeadArgs, role: SeedRole) -> Entry {
    let mut config = Json::object();
    config.set("challenger", args.challenger.as_str());
    config.set("opponent", args.opponent.as_str());
    args.source.ledger_config(&mut config);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stratified_counts_deal_blocks_round_robin() {
        assert_eq!(
            stratified_counts(400, 15)
                .ok()
                .map(|c| c.iter().sum::<usize>()),
            Some(400)
        );
        let c = stratified_counts(400, 15).unwrap_or_default();
        assert_eq!(&c[..10], &[28; 10]);
        assert_eq!(&c[10..], &[24; 5]);
        assert_eq!(stratified_counts(24, 3).ok(), Some(vec![8, 8, 8]));
        assert!(stratified_counts(8, 3).is_err());
        assert!(stratified_counts(10, 1).is_err());
        assert!(stratified_counts(4, 0).is_err());
    }

    #[test]
    fn the_stratified_interval_is_the_pooled_rate_with_within_pair_variance() {
        let s = stratified_json(&[(3, 4), (1, 4)]);
        assert_eq!(s.get("rate"), Some(&Json::from(0.5)));
        let Some(Json::Array(ci)) = s.get("ci95") else {
            panic!("ci95")
        };
        let half = 1.96 * (2.0 * 0.25 * 0.25 * 0.75 / 4.0_f64).sqrt();
        assert_eq!(ci[0], Json::from(round_f64(0.5 - half, 4)));
        assert_eq!(stratified_json(&[]), Json::Null);
        let clipped = stratified_json(&[(4, 4), (4, 4)]);
        assert_eq!(
            clipped.get("ci95"),
            Some(&Json::Array(vec![1.0.into(), 1.0.into()]))
        );
    }
}
