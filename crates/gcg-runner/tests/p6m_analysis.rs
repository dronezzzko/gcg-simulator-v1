//! P6-M `analysis.json` (docs/port/PLAN.md §12.2; `gcg_runner::analysis`): the unit checks
//! the WEAKNESSES entries name, on synthetic game sets with known structure, and the
//! re-simulation observer on real engine games of the random test agent.
//!
//! W-90 (length bias), W-92 (BO3 clustering coverage), W-93 (draw rates), W-94 (run-level
//! error control on null runs), W-95 (never-played fates), W-96 (observed benchmark cards),
//! W-98 (unrounded statistics), W-101 (interpretations used), W-109 (Swiss draws), W-147
//! (mulligan selection), W-46 (cap kinds).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use gcg_core::SplitMix64;
use gcg_core::pool::Cancel;
use gcg_core::pyjson::Json;
use gcg_core::sys::topology::{Topology, TopologySource};
use gcg_engine::EndReason;
use gcg_runner::analysis::{
    Fate, GameFacts, Opening, analysis_json, benchmark_observed, clustered_rate, diagnostics_json,
    double_defeats, holm_padded, hypothesis_families, interpolated, interpretations_used,
    length_stratified, mulligan_confound, never_played, observe_run, outcomes,
};
use gcg_runner::reports::stats::{Distribution, rnd};
use gcg_runner::reports::{ResultsData, build_results_with};
use gcg_runner::{
    BenchmarkConfig, BenchmarkRun, DutCardEvents, Format, GameRecord, MatchRecord, Outcome,
    PlayEvent, Role, RunTiming, SchemaVersion, TestAgentKind, Workers, run_benchmark,
};
use serde_json::Value;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn deck(name: &str) -> gcg_data::Deck {
    gcg_data::load_deck(&root().join("examples/decks").join(name)).expect("an example deck")
}

fn config(matches: u32, format: Format) -> BenchmarkConfig {
    BenchmarkConfig {
        deck_under_test: deck("red-green-zeon.txt"),
        benchmark_deck: deck("blue-white-federation.txt"),
        matches,
        seed: 1,
        workers: Workers::Count(2),
        format,
        ai_preset: "fast",
        out_dir: PathBuf::from("out"),
        decision_log: false,
        replays: 1,
        turn_limit: 200,
        max_actions: 20_000,
        schema: SchemaVersion::V1_1,
        package: "0.1.0",
        code_digest: "0000000000000000000000000000000000000000000000000000000000000000",
    }
}

fn json(doc: &Json) -> Value {
    serde_json::from_str(&gcg_runner::reports::to_json_text(doc)).expect("JSON")
}

fn strs(items: &[&str]) -> Vec<Box<str>> {
    items.iter().map(|s| Box::from(*s)).collect()
}

/// A synthetic game record: only the fields the analysis reads are meaningful.
fn game(m: u32, g: u32, winner: Outcome, turns: i32, drawn: &[&str]) -> GameRecord {
    GameRecord {
        match_index: m,
        game_index: g,
        seed: u64::from(m) * 10 + u64::from(g),
        agent_seeds: [0, 0],
        agents: [Box::from("a"), Box::from("b")],
        dut_seat: 0,
        chooser: None,
        chooser_seat: 0,
        first_player: 0,
        dut_on_play: (m + g).is_multiple_of(2),
        winner,
        end_reason: EndReason::BattleDamage,
        match_point: match winner {
            Outcome::Dut => Some(Role::Dut),
            Outcome::Bench => Some(Role::Bench),
            Outcome::Draw => None,
        },
        turns,
        active_at_end: 0,
        actions: Vec::new(),
        redraws: [false, false],
        dut_cards: DutCardEvents {
            initial_hand: Vec::new(),
            redrew: false,
            opening_hand: Vec::new(),
            drawn: strs(drawn),
            played: Vec::new(),
            never_played: Vec::new(),
            in_hand_at_end: Vec::new(),
        },
        bench_seen: Vec::new(),
        final_state_sha256: String::new(),
        turn_limit: 200,
        max_actions: 20_000,
        decision_log: None,
    }
}

