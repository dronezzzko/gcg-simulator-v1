//! P6-K behaviour tests on the Rust testkit (docs/dev/PLAN.md §12.2 P6-K;
//! docs/dev/eval/weakness-status/P6-K.md).
//!
//! - W-60: the three stripped parentheticals whose meaning the bindings implement (GD05-089
//!   "Don't treat it as a Pilot.", ST12-013 "You choose first, then your opponent.", ST14-014
//!   "This card that is being activated is not counted.").
//! - W-66: the reviewed hits of the miscompile scan (`crates/gcg-effects/tests/p6k_ir_scans.rs`):
//!   compiled two-target steps with no all-targets guard act only when both targets exist,
//!   because the engine's multi-target pre-check (rules 10-1-8-1-1, 10-2-2, 10-3-3-1) covers
//!   them. The same scenarios run on the Python testkit in
//!   `oracle/tests/test_p6k_miscompile.py`.
//! - W-68: each HP-fix binding and the script the Python compiler gives the same card
//!   (`oracle/tests/vectors/p6k_hp_fragments.json`) give identical decisions and states on
//!   a set of HP/damage states: `Stat.HP` already means current HP.

use std::sync::Arc;

use gcg_effects::Registry;
use gcg_engine::canon::state_digest;
use gcg_engine::testkit::{attack, has_action, play, select, yes, zone_of};
use gcg_engine::{
    ActionKind, AddOptions, DecisionKind, GameState, PLAYER_TARGET, Scenario, ScenarioOptions,
    Step, TestkitError, Zone, apply,
};
use serde_json::Value;

const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

const VANILLA_2_2: &str = "GD01-060"; // Zaku Mariner, Lv2 2/2
const VANILLA_3_4: &str = "GD01-013"; // Gundam, Lv4 3/4, no effects
const VANILLA_6_4: &str = "ST14-008"; // Gundam Heavyarms Custom (EW), Lv6 6/4
const VANILLA_5_4: &str = "ST07-003"; // Gundam Virtue, Lv5 5/4
const EF_4_3: &str = "GD01-018"; // blue (Earth Federation) Lv3 4/3
const VAGAN_3_3: &str = "GD03-065"; // vanilla Lv3 3/3 (Vagan)
const MF_CARD: &str = "GD05-042"; // an (MF) card for GD05-089's Burst condition
const PILOT: &str = "GD05-091"; // the Pilot of tests/cards/test_wp_gd05_b.py:713-722
const FINAL_VICTOR: &str = "ST12-013";
const BLAZING: &str = "ST14-014";

fn scenario() -> Scenario {
    Scenario::new(&Registry::embedded(), ScenarioOptions::default())
}

fn add(sc: &mut Scenario, player: u8, number: &str, zone: Zone) -> u16 {
    sc.add(player, number, zone, AddOptions::default())
        .unwrap_or_else(|e| panic!("{number}: {e:?}"))
}

fn add_damaged(sc: &mut Scenario, player: u8, number: &str, damage: i32) -> u16 {
    let opts = AddOptions {
        damage,
        ..AddOptions::default()
    };
    sc.add(player, number, Zone::Battle, opts)
        .unwrap_or_else(|e| panic!("{number}: {e:?}"))
}

fn pending(st: &GameState) -> Option<(DecisionKind, u8)> {
    st.pending().map(|d| (d.hdr.kind, d.hdr.player))
}

fn playable(st: &GameState, uid: u16) -> bool {
    has_action(st, ActionKind::PlayCommand, Some(i32::from(uid)), None)
}

// ---------------------------------------------------------------------------------------------
// W-60

