//! PLAN §3.4 test 1 (T-040): the committed IR is current with its inputs. A data refresh or a
//! binding edit without `oracle/export_ir.py --write` fails here, without Python.

use std::fs;
use std::path::Path;

use sha2::{Digest, Sha256};

use super::{EMBEDDED_IR, raw, repo_root, tables};
use crate::dsl::{CLASSES, ENUMS};

fn sha256_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    Sha256::digest(bytes)
        .iter()
        .fold(String::new(), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}

fn inputs() -> Vec<(&'static str, &'static str)> {
    raw()["inputs"]
        .as_object()
        .expect("inputs is an object")
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str().expect("a hash")))
        .collect()
}

#[test]
fn every_input_hash_equals_the_file_in_the_repository() {
    let root = repo_root();
    let mut stale = Vec::new();
    for (path, want) in inputs() {
        match fs::read(root.join(path)) {
            Ok(bytes) if sha256_hex(&bytes) == want => {}
            Ok(_) => stale.push(format!("{path} changed")),
            Err(e) => stale.push(format!("{path}: {e}")),
        }
    }
    assert!(
        stale.is_empty(),
        "the effect IR is stale; run `uv run python oracle/export_ir.py --write`: {stale:?}"
    );
}

/// The file set of `reports.versions.code_digest()` (`reports/versions.py:32-52`): every file
/// under `src/gcg_sim`, skipping `__pycache__` and `*.pyc`.
fn package_files(dir: &Path, rel: &str, out: &mut Vec<String>) {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .expect("readable package directory")
        .map(|e| e.expect("directory entry"))
        .collect();
    entries.sort_by_key(fs::DirEntry::file_name);
    for e in entries {
        let name = e.file_name().into_string().expect("UTF-8 file name");
        if name == "__pycache__" || name.ends_with(".pyc") {
            continue;
        }
        let path = format!("{rel}/{name}");
        if e.file_type().expect("file type").is_dir() {
            package_files(&e.path(), &path, out);
        } else {
            out.push(path);
        }
    }
}

#[test]
fn inputs_cover_every_file_of_the_python_package() {
    let mut files = Vec::new();
    package_files(&repo_root().join("src/gcg_sim"), "src/gcg_sim", &mut files);
    files.sort();
    let listed: Vec<String> = inputs()
        .into_iter()
        .filter(|(p, _)| p.starts_with("src/gcg_sim/"))
        .map(|(p, _)| p.to_owned())
        .collect();
    assert_eq!(
        files, listed,
        "files were added to or removed from src/gcg_sim; run `uv run python \
         oracle/export_ir.py --write`"
    );
    for port in [
        "oracle/export_ir.py",
        "oracle/gen_dsl.py",
        "oracle/gen_engine_enums.py",
    ] {
        assert!(
            inputs().iter().any(|(p, _)| *p == port),
            "{port} not in inputs"
        );
    }
}

/// The packaged files that `gcg-data` embeds with `include_str!`: the `data_file!` paths of
/// `crates/gcg-data/src/embedded.rs`, relative to `src/gcg_sim/data/`.
fn gcg_data_embedded_paths() -> Vec<String> {
    let src = fs::read_to_string(repo_root().join("crates/gcg-data/src/embedded.rs"))
        .expect("gcg-data embedded.rs");
    src.split("data_file!(\"")
        .skip(1)
        .map(|rest| rest.split('"').next().expect("a closing quote").to_owned())
        .collect()
}

