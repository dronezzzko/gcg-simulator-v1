#!/usr/bin/env bash
# The steps of .github/workflows/rust.yml, run locally in this working tree (docs/dev/PLAN.md
# §13 M17, A29; T-156), so the workflow is exercised before any push.
#
#   scripts/ci_local.sh
#
# The workflow's checkout, toolchain and uv installation steps are the machine's own here: the
# script prints the versions it finds. Unlike scripts/verify_all_rust.sh it does not clone HEAD,
# so uncommitted changes are included.
#
# GCG_GATE_WORKERS=N caps the corpus jobs, the cargo build jobs and the test threads on a
# machine shared with other work (default: every online CPU). No output depends on it.
#
# As in the workflow, the libm vectors decide whether this machine is the golden platform
# (docs/dev/EQUIVALENCE.md). If they fail, the float-dependent steps are skipped, the corpus
# sets that replay recorded actions without floats run instead, and the script exits 1.
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo"
command -v cargo >/dev/null || PATH="$HOME/.cargo/bin:$PATH"

workers="${GCG_GATE_WORKERS:-}"
if [[ -n "$workers" ]]; then
  if [[ ! "$workers" =~ ^[1-9][0-9]*$ ]]; then
    echo "GCG_GATE_WORKERS must be a positive integer, got '$workers'" >&2; exit 2; fi
  export CARGO_BUILD_JOBS="$workers" RUST_TEST_THREADS="$workers"
fi
jobs="${workers:-$(getconf _NPROCESSORS_ONLN)}"

step() { printf '\n== %s\n' "$*"; }

step "Machine and tree"
uname -a
sysctl -n machdep.cpu.brand_string
[[ "$(uname -m)" == arm64 ]] || { echo "ci_local: not an arm64 machine" >&2; exit 1; }
echo "HEAD $(git rev-parse --short HEAD)$( [[ -z "$(git status --porcelain)" ]] || echo ' (working tree has changes)')"

step "Rust toolchain"
rustc --version --verbose
cargo --version

step "uv sync --locked"
uv --version
uv sync --locked

step "cargo fetch --locked"
cargo fetch --locked

step "Golden-platform check (libm vectors exported from Python)"
if cargo test -p gcg-core --release --locked --offline --test vectors; then golden=1; else golden=0; fi

step "cargo fmt --check"
cargo fmt --all --check

step "cargo clippy -D warnings"
cargo clippy --workspace --all-targets --locked --offline -- -D warnings

step "cargo clippy -D warnings (slow, paranoid and testkit features)"
eval "$(cargo metadata --format-version 1 --no-deps --offline | python3 scripts/cargo_test_features.py)"
cargo clippy --workspace --all-targets --locked --offline --features "$SLOW,$DIAG" \
  --target-dir target/paranoid -- -D warnings

step "Build the release binaries"
cargo build --release --locked --offline -p gcg-cli -p gcg-eval
cargo build --release --locked --offline -p gcg-diff

if [[ $golden == 1 ]]; then
  step "cargo test --workspace --release"
  cargo test --workspace --release --locked --offline

  step "Differential corpus, tier 1"
  target/release/gcg-diff corpus --tier 1 --require-all --jobs "$jobs"
fi

step "gcg-diff lints"
target/release/gcg-diff lints

step "Traceability documents current"
target/release/gcg-diff traceability --check

step "tests.json register"
target/release/gcg-diff tests-json --check

if [[ $golden != 1 ]]; then
  step "Non-golden platform"
  target/release/gcg-diff corpus --tier 1 --require-all --jobs "$jobs" \
    --only full,vanilla,fuzz,coverage,tests,views,worlds
  cargo test -p gcg-search --release --locked --offline --test scenarios gapa2_portable_math
  echo "non-golden platform: libm differs (see docs/dev/EQUIVALENCE.md); search, components and baselines sets not checked" >&2
  exit 1
fi

echo
echo "ci_local passed"
