//! `results.json` (`reports/results.py`): the reproducible benchmark result. Its content
//! depends only on the configuration and the code and data versions, never on wall-clock
//! time, the output path or `--workers`.
//!
//! The card facts, `versions()` and the conflict resolutions come from gcg-data
//! (`CardDb`, `gcg_data::versions`, `gcg_data::conflict_resolutions`); [`ResultsData`] carries
//! them, so the document is a pure function of the run and that data.

use std::collections::BTreeMap;
use std::fmt;

use gcg_ai::{AgentEffort, SearchSettings};
use gcg_core::pyjson::Json;
use gcg_data::{CardDb, CardDef, CardType, Color, Deck};

use super::aggregate::{
    self, BenchCardStats, CardStats, DutResult, Mulligan, Outcomes, Splits, bench_card_stats,
    card_stats,
};
use super::hypotheses::{self, ALPHA, MIN_GAMES};
use super::stats::Distribution;
use crate::{BenchmarkConfig, BenchmarkRun, GameRecord, SchemaVersion};

/// `DRAW_REPLAYS` (`reports/results.py:27`).
pub const DRAW_REPLAYS: u32 = 1;

/// `NOTES` (`reports/results.py:28-38`), verbatim.
pub const NOTES: [&str; 4] = [
    "Win rates count draws as non-wins; n is every game (or match). Intervals are 95% Wilson \
     score intervals.",
    "Both seats are played by the same AI (see 'ai'); results measure the decks under that AI, \
     not under human play.",
    "Per-card and benchmark-card signals are correlations within these games; the \
     'hypotheses' list suggests what to test next and is not a conclusion.",
    "A simultaneous defeat (both_defeated) is a game draw; in BO3 match scoring the turn player \
     at the end loses it (TRM 5.2). turn_limit games are engine safety-cap draws and are \
     always counted and reported.",
];

/// What the report reads from one `CardDef` (`db[n].name`, `.card_type`, `.level`, `.cost`,
/// `.color`; `reports/results.py:58-60`, `reports/aggregate.py:143-150`, `:198-210`,
/// `reports/versions.py:107`).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct CardFacts {
    pub name: Box<str>,
    pub card_type: CardType,
    pub level: i32,
    pub cost: i32,
    pub color: Option<Color>,
}

impl CardFacts {
    /// The facts of one card definition.
    #[must_use]
    pub fn of(def: &CardDef) -> Self {
        Self {
            name: def.name.clone(),
            card_type: def.card_type,
            level: def.level,
            cost: def.cost,
            color: def.color,
        }
    }
}

/// A card number of a deck that the lookup does not know (Python's `KeyError` on `db[n]`).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct MissingCard(pub Box<str>);

impl fmt::Display for MissingCard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "card {} is not in the card database", self.0)
    }
}

impl std::error::Error for MissingCard {}

/// `ai.search` of results 1.2.0 (PLAN §12.7): the strengthened search's settings as the
/// run's factory reports them (`AgentFactory::search_block`, O9), with `algorithm`
/// `gcg-search` and the mechanisms sorted.
#[must_use]
pub fn search_json(s: &SearchSettings) -> Json {
    let mut budget = Json::object();
    budget.set("iterations_per_option", s.iterations_per_option);
    budget.set("min_iterations", s.min_iterations);
    budget.set("max_iterations", s.max_iterations);
    budget.set("aux_apply_cap", s.aux_apply_cap);
    let mut mechanisms = s.mechanisms.clone();
    mechanisms.sort();
    let mut o = Json::object();
    o.set("algorithm", "gcg-search");
    o.set("preset", s.preset);
    o.set("mapping", s.mapping);
    o.set("budget", budget);
    o.set("mechanisms", mechanisms);
    o.set("weights", s.weights.as_str());
    o
}

fn count(x: u64) -> i64 {
    i64::try_from(x).unwrap_or(i64::MAX)
}

