//! Live benchmark runs through the engine (T-071, T-074, T-078; PLAN §10.4 E gate): the
//! Rust versions of the `tests/runner/*` cases that play real games, and the Python record sets of
//! `oracle/export_records.py` re-played with the `test-random` agent, whose every report
//! file (`results.json`, `summary.md`, `games.ndjson`, `replays/*.json`) must equal the file
//! Python wrote, byte for byte.
//!
//! Every test here needs the engine's game layer (`new_game`, `apply`; layers L2-L5), so each
//! name starts with `needs_engine_`.

use std::fs;
use std::path::{Path, PathBuf};

use gcg_ai::{AgentCx, AgentError, Scratch};
use gcg_core::pool::{Cancel, Pool, TaskCx};
use gcg_data::{CardCount, Deck};
use gcg_engine::EndReason;
use gcg_runner::game::match_loser_seat;
use gcg_runner::replay::{check_replay, replay_document};
use gcg_runner::reports::{games_ndjson, to_json_text, write_reports};
use gcg_runner::test_agents::factory;
use gcg_runner::{
    AgentFactory, AgentSpec, BenchmarkConfig, BenchmarkRun, Format, GameRecord, MatchRecord,
    Outcome, ReplayError, Role, RunError, SchemaVersion, TestAgentKind, Workers, agent_seed,
    game_seed, play_match, replay, run_benchmark,
};
use serde_json::Value;

fn records() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/vectors/records")
}

fn read_json(path: &Path) -> Value {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn leak(s: &str) -> &'static str {
    Box::leak(s.to_owned().into_boxed_str())
}

fn str_of(v: &Value) -> &str {
    v.as_str().expect("a string")
}

fn u32_of(v: &Value) -> u32 {
    u32::try_from(v.as_u64().expect("an unsigned integer")).expect("u32")
}

fn deck(v: &Value) -> Deck {
    let counts = |v: &Value| -> Box<[CardCount]> {
        v.as_array()
            .expect("a count list")
            .iter()
            .map(|pair| CardCount {
                card_number: str_of(&pair[0]).into(),
                count: u128::from(pair[1].as_u64().expect("count")),
            })
            .collect()
    };
    Deck {
        name: str_of(&v["name"]).into(),
        main: counts(&v["main"]),
        resources: counts(&v["resources"]),
        entries: Box::new([]),
        source: v["source"].as_str().map(Box::from),
        digest: str_of(&v["digest"]).to_owned(),
    }
}

/// The configuration of an exported record set (`run.json`), with Python's `versions()`
/// inputs, so that `results.json` can match byte for byte.
fn config_of(id: &str) -> BenchmarkConfig {
    let spec = read_json(&records().join(id).join("run.json"));
    let c = &spec["config"];
    let versions = &spec["report_data"]["versions"];
    BenchmarkConfig {
        deck_under_test: deck(&c["deck_under_test"]),
        benchmark_deck: deck(&c["benchmark_deck"]),
        matches: u32_of(&c["matches"]),
        seed: c["seed"].as_u64().expect("seed"),
        workers: Workers::Count(u32_of(&c["workers"])),
        format: Format::from_value(str_of(&c["fmt"])).expect("bo1 or bo3"),
        ai_preset: leak(str_of(&c["ai_preset"])),
        out_dir: PathBuf::from("gcg-sim-out"),
        decision_log: c["decision_log"].as_bool().expect("bool"),
        replays: u32_of(&c["replays"]),
        turn_limit: i32::try_from(c["turn_limit"].as_i64().expect("int")).expect("i32"),
        max_actions: c["max_actions"].as_i64().expect("int"),
        schema: SchemaVersion::V1_1,
        package: leak(str_of(&versions["package"])),
        code_digest: leak(str_of(&versions["code_digest"])),
    }
}

fn random_ids() -> Vec<String> {
    read_json(&records().join("index.json"))["runs"]
        .as_array()
        .expect("runs")
        .iter()
        .map(|r| str_of(&r["id"]).to_owned())
        .filter(|id| id.starts_with("random-"))
        .collect()
}

fn run(cfg: &BenchmarkConfig, f: &dyn AgentFactory) -> Result<BenchmarkRun, RunError> {
    run_benchmark(cfg, f, &mut |_, _| {}, &Cancel::new())
}

/// A fresh scratch directory under the target directory, removed by the caller.
fn scratch_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    dir
}

