//! P6-M (docs/port/PLAN.md §12.2): the report and runner items of `gcg-runner`, on runs of the
//! runner's seeded random test agent (real engine games) and on synthetic records.
//!
//! - W-97: the schema files name their versions; W-100 / §12.7: results 1.2.0 and the
//!   corrected reproduce line;
//! - W-102: an abort after any file of a report set leaves the complete old set;
//! - W-99: the in-house gzip writer round-trips through `gzip -dc`;
//! - W-148: the extreme replays;
//! - W-105: kill (a failing match) and resume give the uninterrupted reports byte for byte; a
//!   different configuration is refused; a cut-off last line is ignored;
//! - W-149: the lowest failing match is reported over 50 repetitions at 1, 2 and 8 workers;
//! - W-107: a custom factory runs on 10 workers (the non-`Sync` case is the compile-fail
//!   doctest of `gcg_runner::checkpoint`);
//! - W-108: no crate reads an agent-factory variable;
//! - W-91: the AI-identity guard.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use gcg_ai::{Agent, AgentError};
use gcg_core::pool::Cancel;
use gcg_core::pyjson::Json;
use gcg_runner::checkpoint::{CHECKPOINT_FILE, ResumeError, run_resumable};
use gcg_runner::reports::identity::check_comparable;
use gcg_runner::reports::schema::{RESULTS_SCHEMA_V1_1, RESULTS_SCHEMA_V1_2};
use gcg_runner::reports::set::{EXTREME_REPLAY_FILE, extreme_replay_name, is_report_entry};
use gcg_runner::reports::writer::{deck_file_name, release_factory, written_decks};
use gcg_runner::reports::{
    ResultsData, SearchReport, SeatEffort, SetOptions, TimingHost, build_results_with,
    reproduce_command, select_extremes, validate_results, write_report_set,
};
use gcg_runner::{
    AgentFactory, AgentSpec, BenchmarkConfig, BenchmarkRun, Format, GameRecord, Role, RunError,
    SchemaVersion, TestAgentKind, Workers, agent_seed, run_benchmark,
};
use serde_json::Value;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("p6m-{name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn deck(name: &str) -> gcg_data::Deck {
    gcg_data::load_deck(&root().join("examples/decks").join(name)).expect("an example deck")
}

fn config(matches: u32, seed: u64, format: Format, workers: u32) -> BenchmarkConfig {
    BenchmarkConfig {
        deck_under_test: deck("red-green-zeon.txt"),
        benchmark_deck: deck("blue-white-federation.txt"),
        matches,
        seed,
        workers: Workers::Count(workers),
        format,
        ai_preset: "fast",
        out_dir: PathBuf::from("out"),
        decision_log: false,
        replays: 2,
        turn_limit: 200,
        max_actions: 20_000,
        schema: SchemaVersion::V1_1,
        package: "0.1.0",
        code_digest: "0000000000000000000000000000000000000000000000000000000000000000",
    }
}

fn random() -> Box<dyn AgentFactory> {
    gcg_runner::test_agents::factory(TestAgentKind::Random)
}

fn play(cfg: &BenchmarkConfig) -> BenchmarkRun {
    run_benchmark(cfg, &*random(), &mut |_, _| {}, &Cancel::new()).expect("random games finish")
}

fn write(
    run: &BenchmarkRun,
    out: &Path,
    opts: SetOptions<'_>,
) -> Result<(), gcg_runner::ReportError> {
    let data = ResultsData::embedded(&run.config).expect("known cards");
    let mut doc = |label: &'static str, r: &GameRecord| {
        gcg_runner::replay::replay_document(r, &run.config, label)
    };
    write_report_set(
        run,
        out,
        &Cancel::new(),
        &data,
        &mut doc,
        &TimingHost::rust(),
        opts,
    )
    .map(|_| ())
}

