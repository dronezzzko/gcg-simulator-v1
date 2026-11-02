//! P6-M's `gcg-eval` tools end to end (docs/port/PLAN.md §12.2, A30), through the binary:
//!
//! - T-153: `gcg-eval benchmark` without an opt-in option writes `gcg-sim benchmark`'s bytes
//!   (the shared `cli_support::benchmark_command` pipeline with `gcg-sim`'s factory, run in
//!   process), and ignores `GCG_SIM_AGENT_FACTORY` (W-108); the summary's reproduce command
//!   re-runs to the same `results.json` (W-100, file decks);
//! - the opt-in options add files and leave every reference report byte-identical: `--analysis`
//!   (and `--swiss-draws`), `--compress` (W-99, `gzip -dc`), `--replays-extremes` (W-148),
//!   `--diagnostics` (W-46);
//! - W-105: SIGINT during a `--resume` run, then a resume, gives the uninterrupted reports; a
//!   resume with another configuration is refused;
//! - W-148: `replay-game` verifies every game of a run and rejects a tampered record;
//! - W-156: `replay-check` prints `gcg-sim replay`'s output when the versions match and names
//!   the differing version otherwise;
//! - W-91: `compare-runs` refuses runs of different AIs unless `--allow-mixed`.
//!
//! The benchmark runs play the shipped `fast` preset (2 BO1 matches each).

use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::OnceLock;

use gcg_core::pool::Cancel;
use gcg_eval::benchmark_opts::{SIM_SCHEMA, ShippedFactory, compression};
use gcg_eval::build_info::{CODE_DIGEST, PACKAGE_VERSION};
use gcg_runner::argparse::PyStr;
use gcg_runner::cli_support::{BenchmarkRequest, benchmark_command, replay_command};
use gcg_runner::{AgentFactory, Format, Workers};
use gcg_search::{OpponentList, Preset};
use serde_json::Value;

fn root() -> PathBuf {
    fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")).expect("repo root")
}

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("p6m-cli-{name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn s(p: &Path) -> &str {
    p.to_str().expect("UTF-8 path")
}

fn gcg_eval(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_gcg-eval"))
        .args(args)
        .current_dir(root())
        .env_remove("COLUMNS")
        .output()
        .expect("gcg-eval runs")
}

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

fn ok(args: &[&str]) -> Output {
    let o = gcg_eval(args);
    assert_eq!(o.status.code(), Some(0), "{args:?}: {}", text(&o.stderr));
    o
}

fn deck(name: &str) -> String {
    s(&root().join("examples/decks").join(name)).to_owned()
}

/// The benchmark arguments every run here shares.
fn bench_args(out: &Path) -> Vec<String> {
    [
        deck("red-green-zeon.txt"),
        deck("blue-white-federation.txt"),
        "--matches".to_owned(),
        "2".to_owned(),
        "--seed".to_owned(),
        "5".to_owned(),
        "--format".to_owned(),
        "bo1".to_owned(),
        "--ai-preset".to_owned(),
        "fast".to_owned(),
        "--replays".to_owned(),
        "1".to_owned(),
        "--out".to_owned(),
        s(out).to_owned(),
    ]
    .to_vec()
}

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

fn without(mut t: BTreeMap<String, Vec<u8>>, names: &[&str]) -> BTreeMap<String, Vec<u8>> {
    for n in names {
        t.remove(*n);
    }
    t
}

/// The `gcg-sim benchmark` pipeline run in process with `gcg-sim`'s factory and schema: the
/// baseline every comparison uses, with its stdout and stderr.
fn baseline() -> &'static (PathBuf, String, String) {
    static BASE: OnceLock<(PathBuf, String, String)> = OnceLock::new();
    BASE.get_or_init(|| {
        let out = scratch("baseline").join("reports");
        let args = bench_args(&out);
        let request = BenchmarkRequest {
            deck_under_test: &PyStr::from(args[0].as_str()),
            benchmark_deck: &PyStr::from(args[1].as_str()),
            matches: 2,
            seed: 5,
            workers: Workers::Count(1),
            format: Format::Bo1,
            ai_preset: Preset::Fast.as_str(),
            out: &PyStr::from(s(&out)),
            decision_log: false,
            replays: 1,
            schema: SIM_SCHEMA,
            package: PACKAGE_VERSION,
            code_digest: CODE_DIGEST,
        };
        let (mut o, mut e) = (Vec::new(), Vec::new());
        let factory = ShippedFactory::new(Preset::Fast, None);
        benchmark_command(&request, &factory, false, &Cancel::new(), &mut o, &mut e)
            .expect("the gcg-sim pipeline runs");
        (out, text(&o), text(&e))
    })
}

