//! `tests/ai/test_agents.py` (81 cases), ported: legal choices for every decision kind,
//! determinism, the decision-log format, root-move pruning and the no-effect check of the
//! reference factory's agents (random, greedy and the reference MCTS). Every agent decision is
//! compared with Python's record (`aiport::Recorder`).
//!
//! `test_random_and_greedy_finish_games_legally` draws its seeds with Hypothesis; here it runs
//! on the fixed sample of `midgame.json`'s `hypothesis_sample` (eight seeds in `[0, 2**32]`),
//! whose winner, turn, decision count and final digest Python recorded.

mod aiport;

use aiport::positions::{EXTRA_SELECTS, SITUATIONS, attack_with, ok, situation};
use aiport::{
    PLAYER, Recorder, Sc, agent, apply_ok, ids, midgame, new_game, pending_player, u64_of, with_cx,
};
use gcg_ai::{
    Agent, AgentError, BASELINE_KNOBS, DEFAULT_WEIGHTS, MoveKey, NoopFlags, PruneFlags, canonical,
    ex_used, prune_dominated, without_effect,
};
use gcg_ai_reference::{AGENT_KINDS, PRESETS, ReferenceAgent, ReferenceConfig, STANDARD};
use gcg_engine::canon::state_digest;
use gcg_engine::testkit::pass_all;
use gcg_engine::{Action, ActionKind, AddOptions, DecisionKind, GameState, Uid, Zone};

const M: &str = "test_agents";
/// `DECISION_LIMIT` (`ai/selfplay.py`).
const DECISION_LIMIT: usize = 20_000;

