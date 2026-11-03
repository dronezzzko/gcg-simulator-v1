//! `gcg-diff api-snapshot` (docs/port/WEAKNESSES.md W-129): the committed listing of the public
//! API equals the sources, it covers exactly the library crates, and a changed signature in a
//! copy of the sources fails the check at the changed line (negative control).

use std::fs;
use std::path::{Path, PathBuf};

use gcg_diff::apisnapshot::{SNAPSHOT_FILE, api_crates, check, snapshot};

fn root() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

#[test]
fn the_committed_api_snapshot_matches_the_sources() {
    let problems = check(&root()).expect("listing");
    assert!(
        problems.is_empty(),
        "{}\n(regenerate: target/release/gcg-diff api-snapshot --write)",
        problems.join("\n")
    );
}

#[test]
fn the_listing_covers_exactly_the_library_crates() {
    let crates = api_crates(&root()).expect("manifests");
    assert_eq!(
        crates,
        [
            "gcg-core",
            "gcg-data",
            "gcg-effects",
            "gcg-engine",
            "gcg-ai",
            "gcg-ai-reference",
            "gcg-search",
            "gcg-runner",
        ]
    );
    let text = fs::read_to_string(root().join(SNAPSHOT_FILE)).expect("snapshot");
    let listed: Vec<&str> = text
        .lines()
        .filter_map(|l| l.strip_prefix("# gcg-"))
        .collect();
    assert_eq!(listed.len(), crates.len(), "{listed:?}");
}

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("mkdir");
    for e in fs::read_dir(from).expect("read_dir") {
        let e = e.expect("entry");
        let (src, dst) = (e.path(), to.join(e.file_name()));
        if src.is_dir() {
            copy_tree(&src, &dst);
        } else {
            fs::copy(&src, &dst).expect("copy");
        }
    }
}

#[test]
fn a_changed_public_signature_fails_the_check_at_its_line() {
    let repo = root();
    let tmp = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("api_snapshot_control");
    if tmp.exists() {
        fs::remove_dir_all(&tmp).expect("clean");
    }
    fs::create_dir_all(tmp.join("docs/port")).expect("mkdir");
    fs::copy(repo.join("Cargo.toml"), tmp.join("Cargo.toml")).expect("workspace manifest");
    fs::copy(repo.join(SNAPSHOT_FILE), tmp.join(SNAPSHOT_FILE)).expect("snapshot");
    for e in fs::read_dir(repo.join("crates")).expect("crates") {
        let dir = e.expect("entry").path();
        let to = tmp.join("crates").join(dir.file_name().expect("name"));
        fs::create_dir_all(&to).expect("mkdir");
        fs::copy(dir.join("Cargo.toml"), to.join("Cargo.toml")).expect("manifest");
        copy_tree(&dir.join("src"), &to.join("src"));
    }
    assert!(
        check(&tmp).expect("listing").is_empty(),
        "the copy lists as the repository"
    );

    let pool = tmp.join("crates/gcg-core/src/pool.rs");
    let text = fs::read_to_string(&pool).expect("pool.rs");
    let changed = text.replacen(
        "pub const fn threads(&self) -> NonZeroUsize",
        "pub const fn threads(&self) -> usize",
        1,
    );
    assert_ne!(changed, text, "the control's signature is in pool.rs");
    fs::write(&pool, changed).expect("write");
    let problems = check(&tmp).expect("listing");
    assert_eq!(problems.len(), 2, "{problems:?}");
    assert!(
        problems[0].contains("pub const fn threads(&self) -> usize"),
        "{problems:?}"
    );
    assert!(problems[1].starts_with("1 committed line(s) not in the sources, 1 source line(s)"));

    let private_and_body = text
        .replacen(
            "fn stop_claiming(&self) -> bool {",
            "fn stop_claiming(&self, _unused: u8) -> bool {",
            1,
        )
        .replacen(
            "pub const fn threads(&self) -> NonZeroUsize {",
            "pub const fn threads(&self) -> NonZeroUsize {\n        // a body edit\n",
            1,
        );
    assert_eq!(private_and_body.matches("_unused").count(), 1);
    assert!(private_and_body.contains("// a body edit"));
    fs::write(&pool, private_and_body).expect("write");
    assert!(
        check(&tmp).expect("listing").is_empty(),
        "a private signature and a body edit leave the listing unchanged"
    );
    assert_eq!(
        snapshot(&tmp).expect("listing"),
        fs::read_to_string(repo.join(SNAPSHOT_FILE)).expect("snapshot")
    );
    fs::remove_dir_all(&tmp).expect("clean");
}
