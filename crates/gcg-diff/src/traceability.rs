//! `gcg-diff traceability --check | --write [--junit FILE]` (PLAN §9.2): the Rust
//! `docs/RULES_TRACEABILITY.md` and `docs/port/TRACEABILITY_CARDS.md`.
//!
//! Sources: the rules index (`src/gcg_sim/data/rules/rules_index.json`), the Python tags
//! (`port/corpus/v1/tier1/tests/tags.json`, `port/oracle/export_tags.py`), the outcomes of the
//! per-test traces in the real-engine replay (a `gcg-diff corpus --junit` file, or a replay
//! run here), and the native Rust tests tagged `// gcg-tags: ...` (see [`crate::evidence`]).
//!
//! Each row keeps the Python columns of `gcg_sim.tools.traceability` (`tools/traceability.py:
//! 40-90`: tests keyed `<file name>::<test>`, the first four shown) and adds a "Rust" column:
//! the native tests, then `trace <id>` for every passing per-test trace the row's tags reach.
//! An entry counts as tested for Rust iff it has a native test or a passing trace; for a card
//! also a passing executed-program census (`gcg-diff coverage`). `--check` fails when a file
//! differs from a regeneration, or when an entry that Python tests has no Rust evidence.

use serde_json::Value;

use crate::coverage::{census_not_ran, is_vanilla};
use crate::evidence::{Evidence, traces_of};
use crate::manifest::Repo;

/// The rules document.
pub const RULES_DOC: &str = "docs/RULES_TRACEABILITY.md";
/// The card, FAQ and ruling document.
pub const CARDS_DOC: &str = "docs/port/TRACEABILITY_CARDS.md";

/// One Python tag (`tags.json` `tags`).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Tag {
    pub mark: String,
    pub value: String,
    pub path: String,
    pub test: String,
}

fn read_json(repo: &Repo, rel: &str) -> Result<Value, String> {
    let p = repo.root.join(rel);
    let text = std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", p.display()))
}

fn str_of<'a>(v: &'a Value, key: &str, what: &str) -> Result<&'a str, String> {
    v[key]
        .as_str()
        .ok_or_else(|| format!("{what}: {key} is not a string"))
}

/// The Python tags.
///
/// # Errors
/// An unreadable or malformed `tags.json`.
pub fn python_tags(repo: &Repo) -> Result<Vec<Tag>, String> {
    let v = read_json(repo, "port/corpus/v1/tier1/tests/tags.json")?;
    v["tags"]
        .as_array()
        .ok_or_else(|| "tags.json: no tags list".to_owned())?
        .iter()
        .map(|t| {
            Ok(Tag {
                mark: str_of(t, "mark", "tags.json")?.to_owned(),
                value: str_of(t, "value", "tags.json")?.to_owned(),
                path: str_of(t, "path", "tags.json")?.to_owned(),
                test: str_of(t, "test", "tags.json")?.to_owned(),
            })
        })
        .collect()
}

/// `f"{Path(t.path).name}::{t.test.split('::')[-1]}"` (`tools/traceability.py:43-45`).
fn python_key(t: &Tag) -> String {
    let file = t.path.rsplit('/').next().unwrap_or(&t.path);
    let test = t.test.rsplit("::").next().unwrap_or(&t.test);
    format!("{file}::{test}")
}

/// The Python column's test list and the Rust column for the tags of one entry.
struct Row {
    python: Vec<String>,
    rust: Vec<String>,
}

fn row(tags: &[&Tag], mark: &str, value: &str, ev: &Evidence) -> Row {
    let mut python: Vec<String> = tags.iter().map(|t| python_key(t)).collect();
    python.sort();
    python.dedup();
    let mut rust: Vec<String> = ev
        .native
        .iter()
        .filter(|n| n.tags.iter().any(|(k, v)| k == mark && v == value))
        .map(|n| n.id.clone())
        .collect();
    let mut traces: Vec<&str> = tags
        .iter()
        .flat_map(|t| traces_of(ev, &t.path, &t.test))
        .filter(|id| ev.passed(id))
        .collect();
    traces.sort();
    traces.dedup();
    rust.extend(traces.into_iter().map(|id| format!("trace {id}")));
    Row { python, rust }
}

