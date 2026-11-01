//! The reachability rule of docs/port/PLAN.md §2.2 (T-022, §6.5 item 2): the dependency
//! closure of `gcg-cli` (the `gcg-sim` binary) never contains `gcg-ai-reference` or
//! `gcg-diff`.
//!
//! The test reads the `Cargo.toml` of every workspace crate in the closure, starting at this
//! crate and following normal and build dependencies (plain, `[dependencies.<name>]` tables and
//! `[target.<cfg>.*]` sections, optional ones included, `workspace = true` entries resolved
//! through the root `[workspace.dependencies]`), and fails if any of them lists either crate,
//! under its own key or as the `package` of a renamed entry. Dev-dependencies are not
//! followed: they never enter a normal build (resolver 3, `cargo tree -e normal`), and
//! `gcg-search` has one on the reference for D2's null-config test. It needs no cargo
//! invocation and no network. No TOML crate is available offline, so a small reader of the
//! TOML subset that manifests use is part of the test, with its own negative controls.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

const BANNED: [&str; 2] = ["gcg-ai-reference", "gcg-diff"];

// ---------------------------------------------------------------------------------------------
// A reader for the TOML subset of Cargo manifests

#[derive(Clone, Debug, PartialEq)]
enum Value {
    Str(String),
    Bool(bool),
    /// Numbers and other scalars, as written.
    Raw(String),
    Array(Vec<Value>),
    /// An inline table; keys are dotted paths.
    Table(Vec<(Vec<String>, Value)>),
}

/// Every key of a document with its full path (table header, then the dotted key). The
/// entries of `[[array]]` tables get the path segment `#<index>`.
type Doc = Vec<(Vec<String>, Value)>;

struct Reader<'a> {
    src: &'a str,
    pos: usize,
}

