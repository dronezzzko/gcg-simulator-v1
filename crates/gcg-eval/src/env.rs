//! `gcg-eval env` (docs/port/PLAN.md §7.4, §11.6): the build (package, `code_digest`, target,
//! the CPU features it was compiled for), the CPU tiers from `sysctl` (`hw.perflevelN`), the
//! `hw.optional.arm.*` listing, and the features `is_aarch64_feature_detected!` reports, as
//! JSON. `scripts/compare_python_vs_rust.sh` reads its `code_digest` (`need_rust`). Nothing
//! here measures anything.

use gcg_core::pyjson::Json;
use gcg_core::sys::topology::{Topology, TopologySource, detect};

use crate::build_info::{CODE_DIGEST, PACKAGE_VERSION};

/// The `std::arch` feature names `env` asks about (the ones stable Rust detects on aarch64).
pub const AARCH64_FEATURES: [&str; 28] = [
    "neon", "pmull", "fp16", "fhm", "fcma", "dotprod", "rdm", "lse", "lse2", "crc", "aes", "sha2",
    "sha3", "sm4", "rcpc", "rcpc2", "frintts", "i8mm", "bf16", "dpb", "dpb2", "dit", "flagm",
    "ssbs", "sb", "paca", "pacg", "bti",
];

/// The features the binary was compiled for (`-C target-cpu`): [`AARCH64_FEATURES`] without
/// `pmull`, which rustc folds into its `aes` target feature.
#[must_use]
pub fn compiled_features() -> Vec<&'static str> {
    macro_rules! enabled {
        ($($f:tt),*) => {{
            let mut out = Vec::new();
            $( if cfg!(target_feature = $f) { out.push($f); } )*
            out
        }};
    }
    enabled!(
        "neon", "fp16", "fhm", "fcma", "dotprod", "rdm", "lse", "lse2", "crc", "aes", "sha2",
        "sha3", "sm4", "rcpc", "rcpc2", "frintts", "i8mm", "bf16", "dpb", "dpb2", "dit", "flagm",
        "ssbs", "sb", "paca", "pacg", "bti"
    )
}

/// `std::arch::is_aarch64_feature_detected!` for each of [`AARCH64_FEATURES`]; empty off
/// aarch64.
#[must_use]
pub fn detected_features() -> Vec<(&'static str, bool)> {
    #[cfg(target_arch = "aarch64")]
    {
        macro_rules! detect {
            ($($f:tt),*) => { vec![$( ($f, std::arch::is_aarch64_feature_detected!($f)) ),*] };
        }
        detect!(
            "neon", "pmull", "fp16", "fhm", "fcma", "dotprod", "rdm", "lse", "lse2", "crc", "aes",
            "sha2", "sha3", "sm4", "rcpc", "rcpc2", "frintts", "i8mm", "bf16", "dpb", "dpb2",
            "dit", "flagm", "ssbs", "sb", "paca", "pacg", "bti"
        )
    }
    #[cfg(not(target_arch = "aarch64"))]
    {
        Vec::new()
    }
}

/// The topology block (`timing.json`'s layout, PLAN §7.4) plus `auto_workers`.
#[must_use]
pub fn topology_json(t: &Topology) -> Json {
    let mut o = Json::object();
    o.set(
        "source",
        match t.source {
            TopologySource::Sysctl => "sysctl",
            TopologySource::Fallback => "fallback",
        },
    );
    o.set("logical_cpus", t.logical_cpus);
    o.set("cache_line_bytes", t.cache_line_bytes);
    o.set(
        "levels",
        Json::Array(
            t.levels
                .iter()
                .map(|l| {
                    let mut level = Json::object();
                    level.set("name", l.name.as_str());
                    level.set("logical_cpus", l.logical_cpus);
                    level.set("physical_cpus", l.physical_cpus);
                    level
                })
                .collect(),
        ),
    );
    o.set("auto_workers", t.auto_workers(usize::MAX));
    o
}

/// The whole `env` document.
#[must_use]
pub fn env_json(t: &Topology) -> Json {
    let mut doc = Json::object();
    doc.set("package", PACKAGE_VERSION);
    doc.set("code_digest", CODE_DIGEST);
    let mut target = Json::object();
    target.set("os", std::env::consts::OS);
    target.set("arch", std::env::consts::ARCH);
    target.set("debug_assertions", cfg!(debug_assertions));
    target.set("compiled_features", compiled_features());
    doc.set("build", target);
    doc.set("topology", topology_json(t));
    let mut features = Json::object();
    let mut sysctl = Json::object();
    for (name, value) in &t.arm_features {
        sysctl.set(name.as_str(), *value);
    }
    features.set("sysctl", sysctl);
    let mut detected = Json::object();
    for (name, on) in detected_features() {
        detected.set(name, on);
    }
    features.set("detected", detected);
    doc.set("cpu_features", features);
    doc
}

/// `env_json` of this machine.
#[must_use]
pub fn current() -> Json {
    env_json(&detect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use gcg_core::sys::topology::PerfLevel;

    #[test]
    fn the_document_names_the_digest_and_the_tiers() {
        let t = Topology {
            levels: vec![
                PerfLevel {
                    name: "Performance".to_owned(),
                    logical_cpus: 6,
                    physical_cpus: 6,
                },
                PerfLevel {
                    name: "Efficiency".to_owned(),
                    logical_cpus: 4,
                    physical_cpus: 4,
                },
            ],
            logical_cpus: 10,
            cache_line_bytes: Some(128),
            arm_features: vec![("FEAT_DotProd".to_owned(), 1)],
            source: TopologySource::Sysctl,
        };
        let doc = env_json(&t);
        assert_eq!(doc.get("code_digest"), Some(&Json::from(CODE_DIGEST)));
        assert_eq!(CODE_DIGEST.len(), 64);
        let topo = doc.get("topology").cloned().unwrap_or(Json::Null);
        assert_eq!(topo.get("auto_workers"), Some(&Json::from(10_usize)));
        assert_eq!(
            doc.get("cpu_features")
                .and_then(|f| f.get("sysctl"))
                .and_then(|s| s.get("FEAT_DotProd")),
            Some(&Json::from(1_u64))
        );
        if cfg!(target_arch = "aarch64") {
            let detected = detected_features();
            assert_eq!(
                detected.iter().map(|(n, _)| *n).collect::<Vec<_>>(),
                AARCH64_FEATURES
            );
            assert!(detected.iter().any(|(n, on)| *n == "neon" && *on));
            assert!(
                compiled_features()
                    .iter()
                    .all(|f| AARCH64_FEATURES.contains(f))
            );
        }
    }
}