/// `ai.effort` of results 1.2.0 (PLAN §6.6, §12.7): per seat, seat 0 first, the totals of the
/// search effort its agents reported over every game of the run (`GameRecord::effort`; a game
/// whose agent reported none does not count), and the per-game distributions of iterations
/// and engine applies. Deterministic counts, identical for any `--workers`.
#[must_use]
pub fn effort_json(run: &BenchmarkRun) -> Json {
    Json::Array(
        [0_u8, 1]
            .into_iter()
            .map(|seat| {
                let per_game: Vec<AgentEffort> = run
                    .games()
                    .filter_map(|g| g.effort[usize::from(seat)])
                    .collect();
                let total = per_game
                    .iter()
                    .fold(AgentEffort::default(), |sum, e| sum.plus(e));
                let iterations: Vec<i64> = per_game.iter().map(|e| count(e.iterations)).collect();
                let applies: Vec<i64> = per_game.iter().map(|e| count(e.applies())).collect();
                let mut o = Json::object();
                o.set("seat", seat);
                o.set("games", per_game.len());
                o.set("decisions", total.decisions);
                o.set("searches", total.searches);
                o.set("iterations", total.iterations);
                o.set("applies_tree", total.applies_tree);
                o.set("applies_rollout", total.applies_rollout);
                o.set("applies_aux", total.applies_aux);
                o.set("worlds", total.worlds);
                o.set("rollouts_capped", total.rollouts_capped);
                o.set(
                    "iterations_per_game",
                    Distribution::of(&iterations).to_json(),
                );
                o.set("applies_per_game", Distribution::of(&applies).to_json());
                o
            })
            .collect(),
    )
}

/// The data `build_results` reads besides the run: `versions()` (`reports/versions.py:55-75`,
/// `gcg_data::versions`), `conflict_resolutions` for the two decks (`reports/results.py:181-186`,
/// `gcg_data::conflict_resolutions`) and the facts of every card in either deck. The search
/// block and the effort of results 1.2.0 come from the run itself (`BenchmarkRun::search`,
/// `GameRecord::effort`).
#[derive(Clone, PartialEq, Debug)]
pub struct ResultsData {
    versions: Json,
    conflict_resolutions: Json,
    cards: BTreeMap<Box<str>, CardFacts>,
}

impl ResultsData {
    /// Collects the facts of every main-deck and resource card of both decks of `cfg` through
    /// `lookup` (with gcg-data: `|n| db.get(n).map(CardFacts::of)`).
    ///
    /// # Errors
    /// [`MissingCard`] for the first card number (deck under test first, main before
    /// resources) that `lookup` does not know.
    pub fn new(
        cfg: &BenchmarkConfig,
        versions: Json,
        conflict_resolutions: Json,
        lookup: impl Fn(&str) -> Option<CardFacts>,
    ) -> Result<Self, MissingCard> {
        let mut cards = BTreeMap::new();
        for deck in [&cfg.deck_under_test, &cfg.benchmark_deck] {
            for cc in deck.main.iter().chain(deck.resources.iter()) {
                if cards.contains_key(&cc.card_number) {
                    continue;
                }
                let facts =
                    lookup(&cc.card_number).ok_or_else(|| MissingCard(cc.card_number.clone()))?;
                cards.insert(cc.card_number.clone(), facts);
            }
        }
        Ok(Self {
            versions,
            conflict_resolutions,
            cards,
        })
    }

    /// The data of the embedded packaged sources, as Python's `build_results` reads it
    /// (`reports/results.py:145-189`): `gcg_data::versions(cfg.package, cfg.code_digest)`
    /// (`versions()`, `reports/versions.py:55-75`), `gcg_data::conflict_resolutions` of the
    /// two decks under the labels `deck_under_test` and `benchmark`, each deck's `main +
    /// resources` (`reports/results.py:181-186`), and the card facts from
    /// `CardDb::embedded()` (`get_card_db()`, `reports/results.py:54`).
    ///
    /// # Errors
    /// [`MissingCard`] as [`ResultsData::new`]: a card number the card data does not know.
    pub fn embedded(cfg: &BenchmarkConfig) -> Result<Self, MissingCard> {
        let db = CardDb::embedded();
        let lookup = |n: &str| db.get(n).map(CardFacts::of);
        // Every card is looked up before conflict_resolutions, which panics on an unknown one.
        let probe = Self::new(cfg, Json::Null, Json::Null, lookup)?;
        Ok(Self {
            versions: gcg_data::versions(cfg.package, cfg.code_digest),
            conflict_resolutions: gcg_data::conflict_resolutions(&[
                ("deck_under_test", &cfg.deck_under_test),
                ("benchmark", &cfg.benchmark_deck),
            ]),
            cards: probe.cards,
        })
    }

