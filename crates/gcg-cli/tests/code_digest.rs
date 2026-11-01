//! The build-time code digest (PLAN §2.4, X19; W-103 verification): Python's algorithm
//! (`reports/versions.py:43-52`) over the manifest's files only, so an unrelated file in the
//! source tree leaves it unchanged and an edited source file changes it; and the digest the
//! build embedded is the one of the files as they are.

use std::fs;
use std::path::{Path, PathBuf};

#[allow(dead_code, reason = "emit is for build scripts")]
mod code_digest {
    include!("../../../build-support/code_digest.rs");
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

/// A fresh directory under cargo's per-target temporary directory.
fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("code-digest-{name}"));
    if dir.exists() {
        fs::remove_dir_all(&dir).expect("remove the old scratch tree");
    }
    fs::create_dir_all(&dir).expect("create the scratch tree");
    dir
}

fn write(root: &Path, rel: &str, text: &str) {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    fs::write(path, text).expect("write");
}

const MANIFEST: &str = "# comment\n\nCargo.toml\ncrates/*/src/**/*.rs\ndata/*.json\n";

fn tree(name: &str) -> PathBuf {
    let root = scratch(name);
    write(&root, code_digest::MANIFEST, MANIFEST);
    write(&root, "Cargo.toml", "[workspace]\n");
    write(&root, "crates/a/src/lib.rs", "pub fn a() {}\n");
    write(&root, "crates/a/src/deep/er/m.rs", "pub fn m() {}\n");
    write(&root, "crates/b/src/main.rs", "fn main() {}\n");
    write(&root, "data/x.json", "{}\n");
    root
}

#[test]
fn the_algorithm_is_pythons() {
    let root = tree("python");
    let (hex, selection) = code_digest::compute(&root).expect("digest");
    assert_eq!(
        selection.files,
        [
            "Cargo.toml",
            "crates/a/src/deep/er/m.rs",
            "crates/a/src/lib.rs",
            "crates/b/src/main.rs",
            "data/x.json"
        ]
    );
    // hashlib.sha256(b"".join(p.encode() + b"\0" + data for p, data in sorted(files)))
    // computed with Python 3.15.1 over the same five files.
    assert_eq!(
        hex,
        "1d9828a37876a9615827aee7ee97a63d431f9feb4bc70e794cd3fbef39553471"
    );
}

#[test]
fn unrelated_files_do_not_change_it_and_sources_do() {
    let root = tree("changes");
    let (base, _) = code_digest::compute(&root).expect("digest");
    write(&root, "crates/a/src/notes.txt", "scratch notes\n");
    write(&root, "crates/a/tests/t.rs", "#[test] fn t() {}\n");
    write(&root, "README.md", "readme\n");
    assert_eq!(code_digest::compute(&root).expect("digest").0, base);
    write(&root, "crates/a/src/lib.rs", "pub fn a() { }\n");
    let edited = code_digest::compute(&root).expect("digest").0;
    assert_ne!(edited, base);
    write(&root, "crates/c/src/new.rs", "\n");
    assert_ne!(code_digest::compute(&root).expect("digest").0, edited);
}

#[test]
fn a_missing_literal_entry_fails() {
    let root = tree("missing");
    fs::remove_file(root.join("Cargo.toml")).expect("remove");
    let err = code_digest::compute(&root).expect_err("stale manifest");
    assert!(err.contains("Cargo.toml does not exist"), "{err}");
}

#[test]
fn wildcards() {
    assert!(code_digest::segment_matches("*.rs", "lib.rs"));
    assert!(code_digest::segment_matches("*", "anything"));
    assert!(code_digest::segment_matches("a*b*c", "aXbYc"));
    assert!(!code_digest::segment_matches("*.rs", "lib.rsx"));
    assert!(!code_digest::segment_matches("a*b", "acb_"));
}

#[test]
fn the_embedded_digest_is_the_repositorys() {
    let (hex, selection) = code_digest::compute(&repo_root()).expect("digest of the repository");
    assert_eq!(gcg_cli::CODE_DIGEST, hex);
    assert_eq!(gcg_cli::PACKAGE_VERSION, env!("CARGO_PKG_VERSION"));
    for must in [
        "Cargo.lock",
        "crates/gcg-runner/src/argparse/mod.rs",
        "crates/gcg-core/src/pool.rs",
        "src/gcg_sim/data/gcgapi/cards.ndjson",
        "src/gcg_sim/data/official/banlist.json",
    ] {
        assert!(selection.files.iter().any(|f| f == must), "{must}");
    }
    // A crate's test tree (`crates/<crate>/tests/`) never counts; unit-test modules under
    // `src/` (for example `crates/gcg-effects/src/tests/`) are crate sources and do.
    let in_test_tree = |f: &str| f.starts_with("crates/") && f.split('/').nth(2) == Some("tests");
    assert!(selection.files.iter().all(|f| !in_test_tree(f)));
    for test_file in [
        "crates/gcg-cli/tests/code_digest.rs",
        "crates/gcg-runner/tests/reports_parity.rs",
    ] {
        assert!(repo_root().join(test_file).is_file(), "{test_file}");
        assert!(
            selection.files.iter().all(|f| f != test_file),
            "{test_file}"
        );
    }
}
