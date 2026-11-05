//! `tests/ai/test_baseline.py`, ported: the `baseline-standard` configuration replays the game
//! recorded at commit bc38ab4 (Inv 07 §17). `new_game(DECKS, 4242)`, agents
//! `make_agent("mcts", seed=17+seat, log=True, config=BASELINE_PRESETS["standard"],
//! weights=BASELINE_WEIGHTS)`, every decision applied with `check=True`; the outcome is
//! `(winner, turn, len(actions)) == (1, 20, 115)` and the SHA-256 of
//! `json.dumps({"actions", "logs", "winner", "turn"}, sort_keys=True)` (default separators,
//! `ensure_ascii`) is the recorded digest. The decks, seeds and constants come from
//! `baseline.json`, which the exporter writes after replaying the game in Python and checking
//! the test's constants.

mod common;

use gcg_ai::{Agent, AgentCx, Scratch};
use gcg_ai_reference::{BASELINE_STANDARD, BASELINE_WEIGHTS, make_reference_agent};
use gcg_core::pool::{Cancel, Pool, TaskCx};
use gcg_core::pyjson::{DumpOptions, Json, dumps};
use gcg_effects::Registry;
use gcg_engine::{NewGameOptions, apply, new_game_by_number};
use sha2::{Digest, Sha256};

use common::{int, json, u64_of};

/// `json.dumps(payload, sort_keys=True)` with Python's default separators.
const PY_SORTED: DumpOptions = DumpOptions {
    indent: None,
    sort_keys: true,
    compact: false,
    ensure_ascii: true,
};

#[test]
fn baseline_standard_replays_the_game_recorded_before_the_changes() {
    let spec = json("baseline.json");
    assert_eq!(spec["config"], "baseline-standard");
    assert_eq!(
        spec["digest"],
        "5cff789d2a8bb26cfef4db98b1ef1ef8f37c3bf63210114781d357bd215f3b96"
    );
    let numbers = |deck: &serde_json::Value, part: &str| -> Vec<String> {
        deck[part]
            .as_array()
            .expect("card numbers")
            .iter()
            .map(|n| n.as_str().expect("a card number").to_owned())
            .collect()
    };
    let decks: Vec<(Vec<String>, Vec<String>)> = spec["decks"]
        .as_array()
        .expect("two decks")
        .iter()
        .map(|d| (numbers(d, "main"), numbers(d, "res")))
        .collect();
    let refs: Vec<(Vec<&str>, Vec<&str>)> = decks
        .iter()
        .map(|(m, r)| {
            (
                m.iter().map(String::as_str).collect(),
                r.iter().map(String::as_str).collect(),
            )
        })
        .collect();
    let reg = Registry::embedded();
    let mut st = new_game_by_number(
        &reg,
        [
            (&refs[0].0[..], &refs[0].1[..]),
            (&refs[1].0[..], &refs[1].1[..]),
        ],
        u64_of(&spec["game_seed"]),
        NewGameOptions::default(),
    )
    .expect("the test decks are legal");
    let seeds = spec["agent_seeds"].as_array().expect("two seeds");
    let mut agents: Vec<Box<dyn Agent>> = seeds
        .iter()
        .map(|s| make_reference_agent(&BASELINE_STANDARD, u64_of(s), true, BASELINE_WEIGHTS))
        .collect();
    let cancel = Cancel::new();
    let mut actions = Vec::new();
    while let Some(dec) = st.pending() {
        let p = dec.hdr.player;
        let action = Pool::run_inline(|scratch: &mut Scratch, par: &TaskCx<'_, Scratch>| {
            let mut cx = AgentCx {
                scratch,
                par,
                cancel: &cancel,
            };
            agents[usize::from(p)].choose(&st, p, &mut cx)
        })
        .unwrap_or_else(|e| panic!("decision {}: {e}", actions.len()));
        actions.push(action.to_json());
        apply(&mut st, action, true).unwrap_or_else(|e| panic!("apply: {e}"));
    }
    let mut payload = Json::object();
    payload.set("actions", Json::Array(actions.clone()));
    payload.set(
        "logs",
        Json::Array(
            agents
                .iter()
                .map(|a| Json::Array(a.decision_log().iter().map(|e| e.to_json()).collect()))
                .collect(),
        ),
    );
    payload.set("winner", st.winner().map(i64::from));
    payload.set("turn", st.turn());
    let text = dumps(&payload, PY_SORTED);
    let digest: String = Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    assert_eq!(
        (
            st.winner().map(i64::from),
            i64::from(st.turn()),
            actions.len()
        ),
        (
            Some(int(&spec["winner"])),
            int(&spec["turn"]),
            usize::try_from(int(&spec["actions"])).expect("a count")
        )
    );
    assert_eq!(digest, spec["digest"].as_str().expect("a digest"));
}