impl Reader<'_> {
    fn peek(&self) -> Option<char> {
        self.src[self.pos..].chars().next()
    }

    fn starts_with(&self, s: &str) -> bool {
        self.src[self.pos..].starts_with(s)
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += c.len_utf8();
        Some(c)
    }

    fn fail<T>(&self, what: &str) -> Result<T, String> {
        let line = self.src[..self.pos].matches('\n').count() + 1;
        Err(format!("line {line}: {what}"))
    }

    fn expect(&mut self, c: char) -> Result<(), String> {
        if self.bump() == Some(c) {
            Ok(())
        } else {
            self.fail(&format!("expected {c:?}"))
        }
    }

    fn skip_blank(&mut self) {
        while matches!(self.peek(), Some(' ' | '\t')) {
            self.bump();
        }
    }

    fn skip_comment(&mut self) {
        if self.peek() == Some('#') {
            while !matches!(self.peek(), None | Some('\n')) {
                self.bump();
            }
        }
    }

    /// Blanks, newlines and comments (between array elements and between statements).
    fn skip_space(&mut self) {
        loop {
            self.skip_blank();
            self.skip_comment();
            match self.peek() {
                Some('\n' | '\r') => {
                    self.bump();
                }
                _ => return,
            }
        }
    }

    fn end_of_line(&mut self) -> Result<(), String> {
        self.skip_blank();
        self.skip_comment();
        if self.starts_with("\r\n") {
            self.pos += 2;
            return Ok(());
        }
        match self.peek() {
            None => Ok(()),
            Some('\n') => {
                self.bump();
                Ok(())
            }
            Some(_) => self.fail("expected the end of the line"),
        }
    }

    fn document(&mut self) -> Result<Doc, String> {
        let mut doc = Doc::new();
        let mut prefix: Vec<String> = Vec::new();
        let mut array_tables: BTreeMap<Vec<String>, usize> = BTreeMap::new();
        loop {
            self.skip_space();
            match self.peek() {
                None => return Ok(doc),
                Some('[') => {
                    self.bump();
                    let array = self.peek() == Some('[');
                    if array {
                        self.bump();
                    }
                    self.skip_blank();
                    let mut path = self.key()?;
                    self.skip_blank();
                    self.expect(']')?;
                    if array {
                        self.expect(']')?;
                        let n = array_tables.entry(path.clone()).or_insert(0);
                        path.push(format!("#{n}"));
                        *n += 1;
                    }
                    prefix = path;
                    self.end_of_line()?;
                }
                Some(_) => {
                    let key = self.key()?;
                    self.skip_blank();
                    self.expect('=')?;
                    self.skip_blank();
                    let value = self.value()?;
                    self.end_of_line()?;
                    let mut path = prefix.clone();
                    path.extend(key);
                    doc.push((path, value));
                }
            }
        }
    }

    /// A dotted key of bare and quoted parts.
    fn key(&mut self) -> Result<Vec<String>, String> {
        let mut parts = Vec::new();
        loop {
            let part = match self.peek() {
                Some('"') => self.basic_string()?,
                Some('\'') => self.literal_string()?,
                _ => {
                    let start = self.pos;
                    while matches!(self.peek(), Some(c) if c.is_ascii_alphanumeric() || c == '_' || c == '-')
                    {
                        self.bump();
                    }
                    if self.pos == start {
                        return self.fail("expected a key");
                    }
                    self.src[start..self.pos].to_owned()
                }
            };
            parts.push(part);
            self.skip_blank();
            if self.peek() != Some('.') {
                return Ok(parts);
            }
            self.bump();
            self.skip_blank();
        }
    }

    fn value(&mut self) -> Result<Value, String> {
        match self.peek() {
            Some('"') => self.basic_string().map(Value::Str),
            Some('\'') => self.literal_string().map(Value::Str),
            Some('[') => self.array(),
            Some('{') => self.inline_table(),
            Some(_) if self.starts_with("true") => {
                self.pos += 4;
                Ok(Value::Bool(true))
            }
            Some(_) if self.starts_with("false") => {
                self.pos += 5;
                Ok(Value::Bool(false))
            }
            Some(_) => {
                let start = self.pos;
                while !matches!(
                    self.peek(),
                    None | Some(',' | ']' | '}' | '#' | '\n' | '\r' | ' ' | '\t')
                ) {
                    self.bump();
                }
                if self.pos == start {
                    return self.fail("expected a value");
                }
                Ok(Value::Raw(self.src[start..self.pos].to_owned()))
            }
            None => self.fail("expected a value"),
        }
    }

    fn basic_string(&mut self) -> Result<String, String> {
        if self.starts_with("\"\"\"") {
            self.pos += 3;
            let Some(len) = self.src[self.pos..].find("\"\"\"") else {
                return self.fail("unterminated multi-line string");
            };
            let text = self.src[self.pos..self.pos + len].to_owned();
            self.pos += len + 3;
            return Ok(text);
        }
        self.expect('"')?;
        let mut out = String::new();
        loop {
            match self.bump() {
                None | Some('\n') => return self.fail("unterminated string"),
                Some('"') => return Ok(out),
                Some('\\') => match self.bump() {
                    Some('"') => out.push('"'),
                    Some('\\') => out.push('\\'),
                    Some('n') => out.push('\n'),
                    Some('t') => out.push('\t'),
                    Some('r') => out.push('\r'),
                    Some('u') => {
                        let hex = self.src.get(self.pos..self.pos + 4).unwrap_or("");
                        let Some(c) = u32::from_str_radix(hex, 16).ok().and_then(char::from_u32)
                        else {
                            return self.fail("bad \\u escape");
                        };
                        self.pos += 4;
                        out.push(c);
                    }
                    _ => return self.fail("unsupported escape"),
                },
                Some(c) => out.push(c),
            }
        }
    }

    fn literal_string(&mut self) -> Result<String, String> {
        self.expect('\'')?;
        let Some(len) = self.src[self.pos..].find(['\'', '\n']) else {
            return self.fail("unterminated string");
        };
        let text = self.src[self.pos..self.pos + len].to_owned();
        self.pos += len;
        self.expect('\'')?;
        Ok(text)
    }

    fn array(&mut self) -> Result<Value, String> {
        self.expect('[')?;
        let mut items = Vec::new();
        loop {
            self.skip_space();
            if self.peek() == Some(']') {
                self.bump();
                return Ok(Value::Array(items));
            }
            items.push(self.value()?);
            self.skip_space();
            match self.bump() {
                Some(',') => {}
                Some(']') => return Ok(Value::Array(items)),
                _ => return self.fail("expected ',' or ']' in an array"),
            }
        }
    }

    fn inline_table(&mut self) -> Result<Value, String> {
        self.expect('{')?;
        let mut entries = Vec::new();
        loop {
            self.skip_blank();
            if self.peek() == Some('}') {
                self.bump();
                return Ok(Value::Table(entries));
            }
            let key = self.key()?;
            self.skip_blank();
            self.expect('=')?;
            self.skip_blank();
            entries.push((key, self.value()?));
            self.skip_blank();
            match self.bump() {
                Some(',') => {}
                Some('}') => return Ok(Value::Table(entries)),
                _ => return self.fail("expected ',' or '}' in an inline table"),
            }
        }
    }
}

