//! Every report file regenerated from the records that `port/oracle/export_records.py` exported
//! from real Python benchmark runs (random test agent and the reference `fast` preset; BO1 and
//! BO3; decision log on and off; turn-limit draws; no replays; a quoted deck path; a seed of
//! `2**64 - 1`) and from the hand-built runs of tests/reports, compared byte for byte with the
//! files Python wrote (docs/port/PLAN.md §10.4 E gate, first item).
//!
//! The inputs are `games.ndjson` (read back with `GameRecord::from_json`), the match grouping,
//! the config and the gcg-data values of `run.json`; the frozen entry points (`build_results`,
//! `write_reports`) read the embedded gcg-data values instead, and the tests check that they
//! equal the exported ones. Replay documents take the re-simulation (log and winner seat) from
//! Python's replay file, because the engine is not ported yet; every other field of the replay
//! is rebuilt from the record.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use gcg_core::pool::Cancel;
use gcg_core::pyjson::Json;
use gcg_core::sys::topology::{PerfLevel, Topology, TopologySource};
use gcg_data::{CardCount, CardType, Color, Deck};
use gcg_engine::{Action, ActionKind};
use gcg_runner::reports::{
    CardFacts, Resimulation, ResultsData, TimingHost, build_results, build_results_with,
    games_ndjson, json_from_value, render_summary, replay_document_with, replay_name,
    reproduce_command, select_replays, timing_json, to_json_text, validate_results, write_reports,
    write_reports_with,
};
use gcg_runner::{
    BenchmarkConfig, BenchmarkRun, Format, GameRecord, MatchRecord, Outcome, ReplayError,
    ReportError, RunTiming, SchemaVersion, Workers,
};
use serde_json::Value;

