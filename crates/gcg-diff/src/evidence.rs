//! What the Rust side has shown, for `coverage --check` and `traceability` (PLAN §9.2-§9.3):
//! the per-test traces that pass in the real-engine replay with the tags their `scenario`
//! records carry, whether every game trace of the Python executed-program census passes,
//! which definitions the replay instantiated, and the native Rust tests tagged with a
//! `// gcg-tags: rule=8-5-2 card=GD01-001 faq=Q12 ruling=GD01-001:Q100` line directly above
//! `#[test]`.

use std::path::Path;

use crate::corpus::{self, CorpusArgs, EngineKind};
use crate::manifest::Repo;
use crate::read_trace;
use crate::{Record, TraceKind};

/// The census game sets (`port/oracle/census.py` `GAME_SETS`).
pub const GAME_SETS: [&str; 7] = [
    "full",
    "vanilla",
    "fuzz",
    "coverage",
    "search",
    "baselines",
    "worlds",
];

/// One native Rust test's tags.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct NativeTest {
    /// `crates/<crate>/<path>.rs::<fn>`.
    pub id: String,
    /// `(mark, value)`: `rule`, `card`, `faq` or `ruling`.
    pub tags: Vec<(String, String)>,
}

/// The evidence of one run.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Evidence {
    /// Per-test trace id (the pytest node id) and whether its Rust replay passed.
    pub tests: Vec<(String, bool)>,
    /// Per-test trace id and the tags of its `scenario` records (`kind:value`).
    pub test_tags: Vec<(String, Vec<String>)>,
    /// Game traces of the census sets, and how many failed.
    pub games: usize,
    pub failed_games: usize,
    /// Per def id: a card of it was in a state the replay reached.
    pub instantiated: Vec<bool>,
    pub native: Vec<NativeTest>,
    /// Integrity errors of the corpus run.
    pub errors: Vec<String>,
}

impl Evidence {
    /// Whether the per-test trace `id` passed.
    #[must_use]
    pub fn passed(&self, id: &str) -> bool {
        self.tests.iter().any(|(t, ok)| t == id && *ok)
    }
}

/// The tags of the per-test traces, read from the corpus.
///
/// # Errors
/// A manifest or decoding error.
pub fn test_tags(repo: &Repo) -> Result<Vec<(String, Vec<String>)>, String> {
    let m = crate::manifest::load(repo, 1)?;
    let mut out = Vec::new();
    for e in m
        .entries
        .iter()
        .filter(|e| e.set == "tests" && e.records.is_some())
    {
        for t in read_trace(&m.root.join(&e.path)).map_err(|x| x.to_string())? {
            if t.header.kind != TraceKind::Test {
                continue;
            }
            let mut tags: Vec<String> = Vec::new();
            for r in &t.records {
                if let Record::Scenario(s) = r {
                    for tag in &s.tags {
                        if !tags.contains(tag) {
                            tags.push(tag.clone());
                        }
                    }
                }
            }
            out.push((t.header.id.clone(), tags));
        }
    }
    Ok(out)
}

/// The test outcomes of a `gcg-diff corpus --junit` file: `(classname, name, passed)`.
///
/// # Errors
/// The file cannot be read.
pub fn read_junit(path: &Path) -> Result<Vec<(String, String, bool)>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let attr = |line: &str, key: &str| -> Option<String> {
        let start = line.find(&format!(" {key}=\""))? + key.len() + 3;
        let end = line[start..].find('"')? + start;
        Some(unescape(&line[start..end]))
    };
    let mut out = Vec::new();
    for line in text.lines().filter(|l| l.starts_with("<testcase ")) {
        let (Some(class), Some(name)) = (attr(line, "classname"), attr(line, "name")) else {
            return Err(format!("{}: malformed testcase {line:?}", path.display()));
        };
        out.push((class, name, !line.contains("<failure")));
    }
    Ok(out)
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// The native Rust tests with a `// gcg-tags:` line directly above their `#[test]`.
///
/// # Errors
/// An unreadable source file.
pub fn native_tests(repo: &Repo) -> Result<Vec<NativeTest>, String> {
    let mut files: Vec<String> = crate::lints::scanned_files(&repo.root)?
        .into_iter()
        .filter(|f| f.ends_with(".rs"))
        .collect();
    files.sort();
    let mut out = Vec::new();
    for f in files {
        let text = std::fs::read_to_string(repo.root.join(&f)).map_err(|e| format!("{f}: {e}"))?;
        let lines: Vec<&str> = text.lines().collect();
        for (i, l) in lines.iter().enumerate() {
            let Some(rest) = l.trim().strip_prefix("// gcg-tags:") else {
                continue;
            };
            if lines.get(i + 1).map(|x| x.trim()) != Some("#[test]") {
                continue;
            }
            let Some(name) = lines[i + 2..].iter().find_map(|x| fn_name(x)) else {
                continue;
            };
            let tags = rest
                .split_whitespace()
                .filter_map(|kv| kv.split_once('='))
                .map(|(k, v)| (k.to_owned(), v.to_owned()))
                .collect();
            out.push(NativeTest {
                id: format!("{f}::{name}"),
                tags,
            });
        }
    }
    Ok(out)
}

fn fn_name(line: &str) -> Option<String> {
    let t = line.trim();
    let rest = t
        .strip_prefix("fn ")
        .or_else(|| t.strip_prefix("pub fn "))?;
    let end = rest.find(['(', '<'])?;
    Some(rest[..end].to_owned())
}

