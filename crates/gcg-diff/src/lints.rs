//! `gcg-diff lints` (PLAN §9.9): source rules over `crates/**` that clippy cannot express.
//!
//! 1. no placeholders: the markers of `tests/test_no_placeholders.py`, the `todo` and
//!    `unimplemented` macros, `#[ignore]`, and `#[should_panic]` without `expected` (which
//!    passes on any panic and so hides failures);
//! 2. no timing: outside `crates/gcg-runner/src/clock.rs`, no path into `std::time` or
//!    `core::time` (`use` lists included), no `Instant` or `SystemTime`, no `thread::sleep`, no
//!    `.elapsed(` call and no timeout argument (`recv_timeout`, `wait_timeout`, `park_timeout`).
//!    The engine enum `Duration` and locals named `elapsed` are fine: the rule matches paths,
//!    method calls and those two type names only;
//! 3. no randomness or benchmarks: `rand`, `getrandom`, `[[bench]]`, `#[bench]`, `criterion`, and
//!    `sum::<f64>` (use `gcg_core::pymath::py_sum`);
//! 4. `unsafe` only in `crates/gcg-core/src/libm.rs`, and there only after a `// SAFETY:`
//!    comment;
//! 5. in `crates/gcg-search/src/` only `agent.rs` implements `Agent`, and its `choose` and
//!    `choose_traced` use their true-state parameter only as an argument of `begin`, `finish`
//!    or `Observation::new` (W-135, §12.3);
//! 6. no reference to the three untracked bytecode directories of W-122 in the crates, the
//!    Cargo manifests, `.cargo/config.toml` or the Rust gate scripts. A bare `__pycache__` is
//!    allowed: the reference's `code_digest` walk skips every directory of that name.
//!
//! Rules 2-5 read the code with comments and string literals removed; rules 1 and 6 read the
//! raw text. The banned words are assembled from pieces, so this file passes its own lint.

use std::path::{Path, PathBuf};

/// One finding: repository-relative path, 1-based line, rule number, message.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Finding {
    pub path: String,
    pub line: usize,
    pub rule: u8,
    pub message: String,
}

impl std::fmt::Display for Finding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}: rule {}: {}",
            self.path, self.line, self.rule, self.message
        )
    }
}

/// The lint fixture directory, excluded from the repository scan.
pub const FIXTURE_DIR: &str = "crates/gcg-diff/tests/lint_fixtures/";
const CLOCK_FILE: &str = "crates/gcg-runner/src/clock.rs";
const UNSAFE_FILE: &str = "crates/gcg-core/src/libm.rs";
const SEARCH_SRC: &str = "crates/gcg-search/src/";
const SEARCH_AGENT: &str = "crates/gcg-search/src/agent.rs";
const GATE_SCRIPTS: [&str; 4] = [
    "scripts/verify_all_rust.sh",
    "scripts/verify_binary_offline.sh",
    "scripts/ci_local.sh",
    "scripts/compare_python_vs_rust.sh",
];

fn cat(parts: &[&str]) -> String {
    parts.concat()
}

fn is_word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Byte offsets of `word` in `text` where it is not part of a longer identifier.
fn word_at(text: &str, word: &str) -> Vec<usize> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(i) = text[from..].find(word) {
        let at = from + i;
        let before = text[..at].chars().next_back();
        let after = text[at + word.len()..].chars().next();
        let first_word = word.chars().next().is_some_and(is_word);
        let last_word = word.chars().next_back().is_some_and(is_word);
        if !(first_word && before.is_some_and(is_word))
            && !(last_word && after.is_some_and(is_word))
        {
            out.push(at);
        }
        from = at + word.len().max(1);
    }
    out
}

fn line_of(text: &str, offset: usize) -> usize {
    text[..offset].bytes().filter(|&b| b == b'\n').count() + 1
}

/// `text` with comments, string literals and char literals replaced by spaces (newlines are
/// kept, so offsets map to the same lines).
#[must_use]
pub fn strip_comments_and_strings(text: &str) -> String {
    blank_spans(text, true)
}

