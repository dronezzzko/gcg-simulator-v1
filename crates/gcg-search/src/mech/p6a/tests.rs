//! P6-A's unit tests: the leaf constants and their check (W-02), the adaptive horizon (W-01)
//! and the continuation of capped rollouts (W-15).

use gcg_ai::{AgentError, burst_density};
use gcg_engine::{
    Action, ActionKind, AddOptions, DecisionKind, EngineError, GameState, Scenario,
    ScenarioOptions, Step, Zone, apply,
};

use super::capped::{CONTINUATION_DECISIONS, Continued, continue_line, settled};
use super::horizon::{INV_RACE, MAX_EXTENSION, Unsettled, decide, unsettled};
use super::leaf::{FLOOR, LeafMapping, max_turns};
use super::{Counts, DecisionState, check, leaf_value};
use crate::StrengthenedConfig;
use crate::mech::p6o::math::Math;
use crate::mech::reference::{LEAF_FLOOR, WIN_DECAY};
use crate::mech::tests::{config, run_with, searched_states};
use crate::mech::{Composed, HorizonDecision, LeafCx, Mechanisms, math_of, reference};

const DRAWN: &str = "EB01-007";

fn added(sc: &mut Scenario, p: u8, n: &str, z: Zone, rested: bool) {
    sc.add(
        p,
        n,
        z,
        AddOptions {
            rested,
            ..AddOptions::default()
        },
    )
    .expect("a card");
}

/// The W-01 scenario position (`tests/scenarios/p6a_w01_two_turn_plan.rs`): player 0 to move
/// with one Shield against Gundam Ez8 High Mobility Custom (AP 4), so player 1's race estimate
/// against player 0 is two turns.
fn two_turn_plan() -> GameState {
    let reg = gcg_effects::Registry::embedded();
    let mut sc = Scenario::new(&reg, ScenarioOptions::default());
    added(&mut sc, 0, "GD01-064", Zone::Battle, false);
    added(&mut sc, 0, "GD03-047", Zone::Hand, false);
    added(&mut sc, 0, "GD03-091", Zone::Hand, false);
    sc.resources(0, 3, 0, 0).expect("resources");
    sc.resource_deck(0, 4).expect("a resource deck");
    sc.shields(0, &[DRAWN]).expect("a Shield");
    sc.deck(0, &[DRAWN; 12]);
    added(&mut sc, 1, "GD04-061", Zone::Battle, false);
    added(&mut sc, 1, "EB01-032", Zone::Battle, true);
    sc.resources(1, 2, 0, 0).expect("resources");
    sc.hand(1, &[DRAWN; 3]).expect("a hand");
    sc.deck(1, &[DRAWN; 12]);
    sc.start(Step::Main).expect("the scenario starts");
    sc.into_state()
}

/// Player 0's main phase with a vanilla Unit each and six Shields each: both race estimates
/// are three turns or more.
fn quiet() -> GameState {
    let reg = gcg_effects::Registry::embedded();
    let mut sc = Scenario::new(&reg, ScenarioOptions::default());
    added(&mut sc, 0, "GD01-064", Zone::Battle, false);
    added(&mut sc, 1, "GD01-060", Zone::Battle, false);
    sc.resources(0, 3, 0, 0).expect("resources");
    sc.resources(1, 3, 0, 0).expect("resources");
    sc.shields(0, &[DRAWN; 6]).expect("Shields");
    sc.shields(1, &[DRAWN; 6]).expect("Shields");
    sc.deck(0, &[DRAWN; 12]);
    sc.deck(1, &[DRAWN; 12]);
    sc.start(Step::Main).expect("the scenario starts");
    sc.into_state()
}

/// The W-01 position after player 0's DINN attacks the player: player 1's block decision is
/// pending in the battle.
fn attacking() -> GameState {
    let mut st = two_turn_plan();
    play(&mut st, |a| a.kind == ActionKind::Attack && a.b < 0);
    assert_eq!(st.pending().map(|d| d.hdr.kind), Some(DecisionKind::Block));
    st
}

