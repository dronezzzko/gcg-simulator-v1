//! T-066: the tactical puzzles of `tests/ai/test_puzzles.py` (18 tests x 2 seeds),
//! `test_puzzles_shipped.py` (12 cases) and `test_puzzles_targets.py` (4), ported on the Rust
//! testkit at the reference presets. Every agent decision is compared with Python's full
//! decision record (`aiport::Recorder`): state digest, action, per-decision seed and generator,
//! every alternative bit for bit, the search statistics and the log entries. The assertions of
//! the Python tests follow, line by line.

mod aiport;

use aiport::positions::{
    PUZZLES, act1, amuro_rests, assert_pending, attack_with, burst, choose_first, play_card,
    protection, rest_the_active_unit, rests_the_blocker, select,
};
use aiport::{
    FILLER, PLAYER, Recorder, Sc, agent, agents, apply_ok, choice, ids, pending_player, play_turns,
    registry,
};
use gcg_ai_reference::{BASELINE_STANDARD, BASELINE_WEIGHTS, ReferenceConfig};
use gcg_engine::testkit::zone_of;
use gcg_engine::{Action, ActionKind, AddOptions, DecisionKind, GameState, NewGameOptions, Zone};

const M: &str = "test_puzzles";
const SHIPPED: &str = "test_puzzles_shipped";
const TARGETS: &str = "test_puzzles_targets";
const SEEDS: [u64; 2] = [1, 2];

fn deployed() -> AddOptions<'static> {
    AddOptions {
        deployed_this_turn: true,
        ..AddOptions::default()
    }
}

#[test]
fn every_puzzle_test_is_ported() {
    let names = [
        "test_lethal_attack_the_exposed_player",
        "test_lethal_goes_face_instead_of_taking_a_free_kill",
        "test_lethal_rests_the_blocker_before_attacking",
        "test_lethal_pairs_the_linking_pilot_to_attack_on_deploy_turn",
        "test_pairing_chooses_the_pilot_and_unit_that_link",
        "test_burst_aware_attack_order",
        "test_holds_removal_for_next_turns_lethal",
        "test_takes_the_favourable_trade",
        "test_does_not_attack_into_a_bigger_blocker",
        "test_blocks_to_survive",
        "test_blocks_when_the_blocker_wins_the_fight",
        "test_uses_an_action_step_trick_to_survive",
        "test_removal_targets_the_most_valuable_unit_it_can_destroy",
        "test_excess_trashes_the_weakest_unit",
        "test_takes_a_free_card_from_burst",
        "test_chooses_to_play_first_with_proactive_decks",
        "test_mulligans_a_hand_of_uncastable_cards",
        "test_keeps_a_hand_with_early_plays",
    ];
    let ported: Vec<String> = names.iter().flat_map(|n| ids(n, SEEDS)).collect();
    aiport::assert_ported(M, &ported);

    let mut shipped = ids(
        "test_rests_the_blocker_while_a_bigger_unit_is_already_rested",
        SEEDS,
    );
    for seed in SEEDS {
        for hidden in [0, 4] {
            shipped.push(format!(
                "test_rests_the_active_unit_not_the_rested_one[{hidden}-{seed}]"
            ));
        }
        for big in ["True", "False"] {
            shipped.push(format!("test_protects_the_attacked_unit[{big}-{seed}]"));
        }
    }
    shipped.extend(ids("test_rests_the_active_unit_with_a_hidden_hand", SEEDS));
    aiport::assert_ported(SHIPPED, &shipped);

    let mut targets = Vec::new();
    for (name, ..) in PUZZLES {
        targets.push(format!(
            "test_close_target_calls_get_more_iterations[{name}]"
        ));
        targets.push(format!(
            "test_negative_control_the_baseline_misses_the_pass_rate[{name}]"
        ));
    }
    aiport::assert_ported(TARGETS, &targets);
}

/// One puzzle per seed: `build` makes the position, `check` plays it and asserts.
fn per_seed(
    module: &str,
    test: &str,
    build: impl Fn() -> GameState,
    check: impl Fn(&mut Recorder, GameState, u64),
) {
    for seed in SEEDS {
        let mut rec = Recorder::new(module, &format!("{test}[{seed}]"));
        check(&mut rec, build(), seed);
        rec.finish();
    }
}

/// `play_turns(st, agents(seed))` and `st.winner == 0`.
fn wins_this_turn(rec: &mut Recorder, mut st: GameState, seed: u64) {
    play_turns(rec, &mut st, &mut agents(seed), 1);
    assert_eq!(st.winner(), Some(0));
}