/// PLAN §3.4 test 1, last part: the data bytes `gcg-data` embeds are the bytes the IR was
/// exported from. Every file `gcg-data` embeds is an IR input whose hash equals the file
/// (`include_str!` reads that same file at build time), and the card database `gcg-data`
/// built from its embedded bytes equals the one built from the files the IR hashed.
#[test]
fn the_embedded_data_bytes_equal_gcg_datas() {
    let paths = gcg_data_embedded_paths();
    assert_eq!(paths.len(), 8, "{paths:?}");
    let root = repo_root();
    let listed = inputs();
    for path in &paths {
        let repo_path = format!("src/gcg_sim/data/{path}");
        let want = listed
            .iter()
            .find(|(p, _)| *p == repo_path)
            .map(|(_, h)| *h)
            .unwrap_or_else(|| panic!("{repo_path} is embedded by gcg-data but not an IR input"));
        let bytes = fs::read(root.join(&repo_path)).expect("embedded data file");
        assert_eq!(sha256_hex(&bytes), want, "{repo_path}");
    }
    let read = |p: &str| {
        fs::read_to_string(root.join("src/gcg_sim/data").join(p)).expect("packaged data file")
    };
    let from_files =
        gcg_data::CardDb::from_sources(&read("gcgapi/cards.ndjson"), &read("overrides.json"))
            .expect("the packaged data loads");
    assert_eq!(
        from_files.fingerprints(),
        gcg_data::CardDb::embedded().fingerprints()
    );
}

#[test]
fn the_data_lock_matches() {
    let lock = fs::read(repo_root().join("data/SOURCES.lock.json")).expect("data lock");
    assert_eq!(
        raw()["data_lock_sha256"].as_str(),
        Some(sha256_hex(&lock).as_str())
    );
}

#[test]
fn the_sha256_file_names_the_embedded_ir() {
    let line = fs::read_to_string(repo_root().join("crates/gcg-effects/ir/registry.json.sha256"))
        .expect("registry.json.sha256");
    assert_eq!(
        line,
        format!("{}  registry.json\n", sha256_hex(EMBEDDED_IR.as_bytes()))
    );
}

#[test]
fn the_ir_is_version_1_from_the_golden_platform() {
    let ir = raw();
    assert_eq!(ir["ir_version"], 1);
    let generator = &ir["generator"];
    assert_eq!(generator["exporter"], "oracle/export_ir.py");
    assert_eq!(generator["python"], "3.15.1");
    assert_eq!(generator["hash_seeds_checked"], serde_json::json!([1, 2]));
    assert_eq!(
        generator["git"].as_str().map(str::len),
        Some(40),
        "the git tree id of src/gcg_sim"
    );
}

/// T-040: the IR's schema and enum catalogue match the generated types (the loader refuses
/// any difference; this states the sizes explicitly).
#[test]
fn schema_and_enums_match_the_generated_types() {
    let _ = tables();
    let ir = raw();
    let classes: Vec<&String> = ir["schema"].as_object().expect("schema").keys().collect();
    assert_eq!(classes.len(), CLASSES.len());
    assert_eq!(classes.len(), 147);
    assert_eq!(ir["enums"].as_object().expect("enums").len(), ENUMS.len());
    assert_eq!(ENUMS.len(), 14);
}

/// PLAN §3.4 test 6, IR side: the exported CardDb fingerprints are the recorded values
/// for this data snapshot (the comparison with
/// `gcg-data`'s own fingerprints is `db_fingerprints.rs`).
#[test]
fn the_ir_carries_the_recorded_card_db_fingerprints() {
    let fp = &raw()["db_fingerprints"];
    for (name, want) in [
        (
            "carddefs",
            "c38ea9017c2109ff622745f53eaed210c526369fd8f2aeaa7b8ae17c76baa7de",
        ),
        (
            "def_ids",
            "c4bb4d9269df02c745a753059cfb0f5993290600fc3202d8e54eadd1fc8cd5df",
        ),
        (
            "token_keys",
            "176b958ac9db6a04ae9c71285629fd390bf75f06e3c8fa90d065167958130105",
        ),
        (
            "applied_overrides",
            "613cd0b24183fe239d6806a4442f87d8ae1c0b72e8efa7cc8513123df5d8273b",
        ),
    ] {
        assert_eq!(fp[name].as_str(), Some(want), "db_fingerprints.{name}");
    }
}
