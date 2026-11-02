//! P6-B's unit tests: the static attack step and its engine-correct Blockers (W-32), the
//! strengthened race and `can_kill` features against the reference's (W-38), the solver's
//! lethal, setup, trick and defence lines (W-03, W-17), the leaf values, the critical-decision
//! budget (W-16) and the solver budget of root trees.

use gcg_ai::{PolicyKnobs, ROLE_WEIGHTS, role_features, values};
use gcg_effects::Registry;
use gcg_engine::testkit;
use gcg_engine::{
    Action, ActionKind, AddOptions, DecisionKind, GameState, PLAYER_TARGET, Scenario,
    ScenarioOptions, Step, Uid, Zone,
};

use super::board::{Board, Turn, effective_blockers, race, static_lethal};
use super::leaf::{can_kill, leaf_value, race_feature};
use super::solver::{Limits, Res, Verdict, forced_win, lethal_line};
use super::{DecisionState, LINE_LIMITS, PROVIDES, SOLVER_APPLIES_PER_ITERATION, budget};
use crate::mech::p6o::math::Math;
use crate::mech::reference::{LEAF_FLOOR, WIN_DECAY};
use crate::mech::{Budget, LeafCx, RootCx};
use crate::{MechanismFlags, Preset, StrengthenedConfig};

/// Zaku Mariner, a vanilla 2/2 (`tests/ai/test_puzzles.py:24`).
const FILLER: &str = "GD01-060";
/// GM, a vanilla 2/2 attacker.
const GM: &str = "ST01-005";
/// Demi Trainer, a 1/1 <Blocker>.
const DEMI: &str = "ST01-008";
/// Wing Gundam Zero, 5/7 <High-Maneuver>.
const WING_ZERO: &str = "GD01-024";
/// GQuuuuuuX (Omega Psycommu), 6/5 <Suppression>.
const SUPPRESSOR: &str = "GD03-034";
/// Siege Ploy: 【Main】/【Action】 rest 1 enemy Unit with 5 or less HP.
const SIEGE_PLOY: &str = "ST02-014";
/// Battle of Aces: 【Main】/【Action】 deal 3 damage to 1 damaged enemy Unit.
const BATTLE_OF_ACES: &str = "GD01-111";
/// Girty Lue, a Base: when a friendly (Phantom Pain) Unit links, 1 enemy Unit can't activate
/// <Blocker> during this turn (CANT_BLOCK, `effects/bindings/wp_gd05_b.py:622-640`).
const GIRTY_LUE: &str = "GD05-127";
/// Gaia Gundam, a 3/3 (Phantom Pain) Unit that links with Stellar Loussier.
const GAIA: &str = "GD05-034";
const STELLAR: &str = "GD05-090";

const ATTACKS_ONLY: Limits = LINE_LIMITS;
const WITH_SETUPS: Limits = Limits {
    max_applies: 4000,
    setups: 1,
    own_tricks: true,
};

fn scenario(active: u8) -> Scenario {
    Scenario::new(
        &Registry::embedded(),
        ScenarioOptions {
            active,
            ..ScenarioOptions::default()
        },
    )
}

fn add(sc: &mut Scenario, player: u8, number: &str) -> Uid {
    sc.add(player, number, Zone::Battle, AddOptions::default())
        .unwrap_or_else(|e| panic!("add {number}: {e:?}"))
}

fn add_with(sc: &mut Scenario, player: u8, number: &str, zone: Zone, o: AddOptions<'_>) -> Uid {
    sc.add(player, number, zone, o)
        .unwrap_or_else(|e| panic!("add {number}: {e:?}"))
}

fn rested(sc: &mut Scenario, player: u8, number: &str) -> Uid {
    add_with(
        sc,
        player,
        number,
        Zone::Battle,
        AddOptions {
            rested: true,
            ..AddOptions::default()
        },
    )
}

fn shields(sc: &mut Scenario, player: u8, n: usize) {
    sc.shields(player, &vec![FILLER; n]).expect("shields");
}