/// `text` with only its comments replaced by spaces; string and char literals stay, at the same
/// offsets as in [`strip_comments_and_strings`].
#[must_use]
pub fn strip_comments(text: &str) -> String {
    blank_spans(text, false)
}

fn blank_spans(text: &str, literals: bool) -> String {
    let b = text.as_bytes();
    let mut out: Vec<u8> = b.to_vec();
    let blank = |out: &mut Vec<u8>, from: usize, to: usize| {
        for x in &mut out[from..to] {
            if *x != b'\n' {
                *x = b' ';
            }
        }
    };
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'/' if b.get(i + 1) == Some(&b'/') => {
                let end = b[i..]
                    .iter()
                    .position(|&c| c == b'\n')
                    .map_or(b.len(), |p| i + p);
                blank(&mut out, i, end);
                i = end;
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                let mut depth = 0;
                let mut j = i;
                while j < b.len() {
                    if b[j] == b'/' && b.get(j + 1) == Some(&b'*') {
                        depth += 1;
                        j += 2;
                    } else if b[j] == b'*' && b.get(j + 1) == Some(&b'/') {
                        depth -= 1;
                        j += 2;
                        if depth == 0 {
                            break;
                        }
                    } else {
                        j += 1;
                    }
                }
                blank(&mut out, i, j.min(b.len()));
                i = j;
            }
            b'r' if (b.get(i + 1) == Some(&b'#') || b.get(i + 1) == Some(&b'"'))
                && (i == 0 || !is_word(b[i - 1] as char)) =>
            {
                let mut j = i + 1;
                let mut hashes = 0;
                while b.get(j) == Some(&b'#') {
                    hashes += 1;
                    j += 1;
                }
                if b.get(j) != Some(&b'"') {
                    i += 1;
                    continue;
                }
                let close: Vec<u8> = std::iter::once(b'"')
                    .chain(std::iter::repeat_n(b'#', hashes))
                    .collect();
                let end = b[j + 1..]
                    .windows(close.len())
                    .position(|w| w == close.as_slice())
                    .map_or(b.len(), |p| j + 1 + p + close.len());
                if literals {
                    blank(&mut out, i, end);
                }
                i = end;
            }
            b'"' => {
                let mut j = i + 1;
                while j < b.len() && b[j] != b'"' {
                    j += if b[j] == b'\\' { 2 } else { 1 };
                }
                let end = (j + 1).min(b.len());
                if literals {
                    blank(&mut out, i, end);
                }
                i = end;
            }
            b'\'' => {
                let end = if b.get(i + 1) == Some(&b'\\') {
                    b[i + 2..]
                        .iter()
                        .position(|&c| c == b'\'')
                        .map(|p| i + 2 + p + 1)
                } else {
                    let ch_len = text[i + 1..].chars().next().map_or(1, char::len_utf8);
                    (b.get(i + 1 + ch_len) == Some(&b'\'')).then_some(i + 2 + ch_len)
                };
                match end {
                    Some(e) => {
                        if literals {
                            blank(&mut out, i, e);
                        }
                        i = e;
                    }
                    None => i += 1,
                }
            }
            _ => i += 1,
        }
    }
    String::from_utf8(out).unwrap_or_default()
}