// gcg-tags: card=GD05-089 ruling=GD05-089:Q390
#[test]
fn w60_gd05_089_deployed_as_a_unit_is_not_a_pilot() -> Result<(), TestkitError> {
    let mut sc = scenario();
    sc.resources(1, 5, 0, 0)?;
    let attacker = add(&mut sc, 0, VANILLA_2_2, Zone::Battle);
    let pilot = add(&mut sc, 1, PILOT, Zone::Hand);
    let other = add(&mut sc, 1, VANILLA_3_4, Zone::Battle);
    let asia = sc.shields(1, &["GD05-089"])?[0];
    sc.trash(1, &[MF_CARD, "GD05-043", MF_CARD])?;
    sc.start(Step::Main)?;
    let st = sc.state_mut();
    attack(st, attacker, PLAYER_TARGET)?;
    yes(st)?;
    if pending(st).map(|p| p.0) == Some(DecisionKind::YesNo) {
        yes(st)?;
    }
    assert_eq!(zone_of(st, asia), Zone::Battle);
    let def = st.def_of(asia);
    assert!(def.card_number.starts_with("UNIT:"), "{}", def.card_number);
    assert!(def.card_type.is_unit() && !def.is_pilot_capable());
    gcg_engine::testkit::to_next_turn(st)?;
    assert_eq!(st.active(), 1);
    assert!(
        has_action(
            st,
            ActionKind::Pair,
            Some(i32::from(pilot)),
            Some(i32::from(asia))
        ),
        "a Pilot can be paired with it (Q390)"
    );
    assert!(
        !has_action(
            st,
            ActionKind::Pair,
            Some(i32::from(asia)),
            Some(i32::from(other))
        ),
        "it is not treated as a Pilot: it pairs with no Unit"
    );
    Ok(())
}

// gcg-tags: card=ST12-013
#[test]
fn w60_st12_013_you_choose_first_then_your_opponent() -> Result<(), TestkitError> {
    let mut sc = scenario();
    sc.resources(0, 6, 0, 0)?;
    let mine = [
        add(&mut sc, 0, VANILLA_6_4, Zone::Battle),
        add(&mut sc, 0, VANILLA_5_4, Zone::Battle),
    ];
    let theirs = [
        add(&mut sc, 1, VANILLA_2_2, Zone::Battle),
        add(&mut sc, 1, VANILLA_3_4, Zone::Battle),
    ];
    let victor = add(&mut sc, 0, FINAL_VICTOR, Zone::Hand);
    sc.start(Step::Main)?;
    let st = sc.state_mut();
    play(st, victor, None, 0)?;
    assert_eq!(
        pending(st),
        Some((DecisionKind::Select, 0)),
        "you choose first"
    );
    select(st, &[mine[0]], None)?;
    assert_eq!(
        pending(st),
        Some((DecisionKind::Select, 1)),
        "then your opponent"
    );
    select(st, &[theirs[1]], None)?;
    assert_eq!(zone_of(st, theirs[1]), Zone::Trash);
    assert_eq!(st.card(mine[0]).damage, 3);
    assert_eq!(st.card(mine[1]).damage, 0);
    assert_eq!(st.card(theirs[0]).damage, 0);
    Ok(())
}

fn blazing(commands: usize, enemies: &[&str]) -> Result<(Scenario, u16), TestkitError> {
    let mut sc = scenario();
    sc.resources(0, 5, 0, 0)?;
    let card = add(&mut sc, 0, BLAZING, Zone::Hand);
    for e in enemies {
        add(&mut sc, 1, e, Zone::Battle);
    }
    let mut trash = vec![FINAL_VICTOR; commands];
    trash.push(VANILLA_2_2);
    sc.trash(0, &trash)?;
    sc.start(Step::Main)?;
    Ok((sc, card))
}