fn shown(items: &[String], fmt: impl Fn(&str) -> String) -> String {
    let mut s = items
        .iter()
        .take(4)
        .map(|x| fmt(x))
        .collect::<Vec<_>>()
        .join(", ");
    if items.len() > 4 {
        s.push_str(&format!(" (+{})", items.len() - 4));
    }
    s
}

fn tick(x: &str) -> String {
    format!("`{x}`")
}

fn rust_cell(rust: &[String]) -> String {
    shown(rust, |x| match x.strip_prefix("trace ") {
        Some(id) => format!("trace `{id}`"),
        None => format!("`{x}`"),
    })
}

/// Both documents and the entries without Rust evidence.
pub struct Docs {
    pub rules: String,
    pub cards: String,
    pub problems: Vec<String>,
}

/// Generates both documents from the evidence.
///
/// # Errors
/// An unreadable source.
pub fn generate(repo: &Repo, ev: &Evidence) -> Result<Docs, String> {
    let tags = python_tags(repo)?;
    let mut problems = Vec::new();
    let rules = rules_doc(repo, &tags, ev, &mut problems)?;
    let cards = cards_doc(repo, &tags, ev, &mut problems)?;
    Ok(Docs {
        rules,
        cards,
        problems,
    })
}

fn by_value<'a>(tags: &'a [Tag], mark: &str) -> Vec<(&'a str, Vec<&'a Tag>)> {
    let mut out: Vec<(&str, Vec<&Tag>)> = Vec::new();
    for t in tags.iter().filter(|t| t.mark == mark) {
        match out.iter_mut().find(|(v, _)| *v == t.value) {
            Some(slot) => slot.1.push(t),
            None => out.push((&t.value, vec![t])),
        }
    }
    out
}

fn tags_for<'a>(index: &[(&str, Vec<&'a Tag>)], value: &str) -> Vec<&'a Tag> {
    index
        .iter()
        .find(|(v, _)| *v == value)
        .map(|(_, t)| t.clone())
        .unwrap_or_default()
}

fn rules_doc(
    repo: &Repo,
    tags: &[Tag],
    ev: &Evidence,
    problems: &mut Vec<String>,
) -> Result<String, String> {
    let ri = read_json(repo, "src/gcg_sim/data/rules/rules_index.json")?;
    let version = str_of(&ri["source"], "version", "rules_index.json")?;
    let date = str_of(&ri["source"], "date", "rules_index.json")?;
    let rules = ri["rules"]
        .as_object()
        .ok_or_else(|| "rules_index.json: no rules".to_owned())?;
    let index = by_value(tags, "rule");
    let (mut tested, mut na, mut missing, mut rust_tested, mut rust_missing) = (0, 0, 0, 0, 0);
    let mut rows = Vec::new();
    for (rid, entry) in rules {
        if entry["status"] == "na" {
            na += 1;
            let reason = entry["na_reason"].as_str().unwrap_or_default();
            rows.push(format!("| {rid} | N/A | {reason} | N/A |"));
            continue;
        }
        let r = row(&tags_for(&index, rid), "rule", rid, ev);
        let python = if r.python.is_empty() {
            missing += 1;
            "| MISSING | ".to_owned()
        } else {
            tested += 1;
            format!(
                "| tested ({}) | {} ",
                r.python.len(),
                shown(&r.python, tick)
            )
        };
        let rust = if r.rust.is_empty() {
            rust_missing += 1;
            problems.push(format!("rule {rid} has no Rust evidence"));
            "MISSING".to_owned()
        } else {
            rust_tested += 1;
            rust_cell(&r.rust)
        };
        rows.push(format!("| {rid} {python}| {rust} |"));
    }
    let mut out = vec![
        "# Rules traceability".to_owned(),
        String::new(),
        format!(
            "Comprehensive Rules Ver. {version} ({date}). Generated by `gcg-diff traceability --write` from"
        ),
        "the rules index, the `@pytest.mark.rule` tags (`port/corpus/v1/tier1/tests/tags.json`), the".to_owned(),
        "real-engine replay of the per-test traces (`port/corpus/v1/tier1/tests/`) and the `// gcg-tags:`".to_owned(),
        "lines of native Rust tests. A rule is tested for Rust when a native test or a passing per-test".to_owned(),
        "trace carries its tag. Card, FAQ and ruling tables: `docs/port/TRACEABILITY_CARDS.md`.".to_owned(),
        format!(
            "Summary: Python {tested} rules tested, {na} N/A with reasons, {missing} missing; Rust {rust_tested} tested, {rust_missing} without Rust evidence."
        ),
        String::new(),
        "| rule | status | tests / reason | Rust |".to_owned(),
        "| --- | --- | --- | --- |".to_owned(),
    ];
    out.extend(rows);
    Ok(out.join("\n") + "\n")
}

