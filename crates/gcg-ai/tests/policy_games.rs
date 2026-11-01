//! Game-level vectors of docs/port/inventory/08-ai-policy-eval.md §11 (items 3-7) over the AI
//! test decks (`tests/ai/decks.py`), from `games.ndjson` of
//! `port/oracle/export_policy_vectors.py`, which reproduces each Inventory digest before it
//! writes:
//!
//! - `RandomAgent(7)` vs `RandomAgent(8)` from `new_game(DECKS, 12345)`: at every decision the
//!   policy scores (baseline and adopted knobs), the density, the features and the scores,
//!   bit for bit, and the state digest and the chosen action;
//! - 20 random games: the SHA-256 of every decision's `json.dumps([kind, base, adopted,
//!   score_roles, score_tuned])`;
//! - policy playouts from `new_game(DECKS, 99)` (`play_to_end` seeded with both knob sets and
//!   argmax, `play_until_turn(3)`);
//! - greedy vs greedy and random vs random through `run_game` (`ai/selfplay.py:60-75`), with
//!   the SHA-256 of `json.dumps([log0, log1])`.
//!
//! Needs the engine (`new_game_by_number`, `apply`, the view, `observe`; C and B2) and D2's
//! agent core, decision log (`LogEntry::to_json`), `canonical`, `prune_dominated` and
//! `mulligan_choice`.

mod policy_support;

use std::sync::Arc;

use gcg_ai::{
    Agent, AgentCx, BASELINE_KNOBS, GreedyAgent, PolicyKnobs, ROLE_WEIGHTS, RandomAgent, Scratch,
    TUNED_WEIGHTS, burst_density, evaluate, features, option_scores, play_to_end, play_until_turn,
    role_features, score,
};
use gcg_core::pool::{Cancel, Pool, TaskCx};
use gcg_core::pyjson::{DumpOptions, Json, dumps};
use gcg_core::rng::SplitMix64;
use gcg_effects::Registry;
use gcg_engine::canon::state_digest;
use gcg_engine::{GameState, NewGameOptions, apply, new_game_by_number};
use policy_support::{
    Mismatches, action, f, floats, py_hex, records, registry, same, same_floats, u64_of,
};
use serde_json::Value;
use sha2::{Digest, Sha256};

/// `json.dumps(x)` with Python's defaults.
const PY_DEFAULT: DumpOptions = DumpOptions {
    indent: None,
    sort_keys: false,
    compact: false,
    ensure_ascii: true,
};

/// `DECISION_LIMIT` of `run_game` (`ai/selfplay.py:22`).
const DECISION_LIMIT: usize = 20_000;

fn games() -> Vec<Value> {
    records("games.ndjson")
}

fn find<'a>(recs: &'a [Value], t: &str) -> &'a Value {
    recs.iter()
        .find(|r| r["t"] == t)
        .unwrap_or_else(|| panic!("no {t} record"))
}

/// The adopted knobs of `knobs_for(PRESETS["standard"])`, as the states files record them.
fn adopted() -> PolicyKnobs {
    let recs = policy_support::state_records();
    let h = &recs.iter().find(|r| r["t"] == "header").unwrap()["adopted"];
    PolicyKnobs {
        burst_yes_score: f(&h["burst_yes_score"]),
        rule_polarity_table: h["rule_polarity_table"].as_bool().unwrap(),
        zero_ap_face: h["zero_ap_face"].as_bool().unwrap(),
    }
}

/// `new_game(DECKS, seed)` with Python's defaults.
fn new_game(reg: &Arc<Registry>, recs: &[Value], seed: u64) -> GameState {
    let decks = &find(recs, "decks")["decks"];
    let strs = |v: &Value| -> Vec<String> {
        v.as_array()
            .unwrap()
            .iter()
            .map(|s| s.as_str().unwrap().to_owned())
            .collect()
    };
    let owned: Vec<(Vec<String>, Vec<String>)> = (0..2)
        .map(|p| (strs(&decks[p]["main"]), strs(&decks[p]["res"])))
        .collect();
    let refs: Vec<(Vec<&str>, Vec<&str>)> = owned
        .iter()
        .map(|(m, r)| {
            (
                m.iter().map(String::as_str).collect(),
                r.iter().map(String::as_str).collect(),
            )
        })
        .collect();
    new_game_by_number(
        reg,
        [
            (refs[0].0.as_slice(), refs[0].1.as_slice()),
            (refs[1].0.as_slice(), refs[1].1.as_slice()),
        ],
        seed,
        NewGameOptions::default(),
    )
    .expect("the AI test decks start a game")
}

fn choose(agent: &mut dyn Agent, st: &GameState, player: u8) -> gcg_engine::Action {
    let cancel = Cancel::new();
    Pool::run_inline(|scratch: &mut Scratch, par: &TaskCx<'_, Scratch>| {
        let mut cx = AgentCx {
            scratch,
            par,
            cancel: &cancel,
        };
        agent.choose(st, player, &mut cx)
    })
    .unwrap_or_else(|e| panic!("{} failed: {e:?}", agent.name()))
}