    /// The facts of a card of either deck.
    ///
    /// # Panics
    /// If `number` is not a card of the configuration this data was built for (`new` covers
    /// every card of both decks).
    #[must_use]
    pub fn card(&self, number: &str) -> &CardFacts {
        self.cards
            .get(number)
            .expect("ResultsData::new covers every card of both decks")
    }

    #[must_use]
    pub const fn versions(&self) -> &Json {
        &self.versions
    }

    #[must_use]
    pub const fn conflict_resolutions(&self) -> &Json {
        &self.conflict_resolutions
    }
}

fn copies_json(copies: u128) -> Json {
    Json::Int(i128::try_from(copies).expect("a deck's copy count fits in i128"))
}

/// `_deck_block(deck)` (`reports/results.py:53-69`).
fn deck_block(deck: &Deck, data: &ResultsData) -> Json {
    let listing = |cards: &[gcg_data::CardCount]| {
        Json::Array(
            cards
                .iter()
                .map(|cc| {
                    let mut e = Json::object();
                    e.set("card_number", &*cc.card_number);
                    e.set("name", &*data.card(&cc.card_number).name);
                    e.set("count", copies_json(cc.count));
                    e
                })
                .collect(),
        )
    };
    let mut colors: Vec<&'static str> = Vec::new();
    for cc in &deck.main {
        if let Some(c) = data.card(&cc.card_number).color
            && !colors.contains(&c.as_str())
        {
            colors.push(c.as_str());
        }
    }
    colors.sort();
    let total = |cards: &[gcg_data::CardCount]| copies_json(cards.iter().map(|cc| cc.count).sum());
    let mut o = Json::object();
    o.set("name", &*deck.name);
    o.set("digest", deck.digest.as_str());
    o.set("main_count", total(&deck.main));
    o.set("resource_count", total(&deck.resources));
    o.set("colors", colors);
    o.set("main", listing(&deck.main));
    o.set("resources", listing(&deck.resources));
    o
}

/// `_pick(games, k)` (`reports/results.py:72-79`): the `k` games closest to the lower median
/// length (ties by match and game order), returned in match and game order.
fn pick<'a>(games: &[&'a GameRecord], k: u32) -> Vec<&'a GameRecord> {
    if games.is_empty() || k == 0 {
        return Vec::new();
    }
    let mut turns: Vec<i32> = games.iter().map(|g| g.turns).collect();
    turns.sort();
    let median = i64::from(turns[(turns.len() - 1) / 2]);
    let mut ranked: Vec<&GameRecord> = games.to_vec();
    ranked.sort_by_key(|g| {
        (
            (i64::from(g.turns) - median).abs(),
            g.match_index,
            g.game_index,
        )
    });
    ranked.truncate(usize::try_from(k).unwrap_or(usize::MAX));
    ranked.sort_by_key(|g| (g.match_index, g.game_index));
    ranked
}

/// `replay_name(label, g)` (`reports/results.py:82-83`):
/// `f"replays/{label}-m{match_index:04d}-g{game_index + 1}.json"`.
#[must_use]
pub fn replay_name(label: &str, g: &GameRecord) -> String {
    format!(
        "replays/{label}-m{:04}-g{}.json",
        g.match_index,
        u64::from(g.game_index) + 1
    )
}

