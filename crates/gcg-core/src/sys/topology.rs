//! CPU topology for `--workers auto`, `timing.json` and `gcg-eval env` (docs/dev/PLAN.md A4,
//! §7.4, §11.6).
//!
//! `detect()` runs `/usr/sbin/sysctl` (absolute path, so an empty `PATH` works) for
//! `hw.nperflevels`, then lists `hw.cachelinesize` and `hw.perflevelN` for each level
//! `N < nperflevels` (one process for every `hw.perflevelN.{logicalcpu,physicalcpu,name}`
//! value, the same values `sysctl -n <key>` prints), and the `hw.optional.arm` listing. It
//! falls back to `std::thread::available_parallelism()` when `sysctl` fails. The parser is
//! unit-tested on the captured two-tier output of the M2 Pro (6 Performance + 4 Efficiency)
//! and a synthetic three-tier output in the M6 layout (2 + 4 + 6; no M6 was available).
//! Nothing here measures anything or reads a clock.

use std::process::{Command, Stdio};

/// Absolute path of the `sysctl` binary (A4).
pub const SYSCTL: &str = "/usr/sbin/sysctl";

/// The prefix of the CPU feature entries (`sysctl hw.optional.arm`).
const ARM_PREFIX: &str = "hw.optional.arm.";

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
    /// The sum of `levels[*].logical_cpus`, or `available_parallelism()` for
    /// [`TopologySource::Fallback`]. `--workers auto` uses [`Topology::fast_logical_cpus`].
    pub logical_cpus: usize,
    /// `hw.cachelinesize` (128 on the M2 Pro), if known.
    pub cache_line_bytes: Option<u32>,
    /// `hw.optional.arm.*` entries in sysctl's order, as `(name without the prefix, value)`,
    /// e.g. `("FEAT_DotProd", 1)`; `u64` so that the `caps` bit mask
    /// (`hw.optional.arm.caps`, 60 bits on the M2 Pro) fits.
    pub arm_features: Vec<(String, u64)>,
    pub source: TopologySource,
}

impl Topology {
    /// The logical CPUs of every tier except the slowest one (the efficiency tier), or all of
    /// them on a one-tier machine or without tier information. On the M2 Pro this is the 6
    /// performance cores; on the three-tier M6 layout the 2 super and 4 performance cores.
    #[must_use]
    pub fn fast_logical_cpus(&self) -> usize {
        match self.levels.split_last() {
            Some((_, faster)) if !faster.is_empty() => faster
                .iter()
                .map(|l| usize::try_from(l.logical_cpus).unwrap_or(usize::MAX))
                .sum::<usize>()
                .max(1),
            _ => self.logical_cpus.max(1),
        }
    }

    /// `--workers auto`: `max(1, min(self.fast_logical_cpus(), matches))` (PLAN §7.4). A match
    /// is the unit of work and runs sequentially, so a match that starts late on an efficiency
    /// core (about 2.1x slower per game on the M2 Pro) finishes after the others and leaves
    /// the fast cores idle. Measured on the M2 Pro (docs/dev/BENCHMARKS.md, Part 1 and
    /// Part 2): 6 workers beat 10 on every timed workload, e.g. 100 reference `standard`
    /// matches in 41.3 s against 47.9 s, and 20 `standard` matches in 133.7 s against 150.0 s.
    /// The worker count never changes any output.
    #[must_use]
    pub fn auto_workers(&self, matches: usize) -> usize {
        self.fast_logical_cpus().min(matches).max(1)
    }

    /// The topology when `sysctl` gives nothing usable: `available_parallelism()` (1 if even
    /// that is unknown), no tiers, no cache line size, no CPU features.
    #[must_use]
    fn fallback() -> Self {
        Self {
            levels: Vec::new(),
            logical_cpus: std::thread::available_parallelism().map_or(1, std::num::NonZero::get),
            cache_line_bytes: None,
            arm_features: Vec::new(),
            source: TopologySource::Fallback,
        }
    }
}

/// Detects the CPU tiers through `sysctl`, or falls back to `available_parallelism()`.
#[must_use]
pub fn detect() -> Topology {
    detect_sysctl().unwrap_or_else(Topology::fallback)
}

fn detect_sysctl() -> Option<Topology> {
    let nperflevels = run_sysctl(&["-n", "hw.nperflevels"])?.trim().to_owned();
    let levels: u32 = nperflevels.parse().ok()?;
    let mut keys = vec!["hw.cachelinesize".to_owned()];
    keys.extend((0..levels).map(|n| format!("hw.perflevel{n}")));
    let listing = run_sysctl(&keys.iter().map(String::as_str).collect::<Vec<_>>())?;
    let values = parse_listing(&listing);
    let arm_listing = run_sysctl(&["hw.optional.arm"]).unwrap_or_default();
    let query = |key: &str| {
        if key == "hw.nperflevels" {
            Some(nperflevels.clone())
        } else {
            lookup(&values, key)
        }
    };
    parse_sysctl(&query, &arm_listing)
}