fn start(mut sc: Scenario) -> GameState {
    sc.deck(0, &[FILLER; 12]);
    sc.deck(1, &[FILLER; 12]);
    sc.start(Step::Main).expect("the scenario starts");
    sc.into_state()
}

fn knobs() -> PolicyKnobs {
    crate::preset_config(Preset::Standard).knobs()
}

fn attack(uid: Uid) -> Action {
    Action::new(ActionKind::Attack, i32::from(uid), PLAYER_TARGET, -1)
}

fn attacks_of(st: &GameState) -> Vec<Action> {
    super::board::player_attacks(st.pending().expect("a decision").options)
}

/// The searcher's board: `attackers` GMs and the defender's `shields` Shields, `blockers`
/// active Demi Trainers and `rested_blockers` rested ones.
fn board(attackers: usize, shields_n: usize, blockers: usize, rested_blockers: usize) -> GameState {
    let mut sc = scenario(0);
    for _ in 0..attackers {
        add(&mut sc, 0, GM);
    }
    for _ in 0..blockers {
        add(&mut sc, 1, DEMI);
    }
    for _ in 0..rested_blockers {
        rested(&mut sc, 1, DEMI);
    }
    shields(&mut sc, 0, 3);
    shields(&mut sc, 1, shields_n);
    start(sc)
}

#[test]
fn the_package_provides_its_six_flags() {
    assert_eq!(
        PROVIDES,
        [
            "critical_extension",
            "effective_blockers",
            "lethal_root",
            "solver_can_kill",
            "tactical_solver",
            "threat_leaf",
        ]
    );
    for name in PROVIDES {
        assert_eq!(
            MechanismFlags::FIELDS
                .iter()
                .find(|(n, _)| *n == name)
                .map(|&(_, p)| p),
            Some("P6-B"),
            "{name}"
        );
    }
}

#[test]
fn the_static_attack_step_counts_hits_blockers_bases_and_keywords() {
    // Exposed defender: one attacker and no Blocker is lethal; an active Blocker stops it; a
    // rested one does not.
    assert!(static_lethal(&board(1, 0, 0, 0), 0, Turn::Now).expect("board"));
    assert!(!static_lethal(&board(1, 0, 1, 0), 0, Turn::Now).expect("board"));
    assert!(static_lethal(&board(1, 0, 0, 1), 0, Turn::Now).expect("board"));
    // Two Shields need three hits; one Blocker leaves two.
    assert!(static_lethal(&board(3, 2, 0, 0), 0, Turn::Now).expect("board"));
    assert!(!static_lethal(&board(2, 2, 0, 0), 0, Turn::Now).expect("board"));
    assert!(!static_lethal(&board(3, 2, 1, 0), 0, Turn::Now).expect("board"));
    assert!(static_lethal(&board(4, 2, 1, 0), 0, Turn::Now).expect("board"));
    // <High-Maneuver> can't be blocked.
    let mut sc = scenario(0);
    add(&mut sc, 0, WING_ZERO);
    add(&mut sc, 1, DEMI);
    shields(&mut sc, 0, 3);
    let st = start(sc);
    let b = Board::of(&st, 0, Turn::Now).expect("board");
    assert_eq!(b.attackers.len(), 1);
    assert!(b.attackers[0].unblockable && b.attackers[0].blockable_by == 0);
    assert!(b.lethal());
    // <Suppression> destroys two Shields: it breaks both and one GM finishes; two GMs can't.
    for (big, lethal) in [(SUPPRESSOR, true), (GM, false)] {
        let mut sc = scenario(0);
        add(&mut sc, 0, big);
        add(&mut sc, 0, GM);
        shields(&mut sc, 0, 3);
        shields(&mut sc, 1, 2);
        let st = start(sc);
        let b = Board::of(&st, 0, Turn::Now).expect("board");
        assert_eq!(b.hits_needed(), 3);
        assert_eq!(b.lethal(), lethal, "{big} and a GM against two Shields");
    }
    // A Base of 3 HP takes two 2-AP hits; the third attack finishes.
    for (n, lethal) in [(2, false), (3, true)] {
        let mut sc = scenario(0);
        for _ in 0..n {
            add(&mut sc, 0, GM);
        }
        shields(&mut sc, 0, 3);
        sc.base(1, None, 0).expect("a Base");
        let st = start(sc);
        let b = Board::of(&st, 0, Turn::Now).expect("board");
        assert!(b.base.is_some());
        assert_eq!(b.lethal(), lethal, "{n} attackers against a Base");
    }
}

