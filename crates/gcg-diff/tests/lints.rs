//! `gcg-diff lints` (PLAN §9.9): the repository is clean, and the lint self-test fixture
//! (`tests/lint_fixtures/`) is flagged exactly on its banned lines and nowhere else.

use std::path::Path;

use gcg_diff::lints::{lint_fixture, lint_repo};

fn root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

#[test]
fn the_repository_passes_every_lint() {
    let findings = lint_repo(root()).expect("scan");
    let text: Vec<String> = findings.iter().map(ToString::to_string).collect();
    assert!(findings.is_empty(), "{}", text.join("\n"));
}

#[test]
fn the_fixture_is_flagged_exactly_on_its_banned_lines() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/lint_fixtures");
    let expected = std::fs::read_to_string(dir.join("expected.txt")).expect("expected.txt");
    let mut want: Vec<(String, usize, u8)> = expected
        .lines()
        .map(|l| {
            let parts: Vec<&str> = l.split(' ').collect();
            (
                parts[0].to_owned(),
                parts[1].parse().expect("line"),
                parts[2].parse().expect("rule"),
            )
        })
        .collect();
    want.sort();
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .expect("fixtures")
        .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("fixture_"))
        .collect();
    names.sort();
    assert_eq!(names.len(), 5, "{names:?}");
    let mut got: Vec<(String, usize, u8)> = Vec::new();
    for name in &names {
        let text = std::fs::read_to_string(dir.join(name)).expect("fixture");
        for f in lint_fixture(&text).expect("lint-path line") {
            got.push((name.clone(), f.line, f.rule));
        }
    }
    got.sort();
    got.dedup();
    assert_eq!(got, want);
}
