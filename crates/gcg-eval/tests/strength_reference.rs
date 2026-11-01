//! T-070 (PLAN §9.7): the reference `standard` preset against random (seed 7001) and greedy
//! (seed 7002), 400 mirrored games each on the AI test decks, as
//! `tests/slow/test_ai_strength.py` plays them. The outcome lists equal Python's
//! (`tests/vectors/strength.json`, `export_stats.py --strength`), and Python's bars hold: a win
//! rate of at least 0.90 against random and a Wilson lower bound above 0.50 against greedy, with
//! the MCTS agent in both seats.

use std::path::Path;

use gcg_ai::DEFAULT_WEIGHTS;
use gcg_ai_reference::STANDARD;
use gcg_core::pool::Cancel;
use gcg_eval::decks::embedded;
use gcg_eval::selfplay::play_all;
use gcg_eval::specs::mirrored_specs;
use gcg_eval::stats::{challenger_won, wilson95};
use gcg_eval::{AgentDesc, ChooseFirst, DeckSet};
use gcg_runner::Workers;
use serde_json::Value;

const GAMES: u32 = 400;

fn vectors() -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/vectors/strength.json");
    let text = std::fs::read_to_string(&path).expect("read strength.json");
    serde_json::from_str(&text).expect("parse strength.json")
}

#[test]
fn reference_standard_beats_the_baselines_with_pythons_outcomes() {
    let v = vectors();
    let decks: Vec<_> = embedded(DeckSet::Tests)
        .expect("tests decks")
        .into_iter()
        .map(|d| d.deck)
        .collect();
    let mcts = AgentDesc::Reference {
        config: STANDARD.clone(),
        weights: DEFAULT_WEIGHTS.clone(),
    };
    for case in v["strength"].as_array().expect("strength cases") {
        let kind = case["opponent"].as_str().unwrap_or_default();
        let seed = case["seed"].as_u64().expect("seed");
        let opponent = match kind {
            "random" => AgentDesc::Random,
            "greedy" => AgentDesc::Greedy {
                weights: DEFAULT_WEIGHTS.clone(),
            },
            other => panic!("unknown opponent {other}"),
        };
        let specs = mirrored_specs(GAMES, seed, &mcts, &opponent, [&decks[0], &decks[1]]);
        let outcomes = play_all(
            &specs,
            Workers::Auto,
            ChooseFirst::Agents,
            &Cancel::new(),
            &mut |_| {},
        )
        .expect("the games play");
        let rows: Vec<Value> = outcomes
            .iter()
            .map(|o| {
                serde_json::json!([o.challenger, o.winner, o.turns, o.first_player, o.decisions])
            })
            .collect();
        assert_eq!(
            &Value::Array(rows),
            &case["outcomes"],
            "outcomes against {kind}"
        );
        let wins = outcomes.iter().filter(|o| challenger_won(o)).count() as u64;
        let n = outcomes.len() as u64;
        assert!(n >= 400);
        assert!(
            outcomes.iter().any(|o| o.challenger == 0)
                && outcomes.iter().any(|o| o.challenger == 1)
        );
        let (lower, _) = wilson95(wins, n);
        match kind {
            "random" => assert!(wins * 10 >= n * 9, "{wins}/{n} against random"),
            _ => assert!(
                lower > 0.50,
                "{wins}/{n} against greedy, lower bound {lower}"
            ),
        }
    }
}
