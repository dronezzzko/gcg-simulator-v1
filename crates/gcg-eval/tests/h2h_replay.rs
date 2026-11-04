//! `gcg-eval head-to-head-replay` (PLAN §12.6: E3's losses documented with their replays),
//! through the `gcg-eval` binary on cheap agents (random and greedy on the AI test decks, dev
//! seeds, a ledger in the scratch directory):
//!
//! - the replays of `--index` games reproduce their rows (winner seat, turns), name the seats
//!   as the report does (role, agent, deck) and pass `check_replay`; `index.json` lists them;
//! - `--losses` selects exactly the rows whose winner is not the challenger, for a deck-set
//!   report and a `--pair` report;
//! - a report with one row's turns changed fails (exit 1) naming that game and field, and
//!   writes nothing; a spec override that plays other games fails the same way, one that plays
//!   the same games reproduces them;
//! - a document of another format or version is refused (exit 1), and `--index`/`--losses`
//!   are one-of usage errors (exit 2);
//! - two runs write identical bytes.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

fn root() -> PathBuf {
    fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")).expect("repo root")
}

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("h2h-replay-{name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn s(p: &Path) -> &str {
    p.to_str().expect("UTF-8 path")
}

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

fn gcg_eval(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_gcg-eval"))
        .args(args)
        .current_dir(root())
        .env_remove("COLUMNS")
        .output()
        .expect("gcg-eval runs")
}

fn ok(args: &[&str]) -> String {
    let o = gcg_eval(args);
    assert_eq!(o.status.code(), Some(0), "{args:?}: {}", text(&o.stderr));
    text(&o.stdout)
}

fn read_json(p: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(p).expect("readable")).expect("JSON")
}

/// A head-to-head report of `games` games between `challenger` and `opponent` on the AI test
/// decks (or on `pair`), written into `dir`.
fn report(
    dir: &Path,
    challenger: &str,
    opponent: &str,
    games: &str,
    pair: Option<&str>,
) -> PathBuf {
    let out = dir.join("report.json");
    let ledger = dir.join("ledger.json");
    let mut args = vec![
        "head-to-head",
        challenger,
        opponent,
        "--games",
        games,
        "--seed",
        "20000777",
        "--role",
        "dev",
        "--ledger",
        s(&ledger),
        "--out",
        s(&out),
    ];
    match pair {
        Some(p) => args.extend(["--pair", p]),
        None => args.extend(["--decks", "tests"]),
    }
    ok(&args);
    out
}

/// `[index, pair, challenger, winner, turns, first_player, decisions]` of every row.
fn rows(report: &Path) -> Vec<[i64; 7]> {
    read_json(report)["outcomes"]
        .as_array()
        .expect("outcomes")
        .iter()
        .map(|r| std::array::from_fn(|i| r[i].as_i64().expect("an integer column")))
        .collect()
}

fn losses(rows: &[[i64; 7]]) -> BTreeSet<i64> {
    rows.iter().filter(|r| r[3] != r[2]).map(|r| r[0]).collect()
}

fn indexed(index: &Path) -> BTreeSet<i64> {
    read_json(index)["games"]
        .as_array()
        .expect("games")
        .iter()
        .map(|g| g["index"].as_i64().expect("index"))
        .collect()
}

fn files(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut out: Vec<(String, Vec<u8>)> = fs::read_dir(dir)
        .expect("listable")
        .map(|e| {
            let e = e.expect("entry");
            (
                e.file_name().to_string_lossy().into_owned(),
                fs::read(e.path()).expect("readable"),
            )
        })
        .collect();
    out.sort();
    out
}

