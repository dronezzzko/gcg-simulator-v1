//! Results schema 1.2.0 (docs/port/PLAN.md §12.7, A10; docs/port/contracts.md O9): the runner
//! records the factory's search block (`AgentFactory::search_block`) and each seat's
//! `Agent::effort` per game, `results.json` 1.2.0 writes them as `ai.search` and `ai.effort`
//! (totals and per-game distributions), the 1.2.0 schema accepts exactly such documents, and
//! `--resume` checkpoints keep the effort. The agents here are the runner's seeded random test
//! agent with synthetic, deterministic search statistics, so the plumbing is tested without
//! the strengthened search (`crates/gcg-cli/tests/benchmark.rs` runs the real one).

use std::fs;
use std::path::{Path, PathBuf};

use gcg_ai::{
    Agent, AgentCx, AgentEffort, AgentError, LogEntry, SearchEffort, SearchSettings, SearchStats,
};
use gcg_core::pool::Cancel;
use gcg_core::pyjson::Json;
use gcg_engine::{Action, GameState};
use gcg_runner::checkpoint::{checkpoint_path, match_line, parse_match_line, run_resumable};
use gcg_runner::reports::{
    ResultsData, build_results_with, games_ndjson, json_from_value, render_summary,
    reproduce_command, to_json_text, validate_results,
};
use gcg_runner::{
    AgentFactory, AgentSpec, BenchmarkConfig, BenchmarkRun, Format, SchemaVersion, TestAgentKind,
    TestRandomAgent, Workers, run_benchmark,
};
use serde_json::{Value, json};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn deck(name: &str) -> gcg_data::Deck {
    gcg_data::load_deck(&root().join("examples/decks").join(name)).expect("an example deck")
}

fn config(matches: u32, format: Format, workers: u32) -> BenchmarkConfig {
    BenchmarkConfig {
        deck_under_test: deck("red-green-zeon.txt"),
        benchmark_deck: deck("blue-white-federation.txt"),
        matches,
        seed: 31,
        workers: Workers::Count(workers),
        format,
        ai_preset: "fast",
        out_dir: PathBuf::from("out"),
        decision_log: false,
        replays: 1,
        turn_limit: 200,
        max_actions: 20_000,
        schema: SchemaVersion::V1_2,
        package: "0.1.0",
        code_digest: "0000000000000000000000000000000000000000000000000000000000000000",
    }
}

/// The seeded random test agent, reporting a synthetic search for every decision with more
/// than one option: as many iterations as options, and engine applies that depend on the
/// option count and the seat.
struct Effortful {
    inner: TestRandomAgent,
    seat: u8,
    effort: AgentEffort,
}

impl Agent for Effortful {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn choose(
        &mut self,
        st: &GameState,
        player: u8,
        cx: &mut AgentCx<'_>,
    ) -> Result<Action, AgentError> {
        let n = st.pending().map_or(0, |d| d.options.len());
        let action = self.inner.choose(st, player, cx)?;
        let n64 = u64::try_from(n).expect("an option count");
        let stats = (n > 1).then(|| SearchStats {
            iterations: u32::try_from(n).expect("an option count"),
            budget: 0,
            cap: 0,
            roots: Vec::new(),
            effort: SearchEffort {
                applies_tree: 3 * n64,
                applies_rollout: 7 * n64,
                applies_aux: u64::from(self.seat),
                worlds: n64,
                rollouts_capped: u64::from(n > 4),
            },
        });
        self.effort.record(stats.as_ref());
        Ok(action)
    }

    fn decision_log(&self) -> &[LogEntry] {
        self.inner.decision_log()
    }

    fn effort(&self) -> Option<AgentEffort> {
        Some(self.effort)
    }
}

fn settings() -> SearchSettings {
    SearchSettings {
        preset: "fast",
        mapping: "reference standard",
        iterations_per_option: 64,
        min_iterations: 160,
        max_iterations: 640,
        aux_apply_cap: 16,
        mechanisms: vec!["engine_error_leaf", "root_trees"],
        weights: "ROLE_WEIGHTS".to_owned(),
    }
}

/// A factory of [`Effortful`] agents with a search block.
struct EffortFactory;