/// `select_replays(run)` (`reports/results.py:86-91`): representative games to save, every
/// pick for `win`, then `loss`, then `draw` (`--replays K` each, at most one draw).
#[must_use]
pub fn select_replays(run: &BenchmarkRun) -> Vec<(&'static str, &GameRecord)> {
    let k = run.config.replays;
    let mut out = Vec::new();
    for r in DutResult::ALL {
        let games: Vec<&GameRecord> = run.games().filter(|g| DutResult::of(g) == r).collect();
        let quota = if r == DutResult::Draw {
            k.min(DRAW_REPLAYS)
        } else {
            k
        };
        out.extend(pick(&games, quota).into_iter().map(|g| (r.as_str(), g)));
    }
    out
}

/// `_replay_entries(run)` (`reports/results.py:94-106`).
fn replay_entries(run: &BenchmarkRun) -> Json {
    Json::Array(
        select_replays(run)
            .into_iter()
            .map(|(label, g)| {
                let mut e = Json::object();
                e.set("file", replay_name(label, g));
                e.set("result", label);
                e.set("match_index", g.match_index);
                e.set("game_number", u64::from(g.game_index) + 1);
                e.set("turns", g.turns);
                e.set("end_reason", g.end_reason.as_str());
                e.set("seed", g.seed);
                e
            })
            .collect(),
    )
}

/// `_config_block(run)` (`reports/results.py:109-119`).
fn config_block(c: &BenchmarkConfig) -> Json {
    let mut o = Json::object();
    o.set("matches", c.matches);
    o.set("format", c.format.as_str());
    o.set("max_games_per_match", c.format.games_per_match());
    o.set("wins_needed", c.format.wins_needed());
    o.set("turn_limit", c.turn_limit);
    o.set("max_actions", c.max_actions);
    o.set("replays_per_result", c.replays);
    o
}

/// `_ai_block(run)` (`reports/results.py:122-130`). With no game (Python's `IndexError`) the
/// agents are `null`, which the schema rejects before any file is written. Results 1.2.0 add
/// `search` ([`search_json`] of the run's [`BenchmarkRun::search`]) and `effort`
/// ([`effort_json`]) (PLAN §12.7); a run whose factory has no search block gets neither, and
/// the 1.2.0 schema rejects it.
fn ai_block(run: &BenchmarkRun) -> Json {
    let mut o = ai_block_v1_1(run);
    if run.config.schema == SchemaVersion::V1_2
        && let Some(search) = &run.search
    {
        o.set("search", search_json(search));
        o.set("effort", effort_json(run));
    }
    o
}

fn ai_block_v1_1(run: &BenchmarkRun) -> Json {
    let first = run.games().next();
    let mut o = Json::object();
    o.set("factory", run.agent_factory.as_str());
    o.set("preset", run.config.ai_preset);
    o.set(
        "dut_agent",
        first.map(|g| &*g.agents[usize::from(g.dut_seat)]),
    );
    o.set(
        "bench_agent",
        first.map(|g| &*g.agents[usize::from(g.bench_seat())]),
    );
    o.set("decision_log", run.config.decision_log);
    o
}

/// `_seeds_block(run)` (`reports/results.py:133-142`).
fn seeds_block(c: &BenchmarkConfig) -> Json {
    let mut o = Json::object();
    o.set("master", c.seed);
    o.set("prng", "SplitMix64");
    o.set("game_seed", "derive_seed(master, 'match', m, 'game', g)");
    o.set(
        "agent_seed",
        "derive_seed(master, 'match', m, 'game', g, 'agent', seat)",
    );
    o.set("dut_seat", "m % 2");
    o.set(
        "game_one_chooser",
        "seeded die roll inside new_game (rule 6-2-1-4)",
    );
    o.set(
        "later_game_chooser",
        "loser of the previous game (BO3 Match Rules, TRM 4.6)",
    );
    o
}

/// The statistics `build_results` computes once and shares with the hypotheses
/// (`reports/results.py:147-152`).
struct Aggregates {
    cards: Vec<CardStats>,
    bench_cards: Vec<BenchCardStats>,
    split: Splits,
    mull: Mulligan,
}