#[test]
fn indexed_replays_reproduce_their_rows_and_pass_check_replay() {
    let dir = scratch("indexed");
    let report = report(&dir, "random", "greedy", "8", None);
    let rows = rows(&report);
    let out = dir.join("replays");
    let stdout = ok(&[
        "head-to-head-replay",
        s(&report),
        "--index",
        "5",
        "--index",
        "2",
        "--index",
        "5",
        "--out",
        s(&out),
    ]);
    assert_eq!(stdout.lines().count(), 2, "{stdout}");
    assert!(stdout.starts_with("game 2: seat 0 "), "{stdout}");
    let doc_report = read_json(&report);
    for i in [2_usize, 5] {
        let row = rows[i];
        let doc = read_json(&out.join(format!("game-{i}.json")));
        assert_eq!(doc["format"], "gcg-sim-replay");
        assert_eq!(doc["version"], 1);
        assert_eq!(doc["label"], format!("head-to-head game {i}"));
        assert_eq!(doc["match_index"], i);
        assert_eq!(doc["game_index"], 1);
        assert_eq!(doc["chooser"], Value::Null);
        assert_eq!(doc["expected"]["winner_seat"], row[3]);
        assert_eq!(doc["expected"]["turns"], row[4]);
        let challenger = usize::try_from(row[2]).expect("seat");
        let pair = &doc_report["by_pair"][0]["decks"];
        for seat in 0..2 {
            let s = &doc["seats"][seat];
            assert_eq!(s["seat"], seat);
            let dut = seat == challenger;
            assert_eq!(s["role"], if dut { "dut" } else { "bench" });
            assert_eq!(s["agent"], if dut { "random" } else { "greedy" });
            let swap = (i % 4) / 2;
            assert_eq!(s["deck"], pair[seat ^ swap], "game {i} seat {seat}");
        }
        assert_eq!(
            doc["expected"]["winner"],
            if row[3] == row[2] { "dut" } else { "bench" }
        );
        let check = gcg_runner::replay::check_replay(&doc).expect("replays");
        assert!(check.ok, "game {i}: {:?}", check.mismatches);
        assert_eq!(check.turns, i32::try_from(row[4]).expect("turns"));
    }
    let index = read_json(&out.join("index.json"));
    assert_eq!(index["format"], "gcg-eval-head-to-head-replay");
    assert_eq!(index["report"], "report.json");
    assert_eq!(index["seed"], 20_000_777);
    assert_eq!(index["selection"], "index");
    assert_eq!(indexed(&out.join("index.json")), BTreeSet::from([2, 5]));
    let entry = &index["games"][1];
    assert_eq!(entry["replay"], "game-5.json");
    assert_eq!(entry["turns"], rows[5][4]);
    assert_eq!(entry["decisions"], rows[5][6]);
    assert_eq!(entry["first_player"], rows[5][5]);
    assert_eq!(entry["challenger_seat"], rows[5][2]);
    assert_eq!(
        entry["game_seed"],
        read_json(&out.join("game-5.json"))["seed"]
    );
    assert_eq!(files(&out).len(), 3);
}

#[test]
fn losses_select_every_game_the_challenger_did_not_win() {
    for (name, pair) in [("set", None), ("pair", Some("tests-seed:tests-federation"))] {
        let dir = scratch(&format!("losses-{name}"));
        let report = report(&dir, "random", "random", "16", pair);
        let lost = losses(&rows(&report));
        assert!(!lost.is_empty() && lost.len() < 16, "{name}: {lost:?}");
        let out = dir.join("replays");
        let stdout = ok(&[
            "head-to-head-replay",
            s(&report),
            "--losses",
            "--out",
            s(&out),
        ]);
        assert_eq!(stdout.lines().count(), lost.len(), "{name}");
        assert_eq!(indexed(&out.join("index.json")), lost, "{name}");
        for i in &lost {
            let doc = read_json(&out.join(format!("game-{i}.json")));
            assert!(
                gcg_runner::replay::check_replay(&doc).expect("replays").ok,
                "{name} game {i}"
            );
        }
        assert_eq!(files(&out).len(), lost.len() + 1, "{name}");
    }
}