fn run_of(cfg: BenchmarkConfig, matches: Vec<Vec<GameRecord>>) -> BenchmarkRun {
    let matches = matches
        .into_iter()
        .enumerate()
        .map(|(i, games)| {
            let dut = u32::try_from(games.iter().filter(|g| g.winner == Outcome::Dut).count())
                .expect("small");
            let bench = u32::try_from(games.iter().filter(|g| g.winner == Outcome::Bench).count())
                .expect("small");
            MatchRecord {
                match_index: u32::try_from(i).expect("small"),
                dut_seat: 0,
                winner: match dut.cmp(&bench) {
                    std::cmp::Ordering::Greater => Outcome::Dut,
                    std::cmp::Ordering::Less => Outcome::Bench,
                    std::cmp::Ordering::Equal => Outcome::Draw,
                },
                dut_points: dut,
                bench_points: bench,
                games,
            }
        })
        .collect();
    BenchmarkRun {
        config: cfg,
        matches,
        timing: RunTiming {
            started_at: String::new(),
            finished_at: String::new(),
            wall_seconds: 0.0,
            match_seconds: Vec::new(),
            workers: 1,
            topology: Topology {
                levels: Vec::new(),
                logical_cpus: 1,
                cache_line_bytes: None,
                arm_features: Vec::new(),
                source: TopologySource::Fallback,
            },
        },
        agent_factory: "synthetic".to_owned(),
    }
}

fn uniform(rng: &mut SplitMix64) -> f64 {
    rng.random()
}

#[test]
fn w90_a_pure_length_effect_vanishes_within_length_strata() {
    // Wins depend only on game length; the card is drawn far more often in long games.
    let mut rng = SplitMix64::new(90);
    let card = "GD01-030";
    let mut games = Vec::new();
    for i in 0..3000_u32 {
        let turns = 5 + i32::try_from(rng.randrange(11)).expect("small");
        let p_win = if turns >= 10 { 0.8 } else { 0.2 };
        let won = uniform(&mut rng) < p_win;
        let p_draw_card = f64::from(turns - 4) / 12.0;
        let drawn: &[&str] = if uniform(&mut rng) < p_draw_card {
            &[card]
        } else {
            &[]
        };
        games.push(game(
            i,
            0,
            if won { Outcome::Dut } else { Outcome::Bench },
            turns,
            drawn,
        ));
    }
    let refs: Vec<&GameRecord> = games.iter().collect();
    let doc = json(&length_stratified(
        &refs,
        &config(1, Format::Bo1).deck_under_test,
    ));
    let entry = doc["cards"]
        .as_array()
        .expect("cards")
        .iter()
        .find(|c| c["card_number"] == card)
        .expect("the card");
    let raw = entry["raw_difference"].as_f64().expect("raw");
    let mh = &entry["length_stratified"];
    let d = mh["difference"].as_f64().expect("MH difference");
    let ci = (
        mh["ci95"][0].as_f64().expect("lo"),
        mh["ci95"][1].as_f64().expect("hi"),
    );
    assert!(raw > 0.15, "the raw lift is the length effect: {raw}");
    assert!(d.abs() < 0.05, "stratified difference about zero: {d}");
    assert!(ci.0 < 0.0 && 0.0 < ci.1, "{ci:?}");
}

#[test]
fn w92_match_clustered_intervals_cover_and_plain_ones_do_not() {
    // BO3-like matches of 3 games sharing a latent match strength: p_i is 0.2 or 0.8, so the
    // true game win rate is 0.5 with a within-match correlation of 0.36.
    let mut rng = SplitMix64::new(92);
    let (mut clustered_hits, mut plain_hits) = (0_u32, 0_u32);
    let replications = 1000;
    for _ in 0..replications {
        let clusters: Vec<(u64, u64)> = (0..100)
            .map(|_| {
                let p = if uniform(&mut rng) < 0.5 { 0.2 } else { 0.8 };
                let wins = (0..3).filter(|_| uniform(&mut rng) < p).count();
                (u64::try_from(wins).expect("small"), 3)
            })
            .collect();
        let c = clustered_rate(&clusters, 0, 0);
        let (lo, hi) = c.clustered.expect("clustered");
        clustered_hits += u32::from(lo <= 0.5 && 0.5 <= hi);
        let (lo, hi) = c.wilson.expect("wilson");
        plain_hits += u32::from(lo <= 0.5 && 0.5 <= hi);
        assert!(c.design_effect.expect("deff") > 0.0);
    }
    let cover = f64::from(clustered_hits) / f64::from(replications);
    let plain = f64::from(plain_hits) / f64::from(replications);
    eprintln!("W-92: clustered coverage {cover}, plain Wilson coverage {plain}");
    assert!((0.93..=0.97).contains(&cover), "clustered coverage {cover}");
    assert!(plain < 0.90, "plain Wilson coverage {plain}");
    // The bootstrap is seeded: the same seed gives the same interval, close to the analytic one.
    let clusters: Vec<(u64, u64)> = (0..60).map(|i| (i % 4, 3)).collect();
    let a = clustered_rate(&clusters, 2000, 7);
    assert_eq!(a, clustered_rate(&clusters, 2000, 7));
    let (blo, bhi) = a.bootstrap.expect("bootstrap");
    let (clo, chi) = a.clustered.expect("clustered");
    assert!(
        (blo - clo).abs() < 0.03 && (bhi - chi).abs() < 0.03,
        "{a:?}"
    );
}