impl AgentFactory for EffortFactory {
    fn make(&self, spec: &AgentSpec) -> Result<Box<dyn Agent>, AgentError> {
        Ok(Box::new(Effortful {
            inner: TestRandomAgent::new(spec.seed, spec.log),
            seat: spec.seat,
            effort: AgentEffort::default(),
        }))
    }

    fn describe(&self) -> String {
        "gcg-search::make_agent('fast')".to_owned()
    }

    fn search_block(&self) -> Option<SearchSettings> {
        Some(settings())
    }
}

fn play(cfg: &BenchmarkConfig, f: &dyn AgentFactory) -> BenchmarkRun {
    run_benchmark(cfg, f, &mut |_, _| {}, &Cancel::new()).expect("random games finish")
}

fn results(run: &BenchmarkRun) -> Json {
    let data = ResultsData::embedded(&run.config).expect("known cards");
    build_results_with(run, &data)
}

fn value(doc: &Json) -> Value {
    serde_json::from_str(&to_json_text(doc)).expect("JSON")
}

#[test]
fn a_1_2_run_records_the_search_block_and_each_seats_effort() {
    let cfg = config(4, Format::Bo3, 1);
    let run = play(&cfg, &EffortFactory);
    assert_eq!(run.search, Some(settings()));
    let mut totals = [AgentEffort::default(); 2];
    let mut per_game = [Vec::new(), Vec::new()];
    let mut games = 0_u64;
    for g in run.games() {
        games += 1;
        let [Some(a), Some(b)] = g.effort else {
            panic!("both seats report their effort: {:?}", g.effort);
        };
        // Every recorded action is one decision of one seat's agent.
        assert_eq!(
            a.decisions + b.decisions,
            u64::try_from(g.actions.len()).expect("a count")
        );
        assert!(a.searches > 0 && b.searches > 0);
        assert_eq!((a.applies_aux, b.applies_aux), (0, b.searches));
        for (seat, e) in [a, b].into_iter().enumerate() {
            totals[seat] = totals[seat].plus(&e);
            per_game[seat].push(e);
        }
    }
    let doc = results(&run);
    validate_results(&doc, SchemaVersion::V1_2).expect("a 1.2.0 document");
    let v = value(&doc);
    assert_eq!(v["schema"], "urn:gcg-sim:schema:results:1.2.0");
    assert_eq!(v["schema_version"], "1.2.0");
    assert_eq!(v["versions"]["results_schema"], "1.2.0");
    assert_eq!(
        v["ai"]["search"],
        json!({
            "algorithm": "gcg-search",
            "preset": "fast",
            "mapping": "reference standard",
            "budget": {
                "iterations_per_option": 64,
                "min_iterations": 160,
                "max_iterations": 640,
                "aux_apply_cap": 16
            },
            "mechanisms": ["engine_error_leaf", "root_trees"],
            "weights": "ROLE_WEIGHTS"
        })
    );
    let effort = v["ai"]["effort"].as_array().expect("one entry per seat");
    assert_eq!(effort.len(), 2);
    for (seat, e) in effort.iter().enumerate() {
        let t = totals[seat];
        assert_eq!(e["seat"], seat);
        assert_eq!(e["games"], games);
        for (key, want) in [
            ("decisions", t.decisions),
            ("searches", t.searches),
            ("iterations", t.iterations),
            ("applies_tree", t.applies_tree),
            ("applies_rollout", t.applies_rollout),
            ("applies_aux", t.applies_aux),
            ("worlds", t.worlds),
            ("rollouts_capped", t.rollouts_capped),
        ] {
            assert_eq!(e[key], want, "seat {seat} {key}");
        }
        let iterations: Vec<u64> = per_game[seat].iter().map(|x| x.iterations).collect();
        let applies: Vec<u64> = per_game[seat].iter().map(AgentEffort::applies).collect();
        for (key, values) in [
            ("iterations_per_game", iterations),
            ("applies_per_game", applies),
        ] {
            let d = &e[key];
            assert_eq!(d["n"], games, "{key}");
            assert_eq!(d["min"], *values.iter().min().expect("games"), "{key}");
            assert_eq!(d["max"], *values.iter().max().expect("games"), "{key}");
        }
    }
    // summary.md shows the AI settings in its search line.
    let summary = render_summary(&doc, &reproduce_command(&run));
    assert!(
        summary.contains(
            "Search: `gcg-search`, preset `fast` (reference standard mapping), 64 iterations \
             per option (min 160, max 640, auxiliary apply cap 16 per iteration), mechanisms: \
             `engine_error_leaf`, `root_trees`, weights `ROLE_WEIGHTS`."
        ),
        "{summary}"
    );
    assert!(summary.contains("results schema 1.2.0"), "{summary}");
    // The effort is not part of games.ndjson, and a 1.1.0 document of the same games has no
    // trace of it: it equals the document of the run without any effort or search block.
    let mut bare = run.clone();
    bare.search = None;
    for m in &mut bare.matches {
        for g in &mut m.games {
            g.effort = [None, None];
        }
    }
    assert_eq!(games_ndjson(&run), games_ndjson(&bare));
    let mut run11 = run.clone();
    run11.config.schema = SchemaVersion::V1_1;
    bare.config.schema = SchemaVersion::V1_1;
    let doc11 = results(&run11);
    validate_results(&doc11, SchemaVersion::V1_1).expect("a 1.1.0 document");
    assert_eq!(to_json_text(&doc11), to_json_text(&results(&bare)));
    let v11 = value(&doc11);
    assert!(v11["ai"].get("search").is_none() && v11["ai"].get("effort").is_none());
    assert_eq!(v11["versions"]["results_schema"], "1.1.0");
}

