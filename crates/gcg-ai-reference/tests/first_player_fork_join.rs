//! T-069: the play/draw choice (`first_player_choice`, `ai/setup.py:86-111`) run as a
//! fork-join on a real worker pool equals the serial computation, bit for bit, and Python's.
//!
//! `first_player_choice` hands its `2 x games` policy-vs-policy simulations to
//! `cx.par.fork_join`. A pool runs `min(threads, tasks)` workers and a one-worker run executes
//! every job inline (`gcg_core::pool`), so a single task on a large pool never forks in
//! parallel. Each parallel run here therefore has at least two tasks: the CHOOSE_FIRST
//! decisions and trivial filler tasks, so that workers without a decision of their own take the
//! queued simulations of the others. The decisions are made through the reference agent
//! (`MctsAgent.choose`, `ai/agents.py:93-96`) and through `first_player_choice` directly, for
//! the three reference presets (8, 24 and 48 games), at 2, 4 and 10 workers; every result must
//! equal the inline run (`Pool::run_inline`). The four CHOOSE_FIRST decisions the Python AI
//! tests make (`test_puzzles.py:254-260` for seeds 1 and 2, `test_agents.py:42-52` and
//! `:172-181`) are also checked against their records in `crates/gcg-ai/tests/vectors/ai_tests/`.

mod common;

use std::fs;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::Arc;

use gcg_ai::{AgentCx, Alternative, DEFAULT_WEIGHTS, NoTrace, Scratch, first_player_choice};
use gcg_ai_reference::{FAST, ReferenceConfig, STANDARD, STRONG, make_reference_agent};
use gcg_core::pool::{Cancel, Pool, TaskCx};
use gcg_core::pyjson::{DumpOptions, dumps};
use gcg_core::rng::SplitMix64;
use gcg_effects::Registry;
use gcg_engine::canon::state_digest;
use gcg_engine::{Action, DecisionKind, GameState, NewGameOptions, new_game_by_number};
use serde_json::Value;

use common::{action, alternatives, bits, int, u64_of};

const PY_SORTED: DumpOptions = DumpOptions {
    indent: None,
    sort_keys: true,
    compact: false,
    ensure_ascii: true,
};

fn ai_tests() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../gcg-ai/tests/vectors/ai_tests")
}

/// `DECKS` of `tests/ai/decks.py`, as `midgame.json` records them.
fn decks() -> Vec<(Vec<String>, Vec<String>)> {
    let text = fs::read_to_string(ai_tests().join("midgame.json")).expect("midgame.json");
    let v: Value = serde_json::from_str(&text).expect("JSON");
    let part = |d: &Value, k: &str| -> Vec<String> {
        d[k].as_array()
            .expect("numbers")
            .iter()
            .map(|n| n.as_str().expect("a number").to_owned())
            .collect()
    };
    v["decks"]
        .as_array()
        .expect("decks")
        .iter()
        .map(|d| (part(d, "main"), part(d, "res")))
        .collect()
}

/// `new_game(DECKS, seed)`: pending CHOOSE_FIRST.
fn new_game(reg: &Arc<Registry>, decks: &[(Vec<String>, Vec<String>)], seed: u64) -> GameState {
    let refs: Vec<(Vec<&str>, Vec<&str>)> = decks
        .iter()
        .map(|(m, r)| {
            (
                m.iter().map(String::as_str).collect(),
                r.iter().map(String::as_str).collect(),
            )
        })
        .collect();
    let st = new_game_by_number(
        reg,
        [
            (&refs[0].0[..], &refs[0].1[..]),
            (&refs[1].0[..], &refs[1].1[..]),
        ],
        seed,
        NewGameOptions::default(),
    )
    .expect("legal decks");
    assert_eq!(
        st.pending().map(|d| d.hdr.kind),
        Some(DecisionKind::ChooseFirst)
    );
    st
}