/// Every report file except `timing.json`, by relative path.
fn report_files(dir: &Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for name in ["results.json", "summary.md", "games.ndjson"] {
        out.push((
            name.to_owned(),
            fs::read_to_string(dir.join(name)).expect(name),
        ));
    }
    let replays = dir.join("replays");
    if replays.is_dir() {
        let mut names: Vec<String> = fs::read_dir(&replays)
            .expect("replays")
            .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        for name in names {
            let text = fs::read_to_string(replays.join(&name)).expect("replay");
            out.push((format!("replays/{name}"), text));
        }
    }
    out
}

fn write(run: &BenchmarkRun, dir: &Path) -> Vec<(String, String)> {
    write_reports(run, dir, &Cancel::new(), &mut |r| {
        gcg_runner::replay::resimulate(r, &run.config)
    })
    .expect("reports are written");
    let files = report_files(dir);
    let _ = fs::remove_dir_all(dir);
    files
}

#[test]
fn needs_engine_test_random_runs_reproduce_the_python_records() {
    let ids = random_ids();
    assert_eq!(ids.len(), 7);
    let random = factory(TestAgentKind::Random);
    for id in ids {
        let cfg = config_of(&id);
        let run = run(&cfg, &*random).unwrap_or_else(|e| panic!("{id}: {e}"));
        let dir = records().join(&id);
        assert_eq!(
            games_ndjson(&run),
            fs::read_to_string(dir.join("games.ndjson")).expect("games.ndjson"),
            "{id}: games.ndjson"
        );
        let spec = read_json(&dir.join("run.json"));
        assert_eq!(run.agent_factory, str_of(&spec["agent_factory"]));
        let matches: Vec<Value> = run
            .matches
            .iter()
            .map(|m| serde_json::from_str(&to_json_text(&m.to_json())).expect("JSON"))
            .collect();
        assert_eq!(Value::Array(matches), spec["matches"], "{id}: matches");
        let ours = write(&run, &scratch_dir(&id));
        let python = report_files(&dir);
        assert_eq!(ours.len(), python.len(), "{id}: report files");
        for ((name, text), (py_name, py_text)) in ours.iter().zip(&python) {
            assert_eq!(name, py_name, "{id}");
            assert!(text == py_text, "{id}/{name} differs from Python's");
        }
    }
}

#[test]
fn needs_engine_results_are_identical_at_1_4_and_10_workers() {
    // tests/runner/test_pool.py:18-26, tests/cli/test_console_script.py:45-61 (T-003, T-074).
    let mut cfg = config_of("random-bo3-s3");
    cfg.matches = 24;
    let random = factory(TestAgentKind::Random);
    let mut outputs = Vec::new();
    for workers in [1, 4, 10] {
        cfg.workers = Workers::Count(workers);
        let run = run(&cfg, &*random).expect("random games finish");
        assert_eq!(run.timing.workers, workers);
        outputs.push(write(&run, &scratch_dir(&format!("workers-{workers}"))));
    }
    assert_eq!(outputs[0], outputs[1]);
    assert_eq!(outputs[0], outputs[2]);
}

#[test]
fn needs_engine_an_illegal_agent_fails_the_lowest_match() {
    // tests/runner/test_pool.py:42-47
    let mut cfg = config_of("random-bo3-s3");
    cfg.matches = 6;
    cfg.workers = Workers::Count(2);
    let err = run(&cfg, &*factory(TestAgentKind::Illegal)).expect_err("illegal actions fail");
    assert_eq!(
        err,
        RunError::GameFailure(format!(
            "match 0 game 1 (seed {}) failed: IllegalActionError: illegal action \
             attack(1000000,1000000) for choose_first",
            game_seed(cfg.seed, 0, 0)
        ))
    );
}

/// The random agent, failing for the first game of the listed matches.
struct FailingAt {
    master: u64,
    matches: &'static [u32],
}

impl AgentFactory for FailingAt {
    fn make(&self, spec: &AgentSpec) -> Result<Box<dyn gcg_ai::Agent>, AgentError> {
        if self
            .matches
            .iter()
            .any(|&m| agent_seed(self.master, m, 0, spec.seat) == spec.seed)
        {
            return Err(AgentError::Value("injected".to_owned()));
        }
        factory(TestAgentKind::Random).make(spec)
    }

    fn describe(&self) -> String {
        "failing".to_owned()
    }
}

