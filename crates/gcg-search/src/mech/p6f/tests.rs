//! P6-F's unit tests: the fitted policy's constants and provenance (W-23; the self-play fit
//! itself is `fit_selfplay.rs`, feature `slow`), the Burst tripwire and its negative control
//! (W-163), the exhaustive polarity (W-30), the exact fights (W-31), the ORDER_TRIGGER
//! heuristic (W-29), the end-phase look-ahead (W-28), and the rollout distributions of the
//! look-ahead scorers (W-24, W-27, W-163).

use std::path::PathBuf;
use std::sync::Arc;

use gcg_ai::ROLE_WEIGHTS;
use gcg_ai::values::Fight;
use gcg_ai_reference::ReferenceConfig;
use gcg_core::rng::SplitMix64;
use gcg_effects::{Kw, Node, NodeId, P, ProgramId, Registry, RuleKind, Side};
use gcg_engine::{
    ActionKind, AddOptions, CtxKey, DecisionKind, GameState, PLAYER_TARGET, Scenario,
    ScenarioOptions, Step, Uid, Zone, apply, testkit,
};
use serde_json::{Value, json};

use super::calibrated::Model;
use super::polarity::Polarity;
use super::scorer::Scoring;
use super::{DecisionState, fitted, order, polarity};
use crate::{MechanismFlags, StrengthenedConfig};

pub(super) fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The reference configuration `r` as a strengthened configuration with every mechanism off
/// (the null configuration of `tests/common/mod.rs`) and the flags `on`.
pub(super) fn null_with(r: &ReferenceConfig, on: &[&str]) -> StrengthenedConfig {
    let mut cfg = StrengthenedConfig {
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
    };
    for f in on {
        assert_eq!(cfg.mechanisms.set_text(f, "true"), Some(Ok(())), "{f}");
    }
    cfg
}

/// A scoring of `cfg` with the decision state `state`.
fn scoring<'a>(cfg: &'a StrengthenedConfig, state: &'a DecisionState, model: Model) -> Scoring<'a> {
    Scoring {
        flags: &cfg.mechanisms,
        state,
        cfg,
        knobs: cfg.knobs(),
        model,
    }
}

/// How often `n` rollout draws at temperature `t` pick each option of `st`'s pending decision
/// under `cfg`'s scores.
fn rollout_counts(cfg: &StrengthenedConfig, st: &GameState, t: f64, n: u32) -> Vec<u32> {
    let dec = st.pending().expect("a decision");
    let state = DecisionState::new();
    let scores = scoring(cfg, &state, Model::None)
        .scores(st, &dec, dec.options)
        .expect("scores");
    let mut rng = SplitMix64::new(11);
    let mut counts = vec![0; dec.options.len()];
    for _ in 0..n {
        counts[super::pick(crate::mech::p6o::math::Math::Portable, &scores, &mut rng, t)] += 1;
    }
    counts
}

// ---------------------------------------------------------------------------------------------
// W-23, W-25: the fit

/// The fit's master seeds: tuning role for the fit, dev role for the out-of-sample check
/// (PLAN A14; recorded in `docs/dev/eval/P6-F/seed-ledger.json`).
pub(super) const FIT_MASTER: u64 = 10_600_001;
pub(super) const CHECK_MASTER: u64 = 20_600_001;
pub(super) fn report_path() -> PathBuf {
    repo().join("docs/dev/eval/P6-F/policy-fit.json")
}

fn bits_of(v: &Value) -> Vec<f64> {
    v.as_array()
        .expect("bits")
        .iter()
        .map(|b| f64::from_bits(u64::from_str_radix(b.as_str().expect("hex"), 16).expect("hex")))
        .collect()
}

/// The constants the policy uses are the recorded fit's, bit for bit.
#[test]
fn fitted_constants_are_the_recorded_fit() {
    let report: Value =
        serde_json::from_str(&std::fs::read_to_string(report_path()).expect("the fit"))
            .expect("JSON");
    let own = bits_of(&report["own"]["offset_bits"]);
    assert_eq!(
        own.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
        fitted::OWN_OFFSETS.map(f64::to_bits)
    );
    assert_eq!(report["fit"]["role"], "tuning");
    assert_eq!(report["fit"]["master"], FIT_MASTER);
    assert_eq!(report["check"]["role"], "dev");
    assert!((10_000_000..=19_999_999).contains(&FIT_MASTER));
    assert!((20_000_000..=29_999_999).contains(&CHECK_MASTER));
    assert_eq!(report["own"]["converged"], true);
    // W-23's target: out of sample, the calibrated rollout policy casts Commands and activates
    // abilities within 5 points of the search's own choice frequencies.
    for name in ["main_command", "main_activate"] {
        let row = &report["own"]["shares_check"][name];
        let gap = (row["calibrated"].as_f64().expect("a share")
            - row["search"].as_f64().expect("a share"))
        .abs();
        assert!(gap <= 0.05, "{name}: {row}");
    }
}