/// One CHOOSE_FIRST decision: the game seed, the configuration, the agent seed and logging.
#[derive(Clone)]
struct Case {
    game_seed: u64,
    cfg: &'static ReferenceConfig,
    agent_seed: u64,
    log: bool,
}

/// What a decision computed: the action, every alternative bit for bit, the per-decision
/// generator afterwards and the log entries written.
type Outcome = (Action, Vec<(Action, u64, u32)>, u64, Vec<String>);

/// The agent's decision (`MctsAgent.choose` at CHOOSE_FIRST).
fn agent_decision(
    reg: &Arc<Registry>,
    decks: &[(Vec<String>, Vec<String>)],
    case: &Case,
    cx: &mut AgentCx<'_>,
) -> Outcome {
    let st = new_game(reg, decks, case.game_seed);
    let p = st.pending().expect("a decision").hdr.player;
    let mut agent = make_reference_agent(case.cfg, case.agent_seed, case.log, DEFAULT_WEIGHTS);
    let (a, trace) = agent
        .choose_traced(&st, p, cx, &mut NoTrace)
        .unwrap_or_else(|e| panic!("game {}: {e}", case.game_seed));
    let trace = trace.expect("a decision trace");
    assert!(trace.search.is_none());
    let log = agent
        .decision_log()
        .iter()
        .map(|e| dumps(&e.to_json(), PY_SORTED))
        .collect();
    (a, bits(&trace.alternatives), trace.rng_after, log)
}

/// `first_player_choice(st, player, cfg.first_player_games, SplitMix64(seed), knobs_for(cfg))`.
fn direct_decision(
    reg: &Arc<Registry>,
    decks: &[(Vec<String>, Vec<String>)],
    case: &Case,
    cx: &mut AgentCx<'_>,
) -> Outcome {
    let st = new_game(reg, decks, case.game_seed);
    let p = st.pending().expect("a decision").hdr.player;
    let mut rng = SplitMix64::new(case.agent_seed);
    let (a, alts): (Action, Vec<Alternative>) = first_player_choice(
        &st,
        p,
        case.cfg.first_player_games,
        &mut rng,
        &case.cfg.knobs(),
        cx,
    )
    .unwrap_or_else(|e| panic!("game {}: {e}", case.game_seed));
    (a, bits(&alts), rng.state, Vec::new())
}

type Decide = fn(&Arc<Registry>, &[(Vec<String>, Vec<String>)], &Case, &mut AgentCx<'_>) -> Outcome;

fn serial(
    reg: &Arc<Registry>,
    decks: &[(Vec<String>, Vec<String>)],
    case: &Case,
    decide: Decide,
) -> Outcome {
    let cancel = Cancel::new();
    Pool::run_inline(|scratch: &mut Scratch, par: &TaskCx<'_, Scratch>| {
        let mut cx = AgentCx {
            scratch,
            par,
            cancel: &cancel,
        };
        decide(reg, decks, case, &mut cx)
    })
}

/// The decisions of `cases` on a pool of `workers`, as tasks `0..cases.len()`, followed by
/// filler tasks up to `workers` tasks in all, which end at once and leave their workers free
/// for the queued simulations.
fn on_pool(
    reg: &Arc<Registry>,
    decks: &[(Vec<String>, Vec<String>)],
    cases: &[Case],
    workers: usize,
    decide: Decide,
) -> Vec<Outcome> {
    let cancel = Cancel::new();
    let tasks = cases.len().max(workers);
    assert!(tasks >= 2, "a parallel run needs two tasks");
    let pool = Pool::new(NonZeroUsize::new(workers).expect("workers"));
    let out = pool
        .map_indexed(
            tasks,
            &cancel,
            |i, scratch: &mut Scratch, par: &TaskCx<'_, Scratch>| {
                let mut cx = AgentCx {
                    scratch,
                    par,
                    cancel: &cancel,
                };
                Ok::<_, ()>(cases.get(i).map(|case| decide(reg, decks, case, &mut cx)))
            },
            |_| {},
        )
        .unwrap_or_else(|_| panic!("the pool run failed"));
    out.into_iter().flatten().collect()
}

