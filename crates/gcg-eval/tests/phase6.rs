//! The Phase 6 measurement tools of `gcg-eval` end to end (docs/port/PLAN.md §12.2 P6-I,
//! §12.4-§12.8), through the binary with cheap agents (random, greedy and tiny null
//! configurations of the strengthened search): every tool writes the same report for 1 and 4
//! workers; `ablation` and `budget-curve` rows carry the §12.4 statistics and the agents'
//! effort; seed roles refuse held-out pairs and are recorded in the ledger; `gates` computes
//! E1-E4 from head-to-head reports and fails a gate below its bar (W-19); `decks --overlap`
//! reproduces the W-20 overlaps; `compare-runs` pairs same-seed runs (W-42); `tune` (W-40),
//! `regret` (W-43), `fidelity` (W-18), `calibrate` (W-41) and `evidence` (W-127); and the
//! W-21 decision traces leave every report byte-identical.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use gcg_eval::overlap::overlap;
use serde_json::{Value, json};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("phase6-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn gcg_eval(args: &[&str], cwd: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_gcg-eval"))
        .args(args)
        .current_dir(cwd)
        .env_remove("COLUMNS")
        .output()
        .expect("gcg-eval runs")
}

fn ok(args: &[&str], cwd: &Path) -> Output {
    let o = gcg_eval(args, cwd);
    assert_eq!(
        o.status.code(),
        Some(0),
        "{args:?}: {}",
        String::from_utf8_lossy(&o.stderr)
    );
    o
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn read_json(path: &Path) -> Value {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn s(p: &Path) -> &str {
    p.to_str().expect("UTF-8 path")
}

/// Runs `args --workers W --out FILE` for W = 1 and 4 and returns the report, after checking
/// that both files are byte-identical.
fn same_for_workers(args: &[&str], dir: &Path, name: &str) -> Value {
    let mut texts = Vec::new();
    for w in ["1", "4"] {
        let out = dir.join(format!("{name}-{w}.json"));
        let mut a: Vec<&str> = args.to_vec();
        a.extend(["--workers", w, "--out", s(&out)]);
        ok(&a, &root());
        texts.push(std::fs::read(&out).expect("report"));
    }
    assert_eq!(texts[0], texts[1], "{name}: 1 vs 4 workers");
    read_json(&dir.join(format!("{name}-1.json")))
}

const TINY: &str = "search:null-fast:max_iterations=8,min_iterations=4,iterations_per_option=2";

#[test]
fn ablation_rows_are_deterministic_and_carry_the_row_statistics() {
    let dir = scratch("ablation");
    let ledger = dir.join("ledger.json");
    let doc = same_for_workers(
        &[
            "ablation",
            "leave-one-out",
            "--bundle",
            TINY,
            "--mechanism",
            "observation_keys=0",
            "--decks",
            "tuning",
            "--pairs",
            "all",
            "--games",
            "12",
            "--seed",
            "20000001",
            "--role",
            "dev",
            "--ledger",
            s(&ledger),
        ],
        &dir,
        "loo",
    );
    assert_eq!(doc["format"], "gcg-eval-ablation");
    assert_eq!(doc["row"], "leave-one-out");
    assert_eq!(doc["opponent"], format!("{TINY},observation_keys=0"));
    let summary = &doc["summary"];
    for key in [
        "wins",
        "n",
        "rate",
        "ci95",
        "difference",
        "difference_ci95",
        "discordant",
        "iterations_per_game",
        "applies_per_game",
        "text",
    ] {
        assert!(summary.get(key).is_some(), "summary.{key}");
    }
    assert_eq!(summary["n"], 12);
    for side in ["challenger", "opponent"] {
        assert!(
            summary["iterations_per_game"][side]
                .as_f64()
                .is_some_and(|x| x > 0.0),
            "{side} reports its iterations"
        );
        assert!(
            summary["applies_per_game"][side]
                .as_f64()
                .is_some_and(|x| x > 0.0)
        );
    }
    let paired = &doc["result"]["paired"];
    assert_eq!(paired["deals"], 6);
    assert_eq!(
        summary["discordant"],
        json!([paired["challenger_only"], paired["opponent_only"]])
    );
    assert_eq!(doc["outcomes"].as_array().map(Vec::len), Some(12));
    assert_eq!(
        doc["outcomes"][0].as_array().map(Vec::len),
        Some(11),
        "effort per game"
    );
    let entries = read_json(&ledger)["entries"].clone();
    assert_eq!(entries[0]["command"], "ablation");
    assert_eq!(entries[0]["role"], "dev");
    assert_eq!(
        entries.as_array().map(Vec::len),
        Some(1),
        "the rerun is recorded once"
    );

    let ind = same_for_workers(
        &[
            "ablation",
            "individual",
            "--reference",
            "fast",
            "--mechanism",
            "max_iterations=8,min_iterations=4,iterations_per_option=2",
            "--decks",
            "tuning",
            "--pairs",
            "all",
            "--games",
            "12",
        ],
        &dir,
        "ind",
    );
    assert_eq!(
        (ind["challenger"].as_str(), ind["opponent"].as_str()),
        (
            Some("search:null-fast:max_iterations=8,min_iterations=4,iterations_per_option=2"),
            Some("ref:fast")
        )
    );
    assert_eq!(ind["agents"]["opponent"], "mcts-fast");
}

/// PLAN §12.4: 400 games stratify over 15 pairs (10 pairs of 28 games, 5 of 24), on a split
/// with flagged pairs; the steering roles still refuse a held-out pair.
#[test]
fn rows_stratify_over_the_pairs_and_refuse_held_out_pairs() {
    let dir = scratch("stratified");
    std::fs::create_dir_all(dir.join("docs/port/eval")).expect("dirs");
    std::fs::write(
        dir.join("docs/port/eval/panel.json"),
        json!({"pairs": [
            {"decks": ["tuning-zeon", "tuning-wing"], "held_out": false},
            {"decks": ["tuning-zeon", "tuning-tekkadan"], "held_out": false},
            {"decks": ["tuning-wing", "tuning-tekkadan"], "held_out": true}
        ]})
        .to_string(),
    )
    .expect("panel");
    let refused = gcg_eval(
        &[
            "ablation",
            "leave-one-out",
            "--bundle",
            "search:standard",
            "--mechanism",
            "observation_keys=0",
            "--decks",
            "tuning",
            "--pairs",
            "all",
            "--games",
            "12",
            "--role",
            "tuning",
            "--seed",
            "10000000",
            "--ledger",
            "l.json",
        ],
        &dir,
    );
    assert_eq!(refused.status.code(), Some(1));
    assert!(
        stderr(&refused).contains("may not play the held-out pair tuning-wing:tuning-tekkadan"),
        "{}",
        stderr(&refused)
    );
    let out = dir.join("curve.json");
    ok(
        &[
            "budget-curve",
            TINY,
            "random",
            "--scales",
            "0.5,1",
            "--decks",
            "tuning",
            "--pairs",
            "tuning",
            "--games",
            "12",
            "--role",
            "tuning",
            "--seed",
            "10000000",
            "--ledger",
            "l.json",
            "--out",
            s(&out),
        ],
        &dir,
    );
    let doc = read_json(&out);
    let points = doc["points"].as_array().expect("points");
    assert_eq!(points.len(), 2);
    assert_eq!(points[0]["budget"]["max_iterations"], 4);
    assert_eq!(points[1]["budget"]["max_iterations"], 8);
    let pairs: Vec<u64> = points[0]["by_pair"]
        .as_array()
        .expect("by_pair")
        .iter()
        .map(|p| p["result"]["n"].as_u64().unwrap_or(0))
        .collect();
    assert_eq!(
        pairs,
        [8, 4],
        "3 blocks dealt round-robin over 2 tuning pairs"
    );
    assert!(points[0]["result"]["stratified"]["ci95"].is_array());
    let masters = |p: &Value| -> Vec<Value> {
        p["by_pair"]
            .as_array()
            .expect("by_pair")
            .iter()
            .map(|r| r["master"].clone())
            .collect()
    };
    assert_eq!(
        masters(&points[0]),
        masters(&points[1]),
        "every point plays the same deals"
    );
}

fn h2h_report(
    challenger: &str,
    opponent: &str,
    decks: &str,
    pairs: &[(&str, &str, bool, u64, u64)],
) -> Value {
    let mut by_pair = Vec::new();
    let mut outcomes = Vec::new();
    let mut index = 0_u64;
    for (k, (a, b, held, wins, n)) in pairs.iter().enumerate() {
        by_pair.push(json!({"decks": [a, b], "position": k, "held_out": held, "master": 0}));
        for g in 0..*n {
            let chal = g % 2;
            let winner = if g < *wins { chal } else { 1 - chal };
            outcomes.push(json!([index, k, chal, winner, 10, 0, 100]));
            index += 1;
        }
    }
    json!({
        "format": "gcg-eval-head-to-head", "version": 2, "challenger": challenger,
        "opponent": opponent, "decks": decks, "pairs": "all", "seed": 90000000,
        "role": "eval", "games": index, "choose_first": "seat0",
        "by_pair": by_pair, "outcomes": outcomes
    })
}

fn panel_pairs(wins: impl Fn(usize, bool) -> u64) -> Vec<(String, String, bool, u64, u64)> {
    let decks = ["a", "b", "c", "d", "e", "f", "h1", "h2"];
    let mut out = Vec::new();
    for i in 0..8 {
        for j in i + 1..8 {
            let held = decks[i].starts_with('h') || decks[j].starts_with('h');
            let k = out.len();
            out.push((
                decks[i].to_owned(),
                decks[j].to_owned(),
                held,
                wins(k, held),
                400,
            ));
        }
    }
    out
}

fn write_report(dir: &Path, name: &str, v: &Value) -> PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, v.to_string()).expect("write report");
    p
}

