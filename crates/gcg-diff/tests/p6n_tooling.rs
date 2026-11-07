//! P6-N tooling checks (docs/dev/PLAN.md §12.2, package P6-N; docs/dev/WEAKNESSES.md):
//!
//! - W-112: the slow test tier is exactly `scripts/slow-tier.txt`, slow tests are gated by
//!   their package's `slow` feature (never ignored), and the gate runs both tiers;
//! - W-115 and W-153: `scripts/gate_summary.py` reports the stale research artefacts against the
//!   packaged data commit and the newest news-scan date, with negative controls;
//! - W-122: the P6-N gate scripts never name the untracked bytecode directories;
//! - W-128: `scripts/doc_counts.py` regenerates every marked count and fails on a stale one;
//! - W-121: the card loader accepts recorded `cards.ndjson` records of every card type and
//!   names a renamed field in its error.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

fn repo() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

fn read(p: &Path) -> String {
    fs::read_to_string(p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

/// The workspace packages: `(name, directory)`, in directory order.
fn packages() -> Vec<(String, PathBuf)> {
    let mut dirs: Vec<PathBuf> = fs::read_dir(repo().join("crates"))
        .expect("crates/")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.join("Cargo.toml").is_file())
        .collect();
    dirs.sort();
    dirs.into_iter()
        .map(|d| {
            let text = read(&d.join("Cargo.toml"));
            let name = text
                .lines()
                .find_map(|l| l.strip_prefix("name = \""))
                .and_then(|l| l.strip_suffix('"'))
                .expect("a package name")
                .to_owned();
            (name, d)
        })
        .collect()
}

/// `[[test]]` targets of a manifest: `(path, required features)`.
fn test_targets(manifest: &str) -> Vec<(String, Vec<String>)> {
    let mut out = Vec::new();
    let mut current: Option<(String, Vec<String>)> = None;
    for line in manifest.lines().map(str::trim) {
        if line.starts_with('[') {
            out.extend(current.take());
            if line == "[[test]]" {
                current = Some((String::new(), Vec::new()));
            }
            continue;
        }
        let Some((path, feats)) = current.as_mut() else {
            continue;
        };
        if let Some(p) = line.strip_prefix("path = \"") {
            *path = p.trim_end_matches('"').to_owned();
        } else if let Some(f) = line.strip_prefix("required-features = [") {
            *feats = f
                .trim_end_matches(']')
                .split(',')
                .map(|s| s.trim().trim_matches('"').to_owned())
                .filter(|s| !s.is_empty())
                .collect();
        }
    }
    out.extend(current);
    out
}

fn manifest_has_feature(manifest: &str, feature: &str) -> bool {
    let mut in_features = false;
    for line in manifest.lines().map(str::trim) {
        if line.starts_with('[') {
            in_features = line == "[features]";
        } else if in_features && line.starts_with(&format!("{feature} =")) {
            return true;
        }
    }
    false
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    paths.sort();
    for p in paths {
        if p.is_dir() {
            rust_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

const SLOW_CFG: &str = "#[cfg(feature = \"slow\")]";

/// The slow-gated tests of one source file: the file (`#![cfg(feature = "slow")]`) or each
/// `#[test]` function under `#[cfg(feature = "slow")]`.
fn slow_tests_in(rel: &str, text: &str) -> Vec<String> {
    let file_gate = ["#!", &SLOW_CFG[1..]].concat();
    if text.lines().any(|l| l.trim() == file_gate) {
        return vec![rel.to_owned()];
    }
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    let mut out = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        if *l != SLOW_CFG {
            continue;
        }
        let mut is_test = false;
        for next in &lines[i + 1..] {
            if next.starts_with("#[") || next.starts_with("//") {
                is_test |= *next == "#[test]";
                continue;
            }
            if is_test && let Some(rest) = next.strip_prefix("fn ") {
                let name = rest.split('(').next().unwrap_or(rest);
                out.push(format!("{rel}::{name}"));
            }
            break;
        }
    }
    out
}

#[test]
fn w112_the_slow_tier_is_the_listed_one_and_the_gate_runs_both_tiers() {
    let root = repo();
    let mut found: BTreeSet<String> = BTreeSet::new();
    let allowed = ["slow", "paranoid", "testkit"];
    let ignore_attr = ["#[", "ignore"].concat();
    for (name, dir) in packages() {
        let manifest = read(&dir.join("Cargo.toml"));
        let mut gated = false;
        for (path, feats) in test_targets(&manifest) {
            for f in &feats {
                assert!(
                    allowed.contains(&f.as_str()),
                    "{name} {path}: required feature {f} is never enabled by the gate"
                );
            }
            if feats.iter().any(|f| f == "slow") {
                found.insert(format!("{name} {path}"));
                gated = true;
            }
        }
        let mut files = Vec::new();
        rust_files(&dir.join("tests"), &mut files);
        rust_files(&dir.join("src"), &mut files);
        for f in files {
            let rel = f
                .strip_prefix(&dir)
                .expect("inside the package")
                .to_string_lossy()
                .replace('\\', "/");
            let text = read(&f);
            assert!(
                !gcg_diff::lints::strip_comments_and_strings(&text).contains(&ignore_attr),
                "{name} {rel}: an ignored test (slow tests are feature-gated, PLAN A12)"
            );
            for t in slow_tests_in(&rel, &text) {
                found.insert(format!("{name} {t}"));
                gated = true;
            }
        }
        if gated {
            assert!(
                manifest_has_feature(&manifest, "slow"),
                "{name} gates tests on a slow feature it does not declare"
            );
        }
    }
    let listed: BTreeSet<String> = read(&root.join("scripts/slow-tier.txt"))
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_owned)
        .collect();
    assert_eq!(
        found, listed,
        "the slow-gated tests differ from scripts/slow-tier.txt"
    );
    let gate = read(&root.join("scripts/verify_all_rust.sh"));
    for step in [
        "cargo test --workspace --release --locked --offline\n",
        "cargo test --workspace --release --locked --offline --features \"$SLOW_OFFLINE\"\n",
        "cargo test --workspace --release --locked --offline --features \"$SLOW_TIER2\"\n",
    ] {
        assert!(gate.contains(step), "the gate lacks the step {step:?}");
    }
}

#[test]
fn w112_slow_gating_is_recognised_on_files_and_functions() {
    let file = ["#!", &SLOW_CFG[1..], "\nfn a() {}\n"].concat();
    assert_eq!(slow_tests_in("tests/x.rs", &file), vec!["tests/x.rs"]);
    let funcs = format!(
        "{SLOW_CFG}\nfn helper() {{}}\n\n{SLOW_CFG}\n#[test]\nfn long_one() {{}}\n\n#[test]\nfn fast() {{}}\n"
    );
    assert_eq!(
        slow_tests_in("tests/y.rs", &funcs),
        vec!["tests/y.rs::long_one"]
    );
    let manifest = "[package]\nname = \"p\"\n\n[[test]]\nname = \"e2e\"\npath = \"tests/e2e.rs\"\nrequired-features = [\"slow\"]\n\n[features]\nslow = []\n";
    assert_eq!(
        test_targets(manifest),
        vec![("tests/e2e.rs".to_owned(), vec!["slow".to_owned()])]
    );
    assert!(manifest_has_feature(manifest, "slow"));
    assert!(!manifest_has_feature(manifest, "paranoid"));
}

fn gate_summary(root: &Path) -> (bool, Vec<String>) {
    let out = Command::new("python3")
        .arg(repo().join("scripts/gate_summary.py"))
        .arg("--root")
        .arg(root)
        .output()
        .expect("python3 runs");
    let text = String::from_utf8(out.stdout).expect("UTF-8");
    (
        out.status.success(),
        text.lines().map(str::to_owned).collect(),
    )
}

/// The newest `retrieved_at` of the news listing sources (URL path ending in `/news/`).
fn newest_news_scan(official: &Value) -> String {
    official
        .as_array()
        .expect("official.json is a list")
        .iter()
        .filter(|e| {
            let url = e["url"].as_str().unwrap_or_default();
            let path = url.split('?').next().unwrap_or(url);
            path.ends_with("/news/")
        })
        .filter_map(|e| e["retrieved_at"].as_str())
        .max()
        .expect("a news listing source")
        .to_owned()
}

const RESEARCH: [&str; 4] = [
    "docs/research/DATA_INGESTION.md",
    "docs/research/data_profile.json",
    "docs/research/effect_templates.json",
    "docs/research/work_packages.json",
];

#[test]
fn w115_w153_the_gate_summary_flags_stale_research_and_shows_the_news_scan_date() {
    let root = repo();
    let (ok, lines) = gate_summary(&root);
    assert!(ok, "{lines:?}");
    for current in &RESEARCH {
        assert!(
            lines.iter().any(|l| l
                == &format!(
                    "research artifact {current}: profiles 78a64df, the packaged snapshot"
                )),
            "{current}: {lines:?}"
        );
    }
    assert!(
        !lines
            .iter()
            .any(|l| l.starts_with("WARNING: stale research")),
        "{lines:?}"
    );
    let official: Value = serde_json::from_str(&read(&root.join("data/sources.d/official.json")))
        .expect("official.json");
    let newest = newest_news_scan(&official);
    let news = lines
        .iter()
        .find(|l| l.starts_with("news scan: "))
        .expect("a news-scan line");
    assert!(
        news.starts_with(&format!(
            "news scan: newest retrieved_at of the news sources is {newest} ("
        )),
        "{news}"
    );
}

#[test]
fn w115_w153_negative_controls_follow_the_packaged_commit_and_the_news_dates() {
    let root = repo();
    let tmp = std::env::temp_dir().join(format!("p6n-gate-summary-{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    for rel in RESEARCH
        .iter()
        .chain(&["data/SOURCES.lock.json", "data/sources.d/official.json"])
    {
        let dst = tmp.join(rel);
        fs::create_dir_all(dst.parent().expect("a parent")).expect("mkdir");
        fs::copy(root.join(rel), &dst).expect("copy");
    }
    // The packaged snapshot moves back to 39c85b5: every research artefact is stale.
    let lock = tmp.join("data/SOURCES.lock.json");
    let text = read(&lock).replace(
        "78a64df3519cb6c773b3d2f4a8ecad14d8096b0e",
        "39c85b5032a72b035935b6ca29b326b297908f06",
    );
    fs::write(&lock, text).expect("write the lock");
    // A newer news scan of one listing source.
    let off = tmp.join("data/sources.d/official.json");
    let mut official: Value = serde_json::from_str(&read(&off)).expect("JSON");
    for e in official.as_array_mut().expect("a list") {
        if e["id"] == "official-en-news-rules" {
            e["retrieved_at"] = Value::from("2026-12-01T08:00:00Z");
        }
    }
    fs::write(&off, serde_json::to_string(&official).expect("JSON")).expect("write");
    let (ok, lines) = gate_summary(&tmp);
    assert!(ok, "{lines:?}");
    for stale in &RESEARCH {
        assert!(
            lines.iter().any(|l| l
                == &format!(
                    "WARNING: stale research artifact {stale}: profiles 78a64df, the packaged \
                     snapshot is 39c85b5 (regenerate it on the Python side)"
                )),
            "{stale}: {lines:?}"
        );
    }
    assert!(
        lines.iter().any(|l| l.starts_with(
            "news scan: newest retrieved_at of the news sources is 2026-12-01T08:00:00Z"
        )),
        "{lines:?}"
    );
    // A malformed lock is an error, not a silent pass.
    fs::write(&lock, "{}").expect("write");
    assert!(!gate_summary(&tmp).0);
    fs::remove_dir_all(&tmp).expect("clean up");
}

#[test]
fn w122_the_p6n_gate_scripts_never_name_the_untracked_bytecode() {
    let root = repo();
    for rel in [
        "scripts/verify_all_rust_selftest.sh",
        "scripts/gate_summary.py",
        "scripts/cargo_test_features.py",
        "scripts/check_doc_commands.py",
        "scripts/fuzz_census.py",
        "scripts/test_census.py",
        "scripts/doc_counts.py",
        "scripts/throughput.py",
        "scripts/check_claude_settings.py",
        "scripts/cli_help_snapshot.py",
        "scripts/blind_transcripts.py",
        "scripts/slow-tier.txt",
    ] {
        let findings: Vec<String> = gcg_diff::lints::lint_text(rel, &read(&root.join(rel)))
            .iter()
            .filter(|f| f.rule == 6)
            .map(ToString::to_string)
            .collect();
        assert!(findings.is_empty(), "{findings:?}");
    }
}

#[test]
fn w128_marked_counts_are_regenerated_from_the_data() {
    let root = repo();
    let tmp = std::env::temp_dir().join(format!("p6n-doc-counts-{}", std::process::id()));
    fs::create_dir_all(&tmp).expect("mkdir");
    let run = |text: &str| {
        let doc = tmp.join("doc.md");
        fs::write(&doc, text).expect("write");
        let out = Command::new("python3")
            .arg(root.join("scripts/doc_counts.py"))
            .arg("--check")
            .arg(&doc)
            .output()
            .expect("python3 runs");
        (
            out.status.success(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    };
    let reg = gcg_effects::Registry::embedded();
    let real = reg.db().real_cards().count();
    let good = format!(
        "There are <!-- count:cards.numbers -->{},{:03}<!-- /count --> numbers.\n",
        real / 1000,
        real % 1000
    );
    assert_eq!(run(&good), (true, String::new()));
    let (ok, err) = run("<!-- count:cards.numbers -->1,148<!-- /count -->\n");
    assert!(!ok);
    assert!(
        err.contains("cards.numbers: the document says '1,148'"),
        "{err}"
    );
    let (ok, err) = run("<!-- count:cards.colours -->3<!-- /count -->\n");
    assert!(
        !ok && err.contains("unknown count 'cards.colours'"),
        "{err}"
    );
    // The documents hold no stale count.
    let out = Command::new("python3")
        .arg(root.join("scripts/doc_counts.py"))
        .arg("--check")
        .output()
        .expect("python3 runs");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    fs::remove_dir_all(&tmp).expect("clean up");
}

/// One recorded `cards.ndjson` record per card type, in file order: the format the refresh
/// fetches from gcg-api (the packaged file is byte-identical to the pinned upstream file).
fn recorded_card_records() -> Vec<String> {
    let text = read(&repo().join("src/gcg_sim/data/gcgapi/cards.ndjson"));
    let mut types: Vec<String> = Vec::new();
    let mut out = Vec::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let rec: Value = serde_json::from_str(line).expect("a JSON record");
        let t = rec["card_type"].as_str().expect("a card type").to_owned();
        if !types.contains(&t) {
            types.push(t);
            out.push(line.to_owned());
        }
    }
    out
}

#[test]
fn w121_the_card_loader_accepts_recorded_records_and_names_a_renamed_field() {
    let records = recorded_card_records();
    assert_eq!(records.len(), 8, "one record per card type");
    let fixture = records.join("\n") + "\n";
    let db = gcg_data::CardDb::from_sources(&fixture, "{}").expect("the recorded records load");
    assert_eq!(db.real_cards().count(), 8);
    for line in &records {
        let rec: Value = serde_json::from_str(line).expect("JSON");
        let n = rec["card_number"].as_str().expect("a number");
        let cd = db.get(n).unwrap_or_else(|| panic!("{n} loaded"));
        assert_eq!(
            cd.card_type.as_str(),
            rec["card_type"].as_str().expect("a type")
        );
    }
    for field in ["card_number", "card_type", "product_id"] {
        let renamed = fixture.replacen(&format!("\"{field}\":"), &format!("\"{field}_v2\":"), 1);
        assert_ne!(renamed, fixture);
        let err = gcg_data::CardDb::from_sources(&renamed, "{}")
            .err()
            .unwrap_or_else(|| panic!("renaming {field} must fail the load"))
            .to_string();
        assert!(
            err.contains("cards.ndjson") && err.contains(field),
            "renaming {field}: {err}"
        );
    }
}