#[test]
fn w93_draws_are_non_wins_with_their_rate_beside() {
    let games = [
        game(0, 0, Outcome::Dut, 9, &[]),
        game(0, 1, Outcome::Draw, 9, &[]),
        game(1, 0, Outcome::Bench, 9, &[]),
        game(1, 1, Outcome::Draw, 9, &[]),
    ];
    let run = run_of(
        config(2, Format::Bo3),
        vec![games[..2].to_vec(), games[2..].to_vec()],
    );
    let doc = json(&outcomes(&run));
    assert_eq!(doc["games"]["successes"], 1);
    assert_eq!(doc["games"]["n"], 4);
    assert_eq!(doc["games"]["rate"], 0.25);
    assert_eq!(doc["games"]["draws"], 2);
    assert_eq!(doc["games"]["draw_rate"], 0.5);
    // The same wins-only rate as results.json (`Rate`, reports/stats.py).
    let r = gcg_runner::reports::stats::Rate::new(1, 4);
    assert_eq!(doc["games"]["ci95"][0], r.ci95.expect("ci").0);
}

fn null_run(rng: &mut SplitMix64, cfg: &BenchmarkConfig, games: u32) -> BenchmarkRun {
    let main: Vec<String> = cfg
        .deck_under_test
        .main
        .iter()
        .map(|c| c.card_number.to_string())
        .collect();
    let bench_main: Vec<String> = cfg
        .benchmark_deck
        .main
        .iter()
        .map(|c| c.card_number.to_string())
        .collect();
    let matches = (0..games)
        .map(|m| {
            let won = uniform(rng) < 0.5;
            let drawn: Vec<&str> = main
                .iter()
                .filter(|_| uniform(rng) < 0.5)
                .map(String::as_str)
                .collect();
            let mut g = game(
                m,
                0,
                if won { Outcome::Dut } else { Outcome::Bench },
                10,
                &drawn,
            );
            g.dut_on_play = uniform(rng) < 0.5;
            g.dut_cards.redrew = uniform(rng) < 0.3;
            g.bench_seen = bench_main
                .iter()
                .filter(|_| uniform(rng) < 0.5)
                .map(|s| Box::from(s.as_str()))
                .collect();
            g.bench_seen.sort();
            vec![g]
        })
        .collect();
    run_of(cfg.clone(), matches)
}

#[test]
fn w94_run_level_holm_keeps_the_false_positive_rate_at_alpha() {
    let cfg = config(400, Format::Bo1);
    let data = ResultsData::embedded(&cfg).expect("known cards");
    let mut rng = SplitMix64::new(94);
    let runs = 400;
    let (mut reference, mut run_level) = (0_u32, 0_u32);
    for _ in 0..runs {
        let run = null_run(&mut rng, &cfg, 400);
        let games: Vec<&GameRecord> = run.games().collect();
        let doc = json(&hypothesis_families(&games, &run, &data));
        reference += u32::from(doc["any_reference_significant"] == true);
        run_level += u32::from(doc["any_run_level_significant"] == true);
    }
    let alpha = 0.05;
    let sd = (alpha * (1.0 - alpha) / f64::from(runs)).sqrt();
    let (fr, fl) = (
        f64::from(reference) / f64::from(runs),
        f64::from(run_level) / f64::from(runs),
    );
    eprintln!("W-94: per-family false-positive rate {fr}, run-level {fl} over {runs} null runs");
    assert!(fl <= alpha + 2.0 * sd, "run-level false-positive rate {fl}");
    assert!(fr > fl, "per-family control is weaker: {fr} vs {fl}");
    assert!(fr <= 4.0 * alpha + 3.0 * sd, "per-family rate {fr}");
}