#[test]
fn needs_engine_failures_at_3_and_7_report_match_3_under_10_workers() {
    // T-074 / W-149: whatever the schedule, the lowest failing match is reported.
    let mut cfg = config_of("random-bo3-s3");
    cfg.matches = 20;
    cfg.workers = Workers::Count(10);
    let f = FailingAt {
        master: cfg.seed,
        matches: &[3, 7],
    };
    for _ in 0..5 {
        assert_eq!(
            run(&cfg, &f).expect_err("injected failures"),
            RunError::GameFailure(format!(
                "match 3 game 1 (seed {}) failed: ValueError: injected",
                game_seed(cfg.seed, 3, 0)
            ))
        );
    }
}

fn play(cfg: &BenchmarkConfig, m: u32) -> MatchRecord {
    let cancel = Cancel::new();
    let random = factory(TestAgentKind::Random);
    Pool::run_inline(|scratch: &mut Scratch, par: &TaskCx<'_, Scratch>| {
        let mut cx = AgentCx {
            scratch,
            par,
            cancel: &cancel,
        };
        play_match(cfg, m, &*random, &mut cx)
    })
    .expect("random games finish")
}

#[test]
fn needs_engine_real_bo3_matches_follow_the_loser_chooses_rule() {
    // tests/runner/test_match.py:124-139
    let mut cfg = config_of("random-bo3-s3");
    cfg.seed = 1234;
    for m in 0..8 {
        let record = play(&cfg, m);
        assert!((2..=3).contains(&record.games.len()));
        assert!(
            record
                .games
                .iter()
                .all(|g| g.dut_seat == u8::from(m % 2 == 1))
        );
        assert_eq!(record.games[0].chooser, None);
        for pair in record.games.windows(2) {
            assert_eq!(pair[1].chooser, match_loser_seat(&pair[0]));
            assert_eq!(Some(pair[1].chooser_seat), pair[1].chooser);
        }
        let points = |r: Role| {
            u32::try_from(
                record
                    .games
                    .iter()
                    .filter(|g| g.match_point == Some(r))
                    .count(),
            )
            .expect("small")
        };
        assert_eq!(record.dut_points, points(Role::Dut));
        assert_eq!(record.bench_points, points(Role::Bench));
        assert_eq!(record.dut_points.max(record.bench_points), 2);
        let winner = if record.dut_points == 2 {
            Outcome::Dut
        } else {
            Outcome::Bench
        };
        assert_eq!(record.winner, winner);
    }
}

#[test]
fn needs_engine_turn_limit_games_are_draws_without_match_points() {
    // tests/runner/test_match.py:158-166
    let mut cfg = config_of("random-bo3-s3");
    cfg.seed = 1234;
    cfg.turn_limit = 2;
    let record = play(&cfg, 0);
    assert!(
        record
            .games
            .iter()
            .all(|g| g.end_reason == EndReason::TurnLimit)
    );
    assert!(record.games.iter().all(|g| g.winner == Outcome::Draw));
    assert!(record.games.iter().all(|g| g.chooser.is_none()));
    assert_eq!(record.games.len(), 3);
    assert_eq!(
        (record.winner, record.dut_points, record.bench_points),
        (Outcome::Draw, 0, 0)
    );
}

fn tekkadan_run() -> (BenchmarkConfig, BenchmarkRun) {
    // tests/runner/test_replay.py:34-40: tekkadan vs federation, 40 matches, seed 2024.
    let mut cfg = config_of("random-bo3-tekkadan");
    cfg.matches = 40;
    cfg.seed = 2024;
    let run = run(&cfg, &*factory(TestAgentKind::Random)).expect("random games finish");
    (cfg, run)
}

/// The `config` fixture of `tests/runner/conftest.py:34-36` (federation vs zeon, 6 matches,
/// seed 1234), run with the random agent.
fn fixture_run() -> (BenchmarkConfig, BenchmarkRun) {
    let mut cfg = config_of("random-bo3-s3");
    cfg.matches = 6;
    cfg.seed = 1234;
    let run = run(&cfg, &*factory(TestAgentKind::Random)).expect("random games finish");
    (cfg, run)
}

#[test]
fn needs_engine_many_games_replay_to_the_recorded_final_state() {
    let (cfg, run) = tekkadan_run();
    let games: Vec<&GameRecord> = run.games().collect();
    assert!(games.len() >= 80);
    for g in games {
        let st = replay(g, &cfg).expect("the record replays");
        assert_eq!(gcg_engine::canon::state_digest(&st), g.final_state_sha256);
        assert_eq!(st.turn(), g.turns);
        let winner = st.winner().expect("finished");
        let label = if winner < 0 {
            Outcome::Draw
        } else if u8::try_from(winner).ok() == Some(g.dut_seat) {
            Outcome::Dut
        } else {
            Outcome::Bench
        };
        assert_eq!(g.winner, label);
        assert_eq!(st.end_reason(), Some(g.end_reason));
    }
}