fn read_toml(src: &str) -> Result<Doc, String> {
    Reader { src, pos: 0 }.document()
}

// ---------------------------------------------------------------------------------------------
// Dependencies of a manifest

/// The fields of one dependency entry that decide which crate it is.
#[derive(Clone, Debug, Default, PartialEq)]
struct DepFields {
    package: Option<String>,
    path: Option<String>,
    workspace: bool,
}

impl DepFields {
    fn set(&mut self, field: &str, value: &Value) -> Result<(), String> {
        match (field, value) {
            ("package", Value::Str(s)) => self.package = Some(s.clone()),
            ("path", Value::Str(s)) => self.path = Some(s.clone()),
            ("workspace", Value::Bool(b)) => self.workspace = *b,
            ("package" | "path" | "workspace", other) => {
                return Err(format!("unexpected value for `{field}`: {other:?}"));
            }
            _ => {}
        }
        Ok(())
    }
}

/// Dependency entries `(section, key) -> fields` of the tables under `prefix` whose kind is in
/// `kinds` (`dependencies`, `build-dependencies`), including `[target.<cfg>.<kind>]` when
/// `targets` is set.
fn dependency_tables(
    doc: &Doc,
    prefix: &[&str],
    kinds: &[&str],
    targets: bool,
) -> Result<BTreeMap<(String, String), DepFields>, String> {
    let mut out: BTreeMap<(String, String), DepFields> = BTreeMap::new();
    for (path, value) in doc {
        if path.len() <= prefix.len() || path[..prefix.len()] != *prefix {
            continue;
        }
        let rest = &path[prefix.len()..];
        let (section, rest) = if kinds.contains(&rest[0].as_str()) {
            (rest[0].clone(), &rest[1..])
        } else if targets
            && rest[0] == "target"
            && rest.len() >= 3
            && kinds.contains(&rest[2].as_str())
        {
            (format!("target.{}.{}", rest[1], rest[2]), &rest[3..])
        } else {
            continue;
        };
        let Some(key) = rest.first() else {
            return Err(format!("a value assigned to the table [{section}]"));
        };
        let fields = out.entry((section.clone(), key.clone())).or_default();
        match (&rest[1..], value) {
            ([], Value::Str(_)) => {}
            ([], Value::Table(entries)) => {
                for (k, v) in entries {
                    if let [field] = k.as_slice() {
                        fields.set(field, v)?;
                    }
                }
            }
            ([field], v) => fields.set(field, v)?,
            _ => return Err(format!("unexpected dependency entry {path:?} = {value:?}")),
        }
    }
    Ok(out)
}

fn string_at(doc: &Doc, path: &[&str]) -> Option<String> {
    doc.iter().find_map(|(p, v)| match v {
        Value::Str(s) if p.iter().map(String::as_str).eq(path.iter().copied()) => Some(s.clone()),
        _ => None,
    })
}

/// `a/b/../c` -> `a/c` without touching the file system.
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            other => out.push(other),
        }
    }
    out
}

// ---------------------------------------------------------------------------------------------
// The closure walk

/// What the walk found: every crate of the closure by package name, and each entry that names
/// a banned crate.
#[derive(Debug, Default)]
struct Closure {
    crates: BTreeSet<String>,
    violations: Vec<String>,
}