/// Lint one file given by its repository-relative path.
#[must_use]
pub fn lint_text(rel: &str, text: &str) -> Vec<Finding> {
    let mut out = Vec::new();
    let mut hit = |offset: usize, src: &str, rule: u8, message: String| {
        out.push(Finding {
            path: rel.to_owned(),
            line: line_of(src, offset),
            rule,
            message,
        });
    };
    let is_rust = rel.ends_with(".rs");
    if is_rust {
        for w in [
            cat(&["TO", "DO"]),
            cat(&["FIX", "ME"]),
            cat(&["X", "XX"]),
            cat(&["HA", "CK"]),
        ] {
            for at in word_at(text, &w) {
                hit(at, text, 1, format!("placeholder marker {w}"));
            }
        }
        for m in [cat(&["to", "do!"]), cat(&["unimple", "mented!"])] {
            for at in word_at(text, &m) {
                hit(at, text, 1, format!("placeholder macro {m}"));
            }
        }
        let code = strip_comments_and_strings(text);
        for at in word_at(&code, &cat(&["#[", "ignore"])) {
            hit(at, &code, 1, "an ignored test".to_owned());
        }
        for at in word_at(&code, &cat(&["should", "_panic"])) {
            let attr_end = code[at..].find(']').map_or(code.len(), |p| at + p);
            if !text[at..attr_end].contains("expected") {
                hit(
                    at,
                    &code,
                    1,
                    "should_panic without expected = \"...\" hides any failure".to_owned(),
                );
            }
        }
        lint_code(rel, text, &code, &mut hit);
    }
    // The untracked bytecode lives in three directories (W-122); a bare `__pycache__` is not
    // a reference to them (the Rust versions of the `code_digest` walk must name it).
    for w in [cat(&["tests/", "bench"]), cat(&["tests/", "perf"])] {
        for at in word_at(text, &w) {
            hit(
                at,
                text,
                6,
                format!("reference to an untracked bytecode directory ({w})"),
            );
        }
    }
    let bench_dir = cat(&["ben", "ch/"]);
    let mut from = 0;
    while let Some(i) = text[from..].find(&bench_dir) {
        let at = from + i;
        let before = text[..at].chars().next_back();
        if !before.is_some_and(|c| is_word(c) || c == '-' || c == '.') {
            hit(
                at,
                text,
                6,
                format!("reference to an untracked bytecode directory ({bench_dir})"),
            );
        }
        from = at + bench_dir.len();
    }
    if rel.ends_with("Cargo.toml") {
        lint_manifest(text, &mut hit);
    }
    out
}

fn lint_manifest(text: &str, hit: &mut impl FnMut(usize, &str, u8, String)) {
    let code: String = text
        .lines()
        .map(|l| l.split('#').next().unwrap_or("").to_owned() + "\n")
        .collect();
    for at in word_at(text, &cat(&["[[be", "nch]]"])) {
        hit(at, text, 3, "a benchmark target".to_owned());
    }
    for name in [
        cat(&["ra", "nd"]),
        cat(&["getra", "ndom"]),
        cat(&["crite", "rion"]),
    ] {
        for at in word_at(&code, &name) {
            let rest = code[at + name.len()..].trim_start();
            if rest.starts_with('=') || rest.starts_with('.') {
                hit(at, &code, 3, format!("dependency {name}"));
            }
        }
    }
}

