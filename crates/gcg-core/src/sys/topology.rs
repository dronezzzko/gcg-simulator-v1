//! CPU topology for `--workers auto`, `timing.json` and `gcg-eval env` (docs/port/PLAN.md A4,
//! §7.4, §11.6).
//!
//! `detect()` runs `/usr/sbin/sysctl -n` (absolute path, so an empty `PATH` works) for
//! `hw.nperflevels`, then `hw.perflevelN.{logicalcpu,physicalcpu,name}` for each level
//! `N < nperflevels`, `hw.cachelinesize` and the `hw.optional.arm` listing, and falls back to
//! `std::thread::available_parallelism()` when `sysctl` fails. The parser is unit-tested on
//! captured two-tier (M2 Pro: 6 Performance + 4 Efficiency) and three-tier (M6: 2 + 4 + 6)
//! outputs. Nothing here measures anything or reads a clock. The functions land with workstream
//! E; their frozen signatures are in docs/port/contracts.md.

/// Absolute path of the `sysctl` binary (A4).
pub const SYSCTL: &str = "/usr/sbin/sysctl";

/// One `hw.perflevelN` level; level 0 is the fastest.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct PerfLevel {
    /// `hw.perflevelN.name`, e.g. `Performance`, `Efficiency`.
    pub name: String,
    /// `hw.perflevelN.logicalcpu`.
    pub logical_cpus: u32,
    /// `hw.perflevelN.physicalcpu`.
    pub physical_cpus: u32,
}

/// Where a [`Topology`] came from.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum TopologySource {
    /// Parsed from `sysctl` output.
    Sysctl,
    /// `sysctl` failed: only `available_parallelism()` is known (`levels` is empty).
    Fallback,
}

/// The machine's CPU tiers, as recorded in `timing.json` and printed by `gcg-eval env`.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Topology {
    /// `hw.perflevel0..N-1`, in sysctl order.
    pub levels: Vec<PerfLevel>,
    /// The sum of `levels[*].logical_cpus` (what `--workers auto` uses before the match-count
    /// cap), or `available_parallelism()` for [`TopologySource::Fallback`].
    pub logical_cpus: usize,
    /// `hw.cachelinesize` (128 on the M2 Pro), if known.
    pub cache_line_bytes: Option<u32>,
    /// `hw.optional.arm.*` entries in sysctl's order, as `(name without the prefix, value)`,
    /// e.g. `("FEAT_DotProd", 1)`.
    pub arm_features: Vec<(String, u32)>,
    pub source: TopologySource,
}
