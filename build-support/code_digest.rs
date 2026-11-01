// The build-time `code_digest` of the two shipped binaries (docs/port/PLAN.md §2.4, X19,
// W-103). The build scripts of gcg-cli and gcg-eval include this file inside a module:
//
//     mod code_digest {
//         include!("../../build-support/code_digest.rs");
//     }
//
// and call `code_digest::emit(&repo_root)`. It lives outside the `crates/*` member list, so
// it is not a workspace member. Tests include it the same way.
//
// The algorithm is Python's `code_digest` (`src/gcg_sim/reports/versions.py:43-52`): one
// SHA-256 over every file, in sorted path order, fed `path`, then `b"\0"`, then the file's
// bytes, printed as lowercase hex. Python hashes every file of the installed package, so a
// stray file changes it (W-103). Here the file set is the patterns of
// `build-support/digest-manifest.txt`, so only the sources and embedded data count. Paths
// are relative to the repository root, with `/` separators.

use std::fs;
use std::io;
use std::path::Path;

use sha2::{Digest, Sha256};

/// The manifest, relative to the repository root.
pub const MANIFEST: &str = "build-support/digest-manifest.txt";

/// The files a manifest selects and the directories that were listed to expand its
/// wildcards (the build script watches both, so that a new matching file reruns it).
#[derive(Debug, Default)]
pub struct Selection {
    /// Relative paths, sorted by their UTF-8 bytes (Python's `sorted` over `str` paths sorts
    /// by code point, which is the same order), without duplicates.
    pub files: Vec<String>,
    /// Relative paths of the listed directories, sorted, without duplicates.
    pub dirs: Vec<String>,
}

/// The patterns of a manifest text: one per line; blank lines and lines starting with `#`
/// are skipped; surrounding whitespace is trimmed.
pub fn patterns(manifest: &str) -> Vec<&str> {
    manifest
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect()
}

/// `fnmatch`-style matching of one path segment against a pattern segment in which `*`
/// matches any run of characters (other characters match themselves).
pub fn segment_matches(pattern: &str, name: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let s: Vec<char> = name.chars().collect();
    // Iterative wildcard matching with backtracking to the last `*`.
    let (mut pi, mut si) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while si < s.len() {
        if pi < p.len() && p[pi] == '*' {
            star = Some((pi, si));
            pi += 1;
        } else if pi < p.len() && p[pi] == s[si] {
            pi += 1;
            si += 1;
        } else if let Some((sp, ss)) = star {
            pi = sp + 1;
            si = ss + 1;
            star = Some((sp, ss + 1));
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

fn join(rel: &str, name: &str) -> String {
    if rel.is_empty() {
        name.to_owned()
    } else {
        format!("{rel}/{name}")
    }
}

/// The entries of a directory as `(name, is_dir)`, sorted by name. Names that are not UTF-8
/// are skipped (no source or data file has one).
fn entries(dir: &Path) -> io::Result<Vec<(String, bool)>> {
    let mut out = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        if let Ok(name) = entry.file_name().into_string() {
            let is_dir = entry.path().is_dir();
            out.push((name, is_dir));
        }
    }
    out.sort();
    Ok(out)
}

fn expand(
    root: &Path,
    rel: &str,
    segments: &[&str],
    selection: &mut Selection,
) -> io::Result<()> {
    let Some((segment, rest)) = segments.split_first() else {
        return Ok(());
    };
    let dir = root.join(rel);
    if *segment == "**" {
        expand(root, rel, rest, selection)?;
        selection.dirs.push(rel.to_owned());
        for (name, is_dir) in entries(&dir)? {
            if is_dir {
                expand(root, &join(rel, &name), segments, selection)?;
            }
        }
    } else if segment.contains('*') {
        selection.dirs.push(rel.to_owned());
        for (name, is_dir) in entries(&dir)? {
            if !segment_matches(segment, &name) {
                continue;
            }
            if rest.is_empty() {
                if !is_dir {
                    selection.files.push(join(rel, &name));
                }
            } else if is_dir {
                expand(root, &join(rel, &name), rest, selection)?;
            }
        }
    } else {
        let path = dir.join(segment);
        if rest.is_empty() {
            if path.is_file() {
                selection.files.push(join(rel, segment));
            }
        } else if path.is_dir() {
            expand(root, &join(rel, segment), rest, selection)?;
        }
    }
    Ok(())
}

/// The files the manifest text selects under `root`.
///
/// # Errors
/// A line without `*` that names no existing file (a stale manifest), or an I/O error while
/// listing a directory.
pub fn select(root: &Path, manifest: &str) -> Result<Selection, String> {
    let mut selection = Selection::default();
    for pattern in patterns(manifest) {
        let segments: Vec<&str> = pattern.split('/').collect();
        let before = selection.files.len();
        expand(root, "", &segments, &mut selection)
            .map_err(|e| format!("{MANIFEST}: cannot expand {pattern}: {e}"))?;
        if !pattern.contains('*') && selection.files.len() == before {
            return Err(format!("{MANIFEST}: {pattern} does not exist"));
        }
    }
    selection.files.sort();
    selection.files.dedup();
    selection.dirs.sort();
    selection.dirs.dedup();
    Ok(selection)
}

/// Python's algorithm over `files` (relative paths under `root`, already sorted).
///
/// # Errors
/// A file that cannot be read.
pub fn digest(root: &Path, files: &[String]) -> Result<String, String> {
    let mut h = Sha256::new();
    for rel in files {
        let bytes = fs::read(root.join(rel)).map_err(|e| format!("cannot read {rel}: {e}"))?;
        h.update(rel.as_bytes());
        h.update(b"\0");
        h.update(&bytes);
    }
    Ok(h.finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>())
}

/// The digest of the repository at `root` under its own manifest, with the selection.
///
/// # Errors
/// As [`select`] and [`digest`], or an unreadable manifest.
pub fn compute(root: &Path) -> Result<(String, Selection), String> {
    let manifest = fs::read_to_string(root.join(MANIFEST))
        .map_err(|e| format!("cannot read {MANIFEST}: {e}"))?;
    let selection = select(root, &manifest)?;
    let hex = digest(root, &selection.files)?;
    Ok((hex, selection))
}

/// For build scripts: sets `GCG_CODE_DIGEST` for the crate's compilation and asks cargo to
/// rerun the script when a hashed file or a listed directory changes.
///
/// # Panics
/// When [`compute`] fails: a stale manifest must fail the build, never give a wrong digest.
pub fn emit(root: &Path) {
    let (hex, selection) = match compute(root) {
        Ok(done) => done,
        Err(e) => panic!("code_digest: {e}"),
    };
    println!("cargo:rerun-if-changed={}", root.join(MANIFEST).display());
    for dir in &selection.dirs {
        println!("cargo:rerun-if-changed={}", root.join(dir).display());
    }
    for file in &selection.files {
        println!("cargo:rerun-if-changed={}", root.join(file).display());
    }
    println!("cargo:rustc-env=GCG_CODE_DIGEST={hex}");
}
