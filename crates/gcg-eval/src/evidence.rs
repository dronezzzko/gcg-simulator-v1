//! The evidence directory and its check (docs/port/PLAN.md §12.10; W-127): every number the
//! port uses as evidence comes from a committed `gcg-eval` command whose raw output is
//! committed in `docs/port/eval/evidence/` with the exact command and the commit it ran at,
//! and the port's evaluation documents cite only commits that resolve.
//!
//! - `gcg-eval evidence add FILE --command TEXT` records FILE (a report written with `--out`
//!   under the evidence directory) in `docs/port/eval/evidence/MANIFEST.json`: the path, the
//!   command, the commit (`git rev-parse HEAD`), whether the tree had uncommitted changes
//!   (`git status --porcelain`), and the file's SHA-256. Re-adding a file replaces its entry.
//! - `gcg-eval evidence check [PATH...]` (default `docs/port/eval`) checks every manifest
//!   entry (the file exists and its SHA-256 matches; its commit resolves) and every `.md` and
//!   `.json` file under the paths: each commit cited as `commit <sha>` (or `commits <sha>`) in
//!   Markdown or as a `"commit"` string in JSON must resolve with `git cat-file -e
//!   <sha>^{commit}`, and each backticked `docs/port/eval/...` path in Markdown must exist.
//!   It exits 1 on any failure.
//!
//! Reports stay plain JSON: the zstd encoder is an opt-in feature of the build (W-99), and a
//! row report with its per-game outcomes is small. Paths are relative to the working
//! directory, the repository root.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use gcg_core::pyjson::{DumpOptions, Json, dumps};
use gcg_data::py_path_str;
use gcg_runner::reports::write_atomic;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::bench_cmd::CommandError;

/// The evidence directory.
pub const EVIDENCE_DIR: &str = "docs/port/eval/evidence";
/// Its manifest.
pub const MANIFEST: &str = "docs/port/eval/evidence/MANIFEST.json";
/// The default check root.
pub const CHECK_ROOT: &str = "docs/port/eval";
/// The manifest format.
pub const MANIFEST_FORMAT: &str = "gcg-eval-evidence-manifest";
pub const MANIFEST_VERSION: u32 = 1;

/// What `evidence` was asked to do.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum EvidenceArgs {
    Add { file: PathBuf, command: String },
    Check { paths: Vec<PathBuf> },
}

fn sha256_hex(bytes: &[u8]) -> String {
    let d = Sha256::digest(bytes);
    d.iter().map(|b| format!("{b:02x}")).collect()
}

fn git(args: &[&str]) -> Result<std::process::Output, String> {
    Command::new("git")
        .args(args)
        .output()
        .map_err(|e| format!("cannot run git {}: {e}", args.join(" ")))
}

/// Whether `sha` names a commit of the repository in the working directory.
///
/// # Errors
/// git cannot run.
pub fn commit_resolves(sha: &str) -> Result<bool, String> {
    if !is_sha(sha) {
        return Ok(false);
    }
    let spec = format!("{sha}^{{commit}}");
    Ok(git(&["cat-file", "-e", &spec])?.status.success())
}

/// 7 to 40 lowercase hex digits.
#[must_use]
pub fn is_sha(s: &str) -> bool {
    (7..=40).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// The commits a Markdown text cites (`commit <sha>`, `commits <sha>`, the sha optionally in
/// backticks), with their 1-based line numbers.
#[must_use]
pub fn markdown_commits(text: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let lower = line.to_ascii_lowercase();
        let mut from = 0;
        while let Some(pos) = lower[from..].find("commit") {
            let start = from + pos;
            let before_ok = start == 0 || !lower.as_bytes()[start - 1].is_ascii_alphanumeric();
            let mut rest = &lower[start + "commit".len()..];
            rest = rest.strip_prefix('s').unwrap_or(rest);
            from = start + "commit".len();
            if !before_ok || !rest.starts_with([' ', '\t']) {
                continue;
            }
            let rest = rest.trim_start().trim_start_matches('`');
            let token: String = rest
                .chars()
                .take_while(char::is_ascii_alphanumeric)
                .collect();
            if is_sha(&token) && token.bytes().any(|b| b.is_ascii_digit()) {
                out.push((i + 1, token));
            }
        }
    }
    out
}

