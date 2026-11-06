//! Corpus manifests (PLAN §8.3, §8.4): `oracle/corpus/v1/MANIFEST.json` for tier 1 (committed
//! files under `oracle/corpus/v1/tier1/`, the `tests/` subdirectory of G2 included) and
//! `oracle/corpus/v1/MANIFEST-tier2.json` for tier 2 (regenerated into `build/oracle/tier2/`).
//!
//! Format 1, written by `oracle/traces.py`: `{"format": 1, "tier": n, "root": dir,
//! "files": [{"path", "set", "sha256", "bytes"[, "records", "traces", "generator"]}],
//! "totals": {...}}`; `.ndjson` entries carry the record and trace counts and the generator
//! block of the file's first header without `max_uid`.

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::json::{Obj, as_array, as_hex, as_int, as_str, sha256_hex};

pub const MANIFEST_FORMAT: u32 = 1;
/// Tier 1 limits (PLAN A22).
pub const TIER1_MAX_BYTES: u64 = 45 * 1024 * 1024;
pub const FILE_MAX_BYTES: u64 = 8 * 1024 * 1024;

/// One manifest entry.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Entry {
    pub path: String,
    pub set: String,
    pub sha256: String,
    pub bytes: u64,
    /// Records and traces of a trace file (`.ndjson`); `None` for data files.
    pub records: Option<u64>,
    pub traces: Option<u64>,
    pub generator: Option<Value>,
}

/// A parsed manifest and where its files live.
#[derive(Clone, PartialEq, Debug)]
pub struct Manifest {
    pub tier: u8,
    pub root: PathBuf,
    pub entries: Vec<Entry>,
}

/// The repository paths the corpus commands use.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Repo {
    pub root: PathBuf,
}

impl Repo {
    /// The repository root: `GCG_REPO_ROOT`, else the first ancestor of the working directory
    /// holding `oracle/corpus/v1`, else the checkout this binary was built from.
    #[must_use]
    pub fn locate() -> Self {
        if let Some(root) = std::env::var_os("GCG_REPO_ROOT") {
            return Self { root: root.into() };
        }
        if let Ok(cwd) = std::env::current_dir() {
            for dir in cwd.ancestors() {
                if dir.join("oracle/corpus/v1").is_dir() && dir.join("Cargo.toml").is_file() {
                    return Self {
                        root: dir.to_path_buf(),
                    };
                }
            }
        }
        Self {
            root: Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."),
        }
    }

    #[must_use]
    pub fn manifest_path(&self, tier: u8) -> PathBuf {
        let name = if tier == 1 {
            "MANIFEST.json"
        } else {
            "MANIFEST-tier2.json"
        };
        self.root.join("oracle/corpus/v1").join(name)
    }

    #[must_use]
    pub fn ir_path(&self) -> PathBuf {
        self.root.join("crates/gcg-effects/ir/registry.json")
    }

    /// The SHA-256 every header's `generator.ir_sha256` must equal: the committed IR's, or
    /// `"absent"` while no IR is committed (`traces.py` writes the same).
    ///
    /// # Errors
    /// The IR exists but cannot be read.
    pub fn ir_sha256(&self) -> Result<String, String> {
        let p = self.ir_path();
        if !p.exists() {
            return Ok("absent".to_owned());
        }
        std::fs::read(&p)
            .map(|b| sha256_hex(&b))
            .map_err(|e| format!("{}: {e}", p.display()))
    }
}

/// Read a manifest.
///
/// # Errors
/// An unreadable file or a malformed entry.
pub fn load(repo: &Repo, tier: u8) -> Result<Manifest, String> {
    let path = repo.manifest_path(tier);
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let doc: Value = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut o = Obj::new(&doc, "manifest")?;
    let format: u32 = as_int(o.req("format")?, "manifest.format")?;
    if format != MANIFEST_FORMAT {
        return Err(format!(
            "manifest format {format}; this gcg-diff reads {MANIFEST_FORMAT}"
        ));
    }
    let t: u8 = as_int(o.req("tier")?, "manifest.tier")?;
    if t != tier {
        return Err(format!("{} is the tier {t} manifest", path.display()));
    }
    let root = repo.root.join(as_str(o.req("root")?, "manifest.root")?);
    let mut entries = Vec::new();
    for e in as_array(o.req("files")?, "manifest.files")? {
        entries.push(entry(e)?);
    }
    o.req("totals")?;
    o.finish()?;
    let mut sorted = entries.clone();
    sorted.sort_by(|a, b| a.path.cmp(&b.path));
    if sorted != entries {
        return Err("manifest files are not sorted by path".to_owned());
    }
    Ok(Manifest {
        tier,
        root,
        entries,
    })
}