#[test]
fn the_shipped_factory_is_gcg_sims() {
    for p in Preset::ALL {
        let f = ShippedFactory::new(p, None);
        assert_eq!(f.describe(), p.factory_description());
        assert_eq!(
            gcg_runner::reports::writer::release_factory(p.as_str()),
            p.factory_description()
        );
    }
    let archetype = ShippedFactory::new(Preset::Fast, Some(OpponentList::Archetype));
    assert_eq!(
        archetype.describe(),
        "gcg-search::make_agent('fast', opponent_list='archetype')"
    );
    assert_eq!(
        ShippedFactory::new(Preset::Fast, Some(OpponentList::Known)).describe(),
        Preset::Fast.factory_description()
    );
}

/// W-134's `--opponent-list` is plumbed to the strengthened search's `opponent_list` flag; the
/// `archetype` mechanism is P6-C's, so this build refuses it instead of ignoring it.
#[test]
fn w134_opponent_list_reaches_the_search_flag() {
    let dir = scratch("opponent-list");
    let out = dir.join("reports");
    let args = bench_args(&out);
    let mut cmd: Vec<&str> = vec!["benchmark"];
    cmd.extend(args.iter().map(String::as_str));
    cmd.extend(["--opponent-list", "archetype"]);
    let o = gcg_eval(&cmd);
    assert_eq!(o.status.code(), Some(1));
    let err = text(&o.stderr);
    assert!(
        err.contains("mechanism opponent_list (P6-C) is not available in this build"),
        "{err}"
    );
    assert!(!out.join("results.json").exists());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn t153_benchmark_writes_gcg_sims_bytes_and_ignores_the_factory_variable() {
    let (base, base_out, base_err) = baseline();
    let dir = scratch("t153");
    let out = dir.join("reports");
    let args = bench_args(&out);
    let mut cmd: Vec<&str> = vec!["benchmark"];
    cmd.extend(args.iter().map(String::as_str));
    let o = Command::new(env!("CARGO_BIN_EXE_gcg-eval"))
        .args(&cmd)
        .current_dir(root())
        .env_remove("COLUMNS")
        .env(
            "GCG_SIM_AGENT_FACTORY",
            "tests/runner/random_agent.py:make_random_agent",
        )
        .output()
        .expect("gcg-eval runs");
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
    assert_eq!(
        without(tree(&out), &["timing.json"]),
        without(tree(base), &["timing.json"])
    );
    assert_eq!(text(&o.stdout), base_out.replace(s(base), s(&out)));
    assert_eq!(&text(&o.stderr), base_err);
    let results: Value =
        serde_json::from_slice(&fs::read(out.join("results.json")).expect("results"))
            .expect("JSON");
    assert_eq!(results["ai"]["factory"], "gcg-search::make_agent('fast')");
    // W-104: the deterministic decision counts in timing.json.
    let timing: Value =
        serde_json::from_slice(&fs::read(out.join("timing.json")).expect("timing")).expect("JSON");
    let games = fs::read_to_string(out.join("games.ndjson")).expect("games");
    let counts: Vec<u64> = games
        .lines()
        .map(|l| {
            let v: Value = serde_json::from_str(l).expect("record");
            u64::try_from(v["actions"].as_array().expect("actions").len()).expect("count")
        })
        .collect();
    assert_eq!(timing["decisions_per_game"], serde_json::json!(counts));
    assert_eq!(timing["decisions"], counts.iter().sum::<u64>());
    // W-100: the printed reproduce command re-runs to the same results.json.
    let summary = fs::read_to_string(out.join("summary.md")).expect("summary");
    let line = summary
        .lines()
        .find_map(|l| l.strip_prefix("Reproduce: `"))
        .and_then(|l| l.strip_suffix('`'))
        .expect("a reproduce line");
    let rest = line
        .strip_prefix("gcg-sim benchmark ")
        .expect("a gcg-sim benchmark command");
    let again = dir.join("again");
    let mut cmd: Vec<&str> = vec!["benchmark"];
    cmd.extend(rest.split(' '));
    cmd.extend(["--out", s(&again)]);
    ok(&cmd);
    assert_eq!(
        fs::read(again.join("results.json")).expect("results"),
        fs::read(out.join("results.json")).expect("results")
    );
    let _ = fs::remove_dir_all(&dir);
}

/// The bytes of a `--compress` file: `gzip -dc`, or the zstd decoder of a `zstd` build.
fn unpack(path: &Path) -> Vec<u8> {
    #[cfg(feature = "zstd")]
    {
        zstd::decode_all(&fs::read(path).expect("compressed file")[..]).expect("zstd decodes")
    }
    #[cfg(not(feature = "zstd"))]
    {
        let o = Command::new("gzip")
            .arg("-dc")
            .arg(path)
            .output()
            .expect("gzip runs");
        assert!(o.status.success(), "{}", text(&o.stderr));
        o.stdout
    }
}

#[test]
fn opt_ins_add_files_and_keep_the_reports() {
    let (base, _, _) = baseline();
    let dir = scratch("opt-ins");
    let out = dir.join("reports");
    let args = bench_args(&out);
    let mut cmd: Vec<&str> = vec!["benchmark"];
    cmd.extend(args.iter().map(String::as_str));
    cmd.extend([
        "--analysis",
        "--swiss-draws",
        "--compress",
        "--replays-extremes",
        "--diagnostics",
    ]);
    let o = ok(&cmd);
    let (file, _) = compression();
    let stdout = text(&o.stdout);
    assert!(
        stdout.contains(&format!("  games    {}", s(&out.join(file)))),
        "{stdout}"
    );
    assert!(stdout.contains("  analysis "), "{stdout}");
    let ours = tree(&out);
    let reference = tree(base);
    for (name, bytes) in &reference {
        if name == "timing.json" || name == "games.ndjson" {
            continue;
        }
        assert_eq!(ours.get(name), Some(bytes), "{name} is unchanged");
    }
    assert!(!ours.contains_key("games.ndjson"));
    assert_eq!(unpack(&out.join(file)), reference["games.ndjson"]);
    let analysis: Value =
        serde_json::from_slice(&ours["analysis.json"]).expect("analysis.json is JSON");
    assert_eq!(analysis["format"], "gcg-eval-analysis");
    assert!(analysis["double_defeats"]["swiss_draws"].is_object());
    assert!(analysis["mulligan"]["label"].is_string());
    let diagnostics: Value = serde_json::from_slice(&ours["diagnostics.json"]).expect("JSON");
    assert_eq!(diagnostics["format"], "gcg-eval-diagnostics");
    let extremes = ours
        .keys()
        .filter(|k| k.starts_with("replays/shortest-") || k.starts_with("replays/longest-"))
        .count();
    assert!(extremes >= 2, "{:?}", ours.keys().collect::<Vec<_>>());
    // --swiss-draws needs --analysis (a usage error).
    let mut bad: Vec<&str> = vec!["benchmark"];
    bad.extend(args.iter().map(String::as_str));
    bad.push("--swiss-draws");
    let o = gcg_eval(&bad);
    assert_eq!(o.status.code(), Some(2));
    assert!(text(&o.stderr).contains("--swiss-draws: needs --analysis"));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn w105_an_interrupted_resume_run_finishes_with_the_uninterrupted_reports() {
    let (base, _, _) = baseline();
    let dir = scratch("resume");
    let out = dir.join("reports");
    let args = bench_args(&out);
    let mut cmd: Vec<&str> = vec!["benchmark"];
    cmd.extend(args.iter().map(String::as_str));
    cmd.push("--resume");
    let mut child = Command::new(env!("CARGO_BIN_EXE_gcg-eval"))
        .args(&cmd)
        .current_dir(root())
        .env_remove("COLUMNS")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("gcg-eval starts");
    let stderr = child.stderr.take().expect("stderr");
    let mut lines = BufReader::new(stderr).lines();
    let first = lines.next().expect("a progress line").expect("text");
    assert_eq!(first, "matches 1/2");
    let pid = child.id().to_string();
    assert!(
        Command::new("kill")
            .args(["-INT", &pid])
            .status()
            .expect("kill runs")
            .success()
    );
    let rest: Vec<String> = lines.map_while(Result::ok).collect();
    let status = child.wait().expect("exits");
    assert_eq!(status.code(), Some(130), "{rest:?}");
    let ckpt = out.join("checkpoint.ndjson");
    let saved = fs::read_to_string(&ckpt).expect("the checkpoint stays");
    assert!(
        saved.lines().count() >= 2,
        "a header and the finished match"
    );
    assert!(
        !out.join("results.json").exists(),
        "no reports after an interrupt"
    );
    // Another configuration is refused.
    let mut other: Vec<String> = args.clone();
    other[5] = "6".to_owned();
    let mut cmd2: Vec<&str> = vec!["benchmark"];
    cmd2.extend(other.iter().map(String::as_str));
    cmd2.push("--resume");
    let refused = gcg_eval(&cmd2);
    assert_eq!(refused.status.code(), Some(1));
    assert!(
        text(&refused.stderr).contains("refusing to resume"),
        "{}",
        text(&refused.stderr)
    );
    ok(&cmd);
    assert!(!ckpt.exists());
    assert_eq!(
        without(tree(&out), &["timing.json"]),
        without(tree(base), &["timing.json"])
    );
    let _ = fs::remove_dir_all(&dir);
}

/// A cheap run of the runner's random test agent through `reference-benchmark`.
fn random_run(dir: &Path) -> PathBuf {
    let out = dir.join("random");
    ok(&[
        "reference-benchmark",
        &deck("red-green-zeon.txt"),
        &deck("blue-white-federation.txt"),
        "--matches",
        "4",
        "--seed",
        "8",
        "--agent",
        "test-random",
        "--out",
        s(&out),
    ]);
    out
}

#[test]
fn w148_replay_game_verifies_every_game_of_a_run() {
    let dir = scratch("replay-game");
    let run = random_run(&dir);
    let games = fs::read_to_string(run.join("games.ndjson")).expect("games");
    let results = run.join("results.json");
    for line in games.lines() {
        let v: Value = serde_json::from_str(line).expect("record");
        let (m, g) = (
            v["match_index"].as_u64().expect("m").to_string(),
            (v["game_index"].as_u64().expect("g") + 1).to_string(),
        );
        let o = ok(&[
            "replay-game",
            "--games-ndjson",
            s(&run.join("games.ndjson")),
            "--results",
            s(&results),
            "--match",
            &m,
            "--game",
            &g,
        ]);
        let stdout = text(&o.stdout);
        assert!(stdout.starts_with("T0 P"), "{stdout}");
        assert!(
            stdout.ends_with("replay matches the recorded outcome and final state\n"),
            "{stdout}"
        );
    }
    // A record whose final state was changed fails, with the version diagnosis.
    let tampered = dir.join("tampered.ndjson");
    let first: Value = serde_json::from_str(games.lines().next().expect("a game")).expect("JSON");
    let mut bad = first.clone();
    bad["final_state_sha256"] = Value::String("0".repeat(64));
    fs::write(
        &tampered,
        format!("{}\n", serde_json::to_string(&bad).expect("JSON")),
    )
    .expect("write");
    let o = gcg_eval(&[
        "replay-game",
        "--games-ndjson",
        s(&tampered),
        "--results",
        s(&results),
        "--match",
        "0",
        "--game",
        "1",
    ]);
    assert_eq!(o.status.code(), Some(1));
    let err = text(&o.stderr);
    assert!(
        err.contains("replay does NOT match the recorded outcome"),
        "{err}"
    );
    assert!(err.contains("regression or nondeterminism"), "{err}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn w156_replay_check_names_version_differences() {
    let dir = scratch("replay-check");
    let run = random_run(&dir);
    let replays: Vec<PathBuf> = fs::read_dir(run.join("replays"))
        .expect("replays")
        .map(|e| e.expect("entry").path())
        .collect();
    assert!(!replays.is_empty());
    for r in &replays {
        let o = ok(&["replay-check", s(r)]);
        let mut python = Vec::new();
        replay_command(&PyStr::from(s(r)), false, &mut python).expect("gcg-sim replay passes");
        assert_eq!(
            o.stdout, python,
            "matching versions print gcg-sim replay's output"
        );
    }
    // Out of its run directory: the versions are unknown.
    let loose = dir.join("loose.json");
    fs::copy(&replays[0], &loose).expect("copy");
    let o = ok(&["replay-check", s(&loose)]);
    assert!(text(&o.stdout).contains("note: the versions are unknown"));
    // A run recorded with another dataset_version, and a replay that no longer matches.
    let copy = dir.join("other-data");
    fs::create_dir_all(copy.join("replays")).expect("mkdir");
    let mut results: Value =
        serde_json::from_slice(&fs::read(run.join("results.json")).expect("results"))
            .expect("JSON");
    results["versions"]["dataset_version"] = Value::String("synthetic-older".to_owned());
    fs::write(
        copy.join("results.json"),
        serde_json::to_string(&results).expect("JSON"),
    )
    .expect("write");
    let mut doc: Value =
        serde_json::from_slice(&fs::read(&replays[0]).expect("replay")).expect("JSON");
    doc["expected"]["final_state_sha256"] = Value::String("f".repeat(64));
    let changed = copy.join("replays").join("loss-m0000-g1.json");
    fs::write(&changed, serde_json::to_string(&doc).expect("JSON")).expect("write");
    let o = gcg_eval(&["replay-check", s(&changed)]);
    assert_eq!(o.status.code(), Some(1));
    let err = text(&o.stderr);
    assert!(
        err.contains("replay does NOT match the recorded outcome"),
        "{err}"
    );
    assert!(err.contains("an expected change is possible"), "{err}");
    assert!(
        err.contains("dataset_version: recorded \"synthetic-older\", current "),
        "{err}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn w91_compare_runs_refuses_runs_of_different_ais() {
    let dir = scratch("mixed");
    let a = random_run(&dir);
    let b = dir.join("b");
    fs::create_dir_all(&b).expect("mkdir");
    fs::copy(a.join("games.ndjson"), b.join("games.ndjson")).expect("copy");
    let mut results: Value =
        serde_json::from_slice(&fs::read(a.join("results.json")).expect("results")).expect("JSON");
    results["ai"]["factory"] = Value::String("gcg-search::make_agent('standard')".to_owned());
    fs::write(
        b.join("results.json"),
        serde_json::to_string(&results).expect("JSON"),
    )
    .expect("write");
    let o = gcg_eval(&["compare-runs", s(&a), s(&b)]);
    assert_eq!(o.status.code(), Some(1));
    let err = text(&o.stderr);
    assert!(
        err.contains("not comparable (W-91)") && err.contains("ai.factory"),
        "{err}"
    );
    let o = ok(&["compare-runs", s(&a), s(&b), "--allow-mixed"]);
    assert!(text(&o.stderr).contains("--allow-mixed given"));
    ok(&["compare-runs", s(&a), s(&a)]);
    let _ = fs::remove_dir_all(&dir);
}

#[cfg(feature = "zstd")]
#[test]
fn w99_zstd_output_decompresses_to_the_same_bytes() {
    let (file, encode) = compression();
    assert_eq!(file, "games.ndjson.zst");
    let data = "{\"turn\":12}\n".repeat(5000).into_bytes();
    assert_eq!(zstd::decode_all(&encode(&data)[..]).expect("decodes"), data);
}