fn vectors() -> PathBuf {
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

fn u64_of(v: &Value) -> u64 {
    v.as_u64().expect("an unsigned integer")
}

fn card_counts(v: &Value) -> Box<[CardCount]> {
    v.as_array()
        .expect("a count list")
        .iter()
        .map(|pair| CardCount {
            card_number: str_of(&pair[0]).into(),
            count: u128::from(u64_of(&pair[1])),
        })
        .collect()
}

fn deck(v: &Value) -> Deck {
    Deck {
        name: str_of(&v["name"]).into(),
        main: card_counts(&v["main"]),
        resources: card_counts(&v["resources"]),
        entries: Box::new([]),
        source: v["source"].as_str().map(Box::from),
        digest: str_of(&v["digest"]).to_owned(),
    }
}

fn topology() -> Topology {
    Topology {
        levels: vec![
            PerfLevel {
                name: "Performance".to_owned(),
                logical_cpus: 6,
                physical_cpus: 6,
            },
            PerfLevel {
                name: "Efficiency".to_owned(),
                logical_cpus: 4,
                physical_cpus: 4,
            },
        ],
        logical_cpus: 10,
        cache_line_bytes: Some(128),
        arm_features: Vec::new(),
        source: TopologySource::Sysctl,
    }
}

struct Fixture {
    id: String,
    kind: String,
    dir: PathBuf,
    run: BenchmarkRun,
    data: ResultsData,
    host: TimingHost,
}

fn outcome(s: &str) -> Outcome {
    match s {
        "dut" => Outcome::Dut,
        "bench" => Outcome::Bench,
        "draw" => Outcome::Draw,
        other => panic!("not an outcome: {other}"),
    }
}

fn load(id: &str, kind: &str) -> Fixture {
    let dir = vectors().join(id);
    let spec = read_json(&dir.join("run.json"));
    let c = &spec["config"];
    let versions = &spec["report_data"]["versions"];
    let config = BenchmarkConfig {
        deck_under_test: deck(&c["deck_under_test"]),
        benchmark_deck: deck(&c["benchmark_deck"]),
        matches: u32::try_from(u64_of(&c["matches"])).unwrap(),
        seed: u64_of(&c["seed"]),
        workers: Workers::Count(u32::try_from(u64_of(&c["workers"])).unwrap()),
        format: Format::from_value(str_of(&c["fmt"])).expect("bo1 or bo3"),
        ai_preset: leak(str_of(&c["ai_preset"])),
        out_dir: PathBuf::from("gcg-sim-out"),
        decision_log: c["decision_log"].as_bool().unwrap(),
        replays: u32::try_from(u64_of(&c["replays"])).unwrap(),
        turn_limit: i32::try_from(c["turn_limit"].as_i64().unwrap()).unwrap(),
        max_actions: c["max_actions"].as_i64().unwrap(),
        schema: SchemaVersion::V1_1,
        package: leak(str_of(&versions["package"])),
        code_digest: leak(str_of(&versions["code_digest"])),
    };
    let ndjson = fs::read_to_string(dir.join("games.ndjson")).unwrap();
    let mut games: Vec<GameRecord> = ndjson
        .lines()
        .map(|line| {
            let v: Value = serde_json::from_str(line).unwrap();
            GameRecord::from_json(&v).unwrap_or_else(|e| panic!("{id}: {e}"))
        })
        .collect();
    games.reverse();
    let matches: Vec<MatchRecord> = spec["matches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| {
            let indices = m["games"].as_array().unwrap();
            let mut own = Vec::new();
            for gi in indices {
                let g = games.pop().expect("a game per index");
                assert_eq!(u64::from(g.game_index), u64_of(gi), "{id}: game order");
                own.push(g);
            }
            MatchRecord {
                match_index: u32::try_from(u64_of(&m["match_index"])).unwrap(),
                dut_seat: u8::try_from(u64_of(&m["dut_seat"])).unwrap(),
                games: own,
                winner: outcome(str_of(&m["winner"])),
                dut_points: u32::try_from(u64_of(&m["dut_points"])).unwrap(),
                bench_points: u32::try_from(u64_of(&m["bench_points"])).unwrap(),
            }
        })
        .collect();
    assert!(games.is_empty(), "{id}: every game belongs to a match");
    let t = &spec["timing"];
    let timing = RunTiming {
        started_at: str_of(&t["started_at"]).to_owned(),
        finished_at: str_of(&t["finished_at"]).to_owned(),
        wall_seconds: t["wall_seconds"].as_f64().unwrap(),
        match_seconds: t["match_seconds"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s.as_f64().unwrap())
            .collect(),
        workers: u32::try_from(u64_of(&t["workers"])).unwrap(),
        topology: topology(),
    };
    let cards = spec["report_data"]["cards"].as_object().unwrap().clone();
    let data = ResultsData::new(
        &config,
        json_from_value(versions),
        json_from_value(&spec["report_data"]["conflict_resolutions"]),
        |n| {
            cards.get(n).map(|f| CardFacts {
                name: str_of(&f["name"]).into(),
                card_type: CardType::from_value(str_of(&f["card_type"])).expect("a card type"),
                level: i32::try_from(f["level"].as_i64().unwrap()).unwrap(),
                cost: i32::try_from(f["cost"].as_i64().unwrap()).unwrap(),
                color: f["color"]
                    .as_str()
                    .map(|c| Color::from_value(c).expect("a color")),
            })
        },
    )
    .expect("the exported card facts cover both decks");
    let host = TimingHost {
        python: str_of(&spec["host"]["python"]).to_owned(),
        platform: str_of(&spec["host"]["platform"]).to_owned(),
    };
    Fixture {
        id: id.to_owned(),
        kind: kind.to_owned(),
        dir,
        run: BenchmarkRun {
            config,
            matches,
            timing,
            agent_factory: str_of(&spec["agent_factory"]).to_owned(),
        },
        data,
        host,
    }
}