fn entry(v: &Value) -> Result<Entry, String> {
    let mut o = Obj::new(v, "manifest entry")?;
    let path = as_str(o.req("path")?, "entry.path")?.to_owned();
    let set = as_str(o.req("set")?, "entry.set")?.to_owned();
    let sha256 = as_hex(o.req("sha256")?, 64, "entry.sha256")?.to_owned();
    let bytes = as_int(o.req("bytes")?, "entry.bytes")?;
    let records = o
        .opt("records")
        .map(|r| as_int(r, "entry.records"))
        .transpose()?;
    let traces = o
        .opt("traces")
        .map(|r| as_int(r, "entry.traces"))
        .transpose()?;
    let generator = o.opt("generator").cloned();
    o.finish()?;
    if path.ends_with(".ndjson") != records.is_some() || records.is_some() != traces.is_some() {
        return Err(format!(
            "{path}: trace files and only they carry records and traces"
        ));
    }
    if path.split('/').next() != Some(set.as_str()) {
        return Err(format!(
            "{path}: set {set:?} is not its first path component"
        ));
    }
    Ok(Entry {
        path,
        set,
        sha256,
        bytes,
        records,
        traces,
        generator,
    })
}

/// Every file under `root` (sorted, relative, `/`-separated), hidden files excluded.
///
/// # Errors
/// An unreadable directory.
pub fn files_under(root: &Path) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let rd = std::fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        for item in rd {
            let item = item.map_err(|e| format!("{}: {e}", dir.display()))?;
            let p = item.path();
            let name = item.file_name();
            if name.to_string_lossy().starts_with('.') {
                continue;
            }
            if p.is_dir() {
                stack.push(p);
            } else if let Ok(rel) = p.strip_prefix(root) {
                let parts: Vec<String> = rel
                    .iter()
                    .map(|c| c.to_string_lossy().into_owned())
                    .collect();
                out.push(parts.join("/"));
            }
        }
    }
    out.sort();
    Ok(out)
}

/// What a manifest check found.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct CheckReport {
    pub errors: Vec<String>,
    pub files: u64,
    pub bytes: u64,
}

/// Check every file against its entry: presence, size, SHA-256, record and trace counts, the
/// generator block, the tier-1 size limits, and that no file is unlisted. Trace contents are
/// checked separately by decoding (`corpus --parse-only`).
#[must_use]
pub fn check(m: &Manifest) -> CheckReport {
    let mut r = CheckReport::default();
    let on_disk = match files_under(&m.root) {
        Ok(f) => f,
        Err(e) => {
            r.errors.push(e);
            return r;
        }
    };
    for f in &on_disk {
        if !m.entries.iter().any(|e| &e.path == f) {
            r.errors
                .push(format!("{f}: not listed in the tier {} manifest", m.tier));
        }
    }
    for e in &m.entries {
        let p = m.root.join(&e.path);
        let data = match std::fs::read(&p) {
            Ok(d) => d,
            Err(err) => {
                r.errors.push(format!("{}: {err}", e.path));
                continue;
            }
        };
        r.files += 1;
        r.bytes += data.len() as u64;
        if data.len() as u64 != e.bytes {
            r.errors.push(format!(
                "{}: {} bytes, manifest says {}",
                e.path,
                data.len(),
                e.bytes
            ));
        }
        let sha = sha256_hex(&data);
        if sha != e.sha256 {
            r.errors.push(format!(
                "{}: sha256 {sha}, manifest says {}",
                e.path, e.sha256
            ));
        }
        if m.tier == 1 && data.len() as u64 > FILE_MAX_BYTES {
            r.errors.push(format!(
                "{}: {} bytes, over the {FILE_MAX_BYTES}-byte file limit",
                e.path,
                data.len()
            ));
        }
        if let (Some(records), Some(traces)) = (e.records, e.traces) {
            let lines = data.iter().filter(|&&b| b == b'\n').count() as u64;
            let headers = data
                .split(|&b| b == b'\n')
                .filter(|l| contains(l, b"\"t\":\"header\""))
                .count() as u64;
            if lines != records || headers != traces {
                r.errors.push(format!(
                    "{}: {lines} records and {headers} traces, manifest says {records} and {traces}",
                    e.path
                ));
            }
            if let Some(g) = &e.generator
                && let Some(first) = first_generator(&data)
                && *g != first
            {
                r.errors.push(format!(
                    "{}: the generator block differs from the first header's",
                    e.path
                ));
            }
        }
    }
    if m.tier == 1 && r.bytes > TIER1_MAX_BYTES {
        r.errors.push(format!(
            "tier 1 holds {} bytes, over the {TIER1_MAX_BYTES}-byte limit (A22)",
            r.bytes
        ));
    }
    r
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

fn first_generator(data: &[u8]) -> Option<Value> {
    let line = data
        .split(|&b| b == b'\n')
        .find(|l| contains(l, b"\"t\":\"header\""))?;
    let v: Value = serde_json::from_slice(line).ok()?;
    let mut g = v.get("generator")?.clone();
    g.as_object_mut()?.remove("max_uid");
    Some(g)
}