/// W-19, PLAN §12.6: the gates pass on reports above their bars and fail (exit 1 with
/// `--check`) when one bar is missed; a report with the wrong agents or role never counts.
#[test]
fn w19_gates_compute_e1_to_e4_and_fail_below_a_bar() {
    let dir = scratch("gates");
    let as_ref = |v: &[(String, String, bool, u64, u64)]| -> Vec<(String, String, bool, u64, u64)> {
        v.to_vec()
    };
    let build = |c: &str, o: &str, decks: &str, p: Vec<(String, String, bool, u64, u64)>| {
        let refs: Vec<(&str, &str, bool, u64, u64)> = p
            .iter()
            .map(|(a, b, h, w, n)| (a.as_str(), b.as_str(), *h, *w, *n))
            .collect();
        h2h_report(c, o, decks, &refs)
    };
    let good =
        |held_wins: u64, other: u64| as_ref(&panel_pairs(|_, h| if h { held_wins } else { other }));
    let e1 = write_report(
        &dir,
        "e1.json",
        &build("search:standard", "ref:strong", "panel", good(250, 250)),
    );
    let e2 = write_report(
        &dir,
        "e2.json",
        &build("search:fast", "ref:standard", "panel", good(210, 210)),
    );
    let e3t = write_report(
        &dir,
        "e3t.json",
        &build(
            "search:standard",
            "random",
            "tests",
            vec![(
                "tests-federation".into(),
                "tests-seed".into(),
                false,
                400,
                400,
            )],
        ),
    );
    let e3p = write_report(
        &dir,
        "e3p.json",
        &build("search:standard", "random", "panel", good(400, 400)),
    );
    let e4t = write_report(
        &dir,
        "e4t.json",
        &build(
            "search:standard",
            "greedy",
            "tests",
            vec![(
                "tests-federation".into(),
                "tests-seed".into(),
                false,
                340,
                400,
            )],
        ),
    );
    let e4p = write_report(
        &dir,
        "e4p.json",
        &build("search:standard", "greedy", "panel", good(340, 340)),
    );
    let all = |e1: &Path, e3p: &Path| -> Vec<String> {
        vec![
            "gates".into(),
            "--check".into(),
            "--e1".into(),
            s(e1).into(),
            "--e2".into(),
            s(&e2).into(),
            "--e3-tests".into(),
            s(&e3t).into(),
            "--e3-panel".into(),
            s(e3p).into(),
            "--e4-tests".into(),
            s(&e4t).into(),
            "--e4-panel".into(),
            s(&e4p).into(),
        ]
    };
    let args = all(&e1, &e3p);
    let argv: Vec<&str> = args.iter().map(String::as_str).collect();
    let passed = gcg_eval(&argv, &dir);
    assert_eq!(passed.status.code(), Some(0), "{}", stderr(&passed));
    let doc: Value = serde_json::from_slice(&passed.stdout).expect("JSON");
    assert_eq!(doc["pass"], true);

    // E1 below its bar on the held-out pairs only: pooled-all passes, held-out fails.
    let weak = write_report(
        &dir,
        "e1-weak.json",
        &build("search:standard", "ref:strong", "panel", good(205, 250)),
    );
    let args = all(&weak, &e3p);
    let argv: Vec<&str> = args.iter().map(String::as_str).collect();
    let failed = gcg_eval(&argv, &dir);
    assert_eq!(failed.status.code(), Some(1));
    assert!(
        stderr(&failed).contains("gates not passed: E1"),
        "{}",
        stderr(&failed)
    );
    let doc: Value = serde_json::from_slice(&failed.stdout).expect("JSON");
    let checks = doc["gates"][0]["reports"][0]["checks"]
        .as_array()
        .expect("checks")
        .clone();
    let by_name = |n: &str| {
        checks
            .iter()
            .find(|c| c["check"] == n)
            .cloned()
            .unwrap_or(Value::Null)
    };
    assert_eq!(by_name("pooled lower bound, all pairs")["pass"], true);
    assert_eq!(by_name("pooled lower bound, held-out pairs")["pass"], false);

    // One loss to random fails E3 until it is documented with an existing replay.
    let mut lossy = good(400, 400);
    lossy[3].3 = 399;
    let e3l = write_report(
        &dir,
        "e3l.json",
        &build("search:standard", "random", "panel", lossy),
    );
    let args = all(&e1, &e3l);
    let argv: Vec<&str> = args.iter().map(String::as_str).collect();
    let failed = gcg_eval(&argv, &dir);
    assert!(stderr(&failed).contains("E3"), "{}", stderr(&failed));
    let loss_index = 3 * 400 + 399;
    let replay = write_report(&dir, "loss-replay.json", &json!({}));
    let losses = write_report(
        &dir,
        "losses.json",
        &json!({"losses": [{"report": s(&e3l), "index": loss_index, "replay": s(&replay)}]}),
    );
    let mut args = all(&e1, &e3l);
    args.extend(["--losses-documented".to_owned(), s(&losses).to_owned()]);
    let argv: Vec<&str> = args.iter().map(String::as_str).collect();
    let documented = gcg_eval(&argv, &dir);
    assert_eq!(documented.status.code(), Some(0), "{}", stderr(&documented));

    // A report of the wrong agents or of a steering role never counts.
    let mut wrong = build("search:standard", "ref:standard", "panel", good(300, 300));
    wrong["role"] = json!("dev");
    let wrong = write_report(&dir, "wrong.json", &wrong);
    let args = all(&wrong, &e3p);
    let argv: Vec<&str> = args.iter().map(String::as_str).collect();
    let failed = gcg_eval(&argv, &dir);
    assert_eq!(failed.status.code(), Some(1));
    let doc: Value = serde_json::from_slice(&failed.stdout).expect("JSON");
    let checks = doc["gates"][0]["reports"][0]["checks"]
        .as_array()
        .expect("checks")
        .clone();
    assert!(
        checks
            .iter()
            .any(|c| c["check"] == "agents" && c["pass"] == false)
    );
    assert!(
        checks
            .iter()
            .any(|c| c["check"] == "seed role" && c["pass"] == false)
    );

    // Without --check a missing gate is reported, not fatal.
    let partial = gcg_eval(&["gates", "--e1", s(&e1)], &dir);
    assert_eq!(partial.status.code(), Some(0));
    let doc: Value = serde_json::from_slice(&partial.stdout).expect("JSON");
    assert_eq!(doc["gates"][0]["pass"], true);
    assert_eq!(doc["gates"][1]["pass"], Value::Null);
    assert_eq!(doc["pass"], false);
}