/// Every file under `dir` (relative path, bytes), sorted.
fn tree(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in fs::read_dir(&d).expect("readable") {
            let p = e.expect("entry").path();
            if p.is_dir() {
                stack.push(p);
            } else {
                let rel = p
                    .strip_prefix(dir)
                    .expect("inside")
                    .to_string_lossy()
                    .into_owned();
                out.insert(rel, fs::read(&p).expect("file"));
            }
        }
    }
    out
}

fn without_timing(mut t: BTreeMap<String, Vec<u8>>) -> BTreeMap<String, Vec<u8>> {
    t.remove("timing.json");
    t
}

fn search_report() -> SearchReport {
    SearchReport {
        mapping: "reference standard",
        iterations_per_option: 64,
        min_iterations: 128,
        max_iterations: 640,
        aux_apply_cap: 0,
        mechanisms: vec!["engine_error_leaf".to_owned()],
        effort: [SeatEffort::default(); 2],
    }
}

#[test]
fn w97_schema_files_carry_the_version_they_validate() {
    for (text, v) in [
        (RESULTS_SCHEMA_V1_1, SchemaVersion::V1_1),
        (RESULTS_SCHEMA_V1_2, SchemaVersion::V1_2),
    ] {
        let s: Value = serde_json::from_str(text).expect("a JSON schema");
        assert_eq!(s["$id"], v.id());
        assert_eq!(s["properties"]["schema"]["const"], v.id());
        assert_eq!(s["properties"]["schema_version"]["const"], v.as_str());
        assert_eq!(
            s["properties"]["versions"]["properties"]["results_schema"]["const"],
            v.as_str()
        );
    }
    assert_eq!(
        SchemaVersion::V1_1.as_str(),
        gcg_data::RESULTS_SCHEMA_VERSION
    );
    // The file names: major version only for 1.1.0 (Python's file), plus the minor for 1.2.0.
    assert!(
        root()
            .join("src/gcg_sim/reports/schema/results.v1.schema.json")
            .is_file()
    );
    assert!(
        root()
            .join("crates/gcg-runner/schema/results.v1_2.schema.json")
            .is_file()
    );
}

#[test]
fn schema_1_2_validates_a_run_with_its_search_block_and_rejects_without() {
    let mut cfg = config(2, 11, Format::Bo1, 1);
    let run11 = play(&cfg);
    let data = ResultsData::embedded(&cfg).expect("known cards");
    validate_results(&build_results_with(&run11, &data), SchemaVersion::V1_1).expect("1.1.0");
    cfg.schema = SchemaVersion::V1_2;
    let mut run = run11.clone();
    run.config = cfg.clone();
    let doc = build_results_with(&run, &data.clone().with_search(search_report()));
    validate_results(&doc, SchemaVersion::V1_2).expect("1.2.0 with ai.search and ai.effort");
    assert_eq!(doc.get("schema_version"), Some(&Json::Str("1.2.0".into())));
    let v: Value = serde_json::from_str(&gcg_runner::reports::to_json_text(&doc)).expect("JSON");
    assert_eq!(v["versions"]["results_schema"], "1.2.0");
    assert_eq!(v["ai"]["search"]["algorithm"], "gcg-search");
    assert_eq!(v["ai"]["effort"][1]["seat"], 1);
    // Negative controls: no search block; a reference preset; a 1.2.0 document as 1.1.0.
    let err = validate_results(&build_results_with(&run, &data), SchemaVersion::V1_2)
        .expect_err("ai.search is required");
    assert_eq!(err.pointer, "/ai");
    let mut strong = run.clone();
    strong.config.ai_preset = "strong";
    let err = validate_results(
        &build_results_with(&strong, &data.clone().with_search(search_report())),
        SchemaVersion::V1_2,
    )
    .expect_err("strong is not a shipped preset");
    assert_eq!(err.pointer, "/ai/preset");
    assert!(validate_results(&doc, SchemaVersion::V1_1).is_err());
    // The 1.1.0 document of the same run is unchanged by the 1.2.0 support.
    assert_eq!(
        gcg_runner::reports::to_json_text(&build_results_with(&run11, &data)),
        gcg_runner::reports::to_json_text(&gcg_runner::reports::build_results_with(&run11, &data))
    );
    let summary = gcg_runner::reports::render_summary(&doc, &reproduce_command(&run));
    assert!(summary.contains(
        "Search: `gcg-search` (reference standard mapping), 64 iterations per option (min 128, \
         max 640, auxiliary apply cap 0), mechanisms: `engine_error_leaf`."
    ));
}