fn lint_code(rel: &str, text: &str, code: &str, hit: &mut impl FnMut(usize, &str, u8, String)) {
    if rel != CLOCK_FILE {
        for path in [cat(&["std::", "time"]), cat(&["core::", "time"])] {
            for at in word_at(code, &path) {
                hit(at, code, 2, format!("path into {path}"));
            }
        }
        for (open, _) in code.match_indices(&cat(&["std::", "{"])) {
            let close = code[open..].find('}').map_or(code.len(), |p| open + p);
            for at in word_at(&code[open..close], &cat(&["ti", "me"])) {
                hit(open + at, code, 2, "use list into std::time".to_owned());
            }
        }
        for name in [
            cat(&["Inst", "ant"]),
            cat(&["System", "Time"]),
            cat(&["recv", "_timeout"]),
            cat(&["wait", "_timeout"]),
            cat(&["park", "_timeout"]),
        ] {
            for at in word_at(code, &name) {
                hit(at, code, 2, format!("timing identifier {name}"));
            }
        }
        for call in [cat(&["thread::", "sleep"]), cat(&[".elap", "sed("])] {
            for at in word_at(code, &call) {
                hit(at, code, 2, format!("timing call {call}"));
            }
        }
    }
    for name in [cat(&["getra", "ndom"]), cat(&["crite", "rion"])] {
        for at in word_at(code, &name) {
            hit(at, code, 3, name.clone());
        }
    }
    for pat in [
        cat(&["ra", "nd::"]),
        cat(&["extern crate ra", "nd"]),
        cat(&["use ra", "nd"]),
    ] {
        for at in word_at(code, &pat) {
            hit(at, code, 3, "the rand crate".to_owned());
        }
    }
    for pat in [cat(&["#[be", "nch]"]), cat(&["sum::<", "f64>"])] {
        for (at, _) in code.match_indices(&pat) {
            hit(
                at,
                code,
                3,
                format!("{pat} (benchmarks are banned; sum floats with py_sum)"),
            );
        }
    }
    let kw = cat(&["uns", "afe"]);
    for at in word_at(code, &kw) {
        if rel != UNSAFE_FILE {
            hit(at, code, 4, format!("{kw} outside {UNSAFE_FILE}"));
            continue;
        }
        let line = line_of(code, at);
        let lines: Vec<&str> = text.lines().collect();
        let start = line.saturating_sub(6);
        let justified = lines[start..line.saturating_sub(1).max(start)]
            .iter()
            .any(|l| l.trim_start().starts_with(&cat(&["// SAF", "ETY:"])));
        if !justified {
            hit(
                at,
                code,
                4,
                format!("{kw} without a // SAFETY: comment above it"),
            );
        }
    }
    if rel.starts_with(SEARCH_SRC) {
        lint_search(rel, code, hit);
    }
}

/// Rule 5 (W-135).
fn lint_search(rel: &str, code: &str, hit: &mut impl FnMut(usize, &str, u8, String)) {
    let agent_for = cat(&["Agent ", "for "]);
    for (at, _) in code.match_indices(&agent_for) {
        let line_start = code[..at].rfind('\n').map_or(0, |p| p + 1);
        let head = &code[line_start..at];
        let is_impl = !word_at(head, "impl").is_empty();
        let qualified = head.ends_with("::") || head.ends_with(' ') || head.ends_with('>');
        if is_impl && qualified && rel != SEARCH_AGENT {
            hit(
                at,
                code,
                5,
                "only agent.rs may implement Agent in gcg-search".to_owned(),
            );
        }
    }
    if rel != SEARCH_AGENT {
        return;
    }
    for f in ["fn choose(", "fn choose_traced("] {
        for (at, _) in code.match_indices(f) {
            let Some(params_end) = matching(code, at + f.len() - 1, b'(', b')') else {
                continue;
            };
            let params = &code[at + f.len()..params_end];
            let Some(name) = state_param(params) else {
                continue;
            };
            let Some(open) = code[params_end..].find('{').map(|p| params_end + p) else {
                continue;
            };
            let Some(close) = matching(code, open, b'{', b'}') else {
                continue;
            };
            for use_at in word_at(&code[open..close], &name) {
                let abs = open + use_at;
                if !inside_allowed_call(code, open, abs) {
                    hit(
                        abs,
                        code,
                        5,
                        format!(
                            "the true state {name} is used outside begin/finish/Observation::new"
                        ),
                    );
                }
            }
        }
    }
}

fn state_param(params: &str) -> Option<String> {
    params.split(',').find_map(|p| {
        let (name, ty) = p.split_once(':')?;
        let ty: String = ty.chars().filter(|c| !c.is_whitespace()).collect();
        (ty.starts_with('&') && ty.ends_with("GameState"))
            .then(|| name.trim().trim_start_matches("mut ").to_owned())
    })
}

