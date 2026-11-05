//! Shared foundations of gcg-sim: the seeded RNG chain and the
//! Python-compatible formatting that keeps outputs byte-identical to the reference.

pub mod libm;
pub mod lookup;
pub mod pool;
pub mod portable;
pub mod prop;
pub mod pyfloat;
pub mod pyjson;
pub mod pymath;
pub mod pyrepr;
pub mod rng;
pub mod small;
pub mod stable_hash;
pub mod sys;

pub use rng::{SeedPart, SplitMix64, derive_seed};
