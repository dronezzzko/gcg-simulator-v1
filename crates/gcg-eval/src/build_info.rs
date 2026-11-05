//! The version strings of this build (`results.json` `versions.package` and
//! `versions.code_digest`, both masked by the normaliser, PLAN A6).

/// The build-time `code_digest` (PLAN §2.4, X19, W-103): SHA-256 over the files of
/// `build-support/digest-manifest.txt`, computed by `build.rs`.
pub const CODE_DIGEST: &str = env!("GCG_CODE_DIGEST");

/// `versions.package`: the crate version.
pub const PACKAGE_VERSION: &str = env!("CARGO_PKG_VERSION");