#[test]
fn a_1_2_run_is_identical_for_any_worker_count() {
    let mut first: Option<(String, String)> = None;
    for workers in [1, 4, 10] {
        let run = play(&config(10, Format::Bo1, workers), &EffortFactory);
        assert_eq!(run.timing.workers, workers);
        let got = (to_json_text(&results(&run)), games_ndjson(&run));
        match &first {
            None => first = Some(got),
            Some(want) => assert!(*want == got, "--workers {workers} differs from 1"),
        }
    }
}

#[test]
fn a_factory_without_a_search_block_writes_no_1_2_document() {
    let random = gcg_runner::test_agents::factory(TestAgentKind::Random);
    assert_eq!(random.search_block(), None);
    let run = play(&config(2, Format::Bo1, 1), &*random);
    assert_eq!(run.search, None);
    assert!(run.games().all(|g| g.effort == [None, None]));
    let err = validate_results(&results(&run), SchemaVersion::V1_2)
        .expect_err("ai.search and ai.effort are required");
    assert_eq!(err.pointer, "/ai");
}

#[test]
fn resumed_runs_and_checkpoint_lines_keep_the_effort() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("results-1-2-resume");
    let _ = fs::remove_dir_all(&dir);
    let mut cfg = config(3, Format::Bo3, 2);
    cfg.out_dir = dir.clone();
    let want = play(&cfg, &EffortFactory);
    let path = checkpoint_path(&dir);
    let cancel = Cancel::new();
    let first = run_resumable(&cfg, &EffortFactory, &mut |_, _| {}, &cancel, &path)
        .expect("the checkpointed run");
    // The second run takes every match from the checkpoint.
    let again = run_resumable(&cfg, &EffortFactory, &mut |_, _| {}, &cancel, &path)
        .expect("the resumed run");
    for run in [&first, &again] {
        assert_eq!(run.matches, want.matches);
        assert_eq!(run.search, want.search);
        assert_eq!(to_json_text(&results(run)), to_json_text(&results(&want)));
    }
    // A line round-trips each seat's effort, also a seat that reported none.
    let mut m = want.matches[0].clone();
    m.games[0].effort[1] = None;
    let (back, seconds) = parse_match_line(&match_line(&m, 1.5)).expect("a match line");
    assert_eq!((back, seconds), (m.clone(), 1.5));
    // Records without effort write no effort key, as before.
    for g in &mut m.games {
        g.effort = [None, None];
    }
    let line = match_line(&m, 0.0);
    assert!(!line.contains("\"effort\""));
    assert_eq!(parse_match_line(&line).expect("a match line").0, m);
    let _ = fs::remove_dir_all(&dir);
}

/// A valid 1.2.0 document of the effort factory's run, as JSON.
fn base_document() -> Value {
    let run = play(&config(2, Format::Bo1, 1), &EffortFactory);
    value(&results(&run))
}