/// The first legal action of `st` that `want` accepts, applied.
fn play(st: &mut GameState, want: impl Fn(&Action) -> bool) {
    let a = *st
        .pending()
        .expect("a decision")
        .options
        .iter()
        .find(|a| want(a))
        .expect("a matching action");
    apply(st, a, true).expect("the action applies");
}

/// The leaf context of `s` for player 0 under `cfg`, as the core builds it.
fn leaf_cx<'a>(
    s: &'a GameState,
    cfg: &'a StrengthenedConfig,
    root_turn: i32,
    horizon: i32,
    capped: bool,
) -> LeafCx<'a> {
    LeafCx {
        s,
        player: 0,
        root_turn,
        horizon,
        decisions: 0,
        capped,
        cfg,
        weights: &cfg.weights,
        density: burst_density(s, 0),
        math: math_of(cfg),
    }
}

/// W-02's verification: over horizons 1..8 and extensions 0..2 the worst terminal win exceeds
/// the best possible estimate and the best terminal loss is below the worst one, at every line
/// length; `decay = floor / (max_horizon + max_extension + 1)`.
#[test]
fn generalized_leaf_constants_order_terminal_results_at_every_horizon() {
    for h in 1..=8 {
        for e in 0..=2 {
            let m = LeafMapping::generalized(h, e);
            assert_eq!(m.max_turns, h + e);
            assert_eq!(m.floor.to_bits(), FLOOR.to_bits());
            assert_eq!(m.decay.to_bits(), (FLOOR / f64::from(h + e + 1)).to_bits());
            assert_eq!(m.check(), Ok(()), "h {h} e {e}");
            // Search lines end within `max_turns`; a longer finished line (a pre-game
            // simulation) is shaded as one of `max_turns`.
            for turns in 0..=m.max_turns + 40 {
                assert!(m.win(turns) > m.estimate(1.0), "h {h} e {e} turns {turns}");
                assert!(m.loss(turns) < m.estimate(0.0), "h {h} e {e} turns {turns}");
                if turns > 0 && turns <= m.max_turns {
                    assert!(m.win(turns) < m.win(turns - 1) && m.loss(turns) > m.loss(turns - 1));
                }
            }
            assert_eq!(
                m.win(m.max_turns + 9).to_bits(),
                m.win(m.max_turns).to_bits()
            );
            assert!(m.estimate(0.0) < m.estimate(0.5) && m.estimate(0.5) < m.estimate(1.0));
        }
    }
    // A horizon below one turn still covers the root move's turn boundary.
    assert_eq!(max_turns(0, 0), 1);
    assert_eq!(max_turns(-3, 1), 2);
    assert_eq!(LeafMapping::generalized(0, 0).check(), Ok(()));
}

/// The reference keeps `LEAF_FLOOR`, `WIN_DECAY` and Python's check: four turns pass, five do
/// not; `horizon_turns = 5` is still rejected with the Python message.
#[test]
fn the_reference_constants_keep_their_check() {
    for turns in 1..=4 {
        assert_eq!(LeafMapping::reference(turns).check(), Ok(()), "{turns}");
    }
    assert!(LeafMapping::reference(5).check().is_err());
    let m = LeafMapping::reference(2);
    assert_eq!(
        (m.floor.to_bits(), m.decay.to_bits()),
        (LEAF_FLOOR.to_bits(), WIN_DECAY.to_bits())
    );
    let mut r = gcg_ai_reference::STANDARD.clone();
    r.horizon_turns = 5;
    assert_eq!(
        r.check().map_err(|e| e.to_string()),
        Err(
            "horizon_turns=5 is too long: a win at the horizon would not outrank every \
             evaluated position (WIN_DECAY * horizon_turns < LEAF_FLOOR)"
                .to_owned()
        )
    );
}