/// `run_game(agents, decks, seed)` (`ai/selfplay.py:60-75`): each decision is applied with
/// the legality check.
fn run_game(st: &mut GameState, agents: &mut [Box<dyn Agent>; 2]) -> usize {
    let mut decisions = 0;
    while decisions < DECISION_LIMIT {
        let Some(dec) = st.pending() else { break };
        let p = dec.hdr.player;
        let a = choose(agents[usize::from(p)].as_mut(), st, p);
        apply(st, a, true).unwrap_or_else(|e| panic!("apply {a}: {e}"));
        decisions += 1;
    }
    decisions
}

fn hex_list(xs: &[f64]) -> Json {
    Json::Array(xs.iter().map(|&x| Json::from(py_hex(x))).collect())
}

#[test]
fn random_game_records_match_python() {
    let reg = registry();
    let recs = games();
    let want = find(&recs, "random_game");
    let adopted = adopted();
    let mut st = new_game(&reg, &recs, u64_of(&want["seed"]));
    let seeds = &want["agent_seeds"];
    let mut agents = [
        RandomAgent::new(u64_of(&seeds[0]), false),
        RandomAgent::new(u64_of(&seeds[1]), false),
    ];
    let mut m = Mismatches::default();
    let mut n = 0;
    for rec in want["records"].as_array().unwrap() {
        let id = format!("decision {n}");
        let dec = st.pending().expect("a decision");
        let p = dec.hdr.player;
        m.check(
            &id,
            "kind",
            same(dec.hdr.kind.as_str(), rec["kind"].as_str().unwrap()),
        );
        m.check(
            &id,
            "digest16",
            same(&state_digest(&st)[..16], rec["digest16"].as_str().unwrap()),
        );
        let flt = |r: Result<Vec<f64>, gcg_engine::EngineError>, w: &Value| {
            r.map_err(|e| e.to_string())
                .and_then(|g| same_floats(&g, &floats(w)))
        };
        m.check(
            &id,
            "base",
            flt(
                option_scores(&st, &dec, dec.options, &BASELINE_KNOBS),
                &rec["base"],
            ),
        );
        m.check(
            &id,
            "adopted",
            flt(
                option_scores(&st, &dec, dec.options, &adopted),
                &rec["adopted"],
            ),
        );
        let dens = burst_density(&st, p);
        m.check(&id, "density", same_floats(&dens, &floats(&rec["density"])));
        m.check(
            &id,
            "features",
            flt(features(&st, p, dens), &rec["features"]),
        );
        m.check(&id, "roles", flt(role_features(&st, dens), &rec["roles"]));
        let one = |x: Result<f64, gcg_engine::EngineError>, w: &Value| {
            x.map_err(|e| e.to_string())
                .and_then(|g| same_floats(&[g], &[f(w)]))
        };
        m.check(
            &id,
            "score_roles",
            one(score(&st, p, &ROLE_WEIGHTS, dens), &rec["score_roles"]),
        );
        m.check(
            &id,
            "score_tuned",
            one(score(&st, p, &TUNED_WEIGHTS, dens), &rec["score_tuned"]),
        );
        m.check(
            &id,
            "eval_roles",
            one(evaluate(&st, p, &ROLE_WEIGHTS, dens), &rec["eval_roles"]),
        );
        let chosen = choose(&mut agents[usize::from(p)], &st, p);
        m.check(&id, "chosen", same(chosen, action(&rec["chosen"])));
        apply(&mut st, chosen, true).expect("a legal action");
        n += 1;
    }
    assert!(
        st.pending().is_none(),
        "the game continues past Python's last decision"
    );
    assert_eq!(
        n,
        usize::try_from(want["decisions"].as_u64().unwrap()).unwrap()
    );
    assert_eq!(st.winner().map(i64::from), want["winner"].as_i64());
    assert_eq!(i64::from(st.turn()), want["turn"].as_i64().unwrap());
    m.assert_none(11 * 100);
}

#[test]
fn twenty_random_games_hash_like_python() {
    let reg = registry();
    let recs = games();
    let want = find(&recs, "cover_games");
    let adopted = adopted();
    let base_seed = u64_of(&want["seed_base"]);
    let bases = &want["agent_bases"];
    let mut hasher = Sha256::new();
    let mut kinds: Vec<(String, u64)> = Vec::new();
    for g in 0..u64_of(&want["games"]) {
        let mut agents = [
            RandomAgent::new(u64_of(&bases[0]) + g, false),
            RandomAgent::new(u64_of(&bases[1]) + g, false),
        ];
        let mut st = new_game(&reg, &recs, base_seed + g);
        while let Some(dec) = st.pending() {
            let p = dec.hdr.player;
            let dens = burst_density(&st, p);
            let kind = dec.hdr.kind.as_str().to_owned();
            match kinds.iter_mut().find(|(k, _)| *k == kind) {
                Some(e) => e.1 += 1,
                None => kinds.push((kind.clone(), 1)),
            }
            let row = Json::Array(vec![
                Json::from(kind),
                hex_list(&option_scores(&st, &dec, dec.options, &BASELINE_KNOBS).unwrap()),
                hex_list(&option_scores(&st, &dec, dec.options, &adopted).unwrap()),
                Json::from(py_hex(score(&st, p, &ROLE_WEIGHTS, dens).unwrap())),
                Json::from(py_hex(score(&st, p, &TUNED_WEIGHTS, dens).unwrap())),
            ]);
            hasher.update(dumps(&row, PY_DEFAULT).as_bytes());
            let a = choose(&mut agents[usize::from(p)], &st, p);
            apply(&mut st, a, true).expect("a legal action");
        }
    }
    kinds.sort();
    let want_kinds: Vec<(String, u64)> = want["kinds"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| (k.clone(), v.as_u64().unwrap()))
        .collect();
    assert_eq!(kinds, want_kinds);
    let digest: String = hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    assert_eq!(digest, want["digest"].as_str().unwrap());
}