#[test]
fn w100_the_corrected_reproduce_line_of_results_1_2() {
    let cfg = config(1, 3, Format::Bo1, 1);
    let mut run = play(&cfg);
    // 1.1.0: Python's line, byte for byte (the deck sources, no factory).
    let python = reproduce_command(&run);
    assert_eq!(
        python,
        format!(
            "gcg-sim benchmark {} {} --matches 1 --seed 3 --format bo1 --ai-preset fast --replays 2",
            run.config
                .deck_under_test
                .source
                .as_deref()
                .unwrap_or_default(),
            run.config
                .benchmark_deck
                .source
                .as_deref()
                .unwrap_or_default()
        )
    );
    run.config.schema = SchemaVersion::V1_2;
    // An injected agent: no release command builds it.
    let line = reproduce_command(&run);
    assert!(
        line.starts_with("not reproducible with the release CLI")
            && line.contains("'random_agent.py:make_random_agent'"),
        "{line}"
    );
    assert!(written_decks(&run).is_empty());
    // The release factory of the preset: the command, with an in-memory deck written out.
    run.agent_factory = release_factory("fast");
    assert_eq!(run.agent_factory, "gcg-search::make_agent('fast')");
    assert_eq!(reproduce_command(&run), python);
    let text = fs::read_to_string(root().join("examples/decks/red-green-zeon.txt")).expect("deck");
    run.config.deck_under_test =
        gcg_data::parse_deck(&text, "my deck", None).expect("an in-memory deck");
    run.config.out_dir = PathBuf::from("reports dir");
    let line = reproduce_command(&run);
    assert!(
        line.starts_with("gcg-sim benchmark 'reports dir/decks/deck_under_test/my deck.txt' "),
        "{line}"
    );
    let decks = written_decks(&run);
    assert_eq!(decks.len(), 1);
    assert_eq!(decks[0].0, "decks/deck_under_test/my deck.txt");
    let dir = scratch("w100");
    let path = dir.join(&decks[0].0);
    fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    fs::write(&path, &decks[0].1).expect("deck file");
    let back = gcg_data::load_deck(&path).expect("the written deck parses");
    assert_eq!(&*back.name, "my deck");
    assert_eq!(back.digest, run.config.deck_under_test.digest);
    assert_eq!(back.main, run.config.deck_under_test.main);
    assert_eq!(deck_file_name(Role::Bench, "a/b"), None);
    run.config.deck_under_test.name = "a/b".into();
    assert!(reproduce_command(&run).starts_with("not reproducible with the release CLI: the deck"));
}

#[test]
fn w102_an_abort_after_any_file_keeps_the_complete_old_set() {
    let first = play(&config(3, 21, Format::Bo3, 2));
    let second = play(&config(3, 22, Format::Bo3, 2));
    for foreign in [false, true] {
        let dir = scratch(&format!("w102-{foreign}"));
        let out = dir.join("reports");
        write(&first, &out, SetOptions::default()).expect("the first set");
        if foreign {
            // A file that is not a report: the set is written in place, the file kept.
            fs::write(out.join("notes.txt"), "mine").expect("foreign file");
        }
        let old = tree(&out);
        let mut files = 0;
        let mut count = |_: &Path| {
            files += 1;
            true
        };
        let opts = SetOptions {
            after_file: Some(&mut count),
            ..SetOptions::default()
        };
        let probe = dir.join("probe");
        write(&second, &probe, opts).expect("count the files of the new set");
        let new = tree(&probe);
        assert!(files >= 5, "{files} files");
        for k in 0..files {
            let mut seen = 0;
            let mut abort = |_: &Path| {
                seen += 1;
                seen <= k
            };
            let opts = SetOptions {
                after_file: Some(&mut abort),
                ..SetOptions::default()
            };
            let err = write(&second, &out, opts).expect_err("aborted");
            assert!(matches!(err, gcg_runner::ReportError::Interrupted));
            assert_eq!(tree(&out), old, "abort after file {k} (foreign {foreign})");
            let leftovers: Vec<String> = fs::read_dir(&dir)
                .expect("dir")
                .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
                .filter(|n| n.starts_with('.'))
                .collect();
            assert!(leftovers.is_empty(), "{leftovers:?}");
        }
        write(&second, &out, SetOptions::default()).expect("the new set");
        let mut expect = new.clone();
        if foreign {
            expect.insert("notes.txt".to_owned(), b"mine".to_vec());
        }
        assert_eq!(without_timing(tree(&out)), without_timing(expect));
        let _ = fs::remove_dir_all(&dir);
    }
    assert!(is_report_entry("results.json") && is_report_entry(CHECKPOINT_FILE));
    assert!(!is_report_entry("notes.txt"));
}

