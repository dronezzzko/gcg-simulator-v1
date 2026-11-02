//! P6-E's unit tests: the strengthened steps with their options off are the reference's steps,
//! every flag runs on the searched decisions of the D2 vectors, the EX-sensitivity table, the
//! typed merge (W-10), the real idle line (W-141) and the fail-safe caps (W-12).

use std::path::PathBuf;

use gcg_ai::without_effect;
use gcg_ai::{AgentCx, MoveKey, NoTrace, NoopFlags, PruneFlags, Scratch, SearchResult};
use gcg_ai::{ROLE_WEIGHTS, canonical, make_greedy_agent, merge_equivalent, prune_dominated};
use gcg_core::pool::{Cancel, Pool, TaskCx};
use gcg_core::rng::{SeedPart, SplitMix64, derive_seed};
use gcg_effects::Registry;
use gcg_engine::canon::from_canonical;
use gcg_engine::observe::determinize;

use gcg_engine::ScenarioOptions;
use gcg_engine::{Action, ActionKind, AddOptions, GameState, NewGameOptions, Scenario};
use gcg_engine::{StateSnapshot, Step, Zone, apply, new_game_by_number};
use serde_json::Value;

use super::noop;
use crate::mcts::run;
use crate::mech::Composed;
use crate::{MechanismEffort, Observation, Preset, StrengthenedConfig, preset_config};

/// The searched decisions of the D2 vectors (`crates/gcg-ai/tests/vectors/search/`), as
/// `(state, player, per-decision seed)`.
fn searched_states() -> Vec<(GameState, u8, u64)> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../gcg-ai/tests/vectors/search/decisions.ndjson");
    let reg = Registry::embedded();
    std::fs::read_to_string(path)
        .expect("the vectors")
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("a record"))
        .filter(|rec| rec["search"].as_array().is_some_and(|s| !s.is_empty()))
        .map(|rec| {
            let snap = &rec["snapshot"];
            let st = from_canonical(
                &reg,
                &StateSnapshot {
                    state: snap["state"].clone(),
                    continuous: snap["continuous"].clone(),
                    filter_sets: snap["filter_sets"].clone(),
                    rules_checked: snap["rules_checked"].as_i64().expect("rules_checked"),
                },
            )
            .expect("a state");
            let player = u8::try_from(rec["player"].as_i64().expect("a seat")).expect("a seat");
            let seed = rec["seed"]
                .as_u64()
                .or_else(|| rec["seed"].as_str().and_then(|s| s.parse().ok()))
                .expect("a seed");
            (st, player, seed)
        })
        .collect()
}

/// `fast` (the reference `standard` budget) with every mechanism off but `on`.
fn config(on: &[(&str, &str)]) -> StrengthenedConfig {
    let mut cfg = preset_config(Preset::Fast).clone();
    cfg.mechanisms.engine_error_leaf = false;
    for &(f, v) in on {
        assert_eq!(cfg.mechanisms.set_text(f, v), Some(Ok(())), "{f}");
    }
    cfg
}

/// One search of `cfg` at the observation of `player`, with the root information it left.
fn search(
    st: &GameState,
    player: u8,
    cfg: &StrengthenedConfig,
    seed: u64,
) -> (SearchResult, super::RootInfo) {
    let cancel = Cancel::new();
    let obs = Observation::new(st, player).expect("an observation");
    let composed = Composed::new(cfg);
    let (result, _): (SearchResult, MechanismEffort) =
        Pool::run_inline(|scratch: &mut Scratch, par: &TaskCx<'_, Scratch>| {
            let mut cx = AgentCx {
                scratch,
                par,
                cancel: &cancel,
            };
            let mut rng = SplitMix64::new(seed);
            run(
                &composed,
                obs.info(),
                player,
                cfg,
                &mut rng,
                &mut cx,
                &mut NoTrace,
                None,
            )
        })
        .unwrap_or_else(|e| panic!("search failed: {e}"));
    (result, composed.p6e.info())
}