fn fixtures() -> Vec<Fixture> {
    let index = read_json(&vectors().join("index.json"));
    index["runs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| load(str_of(&r["id"]), str_of(&r["kind"])))
        .collect()
}

fn python_text(f: &Fixture, name: &str) -> String {
    fs::read_to_string(f.dir.join(name)).unwrap_or_else(|e| panic!("{}/{name}: {e}", f.id))
}

/// Line-level first difference, so a failure names the run, file and line.
#[track_caller]
fn assert_same_text(run: &str, file: &str, ours: &str, python: &str) {
    if ours == python {
        return;
    }
    let line = ours
        .lines()
        .zip(python.lines())
        .position(|(a, b)| a != b)
        .unwrap_or_else(|| ours.lines().count().min(python.lines().count()));
    panic!(
        "{run}/{file} differs from Python's at line {}:\n  rust:   {:?}\n  python: {:?}\n  (lengths {} vs {})",
        line + 1,
        ours.lines().nth(line),
        python.lines().nth(line),
        ours.len(),
        python.len()
    );
}

fn without_topology(doc: &Json) -> Json {
    match doc {
        Json::Object(items) => Json::Object(
            items
                .iter()
                .filter(|(k, _)| *k != "topology".into())
                .cloned()
                .collect(),
        ),
        other => other.clone(),
    }
}

/// The re-simulation recorded in Python's replay file.
fn python_resimulation(doc: &Value) -> Resimulation {
    Resimulation {
        winner_seat: doc["expected"]["winner_seat"]
            .as_i64()
            .map(|w| i8::try_from(w).unwrap()),
        log: doc["log"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| str_of(l).to_owned())
            .collect(),
    }
}

#[test]
fn the_export_covers_every_report_path() {
    let all = fixtures();
    assert_eq!(all.iter().filter(|f| f.kind == "benchmark").count(), 10);
    assert_eq!(all.iter().filter(|f| f.kind == "synthetic").count(), 9);
    let kinds: BTreeSet<String> = all
        .iter()
        .flat_map(|f| {
            let r = build_results_with(&f.run, &f.data);
            match r.get("hypotheses") {
                Some(Json::Array(hs)) => hs
                    .iter()
                    .filter_map(|h| match h.get("kind") {
                        Some(Json::Str(k)) => Some(k.clone()),
                        _ => None,
                    })
                    .collect::<Vec<String>>(),
                _ => Vec::new(),
            }
        })
        .collect();
    assert_eq!(kinds.len(), 5, "every hypothesis kind occurs: {kinds:?}");
    assert!(
        all.iter()
            .any(|f| f.run.config.decision_log && f.kind == "benchmark")
    );
    assert!(all.iter().any(|f| f.run.config.format == Format::Bo1));
    assert!(all.iter().any(|f| f.run.config.replays == 0));
    assert!(
        all.iter()
            .any(|f| f.run.games().any(|g| g.winner == Outcome::Draw))
    );
}

#[test]
fn games_ndjson_is_pythons() {
    for f in fixtures() {
        assert_same_text(
            &f.id,
            "games.ndjson",
            &games_ndjson(&f.run),
            &python_text(&f, "games.ndjson"),
        );
    }
}

#[test]
fn results_json_is_pythons_and_validates() {
    for f in fixtures() {
        let results = build_results_with(&f.run, &f.data);
        validate_results(&results, SchemaVersion::V1_1).unwrap_or_else(|e| panic!("{}: {e}", f.id));
        assert_same_text(
            &f.id,
            "results.json",
            &to_json_text(&results),
            &python_text(&f, "results.json"),
        );
    }
}

#[test]
fn summary_md_is_pythons() {
    for f in fixtures() {
        let results = build_results_with(&f.run, &f.data);
        let text = render_summary(&results, &reproduce_command(&f.run));
        assert_same_text(&f.id, "summary.md", &text, &python_text(&f, "summary.md"));
    }
}

#[test]
fn timing_json_is_pythons_plus_the_topology() {
    for f in fixtures().into_iter().filter(|f| f.kind == "benchmark") {
        let doc = timing_json(&f.run, &f.host);
        assert!(matches!(doc.get("topology"), Some(Json::Object(_))));
        let text = to_json_text(&without_topology(&doc));
        assert_same_text(&f.id, "timing.json", &text, &python_text(&f, "timing.json"));
    }
}

#[test]
fn replay_documents_are_pythons() {
    let mut checked = 0;
    for f in fixtures().into_iter().filter(|f| f.kind == "benchmark") {
        let mut expected: BTreeSet<String> = BTreeSet::new();
        let dir = f.dir.join("replays");
        if dir.is_dir() {
            for e in fs::read_dir(&dir).unwrap() {
                expected.insert(format!(
                    "replays/{}",
                    e.unwrap().file_name().to_str().unwrap()
                ));
            }
        }
        let selected = select_replays(&f.run);
        let names: BTreeSet<String> = selected.iter().map(|(l, g)| replay_name(l, g)).collect();
        assert_eq!(names, expected, "{}: selected replays", f.id);
        for (label, record) in selected {
            let name = replay_name(label, record);
            let python = python_text(&f, &name);
            let sim = python_resimulation(&serde_json::from_str(&python).unwrap());
            let doc = replay_document_with(record, &f.run.config, label, &sim);
            assert_same_text(&f.id, &name, &to_json_text(&doc), &python);
            checked += 1;
        }
    }
    assert!(checked >= 20, "only {checked} replays compared");
}

fn tmp_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("reports-{name}-{}", std::process::id()));
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    dir
}

