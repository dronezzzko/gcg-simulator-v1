//! W-38's `can_kill` census, feature `slow`: on 10,000 positions (the turn player's MAIN
//! decisions of random-agent and greedy-agent games on the example decks), the strengthened
//! `can_kill` (the exact static attack step, `board::static_lethal`) and the reference's
//! (`ai/evaluation.py:205-212`, the role feature `can_kill_turn_player`) are compared with an
//! exhaustive search of the attack lines on the true state: the solver with attacks only, every
//! Blocker and every action-step reply of the defender explored, 20,000 applies per position.
//! Positions whose exhaustive search reaches the bound are counted and left out. The
//! disagreement rates are recorded in docs/dev/eval/weakness-status/P6-B.md.

use std::path::PathBuf;

use gcg_ai::{
    Agent, AgentCx, ROLE_FEATURES, Scratch, burst_density, make_greedy_agent, make_random_agent,
    role_features,
};
use gcg_core::pool::{Cancel, Pool, TaskCx};
use gcg_core::rng::{SeedPart, derive_seed};
use gcg_effects::Registry;
use gcg_engine::{DecisionKind, NewGameOptions, apply, new_game_by_number};

use super::board::{Turn, player_attacks, static_lethal};
use super::solver::{Limits, Verdict, lethal_line};

const POSITIONS: usize = 10_000;
const MASTER: u64 = 20_200_300;
const EXHAUSTIVE: Limits = Limits {
    max_applies: 20_000,
    setups: 0,
    own_tricks: false,
};
const DECKS: [&str; 3] = [
    "red-green-zeon",
    "blue-white-federation",
    "purple-white-tekkadan",
];

fn deck(name: &str) -> (Vec<String>, Vec<String>) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/decks")
        .join(format!("{name}.txt"));
    let d = gcg_data::load_deck(&path).unwrap_or_else(|e| panic!("{name}: {e:?}"));
    let expand = |cards: &[gcg_data::CardCount]| -> Vec<String> {
        cards
            .iter()
            .flat_map(|c| {
                std::iter::repeat_n(
                    c.card_number.to_string(),
                    usize::try_from(c.count).expect("a count"),
                )
            })
            .collect()
    };
    (expand(&d.main), expand(&d.resources))
}

fn with_cx<T>(f: impl FnOnce(&mut AgentCx<'_>) -> T) -> T {
    let cancel = Cancel::new();
    Pool::run_inline(|scratch: &mut Scratch, par: &TaskCx<'_, Scratch>| {
        let mut cx = AgentCx {
            scratch,
            par,
            cancel: &cancel,
        };
        f(&mut cx)
    })
}

#[derive(Default, Debug)]
struct Tally {
    positions: usize,
    unknown: usize,
    lethal: usize,
    strengthened_wrong: usize,
    reference_wrong: usize,
    strengthened_false_positive: usize,
    strengthened_false_negative: usize,
}

#[test]
fn the_static_can_kill_agrees_with_an_exhaustive_search_more_often_than_the_reference() {
    let reg = Registry::embedded();
    let knobs = crate::preset_config(crate::Preset::Standard).knobs();
    let ck = ROLE_FEATURES
        .iter()
        .position(|&n| n == "can_kill_turn_player")
        .expect("a role feature");
    let mut t = Tally::default();
    let mut game: i64 = 0;
    while t.positions < POSITIONS {
        let (a, b) = (DECKS[(game % 3) as usize], DECKS[((game + 1) % 3) as usize]);
        let (da, db) = (deck(a), deck(b));
        let ra: (Vec<&str>, Vec<&str>) = (
            da.0.iter().map(String::as_str).collect(),
            da.1.iter().map(String::as_str).collect(),
        );
        let rb: (Vec<&str>, Vec<&str>) = (
            db.0.iter().map(String::as_str).collect(),
            db.1.iter().map(String::as_str).collect(),
        );
        let seed = derive_seed(MASTER, &[SeedPart::Str("game"), SeedPart::Int(game)]);
        let mut st = new_game_by_number(
            &reg,
            [(&ra.0[..], &ra.1[..]), (&rb.0[..], &rb.1[..])],
            seed,
            NewGameOptions::default(),
        )
        .expect("legal decks");
        // Every third game greedy against greedy, the others random against random.
        let mut players: [Box<dyn Agent>; 2] = if game % 3 == 2 {
            [
                make_greedy_agent(seed, false, gcg_ai::DEFAULT_WEIGHTS.clone()),
                make_greedy_agent(seed + 1, false, gcg_ai::DEFAULT_WEIGHTS.clone()),
            ]
        } else {
            [
                make_random_agent(seed, false),
                make_random_agent(seed + 1, false),
            ]
        };
        game += 1;
        while let Some(dec) = st.pending() {
            if t.positions >= POSITIONS {
                break;
            }
            let p = dec.hdr.player;
            if dec.hdr.kind == DecisionKind::Main
                && p == st.active()
                && !player_attacks(dec.options).is_empty()
            {
                t.positions += 1;
                let exhaustive =
                    lethal_line(&st, p, EXHAUSTIVE, &knobs, &player_attacks(dec.options))
                        .expect("solve")
                        .verdict;
                if exhaustive == Verdict::Unknown {
                    t.unknown += 1;
                } else {
                    let truth = matches!(exhaustive, Verdict::Lethal(_));
                    let mine = static_lethal(&st, p, Turn::Now).expect("board");
                    let density = burst_density(&st, p);
                    let theirs = role_features(&st, density).expect("features")[ck] == 1.0;
                    t.lethal += usize::from(truth);
                    t.strengthened_wrong += usize::from(mine != truth);
                    t.reference_wrong += usize::from(theirs != truth);
                    t.strengthened_false_positive += usize::from(mine && !truth);
                    t.strengthened_false_negative += usize::from(!mine && truth);
                }
            }
            let action = with_cx(|cx| players[usize::from(p)].choose(&st.clone(), p, cx))
                .unwrap_or_else(|e| panic!("choose: {e}"));
            apply(&mut st, action, true).unwrap_or_else(|e| panic!("apply {action:?}: {e}"));
        }
    }
    eprintln!("W-38 can_kill census over {game} games: {t:?}");
    // Recorded 2026-11-02 (231 games): 10,000 positions, none at the bound, 1,403 lethal; the
    // static attack step disagrees on 61 (59 false lethals: Bursts, tricks and effects the
    // static game does not play; 2 missed: 【Attack】 effects), the reference's count rule on
    // 694.
    assert_eq!(t.positions, POSITIONS);
    assert!(
        t.strengthened_wrong < t.reference_wrong,
        "the static attack step disagrees with the exhaustive search less often: {t:?}"
    );
}