/// The configuration check: an extension past four turns needs the generalised constants.
#[test]
fn the_leaf_check_covers_the_adaptive_extension() {
    let mut cfg = config(&["adaptive_horizon"]);
    for (h, ok) in [(2, true), (3, true), (4, false)] {
        cfg.horizon_turns = h;
        assert_eq!(check(&cfg).is_ok(), ok, "horizon {h}");
    }
    assert_eq!(
        check(&cfg),
        Err(
            "a line of 5 turns is too long for the reference leaf constants: a win at the \
             extended horizon would not outrank every evaluated position \
             (WIN_DECAY * turns < LEAF_FLOOR); enable generalized_leaf"
                .to_owned()
        )
    );
    cfg.mechanisms.generalized_leaf = true;
    assert_eq!(check(&cfg), Ok(()));
    assert_eq!(LeafMapping::of(&cfg).max_turns, 4 + MAX_EXTENSION);
    cfg.horizon_turns = 8;
    assert_eq!(check(&cfg), Ok(()));
}

/// A search whose configuration fails the check fails the decision before any iteration, as a
/// `ValueError`; with `generalized_leaf` the same horizon searches.
#[test]
fn a_search_refuses_a_horizon_its_leaf_constants_cannot_order() {
    let (st, player, seed) = searched_states(1).remove(0);
    let mut cfg = config(&["adaptive_horizon", "engine_error_leaf"]);
    cfg.horizon_turns = 4;
    match run_with(&Composed::new(&cfg), &st, player, &cfg, seed) {
        Err(AgentError::Engine(EngineError::Value(m))) => {
            assert!(m.contains("enable generalized_leaf"), "{m}");
        }
        other => panic!("expected the leaf check's error, got {other:?}"),
    }
    cfg.mechanisms.generalized_leaf = true;
    assert!(run_with(&Composed::new(&cfg), &st, player, &cfg, seed).is_ok());
}

/// Without `generalized_leaf` the P6-A leaf value is the reference's, bit for bit; with it a
/// finished line is scored with the generalised decay, and so is the engine-error loss.
#[test]
fn leaf_values_use_the_configured_constants() {
    let plain = config(&["capped_rollout_continuation"]);
    let state = DecisionState::new();
    for (s, _, _) in searched_states(24) {
        let root = s.turn() - 1;
        let horizon = s.turn() + 1;
        let a =
            leaf_value(&state, &mut leaf_cx(&s, &plain, root, horizon, false)).expect("a value");
        let b =
            reference::leaf_value(&mut leaf_cx(&s, &plain, root, horizon, false)).expect("a value");
        assert_eq!(a.to_bits(), b.to_bits());
    }
    // Player 0 attacks the player with no Shields and player 1 does not block: a win in turn 3.
    let mut won = two_turn_plan();
    play(&mut won, |a| a.kind == ActionKind::Attack && a.b < 0);
    play(&mut won, |a| a.kind == ActionKind::NoBlock);
    while won.winner().is_none() {
        play(&mut won, |a| a.kind == ActionKind::Pass);
    }
    assert_eq!((won.winner(), won.turn()), (Some(0), 3));
    let mut cfg = config(&["generalized_leaf", "engine_error_leaf"]);
    cfg.horizon_turns = 3;
    let m = LeafMapping::of(&cfg);
    assert_eq!(m.decay.to_bits(), (FLOOR / 4.0).to_bits());
    let mut cx = leaf_cx(&won, &cfg, 1, 4, false);
    assert_eq!(
        leaf_value(&state, &mut cx).expect("a value").to_bits(),
        (1.0 - m.decay * 2.0).to_bits()
    );
    cx.player = 1;
    assert_eq!(
        leaf_value(&state, &mut cx).expect("a value").to_bits(),
        (m.decay * 2.0).to_bits()
    );
    let composed = Composed::new(&cfg);
    let err = EngineError::Value("a test error".into());
    assert_eq!(
        composed.error_leaf(&cx, &err).map(f64::to_bits),
        Some((m.decay * 2.0).to_bits())
    );
    let mut reference_cfg = cfg.clone();
    reference_cfg.mechanisms.generalized_leaf = false;
    let composed = Composed::new(&reference_cfg);
    let cx = leaf_cx(&won, &reference_cfg, 1, 4, false);
    assert_eq!(
        composed.error_leaf(&cx, &err).map(f64::to_bits),
        Some((WIN_DECAY * 2.0).to_bits())
    );
}