/// Player 0's Gaia Gundam links with Stellar Loussier under Girty Lue, whose trigger makes the
/// enemy Demi Trainer unable to block during this turn (CANT_BLOCK). Player 1 has `shields`
/// Shields and no Base. Returns the state at player 0's MAIN decision, Gaia and the Blocker.
fn cant_block(shields_n: usize) -> (GameState, Uid, Uid) {
    let mut sc = scenario(0);
    sc.resources(0, 4, 0, 0).expect("resources");
    sc.base(0, Some(GIRTY_LUE), 0).expect("Girty Lue");
    let gaia = add(&mut sc, 0, GAIA);
    let stellar = add_with(&mut sc, 0, STELLAR, Zone::Hand, AddOptions::default());
    let demi = add(&mut sc, 1, DEMI);
    shields(&mut sc, 0, 3);
    shields(&mut sc, 1, shields_n);
    let mut st = start(sc);
    testkit::play(&mut st, stellar, Some(gaia), 0).expect("pair Stellar with Gaia");
    testkit::select_if_asked(&mut st, &[demi], None).expect("Girty Lue's target");
    testkit::yes_if_asked(&mut st).expect("no question");
    assert_eq!(
        st.pending().map(|d| (d.hdr.player, d.hdr.kind)),
        Some((0, DecisionKind::Main))
    );
    (st, gaia, demi)
}

#[test]
fn engine_blockers_exclude_cant_block_where_the_reference_counts_it() {
    let (st, gaia, demi) = cant_block(0);
    let dv = gcg_engine::view::derived(&st).expect("view");
    assert!(
        gcg_engine::view::rules_of(&dv, demi, gcg_effects::RuleKind::CantBlock)
            .next()
            .is_some(),
        "Girty Lue's trigger stops the Blocker"
    );
    // The reference's count (`values.ready_blockers`) takes every active <Blocker> (pinned).
    assert_eq!(values::ready_blockers(&st, &dv, 1, -1), [demi]);
    // The engine's eligibility, which the strengthened helper calls, excludes it.
    assert!(
        effective_blockers(&st, gaia, 1)
            .expect("blockers")
            .is_empty()
    );
    let b = Board::of(&st, 0, Turn::Now).expect("board");
    assert!(b.blockers.is_empty() && b.lethal());
    // The reference's can_kill subtracts the Blocker (0 for the turn player, pinned); the
    // strengthened count rule and the exact static step see the open lethal.
    let r = role_features(&st, [0.0, 0.0]).expect("features");
    assert_eq!(r[13], 0.0, "the reference's can_kill_turn_player");
    assert_eq!(can_kill(&st, 0, false).expect("can_kill"), 1.0);
    assert_eq!(can_kill(&st, 0, true).expect("can_kill"), 1.0);
    // The solver proves the lethal: the attack is not blocked.
    let out = lethal_line(&st, 0, ATTACKS_ONLY, &knobs(), &attacks_of(&st)).expect("solve");
    assert_eq!(out.verdict, Verdict::Lethal(attack(gaia)));
}

#[test]
fn the_strengthened_race_skips_a_blocker_that_stays_rested_this_turn() {
    // Player 0 attacks with two GMs; player 1 has one Shield and a rested Demi Trainer.
    let st = board(2, 1, 0, 1);
    let r = role_features(&st, [0.0, 0.0]).expect("features");
    // The reference race counts the rested Blocker: 2 hits needed, 1 connects per turn, 2
    // turns (pinned).
    assert_eq!(r[15], 0.5, "the reference's inv_race_turn_player");
    // The strengthened race: the Blocker is still rested this turn, both attacks connect.
    assert_eq!(race(&st, 0, 6).expect("race"), 1);
    assert_eq!(race_feature(&st, 0).expect("race"), 1.0);
    // With the Blocker active it stops one attack this turn as well: 2 turns.
    let st = board(2, 1, 1, 0);
    assert_eq!(race(&st, 0, 6).expect("race"), 2);
}