fn cases() -> Vec<Case> {
    let mut out = Vec::new();
    for (i, cfg) in [&FAST, &STANDARD, &STRONG].into_iter().enumerate() {
        for game_seed in [5, 6, 42] {
            out.push(Case {
                game_seed,
                cfg,
                agent_seed: 1000 + 10 * u64::try_from(i).expect("small") + game_seed,
                log: game_seed == 42,
            });
        }
    }
    out
}

#[test]
fn fork_join_play_draw_choices_equal_the_serial_computation() {
    let reg = Registry::embedded();
    let decks = decks();
    let cases = cases();
    for decide in [agent_decision as Decide, direct_decision as Decide] {
        let want: Vec<Outcome> = cases
            .iter()
            .map(|c| serial(&reg, &decks, c, decide))
            .collect();
        for workers in [2, 4, 10] {
            // Each decision alone with idle workers beside it ...
            for (case, w) in cases.iter().zip(&want) {
                let got = on_pool(&reg, &decks, std::slice::from_ref(case), workers, decide);
                assert_eq!(&got[0], w, "game {} at {workers} workers", case.game_seed);
            }
            // ... and all of them at once, forking side by side.
            let got = on_pool(&reg, &decks, &cases, workers, decide);
            assert_eq!(got, want, "{workers} workers, all decisions");
        }
    }
}

/// The CHOOSE_FIRST decisions of the Python AI tests, by `(module, test id)`.
const PYTHON: [(&str, &str, bool); 4] = [
    (
        "test_puzzles",
        "test_chooses_to_play_first_with_proactive_decks[1]",
        false,
    ),
    (
        "test_puzzles",
        "test_chooses_to_play_first_with_proactive_decks[2]",
        false,
    ),
    (
        "test_agents",
        "test_agent_returns_a_legal_action[choose_first-mcts]",
        false,
    ),
    (
        "test_agents",
        "test_pre_game_choices_log_their_options[choose_first-mcts]",
        true,
    ),
];

fn python_decision(module: &str, test: &str) -> Value {
    let id = format!("tests/ai/{module}.py::{test}");
    let text = fs::read_to_string(ai_tests().join(format!("{module}.ndjson"))).expect("records");
    text.lines()
        .map(|l| serde_json::from_str::<Value>(l).expect("a record"))
        .find(|r| r["id"] == id.as_str())
        .unwrap_or_else(|| panic!("no record {id}"))["decisions"][0]
        .clone()
}

#[test]
fn fork_join_play_draw_choices_equal_pythons() {
    let reg = Registry::embedded();
    let decks = decks();
    let mut cases = Vec::new();
    let mut want = Vec::new();
    for (module, test, log) in PYTHON {
        let rec = python_decision(module, test);
        assert_eq!(rec["agent"], "mcts-standard", "{test}");
        let st = new_game(&reg, &decks, 5);
        assert_eq!(
            state_digest(&st),
            rec["digest"].as_str().expect("a digest"),
            "{test}"
        );
        assert_eq!(int(&rec["k"]), 1, "{test}");
        let logs: Vec<String> = rec["log"]
            .as_array()
            .expect("log")
            .iter()
            .map(|e| e.as_str().expect("an entry").to_owned())
            .collect();
        want.push((
            action(&rec["action"]),
            alternatives(&rec["alts"]),
            u64_of(&rec["rng_after"]),
            logs,
        ));
        cases.push(Case {
            game_seed: 5,
            cfg: &STANDARD,
            agent_seed: u64_of(&rec["agent_seed"]),
            log,
        });
    }
    let serial_out: Vec<Outcome> = cases
        .iter()
        .map(|c| serial(&reg, &decks, c, agent_decision))
        .collect();
    assert_eq!(serial_out, want, "inline");
    for workers in [2, 4, 10] {
        assert_eq!(
            on_pool(&reg, &decks, &cases, workers, agent_decision),
            want,
            "{workers} workers"
        );
    }
}
