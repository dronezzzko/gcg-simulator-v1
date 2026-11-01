//! `gcg-diff weaknesses --check docs/port/WEAKNESSES.md` (PLAN §12.2, the W-id partition check).
//!
//! Checks, over WEAKNESSES.md and `docs/port/PLAN.md`:
//!
//! - the summary table lists every id W-01 to W-164 exactly once, in order, and every id has
//!   exactly one `### W-NN — ...` entry with a `- **Status:**` line;
//! - the §12.2 package table partitions the 164 ids: each id in exactly one package row;
//! - class S (packages P6-A … P6-H) equals the set of summary rows whose resolution type
//!   contains "AI (strengthened search)", with or without a qualifier inside the parentheses;
//! - every scenario-shaped item (verification column contains "scenario") has a scenario test
//!   id in Appendix A (`scenario test W-NN`);
//! - with `--require-resolved` (from M15 on), every entry's status is other than `open` and the
//!   entry records `Evidence:`.

pub const FIRST: u32 = 1;
pub const LAST: u32 = 164;
/// The class-S marker of a resolution type: "AI (strengthened search)", also with a
/// qualifier inside the parentheses ("AI (strengthened search, re-measure)", W-26, W-134),
/// which is how §12.1 counts its 43 items.
const AI_RESOLUTION: &str = "AI (strengthened search";

/// Split a Markdown table row into trimmed cells (`\|` is an escaped pipe inside a cell).
#[must_use]
pub fn cells(row: &str) -> Vec<String> {
    let body = row.trim().trim_start_matches('|').trim_end_matches('|');
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut chars = body.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if chars.peek() == Some(&'|') => {
                cur.push('|');
                chars.next();
            }
            '|' => out.push(std::mem::take(&mut cur).trim().to_owned()),
            c => cur.push(c),
        }
    }
    out.push(cur.trim().to_owned());
    out
}

/// Every `W-NN` id in `text`, in order of appearance.
#[must_use]
pub fn ids_in(text: &str) -> Vec<u32> {
    let mut out = Vec::new();
    let b = text.as_bytes();
    let mut i = 0;
    while let Some(p) = text[i..].find("W-") {
        let at = i + p;
        let digits: String = text[at + 2..]
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        let before_ok = at == 0 || !b[at - 1].is_ascii_alphanumeric();
        if before_ok
            && !digits.is_empty()
            && let Ok(n) = digits.parse()
        {
            out.push(n);
        }
        i = at + 2;
    }
    out
}

fn wid(n: u32) -> String {
    format!("W-{n:02}")
}

/// One summary row.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Row {
    pub id: u32,
    pub resolution: String,
    pub verification: String,
}

fn summary_rows(weak: &str) -> Vec<Row> {
    let mut out = Vec::new();
    let mut inside = false;
    for line in weak.lines() {
        if line.starts_with("## ") {
            inside = line.trim() == "## Summary";
            continue;
        }
        if inside && line.starts_with("| W-") {
            let c = cells(line);
            if let (Some(id), true) = (ids_in(&c[0]).first().copied(), c.len() >= 5) {
                out.push(Row {
                    id,
                    resolution: c[3].clone(),
                    verification: c[4].clone(),
                });
            }
        }
    }
    out
}

/// Entries: `(id, status line, entry text)`.
fn entries(weak: &str) -> Vec<(u32, Option<String>, String)> {
    let mut out: Vec<(u32, Option<String>, String)> = Vec::new();
    for line in weak.lines() {
        if let Some(rest) = line.strip_prefix("### W-") {
            let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
            if let Ok(n) = digits.parse() {
                out.push((n, None, String::new()));
                continue;
            }
        }
        if line.starts_with("## ") || line.starts_with("### ") {
            out.push((0, None, String::new()));
            continue;
        }
        if let Some(last) = out.last_mut() {
            if let Some(status) = line.trim_start().strip_prefix("- **Status:**") {
                last.1 = Some(status.trim().to_owned());
            }
            last.2.push_str(line);
            last.2.push('\n');
        }
    }
    out.retain(|(n, _, _)| *n != 0);
    out
}

/// §12.2 package rows: `(package, class, ids)`.
fn packages(plan: &str) -> Vec<(String, String, Vec<u32>)> {
    let mut out = Vec::new();
    let mut inside = false;
    for line in plan.lines() {
        if line.starts_with("### ") {
            inside = line.starts_with("### 12.2 ");
            continue;
        }
        if inside && line.starts_with("| P6-") {
            let c = cells(line);
            if c.len() >= 3 {
                out.push((c[0].clone(), c[1].clone(), ids_in(&c[2])));
            }
        }
    }
    out
}

