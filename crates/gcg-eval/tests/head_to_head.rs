//! Head-to-head without games: the A24 balance of `--choose-first seat0` over mirrored blocks
//! (T-149, spec level; the realised counts are checked on real games in `tests/e2e.rs`), the
//! pair selections and their seeding, the held-out split, and the report built from outcomes.

use std::path::PathBuf;

use gcg_ai::DEFAULT_WEIGHTS;
use gcg_core::derive_seed;
use gcg_core::pyjson::Json;
use gcg_eval::decks::Panel;
use gcg_eval::head_to_head::{
    HeadToHeadArgs, PairSource, Selection, report, select_with, specs, unfinished,
};
use gcg_eval::specs::{comparison_specs, deals, pair_specs};
use gcg_eval::stats::{Paired, PlayDraw};
use gcg_eval::{
    AgentDesc, ChooseFirst, DECISION_LIMIT, DeckSet, GameOutcome, MIRROR_BLOCK, MirroredSpec,
    PairSet,
};
use gcg_runner::Workers;

fn agents() -> (AgentDesc, AgentDesc) {
    (
        AgentDesc::Random,
        AgentDesc::Greedy {
            weights: DEFAULT_WEIGHTS.clone(),
        },
    )
}

fn set(decks: DeckSet, pairs: PairSet) -> PairSource {
    PairSource::Set { decks, pairs }
}

fn selection(source: &PairSource, panel: Option<&Panel>) -> Selection {
    select_with(source, panel).expect("selection")
}

/// Outcomes as `--choose-first seat0` produces them: seat 0 is on the play in every game.
fn seat0_outcomes(
    specs: &[MirroredSpec],
    winner: impl Fn(&MirroredSpec) -> i8,
) -> Vec<GameOutcome> {
    specs
        .iter()
        .map(|s| GameOutcome {
            index: s.index,
            challenger: s.challenger,
            winner: winner(s),
            turns: 10,
            first_player: 0,
            decisions: 100,
        })
        .collect()
}

/// T-149 (A24): with seat 0 on the play, each agent is on the play in exactly two games of
/// every block of four, plays each deck of the pair once on the play and once on the draw, and
/// both games of a deal put the same deck on the play.
#[test]
fn t149_seat0_balances_play_and_draw_in_every_block() {
    let (c, o) = agents();
    let sel = selection(&set(DeckSet::Tuning, PairSet::All), None);
    let specs = specs(&sel, 48, 77, &c, &o).expect("specs");
    for block in specs.chunks(MIRROR_BLOCK as usize) {
        let mut challenger_on_play = Vec::new();
        let mut challenger_on_draw = Vec::new();
        let mut opponent_on_play = Vec::new();
        let mut opponent_on_draw = Vec::new();
        for s in block {
            let chal_deck = &s.decks[usize::from(s.challenger)];
            let opp_deck = &s.decks[usize::from(1 - s.challenger)];
            if s.challenger == 0 {
                challenger_on_play.push(chal_deck);
                opponent_on_draw.push(opp_deck);
            } else {
                challenger_on_draw.push(chal_deck);
                opponent_on_play.push(opp_deck);
            }
        }
        for side in [
            &challenger_on_play,
            &challenger_on_draw,
            &opponent_on_play,
            &opponent_on_draw,
        ] {
            assert_eq!(side.len(), 2, "two games per role in every block");
            assert_ne!(side[0], side[1], "each deck once per role");
        }
    }
    for (first, second) in deals(&specs) {
        assert_eq!(specs[first].game_seed, specs[second].game_seed);
        assert_eq!(specs[first].decks[0], specs[second].decks[0]);
        assert_eq!((specs[first].challenger, specs[second].challenger), (0, 1));
    }
    let outcomes = seat0_outcomes(&specs, |_| 0);
    let chal = PlayDraw::of(&outcomes, true);
    let opp = PlayDraw::of(&outcomes, false);
    assert_eq!((chal.on_play, chal.on_draw), (24, 24));
    assert_eq!((opp.on_play, opp.on_draw), (24, 24));
    assert_eq!((chal.on_play_wins, chal.on_draw_wins), (24, 0));
    assert_eq!((opp.on_play_wins, opp.on_draw_wins), (24, 0));
    let paired = Paired::of(&specs, &outcomes);
    assert_eq!((paired.a, paired.b, paired.c, paired.d), (24, 0, 0, 0));
    assert_eq!(paired.difference(), Some(0.0));
}