fn na_map(repo: &Repo, rel: &str) -> Result<Vec<(String, String)>, String> {
    let p = repo.root.join(rel);
    let mut files: Vec<std::path::PathBuf> = if p.is_dir() {
        std::fs::read_dir(&p)
            .map_err(|e| format!("{}: {e}", p.display()))?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|f| f.extension().is_some_and(|x| x == "json"))
            .collect()
    } else {
        vec![p]
    };
    files.sort();
    let mut out: Vec<(String, String)> = Vec::new();
    for f in files {
        let text = std::fs::read_to_string(&f).map_err(|e| format!("{}: {e}", f.display()))?;
        let v: Value = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", f.display()))?;
        for (k, reason) in v.as_object().into_iter().flatten() {
            let reason = reason.as_str().unwrap_or_default().to_owned();
            match out.iter_mut().find(|(x, _)| x == k) {
                Some(slot) => slot.1 = reason,
                None => out.push((k.clone(), reason)),
            }
        }
    }
    Ok(out)
}

#[expect(
    clippy::too_many_lines,
    reason = "the three tables of one document, each a short loop"
)]
fn cards_doc(
    repo: &Repo,
    tags: &[Tag],
    ev: &Evidence,
    problems: &mut Vec<String>,
) -> Result<String, String> {
    let reg = gcg_effects::Registry::embedded();
    let census_ok = ev.games > 0 && ev.failed_games == 0;
    let not_ran = census_not_ran(repo)?;
    let card_index = by_value(tags, "card");
    let mut cards: Vec<&gcg_data::CardDef> = reg.db().real_cards().collect();
    cards.sort_by(|a, b| a.card_number.cmp(&b.card_number));
    let mut card_rows = Vec::new();
    let (mut nv, mut v, mut nv_rust) = (0, 0, 0);
    for cd in cards {
        let n = &*cd.card_number;
        let r = row(&tags_for(&card_index, n), "card", n, ev);
        let python = if r.python.is_empty() {
            "—".to_owned()
        } else {
            format!("tested ({}) {}", r.python.len(), shown(&r.python, tick))
        };
        if is_vanilla(&reg, cd.def_id) {
            v += 1;
            card_rows.push(format!(
                "| {n} | vanilla | {python} | no abilities (coverage: instantiated or token) |"
            ));
            continue;
        }
        nv += 1;
        let rust = if !r.rust.is_empty() {
            nv_rust += 1;
            rust_cell(&r.rust)
        } else if census_ok && !not_ran.iter().any(|x| x == n) {
            nv_rust += 1;
            "census: ran in the corpus games".to_owned()
        } else {
            problems.push(format!("card {n} has no Rust evidence"));
            "MISSING".to_owned()
        };
        card_rows.push(format!("| {n} | non-vanilla | {python} | {rust} |"));
    }

    let faq = read_json(repo, "src/gcg_sim/data/gcgapi/rules-faq.json")?;
    let faq_na = na_map(repo, "tests/meta/faq_na.json")?;
    let faq_index = by_value(tags, "faq");
    let mut faq_rows = Vec::new();
    let (mut faq_n, mut faq_na_n, mut faq_rust) = (0, 0, 0);
    for e in faq.as_array().into_iter().flatten() {
        let num = str_of(e, "num", "rules-faq.json")?;
        faq_n += 1;
        faq_rows.push(entry_row(
            num,
            "faq",
            &faq_index,
            &faq_na,
            ev,
            problems,
            &mut faq_na_n,
            &mut faq_rust,
        ));
    }

    let rulings = read_json(repo, "src/gcg_sim/data/gcgapi/rulings.json")?;
    let rulings_na = na_map(repo, "tests/meta/rulings_na")?;
    let ruling_index = by_value(tags, "ruling");
    let mut ruling_rows = Vec::new();
    let (mut ru_n, mut ru_na_n, mut ru_rust) = (0, 0, 0);
    for e in rulings.as_array().into_iter().flatten() {
        let id = format!(
            "{}:{}",
            str_of(e, "card_number", "rulings.json")?,
            str_of(e, "num", "rulings.json")?
        );
        ru_n += 1;
        ruling_rows.push(entry_row(
            &id,
            "ruling",
            &ruling_index,
            &rulings_na,
            ev,
            problems,
            &mut ru_na_n,
            &mut ru_rust,
        ));
    }

    let mut out = vec![
        "# Card, FAQ and ruling traceability".to_owned(),
        String::new(),
        "Generated by `gcg-diff traceability --write` (docs/port/PLAN.md §9.2) from the card database,".to_owned(),
        "the gcg-api FAQ and ruling lists, the N/A reasons in `tests/meta/`, the Python tags".to_owned(),
        "(`port/corpus/v1/tier1/tests/tags.json`), the real-engine replay of the per-test traces and".to_owned(),
        "the `// gcg-tags:` lines of native Rust tests. A non-vanilla card also counts for Rust when the".to_owned(),
        "executed-program census saw it run and every census game trace passes (`gcg-diff coverage`).".to_owned(),
        String::new(),
        format!(
            "Summary: cards {} real ({nv} non-vanilla, {nv_rust} with Rust evidence; {v} vanilla); FAQ {faq_n} ({faq_na_n} N/A, {faq_rust} with Rust evidence); rulings {ru_n} ({ru_na_n} N/A, {ru_rust} with Rust evidence).",
            nv + v
        ),
        String::new(),
        "## Cards".to_owned(),
        String::new(),
        "| card | kind | Python tests | Rust |".to_owned(),
        "| --- | --- | --- | --- |".to_owned(),
    ];
    out.extend(card_rows);
    out.extend([
        String::new(),
        "## FAQ".to_owned(),
        String::new(),
        "| faq | status | Python tests / reason | Rust |".to_owned(),
        "| --- | --- | --- | --- |".to_owned(),
    ]);
    out.extend(faq_rows);
    out.extend([
        String::new(),
        "## Rulings".to_owned(),
        String::new(),
        "| ruling | status | Python tests / reason | Rust |".to_owned(),
        "| --- | --- | --- | --- |".to_owned(),
    ]);
    out.extend(ruling_rows);
    Ok(out.join("\n") + "\n")
}

