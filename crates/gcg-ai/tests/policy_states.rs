//! The playout policy, the evaluation and the random player on decision states sampled from
//! the tier-1 game traces (`states-*.ndjson` of `oracle/export_policy_vectors.py`), bit
//! for bit against Python: `option_scores` for the baseline and the adopted knobs, `choose`
//! with `rng=None` and seeded (with the generator state after), `burst_density` for both
//! observers, `features`, `role_features`, `score` and `evaluate` for both players, the choice
//! polarity, and one `RandomAgent` decision per state. `terminal` records are finished games.
//!
//! Each state is rebuilt with `canon::from_canonical`, whose digest must equal Python's, so it
//! starts with an empty view cache as Python's exporter did. The scorers, the features and
//! `burst_density` call the engine's derived view and `observe::unknown_pools` (B2, C) and
//! D2's `attack_can_matter` and `AgentCore::begin`.

mod policy_support;

use gcg_ai::targeting::{choice_polarity, choice_rule_lasts_this_battle};
use gcg_ai::{
    Agent, AgentCx, BASELINE_KNOBS, HAND_WEIGHTS, NoTrace, PolicyKnobs, ROLE_WEIGHTS, RandomAgent,
    Scratch, TUNED_WEIGHTS, Weights, burst_density, choose, evaluate, features, option_scores,
    role_features, score,
};
use gcg_core::pool::{Cancel, Pool, TaskCx};
use gcg_engine::GameState;
use policy_support::{
    Mismatches, action, action_value, f, floats, registry, same, same_floats, state_of,
    state_records, u64_of,
};
use serde_json::Value;

/// The header and the records of the states files.
fn load() -> (Value, Vec<Value>) {
    let mut recs = state_records();
    let header = recs
        .iter()
        .find(|r| r["t"] == "header")
        .expect("a header")
        .clone();
    recs.retain(|r| r["t"] != "header");
    (header, recs)
}

fn knobs(v: &Value) -> PolicyKnobs {
    PolicyKnobs {
        burst_yes_score: f(&v["burst_yes_score"]),
        rule_polarity_table: v["rule_polarity_table"].as_bool().unwrap(),
        zero_ap_face: v["zero_ap_face"].as_bool().unwrap(),
    }
}

fn id(r: &Value) -> String {
    r["id"].as_str().unwrap().to_owned()
}

/// Every state rebuilds from its snapshot with Python's `state_digest` and pending decision.
#[test]
fn sampled_states_rebuild_with_pythons_digest() {
    let reg = registry();
    let (_, recs) = load();
    let mut m = Mismatches::default();
    let (mut states, mut terminals) = (0, 0);
    for r in &recs {
        let st = state_of(&reg, r);
        if r["t"] == "terminal" {
            terminals += 1;
            m.check(&id(r), "no decision", same(st.pending().is_none(), true));
            continue;
        }
        states += 1;
        let dec = st.pending().expect("a pending decision");
        let options: Vec<Value> = dec.options.iter().map(|&a| action_value(a)).collect();
        m.check(
            &id(r),
            "options",
            same(&Value::from(options), &r["options"]),
        );
        m.check(
            &id(r),
            "kind",
            same(dec.hdr.kind.as_str(), r["kind"].as_str().unwrap()),
        );
    }
    assert!(
        states >= 300 && terminals >= 10,
        "{states} states, {terminals} terminals"
    );
    m.assert_none(2 * states + terminals);
}

#[test]
fn option_scores_match_python() {
    let reg = registry();
    let (header, recs) = load();
    let adopted = knobs(&header["adopted"]);
    assert_eq!(knobs(&header["baseline"]), BASELINE_KNOBS);
    let mut m = Mismatches::default();
    for r in recs.iter().filter(|r| r["t"] == "state") {
        let st = state_of(&reg, r);
        let dec = st.pending().unwrap();
        for (name, k) in [("scores_base", BASELINE_KNOBS), ("scores_adopted", adopted)] {
            let got = option_scores(&st, &dec, dec.options, &k);
            m.check(
                &id(r),
                name,
                got.map_err(|e| e.to_string())
                    .and_then(|g| same_floats(&g, &floats(&r[name]))),
            );
        }
    }
    m.assert_none(600);
}