#[test]
fn policy_playouts_match_python() {
    let reg = registry();
    let recs = games();
    let adopted = adopted();
    let mut n = 0;
    for want in recs.iter().filter(|r| r["t"] == "playout") {
        let label = want["label"].as_str().unwrap();
        let mut st = new_game(&reg, &recs, u64_of(&want["game_seed"]));
        let mut rng =
            (!want["rng_seed"].is_null()).then(|| SplitMix64::new(u64_of(&want["rng_seed"])));
        let knobs = if want["adopted"].as_bool().unwrap() {
            adopted
        } else {
            BASELINE_KNOBS
        };
        let max = u32::try_from(want["max_decisions"].as_u64().unwrap()).unwrap();
        let temperature = f(&want["temperature"]);
        let moves = if want["until_turn"].is_null() {
            play_to_end(&mut st, rng.as_mut(), temperature, max, &knobs)
        } else {
            let turn = i32::try_from(want["until_turn"].as_i64().unwrap()).unwrap();
            play_until_turn(&mut st, turn, rng.as_mut(), temperature, max, &knobs)
        }
        .unwrap_or_else(|e| panic!("{label}: {e}"));
        assert_eq!(
            u64::from(moves),
            want["moves"].as_u64().unwrap(),
            "{label}: moves"
        );
        assert_eq!(
            st.winner().map(i64::from),
            want["winner"].as_i64(),
            "{label}: winner"
        );
        assert_eq!(
            i64::from(st.turn()),
            want["turn"].as_i64().unwrap(),
            "{label}: turn"
        );
        assert_eq!(
            state_digest(&st),
            want["final_digest"].as_str().unwrap(),
            "{label}: state"
        );
        if let Some(rng) = rng {
            assert_eq!(rng.state, u64_of(&want["rng_after"]), "{label}: generator");
        }
        if let Some(kind) = want.get("pending").and_then(Value::as_str) {
            assert_eq!(
                st.pending().map(|d| d.hdr.kind.as_str()),
                Some(kind),
                "{label}"
            );
        }
        n += 1;
    }
    assert_eq!(n, 4);
}

fn logs_digest(agents: &[Box<dyn Agent>; 2]) -> String {
    let logs = Json::Array(
        agents
            .iter()
            .map(|a| Json::Array(a.decision_log().iter().map(|e| e.to_json()).collect()))
            .collect(),
    );
    Sha256::digest(dumps(&logs, PY_DEFAULT).as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn check_run(t: &str, make: impl Fn(u64) -> Box<dyn Agent>) {
    let reg = registry();
    let recs = games();
    let want = find(&recs, t);
    let mut st = new_game(&reg, &recs, u64_of(&want["seed"]));
    let seeds = &want["agent_seeds"];
    let mut agents = [make(u64_of(&seeds[0])), make(u64_of(&seeds[1]))];
    let decisions = run_game(&mut st, &mut agents);
    assert_eq!(
        decisions as u64,
        want["decisions"].as_u64().unwrap(),
        "{t}: decisions"
    );
    assert_eq!(
        st.winner().map(i64::from),
        want["winner"].as_i64(),
        "{t}: winner"
    );
    assert_eq!(
        i64::from(st.turn()),
        want["turn"].as_i64().unwrap(),
        "{t}: turn"
    );
    assert_eq!(
        state_digest(&st),
        want["final_digest"].as_str().unwrap(),
        "{t}: final state"
    );
    assert_eq!(
        logs_digest(&agents),
        want["logs_digest"].as_str().unwrap(),
        "{t}: decision logs"
    );
}

/// Inv 08 §11 item 6: 69 decisions, winner 1, turn 11, logs digest `9241c614…`.
#[test]
fn greedy_vs_greedy_matches_python() {
    check_run("greedy_game", |seed| {
        Box::new(GreedyAgent::new(seed, true, ROLE_WEIGHTS.clone()))
    });
}

/// Inv 08 §11 item 7: 99 decisions, winner 1, turn 19.
#[test]
fn random_vs_random_matches_python() {
    check_run("random_vs_random", |seed| {
        Box::new(RandomAgent::new(seed, true))
    });
}
