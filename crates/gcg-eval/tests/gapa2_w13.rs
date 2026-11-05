//! W-13's end-to-end evidence (GAP-A2, docs/port/eval/weakness-status/GAP-A2.md): the decision
//! trace counts the branches of the no-op Rest-target prune, and on the effect-coverage decks
//! (`docs/port/eval/GAP-A2/decks/`, every packaged card that reads its own rested state) its
//! "while rested" exception fires in 100 `gcg-eval` games.
//!
//! - `w13_the_coverage_decks_fire_the_while_rested_exception`: 100 head-to-head games of two
//!   cheap null-configuration searches on the White/Blue coverage mirror report a non-zero
//!   firing count,
//!   and the trace leaves the head-to-head report byte-identical (A13: the counter replays the
//!   prune and changes no decision).
//! - `w13_the_counted_branch_is_the_prune_s_own`: over policy games on the coverage decks, at
//!   every SELECT the counter says `Pruned` exactly when `prune_dominated` with `rest_targets`
//!   removes moves; the "while rested" exception occurs in the mirror, the Rested-trigger one
//!   against the Red/Green coverage deck.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use gcg_ai::{
    BASELINE_KNOBS, PruneFlags, RestPrune, canonical, choose, prune_dominated, rest_prune,
};
use gcg_engine::{DecisionKind, NewGameOptions, apply, new_game};
use serde_json::Value;

const TINY: &str = "search:null-fast:max_iterations=8,min_iterations=4,iterations_per_option=2";
const WHITE_BLUE: &str = "docs/port/eval/GAP-A2/decks/coverage-white-blue.txt";
const RED_GREEN: &str = "docs/port/eval/GAP-A2/decks/coverage-red-green.txt";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("gapa2-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn ok(args: &[&str]) -> Output {
    let o = Command::new(env!("CARGO_BIN_EXE_gcg-eval"))
        .args(args)
        .current_dir(root())
        .env_remove("COLUMNS")
        .output()
        .expect("gcg-eval runs");
    assert_eq!(
        o.status.code(),
        Some(0),
        "{args:?}: {}",
        String::from_utf8_lossy(&o.stderr)
    );
    o
}

fn s(p: &Path) -> &str {
    p.to_str().expect("UTF-8 path")
}

/// The `rest_prune` values of a decision trace, counted (`null` as `"none"`).
fn branches(trace: &Path) -> BTreeMap<String, u64> {
    let mut out = BTreeMap::new();
    for line in std::fs::read_to_string(trace).expect("a trace").lines() {
        let v: Value = serde_json::from_str(line).expect("a trace line");
        let b = v["rest_prune"].as_str().unwrap_or("none").to_owned();
        if v["search"].is_null() {
            assert_eq!(
                b, "none",
                "only searched decisions carry the branch: {line}"
            );
        }
        *out.entry(b).or_insert(0) += 1;
    }
    out
}

#[test]
fn w13_the_coverage_decks_fire_the_while_rested_exception() {
    let dir = scratch("w13-e2e");
    let pairs = format!("{WHITE_BLUE}:{WHITE_BLUE}");
    let base = [
        "head-to-head",
        TINY,
        TINY,
        "--pair",
        &pairs,
        "--games",
        "100",
        "--seed",
        "13",
        "--workers",
        "2",
    ];
    let (plain, traced, trace) = (
        dir.join("plain.json"),
        dir.join("traced.json"),
        dir.join("trace.ndjson"),
    );
    let mut a: Vec<&str> = base.to_vec();
    a.extend(["--out", s(&plain)]);
    ok(&a);
    let mut a: Vec<&str> = base.to_vec();
    a.extend(["--out", s(&traced), "--decision-trace", s(&trace)]);
    ok(&a);
    assert_eq!(
        std::fs::read(&plain).ok(),
        std::fs::read(&traced).ok(),
        "the counter changes no game"
    );
    let report: Value =
        serde_json::from_str(&std::fs::read_to_string(&plain).expect("report")).expect("JSON");
    assert_eq!(report["games"], 100, "{report}");
    let counts = branches(&trace);
    eprintln!("W-13 coverage decks, 100 games: {counts:?}");
    assert!(
        counts.get("while_rested").copied().unwrap_or(0) > 0,
        "the while-rested exception never fired: {counts:?}"
    );
    assert!(counts.get("pruned").copied().unwrap_or(0) > 0, "{counts:?}");
}

#[test]
fn w13_the_counted_branch_is_the_prune_s_own() {
    let reg = gcg_effects::Registry::embedded();
    let deck = |p: &str| {
        gcg_eval::decks::resolve_deck(s(&root().join(p)))
            .expect("a coverage deck")
            .deck
    };
    let (wb, rg) = (deck(WHITE_BLUE), deck(RED_GREEN));
    let flags = PruneFlags {
        strict_attacks: true,
        rest_targets: true,
    };
    let mut seen: BTreeMap<&str, u64> = BTreeMap::new();
    for game in 0..40u64 {
        let decks = if game % 2 == 0 {
            [&wb, &wb]
        } else {
            [&wb, &rg]
        };
        let mut st = new_game(&reg, decks, game, NewGameOptions::default()).expect("a game");
        for _ in 0..3000 {
            if st.winner().is_some() {
                break;
            }
            let Some(dec) = st.pending() else { break };
            if dec.hdr.kind == DecisionKind::Select && dec.options.len() > 1 {
                let moves = canonical(&st, &dec);
                let pruned = prune_dominated(&st, &dec, moves.clone(), flags).expect("prune");
                let branch = rest_prune(&st, &dec, &moves).expect("branch");
                let removed = pruned.len() < moves.len();
                assert_eq!(
                    removed,
                    branch == Some(RestPrune::Pruned),
                    "game {game}: {branch:?} but the prune kept {} of {}",
                    pruned.len(),
                    moves.len()
                );
                let name = match branch {
                    None => "none",
                    Some(RestPrune::Pruned) => "pruned",
                    Some(RestPrune::RestTrigger) => "rest_trigger",
                    Some(RestPrune::WhileRested) => "while_rested",
                };
                *seen.entry(name).or_insert(0) += 1;
            }
            let a = choose(&st, None, 0.0, &BASELINE_KNOBS).expect("a policy move");
            apply(&mut st, a, true).expect("apply");
        }
    }
    eprintln!("W-13 SELECT decisions of 40 policy games: {seen:?}");
    for branch in ["pruned", "rest_trigger", "while_rested"] {
        assert!(
            seen.get(branch).copied().unwrap_or(0) > 0,
            "{branch}: {seen:?}"
        );
    }
}