/// The backticked `docs/port/eval/...` paths of a Markdown text, with line numbers.
#[must_use]
pub fn markdown_paths(text: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let mut parts = line.split('`');
        parts.next();
        while let Some(inside) = parts.next() {
            if inside.starts_with("docs/port/eval/")
                && !inside.contains(' ')
                && !inside.contains('*')
            {
                out.push((i + 1, inside.to_owned()));
            }
            parts.next();
        }
    }
    out
}

/// Every string under a `"commit"` key, at any depth.
fn json_commits(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::Object(m) => {
            for (k, x) in m {
                if k == "commit"
                    && let Some(s) = x.as_str()
                {
                    out.push(s.to_owned());
                }
                json_commits(x, out);
            }
        }
        Value::Array(xs) => xs.iter().for_each(|x| json_commits(x, out)),
        _ => {}
    }
}

fn collect_files(p: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    if p.is_dir() {
        let mut entries: Vec<PathBuf> = std::fs::read_dir(p)
            .map_err(|e| format!("cannot list {}: {e}", py_path_str(p)))?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .collect();
        entries.sort();
        for e in entries {
            collect_files(&e, out)?;
        }
    } else if p.extension().is_some_and(|x| x == "md" || x == "json") {
        out.push(p.to_owned());
    } else if !p.exists() {
        return Err(format!("{} does not exist", py_path_str(p)));
    }
    Ok(())
}

fn compact() -> DumpOptions {
    DumpOptions {
        indent: Some(1),
        sort_keys: false,
        compact: false,
        ensure_ascii: true,
    }
}