/// W-01's unsettled test: a race estimate of two turns, a battle in progress, or a quiet board.
#[test]
fn the_unsettled_test_reads_the_race_and_pending_resolutions() {
    assert_eq!(gcg_ai::ROLE_FEATURES[INV_RACE], "inv_race_turn_player");
    assert_eq!(gcg_ai::ROLE_FEATURES[INV_RACE + 1], "inv_race_other");
    let race = two_turn_plan();
    assert_eq!(
        unsettled(&race, burst_density(&race, 0)),
        Ok(Unsettled::Race)
    );
    let calm = quiet();
    assert_eq!(
        unsettled(&calm, burst_density(&calm, 0)),
        Ok(Unsettled::Settled)
    );
    let battle = attacking();
    assert!(!battle.battles().is_empty());
    assert_eq!(
        unsettled(&battle, burst_density(&battle, 0)),
        Ok(Unsettled::Resolving)
    );
}

/// `adaptive_horizon` extends a line once, at its horizon, when the leaf is unsettled; a
/// capped line, a line before its horizon, an extended line and a settled leaf are leaves.
#[test]
fn adaptive_horizon_extends_an_unsettled_leaf_once() {
    let cfg = config(&["adaptive_horizon"]);
    let composed = Composed::new(&cfg);
    let race = two_turn_plan();
    let t = race.turn();
    let extend = HorizonDecision::Extend {
        turns: 1,
        decisions: cfg.max_rollout_decisions,
    };
    assert_eq!(decide(&leaf_cx(&race, &cfg, t - 2, t, false)), extend);
    assert_eq!(
        composed.horizon(&leaf_cx(&race, &cfg, t - 2, t, false)),
        extend
    );
    assert_eq!(
        decide(&leaf_cx(&race, &cfg, t - 2, t, true)),
        HorizonDecision::Leaf
    );
    assert_eq!(
        decide(&leaf_cx(&race, &cfg, t - 1, t + 1, false)),
        HorizonDecision::Leaf
    );
    assert_eq!(
        decide(&leaf_cx(&race, &cfg, t - 3, t, false)),
        HorizonDecision::Leaf
    );
    let calm = quiet();
    let t = calm.turn();
    assert_eq!(
        decide(&leaf_cx(&calm, &cfg, t - 2, t, false)),
        HorizonDecision::Leaf
    );
    assert_eq!(composed.p6a.counts().extended, 1);
}

/// On the searched decisions of the D2 vectors, `adaptive_horizon` extends some lines, plays
/// longer rollouts, and is deterministic.
#[test]
fn adaptive_horizon_plays_longer_lines_deterministically() {
    let cfg = config(&["adaptive_horizon"]);
    let null = config(&[]);
    let (mut extended, mut longer, mut searched, mut iterations) = (0, 0, 0, 0);
    for (st, player, seed) in searched_states(10) {
        let mech = Composed::new(&cfg);
        let (a, _) = run_with(&mech, &st, player, &cfg, seed).expect("a search");
        let (b, _) = run_with(&Composed::new(&cfg), &st, player, &cfg, seed).expect("a search");
        assert_eq!(
            (a.action, &a.stats, a.alternatives.len()),
            (b.action, &b.stats, b.alternatives.len())
        );
        let (r, _) = run_with(&Composed::new(&null), &st, player, &null, seed).expect("a search");
        if a.stats.iterations == 0 {
            continue;
        }
        searched += 1;
        iterations += u64::from(a.stats.iterations);
        extended += mech.p6a.counts().extended;
        let per = |x: &gcg_ai::SearchResult| {
            x.stats.effort.applies_rollout as f64 / f64::from(x.stats.iterations)
        };
        longer += u32::from(per(&a) > per(&r));
    }
    eprintln!(
        "adaptive_horizon: {extended} of {iterations} lines extended in {searched} searches, \
         {longer} with longer rollouts per iteration"
    );
    assert!(searched > 0);
    assert!(extended > 0, "no line was extended");
    assert!(longer > 0, "no search rolled out further");
}