// ---------------------------------------------------------------------------------------------
// W-163: the Burst tripwire

/// The packaged Bursts whose review is not clean, as reviewed (W-163's verification). Each is
/// answered by the look-ahead instead of the presets' high score; a change of the list fails
/// the tripwire until the new entries are reviewed and recorded here.
///
/// - GD05-089 Master Asia: "Add this card to your hand. If there are 3 or more (MF) cards in
///   your trash, you may deploy it as an (AP3･HP3) Unit instead." The nested "you may" is an
///   optional replacement; both answers are benefits, so the Burst itself is never harmful.
const REVIEWED_BURSTS: &[(&str, &[&str])] = &[("GD05-089", &["optional"])];

/// The tripwire: the registry's listed Bursts are exactly [`REVIEWED_BURSTS`].
fn burst_tripwire(reg: &Registry) -> Result<(), String> {
    let listed = super::bursts::listed_bursts(reg);
    let want: Vec<(String, Vec<&str>)> = REVIEWED_BURSTS
        .iter()
        .map(|(n, r)| ((*n).to_owned(), r.to_vec()))
        .collect();
    if listed == want {
        Ok(())
    } else {
        Err(format!("listed Bursts changed: {listed:?}"))
    }
}

/// The registry with the synthetic Burst "Discard 2 cards. Draw 1." on EB01-007
/// (`docs/dev/eval/P6-F/synthetic_burst_fragment.json`, from `synthetic_cards.py`).
fn synthetic_burst_registry() -> Registry {
    let doc: Value = serde_json::from_str(
        &std::fs::read_to_string(repo().join("docs/dev/eval/P6-F/synthetic_burst_fragment.json"))
            .expect("the fragment"),
    )
    .expect("JSON");
    assert_eq!(doc["card_numbers"], json!([SYNTHETIC]));
    Registry::embedded()
        .with_fragment(&doc["fragment"])
        .expect("the fragment applies")
}

const SYNTHETIC: &str = "EB01-007";

#[test]
fn burst_tripwire_holds_on_the_packaged_pool() {
    assert_eq!(burst_tripwire(&Registry::embedded()), Ok(()));
}