fn read_manifest() -> Result<Vec<Value>, String> {
    match std::fs::read_to_string(MANIFEST) {
        Ok(t) => {
            let v: Value =
                serde_json::from_str(&t).map_err(|e| format!("{MANIFEST}: not JSON: {e}"))?;
            Ok(v.get("entries")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default())
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(format!("cannot read {MANIFEST}: {e}")),
    }
}

/// `evidence add`.
///
/// # Errors
/// The file is not under the evidence directory or unreadable, or git or the manifest fails.
pub fn add(file: &Path, command: &str) -> Result<Json, String> {
    let shown = py_path_str(file);
    if !shown.starts_with(&format!("{EVIDENCE_DIR}/")) {
        return Err(format!("{shown} is not under {EVIDENCE_DIR}/"));
    }
    let bytes = std::fs::read(file).map_err(|e| format!("cannot read {shown}: {e}"))?;
    let head = git(&["rev-parse", "HEAD"])?;
    if !head.status.success() {
        return Err("git rev-parse HEAD failed".to_owned());
    }
    let commit = String::from_utf8_lossy(&head.stdout).trim().to_owned();
    let status = git(&["status", "--porcelain"])?;
    let dirty = !status.stdout.is_empty();
    let mut entries = read_manifest()?;
    entries.retain(|e| e.get("file").and_then(Value::as_str) != Some(shown.as_str()));
    let mut entry = Json::object();
    entry.set("file", shown.as_str());
    entry.set("command", command);
    entry.set("commit", commit.as_str());
    entry.set("dirty", dirty);
    entry.set("sha256", sha256_hex(&bytes));
    let mut all: Vec<Json> = entries
        .iter()
        .map(gcg_runner::reports::json_from_value)
        .collect();
    all.push(entry.clone());
    let mut doc = Json::object();
    doc.set("format", MANIFEST_FORMAT);
    doc.set("version", MANIFEST_VERSION);
    doc.set("entries", Json::Array(all));
    write_atomic(Path::new(MANIFEST), &(dumps(&doc, compact()) + "\n"))
        .map_err(|e| format!("cannot write {MANIFEST}: {e}"))?;
    Ok(entry)
}

/// `evidence check`: the findings document and whether everything passed.
///
/// # Errors
/// A path that does not exist, or git cannot run.
pub fn check(paths: &[PathBuf]) -> Result<(Json, bool), String> {
    let roots: Vec<PathBuf> = if paths.is_empty() {
        vec![PathBuf::from(CHECK_ROOT)]
    } else {
        paths.to_vec()
    };
    let mut files = Vec::new();
    for r in &roots {
        collect_files(r, &mut files)?;
    }
    let mut ok = true;
    let mut commits = Vec::new();
    let mut cited = Vec::new();
    let mut add_commit = |file: &str, line: Option<usize>, sha: &str| -> Result<(), String> {
        let good = commit_resolves(sha)?;
        let mut o = Json::object();
        o.set("file", file);
        o.set("line", line);
        o.set("commit", sha);
        o.set("resolves", good);
        commits.push(o);
        Ok(())
    };
    for f in &files {
        let shown = py_path_str(f);
        let text = std::fs::read_to_string(f).map_err(|e| format!("cannot read {shown}: {e}"))?;
        if shown.ends_with(".md") {
            for (line, sha) in markdown_commits(&text) {
                add_commit(&shown, Some(line), &sha)?;
            }
            for (line, p) in markdown_paths(&text) {
                let exists = Path::new(&p).exists();
                let mut o = Json::object();
                o.set("file", shown.as_str());
                o.set("line", line);
                o.set("path", p.as_str());
                o.set("exists", exists);
                cited.push(o);
            }
        } else if let Ok(v) = serde_json::from_str::<Value>(&text) {
            let mut found = Vec::new();
            json_commits(&v, &mut found);
            for sha in found {
                add_commit(&shown, None, &sha)?;
            }
        }
    }
    let mut manifest = Vec::new();
    for e in read_manifest()? {
        let file = e
            .get("file")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let want = e.get("sha256").and_then(Value::as_str).unwrap_or_default();
        let got = std::fs::read(&file).ok().map(|b| sha256_hex(&b));
        let commit = e.get("commit").and_then(Value::as_str).unwrap_or_default();
        let mut o = Json::object();
        o.set("file", file.as_str());
        o.set("exists", got.is_some());
        o.set("sha256_matches", got.as_deref() == Some(want));
        o.set("commit_resolves", commit_resolves(commit)?);
        o.set(
            "has_command",
            e.get("command")
                .and_then(Value::as_str)
                .is_some_and(|c| !c.is_empty()),
        );
        manifest.push(o);
    }
    let all_true = |items: &[Json], keys: &[&str]| {
        items
            .iter()
            .all(|o| keys.iter().all(|k| o.get(k) == Some(&Json::Bool(true))))
    };
    ok &= all_true(&commits, &["resolves"]);
    ok &= all_true(&cited, &["exists"]);
    ok &= all_true(
        &manifest,
        &["exists", "sha256_matches", "commit_resolves", "has_command"],
    );
    let mut doc = Json::object();
    doc.set("format", "gcg-eval-evidence-check");
    doc.set("version", MANIFEST_VERSION);
    doc.set("files", files.len());
    doc.set("commits", Json::Array(commits));
    doc.set("paths", Json::Array(cited));
    doc.set("manifest", Json::Array(manifest));
    doc.set("pass", ok);
    Ok((doc, ok))
}

/// Runs `evidence`.
///
/// # Errors
/// As [`add`] and [`check`]; a failed check.
pub fn run(args: &EvidenceArgs, out: &mut dyn Write) -> Result<(), CommandError> {
    match args {
        EvidenceArgs::Add { file, command } => {
            let entry = add(file, command).map_err(CommandError::Invalid)?;
            let _ = writeln!(out, "{}", dumps(&entry, compact()));
            Ok(())
        }
        EvidenceArgs::Check { paths } => {
            let (doc, ok) = check(paths).map_err(CommandError::Invalid)?;
            let _ = writeln!(out, "{}", dumps(&doc, compact()));
            if ok {
                Ok(())
            } else {
                Err(CommandError::Invalid(
                    "evidence check failed: a cited commit does not resolve, a cited path is \
                     missing, or a manifest entry does not match"
                        .to_owned(),
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commits_and_paths_are_found_in_markdown() {
        let text = "Measured at commit 3fbb8aa and commits `6daf17f`.\n\
                    Not a commit: commitment deadbeef1; `commit` alone.\n\
                    A seed 90000001 and a digest `0123456789abcdef0123` are not commits.\n\
                    See `docs/port/eval/evidence/x.json` and `docs/port/eval/*.md`.\n";
        assert_eq!(
            markdown_commits(text),
            [(1, "3fbb8aa".to_owned()), (1, "6daf17f".to_owned())]
        );
        assert_eq!(
            markdown_paths(text),
            [(4, "docs/port/eval/evidence/x.json".to_owned())]
        );
        assert!(is_sha("3fbb8aa") && !is_sha("3fbb8a") && !is_sha("3FBB8AA") && !is_sha("xyzxyzx"));
    }
}