/// The stdout of `/usr/sbin/sysctl <args>`, or `None` if it cannot run, exits non-zero or
/// prints something that is not UTF-8.
fn run_sysctl(args: &[&str]) -> Option<String> {
    let output = Command::new(SYSCTL)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

/// `name: value` lines of a `sysctl` listing, in order; other lines are skipped.
fn parse_listing(listing: &str) -> Vec<(String, String)> {
    listing
        .lines()
        .filter_map(|line| line.split_once(": "))
        .map(|(key, value)| (key.trim().to_owned(), value.trim().to_owned()))
        .collect()
}

/// The value of `key` in a parsed listing (the first entry, as `sysctl -n key` prints one).
fn lookup(values: &[(String, String)], key: &str) -> Option<String> {
    values
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, value)| value.clone())
}

/// The parser behind [`detect`]: `query("hw.nperflevels")` etc. returns the trimmed
/// `sysctl -n` value, `arm_listing` the `sysctl hw.optional.arm` text. `None` when a required
/// key is missing or malformed: `hw.nperflevels` (at least 1) and, for every level, a numeric
/// `logicalcpu` and `physicalcpu` and a non-empty `name`, with at least one logical CPU in
/// total. `hw.cachelinesize` is optional. CPU feature lines whose value is not a `u64` are
/// skipped.
#[must_use]
pub fn parse_sysctl(query: &dyn Fn(&str) -> Option<String>, arm_listing: &str) -> Option<Topology> {
    let count: u32 = query("hw.nperflevels")?.trim().parse().ok()?;
    if count == 0 {
        return None;
    }
    let mut levels = Vec::with_capacity(count as usize);
    for n in 0..count {
        let number = |field: &str| -> Option<u32> {
            query(&format!("hw.perflevel{n}.{field}"))?
                .trim()
                .parse()
                .ok()
        };
        let name = query(&format!("hw.perflevel{n}.name"))?.trim().to_owned();
        if name.is_empty() {
            return None;
        }
        levels.push(PerfLevel {
            name,
            logical_cpus: number("logicalcpu")?,
            physical_cpus: number("physicalcpu")?,
        });
    }
    let logical_cpus = levels
        .iter()
        .map(|level| level.logical_cpus as usize)
        .sum::<usize>();
    if logical_cpus == 0 {
        return None;
    }
    let cache_line_bytes = query("hw.cachelinesize").and_then(|v| v.trim().parse().ok());
    let arm_features = arm_listing
        .lines()
        .filter_map(|line| line.strip_prefix(ARM_PREFIX)?.split_once(": "))
        .filter_map(|(name, value)| Some((name.trim().to_owned(), value.trim().parse().ok()?)))
        .collect();
    Some(Topology {
        levels,
        logical_cpus,
        cache_line_bytes,
        arm_features,
        source: TopologySource::Sysctl,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `sysctl hw.nperflevels hw.cachelinesize hw.perflevel0 hw.perflevel1 hw.optional.arm`,
    /// captured on the Apple M2 Pro (macOS 27.2) the project is measured on.
    const M2_PRO: &str = include_str!("../../tests/fixtures/sysctl-m2pro.txt");
    /// The same listing for the Apple M6 layout (2 super + 4 performance + 6
    /// efficiency cores). Synthetic: written by hand, not captured (no M6 was available).
    const THREE_TIER: &str = include_str!("../../tests/fixtures/sysctl-3tier-synthetic.txt");

    fn parse_fixture(listing: &str) -> Option<Topology> {
        let values = parse_listing(listing);
        let query = |key: &str| lookup(&values, key);
        let arm: String = listing
            .lines()
            .filter(|line| line.starts_with(ARM_PREFIX))
            .map(|line| format!("{line}\n"))
            .collect();
        parse_sysctl(&query, &arm)
    }

    fn level(name: &str, cpus: u32) -> PerfLevel {
        PerfLevel {
            name: name.to_owned(),
            logical_cpus: cpus,
            physical_cpus: cpus,
        }
    }

    #[test]
    fn the_m2_pro_has_two_tiers() {
        let topo = parse_fixture(M2_PRO).expect("parsed");
        assert_eq!(topo.source, TopologySource::Sysctl);
        assert_eq!(
            topo.levels,
            [level("Performance", 6), level("Efficiency", 4)]
        );
        assert_eq!(topo.logical_cpus, 10);
        assert_eq!(topo.cache_line_bytes, Some(128));
        assert!(topo.arm_features.contains(&("FEAT_DotProd".to_owned(), 1)));
        assert!(topo.arm_features.contains(&("FEAT_SME".to_owned(), 0)));
        assert_eq!(
            topo.arm_features.first().map(|f| f.0.as_str()),
            Some("FEAT_FP8")
        );
        // The caps bit mask is kept whole (it needs 60 bits).
        assert_eq!(
            topo.arm_features
                .iter()
                .filter(|(name, _)| name == "caps")
                .collect::<Vec<_>>(),
            [&("caps".to_owned(), 868_632_327_146_696_703)]
        );
        assert_eq!(topo.fast_logical_cpus(), 6);
        assert_eq!(topo.auto_workers(100), 6);
        assert_eq!(topo.auto_workers(3), 3);
        assert_eq!(topo.auto_workers(0), 1);
    }

    #[test]
    fn the_m6_layout_has_three_tiers() {
        let topo = parse_fixture(THREE_TIER).expect("parsed");
        assert_eq!(
            topo.levels,
            [
                level("Super", 2),
                level("Performance", 4),
                level("Efficiency", 6)
            ]
        );
        assert_eq!(topo.logical_cpus, 12);
        assert_eq!(topo.fast_logical_cpus(), 6);
        assert_eq!(topo.auto_workers(100), 6);
        assert_eq!(
            topo.arm_features,
            [
                ("FEAT_DotProd".to_owned(), 1),
                ("FEAT_SME2".to_owned(), 1),
                ("FEAT_BF16".to_owned(), 1),
                ("caps".to_owned(), 868_632_327_146_696_703)
            ]
        );
    }

    #[test]
    fn missing_or_malformed_keys_give_none() {
        let without = |needle: &str| -> String {
            M2_PRO
                .lines()
                .filter(|line| !line.starts_with(needle))
                .map(|line| format!("{line}\n"))
                .collect()
        };
        for key in [
            "hw.nperflevels",
            "hw.perflevel1.logicalcpu:",
            "hw.perflevel0.physicalcpu:",
            "hw.perflevel1.name",
        ] {
            assert_eq!(parse_fixture(&without(key)), None, "without {key}");
        }
        let bad = M2_PRO.replace(
            "hw.perflevel0.logicalcpu: 6",
            "hw.perflevel0.logicalcpu: six",
        );
        assert_eq!(parse_fixture(&bad), None);
        let zero = M2_PRO.replace("hw.nperflevels: 2", "hw.nperflevels: 0");
        assert_eq!(parse_fixture(&zero), None);
        let empty = M2_PRO
            .replace("hw.perflevel0.logicalcpu: 6", "hw.perflevel0.logicalcpu: 0")
            .replace("hw.perflevel1.logicalcpu: 4", "hw.perflevel1.logicalcpu: 0");
        assert_eq!(parse_fixture(&empty), None);
        // The cache line size and the feature listing are optional.
        let topo = parse_fixture(&without("hw.cachelinesize")).expect("parsed");
        assert_eq!(topo.cache_line_bytes, None);
        let values = parse_listing(M2_PRO);
        let query = |key: &str| lookup(&values, key);
        let bare = parse_sysctl(&query, "").expect("parsed");
        assert!(bare.arm_features.is_empty());
        assert_eq!(bare.logical_cpus, 10);
        // Feature values that are not a u64 are skipped.
        let odd = "hw.optional.arm.FEAT_A: 1\nhw.optional.arm.big: 18446744073709551616\n\
                   hw.optional.arm.neg: -1\nhw.optional.arm.max: 18446744073709551615\n";
        let topo = parse_sysctl(&query, odd).expect("parsed");
        assert_eq!(
            topo.arm_features,
            [("FEAT_A".to_owned(), 1), ("max".to_owned(), u64::MAX)]
        );
    }

    #[test]
    fn the_fallback_uses_available_parallelism() {
        let topo = Topology::fallback();
        assert_eq!(topo.source, TopologySource::Fallback);
        assert!(topo.levels.is_empty());
        assert_eq!(
            topo.logical_cpus,
            std::thread::available_parallelism().map_or(1, std::num::NonZero::get)
        );
        assert_eq!(topo.auto_workers(1000), topo.logical_cpus);
    }

    #[test]
    fn detect_reads_this_machine() {
        let topo = detect();
        assert!(topo.logical_cpus >= 1);
        if cfg!(target_os = "macos") && run_sysctl(&["-n", "hw.nperflevels"]).is_some() {
            assert_eq!(topo.source, TopologySource::Sysctl);
            assert!(!topo.levels.is_empty());
            assert_eq!(
                topo.logical_cpus,
                std::thread::available_parallelism().map_or(1, std::num::NonZero::get)
            );
        }
    }
}
