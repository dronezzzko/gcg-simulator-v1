//! `gcg-diff schema-check [--expect-invalid] FILE...` (PLAN §9.5): re-validates written
//! `results.json` files with the runner's schema validator (`gcg_runner::reports::
//! validate_results`, `reports/results.py:48-50`), choosing the schema by the document's
//! `schema_version`. With `--expect-invalid` every file is a negative control and must fail.

use std::path::Path;

use gcg_runner::SchemaVersion;
use gcg_runner::reports::{json_from_value, validate_results};

/// The verdict on one file: `Ok(())` when it validates, else the violation.
///
/// # Errors
/// The file cannot be read or is not JSON; an error in both modes. A document without a
/// known `schema_version` is validated against 1.1.0.
pub fn validate_file(path: &Path) -> Result<Result<(), String>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let doc: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("{}: not JSON: {e}", path.display()))?;
    let version = match doc
        .get("schema_version")
        .and_then(serde_json::Value::as_str)
    {
        Some("1.1.0") => SchemaVersion::V1_1,
        Some("1.2.0") => SchemaVersion::V1_2,
        // A negative control may break schema_version itself; validate it as 1.1.0.
        _ => SchemaVersion::V1_1,
    };
    Ok(validate_results(&json_from_value(&doc), version)
        .map_err(|e| format!("{} at {:?}", e.message, e.pointer)))
}

/// Checks every file; returns the report lines and the errors.
#[must_use]
pub fn check(files: &[String], expect_invalid: bool) -> (Vec<String>, Vec<String>) {
    let mut lines = Vec::new();
    let mut errors = Vec::new();
    for f in files {
        match validate_file(Path::new(f)) {
            Err(e) => errors.push(e),
            Ok(Ok(())) if expect_invalid => {
                errors.push(format!("{f}: validates, but it is a negative control"));
            }
            Ok(Ok(())) => lines.push(format!("{f}: valid")),
            Ok(Err(v)) if expect_invalid => lines.push(format!("{f}: rejected ({v})")),
            Ok(Err(v)) => errors.push(format!("{f}: {v}")),
        }
    }
    if files.is_empty() {
        errors.push("schema-check: no files".to_owned());
    }
    (lines, errors)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn records() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../gcg-runner/tests/vectors/records")
    }

    #[test]
    fn pythons_results_validate_and_a_broken_copy_does_not() {
        let mut files: Vec<String> = std::fs::read_dir(records())
            .expect("record vectors")
            .filter_map(|e| e.ok())
            .map(|e| e.path().join("results.json"))
            .filter(|p| p.exists())
            .map(|p| p.display().to_string())
            .collect();
        files.sort();
        assert!(files.len() >= 5);
        let (lines, errors) = check(&files, false);
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(lines.len(), files.len());
        let (_, errors) = check(&files, true);
        assert_eq!(errors.len(), files.len());

        let dir = std::env::temp_dir().join(format!("gcg-diff-schema-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let mut doc: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&files[0]).expect("read")).expect("json");
        doc["ai"]["preset"] = serde_json::Value::from("bogus");
        let bad = dir.join("bad.json");
        std::fs::write(&bad, doc.to_string()).expect("write");
        let bad = bad.display().to_string();
        let (lines, errors) = check(std::slice::from_ref(&bad), true);
        assert!(errors.is_empty(), "{errors:?}");
        assert!(lines[0].contains("rejected"), "{lines:?}");
        let (_, errors) = check(std::slice::from_ref(&bad), false);
        assert_eq!(errors.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