/// A working directory with the example decks, three evaluation decks (copies of the tuning
/// decks under new names) and a split holding out `eval-wing`.
fn panel_workdir(name: &str) -> PathBuf {
    let dir = scratch(name);
    let eval = dir.join("examples/decks/eval");
    std::fs::create_dir_all(&eval).expect("dirs");
    std::fs::create_dir_all(dir.join("docs/port/eval")).expect("dirs");
    for f in [
        "blue-white-federation",
        "purple-white-tekkadan",
        "red-green-zeon",
    ] {
        std::fs::copy(
            root().join(format!("examples/decks/{f}.txt")),
            dir.join(format!("examples/decks/{f}.txt")),
        )
        .expect("copy example");
    }
    for (from, to) in [
        ("tuning-zeon", "eval-zeon"),
        ("tuning-wing", "eval-wing"),
        ("tuning-tekkadan", "eval-tekkadan"),
    ] {
        std::fs::copy(
            root().join(format!("crates/gcg-eval/decks/{from}.txt")),
            eval.join(format!("{to}.txt")),
        )
        .expect("copy eval deck");
    }
    std::fs::write(
        dir.join("docs/port/eval/panel.json"),
        json!({"held_out_decks": ["eval-wing"], "pairs": []}).to_string(),
    )
    .expect("panel");
    dir
}