#[test]
fn the_solver_finds_lethal_and_proves_its_absence() {
    let st = board(1, 0, 0, 0);
    let gm = st.zone(0, Zone::Battle)[0];
    let out = lethal_line(&st, 0, ATTACKS_ONLY, &knobs(), &attacks_of(&st)).expect("solve");
    assert_eq!(out.verdict, Verdict::Lethal(attack(gm)));
    assert!(out.applies > 0);
    let st = board(3, 2, 1, 0);
    let out = lethal_line(&st, 0, ATTACKS_ONLY, &knobs(), &attacks_of(&st)).expect("solve");
    assert_eq!(out.verdict, Verdict::NoLethal);
    // Four attackers through one Blocker and two Shields: lethal whatever is blocked.
    let st = board(4, 2, 1, 0);
    let out = lethal_line(&st, 0, ATTACKS_ONLY, &knobs(), &attacks_of(&st)).expect("solve");
    assert!(matches!(out.verdict, Verdict::Lethal(a) if a.kind == ActionKind::Attack));
    // A bound too small for the proof is unknown, never a claim.
    let tiny = Limits {
        max_applies: 2,
        ..ATTACKS_ONLY
    };
    let out = lethal_line(&st, 0, tiny, &knobs(), &attacks_of(&st)).expect("solve");
    assert_eq!(out.verdict, Verdict::Unknown);
}

/// `test_lethal_rests_the_blocker_before_attacking` (`tests/ai/test_puzzles.py`): GM against
/// an exposed player with Demi Trainer, Siege Ploy in hand.
fn rest_the_blocker() -> GameState {
    let mut sc = scenario(0);
    sc.resources(0, 3, 0, 0).expect("resources");
    add(&mut sc, 0, GM);
    sc.hand(0, &[SIEGE_PLOY]).expect("hand");
    add(&mut sc, 1, DEMI);
    shields(&mut sc, 0, 3);
    start(sc)
}

#[test]
fn the_solver_plays_a_removal_before_the_attack() {
    let st = rest_the_blocker();
    let options = st.pending().expect("main").options.to_vec();
    let out = lethal_line(&st, 0, ATTACKS_ONLY, &knobs(), &attacks_of(&st)).expect("solve");
    assert_eq!(out.verdict, Verdict::NoLethal, "attacks alone are blocked");
    let first = super::solver::first_moves(&st.pending().expect("main"), &options, true);
    let out = lethal_line(&st, 0, WITH_SETUPS, &knobs(), &first).expect("solve");
    match out.verdict {
        Verdict::Lethal(a) => assert_eq!(a.kind, ActionKind::PlayCommand),
        v => panic!("expected the Siege Ploy line, got {v:?}"),
    }
}

#[test]
fn the_defenders_action_step_trick_holds_and_is_explored() {
    // The attacker's GM is damaged; the exposed defender holds Battle of Aces (3 damage to a
    // damaged enemy Unit, 【Action】, Lv.3 cost 2) with three Resources: it destroys the
    // attacker in the action step.
    let build = |trick: bool| {
        let mut sc = scenario(0);
        add_with(
            &mut sc,
            0,
            GM,
            Zone::Battle,
            AddOptions {
                damage: 1,
                ..AddOptions::default()
            },
        );
        shields(&mut sc, 0, 3);
        sc.resources(1, 3, 0, 0).expect("resources");
        if trick {
            sc.hand(1, &[BATTLE_OF_ACES]).expect("hand");
        }
        start(sc)
    };
    let st = build(false);
    let out = lethal_line(&st, 0, ATTACKS_ONLY, &knobs(), &attacks_of(&st)).expect("solve");
    assert!(matches!(out.verdict, Verdict::Lethal(_)));
    let st = build(true);
    let out = lethal_line(&st, 0, ATTACKS_ONLY, &knobs(), &attacks_of(&st)).expect("solve");
    assert_eq!(
        out.verdict,
        Verdict::NoLethal,
        "the trick saves the defender"
    );
}