#[test]
fn w98_unrounded_statistics_beside_the_reference_values() {
    let values = [1, 2, 3, 4];
    let reference = Distribution::of(&values);
    assert_eq!(reference.median, Some(2));
    let doc = json(&interpolated(&values));
    assert_eq!(doc["median"], 2.5);
    assert_eq!(doc["mean"], 2.5);
    assert_eq!(doc["p10"].as_f64().map(rnd), Some(1.3));
    // A Holm family of one at alpha 0.05: the stored p-value rounds to 0.05, which the
    // reference keeps; the unrounded value fails.
    let p = 0.050_000_4;
    assert_eq!(rnd(p), 0.05);
    assert_eq!(holm_padded(&[rnd(p)], 1), [true]);
    assert_eq!(holm_padded(&[p], 1), [false]);
}

#[test]
fn w95_never_played_copies_are_split_by_fate() {
    let cfg = config(1, Format::Bo1);
    let mut g = game(0, 0, Outcome::Dut, 9, &["GD01-030", "GD01-030", "GD01-031"]);
    g.dut_cards.never_played = strs(&["GD01-030", "GD01-030"]);
    let facts = GameFacts {
        never_played: vec![
            (Box::from("GD01-030"), Fate::DiscardedForCost),
            (Box::from("GD01-030"), Fate::InHandAtEnd),
        ],
        ..GameFacts::default()
    };
    let doc = json(&never_played(&[&g], &[facts], &cfg.deck_under_test));
    let c = doc["cards"]
        .as_array()
        .expect("cards")
        .iter()
        .find(|c| c["card_number"] == "GD01-030")
        .expect("card")
        .clone();
    assert_eq!(
        c["never_played_copies"], 2,
        "the reference count is unchanged"
    );
    assert_eq!(c["by_fate"]["discarded_for_cost"]["count"], 1);
    assert_eq!(c["by_fate"]["in_hand_at_end"]["count"], 1);
    assert_eq!(c["by_fate"]["used_by_effect_from_hand"]["count"], 0);
    assert_eq!(c["by_fate"]["discarded_for_cost"]["n"], 2);
}

#[test]
fn w96_a_benchmark_card_only_discarded_is_observed_not_seen() {
    let cfg = config(1, Format::Bo1);
    let card = cfg.benchmark_deck.main[0].card_number.clone();
    let g = game(0, 0, Outcome::Bench, 9, &[]);
    let facts = GameFacts {
        bench_observed: BTreeSet::from([card.clone()]),
        ..GameFacts::default()
    };
    let doc = json(&benchmark_observed(&[&g], &[facts], &cfg.benchmark_deck));
    let c = &doc["cards"][0];
    assert_eq!(c["card_number"], &*card);
    assert_eq!(c["reference_seen_games"], 0);
    assert_eq!(c["observed_games"], 1);
}

#[test]
fn w101_a_suppression_card_lists_the_suppression_interpretation() {
    let table: Value =
        serde_json::from_str(gcg_runner::analysis::INTERPRETATIONS).expect("the audit table");
    let i18 = table["interpretations"]
        .as_array()
        .expect("list")
        .iter()
        .find(|i| i["id"] == "I-18")
        .expect("I-18: <Suppression> with a Base present");
    let card = i18["tags"]["card"][0].as_str().expect("a Suppression card");
    let facts = GameFacts {
        used: BTreeSet::from([Box::from(card)]),
        ..GameFacts::default()
    };
    let doc = json(&interpretations_used(&[facts, GameFacts::default()]));
    let listed = doc["interpretations"].as_array().expect("list");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0]["id"], "I-18");
    assert_eq!(listed[0]["games"], 1);
    assert!(
        doc["not_traceable_from_games"]
            .as_array()
            .expect("list")
            .iter()
            .any(|i| i == "I-01")
    );
}

#[test]
fn w109_a_double_defeat_decider_is_a_swiss_draw_in_the_analysis_only() {
    let mut decider = game(0, 1, Outcome::Draw, 12, &[]);
    decider.end_reason = EndReason::BothDefeated;
    decider.match_point = Some(Role::Dut);
    let mut first = game(0, 0, Outcome::Dut, 9, &[]);
    first.match_point = Some(Role::Dut);
    let mut run = run_of(
        config(2, Format::Bo3),
        vec![
            vec![first, decider],
            vec![
                game(1, 0, Outcome::Bench, 9, &[]),
                game(1, 1, Outcome::Bench, 9, &[]),
            ],
        ],
    );
    run.matches[0].winner = Outcome::Dut;
    run.matches[0].dut_points = 2;
    let plain = json(&double_defeats(&run, false));
    assert_eq!(plain["games"], 1);
    assert_eq!(plain["bo3_scored_for_the_non_turn_player"], 1);
    assert!(plain.get("swiss_draws").is_none());
    let swiss = json(&double_defeats(&run, true));
    let s = &swiss["swiss_draws"];
    assert_eq!(
        (
            s["dut_wins"].as_u64(),
            s["draws"].as_u64(),
            s["bench_wins"].as_u64()
        ),
        (Some(0), Some(1), Some(1))
    );
    assert_eq!(s["dut_points"], 1);
    assert_eq!(s["matches_rescored"][0], 0);
}

