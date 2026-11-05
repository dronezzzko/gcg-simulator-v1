//! SHA-256-based 64-bit keys that never depend on `std::hash`, the process or the platform
//! (docs/dev/PLAN.md §2.1; transposition keys of the strengthened search, §12.3 S7).

use sha2::{Digest, Sha256};

/// A 64-bit key: the first 8 bytes of a SHA-256 digest, big-endian, as `derive_seed` reads them
/// (`crate::rng::derive_seed`).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct StableKey(pub u64);

/// Feeds bytes into one SHA-256; callers frame variable-length fields themselves (for example
/// with a length prefix) so that different field sequences cannot collide by concatenation.
#[derive(Clone, Debug, Default)]
pub struct StableHasher {
    sha: Sha256,
}

impl StableHasher {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn update(&mut self, bytes: &[u8]) {
        self.sha.update(bytes);
    }

    #[must_use]
    pub fn finish(self) -> StableKey {
        let digest = self.sha.finalize();
        let mut first = [0u8; 8];
        first.copy_from_slice(&digest[..8]);
        StableKey(u64::from_be_bytes(first))
    }
}

/// The key of one byte string.
#[must_use]
pub fn stable_key(bytes: &[u8]) -> StableKey {
    let mut h = StableHasher::new();
    h.update(bytes);
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_is_the_big_endian_sha256_prefix() {
        // sha256("") = e3b0c44298fc1c14 9afbf4c8996fb924...
        assert_eq!(stable_key(b""), StableKey(0xe3b0_c442_98fc_1c14));
        let mut h = StableHasher::new();
        h.update(b"ab");
        h.update(b"c");
        assert_eq!(h.finish(), stable_key(b"abc"));
    }
}