/// W-20: a panel head-to-head reports every pair, every deck by name, the held-out pairs
/// pooled on their own and the pair-stratified interval; `decks --overlap --check` enforces
/// the bound on the held-out decks of the split.
#[test]
fn w20_panel_head_to_head_and_the_overlap_bound() {
    let dir = panel_workdir("panel");
    let out = dir.join("panel.json.out");
    ok(
        &[
            "head-to-head",
            "random",
            "greedy",
            "--decks",
            "panel",
            "--games",
            "60",
            "--out",
            s(&out),
        ],
        &dir,
    );
    let doc = read_json(&out);
    assert_eq!(doc["by_pair"].as_array().map(Vec::len), Some(15));
    assert_eq!(doc["held_out"]["pairs"], 5, "every pair with eval-wing");
    assert_eq!(doc["held_out"]["n"], 20);
    let decks: Vec<&str> = doc["by_deck"]
        .as_array()
        .expect("by_deck")
        .iter()
        .filter_map(|d| d["deck"].as_str())
        .collect();
    assert_eq!(decks.len(), 6);
    assert!(decks.contains(&"eval-wing") && decks.contains(&"red-green-zeon"));
    let total: u64 = doc["by_deck"]
        .as_array()
        .expect("by_deck")
        .iter()
        .map(|d| d["result"]["n"].as_u64().unwrap_or(0))
        .sum();
    assert_eq!(total, 60);
    assert!(doc["result"]["stratified"]["ci95"].is_array());
    assert!(doc["held_out"]["stratified"]["ci95"].is_array());
    assert!(doc["result"]["paired"]["bootstrap_ci95"].is_array());
    let over = gcg_eval(&["decks", "--overlap", "--check"], &dir);
    assert_eq!(
        over.status.code(),
        Some(1),
        "eval-wing is a copy of tuning-wing"
    );
    let report: Value = serde_json::from_slice(&over.stdout).expect("JSON");
    assert_eq!(
        report["check"]["over_bound"][0],
        json!(["eval-wing", "tuning-wing", 50])
    );
    let loose = gcg_eval(&["decks", "--overlap", "--check", "--bound", "50"], &dir);
    assert_eq!(loose.status.code(), Some(0), "{}", stderr(&loose));
}

/// W-123: the binary runs the default deck set and the named example decks from a directory
/// without the repository's files, and labels decks by name.
#[test]
fn w123_named_deck_sets_work_from_any_directory() {
    let dir = scratch("anywhere");
    let out = dir.join("h2h.json");
    ok(
        &[
            "head-to-head",
            "random",
            "random",
            "--games",
            "4",
            "--out",
            s(&out),
        ],
        &dir,
    );
    assert_eq!(
        read_json(&out)["by_pair"][0]["decks"],
        json!(["tests-federation", "tests-seed"])
    );
    let ex = dir.join("examples.json");
    ok(
        &[
            "head-to-head",
            "random",
            "random",
            "--decks",
            "examples",
            "--games",
            "12",
            "--out",
            s(&ex),
        ],
        &dir,
    );
    let doc = read_json(&ex);
    assert_eq!(
        doc["by_pair"][2]["decks"],
        json!(["purple-white-tekkadan", "red-green-zeon"])
    );
    let pair = dir.join("pair.json");
    ok(
        &[
            "head-to-head",
            "random",
            "random",
            "--pair",
            "red-green-zeon:tuning-wing",
            "--games",
            "4",
            "--out",
            s(&pair),
        ],
        &dir,
    );
    assert_eq!(read_json(&pair)["by_deck"][0]["deck"], "red-green-zeon");
}

/// The Phase 6 tools ask the agents through `choose_traced`; every agent kind (random,
/// greedy, the reference and the strengthened search) plays exactly the games it plays
/// through `choose` (`play_all`, the path of `compare` and of Python), so the tools measure
/// the agents the benchmark ships ("no AI effect", PLAN A13).
#[test]
fn traced_games_equal_plain_games_for_every_agent_kind() {
    use gcg_core::pool::Cancel;
    use gcg_eval::selfplay::{play_all, play_all_traced};
    use gcg_eval::specs::comparison_specs;
    use gcg_eval::{ChooseFirst, DeckSet};
    let decks: Vec<_> = gcg_eval::decks::embedded(DeckSet::Tuning)
        .expect("tuning")
        .into_iter()
        .map(|d| d.deck)
        .collect();
    let spec = |t: &str| gcg_eval::agent_spec::parse_agent_spec(t).expect("spec");
    for (c, o) in [
        ("ref:fast", "greedy"),
        (TINY, "random"),
        (
            "search:null-fast:max_iterations=12",
            "ref:fast:max_iterations=12",
        ),
    ] {
        let specs = comparison_specs(12, 77, &spec(c), &spec(o), &decks).expect("specs");
        for cf in [ChooseFirst::Seat0, ChooseFirst::Agents] {
            let w = gcg_runner::Workers::Count(4);
            let plain = play_all(&specs, w, cf, &Cancel::new(), &mut |_| {}).expect("plain");
            let traced: Vec<_> = play_all_traced(&specs, w, cf, &Cancel::new(), &mut |_| {})
                .expect("traced")
                .into_iter()
                .map(|r| r.outcome)
                .collect();
            assert_eq!(plain, traced, "{c} vs {o}, {cf:?}");
        }
    }
}

/// W-127: the port's evaluation documents cite only commits that resolve and evidence paths
/// that exist.
#[test]
fn w127_the_port_evaluation_docs_cite_only_reachable_commits() {
    let o = gcg_eval(&["evidence", "check"], &root());
    assert_eq!(
        o.status.code(),
        Some(0),
        "{}{}",
        stderr(&o),
        String::from_utf8_lossy(&o.stdout)
    );
}

