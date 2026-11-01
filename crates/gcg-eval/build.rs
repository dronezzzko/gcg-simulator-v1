//! Build script of `gcg-eval`: the code digest of `build-support/digest-manifest.txt`
//! (docs/port/PLAN.md §2.4, X19, W-103), exposed as `env!("GCG_CODE_DIGEST")`.

use std::path::PathBuf;

mod code_digest {
    include!("../../build-support/code_digest.rs");
}

fn main() {
    let manifest_dir = std::env::var_os("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("cargo sets CARGO_MANIFEST_DIR for build scripts"));
    code_digest::emit(&manifest_dir.join("..").join(".."));
}