#[test]
fn the_1_2_schema_rejects_documents_that_violate_it() {
    let base = base_document();
    let check = |doc: &Value| validate_results(&json_from_value(doc), SchemaVersion::V1_2);
    assert_eq!(check(&base), Ok(()));
    assert!(validate_results(&json_from_value(&base), SchemaVersion::V1_1).is_err());
    type Mutation = fn(&mut Value);
    let controls: [(&str, Mutation, &str); 22] = [
        (
            "schema_version",
            |d| d["schema_version"] = json!("1.1.0"),
            "/schema_version",
        ),
        (
            "results_schema",
            |d| d["versions"]["results_schema"] = json!("1.1.0"),
            "/versions/results_schema",
        ),
        (
            "reference preset",
            |d| d["ai"]["preset"] = json!("strong"),
            "/ai/preset",
        ),
        ("no search", |d| remove(&mut d["ai"], "search"), "/ai"),
        ("no effort", |d| remove(&mut d["ai"], "effort"), "/ai"),
        (
            "algorithm",
            |d| d["ai"]["search"]["algorithm"] = json!("mcts"),
            "/ai/search/algorithm",
        ),
        (
            "search preset",
            |d| d["ai"]["search"]["preset"] = json!("strong"),
            "/ai/search/preset",
        ),
        (
            "no weights",
            |d| remove(&mut d["ai"]["search"], "weights"),
            "/ai/search",
        ),
        (
            "weights",
            |d| d["ai"]["search"]["weights"] = json!("roles"),
            "/ai/search/weights",
        ),
        (
            "extra search key",
            |d| d["ai"]["search"]["exploration"] = json!(0.3),
            "/ai/search",
        ),
        (
            "budget",
            |d| d["ai"]["search"]["budget"]["iterations_per_option"] = json!(0),
            "/ai/search/budget/iterations_per_option",
        ),
        (
            "aux cap",
            |d| d["ai"]["search"]["budget"]["aux_apply_cap"] = json!(-1),
            "/ai/search/budget/aux_apply_cap",
        ),
        (
            "mechanism name",
            |d| d["ai"]["search"]["mechanisms"][0] = json!("Root-Trees"),
            "/ai/search/mechanisms/0",
        ),
        // The preset, its mapping and the search block's preset agree (oneOf).
        (
            "mapping of another preset",
            |d| d["ai"]["search"]["mapping"] = json!("reference strong"),
            "/ai",
        ),
        (
            "search of another preset",
            |d| {
                d["ai"]["search"]["preset"] = json!("standard");
                d["ai"]["search"]["mapping"] = json!("reference strong");
            },
            "/ai",
        ),
        ("one seat", |d| pop(&mut d["ai"]["effort"]), "/ai/effort"),
        (
            "seats swapped",
            |d| {
                let e = d["ai"]["effort"].as_array_mut().expect("seats");
                e.swap(0, 1);
            },
            "/ai/effort/0/seat",
        ),
        (
            "negative count",
            |d| d["ai"]["effort"][0]["iterations"] = json!(-1),
            "/ai/effort/0/iterations",
        ),
        (
            "fractional count",
            |d| d["ai"]["effort"][1]["applies_tree"] = json!(2.5),
            "/ai/effort/1/applies_tree",
        ),
        (
            "no distribution",
            |d| remove(&mut d["ai"]["effort"][1], "applies_per_game"),
            "/ai/effort/1",
        ),
        (
            "distribution",
            |d| d["ai"]["effort"][0]["iterations_per_game"]["n"] = json!("2"),
            "/ai/effort/0/iterations_per_game/n",
        ),
        (
            "extra effort key",
            |d| d["ai"]["effort"][1]["seconds"] = json!(1.0),
            "/ai/effort/1",
        ),
    ];
    for (name, mutate, pointer) in controls {
        let mut doc = base.clone();
        mutate(&mut doc);
        assert_ne!(doc, base, "{name}: the mutation changes the document");
        let err = check(&doc).expect_err(name);
        assert!(
            err.pointer == pointer || err.pointer.starts_with(&format!("{pointer}/")),
            "{name}: rejected at {:?}, expected {pointer:?} ({err})",
            err.pointer
        );
    }
}

fn remove(v: &mut Value, key: &str) {
    v.as_object_mut().expect("an object").remove(key);
}

fn pop(v: &mut Value) {
    v.as_array_mut().expect("an array").pop();
}
