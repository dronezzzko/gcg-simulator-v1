//! In-house property-test harness (docs/dev/PLAN.md §9.6): case `i` of property `name` draws
//! from `SplitMix64(derive_seed(TEST_SEED, name, i))`; there is no shrinking, and a failure
//! names the property, the case index and its seed so that it can be replayed alone.

use crate::rng::SplitMix64;

/// Master seed of every property test (the ASCII bytes of `gcg-prop`).
pub const TEST_SEED: u64 = 0x6763_672D_7072_6F70;

/// The seed of case `index` of property `name`.
#[must_use]
pub fn case_seed(name: &str, index: u32) -> u64 {
    crate::derive_seed!(TEST_SEED, name, index)
}

/// Runs `property` on cases `0..cases`.
///
/// # Panics
/// On the first failing case, with the property name, case index, seed and message.
pub fn check<F>(name: &str, cases: u32, mut property: F)
where
    F: FnMut(&mut SplitMix64) -> Result<(), String>,
{
    for index in 0..cases {
        let seed = case_seed(name, index);
        let mut rng = SplitMix64::new(seed);
        if let Err(message) = property(&mut rng) {
            panic!("property {name} failed at case {index} (seed {seed:#018x}): {message}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cases_are_reproducible_and_distinct() {
        assert_eq!(case_seed("p", 0), case_seed("p", 0));
        assert_ne!(case_seed("p", 0), case_seed("p", 1));
        assert_ne!(case_seed("p", 0), case_seed("q", 0));
        let mut first = Vec::new();
        check("collect", 3, |rng| {
            first.push(rng.next_u64());
            Ok(())
        });
        let again: Vec<u64> = (0..3)
            .map(|i| SplitMix64::new(case_seed("collect", i)).next_u64())
            .collect();
        assert_eq!(first, again);
    }
}
