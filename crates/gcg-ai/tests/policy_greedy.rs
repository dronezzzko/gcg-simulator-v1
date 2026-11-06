//! `GreedyAgent` decisions on the sampled states of `states-*.ndjson`
//! (`oracle/export_policy_vectors.py`), against Python's chosen action, decision seed,
//! generator state after the decision and every alternative (action, value bit for bit,
//! visits), unranked and unrounded (`ai/agents.py:48-66`).
//!
//! Needs the engine's `observe::determinize` and `apply` (C, L5), the derived view (B2) and
//! D2's `canonical`, `prune_dominated`, `mulligan_choice` and `AgentCore::{begin, finish}`.

mod policy_support;

use gcg_ai::{
    Agent, AgentCx, AgentError, DEFAULT_WEIGHTS, DecisionTrace, GreedyAgent, NoTrace, Scratch,
};
use gcg_core::pool::{Cancel, Pool, TaskCx};
use gcg_engine::Action;
use gcg_engine::canon::state_digest;
use policy_support::{Mismatches, action, f, registry, same, state_of, state_records, u64_of};
use serde_json::Value;

fn decide(
    agent: &mut GreedyAgent,
    st: &gcg_engine::GameState,
    player: u8,
) -> Result<(Action, Option<DecisionTrace>), AgentError> {
    let cancel = Cancel::new();
    Pool::run_inline(|scratch: &mut Scratch, par: &TaskCx<'_, Scratch>| {
        let mut cx = AgentCx {
            scratch,
            par,
            cancel: &cancel,
        };
        agent.choose_traced(st, player, &mut cx, &mut NoTrace)
    })
}

#[test]
fn greedy_decisions_match_python() {
    let reg = registry();
    let recs = state_records();
    let header = recs.iter().find(|r| r["t"] == "header").expect("a header");
    assert_eq!(
        header["greedy_weights"].as_str(),
        Some(DEFAULT_WEIGHTS.feature_set().as_str())
    );
    let mut m = Mismatches::default();
    let mut kinds: Vec<String> = Vec::new();
    for r in recs
        .iter()
        .filter(|r| r["t"] == "state" && !r["greedy"].is_null())
    {
        let id = r["id"].as_str().unwrap();
        let st = state_of(&reg, r);
        let before = state_digest(&st);
        let want = &r["greedy"];
        let player = u8::try_from(r["player"].as_u64().unwrap()).unwrap();
        let mut agent = GreedyAgent::new(u64_of(&want["seed"]), false, DEFAULT_WEIGHTS.clone());
        let kind = r["kind"].as_str().unwrap().to_owned();
        if !kinds.contains(&kind) {
            kinds.push(kind);
        }
        match decide(&mut agent, &st, player) {
            Err(e) => m.check(id, "greedy", Err(format!("{e:?}"))),
            Ok((chosen, trace)) => {
                let trace = trace.expect("a trace");
                m.check(id, "chosen", same(chosen, action(&want["chosen"])));
                m.check(id, "k", same(trace.k, 1));
                m.check(id, "seed", same(trace.seed, u64_of(&want["decision_seed"])));
                m.check(
                    id,
                    "rng after",
                    same(trace.rng_after, u64_of(&want["rng_after"])),
                );
                m.check(id, "no search", same(trace.search.is_none(), true));
                m.check(
                    id,
                    "alternatives",
                    alternatives(&trace, &want["alternatives"]),
                );
            }
        }
        m.check(id, "true state unchanged", same(state_digest(&st), before));
    }
    assert!(kinds.len() >= 6, "greedy vectors cover only {kinds:?}");
    m.assert_none(7 * 80);
}

fn alternatives(trace: &DecisionTrace, want: &Value) -> Result<(), String> {
    let want: Vec<(Action, u64, u32)> = want
        .as_array()
        .unwrap()
        .iter()
        .map(|a| {
            (
                action(&a[0]),
                f(&a[1]).to_bits(),
                u32::try_from(a[2].as_u64().unwrap()).unwrap(),
            )
        })
        .collect();
    let got: Vec<(Action, u64, u32)> = trace
        .alternatives
        .iter()
        .map(|a| (a.action, a.value.to_bits(), a.visits))
        .collect();
    same(got, want)
}