/// The index of the bracket that closes the one at `open`.
fn matching(code: &str, open: usize, l: u8, r: u8) -> Option<usize> {
    let b = code.as_bytes();
    let mut depth = 0usize;
    for (i, &c) in b.iter().enumerate().skip(open) {
        if c == l {
            depth += 1;
        } else if c == r {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
    }
    None
}

fn inside_allowed_call(code: &str, floor: usize, at: usize) -> bool {
    let b = code.as_bytes();
    let mut depth = 0usize;
    let mut i = at;
    while i > floor {
        i -= 1;
        match b[i] {
            b')' => depth += 1,
            b'(' if depth > 0 => depth -= 1,
            b'(' => {
                let callee = code[floor..i].trim_end();
                let name_start = callee
                    .rfind(|c: char| !(is_word(c) || c == ':'))
                    .map_or(0, |p| p + 1);
                let name = &callee[name_start..];
                return name.ends_with("begin")
                    || name.ends_with("finish")
                    || name == "Observation::new";
            }
            _ => {}
        }
    }
    false
}

/// The files the repository scan reads, relative to `root`, sorted.
///
/// # Errors
/// An unreadable directory.
pub fn scanned_files(root: &Path) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut stack: Vec<PathBuf> = vec![root.join("crates")];
    while let Some(dir) = stack.pop() {
        let rd = std::fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        for item in rd {
            let item = item.map_err(|e| format!("{}: {e}", dir.display()))?;
            let p = item.path();
            let name = item.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || name == "target" {
                continue;
            }
            let rel = rel_path(root, &p);
            if p.is_dir() {
                if !format!("{rel}/").starts_with(FIXTURE_DIR) {
                    stack.push(p);
                }
            } else if p.is_file() {
                out.push(rel);
            }
        }
    }
    for extra in ["Cargo.toml", ".cargo/config.toml"]
        .into_iter()
        .chain(GATE_SCRIPTS)
    {
        if root.join(extra).is_file() {
            out.push(extra.to_owned());
        }
    }
    out.sort();
    Ok(out)
}

fn rel_path(root: &Path, p: &Path) -> String {
    let rel = p.strip_prefix(root).unwrap_or(p);
    rel.iter()
        .map(|c| c.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

/// Lint the repository at `root`.
///
/// # Errors
/// An unreadable file or directory.
pub fn lint_repo(root: &Path) -> Result<Vec<Finding>, String> {
    let mut out = Vec::new();
    for rel in scanned_files(root)? {
        let is_text = [".rs", ".toml", ".sh", ".json", ".md", ".txt", ".py"]
            .iter()
            .any(|e| rel.ends_with(e));
        if !is_text {
            continue;
        }
        let text = std::fs::read_to_string(root.join(&rel)).map_err(|e| format!("{rel}: {e}"))?;
        out.extend(lint_text(&rel, &text));
    }
    Ok(out)
}

/// Lint a fixture file: its first line `// lint-path: <path>` (`# lint-path: <path>` for a
/// manifest) names the path it stands for.
///
/// # Errors
/// A fixture without the path line.
pub fn lint_fixture(text: &str) -> Result<Vec<Finding>, String> {
    let first = text.lines().next().unwrap_or("");
    let rel = first
        .strip_prefix("// lint-path: ")
        .or_else(|| first.strip_prefix("# lint-path: "))
        .ok_or_else(|| "a fixture starts with // lint-path: <path>".to_owned())?;
    Ok(lint_text(rel.trim(), text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stripping_keeps_lines_and_removes_literals() {
        let src = "let a = \"std::time\"; // Instant\nlet b = 'x'; let c: &'static str = r#\"unsafe\"#;\n/* SystemTime */ x";
        let code = strip_comments_and_strings(src);
        assert_eq!(code.lines().count(), src.lines().count());
        assert!(!code.contains("time") && !code.contains("Instant") && !code.contains("unsafe"));
        assert!(code.contains("'static"));
    }

    #[test]
    fn words_respect_identifier_boundaries() {
        assert_eq!(word_at("unsafe_code unsafe {", "unsafe"), vec![12]);
        assert!(word_at("my_rand::x", &cat(&["ra", "nd::"])).is_empty());
    }
}