/// Collects the evidence. The per-test outcomes come from `junit` when given (a `gcg-diff
/// corpus --junit` file), else from a real-engine replay of the tier-1 corpus, which with
/// `census` also replays the game sets and records the definitions it instantiates.
///
/// # Errors
/// A manifest or decoding error.
pub fn collect(repo: &Repo, junit: Option<&Path>, census: bool) -> Result<Evidence, String> {
    let mut ev = Evidence {
        test_tags: test_tags(repo)?,
        native: native_tests(repo)?,
        instantiated: vec![false; gcg_data::CardDb::embedded().len()],
        ..Evidence::default()
    };
    if let Some(j) = junit.filter(|_| !census) {
        for (class, name, ok) in read_junit(j)? {
            if class == "tests" {
                ev.tests.push((name, ok));
            } else if GAME_SETS.contains(&class.as_str()) {
                ev.games += 1;
                ev.failed_games += usize::from(!ok);
            }
        }
        return Ok(ev);
    }
    let mut only: Vec<(String, Option<String>)> = vec![("tests".to_owned(), None)];
    if census {
        only.extend(GAME_SETS.iter().map(|s| ((*s).to_owned(), None)));
    }
    let args = CorpusArgs {
        tier: 1,
        only,
        file: None,
        engine: EngineKind::Real,
        roundtrip: false,
        paranoid: cfg!(feature = "paranoid"),
        require_all: true,
        jobs: std::thread::available_parallelism().map_or(1, std::num::NonZero::get),
        report: None,
        junit: None,
        census,
    };
    let report = corpus::run(repo, &args);
    ev.errors = report.errors;
    for r in report.results {
        if r.set == "tests" {
            ev.tests.push((r.id, r.outcome.is_ok()));
        } else {
            ev.games += 1;
            ev.failed_games += usize::from(r.outcome.is_err());
        }
        for d in r.seen_defs {
            if let Some(slot) = ev.instantiated.get_mut(usize::from(d)) {
                *slot = true;
            }
        }
    }
    Ok(ev)
}

/// The trace ids a Python tag of `path::test` covers: the test itself, its parametrisations
/// (`[...]`), the methods of a class, and with `<module>` every test of the file
/// (`gcg_sim.tools.marks.scan`).
#[must_use]
pub fn traces_of<'a>(ev: &'a Evidence, path: &str, test: &str) -> Vec<&'a str> {
    let full = format!("{path}::{test}");
    ev.test_tags
        .iter()
        .map(|(id, _)| id.as_str())
        .filter(|id| {
            let base = id.split('[').next().unwrap_or(id);
            if test == "<module>" {
                base.split("::").next() == Some(path)
            } else {
                base == full || base.starts_with(&format!("{full}::"))
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn junit_files_written_by_corpus_read_back() {
        let report = corpus::CorpusReport {
            errors: Vec::new(),
            results: vec![
                corpus::TraceResult {
                    set: "tests".to_owned(),
                    id: "tests/rules/test_x.py::test_a[<1>&\"2\"]".to_owned(),
                    file: "f".to_owned(),
                    outcome: Ok(crate::ReplayStats::default()),
                    seen_defs: Vec::new(),
                },
                corpus::TraceResult {
                    set: "fuzz".to_owned(),
                    id: "fuzz/s1/g0001".to_owned(),
                    file: "f".to_owned(),
                    outcome: Err("DIVERGED".to_owned()),
                    seen_defs: Vec::new(),
                },
            ],
        };
        let dir = std::env::temp_dir().join(format!("gcg-diff-junit-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let p = dir.join("j.xml");
        std::fs::write(&p, corpus::junit_xml(&report)).expect("write");
        let got = read_junit(&p).expect("reads");
        assert_eq!(
            got,
            vec![
                (
                    "tests".to_owned(),
                    "tests/rules/test_x.py::test_a[<1>&\"2\"]".to_owned(),
                    true
                ),
                ("fuzz".to_owned(), "fuzz/s1/g0001".to_owned(), false),
            ]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tags_reach_parametrised_tests_classes_and_modules() {
        let ev = Evidence {
            test_tags: vec![
                ("tests/a.py::test_x".to_owned(), Vec::new()),
                ("tests/a.py::test_x[1]".to_owned(), Vec::new()),
                ("tests/a.py::test_xy".to_owned(), Vec::new()),
                ("tests/a.py::TestK::test_m".to_owned(), Vec::new()),
                ("tests/b.py::test_z".to_owned(), Vec::new()),
            ],
            ..Evidence::default()
        };
        assert_eq!(
            traces_of(&ev, "tests/a.py", "test_x"),
            ["tests/a.py::test_x", "tests/a.py::test_x[1]"]
        );
        assert_eq!(
            traces_of(&ev, "tests/a.py", "TestK"),
            ["tests/a.py::TestK::test_m"]
        );
        assert_eq!(
            traces_of(&ev, "tests/b.py", "<module>"),
            ["tests/b.py::test_z"]
        );
    }

    #[test]
    fn native_tags_are_read_above_test_functions() {
        assert_eq!(
            fn_name("    fn some_test() {"),
            Some("some_test".to_owned())
        );
        assert_eq!(fn_name("pub fn g<T>(x: T)"), Some("g".to_owned()));
        assert_eq!(fn_name("let x = 1;"), None);
    }
}