#[test]
fn choose_matches_python_with_and_without_a_generator() {
    let reg = registry();
    let (header, recs) = load();
    let adopted = knobs(&header["adopted"]);
    let temps = [
        f(&header["base_temperature"]),
        f(&header["adopted_temperature"]),
    ];
    let mut m = Mismatches::default();
    for r in recs.iter().filter(|r| r["t"] == "state") {
        let st = state_of(&reg, r);
        for (i, (name, k)) in [("base", BASELINE_KNOBS), ("adopted", adopted)]
            .into_iter()
            .enumerate()
        {
            let got = choose(&st, None, 0.0, &k).map_err(|e| e.to_string());
            m.check(
                &id(r),
                &format!("choose_{name}"),
                got.and_then(|a| same(a, action(&r[format!("choose_{name}")]))),
            );
            let want = &r["sample"][name];
            let mut rng = gcg_core::rng::SplitMix64::new(u64_of(&r["sample"]["seed"]));
            let got = choose(&st, Some(&mut rng), temps[i], &k).map_err(|e| e.to_string());
            m.check(
                &id(r),
                &format!("sampled choose {name}"),
                got.and_then(|a| {
                    same(
                        (a, rng.state),
                        (action(&want["chosen"]), u64_of(&want["rng_after"])),
                    )
                }),
            );
        }
    }
    m.assert_none(1200);
}

/// `burst_density`, `features`, `role_features`, `score` (three weight tables) and `evaluate`
/// (two) for both players, each with its own observer's density; decision and terminal states.
#[test]
fn evaluation_matches_python() {
    let reg = registry();
    let (_, recs) = load();
    let mut m = Mismatches::default();
    let tables: [(&str, &Weights); 3] = [
        ("roles", &ROLE_WEIGHTS),
        ("tuned", &TUNED_WEIGHTS),
        ("hand", &HAND_WEIGHTS),
    ];
    for r in &recs {
        let st = state_of(&reg, r);
        let id = id(r);
        for p in 0..2u8 {
            let pi = usize::from(p);
            let dens = burst_density(&st, p);
            m.check(
                &id,
                &format!("burst_density({p})"),
                same_floats(&dens, &floats(&r["density"][pi])),
            );
            let check = |m: &mut Mismatches, what: &str, got: Result<Vec<f64>, _>, want: &Value| {
                m.check(
                    &id,
                    what,
                    got.map_err(|e: gcg_engine::EngineError| e.to_string())
                        .and_then(|g| same_floats(&g, &floats(want))),
                );
            };
            check(
                &mut m,
                &format!("features({p})"),
                features(&st, p, dens),
                &r["features"][pi],
            );
            check(
                &mut m,
                &format!("role_features({p})"),
                role_features(&st, dens),
                &r["roles"][pi],
            );
            for (name, w) in tables {
                check(
                    &mut m,
                    &format!("score {name} ({p})"),
                    score(&st, p, w, dens).map(|x| vec![x]),
                    &Value::from(vec![r["score"][name][pi].clone()]),
                );
                if name != "hand" {
                    check(
                        &mut m,
                        &format!("evaluate {name} ({p})"),
                        evaluate(&st, p, w, dens).map(|x| vec![x]),
                        &Value::from(vec![r["evaluate"][name][pi].clone()]),
                    );
                }
            }
        }
    }
    m.assert_none(16 * 300);
}

/// A finished game evaluates to exactly 1.0, 0.0 or 0.5 (`ai/evaluation.py:322-325`).
#[test]
fn finished_games_evaluate_exactly() {
    let reg = registry();
    let (_, recs) = load();
    let mut n = 0;
    for r in recs.iter().filter(|r| r["t"] == "terminal") {
        let st = state_of(&reg, r);
        let winner = st.winner().expect("a finished game");
        assert_eq!(Some(i64::from(winner)), r["winner"].as_i64(), "{}", id(r));
        for p in 0..2u8 {
            let want = match i32::from(winner) {
                w if w == i32::from(p) => 1.0,
                w if w == 1 - i32::from(p) => 0.0,
                _ => 0.5,
            };
            let got = evaluate(&st, p, &ROLE_WEIGHTS, [0.25, 0.25]).unwrap();
            assert_eq!(got.to_bits(), f64::to_bits(want), "{} player {p}", id(r));
        }
        n += 1;
    }
    assert!(n >= 10, "only {n} finished games");
}

