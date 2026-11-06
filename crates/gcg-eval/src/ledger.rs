//! Seed roles and the seed ledger (docs/dev/PLAN.md A14, §12.5, §12.6).
//!
//! A run under `--role R` must use a master seed inside R's range ([`check_master`]), and the
//! roles that steer the search (`tuning`, `dev`, `confirm`) may not play held-out pairs
//! ([`check_pairs`]). Every master a role run used is appended to
//! `docs/dev/eval/seed-ledger.json` with the run's configuration ([`record`]), so the
//! reviewer can check that tuning and evaluation never shared a seed.
//!
//! Ledger format: `{"version": 1, "entries": [{"role", "master", "command", "config"}, ...]}`
//! in run order, written atomically; an entry identical to an existing one (a rerun) is not
//! repeated.

use std::path::Path;

use gcg_core::pyjson::{DumpOptions, Json, dumps};
use gcg_data::py_path_str;
use gcg_runner::reports::{json_from_value, write_atomic};

use crate::SeedRole;

/// The default ledger path (PLAN A14).
pub const DEFAULT_LEDGER: &str = "docs/dev/eval/seed-ledger.json";
/// The ledger format version.
pub const LEDGER_VERSION: u32 = 1;

/// Whether `role` may play held-out pairs (PLAN §12.5: not under tuning, dev or confirm).
#[must_use]
pub const fn may_play_held_out(role: SeedRole) -> bool {
    matches!(role, SeedRole::Eval | SeedRole::Review)
}

/// `--seed` must lie in the role's range (PLAN A14).
///
/// # Errors
/// `--seed {master} is outside the {role} range [{first}, {last}]`.
pub fn check_master(role: SeedRole, master: u64) -> Result<(), String> {
    if role.contains(master) {
        return Ok(());
    }
    let (first, last) = role.bounds();
    Err(format!(
        "--seed {master} is outside the {} range [{first}, {last}] (PLAN A14)",
        role.as_str()
    ))
}

/// Held-out pairs are refused under the roles that steer the search (PLAN §12.5).
///
/// # Errors
/// The first held-out pair, by name.
pub fn check_pairs<'a>(
    role: SeedRole,
    held_out: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Result<(), String> {
    if may_play_held_out(role) {
        return Ok(());
    }
    match held_out.into_iter().next() {
        None => Ok(()),
        Some((a, b)) => Err(format!(
            "--role {} may not play the held-out pair {a}:{b} (PLAN §12.5)",
            role.as_str()
        )),
    }
}

/// One ledger entry.
#[derive(Clone, PartialEq, Debug)]
pub struct Entry {
    pub role: SeedRole,
    pub master: u64,
    pub command: &'static str,
    pub config: Json,
}

impl Entry {
    #[must_use]
    pub fn to_json(&self) -> Json {
        let mut o = Json::object();
        o.set("role", self.role.as_str());
        o.set("master", self.master);
        o.set("command", self.command);
        o.set("config", self.config.clone());
        o
    }
}

/// The entries of an existing ledger, checked against the role ranges.
///
/// # Errors
/// The file is not a ledger, or an entry's master lies outside its role's range.
pub fn read(path: &Path) -> Result<Vec<Json>, String> {
    let shown = py_path_str(path);
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("cannot read the seed ledger {shown}: {e}")),
    };
    let v: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("{shown}: not JSON: {e}"))?;
    let entries = v
        .get("entries")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| format!("{shown}: not a seed ledger (no \"entries\" list)"))?;
    for (i, e) in entries.iter().enumerate() {
        let role = e
            .get("role")
            .and_then(serde_json::Value::as_str)
            .and_then(SeedRole::from_value);
        let master = e.get("master").and_then(serde_json::Value::as_u64);
        match (role, master) {
            (Some(r), Some(m)) if r.contains(m) => {}
            _ => {
                return Err(format!(
                    "{shown}: entry {i} has no valid role and in-range master"
                ));
            }
        }
    }
    Ok(entries.iter().map(json_from_value).collect())
}

/// Appends `entry` unless an identical one is already there; writes atomically.
///
/// # Errors
/// The ledger cannot be read or written.
pub fn record(path: &Path, entry: &Entry) -> Result<(), String> {
    let mut entries = read(path)?;
    let new = entry.to_json();
    if entries.contains(&new) {
        return Ok(());
    }
    entries.push(new);
    let mut doc = Json::object();
    doc.set("version", LEDGER_VERSION);
    doc.set("entries", Json::Array(entries));
    let opts = DumpOptions {
        indent: Some(1),
        sort_keys: false,
        compact: false,
        ensure_ascii: true,
    };
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("cannot create {}: {e}", py_path_str(dir)))?;
    }
    write_atomic(path, &(dumps(&doc, opts) + "\n"))
        .map_err(|e| format!("cannot write the seed ledger: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges_are_enforced() {
        assert_eq!(check_master(SeedRole::Eval, 90_000_001), Ok(()));
        assert_eq!(
            check_master(SeedRole::Tuning, 90_000_001).err().as_deref(),
            Some("--seed 90000001 is outside the tuning range [10000000, 19999999] (PLAN A14)")
        );
        assert!(check_pairs(SeedRole::Dev, [("a", "b")]).is_err());
        assert!(check_pairs(SeedRole::Dev, []).is_ok());
        assert!(check_pairs(SeedRole::Review, [("a", "b")]).is_ok());
        for role in SeedRole::ALL {
            let (first, last) = role.bounds();
            assert!(check_master(role, first).is_ok() && check_master(role, last).is_ok());
            assert!(
                check_master(role, first - 1).is_err() && check_master(role, last + 1).is_err()
            );
        }
    }

    #[test]
    fn the_ledger_appends_once_and_rejects_bad_entries() {
        let dir = std::env::temp_dir().join(format!("gcg-eval-ledger-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("nested").join("ledger.json");
        let mut config = Json::object();
        config.set("games", 400_u32);
        let entry = Entry {
            role: SeedRole::Dev,
            master: 20_000_001,
            command: "head-to-head",
            config,
        };
        assert_eq!(record(&path, &entry), Ok(()));
        assert_eq!(record(&path, &entry), Ok(()));
        assert_eq!(read(&path).map(|e| e.len()), Ok(1));
        let other = Entry {
            master: 20_000_002,
            ..entry
        };
        assert_eq!(record(&path, &other), Ok(()));
        assert_eq!(read(&path).map(|e| e.len()), Ok(2));
        std::fs::write(
            &path,
            "{\"version\": 1, \"entries\": [{\"role\": \"eval\", \"master\": 5}]}",
        )
        .unwrap_or_default();
        assert!(read(&path).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