#[test]
fn w102_a_symlinked_report_directory_stays_a_link() {
    let run = play(&config(1, 23, Format::Bo1, 1));
    let dir = scratch("w102-link");
    let target = dir.join("target");
    fs::create_dir_all(&target).expect("target dir");
    let link = dir.join("reports");
    std::os::unix::fs::symlink(&target, &link).expect("symlink");
    write(&run, &link, SetOptions::default()).expect("written through the link");
    let meta = fs::symlink_metadata(&link).expect("the link");
    assert!(meta.file_type().is_symlink());
    assert!(target.join("results.json").is_file());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn w99_gzip_output_decompresses_to_the_same_bytes() {
    let games = gcg_runner::reports::games_ndjson(&play(&config(2, 5, Format::Bo1, 1)));
    let mut noise = Vec::new();
    let mut rng = gcg_core::SplitMix64::new(9);
    for _ in 0..70_000 {
        noise.push(u8::try_from(rng.next_u64() & 0xFF).expect("a byte"));
    }
    let samples: Vec<Vec<u8>> = vec![
        Vec::new(),
        b"x".to_vec(),
        b"abcabcabcabcabcabc".to_vec(),
        "{\"turn\":12}\n".repeat(20_000).into_bytes(),
        noise,
        games.into_bytes(),
    ];
    let dir = scratch("w99");
    for (i, data) in samples.iter().enumerate() {
        let path = dir.join(format!("{i}.gz"));
        fs::write(&path, gcg_runner::reports::gzip::gzip(data)).expect("write");
        let out = Command::new("gzip")
            .arg("-dc")
            .arg(&path)
            .output()
            .expect("gzip runs");
        assert!(
            out.status.success(),
            "sample {i}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(&out.stdout, data, "sample {i}");
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn w148_extreme_replays_are_the_minimum_and_maximum_lengths() {
    let run = play(&config(8, 31, Format::Bo3, 2));
    let picks = select_extremes(&run);
    let games: Vec<&GameRecord> = run.games().collect();
    for (kind, label, g) in &picks {
        let same: Vec<&&GameRecord> = games
            .iter()
            .filter(|x| gcg_runner::reports::aggregate::DutResult::of(x).as_str() == *label)
            .collect();
        let target = if *kind == "shortest" {
            same.iter().map(|x| x.turns).min()
        } else {
            same.iter().map(|x| x.turns).max()
        };
        assert_eq!(Some(g.turns), target);
        let first = same
            .iter()
            .filter(|x| Some(x.turns) == target)
            .map(|x| (x.match_index, x.game_index))
            .min();
        assert_eq!(
            first,
            Some((g.match_index, g.game_index)),
            "ties by match index"
        );
        let name = extreme_replay_name(kind, label, g);
        let file = name.strip_prefix("replays/").expect("in replays/");
        assert!(!gcg_runner::reports::writer::is_replay_file_name(file));
        assert!(
            gcg_runner::reports::pyregex::Regex::new(EXTREME_REPLAY_FILE)
                .expect("pattern")
                .is_match_at_start(file)
        );
    }
    assert_eq!(picks.len(), 4);
    let dir = scratch("w148");
    write(&run, &dir.join("plain"), SetOptions::default()).expect("plain");
    let opts = SetOptions {
        extremes: true,
        ..SetOptions::default()
    };
    write(&run, &dir.join("extremes"), opts).expect("extremes");
    let (plain, extremes) = (tree(&dir.join("plain")), tree(&dir.join("extremes")));
    let extra: Vec<&String> = extremes
        .keys()
        .filter(|k| !plain.contains_key(*k))
        .collect();
    assert_eq!(extra.len(), 4, "{extra:?}");
    for (k, v) in &plain {
        if k != "timing.json" {
            assert_eq!(extremes.get(k), Some(v), "{k}");
        }
    }
    let _ = fs::remove_dir_all(&dir);
}

/// The random test agent, except that building any agent of the matches in `fail` fails.
struct FailingFactory {
    inner: Box<dyn AgentFactory>,
    seeds: Vec<u64>,
}

impl FailingFactory {
    fn new(cfg: &BenchmarkConfig, fail: &[u32]) -> Self {
        let mut seeds = Vec::new();
        for &m in fail {
            for g in 0..3 {
                for seat in 0..2 {
                    seeds.push(agent_seed(cfg.seed, m, g, seat));
                }
            }
        }
        Self {
            inner: random(),
            seeds,
        }
    }
}

impl AgentFactory for FailingFactory {
    fn make(&self, spec: &AgentSpec) -> Result<Box<dyn Agent>, AgentError> {
        if self.seeds.contains(&spec.seed) {
            return Err(AgentError::Value("injected failure".to_owned()));
        }
        self.inner.make(spec)
    }

    fn describe(&self) -> String {
        self.inner.describe()
    }
}

fn checkpoint_matches(path: &Path) -> Vec<u32> {
    let text = fs::read_to_string(path).expect("checkpoint");
    let mut m: Vec<u32> = text
        .lines()
        .skip(1)
        .map(|l| {
            let v: Value = serde_json::from_str(l).expect("a match line");
            u32::try_from(v["match_index"].as_u64().expect("index")).expect("u32")
        })
        .collect();
    m.sort();
    m
}

#[test]
fn w105_a_resumed_run_writes_the_uninterrupted_reports() {
    for workers in [1, 3] {
        let cfg = config(6, 41, Format::Bo3, workers);
        let dir = scratch(&format!("w105-{workers}"));
        let whole = play(&cfg);
        write(&whole, &dir.join("whole"), SetOptions::default()).expect("whole");
        let out = dir.join("resumed");
        let ckpt = out.join(CHECKPOINT_FILE);
        // The "kill": match 3 fails, so the run stops with the matches below it checkpointed.
        let failing = FailingFactory::new(&cfg, &[3]);
        let err = run_resumable(&cfg, &failing, &mut |_, _| {}, &Cancel::new(), &ckpt)
            .expect_err("match 3 fails");
        assert!(
            matches!(&err, ResumeError::Run(RunError::GameFailure(m)) if m.starts_with("match 3 game 1")),
            "{err}"
        );
        let done = checkpoint_matches(&ckpt);
        assert!(
            done.starts_with(&[0, 1, 2]) && !done.contains(&3),
            "{done:?}"
        );
        // A cut-off final line (a kill during a write) is ignored.
        let mut text = fs::read_to_string(&ckpt).expect("checkpoint");
        text.push_str("{\"match_index\":4,\"games\":[");
        fs::write(&ckpt, text).expect("truncated line");
        // Another configuration is refused, naming what differs.
        let mut other = cfg.clone();
        other.seed = 42;
        let refused = run_resumable(&other, &*random(), &mut |_, _| {}, &Cancel::new(), &ckpt)
            .expect_err("a different seed");
        assert!(refused.to_string().contains("differs in seed"), "{refused}");
        let resumed = run_resumable(&cfg, &*random(), &mut |_, _| {}, &Cancel::new(), &ckpt)
            .expect("resumed");
        // The cut-off line was dropped before the resumed matches were appended.
        assert_eq!(checkpoint_matches(&ckpt), (0..6).collect::<Vec<u32>>());
        write(&resumed, &out, SetOptions::default()).expect("resumed reports");
        assert!(!ckpt.exists(), "the finished run's checkpoint is removed");
        assert_eq!(
            without_timing(tree(&out)),
            without_timing(tree(&dir.join("whole"))),
            "{workers} workers"
        );
        let _ = fs::remove_dir_all(&dir);
    }
}

#[test]
fn w149_the_lowest_failing_match_is_reported() {
    let mut cfg = config(10, 51, Format::Bo1, 1);
    for workers in [1, 2, 8] {
        cfg.workers = Workers::Count(workers);
        let both = FailingFactory::new(&cfg, &[3, 5]);
        for _ in 0..50 {
            let err = run_benchmark(&cfg, &both, &mut |_, _| {}, &Cancel::new())
                .expect_err("matches 3 and 5 fail");
            assert!(
                matches!(&err, RunError::GameFailure(m) if m.starts_with("match 3 game 1")),
                "{workers} workers: {err:?}"
            );
        }
        // Only match 5 fails: matches 0 to 4 have finished (they are in the checkpoint) when
        // match 5's error is reported.
        let dir = scratch(&format!("w149-{workers}"));
        let ckpt = dir.join(CHECKPOINT_FILE);
        let five = FailingFactory::new(&cfg, &[5]);
        let err = run_resumable(&cfg, &five, &mut |_, _| {}, &Cancel::new(), &ckpt)
            .expect_err("match 5 fails");
        assert!(err.to_string().contains("match 5 game 1"), "{err}");
        let done = checkpoint_matches(&ckpt);
        assert!(
            done.starts_with(&[0, 1, 2, 3, 4]),
            "{workers} workers: {done:?}"
        );
        let _ = fs::remove_dir_all(&dir);
    }
}

/// A factory defined by a test binary: shared by every worker thread with no guard (W-107).
struct CountingFactory {
    built: std::sync::atomic::AtomicU64,
}

impl AgentFactory for CountingFactory {
    fn make(&self, spec: &AgentSpec) -> Result<Box<dyn Agent>, AgentError> {
        self.built
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        random().make(spec)
    }

    fn describe(&self) -> String {
        "a test-binary factory".to_owned()
    }
}

#[test]
fn w107_a_custom_factory_runs_on_ten_workers() {
    let cfg = config(20, 61, Format::Bo1, 10);
    let f = CountingFactory {
        built: std::sync::atomic::AtomicU64::new(0),
    };
    let run = run_benchmark(&cfg, &f, &mut |_, _| {}, &Cancel::new()).expect("runs");
    assert_eq!(run.timing.workers, 10);
    assert_eq!(f.built.load(std::sync::atomic::Ordering::Relaxed), 40);
    assert_eq!(run.agent_factory, "a test-binary factory");
}

#[test]
fn w108_no_crate_reads_an_agent_factory_variable() {
    let mut reads = Vec::new();
    for crate_dir in fs::read_dir(root().join("crates")).expect("crates") {
        let src = crate_dir.expect("entry").path().join("src");
        let mut stack = vec![src];
        while let Some(d) = stack.pop() {
            let Ok(entries) = fs::read_dir(&d) else {
                continue;
            };
            for e in entries {
                let p = e.expect("entry").path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().is_some_and(|x| x == "rs") {
                    let text = fs::read_to_string(&p).expect("source");
                    for (i, line) in text.lines().enumerate() {
                        if line.contains("env::var") || line.contains("var_os(") {
                            reads.push((p.clone(), i + 1, line.trim().to_owned()));
                        }
                    }
                }
            }
        }
    }
    for (path, line, text) in &reads {
        let allowed = text.contains("\"COLUMNS\"") || text.contains("\"GCG_REPO_ROOT\"");
        assert!(allowed, "{}:{line}: {text}", path.display());
    }
    assert!(reads.iter().any(|r| r.2.contains("\"COLUMNS\"")));
}

/// The `.rs` files under `dir`, recursively, with their text.
fn sources(dir: &Path) -> Vec<(PathBuf, String)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = fs::read_dir(&d) else {
            continue;
        };
        for e in entries {
            let p = e.expect("entry").path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "rs") {
                let text = fs::read_to_string(&p).expect("source");
                out.push((p, text));
            }
        }
    }
    out
}

#[test]
fn w93_every_rate_tool_uses_the_one_wins_only_interval() {
    // One Wilson interval in the workspace (`reports/stats.py:12-25`), which counts the
    // successes it is given: every caller passes deck-under-test (or agent) wins, so draws are
    // non-wins everywhere. The rate-producing tools all call it.
    let mut definitions = Vec::new();
    let mut callers = std::collections::BTreeSet::new();
    for (path, text) in sources(&root().join("crates")) {
        let rel = path
            .strip_prefix(root().join("crates"))
            .expect("inside")
            .to_string_lossy()
            .into_owned();
        if rel.contains("/tests/") {
            continue;
        }
        if text.contains("fn wilson(") || text.contains("fn wilson_interval(") {
            definitions.push(rel.clone());
        }
        if text.contains("wilson(") && rel.contains("/src/") {
            callers.insert(rel);
        }
    }
    assert_eq!(definitions, ["gcg-runner/src/reports/stats.rs"]);
    for tool in [
        "gcg-runner/src/analysis.rs",
        "gcg-eval/src/stats.rs",
        "gcg-eval/src/compare_runs.rs",
        "gcg-eval/src/gates.rs",
    ] {
        assert!(
            callers.contains(tool),
            "{tool} calls the shared interval: {callers:?}"
        );
    }
    // A run with draws: the analysis's rate counts them as non-wins, as results.json does.
    let doc = gcg_runner::analysis::wins_only(3, 2, 10);
    let rate = gcg_runner::reports::stats::Rate::new(3, 10);
    assert_eq!(doc.get("rate"), rate.to_json().get("rate"));
    assert_eq!(doc.get("draw_rate"), Some(&Json::Float(0.2)));
}

#[test]
fn w91_runs_of_different_ais_are_not_compared() {
    let doc = |factory: &str, agent: &str, digest: &str| -> Value {
        serde_json::json!({
            "ai": {"factory": factory, "preset": "standard", "dut_agent": agent,
                   "bench_agent": agent, "decision_log": false},
            "versions": {"package": "0.1.0", "code_digest": digest, "results_schema": "1.1.0",
                         "dataset_version": "d1"}
        })
    };
    let strengthened = doc(
        "gcg-search::make_agent('standard')",
        "search-standard",
        "aa",
    );
    let reference = doc(
        "gcg-ai-reference::make_agent('mcts')",
        "mcts-standard",
        "aa",
    );
    let err =
        check_comparable(&strengthened, &reference, ["A", "B"], false).expect_err("different AIs");
    assert!(
        err.contains("ai.factory: \"gcg-search::make_agent('standard')\" vs"),
        "{err}"
    );
    assert!(
        err.contains("ai.dut_agent") && err.contains("--allow-mixed"),
        "{err}"
    );
    assert_eq!(
        check_comparable(&strengthened, &strengthened.clone(), ["A", "B"], false),
        Ok(Vec::new())
    );
    let rebuilt = doc(
        "gcg-search::make_agent('standard')",
        "search-standard",
        "bb",
    );
    assert!(
        check_comparable(&strengthened, &rebuilt, ["A", "B"], false)
            .expect_err("another build")
            .contains("versions.code_digest")
    );
    let notes = check_comparable(&strengthened, &reference, ["A", "B"], true).expect("allowed");
    assert!(
        notes[0].contains("--allow-mixed given") && notes.len() == 4,
        "{notes:?}"
    );
}
