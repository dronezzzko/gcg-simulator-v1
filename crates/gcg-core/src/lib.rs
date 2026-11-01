//! Shared foundations of the Rust gcg-sim port: the seeded RNG chain and the
//! Python-compatible formatting that keeps outputs byte-identical to the reference.

pub mod libm;
pub mod pyfloat;
pub mod pyjson;
pub mod pymath;
pub mod pyrepr;
pub mod rng;

pub use rng::{SeedPart, SplitMix64, derive_seed};