#[test]
fn w147_the_redraw_gap_is_the_ai_rule_not_the_deck() {
    // Wins depend only on the played hand's opening score; the AI redraws mostly low hands.
    let mut rng = SplitMix64::new(147);
    let mut games = Vec::new();
    let mut facts = Vec::new();
    for i in 0..4000_u32 {
        let score =
            [10.0, 15.0, 20.0, 25.0, 30.0][usize::try_from(rng.randrange(5)).expect("small")];
        let redrew = uniform(&mut rng) < if score <= 15.0 { 0.8 } else { 0.1 };
        let won = uniform(&mut rng) < score / 40.0;
        games.push(game(
            i,
            0,
            if won { Outcome::Dut } else { Outcome::Bench },
            9,
            &[],
        ));
        facts.push(GameFacts {
            opening: Some(Opening {
                redrew,
                first: true,
                hand_score: score,
                played_score: score,
                threshold: Some(17.0),
            }),
            ..GameFacts::default()
        });
    }
    let refs: Vec<&GameRecord> = games.iter().collect();
    let doc = json(&mulligan_confound(&refs, &facts));
    let raw = doc["raw_redraw_minus_keep"].as_f64().expect("raw");
    let s = &doc["stratified_redraw_minus_keep"];
    let (lo, hi) = (
        s["ci95"][0].as_f64().expect("lo"),
        s["ci95"][1].as_f64().expect("hi"),
    );
    assert!(raw < -0.1, "raw gap {raw}");
    assert!(lo < 0.0 && 0.0 < hi, "stratified interval [{lo}, {hi}]");
    assert!(
        doc["label"]
            .as_str()
            .is_some_and(|l| l.contains("not of the deck"))
    );
}

fn random_run(cfg: &BenchmarkConfig) -> BenchmarkRun {
    let f = gcg_runner::test_agents::factory(TestAgentKind::Random);
    run_benchmark(cfg, &*f, &mut |_, _| {}, &Cancel::new()).expect("random games finish")
}

#[test]
fn the_observer_partitions_the_reference_never_played_set_and_extends_seen() {
    let mut cfg = config(12, Format::Bo3);
    cfg.decision_log = true;
    let run = random_run(&cfg);
    let facts = observe_run(&run).expect("every game re-simulates");
    let mut fates = BTreeSet::new();
    for (g, f) in run.games().zip(&facts) {
        let mut reference: Vec<&str> = g.dut_cards.never_played.iter().map(|n| &**n).collect();
        let mut ours: Vec<&str> = f.never_played.iter().map(|(n, _)| &**n).collect();
        reference.sort();
        ours.sort();
        assert_eq!(
            ours, reference,
            "match {} game {}",
            g.match_index, g.game_index
        );
        for (n, fate) in &f.never_played {
            fates.insert(*fate);
            if *fate == Fate::InHandAtEnd {
                assert!(g.dut_cards.in_hand_at_end.contains(n));
            }
        }
        for seen in &g.bench_seen {
            assert!(
                f.bench_observed.contains(seen),
                "{seen}: seen but not observed"
            );
        }
        assert_eq!(f.decisions, u64::try_from(g.actions.len()).expect("small"));
        let o = f
            .opening
            .expect("the deck under test made its redraw decision");
        assert_eq!(o.redrew, g.dut_cards.redrew);
    }
    assert!(fates.len() >= 2, "{fates:?}");
    let data = ResultsData::embedded(&cfg).expect("known cards");
    let results = build_results_with(&run, &data);
    let (doc, _) = analysis_json(&run, &data, &results, &facts, Default::default());
    let v = json(&doc);
    assert_eq!(v["format"], "gcg-eval-analysis");
    assert!(
        v["mulligan"]["hands"][0]["threshold"].is_number()
            || v["mulligan"]["hands"][0]["threshold"].is_null()
    );
}