// ---------------------------------------------------------------------------------------------
// lethal

#[test]
fn lethal_attack_the_exposed_player() {
    per_seed(
        M,
        "test_lethal_attack_the_exposed_player",
        || {
            let mut sc = Sc::new();
            sc.add(0, "ST01-005");
            sc.rested(1, "ST04-005");
            sc.shields(0, &[FILLER, FILLER]);
            sc.start()
        },
        wins_this_turn,
    );
}

#[test]
fn lethal_goes_face_instead_of_taking_a_free_kill() {
    per_seed(
        M,
        "test_lethal_goes_face_instead_of_taking_a_free_kill",
        || {
            let mut sc = Sc::new();
            sc.add(0, "ST01-005");
            sc.add(0, "ST01-005");
            sc.rested(1, "ST01-008");
            sc.shields(1, &[FILLER]);
            sc.shields(0, &[FILLER, FILLER, FILLER]);
            sc.start()
        },
        wins_this_turn,
    );
}

#[test]
fn lethal_rests_the_blocker_before_attacking() {
    per_seed(
        M,
        "test_lethal_rests_the_blocker_before_attacking",
        || {
            let mut sc = Sc::new();
            sc.resources(0, 3, 0, 0);
            sc.add(0, "ST01-005");
            sc.hand(0, &["ST02-014"]);
            sc.add(1, "ST01-008");
            sc.shields(0, &[FILLER, FILLER, FILLER]);
            sc.start()
        },
        wins_this_turn,
    );
}

#[test]
fn lethal_pairs_the_linking_pilot_to_attack_on_deploy_turn() {
    per_seed(
        M,
        "test_lethal_pairs_the_linking_pilot_to_attack_on_deploy_turn",
        || {
            let mut sc = Sc::new();
            sc.resources(0, 4, 0, 0);
            sc.add_with(0, "ST01-001", Zone::Battle, deployed());
            sc.hand(0, &["ST01-010"]);
            sc.shields(0, &[FILLER, FILLER, FILLER]);
            sc.start()
        },
        wins_this_turn,
    );
}

#[test]
fn pairing_chooses_the_pilot_and_unit_that_link() {
    for seed in SEEDS {
        let mut rec = Recorder::new(
            M,
            &format!("test_pairing_chooses_the_pilot_and_unit_that_link[{seed}]"),
        );
        let mut sc = Sc::new();
        sc.resources(0, 4, 3, 0);
        sc.add(0, "ST01-005");
        let gundam = sc.add_with(0, "ST01-001", Zone::Battle, deployed());
        let amuro = sc.hand(0, &["ST01-010", "ST01-011"])[0];
        sc.shields(1, &[FILLER]);
        sc.shields(0, &[FILLER, FILLER, FILLER]);
        let mut st = sc.start();
        play_turns(&mut rec, &mut st, &mut agents(seed), 1);
        assert_eq!(st.card(amuro).pair, i32::from(gundam));
        assert_eq!(st.winner(), Some(0));
        rec.finish();
    }
}

#[test]
fn burst_aware_attack_order() {
    per_seed(
        M,
        "test_burst_aware_attack_order",
        || {
            let mut sc = Sc::new();
            sc.add(0, "GD01-027");
            sc.add(0, "ST04-005");
            sc.shields(1, &["ST02-014"]);
            sc.deck(1, &["ST02-014"; 20]);
            sc.shields(0, &[FILLER, FILLER, FILLER]);
            sc.start()
        },
        wins_this_turn,
    );
}

// ---------------------------------------------------------------------------------------------
// holding cards

#[test]
fn holds_removal_for_next_turns_lethal() {
    for seed in SEEDS {
        let mut rec = Recorder::new(
            M,
            &format!("test_holds_removal_for_next_turns_lethal[{seed}]"),
        );
        let mut sc = Sc::new();
        sc.resources(0, 4, 3, 0);
        sc.add_with(0, "GD01-013", Zone::Battle, deployed());
        let ploy = sc.in_hand(0, "ST02-014");
        sc.add(1, "ST01-008");
        sc.resources(1, 2, 0, 0);
        sc.shields(0, &[FILLER, FILLER, FILLER, FILLER]);
        let mut st = sc.start();
        let mut players = agents(seed);
        play_turns(&mut rec, &mut st, &mut players, 1);
        assert_eq!(zone_of(&st, ploy), Zone::Hand);
        play_turns(&mut rec, &mut st, &mut players, 2);
        assert_eq!(st.winner(), Some(0));
        rec.finish();
    }
}

