//! `gcg-diff tests-json --check` (PLAN §9.10): `docs/dev/tests.json` is a JSON array of
//! objects with exactly the keys `id`, `name` and `status`; ids are unique `T-NNN`; a status is
//! `passing`, `failing` or `not-started`; and the id list equals Appendix A of
//! `docs/dev/PLAN.md`, in order.

use serde_json::Value;

pub const STATUSES: [&str; 3] = ["passing", "failing", "not-started"];

/// The `T-NNN` ids of the Appendix A table, in table order.
#[must_use]
pub fn appendix_ids(plan: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut inside = false;
    for line in plan.lines() {
        if line.starts_with("## ") {
            inside = line.starts_with("## Appendix A");
            continue;
        }
        if !inside {
            continue;
        }
        if let Some(rest) = line.strip_prefix("| T-") {
            let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
            if digits.len() == 3 {
                out.push(format!("T-{digits}"));
            }
        }
    }
    out
}

fn is_id(s: &str) -> bool {
    s.len() == 5 && s.starts_with("T-") && s[2..].bytes().all(|b| b.is_ascii_digit())
}

/// Every problem of `tests_json` against `plan`; empty when the file is valid.
#[must_use]
pub fn check(tests_json: &str, plan: &str) -> Vec<String> {
    let mut errors = Vec::new();
    let doc: Value = match serde_json::from_str(tests_json) {
        Ok(v) => v,
        Err(e) => return vec![format!("tests.json is not JSON: {e}")],
    };
    let Some(items) = doc.as_array() else {
        return vec!["tests.json is not a JSON array".to_owned()];
    };
    let mut ids: Vec<String> = Vec::new();
    for (i, item) in items.iter().enumerate() {
        let Some(obj) = item.as_object() else {
            errors.push(format!("entry {i}: not an object"));
            continue;
        };
        let mut keys: Vec<&str> = obj.keys().map(String::as_str).collect();
        keys.sort();
        if keys != ["id", "name", "status"] {
            errors.push(format!(
                "entry {i}: keys {keys:?}, expected exactly id, name, status"
            ));
        }
        let id = obj.get("id").and_then(Value::as_str).unwrap_or("");
        if !is_id(id) {
            errors.push(format!("entry {i}: id {id:?} is not T-NNN"));
        }
        if ids.iter().any(|x| x == id) {
            errors.push(format!("entry {i}: duplicate id {id}"));
        }
        ids.push(id.to_owned());
        if obj
            .get("name")
            .and_then(Value::as_str)
            .is_none_or(str::is_empty)
        {
            errors.push(format!("{id}: name is not a non-empty string"));
        }
        let status = obj.get("status").and_then(Value::as_str).unwrap_or("");
        if !STATUSES.contains(&status) {
            errors.push(format!(
                "{id}: status {status:?} is not one of {STATUSES:?}"
            ));
        }
    }
    let appendix = appendix_ids(plan);
    if appendix.is_empty() {
        errors.push("PLAN.md has no Appendix A test register".to_owned());
    } else if appendix != ids {
        let missing: Vec<&String> = appendix.iter().filter(|a| !ids.contains(a)).collect();
        let extra: Vec<&String> = ids.iter().filter(|a| !appendix.contains(a)).collect();
        if missing.is_empty() && extra.is_empty() {
            errors.push("tests.json lists the Appendix A ids in a different order".to_owned());
        } else {
            errors.push(format!(
                "ids differ from PLAN.md Appendix A: missing {missing:?}, extra {extra:?}"
            ));
        }
    }
    errors
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAN: &str = "## Appendix A. Test register\n\n| Id | M | C |\n| --- | --- | --- |\n| T-001 | M1 | x |\n| T-002 | M2 | y |\n\n## Review log\n| T-999 | not in the appendix |\n";

    #[test]
    fn valid_register_passes_and_each_defect_is_reported() {
        let good = r#"[{"id":"T-001","name":"a","status":"passing"},{"id":"T-002","name":"b","status":"not-started"}]"#;
        assert_eq!(appendix_ids(PLAN), ["T-001", "T-002"]);
        assert!(check(good, PLAN).is_empty());
        let extra_key = r#"[{"id":"T-001","name":"a","status":"passing","x":1},{"id":"T-002","name":"b","status":"failing"}]"#;
        assert_eq!(check(extra_key, PLAN).len(), 1);
        let bad_status = r#"[{"id":"T-001","name":"a","status":"done"},{"id":"T-002","name":"b","status":"failing"}]"#;
        assert_eq!(check(bad_status, PLAN).len(), 1);
        let missing = r#"[{"id":"T-001","name":"a","status":"passing"}]"#;
        assert_eq!(check(missing, PLAN).len(), 1);
        let dup = r#"[{"id":"T-001","name":"a","status":"passing"},{"id":"T-001","name":"b","status":"failing"}]"#;
        assert!(check(dup, PLAN).iter().any(|e| e.contains("duplicate")));
    }
}