fn tree(root: &Path) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in fs::read_dir(&d).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                stack.push(p);
            } else {
                let rel = p
                    .strip_prefix(root)
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .replace('\\', "/");
                out.push((rel, fs::read(&p).unwrap()));
            }
        }
    }
    out.sort();
    out
}

#[test]
fn write_reports_writes_pythons_files() {
    for f in fixtures().into_iter().filter(|f| f.kind == "benchmark") {
        let out = tmp_dir(&f.id);
        let dir = f.dir.clone();
        let mut replay_doc = |label: &'static str, record: &GameRecord| {
            let python = read_json(&dir.join(replay_name(label, record)));
            Ok(replay_document_with(
                record,
                &f.run.config,
                label,
                &python_resimulation(&python),
            ))
        };
        let paths = write_reports_with(
            &f.run,
            &out,
            &Cancel::new(),
            &f.data,
            &mut replay_doc,
            &f.host,
        )
        .unwrap_or_else(|e| panic!("{}: {e}", f.id));
        assert_eq!(paths.results, out.join("results.json"));
        assert_eq!(paths.replays.len(), select_replays(&f.run).len());
        let ours = tree(&out);
        let mut python: Vec<(String, Vec<u8>)> = tree(&f.dir)
            .into_iter()
            .filter(|(name, _)| name != "run.json")
            .collect();
        python.sort();
        let ours_names: Vec<&String> = ours.iter().map(|(n, _)| n).collect();
        let python_names: Vec<&String> = python.iter().map(|(n, _)| n).collect();
        assert_eq!(
            ours_names, python_names,
            "{}: file set (no temporary files left)",
            f.id
        );
        for ((name, a), (_, b)) in ours.iter().zip(&python) {
            let a = String::from_utf8(a.clone()).unwrap();
            let b = String::from_utf8(b.clone()).unwrap();
            if name == "timing.json" {
                let doc = json_from_value(&serde_json::from_str::<Value>(&a).unwrap());
                assert_same_text(&f.id, name, &to_json_text(&without_topology(&doc)), &b);
            } else {
                assert_same_text(&f.id, name, &a, &b);
            }
        }
        fs::remove_dir_all(&out).unwrap();
    }
}