/// W-20: the overlaps the WEAKNESSES entry recomputed, from the files.
#[test]
fn w20_overlaps_reproduce_the_recorded_values() {
    let dir = scratch("overlap");
    let out = dir.join("overlap.json");
    ok(
        &[
            "decks",
            "--overlap",
            "--rows",
            "examples",
            "--cols",
            "tuning,tests",
            "--out",
            s(&out),
        ],
        &root(),
    );
    let doc = read_json(&out);
    let rows: Vec<&str> = doc["rows"]
        .as_array()
        .expect("rows")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    let cols: Vec<&str> = doc["cols"]
        .as_array()
        .expect("cols")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert_eq!(
        rows,
        [
            "blue-white-federation",
            "purple-white-tekkadan",
            "red-green-zeon"
        ]
    );
    assert_eq!(
        cols,
        [
            "tuning-zeon",
            "tuning-wing",
            "tuning-tekkadan",
            "tests-federation",
            "tests-seed"
        ]
    );
    let m = |r: &str, c: &str| {
        let i = rows.iter().position(|x| *x == r).expect("row");
        let j = cols.iter().position(|x| *x == c).expect("col");
        doc["matrix"][i][j].as_u64().expect("count")
    };
    assert_eq!(m("red-green-zeon", "tuning-zeon"), 27);
    assert_eq!(m("purple-white-tekkadan", "tuning-tekkadan"), 29);
    assert_eq!(m("blue-white-federation", "tuning-wing"), 2);
    assert_eq!(m("blue-white-federation", "tests-federation"), 27);
    assert_eq!(m("purple-white-tekkadan", "tests-federation"), 3);
    assert_eq!(m("blue-white-federation", "tests-seed"), 3);
    assert_eq!(m("red-green-zeon", "tests-seed"), 2);
    for d in doc["decks"].as_array().expect("decks") {
        assert_eq!(
            d["within_bound"], false,
            "no example deck may be held out: {d}"
        );
    }
    let decks = gcg_eval::decks::embedded(gcg_eval::DeckSet::Tests).expect("tests");
    assert_eq!(overlap(&decks[0].deck, &decks[0].deck), 50);
    // The bound is enforced against a split: every example deck breaks it.
    let examples = gcg_eval::decks::dir_decks(&root().join("examples/decks")).expect("examples");
    let tuning = gcg_eval::decks::embedded(gcg_eval::DeckSet::Tuning).expect("tuning");
    let split = gcg_eval::decks::Panel {
        held_out: vec!["red-green-zeon".to_owned(), "nope".to_owned()],
        pairs: Vec::new(),
    };
    let (over, unknown) = gcg_eval::overlap::violations(&split, &examples, &tuning, &decks, 10);
    assert_eq!(
        over,
        [("red-green-zeon".to_owned(), "tuning-zeon".to_owned(), 27)]
    );
    assert_eq!(unknown, ["nope"]);
    let (none, _) = gcg_eval::overlap::violations(&split, &examples, &tuning, &decks, 50);
    assert!(none.is_empty());
}

fn write_games(dir: &Path, games: &[(u64, u64, u64, &str)]) {
    std::fs::create_dir_all(dir).expect("dir");
    let text: String = games
        .iter()
        .map(|(m, g, seed, w)| {
            json!({"match_index": m, "game_index": g, "seed": seed, "winner": w, "match_point": w})
                .to_string()
                + "\n"
        })
        .collect();
    std::fs::write(dir.join("games.ndjson"), text).expect("games");
}

/// W-42: two identical same-seed runs differ by zero with no discordant pair; a variant
/// that only turns losses into wins on the same deals gives a narrower paired interval than
/// the unpaired one; runs with different seeds are refused.
#[test]
fn w42_compare_runs_pairs_same_seed_runs() {
    let dir = scratch("compare-runs");
    let run = |name: &str, seed: &str| {
        let out = dir.join(name);
        ok(
            &[
                "reference-benchmark",
                "examples/decks/red-green-zeon.txt",
                "examples/decks/blue-white-federation.txt",
                "--matches",
                "6",
                "--seed",
                seed,
                "--agent",
                "test-random",
                "--out",
                s(&out),
            ],
            &root(),
        );
        out
    };
    let a = run("a", "5");
    let b = run("b", "5");
    let c = run("c", "6");
    let same = ok(&["compare-runs", s(&a), s(&b)], &root());
    let doc: Value = serde_json::from_slice(&same.stdout).expect("JSON");
    let p = &doc["games"]["paired"];
    assert_eq!(
        (p["a_only"].as_u64(), p["b_only"].as_u64()),
        (Some(0), Some(0))
    );
    assert_eq!(p["difference"], 0.0);
    assert_eq!(p["mcnemar_p"], 1.0);
    assert_eq!(doc["matches"]["paired"]["difference"], 0.0);
    let refused = gcg_eval(&["compare-runs", s(&a), s(&c)], &root());
    assert_eq!(refused.status.code(), Some(1));
    assert!(stderr(&refused).contains("do not share their master seed"));

    let base: Vec<(u64, u64, u64, &str)> = (0..200)
        .map(|i| (i, 0, 1000 + i, if i % 2 == 0 { "dut" } else { "bench" }))
        .collect();
    let better: Vec<(u64, u64, u64, &str)> = base
        .iter()
        .map(|&(m, g, seed, w)| {
            (
                m,
                g,
                seed,
                if w == "bench" && m % 20 == 1 {
                    "dut"
                } else {
                    w
                },
            )
        })
        .collect();
    write_games(&dir.join("base"), &base);
    write_games(&dir.join("better"), &better);
    let o = ok(
        &["compare-runs", s(&dir.join("better")), s(&dir.join("base"))],
        &root(),
    );
    let doc: Value = serde_json::from_slice(&o.stdout).expect("JSON");
    let g = &doc["games"];
    let width = |v: &Value| v[1].as_f64().unwrap_or(0.0) - v[0].as_f64().unwrap_or(0.0);
    assert_eq!(g["paired"]["a_only"], 10);
    assert_eq!(g["paired"]["b_only"], 0);
    assert!(
        g["paired"]["ci95"][0].as_f64().unwrap_or(0.0) > 0.0,
        "paired excludes 0: {g}"
    );
    assert!(
        g["unpaired"]["ci95"][0].as_f64().unwrap_or(1.0) < 0.0,
        "unpaired does not: {g}"
    );
    assert!(width(&g["paired"]["ci95"]) < width(&g["unpaired"]["ci95"]));
}