#[test]
fn a_set_selection_is_comparison_specs_and_subsets_keep_their_games() {
    let (c, o) = agents();
    let sel = selection(&set(DeckSet::Tuning, PairSet::All), None);
    assert_eq!(sel.pairings_total, 3);
    let decks: Vec<_> = sel.pairs.iter().map(|p| p.decks.clone()).collect();
    assert_eq!(
        decks
            .iter()
            .map(|[a, b]| (a.name.as_str(), b.name.as_str()))
            .collect::<Vec<_>>(),
        [
            ("tuning-zeon", "tuning-wing"),
            ("tuning-zeon", "tuning-tekkadan"),
            ("tuning-wing", "tuning-tekkadan")
        ]
    );
    let lists = gcg_eval::decks::embedded(DeckSet::Tuning)
        .expect("tuning")
        .into_iter()
        .map(|d| d.deck)
        .collect::<Vec<_>>();
    let all = specs(&sel, 36, 5, &c, &o).expect("specs");
    assert_eq!(
        all,
        comparison_specs(36, 5, &c, &o, &lists).expect("compare specs")
    );

    let panel = Panel {
        held_out: vec!["tuning-zeon".to_owned()],
        pairs: Vec::new(),
    };
    let held = selection(&set(DeckSet::Tuning, PairSet::HeldOut), Some(&panel));
    assert_eq!(
        held.pairs.iter().map(|p| p.position).collect::<Vec<_>>(),
        [0, 1]
    );
    assert!(held.pairs.iter().all(|p| p.held_out));
    let tuning = selection(&set(DeckSet::Tuning, PairSet::Tuning), Some(&panel));
    assert_eq!(
        tuning.pairs.iter().map(|p| p.position).collect::<Vec<_>>(),
        [2]
    );
    let sub = specs(&tuning, 12, 5, &c, &o).expect("subset specs");
    for (s, full) in sub.iter().zip(&all[24..36]) {
        assert_eq!(
            (s.game_seed, s.agent_seeds, &s.decks, s.challenger),
            (
                full.game_seed,
                full.agent_seeds,
                &full.decks,
                full.challenger
            )
        );
    }
    assert_eq!(sub[0].index, 0);
}

#[test]
fn named_pairs_seed_by_list_position() {
    let (c, o) = agents();
    let one = selection(
        &PairSource::Named("tests-federation:tests-seed".to_owned()),
        None,
    );
    let tests = selection(&set(DeckSet::Tests, PairSet::All), None);
    assert_eq!(
        specs(&one, 8, 3, &c, &o).expect("named"),
        specs(&tests, 8, 3, &c, &o).expect("set")
    );
    let two = selection(
        &PairSource::Named("tuning-zeon:tuning-wing,tests-federation:tests-seed".to_owned()),
        None,
    );
    let s = specs(&two, 8, 3, &c, &o).expect("two pairs");
    let pairs: Vec<_> = two
        .pairs
        .iter()
        .map(|p| (p.position, [&p.decks[0].deck, &p.decks[1].deck]))
        .collect();
    assert_eq!(s, pair_specs(8, 3, &c, &o, &pairs, 2).expect("pair specs"));
    assert_eq!(
        s[4].game_seed,
        derive_seed!(derive_seed!(3, "pairing", 1), "block", 0, "decks", 0)
    );
    assert!(select_with(&PairSource::Named("tests-seed".to_owned()), None).is_err());
    assert!(select_with(&PairSource::Named("nope:tests-seed".to_owned()), None).is_err());
}