/// `ALL_SITUATIONS` (`test_agents.py:25-28`): every situation by name.
fn all_situations() -> Vec<(String, fn() -> GameState)> {
    let mut out: Vec<(String, fn() -> GameState)> = SITUATIONS
        .iter()
        .map(|(k, f)| (k.as_str().to_owned(), *f))
        .collect();
    out.extend(EXTRA_SELECTS.iter().map(|(n, f)| ((*n).to_owned(), *f)));
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

#[test]
fn every_agents_test_is_ported() {
    let mut ported: Vec<String> = [
        "test_situations_cover_every_decision_kind",
        "test_choose_rejects_wrong_player",
        "test_make_agent_validates_arguments",
        "test_random_agent_depends_on_seed",
        "test_mcts_log_ranks_by_visits_and_chooses_most_visited",
        "test_log_accumulates_over_a_game",
        "test_canonical_moves_merge_duplicates_and_prefer_fewer_ex",
        "test_mcts_iteration_budget_is_deterministic_and_bounded",
        "test_zero_ap_attack_without_any_follow_up_is_pruned",
        "test_zero_ap_attack_is_kept_when_an_action_step_trick_could_matter",
        "test_a_free_add_to_hand_burst_is_always_accepted",
        "test_a_command_that_changes_nothing_this_turn_is_recognised",
        "test_a_command_with_a_lasting_result_is_not_pruned",
        "test_an_activation_whose_only_result_is_its_cost_is_recognised",
        "test_an_activation_that_enables_an_attack_is_kept",
        "test_a_cost_only_activation_is_recognised_while_other_units_can_still_attack",
        "test_a_search_horizon_must_keep_wins_above_every_estimate",
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect();
    for (name, _) in all_situations() {
        for kind in AGENT_KINDS {
            ported.push(format!("test_agent_returns_a_legal_action[{name}-{kind}]"));
        }
    }
    ported.extend(ids(
        "test_same_seed_same_choices",
        ["random-200", "greedy-60", "mcts-14"],
    ));
    ported.extend(ids("test_decision_log_format", AGENT_KINDS));
    for decision in ["redraw", "choose_first"] {
        for kind in ["greedy", "mcts"] {
            ported.push(format!(
                "test_pre_game_choices_log_their_options[{decision}-{kind}]"
            ));
        }
    }
    aiport::assert_ported(M, &ported);
}

/// `make_agent(kind, seed=seed, log=log)`.
fn make(kind: &str, seed: u64, log: bool) -> Box<dyn Agent> {
    agent(kind, seed, log, None, None)
}

#[test]
fn situations_cover_every_decision_kind() {
    let rec = Recorder::new(M, "test_situations_cover_every_decision_kind");
    let mut covered: Vec<DecisionKind> = SITUATIONS
        .iter()
        .map(|(_, f)| f().pending().expect("a decision").hdr.kind)
        .collect();
    covered.sort_by_key(|k| k.as_str());
    covered.dedup();
    let mut want: Vec<DecisionKind> = DecisionKind::ALL
        .into_iter()
        .filter(|k| *k != DecisionKind::Payment)
        .collect();
    want.sort_by_key(|k| k.as_str());
    assert_eq!(covered, want);
    for (kind, build) in SITUATIONS {
        assert_eq!(build().pending().map(|d| d.hdr.kind), Some(kind));
    }
    rec.finish();
}

#[test]
fn agent_returns_a_legal_action() {
    for (name, build) in all_situations() {
        for kind in AGENT_KINDS {
            let mut rec = Recorder::new(
                M,
                &format!("test_agent_returns_a_legal_action[{name}-{kind}]"),
            );
            let mut st = build();
            let p = pending_player(&st);
            let before = state_digest(&st);
            let action = rec.choose(&mut *make(kind, 3, false), &st, p);
            assert!(st.pending().expect("a decision").options.contains(&action));
            assert_eq!(state_digest(&st), before, "choose() must not mutate st");
            apply_ok(&mut st, action);
            rec.finish();
        }
    }
}

#[test]
fn choose_rejects_wrong_player() {
    let rec = Recorder::new(M, "test_choose_rejects_wrong_player");
    let st = situation(DecisionKind::Main);
    let p = pending_player(&st);
    let err = with_cx(|cx| make("random", 0, false).choose(&st, 1 - p, cx)).unwrap_err();
    assert!(matches!(err, AgentError::NoDecision { .. }));
    assert_eq!(err.py_class(), "ValueError");
    assert!(err.to_string().contains("no decision pending"), "{err}");
    rec.finish();
}

#[test]
fn make_agent_validates_arguments() {
    let rec = Recorder::new(M, "test_make_agent_validates_arguments");
    let mut names: Vec<&str> = PRESETS.iter().map(|(n, _)| *n).collect();
    names.sort();
    assert_eq!(names, ["fast", "standard", "strong"]);
    let preset = |n: &str| PRESETS.iter().find(|(k, _)| *k == n).expect("a preset").1;
    assert!(preset("strong").max_iterations > preset("standard").max_iterations);
    assert!(preset("fast").max_iterations < preset("standard").max_iterations);
    let err = |kind: &str, p: &str| {
        gcg_ai_reference::make_agent(kind, p, 0, false, None, None)
            .err()
            .expect("an error")
            .to_string()
    };
    assert!(err("minimax", "standard").contains("unknown agent kind"));
    assert!(err("mcts", "weak").contains("unknown AI preset"));
    let name = |p: &str| {
        gcg_ai_reference::make_agent("mcts", p, 0, false, None, None)
            .expect("an agent")
            .name()
            .to_owned()
    };
    assert_eq!(name("strong"), "mcts-strong");
    assert_eq!(name("fast"), "mcts-fast");
    rec.finish();
}

/// `_choices(kind, seed, decisions)` (`test_agents.py:65-74`).
fn choices(rec: &mut Recorder, kind: &str, seed: u64, decisions: usize) -> Vec<Action> {
    let mut agents = [make(kind, seed, false), make(kind, seed + 1, false)];
    let mut st = new_game(42);
    let mut out = Vec::new();
    while let Some(dec) = st.pending() {
        if out.len() >= decisions {
            break;
        }
        let p = dec.hdr.player;
        let a = rec.choose(&mut *agents[usize::from(p)], &st, p);
        out.push(a);
        apply_ok(&mut st, a);
    }
    out
}

#[test]
fn same_seed_same_choices() {
    for (kind, decisions) in [("random", 200), ("greedy", 60), ("mcts", 14)] {
        let mut rec = Recorder::new(
            M,
            &format!("test_same_seed_same_choices[{kind}-{decisions}]"),
        );
        let first = choices(&mut rec, kind, 11, decisions);
        assert_eq!(first, choices(&mut rec, kind, 11, decisions));
        rec.finish();
    }
}

#[test]
fn random_agent_depends_on_seed() {
    let mut rec = Recorder::new(M, "test_random_agent_depends_on_seed");
    let one = choices(&mut rec, "random", 1, 60);
    assert_ne!(one, choices(&mut rec, "random", 2, 60));
    rec.finish();
}

#[test]
fn decision_log_format() {
    for kind in AGENT_KINDS {
        let mut rec = Recorder::new(M, &format!("test_decision_log_format[{kind}]"));
        let mut logging = make(kind, 5, true);
        let mut silent = make(kind, 5, false);
        let st = situation(DecisionKind::Main);
        let p = pending_player(&st);
        let chosen = rec.choose(&mut *logging, &st, p);
        assert_eq!(rec.choose(&mut *silent, &st, p), chosen);
        assert!(silent.decision_log().is_empty());
        let log = logging.decision_log();
        assert_eq!(log.len(), 1);
        let entry = &log[0];
        assert_eq!(entry.turn, st.turn());
        assert_eq!(entry.player, p);
        assert_eq!(entry.decision.as_str(), "main");
        assert_eq!(entry.chosen, chosen);
        assert!(!entry.alternatives.is_empty(), "alternatives are listed");
        let options = st.pending().expect("a decision").options;
        for alt in &entry.alternatives {
            assert!(options.contains(&alt.action));
        }
        // The keys and JSON types of the entry (`set(entry) >= {...}`, `json.dumps(log)`).
        let json = gcg_core::pyjson::dumps(&entry.to_json(), aiport::PY_SORTED);
        let v: serde_json::Value = serde_json::from_str(&json).expect("JSON");
        for key in ["turn", "player", "decision", "chosen", "alternatives"] {
            assert!(v.get(key).is_some(), "{key}");
        }
        for alt in v["alternatives"].as_array().expect("alternatives") {
            assert!(alt["value"].is_f64(), "{alt}");
            assert!(alt["visits"].is_u64(), "{alt}");
            assert!(alt.get("action").is_some());
        }
        rec.finish();
    }
}

#[test]
fn mcts_log_ranks_by_visits_and_chooses_most_visited() {
    let mut rec = Recorder::new(M, "test_mcts_log_ranks_by_visits_and_chooses_most_visited");
    let mut a = make("mcts", 9, true);
    let st = situation(DecisionKind::Main);
    let chosen = rec.choose(&mut *a, &st, pending_player(&st));
    let alts = &a.decision_log()[0].alternatives;
    let visits: Vec<u32> = alts.iter().map(|x| x.visits).collect();
    let mut sorted = visits.clone();
    sorted.sort_by(|x, y| y.cmp(x));
    assert_eq!(visits, sorted);
    assert_eq!(alts[0].action, chosen);
    assert!(visits.iter().sum::<u32>() > 0);
    rec.finish();
}

/// `run_game(agents, DECKS, seed)` (`ai/selfplay.py:60-75`): the final state and the number of
/// decisions.
fn run_game(
    rec: Option<&mut Recorder>,
    agents: &mut [Box<dyn Agent>; 2],
    seed: u64,
) -> (GameState, usize) {
    let mut st = new_game(seed);
    let mut decisions = 0;
    let mut rec = rec;
    while let Some(dec) = st.pending() {
        if decisions >= DECISION_LIMIT {
            break;
        }
        let p = dec.hdr.player;
        let a = match rec.as_deref_mut() {
            Some(r) => r.choose(&mut *agents[usize::from(p)], &st, p),
            None => with_cx(|cx| agents[usize::from(p)].choose(&st, p, cx))
                .unwrap_or_else(|e| panic!("decision {decisions}: {e}")),
        };
        apply_ok(&mut st, a);
        decisions += 1;
    }
    (st, decisions)
}

#[test]
fn log_accumulates_over_a_game() {
    let mut rec = Recorder::new(M, "test_log_accumulates_over_a_game");
    let mut agents = [make("greedy", 1, true), make("random", 2, true)];
    let (st, decisions) = run_game(Some(&mut rec), &mut agents, 8);
    assert!(st.winner().is_some());
    let logged = agents[0].decision_log().len() + agents[1].decision_log().len();
    assert_eq!(logged, decisions);
    rec.finish();
}

#[test]
fn canonical_moves_merge_duplicates_and_prefer_fewer_ex() {
    let rec = Recorder::new(
        M,
        "test_canonical_moves_merge_duplicates_and_prefer_fewer_ex",
    );
    let mut sc = Sc::new();
    sc.resources(0, 1, 0, 1);
    sc.hand(0, &["ST01-005", "ST01-005"]);
    let st = sc.start();
    let dec = st.pending().expect("a decision");
    let plays = dec
        .options
        .iter()
        .filter(|o| o.kind == ActionKind::PlayUnit)
        .count();
    assert_eq!(plays, 4);
    let moves: Vec<Action> = canonical(&st, &dec)
        .into_iter()
        .map(|(_, a)| a)
        .filter(|a| a.kind == ActionKind::PlayUnit)
        .collect();
    assert_eq!(moves.len(), 1);
    assert_eq!(ex_used(moves[0]), 0);
    rec.finish();
}

/// The Hypothesis test on its fixed sample (module docs).
#[test]
fn random_and_greedy_finish_games_legally() {
    let sample = midgame()["hypothesis_sample"]
        .as_array()
        .expect("the sample");
    assert_eq!(sample.len(), 8);
    let outcomes = aiport::par_map(sample.len(), |i| {
        let seed = u64_of(&sample[i]["seed"]);
        let mut agents = [make("greedy", seed, false), make("random", seed + 1, false)];
        let (st, decisions) = run_game(None, &mut agents, seed);
        (st.winner(), st.turn(), decisions, state_digest(&st))
    });
    for (want, (winner, turn, decisions, digest)) in sample.iter().zip(outcomes) {
        let winner = winner.map(i64::from);
        assert!(matches!(winner, Some(-1..=1)), "{want}");
        assert_eq!(winner, want["winner"].as_i64(), "{want}");
        assert_eq!(i64::from(turn), want["turn"].as_i64().expect("a turn"));
        assert_eq!(u64::try_from(decisions).ok(), want["decisions"].as_u64());
        assert_eq!(digest, want["digest"].as_str().expect("a digest"));
    }
}

#[test]
fn mcts_iteration_budget_is_deterministic_and_bounded() {
    let mut rec = Recorder::new(M, "test_mcts_iteration_budget_is_deterministic_and_bounded");
    let cfg = ReferenceConfig::new("tiny", 12, 4, 2);
    let st = situation(DecisionKind::Main);
    let mut a: Box<dyn Agent> = Box::new(ReferenceAgent::new(
        cfg.clone(),
        7,
        true,
        DEFAULT_WEIGHTS.clone(),
    ));
    rec.choose(&mut *a, &st, pending_player(&st));
    let visits: u32 = a.decision_log()[0]
        .alternatives
        .iter()
        .map(|x| x.visits)
        .sum();
    let n = canonical(&st, &st.pending().expect("a decision")).len();
    assert!(visits <= cfg.budget(n));
    assert!(cfg.budget(n) <= cfg.max_iterations);
    rec.finish();
}

#[test]
fn pre_game_choices_log_their_options() {
    for decision in [DecisionKind::Redraw, DecisionKind::ChooseFirst] {
        for kind in ["greedy", "mcts"] {
            let mut rec = Recorder::new(
                M,
                &format!(
                    "test_pre_game_choices_log_their_options[{}-{kind}]",
                    decision.as_str()
                ),
            );
            let st = situation(decision);
            let mut a = make(kind, 4, true);
            rec.choose(&mut *a, &st, pending_player(&st));
            let mut logged: Vec<Action> = a.decision_log()[0]
                .alternatives
                .iter()
                .map(|x| x.action)
                .collect();
            logged.sort_by_key(|x| (x.kind.as_str(), x.a, x.b, x.c));
            logged.dedup();
            if kind == "greedy" && decision == DecisionKind::ChooseFirst {
                assert!(logged.is_empty());
            } else {
                let mut options = st.pending().expect("a decision").options.to_vec();
                options.sort_by_key(|x| (x.kind.as_str(), x.a, x.b, x.c));
                options.dedup();
                assert_eq!(logged, options);
            }
            rec.finish();
        }
    }
}

/// `_zero_ap_attacker(trick=...)` (`test_agents.py:184-193`).
fn zero_ap_attacker(trick: bool) -> (GameState, Uid) {
    let mut sc = Sc::new();
    sc.resources(0, 3, 0, 0);
    let cgs = sc.add(0, "ST05-003");
    sc.add(1, "GD01-060");
    sc.shields(1, &["GD01-060", "GD01-060"]);
    if trick {
        sc.in_hand(0, "ST01-014");
    }
    (sc.start(), cgs)
}

fn pruned(st: &GameState) -> Vec<(MoveKey, Action)> {
    let dec = st.pending().expect("a decision");
    ok(prune_dominated(
        st,
        &dec,
        canonical(st, &dec),
        PruneFlags::default(),
    ))
}

#[test]
fn zero_ap_attack_without_any_follow_up_is_pruned() {
    let rec = Recorder::new(M, "test_zero_ap_attack_without_any_follow_up_is_pruned");
    let (st, cgs) = zero_ap_attacker(false);
    let dec = st.pending().expect("a decision");
    assert!(
        dec.options
            .iter()
            .any(|o| o.kind == ActionKind::Attack && o.a == i32::from(cgs))
    );
    let moves = pruned(&st);
    assert!(!moves.iter().any(|(_, a)| a.kind == ActionKind::Attack));
    assert!(!moves.is_empty());
    rec.finish();
}

#[test]
fn zero_ap_attack_is_kept_when_an_action_step_trick_could_matter() {
    let rec = Recorder::new(
        M,
        "test_zero_ap_attack_is_kept_when_an_action_step_trick_could_matter",
    );
    let (st, cgs) = zero_ap_attacker(true);
    assert!(
        pruned(&st)
            .iter()
            .any(|(_, a)| a.kind == ActionKind::Attack && a.a == i32::from(cgs))
    );
    rec.finish();
}

#[test]
fn a_free_add_to_hand_burst_is_always_accepted() {
    let mut rec = Recorder::new(M, "test_a_free_add_to_hand_burst_is_always_accepted");
    let mut sc = Sc::active(1);
    let attacker = sc.add(1, "GD01-060");
    sc.shields(0, &["ST05-010"]);
    let mut st = sc.start();
    attack_with(&mut st, attacker, PLAYER);
    ok(pass_all(&mut st, 50));
    assert_eq!(st.pending().map(|d| d.hdr.kind), Some(DecisionKind::Burst));
    let kinds: Vec<ActionKind> = pruned(&st).into_iter().map(|(_, a)| a.kind).collect();
    assert_eq!(kinds, [ActionKind::Yes]);
    for seed in 0..3 {
        assert_eq!(
            rec.choose(&mut *make("mcts", seed, false), &st, 0),
            Action::of(ActionKind::Yes)
        );
    }
    rec.finish();
}

/// `_after_attacks_with(command)` (`test_agents.py:229-237`).
fn after_attacks_with(command: &str) -> (GameState, Uid) {
    let mut sc = Sc::new();
    sc.resources(0, 4, 0, 0);
    sc.rested(0, "GD01-060");
    sc.add(1, "GD01-060");
    sc.shields(1, &["GD01-060", "GD01-060"]);
    let card = sc.in_hand(0, command);
    (sc.start(), card)
}

/// `without_effect(st, 0, dec, moves, seed=seed)` with the defaults.
fn idle(st: &GameState, moves: &[(MoveKey, Action)], seed: u64) -> Vec<MoveKey> {
    let dec = st.pending().expect("a decision");
    ok(without_effect(
        st,
        0,
        &dec,
        moves,
        seed,
        &BASELINE_KNOBS,
        NoopFlags::default(),
        DEFAULT_WEIGHTS,
    ))
}

#[test]
fn a_command_that_changes_nothing_this_turn_is_recognised() {
    let mut rec = Recorder::new(
        M,
        "test_a_command_that_changes_nothing_this_turn_is_recognised",
    );
    let (st, card) = after_attacks_with("ST01-014");
    let moves = canonical(&st, &st.pending().expect("a decision"));
    let none = idle(&st, &moves, 3);
    let flagged: Vec<Action> = moves
        .iter()
        .filter(|(k, _)| none.contains(k))
        .map(|(_, a)| *a)
        .collect();
    let commands: Vec<Action> = moves
        .iter()
        .map(|(_, a)| *a)
        .filter(|a| a.kind == ActionKind::PlayCommand && a.a == i32::from(card))
        .collect();
    assert_eq!(flagged, commands);
    assert_eq!(
        rec.choose(&mut *make("mcts", 1, false), &st, 0).kind,
        ActionKind::EndMain
    );
    rec.finish();
}

#[test]
fn a_command_with_a_lasting_result_is_not_pruned() {
    let rec = Recorder::new(M, "test_a_command_with_a_lasting_result_is_not_pruned");
    let (st, _) = after_attacks_with("GD01-100");
    let moves = canonical(&st, &st.pending().expect("a decision"));
    assert!(idle(&st, &moves, 3).is_empty());
    rec.finish();
}

/// `_jaburo(enemy_rested, attacker=False)` (`test_agents.py:256-270`): the state and the keys
/// of its activation moves.
fn jaburo(enemy_rested: bool, attacker: bool) -> (GameState, Vec<(MoveKey, Action)>, Vec<MoveKey>) {
    let mut sc = Sc::new();
    sc.base(0, Some("GD04-122"));
    sc.add(0, "GD01-008");
    if attacker {
        sc.add(0, "ST14-008");
    }
    sc.add_with(
        1,
        "GD01-060",
        Zone::Battle,
        AddOptions {
            rested: enemy_rested,
            ..AddOptions::default()
        },
    );
    sc.shields(1, &["GD01-060", "GD01-060"]);
    let st = sc.start();
    let moves = canonical(&st, &st.pending().expect("a decision"));
    let activations: Vec<MoveKey> = moves
        .iter()
        .filter(|(_, a)| a.kind == ActionKind::Activate)
        .map(|(k, _)| *k)
        .collect();
    assert!(!activations.is_empty());
    (st, moves, activations)
}

#[test]
fn an_activation_whose_only_result_is_its_cost_is_recognised() {
    let rec = Recorder::new(
        M,
        "test_an_activation_whose_only_result_is_its_cost_is_recognised",
    );
    let (st, moves, activations) = jaburo(true, false);
    let none = idle(&st, &moves, 5);
    assert!(activations.iter().all(|k| none.contains(k)));
    rec.finish();
}

#[test]
fn an_activation_that_enables_an_attack_is_kept() {
    let rec = Recorder::new(M, "test_an_activation_that_enables_an_attack_is_kept");
    let (st, moves, activations) = jaburo(false, true);
    let none = idle(&st, &moves, 5);
    assert!(!activations.iter().any(|k| none.contains(k)));
    rec.finish();
}

#[test]
fn a_cost_only_activation_is_recognised_while_other_units_can_still_attack() {
    let rec = Recorder::new(
        M,
        "test_a_cost_only_activation_is_recognised_while_other_units_can_still_attack",
    );
    let (st, moves, activations) = jaburo(true, true);
    for seed in 0..4 {
        let none = idle(&st, &moves, seed);
        assert!(activations.iter().all(|k| none.contains(k)), "seed {seed}");
    }
    rec.finish();
}

#[test]
fn a_search_horizon_must_keep_wins_above_every_estimate() {
    let rec = Recorder::new(
        M,
        "test_a_search_horizon_must_keep_wins_above_every_estimate",
    );
    let four = ReferenceConfig {
        horizon_turns: 4,
        ..STANDARD.clone()
    };
    assert!(four.check().is_ok());
    assert_eq!(four.horizon_turns, 4);
    let five = ReferenceConfig {
        horizon_turns: 5,
        ..STANDARD.clone()
    };
    let err = five.check().unwrap_err();
    assert!(
        err.to_string().contains("horizon_turns=5 is too long"),
        "{err}"
    );
    rec.finish();
}