/// W-40: the same seed gives bit-identical weights for any worker count, recorded positions
/// refit to the same weights, and the report holds the CV, the held-out check and a
/// wins-only validation row.
#[test]
fn w40_tune_is_deterministic_and_refits_recorded_positions() {
    let dir = scratch("tune");
    let positions = dir.join("positions.ndjson");
    let args = [
        "tune",
        "--agent",
        "greedy",
        "--decks",
        "tuning",
        "--pairs",
        "all",
        "--games",
        "24",
        "--folds",
        "3",
        "--validate",
        "12",
        "--seed",
        "10000001",
        "--role",
        "tuning",
        "--ledger",
    ];
    let ledger = dir.join("ledger.json");
    let mut a: Vec<&str> = args.to_vec();
    a.push(s(&ledger));
    let doc = same_for_workers(&a, &dir, "tune");
    assert_eq!(doc["feature_set"], "roles");
    assert_eq!(doc["cv"].as_array().map(Vec::len), Some(5));
    let lambda = doc["lambda"].as_f64().expect("lambda");
    assert!(
        doc["cv"]
            .as_array()
            .expect("cv")
            .iter()
            .any(|c| c["lambda"].as_f64() == Some(lambda))
    );
    assert!(doc["positions"]["train"].as_u64().unwrap_or(0) > 0);
    assert!(doc["positions"]["held_out"].as_u64().unwrap_or(0) > 0);
    assert_eq!(
        doc["held_out_check"]["pairs"],
        json!([["tuning-wing", "tuning-tekkadan"]])
    );
    assert!(doc["held_out_check"]["logloss_fitted"].is_f64());
    let v = &doc["validation"];
    assert_eq!(v["summary"]["n"], 12);
    let r = &v["result"];
    assert!(r["wins"].as_u64().unwrap_or(0) + r["draws"].as_u64().unwrap_or(0) <= 12);
    let mut a: Vec<&str> = args.to_vec();
    a.extend([s(&ledger), "--record", s(&positions), "--validate", "0"]);
    ok(&a, &root());
    let refit = dir.join("refit.json");
    ok(
        &[
            "tune",
            "--agent",
            "greedy",
            "--decks",
            "tuning",
            "--pairs",
            "all",
            "--folds",
            "3",
            "--seed",
            "10000001",
            "--positions",
            s(&positions),
            "--out",
            s(&refit),
        ],
        &root(),
    );
    assert_eq!(read_json(&refit)["weights"], doc["weights"]);
}

/// W-43: the oracle against itself has regret exactly 0; the report is the same for any
/// worker count and groups positions by kind and target decisions.
#[test]
fn w43_regret_of_an_agent_against_itself_is_zero() {
    let dir = scratch("regret");
    let doc = same_for_workers(
        &[
            "regret",
            "greedy",
            "greedy",
            "--decks",
            "tuning",
            "--pairs",
            "all",
            "--games",
            "12",
            "--per-game",
            "1",
            "--worlds",
            "2",
        ],
        &dir,
        "self",
    );
    assert_eq!(doc["all"]["positions"], 12);
    assert_eq!(doc["all"]["differing"], 0);
    assert_eq!(doc["all"]["mean_regret"], 0.0);
    for row in doc["outcomes"].as_array().expect("rows") {
        assert_eq!((row[5].as_bool(), row[6].as_f64()), (Some(true), Some(0.0)));
    }
    let other = same_for_workers(
        &[
            "regret",
            "random",
            "greedy",
            "--decks",
            "tuning",
            "--pairs",
            "all",
            "--games",
            "12",
            "--per-game",
            "1",
            "--worlds",
            "2",
        ],
        &dir,
        "random",
    );
    assert!(other["all"]["differing"].as_u64().unwrap_or(0) > 0);
    assert!(other["by_kind"].as_array().is_some_and(|k| !k.is_empty()));
}

/// W-43: an agent that picks random targets shows positive target regret against a one-ply
/// look-ahead, with a 95% interval that excludes 0, and the same report at 1 and 8 workers.
#[test]
fn w43_random_targets_show_positive_target_regret() {
    let dir = scratch("regret-targets");
    let mut texts = Vec::new();
    for w in ["1", "8"] {
        let out = dir.join(format!("targets-{w}.json"));
        ok(
            &[
                "regret",
                "random",
                "greedy",
                "--decisions",
                "target",
                "--decks",
                "tuning",
                "--pairs",
                "all",
                "--games",
                "120",
                "--per-game",
                "4",
                "--worlds",
                "8",
                "--workers",
                w,
                "--out",
                s(&out),
            ],
            &root(),
        );
        texts.push(std::fs::read(&out).expect("report"));
    }
    assert_eq!(texts[0], texts[1], "1 vs 8 workers");
    let doc = read_json(&dir.join("targets-1.json"));
    let t = &doc["target"];
    assert_eq!(
        t["positions"], doc["all"]["positions"],
        "only target decisions are sampled"
    );
    assert!(t["positions"].as_u64().unwrap_or(0) >= 200, "{t}");
    assert!(t["mean_regret"].as_f64().unwrap_or(0.0) > 0.0, "{t}");
    assert!(
        t["ci95"][0].as_f64().unwrap_or(-1.0) > 0.0,
        "the interval excludes 0: {t}"
    );
}

/// W-18: two runs of the same agent rank deck variants identically on the same deals; the
/// report is the same for any worker count.
#[test]
fn w18_fidelity_of_an_agent_with_itself_is_perfect() {
    let dir = scratch("fidelity");
    let doc = same_for_workers(
        &[
            "fidelity",
            "greedy",
            "greedy",
            "--variants",
            "red-green-zeon,purple-white-tekkadan,blue-white-federation",
            "--against",
            "tests-seed",
            "--games",
            "8",
        ],
        &dir,
        "self",
    );
    assert_eq!(doc["by_agent"][0]["results"], doc["by_agent"][1]["results"]);
    assert_eq!(doc["same_best"], true);
    assert!(
        doc["spearman"] == 1.0 || doc["spearman"].is_null(),
        "identical rankings: {}",
        doc["spearman"]
    );
    for v in doc["by_variant"].as_array().expect("variants") {
        assert_eq!(v["paired"]["difference"], 0.0);
    }
    let low = gcg_eval(
        &[
            "fidelity",
            "random",
            "greedy",
            "--variants",
            "red-green-zeon,purple-white-tekkadan",
            "--against",
            "tests-seed",
            "--games",
            "4",
            "--min-spearman",
            "1.5",
        ],
        &root(),
    );
    assert_eq!(low.status.code(), Some(1), "a bar above 1 always fails");
    assert!(stderr(&low).contains("--min-spearman"));
}