#[test]
fn rewriting_removes_stale_replays_but_keeps_other_files() {
    let f = load("random-bo3-s3", "benchmark");
    let out = tmp_dir("stale");
    fs::create_dir_all(out.join("replays")).unwrap();
    for name in [
        "win-m9999-g1.json",
        "draw-m0000-g3.json\n",
        "win-m\u{0660}\u{0660}\u{0660}\u{0660}-g1.json",
    ] {
        fs::write(out.join("replays").join(name), "{}").unwrap();
    }
    fs::write(out.join("replays/notes.txt"), "keep me").unwrap();
    fs::write(out.join("replays/win-m000-g1.json"), "not a replay name").unwrap();
    let dir = f.dir.clone();
    let mut replay_doc = |label: &'static str, record: &GameRecord| {
        let python = read_json(&dir.join(replay_name(label, record)));
        Ok(replay_document_with(
            record,
            &f.run.config,
            label,
            &python_resimulation(&python),
        ))
    };
    let paths = write_reports_with(
        &f.run,
        &out,
        &Cancel::new(),
        &f.data,
        &mut replay_doc,
        &f.host,
    )
    .unwrap();
    assert_eq!(paths.replays.len(), 4);
    let left: BTreeSet<String> = fs::read_dir(out.join("replays"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_str().unwrap().to_owned())
        .collect();
    let mut want: BTreeSet<String> = paths
        .replays
        .iter()
        .map(|p| p.file_name().unwrap().to_str().unwrap().to_owned())
        .collect();
    want.insert("notes.txt".to_owned());
    want.insert("win-m000-g1.json".to_owned());
    assert_eq!(left, want);
    assert_eq!(
        fs::read_to_string(out.join("replays/notes.txt")).unwrap(),
        "keep me"
    );
    fs::remove_dir_all(&out).unwrap();
}

#[test]
fn an_invalid_document_writes_nothing() {
    let mut f = load("fast-bo1-s3", "benchmark");
    f.run.config.schema = SchemaVersion::V1_2;
    let out = tmp_dir("invalid");
    let mut never = |_: &'static str, _: &GameRecord| -> Result<Json, ReplayError> {
        panic!("no replay is built")
    };
    let err =
        write_reports_with(&f.run, &out, &Cancel::new(), &f.data, &mut never, &f.host).unwrap_err();
    assert!(matches!(err, gcg_runner::ReportError::Schema(_)), "{err}");
    assert!(!out.exists());
    let mut f = load("fast-bo1-s3", "benchmark");
    f.run.matches.clear();
    let results = build_results_with(&f.run, &f.data);
    let e = validate_results(&results, SchemaVersion::V1_1).unwrap_err();
    assert_eq!(e.pointer, "/ai/dut_agent");
}

#[test]
fn a_card_unknown_to_the_lookup_is_reported() {
    let f = load("fast-bo1-s3", "benchmark");
    let first = f.run.config.deck_under_test.main[0].card_number.clone();
    let missing = ResultsData::new(&f.run.config, Json::Null, Json::Null, |_| None).unwrap_err();
    assert_eq!(missing.0, first);
    assert_eq!(
        missing.to_string(),
        format!("card {first} is not in the card database")
    );
}

#[test]
fn results_ignore_timing_and_round_trip_records() {
    let mut f = load("random-bo1-s11", "benchmark");
    let before = to_json_text(&build_results_with(&f.run, &f.data));
    f.run.timing.started_at = "2000-01-01T00:00:00+00:00".to_owned();
    f.run.timing.wall_seconds = 1.0;
    f.run.timing.match_seconds.clear();
    f.run.timing.workers = 7;
    assert_eq!(before, to_json_text(&build_results_with(&f.run, &f.data)));
    let timing = timing_json(&f.run, &TimingHost::rust());
    assert_eq!(timing.get("mean_match_seconds"), Some(&Json::Null));
    assert_eq!(
        timing.get("python"),
        Some(&Json::Str("none (Rust build)".to_owned()))
    );
    for g in f.run.games() {
        let line = gcg_core::pyjson::dumps(
            &g.to_json(),
            gcg_core::pyjson::DumpOptions::COMPACT_SORTED_ASCII,
        );
        let back = GameRecord::from_json(&serde_json::from_str(&line).unwrap()).unwrap();
        assert_eq!(&back, g);
    }
}

