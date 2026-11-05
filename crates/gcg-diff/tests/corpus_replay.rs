//! The real-engine gates of the tier-1 corpus (PLAN §8.7, §8.8 item 1, §9.2-§9.4; M5.5 and
//! M6): every trace of every set replays against `gcg-engine` and the Rust agents, the
//! negative controls fire against the real engine, the fuzz port reproduces the recorded fuzz
//! games, and the coverage and traceability checks hold. Feature `slow` (A12): they need the
//! engine layers L2-L5 and the agents, and `scripts/verify_all_rust.sh` runs them.
#![cfg(feature = "slow")]

use std::num::NonZeroUsize;
use std::path::PathBuf;

use gcg_diff::corpus::{self, CorpusArgs, EngineKind};
use gcg_diff::fuzz::{self, AgentKind, AgentSpec, FuzzArgs};
use gcg_diff::manifest::Repo;
use gcg_diff::{Record, read_trace};

fn repo() -> Repo {
    Repo {
        root: PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../..")),
    }
}

#[test]
fn every_tier1_trace_replays_against_the_real_engine() {
    let args = CorpusArgs {
        tier: 1,
        only: Vec::new(),
        file: None,
        engine: EngineKind::Real,
        roundtrip: false,
        paranoid: cfg!(feature = "paranoid"),
        require_all: true,
        jobs: 8,
        report: None,
        junit: None,
        census: false,
    };
    let report = corpus::run(&repo(), &args);
    assert!(report.errors.is_empty(), "{}", report.errors.join("\n"));
    let failures: Vec<String> = report
        .results
        .iter()
        .filter_map(|r| r.outcome.clone().err())
        .take(5)
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert_eq!(report.results.len(), 3804);
    let agent: u64 = corpus::set_stats(&report)
        .iter()
        .map(|(_, s)| s.agent_decisions)
        .sum();
    // Every agent block of the search set (771) and the baselines set (625) is replayed.
    assert_eq!(agent, 771 + 625);
}

#[test]
fn negative_controls_fire_against_the_real_engine() {
    let lines = gcg_diff::selftest::run_real(&repo().root.join("port/corpus/v1/tier1"))
        .expect("every control fires");
    assert!(lines.iter().any(|l| l.contains("(real engine)")));
}

#[test]
fn the_fuzz_port_plays_the_recorded_fuzz_games() {
    let file = repo()
        .root
        .join("port/corpus/v1/tier1/fuzz/s1-g0000-0019.ndjson");
    let traces = read_trace(&file).expect("fuzz traces");
    let args = FuzzArgs {
        games: 20,
        seed: 1,
        workers: NonZeroUsize::new(4).expect("4"),
        agent: AgentSpec::Both(AgentKind::Fuzz),
    };
    let games = fuzz::run(&args).expect("runs");
    for (g, t) in games.iter().zip(&traces) {
        let outcome = t
            .records
            .iter()
            .find_map(|r| match r {
                Record::Outcome(o) => Some(o.clone()),
                _ => None,
            })
            .expect("an outcome");
        assert!(g.ok, "{}: {:?}", t.header.id, g.error);
        assert_eq!(g.actions, outcome.actions, "{}", t.header.id);
        assert_eq!(g.turns, Some(outcome.turn), "{}", t.header.id);
        assert_eq!(
            g.end.as_deref(),
            outcome.end_reason.map(gcg_engine::EndReason::as_str),
            "{}",
            t.header.id
        );
    }
}

#[test]
fn coverage_and_traceability_hold() {
    let (_, problems) = gcg_diff::coverage::check(&repo()).expect("coverage runs");
    assert!(problems.is_empty(), "{}", problems.join("\n"));
    let (_, problems) = gcg_diff::traceability::run(&repo(), None, false).expect("traceability");
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