/// W-41: a synthetic fixture whose observed counts equal the simulated ones has zero
/// calibration error; a known shift is reported as bias; malformed files are refused.
#[test]
fn w41_calibrate_on_synthetic_fixtures() {
    let dir = scratch("calibrate");
    let fixture = root().join("crates/gcg-eval/tests/fixtures/observed-matches.synthetic.json");
    let first = dir.join("first.json");
    ok(
        &[
            "calibrate",
            s(&fixture),
            "--agent",
            "random",
            "--games",
            "8",
            "--out",
            s(&first),
        ],
        &root(),
    );
    let doc = read_json(&first);
    let sims: Vec<u64> = doc["matchups"]
        .as_array()
        .expect("matchups")
        .iter()
        .map(|m| m["simulated"]["wins"].as_u64().expect("wins"))
        .collect();
    let mut base = read_json(&fixture);
    base["matches"][2]["deck_a"] = json!(s(&root().join("examples/decks/red-green-zeon.txt")));
    let with = |shift: u64| {
        let mut v = base.clone();
        for (k, m) in v["matches"]
            .as_array_mut()
            .expect("matches")
            .iter_mut()
            .enumerate()
        {
            let w = (sims[k] + shift).min(8);
            m["wins"] = json!(w);
            m["losses"] = json!(8 - w);
            m["draws"] = json!(0);
        }
        v
    };
    let equal = write_report(&dir, "equal.json", &with(0));
    let out = dir.join("equal-out.json");
    ok(
        &[
            "calibrate",
            s(&equal),
            "--agent",
            "random",
            "--games",
            "8",
            "--out",
            s(&out),
        ],
        &root(),
    );
    let doc = read_json(&out);
    assert_eq!(
        (doc["ece"].as_f64(), doc["bias"].as_f64()),
        (Some(0.0), Some(0.0))
    );
    let biased = write_report(&dir, "biased.json", &with(2));
    let out = dir.join("biased-out.json");
    ok(
        &[
            "calibrate",
            s(&biased),
            "--agent",
            "random",
            "--games",
            "8",
            "--out",
            s(&out),
        ],
        &root(),
    );
    let doc = read_json(&out);
    assert!(doc["bias"].as_f64().unwrap_or(0.0) > 0.1, "{}", doc["bias"]);
    for (name, bad) in [
        ("negative", json!({"wins": -1})),
        ("date", json!({"date": "2026-02-30"})),
        ("extra", json!({"rating": 3})),
    ] {
        let mut v = base.clone();
        if let (Some(m), Some(patch)) = (v["matches"][0].as_object_mut(), bad.as_object()) {
            for (k, x) in patch {
                m.insert(k.clone(), x.clone());
            }
        }
        let p = write_report(&dir, &format!("{name}.json"), &v);
        let refused = gcg_eval(
            &["calibrate", s(&p), "--agent", "random", "--games", "4"],
            &root(),
        );
        assert_eq!(refused.status.code(), Some(1), "{name}");
        assert!(refused.stdout.is_empty(), "{name}: nothing was played");
    }
}

/// W-127: cited commits must resolve, cited evidence paths must exist, and manifest entries
/// must match their files.
#[test]
fn w127_evidence_check_resolves_commits_and_paths() {
    let dir = scratch("evidence");
    let git = |args: &[&str]| {
        let o = Command::new("git")
            .args(args)
            .current_dir(&dir)
            .output()
            .expect("git");
        assert!(
            o.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&o.stderr)
        );
        String::from_utf8_lossy(&o.stdout).trim().to_owned()
    };
    git(&["init", "-q"]);
    git(&["config", "user.email", "test@example.invalid"]);
    git(&["config", "user.name", "test"]);
    std::fs::create_dir_all(dir.join("docs/port/eval/evidence")).expect("dirs");
    std::fs::write(
        dir.join("docs/port/eval/evidence/row.json"),
        "{\"wins\": 1}\n",
    )
    .expect("row");
    git(&["add", "."]);
    git(&["commit", "-q", "-m", "evidence"]);
    let head = git(&["rev-parse", "--short=10", "HEAD"]);
    let added = ok(
        &[
            "evidence",
            "add",
            "docs/port/eval/evidence/row.json",
            "--command",
            "gcg-eval ablation ...",
        ],
        &dir,
    );
    assert!(String::from_utf8_lossy(&added.stdout).contains("\"sha256\""));
    std::fs::write(
        dir.join("docs/port/eval/notes.md"),
        format!("Measured at commit {head}; raw output `docs/port/eval/evidence/row.json`.\n"),
    )
    .expect("notes");
    let good = gcg_eval(&["evidence", "check"], &dir);
    assert_eq!(
        good.status.code(),
        Some(0),
        "{}{}",
        stderr(&good),
        String::from_utf8_lossy(&good.stdout)
    );
    std::fs::write(
        dir.join("docs/port/eval/bad.md"),
        "Measured at commit 5a6f226 (squashed away); `docs/port/eval/evidence/gone.json`.\n",
    )
    .expect("bad");
    let bad = gcg_eval(&["evidence", "check"], &dir);
    assert_eq!(bad.status.code(), Some(1));
    let doc: Value = serde_json::from_slice(&bad.stdout).expect("JSON");
    assert!(
        doc["commits"]
            .as_array()
            .expect("commits")
            .iter()
            .any(|c| c["commit"] == "5a6f226" && c["resolves"] == false)
    );
    assert!(
        doc["paths"]
            .as_array()
            .expect("paths")
            .iter()
            .any(|p| p["exists"] == false)
    );
    std::fs::remove_file(dir.join("docs/port/eval/bad.md")).expect("rm");
    std::fs::write(
        dir.join("docs/port/eval/evidence/row.json"),
        "{\"wins\": 2}\n",
    )
    .expect("edit");
    let changed = gcg_eval(&["evidence", "check"], &dir);
    assert_eq!(
        changed.status.code(),
        Some(1),
        "an edited evidence file no longer matches"
    );
}