#[test]
fn a_row_the_game_does_not_reproduce_fails_naming_the_game() {
    let dir = scratch("tampered");
    let report = report(&dir, "random", "greedy", "8", None);
    let mut doc = read_json(&report);
    let turns = doc["outcomes"][3][4].as_i64().expect("turns");
    doc["outcomes"][3][4] = (turns + 1).into();
    let tampered = dir.join("tampered.json");
    fs::write(&tampered, serde_json::to_string(&doc).expect("JSON")).expect("writable");
    let out = dir.join("replays");
    let o = gcg_eval(&[
        "head-to-head-replay",
        s(&tampered),
        "--index",
        "3",
        "--index",
        "4",
        "--out",
        s(&out),
    ]);
    assert_eq!(o.status.code(), Some(1));
    let err = text(&o.stderr);
    assert!(
        err.contains(&format!("game 3: turns {turns} (report {})", turns + 1)),
        "{err}"
    );
    assert!(!err.contains("game 4"), "{err}");
    assert!(!out.exists(), "nothing is written");

    let o = gcg_eval(&[
        "head-to-head-replay",
        s(&report),
        "--losses",
        "--opponent",
        "greedy:weights=hand",
        "--out",
        s(&out),
    ]);
    assert_eq!(o.status.code(), Some(1), "{}", text(&o.stderr));
    assert!(text(&o.stderr).contains("do not reproduce their rows"));
    assert!(!out.exists());
    ok(&[
        "head-to-head-replay",
        s(&report),
        "--index",
        "0",
        "--opponent",
        "greedy:weights=default",
        "--out",
        s(&out),
    ]);
    let index = read_json(&out.join("index.json"));
    assert_eq!(index["played"]["opponent"], "greedy:weights=default");
    assert_eq!(index["opponent"], "greedy");
    assert_eq!(
        read_json(&out.join("game-0.json"))["seats"][1]["agent"],
        "greedy"
    );
}

#[test]
fn other_documents_and_bad_selections_are_refused() {
    let dir = scratch("refused");
    let out = dir.join("replays");
    let gates = dir.join("gates.json");
    fs::write(&gates, r#"{"format": "gcg-eval-gates", "version": 1}"#).expect("writable");
    let report = report(&dir, "random", "greedy", "4", None);
    let mut v1 = read_json(&report);
    v1["version"] = 1.into();
    let old = dir.join("v1.json");
    fs::write(&old, serde_json::to_string(&v1).expect("JSON")).expect("writable");
    for file in [&gates, &old] {
        let o = gcg_eval(&["head-to-head-replay", s(file), "--losses", "--out", s(&out)]);
        assert_eq!(o.status.code(), Some(1));
        assert!(
            text(&o.stderr).contains("not a gcg-eval-head-to-head v2 report"),
            "{}",
            text(&o.stderr)
        );
    }
    let o = gcg_eval(&["head-to-head-replay", s(&report), "--out", s(&out)]);
    assert_eq!(o.status.code(), Some(2));
    assert!(text(&o.stderr).contains("one of the arguments --index --losses is required"));
    let o = gcg_eval(&[
        "head-to-head-replay",
        s(&report),
        "--index",
        "0",
        "--losses",
        "--out",
        s(&out),
    ]);
    assert_eq!(o.status.code(), Some(2));
    assert!(text(&o.stderr).contains("argument --losses: not allowed with argument --index"));
    let o = gcg_eval(&[
        "head-to-head-replay",
        s(&report),
        "--index",
        "4",
        "--out",
        s(&out),
    ]);
    assert_eq!(o.status.code(), Some(1));
    assert!(text(&o.stderr).contains("--index 4: the report has 4 games"));
    assert!(!out.exists());
}

#[test]
fn two_runs_write_identical_bytes() {
    let dir = scratch("determinism");
    let report = report(
        &dir,
        "random",
        "greedy",
        "8",
        Some("tests-federation:tests-seed"),
    );
    let runs: Vec<Vec<(String, Vec<u8>)>> = ["a", "b"]
        .iter()
        .map(|run| {
            let out = dir.join(run);
            ok(&[
                "head-to-head-replay",
                s(&report),
                "--losses",
                "--out",
                s(&out),
            ]);
            files(&out)
        })
        .collect();
    let lost = losses(&rows(&report));
    assert!(!lost.is_empty());
    assert_eq!(runs[0].len(), lost.len() + 1);
    assert_eq!(runs[0], runs[1]);
}