/// Walks the closure of the crate in `start` (a directory holding a `Cargo.toml`) inside the
/// workspace whose root manifest is in `root`. `read` returns a file's text.
fn walk(
    root: &Path,
    start: &Path,
    read: &dyn Fn(&Path) -> Result<String, String>,
) -> Result<Closure, String> {
    let root_manifest = root.join("Cargo.toml");
    let root_doc = read_toml(&read(&root_manifest)?)
        .map_err(|e| format!("{}: {e}", root_manifest.display()))?;
    let workspace_deps: BTreeMap<String, DepFields> =
        dependency_tables(&root_doc, &["workspace"], &["dependencies"], false)?
            .into_iter()
            .map(|((_, key), fields)| (key, fields))
            .collect();
    let mut closure = Closure::default();
    let mut queue = vec![normalize(start)];
    let mut seen: BTreeSet<PathBuf> = BTreeSet::new();
    while let Some(dir) = queue.pop() {
        if !seen.insert(dir.clone()) {
            continue;
        }
        let manifest = dir.join("Cargo.toml");
        let doc =
            read_toml(&read(&manifest)?).map_err(|e| format!("{}: {e}", manifest.display()))?;
        let name = string_at(&doc, &["package", "name"])
            .ok_or_else(|| format!("{}: no [package] name", manifest.display()))?;
        let deps = dependency_tables(&doc, &[], &["dependencies", "build-dependencies"], true)
            .map_err(|e| format!("{}: {e}", manifest.display()))?;
        for ((section, key), fields) in deps {
            let (package, path) = if fields.workspace {
                let ws = workspace_deps.get(&key).ok_or_else(|| {
                    format!("{name}: [{section}] {key} has workspace = true but the root does not define it")
                })?;
                (
                    fields.package.clone().or_else(|| ws.package.clone()),
                    ws.path.as_ref().map(|p| root.join(p)),
                )
            } else {
                (
                    fields.package.clone(),
                    fields.path.as_ref().map(|p| dir.join(p)),
                )
            };
            let package = package.unwrap_or_else(|| key.clone());
            if BANNED.contains(&package.as_str()) || BANNED.contains(&key.as_str()) {
                closure
                    .violations
                    .push(format!("{name}: [{section}] {key} -> {package}"));
            }
            if let Some(path) = path {
                queue.push(normalize(&path));
            }
        }
        closure.crates.insert(name);
    }
    Ok(closure)
}

// ---------------------------------------------------------------------------------------------
// The rule on this workspace

#[test]
fn gcg_sim_closure_excludes_reference_and_diff() {
    let here = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = here.join("../..");
    let read = |p: &Path| std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()));
    let closure = walk(&root, here, &read).expect("the workspace manifests parse");
    assert!(
        closure.violations.is_empty(),
        "gcg-sim's closure lists a banned crate (PLAN §2.2): {:?}",
        closure.violations
    );
    // The walk really followed the graph: every crate gcg-sim is built from is there, and
    // nothing that would carry the reference.
    for name in [
        "gcg-cli",
        "gcg-runner",
        "gcg-search",
        "gcg-ai",
        "gcg-engine",
        "gcg-effects",
        "gcg-data",
        "gcg-core",
    ] {
        assert!(
            closure.crates.contains(name),
            "{name} missing from {:?}",
            closure.crates
        );
    }
    for name in ["gcg-ai-reference", "gcg-diff", "gcg-eval"] {
        assert!(
            !closure.crates.contains(name),
            "{name} in {:?}",
            closure.crates
        );
    }
}

// ---------------------------------------------------------------------------------------------
// Negative controls on synthetic workspaces

const ROOT: &str = r#"
[workspace]
resolver = "3"
members = [
    "crates/cli",   # the binary
    "crates/mid",
    "crates/ref",
]

[workspace.dependencies]
mid = { path = "crates/mid" }
gcg-ai-reference = { path = "crates/ref" }
alias = { package = "gcg-diff", path = "crates/diff" }
serde_json = { version = "1", features = ["preserve_order", "float_roundtrip"] }
"#;

fn package(name: &str, body: &str) -> String {
    format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\n\n{body}")
}

fn run(cli: &str, mid: &str) -> Closure {
    let files: BTreeMap<PathBuf, String> = [
        ("ws/Cargo.toml", ROOT.to_owned()),
        ("ws/crates/cli/Cargo.toml", package("cli", cli)),
        ("ws/crates/mid/Cargo.toml", package("mid", mid)),
        ("ws/crates/ref/Cargo.toml", package("gcg-ai-reference", "")),
        ("ws/crates/diff/Cargo.toml", package("gcg-diff", "")),
    ]
    .into_iter()
    .map(|(p, s)| (PathBuf::from(p), s))
    .collect();
    let read = |p: &Path| {
        files
            .get(&normalize(p))
            .cloned()
            .ok_or_else(|| format!("no file {}", p.display()))
    };
    walk(Path::new("ws"), Path::new("ws/crates/cli"), &read).expect("synthetic manifests parse")
}

const CLEAN_CLI: &str = "[dependencies]\nmid.workspace = true\nserde_json.workspace = true\n";

#[test]
fn a_clean_closure_passes_and_is_complete() {
    let c = run(CLEAN_CLI, "[dependencies]\nserde_json = \"1\"\n");
    assert!(c.violations.is_empty(), "{:?}", c.violations);
    assert_eq!(
        c.crates,
        ["cli", "mid"].map(str::to_owned).into_iter().collect()
    );
}