/// W-15's unit test: with a tiny decision cap, capped rollouts are recorded (the trace's
/// `rollouts_capped`), every one is flagged by the mechanism, and each is continued with the
/// deterministic policy to a main-phase decision on a copy (auxiliary applies).
#[test]
fn capped_rollouts_continue_to_a_main_phase_decision() {
    let mut cfg = config(&["capped_rollout_continuation"]);
    cfg.max_rollout_decisions = 2;
    let mut capped = 0;
    let mut flagged = Counts::default();
    for (st, player, seed) in searched_states(6) {
        let mech = Composed::new(&cfg);
        let (r, _) = run_with(&mech, &st, player, &cfg, seed).expect("a search");
        capped += r.stats.effort.rollouts_capped;
        let c = mech.p6a.counts();
        assert_eq!(c.capped, r.stats.effort.rollouts_capped);
        assert_eq!(mech.aux_applies(), c.aux_applies);
        flagged.capped += c.capped;
        flagged.aux_applies += c.aux_applies;
        flagged.unsettled += c.unsettled;
    }
    assert!(capped > 0, "the tiny cap was never hit");
    assert!(flagged.aux_applies > 0);
    assert!(flagged.unsettled < flagged.capped);
    // The continuation itself: a rollout of two policy moves from each searched state, then
    // the deterministic policy to a settled position.
    let knobs = cfg.knobs();
    let mut continued = 0;
    for (st, _, seed) in searched_states(24) {
        let mut s = st.clone();
        let mut rng = gcg_core::rng::SplitMix64::new(seed);
        for _ in 0..2 {
            if s.pending().is_none() || s.winner().is_some() {
                break;
            }
            let a = Math::Platform
                .choose(&s, &mut rng, cfg.rollout_temperature, &knobs)
                .expect("a move");
            gcg_ai::apply_unchecked(&mut s, a).expect("the move applies");
        }
        let horizon = st.turn() + 2;
        match continue_line(&mut s, horizon, &knobs).expect("a continuation") {
            Continued::Settled { applies } => {
                assert!(settled(&s) && applies <= CONTINUATION_DECISIONS);
                assert!(
                    s.winner().is_some()
                        || s.pending()
                            .is_some_and(|d| d.hdr.kind == DecisionKind::Main)
                );
                continued += 1;
            }
            Continued::Unsettled { applies } => assert_eq!(applies, CONTINUATION_DECISIONS),
        }
    }
    assert!(continued > 0);
}

/// A capped line still unsettled after its continuation scores 0.5 and is counted.
#[test]
fn an_unsettled_capped_line_scores_one_half() {
    let battle = attacking();
    assert!(!settled(&battle));
    let cfg = config(&["capped_rollout_continuation"]);
    let state = DecisionState::new();
    // The line's horizon is already behind it, so the continuation cannot settle it.
    let t = battle.turn();
    let v = leaf_value(&state, &mut leaf_cx(&battle, &cfg, t - 2, t - 1, true)).expect("a value");
    assert_eq!(v.to_bits(), 0.5f64.to_bits());
    let c = state.counts();
    assert_eq!((c.capped, c.unsettled, c.aux_applies), (1, 1, 0));
    // Within its horizon the same line finishes the battle and is scored there.
    let state = DecisionState::new();
    let v = leaf_value(&state, &mut leaf_cx(&battle, &cfg, t - 1, t + 1, true)).expect("a value");
    let c = state.counts();
    assert_eq!((c.capped, c.unsettled), (1, 0));
    assert!(c.aux_applies > 0 && v > 0.0 && v < 1.0);
}
