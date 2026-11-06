//! GAP-B, W-101 (docs/dev/WEAKNESSES.md): the interpretation-tagged code paths of the engine
//! record when they run (`gcg_engine::diag::interpretation_runs`), on the running thread only,
//! without changing a game.
//!
//! - Over seeded random games between the example decks, the setup interpretations run in
//!   every game (I-05 once per game, I-15 once per Shield placed), attacks on Bases and tokens
//!   leaving the game run in some, and the per-game counts are the same when a game is played
//!   again (the record is a function of the game).
//! - A game's final state does not depend on whether the counters were read.
//! - The resource cap (I-03) runs when a Resource is placed onto a full resource area.
//! - The rule-level readings I-04 (a lasting "all your Units" effect), I-11 (modal Commands),
//!   I-13 (a card deployed as an "(APx･HPy) Unit") and I-17 (【When Linked】 on pairing) run in
//!   the scenarios of their Python tests, and a scenario without the reading does not record it.

mod l5_common;

use gcg_core::rng::SplitMix64;
use gcg_effects::{Kw, Stat};
use gcg_engine::canon::state_digest;
use gcg_engine::diag::{Interpretation, interpretation_runs};
use gcg_engine::testkit::{
    ap, attack, choose_option, engine, has_action, keywords, play, select_if_asked, yes, zone_of,
};
use gcg_engine::{
    ActionKind, AddOptions, DecisionKind, GameState, NewGameOptions, PLAYER_TARGET, Scenario,
    ScenarioOptions, Step, Zone, apply, legal_actions,
};
use l5_common::{engine_records, reg, start_game};
use serde_json::{Value, json};