/// gcg-data's `versions()` with `results_schema` naming the results schema `schema` (1.1.0
/// unchanged): the `versions` of a results document and of `data status`.
#[must_use]
pub fn versions_for(mut versions: Json, schema: SchemaVersion) -> Json {
    if schema != SchemaVersion::V1_1 && matches!(versions, Json::Object(_)) {
        versions.set("results_schema", schema.as_str());
    }
    versions
}

/// `versions` for the run's schema.
fn versions_block(run: &BenchmarkRun, data: &ResultsData) -> Json {
    versions_for(data.versions().clone(), run.config.schema)
}

/// `build_results(run)` (`reports/results.py:145-189`) with gcg-data's values in `data`: the
/// content of `results.json`, keys in Python's insertion order (every writer sorts them).
/// `schema`, `schema_version` and `versions.results_schema` name `run.config.schema`. A 1.1.0
/// document is Python's; a 1.2.0 document (PLAN §12.7) adds `ai.search` and `ai.effort` from
/// the run's search block and records, without which it fails `validate_results` and is never
/// written.
#[must_use]
pub fn build_results_with(run: &BenchmarkRun, data: &ResultsData) -> Json {
    let games: Vec<&GameRecord> = run.games().collect();
    let config = &run.config;
    let agg = Aggregates {
        cards: card_stats(&games, &config.deck_under_test, data),
        bench_cards: bench_card_stats(&games, &config.benchmark_deck, MIN_GAMES, data),
        split: Splits::of(&games, config.format.games_per_match()),
        mull: Mulligan::of(&games),
    };
    let mut decks = Json::object();
    decks.set("deck_under_test", deck_block(&config.deck_under_test, data));
    decks.set("benchmark", deck_block(&config.benchmark_deck, data));
    let mut results = Json::object();
    results.set("matches", Outcomes::of_matches(&run.matches).to_json());
    results.set("games", Outcomes::of_games(games.iter().copied()).to_json());
    let mut thresholds = Json::object();
    thresholds.set("min_games", MIN_GAMES);
    thresholds.set("alpha", ALPHA);
    thresholds.set("multiple_comparisons", "holm");
    let found = hypotheses::hypotheses(&agg.cards, &agg.bench_cards, &agg.split, &agg.mull);
    let mut o = Json::object();
    o.set("schema", config.schema.id());
    o.set("schema_version", config.schema.as_str());
    o.set("versions", versions_block(run, data));
    o.set("config", config_block(config));
    o.set("seeds", seeds_block(config));
    o.set("ai", ai_block(run));
    o.set("decks", decks);
    o.set("results", results);
    o.set("splits", agg.split.to_json());
    o.set("game_length", aggregate::game_length(&games));
    o.set("end_reasons", aggregate::end_reasons(&games));
    o.set("draws", aggregate::draws(&games));
    o.set("mulligan", agg.mull.to_json());
    o.set(
        "cards",
        Json::Array(agg.cards.iter().map(CardStats::to_json).collect()),
    );
    o.set(
        "benchmark_cards",
        Json::Array(
            agg.bench_cards
                .iter()
                .map(BenchCardStats::to_json)
                .collect(),
        ),
    );
    o.set(
        "hypotheses",
        Json::Array(found.iter().map(hypotheses::Hypothesis::to_json).collect()),
    );
    o.set("hypothesis_thresholds", thresholds);
    o.set("conflict_resolutions", data.conflict_resolutions().clone());
    o.set("replays", replay_entries(run));
    o.set("notes", NOTES.to_vec());
    o
}

/// `build_results(run)` (`reports/results.py:145-189`): [`build_results_with`] over the
/// embedded packaged sources ([`ResultsData::embedded`]).
///
/// # Panics
/// If a deck of the configuration holds a card number that is not in the embedded card data
/// (Python raises `KeyError` on `db[n]`); a deck parsed by gcg-data never does.
#[must_use]
pub fn build_results(run: &BenchmarkRun) -> Json {
    let data = ResultsData::embedded(&run.config)
        .unwrap_or_else(|missing| panic!("a parsed deck holds only known cards: {missing}"));
    build_results_with(run, &data)
}