/// W-21: decision traces leave the head-to-head report and the benchmark reports
/// byte-identical, list every root move, and their visits sum to the iterations used.
#[test]
fn w21_decision_traces_change_no_report_and_count_every_iteration() {
    let dir = scratch("trace");
    let plain = dir.join("plain.json");
    let traced = dir.join("traced.json");
    let trace = dir.join("trace.ndjson");
    let base = [
        "head-to-head",
        TINY,
        "greedy",
        "--decks",
        "tests",
        "--games",
        "4",
        "--workers",
        "2",
    ];
    let mut a: Vec<&str> = base.to_vec();
    a.extend(["--out", s(&plain)]);
    ok(&a, &root());
    let mut a: Vec<&str> = base.to_vec();
    a.extend(["--out", s(&traced), "--decision-trace", s(&trace)]);
    ok(&a, &root());
    assert_eq!(std::fs::read(&plain).ok(), std::fs::read(&traced).ok());
    let text = std::fs::read_to_string(&trace).expect("trace");
    let (mut searches, mut greedy) = (0, 0);
    for line in text.lines() {
        let v: Value = serde_json::from_str(line).expect("a trace line");
        if let Some(search) = v["search"].as_object() {
            searches += 1;
            let alts = v["alternatives"].as_array().expect("alternatives");
            let visits: u64 = alts.iter().map(|a| a[2].as_u64().unwrap_or(0)).sum();
            assert_eq!(Some(visits), search["iterations"].as_u64(), "{line}");
            if search["roots"].as_u64().unwrap_or(0) > 1 {
                assert_eq!(Some(alts.len() as u64), search["roots"].as_u64(), "{line}");
            }
        }
        if v["agent"] == "greedy" && v["options"].as_u64().unwrap_or(0) > 1 && v["kind"] != "redraw"
        {
            assert!(v["greedy_cap_hits"].is_u64(), "{line}");
            greedy += 1;
        }
    }
    assert!(
        searches > 10 && greedy > 10,
        "{searches} searches, {greedy} greedy decisions"
    );

    let config = dir.join("tiny.json");
    let Ok(gcg_eval::AgentDesc::Search { config: cfg }) =
        gcg_eval::agent_spec::parse_agent_spec(TINY)
    else {
        panic!("tiny")
    };
    std::fs::write(
        &config,
        gcg_core::pyjson::dumps(
            &gcg_eval::search_config::to_json(&cfg),
            gcg_core::pyjson::DumpOptions::PRETTY_SORTED_UTF8,
        ),
    )
    .expect("config");
    let bench = |out: &Path, extra: &[&str]| {
        let mut a = vec![
            "preset-benchmark",
            "examples/decks/red-green-zeon.txt",
            "examples/decks/blue-white-federation.txt",
            "--matches",
            "2",
            "--format",
            "bo1",
            "--search-config",
            s(&config),
            "--decision-log",
            "--out",
            s(out),
        ];
        a.extend(extra);
        ok(&a, &root());
    };
    let (o1, o2) = (dir.join("b-plain"), dir.join("b-traced"));
    let btrace = dir.join("bench-trace.ndjson");
    bench(&o1, &[]);
    bench(&o2, &["--decision-trace", s(&btrace)]);
    for f in ["games.ndjson", "summary.md", "results.json"] {
        let (x, y) = (
            std::fs::read(o1.join(f)).ok(),
            std::fs::read(o2.join(f)).ok(),
        );
        if f != "games.ndjson" {
            assert_eq!(
                x.map(|t| String::from_utf8_lossy(&t).replace(s(&o1), "OUT")),
                y.map(|t| String::from_utf8_lossy(&t).replace(s(&o2), "OUT")),
                "{f}"
            );
        } else {
            assert_eq!(x, y, "{f}");
        }
    }
    let mut names: Vec<_> = std::fs::read_dir(o1.join("replays"))
        .expect("replays")
        .filter_map(|e| e.ok().map(|e| e.file_name()))
        .collect();
    names.sort();
    for n in names {
        assert_eq!(
            std::fs::read(o1.join("replays").join(&n)).ok(),
            std::fs::read(o2.join("replays").join(&n)).ok(),
            "{n:?}"
        );
    }
    let lines = std::fs::read_to_string(&btrace).expect("bench trace");
    assert!(lines.lines().count() > 20);
    assert!(lines.lines().all(|l| l.starts_with("{\"game\":")));
}

/// W-22: a forced cap hit is counted. With a cap of no decision every move that keeps the turn
/// going is cut; greedy's own cap of 200 cuts no more moves than that.
#[test]
fn w22_greedy_cap_hits_are_counted() {
    use gcg_engine::{DecisionKind, NewGameOptions, apply, new_game};
    let reg = gcg_effects::Registry::embedded();
    let decks = gcg_eval::decks::embedded(gcg_eval::DeckSet::Tests).expect("tests");
    let mut st = new_game(
        &reg,
        [&decks[0].deck, &decks[1].deck],
        7,
        NewGameOptions::default(),
    )
    .expect("a game");
    let mut checked = 0;
    for step in 0..400 {
        let Some((player, kind, n)) = st
            .pending()
            .map(|d| (d.hdr.player, d.hdr.kind, d.options.len()))
        else {
            break;
        };
        if kind == DecisionKind::Main && n > 2 {
            let seed = gcg_core::derive_seed!(9, "cap", step);
            let forced = gcg_eval::trace::greedy_cap_hits(&st, player, seed, 0).expect("count");
            let normal =
                gcg_eval::trace::greedy_cap_hits(&st, player, seed, gcg_ai::GREEDY_MAX_DECISIONS)
                    .expect("count");
            assert!(
                forced.is_some_and(|h| h > 0),
                "a cap of 0 cuts every move that keeps the turn"
            );
            assert!(normal <= forced, "a larger cap cuts no more moves");
            checked += 1;
            if checked == 3 {
                break;
            }
        }
        let first = st.pending().map(|d| d.options[0]).expect("an option");
        apply(&mut st, first, true).expect("apply");
    }
    assert_eq!(checked, 3);
}