#[test]
fn the_report_has_pooled_held_out_and_per_pair_blocks() {
    let (c, o) = agents();
    let panel = Panel {
        held_out: vec!["tuning-tekkadan".to_owned()],
        pairs: Vec::new(),
    };
    let source = set(DeckSet::Tuning, PairSet::All);
    let sel = selection(&source, Some(&panel));
    let specs = specs(&sel, 24, 11, &c, &o).expect("specs");
    let mut outcomes = seat0_outcomes(&specs, |s| i8::try_from(s.challenger).unwrap_or(0));
    outcomes[5].winner = -1;
    outcomes[5].decisions = DECISION_LIMIT;
    let args = HeadToHeadArgs {
        challenger: "random".to_owned(),
        opponent: "greedy".to_owned(),
        source,
        games: 24,
        seed: 11,
        role: None,
        ledger: PathBuf::from("ledger.json"),
        workers: Workers::Count(1),
        choose_first: ChooseFirst::Seat0,
        out: None,
        decision_trace: None,
    };
    let doc = report(&args, ["random", "greedy"], &sel, &specs, &outcomes);
    let Json::Object(items) = &doc else {
        panic!("an object")
    };
    let keys: Vec<String> = items
        .iter()
        .map(|(k, _)| match k {
            gcg_core::pyjson::Key::Str(s) => s.clone(),
            gcg_core::pyjson::Key::Int(i) => i.to_string(),
        })
        .collect();
    assert_eq!(
        keys,
        [
            "format",
            "version",
            "challenger",
            "opponent",
            "agents",
            "decks",
            "pairs",
            "seed",
            "role",
            "games",
            "choose_first",
            "result",
            "held_out",
            "by_pair",
            "by_deck",
            "unfinished",
            "outcomes"
        ]
    );
    let Some(Json::Array(by_deck)) = doc.get("by_deck") else {
        panic!("by_deck")
    };
    let names: Vec<Option<&Json>> = by_deck.iter().map(|d| d.get("deck")).collect();
    assert_eq!(
        names,
        ["tuning-zeon", "tuning-wing", "tuning-tekkadan"]
            .map(|n| Some(Json::from(n)))
            .iter()
            .map(Option::as_ref)
            .collect::<Vec<_>>()
    );
    let deck_games: u64 = by_deck
        .iter()
        .filter_map(|d| d.get("result").and_then(|r| r.get("n")))
        .map(|n| {
            if let Json::Int(i) = n {
                u64::try_from(*i).unwrap_or(0)
            } else {
                0
            }
        })
        .sum();
    assert_eq!(
        deck_games, 24,
        "every game counts for the challenger's deck once"
    );
    let result = doc.get("result").expect("result");
    assert!(
        result.get("stratified").is_some(),
        "a pooled block over 3 pairs"
    );
    assert_eq!(
        result.get("effort"),
        Some(&Json::Null),
        "no effort without traced games"
    );
    assert_eq!(
        doc.get("unfinished"),
        Some(&Json::Array(vec![Json::from(5_u32)]))
    );
    assert_eq!(unfinished(&outcomes), [5]);
    let result = doc.get("result").expect("result");
    assert_eq!(result.get("wins"), Some(&Json::from(23_u64)));
    assert_eq!(result.get("n"), Some(&Json::from(24_u64)));
    let held = doc.get("held_out").expect("held-out block");
    assert_eq!(held.get("pairs"), Some(&Json::from(2_usize)));
    assert_eq!(held.get("n"), Some(&Json::from(16_u64)));
    let Some(Json::Array(rows)) = doc.get("by_pair") else {
        panic!("by_pair")
    };
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].get("held_out"), Some(&Json::Bool(false)));
    assert_eq!(rows[1].get("held_out"), Some(&Json::Bool(true)));
    assert_eq!(
        rows[2].get("master"),
        Some(&Json::from(derive_seed!(11, "pairing", 2)))
    );
    let Some(Json::Array(games)) = doc.get("outcomes") else {
        panic!("outcomes")
    };
    assert_eq!(games.len(), 24);
    assert_eq!(
        games[23],
        Json::Array(vec![
            23_u32.into(),
            2_usize.into(),
            1_u8.into(),
            1_i8.into(),
            10_i32.into(),
            0_u8.into(),
            100_u32.into()
        ])
    );
}