fn scenario_tests(plan: &str) -> Vec<u32> {
    let mut out = Vec::new();
    let mut inside = false;
    for line in plan.lines() {
        if line.starts_with("## ") {
            inside = line.starts_with("## Appendix A");
            continue;
        }
        if inside
            && line.starts_with("| T-")
            && let Some(i) = line.find("scenario test W-")
        {
            out.extend(ids_in(&line[i..]).first());
        }
    }
    out
}

/// Every problem found; empty when the partition holds.
#[must_use]
pub fn check(weak: &str, plan: &str, require_resolved: bool) -> Vec<String> {
    let mut errors = Vec::new();
    let rows = summary_rows(weak);
    let want: Vec<u32> = (FIRST..=LAST).collect();
    let got: Vec<u32> = rows.iter().map(|r| r.id).collect();
    if got != want {
        let missing: Vec<String> = want
            .iter()
            .filter(|w| !got.contains(w))
            .map(|&w| wid(w))
            .collect();
        let dup: Vec<String> = got
            .iter()
            .enumerate()
            .filter(|(i, w)| got[..*i].contains(w))
            .map(|(_, &w)| wid(w))
            .collect();
        errors.push(format!(
            "summary table: {} rows; missing {missing:?}, duplicated {dup:?}{}",
            got.len(),
            if missing.is_empty() && dup.is_empty() {
                ", out of order"
            } else {
                ""
            }
        ));
    }
    let ents = entries(weak);
    for w in &want {
        let n = ents.iter().filter(|(id, _, _)| id == w).count();
        if n != 1 {
            errors.push(format!("{}: {n} entries, expected 1", wid(*w)));
        }
    }
    for (id, status, text) in &ents {
        match status {
            None => errors.push(format!("{}: no - **Status:** line", wid(*id))),
            Some(s)
                if require_resolved && (s.starts_with("open") || !text.contains("Evidence:")) =>
            {
                errors.push(format!(
                    "{}: status {s:?} without resolution evidence",
                    wid(*id)
                ));
            }
            Some(_) => {}
        }
    }
    let pkgs = packages(plan);
    if pkgs.is_empty() {
        errors.push("PLAN.md §12.2 has no package table".to_owned());
    }
    for w in &want {
        let owners: Vec<&str> = pkgs
            .iter()
            .filter(|(_, _, ids)| ids.contains(w))
            .map(|(p, _, _)| p.as_str())
            .collect();
        if owners.len() != 1 {
            errors.push(format!(
                "{}: in {} §12.2 packages {owners:?}, expected exactly 1",
                wid(*w),
                owners.len()
            ));
        }
    }
    let class_s: Vec<u32> = {
        let mut v: Vec<u32> = pkgs
            .iter()
            .filter(|(_, class, _)| class == "S")
            .flat_map(|(_, _, ids)| ids.iter().copied())
            .collect();
        v.sort();
        v
    };
    let ai_rows: Vec<u32> = rows
        .iter()
        .filter(|r| r.resolution.contains(AI_RESOLUTION))
        .map(|r| r.id)
        .collect();
    if class_s != ai_rows {
        let only_s: Vec<String> = class_s
            .iter()
            .filter(|w| !ai_rows.contains(w))
            .map(|&w| wid(w))
            .collect();
        let only_ai: Vec<String> = ai_rows
            .iter()
            .filter(|w| !class_s.contains(w))
            .map(|&w| wid(w))
            .collect();
        errors.push(format!(
            "class S differs from the AI rows: in class S only {only_s:?}, AI rows outside class S {only_ai:?}"
        ));
    }
    let tests = scenario_tests(plan);
    for r in rows.iter().filter(|r| r.verification.contains("scenario")) {
        if !tests.contains(&r.id) {
            errors.push(format!(
                "{}: scenario-shaped but no scenario test id in Appendix A",
                wid(r.id)
            ));
        }
    }
    errors
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cells_keep_escaped_pipes() {
        assert_eq!(cells("| a | `x\\|y` | c |"), ["a", "`x|y`", "c"]);
        assert_eq!(ids_in("W-01, W-164 and XW-3"), [1, 164]);
    }
}
