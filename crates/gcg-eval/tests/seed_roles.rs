//! Seed roles and the seed ledger (T-081; PLAN A14, §12.5, §12.6), through the `gcg-eval`
//! binary with the random agent (cheap games):
//!
//! - the five master ranges are pairwise disjoint, and so are the game and agent seeds that
//!   head-to-head derives from them (`comparison_specs` seeding), checked at both ends and the
//!   middle of every range;
//! - a master outside its role's range is refused before any game, for every role and both
//!   edges, and nothing is recorded;
//! - every finished role run is appended to the ledger once, a rerun is not repeated, and a
//!   ledger with an entry outside its role's range fails the run;
//! - the roles that steer the search (`tuning`, `dev`, `confirm`) cannot play a held-out pair
//!   of the pre-registered split, while `eval` and `review` can.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use gcg_eval::decks::embedded;
use gcg_eval::specs::{comparison_specs, pairing_master};
use gcg_eval::{AgentDesc, DeckSet, SeedRole};
use serde_json::Value;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("seed-roles-{name}"));
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

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn read_json(path: &Path) -> Value {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

#[test]
fn role_ranges_and_the_seeds_they_derive_are_disjoint() {
    for a in SeedRole::ALL {
        for b in SeedRole::ALL {
            let ((af, al), (bf, bl)) = (a.bounds(), b.bounds());
            assert!(a == b || al < bf || bl < af, "{a:?} overlaps {b:?}");
        }
    }
    let decks: Vec<_> = embedded(DeckSet::Tuning)
        .expect("tuning decks")
        .into_iter()
        .map(|d| d.deck)
        .collect();
    let agents = (
        gcg_eval::agent_spec::parse_agent_spec("random").expect("random"),
        gcg_eval::agent_spec::parse_agent_spec("greedy").expect("greedy"),
    );
    let (challenger, opponent): (AgentDesc, AgentDesc) = agents;
    let mut owner: BTreeMap<u64, SeedRole> = BTreeMap::new();
    let mut seeds = 0_usize;
    for role in SeedRole::ALL {
        let (first, last) = role.bounds();
        for master in [first, first + (last - first) / 2, last] {
            let specs =
                comparison_specs(24, master, &challenger, &opponent, &decks).expect("specs");
            let mut derived: Vec<u64> = (0..3).map(|p| pairing_master(master, p, 3)).collect();
            for s in &specs {
                derived.push(s.game_seed);
                derived.extend(s.agent_seeds);
            }
            for seed in derived {
                seeds += 1;
                if let Some(other) = owner.insert(seed, role) {
                    assert_eq!(other, role, "seed {seed} is used by {other:?} and {role:?}");
                }
            }
        }
    }
    assert!(seeds > 1000, "{seeds} derived seeds");
}

#[test]
fn a_master_outside_its_range_is_refused_for_every_role() {
    let dir = scratch("refused");
    let ledger = dir.join("ledger.json");
    let ledger_arg = ledger.to_str().expect("UTF-8");
    for role in SeedRole::ALL {
        let (first, last) = role.bounds();
        for seed in [first - 1, last + 1] {
            let seed_arg = seed.to_string();
            let done = gcg_eval(
                &[
                    "head-to-head",
                    "random",
                    "random",
                    "--decks",
                    "tuning",
                    "--games",
                    "12",
                    "--role",
                    role.as_str(),
                    "--seed",
                    &seed_arg,
                    "--ledger",
                    ledger_arg,
                ],
                &root(),
            );
            assert_eq!(done.status.code(), Some(1), "{role:?} {seed}");
            assert_eq!(
                stderr(&done),
                format!(
                    "error: --seed {seed} is outside the {} range [{first}, {last}] (PLAN A14)\n",
                    role.as_str()
                )
            );
            assert!(done.stdout.is_empty(), "no game was played");
        }
    }
    assert!(!ledger.exists(), "a refused run records nothing");
    let _ = std::fs::remove_dir_all(&dir);
}

fn role_run(role: &str, seed: u64, ledger: &Path) -> Output {
    let seed = seed.to_string();
    gcg_eval(
        &[
            "head-to-head",
            "random",
            "random",
            "--decks",
            "tuning",
            "--games",
            "12",
            "--role",
            role,
            "--seed",
            &seed,
            "--ledger",
            ledger.to_str().expect("UTF-8"),
        ],
        &root(),
    )
}

fn entries(ledger: &Path) -> Vec<Value> {
    read_json(ledger)["entries"]
        .as_array()
        .expect("entries")
        .clone()
}

#[test]
fn role_runs_are_recorded_once_and_a_bad_ledger_fails() {
    let dir = scratch("ledger");
    let ledger = dir.join("nested").join("seed-ledger.json");
    let dev = SeedRole::Dev.bounds().0;
    let done = role_run("dev", dev, &ledger);
    assert_eq!(done.status.code(), Some(0), "{}", stderr(&done));
    let first = entries(&ledger);
    assert_eq!(first.len(), 1);
    assert_eq!(first[0]["role"], "dev");
    assert_eq!(first[0]["master"], dev);
    assert_eq!(first[0]["command"], "head-to-head");
    assert_eq!(first[0]["config"]["challenger"], "random");
    assert_eq!(first[0]["config"]["decks"], "tuning");
    assert_eq!(first[0]["config"]["games"], 12);
    assert_eq!(read_json(&ledger)["version"], 1);

    let rerun = role_run("dev", dev, &ledger);
    assert_eq!(rerun.status.code(), Some(0));
    assert_eq!(entries(&ledger), first, "a rerun is not repeated");

    assert_eq!(role_run("dev", dev + 1, &ledger).status.code(), Some(0));
    let tuning = SeedRole::Tuning.bounds().1;
    assert_eq!(role_run("tuning", tuning, &ledger).status.code(), Some(0));
    let all = entries(&ledger);
    assert_eq!(
        all.iter()
            .map(|e| (
                e["role"].as_str().unwrap_or(""),
                e["master"].as_u64().unwrap_or(0)
            ))
            .collect::<Vec<_>>(),
        [("dev", dev), ("dev", dev + 1), ("tuning", tuning)]
    );

    std::fs::write(
        &ledger,
        "{\"version\": 1, \"entries\": [{\"role\": \"eval\", \"master\": 20000000}]}\n",
    )
    .expect("write");
    let bad = role_run("dev", dev, &ledger);
    assert_eq!(bad.status.code(), Some(1));
    assert!(
        stderr(&bad).contains("entry 0 has no valid role and in-range master"),
        "{}",
        stderr(&bad)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn steering_roles_cannot_play_held_out_pairs() {
    // A working directory with a pre-registered split that holds out every pair with
    // tuning-wing (two of the three tuning pairs).
    let dir = scratch("held-out");
    std::fs::create_dir_all(dir.join("docs/dev/eval")).expect("dirs");
    std::fs::write(
        dir.join("docs/dev/eval/panel.json"),
        "{\"held_out\": [\"tuning-wing\"]}\n",
    )
    .expect("panel");
    let ledger = dir.join("ledger.json");
    let ledger_arg = ledger.to_str().expect("UTF-8");
    for role in [SeedRole::Tuning, SeedRole::Dev, SeedRole::Confirm] {
        let seed = role.bounds().0.to_string();
        let refused = gcg_eval(
            &[
                "head-to-head",
                "random",
                "random",
                "--decks",
                "tuning",
                "--games",
                "12",
                "--role",
                role.as_str(),
                "--seed",
                &seed,
                "--ledger",
                ledger_arg,
            ],
            &dir,
        );
        assert_eq!(refused.status.code(), Some(1), "{role:?}");
        assert_eq!(
            stderr(&refused),
            format!(
                "error: --role {} may not play the held-out pair tuning-zeon:tuning-wing (PLAN \
                 §12.5)\n",
                role.as_str()
            )
        );
        let out = dir.join(format!("{}.json", role.as_str()));
        let tuning_pairs = gcg_eval(
            &[
                "head-to-head",
                "random",
                "random",
                "--decks",
                "tuning",
                "--pairs",
                "tuning",
                "--games",
                "4",
                "--role",
                role.as_str(),
                "--seed",
                &seed,
                "--ledger",
                ledger_arg,
                "--out",
                out.to_str().expect("UTF-8"),
            ],
            &dir,
        );
        assert_eq!(
            tuning_pairs.status.code(),
            Some(0),
            "{}",
            stderr(&tuning_pairs)
        );
        let doc = read_json(&out);
        let pairs: Vec<&Value> = doc["by_pair"].as_array().expect("by_pair").iter().collect();
        assert_eq!(pairs.len(), 1);
        assert_eq!(
            pairs[0]["decks"],
            serde_json::json!(["tuning-zeon", "tuning-tekkadan"])
        );
        assert_eq!(pairs[0]["held_out"], false);
    }
    for role in [SeedRole::Eval, SeedRole::Review] {
        let seed = role.bounds().0.to_string();
        let out = dir.join(format!("{}.json", role.as_str()));
        let done = gcg_eval(
            &[
                "head-to-head",
                "random",
                "random",
                "--decks",
                "tuning",
                "--pairs",
                "held-out",
                "--games",
                "8",
                "--role",
                role.as_str(),
                "--seed",
                &seed,
                "--ledger",
                ledger_arg,
                "--out",
                out.to_str().expect("UTF-8"),
            ],
            &dir,
        );
        assert_eq!(done.status.code(), Some(0), "{}", stderr(&done));
        let doc = read_json(&out);
        assert_eq!(doc["held_out"]["pairs"], 2);
        assert!(
            doc["by_pair"]
                .as_array()
                .expect("by_pair")
                .iter()
                .all(|p| p["held_out"] == true)
        );
    }
    let roles: Vec<String> = entries(&ledger)
        .iter()
        .map(|e| e["role"].as_str().unwrap_or("").to_owned())
        .collect();
    assert_eq!(roles, ["tuning", "dev", "confirm", "eval", "review"]);
    let _ = std::fs::remove_dir_all(&dir);
}

/// R-10 (Phase 7 review): a held-out deck stays held out under another file name and with its
/// lines in another order, because the guard also compares the card multisets with the
/// held-out panel decks; a renamed copy of a tuning deck still plays.
#[test]
fn a_renamed_copy_of_a_held_out_deck_is_still_held_out() {
    let dir = scratch("renamed-held-out");
    let held = std::fs::read_to_string(root().join("examples/decks/eval/blue-red-tempo.txt"))
        .expect("the held-out deck");
    let copy = dir.join("my-tempo.txt");
    std::fs::write(&copy, &held).expect("copy");
    let mut lines: Vec<&str> = held.lines().collect();
    lines.reverse();
    let reordered = dir.join("reordered.txt");
    std::fs::write(&reordered, lines.join("\n") + "\n").expect("reordered copy");
    let tuning = dir.join("my-zeon.txt");
    std::fs::copy(root().join("examples/decks/red-green-zeon.txt"), &tuning).expect("copy");
    let ledger = dir.join("ledger.json");
    let run = |a: &Path, b: &str| {
        let pair = format!("{}:{b}", a.to_str().expect("UTF-8"));
        gcg_eval(
            &[
                "head-to-head",
                "random",
                "random",
                "--pair",
                &pair,
                "--games",
                "4",
                "--role",
                "dev",
                "--seed",
                "20000000",
                "--ledger",
                ledger.to_str().expect("UTF-8"),
            ],
            &root(),
        )
    };
    for deck in [&copy, &reordered] {
        let refused = run(deck, "purple-white-tekkadan");
        assert_eq!(refused.status.code(), Some(1), "{}", stderr(&refused));
        assert!(
            stderr(&refused).contains("may not play the held-out pair"),
            "{}",
            stderr(&refused)
        );
    }
    let played = run(&tuning, "purple-white-tekkadan");
    assert_eq!(played.status.code(), Some(0), "{}", stderr(&played));
    let _ = std::fs::remove_dir_all(&dir);
}