fn bits(r: &SearchResult) -> Vec<(Action, u64, u32)> {
    r.alternatives
        .iter()
        .map(|a| (a.action, a.value.to_bits(), a.visits))
        .collect()
}

#[test]
fn the_ported_no_effect_check_with_its_options_off_is_the_reference() {
    let mut checked = 0;
    for (st, player, seed) in searched_states() {
        let obs = Observation::new(&st, player).expect("an observation");
        let info = obs.info();
        let dec = info.pending().expect("a decision");
        let cfg = preset_config(Preset::Standard);
        let moves: Vec<(MoveKey, Action)> = prune_dominated(
            info,
            &dec,
            canonical(info, &dec),
            PruneFlags {
                strict_attacks: true,
                rest_targets: true,
            },
        )
        .expect("prune");
        for branch_choices in [false, true] {
            let reference = without_effect(
                info,
                player,
                &dec,
                &moves,
                seed,
                &cfg.knobs(),
                NoopFlags {
                    branch_choices,
                    resource_aware: true,
                },
                &ROLE_WEIGHTS,
            )
            .expect("reference");
            let world = determinize(info, player, seed).expect("a world");
            let ported = noop::without_effect(
                info,
                player,
                &dec,
                &moves,
                &[&world],
                &cfg.knobs(),
                noop::Opts {
                    fail_safe: false,
                    branch_all: false,
                    real_idle: false,
                    branch_choices,
                    resource_aware: true,
                    max_decisions: noop::MAX_DECISIONS,
                    node_cap: noop::BRANCH_NODE_CAP,
                    record_idle: false,
                },
                &ROLE_WEIGHTS,
            )
            .expect("ported");
            assert_eq!(ported.idle, reference);
            assert!(ported.penalised.is_empty());
            checked += 1;
        }
    }
    assert!(checked >= 100, "{checked}");
}