#[expect(
    clippy::too_many_arguments,
    reason = "one table row with its two counters"
)]
fn entry_row(
    id: &str,
    mark: &str,
    index: &[(&str, Vec<&Tag>)],
    na: &[(String, String)],
    ev: &Evidence,
    problems: &mut Vec<String>,
    na_count: &mut usize,
    rust_count: &mut usize,
) -> String {
    let r = row(&tags_for(index, id), mark, id, ev);
    let reason = na.iter().find(|(k, _)| k == id).map(|(_, r)| r.as_str());
    if r.python.is_empty() {
        if let Some(reason) = reason {
            *na_count += 1;
            return format!("| {id} | N/A | {} | N/A |", reason.replace('|', "\\|"));
        }
        problems.push(format!("{mark} {id} has no Python test and no N/A reason"));
        return format!("| {id} | MISSING | | MISSING |");
    }
    let rust = if r.rust.is_empty() {
        problems.push(format!("{mark} {id} has no Rust evidence"));
        "MISSING".to_owned()
    } else {
        *rust_count += 1;
        rust_cell(&r.rust)
    };
    format!(
        "| {id} | tested ({}) | {} | {rust} |",
        r.python.len(),
        shown(&r.python, tick)
    )
}

/// `--write` or `--check`: the report lines and the problems.
///
/// # Errors
/// An unreadable source or an unwritable document.
pub fn run(
    repo: &Repo,
    junit: Option<&std::path::Path>,
    write: bool,
) -> Result<(Vec<String>, Vec<String>), String> {
    let ev = crate::evidence::collect(repo, junit, junit.is_none())?;
    let docs = generate(repo, &ev)?;
    let mut lines = Vec::new();
    let mut problems = ev.errors.clone();
    for (rel, text) in [(RULES_DOC, &docs.rules), (CARDS_DOC, &docs.cards)] {
        let p = repo.root.join(rel);
        if write {
            std::fs::write(&p, text).map_err(|e| format!("{}: {e}", p.display()))?;
            lines.push(format!("wrote {rel}"));
        } else {
            let on_disk = std::fs::read_to_string(&p).unwrap_or_default();
            if on_disk == *text {
                lines.push(format!("{rel} is current"));
            } else {
                problems.push(format!(
                    "{rel} differs from a regeneration: run gcg-diff traceability --write \
                     (a Python `gcg_sim.tools.traceability` run drops the Rust column)"
                ));
            }
        }
    }
    problems.extend(docs.problems);
    Ok((lines, problems))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo() -> Repo {
        Repo {
            root: std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../..")),
        }
    }

    #[test]
    fn python_columns_match_the_python_generator() {
        // Without Rust evidence every Python column still renders as
        // `gcg_sim.tools.traceability` renders it (docs/RULES_TRACEABILITY.md at d9d41af).
        let ev = Evidence::default();
        let docs = generate(&repo(), &ev).expect("generates");
        let python = std::fs::read_to_string(repo().root.join(RULES_DOC)).expect("doc");
        let py_rows: Vec<&str> = python.lines().filter(|l| l.starts_with("| 1-")).collect();
        let rs_rows: Vec<String> = docs
            .rules
            .lines()
            .filter(|l| l.starts_with("| 1-"))
            .map(|l| {
                let cut = l.rfind(" | ").expect("a Rust column");
                format!("{} |", &l[..cut])
            })
            .collect();
        if python.contains("| Rust |") {
            return;
        }
        assert_eq!(py_rows.len(), rs_rows.len());
        for (p, r) in py_rows.iter().zip(&rs_rows) {
            assert_eq!(*p, r.as_str());
        }
        assert!(docs.problems.iter().any(|p| p.starts_with("rule ")));
        assert!(docs.cards.contains("| GD01-001 | non-vanilla | tested ("));
    }

    #[test]
    fn python_keys_are_file_and_last_test_component() {
        let t = Tag {
            mark: "rule".to_owned(),
            value: "1-1-1".to_owned(),
            path: "tests/rules/test_turn.py".to_owned(),
            test: "TestX::test_y".to_owned(),
        };
        assert_eq!(python_key(&t), "test_turn.py::test_y");
        assert_eq!(
            shown(&(0..6).map(|i| i.to_string()).collect::<Vec<_>>(), tick),
            "`0`, `1`, `2`, `3` (+2)"
        );
    }
}