/// W-163's negative control: the synthetic harmful Burst in a temporary data copy fails the
/// tripwire, as the self-harming step it is.
#[test]
fn burst_tripwire_fails_on_a_synthetic_harmful_burst() {
    let reg = synthetic_burst_registry();
    let err = burst_tripwire(&reg).expect_err("the tripwire fails");
    assert!(err.contains("EB01-007"), "{err}");
    let listed = super::bursts::listed_bursts(&reg);
    assert!(
        listed.contains(&(SYNTHETIC.to_owned(), vec!["self_harm"])),
        "{listed:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// Positions for the rollout distributions

const FILLER: &str = "GD01-060";
/// Draws per distribution.
const DRAWS: u32 = 20_000;

fn scenario(reg: &Arc<Registry>, active: u8) -> Scenario {
    Scenario::new(
        reg,
        ScenarioOptions {
            active,
            ..ScenarioOptions::default()
        },
    )
}

fn add(sc: &mut Scenario, player: u8, number: &str, zone: Zone) -> Uid {
    sc.add(player, number, zone, AddOptions::default())
        .unwrap_or_else(|e| panic!("add {number}: {e:?}"))
}

fn start(mut sc: Scenario) -> GameState {
    sc.start(Step::Main).expect("the scenario starts");
    sc.into_state()
}

fn pending_kind(st: &GameState) -> Option<DecisionKind> {
    st.pending().map(|d| d.hdr.kind)
}

/// Passes action steps and declines blocks until a decision of `kind` for `player` is pending.
fn until(st: &mut GameState, player: u8, kind: DecisionKind) {
    for _ in 0..64 {
        let dec = st.pending().expect("a decision");
        if dec.hdr.kind == kind && dec.hdr.player == player {
            return;
        }
        match dec.hdr.kind {
            DecisionKind::ActionStep => {
                testkit::pass(st).expect("a pass");
            }
            DecisionKind::Block => {
                testkit::block(st, None).expect("no block");
            }
            k => panic!("unexpected {k:?} before {kind:?}"),
        }
    }
    panic!("no {kind:?} decision");
}

fn share(counts: &[u32], i: usize) -> f64 {
    f64::from(counts[i]) / f64::from(counts.iter().sum::<u32>())
}

fn option_index(st: &GameState, kind: ActionKind, a: Option<i32>) -> usize {
    st.pending()
        .expect("a decision")
        .options
        .iter()
        .position(|o| o.kind == kind && a.is_none_or(|x| o.a == x))
        .expect("the option")
}

// ---------------------------------------------------------------------------------------------
// W-24: modal choices

/// Wings of Light (GD05-102, 【Action】, Lv5 cost 2) cast while the enemy's Gundam attacks: one
/// mode returns the attacker to its owner's hand, the other recovers 3 HP on an undamaged Unit.
fn wings_of_light() -> GameState {
    let reg = Registry::embedded();
    let mut sc = scenario(&reg, 1);
    let attacker = add(&mut sc, 1, "GD01-013", Zone::Battle);
    add(&mut sc, 0, "ST01-005", Zone::Battle);
    let wings = add(&mut sc, 0, "GD05-102", Zone::Hand);
    sc.resources(0, 5, 0, 0).expect("resources");
    sc.shields(0, &[FILLER, FILLER, FILLER]).expect("shields");
    sc.shields(1, &[FILLER, FILLER, FILLER]).expect("shields");
    let mut st = start(sc);
    testkit::attack(&mut st, attacker, PLAYER_TARGET).expect("the attack");
    until(&mut st, 0, DecisionKind::ActionStep);
    testkit::play(&mut st, wings, None, 0).expect("Wings of Light");
    assert_eq!(pending_kind(&st), Some(DecisionKind::Select));
    assert_eq!(st.pending().expect("a decision").hdr.ctx(CtxKey::Mode), 1);
    st
}

/// W-24's unit test: the reference scores modes flat, so its rollouts pick each about half
/// the time; the look-ahead picks the useful mode at least 90% at the rollout temperature.
#[test]
fn modal_lookahead_picks_the_useful_mode() {
    let st = wings_of_light();
    let bounce = option_index(&st, ActionKind::Select, Some(0));
    let t = gcg_ai_reference::STANDARD.rollout_temperature;
    let reference = rollout_counts(&null_with(&gcg_ai_reference::STANDARD, &[]), &st, t, DRAWS);
    let strengthened = rollout_counts(
        &null_with(&gcg_ai_reference::STANDARD, &["modal_lookahead"]),
        &st,
        t,
        DRAWS,
    );
    eprintln!("W-24 bounce share: reference {reference:?}, modal_lookahead {strengthened:?}");
    assert!(
        (share(&reference, bounce) - 0.5).abs() < 0.02,
        "{reference:?}"
    );
    assert_eq!(reference, REFERENCE_MODE_COUNTS);
    assert!(share(&strengthened, bounce) >= 0.9, "{strengthened:?}");
}

/// The reference's mode counts at seed 11 (pinned).
const REFERENCE_MODE_COUNTS: [u32; 2] = [9977, 10023];

// ---------------------------------------------------------------------------------------------
// W-27: YES_NO

/// Garrod Ran & Tiffa Adill (GD02-094) paired: "【When Paired】You may discard 1. If you do,
/// look at the top 3 cards of your deck. You may reveal 1 (Vulture) Unit card among them and
/// add it to your hand." With no Vulture Unit on top the payoff is idle.
fn idle_optional_cost() -> GameState {
    let reg = Registry::embedded();
    let mut sc = scenario(&reg, 0);
    let unit = add(&mut sc, 0, "ST01-005", Zone::Battle);
    let pilot = add(&mut sc, 0, "GD02-094", Zone::Hand);
    add(&mut sc, 0, "GD01-013", Zone::Hand);
    sc.resources(0, 4, 0, 0).expect("resources");
    sc.deck(0, &[FILLER, FILLER, FILLER, FILLER]);
    sc.shields(0, &[FILLER, FILLER]).expect("shields");
    sc.shields(1, &[FILLER, FILLER]).expect("shields");
    let mut st = start(sc);
    testkit::play(&mut st, pilot, Some(unit), 0).expect("the pairing");
    assert_eq!(pending_kind(&st), Some(DecisionKind::YesNo));
    st
}

/// Big-Rang (EB01-030) deployed: "Look at the top 3 cards of your deck. You may reveal 1
/// (G Generation) Unit card that is Lv.3 among them and add it to your hand." Tornado Gundam
/// (EB01-016, a Lv.3 G Generation Unit) is on top: a free beneficial "you may".
fn free_beneficial_may() -> GameState {
    let reg = Registry::embedded();
    let mut sc = scenario(&reg, 0);
    let big_rang = add(&mut sc, 0, "EB01-030", Zone::Hand);
    sc.resources(0, 5, 0, 0).expect("resources");
    sc.deck(0, &["EB01-016", FILLER, FILLER, FILLER]);
    sc.shields(0, &[FILLER, FILLER]).expect("shields");
    sc.shields(1, &[FILLER, FILLER]).expect("shields");
    let mut st = start(sc);
    testkit::play(&mut st, big_rang, None, 0).expect("Big-Rang");
    assert_eq!(pending_kind(&st), Some(DecisionKind::YesNo));
    st
}

/// W-27's unit tests: a harmful optional cost is declined at least 90% (the reference says
/// yes about 88%, pinned) and a free beneficial "you may" is accepted at least 99.9%.
#[test]
fn yes_no_lookahead_declines_an_idle_cost_and_accepts_a_free_benefit() {
    let t = gcg_ai_reference::STANDARD.rollout_temperature;
    let reference = null_with(&gcg_ai_reference::STANDARD, &[]);
    let strengthened = null_with(&gcg_ai_reference::STANDARD, &["yes_no_lookahead"]);

    let st = idle_optional_cost();
    let yes = option_index(&st, ActionKind::Yes, None);
    let r = rollout_counts(&reference, &st, t, DRAWS);
    let s = rollout_counts(&strengthened, &st, t, DRAWS);
    eprintln!("W-27 idle cost yes: reference {r:?}, yes_no_lookahead {s:?}");
    assert_eq!(r, REFERENCE_IDLE_COST_COUNTS);
    assert!((share(&r, yes) - 0.881).abs() < 0.01, "{r:?}");
    assert!(share(&s, yes) <= 0.1, "{s:?}");

    let st = free_beneficial_may();
    let yes = option_index(&st, ActionKind::Yes, None);
    let r = rollout_counts(&reference, &st, t, DRAWS);
    let s = rollout_counts(&strengthened, &st, t, DRAWS);
    eprintln!("W-27 free benefit yes: reference {r:?}, yes_no_lookahead {s:?}");
    assert!(share(&s, yes) >= 0.999, "{s:?}");
}

/// The reference's YES/NO counts on the idle cost at seed 11 (pinned).
const REFERENCE_IDLE_COST_COUNTS: [u32; 2] = [17655, 2345];

// ---------------------------------------------------------------------------------------------
// W-163: Burst distributions

/// Player 1's Gundam attacks player 0, whose top Shield is `top`; player 0 holds two Units.
fn burst_position(reg: &Arc<Registry>, top: &str) -> GameState {
    let mut sc = scenario(reg, 1);
    let attacker = add(&mut sc, 1, "GD01-013", Zone::Battle);
    add(&mut sc, 0, "GD01-013", Zone::Hand);
    add(&mut sc, 0, "ST01-005", Zone::Hand);
    sc.resources(0, 6, 0, 0).expect("resources");
    sc.shields(0, &[top, FILLER, FILLER]).expect("shields");
    sc.shields(1, &[FILLER, FILLER, FILLER]).expect("shields");
    let mut st = start(sc);
    testkit::attack(&mut st, attacker, PLAYER_TARGET).expect("the attack");
    until(&mut st, 0, DecisionKind::Burst);
    st
}

/// W-163's unit tests: the strengthened policy declines the synthetic "Discard 2 cards. Draw
/// 1." Burst at least 90% where the look-ahead shows the loss, and accepts packaged Bursts
/// (one off the list, and GD05-089 on it) at least 99.9%; the reference accepts all of them.
#[test]
fn burst_decline_list_declines_a_harmful_burst_and_accepts_packaged_ones() {
    let t = gcg_ai_reference::STANDARD.rollout_temperature;
    let reference = null_with(&gcg_ai_reference::STANDARD, &[]);
    let strengthened = null_with(&gcg_ai_reference::STANDARD, &["burst_decline_list"]);

    let synthetic = Arc::new(synthetic_burst_registry());
    let st = burst_position(&synthetic, SYNTHETIC);
    let yes = option_index(&st, ActionKind::Yes, None);
    let r = rollout_counts(&reference, &st, t, DRAWS);
    let s = rollout_counts(&strengthened, &st, t, DRAWS);
    eprintln!("W-163 synthetic yes: reference {r:?}, burst_decline_list {s:?}");
    assert!(share(&r, yes) >= 0.999, "{r:?}");
    assert!(share(&s, yes) <= 0.1, "{s:?}");

    let packaged = Registry::embedded();
    for top in ["ST02-014", "GD05-089"] {
        let st = burst_position(&packaged, top);
        let yes = option_index(&st, ActionKind::Yes, None);
        let s = rollout_counts(&strengthened, &st, t, DRAWS);
        eprintln!("W-163 {top} yes: burst_decline_list {s:?}");
        assert!(share(&s, yes) >= 0.999, "{top}: {s:?}");
    }
}

// ---------------------------------------------------------------------------------------------
// The registry's nodes and programs

fn ir_counts() -> (u32, u16) {
    let ir: Value = serde_json::from_str(
        &std::fs::read_to_string(repo().join("crates/gcg-effects/ir/registry.json"))
            .expect("the IR"),
    )
    .expect("JSON");
    let n = |k: &str| ir[k].as_array().expect("a list").len();
    (
        u32::try_from(n("nodes")).expect("nodes fit in u32"),
        u16::try_from(n("programs")).expect("programs fit in u16"),
    )
}

fn nodes(reg: &Registry) -> impl Iterator<Item = &Node> {
    (0..ir_counts().0).map(move |i| reg.node(NodeId(i)))
}

fn programs() -> impl Iterator<Item = ProgramId> {
    (0..ir_counts().1).map(ProgramId)
}

// ---------------------------------------------------------------------------------------------
// W-29: the ORDER_TRIGGER heuristic

/// The first program whose steps satisfy `want`.
fn program_where(reg: &Registry, want: impl Fn(&[&Node]) -> bool) -> ProgramId {
    programs()
        .find(|&p| {
            let steps: Vec<&Node> = reg.program(p).instrs.iter().map(|&n| reg.node(n)).collect();
            want(&steps)
        })
        .expect("such a program in the registry")
}

#[test]
fn order_heuristic_draws_before_discarding_and_damages_before_repair() {
    let reg = Registry::embedded();
    let draws = |s: &[&Node]| {
        s.iter()
            .any(|n| matches!(n, Node::Draw(d) if d.player == P::You))
    };
    let discards = |s: &[&Node]| {
        s.iter()
            .any(|n| matches!(n, Node::Discard(d) if d.player == P::You))
    };
    let damages = |s: &[&Node]| s.iter().any(|n| matches!(n, Node::Damage(_)));
    let recovers = |s: &[&Node]| s.iter().any(|n| matches!(n, Node::Recover(_)));
    let harmless = |s: &[&Node]| {
        !s.iter().any(|n| {
            matches!(
                n,
                Node::Destroy(_)
                    | Node::Rest(_)
                    | Node::ToTrash(_)
                    | Node::Mill(_)
                    | Node::Exile(_)
            )
        })
    };
    let clean = |p: ProgramId| {
        let instrs = &reg.program(p).instrs;
        !super::bursts::review(&reg, instrs, 0..instrs.len()).self_harm
    };
    let draw = program_where(&reg, |s| {
        draws(s) && !discards(s) && !damages(s) && !recovers(s) && harmless(s)
    });
    let discard = program_where(&reg, |s| discards(s) && !draws(s) && !damages(s));
    let damage = program_where(&reg, |s| {
        damages(s) && !recovers(s) && !draws(s) && !discards(s)
    });
    let repair = program_where(&reg, |s| {
        recovers(s) && !damages(s) && !draws(s) && !discards(s)
    });
    let score = |p| order::program_score(&reg, p, true);
    assert!(score(draw) > score(discard), "draw before discard");
    assert!(score(damage) > score(repair), "damage before Repair");
    // The decider's own trigger that harms none of its cards goes before an opponent's.
    assert!(clean(draw));
    assert!(order::program_score(&reg, draw, true) > order::program_score(&reg, draw, false));
}

// ---------------------------------------------------------------------------------------------
// W-30: the exhaustive polarity

/// No granted rule of the registry is UNKNOWN, and every CUSTOM rule id is in the table.
#[test]
fn every_granted_rule_and_custom_id_is_signed() {
    let reg = Registry::embedded();
    let mut rules = 0;
    for node in nodes(&reg) {
        match node {
            Node::RuleGrant(g) => {
                rules += 1;
                assert_ne!(
                    polarity::rule_polarity(&reg, g.rule),
                    Polarity::Unknown,
                    "{}",
                    reg.rule_mod_repr(g.rule)
                );
            }
            Node::RuleMod(m) if m.kind == RuleKind::Custom => {
                let name = reg.str(m.name);
                assert!(
                    polarity::CUSTOM_RULES.iter().any(|(n, _)| *n == name),
                    "CUSTOM rule {name:?} needs a polarity in polarity::CUSTOM_RULES"
                );
            }
            _ => {}
        }
    }
    assert!(rules > 60, "{rules} rule grants");
    for kind in RuleKind::ALL {
        let signed = polarity::rule_kind_polarity(kind);
        let by_amount = matches!(
            kind,
            RuleKind::PlayCostDelta | RuleKind::PlayLevelDelta | RuleKind::Custom
        );
        assert_eq!(signed.is_none(), by_amount, "{kind:?}");
        assert_ne!(signed, Some(Polarity::Unknown), "{kind:?}");
    }
}

/// The sites `tests/ai/test_policy_knobs.py:96-105` lists, which the prefix rule signs wrongly,
/// classify correctly; so do the cost modifiers and computed stat changes the reference leaves
/// UNKNOWN.
#[test]
fn exhaustive_polarity_signs_the_listed_sites_and_computed_effects() {
    let reg = Registry::embedded();
    let cases = [
        (RuleKind::CantReceiveDamage, Side::Friendly, Polarity::Helps),
        (RuleKind::CantBeAttacked, Side::Friendly, Polarity::Helps),
        (
            RuleKind::StaysRestedInStartPhase,
            Side::Enemy,
            Polarity::Harms,
        ),
        (RuleKind::CantBlock, Side::Enemy, Polarity::Harms),
        (RuleKind::MayAttackActive, Side::Friendly, Polarity::Helps),
    ];
    for (kind, side, want) in cases {
        let mut sites = 0;
        for p in programs() {
            let instrs = &reg.program(p).instrs;
            for (pc, &ins) in instrs.iter().enumerate() {
                let Node::Choose(c) = reg.node(ins) else {
                    continue;
                };
                let Node::Sel(sel) = reg.node(c.sel.node()) else {
                    continue;
                };
                if sel.side != side {
                    continue;
                }
                let Some(step) = gcg_ai::targeting::first_step(&reg, instrs, pc) else {
                    continue;
                };
                let Node::Apply(a) = reg.node(instrs[step]) else {
                    continue;
                };
                let Node::RuleGrant(g) = reg.node(a.effect.node()) else {
                    continue;
                };
                let Node::RuleMod(m) = reg.node(g.rule.node()) else {
                    continue;
                };
                if m.kind != kind {
                    continue;
                }
                sites += 1;
                assert_eq!(polarity::polarity_at(&reg, instrs, pc), want, "{kind:?}");
            }
        }
        assert!(sites > 0, "{kind:?} has sites");
    }
    // Every stat change a card applies with a computed amount is signed; the reference
    // (literal parts only) leaves each UNKNOWN.
    let mut computed = 0;
    for node in nodes(&reg) {
        let Node::Apply(a) = node else { continue };
        let Node::StatMod(m) = reg.node(a.effect.node()) else {
            continue;
        };
        if [m.ap, m.hp]
            .iter()
            .any(|v| matches!(v, gcg_effects::ValueRef::Node(_)))
        {
            computed += 1;
            assert_eq!(polarity::effect_polarity(&reg, a.effect), Polarity::Helps);
        }
    }
    assert!(computed > 0, "computed stat changes");
}

// ---------------------------------------------------------------------------------------------
// W-31: exact fights

/// Whether `uid`, alone on `player`'s field, has `kind` among its rules.
fn has_rule(reg: &Arc<Registry>, number: &str, kind: RuleKind) -> bool {
    let mut sc = scenario(reg, 0);
    let uid = add(&mut sc, 1, number, Zone::Battle);
    let st = start(sc);
    let dv = gcg_engine::view::derived(&st).expect("a view");
    gcg_engine::view::rules_of(&dv, uid, kind).next().is_some()
}

/// Up to `n` real Unit numbers (in database order) with a property.
fn units_where(
    reg: &Arc<Registry>,
    n: usize,
    want: impl Fn(&gcg_data::CardDef) -> bool,
) -> Vec<String> {
    reg.db()
        .real_cards()
        .filter(|cd| {
            cd.card_type == gcg_data::CardType::Unit
                && gcg_data::ImplementationStatus::unimplemented_error(&**reg, cd.def_id).is_none()
                && want(cd)
        })
        .map(|cd| cd.card_number.to_string())
        .take(n)
        .collect()
}

/// The battle the engine plays when `attacker` attacks the rested `defender` and both players
/// pass: which of the two left the battle area.
fn engine_battle(st: &GameState, attacker: Uid, defender: Uid) -> Fight {
    let mut t = st.clone();
    testkit::attack(&mut t, attacker, i32::from(defender)).expect("the attack");
    for _ in 0..64 {
        if t.battle().is_none() || t.winner().is_some() {
            break;
        }
        let dec = t.pending().expect("a decision");
        match dec.hdr.kind {
            DecisionKind::Block => {
                testkit::block(&mut t, None).expect("no block");
            }
            DecisionKind::ActionStep => {
                testkit::pass(&mut t).expect("a pass");
            }
            _ => {
                let a = gcg_ai::choose(&t, None, 0.0, &gcg_ai::BASELINE_KNOBS).expect("a move");
                apply(&mut t, a, true).expect("a legal move");
            }
        }
    }
    let gone = |u: Uid| t.card(u).zone != Zone::Battle;
    Fight {
        kills: gone(defender),
        dies: gone(attacker),
    }
}

/// W-31's unit test: on attacker/defender pairs including First Strike, damage-reduction and
/// damage-immunity Units, the exact fight equals the engine's battle; the reference `fight()`
/// disagreements are recorded.
#[test]
fn exact_fights_equal_the_engine_battles() {
    let reg = Registry::embedded();
    let first_strike = units_where(&reg, 6, |cd| {
        reg.card(cd.def_id)
            .printed_keywords
            .get(Kw::FirstStrike)
            .is_some()
    });
    let reducing = units_where(&reg, 6, |cd| {
        has_rule(&reg, &cd.card_number, RuleKind::ReduceDamage)
    });
    let immune = units_where(&reg, 4, |cd| {
        has_rule(&reg, &cd.card_number, RuleKind::CantReceiveDamage)
    });
    // Units with an 【Attack】 trigger: the battle includes what the trigger does.
    let triggered = units_where(&reg, 8, |cd| {
        reg.card(cd.def_id)
            .timings
            .iter()
            .any(|&t| reg.str(t) == "attacks")
    });
    let plain = ["GD01-013", "ST01-005", "ST01-008", "ST04-001"];
    let attackers: Vec<&str> = first_strike
        .iter()
        .chain(&triggered)
        .map(String::as_str)
        .chain(plain)
        .collect();
    let defenders: Vec<&str> = reducing
        .iter()
        .chain(&immune)
        .map(String::as_str)
        .chain(plain)
        .collect();
    eprintln!("first strike {first_strike:?}, reducing {reducing:?}, immune {immune:?}");
    assert!(!first_strike.is_empty() && !reducing.is_empty() && !immune.is_empty());
    let cfg = null_with(&gcg_ai_reference::STANDARD, &["exact_fight"]);
    let (mut pairs, mut disagreements) = (0, 0);
    for a in &attackers {
        for d in &defenders {
            let mut sc = scenario(&reg, 0);
            let attacker = add(&mut sc, 0, a, Zone::Battle);
            let defender = sc
                .add(
                    1,
                    d,
                    Zone::Battle,
                    AddOptions {
                        rested: true,
                        ..AddOptions::default()
                    },
                )
                .expect("the defender");
            sc.shields(0, &[FILLER, FILLER]).expect("shields");
            sc.shields(1, &[FILLER, FILLER]).expect("shields");
            let st = start(sc);
            if !testkit::has_action(
                &st,
                ActionKind::Attack,
                Some(i32::from(attacker)),
                Some(i32::from(defender)),
            ) {
                continue;
            }
            let state = DecisionState::new();
            let sc = scoring(&cfg, &state, Model::None);
            let attack = gcg_engine::Action::new(
                ActionKind::Attack,
                i32::from(attacker),
                i32::from(defender),
                -1,
            );
            let attack = st
                .pending()
                .expect("a decision")
                .options
                .iter()
                .copied()
                .find(|o| o.kind == ActionKind::Attack && o.a == attack.a && o.b == attack.b)
                .expect("the attack");
            let exact = super::fight::simulate(&sc, &st, attack, attacker, Some(defender), None)
                .expect("a battle")
                .expect("budget");
            let engine = engine_battle(&st, attacker, defender);
            assert_eq!(exact.fight, engine, "{a} attacks {d}");
            assert!(!exact.blocked);
            let dv = gcg_engine::view::derived(&st).expect("a view");
            if gcg_ai::values::fight(&st, &dv, attacker, defender) != engine {
                disagreements += 1;
            }
            pairs += 1;
        }
    }
    eprintln!(
        "W-31: {pairs} pairs, reference fight() disagrees with the engine on {disagreements}"
    );
    assert!(pairs >= 40, "{pairs} pairs");
    assert_eq!(disagreements, REFERENCE_FIGHT_DISAGREEMENTS);
}

/// The reference `fight()`'s disagreements with the engine on the corpus (pinned).
const REFERENCE_FIGHT_DISAGREEMENTS: u32 = 2;

/// W-28's unit test: in the opponent's end phase, resting its only Blocker with an 【Action】
/// Command wins the next turn (the Blocker stays rested; in the next turn the card would come
/// after the block). The reference policy prefers to pass (-1.0 against 0.0); the end-phase
/// look-ahead, which plays the next turn out, scores the play far above passing. (Stopping at
/// the next main phase would not see it: the evaluation counts the Command still in hand as an
/// answer to the Blocker, whatever its timing.)
#[test]
fn end_phase_lookahead_plays_the_rest_in_the_opponents_end_phase() {
    let reg = Registry::embedded();
    let mut sc = scenario(&reg, 0);
    sc.resources(0, 2, 1, 0).expect("resources");
    sc.base(0, Some("GD01-126"), 0).expect("a Base");
    sc.add(
        0,
        "GD01-013",
        Zone::Battle,
        AddOptions {
            deployed_this_turn: true,
            ..AddOptions::default()
        },
    )
    .expect("the Gundam");
    let unheralded = add(&mut sc, 0, "GD03-121", Zone::Hand);
    add(&mut sc, 1, "GD04-061", Zone::Battle);
    sc.resources(1, 2, 0, 0).expect("resources");
    sc.deck(0, &[DEAD; 12]);
    sc.deck(1, &[DEAD; 12]);
    sc.hand(1, &[DEAD; 6]).expect("a hand");
    sc.shields(0, &[DEAD; 3]).expect("shields");
    let mut st = start(sc);
    for _ in 0..40 {
        let dec = st.pending().expect("a decision");
        if st.turn() == 4 && dec.hdr.player == 0 && dec.hdr.kind == DecisionKind::ActionStep {
            break;
        }
        let a = match dec.hdr.kind {
            DecisionKind::Main => testkit::end_main(&mut st),
            DecisionKind::ActionStep => testkit::pass(&mut st),
            k => panic!("unexpected {k:?}"),
        };
        a.expect("a move");
    }
    assert!(st.battles().is_empty());
    let play = option_index(&st, ActionKind::PlayCommand, Some(i32::from(unheralded)));
    let pass = option_index(&st, ActionKind::Pass, None);
    let dec = st.pending().expect("the action step");
    let state = DecisionState::new();
    let reference = null_with(&gcg_ai_reference::FAST, &[]);
    let r = scoring(&reference, &state, Model::None)
        .scores(&st, &dec, dec.options)
        .expect("scores");
    assert!(r[play] < r[pass], "{r:?}");
    let cfg = null_with(&gcg_ai_reference::FAST, &["end_phase_actions"]);
    let s = scoring(&cfg, &state, Model::None)
        .scores(&st, &dec, dec.options)
        .expect("scores");
    assert!(s[play] >= s[pass] + 4.0, "{s:?}");
    assert!(
        state.aux_applies() > 2,
        "the look-ahead played the next turn"
    );
}

/// A vanilla Lv.5 Unit neither player can deploy with two Resources.
const DEAD: &str = "EB01-007";

// ---------------------------------------------------------------------------------------------
// W-25: the opponent's own model

/// How often `n` rollout moves of `cfg` pick each option of `st`'s pending decision when the
/// searching player is `searcher`.
fn rollout_moves(cfg: &StrengthenedConfig, st: &GameState, searcher: u8, n: u32) -> Vec<u32> {
    let dec = st.pending().expect("a decision");
    let state = DecisionState::new();
    let mut rng = SplitMix64::new(5);
    let mut counts = vec![0; dec.options.len()];
    for _ in 0..n {
        let a = super::rollout_choice(
            &state,
            &cfg.mechanisms,
            &mut crate::mech::RolloutCx {
                s: st,
                rng: &mut rng,
                player: searcher,
                cfg,
                knobs: cfg.knobs(),
                math: crate::mech::p6o::math::Math::Platform,
            },
        )
        .expect("a move");
        let i = dec.options.iter().position(|&o| o == a).expect("an option");
        counts[i] += 1;
    }
    counts
}

/// W-25's unit test: with `opponent_knobs`, the opponent's rollout moves come from its own
/// model (the fitted offsets at a quarter of the rollout temperature), so a player 1 choosing
/// between close attack targets takes its best one far more often than the shared policy
/// does; the searching player's own moves stay the shared policy's.
#[test]
fn opponent_knobs_model_the_opponent_by_its_own_policy() {
    let reg = Registry::embedded();
    let mut sc = scenario(&reg, 1);
    add(&mut sc, 1, "GD01-013", Zone::Battle);
    for number in ["ST01-005", "GD01-022", "EB01-016"] {
        sc.add(
            0,
            number,
            Zone::Battle,
            AddOptions {
                rested: true,
                ..AddOptions::default()
            },
        )
        .expect("a rested Unit");
    }
    sc.shields(0, &[FILLER; 4]).expect("shields");
    sc.shields(1, &[FILLER; 4]).expect("shields");
    let st = start(sc);
    let dec = st.pending().expect("player 1's main phase");
    assert_eq!((dec.hdr.kind, dec.hdr.player), (DecisionKind::Main, 1));
    let shared = null_with(&gcg_ai_reference::STANDARD, &[]);
    let own = null_with(&gcg_ai_reference::STANDARD, &["opponent_knobs"]);
    let modal = |c: &[u32]| *c.iter().max().expect("counts");
    let as_opponent = rollout_moves(&own, &st, 0, DRAWS);
    let shared_counts = rollout_moves(&shared, &st, 0, DRAWS);
    let as_searcher = rollout_moves(&own, &st, 1, DRAWS);
    eprintln!(
        "W-25 player 1's moves: shared {shared_counts:?}, opponent model {as_opponent:?}, \
         as the searcher {as_searcher:?}"
    );
    assert_eq!(
        as_searcher, shared_counts,
        "the searcher's own moves are unchanged"
    );
    assert!(
        f64::from(modal(&as_opponent)) >= f64::from(modal(&shared_counts)) * 1.2,
        "{as_opponent:?} vs {shared_counts:?}"
    );
}
