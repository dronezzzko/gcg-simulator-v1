//! The reference search against Python's `search()` on the states of `decisions.ndjson`
//! (`port/oracle/export_ai_vectors.py`; PLAN §10.4 D2 gate "every root-function vector equal"):
//! for `fast` at every searched decision and `standard`, `strong` and `baseline-standard` at a
//! sample, the action, every alternative bit for bit, the iteration count and the per-decision
//! generator's state afterwards (seeds drawn up front, Inv 07 §3.3). The agent level is checked
//! on the decisions an agent reaches within its first three decisions: the action, the
//! `DecisionTrace` and the decision-log bytes.

mod common;

use gcg_ai::{Agent, AgentCx, NoTrace, Scratch};
use gcg_ai_reference::{FAST, ReferenceAgent, search};
use gcg_core::pool::{Cancel, Pool, TaskCx};
use gcg_core::pyjson::{DumpOptions, dumps};
use gcg_core::rng::SplitMix64;
use gcg_effects::Registry;

use common::{action, alternatives, bits, config, int, records, state, u64_of};

#[test]
fn searches_match_python_bit_for_bit() {
    let reg = Registry::embedded();
    let cancel = Cancel::new();
    let mut checked = [0usize; 4];
    for rec in records() {
        let Some(searches) = rec["search"].as_array() else {
            continue;
        };
        let st = state(&reg, &rec);
        let player = u8::try_from(int(&rec["player"])).expect("a seat");
        for want in searches {
            let label = want["config"].as_str().expect("a configuration label");
            let (cfg, weights) = config(label);
            let mut rng = SplitMix64::new(u64_of(&rec["seed"]));
            let result = Pool::run_inline(|scratch: &mut Scratch, par: &TaskCx<'_, Scratch>| {
                let mut cx = AgentCx {
                    scratch,
                    par,
                    cancel: &cancel,
                };
                search(&st, player, cfg, weights, &mut rng, &mut cx, &mut NoTrace)
            })
            .unwrap_or_else(|e| panic!("{} {label}: {e}", rec["id"]));
            let id = format!("{} {label}", rec["id"]);
            assert_eq!(result.action, action(&want["action"]), "{id}");
            assert_eq!(
                bits(&result.alternatives),
                alternatives(&want["alts"]),
                "{id}"
            );
            assert_eq!(
                i64::from(result.stats.iterations),
                int(&want["iterations"]),
                "{id}"
            );
            assert_eq!(rng.state, u64_of(&want["rng_after"]), "{id}");
            if result.alternatives.is_empty() {
                assert_eq!(result.stats.roots.len(), 1, "{id}");
            } else {
                assert_eq!(result.stats.roots.len(), result.alternatives.len(), "{id}");
                assert!(result.stats.iterations <= result.stats.cap, "{id}");
            }
            let slot = ["fast", "standard", "strong", "baseline-standard"]
                .iter()
                .position(|l| *l == label)
                .expect("a known label");
            checked[slot] += 1;
        }
    }
    assert!(checked.iter().all(|&n| n > 0), "{checked:?}");
}

#[test]
fn early_agent_decisions_match_python_with_trace_and_log() {
    let reg = Registry::embedded();
    let cancel = Cancel::new();
    let mut checked = 0;
    for rec in records() {
        let k = u64_of(&rec["k"]);
        if k > 3 {
            continue;
        }
        let st = state(&reg, &rec);
        let player = u8::try_from(int(&rec["player"])).expect("a seat");
        let mut agent = ReferenceAgent::new(
            FAST.clone(),
            u64_of(&rec["agent_seed"]),
            true,
            gcg_ai::DEFAULT_WEIGHTS.clone(),
        );
        let mut last = None;
        for _ in 0..k {
            last = Some(
                Pool::run_inline(|scratch: &mut Scratch, par: &TaskCx<'_, Scratch>| {
                    let mut cx = AgentCx {
                        scratch,
                        par,
                        cancel: &cancel,
                    };
                    agent.choose_traced(&st, player, &mut cx, &mut NoTrace)
                })
                .unwrap_or_else(|e| panic!("{}: {e}", rec["id"])),
            );
        }
        let (chosen, trace) = last.expect("k >= 1");
        let trace = trace.expect("the reference agent reports a trace");
        let id = &rec["id"];
        assert_eq!(chosen, action(&rec["chosen"]), "{id}");
        assert_eq!(trace.k, k, "{id}");
        assert_eq!(trace.seed, u64_of(&rec["seed"]), "{id}");
        assert_eq!(trace.rng_after, u64_of(&rec["rng_after"]), "{id}");
        assert_eq!(trace.search.is_some(), rec["search"].is_array(), "{id}");
        let entry = agent.decision_log().last().expect("logged");
        assert_eq!(
            dumps(&entry.to_json(), DumpOptions::COMPACT_SORTED_ASCII),
            rec["log_text"].as_str().expect("log_text"),
            "{id}"
        );
        assert_eq!(
            agent.decision_log().len(),
            usize::try_from(k).expect("small")
        );
        checked += 1;
    }
    assert!(checked >= 12, "{checked}");
}