// ---------------------------------------------------------------------------------------------
// trades and blocks

#[test]
fn takes_the_favourable_trade() {
    for seed in SEEDS {
        let mut rec = Recorder::new(M, &format!("test_takes_the_favourable_trade[{seed}]"));
        let mut sc = Sc::new();
        let gundam = sc.add(0, "GD01-013");
        let dagger = sc.rested(1, "ST04-005");
        sc.shields(0, &[FILLER, FILLER]);
        sc.filler_shields(1, 5);
        let mut st = sc.start();
        play_turns(&mut rec, &mut st, &mut agents(seed), 1);
        assert_eq!(zone_of(&st, dagger), Zone::Trash);
        assert_eq!(zone_of(&st, gundam), Zone::Battle);
        rec.finish();
    }
}

#[test]
fn does_not_attack_into_a_bigger_blocker() {
    for seed in SEEDS {
        let mut rec = Recorder::new(
            M,
            &format!("test_does_not_attack_into_a_bigger_blocker[{seed}]"),
        );
        let mut sc = Sc::new();
        let gm = sc.add(0, "ST01-005");
        sc.add(1, "ST04-001");
        sc.filler_shields(0, 6);
        sc.filler_shields(1, 6);
        let mut st = sc.start();
        play_turns(&mut rec, &mut st, &mut agents(seed), 1);
        assert_eq!(zone_of(&st, gm), Zone::Battle);
        rec.finish();
    }
}

#[test]
fn blocks_to_survive() {
    for seed in SEEDS {
        let mut rec = Recorder::new(M, &format!("test_blocks_to_survive[{seed}]"));
        let mut sc = Sc::active(1);
        let gm = sc.add(1, "ST01-005");
        let blocker = sc.add(0, "ST01-008");
        sc.shields(1, &[FILLER, FILLER]);
        let mut st = sc.start();
        attack_with(&mut st, gm, PLAYER);
        let got = choice(&mut rec, &st, &mut *agents(seed)[0]);
        assert_eq!(got, act1(ActionKind::Block, blocker));
        rec.finish();
    }
}

#[test]
fn blocks_when_the_blocker_wins_the_fight() {
    for seed in SEEDS {
        let mut rec = Recorder::new(
            M,
            &format!("test_blocks_when_the_blocker_wins_the_fight[{seed}]"),
        );
        let mut sc = Sc::active(1);
        let dagger = sc.add(1, "ST04-005");
        let blocker = sc.add(0, "GD01-072");
        sc.filler_shields(0, 5);
        sc.filler_shields(1, 5);
        let mut st = sc.start();
        attack_with(&mut st, dagger, PLAYER);
        let got = choice(&mut rec, &st, &mut *agents(seed)[0]);
        assert_eq!(got, act1(ActionKind::Block, blocker));
        rec.finish();
    }
}

#[test]
fn uses_an_action_step_trick_to_survive() {
    for seed in SEEDS {
        let mut rec = Recorder::new(
            M,
            &format!("test_uses_an_action_step_trick_to_survive[{seed}]"),
        );
        let mut sc = Sc::active(1);
        let gm = sc.add(1, "ST01-005");
        sc.resources(0, 3, 0, 0);
        sc.hand(0, &["ST01-014"]);
        sc.shields(1, &[FILLER, FILLER]);
        let mut st = sc.start();
        attack_with(&mut st, gm, PLAYER);
        assert_pending(&st, DecisionKind::ActionStep);
        play_turns(&mut rec, &mut st, &mut agents(seed), 1);
        assert_eq!(st.winner(), None);
        rec.finish();
    }
}

// ---------------------------------------------------------------------------------------------
// targeting, excess, bursts, mulligans

#[test]
fn removal_targets_the_most_valuable_unit_it_can_destroy() {
    for seed in SEEDS {
        let mut rec = Recorder::new(
            M,
            &format!("test_removal_targets_the_most_valuable_unit_it_can_destroy[{seed}]"),
        );
        let mut sc = Sc::new();
        sc.resources(0, 2, 0, 0);
        let combat = sc.in_hand(0, "ST03-013");
        let aile = sc.add_with(
            1,
            "ST04-001",
            Zone::Battle,
            AddOptions {
                damage: 2,
                ..AddOptions::default()
            },
        );
        sc.add(1, "ST04-005");
        sc.add(1, "ST04-003");
        let mut st = sc.start();
        play_card(&mut st, combat, None);
        assert_pending(&st, DecisionKind::Select);
        let got = choice(&mut rec, &st, &mut *agents(seed)[0]);
        assert_eq!(got, select(aile));
        rec.finish();
    }
}