#[test]
fn needs_engine_record_json_round_trips() {
    // tests/runner/test_replay.py:53-56
    let (_, run) = fixture_run();
    for g in run.games() {
        let text = to_json_text(&g.to_json());
        let v: Value = serde_json::from_str(&text).expect("JSON");
        assert_eq!(&GameRecord::from_json(&v).expect("a record"), g);
    }
}

#[test]
fn needs_engine_replay_documents_verify_and_detect_tampering() {
    // tests/runner/test_replay.py:59-82
    let (cfg, run) = fixture_run();
    let record = run.games().next().expect("a game");
    let doc: Value = serde_json::from_str(&to_json_text(
        &replay_document(record, &cfg, "win").expect("a document"),
    ))
    .expect("JSON");
    let check = check_replay(&doc).expect("checks");
    assert!(check.ok);
    assert!(check.mismatches.is_empty());
    assert_eq!(check.final_state_sha256, record.final_state_sha256);
    assert!(
        check
            .log
            .last()
            .expect("a log")
            .ends_with(&format!("({})", record.end_reason.as_str()))
    );
    assert_eq!(check.log.len(), record.actions.len() + 1);

    let mut truncated = doc.clone();
    truncated["actions"]
        .as_array_mut()
        .expect("actions")
        .pop()
        .expect("an action");
    let bad = check_replay(&truncated).expect("checks");
    assert!(!bad.ok);
    assert!(
        bad.mismatches
            .iter()
            .any(|m| m.starts_with("winner_seat: expected"))
    );

    let mut wrong = doc;
    wrong["expected"]["turns"] = Value::from(i64::from(record.turns) + 1);
    assert_eq!(
        check_replay(&wrong).expect("checks").mismatches,
        [format!(
            "turns: expected {}, replay gave {}",
            record.turns + 1,
            record.turns
        )]
    );
}

#[test]
fn needs_engine_broken_replay_documents_raise_replay_error() {
    // tests/runner/test_replay.py:85-102
    type Mutate = fn(&mut Value);
    let (cfg, run) = fixture_run();
    let record = run.games().next().expect("a game");
    let doc: Value = serde_json::from_str(&to_json_text(
        &replay_document(record, &cfg, "loss").expect("a document"),
    ))
    .expect("JSON");
    let cases: [(Mutate, &str); 5] = [
        (
            |d| d["format"] = "other".into(),
            "not a gcg-sim-replay v1 document",
        ),
        (
            |d| {
                d.as_object_mut().expect("object").remove("seed");
            },
            "malformed replay document: KeyError",
        ),
        (
            |d| {
                d["actions"]
                    .as_array_mut()
                    .expect("actions")
                    .insert(1, serde_json::json!(["attack", 999, 999, -1]));
            },
            "action 1 attack(999,999)",
        ),
        (
            |d| {
                d["actions"]
                    .as_array_mut()
                    .expect("actions")
                    .push(serde_json::json!(["pass", -1, -1, -1]));
            },
            "more actions than the game had",
        ),
        (
            |d| {
                d["actions"]
                    .as_array_mut()
                    .expect("actions")
                    .push(serde_json::json!(["no-such-kind", 0, 0, 0]));
            },
            "malformed replay document",
        ),
    ];
    for (mutate, message) in cases {
        let mut d = doc.clone();
        mutate(&mut d);
        let err: ReplayError = check_replay(&d).expect_err("broken");
        assert!(err.to_string().contains(message), "{err} lacks {message}");
    }
}

#[test]
fn needs_engine_every_saved_python_replay_verifies() {
    // T-004, Python to Rust: every replay file of the exported record sets.
    let mut checked = 0;
    for entry in fs::read_dir(records()).expect("records") {
        let replays = entry.expect("entry").path().join("replays");
        if !replays.is_dir() {
            continue;
        }
        for file in fs::read_dir(&replays).expect("replays") {
            let path = file.expect("file").path();
            let check = check_replay(&read_json(&path)).expect("a valid replay");
            assert!(check.ok, "{}: {:?}", path.display(), check.mismatches);
            checked += 1;
        }
    }
    assert!(checked >= 25, "{checked}");
}