#[test]
fn w46_the_cap_kind_names_the_cap_that_fired() {
    let mut cfg = config(2, Format::Bo1);
    cfg.max_actions = 10;
    let run = random_run(&cfg);
    let facts = observe_run(&run).expect("re-simulates");
    let doc = json(&diagnostics_json(&run, &facts));
    assert_eq!(doc["capped_games"], 2);
    assert_eq!(doc["decision_cap"], 2);
    assert!(run.games().all(|g| g.end_reason == EndReason::TurnLimit));
    let mut cfg = config(2, Format::Bo1);
    cfg.turn_limit = 2;
    let run = random_run(&cfg);
    let facts = observe_run(&run).expect("re-simulates");
    let doc = json(&diagnostics_json(&run, &facts));
    assert_eq!(doc["turn_cap"], 2);
    assert_eq!(doc["games"][0]["cap"], "turns");
}

#[test]
fn played_events_feed_the_own_turn_distribution() {
    let mut g = game(0, 0, Outcome::Dut, 9, &["GD01-030"]);
    g.dut_cards.played = vec![
        PlayEvent {
            card_number: Box::from("GD01-030"),
            turn: 3,
            own_turn: 2,
        },
        PlayEvent {
            card_number: Box::from("GD01-030"),
            turn: 5,
            own_turn: 3,
        },
    ];
    let doc = json(&gcg_runner::analysis::distributions(
        &[&g],
        &config(1, Format::Bo1).deck_under_test,
    ));
    let c = doc["card_own_turn"]
        .as_array()
        .expect("cards")
        .iter()
        .find(|c| c["card_number"] == "GD01-030")
        .expect("card")
        .clone();
    assert_eq!(c["median"], 2.5);
    assert_eq!(c["n"], 2);
}

#[test]
fn w90_card_hypotheses_get_a_paired_swap_test() {
    let cfg = config(40, Format::Bo3);
    let run = run_of(cfg.clone(), vec![vec![game(0, 0, Outcome::Dut, 9, &[])]]);
    let hyp = |id: &str, delta: f64| {
        let mut ev = Json::object();
        ev.set("delta", delta);
        let mut h = Json::object();
        h.set("id", id);
        h.set("evidence", ev);
        h
    };
    let mut results = Json::object();
    results.set(
        "hypotheses",
        Json::Array(vec![
            hyp("card-drawn-delta:GD01-035", 0.12),
            hyp("card-drawn-delta:GD01-031", -0.09),
            hyp("play-draw", 0.2),
        ]),
    );
    let (tests, files) = gcg_runner::analysis::swap_tests(&results, &run);
    let tests = json(&tests);
    assert_eq!(tests.as_array().map(Vec::len), Some(2));
    assert_eq!(files.len(), 2);
    let plus = tests
        .as_array()
        .expect("tests")
        .iter()
        .find(|t| t["hypothesis"] == "card-drawn-delta:GD01-035")
        .expect("the positive card")
        .clone();
    assert_eq!(
        (plus["add"].as_str(), plus["remove"].as_str()),
        (Some("GD01-035"), Some("GD01-031"))
    );
    let commands = plus["commands"].as_array().expect("commands");
    let bench = commands[0].as_str().expect("a command");
    assert!(
        bench.starts_with("gcg-sim benchmark out/variants/"),
        "{bench}"
    );
    assert!(bench.contains(" --seed 1 "), "{bench}");
    let compare = commands[1].as_str().expect("a command");
    assert!(
        compare.starts_with("gcg-eval compare-runs out "),
        "{compare}"
    );
    // The variant deck is legal and differs from the deck under test by exactly the swap.
    let (_, text) = files
        .iter()
        .find(|(n, _)| n == "variants/swap-GD01-035-for-GD01-031.txt")
        .expect("the variant deck");
    let variant = gcg_data::parse_deck(text, "variant", None).expect("parses");
    let count = |d: &gcg_data::Deck, n: &str| {
        d.main
            .iter()
            .find(|c| &*c.card_number == n)
            .map_or(0, |c| c.count)
    };
    assert_eq!(
        count(&variant, "GD01-035"),
        count(&cfg.deck_under_test, "GD01-035") + 1
    );
    assert_eq!(
        count(&variant, "GD01-031"),
        count(&cfg.deck_under_test, "GD01-031") - 1
    );
    let reg = gcg_effects::Registry::embedded();
    let violations = gcg_data::validate(
        &variant,
        gcg_data::ValidateOptions {
            banlist: true,
            implemented: Some(&*reg),
        },
    );
    assert!(violations.is_empty(), "{violations:?}");
}