/// The output files of a report directory as `(name, text)`, without Python's `run.json`.
fn texts(root: &Path) -> Vec<(String, String)> {
    tree(root)
        .into_iter()
        .filter(|(name, _)| name != "run.json")
        .map(|(name, bytes)| (name, String::from_utf8(bytes).unwrap()))
        .collect()
}

/// `doc` without the keys a Rust build writes differently on purpose.
fn without(doc: &Json, keys: &[&str]) -> Json {
    match doc {
        Json::Object(items) => Json::Object(
            items
                .iter()
                .filter(|(k, _)| !keys.iter().any(|key| *k == (*key).into()))
                .cloned()
                .collect(),
        ),
        other => other.clone(),
    }
}

/// The re-simulation of a record, read from Python's replay file of its label.
fn python_resimulator(f: &Fixture) -> impl FnMut(&GameRecord) -> Result<Resimulation, ReplayError> {
    let labels: BTreeMap<(u32, u32), &'static str> = select_replays(&f.run)
        .into_iter()
        .map(|(label, g)| ((g.match_index, g.game_index), label))
        .collect();
    let dir = f.dir.clone();
    move |record: &GameRecord| {
        let label = labels[&(record.match_index, record.game_index)];
        let python = read_json(&dir.join(replay_name(label, record)));
        Ok(python_resimulation(&python))
    }
}

#[test]
fn the_embedded_data_is_the_exported_data_and_build_results_is_pythons() {
    for f in fixtures() {
        let embedded =
            ResultsData::embedded(&f.run.config).unwrap_or_else(|e| panic!("{}: {e}", f.id));
        // run.json was written with sorted keys, so compare as every report writes them.
        assert_same_text(
            &f.id,
            "versions",
            &to_json_text(embedded.versions()),
            &to_json_text(f.data.versions()),
        );
        assert_same_text(
            &f.id,
            "conflict_resolutions",
            &to_json_text(embedded.conflict_resolutions()),
            &to_json_text(f.data.conflict_resolutions()),
        );
        let c = &f.run.config;
        for cc in [&c.deck_under_test, &c.benchmark_deck]
            .into_iter()
            .flat_map(|d| d.main.iter().chain(d.resources.iter()))
        {
            let n = &*cc.card_number;
            assert_eq!(embedded.card(n), f.data.card(n), "{}: card {n}", f.id);
        }
        let results = build_results(&f.run);
        validate_results(&results, SchemaVersion::V1_1).unwrap_or_else(|e| panic!("{}: {e}", f.id));
        assert_same_text(
            &f.id,
            "results.json",
            &to_json_text(&results),
            &python_text(&f, "results.json"),
        );
    }
}

#[test]
fn write_reports_writes_pythons_files_from_the_embedded_data() {
    let rust = TimingHost::rust();
    for f in fixtures().into_iter().filter(|f| f.kind == "benchmark") {
        let out = tmp_dir(&format!("embedded-{}", f.id));
        let mut resimulate = python_resimulator(&f);
        let paths = write_reports(&f.run, &out, &Cancel::new(), &mut resimulate)
            .unwrap_or_else(|e| panic!("{}: {e}", f.id));
        assert_eq!(paths.replays.len(), select_replays(&f.run).len());
        let ours = texts(&out);
        let python = texts(&f.dir);
        let names = |t: &[(String, String)]| t.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>();
        assert_eq!(names(&ours), names(&python), "{}: file set", f.id);
        for ((name, a), (_, b)) in ours.iter().zip(&python) {
            if name != "timing.json" {
                assert_same_text(&f.id, name, a, b);
                continue;
            }
            // timing.json: Python's values except the two host strings, plus the topology.
            let doc = json_from_value(&serde_json::from_str::<Value>(a).unwrap());
            assert_eq!(doc.get("python"), Some(&Json::Str(rust.python.clone())));
            assert_eq!(doc.get("platform"), Some(&Json::Str(rust.platform.clone())));
            assert!(matches!(doc.get("topology"), Some(Json::Object(_))));
            let python_doc = json_from_value(&serde_json::from_str::<Value>(b).unwrap());
            assert_same_text(
                &f.id,
                name,
                &to_json_text(&without(&doc, &["python", "platform", "topology"])),
                &to_json_text(&without(&python_doc, &["python", "platform"])),
            );
        }
        fs::remove_dir_all(&out).unwrap();
    }
}

