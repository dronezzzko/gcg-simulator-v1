//! GAP-B, W-101 (docs/port/WEAKNESSES.md): the interpretation-tagged code paths of the engine
//! record when they run (`gcg_engine::diag::interpretation_runs`), on the running thread only,
//! without changing a game.
//!
//! - Over seeded random games between the example decks, the setup interpretations run in
//!   every game (I-05 once per game, I-15 once per Shield placed), attacks on Bases and tokens
//!   leaving the game run in some, and the per-game counts are the same when a game is played
//!   again (the record is a function of the game).
//! - A game's final state does not depend on whether the counters were read.
//! - The resource cap (I-03) runs when a Resource is placed onto a full resource area.

mod l5_common;

use gcg_core::rng::SplitMix64;
use gcg_engine::canon::state_digest;
use gcg_engine::diag::{Interpretation, interpretation_runs};
use gcg_engine::{
    GameState, NewGameOptions, Scenario, ScenarioOptions, Step, Zone, apply, legal_actions,
};
use l5_common::{engine_records, reg, start_game};
use serde_json::{Value, json};

fn example_decks() -> Vec<&'static Value> {
    let mut out: Vec<&Value> = Vec::new();
    for rec in engine_records("setup_touch") {
        for d in rec["decks"].as_array().expect("decks") {
            if !out.contains(&d) {
                out.push(d);
            }
        }
    }
    out
}

/// One seeded random game; the final state and the interpretation runs it made.
fn random_game(seed: u64) -> (GameState, [u64; Interpretation::ALL.len()]) {
    let decks = example_decks();
    let mut rng = SplitMix64::new(seed);
    let rec = json!({
        "decks": [
            decks[rng.randrange_usize(decks.len())],
            decks[rng.randrange_usize(decks.len())],
        ],
        "seed": rng.next_u64().to_string(),
    });
    let before = interpretation_runs();
    let mut st = start_game(&rec, NewGameOptions::default()).expect("a game");
    loop {
        let options = legal_actions(&st);
        if options.is_empty() {
            break;
        }
        let a = options[rng.randrange_usize(options.len())];
        apply(&mut st, a, true).unwrap_or_else(|e| panic!("game {seed}: {a}: {e}"));
    }
    let after = interpretation_runs();
    (st, std::array::from_fn(|i| after[i] - before[i]))
}

fn index(i: Interpretation) -> usize {
    Interpretation::ALL
        .iter()
        .position(|&x| x == i)
        .expect("an anchored interpretation")
}

#[test]
fn w101_the_engine_records_its_interpretation_paths_in_random_games() {
    let games = 300u64;
    let mut games_with = [0u64; Interpretation::ALL.len()];
    let mut runs = [0u64; Interpretation::ALL.len()];
    for g in 0..games {
        let (st, n) = random_game(0x0101_0000 + g);
        let (again, m) = random_game(0x0101_0000 + g);
        assert_eq!(n, m, "game {g}: the record is a function of the game");
        assert_eq!(state_digest(&st), state_digest(&again));
        assert_eq!(n[index(Interpretation::I05)], 1, "game {g}: one new game");
        assert!(
            n[index(Interpretation::I15)] >= 12,
            "game {g}: at least the twelve setup Shields"
        );
        for k in 0..n.len() {
            games_with[k] += u64::from(n[k] > 0);
            runs[k] += n[k];
        }
    }
    let table: Vec<String> = Interpretation::ALL
        .iter()
        .enumerate()
        .map(|(k, i)| {
            format!(
                "{} {}/{games} games, {} runs",
                i.id(),
                games_with[k],
                runs[k]
            )
        })
        .collect();
    eprintln!("W-101 random games: {}", table.join("; "));
    for i in [Interpretation::I01, Interpretation::I02] {
        assert!(games_with[index(i)] > 0, "{} runs in some game", i.id());
    }
}

#[test]
fn w101_a_resource_placed_on_a_full_resource_area_records_the_cap() {
    let reg = reg();
    let mut sc = Scenario::new(&reg, ScenarioOptions::default());
    sc.resources(0, 15, 0, 0).expect("resources");
    sc.resource_deck(0, 2).expect("a resource deck");
    sc.shields(0, &["GD01-060"; 5]).expect("shields");
    sc.shields(1, &["GD01-060"; 5]).expect("shields");
    sc.deck(0, &["GD01-060"; 10]);
    sc.deck(1, &["GD01-060"; 10]);
    sc.start(Step::ResourceStep).expect("start");
    let mut st = sc.into_state();
    let before = interpretation_runs();
    let mut steps = 0;
    while st.zone(0, Zone::ResourceDeck).len() == 2 && steps < 50 {
        let options = legal_actions(&st);
        let Some(&a) = options.first() else { break };
        apply(&mut st, a, true).expect("a legal action");
        steps += 1;
    }
    let after = interpretation_runs();
    let k = index(Interpretation::I03);
    assert_eq!(
        st.zone(0, Zone::ResourceArea).len(),
        15,
        "the area stays at the cap"
    );
    assert_eq!(
        st.zone(0, Zone::ResourceDeck).len(),
        2,
        "no Resource left the deck"
    );
    assert!(after[k] > before[k], "I-03 recorded ({steps} steps)");
}