/// `choice_polarity` with and without the rule table, and the battle-only flag, from the
/// waiting frame of each state (`ai/targeting.py:58-90`).
#[test]
fn choice_polarity_matches_python() {
    let reg = registry();
    let (_, recs) = load();
    let mut m = Mismatches::default();
    let mut waiting = 0;
    for r in recs.iter().filter(|r| r["t"] == "state") {
        let st = state_of(&reg, r);
        if st.frames().last().is_some_and(|fr| fr.waiting) {
            waiting += 1;
        }
        let got = [
            i64::from(choice_polarity(&st, false).value()),
            i64::from(choice_polarity(&st, true).value()),
        ];
        let want = [
            r["polarity"][0].as_i64().unwrap(),
            r["polarity"][1].as_i64().unwrap(),
        ];
        m.check(&id(r), "polarity", same(got, want));
        m.check(
            &id(r),
            "lasts this battle",
            same(
                choice_rule_lasts_this_battle(&st),
                r["battle"].as_bool().unwrap(),
            ),
        );
    }
    assert!(waiting >= 50, "only {waiting} states with a waiting frame");
    m.assert_none(600);
}

/// One `RandomAgent` decision per state: `options[randrange(n)]` from
/// `derive_seed(seed, "random", "decision", 1)`, a draw even for one option, `1.0 / n` shares
/// with 0 visits (`ai/agents.py:28-32`).
#[test]
fn random_agent_matches_python() {
    let reg = registry();
    let (_, recs) = load();
    let mut m = Mismatches::default();
    for r in recs.iter().filter(|r| r["t"] == "state") {
        let st = state_of(&reg, r);
        let want = &r["random"];
        let mut agent = RandomAgent::new(u64_of(&want["seed"]), false);
        let player = u8::try_from(r["player"].as_u64().unwrap()).unwrap();
        let got = run(&st, |cx| agent.choose_traced(&st, player, cx, &mut NoTrace));
        let id = id(r);
        match got {
            Err(e) => m.check(&id, "random agent", Err(e)),
            Ok((chosen, trace)) => {
                let trace = trace.expect("a trace");
                m.check(&id, "chosen", same(chosen, action(&want["chosen"])));
                m.check(&id, "k", same(trace.k, 1));
                m.check(
                    &id,
                    "seed",
                    same(trace.seed, u64_of(&want["decision_seed"])),
                );
                m.check(
                    &id,
                    "rng after",
                    same(trace.rng_after, u64_of(&want["rng_after"])),
                );
                let n = st.pending().unwrap().options.len();
                let shares: Vec<f64> = trace.alternatives.iter().map(|a| a.value).collect();
                m.check(
                    &id,
                    "shares",
                    same_floats(&shares, &vec![1.0 / n as f64; n]),
                );
                m.check(
                    &id,
                    "alternatives",
                    same(
                        trace
                            .alternatives
                            .iter()
                            .map(|a| (a.action, a.visits))
                            .collect::<Vec<_>>(),
                        st.pending()
                            .unwrap()
                            .options
                            .iter()
                            .map(|&a| (a, 0))
                            .collect(),
                    ),
                );
                m.check(&id, "no search", same(trace.search.is_none(), true));
            }
        }
    }
    m.assert_none(7 * 300);
}

/// Runs one agent call with an `AgentCx` built outside the pool (docs/dev/contracts.md,
/// gcg-ai section).
fn run<T>(
    _st: &GameState,
    f: impl FnOnce(&mut AgentCx<'_>) -> Result<T, gcg_ai::AgentError>,
) -> Result<T, String> {
    let cancel = Cancel::new();
    Pool::run_inline(|scratch: &mut Scratch, par: &TaskCx<'_, Scratch>| {
        let mut cx = AgentCx {
            scratch,
            par,
            cancel: &cancel,
        };
        f(&mut cx)
    })
    .map_err(|e| format!("{e:?}"))
}