fn keys(v: Option<&Json>) -> Vec<String> {
    match v {
        Some(Json::Object(items)) => items
            .iter()
            .map(|(k, _)| match k {
                gcg_core::pyjson::Key::Str(s) => s.clone(),
                other => panic!("not a string key: {other:?}"),
            })
            .collect(),
        other => panic!("not an object: {other:?}"),
    }
}

#[test]
fn timing_json_names_the_rust_build_and_the_topology() {
    let rust = TimingHost::rust();
    assert_eq!(rust.python, "none (Rust build)");
    assert_eq!(
        rust.platform,
        format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
    );
    let f = load("fast-bo3-s9", "benchmark");
    let doc = timing_json(&f.run, &rust);
    // Python's keys in Python's order (reports/writer.py:60-76), then the topology.
    assert_eq!(
        keys(Some(&doc)),
        [
            "note",
            "started_at",
            "finished_at",
            "wall_seconds",
            "workers",
            "matches",
            "games",
            "match_seconds",
            "mean_match_seconds",
            "games_per_second",
            "python",
            "platform",
            "topology"
        ]
    );
    let topology = doc.get("topology");
    assert_eq!(
        keys(topology),
        ["levels", "logical_cpus", "cache_line_bytes", "source"]
    );
    let topology = topology.unwrap();
    assert_eq!(topology.get("logical_cpus"), Some(&Json::Int(10)));
    assert_eq!(topology.get("cache_line_bytes"), Some(&Json::Int(128)));
    assert_eq!(
        topology.get("source"),
        Some(&Json::Str("sysctl".to_owned()))
    );
    let Some(Json::Array(levels)) = topology.get("levels") else {
        panic!("levels is a list");
    };
    assert_eq!(levels.len(), 2);
    assert_eq!(
        levels[0].get("name"),
        Some(&Json::Str("Performance".to_owned()))
    );
    assert_eq!(levels[1].get("logical_cpus"), Some(&Json::Int(4)));
}

#[test]
fn a_failed_resimulation_stops_the_reports() {
    let f = load("fast-bo3-log", "benchmark");
    let selected = select_replays(&f.run);
    assert_eq!(selected.len(), 2, "the fixture saves two replays");
    let out = tmp_dir("failed-resimulation");
    let illegal = ReplayError::Illegal {
        index: 7,
        action: Action::new(ActionKind::Attack, 1_000_000, 1_000_000, -1),
    };
    let mut good = python_resimulator(&f);
    let mut calls = 0;
    let mut resimulate = |record: &GameRecord| {
        calls += 1;
        if calls == 2 {
            Err(illegal.clone())
        } else {
            good(record)
        }
    };
    let err = write_reports(&f.run, &out, &Cancel::new(), &mut resimulate).unwrap_err();
    assert_eq!(
        err.to_string(),
        "cannot build a replay document: action 7 attack(1000000,1000000) is not legal in the \
         replayed game"
    );
    assert!(matches!(&err, ReportError::Replay(e) if *e == illegal));
    assert!(std::error::Error::source(&err).is_some());
    // The replay before the failure is written; nothing after it, results.json included.
    let written: Vec<String> = texts(&out).into_iter().map(|(name, _)| name).collect();
    assert_eq!(written, [replay_name(selected[0].0, selected[0].1)]);
    fs::remove_dir_all(&out).unwrap();
}
