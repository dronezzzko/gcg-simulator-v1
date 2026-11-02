//! `--workers auto` on two- and three-tier machines (T-077; PLAN §7.4, A4): the captured
//! `sysctl` listing of the M2 Pro (6 Performance + 4 Efficiency cores) and the synthetic M6
//! layout (2 Super + 4 Performance + 6 Efficiency; no M6 was available) parse into their tiers;
//! `auto` then uses every logical CPU of every tier, at most one worker per match, while an
//! explicit count keeps Python's rule; `timing.json` records the tiers and the workers used.
//! On this machine, a run under `auto` records `min(logical CPUs, matches)` workers and the
//! same results as one worker.

use std::path::{Path, PathBuf};

use gcg_core::pool::Cancel;
use gcg_core::sys::topology::{self, PerfLevel, Topology, TopologySource, parse_sysctl};
use gcg_runner::reports::{TimingHost, build_results, timing_json, to_json_text};
use gcg_runner::test_agents::factory;
use gcg_runner::{
    BenchmarkConfig, BenchmarkRun, Format, RunTiming, SchemaVersion, TestAgentKind, Workers,
    run_benchmark, workers_used,
};

fn fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../gcg-core/tests/fixtures")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The topology of a `sysctl` listing: `key: value` lines answer the queries, and the
/// `hw.optional.arm.` lines are the feature listing.
fn parse(listing: &str) -> Topology {
    let values: Vec<(String, String)> = listing
        .lines()
        .filter_map(|l| l.split_once(": "))
        .map(|(k, v)| (k.to_owned(), v.trim().to_owned()))
        .collect();
    let query = |key: &str| {
        values
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
    };
    let arm: String = listing
        .lines()
        .filter(|l| l.starts_with("hw.optional.arm."))
        .map(|l| format!("{l}\n"))
        .collect();
    parse_sysctl(&query, &arm).expect("the listing parses")
}

fn tiers(t: &Topology) -> Vec<(&str, u32)> {
    t.levels
        .iter()
        .map(|l: &PerfLevel| (l.name.as_str(), l.logical_cpus))
        .collect()
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn config(matches: u32, workers: Workers) -> BenchmarkConfig {
    let deck = |name: &str| {
        gcg_data::load_deck(&root().join("examples/decks").join(name)).expect("an example deck")
    };
    BenchmarkConfig {
        deck_under_test: deck("purple-white-tekkadan.txt"),
        benchmark_deck: deck("blue-white-federation.txt"),
        matches,
        seed: 77,
        workers,
        format: Format::Bo1,
        ai_preset: "standard",
        out_dir: PathBuf::from("gcg-sim-out"),
        decision_log: false,
        replays: 1,
        turn_limit: 200,
        max_actions: 20_000,
        schema: SchemaVersion::V1_1,
        package: "0.1.0",
        code_digest: "0000000000000000000000000000000000000000000000000000000000000000",
    }
}

#[test]
fn two_and_three_tier_listings_parse_into_their_tiers() {
    let m2 = parse(&fixture("sysctl-m2pro.txt"));
    assert_eq!(tiers(&m2), [("Performance", 6), ("Efficiency", 4)]);
    assert_eq!(
        (m2.logical_cpus, m2.cache_line_bytes, m2.source),
        (10, Some(128), TopologySource::Sysctl)
    );
    let m6 = parse(&fixture("sysctl-3tier-synthetic.txt"));
    assert_eq!(
        tiers(&m6),
        [("Super", 2), ("Performance", 4), ("Efficiency", 6)]
    );
    assert_eq!((m6.logical_cpus, m6.source), (12, TopologySource::Sysctl));
}

#[test]
fn auto_uses_every_tier_and_at_most_one_worker_per_match() {
    for (listing, logical) in [
        ("sysctl-m2pro.txt", 10_u32),
        ("sysctl-3tier-synthetic.txt", 12),
    ] {
        let topo = parse(&fixture(listing));
        for matches in 1..=30_u32 {
            assert_eq!(
                workers_used(&config(matches, Workers::Auto), &topo),
                logical.min(matches),
                "{listing}, {matches} matches"
            );
            // An explicit count keeps runner/run.py's rule whatever the machine.
            for n in [1_u32, 4, 10, 16] {
                let want = if n <= 1 || matches == 1 {
                    1
                } else {
                    n.min(matches)
                };
                assert_eq!(
                    workers_used(&config(matches, Workers::Count(n)), &topo),
                    want,
                    "{listing}, --workers {n}, {matches} matches"
                );
            }
        }
    }
}

#[test]
fn timing_json_records_the_tiers_and_the_workers_used() {
    for (listing, names) in [
        ("sysctl-m2pro.txt", vec!["Performance", "Efficiency"]),
        (
            "sysctl-3tier-synthetic.txt",
            vec!["Super", "Performance", "Efficiency"],
        ),
    ] {
        let topo = parse(&fixture(listing));
        let cfg = config(20, Workers::Auto);
        let run = BenchmarkRun {
            matches: Vec::new(),
            timing: RunTiming {
                started_at: "2026-11-02T00:00:00+00:00".to_owned(),
                finished_at: "2026-11-02T00:00:01+00:00".to_owned(),
                wall_seconds: 1.0,
                match_seconds: Vec::new(),
                workers: workers_used(&cfg, &topo),
                topology: topo.clone(),
            },
            config: cfg,
            agent_factory: "random_agent.py:make_random_agent".to_owned(),
            search: None,
        };
        let doc: serde_json::Value =
            serde_json::from_str(&to_json_text(&timing_json(&run, &TimingHost::rust())))
                .expect("timing.json is JSON");
        assert_eq!(doc["workers"], topo.logical_cpus.min(20));
        let t = &doc["topology"];
        assert_eq!(t["logical_cpus"], topo.logical_cpus);
        assert_eq!(t["source"], "sysctl");
        let levels = t["levels"].as_array().expect("levels");
        assert_eq!(
            levels
                .iter()
                .map(|l| l["name"].as_str().unwrap_or_default())
                .collect::<Vec<_>>(),
            names
        );
        for (l, want) in levels.iter().zip(&topo.levels) {
            assert_eq!(l["logical_cpus"], want.logical_cpus);
            assert_eq!(l["physical_cpus"], want.physical_cpus);
        }
    }
}

/// `--workers auto` on this machine: the workers recorded are `min(logical CPUs, matches)`, and
/// the results are those of one worker.
#[test]
fn needs_engine_auto_runs_like_one_worker_with_every_cpu() {
    let f = factory(TestAgentKind::Random);
    let here = topology::detect();
    let one = run_benchmark(
        &config(12, Workers::Count(1)),
        f.as_ref(),
        &mut |_, _| {},
        &Cancel::new(),
    )
    .expect("one worker");
    let auto = run_benchmark(
        &config(12, Workers::Auto),
        f.as_ref(),
        &mut |_, _| {},
        &Cancel::new(),
    )
    .expect("auto");
    let want = u32::try_from(here.logical_cpus.min(12)).expect("small");
    assert_eq!(auto.timing.workers, want);
    assert_eq!(auto.timing.topology, here);
    assert_eq!(one.timing.workers, 1);
    assert_eq!(one.matches, auto.matches);
    assert_eq!(
        to_json_text(&build_results(&one)),
        to_json_text(&build_results(&auto))
    );
}