/// Every flag alone and all together search every third searched decision of the D2 vectors
/// without error; the counts show how often each changes the null configuration's search and
/// what its root judgements cost.
#[test]
fn every_flag_runs_on_the_vector_decisions() {
    let states: Vec<(GameState, u8, u64)> = searched_states().into_iter().step_by(3).collect();
    let rows: [&[(&str, &str)]; 9] = [
        &[],
        &[("ex_variants", "true")],
        &[("typed_fingerprint", "true")],
        &[("prune_worlds", "4")],
        &[("branch_all_selects", "true")],
        &[("rest_prune_worlds", "true")],
        &[("target_rollouts", "true")],
        &[("real_idle_line", "true")],
        &[
            ("ex_variants", "true"),
            ("typed_fingerprint", "true"),
            ("prune_worlds", "4"),
            ("branch_all_selects", "true"),
            ("rest_prune_worlds", "true"),
            ("target_rollouts", "true"),
            ("real_idle_line", "true"),
        ],
    ];
    let null = config(&[]);
    let nulls: Vec<SearchResult> = states
        .iter()
        .map(|(st, player, seed)| search(st, *player, &null, *seed).0)
        .collect();
    for on in rows {
        let cfg = config(on);
        let (mut changed, mut applies, mut iterations) = (0, 0u64, 0u64);
        for ((st, player, seed), r0) in states.iter().zip(&nulls) {
            let (r, info) = search(st, *player, &cfg, *seed);
            changed += usize::from(bits(&r) != bits(r0) || r.stats.roots != r0.stats.roots);
            applies += info.applies;
            iterations += u64::from(r.stats.iterations);
        }
        eprintln!(
            "{on:?}: {changed}/{} searches differ from the null configuration, \
             {applies} root applies, {iterations} iterations",
            states.len()
        );
        if on.is_empty() {
            assert_eq!(changed, 0);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Positions

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

/// The AI test decks (`tests/ai/decks.py:68-71`), as exported with the midgame vectors.
fn test_decks() -> [(Vec<String>, Vec<String>); 2] {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../gcg-ai/tests/vectors/ai_tests/midgame.json");
    let v: Value =
        serde_json::from_str(&std::fs::read_to_string(path).expect("midgame.json")).expect("JSON");
    let part = |deck: &Value, key: &str| -> Vec<String> {
        deck[key]
            .as_array()
            .expect("card numbers")
            .iter()
            .map(|n| n.as_str().expect("a number").to_owned())
            .collect()
    };
    let d = v["decks"].as_array().expect("two decks");
    [
        (part(&d[0], "main"), part(&d[0], "res")),
        (part(&d[1], "main"), part(&d[1], "res")),
    ]
}

/// A new game of the AI test decks with game seed `seed`.
fn test_game(seed: u64) -> GameState {
    let decks = test_decks();
    let refs: Vec<(Vec<&str>, Vec<&str>)> = decks
        .iter()
        .map(|(m, r)| {
            (
                m.iter().map(String::as_str).collect(),
                r.iter().map(String::as_str).collect(),
            )
        })
        .collect();
    new_game_by_number(
        &Registry::embedded(),
        [
            (&refs[0].0[..], &refs[0].1[..]),
            (&refs[1].0[..], &refs[1].1[..]),
        ],
        seed,
        NewGameOptions::default(),
    )
    .expect("the AI test decks are legal")
}

/// The state at decision `decision` (counted from 0) of greedy self-play game `game` (agent
/// seeds `game` and `game + 1000`, the shipped weights).
fn greedy_state(game: u64, decision: usize) -> GameState {
    let mut st = test_game(game);
    let mut agents = [
        make_greedy_agent(game, false, ROLE_WEIGHTS.clone()),
        make_greedy_agent(game + 1000, false, ROLE_WEIGHTS.clone()),
    ];
    for _ in 0..decision {
        let p = st.pending().expect("a decision").hdr.player;
        let a = with_cx(|cx| agents[usize::from(p)].choose(&st, p, cx)).expect("greedy");
        apply(&mut st, a, true).expect("apply");
    }
    st
}

/// The options of the strengthened no-effect check with the configuration's reference flags.
const fn opts(fail_safe: bool, real_idle: bool, max_decisions: usize) -> noop::Opts {
    noop::Opts {
        fail_safe,
        branch_all: false,
        real_idle,
        branch_choices: true,
        resource_aware: true,
        max_decisions,
        node_cap: noop::BRANCH_NODE_CAP,
        record_idle: true,
    }
}

/// The root moves the reference judges with its no-effect check (`ai/mcts.py:89-103`).
fn reference_moves(st: &GameState) -> Vec<(MoveKey, Action)> {
    let dec = st.pending().expect("a decision");
    prune_dominated(
        st,
        &dec,
        canonical(st, &dec),
        PruneFlags {
            strict_attacks: true,
            rest_targets: true,
        },
    )
    .expect("prune")
}

// ---------------------------------------------------------------------------------------------
// W-141: the real idle line

/// Game 53 of greedy self-play on the AI test decks, decision 24 (player 0, an action step):
/// the deterministic policy, continued in the world without the Command, casts the same
/// Command, so the reference's branch check compares the branch with the policy's own use of
/// the move and prunes it (found by comparing the null configuration with `real_idle_line`
/// over 60 greedy games; the other differences there were moves the reference kept only
/// because its idle line re-cast them).
#[test]
fn the_real_idle_line_never_replays_the_move_under_test() {
    let st = greedy_state(53, 24);
    let player = st.pending().expect("a decision").hdr.player;
    let obs = Observation::new(&st, player).expect("an observation");
    let info = obs.info();
    let dec = info.pending().expect("a decision");
    let moves = reference_moves(info);
    let cfg = preset_config(Preset::Standard);
    // The per-decision generator of `search-standard` (seed 1), first decision: its first draw
    // is the no-effect world's seed (`ai/mcts.py:96`).
    let decision_seed = derive_seed(
        1,
        &[
            SeedPart::Str("search-standard"),
            SeedPart::Str("decision"),
            SeedPart::Int(1),
        ],
    );
    let seed = SplitMix64::new(decision_seed).next_u64();
    let reference = without_effect(
        info,
        player,
        &dec,
        &moves,
        seed,
        &cfg.knobs(),
        NoopFlags {
            branch_choices: true,
            resource_aware: true,
        },
        &ROLE_WEIGHTS,
    )
    .expect("the reference check");
    let (key, action) = *moves
        .iter()
        .find(|(k, _)| k.kind == ActionKind::PlayCommand)
        .expect("a Command");
    assert_eq!(
        reference,
        [key],
        "the reference prunes the Command (pinned)"
    );
    let world = determinize(info, player, seed).expect("the world");
    let density = gcg_ai::burst_density(info, player);
    let mut applies = 0;
    let of_card = |a: &Action| a.kind == action.kind && a.a == action.a;
    let (old, old_idle) = noop::judge_traced(
        &world,
        player,
        action,
        &cfg.knobs(),
        opts(false, false, noop::MAX_DECISIONS),
        &ROLE_WEIGHTS,
        density,
        &mut applies,
    )
    .expect("the ported check");
    assert_eq!(old, noop::Verdict::NoEffect);
    assert!(
        old_idle.iter().any(of_card),
        "the reference's idle line casts the Command itself"
    );
    let (new, new_idle) = noop::judge_traced(
        &world,
        player,
        action,
        &cfg.knobs(),
        opts(false, true, noop::MAX_DECISIONS),
        &ROLE_WEIGHTS,
        density,
        &mut applies,
    )
    .expect("the strengthened check");
    assert_eq!(
        new,
        noop::Verdict::Effect,
        "the strengthened check keeps it"
    );
    assert!(!new_idle.is_empty());
    assert!(
        !new_idle.iter().any(of_card),
        "the real idle line takes no action of the Command's card"
    );
}

// ---------------------------------------------------------------------------------------------
// W-12: caps and the fail-safe

/// Siege Ploy (ST02-014, rest 1 enemy Unit with 5 or less HP) when the only enemy Unit is
/// already rested: without effect.
fn idle_siege_ploy() -> GameState {
    let mut sc = Scenario::new(&Registry::embedded(), ScenarioOptions::default());
    sc.resources(0, 3, 0, 0).expect("resources");
    sc.add(0, "ST01-005", Zone::Battle, AddOptions::default())
        .expect("GM");
    sc.add(0, "ST02-014", Zone::Hand, AddOptions::default())
        .expect("Siege Ploy");
    sc.add(
        1,
        "ST04-005",
        Zone::Battle,
        AddOptions {
            rested: true,
            ..AddOptions::default()
        },
    )
    .expect("Strike Dagger");
    sc.shields(0, &["GD01-060"; 3]).expect("shields");
    sc.start(Step::Main).expect("start");
    sc.into_state()
}

#[test]
fn a_forced_cap_hit_keeps_the_move() {
    let st = idle_siege_ploy();
    let dec = st.pending().expect("the main phase");
    let moves = reference_moves(&st);
    let cfg = preset_config(Preset::Standard);
    let (key, action) = *moves
        .iter()
        .find(|(k, _)| k.kind == ActionKind::PlayCommand)
        .expect("Siege Ploy");
    let world = determinize(&st, 0, 7).expect("a world");
    let density = gcg_ai::burst_density(&st, 0);
    let judged = |o: noop::Opts| {
        let mut applies = 0;
        noop::judge_traced(
            &world,
            0,
            action,
            &cfg.knobs(),
            o,
            &ROLE_WEIGHTS,
            density,
            &mut applies,
        )
        .expect("judged")
        .0
    };
    // At the full cap the move is without effect, in the reference and in the port.
    let reference = without_effect(
        &st,
        0,
        &dec,
        &moves,
        7,
        &cfg.knobs(),
        NoopFlags {
            branch_choices: true,
            resource_aware: true,
        },
        &ROLE_WEIGHTS,
    )
    .expect("the reference check");
    assert_eq!(reference, [key]);
    assert_eq!(
        judged(opts(true, false, noop::MAX_DECISIONS)),
        noop::Verdict::NoEffect
    );
    // A cap of no policy decision cuts every line: the reference compares the cut lines as
    // complete (pinned: without effect), the fail-safe makes the judgement unsure.
    assert_eq!(judged(opts(false, false, 0)), noop::Verdict::NoEffect);
    assert_eq!(judged(opts(true, false, 0)), noop::Verdict::Unsure);
    let found = noop::without_effect(
        &st,
        0,
        &dec,
        &moves,
        &[&world],
        &cfg.knobs(),
        opts(true, false, 0),
        &ROLE_WEIGHTS,
    )
    .expect("the strengthened check");
    assert!(found.idle.is_empty(), "the move is kept");
    assert_eq!(found.penalised, [key], "with the fail-safe prior penalty");
}

// ---------------------------------------------------------------------------------------------
// W-13: the negative control of the Freedom scenario

/// `tests/scenarios/p6e_w13_freedom.rs::freedom_lethal`: Amuro's 【When Paired】 target, with the
/// lethal choice (the rested Zaku) and Freedom.
fn freedom_lethal() -> (GameState, Action, Action) {
    let mut sc = Scenario::new(&Registry::embedded(), ScenarioOptions::default());
    sc.resources(0, 4, 0, 0).expect("resources");
    let host = sc
        .add(0, "GD01-013", Zone::Battle, AddOptions::default())
        .expect("Gundam");
    sc.add(0, "ST01-005", Zone::Battle, AddOptions::default())
        .expect("GM");
    let amuro = sc
        .add(0, "ST01-010", Zone::Hand, AddOptions::default())
        .expect("Amuro");
    sc.shields(0, &["GD01-060"; 5]).expect("shields");
    sc.deck(0, &["GD01-060"; 20]);
    let freedom = sc
        .add(1, "GD03-070", Zone::Battle, AddOptions::default())
        .expect("Freedom");
    let zaku = sc
        .add(
            1,
            "GD01-035",
            Zone::Battle,
            AddOptions {
                rested: true,
                ..AddOptions::default()
            },
        )
        .expect("Zaku");
    sc.shields(1, &["GD01-060"]).expect("shield");
    sc.deck(1, &["GD01-060"; 20]);
    sc.start(Step::Main).expect("start");
    let mut st = sc.into_state();
    gcg_engine::testkit::play(&mut st, amuro, Some(host), 0).expect("pair Amuro");
    let select = |u: gcg_engine::Uid| Action::new(ActionKind::Select, i32::from(u), -1, -1);
    (st, select(zaku), select(freedom))
}

/// The no-op Rest-target prune without its "while rested" exception: every rested enemy
/// target dropped (`ai/actions.py:235-239`), the rest of the root as the reference's.
struct NoException;

impl crate::mech::Mechanisms for NoException {
    fn root_moves(
        &self,
        cx: &mut crate::mech::RootCx<'_>,
    ) -> Result<Vec<(MoveKey, Action)>, gcg_engine::EngineError> {
        let st = cx.st;
        let kept: Vec<(MoveKey, Action)> = canonical(st, cx.dec)
            .into_iter()
            .filter(|(_, a)| {
                let c = st.card(gcg_engine::Uid::try_from(a.a).expect("a uid"));
                !(c.owner != cx.player && c.zone == Zone::Battle && c.rested)
            })
            .collect();
        Ok(kept)
    }
}

#[test]
fn without_the_exception_the_freedom_lethal_is_missed() {
    let (st, lethal, freedom) = freedom_lethal();
    let dec = st.pending().expect("the target");
    // The reference prune keeps both targets: resting Freedom changes the derived view.
    assert_eq!(reference_moves(&st).len(), 2);
    let cfg = preset_config(Preset::Fast);
    let obs = Observation::new(&st, 0).expect("an observation");
    let mut found = 0;
    for seed in 1..=16u64 {
        let (r, _) = with_cx(|cx| {
            let mut rng = SplitMix64::new(seed);
            run(
                &NoException,
                obs.info(),
                0,
                cfg,
                &mut rng,
                cx,
                &mut NoTrace,
                None,
            )
        })
        .expect("a search");
        assert_eq!(r.action, freedom, "the only target left");
        found += u32::from(r.action == lethal);
    }
    assert_eq!(dec.options.len(), 2);
    assert_eq!(
        found, 0,
        "the negative control misses the lethal in every seed"
    );
}

// ---------------------------------------------------------------------------------------------
// W-09: the EX-sensitivity table

/// The node indices an IR JSON value refers to (`{"$": i}`), recursively.
fn refs(v: &Value, out: &mut Vec<usize>) {
    match v {
        Value::Object(m) => {
            if let Some(i) = m.get("$").and_then(Value::as_u64) {
                out.push(usize::try_from(i).expect("an index"));
                return;
            }
            for x in m.values() {
                refs(x, out);
            }
        }
        Value::Array(a) => a.iter().for_each(|x| refs(x, out)),
        _ => {}
    }
}

#[test]
fn the_ex_sensitivity_table_matches_a_walk_of_the_ir() {
    // An independent walk of the IR JSON: every node reachable from an ability that reads an
    // EX payment, and every exiled trigger whose subject is an EX Resource.
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../gcg-effects/ir/registry.json");
    let ir: Value =
        serde_json::from_str(&std::fs::read_to_string(path).expect("the IR")).expect("JSON");
    let nodes = ir["nodes"].as_array().expect("nodes");
    let reads = |n: &Value| {
        let class = n["_"].as_str().unwrap_or("");
        (matches!(class, "EventAmount" | "EventFlag") && n["key"] == "ex_used")
            || (class == "CustomCond"
                && n["name"]
                    .as_str()
                    .is_some_and(|s| s.ends_with("_played_with_ex")))
    };
    let reaches = |root: usize| {
        let mut stack = vec![root];
        let mut seen = vec![false; nodes.len()];
        let mut ex_resource = false;
        let mut hit = false;
        while let Some(i) = stack.pop() {
            if std::mem::replace(&mut seen[i], true) {
                continue;
            }
            hit |= reads(&nodes[i]);
            ex_resource |= nodes[i]["kinds"]
                .as_array()
                .is_some_and(|k| k.iter().any(|x| x == "ex_resource"));
            refs(&nodes[i], &mut stack);
        }
        (hit, ex_resource)
    };
    let (mut own, mut commands, mut exiled) = (Vec::new(), Vec::new(), Vec::new());
    for ab in ir["abilities"].as_array().expect("abilities") {
        let number = ab["card_number"].as_str().expect("a number").to_owned();
        let (hit, ex_resource) =
            reaches(usize::try_from(ab["ability"].as_u64().expect("a node")).expect("an index"));
        let event = ab["trigger_event"].as_str();
        if hit {
            if event == Some("command_played") {
                commands.push(number.clone());
            } else {
                own.push(number.clone());
            }
        }
        if event == Some("exiled") && ex_resource {
            exiled.push(number);
        }
    }
    let reg = Registry::embedded();
    let (mut t_own, mut t_commands, mut t_exiled) = (Vec::new(), Vec::new(), Vec::new());
    for def in reg.db().defs() {
        let s = super::ex::sense(&reg, def.def_id);
        let n = def.card_number.to_string();
        if s.own {
            t_own.push(n.clone());
        }
        if s.commands {
            t_commands.push(n.clone());
        }
        if s.exiled {
            t_exiled.push(n);
        }
    }
    for v in [
        &mut own,
        &mut commands,
        &mut exiled,
        &mut t_own,
        &mut t_commands,
        &mut t_exiled,
    ] {
        v.sort();
        v.dedup();
    }
    assert_eq!(t_own, own);
    assert_eq!(t_commands, commands);
    assert_eq!(t_exiled, exiled);
    // The cards W-09 names.
    assert_eq!(own, ["GD04-106", "GD04-108", "GD05-118"]);
    assert_eq!(commands, ["GD04-020", "GD04-021", "GD04-085"]);
    assert_eq!(exiled, ["GD05-018"]);
}

// ---------------------------------------------------------------------------------------------
// W-10: typed fingerprints

/// Two GM (ST01-005) copies that can attack the player.
fn two_copies() -> (GameState, gcg_engine::Uid, gcg_engine::Uid, gcg_engine::Uid) {
    let mut sc = Scenario::new(&Registry::embedded(), ScenarioOptions::default());
    sc.resources(0, 2, 0, 0).expect("resources");
    let a = sc
        .add(0, "ST01-005", Zone::Battle, AddOptions::default())
        .expect("GM");
    let b = sc
        .add(0, "ST01-005", Zone::Battle, AddOptions::default())
        .expect("GM");
    let other = sc
        .add(0, "GD01-013", Zone::Battle, AddOptions::default())
        .expect("Gundam");
    sc.shields(0, &["GD01-060"; 5]).expect("shields");
    sc.shields(1, &["GD01-060"; 5]).expect("shields");
    sc.start(Step::Main).expect("start");
    (sc.into_state(), a, b, other)
}

fn attacks(moves: &[(MoveKey, Action)]) -> Vec<i32> {
    moves
        .iter()
        .filter(|(_, a)| a.kind == ActionKind::Attack)
        .map(|(_, a)| a.a)
        .collect()
}

#[test]
fn typed_fingerprints_ignore_numbers_that_merely_equal_a_uid() {
    let (mut st, a, b, other) = two_copies();
    let dec = st.pending().expect("the main phase");
    let moves = canonical(&st, &dec);
    let plain = attacks(&merge_equivalent(&st, &dec, moves.clone()).expect("merge"));
    assert!(plain.contains(&i32::from(a)) && !plain.contains(&i32::from(b)));
    // The Gundam's once-per-turn record `(ability, host, zone_seq)` whose zone sequence number
    // happens to equal the first copy's uid.
    let key = [0, i32::from(other), i32::from(a)];
    gcg_engine::testkit::engine::once_add(&mut st, &key).expect("a once key");
    let dec = st.pending().expect("the main phase");
    let moves = canonical(&st, &dec);
    let reference = attacks(&merge_equivalent(&st, &dec, moves.clone()).expect("merge"));
    assert!(
        reference.contains(&i32::from(a)) && reference.contains(&i32::from(b)),
        "the reference keeps the copies apart (pinned)"
    );
    let typed = attacks(&super::fingerprint::merge_equivalent(&st, &dec, moves).expect("merge"));
    assert!(
        typed.contains(&i32::from(a)) && !typed.contains(&i32::from(b)),
        "the typed fingerprint merges them"
    );
    // A record that names the copy as its host still keeps it apart.
    gcg_engine::testkit::engine::once_add(&mut st, &[0, i32::from(a), 1]).expect("a once key");
    let dec = st.pending().expect("the main phase");
    let moves = canonical(&st, &dec);
    let typed = attacks(&super::fingerprint::merge_equivalent(&st, &dec, moves).expect("merge"));
    assert!(typed.contains(&i32::from(a)) && typed.contains(&i32::from(b)));
}

/// A uid-free summary of a state: the turn, phase owner, winner, pending decision kind and
/// option count, every card as `(owner, def, zone, rested, damage, pair def)` sorted, the
/// lasting effects as `(effect, duration, player, target defs sorted)` sorted, and the hand and
/// deck sizes. Two moves on interchangeable cards lead to equal summaries.
fn summary(st: &GameState) -> String {
    let def = |u: i32| {
        usize::try_from(u)
            .ok()
            .filter(|&u| u < st.cards().len())
            .map_or(-1, |u| i32::from(st.cards()[u].def_id.0))
    };
    let mut cards: Vec<(u8, u16, Zone, bool, i32, i32)> = st
        .cards()
        .iter()
        .map(|c| (c.owner, c.def_id.0, c.zone, c.rested, c.damage, def(c.pair)))
        .collect();
    cards.sort();
    let mut lasting: Vec<String> = st
        .lasting()
        .iter()
        .chain(st.delayed())
        .map(|le| {
            let mut t: Vec<i32> = st
                .lasting_targets(le)
                .iter()
                .map(|&(u, _)| def(i32::from(u)))
                .collect();
            t.sort();
            format!("{:?}{:?}{}{t:?}", le.effect, le.duration, le.player)
        })
        .collect();
    lasting.sort();
    let pending = st
        .pending()
        .map(|d| (d.hdr.kind, d.hdr.player, d.options.len()));
    format!(
        "{} {} {:?} {pending:?} {cards:?} {lasting:?}",
        st.turn(),
        st.active(),
        st.winner()
    )
}

/// The successor of `a` at `st`, with a multi-card selection the move starts finished at once
/// (DONE while the same player's SELECT offers it): the engine offers the later candidates
/// only after an earlier one (combinations, not permutations), so picking the first or the
/// last of two interchangeable copies first differs only in which copy the finished selection
/// can add.
fn successor(st: &GameState, a: Action) -> GameState {
    let mut x = st.clone();
    apply(&mut x, a, true).expect("apply");
    while let Some(d) = x.pending() {
        let done = d
            .options
            .iter()
            .copied()
            .find(|o| o.kind == ActionKind::Done);
        match done {
            Some(done)
                if d.hdr.kind == gcg_engine::DecisionKind::Select
                    && d.hdr.player == st.pending().expect("a decision").hdr.player =>
            {
                apply(&mut x, done, true).expect("apply");
            }
            _ => break,
        }
    }
    x
}

/// W-10's property: over the states of 1,000 random games on the AI test decks, every two
/// moves the typed merge puts in one group lead to successor states with equal uid-free
/// summaries (`successor`).
#[test]
fn moves_the_typed_merge_groups_lead_to_equivalent_successors() {
    let (mut pairs, mut extra) = (0usize, 0usize);
    for game in 0..1000u64 {
        let mut st = test_game(game);
        let mut rng = SplitMix64::new(game);
        for _ in 0..300 {
            let Some(dec) = st.pending() else { break };
            if st.winner().is_some() {
                break;
            }
            let moves = canonical(&st, &dec);
            if moves.len() > 1 {
                let groups = super::fingerprint::groups(&st, &dec, &moves).expect("groups");
                let reference = merge_equivalent(&st, &dec, moves.clone()).expect("merge");
                for (i, &g) in groups.iter().enumerate() {
                    if g == i {
                        continue;
                    }
                    pairs += 1;
                    extra += usize::from(reference.iter().any(|(k, _)| *k == moves[i].0));
                    let x = successor(&st, moves[g].1);
                    let y = successor(&st, moves[i].1);
                    assert_eq!(
                        summary(&x),
                        summary(&y),
                        "game {game}: {:?} and {:?}",
                        moves[g].1,
                        moves[i].1
                    );
                }
            }
            let n = dec.options.len();
            let a = dec.options[rng.randrange_usize(n)];
            apply(&mut st, a, true).expect("apply");
        }
    }
    eprintln!("W-10 property: {pairs} merged pairs, {extra} of them merged only by the typed form");
    assert!(pairs > 0);
}