#[test]
fn every_way_of_listing_a_banned_crate_is_found() {
    let cases = [
        // Direct path dependency.
        (
            "[dependencies]\ngcg-ai-reference = { path = \"../ref\" }\n",
            "",
            "cli: [dependencies] gcg-ai-reference -> gcg-ai-reference",
        ),
        // Transitive, through a workspace dependency.
        (
            CLEAN_CLI,
            "[dependencies]\ngcg-ai-reference.workspace = true\n",
            "mid: [dependencies] gcg-ai-reference -> gcg-ai-reference",
        ),
        // Renamed in the workspace table.
        (
            "[dependencies]\nalias = { workspace = true, optional = true }\n",
            "",
            "cli: [dependencies] alias -> gcg-diff",
        ),
        // Renamed in the crate.
        (
            "[dependencies]\nsearch = { package = \"gcg-ai-reference\", path = \"../ref\" }\n",
            "",
            "cli: [dependencies] search -> gcg-ai-reference",
        ),
        // Table form, after a comment and a quoted key.
        (
            "[dependencies]\n\"mid\" = { workspace = true }\n\n# reference\n[dependencies.gcg-diff]\npath = '../diff'\n",
            "",
            "cli: [dependencies] gcg-diff -> gcg-diff",
        ),
        // Target-specific and build dependencies.
        (
            CLEAN_CLI,
            "[target.'cfg(unix)'.dependencies]\ngcg-diff = { path = \"../diff\" }\n",
            "mid: [target.cfg(unix).dependencies] gcg-diff -> gcg-diff",
        ),
        (
            "[build-dependencies]\ngcg-ai-reference = { path = \"../ref\" }\n",
            "",
            "cli: [build-dependencies] gcg-ai-reference -> gcg-ai-reference",
        ),
    ];
    for (cli, mid, expected) in cases {
        let c = run(cli, mid);
        assert!(
            c.violations.iter().any(|v| v == expected),
            "{expected:?} not in {:?}",
            c.violations
        );
    }
}

#[test]
fn dev_dependencies_are_not_followed() {
    let c = run(
        "[dependencies]\nmid.workspace = true\n\n[dev-dependencies]\ngcg-ai-reference.workspace = true\n",
        "[dev-dependencies]\ngcg-diff = { path = \"../diff\" }\n",
    );
    assert!(c.violations.is_empty(), "{:?}", c.violations);
    assert!(!c.crates.contains("gcg-ai-reference"));
}

#[test]
fn the_reader_handles_the_manifest_subset_and_rejects_the_rest() {
    let doc = read_toml(
        "a = [\n  \"x\", # one\n  'y',\n]\n[t]\n\"q.k\" = \"\\u00fc\\\"\" # c\n[[bin]]\nname = \"b\"\n[[bin]]\nname = \"c\"\nn = 1_000\n",
    )
    .unwrap();
    assert_eq!(
        doc,
        vec![
            (
                vec!["a".to_owned()],
                Value::Array(vec![Value::Str("x".into()), Value::Str("y".into())])
            ),
            (
                vec!["t".to_owned(), "q.k".to_owned()],
                Value::Str("\u{fc}\"".into())
            ),
            (
                vec!["bin".to_owned(), "#0".to_owned(), "name".to_owned()],
                Value::Str("b".into())
            ),
            (
                vec!["bin".to_owned(), "#1".to_owned(), "name".to_owned()],
                Value::Str("c".into())
            ),
            (
                vec!["bin".to_owned(), "#1".to_owned(), "n".to_owned()],
                Value::Raw("1_000".into())
            ),
        ]
    );
    for bad in [
        "a = \"open\n",
        "[t\n",
        "x = { y = 1\n",
        "= 3\n",
        "a = 1 b\n",
    ] {
        assert!(read_toml(bad).is_err(), "{bad:?}");
    }
    let mut files = BTreeMap::new();
    files.insert(PathBuf::from("w/Cargo.toml"), "[workspace]\n".to_owned());
    files.insert(
        PathBuf::from("w/c/Cargo.toml"),
        package("c", "[dependencies]\nmissing.workspace = true\n"),
    );
    let read = |p: &Path| {
        files
            .get(&normalize(p))
            .cloned()
            .ok_or_else(|| "absent".to_owned())
    };
    let err = walk(Path::new("w"), Path::new("w/c"), &read).unwrap_err();
    assert!(err.contains("missing has workspace = true"), "{err}");
}