const VANILLA: &str = "GD01-060"; // Zaku Mariner: Lv2 cost1 2/2, no effects
const GM: &str = "ST01-005"; // GM: Lv2 cost1 2/2, no effects
const ALL_YOUR_UNITS: &str = "GD01-105"; // 【Main】All your Units get AP+2 during this turn.
const NATURAL_TALENT: &str = "ST14-013"; // a modal Command; both modes choose enemy Units
const MASTER_ASIA: &str = "GD05-089"; // 【Burst】... deploy it as an (AP3･HP3) Unit instead
const MF_CARDS: [&str; 3] = ["GD05-042", "GD05-043", "GD05-042"]; // (MF) Units
const GYAN: &str = "ST12-007"; // 【When Linked】<First Strike> during this turn; links M'Quve
const MQUVE: &str = "GD01-092"; // a Pilot that links with Gyan
const RIDDHE: &str = "GD01-089"; // a Pilot that does not link with Gyan

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
    for i in [
        Interpretation::I01,
        Interpretation::I02,
        Interpretation::I17,
    ] {
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

/// The interpretation runs that `f` makes on this thread, with its result.
fn runs_of<T>(f: impl FnOnce() -> T) -> (T, [u64; Interpretation::ALL.len()]) {
    let before = interpretation_runs();
    let out = f();
    let after = interpretation_runs();
    (out, std::array::from_fn(|i| after[i] - before[i]))
}

fn scenario() -> Scenario {
    Scenario::new(&reg(), ScenarioOptions::default())
}

fn pending_kind(st: &GameState) -> Option<DecisionKind> {
    st.pending().map(|d| d.hdr.kind)
}

/// `tests/rules/test_faq.py::test_q105_all_your_units_buff_does_not_reach_units_deployed_later`:
/// the lasting effect holds the Units in play when it resolves (I-04), so a Unit deployed later
/// gets nothing. No modal path runs (I-11).
// gcg-tags: faq=Q105 card=GD01-105
#[test]
fn w101_a_lasting_all_your_units_effect_records_i04() {
    let mut sc = scenario();
    sc.resources(0, 5, 0, 0).expect("resources");
    let old = sc
        .add(0, VANILLA, Zone::Battle, AddOptions::default())
        .expect("a Unit");
    let cmd = sc
        .add(0, ALL_YOUR_UNITS, Zone::Hand, AddOptions::default())
        .expect("the Command");
    let new = sc
        .add(0, GM, Zone::Hand, AddOptions::default())
        .expect("a Unit");
    let ((), s) = runs_of(|| sc.start(Step::Main).expect("start"));
    let st = sc.state_mut();
    let ((), n) = runs_of(|| {
        play(st, cmd, None, 0).expect("the Command");
    });
    assert_eq!(ap(st, old).expect("AP"), 4);
    assert_eq!(n[index(Interpretation::I04)], 1, "I-04 at the Apply");
    for k in [s, n] {
        assert_eq!(k[index(Interpretation::I11)], 0, "a Command without modes");
    }
    let ((), m) = runs_of(|| {
        play(st, new, None, 0).expect("the Unit");
    });
    assert_eq!(
        ap(st, new).expect("AP"),
        2,
        "Q105: the later Unit gets nothing"
    );
    assert_eq!(
        m[index(Interpretation::I04)],
        0,
        "deploying applies nothing"
    );
}

/// `tests/cards/test_wp_st12_14.py::test_st14_013_q469_not_playable_without_enemy_units`: the
/// playability check of a modal Command walks its modes (I-11). With an enemy Unit the mode
/// choice offers both modes (I-11 again), and the chosen mode's `Apply` on a chosen Unit is no
/// "all your Units" effect (no I-04).
// gcg-tags: card=ST14-013 ruling=ST14-013:Q469
#[test]
fn w101_a_modal_command_records_i11() {
    let mut sc = scenario();
    sc.resources(0, 4, 0, 0).expect("resources");
    let card = sc
        .add(0, NATURAL_TALENT, Zone::Hand, AddOptions::default())
        .expect("the Command");
    let ((), n) = runs_of(|| sc.start(Step::Main).expect("start"));
    assert!(
        !has_action(
            sc.state(),
            ActionKind::PlayCommand,
            Some(i32::from(card)),
            None
        ),
        "Q469: no enemy Unit, no legal mode"
    );
    assert!(
        n[index(Interpretation::I11)] > 0,
        "I-11 at the playability check of the MAIN decision"
    );

    let mut sc = scenario();
    sc.resources(0, 4, 0, 0).expect("resources");
    let enemy = sc
        .add(1, VANILLA, Zone::Battle, AddOptions::default())
        .expect("an enemy Unit");
    let card = sc
        .add(0, NATURAL_TALENT, Zone::Hand, AddOptions::default())
        .expect("the Command");
    let ((), s) = runs_of(|| sc.start(Step::Main).expect("start"));
    assert!(s[index(Interpretation::I11)] > 0, "the playability check");
    let st = sc.state_mut();
    let ((), n) = runs_of(|| {
        play(st, card, None, 0).expect("the Command");
    });
    assert_eq!(
        pending_kind(st),
        Some(DecisionKind::Select),
        "the mode choice"
    );
    let modes: Vec<i32> = legal_actions(st)
        .iter()
        .filter(|a| a.kind == ActionKind::Select)
        .map(|a| a.a)
        .collect();
    assert_eq!(modes, [0, 1], "both modes have their targets");
    assert!(n[index(Interpretation::I11)] > 0, "the mode choice");
    let ((), m) = runs_of(|| {
        choose_option(st, 1).expect("mode 2");
        select_if_asked(st, &[enemy], None).expect("the enemy Unit");
    });
    assert_eq!(ap(st, enemy).expect("AP"), 0, "AP-3 on a 2-AP Unit");
    assert_eq!(
        m[index(Interpretation::I04)],
        0,
        "a chosen target is no All selection"
    );
}

/// `tests/cards/test_wp_gd05_b.py::test_gd05_089_deployed_as_unit_is_lv6`: the Pilot card that
/// its Burst deploys as an (AP3･HP3) Unit is that Unit, Lv.6, in the battle area (I-13 at the
/// deploy), and the printed card again once it leaves (I-13 at the move).
// gcg-tags: card=GD05-089 ruling=GD05-089:Q389
#[test]
fn w101_a_card_deployed_as_a_unit_records_i13() {
    let mut sc = scenario();
    let attacker = sc
        .add(0, VANILLA, Zone::Battle, AddOptions::default())
        .expect("a Unit");
    let asia = sc.shields(1, &[MASTER_ASIA]).expect("the Shield")[0];
    sc.trash(1, &MF_CARDS).expect("three (MF) cards");
    sc.start(Step::Main).expect("start");
    let st = sc.state_mut();
    let printed = st.def_of(asia).def_id;
    let ((), n) = runs_of(|| {
        attack(st, attacker, PLAYER_TARGET).expect("the attack");
        yes(st).expect("the Burst");
        if pending_kind(st) == Some(DecisionKind::YesNo) {
            yes(st).expect("deploy it as a Unit");
        }
    });
    assert_eq!(zone_of(st, asia), Zone::Battle);
    assert!(st.def_of(asia).card_type.is_unit());
    let dv = gcg_engine::view::derived(st).expect("a view");
    assert_eq!(gcg_engine::view::stat(st, &dv, asia, Stat::Lv), 6, "Q389");
    drop(dv);
    assert_eq!(n[index(Interpretation::I13)], 1, "I-13 at the deploy");
    let ((), m) = runs_of(|| {
        engine::move_card(st, asia, Zone::Trash, false, false, false).expect("to the trash");
    });
    assert_eq!(st.def_of(asia).def_id, printed, "the printed card again");
    assert_eq!(m[index(Interpretation::I13)], 1, "I-13 at the move");
}

/// `tests/cards/test_wp_gd05_b.py::test_gd05_089_burst_adds_it_to_hand_otherwise` with two (MF)
/// cards: the Burst adds the card to the hand and no card becomes a Unit variant (no I-13).
// gcg-tags: card=GD05-089
#[test]
fn w101_a_burst_added_to_the_hand_does_not_record_i13() {
    let mut sc = scenario();
    let attacker = sc
        .add(0, VANILLA, Zone::Battle, AddOptions::default())
        .expect("a Unit");
    let asia = sc.shields(1, &[MASTER_ASIA]).expect("the Shield")[0];
    sc.trash(1, &MF_CARDS[..2]).expect("two (MF) cards");
    sc.start(Step::Main).expect("start");
    let st = sc.state_mut();
    let ((), n) = runs_of(|| {
        attack(st, attacker, PLAYER_TARGET).expect("the attack");
        yes(st).expect("the Burst");
    });
    assert_eq!(zone_of(st, asia), Zone::Hand);
    assert_eq!(n[index(Interpretation::I13)], 0);
}

/// `tests/rules/test_keywords.py::_pair_gyan`: Gyan (link M'Quve) paired with M'Quve emits
/// LINKED (I-17) and gains <First Strike>; paired with Riddhe, a Pilot without the link, it
/// does neither.
// gcg-tags: rule=13-2-11-1
#[test]
fn w101_when_linked_on_a_linked_pairing_records_i17() {
    for (pilot_number, linked) in [(MQUVE, true), (RIDDHE, false)] {
        let mut sc = scenario();
        sc.resources(0, 3, 0, 0).expect("resources");
        let gyan = sc
            .add(0, GYAN, Zone::Battle, AddOptions::default())
            .expect("Gyan");
        let pilot = sc
            .add(0, pilot_number, Zone::Hand, AddOptions::default())
            .expect("a Pilot");
        sc.start(Step::Main).expect("start");
        let st = sc.state_mut();
        let ((), n) = runs_of(|| {
            play(st, pilot, Some(gyan), 0).expect("the pairing");
        });
        assert_eq!(zone_of(st, pilot), Zone::Paired, "{pilot_number}");
        let first_strike = keywords(st, gyan)
            .expect("keywords")
            .iter()
            .any(|&(k, _)| k == Kw::FirstStrike);
        assert_eq!(first_strike, linked, "{pilot_number}: 【When Linked】");
        assert_eq!(
            n[index(Interpretation::I17)],
            u64::from(linked),
            "{pilot_number}: I-17"
        );
    }
}