#[test]
fn forced_win_reads_the_defenders_blocks() {
    // Player 1 attacks the exposed player 0 with `attackers` GMs against one Demi Trainer.
    let build = |attackers: usize, blocker_rested: bool| {
        let mut sc = scenario(1);
        for _ in 0..attackers {
            add(&mut sc, 1, GM);
        }
        shields(&mut sc, 1, 3);
        if blocker_rested {
            rested(&mut sc, 0, DEMI);
        } else {
            add(&mut sc, 0, DEMI);
        }
        start(sc)
    };
    for (attackers, blocker_rested, want) in [
        (1, false, Res::Fail),
        (1, true, Res::Win),
        (2, false, Res::Win),
    ] {
        let st = build(attackers, blocker_rested);
        let (r, applies) = forced_win(&st, 1, ATTACKS_ONLY, &knobs()).expect("solve");
        assert_eq!(
            r, want,
            "{attackers} attackers, Blocker rested: {blocker_rested}"
        );
        assert!(applies > 0);
    }
}

/// The reference `standard` configuration as a strengthened configuration with every mechanism
/// off (the null configuration of `tests/common/mod.rs`) and the shipped weights (the slow
/// audit's).
#[cfg(feature = "slow")]
pub(super) fn null_standard() -> StrengthenedConfig {
    let r = &gcg_ai_reference::STANDARD;
    StrengthenedConfig {
        name: r.name.clone(),
        max_iterations: r.max_iterations,
        min_iterations: r.min_iterations,
        iterations_per_option: r.iterations_per_option,
        exploration: r.exploration,
        prior_weight: r.prior_weight,
        prior_temperature: r.prior_temperature,
        horizon_turns: r.horizon_turns,
        rollout_temperature: r.rollout_temperature,
        value_scale: r.value_scale,
        max_rollout_decisions: r.max_rollout_decisions,
        first_player_games: r.first_player_games,
        mulligan_samples: r.mulligan_samples,
        policy_burst_yes_score: r.policy_burst_yes_score,
        noop_branch_choices: r.noop_branch_choices,
        noop_resource_aware: r.noop_resource_aware,
        strict_attack_prune: r.strict_attack_prune,
        prune_noop_rest_targets: r.prune_noop_rest_targets,
        merge_equivalent_moves: r.merge_equivalent_moves,
        observation_keys: r.observation_keys,
        target_extension: r.target_extension,
        target_extension_z: r.target_extension_z,
        policy_rule_polarity_table: r.policy_rule_polarity_table,
        policy_zero_ap_face: r.policy_zero_ap_face,
        weights: ROLE_WEIGHTS.clone(),
        mechanisms: MechanismFlags::NULL,
    }
}

fn config(flags: &[&str]) -> StrengthenedConfig {
    let mut cfg = crate::preset_config(Preset::Fast).clone();
    cfg.mechanisms = MechanismFlags::NULL;
    for f in flags {
        assert_eq!(cfg.mechanisms.set_text(f, "true"), Some(Ok(())));
    }
    cfg
}

fn leaf_of(cfg: &StrengthenedConfig, st: &GameState, player: u8, root_turn: i32) -> f64 {
    let state = DecisionState::new();
    state.cap.set(u64::MAX);
    let mut cx = LeafCx {
        s: st,
        player,
        root_turn,
        horizon: st.turn(),
        decisions: 0,
        capped: false,
        cfg,
        weights: &ROLE_WEIGHTS,
        density: [0.0, 0.0],
        math: Math::Platform,
    };
    leaf_value(&state, &cfg.mechanisms, &mut cx).expect("leaf")
}