#[test]
fn excess_trashes_the_weakest_unit() {
    for seed in SEEDS {
        let mut rec = Recorder::new(M, &format!("test_excess_trashes_the_weakest_unit[{seed}]"));
        let mut sc = Sc::new();
        sc.resources(0, 2, 0, 0);
        for _ in 0..5 {
            sc.add(0, "GD01-013");
        }
        let weak = sc.add(0, "ST01-008");
        let strike = sc.in_hand(0, "ST04-005");
        let mut st = sc.start();
        play_card(&mut st, strike, None);
        assert_pending(&st, DecisionKind::Excess);
        let got = choice(&mut rec, &st, &mut *agents(seed)[0]);
        assert_eq!(got, select(weak));
        rec.finish();
    }
}

#[test]
fn takes_a_free_card_from_burst() {
    for seed in SEEDS {
        let mut rec = Recorder::new(M, &format!("test_takes_a_free_card_from_burst[{seed}]"));
        let st = burst();
        let got = choice(&mut rec, &st, &mut *agents(seed)[1]);
        assert_eq!(got, Action::of(ActionKind::Yes));
        rec.finish();
    }
}

#[test]
fn chooses_to_play_first_with_proactive_decks() {
    for seed in SEEDS {
        let mut rec = Recorder::new(
            M,
            &format!("test_chooses_to_play_first_with_proactive_decks[{seed}]"),
        );
        let st = choose_first();
        let chooser = pending_player(&st);
        let got = choice(&mut rec, &st, &mut *agents(seed)[usize::from(chooser)]);
        assert_eq!(got, act1(ActionKind::GoFirst, chooser));
        rec.finish();
    }
}

/// `CHEAP` and `HEAVY` (`test_puzzles.py:263-280`).
const CHEAP: [&str; 10] = [
    "ST01-005", "ST01-008", "ST02-009", "GD01-008", "GD01-011", "GD01-021", "GD01-022", "EB01-051",
    "GD01-079", "GD01-085",
];
const HEAVY: [&str; 5] = ["GD01-065", "EB01-001", "EB01-002", "EB01-004", "EB01-005"];

/// `_find_opening(heavy_at_least, heavy_at_most)` (`test_puzzles.py:296-303`): the first game
/// seed below 2000 whose first redraw decision has that many HEAVY cards in hand, after
/// `_opening` (`new_game((MULLIGAN_DECK,)*2, seed)` and the first option).
fn find_opening(heavy_at_least: usize, heavy_at_most: usize) -> GameState {
    let main: Vec<&str> = CHEAP
        .iter()
        .flat_map(|n| [*n; 4])
        .chain(HEAVY.iter().flat_map(|n| [*n; 2]))
        .collect();
    let res = ["R-001"; 10];
    let reg = registry();
    for seed in 0..2000 {
        let mut st = gcg_engine::new_game_by_number(
            &reg,
            [(&main[..], &res[..]), (&main[..], &res[..])],
            seed,
            NewGameOptions::default(),
        )
        .expect("MULLIGAN_DECK is legal");
        let first = st.pending().expect("a decision").options[0];
        apply_ok(&mut st, first);
        let p = pending_player(&st);
        let n = st
            .zone(p, Zone::Hand)
            .iter()
            .filter(|&&u| HEAVY.contains(&st.def_of(u).card_number.as_ref()))
            .count();
        if (heavy_at_least..=heavy_at_most).contains(&n) {
            return st;
        }
    }
    panic!("no opening hand found");
}

#[test]
fn mulligans_a_hand_of_uncastable_cards() {
    let st = find_opening(4, 5);
    for seed in SEEDS {
        let mut rec = Recorder::new(
            M,
            &format!("test_mulligans_a_hand_of_uncastable_cards[{seed}]"),
        );
        let got = choice(&mut rec, &st, &mut *agents(seed)[0]);
        assert_eq!(got, Action::of(ActionKind::Redraw));
        rec.finish();
    }
}

#[test]
fn keeps_a_hand_with_early_plays() {
    let st = find_opening(0, 0);
    for seed in SEEDS {
        let mut rec = Recorder::new(M, &format!("test_keeps_a_hand_with_early_plays[{seed}]"));
        let got = choice(&mut rec, &st, &mut *agents(seed)[0]);
        assert_eq!(got, Action::of(ActionKind::Keep));
        rec.finish();
    }
}

