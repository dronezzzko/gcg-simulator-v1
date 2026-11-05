//! Portable math for the strengthened search (W-137, docs/port/PLAN.md §2.1, X22).
//!
//! The reference search must reproduce CPython on the golden platform, so it calls the platform
//! libm through [`crate::libm`]. The strengthened search has no Python counterpart and instead
//! uses the pure-Rust `libm` crate, which gives the same bits on every platform.

pub use ::libm::{exp, log};