#[test]
fn a_leaf_with_a_proved_lethal_takes_the_time_shaped_terminal_value() {
    let st = board(4, 2, 1, 0);
    let root_turn = st.turn() - 2;
    let reference = leaf_of(&config(&[]), &st, 0, root_turn);
    assert!(reference > LEAF_FLOOR && reference < 1.0 - LEAF_FLOOR);
    let win = 1.0 - WIN_DECAY * 2.0;
    assert_eq!(leaf_of(&config(&["threat_leaf"]), &st, 0, root_turn), win);
    assert_eq!(
        leaf_of(&config(&["threat_leaf"]), &st, 1, root_turn),
        WIN_DECAY * 2.0,
        "the opponent's lethal is the loss value"
    );
    // No lethal: the estimate stays the reference's.
    let st = board(3, 2, 1, 0);
    assert_eq!(
        leaf_of(&config(&["threat_leaf"]), &st, 0, root_turn).to_bits(),
        leaf_of(&config(&[]), &st, 0, root_turn).to_bits()
    );
}

#[test]
fn the_feature_leaf_changes_only_the_can_kill_and_race_features() {
    let (st, _, _) = cant_block(0);
    let reference = leaf_of(&config(&[]), &st, 0, st.turn());
    let can_kill = leaf_of(&config(&["solver_can_kill"]), &st, 0, st.turn());
    let blockers = leaf_of(&config(&["effective_blockers"]), &st, 0, st.turn());
    assert!(
        can_kill > reference,
        "the turn player's open lethal raises the estimate"
    );
    assert!(blockers > reference);
    // Without a P6-B leaf flag the leaf is the reference's bit for bit.
    let cfg = config(&["tactical_solver", "lethal_root", "critical_extension"]);
    assert_eq!(
        leaf_of(&cfg, &st, 0, st.turn()).to_bits(),
        reference.to_bits()
    );
}

#[test]
fn critical_decisions_get_two_extra_budgets_and_root_trees_share_the_solver_cap() {
    let cfg = config(&["critical_extension"]);
    let mut rng = gcg_core::rng::SplitMix64::new(1);
    let b = Budget {
        iterations: 160,
        cap: 160,
    };
    for (st, critical) in [(board(4, 2, 1, 0), true), (board(1, 5, 0, 0), false)] {
        let dec = st.pending().expect("main");
        let cx = RootCx {
            st: &st,
            dec: &dec,
            player: 0,
            cfg: &cfg,
            knobs: cfg.knobs(),
            rng: &mut rng,
        };
        let state = DecisionState::new();
        let out = budget(&state, &cfg.mechanisms, &cx, b);
        let want = if critical { 480 } else { 160 };
        assert_eq!(
            out,
            Budget {
                iterations: 160,
                cap: want
            }
        );
        assert_eq!(
            state.cap.get(),
            SOLVER_APPLIES_PER_ITERATION * u64::from(want)
        );
        let tree = state.for_tree(4);
        assert_eq!(tree.cap.get(), state.cap.get().div_ceil(4));
        assert_eq!((tree.aux_applies(), tree.root_applies()), (0, 0));
    }
}

#[test]
fn a_spent_solver_budget_leaves_rollouts_and_leaves_to_the_reference() {
    let state = DecisionState::new();
    assert!(!state.has_budget(), "no cap before the budget is set");
    state.cap.set(10);
    assert!(state.has_budget());
    state.spend(10);
    assert!(!state.has_budget());
    let st = board(4, 2, 1, 0);
    let cfg = config(&["threat_leaf"]);
    let mut cx = LeafCx {
        s: &st,
        player: 0,
        root_turn: st.turn(),
        horizon: st.turn(),
        decisions: 0,
        capped: false,
        cfg: &cfg,
        weights: &ROLE_WEIGHTS,
        density: [0.0, 0.0],
        math: Math::Platform,
    };
    let spent = leaf_value(&state, &cfg.mechanisms, &mut cx).expect("leaf");
    let reference = crate::mech::reference::leaf_value(&mut cx).expect("leaf");
    assert_eq!(spent.to_bits(), reference.to_bits());
}