// ---------------------------------------------------------------------------------------------
// tests/ai/test_puzzles_shipped.py

#[test]
fn shipped_rests_the_blocker_while_a_bigger_unit_is_already_rested() {
    per_seed(
        SHIPPED,
        "test_rests_the_blocker_while_a_bigger_unit_is_already_rested",
        || rests_the_blocker(true, 0),
        wins_this_turn,
    );
}

#[test]
fn shipped_rests_the_active_unit_not_the_rested_one() {
    for seed in SEEDS {
        for hidden in [0, 4] {
            let mut rec = Recorder::new(
                SHIPPED,
                &format!("test_rests_the_active_unit_not_the_rested_one[{hidden}-{seed}]"),
            );
            let extra: &[&str] = if hidden > 0 { &["ST01-005"] } else { &[] };
            let (st, _, zaku) = amuro_rests(extra, hidden, None);
            let got = rec.choose(&mut *agents(seed)[0], &st, 0);
            assert_eq!(got, select(zaku));
            rec.finish();
        }
    }
}

#[test]
fn shipped_rests_the_active_unit_with_a_hidden_hand() {
    for seed in SEEDS {
        let mut rec = Recorder::new(
            SHIPPED,
            &format!("test_rests_the_active_unit_with_a_hidden_hand[{seed}]"),
        );
        let (st, want) = rest_the_active_unit();
        let p = pending_player(&st);
        assert_eq!(rec.choose(&mut *agents(seed)[0], &st, p), want);
        rec.finish();
    }
}

#[test]
fn shipped_protects_the_attacked_unit() {
    for seed in SEEDS {
        for big in [true, false] {
            let label = if big { "True" } else { "False" };
            let mut rec = Recorder::new(
                SHIPPED,
                &format!("test_protects_the_attacked_unit[{label}-{seed}]"),
            );
            let (mut st, target) = protection(big);
            let mut players = agents(seed);
            while let Some(dec) = st.pending() {
                if st.battle().is_none() {
                    break;
                }
                let p = dec.hdr.player;
                let a = rec.choose(&mut *players[usize::from(p)], &st, p);
                apply_ok(&mut st, a);
            }
            assert_eq!(st.card(target).zone, Zone::Battle);
            rec.finish();
        }
    }
}

// ---------------------------------------------------------------------------------------------
// tests/ai/test_puzzles_targets.py

/// `EXTENDED = replace(BASELINE_PRESETS["standard"], target_extension=3.0)`
/// (`test_puzzles_targets.py:42-43`).
fn extended() -> ReferenceConfig {
    let cfg = ReferenceConfig {
        target_extension: 3.0,
        ..BASELINE_STANDARD.clone()
    };
    cfg.check().expect("a valid configuration");
    cfg
}

/// `passes(build, seeds, config)` (`test_puzzles_targets.py:96-104`).
fn passes(
    rec: &mut Recorder,
    build: fn() -> (GameState, Action),
    seeds: u64,
    cfg: &ReferenceConfig,
) -> u32 {
    let (start, want) = build();
    assert_pending(&start, DecisionKind::Select);
    let player = pending_player(&start);
    let mut solved = 0;
    for seed in 1..=seeds {
        let mut a = agent("mcts", seed, false, Some(cfg), Some(BASELINE_WEIGHTS));
        solved += u32::from(rec.choose(&mut *a, &start.clone(), player) == want);
    }
    solved
}

#[test]
fn close_target_calls_get_more_iterations() {
    let cfg = extended();
    let mut sorted = PUZZLES;
    sorted.sort_by_key(|p| p.0);
    for (name, build, seeds, required) in sorted {
        let mut rec = Recorder::new(
            TARGETS,
            &format!("test_close_target_calls_get_more_iterations[{name}]"),
        );
        let solved = passes(&mut rec, build, seeds, &cfg);
        assert!(solved >= required, "{name}: {solved} < {required}");
        rec.finish();
    }
}

#[test]
fn negative_control_the_baseline_misses_the_pass_rate() {
    let mut sorted = PUZZLES;
    sorted.sort_by_key(|p| p.0);
    for (name, build, seeds, required) in sorted {
        let mut rec = Recorder::new(
            TARGETS,
            &format!("test_negative_control_the_baseline_misses_the_pass_rate[{name}]"),
        );
        let solved = passes(&mut rec, build, seeds, &BASELINE_STANDARD);
        assert!(solved < required, "{name}: {solved} >= {required}");
        rec.finish();
    }
}