// gcg-tags: card=ST14-014
#[test]
fn w60_st14_014_does_not_count_itself() -> Result<(), TestkitError> {
    let (sc, card) = blazing(3, &[VANILLA_6_4])?;
    assert!(
        !playable(sc.state(), card),
        "3 Commands in the trash plus the activating card are not 4"
    );
    let (mut sc, card) = blazing(4, &[VANILLA_6_4])?;
    assert!(playable(sc.state(), card));
    let st = sc.state_mut();
    play(st, card, None, 0)?;
    let enemy = st.zone(1, Zone::Battle)[0];
    assert_eq!(gcg_engine::testkit::ap(st, enemy)?, 3);
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// W-66: the reviewed hits (two-target steps).

/// (card, friendly Unit it needs, resources) for the four compiled two-target Commands.
const TWO_TARGET_COMMANDS: [(&str, &str, usize); 4] = [
    ("GD01-103", EF_4_3, 1),
    ("GD03-116", VAGAN_3_3, 3),
    ("GD04-114", VANILLA_3_4, 2),
    ("GD05-117", VANILLA_3_4, 3),
];

// gcg-tags: card=GD01-103 card=GD03-116 card=GD04-114 card=GD05-117 rule=10-1-8-1-1 rule=10-2-2
#[test]
fn w66_two_target_commands_need_both_targets() -> Result<(), TestkitError> {
    for (number, friendly, resources) in TWO_TARGET_COMMANDS {
        let mut sc = scenario();
        sc.resources(0, resources, 0, 0)?;
        let mine = add(&mut sc, 0, friendly, Zone::Battle);
        let cmd = add(&mut sc, 0, number, Zone::Hand);
        sc.start(Step::Main)?;
        assert!(
            !playable(sc.state(), cmd),
            "{number}: not playable without the enemy target"
        );
        let mut sc = scenario();
        sc.resources(0, resources, 0, 0)?;
        let mine2 = add(&mut sc, 0, friendly, Zone::Battle);
        let foe = add(&mut sc, 1, VANILLA_3_4, Zone::Battle);
        let cmd = add(&mut sc, 0, number, Zone::Hand);
        sc.start(Step::Main)?;
        let st = sc.state_mut();
        assert!(playable(st, cmd), "{number}: playable with both targets");
        play(st, cmd, None, 0)?;
        let touched = |u: u16| st.card(u).rested || st.card(u).damage > 0;
        assert!(
            touched(mine2) && touched(foe),
            "{number}: both targets are affected"
        );
        assert_eq!(mine, mine2);
    }
    Ok(())
}

// gcg-tags: card=GD02-055 rule=10-3-3-1 rule=10-2-2
#[test]
fn w66_gd02_055_deploy_needs_both_targets() -> Result<(), TestkitError> {
    let mut sc = scenario();
    sc.resources(0, 5, 0, 0)?;
    let friend = add(&mut sc, 0, "GD02-015", Zone::Battle);
    let gusion = add(&mut sc, 0, "GD02-055", Zone::Hand);
    sc.start(Step::Main)?;
    let st = sc.state_mut();
    play(st, gusion, None, 0)?;
    assert_eq!(zone_of(st, gusion), Zone::Battle);
    assert_eq!(pending(st), Some((DecisionKind::Main, 0)));
    assert_eq!(
        st.card(friend).damage,
        0,
        "no enemy Unit: the friendly Unit is not damaged"
    );
    assert_eq!(st.card(gusion).damage, 0);
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// W-68: bound and compiled HP filters on HP/damage states.

/// How a card's HP-filtered effect is set off: by pairing `pilot` with `unit`, by playing the
/// card itself (a 【Deploy】 Unit or a Command).
enum Trigger {
    Pair {
        unit: &'static str,
        pilot: &'static str,
    },
    Play,
}

/// The current-HP condition of a card's filter.
type HpTest = fn(i32) -> bool;

/// (card, trigger, threshold test on current HP).
const HP_CARDS: [(&str, Trigger, HpTest); 8] = [
    (
        "GD01-004",
        Trigger::Pair {
            unit: "GD01-004",
            pilot: "GD01-089",
        },
        |hp| hp <= 2,
    ),
    ("ST01-004", Trigger::Play, |hp| hp <= 2),
    (
        "ST01-010",
        Trigger::Pair {
            unit: VANILLA_3_4,
            pilot: "ST01-010",
        },
        |hp| hp <= 5,
    ),
    ("ST02-014", Trigger::Play, |hp| hp <= 5),
    (
        "ST04-001",
        Trigger::Pair {
            unit: "ST04-001",
            pilot: "ST01-010",
        },
        |hp| hp <= 4,
    ),
    ("ST04-013", Trigger::Play, |hp| hp <= 3),
    (
        "GD05-072",
        Trigger::Pair {
            unit: "GD05-072",
            pilot: "GD05-097",
        },
        |hp| hp <= 4,
    ),
    (
        "GD05-083",
        Trigger::Pair {
            unit: VANILLA_3_4,
            pilot: "GD05-083",
        },
        |hp| hp == 1,
    ),
];

/// The enemy Units: printed HP 4 with 0-3 damage, so current HP 4, 3, 2, 1, 1 (two at 1 HP,
/// so every threshold leaves a choice between at least two Units).
const ENEMY_DAMAGE: [i32; 5] = [0, 1, 2, 3, 3];

struct HpRun {
    trace: Vec<String>,
    first_select: Vec<i32>,
    enemies: Vec<(u16, i32)>,
}

fn hp_run(reg: &Arc<Registry>, card: &str, trigger: &Trigger) -> Result<HpRun, TestkitError> {
    let mut sc = Scenario::new(reg, ScenarioOptions::default());
    sc.resources(0, 6, 0, 0)?;
    let enemies: Vec<(u16, i32)> = ENEMY_DAMAGE
        .iter()
        .map(|&d| (add_damaged(&mut sc, 1, VANILLA_3_4, d), 4 - d))
        .collect();
    let (actor, onto) = match trigger {
        Trigger::Pair { unit, pilot } => {
            let u = add(&mut sc, 0, unit, Zone::Battle);
            (add(&mut sc, 0, pilot, Zone::Hand), Some(u))
        }
        Trigger::Play => (add(&mut sc, 0, card, Zone::Hand), None),
    };
    sc.start(Step::Main)?;
    let st = sc.state_mut();
    play(st, actor, onto, 0)?;
    let mut trace = Vec::new();
    let mut first_select = Vec::new();
    for _ in 0..60 {
        let Some(d) = st.pending() else { break };
        if d.hdr.kind == DecisionKind::Main && d.hdr.player == 0 {
            break;
        }
        if d.hdr.kind == DecisionKind::Select && first_select.is_empty() {
            first_select = d
                .options
                .iter()
                .filter(|o| o.kind == ActionKind::Select)
                .map(|o| o.a)
                .collect();
        }
        let options: Vec<String> = d.options.iter().map(ToString::to_string).collect();
        trace.push(format!(
            "{:?} p{}: {}",
            d.hdr.kind,
            d.hdr.player,
            options.join(" ")
        ));
        let first = d.options[0];
        apply(st, first, true).map_err(|e| TestkitError::Assertion(e.to_string()))?;
    }
    trace.push(state_digest(st));
    Ok(HpRun {
        trace,
        first_select,
        enemies,
    })
}

// gcg-tags: card=GD01-004 card=ST01-004 card=ST01-010 card=ST02-014 card=ST04-001 card=ST04-013 card=GD05-072 card=GD05-083 faq=Q96
#[test]
fn w68_compiled_and_bound_hp_filters_agree_on_hp_damage_states() -> Result<(), TestkitError> {
    let text =
        std::fs::read_to_string(format!("{ROOT}/oracle/tests/vectors/p6k_hp_fragments.json"))
            .expect("the W-68 vector");
    let vector: Value = serde_json::from_str(&text).expect("JSON");
    let bound = Registry::embedded();
    let cards = vector["cards"].as_array().expect("cards");
    assert_eq!(cards.len(), HP_CARDS.len());
    for (entry, (card, trigger, wanted)) in cards.iter().zip(HP_CARDS.iter()) {
        assert_eq!(entry["card_number"], *card);
        let b = entry["bound"].as_str().expect("bound repr");
        let c = entry["compiled"].as_str().expect("compiled repr");
        let without_notes = |r: &str| r.split(", notes=").next().unwrap_or(r).to_owned();
        assert_eq!(
            without_notes(c)
                .replace("Stat.HP: 'hp'", "Stat.REMAINING_HP: 'remaining_hp'")
                .replace("source='compiled'", "source='binding'"),
            without_notes(b),
            "{card}: the scripts differ only in the HP stat"
        );
        let compiled = Arc::new(
            bound
                .with_fragment(&entry["fragment"])
                .unwrap_or_else(|e| panic!("{card}: {e}")),
        );
        let def = bound.db().get(card).expect("card").def_id;
        assert_ne!(
            compiled.card(def).script,
            bound.card(def).script,
            "{card}: the fragment replaces the bound script"
        );
        let with_binding = hp_run(&bound, card, trigger)?;
        let with_compiler = hp_run(&compiled, card, trigger)?;
        assert_eq!(with_binding.trace, with_compiler.trace, "{card}");
        let expected: Vec<i32> = with_binding
            .enemies
            .iter()
            .filter(|(_, hp)| wanted(*hp))
            .map(|(u, _)| i32::from(*u))
            .collect();
        let mut got = with_binding.first_select.clone();
        got.retain(|u| {
            with_binding
                .enemies
                .iter()
                .any(|(e, _)| i32::from(*e) == *u)
        });
        assert_eq!(
            got, expected,
            "{card}: the targets are the Units with that current HP"
        );
    }
    Ok(())
}
