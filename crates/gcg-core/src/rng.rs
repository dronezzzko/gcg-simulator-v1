//! Deterministic randomness, bit-identical to `gcg_sim.rng` (Python reference).

use sha2::{Digest, Sha256};

use crate::pyrepr::{PyRepr, repr_tuple};

const GOLDEN_GAMMA: u64 = 0x9E37_79B9_7F4A_7C15;

/// SplitMix64 whose whole state is one 64-bit integer, as `gcg_sim.rng.SplitMix64`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SplitMix64 {
    pub state: u64,
}

impl SplitMix64 {
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    pub const fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(GOLDEN_GAMMA);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform integer in `[0, n)` by unbiased rejection sampling, as Python's `randrange`.
    ///
    /// # Panics
    /// If `n == 0` (Python raises `ValueError`).
    pub fn randrange(&mut self, n: u64) -> u64 {
        assert!(n > 0, "randrange() requires n > 0");
        let limit: u128 = (1u128 << 64) - ((1u128 << 64) % u128::from(n));
        loop {
            let x = self.next_u64();
            if u128::from(x) < limit {
                return x % n;
            }
        }
    }

    /// Uniform index in `[0, n)` for container sizes.
    ///
    /// # Panics
    /// If `n == 0`.
    pub fn randrange_usize(&mut self, n: usize) -> usize {
        let n = u64::try_from(n).expect("usize fits in u64");
        usize::try_from(self.randrange(n)).expect("value below a usize bound")
    }

    /// Uniform float in `[0, 1)` with 53 random bits.
    #[allow(clippy::cast_precision_loss)]
    pub const fn random(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    /// Fisher-Yates from the end, consuming one `randrange(i + 1)` per position.
    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = self.randrange_usize(i + 1);
            items.swap(i, j);
        }
    }
}

/// One element of a `derive_seed` label path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeedPart<'a> {
    Int(i64),
    Str(&'a str),
}

impl From<i64> for SeedPart<'_> {
    fn from(v: i64) -> Self {
        Self::Int(v)
    }
}

impl From<i32> for SeedPart<'_> {
    fn from(v: i32) -> Self {
        Self::Int(i64::from(v))
    }
}

impl From<u32> for SeedPart<'_> {
    fn from(v: u32) -> Self {
        Self::Int(i64::from(v))
    }
}

impl From<usize> for SeedPart<'_> {
    fn from(v: usize) -> Self {
        Self::Int(i64::try_from(v).expect("seed path index fits in i64"))
    }
}

impl<'a> From<&'a str> for SeedPart<'a> {
    fn from(v: &'a str) -> Self {
        Self::Str(v)
    }
}

/// `derive_seed(master, *path)`: SHA-256 of `repr((master & MASK64, *path))`, first 8 bytes
/// big-endian.
#[must_use]
pub fn derive_seed(master: u64, path: &[SeedPart<'_>]) -> u64 {
    let mut parts: Vec<PyRepr<'_>> = Vec::with_capacity(path.len() + 1);
    parts.push(PyRepr::UInt(master));
    for p in path {
        parts.push(match *p {
            SeedPart::Int(v) => PyRepr::Int(v),
            SeedPart::Str(s) => PyRepr::Str(s),
        });
    }
    let material = repr_tuple(&parts);
    let digest = Sha256::digest(material.as_bytes());
    let mut first = [0u8; 8];
    first.copy_from_slice(&digest[..8]);
    u64::from_be_bytes(first)
}

/// `derive_seed!(master, "match", m, "game", g)` builds the path from mixed literals.
#[macro_export]
macro_rules! derive_seed {
    ($master:expr $(, $part:expr)* $(,)?) => {
        $crate::rng::derive_seed($master, &[$($crate::rng::SeedPart::from($part)),*])
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splitmix_first_outputs_match_reference_algorithm() {
        let mut r = SplitMix64::new(0);
        assert_eq!(r.next_u64(), 0xE220_A839_7B1D_CDAF);
        assert_eq!(r.next_u64(), 0x6E78_9E6A_A1B9_65F4);
    }

    #[test]
    fn randrange_stays_in_bounds() {
        let mut r = SplitMix64::new(42);
        for n in 1..200u64 {
            assert!(r.randrange(n) < n);
        }
    }

    #[test]
    fn shuffle_is_a_permutation() {
        let mut r = SplitMix64::new(7);
        let mut v: Vec<u32> = (0..50).collect();
        r.shuffle(&mut v);
        let mut sorted = v.clone();
        #[expect(
            clippy::disallowed_methods,
            reason = "distinct u32 keys: the sort order is unique"
        )]
        sorted.sort_unstable();
        assert_eq!(sorted, (0..50).collect::<Vec<_>>());
    }
}
